//! RxDB's schema seam to the one Sync-Store owner (P3-09–11).
//! No file opening, private sequence or cross-database transaction lives here.

use crate::{
    read_pool::ReadPool,
    writer::{CommitReceiver, WriterHandle},
    JobHandle, JobOptions, ReadJob, StoreError, WriteJob,
};
use rusqlite::{Connection, Transaction};

/// The physical V3 table can be bound without copying it to a new owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RxdbTable {
    pub collection: String,
    pub table: String,
    pub generation: u64,
    pub schema_version: u32,
    pub app_version: String,
}

/// Implemented by P3-05 using TableBinding/install_triggers/backfill_page.
/// Installing a binding must be idempotent, atomic with DDL, and must not
/// allocate an independent counter. Existing rows require bounded backfill
/// before the caller advertises the binding as ready.
pub trait ChangeLogSink {
    fn install_rxdb(&self, tx: &Transaction<'_>, binding: &RxdbTable) -> Result<(), StoreError>;
}

/// The actual P3-05 sink, retaining the registered application's version and
/// generation. Neither a per-table counter nor a fabricated schema version is
/// introduced when a legacy physical table joins the global change log.
pub struct CanonicalChangeLog;
impl ChangeLogSink for CanonicalChangeLog {
    fn install_rxdb(&self, tx: &Transaction<'_>, binding: &RxdbTable) -> Result<(), StoreError> {
        crate::change_log::install_triggers(
            tx,
            &crate::change_log::TableBinding {
                collection: binding.collection.clone(),
                gen: binding.generation,
                table: binding.table.clone(),
                id_column: "id".into(),
                rev_column: "revision".into(),
                data_column: "data".into(),
                deleted_column: "deleted".into(),
                schema_column: None,
                schema_ver: binding.schema_version,
                app_ver: binding.app_version.clone(),
            },
        )
        .map_err(Into::into)
    }
}

pub fn backfill(
    tx: &Transaction<'_>,
    table: &str,
    after: Option<&str>,
) -> Result<Option<String>, StoreError> {
    let (collection, generation): (String, u64) = tx.query_row(
        "SELECT collection,gen FROM _change_sources WHERE table_name=?1",
        [table],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let page = crate::change_log::backfill_page(tx, &collection, generation, after, 500)?;
    Ok(if page.complete { None } else { page.cursor })
}

/// Borrow the already registered owner and its reader pool; never create a
/// second owner just because a new collection is registered.
#[derive(Clone)]
pub struct RxdbOwner {
    writer: WriterHandle,
    readers: ReadPool,
}
impl RxdbOwner {
    /// Construct once per registered DbId and share clones across collections.
    /// Derive the reader path/generation from the writer, never from a caller's
    /// independently supplied connection or path.
    pub async fn attach(writer: WriterHandle, readers: ReadPool) -> Result<Self, StoreError> {
        writer.ready().await?;
        if readers.generation() != writer.generation().0 {
            return Err(StoreError::Invalid(
                "RxDB reader generation mismatch".into(),
            ));
        }
        let path: String = readers
            .read_job(ReadJob::new(JobOptions::default(), |conn| {
                Ok(conn.query_row(
                    "SELECT file FROM pragma_database_list WHERE name='main'",
                    [],
                    |row| row.get(0),
                )?)
            }))
            .await?;
        if crate::DbId::new(path)? != *writer.db_id() {
            return Err(StoreError::Invalid("RxDB reader file mismatch".into()));
        }
        Ok(Self { writer, readers })
    }

    pub fn subscribe_commits(&self) -> CommitReceiver {
        self.writer.subscribe_commits()
    }

    /// Retire admission and drain snapshots before closing the registered
    /// writer. The daemon calls this once, after retiring its collection users.
    pub async fn shutdown(&self) -> Result<(), StoreError> {
        self.readers.shutdown().await.map_err(StoreError::from)?;
        self.writer.shutdown().await
    }

    pub fn write<T: Send + 'static>(
        &self,
        options: JobOptions,
        write: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<JobHandle<T>, StoreError> {
        self.writer.submit(WriteJob::new(options, write))
    }

    pub async fn read<T: Send + 'static>(
        &self,
        options: JobOptions,
        read: impl FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        self.readers.read_job(ReadJob::new(options, read)).await
    }
}

/// Run in the schema owner's transaction after creating the physical table.
/// The sink is installed first: failure leaves the old notification triggers
/// intact when the surrounding owner transaction rolls back.
pub fn install<S: ChangeLogSink>(
    tx: &Transaction<'_>,
    binding: &RxdbTable,
    sink: &S,
) -> Result<(), StoreError> {
    tx.execute_batch("SAVEPOINT rxdb_schema_adapter")?;
    let result = install_inner(tx, binding, sink);
    match result {
        Ok(()) => {
            tx.execute_batch("RELEASE rxdb_schema_adapter")?;
            Ok(())
        }
        Err(error) => {
            tx.execute_batch("ROLLBACK TO rxdb_schema_adapter; RELEASE rxdb_schema_adapter")?;
            Err(error)
        }
    }
}

fn install_inner<S: ChangeLogSink>(
    tx: &Transaction<'_>,
    binding: &RxdbTable,
    sink: &S,
) -> Result<(), StoreError> {
    sink.install_rxdb(tx, binding)?;
    let table = binding.table.replace('"', "\"\"");
    for suffix in ["insert", "update", "delete"] {
        tx.execute_batch(&format!(
            "DROP TRIGGER IF EXISTS \"{table}__rxdb_changed_{suffix}\";"
        ))?;
    }
    // Do not drop __rxdb_changed_tables globally: unported V3 collections may
    // still own rows there. Owned collections use only _seq/_changes.
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='__rxdb_changed_tables')",
        [], |row| row.get(0))?;
    if exists {
        tx.execute(
            "DELETE FROM __rxdb_changed_tables WHERE table_name=?1",
            [&binding.table],
        )?;
    }
    Ok(())
}

