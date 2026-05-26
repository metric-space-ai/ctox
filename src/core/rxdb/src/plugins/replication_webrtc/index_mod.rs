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

#[path = "protocol_contract_generated.rs"]
mod protocol_contract_generated;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::Value;
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
    WebRTCRsConfig, WebRTCRsConnectionHandler, WebRTCRsPeer,
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
use crate::rx_error::{new_rx_error, RxError};
use crate::rxjs_compat::RxSubject;
use crate::types::{DocumentsWithCheckpoint, RxReplicationHandler, RxReplicationMasterChange};
use protocol_contract_generated::{
    CTOX_PROTOCOL_ERROR_CAPABILITY_MISSING, CTOX_PROTOCOL_ERROR_COLLECTION_MISMATCH,
    CTOX_PROTOCOL_ERROR_MISMATCH, CTOX_PROTOCOL_ERROR_MISSING,
    CTOX_PROTOCOL_ERROR_SCHEMA_HASH_MISMATCH, CTOX_PROTOCOL_ERROR_SCHEMA_VERSION_MISMATCH,
    CTOX_REQUIRED_PROTOCOL_CAPABILITIES, CTOX_RXDB_PROTOCOL, CTOX_RXDB_RS_SCHEMA_HASH_SOURCE,
};

const FORK_RESYNC_INTERVAL: Duration = Duration::from_secs(5);
const CTOX_RXDB_NATIVE_CAPABILITIES: &[&str] = &[
    "ctox-rxdb-native-v1",
    "ctox-file-chunks-v1",
    "ctox-replication-handshake-v1",
    "ctox-schema-hash-v1",
    "ctox-peer-session-v1",
    "ctox-checkpoint-epoch-v1",
];

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
pub struct RxWebRTCReplicationPool<H: WebRTCConnectionHandler> {
    pub collection: Arc<RxCollection>,
    pub connection_handler: Arc<H>,
    pub master_replication_handler: Arc<dyn RxReplicationHandler>,
    pub canceled: std::sync::atomic::AtomicBool,
    pub error_subject: RxSubject<RxError>,
    peer_states: Mutex<HashMap<H::Peer, PeerState<H>>>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

struct PeerState<H: WebRTCConnectionHandler> {
    _peer: H::Peer,
    sub_tasks: Vec<tokio::task::JoinHandle<()>>,
    fork_state: Option<Arc<RxReplicationState>>,
}

impl<H: WebRTCConnectionHandler + 'static> RxWebRTCReplicationPool<H> {
    pub fn new(collection: Arc<RxCollection>, connection_handler: Arc<H>) -> Arc<Self> {
        let master_replication_handler = rx_storage_instance_to_replication_handler(
            Arc::clone(&collection.storage_instance),
            Arc::clone(&collection.conflict_handler),
            collection.database.token.clone(),
            false, // keep_meta = false (upstream default)
        );
        Arc::new(Self {
            collection,
            connection_handler,
            master_replication_handler,
            canceled: std::sync::atomic::AtomicBool::new(false),
            error_subject: RxSubject::new(),
            peer_states: Mutex::new(HashMap::new()),
            tasks: Mutex::new(Vec::new()),
        })
    }

    pub fn add_peer(
        &self,
        peer: H::Peer,
        sub_tasks: Vec<tokio::task::JoinHandle<()>>,
        fork_state: Option<Arc<RxReplicationState>>,
    ) {
        let mut states = self.peer_states.lock();
        states.insert(
            peer.clone(),
            PeerState {
                _peer: peer,
                sub_tasks,
                fork_state,
            },
        );
    }

    pub fn remove_peer(&self, peer: &H::Peer) {
        if let Some(state) = self.peer_states.lock().remove(peer) {
            for h in state.sub_tasks.into_iter() {
                h.abort();
            }
            if let Some(fork) = state.fork_state {
                tokio::spawn(async move {
                    fork.cancel().await;
                });
            }
        }
    }

