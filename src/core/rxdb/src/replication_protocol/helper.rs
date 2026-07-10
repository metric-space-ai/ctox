//! Port of `src/replication-protocol/helper.ts`.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::plugins::utils::utils_revision::{create_revision, get_height_of_revision};
use crate::plugins::utils::utils_time::now;
use crate::rx_storage_helper::strip_attachments_data_from_document;
use crate::types::{BulkWriteRow, RxStorageInstance, RxStorageInstanceReplicationState};

// ref: rxdb/src/replication-protocol/helper.ts:19-49
pub fn doc_state_to_write_doc(
    database_instance_token: &str,
    has_attachments: bool,
    keep_meta: bool,
    doc_state: &Value,
    previous: Option<&Value>,
) -> Value {
    let mut out = doc_state.clone();
    if let Some(obj) = out.as_object_mut() {
        // _attachments
        let want_attachments = has_attachments
            && doc_state
                .get("_attachments")
                .map(|v| v.is_object())
                .unwrap_or(false);
        if want_attachments {
            obj.insert(
                "_attachments".to_string(),
                doc_state.get("_attachments").cloned().unwrap(),
            );
        } else {
            obj.insert(
                "_attachments".to_string(),
                Value::Object(serde_json::Map::new()),
            );
        }
        // _meta
        if keep_meta {
            if let Some(m) = doc_state.get("_meta").cloned() {
                obj.insert("_meta".to_string(), m);
            }
        } else {
            let mut meta = previous
                .and_then(|p| p.get("_meta"))
                .cloned()
                .unwrap_or(Value::Object(serde_json::Map::new()));
            if let Some(m_obj) = meta.as_object_mut() {
                m_obj.insert("lwt".to_string(), json!(now()));
                if let Some(ctox_hlc) = doc_state
                    .get("_meta")
                    .and_then(|value| value.get("ctoxHlc"))
                    .cloned()
                {
                    m_obj.insert("ctoxHlc".to_string(), ctox_hlc);
                }
            } else {
                meta = json!({ "lwt": now() });
            }
            obj.insert("_meta".to_string(), meta);
        }
        // _rev
        if !keep_meta {
            obj.insert("_rev".to_string(), Value::String(String::new()));
        }
        let needs_rev = obj
            .get("_rev")
            .and_then(|v| v.as_str())
            .map(|s| s.is_empty())
            .unwrap_or(true);
        if needs_rev {
            let prev_rev = previous
                .and_then(|p| p.get("_rev"))
                .and_then(|v| v.as_str());
            let rev = create_revision(database_instance_token, prev_rev).unwrap_or_default();
            obj.insert("_rev".to_string(), Value::String(rev));
        }
    }
    out
}

// ref: rxdb/src/replication-protocol/helper.ts:51-66
pub fn write_doc_to_doc_state(write_doc: &Value, keep_attachments: bool, keep_meta: bool) -> Value {
    let mut ret = write_doc.clone();
    if let Some(obj) = ret.as_object_mut() {
        if !keep_attachments {
            obj.remove("_attachments");
        }
        if !keep_meta {
            let ctox_hlc = obj
                .get("_meta")
                .and_then(|meta| meta.get("ctoxHlc"))
                .cloned();
            obj.remove("_meta");
            if let Some(ctox_hlc) = ctox_hlc {
                obj.insert("_meta".to_string(), json!({ "ctoxHlc": ctox_hlc }));
            }
            obj.remove("_rev");
        }
    }
    ret
}

// ref: rxdb/src/replication-protocol/helper.ts:69-85
pub fn strip_attachments_data_from_meta_write_rows(
    state: &RxStorageInstanceReplicationState,
    rows: &[BulkWriteRow],
) -> Vec<BulkWriteRow> {
    if !state.has_attachments {
        return rows.to_vec();
    }

    rows.iter()
        .map(|row| {
            let mut document = row.document.clone();
            if let Some(doc_data) = document.get("docData").cloned() {
                if let Some(obj) = document.as_object_mut() {
                    obj.insert(
                        "docData".to_string(),
                        strip_attachments_data_from_document(&doc_data),
                    );
                }
            }
            BulkWriteRow {
                previous: row.previous.clone(),
                document,
            }
        })
        .collect()
}

pub fn remote_revision_height_marker_matches(write_doc: &Value, identifier: &str) -> bool {
    let Some(revision) = write_doc.get("_rev").and_then(Value::as_str) else {
        return false;
    };
    let Ok(revision_height) = get_height_of_revision(revision) else {
        return false;
    };
    write_doc
        .get("_meta")
        .and_then(|meta| meta.get(identifier))
        .and_then(json_number_as_u64)
        .is_some_and(|marked_height| marked_height == revision_height)
}

fn json_number_as_u64(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|n| n.fract() == 0.0)
            .map(|n| n as u64)
    })
}

