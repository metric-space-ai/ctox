//! E26/E27.3: one opening command owns a native speech session. This is
//! process-local binding metadata, not another command ledger or polling loop.
use super::super::webrtc_types::WebRTCPublicationGuard;
use super::v4_native_commands::{NativeCommandAuthority, PendingNativeCompletion};
use super::*;
use ctox_store::change_log::subscription::write_contract::{
    routes::{self, NativeCommand},
    CommandError,
};
use std::time::{Duration, Instant};
use tokio::sync::watch;

#[derive(Clone)]
pub struct NativeSessionFeed {
    changes: watch::Receiver<()>,
    snapshot: Arc<dyn Fn(u64) -> Result<Value, CommandError> + Send + Sync>,
}
impl NativeSessionFeed {
    pub fn new(
        changes: watch::Receiver<()>,
        snapshot: Arc<dyn Fn(u64) -> Result<Value, CommandError> + Send + Sync>,
    ) -> Self {
        Self { changes, snapshot }
    }
    pub fn current(&self, after: u64) -> Result<Value, CommandError> {
        (self.snapshot)(after)
    }
    async fn terminal(&mut self) -> Result<Value, CommandError> {
        loop {
            let value = self.current(0)?;
            if terminal(&value) {
                return Ok(value);
            }
            self.changes.changed().await.map_err(|_| closed())?;
        }
    }
}
pub type NativeSessionObserver<P> =
    Arc<dyn Fn(P, String, String) -> Result<NativeSessionFeed, String> + Send + Sync>;
