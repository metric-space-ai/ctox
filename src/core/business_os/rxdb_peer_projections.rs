// Origin: CTOX
// License: Apache-2.0

use super::rxdb_peer::{
    business_os_projection_sleep_secs, business_users_projection_stamp,
    canonical_projection_document_for_compare, channel_state_projection_stamp_async,
    fill_projection_document_envelope, is_projection_tombstone, knowledge_tables_source_stamp,
    module_catalog_projection_stamp, normalize_business_record_projection_document,
    projection_document_has_valid_revision, remove_projection_rxdb_envelope,
    runtime_settings_projection_stamp, sync_business_users_with_database,
    sync_channel_state_with_database, sync_knowledge_tables_with_database,
    sync_module_catalog_with_database, sync_projection_if_changed_with_strategy,
    sync_runtime_settings_with_database, sync_ticket_state_with_database,
    sync_workspace_branding_with_database, ticket_state_source_stamp,
    update_projection_idle_rounds, upsert_business_record_projection_tombstone,
    workspace_branding_projection_stamp, BackgroundProjectionLoopConfig, NativePeerLoopMetrics,
    BUSINESS_RECORD_PROJECTION_WRITE_BATCH_SIZE, BUSINESS_USERS_PROJECTION_LOOP,
    CHANNEL_STATE_PROJECTION_LOOP, KNOWLEDGE_TABLES_PROJECTION_LOOP,
    MODULE_CATALOG_PROJECTION_LOOP, NATIVE_RXDB_WRITE_LOCK, RUNTIME_SETTINGS_PROJECTION_LOOP,
    TICKET_STATE_PROJECTION_LOOP, WORKSPACE_BRANDING_PROJECTION_LOOP,
};
use rxdb::rx_collection::RxCollection;
use rxdb::rx_database::RxDatabase;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;

/// Per-database document-lookup counter for chat-tracking batch tests.
/// Scoped by database name so parallel sister tests cannot inflate the count.
#[cfg(test)]
static CHAT_TRACKING_BATCH_DOCUMENT_LOOKUPS: OnceLock<Mutex<HashMap<String, usize>>> =
    OnceLock::new();

#[cfg(test)]
fn chat_tracking_batch_document_lookups() -> &'static Mutex<HashMap<String, usize>> {
    CHAT_TRACKING_BATCH_DOCUMENT_LOOKUPS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
fn record_chat_tracking_batch_document_lookup(database_name: &str) {
    let mut counts = chat_tracking_batch_document_lookups()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *counts.entry(database_name.to_string()).or_insert(0) += 1;
}

#[cfg(test)]
pub(super) fn reset_chat_tracking_batch_document_lookups(database_name: &str) {
    let mut counts = chat_tracking_batch_document_lookups()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.insert(database_name.to_string(), 0);
}

#[cfg(test)]
pub(super) fn chat_tracking_batch_document_lookup_count(database_name: &str) -> usize {
    let counts = chat_tracking_batch_document_lookups()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.get(database_name).copied().unwrap_or(0)
}
pub(super) static NOTES_LOOP_METRICS: NativePeerLoopMetrics = NativePeerLoopMetrics::new("notes");
pub(super) static DESKTOP_FILE_INDEX_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("desktop_file_index");
pub(super) static CHANNEL_STATE_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("channel_state");
pub(super) static BUSINESS_USERS_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("business_users");
pub(super) static RUNTIME_SETTINGS_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("runtime_settings");
pub(super) static WORKSPACE_BRANDING_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("workspace_branding");
pub(super) static MODULE_CATALOG_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("module_catalog");
pub(super) static TICKET_STATE_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("ticket_state");
pub(super) static KNOWLEDGE_TABLES_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("knowledge_tables");
pub(super) static BUSINESS_RECORDS_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("business_records");
pub(super) static BUSINESS_COMMANDS_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("business_commands");

pub(super) fn record_native_peer_loop_result(
    metrics: &NativePeerLoopMetrics,
    result: &anyhow::Result<usize>,
    elapsed: Duration,
) {
    match result {
        Ok(rows) => metrics.record(Some(*rows), elapsed),
        Err(_) => metrics.record(None, elapsed),
    }
}

pub(super) fn record_desktop_file_index_loop_result(
    result: &anyhow::Result<usize>,
    elapsed: Duration,
) {
    record_native_peer_loop_result(&DESKTOP_FILE_INDEX_LOOP_METRICS, result, elapsed);
}

pub(super) fn record_native_peer_bool_loop_result(
    metrics: &NativePeerLoopMetrics,
    result: &anyhow::Result<bool>,
    elapsed: Duration,
) {
    match result {
        Ok(true) => metrics.record(Some(1), elapsed),
        Ok(false) => metrics.record(Some(0), elapsed),
        Err(_) => metrics.record(None, elapsed),
    }
}

