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

use super::protocol_contract_generated;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;
use tokio_stream::StreamExt;
use webrtc::peer_connection::RTCIceServer;

use crate::plugin::add_rx_plugin;
use crate::plugins::leader_election::RxDBLeaderElectionPlugin;
use crate::plugins::replication::{
    PullHandler, PushHandler, ReplicationOptions, ReplicationPullHandlerResult,
    ReplicationPullOptions, ReplicationPushOptions, RxReplicationState, StreamFactory,
    replicate_rx_collection,
};
use crate::plugins::replication_webrtc::connection_handler_rs::{
    CollectionAuthzHook, DocumentReadAuthzHook, WebRTCRsConfig, WebRTCRsConnectionHandler,
    WebRTCRsPeer,
};
use crate::plugins::replication_webrtc::signaling_client::SignalingClient;
use crate::plugins::replication_webrtc::webrtc_helper::{
    is_master_in_webrtc_replication, send_message_and_await_answer,
};
use crate::plugins::replication_webrtc::webrtc_types::{
    WebRTCConnectionHandler, WebRTCMessage, WebRTCResponse, WebRTCWireFrame,
};
use crate::plugins::utils::utils_string::random_token;
use crate::replication_protocol::index_mod::rx_storage_instance_to_replication_handler;
use crate::rx_collection::RxCollection;
use crate::rx_error::{RxError, new_rx_error};
use crate::rxjs_compat::RxSubject;
use crate::types::{DocumentsWithCheckpoint, RxReplicationHandler, RxReplicationMasterChange};
use protocol_contract_generated::{
    CTOX_PROTOCOL_ERROR_CAPABILITY_MISSING, CTOX_PROTOCOL_ERROR_COLLECTION_MISMATCH,
    CTOX_PROTOCOL_ERROR_MISMATCH, CTOX_PROTOCOL_ERROR_MISSING,
    CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH, CTOX_PROTOCOL_ERROR_SCHEMA_VERSION_MISMATCH,
    CTOX_QUERY_FETCH_CAPABILITY, CTOX_REQUIRED_PROTOCOL_CAPABILITIES, CTOX_RXDB_PROTOCOL,
    CTOX_RXDB_RS_SCHEMA_HASH_SOURCE,
};

