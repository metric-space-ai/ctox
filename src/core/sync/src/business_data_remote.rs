//! Policy-checked native BusinessData source and client transport.
//!
//! Cursors are random server references, not revisions. Queries page one
//! storage snapshot. Watches retain a bounded, replayable history; unavailable
//! history produces an explicit Reset. Every server request revalidates the
//! capability and scope. The client pump buffers bounded events until its
//! request response has been released to the private IPC stream.

use crate::business_data_contract::{
    NativeBusinessDataBinding as Binding, NativeBusinessDataCommand as Command,
    NativeBusinessDataCommandState as CommandState, NativeBusinessDataCommandStatus,
    NativeBusinessDataErrorCode as ErrorCode, NativeBusinessDataEvent as Event,
    NativeBusinessDataEventPayload as EventPayload, NativeBusinessDataOperation as Operation,
    NativeBusinessDataQuery as Query, NativeBusinessDataRecord as Record,
    NativeBusinessDataRequest as Request, NativeBusinessDataResponse as Response,
    NativeBusinessDataResult as Result, NativeBusinessDataScope as Scope,
    NativeBusinessDataSessionRef as SessionRef, CTOX_BUSINESS_DATA_MAX_SNAPSHOT_BYTES,
    CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
};
use crate::native::NativePool;
use async_trait::async_trait;
use futures_util::StreamExt;
use ring::rand::{SecureRandom, SystemRandom};
use rxdb::plugins::replication_webrtc::{
    send_message_and_await_answer, WebRTCMessage, WebRTCRsConnection, WebRTCRsConnectionHandler,
    WebRTCWireFrame,
};
use rxdb::rx_collection::RxCollection;
use rxdb::rx_database::RxDatabase;
use rxdb::rx_query_helper::{normalize_mango_query, prepare_query};
use rxdb::types::storage::RxStorageSnapshotEvent;
use rxdb::types::MangoQuery;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{mpsc, Notify};

pub const BUSINESS_DATA_RPC_METHOD: &str = "ctox.business_data.v1";
pub const BUSINESS_DATA_EVENT_METHOD: &str = "ctox.business_data.event.v1";
const MAX_SNAPSHOTS_PER_IDENTITY: usize = 16;
const MAX_SUBSCRIPTIONS_PER_IDENTITY: usize = 16;
const RETAINED_EVENTS: usize = 512;
const CLIENT_EVENT_BUFFER: usize = 64;
const REMOTE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteIdentity {
    pub user_id: String,
    pub authorization_epoch: u64,
    pub instance_id: String,
}

impl RemoteIdentity {
    fn key(&self) -> String {
        format!(
            "{}\u{0}{}\u{0}{}",
            self.user_id, self.authorization_epoch, self.instance_id
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

#[async_trait]
pub trait BusinessDataAccessPolicy: Send + Sync {
    async fn identity(&self, capability_token: &str) -> Option<RemoteIdentity>;
    async fn authorize(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        collection: &str,
        access: Access,
        scope: &Scope,
    ) -> io::Result<()>;
    async fn submit_command(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        command: &Command,
    ) -> io::Result<CommandState>;
    async fn command_state(
        &self,
        identity: &RemoteIdentity,
        document: &Value,
    ) -> io::Result<CommandState>;
}

#[derive(Debug)]
struct RemoteError {
    code: ErrorCode,
    message: String,
    retryable: bool,
}

impl RemoteError {
    fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidRequest, message, false)
    }
    fn unauthorized() -> Self {
        Self::new(
            ErrorCode::Unauthorized,
            "BusinessData capability was not current",
            false,
        )
    }
    fn limit(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::LimitExceeded, message, true)
    }
    fn reset() -> Self {
        Self::new(
            ErrorCode::ResetRequired,
            "BusinessData cursor expired",
            false,
        )
    }
    fn response(self, request_id: &str) -> Response {
        Response {
            version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
            request_id: request_id.to_owned(),
            result: Result::Rejected {
                code: self.code,
                message: self.message,
                retryable: self.retryable,
            },
        }
    }
}

fn response_result(request_id: &str, result: Result) -> Response {
    Response {
        version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
        request_id: request_id.to_owned(),
        result,
    }
}

