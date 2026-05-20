//! Functional port of `src/plugins/replication/index.ts`.
//!
//! Upstream is the 25 KB user-facing replication API. CTOX consumers
//! (`replicate_web_rtc` fork-path; potential SaaS plugins) call
//! [`replicate_rx_collection`] with user-supplied `pull` / `push` handlers
//! that fetch/send docs from/to a remote endpoint. The returned
//! [`RxReplicationState`] exposes RxJS-style streams (`received`, `sent`,
//! `error`, `canceled`, `active`).
//!
//! Scope:
//! - Type surface (`ReplicationOptions`, `ReplicationPullOptions`,
//!   `ReplicationPushOptions`, `ReplicationPullHandlerResult`,
//!   `PullHandler`, `PushHandler`, `StreamFactory`) — done.
//! - `RxReplicationState` struct with 5 reactive subjects + state fields — done.
//! - `replicate_rx_collection(options)` constructor — done.
//! - `RxReplicationState::start()` — **functional**: builds the meta-instance
//!   via `database.storage.create_storage_instance`, wraps pull/push closures
//!   in a `ClosureReplicationHandler`, registers the connected meta storage in
//!   the database internal store and starts the `replicate_rx_storage_instance`
//!   state machine. Retry/error envelope is delegated to the underlying
//!   protocol; on top of it, errors on the bus surface via the `error` subject.
//! - `RxReplicationState::cancel()` — cancels the protocol and removes the
//!   connected meta storage registration.
//! - Retry / awaitRetry, start-queue serialization, pause/cancel lifecycle,
//!   remote RESYNC/event injection, and the public await-in-sync helpers are
//!   implemented. Browser visibility and preventHibernateBrowserTab are CTOX
//!   server-side no-ops.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;

use crate::plugins::replication::replication_helper::{
    await_retry, default_modifier, handle_pulled_documents_with_schema,
    swap_default_deleted_to_deleted_field,
};
use crate::plugins::utils::utils_error::error_to_plain_json;
use crate::rx_collection::RxCollection;
use crate::rx_database_internal_store::{
    add_connected_storage_to_collection, remove_connected_storage_from_collection,
};
use crate::rx_error::{new_rx_error, RxError};
use crate::rxjs_compat::{RxBehaviorSubject, RxStream, RxSubject};
use crate::types::{
    DocumentsWithCheckpoint, InitialCheckpoint, RxJsonSchema, RxReplicationHandler,
    RxReplicationMasterChange, RxReplicationWriteToMasterRow, RxStorageInstanceCreationParams,
    RxStorageInstanceReplicationInput,
};

/// Result of a pull-handler invocation.
#[derive(Debug, Clone)]
pub struct ReplicationPullHandlerResult {
    pub documents: Vec<Value>,
    pub checkpoint: Value,
}

