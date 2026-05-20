//! Port of `src/replication-protocol/downstream.ts`.
//!
//! Functional downstream replication protocol port for CTOX core.
//! - Conflict-aware initial sync (full 4-case decision in `persist_from_master`).
//! - Ongoing subscription to `replication_handler.master_change_stream()`:
//!   each `DocumentsWithCheckpoint` batch from the master is run through
//!   `persist_from_master`, while `"RESYNC"` sentinels trigger a paginated
//!   downstream resync.
//! - Per-batch `stream_queue.down` locking plus resync time cutoffs so
//!   already-covered master stream events cannot roll checkpoints back.
//! - Immediately buffered master stream document batches are coalesced into
//!   one `persist_from_master` call with stacked checkpoints. This is the Rust
//!   equivalent of RxDB's `addNewTask`/`openTasks` Promise-chain batching.
//! - Cancel handling: ongoing task aborts when `state.events.canceled` flips.
//! - `persist_from_master` deduplicates incoming master documents by primary key
//!   before writing, replacing RxDB's `nonPersistedFromMaster` Promise queue with
//!   a serialized, ownership-safe Rust path.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering as AtomicOrdering};
use std::sync::Arc;

use futures::{FutureExt, StreamExt};
use serde_json::{json, Value};

use crate::replication_protocol::checkpoint::{get_last_checkpoint_doc, set_checkpoint};
use crate::replication_protocol::helper::{
    doc_state_to_write_doc, remote_revision_height_marker_matches,
    strip_attachments_data_from_meta_write_rows, write_doc_to_doc_state,
};
use crate::replication_protocol::meta_instance::{
    get_assumed_master_state, get_meta_write_row, AssumedMaster,
};
use crate::rx_storage_helper::stack_checkpoints;
use crate::types::{
    BulkWriteRow, RxReplicationMasterChange, RxStorageInstanceReplicationState,
    RxStorageReplicationDirection,
};

// ref: rxdb/src/replication-protocol/downstream.ts:51-167
pub async fn start_replication_downstream(state: Arc<RxStorageInstanceReplicationState>) {
    // 1. Initial checkpoint write.
    if let Some(initial) = state.input.initial_checkpoint.as_ref() {
        if let Some(cp) = initial.downstream.as_ref() {
            match get_last_checkpoint_doc(&state, RxStorageReplicationDirection::Down).await {
                Ok(None) => {
                    if let Err(e) =
                        set_checkpoint(&state, RxStorageReplicationDirection::Down, cp.clone())
                            .await
                    {
                        tracing::error!(
                            target: "ctox_rxdb::replication_protocol::downstream",
                            "initial checkpoint write failed: {e}",
                        );
                        return;
                    }
                }
                Ok(Some(_)) => {}
                Err(e) => {
                    tracing::error!(
                        target: "ctox_rxdb::replication_protocol::downstream",
                        "get_last_checkpoint_doc failed: {e}",
                    );
                    return;
                }
            }
        }
    }

    // 2. Spawn ongoing master.change_stream subscription early.
    let timer = Arc::new(AtomicI64::new(0));
    let last_time_master_changes_requested = Arc::new(AtomicI64::new(-1));
    let ongoing = spawn_ongoing_downstream_with_timing(
        Arc::clone(&state),
        Arc::clone(&timer),
        Arc::clone(&last_time_master_changes_requested),
    );

    // 3. Initial resync.
    if let Err(e) =
        downstream_resync_once_with_timing(&state, &timer, &last_time_master_changes_requested)
            .await
    {
        tracing::error!(
            target: "ctox_rxdb::replication_protocol::downstream",
            "downstreamResyncOnce failed: {e}",
        );
    }
    if !state.first_sync_done.down.get_value() && !state.events.canceled.get_value() {
        state.first_sync_done.down.next(true);
    }

    // 4. Stay alive until canceled, then drop the subscription.
    wait_for_cancel_down(&state).await;
    ongoing.abort();
}

/// Spawn the long-lived task that consumes `replication_handler.master_change_stream()`
/// and writes each batch to the fork via `persist_from_master`.
#[cfg(test)]
fn spawn_ongoing_downstream(
    state: Arc<RxStorageInstanceReplicationState>,
) -> tokio::task::JoinHandle<()> {
    spawn_ongoing_downstream_with_timing(
        state,
        Arc::new(AtomicI64::new(0)),
        Arc::new(AtomicI64::new(-1)),
    )
}

