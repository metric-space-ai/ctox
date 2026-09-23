//! Port of `src/replication-protocol/index.ts`.
//!
//! Functional replication protocol port for CTOX core. The standalone control
//! helpers below are ported:
//! - `await_rx_storage_replication_first_in_sync`
//! - `await_rx_storage_replication_in_sync`
//! - `await_rx_storage_replication_idle`
//! - `cancel_rx_storage_replication`
//! - `replicate_rx_storage_instance`, including upstream/downstream task
//!   startup, first-sync subjects, checkpoint key creation, and queue state.
//! - `rx_storage_instance_to_replication_handler`, backed by the storage-native
//!   changed-documents API and conflict-aware master writes.
//!
//! Renamed to `index_mod.rs` per the Rust reserved-name avoidance pattern
//! used elsewhere in this port.

use std::sync::Arc;

use tokio_stream::StreamExt;

use crate::plugins::utils::utils_object_deep_equal::deep_equal;
#[cfg(test)]
#[path = "test_utils.rs"]
pub(crate) mod test_utils;

use crate::types::{
    FirstSyncDone, ReplicationEvents, ReplicationStats, RxStorageInstanceReplicationInput,
    RxStorageInstanceReplicationState, StreamQueue,
};

// ref: rxdb/src/replication-protocol/index.ts:119-132
/// Resolves once both initial syncs (down + up) have completed.
pub async fn await_rx_storage_replication_first_in_sync(
    state: Arc<RxStorageInstanceReplicationState>,
) {
    let mut down_stream = state.first_sync_done.down.subscribe();
    let mut up_stream = state.first_sync_done.up.subscribe();
    let mut down_done = state.first_sync_done.down.get_value();
    let mut up_done = state.first_sync_done.up.get_value();
    while !(down_done && up_done) {
        tokio::select! {
            Some(v) = down_stream.next() => { down_done = v; }
            Some(v) = up_stream.next() => { up_done = v; }
            else => break,
        }
    }
}

// ref: rxdb/src/replication-protocol/index.ts:134-142
/// Awaits the current head of each stream queue + the checkpoint queue.
/// Upstream uses `Promise.all([streamQueue.up, streamQueue.down, checkpointQueue])`.
pub async fn await_rx_storage_replication_in_sync(state: Arc<RxStorageInstanceReplicationState>) {
    let _down = state.stream_queue.down.lock().await;
    let _up = state.stream_queue.up.lock().await;
    let _cp = state.checkpoint_queue.lock().await;
}

#[derive(Clone, Copy)]
pub enum ReplicationIdleRequirement {
    /// Wait for the direction's retained activity signal and tracked queue.
    ActivityAndQueue,
    /// Wait only for the current queue head. Used where upstream RxDB explicitly
    /// awaits `streamQueue.up`, not direction inactivity.
    QueueOnly,
}

/// Waits for the requested quiescence level of one replication direction.
///
/// Activity is published before queue acquisition, and both the activity subject
/// and tracked queue retain their current state for late subscribers. The final
/// recheck closes the race where new work starts between the two awaits.
pub async fn await_rx_storage_replication_direction_idle(
    state: &RxStorageInstanceReplicationState,
    direction: crate::types::RxStorageReplicationDirection,
    requirement: ReplicationIdleRequirement,
) {
    let (active, queue) = match direction {
        crate::types::RxStorageReplicationDirection::Down => {
            (&state.events.active.down, &state.stream_queue.down)
        }
        crate::types::RxStorageReplicationDirection::Up => {
            (&state.events.active.up, &state.stream_queue.up)
        }
    };

    loop {
        if matches!(requirement, ReplicationIdleRequirement::ActivityAndQueue) && active.get_value()
        {
            let mut activity = active.subscribe();
            while let Some(is_active) = activity.next().await {
                if !is_active {
                    break;
                }
            }
        }
        queue.wait_until_idle().await;
        if queue.is_idle()
            && (matches!(requirement, ReplicationIdleRequirement::QueueOnly) || !active.get_value())
        {
            return;
        }
    }
}

