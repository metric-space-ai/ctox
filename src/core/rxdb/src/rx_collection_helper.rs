//! Port of `src/rx-collection-helper.ts`.
//!
//! Three exports:
//! - [`fill_object_data_before_insert`] — fills defaults, `_meta`, `_deleted`,
//!   `_attachments`, `_rev` and composite primary key before a user-side
//!   `insert(data)` reaches the storage layer.
//! - [`create_rx_collection_storage_instance`] — sets `multi_instance` and
//!   delegates to `storage.create_storage_instance`.
//! - [`remove_collection_storages`] — drops the storage instance plus all
//!   connected meta-stores listed in the internal store's collection doc.
//! - [`ensure_rx_collection_is_not_closed`] — guards public collection methods.
//!
//! T1 deviations:
//! - Upstream takes `RxDatabase`/`RxCollection` handles; we take the explicit
//!   storage / internal-store / token / name / schema arguments since
//!   `RxDatabase` and `RxCollection` are stubs. Full surfaces re-route to
//!   these once phase-6 lands.
//! - `runAsyncPluginHooks('postRemoveRxCollection', ...)` is invoked with a
//!   JSON object containing the same `{storage, databaseName, collectionName}`
//!   fields upstream uses.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::hooks::run_async_plugin_hooks;
use crate::plugins::utils::utils_document::{get_default_revision, get_default_rx_document_meta};
use crate::plugins::utils::utils_revision::create_revision;
use crate::plugins::utils::utils_time::now;
use crate::rx_database_internal_store::get_all_collection_documents;
use crate::rx_error::{new_rx_error, RxResult};
use crate::rx_schema::RxSchema;
use crate::rx_schema_helper::fill_primary_key;
use crate::rx_storage_helper::flat_clone_doc_with_meta;
use crate::types::{
    BulkWriteRow, PrimaryKey, RxJsonSchema, RxStorage, RxStorageInstance,
    RxStorageInstanceCreationParams,
};

// ref: rxdb/src/rx-collection-helper.ts:35-59
/// Fills defaults + canonical envelope fields on `data`. Returns the
/// populated value (the input is consumed; callers `flat_clone` first if they
/// need to keep the original).
pub fn fill_object_data_before_insert(schema: &RxSchema, mut data: Value) -> RxResult<Value> {
    // ref: rxdb/src/rx-schema-helper.ts:344-353 fillObjectWithDefaults
    if let Some(obj) = data.as_object_mut() {
        for (key, default) in schema.default_values().iter() {
            let needs_default = match obj.get(key) {
                None | Some(Value::Null) => true,
                _ => false,
            };
            if needs_default {
                obj.insert(key.clone(), default.clone());
            }
        }
    }
    // Composite primary key — primary-key field is filled from constituents.
    if !matches!(schema.json_schema.primary_key, PrimaryKey::Simple(_)) {
        fill_primary_key(&schema.primary_path, &schema.json_schema, &mut data)?;
    }
    if let Some(obj) = data.as_object_mut() {
        obj.insert(
            "_meta".to_string(),
            serde_json::to_value(get_default_rx_document_meta()).unwrap_or(Value::Null),
        );
        if !obj.contains_key("_deleted") {
            obj.insert("_deleted".to_string(), Value::Bool(false));
        }
        if !obj.contains_key("_attachments") {
            obj.insert(
                "_attachments".to_string(),
                Value::Object(serde_json::Map::new()),
            );
        }
        if !obj.contains_key("_rev") {
            obj.insert("_rev".to_string(), Value::String(get_default_revision()));
        }
    }
    Ok(data)
}

// ref: rxdb/src/rx-collection-helper.ts:64-73
/// Sets `multi_instance` to the database setting and delegates to
/// `storage.create_storage_instance`.
pub async fn create_rx_collection_storage_instance(
    storage: &Arc<dyn RxStorage>,
    db_multi_instance: bool,
    mut params: RxStorageInstanceCreationParams,
) -> RxResult<Arc<dyn RxStorageInstance>> {
    params.multi_instance = db_multi_instance;
    storage.create_storage_instance(params).await
}