/// Compatibility readers see the authoritative log position, never a local
/// table-write count. Read inside the same WAL snapshot as the documents.
pub fn table_change_position(conn: &Connection, table: &str) -> Result<u64, StoreError> {
    let bound: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM _change_sources WHERE table_name=?1)",
        [table],
        |row| row.get(0),
    )?;
    if !bound {
        return Err(StoreError::Invalid(
            "unknown RxDB change-log binding".into(),
        ));
    }
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(c.seq),0) FROM _changes c
        JOIN _change_sources s ON s.collection=c.collection AND s.gen=c.gen
        WHERE s.table_name=?1",
        [table],
        |row| row.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::WriterOptions;
    use crate::{
        config::{AdmissionBudget, ConnectionConfig},
        registry::Registry,
        DbId,
    };

    #[tokio::test]
    async fn owner_jobs_rollback_and_keep_one_reader_snapshot_while_writer_commits() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbId::new(dir.path().join("rxdb.sqlite3")).unwrap();
        let writer = Registry.open(db.clone(), WriterOptions::default()).unwrap();
        writer.ready().await.unwrap();
        writer
            .submit(WriteJob::new(JobOptions::default(), |tx| {
                tx.execute_batch("CREATE TABLE docs(id INTEGER); INSERT INTO docs VALUES(1)")?;
                Ok(())
            }))
            .unwrap()
            .await
            .unwrap();
        let mut budget = AdmissionBudget::default();
        budget.execution = std::time::Duration::from_secs(3);
        let readers = ReadPool::for_database(
            writer.db_id(),
            writer.generation(),
            ConnectionConfig::default(),
            budget,
        )
        .await
        .unwrap();
        let owner = RxdbOwner::attach(writer.clone(), readers).await.unwrap();

        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let read_owner = owner.clone();
        let read = tokio::spawn(async move {
            read_owner
                .read(JobOptions::default(), move |conn| {
                    let before: i64 =
                        conn.query_row("SELECT COUNT(*) FROM docs", [], |r| r.get(0))?;
                    let _ = started.send(());
                    wait.recv_timeout(std::time::Duration::from_secs(2))
                        .map_err(|_| StoreError::DeadlineExceeded)?;
                    let after: i64 =
                        conn.query_row("SELECT COUNT(*) FROM docs", [], |r| r.get(0))?;
                    Ok((before, after))
                })
                .await
        });
        ready.await.unwrap();
        owner
            .write(JobOptions::default(), |tx| {
                tx.execute("INSERT INTO docs VALUES(2)", [])?;
                Ok(())
            })
            .unwrap()
            .await
            .unwrap();
        release.send(()).unwrap();
        assert_eq!(read.await.unwrap().unwrap(), (1, 1));
        let error = owner
            .write(JobOptions::default(), |tx| -> Result<(), StoreError> {
                tx.execute("INSERT INTO docs VALUES(3)", [])?;
                Err(StoreError::Invalid("abort".into()))
            })
            .unwrap()
            .await;
        assert!(error.is_err());
        assert_eq!(
            owner
                .read(JobOptions::default(), |conn| {
                    Ok(conn.query_row("SELECT COUNT(*) FROM docs", [], |r| r.get::<_, i64>(0))?)
                })
                .await
                .unwrap(),
            2
        );
        owner.shutdown().await.unwrap();
    }

    struct Sink(bool);
    impl ChangeLogSink for Sink {
        fn install_rxdb(
            &self,
            tx: &Transaction<'_>,
            binding: &RxdbTable,
        ) -> Result<(), StoreError> {
            tx.execute_batch("CREATE TABLE sink_work(id INTEGER)")?;
            if !self.0 {
                return Err(StoreError::Invalid("sink failed".into()));
            }
            tx.execute_batch(
                "CREATE TABLE _changes(collection TEXT,gen INTEGER,seq INTEGER);
                CREATE TABLE _change_sources(collection TEXT,gen INTEGER,table_name TEXT);
                INSERT INTO _changes VALUES('docs',1,73);",
            )?;
            tx.execute(
                "INSERT INTO _change_sources VALUES('docs',1,?1)",
                [&binding.table],
            )?;
            Ok(())
        }
    }
    #[test]
    fn log_install_rollback_preserves_old_trigger_and_counter_maps_to_global_seq() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE docs(id INTEGER); CREATE TABLE __rxdb_changed_tables(table_name TEXT,changed_at INTEGER);
            INSERT INTO __rxdb_changed_tables VALUES('docs',5);
            CREATE TRIGGER docs__rxdb_changed_insert AFTER INSERT ON docs BEGIN
              UPDATE __rxdb_changed_tables SET changed_at=changed_at+1; END;").unwrap();
        let binding = RxdbTable {
            collection: "docs".into(),
            table: "docs".into(),
            generation: 1,
            schema_version: 1,
            app_version: "1.0".into(),
        };
        {
            let tx = conn.transaction().unwrap();
            assert!(install(&tx, &binding, &Sink(false)).is_err());
            // Catching the error cannot accidentally commit partial sink work.
            tx.commit().unwrap();
        }
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name='docs__rxdb_changed_insert'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name='sink_work'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        let tx = conn.transaction().unwrap();
        install(&tx, &binding, &Sink(true)).unwrap();
        assert_eq!(table_change_position(&tx, "docs").unwrap(), 73);
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM __rxdb_changed_tables", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name='docs__rxdb_changed_insert'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        tx.commit().unwrap();
    }
}
