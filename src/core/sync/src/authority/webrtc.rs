//! Execution control over the existing multiplexed CTOX Sync DataChannel.
//! Routing hints may change on reconnect; configured signing keys remain authority.

#[cfg(all(test, unix))]
mod routing_tests {
    use super::*;
    #[tokio::test]
    async fn unsigned_startup_route_is_not_a_proved_connection() -> io::Result<()> {
        let pool = RxWebRTCReplicationPool::new_multi(Vec::new(), WebRTCRsConnectionHandler::new());
        let mut identities = BTreeSet::new();
        for _ in 0..3 {
            let bytes = SigningIdentity::generate_pkcs8()?;
            identities.insert(SigningIdentity::from_pkcs8(&bytes)?.public_identity());
        }
        let pin = identities.first().unwrap().clone();
        let channel = WebRtcControlChannel::new(&pool, identities, Duration::from_secs(1))?;
        channel.set_route(&pin, "unsigned-route-hint".into())?;
        assert!(channel.current_peer(&pin)?.is_none());
        assert_eq!(
            channel.current_peer("unconfigured").err().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
        Ok(())
    }

    #[tokio::test]
    async fn workload_pins_wake_discovery_without_joining_raft() -> io::Result<()> {
        let pool = RxWebRTCReplicationPool::new_multi(Vec::new(), WebRTCRsConnectionHandler::new());
        let mut voters = BTreeSet::new();
        for _ in 0..3 {
            let bytes = SigningIdentity::generate_pkcs8()?;
            voters.insert(SigningIdentity::from_pkcs8(&bytes)?.public_identity());
        }
        let channel = WebRtcControlChannel::new(&pool, voters.clone(), Duration::from_secs(1))?;
        let worker = format!("ed25519:{:064x}", 1);
        channel.register_workload_pin(&worker)?;
        tokio::time::timeout(
            Duration::from_millis(100),
            channel.workload_routes_changed(),
        )
        .await
        .map_err(io::Error::other)?;
        assert!(
            channel.current_peer(&worker)?.is_none(),
            "a pin alone is no route proof"
        );
        assert_eq!(
            channel.allowed, voters,
            "workload enrollment cannot add a Raft member"
        );
        assert!(channel
            .set_route(&worker, "unsigned-worker".into())
            .is_err());
        assert!(
            channel.request(&worker, Value::Null).await.is_err(),
            "Raft transport cannot use a workload-only target"
        );
        assert!(!channel.routes.read().unwrap().contains_key(&worker));
        for n in 1..=64 {
            channel.register_workload_pin(&format!("ed25519:{n:064x}"))?;
        }
        channel.register_workload_pin(&worker)?;
        assert!(channel
            .register_workload_pin(&format!("ed25519:{:064x}", 65))
            .is_err());
        assert!(channel.register_workload_pin("unsigned-worker").is_err());
        assert_eq!(channel.workload_pins.read().unwrap().len(), 64);
        Ok(())
    }
}
use super::{
    auth::{self, ControlChannel, SigningIdentity},
    network::CONTROL_METHOD,
    node::AuthorityNode,
};
use async_trait::async_trait;
#[cfg(unix)]
use rxdb::plugins::replication_webrtc::SignalingClient;
use rxdb::plugins::replication_webrtc::{
    send_message_and_await_answer, RxWebRTCReplicationPool, WebRTCConnectionHandler, WebRTCMessage,
    WebRTCRsConnection, WebRTCRsConnectionHandler,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::{Arc, RwLock, Weak},
    time::Duration,
};

pub struct WebRtcControlChannel {
    pool: Weak<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>,
    allowed: BTreeSet<String>,
    routes: RwLock<BTreeMap<String, String>>,
    authenticated_routes: RwLock<BTreeMap<String, (String, WebRTCRsConnection)>>,
    workload_pins: RwLock<BTreeSet<String>>,
    workload_routes: RwLock<BTreeMap<String, (String, WebRTCRsConnection)>>,
    workload_changed: tokio::sync::Notify,
    deadline: Duration,
}
impl WebRtcControlChannel {
    pub fn new(
        pool: &Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>,
        allowed: BTreeSet<String>,
        deadline: Duration,
    ) -> io::Result<Self> {
        for identity in &allowed {
            auth::public_key(identity)?;
        }
        if allowed.len() != 3 || deadline.is_zero() || deadline > Duration::from_secs(30) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "control channel requires three configured keys and a bounded deadline",
            ));
        }
        Ok(Self {
            pool: Arc::downgrade(pool),
            allowed,
            routes: RwLock::new(BTreeMap::new()),
            authenticated_routes: RwLock::new(BTreeMap::new()),
            workload_pins: RwLock::new(BTreeSet::new()),
            workload_routes: RwLock::new(BTreeMap::new()),
            workload_changed: tokio::sync::Notify::new(),
            deadline,
        })
    }
    /// Discovery supplies a route only. SignedTransport verifies the endpoint key.
    pub fn set_route(&self, identity: &str, peer: String) -> io::Result<()> {
        if !self.allowed.contains(identity) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unconfigured control peer",
            ));
        }
        self.routes
            .write()
            .map_err(|_| io::Error::other("control route lock poisoned"))?
            .insert(identity.into(), peer);
        Ok(())
    }
    /// An explicit workload target pin permits only public identity discovery.
    /// It never changes the three Raft voters or grants execution authority.
    /// Pins live at most as long as this host and are bounded independently.
    pub fn register_workload_pin(&self, identity: &str) -> io::Result<()> {
        auth::public_key(identity)?;
        if self.allowed.contains(identity) {
            return Ok(());
        }
        let mut pins = self
            .workload_pins
            .write()
            .map_err(|_| io::Error::other("workload pin lock poisoned"))?;
        if pins.contains(identity) {
            return Ok(());
        }
        if pins.len() >= 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "workload discovery pin limit",
            ));
        }
        pins.insert(identity.into());
        drop(pins);
        self.workload_changed.notify_one();
        Ok(())
    }

    pub(crate) async fn workload_routes_changed(&self) {
        self.workload_changed.notified().await;
    }

    /// An already proved current connection, never an unsigned discovery hint.
    /// Consumers still need their workload grant and must retain this lifetime.
    pub fn current_peer(&self, identity: &str) -> io::Result<Option<(String, WebRTCRsConnection)>> {
        let voter = self.allowed.contains(identity);
        if !voter
            && !self
                .workload_pins
                .read()
                .map_err(|_| io::Error::other("workload pin lock poisoned"))?
                .contains(identity)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unconfigured control peer",
            ));
        }
        let bindings = if voter {
            &self.authenticated_routes
        } else {
            &self.workload_routes
        };
        let binding = bindings
            .read()
            .map_err(|_| io::Error::other("control route lock poisoned"))?
            .get(identity)
            .cloned();
        let Some((route, peer)) = binding else {
            return Ok(None);
        };
        let pool = self
            .pool
            .upgrade()
            .ok_or_else(|| io::Error::other("authority pool stopped"))?;
        if pool.connection_handler.connection_for_peer(&route).as_ref() != Some(&peer)
            || !pool.is_peer_ready_for_control(&peer)
        {
            return Ok(None);
        }
        Ok(Some((route, peer)))
    }

    /// Probe one admitted connection. Only a fresh proof from a configured key
    /// updates routing; scope, nonce and the signaling lifetime are bound.
    #[cfg(unix)]
    pub(crate) async fn discover_route(
        &self,
        key: &SigningIdentity,
        scope: &str,
        route: &str,
    ) -> io::Result<()> {
        let pool = self
            .pool
            .upgrade()
            .ok_or_else(|| io::Error::other("authority pool stopped"))?;
        let connection = pool
            .connection_handler
            .connection_for_peer(route)
            .ok_or_else(|| io::Error::other("authority route disconnected"))?;
        let workload_pins = self
            .workload_pins
            .read()
            .map_err(|_| io::Error::other("workload pin lock poisoned"))?
            .clone();
        let proof_pins = self.allowed.union(&workload_pins).cloned().collect();
        let probe = auth::route::RouteProbe::new(key, scope, route)?;
        let reply = self
            .request_route(route, auth::route::METHOD, probe.request())
            .await?;
        let identity = probe.verify(reply, &proof_pins)?;
        if pool.connection_handler.connection_for_peer(route).as_ref() != Some(&connection)
            || !pool.is_peer_ready_for_control(&connection)
        {
            return Err(io::Error::other("authority route changed during discovery"));
        }
        let bindings = if self.allowed.contains(&identity) {
            self.set_route(&identity, route.into())?;
            &self.authenticated_routes
        } else {
            &self.workload_routes
        };
        bindings
            .write()
            .map_err(|_| io::Error::other("control route lock poisoned"))?
            .insert(identity, (route.to_owned(), connection));
        Ok(())
    }

    async fn request_route(&self, route: &str, method: &str, envelope: Value) -> io::Result<Value> {
        let pool = self.pool.upgrade().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                "authority replication pool is stopped",
            )
        })?;
        // Configuration pins a routing hint, never a reusable connection handle.
        // Resolve it anew, then bind the entire request to that one lifetime.
        let peer = pool
            .connection_handler
            .connection_for_peer(route)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "control peer has no open WebRTC connection",
                )
            })?;
        if !pool.is_peer_ready_for_control(&peer) {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "authority peer has not completed room admission",
            ));
        }
        // A signed nonce also supplies the existing multiplexer's correlation ID.
        let nonce = envelope["body"]["nonce"]
            .as_str()
            .filter(|n| n.len() == 32 && n.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing control nonce"))?;
        let request = WebRTCMessage {
            id: format!("{method}:{nonce}"),
            method: method.into(),
            params: vec![envelope],
            collection: None,
        };
        let response = tokio::time::timeout(
            self.deadline,
            send_message_and_await_answer(pool.connection_handler.clone(), peer, request),
        )
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "authority WebRTC exchange timed out",
            )
        })?
        .map_err(|e| io::Error::other(e.to_string()))?;
        if let Some(error) = response.error {
            return Err(io::Error::other(error));
        }
        Ok(response.result)
    }
}
#[async_trait]
impl ControlChannel for WebRtcControlChannel {
    async fn request(&self, target_identity: &str, envelope: Value) -> io::Result<Value> {
        let route = self
            .routes
            .read()
            .map_err(|_| io::Error::other("control route lock poisoned"))?
            .get(target_identity)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "control peer has no live WebRTC route",
                )
            })?;
        self.request_route(&route, CONTROL_METHOD, envelope).await
    }
}

