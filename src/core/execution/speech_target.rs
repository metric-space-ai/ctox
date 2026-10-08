// Origin: CTOX
// License: AGPL-3.0-only
//! Current-grant speech receiver on the existing native control pool.
//! No HTTP, new peer, cloud fallback, or provider credential forwarding.
use super::target_policy::{IntentAdmission, TargetPolicy};
use super::*;
use ctox_sync::{
    authority::auth::speech_wire::{
        SpeechComputerReply as Reply, SpeechDenial as Denial, SpeechOperation as Op,
        SpeechWorkload, VerifiedSpeechRequest, METHOD,
    },
    native::NativePool,
};
use rxdb::plugins::replication_webrtc::{
    index_mod::{GuardedAuxiliaryRequestHandler, GuardedAuxiliaryResponse},
    RxWebRTCReplicationPool, WebRTCPublicationGuard, WebRTCRsConnection, WebRTCRsConnectionHandler,
};
use rxdb::rx_error::{new_rx_error, RxResult};
use std::{
    collections::HashMap,
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    task::{Context, Poll},
};
use tokio::{io::ReadBuf, sync::Mutex as AsyncMutex};

type Pool = RxWebRTCReplicationPool<WebRTCRsConnectionHandler>;
const MAX_OBJECTS: usize = 64;
struct OwnedTask(JoinHandle<Result<ReadyAudio, Denial>>);
impl Drop for OwnedTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
struct ReadyAudio {
    bytes: Vec<u8>,
    sha: String,
    duration_ms: u64,
}
struct Entry {
    peer: WebRTCRsConnection,
    policy: Arc<TargetPolicy>,
    deadline: Instant,
    work: AsyncMutex<Work>,
    lease: Mutex<Option<tokio::sync::OwnedSemaphorePermit>>,
}
impl Entry {
    fn release(&self) {
        self.lease.lock().unwrap_or_else(|p| p.into_inner()).take();
    }
}

enum Work {
    Opening,
    Stt {
        socket: BufReader<RuntimeSocket>,
        sequence: u64,
        bytes: u64,
        text: String,
        last: Option<(String, Reply)>,
    },
    Final {
        sequence: u64,
        digest: String,
        reply: Reply,
    },
    Synthesis(OwnedTask),
    Audio(ReadyAudio),
    Failed(Denial),
    Cancelled,
}
struct Server {
    root: PathBuf,
    generation: String,
    alive: AtomicBool,
    pool: Weak<Pool>,
    entries: Mutex<HashMap<String, Arc<Entry>>>,
    capacity: Arc<tokio::sync::Semaphore>,
}
pub(crate) struct TargetSpeechHost {
    server: Arc<Server>,
    reaper: JoinHandle<()>,
}
impl Drop for TargetSpeechHost {
    fn drop(&mut self) {
        self.server.alive.store(false, Ordering::Release);
        self.reaper.abort();
        self.server
            .entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }
}
impl TargetSpeechHost {
    pub(crate) fn start(root: &Path, pool: NativePool) -> anyhow::Result<Self> {
        let server = Arc::new(Server {
            root: root.into(),
            generation: Uuid::new_v4().to_string(),
            alive: AtomicBool::new(true),
            pool: Arc::downgrade(&pool),
            entries: Mutex::new(HashMap::new()),
            capacity: Arc::new(tokio::sync::Semaphore::new(2)),
        });
        let weak = Arc::downgrade(&server);
        let handler: GuardedAuxiliaryRequestHandler<WebRTCRsConnection> =
            Arc::new(move |peer, _, params| {
                let server = weak.upgrade();
                Box::pin(async move {
                    let server = server.ok_or_else(|| "speech_host_retired".to_string())?;
                    server
                        .answer(peer, params)
                        .await
                        .map_err(|_| "speech_request_rejected".to_string())
                })
            });
        crate::sync_host::native_control_channel(root)?.register_handler(METHOD, handler)?;
        let weak = Arc::downgrade(&server);
        let reaper = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let Some(server) = weak.upgrade() else {
                    break;
                };
                if !server.alive.load(Ordering::Acquire) {
                    break;
                }
                if let Ok(mut entries) = server.entries.lock() {
                    let pool = server.pool.upgrade();
                    entries.retain(|_, entry| {
                        entry.deadline > Instant::now()
                            && pool
                                .as_ref()
                                .is_some_and(|pool| pool.is_peer_ready_for_control(&entry.peer))
                    });
                };
            }
        });
        Ok(Self { server, reaper })
    }
}

