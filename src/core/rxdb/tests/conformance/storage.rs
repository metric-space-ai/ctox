use std::collections::HashMap;
use std::sync::Arc;

use rxdb::plugins::storage_memory::index_mod::get_rx_storage_memory;
use rxdb::rx_query_helper::{normalize_mango_query, prepare_query};
use rxdb::rx_schema_helper::fill_with_default_settings;
use rxdb::storage::sqlite::index_mod::get_rx_storage_sqlite;
use rxdb::storage::sqlite::types::RxStorageSqliteSettings;
use rxdb::types::{
    BulkWriteRow, JsonSchema, MangoQuery, PrimaryKey, RxJsonSchema, RxStorage, RxStorageInstance,
    RxStorageInstanceCreationParams,
};
use serde_json::{json, Value};
use tempfile::TempDir;

struct Backend {
    name: &'static str,
    instance: Arc<dyn RxStorageInstance>,
    _temp_dir: Option<TempDir>,
}

impl Backend {
    async fn memory(test_name: &str, schema: RxJsonSchema) -> Self {
        let storage: Arc<dyn RxStorage> = get_rx_storage_memory(());
        let instance = storage
            .create_storage_instance(params(test_name, "memory", schema))
            .await
            .expect("memory storage instance");
        Self {
            name: "memory",
            instance,
            _temp_dir: None,
        }
    }

    async fn sqlite(test_name: &str, schema: RxJsonSchema) -> Self {
        let temp_dir = tempfile::tempdir().expect("sqlite temp dir");
        let storage: Arc<dyn RxStorage> = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: temp_dir.path().join("ctox-rxdb-conformance.sqlite3"),
        });
        let instance = storage
            .create_storage_instance(params(test_name, "sqlite", schema))
            .await
            .expect("sqlite storage instance");
        Self {
            name: "sqlite",
            instance,
            _temp_dir: Some(temp_dir),
        }
    }
}

async fn backend_pair(test_name: &str) -> [Backend; 2] {
    let schema = person_schema();
    [
        Backend::memory(test_name, schema.clone()).await,
        Backend::sqlite(test_name, schema).await,
    ]
}

fn params(
    test_name: &str,
    backend_name: &str,
    schema: RxJsonSchema,
) -> RxStorageInstanceCreationParams {
    RxStorageInstanceCreationParams {
        database_instance_token: format!("token-{test_name}-{backend_name}"),
        database_name: format!("conformance-{test_name}-{backend_name}"),
        collection_name: "people".to_string(),
        schema,
        options: HashMap::new(),
        multi_instance: false,
        dev_mode: true,
        password: None,
    }
}

fn person_schema() -> RxJsonSchema {
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
            schema_type: Some("string".to_string()),
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
        required: vec!["id".to_string(), "name".to_string(), "age".to_string()],
        indexes: vec![vec!["age".to_string()]],
        encrypted: Vec::new(),
        internal_indexes: Vec::new(),
        key_compression: false,
        attachments: None,
        additional_properties: false,
        extra: HashMap::new(),
    })
}

fn doc(id: &str, name: &str, age: i64, revision: &str, lwt: f64, deleted: bool) -> Value {
    json!({
        "id": id,
        "name": name,
        "age": age,
        "_rev": revision,
        "_deleted": deleted,
        "_meta": { "lwt": lwt },
        "_attachments": {}
    })
}

fn seed_rows() -> Vec<BulkWriteRow> {
    vec![
        BulkWriteRow {
            previous: None,
            document: doc("alice", "Alice", 31, "1-alice", 1.0, false),
        },
        BulkWriteRow {
            previous: None,
            document: doc("bob", "Bob", 42, "1-bob", 2.0, false),
        },
        BulkWriteRow {
            previous: None,
            document: doc("cara", "Cara", 27, "1-cara", 3.0, false),
        },
    ]
}

fn ids(documents: &[Value]) -> Vec<String> {
    documents
        .iter()
        .map(|document| {
            document
                .get("id")
                .and_then(Value::as_str)
                .expect("document id")
                .to_string()
        })
        .collect()
}

async fn seed(instance: &Arc<dyn RxStorageInstance>) {
    let response = instance
        .bulk_write(seed_rows(), "conformance-seed")
        .await
        .expect("bulk_write seed");
    assert!(
        response.error.is_empty(),
        "seed write should not contain storage errors: {:?}",
        response.error
    );
}

#[tokio::test]
async fn bulk_write_and_find_documents_by_id_match() {
    for backend in backend_pair("bulk-write-find").await {
        seed(&backend.instance).await;

        let documents = backend
            .instance
            .find_documents_by_id(&["bob".to_string(), "alice".to_string()], false)
            .await
            .expect("find_documents_by_id");

        assert_eq!(ids(&documents), vec!["bob", "alice"]);
        assert_eq!(documents[0].get("age").and_then(Value::as_i64), Some(42));
        assert_eq!(
            documents[1].get("name").and_then(Value::as_str),
            Some("Alice")
        );
    }
}

#[tokio::test]
async fn query_and_count_with_mango_selector_sort_match() {
    for backend in backend_pair("query-count").await {
        seed(&backend.instance).await;

        let mut age_sort = HashMap::new();
        age_sort.insert("age".to_string(), "asc".to_string());
        let query = normalize_mango_query(
            backend.instance.schema(),
            MangoQuery {
                selector: Some(json!({ "age": { "$gte": 30 } })),
                sort: Some(vec![age_sort]),
                index: None,
                limit: None,
                skip: Some(0),
            },
        );
        let prepared = prepare_query(backend.instance.schema(), query).expect("prepare query");

        let result = backend.instance.query(&prepared).await.expect("query");
        let count = backend.instance.count(&prepared).await.expect("count");

        assert_eq!(ids(&result.documents), vec!["alice", "bob"]);
        assert_eq!(count.count, 2);
        assert_eq!(count.mode, "fast");
    }
}

#[tokio::test]
async fn changed_documents_since_checkpoint_matches() {
    for backend in backend_pair("changed-since").await {
        seed(&backend.instance).await;

        let changed = backend
            .instance
            .get_changed_documents_since(10, Some(&json!({ "id": "alice", "lwt": 1.0 })))
            .await
            .expect("changed documents since");

        assert_eq!(
            ids(&changed.documents),
            vec!["bob", "cara"],
            "{}",
            backend.name
        );
        assert_eq!(
            changed.checkpoint,
            json!({ "id": "cara", "lwt": 3.0 }),
            "{}",
            backend.name
        );
    }
}

#[tokio::test]
async fn deleted_document_visibility_matches() {
    for backend in backend_pair("deleted-visibility").await {
        seed(&backend.instance).await;

        let previous = doc("cara", "Cara", 27, "1-cara", 3.0, false);
        let response = backend
            .instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: Some(previous),
                    document: doc("cara", "Cara", 27, "2-cara", 4.0, true),
                }],
                "conformance-delete",
            )
            .await
            .expect("delete write");
        assert!(
            response.error.is_empty(),
            "delete write should not contain storage errors: {:?}",
            response.error
        );

        let hidden = backend
            .instance
            .find_documents_by_id(&["cara".to_string()], false)
            .await
            .expect("find without deleted");
        let visible = backend
            .instance
            .find_documents_by_id(&["cara".to_string()], true)
            .await
            .expect("find with deleted");

        assert!(hidden.is_empty());
        assert_eq!(ids(&visible), vec!["cara"]);
        assert_eq!(
            visible[0].get("_deleted").and_then(Value::as_bool),
            Some(true)
        );
    }
}
