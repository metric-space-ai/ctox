//! Exercise native snapshots through the actual database collection wrapper.
use rxdb::{
    rx_database::{create_rx_database, RxCollectionCreator, RxDatabaseCreator},
    rx_query_helper::{normalize_mango_query, prepare_query},
    storage::sqlite::{get_rx_storage_sqlite, sql, RxStorageSqliteSettings},
    types::{HashFunction, HashOutput, MangoQuery, RxStorageSnapshotEvent},
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};

struct Hash;
impl HashFunction for Hash {
    fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
        Box::pin(async move { rxdb::plugins::utils::utils_hash::native_sha256(&input) })
    }
}

#[tokio::test]
async fn wrapped_collection_keeps_snapshot_boundary_and_cancellation() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("native.sqlite");
    let name = "wrapped-native-snapshot";
    let database = create_rx_database(RxDatabaseCreator {
        name: name.into(),
        storage: get_rx_storage_sqlite(RxStorageSqliteSettings {
            database_path: path.clone(),
        }),
        multi_instance: false,
        password: None,
        hash_function: Arc::new(Hash),
        options: HashMap::new(),
        ignore_duplicate: false,
        close_duplicates: false,
        event_reduce: false,
        allow_slow_count: false,
    })
    .await
    .unwrap();
    database
        .add_collections(HashMap::from([(
            "records".into(),
            RxCollectionCreator {
                schema: serde_json::from_value(json!({
                    "version": 0, "primaryKey": "id", "type": "object",
                    "properties": { "id": { "type": "string", "maxLength": 64 } },
                    "required": ["id"]
                }))
                .unwrap(),
                conflict_handler: None,
                options: HashMap::new(),
            },
        )]))
        .await
        .unwrap();
    let collection = database.collection("records").unwrap();
    for index in 0..5 {
        collection
            .insert(json!({"id": format!("row-{index}")}))
            .await
            .unwrap();
    }
    let schema = &collection.schema.as_ref().unwrap().json_schema;
    let query = prepare_query(
        schema,
        normalize_mango_query(
            schema,
            MangoQuery {
                selector: Some(json!({"_deleted": false})),
                sort: Some(vec![HashMap::from([("id".into(), "asc".into())])]),
                index: None,
                limit: None,
                skip: Some(0),
            },
        ),
    )
    .unwrap();
    let storage = collection.storage_instance.clone();
    let prepared = query.clone();
    let events = tokio::task::spawn_blocking(move || {
        let mut events = Vec::new();
        let result = storage.query_snapshot_stream_into_blocking(&prepared, 2, &mut |event| {
            if matches!(event, RxStorageSnapshotEvent::Start { .. }) {
                let connection = rusqlite::Connection::open(&path).unwrap();
                sql::insert_document(
                    &connection,
                    &sql::table_name(name, "records", 0),
                    "id",
                    &json!({"id":"row-late","_deleted":false,"_rev":"1-late",
                        "_meta":{"lwt":10.0},"_attachments":{}}),
                )
                .unwrap();
            }
            events.push(event);
            Ok(true)
        });
        result
            .expect("the actual wrapped SQLite collection supports snapshots")
            .unwrap();
        events
    })
    .await
    .unwrap();
    assert!(matches!(
        events.first(),
        Some(RxStorageSnapshotEvent::Start { change_counter: 5 })
    ));
    assert!(matches!(events.last(), Some(RxStorageSnapshotEvent::End)));
    let pages: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            RxStorageSnapshotEvent::Documents(documents) => Some(documents),
            _ => None,
        })
        .collect();
    assert_eq!(
        pages.iter().map(|page| page.len()).collect::<Vec<_>>(),
        [2, 2, 1]
    );
    assert_eq!(
        pages
            .into_iter()
            .flatten()
            .map(|document| document["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["row-0", "row-1", "row-2", "row-3", "row-4"]
    );
    assert_eq!(
        collection
            .storage_instance
            .query(&query)
            .await
            .unwrap()
            .documents
            .len(),
        6,
        "the concurrent committed row exists but was outside the pinned snapshot"
    );

    let storage = collection.storage_instance.clone();
    let cancelled = tokio::task::spawn_blocking(move || {
        let mut events = Vec::new();
        let result = storage.query_snapshot_stream_into_blocking(&query, 2, &mut |event| {
            let keep_reading = !matches!(event, RxStorageSnapshotEvent::Documents(_));
            events.push(event);
            Ok(keep_reading)
        });
        result.expect("wrapped snapshots remain supported").unwrap();
        events
    })
    .await
    .unwrap();
    assert!(matches!(
        cancelled.first(),
        Some(RxStorageSnapshotEvent::Start { change_counter: 6 })
    ));
    assert_eq!(cancelled.len(), 2);
    assert!(
        matches!(cancelled.last(), Some(RxStorageSnapshotEvent::Documents(documents)) if documents.len() == 2)
    );
    assert!(!cancelled
        .iter()
        .any(|event| matches!(event, RxStorageSnapshotEvent::End)));
    database.close().await.unwrap();
}
