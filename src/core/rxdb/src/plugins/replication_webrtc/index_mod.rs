//! Functional port of `src/plugins/replication-webrtc/index.ts`.
//!
//! `replicateWebRTC` upstream supports both roles (master and fork) per peer.
//! Both paths are wired now:
//! - **Master path:** when CTOX is picked as master for a peer connection, it
//!   answers incoming requests using `master_replication_handler` (from gap-item
//!   N5 / replication-protocol/index).
//! - **Fork path:** when CTOX is picked as fork, it constructs an
//!   `RxReplicationState` via [`replicate_rx_collection`] with pull/push
//!   handlers that delegate to [`send_message_and_await_answer`] for
//!   `masterChangesSince` and `masterWrite`, and feeds the peer's
//!   `masterChangeStream$` responses into the pull `stream_factory`.
//!
//! T1 deviations:
//! - Returns a `RxWebRTCReplicationPool` containing the per-peer state map.
//! - Loop avoidance, peer-token handshake, master/fork picking via
//!   `is_master_in_webrtc_replication` are all in place.
//! - Per-peer fork `RxReplicationState`s are owned by `PeerState` so cancel
//!   propagates on `remove_peer`.

use super::{protocol_contract_generated, NativePeerRole};

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use parking_lot::Mutex;
use rand::RngCore;
use serde_json::Value;
use tokio::sync::{Mutex as AsyncMutex, Semaphore};
use tokio_stream::StreamExt;
use webrtc::peer_connection::RTCIceServer;

use crate::plugin::add_rx_plugin;
use crate::plugins::leader_election::RxDBLeaderElectionPlugin;
use crate::plugins::replication::{
    replicate_rx_collection, PullHandler, PushHandler, ReplicationOptions,
    ReplicationPullHandlerResult, ReplicationPullOptions, ReplicationPushOptions,
    RxReplicationState, StreamFactory,
};
use crate::plugins::replication_webrtc::connection_handler_rs::{
    publish_best_effort_send_error, CollectionAuthzHook, CollectionEagerPullHook,
    CollectionLiveChangeHook, DocumentReadAuthzHook, DocumentWriteAuthzHook, WebRTCRsConfig,
    WebRTCRsConnectionHandler, WebRTCRsPeer,
};
use crate::plugins::replication_webrtc::signaling_client::SignalingClient;
use crate::plugins::replication_webrtc::webrtc_helper::{
    is_master_in_webrtc_replication, send_message_and_await_answer,
};
use crate::plugins::replication_webrtc::webrtc_types::{
    WebRTCConnectionHandler, WebRTCDocumentFilter, WebRTCMessage, WebRTCPeerSessionValidation,
    WebRTCPeerSessionValidator, WebRTCPeerValidator, WebRTCResponse, WebRTCWireFrame,
    CTOX_BROWSER_INPUT_RESPONSE_COLLECTION, CTOX_BROWSER_LIVE_RESPONSE_COLLECTION,
};
use crate::plugins::utils::utils_string::random_token;
use crate::replication_protocol::index_mod::rx_storage_instance_to_replication_handler;
use crate::rx_collection::RxCollection;
use crate::rx_database::RxDatabase;
use crate::rx_error::{new_rx_error, RxError};
use crate::rxjs_compat::RxSubject;
use crate::types::{DocumentsWithCheckpoint, RxReplicationHandler, RxReplicationMasterChange};
use protocol_contract_generated::{
    CTOX_APP_RUNTIME_CAPABILITY, CTOX_CHECKPOINT_GENERATION_CAPABILITY,
    CTOX_COMMAND_LIFECYCLE_CAPABILITY, CTOX_PRESENCE_CAPABILITY,
    CTOX_PROTOCOL_ERROR_CAPABILITY_MISSING, CTOX_PROTOCOL_ERROR_COLLECTION_MISMATCH,
    CTOX_PROTOCOL_ERROR_MISMATCH, CTOX_PROTOCOL_ERROR_MISSING,
    CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH, CTOX_PROTOCOL_ERROR_SCHEMA_VERSION_MISMATCH,
    CTOX_QUERY_FETCH_CAPABILITY, CTOX_REQUIRED_PROTOCOL_CAPABILITIES, CTOX_RXDB_PROTOCOL,
    CTOX_RXDB_RS_SCHEMA_HASH_SOURCE,
};

const FORK_RESYNC_INTERVAL: Duration = Duration::from_secs(5);
const PROTOCOL_ROOM_PAYLOAD_CACHE_TTL: Duration = Duration::from_secs(5);
const MAX_CONCURRENT_MASTER_PULLS: usize = 4;
const MAX_CONCURRENT_REQUEST_TASKS: usize = 32;
const MAX_CONCURRENT_HANDSHAKE_REQUESTS: usize = 4;
const MAX_CONCURRENT_AUXILIARY_REQUESTS: usize = 8;
const BROWSER_LIVE_METHOD: &str = "ctox.browser.live.v1";
const OUTBOUND_SELLIFY_LOOKUP_METHOD: &str = "ctox.outbound.sellify_lookup.v1";
const OUTBOUND_SELLIFY_RESPONSE_COLLECTION: &str = "outbound_lead_generation_leads";

fn auxiliary_response_collection(method: &str, operation: &str) -> Option<String> {
    if method == BROWSER_LIVE_METHOD {
        return Some(
            if operation == "input" {
                CTOX_BROWSER_INPUT_RESPONSE_COLLECTION
            } else {
                CTOX_BROWSER_LIVE_RESPONSE_COLLECTION
            }
            .to_string(),
        );
    }
    // Sellify lookups are read-only requests issued while the customer tenant lead
    // collection is foreground. Tag only their response for the normal
    // active-collection priority lane so it cannot sit behind a cold full
    // replication backlog and exhaust the browser's bounded request budget.
    if method == OUTBOUND_SELLIFY_LOOKUP_METHOD {
        return Some(OUTBOUND_SELLIFY_RESPONSE_COLLECTION.to_string());
    }
    None
}

static BROWSER_LIVE_WIRE_RECEIVED: AtomicU64 = AtomicU64::new(0);
static BROWSER_LIVE_HANDLER_FOUND: AtomicU64 = AtomicU64::new(0);
static BROWSER_LIVE_HANDLER_MISSING: AtomicU64 = AtomicU64::new(0);

pub fn auxiliary_request_metrics_snapshot() -> Value {
    serde_json::json!({
        "schema": "ctox.rxdb.auxiliary_request_counters.v1",
        "browser_live_wire_received": BROWSER_LIVE_WIRE_RECEIVED.load(Ordering::Relaxed),
        "browser_live_handler_found": BROWSER_LIVE_HANDLER_FOUND.load(Ordering::Relaxed),
        "browser_live_handler_missing": BROWSER_LIVE_HANDLER_MISSING.load(Ordering::Relaxed),
    })
}

pub type AuxiliaryRequestFuture =
    Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'static>>;
pub type AuxiliaryRequestHandler =
    Arc<dyn Fn(String, String, Vec<Value>) -> AuxiliaryRequestFuture + Send + Sync + 'static>;
const CTOX_RXDB_NATIVE_CAPABILITIES: &[&str] = &[
    "ctox-rxdb-native-v1",
    "ctox-file-chunks-v1",
    "ctox-replication-handshake-v1",
    "ctox-schema-hash-v1",
    "ctox-peer-session-v1",
    "ctox-device-proof-v1",
    "ctox-checkpoint-epoch-v1",
    CTOX_CHECKPOINT_GENERATION_CAPABILITY,
    CTOX_APP_RUNTIME_CAPABILITY,
    CTOX_QUERY_FETCH_CAPABILITY,
    CTOX_COMMAND_LIFECYCLE_CAPABILITY,
    "ctox-browser-live-v1",
    "ctox-workjet-device-control-v1",
    // Presence is ephemeral transport state (ctox-presence-v1): always
    // advertised, never gated on a runtime flag — there is no persistence or
    // policy surface behind it, only the in-memory hub in the connection
    // handler.
    CTOX_PRESENCE_CAPABILITY,
];

pub fn remote_supports_query_fetch(remote_protocol: &Value) -> bool {
    remote_protocol
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|caps| {
            caps.iter()
                .any(|item| item.as_str() == Some(CTOX_QUERY_FETCH_CAPABILITY))
        })
        .unwrap_or(false)
}

pub type RxWebRTCReplicationState<H> = RxWebRTCReplicationPool<H>;
type ForkStateMap<P> = HashMap<(String, P), Arc<RxReplicationState>>;

#[derive(Clone)]
pub struct SyncOptionsWebRTC<H: WebRTCConnectionHandler> {
    pub collection: Arc<RxCollection>,
    pub connection_handler: Arc<H>,
    pub topic: Option<String>,
    pub is_peer_valid: Option<WebRTCPeerValidator<H::Peer>>,
    pub is_peer_session_valid: Option<WebRTCPeerSessionValidator>,
    pub pull_batch_size: u64,
    pub push_batch_size: u64,
    pub retry_time: u64,
}

impl<H: WebRTCConnectionHandler> SyncOptionsWebRTC<H> {
    pub fn new(collection: Arc<RxCollection>, connection_handler: Arc<H>) -> Self {
        Self {
            collection,
            connection_handler,
            topic: None,
            is_peer_valid: None,
            is_peer_session_valid: None,
            pull_batch_size: 20,
            push_batch_size: 20,
            retry_time: 5_000,
        }
    }
}

pub struct SyncOptionsWebRTCRs {
    pub collection: Arc<RxCollection>,
    pub signaling_url: String,
    pub topic: String,
    pub peer_session_id: String,
    pub ice_servers: Vec<RTCIceServer>,
    pub is_peer_valid: Option<WebRTCPeerValidator<WebRTCRsPeer>>,
    pub is_peer_session_valid: Option<WebRTCPeerSessionValidator>,
    pub pull_batch_size: u64,
    pub push_batch_size: u64,
    pub retry_time: u64,
}

impl SyncOptionsWebRTCRs {
    pub fn new(
        collection: Arc<RxCollection>,
        signaling_url: impl Into<String>,
        topic: impl Into<String>,
    ) -> Self {
        Self {
            collection,
            signaling_url: signaling_url.into(),
            topic: topic.into(),
            peer_session_id: format!("rxdb-rs-{}", random_token(Some(16))),
            ice_servers: Vec::new(),
            is_peer_valid: None,
            is_peer_session_valid: None,
            pull_batch_size: 20,
            push_batch_size: 20,
            retry_time: 5_000,
        }
    }
}

#[derive(Clone)]
struct WebRTCReplicationTuning {
    topic: Option<String>,
    peer_session_id: Option<Arc<str>>,
    is_peer_session_valid: Option<WebRTCPeerSessionValidator>,
    pull_batch_size: u64,
    push_batch_size: u64,
    retry_time: u64,
}

impl Default for WebRTCReplicationTuning {
    fn default() -> Self {
        Self {
            topic: None,
            peer_session_id: None,
            is_peer_session_valid: None,
            pull_batch_size: 20,
            push_batch_size: 20,
            retry_time: 5_000,
        }
    }
}

// ref: rxdb/src/plugins/replication-webrtc/index.ts:230-298
/// Connection pool that owns the WebRTC connection handler and tracks peer
/// state. Returned by [`replicate_web_rtc`].
///
/// Phase 3 (single multiplexed stream): one pool now serves EVERY collection
/// in the sync room over ONE [`WebRTCConnectionHandler`] (one signaling room +
/// one RTCPeerConnection + one DataChannel per peer). The per-collection state
/// — master replication handler + fork replication state — lives in
/// collection-keyed maps; the message-stream loop demultiplexes inbound
/// `masterChangesSince` / `masterWrite` frames to the right collection via the
/// frame's `collection` field, and one master-change relay task per
/// (collection, peer) emits a collection-qualified `masterChangeStream$`.
pub struct RxWebRTCReplicationPool<H: WebRTCConnectionHandler> {
    pub connection_handler: Arc<H>,
    /// Per-collection master replication handler, keyed by collection name.
    pub master_replication_handlers: HashMap<String, Arc<dyn RxReplicationHandler>>,
    pub canceled: std::sync::atomic::AtomicBool,
    /// Makes concurrent `cancel()` callers wait for the in-flight teardown
    /// instead of returning merely because cancellation has started.
    cancel_lifecycle: AsyncMutex<()>,
    pub error_subject: RxSubject<RxError>,
    pub query_fetch_registry: Arc<super::query_fetch_handler::QueryFetchRegistry>,
    pub file_fetch_registry: Arc<super::file_fetch_handler::FileFetchRegistry>,
    /// Typed, explicitly registered request methods that share the authenticated
    /// WebRTC DataChannel without becoming RxDB documents. This is reserved for
    /// latency-sensitive ephemeral control planes such as the live Browser.
    auxiliary_request_handlers: Mutex<HashMap<String, AuxiliaryRequestHandler>>,
    /// Room admission is also required before sending auxiliary control RPCs.
    authenticated_peers: Arc<Mutex<HashSet<H::Peer>>>,
    outbound_ready_peers: Mutex<HashSet<H::Peer>>,
    /// Legacy compatibility must not hide an unverifiable multiplex handshake;
    /// this counter keeps every allowed or rejected omission operator-visible.
    missing_collection_schemas_warning_count: std::sync::atomic::AtomicU64,
    /// Every collection multiplexed onto this connection, keyed by name.
    collections: HashMap<String, Arc<RxCollection>>,
    /// Short-lived shared room payload for the `ctoxProtocol` handshake.
    ///
    /// A fresh Business OS browser can send multiple `ctoxProtocol` requests
    /// while the native peer also starts its own handshake. Building
    /// collectionSchemas/collectionCheckpoints for the whole multiplexed room
    /// per task fans out into hundreds of SQLite checkpoint snapshots. Cache
    /// inspection stays synchronous and independent from the build gate so a
    /// valid hit never waits behind storage work.
    protocol_room_payload_cache: Mutex<ProtocolRoomPayloadCache>,
    /// Coalesces cache misses without holding the cache mutex across schema or
    /// checkpoint awaits. Waiters re-check the cache after acquiring this gate
    /// and consume the winner's payload instead of starting another build.
    protocol_room_payload_build: AsyncMutex<()>,
    /// Historical pulls perform synchronous SQLite work below an async
    /// boundary. Bound them so command writes retain a runnable Tokio worker.
    master_pull_semaphore: Semaphore,
    /// Bounds replication, query, and file-fetch request futures.
    request_semaphore: Arc<Semaphore>,
    /// Protocol admission must remain runnable when data transfers back up.
    handshake_request_semaphore: Arc<Semaphore>,
    auxiliary_request_semaphore: Arc<Semaphore>,
    /// Per-peer sub-tasks (the master-change relay tasks, one per collection).
    peer_states: Mutex<HashMap<H::Peer, PeerState>>,
    /// Fork replication states keyed by (collection, peer). One entry per
    /// collection per peer for which CTOX was elected fork.
    fork_states: Mutex<ForkStateMap<H::Peer>>,
    /// Serializes replacement/removal so displaced fork cancellation completes
    /// before a successor becomes visible and before pool cancellation returns.
    fork_state_lifecycle: AsyncMutex<()>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

struct PeerState {
    sub_tasks: Vec<tokio::task::JoinHandle<()>>,
}

#[derive(Clone, Default)]
struct ProtocolRoomPayload {
    collection_schemas: Option<Value>,
    collection_checkpoints: Option<Value>,
}

#[derive(Default)]
struct ProtocolRoomPayloadCache {
    payload: Option<ProtocolRoomPayload>,
    built_at: Option<Instant>,
}

impl<H: WebRTCConnectionHandler + 'static> RxWebRTCReplicationPool<H> {
    /// Single-collection constructor (back-compat). Builds a multiplexed pool
    /// that happens to carry exactly one collection.
    pub fn new(collection: Arc<RxCollection>, connection_handler: Arc<H>) -> Arc<Self> {
        Self::new_multi(vec![collection], connection_handler)
    }