    pub async fn cancel(&self) {
        if self
            .canceled
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        // Cancel all peer sub-tasks.
        let peers: Vec<H::Peer> = self.peer_states.lock().keys().cloned().collect();
        for p in peers.iter() {
            self.remove_peer(p);
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
        collection,
        connection_handler,
        is_peer_valid,
        WebRTCReplicationTuning::default(),
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
        options.collection,
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
        options.collection,
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

async fn replicate_web_rtc_inner<H>(
    collection: Arc<RxCollection>,
    connection_handler: Arc<H>,
    is_peer_valid: Option<Arc<dyn Fn(&H::Peer) -> bool + Send + Sync>>,
    tuning: WebRTCReplicationTuning,
) -> Result<Arc<RxWebRTCReplicationPool<H>>, RxError>
where
    H: WebRTCConnectionHandler + 'static,
{
    // ref: rxdb/src/plugins/replication-webrtc/index.ts:44
    let _ = add_rx_plugin(Arc::new(RxDBLeaderElectionPlugin));

    // ref: rxdb/src/plugins/replication-webrtc/index.ts:58-60
    if collection.database.multi_instance {
        collection.database.wait_for_leadership().await;
    }

    let storage_token = collection.database.storage_token.clone();
    let request_flag = random_token(Some(10));
    let request_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let peer_session_id = tuning.peer_session_id.clone();
    let pool =
        RxWebRTCReplicationPool::<H>::new(Arc::clone(&collection), Arc::clone(&connection_handler));

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
        let collection = Arc::clone(&collection);
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
                let result = match item.message.method.as_str() {
                    "token" => Some(Value::String(storage_token.clone())),
                    "ctoxProtocol" => {
                        Some(ctox_protocol_response(&collection, peer_session_id.as_deref()).await)
                    }
                    _ => None,
                };
                let Some(result) = result else {
                    continue;
                };
                let resp = WebRTCResponse {
                    id: item.message.id,
                    result,
                    error: None,
                };
                let _ = handler
                    .send(&item.peer, WebRTCWireFrame::Response(resp))
                    .await;
            }
        });
        pool.tasks.lock().push(t);
    }

    // ref: rxdb/src/plugins/replication-webrtc/index.ts:97-221
    // On new peer: handshake, pick master/fork, register handlers.
    {
        let pool_clone = Arc::clone(&pool);
        let handler = Arc::clone(&connection_handler);
        let collection = Arc::clone(&collection);
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
                // 1. CTOX protocol handshake. Rust actively reads the remote
                // role so Browser/CTOX pairs make the same deterministic
                // master/fork decision instead of relying on random storage
                // token ordering after reconnects.
                let req_id = {
                    let n = request_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    format!("{}|{}|{}", collection.database.token, request_flag, n)
                };
                let local_protocol =
                    ctox_protocol_response(&collection, peer_session_id.as_deref()).await;
                let protocol_response = match send_message_and_await_answer(
                    Arc::clone(&handler),
                    peer.clone(),
                    WebRTCMessage {
                        id: req_id,
                        method: "ctoxProtocol".to_string(),
                        params: vec![local_protocol.clone()],
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
                        continue;
                    }
                };
                if let Err(e) = validate_ctox_protocol_response(
                    &protocol_response.result,
                    local_protocol.get("collection"),
                    true,
                ) {
                    pool_clone.error_subject.next(e);
                    continue;
                }
                let remote_peer_role = protocol_response
                    .result
                    .pointer("/peerSession/role")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();

                // 2. Token handshake.
                let req_id = {
                    let n = request_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    format!("{}|{}|{}", collection.database.token, request_flag, n)
                };
                let token_response = match send_message_and_await_answer(
                    Arc::clone(&handler),
                    peer.clone(),
                    WebRTCMessage {
                        id: req_id,
                        method: "token".to_string(),
                        params: vec![],
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
                        continue;
                    }
                };
                let peer_token = token_response
                    .result
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let hash_fn = Arc::clone(&collection.database.hash_function);
                let elected_master =
                    is_master_in_webrtc_replication(hash_fn, &storage_token, &peer_token).await;
                let is_master = if remote_peer_role == "browser" {
                    true
                } else {
                    elected_master
                };

                let mut peer_sub_tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();
                if is_master {
                    // ref: rxdb/src/plugins/replication-webrtc/index.ts:134-171
                    // Master path: relay our master_change_stream + answer method calls.
                    let pool_for_stream = Arc::clone(&pool_clone);
                    let handler_for_stream = Arc::clone(&handler);
                    let peer_for_stream = peer.clone();
                    let stream_task = tokio::spawn(async move {
                        let mut master_stream = pool_for_stream
                            .master_replication_handler
                            .master_change_stream();
                        while let Some(ev) = master_stream.next().await {
                            let resp = WebRTCResponse {
                                id: "masterChangeStream$".to_string(),
                                result: serde_json::to_value(&ev).unwrap_or(Value::Null),
                                error: None,
                            };
                            let _ = handler_for_stream
                                .send(&peer_for_stream, WebRTCWireFrame::Response(resp))
                                .await;
                        }
                    });
                    peer_sub_tasks.push(stream_task);

                    let pool_for_msgs = Arc::clone(&pool_clone);
                    let handler_for_msgs = Arc::clone(&handler);
                    let peer_for_msgs = peer.clone();
                    let msg_task = tokio::spawn(async move {
                        let mut msgs = handler_for_msgs.message_stream();
                        while let Some(item) = msgs.next().await {
                            if item.peer != peer_for_msgs {
                                continue;
                            }
                            if item.message.method == "token"
                                || item.message.method == "ctoxProtocol"
                            {
                                continue;
                            }
                            let result = call_master_method(
                                pool_for_msgs.master_replication_handler.as_ref(),
                                &item.message.method,
                                item.message.params.clone(),
                            )
                            .await;
                            let resp = WebRTCResponse {
                                id: item.message.id.clone(),
                                result,
                                error: None,
                            };
                            let _ = handler_for_msgs
                                .send(&item.peer, WebRTCWireFrame::Response(resp))
                                .await;
                        }
                    });
                    peer_sub_tasks.push(msg_task);
                    pool_clone.add_peer(peer, peer_sub_tasks, None);
                } else {
                    // ref: rxdb/src/plugins/replication-webrtc/index.ts:172-218
                    // Fork path: build pull.handler / push.handler closures that
                    // tunnel `masterChangesSince` / `masterWrite` over the peer
                    // via send_message_and_await_answer, and feed
                    // `masterChangeStream$` responses into the pull stream.
                    let fork_state = build_fork_replication_state(
                        Arc::clone(&collection),
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
                            pool_clone.add_peer(peer, peer_sub_tasks, Some(state));
                        }
                        Err(e) => {
                            pool_clone.error_subject.next(e);
                            pool_clone.add_peer(peer, peer_sub_tasks, None);
                        }
                    }
                }
            }
        });
        pool.tasks.lock().push(t);
    }

    Ok(pool)
}

