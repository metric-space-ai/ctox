use std::collections::HashMap;
use std::sync::Arc;

use rxdb::plugins::storage_memory::index_mod::get_rx_storage_memory;
use rxdb::replication_protocol::default_conflict_handler::DefaultConflictHandler;
use rxdb::replication_protocol::index_mod::rx_storage_instance_to_replication_handler;
use rxdb::rx_schema_helper::fill_with_default_settings;
use rxdb::rxjs_compat::RxStream;
use rxdb::storage::sqlite::index_mod::get_rx_storage_sqlite;
use rxdb::storage::sqlite::types::RxStorageSqliteSettings;
use rxdb::types::{
    BulkWriteRow, JsonSchema, PrimaryKey, RxConflictHandler, RxConflictHandlerInput, RxJsonSchema,
    RxReplicationHandler, RxReplicationMasterChange, RxReplicationWriteToMasterRow, RxStorage,
    RxStorageInstance, RxStorageInstanceCreationParams,
};
use serde_json::{json, Value};
use tempfile::TempDir;

struct Backend {
    name: &'static str,
    instance: Arc<dyn RxStorageInstance>,
    _temp_dir: Option<TempDir>,
}

struct RejectingEqualConflictHandler;

#[async_trait::async_trait]
impl RxConflictHandler for RejectingEqualConflictHandler {
    async fn is_equal(&self, _a: &Value, _b: &Value, _ctx: &str) -> bool {
        false
    }