// ref: rxdb/src/replication-protocol/index.ts:145-167
/// Canonical replication-quiescence primitive.
///
/// Initial sync must be complete, both activity subjects must report inactive,
/// both tracked stream queues must have no waiter or holder, and the checkpoint
/// queue must drain. The checkpoint lock is released before direction state is
/// rechecked, so this never waits for direction work while holding a lock that
/// direction work needs for `set_checkpoint()`.
pub async fn await_rx_storage_replication_idle(state: Arc<RxStorageInstanceReplicationState>) {
    await_rx_storage_replication_first_in_sync(Arc::clone(&state)).await;
    loop {
        await_rx_storage_replication_direction_idle(
            &state,
            crate::types::RxStorageReplicationDirection::Down,
            ReplicationIdleRequirement::ActivityAndQueue,
        )
        .await;
        await_rx_storage_replication_direction_idle(
            &state,
            crate::types::RxStorageReplicationDirection::Up,
            ReplicationIdleRequirement::ActivityAndQueue,
        )
        .await;

        {
            let _checkpoint = state.checkpoint_queue.lock().await;
        }

        // Work can enter a direction while checkpoint draining is blocked. Its
        // retained activity/queue state makes that visible without a fixed
        // second drain; loop only when the post-checkpoint snapshot changed.
        if !state.events.active.down.get_value()
            && !state.events.active.up.get_value()
            && state.stream_queue.down.is_idle()
            && state.stream_queue.up.is_idle()
        {
            return;
        }
    }
}

// ref: rxdb/src/replication-protocol/index.ts:321-332
/// Cancels a running replication. Idempotent.
pub async fn cancel_rx_storage_replication(state: Arc<RxStorageInstanceReplicationState>) {
    state.events.canceled.next(true);
    // Upstream calls .complete() on the per-direction Subjects to signal end-of-stream.
    // tokio broadcast/watch close on Sender drop; since we hold the Subjects
    // inside the state Arc, completion is implicit at state drop time.
    // We at least drain the checkpoint queue here.
    let _cp = state.checkpoint_queue.lock().await;
}

// ref: rxdb/src/replication-protocol/index.ts:58-117
/// Build the replication state and start the upstream + downstream halves.
///
/// Starts the conflict-aware upstream and downstream halves in background
/// tasks and returns the shared replication state used by the public helpers.
pub async fn replicate_rx_storage_instance(
    input: RxStorageInstanceReplicationInput,
) -> Arc<RxStorageInstanceReplicationState> {
    use crate::replication_protocol::checkpoint::get_checkpoint_key;
    use crate::replication_protocol::downstream::start_replication_downstream;
    use crate::replication_protocol::upstream::start_replication_upstream;
    use crate::rx_schema_helper::get_primary_field_of_primary_key;

    // Upstream calls `getUnderlyingPersistentStorage` to unwrap a chain of
    // wrapping storages. CTOX does not currently use that chain pattern
    // (no `WrappedRxStorageInstance` ported yet); the inputs are assumed to
    // already be the lowest-level instance.

    let primary_path = get_primary_field_of_primary_key(&input.fork_instance.schema().primary_key);
    let has_attachments = input
        .fork_instance
        .schema()
        .extra
        .contains_key("attachments");
    let checkpoint_key = get_checkpoint_key(&input).await;
    let downstream_bulk_write_flag = format!("replication-downstream-{checkpoint_key}");

    let state = Arc::new(RxStorageInstanceReplicationState {
        primary_path,
        input: Arc::new(input),
        checkpoint_key,
        downstream_bulk_write_flag,
        last_checkpoint_doc: parking_lot::Mutex::new(std::collections::HashMap::new()),
        events: ReplicationEvents::new(),
        stats: ReplicationStats::new(),
        first_sync_done: FirstSyncDone::default(),
        stream_queue: StreamQueue::default(),
        checkpoint_queue: tokio::sync::Mutex::new(()),
        has_attachments,
    });

    let state_for_down = Arc::clone(&state);
    tokio::spawn(async move {
        start_replication_downstream(state_for_down).await;
    });
    let state_for_up = Arc::clone(&state);
    tokio::spawn(async move {
        start_replication_upstream(state_for_up).await;
    });

    state
}

/// Host hook applied to every document a remote peer writes to this master,
/// before it is persisted: `(collection_name, new_document_state)`. CTOX uses
/// it to take secret values out of replicated command documents, which would
/// otherwise rest in the master store and replicate to every other peer until
/// the host overwrote them (CTOX-Feldbefund 11.09.2026). Not part of upstream
/// RxDB.
pub type MasterWriteSanitizer = Arc<dyn Fn(&str, &mut serde_json::Value) + Send + Sync>;

static MASTER_WRITE_SANITIZER: std::sync::OnceLock<MasterWriteSanitizer> =
    std::sync::OnceLock::new();

/// Install the process-wide master-write sanitizer. The first installation
/// wins; later calls return `false`.
pub fn set_master_write_sanitizer(sanitizer: MasterWriteSanitizer) -> bool {
    MASTER_WRITE_SANITIZER.set(sanitizer).is_ok()
}

// ref: rxdb/src/replication-protocol/index.ts:170-318
/// Adapt a storage instance + conflict handler into a `RxReplicationHandler`.
/// The handler exposes the master-side surface used by the upstream replication
/// state machine: change-stream, paginated `changesSince`, and `masterWrite`
/// with conflict detection.
pub fn rx_storage_instance_to_replication_handler(
    instance: Arc<dyn crate::types::RxStorageInstance>,
    conflict_handler: Arc<dyn crate::types::RxConflictHandler>,
    database_instance_token: String,
    keep_meta: bool,
) -> Arc<dyn crate::types::RxReplicationHandler> {
    Arc::new(StorageReplicationHandler {
        instance,
        conflict_handler,
        database_instance_token,
        keep_meta,
    })
}

