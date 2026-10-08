// Origin: CTOX
// License: AGPL-3.0-only
//! Speech-only source routes on the existing native control host.
//! Local operator configuration is not a browser permission. Meeting callers
//! still bind/revalidate their current Owner capability before and after awaits.
use super::*;
use ctox_sync::authority::auth::{
    speech_wire::{
        SignedSpeechRequest, SpeechComputerBinding, SpeechComputerReply as Reply,
        SpeechComputerRequest, SpeechDenial, SpeechOperation as Op, SpeechWorkload, MAX_READ_BYTES,
        METHOD,
    },
    validate_public_identity,
};
use rxdb::{
    plugins::replication_webrtc::WebRTCPublicationGuard,
    rx_error::{new_rx_error, RxResult},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

const COMPUTER_CONFIG_KEY: &str = "speech_computer_routes";
const MAX_CONFIG_BYTES: u64 = 16 * 1024;

/// No provider keys or network endpoints. The route selects an already accepted
/// native peer; the independent signing pins authenticate each request/reply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpeechComputerRoute {
    pub scope_id: String,
    #[serde(default)]
    pub native_peer_route: String,
    pub source_signing_identity: String,
    pub target_signing_identity: String,
    pub binding: SpeechComputerBinding,
    pub expires_at_unix_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpeechComputerConfig {
    pub transcription: Option<SpeechComputerRoute>,
    pub synthesis: Option<SpeechComputerRoute>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Saved {
    /// A new local authority epoch on every operator save, including revoke.
    epoch: String,
    config: SpeechComputerConfig,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn validate_route(
    route: &SpeechComputerRoute,
    role: SpeechWorkload,
    now: u64,
) -> anyhow::Result<()> {
    validate_public_identity(&route.source_signing_identity)?;
    validate_public_identity(&route.target_signing_identity)?;
    anyhow::ensure!(
        route.source_signing_identity != route.target_signing_identity,
        "speech computer must be a distinct pinned peer"
    );
    anyhow::ensure!(
        !route.scope_id.is_empty()
            && route.scope_id.len() <= 128
            && route
                .scope_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')),
        "invalid native speech scope"
    );
    anyhow::ensure!(
        route.native_peer_route.len() <= 256
            && route.native_peer_route.trim() == route.native_peer_route
            && !route.native_peer_route.chars().any(char::is_control),
        "invalid native speech peer"
    );
    anyhow::ensure!(
        route.binding.workload == role
            && route.expires_at_unix_ms > now
            && route.expires_at_unix_ms.saturating_sub(now) <= 86_400_000,
        "speech route must match workload and expire within one day"
    );
    let operation = match role {
        SpeechWorkload::Transcription => Op::OpenTranscription {
            intent_id: "validate".into(),
            sample_rate_hz: 16_000,
        },
        SpeechWorkload::Synthesis => Op::StartSynthesis {
            intent_id: "validate".into(),
            text: "validate".into(),
            voice_id: "validate".into(),
        },
    };
    SpeechComputerRequest {
        binding: route.binding.clone(),
        operation,
    }
    .validate()?;
    Ok(())
}

impl SpeechComputerConfig {
    /// Local operator only, same encrypted issuer fence as native Sync policy.
    /// Never exposed as an unauthenticated remote configuration operation.
    pub fn save(&self, root: &Path) -> anyhow::Result<()> {
        let now = now_ms();
        for (route, role) in [
            (&self.transcription, SpeechWorkload::Transcription),
            (&self.synthesis, SpeechWorkload::Synthesis),
        ] {
            if let Some(route) = route {
                validate_route(route, role, now)?;
            }
        }
        let host = crate::sync_host::handoff_configuration(root)?;
        crate::sync_host::with_current_signing_identity(root, |identity| {
            host.validate_key(identity)?;
            for route in [&self.transcription, &self.synthesis].into_iter().flatten() {
                anyhow::ensure!(
                    route.source_signing_identity == identity.public_identity()
                        && route.scope_id == host.scope_id,
                    "speech source identity or scope differs from native host"
                );
            }
            crate::persistence::store_json_payload(
                root,
                COMPUTER_CONFIG_KEY,
                Some(&Saved {
                    epoch: Uuid::new_v4().to_string(),
                    config: self.clone(),
                }),
            )
        })
    }

    pub fn load(root: &Path) -> anyhow::Result<Self> {
        Ok(load_saved(root)?.config)
    }
}

fn load_saved(root: &Path) -> anyhow::Result<Saved> {
    crate::persistence::load_json_payload(root, COMPUTER_CONFIG_KEY)?
        .ok_or_else(|| anyhow::anyhow!("speech computer route is not configured"))
}

pub fn configure_from_file(root: &Path, path: &Path) -> anyhow::Result<SpeechComputerConfig> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_CONFIG_BYTES,
        "speech computer configuration too large"
    );
    let config: SpeechComputerConfig = serde_json::from_slice(&bytes)?;
    config.save(root)?;
    Ok(config)
}

struct Current {
    root: PathBuf,
    saved: Saved,
    route: SpeechComputerRoute,
}
impl Current {
    fn load(root: &Path, role: SpeechWorkload) -> Result<Arc<Self>, SpeechError> {
        let saved = load_saved(root).map_err(|_| SpeechError::ConfigurationUnavailable)?;
        let route = match role {
            SpeechWorkload::Transcription => &saved.config.transcription,
            SpeechWorkload::Synthesis => &saved.config.synthesis,
        }
        .clone()
        .ok_or(SpeechError::ConfigurationUnavailable)?;
        validate_route(&route, role, now_ms())
            .map_err(|_| SpeechError::ConfigurationUnavailable)?;
        let current = Arc::new(Self {
            root: root.into(),
            saved,
            route,
        });
        current
            .with_current(|_| Ok(()))
            .map_err(|_| SpeechError::ConfigurationUnavailable)?;
        Ok(current)
    }

    fn with_current<T>(
        &self,
        apply: impl FnOnce(&ctox_sync::authority::auth::SigningIdentity) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        crate::sync_host::with_current_signing_identity(&self.root, |identity| {
            anyhow::ensure!(
                identity.public_identity() == self.route.source_signing_identity,
                "speech source issuer retired"
            );
            anyhow::ensure!(
                self.route.expires_at_unix_ms > now_ms(),
                "speech route expired"
            );
            anyhow::ensure!(
                load_saved(&self.root)? == self.saved,
                "speech route retired"
            );
            apply(identity)
        })
    }
}
impl WebRTCPublicationGuard for Current {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        self.with_current(|_| publish().map_err(|_| anyhow::anyhow!("speech publication rejected")))
            .map_err(|_| new_rx_error("CTOX_SPEECH_ROUTE_RETIRED", None))
    }
}

