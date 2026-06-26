//! Helper functions for accessing the RxStorage instances.
//!
//! **Partial port.** This module contains the subset of upstream
//! `src/rx-storage-helper.ts` that is currently needed by the Rust daemon.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream;
use futures::StreamExt;
use serde_json::{json, Value};
use tokio::sync::Mutex as TokioMutex;

use crate::hooks::run_plugin_hooks;
use crate::plugins::utils::utils_revision::create_revision;
use crate::plugins::utils::utils_time::now;
use crate::rx_database::RxDatabase;
use crate::rx_error::{new_rx_error, RxError, RxResult};
use crate::rx_schema_helper::get_primary_field_of_primary_key;
use crate::types::{
    BulkWriteRow, RxJsonSchema, RxStorage, RxStorageBulkWriteResponse,
    RxStorageChangedDocumentsSinceResult, RxStorageCountResult, RxStorageInstance,
    RxStorageInstanceCreationParams, RxStorageQueryResult, RxStorageWriteError,
};

// ref: rxdb/src/rx-storage-helper.ts:52
pub const INTERNAL_STORAGE_NAME: &str = "_rxdb_internal";

// ref: rxdb/src/rx-storage-helper.ts:53
pub const RX_DATABASE_LOCAL_DOCS_STORAGE_NAME: &str = "rxdatabase_storage_local";

// ref: rxdb/src/rx-storage-helper.ts:55-66
pub async fn get_single_document(
    storage_instance: &dyn RxStorageInstance,
    document_id: &str,
) -> RxResult<Option<Value>> {
    let results = storage_instance
        .find_documents_by_id(&[document_id.to_string()], false)
        .await?;
    Ok(results.into_iter().next())
}

// ref: rxdb/src/rx-storage-helper.ts:68-90
/// Writes a single document, returning the written document on success.
/// Returns the first storage write error if any occurred.
pub async fn write_single(
    instance: &dyn RxStorageInstance,
    write_row: BulkWriteRow,
    context: &str,
) -> RxResult<Value> {
    let write_result = instance
        .bulk_write(vec![write_row.clone()], context)
        .await?;
    if let Some(first_err) = write_result.error.first() {
        return Err(new_rx_error(
            "STO13",
            Some(json!({
                "message": format!("write_single failed: status {}", first_err.status),
                "writeError": serde_json::to_value(first_err).unwrap_or(Value::Null),
            })),
        ));
    }
    let primary_path = get_primary_field_of_primary_key(&instance.schema().primary_key);
    let success = get_written_documents_from_bulk_write_response(
        &primary_path,
        &[write_row],
        &write_result,
        None,
    );
    success.into_iter().next().ok_or_else(|| {
        new_rx_error(
            "SNH",
            Some(json!({ "message": "write_single produced no success row" })),
        )
    })
}

// ref: rxdb/src/rx-storage-helper.ts:96-111
/// Observe the plain document data of a single document.
///
/// Upstream builds this with `changeStream().pipe(map/filter/startWith/switchMap)`.
/// Rust returns a boxed stream that first yields the current document, if it
/// exists, then yields later `documentData` values from matching change events.
pub fn observe_single(
    storage_instance: Arc<dyn RxStorageInstance>,
    document_id: impl Into<String>,
) -> crate::rxjs_compat::RxStream<Value> {
    let document_id = document_id.into();
    let first_storage = Arc::clone(&storage_instance);
    let first_id = document_id.clone();
    let first = stream::once(async move {
        get_single_document(first_storage.as_ref(), &first_id)
            .await
            .ok()
            .flatten()
    })
    .filter_map(|v| async move { v });

    let changes_id = document_id;
    let changes = storage_instance.change_stream().filter_map(move |bulk| {
        let id = changes_id.clone();
        async move {
            bulk.events
                .into_iter()
                .find(|ev| ev.document_id == id)
                .and_then(|ev| ev.document_data)
        }
    });

    Box::pin(first.chain(changes))
}

