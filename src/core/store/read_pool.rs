//! Four WAL snapshot readers per registry-owned database, never on an async worker.
//!
//! The registry owns one pool per DbId. Closures return owned results, not SQLite
//! handles. All pools share a process-wide bound on blocking work (including open
//! and generation renewal), and each pool separately bounds admitted jobs.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::config::{AdmissionBudget, ConnectionConfig, MAX_BLOCKING_READS, READ_CONNECTIONS};
use crate::{DbGeneration, DbId, JobOptions, ReadJob, StoreError};

#[derive(Debug)]
pub enum ReadPoolError {
    Busy,
    AdmissionTimeout,
    ExecutionTimeout,
    Cancelled,
    Closed,
    GenerationChanged { expected: u64, actual: u64 },
    FileReplaced,
    InvalidBudget,
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Worker(String),
    Store(StoreError),
}

impl fmt::Display for ReadPoolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => formatter.write_str("store reader admission is full"),
            Self::AdmissionTimeout => {
                formatter.write_str("store reader admission deadline exceeded")
            }
            Self::ExecutionTimeout => {
                formatter.write_str("store reader execution deadline exceeded")
            }
            Self::Cancelled => formatter.write_str("store reader job cancelled"),
            Self::Closed => formatter.write_str("store reader pool closed"),
            Self::GenerationChanged { expected, actual } => write!(
                formatter,
                "reader generation changed from {expected} to {actual}"
            ),
            Self::FileReplaced => {
                formatter.write_str("store file replaced; renew the reader generation")
            }
            Self::InvalidBudget => {
                formatter.write_str("reader admission and execution budgets must be nonzero")
            }
            Self::Io(error) => error.fmt(formatter),
            Self::Sqlite(error) => error.fmt(formatter),
            Self::Worker(error) => write!(formatter, "blocking reader failed: {error}"),
            Self::Store(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ReadPoolError {}
impl From<rusqlite::Error> for ReadPoolError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}
impl From<std::io::Error> for ReadPoolError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ReadPoolError> for StoreError {
    fn from(error: ReadPoolError) -> Self {
        match error {
            ReadPoolError::Busy => Self::Busy,
            ReadPoolError::AdmissionTimeout | ReadPoolError::ExecutionTimeout => {
                Self::DeadlineExceeded
            }
            ReadPoolError::Cancelled => Self::Cancelled,
            ReadPoolError::Closed => Self::Closed,
            ReadPoolError::Store(error) => error,
            ReadPoolError::Sqlite(error) => error.into(),
            ReadPoolError::Io(error) => error.into(),
            ReadPoolError::Worker(_) => Self::Panicked,
            other => Self::Invalid(other.to_string()),
        }
    }
}

fn blocking_budget() -> Arc<Semaphore> {
    static BLOCKING: OnceLock<Arc<Semaphore>> = OnceLock::new();
    BLOCKING
        .get_or_init(|| Arc::new(Semaphore::new(MAX_BLOCKING_READS)))
        .clone()
}

#[derive(Debug, PartialEq, Eq)]
struct FileIdentity {
    canonical: PathBuf,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(not(unix))]
    created: Option<std::time::SystemTime>,
}

impl FileIdentity {
    fn at(path: &Path) -> Result<Self, ReadPoolError> {
        let metadata = std::fs::metadata(path)?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            canonical: std::fs::canonicalize(path)?,
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
            #[cfg(not(unix))]
            created: metadata.created().ok(),
        })
    }
}

struct Readers {
    connections: Vec<Connection>,
    identity: FileIdentity,
}

struct Inner {
    path: PathBuf,
    config: ConnectionConfig,
    budget: AdmissionBudget,
    generation: AtomicU64,
    closed: AtomicBool,
    admission: Arc<Semaphore>,
    slots: Arc<Semaphore>,
    readers: Mutex<Readers>,
}

#[derive(Clone)]
pub struct ReadPool(Arc<Inner>);