fn spawn_ongoing_downstream_with_timing(
    state: Arc<RxStorageInstanceReplicationState>,
    timer: Arc<AtomicI64>,
    last_time_master_changes_requested: Arc<AtomicI64>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut stream = state.input.replication_handler.master_change_stream();
        while let Some(master_change) = stream.next().await {
            if state.events.canceled.get_value() {
                break;
            }
            let task_time = timer.fetch_add(1, AtomicOrdering::SeqCst);
            wait_until_upstream_inactive(&state).await;
            if task_time < last_time_master_changes_requested.load(AtomicOrdering::SeqCst) {
                continue;
            }
            match master_change {
                RxReplicationMasterChange::Resync => {
                    run_downstream_resync_from_stream(
                        &state,
                        &timer,
                        &last_time_master_changes_requested,
                    )
                    .await;
                }
                RxReplicationMasterChange::Documents(docs_with_cp) => {
                    let mut tasks = vec![(task_time, docs_with_cp)];
                    let mut run_resync_after_batch = false;
                    while let Some(Some(next_master_change)) = stream.next().now_or_never() {
                        if state.events.canceled.get_value() {
                            break;
                        }
                        let next_task_time = timer.fetch_add(1, AtomicOrdering::SeqCst);
                        match next_master_change {
                            RxReplicationMasterChange::Documents(next_docs_with_cp) => {
                                tasks.push((next_task_time, next_docs_with_cp));
                            }
                            RxReplicationMasterChange::Resync => {
                                run_resync_after_batch = true;
                                break;
                            }
                        }
                    }
                    if let Err(e) = process_downstream_master_change_tasks(
                        &state,
                        &last_time_master_changes_requested,
                        tasks,
                    )
                    .await
                    {
                        tracing::error!(
                            target: "ctox_rxdb::replication_protocol::downstream",
                            "ongoing persist_from_master failed: {e}",
                        );
                    }
                    if run_resync_after_batch {
                        run_downstream_resync_from_stream(
                            &state,
                            &timer,
                            &last_time_master_changes_requested,
                        )
                        .await;
                    }
                }
            }
        }
    })
}

async fn process_downstream_master_change_tasks(
    state: &Arc<RxStorageInstanceReplicationState>,
    last_time_master_changes_requested: &AtomicI64,
    tasks: Vec<(i64, crate::types::DocumentsWithCheckpoint)>,
) -> Result<(), crate::rx_error::RxError> {
    let cutoff = last_time_master_changes_requested.load(AtomicOrdering::SeqCst);
    let mut docs = Vec::new();
    let mut checkpoints = Vec::new();
    let mut emit_count = 0;
    for (task_time, docs_with_cp) in tasks {
        if task_time < cutoff || docs_with_cp.documents.is_empty() {
            continue;
        }
        emit_count += 1;
        docs.extend(docs_with_cp.documents);
        checkpoints.push(Some(docs_with_cp.checkpoint));
    }
    if docs.is_empty() && checkpoints.is_empty() {
        return Ok(());
    }
    {
        let mut stats = state.stats.down.lock();
        stats.master_change_stream_emit += emit_count;
    }
    let checkpoint = stack_checkpoints(&checkpoints);
    let _g = state.stream_queue.down.lock().await;
    state.events.active.down.next(true);
    let result = persist_from_master(state, docs, checkpoint).await;
    state.events.active.down.next(false);
    result
}

async fn run_downstream_resync_from_stream(
    state: &Arc<RxStorageInstanceReplicationState>,
    timer: &AtomicI64,
    last_time_master_changes_requested: &AtomicI64,
) {
    state.events.active.down.next(true);
    if let Err(e) =
        downstream_resync_once_with_timing(state, timer, last_time_master_changes_requested).await
    {
        tracing::error!(
            target: "ctox_rxdb::replication_protocol::downstream",
            "RESYNC downstreamResyncOnce failed: {e}",
        );
    }
    state.events.active.down.next(false);
}

async fn wait_until_upstream_inactive(state: &RxStorageInstanceReplicationState) {
    if !state.events.active.up.get_value() {
        return;
    }
    let mut upstream_active = state.events.active.up.subscribe();
    while let Some(is_active) = upstream_active.next().await {
        if !is_active {
            return;
        }
    }
}

async fn wait_for_cancel_down(state: &Arc<RxStorageInstanceReplicationState>) {
    if state.events.canceled.get_value() {
        return;
    }
    let mut s = state.events.canceled.subscribe();
    while let Some(v) = s.next().await {
        if v {
            return;
        }
    }
}

