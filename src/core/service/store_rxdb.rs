//! Native RxDB job boundary: the existing registered writer and reader pool,
//! with original RxErrors preserved while failed mutations roll back.
use super::store_runtime::Database;
use ctox_store::{
    rxdb_schema_adapter::{self, CanonicalChangeLog, RxdbTable},
    Cancellation, JobOptions, ReadJob, StoreError, WriteJob,
};
use rxdb::{
    rx_error::{new_rx_error, RxError, RxResult},
    storage::sqlite::types::{
        RxStorageSqlite, RxStorageSqliteSettings, SqliteJobTicket, SqliteJobValue, SqliteOwnerJob,
        SqliteOwnerJobs, SqliteOwnerWriteJob,
    },
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{Duration, Instant},
};
use tokio::{
    runtime::Handle,
    sync::{watch, Semaphore},
};

/// Resolve metadata in the current schema transaction, not from an old policy
/// or app-version snapshot. Unknown/incompatible physical bindings fail closed.
pub(crate) type BindingResolver = Arc<
    dyn Fn(&rusqlite::Transaction<'_>, &str, &str, i32) -> Result<RxdbTable, StoreError>
        + Send
        + Sync,
>;

type OriginalError = Arc<Mutex<Option<RxError>>>;
fn error(error: StoreError) -> RxError {
    let code = match error {
        StoreError::Busy => "SQLITE_OWNER_BUSY",
        StoreError::Cancelled => "SQLITE_OWNER_CANCELLED",
        StoreError::DeadlineExceeded => "SQLITE_OWNER_DEADLINE",
        StoreError::Closed => "SQLITE_OWNER_CLOSED",
        _ => "SQLITE_OWNER",
    };
    // SQL, documents and credentials must not leak through the error bridge.
    new_rx_error(code, None)
}
fn result(
    value: Result<SqliteJobValue, StoreError>,
    original: &OriginalError,
) -> RxResult<SqliteJobValue> {
    value.map_err(|failure| {
        original
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
            .unwrap_or_else(|| error(failure))
    })
}
fn operation_error(original: &OriginalError, failure: RxError) -> StoreError {
    *original.lock().unwrap_or_else(|p| p.into_inner()) = Some(failure);
    // Returning Err to the writer is essential. Ok(Err(RxError)) would commit.
    StoreError::Invalid("RxDB operation rejected".into())
}
fn options() -> JobOptions {
    JobOptions {
        deadline: Some(Instant::now() + Duration::from_secs(30)),
        ..Default::default()
    }
}
struct Forwarder(tokio::task::JoinHandle<()>);
impl Drop for Forwarder {
    fn drop(&mut self) {
        self.0.abort();
    }
}

// Lookup is injection only. A missing, stopped or retired daemon owner never
// falls back to opening the path supplied by a helper/CLI process.
static NATIVE_STORES: OnceLock<Mutex<HashMap<PathBuf, Weak<RxdbJobs>>>> = OnceLock::new();
pub(crate) struct NativeStoreRegistration {
    path: PathBuf,
    owner: Arc<RxdbJobs>,
}
impl Drop for NativeStoreRegistration {
    fn drop(&mut self) {
        let mut stores = NATIVE_STORES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if stores
            .get(&self.path)
            .is_some_and(|w| w.ptr_eq(&Arc::downgrade(&self.owner)))
        {
            stores.remove(&self.path);
        }
    }
}
pub(crate) async fn native_storage(path: PathBuf) -> Result<Arc<RxStorageSqlite>, StoreError> {
    let id = tokio::task::spawn_blocking(move || ctox_store::DbId::new(path))
        .await
        .map_err(|_| StoreError::Panicked)??;
    let jobs = NATIVE_STORES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(id.path())
        .and_then(Weak::upgrade)
        .ok_or(StoreError::Closed)?;
    jobs.database.writer.ready().await?;
    Ok(jobs.storage())
}

pub(crate) struct RxdbJobs {
    database: Database,
    runtime: Handle,
    bindings: BindingResolver,
    commits: watch::Receiver<u64>,
    _forwarder: Forwarder,
    admission: Arc<Semaphore>,
}
impl RxdbJobs {
    /// Explicit composition-root registration. The guard is retired before the
    /// owner drains; retaining it also retains the bridge's commit forwarder.
    pub async fn install_native(
        database: Database,
        runtime: Handle,
    ) -> Result<NativeStoreRegistration, StoreError> {
        let schema = rxdb::rx_database_internal_store::build_internal_store_schema();
        let definition = ctox_store::schema::CollectionDefinition {
            name: rxdb::rx_storage_helper::INTERNAL_STORAGE_NAME.into(),
            app_id: "ctox-rxdb-internal".into(),
            schema_ver: u32::try_from(schema.version)
                .map_err(|_| StoreError::Invalid("invalid internal schema version".into()))?,
            schema_json: serde_json::to_value(schema)
                .map_err(|_| StoreError::Invalid("internal schema encoding".into()))?,
            class: "control".into(),
            client_limit: 0,
            byte_budget: 0,
            rights_ver: 0,
            app_ver: "rxdb-internal-schema-1".into(),
        };
        database
            .writer
            .submit(WriteJob::new(options(), move |tx| {
                match ctox_store::schema::active_collection(tx, &definition.name)? {
                    Some(existing) if existing.definition == definition => {}
                    Some(_) => {
                        return Err(StoreError::Invalid(
                            "internal schema migration required".into(),
                        ))
                    }
                    None => {
                        ctox_store::schema::create_collection(tx, &definition)?;
                    }
                }
                Ok(())
            }))?
            .await?;
        let owner = Self::attach_canonical(database, runtime).await?;
        let path = owner.database.writer.db_id().path().to_path_buf();
        let mut stores = NATIVE_STORES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if stores.get(&path).and_then(Weak::upgrade).is_some() {
            return Err(StoreError::AlreadyOwned);
        }
        stores.insert(path.clone(), Arc::downgrade(&owner));
        Ok(NativeStoreRegistration { path, owner })
    }

    pub async fn attach_canonical(
        database: Database,
        runtime: Handle,
    ) -> Result<Arc<Self>, StoreError> {
        Self::attach(
            database,
            runtime,
            Arc::new(|tx, table, name, version| {
                let active = ctox_store::schema::active_collection(tx, name)?
                    .ok_or_else(|| StoreError::Invalid("collection not registered".into()))?;
                if active.table != table
                    || i32::try_from(active.definition.schema_ver).ok() != Some(version)
                {
                    return Err(StoreError::Invalid(
                        "active collection binding changed".into(),
                    ));
                }
                Ok(RxdbTable {
                    collection: name.into(),
                    table: active.table,
                    generation: active.gen,
                    schema_version: active.definition.schema_ver,
                    app_version: active.definition.app_ver,
                })
            }),
        )
        .await
    }

    pub async fn attach(
        database: Database,
        runtime: Handle,
        bindings: BindingResolver,
    ) -> Result<Arc<Self>, StoreError> {
        ctox_store::rxdb_schema_adapter::RxdbOwner::attach(
            database.writer.clone(),
            database.readers.clone(),
        )
        .await?;
        let mut notifications = database.writer.subscribe_commits();
        let head = database
            .readers
            .read_job(ReadJob::new(options(), |conn| {
                Ok(
                    conn.query_row("SELECT v FROM _seq WHERE singleton=1", [], |row| {
                        row.get::<_, u64>(0)
                    })?,
                )
            }))
            .await?;
        let (send, commits) = watch::channel(head);
        let forwarder = runtime.spawn(async move {
            if let Some(latest) = notifications.latest() {
                let seen = *send.borrow();
                if latest.head_seq > seen {
                    send.send_replace(latest.head_seq);
                }
            }
            while let Ok(committed) = notifications.changed().await {
                let seen = *send.borrow();
                if committed.head_seq >= seen {
                    send.send_replace(committed.head_seq);
                }
            }
        });
        Ok(Arc::new(Self {
            database,
            runtime,
            bindings,
            commits,
            _forwarder: Forwarder(forwarder),
            admission: Arc::new(Semaphore::new(1024)),
        }))
    }
    pub fn storage(self: &Arc<Self>) -> Arc<RxStorageSqlite> {
        RxStorageSqlite::with_owner(
            RxStorageSqliteSettings {
                database_path: self.database.writer.db_id().path().to_path_buf(),
            },
            self.clone(),
        )
    }
}
impl SqliteOwnerJobs for RxdbJobs {
    fn collection_table(
        &self,
        tx: &rusqlite::Transaction<'_>,
        collection: &str,
        version: i32,
    ) -> RxResult<String> {
        let registered = ctox_store::schema::active_collection(tx, collection)
            .map_err(StoreError::from)
            .map_err(error)?
            .ok_or_else(|| error(StoreError::Invalid("collection not registered".into())))?;
        if i32::try_from(registered.definition.schema_ver).ok() != Some(version) {
            return Err(error(StoreError::Invalid(
                "collection schema version mismatch".into(),
            )));
        }
        Ok(registered.table)
    }

    fn read(&self, job: SqliteOwnerJob) -> RxResult<SqliteJobTicket> {
        // Bound bridge futures as well as SQLite jobs. There is no unbounded
        // Tokio task backlog ahead of the four-reader admission boundary.
        let permit = self
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| error(StoreError::Busy))?;
        let readers = self.database.readers.clone();
        let original = OriginalError::default();
        let captured = original.clone();
        let options = options();
        let cancellation = options.cancellation.clone();
        let (mut reply, ticket) = tokio::sync::oneshot::channel();
        self.runtime.spawn(async move {
            let _permit = permit;
            let read = readers.read_job(ReadJob::new(options, move |conn| {
                job(conn).map_err(|failure| operation_error(&captured, failure))
            }));
            tokio::select! {
                value = read => { let _ = reply.send(result(value, &original)); },
                _ = reply.closed() => cancellation.cancel(),
            }
        });
        Ok(ticket)
    }
    fn write(&self, job: SqliteOwnerWriteJob) -> RxResult<SqliteJobTicket> {
        let permit = self
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| error(StoreError::Busy))?;
        let original = OriginalError::default();
        let captured = original.clone();
        let options = options();
        let cancellation: Cancellation = options.cancellation.clone();
        let pending = self
            .database
            .writer
            .submit(WriteJob::new(options, move |tx| {
                job(tx).map_err(|failure| operation_error(&captured, failure))
            }))
            .map_err(error)?;
        let (mut reply, ticket) = tokio::sync::oneshot::channel();
        self.runtime.spawn(async move {
            let _permit = permit;
            tokio::select! {
                value = pending => { let _ = reply.send(result(value, &original)); },
                _ = reply.closed() => cancellation.cancel(),
            }
        });
        Ok(ticket)
    }
    fn install_change_log(
        &self,
        tx: &rusqlite::Transaction<'_>,
        table: &str,
        collection: &str,
        version: i32,
    ) -> RxResult<()> {
        let binding = (self.bindings)(tx, table, collection, version).map_err(error)?;
        if binding.table != table
            || binding.collection != collection
            || i32::try_from(binding.schema_version).ok() != Some(version)
        {
            return Err(error(StoreError::Invalid(
                "RxDB schema binding mismatch".into(),
            )));
        }
        rxdb_schema_adapter::install(tx, &binding, &CanonicalChangeLog).map_err(error)
    }
    fn backfill_change_log(
        &self,
        tx: &rusqlite::Transaction<'_>,
        table: &str,
        after: Option<&str>,
    ) -> RxResult<Option<String>> {
        rxdb_schema_adapter::backfill(tx, table, after).map_err(error)
    }
    fn subscribe_commits(&self) -> watch::Receiver<u64> {
        self.commits.clone()
    }
}

#[cfg(all(test, unix))]
#[path = "store_rxdb_tests.rs"]
mod tests;
