//! SQLite [`crate::types::RxStorageInstance`] implementation.

use std::collections::{BTreeSet, HashMap};
use std::ops::{Deref, DerefMut};
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::Mutex;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, Notify};

use crate::plugins::utils::utils_string::random_token;
use crate::rx_error::{new_rx_error, RxError, RxResult};
use crate::rx_query_helper::{
    get_query_matcher, get_sort_comparator, DeterministicSortComparator, QueryMatcher,
};
use crate::rx_schema_helper::get_primary_field_of_primary_key;
use crate::rxjs_compat::{RxStream, RxSubject, DEFAULT_SUBJECT_BUFFER};
use crate::types::{
    BulkWriteRow, EventBulk, FilledMangoQuery, RxJsonSchema, RxQueryPlan,
    RxStorageBulkWriteResponse, RxStorageChangedDocumentsSinceResult, RxStorageCountResult,
    RxStorageInstance, RxStorageInstanceCreationParams, RxStorageQueryResult,
};

use super::cleanup::cleanup_deleted_documents;
use super::metrics::{
    record_query_fallback_attribution, record_query_fallback_rows,
    record_sqlite_external_poll_drain, record_sqlite_external_poll_drain_failure,
    record_sqlite_writer_lock_acquire, record_sqlite_writer_lock_held,
    record_sqlite_writer_lock_wait, SQLITE_BULK_WRITE_CALLS, SQLITE_BULK_WRITE_ROWS,
    SQLITE_CHANGED_DOCUMENTS_SINCE_CALLS, SQLITE_CHANGED_DOCUMENTS_SINCE_RESULTS,
    SQLITE_COUNT_CALLS, SQLITE_COUNT_FALLBACK_QUERY_CALLS, SQLITE_FIND_DOCUMENTS_BY_ID_CALLS,
    SQLITE_FIND_DOCUMENTS_BY_ID_REQUESTED, SQLITE_FIND_DOCUMENTS_BY_ID_RESULTS, SQLITE_QUERY_CALLS,
    SQLITE_QUERY_FALLBACK_CALLS, SQLITE_QUERY_FALLBACK_INDEXED_CANDIDATE_CALLS,
    SQLITE_QUERY_FALLBACK_TOO_BROAD_CALLS, SQLITE_QUERY_RESULTS, SQLITE_QUERY_STREAM_CALLS,
    SQLITE_QUERY_STREAM_RESULTS, SQLITE_QUERY_STREAM_UNSUPPORTED_CALLS,
    SQLITE_READ_ONLY_OPEN_CALLS, SQLITE_READ_ONLY_OPEN_FAILURES, SQLITE_WRITER_FALLBACKS,
    SQLITE_WRITE_TRANSACTIONS_COMMITTED, SQLITE_WRITE_TRANSACTIONS_FAILED,
    SQLITE_WRITE_TRANSACTIONS_STARTED,
};
use super::sql::{
    compile_count_sql, compile_query_plan_candidate_sql, compile_query_sql,
    count_with_compiled_sql, documents_by_ids, drop_table, for_each_document,
    for_each_document_with_compiled_sql, insert_document, query_documents_with_compiled_sql,
    quote_identifier, update_document, CompiledSqliteQuery,
};
use super::types::{
    sqlite_concurrent_reader_error, sqlite_error, sqlite_path_is_in_memory, SharedSqliteConnection,
};

const SQLITE_EXTERNAL_POLL_DEFAULT_LIMIT: u64 = 50;
const SQLITE_EXTERNAL_POLL_MAX_BATCHES_PER_WAKE: usize = 32;
const SQLITE_EXTERNAL_POLL_SAFETY_INTERVAL: Duration = Duration::from_secs(60);
const SQLITE_EXTERNAL_POLL_FAILURE_BACKOFF_BASE: Duration = Duration::from_millis(25);
const SQLITE_EXTERNAL_POLL_FAILURE_BACKOFF_MAX: Duration = Duration::from_secs(5);
const SQLITE_QUERY_FALLBACK_SCAN_LIMIT: u64 = 4096;
const SQLITE_QUERY_FALLBACK_TOO_BROAD: &str = "SQLITE_QUERY_FALLBACK_TOO_BROAD";
const SQLITE_QUERY_STREAM_UNSUPPORTED: &str = "SQLITE_QUERY_STREAM_UNSUPPORTED";

static INSTANCE_ID: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static CHANGED_DOCUMENTS_SINCE_TABLE_CALLS: OnceLock<StdMutex<HashMap<String, usize>>> =
    OnceLock::new();
#[cfg(test)]
static FIND_DOCUMENTS_BY_ID_WRITER_FALLBACKS: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static CHANGED_DOCUMENTS_SINCE_WRITER_FALLBACKS: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static QUERY_WRITER_FALLBACKS: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static TEST_EXTERNAL_POLL_SAFETY_INTERVAL_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static TEST_QUERY_STREAM_ROW_DELAY_MS_BY_TABLE: OnceLock<StdMutex<HashMap<String, u64>>> =
    OnceLock::new();

#[cfg(test)]
fn set_test_query_stream_row_delay_ms(table_name: &str, delay_ms: u64) {
    let mut delays = TEST_QUERY_STREAM_ROW_DELAY_MS_BY_TABLE
        .get_or_init(|| StdMutex::new(HashMap::new()))
        .lock()
        .unwrap();
    if delay_ms == 0 {
        delays.remove(table_name);
    } else {
        delays.insert(table_name.to_string(), delay_ms);
    }
}

#[cfg(test)]
fn test_query_stream_row_delay(table_name: &str) -> Duration {
    let delay_ms = TEST_QUERY_STREAM_ROW_DELAY_MS_BY_TABLE
        .get_or_init(|| StdMutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .get(table_name)
        .copied()
        .unwrap_or(0);
    Duration::from_millis(delay_ms)
}

#[cfg(test)]
fn reset_changed_documents_since_table_call_count(table_name: &str) {
    let mut counts = CHANGED_DOCUMENTS_SINCE_TABLE_CALLS
        .get_or_init(|| StdMutex::new(HashMap::new()))
        .lock()
        .unwrap();
    counts.insert(table_name.to_string(), 0);
}

#[cfg(test)]
fn changed_documents_since_table_call_count(table_name: &str) -> usize {
    let counts = CHANGED_DOCUMENTS_SINCE_TABLE_CALLS
        .get_or_init(|| StdMutex::new(HashMap::new()))
        .lock()
        .unwrap();
    counts.get(table_name).copied().unwrap_or(0)
}

#[cfg(test)]
fn set_test_external_poll_safety_interval_ms(ms: u64) {
    TEST_EXTERNAL_POLL_SAFETY_INTERVAL_MS.store(ms, Ordering::SeqCst);
}

pub use super::metrics::sqlite_runtime_counters_snapshot;
pub(crate) use super::metrics::timed_sqlite_statement;
pub(crate) struct TimedSqliteWriterGuard<'a> {
    guard: parking_lot::MutexGuard<'a, rusqlite::Connection>,
    held_started: Instant,
}

impl Deref for TimedSqliteWriterGuard<'_> {
    type Target = rusqlite::Connection;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl DerefMut for TimedSqliteWriterGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl Drop for TimedSqliteWriterGuard<'_> {
    fn drop(&mut self) {
        record_sqlite_writer_lock_held(self.held_started.elapsed());
    }
}

pub(crate) fn lock_sqlite_writer(
    connection: &SharedSqliteConnection,
) -> TimedSqliteWriterGuard<'_> {
    record_sqlite_writer_lock_acquire();
    let lock_wait_started = Instant::now();
    let guard = connection.lock();
    record_sqlite_writer_lock_wait(lock_wait_started.elapsed());
    TimedSqliteWriterGuard {
        guard,
        held_started: Instant::now(),
    }
}

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

fn is_stale_replicated_business_command_pending_write(
    collection_name: &str,
    context: &str,
    write: &BulkWriteRow,
    document_in_db: &Value,
) -> bool {
    crate::collection_policy::collection_policy()
        .write_guard(collection_name, context)
        .rejects(&write.document, document_in_db)
}

struct TableNotifier {
    notify: Notify,
    generation: AtomicU64,
    local_hook_generation: AtomicU64,
}

impl TableNotifier {
    fn new() -> Self {
        Self {
            notify: Notify::new(),
            generation: AtomicU64::new(0),
            local_hook_generation: AtomicU64::new(0),
        }
    }

    fn signal(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        // A table can have multiple independent observers: the storage
        // change-stream drain plus latency-sensitive consumers such as the
        // Business OS command intake. Waking only one observer made the other
        // wait for its safety timeout even though the shared generation had
        // advanced.
        self.notify.notify_waiters();
    }

    fn signal_local_hook(&self) {
        self.local_hook_generation.fetch_add(1, Ordering::SeqCst);
        self.signal();
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    fn local_hook_generation(&self) -> u64 {
        self.local_hook_generation.load(Ordering::SeqCst)
    }
}

struct TableNotifierRegistryEntry {
    notifier: Arc<TableNotifier>,
    references: usize,
}

static UPDATE_REGISTRY: OnceLock<StdMutex<HashMap<String, TableNotifierRegistryEntry>>> =
    OnceLock::new();

pub(crate) fn database_key_for_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn registry_key(database_key: &str, table_name: &str) -> String {
    format!("{database_key}\0{table_name}")
}

fn register_table_notifier(database_key: &str, table_name: &str) -> Arc<TableNotifier> {
    let mut map = UPDATE_REGISTRY
        .get_or_init(|| StdMutex::new(HashMap::new()))
        .lock()
        .unwrap();
    let entry = map
        .entry(registry_key(database_key, table_name))
        .or_insert_with(|| TableNotifierRegistryEntry {
            notifier: Arc::new(TableNotifier::new()),
            references: 0,
        });
    entry.references = entry.references.saturating_add(1);
    Arc::clone(&entry.notifier)
}

fn unregister_table_notifier(database_key: &str, table_name: &str) {
    if let Some(registry) = UPDATE_REGISTRY.get() {
        let mut map = registry.lock().unwrap();
        let key = registry_key(database_key, table_name);
        let remove = map.get_mut(&key).is_some_and(|entry| {
            entry.references = entry.references.saturating_sub(1);
            entry.references == 0
        });
        if remove {
            map.remove(&key);
        }
    }
}

pub fn notify_table_change(database_key: &str, table_name: &str) -> bool {
    if let Some(registry) = UPDATE_REGISTRY.get() {
        let map = registry.lock().unwrap();
        if let Some(entry) = map.get(&registry_key(database_key, table_name)) {
            entry.notifier.signal_local_hook();
            return true;
        }
    }
    false
}

/// Wakes a live storage instance after another connection in this process
/// committed a write to its physical collection table.
///
/// Direct projection writers know the database path rather than the storage
/// registry's normalized key. Keeping that conversion here avoids coupling
/// callers to the registry-key representation and lets the local-hook
/// generation suppress the later filesystem-watcher echo.
pub fn notify_table_change_for_path(database_path: &Path, table_name: &str) -> bool {
    notify_table_change(&database_key_for_path(database_path), table_name)
}

pub(crate) fn notify_external_table_change(database_key: &str, table_name: &str) -> bool {
    if let Some(registry) = UPDATE_REGISTRY.get() {
        let map = registry.lock().unwrap();
        if let Some(entry) = map.get(&registry_key(database_key, table_name)) {
            entry.notifier.signal();
            return true;
        }
    }
    false
}

pub fn table_change_generation(database_key: &str, table_name: &str) -> Option<u64> {
    let registry = UPDATE_REGISTRY.get()?;
    let map = registry.lock().unwrap();
    map.get(&registry_key(database_key, table_name))
        .map(|entry| entry.notifier.generation())
}

pub(crate) fn table_local_hook_generation(database_key: &str, table_name: &str) -> Option<u64> {
    let registry = UPDATE_REGISTRY.get()?;
    let map = registry.lock().unwrap();
    map.get(&registry_key(database_key, table_name))
        .map(|entry| entry.notifier.local_hook_generation())
}

pub fn table_change_generation_for_path(database_path: &Path, table_name: &str) -> Option<u64> {
    table_change_generation(&database_key_for_path(database_path), table_name)
}

pub async fn wait_for_table_change_for_path(
    database_path: &Path,
    table_name: &str,
    seen_generation: u64,
    timeout_duration: Duration,
) -> u64 {
    let database_key = database_key_for_path(database_path);
    let notifier = UPDATE_REGISTRY.get().and_then(|registry| {
        let map = registry.lock().unwrap();
        map.get(&registry_key(&database_key, table_name))
            .map(|entry| Arc::clone(&entry.notifier))
    });
    let Some(notifier) = notifier else {
        tokio::time::sleep(timeout_duration).await;
        return table_change_generation(&database_key, table_name).unwrap_or(seen_generation);
    };
    loop {
        let notified = notifier.notify.notified();
        tokio::pin!(notified);
        let current_generation = notifier.generation();
        if current_generation != seen_generation {
            return current_generation;
        }
        tokio::select! {
            _ = tokio::time::sleep(timeout_duration) => {
                return notifier.generation();
            }
            _ = &mut notified => {
                let current_generation = notifier.generation();
                if current_generation != seen_generation {
                    return current_generation;
                }
            }
        }
    }
}

pub fn notify_database_change(database_key: &str) {
    if let Some(registry) = UPDATE_REGISTRY.get() {
        let map = registry.lock().unwrap();
        let prefix = format!("{database_key}\0");
        for (key, entry) in map.iter() {
            if key.starts_with(&prefix) {
                entry.notifier.signal();
            }
        }
    }
}

enum ReadOperation {
    FindDocumentsById,
    Query,
    Count,
    ChangedDocumentsSince,
}

enum ReadConnection {
    ReadOnly(SharedSqliteConnection),
    WriterFallback(SharedSqliteConnection),
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
    external_notifier: Arc<TableNotifier>,
    notifier_registered: AtomicBool,
    read_connection: Arc<Mutex<Option<SharedSqliteConnection>>>,
    instance_id: u64,
    #[cfg(test)]
    bulk_write_current_state_batch_loads: Arc<AtomicUsize>,
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
        let changes = RxSubject::with_lag_signal(DEFAULT_SUBJECT_BUFFER, |skipped| {
            Some(EventBulk::rxsubject_lagged(skipped))
        });
        let closed = Arc::new(AtomicBool::new(false));
        let external_checkpoint = Arc::new(Mutex::new({
            let conn = lock_sqlite_writer(&connection);
            latest_checkpoint(&conn, &table_name).unwrap_or_else(|| json!({ "id": "", "lwt": 0 }))
        }));
        let read_connection = Arc::new(Mutex::new(None));