// ref: rxdb/src/replication-protocol/downstream.ts:175-217
async fn downstream_resync_once_with_timing(
    state: &Arc<RxStorageInstanceReplicationState>,
    timer: &AtomicI64,
    last_time_master_changes_requested: &AtomicI64,
) -> Result<(), crate::rx_error::RxError> {
    {
        let mut stats = state.stats.down.lock();
        stats.downstream_resync_once += 1;
    }
    if state.events.canceled.get_value() {
        return Ok(());
    }
    let last_checkpoint_doc =
        get_last_checkpoint_doc(state, RxStorageReplicationDirection::Down).await?;
    let mut last_checkpoint: Value = last_checkpoint_doc.unwrap_or(Value::Null);
    let pull_batch_size = state.input.pull_batch_size;

    while !state.events.canceled.get_value() {
        // Acquire stream_queue.down per batch so ongoing events can interleave.
        let _g = state.stream_queue.down.lock().await;
        let request_time = timer.fetch_add(1, AtomicOrdering::SeqCst);
        last_time_master_changes_requested.store(request_time, AtomicOrdering::SeqCst);
        let down_result = state
            .input
            .replication_handler
            .master_changes_since(
                if last_checkpoint.is_null() {
                    None
                } else {
                    Some(last_checkpoint.clone())
                },
                pull_batch_size,
            )
            .await?;
        if down_result.documents.is_empty() {
            break;
        }
        last_checkpoint = stack_checkpoints(&[
            if last_checkpoint.is_null() {
                None
            } else {
                Some(last_checkpoint.clone())
            },
            Some(down_result.checkpoint.clone()),
        ]);
        persist_from_master(
            state,
            down_result.documents.clone(),
            last_checkpoint.clone(),
        )
        .await?;
        let small = (down_result.documents.len() as u64) < pull_batch_size;
        drop(_g);
        if small {
            break;
        }
    }
    Ok(())
}

// ref: rxdb/src/replication-protocol/downstream.ts:255-end (persistFromMaster, FULL conflict-aware version)
async fn persist_from_master(
    state: &Arc<RxStorageInstanceReplicationState>,
    docs: Vec<Value>,
    new_down_checkpoint: Value,
) -> Result<(), crate::rx_error::RxError> {
    {
        let mut stats = state.stats.down.lock();
        stats.persist_from_master += 1;
    }
    if docs.is_empty() {
        return set_checkpoint(
            state,
            RxStorageReplicationDirection::Down,
            new_down_checkpoint,
        )
        .await;
    }

    let primary_path = state.primary_path.clone();
    let has_attachments = state.has_attachments;
    let keep_meta = state.input.keep_meta;

    // Build downDocsById for quick lookup.
    let mut down_docs_by_id: HashMap<String, Value> = HashMap::new();
    for d in docs.into_iter() {
        if let Some(id) = d.get(&primary_path).and_then(|v| v.as_str()) {
            down_docs_by_id.insert(id.to_string(), d);
        }
    }
    let doc_ids: Vec<String> = down_docs_by_id.keys().cloned().collect();

    // Read fork state + assumed-master state in parallel.
    let (fork_state_list, assumed_master_state) = tokio::join!(
        state
            .input
            .fork_instance
            .find_documents_by_id(&doc_ids, true),
        get_assumed_master_state(state, &doc_ids),
    );
    let fork_state_list = fork_state_list?;
    let assumed_master_state = assumed_master_state?;

    let mut fork_state_by_id: HashMap<String, Value> = HashMap::new();
    for ex in fork_state_list.into_iter() {
        if let Some(id) = ex.get(&primary_path).and_then(|v| v.as_str()) {
            fork_state_by_id.insert(id.to_string(), ex);
        }
    }

    let mut write_rows_to_fork: Vec<BulkWriteRow> = Vec::new();
    let mut write_rows_to_meta: Vec<BulkWriteRow> = Vec::new();

    for (doc_id, master_doc_state) in down_docs_by_id.iter() {
        let fork_state = fork_state_by_id.get(doc_id).cloned();
        let assumed_master = assumed_master_state.get(doc_id).cloned();
        let prepared_master = doc_state_to_write_doc(
            &state.checkpoint_key,
            has_attachments,
            keep_meta,
            master_doc_state,
            None,
        );

        match (fork_state, assumed_master) {
            (None, _) => {
                // No fork doc — straight insert.
                write_rows_to_fork.push(BulkWriteRow {
                    previous: None,
                    document: prepared_master.clone(),
                });
                let meta_row = get_meta_write_row(
                    state,
                    &write_doc_to_doc_state(&prepared_master, has_attachments, keep_meta),
                    None,
                    None,
                )
                .await?;
                write_rows_to_meta.push(meta_row);
            }
            (Some(fork), Some(asm)) => {
                if asm
                    .meta_document
                    .get("isResolvedConflict")
                    .and_then(Value::as_str)
                    == fork.get("_rev").and_then(Value::as_str)
                {
                    let _up_queue_guard = state.stream_queue.up.lock().await;
                    drop(_up_queue_guard);
                }
                let fork_clean = write_doc_to_doc_state(&fork, has_attachments, keep_meta);
                let master_clean =
                    write_doc_to_doc_state(master_doc_state, has_attachments, keep_meta);
                let already_equal = state
                    .input
                    .conflict_handler
                    .is_equal(&fork_clean, &master_clean, "downstream-already-equal")
                    .await;
                if already_equal {
                    // Skip — fork already mirrors master. Refresh meta-doc for
                    // bookkeeping (assumed_master_state.docData = master).
                    let meta_row =
                        get_meta_write_row(state, &master_clean, Some(&asm.meta_document), None)
                            .await?;
                    write_rows_to_meta.push(meta_row);
                    continue;
                }
                // Compare against assumed master: did the fork diverge?
                let mut fork_matches_assumed = state
                    .input
                    .conflict_handler
                    .is_equal(&fork_clean, &asm.doc_data, "downstream-vs-assumed")
                    .await;
                if !fork_matches_assumed
                    && asm.doc_data.get("_rev").is_some()
                    && remote_revision_height_marker_matches(&fork, &state.input.identifier)
                {
                    fork_matches_assumed = true;
                }
                if fork_matches_assumed {
                    // No conflict — fast-forward fork to master.
                    write_rows_to_fork.push(BulkWriteRow {
                        previous: Some(fork.clone()),
                        document: doc_state_to_write_doc(
                            &state.checkpoint_key,
                            has_attachments,
                            keep_meta,
                            master_doc_state,
                            Some(&fork),
                        ),
                    });
                    let meta_row =
                        get_meta_write_row(state, &master_clean, Some(&asm.meta_document), None)
                            .await?;
                    write_rows_to_meta.push(meta_row);
                } else {
                    // Local fork has unpushed changes. Downstream must not
                    // resolve or overwrite them; upstream will resolve later.
                    continue;
                }
            }
            (Some(_fork), None) => {
                // Existing local doc without assumed-master state is an
                // unreplicated local write. Skip downstream overwrite.
                continue;
            }
        }
    }

    if !write_rows_to_fork.is_empty() {
        let context = state.downstream_bulk_write_flag.clone();
        let write_rows_for_emit = write_rows_to_fork.clone();
        let result = state
            .input
            .fork_instance
            .bulk_write(write_rows_to_fork, &context)
            .await?;
        let failed_ids: std::collections::HashSet<String> = result
            .error
            .iter()
            .map(|error| error.document_id.clone())
            .collect();
        for row in write_rows_for_emit.iter() {
            let Some(id) = row
                .document
                .get(&primary_path)
                .and_then(|value| value.as_str())
            else {
                continue;
            };
            if failed_ids.contains(id) {
                continue;
            }
            state.events.processed.down.next(json!({
                "document": write_doc_to_doc_state(&row.document, has_attachments, keep_meta),
            }));
        }
    }
    if !write_rows_to_meta.is_empty() {
        let _ = state
            .input
            .meta_instance
            .bulk_write(
                strip_attachments_data_from_meta_write_rows(state, &write_rows_to_meta),
                "replication-meta-write",
            )
            .await?;
    }

    set_checkpoint(
        state,
        RxStorageReplicationDirection::Down,
        new_down_checkpoint,
    )
    .await?;
    Ok(())
}

