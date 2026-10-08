// Origin: CTOX
// License: AGPL-3.0-only
//! Renderer ingress for the real gateway stream. No caller transcript is accepted.
#[cfg(test)]
#[path = "rxdb_peer_jour_fixe_speech_tests.rs"]
mod integration_tests;
use super::{
    project_chats::{
        jour_fixe_owner::LiveMeetingBinding,
        jour_fixe_speech::{self, BoundTranscription},
    },
    store,
};
use crate::execution::speech::{PcmFormat, SpeechGateway, VerifiedTranscriptEvent};
use anyhow::{ensure, Context};
use base64::{engine::general_purpose::STANDARD, Engine};
use rxdb::plugins::replication_webrtc::{
    index_mod::GuardedAuxiliaryResponse, WebRTCPublicationGuard,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, Notify};
use uuid::Uuid;

pub(super) const METHOD: &str = "ctox.workjet.jour_fixe.speech.v1";
pub(super) const CAPABILITY: &str = "ctox-workjet-jour-fixe-speech-v1";
const MAX_PCM: usize = 3200;
const MAX_AUDIO: usize = 16_000 * 2 * 15;
const MAX_EVENTS: usize = 32;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Scope {
    instance_id: String,
    project_id: String,
    meeting_id: String,
    deck_revision: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    op: String,
    scope: Scope,
    #[serde(default)]
    request_id: Option<String>,
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
            "invalid speech request"
        );
        let r: Self = serde_json::from_value(params.into_iter().next().unwrap())?;
        ensure!(
            [
                &r.scope.instance_id,
                &r.scope.project_id,
                &r.scope.meeting_id
            ]
            .iter()
            .all(|s| !s.is_empty() && s.len() <= 256 && s.trim() == *s),
            "invalid speech scope"
        );
        ensure!(r.scope.deck_revision > 0, "invalid speech deck");
        let valid = match r.op.as_str() {
            "open" => {
                r.request_id
                    .as_ref()
                    .is_some_and(|v| Uuid::parse_str(v).is_ok())
                    && r.stream_id.is_none()
                    && r.sequence.is_none()
                    && r.pcm_base64.is_none()
                    && r.after_sequence.is_none()
            }
            "write" => {
                r.stream_id.is_some()
                    && r.sequence.is_some_and(|v| v > 0)
                    && r.pcm_base64.as_ref().is_some_and(|v| v.len() <= 4268)
                    && r.request_id.is_none()
                    && r.after_sequence.is_none()
            }
            "read" => {
                r.stream_id.is_some()
                    && r.after_sequence.is_some()
                    && r.request_id.is_none()
                    && r.sequence.is_none()
                    && r.pcm_base64.is_none()
            }
            "finish" | "cancel" => {
                r.stream_id.is_some()
                    && r.request_id.is_none()
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
            "invalid speech operation"
        );
        Ok(r)
    }
}

type Current = Arc<dyn Fn() -> bool + Send + Sync>;
struct Authority {
    root: PathBuf,
    token: String,
    scope: Scope,
    binding: LiveMeetingBinding,
    current: Current,
}
impl Authority {
    fn check(&self) -> anyhow::Result<()> {
        ensure!((self.current)(), "speech connection retired");
        ensure!(
            store::existing_instance_id(&self.root)? == self.scope.instance_id,
            "speech instance changed"
        );
        jour_fixe_speech::revalidate_binding(&self.root, &self.token, &self.binding)?;
        ensure!((self.current)(), "speech connection retired");
        Ok(())
    }
}
impl WebRTCPublicationGuard for Authority {
    fn with_current(
        &self,
        publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
    ) -> rxdb::rx_error::RxResult<()> {
        jour_fixe_speech::with_publication_authority(
            &self.root,
            &self.token,
            &self.scope.instance_id,
            &self.binding,
            || {
                ensure!((self.current)(), "speech connection retired");
                publish().map_err(anyhow::Error::msg)
            },
        )
        .map_err(|_| rxdb::rx_error::new_rx_error("JOUR_FIXE_SPEECH_RETIRED", None))
    }
}

