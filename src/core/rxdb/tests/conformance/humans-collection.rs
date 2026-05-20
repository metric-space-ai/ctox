use std::collections::HashMap;
use std::sync::Arc;

use rxdb::plugins::storage_memory::index_mod::get_rx_storage_memory;
use rxdb::plugins::utils::utils_string::random_token;
use rxdb::rx_collection::RxCollection;
use rxdb::rx_database::{create_rx_database, RxCollectionCreator, RxDatabase, RxDatabaseCreator};
use rxdb::rx_error::{new_rx_error, RxResult};
use rxdb::types::{
    HashFunction, HashOutput, RxConflictHandler, RxJsonSchema, RxJsonSchemaAttachments, RxStorage,
};
use serde_json::{json, Value};

use crate::schema_objects;
use crate::schemas;

struct TestHashFunction;

impl HashFunction for TestHashFunction {
    fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
        Box::pin(async move { format!("hash:{input}") })
    }
}

pub struct MultipleCollections {
    pub db: Arc<RxDatabase>,
    pub collection: Arc<RxCollection>,
    pub collection2: Arc<RxCollection>,
}

fn default_storage() -> Arc<dyn RxStorage> {
    get_rx_storage_memory(())
}

fn normalize_indexes(schema: &mut Value) {
    let Some(indexes) = schema.get_mut("indexes") else {
        return;
    };
    let Some(indexes_array) = indexes.as_array_mut() else {
        return;
    };

    let normalized = indexes_array
        .iter()
        .map(|index| {
            if let Some(index) = index.as_str() {
                json!([index])
            } else if let Some(parts) = index.as_array() {
                Value::Array(
                    parts
                        .iter()
                        .map(|part| {
                            part.as_str()
                                .map(|part| json!(part))
                                .unwrap_or_else(|| json!(part.to_string()))
                        })
                        .collect(),
                )
            } else {
                json!([index.to_string()])
            }
        })
        .collect();
    *indexes = Value::Array(normalized);
}

fn schema_from_value(mut schema: Value) -> RxResult<RxJsonSchema> {
    normalize_indexes(&mut schema);
    serde_json::from_value(schema.clone()).map_err(|err| {
        new_rx_error(
            "CONFORMANCE_SCHEMA",
            Some(json!({
                "message": err.to_string(),
                "schema": schema
            })),
        )
    })
}

fn with_attachments(mut schema: Value) -> Value {
    schema["attachments"] = json!({});
    schema
}

fn with_key_compression(mut schema: Value, key_compression: bool) -> Value {
    schema["keyCompression"] = json!(key_compression);
    schema
}

async fn create_db(
    name: Option<String>,
    multi_instance: bool,
    event_reduce: bool,
    storage: Option<Arc<dyn RxStorage>>,
    password: Option<String>,
) -> RxResult<Arc<RxDatabase>> {
    create_rx_database(RxDatabaseCreator {
        name: name.unwrap_or_else(|| random_token(Some(10))),
        storage: storage.unwrap_or_else(default_storage),
        multi_instance,
        password,
        hash_function: Arc::new(TestHashFunction),
        options: HashMap::new(),
        ignore_duplicate: true,
        close_duplicates: false,
        event_reduce,
        allow_slow_count: false,
    })
    .await
}

async fn add_collection(
    db: &Arc<RxDatabase>,
    name: &str,
    schema: Value,
    conflict_handler: Option<Arc<dyn RxConflictHandler>>,
) -> RxResult<Arc<RxCollection>> {
    let collections = db
        .add_collections(HashMap::from([(
            name.to_string(),
            RxCollectionCreator {
                schema: schema_from_value(schema)?,
                conflict_handler,
                options: HashMap::new(),
            },
        )]))
        .await?;
    collections.get(name).cloned().ok_or_else(|| {
        new_rx_error(
            "CONFORMANCE_COLLECTION",
            Some(json!({ "collection": name })),
        )
    })
}

async fn seed_collection(
    collection: &Arc<RxCollection>,
    amount: usize,
    mut factory: impl FnMut() -> Value,
) -> RxResult<()> {
    if amount == 0 {
        return Ok(());
    }
    let result = collection
        .bulk_insert((0..amount).map(|_| factory()).collect())
        .await?;
    assert!(
        result.error.is_empty(),
        "seed write should not contain storage errors: {:?}",
        result.error
    );
    Ok(())
}

