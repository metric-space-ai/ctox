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
    NativeBusinessDataResult as WireResult, NativeBusinessDataScope as Scope,
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
    async fn authorize_query(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        collection: &str,
        scope: &Scope,
        _query: &Value,
    ) -> io::Result<()> {
        self.authorize(identity, capability_token, collection, Access::Read, scope)
            .await
    }
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
    async fn document_view(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        collection: &str,
        document: &Value,
    ) -> io::Result<Option<Value>>;
    async fn command_event(
        &self,
        identity: &RemoteIdentity,
        capability_token: &str,
        expected_command_id: &str,
        document: &Value,
    ) -> io::Result<Option<CommandState>>;
}

#[derive(Clone)]
struct AuthorizedContext {
    identity: RemoteIdentity,
    capability_token: String,
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
            result: WireResult::Rejected {
                code: self.code,
                message: self.message,
                retryable: self.retryable,
            },
        }
    }
}

fn response_result(request_id: &str, result: WireResult) -> Response {
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
    session: Mutex<SessionRef>,
    identity_key: String,
    fingerprint: String,
    selector: Value,
    query: Value,
    scope: Scope,
    collection: String,
    mode: WatchMode,
    command_id: Option<String>,
    visible_ids: Mutex<HashSet<String>>,
    authority: AuthorizedContext,
    policy: Arc<dyn BusinessDataAccessPolicy>,
    sequence: AtomicU64,
    history: Mutex<VecDeque<HistoryEntry>>,
    history_complete: AtomicBool,
    terminal: AtomicBool,
    peer: tokio::sync::Mutex<Option<WebRTCRsConnection>>,
    resume: tokio::sync::Mutex<Option<u64>>,
    lifecycle: tokio::sync::Mutex<()>,
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
                if let Err(error) = task.await {
                    if !error.is_cancelled() {
                        failure.get_or_insert_with(|| io::Error::other(error));
                    }
                }
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
                if let Err(error) = task.await {
                    if !error.is_cancelled() {
                        failure.get_or_insert_with(|| io::Error::other(error));
                    }
                }
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
        failure.map_or(Ok(()), Err)
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
                    None,
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
                if state.command_id != command.command_id {
                    return Err(RemoteError::unauthorized());
                }
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
                if self
                    .policy
                    .identity(capability_token)
                    .await
                    .as_ref()
                    .map(RemoteIdentity::key)
                    .as_deref()
                    != Some(identity_key.as_str())
                {
                    return Err(RemoteError::unauthorized());
                }
                Ok(response_result(
                    request.request_id.as_str(),
                    WireResult::Command {
                        session: session.clone(),
                        state,
                    },
                ))
            }
            Operation::ObserveCommand {
                session,
                command_id,
            } => {
                // Authorize and resolve the owned command before installing any
                // subscription. A rejected observation therefore leaves no
                // live task behind.
                self.policy
                    .authorize(
                        identity,
                        capability_token,
                        "business_commands",
                        Access::Read,
                        &Scope::Instance {},
                    )
                    .await
                    .map_err(|_| RemoteError::unauthorized())?;
                let state = self
                    .policy
                    .command_state(identity, &json!({ "id": command_id }))
                    .await
                    .map_err(|error| {
                        RemoteError::new(ErrorCode::Unauthorized, error.to_string(), false)
                    })?;
                if state.command_id != *command_id {
                    return Err(RemoteError::unauthorized());
                }
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
                        Some(command_id),
                    )
                    .await?;
                let WireResult::Subscribed { session, .. } = response.result else {
                    return Err(RemoteError::new(
                        ErrorCode::Internal,
                        "command observation failed",
                        false,
                    ));
                };
                Ok(response_result(
                    request.request_id.as_str(),
                    WireResult::Command { session, state },
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
        self.policy
            .authorize_query(
                identity,
                capability_token,
                &query.collection,
                &query.scope,
                &query.query,
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
        let identity_key = identity_key.as_str();
        let snapshot = match cursor {
            Some(cursor) => {
                let record = self.cursor(cursor).ok_or_else(RemoteError::reset)?;
                if record.target != CursorTarget::Snapshot
                    || record.identity_key != identity_key
                    || record.fingerprint != query_fingerprint(query)
                {
                    return Err(RemoteError::reset());
                }
                let snapshot = self
                    .snapshots
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .get(identity_key)
                    .and_then(|values| values.get(&record.target_id))
                    .cloned()
                    .ok_or_else(RemoteError::reset)?;
                self.policy
                    .authorize_query(
                        identity,
                        capability_token,
                        &query.collection,
                        &query.scope,
                        &query.query,
                    )
                    .await
                    .map_err(|error| {
                        RemoteError::new(ErrorCode::Unauthorized, error.to_string(), false)
                    })?;
                snapshot
            }
            None => {
                let (collection, prepared, fingerprint) = self
                    .prepare_for_snapshot(identity, query, capability_token)
                    .await?;
                self.start_snapshot(identity_key, collection, prepared, fingerprint, query)
                    .await?
            }
        };
        let mut receiver = snapshot.receiver.lock().await;
        // A page cursor names one continuation, not permission to advance the
        // snapshot repeatedly. Serialize consumption with receiving the page.
        if let Some(cursor) = cursor {
            if self
                .cursors
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(cursor)
                .is_none()
            {
                return Err(RemoteError::reset());
            }
        }
        let (documents, last_batch) = match receiver.recv().await {
            Some(SnapshotItem::Batch(documents, last_batch)) => (documents, last_batch),
            Some(SnapshotItem::Failed) | None => {
                drop(receiver);
                self.fail_snapshot(identity_key, &snapshot).await;
                return Err(RemoteError::reset());
            }
        };
        let mut records = Vec::with_capacity(documents.len());
        for document in documents {
            match self
                .policy
                .document_view(identity, capability_token, &snapshot.collection, &document)
                .await
            {
                Ok(Some(document)) => records.push(document_record(document)),
                Ok(None) => {}
                Err(_) => {
                    drop(receiver);
                    self.fail_snapshot(identity_key, &snapshot).await;
                    return Err(RemoteError::unauthorized());
                }
            }
        }
        let has_more = !last_batch;
        if last_batch {
            snapshot.complete.store(true, Ordering::SeqCst);
        }
        drop(receiver);
        let next_cursor = if has_more {
            match self.insert_cursor(
                CursorTarget::Snapshot,
                snapshot.snapshot_id.clone(),
                identity_key,
                snapshot.fingerprint.clone(),
                0,
            ) {
                Ok(cursor) => Some(cursor),
                Err(error) => {
                    self.fail_snapshot(identity_key, &snapshot).await;
                    return Err(error);
                }
            }
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

        // Storage, projection policy and producer completion may all await.
        // Recheck source authority after them, including empty result pages.
        let authorized = self
            .policy
            .authorize_query(
                identity,
                capability_token,
                &query.collection,
                &query.scope,
                &query.query,
            )
            .await
            .is_ok();
        let current_identity = self.policy.identity(capability_token).await;
        if !authorized
            || current_identity
                .as_ref()
                .map(RemoteIdentity::key)
                .as_deref()
                != Some(identity_key)
        {
            if let Some(cursor) = next_cursor.as_ref() {
                self.cursors
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(cursor);
            }
            self.fail_snapshot(identity_key, &snapshot).await;
            return Err(RemoteError::unauthorized());
        }

        Ok(response_result(
            request_id,
            WireResult::Page {
                session: session.clone(),
                snapshot_id: snapshot.snapshot_id.clone(),
                records,
                next_page_cursor: next_cursor,
                snapshot_complete: !has_more,
            },
        ))
    }

    async fn fail_snapshot(&self, identity_key: &str, snapshot: &Arc<SnapshotSession>) {
        snapshot.receiver.lock().await.close();
        if let Some(task) = snapshot.task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
        self.cursors
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|_, record| {
                record.target != CursorTarget::Snapshot
                    || record.target_id != snapshot.snapshot_id
                    || record.identity_key != identity_key
            });
        self.snapshots
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(identity_key)
            .and_then(|values| values.remove(&snapshot.snapshot_id));
    }

    async fn prepare_for_snapshot(
        &self,
        identity: &RemoteIdentity,
        query: &Query,
        capability_token: &str,
    ) -> Result<(Arc<RxCollection>, Value, String), RemoteError> {
        let identity_key = identity.key();
        {
            let snapshots = self
                .snapshots
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if snapshots
                .get(&identity_key)
                .map_or(0, |values| values.len())
                >= MAX_SNAPSHOTS_PER_IDENTITY
            {
                return Err(RemoteError::limit("BusinessData snapshot limit exceeded"));
            }
        }
        let (collection, prepared, fingerprint) = self
            .prepare_authorized(identity, query, capability_token)
            .await?;
        Ok((collection, prepared, fingerprint))
    }

    async fn prepare_authorized(
        &self,
        identity: &RemoteIdentity,
        query: &Query,
        capability_token: &str,
    ) -> Result<(Arc<RxCollection>, Value, String), RemoteError> {
        self.prepare(identity, capability_token, query, Access::Read)
            .await
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
            let mut pending = None;
            let mut ended = false;
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
                                if let Some(previous) = pending.replace(documents) {
                                    sender
                                        .blocking_send(SnapshotItem::Batch(previous, false))
                                        .map_err(|_| {
                                            rxdb::rx_error::new_rx_error(
                                                "BUSINESS_DATA_SNAPSHOT_CLOSED",
                                                None,
                                            )
                                        })?;
                                }
                                Ok(true)
                            }
                            RxStorageSnapshotEvent::End => {
                                ended = true;
                                Ok(false)
                            }
                            RxStorageSnapshotEvent::Start { .. } => Ok(true),
                        }
                    });
            // Keep one bounded page until storage confirms successful completion.
            // A missing End, error or panic can never manufacture a final page.
            if matches!(result, Some(Ok(()))) && ended {
                let _ =
                    sender.blocking_send(SnapshotItem::Batch(pending.unwrap_or_default(), true));
            } else {
                let _ = sender.blocking_send(SnapshotItem::Failed);
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
        if cursors.len() >= 8_192 {
            return Err(RemoteError::new(
                ErrorCode::LimitExceeded,
                "BusinessData cursor capacity exhausted",
                true,
            ));
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
        command_id: Option<&str>,
    ) -> Result<Response, RemoteError> {
        let (collection, prepared, fingerprint) = self
            .prepare(identity, capability_token, query, Access::Read)
            .await?;
        let identity_key = identity.key();
        if let Some(cursor) = resume_cursor {
            let record = self.cursor(cursor).ok_or_else(RemoteError::reset)?;
            if record.target != CursorTarget::Watch
                || record.identity_key != identity_key
                || record.fingerprint != fingerprint
            {
                return Err(RemoteError::reset());
            }
            let subscription = self
                .subscriptions
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get(&identity_key)
                .and_then(|values| values.get(&record.target_id))
                .cloned();
            let Some(subscription) = subscription else {
                return Err(RemoteError::reset());
            };
            // Replacement must wait for an in-flight event to complete its
            // authority-checked publication. Otherwise state resolved for the
            // old peer/session could be sent through the new one.
            let _lifecycle_guard = subscription.lifecycle.lock().await;
            if subscription.terminal.load(Ordering::SeqCst)
                || subscription.mode != mode
                || subscription.command_id.as_deref() != command_id
            {
                return Err(RemoteError::reset());
            }
            subscription
                .ensure_authorized()
                .await
                .map_err(|error| RemoteError::new(ErrorCode::ResetRequired, error, false))?;
            if !self.sender.handler.is_peer_current(peer) {
                return Err(RemoteError::reset());
            }
            let mut bound_peer = subscription.peer.lock().await;
            *subscription
                .session
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = session.clone();
            *bound_peer = Some(peer.clone());
            drop(bound_peer);
            *subscription.resume.lock().await = Some(record.sequence);
            subscription.notify.notify_one();
            return Ok(response_result(
                request_id,
                WireResult::Subscribed {
                    session: session.clone(),
                    subscription_id: subscription.subscription_id.clone(),
                },
            ));
        }
        self.fresh_watch(
            peer,
            identity,
            capability_token,
            collection,
            prepared,
            fingerprint,
            request_id,
            query,
            session,
            mode,
            command_id,
        )
        .await
    }

    async fn fresh_watch(
        self: &Arc<Self>,
        peer: &WebRTCRsConnection,
        identity: &RemoteIdentity,
        capability_token: &str,
        collection: Arc<RxCollection>,
        prepared: Value,
        fingerprint: String,
        request_id: &str,
        query: &Query,
        session: &SessionRef,
        mode: WatchMode,
        command_id: Option<&str>,
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
        let subscription_id = match command_id {
            Some(command_id) => format!("command:{command_id}"),
            None => random_token("subscription").map_err(|_| {
                RemoteError::new(ErrorCode::Internal, "subscription ID failed", true)
            })?,
        };
        let selector = prepared
            .pointer("/query/selector")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let authority = AuthorizedContext {
            identity: identity.clone(),
            capability_token: capability_token.to_owned(),
        };
        let subscription = Arc::new(Subscription {
            subscription_id: subscription_id.clone(),
            session: Mutex::new(session.clone()),
            identity_key: identity_key.clone(),
            fingerprint,
            selector,
            scope: query.scope.clone(),
            query: query.query.clone(),
            collection: query.collection.clone(),
            mode,
            command_id: command_id.map(str::to_owned),
            visible_ids: Mutex::new(HashSet::new()),
            authority,
            policy: Arc::clone(&self.policy),
            sequence: AtomicU64::new(1),
            history: Mutex::default(),
            history_complete: AtomicBool::new(true),
            terminal: AtomicBool::new(false),
            peer: tokio::sync::Mutex::new(Some(peer.clone())),
            resume: tokio::sync::Mutex::new(None),
            lifecycle: tokio::sync::Mutex::new(()),
            notify: Notify::new(),
            task: tokio::sync::Mutex::new(None),
            cursors: self.cursors.clone(),
            sender: self.sender.clone(),
        });

        {
            let mut subscriptions = self
                .subscriptions
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if subscriptions
                .get(&identity_key)
                .is_some_and(|values| values.contains_key(&subscription_id))
            {
                return Err(RemoteError::invalid(
                    "BusinessData command subscription already exists",
                ));
            }
            let task = self.start_subscription_task(subscription.clone(), collection, prepared);
            *subscription
                .task
                .try_lock()
                .expect("new subscription task slot") = Some(task);
            subscriptions
                .entry(identity_key)
                .or_default()
                .insert(subscription_id.clone(), subscription);
        }
        Ok(response_result(
            request_id,
            WireResult::Subscribed {
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
            let (sender, mut receiver) = mpsc::channel::<Result<(Vec<Value>, bool), String>>(2);
            let snapshot_collection = collection.clone();
            let snapshot_prepared = prepared.clone();
            let snapshot_task = tokio::task::spawn_blocking(move || {
                let mut ended = false;
                let result = snapshot_collection
                    .storage_instance
                    .query_snapshot_stream_into_blocking(&snapshot_prepared, 64, &mut |event| {
                        match event {
                            RxStorageSnapshotEvent::Documents(documents) => sender
                                .blocking_send(Ok((documents, false)))
                                .map(|_| true)
                                .map_err(|_| {
                                    rxdb::rx_error::new_rx_error(
                                        "BUSINESS_DATA_SNAPSHOT_CLOSED",
                                        None,
                                    )
                                }),
                            RxStorageSnapshotEvent::End => {
                                ended = true;
                                Ok(false)
                            }
                            RxStorageSnapshotEvent::Start { .. } => Ok(true),
                        }
                    });
                match result {
                    Some(Ok(())) if ended => {
                        let _ = sender.blocking_send(Ok((Vec::new(), true)));
                    }
                    Some(Err(_)) => {
                        let _ = sender.blocking_send(Err("BusinessData snapshot failed".into()));
                    }
                    _ => {
                        let _ = sender.blocking_send(Err(
                            "BusinessData snapshot ended before completion".into(),
                        ));
                    }
                }
            });
            if subscription
                .emit(
                    EventPayload::SnapshotStart {
                        snapshot_id: subscription.subscription_id.clone(),
                    },
                    false,
                )
                .await
                .is_err()
            {
                subscription.terminal.store(true, Ordering::SeqCst);
                receiver.close();
                let _ = snapshot_task.await;
                return;
            }

            let mut failed_snapshot = false;
            let mut snapshot_complete = false;
            while let Some(item) = receiver.recv().await {
                match item {
                    Ok((_documents, true)) => {
                        snapshot_complete = true;
                        break;
                    }
                    Ok((documents, false)) => {
                        if subscription.mode == WatchMode::Command {
                            for document in documents {
                                match subscription.command_event(&document).await {
                                    Ok(Some(state)) => {
                                        if subscription
                                            .emit(EventPayload::Command { state }, false)
                                            .await
                                            .is_err()
                                        {
                                            subscription.terminal.store(true, Ordering::SeqCst);
                                            receiver.close();
                                            let _ = snapshot_task.await;
                                            return;
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(error) => {
                                        failed_snapshot = true;
                                        let _ = subscription
                                            .emit(
                                                EventPayload::Error {
                                                    code: ErrorCode::Unauthorized,
                                                    message: error.to_string(),
                                                    retryable: false,
                                                },
                                                false,
                                            )
                                            .await;
                                        break;
                                    }
                                }
                            }
                            if failed_snapshot {
                                break;
                            }
                        } else {
                            let mut records = Vec::with_capacity(documents.len());
                            let mut failed = false;
                            for document in documents {
                                match subscription.view_document(&document).await {
                                    Ok(Some(document)) => records.push(document_record(document)),
                                    Ok(None) => {}
                                    Err(error) => {
                                        failed_snapshot = true;
                                        failed = true;
                                        let _ = subscription
                                            .emit(
                                                EventPayload::Error {
                                                    code: ErrorCode::ResetRequired,
                                                    message: error,
                                                    retryable: true,
                                                },
                                                false,
                                            )
                                            .await;
                                        break;
                                    }
                                }
                            }
                            if failed {
                                break;
                            }
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
                                receiver.close();
                                let _ = snapshot_task.await;
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        failed_snapshot = true;
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
                        break;
                    }
                }
            }
            receiver.close();
            if snapshot_task.await.is_err() {
                failed_snapshot = true;
            }
            if failed_snapshot || !snapshot_complete {
                let _ = subscription
                    .emit(
                        EventPayload::Error {
                            code: ErrorCode::ResetRequired,
                            message: "BusinessData snapshot ended before completion".into(),
                            retryable: true,
                        },
                        false,
                    )
                    .await;
                subscription.terminal.store(true, Ordering::SeqCst);
                return;
            }

            // The live stream subscribed before storage snapshot creation. Drain
            // changes buffered by snapshot end before declaring caught up; writes
            // arriving after the drain remain normal post-CaughtUp events.
            loop {
                let bulk = match futures_util::FutureExt::now_or_never(changes.next()) {
                    None => break,
                    Some(Some(bulk)) if !bulk.is_rxsubject_lagged() => bulk,
                    Some(_) => {
                        // A closed stream or lag marker cannot prove that all
                        // changes made during snapshot creation were drained.
                        let _ = subscription
                            .emit(
                                EventPayload::Reset {
                                    code: ErrorCode::ResetRequired,
                                },
                                false,
                            )
                            .await;
                        subscription.terminal.store(true, Ordering::SeqCst);
                        return;
                    }
                };
                let ids: HashSet<String> = bulk
                    .events
                    .iter()
                    .map(|event| event.document_id.clone())
                    .collect();
                for document_id in ids {
                    if subscription
                        .process_change(&collection, document_id)
                        .await
                        .is_err()
                    {
                        let _ = subscription
                            .emit(
                                EventPayload::Reset {
                                    code: ErrorCode::ResetRequired,
                                },
                                false,
                            )
                            .await;
                        subscription.terminal.store(true, Ordering::SeqCst);
                        return;
                    }
                }
            }

            if subscription
                .emit(
                    EventPayload::SnapshotEnd {
                        snapshot_id: subscription.subscription_id.clone(),
                        cursor: String::new(),
                    },
                    false,
                )
                .await
                .is_err()
                || subscription
                    .emit(
                        EventPayload::CaughtUp {
                            cursor: String::new(),
                        },
                        false,
                    )
                    .await
                    .is_err()
            {
                subscription.terminal.store(true, Ordering::SeqCst);
                return;
            }

            loop {
                tokio::select! {
                    _ = subscription.notify.notified() => {
                        if subscription.deliver_resume().await.is_err() {
                            subscription.terminal.store(true, Ordering::SeqCst);
                            return;
                        }
                    }
                    item = changes.next() => {
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
                        if bulk.is_rxsubject_lagged() {
                            let _ = subscription.emit(
                                EventPayload::Reset { code: ErrorCode::ResetRequired },
                                false,
                            ).await;
                            subscription.terminal.store(true, Ordering::SeqCst);
                            return;
                        }
                        let ids: HashSet<String> = bulk.events.iter()
                            .map(|event| event.document_id.clone())
                            .collect();
                        for document_id in ids {
                            if subscription.process_change(&collection, document_id).await.is_err() {
                                let _ = subscription.emit(
                                    EventPayload::Reset { code: ErrorCode::ResetRequired }, false,
                                ).await;
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
        subscription.notify.notify_one();
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
            WireResult::Unwatched {
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
    async fn ensure_authorized(&self) -> Result<(), String> {
        self.policy
            .authorize_query(
                &self.authority.identity,
                &self.authority.capability_token,
                &self.collection,
                &self.scope,
                &self.query,
            )
            .await
            .map_err(|error| error.to_string())?;
        let current = self.policy.identity(&self.authority.capability_token).await;
        if current.as_ref().map(RemoteIdentity::key).as_deref() != Some(self.identity_key.as_str())
        {
            return Err("BusinessData subscription identity changed".into());
        }
        Ok(())
    }

    async fn view_document(&self, document: &Value) -> Result<Option<Value>, String> {
        self.policy
            .document_view(
                &self.authority.identity,
                &self.authority.capability_token,
                &self.collection,
                document,
            )
            .await
            .map_err(|error| error.to_string())
    }

    async fn command_event(&self, document: &Value) -> io::Result<Option<CommandState>> {
        let Some(expected) = self.command_id.as_deref() else {
            return Ok(None);
        };
        let actual = document
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if actual != expected {
            return Ok(None);
        }
        let state = self
            .policy
            .command_event(
                &self.authority.identity,
                &self.authority.capability_token,
                expected,
                document,
            )
            .await?;
        if state
            .as_ref()
            .is_some_and(|state| state.command_id != expected)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "command event identity changed",
            ));
        }
        Ok(state)
    }

    async fn process_change(
        &self,
        collection: &Arc<RxCollection>,
        document_id: String,
    ) -> Result<(), String> {
        // Hold authority publication and peer/session replacement as one
        // critical section so an event resolved before replacement cannot be
        // emitted through a later identity envelope.
        let _lifecycle_guard = self.lifecycle.lock().await;
        let documents = collection
            .storage_instance
            .find_documents_by_id(&[document_id.clone()], true)
            .await
            .map_err(|error| error.to_string())?;
        if self.mode == WatchMode::Command {
            if let Some(document) = documents.into_iter().next() {
                let state = self
                    .command_event(&document)
                    .await
                    .map_err(|error| error.to_string())?;
                if let Some(state) = state {
                    self.emit(EventPayload::Command { state }, false).await?;
                }
            }
            return Ok(());
        }
        let Some(document) = documents.into_iter().next() else {
            return self
                .emit(
                    EventPayload::Remove {
                        cursor: String::new(),
                        document_id,
                        recovery: false,
                    },
                    false,
                )
                .await;
        };
        let view = match self.visible(collection, &document).await {
            Some(true) => self.view_document(&document).await?,
            Some(false) => None,
            None => return Err("BusinessData live query evaluation failed".into()),
        };
        match view {
            Some(document) => {
                self.emit(
                    EventPayload::Upsert {
                        cursor: String::new(),
                        record: document_record(document),
                        recovery: false,
                    },
                    false,
                )
                .await
            }
            None => {
                self.emit(
                    EventPayload::Remove {
                        cursor: String::new(),
                        document_id,
                        recovery: false,
                    },
                    false,
                )
                .await
            }
        }
    }

    async fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }

    async fn visible(&self, collection: &Arc<RxCollection>, document: &Value) -> Option<bool> {
        let schema = collection.schema.as_ref()?;
        let document_id = document.get("id")?.as_str()?;
        let mango: MangoQuery = serde_json::from_value(json!({
            "selector": {"$and": [self.selector.clone(), {
                "id": {"$eq": document_id}, "_deleted": false
            }]},
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
            if let Err(error) = self.send_history(entry, true).await {
                let _ = self
                    .emit(
                        EventPayload::Reset {
                            code: ErrorCode::ResetRequired,
                        },
                        false,
                    )
                    .await;
                return Err(error);
            }
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

    async fn send_control(&self, payload: EventPayload) -> Result<(), String> {
        let sequence = self.next_sequence().await;
        let event = Event {
            version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
            session: self
                .session
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone(),
            subscription_id: self.subscription_id.clone(),
            sequence,
            payload,
        };
        self.send(&event).await
    }

    async fn emit(&self, payload: EventPayload, recovery: bool) -> Result<(), String> {
        if matches!(
            &payload,
            EventPayload::Error { .. } | EventPayload::Reset { .. }
        ) {
            // Recovery signals carry no resumable cursor and must remain
            // deliverable even when the cursor budget is exhausted.
            return self.send_control(payload).await;
        }
        if !matches!(
            payload,
            EventPayload::Error { .. } | EventPayload::Reset { .. }
        ) {
            self.ensure_authorized().await?;
        }
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
        {
            let mut visible_ids = self
                .visible_ids
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            match &payload {
                EventPayload::SnapshotPage { records, .. } => {
                    visible_ids.extend(records.iter().map(|record| record.document_id.clone()));
                }
                EventPayload::Upsert { record, .. } => {
                    visible_ids.insert(record.document_id.clone());
                }
                EventPayload::Remove { document_id, .. } => {
                    if !visible_ids.remove(document_id) {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        let sequence = self.next_sequence().await;
        let cursor = random_token("cursor").map_err(|_| "cursor generation failed".to_string())?;
        let payload = assign_cursor(payload, cursor.clone());
        let inserted = {
            let mut cursors = self
                .cursors
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if cursors.len() >= 8_192 {
                false
            } else {
                cursors.insert(
                    cursor.clone(),
                    CursorRecord {
                        target: CursorTarget::Watch,
                        target_id: self.subscription_id.clone(),
                        identity_key: self.identity_key.clone(),
                        fingerprint: self.fingerprint.clone(),
                        sequence,
                    },
                );
                true
            }
        };
        if !inserted {
            self.terminal.store(true, Ordering::SeqCst);
            let _ = self
                .send_control(EventPayload::Reset {
                    code: ErrorCode::ResetRequired,
                })
                .await;
            return Err("BusinessData cursor capacity exhausted".into());
        }
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
                if let Some(expired) = history.pop_front() {
                    self.cursors
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .remove(&expired.cursor);
                }
                self.history_complete.store(false, Ordering::SeqCst);
            }
        }
        let event = Event {
            version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
            session: self
                .session
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone(),
            subscription_id: self.subscription_id.clone(),
            sequence,
            payload,
        };
        self.send(&event).await
    }

    async fn send_history(&self, entry: HistoryEntry, recovery: bool) -> Result<(), String> {
        self.ensure_authorized().await?;
        let payload = match entry.payload {
            // Only this recovery attempt may declare itself caught up, after
            // every replayed item has passed current authority checks.
            EventPayload::CaughtUp { .. } => return Ok(()),
            EventPayload::Upsert { cursor, record, .. } => {
                let Some(document) = self.view_document(&record.document).await? else {
                    return Err("BusinessData replay visibility changed".into());
                };
                EventPayload::Upsert {
                    cursor,
                    record: document_record(document),
                    recovery: true,
                }
            }
            EventPayload::SnapshotPage {
                snapshot_id,
                records,
            } => {
                let mut visible = Vec::with_capacity(records.len());
                for record in records {
                    if let Some(document) = self.view_document(&record.document).await? {
                        visible.push(document_record(document));
                    } else {
                        return Err("BusinessData snapshot replay visibility changed".into());
                    }
                }
                EventPayload::SnapshotPage {
                    snapshot_id,
                    records: visible,
                }
            }
            EventPayload::Command { .. } => {
                let Some(command_id) = self.command_id.as_deref() else {
                    return Ok(());
                };
                match self
                    .command_event(&json!({ "id": command_id }))
                    .await
                    .map_err(|error| error.to_string())?
                {
                    Some(state) => EventPayload::Command { state },
                    None => return Err("BusinessData command replay authority changed".into()),
                }
            }
            other => other,
        };
        let event = Event {
            version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
            session: self
                .session
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone(),
            subscription_id: self.subscription_id.clone(),
            sequence: entry.sequence,
            payload,
        };
        self.send(&event).await
    }

    async fn send(&self, event: &Event) -> Result<(), String> {
        // Resume changes the peer and session under this same lock. Holding it
        // through publication prevents an old event crossing that transition.
        let mut bound_peer = self.peer.lock().await;
        let Some(peer) = bound_peer.as_ref() else {
            return Err("BusinessData subscription peer retired".into());
        };
        if !matches!(
            &event.payload,
            EventPayload::Error { .. } | EventPayload::Reset { .. }
        ) {
            self.ensure_authorized().await?;
            if self.terminal.load(Ordering::SeqCst) {
                return Err("BusinessData subscription is terminal".into());
            }
        }
        if event.session
            != *self
                .session
                .lock()
                .unwrap_or_else(|error| error.into_inner())
        {
            return Err("BusinessData subscription session changed".into());
        }
        if !self.sender.handler.is_peer_current(&peer) {
            *bound_peer = None;
            return Err("BusinessData subscription peer retired".into());
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
    let result: WireResult = serde_json::from_value(response.result)
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
    alive: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
    release: Arc<AtomicBool>,
    notify: Arc<Notify>,
    accepted_subscription: Arc<Mutex<Option<String>>>,
}

impl OwnedEventPump {
    pub fn invalidate(&self) {
        self.alive.store(false, Ordering::SeqCst);
    }

    pub fn accept(&self, subscription_id: &str) {
        if let Ok(mut accepted) = self.accepted_subscription.lock() {
            *accepted = Some(subscription_id.to_owned());
        }
    }

    pub fn release(&self) {
        self.release.store(true, Ordering::SeqCst);
        self.notify.notify_one();
    }

    pub async fn shutdown(mut self) {
        self.alive.store(false, Ordering::SeqCst);
        self.task.abort();
        let _ = (&mut self.task).await;
    }
}

impl Drop for OwnedEventPump {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
        self.task.abort();
    }
}

#[cfg(test)]
mod event_publication_tests {
    use super::*;

    fn event(sequence: u64) -> Event {
        Event {
            version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
            session: SessionRef {
                handle: "session".into(),
                generation: 7,
            },
            subscription_id: "subscription".into(),
            sequence,
            payload: EventPayload::Reset {
                code: ErrorCode::ResetRequired,
            },
        }
    }

    #[tokio::test]
    async fn backpressure_revalidates_authority_before_publication() {
        let (sender, mut receiver) = mpsc::channel(1);
        let alive = Arc::new(AtomicBool::new(true));
        let failed = Arc::new(AtomicBool::new(false));
        let initial_authority: EventAuthorityCheck = Arc::new(|| Box::pin(async { true }));
        assert!(
            publish_authorized_event(&sender, &initial_authority, &alive, &failed, event(1)).await
        );
        let allowed = Arc::new(AtomicBool::new(true));
        let checked = Arc::new(AtomicBool::new(false));
        let authority: EventAuthorityCheck = {
            let allowed = allowed.clone();
            let checked = checked.clone();
            Arc::new(move || {
                let allowed = allowed.clone();
                let checked = checked.clone();
                Box::pin(async move {
                    checked.store(true, Ordering::SeqCst);
                    allowed.load(Ordering::SeqCst)
                })
            })
        };
        let mut completion = event(2);
        completion.payload = EventPayload::CaughtUp {
            cursor: "cursor".into(),
        };
        let publication =
            publish_authorized_event(&sender, &authority, &alive, &failed, completion);
        tokio::pin!(publication);
        assert!(futures_util::FutureExt::now_or_never(publication.as_mut()).is_none());
        assert!(!checked.load(Ordering::SeqCst));
        allowed.store(false, Ordering::SeqCst);
        assert_eq!(receiver.recv().await.unwrap().event.sequence, 1);
        assert!(!publication.await);
        assert!(checked.load(Ordering::SeqCst));
        let reset = receiver
            .try_recv()
            .expect("denial must terminate the client snapshot");
        assert!(matches!(
            reset.event.payload,
            EventPayload::Reset {
                code: ErrorCode::ResetRequired
            }
        ));
        allowed.store(true, Ordering::SeqCst);
        assert!(
            !(reset.authority)().await,
            "later recovery cannot revive denied data"
        );
    }

    #[tokio::test]
    async fn authorized_publication_preserves_event_identity() {
        let (sender, mut receiver) = mpsc::channel(1);
        let authority: EventAuthorityCheck = Arc::new(|| Box::pin(async { true }));
        let alive = Arc::new(AtomicBool::new(true));
        let failed = Arc::new(AtomicBool::new(false));
        assert!(publish_authorized_event(&sender, &authority, &alive, &failed, event(3)).await);
        let delivered = receiver.recv().await.unwrap().event;
        assert_eq!(delivered.session.handle, "session");
        assert_eq!(delivered.session.generation, 7);
        assert_eq!(delivered.subscription_id, "subscription");
        assert_eq!(delivered.sequence, 3);
    }
}

pub type EventAuthorityCheck = Arc<
    dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>> + Send + Sync,
>;

async fn publish_authorized_event(
    events: &mpsc::Sender<crate::business_data_ipc::QueuedBusinessDataEvent>,
    authority: &EventAuthorityCheck,
    alive: &Arc<AtomicBool>,
    failed: &Arc<AtomicBool>,
    mut event: Event,
) -> bool {
    let Ok(permit) = events.reserve().await else {
        return false;
    };
    if failed.load(Ordering::SeqCst) || !alive.load(Ordering::SeqCst) {
        return false;
    }
    let authorized = authority().await;
    if failed.load(Ordering::SeqCst) || !alive.load(Ordering::SeqCst) {
        return false;
    }
    let delivery_authority: EventAuthorityCheck = if authorized {
        authority.clone()
    } else {
        event.payload = EventPayload::Reset {
            code: ErrorCode::ResetRequired,
        };
        // Retain this denial through queueing even if host authority recovers.
        // The writer emits one reset and latches this exact watch as failed.
        Arc::new(|| Box::pin(async { false }))
    };
    // Capacity is reserved before the authority await. No further await can
    // delay publication after the final session/epoch check.
    permit.send(crate::business_data_ipc::QueuedBusinessDataEvent {
        event,
        alive: alive.clone(),
        failed: failed.clone(),
        authority: delivery_authority,
    });
    authorized
}

pub fn spawn_remote_event_pump(
    pool: NativePool,
    peer: WebRTCRsConnection,
    session: SessionRef,
    events: mpsc::Sender<crate::business_data_ipc::QueuedBusinessDataEvent>,
    authority: EventAuthorityCheck,
) -> OwnedEventPump {
    let alive = Arc::new(AtomicBool::new(true));
    let failed = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let notify = Arc::new(Notify::new());
    let accepted_subscription = Arc::new(Mutex::new(None::<String>));
    // Subscribe before returning to the caller that starts the remote request.
    // The spawned task may not be polled before the source publishes its first
    // snapshot event; registering inside it would silently lose that event.
    let mut messages = pool.connection_handler.message_stream();
    let task = {
        let alive = alive.clone();
        let failed = failed.clone();
        let release = release.clone();
        let notify = notify.clone();
        let accepted_subscription = accepted_subscription.clone();
        tokio::spawn(async move {
            let mut queue = VecDeque::<Event>::new();
            let mut overflowed = false;
            loop {
                // A notification only wakes this loop. The atomic release flag
                // is authoritative, including when notification preceded polling.
                if release.load(Ordering::SeqCst) {
                    let accepted = accepted_subscription
                        .lock()
                        .ok()
                        .and_then(|accepted| accepted.clone());
                    let Some(accepted) = accepted else {
                        return;
                    };
                    if overflowed {
                        let _ = publish_authorized_event(
                            &events,
                            &authority,
                            &alive,
                            &failed,
                            Event {
                                version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
                                session: session.clone(),
                                subscription_id: accepted,
                                sequence: 0,
                                payload: EventPayload::Reset {
                                    code: ErrorCode::ResetRequired,
                                },
                            },
                        )
                        .await;
                        return;
                    }
                    while let Some(event) = queue.pop_front() {
                        if event.subscription_id != accepted {
                            continue;
                        }
                        if !publish_authorized_event(&events, &authority, &alive, &failed, event)
                            .await
                        {
                            return;
                        }
                    }
                }
                tokio::select! {
                    _ = notify.notified() => {}
                    item = messages.next() => {
                        let Some(item) = item else { return; };
                        if item.peer != peer || item.message.method != BUSINESS_DATA_EVENT_METHOD {
                            continue;
                        }
                        let Some(value) = item.message.params.into_iter().next() else { continue; };
                        let Ok(event) = serde_json::from_value::<Event>(value) else { continue; };
                        if event.version != CTOX_BUSINESS_DATA_PROTOCOL_VERSION
                            || event.session != session
                            || event.subscription_id.is_empty()
                            || event.sequence == 0
                        {
                            continue;
                        }
                        if release.load(Ordering::SeqCst) {
                            let accepted = accepted_subscription.lock().ok()
                                .and_then(|accepted| accepted.clone());
                            if accepted.as_deref() != Some(event.subscription_id.as_str()) {
                                continue;
                            }
                            if !publish_authorized_event(&events, &authority, &alive, &failed, event).await { return; }
                        } else if queue.len() >= CLIENT_EVENT_BUFFER {
                            // Defer the explicit reset until exact subscription
                            // acceptance; never invent a subscription identity.
                            queue.clear();
                            overflowed = true;
                        } else if !overflowed {
                            queue.push_back(event);
                        }
                    }
                }
            }
        })
    };
    OwnedEventPump {
        alive,
        task,
        release,
        notify,
        accepted_subscription,
    }
}
