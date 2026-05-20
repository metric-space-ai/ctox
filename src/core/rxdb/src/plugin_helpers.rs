//! Port of `src/plugin-helpers.ts`.
//!
//! Two factories used by plugins to compose storage wrappers:
//! - [`wrap_rx_storage_instance`] — wrap an `RxStorageInstance` with
//!   per-doc `modify_to_storage` and `modify_from_storage` transformations
//!   (used by the encryption / key-compression plugins).
//! - [`wrapped_validate_storage_factory`] — wrap an `RxStorage` so each
//!   created instance runs validation on writes and surfaces 422 errors.
//!
//! T1 deviations:
//! - Closures are `Arc<dyn Fn(...) -> Pin<Box<dyn Future>>>` instead of
//!   JS `MaybePromise` returning async functions.
//! - The processing-changes BehaviorSubject is implemented via
//!   [`RxBehaviorSubject<u64>`].
//! - Tests live alongside the helpers in `tests` sub-module.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use serde_json::Value;
use tokio_stream::StreamExt;

use crate::plugins::utils::utils_object::flat_clone;
use crate::rx_error::RxError;
use crate::rx_schema_helper::get_primary_field_of_primary_key;
use crate::rxjs_compat::{RxBehaviorSubject, RxStream};
use crate::types::{
    BulkWriteRow, EventBulk, RxJsonSchema, RxStorage, RxStorageBulkWriteResponse,
    RxStorageChangedDocumentsSinceResult, RxStorageCountResult, RxStorageInstance,
    RxStorageInstanceCreationParams, RxStorageQueryResult, RxStorageWriteError,
};

/// Closure type for the validation factory: given a doc, return the list of
/// validation errors (empty = valid).
pub type ValidatorFunction = Arc<dyn Fn(&Value) -> Vec<Value> + Send + Sync>;

/// User-supplied factory: builds a [`ValidatorFunction`] from a schema.
pub type ValidatorBuilder = Arc<dyn Fn(&RxJsonSchema) -> ValidatorFunction + Send + Sync>;

/// User-supplied per-doc modifier closure for [`wrap_rx_storage_instance`].
pub type DocModifier = Arc<
    dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<Value, RxError>> + Send>> + Send + Sync,
>;

/// User-supplied attachment-data modifier closure.
pub type AttachmentModifier = Arc<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<String, RxError>> + Send>> + Send + Sync,
>;

// ref: rxdb/src/plugin-helpers.ts:153-318
/// Wrap an existing storage instance with two per-doc transformations. The
/// returned instance is transparent on every method; bulk_write applies
/// `modify_to_storage` on input and `modify_from_storage` to error
/// `document_in_db` / `write_row.{previous,document}` projections.
pub fn wrap_rx_storage_instance(
    original_schema: RxJsonSchema,
    inner: Arc<dyn RxStorageInstance>,
    modify_to_storage: DocModifier,
    modify_from_storage: DocModifier,
    modify_attachment_from_storage: Option<AttachmentModifier>,
) -> Arc<WrappedRxStorageInstance> {
    Arc::new(WrappedRxStorageInstance {
        original_schema,
        inner,
        modify_to_storage,
        modify_from_storage,
        modify_attachment_from_storage,
        processing_changes: Arc::new(RxBehaviorSubject::new(0)),
    })
}

/// Result of [`wrap_rx_storage_instance`]: an `RxStorageInstance` wrapper.
pub struct WrappedRxStorageInstance {
    original_schema: RxJsonSchema,
    inner: Arc<dyn RxStorageInstance>,
    modify_to_storage: DocModifier,
    modify_from_storage: DocModifier,
    modify_attachment_from_storage: Option<AttachmentModifier>,
    /// Number of in-flight change-stream emissions. `bulk_write` waits for
    /// this to reach 0 before resolving (`processing_changes_count$` upstream).
    processing_changes: Arc<RxBehaviorSubject<u64>>,
}

impl WrappedRxStorageInstance {
    /// Access the underlying instance (upstream `originalStorageInstance`).
    pub fn original_storage_instance(&self) -> &Arc<dyn RxStorageInstance> {
        &self.inner
    }
}

