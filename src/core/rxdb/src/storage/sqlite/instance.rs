//! SQLite [`crate::types::RxStorageInstance`] implementation.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use tokio::sync::Notify;

use crate::plugins::utils::utils_string::random_token;
use crate::rx_error::{new_rx_error, RxError, RxResult};
use crate::rx_query_helper::{get_query_matcher, get_sort_comparator};
use crate::rx_schema_helper::get_primary_field_of_primary_key;
use crate::rxjs_compat::{RxStream, RxSubject};
use crate::types::{
    BulkWriteRow, EventBulk, FilledMangoQuery, RxJsonSchema, RxStorageBulkWriteResponse,
    RxStorageChangedDocumentsSinceResult, RxStorageCountResult, RxStorageInstance,
    RxStorageInstanceCreationParams, RxStorageQueryResult,
};

use super::cleanup::cleanup_deleted_documents;
use super::sql::{
    all_documents, document_by_id, drop_table, insert_document, quote_identifier, update_document,
};
use super::types::{sqlite_error, SharedSqliteConnection};

static INSTANCE_ID: AtomicU64 = AtomicU64::new(0);

static UPDATE_REGISTRY: OnceLock<StdMutex<HashMap<String, Arc<Notify>>>> = OnceLock::new();

pub fn register_table_notifier(table_name: &str, notifier: Arc<Notify>) {
    let mut map = UPDATE_REGISTRY
        .get_or_init(|| StdMutex::new(HashMap::new()))
        .lock()
        .unwrap();
    map.insert(table_name.to_string(), notifier);
}

pub fn unregister_table_notifier(table_name: &str) {
    if let Some(registry) = UPDATE_REGISTRY.get() {
        let mut map = registry.lock().unwrap();
        map.remove(table_name);
    }
}

pub fn notify_table_change(table_name: &str) {
    if let Some(registry) = UPDATE_REGISTRY.get() {
        let map = registry.lock().unwrap();
        if let Some(notifier) = map.get(table_name) {
            notifier.notify_one();
        }
    }
}

pub struct RxStorageInstanceSqlite {
    pub database_name: String,
    pub collection_name: String,
    pub schema: RxJsonSchema,
    pub connection: SharedSqliteConnection,
    pub table_name: String,
    pub primary_path: String,
    changes: RxSubject<EventBulk>,
    closed: Arc<AtomicBool>,
    external_checkpoint: Arc<Mutex<Value>>,
    instance_id: u64,
}

impl RxStorageInstanceSqlite {
    pub fn new(
        connection: SharedSqliteConnection,
        params: RxStorageInstanceCreationParams,
        table_name: String,
    ) -> Self {
        let primary_path = get_primary_field_of_primary_key(&params.schema.primary_key);
        let changes = RxSubject::new();
        let closed = Arc::new(AtomicBool::new(false));
        let external_checkpoint = Arc::new(Mutex::new({
            let conn = connection.lock();
            latest_checkpoint(&conn, &table_name).unwrap_or_else(|| json!({ "id": "", "lwt": 0 }))
        }));

        let notifier = Arc::new(Notify::new());
        register_table_notifier(&table_name, Arc::clone(&notifier));

        start_external_write_poll(
            Arc::clone(&connection),
            table_name.clone(),
            primary_path.clone(),
            changes.clone(),
            Arc::clone(&closed),
            Arc::clone(&external_checkpoint),
            notifier,
        );
        Self {
            database_name: params.database_name,
            collection_name: params.collection_name,
            schema: params.schema,
            connection,
            table_name,
            primary_path,
            changes,
            closed,
            external_checkpoint,
            instance_id: INSTANCE_ID.fetch_add(1, Ordering::SeqCst),
        }
    }

    fn ensure_open(&self, method: &str) -> RxResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(new_rx_error(
                "SQLITE_CLOSED",
                Some(json!({
                    "method": method,
                    "databaseName": self.database_name,
                    "collectionName": self.collection_name,
                    "instanceId": self.instance_id,
                })),
            ));
        }
        Ok(())
    }

    fn load_docs_in_db(&self, conn: &rusqlite::Connection) -> RxResult<HashMap<String, Value>> {
        let docs = all_documents(conn, &self.table_name)?;
        let mut docs_in_db = HashMap::new();
        for doc in docs {
            if let Some(id) = doc.get(&self.primary_path).and_then(Value::as_str) {
                docs_in_db.insert(id.to_string(), doc);
            }
        }
        Ok(docs_in_db)
    }
}

