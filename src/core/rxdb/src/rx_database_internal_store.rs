//! Port of `src/rx-database-internal-store.ts`.
//!
//! Per-database meta storage: tracks the storage token (used to dedupe
//! broadcast-channel messages between in-process databases of the same name)
//! and the list of registered collections + connected-storage subscriptions
//! (replication meta instances, etc.).
//!
//! T1 deviations:
//! - Functions that upstream walk `collection.database.internalStore` take an
//!   explicit `internal_store: &Arc<dyn RxStorageInstance>` argument so they
//!   can run before the full `RxDatabase`/`RxCollection` surfaces land.
//! - `RxDatabase.password` is `Option<String>` (CTOX hashes the raw string).
//! - The internal-store schema (`INTERNAL_STORE_SCHEMA_BASE`) is a `LazyLock`
//!   that runs `fill_with_default_settings` once.
//! - Browser-only `sharding` config is omitted; CTOX never enabled the
//!   sharding plugin.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::plugins::utils::utils_document::{get_default_revision, get_default_rx_document_meta};
use crate::plugins::utils::utils_string::random_token;
use crate::rx_error::{is_bulk_write_conflict_error, new_rx_error, RxResult};
use crate::rx_schema_helper::{
    fill_with_default_settings, get_composed_primary_key_of_document_data,
};
use crate::rx_storage_helper::{
    get_single_document, get_written_documents_from_bulk_write_response, write_single,
};
use crate::types::{
    BulkWriteRow, CompositePrimaryKey, JsonSchema, PrimaryKey, RxJsonSchema, RxStorageInstance,
};

// ref: rxdb/src/rx-database-internal-store.ts:31-34
pub const INTERNAL_CONTEXT_COLLECTION: &str = "collection";
pub const INTERNAL_CONTEXT_STORAGE_TOKEN: &str = "storage-token";
pub const INTERNAL_CONTEXT_MIGRATION_STATUS: &str = "rx-migration-status";
pub const INTERNAL_CONTEXT_PIPELINE_CHECKPOINT: &str = "rx-pipeline-checkpoint";

// ref: rxdb/src/rx-database-internal-store.ts:44
pub const INTERNAL_STORE_SCHEMA_TITLE: &str = "RxInternalDocument";

// ref: rxdb/src/rx-database-internal-store.ts:46-99
/// Build the internal-store schema. Each `RxDatabase` instance gets its own
/// schema instance because `fill_with_default_settings` is destructive and
/// callers may serialize the result with backend-specific extras.
pub fn build_internal_store_schema() -> RxJsonSchema {
    let mut properties: HashMap<String, JsonSchema> = HashMap::new();
    properties.insert(
        "id".to_string(),
        JsonSchema {
            schema_type: Some("string".to_string()),
            max_length: Some(200),
            ..Default::default()
        },
    );
    properties.insert(
        "key".to_string(),
        JsonSchema {
            schema_type: Some("string".to_string()),
            ..Default::default()
        },
    );
    // ref: rxdb/src/rx-database-internal-store.ts:66-75
    let context_extra: HashMap<String, Value> = {
        let mut m = HashMap::new();
        m.insert(
            "enum".to_string(),
            json!([
                INTERNAL_CONTEXT_COLLECTION,
                INTERNAL_CONTEXT_STORAGE_TOKEN,
                INTERNAL_CONTEXT_MIGRATION_STATUS,
                INTERNAL_CONTEXT_PIPELINE_CHECKPOINT,
                "OTHER",
            ]),
        );
        m
    };
    properties.insert(
        "context".to_string(),
        JsonSchema {
            schema_type: Some("string".to_string()),
            extra: context_extra,
            ..Default::default()
        },
    );
    properties.insert(
        "data".to_string(),
        JsonSchema {
            schema_type: Some("object".to_string()),
            additional_properties: Some(true),
            ..Default::default()
        },
    );

    let base = RxJsonSchema {
        version: 0,
        primary_key: PrimaryKey::Composite(CompositePrimaryKey {
            key: "id".to_string(),
            fields: vec!["context".to_string(), "key".to_string()],
            separator: "|".to_string(),
        }),
        schema_type: "object".to_string(),
        properties,
        required: vec!["key".to_string(), "context".to_string(), "data".to_string()],
        indexes: Vec::new(),
        encrypted: Vec::new(),
        internal_indexes: Vec::new(),
        key_compression: false,
        attachments: None,
        additional_properties: false,
        extra: {
            let mut m = HashMap::new();
            m.insert(
                "title".to_string(),
                Value::String(INTERNAL_STORE_SCHEMA_TITLE.to_string()),
            );
            m
        },
    };
    fill_with_default_settings(base)
}

