// Origin: CTOX
// License: AGPL-3.0-only
//! Transient draft dictation. No meeting binding, command, transcript or receipt write.
use super::{policy::BusinessOsPermission, store};
use crate::execution::speech::{
    PcmFormat, SpeechBackend, SpeechError, SpeechGateway, SpeechRuntimeConfig, TranscriptionStream,
    VerifiedTranscriptEvent,
};
use anyhow::{ensure, Context};
use base64::{engine::general_purpose::STANDARD, Engine};
use rxdb::plugins::replication_webrtc::{
    index_mod::GuardedAuxiliaryResponse, WebRTCPublicationGuard,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, Notify};
use uuid::Uuid;
use zeroize::Zeroizing;

pub(super) const METHOD: &str = "ctox.workjet.speech.dictation.v1";
pub(super) const CAPABILITY: &str = "ctox-workjet-speech-dictation-v1";
const MAX_PCM: usize = 3200;
const MAX_AUDIO: usize = 16_000 * 2 * 60;
const MAX_TEXT: usize = 32768;
const RETAIN: Duration = Duration::from_secs(100);

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Scope {
    instance_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    op: String,
    command_id: String,
    scope: Scope,
    #[serde(default)]
    stream_id: Option<String>,
    #[serde(default)]
    sequence: Option<u64>,
    #[serde(default)]
    pcm_base64: Option<String>,
    #[serde(default)]
    after_sequence: Option<u64>,
}
impl Request {
    fn parse(params: Vec<Value>) -> anyhow::Result<Self> {
        ensure!(
            params.len() == 1 && serde_json::to_vec(&params)?.len() <= 6144,
            "invalid_request"
        );
        let r: Self = serde_json::from_value(params.into_iter().next().unwrap())
            .map_err(|_| anyhow::anyhow!("invalid_request"))?;
        ensure!(
            Uuid::parse_str(&r.command_id).is_ok()
                && !r.scope.instance_id.is_empty()
                && r.scope.instance_id.len() <= 256
                && r.scope.instance_id.trim() == r.scope.instance_id
                && !r.scope.instance_id.chars().any(char::is_control),
            "invalid_request"
        );
        let valid = match r.op.as_str() {
            "open" => {
                r.stream_id.is_none()
                    && r.sequence.is_none()
                    && r.pcm_base64.is_none()
                    && r.after_sequence.is_none()
            }
            "write" => {
                r.stream_id.is_some()
                    && r.sequence
                        .is_some_and(|v| v > 0 && v <= 9_007_199_254_740_991)
                    && r.pcm_base64
                        .as_ref()
                        .is_some_and(|v| !v.is_empty() && v.len() <= 4268)
                    && r.after_sequence.is_none()
            }
            "read" => {
                r.stream_id.is_some()
                    && r.after_sequence.is_some_and(|v| v <= 9_007_199_254_740_991)
                    && r.sequence.is_none()
                    && r.pcm_base64.is_none()
            }
            "finish" | "cancel" => {
                r.stream_id.is_some()
                    && r.sequence.is_none()
                    && r.pcm_base64.is_none()
                    && r.after_sequence.is_none()
            }
            _ => false,
        };
        ensure!(
            valid
                && r.stream_id
                    .as_ref()
                    .is_none_or(|id| Uuid::parse_str(id).is_ok()),
            "invalid_request"
        );
        Ok(r)
    }
    fn pcm(&self) -> anyhow::Result<Vec<u8>> {
        let pcm = STANDARD
            .decode(self.pcm_base64.as_ref().context("invalid_request")?)
            .map_err(|_| anyhow::anyhow!("invalid_request"))?;
        ensure!(
            !pcm.is_empty() && pcm.len().is_multiple_of(2) && pcm.len() <= MAX_PCM,
            "invalid_request"
        );
        Ok(pcm)
    }
}
type Current = Arc<dyn Fn() -> bool + Send + Sync>;

