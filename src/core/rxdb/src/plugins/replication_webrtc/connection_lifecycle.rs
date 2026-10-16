//! One bounded V4 lifecycle task per peer. No collection tasks, polling, new
//! runtime, signaling offers or fallback to the V3 collection handshake.
use super::hello::{Hello, HelloFrame, HelloOk, HelloPolicy, HelloReject, RejectReason, WireSeq};
use futures::{future::BoxFuture, FutureExt};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{timeout_at, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connecting,
    Authenticating,
    Negotiated,
    Live,
    Draining,
    Closed,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConnectionSnapshot {
    pub peer_id: String,
    pub state: ConnectionState,
    pub rejection: Option<HelloReject>,
}

/// Produced by the host only after current identity, nonce/signature and rights
/// checks. Never Debug: issued scope tokens are transport data, not diagnostics.
pub struct AuthorizedSession {
    pub instance_id: String,
    pub device_id: String,
    pub epoch: String,
    pub head_seq: WireSeq,
    pub scope_tokens: Vec<String>,
    pub rights_ver: WireSeq,
}
/// The host must verify the signature over this connection's fresh nonce,
/// resolve current device/user/role authority and issue only authorized scopes.
/// No verifier is installed implicitly; a callback error is always redacted.
pub type AuthenticatePeer = Arc<
    dyn Fn(String, Hello) -> BoxFuture<'static, Result<AuthorizedSession, HelloReject>>
        + Send
        + Sync,
>;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum LifecycleError {
    #[error("a lifecycle task already owns this peer")]
    PeerAlreadyOwned,
    #[error("invalid peer, challenge or handshake deadline")]
    InvalidConfiguration,
    #[error("peer lifecycle is closed")]
    Closed,
    #[error("event is not valid in the current connection state")]
    InvalidTransition,
}
enum Command {
    TransportConnected(oneshot::Sender<Result<(), LifecycleError>>),
    Hello(Hello, oneshot::Sender<Result<HelloFrame, LifecycleError>>),
    HelloOkSent(oneshot::Sender<Result<(), LifecycleError>>),
    Reject(
        HelloReject,
        oneshot::Sender<Result<HelloFrame, LifecycleError>>,
    ),
}
#[derive(Clone)]
pub struct PeerLifecycle {
    commands: mpsc::Sender<Command>,
    close: watch::Sender<bool>,
    snapshot: watch::Receiver<ConnectionSnapshot>,
    finished: watch::Receiver<bool>,
}
impl PeerLifecycle {
    pub fn snapshot(&self) -> ConnectionSnapshot {
        self.snapshot.borrow().clone()
    }
    /// Watch state/reason for the host state zone; never includes proof/tokens.
    pub fn subscribe(&self) -> watch::Receiver<ConnectionSnapshot> {
        self.snapshot.clone()
    }
    async fn request<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<T>) -> Command,
    ) -> Result<T, LifecycleError> {
        let (tx, rx) = oneshot::channel();
        // Await capacity: no try_send and no silent loss.
        self.commands
            .send(make(tx))
            .await
            .map_err(|_| LifecycleError::Closed)?;
        rx.await.map_err(|_| LifecycleError::Closed)
    }
    pub async fn transport_connected(&self) -> Result<(), LifecycleError> {
        self.request(Command::TransportConnected).await?
    }
    pub async fn receive_hello(&self, hello: Hello) -> Result<HelloFrame, LifecycleError> {
        self.request(|reply| Command::Hello(hello, reply)).await?
    }
    /// Entry seam for a responder's first control-lane message. Malformed or
    /// out-of-order frames also produce an explicit wire/state rejection.
    pub async fn receive_frame(&self, bytes: &[u8]) -> Result<HelloFrame, LifecycleError> {
        match HelloFrame::decode(bytes) {
            Ok(HelloFrame::Hello(hello)) => self.receive_hello(hello).await,
            result => {
                let error = result.err().unwrap_or_else(|| {
                    HelloReject::new(RejectReason::UnexpectedMessage, "hello first")
                });
                self.request(|reply| Command::Reject(error, reply)).await?
            }
        }
    }
    /// Call only after the selected control sender confirms SCTP handoff of
    /// hello_ok; a negotiated session must not accept application traffic early.
    pub async fn hello_ok_sent(&self) -> Result<(), LifecycleError> {
        self.request(Command::HelloOkSent).await?
    }
    /// Cancels a pending authenticator too; no waiting for its I/O timeout.
    pub async fn drain(&self) {
        self.close.send_replace(true);
        let mut finished = self.finished.clone();
        let _ = finished.wait_for(|done| *done).await;
    }
}
struct Slot {
    generation: u64,
    handle: PeerLifecycle,
}
struct RegistryInner {
    peers: Mutex<HashMap<String, Slot>>,
    next_generation: std::sync::atomic::AtomicU64,
    closed: std::sync::atomic::AtomicBool,
}
impl Drop for RegistryInner {
    fn drop(&mut self) {
        for slot in self.peers.get_mut().values() {
            slot.handle.close.send_replace(true);
        }
    }
}
#[derive(Clone)]
pub struct PeerLifecycleRegistry(Arc<RegistryInner>);
impl Default for PeerLifecycleRegistry {
    fn default() -> Self {
        Self(Arc::new(RegistryInner {
            peers: Mutex::new(HashMap::new()),
            next_generation: std::sync::atomic::AtomicU64::new(1),
            closed: std::sync::atomic::AtomicBool::new(false),
        }))
    }
}
impl PeerLifecycleRegistry {
    pub fn peer_count(&self) -> usize {
        self.0.peers.lock().len()
    }
    pub fn start(
        &self,
        peer_id: String,
        challenge: String,
        policy: HelloPolicy,
        authenticate: AuthenticatePeer,
        handshake_timeout: Duration,
    ) -> Result<PeerLifecycle, LifecycleError> {
        self.start_inner(
            peer_id,
            Some(challenge),
            policy,
            authenticate,
            handshake_timeout,
        )
    }
    /// E50 browser-only responder. The host callback MUST verify and consume
    /// its full signed launch ticket and revalidate current user/scope authority.
    /// Native callers continue through start(), which always requires a challenge.
    pub fn start_browser(
        &self,
        peer_id: String,
        policy: HelloPolicy,
        admission: AuthenticatePeer,
        handshake_timeout: Duration,
    ) -> Result<PeerLifecycle, LifecycleError> {
        self.start_inner(peer_id, None, policy, admission, handshake_timeout)
    }
    fn start_inner(
        &self,
        peer_id: String,
        challenge: Option<String>,
        policy: HelloPolicy,
        authenticate: AuthenticatePeer,
        handshake_timeout: Duration,
    ) -> Result<PeerLifecycle, LifecycleError> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| LifecycleError::InvalidConfiguration)?;
        if peer_id.trim().is_empty()
            || peer_id.len() > 256
            || challenge.as_ref().is_some_and(|challenge| {
                challenge.len() != 43
                    || !challenge
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            })
            || handshake_timeout.is_zero()
            || handshake_timeout > Duration::from_secs(60)
        {
            return Err(LifecycleError::InvalidConfiguration);
        }
        let mut peers = self.0.peers.lock();
        if self.0.closed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(LifecycleError::Closed);
        }
        if peers.contains_key(&peer_id) {
            return Err(LifecycleError::PeerAlreadyOwned);
        }
        let generation = self
            .0
            .next_generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (commands, receiver) = mpsc::channel(16);
        let (close, cancel) = watch::channel(false);
        let initial = ConnectionSnapshot {
            peer_id: peer_id.clone(),
            state: ConnectionState::Connecting,
            rejection: None,
        };
        let (states, snapshot) = watch::channel(initial);
        let (completed, finished) = watch::channel(false);
        let handle = PeerLifecycle {
            commands,
            close,
            snapshot,
            finished,
        };
        peers.insert(
            peer_id.clone(),
            Slot {
                generation,
                handle: handle.clone(),
            },
        );
        let registry = Arc::downgrade(&self.0);
        runtime.spawn(async move {
            let failure_states = states.clone();
            let run = run_peer(
                peer_id.clone(),
                challenge,
                policy,
                authenticate,
                receiver,
                cancel,
                states,
                handshake_timeout,
            );
            if std::panic::AssertUnwindSafe(run)
                .catch_unwind()
                .await
                .is_err()
            {
                reject(
                    &failure_states,
                    HelloReject::new(RejectReason::AuthenticationFailed, "peer task failed"),
                );
                state(&failure_states, ConnectionState::Draining);
                state(&failure_states, ConnectionState::Closed);
            }
            retire(&registry, &peer_id, generation);
            completed.send_replace(true);
        });
        Ok(handle)
    }
    /// Terminal owner shutdown: no new peer may race the drain snapshot.
    pub async fn drain_all(&self) {
        let handles: Vec<_> = {
            let peers = self.0.peers.lock();
            self.0
                .closed
                .store(true, std::sync::atomic::Ordering::Release);
            peers.values().map(|s| s.handle.clone()).collect()
        };
        // Signal every peer before awaiting any (also cancels authority I/O).
        for handle in &handles {
            handle.close.send_replace(true);
        }
        for handle in handles {
            handle.drain().await;
        }
    }
}
fn retire(registry: &Weak<RegistryInner>, peer_id: &str, generation: u64) {
    if let Some(registry) = registry.upgrade() {
        let mut peers = registry.peers.lock();
        if peers
            .get(peer_id)
            .is_some_and(|slot| slot.generation == generation)
        {
            peers.remove(peer_id);
        }
    }
}
fn state(states: &watch::Sender<ConnectionSnapshot>, next: ConnectionState) {
    states.send_modify(|current| {
        tracing::info!(peer_id = %current.peer_id, from = ?current.state, to = ?next,
            "V4 peer lifecycle transition");
        current.state = next;
    });
}
fn reject(states: &watch::Sender<ConnectionSnapshot>, rejection: HelloReject) {
    states.send_modify(|current| {
        tracing::warn!(peer_id = %current.peer_id, reason = ?rejection.reason,
            "V4 hello rejected");
        current.rejection = Some(rejection);
    });
}

