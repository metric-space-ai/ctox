//! Rust-native port of `src/rx-database.ts`.
//!
//! The core database lifecycle is implemented around typed Rust ownership:
//! `create_rx_database`, internal-store/storage-token setup, duplicate-open
//! handling, collection registration/removal, event-bulk fan-in, idle locking,
//! and close/remove flows. Plugin-only methods such as JSON dump, backup,
//! state, leader-election handles, and migration-state enumeration remain
//! explicit `PLUGIN_MISSING` boundaries until CTOX ships those plugins.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::LazyLock;
use std::sync::{Arc, Weak};

use futures::{stream, StreamExt};
use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::Mutex as TokioMutex;

use crate::plugins::utils::utils_document::{get_default_revision, get_default_rx_document_meta};
use crate::plugins::utils::utils_error::plugin_missing;
use crate::plugins::utils::utils_object::sort_object;
use crate::plugins::utils::utils_revision::create_revision;
use crate::plugins::utils::utils_string::random_token;
use crate::plugins::utils::utils_time::now;
use crate::replication_protocol::DefaultConflictHandler;
use crate::rx_collection::RxCollection;
use crate::rx_collection_helper::create_rx_collection_storage_instance;
use crate::rx_collection_helper::remove_collection_storages;
use crate::rx_database_internal_store::{
    build_internal_store_schema, collection_name_primary, ensure_storage_token_document_exists,
    get_all_collection_documents, get_primary_key_of_internal_document, storage_token_document_id,
    INTERNAL_CONTEXT_COLLECTION,
};
use crate::rx_error::{new_rx_error, RxError, RxResult};
use crate::rx_schema::{create_rx_schema, RxSchema};
use crate::rx_storage_helper::{
    flat_clone_doc_with_meta, get_single_document, get_wrapped_storage_instance,
    INTERNAL_STORAGE_NAME,
};
use crate::rxjs_compat::RxStream;
use crate::types::{
    BulkWriteRow, RxChangeEventBulk, RxConflictHandler, RxJsonSchema, RxStorage, RxStorageInstance,
    RxStorageInstanceCreationParams, SharedHashFunction,
};

static OPEN_DATABASES: LazyLock<Mutex<HashMap<String, Vec<Weak<RxDatabase>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static DB_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
pub struct RxDatabaseCreator {
    pub name: String,
    pub storage: Arc<dyn RxStorage>,
    pub multi_instance: bool,
    pub password: Option<String>,
    pub hash_function: SharedHashFunction,
    pub options: HashMap<String, Value>,
    pub ignore_duplicate: bool,
    pub close_duplicates: bool,
    pub event_reduce: bool,
    pub allow_slow_count: bool,
}

#[derive(Clone)]
pub struct RxCollectionCreator {
    pub schema: RxJsonSchema,
    pub conflict_handler: Option<Arc<dyn RxConflictHandler>>,
    pub options: HashMap<String, Value>,
}

pub struct RxDatabase {
    pub name: String,
    pub token: String,
    pub storage_token: String,
    pub multi_instance: bool,
    pub hash_function: SharedHashFunction,
    pub storage: Arc<dyn RxStorage>,
    /// `None` until `rx_database_internal_store::create_rx_database_storage_instance`
    /// runs at DB-create time (phase-6 wires it). `rx_database_internal_store`
    /// helpers take an explicit `internal_store` argument so they work before
    /// the full DB lifecycle lands.
    pub internal_store: Option<Arc<dyn RxStorageInstance>>,
    /// Optional encryption password (hashed before write).
    pub password: Option<String>,
    /// RxDB code version — written into the storage-token doc to gate
    /// state-vs-code compatibility checks.
    pub rxdb_version: String,
    pub event_reduce: bool,
    pub allow_slow_count: bool,
    pub collections: Mutex<HashMap<String, Arc<RxCollection>>>,
    startup_errors: Mutex<Vec<crate::rx_error::RxError>>,
    /// Upstream `storageInstances` set. The Rust port currently tracks the
    /// live count; later collection lifecycle work can replace this with a
    /// typed registry once callers need enumeration.
    storage_instances: AtomicUsize,
    closed: AtomicBool,
    /// Upstream `idleQueue.wrapCall()` backing for `lockedRun()`.
    idle_queue: TokioMutex<()>,
}

impl RxDatabase {
    /// Builder-style minimal constructor.
    pub fn new(
        name: impl Into<String>,
        token: impl Into<String>,
        storage_token: impl Into<String>,
        multi_instance: bool,
        hash_function: SharedHashFunction,
        storage: Arc<dyn RxStorage>,
    ) -> Arc<Self> {
        Self::new_with_query_options(
            name,
            token,
            storage_token,
            multi_instance,
            hash_function,
            storage,
            true,
            false,
        )
    }

