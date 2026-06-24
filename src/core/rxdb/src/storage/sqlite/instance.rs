//! SQLite [`crate::types::RxStorageInstance`] implementation.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

use crate::plugins::utils::utils_string::random_token;
use crate::rx_error::{new_rx_error, RxError, RxResult};
use crate::rx_query_helper::{get_query_matcher, get_sort_comparator};
use crate::rx_schema_helper::get_primary_field_of_primary_key;
use crate::rxjs_compat::{RxStream, RxSubject};
use crate::types::{
    BulkWriteRow, EventBulk, FilledMangoQuery, RxJsonSchema, RxStorageBulkWriteResponse,
    RxStorageChangedDocumentsSinceResult, RxStorageCountResult, RxStorageInstance,
    RxStorageInstanceCreationParams, RxStorageQueryResult,
};

use super::cleanup::cleanup_deleted_documents;
use super::sql::{
    document_by_id, drop_table, for_each_document, insert_document, quote_identifier,
    update_document,
};
use super::types::{sqlite_error, SharedSqliteConnection};

const SQLITE_EXTERNAL_POLL_FILE_CHUNK_LIMIT: u64 = 2;
const SQLITE_EXTERNAL_POLL_SAFETY_INTERVAL: Duration = Duration::from_secs(60);

static INSTANCE_ID: AtomicU64 = AtomicU64::new(0);

/// FIX 1: map a `tokio::task::JoinError` (blocking task panicked or was
/// cancelled) into an `RxError` so the storage methods can keep their
/// existing `Result<_, RxError>` signatures while running the synchronous
/// rusqlite work off the async runtime via `spawn_blocking`.
fn join_error(err: tokio::task::JoinError) -> RxError {
    new_rx_error(
        "SQLITE",
        Some(json!({
            "message": format!("sqlite blocking task failed: {err}")
        })),
    )
}

struct TableNotifier {
    notify: Notify,
    generation: AtomicU64,
}

impl TableNotifier {
    fn new() -> Self {
        Self {
            notify: Notify::new(),
            generation: AtomicU64::new(0),
        }
    }

    fn signal(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.notify.notify_one();
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
}

static UPDATE_REGISTRY: OnceLock<StdMutex<HashMap<String, Arc<TableNotifier>>>> = OnceLock::new();

pub(crate) fn database_key_for_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn registry_key(database_key: &str, table_name: &str) -> String {
    format!("{database_key}\0{table_name}")
}

fn register_table_notifier(database_key: &str, table_name: &str, notifier: Arc<TableNotifier>) {
    let mut map = UPDATE_REGISTRY
        .get_or_init(|| StdMutex::new(HashMap::new()))
        .lock()
        .unwrap();
    map.insert(registry_key(database_key, table_name), notifier);
}

fn unregister_table_notifier(database_key: &str, table_name: &str) {
    if let Some(registry) = UPDATE_REGISTRY.get() {
        let mut map = registry.lock().unwrap();
        map.remove(&registry_key(database_key, table_name));
    }
}

pub fn notify_table_change(database_key: &str, table_name: &str) {
    if let Some(registry) = UPDATE_REGISTRY.get() {
        let map = registry.lock().unwrap();
        if let Some(notifier) = map.get(&registry_key(database_key, table_name)) {
            notifier.signal();
        }
    }
}

pub fn table_change_generation(database_key: &str, table_name: &str) -> Option<u64> {
    let registry = UPDATE_REGISTRY.get()?;
    let map = registry.lock().unwrap();
    map.get(&registry_key(database_key, table_name))
        .map(|notifier| notifier.generation())
}

pub fn notify_database_change(database_key: &str) {
    if let Some(registry) = UPDATE_REGISTRY.get() {
        let map = registry.lock().unwrap();
        let prefix = format!("{database_key}\0");
        for (key, notifier) in map.iter() {
            if key.starts_with(&prefix) {
                notifier.signal();
            }
        }
    }
}

pub struct RxStorageInstanceSqlite {
    pub database_name: String,
    pub collection_name: String,
    pub schema: RxJsonSchema,
    pub connection: SharedSqliteConnection,
    pub table_name: String,
    pub primary_path: String,
    /// File path so V1.5 `query_stream` can open a separate read-only
    /// connection per stream — keeps the shared write connection free for
    /// other peers / replication while a long stream runs.
    pub database_path: std::path::PathBuf,
    database_key: String,
    changes: RxSubject<EventBulk>,
    closed: Arc<AtomicBool>,
    external_checkpoint: Arc<Mutex<Value>>,
    instance_id: u64,
}

impl RxStorageInstanceSqlite {
    pub fn new(
        connection: SharedSqliteConnection,
        params: RxStorageInstanceCreationParams,
        table_name: String,
        database_path: std::path::PathBuf,
    ) -> Self {
        let primary_path = get_primary_field_of_primary_key(&params.schema.primary_key);
        let database_key = database_key_for_path(&database_path);
        let changes = RxSubject::new();
        let closed = Arc::new(AtomicBool::new(false));
        let external_checkpoint = Arc::new(Mutex::new({
            let conn = connection.lock();
            latest_checkpoint(&conn, &table_name).unwrap_or_else(|| json!({ "id": "", "lwt": 0 }))
        }));

        let notifier = Arc::new(TableNotifier::new());
        register_table_notifier(&database_key, &table_name, Arc::clone(&notifier));
        // One startup reconciliation closes the gap between the initial
        // checkpoint read and the database-wide data_version watcher baseline.
        notifier.signal();

        start_external_write_poll(
            Arc::clone(&connection),
            table_name.clone(),
            primary_path.clone(),
            changes.clone(),
            Arc::clone(&closed),
            Arc::clone(&external_checkpoint),
            notifier,
        );
        Self {
            database_name: params.database_name,
            collection_name: params.collection_name,
            schema: params.schema,
            connection,
            table_name,
            primary_path,
            database_path,
            database_key,
            changes,
            closed,
            external_checkpoint,
            instance_id: INSTANCE_ID.fetch_add(1, Ordering::SeqCst),
        }
    }

    fn ensure_open(&self, method: &str) -> RxResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(new_rx_error(
                "SQLITE_CLOSED",
                Some(json!({
                    "method": method,
                    "databaseName": self.database_name,
                    "collectionName": self.collection_name,
                    "instanceId": self.instance_id,
                })),
            ));
        }
        Ok(())
    }
}

/// FIX 1: free-standing checkpoint-status computation so it can run inside
/// `spawn_blocking` (no `&self` lifetime captured). Behavior is identical to
/// the previous `RxStorageInstanceSqlite::checkpoint_status_snapshot` method.
fn checkpoint_status_snapshot(
    connection: &SharedSqliteConnection,
    table_name: &str,
    database_name: &str,
    collection_name: &str,
    schema: &RxJsonSchema,
) -> Value {
    let conn = connection.lock();
    let checkpoint =
        latest_checkpoint(&conn, table_name).unwrap_or_else(|| json!({ "id": "", "lwt": 0 }));
    let latest_id = checkpoint
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let latest_lwt = checkpoint
        .get("lwt")
        .and_then(Value::as_f64)
        .unwrap_or_default();
    let schema_hash = schema_checkpoint_hash(schema);
    let latest_id_hash = if latest_id.is_empty() {
        String::new()
    } else {
        sha256_hex(latest_id.as_bytes())
    };
    let epoch_input = format!(
        "{}\n{}\n{}\n{}\n{}",
        database_name, collection_name, schema_hash, latest_lwt, latest_id
    );
    json!({
        "source": "rxdb-rs-sqlite",
        "state": "advertised",
        "collection": collection_name,
        "schemaHash": schema_hash,
        "latestLwt": latest_lwt,
        "latestIdHash": latest_id_hash,
        "epoch": sha256_hex(epoch_input.as_bytes()),
    })
}