struct StorageReplicationHandler {
    instance: Arc<dyn crate::types::RxStorageInstance>,
    conflict_handler: Arc<dyn crate::types::RxConflictHandler>,
    database_instance_token: String,
    keep_meta: bool,
}

#[async_trait::async_trait]
impl crate::types::RxReplicationHandler for StorageReplicationHandler {
    fn master_change_stream(
        &self,
    ) -> crate::rxjs_compat::RxStream<crate::types::RxReplicationMasterChange> {
        use crate::replication_protocol::helper::write_doc_to_doc_state;
        if crate::rx_collection::is_demand_only_chunk_collection_name(
            self.instance.collection_name(),
        ) {
            return Box::pin(futures::stream::empty());
        }
        let has_attachments = self.instance.schema().extra.contains_key("attachments");
        let keep_meta = self.keep_meta;
        let max_event_bytes = crate::collection_policy::collection_policy()
            .master_response_ceiling_bytes(self.instance.collection_name());
        let stream = self.instance.change_stream();
        Box::pin(stream.map(move |event_bulk| {
            if event_bulk.is_rxsubject_lagged() {
                return crate::types::RxReplicationMasterChange::Resync;
            }
            let documents: Vec<serde_json::Value> = event_bulk
                .events
                .iter()
                .filter_map(|event| {
                    event.document_data.as_ref().map(|document_data| {
                        write_doc_to_doc_state(document_data, has_attachments, keep_meta)
                    })
                })
                .collect();
            let change = crate::types::RxReplicationMasterChange::Documents(
                crate::types::DocumentsWithCheckpoint {
                    documents,
                    checkpoint: event_bulk
                        .checkpoint
                        .clone()
                        .unwrap_or(serde_json::Value::Null),
                },
            );
            // A live storage burst must obey the same bound as catch-up.
            // Resync retains the durable checkpoint and drains the unchanged
            // documents through byte-bounded master_changes_since pages.
            if serde_json::to_vec(&change).map_or(true, |bytes| bytes.len() > max_event_bytes) {
                crate::types::RxReplicationMasterChange::Resync
            } else {
                change
            }
        }))
    }

    async fn master_changes_since(
        &self,
        checkpoint: Option<serde_json::Value>,
        batch_size: u64,
    ) -> Result<crate::types::DocumentsWithCheckpoint, crate::rx_error::RxError> {
        use crate::rx_schema_helper::get_primary_field_of_primary_key;

        let has_attachments = self.instance.schema().extra.contains_key("attachments");
        let transfer_ceiling_bytes = crate::collection_policy::collection_policy()
            .master_response_ceiling_bytes(self.instance.collection_name());
        let result = self
            .instance
            .get_changed_documents_since(batch_size, checkpoint.as_ref())
            .await?;
        // Match upstream RxDB: an empty page echoes the requested checkpoint so
        // the replication head cannot advance without a corresponding document.
        // With no requested checkpoint, retain the storage's initial checkpoint.
        let next_checkpoint = if result.documents.is_empty() {
            checkpoint.unwrap_or(result.checkpoint)
        } else {
            result.checkpoint
        };
        // Every response is byte-bounded, not only historically large chunk
        // collections. A count-only batch of ordinary JSON documents can exceed
        // the framed WebRTC transfer limit and then fail forever at the same
        // checkpoint. The limiter advances only to the last document actually
        // sent, so the browser's drain-until-empty loop receives the remainder.
        let primary_path = get_primary_field_of_primary_key(&self.instance.schema().primary_key);
        let limited = limit_master_response(
            result.documents,
            &primary_path,
            has_attachments,
            self.keep_meta,
            transfer_ceiling_bytes,
            &next_checkpoint,
        );
        let (documents, response_checkpoint) = (limited.documents, limited.checkpoint);
        Ok(crate::types::DocumentsWithCheckpoint {
            documents,
            checkpoint: response_checkpoint,
        })
    }

