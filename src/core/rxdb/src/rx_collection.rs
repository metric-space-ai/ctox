//! Rust-native port of `src/rx-collection.ts`.
//!
//! The API keeps the upstream collection lifecycle and write/query surface, but
//! exposes observable fields as typed streams (`event_bulks`, `change_events`,
//! `insert_events`, `update_events`, `remove_events`, `checkpoint_stream`).
//! Plugin-only surfaces such as migration-schema stay as explicit
//! `PLUGIN_MISSING` stubs until CTOX ships those plugins.

use std::collections::{HashMap, HashSet};
use std::future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::{future::BoxFuture, stream, StreamExt};
use parking_lot::Mutex;
use serde_json::Value;
use tokio::sync::Mutex as TokioMutex;

use crate::change_event_buffer::{create_change_event_buffer, ChangeEventBuffer};
use crate::doc_cache::{map_documents_data_to_cache_docs, DocumentCache};
use crate::hooks::run_plugin_hooks;
use crate::incremental_write::IncrementalWriteQueue;
use crate::plugins::migration_schema::RxMigrationState;
use crate::plugins::utils::utils_error::plugin_missing;
use crate::plugins::utils::utils_time::now;
use crate::query_cache::{DEFAULT_TRY_TO_KEEP_MAX, DEFAULT_UNEXECUTED_LIFETIME_MS};
use crate::rx_collection_helper::{
    ensure_rx_collection_is_not_closed, fill_object_data_before_insert, remove_collection_storages,
};
use crate::rx_database::RxDatabase;
use crate::rx_document::{before_document_update_write, RxDocument};
use crate::rx_error::{new_rx_error, RxResult};
use crate::rx_query::{create_rx_query, RxQueryBase, RxQueryOp};
use crate::rx_schema::RxSchema;
use crate::rx_storage_helper::throw_if_is_storage_write_error;
use crate::rxjs_compat::RxStream;
use crate::types::{
    BulkWriteRow, EventBulk, RxStorageBulkWriteResponse, RxStorageChangeEvent, RxStorageWriteError,
};
use crate::types::{MangoQuery, RxConflictHandler, RxStorageInstance};

/// Close-callback list type.
pub type OnCloseCallback = Box<dyn FnOnce() + Send>;
pub type AwaitBeforeReadCallback = Arc<dyn Fn() -> BoxFuture<'static, RxResult<()>> + Send + Sync>;
pub type CollectionHookCallback = Arc<
    dyn Fn(Value, Option<Arc<RxDocument>>) -> BoxFuture<'static, RxResult<Value>> + Send + Sync,
>;

/// Business OS file/blob chunk collections are demand-only byte stores. They
/// must not arm eager collection-level change buffers because those buffers
/// force the SQLite external-poll path to deserialize large chunk stores even
/// when no live replication/read workload needs chunk change events.
pub(crate) fn is_demand_only_chunk_collection_name(name: &str) -> bool {
    matches!(
        name,
        "desktop_file_chunks" | "document_blob_chunks" | "spreadsheet_blob_chunks"
    )
}

#[derive(Clone)]
pub struct BulkDocumentWriteResult {
    pub success: Vec<Arc<RxDocument>>,
    pub error: Vec<RxStorageWriteError>,
}

/// Minimal stub of upstream `RxCollection`.
pub struct RxCollection {
    pub name: String,
    pub database: Arc<RxDatabase>,
    pub storage_instance: Arc<dyn RxStorageInstance>,
    pub conflict_handler: Arc<dyn RxConflictHandler>,
    pub schema: Option<Arc<RxSchema>>,
    pub doc_cache: Option<Arc<DocumentCache<RxDocument>>>,
    pub change_event_buffer: Option<Arc<ChangeEventBuffer>>,
    pub incremental_write_queue: Option<Arc<IncrementalWriteQueue>>,
    query_cache: Mutex<HashMap<String, Arc<RxQueryBase>>>,
    query_cache_replacement_running: AtomicBool,
    incremental_upsert_lock: TokioMutex<()>,
    await_before_reads: Mutex<Vec<AwaitBeforeReadCallback>>,
    hooks: Mutex<HashMap<(String, String), Vec<CollectionHookCallback>>>,
    on_close: Mutex<Vec<OnCloseCallback>>,
    on_remove: Mutex<Vec<OnCloseCallback>>,
    closed: AtomicBool,
}

impl RxCollection {
    pub fn new(
        name: impl Into<String>,
        database: Arc<RxDatabase>,
        storage_instance: Arc<dyn RxStorageInstance>,
        conflict_handler: Arc<dyn RxConflictHandler>,
    ) -> Arc<Self> {
        Arc::new(Self {
            name: name.into(),
            database,
            storage_instance,
            conflict_handler,
            schema: None,
            doc_cache: None,
            change_event_buffer: None,
            incremental_write_queue: None,
            query_cache: Mutex::new(HashMap::new()),
            query_cache_replacement_running: AtomicBool::new(false),
            incremental_upsert_lock: TokioMutex::new(()),
            await_before_reads: Mutex::new(Vec::new()),
            hooks: Mutex::new(HashMap::new()),
            on_close: Mutex::new(Vec::new()),
            on_remove: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
        })
    }

    pub fn new_with_schema(
        name: impl Into<String>,
        database: Arc<RxDatabase>,
        storage_instance: Arc<dyn RxStorageInstance>,
        conflict_handler: Arc<dyn RxConflictHandler>,
        schema: Arc<RxSchema>,
    ) -> Arc<Self> {
        let name = name.into();
        let primary_path = schema.primary_path.clone();

        Arc::new_cyclic(|weak_collection| {
            let weak_for_document_creator = weak_collection.clone();
            let document_creator = Arc::new(move |doc_data| {
                let collection = weak_for_document_creator
                    .upgrade()
                    .expect("RxCollection dropped while DocumentCache creates a document");
                RxDocument::new(collection, doc_data)
            });
            let doc_cache =
                DocumentCache::new_without_stream(primary_path.clone(), document_creator);
            let change_event_buffer = if is_demand_only_chunk_collection_name(&name) {
                None
            } else {
                Some(create_change_event_buffer(storage_instance.change_stream()))
            };
            let incremental_write_queue = IncrementalWriteQueue::new(
                Arc::clone(&storage_instance),
                primary_path.clone(),
                {
                    let weak_collection = weak_collection.clone();
                    Arc::new(move |mut new_data, old_data| {
                        let weak_collection = weak_collection.clone();
                        Box::pin(async move {
                            let collection = weak_collection.upgrade().ok_or_else(|| {
                                new_rx_error(
                                    "COL_INCREMENTAL_QUEUE",
                                    Some(serde_json::json!({
                                        "message": "collection dropped before incremental pre-write",
                                    })),
                                )
                            })?;
                            before_document_update_write(&collection, &mut new_data, &old_data)
                                .await?;
                            if collection.has_hooks("pre", "save") {
                                new_data =
                                    collection.run_hooks("pre", "save", new_data, None).await?;
                            }
                            Ok(new_data)
                        })
                    })
                },
                {
                    let weak_collection = weak_collection.clone();
                    Arc::new(move |doc| {
                        let weak_collection = weak_collection.clone();
                        Box::pin(async move {
                            if let Some(collection) = weak_collection.upgrade() {
                                if collection.has_hooks("post", "save") {
                                    collection.run_hooks("post", "save", doc, None).await?;
                                }
                            }
                            Ok(())
                        })
                    })
                },
            );

            Self {
                name,
                database,
                storage_instance,
                conflict_handler,
                schema: Some(schema),
                doc_cache: Some(doc_cache),
                change_event_buffer,
                incremental_write_queue: Some(incremental_write_queue),
                query_cache: Mutex::new(HashMap::new()),
                query_cache_replacement_running: AtomicBool::new(false),
                incremental_upsert_lock: TokioMutex::new(()),
                await_before_reads: Mutex::new(Vec::new()),
                hooks: Mutex::new(HashMap::new()),
                on_close: Mutex::new(Vec::new()),
                on_remove: Mutex::new(Vec::new()),
                closed: AtomicBool::new(false),
            }
        })
    }