    /// Phase 3 constructor: build a pool that multiplexes every supplied
    /// collection over one connection handler. All collections are registered
    /// into the shared query/file fetch registries (keyed by collection name)
    /// and each gets its own master replication handler.
    pub fn new_multi(collections: Vec<Arc<RxCollection>>, connection_handler: Arc<H>) -> Arc<Self> {
        let registry = Arc::new(super::query_fetch_handler::QueryFetchRegistry::new(
            protocol_contract_generated::CTOX_QUERY_MAX_IN_FLIGHT_STREAMS as u64,
        ));
        let file_registry = Arc::new(super::file_fetch_handler::FileFetchRegistry::new(
            protocol_contract_generated::CTOX_QUERY_MAX_IN_FLIGHT_STREAMS as u64,
        ));
        registry.set_auth_check(Arc::new(|_peer_identity, _collection| true));
        file_registry.set_auth_check(Arc::new(|_peer_identity, _collection| true));
        let mut master_replication_handlers: HashMap<String, Arc<dyn RxReplicationHandler>> =
            HashMap::new();
        let mut collection_map: HashMap<String, Arc<RxCollection>> = HashMap::new();
        for collection in collections.into_iter() {
            let handler = rx_storage_instance_to_replication_handler(
                Arc::clone(&collection.storage_instance),
                Arc::clone(&collection.conflict_handler),
                collection.database.token.clone(),
                false, // keep_meta = false (upstream default)
            );
            // Auto-register the collection into the shared registries. The
            // browser will only ask for collections it has been told about;
            // this is a no-op for unknown collections.
            registry.register(Arc::clone(&collection));
            master_replication_handlers.insert(collection.name.clone(), handler);
            collection_map.insert(collection.name.clone(), collection);
        }
        Arc::new(Self {
            connection_handler,
            master_replication_handlers,
            canceled: std::sync::atomic::AtomicBool::new(false),
            cancel_lifecycle: AsyncMutex::new(()),
            error_subject: RxSubject::new(),
            query_fetch_registry: registry,
            file_fetch_registry: file_registry,
            auxiliary_request_handlers: Mutex::new(HashMap::new()),
            authenticated_peers: Arc::new(Mutex::new(HashSet::new())),
            outbound_ready_peers: Mutex::new(HashSet::new()),
            missing_collection_schemas_warning_count: std::sync::atomic::AtomicU64::new(0),
            collections: collection_map,
            protocol_room_payload_cache: Mutex::new(ProtocolRoomPayloadCache::default()),
            protocol_room_payload_build: AsyncMutex::new(()),
            master_pull_semaphore: Semaphore::new(MAX_CONCURRENT_MASTER_PULLS),
            request_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUEST_TASKS)),
            handshake_request_semaphore: Arc::new(Semaphore::new(
                MAX_CONCURRENT_HANDSHAKE_REQUESTS,
            )),
            auxiliary_request_semaphore: Arc::new(Semaphore::new(
                MAX_CONCURRENT_AUXILIARY_REQUESTS,
            )),
            peer_states: Mutex::new(HashMap::new()),
            fork_states: Mutex::new(HashMap::new()),
            fork_state_lifecycle: AsyncMutex::new(()),
            tasks: Mutex::new(Vec::new()),
        })
    }

    /// All collections multiplexed onto this connection.
    pub fn collections(&self) -> Vec<Arc<RxCollection>> {
        self.collections.values().cloned().collect()
    }

    /// Both inbound admission and our completed protocol/token handshake are
    /// required. Receiving a peer's probe does not prove it has admitted us.
    pub fn is_peer_ready_for_control(&self, peer: &H::Peer) -> bool {
        !self.canceled.load(std::sync::atomic::Ordering::SeqCst)
            && self.connection_handler.is_peer_current(peer)
            && self.authenticated_peers.lock().contains(peer)
            && self.outbound_ready_peers.lock().contains(peer)
    }

    fn mark_peer_admitted(&self, peer: &H::Peer, outbound: bool) {
        let mut states = self.peer_states.lock();
        if !self.connection_handler.is_peer_current(peer)
            || self.canceled.load(std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        // An inbound handshake may precede consumption of the open event.
        // It belongs to the same transport handle, never to a route-only slot.
        states.entry(peer.clone()).or_insert_with(|| PeerState {
            sub_tasks: Vec::new(),
        });
        if outbound {
            self.outbound_ready_peers.lock().insert(peer.clone());
        } else {
            self.authenticated_peers.lock().insert(peer.clone());
        }
    }

    pub fn set_auxiliary_request_handler(
        &self,
        method: impl Into<String>,
        handler: AuxiliaryRequestHandler,
    ) {
        self.auxiliary_request_handlers
            .lock()
            .insert(method.into(), handler);
    }

    /// Register a lifecycle-owned control contract without silently replacing
    /// another owner. Existing replaceable auxiliary handlers retain their API.
    pub fn register_auxiliary_request_handler(
        &self,
        method: impl Into<String>,
        handler: AuxiliaryRequestHandler,
    ) -> Result<(), RxError> {
        let method = method.into();
        let mut handlers = self.auxiliary_request_handlers.lock();
        if handlers.contains_key(&method) {
            return Err(new_rx_error(
                "RC_WEBRTC_CONTROL",
                Some(serde_json::json!({
                    "message": "auxiliary control contract already has an owner",
                    "method": method,
                })),
            ));
        }
        handlers.insert(method, handler);
        Ok(())
    }

    /// Snapshot the number of unverifiable multiplex schema handshakes seen.
    pub fn count_missing_collection_schemas_warnings(&self) -> u64 {
        self.missing_collection_schemas_warning_count
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    fn collection_by_name(&self, name: &str) -> Option<Arc<RxCollection>> {
        self.collections.get(name).cloned()
    }

    fn master_handler_for(&self, name: &str) -> Option<Arc<dyn RxReplicationHandler>> {
        self.master_replication_handlers.get(name).cloned()
    }

    fn cached_protocol_room_payload(&self, now: Instant) -> Option<ProtocolRoomPayload> {
        let cache = self.protocol_room_payload_cache.lock();
        match (&cache.payload, cache.built_at) {
            (Some(payload), Some(built_at))
                if now.duration_since(built_at) <= PROTOCOL_ROOM_PAYLOAD_CACHE_TTL =>
            {
                Some(payload.clone())
            }
            _ => None,
        }
    }

    async fn protocol_room_payload_with<F, Fut>(&self, build: F) -> ProtocolRoomPayload
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ProtocolRoomPayload>,
    {
        // Fast path deliberately does not touch the async build gate. A valid
        // cached handshake therefore remains available even while another task
        // owns the in-flight-build slot.
        if let Some(payload) = self.cached_protocol_room_payload(Instant::now()) {
            return payload;
        }

        let _build_guard = self.protocol_room_payload_build.lock().await;
        // Every waiter re-checks after the winner completes. This preserves
        // exactly-one-build coalescing without retaining the cache mutex across
        // schema hashing or storage checkpoint awaits.
        if let Some(payload) = self.cached_protocol_room_payload(Instant::now()) {
            return payload;
        }

        let payload = build().await;
        let mut cache = self.protocol_room_payload_cache.lock();
        cache.built_at = Some(Instant::now());
        cache.payload = Some(payload.clone());
        payload
    }

    async fn protocol_room_payload(&self) -> ProtocolRoomPayload {
        let collections = self.collections();
        self.protocol_room_payload_with(|| async move {
            ProtocolRoomPayload {
                collection_schemas: Some(collection_schemas_payload(&collections).await),
                collection_checkpoints: Some(collection_checkpoints_payload(&collections).await),
            }
        })
        .await
    }

    /// Register the per-peer relay sub-tasks (one master-change relay per
    /// collection for which CTOX is master to this peer). Fork states are
    /// tracked separately in [`Self::add_fork_state`].
    pub fn add_peer(&self, peer: H::Peer, sub_tasks: Vec<tokio::task::JoinHandle<()>>) {
        let mut states = self.peer_states.lock();
        if self.canceled.load(std::sync::atomic::Ordering::SeqCst)
            || !self.connection_handler.is_peer_current(&peer)
        {
            for task in sub_tasks {
                task.abort();
            }
            return;
        }
        let entry = states.entry(peer).or_insert_with(|| PeerState {
            sub_tasks: Vec::new(),
        });
        entry.sub_tasks.extend(sub_tasks);
    }

    /// Start a pool-owned task behind a registration gate. The future cannot do
    /// work before its handle is visible to `cancel`, and a concurrent cancel
    /// either rejects it before start or aborts and joins it from `self.tasks`.
    fn spawn_tracked<F>(self: &Arc<Self>, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.spawn_limited(Arc::clone(&self.request_semaphore), future);
    }

    fn spawn_replication_request<F>(self: &Arc<Self>, method: &str, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if matches!(method, "token" | "ctoxProtocol") {
            self.spawn_limited(Arc::clone(&self.handshake_request_semaphore), future);
        } else {
            self.spawn_tracked(future);
        }
    }

    fn spawn_limited<F>(self: &Arc<Self>, request_semaphore: Arc<Semaphore>, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            if start_rx.await.is_err() {
                return;
            }
            let Ok(_permit) = request_semaphore.acquire_owned().await else {
                return;
            };
            future.await;
        });
        let mut tasks = self.tasks.lock();
        tasks.retain(|task| !task.is_finished());
        if self.canceled.load(std::sync::atomic::Ordering::SeqCst) {
            task.abort();
            return;
        }
        tasks.push(task);
        drop(tasks);
        let _ = start_tx.send(());
    }

    /// Track latency-sensitive auxiliary work (for example Browser live input
    /// and frames) independently from bulk replication requests. A restored
    /// workspace can occupy every ordinary request permit with collection
    /// pulls; queuing interactive work behind those pulls turns a healthy
    /// DataChannel into a deterministic 30-second UI timeout.
    fn spawn_auxiliary_tracked<F>(self: &Arc<Self>, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.spawn_limited(Arc::clone(&self.auxiliary_request_semaphore), future);
    }

    /// Record a per-(collection, peer) fork replication state so cancel
    /// propagates on `remove_peer` / `cancel`.
    pub async fn add_fork_state(
        &self,
        collection: String,
        peer: H::Peer,
        fork_state: Arc<RxReplicationState>,
    ) {
        let _lifecycle = self.fork_state_lifecycle.lock().await;
        if self.canceled.load(std::sync::atomic::Ordering::SeqCst)
            || !self.connection_handler.is_peer_current(&peer)
        {
            fork_state.cancel().await;
            return;
        }
        let identity = self.connection_handler.peer_identity(&peer);
        let displaced: Vec<_> = {
            let mut forks = self.fork_states.lock();
            let keys: Vec<_> = forks
                .iter()
                .filter(|((name, connection), state)| {
                    name == &collection
                        && (self.connection_handler.peer_identity(connection) == identity
                            || state.replication_identifier == fork_state.replication_identifier)
                })
                .map(|(key, _)| key.clone())
                .collect();
            keys.into_iter()
                .filter_map(|key| forks.remove(&key))
                .collect()
        };
        for old in displaced {
            // A re-handshake replacement must not become active until the old
            // state has stopped touching the shared checkpoint metadata.
            old.cancel().await;
        }
        if self.canceled.load(std::sync::atomic::Ordering::SeqCst)
            || !self.connection_handler.is_peer_current(&peer)
        {
            fork_state.cancel().await;
            return;
        }
        let key = (collection, peer);
        // Register before awaiting start so cancellation always owns the state.
        // The lifecycle guard prevents replacement/checkpoint writers overlapping.
        self.fork_states
            .lock()
            .insert(key.clone(), Arc::clone(&fork_state));
        if let Err(error) = fork_state.start().await {
            self.fork_states.lock().remove(&key);
            fork_state.cancel().await;
            self.error_subject.next(error);
        }
    }

    pub async fn remove_peer(&self, peer: &H::Peer) {
        let sub_tasks = {
            let mut states = self.peer_states.lock();
            let tasks = states
                .remove(peer)
                .map(|state| state.sub_tasks)
                .unwrap_or_default();
            self.authenticated_peers.lock().remove(peer);
            self.outbound_ready_peers.lock().remove(peer);
            tasks
        };
        let peer_identity = self.connection_handler.connection_identity(peer);
        self.query_fetch_registry.cancel_peer(&peer_identity);
        self.file_fetch_registry.cancel_peer(&peer_identity);
        for task in &sub_tasks {
            task.abort();
        }
        for task in sub_tasks {
            let _ = task.await;
        }

        let _lifecycle = self.fork_state_lifecycle.lock().await;
        // Cancel and drop every fork state bound to this peer (all collections).
        let drained: Vec<Arc<RxReplicationState>> = {
            let mut forks = self.fork_states.lock();
            let keys: Vec<(String, H::Peer)> =
                forks.keys().filter(|(_, p)| p == peer).cloned().collect();
            keys.into_iter().filter_map(|k| forks.remove(&k)).collect()
        };
        for fork in drained {
            fork.cancel().await;
        }
    }

    pub async fn cancel(&self) {
        let _cancel_lifecycle = self.cancel_lifecycle.lock().await;
        if self
            .canceled
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }

        self.authenticated_peers.lock().clear();
        self.outbound_ready_peers.lock().clear();
        // Stop request/loop tasks first and join their cancellation. Once this
        // completes they can neither enter storage nor emit a late response.
        let tasks = std::mem::take(&mut *self.tasks.lock());
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            let _ = task.await;
        }

        // Cancel all peer sub-tasks + fork states.
        let peers: Vec<H::Peer> = self.peer_states.lock().keys().cloned().collect();
        for peer in peers {
            self.remove_peer(&peer).await;
        }
        // Cancel any fork states whose peer never had a peer_states entry.
        let _lifecycle = self.fork_state_lifecycle.lock().await;
        let drained: Vec<Arc<RxReplicationState>> = self
            .fork_states
            .lock()
            .drain()
            .map(|(_, value)| value)
            .collect();
        for fork in drained {
            fork.cancel().await;
        }
        drop(_lifecycle);
        let _ = self.connection_handler.close().await;
    }
}

// ref: rxdb/src/plugins/replication-webrtc/index.ts:40-223
/// Replicate an `RxCollection` over a WebRTC mesh.
///
/// When CTOX is picked as master for a peer, the pool serves that peer's
/// `master_changes_since` / `master_write` / `master_change_stream` queries
/// from `master_replication_handler`. When CTOX is picked as fork, the pool
/// creates a per-peer [`RxReplicationState`] that delegates pull/push calls to
/// the remote master over WebRTC.
pub async fn replicate_web_rtc<H>(
    collection: Arc<RxCollection>,
    connection_handler: Arc<H>,
    is_peer_valid: Option<WebRTCPeerValidator<H::Peer>>,
) -> Result<Arc<RxWebRTCReplicationPool<H>>, RxError>
where
    H: WebRTCConnectionHandler + 'static,
{
    replicate_web_rtc_inner(
        Arc::clone(&collection.database),
        vec![collection],
        connection_handler,
        is_peer_valid,
        WebRTCReplicationTuning::default(),
    )
    .await
}

/// Phase 3 multiplexed entry point: replicate EVERY supplied collection over
/// ONE connection handler (one room / RTCPeerConnection / DataChannel per
/// peer). Frames are demultiplexed by their `collection` field.
pub async fn replicate_web_rtc_multi<H>(
    collections: Vec<Arc<RxCollection>>,
    connection_handler: Arc<H>,
    is_peer_valid: Option<WebRTCPeerValidator<H::Peer>>,
    tuning_topic: Option<String>,
    peer_session_id: Option<Arc<str>>,
) -> Result<Arc<RxWebRTCReplicationPool<H>>, RxError>
where
    H: WebRTCConnectionHandler + 'static,
{
    replicate_web_rtc_multi_with_validators(
        collections_database(&collections)?,
        collections,
        connection_handler,
        is_peer_valid,
        None,
        tuning_topic,
        peer_session_id,
    )
    .await
}

/// One database identity and zero or more replicated collections, with
/// independent signaling-peer and durable-session admission predicates.
/// A control-only peer uses an empty collection set, not a dummy collection.
pub async fn replicate_web_rtc_multi_with_validators<H>(
    database: Arc<RxDatabase>,
    collections: Vec<Arc<RxCollection>>,
    connection_handler: Arc<H>,
    is_peer_valid: Option<WebRTCPeerValidator<H::Peer>>,
    is_peer_session_valid: Option<WebRTCPeerSessionValidator>,
    tuning_topic: Option<String>,
    peer_session_id: Option<Arc<str>>,
) -> Result<Arc<RxWebRTCReplicationPool<H>>, RxError>
where
    H: WebRTCConnectionHandler + 'static,
{
    replicate_web_rtc_inner(
        database,
        collections,
        connection_handler,
        is_peer_valid,
        WebRTCReplicationTuning {
            topic: tuning_topic,
            peer_session_id,
            is_peer_session_valid,
            ..WebRTCReplicationTuning::default()
        },
    )
    .await
}

pub async fn replicate_web_rtc_with_options<H>(
    options: SyncOptionsWebRTC<H>,
) -> Result<Arc<RxWebRTCReplicationPool<H>>, RxError>
where
    H: WebRTCConnectionHandler + 'static,
{
    replicate_web_rtc_inner(
        Arc::clone(&options.collection.database),
        vec![options.collection],
        options.connection_handler,
        options.is_peer_valid,
        WebRTCReplicationTuning {
            topic: options.topic,
            peer_session_id: None,
            is_peer_session_valid: options.is_peer_session_valid,
            pull_batch_size: options.pull_batch_size,
            push_batch_size: options.push_batch_size,
            retry_time: options.retry_time,
        },
    )
    .await
}

pub async fn replicate_web_rtc_rs(
    options: SyncOptionsWebRTCRs,
) -> Result<Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>, RxError> {
    let room = options.topic.clone();
    let signaling = SignalingClient::connect(options.signaling_url).await?;
    let mut config = WebRTCRsConfig::new(signaling, room);
    if !options.ice_servers.is_empty() {
        config.ice_servers = options.ice_servers;
    }
    let handler = WebRTCRsConnectionHandler::new_with_signaling(config).await?;
    replicate_web_rtc_inner(
        Arc::clone(&options.collection.database),
        vec![options.collection],
        handler,
        native_route_validator(options.is_peer_valid),
        WebRTCReplicationTuning {
            topic: Some(options.topic),
            peer_session_id: Some(Arc::<str>::from(options.peer_session_id)),
            is_peer_session_valid: options.is_peer_session_valid,
            pull_batch_size: options.pull_batch_size,
            push_batch_size: options.push_batch_size,
            retry_time: options.retry_time,
        },
    )
    .await
}

/// Phase 3: bring up ONE multiplexed replication session for an entire sync
/// room. Connects one [`SignalingClient`] + one [`WebRTCRsConnectionHandler`]
/// joined to `topic` (the bare sync room, NOT a per-collection topic) and
/// registers every supplied collection's master handler + fork state behind
/// it. This is the native counterpart to the browser's single
/// `CtoxWebRtcNativePeer` per sync room.
// Known finding, deliberately NOT fixed here: this and the two URL-provider
// variants below forward the same collection/auth/tuning/transport fields, and
// the options-object direction should eventually own the multiplex config too.
// That is an API change, out of scope for the clippy pass (SYNC-A-E2) — it is
// recorded for the final re-review instead of being rushed.
#[allow(clippy::too_many_arguments)]
pub async fn replicate_web_rtc_rs_multi(
    collections: Vec<Arc<RxCollection>>,
    signaling_url: String,
    topic: String,
    peer_session_id: String,
    ice_servers: Vec<RTCIceServer>,
    is_peer_valid: Option<WebRTCPeerValidator<WebRTCRsPeer>>,
    pull_batch_size: u64,
    push_batch_size: u64,
    retry_time: u64,
) -> Result<Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>, RxError> {
    replicate_web_rtc_rs_multi_with_url_provider(
        collections,
        Arc::new(move || signaling_url.clone()),
        topic,
        peer_session_id,
        ice_servers,
        is_peer_valid,
        None,
        None,
        None,
        None,
        None,
        None,
        pull_batch_size,
        push_batch_size,
        retry_time,
    )
    .await
}

/// Like [`replicate_web_rtc_rs_multi`], but the signaling URL comes from a
/// provider that is re-evaluated on every reconnect attempt. Callers whose
/// URLs carry freshness-windowed auth params (`token_iat`/`token_exp`) MUST
/// use this variant — a frozen URL turns into a permanent join-rejection loop
/// once the window expires.
#[allow(clippy::too_many_arguments)]
pub async fn replicate_web_rtc_rs_multi_with_url_provider(
    collections: Vec<Arc<RxCollection>>,
    signaling_url_provider: Arc<dyn Fn() -> String + Send + Sync>,
    topic: String,
    peer_session_id: String,
    ice_servers: Vec<RTCIceServer>,
    is_peer_valid: Option<WebRTCPeerValidator<WebRTCRsPeer>>,
    collection_authz: Option<CollectionAuthzHook>,
    collection_eager_pull: Option<CollectionEagerPullHook>,
    collection_live_change: Option<CollectionLiveChangeHook>,
    collection_write_authz: Option<CollectionAuthzHook>,
    document_read_authz: Option<DocumentReadAuthzHook>,
    document_write_authz: Option<DocumentWriteAuthzHook>,
    pull_batch_size: u64,
    push_batch_size: u64,
    retry_time: u64,
) -> Result<Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>, RxError> {
    let provider = Arc::clone(&signaling_url_provider);
    replicate_web_rtc_rs_multi_with_url_list_provider(
        collections,
        Arc::new(move || vec![provider()]),
        topic,
        peer_session_id,
        ice_servers,
        is_peer_valid,
        collection_authz,
        collection_eager_pull,
        collection_live_change,
        collection_write_authz,
        document_read_authz,
        document_write_authz,
        pull_batch_size,
        push_batch_size,
        retry_time,
    )
    .await
}

/// Like [`replicate_web_rtc_rs_multi_with_url_provider`], but the provider
/// returns a FAILOVER LIST of signaling URLs (each re-derived per attempt).
/// The signaling client tries the list in order on the initial connect and
/// rotates to the next candidate only when an establish attempt fails — the
/// configured URL list used to be cosmetic (only the first entry was ever
/// used), which made the primary signaling server a single point of failure
/// for new pairings.
#[allow(clippy::too_many_arguments)]
pub async fn replicate_web_rtc_rs_multi_with_url_list_provider(
    collections: Vec<Arc<RxCollection>>,
    signaling_url_provider: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
    topic: String,
    peer_session_id: String,
    ice_servers: Vec<RTCIceServer>,
    is_peer_valid: Option<WebRTCPeerValidator<WebRTCRsPeer>>,
    collection_authz: Option<CollectionAuthzHook>,
    collection_eager_pull: Option<CollectionEagerPullHook>,
    collection_live_change: Option<CollectionLiveChangeHook>,
    collection_write_authz: Option<CollectionAuthzHook>,
    document_read_authz: Option<DocumentReadAuthzHook>,
    document_write_authz: Option<DocumentWriteAuthzHook>,
    pull_batch_size: u64,
    push_batch_size: u64,
    retry_time: u64,
) -> Result<Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>, RxError> {
    replicate_web_rtc_rs_multi_with_url_list_provider_and_validators(
        collections,
        signaling_url_provider,
        topic,
        peer_session_id,
        ice_servers,
        is_peer_valid,
        None,
        collection_authz,
        collection_eager_pull,
        collection_live_change,
        collection_write_authz,
        document_read_authz,
        document_write_authz,
        pull_batch_size,
        push_batch_size,
        retry_time,
    )
    .await
}

/// URL-list provider variant with a separate durable browser-session gate.
/// Existing callers keep using the wrapper above with that gate disabled.
#[allow(clippy::too_many_arguments)]
pub async fn replicate_web_rtc_rs_multi_with_url_list_provider_and_validators(
    collections: Vec<Arc<RxCollection>>,
    signaling_url_provider: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
    topic: String,
    peer_session_id: String,
    ice_servers: Vec<RTCIceServer>,
    is_peer_valid: Option<WebRTCPeerValidator<WebRTCRsPeer>>,
    is_peer_session_valid: Option<WebRTCPeerSessionValidator>,
    collection_authz: Option<CollectionAuthzHook>,
    collection_eager_pull: Option<CollectionEagerPullHook>,
    collection_live_change: Option<CollectionLiveChangeHook>,
    collection_write_authz: Option<CollectionAuthzHook>,
    document_read_authz: Option<DocumentReadAuthzHook>,
    document_write_authz: Option<DocumentWriteAuthzHook>,
    pull_batch_size: u64,
    push_batch_size: u64,
    retry_time: u64,
) -> Result<Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>, RxError> {
    let database = collections_database(&collections)?;
    let provider = Arc::clone(&signaling_url_provider);
    let signaling = SignalingClient::connect_with_url_list_provider(move || provider()).await?;
    let mut config = WebRTCRsConfig::new(signaling, topic.clone());
    if !ice_servers.is_empty() {
        config.ice_servers = ice_servers;
    }
    let handler = WebRTCRsConnectionHandler::new_with_signaling(config).await?;
    // #12c: install the per-collection authz hook before peers connect.
    handler.set_collection_authz(collection_authz);
    handler.set_collection_eager_pull(collection_eager_pull);
    handler.set_collection_live_change(collection_live_change);
    handler.set_collection_write_authz(collection_write_authz);
    handler.set_document_read_authz(document_read_authz);
    handler.set_document_write_authz(document_write_authz);
    replicate_web_rtc_inner(
        database,
        collections,
        handler,
        native_route_validator(is_peer_valid),
        WebRTCReplicationTuning {
            topic: Some(topic),
            peer_session_id: Some(Arc::<str>::from(peer_session_id)),
            is_peer_session_valid,
            pull_batch_size,
            push_batch_size,
            retry_time,
        },
    )
    .await
}

/// Route policy stays independent from the transport's connection lifetime.
fn native_route_validator(
    validator: Option<WebRTCPeerValidator<WebRTCRsPeer>>,
) -> Option<WebRTCPeerValidator<super::WebRTCRsConnection>> {
    validator.map(|validate| {
        Arc::new(move |peer: &super::WebRTCRsConnection| validate(&peer.peer_id().to_owned()))
            as WebRTCPeerValidator<super::WebRTCRsConnection>
    })
}

fn collections_database(collections: &[Arc<RxCollection>]) -> Result<Arc<RxDatabase>, RxError> {
    collections
        .first()
        .map(|collection| Arc::clone(&collection.database))
        .ok_or_else(|| {
            new_rx_error(
                "RC_WEBRTC",
                Some(serde_json::json!({
                    "message": "empty replication set requires an explicit database identity"
                })),
            )
        })
}