#[derive(Default)]
struct Snapshot {
    events: VecDeque<Value>,
    event_sequence: u64,
    error: Option<&'static str>,
    committed: Option<Value>,
    finished: bool,
}
struct Session {
    authority: Arc<Authority>,
    commands: mpsc::Sender<Command>,
    cancel: Arc<Notify>,
    canceled: Arc<AtomicBool>,
    snapshot: Arc<Mutex<Snapshot>>,
    commit_gate: Arc<Mutex<()>>,
    request_id: String,
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
    fn snapshot(&self, id: &str, after: u64) -> anyhow::Result<Value> {
        let s = self
            .snapshot
            .lock()
            .map_err(|_| anyhow::anyhow!("speech state unavailable"))?;
        ensure!(
            after <= s.event_sequence
                && s.events
                    .front()
                    .is_none_or(|e| after.saturating_add(1) >= e["sequence"].as_u64().unwrap_or(0)),
            "speech event cursor expired"
        );
        Ok(
            json!({"streamId":id, "state":if self.canceled.load(Ordering::SeqCst) {"canceled"} else if s.error.is_some() {"failed"} else if s.committed.is_some() {"committed"} else if s.finished {"finishing"} else {"open"},
            "events":s.events.iter().filter(|e| e["sequence"].as_u64().unwrap_or(0) > after).collect::<Vec<_>>(),
            "receipt":s.committed, "error":s.error}),
        )
    }
    fn cancel(&self) {
        // If cancellation wins this fence, no final is admitted afterwards.
        // A domain commit that already won cannot be undone by disconnecting.
        let _fence = self.commit_gate.lock().unwrap_or_else(|e| e.into_inner());
        self.canceled.store(true, Ordering::SeqCst);
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
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("speech registry unavailable"))?;
        let (owner, session) = sessions
            .get(r.stream_id.as_deref().context("missing stream")?)
            .context("speech stream unavailable")?;
        ensure!(
            owner == peer && session.authority.token == token && session.authority.scope == r.scope,
            "speech stream unavailable"
        );
        Ok(Arc::clone(session))
    }
}

