//! Port of `src/replication-protocol/upstream.ts`.
//!
//! Functional upstream replication protocol port for CTOX core.
//! - Initial-checkpoint write plus conflict-aware initial sync with
//!   assumed-master tracking, conflict resolution, meta-doc writes, and repeat
//!   sync when conflict writes create more fork changes.
//! - Ongoing changeStream subscription on the fork: every post-initial-sync
//!   change-event-bulk is run through `persist_to_master`.
//! - Loop avoidance: events with `context == state.downstream_bulk_write_flag`
//!   advance the upstream checkpoint without re-pushing pulled documents.
//! - Per-batch `stream_queue.up` locking plus initial-sync time cutoffs so
//!   already-covered changeStream events cannot roll checkpoints back.
//! - `wait_before_persist` throttle placement, buffered change-event batching,
//!   strict `push_batch_size` master-write chunking, attachment-stripped meta
//!   writes, and cancel handling. The buffered drain plus `stream_queue.up`
//!   mutex is the Rust equivalent of RxDB's `openTasks` Promise-chain batching.
//! - `masterChangeStream$` `"RESYNC"` trigger, via the typed
//!   `RxReplicationMasterChange` enum.

use std::sync::atomic::{AtomicI64, Ordering as AtomicOrdering};
use std::sync::Arc;

use futures::FutureExt;
use serde_json::Value;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

use crate::replication_protocol::checkpoint::{get_last_checkpoint_doc, set_checkpoint};
use crate::replication_protocol::conflicts::resolve_conflict_error;
use crate::replication_protocol::helper::{
    remote_revision_height_marker_matches, strip_attachments_data_from_meta_write_rows,
};
use crate::rx_storage_helper::{get_written_documents_from_bulk_write_response, stack_checkpoints};
use crate::types::{
    BulkWriteRow, EventBulk, RxConflictHandlerInput, RxReplicationMasterChange,
    RxReplicationWriteToMasterRow, RxStorageInstanceReplicationState,
    RxStorageReplicationDirection,
};

// ref: rxdb/src/replication-protocol/upstream.ts:54-end
pub async fn start_replication_upstream(state: Arc<RxStorageInstanceReplicationState>) {
    // 1. Initial-checkpoint write (single op, no lock needed).
    if let Some(initial) = state.input.initial_checkpoint.as_ref() {
        if let Some(cp) = initial.upstream.as_ref() {
            match get_last_checkpoint_doc(&state, RxStorageReplicationDirection::Up).await {
                Ok(None) => {
                    if let Err(e) =
                        set_checkpoint(&state, RxStorageReplicationDirection::Up, cp.clone()).await
                    {
                        tracing::error!(
                            target: "ctox_rxdb::replication_protocol::upstream",
                            "initial checkpoint write failed: {e}",
                        );
                        return;
                    }
                }
                Ok(Some(_)) => {}
                Err(e) => {
                    tracing::error!(
                        target: "ctox_rxdb::replication_protocol::upstream",
                        "get_last_checkpoint_doc failed: {e}",
                    );
                    return;
                }
            }
        }
    }

    // 2. Spawn ongoing fork.changeStream subscription EARLY so events that
    //    arrive during the initial sync don't get lost.
    let timer = Arc::new(AtomicI64::new(0));
    let initial_sync_start_time = Arc::new(AtomicI64::new(-1));
    let ongoing = spawn_ongoing_upstream_with_timing(
        Arc::clone(&state),
        Arc::clone(&timer),
        Arc::clone(&initial_sync_start_time),
    );
    let resync_listener = spawn_upstream_resync_listener_with_timing(
        Arc::clone(&state),
        Arc::clone(&timer),
        Arc::clone(&initial_sync_start_time),
    );

    // 3. Initial sync (per-batch lock).
    if let Err(e) = upstream_initial_sync_until_no_conflicts_with_timing(
        &state,
        &timer,
        &initial_sync_start_time,
    )
    .await
    {
        tracing::error!(
            target: "ctox_rxdb::replication_protocol::upstream",
            "upstreamInitialSync failed: {e}",
        );
    }
    if !state.first_sync_done.up.get_value() && !state.events.canceled.get_value() {
        state.first_sync_done.up.next(true);
    }

    // 4. Stay alive until canceled, then drop the subscription.
    wait_for_cancel(&state).await;
    ongoing.abort();
    resync_listener.abort();
}

#[cfg(test)]
async fn upstream_initial_sync_until_no_conflicts(
    state: &Arc<RxStorageInstanceReplicationState>,
) -> Result<(), crate::rx_error::RxError> {
    let timer = AtomicI64::new(0);
    let initial_sync_start_time = AtomicI64::new(-1);
    upstream_initial_sync_until_no_conflicts_with_timing(state, &timer, &initial_sync_start_time)
        .await
}