async fn run_background_projection_loop<
    Stamp,
    SourceStamp,
    SourceStampFuture,
    Project,
    ProjectFuture,
>(
    config: BackgroundProjectionLoopConfig,
    mut source_stamp: SourceStamp,
    mut project: Project,
) where
    Stamp: PartialEq,
    SourceStamp: FnMut() -> SourceStampFuture,
    SourceStampFuture: std::future::Future<Output = anyhow::Result<Stamp>>,
    Project: FnMut() -> ProjectFuture,
    ProjectFuture: std::future::Future<Output = anyhow::Result<usize>>,
{
    let mut last_source_stamp = None;
    let mut consecutive_idle_rounds = 0u32;
    loop {
        let started = Instant::now();
        let result = sync_projection_if_changed_with_strategy(
            &mut last_source_stamp,
            config.stamp_strategy,
            &mut source_stamp,
            &mut project,
        )
        .await;
        record_native_peer_loop_result(config.metrics, &result, started.elapsed());
        update_projection_idle_rounds(result, &mut consecutive_idle_rounds, config.failure_prefix);
        tokio::time::sleep(Duration::from_secs(business_os_projection_sleep_secs(
            config.active_interval_secs,
            consecutive_idle_rounds,
        )))
        .await;
    }
}

pub(super) async fn sync_channel_state_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let stamp_root = root.clone();
    run_background_projection_loop(
        CHANNEL_STATE_PROJECTION_LOOP,
        move || {
            let root = stamp_root.clone();
            async move { channel_state_projection_stamp_async(&root).await }
        },
        move || {
            let root = root.clone();
            let database = Arc::clone(&database);
            let database_write_lock = Arc::clone(&database_write_lock);
            async move {
                let _guard = database_write_lock.lock().await;
                sync_channel_state_with_database(&root, &database).await
            }
        },
    )
    .await;
}

pub(super) async fn sync_business_users_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let stamp_root = root.clone();
    run_background_projection_loop(
        BUSINESS_USERS_PROJECTION_LOOP,
        move || {
            let root = stamp_root.clone();
            async move { business_users_projection_stamp(&root).await }
        },
        move || {
            let root = root.clone();
            let database = Arc::clone(&database);
            let database_write_lock = Arc::clone(&database_write_lock);
            async move {
                let _guard = database_write_lock.lock().await;
                sync_business_users_with_database(&root, &database).await
            }
        },
    )
    .await;
}

pub(super) async fn sync_runtime_settings_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let stamp_root = root.clone();
    run_background_projection_loop(
        RUNTIME_SETTINGS_PROJECTION_LOOP,
        move || {
            let root = stamp_root.clone();
            async move { runtime_settings_projection_stamp(&root).await }
        },
        move || {
            let root = root.clone();
            let database = Arc::clone(&database);
            let database_write_lock = Arc::clone(&database_write_lock);
            async move {
                let _guard = database_write_lock.lock().await;
                sync_runtime_settings_with_database(&root, &database).await
            }
        },
    )
    .await;
}

pub(super) async fn sync_workspace_branding_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let stamp_root = root.clone();
    run_background_projection_loop(
        WORKSPACE_BRANDING_PROJECTION_LOOP,
        move || {
            let root = stamp_root.clone();
            async move { workspace_branding_projection_stamp(&root).await }
        },
        move || {
            let root = root.clone();
            let database = Arc::clone(&database);
            let database_write_lock = Arc::clone(&database_write_lock);
            async move {
                let _guard = database_write_lock.lock().await;
                sync_workspace_branding_with_database(&root, &database).await
            }
        },
    )
    .await;
}

pub(super) async fn sync_module_catalog_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let stamp_root = root.clone();
    run_background_projection_loop(
        MODULE_CATALOG_PROJECTION_LOOP,
        move || {
            let root = stamp_root.clone();
            async move { module_catalog_projection_stamp(&root).await }
        },
        move || {
            let root = root.clone();
            let database = Arc::clone(&database);
            let database_write_lock = Arc::clone(&database_write_lock);
            async move {
                let _guard = database_write_lock.lock().await;
                sync_module_catalog_with_database(&root, &database).await
            }
        },
    )
    .await;
}