impl ReadPool {
    /// The registry's typed entry point; it keeps one pool for this DbId.
    pub async fn for_database(
        db: &DbId,
        generation: DbGeneration,
        config: ConnectionConfig,
        budget: AdmissionBudget,
    ) -> Result<Self, StoreError> {
        Self::open(db.path(), generation.0, config, budget)
            .await
            .map_err(Into::into)
    }

    /// Called by the registry once per DbId after writer initialization. No file
    /// creation, journal migration or blocking filesystem work on this caller.
    pub async fn open(
        path: impl Into<PathBuf>,
        generation: u64,
        config: ConnectionConfig,
        budget: AdmissionBudget,
    ) -> Result<Self, ReadPoolError> {
        if AdmissionBudget::new(budget.capacity, budget.wait, budget.execution).is_none() {
            return Err(ReadPoolError::InvalidBudget);
        }
        let path = path.into();
        let permit = acquire(blocking_budget(), budget.wait).await?;
        let open_path = path.clone();
        let readers = blocking(move || {
            let _permit = permit;
            open_readers(&open_path, config)
        })
        .await?;
        Ok(Self(Arc::new(Inner {
            path,
            config,
            budget,
            generation: AtomicU64::new(generation),
            closed: AtomicBool::new(false),
            admission: Arc::new(Semaphore::new(budget.capacity.get())),
            slots: Arc::new(Semaphore::new(READ_CONNECTIONS)),
            readers: Mutex::new(readers),
        })))
    }

    pub fn generation(&self) -> u64 {
        self.0.generation.load(Ordering::Acquire)
    }

    pub fn close(&self) {
        self.0.closed.store(true, Ordering::Release);
        self.0.admission.close();
    }

    /// Owner shutdown: stop admission, drain all snapshots and close the four
    /// handles on a bounded blocking worker, not on the daemon's async worker.
    pub async fn shutdown(&self) -> Result<(), ReadPoolError> {
        self.close();
        let deadline = tokio::time::Instant::now() + self.0.budget.wait;
        let slots = tokio::time::timeout_at(
            deadline,
            self.0
                .slots
                .clone()
                .acquire_many_owned(READ_CONNECTIONS as u32),
        )
        .await
        .map_err(|_| ReadPoolError::AdmissionTimeout)?
        .map_err(|_| ReadPoolError::Closed)?;
        let permit = acquire_until(blocking_budget(), deadline).await?;
        let inner = self.0.clone();
        blocking(move || {
            let (_slots, _permit) = (slots, permit);
            let connections = std::mem::take(
                &mut inner
                    .readers
                    .lock()
                    .map_err(|_| ReadPoolError::Worker("reader mutex poisoned".into()))?
                    .connections,
            );
            drop(connections);
            Ok(())
        })
        .await
    }

    pub async fn read<T, F>(&self, job: F) -> Result<T, ReadPoolError>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> rusqlite::Result<T> + Send + 'static,
    {
        self.read_with_options(JobOptions::default(), move |transaction| {
            job(transaction).map_err(Into::into)
        })
        .await
    }