    /// `collection.onClose.push(fn)` upstream is just an array of close-hooks.
    pub fn on_close_push(&self, cb: OnCloseCallback) {
        self.on_close.lock().push(cb);
    }

    pub fn on_remove_push(&self, cb: OnCloseCallback) {
        self.on_remove.lock().push(cb);
    }

    // ref: rxdb/src/rx-collection.ts awaitBeforeReads
    pub fn add_await_before_read(&self, cb: AwaitBeforeReadCallback) {
        self.await_before_reads.lock().push(cb);
    }

    pub fn remove_await_before_read(&self, cb: &AwaitBeforeReadCallback) {
        self.await_before_reads
            .lock()
            .retain(|existing| !Arc::ptr_eq(existing, cb));
    }

    pub async fn await_before_reads(&self) -> RxResult<()> {
        let callbacks = self.await_before_reads.lock().clone();
        for callback in callbacks {
            callback().await?;
        }
        Ok(())
    }

    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn ensure_not_closed(&self) -> RxResult<()> {
        ensure_rx_collection_is_not_closed(
            &self.name,
            self.schema
                .as_ref()
                .map(|schema| schema.version())
                .unwrap_or_default(),
            self.closed(),
        )
    }

    pub fn close(&self) {
        if self.closed.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        if let Some(buffer) = &self.change_event_buffer {
            buffer.close();
        }
        let cbs = std::mem::take(&mut *self.on_close.lock());
        for cb in cbs.into_iter() {
            cb();
        }
    }

    // ref: rxdb/src/rx-collection.ts remove
    pub async fn remove(self: &Arc<Self>) -> RxResult<()> {
        self.ensure_not_closed()?;
        let internal_store = self.database.internal_store.as_ref().ok_or_else(|| {
            new_rx_error(
                "COL_REMOVE_INTERNAL_STORE",
                Some(serde_json::json!({ "collection": self.name })),
            )
        })?;
        remove_collection_storages(
            &self.database.storage,
            internal_store,
            &self.database.token,
            &self.database.name,
            &self.name,
            self.database.multi_instance,
            self.database.password.as_deref(),
            Some(&self.database.hash_function),
        )
        .await?;
        self.close();
        let remove_callbacks = std::mem::take(&mut *self.on_remove.lock());
        for cb in remove_callbacks.into_iter() {
            cb();
        }
        self.database.collections.lock().remove(&self.name);
        Ok(())
    }

    pub fn primary_path(&self) -> Option<String> {
        self.schema
            .as_ref()
            .map(|schema| schema.primary_path.clone())
    }

    pub fn schema_required(&self) -> RxResult<&Arc<RxSchema>> {
        self.schema.as_ref().ok_or_else(|| {
            new_rx_error(
                "COL_SCHEMA",
                Some(serde_json::json!({ "collection": self.name })),
            )
        })
    }

    pub fn doc_cache(&self) -> crate::rx_error::RxResult<&Arc<DocumentCache<RxDocument>>> {
        self.doc_cache.as_ref().ok_or_else(|| {
            crate::rx_error::new_rx_error(
                "COL_DOC_CACHE",
                Some(serde_json::json!({ "collection": self.name })),
            )
        })
    }

    pub fn change_event_buffer(&self) -> crate::rx_error::RxResult<&Arc<ChangeEventBuffer>> {
        self.change_event_buffer.as_ref().ok_or_else(|| {
            crate::rx_error::new_rx_error(
                "COL_CHANGE_EVENT_BUFFER",
                Some(serde_json::json!({ "collection": self.name })),
            )
        })
    }

    pub fn event_bulks(&self) -> RxStream<EventBulk> {
        self.storage_instance.change_stream()
    }

    // ref: rxdb/src/rx-collection.ts checkpoint$
    pub fn checkpoint_stream(&self) -> RxStream<Option<Value>> {
        Box::pin(self.event_bulks().map(|bulk| bulk.checkpoint))
    }

    pub fn change_events(&self) -> RxStream<RxStorageChangeEvent> {
        Box::pin(
            self.event_bulks()
                .flat_map(|bulk| stream::iter(bulk.events)),
        )
    }

    pub fn insert_events(&self) -> RxStream<RxStorageChangeEvent> {
        Box::pin(
            self.change_events()
                .filter(|event| future::ready(event.operation == "INSERT")),
        )
    }

    pub fn update_events(&self) -> RxStream<RxStorageChangeEvent> {
        Box::pin(
            self.change_events()
                .filter(|event| future::ready(event.operation == "UPDATE")),
        )
    }

    pub fn remove_events(&self) -> RxStream<RxStorageChangeEvent> {
        Box::pin(
            self.change_events()
                .filter(|event| future::ready(event.operation == "DELETE")),
        )
    }

    // ref: rxdb/src/rx-collection.ts addHook / hasHooks / _runHooks
    pub fn add_hook(
        &self,
        when: impl Into<String>,
        key: impl Into<String>,
        hook: CollectionHookCallback,
    ) -> RxResult<()> {
        let when = when.into();
        let key = key.into();
        if when != "pre" && when != "post" {
            return Err(new_rx_error(
                "COL8",
                Some(serde_json::json!({ "when": when, "key": key })),
            ));
        }
        if !matches!(key.as_str(), "insert" | "save" | "remove" | "create") {
            return Err(new_rx_error(
                "COL9",
                Some(serde_json::json!({ "key": key })),
            ));
        }
        self.hooks.lock().entry((when, key)).or_default().push(hook);
        Ok(())
    }