async fn replicate_web_rtc_inner<H>(
    database: Arc<RxDatabase>,
    collections: Vec<Arc<RxCollection>>,
    connection_handler: Arc<H>,
    is_peer_valid: Option<WebRTCPeerValidator<H::Peer>>,
    tuning: WebRTCReplicationTuning,
) -> Result<Arc<RxWebRTCReplicationPool<H>>, RxError>
where
    H: WebRTCConnectionHandler + 'static,
{
    // ref: rxdb/src/plugins/replication-webrtc/index.ts:44
    let _ = add_rx_plugin(Arc::new(RxDBLeaderElectionPlugin));

    if collections
        .iter()
        .any(|collection| !Arc::ptr_eq(&collection.database, &database))
    {
        return Err(new_rx_error(
            "RC_WEBRTC",
            Some(serde_json::json!({
                "message": "every replicated collection must belong to the supplied database"
            })),
        ));
    }
    let representative = collections.first().cloned();
    let multiplexed = collections.len() > 1;

    // ref: rxdb/src/plugins/replication-webrtc/index.ts:58-60
    if database.multi_instance {
        database.wait_for_leadership().await;
    }

    let storage_token = database.storage_token.clone();
    let request_flag = random_token(Some(10));
    let request_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let peer_session_id = tuning.peer_session_id.clone();
    let is_peer_session_valid = tuning.is_peer_session_valid.clone();
    // When a validator is installed, no data-plane RPC is served until that
    // peer has completed an accepted ctoxProtocol round. This makes a deferred
    // bound token fail closed even when legacy collection authz is disabled.
    let pool = RxWebRTCReplicationPool::<H>::new_multi(
        collections.clone(),
        Arc::clone(&connection_handler),
    );
    let authenticated_peers = Arc::clone(&pool.authenticated_peers);

    // Wire pool-level subscriptions: error$ relay and disconnect → remove_peer.
    {
        let pool_clone = Arc::clone(&pool);
        let mut err_stream = connection_handler.error_stream();
        let t = tokio::spawn(async move {
            while let Some(e) = err_stream.next().await {
                pool_clone.error_subject.next(e);
            }
        });
        pool.tasks.lock().push(t);
    }
    {
        let pool_clone = Arc::clone(&pool);
        let mut disc_stream = connection_handler.disconnect_stream();
        let t = tokio::spawn(async move {
            while let Some(peer) = disc_stream.next().await {
                pool_clone.remove_peer(&peer).await;
            }
        });
        pool.tasks.lock().push(t);
    }

    // ref: rxdb/src/plugins/replication-webrtc/index.ts:86-95
    // Answer control handshake requests from remote peers.
    {
        let pool_clone = Arc::clone(&pool);
        let handler = Arc::clone(&connection_handler);
        let representative = representative.clone();
        let storage_token = storage_token.clone();
        let peer_session_id = peer_session_id.clone();
        let is_peer_session_valid = is_peer_session_valid.clone();
        let authenticated_peers = Arc::clone(&authenticated_peers);
        let mut msg_stream = connection_handler.message_stream();
        let t = tokio::spawn(async move {
            while let Some(item) = msg_stream.next().await {
                if pool_clone
                    .canceled
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    break;
                }
                if is_peer_session_valid.is_some()
                    && !matches!(item.message.method.as_str(), "ctoxProtocol" | "token")
                    && !authenticated_peers.lock().contains(&item.peer)
                {
                    let response = WebRTCResponse {
                        id: item.message.id,
                        result: Value::Null,
                        error: Some("peer_authentication_required".to_string()),
                        collection: item.message.collection,
                    };
                    if let Err(error) = handler
                        .send(&item.peer, WebRTCWireFrame::Response(response))
                        .await
                    {
                        pool_clone.error_subject.next(error);
                    }
                    handler.close_peer(&item.peer).await;
                    continue;
                }
                if item.message.method == super::query_fetch_handler::query_fetch_method() {
                    let registry = Arc::clone(&pool_clone.query_fetch_registry);
                    let handler_clone = Arc::clone(&handler);
                    let peer = item.peer.clone();
                    let peer_identity = handler.peer_identity(&peer);
                    let message = item.message.clone();
                    pool_clone.spawn_tracked(async move {
                        let _ = super::query_fetch_handler::run_query_fetch(
                            registry,
                            handler_clone,
                            peer,
                            peer_identity,
                            message,
                        )
                        .await;
                    });
                    continue;
                }
                if item.message.method == super::query_fetch_handler::query_cancel_method() {
                    if let Ok(request_id) =
                        super::query_fetch_handler::parse_query_cancel_request(&item.message)
                    {
                        let peer_identity = handler.connection_identity(&item.peer);
                        pool_clone
                            .query_fetch_registry
                            .cancel(&peer_identity, &request_id);
                    }
                    continue;
                }
                if item.message.method == super::file_fetch_handler::file_fetch_method() {
                    let registry = Arc::clone(&pool_clone.file_fetch_registry);
                    let handler_clone = Arc::clone(&handler);
                    let peer = item.peer.clone();
                    let peer_identity = handler.peer_identity(&peer);
                    let message = item.message.clone();
                    pool_clone.spawn_tracked(async move {
                        let _ = super::file_fetch_handler::run_file_fetch(
                            registry,
                            handler_clone,
                            peer,
                            peer_identity,
                            message,
                        )
                        .await;
                    });
                    continue;
                }
                if item.message.method == super::file_fetch_handler::file_cancel_method() {
                    if let Ok(request_id) =
                        super::file_fetch_handler::parse_file_cancel_request(&item.message)
                    {
                        let peer_identity = handler.connection_identity(&item.peer);
                        pool_clone
                            .file_fetch_registry
                            .cancel(&peer_identity, &request_id);
                    }
                    continue;
                }
                let is_browser_live = item.message.method == BROWSER_LIVE_METHOD;
                if is_browser_live {
                    BROWSER_LIVE_WIRE_RECEIVED.fetch_add(1, Ordering::Relaxed);
                }
                if let Some(auxiliary_handler) = pool_clone
                    .auxiliary_request_handlers
                    .lock()
                    .get(&item.message.method)
                    .cloned()
                {
                    if is_browser_live {
                        BROWSER_LIVE_HANDLER_FOUND.fetch_add(1, Ordering::Relaxed);
                    }
                    let pool_task = Arc::clone(&pool_clone);
                    let handler_task = Arc::clone(&handler);
                    let peer = item.peer.clone();
                    let request = item.message.clone();
                    let browser_operation = request
                        .params
                        .first()
                        .and_then(|params| params.get("op"))
                        .and_then(Value::as_str)
                        .unwrap_or("live")
                        .to_string();
                    let response_collection =
                        auxiliary_response_collection(&request.method, &browser_operation);
                    let peer_identity = handler.peer_identity(&peer);
                    let capability_token = handler.peer_capability_token(&peer).unwrap_or_default();
                    pool_clone.spawn_auxiliary_tracked(async move {
                        let answer =
                            auxiliary_handler(peer_identity, capability_token, request.params)
                                .await;
                        let (result, error) = match answer {
                            Ok(result) => (result, None),
                            Err(error) => (Value::Null, Some(error)),
                        };
                        let response = WebRTCResponse {
                            id: request.id,
                            result,
                            error,
                            // Internal scheduler marker: Browser live frames
                            // and input ACKs jump ahead of the normal sync
                            // response backlog while staying on the single,
                            // ACK-safe primary transport.
                            collection: response_collection,
                        };
                        // Browser -> native live requests use the dedicated
                        // auxiliary channel so input is never queued behind a
                        // collection transfer. Send the answer through the
                        // primary framed transport: it already provides ACK,
                        // retry and bounded backpressure, while the current
                        // Rust WebRTC auxiliary channel silently drops its
                        // native -> browser direction on the production TURN
                        // path despite reporting send success. Pending RPC ids
                        // are shared across both channels, so the browser
                        // correlates this response without a protocol change.
                        if let Err(error) = handler_task
                            .send(&peer, WebRTCWireFrame::Response(response))
                            .await
                        {
                            pool_task.error_subject.next(error);
                        }
                    });
                    continue;
                }
                if is_browser_live {
                    BROWSER_LIVE_HANDLER_MISSING.fetch_add(1, Ordering::Relaxed);
                }
                // Phase 3 demux: the `collection` field on the frame selects
                // which collection's master handler answers a plain
                // replication request. Handshake frames (`token` /
                // `ctoxProtocol`) are room-level and ignore it. When a frame
                // omits the field (V1 peers / single-collection rooms) we fall
                // back to the representative collection so the legacy
                // per-collection contract keeps working unchanged.
                //
                // Each request is answered in its own task, mirroring the
                // query/file-fetch paths above. Answering inline used to
                // head-of-line-block EVERY peer behind one slow storage call
                // or one wedged peer's send (the framed transport waits up to
                // ~90s of ack timeouts per window), which presented as
                // room-wide handshake timeouts and sync stalls. Ordering
                // stays correct: the browser serializes `masterWrite` /
                // `masterChangesSince` per collection by awaiting each answer
                // before sending the next request.
                match item.message.method.as_str() {
                    "token" | "ctoxProtocol" | "masterChangesSince" | "masterWrite" => {}
                    _ => continue,
                }
                let pool_task = Arc::clone(&pool_clone);
                let handler_task = Arc::clone(&handler);
                let representative_task = representative.clone();
                let storage_token = storage_token.clone();
                let peer_session_id = peer_session_id.clone();
                let is_peer_session_valid = is_peer_session_valid.clone();
                let request_method = item.message.method.clone();
                pool_clone.spawn_replication_request(&request_method, async move {
                    if pool_task
                        .canceled
                        .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        return;
                    }
                    let frame_collection = item.message.collection.clone();
                    if item.message.method == "ctoxProtocol" {
                        let mut may_capture_capability = true;
                        if let Some(check) = &is_peer_session_valid {
                            let remote_protocol = item
                                .message
                                .params
                                .first()
                                .unwrap_or(&Value::Null);
                            match check(remote_protocol, None) {
                                WebRTCPeerSessionValidation::Accept => {
                                    pool_task.mark_peer_admitted(&item.peer, false);
                                }
                                WebRTCPeerSessionValidation::Defer => {
                                    may_capture_capability = false;
                                }
                                WebRTCPeerSessionValidation::Reject => {
                                let response = WebRTCResponse {
                                    id: item.message.id,
                                    result: Value::Null,
                                        error: Some("peer_authentication_failed".to_string()),
                                    collection: frame_collection,
                                };
                                if let Err(error) = handler_task
                                    .send(&item.peer, WebRTCWireFrame::Response(response))
                                    .await
                                {
                                    pool_task.error_subject.next(error);
                                }
                                handler_task.close_peer(&item.peer).await;
                                return;
                                }
                            }
                        }
                        if may_capture_capability {
                            if let Some(token) = item
                            .message
                            .params
                            .first()
                            .and_then(|payload| payload.pointer("/peerSession/capabilityToken"))
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|token| !token.is_empty())
                        {
                            handler_task.set_peer_capability_token(&item.peer, token.to_string());
                            }
                        }
                    }
                    let result = match item.message.method.as_str() {
                        "token" => Value::String(storage_token),
                        "ctoxProtocol" => {
                            let flag = pool_task.query_fetch_registry.is_feature_enabled();
                            // Resolve the protocol payload for the collection
                            // the remote asked about (if it tagged one);
                            // otherwise the representative. This keeps the
                            // advertised schema hash/version aligned with the
                            // collection in play.
                            let target = frame_collection
                                .as_deref()
                                .and_then(|name| pool_task.collection_by_name(name))
                                .or_else(|| representative_task.clone());
                            let room_payload = pool_task.protocol_room_payload().await;
                            ctox_protocol_response_with_flag(
                                target.as_ref(),
                                peer_session_id.as_deref(),
                                flag,
                                room_payload.collection_schemas,
                                room_payload.collection_checkpoints,
                                Some(&storage_token),
                                handler_task.local_peer_role(),
                            )
                            .await
                        }
                        // masterChangesSince | masterWrite — route to the
                        // frame's collection master handler. An unknown
                        // collection means the remote asked about a collection
                        // this peer does not serve — answer with a
                        // replication-io error rather than silently dropping.
                        method => {
                            let target_name = frame_collection
                                .clone()
                                .or_else(|| representative_task.as_ref().map(|collection| collection.name.clone()))
                                .unwrap_or_default();
                            if !handler_task
                                .is_collection_authorized_for_peer(&item.peer, &target_name)
                            {
                                // #12c: deny pull/write of a collection this peer's
                                // role may not read (server-authoritative).
                                replication_error_result(
                                    "RC_WEBRTC_PEER",
                                    "replication-io",
                                    "unknown",
                                    serde_json::json!({
                                        "collection": target_name,
                                        "message": "peer is not authorized for collection",
                                    }),
                                    Vec::new(),
                                )
                            } else if method == "masterWrite"
                                && !handler_task.is_collection_write_authorized_for_peer(
                                    &item.peer,
                                    &target_name,
                                )
                            {
                                replication_error_result(
                                    "RC_WEBRTC_PEER",
                                    "replication-io",
                                    "push",
                                    serde_json::json!({
                                        "collection": target_name,
                                        "message": "peer is not authorized to write collection",
                                    }),
                                    Vec::new(),
                                )
                            } else if method == "masterWrite"
                                && !handler_task.are_documents_write_authorized_for_peer(
                                    &item.peer,
                                    &target_name,
                                    &item.message.params,
                                )
                            {
                                replication_error_result(
                                    "RC_WEBRTC_PEER",
                                    "replication-io",
                                    "push",
                                    serde_json::json!({
                                        "collection": target_name,
                                        "message": "peer is not authorized to write one or more documents",
                                    }),
                                    Vec::new(),
                                )
                            } else if method == "masterChangesSince"
                                && !handler_task.is_eager_collection_pull_authorized_for_peer(
                                    &item.peer,
                                    &target_name,
                                )
                            {
                                demand_only_master_pull_result(&item.message.params)
                            } else {
                                match pool_task.master_handler_for(&target_name) {
                                    Some(master) => {
                                        let _pull_permit = if method == "masterChangesSince" {
                                            pool_task.master_pull_semaphore.acquire().await.ok()
                                        } else {
                                            None
                                        };
                                        if pool_task
                                            .canceled
                                            .load(std::sync::atomic::Ordering::SeqCst)
                                        {
                                            return;
                                        }
                                        let document_filter = if method == "masterChangesSince" {
                                            handler_task
                                                .document_filter_for_peer(&item.peer, &target_name)
                                        } else {
                                            None
                                        };
                                        let mut response = call_master_method(
                                            master.as_ref(),
                                            method,
                                            item.message.params.clone(),
                                            document_filter,
                                        )
                                        .await;
                                        if let Some(fields) = handler_task.document_fields_for_peer(&item.peer, &target_name) {
                                            super::webrtc_types::mask_master_response(&mut response, &fields);
                                        }
                                        response
                                    }
                                    None => replication_error_result(
                                        "RC_WEBRTC_PEER",
                                        "replication-io",
                                        "unknown",
                                        serde_json::json!({
                                            "collection": target_name,
                                            "message": "no master handler registered for collection",
                                        }),
                                        Vec::new(),
                                    ),
                                }
                            }
                        }
                    };
                    if pool_task
                        .canceled
                        .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        return;
                    }
                    // Echo the collection back on the answer so a multiplexing
                    // browser can correlate the response without relying solely
                    // on the request id map.
                    let resp = WebRTCResponse {
                        id: item.message.id,
                        result,
                        error: None,
                        collection: frame_collection,
                    };
                    if let Err(error) = handler_task
                        .send(&item.peer, WebRTCWireFrame::Response(resp))
                        .await
                    {
                        pool_task.error_subject.next(error);
                    }
                });
            }
        });
        pool.tasks.lock().push(t);
    }

    // ref: rxdb/src/plugins/replication-webrtc/index.ts:97-221
    // On new peer: handshake once (room-level), pick master/fork (room-level
    // election), then register per-collection handlers/relays across the whole
    // multiplexed set.
    {
        let pool_clone = Arc::clone(&pool);
        let handler = Arc::clone(&connection_handler);
        let representative = representative.clone();
        let collections = collections.clone();
        let database = Arc::clone(&database);
        let storage_token = storage_token.clone();
        let request_counter = Arc::clone(&request_counter);
        let is_peer_session_valid = is_peer_session_valid.clone();
        let mut connect_stream = connection_handler.connect_stream();
        let t = tokio::spawn(async move {
            while let Some(peer) = connect_stream.next().await {
                if pool_clone
                    .canceled
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    break;
                }
                if let Some(check) = &is_peer_valid {
                    if !check(&peer) {
                        continue;
                    }
                }
                // FIX 5: spawn the per-peer handshake + master/fork build in
                // its own task. The handshake performs two full request/answer
                // round-trips (`ctoxProtocol`, then `token`) plus the fork
                // replication-state build, all awaited. Running them inline in
                // this loop meant one peer with a stalled handshake serialized
                // and blocked the bring-up of every subsequent peer. Each peer
                // now drives its own handshake concurrently; the loop only
                // dispatches.
                let pool_clone_outer = Arc::clone(&pool_clone);
                let pool_clone = Arc::clone(&pool_clone);
                let handler = Arc::clone(&handler);
                let representative = representative.clone();
                let collections = collections.clone();
                let database = Arc::clone(&database);
                let storage_token = storage_token.clone();
                let request_counter = Arc::clone(&request_counter);
                let request_flag = request_flag.clone();
                let peer_session_id = peer_session_id.clone();
                let is_peer_session_valid = is_peer_session_valid.clone();
                let tuning = tuning.clone();
                let peer_for_tracking = peer.clone();
                let handshake_task = tokio::spawn(async move {
                    if pool_clone
                        .canceled
                        .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        return;
                    }
                    // 1. CTOX protocol handshake. Rust actively reads the remote
                    // role so Browser/CTOX pairs make the same deterministic
                    // master/fork decision instead of relying on random storage
                    // token ordering after reconnects. The handshake is
                    // room-level: the representative collection's protocol payload
                    // stands in for the whole multiplexed set.
                    let req_id = {
                        let n = request_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        format!("{}|{}|{}", database.token, request_flag, n)
                    };
                    let local_flag = pool_clone.query_fetch_registry.is_feature_enabled();
                    let local_room_payload = pool_clone.protocol_room_payload().await;
                    let local_collection_schemas = local_room_payload.collection_schemas.clone();
                    let mut local_protocol = ctox_protocol_response_with_flag(
                        representative.as_ref(),
                        peer_session_id.as_deref(),
                        local_flag,
                        local_room_payload.collection_schemas,
                        local_room_payload.collection_checkpoints,
                        Some(&storage_token),
                        handler.local_peer_role(),
                    )
                    .await;
                    let device_proof_nonce = fresh_device_proof_nonce();
                    local_protocol["peerSession"]["deviceProofNonce"] =
                        Value::String(device_proof_nonce.clone());
                    let protocol_response = match send_message_and_await_answer(
                        Arc::clone(&handler),
                        peer.clone(),
                        WebRTCMessage {
                            id: req_id,
                            method: "ctoxProtocol".to_string(),
                            params: vec![local_protocol.clone()],
                            collection: None,
                        },
                    )
                    .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            pool_clone.error_subject.next(new_rx_error(
                                "RC_WEBRTC_PROTOCOL",
                                Some(serde_json::json!({ "message": e.to_string() })),
                            ));
                            // A failed handshake on a live transport used to park
                            // the peer half-dead forever (channel open, no
                            // replication, no retry). Close the transport so both
                            // sides observe a disconnect and rebuild cleanly.
                            handler.close_peer(&peer).await;
                            return;
                        }
                    };
                    if let Some(check) = &is_peer_session_valid {
                        if check(&protocol_response.result, Some(&device_proof_nonce))
                            != WebRTCPeerSessionValidation::Accept
                        {
                            pool_clone.error_subject.next(new_rx_error(
                                "RC_WEBRTC_PEER",
                                Some(serde_json::json!({
                                    "code": "peer_authentication_failed",
                                    "message": "remote peerSession or device proof is invalid",
                                })),
                            ));
                            handler.close_peer(&peer).await;
                            return;
                        }
                    }
                    // The single-collection name/hash check on `local_protocol
                    // .collection` is meaningless under multiplex (the remote's
                    // representative may differ from ours). We still enforce
                    // protocol/capability compatibility at the room level.
                    // Modern peers identify their collections by map, even when
                    // one side serves a single collection and the other many.
                    // Their representative names need not match. Preserve the
                    // original single-collection check only for legacy payloads.
                    let collection_for_validation = if multiplexed
                        || representative.is_none()
                        || protocol_response
                            .result
                            .get("collectionSchemas")
                            .and_then(Value::as_object)
                            .is_some()
                    {
                        None
                    } else {
                        local_protocol.get("collection")
                    };
                    if let Err(e) = validate_ctox_protocol_response(
                        &protocol_response.result,
                        collection_for_validation,
                        !multiplexed,
                    ) {
                        pool_clone.error_subject.next(e);
                        handler.close_peer(&peer).await;
                        return;
                    }
                    // Phase 3 schema-validation hardening: under multiplex, validate
                    // EACH collection's schema hash individually against the remote's
                    // `collectionSchemas` map. A mismatch surfaces the
                    // schemaHashMismatch error for THAT collection and excludes just
                    // it from this peer's master/fork registration — the room itself
                    // stays up and every compatible collection syncs.
                    let schema_mismatch_collections = match validate_multiplex_collection_schemas(
                        local_collection_schemas.as_ref(),
                        &protocol_response.result,
                        &pool_clone.missing_collection_schemas_warning_count,
                    ) {
                        Ok(mismatches) => mismatches,
                        Err(error) => {
                            pool_clone.error_subject.next(error);
                            handler.close_peer(&peer).await;
                            return;
                        }
                    };
                    for collection in collections.iter() {
                        if let Some(err) = schema_mismatch_error_for(
                            &schema_mismatch_collections,
                            &collection.name,
                        ) {
                            pool_clone.error_subject.next(err);
                        }
                    }
                    let remote_peer_role = protocol_response
                        .result
                        .pointer("/peerSession/role")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    // #12c: capture the peer's capability token from its handshake
                    // peerSession so the master can authorize per-collection reads
                    // for this peer. Absent/empty => the authz hook (if installed)
                    // treats it as least privilege.
                    if let Some(token) = protocol_response
                        .result
                        .pointer("/peerSession/capabilityToken")
                        .and_then(Value::as_str)
                        .filter(|token| !token.is_empty())
                    {
                        handler.set_peer_capability_token(&peer, token.to_string());
                    }
                    // Open the data plane only after PoP/session admission,
                    // protocol/schema validation, and capability capture have
                    // all completed for this handshake.
                    if is_peer_session_valid.is_some() {
                        pool_clone.mark_peer_admitted(&peer, false);
                    }

                    // 2. Token handshake.
                    let req_id = {
                        let n = request_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        format!("{}|{}|{}", database.token, request_flag, n)
                    };
                    let token_response = match send_message_and_await_answer(
                        Arc::clone(&handler),
                        peer.clone(),
                        WebRTCMessage {
                            id: req_id,
                            method: "token".to_string(),
                            params: vec![],
                            collection: None,
                        },
                    )
                    .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            pool_clone.error_subject.next(new_rx_error(
                                "RC_WEBRTC_PEER",
                                Some(serde_json::json!({ "message": e.to_string() })),
                            ));
                            handler.close_peer(&peer).await;
                            return;
                        }
                    };
                    let peer_token = token_response
                        .result
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    if peer_token.is_empty() {
                        // An empty token corrupts the master election (both sides
                        // can elect master) AND collapses the replication
                        // identifier — distinct peers would share one checkpoint
                        // meta. Treat it as a handshake failure.
                        pool_clone.error_subject.next(new_rx_error(
                        "RC_WEBRTC_PEER",
                        Some(serde_json::json!({
                            "message": "peer answered the token handshake with an empty/non-string token",
                        })),
                    ));
                        handler.close_peer(&peer).await;
                        return;
                    }
                    pool_clone.mark_peer_admitted(&peer, true);
                    let hash_fn = Arc::clone(&database.hash_function);
                    let elected_master =
                        is_master_in_webrtc_replication(hash_fn, &storage_token, &peer_token).await;
                    let is_master = if remote_peer_role == "browser" {
                        true
                    } else {
                        elected_master
                    };

                    // Phase 3: the master/fork decision is room-level, but the
                    // handlers are per-collection. Register one master-change
                    // relay (master path) or one fork replication state (fork
                    // path) PER collection multiplexed on this connection.
                    let mut peer_sub_tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();
                    let remote_collections = protocol_response
                        .result
                        .get("collectionSchemas")
                        .and_then(Value::as_object);
                    if is_master {
                        // ref: rxdb/src/plugins/replication-webrtc/index.ts:134-171
                        // Master path: relay each collection's master_change_stream
                        // tagged with its collection so the fork side can
                        // demultiplex. Method-call answers are served by the
                        // message-stream loop above.
                        for collection in collections.iter() {
                            let collection_name = collection.name.clone();
                            if remote_collections
                                .is_some_and(|schemas| !schemas.contains_key(&collection_name))
                            {
                                continue;
                            }
                            if crate::rx_collection::is_demand_only_chunk_collection_name(
                                &collection_name,
                            ) {
                                continue;
                            }
                            // Phase 3 schema-validation hardening: skip a collection
                            // whose schema mismatched the remote — no master-change
                            // relay for it, so no rows flow until reconciled.
                            if schema_mismatch_collections.contains(&collection_name) {
                                continue;
                            }
                            let Some(master) = pool_clone.master_handler_for(&collection_name)
                            else {
                                continue;
                            };
                            let handler_for_stream = Arc::clone(&handler);
                            let peer_for_stream = peer.clone();
                            let error_subject = pool_clone.error_subject.clone();
                            let stream_task = tokio::spawn(async move {
                                let mut master_stream = master.master_change_stream();
                                while let Some(ev) = master_stream.next().await {
                                    if !handler_for_stream.is_collection_active_for_peer(
                                        &peer_for_stream,
                                        &collection_name,
                                    ) && !handler_for_stream
                                        .is_inactive_live_change_authorized_for_peer(
                                            &peer_for_stream,
                                            &collection_name,
                                        )
                                    {
                                        continue;
                                    }
                                    // #12c: do not push live changes of a collection
                                    // this peer's role may not read.
                                    if !handler_for_stream.is_collection_authorized_for_peer(
                                        &peer_for_stream,
                                        &collection_name,
                                    ) {
                                        continue;
                                    }
                                    let Some(ev) = handler_for_stream
                                        .filter_master_change_for_peer(
                                            &peer_for_stream,
                                            &collection_name,
                                            ev,
                                        )
                                    else {
                                        continue;
                                    };
                                    let resp = WebRTCResponse {
                                        // Collection-qualified id avoids the
                                        // single-id collision when many
                                        // collections share one DataChannel; the
                                        // `collection` field carries the same key
                                        // for routing on the browser.
                                        id: master_change_stream_id(&collection_name),
                                        result: serde_json::to_value(&ev).unwrap_or(Value::Null),
                                        error: None,
                                        collection: Some(collection_name.clone()),
                                    };
                                    if let Err(error) = handler_for_stream
                                        .send(&peer_for_stream, WebRTCWireFrame::Response(resp))
                                        .await
                                    {
                                        publish_best_effort_send_error(&error_subject, error);
                                    }
                                }
                            });
                            peer_sub_tasks.push(stream_task);
                        }
                        pool_clone.add_peer(peer, peer_sub_tasks);
                    } else {
                        // ref: rxdb/src/plugins/replication-webrtc/index.ts:172-218
                        // Fork path: build one RxReplicationState per collection.
                        // Each tunnels collection-tagged `masterChangesSince` /
                        // `masterWrite` over the shared peer and filters the
                        // collection-qualified `masterChangeStream$` for its pull
                        // stream.
                        pool_clone.add_peer(peer.clone(), peer_sub_tasks);
                        for collection in collections.iter() {
                            if remote_collections
                                .is_some_and(|schemas| !schemas.contains_key(&collection.name))
                            {
                                continue;
                            }
                            // Phase 3 schema-validation hardening: skip building a
                            // fork replication state for a schema-mismatched
                            // collection — it stays quiesced until reconciled.
                            if schema_mismatch_collections.contains(&collection.name) {
                                continue;
                            }
                            let fork_state = build_fork_replication_state(
                                Arc::clone(collection),
                                Arc::clone(&handler),
                                peer.clone(),
                                peer_token.clone(),
                                Arc::clone(&request_counter),
                                request_flag.clone(),
                                tuning.clone(),
                            )
                            .await;
                            match fork_state {
                                Ok(state) => {
                                    // Mirror fork pull/push errors (RC_PULL /
                                    // RC_PUSH retry loops) onto the pool error
                                    // stream — they were previously dropped
                                    // unobserved, which made stalled collections
                                    // undiagnosable. The relay is registered as a
                                    // peer sub-task so it dies with the peer.
                                    let pool_for_errors = Arc::clone(&pool_clone);
                                    let mut fork_errors = state.error_stream();
                                    let err_task = tokio::spawn(async move {
                                        while let Some(e) = fork_errors.next().await {
                                            pool_for_errors.error_subject.next(e);
                                        }
                                    });
                                    pool_clone.add_peer(peer.clone(), vec![err_task]);
                                    pool_clone
                                        .add_fork_state(
                                            collection.name.clone(),
                                            peer.clone(),
                                            state,
                                        )
                                        .await;
                                }
                                Err(e) => {
                                    pool_clone.error_subject.next(e);
                                }
                            }
                        }
                    }
                });
                // Track the handshake task on the PEER, not the pool: when
                // the peer disconnects mid-handshake, `remove_peer` aborts it
                // — a late-completing handshake used to register relays/fork
                // states for an already-dead peer (zombie state that retried
                // forever). Per-peer tracking also stops the unbounded
                // `pool.tasks` growth across reconnect churn.
                pool_clone_outer.add_peer(peer_for_tracking, vec![handshake_task]);
            }
        });
        pool.tasks.lock().push(t);
    }

    // Production visibility: every error funneled into the pool subject
    // (transport, protocol/handshake, fork pull/push) gets at least a log
    // line. RxSubject drops values with no subscriber, so without this the
    // entire failure surface of the replication layer was silent.
    {
        let mut log_stream = pool.error_subject.subscribe();
        let t = tokio::spawn(async move {
            while let Some(err) = log_stream.next().await {
                tracing::warn!(
                    target: "ctox_rxdb::replication_webrtc",
                    "webrtc replication error: {err}"
                );
            }
        });
        pool.tasks.lock().push(t);
    }

    Ok(pool)
}

