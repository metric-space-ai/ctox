// Origin: CTOX
// License: AGPL-3.0-only
//! Bounded adapter RPC on the already-running native control pool.
//! The workload owner supplies current grants, signatures and reply verification.
#[cfg(test)]
#[path = "control_channel_tests.rs"]
mod tests;
use ctox_sync::native::NativePool;
use futures_util::StreamExt;
use rxdb::plugins::replication_webrtc::{
    index_mod::{GuardedAuxiliaryRequestHandler, GuardedAuxiliaryResponse},
    WebRTCConnectionHandler, WebRTCMessage, WebRTCPublicationGuard, WebRTCRsConnection,
    WebRTCWireFrame,
};
use rxdb::rx_error::{new_rx_error, RxResult};
use serde_json::Value;
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::Duration,
};
use tokio::sync::{watch, Semaphore};

const MAX_REQUEST: usize = 32 * 1024;
const MAX_RESPONSE: usize = 128 * 1024;
const MAX_PENDING: usize = 32;
type Pool = rxdb::plugins::replication_webrtc::RxWebRTCReplicationPool<
    rxdb::plugins::replication_webrtc::WebRTCRsConnectionHandler,
>;

struct State {
    alive: Mutex<bool>,
    retired: watch::Sender<bool>,
    pool: Weak<Pool>,
    runtime: tokio::runtime::Handle,
    pending: Arc<Semaphore>,
}
type Registry = HashMap<PathBuf, Weak<State>>;
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Retained by the actual runtime. Handles never prolong host/pool authority.
pub(super) struct Owner {
    root: PathBuf,
    state: Arc<State>,
}
#[derive(Clone)]
pub(crate) struct NativeControlChannel {
    state: Weak<State>,
}
/// The workload must authenticate the reply against this independently pinned
/// signing identity and its own fresh request nonce. A route is never identity.
pub(crate) trait NativeControlReplyVerifier: Send + Sync {
    fn verify(&self, expected_identity: &str, reply: &Value) -> io::Result<Value>;
}
impl Owner {
    pub(super) fn start(root: &Path, pool: &NativePool) -> io::Result<Self> {
        let root = std::fs::canonicalize(root)?;
        let (retired, _) = watch::channel(false);
        let state = Arc::new(State {
            alive: Mutex::new(true),
            retired,
            pool: Arc::downgrade(pool),
            runtime: tokio::runtime::Handle::try_current().map_err(io::Error::other)?,
            pending: Arc::new(Semaphore::new(MAX_PENDING)),
        });
        let mut entries = registry().lock().map_err(|_| unavailable())?;
        if entries.get(&root).and_then(Weak::upgrade).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "native control host already registered",
            ));
        }
        entries.insert(root.clone(), Arc::downgrade(&state));
        Ok(Self { root, state })
    }
    pub(super) fn channel(&self) -> NativeControlChannel {
        NativeControlChannel {
            state: Arc::downgrade(&self.state),
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.channel().retire();
        if let Ok(mut entries) = registry().lock() {
            if entries
                .get(&self.root)
                .is_some_and(|entry| Weak::ptr_eq(entry, &Arc::downgrade(&self.state)))
            {
                entries.remove(&self.root);
            }
        }
    }
}
pub(crate) fn native_control_channel(root: &Path) -> io::Result<NativeControlChannel> {
    let root = std::fs::canonicalize(root)?;
    let state = registry()
        .lock()
        .map_err(|_| unavailable())?
        .get(&root)
        .cloned()
        .ok_or_else(unavailable)?;
    let handle = NativeControlChannel { state };
    handle.current()?;
    Ok(handle)
}
fn unavailable() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotConnected,
        "native control host unavailable",
    )
}
fn denied() -> rxdb::rx_error::RxError {
    new_rx_error("CTOX_NATIVE_CONTROL_RETIRED", None)
}
fn valid_method(method: &str) -> bool {
    (method == "ctox.native.speech.v1" || method.starts_with("ctox.sync.workload."))
        && method.len() <= 128
        && method
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
fn valid_identity(identity: &str) -> bool {
    identity.strip_prefix("ed25519:").is_some_and(|key| {
        key.len() == 64
            && key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Workload authority precedes lifecycle/pool locks, matching receiver guards.
/// Every physical send poll reacquires these guards; Pending holds none.
struct Publication {
    state: Weak<State>,
    // Receiver responses are already wrapped by RxDB AuxiliaryPublicationGuard.
    // Reentering its pool fence from this inner guard would deadlock.
    peer: Option<WebRTCRsConnection>,
    workload: Arc<dyn WebRTCPublicationGuard>,
}
impl WebRTCPublicationGuard for Publication {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        with_once(self.workload.as_ref(), &mut || {
            let state = self.state.upgrade().ok_or_else(denied)?;
            let alive = state.alive.lock().map_err(|_| denied())?;
            if !*alive {
                return Err(denied());
            }
            if let Some(peer) = &self.peer {
                let pool = state.pool.upgrade().ok_or_else(denied)?;
                pool.with_current_native_control_peer(peer, &mut *publish)?
            } else {
                publish()
            }
        })
    }
}
fn with_once(
    guard: &dyn WebRTCPublicationGuard,
    publish: &mut dyn FnMut() -> RxResult<()>,
) -> RxResult<()> {
    let mut called = 0usize;
    let mut publication_failed = false;
    guard.with_current(&mut || {
        called = called.saturating_add(1);
        if called != 1 {
            return Err(denied());
        }
        let result = publish();
        publication_failed |= result.is_err();
        result
    })?;
    if called != 1 || publication_failed {
        return Err(denied());
    }
    Ok(())
}
struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);
impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
impl NativeControlChannel {
    fn current(&self) -> io::Result<Arc<State>> {
        let state = self.state.upgrade().ok_or_else(unavailable)?;
        if !*state.alive.lock().map_err(|_| unavailable())? {
            return Err(unavailable());
        }
        Ok(state)
    }
    pub(super) fn retire(&self) {
        if let Some(state) = self.state.upgrade() {
            if let Ok(mut alive) = state.alive.lock() {
                *alive = false;
                state.retired.send_replace(true);
                state.pending.close();
            }
        }
    }

    /// No discovery, new connection, account provisioning or implicit grant.
    /// The adapter supplies a signed request, current source-publication guard
    /// and mandatory pinned reply verifier. Cancellation drops the exact call.
    pub(crate) async fn request(
        &self,
        route: &str,
        expected_identity: &str,
        method: &str,
        envelope: Value,
        deadline: Duration,
        publication: Arc<dyn WebRTCPublicationGuard>,
        verifier: Arc<dyn NativeControlReplyVerifier>,
    ) -> io::Result<Value> {
        if !valid_method(method)
            || !valid_identity(expected_identity)
            || route.is_empty()
            || route.len() > 256
            || route.trim() != route
            || route.chars().any(char::is_control)
            || deadline.is_zero()
            || deadline > Duration::from_secs(30)
            || serde_json::to_vec(&envelope)
                .map_err(io::Error::other)?
                .len()
                > MAX_REQUEST
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid native workload request",
            ));
        }
        let state = self.current()?;
        let permit = state.pending.clone().try_acquire_owned().map_err(|_| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                "native workload request capacity",
            )
        })?;
        let pool = state.pool.upgrade().ok_or_else(unavailable)?;
        let peer = pool
            .connection_handler
            .connection_for_peer(route)
            .ok_or_else(unavailable)?;
        let guard: Arc<dyn WebRTCPublicationGuard> = Arc::new(Publication {
            state: self.state.clone(),
            peer: Some(peer.clone()),
            workload: publication,
        });
        guard
            .with_current(&mut || Ok(()))
            .map_err(|_| unavailable())?;
        let mut retired = state.retired.subscribe();
        if *retired.borrow() {
            return Err(unavailable());
        }
        let pin = expected_identity.to_owned();
        let method = method.to_owned();
        let task = state.runtime.spawn(async move {
            let _permit = permit;
            let handler = pool.connection_handler.clone();
            // Subscribe before send, including an immediate response/retirement.
            let mut answers = handler.response_stream();
            let mut disconnected = handler.disconnect_stream();
            let id = format!("{method}:{}", uuid::Uuid::new_v4());
            let exchange = async {
                handler.send_guarded(&peer, WebRTCWireFrame::Message(WebRTCMessage {
                    id: id.clone(), method, params: vec![envelope], collection: None,
                }), guard.clone()).await.map_err(|_| unavailable())?;
                loop {
                    tokio::select! {
                        answer = answers.next() => {
                            let answer = answer.ok_or_else(unavailable)?;
                            if answer.peer != peer || answer.response.id != id { continue; }
                            if answer.response.error.is_some()
                                || serde_json::to_vec(&answer.response.result).map_err(io::Error::other)?.len() > MAX_RESPONSE {
                                return Err(io::Error::other("native workload response rejected"));
                            }
                            let mut verified = None;
                            guard.with_current(&mut || {
                                verified = Some(verifier.verify(&pin, &answer.response.result));
                                Ok(())
                            }).map_err(|_| unavailable())?;
                            return verified.ok_or_else(unavailable)?;
                        }
                        gone = disconnected.next() => {
                            if gone.as_ref().is_none_or(|gone| gone == &peer) { return Err(unavailable()); }
                        }
                    }
                }
            };
            tokio::select! {
                biased;
                _ = retired.changed() => Err(unavailable()),
                result = tokio::time::timeout(deadline, exchange) =>
                    result.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "native workload request deadline"))?,
            }
        });
        let mut owned = AbortOnDrop(task);
        (&mut owned.0).await.map_err(|_| unavailable())?
    }

    /// Receiver callbacks must independently validate the signed sender,
    /// current workload grant and nonce, then return their publication guard.
    /// This wrapper adds host lifetime, exact peer and bounded response fences.
    pub(crate) fn register_handler(
        &self,
        method: &str,
        handler: GuardedAuxiliaryRequestHandler<WebRTCRsConnection>,
    ) -> io::Result<()> {
        if !valid_method(method) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid native workload method",
            ));
        }
        let state = self.current()?;
        let pool = state.pool.upgrade().ok_or_else(unavailable)?;
        let weak = self.state.clone();
        let wrapped: GuardedAuxiliaryRequestHandler<WebRTCRsConnection> = Arc::new(
            move |peer, token, params| {
                let weak = weak.clone();
                let handler = handler.clone();
                Box::pin(async move {
                    let state = weak.upgrade().ok_or("native workload host retired")?;
                    let pool = state.pool.upgrade().ok_or("native workload pool retired")?;
                    if params.len() != 1
                        || serde_json::to_vec(&params)
                            .map_err(|_| "native workload request invalid")?
                            .len()
                            > MAX_REQUEST + 2
                    {
                        return Err("native workload request too large".into());
                    }
                    pool.with_current_native_control_peer(&peer, || ())
                        .map_err(|_| "native workload peer retired")?;
                    let mut retired = state.retired.subscribe();
                    if *retired.borrow() {
                        return Err("native workload host retired".into());
                    }
                    let result = tokio::select! {
                        biased;
                        _ = retired.changed() => return Err("native workload host retired".into()),
                        result = tokio::time::timeout(Duration::from_secs(30), handler(peer.clone(), token, params)) =>
                            result.map_err(|_| "native workload handler deadline")??,
                    };
                    if serde_json::to_vec(&result.result)
                        .map_err(|_| "native workload response invalid")?
                        .len()
                        > MAX_RESPONSE
                    {
                        return Err("native workload response too large".into());
                    }
                    Ok(GuardedAuxiliaryResponse {
                        result: result.result,
                        publication: Arc::new(Publication {
                            state: weak,
                            peer: None,
                            workload: result.publication,
                        }),
                    })
                })
            },
        );
        pool.register_guarded_native_control_handler(method, wrapped)
            .map_err(|_| io::Error::other("native workload handler registration rejected"))
    }
}
