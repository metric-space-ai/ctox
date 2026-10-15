//! Injected file-owner seam for the legacy channel/LCM and runtime stores.
//!
//! No connection is opened here. The composition root supplies the existing
//! writer and its reader pool after schema-registry initialization (P3-06).
//! Legacy APIs returning Connections are a separate caller migration.

use ctox_store::{
    read_pool::ReadPool, writer::WriterHandle, DbId, JobOptions, ReadJob, StoreError, WriteJob,
};
use rusqlite::{Connection, Transaction};
use std::path::Path;

#[derive(Clone)]
pub(crate) struct OwnerAdapter {
    writer: WriterHandle,
    readers: ReadPool,
}

impl OwnerAdapter {
    /// Attach only to the requested canonical file and the same owner generation.
    /// The reader identity check runs inside its bounded snapshot job.
    pub(crate) async fn attach(
        path: &Path,
        writer: WriterHandle,
        readers: ReadPool,
    ) -> Result<Self, StoreError> {
        let expected = DbId::new(path)?;
        if writer.db_id() != &expected || readers.generation() != writer.generation().0 {
            return Err(StoreError::Invalid("store owner identity mismatch".into()));
        }
        writer.ready().await?;
        let reader_path: String = readers
            .read_job(ReadJob::new(JobOptions::default(), |conn| {
                Ok(conn.query_row(
                    "SELECT file FROM pragma_database_list WHERE name = 'main'",
                    [],
                    |row| row.get(0),
                )?)
            }))
            .await?;
        if DbId::new(reader_path)? != expected {
            return Err(StoreError::Invalid("store reader identity mismatch".into()));
        }
        Ok(Self { writer, readers })
    }

    fn check_generation(&self) -> Result<(), StoreError> {
        if self.readers.generation() != self.writer.generation().0 {
            return Err(StoreError::Invalid(
                "store adapter generation expired".into(),
            ));
        }
        Ok(())
    }

    /// One existing writer transaction; never open, nest or escape a connection.
    pub(crate) async fn write<T: Send + 'static>(
        &self,
        options: JobOptions,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        self.check_generation()?;
        self.writer.submit(WriteJob::new(options, operation))?.await
    }

    /// One snapshot per job, with query_only enforced by the supplied pool.
    pub(crate) async fn read<T: Send + 'static>(
        &self,
        options: JobOptions,
        operation: impl FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        self.check_generation()?;
        self.readers
            .read_job(ReadJob::new(options, operation))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_store::{
        config::{AdmissionBudget, ConnectionConfig},
        registry::Registry,
        writer::WriterOptions,
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn owner_adapter_keeps_writer_connection_and_reader_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.sqlite3");
        let writer = Registry.open(DbId::new(&path).unwrap(), WriterOptions {
            configure: Some(Arc::new(|conn| {
                ConnectionConfig::default().apply_writer(conn)?;
                conn.execute_batch("CREATE TEMP TABLE owner_connection_sentinel (value INTEGER); INSERT INTO owner_connection_sentinel VALUES (73); CREATE TABLE durable_value (value INTEGER);")?;
                Ok(())
            })),
            ..WriterOptions::default()
        }).unwrap();
        writer.ready().await.unwrap();
        let readers = ReadPool::for_database(
            writer.db_id(),
            writer.generation(),
            ConnectionConfig::default(),
            AdmissionBudget::default(),
        )
        .await
        .unwrap();
        let adapter = OwnerAdapter::attach(&path, writer.clone(), readers.clone())
            .await
            .unwrap();
        // A second Connection cannot see this writer's TEMP schema.
        adapter
            .write(JobOptions::default(), |tx| {
                let identity: i64 =
                    tx.query_row("SELECT value FROM owner_connection_sentinel", [], |r| {
                        r.get(0)
                    })?;
                assert_eq!(identity, 73);
                tx.execute("INSERT INTO durable_value VALUES (?1)", [identity])?;
                Ok(())
            })
            .await
            .unwrap();
        let value: i64 = adapter
            .read(JobOptions::default(), |conn| {
                Ok(conn.query_row("SELECT value FROM durable_value", [], |r| r.get(0))?)
            })
            .await
            .unwrap();
        assert_eq!(value, 73);
        assert!(adapter
            .read(JobOptions::default(), |conn| {
                conn.execute("INSERT INTO durable_value VALUES (99)", [])?;
                Ok(())
            })
            .await
            .is_err());
        assert!(matches!(
            Registry.open(DbId::new(&path).unwrap(), WriterOptions::default()),
            Err(StoreError::AlreadyOwned)
        ));
        readers.shutdown().await.unwrap();
        writer.shutdown().await.unwrap();
    }
}