async fn upstream_initial_sync_until_no_conflicts_with_timing(
    state: &Arc<RxStorageInstanceReplicationState>,
    timer: &AtomicI64,
    initial_sync_start_time: &AtomicI64,
) -> Result<(), crate::rx_error::RxError> {
    loop {
        if !upstream_initial_sync_with_timing(state, timer, initial_sync_start_time).await? {
            return Ok(());
        }
    }
}

/// Run the initial paginated push of fork→master.
/// Acquires `state.stream_queue.up` once per batch so ongoing events can
/// interleave between batches.
async fn upstream_initial_sync_with_timing(
    state: &Arc<RxStorageInstanceReplicationState>,
    timer: &AtomicI64,
    initial_sync_start_time: &AtomicI64,
) -> Result<bool, crate::rx_error::RxError> {
    {
        let mut stats = state.stats.up.lock();
        stats.upstream_initial_sync += 1;
    }
    if state.events.canceled.get_value() {
        return Ok(false);
    }
    let last_checkpoint_doc =
        get_last_checkpoint_doc(state, RxStorageReplicationDirection::Up).await?;
    let mut last_checkpoint: Value = last_checkpoint_doc.unwrap_or(Value::Null);
    let push_batch_size = state.input.push_batch_size;
    let mut had_conflict_writes = false;

    while !state.events.canceled.get_value() {
        let _g = state.stream_queue.up.lock().await;
        let fetch_time = timer.fetch_add(1, AtomicOrdering::SeqCst);
        initial_sync_start_time.store(fetch_time, AtomicOrdering::SeqCst);
        let up_result = state
            .input
            .fork_instance
            .get_changed_documents_since(
                push_batch_size,
                if last_checkpoint.is_null() {
                    None
                } else {
                    Some(&last_checkpoint)
                },
            )
            .await?;
        if up_result.documents.is_empty() {
            break;
        }
        last_checkpoint = stack_checkpoints(&[
            if last_checkpoint.is_null() {
                None
            } else {
                Some(last_checkpoint.clone())
            },
            Some(up_result.checkpoint.clone()),
        ]);
        if persist_to_master(state, up_result.documents.clone(), last_checkpoint.clone()).await? {
            had_conflict_writes = true;
        }
        let small = (up_result.documents.len() as u64) < push_batch_size;
        drop(_g);
        if small {
            break;
        }
    }
    Ok(had_conflict_writes)
}

/// Spawn the long-lived task that consumes `fork.change_stream()` and pushes
/// each event-bulk to master.
#[cfg(test)]
fn spawn_ongoing_upstream(state: Arc<RxStorageInstanceReplicationState>) -> JoinHandle<()> {
    spawn_ongoing_upstream_with_timing(
        state,
        Arc::new(AtomicI64::new(0)),
        Arc::new(AtomicI64::new(-1)),
    )
}

fn spawn_ongoing_upstream_with_timing(
    state: Arc<RxStorageInstanceReplicationState>,
    timer: Arc<AtomicI64>,
    initial_sync_start_time: Arc<AtomicI64>,
) -> JoinHandle<()> {
    let downstream_flag = state.downstream_bulk_write_flag.clone();
    tokio::spawn(async move {
        let mut stream = state.input.fork_instance.change_stream();
        while let Some(event_bulk) = stream.next().await {
            if state.events.canceled.get_value() {
                break;
            }
            if state.events.paused.get_value() {
                continue;
            }
            let task_time = timer.fetch_add(1, AtomicOrdering::SeqCst);
            if let Some(wait_before_persist) = state.input.wait_before_persist.as_ref() {
                wait_before_persist().await;
            }
            let mut lagged_resync = event_bulk.is_rxsubject_lagged();
            let mut tasks = vec![(task_time, event_bulk)];
            while let Some(Some(next_event_bulk)) = stream.next().now_or_never() {
                if state.events.canceled.get_value() {
                    break;
                }
                if state.events.paused.get_value() {
                    continue;
                }
                if next_event_bulk.is_rxsubject_lagged() {
                    lagged_resync = true;
                    continue;
                }
                let task_time = timer.fetch_add(1, AtomicOrdering::SeqCst);
                tasks.push((task_time, next_event_bulk));
            }
            if lagged_resync {
                state.events.active.up.next(true);
                if let Err(e) = upstream_initial_sync_until_no_conflicts_with_timing(
                    &state,
                    &timer,
                    &initial_sync_start_time,
                )
                .await
                {
                    tracing::error!(
                        target: "ctox_rxdb::replication_protocol::upstream",
                        "lagged fork change stream RESYNC upstreamInitialSync failed: {e}",
                    );
                }
                state.events.active.up.next(false);
                continue;
            }
            {
                let mut stats = state.stats.up.lock();
                stats.fork_change_stream_emit += 1;
            }
            if let Err(e) = process_upstream_event_tasks(
                &state,
                &downstream_flag,
                &initial_sync_start_time,
                tasks,
            )
            .await
            {
                tracing::error!(
                    target: "ctox_rxdb::replication_protocol::upstream",
                    "ongoing persist_to_master failed: {e}",
                );
            }
        }
    })
}