    /// Common P3-01 job contract. User cancellation/deadline is checked before
    /// admission, within SQLite's progress handler and before returning a page.
    pub async fn read_job<T: Send + 'static>(&self, job: ReadJob<T>) -> Result<T, StoreError> {
        self.read_with_options(job.options, move |transaction| {
            (job.operation)(transaction).map_err(ReadPoolError::Store)
        })
        .await
        .map_err(Into::into)
    }

    async fn read_with_options<T, F>(&self, options: JobOptions, job: F) -> Result<T, ReadPoolError>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> Result<T, ReadPoolError> + Send + 'static,
    {
        options.check().map_err(ReadPoolError::Store)?;
        if self.0.closed.load(Ordering::Acquire) {
            return Err(ReadPoolError::Closed);
        }
        let admission =
            self.0
                .admission
                .clone()
                .try_acquire_owned()
                .map_err(|error| match error {
                    tokio::sync::TryAcquireError::Closed => ReadPoolError::Closed,
                    tokio::sync::TryAcquireError::NoPermits => ReadPoolError::Busy,
                })?;
        let generation = self.generation();
        let admission_deadline = options.deadline.map_or_else(
            || Instant::now() + self.0.budget.wait,
            |deadline| deadline.min(Instant::now() + self.0.budget.wait),
        );
        let deadline = tokio::time::Instant::from_std(admission_deadline);
        let slot = acquire_until(self.0.slots.clone(), deadline).await?;
        let permit = acquire_until(blocking_budget(), deadline).await?;
        let cancelled = CancelOnDrop(Arc::new(AtomicBool::new(false)));
        let cancel = cancelled.0.clone();
        let inner = self.0.clone();
        // Permits live IN the blocking closure, even if the calling future is
        // dropped. Cancellation cannot free capacity while SQLite is still busy.
        let result = blocking(move || {
            let (_admission, _slot, _permit) = (admission, slot, permit);
            options.check().map_err(ReadPoolError::Store)?;
            if Instant::now() >= admission_deadline {
                return Err(ReadPoolError::AdmissionTimeout);
            }
            inner.check(generation)?;
            let connection = {
                let mut readers = inner
                    .readers
                    .lock()
                    .map_err(|_| ReadPoolError::Worker("reader mutex poisoned".into()))?;
                if FileIdentity::at(&inner.path)? != readers.identity {
                    return Err(ReadPoolError::FileReplaced);
                }
                readers
                    .connections
                    .pop()
                    .ok_or_else(|| ReadPoolError::Worker("missing admitted reader".into()))?
            };
            let mut lease = ReaderLease {
                connection: Some(connection),
                inner: inner.clone(),
            };
            let connection = lease.connection.as_mut().expect("owned reader");
            let until = options.deadline.map_or_else(
                || Instant::now() + inner.budget.execution,
                |deadline| deadline.min(Instant::now() + inner.budget.execution),
            );
            let interrupted = cancel.clone();
            let external_cancel = options.cancellation.clone();
            connection.progress_handler(
                1000,
                Some(move || {
                    interrupted.load(Ordering::Acquire)
                        || external_cancel.is_cancelled()
                        || Instant::now() >= until
                }),
            );
            let result = (|| {
                if cancel.load(Ordering::Acquire) {
                    return Err(ReadPoolError::Cancelled);
                }
                let transaction =
                    connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
                // BEGIN DEFERRED alone has no snapshot. Pin it BEFORE invoking
                // the job, so schema, head_seq, changes and documents agree.
                transaction.query_row("SELECT count(*) FROM sqlite_schema", [], |row| {
                    row.get::<_, i64>(0)
                })?;
                let value = job(&transaction)?;
                transaction.rollback()?;
                options.check().map_err(ReadPoolError::Store)?;
                inner.check(generation)?;
                let readers = inner
                    .readers
                    .lock()
                    .map_err(|_| ReadPoolError::Worker("reader mutex poisoned".into()))?;
                if FileIdentity::at(&inner.path)? != readers.identity {
                    return Err(ReadPoolError::FileReplaced);
                }
                Ok(value)
            })();
            connection.progress_handler(0, None::<fn() -> bool>);
            if cancel.load(Ordering::Acquire) || options.cancellation.is_cancelled() {
                Err(ReadPoolError::Cancelled)
            } else if Instant::now() >= until {
                Err(ReadPoolError::ExecutionTimeout)
            } else {
                result
            }
        })
        .await;
        drop(cancelled);
        result
    }

    /// Drain in-flight snapshots, build four fresh readers, then publish the
    /// generation. A failed reopen leaves the prior generation intact. Queued
    /// jobs captured on the old generation fail instead of crossing cutover.
    pub async fn renew_generation(&self, expected: u64, next: u64) -> Result<(), ReadPoolError> {
        self.0.check(expected)?;
        let admission =
            self.0
                .admission
                .clone()
                .try_acquire_owned()
                .map_err(|error| match error {
                    tokio::sync::TryAcquireError::Closed => ReadPoolError::Closed,
                    tokio::sync::TryAcquireError::NoPermits => ReadPoolError::Busy,
                })?;
        if next <= expected {
            return Err(ReadPoolError::GenerationChanged {
                expected: expected.saturating_add(1),
                actual: next,
            });
        }
        let deadline = tokio::time::Instant::now() + self.0.budget.wait;
        let slots = tokio::time::timeout_at(
            deadline,
            self.0
                .slots
                .clone()
                .acquire_many_owned(READ_CONNECTIONS as u32),
        )
        .await
        .map_err(|_| ReadPoolError::AdmissionTimeout)?
        .map_err(|_| ReadPoolError::Closed)?;
        let permit = acquire_until(blocking_budget(), deadline).await?;
        let inner = self.0.clone();
        blocking(move || {
            let (_admission, _slots, _permit) = (admission, slots, permit);
            inner.check(expected)?;
            let readers = open_readers(&inner.path, inner.config)?;
            *inner
                .readers
                .lock()
                .map_err(|_| ReadPoolError::Worker("reader mutex poisoned".into()))? = readers;
            inner.generation.store(next, Ordering::Release);
            Ok(())
        })
        .await
    }
}

