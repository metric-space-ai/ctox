//! Real owner-backed fixtures for integration-test preparation.
//!
//! The caller owns an isolated directory and must call `shutdown` before removing
//! it. Preparation runs on the same bounded writer as production jobs; no raw
//! connection escapes to a test or a Tokio worker. This API grants no exemption
//! to open a live daemon's file.

use crate::{
    config::{AdmissionBudget, ConnectionConfig},
    read_pool::ReadPool,
    registry::Registry,
    writer::{WriterHandle, WriterOptions},
    DbId, JobOptions, ReadJob, StoreError, WriteJob,
};
use rusqlite::{Connection, Transaction};

pub struct StoreFixture {
    writer: WriterHandle,
    readers: ReadPool,
}

impl StoreFixture {
    /// Acquire the real file owner. An already-owned file is rejected, including
    /// tests pointing accidentally at a running daemon's store.
    pub async fn open(id: DbId) -> Result<Self, StoreError> {
        let writer = Registry.open(id, WriterOptions::default())?;
        if let Err(error) = writer.ready().await {
            let _ = writer.shutdown().await;
            return Err(error);
        }
        let readers = match ReadPool::for_database(
            writer.db_id(),
            writer.generation(),
            ConnectionConfig::default(),
            AdmissionBudget::default(),
        )
        .await
        {
            Ok(readers) => readers,
            Err(error) => {
                let _ = writer.shutdown().await;
                return Err(error);
            }
        };
        Ok(Self { writer, readers })
    }

    pub fn writer(&self) -> WriterHandle {
        self.writer.clone()
    }

    pub fn readers(&self) -> ReadPool {
        self.readers.clone()
    }

    /// Transfer lifecycle custody to a component whose shutdown already drains
    /// both handles. No additional pool or connection is created.
    pub fn into_parts(self) -> (WriterHandle, ReadPool) {
        (self.writer, self.readers)
    }

    pub async fn write<T: Send + 'static>(
        &self,
        options: JobOptions,
        prepare: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        self.writer.submit(WriteJob::new(options, prepare))?.await
    }

    pub async fn read<T: Send + 'static>(
        &self,
        options: JobOptions,
        inspect: impl FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        self.readers.read_job(ReadJob::new(options, inspect)).await
    }

    /// Keep this fixture alive on a drain error and retry after the outstanding
    /// operation completes. File ownership is released only by the writer.
    pub async fn shutdown(&self) -> Result<(), StoreError> {
        self.writer.stop_admission();
        self.readers.shutdown().await?;
        self.writer.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preparation_preserves_real_owner_rollback_and_reader_isolation() {
        let directory = tempfile::tempdir().unwrap();
        let id = DbId::new(directory.path().join("fixture.sqlite3")).unwrap();
        let fixture = StoreFixture::open(id.clone()).await.unwrap();
        assert!(matches!(
            StoreFixture::open(id).await,
            Err(StoreError::AlreadyOwned)
        ));
        let caller = std::thread::current().id();
        fixture.write(Default::default(), move |tx| {
            assert_ne!(std::thread::current().id(), caller);
            tx.execute_batch("CREATE TABLE fixture_value(id INTEGER PRIMARY KEY); INSERT INTO fixture_value VALUES (1)")?;
            Ok(())
        }).await.unwrap();
        let result: Result<(), StoreError> = fixture
            .write(Default::default(), |tx| {
                tx.execute("INSERT INTO fixture_value VALUES (2)", [])?;
                Err(StoreError::Cancelled)
            })
            .await;
        assert!(matches!(result, Err(StoreError::Cancelled)));
        let value: i64 = fixture
            .read(Default::default(), |conn| {
                Ok(conn.query_row("SELECT SUM(id) FROM fixture_value", [], |r| r.get(0))?)
            })
            .await
            .unwrap();
        assert_eq!(value, 1, "failed preparation must roll back");
        assert!(fixture
            .read(Default::default(), |conn| {
                conn.execute("INSERT INTO fixture_value VALUES (3)", [])?;
                Ok(())
            })
            .await
            .is_err());
        fixture.shutdown().await.unwrap();
    }
}