        let notifier = register_table_notifier(&database_key, &table_name);
        // One startup reconciliation closes the gap between the initial
        // checkpoint read and the database-wide data_version watcher baseline.
        notifier.signal();

        start_external_write_poll(ExternalWritePoll {
            connection: Arc::clone(&connection),
            database_path: database_path.clone(),
            table_name: table_name.clone(),
            primary_path: primary_path.clone(),
            changes: changes.clone(),
            closed: Arc::clone(&closed),
            checkpoint: Arc::clone(&external_checkpoint),
            notifier: Arc::clone(&notifier),
            read_connection: Arc::clone(&read_connection),
        });
        let external_notifier = notifier;
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
            external_notifier,
            notifier_registered: AtomicBool::new(true),
            read_connection,
            instance_id: INSTANCE_ID.fetch_add(1, Ordering::SeqCst),
            #[cfg(test)]
            bulk_write_current_state_batch_loads: Arc::new(AtomicUsize::new(0)),
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

    fn unregister_table_notifier_once(&self) {
        if self.notifier_registered.swap(false, Ordering::SeqCst) {
            unregister_table_notifier(&self.database_key, &self.table_name);
        }
    }

    async fn with_read_connection<T, F>(&self, operation: ReadOperation, read: F) -> RxResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&rusqlite::Connection) -> RxResult<T> + Send + 'static,
    {
        let connection = match self.open_read_only_connection() {
            Ok(connection) => ReadConnection::ReadOnly(connection),
            Err(_) => {
                SQLITE_READ_ONLY_OPEN_FAILURES.fetch_add(1, Ordering::Relaxed);
                SQLITE_WRITER_FALLBACKS.fetch_add(1, Ordering::Relaxed);
                #[cfg(test)]
                match operation {
                    ReadOperation::FindDocumentsById => {
                        FIND_DOCUMENTS_BY_ID_WRITER_FALLBACKS.fetch_add(1, Ordering::SeqCst);
                    }
                    ReadOperation::Query => {
                        QUERY_WRITER_FALLBACKS.fetch_add(1, Ordering::SeqCst);
                    }
                    ReadOperation::ChangedDocumentsSince => {
                        CHANGED_DOCUMENTS_SINCE_WRITER_FALLBACKS.fetch_add(1, Ordering::SeqCst);
                    }
                    ReadOperation::Count => {}
                }
                #[cfg(not(test))]
                let _ = operation;

                // In-memory databases cannot be reopened as independent read-only
                // connections. Preserve the legacy shared-writer fallback there and
                // for read-only connection open failures.
                ReadConnection::WriterFallback(Arc::clone(&self.connection))
            }
        };
        tokio::task::spawn_blocking(move || match connection {
            ReadConnection::ReadOnly(connection) => {
                let conn = connection.lock();
                read(&conn)
            }
            ReadConnection::WriterFallback(connection) => {
                let conn = lock_sqlite_writer(&connection);
                read(&conn)
            }
        })
        .await
        .map_err(join_error)?
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
    let conn = lock_sqlite_writer(connection);
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

fn query_operator_families(query: &FilledMangoQuery) -> Vec<String> {
    let mut operators = BTreeSet::new();
    collect_selector_operator_families(&query.selector, &mut operators);
    if operators.is_empty() {
        operators.insert("$none".to_string());
    }
    operators.into_iter().collect()
}

fn collect_selector_operator_families(selector: &Value, operators: &mut BTreeSet<String>) {
    let Some(selector) = selector.as_object() else {
        return;
    };
    for (field_or_operator, matcher) in selector {
        if field_or_operator.starts_with('$') {
            operators.insert(field_or_operator.clone());
            collect_logical_operator_children(matcher, operators);
        } else {
            collect_matcher_operator_families(matcher, operators);
        }
    }
}

fn collect_matcher_operator_families(matcher: &Value, operators: &mut BTreeSet<String>) {
    let Some(matcher) = matcher.as_object() else {
        operators.insert("$eq".to_string());
        return;
    };
    let mut found_operator = false;
    for (operator, value) in matcher {
        if operator.starts_with('$') {
            found_operator = true;
            operators.insert(operator.clone());
            collect_logical_operator_children(value, operators);
        }
    }
    if !found_operator {
        operators.insert("$eq".to_string());
    }
}

fn collect_logical_operator_children(value: &Value, operators: &mut BTreeSet<String>) {
    if let Some(children) = value.as_array() {
        for child in children {
            collect_selector_operator_families(child, operators);
        }
    } else {
        collect_selector_operator_families(value, operators);
    }
}

fn parse_filled_query(prepared_query: &Value) -> RxResult<FilledMangoQuery> {
    serde_json::from_value(prepared_query.get("query").cloned().unwrap_or(Value::Null)).map_err(
        |err| {
            new_rx_error(
                "SQLITE_QUERY",
                Some(json!({ "message": format!("invalid prepared query: {err}") })),
            )
        },
    )
}

struct QueryDocumentsRequest {
    table_name: String,
    collection_name: String,
    primary_ids: Option<Vec<String>>,
    compiled_sql: Option<CompiledSqliteQuery>,
    fallback_candidate_sql: Option<CompiledSqliteQuery>,
    fallback_operator_families: Vec<String>,
    matcher: QueryMatcher,
    comparator: DeterministicSortComparator,
    skip: usize,
    skip_plus_limit: usize,
}

fn execute_query_documents(
    conn: &rusqlite::Connection,
    request: QueryDocumentsRequest,
) -> RxResult<Vec<Value>> {
    let QueryDocumentsRequest {
        table_name,
        collection_name,
        primary_ids,
        compiled_sql,
        fallback_candidate_sql,
        fallback_operator_families,
        matcher,
        comparator,
        skip,
        skip_plus_limit,
    } = request;
    let table_name = table_name.as_str();
    let collection_name = collection_name.as_str();
    if let Some(ids) = primary_ids {
        let mut rows = Vec::new();
        for doc in documents_by_ids(conn, table_name, &ids, true)? {
            if matcher(&doc) {
                rows.push(doc);
            }
        }
        rows.sort_by(|a, b| comparator(a, b));
        let start = skip.min(rows.len());
        let end = skip_plus_limit.min(rows.len());
        return Ok(rows[start..end].to_vec());
    }

    if let Some(compiled) = compiled_sql {
        return query_documents_with_compiled_sql(conn, &compiled);
    }

    SQLITE_QUERY_FALLBACK_CALLS.fetch_add(1, Ordering::Relaxed);
    record_query_fallback_attribution(collection_name, &fallback_operator_families);
    if fallback_candidate_sql.is_some() {
        SQLITE_QUERY_FALLBACK_INDEXED_CANDIDATE_CALLS.fetch_add(1, Ordering::Relaxed);
    }
    let mut visited_rows = 0u64;
    let mut rows: Vec<Value> = Vec::new();
    let mut visit_document = |doc: Value| {
        visited_rows = visited_rows.saturating_add(1);
        if visited_rows > SQLITE_QUERY_FALLBACK_SCAN_LIMIT {
            SQLITE_QUERY_FALLBACK_TOO_BROAD_CALLS.fetch_add(1, Ordering::Relaxed);
            return Err(new_rx_error(
                SQLITE_QUERY_FALLBACK_TOO_BROAD,
                Some(json!({
                    "message": "SQLite Mango fallback scanned too many candidate rows; add an indexable selector or SQL compiler support for this query",
                    "table": table_name,
                    "visitedRows": visited_rows,
                    "scanLimit": SQLITE_QUERY_FALLBACK_SCAN_LIMIT,
                    "usedIndexedCandidatePlan": fallback_candidate_sql.is_some(),
                })),
            ));
        }
        if matcher(&doc) {
            rows.push(doc);
        }
        Ok(true)
    };
    let fallback_result = if let Some(compiled) = &fallback_candidate_sql {
        for_each_document_with_compiled_sql(conn, compiled, &mut visit_document)
    } else {
        for_each_document(conn, table_name, &mut visit_document)
    };
    record_query_fallback_rows(collection_name, &fallback_operator_families, visited_rows);
    fallback_result?;
    rows.sort_by(|a, b| comparator(a, b));
    let start = skip.min(rows.len());
    let end = skip_plus_limit.min(rows.len());
    Ok(rows[start..end].to_vec())
}

struct ExternalWritePoll {
    connection: SharedSqliteConnection,
    database_path: std::path::PathBuf,
    table_name: String,
    primary_path: String,
    changes: RxSubject<EventBulk>,
    closed: Arc<AtomicBool>,
    checkpoint: Arc<Mutex<Value>>,
    notifier: Arc<TableNotifier>,
    read_connection: Arc<Mutex<Option<SharedSqliteConnection>>>,
}

fn start_external_write_poll(poll: ExternalWritePoll) {
    let ExternalWritePoll {
        connection,
        database_path,
        table_name,
        primary_path,
        changes,
        closed,
        checkpoint,
        notifier,
        read_connection,
    } = poll;
    let Ok(_) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let safety_interval = external_poll_safety_interval_for_path(&database_path);
    tokio::spawn(async move {
        let mut seen_generation = 0;
        let mut consecutive_failures = 0u32;
        let mut retry_failed_drain = false;
        loop {
            let mut safety_poll = false;
            if !retry_failed_drain && notifier.generation.load(Ordering::SeqCst) == seen_generation
            {
                if let Some(safety_interval) = safety_interval {
                    tokio::select! {
                        _ = tokio::time::sleep(safety_interval) => {
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
                } else {
                    notifier.notify.notified().await;
                    if closed.load(Ordering::SeqCst) {
                        break;
                    }
                }
            }
            if closed.load(Ordering::SeqCst) {
                break;
            }
            let current_generation = notifier.generation.load(Ordering::SeqCst);
            if current_generation == seen_generation && !safety_poll && !retry_failed_drain {
                continue;
            }
            // FIX 1: run the per-table poll query off the tokio worker thread.
            // Each instance spawns one of these loops; doing the blocking
            // rusqlite read directly on a worker (1-2 on a small VPS) is what
            // starves the heartbeat timer + replication. We move owned clones
            // into `spawn_blocking` and only await the `Send` result here.
            let poll_conn = Arc::clone(&connection);
            let poll_database_path = database_path.clone();
            let poll_table = table_name.clone();
            let poll_checkpoint = checkpoint.lock().clone();
            let poll_read_connection = Arc::clone(&read_connection);
            let result = tokio::task::spawn_blocking(move || {
                if sqlite_path_is_in_memory(&poll_database_path) {
                    let conn = poll_conn.lock();
                    return drain_external_changed_documents_since(
                        &conn,
                        &poll_table,
                        poll_checkpoint,
                    );
                }
                let conn = cached_read_only_connection_for_path(
                    &poll_database_path,
                    &poll_read_connection,
                )?;
                let conn = conn.lock();
                drain_external_changed_documents_since(&conn, &poll_table, poll_checkpoint)
            })
            .await;
            // A broken table or connection must remain observable without
            // turning repeated notifications into a hot retry loop.
            let drain = match result {
                Ok(Ok(drain)) => {
                    seen_generation = current_generation;
                    consecutive_failures = 0;
                    retry_failed_drain = false;
                    drain
                }
                Ok(Err(err)) => {
                    retry_failed_drain = true;
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    record_sqlite_external_poll_drain_failure(&table_name);
                    let backoff = external_poll_failure_backoff(consecutive_failures);
                    eprintln!(
                        "[ctox][sqlite] external change-feed drain failed for table {} (consecutive failures: {}, retry backoff: {:?}): {:?}",
                        table_name, consecutive_failures, backoff, err
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                Err(err) => {
                    retry_failed_drain = true;
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    record_sqlite_external_poll_drain_failure(&table_name);
                    let backoff = external_poll_failure_backoff(consecutive_failures);
                    eprintln!(
                        "[ctox][sqlite] external change-feed blocking task failed for table {} (consecutive failures: {}, retry backoff: {:?}): {}",
                        table_name, consecutive_failures, backoff, err
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }
            };
            if drain.batches.is_empty() {
                *checkpoint.lock() = drain.checkpoint;
                continue;
            }
            let drained_to_empty = drain.drained_to_empty;
            *checkpoint.lock() = drain.checkpoint.clone();
            for batch in drain.batches {
                let events = batch
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
                if !events.is_empty() {
                    changes.next(EventBulk {
                        id: random_token(Some(10)),
                        events,
                        checkpoint: Some(batch.checkpoint),
                        context: Some("sqlite-external-poll".to_string()),
                    });
                }
            }
            if !drained_to_empty && !closed.load(Ordering::SeqCst) {
                notifier.signal();
            }
        }
    });
}

fn external_poll_safety_interval_for_path(database_path: &Path) -> Option<Duration> {
    if sqlite_path_is_in_memory(database_path) {
        Some(external_poll_safety_interval())
    } else {
        None
    }
}

fn external_poll_safety_interval() -> Duration {
    #[cfg(test)]
    {
        let override_ms = TEST_EXTERNAL_POLL_SAFETY_INTERVAL_MS.load(Ordering::SeqCst);
        if override_ms > 0 {
            return Duration::from_millis(override_ms);
        }
    }
    SQLITE_EXTERNAL_POLL_SAFETY_INTERVAL
}

fn external_poll_failure_backoff(consecutive_failures: u32) -> Duration {
    if consecutive_failures == 0 {
        return Duration::ZERO;
    }
    let exponent = consecutive_failures.saturating_sub(1).min(31);
    SQLITE_EXTERNAL_POLL_FAILURE_BACKOFF_BASE
        .checked_mul(1u32 << exponent)
        .unwrap_or(SQLITE_EXTERNAL_POLL_FAILURE_BACKOFF_MAX)
        .min(SQLITE_EXTERNAL_POLL_FAILURE_BACKOFF_MAX)
}

fn open_read_only_connection_for_path(path: &Path) -> RxResult<rusqlite::Connection> {
    use rusqlite::OpenFlags;
    SQLITE_READ_ONLY_OPEN_CALLS.fetch_add(1, Ordering::Relaxed);
    let conn = rusqlite::Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(super::types::sqlite_error)?;
    conn.busy_timeout(std::time::Duration::from_secs(10))
        .map_err(super::types::sqlite_error)?;
    Ok(conn)
}

fn open_dedicated_read_only_connection_for_path(path: &Path) -> RxResult<rusqlite::Connection> {
    if sqlite_path_is_in_memory(path) {
        return Err(sqlite_concurrent_reader_error());
    }
    open_read_only_connection_for_path(path)
}

fn cached_read_only_connection_for_path(
    path: &Path,
    cache: &Arc<Mutex<Option<SharedSqliteConnection>>>,
) -> RxResult<SharedSqliteConnection> {
    if sqlite_path_is_in_memory(path) {
        return Err(sqlite_concurrent_reader_error());
    }
    let mut guard = cache.lock();
    if let Some(existing) = guard.as_ref() {
        return Ok(Arc::clone(existing));
    }
    let shared = Arc::new(Mutex::new(open_read_only_connection_for_path(path)?));
    *guard = Some(Arc::clone(&shared));
    Ok(shared)
}

struct ExternalPollDrain {
    batches: Vec<RxStorageChangedDocumentsSinceResult>,
    checkpoint: Value,
    drained_to_empty: bool,
}

fn drain_external_changed_documents_since(
    conn: &rusqlite::Connection,
    table_name: &str,
    initial_checkpoint: Value,
) -> RxResult<ExternalPollDrain> {
    let poll_limit = crate::collection_policy::collection_policy()
        .poll_batch_limit(table_name)
        .map(|limit| limit as u64)
        .unwrap_or(SQLITE_EXTERNAL_POLL_DEFAULT_LIMIT);
    let mut checkpoint = initial_checkpoint;
    let mut batches = Vec::new();
    let mut rows = 0usize;
    let mut empty_batches = 0usize;
    for _ in 0..SQLITE_EXTERNAL_POLL_MAX_BATCHES_PER_WAKE {
        let result = changed_documents_since(conn, table_name, poll_limit, Some(&checkpoint))?;
        checkpoint = result.checkpoint.clone();
        if result.documents.is_empty() {
            empty_batches += 1;
            record_sqlite_external_poll_drain(table_name, batches.len(), empty_batches, rows, true);
            return Ok(ExternalPollDrain {
                batches,
                checkpoint,
                drained_to_empty: true,
            });
        }
        rows = rows.saturating_add(result.documents.len());
        batches.push(result);
    }
    record_sqlite_external_poll_drain(table_name, batches.len(), empty_batches, rows, false);
    Ok(ExternalPollDrain {
        batches,
        checkpoint,
        drained_to_empty: false,
    })
}

fn latest_checkpoint(conn: &rusqlite::Connection, table_name: &str) -> Option<Value> {
    let _statement_timer = timed_sqlite_statement();
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
    limit: u64,
    checkpoint: Option<&Value>,
) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
    SQLITE_CHANGED_DOCUMENTS_SINCE_CALLS.fetch_add(1, Ordering::Relaxed);
    #[cfg(test)]
    {
        let mut counts = CHANGED_DOCUMENTS_SINCE_TABLE_CALLS
            .get_or_init(|| StdMutex::new(HashMap::new()))
            .lock()
            .unwrap();
        *counts.entry(table_name.to_string()).or_insert(0) += 1;
    }

    let since_lwt = checkpoint
        .and_then(|checkpoint| checkpoint.get("lwt"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let since_id = checkpoint
        .and_then(|checkpoint| checkpoint.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let _statement_timer = timed_sqlite_statement();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT data, id, lastWriteTime FROM {} WHERE lastWriteTime > ? OR (lastWriteTime = ? AND id > ?) ORDER BY lastWriteTime ASC, id ASC LIMIT ?",
            quote_identifier(table_name)
        ))
        .map_err(sqlite_error)?;
    let rows = stmt
        .query_map(
            params![since_lwt, since_lwt, since_id, limit as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            },
        )
        .map_err(sqlite_error)?;
    let mut documents = Vec::new();
    let mut latest_checkpoint: Option<Value> = None;
    for row in rows {
        let (data, id, lwt) = row.map_err(sqlite_error)?;
        documents.push(serde_json::from_str::<Value>(&data).map_err(|err| {
            new_rx_error("SQLITE_JSON", Some(json!({ "message": err.to_string() })))
        })?);
        latest_checkpoint = Some(json!({ "id": id, "lwt": lwt }));
    }
    let checkpoint = latest_checkpoint
        .or_else(|| checkpoint.cloned())
        .unwrap_or_else(|| json!({ "id": "", "lwt": 0 }));
    SQLITE_CHANGED_DOCUMENTS_SINCE_RESULTS.fetch_add(documents.len() as u64, Ordering::Relaxed);
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
        SQLITE_BULK_WRITE_CALLS.fetch_add(1, Ordering::Relaxed);
        SQLITE_BULK_WRITE_ROWS.fetch_add(document_writes.len() as u64, Ordering::Relaxed);

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
        let collection_name = self.collection_name.clone();
        let context = context.to_string();
        #[cfg(test)]
        let bulk_write_current_state_batch_loads =
            Arc::clone(&self.bulk_write_current_state_batch_loads);

        let (error, event_bulk, checkpoint): (
            Vec<crate::types::RxStorageWriteError>,
            Option<EventBulk>,
            Option<Value>,
        ) = tokio::task::spawn_blocking(move || -> RxResult<_> {
            let mut conn = lock_sqlite_writer(&connection);
            let result = (|| -> RxResult<_> {
                SQLITE_WRITE_TRANSACTIONS_STARTED.fetch_add(1, Ordering::Relaxed);
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
                    // global write mutex), the dominant scaling risk for large
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
                    #[cfg(test)]
                    bulk_write_current_state_batch_loads.fetch_add(1, Ordering::SeqCst);
                    for doc in documents_by_ids(&tx, &table_name, &ids, true)? {
                        if let Some(id) = doc.get(&primary_path).and_then(Value::as_str) {
                            docs_in_db.insert(id.to_string(), doc);
                        }
                    }
                }
                let mut server_owned_errors: Vec<crate::types::RxStorageWriteError> = Vec::new();
                if crate::collection_policy::collection_policy()
                    .write_guard(&collection_name, &context)
                    .is_active()
                {
                    let mut kept = Vec::with_capacity(document_writes.len());
                    for write in document_writes.drain(..) {
                        let document_id = write
                            .document
                            .get(&primary_path)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        if let Some(in_db) = docs_in_db.get(&document_id) {
                            if is_stale_replicated_business_command_pending_write(
                                &collection_name,
                                &context,
                                &write,
                                in_db,
                            ) {
                                server_owned_errors.push(crate::types::RxStorageWriteError {
                                    status: 409,
                                    is_error: true,
                                    document_id,
                                    write_row: write,
                                    document_in_db: Some(in_db.clone()),
                                    validation_errors: Vec::new(),
                                    schema: None,
                                    attachment_id: None,
                                });
                                continue;
                            }
                        }
                        kept.push(write);
                    }
                    document_writes = kept;
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
                error.extend(server_owned_errors);

                for row in categorized.bulk_insert_docs.iter() {
                    insert_document(&tx, &table_name, &primary_path, &row.document)?;
                }
                for row in categorized.bulk_update_docs.iter() {
                    update_document(&tx, &table_name, &primary_path, row)?;
                }
                tx.commit().map_err(sqlite_error)?;
                SQLITE_WRITE_TRANSACTIONS_COMMITTED.fetch_add(1, Ordering::Relaxed);

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
            })();
            if result.is_err() {
                SQLITE_WRITE_TRANSACTIONS_FAILED.fetch_add(1, Ordering::Relaxed);
            }
            result
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
        SQLITE_FIND_DOCUMENTS_BY_ID_CALLS.fetch_add(1, Ordering::Relaxed);
        SQLITE_FIND_DOCUMENTS_BY_ID_REQUESTED.fetch_add(ids.len() as u64, Ordering::Relaxed);
        let table_name = self.table_name.clone();
        let ids = ids.to_vec();
        let documents = self
            .with_read_connection(ReadOperation::FindDocumentsById, move |conn| {
                documents_by_ids(conn, &table_name, &ids, with_deleted)
            })
            .await?;
        SQLITE_FIND_DOCUMENTS_BY_ID_RESULTS.fetch_add(documents.len() as u64, Ordering::Relaxed);
        Ok(documents)
    }

    async fn query_stream_into(
        &self,
        prepared_query: &Value,
        chunk_size: usize,
        on_batch: &mut (dyn FnMut(Vec<Value>) -> Result<bool, RxError> + Send),
    ) -> Result<(), RxError> {
        self.ensure_open("query_stream")?;
        SQLITE_QUERY_STREAM_CALLS.fetch_add(1, Ordering::Relaxed);

        let database_path = self.database_path.clone();
        let table_name = self.table_name.clone();
        let collection_name = self.collection_name.clone();
        let primary_path = self.primary_path.clone();
        let prepared_query = prepared_query.clone();
        // The cursor can outlive many async scheduling quanta. Keeping it on
        // Tokio's blocking pool prevents a slow consumer or large result set
        // from starving replication timers on small runtimes.
        let (batch_sender, mut batch_receiver) = mpsc::channel::<Vec<Value>>(1);
        let (decision_sender, decision_receiver) = std::sync::mpsc::channel::<bool>();
        let stream_task = tokio::task::spawn_blocking(move || {
            query_stream_on_dedicated_connection(
                &database_path,
                &table_name,
                &collection_name,
                &primary_path,
                &prepared_query,
                chunk_size,
                |batch| {
                    if batch_sender.blocking_send(batch).is_err() {
                        return Ok(false);
                    }
                    Ok(decision_receiver.recv().unwrap_or(false))
                },
            )
        });

        let mut callback_error = None;
        while let Some(batch) = batch_receiver.recv().await {
            match on_batch(batch) {
                Ok(true) => {
                    if decision_sender.send(true).is_err() {
                        break;
                    }
                }
                Ok(false) => {
                    let _ = decision_sender.send(false);
                    batch_receiver.close();
                    break;
                }
                Err(err) => {
                    callback_error = Some(err);
                    let _ = decision_sender.send(false);
                    batch_receiver.close();
                    break;
                }
            }
        }
        let stream_result = stream_task.await.map_err(join_error)?;
        if let Some(err) = callback_error {
            return Err(err);
        }
        stream_result
    }

    fn query_stream_into_blocking(
        &self,
        prepared_query: &Value,
        chunk_size: usize,
        on_batch: &mut (dyn FnMut(Vec<Value>) -> Result<bool, RxError> + Send),
    ) -> Option<Result<(), RxError>> {
        Some(self.query_stream(prepared_query, chunk_size, on_batch))
    }

    async fn query(&self, prepared_query: &Value) -> Result<RxStorageQueryResult, RxError> {
        self.ensure_open("query")?;
        SQLITE_QUERY_CALLS.fetch_add(1, Ordering::Relaxed);
        let query = parse_filled_query(prepared_query)?;
        let skip = query.skip.unwrap_or(0) as usize;
        let limit = query
            .limit
            .map(|limit| limit as usize)
            .unwrap_or(usize::MAX);
        let skip_plus_limit = skip.saturating_add(limit);
        let matcher = get_query_matcher(&self.schema, &query);
        let comparator = get_sort_comparator(&self.schema, &query);
        let primary_ids = primary_key_selector_ids(&query, &self.primary_path);
        let fallback_operator_families = query_operator_families(&query);
        let compiled_sql = if primary_ids.is_none() {
            compile_query_sql(&self.table_name, &self.primary_path, &query)
        } else {
            None
        };
        let fallback_candidate_sql = if primary_ids.is_none() && compiled_sql.is_none() {
            prepared_query
                .get("queryPlan")
                .cloned()
                .and_then(|value| serde_json::from_value::<RxQueryPlan>(value).ok())
                .and_then(|plan| {
                    compile_query_plan_candidate_sql(&self.table_name, &self.primary_path, &plan)
                })
        } else {
            None
        };

        let table_name = self.table_name.clone();
        let collection_name = self.collection_name.clone();
        let documents = self
            .with_read_connection(ReadOperation::Query, move |conn| {
                execute_query_documents(
                    conn,
                    QueryDocumentsRequest {
                        table_name,
                        collection_name,
                        primary_ids,
                        compiled_sql,
                        fallback_candidate_sql,
                        fallback_operator_families,
                        matcher,
                        comparator,
                        skip,
                        skip_plus_limit,
                    },
                )
            })
            .await?;
        SQLITE_QUERY_RESULTS.fetch_add(documents.len() as u64, Ordering::Relaxed);
        Ok(RxStorageQueryResult { documents })
    }

    async fn count(&self, prepared_query: &Value) -> Result<RxStorageCountResult, RxError> {
        self.ensure_open("count")?;
        SQLITE_COUNT_CALLS.fetch_add(1, Ordering::Relaxed);
        let query = parse_filled_query(prepared_query)?;
        if let Some(compiled) = compile_count_sql(&self.table_name, &self.primary_path, &query) {
            let count = self
                .with_read_connection(ReadOperation::Count, move |conn| {
                    count_with_compiled_sql(conn, &compiled)
                })
                .await?;
            return Ok(RxStorageCountResult {
                count,
                mode: "fast".to_string(),
            });
        }
        SQLITE_COUNT_FALLBACK_QUERY_CALLS.fetch_add(1, Ordering::Relaxed);
        let result = self.query(prepared_query).await?;
        Ok(RxStorageCountResult {
            count: result.documents.len() as u64,
            mode: "slow".to_string(),
        })
    }

    async fn get_changed_documents_since(
        &self,
        limit: u64,
        checkpoint: Option<&Value>,
    ) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
        self.ensure_open("get_changed_documents_since")?;
        let table_name = self.table_name.clone();
        let checkpoint = checkpoint.cloned();
        self.with_read_connection(ReadOperation::ChangedDocumentsSince, move |conn| {
            changed_documents_since(conn, &table_name, limit, checkpoint.as_ref())
        })
        .await
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
            let conn = lock_sqlite_writer(&connection);
            drop_table(&conn, &table_name)
        })
        .await
        .map_err(join_error)??;
        self.closed.store(true, Ordering::SeqCst);
        self.external_notifier.signal();
        self.unregister_table_notifier_once();
        Ok(())
    }

    async fn close(&self) -> Result<(), RxError> {
        self.closed.store(true, Ordering::SeqCst);
        self.external_notifier.signal();
        self.unregister_table_notifier_once();
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
            "SQLITE_ATTACHMENT",
            Some(json!({
                "message": "sqlite storage does not inline attachment payloads"
            })),
        ))
    }
}

impl Drop for RxStorageInstanceSqlite {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
        self.external_notifier.signal();
        self.unregister_table_notifier_once();
    }
}

fn query_stream_on_dedicated_connection<F>(
    database_path: &Path,
    table_name: &str,
    collection_name: &str,
    primary_path: &str,
    prepared_query: &Value,
    chunk_size: usize,
    mut visit: F,
) -> RxResult<()>
where
    F: FnMut(Vec<Value>) -> RxResult<bool>,
{
    let query = parse_filled_query(prepared_query)?;
    // A stream deliberately does not borrow the cached point-read connection:
    // its cursor stays open across callbacks and must not serialize unrelated reads.
    let read_conn = match open_dedicated_read_only_connection_for_path(database_path) {
        Ok(conn) => conn,
        Err(err) => {
            SQLITE_READ_ONLY_OPEN_FAILURES.fetch_add(1, Ordering::Relaxed);
            return Err(err);
        }
    };

    if let Some(compiled) = compile_query_sql(table_name, primary_path, &query) {
        let chunk = chunk_size.max(1);
        let mut batch = Vec::with_capacity(chunk);
        let mut keep_streaming = true;
        for_each_document_with_compiled_sql(&read_conn, &compiled, |doc| {
            SQLITE_QUERY_STREAM_RESULTS.fetch_add(1, Ordering::Relaxed);
            #[cfg(test)]
            std::thread::sleep(test_query_stream_row_delay(table_name));
            batch.push(doc);
            if batch.len() >= chunk {
                let emit = std::mem::take(&mut batch);
                keep_streaming = visit(emit)?;
                if !keep_streaming {
                    return Ok(false);
                }
            }
            Ok(true)
        })?;
        if keep_streaming && !batch.is_empty() && !visit(batch)? {
            return Ok(());
        }
        return Ok(());
    }

    SQLITE_QUERY_STREAM_UNSUPPORTED_CALLS.fetch_add(1, Ordering::Relaxed);
    Err(new_rx_error(
        SQLITE_QUERY_STREAM_UNSUPPORTED,
        Some(json!({
            "message": "query_stream requires a SQL-compilable Mango query; refusing Rust matcher fallback on the WebRTC query-fetch hot path",
            "collection": collection_name,
            "table": table_name,
        })),
    ))
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
        visit: F,
    ) -> RxResult<()>
    where
        F: FnMut(Vec<Value>) -> RxResult<bool>,
    {
        self.ensure_open("query_stream")?;
        SQLITE_QUERY_STREAM_CALLS.fetch_add(1, Ordering::Relaxed);
        query_stream_on_dedicated_connection(
            &self.database_path,
            &self.table_name,
            &self.collection_name,
            &self.primary_path,
            prepared_query,
            chunk_size,
            visit,
        )
    }

    fn open_read_only_connection(&self) -> RxResult<SharedSqliteConnection> {
        let path = &self.database_path;
        // SQLite supports `:memory:` only for the connection that created
        // it, so point reads retain the legacy writer fallback there. For
        // file-backed DBs, WAL mode gives us a real concurrent reader.
        if sqlite_path_is_in_memory(path) {
            return Err(sqlite_concurrent_reader_error());
        }
        cached_read_only_connection_for_path(path, &self.read_connection)
    }
}

// clippy::int_plus_one is allowed here on purpose: these counter assertions read
// "at least N more than before" and sit next to siblings with `+ 3` / `+ SCAN_LIMIT + 1`.
// Rewriting the `+ 1` cases to `>` would make one member of each group inconsistent
// with the others and obscure the intent, for no semantic gain.
#[allow(clippy::int_plus_one)]
#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::{HashMap, HashSet};