    async fn resolve(&self, input: &RxConflictHandlerInput, _ctx: &str) -> Value {
        input.real_master_state.clone()
    }
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
            database_path: temp_dir.path().join("ctox-rxdb-replication.sqlite3"),
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
        database_name: format!("conformance-replication-{test_name}-{backend_name}"),
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

fn write_doc(id: &str, name: &str, age: i64, revision: &str, lwt: f64, deleted: bool) -> Value {
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

fn doc_state(id: &str, name: &str, age: i64, deleted: bool) -> Value {
    json!({
        "id": id,
        "name": name,
        "age": age,
        "_deleted": deleted
    })
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
        .bulk_write(
            vec![
                BulkWriteRow {
                    previous: None,
                    document: write_doc("alice", "Alice", 31, "1-alice", 1.0, false),
                },
                BulkWriteRow {
                    previous: None,
                    document: write_doc("bob", "Bob", 42, "1-bob", 2.0, false),
                },
            ],
            "conformance-replication-seed",
        )
        .await
        .expect("bulk_write seed");

    assert!(
        response.error.is_empty(),
        "seed write should not contain storage errors: {:?}",
        response.error
    );
}

pub fn get_pull_handler(
    remote_instance: Arc<dyn RxStorageInstance>,
    database_instance_token: impl Into<String>,
) -> Arc<dyn RxReplicationHandler> {
    rx_storage_instance_to_replication_handler(
        remote_instance,
        Arc::new(DefaultConflictHandler),
        database_instance_token.into(),
        false,
    )
}

pub fn get_pull_stream(
    remote_instance: Arc<dyn RxStorageInstance>,
    database_instance_token: impl Into<String>,
) -> RxStream<RxReplicationMasterChange> {
    get_pull_handler(remote_instance, database_instance_token).master_change_stream()
}

pub fn get_push_handler(
    remote_instance: Arc<dyn RxStorageInstance>,
    database_instance_token: impl Into<String>,
) -> Arc<dyn RxReplicationHandler> {
    rx_storage_instance_to_replication_handler(
        remote_instance,
        Arc::new(DefaultConflictHandler),
        database_instance_token.into(),
        false,
    )
}

pub fn clean_doc_to_compare(document: &Value) -> Value {
    let mut cleaned = document.clone();
    if let Value::Object(object) = &mut cleaned {
        object.remove("_meta");
        object.remove("_rev");
    }
    cleaned
}

pub fn ensure_equal_state(
    documents_a: &[Value],
    documents_b: &[Value],
    context: Option<&str>,
) -> Result<(), String> {
    if documents_a.len() != documents_b.len() {
        return Err(format!(
            "STATE not equal (context: {:?}): length mismatch {} != {}",
            context,
            documents_a.len(),
            documents_b.len()
        ));
    }

    for (index, (doc_a, doc_b)) in documents_a.iter().zip(documents_b.iter()).enumerate() {
        let clean_a = clean_doc_to_compare(doc_a);
        let clean_b = clean_doc_to_compare(doc_b);
        if clean_a != clean_b {
            return Err(format!(
                "STATE not equal (context: {:?}) at index {index}: left={clean_a} right={clean_b}",
                context
            ));
        }
    }

    Ok(())
}

#[tokio::test]
async fn storage_replication_handler_empty_master_changes_shape_matches() {
    let backend = Backend::sqlite("handler-empty-changes", person_schema()).await;
    let handler = get_pull_handler(
        Arc::clone(&backend.instance),
        format!("replication-token-{}", backend.name),
    );

    let changes = handler
        .master_changes_since(None, 10)
        .await
        .expect("master_changes_since");

    assert!(changes.documents.is_empty(), "{}", backend.name);
    assert_eq!(
        changes.checkpoint,
        json!({ "id": "", "lwt": 0 }),
        "{}",
        backend.name
    );
}

#[tokio::test]
async fn pull_stream_helper_exposes_master_change_stream() {
    let backend = Backend::memory("pull-stream-helper", person_schema()).await;
    let stream = get_pull_stream(
        Arc::clone(&backend.instance),
        format!("replication-token-{}", backend.name),
    );
    drop(stream);
}

#[tokio::test]
async fn storage_replication_handler_master_changes_since_matches_storage_checkpointing() {
    for backend in backend_pair("handler-changes-since").await {
        seed(&backend.instance).await;
        let handler = get_pull_handler(
            Arc::clone(&backend.instance),
            format!("replication-token-{}", backend.name),
        );

        let changes = handler
            .master_changes_since(Some(json!({ "id": "alice", "lwt": 1.0 })), 10)
            .await
            .expect("master_changes_since");

        assert_eq!(ids(&changes.documents), vec!["bob"], "{}", backend.name);
        assert_eq!(
            changes.checkpoint,
            json!({ "id": "bob", "lwt": 2.0 }),
            "{}",
            backend.name
        );
        assert_eq!(
            changes.documents[0],
            doc_state("bob", "Bob", 42, false),
            "{}",
            backend.name
        );
    }
}

#[tokio::test]
async fn storage_replication_handler_master_write_persists_and_reports_conflicts() {
    for backend in backend_pair("handler-master-write").await {
        seed(&backend.instance).await;
        let handler = get_push_handler(
            Arc::clone(&backend.instance),
            format!("replication-token-{}", backend.name),
        );

        let conflicts = handler
            .master_write(vec![RxReplicationWriteToMasterRow {
                new_document_state: doc_state("cara", "Cara", 27, false),
                assumed_master_state: None,
            }])
            .await
            .expect("master_write insert");
        assert!(conflicts.is_empty(), "{}", backend.name);

        let stored = backend
            .instance
            .find_documents_by_id(&["cara".to_string()], false)
            .await
            .expect("find inserted document");
        assert_eq!(ids(&stored), vec!["cara"], "{}", backend.name);
        assert!(stored[0].get("_rev").is_some(), "{}", backend.name);
        assert!(stored[0].get("_meta").is_some(), "{}", backend.name);

        let conflicts = handler
            .master_write(vec![RxReplicationWriteToMasterRow {
                new_document_state: doc_state("alice", "Alicia", 32, false),
                assumed_master_state: None,
            }])
            .await
            .expect("master_write conflict");

        assert_eq!(
            conflicts,
            vec![doc_state("alice", "Alice", 31, false)],
            "{}",
            backend.name
        );
    }
}

#[tokio::test]
async fn storage_replication_handler_accepts_exact_assumed_master_despite_handler_false_negative() {
    for backend in backend_pair("handler-equal-assumed-fallback").await {
        seed(&backend.instance).await;
        let handler = rx_storage_instance_to_replication_handler(
            Arc::clone(&backend.instance),
            Arc::new(RejectingEqualConflictHandler),
            format!("replication-token-{}", backend.name),
            false,
        );
        let mut assumed = doc_state("alice", "Alice", 31, false);
        assumed["age"] = json!(31.0);

        let conflicts = handler
            .master_write(vec![RxReplicationWriteToMasterRow {
                new_document_state: doc_state("alice", "Alicia", 32, false),
                assumed_master_state: Some(assumed),
            }])
            .await
            .expect("exact assumed master fallback");

        assert!(conflicts.is_empty(), "{}", backend.name);
        let stored = backend
            .instance
            .find_documents_by_id(&["alice".to_string()], false)
            .await
            .expect("find updated document");
        assert_eq!(
            stored[0].get("name"),
            Some(&json!("Alicia")),
            "{}",
            backend.name
        );
    }
}

#[test]
fn ensure_equal_state_ignores_revision_and_meta_fields() {
    let left = vec![write_doc("alice", "Alice", 31, "1-left", 1.0, false)];
    let right = vec![write_doc("alice", "Alice", 31, "9-right", 99.0, false)];

    assert!(ensure_equal_state(&left, &right, Some("equal-clean-state")).is_ok());

    let different = vec![write_doc("alice", "Alicia", 31, "9-right", 99.0, false)];
    let error = ensure_equal_state(&left, &different, Some("different-state")).unwrap_err();
    assert!(error.contains("different-state"));
    assert!(error.contains("Alicia"));
}
