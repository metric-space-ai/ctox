//! File ownership shared by SQLx harness pools and dedicated native actors.
//! Included by the owning crates until their dependency boundaries are migrated.

use std::{
    fs::{File, OpenOptions},
    future::Future,
    io,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
};

pub struct FileOwner {
    file: File,
    path: PathBuf,
    exclusive: bool,
}

impl FileOwner {
    pub fn identity(path: &Path) -> io::Result<PathBuf> {
        let path = if path.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if path.metadata()?.nlink() > 1 {
                    return Err(io::Error::other(
                        "hard-linked database cannot have a unique owner",
                    ));
                }
            }
            path.canonicalize()?
        } else {
            let absolute = std::path::absolute(path)?;
            absolute
                .parent()
                .ok_or_else(|| io::Error::other("database parent missing"))?
                .canonicalize()?
                .join(
                    absolute
                        .file_name()
                        .ok_or_else(|| io::Error::other("database filename missing"))?,
                )
        };
        Ok(path)
    }

    pub fn acquire(path: &Path) -> io::Result<Self> {
        let path = Self::identity(path)?;
        let mut lock = path.as_os_str().to_os_string();
        lock.push(".ctox-store.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock)?;
        file.try_lock()
            .map_err(|error| io::Error::other(format!("database already owned: {error}")))?;
        Ok(Self {
            file,
            path,
            exclusive: true,
        })
    }

    /// Temporary compatibility access: multiple legacy connections may coexist,
    /// but none can overlap the exclusive file owner. Hold until SQLite closes.
    pub fn acquire_legacy_shared(path: &Path) -> io::Result<Self> {
        let path = Self::identity(path)?;
        let mut lock = path.as_os_str().to_os_string();
        lock.push(".ctox-store.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock)?;
        file.try_lock_shared()
            .map_err(|error| io::Error::other(format!("database already owned: {error}")))?;
        Ok(Self {
            file,
            path,
            exclusive: false,
        })
    }

    pub fn require_exclusive(&self) -> io::Result<()> {
        if self.exclusive {
            Ok(())
        } else {
            Err(io::Error::other("exclusive file owner required"))
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for FileOwner {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub const OWNER_QUEUE_CAPACITY: usize = 1024;
type Operation<C> = Box<dyn FnOnce(&mut C) + Send + 'static>;

/// One initialized native connection, one OS thread, bounded nonblocking admission.
/// The connection never leaves that thread. Dropped callers do not cancel a commit.
pub struct BoundedOwner<C> {
    sender: Arc<mpsc::SyncSender<Operation<C>>>,
    failure: Arc<Mutex<Option<String>>>,
    closed: tokio::sync::watch::Receiver<bool>,
}

impl<C> Clone for BoundedOwner<C> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            failure: self.failure.clone(),
            closed: self.closed.clone(),
        }
    }
}

impl<C: Send + 'static> BoundedOwner<C> {
    pub fn start(
        path: PathBuf,
        initialize: impl FnOnce(&Path) -> io::Result<C> + Send + 'static,
    ) -> io::Result<Self> {
        Self::start_with_capacity(path, OWNER_QUEUE_CAPACITY, initialize)
    }

    fn start_with_capacity(
        path: PathBuf,
        capacity: usize,
        initialize: impl FnOnce(&Path) -> io::Result<C> + Send + 'static,
    ) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Operation<C>>(capacity);
        let (closed_sender, closed) = tokio::sync::watch::channel(false);
        let failure = Arc::new(Mutex::new(None));
        let worker_failure = failure.clone();
        std::thread::Builder::new()
            .name("ctox-file-owner".into())
            .spawn(move || {
                let opened = (|| {
                    let lease = if path == Path::new(":memory:") {
                        None
                    } else {
                        Some(FileOwner::acquire(&path)?)
                    };
                    let connection =
                        initialize(lease.as_ref().map_or(path.as_path(), FileOwner::path))?;
                    Ok::<_, io::Error>((lease, connection))
                })();
                match opened {
                    Ok((lease, mut connection)) => {
                        for operation in receiver {
                            operation(&mut connection);
                        }
                        // Close the actual connection before releasing its canonical file lease.
                        drop(connection);
                        drop(lease);
                    }
                    Err(error) => {
                        *worker_failure.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(error.to_string());
                    }
                }
                let _ = closed_sender.send(true);
            })?;
        Ok(Self {
            sender: Arc::new(sender),
            failure,
            closed,
        })
    }

    pub fn closed_signal(&self) -> tokio::sync::watch::Receiver<bool> {
        self.closed.clone()
    }

    fn closed(&self) -> io::Error {
        io::Error::other(
            self.failure
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .unwrap_or_else(|| "database owner closed".into()),
        )
    }

    pub fn try_call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut C) -> io::Result<T> + Send + 'static,
    ) -> io::Result<tokio::sync::oneshot::Receiver<io::Result<T>>> {
        let (reply, result) = tokio::sync::oneshot::channel();
        let job = Box::new(move |connection: &mut C| {
            let value =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(connection)))
                    .unwrap_or_else(|_| Err(io::Error::other("database owner operation panicked")));
            let _ = reply.send(value);
        });
        self.sender.try_send(job).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => {
                io::Error::new(io::ErrorKind::WouldBlock, "database owner queue full")
            }
            mpsc::TrySendError::Disconnected(_) => self.closed(),
        })?;
        Ok(result)
    }

    pub async fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut C) -> io::Result<T> + Send + 'static,
    ) -> io::Result<T> {
        self.try_call(operation)?.await.map_err(|_| self.closed())?
    }
}