    use crate::rx_query_helper::{normalize_mango_query, prepare_query};
    use crate::storage::sqlite::sql::{
        reset_sqlite_json_document_decode_count, sqlite_json_document_decode_count,
    };
    use crate::storage::sqlite::{
        create_storage_instance, get_rx_storage_sqlite, RxStorageSqliteSettings,
    };
    use crate::types::{JsonSchema, MangoQuery, PrimaryKey, RxStorageInstanceCreationParams};

    fn runtime_counter(name: &str) -> u64 {
        sqlite_runtime_counters_snapshot()
            .get(name)
            .and_then(Value::as_u64)
            .unwrap_or(0)
    }

    fn runtime_counter_pointer(pointer: &str) -> u64 {
        sqlite_runtime_counters_snapshot()
            .pointer(pointer)
            .and_then(Value::as_u64)
            .unwrap_or(0)
    }

    fn runtime_counter_map_value(map_name: &str, key: &str) -> u64 {
        sqlite_runtime_counters_snapshot()
            .get(map_name)
            .and_then(Value::as_object)
            .and_then(|map| map.get(key))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    }

    fn test_schema() -> RxJsonSchema {
        let mut properties = HashMap::new();
        properties.insert(
            "id".to_string(),
            JsonSchema {
                schema_type: Some("string".into()),
                ..Default::default()
            },
        );
        properties.insert(
            "age".to_string(),
            JsonSchema {
                schema_type: Some("number".into()),
                ..Default::default()
            },
        );
        properties.insert(
            "_deleted".to_string(),
            JsonSchema {
                schema_type: Some("boolean".into()),
                ..Default::default()
            },
        );
        properties.insert(
            "_rev".to_string(),
            JsonSchema {
                schema_type: Some("string".into()),
                ..Default::default()
            },
        );
        let mut meta_properties = HashMap::new();
        meta_properties.insert(
            "lwt".to_string(),
            JsonSchema {
                schema_type: Some("number".into()),
                ..Default::default()
            },
        );
        properties.insert(
            "_meta".to_string(),
            JsonSchema {
                schema_type: Some("object".into()),
                properties: meta_properties,
                ..Default::default()
            },
        );
        properties.insert(
            "_attachments".to_string(),
            JsonSchema {
                schema_type: Some("object".into()),
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

    fn business_command_params(schema: RxJsonSchema) -> RxStorageInstanceCreationParams {
        let mut params = params(schema);
        params.collection_name = "business_commands".to_string();
        params
    }

    fn command_doc(id: &str, rev: &str, status: &str, lwt: f64, result: Option<Value>) -> Value {
        let mut document = doc(id, rev, 1, false, lwt);
        let object = document.as_object_mut().expect("command doc object");
        object.insert("command_id".to_string(), Value::String(id.to_string()));
        object.insert("status".to_string(), Value::String(status.to_string()));
        if let Some(result) = result {
            object.insert("result".to_string(), result);
        }
        document
    }

    #[tokio::test]
    async fn table_notifier_wakes_all_registered_observers() {
        struct TestWake;
        impl std::task::Wake for TestWake {
            fn wake(self: Arc<Self>) {}
        }

        let notifier = TableNotifier::new();
        let first = notifier.notify.notified();
        let second = notifier.notify.notified();
        tokio::pin!(first);
        tokio::pin!(second);
        let waker = std::task::Waker::from(Arc::new(TestWake));
        let mut context = std::task::Context::from_waker(&waker);
        assert!(std::future::Future::poll(first.as_mut(), &mut context).is_pending());
        assert!(std::future::Future::poll(second.as_mut(), &mut context).is_pending());

        notifier.signal();

        tokio::time::timeout(Duration::from_millis(100), async {
            first.await;
            second.await;
        })
        .await
        .expect("one table change must wake every registered observer");
    }

    #[tokio::test]
    async fn replicated_business_command_pending_write_cannot_regress_server_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let instance = create_storage_instance(&storage, business_command_params(test_schema()))
            .await
            .unwrap();
        let completed = command_doc(
            "cmd_device_code",
            "2-completed",
            "completed",
            2.0,
            Some(json!({ "user_code": "T26Z-KTVTF" })),
        );
        instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: completed.clone(),
                }],
                "seed-completed-command",
            )
            .await
            .unwrap();