    pub fn has_hooks(&self, when: &str, key: &str) -> bool {
        self.hooks
            .lock()
            .get(&(when.to_string(), key.to_string()))
            .is_some_and(|hooks| !hooks.is_empty())
    }

    pub(crate) async fn run_hooks(
        &self,
        when: &str,
        key: &str,
        mut data: Value,
        instance: Option<Arc<RxDocument>>,
    ) -> RxResult<Value> {
        let hooks = self
            .hooks
            .lock()
            .get(&(when.to_string(), key.to_string()))
            .cloned()
            .unwrap_or_default();
        for hook in hooks {
            data = hook(data, instance.clone()).await?;
        }
        Ok(data)
    }

    pub fn incremental_write_queue(
        &self,
    ) -> crate::rx_error::RxResult<&Arc<IncrementalWriteQueue>> {
        self.incremental_write_queue.as_ref().ok_or_else(|| {
            crate::rx_error::new_rx_error(
                "COL_INCREMENTAL_QUEUE",
                Some(serde_json::json!({ "collection": self.name })),
            )
        })
    }

    pub fn get_by_query_cache(self: &Arc<Self>, query: Arc<RxQueryBase>) -> Arc<RxQueryBase> {
        let Ok(key) = query.to_string_key() else {
            return query;
        };
        let cached = {
            let mut cache = self.query_cache.lock();
            if let Some(existing) = cache.get(&key) {
                Arc::clone(existing)
            } else {
                cache.insert(key, Arc::clone(&query));
                query
            }
        };
        self.trigger_query_cache_replacement();
        cached
    }

    pub fn query_cache_size(&self) -> usize {
        self.query_cache.lock().len()
    }