struct VerifiedReply {
    pin: String,
    signed: SignedSpeechRequest,
}
impl crate::sync_host::NativeControlReplyVerifier for VerifiedReply {
    fn verify(&self, pin: &str, reply: &Value) -> std::io::Result<Value> {
        if pin != self.pin {
            return Err(std::io::Error::other("speech target pin changed"));
        }
        serde_json::to_value(self.signed.verify_reply(reply.clone())?)
            .map_err(std::io::Error::other)
    }
}

#[derive(Clone)]
struct Client {
    current: Arc<Current>,
    channel: crate::sync_host::NativeControlChannel,
    peer: crate::sync_host::NativeControlPeer,
}
impl Client {
    fn open(root: &Path, role: SpeechWorkload) -> Result<Self, SpeechError> {
        let current = Current::load(root, role)?;
        let channel =
            crate::sync_host::native_control_channel(root).map_err(|_| SpeechError::Transport)?;
        let peer = channel
            .bind_identity(&current.route.target_signing_identity)
            .map_err(|_| SpeechError::Transport)?
            .ok_or(SpeechError::Transport)?;
        if !current.route.native_peer_route.is_empty()
            && peer.route() != current.route.native_peer_route
        {
            return Err(SpeechError::Transport);
        }
        Ok(Self {
            current,
            channel,
            peer,
        })
    }