        let stale_pending = command_doc("cmd_device_code", "3-pending", "pending_sync", 3.0, None);
        let response = instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: Some(completed.clone()),
                    document: stale_pending,
                }],
                "replication-master-write",
            )
            .await
            .unwrap();

        assert_eq!(response.error.len(), 1);
        assert_eq!(response.error[0].status, 409);
        assert_eq!(
            response.error[0]
                .document_in_db
                .as_ref()
                .and_then(|doc| doc.get("status"))
                .and_then(Value::as_str),
            Some("completed")
        );

        let stored = instance
            .find_documents_by_id(&["cmd_device_code".to_string()], true)
            .await
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(
            stored[0].get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            stored[0]
                .get("result")
                .and_then(|result| result.get("user_code"))
                .and_then(Value::as_str),
            Some("T26Z-KTVTF")
        );
    }

    #[tokio::test]
    async fn sqlite_runtime_counters_report_statement_and_writer_lock_timing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ctox.sqlite3");
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: path,
        });
        let instance = create_storage_instance(&storage, params(test_schema()))
            .await
            .unwrap();

        let statements_before = runtime_counter("statements_executed");
        let statement_ns_before = runtime_counter("statement_elapsed_ns_total");
        let lock_acquires_before = runtime_counter("writer_lock_acquire_calls");
        let lock_wait_ns_before = runtime_counter("writer_lock_wait_ns_total");
        let lock_held_ns_before = runtime_counter("writer_lock_held_ns_total");

        instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("timing", "1-timing", 1, false, 1.0),
                }],
                "timing-counter-test",
            )
            .await
            .unwrap();

        assert!(
            runtime_counter("statements_executed") > statements_before,
            "SQLite statement counter must advance for real storage work"
        );
        assert!(
            runtime_counter("statement_elapsed_ns_total") > statement_ns_before,
            "SQLite statement elapsed time must be visible in runtime counters"
        );
        assert!(
            runtime_counter("writer_lock_acquire_calls") > lock_acquires_before,
            "SQLite writer lock acquisition counter must advance for writes"
        );
        assert!(
            runtime_counter("writer_lock_wait_ns_total") > lock_wait_ns_before,
            "SQLite writer lock wait time must be visible in runtime counters"
        );
        assert!(
            runtime_counter("writer_lock_held_ns_total") > lock_held_ns_before,
            "SQLite writer lock held time must be visible in runtime counters"
        );
    }

    struct ExternalPollSafetyIntervalReset;

    impl Drop for ExternalPollSafetyIntervalReset {
        fn drop(&mut self) {
            set_test_external_poll_safety_interval_ms(0);
        }
    }

    struct QueryStreamRowDelayReset(String);

    impl Drop for QueryStreamRowDelayReset {
        fn drop(&mut self) {
            set_test_query_stream_row_delay_ms(&self.0, 0);
        }
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
    async fn cleanup_deletes_in_bounded_batches_and_reports_when_more_remains() {
        // The unbounded statement was a live hazard, not a style question: on a
        // measured customer instance 301.449 tombstones sat in one table, and
        // deleting them in a single statement holds the write lock against every
        // replication write. The bool "am I done" is part of the upstream
        // contract and used to always be true.
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let instance = create_storage_instance(&storage, params(test_schema()))
            .await
            .unwrap();

        const TOMBSTONES: usize = 2_500;
        let rows = (0..TOMBSTONES)
            .map(|index| BulkWriteRow {
                previous: None,
                document: doc(&format!("gone-{index}"), "1-a", 1, true, 1.0),
            })
            .collect::<Vec<_>>();
        instance.bulk_write(rows, "seed").await.unwrap();
        instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("alive", "1-a", 1, false, 1.0),
                }],
                "seed",
            )
            .await
            .unwrap();

        // First pass hits the batch cap, so it must say it is not finished.
        assert!(
            !instance.cleanup(0).await.unwrap(),
            "a full batch must report that tombstones remain"
        );
        // Second pass clears the rest and reports completion.
        assert!(
            instance.cleanup(0).await.unwrap(),
            "the remaining tombstones fit in one batch, so cleanup is done"
        );
        assert!(
            instance.cleanup(0).await.unwrap(),
            "a clean collection stays done"
        );

        let survivors = instance
            .find_documents_by_id(&["alive".to_string(), "gone-0".to_string()], true)
            .await
            .unwrap();
        assert_eq!(
            survivors.len(),
            1,
            "cleanup must remove tombstones and keep live documents"
        );
    }

    #[tokio::test]
    async fn find_documents_by_id_file_backed_uses_read_only_connection() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let instance = create_storage_instance(&storage, params(test_schema()))
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
                        document: doc("b", "1-b", 2, false, 2.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("deleted", "1-c", 3, true, 3.0),
                    },
                ],
                "seed",
            )
            .await
            .unwrap();

        FIND_DOCUMENTS_BY_ID_WRITER_FALLBACKS.store(0, Ordering::SeqCst);
        let docs = instance
            .find_documents_by_id(
                &[
                    "missing".to_string(),
                    "b".to_string(),
                    "a".to_string(),
                    "b".to_string(),
                    "deleted".to_string(),
                ],
                false,
            )
            .await
            .unwrap();
        let ids = docs
            .iter()
            .filter_map(|doc| doc.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["b", "a", "b"]);
        assert_eq!(
            FIND_DOCUMENTS_BY_ID_WRITER_FALLBACKS.load(Ordering::SeqCst),
            0,
            "file-backed find_documents_by_id must not use the shared writer connection fallback"
        );

        let deleted = instance
            .find_documents_by_id(&["deleted".to_string()], true)
            .await
            .unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(
            FIND_DOCUMENTS_BY_ID_WRITER_FALLBACKS.load(Ordering::SeqCst),
            0,
            "with_deleted lookup must also stay on the read-only connection"
        );
    }

    #[tokio::test]
    async fn file_backed_reads_reuse_cached_read_only_connection() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let instance = create_storage_instance(&storage, params(test_schema()))
            .await
            .unwrap();
        instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("a", "1-a", 1, false, 1.0),
                }],
                "seed",
            )
            .await
            .unwrap();

        let first = instance.open_read_only_connection().unwrap();
        let second = instance.open_read_only_connection().unwrap();
        assert!(
            std::sync::Arc::ptr_eq(&first, &second),
            "file-backed storage must reuse one read-only connection per instance"
        );

        instance
            .find_documents_by_id(&["a".to_string()], false)
            .await
            .unwrap();
        instance
            .get_changed_documents_since(10, None)
            .await
            .unwrap();

        // Anchored to this instance's cached connection rather than the
        // process-wide `read_only_open_calls` counter: that counter is shared
        // by every test in the binary, so a zero delta across these two reads
        // silently assumed no other parallel test opened a read-only
        // connection in the same window. Comparing the cached handle proves
        // the same property — the hot read paths did not reopen and re-cache
        // this instance's read-only connection — without depending on what
        // the rest of the suite happens to be doing.
        let after_reads = instance.open_read_only_connection().unwrap();
        assert!(
            std::sync::Arc::ptr_eq(&first, &after_reads),
            "hot read paths must not reopen read-only SQLite connections"
        );
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
        let batch_loads_before = instance
            .bulk_write_current_state_batch_loads
            .load(Ordering::SeqCst);
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
        assert_eq!(
            instance
                .bulk_write_current_state_batch_loads
                .load(Ordering::SeqCst)
                - batch_loads_before,
            1,
            "bulk_write current-state load should execute exactly one instance-scoped batched ids lookup"
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
    async fn query_indexed_selector_pushes_filter_and_window_into_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let rows: Vec<BulkWriteRow> = (0..1_000)
            .map(|idx| BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{idx:04}"), "1-a", idx, false, idx as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "asc".to_string());
        let filled = normalize_mango_query(
            &schema,
            MangoQuery {
                selector: Some(json!({ "age": { "$gte": 990 } })),
                sort: Some(vec![sort]),
                index: None,
                limit: Some(3),
                skip: Some(0),
            },
        );
        let compiled = compile_query_sql(&instance.table_name, &instance.primary_path, &filled)
            .expect("age selector should compile to SQL");
        let conn = storage.connection().unwrap();
        let conn = conn.lock();
        reset_sqlite_json_document_decode_count();
        let documents = query_documents_with_compiled_sql(&conn, &compiled).unwrap();
        let ages = documents
            .iter()
            .map(|doc| doc.get("age").and_then(Value::as_i64).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ages, vec![990, 991, 992]);
        assert_eq!(
            sqlite_json_document_decode_count(),
            3,
            "indexed LIMIT 3 query must decode only returned rows, not the whole table"
        );

        let count_filled = normalize_mango_query(
            &schema,
            MangoQuery {
                selector: Some(json!({ "age": { "$gte": 990 } })),
                sort: None,
                index: None,
                limit: None,
                skip: None,
            },
        );
        let count_prepared = prepare_query(&schema, count_filled).unwrap();
        let count_query: FilledMangoQuery =
            serde_json::from_value(count_prepared.get("query").cloned().unwrap_or(Value::Null))
                .unwrap();
        let count_compiled =
            compile_count_sql(&instance.table_name, &instance.primary_path, &count_query)
                .expect("age count should compile to SQL");
        reset_sqlite_json_document_decode_count();
        let count = count_with_compiled_sql(&conn, &count_compiled).unwrap();
        assert_eq!(count, 10);
        assert_eq!(
            sqlite_json_document_decode_count(),
            0,
            "compiled COUNT(*) must not deserialize matching documents"
        );

        let mut statement = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {}", compiled.sql))
            .unwrap();
        let plan = statement
            .query_map(rusqlite::params_from_iter(compiled.params.iter()), |row| {
                row.get::<_, String>(3)
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n");
        assert!(
            plan.contains("_json_age_idx"),
            "expected SQLite to use the schema expression index, got plan:\n{plan}"
        );
    }

    // clippy::await_holding_lock is allowed here deliberately: holding the writer
    // mutex across the await IS the subject of this test — it proves the read path
    // proceeds while a writer holds the lock. Releasing it would delete the premise.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn compiled_query_and_count_do_not_wait_for_writer_mutex() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let rows: Vec<BulkWriteRow> = (0..100)
            .map(|idx| BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{idx:03}"), "1-a", idx, false, idx as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "asc".to_string());
        let prepared = prepare_query(
            &schema,
            normalize_mango_query(
                &schema,
                MangoQuery {
                    selector: Some(json!({ "age": { "$gte": 95 } })),
                    sort: Some(vec![sort]),
                    index: None,
                    limit: Some(2),
                    skip: Some(0),
                },
            ),
        )
        .unwrap();

        let shared_conn = storage.connection().unwrap();
        let _writer_guard = shared_conn.lock();

        let query_instance = Arc::clone(&instance);
        let query_prepared = prepared.clone();
        let (query_tx, mut query_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let _ = query_tx.send(query_instance.query(&query_prepared).await);
        });
        let query_result = tokio::time::timeout(Duration::from_secs(1), &mut query_rx)
            .await
            .expect("compiled query waited for shared writer mutex")
            .expect("compiled query task dropped")
            .unwrap();
        let ages = query_result
            .documents
            .iter()
            .map(|doc| doc.get("age").and_then(Value::as_i64).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ages, vec![95, 96]);

        let count_instance = Arc::clone(&instance);
        let count_prepared = prepared.clone();
        let (count_tx, mut count_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let _ = count_tx.send(count_instance.count(&count_prepared).await);
        });
        let count_result = tokio::time::timeout(Duration::from_secs(1), &mut count_rx)
            .await
            .expect("compiled count waited for shared writer mutex")
            .expect("compiled count task dropped")
            .unwrap();
        assert_eq!(count_result.count, 2);
    }

    // clippy::await_holding_lock is allowed here deliberately: holding the writer
    // mutex across the await IS the subject of this test — it proves the read path
    // proceeds while a writer holds the lock. Releasing it would delete the premise.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn query_fallback_does_not_wait_for_writer_mutex() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let rows: Vec<BulkWriteRow> = (0..100)
            .map(|idx| BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{idx:03}"), "1-a", idx, false, idx as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "asc".to_string());
        let prepared = prepare_query(
            &schema,
            normalize_mango_query(
                &schema,
                MangoQuery {
                    selector: Some(json!({ "id": { "$regex": "^doc-09[57]$" } })),
                    sort: Some(vec![sort]),
                    index: None,
                    limit: None,
                    skip: Some(0),
                },
            ),
        )
        .unwrap();

        QUERY_WRITER_FALLBACKS.store(0, Ordering::SeqCst);
        let fallback_calls_before = runtime_counter("query_fallback_calls");
        let fallback_rows_before = runtime_counter("query_fallback_rows_visited");
        let fallback_decoded_before = runtime_counter("query_fallback_rows_decoded");
        let shared_conn = storage.connection().unwrap();
        let _writer_guard = shared_conn.lock();

        let query_instance = Arc::clone(&instance);
        let query_prepared = prepared.clone();
        let (query_tx, mut query_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let _ = query_tx.send(query_instance.query(&query_prepared).await);
        });
        let query_result = tokio::time::timeout(Duration::from_secs(1), &mut query_rx)
            .await
            .expect("fallback query waited for shared writer mutex")
            .expect("fallback query task dropped")
            .unwrap();
        let ages = query_result
            .documents
            .iter()
            .map(|doc| doc.get("age").and_then(Value::as_i64).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ages, vec![95, 97]);
        assert_eq!(
            QUERY_WRITER_FALLBACKS.load(Ordering::SeqCst),
            0,
            "file-backed query fallback must not use the shared writer connection fallback"
        );
        assert!(
            runtime_counter("query_fallback_calls") > fallback_calls_before,
            "runtime counters must expose normal query fallback calls"
        );
        assert!(
            runtime_counter("query_fallback_rows_visited") >= fallback_rows_before + 100,
            "runtime counters must expose rows visited by Rust matcher fallback"
        );
        assert!(
            runtime_counter("query_fallback_rows_decoded") >= fallback_decoded_before + 100,
            "runtime counters must expose rows decoded by Rust matcher fallback"
        );
    }

    #[tokio::test]
    async fn query_fallback_uses_query_plan_candidate_bounds_before_rust_matcher() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let rows: Vec<BulkWriteRow> = (0..1_000)
            .map(|idx| BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{idx:04}"), "1-a", idx, false, idx as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "asc".to_string());
        let prepared = prepare_query(
            &schema,
            normalize_mango_query(
                &schema,
                MangoQuery {
                    selector: Some(json!({
                        "age": { "$gte": 990 },
                        "id": { "$regex": "^doc-099[57]$" }
                    })),
                    sort: Some(vec![sort]),
                    index: None,
                    limit: None,
                    skip: Some(0),
                },
            ),
        )
        .unwrap();
        let query_plan: RxQueryPlan =
            serde_json::from_value(prepared.get("queryPlan").cloned().unwrap()).unwrap();
        let candidate = compile_query_plan_candidate_sql(
            &instance.table_name,
            &instance.primary_path,
            &query_plan,
        )
        .expect("query plan should produce an age-bounded candidate query");
        let conn = storage.connection().unwrap();
        {
            let conn = conn.lock();
            let mut statement = conn
                .prepare(&format!("EXPLAIN QUERY PLAN {}", candidate.sql))
                .unwrap();
            let plan = statement
                .query_map(rusqlite::params_from_iter(candidate.params.iter()), |row| {
                    row.get::<_, String>(3)
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n");
            assert!(
                plan.contains("_json_age_idx"),
                "expected query-plan candidate fallback to use the age index, got plan:\n{plan}"
            );
        }

        let fallback_calls_before = runtime_counter("query_fallback_calls");
        let candidate_calls_before = runtime_counter("query_fallback_indexed_candidate_calls");
        let fallback_rows_before = runtime_counter("query_fallback_rows_visited");
        let fallback_decoded_before = runtime_counter("query_fallback_rows_decoded");
        let collection_fallback_before =
            runtime_counter_pointer("/query_fallback_by_collection/docs");
        let regex_fallback_before = runtime_counter_pointer("/query_fallback_by_operator/$regex");
        let collection_regex_fallback_before =
            runtime_counter_pointer("/query_fallback_by_collection_operator/docs/$regex");
        let collection_rows_before =
            runtime_counter_pointer("/query_fallback_rows_visited_by_collection/docs");
        let collection_decoded_before =
            runtime_counter_pointer("/query_fallback_rows_decoded_by_collection/docs");
        let regex_rows_before =
            runtime_counter_pointer("/query_fallback_rows_visited_by_operator/$regex");
        let regex_decoded_before =
            runtime_counter_pointer("/query_fallback_rows_decoded_by_operator/$regex");
        let collection_regex_rows_before = runtime_counter_pointer(
            "/query_fallback_rows_visited_by_collection_operator/docs/$regex",
        );
        let collection_regex_decoded_before = runtime_counter_pointer(
            "/query_fallback_rows_decoded_by_collection_operator/docs/$regex",
        );
        let result = instance.query(&prepared).await.unwrap();
        let ids = result
            .documents
            .iter()
            .filter_map(|doc| doc.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["doc-0995", "doc-0997"]);
        assert_eq!(
            runtime_counter("query_fallback_calls"),
            fallback_calls_before + 1,
            "unsupported selector must still be counted as a fallback"
        );
        assert_eq!(
            runtime_counter("query_fallback_indexed_candidate_calls"),
            candidate_calls_before + 1,
            "fallback must record that it used a SQL candidate plan"
        );
        assert_eq!(
            runtime_counter("query_fallback_rows_visited") - fallback_rows_before,
            10,
            "Rust matcher must only inspect the age-index candidate window"
        );
        assert_eq!(
            runtime_counter("query_fallback_rows_decoded") - fallback_decoded_before,
            10,
            "fallback decode counter must match the bounded candidate window"
        );
        assert_eq!(
            runtime_counter_pointer("/query_fallback_by_collection/docs"),
            collection_fallback_before + 1,
            "fallback attribution must include the collection name"
        );
        assert_eq!(
            runtime_counter_pointer("/query_fallback_by_operator/$regex"),
            regex_fallback_before + 1,
            "fallback attribution must include the unsupported operator family"
        );
        assert_eq!(
            runtime_counter_pointer("/query_fallback_by_collection_operator/docs/$regex"),
            collection_regex_fallback_before + 1,
            "fallback attribution must include collection/operator pairs"
        );
        assert_eq!(
            runtime_counter_pointer("/query_fallback_rows_visited_by_collection/docs"),
            collection_rows_before + 10,
            "fallback row visits must be attributed to the collection"
        );
        assert_eq!(
            runtime_counter_pointer("/query_fallback_rows_decoded_by_collection/docs"),
            collection_decoded_before + 10,
            "fallback row decodes must be attributed to the collection"
        );
        assert_eq!(
            runtime_counter_pointer("/query_fallback_rows_visited_by_operator/$regex"),
            regex_rows_before + 10,
            "fallback row visits must be attributed to the operator"
        );
        assert_eq!(
            runtime_counter_pointer("/query_fallback_rows_decoded_by_operator/$regex"),
            regex_decoded_before + 10,
            "fallback row decodes must be attributed to the operator"
        );
        assert_eq!(
            runtime_counter_pointer(
                "/query_fallback_rows_visited_by_collection_operator/docs/$regex"
            ),
            collection_regex_rows_before + 10,
            "fallback row visits must be attributed to the collection/operator pair"
        );
        assert_eq!(
            runtime_counter_pointer(
                "/query_fallback_rows_decoded_by_collection_operator/docs/$regex"
            ),
            collection_regex_decoded_before + 10,
            "fallback row decodes must be attributed to the collection/operator pair"
        );
    }

    #[tokio::test]
    async fn query_fallback_without_candidate_bounds_fails_after_scan_limit() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let rows: Vec<BulkWriteRow> = (0..(SQLITE_QUERY_FALLBACK_SCAN_LIMIT + 5))
            .map(|idx| BulkWriteRow {
                previous: None,
                document: doc(
                    &format!("doc-{idx:05}"),
                    "1-a",
                    i64::try_from(idx).unwrap(),
                    false,
                    idx as f64,
                ),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("id".to_string(), "asc".to_string());
        let prepared = prepare_query(
            &schema,
            normalize_mango_query(
                &schema,
                MangoQuery {
                    selector: Some(json!({ "id": { "$regex": "^never-matches$" } })),
                    sort: Some(vec![sort]),
                    index: None,
                    limit: None,
                    skip: Some(0),
                },
            ),
        )
        .unwrap();
        let fallback_rows_before = runtime_counter("query_fallback_rows_visited");
        let fallback_decoded_before = runtime_counter("query_fallback_rows_decoded");
        let too_broad_before = runtime_counter("query_fallback_too_broad_calls");
        let collection_regex_fallback_before =
            runtime_counter_pointer("/query_fallback_by_collection_operator/docs/$regex");
        let err = instance.query(&prepared).await.unwrap_err();
        assert_eq!(err.code(), SQLITE_QUERY_FALLBACK_TOO_BROAD);
        assert!(
            runtime_counter("query_fallback_rows_visited")
                >= fallback_rows_before + SQLITE_QUERY_FALLBACK_SCAN_LIMIT + 1,
            "too-broad fallback abort must still report visited rows"
        );
        assert!(
            runtime_counter("query_fallback_rows_decoded")
                >= fallback_decoded_before + SQLITE_QUERY_FALLBACK_SCAN_LIMIT + 1,
            "too-broad fallback abort must still report decoded rows"
        );
        assert_eq!(
            runtime_counter("query_fallback_too_broad_calls"),
            too_broad_before + 1,
            "broad fallback aborts must be visible in runtime counters"
        );
        assert_eq!(
            runtime_counter_pointer("/query_fallback_by_collection_operator/docs/$regex"),
            collection_regex_fallback_before + 1,
            "too-broad fallback aborts must still be attributed"
        );
    }

    #[tokio::test]
    async fn count_fallback_reports_slow_mode_and_counter() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let rows: Vec<BulkWriteRow> = (0..10)
            .map(|idx| BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{idx:03}"), "1-a", idx, false, idx as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let prepared = prepare_query(
            &schema,
            normalize_mango_query(
                &schema,
                MangoQuery {
                    selector: Some(json!({ "id": { "$regex": "^doc-00[12]$" } })),
                    index: None,
                    limit: None,
                    skip: Some(0),
                    ..Default::default()
                },
            ),
        )
        .unwrap();
        let fallback_count_before = runtime_counter("count_fallback_query_calls");
        let result = instance.count(&prepared).await.unwrap();
        assert_eq!(result.count, 2);
        assert_eq!(
            result.mode, "slow",
            "count fallback must not report fast mode after materializing query results"
        );
        assert!(
            runtime_counter("count_fallback_query_calls") > fallback_count_before,
            "runtime counters must expose count fallback calls"
        );
    }

    #[tokio::test]
    async fn query_stream_compiled_sql_stops_without_materializing_remaining_rows() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let mut schema = test_schema();
        schema.indexes = vec![vec!["id".to_string()]];
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let rows: Vec<BulkWriteRow> = (0..100)
            .map(|idx| BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{idx:03}"), "1-a", idx, false, idx as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        {
            let conn = storage.connection().unwrap();
            conn.lock()
                .execute(
                    &format!(
                        "UPDATE {} SET data = ? WHERE id = ?",
                        quote_identifier(&instance.table_name)
                    ),
                    params!["{not-json", "doc-099"],
                )
                .unwrap();
        }

        let mut sort = HashMap::new();
        sort.insert("id".to_string(), "asc".to_string());
        let prepared = prepare_query(
            &schema,
            normalize_mango_query(
                &schema,
                MangoQuery {
                    selector: Some(json!({})),
                    sort: Some(vec![sort]),
                    index: None,
                    limit: None,
                    skip: Some(0),
                },
            ),
        )
        .unwrap();

        let mut seen = Vec::new();
        instance
            .query_stream(&prepared, 2, |batch| {
                seen.extend(
                    batch
                        .iter()
                        .filter_map(|doc| doc.get("id").and_then(Value::as_str).map(str::to_owned)),
                );
                Ok(false)
            })
            .unwrap();

        assert_eq!(seen, vec!["doc-000".to_string(), "doc-001".to_string()]);
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
    async fn query_stream_cursor_does_not_block_the_async_worker() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let mut creation_params = params(schema.clone());
        creation_params.collection_name = "sync_a_stream_worker".to_string();
        let instance = create_storage_instance(&storage, creation_params)
            .await
            .unwrap();
        instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("doc-00", "1-a", 0, false, 0.0),
                }],
                "seed",
            )
            .await
            .unwrap();
        let prepared = prepare_query(
            &schema,
            normalize_mango_query(
                &schema,
                MangoQuery {
                    selector: Some(json!({})),
                    sort: None,
                    index: None,
                    limit: None,
                    skip: None,
                },
            ),
        )
        .unwrap();

        set_test_query_stream_row_delay_ms(&instance.table_name, 250);
        let _delay_reset = QueryStreamRowDelayReset(instance.table_name.clone());
        let started = tokio::time::Instant::now();
        let mut streamed_ids = Vec::new();
        let mut on_batch = |batch: Vec<Value>| {
            streamed_ids.extend(
                batch
                    .iter()
                    .filter_map(|doc| doc.get("id").and_then(Value::as_str).map(str::to_owned)),
            );
            Ok(false)
        };
        let (stream_result, timer_elapsed) = tokio::join!(
            instance.query_stream_into(&prepared, 1, &mut on_batch),
            async {
                tokio::time::sleep(Duration::from_millis(25)).await;
                started.elapsed()
            }
        );

        stream_result.unwrap();
        assert_eq!(streamed_ids, vec!["doc-00".to_string()]);
        assert!(
            timer_elapsed < Duration::from_millis(180),
            "the async timer ran after {timer_elapsed:?}, so the SQLite cursor blocked its worker"
        );
    }

    #[tokio::test]
    async fn query_stream_uses_dedicated_reader_while_point_read_runs() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let mut schema = test_schema();
        schema.indexes = vec![vec!["id".to_string()]];
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        let rows = (0..20)
            .map(|idx| BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{idx:02}"), "1-a", idx, false, idx as f64),
            })
            .collect();
        instance.bulk_write(rows, "seed").await.unwrap();

        let mut sort = HashMap::new();
        sort.insert("id".to_string(), "asc".to_string());
        let prepared = prepare_query(
            &schema,
            normalize_mango_query(
                &schema,
                MangoQuery {
                    selector: Some(json!({})),
                    sort: Some(vec![sort]),
                    index: None,
                    limit: None,
                    skip: None,
                },
            ),
        )
        .unwrap();

        let read_instance = Arc::clone(&instance);
        let mut batches = 0usize;
        instance
            .query_stream_into(&prepared, 1, &mut |batch| {
                batches += 1;
                assert_eq!(batch.len(), 1);
                assert_eq!(batch[0]["id"], "doc-00");

                let read_instance = Arc::clone(&read_instance);
                let point_read = std::thread::spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    runtime.block_on(async move {
                        tokio::time::timeout(
                            Duration::from_millis(750),
                            read_instance.find_documents_by_id(&["doc-19".to_string()], false),
                        )
                        .await
                    })
                })
                .join()
                .expect("point-read thread must not panic")
                .expect("point read must not wait for the open stream cursor")
                .expect("point read must succeed");
                assert_eq!(point_read.len(), 1);
                assert_eq!(point_read[0]["id"], "doc-19");
                Ok(false)
            })
            .await
            .unwrap();
        assert_eq!(batches, 1);
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
    async fn changed_documents_since_checkpoint_uses_sqlite_lwt_when_document_meta_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let instance = create_storage_instance(&storage, params(test_schema()))
            .await
            .unwrap();
        let table = quote_identifier(&instance.table_name);
        let document = json!({
            "id": "catalog",
            "age": 1,
            "_rev": "1-catalog",
            "_deleted": false,
            "_attachments": {}
        });
        {
            let connection = storage.connection().unwrap();
            let conn = connection.lock();
            conn.execute(
                &format!(
                    "INSERT INTO {table} (id, revision, deleted, lastWriteTime, data)
                     VALUES (?1, ?2, 0, ?3, ?4)"
                ),
                params![
                    "catalog",
                    "1-catalog",
                    1782904408877.0_f64,
                    serde_json::to_string(&document).unwrap()
                ],
            )
            .unwrap();
        }

        let first = instance
            .get_changed_documents_since(10, None)
            .await
            .unwrap();
        assert_eq!(first.documents.len(), 1);
        assert_eq!(
            first.checkpoint,
            json!({ "id": "catalog", "lwt": 1782904408877.0_f64 })
        );

        let second = instance
            .get_changed_documents_since(10, Some(&first.checkpoint))
            .await
            .unwrap();
        assert!(
            second.documents.is_empty(),
            "second poll must not replay the row when document _meta.lwt is missing"
        );
        assert_eq!(second.checkpoint, first.checkpoint);
    }

    // clippy::await_holding_lock is allowed here deliberately: holding the writer
    // mutex across the await IS the subject of this test — it proves the read path
    // proceeds while a writer holds the lock. Releasing it would delete the premise.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn changed_documents_since_file_backed_uses_read_only_connection() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let instance = create_storage_instance(&storage, params(test_schema()))
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
                        document: doc("b", "1-b", 1, false, 2.0),
                    },
                ],
                "insert",
            )
            .await
            .unwrap();

        CHANGED_DOCUMENTS_SINCE_WRITER_FALLBACKS.store(0, Ordering::SeqCst);
        let shared_conn = storage.connection().unwrap();
        let _writer_guard = shared_conn.lock();

        let changed_instance = Arc::clone(&instance);
        let (changed_tx, mut changed_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let _ = changed_tx.send(changed_instance.get_changed_documents_since(10, None).await);
        });
        let changed = tokio::time::timeout(Duration::from_secs(1), &mut changed_rx)
            .await
            .expect("changed_documents_since waited for shared writer mutex")
            .expect("changed_documents_since task dropped")
            .unwrap();
        let ids = changed
            .documents
            .iter()
            .filter_map(|doc| doc.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["a", "b"]);
        assert_eq!(
            CHANGED_DOCUMENTS_SINCE_WRITER_FALLBACKS.load(Ordering::SeqCst),
            0,
            "file-backed get_changed_documents_since must not use the shared writer connection fallback"
        );
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

    fn checkpoint_contract_fixture() -> Value {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/webrtc-checkpoint-contract.json"
        ))
        .expect("parse checkpoint contract fixture")
    }

    fn render_checkpoint_epoch_input(
        template: &str,
        database_name: &str,
        collection_name: &str,
        schema_hash: &str,
        latest_lwt: f64,
        latest_id: &str,
    ) -> String {
        template
            .replace("{databaseName}", database_name)
            .replace("{collectionName}", collection_name)
            .replace("{schemaHash}", schema_hash)
            // f64 Display: an integer lwt renders WITHOUT a decimal point,
            // matching JS `String(Number)` — that equivalence is part of the
            // pinned contract (fixture `checkpointStatus.lwtRendering`).
            .replace("{latestLwt}", &format!("{latest_lwt}"))
            .replace("{latestId}", latest_id)
    }

    /// SYNC-03: the checkpoint wire shape is pinned by
    /// `tests/fixtures/webrtc-checkpoint-contract.json` on BOTH language
    /// sides. A one-sided change to the field list, the epoch derivation, or
    /// the empty-cursor fallback must turn this test red.
    #[tokio::test]
    async fn checkpoint_contract_fixture_matches_checkpoint_status() {
        let fixture = checkpoint_contract_fixture();
        assert_eq!(fixture["contract"], "ctox-checkpoint-contract-v1");
        let template = fixture["checkpointStatus"]["epochInputTemplate"]
            .as_str()
            .expect("epoch input template");

        // 1. The fixture's worked-example hashes are real sha256 values under
        //    the SAME hash + format pipeline production uses. This proves the
        //    fixture numbers themselves (the JS smoke recomputes them with
        //    node:crypto, pinning the algorithm cross-language).
        let example = &fixture["example"];
        assert_eq!(
            sha256_hex(example["latestId"].as_str().unwrap().as_bytes()),
            example["latestIdHash"].as_str().unwrap()
        );
        let example_epoch_input = render_checkpoint_epoch_input(
            template,
            example["databaseName"].as_str().unwrap(),
            example["collectionName"].as_str().unwrap(),
            example["schemaHash"].as_str().unwrap(),
            example["latestLwt"].as_f64().unwrap(),
            example["latestId"].as_str().unwrap(),
        );
        assert_eq!(
            example_epoch_input,
            example["epochInput"].as_str().unwrap(),
            "fixture epochInput must equal the substituted epochInputTemplate"
        );
        assert_eq!(
            sha256_hex(example_epoch_input.as_bytes()),
            example["epoch"].as_str().unwrap()
        );

        // 2. A real storage instance emits EXACTLY the fixture's field list
        //    and derives its epoch via the fixture template.
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let instance = create_storage_instance(&storage, params(test_schema()))
            .await
            .unwrap();

        // Empty collection: the `{id: "", lwt: 0}` replication-cursor
        // fallback pinned by the fixture flows into the epoch input.
        let empty_status = instance.replication_checkpoint_status().await;
        let empty_cursor = &fixture["replicationCursor"]["empty"];
        assert_eq!(empty_cursor["id"], "");
        assert_eq!(empty_cursor["lwt"], 0);
        assert_eq!(
            empty_status["latestLwt"].as_f64(),
            empty_cursor["lwt"].as_f64()
        );
        assert_eq!(empty_status["latestIdHash"], "");
        let empty_epoch_input = render_checkpoint_epoch_input(
            template,
            "db",
            "docs",
            empty_status["schemaHash"].as_str().unwrap(),
            empty_cursor["lwt"].as_f64().unwrap(),
            empty_cursor["id"].as_str().unwrap(),
        );
        assert_eq!(
            empty_status["epoch"].as_str().unwrap(),
            sha256_hex(empty_epoch_input.as_bytes()),
            "empty-collection epoch must derive from the fixture's empty replication cursor"
        );

        instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("note-1", "1-a", 1, false, 7.0),
                }],
                "insert",
            )
            .await
            .unwrap();
        let status = instance.replication_checkpoint_status().await;

        let mut expected_fields: Vec<&str> = fixture["checkpointStatus"]["fields"]
            .as_array()
            .expect("checkpoint status field list")
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        expected_fields.sort_unstable();
        let mut actual_fields: Vec<&str> = status
            .as_object()
            .expect("checkpoint status object")
            .keys()
            .map(String::as_str)
            .collect();
        actual_fields.sort_unstable();
        assert_eq!(
            actual_fields, expected_fields,
            "checkpoint status field list drifted from webrtc-checkpoint-contract.json"
        );

        assert_eq!(status["source"], fixture["checkpointStatus"]["source"]);
        assert_eq!(status["state"], fixture["checkpointStatus"]["state"]);
        assert_eq!(status["state"], "advertised");
        assert_eq!(status["collection"], "docs");
        assert_eq!(
            status["latestIdHash"].as_str().unwrap(),
            sha256_hex(b"note-1")
        );

        let live_epoch_input = render_checkpoint_epoch_input(
            template,
            "db",
            "docs",
            status["schemaHash"].as_str().unwrap(),
            status["latestLwt"].as_f64().unwrap(),
            "note-1",
        );
        assert_eq!(
            status["epoch"].as_str().unwrap(),
            sha256_hex(live_epoch_input.as_bytes()),
            "live epoch must be sha256 of the fixture's epochInputTemplate substitution"
        );
    }

    #[tokio::test]
    async fn external_change_feed_failures_are_counted_and_backed_off() {
        assert_eq!(
            external_poll_failure_backoff(1),
            SQLITE_EXTERNAL_POLL_FAILURE_BACKOFF_BASE
        );
        assert_eq!(
            external_poll_failure_backoff(2),
            SQLITE_EXTERNAL_POLL_FAILURE_BACKOFF_BASE * 2
        );
        assert_eq!(
            external_poll_failure_backoff(u32::MAX),
            SQLITE_EXTERNAL_POLL_FAILURE_BACKOFF_MAX
        );

        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: std::path::PathBuf::from(":memory:"),
        });
        let mut creation_params = params(test_schema());
        creation_params.collection_name = "sync_a_change_feed_failure".to_string();
        let instance = create_storage_instance(&storage, creation_params)
            .await
            .unwrap();
        let failures_before = runtime_counter_map_value(
            "external_poll_drain_failures_by_table",
            &instance.table_name,
        );

        {
            let conn = instance.connection.lock();
            conn.execute(
                &format!("DROP TABLE {}", quote_identifier(&instance.table_name)),
                [],
            )
            .unwrap();
        }
        instance.external_notifier.signal();

        tokio::time::timeout(Duration::from_secs(1), async {
            while runtime_counter_map_value(
                "external_poll_drain_failures_by_table",
                &instance.table_name,
            ) < failures_before + 1
            {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("first failed change-feed drain must be counted");

        tokio::time::sleep(SQLITE_EXTERNAL_POLL_FAILURE_BACKOFF_BASE / 2).await;
        assert_eq!(
            runtime_counter_map_value(
                "external_poll_drain_failures_by_table",
                &instance.table_name,
            ),
            failures_before + 1,
            "the first failed drain must not retry before its backoff expires"
        );

        tokio::time::timeout(Duration::from_secs(1), async {
            while runtime_counter_map_value(
                "external_poll_drain_failures_by_table",
                &instance.table_name,
            ) < failures_before + 2
            {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("failed change-feed drain must retry after backoff");

        tokio::time::sleep(SQLITE_EXTERNAL_POLL_FAILURE_BACKOFF_BASE).await;
        assert_eq!(
            runtime_counter_map_value(
                "external_poll_drain_failures_by_table",
                &instance.table_name,
            ),
            failures_before + 2,
            "the second failed drain must use the doubled backoff"
        );
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

    // clippy::await_holding_lock is allowed here deliberately: holding the writer
    // mutex across the await IS the subject of this test — it proves the read path
    // proceeds while a writer holds the lock. Releasing it would delete the premise.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn external_write_poll_uses_read_only_connection_while_writer_mutex_is_held() {
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

        tokio::time::sleep(Duration::from_millis(100)).await;

        {
            let conn = rusqlite::Connection::open(&database_path).unwrap();
            insert_document(
                &conn,
                &instance.table_name,
                &instance.primary_path,
                &doc("external-readonly", "1-external", 7, false, 10.0),
            )
            .unwrap();
        }

        let shared_conn = storage.connection().unwrap();
        let _writer_guard = shared_conn.lock();
        notify_table_change_for_path(&database_path, &instance.table_name);

        let bulk = timeout(Duration::from_millis(750), stream.next())
            .await
            .expect("external poll should not wait for the shared writer mutex")
            .expect("change stream should stay open");
        assert_eq!(bulk.context.as_deref(), Some("sqlite-external-poll"));
        assert_eq!(bulk.events.len(), 1);
        assert_eq!(bulk.events[0].document_id, "external-readonly");
    }

    #[tokio::test]
    async fn same_table_instances_share_notifications_until_the_last_instance_closes() {
        use tokio::time::timeout;
        use tokio_stream::StreamExt;

        let dir = tempfile::tempdir().unwrap();
        let database_path = dir.path().join("ctox.sqlite3");
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: database_path.clone(),
        });
        let first = create_storage_instance(&storage, params(test_schema()))
            .await
            .unwrap();
        let second = create_storage_instance(&storage, params(test_schema()))
            .await
            .unwrap();
        assert_eq!(first.table_name, second.table_name);
        let mut first_stream = first.change_stream();
        let mut second_stream = second.change_stream();
        tokio::time::sleep(Duration::from_millis(100)).await;

        {
            let conn = rusqlite::Connection::open(&database_path).unwrap();
            insert_document(
                &conn,
                &first.table_name,
                &first.primary_path,
                &doc("shared-wake", "1-external", 7, false, 10.0),
            )
            .unwrap();
        }
        assert!(notify_table_change_for_path(
            &database_path,
            &first.table_name
        ));
        for stream in [&mut first_stream, &mut second_stream] {
            let bulk = timeout(Duration::from_millis(750), stream.next())
                .await
                .expect("every live instance must receive the shared table wake")
                .expect("change stream should stay open");
            assert_eq!(bulk.events[0].document_id, "shared-wake");
        }

        first.close().await.unwrap();
        {
            let conn = rusqlite::Connection::open(&database_path).unwrap();
            insert_document(
                &conn,
                &second.table_name,
                &second.primary_path,
                &doc("after-first-close", "1-external", 8, false, 20.0),
            )
            .unwrap();
        }
        assert!(notify_table_change_for_path(
            &database_path,
            &second.table_name
        ));
        let bulk = timeout(Duration::from_millis(750), second_stream.next())
            .await
            .expect("closing one instance must retain the shared table wake")
            .expect("remaining change stream should stay open");
        assert_eq!(bulk.events[0].document_id, "after-first-close");
    }

    // Backlog OS-A1: the idle-budget guard for the external write poll lives
    // in tests/idle_budget.rs as an INTEGRATION test on purpose — it holds a
    // live poll thread for several seconds, and in the shared lib-test
    // process its connection opens inflated the exact global-counter deltas
    // other tests assert (file_backed_reads_reuse_cached_read_only_connection,
    // bulk_write_reads_only_written_ids_state_among_many_rows). A separate
    // test binary gets its own counter statics.

    #[tokio::test]
    async fn change_stream_drains_multiple_external_batches_per_wake() {
        use tokio::time::{timeout, Instant};
        use tokio_stream::StreamExt;

        let dir = tempfile::tempdir().unwrap();
        let database_path = dir.path().join("ctox.sqlite3");
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: database_path.clone(),
        });
        let schema = test_schema();
        let mut creation_params = params(schema);
        creation_params.collection_name = "other_connection_counter".to_string();
        let instance = create_storage_instance(&storage, creation_params)
            .await
            .unwrap();
        let mut stream = instance.change_stream();

        let total = SQLITE_EXTERNAL_POLL_DEFAULT_LIMIT as usize * 2 + 7;
        {
            let mut conn = rusqlite::Connection::open(&database_path).unwrap();
            let tx = conn.transaction().unwrap();
            for idx in 0..total {
                insert_document(
                    &tx,
                    &instance.table_name,
                    &instance.primary_path,
                    &doc(
                        &format!("external-burst-{idx:03}"),
                        "1-external",
                        idx as i64,
                        false,
                        10.0 + idx as f64,
                    ),
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }

        let drain_calls_before = runtime_counter("external_poll_drain_calls");
        let drain_batches_before = runtime_counter("external_poll_drain_batches");
        let drain_empty_batches_before = runtime_counter("external_poll_drain_empty_batches");
        let drain_rows_visited_before = runtime_counter("external_poll_drain_rows_visited");
        let drain_rows_before = runtime_counter("external_poll_drain_rows_decoded");
        let drain_budget_exhaustions_before =
            runtime_counter("external_poll_drain_budget_exhaustions");
        let drain_table_rows_before =
            runtime_counter_map_value("external_poll_drain_rows_by_table", &instance.table_name);
        let drain_table_batches_before =
            runtime_counter_map_value("external_poll_drain_batches_by_table", &instance.table_name);
        let drain_table_budget_exhaustions_before = runtime_counter_map_value(
            "external_poll_drain_budget_exhaustions_by_table",
            &instance.table_name,
        );

        notify_table_change(&database_key_for_path(&database_path), &instance.table_name);

        let deadline = Instant::now() + Duration::from_millis(750);
        let mut seen = HashSet::new();
        let mut bulks = 0usize;
        while seen.len() < total {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "external poll did not drain burst before the 1s safety poll"
            );
            let bulk = timeout(remaining, stream.next())
                .await
                .expect("external burst batch should arrive before safety poll")
                .expect("change stream should stay open");
            bulks += 1;
            for event in bulk.events {
                seen.insert(event.document_id);
            }
        }

        assert_eq!(seen.len(), total);
        assert!(
            bulks >= 3,
            "burst should be emitted as multiple bounded batches"
        );
        assert!(
            runtime_counter("external_poll_drain_calls") >= drain_calls_before + 1,
            "external poll should count the notified drain"
        );
        assert!(
            runtime_counter("external_poll_drain_batches") >= drain_batches_before + 3,
            "external poll should count all non-empty drain batches"
        );
        assert!(
            runtime_counter("external_poll_drain_empty_batches") >= drain_empty_batches_before + 1,
            "external poll should count the empty drain terminator"
        );
        assert!(
            runtime_counter("external_poll_drain_rows_decoded") >= drain_rows_before + total as u64,
            "external poll should count drained rows"
        );
        assert!(
            runtime_counter("external_poll_drain_rows_visited")
                >= drain_rows_visited_before + total as u64,
            "external poll should count visited rows"
        );
        assert_eq!(
            runtime_counter("external_poll_drain_budget_exhaustions"),
            drain_budget_exhaustions_before,
            "three-batch burst should drain before the per-wake budget is exhausted"
        );
        assert!(
            runtime_counter_map_value("external_poll_drain_rows_by_table", &instance.table_name)
                >= drain_table_rows_before + total as u64,
            "external poll should attribute drained rows to the table"
        );
        assert!(
            runtime_counter_map_value("external_poll_drain_batches_by_table", &instance.table_name)
                >= drain_table_batches_before + 3,
            "external poll should attribute drain batches to the table"
        );
        assert_eq!(
            runtime_counter_map_value(
                "external_poll_drain_budget_exhaustions_by_table",
                &instance.table_name
            ),
            drain_table_budget_exhaustions_before,
            "three-batch burst should not exhaust the table drain budget"
        );
    }

    #[tokio::test]
    async fn change_stream_drains_desktop_file_chunk_batches_per_wake() {
        use tokio::time::{timeout, Instant};
        use tokio_stream::StreamExt;

        let dir = tempfile::tempdir().unwrap();
        let database_path = dir.path().join("ctox.sqlite3");
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: database_path.clone(),
        });
        let schema = test_schema();
        let mut creation_params = params(schema);
        creation_params.collection_name = "desktop_file_chunks".to_string();
        let instance = create_storage_instance(&storage, creation_params)
            .await
            .unwrap();
        let mut stream = instance.change_stream();

        let chunk_limit = crate::collection_policy::collection_policy()
            .poll_batch_limit("desktop_file_chunks")
            .expect("desktop_file_chunks poll limit");
        let total = chunk_limit * 3 + 1;
        {
            let mut conn = rusqlite::Connection::open(&database_path).unwrap();
            let tx = conn.transaction().unwrap();
            for idx in 0..total {
                insert_document(
                    &tx,
                    &instance.table_name,
                    &instance.primary_path,
                    &doc(
                        &format!("chunk-burst-{idx:03}"),
                        "1-external",
                        idx as i64,
                        false,
                        20.0 + idx as f64,
                    ),
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }

        notify_table_change(&database_key_for_path(&database_path), &instance.table_name);

        let deadline = Instant::now() + Duration::from_millis(750);
        let mut seen = HashSet::new();
        let mut bulks = 0usize;
        while seen.len() < total {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "desktop_file_chunks catch-up waited for the safety poll"
            );
            let bulk = timeout(remaining, stream.next())
                .await
                .expect("desktop_file_chunks batch should arrive before safety poll")
                .expect("change stream should stay open");
            bulks += 1;
            for event in bulk.events {
                seen.insert(event.document_id);
            }
        }

        assert_eq!(seen.len(), total);
        assert!(
            bulks >= 4,
            "desktop_file_chunks must catch up through multiple small batches"
        );
    }

    #[tokio::test]
    async fn change_stream_emits_other_connection_sqlite_writes() {
        use tokio::time::{timeout, Instant};
        use tokio_stream::StreamExt;

        let dir = tempfile::tempdir().unwrap();
        let database_path = dir.path().join("ctox.sqlite3");
        let poll_connection_opens_before = runtime_counter("external_poll_connection_opens");
        let changed_table_reads_before = runtime_counter("external_poll_changed_table_reads");
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: database_path.clone(),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema))
            .await
            .unwrap();
        let mut stream = instance.change_stream();

        let startup_deadline = Instant::now() + Duration::from_secs(2);
        while changed_documents_since_table_call_count(&instance.table_name) == 0
            || runtime_counter("external_poll_connection_opens") <= poll_connection_opens_before
            || runtime_counter("external_poll_changed_table_reads") <= changed_table_reads_before
        {
            assert!(
                Instant::now() < startup_deadline,
                "database-wide external poll did not complete its startup read"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            timeout(Duration::from_millis(100), stream.next())
                .await
                .is_err(),
            "empty startup reconciliation should settle before the external write"
        );
        reset_changed_documents_since_table_call_count(&instance.table_name);

        // Let the database-wide watcher leave its initial active window. This
        // proves the standby cadence still notices an unrelated SQLite
        // connection promptly instead of only exercising the startup poll.
        tokio::time::sleep(Duration::from_secs(4)).await;
        let table_notifications_before =
            runtime_counter_map_value("external_poll_notifications_by_table", &instance.table_name);

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

        let bulk = match timeout(Duration::from_secs(3), stream.next()).await {
            Ok(Some(bulk)) => bulk,
            Ok(None) => panic!("change stream closed before other-connection write was emitted"),
            Err(_) => panic!(
                "other-connection write should be emitted; sqlite counters: {}",
                sqlite_runtime_counters_snapshot()
            ),
        };
        assert_eq!(bulk.context.as_deref(), Some("sqlite-external-poll"));
        assert_eq!(
            bulk.checkpoint,
            Some(json!({ "id": "external-connection", "lwt": 10.0 }))
        );
        assert_eq!(bulk.events.len(), 1);
        assert_eq!(bulk.events[0].document_id, "external-connection");
        assert_eq!(bulk.events[0].operation, "UPDATE");
        assert!(
            runtime_counter_map_value("external_poll_notifications_by_table", &instance.table_name,)
                > table_notifications_before,
            "database-wide external poll notification counter must attribute the wake to the table"
        );
    }

    #[tokio::test]
    async fn file_backed_external_poll_has_no_per_collection_idle_safety_drains() {
        use tokio::time::Instant;

        set_test_external_poll_safety_interval_ms(25);
        let _safety_reset = ExternalPollSafetyIntervalReset;

        let dir = tempfile::tempdir().unwrap();
        let database_path = dir.path().join("ctox.sqlite3");
        assert!(external_poll_safety_interval_for_path(&database_path).is_none());
        assert!(external_poll_safety_interval_for_path(Path::new(":memory:")).is_some());

        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: database_path.clone(),
        });

        let mut instances = Vec::new();
        let mut table_names = Vec::new();
        for idx in 0..12 {
            let mut creation_params = params(test_schema());
            creation_params.collection_name = format!("idle_docs_{idx}");
            let instance = create_storage_instance(&storage, creation_params)
                .await
                .unwrap();
            table_names.push(instance.table_name.clone());
            instances.push(instance);
        }

        let startup_deadline = Instant::now() + Duration::from_secs(2);
        for table_name in &table_names {
            while changed_documents_since_table_call_count(table_name) == 0 {
                assert!(
                    Instant::now() < startup_deadline,
                    "startup reconciliation did not drain table {table_name}"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            reset_changed_documents_since_table_call_count(table_name);
        }
        let notification_counts_before = table_names
            .iter()
            .map(|table_name| {
                (
                    table_name.clone(),
                    runtime_counter_map_value("external_poll_notifications_by_table", table_name),
                )
            })
            .collect::<HashMap<_, _>>();

        tokio::time::sleep(Duration::from_millis(150)).await;

        for table_name in &table_names {
            assert_eq!(
                changed_documents_since_table_call_count(table_name),
                0,
                "file-backed idle table {table_name} was drained without a table notification"
            );
            assert_eq!(
                runtime_counter_map_value("external_poll_notifications_by_table", table_name),
                notification_counts_before
                    .get(table_name)
                    .copied()
                    .unwrap_or(0),
                "database-wide watcher emitted a notification for idle table {table_name}"
            );
        }

        for instance in instances {
            instance.close().await.unwrap();
        }
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