/// SQLx pool lease: clones keep the owner alive; the owner thread opens,
/// migrates and closes using the existing Tokio handle, never a new runtime.
pub struct OwnedResource<C> {
    resource: C,
    lifetime: Arc<mpsc::SyncSender<()>>,
    closed: tokio::sync::watch::Receiver<bool>,
}
impl<C: Clone> Clone for OwnedResource<C> {
    fn clone(&self) -> Self {
        Self {
            resource: self.resource.clone(),
            lifetime: self.lifetime.clone(),
            closed: self.closed.clone(),
        }
    }
}
impl<C> OwnedResource<C> {
    pub fn closed_signal(&self) -> tokio::sync::watch::Receiver<bool> {
        self.closed.clone()
    }
}
// Keeps only a weak lifetime token. A cached pool handle cannot keep an
// owner alive: the owner thread explicitly closes every connection first.
pub struct WeakResource<C> {
    resource: C,
    lifetime: std::sync::Weak<mpsc::SyncSender<()>>,
    closed: tokio::sync::watch::Receiver<bool>,
}
impl<C: Clone> OwnedResource<C> {
    pub fn downgrade(&self) -> WeakResource<C> {
        WeakResource {
            resource: self.resource.clone(),
            lifetime: Arc::downgrade(&self.lifetime),
            closed: self.closed.clone(),
        }
    }
}
impl<C: Clone> WeakResource<C> {
    pub fn upgrade(&self) -> Option<OwnedResource<C>> {
        Some(OwnedResource {
            resource: self.resource.clone(),
            lifetime: self.lifetime.upgrade()?,
            closed: self.closed.clone(),
        })
    }
    pub fn finished(&self) -> bool {
        *self.closed.borrow()
    }
    pub fn closed_signal(&self) -> tokio::sync::watch::Receiver<bool> {
        self.closed.clone()
    }
}
impl<C> std::ops::Deref for OwnedResource<C> {
    type Target = C;
    fn deref(&self) -> &C {
        &self.resource
    }
}
impl<C> AsRef<C> for OwnedResource<C> {
    fn as_ref(&self) -> &C {
        &self.resource
    }
}

// Only called on the dedicated owner thread. SQLite SQLx close waits for its
// own native worker. Wake-driven parking is not polling or an async-worker wait.
fn finish_on_owner_thread<F: Future>(future: F) -> F::Output {
    struct Notify(std::thread::Thread);
    impl Wake for Notify {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Notify(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park(),
        }
    }
}

pub async fn open_async_owned<C, F, Close>(
    path: PathBuf,
    initialize: impl FnOnce(Arc<FileOwner>) -> F + Send + 'static,
    close: impl FnOnce(C) -> Close + Send + 'static,
) -> io::Result<OwnedResource<C>>
where
    C: Clone + Send + 'static,
    F: Future<Output = io::Result<C>> + Send + 'static,
    Close: Future<Output = ()> + Send + 'static,
{
    let handle = tokio::runtime::Handle::try_current().map_err(io::Error::other)?;
    let (ready, result) = tokio::sync::oneshot::channel();
    let (sender, receiver) = mpsc::sync_channel(1);
    let (closed_sender, closed) = tokio::sync::watch::channel(false);
    std::thread::Builder::new()
        .name("ctox-sqlx-owner".into())
        .spawn(move || {
            let lease = match FileOwner::acquire(&path) {
                Ok(lease) => Arc::new(lease),
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return;
                }
            };
            let _context = handle.enter();
            // SQLx initialization must be polled as a real task on the existing
            // runtime (timers/cooperative scheduling included). The dedicated
            // owner keeps the file lease and waits only for that task's result.
            let initialized = finish_on_owner_thread(handle.spawn(initialize(lease.clone())))
                .map_err(io::Error::other)
                .and_then(|result| result);
            match initialized {
                Ok(resource) => {
                    // If startup's caller was cancelled, send drops this wrapper,
                    // disconnects receiver and still closes before releasing lease.
                    let _ = ready.send(Ok(OwnedResource {
                        resource: resource.clone(),
                        lifetime: Arc::new(sender),
                        closed,
                    }));
                    let _ = receiver.recv();
                    finish_on_owner_thread(close(resource));
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                }
            }
            drop(lease);
            let _ = closed_sender.send(true);
        })?;
    result
        .await
        .map_err(|_| io::Error::other("database owner startup closed"))?
}