pub async fn create(
    size: usize,
    collection_name: Option<&str>,
    multi_instance: bool,
    event_reduce: bool,
    storage: Option<Arc<dyn RxStorage>>,
) -> RxResult<Arc<RxCollection>> {
    let name = collection_name.unwrap_or("human");
    let db = create_db(None, multi_instance, event_reduce, storage, None).await?;
    let collection = add_collection(&db, name, schemas::human(), None).await?;
    seed_collection(&collection, size, || {
        schema_objects::human_data(None, None, None)
    })
    .await?;
    Ok(collection)
}

pub async fn create_by_schema(
    schema: Value,
    name: Option<&str>,
    storage: Option<Arc<dyn RxStorage>>,
) -> RxResult<Arc<RxCollection>> {
    let collection_name = name.unwrap_or("human");
    let db = create_db(None, true, true, storage, None).await?;
    add_collection(&db, collection_name, schema, None).await
}

pub async fn create_attachments(
    size: usize,
    name: Option<&str>,
    multi_instance: bool,
) -> RxResult<Arc<RxCollection>> {
    let collection_name = name.unwrap_or("human");
    let db = create_db(None, multi_instance, true, None, None).await?;
    let collection = add_collection(
        &db,
        collection_name,
        with_attachments(schemas::human()),
        None,
    )
    .await?;
    seed_collection(&collection, size, || {
        schema_objects::human_data(None, None, None)
    })
    .await?;
    Ok(collection)
}

pub async fn create_no_compression(size: usize, name: Option<&str>) -> RxResult<Arc<RxCollection>> {
    let collection_name = name.unwrap_or("human");
    let db = create_db(None, true, true, None, None).await?;
    let collection = add_collection(
        &db,
        collection_name,
        with_key_compression(schemas::human(), false),
        None,
    )
    .await?;
    seed_collection(&collection, size, || {
        schema_objects::human_data(None, None, None)
    })
    .await?;
    Ok(collection)
}

pub async fn create_age_index(amount: usize) -> RxResult<Arc<RxCollection>> {
    let db = create_db(None, true, true, None, None).await?;
    let collection = add_collection(&db, "humana", schemas::human_age_index(), None).await?;
    seed_collection(&collection, amount, || {
        schema_objects::human_data(None, None, None)
    })
    .await?;
    Ok(collection)
}