    pub fn run_query_cache_replacement(&self, try_to_keep_max: usize, unexecuted_lifetime_ms: f64) {
        let entries: Vec<(String, Arc<RxQueryBase>)> = {
            let cache = self.query_cache.lock();
            if cache.len() < try_to_keep_max {
                return;
            }
            cache
                .iter()
                .map(|(key, query)| (key.clone(), Arc::clone(query)))
                .collect()
        };
        let min_unexecuted = now() - unexecuted_lifetime_ms;
        let mut maybe_uncache = Vec::new();
        let mut remove_now = Vec::new();
        for (key, query) in entries {
            if query.subscriber_count() > 0 {
                continue;
            }
            if *query.last_ensure_equal.lock() == 0.0 && query.creation_time < min_unexecuted {
                remove_now.push((key, query));
            } else {
                maybe_uncache.push((key, query));
            }
        }

        self.uncache_queries(remove_now);

        if maybe_uncache.len() <= try_to_keep_max {
            return;
        }
        let must_uncache = maybe_uncache.len() - try_to_keep_max;
        maybe_uncache.sort_by(|a, b| {
            a.1.last_ensure_equal
                .lock()
                .partial_cmp(&b.1.last_ensure_equal.lock())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.uncache_queries(maybe_uncache.into_iter().take(must_uncache).collect());
    }

    fn uncache_queries(&self, queries: Vec<(String, Arc<RxQueryBase>)>) {
        if queries.is_empty() {
            return;
        }
        let mut cache = self.query_cache.lock();
        for (key, query) in queries {
            if cache
                .get(&key)
                .is_some_and(|cached| Arc::ptr_eq(cached, &query))
            {
                query.mark_uncached();
                cache.remove(&key);
            }
        }
    }

    pub fn trigger_query_cache_replacement(self: &Arc<Self>) {
        if self
            .query_cache_replacement_running
            .swap(true, Ordering::SeqCst)
        {
            return;
        }
        let collection = Arc::clone(self);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                tokio::task::yield_now().await;
                collection.run_query_cache_replacement(
                    DEFAULT_TRY_TO_KEEP_MAX,
                    DEFAULT_UNEXECUTED_LIFETIME_MS,
                );
                collection
                    .query_cache_replacement_running
                    .store(false, Ordering::SeqCst);
            });
        } else {
            self.run_query_cache_replacement(
                DEFAULT_TRY_TO_KEEP_MAX,
                DEFAULT_UNEXECUTED_LIFETIME_MS,
            );
            self.query_cache_replacement_running
                .store(false, Ordering::SeqCst);
        }
    }

    pub(crate) fn invalidate_query_cache(&self) {
        let queries: Vec<_> = self.query_cache.lock().values().cloned().collect();
        for query in queries {
            query.mark_dirty();
        }
    }

    async fn written_success_documents(
        &self,
        primary_path: &str,
        write_rows: &[BulkWriteRow],
        response: &RxStorageBulkWriteResponse,
    ) -> RxResult<Vec<Value>> {
        let error_ids: HashSet<&str> = response
            .error
            .iter()
            .map(|error| error.document_id.as_str())
            .collect();
        let mut success_ids = Vec::new();
        for row in write_rows {
            let Some(id) = row.document.get(primary_path).and_then(Value::as_str) else {
                continue;
            };
            if !error_ids.contains(id) {
                success_ids.push(id.to_string());
            }
        }
        if success_ids.is_empty() {
            return Ok(Vec::new());
        }

        let persisted = self
            .storage_instance
            .find_documents_by_id(&success_ids, true)
            .await?;
        let persisted_by_id: HashMap<String, Value> = persisted
            .into_iter()
            .filter_map(|doc| {
                let id = doc.get(primary_path)?.as_str()?.to_string();
                Some((id, doc))
            })
            .collect();

        Ok(success_ids
            .into_iter()
            .filter_map(|id| persisted_by_id.get(&id).cloned())
            .collect())
    }

    // ref: rxdb/src/rx-collection.ts insert
    pub async fn insert(self: &Arc<Self>, json: Value) -> RxResult<Arc<RxDocument>> {
        self.ensure_not_closed()?;
        let write_result = self.bulk_insert(vec![json.clone()]).await?;
        throw_if_is_storage_write_error(
            &self.name,
            json.get(&self.primary_path().unwrap_or_else(|| String::from("id")))
                .and_then(Value::as_str)
                .unwrap_or_default(),
            &json,
            write_result.error.first(),
        )?;
        write_result.success.into_iter().next().ok_or_else(|| {
            new_rx_error(
                "COL_INSERT_EMPTY",
                Some(serde_json::json!({ "collection": self.name })),
            )
        })
    }

    // ref: rxdb/src/rx-collection.ts insertIfNotExists
    pub async fn insert_if_not_exists(self: &Arc<Self>, json: Value) -> RxResult<Arc<RxDocument>> {
        let write_result = self.bulk_insert(vec![json]).await?;
        if let Some(error) = write_result.error.first() {
            if error.status == 409 {
                let doc_in_db = error.document_in_db.as_ref().ok_or_else(|| {
                    new_rx_error(
                        "COL_INSERT_CONFLICT",
                        Some(serde_json::json!({ "documentId": error.document_id })),
                    )
                })?;
                return self.doc_cache()?.get_cached_rx_document(doc_in_db);
            }
            return Err(new_rx_error(
                "COL_INSERT_ERROR",
                Some(serde_json::to_value(error).unwrap_or(Value::Null)),
            ));
        }
        write_result.success.into_iter().next().ok_or_else(|| {
            new_rx_error(
                "COL_INSERT_EMPTY",
                Some(serde_json::json!({ "collection": self.name })),
            )
        })
    }

    // ref: rxdb/src/rx-collection.ts bulkInsert
    pub async fn bulk_insert(
        self: &Arc<Self>,
        docs_data: Vec<Value>,
    ) -> RxResult<BulkDocumentWriteResult> {
        self.ensure_not_closed()?;
        if docs_data.is_empty() {
            return Ok(BulkDocumentWriteResult {
                success: Vec::new(),
                error: Vec::new(),
            });
        }

        let schema = self.schema_required()?;
        let primary_path = schema.primary_path.clone();
        let mut ids = HashSet::new();
        let mut insert_rows = Vec::with_capacity(docs_data.len());
        for doc_data in docs_data {
            let mut use_doc_data = fill_object_data_before_insert(schema, doc_data)?;
            if self.has_hooks("pre", "insert") {
                use_doc_data = self.run_hooks("pre", "insert", use_doc_data, None).await?;
            }
            let id = use_doc_data
                .get(&primary_path)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            ids.insert(id);
            insert_rows.push(BulkWriteRow {
                previous: None,
                document: use_doc_data,
            });
        }

        if ids.len() != insert_rows.len() {
            return Err(new_rx_error(
                "COL22",
                Some(serde_json::json!({
                    "collection": self.name,
                    "message": "duplicate primary keys in bulk_insert"
                })),
            ));
        }

        let results = self
            .storage_instance
            .bulk_write(insert_rows.clone(), "rx-collection-bulk-insert")
            .await?;
        let success_data = self
            .written_success_documents(&primary_path, &insert_rows, &results)
            .await?;
        let success = map_documents_data_to_cache_docs(self.doc_cache()?, &success_data)?;
        if self.has_hooks("post", "insert") {
            for (doc_data, document) in success_data.iter().zip(success.iter()) {
                self.run_hooks(
                    "post",
                    "insert",
                    doc_data.clone(),
                    Some(Arc::clone(document)),
                )
                .await?;
            }
        }

        Ok(BulkDocumentWriteResult {
            success,
            error: results.error,
        })
    }

    // ref: rxdb/src/rx-collection.ts bulkRemove
    pub async fn bulk_remove_by_ids(
        self: &Arc<Self>,
        ids: Vec<String>,
    ) -> RxResult<BulkDocumentWriteResult> {
        self.ensure_not_closed()?;
        if ids.is_empty() {
            return Ok(BulkDocumentWriteResult {
                success: Vec::new(),
                error: Vec::new(),
            });
        }
        let primary_path = self.schema_required()?.primary_path.clone();
        let docs = self
            .storage_instance
            .find_documents_by_id(&ids, false)
            .await?;
        let mut rows = Vec::with_capacity(docs.len());
        let mut previous_by_id = HashMap::new();
        let mut instance_by_id = HashMap::new();
        for doc in docs {
            let rx_document = self.doc_cache()?.get_cached_rx_document(&doc)?;
            let mut previous_doc = doc;
            if self.has_hooks("pre", "remove") {
                previous_doc = self
                    .run_hooks(
                        "pre",
                        "remove",
                        previous_doc,
                        Some(Arc::clone(&rx_document)),
                    )
                    .await?;
            }
            let Some(id) = previous_doc.get(&primary_path).and_then(Value::as_str) else {
                continue;
            };
            previous_by_id.insert(id.to_string(), previous_doc.clone());
            instance_by_id.insert(id.to_string(), rx_document);
            let mut write_doc = previous_doc.clone();
            if let Some(obj) = write_doc.as_object_mut() {
                obj.insert("_deleted".to_string(), Value::Bool(true));
            }
            rows.push(BulkWriteRow {
                previous: Some(previous_doc),
                document: write_doc,
            });
        }
        let results = self
            .storage_instance
            .bulk_write(rows.clone(), "rx-collection-bulk-remove")
            .await?;
        let success_data = self
            .written_success_documents(&primary_path, &rows, &results)
            .await?;
        let success = map_documents_data_to_cache_docs(self.doc_cache()?, &success_data)?;
        for (document, doc_data) in success.iter().zip(success_data.iter()) {
            document.replace_data(doc_data.clone());
        }
        if self.has_hooks("post", "remove") {
            for doc_data in success_data.iter() {
                let Some(id) = doc_data.get(&primary_path).and_then(Value::as_str) else {
                    continue;
                };
                self.run_hooks(
                    "post",
                    "remove",
                    previous_by_id
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| doc_data.clone()),
                    instance_by_id.get(id).cloned(),
                )
                .await?;
            }
        }
        Ok(BulkDocumentWriteResult {
            success,
            error: results.error,
        })
    }

    // ref: rxdb/src/rx-collection.ts bulkUpsert
    pub async fn bulk_upsert(
        self: &Arc<Self>,
        docs_data: Vec<Value>,
    ) -> RxResult<BulkDocumentWriteResult> {
        self.ensure_not_closed()?;
        let schema = self.schema_required()?;
        let primary_path = schema.primary_path.clone();
        let mut use_json_by_doc_id = HashMap::new();
        let mut insert_data = Vec::with_capacity(docs_data.len());
        for doc_data in docs_data {
            let use_json = fill_object_data_before_insert(schema, doc_data)?;
            let primary = use_json
                .get(&primary_path)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    new_rx_error(
                        "COL3",
                        Some(serde_json::json!({
                            "primaryPath": primary_path,
                            "data": use_json,
                        })),
                    )
                })?
                .to_string();
            use_json_by_doc_id.insert(primary, use_json.clone());
            insert_data.push(use_json);
        }

        let insert_result = self.bulk_insert(insert_data).await?;
        let mut success = insert_result.success;
        let mut error = Vec::new();
        let mut update_rows = Vec::new();
        for err in insert_result.error {
            if err.status != 409 {
                error.push(err);
                continue;
            }
            let Some(write_data) = use_json_by_doc_id.get(&err.document_id).cloned() else {
                error.push(err);
                continue;
            };
            let Some(doc_in_db) = err.document_in_db.clone() else {
                error.push(err);
                continue;
            };
            let mut document = doc_in_db.clone();
            if let (Some(document_obj), Some(write_obj)) =
                (document.as_object_mut(), write_data.as_object())
            {
                for (key, value) in write_obj {
                    if key == "_rev" || key == "_meta" || key == "_attachments" {
                        continue;
                    }
                    document_obj.insert(key.clone(), value.clone());
                }
            } else {
                document = write_data;
            }
            update_rows.push(BulkWriteRow {
                previous: Some(doc_in_db),
                document,
            });
        }

        if !update_rows.is_empty() {
            let update_result = self
                .storage_instance
                .bulk_write(update_rows.clone(), "rx-collection-bulk-upsert")
                .await?;
            let success_data = self
                .written_success_documents(&primary_path, &update_rows, &update_result)
                .await?;
            let updated_docs = map_documents_data_to_cache_docs(self.doc_cache()?, &success_data)?;
            for (document, doc_data) in updated_docs.iter().zip(success_data.iter()) {
                document.replace_data(doc_data.clone());
            }
            success.extend(updated_docs);
            error.extend(update_result.error);
        }

        Ok(BulkDocumentWriteResult { success, error })
    }

    // ref: rxdb/src/rx-collection.ts upsert
    pub async fn upsert(self: &Arc<Self>, json: Value) -> RxResult<Arc<RxDocument>> {
        self.ensure_not_closed()?;
        let primary_path = self.schema_required()?.primary_path.clone();
        let bulk_result = self.bulk_upsert(vec![json.clone()]).await?;
        throw_if_is_storage_write_error(
            &self.name,
            json.get(&primary_path)
                .and_then(Value::as_str)
                .unwrap_or_default(),
            &json,
            bulk_result.error.first(),
        )?;
        bulk_result.success.into_iter().next().ok_or_else(|| {
            new_rx_error(
                "COL_UPSERT_EMPTY",
                Some(serde_json::json!({ "collection": self.name })),
            )
        })
    }

    // ref: rxdb/src/rx-collection.ts incrementalUpsert
    pub async fn incremental_upsert(self: &Arc<Self>, json: Value) -> RxResult<Arc<RxDocument>> {
        self.ensure_not_closed()?;
        let _guard = self.incremental_upsert_lock.lock().await;
        let schema = self.schema_required()?;
        let primary_path = schema.primary_path.clone();
        let use_json = fill_object_data_before_insert(schema, json.clone())?;
        let primary = use_json
            .get(&primary_path)
            .and_then(Value::as_str)
            .filter(|primary| !primary.is_empty())
            .ok_or_else(|| {
                new_rx_error(
                    "COL4",
                    Some(serde_json::json!({
                        "data": json,
                    })),
                )
            })?
            .to_string();

        if let Some(doc_data) = self
            .doc_cache()?
            .get_latest_document_data_if_exists(&primary)
        {
            let doc = self.doc_cache()?.get_cached_rx_document(&doc_data)?;
            return doc
                .incremental_modify(Box::new(move |_inner_doc| {
                    let use_json = use_json.clone();
                    Box::pin(async move { Ok(use_json) })
                }))
                .await;
        }

        let existing = self
            .find_one(Some(MangoQuery {
                selector: Some(serde_json::json!({ primary_path: { "$eq": primary } })),
                ..Default::default()
            }))?
            .exec(false)
            .await?;
        if existing.is_null() {
            self.insert(use_json).await
        } else {
            let doc = self.doc_cache()?.get_cached_rx_document(&existing)?;
            doc.incremental_modify(Box::new(move |_inner_doc| {
                let use_json = use_json.clone();
                Box::pin(async move { Ok(use_json) })
            }))
            .await
        }
    }

    pub fn find(self: &Arc<Self>, query: Option<MangoQuery>) -> RxResult<Arc<RxQueryBase>> {
        self.ensure_not_closed()?;
        let query = self.prepare_rx_query(RxQueryOp::Find, query)?;
        Ok(create_rx_query(
            RxQueryOp::Find,
            query,
            Arc::clone(self),
            None,
        ))
    }

    pub fn find_one(self: &Arc<Self>, query: Option<MangoQuery>) -> RxResult<Arc<RxQueryBase>> {
        self.ensure_not_closed()?;
        let mut query = self.prepare_rx_query(RxQueryOp::FindOne, query)?;
        if query.limit.is_some() {
            return Err(new_rx_error("QU6", None));
        }
        query.limit = Some(1);
        Ok(create_rx_query(
            RxQueryOp::FindOne,
            query,
            Arc::clone(self),
            None,
        ))
    }

    pub fn find_by_ids(self: &Arc<Self>, ids: Vec<String>) -> RxResult<Arc<RxQueryBase>> {
        self.ensure_not_closed()?;
        let primary_path = self.primary_path().unwrap_or_else(|| "id".to_string());
        Ok(create_rx_query(
            RxQueryOp::FindByIds,
            MangoQuery {
                selector: Some(serde_json::json!({ primary_path: { "$in": ids } })),
                ..Default::default()
            },
            Arc::clone(self),
            None,
        ))
    }

    pub fn count(self: &Arc<Self>, query: Option<MangoQuery>) -> RxResult<Arc<RxQueryBase>> {
        self.ensure_not_closed()?;
        Ok(create_rx_query(
            RxQueryOp::Count,
            query.unwrap_or_default(),
            Arc::clone(self),
            None,
        ))
    }

    fn prepare_rx_query(
        self: &Arc<Self>,
        op: RxQueryOp,
        query: Option<MangoQuery>,
    ) -> RxResult<MangoQuery> {
        let mut payload = serde_json::json!({
            "op": op.as_str(),
            "queryObj": query
                .as_ref()
                .map(|query| serde_json::to_value(query).unwrap_or(Value::Null))
                .unwrap_or(Value::Null),
            "collection": self.name,
        });
        run_plugin_hooks("prePrepareRxQuery", &mut payload);
        let query_value = payload.get("queryObj").cloned().unwrap_or(Value::Null);
        if query_value.is_null() {
            return Ok(MangoQuery::default());
        }
        serde_json::from_value(query_value).map_err(|err| {
            new_rx_error(
                "COL_PREPARE_RX_QUERY",
                Some(serde_json::json!({
                    "collection": self.name,
                    "op": op.as_str(),
                    "message": err.to_string(),
                })),
            )
        })
    }

    // ref: rxdb/src/rx-collection.ts cleanup
    pub async fn cleanup(self: &Arc<Self>, minimum_deleted_time: Option<i64>) -> RxResult<bool> {
        self.ensure_not_closed()?;
        self.storage_instance
            .cleanup(minimum_deleted_time.unwrap_or(0))
            .await
    }

    // ref: rxdb/src/rx-collection.ts migrationNeeded/getMigrationState/startMigration/migratePromise
    pub async fn migration_needed(&self) -> RxResult<bool> {
        Err(plugin_missing_rx_error("migration-schema"))
    }

    pub fn get_migration_state(&self) -> RxResult<RxMigrationState> {
        Err(plugin_missing_rx_error("migration-schema"))
    }

    pub async fn start_migration(&self, _batch_size: Option<u64>) -> RxResult<()> {
        self.ensure_not_closed()?;
        Err(plugin_missing_rx_error("migration-schema"))
    }

    pub async fn migrate_promise(&self, _batch_size: Option<u64>) -> RxResult<Value> {
        Err(plugin_missing_rx_error("migration-schema"))
    }
}