fn fresh_device_proof_nonce() -> String {
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Phase 3: the collection-qualified `masterChangeStream$` response id. Each
/// collection's master-change relay emits under its own id so a single
/// DataChannel can carry every collection's live stream without the responses
/// colliding. The browser builds the identical id to route inbound
/// master-change events to the right collection's pull.
pub fn master_change_stream_id(collection: &str) -> String {
    format!("masterChangeStream$:{collection}")
}

/// Phase 3 schema-validation hardening: build the per-collection schema-hash
/// map (`{ name -> { schemaVersion, schemaHash, schemaHashSource } }`) for
/// every collection multiplexed on the connection. Returned as a `Value` to
/// attach to the `ctoxProtocol` handshake payload so the browser can validate
/// EACH collection individually under multiplex instead of skipping schema
/// validation wholesale. Zero, one and multiple collections use the same
/// explicit map; omitting it would make a modern peer fail its own schema gate.
async fn collection_schemas_payload(collections: &[Arc<RxCollection>]) -> Value {
    let mut map = serde_json::Map::with_capacity(collections.len());
    for collection in collections.iter() {
        if let Some(schema) = &collection.schema {
            map.insert(
                collection.name.clone(),
                serde_json::json!({
                    "schemaVersion": schema.version(),
                    "schemaHash": schema.hash().await,
                    "schemaHashSource": CTOX_RXDB_RS_SCHEMA_HASH_SOURCE,
                }),
            );
        }
    }
    Value::Object(map)
}

/// Phase 3 multiplex: per-collection checkpoint-status map for the room
/// handshake. The single room-level `ctoxProtocol` answer used to carry only
/// the REPRESENTATIVE collection's checkpoint; every other collection that
/// derived its protocol from the room handshake then advertised a checkpoint
/// epoch belonging to a different collection (visible as
/// `checkpoint.collection != session.collection` in the browser's
/// peer-session evidence after a native restart). Always explicit, including
/// single-collection rooms and control-only peers.
async fn collection_checkpoints_payload(collections: &[Arc<RxCollection>]) -> Value {
    let mut map = serde_json::Map::with_capacity(collections.len());
    for collection in collections.iter() {
        let checkpoint = collection
            .storage_instance
            .replication_checkpoint_status()
            .await;
        if !checkpoint.is_null() {
            map.insert(collection.name.clone(), checkpoint);
        }
    }
    Value::Object(map)
}

async fn ctox_protocol_response_with_flag(
    collection: Option<&Arc<RxCollection>>,
    peer_session_id: Option<&str>,
    query_demand_loading_enabled: bool,
    collection_schemas: Option<Value>,
    collection_checkpoints: Option<Value>,
    storage_generation: Option<&str>,
    peer_role: NativePeerRole,
) -> Value {
    let collection_payload = if let Some(collection) = collection {
        let checkpoint = collection
            .storage_instance
            .replication_checkpoint_status()
            .await;
        match &collection.schema {
            Some(schema) => serde_json::json!({
                "name": collection.name,
                "schemaVersion": schema.version(),
                "schemaHash": schema.hash().await,
                "schemaHashSource": CTOX_RXDB_RS_SCHEMA_HASH_SOURCE,
                "checkpoint": checkpoint,
            }),
            None => Value::Null,
        }
    } else {
        Value::Null
    };
    ctox_protocol_response_payload_with_flag(
        collection_payload,
        peer_session_id,
        query_demand_loading_enabled,
        collection_schemas,
        collection_checkpoints,
        storage_generation,
        peer_role,
    )
}

#[cfg(test)]
fn ctox_protocol_response_payload(collection: Value, peer_session_id: Option<&str>) -> Value {
    ctox_protocol_response_payload_with_flag(
        collection,
        peer_session_id,
        true,
        None,
        None,
        None,
        NativePeerRole::CtoxInstance,
    )
}

fn ctox_protocol_response_payload_with_flag(
    collection: Value,
    peer_session_id: Option<&str>,
    query_demand_loading_enabled: bool,
    collection_schemas: Option<Value>,
    collection_checkpoints: Option<Value>,
    storage_generation: Option<&str>,
    peer_role: NativePeerRole,
) -> Value {
    let peer_session_id = peer_session_id
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("rxdb-rs-{}", random_token(Some(16))));
    // V1.5 production hardening: the server advertises whether demand-loading
    // is currently enabled. Browsers that see `queryDemandLoadingEnabled: false`
    // MUST fall back to the V1 replication path even when they themselves
    // carry the capability. This is the runtime feature-flag handshake.
    let advertised_capabilities: Vec<&str> = CTOX_RXDB_NATIVE_CAPABILITIES
        .iter()
        .copied()
        .filter(|cap| {
            // Strip the query-fetch capability when the flag is off so a
            // V1.5-aware browser also treats this peer as V1.
            *cap != CTOX_QUERY_FETCH_CAPABILITY || query_demand_loading_enabled
        })
        .collect();
    let mut payload = serde_json::json!({
        "protocol": CTOX_RXDB_PROTOCOL,
        "capabilities": advertised_capabilities,
        "collection": collection,
        "peerSession": {
            "role": peer_role.as_str(),
            "sessionId": peer_session_id,
        },
        "v1_5": {
            "queryDemandLoadingEnabled": query_demand_loading_enabled,
        },
        "storageGeneration": storage_generation.filter(|value| !value.trim().is_empty()),
        "nativeTimeMs": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    });
    // Phase 3 schema-validation hardening: attach the per-collection schema-hash
    // map under multiplex so the browser validates each collection's schema
    // individually. Omitted entirely (key absent) for single-collection rooms.
    if let Some(schemas) = collection_schemas {
        payload["collectionSchemas"] = schemas;
    }
    // Phase 3 multiplex: per-collection checkpoint epochs, so a collection
    // deriving its protocol from the room handshake advertises ITS OWN
    // checkpoint evidence (the browser prefers `collectionCheckpoints` in
    // `remoteProtocolForCollection`).
    if let Some(checkpoints) = collection_checkpoints {
        payload["collectionCheckpoints"] = checkpoints;
    }
    payload
}

fn validate_ctox_protocol_response(
    value: &Value,
    local_collection: Option<&Value>,
    validate_schema: bool,
) -> Result<(), RxError> {
    let protocol = value
        .get("protocol")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if protocol.is_empty() {
        return Err(ctox_protocol_error(
            CTOX_PROTOCOL_ERROR_MISSING,
            "CTOX RxDB WebRTC protocol marker is missing",
            Some(Value::String(CTOX_RXDB_PROTOCOL.to_string())),
            Some(Value::Null),
            None,
        ));
    }
    if protocol != CTOX_RXDB_PROTOCOL {
        return Err(ctox_protocol_error(
            CTOX_PROTOCOL_ERROR_MISMATCH,
            "incompatible CTOX RxDB WebRTC protocol",
            Some(Value::String(CTOX_RXDB_PROTOCOL.to_string())),
            Some(Value::String(protocol.to_string())),
            None,
        ));
    }
    let capabilities = value
        .get("capabilities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for capability in CTOX_REQUIRED_PROTOCOL_CAPABILITIES {
        let has_capability = capabilities
            .iter()
            .any(|item| item.as_str() == Some(*capability));
        if !has_capability {
            return Err(ctox_protocol_error(
                CTOX_PROTOCOL_ERROR_CAPABILITY_MISSING,
                "remote CTOX RxDB peer is missing a required capability",
                Some(Value::String((*capability).to_string())),
                Some(Value::Array(capabilities)),
                None,
            ));
        }
    }
    if let (Some(local), Some(remote)) = (local_collection, value.get("collection")) {
        let local_name = local
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let remote_name = remote
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !local_name.is_empty() && !remote_name.is_empty() && local_name != remote_name {
            return Err(ctox_protocol_error(
                CTOX_PROTOCOL_ERROR_COLLECTION_MISMATCH,
                "CTOX RxDB collection mismatch",
                Some(Value::String(local_name.to_string())),
                Some(Value::String(remote_name.to_string())),
                Some(local_name.to_string()),
            ));
        }
        let local_version = local.get("schemaVersion").and_then(Value::as_i64);
        let remote_version = remote.get("schemaVersion").and_then(Value::as_i64);
        if let (Some(expected), Some(actual)) = (local_version, remote_version) {
            if expected != actual {
                if !validate_schema {
                    return Ok(());
                }
                return Err(ctox_protocol_error(
                    CTOX_PROTOCOL_ERROR_SCHEMA_VERSION_MISMATCH,
                    "CTOX RxDB schema version mismatch",
                    Some(Value::Number(expected.into())),
                    Some(Value::Number(actual.into())),
                    non_empty(local_name).map(str::to_string),
                ));
            }
        }
        let local_hash = local
            .get("schemaHash")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let remote_hash = remote
            .get("schemaHash")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if validate_schema
            && !local_hash.is_empty()
            && !remote_hash.is_empty()
            && local_hash != remote_hash
        {
            return Err(ctox_protocol_error(
                CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH,
                "CTOX RxDB schema hash mismatch",
                Some(Value::String(local_hash.to_string())),
                Some(Value::String(remote_hash.to_string())),
                non_empty(local_name).map(str::to_string),
            ));
        }
    }
    Ok(())
}

fn remote_is_multiplex_schema_capable(remote_protocol: &Value) -> bool {
    let has_checkpoint_generation_capability = remote_protocol
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|capabilities| {
            capabilities.iter().any(|capability| {
                capability.as_str() == Some(CTOX_CHECKPOINT_GENERATION_CAPABILITY)
            })
        })
        .unwrap_or(false);

    // The schema-hash capability predates multiplexing and cannot distinguish a
    // legacy peer. These checkpoint/clock fields belong to the multiplex-aware
    // payload shape, whose invariant is to carry collectionSchemas as well.
    has_checkpoint_generation_capability
        || ["collectionCheckpoints", "storageGeneration", "nativeTimeMs"]
            .iter()
            .any(|field| remote_protocol.get(*field).is_some())
}

fn validate_multiplex_collection_schemas(
    local_collection_schemas: Option<&Value>,
    remote_protocol: &Value,
    missing_warning_count: &std::sync::atomic::AtomicU64,
) -> Result<std::collections::HashSet<String>, RxError> {
    if remote_protocol
        .get("collectionSchemas")
        .and_then(Value::as_object)
        .is_none()
    {
        let remote_schema_capable = remote_is_multiplex_schema_capable(remote_protocol);
        let warning_count =
            missing_warning_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let disposition = if remote_schema_capable {
            "rejecting schema-capable peer"
        } else {
            "allowing legacy peer"
        };
        tracing::warn!(
            target: "ctox_rxdb::replication_webrtc",
            warning_count,
            remote_schema_capable,
            disposition,
            "multiplex WebRTC handshake omitted collectionSchemas"
        );
        if remote_schema_capable {
            return Err(ctox_protocol_error(
                CTOX_PROTOCOL_ERROR_CAPABILITY_MISSING,
                "schema-capable CTOX RxDB peer omitted collectionSchemas in multiplex handshake",
                Some(Value::String("collectionSchemas object".to_string())),
                Some(
                    remote_protocol
                        .get("collectionSchemas")
                        .cloned()
                        .unwrap_or(Value::Null),
                ),
                None,
            ));
        }
    }

    Ok(collect_collection_schema_mismatches(
        local_collection_schemas,
        remote_protocol,
    ))
}

/// Phase 3 schema-validation hardening: compare the local `collectionSchemas`
/// map against the remote protocol response's `collectionSchemas` map and
/// return the set of collection names whose schema hash / version mismatched.
/// Collections the remote does not advertise are NOT flagged (it simply does
/// not serve them on this connection). This is the native counterpart to the
/// browser's `assertCollectionSchemasCompatible`.
fn collect_collection_schema_mismatches(
    local_collection_schemas: Option<&Value>,
    remote_protocol: &Value,
) -> std::collections::HashSet<String> {
    let mut mismatches = std::collections::HashSet::new();
    let Some(local_map) = local_collection_schemas.and_then(Value::as_object) else {
        return mismatches;
    };
    let remote_map = remote_protocol
        .get("collectionSchemas")
        .and_then(Value::as_object);
    let Some(remote_map) = remote_map else {
        // Only the wrapper's legacy branch may reach this fallback; a peer with
        // multiplex-aware markers is rejected before collection paths exist.
        return mismatches;
    };
    for (name, local_entry) in local_map.iter() {
        let Some(remote_entry) = remote_map.get(name) else {
            continue;
        };
        let local_version = local_entry.get("schemaVersion").and_then(Value::as_i64);
        let remote_version = remote_entry.get("schemaVersion").and_then(Value::as_i64);
        if let (Some(expected), Some(actual)) = (local_version, remote_version) {
            if expected != actual {
                mismatches.insert(name.clone());
                continue;
            }
        }
        let local_hash = local_entry
            .get("schemaHash")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let remote_hash = remote_entry
            .get("schemaHash")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !local_hash.is_empty() && !remote_hash.is_empty() && local_hash != remote_hash {
            mismatches.insert(name.clone());
        }
    }
    mismatches
}

/// Build the `schemaHashMismatch` error for a collection that was flagged by
/// [`collect_collection_schema_mismatches`], or `None` when the collection is
/// compatible. Surfaces the same error code as the single-collection path so
/// existing browser/native error handling treats it identically.
fn schema_mismatch_error_for(
    mismatches: &std::collections::HashSet<String>,
    collection_name: &str,
) -> Option<RxError> {
    if !mismatches.contains(collection_name) {
        return None;
    }
    Some(ctox_protocol_error(
        CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH,
        "CTOX RxDB schema hash mismatch",
        None,
        None,
        Some(collection_name.to_string()),
    ))
}