    async fn call(&self, operation: Op) -> Result<Reply, SpeechError> {
        let current = self.current.clone();
        let request = SpeechComputerRequest {
            binding: current.route.binding.clone(),
            operation,
        };
        let signing_current = current.clone();
        let signed = tokio::task::spawn_blocking(move || {
            signing_current.with_current(|identity| {
                Ok(SignedSpeechRequest::new(
                    identity,
                    &signing_current.route.target_signing_identity,
                    &signing_current.route.scope_id,
                    request,
                )?)
            })
        })
        .await
        .map_err(|_| SpeechError::Transport)?
        .map_err(|_| SpeechError::ConfigurationUnavailable)?;
        let envelope = signed.envelope.clone();
        let verifier = Arc::new(VerifiedReply {
            pin: current.route.target_signing_identity.clone(),
            signed,
        });
        let result = self
            .channel
            .request_on(
                &self.peer,
                &current.route.target_signing_identity,
                METHOD,
                envelope,
                Duration::from_secs(30),
                current.clone(),
                verifier,
            )
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::TimedOut {
                    SpeechError::TimedOut
                } else if e.kind() == std::io::ErrorKind::WouldBlock {
                    SpeechError::Backpressure
                } else if e.kind() == std::io::ErrorKind::InvalidData {
                    SpeechError::InvalidResponse
                } else {
                    SpeechError::Transport
                }
            })?;
        let reply: Reply =
            serde_json::from_value(result).map_err(|_| SpeechError::InvalidResponse)?;
        match reply {
            Reply::Denied { reason } => Err(match reason {
                SpeechDenial::GrantDenied
                | SpeechDenial::GrantExpired
                | SpeechDenial::RouteRetired => SpeechError::ConfigurationUnavailable,
                SpeechDenial::Busy | SpeechDenial::Backpressure => SpeechError::Backpressure,
                SpeechDenial::UnsupportedModel => SpeechError::UnsupportedBackend,
                SpeechDenial::InvalidSequence => SpeechError::InvalidResponse,
                SpeechDenial::TimedOut => SpeechError::TimedOut,
                SpeechDenial::InferenceFailed => SpeechError::Transport,
            }),
            reply => Ok(reply),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SpeechRouteDiagnostic {
    workload: SpeechWorkload,
    elapsed_ms: u64,
    ready: bool,
    transport: crate::sync_host::NativeControlDiagnostic,
}
impl SpeechRouteDiagnostic {
    pub fn ready(&self) -> bool {
        self.ready
    }
}

/// Local operator snapshot after a bounded wait; no speech operation is sent.
pub async fn diagnose_route(
    root: &Path,
    role: SpeechWorkload,
) -> Result<SpeechRouteDiagnostic, SpeechError> {
    let started = Instant::now();
    let current = Current::load(root, role)?;
    let channel =
        crate::sync_host::native_control_channel(root).map_err(|_| SpeechError::Transport)?;
    loop {
        current
            .with_current(|_| Ok(()))
            .map_err(|_| SpeechError::ConfigurationUnavailable)?;
        let transport = channel
            .diagnostic(&current.route.target_signing_identity)
            .map_err(|_| SpeechError::Transport)?;
        let ready = transport.verified_target_route
            && channel
                .bind_identity(&current.route.target_signing_identity)
                .map_err(|_| SpeechError::Transport)?
                .is_some_and(|peer| {
                    current.route.native_peer_route.is_empty()
                        || peer.route() == current.route.native_peer_route
                });
        if ready || started.elapsed() >= Duration::from_secs(15) {
            return Ok(SpeechRouteDiagnostic {
                workload: role,
                elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                ready,
                transport,
            });
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Operator diagnostics await their configured host's existing peer before
/// timing audio. Never retry a request whose effect may already have started.
pub async fn wait_for_route(root: &Path, role: SpeechWorkload) -> Result<(), SpeechError> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match Client::open(root, role) {
            Ok(_) => return Ok(()),
            Err(SpeechError::Transport) => {}
            Err(error) => return Err(error),
        }
        if Instant::now() >= deadline {
            return Err(SpeechError::TimedOut);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Dropping/aborting the stream cancels its exact remote operation using the
/// same current grant. Cleanup is bounded, cannot reconnect or revive authority.
struct CancelOnDrop {
    client: Client,
    operation: Op,
    completed: Arc<AtomicBool>,
}
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if self.completed.load(Ordering::Acquire) {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let client = self.client.clone();
            let operation = self.operation.clone();
            runtime.spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(3), client.call(operation)).await;
            });
        }
    }
}