// ref: rxdb/src/rx-database-internal-store.ts:102-113
/// Compose the composite primary key of an internal-store document.
pub fn get_primary_key_of_internal_document(key: &str, context: &str) -> String {
    let schema = build_internal_store_schema();
    let probe = json!({ "key": key, "context": context });
    get_composed_primary_key_of_document_data(&schema, &probe).unwrap_or_else(|_| {
        // Composite key with separator "|" is deterministic; the only error
        // case (missing field) cannot trip here because we supplied both.
        format!("{context}|{key}")
    })
}

// ref: rxdb/src/rx-database-internal-store.ts:119-138
/// Returns all internal documents with `context == 'collection'`, sorted by id.
pub async fn get_all_collection_documents(
    storage_instance: &Arc<dyn RxStorageInstance>,
) -> RxResult<Vec<Value>> {
    use crate::rx_query_helper::{normalize_mango_query, prepare_query};
    use crate::types::MangoQuery;

    let mut sort_entry = HashMap::new();
    sort_entry.insert("id".to_string(), "asc".to_string());
    let mango = MangoQuery {
        selector: Some(json!({
            "context": INTERNAL_CONTEXT_COLLECTION,
            "_deleted": { "$eq": false },
        })),
        sort: Some(vec![sort_entry]),
        skip: Some(0),
        limit: None,
        index: None,
    };
    let filled = normalize_mango_query(storage_instance.schema(), mango);
    let prepared = prepare_query(storage_instance.schema(), filled)?;
    let result = storage_instance.query(&prepared).await?;
    Ok(result.documents)
}

// ref: rxdb/src/rx-database-internal-store.ts:145-150
pub const STORAGE_TOKEN_DOCUMENT_KEY: &str = "storageToken";

/// Composite primary key of the storage-token document.
pub fn storage_token_document_id() -> String {
    get_primary_key_of_internal_document(STORAGE_TOKEN_DOCUMENT_KEY, INTERNAL_CONTEXT_STORAGE_TOKEN)
}