/// Public identity proof only. Room admission still applies; no collection,
/// member or execution permission is conferred by this method.
#[cfg(unix)]
pub(crate) fn register_route_receiver<A: super::client::ExecutionAuthority + 'static>(
    pool: &RxWebRTCReplicationPool<WebRTCRsConnectionHandler>,
    signaling: &Arc<SignalingClient>,
    identity: Arc<SigningIdentity>,
    scope: String,
    node: &Arc<A>,
) -> io::Result<()> {
    // Attachment constructors pin the key; this method only proves possession.
    if node.scope_id() != scope {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "route receiver does not match authority scope",
        ));
    }
    let node = Arc::downgrade(node);
    let signaling = Arc::downgrade(signaling);
    pool.register_auxiliary_request_handler(
        auth::route::METHOD,
        Arc::new(move |_, _, mut params| {
            let identity = identity.clone();
            let scope = scope.clone();
            let node = node.clone();
            let signaling = signaling.clone();
            Box::pin(async move {
                if params.len() != 1 || node.upgrade().is_none() {
                    return Err(
                        "route discovery requires one challenge and a running authority".into(),
                    );
                }
                let route = signaling
                    .upgrade()
                    .and_then(|s| s.own_peer_id())
                    .ok_or_else(|| "authority signaling is disconnected".to_owned())?;
                auth::route::receive(&identity, &scope, &route, params.remove(0))
                    .map_err(|error| error.to_string())
            })
        }),
    )
    .map_err(|error| io::Error::other(error.to_string()))
}
/// Install once during the host-owned pool lifecycle. The pool's room admission
/// still applies; the signed envelope then enforces the narrower voting group.
/// Weak node capture ensures a surviving pool cannot keep a stopped authority alive.
pub fn register_receiver<H: WebRTCConnectionHandler + 'static>(
    pool: &RxWebRTCReplicationPool<H>,
    identity: Arc<SigningIdentity>,
    scope: String,
    node: &Arc<AuthorityNode>,
) -> io::Result<()> {
    if !node.matches_local_identity(&scope, &identity.public_identity()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "control receiver does not match its authority node",
        ));
    }
    let node = Arc::downgrade(node);
    pool.register_auxiliary_request_handler(
        CONTROL_METHOD,
        Arc::new(move |_route_identity, _capability, mut params| {
            let identity = identity.clone();
            let scope = scope.clone();
            let node = node.clone();
            Box::pin(async move {
                if params.len() != 1 {
                    return Err("authority expects one signed envelope".into());
                }
                let node = node
                    .upgrade()
                    .ok_or_else(|| "authority is stopped".to_owned())?;
                auth::receive(&identity, &scope, &node, params.remove(0))
                    .await
                    .map_err(|e| e.to_string())
            })
        }),
    )
    .map_err(|error| io::Error::other(error.to_string()))
}
