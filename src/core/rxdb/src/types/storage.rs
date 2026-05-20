//! Storage-layer types — port of the storage-related entries in
//! `rxdb/src/types/{rx-storage,rx-storage-bulk-write,rx-storage-instance}.d.ts`.
//!
//! T1 decision: document data is represented as `serde_json::Value` end-to-end
//! (see [`crate::types::document::RxDocumentData`]). Upstream generic
//! `<RxDocType>` is therefore elided here; users wanting strong typing
//! deserialize from `Value` at their own layer.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::rx_error::RxError;
use crate::rxjs_compat::RxStream;
use crate::types::{FilledMangoQuery, RxJsonSchema};

// ref: rxdb/src/types/rx-attachment.d.ts RxAttachmentData
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxAttachmentData {
    pub digest: String,
    pub length: u64,
    #[serde(rename = "type")]
    pub content_type: String,
}

// ref: rxdb/src/types/rx-attachment.d.ts RxAttachmentWriteData
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxAttachmentWriteData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    pub digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u64>,
    #[serde(rename = "type")]
    pub content_type: String,
}

// ref: rxdb/src/types/rx-storage.d.ts BulkWriteRow<RxDocType>
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BulkWriteRow {
    /// Previous document state (None for inserts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Value>,
    /// New document state to write.
    pub document: Value,
}

// ref: rxdb/src/types/rx-storage.d.ts RxStorageWriteErrorBase
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxStorageWriteError {
    /// HTTP-style status: 409 conflict, 422 validation, 510 attachment missing.
    pub status: u16,
    #[serde(rename = "isError", default = "default_true")]
    pub is_error: bool,
    #[serde(rename = "documentId")]
    pub document_id: String,
    #[serde(rename = "writeRow")]
    pub write_row: BulkWriteRow,
    /// Present only for conflict errors (status=409).
    #[serde(
        rename = "documentInDb",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub document_in_db: Option<Value>,
    /// Present only for validation errors (status=422).
    #[serde(
        rename = "validationErrors",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub validation_errors: Vec<Value>,
    /// Present for validation errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<RxJsonSchema>,
    /// Present for attachment errors (status=510).
    #[serde(
        rename = "attachmentId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub attachment_id: Option<String>,
}

fn default_true() -> bool {
    true
}

// ref: rxdb/src/types/rx-storage.d.ts RxStorageBulkWriteResponse<RxDocType>
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RxStorageBulkWriteResponse {
    /// Errors per failed write (success is implicit when no error for the document id).
    #[serde(default)]
    pub error: Vec<RxStorageWriteError>,
}

// ref: rxdb/src/types/rx-storage.d.ts RxStorageChangeEvent<RxDocType>
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RxStorageChangeEvent {
    /// One of "INSERT" | "UPDATE" | "DELETE".
    pub operation: String,
    #[serde(rename = "documentId")]
    pub document_id: String,
    #[serde(
        rename = "documentData",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub document_data: Option<Value>,
    #[serde(
        rename = "previousDocumentData",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_document_data: Option<Value>,
    #[serde(rename = "isLocal", default)]
    pub is_local: bool,
}

// ref: rxdb/src/types/rx-storage.d.ts BulkWriteRowProcessed<RxDocType>
//
// Same shape as `BulkWriteRow`; semantic distinction marks that attachments
// have been stripped to their normal-data form by `categorize_bulk_write_rows`.
pub type BulkWriteRowProcessed = BulkWriteRow;

// ref: rxdb/src/types/rx-storage.d.ts CategorizeBulkWriteRowsOutput<RxDocType>
#[derive(Debug, Clone, Default)]
pub struct CategorizeBulkWriteRowsOutput {
    pub bulk_insert_docs: Vec<BulkWriteRowProcessed>,
    pub bulk_update_docs: Vec<BulkWriteRowProcessed>,
    pub newest_row: Option<BulkWriteRowProcessed>,
    pub errors: Vec<RxStorageWriteError>,
    pub event_bulk: EventBulk,
    pub attachments_add: Vec<AttachmentEvent>,
    pub attachments_remove: Vec<AttachmentEvent>,
    pub attachments_update: Vec<AttachmentEvent>,
}

#[derive(Debug, Clone)]
pub struct AttachmentEvent {
    pub document_id: String,
    pub attachment_id: String,
    pub attachment_data: Option<RxAttachmentWriteData>,
    pub digest: String,
}

// ref: rxdb/src/types/rx-change-event.d.ts RxChangeEvent<DocType>
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxChangeEvent {
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    #[serde(rename = "documentId")]
    pub document_id: String,
    #[serde(rename = "isLocal", default)]
    pub is_local: bool,
    pub operation: String,
    #[serde(
        rename = "documentData",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub document_data: Option<Value>,
    #[serde(
        rename = "previousDocumentData",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_document_data: Option<Value>,
}

