//! One connection on one OS thread; bounded, non-blocking admission.
//!
//! Operations are trusted, short native closures. They must not perform I/O,
//! issue transaction-control SQL, or escape the supplied transaction. Long
//! migrations compute outside this thread and submit bounded write chunks.
use crate::types::*;
use rusqlite::{Connection, Transaction, TransactionBehavior};
use std::{
    fs::{File, OpenOptions},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, watch};

pub const QUEUE_CAPACITY: usize = 1024;
const MAX_BATCH: usize = 64;
const BATCH_BUDGET: Duration = Duration::from_millis(10);
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Read by the writer *inside* the committing transaction. Only Sync-Store
/// installs this hook; other databases have no published sequence (§2.2).
/// Hook errors roll back the writes, never publish a guessed head.
pub type HeadSeqHook = Arc<dyn Fn(&Connection) -> Result<u64, StoreError> + Send + Sync>;
pub type ConfigureHook = Arc<dyn Fn(&Connection) -> Result<(), StoreError> + Send + Sync>;

#[derive(Clone, Default)]
pub struct WriterOptions {
    pub head_seq: Option<HeadSeqHook>,
    /// P3-04 installs the canonical configuration here. Called on the writer.
    pub configure: Option<ConfigureHook>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Starting,
    Ready,
    Closing,
    Stopped(Result<(), StoreError>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitNotification {
    pub db_id: DbId,
    pub generation: DbGeneration,
    pub head_seq: u64,
}

/// A watch retains the newest committed head. Intermediate heads may coalesce:
/// readers drain `_changes` by cursor, not by counting wakeups (§16.1).
pub type CommitReceiver = watch::Receiver<Option<CommitNotification>>;
type Completion = Box<dyn FnOnce(Option<StoreError>) + Send>;
struct Outcome {
    finish: Completion,
    successful: bool,
}
trait Pending: Send {
    fn options(&self) -> &JobOptions;
    fn run(self: Box<Self>, tx: &Transaction<'_>) -> Outcome;
    fn reject(self: Box<Self>, error: StoreError);
}
struct TypedPending<T> {
    job: WriteJob<T>,
    reply: oneshot::Sender<Result<T, StoreError>>,
}
impl<T: Send + 'static> Pending for TypedPending<T> {
    fn options(&self) -> &JobOptions {
        &self.job.options
    }
    fn run(self: Box<Self>, tx: &Transaction<'_>) -> Outcome {
        let TypedPending { job, reply } = *self;
        let result = job
            .options
            .check()
            .and_then(|()| {
                catch_unwind(AssertUnwindSafe(|| (job.operation)(tx)))
                    .unwrap_or(Err(StoreError::Panicked))
            })
            .and_then(|value| {
                job.options.check()?;
                Ok(value)
            });
        let successful = result.is_ok();
        Outcome {
            successful,
            finish: Box::new(move |override_error| {
                let _ = reply.send(override_error.map_or(result, Err));
            }),
        }
    }
    fn reject(self: Box<Self>, error: StoreError) {
        let _ = self.reply.send(Err(error));
    }
}

pub(crate) struct Inner {
    pub(crate) db_id: DbId,
    pub(crate) generation: DbGeneration,
    queue: Mutex<Option<SyncSender<Box<dyn Pending>>>>,
    state: watch::Sender<Lifecycle>,
    commits: watch::Sender<Option<CommitNotification>>,
}
impl Inner {
    fn close(&self) {
        // Admission and close share this tiny lock: every accepted job precedes
        // disconnection and is drained; no shutdown marker needs a queue slot.
        let mut queue = self.queue.lock().unwrap_or_else(|p| p.into_inner());
        if queue.take().is_some() && !matches!(*self.state.borrow(), Lifecycle::Stopped(_)) {
            self.state.send_replace(Lifecycle::Closing);
        }
    }
    pub(crate) fn stopped(&self) -> bool {
        matches!(*self.state.borrow(), Lifecycle::Stopped(_))
    }
}
struct LastHandle(Weak<Inner>);
impl Drop for LastHandle {
    fn drop(&mut self) {
        if let Some(inner) = self.0.upgrade() {
            inner.close();
        }
    }
}
#[derive(Clone)]
pub struct WriterHandle {
    pub(crate) inner: Arc<Inner>,
    _last: Arc<LastHandle>,
}
impl WriterHandle {
    pub fn db_id(&self) -> &DbId {
        &self.inner.db_id
    }
    pub fn generation(&self) -> DbGeneration {
        self.inner.generation
    }
    pub fn lifecycle(&self) -> Lifecycle {
        self.inner.state.borrow().clone()
    }
    pub fn subscribe_commits(&self) -> CommitReceiver {
        self.inner.commits.subscribe()
    }
    pub async fn ready(&self) -> Result<(), StoreError> {
        let mut state = self.inner.state.subscribe();
        loop {
            let current = state.borrow_and_update().clone();
            match current {
                Lifecycle::Ready => return Ok(()),
                Lifecycle::Starting => {}
                Lifecycle::Stopped(Err(error)) => return Err(error),
                _ => return Err(StoreError::Closed),
            }
            state.changed().await.map_err(|_| StoreError::Closed)?;
        }
    }
    /// Does not wait for queue space or execute SQLite on the caller.
    pub fn submit<T: Send + 'static>(&self, job: WriteJob<T>) -> Result<JobHandle<T>, StoreError> {
        job.options.check()?;
        let (reply, receive) = oneshot::channel();
        let queue = self.inner.queue.lock().unwrap_or_else(|p| p.into_inner());
        let queue = queue.as_ref().ok_or(StoreError::Closed)?;
        queue
            .try_send(Box::new(TypedPending { job, reply }))
            .map_err(|error| match error {
                TrySendError::Full(_) => StoreError::Busy,
                TrySendError::Disconnected(_) => StoreError::Closed,
            })?;
        Ok(JobHandle(receive))
    }
    /// Reject new jobs, drain accepted work, close SQLite, release ownership.
    /// Awaiting this never blocks an async worker or calls block_on.
    pub async fn shutdown(&self) -> Result<(), StoreError> {
        self.inner.close();
        let mut state = self.inner.state.subscribe();
        loop {
            if let Lifecycle::Stopped(result) = state.borrow_and_update().clone() {
                return result;
            }
            state.changed().await.map_err(|_| StoreError::Closed)?;
        }
    }
}

pub(crate) fn start(db_id: DbId, options: WriterOptions) -> Result<WriterHandle, StoreError> {
    let (send, receive) = mpsc::sync_channel(QUEUE_CAPACITY);
    let (state, _) = watch::channel(Lifecycle::Starting);
    let (commits, _) = watch::channel(None);
    let generation = DbGeneration(NEXT_GENERATION.fetch_add(1, Ordering::Relaxed));
    let inner = Arc::new(Inner {
        db_id,
        generation,
        queue: Mutex::new(Some(send)),
        state,
        commits,
    });
    let handle = WriterHandle {
        _last: Arc::new(LastHandle(Arc::downgrade(&inner))),
        inner: inner.clone(),
    };
    std::thread::Builder::new()
        .name(format!("ctox-store-{}", generation.0))
        .spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| serve(&inner, receive, options)))
                .unwrap_or(Err(StoreError::Panicked));
            // serve's connection and file guard have dropped before publishing stop.
            inner.close();
            inner.state.send_replace(Lifecycle::Stopped(result));
        })?;
    Ok(handle)
}