pub(super) async fn open_transcription(
    root: &Path,
    format: PcmFormat,
) -> Result<TranscriptionStream, SpeechError> {
    if format.sample_rate_hz != 16_000 {
        return Err(SpeechError::InvalidRequest);
    }
    let root = root.to_path_buf();
    let client =
        tokio::task::spawn_blocking(move || Client::open(&root, SpeechWorkload::Transcription))
            .await
            .map_err(|_| SpeechError::Transport)??;
    let reply = client
        .call(Op::OpenTranscription {
            intent_id: Uuid::new_v4().to_string(),
            sample_rate_hz: 16_000,
        })
        .await?;
    let Reply::TranscriptionOpened { stream_id, model } = reply else {
        return Err(SpeechError::InvalidResponse);
    };
    let completed = Arc::new(AtomicBool::new(false));
    let cancel = CancelOnDrop {
        client: client.clone(),
        operation: Op::CancelTranscription {
            stream_id: stream_id.clone(),
        },
        completed,
    };
    let (input, rx) = mpsc::channel(8);
    let (tx, events) = mpsc::channel(32);
    let (done_tx, terminal) = oneshot::channel();
    let task = tokio::spawn(async move {
        let result = tokio::time::timeout(
            SESSION_TIMEOUT,
            pump(client, stream_id, model, rx, &tx, cancel),
        )
        .await
        .unwrap_or(Err(SpeechError::TimedOut));
        let _ = done_tx.send(result);
    });
    Ok(TranscriptionStream {
        stream_id: Uuid::new_v4().to_string(),
        input,
        events,
        terminal: Some(terminal),
        task,
        format,
        finished: false,
    })
}

async fn pump(
    client: Client,
    stream_id: String,
    model: String,
    mut input: mpsc::Receiver<Input>,
    events: &mpsc::Sender<Result<TranscriptEvent, SpeechError>>,
    cancel: CancelOnDrop,
) -> Result<(), SpeechError> {
    let started = Instant::now();
    let mut sequence = 0;
    let mut event_sequence = 0;
    let mut previous = String::new();
    let mut pending = None;
    loop {
        let command = match pending.take() {
            Some(command) => command,
            None => match input.recv().await {
                Some(command) => command,
                None => break,
            },
        };
        let (operation, finish) = match command {
            Input::Audio(pcm) => {
                let pcm = coalesce_audio(pcm, &mut input, &mut pending);
                sequence += 1;
                (
                    Op::Append {
                        stream_id: stream_id.clone(),
                        sequence,
                        pcm16_hex: encode_hex(&pcm),
                    },
                    None,
                )
            }
            // Each bounded append is acknowledged. A flush preserves ordering
            // but implies no final sentence; only Finish can mint a final receipt.
            Input::Flush => continue,
            Input::Finish(mark) => {
                sequence += 1;
                (
                    Op::Finish {
                        stream_id: stream_id.clone(),
                        sequence,
                    },
                    Some(mark),
                )
            }
            Input::Cancel => return Ok(()),
        };
        let Reply::Transcript {
            text,
            is_final,
            audio_duration_ms,
            ..
        } = client.call(operation).await?
        else {
            return Err(SpeechError::InvalidResponse);
        };
        if is_final {
            event_sequence += 1;
            events
                .try_send(Ok(TranscriptEvent::Final {
                    sequence: event_sequence,
                    text,
                    model,
                    finish_to_final_ms: finish.map(|mark| millis(mark.elapsed())),
                    audio_duration_ms,
                }))
                .map_err(|_| SpeechError::Backpressure)?;
            cancel.completed.store(true, Ordering::Release);
            return Ok(());
        }
        if text != previous {
            let delta = text
                .strip_prefix(&previous)
                .ok_or(SpeechError::InvalidResponse)?
                .to_string();
            previous = text;
            event_sequence += 1;
            events
                .try_send(Ok(TranscriptEvent::Partial {
                    sequence: event_sequence,
                    text: delta,
                    received_after_start_ms: millis(started.elapsed()),
                }))
                .map_err(|_| SpeechError::Backpressure)?;
        }
    }
    Ok(())
}

