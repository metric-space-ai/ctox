//! Shared contract for writer, reader, schema and change-log strands.

use rusqlite::{Connection, Transaction};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Instant,
};
use tokio::sync::oneshot;

/// File identity resolved before registration. Parent directories must exist.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DbId(PathBuf);
impl DbId {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        let resolved = if path.exists() {
            path.canonicalize()?
        } else {
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()?.join(path)
            };
            let name = absolute
                .file_name()
                .ok_or_else(|| StoreError::Invalid("database needs a filename".into()))?;
            absolute
                .parent()
                .ok_or_else(|| StoreError::Invalid("database needs a parent".into()))?
                .canonicalize()?
                .join(name)
        };
        #[cfg(unix)]
        if let Ok(metadata) = resolved.metadata() {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() > 1 {
                return Err(StoreError::Invalid(
                    "hard-linked database files cannot have a unique path owner".into(),
                ));
            }
        }
        Ok(Self(resolved))
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// Process-local owner incarnation; persisted collection generations are separate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DbGeneration(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreError {
    Busy,
    Cancelled,
    DeadlineExceeded,
    Closed,
    AlreadyOwned,
    Invalid(String),
    Sqlite(String),
    Io(String),
    Panicked,
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for StoreError {}
impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        match error.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
                Self::Busy
            }
            _ => Self::Sqlite(error.to_string()),
        }
    }
}
impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Checked before execution and again before commit. Running closures must stay
/// short; cancellation never interrupts arbitrary Rust code or undoes a commit.
#[derive(Clone, Default)]
pub struct JobOptions {
    pub deadline: Option<Instant>,
    pub cancellation: Cancellation,
    /// Only adjacent writes with the same explicit key may share a transaction.
    pub batch_key: Option<String>,
}
impl JobOptions {
    pub fn check(&self) -> Result<(), StoreError> {
        if self.cancellation.is_cancelled() {
            return Err(StoreError::Cancelled);
        }
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(StoreError::DeadlineExceeded);
        }
        Ok(())
    }
}

pub struct JobHandle<T>(pub(crate) oneshot::Receiver<Result<T, StoreError>>);
impl<T> Future for JobHandle<T> {
    type Output = Result<T, StoreError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0)
            .poll(cx)
            .map(|result| result.unwrap_or(Err(StoreError::Closed)))
    }
}
impl<T> JobHandle<T> {
    /// Only for synchronous callers outside an async runtime.
    pub fn blocking_wait(self) -> Result<T, StoreError> {
        self.0.blocking_recv().unwrap_or(Err(StoreError::Closed))
    }
}

pub type ReadOperation<T> = Box<dyn FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static>;
pub type WriteOperation<T> =
    Box<dyn FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static>;

pub struct ReadJob<T> {
    pub options: JobOptions,
    pub operation: ReadOperation<T>,
}
impl<T> ReadJob<T> {
    pub fn new(
        options: JobOptions,
        operation: impl FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static,
    ) -> Self {
        Self {
            options,
            operation: Box::new(operation),
        }
    }
}
pub struct WriteJob<T> {
    pub options: JobOptions,
    pub operation: WriteOperation<T>,
}
impl<T> WriteJob<T> {
    pub fn new(
        options: JobOptions,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError> + Send + 'static,
    ) -> Self {
        Self {
            options,
            operation: Box::new(operation),
        }
    }
}