// ref: rxdb/src/rx-storage-helper.ts:114-127
/// Checkpoints must be stackable. Used by storages like the sharding plugin
/// where a checkpoint only represents the document state from some, but not
/// all, shards.
pub fn stack_checkpoints(checkpoints: &[Option<Value>]) -> Value {
    let mut out = serde_json::Map::new();
    for cp in checkpoints.iter().flatten() {
        if let Some(obj) = cp.as_object() {
            for (k, v) in obj.iter() {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    Value::Object(out)
}

// ref: rxdb/src/rx-storage-helper.ts:129-154
/// Throws an `RxError` mapped from a storage write error. Upstream returns
/// `void`; the Rust port returns `RxResult<()>` so the caller chains with `?`.
pub fn throw_if_is_storage_write_error(
    collection_name: &str,
    document_id: &str,
    write_data: &Value,
    error: Option<&RxStorageWriteError>,
) -> RxResult<()> {
    let Some(error) = error else {
        return Ok(());
    };
    if error.status == 409 {
        return Err(new_rx_error(
            "CONFLICT",
            Some(json!({
                "collection": collection_name,
                "id": document_id,
                "writeError": serde_json::to_value(error).unwrap_or(Value::Null),
                "data": write_data,
            })),
        ));
    } else if error.status == 422 {
        return Err(new_rx_error(
            "VD2",
            Some(json!({
                "collection": collection_name,
                "id": document_id,
                "writeError": serde_json::to_value(error).unwrap_or(Value::Null),
                "data": write_data,
            })),
        ));
    }
    Err(new_rx_error(
        "STO14",
        Some(json!({
            "status": error.status,
            "documentId": error.document_id,
        })),
    ))
}

// ref: rxdb/src/rx-storage-helper.ts:466-470
pub fn get_attachment_size(attachment_base64_string: &str) -> usize {
    // Decode base64 length; upstream is `atob(str).length`.
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(attachment_base64_string)
        .map(|v| v.len())
        .unwrap_or(0)
}

// ref: rxdb/src/rx-storage-helper.ts:472-486
/// For CTOX's MVP scope attachments are out-of-band; this is a near-pass-through.
pub fn attachment_write_data_to_normal_data(write_data: &Value) -> Value {
    if let Some(obj) = write_data.as_object() {
        if let Some(data) = obj.get("data").and_then(|v| v.as_str()) {
            return json!({
                "length": get_attachment_size(data),
                "digest": obj.get("digest").cloned().unwrap_or(Value::Null),
                "type": obj.get("type").cloned().unwrap_or(Value::Null),
            });
        }
    }
    write_data.clone()
}

// ref: rxdb/src/rx-storage-helper.ts:488-501
pub fn strip_attachments_data_from_document(doc: &Value) -> Value {
    let Some(obj) = doc.as_object() else {
        return doc.clone();
    };
    let attachments = obj.get("_attachments").and_then(|v| v.as_object());
    let needs_strip = attachments.map(|a| !a.is_empty()).unwrap_or(false);
    if !needs_strip {
        return doc.clone();
    }
    let mut out = obj.clone();
    let mut new_attachments = serde_json::Map::new();
    if let Some(att) = attachments {
        for (k, v) in att.iter() {
            new_attachments.insert(k.clone(), attachment_write_data_to_normal_data(v));
        }
    }
    out.insert("_attachments".to_string(), Value::Object(new_attachments));
    Value::Object(out)
}

// ref: rxdb/src/rx-storage-helper.ts:459-464
pub fn strip_attachments_data_from_row(write_row: &BulkWriteRow) -> BulkWriteRow {
    BulkWriteRow {
        previous: write_row.previous.clone(),
        document: strip_attachments_data_from_document(&write_row.document),
    }
}

// ref: rxdb/src/rx-storage-helper.ts:503-519
/// Flat clone the document data and also the `_meta` field. Used many times
/// when we want to change the meta during replication etc.
pub fn flat_clone_doc_with_meta(doc: &Value) -> Value {
    let mut copy = doc.clone();
    if let Some(obj) = copy.as_object_mut() {
        if let Some(meta) = obj.get("_meta").cloned() {
            obj.insert("_meta".to_string(), meta);
        }
    }
    copy
}

/// Result of [`get_wrapped_storage_instance`].
///
/// This mirrors upstream's `WrappedRxStorageInstance`: all storage access is
/// serialized through `database.lockedRun()`, bulk writes receive fresh `_rev`
/// and `_meta.lwt` values, and deleted-doc reinserts are retried with the
/// deleted in-storage document as `previous`.
pub struct DatabaseWrappedStorageInstance {
    database: Arc<RxDatabase>,
    inner: Arc<dyn RxStorageInstance>,
    schema: RxJsonSchema,
    primary_path: String,
}

impl DatabaseWrappedStorageInstance {
    pub fn original_storage_instance(&self) -> &Arc<dyn RxStorageInstance> {
        &self.inner
    }
}

// ref: rxdb/src/rx-storage-helper.ts:531-728
pub fn get_wrapped_storage_instance(
    database: Arc<RxDatabase>,
    storage_instance: Arc<dyn RxStorageInstance>,
    rx_json_schema: RxJsonSchema,
) -> Arc<DatabaseWrappedStorageInstance> {
    let primary_path = get_primary_field_of_primary_key(&storage_instance.schema().primary_key);
    let ret = Arc::new(DatabaseWrappedStorageInstance {
        database: Arc::clone(&database),
        inner: storage_instance,
        schema: rx_json_schema,
        primary_path,
    });
    database.register_storage_instance();
    ret
}

#[async_trait]
impl RxStorageInstance for DatabaseWrappedStorageInstance {
    fn database_name(&self) -> &str {
        self.inner.database_name()
    }

    fn collection_name(&self) -> &str {
        self.inner.collection_name()
    }

    fn schema(&self) -> &RxJsonSchema {
        &self.schema
    }

    async fn bulk_write(
        &self,
        rows: Vec<BulkWriteRow>,
        context: &str,
    ) -> Result<RxStorageBulkWriteResponse, RxError> {
        // INVARIANT (checkpoint safety): `_meta.lwt` stamping and the storage
        // commit happen ATOMICALLY under the same `locked_run` lock that also
        // serializes `get_changed_documents_since`. Stamping outside the lock
        // let a concurrent writer with a HIGHER lwt commit first; a pull
        // reading in that window advanced its checkpoint past the still
        // uncommitted lower-lwt rows, which were then invisible to checkpoint
        // iteration forever (observed as silently missing desktop_file_chunks
        // after workspace churn). Upstream JS is single-threaded, so it gets
        // this ordering for free; the Rust port must enforce it explicitly.
        let storage = Arc::clone(&self.inner);
        let database = Arc::clone(&self.database);
        let ctx = context.to_string();
        let (write_result, write_rows_for_retry) = self
            .database
            .locked_run(move || async move {
                let time = now();
                let mut to_storage_write_rows = Vec::with_capacity(rows.len());
                for write_row in rows.into_iter() {
                    let previous = write_row.previous;
                    let mut document = flat_clone_doc_with_meta(&write_row.document);
                    if let Some(obj) = document.as_object_mut() {
                        let meta = obj
                            .entry("_meta".to_string())
                            .or_insert_with(|| json!({ "lwt": time }));
                        if let Some(meta_obj) = meta.as_object_mut() {
                            meta_obj.insert("lwt".to_string(), json!(time));
                        } else {
                            *meta = json!({ "lwt": time });
                        }
                        let previous_rev = previous
                            .as_ref()
                            .and_then(|doc| doc.get("_rev"))
                            .and_then(|rev| rev.as_str())
                            .filter(|rev| !rev.is_empty());
                        obj.insert(
                            "_rev".to_string(),
                            Value::String(create_revision(&database.token, previous_rev)?),
                        );
                    }
                    to_storage_write_rows.push(BulkWriteRow { previous, document });
                }

                let mut hook_payload = json!({
                    "storageInstance": {
                        "databaseName": storage.database_name(),
                        "collectionName": storage.collection_name()
                    },
                    "rows": to_storage_write_rows
                });
                run_plugin_hooks("preStorageWrite", &mut hook_payload);
                let to_storage_write_rows: Vec<BulkWriteRow> = hook_payload
                    .get("rows")
                    .cloned()
                    .and_then(|rows| serde_json::from_value(rows).ok())
                    .unwrap_or_default();

                let write_rows_for_retry = to_storage_write_rows.clone();
                let result = storage.bulk_write(to_storage_write_rows, &ctx).await?;
                Ok::<_, RxError>((result, write_rows_for_retry))
            })
            .await?;

        let mut use_write_result = RxStorageBulkWriteResponse { error: Vec::new() };
        let mut re_insert_errors = Vec::new();
        for error in write_result.error.into_iter() {
            let is_deleted_reinsert = error.status == 409
                && error.write_row.previous.is_none()
                && !is_deleted_doc(&error.write_row.document)
                && error
                    .document_in_db
                    .as_ref()
                    .map(is_deleted_doc)
                    .unwrap_or(false);
            if is_deleted_reinsert {
                re_insert_errors.push(error);
            } else {
                use_write_result.error.push(error);
            }
        }

        if re_insert_errors.is_empty() {
            return Ok(use_write_result);
        }

        let mut re_insert_ids = HashSet::new();
        let mut re_inserts = Vec::with_capacity(re_insert_errors.len());
        for error in re_insert_errors.into_iter() {
            re_insert_ids.insert(error.document_id.clone());
            let previous = error.document_in_db.clone();
            let previous_rev = previous
                .as_ref()
                .and_then(|doc| doc.get("_rev"))
                .and_then(|rev| rev.as_str())
                .filter(|rev| !rev.is_empty());
            let mut document = error.write_row.document.clone();
            if let Some(obj) = document.as_object_mut() {
                obj.insert(
                    "_rev".to_string(),
                    Value::String(create_revision(&self.database.token, previous_rev)?),
                );
            }
            re_inserts.push(BulkWriteRow { previous, document });
        }

        let sub_rows_for_success = re_inserts.clone();
        let storage = Arc::clone(&self.inner);
        let ctx = context.to_string();
        let sub_result = self
            .database
            .locked_run(move || async move {
                // Same checkpoint-safety invariant as the first pass: the rows
                // still carry the lwt stamped during the first locked_run; by
                // now other writers may have committed higher lwts. Re-stamp
                // inside THIS lock so commit order keeps matching lwt order.
                let time = now();
                let re_inserts: Vec<BulkWriteRow> = re_inserts
                    .into_iter()
                    .map(|mut row| {
                        if let Some(meta) = row
                            .document
                            .as_object_mut()
                            .and_then(|obj| obj.get_mut("_meta"))
                            .and_then(|meta| meta.as_object_mut())
                        {
                            meta.insert("lwt".to_string(), json!(time));
                        }
                        row
                    })
                    .collect();
                storage.bulk_write(re_inserts, &ctx).await
            })
            .await?;
        use_write_result.error.extend(sub_result.error.clone());

        let _success = get_written_documents_from_bulk_write_response(
            &self.primary_path,
            &write_rows_for_retry,
            &use_write_result,
            Some(&re_insert_ids),
        );
        let _sub_success = get_written_documents_from_bulk_write_response(
            &self.primary_path,
            &sub_rows_for_success,
            &sub_result,
            None,
        );

        Ok(use_write_result)
    }

    async fn find_documents_by_id(
        &self,
        ids: &[String],
        with_deleted: bool,
    ) -> Result<Vec<Value>, RxError> {
        let storage = Arc::clone(&self.inner);
        let ids = ids.to_vec();
        self.database
            .locked_run(
                move || async move { storage.find_documents_by_id(&ids, with_deleted).await },
            )
            .await
    }

    async fn query(&self, prepared_query: &Value) -> Result<RxStorageQueryResult, RxError> {
        let storage = Arc::clone(&self.inner);
        let query = prepared_query.clone();
        self.database
            .locked_run(move || async move { storage.query(&query).await })
            .await
    }

    fn query_stream_into_blocking(
        &self,
        prepared_query: &Value,
        chunk_size: usize,
        on_batch: &mut (dyn FnMut(Vec<Value>) -> Result<bool, RxError> + Send),
    ) -> Option<Result<(), RxError>> {
        self.inner
            .query_stream_into_blocking(prepared_query, chunk_size, on_batch)
    }

    async fn count(&self, prepared_query: &Value) -> Result<RxStorageCountResult, RxError> {
        let storage = Arc::clone(&self.inner);
        let query = prepared_query.clone();
        self.database
            .locked_run(move || async move { storage.count(&query).await })
            .await
    }

    async fn get_changed_documents_since(
        &self,
        limit: u64,
        checkpoint: Option<&Value>,
    ) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
        let storage = Arc::clone(&self.inner);
        let checkpoint = checkpoint.cloned();
        self.database
            .locked_run(move || async move {
                storage
                    .get_changed_documents_since(limit, checkpoint.as_ref())
                    .await
            })
            .await
    }

    fn change_stream(&self) -> crate::rxjs_compat::RxStream<crate::types::EventBulk> {
        self.inner.change_stream()
    }

    async fn cleanup(&self, min_deleted_time: i64) -> Result<bool, RxError> {
        let storage = Arc::clone(&self.inner);
        self.database
            .locked_run(move || async move { storage.cleanup(min_deleted_time).await })
            .await
    }

    async fn remove(&self) -> Result<(), RxError> {
        self.database.unregister_storage_instance();
        let storage = Arc::clone(&self.inner);
        self.database
            .locked_run(move || async move { storage.remove().await })
            .await
    }

    async fn close(&self) -> Result<(), RxError> {
        self.database.unregister_storage_instance();
        let storage = Arc::clone(&self.inner);
        self.database
            .locked_run(move || async move { storage.close().await })
            .await
    }

    async fn get_attachment_data(
        &self,
        document_id: &str,
        attachment_id: &str,
        digest: &str,
    ) -> Result<String, RxError> {
        let storage = Arc::clone(&self.inner);
        let document_id = document_id.to_string();
        let attachment_id = attachment_id.to_string();
        let digest = digest.to_string();
        self.database
            .locked_run(move || async move {
                storage
                    .get_attachment_data(&document_id, &attachment_id, &digest)
                    .await
            })
            .await
    }

    fn underlying_persistent_storage(&self) -> Option<Arc<dyn RxStorageInstance>> {
        Some(Arc::clone(&self.inner))
    }
}

fn is_deleted_doc(doc: &Value) -> bool {
    doc.get("_deleted")
        .and_then(|deleted| deleted.as_bool())
        .unwrap_or(false)
}

// ref: rxdb/src/rx-storage-helper.ts:730-749
/// Each RxStorage implementation should run this method at the first step of
/// `createStorageInstance` to ensure that the configuration is correct.
pub fn ensure_rx_storage_instance_params_are_correct(
    params: &RxStorageInstanceCreationParams,
) -> RxResult<()> {
    if params.schema.key_compression {
        return Err(new_rx_error(
            "UT5",
            Some(json!({ "args": { "schemaKeyCompression": true } })),
        ));
    }
    if has_encryption(&params.schema) {
        return Err(new_rx_error(
            "UT6",
            Some(json!({ "args": { "schemaHasEncryption": true } })),
        ));
    }
    if params
        .schema
        .attachments
        .as_ref()
        .and_then(|attachments| attachments.compression.as_ref())
        .is_some()
    {
        return Err(new_rx_error(
            "UT7",
            Some(json!({ "args": { "schemaAttachmentsCompression": true } })),
        ));
    }
    Ok(())
}

// ref: rxdb/src/rx-storage-helper.ts:752-761
pub fn has_encryption(json_schema: &RxJsonSchema) -> bool {
    !json_schema.encrypted.is_empty()
        || json_schema
            .attachments
            .as_ref()
            .map(|attachments| attachments.encrypted)
            .unwrap_or(false)
}

// ref: rxdb/src/rx-storage-helper.ts:861-900
/// Returns the documents that were successfully written, excluding any whose
/// document id appears in `response.error` or in `reInsertIds`.
pub fn get_written_documents_from_bulk_write_response(
    primary_path: &str,
    write_rows: &[BulkWriteRow],
    response: &RxStorageBulkWriteResponse,
    re_insert_ids: Option<&std::collections::HashSet<String>>,
) -> Vec<Value> {
    let has_re_insert = re_insert_ids.is_some();
    let has_errors = !response.error.is_empty();
    let mut error_ids: std::collections::HashSet<String> =
        re_insert_ids.cloned().unwrap_or_default();
    for err in response.error.iter() {
        error_ids.insert(err.document_id.clone());
    }
    let mut ret = Vec::with_capacity(write_rows.len().saturating_sub(response.error.len()));
    if has_errors || has_re_insert {
        for row in write_rows.iter() {
            let id_value = row.document.get(primary_path);
            let id = id_value.and_then(|v| v.as_str()).unwrap_or_default();
            if !error_ids.contains(id) {
                ret.push(strip_attachments_data_from_document(&row.document));
            }
        }
    } else {
        for row in write_rows.iter() {
            ret.push(strip_attachments_data_from_document(&row.document));
        }
    }
    ret
}

/// Memoization map analogue of upstream's `BULK_WRITE_SUCCESS_MAP` WeakMap.
/// In Rust we don't have a WeakMap on plain values; callers that need
/// memoization should hold the result themselves. Provided as a marker that
/// the optimisation surface exists.
pub fn bulk_write_success_cache_capacity_hint() -> HashMap<String, Vec<Value>> {
    HashMap::new()
}

// ref: rxdb/src/rx-storage-helper.ts:165-457
/// Analyzes a list of `BulkWriteRow`s and determines which documents must be
/// inserted/updated and which cause conflicts.
///
pub fn categorize_bulk_write_rows(
    schema_has_attachments: bool,
    primary_path: &str,
    docs_in_db: &std::collections::HashMap<String, Value>,
    bulk_write_rows: &[crate::types::BulkWriteRow],
    context: &str,
) -> crate::types::CategorizeBulkWriteRowsOutput {
    categorize_bulk_write_rows_with_hooks(
        schema_has_attachments,
        primary_path,
        docs_in_db,
        bulk_write_rows,
        context,
        None,
        None,
    )
}

// ref: rxdb/src/rx-storage-helper.ts:165-457
/// Variant of [`categorize_bulk_write_rows`] that also accepts the upstream
/// `onInsert` / `onUpdate` callbacks used by some storages.
pub fn categorize_bulk_write_rows_with_hooks(
    schema_has_attachments: bool,
    primary_path: &str,
    docs_in_db: &std::collections::HashMap<String, Value>,
    bulk_write_rows: &[crate::types::BulkWriteRow],
    context: &str,
    on_insert: Option<&dyn Fn(&Value)>,
    on_update: Option<&dyn Fn(&Value)>,
) -> crate::types::CategorizeBulkWriteRowsOutput {
    use crate::plugins::utils::utils_string::random_token;
    use crate::types::{
        AttachmentEvent, BulkWriteRowProcessed, CategorizeBulkWriteRowsOutput, EventBulk,
        RxStorageChangeEvent, RxStorageWriteError,
    };

    let event_bulk_id = random_token(Some(10));
    let mut event_bulk = EventBulk {
        id: event_bulk_id,
        events: Vec::new(),
        checkpoint: None,
        context: Some(context.to_string()),
    };
    let mut bulk_insert_docs: Vec<BulkWriteRowProcessed> = Vec::new();
    let mut bulk_update_docs: Vec<BulkWriteRowProcessed> = Vec::new();
    let mut errors: Vec<RxStorageWriteError> = Vec::new();
    let mut attachments_add: Vec<AttachmentEvent> = Vec::new();
    let mut attachments_remove: Vec<AttachmentEvent> = Vec::new();
    let mut attachments_update: Vec<AttachmentEvent> = Vec::new();

    let has_docs_in_db = !docs_in_db.is_empty();
    let mut newest_row: Option<BulkWriteRowProcessed> = None;

    for write_row in bulk_write_rows.iter() {
        let document = &write_row.document;
        let previous = write_row.previous.as_ref();
        let doc_id = document
            .get(primary_path)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let document_deleted = document
            .get("_deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let previous_deleted = previous
            .and_then(|p| p.get("_deleted"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let document_in_db = if has_docs_in_db {
            docs_in_db.get(&doc_id)
        } else {
            None
        };
        let mut attachment_error: Option<RxStorageWriteError> = None;

        if document_in_db.is_none() {
            // Insert path.
            let inserted_is_deleted = document_deleted;
            if schema_has_attachments {
                for (attachment_id, attachment_data) in attachment_entries(document) {
                    if attachment_data.get("data").is_none() {
                        attachment_error = Some(RxStorageWriteError {
                            status: 510,
                            is_error: true,
                            document_id: doc_id.clone(),
                            write_row: write_row.clone(),
                            document_in_db: None,
                            validation_errors: Vec::new(),
                            schema: None,
                            attachment_id: Some(attachment_id),
                        });
                        break;
                    }
                    attachments_add.push(AttachmentEvent {
                        document_id: doc_id.clone(),
                        attachment_id,
                        attachment_data: serde_json::from_value(attachment_data.clone()).ok(),
                        digest: attachment_data
                            .get("digest")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    });
                }
            }

            if let Some(err) = attachment_error {
                errors.push(err);
            } else {
                let row = if schema_has_attachments {
                    strip_attachments_data_from_row(write_row)
                } else {
                    write_row.clone()
                };
                bulk_insert_docs.push(row.clone());
                if let Some(cb) = on_insert {
                    cb(document);
                }
                newest_row = Some(row);
            }

            if !inserted_is_deleted {
                let event = RxStorageChangeEvent {
                    operation: "INSERT".to_string(),
                    document_id: doc_id.clone(),
                    document_data: Some(if schema_has_attachments {
                        strip_attachments_data_from_document(document)
                    } else {
                        document.clone()
                    }),
                    previous_document_data: previous.map(|prev| {
                        if schema_has_attachments {
                            strip_attachments_data_from_document(prev)
                        } else {
                            prev.clone()
                        }
                    }),
                    is_local: false,
                };
                event_bulk.events.push(event);
            }
        } else {
            // Update path.
            let in_db = document_in_db.unwrap();
            let rev_in_db = in_db.get("_rev").and_then(|v| v.as_str()).unwrap_or("");
            let conflict = match previous {
                None => true,
                Some(prev) => {
                    let prev_rev = prev.get("_rev").and_then(|v| v.as_str()).unwrap_or("");
                    rev_in_db != prev_rev
                }
            };
            if conflict {
                errors.push(RxStorageWriteError {
                    status: 409,
                    is_error: true,
                    document_id: doc_id.clone(),
                    write_row: write_row.clone(),
                    document_in_db: Some(in_db.clone()),
                    validation_errors: Vec::new(),
                    schema: None,
                    attachment_id: None,
                });
                continue;
            }

            let updated_row = if schema_has_attachments {
                strip_attachments_data_from_row(write_row)
            } else {
                write_row.clone()
            };

            if schema_has_attachments {
                if document_deleted {
                    if let Some(prev) = previous {
                        for (attachment_id, attachment_data) in attachment_entries(prev) {
                            attachments_remove.push(AttachmentEvent {
                                document_id: doc_id.clone(),
                                attachment_id,
                                attachment_data: None,
                                digest: attachment_data
                                    .get("digest")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                            });
                        }
                    }
                } else {
                    for (attachment_id, attachment_data) in attachment_entries(document) {
                        let previous_attachment_data =
                            previous.and_then(|prev| attachment_by_id(prev, &attachment_id));
                        if previous_attachment_data.is_none()
                            && attachment_data.get("data").is_none()
                        {
                            attachment_error = Some(RxStorageWriteError {
                                status: 510,
                                is_error: true,
                                document_id: doc_id.clone(),
                                write_row: write_row.clone(),
                                document_in_db: Some(in_db.clone()),
                                validation_errors: Vec::new(),
                                schema: None,
                                attachment_id: Some(attachment_id),
                            });
                            break;
                        }
                    }
                    if attachment_error.is_none() {
                        for (attachment_id, attachment_data) in attachment_entries(document) {
                            let previous_attachment_data =
                                previous.and_then(|prev| attachment_by_id(prev, &attachment_id));
                            if previous_attachment_data.is_none() {
                                attachments_add.push(AttachmentEvent {
                                    document_id: doc_id.clone(),
                                    attachment_id,
                                    attachment_data: serde_json::from_value(
                                        attachment_data.clone(),
                                    )
                                    .ok(),
                                    digest: attachment_data
                                        .get("digest")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string(),
                                });
                            } else {
                                let new_digest =
                                    attachment_by_id(&updated_row.document, &attachment_id)
                                        .and_then(|v| v.get("digest"))
                                        .and_then(Value::as_str)
                                        .unwrap_or_default();
                                let previous_digest = previous_attachment_data
                                    .and_then(|v| v.get("digest"))
                                    .and_then(Value::as_str)
                                    .unwrap_or_default();
                                if attachment_data.get("data").is_some()
                                    && previous_digest != new_digest
                                {
                                    attachments_update.push(AttachmentEvent {
                                        document_id: doc_id.clone(),
                                        attachment_id,
                                        attachment_data: serde_json::from_value(
                                            attachment_data.clone(),
                                        )
                                        .ok(),
                                        digest: attachment_data
                                            .get("digest")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }

            if let Some(err) = attachment_error {
                errors.push(err);
            } else {
                bulk_update_docs.push(updated_row.clone());
                if let Some(cb) = on_update {
                    cb(document);
                }
                newest_row = Some(updated_row.clone());
            }

            let (operation, ev_doc, prev_doc): (&str, Option<Value>, Option<Value>) =
                if previous_deleted && !document_deleted {
                    (
                        "INSERT",
                        Some(if schema_has_attachments {
                            strip_attachments_data_from_document(document)
                        } else {
                            document.clone()
                        }),
                        None,
                    )
                } else if previous.is_some() && !previous_deleted && !document_deleted {
                    (
                        "UPDATE",
                        Some(if schema_has_attachments {
                            strip_attachments_data_from_document(document)
                        } else {
                            document.clone()
                        }),
                        previous.cloned(),
                    )
                } else if document_deleted {
                    ("DELETE", Some(document.clone()), previous.cloned())
                } else {
                    // SNH — should not happen in upstream either.
                    continue;
                };
            event_bulk.events.push(RxStorageChangeEvent {
                operation: operation.to_string(),
                document_id: doc_id,
                document_data: ev_doc,
                previous_document_data: prev_doc,
                is_local: false,
            });
        }
    }

    CategorizeBulkWriteRowsOutput {
        bulk_insert_docs,
        bulk_update_docs,
        newest_row,
        errors,
        event_bulk,
        attachments_add,
        attachments_remove,
        attachments_update,
    }
}

fn attachment_entries(doc: &Value) -> Vec<(String, Value)> {
    doc.get("_attachments")
        .and_then(Value::as_object)
        .map(|attachments| {
            attachments
                .iter()
                .map(|(id, data)| (id.clone(), data.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn attachment_by_id<'a>(doc: &'a Value, attachment_id: &str) -> Option<&'a Value> {
    doc.get("_attachments")
        .and_then(Value::as_object)
        .and_then(|attachments| attachments.get(attachment_id))
}

// ref: rxdb/src/rx-storage-helper.ts:763-808
/// Builds the `FilledMangoQuery` used to fetch changes since a checkpoint.
pub fn get_changed_documents_since_query(
    schema: &crate::types::RxJsonSchema,
    limit: u64,
    checkpoint: Option<&Value>,
) -> crate::types::FilledMangoQuery {
    use crate::plugins::utils::utils_document::RX_META_LWT_MINIMUM;
    use crate::rx_query_helper::normalize_mango_query;
    use crate::types::MangoQuery;

    let primary_path =
        crate::rx_schema_helper::get_primary_field_of_primary_key(&schema.primary_key);
    let since_lwt = checkpoint
        .and_then(|c| c.get("lwt"))
        .and_then(|v| v.as_f64())
        .unwrap_or(RX_META_LWT_MINIMUM as f64);
    let since_id = checkpoint
        .and_then(|c| c.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let selector = json!({
        "$or": [
            { "_meta.lwt": { "$gt": since_lwt } },
            {
                "_meta.lwt": { "$eq": since_lwt },
                primary_path.clone(): {
                    "$gt": if checkpoint.is_some() {
                        Value::String(since_id)
                    } else {
                        Value::String(String::new())
                    }
                }
            }
        ],
        "_meta.lwt": { "$gte": since_lwt }
    });
    let mut sort_meta = std::collections::HashMap::new();
    sort_meta.insert("_meta.lwt".to_string(), "asc".to_string());
    let mut sort_pk = std::collections::HashMap::new();
    sort_pk.insert(primary_path, "asc".to_string());
    let mango_query = MangoQuery {
        selector: Some(selector),
        sort: Some(vec![sort_meta, sort_pk]),
        index: None,
        limit: Some(limit),
        skip: Some(0),
    };
    normalize_mango_query(schema, mango_query)
}

// ref: rxdb/src/rx-storage-helper.ts:810-851
/// Fallback implementation of `getChangedDocumentsSince` for storages that
/// do not provide their own — uses `prepare_query` + `query()`.
pub async fn get_changed_documents_since_via_query(
    storage: &dyn crate::types::RxStorageInstance,
    limit: u64,
    checkpoint: Option<&Value>,
) -> RxResult<crate::types::RxStorageChangedDocumentsSinceResult> {
    use crate::rx_query_helper::prepare_query;
    let primary_path =
        crate::rx_schema_helper::get_primary_field_of_primary_key(&storage.schema().primary_key);
    let filled = get_changed_documents_since_query(storage.schema(), limit, checkpoint);
    let prepared = prepare_query(storage.schema(), filled)?;
    let result = storage.query(&prepared).await?;
    let documents = result.documents;
    let last_doc = documents.last().cloned();
    let new_checkpoint = match last_doc {
        Some(d) => json!({
            "id": d.get(&primary_path).cloned().unwrap_or(Value::Null),
            "lwt": d.get("_meta").and_then(|m| m.get("lwt")).cloned().unwrap_or(json!(0)),
        }),
        None => checkpoint
            .cloned()
            .unwrap_or_else(|| json!({ "id": "", "lwt": 0 })),
    };
    Ok(crate::types::RxStorageChangedDocumentsSinceResult {
        documents,
        checkpoint: new_checkpoint,
    })
}

// ref: rxdb/src/rx-storage-helper.ts:810-851
/// Upstream helper that delegates to a storage-specific
/// `getChangedDocumentsSince` when present, otherwise falls back to querying.
/// In this Rust trait the method is mandatory; storages that do not have a
/// native implementation can delegate back to
/// [`get_changed_documents_since_via_query`] in their trait impl.
pub async fn get_changed_documents_since(
    storage: &dyn crate::types::RxStorageInstance,
    limit: u64,
    checkpoint: Option<&Value>,
) -> RxResult<crate::types::RxStorageChangedDocumentsSinceResult> {
    storage.get_changed_documents_since(limit, checkpoint).await
}
// ref: rxdb/src/rx-storage-helper.ts:907-1014
/// Wraps a storage and simulates delays. Mostly used in tests.
pub fn random_delay_storage(
    storage: Arc<dyn RxStorage>,
    delay_time_before: Arc<dyn Fn() -> u64 + Send + Sync>,
    delay_time_after: Arc<dyn Fn() -> u64 + Send + Sync>,
) -> Arc<dyn RxStorage> {
    Arc::new(RandomDelayStorage {
        name: format!("random-delay-{}", storage.name()),
        storage,
        delay_time_before,
        delay_time_after,
    })
}

struct RandomDelayStorage {
    name: String,
    storage: Arc<dyn RxStorage>,
    delay_time_before: Arc<dyn Fn() -> u64 + Send + Sync>,
    delay_time_after: Arc<dyn Fn() -> u64 + Send + Sync>,
}

#[async_trait]
impl RxStorage for RandomDelayStorage {
    fn name(&self) -> &str {
        &self.name
    }

    async fn create_storage_instance(
        &self,
        params: RxStorageInstanceCreationParams,
    ) -> Result<Arc<dyn RxStorageInstance>, RxError> {
        sleep_millis((self.delay_time_before)()).await;
        let storage_instance = self.storage.create_storage_instance(params).await?;
        sleep_millis((self.delay_time_after)()).await;
        Ok(Arc::new(RandomDelayStorageInstance {
            inner: storage_instance,
            delay_time_before: Arc::clone(&self.delay_time_before),
            delay_time_after: Arc::clone(&self.delay_time_after),
            write_queue: TokioMutex::new(()),
        }))
    }
}

struct RandomDelayStorageInstance {
    inner: Arc<dyn RxStorageInstance>,
    delay_time_before: Arc<dyn Fn() -> u64 + Send + Sync>,
    delay_time_after: Arc<dyn Fn() -> u64 + Send + Sync>,
    /// Upstream chains `randomDelayStorageWriteQueue` to keep writes ordered.
    write_queue: TokioMutex<()>,
}

#[async_trait]
impl RxStorageInstance for RandomDelayStorageInstance {
    fn database_name(&self) -> &str {
        self.inner.database_name()
    }

    fn collection_name(&self) -> &str {
        self.inner.collection_name()
    }

    fn schema(&self) -> &RxJsonSchema {
        self.inner.schema()
    }

    async fn bulk_write(
        &self,
        document_writes: Vec<BulkWriteRow>,
        context: &str,
    ) -> Result<RxStorageBulkWriteResponse, RxError> {
        let _guard = self.write_queue.lock().await;
        sleep_millis((self.delay_time_before)()).await;
        let ret = self.inner.bulk_write(document_writes, context).await?;
        sleep_millis((self.delay_time_after)()).await;
        Ok(ret)
    }

    async fn find_documents_by_id(
        &self,
        ids: &[String],
        with_deleted: bool,
    ) -> Result<Vec<Value>, RxError> {
        sleep_millis((self.delay_time_before)()).await;
        let ret = self.inner.find_documents_by_id(ids, with_deleted).await?;
        sleep_millis((self.delay_time_after)()).await;
        Ok(ret)
    }

    async fn query(&self, prepared_query: &Value) -> Result<RxStorageQueryResult, RxError> {
        sleep_millis((self.delay_time_before)()).await;
        let ret = self.inner.query(prepared_query).await?;
        Ok(ret)
    }

    async fn count(&self, prepared_query: &Value) -> Result<RxStorageCountResult, RxError> {
        sleep_millis((self.delay_time_before)()).await;
        let ret = self.inner.count(prepared_query).await?;
        sleep_millis((self.delay_time_after)()).await;
        Ok(ret)
    }

    async fn get_changed_documents_since(
        &self,
        limit: u64,
        checkpoint: Option<&Value>,
    ) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
        sleep_millis((self.delay_time_before)()).await;
        let ret = self
            .inner
            .get_changed_documents_since(limit, checkpoint)
            .await?;
        sleep_millis((self.delay_time_after)()).await;
        Ok(ret)
    }

    fn change_stream(&self) -> crate::rxjs_compat::RxStream<crate::types::EventBulk> {
        self.inner.change_stream()
    }

    async fn cleanup(&self, min_deleted_time: i64) -> Result<bool, RxError> {
        sleep_millis((self.delay_time_before)()).await;
        let ret = self.inner.cleanup(min_deleted_time).await?;
        sleep_millis((self.delay_time_after)()).await;
        Ok(ret)
    }

    async fn remove(&self) -> Result<(), RxError> {
        sleep_millis((self.delay_time_before)()).await;
        let ret = self.inner.remove().await;
        sleep_millis((self.delay_time_after)()).await;
        ret
    }

    async fn close(&self) -> Result<(), RxError> {
        sleep_millis((self.delay_time_before)()).await;
        let ret = self.inner.close().await;
        sleep_millis((self.delay_time_after)()).await;
        ret
    }

    async fn get_attachment_data(
        &self,
        document_id: &str,
        attachment_id: &str,
        digest: &str,
    ) -> Result<String, RxError> {
        sleep_millis((self.delay_time_before)()).await;
        let ret = self
            .inner
            .get_attachment_data(document_id, attachment_id, digest)
            .await?;
        sleep_millis((self.delay_time_after)()).await;
        Ok(ret)
    }

    fn underlying_persistent_storage(&self) -> Option<Arc<dyn RxStorageInstance>> {
        self.inner.underlying_persistent_storage()
    }
}

async fn sleep_millis(ms: u64) {
    if ms > 0 {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

#[allow(dead_code)]
fn _phantom_use_error(_e: RxError) {}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::types::{
        HashFunction, HashOutput, JsonSchema, PrimaryKey, RxStorageInstanceCreationParams,
    };

    struct TestHashFunction;

    impl HashFunction for TestHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("hash:{input}") })
        }
    }

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
            indexes: Vec::new(),
            encrypted: Vec::new(),
            internal_indexes: Vec::new(),
            key_compression: false,
            attachments: None,
            additional_properties: true,
            extra: HashMap::new(),
        }
    }

    fn storage_params(schema: RxJsonSchema) -> RxStorageInstanceCreationParams {
        RxStorageInstanceCreationParams {
            database_instance_token: "db-token".to_string(),
            database_name: "db".to_string(),
            collection_name: "docs".to_string(),
            schema,
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: false,
            password: None,
        }
    }

    fn test_database(storage: Arc<dyn RxStorage>) -> Arc<RxDatabase> {
        RxDatabase::new(
            "db",
            "db-token",
            "storage-token",
            false,
            Arc::new(TestHashFunction),
            storage,
        )
    }

    fn base_doc(id: &str, rev: &str, deleted: bool, attachments: Value) -> Value {
        serde_json::json!({
            "id": id,
            "_rev": rev,
            "_deleted": deleted,
            "_meta": { "lwt": 1.0 },
            "_attachments": attachments
        })
    }

    #[test]
    fn strip_attachments_data_from_row_removes_inline_data() {
        let row = BulkWriteRow {
            previous: None,
            document: base_doc(
                "a",
                "1-token",
                false,
                serde_json::json!({
                    "file": {
                        "data": "aGVsbG8=",
                        "digest": "sha256-a",
                        "type": "text/plain"
                    }
                }),
            ),
        };

        let stripped = strip_attachments_data_from_row(&row);
        let attachment = stripped
            .document
            .get("_attachments")
            .and_then(|v| v.get("file"))
            .unwrap();
        assert!(attachment.get("data").is_none());
        assert_eq!(attachment.get("length").and_then(Value::as_u64), Some(5));
    }

    #[test]
    fn categorize_insert_reports_missing_attachment_data() {
        let row = BulkWriteRow {
            previous: None,
            document: base_doc(
                "a",
                "1-token",
                false,
                serde_json::json!({
                    "file": {
                        "digest": "sha256-a",
                        "type": "text/plain"
                    }
                }),
            ),
        };

        let categorized =
            categorize_bulk_write_rows(true, "id", &HashMap::new(), &[row], "attachment-test");

        assert_eq!(categorized.errors.len(), 1);
        assert_eq!(categorized.errors[0].status, 510);
        assert_eq!(categorized.errors[0].attachment_id.as_deref(), Some("file"));
        assert!(categorized.bulk_insert_docs.is_empty());
    }

    #[test]
    fn categorize_update_tracks_attachment_update() {
        let previous = base_doc(
            "a",
            "1-token",
            false,
            serde_json::json!({
                "file": {
                    "digest": "sha256-old",
                    "type": "text/plain",
                    "length": 3
                }
            }),
        );
        let next = base_doc(
            "a",
            "2-token",
            false,
            serde_json::json!({
                "file": {
                    "data": "aGVsbG8=",
                    "digest": "sha256-new",
                    "type": "text/plain"
                }
            }),
        );
        let row = BulkWriteRow {
            previous: Some(previous.clone()),
            document: next,
        };
        let docs_in_db = HashMap::from([(String::from("a"), previous)]);

        let categorized =
            categorize_bulk_write_rows(true, "id", &docs_in_db, &[row], "attachment-test");

        assert!(categorized.errors.is_empty());
        assert_eq!(categorized.attachments_update.len(), 1);
        assert_eq!(categorized.attachments_update[0].attachment_id, "file");
        assert_eq!(categorized.bulk_update_docs.len(), 1);
    }

    #[tokio::test]
    async fn wrapped_storage_sets_revision_lwt_and_tracks_instance() {
        let schema = test_schema();
        let storage: Arc<dyn RxStorage> = crate::plugins::storage_memory::get_rx_storage_memory(());
        let inner = storage
            .create_storage_instance(storage_params(schema.clone()))
            .await
            .unwrap();
        let database = test_database(storage);

        let wrapped =
            get_wrapped_storage_instance(Arc::clone(&database), Arc::clone(&inner), schema);
        assert_eq!(database.storage_instance_count(), 1);

        let result = wrapped
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: base_doc("doc-1", "0-old", false, serde_json::json!({})),
                }],
                "wrapped-test",
            )
            .await
            .unwrap();
        assert!(result.error.is_empty());

        let docs = inner
            .find_documents_by_id(&["doc-1".to_string()], true)
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].get("_rev").and_then(Value::as_str),
            Some("1-db-token")
        );
        assert!(
            docs[0]
                .get("_meta")
                .and_then(|meta| meta.get("lwt"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                > 0.0
        );

        wrapped.close().await.unwrap();
        assert_eq!(database.storage_instance_count(), 0);
    }

    #[tokio::test]
    async fn wrapped_storage_reinserts_deleted_documents() {
        let schema = test_schema();
        let storage: Arc<dyn RxStorage> = crate::plugins::storage_memory::get_rx_storage_memory(());
        let inner = storage
            .create_storage_instance(storage_params(schema.clone()))
            .await
            .unwrap();
        inner
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: base_doc("doc-1", "1-old", true, serde_json::json!({})),
                }],
                "seed-deleted",
            )
            .await
            .unwrap();

        let database = test_database(storage);
        let wrapped = get_wrapped_storage_instance(database, Arc::clone(&inner), schema);
        let result = wrapped
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: base_doc("doc-1", "0-old", false, serde_json::json!({})),
                }],
                "reinsert-test",
            )
            .await
            .unwrap();

        assert!(result.error.is_empty());
        let docs = inner
            .find_documents_by_id(&["doc-1".to_string()], false)
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].get("_deleted").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            docs[0].get("_rev").and_then(Value::as_str),
            Some("2-db-token")
        );
    }

    /// REGRESSION (checkpoint safety): `_meta.lwt` stamping and the storage
    /// commit must be atomic under `locked_run`. When stamping happened
    /// outside the lock, a writer could commit a HIGHER lwt before a
    /// concurrent writer's LOWER lwt landed; a checkpoint iterator reading in
    /// that window advanced past the uncommitted rows and never saw them
    /// (observed as silently missing desktop_file_chunks after workspace
    /// churn in the rxdb-soak E2E). Hammers concurrent single-row writers
    /// against a continuous checkpoint drain and asserts nothing is skipped.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn checkpoint_iteration_never_skips_docs_under_concurrent_writers() {
        const WRITERS: usize = 4;
        const DOCS_PER_WRITER: usize = 50;

        let schema = test_schema();
        let storage: Arc<dyn RxStorage> = crate::plugins::storage_memory::get_rx_storage_memory(());
        let inner = storage
            .create_storage_instance(storage_params(schema.clone()))
            .await
            .unwrap();
        let database = test_database(storage);
        let wrapped =
            get_wrapped_storage_instance(Arc::clone(&database), Arc::clone(&inner), schema);

        let mut writers = Vec::new();
        for writer in 0..WRITERS {
            let wrapped = Arc::clone(&wrapped);
            writers.push(tokio::spawn(async move {
                for doc in 0..DOCS_PER_WRITER {
                    let id = format!("w{writer}-d{doc:03}");
                    let result = wrapped
                        .bulk_write(
                            vec![BulkWriteRow {
                                previous: None,
                                document: base_doc(&id, "0-new", false, serde_json::json!({})),
                            }],
                            "checkpoint-race-test",
                        )
                        .await
                        .unwrap();
                    assert!(result.error.is_empty());
                    tokio::task::yield_now().await;
                }
            }));
        }

        // Drain checkpoint iteration continuously while the writers run. A
        // small limit forces many iterations so a reader interleaves with
        // writers as often as possible.
        let mut seen: HashSet<String> = HashSet::new();
        let mut checkpoint: Option<Value> = None;
        let total = WRITERS * DOCS_PER_WRITER;
        let mut writers_done = false;
        loop {
            let result = wrapped
                .get_changed_documents_since(7, checkpoint.as_ref())
                .await
                .unwrap();
            let empty = result.documents.is_empty();
            for doc in &result.documents {
                if let Some(id) = doc.get("id").and_then(Value::as_str) {
                    seen.insert(id.to_string());
                }
            }
            checkpoint = Some(result.checkpoint);
            if empty {
                if writers_done {
                    break;
                }
                writers_done = writers.iter().all(|writer| writer.is_finished());
                tokio::task::yield_now().await;
            }
        }
        for writer in writers {
            writer.await.unwrap();
        }

        assert_eq!(
            seen.len(),
            total,
            "checkpoint iteration skipped {} docs (lwt stamped outside locked_run?)",
            total - seen.len()
        );
        wrapped.close().await.unwrap();
    }

    fn schema(extra: serde_json::Map<String, Value>) -> RxJsonSchema {
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
            additional_properties: false,
            extra: extra.into_iter().collect(),
        }
    }

    #[test]
    fn has_encryption_detects_attachment_encryption() {
        let mut schema = schema(serde_json::Map::new());
        schema.attachments = Some(crate::types::RxJsonSchemaAttachments {
            encrypted: true,
            compression: None,
        });
        assert!(has_encryption(&schema));
    }

    #[test]
    fn ensure_params_rejects_attachment_compression() {
        let mut schema = schema(serde_json::Map::new());
        schema.attachments = Some(crate::types::RxJsonSchemaAttachments {
            encrypted: false,
            compression: Some("gzip".to_string()),
        });
        let params = RxStorageInstanceCreationParams {
            database_instance_token: "token".to_string(),
            database_name: "db".to_string(),
            collection_name: "col".to_string(),
            schema,
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: false,
            password: None,
        };

        let err = ensure_rx_storage_instance_params_are_correct(&params).unwrap_err();
        assert!(format!("{err}").contains("UT7"));
    }
}