    pub fn new_with_query_options(
        name: impl Into<String>,
        token: impl Into<String>,
        storage_token: impl Into<String>,
        multi_instance: bool,
        hash_function: SharedHashFunction,
        storage: Arc<dyn RxStorage>,
        event_reduce: bool,
        allow_slow_count: bool,
    ) -> Arc<Self> {
        DB_COUNT.fetch_add(1, Ordering::SeqCst);
        Arc::new(Self {
            name: name.into(),
            token: token.into(),
            storage_token: storage_token.into(),
            multi_instance,
            hash_function,
            storage,
            internal_store: None,
            password: None,
            rxdb_version: crate::plugins::utils::utils_rxdb_version::RXDB_VERSION.to_string(),
            event_reduce,
            allow_slow_count,
            collections: Mutex::new(HashMap::new()),
            startup_errors: Mutex::new(Vec::new()),
            storage_instances: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            idle_queue: TokioMutex::new(()),
        })
    }

    /// `RxDatabase.waitForLeadership()` — single-process CTOX returns true
    /// immediately (see `leader_election` plugin).
    pub async fn wait_for_leadership(&self) -> bool {
        crate::plugins::leader_election::wait_for_leadership(self.multi_instance).await
    }

    /// `RxDatabase.isLeader()` — always true for single-process CTOX.
    pub fn is_leader(&self) -> bool {
        crate::plugins::leader_election::is_leader(self.multi_instance)
    }

    // ref: rxdb/src/rx-database.ts plugin-backed methods
    pub async fn export_json(&self, _collections: Option<Vec<String>>) -> RxResult<Value> {
        Err(plugin_missing_rx_error("json-dump"))
    }

    pub async fn import_json(&self, _exported_json: Value) -> RxResult<()> {
        Err(plugin_missing_rx_error("json-dump"))
    }

    pub async fn add_state(&self, _name: Option<String>) -> RxResult<Value> {
        Err(plugin_missing_rx_error("state"))
    }

    pub fn backup(&self, _options: Value) -> RxResult<Value> {
        Err(plugin_missing_rx_error("backup"))
    }

    pub fn leader_elector(&self) -> RxResult<Value> {
        Err(plugin_missing_rx_error("leader-election"))
    }

    pub fn migration_states(&self) -> RxResult<Value> {
        Err(plugin_missing_rx_error("migration-schema"))
    }

    /// `RxDatabase.lockedRun(fn)` — serialize storage work through the DB idle
    /// queue, mirroring upstream's `IdleQueue.wrapCall`.
    pub async fn locked_run<F, Fut, T>(&self, f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let _guard = self.idle_queue.lock().await;
        f().await
    }

    /// Upstream `requestIdlePromise()` waits until the DB idle queue is empty.
    pub async fn request_idle_promise(&self) {
        self.locked_run(|| async {}).await;
    }

    /// CTOX has no browser idle callback. The no-queue variant resolves
    /// immediately, matching the server-side execution model.
    pub async fn request_idle_promise_no_queue(&self) {}

    pub fn register_storage_instance(&self) {
        self.storage_instances.fetch_add(1, Ordering::SeqCst);
    }

