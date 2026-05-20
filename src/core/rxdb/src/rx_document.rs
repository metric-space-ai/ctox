//! Port of `src/rx-document.ts`.
//!
//! This is a file-by-file porting pass, not the final ergonomic Rust API.
//! Prototype getters from upstream become inherent methods, plugin-overwritten
//! methods return the same "plugin missing" class of error, and collection-level
//! behaviour is wired only where the current `RxCollection` surface exists.

use std::collections::HashMap;
use std::sync::Arc;

use futures::future::{self, BoxFuture};
use futures::{stream, StreamExt};
use parking_lot::Mutex;
use serde_json::Value;

use crate::hooks::run_plugin_hooks;
use crate::incremental_write::modifier_from_public_to_internal;
use crate::overwritable::OVERWRITABLE;
use crate::plugins::utils::utils_error::plugin_missing;
use crate::plugins::utils::utils_object::{clone_deep, flat_clone};
use crate::plugins::utils::utils_object_dot_prop::get_property;
use crate::rx_change_event::get_document_data_of_rx_change_event;
use crate::rx_collection::RxCollection;
use crate::rx_error::{new_rx_error, RxResult};
use crate::rx_schema_helper::{get_schema_by_object_path, has_schema_object_path};
use crate::rx_storage_helper::{
    get_written_documents_from_bulk_write_response, throw_if_is_storage_write_error,
};
use crate::rxjs_compat::{reactive_from_stream, RxBehaviorSubject, RxStream};
use crate::types::{
    BulkWriteRow, MangoQuery, RxDocumentData, RxDocumentWriteData, RxStorageChangeEvent,
};

/// Public modifier callback used by `modify`/`incremental_modify`.
pub type ModifyFunction = Box<dyn FnOnce(Value) -> BoxFuture<'static, RxResult<Value>> + Send>;

// ref: rxdb/src/rx-document.ts:44-406
/// Rust counterpart to upstream `basePrototype` methods.
pub struct RxDocument {
    pub collection: Option<Arc<RxCollection>>,
    pub data: Mutex<RxDocumentData>,
    pub property_cache: Mutex<HashMap<String, Value>>,
    pub is_instance_of_rx_document: bool,
}

impl RxDocument {
    // ref: rxdb/src/rx-document.ts:408-426
    pub fn new(collection: Arc<RxCollection>, doc_data: RxDocumentData) -> Arc<Self> {
        Arc::new(Self {
            collection: Some(collection),
            data: Mutex::new(doc_data),
            property_cache: Mutex::new(HashMap::new()),
            is_instance_of_rx_document: true,
        })
    }

    /// Test/helper constructor for methods that do not need collection access.
    pub fn new_detached(doc_data: RxDocumentData) -> Arc<Self> {
        Arc::new(Self {
            collection: None,
            data: Mutex::new(doc_data),
            property_cache: Mutex::new(HashMap::new()),
            is_instance_of_rx_document: true,
        })
    }

    // ref: rxdb/src/rx-document.ts:45-51
    pub fn primary_path(&self) -> RxResult<String> {
        self.collection_ref()?.primary_path().ok_or_else(|| {
            new_rx_error(
                "DOC_PRIMARY_PATH",
                Some(serde_json::json!({ "message": "collection schema not wired" })),
            )
        })
    }

    // ref: rxdb/src/rx-document.ts:52-58
    pub fn primary(&self) -> RxResult<String> {
        let primary_path = self.primary_path()?;
        let data = self.data.lock();
        document_id_from_primary(&data, &primary_path)
    }

    // ref: rxdb/src/rx-document.ts:59-65
    pub fn revision(&self) -> Option<String> {
        self.data
            .lock()
            .get("_rev")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    }