async fn process_upstream_event_tasks(
    state: &Arc<RxStorageInstanceReplicationState>,
    downstream_flag: &str,
    initial_sync_start_time: &AtomicI64,
    tasks: Vec<(i64, EventBulk)>,
) -> Result<(), crate::rx_error::RxError> {
    let mut docs: Vec<Value> = Vec::new();
    let mut checkpoint: Option<Value> = None;
    for (task_time, event_bulk) in tasks {
        if task_time < initial_sync_start_time.load(AtomicOrdering::SeqCst) {
            continue;
        }
        if event_bulk.context.as_deref() != Some(downstream_flag) {
            docs.extend(
                event_bulk
                    .events
                    .iter()
                    .filter_map(|ev| ev.document_data.clone()),
            );
        }
        checkpoint = Some(stack_checkpoints(&[
            checkpoint,
            event_bulk.checkpoint.clone(),
        ]));
    }
    let Some(checkpoint) = checkpoint else {
        return Ok(());
    };
    let _g = state.stream_queue.up.lock().await;
    state.events.active.up.next(true);
    let result = persist_to_master(state, docs, checkpoint).await;
    state.events.active.up.next(false);
    result.map(|_| ())
}

fn spawn_upstream_resync_listener_with_timing(
    state: Arc<RxStorageInstanceReplicationState>,
    timer: Arc<AtomicI64>,
    initial_sync_start_time: Arc<AtomicI64>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut master_stream = state.input.replication_handler.master_change_stream();
        while let Some(master_change) = master_stream.next().await {
            if state.events.canceled.get_value() {
                break;
            }
            let RxReplicationMasterChange::Resync = master_change else {
                continue;
            };
            let task_time = timer.fetch_add(1, AtomicOrdering::SeqCst);
            if task_time < initial_sync_start_time.load(AtomicOrdering::SeqCst) {
                continue;
            }
            state.events.active.up.next(true);
            if let Err(e) = upstream_initial_sync_until_no_conflicts_with_timing(
                &state,
                &timer,
                &initial_sync_start_time,
            )
            .await
            {
                tracing::error!(
                    target: "ctox_rxdb::replication_protocol::upstream",
                    "RESYNC upstreamInitialSync failed: {e}",
                );
            }
            state.events.active.up.next(false);
        }
    })
}