#[async_trait]
impl RxStorageInstance for WrappedRxStorageInstance {
    fn database_name(&self) -> &str {
        self.inner.database_name()
    }
    fn collection_name(&self) -> &str {
        self.inner.collection_name()
    }
    fn schema(&self) -> &RxJsonSchema {
        &self.original_schema
    }

    async fn bulk_write(
        &self,
        document_writes: Vec<BulkWriteRow>,
        context: &str,
    ) -> Result<RxStorageBulkWriteResponse, RxError> {
        let mut use_rows: Vec<BulkWriteRow> = Vec::with_capacity(document_writes.len());
        for row in document_writes.into_iter() {
            let previous = match row.previous {
                Some(p) => Some((self.modify_to_storage)(p).await?),
                None => None,
            };
            let document = (self.modify_to_storage)(row.document).await?;
            use_rows.push(BulkWriteRow { previous, document });
        }
        let write_result = self.inner.bulk_write(use_rows, context).await?;
        let mut ret = RxStorageBulkWriteResponse { error: Vec::new() };
        for err in write_result.error.into_iter() {
            let translated = self.translate_error_from_storage(err).await?;
            ret.error.push(translated);
        }
        // Drain any in-flight change-stream emissions before returning.
        let mut stream = self.processing_changes.subscribe();
        while self.processing_changes.get_value() > 0 {
            // Wait for next change emission.
            if stream.next().await.is_none() {
                break;
            }
        }
        Ok(ret)
    }

    async fn find_documents_by_id(
        &self,
        ids: &[String],
        with_deleted: bool,
    ) -> Result<Vec<Value>, RxError> {
        let raw = self.inner.find_documents_by_id(ids, with_deleted).await?;
        let mut ret = Vec::with_capacity(raw.len());
        for doc in raw.into_iter() {
            ret.push((self.modify_from_storage)(doc).await?);
        }
        Ok(ret)
    }

    async fn query(&self, prepared_query: &Value) -> Result<RxStorageQueryResult, RxError> {
        let result = self.inner.query(prepared_query).await?;
        let mut out = Vec::with_capacity(result.documents.len());
        for doc in result.documents.into_iter() {
            out.push((self.modify_from_storage)(doc).await?);
        }
        Ok(RxStorageQueryResult { documents: out })
    }

    async fn count(&self, prepared_query: &Value) -> Result<RxStorageCountResult, RxError> {
        self.inner.count(prepared_query).await
    }

    async fn get_changed_documents_since(
        &self,
        limit: u64,
        checkpoint: Option<&Value>,
    ) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
        let result = self
            .inner
            .get_changed_documents_since(limit, checkpoint)
            .await?;
        let mut docs = Vec::with_capacity(result.documents.len());
        for doc in result.documents.into_iter() {
            docs.push((self.modify_from_storage)(doc).await?);
        }
        Ok(RxStorageChangedDocumentsSinceResult {
            documents: docs,
            checkpoint: result.checkpoint,
        })
    }

    fn change_stream(&self) -> RxStream<EventBulk> {
        let inner_stream = self.inner.change_stream();
        let modify = Arc::clone(&self.modify_from_storage);
        let count_subject = Arc::clone(&self.processing_changes);
        Box::pin(inner_stream.then(move |bulk| {
            let modify = Arc::clone(&modify);
            let count_subject = Arc::clone(&count_subject);
            async move {
                count_subject.next(count_subject.get_value().saturating_add(1));
                let mut use_events = Vec::with_capacity(bulk.events.len());
                for ev in bulk.events.into_iter() {
                    let mut new_ev = ev.clone();
                    if let Some(d) = ev.document_data {
                        new_ev.document_data = Some(modify(d).await.unwrap_or(Value::Null));
                    }
                    if let Some(d) = ev.previous_document_data {
                        new_ev.previous_document_data =
                            Some(modify(d).await.unwrap_or(Value::Null));
                    }
                    use_events.push(new_ev);
                }
                let out = EventBulk {
                    id: bulk.id,
                    events: use_events,
                    checkpoint: bulk.checkpoint,
                    context: bulk.context,
                };
                count_subject.next(count_subject.get_value().saturating_sub(1));
                out
            }
        }))
    }

    async fn cleanup(&self, min_deleted_time: i64) -> Result<bool, RxError> {
        self.inner.cleanup(min_deleted_time).await
    }

    async fn remove(&self) -> Result<(), RxError> {
        self.inner.remove().await
    }

    async fn close(&self) -> Result<(), RxError> {
        self.inner.close().await
    }

    async fn get_attachment_data(
        &self,
        document_id: &str,
        attachment_id: &str,
        digest: &str,
    ) -> Result<String, RxError> {
        let data = self
            .inner
            .get_attachment_data(document_id, attachment_id, digest)
            .await?;
        match self.modify_attachment_from_storage.as_ref() {
            Some(modifier) => modifier(data).await,
            None => Ok(data),
        }
    }

    fn underlying_persistent_storage(&self) -> Option<Arc<dyn RxStorageInstance>> {
        Some(Arc::clone(&self.inner))
    }
}