async fn ctox_protocol_response(
    collection: &Arc<RxCollection>,
    peer_session_id: Option<&str>,
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
    ctox_protocol_response_payload(collection_payload, peer_session_id)
}

fn ctox_protocol_response_payload(collection: Value, peer_session_id: Option<&str>) -> Value {
    let peer_session_id = peer_session_id
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("rxdb-rs-{}", random_token(Some(16))));
    serde_json::json!({
        "protocol": CTOX_RXDB_PROTOCOL,
        "capabilities": CTOX_RXDB_NATIVE_CAPABILITIES,
        "collection": collection,
        "peerSession": {
            "role": "ctox_instance",
            "sessionId": peer_session_id,
        },
    })
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
        Arc::new(move |checkpoint, batch_size| {
            let handler = Arc::clone(&handler);
            let peer = peer.clone();
            let next_request_id = Arc::clone(&next_request_id);
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
        Arc::new(move |rows| {
            let handler = Arc::clone(&handler);
            let peer = peer.clone();
            let next_request_id = Arc::clone(&next_request_id);
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
        Arc::new(move || {
            let handler = Arc::clone(&handler);
            let peer_for_filter = peer.clone();
            let remote_master_events = handler.response_stream().filter_map(move |item| {
                if item.peer != peer_for_filter || item.response.id != "masterChangeStream$" {
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
                Ok(result) => serde_json::to_value(&result).unwrap_or_else(|e| {
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
                }),
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
}