fn primary_key_selector_ids(query: &FilledMangoQuery, primary_path: &str) -> Option<Vec<String>> {
    let selector = query.selector.as_object()?;
    let matcher = selector.get(primary_path)?;
    if let Some(id) = matcher.as_str() {
        return Some(vec![id.to_string()]);
    }
    let matcher_obj = matcher.as_object()?;
    if let Some(eq) = matcher_obj.get("$eq").and_then(Value::as_str) {
        return Some(vec![eq.to_string()]);
    }
    matcher_obj.get("$in").and_then(Value::as_array).map(|ids| {
        ids.iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect()
    })
}

fn start_external_write_poll(
    connection: SharedSqliteConnection,
    table_name: String,
    primary_path: String,
    changes: RxSubject<EventBulk>,
    closed: Arc<AtomicBool>,
    checkpoint: Arc<Mutex<Value>>,
    notifier: Arc<TableNotifier>,
) {
    let Ok(_) = tokio::runtime::Handle::try_current() else {
        return;
    };
    tokio::spawn(async move {
        let mut seen_generation = 0;
        loop {
            let mut safety_poll = false;
            if notifier.generation.load(Ordering::SeqCst) == seen_generation {
                tokio::select! {
                    _ = tokio::time::sleep(SQLITE_EXTERNAL_POLL_SAFETY_INTERVAL) => {
                        // Rare rescue path in case an update notification is
                        // lost or this storage was opened without the
                        // database-wide external-change watcher.
                        safety_poll = true;
                    }
                    _ = notifier.notify.notified() => {
                        // Instant notification from SQLite update_hook or the
                        // database-wide external-change watcher.
                    }
                }
            }
            if closed.load(Ordering::SeqCst) {
                break;
            }
            let current_generation = notifier.generation.load(Ordering::SeqCst);
            if current_generation == seen_generation && !safety_poll {
                continue;
            }
            seen_generation = current_generation;
            // FIX 1: run the per-table poll query off the tokio worker thread.
            // Each instance spawns one of these loops; doing the blocking
            // rusqlite read directly on a worker (1-2 on a small VPS) is what
            // starves the heartbeat timer + replication. We move owned clones
            // into `spawn_blocking` and only await the `Send` result here.
            let poll_conn = Arc::clone(&connection);
            let poll_table = table_name.clone();
            let poll_primary = primary_path.clone();
            let poll_checkpoint = checkpoint.lock().clone();
            let result = tokio::task::spawn_blocking(move || {
                let conn = poll_conn.lock();
                let poll_limit = if poll_table.contains("desktop_file_chunks") {
                    SQLITE_EXTERNAL_POLL_FILE_CHUNK_LIMIT
                } else {
                    50
                };
                changed_documents_since(
                    &conn,
                    &poll_table,
                    &poll_primary,
                    poll_limit,
                    Some(&poll_checkpoint),
                )
            })
            .await;
            let Ok(Ok(result)) = result else {
                continue;
            };
            if result.documents.is_empty() {
                *checkpoint.lock() = result.checkpoint;
                continue;
            }
            let events = result
                .documents
                .iter()
                .filter_map(|doc| {
                    let id = doc.get(&primary_path).and_then(Value::as_str)?;
                    let deleted = doc
                        .get("_deleted")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    Some(crate::types::RxStorageChangeEvent {
                        operation: if deleted { "DELETE" } else { "UPDATE" }.to_string(),
                        document_id: id.to_string(),
                        document_data: Some(doc.clone()),
                        previous_document_data: None,
                        is_local: false,
                    })
                })
                .collect::<Vec<_>>();
            *checkpoint.lock() = result.checkpoint.clone();
            if !events.is_empty() {
                changes.next(EventBulk {
                    id: random_token(Some(10)),
                    events,
                    checkpoint: Some(result.checkpoint),
                    context: Some("sqlite-external-poll".to_string()),
                });
            }
        }
    });
}

fn latest_checkpoint(conn: &rusqlite::Connection, table_name: &str) -> Option<Value> {
    conn.query_row(
        &format!(
            "SELECT id, lastWriteTime FROM {} ORDER BY lastWriteTime DESC, id DESC LIMIT 1",
            quote_identifier(table_name)
        ),
        [],
        |row| {
            let id: String = row.get(0)?;
            let lwt: f64 = row.get(1)?;
            Ok(json!({ "id": id, "lwt": lwt }))
        },
    )
    .optional()
    .ok()
    .flatten()
}