/// User-supplied pull-handler closure.
pub type PullHandler = Arc<
    dyn Fn(
            Option<Value>,
            u64,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<ReplicationPullHandlerResult, RxError>>
                    + Send,
            >,
        > + Send
        + Sync,
>;

/// User-supplied push-handler closure. Returns the master-side conflicts.
pub type PushHandler = Arc<
    dyn Fn(
            Vec<RxReplicationWriteToMasterRow>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Vec<Value>, RxError>> + Send>,
        > + Send
        + Sync,
>;

/// User-supplied per-document modifier (pre-push or post-pull).
pub type DocumentModifier = Arc<
    dyn Fn(
            Value,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, RxError>> + Send>>
        + Send
        + Sync,
>;

/// Factory for the master-change stream (called per subscriber).
pub type StreamFactory = Arc<dyn Fn() -> RxStream<RxReplicationMasterChange> + Send + Sync>;

// ref: rxdb/src/types/replication.d.ts ReplicationPullOptions<RxDocType, CheckpointType>
pub struct ReplicationPullOptions {
    pub handler: PullHandler,
    pub stream_factory: Option<StreamFactory>,
    pub batch_size: u64,
    pub modifier: Option<DocumentModifier>,
    /// "asc" | "desc" (default "asc")
    pub initial_checkpoint: Option<Value>,
}

// ref: rxdb/src/types/replication.d.ts ReplicationPushOptions<RxDocType>
pub struct ReplicationPushOptions {
    pub handler: PushHandler,
    pub batch_size: u64,
    pub modifier: Option<DocumentModifier>,
    pub initial_checkpoint: Option<Value>,
}

// ref: rxdb/src/types/replication.d.ts ReplicationOptions<RxDocType, CheckpointType>
pub struct ReplicationOptions {
    pub replication_identifier: String,
    pub collection: Arc<RxCollection>,
    pub deleted_field: String,
    pub pull: Option<ReplicationPullOptions>,
    pub push: Option<ReplicationPushOptions>,
    pub live: bool,
    pub retry_time: u64,
    pub auto_start: bool,
    pub wait_for_leadership: bool,
}

// ref: rxdb/src/plugins/replication/index.ts:76+
pub struct RxReplicationState {
    pub replication_identifier: String,
    pub collection: Arc<RxCollection>,
    pub deleted_field: String,
    pub live: bool,
    pub retry_time: u64,
    pub auto_start: bool,
    pub wait_for_leadership: bool,

    // Reactive subjects (the public surface upstream exposes as observables).
    pub received: RxSubject<Value>,
    pub sent: RxSubject<Value>,
    pub error: RxSubject<RxError>,
    pub canceled: RxBehaviorSubject<bool>,
    pub active: RxBehaviorSubject<bool>,
    pub paused: RxBehaviorSubject<bool>,

    remote_events: RxSubject<RxReplicationMasterChange>,
    start_lock: TokioMutex<()>,
    was_started: AtomicBool,
    pull: Option<ReplicationPullOptions>,
    push: Option<ReplicationPushOptions>,
    internal_state: TokioMutex<Option<Arc<crate::types::RxStorageInstanceReplicationState>>>,
    connected_storage: TokioMutex<Option<(String, RxJsonSchema)>>,
    event_tasks: parking_lot::Mutex<Vec<JoinHandle<()>>>,
    on_cancel: parking_lot::Mutex<Vec<Box<dyn FnOnce() + Send>>>,
}

impl RxReplicationState {
    pub fn new(opts: ReplicationOptions) -> Arc<Self> {
        Arc::new(Self {
            replication_identifier: opts.replication_identifier,
            collection: opts.collection,
            deleted_field: opts.deleted_field,
            live: opts.live,
            retry_time: opts.retry_time,
            auto_start: opts.auto_start,
            wait_for_leadership: opts.wait_for_leadership,
            received: RxSubject::new(),
            sent: RxSubject::new(),
            error: RxSubject::new(),
            canceled: RxBehaviorSubject::new(false),
            active: RxBehaviorSubject::new(false),
            paused: RxBehaviorSubject::new(false),
            remote_events: RxSubject::new(),
            start_lock: TokioMutex::new(()),
            was_started: AtomicBool::new(false),
            pull: opts.pull,
            push: opts.push,
            internal_state: TokioMutex::new(None),
            connected_storage: TokioMutex::new(None),
            event_tasks: parking_lot::Mutex::new(Vec::new()),
            on_cancel: parking_lot::Mutex::new(Vec::new()),
        })
    }

    pub fn received_stream(&self) -> RxStream<Value> {
        self.received.subscribe()
    }
    pub fn sent_stream(&self) -> RxStream<Value> {
        self.sent.subscribe()
    }
    pub fn error_stream(&self) -> RxStream<RxError> {
        self.error.subscribe()
    }
    pub fn canceled_stream(&self) -> RxStream<bool> {
        self.canceled.subscribe()
    }
    pub fn active_stream(&self) -> RxStream<bool> {
        self.active.subscribe()
    }

    pub fn on_cancel(&self, cb: Box<dyn FnOnce() + Send>) {
        self.on_cancel.lock().push(cb);
    }

    pub fn was_started(&self) -> bool {
        self.was_started.load(Ordering::SeqCst)
    }

    /// Start the underlying `replicate_rx_storage_instance` machinery.
    ///
    /// Builds the meta-instance via `database.storage.create_storage_instance`,
    /// wraps pull/push handler closures in a [`ClosureReplicationHandler`] and
    /// starts the protocol. Idempotent and serialized like upstream's
    /// `startQueue`: subsequent calls wait until the first start has finished
    /// initializing the internal replication state.
    pub async fn start(self: &Arc<Self>) -> Result<(), RxError> {
        use crate::replication_protocol::index_mod::replicate_rx_storage_instance;
        use crate::replication_protocol::meta_instance::get_rx_replication_meta_instance_schema;

        let _start_guard = self.start_lock.lock().await;
        if self.was_started.swap(true, Ordering::SeqCst) {
            if let Some(state) = self.internal_state.lock().await.as_ref() {
                state.events.paused.next(false);
            }
            self.paused.next(false);
            return Ok(());
        }
        if self.wait_for_leadership && self.collection.database.multi_instance {
            self.collection.database.wait_for_leadership().await;
        }
        self.paused.next(false);

        let database = Arc::clone(&self.collection.database);
        let collection_schema = self.collection.storage_instance.schema().clone();

        // ref: rxdb/src/plugins/replication/index.ts:122-125
        let hash_input = format!("{}-{}", self.collection.name, self.replication_identifier);
        let hash = database.hash_function.hash(hash_input).await;
        let meta_collection_name = format!("rx-replication-meta-{}", hash);
        let meta_schema = get_rx_replication_meta_instance_schema(&collection_schema, false)?;
        let meta_schema_for_registry = meta_schema.clone();

        // ref: rxdb/src/plugins/replication/index.ts:204-214
        let meta_instance = database
            .storage
            .create_storage_instance(RxStorageInstanceCreationParams {
                database_instance_token: database.token.clone(),
                database_name: database.name.clone(),
                collection_name: meta_collection_name.clone(),
                schema: meta_schema,
                options: std::collections::HashMap::new(),
                multi_instance: database.multi_instance,
                dev_mode: false,
                password: None,
            })
            .await?;
        if let Some(internal_store) = database.internal_store.as_ref() {
            add_connected_storage_to_collection(
                internal_store,
                &self.collection.name,
                &collection_schema,
                &meta_collection_name,
                &meta_schema_for_registry,
            )
            .await?;
            *self.connected_storage.lock().await =
                Some((meta_collection_name.clone(), meta_schema_for_registry));
        }

        // Build the closure-driven replication handler.
        let push_batch_size = self.push.as_ref().map(|p| p.batch_size).unwrap_or(100);
        let pull_batch_size = self.pull.as_ref().map(|p| p.batch_size).unwrap_or(100);
        let initial_checkpoint = InitialCheckpoint {
            upstream: self
                .push
                .as_ref()
                .and_then(|p| p.initial_checkpoint.clone()),
            downstream: self
                .pull
                .as_ref()
                .and_then(|p| p.initial_checkpoint.clone()),
        };
        let handler: Arc<dyn RxReplicationHandler> = Arc::new(ClosureReplicationHandler {
            pull_handler: self.pull.as_ref().map(|p| Arc::clone(&p.handler)),
            push_handler: self.push.as_ref().map(|p| Arc::clone(&p.handler)),
            stream_factory: self.pull.as_ref().and_then(|p| p.stream_factory.clone()),
            pull_modifier: self
                .pull
                .as_ref()
                .and_then(|p| p.modifier.clone())
                .unwrap_or_else(default_document_modifier),
            push_modifier: self
                .push
                .as_ref()
                .and_then(|p| p.modifier.clone())
                .unwrap_or_else(default_document_modifier),
            collection_schema: collection_schema.clone(),
            deleted_field: self.deleted_field.clone(),
            retry_time: self.retry_time,
            canceled: self.canceled.clone(),
            paused: self.paused.clone(),
            error_relay: self.error.clone(),
            remote_events: self.remote_events.clone(),
        });

        let input = RxStorageInstanceReplicationInput {
            identifier: format!("rxdbreplication{}", self.replication_identifier),
            fork_instance: Arc::clone(&self.collection.storage_instance),
            meta_instance,
            hash_function: Arc::clone(&database.hash_function),
            conflict_handler: Arc::clone(&self.collection.conflict_handler),
            replication_handler: handler,
            push_batch_size,
            pull_batch_size,
            bulk_size: std::cmp::max(push_batch_size, pull_batch_size),
            keep_meta: false,
            initial_checkpoint: Some(initial_checkpoint),
            wait_before_persist: None,
        };
        let internal = replicate_rx_storage_instance(input).await;
        self.wire_internal_events(Arc::clone(&internal));
        *self.internal_state.lock().await = Some(internal);
        if !self.live {
            let state = self.internal_state.lock().await.clone().ok_or_else(|| {
                new_rx_error(
                    "RC_STATE",
                    Some(
                        serde_json::json!({ "replicationIdentifier": self.replication_identifier }),
                    ),
                )
            })?;
            crate::replication_protocol::index_mod::await_rx_storage_replication_first_in_sync(
                Arc::clone(&state),
            )
            .await;
            crate::replication_protocol::index_mod::await_rx_storage_replication_in_sync(state)
                .await;
            self.cancel().await;
        }
        Ok(())
    }

    fn wire_internal_events(
        self: &Arc<Self>,
        internal: Arc<crate::types::RxStorageInstanceReplicationState>,
    ) {
        use tokio_stream::StreamExt;

        let received = self.received.clone();
        let mut down_processed = internal.events.processed.down.subscribe();
        let received_task = tokio::spawn(async move {
            while let Some(row) = down_processed.next().await {
                let document = row.get("document").cloned().unwrap_or_else(|| row.clone());
                received.next(document);
            }
        });

        let sent = self.sent.clone();
        let mut up_processed = internal.events.processed.up.subscribe();
        let sent_task = tokio::spawn(async move {
            while let Some(row) = up_processed.next().await {
                let document = row
                    .get("newDocumentState")
                    .cloned()
                    .unwrap_or_else(|| row.clone());
                sent.next(document);
            }
        });

        let down_active = Arc::new(AtomicBool::new(internal.events.active.down.get_value()));
        let up_active = Arc::new(AtomicBool::new(internal.events.active.up.get_value()));
        self.active
            .next(down_active.load(Ordering::SeqCst) || up_active.load(Ordering::SeqCst));

        let active_down = self.active.clone();
        let down_active_for_task = Arc::clone(&down_active);
        let up_active_for_down = Arc::clone(&up_active);
        let mut down_stream = internal.events.active.down.subscribe();
        let active_down_task = tokio::spawn(async move {
            while let Some(is_down_active) = down_stream.next().await {
                down_active_for_task.store(is_down_active, Ordering::SeqCst);
                active_down.next(is_down_active || up_active_for_down.load(Ordering::SeqCst));
            }
        });

        let active_up = self.active.clone();
        let down_active_for_up = Arc::clone(&down_active);
        let up_active_for_task = Arc::clone(&up_active);
        let mut up_stream = internal.events.active.up.subscribe();
        let active_up_task = tokio::spawn(async move {
            while let Some(is_up_active) = up_stream.next().await {
                up_active_for_task.store(is_up_active, Ordering::SeqCst);
                active_up.next(down_active_for_up.load(Ordering::SeqCst) || is_up_active);
            }
        });

        self.event_tasks.lock().extend([
            received_task,
            sent_task,
            active_down_task,
            active_up_task,
        ]);
    }

    pub async fn cancel(self: &Arc<Self>) {
        if self.canceled.get_value() {
            return;
        }
        self.canceled.next(true);
        self.active.next(false);
        let cbs = std::mem::take(&mut *self.on_cancel.lock());
        for cb in cbs.into_iter() {
            cb();
        }
        if let Some(state) = self.internal_state.lock().await.as_ref() {
            crate::replication_protocol::index_mod::cancel_rx_storage_replication(Arc::clone(
                state,
            ))
            .await;
        }
        for task in std::mem::take(&mut *self.event_tasks.lock()) {
            task.abort();
        }
        let Some((storage_collection_name, schema)) = self.connected_storage.lock().await.take()
        else {
            return;
        };
        let Some(internal_store) = self.collection.database.internal_store.as_ref() else {
            return;
        };
        let collection_schema = self.collection.storage_instance.schema().clone();
        let _ = remove_connected_storage_from_collection(
            internal_store,
            &self.collection.name,
            &collection_schema,
            &storage_collection_name,
            &schema,
        )
        .await;
    }

    pub async fn pause(self: &Arc<Self>) -> Result<(), RxError> {
        if self.internal_state.lock().await.is_none() && self.auto_start {
            self.start().await?;
        }
        if let Some(state) = self.internal_state.lock().await.as_ref() {
            state.events.paused.next(true);
        }
        self.paused.next(true);
        self.active.next(false);
        Ok(())
    }

    pub fn is_paused(&self) -> bool {
        self.paused.get_value()
    }

    pub fn is_stopped(&self) -> bool {
        self.canceled.get_value()
    }

    pub fn is_stopped_or_paused(&self) -> bool {
        self.is_stopped() || self.is_paused()
    }

    pub async fn await_initial_replication(self: &Arc<Self>) -> Result<(), RxError> {
        self.start().await?;
        let state = self.internal_state.lock().await.clone().ok_or_else(|| {
            crate::rx_error::new_rx_error(
                "RC_STATE",
                Some(serde_json::json!({ "replicationIdentifier": self.replication_identifier })),
            )
        })?;
        crate::replication_protocol::index_mod::await_rx_storage_replication_first_in_sync(state)
            .await;
        Ok(())
    }

    pub async fn await_in_sync(self: &Arc<Self>) -> Result<bool, RxError> {
        self.await_initial_replication().await?;
        let state = self.internal_state.lock().await.clone().ok_or_else(|| {
            crate::rx_error::new_rx_error(
                "RC_STATE",
                Some(serde_json::json!({ "replicationIdentifier": self.replication_identifier })),
            )
        })?;
        for _ in 0..2 {
            self.collection.database.request_idle_promise().await;
            crate::replication_protocol::index_mod::await_rx_storage_replication_in_sync(
                Arc::clone(&state),
            )
            .await;
        }
        Ok(true)
    }

    pub fn emit_event(&self, event: RxReplicationMasterChange) {
        self.remote_events.next(event);
    }

    pub fn re_sync(&self) {
        self.emit_event(RxReplicationMasterChange::Resync);
    }
}

// ref: rxdb/src/plugins/replication/index.ts replicateRxCollection
/// Construct a new `RxReplicationState`. If `opts.auto_start = true`, also
/// invokes `start()`.
pub async fn replicate_rx_collection(
    opts: ReplicationOptions,
) -> Result<Arc<RxReplicationState>, RxError> {
    if opts.pull.is_none() && opts.push.is_none() {
        return Err(new_rx_error(
            "UT3",
            Some(serde_json::json!({
                "collection": opts.collection.name,
                "args": {
                    "replicationIdentifier": opts.replication_identifier,
                },
            })),
        ));
    }
    let auto_start = opts.auto_start;
    let state = RxReplicationState::new(opts);
    if auto_start {
        state.start().await?;
    }
    Ok(state)
}

// ref: rxdb/src/plugins/replication/index.ts:235-298
/// Adapter that turns user-supplied pull/push closures into a
/// [`RxReplicationHandler`] consumable by `replicate_rx_storage_instance`.
/// `master_change_stream` falls through to the optional stream factory; if no
/// factory is provided, the handler emits an empty stream (= pull-only via
/// polling / push-only setups).
struct ClosureReplicationHandler {
    pull_handler: Option<PullHandler>,
    push_handler: Option<PushHandler>,
    stream_factory: Option<StreamFactory>,
    pull_modifier: DocumentModifier,
    push_modifier: DocumentModifier,
    collection_schema: RxJsonSchema,
    deleted_field: String,
    retry_time: u64,
    canceled: RxBehaviorSubject<bool>,
    paused: RxBehaviorSubject<bool>,
    error_relay: RxSubject<RxError>,
    remote_events: RxSubject<RxReplicationMasterChange>,
}

#[async_trait::async_trait]
impl crate::types::RxReplicationHandler for ClosureReplicationHandler {
    fn master_change_stream(&self) -> RxStream<RxReplicationMasterChange> {
        use futures::StreamExt;

        let schema = self.collection_schema.clone();
        let deleted_field = self.deleted_field.clone();
        let pull_modifier = Arc::clone(&self.pull_modifier);
        let canceled = self.canceled.clone();
        let paused = self.paused.clone();
        let error_relay = self.error_relay.clone();
        let remote_events = self.remote_events.subscribe();
        let stream: RxStream<RxReplicationMasterChange> = match &self.stream_factory {
            Some(factory) => Box::pin(futures::stream::select(remote_events, factory())),
            None => remote_events,
        };
        Box::pin(stream.filter_map(move |item| {
            let schema = schema.clone();
            let deleted_field = deleted_field.clone();
            let pull_modifier = Arc::clone(&pull_modifier);
            let canceled = canceled.clone();
            let paused = paused.clone();
            let error_relay = error_relay.clone();
            async move {
                let RxReplicationMasterChange::Documents(item) = item else {
                    return Some(RxReplicationMasterChange::Resync);
                };
                if canceled.get_value() || paused.get_value() {
                    return None;
                }
                let checkpoint = item.checkpoint.clone();
                match prepare_pulled_documents(
                    &schema,
                    &deleted_field,
                    &pull_modifier,
                    item.documents,
                )
                .await
                {
                    Ok(documents) => Some(RxReplicationMasterChange::Documents(
                        DocumentsWithCheckpoint {
                            documents,
                            checkpoint,
                        },
                    )),
                    Err(e) => {
                        error_relay.next(e);
                        None
                    }
                }
            }
        }))
    }

    async fn master_changes_since(
        &self,
        checkpoint: Option<Value>,
        batch_size: u64,
    ) -> Result<DocumentsWithCheckpoint, RxError> {
        let Some(handler) = self.pull_handler.as_ref() else {
            return Ok(DocumentsWithCheckpoint {
                documents: Vec::new(),
                checkpoint: Value::Null,
            });
        };
        let result = loop {
            if self.canceled.get_value() || self.paused.get_value() {
                return Ok(DocumentsWithCheckpoint {
                    documents: Vec::new(),
                    checkpoint: Value::Null,
                });
            }
            match handler(checkpoint.clone(), batch_size).await {
                Ok(result) => break result,
                Err(e) => {
                    self.error_relay.next(new_rx_error(
                        "RC_PULL",
                        Some(serde_json::json!({
                            "checkpoint": checkpoint.clone(),
                            "errors": [error_to_plain_json(&e)],
                            "direction": "pull",
                        })),
                    ));
                    await_retry(self.retry_time).await;
                }
            }
        };
        match prepare_pulled_documents(
            &self.collection_schema,
            &self.deleted_field,
            &self.pull_modifier,
            result.documents,
        )
        .await
        {
            Ok(documents) => Ok(DocumentsWithCheckpoint {
                documents,
                checkpoint: result.checkpoint,
            }),
            Err(e) => {
                self.error_relay.next(e.clone());
                Err(e)
            }
        }
    }

    async fn master_write(
        &self,
        rows: Vec<RxReplicationWriteToMasterRow>,
    ) -> Result<Vec<Value>, RxError> {
        let Some(handler) = self.push_handler.as_ref() else {
            return Ok(Vec::new());
        };
        let original_rows = rows.clone();
        let rows = prepare_push_rows(&self.deleted_field, &self.push_modifier, rows).await?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let conflicts = loop {
            if self.canceled.get_value() || self.paused.get_value() {
                return Ok(Vec::new());
            }
            match handler(rows.clone()).await {
                Ok(conflicts) => break conflicts,
                Err(e) => {
                    self.error_relay.next(new_rx_error(
                        "RC_PUSH",
                        Some(serde_json::json!({
                            "pushRows": original_rows.clone(),
                            "errors": [error_to_plain_json(&e)],
                            "direction": "push",
                        })),
                    ));
                    await_retry(self.retry_time).await;
                }
            }
        };
        handle_pulled_documents_with_schema(&self.collection_schema, &self.deleted_field, conflicts)
    }
}

fn default_document_modifier() -> DocumentModifier {
    Arc::new(|doc| default_modifier(doc))
}

async fn prepare_pulled_documents(
    schema: &RxJsonSchema,
    deleted_field: &str,
    modifier: &DocumentModifier,
    documents: Vec<Value>,
) -> Result<Vec<Value>, RxError> {
    let documents = handle_pulled_documents_with_schema(schema, deleted_field, documents)?;
    let mut out = Vec::with_capacity(documents.len());
    for document in documents {
        out.push(modifier(document).await?);
    }
    Ok(out)
}

async fn prepare_push_rows(
    deleted_field: &str,
    modifier: &DocumentModifier,
    rows: Vec<RxReplicationWriteToMasterRow>,
) -> Result<Vec<RxReplicationWriteToMasterRow>, RxError> {
    let mut out = Vec::with_capacity(rows.len());
    for mut row in rows {
        row.new_document_state = modifier(row.new_document_state).await?;
        if row.new_document_state.is_null() {
            continue;
        }
        if let Some(assumed) = row.assumed_master_state.take() {
            let modified = modifier(assumed).await?;
            row.assumed_master_state = if modified.is_null() {
                None
            } else {
                Some(swap_default_deleted_to_deleted_field(
                    deleted_field,
                    &modified,
                ))
            };
        }
        row.new_document_state =
            swap_default_deleted_to_deleted_field(deleted_field, &row.new_document_state);
        out.push(row);
    }
    Ok(out)
}

/// Construct a default `ReplicationOptions` for a single-collection setup.
/// Convenience helper for callers that don't need the full surface.
pub fn default_replication_options(
    replication_identifier: impl Into<String>,
    collection: Arc<RxCollection>,
) -> ReplicationOptions {
    let _ = default_modifier; // silence unused-import warning in some build configs
    ReplicationOptions {
        replication_identifier: replication_identifier.into(),
        collection,
        deleted_field: "_deleted".to_string(),
        pull: None,
        push: None,
        live: true,
        retry_time: 5_000,
        auto_start: false,
        wait_for_leadership: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    use serde_json::json;

    use crate::plugins::storage_memory::get_rx_storage_memory;
    use crate::rx_database::{create_rx_database, RxCollectionCreator, RxDatabaseCreator};
    use crate::rx_database_internal_store::get_all_collection_documents;
    use crate::types::{HashFunction, HashOutput, JsonSchema, PrimaryKey, RxJsonSchema, RxStorage};

    struct TestHashFunction;

    impl HashFunction for TestHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("hash:{input}") })
        }
    }

    struct BlockingHashFunction {
        block_replication_hash: AtomicBool,
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }

    impl BlockingHashFunction {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                block_replication_hash: AtomicBool::new(true),
                entered: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
            })
        }
    }

    impl HashFunction for BlockingHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            let should_block = input == "humans-remote"
                && self.block_replication_hash.swap(false, Ordering::SeqCst);
            let entered = &self.entered;
            let release = &self.release;
            Box::pin(async move {
                if should_block {
                    entered.notify_waiters();
                    release.notified().await;
                }
                format!("hash:{input}")
            })
        }
    }

    fn test_schema() -> RxJsonSchema {
        let mut properties = HashMap::new();
        properties.insert(
            "id".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
                max_length: Some(100),
                ..Default::default()
            },
        );
        RxJsonSchema {
            version: 0,
            primary_key: PrimaryKey::Simple("id".to_string()),
            schema_type: "object".to_string(),
            properties,
            required: vec!["id".to_string()],
            indexes: Vec::new(),
            encrypted: Vec::new(),
            internal_indexes: Vec::new(),
            key_compression: false,
            attachments: None,
            additional_properties: false,
            extra: HashMap::new(),
        }
    }

    fn empty_pull_options() -> ReplicationPullOptions {
        ReplicationPullOptions {
            handler: Arc::new(|_checkpoint, _batch_size| {
                Box::pin(async {
                    Ok(ReplicationPullHandlerResult {
                        documents: Vec::new(),
                        checkpoint: Value::Null,
                    })
                })
            }),
            stream_factory: None,
            batch_size: 10,
            modifier: None,
            initial_checkpoint: None,
        }
    }

    #[tokio::test]
    async fn start_registers_and_cancel_removes_connected_meta_storage() {
        let storage: Arc<dyn RxStorage> = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "replication-index-db".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        let collections = database
            .add_collections(HashMap::from([(
                "humans".to_string(),
                RxCollectionCreator {
                    schema: test_schema(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .unwrap();
        let collection = collections.get("humans").unwrap().clone();
        let mut opts = default_replication_options("remote", collection);
        opts.pull = Some(empty_pull_options());
        let state = replicate_rx_collection(opts).await.unwrap();

        state.start().await.unwrap();
        state.await_initial_replication().await.unwrap();
        assert!(state.await_in_sync().await.unwrap());

        let docs = get_all_collection_documents(database.internal_store.as_ref().unwrap())
            .await
            .unwrap();
        let connected = docs[0]
            .get("data")
            .and_then(|data| data.get("connectedStorages"))
            .and_then(|storages| storages.as_array())
            .unwrap();
        assert_eq!(connected.len(), 1);
        assert!(connected[0]
            .get("collectionName")
            .and_then(|value| value.as_str())
            .unwrap()
            .starts_with("rx-replication-meta-hash:humans-remote"));
        assert_eq!(connected[0]["schema"]["version"], json!(0));

        state.pause().await.unwrap();
        assert!(state.is_paused());
        assert!(state.is_stopped_or_paused());
        state.start().await.unwrap();
        assert!(!state.is_paused());

        state.cancel().await;
        assert!(state.is_stopped());

        let docs_after = get_all_collection_documents(database.internal_store.as_ref().unwrap())
            .await
            .unwrap();
        let connected_after = docs_after[0]
            .get("data")
            .and_then(|data| data.get("connectedStorages"))
            .and_then(|storages| storages.as_array())
            .unwrap();
        assert!(connected_after.is_empty());
    }

    #[tokio::test]
    async fn await_in_sync_drains_replication_queues_twice() {
        let storage: Arc<dyn RxStorage> = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "replication-index-await-sync-db".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        let collections = database
            .add_collections(HashMap::from([(
                "humans".to_string(),
                RxCollectionCreator {
                    schema: test_schema(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .unwrap();
        let collection = collections.get("humans").unwrap().clone();
        let mut opts = default_replication_options("remote", collection);
        opts.pull = Some(empty_pull_options());
        let state = replicate_rx_collection(opts).await.unwrap();
        state.start().await.unwrap();

        let internal = state.internal_state.lock().await.clone().unwrap();
        let checkpoint_guard = internal.checkpoint_queue.lock().await;
        let await_state = Arc::clone(&state);
        let await_task = tokio::spawn(async move { await_state.await_in_sync().await });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(
            !await_task.is_finished(),
            "first drain must wait for the checkpoint queue"
        );

        let blocker_internal = Arc::clone(&internal);
        let blocker_release = Arc::new(tokio::sync::Notify::new());
        let blocker_release_for_task = Arc::clone(&blocker_release);
        let blocker_task = tokio::spawn(async move {
            let _down = blocker_internal.stream_queue.down.lock().await;
            blocker_release_for_task.notified().await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        drop(checkpoint_guard);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(
            !await_task.is_finished(),
            "second drain must catch queue work that was waiting during the first drain"
        );

        blocker_release.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(1), await_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        blocker_task.await.unwrap();
        state.cancel().await;
    }

    #[tokio::test]
    async fn parallel_start_waits_for_internal_state_initialization() {
        let storage: Arc<dyn RxStorage> = get_rx_storage_memory(());
        let hash_function = BlockingHashFunction::new();
        let database = create_rx_database(RxDatabaseCreator {
            name: "replication-index-parallel-start-db".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: hash_function.clone(),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        let collections = database
            .add_collections(HashMap::from([(
                "humans".to_string(),
                RxCollectionCreator {
                    schema: test_schema(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .unwrap();
        let collection = collections.get("humans").unwrap().clone();
        let mut opts = default_replication_options("remote", collection);
        opts.pull = Some(empty_pull_options());
        let state = replicate_rx_collection(opts).await.unwrap();

        let first_state = Arc::clone(&state);
        let first_start = tokio::spawn(async move { first_state.start().await });
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            hash_function.entered.notified(),
        )
        .await
        .unwrap();

        let second_state = Arc::clone(&state);
        let second_start = tokio::spawn(async move { second_state.start().await });
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(
            !second_start.is_finished(),
            "second start must wait for the first start initialization"
        );

        hash_function.release.notify_one();
        first_start.await.unwrap().unwrap();
        second_start.await.unwrap().unwrap();

        assert!(state.was_started());
        assert!(state.internal_state.lock().await.is_some());
        let docs = get_all_collection_documents(database.internal_store.as_ref().unwrap())
            .await
            .unwrap();
        let connected = docs[0]
            .get("data")
            .and_then(|data| data.get("connectedStorages"))
            .and_then(|storages| storages.as_array())
            .unwrap();
        assert_eq!(connected.len(), 1);

        state.cancel().await;
    }

    #[tokio::test]
    async fn replicate_rx_collection_rejects_missing_pull_and_push() {
        let storage: Arc<dyn RxStorage> = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "replication-index-guard-db".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        let collections = database
            .add_collections(HashMap::from([(
                "humans".to_string(),
                RxCollectionCreator {
                    schema: test_schema(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .unwrap();
        let collection = collections.get("humans").unwrap().clone();

        let err = match replicate_rx_collection(default_replication_options("remote", collection))
            .await
        {
            Ok(_) => panic!("replication without pull or push must fail"),
            Err(err) => err,
        };

        assert_eq!(err.code(), "UT3");
    }

    #[tokio::test]
    async fn non_live_replication_auto_cancels_after_initial_sync() {
        let storage: Arc<dyn RxStorage> = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "replication-index-non-live-db".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        let collections = database
            .add_collections(HashMap::from([(
                "humans".to_string(),
                RxCollectionCreator {
                    schema: test_schema(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .unwrap();
        let collection = collections.get("humans").unwrap().clone();
        let mut opts = default_replication_options("remote", collection);
        opts.pull = Some(empty_pull_options());
        opts.live = false;

        let state = replicate_rx_collection(opts).await.unwrap();
        state.start().await.unwrap();

        assert!(state.is_stopped());
        let docs = get_all_collection_documents(database.internal_store.as_ref().unwrap())
            .await
            .unwrap();
        let connected = docs[0]
            .get("data")
            .and_then(|data| data.get("connectedStorages"))
            .and_then(|storages| storages.as_array())
            .unwrap();
        assert!(connected.is_empty());
    }

    #[tokio::test]
    async fn master_changes_since_normalizes_deleted_field_and_applies_modifier() {
        let pull_handler: PullHandler = Arc::new(|_checkpoint, _batch_size| {
            Box::pin(async {
                Ok(ReplicationPullHandlerResult {
                    documents: vec![json!({ "id": "alice", "deleted": true })],
                    checkpoint: json!({ "sequence": 1 }),
                })
            })
        });
        let pull_modifier: DocumentModifier = Arc::new(|mut document| {
            Box::pin(async move {
                document
                    .as_object_mut()
                    .unwrap()
                    .insert("fromModifier".to_string(), json!(true));
                Ok(document)
            })
        });
        let handler = ClosureReplicationHandler {
            pull_handler: Some(pull_handler),
            push_handler: None,
            stream_factory: None,
            pull_modifier,
            push_modifier: default_document_modifier(),
            collection_schema: test_schema(),
            deleted_field: "deleted".to_string(),
            retry_time: 0,
            canceled: RxBehaviorSubject::new(false),
            paused: RxBehaviorSubject::new(false),
            error_relay: RxSubject::new(),
            remote_events: RxSubject::new(),
        };

        let result = handler.master_changes_since(None, 10).await.unwrap();

        assert_eq!(result.checkpoint, json!({ "sequence": 1 }));
        assert_eq!(result.documents.len(), 1);
        assert_eq!(result.documents[0]["_deleted"], json!(true));
        assert_eq!(result.documents[0]["fromModifier"], json!(true));
        assert!(result.documents[0].get("deleted").is_none());
    }

    #[tokio::test]
    async fn master_changes_since_returns_empty_while_paused() {
        let pull_calls = Arc::new(AtomicUsize::new(0));
        let pull_calls_for_handler = Arc::clone(&pull_calls);
        let pull_handler: PullHandler = Arc::new(move |_checkpoint, _batch_size| {
            let pull_calls = Arc::clone(&pull_calls_for_handler);
            Box::pin(async move {
                pull_calls.fetch_add(1, Ordering::SeqCst);
                Ok(ReplicationPullHandlerResult {
                    documents: vec![json!({ "id": "alice", "_deleted": false })],
                    checkpoint: json!({ "sequence": 1 }),
                })
            })
        });
        let handler = ClosureReplicationHandler {
            pull_handler: Some(pull_handler),
            push_handler: None,
            stream_factory: None,
            pull_modifier: default_document_modifier(),
            push_modifier: default_document_modifier(),
            collection_schema: test_schema(),
            deleted_field: "_deleted".to_string(),
            retry_time: 0,
            canceled: RxBehaviorSubject::new(false),
            paused: RxBehaviorSubject::new(true),
            error_relay: RxSubject::new(),
            remote_events: RxSubject::new(),
        };

        let result = handler.master_changes_since(None, 10).await.unwrap();

        assert!(result.documents.is_empty());
        assert!(result.checkpoint.is_null());
        assert_eq!(pull_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn master_changes_since_wraps_handler_errors_with_pull_envelope() {
        use futures::StreamExt;

        let pull_calls = Arc::new(AtomicUsize::new(0));
        let pull_calls_for_handler = Arc::clone(&pull_calls);
        let pull_handler: PullHandler = Arc::new(move |_checkpoint, _batch_size| {
            let pull_calls = Arc::clone(&pull_calls_for_handler);
            Box::pin(async move {
                if pull_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(new_rx_error("TEST_PULL", Some(json!({ "attempt": 1 }))));
                }
                Ok(ReplicationPullHandlerResult {
                    documents: vec![json!({ "id": "alice", "_deleted": false })],
                    checkpoint: json!({ "sequence": 2 }),
                })
            })
        });
        let error_relay = RxSubject::new();
        let mut errors = error_relay.subscribe();
        let handler = ClosureReplicationHandler {
            pull_handler: Some(pull_handler),
            push_handler: None,
            stream_factory: None,
            pull_modifier: default_document_modifier(),
            push_modifier: default_document_modifier(),
            collection_schema: test_schema(),
            deleted_field: "_deleted".to_string(),
            retry_time: 0,
            canceled: RxBehaviorSubject::new(false),
            paused: RxBehaviorSubject::new(false),
            error_relay,
            remote_events: RxSubject::new(),
        };

        let result = handler
            .master_changes_since(Some(json!({ "sequence": 1 })), 10)
            .await
            .unwrap();
        let err = tokio::time::timeout(std::time::Duration::from_secs(1), errors.next())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(pull_calls.load(Ordering::SeqCst), 2);
        assert_eq!(result.checkpoint, json!({ "sequence": 2 }));
        assert_eq!(err.code(), "RC_PULL");
        assert_eq!(err.parameters()["direction"], json!("pull"));
        assert_eq!(err.parameters()["checkpoint"], json!({ "sequence": 1 }));
        assert_eq!(err.parameters()["errors"][0]["code"], json!("TEST_PULL"));
        assert_eq!(err.parameters()["errors"][0]["rxdb"], json!(true));
    }

    #[tokio::test]
    async fn master_change_stream_drops_batches_when_pull_modifier_errors() {
        use futures::StreamExt;

        let stream_factory: StreamFactory = Arc::new(|| {
            Box::pin(futures::stream::once(async {
                RxReplicationMasterChange::Documents(DocumentsWithCheckpoint {
                    documents: vec![json!({ "id": "alice", "_deleted": false })],
                    checkpoint: json!({ "sequence": 1 }),
                })
            }))
        });
        let pull_modifier: DocumentModifier =
            Arc::new(|_document| Box::pin(async { Err(new_rx_error("TEST_PULL_MODIFIER", None)) }));
        let error_relay = RxSubject::new();
        let mut errors = error_relay.subscribe();
        let handler = ClosureReplicationHandler {
            pull_handler: None,
            push_handler: None,
            stream_factory: Some(stream_factory),
            pull_modifier,
            push_modifier: default_document_modifier(),
            collection_schema: test_schema(),
            deleted_field: "_deleted".to_string(),
            retry_time: 0,
            canceled: RxBehaviorSubject::new(false),
            paused: RxBehaviorSubject::new(false),
            error_relay,
            remote_events: RxSubject::new(),
        };

        let mut stream = handler.master_change_stream();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), stream.next())
                .await
                .is_err()
        );
        let err = tokio::time::timeout(std::time::Duration::from_secs(1), errors.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(err.code(), "TEST_PULL_MODIFIER");
    }

    #[tokio::test]
    async fn master_change_stream_passes_remote_resync_events() {
        use futures::StreamExt;

        let remote_events = RxSubject::new();
        let handler = ClosureReplicationHandler {
            pull_handler: None,
            push_handler: None,
            stream_factory: None,
            pull_modifier: default_document_modifier(),
            push_modifier: default_document_modifier(),
            collection_schema: test_schema(),
            deleted_field: "_deleted".to_string(),
            retry_time: 0,
            canceled: RxBehaviorSubject::new(false),
            paused: RxBehaviorSubject::new(false),
            error_relay: RxSubject::new(),
            remote_events: remote_events.clone(),
        };
        let mut stream = handler.master_change_stream();

        remote_events.next(RxReplicationMasterChange::Resync);
        let item = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(item, RxReplicationMasterChange::Resync);
    }

    #[tokio::test]
    async fn master_write_applies_modifier_and_remote_deleted_field() {
        let seen_rows = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let seen_rows_for_handler = Arc::clone(&seen_rows);
        let push_handler: PushHandler = Arc::new(move |rows| {
            let seen_rows = Arc::clone(&seen_rows_for_handler);
            Box::pin(async move {
                *seen_rows.lock() = rows;
                Ok(vec![json!({ "id": "alice", "deleted": true })])
            })
        });
        let push_modifier: DocumentModifier = Arc::new(|mut document| {
            Box::pin(async move {
                document
                    .as_object_mut()
                    .unwrap()
                    .insert("fromModifier".to_string(), json!(true));
                Ok(document)
            })
        });
        let handler = ClosureReplicationHandler {
            pull_handler: None,
            push_handler: Some(push_handler),
            stream_factory: None,
            pull_modifier: default_document_modifier(),
            push_modifier,
            collection_schema: test_schema(),
            deleted_field: "deleted".to_string(),
            retry_time: 0,
            canceled: RxBehaviorSubject::new(false),
            paused: RxBehaviorSubject::new(false),
            error_relay: RxSubject::new(),
            remote_events: RxSubject::new(),
        };

        let conflicts = handler
            .master_write(vec![RxReplicationWriteToMasterRow {
                new_document_state: json!({ "id": "alice", "_deleted": true }),
                assumed_master_state: Some(json!({ "id": "alice", "_deleted": false })),
            }])
            .await
            .unwrap();

        let rows = seen_rows.lock();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].new_document_state["deleted"], json!(true));
        assert_eq!(rows[0].new_document_state["fromModifier"], json!(true));
        assert!(rows[0].new_document_state.get("_deleted").is_none());
        assert_eq!(
            rows[0].assumed_master_state.as_ref().unwrap()["deleted"],
            json!(false)
        );
        assert_eq!(conflicts[0]["_deleted"], json!(true));
        assert!(conflicts[0].get("deleted").is_none());
    }

    #[tokio::test]
    async fn master_write_wraps_handler_errors_with_push_envelope() {
        use futures::StreamExt;

        let push_calls = Arc::new(AtomicUsize::new(0));
        let push_calls_for_handler = Arc::clone(&push_calls);
        let push_handler: PushHandler = Arc::new(move |_rows| {
            let push_calls = Arc::clone(&push_calls_for_handler);
            Box::pin(async move {
                if push_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(new_rx_error("TEST_PUSH", Some(json!({ "attempt": 1 }))));
                }
                Ok(Vec::new())
            })
        });
        let error_relay = RxSubject::new();
        let mut errors = error_relay.subscribe();
        let handler = ClosureReplicationHandler {
            pull_handler: None,
            push_handler: Some(push_handler),
            stream_factory: None,
            pull_modifier: default_document_modifier(),
            push_modifier: default_document_modifier(),
            collection_schema: test_schema(),
            deleted_field: "_deleted".to_string(),
            retry_time: 0,
            canceled: RxBehaviorSubject::new(false),
            paused: RxBehaviorSubject::new(false),
            error_relay,
            remote_events: RxSubject::new(),
        };

        let conflicts = handler
            .master_write(vec![RxReplicationWriteToMasterRow {
                new_document_state: json!({ "id": "alice", "_deleted": false }),
                assumed_master_state: Some(json!({ "id": "alice", "_deleted": false })),
            }])
            .await
            .unwrap();
        let err = tokio::time::timeout(std::time::Duration::from_secs(1), errors.next())
            .await
            .unwrap()
            .unwrap();

        assert!(conflicts.is_empty());
        assert_eq!(push_calls.load(Ordering::SeqCst), 2);
        assert_eq!(err.code(), "RC_PUSH");
        assert_eq!(err.parameters()["direction"], json!("push"));
        assert_eq!(
            err.parameters()["pushRows"][0]["newDocumentState"]["id"],
            json!("alice")
        );
        assert_eq!(err.parameters()["errors"][0]["code"], json!("TEST_PUSH"));
        assert_eq!(err.parameters()["errors"][0]["rxdb"], json!(true));
    }

    #[tokio::test]
    async fn master_write_returns_empty_while_paused() {
        let push_calls = Arc::new(AtomicUsize::new(0));
        let push_calls_for_handler = Arc::clone(&push_calls);
        let push_handler: PushHandler = Arc::new(move |_rows| {
            let push_calls = Arc::clone(&push_calls_for_handler);
            Box::pin(async move {
                push_calls.fetch_add(1, Ordering::SeqCst);
                Ok(Vec::new())
            })
        });
        let handler = ClosureReplicationHandler {
            pull_handler: None,
            push_handler: Some(push_handler),
            stream_factory: None,
            pull_modifier: default_document_modifier(),
            push_modifier: default_document_modifier(),
            collection_schema: test_schema(),
            deleted_field: "_deleted".to_string(),
            retry_time: 0,
            canceled: RxBehaviorSubject::new(false),
            paused: RxBehaviorSubject::new(true),
            error_relay: RxSubject::new(),
            remote_events: RxSubject::new(),
        };

        let conflicts = handler
            .master_write(vec![RxReplicationWriteToMasterRow {
                new_document_state: json!({ "id": "alice", "_deleted": false }),
                assumed_master_state: None,
            }])
            .await
            .unwrap();

        assert!(conflicts.is_empty());
        assert_eq!(push_calls.load(Ordering::SeqCst), 0);
    }
}