struct Publication {
    server: Weak<Server>,
    policy: Arc<TargetPolicy>,
    peer: WebRTCRsConnection,
    object_id: Option<String>,
}
impl WebRTCPublicationGuard for Publication {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        self.policy
            .with_current(|_| {
                let server = self.server.upgrade().ok_or(Denial::RouteRetired)?;
                if !server.alive.load(Ordering::Acquire) {
                    return Err(Denial::RouteRetired);
                }
                let entries = server.entries.lock().map_err(|_| Denial::RouteRetired)?;
                if let Some(id) = &self.object_id {
                    let entry = entries.get(id).ok_or(Denial::RouteRetired)?;
                    if entry.peer != self.peer || entry.deadline <= Instant::now() {
                        return Err(Denial::RouteRetired);
                    }
                }
                let pool = server.pool.upgrade().ok_or(Denial::RouteRetired)?;
                pool.with_current_native_control_peer(&self.peer, publish)
                    .map_err(|_| Denial::RouteRetired)?
                    .map_err(|_| Denial::RouteRetired)
            })
            .map_err(|_| new_rx_error("CTOX_SPEECH_AUTHORITY_RETIRED", None))
    }
}
/// Reacquire current policy, live host/object and exact native connection at
/// EVERY private IPC write/read poll. Pending holds no authority/store locks.
struct GuardedIo {
    inner: tokio::net::UnixStream,
    guard: Arc<dyn WebRTCPublicationGuard>,
}
fn guarded_poll<T>(
    guard: &dyn WebRTCPublicationGuard,
    apply: impl FnOnce() -> Poll<io::Result<T>>,
) -> Poll<io::Result<T>> {
    let mut apply = Some(apply);
    let mut result = None;
    let authority = guard.with_current(&mut || {
        let apply = apply
            .take()
            .ok_or_else(|| new_rx_error("CTOX_SPEECH_MULTIPLE_POLLS", None))?;
        result = Some(apply());
        Ok(())
    });
    if authority.is_err() {
        return Poll::Ready(Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "speech authority retired",
        )));
    }
    result.unwrap_or_else(|| Poll::Ready(Err(io::Error::other("speech guard omitted poll"))))
}
impl AsyncRead for GuardedIo {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let guard = this.guard.clone();
        guarded_poll(guard.as_ref(), || {
            Pin::new(&mut this.inner).poll_read(cx, buf)
        })
    }
}
impl AsyncWrite for GuardedIo {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let guard = this.guard.clone();
        guarded_poll(guard.as_ref(), || {
            Pin::new(&mut this.inner).poll_write(cx, buf)
        })
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let guard = this.guard.clone();
        guarded_poll(guard.as_ref(), || Pin::new(&mut this.inner).poll_flush(cx))
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let guard = this.guard.clone();
        guarded_poll(guard.as_ref(), || {
            Pin::new(&mut this.inner).poll_shutdown(cx)
        })
    }
}
impl Server {
    fn publication(
        self: &Arc<Self>,
        policy: Arc<TargetPolicy>,
        peer: WebRTCRsConnection,
        id: Option<String>,
    ) -> Arc<dyn WebRTCPublicationGuard> {
        Arc::new(Publication {
            server: Arc::downgrade(self),
            policy,
            peer,
            object_id: id,
        })
    }
    fn entry(
        &self,
        id: &str,
        peer: &WebRTCRsConnection,
        request: &VerifiedSpeechRequest,
    ) -> Result<Arc<Entry>, Denial> {
        let entries = self.entries.lock().map_err(|_| Denial::RouteRetired)?;
        let entry = entries.get(id).cloned().ok_or(Denial::RouteRetired)?;
        drop(entries);
        if entry.peer != *peer
            || entry.deadline <= Instant::now()
            || entry.policy.binding() != &request.request().binding
            || entry.policy.source_identity() != request.sender()
        {
            return Err(Denial::GrantDenied);
        }
        entry.policy.with_current(|_| Ok(()))?;
        Ok(entry)
    }
    async fn answer(
        self: Arc<Self>,
        peer: WebRTCRsConnection,
        params: Vec<Value>,
    ) -> Result<GuardedAuxiliaryResponse, Denial> {
        if params.len() != 1 || !self.alive.load(Ordering::Acquire) {
            return Err(Denial::RouteRetired);
        }
        let root = self.root.clone();
        let envelope = params.into_iter().next().unwrap();
        let (policy, request) =
            tokio::task::spawn_blocking(move || TargetPolicy::verify(&root, envelope))
                .await
                .map_err(|_| Denial::RouteRetired)??;
        let (answer, id) = match self.execute(&peer, &policy, &request).await {
            Ok(value) => value,
            Err(reason) => (Reply::Denied { reason }, None),
        };
        let current = policy.clone();
        let signed = request.clone();
        let result = tokio::task::spawn_blocking(move || current.reply(&signed, answer))
            .await
            .map_err(|_| Denial::RouteRetired)??;
        Ok(GuardedAuxiliaryResponse {
            result,
            publication: self.publication(policy, peer, id),
        })
    }
    async fn execute(
        self: &Arc<Self>,
        peer: &WebRTCRsConnection,
        policy: &Arc<TargetPolicy>,
        request: &Arc<VerifiedSpeechRequest>,
    ) -> Result<(Reply, Option<String>), Denial> {
        use crate::inference::native_stt::{LocalSttRequest as S, LocalSttResponse as R};
        let binding = &request.request().binding;
        match &request.request().operation {
            Op::OpenTranscription { .. } | Op::StartSynthesis { .. } => {
                let admitted = policy.reserve_intent(request, &self.generation)?;
                let id = match admitted {
                    IntentAdmission::Existing(id) => {
                        let entry = self.entry(&id, peer, request)?;
                        let work = entry.work.lock().await;
                        let reply = match (&request.request().operation, &*work) {
                            (Op::OpenTranscription { .. }, Work::Stt { .. }) => {
                                Reply::TranscriptionOpened {
                                    stream_id: id.clone(),
                                    model: binding.model.clone(),
                                }
                            }
                            (Op::StartSynthesis { .. }, Work::Synthesis(_) | Work::Audio(_)) => {
                                Reply::SynthesisStarted {
                                    run_id: id.clone(),
                                    model: binding.model.clone(),
                                }
                            }
                            _ => return Err(Denial::RouteRetired),
                        };
                        return Ok((reply, Some(id)));
                    }
                    IntentAdmission::New(id) => id,
                };
                let lease = self
                    .capacity
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| Denial::Busy)?;
                let entry = Arc::new(Entry {
                    peer: peer.clone(),
                    policy: policy.clone(),
                    deadline: Instant::now()
                        + Duration::from_secs(
                            if binding.workload == SpeechWorkload::Transcription {
                                90
                            } else {
                                150
                            },
                        ),
                    work: AsyncMutex::new(Work::Opening),
                    lease: Mutex::new(Some(lease)),
                });
                policy.with_current(|_| {
                    if !self.alive.load(Ordering::Acquire) {
                        return Err(Denial::RouteRetired);
                    }
                    let mut entries = self.entries.lock().map_err(|_| Denial::RouteRetired)?;
                    if entries.len() >= MAX_OBJECTS {
                        return Err(Denial::Busy);
                    }
                    entries.insert(id.clone(), entry.clone());
                    Ok(())
                })?;
                let guard = self.publication(policy.clone(), peer.clone(), Some(id.clone()));
                let socket = connect_runtime(&self.root, binding, guard.clone()).await;
                let socket = match socket {
                    Ok(socket) => socket,
                    Err(reason) => {
                        *entry.work.lock().await = Work::Failed(reason);
                        entry.release();
                        return Err(reason);
                    }
                };
                match &request.request().operation {
                    Op::OpenTranscription { sample_rate_hz, .. } => {
                        let mut socket = BufReader::new(socket);
                        let ready = runtime_roundtrip(
                            &mut socket,
                            &S::TranscriptionOpen {
                                model: Some(binding.model.clone()),
                                sample_rate_hz: *sample_rate_hz,
                            },
                        )
                        .await;
                        let failure = match ready {
                            Ok(R::StreamReady { model }) if model == binding.model => None,
                            Ok(_) => Some(Denial::UnsupportedModel),
                            Err(_) => Some(Denial::InferenceFailed),
                        };
                        if let Some(reason) = failure {
                            *entry.work.lock().await = Work::Failed(reason);
                            entry.release();
                            return Err(reason);
                        }
                        *entry.work.lock().await = Work::Stt {
                            socket,
                            sequence: 0,
                            bytes: 0,
                            text: String::new(),
                            last: None,
                        };
                        Ok((
                            Reply::TranscriptionOpened {
                                stream_id: id.clone(),
                                model: binding.model.clone(),
                            },
                            Some(id),
                        ))
                    }
                    Op::StartSynthesis { text, voice_id, .. } => {
                        let model = binding.model.clone();
                        let text = text.clone();
                        let voice = voice_id.clone();
                        let task = tokio::spawn(async move {
                            tokio::time::timeout(
                                Duration::from_secs(120),
                                synthesize_local(socket, model, text, voice),
                            )
                            .await
                            .unwrap_or(Err(Denial::TimedOut))
                        });
                        *entry.work.lock().await = Work::Synthesis(OwnedTask(task));
                        Ok((
                            Reply::SynthesisStarted {
                                run_id: id.clone(),
                                model: binding.model.clone(),
                            },
                            Some(id),
                        ))
                    }
                    _ => unreachable!(),
                }
            }
            Op::Append {
                stream_id,
                sequence,
                ..
            }
            | Op::Finish {
                stream_id,
                sequence,
            } => {
                let entry = self.entry(stream_id, peer, request)?;
                let hash = format!(
                    "{:x}",
                    Sha256::digest(
                        serde_json::to_vec(&request.request().operation)
                            .map_err(|_| Denial::InvalidSequence)?
                    )
                );
                let mut work = entry.work.lock().await;
                if let Work::Final {
                    sequence: previous,
                    digest,
                    reply,
                } = &*work
                {
                    if previous == sequence && digest == &hash {
                        return Ok((reply.clone(), Some(stream_id.clone())));
                    }
                    return Err(Denial::InvalidSequence);
                }
                let Work::Stt {
                    socket,
                    sequence: previous,
                    bytes,
                    text,
                    last,
                } = &mut *work
                else {
                    return Err(work_denial(&work));
                };
                if *previous == *sequence {
                    if let Some((digest, reply)) = last {
                        if digest == &hash {
                            return Ok((reply.clone(), Some(stream_id.clone())));
                        }
                    }
                    return Err(Denial::InvalidSequence);
                }
                if previous.checked_add(1) != Some(*sequence) {
                    return Err(Denial::InvalidSequence);
                }
                let (command, finish, new_bytes) = match &request.request().operation {
                    Op::Append { pcm16_hex, .. } => {
                        let pcm = decode_pcm(pcm16_hex)?;
                        let count = bytes
                            .checked_add(pcm.len() as u64)
                            .filter(|n| *n <= 480_000)
                            .ok_or(Denial::Backpressure)?;
                        (
                            S::TranscriptionAppend {
                                pcm_base64: BASE64.encode(pcm),
                            },
                            false,
                            count,
                        )
                    }
                    Op::Finish { .. } => (S::TranscriptionFinish, true, *bytes),
                    _ => unreachable!(),
                };
                let response = runtime_roundtrip(socket, &command)
                    .await
                    .map_err(|_| Denial::InferenceFailed)?;
                let (actual_text, duration) = match response {
                    R::StreamUpdate {
                        sequence: actual,
                        text: partial,
                        audio_duration_ms,
                    } if actual == *sequence && !finish => {
                        (partial.unwrap_or_else(|| text.clone()), audio_duration_ms)
                    }
                    R::TranscriptionFinal {
                        sequence: actual,
                        text,
                        model,
                        audio_duration_ms,
                    } if actual == *sequence && finish && model == binding.model => {
                        (text, audio_duration_ms)
                    }
                    _ => return Err(Denial::InvalidSequence),
                };
                if actual_text.len() > 8192 || duration > 15_000 {
                    return Err(Denial::Backpressure);
                }
                let reply = Reply::Transcript {
                    stream_id: stream_id.clone(),
                    sequence: *sequence,
                    text: actual_text.clone(),
                    model: binding.model.clone(),
                    audio_duration_ms: duration,
                    is_final: finish,
                };
                *previous = *sequence;
                *bytes = new_bytes;
                *text = actual_text;
                *last = Some((hash.clone(), reply.clone()));
                if finish {
                    entry.release();
                    *work = Work::Final {
                        sequence: *sequence,
                        digest: hash,
                        reply: reply.clone(),
                    };
                }
                Ok((reply, Some(stream_id.clone())))
            }
            Op::CancelTranscription { stream_id } | Op::CancelSynthesis { run_id: stream_id } => {
                let entry = self.entry(stream_id, peer, request)?;
                *entry.work.lock().await = Work::Cancelled;
                entry.release();
                let reply = if binding.workload == SpeechWorkload::Transcription {
                    Reply::TranscriptionCancelled {
                        stream_id: stream_id.clone(),
                    }
                } else {
                    Reply::SynthesisCancelled {
                        run_id: stream_id.clone(),
                    }
                };
                Ok((reply, Some(stream_id.clone())))
            }
            Op::SynthesisStatus { run_id } => {
                let entry = self.entry(run_id, peer, request)?;
                let mut work = entry.work.lock().await;
                if let Work::Synthesis(task) = &mut *work {
                    if !task.0.is_finished() {
                        return Ok((
                            Reply::SynthesisPending {
                                run_id: run_id.clone(),
                            },
                            Some(run_id.clone()),
                        ));
                    }
                    let result = (&mut task.0).await.unwrap_or(Err(Denial::InferenceFailed));
                    *work = match result {
                        Ok(audio) => Work::Audio(audio),
                        Err(reason) => {
                            entry.release();
                            Work::Failed(reason)
                        }
                    };
                }
                let Work::Audio(audio) = &*work else {
                    return Err(work_denial(&work));
                };
                Ok((
                    Reply::SynthesisReady {
                        run_id: run_id.clone(),
                        model: binding.model.clone(),
                        audio_sha256: audio.sha.clone(),
                        audio_bytes: audio.bytes.len() as u64,
                        sample_rate_hz: 24_000,
                        audio_duration_ms: audio.duration_ms,
                    },
                    Some(run_id.clone()),
                ))
            }
            Op::ReadSynthesis {
                run_id,
                offset,
                max_bytes,
            } => {
                let entry = self.entry(run_id, peer, request)?;
                let work = entry.work.lock().await;
                let Work::Audio(audio) = &*work else {
                    return Err(work_denial(&work));
                };
                let from = *offset as usize;
                if from > audio.bytes.len() {
                    return Err(Denial::InvalidSequence);
                }
                let to = from
                    .saturating_add(*max_bytes as usize)
                    .min(audio.bytes.len());
                Ok((
                    Reply::SynthesisChunk {
                        run_id: run_id.clone(),
                        offset: *offset,
                        total_bytes: audio.bytes.len() as u64,
                        hex: encode_hex(&audio.bytes[from..to]),
                    },
                    Some(run_id.clone()),
                ))
            }
        }
    }
}
fn work_denial(work: &Work) -> Denial {
    match work {
        Work::Failed(reason) => *reason,
        Work::Opening => Denial::Busy,
        _ => Denial::RouteRetired,
    }
}
fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 15) as usize] as char);
    }
    out
}
fn decode_pcm(hex: &str) -> Result<Vec<u8>, Denial> {
    if hex.is_empty() || hex.len() > 6400 || hex.len() % 4 != 0 {
        return Err(Denial::InvalidSequence);
    }
    hex.as_bytes()
        .chunks_exact(2)
        .map(|p| {
            fn digit(b: u8) -> Result<u8, Denial> {
                match b {
                    b'0'..=b'9' => Ok(b - b'0'),
                    b'a'..=b'f' => Ok(b - b'a' + 10),
                    _ => Err(Denial::InvalidSequence),
                }
            }
            Ok(digit(p[0])? * 16 + digit(p[1])?)
        })
        .collect()
}
async fn connect_runtime(
    root: &Path,
    binding: &ctox_sync::authority::auth::speech_wire::SpeechComputerBinding,
    guard: Arc<dyn WebRTCPublicationGuard>,
) -> Result<RuntimeSocket, Denial> {
    use crate::inference::{
        engine::AuxiliaryRole, local_transport::LocalTransport,
        runtime_kernel::InferenceRuntimeKernel,
    };
    let root = root.to_path_buf();
    let model = binding.model.clone();
    let role = binding.workload;
    let transport = tokio::task::spawn_blocking(move || {
        let config = SpeechRuntimeConfig::load(&root).map_err(|_| Denial::UnsupportedModel)?;
        if (role == SpeechWorkload::Transcription && config.transcription != SpeechBackend::Runtime)
            || (role == SpeechWorkload::Synthesis && config.synthesis != SpeechBackend::Runtime)
        {
            return Err(Denial::UnsupportedModel);
        }
        let kernel =
            InferenceRuntimeKernel::resolve(&root).map_err(|_| Denial::UnsupportedModel)?;
        let binding = kernel
            .binding_for_auxiliary_role(if role == SpeechWorkload::Transcription {
                AuxiliaryRole::Stt
            } else {
                AuxiliaryRole::Tts
            })
            .filter(|b| b.request_model == model)
            .ok_or(Denial::UnsupportedModel)?;
        Ok(binding.transport.clone())
    })
    .await
    .map_err(|_| Denial::RouteRetired)??;
    let LocalTransport::UnixSocket { path } = transport else {
        return Err(Denial::UnsupportedModel);
    };
    guard
        .with_current(&mut || Ok(()))
        .map_err(|_| Denial::RouteRetired)?;
    let inner = tokio::time::timeout(IO_TIMEOUT, tokio::net::UnixStream::connect(path))
        .await
        .map_err(|_| Denial::TimedOut)?
        .map_err(|_| Denial::InferenceFailed)?;
    Ok(Box::new(GuardedIo { inner, guard }))
}
async fn synthesize_local(
    socket: RuntimeSocket,
    model: String,
    text: String,
    voice: String,
) -> Result<ReadyAudio, Denial> {
    use crate::inference::native_tts::{LocalTtsRequest, LocalTtsResponse};
    let mut socket = BufReader::new(socket);
    let mut payload = serde_json::to_vec(&LocalTtsRequest::SpeechCreate {
        model: Some(model.clone()),
        input: text,
        voice: Some(voice),
        response_format: Some("wav".into()),
    })
    .map_err(|_| Denial::InferenceFailed)?;
    payload.push(b'\n');
    socket
        .get_mut()
        .write_all(&payload)
        .await
        .map_err(|_| Denial::RouteRetired)?;
    socket
        .get_mut()
        .flush()
        .await
        .map_err(|_| Denial::RouteRetired)?;
    let mut bytes = Vec::new();
    loop {
        let buffer = socket.fill_buf().await.map_err(|_| Denial::RouteRetired)?;
        if buffer.is_empty() {
            return Err(Denial::InferenceFailed);
        }
        let n = buffer
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| n + 1)
            .unwrap_or(buffer.len());
        let done = buffer[n - 1] == b'\n';
        if bytes.len() + n > MAX_AUDIO_BYTES * 2 {
            return Err(Denial::Backpressure);
        }
        bytes.extend_from_slice(&buffer[..n]);
        socket.consume(n);
        if done {
            break;
        }
    }
    let response: LocalTtsResponse =
        serde_json::from_slice(&bytes).map_err(|_| Denial::InferenceFailed)?;
    let LocalTtsResponse::Speech {
        model: actual,
        audio_base64,
        response_format,
    } = response
    else {
        return Err(Denial::InferenceFailed);
    };
    if actual != model || response_format != "wav" {
        return Err(Denial::UnsupportedModel);
    }
    let audio = BASE64
        .decode(audio_base64)
        .map_err(|_| Denial::InferenceFailed)?;
    if audio.len() > MAX_AUDIO_BYTES || audio.len() < 44 {
        return Err(Denial::Backpressure);
    }
    // The curated native TTS writer emits the canonical44-byte PCM header.
    let data = u32::from_le_bytes(audio[40..44].try_into().unwrap()) as u64;
    let duration_ms = data / 2 * 1000 / 24_000;
    if duration_ms == 0 || duration_ms > 40_960 {
        return Err(Denial::InferenceFailed);
    }
    let sha = format!("{:x}", Sha256::digest(&audio));
    super::computer::verify_audio(&audio, &sha, duration_ms)
        .map_err(|_| Denial::InferenceFailed)?;
    Ok(ReadyAudio {
        bytes: audio,
        sha,
        duration_ms,
    })
}

#[cfg(test)]
#[path = "speech_target_tests.rs"]
mod tests;