// ref: rxdb/src/rx-collection-helper.ts:79-180
/// Remove the main storage of the collection and every connected meta
/// storage (replication meta etc.) listed in the internal store.
///
/// `hash_function` controls whether the meta documents themselves are
/// soft-deleted (`Some`) or left in place (`None` — used when the caller is
/// about to remove the whole internal store anyway).
#[allow(clippy::too_many_arguments)]
pub async fn remove_collection_storages(
    storage: &Arc<dyn RxStorage>,
    database_internal_store: &Arc<dyn RxStorageInstance>,
    database_instance_token: &str,
    database_name: &str,
    collection_name: &str,
    multi_instance: bool,
    password: Option<&str>,
    hash_function: Option<&crate::types::SharedHashFunction>,
) -> RxResult<()> {
    let all_meta_docs = get_all_collection_documents(database_internal_store).await?;
    let relevant_meta_docs: Vec<Value> = all_meta_docs
        .into_iter()
        .filter(|d| {
            d.get("data")
                .and_then(|data| data.get("name"))
                .and_then(|v| v.as_str())
                == Some(collection_name)
        })
        .collect();

    #[derive(Clone)]
    struct RemoveEntry {
        collection_name: String,
        schema: RxJsonSchema,
        is_collection: bool,
    }
    let mut remove: Vec<RemoveEntry> = Vec::new();
    for meta_doc in relevant_meta_docs.iter() {
        let data = match meta_doc.get("data") {
            Some(d) => d,
            None => continue,
        };
        let coll_name = data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let schema = data
            .get("schema")
            .cloned()
            .and_then(|s| serde_json::from_value::<RxJsonSchema>(s).ok());
        if let Some(schema) = schema {
            remove.push(RemoveEntry {
                collection_name: coll_name.clone(),
                schema,
                is_collection: true,
            });
        }
        if let Some(arr) = data.get("connectedStorages").and_then(|v| v.as_array()) {
            for row in arr.iter() {
                let inner_name = row
                    .get("collectionName")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let inner_schema = row
                    .get("schema")
                    .cloned()
                    .and_then(|s| serde_json::from_value::<RxJsonSchema>(s).ok());
                if let Some(s) = inner_schema {
                    remove.push(RemoveEntry {
                        collection_name: inner_name,
                        schema: s,
                        is_collection: false,
                    });
                }
            }
        }
    }

    // Dedupe by `{collectionName}||{schemaVersion}`.
    let mut seen = HashSet::new();
    remove.retain(|entry| {
        let key = format!("{}||{}", entry.collection_name, entry.schema.version);
        seen.insert(key)
    });

    for entry in remove.into_iter() {
        let storage_instance = storage
            .create_storage_instance(RxStorageInstanceCreationParams {
                database_instance_token: database_instance_token.to_string(),
                database_name: database_name.to_string(),
                collection_name: entry.collection_name.clone(),
                schema: entry.schema,
                options: std::collections::HashMap::new(),
                multi_instance,
                dev_mode: false,
                password: password.map(|p| p.to_string()),
            })
            .await?;
        storage_instance.remove().await?;
        if entry.is_collection {
            let mut payload = json!({
                "storageName": storage.name(),
                "databaseName": database_name,
                "collectionName": collection_name,
            });
            run_async_plugin_hooks("postRemoveRxCollection", &mut payload).await;
        }
    }

    // Soft-delete the meta documents themselves, if requested.
    if hash_function.is_some() {
        let write_rows: Vec<BulkWriteRow> = relevant_meta_docs
            .iter()
            .map(|doc| {
                let mut write_doc = flat_clone_doc_with_meta(doc);
                if let Some(obj) = write_doc.as_object_mut() {
                    obj.insert("_deleted".to_string(), Value::Bool(true));
                    if let Some(meta) = obj.get_mut("_meta").and_then(|v| v.as_object_mut()) {
                        meta.insert("lwt".to_string(), json!(now()));
                    }
                    let prev_rev = doc.get("_rev").and_then(|v| v.as_str());
                    let rev =
                        create_revision(database_instance_token, prev_rev).unwrap_or_default();
                    obj.insert("_rev".to_string(), Value::String(rev));
                }
                BulkWriteRow {
                    previous: Some(doc.clone()),
                    document: write_doc,
                }
            })
            .collect();
        if !write_rows.is_empty() {
            database_internal_store
                .bulk_write(write_rows, "rx-database-remove-collection-all")
                .await?;
        }
    }
    Ok(())
}

// ref: rxdb/src/rx-collection-helper.ts:183-195
/// Throws `COL21` if `closed == true`. Callers should invoke this at the top
/// of every public collection method.
pub fn ensure_rx_collection_is_not_closed(
    collection_name: &str,
    schema_version: i32,
    closed: bool,
) -> RxResult<()> {
    if closed {
        return Err(new_rx_error(
            "COL21",
            Some(json!({
                "collection": collection_name,
                "version": schema_version,
            })),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::utils::utils_object::flat_clone;

    struct TestHashFunction;
    impl crate::types::HashFunction for TestHashFunction {
        fn hash<'a>(&'a self, input: String) -> crate::types::HashOutput<'a> {
            use sha2::{Digest, Sha256};
            Box::pin(async move {
                let mut h = Sha256::new();
                h.update(input.as_bytes());
                format!("{:x}", h.finalize())
            })
        }
    }

    fn simple_schema() -> RxSchema {
        let json_schema = RxJsonSchema {
            version: 0,
            primary_key: PrimaryKey::Simple("id".to_string()),
            schema_type: "object".to_string(),
            properties: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "id".to_string(),
                    crate::types::JsonSchema {
                        schema_type: Some("string".to_string()),
                        max_length: Some(100),
                        ..Default::default()
                    },
                );
                m.insert(
                    "name".to_string(),
                    crate::types::JsonSchema {
                        schema_type: Some("string".to_string()),
                        default: Some(json!("anonymous")),
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
            extra: std::collections::HashMap::new(),
        };
        let hash_function: crate::types::SharedHashFunction = Arc::new(TestHashFunction);
        RxSchema::new(json_schema, hash_function).expect("RxSchema::new")
    }

    #[test]
    fn fill_object_fills_defaults_and_envelope() {
        let schema = simple_schema();
        let data = json!({ "id": "doc1" });
        let filled = fill_object_data_before_insert(&schema, flat_clone(&data)).unwrap();
        assert_eq!(
            filled.get("name").and_then(|v| v.as_str()),
            Some("anonymous")
        );
        assert_eq!(filled.get("_deleted"), Some(&Value::Bool(false)));
        assert!(filled.get("_meta").is_some());
        assert!(filled.get("_attachments").is_some());
        assert!(filled.get("_rev").is_some());
    }

    #[test]
    fn ensure_not_closed_returns_err_when_closed() {
        let err = ensure_rx_collection_is_not_closed("users", 0, true).unwrap_err();
        assert_eq!(err.code(), "COL21");
        assert!(ensure_rx_collection_is_not_closed("users", 0, false).is_ok());
    }
}