pub(super) fn register(
    pool: &ctox_sync::native::NativePool,
    root: &std::path::Path,
) -> rxdb::rx_error::RxResult<()> {
    use rxdb::plugins::replication_webrtc::WebRTCConnectionHandler;
    let registry = Arc::new(Registry {
        sessions: Mutex::new(HashMap::new()),
        slots: Arc::new(tokio::sync::Semaphore::new(2)),
        opening: tokio::sync::Mutex::new(()),
    });
    let weak_pool = Arc::downgrade(pool);
    let root = root.to_path_buf();
    pool.register_guarded_auxiliary_request_handler(
        METHOD,
        Arc::new(move |peer, token, params| {
            let registry = Arc::clone(&registry);
            let weak_pool = weak_pool.clone();
            let root = root.clone();
            Box::pin(async move {
                let request =
                    Request::parse(params).map_err(|_| "invalid speech request".to_owned())?;
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
                let result = handle(registry, peer, root, token, current, request).await;
                result.map_err(|e| {
                    // No provider payload, credentials, transcript or caller parameters in errors.
                    if e.to_string() == "invalid speech request" {
                        "invalid speech request".to_owned()
                    } else {
                        "speech stream unavailable or retired".to_owned()
                    }
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
    if r.op == "open" {
        let _opening =
            tokio::time::timeout(Duration::from_secs(15), registry.opening.lock()).await?;
        let existing = {
            let entries = registry.sessions.lock().unwrap();
            entries
                .iter()
                .find(|(_, (p, s))| {
                    p == &peer
                        && s.authority.token == token
                        && s.authority.scope == r.scope
                        && Some(&s.request_id) == r.request_id.as_ref()
                })
                .map(|(id, (_, s))| (id.clone(), Arc::clone(s)))
        };
        if let Some((id, session)) = existing {
            let a = Arc::clone(&session.authority);
            tokio::task::spawn_blocking(move || a.check()).await??;
            let after = session.snapshot.lock().unwrap().event_sequence;
            let mut value = session.snapshot(&id, after)?;
            value["requestId"] = json!(r.request_id);
            return Ok(GuardedAuxiliaryResponse {
                result: value,
                publication: Arc::clone(&session.authority) as Arc<dyn WebRTCPublicationGuard>,
            });
        }
        let permit = Arc::clone(&registry.slots)
            .try_acquire_owned()
            .context("speech capacity")?;
        {
            let mut entries = registry
                .sessions
                .lock()
                .map_err(|_| anyhow::anyhow!("speech registry unavailable"))?;
            entries.retain(|_, (_, s)| s.opened.elapsed() < Duration::from_secs(45));
            ensure!(entries.len() < 32, "speech receipt capacity");
        }
        // requestId is a UI correlation nonce, not a persisted command or credential.
        let scope = r.scope;
        let pre_root = root.clone();
        let pre_token = token.clone();
        let pre_scope = scope.clone();
        let binding = tokio::task::spawn_blocking(move || {
            ensure!(
                store::existing_instance_id(&pre_root)? == pre_scope.instance_id,
                "speech instance changed"
            );
            jour_fixe_speech::open_binding(
                &pre_root,
                &pre_token,
                &pre_scope.project_id,
                &pre_scope.meeting_id,
                pre_scope.deck_revision,
            )
        })
        .await??;
        ensure!(current(), "speech connection retired");
        let open_root = root.clone();
        let stream = tokio::time::timeout(Duration::from_secs(15), async move {
            SpeechGateway::from_root(&open_root)?
                .open_transcription(PcmFormat::default())
                .await
                .map_err(anyhow::Error::from)
        })
        .await??;
        let authority = Arc::new(Authority {
            root,
            token,
            scope,
            binding: binding.clone(),
            current,
        });
        let a = Arc::clone(&authority);
        let bound = tokio::task::spawn_blocking(move || {
            a.check()?;
            BoundTranscription::bind(&a.root, &a.token, binding, stream)
        })
        .await??;
        let id = bound.stream_id().to_owned();
        let (commands, rx) = mpsc::channel(8);
        let session = Arc::new(Session {
            authority: Arc::clone(&authority),
            commands,
            cancel: Arc::new(Notify::new()),
            canceled: Arc::new(AtomicBool::new(false)),
            snapshot: Arc::new(Mutex::new(Snapshot::default())),
            commit_gate: Arc::new(Mutex::new(())),
            request_id: r.request_id.clone().unwrap(),
            opened: Instant::now(),
            task: Mutex::new(None),
        });
        let task = tokio::spawn(run(
            bound,
            rx,
            Arc::clone(&authority),
            Arc::clone(&session.cancel),
            Arc::clone(&session.canceled),
            Arc::clone(&session.snapshot),
            Arc::clone(&session.commit_gate),
            permit,
        ));
        *session.task.lock().unwrap() = Some(task);
        registry
            .sessions
            .lock()
            .unwrap()
            .insert(id.clone(), (peer, Arc::clone(&session)));
        return Ok(GuardedAuxiliaryResponse {
            result: json!({"streamId":id, "state":"open", "events":[], "receipt":null, "error":null, "requestId":r.request_id}),
            publication: authority,
        });
    }
    let session = registry.lookup(&peer, &token, &r)?;
    let a = Arc::clone(&session.authority);
    tokio::task::spawn_blocking(move || a.check()).await??;
    let id = r.stream_id.as_ref().unwrap();
    if r.op == "cancel" {
        let owned = Arc::clone(&session);
        tokio::task::spawn_blocking(move || owned.cancel()).await?;
    }
    if r.op == "finish" && session.snapshot.lock().unwrap().committed.is_some() {
        let after = session.snapshot.lock().unwrap().event_sequence;
        return Ok(GuardedAuxiliaryResponse {
            result: session.snapshot(id, after)?,
            publication: Arc::clone(&session.authority) as Arc<dyn WebRTCPublicationGuard>,
        });
    }
    if r.op == "write" || r.op == "finish" {
        ensure!(!session.canceled.load(Ordering::SeqCst), "speech canceled");
        let (tx, rx) = oneshot::channel();
        let cmd = if r.op == "write" {
            let pcm = STANDARD.decode(r.pcm_base64.as_ref().unwrap())?;
            ensure!(
                !pcm.is_empty() && pcm.len() % 2 == 0 && pcm.len() <= MAX_PCM,
                "invalid speech request"
            );
            Command::Write(r.sequence.unwrap(), pcm, tx)
        } else {
            Command::Finish(tx)
        };
        session
            .commands
            .try_send(cmd)
            .map_err(|_| anyhow::anyhow!("speech backpressure"))?;
        tokio::time::timeout(Duration::from_secs(2), rx)
            .await??
            .map_err(anyhow::Error::msg)?;
    }
    let after = r
        .after_sequence
        .unwrap_or_else(|| session.snapshot.lock().unwrap().event_sequence);
    Ok(GuardedAuxiliaryResponse {
        result: session.snapshot(id, after)?,
        publication: Arc::clone(&session.authority) as Arc<dyn WebRTCPublicationGuard>,
    })
}

#[allow(clippy::too_many_arguments)]
async fn run(
    mut bound: BoundTranscription,
    mut commands: mpsc::Receiver<Command>,
    authority: Arc<Authority>,
    cancel: Arc<Notify>,
    canceled: Arc<AtomicBool>,
    snapshot: Arc<Mutex<Snapshot>>,
    commit_gate: Arc<Mutex<()>>,
    _permit: tokio::sync::OwnedSemaphorePermit,
) {
    let started = Instant::now();
    let mut touched = Instant::now();
    let mut bytes = 0;
    let mut sequence = 0;
    let mut previous: Vec<u8> = Vec::new();
    let mut finishing = false;
    let mut finish_at = None;
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    let outcome: Result<(), &'static str> = loop {
        tokio::select! {
            biased;
            _ = cancel.notified() => break Err("canceled"),
            _ = tick.tick() => {
                if canceled.load(Ordering::SeqCst) || !(authority.current)() { break Err("retired"); }
                if started.elapsed() > Duration::from_secs(40) || touched.elapsed() > Duration::from_secs(5) && !finishing
                    || finish_at.is_some_and(|at: Instant| at.elapsed() > Duration::from_secs(15)) { break Err("timeout"); }
                let a = Arc::clone(&authority);
                if !matches!(tokio::task::spawn_blocking(move || a.check()).await, Ok(Ok(()))) { break Err("retired"); }
            },
            cmd = commands.recv() => {
                touched = Instant::now();
                match cmd {
                    Some(Command::Write(next, pcm, ack)) => {
                        if !finishing && next == sequence && pcm == previous { let _ = ack.send(Ok(())); continue; }
                        if finishing || next != sequence + 1 || bytes + pcm.len() > MAX_AUDIO { let _ = ack.send(Err("invalid_sequence_or_audio")); break Err("invalid_sequence_or_audio"); }
                        if let Err(e) = bound.stream_mut().append_pcm(&pcm) { let _ = ack.send(Err("backpressure")); let _ = e; break Err("backpressure"); }
                        sequence = next; bytes += pcm.len(); previous = pcm; let _ = ack.send(Ok(()));
                    },
                    Some(Command::Finish(ack)) => {
                        if !finishing {
                            if bytes == 0 || bound.stream_mut().finish_audio().is_err() { let _ = ack.send(Err("invalid_finish")); break Err("invalid_finish"); }
                            finishing = true; finish_at = Some(Instant::now()); snapshot.lock().unwrap().finished = true;
                        }
                        let _ = ack.send(Ok(()));
                    },
                    None => break Err("canceled"),
                }
            },
            event = bound.stream_mut().next_verified_event() => match event {
                Some(Ok(VerifiedTranscriptEvent::Partial {sequence: producer_sequence, text, ..})) => {
                    if text.len() > 8192 { break Err("invalid_response"); }
                    let mut s = snapshot.lock().unwrap(); s.event_sequence += 1;
                    let cursor = s.event_sequence;
                    s.events.push_back(json!({"sequence":cursor, "producerSequence":producer_sequence, "text":text}));
                    while s.events.len() > MAX_EVENTS || s.events.iter().map(|e| e["text"].as_str().map_or(0, str::len)).sum::<usize>() > 65536 { s.events.pop_front(); }
                },
                Some(Ok(VerifiedTranscriptEvent::Final(final_value))) => {
                    if !finishing { break Err("invalid_response"); }
                    let a = Arc::clone(&authority); let stop = Arc::clone(&canceled); let gate = Arc::clone(&commit_gate);
                    let committed = tokio::task::spawn_blocking(move || {
                        let _fence = gate.lock().map_err(|_| anyhow::anyhow!("speech commit fence unavailable"))?;
                        ensure!(!stop.load(Ordering::SeqCst), "speech canceled"); a.check()?;
                        bound.stage_final(&a.root, &a.token, final_value)?;
                        let result = jour_fixe_speech::submit_staged_final(&a.root, &a.token, &a.binding, bound.stream_id())?;
                        ensure!(result["status"] == "completed", "speech domain commit failed");
                        let revision = jour_fixe_speech::committed_revision(&a.root, &a.token, &a.binding, bound.stream_id())?;
                        Ok::<_, anyhow::Error>(revision)
                    }).await;
                    match committed { Ok(Ok(revision)) => { snapshot.lock().unwrap().committed = Some(json!({"handle":Uuid::new_v4().to_string(), "meetingRevision":revision})); return; },
                        _ => { snapshot.lock().unwrap().error = Some("commit_failed"); return; } }
                },
                Some(Err(_)) => break Err("gateway_failed"),
                None => break Err("missing_final"),
            },
        }
    };
    snapshot.lock().unwrap().error = outcome.err();
    bound.cancel().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(op: &str) -> Value {
        json!({"op":op,"scope":{"instanceId":"instance","projectId":"project","meetingId":"meeting","deckRevision":1}})
    }
    #[test]
    fn wire_cannot_supply_final_text_or_receipt() {
        for field in [
            "text",
            "receipt",
            "speaker",
            "capabilityToken",
            "model",
            "voice",
        ] {
            let mut v = request("open");
            v["requestId"] = json!(Uuid::new_v4());
            v[field] = json!("forged");
            assert!(Request::parse(vec![v]).is_err());
        }
    }
    #[test]
    fn strict_operation_and_audio_bounds() {
        let mut v = request("write");
        v["streamId"] = json!(Uuid::new_v4());
        v["sequence"] = json!(1);
        v["pcmBase64"] = json!(STANDARD.encode([0; 3200]));
        assert!(Request::parse(vec![v.clone()]).is_ok());
        v["pcmBase64"] = json!(STANDARD.encode([0; 3202]));
        assert!(Request::parse(vec![v]).is_err());
    }
    #[test]
    fn scope_is_not_mutable_or_optional() {
        let mut v = request("open");
        v["requestId"] = json!(Uuid::new_v4());
        assert!(Request::parse(vec![v.clone()]).is_ok());
        v["scope"]["deckRevision"] = json!(0);
        assert!(Request::parse(vec![v]).is_err());
    }
}