const FORK_RESYNC_INTERVAL: Duration = Duration::from_secs(5);
const PROTOCOL_ROOM_PAYLOAD_CACHE_TTL: Duration = Duration::from_secs(5);
const CTOX_RXDB_NATIVE_CAPABILITIES: &[&str] = &[
    "ctox-rxdb-native-v1",
    "ctox-file-chunks-v1",
    "ctox-replication-handshake-v1",
    "ctox-schema-hash-v1",
    "ctox-peer-session-v1",
    "ctox-checkpoint-epoch-v1",
    CTOX_QUERY_FETCH_CAPABILITY,
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

#[derive(Clone)]
pub struct SyncOptionsWebRTC<H: WebRTCConnectionHandler> {
    pub collection: Arc<RxCollection>,
    pub connection_handler: Arc<H>,
    pub topic: Option<String>,
    pub is_peer_valid: Option<Arc<dyn Fn(&H::Peer) -> bool + Send + Sync>>,
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
    pub is_peer_valid: Option<Arc<dyn Fn(&WebRTCRsPeer) -> bool + Send + Sync>>,
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
    pull_batch_size: u64,
    push_batch_size: u64,
    retry_time: u64,
}

impl Default for WebRTCReplicationTuning {
    fn default() -> Self {
        Self {
            topic: None,
            peer_session_id: None,
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
    /// First registered collection — kept for back-compat with callers that
    /// expect a representative collection. Use [`Self::collections`] for the
    /// full multiplexed set.
    pub collection: Arc<RxCollection>,
    pub connection_handler: Arc<H>,
    /// Per-collection master replication handler, keyed by collection name.
    pub master_replication_handlers: HashMap<String, Arc<dyn RxReplicationHandler>>,
    pub canceled: std::sync::atomic::AtomicBool,
    pub error_subject: RxSubject<RxError>,
    pub query_fetch_registry: Arc<super::query_fetch_handler::QueryFetchRegistry>,
    pub file_fetch_registry: Arc<super::file_fetch_handler::FileFetchRegistry>,
    /// Every collection multiplexed onto this connection, keyed by name.
    collections: HashMap<String, Arc<RxCollection>>,
    /// Short-lived shared room payload for the `ctoxProtocol` handshake.
    ///
    /// A fresh Business OS browser can send multiple `ctoxProtocol` requests
    /// while the native peer also starts its own handshake. Building
    /// collectionSchemas/collectionCheckpoints for the whole multiplexed room
    /// per task fans out into hundreds of SQLite checkpoint snapshots. Share
    /// one in-flight build and reuse it briefly; individual target collection
    /// payloads are still resolved per response.
    protocol_room_payload_cache: AsyncMutex<ProtocolRoomPayloadCache>,
    /// Per-peer sub-tasks (the master-change relay tasks, one per collection).
    peer_states: Mutex<HashMap<H::Peer, PeerState>>,
    /// Fork replication states keyed by (collection, peer). One entry per
    /// collection per peer for which CTOX was elected fork.
    fork_states: Mutex<HashMap<(String, H::Peer), Arc<RxReplicationState>>>,
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
        assert!(
            !collections.is_empty(),
            "RxWebRTCReplicationPool requires at least one collection"
        );
        let representative = Arc::clone(&collections[0]);
        let registry = Arc::new(super::query_fetch_handler::QueryFetchRegistry::new(
            protocol_contract_generated::CTOX_QUERY_MAX_IN_FLIGHT_STREAMS as u64,
        ));
        let file_registry = Arc::new(super::file_fetch_handler::FileFetchRegistry::new(
            protocol_contract_generated::CTOX_QUERY_MAX_IN_FLIGHT_STREAMS as u64,
        ));
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
            collection: representative,
            connection_handler,
            master_replication_handlers,
            canceled: std::sync::atomic::AtomicBool::new(false),
            error_subject: RxSubject::new(),
            query_fetch_registry: registry,
            file_fetch_registry: file_registry,
            collections: collection_map,
            protocol_room_payload_cache: AsyncMutex::new(ProtocolRoomPayloadCache::default()),
            peer_states: Mutex::new(HashMap::new()),
            fork_states: Mutex::new(HashMap::new()),
            tasks: Mutex::new(Vec::new()),
        })
    }

    /// All collections multiplexed onto this connection.
    pub fn collections(&self) -> Vec<Arc<RxCollection>> {
        self.collections.values().cloned().collect()
    }

    fn collection_by_name(&self, name: &str) -> Option<Arc<RxCollection>> {
        self.collections.get(name).cloned()
    }

    fn master_handler_for(&self, name: &str) -> Option<Arc<dyn RxReplicationHandler>> {
        self.master_replication_handlers.get(name).cloned()
    }

    async fn protocol_room_payload(&self) -> ProtocolRoomPayload {
        let now = Instant::now();
        let mut cache = self.protocol_room_payload_cache.lock().await;
        if let (Some(payload), Some(built_at)) = (&cache.payload, cache.built_at) {
            if now.duration_since(built_at) <= PROTOCOL_ROOM_PAYLOAD_CACHE_TTL {
                return payload.clone();
            }
        }

        let collections = self.collections();
        let payload = ProtocolRoomPayload {
            collection_schemas: collection_schemas_payload(&collections).await,
            collection_checkpoints: collection_checkpoints_payload(&collections).await,
        };
        cache.built_at = Some(Instant::now());
        cache.payload = Some(payload.clone());
        payload
    }

    /// Register the per-peer relay sub-tasks (one master-change relay per
    /// collection for which CTOX is master to this peer). Fork states are
    /// tracked separately in [`Self::add_fork_state`].
    pub fn add_peer(&self, peer: H::Peer, sub_tasks: Vec<tokio::task::JoinHandle<()>>) {
        let mut states = self.peer_states.lock();
        let entry = states.entry(peer).or_insert_with(|| PeerState {
            sub_tasks: Vec::new(),
        });
        entry.sub_tasks.extend(sub_tasks);
    }

    /// Record a per-(collection, peer) fork replication state so cancel
    /// propagates on `remove_peer` / `cancel`.
    pub fn add_fork_state(
        &self,
        collection: String,
        peer: H::Peer,
        fork_state: Arc<RxReplicationState>,
    ) {
        let displaced = self
            .fork_states
            .lock()
            .insert((collection, peer), fork_state);
        if let Some(old) = displaced {
            // A re-handshake replaced this fork state (same peer id
            // reconnect). The displaced state's replication machinery runs in
            // detached tasks — without an explicit cancel it keeps pulling /
            // pushing and races the new state on the shared checkpoint meta
            // (both derive the same replication identifier).
            tokio::spawn(async move {
                old.cancel().await;
            });
        }
    }

    pub fn remove_peer(&self, peer: &H::Peer) {
        if let Some(state) = self.peer_states.lock().remove(peer) {
            for h in state.sub_tasks.into_iter() {
                h.abort();
            }
        }
        // Cancel and drop every fork state bound to this peer (all collections).
        let drained: Vec<Arc<RxReplicationState>> = {
            let mut forks = self.fork_states.lock();
            let keys: Vec<(String, H::Peer)> =
                forks.keys().filter(|(_, p)| p == peer).cloned().collect();
            keys.into_iter().filter_map(|k| forks.remove(&k)).collect()
        };
        for fork in drained.into_iter() {
            tokio::spawn(async move {
                fork.cancel().await;
            });
        }
    }

    pub async fn cancel(&self) {
        if self
            .canceled
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        // Cancel all peer sub-tasks + fork states.
        let peers: Vec<H::Peer> = self.peer_states.lock().keys().cloned().collect();
        for p in peers.iter() {
            self.remove_peer(p);
        }
        // Cancel any fork states whose peer never had a peer_states entry.
        let drained: Vec<Arc<RxReplicationState>> =
            self.fork_states.lock().drain().map(|(_, v)| v).collect();
        for fork in drained.into_iter() {
            fork.cancel().await;
        }
        // Cancel pool-level tasks.
        let tasks = std::mem::take(&mut *self.tasks.lock());
        for t in tasks.into_iter() {
            t.abort();
        }
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
    is_peer_valid: Option<Arc<dyn Fn(&H::Peer) -> bool + Send + Sync>>,
) -> Result<Arc<RxWebRTCReplicationPool<H>>, RxError>
where
    H: WebRTCConnectionHandler + 'static,
{
    replicate_web_rtc_inner(
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
    is_peer_valid: Option<Arc<dyn Fn(&H::Peer) -> bool + Send + Sync>>,
    tuning_topic: Option<String>,
    peer_session_id: Option<Arc<str>>,
) -> Result<Arc<RxWebRTCReplicationPool<H>>, RxError>
where
    H: WebRTCConnectionHandler + 'static,
{
    replicate_web_rtc_inner(
        collections,
        connection_handler,
        is_peer_valid,
        WebRTCReplicationTuning {
            topic: tuning_topic,
            peer_session_id,
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
        vec![options.collection],
        options.connection_handler,
        options.is_peer_valid,
        WebRTCReplicationTuning {
            topic: options.topic,
            peer_session_id: None,
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
        vec![options.collection],
        handler,
        options.is_peer_valid,
        WebRTCReplicationTuning {
            topic: Some(options.topic),
            peer_session_id: Some(Arc::<str>::from(options.peer_session_id)),
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
pub async fn replicate_web_rtc_rs_multi(
    collections: Vec<Arc<RxCollection>>,
    signaling_url: String,
    topic: String,
    peer_session_id: String,
    ice_servers: Vec<RTCIceServer>,
    is_peer_valid: Option<Arc<dyn Fn(&WebRTCRsPeer) -> bool + Send + Sync>>,
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
    is_peer_valid: Option<Arc<dyn Fn(&WebRTCRsPeer) -> bool + Send + Sync>>,
    collection_authz: Option<CollectionAuthzHook>,
    collection_write_authz: Option<CollectionAuthzHook>,
    document_read_authz: Option<DocumentReadAuthzHook>,
    pull_batch_size: u64,
    push_batch_size: u64,
    retry_time: u64,
) -> Result<Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>, RxError> {
    let provider = Arc::clone(&signaling_url_provider);
    let signaling = SignalingClient::connect_with_url_provider(move || provider()).await?;
    let mut config = WebRTCRsConfig::new(signaling, topic.clone());
    if !ice_servers.is_empty() {
        config.ice_servers = ice_servers;
    }
    let handler = WebRTCRsConnectionHandler::new_with_signaling(config).await?;
    // #12c: install the per-collection authz hook before peers connect.
    handler.set_collection_authz(collection_authz);
    handler.set_collection_write_authz(collection_write_authz);
    handler.set_document_read_authz(document_read_authz);
    replicate_web_rtc_inner(
        collections,
        handler,
        is_peer_valid,
        WebRTCReplicationTuning {
            topic: Some(topic),
            peer_session_id: Some(Arc::<str>::from(peer_session_id)),
            pull_batch_size,
            push_batch_size,
            retry_time,
        },
    )
    .await
}

async fn replicate_web_rtc_inner<H>(
    collections: Vec<Arc<RxCollection>>,
    connection_handler: Arc<H>,
    is_peer_valid: Option<Arc<dyn Fn(&H::Peer) -> bool + Send + Sync>>,
    tuning: WebRTCReplicationTuning,
) -> Result<Arc<RxWebRTCReplicationPool<H>>, RxError>
where
    H: WebRTCConnectionHandler + 'static,
{
    // ref: rxdb/src/plugins/replication-webrtc/index.ts:44
    let _ = add_rx_plugin(Arc::new(RxDBLeaderElectionPlugin));

    assert!(
        !collections.is_empty(),
        "replicate_web_rtc_inner requires at least one collection"
    );
    // Representative collection: every collection in a sync room shares the
    // same database, so the database-level fields (token, storage_token,
    // hash_function, multi_instance) used for leadership + master/fork
    // election are identical across the set. We read them off the first one.
    let representative = Arc::clone(&collections[0]);
    let multiplexed = collections.len() > 1;

    // ref: rxdb/src/plugins/replication-webrtc/index.ts:58-60
    if representative.database.multi_instance {
        representative.database.wait_for_leadership().await;
    }

    let storage_token = representative.database.storage_token.clone();
    let request_flag = random_token(Some(10));
    let request_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let peer_session_id = tuning.peer_session_id.clone();
    let pool = RxWebRTCReplicationPool::<H>::new_multi(
        collections.clone(),
        Arc::clone(&connection_handler),
    );

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
                pool_clone.remove_peer(&peer);
            }
        });
        pool.tasks.lock().push(t);
    }

    // ref: rxdb/src/plugins/replication-webrtc/index.ts:86-95
    // Answer control handshake requests from remote peers.
    {
        let pool_clone = Arc::clone(&pool);
        let handler = Arc::clone(&connection_handler);
        let representative = Arc::clone(&representative);
        let storage_token = storage_token.clone();
        let peer_session_id = peer_session_id.clone();
        let mut msg_stream = connection_handler.message_stream();
        let t = tokio::spawn(async move {
            while let Some(item) = msg_stream.next().await {
                if pool_clone
                    .canceled
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    break;
                }
                if item.message.method == super::query_fetch_handler::query_fetch_method() {
                    let registry = Arc::clone(&pool_clone.query_fetch_registry);
                    let handler_clone = Arc::clone(&handler);
                    let peer = item.peer.clone();
                    let peer_identity = handler.peer_identity(&peer);
                    let message = item.message.clone();
                    tokio::spawn(async move {
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
                        pool_clone.query_fetch_registry.cancel(&request_id);
                    }
                    continue;
                }
                if item.message.method == super::file_fetch_handler::file_fetch_method() {
                    let registry = Arc::clone(&pool_clone.file_fetch_registry);
                    let handler_clone = Arc::clone(&handler);
                    let peer = item.peer.clone();
                    let peer_identity = handler.peer_identity(&peer);
                    let message = item.message.clone();
                    tokio::spawn(async move {
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
                        pool_clone.file_fetch_registry.cancel(&request_id);
                    }
                    continue;
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
                let representative_task = Arc::clone(&representative);
                let storage_token = storage_token.clone();
                let peer_session_id = peer_session_id.clone();
                tokio::spawn(async move {
                    let frame_collection = item.message.collection.clone();
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
                                .unwrap_or_else(|| Arc::clone(&representative_task));
                            let room_payload = pool_task.protocol_room_payload().await;
                            ctox_protocol_response_with_flag(
                                &target,
                                peer_session_id.as_deref(),
                                flag,
                                room_payload.collection_schemas,
                                room_payload.collection_checkpoints,
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
                                .unwrap_or_else(|| representative_task.name.clone());
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
                            } else {
                                match pool_task.master_handler_for(&target_name) {
                                    Some(master) => {
                                        let document_filter = if method == "masterChangesSince" {
                                            handler_task
                                                .document_filter_for_peer(&item.peer, &target_name)
                                        } else {
                                            None
                                        };
                                        call_master_method(
                                            master.as_ref(),
                                            method,
                                            item.message.params.clone(),
                                            document_filter,
                                        )
                                        .await
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
        let representative = Arc::clone(&representative);
        let collections = collections.clone();
        let storage_token = storage_token.clone();
        let request_counter = Arc::clone(&request_counter);
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
                let representative = Arc::clone(&representative);
                let collections = collections.clone();
                let storage_token = storage_token.clone();
                let request_counter = Arc::clone(&request_counter);
                let request_flag = request_flag.clone();
                let peer_session_id = peer_session_id.clone();
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
                        format!("{}|{}|{}", representative.database.token, request_flag, n)
                    };
                    let local_flag = pool_clone.query_fetch_registry.is_feature_enabled();
                    let local_room_payload = pool_clone.protocol_room_payload().await;
                    let local_collection_schemas = local_room_payload.collection_schemas.clone();
                    let local_protocol = ctox_protocol_response_with_flag(
                        &representative,
                        peer_session_id.as_deref(),
                        local_flag,
                        local_room_payload.collection_schemas,
                        local_room_payload.collection_checkpoints,
                    )
                    .await;
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
                    // The single-collection name/hash check on `local_protocol
                    // .collection` is meaningless under multiplex (the remote's
                    // representative may differ from ours). We still enforce
                    // protocol/capability compatibility at the room level.
                    let collection_for_validation = if multiplexed {
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
                    let mut schema_mismatch_collections: std::collections::HashSet<String> =
                        std::collections::HashSet::new();
                    if multiplexed {
                        schema_mismatch_collections = collect_collection_schema_mismatches(
                            local_collection_schemas.as_ref(),
                            &protocol_response.result,
                        );
                        for collection in collections.iter() {
                            if let Some(err) = schema_mismatch_error_for(
                                &schema_mismatch_collections,
                                &collection.name,
                            ) {
                                pool_clone.error_subject.next(err);
                            }
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

                    // 2. Token handshake.
                    let req_id = {
                        let n = request_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        format!("{}|{}|{}", representative.database.token, request_flag, n)
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
                    let hash_fn = Arc::clone(&representative.database.hash_function);
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
                    if is_master {
                        // ref: rxdb/src/plugins/replication-webrtc/index.ts:134-171
                        // Master path: relay each collection's master_change_stream
                        // tagged with its collection so the fork side can
                        // demultiplex. Method-call answers are served by the
                        // message-stream loop above.
                        for collection in collections.iter() {
                            let collection_name = collection.name.clone();
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
                            let stream_task = tokio::spawn(async move {
                                let mut master_stream = master.master_change_stream();
                                while let Some(ev) = master_stream.next().await {
                                    if !handler_for_stream.is_collection_active_for_peer(
                                        &peer_for_stream,
                                        &collection_name,
                                    ) {
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
                                    let _ = handler_for_stream
                                        .send(&peer_for_stream, WebRTCWireFrame::Response(resp))
                                        .await;
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
                                    pool_clone.add_fork_state(
                                        collection.name.clone(),
                                        peer.clone(),
                                        state,
                                    );
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
/// validation wholesale. Returns `None` for single-collection rooms so the
/// legacy handshake stays byte-identical.
async fn collection_schemas_payload(collections: &[Arc<RxCollection>]) -> Option<Value> {
    if collections.len() <= 1 {
        return None;
    }
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
    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

/// Phase 3 multiplex: per-collection checkpoint-status map for the room
/// handshake. The single room-level `ctoxProtocol` answer used to carry only
/// the REPRESENTATIVE collection's checkpoint; every other collection that
/// derived its protocol from the room handshake then advertised a checkpoint
/// epoch belonging to a different collection (visible as
/// `checkpoint.collection != session.collection` in the browser's
/// peer-session evidence after a native restart). Omitted (key absent) for
/// single-collection rooms, mirroring `collection_schemas_payload`.
async fn collection_checkpoints_payload(collections: &[Arc<RxCollection>]) -> Option<Value> {
    if collections.len() <= 1 {
        return None;
    }
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
    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

async fn ctox_protocol_response_with_flag(
    collection: &Arc<RxCollection>,
    peer_session_id: Option<&str>,
    query_demand_loading_enabled: bool,
    collection_schemas: Option<Value>,
    collection_checkpoints: Option<Value>,
) -> Value {
    let checkpoint = collection
        .storage_instance
        .replication_checkpoint_status()
        .await;
    let collection_payload = match &collection.schema {
        Some(schema) => serde_json::json!({
            "name": collection.name,
            "schemaVersion": schema.version(),
            "schemaHash": schema.hash().await,
            "schemaHashSource": CTOX_RXDB_RS_SCHEMA_HASH_SOURCE,
            "checkpoint": checkpoint,
        }),
        None => Value::Null,
    };
    ctox_protocol_response_payload_with_flag(
        collection_payload,
        peer_session_id,
        query_demand_loading_enabled,
        collection_schemas,
        collection_checkpoints,
    )
}

#[cfg(test)]
fn ctox_protocol_response_payload(collection: Value, peer_session_id: Option<&str>) -> Value {
    ctox_protocol_response_payload_with_flag(collection, peer_session_id, true, None, None)
}

fn ctox_protocol_response_payload_with_flag(
    collection: Value,
    peer_session_id: Option<&str>,
    query_demand_loading_enabled: bool,
    collection_schemas: Option<Value>,
    collection_checkpoints: Option<Value>,
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
            "role": "ctox_instance",
            "sessionId": peer_session_id,
        },
        "v1_5": {
            "queryDemandLoadingEnabled": query_demand_loading_enabled,
        },
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
        // The remote did not advertise per-collection schemas (older peer or
        // single-collection room on its side). Fall back to no per-collection
        // flagging — the room-level protocol/capability check already passed.
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
    if value.is_empty() { None } else { Some(value) }
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
        auto_start: true,
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

/// Dispatch a method call on the master replication handler.
async fn call_master_method(
    handler: &dyn RxReplicationHandler,
    method: &str,
    params: Vec<Value>,
    document_filter: Option<Arc<dyn Fn(&Value) -> bool + Send + Sync>>,
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
        assert!(
            result["message"]
                .as_str()
                .unwrap_or_default()
                .contains("masterWrite request decode")
        );
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
        assert!(
            capabilities
                .iter()
                .any(|value| value.as_str() == Some("ctox-replication-handshake-v1"))
        );
        assert!(
            capabilities
                .iter()
                .any(|value| value.as_str() == Some("ctox-schema-hash-v1"))
        );
        assert!(
            capabilities
                .iter()
                .any(|value| value.as_str() == Some("ctox-peer-session-v1"))
        );
        assert!(
            capabilities
                .iter()
                .any(|value| value.as_str() == Some("ctox-checkpoint-epoch-v1"))
        );
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
            assert!(
                required
                    .iter()
                    .any(|item| item.as_str() == Some(*capability))
            );
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
    fn missing_remote_collection_schemas_map_skips_per_collection_check() {
        let local = local_schemas_two();
        // Older/single-collection remote: no `collectionSchemas` key at all.
        let remote = serde_json::json!({
            "protocol": CTOX_RXDB_PROTOCOL,
            "capabilities": CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
        });
        let mismatches = collect_collection_schema_mismatches(Some(&local), &remote);
        assert!(mismatches.is_empty());
    }

    #[tokio::test]
    async fn collection_schemas_payload_is_none_for_zero_or_one_collection() {
        // Single-collection (and empty) rooms must keep the legacy handshake
        // byte-identical: no `collectionSchemas` map. The guard is a pure
        // length check, so an empty slice exercises the same branch a
        // one-collection slice would.
        let payload = collection_schemas_payload(&[]).await;
        assert!(
            payload.is_none(),
            "rooms with <= 1 collection must omit the per-collection schema map"
        );
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
        );
        assert!(single.get("collectionSchemas").is_none());
        assert!(single.get("collectionCheckpoints").is_none());
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
        );
        assert!(multi.get("collectionSchemas").is_some());
        assert!(
            multi
                .pointer("/collectionSchemas/desktop_files/schemaHash")
                .is_some()
        );
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
    struct MockPeer(String);

    struct MockHandler {
        connect: crate::rxjs_compat::RxSubject<MockPeer>,
        disconnect: crate::rxjs_compat::RxSubject<MockPeer>,
        message: crate::rxjs_compat::RxSubject<PeerWithMessage<MockPeer>>,
        response: crate::rxjs_compat::RxSubject<PeerWithResponse<MockPeer>>,
        error: crate::rxjs_compat::RxSubject<RxError>,
        sent: StdArc<PlMutex<Vec<WebRTCWireFrame>>>,
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
                closed_peers: StdArc::new(PlMutex::new(Vec::new())),
            })
        }

        /// Feed an inbound request frame as if it arrived from `peer`.
        fn inject_message(&self, peer: &str, message: WebRTCMessage) {
            self.message.next(PeerWithMessage {
                peer: MockPeer(peer.to_string()),
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
            self.sent.lock().push(frame);
            Ok(())
        }
        async fn close(&self) -> Result<(), RxError> {
            Ok(())
        }
        async fn close_peer(&self, peer: &MockPeer) {
            self.closed_peers.lock().push(peer.0.clone());
        }
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

    #[tokio::test]
    async fn ctox_protocol_response_carries_multiplex_room_payload() {
        let alpha = crate::rx_collection::test_support::test_collection_named("proto_alpha").await;
        let beta = crate::rx_collection::test_support::test_collection_named("proto_beta").await;
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

    /// Phase 3: two collections multiplexed on ONE handler must demultiplex
    /// `masterChangesSince` frames to the right collection's master handler.
    /// We insert distinct docs into each collection, push a collection-tagged
    /// request for each over the single mock handler, and assert each answer
    /// carries ONLY that collection's documents (and echoes its collection).
    #[tokio::test]
    async fn two_collections_demux_master_changes_on_one_handler() {
        let alpha = crate::rx_collection::test_support::test_collection_named("alpha").await;
        let beta = crate::rx_collection::test_support::test_collection_named("beta").await;
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
        let peer = MockPeer("p1".to_string());
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
                MockPeer("p1".to_string()),
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

        let peer = MockPeer("p1".to_string());
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
}