// ref: rxdb/src/rx-database-internal-store.ts:152-244
/// Ensure the storage-token doc exists. Inserts it; on conflict reads the
/// existing one and validates `rxdbVersion` + `passwordHash` against the
/// in-memory database state.
pub async fn ensure_storage_token_document_exists(
    internal_store: &Arc<dyn RxStorageInstance>,
    hash_function: &crate::types::SharedHashFunction,
    db_name: &str,
    db_token: &str,
    db_password: Option<&str>,
    rxdb_version: &str,
) -> RxResult<Value> {
    let storage_token = random_token(Some(10));

    let password_hash = if let Some(pwd) = db_password {
        let serialized = serde_json::to_string(pwd).unwrap_or_else(|_| String::new());
        Some(hash_function.hash(serialized).await)
    } else {
        None
    };

    let mut data = json!({
        "rxdbVersion": rxdb_version,
        "token": storage_token,
        "instanceToken": db_token,
    });
    if let Some(ph) = password_hash.as_ref() {
        if let Some(obj) = data.as_object_mut() {
            obj.insert("passwordHash".to_string(), Value::String(ph.clone()));
        }
    }

    let doc = json!({
        "id": storage_token_document_id(),
        "context": INTERNAL_CONTEXT_STORAGE_TOKEN,
        "key": STORAGE_TOKEN_DOCUMENT_KEY,
        "data": data,
        "_deleted": false,
        "_meta": get_default_rx_document_meta(),
        "_rev": get_default_revision(),
        "_attachments": {},
    });

    let write_rows = vec![BulkWriteRow {
        previous: None,
        document: doc,
    }];
    let write_result = internal_store
        .bulk_write(write_rows.clone(), "internal-add-storage-token")
        .await?;

    if write_result.error.is_empty() {
        let written =
            get_written_documents_from_bulk_write_response("id", &write_rows, &write_result, None);
        return Ok(written.into_iter().next().unwrap_or(Value::Null));
    }

    // Conflict path: another instance inserted the storage-token doc first.
    let err = &write_result.error[0];
    let err_value = serde_json::to_value(err).unwrap_or(Value::Null);
    if is_bulk_write_conflict_error(&err_value).is_some() {
        let doc_in_db = err.document_in_db.clone().unwrap_or(Value::Null);
        let existing_version = doc_in_db
            .get("data")
            .and_then(|d| d.get("rxdbVersion"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if !is_database_state_version_compatible_with_database_code(existing_version, rxdb_version)
        {
            return Err(new_rx_error(
                "DM5",
                Some(json!({
                    "args": {
                        "database": db_name,
                        "databaseStateVersion": existing_version,
                        "codeVersion": rxdb_version,
                    }
                })),
            ));
        }
        if let Some(ph) = password_hash.as_ref() {
            let existing_hash = doc_in_db
                .get("data")
                .and_then(|d| d.get("passwordHash"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if ph != existing_hash {
                return Err(new_rx_error(
                    "DB1",
                    Some(json!({
                        "passwordHash": ph,
                        "existingPasswordHash": existing_hash,
                    })),
                ));
            }
        }
        return Ok(doc_in_db);
    }
    // Non-conflict storage error — propagate as a coded RxError.
    Err(new_rx_error("COL20", Some(err_value)))
}

// ref: rxdb/src/rx-database-internal-store.ts:247-270
/// Checks that the stored DB code version is forward-compatible with the
/// running code version. Major-version skews are rejected (with one explicit
/// 15→16 carve-out from upstream).
pub fn is_database_state_version_compatible_with_database_code(
    state_version: &str,
    code_version: &str,
) -> bool {
    if state_version.is_empty() {
        return false;
    }
    let state_major = state_version.split('.').next().unwrap_or_default();
    let code_major = code_version.split('.').next().unwrap_or_default();
    if state_major == "15" && code_major == "16" {
        return true;
    }
    state_major == code_major
}

// ref: rxdb/src/rx-database-internal-store.ts:276-334
/// Append `storage_collection_name` + `schema` to the collection's
/// `connectedStorages` list. Idempotent — no-op if already present. Loops on
/// write-conflict (other writers may be updating the same doc).
pub async fn add_connected_storage_to_collection(
    internal_store: &Arc<dyn RxStorageInstance>,
    collection_name: &str,
    collection_schema: &RxJsonSchema,
    storage_collection_name: &str,
    schema: &RxJsonSchema,
) -> RxResult<()> {
    if collection_schema.version != schema.version {
        return Err(new_rx_error(
            "SNH",
            Some(json!({
                "schema": schema,
                "version": collection_schema.version,
                "name": collection_name,
                "args": { "storageCollectionName": storage_collection_name },
            })),
        ));
    }

    let collection_name_with_version = collection_name_primary(collection_name, collection_schema);
    let collection_doc_id = get_primary_key_of_internal_document(
        &collection_name_with_version,
        INTERNAL_CONTEXT_COLLECTION,
    );

    loop {
        let collection_doc =
            get_single_document(internal_store.as_ref(), &collection_doc_id).await?;
        let Some(prev) = collection_doc else {
            return Err(new_rx_error(
                "SNH",
                Some(json!({
                    "message": "collection doc not found in internal store",
                    "collection": collection_name,
                })),
            ));
        };
        let mut save_data = prev.clone();
        let already_there = save_data
            .get("data")
            .and_then(|d| d.get("connectedStorages"))
            .and_then(|cs| cs.as_array())
            .map(|arr| {
                arr.iter().any(|row| {
                    row.get("collectionName").and_then(|v| v.as_str())
                        == Some(storage_collection_name)
                        && row
                            .get("schema")
                            .and_then(|s| s.get("version"))
                            .and_then(|v| v.as_i64())
                            == Some(schema.version as i64)
                })
            })
            .unwrap_or(false);
        if already_there {
            return Ok(());
        }
        if let Some(data) = save_data.get_mut("data").and_then(|d| d.as_object_mut()) {
            let entry = json!({
                "collectionName": storage_collection_name,
                "schema": schema,
            });
            let arr = data
                .entry("connectedStorages".to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Some(a) = arr.as_array_mut() {
                a.push(entry);
            }
        }
        match write_single(
            internal_store.as_ref(),
            BulkWriteRow {
                previous: Some(prev),
                document: save_data,
            },
            "add-connected-storage-to-collection",
        )
        .await
        {
            Ok(_) => return Ok(()),
            Err(e) => {
                let err_value = serde_json::to_value(&e.parameters()).unwrap_or(Value::Null);
                if is_bulk_write_conflict_error(&err_value).is_some() {
                    continue;
                }
                return Err(e);
            }
        }
    }
}

// ref: rxdb/src/rx-database-internal-store.ts:337-392
pub async fn remove_connected_storage_from_collection(
    internal_store: &Arc<dyn RxStorageInstance>,
    collection_name: &str,
    collection_schema: &RxJsonSchema,
    storage_collection_name: &str,
    schema: &RxJsonSchema,
) -> RxResult<()> {
    if collection_schema.version != schema.version {
        return Err(new_rx_error(
            "SNH",
            Some(json!({
                "schema": schema,
                "version": collection_schema.version,
                "name": collection_name,
                "args": { "storageCollectionName": storage_collection_name },
            })),
        ));
    }

    let collection_name_with_version = collection_name_primary(collection_name, collection_schema);
    let collection_doc_id = get_primary_key_of_internal_document(
        &collection_name_with_version,
        INTERNAL_CONTEXT_COLLECTION,
    );

    loop {
        let collection_doc =
            get_single_document(internal_store.as_ref(), &collection_doc_id).await?;
        let Some(prev) = collection_doc else {
            return Ok(());
        };
        let mut save_data = prev.clone();
        let was_there = save_data
            .get("data")
            .and_then(|d| d.get("connectedStorages"))
            .and_then(|cs| cs.as_array())
            .map(|arr| {
                arr.iter().any(|row| {
                    row.get("collectionName").and_then(|v| v.as_str())
                        == Some(storage_collection_name)
                        && row
                            .get("schema")
                            .and_then(|s| s.get("version"))
                            .and_then(|v| v.as_i64())
                            == Some(schema.version as i64)
                })
            })
            .unwrap_or(false);
        if !was_there {
            return Ok(());
        }
        if let Some(data) = save_data.get_mut("data").and_then(|d| d.as_object_mut()) {
            if let Some(arr) = data
                .get_mut("connectedStorages")
                .and_then(|v| v.as_array_mut())
            {
                arr.retain(|row| {
                    row.get("collectionName").and_then(|v| v.as_str())
                        != Some(storage_collection_name)
                });
            }
        }
        match write_single(
            internal_store.as_ref(),
            BulkWriteRow {
                previous: Some(prev),
                document: save_data,
            },
            "remove-connected-storage-from-collection",
        )
        .await
        {
            Ok(_) => return Ok(()),
            Err(e) => {
                let err_value = serde_json::to_value(&e.parameters()).unwrap_or(Value::Null);
                if is_bulk_write_conflict_error(&err_value).is_some() {
                    continue;
                }
                return Err(e);
            }
        }
    }
}

// ref: rxdb/src/rx-database-internal-store.ts:400-402
/// Composite name used inside the internal store. Appends the schema version
/// so v1 of a collection and v2 of the same collection live as separate
/// internal-store docs.
pub fn collection_name_primary(name: &str, schema: &RxJsonSchema) -> String {
    format!("{}-{}", name, schema.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_composite_primary_key() {
        let s = build_internal_store_schema();
        match s.primary_key {
            PrimaryKey::Composite(ref c) => {
                assert_eq!(c.key, "id");
                assert_eq!(c.fields, vec!["context", "key"]);
                assert_eq!(c.separator, "|");
            }
            _ => panic!("expected composite primary key"),
        }
    }

    #[test]
    fn primary_key_composes_correctly() {
        let id = get_primary_key_of_internal_document("foo", "collection");
        assert_eq!(id, "collection|foo");
    }

    #[test]
    fn storage_token_doc_id_is_stable() {
        let id1 = storage_token_document_id();
        let id2 = storage_token_document_id();
        assert_eq!(id1, id2);
        assert_eq!(id1, "storage-token|storageToken");
    }

    #[test]
    fn version_compat_same_major() {
        assert!(is_database_state_version_compatible_with_database_code(
            "16.20.0", "16.99.0"
        ));
    }

    #[test]
    fn version_compat_v15_to_v16_carveout() {
        assert!(is_database_state_version_compatible_with_database_code(
            "15.5.0", "16.0.0"
        ));
    }

    #[test]
    fn version_compat_rejects_major_skew() {
        assert!(!is_database_state_version_compatible_with_database_code(
            "14.0.0", "16.0.0"
        ));
        assert!(!is_database_state_version_compatible_with_database_code(
            "17.0.0", "16.0.0"
        ));
    }

    #[test]
    fn version_compat_rejects_empty() {
        assert!(!is_database_state_version_compatible_with_database_code(
            "", "16.0.0"
        ));
    }

    struct Sha256HashFunction;

    impl crate::types::HashFunction for Sha256HashFunction {
        fn hash<'a>(&'a self, input: String) -> crate::types::HashOutput<'a> {
            use sha2::{Digest, Sha256};
            Box::pin(async move {
                let mut h = Sha256::new();
                h.update(input.as_bytes());
                format!("{:x}", h.finalize())
            })
        }
    }

    #[tokio::test]
    async fn ensure_storage_token_writes_then_reads_same_doc() {
        use crate::plugins::storage_memory::rx_storage_instance_memory::RxStorageMemory;
        use crate::types::{RxStorageInstanceCreationParams, SharedHashFunction};

        let storage = RxStorageMemory::new();
        let params = RxStorageInstanceCreationParams {
            database_instance_token: "tok-A".to_string(),
            database_name: "db-A".to_string(),
            collection_name: "_rxdb_internal".to_string(),
            schema: build_internal_store_schema(),
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: false,
            password: None,
        };
        let instance =
            <RxStorageMemory as crate::types::RxStorage>::create_storage_instance(&storage, params)
                .await
                .expect("create_storage_instance");
        let hash_fn: SharedHashFunction = Arc::new(Sha256HashFunction);

        // First call inserts.
        let doc1 = ensure_storage_token_document_exists(
            &instance, &hash_fn, "db-A", "tok-A", None, "16.20.0",
        )
        .await
        .expect("first insert");
        assert_eq!(
            doc1.get("id").and_then(|v| v.as_str()),
            Some(storage_token_document_id().as_str())
        );

        // Second call hits the conflict path and returns the existing doc.
        let doc2 = ensure_storage_token_document_exists(
            &instance, &hash_fn, "db-A", "tok-B", None, "16.20.0",
        )
        .await
        .expect("conflict-path returns existing");
        let token1 = doc1
            .get("data")
            .and_then(|d| d.get("token"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let token2 = doc2
            .get("data")
            .and_then(|d| d.get("token"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        assert_eq!(
            token1, token2,
            "conflict path must surface the originally written storage token"
        );
    }

    #[tokio::test]
    async fn ensure_storage_token_rejects_incompatible_major_version() {
        use crate::plugins::storage_memory::rx_storage_instance_memory::RxStorageMemory;
        use crate::types::{RxStorageInstanceCreationParams, SharedHashFunction};

        let storage = RxStorageMemory::new();
        let params = RxStorageInstanceCreationParams {
            database_instance_token: "tok".to_string(),
            database_name: "db".to_string(),
            collection_name: "_rxdb_internal".to_string(),
            schema: build_internal_store_schema(),
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: false,
            password: None,
        };
        let instance =
            <RxStorageMemory as crate::types::RxStorage>::create_storage_instance(&storage, params)
                .await
                .unwrap();
        let hash_fn: SharedHashFunction = Arc::new(Sha256HashFunction);
        let _ =
            ensure_storage_token_document_exists(&instance, &hash_fn, "db", "tok", None, "14.0.0")
                .await
                .unwrap();
        let err = ensure_storage_token_document_exists(
            &instance, &hash_fn, "db", "tok2", None, "16.20.0",
        )
        .await
        .expect_err("expected DM5");
        assert_eq!(err.code(), "DM5");
    }

    #[test]
    fn collection_name_primary_appends_version() {
        let schema = RxJsonSchema {
            version: 3,
            primary_key: PrimaryKey::Simple("id".to_string()),
            schema_type: "object".to_string(),
            properties: HashMap::new(),
            required: vec![],
            indexes: vec![],
            encrypted: vec![],
            internal_indexes: vec![],
            key_compression: false,
            attachments: None,
            additional_properties: false,
            extra: HashMap::new(),
        };
        assert_eq!(collection_name_primary("users", &schema), "users-3");
    }
}
