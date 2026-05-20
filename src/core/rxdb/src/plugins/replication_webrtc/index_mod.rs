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

const FORK_RESYNC_INTERVAL: Duration = Duration::from_secs(5);

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
    pub ice_servers: Vec<RTCIceServer>,
    pub is_peer_valid: Option<Arc<dyn Fn(&WebRTCRsPeer) -> bool + Send + Sync>>,
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
            ice_servers: Vec::new(),
            is_peer_valid: None,
        }
    }
}

#[derive(Clone)]
struct WebRTCReplicationTuning {
    topic: Option<String>,
    pull_batch_size: u64,
    push_batch_size: u64,
    retry_time: u64,
}

impl Default for WebRTCReplicationTuning {
    fn default() -> Self {
        Self {
            topic: None,
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
    replicate_web_rtc_with_options(SyncOptionsWebRTC {
        collection: options.collection,
        connection_handler: handler,
        topic: Some(options.topic),
        is_peer_valid: options.is_peer_valid,
        pull_batch_size: 20,
        push_batch_size: 20,
        retry_time: 5_000,
    })
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
    // Answer "token" requests from remote peers.
    {
        let pool_clone = Arc::clone(&pool);
        let handler = Arc::clone(&connection_handler);
        let storage_token = storage_token.clone();
        let mut msg_stream = connection_handler.message_stream();
        let t = tokio::spawn(async move {
            while let Some(item) = msg_stream.next().await {
                if pool_clone
                    .canceled
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    break;
                }
                if item.message.method == "token" {
                    let resp = WebRTCResponse {
                        id: item.message.id,
                        result: Value::String(storage_token.clone()),
                        error: None,
                    };
                    let _ = handler
                        .send(&item.peer, WebRTCWireFrame::Response(resp))
                        .await;
                }
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
                // 1. Token handshake.
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
                let is_master =
                    is_master_in_webrtc_replication(hash_fn, &storage_token, &peer_token).await;

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
                            if item.message.method == "token" {
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
                                result: result.unwrap_or(Value::Null),
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
                        params: vec![checkpoint.unwrap_or(Value::Null), Value::from(batch_size)],
                    },
                )
                .await?;
                let docs_cp: DocumentsWithCheckpoint =
                    serde_json::from_value(answer.result.clone()).map_err(|e| {
                        new_rx_error(
                            "RC_WEBRTC_PEER",
                            Some(serde_json::json!({
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
                        params: vec![rows_json],
                    },
                )
                .await?;
                let conflicts: Vec<Value> =
                    serde_json::from_value(answer.result.clone()).unwrap_or_default();
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
) -> Option<Value> {
    match method {
        "masterChangesSince" => {
            let checkpoint = params.first().cloned();
            let batch_size = params.get(1).and_then(|v| v.as_u64()).unwrap_or(20);
            let normalized_checkpoint = match checkpoint {
                Some(Value::Null) | None => None,
                other => other,
            };
            handler
                .master_changes_since(normalized_checkpoint, batch_size)
                .await
                .ok()
                .and_then(|r| serde_json::to_value(&r).ok())
        }
        "masterWrite" => {
            let rows: Vec<crate::types::RxReplicationWriteToMasterRow> = params
                .first()
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            handler
                .master_write(rows)
                .await
                .ok()
                .map(|c| serde_json::to_value(&c).unwrap_or(Value::Null))
        }
        _ => {
            tracing::warn!(
                target: "ctox_rxdb::plugins::replication_webrtc",
                "unknown method on master handler: {method}",
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn periodic_resync_stream_emits_resync() {
        let mut stream = periodic_resync_stream(Duration::from_millis(1));
        let item = tokio::time::timeout(Duration::from_millis(100), stream.next())
            .await
            .expect("periodic resync should emit before timeout");
        assert_eq!(item, Some(RxReplicationMasterChange::Resync));
    }
}