pub(super) async fn sync_ticket_state_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    _database_write_lock: Arc<AsyncMutex<()>>,
) {
    let stamp_root = root.clone();
    run_background_projection_loop(
        TICKET_STATE_PROJECTION_LOOP,
        move || {
            let root = stamp_root.clone();
            async move { ticket_state_source_stamp(&root).await }
        },
        move || {
            let root = root.clone();
            let database = Arc::clone(&database);
            async move {
                // This collection family has one writer. Source loading must
                // not hold the cross-loop lock; writes yield between batches.
                sync_ticket_state_with_database(&root, &database).await
            }
        },
    )
    .await;
}

pub(super) async fn sync_knowledge_tables_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    _database_write_lock: Arc<AsyncMutex<()>>,
) {
    let stamp_root = root.clone();
    run_background_projection_loop(
        KNOWLEDGE_TABLES_PROJECTION_LOOP,
        move || {
            let root = stamp_root.clone();
            async move { knowledge_tables_source_stamp(&root).await }
        },
        move || {
            let root = root.clone();
            let database = Arc::clone(&database);
            async move { sync_knowledge_tables_with_database(&root, &database).await }
        },
    )
    .await;
}

/// Bound the shared writer's occupancy by both count and approximate payload
/// bytes. A large first projection may keep working, but intake gets a turn
/// between pages. Never cancel a partially written batch on a wall timer.
pub(super) async fn upsert_background_projection_pages(
    collection: &Arc<RxCollection>,
    collection_name: &str,
    documents: Vec<Value>,
) -> anyhow::Result<usize> {
    let mut documents = documents.into_iter().peekable();
    let mut count = 0;
    while documents.peek().is_some() {
        let mut page = Vec::new();
        let mut bytes = 0;
        while page.len() < 16 && (page.is_empty() || bytes < 256 * 1024) {
            let Some(document) = documents.next() else {
                break;
            };
            bytes += serde_json::to_vec(&document)?.len();
            page.push(document);
        }
        {
            let _guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
            count +=
                bulk_upsert_business_record_projection_documents(collection, collection_name, page)
                    .await?;
        }
        tokio::task::yield_now().await;
    }
    Ok(count)
}

pub(super) fn support_projection_collection(collection: &str) -> Option<&'static str> {
    match collection {
        "support_inboxes" => Some("support_inboxes"),
        "support_conversations" => Some("support_conversations"),
        "support_thread_links" => Some("support_thread_links"),
        "support_identity_links" => Some("support_identity_links"),
        "support_notes" => Some("support_notes"),
        "support_conversation_events" => Some("support_conversation_events"),
        "support_labels" => Some("support_labels"),
        "support_label_assignments" => Some("support_label_assignments"),
        "support_views" => Some("support_views"),
        "support_view_filters" => Some("support_view_filters"),
        "support_assignment_policies" => Some("support_assignment_policies"),
        "support_assignment_events" => Some("support_assignment_events"),
        "support_macros" => Some("support_macros"),
        "support_automation_rules" => Some("support_automation_rules"),
        "support_sla_policies" => Some("support_sla_policies"),
        "support_applied_slas" => Some("support_applied_slas"),
        "support_sla_events" => Some("support_sla_events"),
        "support_agent_requests" => Some("support_agent_requests"),
        "support_agent_suggestions" => Some("support_agent_suggestions"),
        "support_reporting_events" => Some("support_reporting_events"),
        "support_reporting_rollups" => Some("support_reporting_rollups"),
        _ => None,
    }
}

pub(super) fn appsec_projection_collection(collection: &str) -> Option<&'static str> {
    match collection {
        "appsec_assessments" => Some("appsec_assessments"),
        "appsec_runs" => Some("appsec_runs"),
        "appsec_artifacts" => Some("appsec_artifacts"),
        "appsec_findings" => Some("appsec_findings"),
        "appsec_investigations" => Some("appsec_investigations"),
        "appsec_coverage" => Some("appsec_coverage"),
        "appsec_pipeline_stages" => Some("appsec_pipeline_stages"),
        "appsec_scanner_inventory" => Some("appsec_scanner_inventory"),
        "appsec_approvals" => Some("appsec_approvals"),
        _ => None,
    }
}

pub(super) fn threads_projection_collection(collection: &str) -> Option<&'static str> {
    match collection {
        "user_threads" => Some("user_threads"),
        "user_thread_messages" => Some("user_thread_messages"),
        "user_thread_links" => Some("user_thread_links"),
        "user_notifications" => Some("user_notifications"),
        "ctox_task_approval_requests" => Some("ctox_task_approval_requests"),
        _ => None,
    }
}