// ref: rxdb/src/types/rx-change-event.d.ts RxChangeEventBulk<DocType>
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxChangeEventBulk {
    pub id: String,
    pub events: Vec<RxStorageChangeEvent>,
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    #[serde(rename = "isLocal", default)]
    pub is_local: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

// ref: rxdb/src/types/rx-storage.d.ts EventBulk<EventType, CheckpointType>
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EventBulk {
    pub id: String,
    pub events: Vec<RxStorageChangeEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

// ref: rxdb/src/types/rx-storage.d.ts RxStorageQueryResult<RxDocType>
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RxStorageQueryResult {
    pub documents: Vec<Value>,
}

// ref: rxdb/src/types/rx-storage.d.ts RxStorageCountResult
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RxStorageCountResult {
    pub count: u64,
    /// "exact" | "estimated"
    pub mode: String,
}

// ref: rxdb/src/types/rx-storage.d.ts RxStorageChangedDocumentsSinceResult<RxDocType, Checkpoint>
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RxStorageChangedDocumentsSinceResult {
    pub documents: Vec<Value>,
    pub checkpoint: Value,
}

// ref: rxdb/src/types/rx-storage.d.ts RxStorageInstanceCreationParams<RxDocType, InstanceCreationOptions>
#[derive(Debug, Clone)]
pub struct RxStorageInstanceCreationParams {
    pub database_instance_token: String,
    pub database_name: String,
    pub collection_name: String,
    pub schema: RxJsonSchema,
    pub options: HashMap<String, Value>,
    pub multi_instance: bool,
    pub dev_mode: bool,
    pub password: Option<String>,
}

// ref: rxdb/src/types/rx-storage.d.ts RxStorage<Internals, InstanceCreationOptions>
//
// T1: trait-object form. A backend factory that creates storage instances.
// Upstream `RxStorage.createStorageInstance(params)` is one of two methods on
// the user-facing interface (the other being `name`/`statics`, which we elide
// for the stub-level surface CTOX needs to wire replication).
#[async_trait]
pub trait RxStorage: Send + Sync {
    /// Stable identifier — used in `assumedMasterStateKey` revision-key
    /// construction (see `replication_protocol::meta_instance`).
    fn name(&self) -> &str;

    async fn create_storage_instance(
        &self,
        params: RxStorageInstanceCreationParams,
    ) -> Result<std::sync::Arc<dyn RxStorageInstance>, RxError>;
}

// ref: rxdb/src/types/rx-storage-instance.d.ts RxStorageInstance<RxDocType, Internals, InstanceCreationOptions, CheckpointType>
//
// T1: trait-object form. Operations are `async fn` via `#[async_trait]`.
// Backends implement this; consumers (replication, RxCollection) hold
// `Arc<dyn RxStorageInstance>`.
#[async_trait]
pub trait RxStorageInstance: Send + Sync {
    fn database_name(&self) -> &str;
    fn collection_name(&self) -> &str;
    fn schema(&self) -> &RxJsonSchema;

    async fn bulk_write(
        &self,
        document_writes: Vec<BulkWriteRow>,
        context: &str,
    ) -> Result<RxStorageBulkWriteResponse, RxError>;

    async fn find_documents_by_id(
        &self,
        ids: &[String],
        with_deleted: bool,
    ) -> Result<Vec<Value>, RxError>;

    async fn query(&self, prepared_query: &Value) -> Result<RxStorageQueryResult, RxError>;

    async fn count(&self, prepared_query: &Value) -> Result<RxStorageCountResult, RxError>;

    async fn get_changed_documents_since(
        &self,
        limit: u64,
        checkpoint: Option<&Value>,
    ) -> Result<RxStorageChangedDocumentsSinceResult, RxError>;

    /// Emits a stream of `EventBulk` change notifications.
    fn change_stream(&self) -> RxStream<EventBulk>;

    async fn cleanup(&self, min_deleted_time: i64) -> Result<bool, RxError>;

    async fn remove(&self) -> Result<(), RxError>;

    async fn close(&self) -> Result<(), RxError>;

    /// Upstream `getAttachmentData(docId, attachmentId, digest)`. For CTOX
    /// attachments are out-of-band Parquet files; backends that do not support
    /// attachments may return [`RxError::Coded`] with code `STO12`.
    async fn get_attachment_data(
        &self,
        _document_id: &str,
        _attachment_id: &str,
        _digest: &str,
    ) -> Result<String, RxError> {
        Err(crate::rx_error::new_rx_error(
            "STO12",
            Some(serde_json::json!({
                "message": "attachments are not supported by this storage instance",
            })),
        ))
    }

    // ref: rxdb/src/types/rx-storage-instance.d.ts underlyingPersistentStorage
    /// For wrapping storage instances (validate/encryption/key-compression):
    /// return the inner instance one level down. `None` means this is the
    /// terminal (persistent) storage. Used by
    /// [`crate::replication_protocol::helper::get_underlying_persistent_storage`]
    /// to walk to the bottom of the wrapping chain.
    fn underlying_persistent_storage(&self) -> Option<std::sync::Arc<dyn RxStorageInstance>> {
        None
    }
}

// ref: rxdb/src/types/rx-storage.d.ts PreparedQuery<RxDocType>
//
// In upstream this is opaque per-backend. We carry it as a JSON Value with the
// normalized `FilledMangoQuery` embedded by convention.
pub type PreparedQuery = Value;

/// Helper: wrap a [`FilledMangoQuery`] as a [`PreparedQuery`].
pub fn prepare_query_value(q: &FilledMangoQuery) -> Value {
    serde_json::to_value(q).unwrap_or(Value::Null)
}