fn start_external_write_poll(
    connection: SharedSqliteConnection,
    table_name: String,
    primary_path: String,
    changes: RxSubject<EventBulk>,
    closed: Arc<AtomicBool>,
    checkpoint: Arc<Mutex<Value>>,
    notifier: Arc<Notify>,
) {
    let Ok(_) = tokio::runtime::Handle::try_current() else {
        return;
    };
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(1000)) => {
                    // Fallback low-CPU poll for external processes
                }
                _ = notifier.notified() => {
                    // Instant notification from SQLite update_hook!
                }
            }
            if closed.load(Ordering::SeqCst) {
                break;
            }
            let result = {
                let checkpoint = checkpoint.lock().clone();
                let conn = connection.lock();
                changed_documents_since(&conn, &table_name, &primary_path, 50, Some(&checkpoint))
            };
            let Ok(result) = result else {
                continue;
            };
            if result.documents.is_empty() {
                *checkpoint.lock() = result.checkpoint;
                continue;
            }
            let events = result
                .documents
                .iter()
                .filter_map(|doc| {
                    let id = doc.get(&primary_path).and_then(Value::as_str)?;
                    let deleted = doc
                        .get("_deleted")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    Some(crate::types::RxStorageChangeEvent {
                        operation: if deleted { "DELETE" } else { "UPDATE" }.to_string(),
                        document_id: id.to_string(),
                        document_data: Some(doc.clone()),
                        previous_document_data: None,
                        is_local: false,
                    })
                })
                .collect::<Vec<_>>();
            *checkpoint.lock() = result.checkpoint.clone();
            if !events.is_empty() {
                changes.next(EventBulk {
                    id: random_token(Some(10)),
                    events,
                    checkpoint: Some(result.checkpoint),
                    context: Some("sqlite-external-poll".to_string()),
                });
            }
        }
    });
}

fn latest_checkpoint(conn: &rusqlite::Connection, table_name: &str) -> Option<Value> {
    conn.query_row(
        &format!(
            "SELECT id, lastWriteTime FROM {} ORDER BY lastWriteTime DESC, id DESC LIMIT 1",
            quote_identifier(table_name)
        ),
        [],
        |row| {
            let id: String = row.get(0)?;
            let lwt: f64 = row.get(1)?;
            Ok(json!({ "id": id, "lwt": lwt }))
        },
    )
    .optional()
    .ok()
    .flatten()
}