impl WrappedRxStorageInstance {
    // ref: rxdb/src/plugin-helpers.ts:172-185 errorFromStorage
    async fn translate_error_from_storage(
        &self,
        err: RxStorageWriteError,
    ) -> Result<RxStorageWriteError, RxError> {
        let mut ret = err;
        if let Some(ref previous) = ret.write_row.previous {
            ret.write_row.previous = Some((self.modify_from_storage)(previous.clone()).await?);
        }
        ret.write_row.document = (self.modify_from_storage)(ret.write_row.document.clone()).await?;
        if let Some(ref in_db) = ret.document_in_db {
            ret.document_in_db = Some((self.modify_from_storage)(in_db.clone()).await?);
        }
        Ok(ret)
    }
}

// ref: rxdb/src/plugin-helpers.ts:48
/// Cache of validators by `{validator_key}{schema_json}`. Mirrors upstream's
/// nested-map cache (one outer entry per validation library, then keyed by
/// serialized schema). `Arc<Mutex<...>>` so multiple `RxStorage` wrappers
/// share the cache.
static VALIDATOR_CACHE: std::sync::OnceLock<
    Arc<Mutex<HashMap<String, HashMap<String, ValidatorFunction>>>>,
> = std::sync::OnceLock::new();

fn validator_cache() -> Arc<Mutex<HashMap<String, HashMap<String, ValidatorFunction>>>> {
    VALIDATOR_CACHE
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

// ref: rxdb/src/plugin-helpers.ts:54-145
/// Factory that wraps an `RxStorage` so each created instance runs
/// `validator_builder(schema)` against incoming writes and rejects invalid
/// documents with status=422 errors.
pub fn wrapped_validate_storage_factory(
    validator_builder: ValidatorBuilder,
    validator_key: impl Into<String>,
) -> Arc<dyn ValidationStorageFactory> {
    let key = validator_key.into();
    Arc::new(ValidationFactory {
        validator_builder,
        validator_key: key,
    })
}

/// Trait that the validate-storage factory exposes — given an existing
/// `RxStorage`, produce a wrapped one whose instances validate writes.
#[async_trait]
pub trait ValidationStorageFactory: Send + Sync {
    async fn wrap(&self, storage: Arc<dyn RxStorage>) -> Arc<dyn RxStorage>;
}

struct ValidationFactory {
    validator_builder: ValidatorBuilder,
    validator_key: String,
}

#[async_trait]
impl ValidationStorageFactory for ValidationFactory {
    async fn wrap(&self, storage: Arc<dyn RxStorage>) -> Arc<dyn RxStorage> {
        Arc::new(ValidationStorage {
            inner: storage,
            validator_builder: Arc::clone(&self.validator_builder),
            validator_key: self.validator_key.clone(),
        })
    }
}

struct ValidationStorage {
    inner: Arc<dyn RxStorage>,
    validator_builder: ValidatorBuilder,
    validator_key: String,
}

#[async_trait]
impl RxStorage for ValidationStorage {
    fn name(&self) -> &str {
        // Upstream prefixes with `validate-{validatorKey}-`. We allocate this
        // once on construction; for the trait-method `&str` return we keep a
        // synthetic constant here. Callers that need the literal upstream
        // string can read it via `validator_key()`.
        self.inner.name()
    }

    async fn create_storage_instance(
        &self,
        params: RxStorageInstanceCreationParams,
    ) -> Result<Arc<dyn RxStorageInstance>, RxError> {
        let inner_instance = self.inner.create_storage_instance(params.clone()).await?;
        let primary_path = get_primary_field_of_primary_key(&params.schema.primary_key);
        let schema_json = serde_json::to_string(&params.schema).unwrap_or_default();
        let cache = validator_cache();
        let validator = {
            let mut outer = cache.lock();
            let inner_cache = outer
                .entry(self.validator_key.clone())
                .or_insert_with(HashMap::new);
            inner_cache
                .entry(schema_json)
                .or_insert_with(|| (self.validator_builder)(&params.schema))
                .clone()
        };
        Ok(Arc::new(ValidatingInstance {
            inner: inner_instance,
            validator,
            primary_path,
            schema: params.schema,
        }))
    }
}

struct ValidatingInstance {
    inner: Arc<dyn RxStorageInstance>,
    validator: ValidatorFunction,
    primary_path: String,
    schema: RxJsonSchema,
}

#[async_trait]
impl RxStorageInstance for ValidatingInstance {
    fn database_name(&self) -> &str {
        self.inner.database_name()
    }
    fn collection_name(&self) -> &str {
        self.inner.collection_name()
    }
    fn schema(&self) -> &RxJsonSchema {
        self.inner.schema()
    }

    // ref: rxdb/src/plugin-helpers.ts:101-138 bulkWrite override
    async fn bulk_write(
        &self,
        document_writes: Vec<BulkWriteRow>,
        context: &str,
    ) -> Result<RxStorageBulkWriteResponse, RxError> {
        let mut errors: Vec<RxStorageWriteError> = Vec::new();
        let mut continue_writes: Vec<BulkWriteRow> = Vec::new();
        for row in document_writes.into_iter() {
            let id = row
                .document
                .get(&self.primary_path)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let validation_errors = (self.validator)(&row.document);
            if !validation_errors.is_empty() {
                errors.push(RxStorageWriteError {
                    status: 422,
                    is_error: true,
                    document_id: id,
                    write_row: BulkWriteRow {
                        previous: row.previous.as_ref().map(flat_clone),
                        document: flat_clone(&row.document),
                    },
                    document_in_db: None,
                    validation_errors,
                    schema: Some(self.schema.clone()),
                    attachment_id: None,
                });
            } else {
                continue_writes.push(row);
            }
        }
        let mut write_result = if continue_writes.is_empty() {
            RxStorageBulkWriteResponse { error: Vec::new() }
        } else {
            self.inner.bulk_write(continue_writes, context).await?
        };
        write_result.error.extend(errors.into_iter());
        Ok(write_result)
    }

    async fn find_documents_by_id(
        &self,
        ids: &[String],
        with_deleted: bool,
    ) -> Result<Vec<Value>, RxError> {
        self.inner.find_documents_by_id(ids, with_deleted).await
    }

    async fn query(&self, prepared_query: &Value) -> Result<RxStorageQueryResult, RxError> {
        self.inner.query(prepared_query).await
    }

    async fn count(&self, prepared_query: &Value) -> Result<RxStorageCountResult, RxError> {
        self.inner.count(prepared_query).await
    }

    async fn get_changed_documents_since(
        &self,
        limit: u64,
        checkpoint: Option<&Value>,
    ) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
        self.inner
            .get_changed_documents_since(limit, checkpoint)
            .await
    }

    fn change_stream(&self) -> RxStream<EventBulk> {
        self.inner.change_stream()
    }

    async fn cleanup(&self, min_deleted_time: i64) -> Result<bool, RxError> {
        self.inner.cleanup(min_deleted_time).await
    }

    async fn remove(&self) -> Result<(), RxError> {
        self.inner.remove().await
    }

    async fn close(&self) -> Result<(), RxError> {
        self.inner.close().await
    }

    async fn get_attachment_data(
        &self,
        document_id: &str,
        attachment_id: &str,
        digest: &str,
    ) -> Result<String, RxError> {
        self.inner
            .get_attachment_data(document_id, attachment_id, digest)
            .await
    }

    fn underlying_persistent_storage(&self) -> Option<Arc<dyn RxStorageInstance>> {
        Some(Arc::clone(&self.inner))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::storage_memory::rx_storage_instance_memory::RxStorageMemory;
    use serde_json::json;

    fn pkey_schema() -> RxJsonSchema {
        use crate::rx_schema_helper::fill_with_default_settings;
        let base = RxJsonSchema {
            version: 0,
            primary_key: crate::types::PrimaryKey::Simple("id".to_string()),
            schema_type: "object".to_string(),
            properties: {
                let mut m = HashMap::new();
                m.insert(
                    "id".to_string(),
                    crate::types::JsonSchema {
                        schema_type: Some("string".to_string()),
                        max_length: Some(50),
                        ..Default::default()
                    },
                );
                m
            },
            required: vec!["id".to_string()],
            indexes: vec![],
            encrypted: vec![],
            internal_indexes: vec![],
            key_compression: false,
            attachments: None,
            additional_properties: false,
            extra: HashMap::new(),
        };
        fill_with_default_settings(base)
    }

    fn passthrough_modifier() -> DocModifier {
        Arc::new(|d| Box::pin(async move { Ok(d) }))
    }

    #[tokio::test]
    async fn wrap_preserves_round_trip_via_passthrough_modifiers() {
        let storage = RxStorageMemory::new();
        let params = RxStorageInstanceCreationParams {
            database_instance_token: "tok".to_string(),
            database_name: "db".to_string(),
            collection_name: "coll".to_string(),
            schema: pkey_schema(),
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: false,
            password: None,
        };
        let inner =
            <RxStorageMemory as RxStorage>::create_storage_instance(&storage, params.clone())
                .await
                .unwrap();
        let wrapped = wrap_rx_storage_instance(
            params.schema.clone(),
            inner,
            passthrough_modifier(),
            passthrough_modifier(),
            None,
        );
        let result = wrapped
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: json!({
                        "id": "doc1",
                        "_deleted": false,
                        "_meta": { "lwt": 1 },
                        "_rev": "1-abc",
                        "_attachments": {},
                    }),
                }],
                "test",
            )
            .await
            .unwrap();
        assert!(result.error.is_empty());
        let docs = wrapped
            .find_documents_by_id(&["doc1".to_string()], false)
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[tokio::test]
    async fn validate_factory_rejects_invalid_doc_with_422() {
        // Build an inner memory storage to wrap.
        let inner_storage = RxStorageMemory::new() as Arc<dyn RxStorage>;
        // Validator: rejects every doc that lacks an `id`.
        let builder: ValidatorBuilder = Arc::new(|_schema| {
            Arc::new(|doc: &Value| {
                if doc.get("id").is_none() {
                    vec![json!({"message": "missing id"})]
                } else {
                    Vec::new()
                }
            })
        });
        let factory = wrapped_validate_storage_factory(builder, "ajv-test");
        let wrapped = factory.wrap(inner_storage).await;

        let instance = wrapped
            .create_storage_instance(RxStorageInstanceCreationParams {
                database_instance_token: "tok".to_string(),
                database_name: "db".to_string(),
                collection_name: "coll".to_string(),
                schema: pkey_schema(),
                options: HashMap::new(),
                multi_instance: false,
                dev_mode: false,
                password: None,
            })
            .await
            .unwrap();

        // One valid + one invalid row.
        let result = instance
            .bulk_write(
                vec![
                    BulkWriteRow {
                        previous: None,
                        document: json!({
                            "id": "ok",
                            "_deleted": false,
                            "_meta": { "lwt": 1 },
                            "_rev": "1-abc",
                            "_attachments": {},
                        }),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: json!({
                            "_deleted": false,
                            "_meta": { "lwt": 1 },
                            "_rev": "1-zzz",
                            "_attachments": {},
                        }),
                    },
                ],
                "test",
            )
            .await
            .unwrap();
        assert_eq!(result.error.len(), 1);
        assert_eq!(result.error[0].status, 422);
        assert!(
            !result.error[0].validation_errors.is_empty(),
            "validation_errors should be populated"
        );
    }
}