fn plugin_missing_rx_error(plugin_key: &str) -> crate::rx_error::RxError {
    let err = plugin_missing(plugin_key);
    new_rx_error(
        "PLUGIN_MISSING",
        Some(serde_json::json!({
            "plugin": plugin_key,
            "message": err.to_string(),
        })),
    )
}

/// Test-only collection builders shared across crate test modules (e.g. the
/// replication-webrtc Phase 3 demux test needs to build multiple in-memory
/// collections behind one connection). Kept `pub(crate)` + `#[cfg(test)]` so
/// it never ships in a non-test build.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::collections::HashMap;

    use crate::plugins::storage_memory::get_rx_storage_memory;
    use crate::replication_protocol::default_conflict_handler::DefaultConflictHandler;
    use crate::rx_schema::create_rx_schema;
    use crate::types::{
        HashFunction, HashOutput, JsonSchema, PrimaryKey, RxJsonSchema,
        RxStorageInstanceCreationParams,
    };

    struct SupportHashFunction;

    impl HashFunction for SupportHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("hash:{input}") })
        }
    }

    fn raw_schema() -> RxJsonSchema {
        let mut properties = HashMap::new();
        properties.insert(
            "id".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
                max_length: Some(100),
                ..Default::default()
            },
        );
        properties.insert(
            "age".to_string(),
            JsonSchema {
                schema_type: Some("number".to_string()),
                ..Default::default()
            },
        );
        RxJsonSchema {
            version: 0,
            primary_key: PrimaryKey::Simple("id".to_string()),
            schema_type: "object".to_string(),
            properties,
            required: vec!["id".to_string()],
            indexes: vec![vec!["age".to_string()]],
            encrypted: Vec::new(),
            internal_indexes: Vec::new(),
            key_compression: false,
            attachments: None,
            additional_properties: false,
            extra: HashMap::new(),
        }
    }

    /// Build a standalone in-memory `RxCollection` with a fixed `{id, age}`
    /// schema. Each call uses its own database token so collections are
    /// independent (used to verify cross-collection isolation under multiplex).
    pub(crate) async fn test_collection_named(name: &str) -> Arc<RxCollection> {
        let hash_function = Arc::new(SupportHashFunction);
        let schema =
            Arc::new(create_rx_schema(raw_schema(), hash_function.clone(), false).unwrap());
        let storage = get_rx_storage_memory(());
        let raw_storage_instance = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: format!("db-token-{name}"),
                    database_name: format!("db-{name}"),
                    collection_name: name.to_string(),
                    schema: schema.json_schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let database = RxDatabase::new(
            &format!("db-{name}"),
            &format!("db-token-{name}"),
            &format!("storage-token-{name}"),
            false,
            hash_function,
            storage,
        );
        let storage_instance = crate::rx_storage_helper::get_wrapped_storage_instance(
            Arc::clone(&database),
            raw_storage_instance,
            schema.json_schema.clone(),
        );
        RxCollection::new_with_schema(
            name,
            database,
            storage_instance,
            Arc::new(DefaultConflictHandler),
            schema,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::plugins::storage_memory::get_rx_storage_memory;
    use crate::replication_protocol::default_conflict_handler::DefaultConflictHandler;
    use crate::rx_schema::create_rx_schema;
    use crate::types::{
        HashFunction, HashOutput, JsonSchema, PrimaryKey, RxJsonSchema,
        RxStorageInstanceCreationParams,
    };

    struct TestHashFunction;

    impl HashFunction for TestHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("hash:{input}") })
        }
    }

    fn raw_schema() -> RxJsonSchema {
        let mut properties = HashMap::new();
        properties.insert(
            "id".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
                max_length: Some(100),
                ..Default::default()
            },
        );
        properties.insert(
            "age".to_string(),
            JsonSchema {
                schema_type: Some("number".to_string()),
                ..Default::default()
            },
        );
        RxJsonSchema {
            version: 0,
            primary_key: PrimaryKey::Simple("id".to_string()),
            schema_type: "object".to_string(),
            properties,
            required: vec!["id".to_string()],
            indexes: vec![vec!["age".to_string()]],
            encrypted: Vec::new(),
            internal_indexes: Vec::new(),
            key_compression: false,
            attachments: None,
            additional_properties: false,
            extra: HashMap::new(),
        }
    }

    async fn test_collection() -> Arc<RxCollection> {
        test_collection_named("docs").await
    }

    async fn test_collection_named(name: &str) -> Arc<RxCollection> {
        let hash_function = Arc::new(TestHashFunction);
        let schema =
            Arc::new(create_rx_schema(raw_schema(), hash_function.clone(), false).unwrap());
        let storage = get_rx_storage_memory(());
        let raw_storage_instance = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db".to_string(),
                    collection_name: name.to_string(),
                    schema: schema.json_schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let database = RxDatabase::new(
            "db",
            "db-token",
            "storage-token",
            false,
            hash_function,
            storage,
        );
        let storage_instance = crate::rx_storage_helper::get_wrapped_storage_instance(
            Arc::clone(&database),
            raw_storage_instance,
            schema.json_schema.clone(),
        );
        RxCollection::new_with_schema(
            name,
            database,
            storage_instance,
            Arc::new(DefaultConflictHandler),
            schema,
        )
    }

    #[tokio::test]
    async fn insert_upsert_query_and_remove_round_trip() {
        let collection = test_collection().await;
        let inserted = collection
            .insert(serde_json::json!({ "id": "a", "age": 1 }))
            .await
            .unwrap();
        assert_eq!(inserted.primary().unwrap(), "a");

        let upserted = collection
            .upsert(serde_json::json!({ "id": "a", "age": 2 }))
            .await
            .unwrap();
        assert_eq!(upserted.get("age").unwrap(), serde_json::json!(2));

        upserted
            .patch(serde_json::json!({ "age": 5 }))
            .await
            .unwrap();
        assert_eq!(upserted.get("age").unwrap(), serde_json::json!(5));

        let found = collection
            .find_one(Some(MangoQuery {
                selector: Some(serde_json::json!({ "id": { "$eq": "a" } })),
                ..Default::default()
            }))
            .unwrap()
            .exec(true)
            .await
            .unwrap();
        assert_eq!(found.get("age").and_then(Value::as_i64), Some(5));

        let removed = collection
            .bulk_remove_by_ids(vec!["a".to_string()])
            .await
            .unwrap();
        assert_eq!(removed.success.len(), 1);
        assert!(removed.success[0].deleted());

        let count = collection.count(None).unwrap().exec(false).await.unwrap();
        assert_eq!(count, serde_json::json!(0));
    }

    #[tokio::test]
    async fn insert_runs_collection_pre_and_post_hooks() {
        let collection = test_collection().await;
        let post_calls = Arc::new(AtomicUsize::new(0));
        collection
            .add_hook(
                "pre",
                "insert",
                Arc::new(|mut doc_data, _instance| {
                    Box::pin(async move {
                        if let Some(obj) = doc_data.as_object_mut() {
                            obj.insert("age".to_string(), serde_json::json!(5));
                        }
                        Ok(doc_data)
                    })
                }),
            )
            .unwrap();
        collection
            .add_hook("post", "insert", {
                let post_calls = Arc::clone(&post_calls);
                Arc::new(move |doc_data, instance| {
                    let post_calls = Arc::clone(&post_calls);
                    Box::pin(async move {
                        assert_eq!(doc_data.get("age"), Some(&serde_json::json!(5)));
                        assert_eq!(
                            instance
                                .as_ref()
                                .and_then(|document| document.primary().ok())
                                .as_deref(),
                            Some("a")
                        );
                        post_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(doc_data)
                    })
                })
            })
            .unwrap();

        let inserted = collection
            .insert(serde_json::json!({ "id": "a", "age": 1 }))
            .await
            .unwrap();

        assert_eq!(inserted.get("age").unwrap(), serde_json::json!(5));
        assert_eq!(post_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn bulk_remove_runs_collection_pre_and_post_hooks() {
        let collection = test_collection().await;
        collection
            .insert(serde_json::json!({ "id": "a", "age": 1 }))
            .await
            .unwrap();
        let post_calls = Arc::new(AtomicUsize::new(0));
        collection
            .add_hook(
                "pre",
                "remove",
                Arc::new(|mut doc_data, instance| {
                    Box::pin(async move {
                        assert_eq!(
                            instance
                                .as_ref()
                                .and_then(|document| document.primary().ok())
                                .as_deref(),
                            Some("a")
                        );
                        if let Some(obj) = doc_data.as_object_mut() {
                            obj.insert("age".to_string(), serde_json::json!(9));
                        }
                        Ok(doc_data)
                    })
                }),
            )
            .unwrap();
        collection
            .add_hook("post", "remove", {
                let post_calls = Arc::clone(&post_calls);
                Arc::new(move |doc_data, instance| {
                    let post_calls = Arc::clone(&post_calls);
                    Box::pin(async move {
                        assert_eq!(doc_data.get("age"), Some(&serde_json::json!(9)));
                        assert_eq!(
                            instance
                                .as_ref()
                                .and_then(|document| document.primary().ok())
                                .as_deref(),
                            Some("a")
                        );
                        post_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(doc_data)
                    })
                })
            })
            .unwrap();

        let removed = collection
            .bulk_remove_by_ids(vec!["a".to_string()])
            .await
            .unwrap();

        assert_eq!(removed.success.len(), 1);
        assert!(removed.success[0].deleted());
        assert_eq!(removed.success[0].get("age").unwrap(), serde_json::json!(9));
        assert_eq!(post_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn incremental_upsert_inserts_then_updates_existing_document() {
        let collection = test_collection().await;
        let inserted = collection
            .incremental_upsert(serde_json::json!({ "id": "a", "age": 1 }))
            .await
            .unwrap();
        assert_eq!(inserted.get("age").unwrap(), serde_json::json!(1));

        let updated = collection
            .incremental_upsert(serde_json::json!({ "id": "a", "age": 7 }))
            .await
            .unwrap();
        assert_eq!(updated.get("age").unwrap(), serde_json::json!(7));
        assert_eq!(inserted.get("age").unwrap(), serde_json::json!(7));

        let found = collection
            .find_one(Some(MangoQuery {
                selector: Some(serde_json::json!({ "id": { "$eq": "a" } })),
                ..Default::default()
            }))
            .unwrap()
            .exec(true)
            .await
            .unwrap();
        assert_eq!(found.get("age"), Some(&serde_json::json!(7)));
    }

    #[tokio::test]
    async fn find_one_forces_limit_and_rejects_caller_limit() {
        let collection = test_collection().await;
        let query = collection
            .find_one(Some(MangoQuery {
                selector: Some(serde_json::json!({ "age": { "$gte": 1 } })),
                ..Default::default()
            }))
            .unwrap();
        let prepared = query.get_prepared_query().unwrap();
        assert_eq!(
            prepared
                .get("query")
                .and_then(|query| query.get("limit"))
                .and_then(Value::as_u64),
            Some(1)
        );

        let err = match collection.find_one(Some(MangoQuery {
            selector: Some(serde_json::json!({})),
            limit: Some(2),
            ..Default::default()
        })) {
            Ok(_) => panic!("find_one must reject caller-provided limit"),
            Err(err) => err,
        };
        assert_eq!(err.code(), "QU6");
    }

    #[tokio::test]
    async fn find_runs_pre_prepare_rx_query_hook_mutations() {
        let collection = test_collection_named("hook_docs").await;
        collection
            .bulk_insert(vec![
                serde_json::json!({ "id": "a", "age": 1 }),
                serde_json::json!({ "id": "b", "age": 2 }),
            ])
            .await
            .unwrap();

        let hook = crate::hooks::Hook::Sync(Arc::new(|payload| {
            if payload.get("op").and_then(Value::as_str) != Some("find") {
                return;
            }
            let is_marker_query = payload
                .get("queryObj")
                .and_then(|query| query.get("selector"))
                .and_then(|selector| selector.get("__hook_marker"))
                .and_then(Value::as_bool)
                == Some(true);
            if !is_marker_query {
                return;
            }
            payload["queryObj"] = serde_json::json!({
                "selector": { "id": { "$eq": "b" } }
            });
        }));
        crate::hooks::push_hook("prePrepareRxQuery", hook.clone());

        let query = collection
            .find(Some(MangoQuery {
                selector: Some(serde_json::json!({ "__hook_marker": true })),
                ..Default::default()
            }))
            .unwrap();
        assert_eq!(
            query.mango_query.selector,
            Some(serde_json::json!({ "id": { "$eq": "b" } }))
        );
        let result = query.exec(false).await.unwrap();
        crate::hooks::clear_hook("prePrepareRxQuery", &hook);

        let docs = result.as_array().unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].get("id").and_then(Value::as_str), Some("b"));
    }

    #[tokio::test]
    async fn query_factories_reject_closed_collection() {
        let collection = test_collection().await;
        collection.close();

        let find_err = match collection.find(None) {
            Ok(_) => panic!("find must reject closed collection"),
            Err(err) => err,
        };
        let find_one_err = match collection.find_one(None) {
            Ok(_) => panic!("find_one must reject closed collection"),
            Err(err) => err,
        };
        let find_by_ids_err = match collection.find_by_ids(vec!["a".to_string()]) {
            Ok(_) => panic!("find_by_ids must reject closed collection"),
            Err(err) => err,
        };
        let count_err = match collection.count(None) {
            Ok(_) => panic!("count must reject closed collection"),
            Err(err) => err,
        };

        assert_eq!(find_err.code(), "COL21");
        assert_eq!(find_one_err.code(), "COL21");
        assert_eq!(find_by_ids_err.code(), "COL21");
        assert_eq!(count_err.code(), "COL21");
    }

    #[tokio::test]
    async fn cleanup_delegates_to_storage_instance() {
        let collection = test_collection().await;
        collection
            .insert(serde_json::json!({ "id": "a", "age": 1 }))
            .await
            .unwrap();
        let removed = collection
            .bulk_remove_by_ids(vec!["a".to_string()])
            .await
            .unwrap();
        assert_eq!(removed.success.len(), 1);

        assert!(collection.cleanup(Some(0)).await.unwrap());

        let remaining = collection
            .storage_instance
            .find_documents_by_id(&["a".to_string()], true)
            .await
            .unwrap();
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn migration_schema_methods_return_plugin_missing() {
        let collection = test_collection().await;

        assert_eq!(
            collection.migration_needed().await.unwrap_err().code(),
            "PLUGIN_MISSING"
        );
        assert_eq!(
            collection.get_migration_state().unwrap_err().code(),
            "PLUGIN_MISSING"
        );
        assert_eq!(
            collection
                .start_migration(Some(10))
                .await
                .unwrap_err()
                .code(),
            "PLUGIN_MISSING"
        );
        assert_eq!(
            collection
                .migrate_promise(Some(10))
                .await
                .unwrap_err()
                .code(),
            "PLUGIN_MISSING"
        );
    }

    #[tokio::test]
    async fn bulk_insert_rejects_duplicate_primary_keys() {
        let collection = test_collection().await;
        let err = match collection
            .bulk_insert(vec![
                serde_json::json!({ "id": "a", "age": 1 }),
                serde_json::json!({ "id": "a", "age": 2 }),
            ])
            .await
        {
            Ok(_) => panic!("duplicate primary keys should fail"),
            Err(err) => err,
        };
        assert_eq!(err.code(), "COL22");
    }

    #[tokio::test]
    async fn change_event_buffer_tracks_storage_events() {
        let collection = test_collection().await;
        let buffer = Arc::clone(collection.change_event_buffer().unwrap());

        collection
            .insert(serde_json::json!({ "id": "a", "age": 1 }))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(buffer.get_counter() >= 1);
        assert!(buffer.get_buffer().iter().any(|event| {
            event.document_data.as_ref().and_then(|doc| doc.get("id"))
                == Some(&serde_json::json!("a"))
        }));
    }

    #[tokio::test]
    async fn demand_only_chunk_collections_skip_eager_change_event_buffer() {
        let collection = test_collection_named("desktop_file_chunks").await;

        assert!(collection.change_event_buffer.is_none());
        let err = match collection.change_event_buffer() {
            Ok(_) => panic!("chunk collections must not create an eager change buffer"),
            Err(err) => err,
        };
        assert_eq!(err.code(), "COL_CHANGE_EVENT_BUFFER");

        let mut events = collection.event_bulks();
        collection
            .insert(serde_json::json!({ "id": "chunk-a", "age": 1 }))
            .await
            .unwrap();

        let event_bulk = tokio::time::timeout(std::time::Duration::from_secs(1), events.next())
            .await
            .unwrap()
            .unwrap();
        assert!(event_bulk
            .events
            .iter()
            .any(|event| event.document_id == "chunk-a"));
    }

    #[tokio::test]
    async fn event_bulks_exposes_collection_change_stream() {
        let collection = test_collection().await;
        let mut events = collection.event_bulks();

        collection
            .insert(serde_json::json!({ "id": "a", "age": 1 }))
            .await
            .unwrap();

        let event_bulk = tokio::time::timeout(std::time::Duration::from_secs(1), events.next())
            .await
            .unwrap()
            .unwrap();
        assert!(event_bulk
            .events
            .iter()
            .any(|event| event.document_id == "a"));
    }

    #[tokio::test]
    async fn checkpoint_stream_maps_collection_change_stream_checkpoints() {
        let collection = test_collection().await;
        let mut checkpoints = collection.checkpoint_stream();

        collection
            .insert(serde_json::json!({ "id": "a", "age": 1 }))
            .await
            .unwrap();

        let checkpoint =
            tokio::time::timeout(std::time::Duration::from_secs(1), checkpoints.next())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            checkpoint.and_then(|checkpoint| checkpoint.get("id").cloned()),
            Some(serde_json::json!("a"))
        );
    }

    #[tokio::test]
    async fn typed_change_event_streams_filter_by_operation() {
        let collection = test_collection().await;
        let mut inserts = collection.insert_events();
        let mut updates = collection.update_events();
        let mut removes = collection.remove_events();

        collection
            .insert(serde_json::json!({ "id": "a", "age": 1 }))
            .await
            .unwrap();
        collection
            .upsert(serde_json::json!({ "id": "a", "age": 2 }))
            .await
            .unwrap();
        collection
            .bulk_remove_by_ids(vec!["a".to_string()])
            .await
            .unwrap();

        let insert = tokio::time::timeout(std::time::Duration::from_secs(1), inserts.next())
            .await
            .unwrap()
            .unwrap();
        let update = tokio::time::timeout(std::time::Duration::from_secs(1), updates.next())
            .await
            .unwrap()
            .unwrap();
        let remove = tokio::time::timeout(std::time::Duration::from_secs(1), removes.next())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(insert.operation, "INSERT");
        assert_eq!(update.operation, "UPDATE");
        assert_eq!(remove.operation, "DELETE");
        assert_eq!(insert.document_id, "a");
        assert_eq!(update.document_id, "a");
        assert_eq!(remove.document_id, "a");
    }
}