/// Drain only audio already waiting behind this frame. Native RPCs are signed
/// and acknowledged, so forwarding every 20ms capture frame separately can
/// outpace them. The wire already admits 100ms/3200-byte appends at16kHz.
/// Never wait for more audio, cross a control command, split a capture frame,
/// or grow the input queue. One deferred command preserves exact FIFO order.
fn coalesce_audio(
    mut pcm: Vec<u8>,
    input: &mut mpsc::Receiver<Input>,
    pending: &mut Option<Input>,
) -> Vec<u8> {
    const MAX_APPEND_BYTES: usize = 3200;
    while pcm.len() < MAX_APPEND_BYTES {
        match input.try_recv() {
            Ok(Input::Audio(next)) if pcm.len() + next.len() <= MAX_APPEND_BYTES => {
                pcm.extend_from_slice(&next);
            }
            Ok(command) => {
                *pending = Some(command);
                break;
            }
            Err(_) => break,
        }
    }
    pcm
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 15) as usize] as char);
    }
    encoded
}
fn decode_hex(encoded: &str) -> Result<Vec<u8>, SpeechError> {
    if encoded.len() > MAX_READ_BYTES as usize * 2 || encoded.len() % 2 != 0 {
        return Err(SpeechError::InvalidResponse);
    }
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            fn digit(byte: u8) -> Result<u8, SpeechError> {
                match byte {
                    b'0'..=b'9' => Ok(byte - b'0'),
                    b'a'..=b'f' => Ok(byte - b'a' + 10),
                    _ => Err(SpeechError::InvalidResponse),
                }
            }
            Ok(digit(pair[0])? * 16 + digit(pair[1])?)
        })
        .collect()
}

pub(super) fn verify_audio(
    audio: &[u8],
    digest: &str,
    duration_ms: u64,
) -> Result<(), SpeechError> {
    if format!("{:x}", Sha256::digest(audio)) != digest {
        return Err(SpeechError::InvalidResponse);
    }
    // Parse actual RIFF chunks. Signed metadata does not establish playability.
    if audio.len() < 44
        || &audio[..4] != b"RIFF"
        || &audio[8..12] != b"WAVE"
        || u32::from_le_bytes(audio[4..8].try_into().unwrap()) as usize + 8 != audio.len()
    {
        return Err(SpeechError::InvalidResponse);
    }
    let mut at = 12;
    let mut pcm_format = false;
    let mut data = None;
    while at + 8 <= audio.len() {
        let size = u32::from_le_bytes(audio[at + 4..at + 8].try_into().unwrap()) as usize;
        let start = at + 8;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= audio.len())
            .ok_or(SpeechError::InvalidResponse)?;
        match &audio[at..at + 4] {
            b"fmt " if size >= 16 => {
                if pcm_format {
                    return Err(SpeechError::InvalidResponse);
                }
                pcm_format = &audio[start..start + 2] == 1u16.to_le_bytes()
                    && &audio[start + 2..start + 4] == 1u16.to_le_bytes()
                    && &audio[start + 4..start + 8] == 24_000u32.to_le_bytes()
                    && &audio[start + 8..start + 12] == 48_000u32.to_le_bytes()
                    && &audio[start + 12..start + 14] == 2u16.to_le_bytes()
                    && &audio[start + 14..start + 16] == 16u16.to_le_bytes();
                if !pcm_format {
                    return Err(SpeechError::InvalidResponse);
                }
            }
            b"data" => {
                if data.is_some() || size == 0 || size % 2 != 0 {
                    return Err(SpeechError::InvalidResponse);
                }
                data = Some(size);
            }
            _ => {}
        }
        at = end
            .checked_add(size % 2)
            .ok_or(SpeechError::InvalidResponse)?;
    }
    let samples = data.ok_or(SpeechError::InvalidResponse)? / 2;
    if at != audio.len() || !pcm_format || samples as u64 * 1000 / 24_000 != duration_ms {
        return Err(SpeechError::InvalidResponse);
    }
    Ok(())
}