// ref: rxdb/src/replication-protocol/helper.ts:87-98
pub fn get_underlying_persistent_storage(
    instance: Arc<dyn RxStorageInstance>,
) -> Arc<dyn RxStorageInstance> {
    let mut current = instance;
    while let Some(next) = current.underlying_persistent_storage() {
        current = next;
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use async_trait::async_trait;
    use serde_json::json;

    use crate::rx_error::{new_rx_error, RxError};
    use crate::rxjs_compat::RxStream;
    use crate::types::{
        EventBulk, HashFunction, HashOutput, RxConflictHandler, RxConflictHandlerInput,
        RxJsonSchema, RxReplicationHandler, RxStorageBulkWriteResponse,
        RxStorageChangedDocumentsSinceResult, RxStorageCountResult,
        RxStorageInstanceReplicationInput, RxStorageQueryResult,
    };

    fn test_schema() -> RxJsonSchema {
        RxJsonSchema {
            version: 0,
            primary_key: crate::types::PrimaryKey::Simple("id".to_string()),
            schema_type: "object".to_string(),
            properties: HashMap::new(),
            required: Vec::new(),
            indexes: Vec::new(),
            encrypted: Vec::new(),
            internal_indexes: Vec::new(),
            key_compression: false,
            attachments: None,
            additional_properties: true,
            extra: HashMap::new(),
        }
    }

    struct NoopStorageInstance {
        collection_name: String,
        schema: RxJsonSchema,
        underlying: Option<Arc<dyn RxStorageInstance>>,
    }

    impl NoopStorageInstance {
        fn new(collection_name: &str, underlying: Option<Arc<dyn RxStorageInstance>>) -> Arc<Self> {
            Arc::new(Self {
                collection_name: collection_name.to_string(),
                schema: test_schema(),
                underlying,
            })
        }
    }

    #[async_trait]
    impl RxStorageInstance for NoopStorageInstance {
        fn database_name(&self) -> &str {
            "db"
        }

        fn collection_name(&self) -> &str {
            &self.collection_name
        }

        fn schema(&self) -> &RxJsonSchema {
            &self.schema
        }

        async fn bulk_write(
            &self,
            _document_writes: Vec<BulkWriteRow>,
            _context: &str,
        ) -> Result<RxStorageBulkWriteResponse, RxError> {
            Ok(RxStorageBulkWriteResponse::default())
        }

        async fn find_documents_by_id(
            &self,
            _ids: &[String],
            _with_deleted: bool,
        ) -> Result<Vec<Value>, RxError> {
            Ok(Vec::new())
        }

        async fn query(&self, _prepared_query: &Value) -> Result<RxStorageQueryResult, RxError> {
            Ok(RxStorageQueryResult::default())
        }

        async fn count(&self, _prepared_query: &Value) -> Result<RxStorageCountResult, RxError> {
            Ok(RxStorageCountResult {
                count: 0,
                mode: "exact".to_string(),
            })
        }

        async fn get_changed_documents_since(
            &self,
            _limit: u64,
            _checkpoint: Option<&Value>,
        ) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
            Ok(RxStorageChangedDocumentsSinceResult::default())
        }

        fn change_stream(&self) -> RxStream<EventBulk> {
            Box::pin(futures::stream::empty())
        }

        async fn cleanup(&self, _min_deleted_time: i64) -> Result<bool, RxError> {
            Ok(true)
        }

        async fn remove(&self) -> Result<(), RxError> {
            Ok(())
        }

        async fn close(&self) -> Result<(), RxError> {
            Ok(())
        }

        fn underlying_persistent_storage(&self) -> Option<Arc<dyn RxStorageInstance>> {
            self.underlying.clone()
        }
    }

    struct NoopConflictHandler;

    #[async_trait]
    impl RxConflictHandler for NoopConflictHandler {
        async fn is_equal(&self, _a: &Value, _b: &Value, _ctx: &str) -> bool {
            true
        }

        async fn resolve(&self, input: &RxConflictHandlerInput, _ctx: &str) -> Value {
            input.new_document_state.clone()
        }
    }

    struct NoopReplicationHandler;

    #[async_trait]
    impl RxReplicationHandler for NoopReplicationHandler {
        fn master_change_stream(
            &self,
        ) -> crate::rxjs_compat::RxStream<crate::types::RxReplicationMasterChange> {
            Box::pin(futures::stream::empty())
        }

        async fn master_changes_since(
            &self,
            _checkpoint: Option<Value>,
            _batch_size: u64,
        ) -> Result<crate::types::DocumentsWithCheckpoint, RxError> {
            Err(new_rx_error("TEST", None))
        }

        async fn master_write(
            &self,
            _rows: Vec<crate::types::RxReplicationWriteToMasterRow>,
        ) -> Result<Vec<Value>, RxError> {
            Ok(Vec::new())
        }
    }

    struct TestHashFunction;

    impl HashFunction for TestHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("hash:{input}") })
        }
    }

    fn replication_state(has_attachments: bool) -> RxStorageInstanceReplicationState {
        let instance = NoopStorageInstance::new("meta", None);
        let input = RxStorageInstanceReplicationInput {
            identifier: "id".to_string(),
            fork_instance: instance.clone(),
            meta_instance: instance,
            hash_function: Arc::new(TestHashFunction),
            conflict_handler: Arc::new(NoopConflictHandler),
            replication_handler: Arc::new(NoopReplicationHandler),
            push_batch_size: 5,
            pull_batch_size: 5,
            bulk_size: 5,
            keep_meta: false,
            initial_checkpoint: None,
            wait_before_persist: None,
        };

        RxStorageInstanceReplicationState {
            primary_path: "id".to_string(),
            input: Arc::new(input),
            checkpoint_key: "checkpoint".to_string(),
            downstream_bulk_write_flag: "down".to_string(),
            last_checkpoint_doc: parking_lot::Mutex::new(HashMap::new()),
            events: crate::types::ReplicationEvents::new(),
            stats: crate::types::ReplicationStats::new(),
            first_sync_done: crate::types::FirstSyncDone::default(),
            stream_queue: crate::types::StreamQueue::default(),
            checkpoint_queue: tokio::sync::Mutex::new(()),
            has_attachments,
        }
    }

    #[test]
    fn keep_meta_false_preserves_only_ctox_hlc_across_wire_state() {
        let write_doc = json!({
            "id": "doc-1",
            "_rev": "1-test",
            "_meta": {
                "lwt": 42,
                "ctoxHlc": "16:1:browser-a",
                "ctoxReplicationOrigin": {"role": "browser"}
            }
        });
        let state = write_doc_to_doc_state(&write_doc, false, false);
        assert_eq!(
            state.pointer("/_meta/ctoxHlc"),
            Some(&json!("16:1:browser-a"))
        );
        assert!(state.pointer("/_meta/lwt").is_none());
        assert!(state.pointer("/_meta/ctoxReplicationOrigin").is_none());
        assert!(state.get("_rev").is_none());
    }

    #[test]
    fn strips_attachment_data_from_meta_write_rows_when_enabled() {
        let state = replication_state(true);
        let rows = vec![BulkWriteRow {
            previous: Some(json!({"id": "doc-1"})),
            document: json!({
                "id": "doc-1|0",
                "docData": {
                    "id": "doc-1",
                    "_attachments": {
                        "a.txt": {
                            "data": "aGVsbG8=",
                            "digest": "sha256-x",
                            "length": 5,
                            "type": "text/plain"
                        }
                    }
                }
            }),
        }];

        let stripped = strip_attachments_data_from_meta_write_rows(&state, &rows);
        assert_eq!(rows[0].previous, stripped[0].previous);
        assert_eq!(
            stripped[0]
                .document
                .pointer("/docData/_attachments/a.txt/data"),
            None
        );
        assert_eq!(
            stripped[0]
                .document
                .pointer("/docData/_attachments/a.txt/digest"),
            Some(&json!("sha256-x"))
        );
        assert!(rows[0]
            .document
            .pointer("/docData/_attachments/a.txt/data")
            .is_some());
    }

    #[test]
    fn leaves_meta_write_rows_unchanged_without_attachments() {
        let state = replication_state(false);
        let rows = vec![BulkWriteRow {
            previous: None,
            document: json!({"id": "doc-1|0", "docData": {"id": "doc-1"}}),
        }];

        assert_eq!(
            strip_attachments_data_from_meta_write_rows(&state, &rows)[0].document,
            rows[0].document
        );
    }

    #[test]
    fn remote_revision_height_marker_accepts_integer_and_float_markers() {
        assert!(remote_revision_height_marker_matches(
            &json!({
                "_rev": "2-master",
                "_meta": { "replication-test": 2 }
            }),
            "replication-test"
        ));
        assert!(remote_revision_height_marker_matches(
            &json!({
                "_rev": "2-master",
                "_meta": { "replication-test": 2.0 }
            }),
            "replication-test"
        ));
        assert!(!remote_revision_height_marker_matches(
            &json!({
                "_rev": "3-master",
                "_meta": { "replication-test": 2 }
            }),
            "replication-test"
        ));
    }

    #[test]
    fn walks_to_underlying_persistent_storage() {
        let persistent = NoopStorageInstance::new("persistent", None);
        let middle = NoopStorageInstance::new("middle", Some(persistent.clone()));
        let top = NoopStorageInstance::new("top", Some(middle));

        let found = get_underlying_persistent_storage(top);
        assert_eq!(found.collection_name(), "persistent");
    }
}