fn changed_documents_since(
    conn: &rusqlite::Connection,
    table_name: &str,
    primary_path: &str,
    limit: u64,
    checkpoint: Option<&Value>,
) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
    let since_lwt = checkpoint
        .and_then(|checkpoint| checkpoint.get("lwt"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let since_id = checkpoint
        .and_then(|checkpoint| checkpoint.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let mut stmt = conn
        .prepare(&format!(
            "SELECT data FROM {} WHERE lastWriteTime > ? OR (lastWriteTime = ? AND id > ?) ORDER BY lastWriteTime ASC, id ASC LIMIT ?",
            quote_identifier(table_name)
        ))
        .map_err(sqlite_error)?;
    let rows = stmt
        .query_map(
            params![since_lwt, since_lwt, since_id, limit as i64],
            |row| row.get::<_, String>(0),
        )
        .map_err(sqlite_error)?;
    let mut documents = Vec::new();
    for row in rows {
        let data = row.map_err(sqlite_error)?;
        documents.push(serde_json::from_str::<Value>(&data).map_err(|err| {
            new_rx_error("SQLITE_JSON", Some(json!({ "message": err.to_string() })))
        })?);
    }
    let checkpoint = documents
        .last()
        .map(|doc| {
            json!({
                "id": doc.get(primary_path).cloned().unwrap_or(Value::Null),
                "lwt": doc
                    .get("_meta")
                    .and_then(|meta| meta.get("lwt"))
                    .cloned()
                    .unwrap_or(json!(0)),
            })
        })
        .or_else(|| checkpoint.cloned())
        .unwrap_or_else(|| json!({ "id": "", "lwt": 0 }));
    Ok(RxStorageChangedDocumentsSinceResult {
        documents,
        checkpoint,
    })
}

#[async_trait]
impl RxStorageInstance for RxStorageInstanceSqlite {
    fn database_name(&self) -> &str {
        &self.database_name
    }

    fn collection_name(&self) -> &str {
        &self.collection_name
    }

    fn schema(&self) -> &RxJsonSchema {
        &self.schema
    }

    async fn bulk_write(
        &self,
        document_writes: Vec<BulkWriteRow>,
        context: &str,
    ) -> Result<RxStorageBulkWriteResponse, RxError> {
        self.ensure_open("bulk_write")?;
        let mut ret = RxStorageBulkWriteResponse { error: Vec::new() };
        let mut event_bulk: Option<EventBulk> = None;
        let mut checkpoint: Option<Value> = None;
        {
            let mut conn = self.connection.lock();
            let tx = conn.transaction().map_err(sqlite_error)?;
            let docs_in_db = self.load_docs_in_db(&tx)?;
            let categorized = crate::rx_storage_helper::categorize_bulk_write_rows(
                self.schema.attachments.is_some(),
                &self.primary_path,
                &docs_in_db,
                &document_writes,
                context,
            );
            ret.error = categorized.errors;

            for row in categorized.bulk_insert_docs.iter() {
                insert_document(&tx, &self.table_name, &self.primary_path, &row.document)?;
            }
            for row in categorized.bulk_update_docs.iter() {
                update_document(&tx, &self.table_name, &self.primary_path, row)?;
            }
            tx.commit().map_err(sqlite_error)?;

            if !categorized.event_bulk.events.is_empty() {
                if let Some(newest) = categorized.newest_row.as_ref() {
                    checkpoint = Some(json!({
                        "id": newest.document.get(&self.primary_path).cloned().unwrap_or(Value::Null),
                        "lwt": newest
                            .document
                            .get("_meta")
                            .and_then(|meta| meta.get("lwt"))
                            .cloned()
                            .unwrap_or(json!(0)),
                    }));
                }
                let mut bulk = categorized.event_bulk;
                bulk.checkpoint = checkpoint.clone();
                event_bulk = Some(bulk);
            }
        }

        if let Some(checkpoint) = checkpoint {
            *self.external_checkpoint.lock() = checkpoint;
        }
        if let Some(bulk) = event_bulk {
            self.changes.next(bulk);
        }
        Ok(ret)
    }

    async fn find_documents_by_id(
        &self,
        ids: &[String],
        with_deleted: bool,
    ) -> Result<Vec<Value>, RxError> {
        self.ensure_open("find_documents_by_id")?;
        let conn = self.connection.lock();
        let mut ret = Vec::new();
        for id in ids {
            if let Some(doc) = document_by_id(&conn, &self.table_name, id)? {
                let deleted = doc
                    .get("_deleted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if with_deleted || !deleted {
                    ret.push(doc);
                }
            }
        }
        Ok(ret)
    }

    async fn query(&self, prepared_query: &Value) -> Result<RxStorageQueryResult, RxError> {
        self.ensure_open("query")?;
        let query: FilledMangoQuery =
            serde_json::from_value(prepared_query.get("query").cloned().unwrap_or(Value::Null))
                .map_err(|err| {
                    new_rx_error(
                        "SQLITE_QUERY",
                        Some(json!({ "message": format!("invalid prepared query: {err}") })),
                    )
                })?;
        let skip = query.skip.unwrap_or(0) as usize;
        let limit = query
            .limit
            .map(|limit| limit as usize)
            .unwrap_or(usize::MAX);
        let skip_plus_limit = skip.saturating_add(limit);
        let matcher = get_query_matcher(&self.schema, &query);
        let comparator = get_sort_comparator(&self.schema, &query);

        let conn = self.connection.lock();
        let mut rows: Vec<Value> = all_documents(&conn, &self.table_name)?
            .into_iter()
            .filter(|doc| matcher(doc))
            .collect();
        rows.sort_by(|a, b| comparator(a, b));
        let start = skip.min(rows.len());
        let end = skip_plus_limit.min(rows.len());
        Ok(RxStorageQueryResult {
            documents: rows[start..end].to_vec(),
        })
    }

    async fn count(&self, prepared_query: &Value) -> Result<RxStorageCountResult, RxError> {
        let result = self.query(prepared_query).await?;
        Ok(RxStorageCountResult {
            count: result.documents.len() as u64,
            mode: "fast".to_string(),
        })
    }

    async fn get_changed_documents_since(
        &self,
        limit: u64,
        checkpoint: Option<&Value>,
    ) -> Result<RxStorageChangedDocumentsSinceResult, RxError> {
        self.ensure_open("get_changed_documents_since")?;
        let conn = self.connection.lock();
        changed_documents_since(
            &conn,
            &self.table_name,
            &self.primary_path,
            limit,
            checkpoint,
        )
    }

    fn change_stream(&self) -> RxStream<EventBulk> {
        self.changes.subscribe()
    }

    async fn cleanup(&self, min_deleted_time: i64) -> Result<bool, RxError> {
        self.ensure_open("cleanup")?;
        cleanup_deleted_documents(&self.connection, &self.table_name, min_deleted_time)
    }

    async fn remove(&self) -> Result<(), RxError> {
        self.ensure_open("remove")?;
        {
            let conn = self.connection.lock();
            drop_table(&conn, &self.table_name)?;
        }
        self.closed.store(true, Ordering::SeqCst);
        unregister_table_notifier(&self.table_name);
        Ok(())
    }

    async fn close(&self) -> Result<(), RxError> {
        self.closed.store(true, Ordering::SeqCst);
        unregister_table_notifier(&self.table_name);
        Ok(())
    }

    async fn get_attachment_data(
        &self,
        _document_id: &str,
        _attachment_id: &str,
        _digest: &str,
    ) -> Result<String, RxError> {
        Err(new_rx_error(
            "SQL1",
            Some(json!({
                "message": "sqlite storage does not inline attachment payloads"
            })),
        ))
    }
}

impl Drop for RxStorageInstanceSqlite {
    fn drop(&mut self) {
        unregister_table_notifier(&self.table_name);
    }
}

#[allow(dead_code)]
fn _zero_use() {
    let _ = random_token(Some(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use crate::rx_query_helper::{normalize_mango_query, prepare_query};
    use crate::storage::sqlite::{
        create_storage_instance, get_rx_storage_sqlite, RxStorageSqliteSettings,
    };
    use crate::types::{JsonSchema, MangoQuery, PrimaryKey, RxStorageInstanceCreationParams};

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
            "age".to_string(),
            JsonSchema {
                schema_type: Some("number".to_string()),
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
            indexes: vec![vec!["age".to_string()]],
            encrypted: Vec::new(),
            internal_indexes: Vec::new(),
            key_compression: false,
            attachments: None,
            additional_properties: true,
            extra: HashMap::new(),
        }
    }

    fn params(schema: RxJsonSchema) -> RxStorageInstanceCreationParams {
        RxStorageInstanceCreationParams {
            database_instance_token: "token".to_string(),
            database_name: "db".to_string(),
            collection_name: "docs".to_string(),
            schema,
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: false,
            password: None,
        }
    }

    fn doc(id: &str, rev: &str, age: i64, deleted: bool, lwt: f64) -> Value {
        json!({
            "id": id,
            "age": age,
            "_rev": rev,
            "_deleted": deleted,
            "_meta": { "lwt": lwt },
            "_attachments": {}
        })
    }

    #[tokio::test]
    async fn persists_documents_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ctox.sqlite3");
        let schema = test_schema();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: path.clone(),
        });
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        instance
            .bulk_write(
                vec![BulkWriteRow {
                    previous: None,
                    document: doc("a", "1-a", 1, false, 1.0),
                }],
                "insert",
            )
            .await
            .unwrap();
        instance.close().await.unwrap();

        let reopened = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: path,
        });
        let instance = create_storage_instance(&reopened, params(schema))
            .await
            .unwrap();
        let docs = instance
            .find_documents_by_id(&["a".to_string()], false)
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].get("age").and_then(Value::as_i64), Some(1));
    }

    #[tokio::test]
    async fn query_filters_sorts_and_limits_documents() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema.clone()))
            .await
            .unwrap();
        instance
            .bulk_write(
                vec![
                    BulkWriteRow {
                        previous: None,
                        document: doc("a", "1-a", 1, false, 1.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("b", "1-b", 3, false, 2.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("c", "1-c", 2, false, 3.0),
                    },
                ],
                "insert",
            )
            .await
            .unwrap();

        let mut sort = HashMap::new();
        sort.insert("age".to_string(), "asc".to_string());
        let filled = normalize_mango_query(
            &schema,
            MangoQuery {
                selector: Some(json!({ "age": { "$gte": 2 } })),
                sort: Some(vec![sort]),
                index: None,
                limit: Some(1),
                skip: Some(0),
            },
        );
        let prepared = prepare_query(&schema, filled).unwrap();
        let result = instance.query(&prepared).await.unwrap();
        assert_eq!(result.documents.len(), 1);
        assert_eq!(
            result.documents[0].get("id").and_then(Value::as_str),
            Some("c")
        );
    }

    #[tokio::test]
    async fn changed_documents_since_uses_lwt_then_id_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema))
            .await
            .unwrap();
        instance
            .bulk_write(
                vec![
                    BulkWriteRow {
                        previous: None,
                        document: doc("a", "1-a", 1, false, 1.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("b", "1-b", 1, false, 1.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("c", "1-c", 1, false, 2.0),
                    },
                ],
                "insert",
            )
            .await
            .unwrap();

        let changed = instance
            .get_changed_documents_since(10, Some(&json!({ "id": "a", "lwt": 1.0 })))
            .await
            .unwrap();
        let ids: Vec<_> = changed
            .documents
            .iter()
            .filter_map(|doc| doc.get("id").and_then(Value::as_str))
            .collect();
        assert_eq!(ids, vec!["b", "c"]);
        assert_eq!(changed.checkpoint, json!({ "id": "c", "lwt": 2.0 }));
    }

    #[tokio::test]
    async fn change_stream_emits_external_sqlite_writes() {
        use tokio::time::timeout;
        use tokio_stream::StreamExt;

        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema))
            .await
            .unwrap();
        let mut stream = instance.change_stream();

        {
            let conn = instance.connection.lock();
            insert_document(
                &conn,
                &instance.table_name,
                &instance.primary_path,
                &doc("external", "1-external", 7, false, 10.0),
            )
            .unwrap();
        }

        let bulk = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("external write should be emitted")
            .expect("change stream should stay open");
        assert_eq!(bulk.context.as_deref(), Some("sqlite-external-poll"));
        assert_eq!(
            bulk.checkpoint,
            Some(json!({ "id": "external", "lwt": 10.0 }))
        );
        assert_eq!(bulk.events.len(), 1);
        assert_eq!(bulk.events[0].document_id, "external");
        assert_eq!(bulk.events[0].operation, "UPDATE");
    }

    #[tokio::test]
    async fn cleanup_removes_old_deleted_documents_only() {
        let dir = tempfile::tempdir().unwrap();
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: dir.path().join("ctox.sqlite3"),
        });
        let schema = test_schema();
        let instance = create_storage_instance(&storage, params(schema))
            .await
            .unwrap();
        instance
            .bulk_write(
                vec![
                    BulkWriteRow {
                        previous: None,
                        document: doc("deleted", "1-a", 1, true, 1.0),
                    },
                    BulkWriteRow {
                        previous: None,
                        document: doc("active", "1-b", 1, false, 1.0),
                    },
                ],
                "insert",
            )
            .await
            .unwrap();

        assert!(instance.cleanup(1).await.unwrap());
        let deleted = instance
            .find_documents_by_id(&["deleted".to_string()], true)
            .await
            .unwrap();
        let active = instance
            .find_documents_by_id(&["active".to_string()], false)
            .await
            .unwrap();
        assert!(deleted.is_empty());
        assert_eq!(active.len(), 1);
    }
}