    // ref: rxdb/src/rx-document.ts:84-90
    pub fn deleted(&self) -> bool {
        self.data
            .lock()
            .get("_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    // ref: rxdb/src/rx-document.ts:92-95
    pub fn get_latest(self: &Arc<Self>) -> RxResult<Arc<RxDocument>> {
        let collection = self.collection_ref()?;
        let doc_cache = collection.doc_cache()?;
        let latest_doc_data = doc_cache.get_latest_document_data(&self.primary()?)?;
        doc_cache.get_cached_rx_document(&latest_doc_data)
    }

    pub fn change_events(&self) -> RxResult<RxStream<RxStorageChangeEvent>> {
        let document_id = self.primary()?;
        let collection = Arc::clone(self.collection_ref()?);
        Ok(Box::pin(collection.change_events().filter(move |event| {
            future::ready(event.document_id == document_id)
        })))
    }

    // ref: rxdb/src/rx-document.ts:99-121 `$`
    pub fn document_stream(self: &Arc<Self>) -> RxResult<RxStream<Arc<RxDocument>>> {
        let document_id = self.primary()?;
        let collection = Arc::clone(self.collection_ref()?);
        let current = self.get_latest()?;
        let changes = collection
            .change_events()
            .filter(move |event| future::ready(event.document_id == document_id))
            .filter_map(move |event| {
                let collection = Arc::clone(&collection);
                async move {
                    let document_data = get_document_data_of_rx_change_event(&event);
                    collection
                        .doc_cache()
                        .ok()
                        .and_then(|cache| cache.get_cached_rx_document(&document_data).ok())
                }
            });
        let last_revision = Arc::new(Mutex::new(None::<Option<String>>));
        let stream = stream::once(future::ready(current)).chain(changes);
        Ok(Box::pin(stream.filter_map(move |document| {
            let last_revision = Arc::clone(&last_revision);
            async move {
                let revision = document.revision();
                let mut last = last_revision.lock();
                if last.as_ref() == Some(&revision) {
                    None
                } else {
                    *last = Some(revision);
                    Some(document)
                }
            }
        })))
    }

    pub fn deleted_stream(self: &Arc<Self>) -> RxResult<RxStream<bool>> {
        Ok(Box::pin(self.document_stream()?.map(|document| {
            document
                .data
                .lock()
                .get("_deleted")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })))
    }

    // ref: rxdb/src/rx-document.ts:75-82 deleted$$
    pub fn deleted_double_dollar(self: &Arc<Self>) -> RxResult<RxBehaviorSubject<bool>> {
        Ok(reactive_from_stream(
            self.get_latest()?.deleted(),
            Box::pin(self.deleted_stream()?.skip(1)),
        ))
    }

    // ref: rxdb/src/rx-document.ts:115-122 $$
    pub fn double_dollar(self: &Arc<Self>) -> RxResult<RxBehaviorSubject<Arc<RxDocument>>> {
        Ok(reactive_from_stream(
            self.get_latest()?,
            Box::pin(self.document_stream()?.skip(1)),
        ))
    }

    // ref: rxdb/src/rx-document.ts:123-162
    /// `get$` is reactive upstream. The file-by-file port exposes the current
    /// value so `get$$`/reactivity can be wired later without changing path
    /// validation behaviour.
    pub fn get_dollar(&self, path: &str) -> RxResult<Value> {
        self.validate_observable_path(path)?;
        self.get(path)
    }

    // ref: rxdb/src/rx-document.ts:164-173
    pub fn get_double_dollar(self: &Arc<Self>, path: &str) -> RxResult<RxBehaviorSubject<Value>> {
        let latest = self.get_latest()?.get(path)?;
        Ok(reactive_from_stream(
            latest,
            Box::pin(self.get_stream(path)?.skip(1)),
        ))
    }

    // ref: rxdb/src/rx-document.ts:127-159 get$
    pub fn get_stream(self: &Arc<Self>, path: &str) -> RxResult<RxStream<Value>> {
        self.validate_observable_path(path)?;
        let path = path.to_string();
        let last_value = Arc::new(Mutex::new(None::<Value>));
        Ok(Box::pin(
            self.document_stream()?
                .map(move |document| document.get(&path).unwrap_or(Value::Null))
                .filter_map(move |value| {
                    let last_value = Arc::clone(&last_value);
                    async move {
                        let mut last = last_value.lock();
                        if last.as_ref() == Some(&value) {
                            None
                        } else {
                            *last = Some(value.clone());
                            Some(value)
                        }
                    }
                }),
        ))
    }

    // ref: rxdb/src/rx-document.ts:178-215
    pub async fn populate(&self, path: &str) -> RxResult<Option<Value>> {
        let collection = self.collection_ref()?;
        let schema = collection.schema_required()?;
        let schema_obj = get_schema_by_object_path(&schema.json_schema, path);
        let value = self.get(path)?;
        if value.is_null() {
            return Ok(None);
        }
        if schema_obj.schema_type.is_none()
            && schema_obj.properties.is_empty()
            && schema_obj.extra.is_empty()
        {
            return Err(new_rx_error(
                "DOC5",
                Some(serde_json::json!({ "path": path })),
            ));
        }
        let ref_name = schema_obj
            .extra
            .get("ref")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                new_rx_error(
                    "DOC6",
                    Some(serde_json::json!({
                        "path": path,
                        "schemaObj": serde_json::to_value(&schema_obj).unwrap_or(Value::Null),
                    })),
                )
            })?;
        let ref_collection = collection.database.collection(ref_name).ok_or_else(|| {
            new_rx_error(
                "DOC7",
                Some(serde_json::json!({
                    "ref": ref_name,
                    "path": path,
                    "schemaObj": serde_json::to_value(&schema_obj).unwrap_or(Value::Null),
                })),
            )
        })?;

        if schema_obj.schema_type.as_deref() == Some("array") {
            let ids: Vec<String> = value
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| {
                            value
                                .as_str()
                                .map(ToString::to_string)
                                .or_else(|| Some(value.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let docs = ref_collection.find_by_ids(ids.clone())?.exec(false).await?;
            let Some(map) = docs.as_object() else {
                return Ok(Some(Value::Array(Vec::new())));
            };
            let ordered = ids.iter().filter_map(|id| map.get(id).cloned()).collect();
            Ok(Some(Value::Array(ordered)))
        } else {
            let primary_path = ref_collection
                .primary_path()
                .unwrap_or_else(|| "id".to_string());
            let query = MangoQuery {
                selector: Some(serde_json::json!({ primary_path: { "$eq": value } })),
                ..Default::default()
            };
            let doc = ref_collection.find_one(Some(query))?.exec(false).await?;
            if doc.is_null() {
                Ok(None)
            } else {
                Ok(Some(doc))
            }
        }
    }

    // ref: rxdb/src/rx-document.ts:217-224
    pub fn get(&self, obj_path: &str) -> RxResult<Value> {
        Ok(get_document_property(self, obj_path))
    }

    // ref: rxdb/src/rx-document.ts:226-239
    pub fn to_json(&self, with_meta_fields: bool) -> Value {
        let data = self.data.lock().clone();
        if with_meta_fields {
            return (OVERWRITABLE.load().deep_freeze_when_dev_mode)(data);
        }
        let mut data = flat_clone(&data);
        if let Some(obj) = data.as_object_mut() {
            obj.remove("_rev");
            obj.remove("_attachments");
            obj.remove("_deleted");
            obj.remove("_meta");
        }
        (OVERWRITABLE.load().deep_freeze_when_dev_mode)(data)
    }

    // ref: rxdb/src/rx-document.ts:240-242
    pub fn to_mutable_json(&self, with_meta_fields: bool) -> Value {
        clone_deep(&self.to_json(with_meta_fields))
    }

    // ref: rxdb/src/rx-document.ts:249-270
    pub fn update(&self, _update_obj: Value) -> RxResult<()> {
        Err(plugin_missing_rx_error("update"))
    }

    pub fn incremental_update(&self, _update_obj: Value) -> RxResult<()> {
        Err(plugin_missing_rx_error("update"))
    }

    pub fn update_crdt(&self, _update_obj: Value) -> RxResult<()> {
        Err(plugin_missing_rx_error("crdt"))
    }

    pub fn put_attachment(&self) -> RxResult<()> {
        Err(plugin_missing_rx_error("attachments"))
    }

    pub fn put_attachment_base64(&self) -> RxResult<()> {
        Err(plugin_missing_rx_error("attachments"))
    }

    pub fn get_attachment(&self) -> RxResult<()> {
        Err(plugin_missing_rx_error("attachments"))
    }

    pub fn all_attachments(&self) -> RxResult<()> {
        Err(plugin_missing_rx_error("attachments"))
    }

    // ref: rxdb/src/rx-document.ts:272-281
    pub async fn modify(
        self: &Arc<Self>,
        mutation_function: ModifyFunction,
    ) -> RxResult<Arc<Self>> {
        let old_data = self.data.lock().clone();
        let new_data =
            modifier_from_public_to_internal(mutation_function)(old_data.clone()).await?;
        self.save_data(new_data, old_data).await
    }

    // ref: rxdb/src/rx-document.ts:286-297
    pub async fn incremental_modify(
        self: &Arc<Self>,
        mutation_function: ModifyFunction,
    ) -> RxResult<Arc<Self>> {
        let collection = self.collection_ref()?;
        let queue = collection.incremental_write_queue()?;
        let result = queue
            .add_write(
                self.data.lock().clone(),
                modifier_from_public_to_internal(mutation_function),
            )
            .await?;
        self.replace_data(result.clone());
        collection.doc_cache()?.get_cached_rx_document(&result)
    }

    // ref: rxdb/src/rx-document.ts:300-313
    pub async fn patch(self: &Arc<Self>, patch: Value) -> RxResult<Arc<Self>> {
        let old_data = self.data.lock().clone();
        let mut new_data = clone_deep(&old_data);
        if let (Some(new_obj), Some(patch_obj)) = (new_data.as_object_mut(), patch.as_object()) {
            for (k, v) in patch_obj {
                new_obj.insert(k.clone(), v.clone());
            }
        }
        self.save_data(new_data, old_data).await
    }

    // ref: rxdb/src/rx-document.ts:318-331
    pub async fn incremental_patch(self: &Arc<Self>, patch: Value) -> RxResult<Arc<Self>> {
        self.incremental_modify(Box::new(move |mut doc_data| {
            Box::pin(async move {
                if let (Some(doc_obj), Some(patch_obj)) =
                    (doc_data.as_object_mut(), patch.as_object())
                {
                    for (k, v) in patch_obj {
                        doc_obj.insert(k.clone(), v.clone());
                    }
                }
                Ok(doc_data)
            })
        }))
        .await
    }

    // ref: rxdb/src/rx-document.ts:336-367
    pub async fn save_data(
        self: &Arc<Self>,
        mut new_data: RxDocumentWriteData,
        old_data: RxDocumentData,
    ) -> RxResult<Arc<Self>> {
        new_data = flat_clone(&new_data);
        if self.deleted() {
            return Err(new_rx_error(
                "DOC11",
                Some(serde_json::json!({
                    "id": self.primary().unwrap_or_default(),
                    "document": self.to_json(true),
                })),
            ));
        }

        let collection = self.collection_ref()?;
        before_document_update_write(collection, &mut new_data, &old_data).await?;
        if collection.has_hooks("pre", "save") {
            new_data = collection
                .run_hooks("pre", "save", new_data, Some(Arc::clone(self)))
                .await?;
        }

        let write_rows = vec![BulkWriteRow {
            previous: Some(old_data),
            document: new_data.clone(),
        }];
        let write_result = collection
            .storage_instance
            .bulk_write(write_rows.clone(), "rx-document-save-data")
            .await?;
        throw_if_is_storage_write_error(
            &collection.name,
            &self.primary().unwrap_or_default(),
            &new_data,
            write_result.error.first(),
        )?;

        let primary_path = collection.primary_path().ok_or_else(|| {
            new_rx_error(
                "DOC_PRIMARY_PATH",
                Some(serde_json::json!({ "collection": collection.name })),
            )
        })?;
        let written = get_written_documents_from_bulk_write_response(
            &primary_path,
            &write_rows,
            &write_result,
            None,
        );
        let primary = document_id_from_primary(&new_data, &primary_path)?;
        let persisted = collection
            .storage_instance
            .find_documents_by_id(std::slice::from_ref(&primary), true)
            .await?
            .into_iter()
            .next()
            .or_else(|| written.first().cloned())
            .unwrap_or(new_data);
        self.replace_data(persisted.clone());
        if collection.has_hooks("post", "save") {
            collection
                .run_hooks("post", "save", persisted.clone(), Some(Arc::clone(self)))
                .await?;
        }
        collection.doc_cache()?.get_cached_rx_document(&persisted)
    }

    // ref: rxdb/src/rx-document.ts:373-392
    pub async fn remove(self: &Arc<Self>) -> RxResult<Arc<Self>> {
        if self.deleted() {
            return Err(new_rx_error(
                "DOC13",
                Some(serde_json::json!({
                    "id": self.primary().unwrap_or_default(),
                    "document": self.to_json(true),
                })),
            ));
        }
        self.incremental_remove().await
    }

    // ref: rxdb/src/rx-document.ts:393-401
    pub async fn incremental_remove(self: &Arc<Self>) -> RxResult<Arc<Self>> {
        self.incremental_modify(Box::new(|mut doc_data| {
            Box::pin(async move {
                if let Some(obj) = doc_data.as_object_mut() {
                    obj.insert("_deleted".to_string(), Value::Bool(true));
                }
                Ok(doc_data)
            })
        }))
        .await
    }

    // ref: rxdb/src/rx-document.ts:402-404
    pub fn close(&self) -> RxResult<()> {
        Err(new_rx_error("DOC14", None))
    }

    fn collection_ref(&self) -> RxResult<&Arc<RxCollection>> {
        self.collection.as_ref().ok_or_else(|| {
            new_rx_error(
                "DOC_COLLECTION",
                Some(serde_json::json!({ "message": "document has no collection" })),
            )
        })
    }

    pub(crate) fn replace_data(&self, new_data: RxDocumentData) {
        *self.data.lock() = new_data;
        self.property_cache.lock().clear();
    }

    fn validate_observable_path(&self, path: &str) -> RxResult<()> {
        if !(OVERWRITABLE.load().is_dev_mode)() {
            return Ok(());
        }
        if path.contains(".item.") {
            return Err(new_rx_error(
                "DOC1",
                Some(serde_json::json!({ "path": path })),
            ));
        }
        if path == self.primary_path().unwrap_or_default() {
            return Err(new_rx_error("DOC2", None));
        }
        let collection = self.collection_ref()?;
        if let Some(schema) = &collection.schema {
            if schema.final_fields.iter().any(|field| field == path) {
                return Err(new_rx_error(
                    "DOC3",
                    Some(serde_json::json!({ "path": path })),
                ));
            }
            if !has_schema_object_path(&schema.json_schema, path) {
                return Err(new_rx_error(
                    "DOC4",
                    Some(serde_json::json!({ "path": path })),
                ));
            }
        }
        Ok(())
    }
}

// ref: rxdb/src/rx-document.ts:408-426
#[derive(Clone, Default)]
pub struct RxDocumentConstructor;

impl RxDocumentConstructor {
    pub fn construct(
        &self,
        collection: Arc<RxCollection>,
        doc_data: RxDocumentData,
    ) -> Arc<RxDocument> {
        RxDocument::new(collection, doc_data)
    }
}

// ref: rxdb/src/rx-document.ts:408-427
pub fn create_rx_document_constructor() -> RxDocumentConstructor {
    RxDocumentConstructor
}

// ref: rxdb/src/rx-document.ts:430-438
pub fn create_with_constructor(
    constructor: &RxDocumentConstructor,
    collection: Arc<RxCollection>,
    json_data: RxDocumentData,
) -> Arc<RxDocument> {
    let doc = constructor.construct(collection, json_data);
    let mut payload = doc.to_json(true);
    run_plugin_hooks("createRxDocument", &mut payload);
    doc
}

// ref: rxdb/src/rx-document.ts:440-442
pub fn is_rx_document(obj: &RxDocument) -> bool {
    obj.is_instance_of_rx_document
}

// ref: rxdb/src/rx-document.ts:445-465
pub async fn before_document_update_write(
    collection: &RxCollection,
    new_data: &mut RxDocumentWriteData,
    old_data: &RxDocumentData,
) -> RxResult<()> {
    let old_meta = old_data.get("_meta").cloned().unwrap_or(Value::Null);
    let new_meta = new_data.get("_meta").cloned().unwrap_or(Value::Null);
    let mut merged = serde_json::Map::new();
    if let Some(obj) = old_meta.as_object() {
        merged.extend(obj.clone());
    }
    if let Some(obj) = new_meta.as_object() {
        merged.extend(obj.clone());
    }
    if let Some(obj) = new_data.as_object_mut() {
        obj.insert("_meta".to_string(), Value::Object(merged));
    }

    if (OVERWRITABLE.load().is_dev_mode)() {
        if let Some(schema) = &collection.schema {
            schema.validate_change(old_data, new_data)?;
        }
    }
    Ok(())
}

// ref: rxdb/src/rx-document.ts:472-538
pub fn get_document_property(doc: &RxDocument, obj_path: &str) -> Value {
    if let Some(value) = doc.property_cache.lock().get(obj_path).cloned() {
        return value;
    }
    let data = doc.data.lock().clone();
    let value = get_property(&data, obj_path, None);
    let frozen = (OVERWRITABLE.load().deep_freeze_when_dev_mode)(value);
    doc.property_cache
        .lock()
        .insert(obj_path.to_string(), frozen.clone());
    frozen
}

fn plugin_missing_rx_error(plugin_key: &str) -> crate::rx_error::RxError {
    let err = plugin_missing(plugin_key);
    new_rx_error(
        "PLUGIN_MISSING",
        Some(serde_json::json!({
            "plugin": plugin_key,
            "message": err.to_string(),
        })),
    )
}

fn document_id_from_primary(doc_data: &Value, primary_path: &str) -> RxResult<String> {
    match doc_data.get(primary_path) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(v) => Ok(v.to_string()),
        None => Err(new_rx_error(
            "DOC_PRIMARY",
            Some(serde_json::json!({ "primaryPath": primary_path })),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, LazyLock};

    use crate::plugins::storage_memory::get_rx_storage_memory;
    use crate::replication_protocol::default_conflict_handler::DefaultConflictHandler;
    use crate::rx_schema::create_rx_schema;
    use crate::types::{
        HashFunction, HashOutput, JsonSchema, PrimaryKey, RxJsonSchema,
        RxStorageInstanceCreationParams,
    };

    static DEV_MODE_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct TestHashFunction;

    impl HashFunction for TestHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("hash:{input}") })
        }
    }