#[allow(clippy::too_many_arguments)]
async fn run_peer(
    peer_id: String,
    challenge: Option<String>,
    policy: HelloPolicy,
    authenticate: AuthenticatePeer,
    mut commands: mpsc::Receiver<Command>,
    mut cancel: watch::Receiver<bool>,
    states: watch::Sender<ConnectionSnapshot>,
    handshake_timeout: Duration,
) {
    let deadline = Instant::now() + handshake_timeout;
    loop {
        let awaiting_handshake = states.borrow().state != ConnectionState::Live;
        let command = tokio::select! {
            biased;
            _ = cancel.changed() => break,
            _ = tokio::time::sleep_until(deadline), if awaiting_handshake => {
                reject(&states, HelloReject::new(RejectReason::HandshakeTimeout, "hello before deadline"));
                break;
            }
            command = commands.recv() => match command { Some(command) => command, None => break },
        };
        match command {
            Command::Reject(error, reply) => {
                reject(&states, error.clone());
                let _ = reply.send(Ok(HelloFrame::HelloReject(error)));
                break;
            }
            Command::TransportConnected(reply) => {
                let result = if states.borrow().state == ConnectionState::Connecting {
                    state(&states, ConnectionState::Authenticating);
                    Ok(())
                } else {
                    Err(LifecycleError::InvalidTransition)
                };
                let _ = reply.send(result);
            }
            Command::Hello(hello, reply) => {
                if states.borrow().state != ConnectionState::Authenticating {
                    let _ = reply.send(Err(LifecycleError::InvalidTransition));
                    continue;
                }
                let result = async {
                    let selected = policy.negotiate(&hello)?;
                    if challenge
                        .as_ref()
                        .is_some_and(|challenge| !hello.device_proof.matches_challenge(challenge))
                    {
                        return Err(HelloReject::new(
                            RejectReason::DeviceProofInvalid,
                            "fresh connection device proof",
                        ));
                    }
                    let device_id = hello.device_proof.device_id.clone();
                    let authorized = authenticate(peer_id.clone(), hello).await.map_err(|_| {
                        HelloReject::new(
                            RejectReason::AuthenticationFailed,
                            "current device and scope authority",
                        )
                    })?;
                    if authorized.instance_id != policy.instance_id
                        || authorized.device_id != device_id
                        || authorized.epoch.trim().is_empty()
                        || authorized
                            .scope_tokens
                            .iter()
                            .any(|token| token.trim().is_empty())
                    {
                        return Err(HelloReject::new(
                            RejectReason::AuthenticationFailed,
                            "authority bound to peer and instance",
                        ));
                    }
                    Ok(HelloOk {
                        protocol_ver: super::hello::PROTOCOL_VERSION,
                        bootstrap_ver: selected.bootstrap_ver,
                        app_api_ver: selected.app_api_ver,
                        instance_id: authorized.instance_id,
                        capabilities: selected.capabilities,
                        transport: selected.transport,
                        epoch: authorized.epoch,
                        head_seq: authorized.head_seq,
                        scope_tokens: authorized.scope_tokens,
                        rights_ver: authorized.rights_ver,
                    })
                };
                let outcome = tokio::select! {
                    biased;
                    _ = cancel.changed() => { let _ = reply.send(Err(LifecycleError::Closed)); break; }
                    result = timeout_at(deadline, result) => result.unwrap_or_else(|_|
                        Err(HelloReject::new(RejectReason::HandshakeTimeout, "hello before deadline"))),
                };
                match outcome.and_then(|ok| {
                    HelloFrame::HelloOk(ok.clone()).encode()?;
                    Ok(ok)
                }) {
                    Ok(ok) => {
                        state(&states, ConnectionState::Negotiated);
                        if reply.send(Ok(HelloFrame::HelloOk(ok))).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        reject(&states, error.clone());
                        // Return the rejection to the control sender AND retain
                        // its safe reason in the state projection for the UI.
                        let _ = reply.send(Ok(HelloFrame::HelloReject(error)));
                        break;
                    }
                }
            }
            Command::HelloOkSent(reply) => {
                let result = if states.borrow().state == ConnectionState::Negotiated {
                    state(&states, ConnectionState::Live);
                    Ok(())
                } else {
                    Err(LifecycleError::InvalidTransition)
                };
                let _ = reply.send(result);
            }
        }
    }
    state(&states, ConnectionState::Draining);
    commands.close();
    state(&states, ConnectionState::Closed);
}

#[cfg(test)]
#[path = "connection_lifecycle_tests.rs"]
mod tests;