impl Inner {
    fn check(&self, expected: u64) -> Result<(), ReadPoolError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ReadPoolError::Closed);
        }
        let actual = self.generation.load(Ordering::Acquire);
        if actual != expected {
            return Err(ReadPoolError::GenerationChanged { expected, actual });
        }
        Ok(())
    }
}

struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

struct ReaderLease {
    connection: Option<Connection>,
    inner: Arc<Inner>,
}
impl Drop for ReaderLease {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            connection.progress_handler(0, None::<fn() -> bool>);
            // This guard also runs while unwinding a panicking callback.
            // No failed job may leave a transaction or a changed query policy
            // attached to a connection returned to another caller.
            let rollback_ok =
                connection.is_autocommit() || connection.execute_batch("ROLLBACK").is_ok();
            if rollback_ok && self.inner.config.apply_reader(&connection).is_ok() {
                if let Ok(mut readers) = self.inner.readers.lock() {
                    readers.connections.push(connection);
                }
            } else {
                // Do not silently shrink four slots to three usable handles.
                // A connection that cannot be made clean poisons this pool.
                self.inner.closed.store(true, Ordering::Release);
                self.inner.admission.close();
            }
        }
    }
}

fn open_readers(path: &Path, config: ConnectionConfig) -> Result<Readers, ReadPoolError> {
    let identity = FileIdentity::at(path)?;
    let mut connections = Vec::with_capacity(READ_CONNECTIONS);
    for _ in 0..READ_CONNECTIONS {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        config.apply_reader(&connection)?;
        connections.push(connection);
    }
    if FileIdentity::at(path)? != identity {
        return Err(ReadPoolError::FileReplaced);
    }
    Ok(Readers {
        connections,
        identity,
    })
}

async fn acquire(
    semaphore: Arc<Semaphore>,
    wait: std::time::Duration,
) -> Result<OwnedSemaphorePermit, ReadPoolError> {
    acquire_until(semaphore, tokio::time::Instant::now() + wait).await
}

async fn acquire_until(
    semaphore: Arc<Semaphore>,
    deadline: tokio::time::Instant,
) -> Result<OwnedSemaphorePermit, ReadPoolError> {
    tokio::time::timeout_at(deadline, semaphore.acquire_owned())
        .await
        .map_err(|_| ReadPoolError::AdmissionTimeout)?
        .map_err(|_| ReadPoolError::Closed)
}

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, ReadPoolError> + Send + 'static,
) -> Result<T, ReadPoolError> {
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|error| ReadPoolError::Worker(error.to_string()))?
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
