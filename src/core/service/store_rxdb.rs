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
    sync::{Arc, Mutex},
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

pub(crate) struct RxdbJobs {
    database: Database,
    runtime: Handle,
    bindings: BindingResolver,
    commits: watch::Receiver<u64>,
    _forwarder: Forwarder,
    admission: Arc<Semaphore>,
}
impl RxdbJobs {
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