    pub fn unregister_storage_instance(&self) {
        let _ =
            self.storage_instances
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                    Some(current.saturating_sub(1))
                });
    }

    pub fn storage_instance_count(&self) -> usize {
        self.storage_instances.load(Ordering::SeqCst)
    }

    pub fn collection(&self, name: &str) -> Option<Arc<RxCollection>> {
        self.collections.lock().get(name).cloned()
    }

    pub fn collection_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.collections.lock().keys().cloned().collect();
        names.sort();
        names
    }

    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    // ref: rxdb/src/rx-database.ts startupErrors / ensureNoStartupErrors
    pub fn add_startup_error(&self, error: crate::rx_error::RxError) {
        self.startup_errors.lock().push(error);
    }

    pub async fn ensure_no_startup_errors(&self) -> RxResult<()> {
        if let Some(error) = self.startup_errors.lock().first().cloned() {
            Err(error)
        } else {
            Ok(())
        }
    }

    pub fn event_bulks(&self) -> RxStream<RxChangeEventBulk> {
        let streams: Vec<RxStream<RxChangeEventBulk>> = self
            .collections
            .lock()
            .values()
            .cloned()
            .map(|collection| {
                let collection_name = collection.name.clone();
                Box::pin(collection.event_bulks().map(move |bulk| RxChangeEventBulk {
                    id: bulk.id,
                    events: bulk.events,
                    collection_name: collection_name.clone(),
                    is_local: false,
                    checkpoint: bulk.checkpoint,
                    context: bulk.context,
                })) as RxStream<RxChangeEventBulk>
            })
            .collect();
        Box::pin(stream::select_all(streams))
    }

    pub async fn add_collections(
        self: &Arc<Self>,
        collection_creators: HashMap<String, RxCollectionCreator>,
    ) -> RxResult<HashMap<String, Arc<RxCollection>>> {
        let internal_store = self.internal_store.as_ref().cloned().ok_or_else(|| {
            new_rx_error("DB_INTERNAL_STORE", Some(json!({ "database": self.name })))
        })?;
        let mut created = HashMap::new();

        for (name, creator) in collection_creators {
            let collection = self
                .add_single_collection(&internal_store, name, creator)
                .await?;
            created.insert(collection.name.clone(), collection);
        }

        Ok(created)
    }

    /// FIX 4: per-collection fault-tolerant registration. Each collection is
    /// registered independently; a collection that fails (e.g. genuine schema
    /// drift returning `DB6`) is reported in the returned error map and
    /// SKIPPED, while every other collection still comes up. This lets the
    /// native peer bring up required collections (and the rest) even when an
    /// optional collection has drifted, instead of aborting the whole peer on
    /// the first failure. The strict `add_collections` is unchanged for other
    /// callers, and the auto-repair-when-structurally-identical path inside
    /// `write_collection_meta` is reused untouched.
    pub async fn add_collections_tolerant(
        self: &Arc<Self>,
        collection_creators: HashMap<String, RxCollectionCreator>,
    ) -> RxResult<(HashMap<String, Arc<RxCollection>>, HashMap<String, RxError>)> {
        let internal_store = self.internal_store.as_ref().cloned().ok_or_else(|| {
            new_rx_error("DB_INTERNAL_STORE", Some(json!({ "database": self.name })))
        })?;
        let mut created = HashMap::new();
        let mut failed = HashMap::new();

        for (name, creator) in collection_creators {
            match self
                .add_single_collection(&internal_store, name.clone(), creator)
                .await
            {
                Ok(collection) => {
                    created.insert(collection.name.clone(), collection);
                }
                Err(err) => {
                    failed.insert(name, err);
                }
            }
        }

        Ok((created, failed))
    }

    /// FIX 4: register exactly one collection. Extracted from `add_collections`
    /// so the strict and fault-tolerant entry points share identical
    /// per-collection semantics (including the existing auto-repair path).
    async fn add_single_collection(
        self: &Arc<Self>,
        internal_store: &Arc<dyn RxStorageInstance>,
        name: String,
        creator: RxCollectionCreator,
    ) -> RxResult<Arc<RxCollection>> {
        if self.collections.lock().contains_key(&name) {
            return Err(new_rx_error("DB3", Some(json!({ "name": name }))));
        }

        let schema = Arc::new(create_rx_schema(
            creator.schema,
            Arc::clone(&self.hash_function),
            true,
        )?);
        write_collection_meta(self, internal_store, &name, &schema).await?;

        let raw_storage_instance = create_rx_collection_storage_instance(
            &self.storage,
            self.multi_instance,
            RxStorageInstanceCreationParams {
                database_instance_token: self.token.clone(),
                database_name: self.name.clone(),
                collection_name: name.clone(),
                schema: schema.json_schema.clone(),
                options: creator.options,
                multi_instance: self.multi_instance,
                dev_mode: false,
                password: self.password.clone(),
            },
        )
        .await?;
        let storage_instance = get_wrapped_storage_instance(
            Arc::clone(self),
            raw_storage_instance,
            schema.json_schema.clone(),
        );
        let conflict_handler = creator
            .conflict_handler
            .unwrap_or_else(|| Arc::new(DefaultConflictHandler));
        let collection = RxCollection::new_with_schema(
            name.clone(),
            Arc::clone(self),
            storage_instance,
            conflict_handler,
            schema,
        );
        self.collections
            .lock()
            .insert(name.clone(), Arc::clone(&collection));
        Ok(collection)
    }

    // ref: rxdb/src/rx-database.ts removeCollectionDoc
    pub async fn remove_collection_doc(&self, name: &str, schema: &RxJsonSchema) -> RxResult<()> {
        let internal_store = self.internal_store.as_ref().ok_or_else(|| {
            new_rx_error("DB_INTERNAL_STORE", Some(json!({ "database": self.name })))
        })?;
        let collection_name_with_version = collection_name_primary(name, schema);
        let document_id = get_primary_key_of_internal_document(
            &collection_name_with_version,
            INTERNAL_CONTEXT_COLLECTION,
        );
        let doc = get_single_document(internal_store.as_ref(), &document_id)
            .await?
            .ok_or_else(|| new_rx_error("SNH", Some(json!({ "name": name, "schema": schema }))))?;
        let mut write_doc = flat_clone_doc_with_meta(&doc);
        if let Some(obj) = write_doc.as_object_mut() {
            obj.insert("_deleted".to_string(), Value::Bool(true));
            if let Some(meta) = obj.get_mut("_meta").and_then(Value::as_object_mut) {
                meta.insert("lwt".to_string(), json!(now()));
            }
            let previous_rev = doc.get("_rev").and_then(Value::as_str);
            let revision = create_revision(&self.token, previous_rev).unwrap_or_default();
            obj.insert("_rev".to_string(), Value::String(revision));
        }
        let result = internal_store
            .bulk_write(
                vec![BulkWriteRow {
                    previous: Some(doc),
                    document: write_doc,
                }],
                "rx-database-remove-collection",
            )
            .await?;
        if let Some(error) = result.error.first() {
            return Err(new_rx_error(
                "DB_REMOVE_COLLECTION_DOC",
                Some(json!({ "database": self.name, "writeError": error })),
            ));
        }
        Ok(())
    }

    pub async fn close(&self) -> RxResult<()> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let _ = DB_COUNT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            Some(current.saturating_sub(1))
        });
        let collections: Vec<Arc<RxCollection>> =
            self.collections.lock().values().cloned().collect();
        for collection in collections {
            collection.close();
            collection.storage_instance.close().await?;
        }
        if let Some(internal_store) = &self.internal_store {
            internal_store.close().await?;
        }
        unregister_open_database(self);
        Ok(())
    }

    // ref: rxdb/src/rx-database.ts isRxDatabaseFirstTimeInstantiated
    pub async fn is_first_time_instantiated(&self) -> RxResult<bool> {
        let internal_store = self.internal_store.as_ref().ok_or_else(|| {
            new_rx_error("DB_INTERNAL_STORE", Some(json!({ "database": self.name })))
        })?;
        let docs = internal_store
            .find_documents_by_id(&[storage_token_document_id()], false)
            .await?;
        let instance_token = docs
            .first()
            .and_then(|doc| doc.get("data"))
            .and_then(|data| data.get("instanceToken"))
            .and_then(Value::as_str);
        Ok(instance_token == Some(self.token.as_str()))
    }
}