    fn doc() -> Value {
        serde_json::json!({
            "id": "a",
            "name": { "first": "Ada" },
            "_rev": "1-token",
            "_deleted": false,
            "_attachments": {},
            "_meta": { "lwt": 1.0 }
        })
    }

    fn raw_schema() -> RxJsonSchema {
        let mut name_properties = HashMap::new();
        name_properties.insert(
            "first".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
                ..Default::default()
            },
        );
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
            "name".to_string(),
            JsonSchema {
                schema_type: Some("object".to_string()),
                properties: name_properties,
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
            additional_properties: false,
            extra: HashMap::new(),
        }
    }

    async fn test_document_with_collection() -> Arc<RxDocument> {
        let hash_function = Arc::new(TestHashFunction);
        let schema =
            Arc::new(create_rx_schema(raw_schema(), hash_function.clone(), false).unwrap());
        let storage = get_rx_storage_memory(());
        let raw_storage_instance = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "doc-db".to_string(),
                    collection_name: "docs".to_string(),
                    schema: schema.json_schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let database = crate::rx_database::RxDatabase::new(
            "doc-db",
            "db-token",
            "storage-token",
            false,
            hash_function,
            storage,
        );
        let storage_instance = crate::rx_storage_helper::get_wrapped_storage_instance(
            Arc::clone(&database),
            raw_storage_instance,
            schema.json_schema.clone(),
        );
        let collection = RxCollection::new_with_schema(
            "docs",
            database,
            storage_instance,
            Arc::new(DefaultConflictHandler),
            schema,
        );
        RxDocument::new(collection, doc())
    }

    #[test]
    fn to_json_strips_meta_fields_by_default() {
        let doc = RxDocument::new_detached(doc());
        let json = doc.to_json(false);
        assert_eq!(json.get("id").and_then(Value::as_str), Some("a"));
        assert!(json.get("_rev").is_none());
        assert!(json.get("_meta").is_none());
    }

    #[test]
    fn get_document_property_caches_nested_values() {
        let doc = RxDocument::new_detached(doc());
        assert_eq!(doc.get("name.first").unwrap().as_str(), Some("Ada"));
        assert!(doc.property_cache.lock().contains_key("name.first"));
    }

    #[test]
    fn is_rx_document_checks_marker() {
        let doc = RxDocument::new_detached(doc());
        assert!(is_rx_document(&doc));
    }

    #[tokio::test]
    async fn get_dollar_rejects_unknown_schema_path_in_dev_mode() {
        let doc = test_document_with_collection().await;
        let _guard = DEV_MODE_TEST_LOCK.lock();
        let previous = OVERWRITABLE.load_full();
        crate::overwritable::replace_overwritable(|current| crate::overwritable::Overwritable {
            is_dev_mode: Arc::new(|| true),
            deep_freeze_when_dev_mode: Arc::clone(&current.deep_freeze_when_dev_mode),
            tunnel_error_message: Arc::clone(&current.tunnel_error_message),
        });

        assert_eq!(
            doc.get_dollar("name.first").unwrap(),
            serde_json::json!("Ada")
        );
        let err = doc.get_dollar("name.last").unwrap_err();
        assert_eq!(err.code(), "DOC4");

        OVERWRITABLE.store(previous);
    }

    #[tokio::test]
    async fn deleted_stream_starts_current_and_tracks_document_delete() {
        let doc = test_document_with_collection().await;
        let collection = Arc::clone(doc.collection.as_ref().unwrap());
        collection
            .insert(serde_json::json!({ "id": "a", "name": { "first": "Ada" } }))
            .await
            .unwrap();

        let mut deleted = doc.deleted_stream().unwrap();
        assert_eq!(deleted.next().await, Some(false));
        let deleted_signal = doc.deleted_double_dollar().unwrap();
        let mut deleted_signal_values = deleted_signal.subscribe();
        assert_eq!(deleted_signal_values.next().await, Some(false));

        collection
            .bulk_remove_by_ids(vec!["a".to_string()])
            .await
            .unwrap();

        let next = tokio::time::timeout(std::time::Duration::from_secs(1), deleted.next())
            .await
            .unwrap();
        assert_eq!(next, Some(true));
        let signal_next = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            deleted_signal_values.next(),
        )
        .await
        .unwrap();
        assert_eq!(signal_next, Some(true));
    }

    #[tokio::test]
    async fn document_and_field_streams_start_current_and_track_updates() {
        let seed = test_document_with_collection().await;
        let collection = Arc::clone(seed.collection.as_ref().unwrap());
        let doc = collection
            .insert(serde_json::json!({ "id": "a", "name": { "first": "Ada" } }))
            .await
            .unwrap();

        let mut documents = doc.document_stream().unwrap();
        let first_doc = documents.next().await.unwrap();
        assert_eq!(
            first_doc.get("name.first").unwrap(),
            serde_json::json!("Ada")
        );

        let mut first_names = doc.get_stream("name.first").unwrap();
        assert_eq!(first_names.next().await, Some(serde_json::json!("Ada")));
        let first_name_signal = doc.get_double_dollar("name.first").unwrap();
        let mut first_name_signal_values = first_name_signal.subscribe();
        assert_eq!(
            first_name_signal_values.next().await,
            Some(serde_json::json!("Ada"))
        );
        let document_signal = doc.double_dollar().unwrap();
        assert_eq!(
            document_signal.get_value().get("name.first").unwrap(),
            serde_json::json!("Ada")
        );

        doc.patch(serde_json::json!({ "extra": true }))
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), first_names.next())
                .await
                .is_err(),
            "get_stream must suppress unchanged field values"
        );

        doc.patch(serde_json::json!({ "name": { "first": "Grace" } }))
            .await
            .unwrap();

        let changed_name =
            tokio::time::timeout(std::time::Duration::from_secs(1), first_names.next())
                .await
                .unwrap();
        assert_eq!(changed_name, Some(serde_json::json!("Grace")));
        let changed_signal_name = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            first_name_signal_values.next(),
        )
        .await
        .unwrap();
        assert_eq!(changed_signal_name, Some(serde_json::json!("Grace")));
    }

    #[tokio::test]
    async fn save_data_runs_collection_pre_and_post_save_hooks() {
        let hash_function = Arc::new(TestHashFunction);
        let schema =
            Arc::new(create_rx_schema(raw_schema(), hash_function.clone(), false).unwrap());
        let storage = get_rx_storage_memory(());
        let raw_storage_instance = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "db-token".to_string(),
                    database_name: "save-hook-db".to_string(),
                    collection_name: "docs".to_string(),
                    schema: schema.json_schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let database = crate::rx_database::RxDatabase::new(
            "save-hook-db",
            "db-token",
            "storage-token",
            false,
            hash_function,
            storage,
        );
        let storage_instance = crate::rx_storage_helper::get_wrapped_storage_instance(
            Arc::clone(&database),
            raw_storage_instance,
            schema.json_schema.clone(),
        );
        let collection = RxCollection::new_with_schema(
            "docs",
            database,
            storage_instance,
            Arc::new(DefaultConflictHandler),
            schema,
        );
        let post_calls = Arc::new(AtomicUsize::new(0));
        collection
            .add_hook(
                "pre",
                "save",
                Arc::new(|mut doc_data, instance| {
                    Box::pin(async move {
                        assert_eq!(
                            instance
                                .as_ref()
                                .and_then(|document| document.primary().ok())
                                .as_deref(),
                            Some("a")
                        );
                        if let Some(obj) = doc_data.as_object_mut() {
                            obj.insert("name".to_string(), serde_json::json!({ "first": "Grace" }));
                        }
                        Ok(doc_data)
                    })
                }),
            )
            .unwrap();
        collection
            .add_hook("post", "save", {
                let post_calls = Arc::clone(&post_calls);
                Arc::new(move |doc_data, instance| {
                    let post_calls = Arc::clone(&post_calls);
                    Box::pin(async move {
                        assert_eq!(
                            doc_data.get("name").and_then(|name| name.get("first")),
                            Some(&serde_json::json!("Grace"))
                        );
                        assert_eq!(
                            instance
                                .as_ref()
                                .and_then(|document| document.primary().ok())
                                .as_deref(),
                            Some("a")
                        );
                        post_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(doc_data)
                    })
                })
            })
            .unwrap();

        let doc = collection
            .insert(serde_json::json!({ "id": "a", "name": { "first": "Ada" } }))
            .await
            .unwrap();
        let updated = doc
            .patch(serde_json::json!({ "name": { "first": "Katherine" } }))
            .await
            .unwrap();

        assert_eq!(
            updated.get("name.first").unwrap(),
            serde_json::json!("Grace")
        );
        assert_eq!(post_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn incremental_modify_runs_queue_pre_and_post_save_hooks() {
        let seed = test_document_with_collection().await;
        let collection = Arc::clone(seed.collection.as_ref().unwrap());
        let post_calls = Arc::new(AtomicUsize::new(0));
        collection
            .add_hook(
                "pre",
                "save",
                Arc::new(|mut doc_data, instance| {
                    Box::pin(async move {
                        assert!(instance.is_none());
                        if let Some(obj) = doc_data.as_object_mut() {
                            obj.insert(
                                "name".to_string(),
                                serde_json::json!({ "first": "QueueGrace" }),
                            );
                        }
                        Ok(doc_data)
                    })
                }),
            )
            .unwrap();
        collection
            .add_hook("post", "save", {
                let post_calls = Arc::clone(&post_calls);
                Arc::new(move |doc_data, instance| {
                    let post_calls = Arc::clone(&post_calls);
                    Box::pin(async move {
                        assert!(instance.is_none());
                        assert_eq!(
                            doc_data.get("name").and_then(|name| name.get("first")),
                            Some(&serde_json::json!("QueueGrace"))
                        );
                        post_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(doc_data)
                    })
                })
            })
            .unwrap();

        let doc = collection
            .insert(serde_json::json!({ "id": "a", "name": { "first": "Ada" } }))
            .await
            .unwrap();
        let updated = doc
            .incremental_patch(serde_json::json!({ "name": { "first": "Katherine" } }))
            .await
            .unwrap();

        assert_eq!(
            updated.get("name.first").unwrap(),
            serde_json::json!("QueueGrace")
        );
        assert_eq!(post_calls.load(Ordering::SeqCst), 1);
    }
}