#[derive(Clone)]
pub(super) struct NativeSessionBinding<P> {
    peer: P,
    capability: String,
    command: NativeCommand,
    authority: Arc<dyn NativeCommandAuthority>,
    publication: Arc<dyn WebRTCPublicationGuard>,
    peer_identity: String,
    stream_id: String,
    feed: NativeSessionFeed,
    created: Instant,
}
pub(super) fn closed() -> CommandError {
    routes::error("not_found", "SPEECH_STREAM_CLOSED")
}
fn terminal(value: &Value) -> bool {
    matches!(
        value["state"].as_str(),
        Some("finished" | "committed" | "canceled" | "cancelled" | "failed")
    )
}
fn opening_value(command: &NativeCommand, mut value: Value) -> Value {
    if command.kind == "ctox.workjet.speech.dictation.v1" {
        value["commandId"] = Value::String(command.command_id.clone());
        value["op"] = Value::String("open".into());
    } else {
        value["requestId"] = Value::String(command.command_id.clone());
    }
    value
}
impl<P> NativeSessionBinding<P> {
    fn update(&self, value: Value) -> PendingNativeCompletion {
        PendingNativeCompletion {
            command: self.command.clone(),
            result: Ok(opening_value(&self.command, value)),
            publication: self.publication.clone(),
            authority: self.authority.clone(),
            peer_identity: self.peer_identity.clone(),
            capability: self.capability.clone(),
            linked: None,
            canonical_replay: true,
        }
    }
    fn check(&self) -> Result<(), CommandError> {
        self.authority
            .with_current(
                &self.command,
                &self.peer_identity,
                &self.capability,
                &mut || self.publication.with_current(&mut || Ok(())),
            )
            .map_err(|_| closed())
    }
}
/// The Core retains this stream with its actual opening-command fence. Each
/// next() waits for the real native producer's watch notification. No timer or
/// requestNative read loop is introduced. Publish returned values on P1 through
/// PendingNativeCompletion::publish in the existing writer.
pub struct NativeSessionStream {
    next: Box<dyn Fn(Value) -> PendingNativeCompletion + Send + Sync>,
    check: Box<dyn Fn() -> Result<(), CommandError> + Send + Sync>,
    feed: NativeSessionFeed,
    after: u64,
    done: bool,
}
impl NativeSessionStream {
    pub async fn next(&mut self) -> Result<Option<PendingNativeCompletion>, CommandError> {
        if self.done {
            return Ok(None);
        }
        (self.check)()?;
        self.feed.changes.changed().await.map_err(|_| closed())?;
        (self.check)()?;
        let mut value = self.feed.current(self.after)?;
        if terminal(&value) {
            value = self.feed.current(0)?;
        }
        let mut previous = self.after;
        for event in value["events"].as_array().into_iter().flatten() {
            let sequence = event["sequence"]
                .as_u64()
                .ok_or_else(|| routes::error("invalid", "speech_event_sequence"))?;
            if sequence <= previous {
                return Err(routes::error("invalid", "speech_event_sequence"));
            }
            previous = sequence;
        }
        self.after = previous;
        self.done = terminal(&value);
        Ok(Some((self.next)(value)))
    }
}
pub(super) struct SessionCall<P> {
    pub params: Value,
    pub parent: Option<NativeSessionBinding<P>>,
    pub cached: Option<Value>,
}
impl<H: WebRTCConnectionHandler + 'static> RxWebRTCReplicationPool<H> {
    /// Only the native handler owner registers this in-process observer. It is
    /// not a new public wire method; lookup still binds the original peer/token.
    pub fn register_native_session_observer(
        &self,
        method: &str,
        observer: NativeSessionObserver<H::Peer>,
    ) -> crate::rx_error::RxResult<()> {
        if !routes::resolve(method).is_ok_and(|r| r.streaming) {
            return Err(new_rx_error("E25_SESSION_ROUTE", None));
        }
        self.native_session_observers
            .lock()
            .insert(method.into(), observer);
        Ok(())
    }
    pub fn watch_native_session(
        &self,
        peer: &H::Peer,
        command: &NativeCommand,
    ) -> Result<NativeSessionStream, CommandError> {
        let binding = self
            .native_command_sessions
            .lock()
            .get(&command.command_id)
            .cloned()
            .ok_or_else(closed)?;
        if &binding.peer != peer
            || binding.command.actor_id != command.actor_id
            || binding.command.app_id != command.app_id
            || binding.command.kind != command.kind
            || binding.created.elapsed() >= Duration::from_secs(100)
        {
            return Err(closed());
        }
        binding.check()?;
        let mut feed = binding.feed.clone();
        feed.changes.mark_changed();
        let check = binding.clone();
        Ok(NativeSessionStream {
            next: Box::new(move |v| binding.update(v)),
            check: Box::new(move || check.check()),
            feed,
            after: 0,
            done: false,
        })
    }
    pub(super) fn session_call(
        &self,
        peer: &H::Peer,
        capability: &str,
        command: &NativeCommand,
    ) -> Result<SessionCall<H::Peer>, CommandError> {
        let mut params = command.payload.clone();
        if !routes::resolve(&command.kind)?.streaming || command.payload.get("op").is_none() {
            return Ok(SessionCall {
                params,
                parent: None,
                cached: None,
            });
        }
        self.native_command_sessions
            .lock()
            .retain(|_, b| b.created.elapsed() < Duration::from_secs(100));
        let op = command.payload["op"]
            .as_str()
            .ok_or_else(|| routes::error("invalid", "speech_operation"))?;
        if !["open", "write", "read", "finish", "cancel"].contains(&op) {
            return Err(routes::error("invalid", "speech_operation"));
        }
        let object = params
            .as_object_mut()
            .ok_or_else(|| routes::error("invalid", "speech_payload"))?;
        object.remove("commandId");
        object.remove("requestId");
        if op == "open" {
            if object.contains_key("streamId") || object.contains_key("session_command_id") {
                return Err(routes::error("invalid", "speech_open_session"));
            }
            if !self
                .native_session_observers
                .lock()
                .contains_key(&command.kind)
            {
                return Err(routes::error("unavailable", "speech_event_feed_missing"));
            }
            object.insert(
                if command.kind == "ctox.workjet.speech.dictation.v1" {
                    "commandId"
                } else {
                    "requestId"
                }
                .into(),
                Value::String(command.command_id.clone()),
            );
            return Ok(SessionCall {
                params,
                parent: None,
                cached: None,
            });
        }
        let id = object
            .remove("session_command_id")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(closed)?;
        let parent = self
            .native_command_sessions
            .lock()
            .get(&id)
            .cloned()
            .ok_or_else(closed)?;
        if parent.peer != *peer
            || parent.capability != capability
            || parent.command.app_id != command.app_id
            || parent.command.actor_id != command.actor_id
            || parent.command.kind != command.kind
        {
            return Err(closed());
        }
        parent.check()?;
        if object
            .get("scope")
            .is_some_and(|v| v != &parent.command.payload["scope"])
            || object.contains_key("streamId")
        {
            return Err(closed());
        }
        object.insert("scope".into(), parent.command.payload["scope"].clone());
        object.insert("streamId".into(), Value::String(parent.stream_id.clone()));
        if command.kind == "ctox.workjet.speech.dictation.v1" {
            object.insert(
                "commandId".into(),
                Value::String(command.command_id.clone()),
            );
        }
        let cached = if op == "read" {
            Some(
                parent
                    .feed
                    .current(command.payload["afterSequence"].as_u64().unwrap_or(0))?,
            )
        } else {
            None
        };
        Ok(SessionCall {
            params,
            parent: Some(parent),
            cached,
        })
    }
    pub(super) async fn attach_session_result(
        &self,
        peer: H::Peer,
        mut call: SessionCall<H::Peer>,
        pending: &mut PendingNativeCompletion,
        remaining: Duration,
    ) -> Result<(), CommandError> {
        let Ok(value) = pending.result.clone() else {
            return Ok(());
        };
        if pending.command.payload["op"] == "open"
            && routes::resolve(&pending.command.kind)?.streaming
        {
            let id = value["streamId"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 180)
                .ok_or_else(|| routes::error("invalid", "speech_stream_id"))?
                .to_owned();
            let observer = self
                .native_session_observers
                .lock()
                .get(&pending.command.kind)
                .cloned()
                .ok_or_else(|| routes::error("unavailable", "speech_event_feed_missing"))?;
            let feed = observer(peer.clone(), pending.capability.clone(), id.clone())
                .map_err(|_| closed())?;
            pending.result = Ok(opening_value(&pending.command, feed.current(0)?));
            let binding = NativeSessionBinding {
                peer,
                capability: pending.capability.clone(),
                command: pending.command.clone(),
                authority: pending.authority.clone(),
                publication: pending.publication.clone(),
                peer_identity: pending.peer_identity.clone(),
                stream_id: id,
                feed,
                created: Instant::now(),
            };
            let mut sessions = self.native_command_sessions.lock();
            if sessions.len() >= 32 && !sessions.contains_key(&pending.command.command_id) {
                return Err(routes::error("quota", "speech_sessions"));
            }
            if let Some(old) = sessions.get(&pending.command.command_id) {
                if old.peer != binding.peer
                    || old.command.actor_id != binding.command.actor_id
                    || old.command.app_id != binding.command.app_id
                    || old.stream_id != binding.stream_id
                {
                    return Err(closed());
                }
            }
            sessions.insert(pending.command.command_id.clone(), binding);
        }
        if let Some(parent) = call.parent.as_mut() {
            let value = if pending.command.payload["op"] == "finish" && !terminal(&value) {
                tokio::time::timeout(remaining, parent.feed.terminal())
                    .await
                    .map_err(|_| routes::error("expired", "speech_finish_timeout"))??
            } else {
                value.clone()
            };
            let canonical = if terminal(&value) {
                parent.feed.current(0)?
            } else {
                value.clone()
            };
            let mut own = value;
            if pending.command.kind == "ctox.workjet.speech.dictation.v1" {
                own["commandId"] = Value::String(pending.command.command_id.clone());
                own["op"] = pending.command.payload["op"].clone();
            }
            pending.result = Ok(own);
            pending.linked = Some(Box::new(parent.update(canonical)));
        }
        Ok(())
    }
}