pub(super) async fn find_projection_documents_by_id(
    collection: &Arc<RxCollection>,
    collection_name: &str,
    ids: BTreeSet<String>,
) -> anyhow::Result<HashMap<String, Value>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    #[cfg(test)]
    record_chat_tracking_batch_document_lookup(&collection.database.name);
    let ids = ids.into_iter().collect::<Vec<_>>();
    let documents = collection
        .storage_instance
        .find_documents_by_id(&ids, false)
        .await
        .map_err(|err| anyhow::anyhow!("find {collection_name} projection documents: {err}"))?;
    let mut by_id = HashMap::with_capacity(documents.len());
    for document in documents {
        let Some(id) = document
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        by_id.insert(id, document);
    }
    Ok(by_id)
}

pub(super) async fn upsert_business_record_projection_document(
    collection: &Arc<RxCollection>,
    collection_name: &str,
    mut document: Value,
) -> anyhow::Result<()> {
    if is_projection_tombstone(&document) {
        remove_projection_rxdb_envelope(&mut document);
        return upsert_business_record_projection_tombstone(collection, document).await;
    }
    normalize_business_record_projection_document(collection, collection_name, &mut document)?;
    let document = fill_projection_document_envelope(collection, document, collection_name)?;
    collection
        .upsert(document)
        .await
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!("upsert {collection_name} projection: {err}"))
}

/// Project a pulled collection batch without turning every record into its own
/// SQLite transaction. The old per-document `collection.upsert()` loop made a
/// cold native-peer start perform tens of thousands of transactions while it
/// held the projection writer lock; on a realistic Business OS store this
/// blocked command ingestion and browser replication for 6-7 minutes.
pub(super) async fn bulk_upsert_business_record_projection_documents(
    collection: &Arc<RxCollection>,
    collection_name: &str,
    documents: Vec<Value>,
) -> anyhow::Result<usize> {
    if documents.is_empty() {
        return Ok(0);
    }
    let schema = collection
        .schema_required()
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let primary_path = schema.primary_path.clone();
    let mut normal_documents = Vec::with_capacity(documents.len());
    let mut tombstones = Vec::new();
    for mut document in documents {
        if let Some(object) = document.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
            object
                .entry("is_deleted".to_string())
                .or_insert_with(|| Value::Bool(false));
        }
        if is_projection_tombstone(&document) {
            tombstones.push(document);
            continue;
        }
        normalize_business_record_projection_document(collection, collection_name, &mut document)?;
        normal_documents.push(fill_projection_document_envelope(
            collection,
            document,
            collection_name,
        )?);
    }

    let mut existing_by_id = HashMap::<String, Value>::new();
    for id_batch in normal_documents
        .iter()
        .filter_map(|document| {
            document
                .get(&primary_path)
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>()
        .chunks(BUSINESS_RECORD_PROJECTION_WRITE_BATCH_SIZE)
    {
        for existing in collection
            .storage_instance
            .find_documents_by_id(id_batch, true)
            .await
            .map_err(|err| anyhow::anyhow!("load existing projection batch: {err}"))?
        {
            if let Some(id) = existing
                .get(&primary_path)
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                existing_by_id.insert(id, existing);
            }
        }
    }

    let changed_documents = normal_documents
        .into_iter()
        .filter(|document| {
            let Some(id) = document.get(&primary_path).and_then(Value::as_str) else {
                return true;
            };
            existing_by_id.get(id).map_or(true, |existing| {
                !projection_document_has_valid_revision(existing)
                    || canonical_projection_document_for_compare(existing)
                        != canonical_projection_document_for_compare(document)
            })
        })
        .collect::<Vec<_>>();

    let mut changed = 0usize;
    for batch in changed_documents.chunks(BUSINESS_RECORD_PROJECTION_WRITE_BATCH_SIZE) {
        let batch_documents = batch.to_vec();
        let result = collection
            .bulk_upsert(batch_documents.clone())
            .await
            .map_err(|err| anyhow::anyhow!("bulk upsert projection batch: {err}"))?;
        changed = changed.saturating_add(result.success.len());
        if result.error.is_empty() {
            continue;
        }
        let failed_ids = result
            .error
            .iter()
            .map(|error| error.document_id.as_str())
            .collect::<HashSet<_>>();
        for document in batch_documents {
            let Some(id) = document.get(&primary_path).and_then(Value::as_str) else {
                continue;
            };
            if !failed_ids.contains(id) {
                continue;
            }
            upsert_business_record_projection_document(collection, collection_name, document)
                .await?;
            changed = changed.saturating_add(1);
        }
    }

    for tombstone in tombstones {
        upsert_business_record_projection_document(collection, collection_name, tombstone).await?;
        changed = changed.saturating_add(1);
    }
    Ok(changed)
}

// Keep the module-size contract's final test marker after all production code;
// behavioral coverage remains in rxdb_peer.rs so private semantic state stays local.
#[cfg(test)]
mod tests {}
