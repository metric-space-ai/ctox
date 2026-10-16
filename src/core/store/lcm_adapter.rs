//! Injected owners for legacy channel/LCM, Runtime and Secrets callers.
//! No connection is opened here. One registered adapter shares the supplied
//! writer/read pool. Weak registrations expire when their last handle drops.
use ctox_store::{
    read_pool::ReadPool, writer::WriterHandle, DbId, JobOptions, ReadJob, StoreError, WriteJob,
};
use rusqlite::{Connection, Transaction};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, OnceLock, Weak},
};

struct Inner {
    writer: WriterHandle,
    readers: ReadPool,
    runtime: tokio::runtime::Handle,
    publication: Mutex<bool>,
}
#[derive(Clone)]
pub(crate) struct OwnerAdapter {
    inner: Arc<Inner>,
}
pub(crate) struct PublicationReservation {
    inner: Arc<Inner>,
}
impl Drop for PublicationReservation {
    fn drop(&mut self) {
        *self
            .inner
            .publication
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = false;
    }
}

fn installed() -> &'static Mutex<HashMap<DbId, Weak<Inner>>> {
    static OWNERS: OnceLock<Mutex<HashMap<DbId, Weak<Inner>>>> = OnceLock::new();
    OWNERS.get_or_init(Default::default)
}
impl OwnerAdapter {
    pub(crate) async fn attach(
        path: &Path,
        writer: WriterHandle,
        readers: ReadPool,
    ) -> Result<Self, StoreError> {
        let identity_path = path.to_owned();
        let expected = tokio::task::spawn_blocking(move || DbId::new(identity_path))
            .await
            .map_err(|_| StoreError::Panicked)??;
        if writer.db_id() != &expected || readers.generation() != writer.generation().0 {
            return Err(StoreError::Invalid("store owner identity mismatch".into()));
        }
        writer.ready().await?;
        let reader_path: String = readers
            .read_job(ReadJob::new(JobOptions::default(), |conn| {
                Ok(conn.query_row(
                    "SELECT file FROM pragma_database_list WHERE name='main'",
                    [],
                    |row| row.get(0),
                )?)
            }))
            .await?;
        let reader_identity = tokio::task::spawn_blocking(move || DbId::new(reader_path))
            .await
            .map_err(|_| StoreError::Panicked)??;
        if reader_identity != expected {
            return Err(StoreError::Invalid("store reader identity mismatch".into()));
        }
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| StoreError::Invalid("existing runtime required".into()))?;
        let mut owners = installed().lock().unwrap_or_else(|e| e.into_inner());
        owners.retain(|_, owner| owner.strong_count() > 0);
        if let Some(inner) = owners.get(&expected).and_then(Weak::upgrade) {
            if inner.writer.generation() == writer.generation() {
                return Ok(Self { inner });
            }
        }
        let inner = Arc::new(Inner {
            writer,
            readers,
            runtime,
            publication: Mutex::new(false),
        });
        owners.insert(expected, Arc::downgrade(&inner));
        Ok(Self { inner })
    }
    pub(crate) fn for_path(path: &Path) -> Result<Self, StoreError> {
        let id = DbId::new(path)?;
        installed()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .and_then(Weak::upgrade)
            .map(|inner| Self { inner })
            .ok_or_else(|| StoreError::Invalid("file owner not injected".into()))
    }
    fn check_generation(&self) -> Result<(), StoreError> {
        let owners = installed().lock().unwrap_or_else(|e| e.into_inner());
        let current = owners
            .get(self.inner.writer.db_id())
            .and_then(Weak::upgrade);
        if self.inner.readers.generation() != self.inner.writer.generation().0
            || !current.is_some_and(|inner| Arc::ptr_eq(&inner, &self.inner))
        {
            return Err(StoreError::Invalid(
                "store adapter generation expired".into(),
            ));
        }
        Ok(())
    }
    pub(crate) async fn write<T: Send + 'static>(
        &self,
        options: JobOptions,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        self.submit_regular(options, operation)?.await
    }
    fn submit_regular<T: Send + 'static>(
        &self,
        options: JobOptions,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<ctox_store::JobHandle<T>, StoreError> {
        self.check_generation()?;
        let reserved = self
            .inner
            .publication
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *reserved {
            return Err(StoreError::Busy);
        }
        self.inner.writer.submit(WriteJob::new(options, operation))
    }
    /// Admission and reservation share one short lock. Never queue a native
    /// lock holder behind its own issuer callback, and never hold this mutex
    /// while the callback runs. The returned capability clears only on release.
    pub(crate) fn submit_publication(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<(), StoreError> + Send + 'static,
    ) -> Result<(ctox_store::JobHandle<()>, PublicationReservation), StoreError> {
        crate::persistence::require_sync_owner_context()
            .map_err(|_| StoreError::Invalid("sync caller requires run_core_sync".into()))?;
        self.check_generation()?;
        let mut reserved = self
            .inner
            .publication
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *reserved {
            return Err(StoreError::Busy);
        }
        let handle = self
            .inner
            .writer
            .submit(WriteJob::new(JobOptions::default(), operation))?;
        *reserved = true;
        Ok((
            handle,
            PublicationReservation {
                inner: self.inner.clone(),
            },
        ))
    }
    pub(crate) async fn read<T: Send + 'static>(
        &self,
        options: JobOptions,
        operation: impl FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        self.check_generation()?;
        let value = self
            .inner
            .readers
            .read_job(ReadJob::new(options, operation))
            .await?;
        // A completed old snapshot must not escape after generation renewal.
        self.check_generation()?;
        Ok(value)
    }
    /// Only the bounded run_core_sync compatibility executor may wait. Never
    /// wait on a Tokio worker, on the writer callback, or on a reader callback.
    /// Nonblocking admission for a compatibility publication handshake. The
    /// caller retains its borrowed callback on the admitted blocking worker.
    pub(crate) fn submit_sync<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<ctox_store::JobHandle<T>, StoreError> {
        crate::persistence::require_sync_owner_context()
            .map_err(|_| StoreError::Invalid("sync caller requires run_core_sync".into()))?;
        self.check_generation()?;
        self.submit_regular(JobOptions::default(), operation)
    }
    pub(crate) fn write_blocking<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        crate::persistence::require_sync_owner_context()
            .map_err(|_| StoreError::Invalid("sync caller requires run_core_sync".into()))?;
        self.check_generation()?;
        self.submit_regular(JobOptions::default(), operation)?
            .blocking_wait()
    }
    pub(crate) fn read_blocking<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        crate::persistence::require_sync_owner_context()
            .map_err(|_| StoreError::Invalid("sync caller requires run_core_sync".into()))?;
        self.check_generation()?;
        let readers = self.inner.readers.clone();
        let (reply, result) = tokio::sync::oneshot::channel();
        self.inner.runtime.spawn(async move {
            let value = readers
                .read_job(ReadJob::new(JobOptions::default(), operation))
                .await;
            let _ = reply.send(value);
        });
        let value = result.blocking_recv().unwrap_or(Err(StoreError::Closed))?;
        self.check_generation()?;
        Ok(value)
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
        // Admit a real read but deliberately leave its completion unpolled.
        // Shutdown drains the pool; a new owner generation then replaces it.
        let (release, blocked) = std::sync::mpsc::channel();
        let mut old_snapshot = Box::pin(adapter.read(JobOptions::default(), move |conn| {
            blocked.recv().map_err(|_| StoreError::Closed)?;
            Ok(conn.query_row("SELECT value FROM durable_value", [], |r| {
                r.get::<_, i64>(0)
            })?)
        }));
        struct CompletionWake(tokio::sync::Notify);
        impl std::task::Wake for CompletionWake {
            fn wake(self: Arc<Self>) {
                self.0.notify_one();
            }
        }
        let completed = Arc::new(CompletionWake(tokio::sync::Notify::new()));
        {
            use std::future::Future;
            let waker = std::task::Waker::from(completed.clone());
            let mut cx = std::task::Context::from_waker(&waker);
            assert!(old_snapshot.as_mut().poll(&mut cx).is_pending());
        }
        release.send(()).unwrap();
        // JoinHandle wakes only after the real blocking snapshot has completed;
        // leave its successful result queued while the owner is replaced.
        completed.0.notified().await;
        readers.shutdown().await.unwrap();
        writer.shutdown().await.unwrap();
        let (next_writer, next_readers) =
            ctox_store::testkit::StoreFixture::open(DbId::new(&path).unwrap())
                .await
                .unwrap()
                .into_parts();
        let current = OwnerAdapter::attach(&path, next_writer.clone(), next_readers.clone())
            .await
            .unwrap();
        assert!(
            matches!(old_snapshot.await, Err(StoreError::Invalid(_))),
            "completed snapshot escaped its retired generation"
        );
        assert_eq!(
            current
                .read(JobOptions::default(), |conn| Ok(conn.query_row(
                    "SELECT value FROM durable_value",
                    [],
                    |r| r.get::<_, i64>(0)
                )?))
                .await
                .unwrap(),
            73
        );
        drop(current);
        assert!(
            OwnerAdapter::for_path(&path).is_err(),
            "registration must not retain a dead adapter"
        );
        next_readers.shutdown().await.unwrap();
        next_writer.shutdown().await.unwrap();
    }
}