fn random_token(label: &str) -> io::Result<String> {
    let mut bytes = [0_u8; 24];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| io::Error::other("BusinessData token generation failed"))?;
    Ok(format!(
        "{label}_{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

fn query_fingerprint(query: &Query) -> String {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(query).unwrap_or_default());
    format!("sha256:{:x}", hasher.finalize())
}

enum SnapshotItem {
    Batch(Vec<Value>, bool),
    Failed,
}

struct SnapshotSession {
    snapshot_id: String,
    identity_key: String,
    fingerprint: String,
    collection: String,
    receiver: tokio::sync::Mutex<mpsc::Receiver<SnapshotItem>>,
    complete: AtomicBool,
    task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

struct HistoryEntry {
    sequence: u64,
    cursor: String,
    payload: EventPayload,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WatchMode {
    Query,
    Command,
}

struct Subscription {
    subscription_id: String,
    identity_key: String,
    fingerprint: String,
    selector: Value,
    mode: WatchMode,
    sequence: AtomicU64,
    history: Mutex<VecDeque<HistoryEntry>>,
    history_complete: AtomicBool,
    terminal: AtomicBool,
    peer: tokio::sync::Mutex<Option<WebRTCRsConnection>>,
    resume: tokio::sync::Mutex<Option<u64>>,
    notify: Notify,
    task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    cursors: Arc<Mutex<HashMap<String, CursorRecord>>>,
    sender: Arc<WebRtcEventSender>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CursorTarget {
    Snapshot,
    Watch,
}

#[derive(Clone)]
struct CursorRecord {
    target: CursorTarget,
    target_id: String,
    identity_key: String,
    fingerprint: String,
    sequence: u64,
}

struct WebRtcEventSender {
    handler: Arc<WebRTCRsConnectionHandler>,
}

impl WebRtcEventSender {
    async fn send(&self, peer: &WebRTCRsConnection, event: &Event) -> Result<(), String> {
        let value = serde_json::to_value(event).map_err(|_| "event encoding failed".to_string())?;
        let id =
            random_token("business-data-event").map_err(|_| "id generation failed".to_string())?;
        self.handler
            .send(
                peer,
                WebRTCWireFrame::Message(WebRTCMessage {
                    id,
                    method: BUSINESS_DATA_EVENT_METHOD.into(),
                    params: vec![value],
                    collection: None,
                }),
            )
            .await
            .map_err(|error| error.to_string())
    }
}

/// Native RxDB-backed source. It is registered on one WebRTC pool and does not
/// expose HTTP, a secondary store, or local privileged bypasses.
pub struct BusinessDataSource {
    database: Arc<RxDatabase>,
    policy: Arc<dyn BusinessDataAccessPolicy>,
    snapshots: Mutex<HashMap<String, HashMap<String, Arc<SnapshotSession>>>>,
    subscriptions: Mutex<HashMap<String, HashMap<String, Arc<Subscription>>>>,
    cursors: Arc<Mutex<HashMap<String, CursorRecord>>>,
    sender: Arc<WebRtcEventSender>,
}

impl BusinessDataSource {
    pub fn new(
        database: Arc<RxDatabase>,
        policy: Arc<dyn BusinessDataAccessPolicy>,
        handler: Arc<WebRTCRsConnectionHandler>,
    ) -> Arc<Self> {
        Arc::new(Self {
            database,
            policy,
            snapshots: Mutex::default(),
            subscriptions: Mutex::default(),
            cursors: Arc::default(),
            sender: Arc::new(WebRtcEventSender { handler }),
        })
    }

    /// Register the typed private WebRTC request handler.
    pub fn register(self: &Arc<Self>, pool: &NativePool) -> Result<(), rxdb::rx_error::RxError> {
        let source = self.clone();
        pool.register_auxiliary_request_handler(
            BUSINESS_DATA_RPC_METHOD,
            Arc::new(move |peer_identity, capability_token, params| {
                let source = source.clone();
                let handler = source.sender.handler.clone();
                Box::pin(async move {
                    if params.len() != 1 {
                        return Err("invalid BusinessData request envelope".into());
                    }
                    #[derive(serde::Deserialize)]
                    struct Envelope {
                        request: Request,
                    }
                    let envelope: Envelope =
                        serde_json::from_value(params.into_iter().next().unwrap())
                            .map_err(|_| "invalid BusinessData request envelope".to_string())?;
                    let peer = handler
                        .connection_for_peer(&peer_identity)
                        .ok_or_else(|| "BusinessData connection retired".to_string())?;
                    source
                        .handle(&peer, &capability_token, envelope.request)
                        .await
                        .map_err(|error| error.to_string())
                        .and_then(|response| {
                            serde_json::to_value(response)
                                .map_err(|_| "response encoding failed".into())
                        })
                })
            }),
        )
    }

    pub async fn shutdown(&self) -> io::Result<()> {
        let mut failure = None;
        let snapshots: Vec<_> = self
            .snapshots
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .flat_map(|values| values.values().cloned())
            .collect();
        for snapshot in snapshots {
            if let Some(task) = snapshot.task.lock().await.take() {
                task.abort();
                let _ = task.await;
            }
        }
        let subscriptions: Vec<_> = self
            .subscriptions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .flat_map(|values| values.values().cloned())
            .collect();
        for subscription in subscriptions {
            if let Some(task) = subscription.task.lock().await.take() {
                task.abort();
                let _ = task.await;
            }
        }
        self.snapshots
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.subscriptions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.cursors
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        failure
    }

    async fn handle(
        self: &Arc<Self>,
        peer: &WebRTCRsConnection,
        capability_token: &str,
        request: Request,
    ) -> io::Result<Response> {
        if request.version != CTOX_BUSINESS_DATA_PROTOCOL_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported source version",
            ));
        }
        let identity = self
            .policy
            .identity(capability_token)
            .await
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::PermissionDenied, "capability is not current")
            })?;
        let response = self
            .process(peer, &identity, capability_token, &request)
            .await
            .unwrap_or_else(|error| error.response(&request.request_id));
        Ok(response)
    }

    async fn process(
        self: &Arc<Self>,
        peer: &WebRTCRsConnection,
        identity: &RemoteIdentity,
        capability_token: &str,
        request: &Request,
    ) -> Result<Response, RemoteError> {
        let identity_key = identity.key();
        match &request.operation {
            Operation::Query {
                session,
                query,
                page_cursor,
            } => {
                self.query(
                    identity,
                    request.request_id.as_str(),
                    session,
                    query,
                    page_cursor.as_deref(),
                    capability_token,
                )
                .await
            }
            Operation::Watch {
                session,
                query,
                resume_cursor,
            } => {
                self.watch(
                    peer,
                    identity,
                    capability_token,
                    request.request_id.as_str(),
                    session,
                    query,
                    resume_cursor.as_deref(),
                    WatchMode::Query,
                )
                .await
            }
            Operation::Unwatch {
                session,
                subscription_id,
            } => {
                self.unwatch(
                    &identity_key,
                    request.request_id.as_str(),
                    session,
                    subscription_id,
                )
                .await
            }
            Operation::SubmitCommand { session, command } => {
                self.policy
                    .authorize(
                        identity,
                        capability_token,
                        "business_commands",
                        Access::Write,
                        &Scope::Instance {},
                    )
                    .await
                    .map_err(|_| RemoteError::unauthorized())?;
                let state = self
                    .policy
                    .submit_command(identity, capability_token, command)
                    .await
                    .map_err(|error| {
                        RemoteError::new(ErrorCode::Unauthorized, error.to_string(), false)
                    })?;
                Ok(response_result(
                    request.request_id.as_str(),
                    Result::Command {
                        session: session.clone(),
                        state,
                    },
                ))
            }
            Operation::ObserveCommand {
                session,
                command_id,
            } => {
                let query = command_watch_query(command_id);
                let response = self
                    .watch(
                        peer,
                        identity,
                        capability_token,
                        request.request_id.as_str(),
                        session,
                        &query,
                        None,
                        WatchMode::Command,
                    )
                    .await?;
                let Result::Subscribed { session, .. } = response.result else {
                    return Err(RemoteError::new(
                        ErrorCode::Internal,
                        "command observation failed",
                        false,
                    ));
                };
                let state = self
                    .policy
                    .command_state(identity, &json!({ "id": command_id }))
                    .await
                    .map_err(|error| {
                        RemoteError::new(ErrorCode::Unauthorized, error.to_string(), false)
                    })?;
                Ok(response_result(
                    request.request_id.as_str(),
                    Result::Command { session, state },
                ))
            }
            Operation::Open { .. } | Operation::Status { .. } | Operation::Close { .. } => {
                Err(RemoteError::new(
                    ErrorCode::Unsupported,
                    "lifecycle belongs to the native session",
                    false,
                ))
            }
        }
    }

    fn collection(&self, name: &str) -> Result<Arc<RxCollection>, RemoteError> {
        self.database.collection(name).ok_or_else(|| {
            RemoteError::new(
                ErrorCode::Unsupported,
                "collection is not registered",
                false,
            )
        })
    }

    async fn prepare(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        query: &Query,
        access: Access,
    ) -> Result<(Arc<RxCollection>, Value, String), RemoteError> {
        self.policy
            .authorize(
                identity,
                capability_token,
                &query.collection,
                access,
                &query.scope,
            )
            .await
            .map_err(|error| RemoteError::new(ErrorCode::Unauthorized, error.to_string(), false))?;
        let collection = self.collection(&query.collection)?;
        let schema = collection.schema.as_ref().ok_or_else(|| {
            RemoteError::new(ErrorCode::SchemaMismatch, "collection has no schema", false)
        })?;
        let mut mango: MangoQuery = serde_json::from_value(query.query.clone())
            .map_err(|error| RemoteError::invalid(format!("invalid Mango query: {error}")))?;
        if mango.sort.as_ref().map_or(true, |sort| sort.is_empty()) {
            return Err(RemoteError::invalid(
                "query must provide supported ordering",
            ));
        }
        mango.selector = Some(scope_selector(&query.scope, mango.selector.take()));
        let normalized = normalize_mango_query(schema, mango);
        let prepared = prepare_query(schema, normalized).map_err(|error| {
            RemoteError::new(ErrorCode::SchemaMismatch, error.to_string(), false)
        })?;
        Ok((collection, prepared, query_fingerprint(query)))
    }

    async fn query(
        self: &Arc<Self>,
        identity: &RemoteIdentity,
        request_id: &str,
        session: &SessionRef,
        query: &Query,
        cursor: Option<&str>,
        capability_token: &str,
    ) -> Result<Response, RemoteError> {
        let identity_key = identity.key();
        let snapshot = match cursor {
            Some(cursor) => {
                let record = self.cursor(cursor).ok_or_else(RemoteError::reset)?;
                if record.target != CursorTarget::Snapshot
                    || record.identity_key != identity_key
                    || record.fingerprint != query_fingerprint(query)
                {
                    return Err(RemoteError::reset());
                }
                self.snapshots
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .get(identity_key)
                    .and_then(|values| values.get(&record.target_id))
                    .cloned()
                    .ok_or_else(RemoteError::reset)?;
                self.policy
                    .authorize(
                        identity,
                        capability_token,
                        &query.collection,
                        Access::Read,
                        &query.scope,
                    )
                    .await
                    .map_err(|error| {
                        RemoteError::new(ErrorCode::Unauthorized, error.to_string(), false)
                    })?;
            }
            None => {
                let (collection, prepared, fingerprint) = self
                    .prepare_for_snapshot(identity_key, query, capability_token)
                    .await?;
                self.start_snapshot(identity_key, collection, prepared, fingerprint, query)
                    .await?
            }
        };
        let mut receiver = snapshot.receiver.lock().await;
        let (documents, last_batch) = match receiver.recv().await {
            Some(SnapshotItem::Batch(documents, last_batch)) => (documents, last_batch),
            Some(SnapshotItem::Failed) | None => (Vec::new(), true),
        };
        let records = documents.into_iter().map(document_record).collect();
        let has_more = !last_batch;
        if last_batch {
            snapshot.complete.store(true, Ordering::SeqCst);
        }
        drop(receiver);
        let next_cursor = if has_more {
            Some(self.insert_cursor(
                CursorTarget::Snapshot,
                snapshot.snapshot_id.clone(),
                identity_key,
                snapshot.fingerprint.clone(),
                0,
            )?)
        } else {
            if let Some(task) = snapshot.task.lock().await.take() {
                let _ = task.await;
            }
            self.snapshots
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get_mut(identity_key)
                .and_then(|values| values.remove(&snapshot.snapshot_id));
            None
        };
        Ok(response_result(
            request_id,
            Result::Page {
                session: session.clone(),
                snapshot_id: snapshot.snapshot_id.clone(),
                records,
                next_page_cursor: next_cursor,
                snapshot_complete: !has_more,
            },
        ))
    }

    async fn prepare_for_snapshot(
        &self,
        identity_key: &str,
        query: &Query,
        capability_token: &str,
    ) -> Result<(Arc<RxCollection>, Value, String), RemoteError> {
        {
            let snapshots = self
                .snapshots
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if snapshots.get(identity_key).map_or(0, |values| values.len())
                >= MAX_SNAPSHOTS_PER_IDENTITY
            {
                return Err(RemoteError::limit("BusinessData snapshot limit exceeded"));
            }
        }
        let (collection, prepared, fingerprint) = self
            .prepare_authorized(identity_key, query, capability_token)
            .await?;
        Ok((collection, prepared, fingerprint))
    }

    async fn prepare_authorized(
        &self,
        identity: &RemoteIdentity,
        query: &Query,
        capability_token: &str,
    ) -> Result<(Arc<RxCollection>, Value, String), RemoteError> {
        self.prepare(identity, query, capability_token).await
    }

    async fn start_snapshot(
        self: &Arc<Self>,
        identity_key: &str,
        collection: Arc<RxCollection>,
        prepared: Value,
        fingerprint: String,
        query: &Query,
    ) -> Result<Arc<SnapshotSession>, RemoteError> {
        let (sender, receiver) = mpsc::channel(2);
        let snapshot_id = random_token("snapshot")
            .map_err(|_| RemoteError::new(ErrorCode::Internal, "snapshot ID failed", true))?;
        let task_collection = collection.clone();
        let page_size = query.page_size.max(1) as usize;
        let task = tokio::task::spawn_blocking(move || {
            let result =
                task_collection
                    .storage_instance
                    .query_snapshot_stream_into_blocking(&prepared, page_size, &mut |event| {
                        match event {
                            RxStorageSnapshotEvent::Documents(documents) => {
                                let size = serde_json::to_vec(&documents)
                                    .map(|bytes| bytes.len())
                                    .unwrap_or(usize::MAX);
                                if size > CTOX_BUSINESS_DATA_MAX_SNAPSHOT_BYTES as usize {
                                    return Err(rxdb::rx_error::new_rx_error(
                                        "BUSINESS_DATA_SNAPSHOT_TOO_LARGE",
                                        None,
                                    ));
                                }
                                sender
                                    .blocking_send(SnapshotItem::Batch(documents, false))
                                    .map(|_| true)
                            }
                            RxStorageSnapshotEvent::End => sender
                                .blocking_send(SnapshotItem::Batch(Vec::new(), true))
                                .map(|_| false),
                            RxStorageSnapshotEvent::Start { .. } => Ok(true),
                        }
                    });
            if let Err(error) = result.unwrap_or_else(|| {
                Err(rxdb::rx_error::new_rx_error(
                    "BUSINESS_DATA_SNAPSHOT_UNSUPPORTED",
                    None,
                ))
            }) {
                let _ = sender.blocking_send(SnapshotItem::Failed);
                eprintln!("[business-data] snapshot failed: {error}");
            }
        });
        let snapshot = Arc::new(SnapshotSession {
            snapshot_id: snapshot_id.clone(),
            identity_key: identity_key.to_owned(),
            fingerprint,
            collection: query.collection.clone(),
            receiver: tokio::sync::Mutex::new(receiver),
            complete: AtomicBool::new(false),
            task: tokio::sync::Mutex::new(Some(task)),
        });
        self.snapshots
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry(identity_key.to_owned())
            .or_default()
            .insert(snapshot_id, snapshot.clone());
        Ok(snapshot)
    }

    fn cursor(&self, token: &str) -> Option<CursorRecord> {
        self.cursors
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(token)
            .cloned()
    }

    fn insert_cursor(
        &self,
        target: CursorTarget,
        target_id: String,
        identity_key: &str,
        fingerprint: String,
        sequence: u64,
    ) -> Result<String, RemoteError> {
        let token = random_token(if target == CursorTarget::Snapshot {
            "page"
        } else {
            "watch"
        })
        .map_err(|_| RemoteError::new(ErrorCode::Internal, "cursor generation failed", true))?;
        let mut cursors = self
            .cursors
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if cursors.len() >= 8_192 {
            cursors.retain(|_, record| record.identity_key != identity_key);
        }
        cursors.insert(
            token.clone(),
            CursorRecord {
                target,
                target_id,
                identity_key: identity_key.to_owned(),
                fingerprint,
                sequence,
            },
        );
        Ok(token)
    }

    async fn watch(
        self: &Arc<Self>,
        peer: &WebRTCRsConnection,
        identity: &RemoteIdentity,
        capability_token: &str,
        request_id: &str,
        session: &SessionRef,
        query: &Query,
        resume_cursor: Option<&str>,
        mode: WatchMode,
    ) -> Result<Response, RemoteError> {
        let (collection, prepared, fingerprint) = self
            .prepare(identity, capability_token, query, Access::Read)
            .await?;
        let identity_key = identity.key();
        let resume_sequence = match resume_cursor {
            Some(cursor) => {
                let Some(record) = self.cursor(cursor) else {
                    return self
                        .fresh_watch(
                            peer,
                            identity,
                            collection,
                            prepared,
                            fingerprint,
                            request_id,
                            query,
                            session,
                            mode,
                        )
                        .await;
                };
                if record.target != CursorTarget::Watch
                    || record.identity_key != identity_key
                    || record.fingerprint != fingerprint
                {
                    return self
                        .fresh_watch(
                            peer,
                            identity,
                            collection,
                            prepared,
                            fingerprint,
                            request_id,
                            query,
                            session,
                            mode,
                        )
                        .await;
                }
                let subscription = self
                    .subscriptions
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .get(&identity_key)
                    .and_then(|values| values.get(&record.target_id))
                    .cloned();
                let Some(subscription) = subscription else {
                    return self
                        .fresh_watch(
                            peer,
                            identity,
                            collection,
                            prepared,
                            fingerprint,
                            request_id,
                            query,
                            session,
                            mode,
                        )
                        .await;
                };
                *subscription.peer.lock().await = Some(peer.clone());
                *subscription.resume.lock().await = Some(record.sequence);
                subscription.notify.notify_waiters();
                return Ok(response_result(
                    request_id,
                    Result::Subscribed {
                        session: session.clone(),
                        subscription_id: subscription.subscription_id,
                    },
                ));
            }
            None => 0,
        };
        let _ = resume_sequence;
        self.fresh_watch(
            peer,
            identity,
            collection,
            prepared,
            fingerprint,
            request_id,
            query,
            session,
            mode,
        )
        .await
    }

    async fn fresh_watch(
        self: &Arc<Self>,
        peer: &WebRTCRsConnection,
        identity: &RemoteIdentity,
        collection: Arc<RxCollection>,
        prepared: Value,
        fingerprint: String,
        request_id: &str,
        query: &Query,
        session: &SessionRef,
        mode: WatchMode,
    ) -> Result<Response, RemoteError> {
        let identity_key = identity.key();
        {
            let subscriptions = self
                .subscriptions
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if subscriptions
                .get(&identity_key)
                .map_or(0, |values| values.len())
                >= MAX_SUBSCRIPTIONS_PER_IDENTITY
            {
                return Err(RemoteError::limit(
                    "BusinessData subscription limit exceeded",
                ));
            }
        }
        let subscription_id = random_token("subscription")
            .map_err(|_| RemoteError::new(ErrorCode::Internal, "subscription ID failed", true))?;
        let selector = prepared
            .pointer("/query/selector")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let subscription = Arc::new(Subscription {
            subscription_id: subscription_id.clone(),
            identity_key: identity_key.clone(),
            fingerprint,
            selector,
            mode,
            sequence: AtomicU64::new(1),
            history: Mutex::default(),
            history_complete: AtomicBool::new(true),
            terminal: AtomicBool::new(false),
            peer: tokio::sync::Mutex::new(Some(peer.clone())),
            resume: tokio::sync::Mutex::new(Some(0)),
            notify: Notify::new(),
            task: tokio::sync::Mutex::new(None),
            cursors: self.cursors.clone(),
            sender: self.sender.clone(),
        });
        let task = self.start_subscription_task(subscription.clone(), collection, prepared);
        *subscription.task.lock().await = Some(task);
        self.subscriptions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry(identity_key)
            .or_default()
            .insert(subscription_id.clone(), subscription);
        Ok(response_result(
            request_id,
            Result::Subscribed {
                session: session.clone(),
                subscription_id,
            },
        ))
    }

    fn start_subscription_task(
        self: &Arc<Self>,
        subscription: Arc<Subscription>,
        collection: Arc<RxCollection>,
        prepared: Value,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut changes = collection.event_bulks();
            let (sender, mut receiver) = mpsc::channel::<Result<Vec<Value>, String>>(2);
            let snapshot_collection = collection.clone();
            let snapshot_prepared = prepared.clone();
            let snapshot_task = tokio::task::spawn_blocking(move || {
                let result = snapshot_collection
                    .storage_instance
                    .query_snapshot_stream_into_blocking(&snapshot_prepared, 64, &mut |event| {
                        match event {
                            RxStorageSnapshotEvent::Documents(documents) => {
                                sender.blocking_send(Ok((documents, false))).map(|_| true)
                            }
                            RxStorageSnapshotEvent::End => {
                                sender.blocking_send(Ok((Vec::new(), true))).map(|_| false)
                            }
                            RxStorageSnapshotEvent::Start { .. } => Ok(true),
                        }
                    });
                if let Err(error) = result.unwrap_or_else(|| {
                    Err(rxdb::rx_error::new_rx_error(
                        "BUSINESS_DATA_SNAPSHOT_UNSUPPORTED",
                        None,
                    ))
                }) {
                    let _ = sender.blocking_send(Err(error.to_string()));
                }
            });
            let _ = subscription
                .emit(
                    EventPayload::SnapshotStart {
                        snapshot_id: subscription.subscription_id.clone(),
                    },
                    false,
                )
                .await;
            while let Some(item) = receiver.recv().await {
                match item {
                    Ok((documents, last_batch)) if last_batch => break,
                    Ok((documents, _)) => {
                        let records = documents.into_iter().map(document_record).collect();
                        if subscription
                            .emit(
                                EventPayload::SnapshotPage {
                                    snapshot_id: subscription.subscription_id.clone(),
                                    records,
                                },
                                false,
                            )
                            .await
                            .is_err()
                        {
                            subscription.terminal.store(true, Ordering::SeqCst);
                            let _ = snapshot_task.await;
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = subscription
                            .emit(
                                EventPayload::Error {
                                    code: ErrorCode::Disconnected,
                                    message: error,
                                    retryable: true,
                                },
                                false,
                            )
                            .await;
                        subscription.terminal.store(true, Ordering::SeqCst);
                        let _ = snapshot_task.await;
                        return;
                    }
                }
            }
            let _ = snapshot_task.await;
            let _ = subscription
                .emit(
                    EventPayload::SnapshotEnd {
                        snapshot_id: subscription.subscription_id.clone(),
                        cursor: String::new(),
                    },
                    false,
                )
                .await;
            let _ = subscription
                .emit(
                    EventPayload::CaughtUp {
                        cursor: String::new(),
                    },
                    false,
                )
                .await;
            let mut delivered_resume = false;
            loop {
                tokio::select! {
                    _ = subscription.notify.notified() => {
                        if !delivered_resume && subscription.deliver_resume().await.is_err() {
                            subscription.terminal.store(true, Ordering::SeqCst);
                            return;
                        }
                        delivered_resume = true;
                    }
                    item = changes.next() => {
                        delivered_resume = true;
                        if subscription.deliver_resume().await.is_err() {
                            subscription.terminal.store(true, Ordering::SeqCst);
                            return;
                        }
                        let Some(bulk) = item else {
                            let _ = subscription.emit(EventPayload::Error {
                                code: ErrorCode::Disconnected,
                                message: "BusinessData change stream closed".into(),
                                retryable: true,
                            }, false).await;
                            subscription.terminal.store(true, Ordering::SeqCst);
                            return;
                        };
                        let ids: HashSet<String> = bulk.events.iter().map(|event| event.document_id.clone()).collect();
                        for document_id in ids {
                            let Ok(mut documents) = collection
                                .storage_instance
                                .find_documents_by_id(&[document_id.clone()], true)
                                .await
                            else {
                                subscription.terminal.store(true, Ordering::SeqCst);
                                return;
                            };
                            if subscription.mode == WatchMode::Command {
                                if let Some(document) = documents.pop() {
                                    let state = command_state_from_document(&document);
                                    let _ = subscription.emit(EventPayload::Command { state }, false).await;
                                }
                                continue;
                            }
                            let Some(document) = documents.pop() else {
                                let _ = subscription.emit(EventPayload::Remove {
                                    cursor: String::new(),
                                    document_id,
                                    recovery: false,
                                }, false).await;
                                continue;
                            };
                            let visible = subscription.visible(&collection, &document).await.unwrap_or(false);
                            let result = if visible {
                                subscription.emit(EventPayload::Upsert {
                                    cursor: String::new(),
                                    record: document_record(document),
                                    recovery: false,
                                }, false).await
                            } else {
                                subscription.emit(EventPayload::Remove {
                                    cursor: String::new(),
                                    document_id,
                                    recovery: false,
                                }, false).await
                            };
                            if result.is_err() {
                                subscription.terminal.store(true, Ordering::SeqCst);
                                return;
                            }
                        }
                    }
                }
            }
        })
    }

    async fn unwatch(
        self: &Arc<Self>,
        identity_key: &str,
        request_id: &str,
        session: &SessionRef,
        subscription_id: &str,
    ) -> Result<Response, RemoteError> {
        let subscription = self
            .subscriptions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(identity_key)
            .and_then(|values| values.remove(subscription_id));
        let Some(subscription) = subscription else {
            return Err(RemoteError::new(
                ErrorCode::UnknownSession,
                "unknown BusinessData subscription",
                false,
            ));
        };
        subscription.terminal.store(true, Ordering::SeqCst);
        subscription.notify.notify_waiters();
        if let Some(task) = subscription.task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
        self.cursors
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|_, record| {
                record.target != CursorTarget::Watch
                    || record.target_id != subscription_id
                    || record.identity_key != identity_key
            });
        Ok(response_result(
            request_id,
            Result::Unwatched {
                session: session.clone(),
                subscription_id: subscription_id.to_owned(),
            },
        ))
    }
}