pub(super) async fn synthesize(
    root: &Path,
    request: &SpeechRequest,
    configured_voice: Option<&String>,
) -> Result<VerifiedSpeechOutput, SpeechError> {
    if request.format != SpeechAudioFormat::Wav
        || request.text.trim().is_empty()
        || request.text.len() > 4096
    {
        return Err(SpeechError::InvalidRequest);
    }
    let voice = request
        .voice_id
        .as_ref()
        .or(configured_voice)
        .ok_or(SpeechError::MissingVoice)?;
    if voice.trim().is_empty() || voice.len() > 256 {
        return Err(SpeechError::InvalidRequest);
    }
    let root = root.to_path_buf();
    let client =
        tokio::task::spawn_blocking(move || Client::open(&root, SpeechWorkload::Synthesis))
            .await
            .map_err(|_| SpeechError::Transport)??;
    let started = Instant::now();
    let reply = client
        .call(Op::StartSynthesis {
            intent_id: Uuid::new_v4().to_string(),
            text: request.text.clone(),
            voice_id: voice.clone(),
        })
        .await?;
    let Reply::SynthesisStarted { run_id, model } = reply else {
        return Err(SpeechError::InvalidResponse);
    };
    let cancel = CancelOnDrop {
        client: client.clone(),
        operation: Op::CancelSynthesis {
            run_id: run_id.clone(),
        },
        completed: Arc::new(AtomicBool::new(false)),
    };
    let result = tokio::time::timeout(Duration::from_secs(120), async {
        let (sha, total, duration_ms) = loop {
            match client
                .call(Op::SynthesisStatus {
                    run_id: run_id.clone(),
                })
                .await?
            {
                Reply::SynthesisPending { .. } => {
                    tokio::time::sleep(Duration::from_millis(100)).await
                }
                Reply::SynthesisReady {
                    audio_sha256,
                    audio_bytes,
                    audio_duration_ms,
                    ..
                } => break (audio_sha256, audio_bytes, audio_duration_ms),
                _ => return Err(SpeechError::InvalidResponse),
            }
        };
        let mut audio = Vec::with_capacity(total as usize);
        while (audio.len() as u64) < total {
            let reply = client
                .call(Op::ReadSynthesis {
                    run_id: run_id.clone(),
                    offset: audio.len() as u64,
                    max_bytes: MAX_READ_BYTES,
                })
                .await?;
            let Reply::SynthesisChunk {
                total_bytes, hex, ..
            } = reply
            else {
                return Err(SpeechError::InvalidResponse);
            };
            if total_bytes != total || hex.is_empty() {
                return Err(SpeechError::InvalidResponse);
            }
            audio.extend_from_slice(&decode_hex(&hex)?);
        }
        verify_audio(&audio, &sha, duration_ms)?;
        Ok(VerifiedSpeechOutput {
            run_id: run_id.clone(),
            text_sha256: format!("{:x}", Sha256::digest(request.text.as_bytes())),
            audio_sha256: sha,
            output: SpeechOutput {
                audio,
                format: SpeechAudioFormat::Wav,
                model,
                input_characters: request.text.chars().count(),
                elapsed_ms: millis(started.elapsed()),
            },
        })
    })
    .await
    .map_err(|_| SpeechError::TimedOut)?;
    if result.is_ok() {
        // Release the receiver's bounded audio buffer once all verified bytes
        // are local. Cancellation remains best effort on the exact live route.
        if matches!(
            tokio::time::timeout(
                Duration::from_secs(3),
                client.call(Op::CancelSynthesis {
                    run_id: run_id.clone()
                }),
            )
            .await,
            Ok(Ok(Reply::SynthesisCancelled { .. }))
        ) {
            cancel.completed.store(true, Ordering::Release);
        }
    }
    result
}
#[cfg(test)]
#[path = "speech_computer_tests.rs"]
mod tests;