struct FileOwner(File);
impl FileOwner {
    fn acquire(db_id: &DbId) -> Result<Self, StoreError> {
        // A separate stable lock inode: never unlink it on shutdown. Explicit
        // unlock also releases a descriptor inherited in a fork/exec window.
        let mut name = db_id.path().file_name().unwrap().to_os_string();
        name.push(".ctox-store.lock");
        let path = db_id.path().with_file_name(name);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => StoreError::AlreadyOwned,
            std::fs::TryLockError::Error(error) => error.into(),
        })?;
        Ok(Self(file))
    }
}
impl Drop for FileOwner {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn serve(
    inner: &Inner,
    receive: Receiver<Box<dyn Pending>>,
    options: WriterOptions,
) -> Result<(), StoreError> {
    let _owner = FileOwner::acquire(&inner.db_id)?;
    // Revalidate cached identities at acquisition, including links created
    // between DbId construction and startup.
    if DbId::new(inner.db_id.path())? != inner.db_id {
        return Err(StoreError::Invalid(
            "database path identity changed before startup".into(),
        ));
    }
    let mut connection = Connection::open(inner.db_id.path())?;
    DbId::new(inner.db_id.path())?;
    // P3-04 replaces this bootstrap with its shared pragma configuration. Keep
    // defaults safe while the skeleton's config module is being implemented.
    connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=0; PRAGMA foreign_keys=ON; PRAGMA journal_size_limit=67108864; PRAGMA wal_autocheckpoint=1000;")?;
    if let Some(configure) = &options.configure {
        configure(&connection)?;
    }
    {
        let queue = inner.queue.lock().unwrap_or_else(|p| p.into_inner());
        if queue.is_some() {
            inner.state.send_replace(Lifecycle::Ready);
        }
    }
    let mut deferred = None;
    while let Some(first) = deferred.take().or_else(|| receive.recv().ok()) {
        let key = first.options().batch_key.clone();
        let mut jobs = vec![first];
        // Only collect already admitted neighbors; never delay a lone write to
        // fill a batch. The time budget is also checked while executing.
        if key.is_some() {
            while jobs.len() < MAX_BATCH {
                match receive.try_recv() {
                    Ok(job) if job.options().batch_key == key => jobs.push(job),
                    Ok(job) => {
                        deferred = Some(job);
                        break;
                    }
                    Err(_) => break,
                }
            }
        }
        execute_batch(inner, &mut connection, &options, jobs);
    }
    connection
        .close()
        .map_err(|(_, error)| StoreError::from(error))?;
    Ok(())
}

fn execute_batch(
    inner: &Inner,
    connection: &mut Connection,
    options: &WriterOptions,
    jobs: Vec<Box<dyn Pending>>,
) {
    let mut jobs = jobs.into_iter().peekable();
    // Split by *execution* time as well as count: a costly job never causes 63
    // more closures to extend the same transaction. Individual closures remain
    // subject to the documented short-job contract (cannot preempt Rust).
    while jobs.peek().is_some() {
        let tx = match connection.transaction_with_behavior(TransactionBehavior::Immediate) {
            Ok(tx) => tx,
            Err(error) => {
                let error = StoreError::from(error);
                for job in jobs {
                    job.reject(error.clone());
                }
                return;
            }
        };
        let started = Instant::now();
        let mut completions = Vec::new();
        let mut checks = Vec::new();
        let mut successful = false;
        let mut failure = None;
        for job in jobs.by_ref() {
            let job_options = job.options().clone();
            if let Err(error) = tx.execute_batch("SAVEPOINT ctox_store_job") {
                job.reject(error.into());
                failure = Some(StoreError::Closed);
                break;
            }
            let outcome = job.run(&tx);
            let finish_sql = if outcome.successful {
                "RELEASE ctox_store_job"
            } else {
                "ROLLBACK TO ctox_store_job; RELEASE ctox_store_job"
            };
            if let Err(error) = tx.execute_batch(finish_sql) {
                failure = Some(error.into());
            }
            successful |= outcome.successful;
            if outcome.successful {
                checks.push(job_options);
            }
            completions.push(outcome.finish);
            if failure.is_some() || started.elapsed() >= BATCH_BUDGET {
                break;
            }
        }
        let result = (|| {
            if let Some(error) = failure {
                return Err(error);
            }
            let head_seq = if successful {
                options
                    .head_seq
                    .as_ref()
                    .map(|hook| hook(&tx))
                    .transpose()?
            } else {
                None
            };
            for check in checks {
                check.check()?;
            }
            tx.commit()?;
            // Commit succeeds before *either* notification or job result.
            if let Some(head_seq) = head_seq {
                inner.commits.send_replace(Some(CommitNotification {
                    db_id: inner.db_id.clone(),
                    generation: inner.generation,
                    head_seq,
                }));
            }
            Ok(())
        })();
        for finish in completions {
            finish(result.as_ref().err().cloned());
        }
    }
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