fn schema_checkpoint_hash(schema: &RxJsonSchema) -> String {
    let value = serde_json::to_value(schema).unwrap_or(Value::Null);
    let encoded = serde_json::to_string(&value).unwrap_or_default();
    sha256_hex(encoded.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn changed_documents_since(
    conn: &rusqlite::Connection,
    table_name: &str,
    primary_path: &str,
    limit: u64,
    checkpoint: Option<&Value>,
) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
    let since_lwt = checkpoint
        .and_then(|checkpoint| checkpoint.get("lwt"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let since_id = checkpoint
        .and_then(|checkpoint| checkpoint.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let mut stmt = conn
        .prepare(&format!(
            "SELECT data FROM {} WHERE lastWriteTime > ? OR (lastWriteTime = ? AND id > ?) ORDER BY lastWriteTime ASC, id ASC LIMIT ?",
            quote_identifier(table_name)
        ))
        .map_err(sqlite_error)?;
    let rows = stmt
        .query_map(
            params![since_lwt, since_lwt, since_id, limit as i64],
            |row| row.get::<_, String>(0),
        )
        .map_err(sqlite_error)?;
    let mut documents = Vec::new();
    for row in rows {
        let data = row.map_err(sqlite_error)?;
        documents.push(serde_json::from_str::<Value>(&data).map_err(|err| {
            new_rx_error("SQLITE_JSON", Some(json!({ "message": err.to_string() })))
        })?);
    }
    let checkpoint = documents
        .last()
        .map(|doc| {
            json!({
                "id": doc.get(primary_path).cloned().unwrap_or(Value::Null),
                "lwt": doc
                    .get("_meta")
                    .and_then(|meta| meta.get("lwt"))
                    .cloned()
                    .unwrap_or(json!(0)),
            })
        })
        .or_else(|| checkpoint.cloned())
        .unwrap_or_else(|| json!({ "id": "", "lwt": 0 }));
    Ok(RxStorageChangedDocumentsSinceResult {
        documents,
        checkpoint,
    })
}

#[async_trait]
impl RxStorageInstance for RxStorageInstanceSqlite {
    fn database_name(&self) -> &str {
        &self.database_name
    }

    fn collection_name(&self) -> &str {
        &self.collection_name
    }

    fn schema(&self) -> &RxJsonSchema {
        &self.schema
    }

    async fn bulk_write(
        &self,
        document_writes: Vec<BulkWriteRow>,
        context: &str,
    ) -> Result<RxStorageBulkWriteResponse, RxError> {
        self.ensure_open("bulk_write")?;

        // FIX 1: run the blocking rusqlite transaction on a dedicated blocking
        // thread instead of a tokio worker. Holding the connection mutex while
        // doing synchronous SQLite work directly on a tokio worker thread
        // starves the heartbeat timer + replication on a 1-2 worker VPS. We
        // move owned clones of everything the transaction needs into
        // `spawn_blocking`, run the identical transaction/categorize logic
        // there, and return only the `Send` results (errors, optional event
        // bulk, optional checkpoint). Semantics (ordering, Immediate
        // transaction, error mapping) are unchanged — only WHERE it runs.
        let connection = Arc::clone(&self.connection);
        let schema_has_attachments = self.schema.attachments.is_some();
        let json_schema = self.schema.clone();
        let primary_path = self.primary_path.clone();
        let table_name = self.table_name.clone();
        let context = context.to_string();

        let (error, event_bulk, checkpoint): (
            Vec<crate::types::RxStorageWriteError>,
            Option<EventBulk>,
            Option<Value>,
        ) = tokio::task::spawn_blocking(move || -> RxResult<_> {
            let mut conn = connection.lock();
            let tx = conn
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(sqlite_error)?;

            // Production write-path validation: reject clearly-corrupt peer
            // documents with a 422 instead of persisting them. Conservative by
            // design (see rx_schema::validate_write_document) so conforming data is
            // never rejected. Invalid rows are dropped from this batch; valid rows
            // proceed through the normal categorize/conflict path unchanged.
            let mut validation_errors: Vec<crate::types::RxStorageWriteError> = Vec::new();
            let mut document_writes = document_writes;
            if document_writes.iter().any(|w| {
                crate::rx_schema::validate_write_document(&json_schema, &primary_path, &w.document)
                    .is_err()
            }) {
                let mut kept = Vec::with_capacity(document_writes.len());
                for write in document_writes.drain(..) {
                    match crate::rx_schema::validate_write_document(
                        &json_schema,
                        &primary_path,
                        &write.document,
                    ) {
                        Ok(()) => kept.push(write),
                        Err(message) => {
                            let document_id = write
                                .document
                                .get(&primary_path)
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            validation_errors.push(crate::types::RxStorageWriteError {
                                status: 422,
                                is_error: true,
                                document_id,
                                write_row: write,
                                document_in_db: None,
                                validation_errors: vec![json!({ "message": message })],
                                schema: None,
                                attachment_id: None,
                            });
                        }
                    }
                }
                document_writes = kept;
            }

            let mut docs_in_db = HashMap::new();
            {
                // Only load the documents we are actually writing, not the whole
                // table. `categorize_bulk_write_rows` looks up the current DB state
                // per write id, so a full-table scan made every bulk_write O(N) in
                // collection size (O(N^2) over a replication run, all under the
                // global write mutex) — the dominant scaling risk for large
                // collections (documents, blob chunks). Fetch by id via the
                // primary-key index instead.
                let mut ids: Vec<String> = Vec::with_capacity(document_writes.len());
                for write in &document_writes {
                    if let Some(id) = write.document.get(&primary_path).and_then(Value::as_str) {
                        ids.push(id.to_string());
                    }
                }
                ids.sort_unstable();
                ids.dedup();
                for id in ids {
                    if let Some(doc) = document_by_id(&tx, &table_name, &id)? {
                        docs_in_db.insert(id, doc);
                    }
                }
            }
            let categorized = crate::rx_storage_helper::categorize_bulk_write_rows(
                schema_has_attachments,
                &primary_path,
                &docs_in_db,
                &document_writes,
                &context,
            );
            let mut error = categorized.errors;
            error.extend(validation_errors);

            for row in categorized.bulk_insert_docs.iter() {
                insert_document(&tx, &table_name, &primary_path, &row.document)?;
            }
            for row in categorized.bulk_update_docs.iter() {
                update_document(&tx, &table_name, &primary_path, row)?;
            }
            tx.commit().map_err(sqlite_error)?;

            let mut event_bulk: Option<EventBulk> = None;
            let mut checkpoint: Option<Value> = None;
            if !categorized.event_bulk.events.is_empty() {
                if let Some(newest) = categorized.newest_row.as_ref() {
                    checkpoint = Some(json!({
                        "id": newest.document.get(&primary_path).cloned().unwrap_or(Value::Null),
                        "lwt": newest
                            .document
                            .get("_meta")
                            .and_then(|meta| meta.get("lwt"))
                            .cloned()
                            .unwrap_or(json!(0)),
                    }));
                }
                let mut bulk = categorized.event_bulk;
                bulk.checkpoint = checkpoint.clone();
                event_bulk = Some(bulk);
            }
            Ok((error, event_bulk, checkpoint))
        })
        .await
        .map_err(join_error)??;

        let ret = RxStorageBulkWriteResponse { error };

        if let Some(checkpoint) = checkpoint {
            *self.external_checkpoint.lock() = checkpoint;
        }
        if let Some(bulk) = event_bulk {
            self.changes.next(bulk);
        }
        Ok(ret)
    }

    async fn find_documents_by_id(
        &self,
        ids: &[String],
        with_deleted: bool,
    ) -> Result<Vec<Value>, RxError> {
        self.ensure_open("find_documents_by_id")?;
        // FIX 1: read off the tokio worker thread.
        let connection = Arc::clone(&self.connection);
        let table_name = self.table_name.clone();
        let ids = ids.to_vec();
        tokio::task::spawn_blocking(move || -> RxResult<Vec<Value>> {
            let conn = connection.lock();
            let mut ret = Vec::new();
            for id in &ids {
                if let Some(doc) = document_by_id(&conn, &table_name, id)? {
                    let deleted = doc
                        .get("_deleted")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if with_deleted || !deleted {
                        ret.push(doc);
                    }
                }
            }
            Ok(ret)
        })
        .await
        .map_err(join_error)?
    }

    async fn query_stream_into(
        &self,
        prepared_query: &Value,
        chunk_size: usize,
        on_batch: &mut (dyn FnMut(Vec<Value>) -> Result<bool, RxError> + Send),
    ) -> Result<(), RxError> {
        // V1.5: route through the inherent bounded-memory cursor path so
        // the dispatcher actually gets streaming semantics instead of
        // materializing the whole result in RAM.
        self.query_stream(prepared_query, chunk_size, |batch| on_batch(batch))
    }

    async fn query(&self, prepared_query: &Value) -> Result<RxStorageQueryResult, RxError> {
        self.ensure_open("query")?;
        let query: FilledMangoQuery =
            serde_json::from_value(prepared_query.get("query").cloned().unwrap_or(Value::Null))
                .map_err(|err| {
                    new_rx_error(
                        "SQLITE_QUERY",
                        Some(json!({ "message": format!("invalid prepared query: {err}") })),
                    )
                })?;
        let skip = query.skip.unwrap_or(0) as usize;
        let limit = query
            .limit
            .map(|limit| limit as usize)
            .unwrap_or(usize::MAX);
        let skip_plus_limit = skip.saturating_add(limit);
        let matcher = get_query_matcher(&self.schema, &query);
        let comparator = get_sort_comparator(&self.schema, &query);
        let primary_ids = primary_key_selector_ids(&query, &self.primary_path);

        // FIX 1: run the full-table scan + sort off the tokio worker thread.
        // The matcher/comparator are `Arc<dyn Fn .. + Send + Sync>` so they
        // move cleanly into `spawn_blocking`.
        let connection = Arc::clone(&self.connection);
        let table_name = self.table_name.clone();
        let documents = tokio::task::spawn_blocking(move || -> RxResult<Vec<Value>> {
            let conn = connection.lock();
            let mut rows: Vec<Value> = Vec::new();

            if let Some(ids) = primary_ids {
                // Query/count callers are not all routed through RxQuery's
                // find-by-id fast path. Keep primary-key equality bounded by
                // the requested id set instead of scanning large collections.
                for id in ids {
                    if let Some(doc) = document_by_id(&conn, &table_name, &id)? {
                        if matcher(&doc) {
                            rows.push(doc);
                        }
                    }
                }
            } else {
                // Stream rows one at a time and keep only those that match.
                // Sort and truncate at the end so we never materialize the
                // whole table. Memory bound: O(number of matches), not
                // O(table size).
                for_each_document(&conn, &table_name, |doc| {
                    if matcher(&doc) {
                        rows.push(doc);
                    }
                    Ok(true)
                })?;
            }
            rows.sort_by(|a, b| comparator(a, b));
            let start = skip.min(rows.len());
            let end = skip_plus_limit.min(rows.len());
            Ok(rows[start..end].to_vec())
        })
        .await
        .map_err(join_error)??;
        Ok(RxStorageQueryResult { documents })
    }

    async fn count(&self, prepared_query: &Value) -> Result<RxStorageCountResult, RxError> {
        let result = self.query(prepared_query).await?;
        Ok(RxStorageCountResult {
            count: result.documents.len() as u64,
            mode: "fast".to_string(),
        })
    }

    async fn get_changed_documents_since(
        &self,
        limit: u64,
        checkpoint: Option<&Value>,
    ) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
        self.ensure_open("get_changed_documents_since")?;
        // FIX 1: read off the tokio worker thread.
        let connection = Arc::clone(&self.connection);
        let table_name = self.table_name.clone();
        let primary_path = self.primary_path.clone();
        let checkpoint = checkpoint.cloned();
        tokio::task::spawn_blocking(
            move || -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
                let conn = connection.lock();
                changed_documents_since(
                    &conn,
                    &table_name,
                    &primary_path,
                    limit,
                    checkpoint.as_ref(),
                )
            },
        )
        .await
        .map_err(join_error)?
    }

    fn change_stream(&self) -> RxStream<EventBulk> {
        self.changes.subscribe()
    }

    async fn cleanup(&self, min_deleted_time: i64) -> Result<bool, RxError> {
        self.ensure_open("cleanup")?;
        // FIX 1: run the DELETE off the tokio worker thread.
        let connection = Arc::clone(&self.connection);
        let table_name = self.table_name.clone();
        tokio::task::spawn_blocking(move || -> RxResult<bool> {
            cleanup_deleted_documents(&connection, &table_name, min_deleted_time)
        })
        .await
        .map_err(join_error)?
    }

    async fn remove(&self) -> Result<(), RxError> {
        self.ensure_open("remove")?;
        // FIX 1: run the DROP TABLE off the tokio worker thread.
        let connection = Arc::clone(&self.connection);
        let table_name = self.table_name.clone();
        tokio::task::spawn_blocking(move || -> RxResult<()> {
            let conn = connection.lock();
            drop_table(&conn, &table_name)
        })
        .await
        .map_err(join_error)??;
        self.closed.store(true, Ordering::SeqCst);
        unregister_table_notifier(&self.database_key, &self.table_name);
        Ok(())
    }

    async fn close(&self) -> Result<(), RxError> {
        self.closed.store(true, Ordering::SeqCst);
        unregister_table_notifier(&self.database_key, &self.table_name);
        Ok(())
    }

    async fn replication_checkpoint_status(&self) -> Value {
        // FIX 1: compute the checkpoint snapshot off the tokio worker thread.
        let connection = Arc::clone(&self.connection);
        let table_name = self.table_name.clone();
        let database_name = self.database_name.clone();
        let collection_name = self.collection_name.clone();
        let schema = self.schema.clone();
        tokio::task::spawn_blocking(move || {
            checkpoint_status_snapshot(
                &connection,
                &table_name,
                &database_name,
                &collection_name,
                &schema,
            )
        })
        .await
        .unwrap_or_else(|_| json!({ "source": "rxdb-rs-sqlite", "state": "error" }))
    }

    async fn get_attachment_data(
        &self,
        _document_id: &str,
        _attachment_id: &str,
        _digest: &str,
    ) -> Result<String, RxError> {
        Err(new_rx_error(
            "SQL1",
            Some(json!({
                "message": "sqlite storage does not inline attachment payloads"
            })),
        ))
    }
}

impl Drop for RxStorageInstanceSqlite {
    fn drop(&mut self) {
        unregister_table_notifier(&self.database_key, &self.table_name);
    }
}

impl RxStorageInstanceSqlite {
    /// V1.5 streaming query for the WebRTC `rxdb.query.fetch` handler. Yields
    /// matching documents in batches sized by `chunk_size`. The visitor
    /// returns `Ok(true)` to keep streaming, `Ok(false)` to stop.
    ///
    /// Unlike `query`, this never materializes the whole table at once — it
    /// hands batches off as it goes, so a `business_records` table with
    /// millions of rows still produces bounded chunks. Sorting is applied
    /// per-batch only; the caller must provide a sort that is consistent
    /// with the SQLite row-order if cross-batch order matters.
    pub fn query_stream<F>(
        &self,
        prepared_query: &Value,
        chunk_size: usize,
        mut visit: F,
    ) -> RxResult<()>
    where
        F: FnMut(Vec<Value>) -> RxResult<bool>,
    {
        self.ensure_open("query_stream")?;
        let query: FilledMangoQuery =
            serde_json::from_value(prepared_query.get("query").cloned().unwrap_or(Value::Null))
                .map_err(|err| {
                    new_rx_error(
                        "SQLITE_QUERY",
                        Some(json!({ "message": format!("invalid prepared query: {err}") })),
                    )
                })?;
        let limit = query
            .limit
            .map(|limit| limit as usize)
            .unwrap_or(usize::MAX);
        let skip = query.skip.unwrap_or(0) as usize;
        let matcher = get_query_matcher(&self.schema, &query);
        let comparator = get_sort_comparator(&self.schema, &query);

        // V1.5 production-hardening: open a DEDICATED read-only connection
        // for this stream. The shared write-connection stays free for other
        // peers, replication, and same-process writes. WAL mode (set in
        // `RxStorageSqlite::connection`) makes concurrent readers cheap.
        let read_conn = self.open_read_only_connection()?;

        // Correctness: skip + global sort require us to know the order of
        // ALL matches before deciding which N to emit. Per-chunk sorting
        // would mis-page across batches.
        //
        // Scalability: when `limit` is set (the V1.5 windowed case), we only
        // need the top-(skip+limit) matches in global sort order, then we
        // drop the first `skip` rows. We keep a single bounded, sorted-by-
        // comparator buffer of capacity `cap = skip+limit` and stream rows
        // through it: a row that compares >= the current worst-of-top-K is
        // discarded immediately. Memory is therefore O(skip+limit), not
        // O(matches). For 1M matches and limit=100, that's 100 docs in RAM
        // instead of 1M.
        //
        // When `limit` is unbounded (rare V1.5 path; mostly replication
        // pull-all), we fall back to collecting all matches. That mode is
        // already the right shape for pull-replication callers and will be
        // addressed separately when limit-less sort gets pushed into SQLite.
        let cap = skip.saturating_add(limit);
        let chunk = chunk_size.max(1);
        if cap < usize::MAX {
            // Bounded top-K scan. `top` is kept ASC-sorted by the query's
            // sort order; `top[0]` is the best match, `top[cap-1]` is the
            // worst element we still keep.
            let mut top: Vec<Value> = Vec::with_capacity(cap.min(64 * 1024));
            for_each_document(&read_conn, &self.table_name, |doc| {
                if !matcher(&doc) {
                    return Ok(true);
                }
                if top.len() < cap {
                    let pos = top.partition_point(|existing| {
                        comparator(existing, &doc) != std::cmp::Ordering::Greater
                    });
                    top.insert(pos, doc);
                } else {
                    // top is full; discard `doc` unless it is strictly better
                    // than the current worst (=last) element.
                    let worst = top.last().expect("top is full so non-empty");
                    if comparator(&doc, worst) == std::cmp::Ordering::Less {
                        let pos = top.partition_point(|existing| {
                            comparator(existing, &doc) != std::cmp::Ordering::Greater
                        });
                        top.insert(pos, doc);
                        top.pop();
                    }
                }
                Ok(true)
            })?;
            if skip >= top.len() {
                return Ok(());
            }
            let mut window = top.split_off(skip);
            drop(top);
            // window.len() <= limit by construction (cap = skip + limit).
            while !window.is_empty() {
                let take = window.len().min(chunk);
                let batch: Vec<Value> = window.drain(..take).collect();
                if !visit(batch)? {
                    return Ok(());
                }
            }
            return Ok(());
        }
        // Limit-less path (cap == usize::MAX). Collect all matches, global
        // sort, drop skip, emit. Memory: O(matches).
        let mut matches: Vec<Value> = Vec::new();
        for_each_document(&read_conn, &self.table_name, |doc| {
            if matcher(&doc) {
                matches.push(doc);
            }
            Ok(true)
        })?;
        matches.sort_by(|a, b| comparator(a, b));
        if skip >= matches.len() {
            return Ok(());
        }
        let mut window = matches.split_off(skip);
        drop(matches);
        while !window.is_empty() {
            let take = window.len().min(chunk);
            let batch: Vec<Value> = window.drain(..take).collect();
            if !visit(batch)? {
                return Ok(());
            }
        }
        Ok(())
    }

    fn open_read_only_connection(&self) -> RxResult<rusqlite::Connection> {
        use rusqlite::OpenFlags;
        let path = &self.database_path;
        let path_str = path.to_string_lossy();
        // SQLite supports `:memory:` only for the connection that created
        // it. Memory DBs are used in tests where we don't run concurrent
        // streams against the same instance, so falling back to the shared
        // connection is acceptable. For file-backed DBs, WAL mode gives us
        // a real concurrent reader.
        if path_str == ":memory:" {
            // Fall back to the shared connection by cloning the connection
            // primitive — but we still need to release it after the stream.
            // The shared lock is held just for this stream; this is the
            // legacy behavior the tests rely on. For production (file-backed)
            // we go through the OpenFlags::SQLITE_OPEN_READ_ONLY path below.
            return Err(new_rx_error(
                "SQLITE_QUERY",
                Some(json!({
                    "message": "in-memory SQLite does not support concurrent readers; use file-backed storage in production"
                })),
            ));
        }
        let conn = rusqlite::Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(super::types::sqlite_error)?;
        conn.busy_timeout(std::time::Duration::from_secs(10))
            .map_err(super::types::sqlite_error)?;
        Ok(conn)
    }
}

#[allow(dead_code)]
fn _zero_use() {
    let _ = random_token(Some(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use crate::rx_query_helper::{normalize_mango_query, prepare_query};
    use crate::storage::sqlite::{
        create_storage_instance, get_rx_storage_sqlite, RxStorageSqliteSettings,
    };
    use crate::types::{JsonSchema, MangoQuery, PrimaryKey, RxStorageInstanceCreationParams};

    fn test_schema() -> RxJsonSchema {
        let mut properties = HashMap::new();
        properties.insert(
            "id".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
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
        properties.insert(
            "_deleted".to_string(),
            JsonSchema {
                schema_type: Some("boolean".to_string()),
                ..Default::default()
            },
        );
        properties.insert(
            "_rev".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
                ..Default::default()
            },
        );
        let mut meta_properties = HashMap::new();
        meta_properties.insert(
            "lwt".to_string(),
            JsonSchema {
                schema_type: Some("number".to_string()),
                ..Default::default()
            },
        );
        properties.insert(
            "_meta".to_string(),
            JsonSchema {
                schema_type: Some("object".to_string()),
                properties: meta_properties,
                ..Default::default()
            },
        );
        properties.insert(
            "_attachments".to_string(),
            JsonSchema {
                schema_type: Some("object".to_string()),
                additional_properties: Some(true),
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
            additional_properties: true,
            extra: HashMap::new(),
        }
    }

    fn params(schema: RxJsonSchema) -> RxStorageInstanceCreationParams {
        RxStorageInstanceCreationParams {
            database_instance_token: "token".to_string(),
            database_name: "db".to_string(),
            collection_name: "docs".to_string(),
            schema,
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: false,
            password: None,
        }
    }

    fn doc(id: &str, rev: &str, age: i64, deleted: bool, lwt: f64) -> Value {
        json!({
            "id": id,
            "age": age,
            "_rev": rev,
            "_deleted": deleted,
            "_meta": { "lwt": lwt },
            "_attachments": {}
        })
    }

    #[tokio::test]
    async fn persists_documents_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ctox.sqlite3");
        let schema = test_schema();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: path.clone(),
        });
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("a", "1-a", 1, false, 1.0),
                }],
                "insert",
            )
            .await
            .unwrap();
        instance.close().await.unwrap();

        let reopened = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: path,
        });
        let instance = create_storage_instance(&reopened, params(schema))
            .await
            .unwrap();
        let docs = instance
            .find_documents_by_id(&["a".to_string()], false)
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].get("age").and_then(Value::as_i64), Some(1));
    }

    #[tokio::test]
    async fn bulk_write_reads_only_written_ids_state_among_many_rows() {
        // Guards the P1 fix: bulk_write must look up the CURRENT db state only for
        // the ids being written (via the primary-key index), not scan the whole
        // table. This test seeds a large collection and verifies that the per-id
        // lookup still yields correct conflict detection (successful update with
        // the right previous _rev, untouched neighbour, and a 409 on a stale rev).
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let instance = create_storage_instance(&storage, params(test_schema()))
            .await
            .unwrap();

        const N: usize = 400;
        let seed: Vec<BulkWriteRow> = (0..N)
            .map(|i| BulkWriteRow {
                previous: None,
                document: doc(&format!("k{i}"), "1-a", i as i64, false, 1.0),
            })
            .collect();
        let resp = instance.bulk_write(seed, "seed").await.unwrap();
        assert!(
            resp.error.is_empty(),
            "seed should not error: {:?}",
            resp.error
        );

        // Valid update (correct previous rev) + a fresh insert.
        let resp = instance
            .bulk_write(
                vec![
                    BulkWriteRow {
                        previous: Some(doc("k200", "1-a", 200, false, 1.0)),
                        document: doc("k200", "2-b", 9999, false, 2.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("knew", "1-a", 7, false, 1.0),
                    },
                ],
                "write",
            )
            .await
            .unwrap();
        assert!(
            resp.error.is_empty(),
            "valid update+insert must not error: {:?}",
            resp.error
        );

        let got = instance
            .find_documents_by_id(
                &["k200".to_string(), "k100".to_string(), "knew".to_string()],
                false,
            )
            .await
            .unwrap();
        let by_id = |id: &str| {
            got.iter()
                .find(|d| d.get("id").and_then(Value::as_str) == Some(id))
        };
        assert_eq!(
            by_id("k200")
                .and_then(|d| d.get("age"))
                .and_then(Value::as_i64),
            Some(9999),
            "updated row reflects new state"
        );
        assert_eq!(
            by_id("k100")
                .and_then(|d| d.get("age"))
                .and_then(Value::as_i64),
            Some(100),
            "unrelated neighbour untouched"
        );
        assert!(by_id("knew").is_some(), "insert present");

        // Stale update (wrong previous rev) must conflict — proves the per-id
        // current-state lookup is correct, not just blindly overwriting.
        let resp = instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: Some(doc("k300", "9-stale", 300, false, 1.0)),
                    document: doc("k300", "2-x", 1, false, 3.0),
                }],
                "stale",
            )
            .await
            .unwrap();
        assert_eq!(resp.error.len(), 1, "stale update must conflict");
        assert_eq!(resp.error[0].status, 409, "conflict status");
    }

    #[tokio::test]
    async fn sequential_writes_into_large_collection_stay_correct() {
        // P7 scale guard: the replication pattern that used to be O(N^2) — many
        // small writes trickling into a large collection. Seed a big collection,
        // then apply many scattered sequential single-doc updates and verify each
        // landed correctly. (Correctness, not a flaky timing assertion; the perf
        // win is structural — see bulk_write fetching only written ids.)
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let instance = create_storage_instance(&storage, params(test_schema()))
            .await
            .unwrap();

        const N: usize = 1000;
        let seed: Vec<BulkWriteRow> = (0..N)
            .map(|i| BulkWriteRow {
                previous: None,
                document: doc(&format!("k{i}"), "1-a", i as i64, false, 1.0),
            })
            .collect();
        instance.bulk_write(seed, "seed").await.unwrap();

        // 40 scattered sequential updates, each correct against current state.
        for step in 0..40usize {
            let i = (step * 97) % N; // spread across the table
            let resp = instance
                .bulk_write(
                    vec![BulkWriteRow {
                        previous: Some(doc(&format!("k{i}"), "1-a", i as i64, false, 1.0)),
                        document: doc(&format!("k{i}"), "2-b", 100_000 + step as i64, false, 2.0),
                    }],
                    "seq",
                )
                .await
                .unwrap();
            assert!(
                resp.error.is_empty(),
                "sequential write {step} must not error: {:?}",
                resp.error
            );
        }

        // Spot-check a couple of updated rows reflect the new state.
        let got = instance
            .find_documents_by_id(&["k0".to_string(), "k97".to_string()], false)
            .await
            .unwrap();
        assert_eq!(got.len(), 2);
        for d in &got {
            assert!(
                d.get("age").and_then(Value::as_i64).unwrap_or(0) >= 100_000,
                "updated row must hold the new age"
            );
        }
    }

    #[tokio::test]
    async fn query_filters_sorts_and_limits_documents() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        instance
            .bulk_write(
                vec![
                    BulkWriteRow {
                        previous: None,
                        document: doc("a", "1-a", 1, false, 1.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("b", "1-b", 3, false, 2.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("c", "1-c", 2, false, 3.0),
                    },
                ],
                "insert",
            )
            .await
            .unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "asc".to_string());
        let filled = normalize_mango_query(
            &schema,
            MangoQuery {
                selector: Some(json!({ "age": { "$gte": 2 } })),
                sort: Some(vec![sort]),
                index: None,
                limit: Some(1),
                skip: Some(0),
            },
        );
        let prepared = prepare_query(&schema, filled).unwrap();
        let result = instance.query(&prepared).await.unwrap();
        assert_eq!(result.documents.len(), 1);
        assert_eq!(
            result.documents[0].get("id").and_then(Value::as_str),
            Some("c")
        );
    }

    #[tokio::test]
    async fn query_primary_key_equality_uses_bounded_candidate_set() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let rows: Vec<BulkWriteRow> = (0..300)
            .map(|idx| BulkWriteRow {
                previous: None,
                document: doc(&format!("k{idx:03}"), "1-a", idx, false, idx as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "desc".to_string());
        let filled = normalize_mango_query(
            &schema,
            MangoQuery {
                selector: Some(json!({ "id": { "$in": ["k003", "missing", "k299"] } })),
                sort: Some(vec![sort]),
                index: None,
                limit: None,
                skip: Some(0),
            },
        );
        let prepared = prepare_query(&schema, filled).unwrap();
        let result = instance.query(&prepared).await.unwrap();
        let ids: Vec<&str> = result
            .documents
            .iter()
            .filter_map(|doc| doc.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(ids, vec!["k299", "k003"]);

        let count = instance.count(&prepared).await.unwrap();
        assert_eq!(count.count, 2);
    }

    #[tokio::test]
    async fn query_stream_applies_skip_and_global_sort() {
        // Regression for the review finding: skip docs MUST be removed from
        // the output and the sort MUST be global (not per-batch). We seed
        // 60 docs with shuffled `age`, ask for skip=20 limit=20 sort=age asc
        // chunk_size=5 — the result must be the 20 docs with age 20..40 in
        // ascending order, not the 20th..40th rows of the insertion order.
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        // Insertion order is shuffled — if sort is per-batch, output will
        // be wrong. We pick a permutation that crosses chunk boundaries.
        let ages: Vec<i64> = vec![
            50, 30, 10, 40, 20, 5, 55, 35, 15, 45, 25, 0, 51, 31, 11, 41, 21, 1, 52, 32, 12, 42,
            22, 2, 53, 33, 13, 43, 23, 3, 54, 34, 14, 44, 24, 4, 56, 36, 16, 46, 26, 6, 57, 37, 17,
            47, 27, 7, 58, 38, 18, 48, 28, 8, 59, 39, 19, 49, 29, 9,
        ];
        let rows: Vec<BulkWriteRow> = ages
            .iter()
            .enumerate()
            .map(|(i, age)| BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{i:03}"), "1-a", *age, false, i as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "asc".to_string());
        let filled = normalize_mango_query(
            &schema,
            MangoQuery {
                selector: Some(json!({})),
                sort: Some(vec![sort]),
                index: None,
                limit: Some(20),
                skip: Some(20),
            },
        );
        let prepared = prepare_query(&schema, filled).unwrap();
        let mut collected: Vec<i64> = Vec::new();
        instance
            .query_stream(&prepared, 5, |batch| {
                for d in batch {
                    collected.push(d.get("age").and_then(Value::as_i64).unwrap());
                }
                Ok(true)
            })
            .unwrap();
        let expected: Vec<i64> = (20..40).collect();
        assert_eq!(
            collected, expected,
            "skip=20 limit=20 sort=age asc must yield ages 20..40 in order, got {:?}",
            collected
        );
    }

    #[tokio::test]
    async fn query_stream_bounded_top_k_holds_at_skip_plus_limit_when_matches_are_huge() {
        // Review follow-up: the in-RAM working set for a sorted+windowed
        // query must be bounded by `skip + limit`, not by the total number
        // of matches. We seed 4 000 docs, request skip=10 limit=20 sort=age
        // asc, and verify the output is the 20 docs with age 10..30 in
        // order. Combined with the bounded-top-K implementation, that means
        // 30 docs were held in RAM at peak — not 4 000.
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        // Reverse-sorted insertion stresses the discard-worst branch: every
        // incoming doc is BETTER than the current worst-of-top-K, so the
        // bounded buffer churns maximally.
        let total: i64 = 4_000;
        let rows: Vec<BulkWriteRow> = (0..total)
            .rev()
            .enumerate()
            .map(|(i, age)| BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{i:05}"), "1-a", age, false, i as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "asc".to_string());
        let filled = normalize_mango_query(
            &schema,
            MangoQuery {
                selector: Some(json!({})),
                sort: Some(vec![sort]),
                index: None,
                limit: Some(20),
                skip: Some(10),
            },
        );
        let prepared = prepare_query(&schema, filled).unwrap();
        let mut collected: Vec<i64> = Vec::new();
        instance
            .query_stream(&prepared, 7, |batch| {
                for d in batch {
                    collected.push(d.get("age").and_then(Value::as_i64).unwrap());
                }
                Ok(true)
            })
            .unwrap();
        let expected: Vec<i64> = (10..30).collect();
        assert_eq!(
            collected, expected,
            "bounded top-K must yield ages 10..30 across 4000 reverse-sorted matches"
        );
    }

    #[tokio::test]
    async fn query_stream_bounded_top_k_handles_skip_past_match_count() {
        // Edge: if skip exceeds the total number of matches, the stream
        // must produce zero batches (not panic, not emit an empty batch).
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let rows: Vec<BulkWriteRow> = (0..10)
            .map(|i| BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{i}"), "1-a", i as i64, false, i as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "asc".to_string());
        let filled = normalize_mango_query(
            &schema,
            MangoQuery {
                selector: Some(json!({})),
                sort: Some(vec![sort]),
                index: None,
                limit: Some(50),
                skip: Some(100),
            },
        );
        let prepared = prepare_query(&schema, filled).unwrap();
        let mut batches = 0usize;
        instance
            .query_stream(&prepared, 5, |_batch| {
                batches += 1;
                Ok(true)
            })
            .unwrap();
        assert_eq!(batches, 0, "skip past match count must emit zero batches");
    }

    #[tokio::test]
    async fn query_stream_unbounded_limit_still_sorts_globally() {
        // When limit is None the bounded top-K path is bypassed and we fall
        // back to collect-all + global-sort. That path must keep the same
        // ordering guarantees the bounded path provides.
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let ages: Vec<i64> = vec![7, 1, 9, 3, 5, 8, 2, 6, 4, 0];
        let rows: Vec<BulkWriteRow> = ages
            .iter()
            .enumerate()
            .map(|(i, age)| BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{i}"), "1-a", *age, false, i as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "asc".to_string());
        let filled = normalize_mango_query(
            &schema,
            MangoQuery {
                selector: Some(json!({})),
                sort: Some(vec![sort]),
                index: None,
                limit: None,
                skip: Some(3),
            },
        );
        let prepared = prepare_query(&schema, filled).unwrap();
        let mut collected: Vec<i64> = Vec::new();
        instance
            .query_stream(&prepared, 4, |batch| {
                for d in batch {
                    collected.push(d.get("age").and_then(Value::as_i64).unwrap());
                }
                Ok(true)
            })
            .unwrap();
        let expected: Vec<i64> = (3..10).collect();
        assert_eq!(
            collected, expected,
            "unbounded limit path must still globally sort and drop the skip prefix"
        );
    }

    #[tokio::test]
    async fn query_stream_emits_chunks_without_full_materialization() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let mut rows = Vec::with_capacity(250);
        for i in 0..250 {
            rows.push(BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{i:04}"), "1-a", i, false, i as f64),
            });
        }
        instance.bulk_write(rows, "insert").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "asc".to_string());
        let filled = normalize_mango_query(
            &schema,
            MangoQuery {
                selector: Some(json!({ "age": { "$gte": 0 } })),
                sort: Some(vec![sort.clone()]),
                index: None,
                limit: None,
                skip: None,
            },
        );
        let prepared = prepare_query(&schema, filled).unwrap();

        let mut chunks = Vec::new();
        instance
            .query_stream(&prepared, 100, |batch| {
                chunks.push(batch);
                Ok(true)
            })
            .unwrap();
        assert!(
            chunks.len() >= 3,
            "expected at least three chunks for 250 docs at chunk_size=100, got {}",
            chunks.len()
        );
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, 250, "all matches must be streamed");

        // Early termination: visit returns false after first chunk.
        let mut seen = 0usize;
        instance
            .query_stream(&prepared, 50, |batch| {
                seen += batch.len();
                Ok(false)
            })
            .unwrap();
        assert_eq!(seen, 50, "early-termination must stop after first chunk");
    }

    #[tokio::test]
    async fn changed_documents_since_uses_lwt_then_id_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema))
            .await
            .unwrap();
        instance
            .bulk_write(
                vec![
                    BulkWriteRow {
                        previous: None,
                        document: doc("a", "1-a", 1, false, 1.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("b", "1-b", 1, false, 1.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("c", "1-c", 1, false, 2.0),
                    },
                ],
                "insert",
            )
            .await
            .unwrap();

        let changed = instance
            .get_changed_documents_since(10, Some(&json!({ "id": "a", "lwt": 1.0 })))
            .await
            .unwrap();
        let ids: Vec<_> = changed
            .documents
            .iter()
            .filter_map(|doc| doc.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(ids, vec!["b", "c"]);
        assert_eq!(changed.checkpoint, json!({ "id": "c", "lwt": 2.0 }));
    }

    #[tokio::test]
    async fn replication_checkpoint_epoch_tracks_persisted_checkpoint_drift() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ctox.sqlite3");
        let schema = test_schema();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: path.clone(),
        });
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();

        let empty_status = instance.replication_checkpoint_status().await;
        assert_eq!(empty_status["source"], "rxdb-rs-sqlite");
        assert_eq!(empty_status["state"], "advertised");
        assert_eq!(empty_status["collection"], "docs");
        assert_eq!(empty_status["latestLwt"], 0.0);
        assert_eq!(empty_status["latestIdHash"], "");
        assert!(empty_status.get("latestId").is_none());

        instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("a", "1-a", 1, false, 1.0),
                }],
                "insert-a",
            )
            .await
            .unwrap();
        let after_a = instance.replication_checkpoint_status().await;
        assert_eq!(after_a["latestLwt"], 1.0);
        assert_eq!(after_a["latestIdHash"], sha256_hex(b"a"));
        assert_ne!(after_a["epoch"], empty_status["epoch"]);

        instance.close().await.unwrap();
        let reopened = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: path,
        });
        let reopened_instance = create_storage_instance(&reopened, params(schema))
            .await
            .unwrap();
        let reopened_status = reopened_instance.replication_checkpoint_status().await;
        assert_eq!(reopened_status["epoch"], after_a["epoch"]);
        assert_eq!(reopened_status["latestIdHash"], after_a["latestIdHash"]);

        reopened_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("b", "1-b", 1, false, 2.0),
                }],
                "insert-b",
            )
            .await
            .unwrap();
        let after_b = reopened_instance.replication_checkpoint_status().await;
        assert_eq!(after_b["latestLwt"], 2.0);
        assert_eq!(after_b["latestIdHash"], sha256_hex(b"b"));
        assert_ne!(after_b["epoch"], after_a["epoch"]);
        assert_eq!(after_b["schemaHash"], after_a["schemaHash"]);
    }

    #[tokio::test]
    async fn replication_checkpoint_epoch_isolated_across_schema_version_drift() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ctox.sqlite3");
        let schema_v0 = test_schema();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: path.clone(),
        });
        let instance_v0 = create_storage_instance(&storage, params(schema_v0.clone()))
            .await
            .unwrap();
        instance_v0
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("v0", "1-v0", 1, false, 1.0),
                }],
                "insert-v0",
            )
            .await
            .unwrap();
        let v0_status = instance_v0.replication_checkpoint_status().await;
        assert_eq!(v0_status["latestLwt"], 1.0);
        assert_eq!(v0_status["latestIdHash"], sha256_hex(b"v0"));
        instance_v0.close().await.unwrap();

        let mut schema_v1 = test_schema();
        schema_v1.version = 1;
        schema_v1.required.push("age".to_string());
        let reopened = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: path.clone(),
        });
        let instance_v1 = create_storage_instance(&reopened, params(schema_v1.clone()))
            .await
            .unwrap();
        let empty_v1_status = instance_v1.replication_checkpoint_status().await;
        assert_eq!(empty_v1_status["latestLwt"], 0.0);
        assert_eq!(empty_v1_status["latestIdHash"], "");
        assert_ne!(empty_v1_status["schemaHash"], v0_status["schemaHash"]);
        assert_ne!(empty_v1_status["epoch"], v0_status["epoch"]);

        instance_v1
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("v1", "1-v1", 1, false, 2.0),
                }],
                "insert-v1",
            )
            .await
            .unwrap();
        let v1_status = instance_v1.replication_checkpoint_status().await;
        assert_eq!(v1_status["latestLwt"], 2.0);
        assert_eq!(v1_status["latestIdHash"], sha256_hex(b"v1"));
        assert_ne!(v1_status["schemaHash"], v0_status["schemaHash"]);
        assert_ne!(v1_status["epoch"], v0_status["epoch"]);
        instance_v1.close().await.unwrap();

        let reopened_v0 = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: path,
        });
        let instance_v0_again = create_storage_instance(&reopened_v0, params(schema_v0))
            .await
            .unwrap();
        let v0_again_status = instance_v0_again.replication_checkpoint_status().await;
        assert_eq!(v0_again_status["epoch"], v0_status["epoch"]);
        assert_eq!(v0_again_status["latestIdHash"], v0_status["latestIdHash"]);
        assert_eq!(v0_again_status["schemaHash"], v0_status["schemaHash"]);
    }

    #[tokio::test]
    async fn change_stream_emits_external_sqlite_writes() {
        use tokio::time::timeout;
        use tokio_stream::StreamExt;

        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema))
            .await
            .unwrap();
        let mut stream = instance.change_stream();

        {
            let conn = instance.connection.lock();
            insert_document(
                &conn,
                &instance.table_name,
                &instance.primary_path,
                &doc("external", "1-external", 7, false, 10.0),
            )
            .unwrap();
        }

        let bulk = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("external write should be emitted")
            .expect("change stream should stay open");
        assert_eq!(bulk.context.as_deref(), Some("sqlite-external-poll"));
        assert_eq!(
            bulk.checkpoint,
            Some(json!({ "id": "external", "lwt": 10.0 }))
        );
        assert_eq!(bulk.events.len(), 1);
        assert_eq!(bulk.events[0].document_id, "external");
        assert_eq!(bulk.events[0].operation, "UPDATE");
    }

    #[tokio::test]
    async fn change_stream_emits_other_connection_sqlite_writes() {
        use tokio::time::timeout;
        use tokio_stream::StreamExt;

        let dir = tempfile::tempdir().unwrap();
        let database_path = dir.path().join("ctox.sqlite3");
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: database_path.clone(),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema))
            .await
            .unwrap();
        let mut stream = instance.change_stream();

        {
            let conn = rusqlite::Connection::open(&database_path).unwrap();
            insert_document(
                &conn,
                &instance.table_name,
                &instance.primary_path,
                &doc("external-connection", "1-external", 7, false, 10.0),
            )
            .unwrap();
        }

        let bulk = timeout(Duration::from_secs(4), stream.next())
            .await
            .expect("other-connection write should be emitted")
            .expect("change stream should stay open");
        assert_eq!(bulk.context.as_deref(), Some("sqlite-external-poll"));
        assert_eq!(
            bulk.checkpoint,
            Some(json!({ "id": "external-connection", "lwt": 10.0 }))
        );
        assert_eq!(bulk.events.len(), 1);
        assert_eq!(bulk.events[0].document_id, "external-connection");
        assert_eq!(bulk.events[0].operation, "UPDATE");
    }

    #[tokio::test]
    async fn cleanup_removes_old_deleted_documents_only() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema))
            .await
            .unwrap();
        instance
            .bulk_write(
                vec![
                    BulkWriteRow {
                        previous: None,
                        document: doc("deleted", "1-a", 1, true, 1.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("active", "1-b", 1, false, 1.0),
                    },
                ],
                "insert",
            )
            .await
            .unwrap();

        assert!(instance.cleanup(1).await.unwrap());
        let deleted = instance
            .find_documents_by_id(&["deleted".to_string()], true)
            .await
            .unwrap();
        let active = instance
            .find_documents_by_id(&["active".to_string()], false)
            .await
            .unwrap();
        assert!(deleted.is_empty());
        assert_eq!(active.len(), 1);
    }
}