fn command_watch_query(command_id: &str) -> Query {
    Query {
        collection: "business_commands".into(),
        scope: Scope::Instance {},
        query: json!({
            "selector": { "id": { "$eq": command_id } },
            "sort": [{ "id": "asc" }]
        }),
        page_size: 1,
    }
}

fn command_state_from_document(document: &Value) -> CommandState {
    let status = match document.get("status").and_then(Value::as_str) {
        Some("completed") => NativeBusinessDataCommandStatus::Completed,
        Some("failed") => NativeBusinessDataCommandStatus::Failed,
        Some("unknown") => NativeBusinessDataCommandStatus::Unknown,
        _ => NativeBusinessDataCommandStatus::Pending,
    };
    CommandState {
        command_id: document
            .get("command_id")
            .or_else(|| document.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        status,
        result: document
            .get("result")
            .cloned()
            .filter(|value| !value.is_null()),
        error: document
            .get("last_retry_error")
            .or_else(|| document.get("error"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

fn document_record(document: Value) -> Record {
    let document_id = document
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Record {
        document_id,
        document,
    }
}

fn scope_selector(scope: &Scope, selector: Option<Value>) -> Value {
    let mut selector = selector
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    selector.insert("_deleted".into(), json!(false));
    match scope {
        Scope::Instance {} => {}
        Scope::Project { project_id } => {
            selector.insert("project_id".into(), json!(project_id));
        }
        Scope::Thread {
            project_id,
            thread_id,
        } => {
            selector.insert("project_id".into(), json!(project_id));
            selector.insert("thread_id".into(), json!(thread_id));
        }
    }
    Value::Object(selector)
}

impl Subscription {
    async fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }

    async fn visible(&self, collection: &Arc<RxCollection>, document: &Value) -> Option<bool> {
        let schema = collection.schema.as_ref()?;
        let document_id = document.get("id")?.as_str()?;
        let mut selector = self.selector.as_object().cloned().unwrap_or_default();
        selector.insert("id".into(), json!({ "$eq": document_id }));
        selector.insert("_deleted".into(), json!(false));
        let mango: MangoQuery = serde_json::from_value(json!({
            "selector": selector,
            "sort": [{ "id": "asc" }]
        }))
        .ok()?;
        let normalized = normalize_mango_query(schema, mango);
        let prepared = prepare_query(schema, normalized).ok()?;
        let result = collection.storage_instance.query(&prepared).await.ok()?;
        Some(result.documents.iter().any(|value| value == document))
    }

    async fn deliver_resume(&self) -> Result<(), String> {
        let Some(from_sequence) = self.resume.lock().await.take() else {
            return Ok(());
        };
        let replayable;
        {
            let history = self
                .history
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            replayable = self.history_complete.load(Ordering::SeqCst)
                && history
                    .back()
                    .map_or(from_sequence == 0, |last| last.sequence >= from_sequence);
        }
        if !replayable {
            self.emit(
                EventPayload::Reset {
                    code: ErrorCode::ResetRequired,
                },
                false,
            )
            .await?;
            return Err("BusinessData history is unavailable".into());
        }
        let entries: Vec<HistoryEntry> = self
            .history
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|entry| entry.sequence > from_sequence)
            .cloned()
            .collect();
        for entry in entries {
            self.send_history(entry, true).await?;
        }
        self.emit(
            EventPayload::CaughtUp {
                cursor: String::new(),
            },
            false,
        )
        .await?;
        Ok(())
    }

    async fn emit(&self, payload: EventPayload, recovery: bool) -> Result<(), String> {
        let payload = match payload {
            EventPayload::Upsert { cursor, record, .. } => EventPayload::Upsert {
                cursor,
                record,
                recovery,
            },
            EventPayload::Remove {
                cursor,
                document_id,
                ..
            } => EventPayload::Remove {
                cursor,
                document_id,
                recovery,
            },
            other => other,
        };
        let sequence = self.next_sequence().await;
        let cursor = random_token("cursor").map_err(|_| "cursor generation failed".to_string())?;
        let payload = assign_cursor(payload, cursor.clone());
        self.cursors
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                cursor.clone(),
                CursorRecord {
                    target: CursorTarget::Watch,
                    target_id: self.subscription_id.clone(),
                    identity_key: self.identity_key.clone(),
                    fingerprint: self.fingerprint.clone(),
                    sequence,
                },
            );
        {
            let mut history = self
                .history
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            history.push_back(HistoryEntry {
                sequence,
                cursor: cursor.clone(),
                payload: payload.clone(),
            });
            while history.len() > RETAINED_EVENTS {
                history.pop_front();
                self.history_complete.store(false, Ordering::SeqCst);
            }
        }
        let event = Event {
            version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
            session: SessionRef {
                handle: String::new(),
                generation: 0,
            },
            subscription_id: self.subscription_id.clone(),
            sequence,
            payload,
        };
        self.send(&event).await
    }

    async fn send_history(&self, entry: HistoryEntry, recovery: bool) -> Result<(), String> {
        let payload = match entry.payload {
            EventPayload::Upsert { cursor, record, .. } => EventPayload::Upsert {
                cursor,
                record,
                recovery,
            },
            EventPayload::Remove {
                cursor,
                document_id,
                ..
            } => EventPayload::Remove {
                cursor,
                document_id,
                recovery,
            },
            other => other,
        };
        let event = Event {
            version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
            session: SessionRef {
                handle: String::new(),
                generation: 0,
            },
            subscription_id: self.subscription_id.clone(),
            sequence: entry.sequence,
            payload,
        };
        self.send(&event).await
    }

    async fn send(&self, event: &Event) -> Result<(), String> {
        let peer = self.peer.lock().await.clone();
        let Some(peer) = peer else { return Ok(()) };
        let current = self.sender.handler.is_peer_current(&peer);
        if !current {
            *self.peer.lock().await = None;
            return Ok(());
        }
        self.sender.send(&peer, event).await
    }
}

fn assign_cursor(payload: EventPayload, cursor: String) -> EventPayload {
    match payload {
        EventPayload::SnapshotEnd { snapshot_id, .. } => EventPayload::SnapshotEnd {
            snapshot_id,
            cursor,
        },
        EventPayload::Upsert {
            record, recovery, ..
        } => EventPayload::Upsert {
            cursor,
            record,
            recovery,
        },
        EventPayload::Remove {
            document_id,
            recovery,
            ..
        } => EventPayload::Remove {
            cursor,
            document_id,
            recovery,
        },
        EventPayload::CaughtUp { .. } => EventPayload::CaughtUp { cursor },
        other => other,
    }
}

/// Issue one source request over the existing admitted WebRTC DataChannel.
pub async fn remote_request(
    pool: NativePool,
    peer: WebRTCRsConnection,
    request: &Request,
) -> io::Result<Response> {
    if !pool.is_peer_ready_for_control(&peer) {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "BusinessData peer is not ready",
        ));
    }
    let exchange = async {
        let params = vec![serde_json::to_value(request)
            .map_err(|_| io::Error::other("BusinessData request encoding failed"))?];
        send_message_and_await_answer(
            pool.connection_handler.clone(),
            peer,
            WebRTCMessage {
                id: format!("business-data-{}", random_token("rpc").unwrap_or_default()),
                method: BUSINESS_DATA_RPC_METHOD.into(),
                params,
                collection: None,
            },
        )
        .await
    };
    let response = tokio::time::timeout(REMOTE_TIMEOUT, exchange)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "BusinessData request timed out"))?
        .map_err(|_| io::Error::other("BusinessData source rejected the request"))?;
    if response.error.is_some()
        || serde_json::to_vec(&response.result)
            .map(|bytes| bytes.len() > CTOX_BUSINESS_DATA_MAX_SNAPSHOT_BYTES as usize)
            .unwrap_or(true)
    {
        return Err(io::Error::other("BusinessData source response was invalid"));
    }
    let result: Result = serde_json::from_value(response.result)
        .map_err(|_| io::Error::other("BusinessData response could not be decoded"))?;
    Ok(Response {
        version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
        request_id: request.request_id.clone(),
        result,
    })
}