    async fn master_write(
        &self,
        rows: Vec<crate::types::RxReplicationWriteToMasterRow>,
    ) -> Result<Vec<serde_json::Value>, crate::rx_error::RxError> {
        use crate::replication_protocol::helper::{doc_state_to_write_doc, write_doc_to_doc_state};
        use crate::rx_schema_helper::get_primary_field_of_primary_key;
        use crate::types::BulkWriteRow;

        let primary_path = get_primary_field_of_primary_key(&self.instance.schema().primary_key);
        let has_attachments = self.instance.schema().extra.contains_key("attachments");

        // Index input rows by doc id.
        let mut row_by_id: std::collections::HashMap<
            String,
            crate::types::RxReplicationWriteToMasterRow,
        > = std::collections::HashMap::new();
        for mut row in rows.into_iter() {
            if let Some(sanitize) = MASTER_WRITE_SANITIZER.get() {
                sanitize(self.instance.collection_name(), &mut row.new_document_state);
            }
            let id = row
                .new_document_state
                .get(&primary_path)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            row_by_id.insert(id, row);
        }
        let ids: Vec<String> = row_by_id.keys().cloned().collect();

        // Fetch current master state for those ids.
        let master_docs_list = self.instance.find_documents_by_id(&ids, true).await?;
        let mut master_docs_state: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        for doc in master_docs_list.into_iter() {
            if let Some(id) = doc.get(&primary_path).and_then(|v| v.as_str()) {
                master_docs_state.insert(id.to_string(), doc);
            }
        }

        let mut conflicts: Vec<serde_json::Value> = Vec::new();
        let mut write_rows: Vec<BulkWriteRow> = Vec::new();

        // Rueckkehrer-Schutz: Ein Client mit tagealtem lokalen Spiegel besteht
        // die optimistische Sperre, indem er erst den aktuellen Master zieht
        // und seinen alten Inhalt obendrauf setzt. Am 19.08.2026 hat genau das
        // auf managed production tenant zweimal Bestandsdaten rueckwaerts ueberschrieben
        // (12:47 alle 19 Leads, 23:08 zwei weitere — Schreiber oazkvgmhxl und
        // vyeayvygxs). Traegt der eingehende Zustand ein updated_at_ms, das
        // deutlich AELTER ist als der Master, ist das keine Bearbeitung,
        // sondern eine Regression: sie wird als Konflikt beantwortet, der
        // Client uebernimmt den Master.
        const STALE_REGRESSION_THRESHOLD_MS: i64 = 60 * 60 * 1_000;
        fn incoming_is_stale_regression(
            incoming: &serde_json::Value,
            master: &serde_json::Value,
        ) -> bool {
            let lesen = |doc: &serde_json::Value| -> Option<i64> {
                doc.get("updated_at_ms")
                    .and_then(serde_json::Value::as_i64)
                    .filter(|value| *value > 0)
            };
            match (lesen(incoming), lesen(master)) {
                (Some(neu), Some(bestand)) => neu + STALE_REGRESSION_THRESHOLD_MS < bestand,
                _ => false,
            }
        }

        for (id, row) in row_by_id.into_iter() {
            let master_state = master_docs_state.get(&id).cloned();
            match (master_state, row.assumed_master_state.as_ref()) {
                (None, _) => {
                    let doc = doc_state_to_write_doc(
                        &self.database_instance_token,
                        has_attachments,
                        self.keep_meta,
                        &row.new_document_state,
                        None,
                    );
                    write_rows.push(BulkWriteRow {
                        previous: None,
                        document: doc,
                    });
                }
                (Some(master_state), None) => {
                    conflicts.push(write_doc_to_doc_state(
                        &master_state,
                        has_attachments,
                        self.keep_meta,
                    ));
                }
                (Some(master_state), Some(assumed)) => {
                    let master_state_doc =
                        write_doc_to_doc_state(&master_state, has_attachments, self.keep_meta);
                    let handler_matches = self
                        .conflict_handler
                        .is_equal(
                            &master_state_doc,
                            assumed,
                            "rxStorageInstanceToReplicationHandler-masterWrite",
                        )
                        .await;
                    // Mixed-version peers can use conflict handlers that reject
                    // two structurally identical wire states. The exact JSON
                    // equality is authoritative for this optimistic-lock check.
                    if (handler_matches || deep_equal(&master_state_doc, assumed))
                        && !incoming_is_stale_regression(&row.new_document_state, &master_state_doc)
                    {
                        let doc = doc_state_to_write_doc(
                            &self.database_instance_token,
                            has_attachments,
                            self.keep_meta,
                            &row.new_document_state,
                            Some(&master_state),
                        );
                        write_rows.push(BulkWriteRow {
                            previous: Some(master_state),
                            document: doc,
                        });
                    } else {
                        conflicts.push(master_state_doc);
                    }
                }
            }
        }

        if !write_rows.is_empty() {
            let result = self
                .instance
                .bulk_write(write_rows, "replication-master-write")
                .await?;
            for err in result.error.iter() {
                if err.status != 409 {
                    return Err(crate::rx_error::new_rx_error(
                        "SNH",
                        Some(serde_json::json!({
                            "name": "non conflict error",
                            "error": serde_json::to_value(err).unwrap_or(serde_json::Value::Null),
                        })),
                    ));
                }
                if let Some(in_db) = &err.document_in_db {
                    conflicts.push(write_doc_to_doc_state(
                        in_db,
                        has_attachments,
                        self.keep_meta,
                    ));
                }
            }
        }

        Ok(conflicts)
    }
}

