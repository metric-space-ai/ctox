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
use crate::types::{
    FirstSyncDone, ReplicationEvents, ReplicationStats, RxStorageInstanceReplicationInput,
    RxStorageInstanceReplicationState, StreamQueue,
};

const DESKTOP_FILE_CHUNKS_MASTER_RESPONSE_MAX_BYTES: usize = 96 * 1024;

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

// ref: rxdb/src/replication-protocol/index.ts:145-167
/// Awaits replication to be idle: first in-sync, then all queues drained.
///
/// Upstream checks `down === state.streamQueue.down && up === state.streamQueue.up`
/// after awaiting, to detect whether new tasks were enqueued during the wait.
/// With `tokio::sync::Mutex` the analogous check is "did anyone try to acquire
/// the lock while we held it?". We model this by acquiring all three locks
/// twice — if no new task came in, the second acquire is immediate.
pub async fn await_rx_storage_replication_idle(state: Arc<RxStorageInstanceReplicationState>) {
    await_rx_storage_replication_first_in_sync(Arc::clone(&state)).await;
    // First drain.
    {
        let _down = state.stream_queue.down.lock().await;
        let _up = state.stream_queue.up.lock().await;
    }
    // Second drain to confirm idle.
    {
        let _down = state.stream_queue.down.lock().await;
        let _up = state.stream_queue.up.lock().await;
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
        .get("attachments")
        .is_some();
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
        let has_attachments = self.instance.schema().extra.get("attachments").is_some();
        let keep_meta = self.keep_meta;
        let stream = self.instance.change_stream();
        Box::pin(stream.map(move |event_bulk| {
            if event_bulk.is_rxsubject_lagged() {
                return crate::types::RxReplicationMasterChange::Resync;
            }
            let documents: Vec<serde_json::Value> = event_bulk
                .events
                .iter()
                .map(|event| {
                    let doc = event
                        .document_data
                        .clone()
                        .unwrap_or(serde_json::Value::Null);
                    write_doc_to_doc_state(&doc, has_attachments, keep_meta)
                })
                .collect();
            crate::types::RxReplicationMasterChange::Documents(
                crate::types::DocumentsWithCheckpoint {
                    documents,
                    checkpoint: event_bulk
                        .checkpoint
                        .clone()
                        .unwrap_or(serde_json::Value::Null),
                },
            )
        }))
    }

    async fn master_changes_since(
        &self,
        checkpoint: Option<serde_json::Value>,
        batch_size: u64,
    ) -> Result<crate::types::DocumentsWithCheckpoint, crate::rx_error::RxError> {
        use crate::replication_protocol::helper::write_doc_to_doc_state;
        use crate::rx_schema_helper::get_primary_field_of_primary_key;

        let has_attachments = self.instance.schema().extra.get("attachments").is_some();
        let is_file_chunks = self.instance.collection_name() == "desktop_file_chunks";
        let result = self
            .instance
            .get_changed_documents_since(batch_size, checkpoint.as_ref())
            .await?;
        let next_checkpoint = if result.documents.is_empty() {
            checkpoint.unwrap_or(result.checkpoint)
        } else {
            result.checkpoint
        };
        let documents = if is_file_chunks {
            let primary_path =
                get_primary_field_of_primary_key(&self.instance.schema().primary_key);
            let limited = limit_desktop_file_chunk_response(
                result.documents,
                &primary_path,
                has_attachments,
                self.keep_meta,
                DESKTOP_FILE_CHUNKS_MASTER_RESPONSE_MAX_BYTES,
                &next_checkpoint,
            );
            return Ok(crate::types::DocumentsWithCheckpoint {
                documents: limited.documents,
                checkpoint: limited.checkpoint,
            });
        } else {
            result
                .documents
                .into_iter()
                .map(|d| write_doc_to_doc_state(&d, has_attachments, self.keep_meta))
                .collect()
        };
        Ok(crate::types::DocumentsWithCheckpoint {
            documents,
            checkpoint: next_checkpoint,
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
        let has_attachments = self.instance.schema().extra.get("attachments").is_some();

        // Index input rows by doc id.
        let mut row_by_id: std::collections::HashMap<
            String,
            crate::types::RxReplicationWriteToMasterRow,
        > = std::collections::HashMap::new();
        for row in rows.into_iter() {
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
                    if handler_matches || deep_equal(&master_state_doc, assumed) {
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

fn limit_desktop_file_chunk_response(
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

    for raw in raw_documents.into_iter() {
        let document = write_doc_to_doc_state(&raw, has_attachments, keep_meta);
        let document_bytes = serde_json::to_vec(&document)
            .map(|encoded| encoded.len().saturating_add(1))
            .unwrap_or(max_bytes.saturating_add(1));
        if !documents.is_empty() && bytes.saturating_add(document_bytes) > max_bytes {
            break;
        }
        bytes = bytes.saturating_add(document_bytes);
        checkpoint = checkpoint_from_document(&raw, primary_path)
            .unwrap_or_else(|| fallback_checkpoint.clone());
        documents.push(document);
    }

    if documents.len() == total_count {
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

    use serde_json::json;
    use tokio::time::{timeout, Duration};

    use crate::plugins::storage_memory::get_rx_storage_memory;
    use crate::replication_protocol::default_conflict_handler::DefaultConflictHandler;
    use crate::rx_schema_helper::fill_with_default_settings;
    use crate::rxjs_compat::DEFAULT_SUBJECT_BUFFER;
    use crate::types::{
        BulkWriteRow, JsonSchema, PrimaryKey, RxJsonSchema, RxReplicationMasterChange,
        RxStorageInstance, RxStorageInstanceCreationParams,
    };

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
        fill_with_default_settings(RxJsonSchema {
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
            additional_properties: true,
            extra: HashMap::new(),
        })
    }

    #[test]
    fn desktop_file_chunk_response_limit_advances_checkpoint_to_last_sent_doc() {
        let docs = vec![
            json!({"id":"a","data":"x".repeat(48),"_meta":{"lwt":1.0}}),
            json!({"id":"b","data":"y".repeat(48),"_meta":{"lwt":2.0}}),
            json!({"id":"c","data":"z".repeat(48),"_meta":{"lwt":3.0}}),
        ];
        let limited = limit_desktop_file_chunk_response(
            docs,
            "id",
            false,
            true,
            120,
            &json!({"id":"c","lwt":3.0}),
        );

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
        let limited = limit_desktop_file_chunk_response(docs, "id", false, true, 4096, &fallback);

        assert_eq!(limited.documents.len(), 2);
        assert_eq!(limited.checkpoint, fallback);
    }

    #[tokio::test]
    async fn storage_master_change_stream_lag_maps_to_resync() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
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
        let schema = test_schema();
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