#[allow(dead_code)]
fn _phantom_use(_a: &AssumedMaster) -> Value {
    json!({})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use serde_json::json;

    use crate::plugins::storage_memory::get_rx_storage_memory;
    use crate::replication_protocol::default_conflict_handler::DefaultConflictHandler;
    use crate::replication_protocol::meta_instance::get_rx_replication_meta_instance_schema;
    use crate::rx_schema_helper::fill_with_default_settings;
    use crate::rxjs_compat::{RxStream, RxSubject};
    use crate::types::{
        DocumentsWithCheckpoint, FirstSyncDone, HashFunction, HashOutput, JsonSchema, PrimaryKey,
        ReplicationEvents, ReplicationStats, RxJsonSchema, RxReplicationHandler,
        RxReplicationMasterChange, RxStorageInstance, RxStorageInstanceCreationParams,
        RxStorageInstanceReplicationInput, RxStorageInstanceReplicationState,
        RxStorageReplicationDirection, StreamQueue,
    };

    struct TestHashFunction;

    impl HashFunction for TestHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("hash:{input}") })
        }
    }

    struct NoopReplicationHandler;

    #[async_trait]
    impl RxReplicationHandler for NoopReplicationHandler {
        fn master_change_stream(&self) -> RxStream<RxReplicationMasterChange> {
            Box::pin(tokio_stream::empty())
        }

        async fn master_changes_since(
            &self,
            _checkpoint: Option<Value>,
            _batch_size: u64,
        ) -> Result<DocumentsWithCheckpoint, crate::rx_error::RxError> {
            Ok(DocumentsWithCheckpoint {
                documents: Vec::new(),
                checkpoint: Value::Null,
            })
        }

        async fn master_write(
            &self,
            _rows: Vec<crate::types::RxReplicationWriteToMasterRow>,
        ) -> Result<Vec<Value>, crate::rx_error::RxError> {
            Ok(Vec::new())
        }
    }

    struct StreamReplicationHandler {
        stream: RxSubject<RxReplicationMasterChange>,
    }

    #[async_trait]
    impl RxReplicationHandler for StreamReplicationHandler {
        fn master_change_stream(&self) -> RxStream<RxReplicationMasterChange> {
            self.stream.subscribe()
        }

        async fn master_changes_since(
            &self,
            _checkpoint: Option<Value>,
            _batch_size: u64,
        ) -> Result<DocumentsWithCheckpoint, crate::rx_error::RxError> {
            Ok(DocumentsWithCheckpoint {
                documents: Vec::new(),
                checkpoint: Value::Null,
            })
        }

        async fn master_write(
            &self,
            _rows: Vec<crate::types::RxReplicationWriteToMasterRow>,
        ) -> Result<Vec<Value>, crate::rx_error::RxError> {
            Ok(Vec::new())
        }
    }

    struct ResyncPullReplicationHandler {
        stream: RxSubject<RxReplicationMasterChange>,
        master_changes_since_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl RxReplicationHandler for ResyncPullReplicationHandler {
        fn master_change_stream(&self) -> RxStream<RxReplicationMasterChange> {
            self.stream.subscribe()
        }

        async fn master_changes_since(
            &self,
            _checkpoint: Option<Value>,
            _batch_size: u64,
        ) -> Result<DocumentsWithCheckpoint, crate::rx_error::RxError> {
            if self
                .master_changes_since_calls
                .fetch_add(1, Ordering::SeqCst)
                == 0
            {
                Ok(DocumentsWithCheckpoint {
                    documents: vec![json!({
                        "id": "a",
                        "age": 7,
                        "_deleted": false
                    })],
                    checkpoint: json!({ "sequence": 7 }),
                })
            } else {
                Ok(DocumentsWithCheckpoint {
                    documents: Vec::new(),
                    checkpoint: json!({ "sequence": 7 }),
                })
            }
        }

        async fn master_write(
            &self,
            _rows: Vec<crate::types::RxReplicationWriteToMasterRow>,
        ) -> Result<Vec<Value>, crate::rx_error::RxError> {
            Ok(Vec::new())
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
        properties.insert(
            "age".to_string(),
            JsonSchema {
                schema_type: Some("number".to_string()),
                ..Default::default()
            },
        );
        fill_with_default_settings(RxJsonSchema {
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
        })
    }

    fn fork_doc(age: i64, rev: &str) -> Value {
        json!({
            "id": "a",
            "age": age,
            "_deleted": false,
            "_attachments": {},
            "_rev": rev,
            "_meta": { "lwt": 1.0 },
        })
    }

    #[tokio::test]
    async fn downstream_skips_unpushed_local_fork_conflicts() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-conflict".to_string(),
                    collection_name: "docs".to_string(),
                    schema: schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        fork_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: fork_doc(2, "1-local"),
                }],
                "seed-local",
            )
            .await
            .unwrap();
        let meta_schema = get_rx_replication_meta_instance_schema(&schema, false).unwrap();
        let meta_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-conflict".to_string(),
                    collection_name: "meta".to_string(),
                    schema: meta_schema,
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: Arc::clone(&fork_instance),
            meta_instance: Arc::clone(&meta_instance),
            hash_function: Arc::new(TestHashFunction),
            conflict_handler: Arc::new(DefaultConflictHandler),
            replication_handler: Arc::new(NoopReplicationHandler),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: Arc::new(input),
            checkpoint_key: "checkpoint".to_string(),
            downstream_bulk_write_flag: "downstream".to_string(),
            last_checkpoint_doc: parking_lot::Mutex::new(HashMap::new()),
            events: ReplicationEvents::new(),
            stats: ReplicationStats::new(),
            first_sync_done: FirstSyncDone::default(),
            stream_queue: StreamQueue::default(),
            checkpoint_queue: tokio::sync::Mutex::new(()),
            has_attachments: false,
        });
        let assumed_master = json!({
            "id": "a",
            "age": 1,
            "_deleted": false
        });
        let meta_row = get_meta_write_row(&state, &assumed_master, None, None)
            .await
            .unwrap();
        meta_instance
            .bulk_write(vec![meta_row], "seed-assumed-master")
            .await
            .unwrap();

        persist_from_master(
            &state,
            vec![json!({
                "id": "a",
                "age": 3,
                "_deleted": false
            })],
            json!({ "sequence": 1 }),
        )
        .await
        .unwrap();

        let fork_docs = fork_instance
            .find_documents_by_id(&["a".to_string()], true)
            .await
            .unwrap();
        assert_eq!(fork_docs[0]["age"], json!(2));
        let assumed_after = get_assumed_master_state(&state, &["a".to_string()])
            .await
            .unwrap();
        assert_eq!(assumed_after["a"].doc_data["age"], json!(1));
        let checkpoint = get_last_checkpoint_doc(&state, RxStorageReplicationDirection::Down)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(checkpoint["sequence"], json!(1));
    }

    #[tokio::test]
    async fn downstream_fast_forwards_remote_revision_marked_fork_state() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-rev-height".to_string(),
                    collection_name: "docs".to_string(),
                    schema: schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        fork_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: json!({
                        "id": "a",
                        "age": 2,
                        "_deleted": false,
                        "_attachments": {},
                        "_rev": "2-local",
                        "_meta": {
                            "lwt": 1.0,
                            "replication-test": 2
                        },
                    }),
                }],
                "seed-local",
            )
            .await
            .unwrap();
        let meta_schema = get_rx_replication_meta_instance_schema(&schema, false).unwrap();
        let meta_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-rev-height".to_string(),
                    collection_name: "meta".to_string(),
                    schema: meta_schema,
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: Arc::clone(&fork_instance),
            meta_instance: Arc::clone(&meta_instance),
            hash_function: Arc::new(TestHashFunction),
            conflict_handler: Arc::new(DefaultConflictHandler),
            replication_handler: Arc::new(NoopReplicationHandler),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: Arc::new(input),
            checkpoint_key: "checkpoint".to_string(),
            downstream_bulk_write_flag: "downstream".to_string(),
            last_checkpoint_doc: parking_lot::Mutex::new(HashMap::new()),
            events: ReplicationEvents::new(),
            stats: ReplicationStats::new(),
            first_sync_done: FirstSyncDone::default(),
            stream_queue: StreamQueue::default(),
            checkpoint_queue: tokio::sync::Mutex::new(()),
            has_attachments: false,
        });
        let assumed_master = json!({
            "id": "a",
            "age": 1,
            "_deleted": false,
            "_rev": "2-master"
        });
        let meta_row = get_meta_write_row(&state, &assumed_master, None, None)
            .await
            .unwrap();
        meta_instance
            .bulk_write(vec![meta_row], "seed-assumed-master")
            .await
            .unwrap();

        persist_from_master(
            &state,
            vec![json!({
                "id": "a",
                "age": 3,
                "_deleted": false,
                "_rev": "3-master"
            })],
            json!({ "sequence": 2 }),
        )
        .await
        .unwrap();

        let fork_docs = fork_instance
            .find_documents_by_id(&["a".to_string()], true)
            .await
            .unwrap();
        assert_eq!(fork_docs[0]["age"], json!(3));
        let assumed_after = get_assumed_master_state(&state, &["a".to_string()])
            .await
            .unwrap();
        assert_eq!(assumed_after["a"].doc_data["age"], json!(3));
    }

    #[tokio::test]
    async fn downstream_waits_for_upstream_queue_when_fork_is_resolved_conflict() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-resolved-conflict-wait".to_string(),
                    collection_name: "docs".to_string(),
                    schema: schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        fork_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: fork_doc(2, "2-resolved"),
                }],
                "seed-local",
            )
            .await
            .unwrap();
        let meta_schema = get_rx_replication_meta_instance_schema(&schema, false).unwrap();
        let meta_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-resolved-conflict-wait".to_string(),
                    collection_name: "meta".to_string(),
                    schema: meta_schema,
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: Arc::clone(&fork_instance),
            meta_instance: Arc::clone(&meta_instance),
            hash_function: Arc::new(TestHashFunction),
            conflict_handler: Arc::new(DefaultConflictHandler),
            replication_handler: Arc::new(NoopReplicationHandler),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: Arc::new(input),
            checkpoint_key: "checkpoint".to_string(),
            downstream_bulk_write_flag: "downstream".to_string(),
            last_checkpoint_doc: parking_lot::Mutex::new(HashMap::new()),
            events: ReplicationEvents::new(),
            stats: ReplicationStats::new(),
            first_sync_done: FirstSyncDone::default(),
            stream_queue: StreamQueue::default(),
            checkpoint_queue: tokio::sync::Mutex::new(()),
            has_attachments: false,
        });
        let assumed_master = json!({
            "id": "a",
            "age": 1,
            "_deleted": false
        });
        let meta_row = get_meta_write_row(&state, &assumed_master, None, Some("2-resolved"))
            .await
            .unwrap();
        meta_instance
            .bulk_write(vec![meta_row], "seed-assumed-master")
            .await
            .unwrap();

        let up_queue_guard = state.stream_queue.up.lock().await;
        let state_for_task = Arc::clone(&state);
        let persist_task = tokio::spawn(async move {
            persist_from_master(
                &state_for_task,
                vec![json!({
                    "id": "a",
                    "age": 3,
                    "_deleted": false
                })],
                json!({ "sequence": 3 }),
            )
            .await
            .unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let fork_docs = fork_instance
            .find_documents_by_id(&["a".to_string()], true)
            .await
            .unwrap();
        assert_eq!(fork_docs[0]["age"], json!(2));
        assert!(!persist_task.is_finished());

        drop(up_queue_guard);
        tokio::time::timeout(std::time::Duration::from_secs(1), persist_task)
            .await
            .unwrap()
            .unwrap();
        let fork_docs = fork_instance
            .find_documents_by_id(&["a".to_string()], true)
            .await
            .unwrap();
        assert_eq!(fork_docs[0]["age"], json!(2));
        let checkpoint = get_last_checkpoint_doc(&state, RxStorageReplicationDirection::Down)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(checkpoint["sequence"], json!(3));
    }

    #[tokio::test]
    async fn downstream_master_stream_waits_until_upstream_inactive() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-active-up".to_string(),
                    collection_name: "docs".to_string(),
                    schema: schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let meta_schema = get_rx_replication_meta_instance_schema(&schema, false).unwrap();
        let meta_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-active-up".to_string(),
                    collection_name: "meta".to_string(),
                    schema: meta_schema,
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let master_stream = RxSubject::new();
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: Arc::clone(&fork_instance),
            meta_instance,
            hash_function: Arc::new(TestHashFunction),
            conflict_handler: Arc::new(DefaultConflictHandler),
            replication_handler: Arc::new(StreamReplicationHandler {
                stream: master_stream.clone(),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: Arc::new(input),
            checkpoint_key: "checkpoint".to_string(),
            downstream_bulk_write_flag: "downstream".to_string(),
            last_checkpoint_doc: parking_lot::Mutex::new(HashMap::new()),
            events: ReplicationEvents::new(),
            stats: ReplicationStats::new(),
            first_sync_done: FirstSyncDone::default(),
            stream_queue: StreamQueue::default(),
            checkpoint_queue: tokio::sync::Mutex::new(()),
            has_attachments: false,
        });
        state.events.active.up.next(true);
        let ongoing = spawn_ongoing_downstream(Arc::clone(&state));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        master_stream.next(RxReplicationMasterChange::Documents(
            DocumentsWithCheckpoint {
                documents: vec![json!({
                    "id": "a",
                    "age": 3,
                    "_deleted": false
                })],
                checkpoint: json!({ "sequence": 1 }),
            },
        ));

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(fork_instance
            .find_documents_by_id(&["a".to_string()], true)
            .await
            .unwrap()
            .is_empty());

        state.events.active.up.next(false);
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                let docs = fork_instance
                    .find_documents_by_id(&["a".to_string()], true)
                    .await
                    .unwrap();
                if !docs.is_empty() {
                    assert_eq!(docs[0]["age"], json!(3));
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        ongoing.abort();
    }

    #[tokio::test]
    async fn ongoing_downstream_batches_buffered_master_events() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-buffered-batch".to_string(),
                    collection_name: "docs".to_string(),
                    schema: schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let meta_schema = get_rx_replication_meta_instance_schema(&schema, false).unwrap();
        let meta_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-buffered-batch".to_string(),
                    collection_name: "meta".to_string(),
                    schema: meta_schema,
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let master_stream = RxSubject::new();
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: Arc::clone(&fork_instance),
            meta_instance,
            hash_function: Arc::new(TestHashFunction),
            conflict_handler: Arc::new(DefaultConflictHandler),
            replication_handler: Arc::new(StreamReplicationHandler {
                stream: master_stream.clone(),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: Arc::new(input),
            checkpoint_key: "checkpoint".to_string(),
            downstream_bulk_write_flag: "downstream".to_string(),
            last_checkpoint_doc: parking_lot::Mutex::new(HashMap::new()),
            events: ReplicationEvents::new(),
            stats: ReplicationStats::new(),
            first_sync_done: FirstSyncDone::default(),
            stream_queue: StreamQueue::default(),
            checkpoint_queue: tokio::sync::Mutex::new(()),
            has_attachments: false,
        });
        state.events.active.up.next(true);
        let ongoing = spawn_ongoing_downstream(Arc::clone(&state));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        master_stream.next(RxReplicationMasterChange::Documents(
            DocumentsWithCheckpoint {
                documents: vec![json!({
                    "id": "a",
                    "age": 3,
                    "_deleted": false
                })],
                checkpoint: json!({ "sequence": 1 }),
            },
        ));
        master_stream.next(RxReplicationMasterChange::Documents(
            DocumentsWithCheckpoint {
                documents: vec![json!({
                    "id": "b",
                    "age": 4,
                    "_deleted": false
                })],
                checkpoint: json!({ "sequence": 2 }),
            },
        ));

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(fork_instance
            .find_documents_by_id(&["a".to_string(), "b".to_string()], true)
            .await
            .unwrap()
            .is_empty());

        state.events.active.up.next(false);
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                let docs = fork_instance
                    .find_documents_by_id(&["a".to_string(), "b".to_string()], true)
                    .await
                    .unwrap();
                if docs.len() == 2 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        let stats = state.stats.down.lock();
        assert_eq!(stats.master_change_stream_emit, 2);
        assert_eq!(stats.persist_from_master, 1);
        drop(stats);
        let checkpoint = get_last_checkpoint_doc(&state, RxStorageReplicationDirection::Down)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(checkpoint["sequence"], json!(2));
        ongoing.abort();
    }

    #[tokio::test]
    async fn downstream_master_stream_skips_events_covered_by_resync_cutoff() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-cutoff".to_string(),
                    collection_name: "docs".to_string(),
                    schema: schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let meta_schema = get_rx_replication_meta_instance_schema(&schema, false).unwrap();
        let meta_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-cutoff".to_string(),
                    collection_name: "meta".to_string(),
                    schema: meta_schema,
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let master_stream = RxSubject::new();
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: Arc::clone(&fork_instance),
            meta_instance,
            hash_function: Arc::new(TestHashFunction),
            conflict_handler: Arc::new(DefaultConflictHandler),
            replication_handler: Arc::new(StreamReplicationHandler {
                stream: master_stream.clone(),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: Arc::new(input),
            checkpoint_key: "checkpoint".to_string(),
            downstream_bulk_write_flag: "downstream".to_string(),
            last_checkpoint_doc: parking_lot::Mutex::new(HashMap::new()),
            events: ReplicationEvents::new(),
            stats: ReplicationStats::new(),
            first_sync_done: FirstSyncDone::default(),
            stream_queue: StreamQueue::default(),
            checkpoint_queue: tokio::sync::Mutex::new(()),
            has_attachments: false,
        });
        let timer = Arc::new(AtomicI64::new(0));
        let last_time_master_changes_requested = Arc::new(AtomicI64::new(1));
        let ongoing = spawn_ongoing_downstream_with_timing(
            Arc::clone(&state),
            Arc::clone(&timer),
            last_time_master_changes_requested,
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        master_stream.next(RxReplicationMasterChange::Documents(
            DocumentsWithCheckpoint {
                documents: vec![json!({
                    "id": "a",
                    "age": 3,
                    "_deleted": false
                })],
                checkpoint: json!({ "sequence": 1 }),
            },
        ));

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while timer.load(AtomicOrdering::SeqCst) == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        ongoing.abort();

        assert!(fork_instance
            .find_documents_by_id(&["a".to_string()], true)
            .await
            .unwrap()
            .is_empty());
        assert!(
            get_last_checkpoint_doc(&state, RxStorageReplicationDirection::Down)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn master_change_stream_resync_triggers_downstream_resync() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-resync".to_string(),
                    collection_name: "docs".to_string(),
                    schema: schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let meta_schema = get_rx_replication_meta_instance_schema(&schema, false).unwrap();
        let meta_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-resync".to_string(),
                    collection_name: "meta".to_string(),
                    schema: meta_schema,
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let master_stream = RxSubject::new();
        let master_changes_since_calls = Arc::new(AtomicUsize::new(0));
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: Arc::clone(&fork_instance),
            meta_instance,
            hash_function: Arc::new(TestHashFunction),
            conflict_handler: Arc::new(DefaultConflictHandler),
            replication_handler: Arc::new(ResyncPullReplicationHandler {
                stream: master_stream.clone(),
                master_changes_since_calls: Arc::clone(&master_changes_since_calls),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: Arc::new(input),
            checkpoint_key: "checkpoint".to_string(),
            downstream_bulk_write_flag: "downstream".to_string(),
            last_checkpoint_doc: parking_lot::Mutex::new(HashMap::new()),
            events: ReplicationEvents::new(),
            stats: ReplicationStats::new(),
            first_sync_done: FirstSyncDone::default(),
            stream_queue: StreamQueue::default(),
            checkpoint_queue: tokio::sync::Mutex::new(()),
            has_attachments: false,
        });
        state.events.active.up.next(false);
        let ongoing = spawn_ongoing_downstream(Arc::clone(&state));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        master_stream.next(RxReplicationMasterChange::Resync);

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                let docs = fork_instance
                    .find_documents_by_id(&["a".to_string()], true)
                    .await
                    .unwrap();
                if docs.first().and_then(|doc| doc.get("age")) == Some(&json!(7)) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(master_changes_since_calls.load(Ordering::SeqCst) > 0);
        ongoing.abort();
    }
}