/// Owned bounded pump. Starting it before Watch/Observe guarantees the private
/// consumer sees the response before any event; Release forwards buffered data.
pub struct OwnedEventPump {
    task: tokio::task::JoinHandle<()>,
    release: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl OwnedEventPump {
    pub fn release(&self) {
        self.release.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub async fn shutdown(self) {
        self.task.abort();
        let _ = self.task.await;
    }
}

pub fn spawn_remote_event_pump(
    pool: NativePool,
    peer: WebRTCRsConnection,
    session: SessionRef,
    events: mpsc::Sender<Event>,
) -> OwnedEventPump {
    let release = Arc::new(AtomicBool::new(false));
    let notify = Arc::new(Notify::new());
    let task = {
        let release = release.clone();
        let notify = notify.clone();
        tokio::spawn(async move {
            let mut queue = VecDeque::new();
            let mut messages = pool.connection_handler.message_stream();
            loop {
                let drain = async {
                    while let Some(event) = queue.pop_front() {
                        if events.send(event).await.is_err() {
                            return;
                        }
                    }
                    std::future::pending::<()>().await;
                };
                tokio::select! {
                    _ = notify.notified(), if !release.load(Ordering::SeqCst) => {
                        while let Some(event) = queue.pop_front() {
                            if events.send(event).await.is_err() { return; }
                        }
                    }
                    _ = drain => {}
                    item = messages.next() => {
                        let Some(item) = item else { return; };
                        if item.message.method != BUSINESS_DATA_EVENT_METHOD { continue; }
                        let Some(value) = item.message.params.into_iter().next() else { continue; };
                        let Ok(mut event) = serde_json::from_value::<Event>(value) else { continue; };
                        if event.version != CTOX_BUSINESS_DATA_PROTOCOL_VERSION
                            || event.subscription_id.is_empty()
                            || event.sequence == 0
                        {
                            continue;
                        }
                        event.session = session.clone();
                        if release.load(Ordering::SeqCst) {
                            if events.send(event).await.is_err() { return; }
                        } else if queue.len() >= CLIENT_EVENT_BUFFER {
                            queue.clear();
                            let _ = events.send(Event {
                                version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
                                session: session.clone(),
                                subscription_id: String::new(),
                                sequence: 0,
                                payload: EventPayload::Reset { code: ErrorCode::ResetRequired },
                            }).await;
                            return;
                        } else {
                            queue.push_back(event);
                        }
                    }
                }
            }
        })
    };
    OwnedEventPump {
        task,
        release,
        notify,
    }
}