async fn wait_for_cancel(state: &Arc<RxStorageInstanceReplicationState>) {
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

// ref: rxdb/src/replication-protocol/upstream.ts:274-end (persistToMaster, conflict-aware)
async fn persist_to_master(
    state: &Arc<RxStorageInstanceReplicationState>,
    docs: Vec<Value>,
    new_up_checkpoint: Value,
) -> Result<bool, crate::rx_error::RxError> {
    {
        let mut stats = state.stats.up.lock();
        stats.persist_to_master += 1;
    }
    if docs.is_empty() {
        set_checkpoint(state, RxStorageReplicationDirection::Up, new_up_checkpoint).await?;
        return Ok(false);
    }

    use crate::replication_protocol::helper::write_doc_to_doc_state;
    use crate::replication_protocol::meta_instance::{
        get_assumed_master_state, get_meta_write_row,
    };
    let has_attachments = state.has_attachments;
    let keep_meta = state.input.keep_meta;
    let primary_path = state.primary_path.clone();

    let mut up_docs_by_id: std::collections::HashMap<String, Value> =
        std::collections::HashMap::new();
    for d in docs.into_iter() {
        if let Some(id) = d.get(&primary_path).and_then(|v| v.as_str()) {
            up_docs_by_id.insert(id.to_string(), d);
        }
    }
    let doc_ids: Vec<String> = up_docs_by_id.keys().cloned().collect();
    let assumed_master_state = get_assumed_master_state(state, &doc_ids).await?;

    let mut rows: Vec<RxReplicationWriteToMasterRow> = Vec::new();
    for (doc_id, full_doc) in up_docs_by_id.iter() {
        let doc_clean = write_doc_to_doc_state(full_doc, has_attachments, keep_meta);
        let assumed = assumed_master_state.get(doc_id);
        let skip = if let Some(asm) = assumed {
            let is_resolved_conflict_current = asm
                .meta_document
                .get("isResolvedConflict")
                .and_then(Value::as_str)
                == full_doc.get("_rev").and_then(Value::as_str);
            let already_equal = state
                .input
                .conflict_handler
                .is_equal(&asm.doc_data, &doc_clean, "upstream-check-if-equal")
                .await;
            let revision_height_already_marked = asm.doc_data.get("_rev").is_some()
                && remote_revision_height_marker_matches(full_doc, &state.input.identifier);
            (already_equal && !is_resolved_conflict_current) || revision_height_already_marked
        } else {
            false
        };
        if skip {
            continue;
        }
        rows.push(RxReplicationWriteToMasterRow {
            new_document_state: doc_clean,
            assumed_master_state: assumed.map(|a| a.doc_data.clone()),
        });
    }

    if rows.is_empty() {
        set_checkpoint(state, RxStorageReplicationDirection::Up, new_up_checkpoint).await?;
        return Ok(false);
    }

    let push_batch_size = usize::try_from(state.input.push_batch_size)
        .ok()
        .filter(|size| *size > 0)
        .unwrap_or(rows.len().max(1));
    let mut conflicts = Vec::new();
    for batch in rows.chunks(push_batch_size) {
        let mut batch_conflicts = state
            .input
            .replication_handler
            .master_write(batch.to_vec())
            .await?;
        conflicts.append(&mut batch_conflicts);
    }
    let conflict_ids: std::collections::HashSet<String> = conflicts
        .iter()
        .filter_map(|c| {
            c.get(&primary_path)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    let mut meta_write_rows: Vec<crate::types::BulkWriteRow> = Vec::new();
    for row in rows.iter() {
        let doc_id = row
            .new_document_state
            .get(&primary_path)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if conflict_ids.contains(&doc_id) {
            continue;
        }
        let previous_meta = assumed_master_state
            .get(&doc_id)
            .map(|asm| asm.meta_document.clone());
        let meta_row =
            get_meta_write_row(state, &row.new_document_state, previous_meta.as_ref(), None)
                .await?;
        meta_write_rows.push(meta_row);
    }
    if !meta_write_rows.is_empty() {
        let _ = state
            .input
            .meta_instance
            .bulk_write(
                strip_attachments_data_from_meta_write_rows(state, &meta_write_rows),
                "replication-meta-write",
            )
            .await?;
    }
    for row in rows.iter() {
        let doc_id = row
            .new_document_state
            .get(&primary_path)
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if !conflict_ids.contains(doc_id) {
            state
                .events
                .processed
                .up
                .next(serde_json::to_value(row).unwrap_or(serde_json::Value::Null));
        }
    }

    let mut had_conflict_writes = false;
    if !conflicts.is_empty() {
        {
            let mut stats = state.stats.up.lock();
            stats.persist_to_master_had_conflicts += 1;
        }
        let conflicts_by_id: std::collections::HashMap<String, Value> = conflicts
            .iter()
            .filter_map(|conflict| {
                conflict
                    .get(&primary_path)
                    .and_then(|value| value.as_str())
                    .map(|id| (id.to_string(), conflict.clone()))
            })
            .collect();
        let rows_by_id: std::collections::HashMap<String, RxReplicationWriteToMasterRow> = rows
            .iter()
            .filter_map(|row| {
                row.new_document_state
                    .get(&primary_path)
                    .and_then(|value| value.as_str())
                    .map(|id| (id.to_string(), row.clone()))
            })
            .collect();
        let mut conflict_write_fork: Vec<BulkWriteRow> = Vec::new();
        let mut conflict_meta_by_id: std::collections::HashMap<String, BulkWriteRow> =
            std::collections::HashMap::new();

        for (doc_id, real_master_state) in conflicts_by_id.iter() {
            let Some(write_to_master_row) = rows_by_id.get(doc_id) else {
                continue;
            };
            let Some(fork_state) = up_docs_by_id.get(doc_id) else {
                continue;
            };
            let input = RxConflictHandlerInput {
                real_master_state: real_master_state.clone(),
                assumed_master_state: write_to_master_row.assumed_master_state.clone(),
                new_document_state: write_to_master_row.new_document_state.clone(),
            };
            let resolved = resolve_conflict_error(state, &input, fork_state).await?;
            if let Some(resolved_doc) = resolved {
                state.events.resolved_conflicts.next(serde_json::json!({
                    "input": input,
                    "output": resolved_doc,
                }));
                let resolved_rev = resolved_doc
                    .get("_rev")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string);
                conflict_write_fork.push(BulkWriteRow {
                    previous: Some(fork_state.clone()),
                    document: resolved_doc,
                });
                let previous_meta = assumed_master_state
                    .get(doc_id)
                    .map(|assumed| assumed.meta_document.clone());
                let meta_row = get_meta_write_row(
                    state,
                    real_master_state,
                    previous_meta.as_ref(),
                    resolved_rev.as_deref(),
                )
                .await?;
                conflict_meta_by_id.insert(doc_id.clone(), meta_row);
            }
        }

        if !conflict_write_fork.is_empty() {
            had_conflict_writes = true;
            {
                let mut stats = state.stats.up.lock();
                stats.persist_to_master_conflict_writes += 1;
            }
            let fork_write_result = state
                .input
                .fork_instance
                .bulk_write(conflict_write_fork.clone(), "replication-up-write-conflict")
                .await?;
            for error in fork_write_result.error.iter() {
                if error.status != 409 {
                    return Err(crate::rx_error::new_rx_error(
                        "RC_PUSH",
                        Some(serde_json::json!({ "writeError": error })),
                    ));
                }
            }
            let successful_conflict_docs = get_written_documents_from_bulk_write_response(
                &primary_path,
                &conflict_write_fork,
                &fork_write_result,
                None,
            );
            let mut meta_rows = Vec::new();
            for doc in successful_conflict_docs.iter() {
                if let Some(doc_id) = doc.get(&primary_path).and_then(|value| value.as_str()) {
                    if let Some(row) = conflict_meta_by_id.get(doc_id).cloned() {
                        meta_rows.push(row);
                    }
                }
            }
            if !meta_rows.is_empty() {
                let _ = state
                    .input
                    .meta_instance
                    .bulk_write(
                        strip_attachments_data_from_meta_write_rows(state, &meta_rows),
                        "replication-up-write-conflict-meta",
                    )
                    .await?;
            }
        }
    }

    set_checkpoint(state, RxStorageReplicationDirection::Up, new_up_checkpoint).await?;
    Ok(had_conflict_writes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use serde_json::json;

    use crate::plugins::storage_memory::get_rx_storage_memory;
    use crate::replication_protocol::default_conflict_handler::DefaultConflictHandler;
    use crate::replication_protocol::meta_instance::{
        get_meta_write_row, get_rx_replication_meta_instance_schema,
    };
    use crate::rx_schema_helper::fill_with_default_settings;
    use crate::rxjs_compat::{RxStream, RxSubject};
    use crate::types::{
        DocumentsWithCheckpoint, FirstSyncDone, HashFunction, HashOutput, JsonSchema, PrimaryKey,
        ReplicationEvents, ReplicationStats, RxJsonSchema, RxJsonSchemaAttachments,
        RxReplicationHandler, RxReplicationMasterChange, RxStorageInstance,
        RxStorageInstanceCreationParams, RxStorageInstanceReplicationInput,
        RxStorageInstanceReplicationState, StreamQueue,
    };

    struct TestHashFunction;

    impl HashFunction for TestHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("hash:{input}") })
        }
    }

    struct ConflictOnceHandler;

    #[async_trait]
    impl RxReplicationHandler for ConflictOnceHandler {
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
            rows: Vec<RxReplicationWriteToMasterRow>,
        ) -> Result<Vec<Value>, crate::rx_error::RxError> {
            assert_eq!(rows.len(), 1);
            Ok(vec![json!({
                "id": "a",
                "age": 2,
                "_deleted": false,
            })])
        }
    }

    struct CountingHandler {
        master_write_calls: std::sync::Arc<AtomicUsize>,
    }

    #[async_trait]
    impl RxReplicationHandler for CountingHandler {
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
            _rows: Vec<RxReplicationWriteToMasterRow>,
        ) -> Result<Vec<Value>, crate::rx_error::RxError> {
            self.master_write_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        }
    }

    struct BatchCountingHandler {
        master_write_calls: std::sync::Arc<AtomicUsize>,
        batch_sizes: std::sync::Arc<parking_lot::Mutex<Vec<usize>>>,
    }

    #[async_trait]
    impl RxReplicationHandler for BatchCountingHandler {
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
            rows: Vec<RxReplicationWriteToMasterRow>,
        ) -> Result<Vec<Value>, crate::rx_error::RxError> {
            self.master_write_calls.fetch_add(1, Ordering::SeqCst);
            self.batch_sizes.lock().push(rows.len());
            Ok(Vec::new())
        }
    }

    struct ResyncCountingHandler {
        stream: RxSubject<RxReplicationMasterChange>,
        master_write_calls: std::sync::Arc<AtomicUsize>,
    }

    #[async_trait]
    impl RxReplicationHandler for ResyncCountingHandler {
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
            _rows: Vec<RxReplicationWriteToMasterRow>,
        ) -> Result<Vec<Value>, crate::rx_error::RxError> {
            self.master_write_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        }
    }

    struct ConflictThenAcceptHandler {
        master_write_calls: std::sync::Arc<AtomicUsize>,
        seen_ages: std::sync::Arc<parking_lot::Mutex<Vec<i64>>>,
    }

    #[async_trait]
    impl RxReplicationHandler for ConflictThenAcceptHandler {
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
            rows: Vec<RxReplicationWriteToMasterRow>,
        ) -> Result<Vec<Value>, crate::rx_error::RxError> {
            assert_eq!(rows.len(), 1);
            let age = rows[0]
                .new_document_state
                .get("age")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            self.seen_ages.lock().push(age);
            let call = self.master_write_calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                Ok(vec![json!({
                    "id": "a",
                    "age": 2,
                    "_deleted": false,
                })])
            } else {
                Ok(Vec::new())
            }
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

    fn fork_doc(age: i64) -> Value {
        fork_doc_with_id("a", age, 100.0)
    }

    fn fork_doc_with_id(id: &str, age: i64, lwt: f64) -> Value {
        json!({
            "id": id,
            "age": age,
            "_deleted": false,
            "_attachments": {},
            "_rev": "1-local",
            "_meta": { "lwt": lwt },
        })
    }

    fn fork_doc_with_attachment() -> Value {
        json!({
            "id": "a",
            "age": 1,
            "_deleted": false,
            "_attachments": {
                "avatar": {
                    "data": "aGVsbG8=",
                    "digest": "sha256:avatar",
                    "length": 5,
                    "type": "text/plain"
                }
            },
            "_rev": "1-local",
            "_meta": { "lwt": 1.0 },
        })
    }

    #[tokio::test]
    async fn persist_to_master_resolves_master_conflicts_back_to_fork() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db".to_string(),
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
                    document: fork_doc(1),
                }],
                "seed",
            )
            .await
            .unwrap();
        let meta_schema = get_rx_replication_meta_instance_schema(&schema, false).unwrap();
        let meta_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db".to_string(),
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
            fork_instance: fork_instance.clone(),
            meta_instance,
            hash_function: std::sync::Arc::new(TestHashFunction),
            conflict_handler: std::sync::Arc::new(DefaultConflictHandler),
            replication_handler: std::sync::Arc::new(ConflictOnceHandler),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = std::sync::Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: std::sync::Arc::new(input),
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

        persist_to_master(&state, vec![fork_doc(1)], json!({ "sequence": 1 }))
            .await
            .unwrap();

        let docs = fork_instance
            .find_documents_by_id(&["a".to_string()], true)
            .await
            .unwrap();
        assert_eq!(docs[0]["age"], json!(2));
        assert_eq!(state.stats.up.lock().persist_to_master_had_conflicts, 1);
        assert_eq!(state.stats.up.lock().persist_to_master_conflict_writes, 1);
    }

    #[tokio::test]
    async fn initial_upstream_sync_repeats_after_conflict_write() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-initial-conflict-retry".to_string(),
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
                    document: fork_doc(1),
                }],
                "seed",
            )
            .await
            .unwrap();
        let meta_schema = get_rx_replication_meta_instance_schema(&schema, false).unwrap();
        let meta_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-initial-conflict-retry".to_string(),
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
        let master_write_calls = std::sync::Arc::new(AtomicUsize::new(0));
        let seen_ages = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: fork_instance.clone(),
            meta_instance,
            hash_function: std::sync::Arc::new(TestHashFunction),
            conflict_handler: std::sync::Arc::new(DefaultConflictHandler),
            replication_handler: std::sync::Arc::new(ConflictThenAcceptHandler {
                master_write_calls: std::sync::Arc::clone(&master_write_calls),
                seen_ages: std::sync::Arc::clone(&seen_ages),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = std::sync::Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: std::sync::Arc::new(input),
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

        upstream_initial_sync_until_no_conflicts(&state)
            .await
            .unwrap();

        assert_eq!(master_write_calls.load(Ordering::SeqCst), 2);
        assert_eq!(*seen_ages.lock(), vec![1, 2]);
        assert_eq!(state.stats.up.lock().upstream_initial_sync, 2);
        let docs = fork_instance
            .find_documents_by_id(&["a".to_string()], true)
            .await
            .unwrap();
        assert_eq!(docs[0]["age"], json!(2));
    }

    #[tokio::test]
    async fn persist_to_master_skips_revision_height_marked_as_replicated() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-rev-height-skip".to_string(),
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
        let meta_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-rev-height-skip".to_string(),
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
        let master_write_calls = std::sync::Arc::new(AtomicUsize::new(0));
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance,
            meta_instance: meta_instance.clone(),
            hash_function: std::sync::Arc::new(TestHashFunction),
            conflict_handler: std::sync::Arc::new(DefaultConflictHandler),
            replication_handler: std::sync::Arc::new(CountingHandler {
                master_write_calls: std::sync::Arc::clone(&master_write_calls),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = std::sync::Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: std::sync::Arc::new(input),
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

        let fork_state_marked_by_remote_rev = json!({
            "id": "a",
            "age": 99,
            "_deleted": false,
            "_attachments": {},
            "_rev": "2-local",
            "_meta": {
                "lwt": 100.0,
                "replication-test": 2
            },
        });
        persist_to_master(
            &state,
            vec![fork_state_marked_by_remote_rev],
            json!({ "sequence": 2 }),
        )
        .await
        .unwrap();

        assert_eq!(master_write_calls.load(Ordering::SeqCst), 0);
        let checkpoint = get_last_checkpoint_doc(&state, RxStorageReplicationDirection::Up)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(checkpoint["sequence"], json!(2));
    }

    #[tokio::test]
    async fn downstream_originated_events_advance_upstream_checkpoint_without_push() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-checkpoint".to_string(),
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
        let meta_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-downstream-checkpoint".to_string(),
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
        let master_write_calls = std::sync::Arc::new(AtomicUsize::new(0));
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: fork_instance.clone(),
            meta_instance,
            hash_function: std::sync::Arc::new(TestHashFunction),
            conflict_handler: std::sync::Arc::new(DefaultConflictHandler),
            replication_handler: std::sync::Arc::new(CountingHandler {
                master_write_calls: std::sync::Arc::clone(&master_write_calls),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = std::sync::Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: std::sync::Arc::new(input),
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
        let ongoing = spawn_ongoing_upstream(std::sync::Arc::clone(&state));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        fork_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: fork_doc(1),
                }],
                "downstream",
            )
            .await
            .unwrap();

        let checkpoint = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if let Some(checkpoint) =
                    get_last_checkpoint_doc(&state, RxStorageReplicationDirection::Up)
                        .await
                        .unwrap()
                {
                    break checkpoint;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        ongoing.abort();

        assert_eq!(master_write_calls.load(Ordering::SeqCst), 0);
        assert_eq!(checkpoint["id"], json!("a"));
        assert_eq!(state.stats.up.lock().persist_to_master, 1);
    }

    #[tokio::test]
    async fn ongoing_upstream_skips_events_covered_by_initial_sync_cutoff() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-upstream-cutoff".to_string(),
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
        let meta_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-upstream-cutoff".to_string(),
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
        let master_write_calls = std::sync::Arc::new(AtomicUsize::new(0));
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: fork_instance.clone(),
            meta_instance,
            hash_function: std::sync::Arc::new(TestHashFunction),
            conflict_handler: std::sync::Arc::new(DefaultConflictHandler),
            replication_handler: std::sync::Arc::new(CountingHandler {
                master_write_calls: std::sync::Arc::clone(&master_write_calls),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = std::sync::Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: std::sync::Arc::new(input),
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
        let timer = std::sync::Arc::new(AtomicI64::new(0));
        let initial_sync_start_time = std::sync::Arc::new(AtomicI64::new(1));
        let ongoing = spawn_ongoing_upstream_with_timing(
            std::sync::Arc::clone(&state),
            std::sync::Arc::clone(&timer),
            initial_sync_start_time,
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        fork_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: fork_doc(1),
                }],
                "local-write-before-initial-fetch",
            )
            .await
            .unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while timer.load(AtomicOrdering::SeqCst) == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        ongoing.abort();

        assert_eq!(master_write_calls.load(Ordering::SeqCst), 0);
        assert!(
            get_last_checkpoint_doc(&state, RxStorageReplicationDirection::Up)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn persist_to_master_strips_attachment_data_from_meta_rows() {
        let storage = get_rx_storage_memory(());
        let mut schema = test_schema();
        schema.attachments = Some(RxJsonSchemaAttachments::default());
        let fork_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-attachment-meta".to_string(),
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
        let meta_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-attachment-meta".to_string(),
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
        let master_write_calls = std::sync::Arc::new(AtomicUsize::new(0));
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance,
            meta_instance,
            hash_function: std::sync::Arc::new(TestHashFunction),
            conflict_handler: std::sync::Arc::new(DefaultConflictHandler),
            replication_handler: std::sync::Arc::new(CountingHandler {
                master_write_calls: std::sync::Arc::clone(&master_write_calls),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = std::sync::Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: std::sync::Arc::new(input),
            checkpoint_key: "checkpoint".to_string(),
            downstream_bulk_write_flag: "downstream".to_string(),
            last_checkpoint_doc: parking_lot::Mutex::new(HashMap::new()),
            events: ReplicationEvents::new(),
            stats: ReplicationStats::new(),
            first_sync_done: FirstSyncDone::default(),
            stream_queue: StreamQueue::default(),
            checkpoint_queue: tokio::sync::Mutex::new(()),
            has_attachments: true,
        });

        persist_to_master(
            &state,
            vec![fork_doc_with_attachment()],
            json!({ "sequence": 1 }),
        )
        .await
        .unwrap();

        let assumed = crate::replication_protocol::meta_instance::get_assumed_master_state(
            &state,
            &["a".to_string()],
        )
        .await
        .unwrap();
        let attachment = &assumed["a"].doc_data["_attachments"]["avatar"];
        assert_eq!(master_write_calls.load(Ordering::SeqCst), 1);
        assert!(attachment.get("data").is_none());
        assert_eq!(attachment["digest"], json!("sha256:avatar"));
        assert_eq!(attachment["length"], json!(5));
        assert_eq!(attachment["type"], json!("text/plain"));
    }

    #[tokio::test]
    async fn ongoing_upstream_waits_before_persisting_change_events() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-wait-before-persist".to_string(),
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
        let meta_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-wait-before-persist".to_string(),
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
        let wait_started = std::sync::Arc::new(AtomicUsize::new(0));
        let wait_notify = std::sync::Arc::new(tokio::sync::Notify::new());
        let master_write_calls = std::sync::Arc::new(AtomicUsize::new(0));
        let wait_started_for_closure = std::sync::Arc::clone(&wait_started);
        let wait_notify_for_closure = std::sync::Arc::clone(&wait_notify);
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: fork_instance.clone(),
            meta_instance,
            hash_function: std::sync::Arc::new(TestHashFunction),
            conflict_handler: std::sync::Arc::new(DefaultConflictHandler),
            replication_handler: std::sync::Arc::new(CountingHandler {
                master_write_calls: std::sync::Arc::clone(&master_write_calls),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: Some(std::sync::Arc::new(move || {
                let wait_started = std::sync::Arc::clone(&wait_started_for_closure);
                let wait_notify = std::sync::Arc::clone(&wait_notify_for_closure);
                Box::pin(async move {
                    wait_started.fetch_add(1, Ordering::SeqCst);
                    wait_notify.notified().await;
                })
            })),
        };
        let state = std::sync::Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: std::sync::Arc::new(input),
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
        let ongoing = spawn_ongoing_upstream(std::sync::Arc::clone(&state));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        fork_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: fork_doc(1),
                }],
                "local-write",
            )
            .await
            .unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while wait_started.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(master_write_calls.load(Ordering::SeqCst), 0);

        wait_notify.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while master_write_calls.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        ongoing.abort();
    }

    #[tokio::test]
    async fn ongoing_upstream_batches_buffered_change_events() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-upstream-batching".to_string(),
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
        let meta_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-upstream-batching".to_string(),
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
        let wait_started = std::sync::Arc::new(AtomicUsize::new(0));
        let wait_notify = std::sync::Arc::new(tokio::sync::Notify::new());
        let master_write_calls = std::sync::Arc::new(AtomicUsize::new(0));
        let batch_sizes = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let wait_started_for_closure = std::sync::Arc::clone(&wait_started);
        let wait_notify_for_closure = std::sync::Arc::clone(&wait_notify);
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: fork_instance.clone(),
            meta_instance,
            hash_function: std::sync::Arc::new(TestHashFunction),
            conflict_handler: std::sync::Arc::new(DefaultConflictHandler),
            replication_handler: std::sync::Arc::new(BatchCountingHandler {
                master_write_calls: std::sync::Arc::clone(&master_write_calls),
                batch_sizes: std::sync::Arc::clone(&batch_sizes),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: Some(std::sync::Arc::new(move || {
                let wait_started = std::sync::Arc::clone(&wait_started_for_closure);
                let wait_notify = std::sync::Arc::clone(&wait_notify_for_closure);
                Box::pin(async move {
                    wait_started.fetch_add(1, Ordering::SeqCst);
                    wait_notify.notified().await;
                })
            })),
        };
        let state = std::sync::Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: std::sync::Arc::new(input),
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
        let ongoing = spawn_ongoing_upstream(std::sync::Arc::clone(&state));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        fork_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: fork_doc_with_id("a", 1, 100.0),
                }],
                "local-write-a",
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while wait_started.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        fork_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: fork_doc_with_id("b", 2, 101.0),
                }],
                "local-write-b",
            )
            .await
            .unwrap();

        wait_notify.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while master_write_calls.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        assert_eq!(master_write_calls.load(Ordering::SeqCst), 1);
        assert_eq!(*batch_sizes.lock(), vec![2]);
        ongoing.abort();
    }

    #[tokio::test]
    async fn master_change_stream_resync_triggers_upstream_initial_sync() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let fork_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-upstream-resync".to_string(),
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
        let meta_instance: std::sync::Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-upstream-resync".to_string(),
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
        fork_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: fork_doc(1),
                }],
                "local-write-before-resync",
            )
            .await
            .unwrap();

        let master_stream = RxSubject::new();
        let master_write_calls = std::sync::Arc::new(AtomicUsize::new(0));
        let input = RxStorageInstanceReplicationInput {
            identifier: "replication-test".to_string(),
            fork_instance: fork_instance.clone(),
            meta_instance,
            hash_function: std::sync::Arc::new(TestHashFunction),
            conflict_handler: std::sync::Arc::new(DefaultConflictHandler),
            replication_handler: std::sync::Arc::new(ResyncCountingHandler {
                stream: master_stream.clone(),
                master_write_calls: std::sync::Arc::clone(&master_write_calls),
            }),
            push_batch_size: 100,
            pull_batch_size: 100,
            bulk_size: 100,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };
        let state = std::sync::Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: std::sync::Arc::new(input),
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
        let resync_listener = spawn_upstream_resync_listener_with_timing(
            std::sync::Arc::clone(&state),
            std::sync::Arc::new(AtomicI64::new(0)),
            std::sync::Arc::new(AtomicI64::new(-1)),
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        master_stream.next(RxReplicationMasterChange::Resync);

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while master_write_calls.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        resync_listener.abort();
    }
}