fn config_binding(root: &Path) -> anyhow::Result<String> {
    let config = SpeechRuntimeConfig::load(root)?;
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&config.transcription)?);
    if config.transcription == SpeechBackend::Mistral {
        let key = Zeroizing::new(crate::execution::speech::mistral_key(root).unwrap_or_default());
        digest.update(key.as_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}

struct Authority {
    root: PathBuf,
    token: String,
    scope: Scope,
    config_binding: String,
    current: Current,
}
impl Authority {
    fn binding_check(&self) -> anyhow::Result<()> {
        ensure!(
            store::existing_instance_id(&self.root)? == self.scope.instance_id,
            "retired"
        );
        ensure!(
            config_binding(&self.root)? == self.config_binding,
            "retired"
        );
        Ok(())
    }
    fn policy_from_signer<T>(
        &self,
        secret: &[u8],
        apply: impl FnOnce() -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let mut conn = rusqlite::Connection::open_with_flags(
            store::business_os_store_path(&self.root),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
        let tx = conn.transaction()?;
        let at = chrono::Utc::now().timestamp_millis();
        store::verified_webrtc_capability_claims_from_connection(&tx, &self.token, secret, at)
            .context("retired")?;
        ensure!(
            store::check_webrtc_collection_permission_from_connection(
                &tx,
                &self.token,
                secret,
                "business_commands",
                BusinessOsPermission::DataWrite,
                at
            )?,
            "denied"
        );
        apply()
    }
    fn policy<T>(&self, publish: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
        self.binding_check()?;
        store::with_current_webrtc_capability_signer(&self.root, |secret| {
            self.policy_from_signer(secret, publish)
        })
    }
    fn check(&self) -> anyhow::Result<()> {
        ensure!((self.current)(), "retired");
        self.binding_check()?;
        // Preparatory probes must not compete for the publication writer fence.
        // Only the guarded native responder may publish a transcript.
        store::with_webrtc_capability_signer_snapshot(&self.root, |secret| {
            self.policy_from_signer(secret, || Ok(()))
        })?;
        ensure!((self.current)(), "retired");
        Ok(())
    }
}
impl WebRTCPublicationGuard for Authority {
    fn with_current(
        &self,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()> {
        // Native responder holds its exact peer-generation fence during this callback.
        self.policy(|| publish().map_err(anyhow::Error::msg))
            .map_err(|_| rxdb::rx_error::new_rx_error("DICTATION_RETIRED", None))
    }
}
fn error_class(error: SpeechError) -> &'static str {
    match error {
        SpeechError::MissingCredential => "missing_credential",
        SpeechError::ConfigurationUnavailable => "configuration_unavailable",
        SpeechError::UnsupportedBackend => "backend_unavailable",
        SpeechError::TimedOut => "timeout",
        SpeechError::ProviderRejected {
            http_status: Some(401),
        } => "credentials_rejected",
        SpeechError::ProviderRejected {
            http_status: Some(403),
        } => "access_denied",
        SpeechError::ProviderRejected {
            http_status: Some(429),
        } => "rate_limit",
        SpeechError::ProviderRejected {
            http_status: Some(402),
        } => "quota",
        SpeechError::ProviderRejected { .. } => "provider_rejected",
        SpeechError::Backpressure => "backpressure",
        SpeechError::InvalidResponse => "invalid_response",
        _ => "transport",
    }
}

#[derive(Default)]
struct Snapshot {
    event_sequence: u64,
    partial: String,
    latest_event: Option<Value>,
    text: Option<String>,
    error: Option<&'static str>,
    finishing: bool,
}
struct Session {
    authority: Arc<Authority>,
    commands: mpsc::Sender<Command>,
    cancel: Arc<Notify>,
    canceled: Arc<AtomicBool>,
    snapshot: Arc<Mutex<Snapshot>>,
    final_gate: Arc<Mutex<()>>,
    open_command_id: String,
    opened: Instant,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.canceled.store(true, Ordering::SeqCst);
        self.cancel.notify_one();
        if let Ok(task) = self.task.get_mut() {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
    }
}
enum Command {
    Write(u64, Vec<u8>, oneshot::Sender<Result<(), &'static str>>),
    Finish(oneshot::Sender<Result<(), &'static str>>),
}
impl Session {
    fn snapshot(&self, r: &Request, id: &str) -> anyhow::Result<Value> {
        let s = self
            .snapshot
            .lock()
            .map_err(|_| anyhow::anyhow!("unavailable"))?;
        let after = r.after_sequence.unwrap_or(s.event_sequence);
        ensure!(after <= s.event_sequence, "invalid_cursor");
        let canceled = self.canceled.load(Ordering::SeqCst);
        let state = if canceled {
            "canceled"
        } else if s.error.is_some() {
            "failed"
        } else if s.text.is_some() {
            "finished"
        } else if s.finishing {
            "finishing"
        } else {
            "open"
        };
        // A single coalesced full partial snapshot avoids expired delta cursors.
        // No text is published after cancellation or failure, including a raced final.
        let events: Vec<&Value> = if canceled || s.error.is_some() || after >= s.event_sequence {
            vec![]
        } else {
            s.latest_event.iter().collect()
        };
        Ok(
            json!({"action":"speech.dictation", "commandId":r.command_id, "op":r.op,
            "streamId":id, "state":state, "events":events,
            "text":if canceled || s.error.is_some() { None } else { s.text.as_ref() }, "error":s.error}),
        )
    }
    fn cancel(&self) {
        let _fence = self.final_gate.lock().unwrap_or_else(|e| e.into_inner());
        self.canceled.store(true, Ordering::SeqCst);
        let mut s = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        s.partial.clear();
        s.latest_event = None;
        s.text = None;
        self.cancel.notify_one();
    }
}
struct Registry<P> {
    sessions: Mutex<HashMap<String, (P, Arc<Session>)>>,
    slots: Arc<tokio::sync::Semaphore>,
    opening: tokio::sync::Mutex<()>,
}
impl<P: Eq> Registry<P> {
    fn lookup(&self, peer: &P, token: &str, r: &Request) -> anyhow::Result<Arc<Session>> {
        let entries = self
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("unavailable"))?;
        let (owner, session) = entries
            .get(r.stream_id.as_deref().context("invalid_request")?)
            .context("unavailable")?;
        ensure!(
            owner == peer && session.authority.token == token && session.authority.scope == r.scope,
            "unavailable"
        );
        ensure!(session.opened.elapsed() < RETAIN, "retired");
        Ok(Arc::clone(session))
    }
}
pub(super) fn register(
    pool: &ctox_sync::native::NativePool,
    root: &Path,
) -> rxdb::rx_error::RxResult<()> {
    use rxdb::plugins::replication_webrtc::WebRTCConnectionHandler;
    let registry = Arc::new(Registry {
        sessions: Mutex::new(HashMap::new()),
        slots: Arc::new(tokio::sync::Semaphore::new(2)),
        opening: tokio::sync::Mutex::new(()),
    });
    let weak_pool = Arc::downgrade(pool);
    let root = root.to_owned();
    pool.register_guarded_auxiliary_request_handler(
        METHOD,
        Arc::new(move |peer, token, params| {
            let registry = Arc::clone(&registry);
            let root = root.clone();
            let weak_pool = weak_pool.clone();
            Box::pin(async move {
                let request =
                    Request::parse(params).map_err(|_| "dictation invalid_request".to_owned())?;
                let peer_for_check = peer.clone();
                let token_for_check = token.clone();
                let current: Current = Arc::new(move || {
                    weak_pool.upgrade().is_some_and(|p| {
                        !p.canceled.load(Ordering::SeqCst)
                            && p.connection_handler.is_peer_current(&peer_for_check)
                            && p.connection_handler
                                .peer_capability_token(&peer_for_check)
                                .as_deref()
                                == Some(token_for_check.as_str())
                    })
                });
                handle(registry, peer, root, token, current, request)
                    .await
                    .map_err(|e| {
                        format!(
                            "dictation {}",
                            match e.to_string().as_str() {
                                "invalid_request" => "invalid_request",
                                "capacity" => "busy",
                                "invalid_cursor" => "invalid_cursor",
                                _ => "unavailable_or_retired",
                            }
                        )
                    })
            })
        }),
    )
}
async fn handle<P: Eq + Clone>(
    registry: Arc<Registry<P>>,
    peer: P,
    root: PathBuf,
    token: String,
    current: Current,
    r: Request,
) -> anyhow::Result<GuardedAuxiliaryResponse> {
    let session = if r.op == "open" {
        let _opening =
            tokio::time::timeout(Duration::from_secs(15), registry.opening.lock()).await?;
        let existing = {
            let mut entries = registry
                .sessions
                .lock()
                .map_err(|_| anyhow::anyhow!("unavailable"))?;
            entries.retain(|_, (_, s)| s.opened.elapsed() < RETAIN);
            entries
                .iter()
                .find(|(_, (p, s))| {
                    p == &peer
                        && s.authority.token == token
                        && s.authority.scope == r.scope
                        && s.open_command_id == r.command_id
                })
                .map(|(_, (_, s))| Arc::clone(s))
        };
        if let Some(session) = existing {
            session
        } else {
            ensure!(registry.sessions.lock().unwrap().len() < 32, "capacity");
            let permit = Arc::clone(&registry.slots)
                .try_acquire_owned()
                .context("capacity")?;
            let pre_root = root.clone();
            let scope = r.scope.clone();
            let token = token.clone();
            let authority = tokio::task::spawn_blocking(move || {
                let a = Arc::new(Authority {
                    config_binding: config_binding(&pre_root)?,
                    root: pre_root,
                    token,
                    scope,
                    current,
                });
                a.check()?;
                Ok::<_, anyhow::Error>(a)
            })
            .await??;
            let open_root = root.clone();
            let opened = tokio::time::timeout(Duration::from_secs(15), async move {
                SpeechGateway::from_root(&open_root)?
                    .open_transcription(PcmFormat::default())
                    .await
            })
            .await
            .unwrap_or(Err(SpeechError::TimedOut));
            let a = Arc::clone(&authority);
            tokio::task::spawn_blocking(move || a.check()).await??;
            let id = opened
                .as_ref()
                .map(|s| s.stream_id().to_owned())
                .unwrap_or_else(|_| Uuid::new_v4().to_string());
            let (commands, rx) = mpsc::channel(8);
            let snapshot = Snapshot {
                error: opened.as_ref().err().copied().map(error_class),
                ..Snapshot::default()
            };
            let session = Arc::new(Session {
                authority: Arc::clone(&authority),
                commands,
                cancel: Arc::new(Notify::new()),
                canceled: Arc::new(AtomicBool::new(false)),
                snapshot: Arc::new(Mutex::new(snapshot)),
                final_gate: Arc::new(Mutex::new(())),
                open_command_id: r.command_id.clone(),
                opened: Instant::now(),
                task: Mutex::new(None),
            });
            if let Ok(stream) = opened {
                *session.task.lock().unwrap() = Some(tokio::spawn(run(
                    stream,
                    rx,
                    Arc::clone(&authority),
                    Arc::clone(&session.cancel),
                    Arc::clone(&session.canceled),
                    Arc::clone(&session.snapshot),
                    Arc::clone(&session.final_gate),
                    permit,
                )));
            }
            registry
                .sessions
                .lock()
                .unwrap()
                .insert(id, (peer, Arc::clone(&session)));
            session
        }
    } else {
        registry.lookup(&peer, &token, &r)?
    };
    let a = Arc::clone(&session.authority);
    tokio::task::spawn_blocking(move || a.check()).await??;
    let id = if r.op == "open" {
        registry
            .sessions
            .lock()
            .unwrap()
            .iter()
            .find(|(_, (_, s))| Arc::ptr_eq(s, &session))
            .map(|(id, _)| id.clone())
            .context("retired")?
    } else {
        r.stream_id.clone().unwrap()
    };
    if r.op == "cancel" {
        session.cancel();
    }
    if r.op == "write" || r.op == "finish" {
        let terminal = {
            let s = session.snapshot.lock().unwrap();
            s.error.is_some() || s.text.is_some() || session.canceled.load(Ordering::SeqCst)
        };
        if !terminal {
            let (tx, rx) = oneshot::channel();
            let cmd = if r.op == "write" {
                Command::Write(r.sequence.unwrap(), r.pcm()?, tx)
            } else {
                Command::Finish(tx)
            };
            session
                .commands
                .try_send(cmd)
                .map_err(|_| anyhow::anyhow!("capacity"))?;
            tokio::time::timeout(Duration::from_secs(2), rx)
                .await??
                .map_err(anyhow::Error::msg)?;
        }
    }
    Ok(GuardedAuxiliaryResponse {
        result: session.snapshot(&r, &id)?,
        publication: Arc::clone(&session.authority) as Arc<dyn WebRTCPublicationGuard>,
    })
}
#[allow(clippy::too_many_arguments)]
async fn run(
    mut stream: TranscriptionStream,
    mut commands: mpsc::Receiver<Command>,
    authority: Arc<Authority>,
    cancel: Arc<Notify>,
    canceled: Arc<AtomicBool>,
    snapshot: Arc<Mutex<Snapshot>>,
    final_gate: Arc<Mutex<()>>,
    _permit: tokio::sync::OwnedSemaphorePermit,
) {
    let id = stream.stream_id().to_owned();
    let started = Instant::now();
    let mut touched = Instant::now();
    let mut bytes = 0;
    let mut sequence = 0;
    let mut previous = Vec::new();
    let mut finishing = false;
    let mut finish_at = None;
    let mut producer_sequence = 0;
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    let outcome: Result<(), &'static str> = loop {
        tokio::select! {
            biased;
            _ = cancel.notified() => break Err("canceled"),
            _ = tick.tick() => {
                if canceled.load(Ordering::SeqCst) || !(authority.current)() { break Err("retired"); }
                if started.elapsed() > Duration::from_secs(80) || touched.elapsed() > Duration::from_secs(5) && !finishing
                    || finish_at.is_some_and(|at: Instant| at.elapsed() > Duration::from_secs(15)) { break Err("timeout"); }
                let a = Arc::clone(&authority);
                if !matches!(tokio::task::spawn_blocking(move || a.check()).await, Ok(Ok(()))) { break Err("retired"); }
            },
            cmd = commands.recv() => {
                touched = Instant::now();
                match cmd {
                    Some(Command::Write(next, pcm, ack)) => {
                        if !finishing && next == sequence && pcm == previous { let _ = ack.send(Ok(())); continue; }
                        if finishing || next != sequence + 1 || bytes + pcm.len() > MAX_AUDIO {
                            let _ = ack.send(Err("invalid_sequence_or_audio")); break Err("invalid_sequence_or_audio");
                        }
                        if let Err(error) = stream.append_pcm(&pcm) {
                            let class = error_class(error); let _ = ack.send(Err(class)); break Err(class);
                        }
                        sequence = next; bytes += pcm.len(); previous = pcm; let _ = ack.send(Ok(()));
                    },
                    Some(Command::Finish(ack)) => {
                        if !finishing {
                            if bytes == 0 || stream.finish_audio().is_err() {
                                let _ = ack.send(Err("invalid_finish")); break Err("invalid_finish");
                            }
                            finishing = true; finish_at = Some(Instant::now()); snapshot.lock().unwrap().finishing = true;
                        }
                        let _ = ack.send(Ok(()));
                    },
                    None => break Err("canceled"),
                }
            },
            event = stream.next_verified_event() => match event {
                Some(Ok(VerifiedTranscriptEvent::Partial {stream_id, sequence: next, text, ..})) => {
                    if stream_id != id || next <= producer_sequence || text.len() > 8192 { break Err("invalid_response"); }
                    producer_sequence = next;
                    let mut s = snapshot.lock().unwrap();
                    if s.partial.len() + text.len() > 8192 { break Err("invalid_response"); }
                    s.partial.push_str(&text); s.event_sequence += 1;
                    s.latest_event = Some(json!({"sequence":s.event_sequence, "text":s.partial}));
                },
                Some(Ok(VerifiedTranscriptEvent::Final(value))) => {
                    if !finishing || value.stream_id() != id || value.sequence() <= producer_sequence
                        || value.text().len() > MAX_TEXT || value.text().trim().is_empty()
                        || value.audio_duration_ms() > 60_000 { break Err("invalid_response"); }
                    let a = Arc::clone(&authority); let stop = Arc::clone(&canceled);
                    let gate = Arc::clone(&final_gate); let state = Arc::clone(&snapshot);
                    let admitted = tokio::task::spawn_blocking(move || {
                        let _fence = gate.lock().map_err(|_| anyhow::anyhow!("retired"))?;
                        ensure!(!stop.load(Ordering::SeqCst), "canceled"); a.check()?;
                        state.lock().map_err(|_| anyhow::anyhow!("retired"))?.text = Some(value.text().to_owned());
                        Ok::<_, anyhow::Error>(())
                    }).await;
                    if matches!(admitted, Ok(Ok(()))) { return; }
                    break Err("retired");
                },
                Some(Err(error)) => break Err(error_class(error)),
                None => break Err("missing_final"),
            }
        }
    };
    {
        let mut s = snapshot.lock().unwrap();
        s.error = outcome.err();
        s.partial.clear();
        s.latest_event = None;
        s.text = None;
    }
    stream.cancel().await;
}
#[cfg(test)]
#[path = "rxdb_peer_dictation_tests.rs"]
mod tests;