fn non_empty(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn ctox_protocol_error(
    code: &str,
    message: &str,
    expected: Option<Value>,
    actual: Option<Value>,
    collection: Option<String>,
) -> RxError {
    let mut params = serde_json::json!({
        "type": "ctoxError",
        "name": "CtoxRxdbProtocolError",
        "code": code,
        "message": message,
        "phase": "rxdb-protocol-handshake",
        "retryable": false,
    });
    if let Some(expected) = expected {
        params["expected"] = expected;
    }
    if let Some(actual) = actual {
        params["actual"] = actual;
    }
    if let Some(collection) = collection {
        params["collection"] = Value::String(collection);
    }
    new_rx_error("RC_WEBRTC_PROTOCOL", Some(params))
}

// ref: rxdb/src/plugins/replication-webrtc/index.ts:172-218
/// Construct an `RxReplicationState` for the fork-path branch of a single
/// peer connection. Pull/push handlers tunnel calls over the WebRTC peer.
async fn build_fork_replication_state<H>(
    collection: Arc<RxCollection>,
    handler: Arc<H>,
    peer: H::Peer,
    peer_token: String,
    request_counter: Arc<std::sync::atomic::AtomicU64>,
    request_flag: String,
    tuning: WebRTCReplicationTuning,
) -> Result<Arc<RxReplicationState>, RxError>
where
    H: WebRTCConnectionHandler + 'static,
{
    let db_token = collection.database.token.clone();
    // Phase 3: every frame this fork emits is tagged with its collection so the
    // remote master's demux loop routes it to the right master handler, and so
    // the master-change stream filter below matches the right qualified id.
    let collection_name = collection.name.clone();
    let next_request_id = {
        let counter = Arc::clone(&request_counter);
        let flag = request_flag.clone();
        let token = db_token.clone();
        Arc::new(move || {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            format!("{}|{}|{}", token, flag, n)
        })
    };

    let pull_handler: PullHandler = {
        let handler = Arc::clone(&handler);
        let peer = peer.clone();
        let next_request_id = Arc::clone(&next_request_id);
        let collection_name = collection_name.clone();
        Arc::new(move |checkpoint, batch_size| {
            let handler = Arc::clone(&handler);
            let peer = peer.clone();
            let next_request_id = Arc::clone(&next_request_id);
            let collection_name = collection_name.clone();
            Box::pin(async move {
                let id = next_request_id();
                let answer = send_message_and_await_answer(
                    handler,
                    peer,
                    WebRTCMessage {
                        id,
                        method: "masterChangesSince".to_string(),
                        params: vec![
                            checkpoint.clone().unwrap_or(Value::Null),
                            Value::from(batch_size),
                        ],
                        collection: Some(collection_name),
                    },
                )
                .await?;
                if let Some(error) = replication_error_from_webrtc_result(&answer.result) {
                    return Err(error);
                }
                let docs_cp: DocumentsWithCheckpoint =
                    serde_json::from_value(answer.result.clone()).map_err(|e| {
                        new_rx_error(
                            "RC_PULL",
                            Some(serde_json::json!({
                                "direction": "pull",
                                "phase": "replication-pull",
                                "checkpoint": checkpoint,
                                "batchSize": batch_size,
                                "message": format!("fork masterChangesSince decode: {}", e),
                            })),
                        )
                    })?;
                Ok(ReplicationPullHandlerResult {
                    documents: docs_cp.documents,
                    checkpoint: docs_cp.checkpoint,
                })
            })
        })
    };

    let push_handler: PushHandler = {
        let handler = Arc::clone(&handler);
        let peer = peer.clone();
        let next_request_id = Arc::clone(&next_request_id);
        let collection_name = collection_name.clone();
        Arc::new(move |rows| {
            let handler = Arc::clone(&handler);
            let peer = peer.clone();
            let next_request_id = Arc::clone(&next_request_id);
            let collection_name = collection_name.clone();
            Box::pin(async move {
                let id = next_request_id();
                let rows_json = serde_json::to_value(&rows).unwrap_or(Value::Null);
                let answer = send_message_and_await_answer(
                    handler,
                    peer,
                    WebRTCMessage {
                        id,
                        method: "masterWrite".to_string(),
                        params: vec![rows_json.clone()],
                        collection: Some(collection_name),
                    },
                )
                .await?;
                if let Some(error) = replication_error_from_webrtc_result(&answer.result) {
                    return Err(error);
                }
                let conflicts: Vec<Value> =
                    serde_json::from_value(answer.result.clone()).map_err(|e| {
                        new_rx_error(
                            "RC_PUSH_NO_AR",
                            Some(serde_json::json!({
                                "direction": "push",
                                "phase": "replication-push",
                                "pushRows": rows_json,
                                "message": format!("fork masterWrite decode: {}", e),
                            })),
                        )
                    })?;
                Ok(conflicts)
            })
        })
    };

    let stream_factory: StreamFactory = {
        let handler = Arc::clone(&handler);
        let peer = peer.clone();
        let collection_name = collection_name.clone();
        Arc::new(move || {
            let handler = Arc::clone(&handler);
            let peer_for_filter = peer.clone();
            // Phase 3: match the collection-qualified `masterChangeStream$` id
            // so this fork's pull only sees its own collection's live changes
            // off the shared DataChannel. We accept either the qualified id or
            // the explicit `collection` field (whichever the remote set), and
            // tolerate the legacy bare id for single-collection / V1 peers.
            let qualified_id = master_change_stream_id(&collection_name);
            let collection_name = collection_name.clone();
            let remote_master_events = handler.response_stream().filter_map(move |item| {
                if item.peer != peer_for_filter {
                    return None;
                }
                let matches_qualified = item.response.id == qualified_id;
                let matches_field = item.response.collection.as_deref() == Some(&collection_name);
                let matches_legacy =
                    item.response.id == "masterChangeStream$" && item.response.collection.is_none();
                if !(matches_qualified || matches_field || matches_legacy) {
                    return None;
                }
                serde_json::from_value::<RxReplicationMasterChange>(item.response.result.clone())
                    .ok()
            });
            let periodic_resync = periodic_resync_stream(FORK_RESYNC_INTERVAL);
            Box::pin(futures::stream::select(
                remote_master_events,
                periodic_resync,
            ))
        })
    };

    let pull = ReplicationPullOptions {
        handler: pull_handler,
        stream_factory: Some(stream_factory),
        batch_size: tuning.pull_batch_size,
        modifier: None,
        initial_checkpoint: None,
    };
    let push = ReplicationPushOptions {
        handler: push_handler,
        batch_size: tuning.push_batch_size,
        modifier: None,
        initial_checkpoint: None,
    };
    let replication_identifier = if let Some(topic) = tuning.topic.as_deref() {
        format!("{}||{}||{}", collection.name, topic, peer_token)
    } else {
        format!("{}||{}", collection.name, peer_token)
    };

    let opts = ReplicationOptions {
        replication_identifier,
        collection,
        deleted_field: "_deleted".to_string(),
        pull: Some(pull),
        push: Some(push),
        live: true,
        retry_time: tuning.retry_time,
        // The pool starts this only after the displaced connection's state stops.
        auto_start: false,
        wait_for_leadership: false,
    };

    replicate_rx_collection(opts).await
}

fn periodic_resync_stream(
    interval: Duration,
) -> crate::rxjs_compat::RxStream<RxReplicationMasterChange> {
    Box::pin(futures::stream::unfold((), move |_| async move {
        tokio::time::sleep(interval).await;
        Some((RxReplicationMasterChange::Resync, ()))
    }))
}

fn demand_only_master_pull_result(params: &[Value]) -> Value {
    serde_json::json!({
        "documents": [],
        "checkpoint": params.first().cloned().unwrap_or(Value::Null),
        "demandOnly": true,
    })
}

/// Dispatch a method call on the master replication handler.
async fn call_master_method(
    handler: &dyn RxReplicationHandler,
    method: &str,
    params: Vec<Value>,
    document_filter: Option<WebRTCDocumentFilter>,
) -> Value {
    match method {
        "masterChangesSince" => {
            let checkpoint = params.first().cloned();
            let batch_size = params.get(1).and_then(|v| v.as_u64()).unwrap_or(20);
            let normalized_checkpoint = match checkpoint {
                Some(Value::Null) | None => None,
                other => other,
            };
            match handler
                .master_changes_since(normalized_checkpoint.clone(), batch_size)
                .await
            {
                Ok(mut result) => {
                    if let Some(filter) = document_filter {
                        result.documents.retain(|document| filter(document));
                    }
                    serde_json::to_value(&result).unwrap_or_else(|e| {
                        replication_error_result(
                            "RC_PULL",
                            "replication-pull",
                            "pull",
                            serde_json::json!({
                                "checkpoint": normalized_checkpoint,
                                "batchSize": batch_size,
                                "message": format!("masterChangesSince encode: {}", e),
                            }),
                            Vec::new(),
                        )
                    })
                }
                Err(error) => replication_error_result(
                    "RC_PULL",
                    "replication-pull",
                    "pull",
                    serde_json::json!({
                        "checkpoint": normalized_checkpoint,
                        "batchSize": batch_size,
                    }),
                    vec![rx_error_to_value(&error)],
                ),
            }
        }
        "masterWrite" => {
            let rows_value = params.first().cloned().unwrap_or(Value::Null);
            let rows: Vec<crate::types::RxReplicationWriteToMasterRow> =
                match serde_json::from_value(rows_value.clone()) {
                    Ok(rows) => rows,
                    Err(error) => {
                        return replication_error_result(
                            "RC_PUSH",
                            "replication-push",
                            "push",
                            serde_json::json!({
                                "pushRows": rows_value,
                                "message": format!("masterWrite request decode: {}", error),
                            }),
                            Vec::new(),
                        );
                    }
                };
            let row_count = rows.len();
            match handler.master_write(rows).await {
                Ok(conflicts) => serde_json::to_value(&conflicts).unwrap_or_else(|e| {
                    replication_error_result(
                        "RC_PUSH",
                        "replication-push",
                        "push",
                        serde_json::json!({
                            "rowCount": row_count,
                            "message": format!("masterWrite encode: {}", e),
                        }),
                        Vec::new(),
                    )
                }),
                Err(error) => replication_error_result(
                    "RC_PUSH",
                    "replication-push",
                    "push",
                    serde_json::json!({
                        "rowCount": row_count,
                    }),
                    vec![rx_error_to_value(&error)],
                ),
            }
        }
        _ => {
            tracing::warn!(
                target: "ctox_rxdb::plugins::replication_webrtc",
                "unknown method on master handler: {method}",
            );
            replication_error_result(
                "RC_WEBRTC_PEER",
                "replication-io",
                "unknown",
                serde_json::json!({
                    "method": method,
                    "message": "unknown WebRTC master method",
                }),
                Vec::new(),
            )
        }
    }
}

fn replication_error_result(
    code: &str,
    phase: &str,
    direction: &str,
    details: Value,
    errors: Vec<Value>,
) -> Value {
    let mut payload = serde_json::json!({
        "type": "ctoxError",
        "scope": "replication",
        "rxdb": true,
        "code": code,
        "phase": phase,
        "direction": direction,
    });
    if let Some(object) = payload.as_object_mut() {
        if let Some(details_object) = details.as_object() {
            for (key, value) in details_object {
                object.insert(key.clone(), value.clone());
            }
        }
        if !errors.is_empty() {
            object.insert("errors".to_string(), Value::Array(errors));
        }
    }
    payload
}

fn replication_error_from_webrtc_result(result: &Value) -> Option<RxError> {
    let object = result.as_object()?;
    let is_replication_error = object.get("type").and_then(Value::as_str) == Some("ctoxError")
        && object.get("scope").and_then(Value::as_str) == Some("replication")
        && object.get("rxdb").and_then(Value::as_bool).unwrap_or(false);
    if !is_replication_error {
        return None;
    }
    let code = object
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("RC_WEBRTC_PEER");
    Some(new_rx_error(code, Some(result.clone())))
}

fn rx_error_to_value(error: &RxError) -> Value {
    serde_json::json!({
        "rxdb": true,
        "code": error.code(),
        "name": error.name(),
        "message": error.to_string(),
        "parameters": error.parameters(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auxiliary_response_priority_marker_covers_latency_sensitive_foreground_reads() {
        assert_eq!(
            auxiliary_response_collection(BROWSER_LIVE_METHOD, "input").as_deref(),
            Some(CTOX_BROWSER_INPUT_RESPONSE_COLLECTION)
        );
        assert_eq!(
            auxiliary_response_collection(BROWSER_LIVE_METHOD, "session.list").as_deref(),
            Some(CTOX_BROWSER_LIVE_RESPONSE_COLLECTION)
        );
        assert_eq!(
            auxiliary_response_collection(OUTBOUND_SELLIFY_LOOKUP_METHOD, "live").as_deref(),
            Some(OUTBOUND_SELLIFY_RESPONSE_COLLECTION)
        );
        assert_eq!(
            auxiliary_response_collection("ctox.unknown.v1", "live"),
            None
        );
    }

    /// Backlog OS-G1: the error-classification corpus
    /// (tests/fixtures/replication-error-classification.json) is consumed by
    /// the browser suite to pin the classification cascade. This side pins
    /// the PRODUCER: every `ctox_rxdb_*` code the corpus expects must be one
    /// of the generated protocol-contract error codes this crate emits — a
    /// one-sided rename (fixture regenerated, corpus forgotten, or vice
    /// versa) fails here instead of shipping as unclassifiable errors.
    #[test]
    fn error_classification_corpus_codes_match_protocol_contract() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/replication-error-classification.json"
        ))
        .expect("corpus fixture must parse");
        let contract_codes = [
            protocol_contract_generated::CTOX_PROTOCOL_ERROR_MISSING,
            protocol_contract_generated::CTOX_PROTOCOL_ERROR_MISMATCH,
            protocol_contract_generated::CTOX_PROTOCOL_ERROR_CAPABILITY_MISSING,
            protocol_contract_generated::CTOX_PROTOCOL_ERROR_COLLECTION_MISMATCH,
            protocol_contract_generated::CTOX_PROTOCOL_ERROR_SCHEMA_VERSION_MISMATCH,
            protocol_contract_generated::CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH,
        ];
        let cases = corpus
            .get("cases")
            .and_then(serde_json::Value::as_array)
            .expect("corpus must carry cases");
        assert!(cases.len() >= 10, "corpus must not silently shrink");
        let mut checked = 0usize;
        for case in cases {
            let Some(code) = case.get("expectedCode").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !code.starts_with("ctox_rxdb_") {
                continue;
            }
            assert!(
                contract_codes.contains(&code),
                "corpus expects `{code}`, which is not a generated protocol \
                 contract error code — regenerate the contract or fix the corpus"
            );
            checked += 1;
        }
        assert!(
            checked >= 1,
            "corpus must cover at least one generated ctox_rxdb_* code"
        );
    }

    #[test]
    fn demand_only_pull_finishes_without_reading_master_storage() {
        let checkpoint = serde_json::json!({ "id": "last", "lwt": 42 });
        let result = demand_only_master_pull_result(&[checkpoint.clone(), Value::from(20)]);
        assert_eq!(result.get("documents"), Some(&serde_json::json!([])));
        assert_eq!(result.get("checkpoint"), Some(&checkpoint));
        assert_eq!(
            result.get("demandOnly").and_then(Value::as_bool),
            Some(true)
        );
    }

    struct StaticReplicationHandler {
        pull_error: Option<RxError>,
        push_error: Option<RxError>,
    }

    #[async_trait::async_trait]
    impl RxReplicationHandler for StaticReplicationHandler {
        fn master_change_stream(&self) -> crate::rxjs_compat::RxStream<RxReplicationMasterChange> {
            Box::pin(futures::stream::empty())
        }

        async fn master_changes_since(
            &self,
            _checkpoint: Option<Value>,
            _batch_size: u64,
        ) -> Result<DocumentsWithCheckpoint, RxError> {
            if let Some(error) = &self.pull_error {
                return Err(error.clone());
            }
            Ok(DocumentsWithCheckpoint {
                documents: vec![serde_json::json!({ "id": "alice", "_deleted": false })],
                checkpoint: serde_json::json!({ "sequence": 2 }),
            })
        }

        async fn master_write(
            &self,
            _rows: Vec<crate::types::RxReplicationWriteToMasterRow>,
        ) -> Result<Vec<Value>, RxError> {
            if let Some(error) = &self.push_error {
                return Err(error.clone());
            }
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn periodic_resync_stream_emits_resync() {
        let mut stream = periodic_resync_stream(Duration::from_millis(1));
        let item = tokio::time::timeout(Duration::from_millis(100), stream.next())
            .await
            .expect("periodic resync should emit before timeout");
        assert_eq!(item, Some(RxReplicationMasterChange::Resync));
    }

    #[tokio::test]
    async fn call_master_method_wraps_master_changes_since_errors() {
        let handler = StaticReplicationHandler {
            pull_error: Some(new_rx_error(
                "TEST_PULL",
                Some(serde_json::json!({ "attempt": 1 })),
            )),
            push_error: None,
        };

        let result = call_master_method(
            &handler,
            "masterChangesSince",
            vec![serde_json::json!({ "sequence": 1 }), Value::from(10)],
            None,
        )
        .await;

        assert_eq!(result["type"], serde_json::json!("ctoxError"));
        assert_eq!(result["scope"], serde_json::json!("replication"));
        assert_eq!(result["code"], serde_json::json!("RC_PULL"));
        assert_eq!(result["direction"], serde_json::json!("pull"));
        assert_eq!(result["phase"], serde_json::json!("replication-pull"));
        assert_eq!(result["checkpoint"], serde_json::json!({ "sequence": 1 }));
        assert_eq!(result["batchSize"], serde_json::json!(10));
        assert_eq!(result["errors"][0]["code"], serde_json::json!("TEST_PULL"));
        assert_eq!(result["errors"][0]["rxdb"], serde_json::json!(true));

        let error = replication_error_from_webrtc_result(&result)
            .expect("replication error result should become RxError");
        assert_eq!(error.code(), "RC_PULL");
        assert_eq!(error.parameters()["direction"], serde_json::json!("pull"));
    }

    #[tokio::test]
    async fn call_master_method_filters_master_changes_since_documents() {
        let handler = StaticReplicationHandler {
            pull_error: None,
            push_error: None,
        };

        let result = call_master_method(
            &handler,
            "masterChangesSince",
            vec![Value::Null, Value::from(10)],
            Some(Arc::new(|document: &Value| {
                document.get("id").and_then(Value::as_str) == Some("bob")
            })),
        )
        .await;

        assert_eq!(result["documents"], serde_json::json!([]));
        assert_eq!(result["checkpoint"], serde_json::json!({ "sequence": 2 }));
    }

    #[tokio::test]
    async fn call_master_method_wraps_master_write_errors() {
        let handler = StaticReplicationHandler {
            pull_error: None,
            push_error: Some(new_rx_error(
                "TEST_PUSH",
                Some(serde_json::json!({ "attempt": 1 })),
            )),
        };

        let result = call_master_method(
            &handler,
            "masterWrite",
            vec![serde_json::json!([{
                "newDocumentState": { "id": "alice", "_deleted": false },
                "assumedMasterState": { "id": "alice", "_deleted": false }
            }])],
            None,
        )
        .await;

        assert_eq!(result["type"], serde_json::json!("ctoxError"));
        assert_eq!(result["scope"], serde_json::json!("replication"));
        assert_eq!(result["code"], serde_json::json!("RC_PUSH"));
        assert_eq!(result["direction"], serde_json::json!("push"));
        assert_eq!(result["phase"], serde_json::json!("replication-push"));
        assert_eq!(result["rowCount"], serde_json::json!(1));
        assert_eq!(result["errors"][0]["code"], serde_json::json!("TEST_PUSH"));
        assert_eq!(result["errors"][0]["rxdb"], serde_json::json!(true));

        let error = replication_error_from_webrtc_result(&result)
            .expect("replication error result should become RxError");
        assert_eq!(error.code(), "RC_PUSH");
        assert_eq!(error.parameters()["direction"], serde_json::json!("push"));
    }

    #[tokio::test]
    async fn call_master_method_rejects_invalid_master_write_rows() {
        let handler = StaticReplicationHandler {
            pull_error: None,
            push_error: None,
        };

        let result = call_master_method(
            &handler,
            "masterWrite",
            vec![serde_json::json!({ "newDocumentState": { "id": "not-an-array" } })],
            None,
        )
        .await;

        assert_eq!(result["type"], serde_json::json!("ctoxError"));
        assert_eq!(result["code"], serde_json::json!("RC_PUSH"));
        assert_eq!(result["direction"], serde_json::json!("push"));
        assert!(result["message"]
            .as_str()
            .unwrap_or_default()
            .contains("masterWrite request decode"));
    }

    #[test]
    fn ctox_protocol_response_advertises_native_capabilities() {
        let payload = ctox_protocol_response_payload(
            serde_json::json!({
                "name": "desktop_files",
                "schemaVersion": 0,
                "schemaHash": "schema-hash-1",
                "schemaHashSource": CTOX_RXDB_RS_SCHEMA_HASH_SOURCE,
                "checkpoint": {
                    "source": "rxdb-rs-sqlite",
                    "state": "advertised",
                    "collection": "desktop_files",
                    "schemaHash": "schema-hash-1",
                    "latestLwt": 0,
                    "latestIdHash": "",
                    "epoch": "checkpoint-epoch-1"
                }
            }),
            Some("rxdb-rs-test-session"),
        );
        assert_eq!(
            payload.get("protocol").and_then(Value::as_str),
            Some(CTOX_RXDB_PROTOCOL)
        );
        let capabilities = payload
            .get("capabilities")
            .and_then(Value::as_array)
            .expect("capabilities array");
        assert!(capabilities
            .iter()
            .any(|value| value.as_str() == Some("ctox-replication-handshake-v1")));
        assert!(capabilities
            .iter()
            .any(|value| value.as_str() == Some("ctox-schema-hash-v1")));
        assert!(capabilities
            .iter()
            .any(|value| value.as_str() == Some("ctox-peer-session-v1")));
        assert!(capabilities
            .iter()
            .any(|value| value.as_str() == Some("ctox-checkpoint-epoch-v1")));
        assert!(capabilities
            .iter()
            .any(|value| value.as_str() == Some(CTOX_COMMAND_LIFECYCLE_CAPABILITY)));
        assert_eq!(
            payload
                .pointer("/collection/schemaHash")
                .and_then(Value::as_str),
            Some("schema-hash-1")
        );
        assert_eq!(
            payload
                .pointer("/collection/schemaHashSource")
                .and_then(Value::as_str),
            Some(CTOX_RXDB_RS_SCHEMA_HASH_SOURCE)
        );
        assert_eq!(
            payload
                .pointer("/collection/checkpoint/epoch")
                .and_then(Value::as_str),
            Some("checkpoint-epoch-1")
        );
        assert_eq!(
            payload.pointer("/peerSession/role").and_then(Value::as_str),
            Some("ctox_instance")
        );
        assert_eq!(
            payload
                .pointer("/peerSession/sessionId")
                .and_then(Value::as_str),
            Some("rxdb-rs-test-session")
        );
        assert!(validate_ctox_protocol_response(&payload, payload.get("collection"), true).is_ok());
    }

    #[test]
    fn ctox_protocol_response_uses_supplied_peer_session_per_payload() {
        let first = ctox_protocol_response_payload(Value::Null, Some("rxdb-rs-session-a"));
        let second = ctox_protocol_response_payload(Value::Null, Some("rxdb-rs-session-b"));
        assert_eq!(
            first
                .pointer("/peerSession/sessionId")
                .and_then(Value::as_str),
            Some("rxdb-rs-session-a")
        );
        assert_eq!(
            second
                .pointer("/peerSession/sessionId")
                .and_then(Value::as_str),
            Some("rxdb-rs-session-b")
        );
    }

    #[test]
    fn ctox_protocol_response_rejects_mismatch() {
        let result = validate_ctox_protocol_response(
            &serde_json::json!({
                "protocol": "rxdb-upstream",
                "capabilities": CTOX_REQUIRED_PROTOCOL_CAPABILITIES
            }),
            None,
            true,
        );
        let error = result.expect_err("protocol mismatch must fail");
        assert_eq!(error.code(), "RC_WEBRTC_PROTOCOL");
        assert_eq!(
            error.parameters().get("code").and_then(Value::as_str),
            Some(CTOX_PROTOCOL_ERROR_MISMATCH)
        );
    }

    #[test]
    fn ctox_protocol_fixture_matches_rust_handshake_contract() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/webrtc-rxdb-protocol.json"
        ))
        .expect("parse WebRTC RxDB protocol fixture");
        assert_eq!(
            fixture.get("protocol").and_then(Value::as_str),
            Some(CTOX_RXDB_PROTOCOL)
        );
        assert_eq!(
            fixture
                .pointer("/errorCodes/schemaHashMismatch")
                .and_then(Value::as_str),
            Some(CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH)
        );
        assert_eq!(
            fixture
                .pointer("/schemaHashSources/rxdbRs")
                .and_then(Value::as_str),
            Some(CTOX_RXDB_RS_SCHEMA_HASH_SOURCE)
        );
        let required = fixture
            .get("requiredCapabilities")
            .and_then(Value::as_array)
            .expect("required capabilities");
        assert_eq!(required.len(), CTOX_REQUIRED_PROTOCOL_CAPABILITIES.len());
        for capability in CTOX_REQUIRED_PROTOCOL_CAPABILITIES {
            assert!(required
                .iter()
                .any(|item| item.as_str() == Some(*capability)));
        }
        let browser = fixture
            .pointer("/compatible/browser")
            .expect("browser fixture");
        let native = fixture
            .pointer("/compatible/native")
            .expect("native fixture");
        assert!(validate_ctox_protocol_response(native, browser.get("collection"), true).is_ok());

        let mut missing_capability = native.clone();
        missing_capability["capabilities"] = serde_json::json!(["ctox-schema-hash-v1"]);
        assert_protocol_error(
            validate_ctox_protocol_response(&missing_capability, browser.get("collection"), true),
            CTOX_PROTOCOL_ERROR_CAPABILITY_MISSING,
        );

        let mut collection_mismatch = native.clone();
        collection_mismatch["collection"]["name"] = serde_json::json!("desktop_file_chunks");
        assert_protocol_error(
            validate_ctox_protocol_response(&collection_mismatch, browser.get("collection"), true),
            CTOX_PROTOCOL_ERROR_COLLECTION_MISMATCH,
        );

        let mut version_mismatch = native.clone();
        version_mismatch["collection"]["schemaVersion"] = serde_json::json!(2);
        assert_protocol_error(
            validate_ctox_protocol_response(&version_mismatch, browser.get("collection"), true),
            CTOX_PROTOCOL_ERROR_SCHEMA_VERSION_MISMATCH,
        );

        let mut hash_mismatch = native.clone();
        hash_mismatch["collection"]["schemaHash"] = serde_json::json!("different-fixture-hash");
        assert_protocol_error(
            validate_ctox_protocol_response(&hash_mismatch, browser.get("collection"), true),
            CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH,
        );
    }

    /// SYNC-03: the checkpoint wire shape is pinned by
    /// `tests/fixtures/webrtc-checkpoint-contract.json` on BOTH language
    /// sides (Rust here + `checkpoint-contract-smoke.mjs` in the browser
    /// runtime suite). This test pins the handshake key paths the checkpoint
    /// payload travels under.
    #[test]
    fn checkpoint_contract_fixture_matches_handshake_payload() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/webrtc-checkpoint-contract.json"
        ))
        .expect("parse checkpoint contract fixture");
        assert_eq!(
            fixture.get("contract").and_then(Value::as_str),
            Some("ctox-checkpoint-contract-v1")
        );

        // Capability names pinned by the fixture must match the generated
        // wire contract and be advertised by the native peer.
        assert_eq!(
            fixture
                .pointer("/capabilities/checkpointGeneration")
                .and_then(Value::as_str),
            Some(CTOX_CHECKPOINT_GENERATION_CAPABILITY)
        );
        assert_eq!(
            fixture
                .pointer("/validityKeys/v2/capability")
                .and_then(Value::as_str),
            Some(CTOX_CHECKPOINT_GENERATION_CAPABILITY)
        );
        for pointer in [
            "/capabilities/peerSession",
            "/capabilities/checkpointEpoch",
            "/capabilities/checkpointGeneration",
        ] {
            let capability = fixture
                .pointer(pointer)
                .and_then(Value::as_str)
                .expect("capability name");
            assert!(
                CTOX_RXDB_NATIVE_CAPABILITIES.contains(&capability),
                "native peer must advertise fixture capability {capability}"
            );
        }

        // Build a checkpoint payload straight from the fixture's worked
        // example and pass it through the REAL handshake payload builder.
        let example = &fixture["example"];
        let checkpoint = serde_json::json!({
            "source": fixture["checkpointStatus"]["source"],
            "state": fixture["checkpointStatus"]["state"],
            "collection": example["collectionName"],
            "schemaHash": example["schemaHash"],
            "latestLwt": example["latestLwt"],
            "latestIdHash": example["latestIdHash"],
            "epoch": example["epoch"],
        });
        let collection_payload = serde_json::json!({
            "name": example["collectionName"],
            "schemaVersion": 0,
            "schemaHash": example["schemaHash"],
            "schemaHashSource": CTOX_RXDB_RS_SCHEMA_HASH_SOURCE,
            "checkpoint": checkpoint,
        });

        // Single-collection room, no storage generation: collection.checkpoint
        // present, collectionCheckpoints key ABSENT, storageGeneration is JSON
        // null (key present; browsers treat null as absent).
        let single =
            ctox_protocol_response_payload(collection_payload.clone(), Some("rxdb-rs-run-a"));
        assert_eq!(single.pointer("/collection/checkpoint"), Some(&checkpoint));
        assert!(
            single.get("collectionCheckpoints").is_none(),
            "collectionCheckpoints key must be absent for single-collection rooms"
        );
        assert_eq!(
            single.get("storageGeneration"),
            Some(&Value::Null),
            "storageGeneration must be JSON null when the native peer has no generation"
        );
        assert_eq!(
            single
                .pointer("/peerSession/sessionId")
                .and_then(Value::as_str),
            Some("rxdb-rs-run-a")
        );
        assert!(single.get("nativeTimeMs").and_then(Value::as_u64).is_some());

        // Multiplexed room with a storage generation: every checkpoint key
        // path listed in the fixture resolves on the payload.
        let checkpoints_map = serde_json::json!({
            example["collectionName"].as_str().unwrap(): checkpoint,
        });
        let storage_generation = fixture
            .pointer("/validityKeys/v2/example/storageGeneration")
            .and_then(Value::as_str)
            .expect("v2 example storage generation");
        let multiplexed = ctox_protocol_response_payload_with_flag(
            collection_payload,
            Some("rxdb-rs-run-a"),
            true,
            None,
            Some(checkpoints_map.clone()),
            Some(storage_generation),
            NativePeerRole::CtoxInstance,
        );
        assert_eq!(
            multiplexed.get("collectionCheckpoints"),
            Some(&checkpoints_map)
        );
        assert_eq!(
            multiplexed.get("storageGeneration").and_then(Value::as_str),
            Some(storage_generation)
        );
        for path in fixture["handshake"]["checkpointKeyPaths"]
            .as_array()
            .expect("handshake checkpoint key paths")
        {
            let pointer = format!("/{}", path.as_str().unwrap().replace('.', "/"));
            assert!(
                multiplexed.pointer(&pointer).is_some(),
                "handshake key path {path} missing from ctoxProtocol payload"
            );
        }

        // The fixture's v1/v2 validity-key worked examples are consistent with
        // this payload's checkpoint evidence (the browser derivation itself is
        // driven by checkpoint-contract-smoke.mjs against the real JS code).
        let expected_v2_key = format!(
            "{}|{}|{}|{}",
            storage_generation,
            multiplexed
                .pointer("/collection/name")
                .and_then(Value::as_str)
                .unwrap(),
            multiplexed
                .pointer("/collection/schemaHash")
                .and_then(Value::as_str)
                .unwrap(),
            multiplexed
                .pointer("/collection/checkpoint/epoch")
                .and_then(Value::as_str)
                .unwrap(),
        );
        assert_eq!(
            fixture
                .pointer("/validityKeys/v2/example/key")
                .and_then(Value::as_str),
            Some(expected_v2_key.as_str())
        );
        let expected_v1_key = format!(
            "{}|{}|{}",
            multiplexed
                .pointer("/collection/checkpoint/epoch")
                .and_then(Value::as_str)
                .unwrap(),
            "rxdb-rs-run-a",
            multiplexed
                .pointer("/collection/schemaHash")
                .and_then(Value::as_str)
                .unwrap(),
        );
        assert_eq!(
            fixture
                .pointer("/validityKeys/v1/example/key")
                .and_then(Value::as_str),
            Some(expected_v1_key.as_str())
        );
    }

    fn assert_protocol_error(result: Result<(), RxError>, expected_code: &str) {
        let error = result.expect_err("protocol validation must fail");
        assert_eq!(error.code(), "RC_WEBRTC_PROTOCOL");
        assert_eq!(
            error.parameters().get("name").and_then(Value::as_str),
            Some("CtoxRxdbProtocolError")
        );
        assert_eq!(
            error.parameters().get("code").and_then(Value::as_str),
            Some(expected_code)
        );
        assert_eq!(
            error.parameters().get("phase").and_then(Value::as_str),
            Some("rxdb-protocol-handshake")
        );
    }

    #[test]
    fn master_change_stream_id_is_collection_qualified() {
        assert_eq!(
            master_change_stream_id("documents"),
            "masterChangeStream$:documents"
        );
        assert_ne!(
            master_change_stream_id("documents"),
            master_change_stream_id("desktop_files")
        );
    }

    // -------------------------------------------------------------------------
    // Phase 3 schema-validation hardening: per-collection schema validation
    // under multiplex (replaces the old wholesale `validate_schema` skip).
    // -------------------------------------------------------------------------

    fn local_schemas_two() -> Value {
        serde_json::json!({
            "documents": { "schemaVersion": 0, "schemaHash": "hash-docs", "schemaHashSource": "rxdb-rs" },
            "desktop_files": { "schemaVersion": 0, "schemaHash": "hash-files", "schemaHashSource": "rxdb-rs" },
        })
    }

    fn remote_protocol_with_schemas(schemas: Value) -> Value {
        serde_json::json!({
            "protocol": CTOX_RXDB_PROTOCOL,
            "capabilities": CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
            "collectionSchemas": schemas,
            "peerSession": { "role": "browser" },
        })
    }

    #[test]
    fn per_collection_schema_match_yields_no_mismatch() {
        let local = local_schemas_two();
        let remote = remote_protocol_with_schemas(local.clone());
        let mismatches = collect_collection_schema_mismatches(Some(&local), &remote);
        assert!(
            mismatches.is_empty(),
            "matching per-collection hashes must not flag any collection: {mismatches:?}"
        );
    }

    #[test]
    fn per_collection_schema_hash_mismatch_flags_only_that_collection() {
        let local = local_schemas_two();
        // Same `documents` hash, different `desktop_files` hash.
        let remote = remote_protocol_with_schemas(serde_json::json!({
            "documents": { "schemaVersion": 0, "schemaHash": "hash-docs" },
            "desktop_files": { "schemaVersion": 0, "schemaHash": "DRIFTED" },
        }));
        let mismatches = collect_collection_schema_mismatches(Some(&local), &remote);
        assert_eq!(
            mismatches.len(),
            1,
            "only the drifted collection is flagged"
        );
        assert!(mismatches.contains("desktop_files"));
        assert!(!mismatches.contains("documents"));
        // The error builder surfaces the existing schemaHashMismatch code for
        // the flagged collection, and None for the compatible one.
        let err = schema_mismatch_error_for(&mismatches, "desktop_files")
            .expect("flagged collection must produce an error");
        assert_eq!(err.code(), "RC_WEBRTC_PROTOCOL");
        assert_eq!(
            err.parameters().get("code").and_then(Value::as_str),
            Some(CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH)
        );
        assert_eq!(
            err.parameters().get("collection").and_then(Value::as_str),
            Some("desktop_files")
        );
        assert!(schema_mismatch_error_for(&mismatches, "documents").is_none());
    }

    #[test]
    fn per_collection_schema_version_mismatch_flags_collection() {
        let local = local_schemas_two();
        let remote = remote_protocol_with_schemas(serde_json::json!({
            "documents": { "schemaVersion": 0, "schemaHash": "hash-docs" },
            "desktop_files": { "schemaVersion": 9, "schemaHash": "hash-files" },
        }));
        let mismatches = collect_collection_schema_mismatches(Some(&local), &remote);
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches.contains("desktop_files"));
    }

    #[test]
    fn collection_not_advertised_by_remote_is_not_flagged() {
        let local = local_schemas_two();
        // Remote only advertises `documents`; `desktop_files` absent → benign.
        let remote = remote_protocol_with_schemas(serde_json::json!({
            "documents": { "schemaVersion": 0, "schemaHash": "hash-docs" },
        }));
        let mismatches = collect_collection_schema_mismatches(Some(&local), &remote);
        assert!(
            mismatches.is_empty(),
            "a collection the remote does not serve must not be flagged: {mismatches:?}"
        );
    }

    #[test]
    fn legacy_remote_without_collection_schemas_is_allowed_and_warning_counted() {
        let local = local_schemas_two();
        let remote = serde_json::json!({
            "protocol": CTOX_RXDB_PROTOCOL,
            "capabilities": CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
        });
        let warning_count = std::sync::atomic::AtomicU64::new(0);

        let mismatches =
            validate_multiplex_collection_schemas(Some(&local), &remote, &warning_count)
                .expect("a peer without multiplex-aware markers remains legacy-compatible");

        assert!(mismatches.is_empty());
        assert_eq!(
            warning_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the compatibility exception must remain observable"
        );
    }

    #[test]
    fn schema_capable_remote_without_collection_schemas_is_rejected_and_warning_counted() {
        let local = local_schemas_two();
        let remote = serde_json::json!({
            "protocol": CTOX_RXDB_PROTOCOL,
            "capabilities": CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
            "nativeTimeMs": 1_722_000_000_000_u64,
        });
        let warning_count = std::sync::atomic::AtomicU64::new(0);

        assert_protocol_error(
            validate_multiplex_collection_schemas(Some(&local), &remote, &warning_count)
                .map(|_| ()),
            CTOX_PROTOCOL_ERROR_CAPABILITY_MISSING,
        );
        assert_eq!(
            warning_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "rejected omissions must be counted before teardown"
        );
    }

    #[tokio::test]
    async fn collection_schemas_payload_covers_zero_and_one_collection() {
        assert_eq!(collection_schemas_payload(&[]).await, serde_json::json!({}));
        assert_eq!(
            collection_checkpoints_payload(&[]).await,
            serde_json::json!({})
        );
        let collection = crate::rx_collection::test_support::test_collection_named("single").await;
        let payload = collection_schemas_payload(std::slice::from_ref(&collection)).await;
        assert_eq!(payload.as_object().unwrap().len(), 1);
        assert_eq!(payload["single"]["schemaVersion"], 0);
        assert_eq!(
            payload["single"]["schemaHash"],
            collection.schema.as_ref().unwrap().hash().await
        );
        assert!(collection_checkpoints_payload(&[collection])
            .await
            .is_object());
    }

    #[test]
    fn native_runtime_role_is_explicit_and_rejects_browser_or_unknown_values() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/webrtc-rxdb-protocol.json"
        ))
        .unwrap();
        for (role, name) in [
            (NativePeerRole::CtoxInstance, "native"),
            (NativePeerRole::WorkjetExecutor, "worker"),
        ] {
            let wire = fixture["roles"][name].as_str().unwrap();
            assert_eq!(role.as_str(), wire);
            assert_eq!(NativePeerRole::from_wire(wire), Some(role));
            assert_eq!(
                serde_json::from_value::<NativePeerRole>(serde_json::json!(wire)).unwrap(),
                role
            );
            let payload = ctox_protocol_response_payload_with_flag(
                Value::Null,
                Some("session"),
                true,
                None,
                None,
                None,
                role,
            );
            assert_eq!(payload["peerSession"]["role"], wire);
            assert_eq!(payload["peerSession"]["sessionId"], "session");
        }
        for invalid in [
            "browser",
            "",
            "worker",
            " workjet_executor",
            "WORKJET_EXECUTOR",
        ] {
            assert!(NativePeerRole::from_wire(invalid).is_none());
            assert!(serde_json::from_value::<NativePeerRole>(serde_json::json!(invalid)).is_err());
        }
    }

    #[test]
    fn handshake_payload_omits_collection_schemas_when_none() {
        // The single-collection handshake payload must not carry the new keys,
        // so the wire stays byte-compatible with V1 peers.
        let single = ctox_protocol_response_payload_with_flag(
            serde_json::json!({ "name": "documents" }),
            Some("rxdb-rs-session"),
            true,
            None,
            None,
            Some("storage-generation-1"),
            NativePeerRole::CtoxInstance,
        );
        assert!(single.get("collectionSchemas").is_none());
        assert!(single.get("collectionCheckpoints").is_none());
        assert_eq!(single["peerSession"]["role"], "ctox_instance");
        // Multiplexed payload carries the maps.
        let multi = ctox_protocol_response_payload_with_flag(
            serde_json::json!({ "name": "documents" }),
            Some("rxdb-rs-session"),
            true,
            Some(local_schemas_two()),
            Some(serde_json::json!({
                "documents": { "source": "rxdb-rs-sqlite", "state": "advertised", "collection": "documents" },
                "desktop_files": { "source": "rxdb-rs-sqlite", "state": "advertised", "collection": "desktop_files" },
            })),
            Some("storage-generation-1"),
            NativePeerRole::CtoxInstance,
        );
        assert!(multi.get("collectionSchemas").is_some());
        assert!(multi
            .pointer("/collectionSchemas/desktop_files/schemaHash")
            .is_some());
        // REGRESSION: under multiplex every collection deriving its protocol
        // from the room handshake must find ITS OWN checkpoint here — the
        // representative-only checkpoint mislabeled every other collection's
        // peer-session evidence after a native restart.
        assert_eq!(
            multi
                .pointer("/collectionCheckpoints/desktop_files/collection")
                .and_then(Value::as_str),
            Some("desktop_files")
        );
    }

    // -------------------------------------------------------------------------
    // Phase 3 demux test infrastructure: a mock WebRTCConnectionHandler that
    // lets a test push inbound `PeerWithMessage`s and observe outbound frames,
    // so we can drive the multiplexed message-stream loop end to end with two
    // collections sharing ONE handler and assert frames demultiplex correctly.
    // -------------------------------------------------------------------------

    use crate::plugins::replication_webrtc::webrtc_types::{
        PeerWithMessage, PeerWithResponse, WebRTCConnectionHandler,
    };
    use crate::rxjs_compat::RxStream;
    use parking_lot::Mutex as PlMutex;
    use std::sync::Arc as StdArc;

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    struct MockPeer(String, u64);

    struct MockHandler {
        connect: crate::rxjs_compat::RxSubject<MockPeer>,
        disconnect: crate::rxjs_compat::RxSubject<MockPeer>,
        message: crate::rxjs_compat::RxSubject<PeerWithMessage<MockPeer>>,
        response: crate::rxjs_compat::RxSubject<PeerWithResponse<MockPeer>>,
        error: crate::rxjs_compat::RxSubject<RxError>,
        sent: StdArc<PlMutex<Vec<WebRTCWireFrame>>>,
        sent_subject: crate::rxjs_compat::RxSubject<WebRTCWireFrame>,
        closed_peers: StdArc<PlMutex<Vec<String>>>,
    }

    impl MockHandler {
        fn new() -> StdArc<Self> {
            StdArc::new(Self {
                connect: crate::rxjs_compat::RxSubject::new(),
                disconnect: crate::rxjs_compat::RxSubject::new(),
                message: crate::rxjs_compat::RxSubject::new(),
                response: crate::rxjs_compat::RxSubject::new(),
                error: crate::rxjs_compat::RxSubject::new(),
                sent: StdArc::new(PlMutex::new(Vec::new())),
                sent_subject: crate::rxjs_compat::RxSubject::new(),
                closed_peers: StdArc::new(PlMutex::new(Vec::new())),
            })
        }

        /// Feed an inbound request frame as if it arrived from `peer`.
        fn inject_message(&self, peer: &str, message: WebRTCMessage) {
            self.message.next(PeerWithMessage {
                peer: MockPeer(peer.to_string(), 1),
                message,
            });
        }

        /// Collect outbound responses sent back over the channel.
        fn sent_responses(&self) -> Vec<WebRTCResponse> {
            self.sent
                .lock()
                .iter()
                .filter_map(|frame| match frame {
                    WebRTCWireFrame::Response(r) => Some(r.clone()),
                    _ => None,
                })
                .collect()
        }
    }

    #[async_trait::async_trait]
    impl WebRTCConnectionHandler for MockHandler {
        // This fixture has no private document fields.
        fn document_fields_for_peer(&self, _: &Self::Peer, _: &str) -> Option<Vec<String>> {
            None
        }
        type Peer = MockPeer;

        fn connect_stream(&self) -> RxStream<MockPeer> {
            self.connect.subscribe()
        }
        fn disconnect_stream(&self) -> RxStream<MockPeer> {
            self.disconnect.subscribe()
        }
        fn message_stream(&self) -> RxStream<PeerWithMessage<MockPeer>> {
            self.message.subscribe()
        }
        fn response_stream(&self) -> RxStream<PeerWithResponse<MockPeer>> {
            self.response.subscribe()
        }
        fn error_stream(&self) -> RxStream<RxError> {
            self.error.subscribe()
        }
        async fn send(&self, _peer: &MockPeer, frame: WebRTCWireFrame) -> Result<(), RxError> {
            self.sent_subject.next(frame.clone());
            self.sent.lock().push(frame);
            Ok(())
        }
        async fn close(&self) -> Result<(), RxError> {
            Ok(())
        }
        async fn close_peer(&self, peer: &MockPeer) {
            self.closed_peers.lock().push(peer.0.clone());
        }

        fn peer_identity(&self, peer: &MockPeer) -> String {
            peer.0.clone()
        }

        fn connection_identity(&self, peer: &MockPeer) -> String {
            format!("{}@{}", peer.0, peer.1)
        }
    }

    #[tokio::test]
    async fn lifecycle_owned_auxiliary_contract_cannot_be_registered_twice() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("control_owner").await;
        let pool = RxWebRTCReplicationPool::new(collection, MockHandler::new());
        let first: AuxiliaryRequestHandler =
            Arc::new(|_, _, _| Box::pin(async { Ok(serde_json::json!("first")) }));
        let second: AuxiliaryRequestHandler =
            Arc::new(|_, _, _| Box::pin(async { Ok(serde_json::json!("second")) }));
        pool.register_auxiliary_request_handler("ctox.sync.authority.v1", first)
            .unwrap();
        assert!(pool
            .register_auxiliary_request_handler("ctox.sync.authority.v1", second)
            .is_err());
        let installed = pool
            .auxiliary_request_handlers
            .lock()
            .get("ctox.sync.authority.v1")
            .unwrap()
            .clone();
        assert_eq!(
            installed(String::new(), String::new(), vec![])
                .await
                .unwrap(),
            serde_json::json!("first")
        );
    }

    #[tokio::test]
    async fn control_requires_both_handshake_directions_and_resets_on_disconnect() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("control_admission").await;
        let pool = RxWebRTCReplicationPool::new(collection, MockHandler::new());
        let peer = MockPeer("native-control".into(), 1);
        assert!(!pool.is_peer_ready_for_control(&peer));
        pool.authenticated_peers.lock().insert(peer.clone());
        assert!(
            !pool.is_peer_ready_for_control(&peer),
            "inbound probe alone is not reciprocal admission"
        );
        pool.outbound_ready_peers.lock().insert(peer.clone());
        assert!(pool.is_peer_ready_for_control(&peer));
        pool.remove_peer(&peer).await;
        assert!(!pool.is_peer_ready_for_control(&peer));
        pool.outbound_ready_peers.lock().insert(peer.clone());
        pool.cancel().await;
        assert!(!pool.is_peer_ready_for_control(&peer));
    }

    #[tokio::test]
    async fn delayed_disconnect_must_not_cancel_a_replacement_handshake() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let collection =
                crate::rx_collection::test_support::test_collection_named("replacement_admission")
                    .await;
            let handler = MockHandler::new();
            let pool = replicate_web_rtc_multi_with_validators(
                Arc::clone(&collection.database),
                vec![collection],
                handler.clone(),
                None,
                Some(StdArc::new(|_, _| WebRTCPeerSessionValidation::Accept)),
                Some("replacement-room".into()),
                Some(StdArc::<str>::from("local-session")),
            )
            .await
            .unwrap();
            let blocker = MockPeer("cleanup-blocker".into(), 1);
            let old_peer = MockPeer("reconnecting-peer".into(), 1);
            let peer = MockPeer(old_peer.0.clone(), 2);
            assert_eq!(
                handler.peer_identity(&old_peer),
                handler.peer_identity(&peer)
            );
            assert_ne!(
                old_peer, peer,
                "a reconnect keeps its route but changes its lifetime"
            );
            let barrier = MockPeer("disconnect-barrier".into(), 1);
            pool.add_peer(blocker.clone(), vec![]);
            pool.add_peer(old_peer.clone(), vec![]);

            struct Aborted(Option<tokio::sync::oneshot::Sender<()>>);
            impl Drop for Aborted {
                fn drop(&mut self) {
                    if let Some(tx) = self.0.take() {
                        let _ = tx.send(());
                    }
                }
            }
            let (started_tx, started_rx) = tokio::sync::oneshot::channel();
            let (aborted_tx, aborted_rx) = tokio::sync::oneshot::channel();
            let barrier_task = tokio::spawn(async move {
                let _aborted = Aborted(Some(aborted_tx));
                let _ = started_tx.send(());
                std::future::pending::<()>().await;
            });
            pool.add_peer(barrier.clone(), vec![barrier_task]);
            started_rx.await.unwrap();

            // Hold old cleanup at a real await boundary, then enqueue the old
            // disconnect before publishing the replacement connection.
            let cleanup = pool.fork_state_lifecycle.lock().await;
            handler.disconnect.next(blocker.clone());
            while pool.peer_states.lock().contains_key(&blocker) {
                tokio::task::yield_now().await;
            }
            let mut sent = handler.sent_subject.subscribe();
            handler.disconnect.next(old_peer.clone());
            handler.connect.next(peer.clone());
            handler.disconnect.next(barrier);

            // Complete the actual outbound protocol/token handshake. A new
            // connection must be able to progress while unrelated cleanup waits.
            loop {
                let WebRTCWireFrame::Message(request) = sent.next().await.unwrap() else {
                    continue;
                };
                let result = match request.method.as_str() {
                    "ctoxProtocol" => request.params[0].clone(),
                    "token" => Value::String("remote-storage-token".into()),
                    _ => continue,
                };
                let is_token = request.method == "token";
                handler.response.next(PeerWithResponse {
                    peer: peer.clone(),
                    response: WebRTCResponse {
                        id: request.id,
                        result,
                        error: None,
                        collection: None,
                    },
                });
                if is_token {
                    break;
                }
            }
            while !pool.is_peer_ready_for_control(&peer) {
                tokio::task::yield_now().await;
            }
            drop(cleanup);
            // This task belongs to the disconnect queued AFTER the stale one:
            // its cancellation proves the consumer has processed that event.
            aborted_rx.await.unwrap();
            assert!(
                pool.is_peer_ready_for_control(&peer),
                "old queued disconnect erased the replacement's confirmed admission"
            );
            pool.cancel().await;
        })
        .await
        .expect("lifecycle regression must settle without extending transport deadlines");
    }

    #[tokio::test]
    async fn protocol_room_payload_cache_hit_bypasses_in_flight_build_gate() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("protocol_cache_hit").await;
        let pool = RxWebRTCReplicationPool::new(collection, MockHandler::new());
        let cached_payload = ProtocolRoomPayload {
            collection_schemas: Some(serde_json::json!({ "source": "cache" })),
            collection_checkpoints: Some(serde_json::json!({ "checkpoint": 1 })),
        };
        {
            let mut cache = pool.protocol_room_payload_cache.lock();
            cache.payload = Some(cached_payload.clone());
            cache.built_at = Some(Instant::now());
        }

        // Holding this gate simulates a slow storage-backed build. The cache
        // lookup must not acquire it when the TTL entry is still valid.
        let build_guard = pool.protocol_room_payload_build.lock().await;
        let build_calls = StdArc::new(std::sync::atomic::AtomicUsize::new(0));
        let build_calls_for_closure = StdArc::clone(&build_calls);
        let hit = tokio::time::timeout(
            Duration::from_millis(100),
            pool.protocol_room_payload_with(move || async move {
                build_calls_for_closure.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ProtocolRoomPayload::default()
            }),
        )
        .await
        .expect("valid cache hit must not wait behind the in-flight build gate");

        assert_eq!(hit.collection_schemas, cached_payload.collection_schemas);
        assert_eq!(
            build_calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "a valid hit must not invoke the payload builder"
        );
        drop(build_guard);
        pool.cancel().await;
    }

    #[tokio::test]
    async fn protocol_room_payload_concurrent_misses_coalesce_one_build() {
        const CALLERS: usize = 12;
        let collection =
            crate::rx_collection::test_support::test_collection_named("protocol_cache_miss").await;
        let pool = RxWebRTCReplicationPool::new(collection, MockHandler::new());
        let start = StdArc::new(tokio::sync::Barrier::new(CALLERS + 1));
        let build_calls = StdArc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut callers = Vec::with_capacity(CALLERS);

        for _ in 0..CALLERS {
            let pool = StdArc::clone(&pool);
            let start = StdArc::clone(&start);
            let build_calls = StdArc::clone(&build_calls);
            callers.push(tokio::spawn(async move {
                start.wait().await;
                pool.protocol_room_payload_with(move || async move {
                    let build_number =
                        build_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    ProtocolRoomPayload {
                        collection_schemas: Some(serde_json::json!({
                            "build": build_number
                        })),
                        collection_checkpoints: None,
                    }
                })
                .await
            }));
        }

        start.wait().await;
        for caller in callers {
            let payload = caller.await.expect("cache caller task");
            assert_eq!(
                payload
                    .collection_schemas
                    .as_ref()
                    .and_then(|value| value.get("build"))
                    .and_then(Value::as_u64),
                Some(1),
                "all waiters must consume the winner's payload"
            );
        }
        assert_eq!(
            build_calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "concurrent cache misses must invoke exactly one builder"
        );
        pool.cancel().await;
    }

    fn master_changes_since_frame(id: &str, collection: &str) -> WebRTCMessage {
        WebRTCMessage {
            id: id.to_string(),
            method: "masterChangesSince".to_string(),
            // checkpoint = null, batch_size = 10
            params: vec![Value::Null, Value::from(10u64)],
            collection: Some(collection.to_string()),
        }
    }

    fn ctox_protocol_frame(id: &str, collection: &str) -> WebRTCMessage {
        WebRTCMessage {
            id: id.to_string(),
            method: "ctoxProtocol".to_string(),
            params: vec![],
            collection: Some(collection.to_string()),
        }
    }

    fn ctox_protocol_frame_with_session(
        id: &str,
        collection: &str,
        session_id: Option<&str>,
    ) -> WebRTCMessage {
        let mut peer_session = serde_json::Map::from_iter([(
            "role".to_string(),
            Value::String("browser".to_string()),
        )]);
        if let Some(session_id) = session_id {
            peer_session.insert(
                "sessionId".to_string(),
                Value::String(session_id.to_string()),
            );
        }
        WebRTCMessage {
            id: id.to_string(),
            method: "ctoxProtocol".to_string(),
            params: vec![serde_json::json!({ "peerSession": peer_session })],
            collection: Some(collection.to_string()),
        }
    }

    #[tokio::test]
    async fn control_only_protocol_explicitly_advertises_no_collections() {
        let pool = RxWebRTCReplicationPool::new_multi(Vec::new(), MockHandler::new());
        let payload = pool.protocol_room_payload().await;
        assert_eq!(payload.collection_schemas, Some(serde_json::json!({})));
        assert_eq!(payload.collection_checkpoints, Some(serde_json::json!({})));
        let protocol = ctox_protocol_response_with_flag(
            None,
            Some("worker-session"),
            false,
            payload.collection_schemas,
            payload.collection_checkpoints,
            Some("worker-storage"),
            NativePeerRole::WorkjetExecutor,
        )
        .await;
        assert_eq!(protocol["collection"], Value::Null);
        assert_eq!(protocol["peerSession"]["role"], "workjet_executor");
        let warnings = std::sync::atomic::AtomicU64::new(0);
        assert!(validate_multiplex_collection_schemas(
            Some(&local_schemas_two()),
            &protocol,
            &warnings,
        )
        .unwrap()
        .is_empty());
        let mut missing = protocol.clone();
        missing.as_object_mut().unwrap().remove("collectionSchemas");
        assert!(
            validate_multiplex_collection_schemas(
                Some(&serde_json::json!({})),
                &missing,
                &warnings,
            )
            .is_err(),
            "an explicit empty set must not permit missing schema proofs"
        );
        pool.cancel().await;
    }

    #[tokio::test]
    async fn control_only_remote_creates_neither_master_relays_nor_fork_writers() {
        let mut elections = std::collections::HashSet::new();
        for remote_token in ["aaaa-control", "zzzz-control"] {
            let collection =
                crate::rx_collection::test_support::test_collection_named("control_boundary").await;
            elections.insert(
                is_master_in_webrtc_replication(
                    collection.database.hash_function.clone(),
                    &collection.database.storage_token,
                    remote_token,
                )
                .await,
            );
            let handler = MockHandler::new();
            let pool = replicate_web_rtc_multi_with_validators(
                collection.database.clone(),
                vec![collection],
                handler.clone(),
                None,
                Some(Arc::new(|_, _| WebRTCPeerSessionValidation::Accept)),
                None,
                None,
            )
            .await
            .unwrap();
            let peer = MockPeer("control-peer".into(), 1);
            let protocol = ctox_protocol_response_with_flag(
                None,
                Some("control-session"),
                false,
                Some(serde_json::json!({})),
                Some(serde_json::json!({})),
                Some(remote_token),
                NativePeerRole::WorkjetExecutor,
            )
            .await;
            let mut requests = handler.sent_subject.subscribe();
            handler.connect.next(peer.clone());
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    tokio::select! {
                        frame = requests.next() => {
                            if let Some(WebRTCWireFrame::Message(request)) = frame {
                                let result = match request.method.as_str() {
                                    "ctoxProtocol" => protocol.clone(),
                                    "token" => Value::String(remote_token.into()),
                                    other => panic!("control-only peer received data request: {other}"),
                                };
                                handler.response.next(PeerWithResponse {
                                    peer: peer.clone(),
                                    response: WebRTCResponse { id: request.id, result, error: None, collection: None },
                                });
                            }
                        }
                        _ = tokio::time::sleep(Duration::from_millis(1)) => {
                            if pool.is_peer_ready_for_control(&peer)
                                && pool.peer_states.lock().get(&peer).is_some_and(
                                    |state| state.sub_tasks.iter().all(tokio::task::JoinHandle::is_finished))
                            {
                                break;
                            }
                        }
                    }
                }
            }).await.expect("control handshake must finish without persistent replication tasks");
            assert!(pool.fork_states.lock().is_empty());
            pool.cancel().await;
        }
        assert_eq!(
            elections.len(),
            2,
            "exercise both master and fork elections"
        );
    }

    #[tokio::test]
    async fn implicit_database_entrypoint_rejects_empty_replication_set() {
        let error = replicate_web_rtc_multi(Vec::new(), MockHandler::new(), None, None, None)
            .await
            .err()
            .expect("empty set cannot infer database identity");
        assert_eq!(error.code(), "RC_WEBRTC");
    }

    #[tokio::test]
    async fn ctox_protocol_response_carries_multiplex_room_payload() {
        let alpha = crate::rx_collection::test_support::test_collection_named("proto_alpha").await;
        let beta = crate::rx_collection::test_support::test_collection_in_database(
            "proto_beta",
            alpha.database.clone(),
        )
        .await;
        let handler = MockHandler::new();
        let pool = replicate_web_rtc_multi(
            vec![StdArc::clone(&alpha), StdArc::clone(&beta)],
            StdArc::clone(&handler),
            None,
            Some("test-room-protocol".to_string()),
            Some(StdArc::<str>::from("rxdb-rs-protocol-test")),
        )
        .await
        .expect("bring up multiplexed pool");

        handler.inject_message(
            "browser-1",
            ctox_protocol_frame("req-protocol", "proto_beta"),
        );
        for _ in 0..30 {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(5)).await;
            if !handler.sent_responses().is_empty() {
                break;
            }
        }

        let responses = handler.sent_responses();
        let response = responses
            .iter()
            .find(|item| item.id == "req-protocol")
            .expect("ctoxProtocol answer present");
        assert_eq!(response.collection.as_deref(), Some("proto_beta"));
        assert_eq!(
            response
                .result
                .pointer("/collection/name")
                .and_then(Value::as_str),
            Some("proto_beta")
        );
        assert!(
            response
                .result
                .pointer("/collectionSchemas/proto_alpha/schemaHash")
                .and_then(Value::as_str)
                .is_some(),
            "multiplex schema map must include every collection"
        );
        assert_eq!(
            response
                .result
                .pointer("/collectionCheckpoints/proto_beta/collection")
                .and_then(Value::as_str),
            Some("proto_beta")
        );

        pool.cancel().await;
    }

    #[tokio::test]
    async fn revoked_inbound_peer_session_answers_and_closes_without_handshake() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("revoked_inbound").await;
        let handler = MockHandler::new();
        let validator: WebRTCPeerSessionValidator = StdArc::new(|payload, _| {
            match payload
                .pointer("/peerSession/sessionId")
                .and_then(Value::as_str)
            {
                Some("revoked-browser-device") | None => WebRTCPeerSessionValidation::Reject,
                Some(_) => WebRTCPeerSessionValidation::Accept,
            }
        });
        let pool = replicate_web_rtc_multi_with_validators(
            StdArc::clone(&collection.database),
            vec![StdArc::clone(&collection)],
            StdArc::clone(&handler),
            None,
            Some(validator),
            Some("revocation-room".to_string()),
            Some(StdArc::<str>::from("native-session")),
        )
        .await
        .expect("bring up pool with durable validator");

        handler.inject_message(
            "ephemeral-socket-peer",
            ctox_protocol_frame_with_session(
                "revoked-request",
                "revoked_inbound",
                Some("revoked-browser-device"),
            ),
        );
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if !handler.sent_responses().is_empty() && !handler.closed_peers.lock().is_empty() {
                break;
            }
        }

        let response = handler
            .sent_responses()
            .into_iter()
            .find(|response| response.id == "revoked-request")
            .expect("revocation response sent");
        assert_eq!(response.result, Value::Null);
        assert_eq!(
            response.error.as_deref(),
            Some("peer_authentication_failed")
        );
        assert_eq!(response.collection.as_deref(), Some("revoked_inbound"));
        assert_eq!(
            handler.closed_peers.lock().as_slice(),
            &["ephemeral-socket-peer".to_string()]
        );
        assert_eq!(
            handler.sent_responses().len(),
            1,
            "revoked request must not continue into the normal handshake response"
        );
        pool.cancel().await;
    }

    #[tokio::test]
    async fn missing_inbound_peer_session_is_rejected_when_validator_is_enabled() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("missing_session").await;
        let handler = MockHandler::new();
        let validator: WebRTCPeerSessionValidator = StdArc::new(|payload, _| {
            if payload
                .pointer("/peerSession/sessionId")
                .and_then(Value::as_str)
                .is_some()
            {
                WebRTCPeerSessionValidation::Accept
            } else {
                WebRTCPeerSessionValidation::Reject
            }
        });
        let pool = replicate_web_rtc_multi_with_validators(
            StdArc::clone(&collection.database),
            vec![StdArc::clone(&collection)],
            StdArc::clone(&handler),
            None,
            Some(validator),
            Some("revocation-room".to_string()),
            Some(StdArc::<str>::from("native-session")),
        )
        .await
        .expect("bring up pool with durable validator");

        handler.inject_message(
            "missing-session-peer",
            ctox_protocol_frame_with_session("missing-request", "missing_session", None),
        );
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if !handler.sent_responses().is_empty() && !handler.closed_peers.lock().is_empty() {
                break;
            }
        }
        let response = handler
            .sent_responses()
            .into_iter()
            .find(|response| response.id == "missing-request")
            .expect("missing session rejection sent");
        assert_eq!(response.result, Value::Null);
        assert_eq!(
            response.error.as_deref(),
            Some("peer_authentication_failed")
        );
        assert_eq!(
            handler.closed_peers.lock().as_slice(),
            &["missing-session-peer".to_string()]
        );
        pool.cancel().await;
    }

    #[tokio::test]
    async fn accepted_inbound_peer_session_completes_protocol_response() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("accepted_inbound").await;
        let handler = MockHandler::new();
        let validator: WebRTCPeerSessionValidator = StdArc::new(|payload, _| {
            if payload
                .pointer("/peerSession/sessionId")
                .and_then(Value::as_str)
                == Some("accepted-browser-device")
            {
                WebRTCPeerSessionValidation::Accept
            } else {
                WebRTCPeerSessionValidation::Reject
            }
        });
        let pool = replicate_web_rtc_multi_with_validators(
            StdArc::clone(&collection.database),
            vec![StdArc::clone(&collection)],
            StdArc::clone(&handler),
            None,
            Some(validator),
            Some("revocation-room".to_string()),
            Some(StdArc::<str>::from("native-session")),
        )
        .await
        .expect("bring up pool with durable validator");

        handler.inject_message(
            "ephemeral-socket-peer",
            ctox_protocol_frame_with_session(
                "accepted-request",
                "accepted_inbound",
                Some("accepted-browser-device"),
            ),
        );
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if !handler.sent_responses().is_empty() {
                break;
            }
        }

        let response = handler
            .sent_responses()
            .into_iter()
            .find(|response| response.id == "accepted-request")
            .expect("normal protocol response sent");
        assert!(response.error.is_none());
        assert_eq!(
            response
                .result
                .pointer("/peerSession/sessionId")
                .and_then(Value::as_str),
            Some("native-session")
        );
        assert!(handler.closed_peers.lock().is_empty());
        pool.cancel().await;
    }

    #[tokio::test]
    async fn deferred_bound_session_cannot_send_data_before_native_challenge() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("deferred_bound").await;
        let handler = MockHandler::new();
        let validator: WebRTCPeerSessionValidator = StdArc::new(|_, expected_nonce| {
            if expected_nonce.is_none() {
                WebRTCPeerSessionValidation::Defer
            } else {
                WebRTCPeerSessionValidation::Reject
            }
        });
        let pool = replicate_web_rtc_multi_with_validators(
            StdArc::clone(&collection.database),
            vec![StdArc::clone(&collection)],
            StdArc::clone(&handler),
            None,
            Some(validator),
            Some("proof-room".to_string()),
            Some(StdArc::<str>::from("native-session")),
        )
        .await
        .expect("bring up pool with proof validator");

        handler.inject_message(
            "bound-peer",
            ctox_protocol_frame_with_session(
                "deferred-protocol",
                "deferred_bound",
                Some("browser:proof-room"),
            ),
        );
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if handler
                .sent_responses()
                .iter()
                .any(|response| response.id == "deferred-protocol")
            {
                break;
            }
        }
        handler.inject_message(
            "bound-peer",
            master_changes_since_frame("premature-read", "deferred_bound"),
        );
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if handler
                .sent_responses()
                .iter()
                .any(|response| response.id == "premature-read")
            {
                break;
            }
        }
        let response = handler
            .sent_responses()
            .into_iter()
            .find(|response| response.id == "premature-read")
            .expect("premature data request rejected");
        assert_eq!(
            response.error.as_deref(),
            Some("peer_authentication_required")
        );
        assert!(handler
            .closed_peers
            .lock()
            .contains(&"bound-peer".to_string()));
        pool.cancel().await;
    }

    /// Phase 3: two collections multiplexed on ONE handler must demultiplex
    /// `masterChangesSince` frames to the right collection's master handler.
    /// We insert distinct docs into each collection, push a collection-tagged
    /// request for each over the single mock handler, and assert each answer
    /// carries ONLY that collection's documents (and echoes its collection).
    #[tokio::test]
    async fn two_collections_demux_master_changes_on_one_handler() {
        let alpha = crate::rx_collection::test_support::test_collection_named("alpha").await;
        let beta = crate::rx_collection::test_support::test_collection_in_database(
            "beta",
            alpha.database.clone(),
        )
        .await;
        alpha
            .insert(serde_json::json!({ "id": "alpha-doc", "age": 1 }))
            .await
            .expect("insert alpha doc");
        beta.insert(serde_json::json!({ "id": "beta-doc", "age": 2 }))
            .await
            .expect("insert beta doc");

        let handler = MockHandler::new();
        let pool = replicate_web_rtc_multi(
            vec![StdArc::clone(&alpha), StdArc::clone(&beta)],
            StdArc::clone(&handler),
            None,
            Some("test-room".to_string()),
            Some(StdArc::<str>::from("rxdb-rs-test")),
        )
        .await
        .expect("bring up multiplexed pool");

        // Both collections are registered behind the one connection.
        let mut names: Vec<String> = pool
            .collections()
            .into_iter()
            .map(|c| c.name.clone())
            .collect();
        names.sort();
        assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);

        // Drive collection-tagged requests over the single handler. A short
        // yield lets the spawned message-stream loop process each frame.
        handler.inject_message(
            "browser-1",
            master_changes_since_frame("req-alpha", "alpha"),
        );
        handler.inject_message("browser-1", master_changes_since_frame("req-beta", "beta"));
        for _ in 0..20 {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(5)).await;
            if handler.sent_responses().len() >= 2 {
                break;
            }
        }

        let responses = handler.sent_responses();
        let alpha_resp = responses
            .iter()
            .find(|r| r.id == "req-alpha")
            .expect("alpha answer present");
        let beta_resp = responses
            .iter()
            .find(|r| r.id == "req-beta")
            .expect("beta answer present");

        // Each answer echoes its routing collection ...
        assert_eq!(alpha_resp.collection.as_deref(), Some("alpha"));
        assert_eq!(beta_resp.collection.as_deref(), Some("beta"));

        // ... and carries ONLY that collection's document — proving the demux
        // routed each frame to the correct master handler / storage instance.
        let alpha_docs: DocumentsWithCheckpoint =
            serde_json::from_value(alpha_resp.result.clone()).expect("decode alpha changes");
        let beta_docs: DocumentsWithCheckpoint =
            serde_json::from_value(beta_resp.result.clone()).expect("decode beta changes");
        let alpha_ids: Vec<String> = alpha_docs
            .documents
            .iter()
            .filter_map(|d| d.get("id").and_then(Value::as_str).map(str::to_string))
            .collect();
        let beta_ids: Vec<String> = beta_docs
            .documents
            .iter()
            .filter_map(|d| d.get("id").and_then(Value::as_str).map(str::to_string))
            .collect();
        assert_eq!(alpha_ids, vec!["alpha-doc".to_string()]);
        assert_eq!(beta_ids, vec!["beta-doc".to_string()]);

        pool.cancel().await;
    }

    /// A frame tagged with a collection the peer does not serve must produce a
    /// replication-io error answer, never a cross-collection leak.
    #[tokio::test]
    async fn unknown_collection_frame_answers_with_io_error() {
        let alpha = crate::rx_collection::test_support::test_collection_named("alpha2").await;
        let handler = MockHandler::new();
        let pool = replicate_web_rtc_multi(
            vec![StdArc::clone(&alpha)],
            StdArc::clone(&handler),
            None,
            Some("test-room-2".to_string()),
            Some(StdArc::<str>::from("rxdb-rs-test-2")),
        )
        .await
        .expect("bring up pool");

        handler.inject_message(
            "browser-1",
            master_changes_since_frame("req-ghost", "does_not_exist"),
        );
        for _ in 0..20 {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(5)).await;
            if !handler.sent_responses().is_empty() {
                break;
            }
        }

        let responses = handler.sent_responses();
        let ghost = responses
            .iter()
            .find(|r| r.id == "req-ghost")
            .expect("ghost answer present");
        assert_eq!(ghost.collection.as_deref(), Some("does_not_exist"));
        assert_eq!(ghost.result["type"], serde_json::json!("ctoxError"));
        assert_eq!(ghost.result["code"], serde_json::json!("RC_WEBRTC_PEER"));
        assert_eq!(
            ghost.result["collection"],
            serde_json::json!("does_not_exist")
        );

        pool.cancel().await;
    }
    /// REGRESSION (52a1bf45): a request in flight when its peer disconnects
    /// must FAIL, not hang. The per-subscriber response stream only ends when
    /// the whole handler is dropped, so without racing the disconnect event a
    /// stuck handshake/fork pull was uncancelable until process restart.
    #[tokio::test]
    async fn request_in_flight_fails_when_peer_disconnects() {
        let handler = MockHandler::new();
        let peer = MockPeer("p1".to_string(), 1);
        let request = tokio::spawn(send_message_and_await_answer(
            StdArc::clone(&handler) as StdArc<dyn WebRTCConnectionHandler<Peer = MockPeer>>,
            peer.clone(),
            WebRTCMessage {
                id: "req-1".to_string(),
                method: "token".to_string(),
                params: vec![],
                collection: None,
            },
        ));
        // Let the request subscribe + send, then drop the peer.
        tokio::time::sleep(Duration::from_millis(20)).await;
        handler.disconnect.next(peer);
        let result = tokio::time::timeout(Duration::from_secs(2), request)
            .await
            .expect("request must settle promptly after peer disconnect")
            .expect("task must not panic");
        let err = result.expect_err("disconnect must surface as an error, not an answer");
        assert!(
            err.to_string().contains("disconnected"),
            "error should name the disconnect: {err}"
        );
    }

    /// REGRESSION (52a1bf45): a request whose answer never arrives (channel
    /// alive, remote silent) must fail at the request deadline instead of
    /// parking its caller forever. Paused time fast-forwards the 60s deadline.
    #[tokio::test(start_paused = true)]
    async fn request_without_answer_times_out() {
        let handler = MockHandler::new();
        let result = tokio::time::timeout(
            Duration::from_secs(120),
            send_message_and_await_answer(
                StdArc::clone(&handler) as StdArc<dyn WebRTCConnectionHandler<Peer = MockPeer>>,
                MockPeer("p1".to_string(), 1),
                WebRTCMessage {
                    id: "req-timeout".to_string(),
                    method: "token".to_string(),
                    params: vec![],
                    collection: None,
                },
            ),
        )
        .await
        .expect("request must settle at its own deadline");
        let err = result.expect_err("a never-answered request must error");
        assert!(
            err.to_string().contains("no answer within"),
            "error should name the deadline: {err}"
        );
    }

    #[tokio::test]
    async fn revoked_outbound_peer_session_fails_handshake_and_closes_peer() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("revoked_outbound").await;
        let handler = MockHandler::new();
        let validator: WebRTCPeerSessionValidator = StdArc::new(|payload, _| {
            match payload
                .pointer("/peerSession/sessionId")
                .and_then(Value::as_str)
            {
                Some("revoked-browser-device") | None => WebRTCPeerSessionValidation::Reject,
                Some(_) => WebRTCPeerSessionValidation::Accept,
            }
        });
        let pool = replicate_web_rtc_multi_with_validators(
            StdArc::clone(&collection.database),
            vec![StdArc::clone(&collection)],
            StdArc::clone(&handler),
            None,
            Some(validator),
            Some("revocation-room".to_string()),
            Some(StdArc::<str>::from("native-session")),
        )
        .await
        .expect("bring up pool with durable validator");
        let mut errors = pool.error_subject.subscribe();
        let peer = MockPeer("ephemeral-socket-peer".to_string(), 1);
        handler.connect.next(peer.clone());

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut answered_protocol = false;
        while tokio::time::Instant::now() < deadline && !answered_protocol {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let requests: Vec<WebRTCMessage> = handler
                .sent
                .lock()
                .iter()
                .filter_map(|frame| match frame {
                    WebRTCWireFrame::Message(message) => Some(message.clone()),
                    _ => None,
                })
                .collect();
            for request in requests {
                if request.method != "ctoxProtocol" || answered_protocol {
                    continue;
                }
                answered_protocol = true;
                let mut remote_protocol = request.params.first().cloned().unwrap_or(Value::Null);
                remote_protocol["peerSession"]["sessionId"] =
                    Value::String("revoked-browser-device".to_string());
                handler.response.next(PeerWithResponse {
                    peer: peer.clone(),
                    response: WebRTCResponse {
                        id: request.id,
                        result: remote_protocol,
                        error: None,
                        collection: None,
                    },
                });
            }
        }
        assert!(
            answered_protocol,
            "native outbound ctoxProtocol request observed"
        );

        let error = tokio::time::timeout(Duration::from_secs(2), errors.next())
            .await
            .expect("revocation error surfaces")
            .expect("error stream alive");
        assert_eq!(error.code(), "RC_WEBRTC_PEER");
        assert_eq!(
            error.parameters().get("code").and_then(Value::as_str),
            Some("peer_authentication_failed")
        );
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline && handler.closed_peers.lock().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            handler.closed_peers.lock().as_slice(),
            &["ephemeral-socket-peer".to_string()]
        );
        assert!(
            handler.sent.lock().iter().all(|frame| !matches!(
                frame,
                WebRTCWireFrame::Message(message) if message.method == "token"
            )),
            "rejected durable identity must stop before token handshake"
        );
        pool.cancel().await;
    }

    /// REGRESSION (52a1bf45): an empty/non-string `token` answer corrupts the
    /// master election and collapses the replication identifier, so the
    /// handshake must fail AND tear the transport down (close_peer) instead of
    /// leaving a half-dead peer that answers requests but never replicates.
    #[tokio::test]
    async fn empty_token_answer_fails_handshake_and_closes_peer() {
        let collection = crate::rx_collection::test_support::test_collection_named("etok").await;
        let handler = MockHandler::new();
        let pool = replicate_web_rtc_multi(
            vec![StdArc::clone(&collection)],
            StdArc::clone(&handler),
            None,
            Some("test-room".to_string()),
            Some(StdArc::<str>::from("rxdb-rs-test")),
        )
        .await
        .expect("bring up pool");
        let mut errors = pool.error_subject.subscribe();

        let peer = MockPeer("p1".to_string(), 1);
        handler.connect.next(peer.clone());

        // Answer the handshake requests the pool sends us: a valid protocol
        // payload first, then an EMPTY token.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut answered_protocol = false;
        let mut answered_token = false;
        while tokio::time::Instant::now() < deadline && !(answered_protocol && answered_token) {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let requests: Vec<WebRTCMessage> = handler
                .sent
                .lock()
                .iter()
                .filter_map(|frame| match frame {
                    WebRTCWireFrame::Message(m) => Some(m.clone()),
                    _ => None,
                })
                .collect();
            for request in requests {
                if request.method == "ctoxProtocol" && !answered_protocol {
                    answered_protocol = true;
                    let local_protocol = request.params.first().cloned().unwrap_or(Value::Null);
                    handler.response.next(PeerWithResponse {
                        peer: peer.clone(),
                        response: WebRTCResponse {
                            id: request.id.clone(),
                            // Echo our own payload back: protocol-compatible.
                            result: local_protocol,
                            error: None,
                            collection: None,
                        },
                    });
                } else if request.method == "token" && !answered_token {
                    answered_token = true;
                    handler.response.next(PeerWithResponse {
                        peer: peer.clone(),
                        response: WebRTCResponse {
                            id: request.id.clone(),
                            result: Value::String(String::new()),
                            error: None,
                            collection: None,
                        },
                    });
                }
            }
        }
        assert!(
            answered_protocol && answered_token,
            "handshake requests observed"
        );

        // The pool must surface the handshake error AND close the transport.
        let err = tokio::time::timeout(Duration::from_secs(2), errors.next())
            .await
            .expect("handshake error surfaces")
            .expect("error stream alive");
        assert!(
            err.to_string().contains("empty"),
            "error names the empty token: {err}"
        );
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline && handler.closed_peers.lock().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            handler.closed_peers.lock().as_slice(),
            &["p1".to_string()],
            "handshake failure must close the peer transport"
        );
        pool.cancel().await;
    }

    fn dormant_fork_state(
        collection: StdArc<RxCollection>,
        id: &str,
    ) -> StdArc<RxReplicationState> {
        RxReplicationState::new(ReplicationOptions {
            replication_identifier: id.to_string(),
            collection,
            deleted_field: "_deleted".to_string(),
            pull: None,
            push: None,
            live: true,
            retry_time: 5_000,
            auto_start: false,
            wait_for_leadership: false,
        })
    }

    /// Regression for A0.10 finding 4: replacement, peer removal, and pool
    /// cancellation must not return before displaced fork cancellation finishes.
    #[tokio::test]
    async fn fork_state_replacement_and_removal_await_cancellation() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("fork-await").await;
        let handler = MockHandler::new();
        let pool = RxWebRTCReplicationPool::new(StdArc::clone(&collection), handler);
        let peer = MockPeer("peer-fork".to_string(), 1);

        let old = dormant_fork_state(StdArc::clone(&collection), "retained-checkpoint");
        let old_finished = StdArc::new(std::sync::atomic::AtomicBool::new(false));
        let old_finished_for_cancel = StdArc::clone(&old_finished);
        old.on_cancel(Box::new(move || {
            std::thread::sleep(Duration::from_millis(40));
            old_finished_for_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        pool.add_fork_state("fork-await".to_string(), peer.clone(), old)
            .await;

        let old_peer = peer;
        // Signaling may assign a different route after reconnect, while the
        // remote storage token still selects the same checkpoint metadata.
        let peer = MockPeer("reassigned-fork-route".into(), old_peer.1 + 1);
        let replacement = dormant_fork_state(StdArc::clone(&collection), "retained-checkpoint");
        let started_at = Instant::now();
        pool.add_fork_state(
            "fork-await".to_string(),
            peer.clone(),
            StdArc::clone(&replacement),
        )
        .await;
        assert!(old_finished.load(std::sync::atomic::Ordering::SeqCst));
        assert!(started_at.elapsed() >= Duration::from_millis(40));
        assert!(StdArc::ptr_eq(
            pool.fork_states
                .lock()
                .get(&("fork-await".to_string(), peer.clone()))
                .expect("replacement active"),
            &replacement,
        ));

        pool.remove_peer(&old_peer).await;
        assert!(
            pool.fork_states
                .lock()
                .contains_key(&("fork-await".to_string(), peer.clone())),
            "late removal of the old connection must retain its replacement"
        );

        let replacement_finished = StdArc::new(std::sync::atomic::AtomicBool::new(false));
        let replacement_finished_for_cancel = StdArc::clone(&replacement_finished);
        replacement.on_cancel(Box::new(move || {
            std::thread::sleep(Duration::from_millis(40));
            replacement_finished_for_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        pool.remove_peer(&peer).await;
        assert!(replacement_finished.load(std::sync::atomic::Ordering::SeqCst));
        assert!(pool.fork_states.lock().is_empty());

        let final_state = dormant_fork_state(StdArc::clone(&collection), "final");
        let final_finished = StdArc::new(std::sync::atomic::AtomicBool::new(false));
        let final_finished_for_cancel = StdArc::clone(&final_finished);
        final_state.on_cancel(Box::new(move || {
            std::thread::sleep(Duration::from_millis(40));
            final_finished_for_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        pool.add_fork_state("fork-await".to_string(), peer, final_state)
            .await;
        pool.cancel().await;
        assert!(final_finished.load(std::sync::atomic::Ordering::SeqCst));
    }

    /// Regression for A0.10 finding 5: request work is registered before it can
    /// start, and `cancel()` aborts and joins it before any post-abort storage or
    /// response side effect can run.
    #[tokio::test]
    async fn cancel_aborts_and_joins_tracked_request_tasks() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("tracked-request").await;
        let handler = MockHandler::new();
        let pool = RxWebRTCReplicationPool::new(collection, handler);
        let started = StdArc::new(tokio::sync::Notify::new());
        let started_count = StdArc::new(std::sync::atomic::AtomicU64::new(0));
        let release = StdArc::new(tokio::sync::Notify::new());
        let storage_touches = StdArc::new(std::sync::atomic::AtomicU64::new(0));
        let responses = StdArc::new(std::sync::atomic::AtomicU64::new(0));

        for _ in 0..=MAX_CONCURRENT_REQUEST_TASKS {
            let started_for_task = StdArc::clone(&started);
            let started_count_for_task = StdArc::clone(&started_count);
            let release_for_task = StdArc::clone(&release);
            let storage_for_task = StdArc::clone(&storage_touches);
            let responses_for_task = StdArc::clone(&responses);
            pool.spawn_tracked(async move {
                started_count_for_task.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                started_for_task.notify_one();
                release_for_task.notified().await;
                storage_for_task.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                responses_for_task.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            });
        }
        while started_count.load(std::sync::atomic::Ordering::SeqCst)
            < MAX_CONCURRENT_REQUEST_TASKS as u64
        {
            started.notified().await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(
            started_count.load(std::sync::atomic::Ordering::SeqCst),
            MAX_CONCURRENT_REQUEST_TASKS as u64,
            "one excess request must remain behind the pool semaphore"
        );

        pool.cancel().await;
        release.notify_waiters();
        tokio::task::yield_now().await;

        assert_eq!(storage_touches.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(responses.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(pool.tasks.lock().is_empty());
    }

    #[tokio::test]
    async fn inbound_handshakes_progress_while_data_and_auxiliary_capacity_is_full() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("handshake-priority").await;
        let handler = MockHandler::new();
        let pool = replicate_web_rtc_multi_with_validators(
            Arc::clone(&collection.database),
            vec![collection],
            handler.clone(),
            None,
            None,
            Some("handshake-room".into()),
            Some(StdArc::<str>::from("native-session")),
        )
        .await
        .unwrap();
        let data_permits = Arc::clone(&pool.request_semaphore)
            .acquire_many_owned(MAX_CONCURRENT_REQUEST_TASKS as u32)
            .await
            .unwrap();
        let auxiliary_permits = Arc::clone(&pool.auxiliary_request_semaphore)
            .acquire_many_owned(MAX_CONCURRENT_AUXILIARY_REQUESTS as u32)
            .await
            .unwrap();
        let mut sent = handler.sent_subject.subscribe();
        handler.inject_message(
            "browser",
            master_changes_since_frame("bulk", "handshake-priority"),
        );
        handler.inject_message(
            "browser",
            ctox_protocol_frame("protocol", "handshake-priority"),
        );
        handler.inject_message(
            "browser",
            WebRTCMessage {
                id: "token".into(),
                method: "token".into(),
                params: vec![],
                collection: None,
            },
        );
        let handshakes = tokio::time::timeout(Duration::from_secs(2), async {
            let mut received = std::collections::HashSet::new();
            while received.len() < 2 {
                if let WebRTCWireFrame::Response(response) = sent.next().await.unwrap() {
                    assert_ne!(
                        response.id, "bulk",
                        "data requests must retain their capacity limit"
                    );
                    assert!(response.error.is_none());
                    match response.id.as_str() {
                        "protocol" => assert_eq!(response.result["protocol"], CTOX_RXDB_PROTOCOL),
                        "token" => assert!(response
                            .result
                            .as_str()
                            .is_some_and(|token| !token.is_empty())),
                        other => panic!("unexpected response {other}"),
                    }
                    received.insert(response.id);
                }
            }
        })
        .await;
        drop(data_permits);
        let bulk = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let WebRTCWireFrame::Response(response) = sent.next().await.unwrap() {
                    if response.id == "bulk" {
                        break;
                    }
                }
            }
        })
        .await;
        pool.cancel().await;
        drop(auxiliary_permits);
        assert!(
            handshakes.is_ok(),
            "protocol/token must not wait behind full data capacity"
        );
        assert!(
            bulk.is_ok(),
            "queued replication must resume after capacity is returned"
        );
    }

    #[tokio::test]
    async fn reserved_handshake_capacity_preserves_session_rejection() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("handshake-denial").await;
        let handler = MockHandler::new();
        let pool = replicate_web_rtc_multi_with_validators(
            Arc::clone(&collection.database),
            vec![collection],
            handler.clone(),
            None,
            Some(StdArc::new(|_, _| WebRTCPeerSessionValidation::Reject)),
            Some("handshake-room".into()),
            Some(StdArc::<str>::from("native-session")),
        )
        .await
        .unwrap();
        let permits = Arc::clone(&pool.request_semaphore)
            .acquire_many_owned(MAX_CONCURRENT_REQUEST_TASKS as u32)
            .await
            .unwrap();
        let mut sent = handler.sent_subject.subscribe();
        handler.inject_message(
            "rejected-browser",
            ctox_protocol_frame("rejected", "handshake-denial"),
        );
        let response = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let WebRTCWireFrame::Response(response) = sent.next().await.unwrap() {
                    if response.id == "rejected" {
                        break response;
                    }
                }
            }
        })
        .await;
        pool.cancel().await;
        drop(permits);
        let response = response.expect("session rejection must not wait for bulk capacity");
        assert_eq!(
            response.error.as_deref(),
            Some("peer_authentication_failed")
        );
        assert!(response.result.is_null());
        assert!(pool.authenticated_peers.lock().is_empty());
    }

    #[tokio::test]
    async fn auxiliary_requests_do_not_wait_behind_saturated_replication_work() {
        let collection =
            crate::rx_collection::test_support::test_collection_named("auxiliary-priority").await;
        let handler = MockHandler::new();
        let pool = RxWebRTCReplicationPool::new(collection, handler);
        let ordinary_permits = Arc::clone(&pool.request_semaphore)
            .acquire_many_owned(MAX_CONCURRENT_REQUEST_TASKS as u32)
            .await
            .unwrap();
        let finished = StdArc::new(tokio::sync::Notify::new());
        let finished_for_task = StdArc::clone(&finished);

        pool.spawn_auxiliary_tracked(async move {
            finished_for_task.notify_one();
        });

        tokio::time::timeout(Duration::from_millis(100), finished.notified())
            .await
            .expect("interactive auxiliary work must have a reserved execution lane");
        drop(ordinary_permits);
        pool.cancel().await;
    }
}