struct LimitedMasterResponse {
    documents: Vec<serde_json::Value>,
    checkpoint: serde_json::Value,
}

/// Hard framing limit of the WebRTC transport. A payload above this can never be
/// delivered, no matter how the response is batched.
const WIRE_TRANSFER_LIMIT_BYTES: usize = 8 * 1024 * 1024;

fn limit_master_response(
    raw_documents: Vec<serde_json::Value>,
    primary_path: &str,
    has_attachments: bool,
    keep_meta: bool,
    max_bytes: usize,
    fallback_checkpoint: &serde_json::Value,
) -> LimitedMasterResponse {
    use crate::replication_protocol::helper::write_doc_to_doc_state;

    if raw_documents.is_empty() {
        return LimitedMasterResponse {
            documents: Vec::new(),
            checkpoint: fallback_checkpoint.clone(),
        };
    }

    let total_count = raw_documents.len();
    let mut documents = Vec::with_capacity(total_count);
    let mut checkpoint = fallback_checkpoint.clone();
    let mut bytes = 2usize; // JSON array brackets.
    let mut skipped = 0usize;

    for raw in raw_documents.into_iter() {
        let document = write_doc_to_doc_state(&raw, has_attachments, keep_meta);
        let document_bytes = serde_json::to_vec(&document)
            .map(|encoded| encoded.len().saturating_add(1))
            .unwrap_or(max_bytes.saturating_add(1));
        // A single document above the wire limit can never be framed. Sending it
        // alone wedges the stream forever: the peer retries the same checkpoint,
        // fails again, and every collection behind it starves. One 18 MiB command
        // document did exactly that and left the whole browser data plane empty.
        // Skip it loudly and advance past it so the remainder still drains.
        if document_bytes > WIRE_TRANSFER_LIMIT_BYTES {
            let id = raw
                .get(primary_path)
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<unknown>");
            eprintln!(
                "[ctox-rxdb] skipping undeliverable document `{id}` ({document_bytes} bytes) — \
                 above the {WIRE_TRANSFER_LIMIT_BYTES} byte wire transfer limit"
            );
            checkpoint = checkpoint_from_document(&raw, primary_path)
                .unwrap_or_else(|| fallback_checkpoint.clone());
            skipped += 1;
            continue;
        }
        if !documents.is_empty() && bytes.saturating_add(document_bytes) > max_bytes {
            break;
        }
        bytes = bytes.saturating_add(document_bytes);
        checkpoint = checkpoint_from_document(&raw, primary_path)
            .unwrap_or_else(|| fallback_checkpoint.clone());
        documents.push(document);
    }

    if documents.len().saturating_add(skipped) == total_count {
        checkpoint = fallback_checkpoint.clone();
    }

    LimitedMasterResponse {
        documents,
        checkpoint,
    }
}