// ref: rxdb/src/rx-database.ts dbCount
pub fn db_count() -> usize {
    DB_COUNT.load(Ordering::SeqCst)
}

fn plugin_missing_rx_error(plugin_key: &str) -> crate::rx_error::RxError {
    let err = plugin_missing(plugin_key);
    new_rx_error(
        "PLUGIN_MISSING",
        Some(json!({
            "plugin": plugin_key,
            "message": err.to_string(),
        })),
    )
}

pub async fn create_rx_database(creator: RxDatabaseCreator) -> RxResult<Arc<RxDatabase>> {
    let database_name = creator.name.clone();

    let duplicates = live_open_databases(&database_name);
    if creator.close_duplicates {
        for database in duplicates {
            database.close().await?;
        }
    } else if !creator.ignore_duplicate && !duplicates.is_empty() {
        return Err(new_rx_error(
            "DB8",
            Some(json!({ "database": database_name })),
        ));
    }

    let result: RxResult<Arc<RxDatabase>> = async {
        let database_instance_token = random_token(Some(10));
        let internal_store = create_rx_database_storage_instance(
            &database_instance_token,
            &creator.storage,
            &creator.name,
            creator.options.clone(),
            creator.multi_instance,
            creator.password.clone(),
        )
        .await?;
        let storage_token_doc = ensure_storage_token_document_exists(
            &internal_store,
            &creator.hash_function,
            &creator.name,
            &database_instance_token,
            creator.password.as_deref(),
            crate::plugins::utils::utils_rxdb_version::RXDB_VERSION,
        )
        .await?;
        let storage_token = storage_token_doc
            .get("data")
            .and_then(|data| data.get("token"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        DB_COUNT.fetch_add(1, Ordering::SeqCst);
        let database = Arc::new(RxDatabase {
            name: creator.name,
            token: database_instance_token,
            storage_token,
            multi_instance: creator.multi_instance,
            hash_function: creator.hash_function,
            storage: creator.storage,
            internal_store: Some(internal_store),
            password: creator.password,
            rxdb_version: crate::plugins::utils::utils_rxdb_version::RXDB_VERSION.to_string(),
            event_reduce: creator.event_reduce,
            allow_slow_count: creator.allow_slow_count,
            collections: Mutex::new(HashMap::new()),
            startup_errors: Mutex::new(Vec::new()),
            storage_instances: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            idle_queue: TokioMutex::new(()),
        });
        register_open_database(&database_name, &database);
        Ok(database)
    }
    .await;
    result
}

fn live_open_databases(name: &str) -> Vec<Arc<RxDatabase>> {
    let mut open = OPEN_DATABASES.lock();
    let Some(entries) = open.get_mut(name) else {
        return Vec::new();
    };
    let mut live = Vec::new();
    entries.retain(|weak| {
        if let Some(database) = weak.upgrade() {
            if !database.closed() {
                live.push(database);
                return true;
            }
        }
        false
    });
    if entries.is_empty() {
        open.remove(name);
    }
    live
}

fn register_open_database(name: &str, database: &Arc<RxDatabase>) {
    let mut open = OPEN_DATABASES.lock();
    open.entry(name.to_string())
        .or_default()
        .push(Arc::downgrade(database));
}

fn unregister_open_database(database: &RxDatabase) {
    let mut open = OPEN_DATABASES.lock();
    let Some(entries) = open.get_mut(&database.name) else {
        return;
    };
    entries.retain(|weak| {
        weak.upgrade()
            .is_some_and(|open_database| !std::ptr::eq(open_database.as_ref(), database))
    });
    if entries.is_empty() {
        open.remove(&database.name);
    }
}

pub async fn create_rx_database_storage_instance(
    database_instance_token: &str,
    storage: &Arc<dyn RxStorage>,
    database_name: &str,
    options: HashMap<String, Value>,
    multi_instance: bool,
    password: Option<String>,
) -> RxResult<Arc<dyn RxStorageInstance>> {
    storage
        .create_storage_instance(RxStorageInstanceCreationParams {
            database_instance_token: database_instance_token.to_string(),
            database_name: database_name.to_string(),
            collection_name: INTERNAL_STORAGE_NAME.to_string(),
            schema: build_internal_store_schema(),
            options,
            multi_instance,
            dev_mode: false,
            password,
        })
        .await
}

pub async fn remove_rx_database(
    database_name: &str,
    storage: &Arc<dyn RxStorage>,
    multi_instance: bool,
    password: Option<String>,
) -> RxResult<Vec<String>> {
    let database_instance_token = random_token(Some(10));
    let internal_store = create_rx_database_storage_instance(
        &database_instance_token,
        storage,
        database_name,
        HashMap::new(),
        multi_instance,
        password.clone(),
    )
    .await?;
    let collection_docs = get_all_collection_documents(&internal_store).await?;
    let mut collection_names: Vec<String> = collection_docs
        .iter()
        .filter_map(|doc| {
            doc.get("data")
                .and_then(|data| data.get("name"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect();
    collection_names.sort();
    collection_names.dedup();

    for collection_name in &collection_names {
        remove_collection_storages(
            storage,
            &internal_store,
            &database_instance_token,
            database_name,
            collection_name,
            multi_instance,
            password.as_deref(),
            None,
        )
        .await?;
    }
    internal_store.remove().await?;
    Ok(collection_names)
}

async fn write_collection_meta(
    database: &RxDatabase,
    internal_store: &Arc<dyn RxStorageInstance>,
    collection_name: &str,
    schema: &RxSchema,
) -> RxResult<()> {
    let collection_name_with_version =
        collection_name_primary(collection_name, &schema.json_schema);
    let schema_hash = schema.hash().await;
    let meta_doc = json!({
        "id": get_primary_key_of_internal_document(
            &collection_name_with_version,
            INTERNAL_CONTEXT_COLLECTION
        ),
        "key": collection_name_with_version,
        "context": INTERNAL_CONTEXT_COLLECTION,
        "data": {
            "name": collection_name,
            "schemaHash": schema_hash,
            "schema": schema.json_schema,
            "version": schema.version(),
            "connectedStorages": [],
        },
        "_deleted": false,
        "_meta": get_default_rx_document_meta(),
        "_rev": get_default_revision(),
        "_attachments": {},
    });
    let write_rows = vec![BulkWriteRow {
        previous: None,
        document: meta_doc,
    }];
    let write_result = internal_store
        .bulk_write(write_rows, "rx-database-add-collection")
        .await?;
    for error in write_result.error {
        if error.status != 409 {
            return Err(new_rx_error(
                "DB12",
                Some(json!({
                    "database": database.name,
                    "writeError": error,
                })),
            ));
        }
        let doc_in_db = error.document_in_db.unwrap_or(Value::Null);
        let previous_hash = doc_in_db
            .get("data")
            .and_then(|data| data.get("schemaHash"))
            .and_then(Value::as_str);
        if previous_hash != Some(schema_hash.as_str()) {
            let previous_schema = doc_in_db.get("data").and_then(|data| data.get("schema"));
            let current_schema = serde_json::to_value(&schema.json_schema).unwrap_or(Value::Null);
            if previous_schema
                .map(|stored| schemas_compatible_for_meta_repair(stored, &current_schema))
                .unwrap_or(false)
            {
                repair_collection_meta_schema_hash(
                    database,
                    internal_store,
                    doc_in_db,
                    &schema_hash,
                    &current_schema,
                )
                .await?;
                continue;
            }
            return Err(new_rx_error(
                "DB6",
                Some(json!({
                    "database": database.name,
                    "collection": collection_name,
                    "previousSchemaHash": previous_hash,
                    "schemaHash": schema_hash,
                })),
            ));
        }
    }
    Ok(())
}

fn schemas_compatible_for_meta_repair(stored: &Value, current: &Value) -> bool {
    let stored = normalize_schema_for_meta_repair(stored);
    let current = normalize_schema_for_meta_repair(current);
    if stored == current {
        return true;
    }
    schemas_allow_additive_optional_properties(&stored, &current)
}

fn normalize_schema_for_meta_repair(schema: &Value) -> Value {
    let mut normalized = schema.clone();
    strip_legacy_rxdb_reserved_schema_fields(&mut normalized);
    sort_object(&normalized, true)
}

fn strip_legacy_rxdb_reserved_schema_fields(schema: &mut Value) {
    const RESERVED_FIELDS: &[&str] = &["_attachments", "_deleted", "_meta", "_rev"];
    let Some(obj) = schema.as_object_mut() else {
        return;
    };
    if let Some(properties) = obj.get_mut("properties").and_then(Value::as_object_mut) {
        for field in RESERVED_FIELDS {
            properties.remove(*field);
        }
    }
    if let Some(required) = obj.get_mut("required").and_then(Value::as_array_mut) {
        required.retain(|field| {
            field
                .as_str()
                .map(|name| !RESERVED_FIELDS.contains(&name))
                .unwrap_or(true)
        });
    }
}

fn schemas_allow_additive_optional_properties(stored: &Value, current: &Value) -> bool {
    let (Some(stored_obj), Some(current_obj)) = (stored.as_object(), current.as_object()) else {
        return false;
    };
    for (key, stored_value) in stored_obj {
        if key == "properties" || key == "required" {
            continue;
        }
        if current_obj.get(key) != Some(stored_value) {
            return false;
        }
    }
    for (key, current_value) in current_obj {
        if key == "properties" || key == "required" {
            continue;
        }
        if stored_obj.get(key) != Some(current_value) {
            return false;
        }
    }

    let stored_required = schema_required_fields(stored);
    let current_required = schema_required_fields(current);
    if stored_required != current_required {
        return false;
    }

    let Some(stored_properties) = stored_obj.get("properties").and_then(Value::as_object) else {
        return current_obj
            .get("properties")
            .and_then(Value::as_object)
            .map(|properties| {
                properties
                    .keys()
                    .all(|name| !current_required.contains(name))
            })
            .unwrap_or(true);
    };
    let Some(current_properties) = current_obj.get("properties").and_then(Value::as_object) else {
        return stored_properties.is_empty();
    };

    for (name, stored_property) in stored_properties {
        if current_properties.get(name) != Some(stored_property) {
            return false;
        }
    }
    current_properties
        .keys()
        .filter(|name| !stored_properties.contains_key(*name))
        .all(|name| !current_required.contains(name))
}

fn schema_required_fields(schema: &Value) -> std::collections::BTreeSet<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|required| {
            required
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

async fn repair_collection_meta_schema_hash(
    database: &RxDatabase,
    internal_store: &Arc<dyn RxStorageInstance>,
    previous: Value,
    schema_hash: &str,
    schema_json: &Value,
) -> RxResult<()> {
    let mut document = flat_clone_doc_with_meta(&previous);
    if let Some(obj) = document.as_object_mut() {
        if let Some(data) = obj.get_mut("data").and_then(Value::as_object_mut) {
            data.insert(
                "schemaHash".to_string(),
                Value::String(schema_hash.to_string()),
            );
            data.insert("schema".to_string(), schema_json.clone());
        }
        if let Some(meta) = obj.get_mut("_meta").and_then(Value::as_object_mut) {
            meta.insert("lwt".to_string(), json!(now()));
        }
        let previous_rev = previous.get("_rev").and_then(Value::as_str);
        obj.insert(
            "_rev".to_string(),
            Value::String(create_revision(&database.token, previous_rev).unwrap_or_default()),
        );
    }
    let result = internal_store
        .bulk_write(
            vec![BulkWriteRow {
                previous: Some(previous),
                document,
            }],
            "rx-database-repair-collection-schema-hash",
        )
        .await?;
    if let Some(error) = result.error.first() {
        if error.status != 409 {
            return Err(new_rx_error(
                "DB12",
                Some(json!({
                    "database": database.name,
                    "writeError": error,
                })),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::storage_memory::get_rx_storage_memory;
    use crate::types::{HashFunction, HashOutput, JsonSchema, PrimaryKey};
    use futures::StreamExt;

    struct TestHashFunction;

    impl HashFunction for TestHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("hash:{input}") })
        }
    }

    fn test_schema() -> RxJsonSchema {
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
            "age".to_string(),
            JsonSchema {
                schema_type: Some("number".to_string()),
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
            additional_properties: false,
            extra: HashMap::new(),
        }
    }

    fn pet_schema_with_owner_ref() -> RxJsonSchema {
        let mut schema = test_schema();
        schema.properties.insert(
            "owner".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
                extra: HashMap::from([("ref".to_string(), json!("humans"))]),
                ..Default::default()
            },
        );
        schema
    }

    fn group_schema_with_member_refs() -> RxJsonSchema {
        let mut schema = test_schema();
        schema.properties.insert(
            "members".to_string(),
            JsonSchema {
                schema_type: Some("array".to_string()),
                items: Some(Box::new(JsonSchema {
                    schema_type: Some("string".to_string()),
                    ..Default::default()
                })),
                extra: HashMap::from([("ref".to_string(), json!("humans"))]),
                ..Default::default()
            },
        );
        schema
    }

    #[test]
    fn schema_meta_repair_ignores_legacy_rxdb_reserved_fields() {
        let stored = json!({
            "version": 0,
            "primaryKey": "id",
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "status": { "type": "string" },
                "_deleted": { "type": "boolean" },
                "_rev": { "type": "string" },
                "_meta": { "type": "object" },
                "_attachments": { "type": "object" }
            },
            "required": ["id", "status", "_deleted", "_rev", "_meta", "_attachments"],
            "additionalProperties": false
        });
        let current = json!({
            "version": 0,
            "primaryKey": "id",
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "status": { "type": "string" }
            },
            "required": ["id", "status"],
            "additionalProperties": false
        });

        assert!(schemas_compatible_for_meta_repair(&stored, &current));
    }

    #[test]
    fn schema_meta_repair_allows_additive_optional_properties() {
        let stored = json!({
            "version": 0,
            "primaryKey": "id",
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "title": { "type": "string" },
                "status": { "type": "string" }
            },
            "required": ["id", "title", "status"],
            "additionalProperties": false
        });
        let current = json!({
            "version": 0,
            "primaryKey": "id",
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "title": { "type": "string" },
                "status": { "type": "string" },
                "command_id": { "type": "string" },
                "command_type": { "type": "string" }
            },
            "required": ["id", "title", "status"],
            "additionalProperties": false
        });

        assert!(schemas_compatible_for_meta_repair(&stored, &current));
    }

    #[test]
    fn schema_meta_repair_rejects_required_property_additions() {
        let stored = json!({
            "version": 0,
            "primaryKey": "id",
            "type": "object",
            "properties": {
                "id": { "type": "string" }
            },
            "required": ["id"],
            "additionalProperties": false
        });
        let current = json!({
            "version": 0,
            "primaryKey": "id",
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "status": { "type": "string" }
            },
            "required": ["id", "status"],
            "additionalProperties": false
        });

        assert!(!schemas_compatible_for_meta_repair(&stored, &current));
    }

    #[tokio::test]
    async fn create_database_and_add_collection() {
        let storage = get_rx_storage_memory(());
        let before_count = db_count();
        let database = create_rx_database(RxDatabaseCreator {
            name: "db".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();

        assert!(database.internal_store.is_some());
        assert!(!database.token.is_empty());
        assert!(!database.storage_token.is_empty());
        assert!(database.is_first_time_instantiated().await.unwrap());
        assert!(db_count() >= before_count + 1);

        let collections = database
            .add_collections(HashMap::from([(
                "humans".to_string(),
                RxCollectionCreator {
                    schema: test_schema(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .unwrap();
        let humans = collections.get("humans").unwrap();
        humans
            .insert(json!({ "id": "alice", "age": 42 }))
            .await
            .unwrap();

        assert!(database.collection("humans").is_some());
        assert_eq!(database.collection_names(), vec!["humans".to_string()]);
        assert_eq!(
            humans
                .find_one(None)
                .unwrap()
                .exec(true)
                .await
                .unwrap()
                .get("id")
                .cloned(),
            Some(json!("alice"))
        );
        database.close().await.unwrap();
    }

    #[tokio::test]
    async fn add_collection_rejects_same_name_twice() {
        let storage = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "db2".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();

        let creator = RxCollectionCreator {
            schema: test_schema(),
            conflict_handler: None,
            options: HashMap::new(),
        };
        database
            .add_collections(HashMap::from([("humans".to_string(), creator.clone())]))
            .await
            .unwrap();
        let result = database
            .add_collections(HashMap::from([("humans".to_string(), creator)]))
            .await;
        let Err(err) = result else {
            panic!("expected duplicate collection error");
        };
        assert_eq!(err.code(), "DB3");
    }

    #[tokio::test]
    async fn remove_collection_doc_soft_deletes_internal_metadata() {
        let storage = get_rx_storage_memory(());
        let schema = test_schema();
        let database = create_rx_database(RxDatabaseCreator {
            name: "db-remove-collection-doc".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        database
            .add_collections(HashMap::from([(
                "humans".to_string(),
                RxCollectionCreator {
                    schema: schema.clone(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .unwrap();
        let internal_store = database.internal_store.as_ref().unwrap();
        assert_eq!(
            get_all_collection_documents(internal_store)
                .await
                .unwrap()
                .len(),
            1
        );

        database
            .remove_collection_doc("humans", &schema)
            .await
            .unwrap();

        assert!(get_all_collection_documents(internal_store)
            .await
            .unwrap()
            .is_empty());
        let Err(err) = database.remove_collection_doc("humans", &schema).await else {
            panic!("expected missing collection metadata error");
        };
        assert_eq!(err.code(), "SNH");
        database.close().await.unwrap();
    }

    #[tokio::test]
    async fn create_database_rejects_duplicate_open_name_and_reopens_after_close() {
        let storage = get_rx_storage_memory(());
        let first = create_rx_database(RxDatabaseCreator {
            name: "db-duplicate".to_string(),
            storage: storage.clone(),
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();

        let duplicate = create_rx_database(RxDatabaseCreator {
            name: "db-duplicate".to_string(),
            storage: storage.clone(),
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await;
        let Err(err) = duplicate else {
            panic!("expected duplicate database error");
        };
        assert_eq!(err.code(), "DB8");

        let ignored = create_rx_database(RxDatabaseCreator {
            name: "db-duplicate".to_string(),
            storage: storage.clone(),
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: true,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        assert!(!first.closed());
        assert!(!ignored.closed());
        ignored.close().await.unwrap();

        let replacement = create_rx_database(RxDatabaseCreator {
            name: "db-duplicate".to_string(),
            storage: storage.clone(),
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: true,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        assert!(first.closed());
        replacement.close().await.unwrap();

        let reopened = create_rx_database(RxDatabaseCreator {
            name: "db-duplicate".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        reopened.request_idle_promise().await;
        reopened.request_idle_promise_no_queue().await;
    }

    #[tokio::test]
    async fn plugin_backed_database_methods_return_plugin_missing() {
        let storage = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "db-plugin-missing".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();

        assert_eq!(
            database.export_json(None).await.unwrap_err().code(),
            "PLUGIN_MISSING"
        );
        assert_eq!(
            database.import_json(Value::Null).await.unwrap_err().code(),
            "PLUGIN_MISSING"
        );
        assert_eq!(
            database.add_state(None).await.unwrap_err().code(),
            "PLUGIN_MISSING"
        );
        assert_eq!(
            database.backup(Value::Null).unwrap_err().code(),
            "PLUGIN_MISSING"
        );
        assert_eq!(
            database.leader_elector().unwrap_err().code(),
            "PLUGIN_MISSING"
        );
        assert_eq!(
            database.migration_states().unwrap_err().code(),
            "PLUGIN_MISSING"
        );

        database.close().await.unwrap();
    }

    #[tokio::test]
    async fn ensure_no_startup_errors_returns_first_queued_error() {
        let storage = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "db-startup-errors".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();

        database.ensure_no_startup_errors().await.unwrap();
        database.add_startup_error(new_rx_error("DB_STARTUP_TEST", None));

        assert_eq!(
            database
                .ensure_no_startup_errors()
                .await
                .unwrap_err()
                .code(),
            "DB_STARTUP_TEST"
        );

        database.close().await.unwrap();
    }

    #[tokio::test]
    async fn remove_database_removes_known_collections_and_internal_store() {
        let storage: Arc<dyn RxStorage> = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "db3".to_string(),
            storage: storage.clone(),
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        database
            .add_collections(HashMap::from([(
                "humans".to_string(),
                RxCollectionCreator {
                    schema: test_schema(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .unwrap();
        database.close().await.unwrap();

        let removed = remove_rx_database("db3", &storage, false, None)
            .await
            .unwrap();
        assert_eq!(removed, vec!["humans".to_string()]);

        let recreated = create_rx_database(RxDatabaseCreator {
            name: "db3".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        assert!(
            get_all_collection_documents(recreated.internal_store.as_ref().unwrap())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn collection_remove_cleans_registry_and_metadata() {
        let storage = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "db4".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        let collections = database
            .add_collections(HashMap::from([(
                "humans".to_string(),
                RxCollectionCreator {
                    schema: test_schema(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .unwrap();

        let remove_calls = Arc::new(AtomicUsize::new(0));
        collections.get("humans").unwrap().on_remove_push({
            let remove_calls = Arc::clone(&remove_calls);
            Box::new(move || {
                remove_calls.fetch_add(1, Ordering::SeqCst);
            })
        });

        collections.get("humans").unwrap().remove().await.unwrap();
        assert!(database.collection("humans").is_none());
        assert_eq!(remove_calls.load(Ordering::SeqCst), 1);
        assert!(
            get_all_collection_documents(database.internal_store.as_ref().unwrap())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn document_populate_resolves_referenced_collection() {
        let storage = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "db5".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        let collections = database
            .add_collections(HashMap::from([
                (
                    "humans".to_string(),
                    RxCollectionCreator {
                        schema: test_schema(),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "pets".to_string(),
                    RxCollectionCreator {
                        schema: pet_schema_with_owner_ref(),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
            ]))
            .await
            .unwrap();

        collections
            .get("humans")
            .unwrap()
            .insert(json!({ "id": "alice", "age": 42 }))
            .await
            .unwrap();
        let pet = collections
            .get("pets")
            .unwrap()
            .insert(json!({ "id": "fido", "owner": "alice" }))
            .await
            .unwrap();
        let owner = pet.populate("owner").await.unwrap().unwrap();

        assert_eq!(owner.get("id").and_then(Value::as_str), Some("alice"));
        database.close().await.unwrap();
    }

    #[tokio::test]
    async fn document_populate_resolves_array_references_in_order() {
        let storage = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "db6".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        let collections = database
            .add_collections(HashMap::from([
                (
                    "humans".to_string(),
                    RxCollectionCreator {
                        schema: test_schema(),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "groups".to_string(),
                    RxCollectionCreator {
                        schema: group_schema_with_member_refs(),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
            ]))
            .await
            .unwrap();

        let humans = collections.get("humans").unwrap();
        humans
            .bulk_insert(vec![
                json!({ "id": "alice", "age": 42 }),
                json!({ "id": "bob", "age": 33 }),
            ])
            .await
            .unwrap();
        let group = collections
            .get("groups")
            .unwrap()
            .insert(json!({ "id": "team", "members": ["bob", "alice"] }))
            .await
            .unwrap();

        let members = group.populate("members").await.unwrap().unwrap();
        let member_ids: Vec<&str> = members
            .as_array()
            .unwrap()
            .iter()
            .map(|member| member.get("id").and_then(Value::as_str).unwrap())
            .collect();
        assert_eq!(member_ids, vec!["bob", "alice"]);

        database.close().await.unwrap();
    }

    #[tokio::test]
    async fn database_event_bulks_merges_collection_streams() {
        let storage = get_rx_storage_memory(());
        let database = create_rx_database(RxDatabaseCreator {
            name: "db7".to_string(),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(TestHashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: false,
            event_reduce: true,
            allow_slow_count: false,
        })
        .await
        .unwrap();
        let collections = database
            .add_collections(HashMap::from([(
                "humans".to_string(),
                RxCollectionCreator {
                    schema: test_schema(),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .unwrap();
        let mut event_bulks = database.event_bulks();

        collections
            .get("humans")
            .unwrap()
            .insert(json!({ "id": "alice", "age": 42 }))
            .await
            .unwrap();

        let event_bulk =
            tokio::time::timeout(std::time::Duration::from_secs(1), event_bulks.next())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(event_bulk.collection_name, "humans");
        assert!(event_bulk
            .events
            .iter()
            .any(|event| event.document_id == "alice"));

        database.close().await.unwrap();
    }
}