pub async fn multiple_on_same_db(size: usize) -> RxResult<MultipleCollections> {
    let db = create_db(None, true, true, None, None).await?;
    let collections = db
        .add_collections(HashMap::from([
            (
                "human".to_string(),
                RxCollectionCreator {
                    schema: schema_from_value(schemas::human())?,
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            ),
            (
                "human2".to_string(),
                RxCollectionCreator {
                    schema: schema_from_value(schemas::human())?,
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            ),
        ]))
        .await?;
    let collection = collections.get("human").unwrap().clone();
    let collection2 = collections.get("human2").unwrap().clone();
    seed_collection(&collection, size, || {
        schema_objects::human_data(None, None, None)
    })
    .await?;
    seed_collection(&collection2, size, || {
        schema_objects::human_data(None, None, None)
    })
    .await?;
    Ok(MultipleCollections {
        db,
        collection,
        collection2,
    })
}

pub async fn create_nested(amount: usize) -> RxResult<Arc<RxCollection>> {
    let db = create_db(None, true, true, None, None).await?;
    let collection = add_collection(&db, "nestedhuman", schemas::nested_human(), None).await?;
    seed_collection(&collection, amount, || {
        schema_objects::nested_human_data(None)
    })
    .await?;
    Ok(collection)
}

pub async fn create_deep_nested(amount: usize) -> RxResult<Arc<RxCollection>> {
    let db = create_db(None, true, true, None, None).await?;
    let collection = add_collection(&db, "nestedhuman", schemas::deep_nested_human(), None).await?;
    seed_collection(&collection, amount, schema_objects::deep_nested_human_data).await?;
    Ok(collection)
}

pub async fn create_multi_instance(
    name: &str,
    amount: usize,
    password: Option<String>,
    storage: Option<Arc<dyn RxStorage>>,
) -> RxResult<Arc<RxCollection>> {
    let db = create_db(Some(name.to_string()), true, true, storage, password).await?;
    let collection = add_collection(&db, "human", schemas::human(), None).await?;
    seed_collection(&collection, amount, || {
        schema_objects::human_data(None, None, None)
    })
    .await?;
    Ok(collection)
}

pub async fn create_primary(
    amount: usize,
    name: Option<String>,
    multi_instance: bool,
) -> RxResult<Arc<RxCollection>> {
    let db = create_db(name, multi_instance, true, None, None).await?;
    let collection = add_collection(&db, "human", schemas::primary_human(), None).await?;
    seed_collection(&collection, amount, schema_objects::simple_human_data).await?;
    Ok(collection)
}

pub async fn create_human_with_timestamp(
    amount: usize,
    database_name: Option<String>,
    multi_instance: bool,
    storage: Option<Arc<dyn RxStorage>>,
    conflict_handler: Option<Arc<dyn RxConflictHandler>>,
) -> RxResult<Arc<RxCollection>> {
    let db = create_db(database_name, multi_instance, true, storage, None).await?;
    let collection = add_collection(
        &db,
        "humans",
        schemas::human_with_timestamp(),
        conflict_handler,
    )
    .await?;
    seed_collection(&collection, amount, || {
        schema_objects::human_with_timestamp_data(None)
    })
    .await?;
    Ok(collection)
}

pub async fn create_migration_collection(
    amount: usize,
    name: Option<String>,
    auto_migrate: bool,
    with_attachment: bool,
) -> RxResult<Arc<RxCollection>> {
    let database_name = name.unwrap_or_else(|| random_token(Some(10)));
    let storage = default_storage();
    let db = create_db(
        Some(database_name.clone()),
        true,
        true,
        Some(Arc::clone(&storage)),
        None,
    )
    .await?;
    let schema = if with_attachment {
        with_attachments(schemas::simple_human())
    } else {
        schemas::simple_human()
    };
    let collection = add_collection(&db, "human", schema, None).await?;
    seed_collection(&collection, amount, || {
        schema_objects::simple_human_age(None)
    })
    .await?;
    db.close().await?;

    let db2 = create_db(Some(database_name), true, true, Some(storage), None).await?;
    let mut next_schema = schemas::simple_human_v3();
    if with_attachment {
        next_schema = with_attachments(next_schema);
    }
    if auto_migrate {
        next_schema["autoMigrate"] = json!(true);
    }
    add_collection(&db2, "human", next_schema, None).await
}

pub async fn create_related(name: Option<String>) -> RxResult<Arc<RxCollection>> {
    let db = create_db(name, true, true, None, None).await?;
    let collection = add_collection(&db, "human", schemas::ref_human(), None).await?;
    let mut doc1 = schema_objects::ref_human_data(None);
    let doc1_name = doc1["name"].as_str().unwrap().to_string();
    let doc2 = schema_objects::ref_human_data(Some(&doc1_name));
    let doc2_name = doc2["name"].as_str().unwrap().to_string();
    doc1["bestFriend"] = json!(doc2_name);
    collection.insert(doc1).await?;
    collection.insert(doc2).await?;
    Ok(collection)
}

pub async fn create_related_nested(name: Option<String>) -> RxResult<Arc<RxCollection>> {
    let db = create_db(name, true, true, None, None).await?;
    let collection = add_collection(&db, "human", schemas::ref_human_nested(), None).await?;
    let mut doc1 = schema_objects::ref_human_nested_data(None);
    let doc1_name = doc1["name"].as_str().unwrap().to_string();
    let doc2 = schema_objects::ref_human_nested_data(Some(&doc1_name));
    let doc2_name = doc2["name"].as_str().unwrap().to_string();
    doc1["foo"]["bestFriend"] = json!(doc2_name);
    collection.insert(doc1).await?;
    collection.insert(doc2).await?;
    Ok(collection)
}

pub async fn create_id_and_age_index(amount: usize) -> RxResult<Arc<RxCollection>> {
    let db = create_db(None, true, true, None, None).await?;
    let collection = add_collection(&db, "humana", schemas::human_id_and_age_index(), None).await?;
    seed_collection(&collection, amount, || {
        schema_objects::human_with_id_and_age_index_document_type(None)
    })
    .await?;
    Ok(collection)
}

pub async fn create_human_with_ownership(
    amount: usize,
    database_name: Option<String>,
    multi_instance: bool,
    owner: &str,
    storage: Option<Arc<dyn RxStorage>>,
    conflict_handler: Option<Arc<dyn RxConflictHandler>>,
) -> RxResult<Arc<RxCollection>> {
    let db = create_db(database_name, multi_instance, true, storage, None).await?;
    let collection = add_collection(
        &db,
        "humans",
        schemas::human_with_ownership(),
        conflict_handler,
    )
    .await?;
    seed_collection(&collection, amount, || {
        schema_objects::human_with_ownership_data(None, owner)
    })
    .await?;
    Ok(collection)
}

pub fn attachments_schema(schema: &Value) -> Value {
    let mut ret = schema.clone();
    ret["attachments"] = serde_json::to_value(RxJsonSchemaAttachments::default()).unwrap();
    ret
}

#[tokio::test]
async fn create_human_collection_seeds_requested_amount() {
    let collection = create(3, Some("human"), true, true, None)
        .await
        .expect("human collection");
    assert_eq!(
        collection.count(None).unwrap().exec(false).await.unwrap(),
        json!(3)
    );
}

#[tokio::test]
async fn specialized_collection_factories_seed_expected_collections() {
    let age = create_age_index(2).await.expect("age collection");
    assert_eq!(
        age.count(None).unwrap().exec(false).await.unwrap(),
        json!(2)
    );

    let nested = create_nested(1).await.expect("nested collection");
    assert_eq!(
        nested.count(None).unwrap().exec(false).await.unwrap(),
        json!(1)
    );

    let related = create_related(None).await.expect("related collection");
    assert_eq!(
        related.count(None).unwrap().exec(false).await.unwrap(),
        json!(2)
    );
}

#[tokio::test]
async fn multiple_same_db_and_zero_size_factories_work() {
    let collections = multiple_on_same_db(2).await.expect("multiple collections");
    assert_eq!(
        collections
            .collection
            .count(None)
            .unwrap()
            .exec(false)
            .await
            .unwrap(),
        json!(2)
    );
    assert_eq!(
        collections
            .collection2
            .count(None)
            .unwrap()
            .exec(false)
            .await
            .unwrap(),
        json!(2)
    );
    assert!(!collections.db.closed());

    let primary = create_primary(0, None, true)
        .await
        .expect("primary collection");
    assert_eq!(
        primary.count(None).unwrap().exec(false).await.unwrap(),
        json!(0)
    );
}

#[tokio::test]
async fn timestamp_and_ownership_factories_preserve_special_fields() {
    let timestamp = create_human_with_timestamp(1, None, true, None, None)
        .await
        .expect("timestamp collection");
    let docs = timestamp.find(None).unwrap().exec(false).await.unwrap();
    assert!(docs.as_array().unwrap()[0]["updatedAt"].is_number());

    let ownership = create_human_with_ownership(1, None, true, "alice", None, None)
        .await
        .expect("ownership collection");
    let docs = ownership.find(None).unwrap().exec(false).await.unwrap();
    assert_eq!(docs.as_array().unwrap()[0]["owner"], "alice");
}

#[tokio::test]
async fn remaining_factory_surface_creates_collections() {
    let by_schema = create_by_schema(schemas::human_minimal(), Some("minimal"), None)
        .await
        .expect("by schema");
    assert_eq!(
        by_schema.count(None).unwrap().exec(false).await.unwrap(),
        json!(0)
    );

    let attachments = create_attachments(0, Some("attachhuman"), true)
        .await
        .expect("attachments");
    assert_eq!(
        attachments.count(None).unwrap().exec(false).await.unwrap(),
        json!(0)
    );
    assert!(attachments_schema(&schemas::human())["attachments"].is_object());

    let no_compression = create_no_compression(1, Some("nocompress"))
        .await
        .expect("no compression");
    assert_eq!(
        no_compression
            .count(None)
            .unwrap()
            .exec(false)
            .await
            .unwrap(),
        json!(1)
    );

    let deep = create_deep_nested(1).await.expect("deep nested");
    assert_eq!(
        deep.count(None).unwrap().exec(false).await.unwrap(),
        json!(1)
    );

    let multi = create_multi_instance(&random_token(Some(10)), 1, None, None)
        .await
        .expect("multi instance");
    assert_eq!(
        multi.count(None).unwrap().exec(false).await.unwrap(),
        json!(1)
    );

    let migration = create_migration_collection(0, None, false, false)
        .await
        .expect("migration collection");
    assert_eq!(
        migration.count(None).unwrap().exec(false).await.unwrap(),
        json!(0)
    );

    let related_nested = create_related_nested(None).await.expect("related nested");
    assert_eq!(
        related_nested
            .count(None)
            .unwrap()
            .exec(false)
            .await
            .unwrap(),
        json!(2)
    );

    let id_age = create_id_and_age_index(1).await.expect("id age");
    assert_eq!(
        id_age.count(None).unwrap().exec(false).await.unwrap(),
        json!(1)
    );
}