fn checkpoint_from_document(
    document: &serde_json::Value,
    primary_path: &str,
) -> Option<serde_json::Value> {
    let id = document.get(primary_path)?.clone();
    let lwt = document
        .get("_meta")
        .and_then(|meta| meta.get("lwt"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!(0));
    Some(serde_json::json!({ "id": id, "lwt": lwt }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};

    use serde_json::{json, Value};
    use tokio::time::{timeout, Duration};

    use crate::plugins::storage_memory::get_rx_storage_memory;
    use crate::replication_protocol::default_conflict_handler::DefaultConflictHandler;
    use crate::replication_protocol::index_mod::test_utils::{
        test_schema_variant, TestHashFunction, TestSchemaVariant,
    };
    use crate::rxjs_compat::DEFAULT_SUBJECT_BUFFER;
    use crate::types::{
        BulkWriteRow, DocumentsWithCheckpoint, FirstSyncDone, ReplicationEvents, ReplicationStats,
        RxReplicationMasterChange, RxStorageInstance, RxStorageInstanceCreationParams,
        RxStorageInstanceReplicationInput, RxStorageInstanceReplicationState,
        RxStorageReplicationDirection, StreamQueue,
    };

    async fn idle_test_state(database_name: &str) -> Arc<RxStorageInstanceReplicationState> {
        let storage = get_rx_storage_memory(());
        let schema = test_schema_variant(TestSchemaVariant::ProtocolIndex);
        let fork_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: database_name.to_string(),
                    collection_name: "fork".to_string(),
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
        let meta_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: database_name.to_string(),
                    collection_name: "meta".to_string(),
                    schema,
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let conflict_handler = Arc::new(DefaultConflictHandler);
        let replication_handler = rx_storage_instance_to_replication_handler(
            Arc::clone(&fork_instance),
            conflict_handler.clone(),
            "db-token".to_string(),
            false,
        );
        let state = Arc::new(RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: Arc::new(RxStorageInstanceReplicationInput {
                identifier: "idle-test".to_string(),
                fork_instance,
                meta_instance,
                hash_function: Arc::new(TestHashFunction),
                conflict_handler,
                replication_handler,
                push_batch_size: 10,
                pull_batch_size: 10,
                bulk_size: 10,
                keep_meta: false,
                initial_checkpoint: None,
                wait_before_persist: None,
            }),
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
        state.first_sync_done.down.next(true);
        state.first_sync_done.up.next(true);
        state
            .events
            .active
            .finish_initial(RxStorageReplicationDirection::Down);
        state
            .events
            .active
            .finish_initial(RxStorageReplicationDirection::Up);
        state
    }

    #[tokio::test]
    async fn replication_idle_returns_immediately_when_already_idle() {
        let state = idle_test_state("replication-idle-already-idle").await;

        timeout(
            Duration::from_millis(100),
            await_rx_storage_replication_idle(state),
        )
        .await
        .expect("already-idle replication must not wait for a future activity edge");
    }

    #[tokio::test]
    async fn replication_idle_waits_for_running_work_then_returns() {
        let state = idle_test_state("replication-idle-running-work").await;
        let activity = state.events.active.track(RxStorageReplicationDirection::Up);
        let state_for_wait = Arc::clone(&state);
        let idle = tokio::spawn(async move {
            await_rx_storage_replication_idle(state_for_wait).await;
        });

        tokio::task::yield_now().await;
        assert!(!idle.is_finished(), "idle resolved while work was active");

        drop(activity);
        timeout(Duration::from_secs(1), idle)
            .await
            .expect("idle must resolve after activity ends")
            .unwrap();
    }

    #[tokio::test]
    async fn large_live_burst_resyncs_and_drains_every_byte_before_small_live_updates() {
        let state = idle_test_state("large-live-burst").await;
        let handler = &state.input.replication_handler;
        let mut stream = handler.master_change_stream();
        let expected: Vec<Value> = (0..21).map(|index| json!({
            "id":format!("source-{index:02}"), "content":"export const α = \"value\";\n".repeat(18_000),
            "_rev":"1-source", "_deleted":false, "_meta":{"lwt":index+1}, "_attachments":{}
        })).collect();
        assert!(serde_json::to_vec(&expected).unwrap().len() > WIRE_TRANSFER_LIMIT_BYTES);
        let result = state
            .input
            .fork_instance
            .bulk_write(
                expected
                    .iter()
                    .map(|doc| BulkWriteRow {
                        previous: None,
                        document: doc.clone(),
                    })
                    .collect(),
                "large-live-fixture",
            )
            .await
            .unwrap();
        assert!(result.error.is_empty());
        let event = timeout(Duration::from_secs(2), stream.next())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, RxReplicationMasterChange::Resync));
        let mut checkpoint = None;
        let mut received = BTreeSet::new();
        loop {
            let page = handler
                .master_changes_since(checkpoint.clone(), 20)
                .await
                .unwrap();
            assert!(serde_json::to_vec(&page).unwrap().len() < 1024 * 1024 + 1024);
            if page.documents.is_empty() {
                break;
            }
            assert_ne!(checkpoint.as_ref(), Some(&page.checkpoint));
            for doc in page.documents {
                let original = expected.iter().find(|v| v["id"] == doc["id"]).unwrap();
                assert_eq!(doc["content"], original["content"]);
                assert!(received.insert(doc["id"].as_str().unwrap().to_owned()));
            }
            checkpoint = Some(page.checkpoint);
        }
        assert_eq!(received.len(), expected.len());
        let small = json!({"id":"after-burst", "content":"ok", "_rev":"1-small",
            "_deleted":false, "_meta":{"lwt":22}, "_attachments":{}});
        assert!(state
            .input
            .fork_instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: small
                }],
                "small-live-fixture"
            )
            .await
            .unwrap()
            .error
            .is_empty());
        let event = timeout(Duration::from_secs(2), stream.next())
            .await
            .unwrap()
            .unwrap();
        let RxReplicationMasterChange::Documents(page) = event else {
            panic!("small update should stream directly")
        };
        assert_eq!(page.documents[0]["id"], "after-burst");
    }

    #[test]
    fn desktop_file_chunk_response_limit_advances_checkpoint_to_last_sent_doc() {
        let docs = vec![
            json!({"id":"a","data":"x".repeat(48),"_meta":{"lwt":1.0}}),
            json!({"id":"b","data":"y".repeat(48),"_meta":{"lwt":2.0}}),
            json!({"id":"c","data":"z".repeat(48),"_meta":{"lwt":3.0}}),
        ];
        let limited =
            limit_master_response(docs, "id", false, true, 120, &json!({"id":"c","lwt":3.0}));

        assert_eq!(limited.documents.len(), 1);
        assert_eq!(limited.checkpoint, json!({"id":"a","lwt":1.0}));
    }

    #[test]
    fn desktop_file_chunk_response_limit_uses_fallback_checkpoint_when_all_fit() {
        let docs = vec![
            json!({"id":"a","data":"x","_meta":{"lwt":1.0}}),
            json!({"id":"b","data":"y","_meta":{"lwt":2.0}}),
        ];
        let fallback = json!({"id":"b","lwt":2.0});
        let limited = limit_master_response(docs, "id", false, true, 4096, &fallback);

        assert_eq!(limited.documents.len(), 2);
        assert_eq!(limited.checkpoint, fallback);
    }

    #[test]
    fn a_single_undeliverable_document_does_not_wedge_the_stream() {
        // One 18 MiB `sellify.sync.refresh` command whose result carried the whole
        // projection list froze the tenant's browser data plane: it exceeded the
        // wire limit on its own, so every retry failed at the same checkpoint and
        // no collection behind it ever replicated.
        let oversized = json!({
            "id": "cmd-oversized",
            "payload": "x".repeat(9 * 1024 * 1024),
            "_meta": { "lwt": 1.0 },
        });
        let small = json!({
            "id": "cmd-small",
            "payload": "ok",
            "_meta": { "lwt": 2.0 },
        });
        let fallback = json!({ "id": "cmd-small", "lwt": 2.0 });

        let limited = limit_master_response(
            vec![oversized, small],
            "id",
            false,
            false,
            crate::collection_policy::DEFAULT_MASTER_RESPONSE_CEILING_BYTES,
            &fallback,
        );

        let ids = limited
            .documents
            .iter()
            .map(|doc| doc["id"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec!["cmd-small".to_string()],
            "the undeliverable document must be skipped, the deliverable one sent"
        );
        assert!(
            serde_json::to_vec(&limited.documents).unwrap().len() < 8 * 1024 * 1024,
            "the response must stay under the wire transfer limit"
        );
        assert_eq!(
            limited.checkpoint, fallback,
            "the checkpoint must advance past the skipped document so the drain continues"
        );
    }

    #[test]
    fn generic_master_response_ceiling_drains_twenty_one_large_documents() {
        let docs = (0..21usize)
            .map(|index| {
                json!({
                    "id": format!("lead-{index:02}"),
                    "payload": "x".repeat(600 * 1024),
                    "_meta": { "lwt": index as f64 + 1.0 },
                })
            })
            .collect::<Vec<_>>();
        let first_count_only_page = DocumentsWithCheckpoint {
            documents: docs[..20].to_vec(),
            checkpoint: checkpoint_from_document(&docs[19], "id").unwrap(),
        };
        assert!(
            serde_json::to_vec(&first_count_only_page).unwrap().len() > 8 * 1024 * 1024,
            "the regression fixture must exceed the framed WebRTC transfer ceiling"
        );

        let max_bytes = crate::collection_policy::collection_policy()
            .master_response_ceiling_bytes("ordinary_large_documents");
        let mut offset = 0usize;
        let mut received_ids = Vec::new();
        while offset < docs.len() {
            let page_end = (offset + 20).min(docs.len());
            let page = docs[offset..page_end].to_vec();
            let fallback = checkpoint_from_document(page.last().unwrap(), "id").unwrap();
            let limited = limit_master_response(page, "id", false, true, max_bytes, &fallback);
            assert!(
                !limited.documents.is_empty(),
                "bounded pull made no progress"
            );
            let encoded = serde_json::to_vec(&DocumentsWithCheckpoint {
                documents: limited.documents.clone(),
                checkpoint: limited.checkpoint.clone(),
            })
            .unwrap();
            assert!(
                encoded.len() <= max_bytes + 1024,
                "bounded response is {} bytes for a {} byte ceiling",
                encoded.len(),
                max_bytes
            );
            received_ids.extend(
                limited
                    .documents
                    .iter()
                    .filter_map(|document| document.get("id").and_then(Value::as_str))
                    .map(str::to_owned),
            );
            offset += limited.documents.len();
        }

        assert_eq!(
            received_ids,
            (0..21usize)
                .map(|index| format!("lead-{index:02}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn knowledge_tables_response_limit_drains_57_large_chunks_without_skipping_rows() {
        let mut docs = Vec::with_capacity(57);
        let mut expected_rows = 0usize;
        for chunk_index in 0..57usize {
            let row_count = if chunk_index < 31 { 86 } else { 85 };
            expected_rows += row_count;
            let rows = (0..row_count)
                .map(|row_index| {
                    json!({
                        "row_id": chunk_index * 100 + row_index,
                        "value": "x".repeat(4550),
                    })
                })
                .collect::<Vec<_>>();
            let document = json!({
                "id": format!("measured_load_points:{chunk_index}"),
                "chunk_index": chunk_index,
                "chunk_count": 57,
                "row_count": row_count,
                "rows": rows,
                "_meta": { "lwt": chunk_index as f64 + 1.0 },
            });
            let encoded_size = serde_json::to_vec(&document).unwrap().len();
            assert!(
                (380 * 1024..=414 * 1024).contains(&encoded_size),
                "fixture chunk {chunk_index} is {encoded_size} bytes, outside the production range"
            );
            docs.push(document);
        }
        assert_eq!(expected_rows, 4_876);

        // Simulate the browser pull loop: every byte-bounded response must
        // advance to the last document actually sent, and the loop must drain
        // all 57 responses before accepting the final checkpoint.
        let mut offset = 0usize;
        let mut checkpoint = json!(null);
        let mut received = Vec::new();
        while offset < docs.len() {
            let fallback = checkpoint_from_document(docs.last().unwrap(), "id").unwrap();
            let limited = limit_master_response(
                docs[offset..].to_vec(),
                "id",
                false,
                true,
                crate::collection_policy::collection_policy()
                    .transfer_ceiling_bytes("knowledge_tables")
                    .expect("knowledge_tables ceiling"),
                &fallback,
            );
            assert!(
                !limited.documents.is_empty(),
                "large knowledge response made no progress"
            );
            checkpoint = limited.checkpoint;
            let sent = limited.documents.len();
            received.extend(limited.documents);
            offset += sent;
        }

        let received_rows: usize = received
            .iter()
            .map(|doc| {
                doc.get("rows")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len)
            })
            .sum();
        assert_eq!(received.len(), 57);
        assert_eq!(received_rows, expected_rows);
        assert_eq!(
            checkpoint.get("id").and_then(Value::as_str),
            Some("measured_load_points:56")
        );
    }

    #[tokio::test]
    async fn storage_master_change_stream_lag_maps_to_resync() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema_variant(TestSchemaVariant::ProtocolIndex);
        let instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-master-lag".to_string(),
                    collection_name: "docs".to_string(),
                    schema,
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let handler = rx_storage_instance_to_replication_handler(
            Arc::clone(&instance),
            Arc::new(DefaultConflictHandler),
            "db-token".to_string(),
            false,
        );
        let mut stream = handler.master_change_stream();
        for i in 0..(DEFAULT_SUBJECT_BUFFER + 8) {
            instance
                .bulk_write(
                    vec![BulkWriteRow {
                        previous: None,
                        document: json!({ "id": format!("doc-{i}") }),
                    }],
                    "test",
                )
                .await
                .unwrap();
        }

        let first = timeout(Duration::from_secs(1), stream.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first, RxReplicationMasterChange::Resync);
    }

    #[tokio::test]
    async fn slow_master_change_stream_peer_recovers_all_docs_after_resync() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema_variant(TestSchemaVariant::ProtocolIndex);
        let instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "db-master-slow-peer-resync".to_string(),
                    collection_name: "docs".to_string(),
                    schema,
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let handler = rx_storage_instance_to_replication_handler(
            Arc::clone(&instance),
            Arc::new(DefaultConflictHandler),
            "db-token".to_string(),
            false,
        );
        let mut stream = handler.master_change_stream();
        let total = DEFAULT_SUBJECT_BUFFER + 31;
        for i in 0..total {
            instance
                .bulk_write(
                    vec![BulkWriteRow {
                        previous: None,
                        document: json!({ "id": format!("doc-{i:04}") }),
                    }],
                    "test",
                )
                .await
                .unwrap();
        }

        let first = timeout(Duration::from_secs(1), stream.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first, RxReplicationMasterChange::Resync);

        let mut checkpoint = None;
        let mut recovered_ids = BTreeSet::new();
        loop {
            let page = handler
                .master_changes_since(checkpoint.take(), 17)
                .await
                .unwrap();
            if page.documents.is_empty() {
                break;
            }
            for doc in page.documents {
                recovered_ids.insert(
                    doc.get("id")
                        .and_then(|value| value.as_str())
                        .expect("recovered document id")
                        .to_string(),
                );
            }
            checkpoint = Some(page.checkpoint);
        }

        let expected_last = format!("doc-{:04}", total - 1);
        assert_eq!(recovered_ids.len(), total);
        assert_eq!(recovered_ids.first().map(String::as_str), Some("doc-0000"));
        assert_eq!(
            recovered_ids.last().map(String::as_str),
            Some(expected_last.as_str())
        );
    }
}
