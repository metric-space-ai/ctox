//! Daemon-owned, durable content transfers. This crate grants no execution authority.
use anyhow::{bail, ensure, Context, Result};
use aria2_rust::{
    http::{self, HttpJob, HttpProgress},
    OptionSet,
};
use fs2::FileExt;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::watch;

#[path = "../../store/harness_adapter.rs"]
mod owner_adapter;

mod peer;
pub use peer::{PeerAccountBinding, PeerRangeSource, PeerReadFailure, PeerSource};
pub mod ssh_exec;
mod storage;
pub mod storage_smb;
pub mod storage_ssh;
pub use storage::{
    validate_relative_path, StorageConnection, StorageDirection, StorageResolver, StorageTransfer,
};

pub const TRANSFER_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS ctox_transfer_jobs (
                id TEXT PRIMARY KEY, request TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('queued','running','paused','cancelled','failed','completed')),
                desired TEXT NOT NULL CHECK(desired IN ('run','pause','cancel')),
                completed_bytes INTEGER NOT NULL DEFAULT 0,
                error_code TEXT, receipt TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at INTEGER NOT NULL DEFAULT (unixepoch())
             );";

pub const ENGINE_REVISION: &str = "8364bcd7902dbd853a0f746c3dc937bbaadec561";

/// Local authorized callers supply immutable content identity, never arbitrary engine options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DownloadRequest {
    pub id: String,
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_source: Option<PeerSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<StorageTransfer>,
    pub sha256: String,
    pub size: u64,
}

impl DownloadRequest {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty()
                && self.id.len() <= 128
                && self
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "invalid transfer id"
        );
        ensure!(
            self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "expected lowercase SHA-256 required"
        );
        ensure!(self.size <= i64::MAX as u64, "content too large");
        if let Some(storage) = &self.storage {
            ensure!(
                self.sources.is_empty() && self.peer_source.is_none(),
                "storage cannot mix with HTTP or peer sources"
            );
            return storage.validate();
        }
        if let Some(peer) = &self.peer_source {
            ensure!(
                self.sources.is_empty(),
                "peer and HTTP sources cannot be mixed"
            );
            return peer.validate();
        }
        ensure!(
            !self.sources.is_empty() && self.sources.len() <= 16,
            "one to sixteen sources required"
        );
        for source in &self.sources {
            ensure!(source.len() <= 8192, "source URL too long");
            let u = url::Url::parse(source)?;
            ensure!(
                matches!(u.scheme(), "http" | "https") && u.host_str().is_some(),
                "HTTP(S) sources only"
            );
            ensure!(
                u.username().is_empty()
                    && u.password().is_none()
                    && u.query().is_none()
                    && u.fragment().is_none(),
                "credential-bearing/query/fragment URLs require a future secret-store resolver"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
    pub transfer_id: String,
    pub sha256: String,
    pub size: u64,
    /// Relative to the daemon's private artifact root, never a caller-selected destination.
    pub artifact: String,
    pub engine_revision: Option<String>,
    #[serde(default)]
    pub transport: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub request: DownloadRequest,
    pub state: String,
    pub completed_bytes: u64,
    pub error_code: Option<String>,
    pub receipt: Option<Receipt>,
}

#[derive(Clone)]
pub struct Store {
    db: PathBuf,
    artifacts: PathBuf,
}

impl Store {
    /// `db` is CTOX's canonical runtime/ctox.sqlite3, not the replicated document store.
    pub fn open(db: impl Into<PathBuf>, artifacts: impl Into<PathBuf>) -> Result<Self> {
        let db = db.into();
        if let Some(parent) = db.parent() {
            fs::create_dir_all(parent)?;
        }
        let artifacts = artifacts.into();
        private_directory(&artifacts)?;
        let artifacts = artifacts.canonicalize()?;
        private_directory(&artifacts.join("staging"))?;
        private_directory(&artifacts.join("objects"))?;
        let store = Self { db, artifacts };
        store.connection()?.execute_batch(TRANSFER_SCHEMA)?;
        Ok(store)
    }

    fn connection(&self) -> Result<TransferConnection> {
        open_transfer_connection(&self.db, false)
    }

    pub fn enqueue(&self, request: DownloadRequest) -> Result<Transfer> {
        request.validate()?;
        let encoded = serde_json::to_string(&request)?;
        let c = self.connection()?;
        c.execute("INSERT INTO ctox_transfer_jobs(id,request,state,desired) VALUES (?1,?2,'queued','run') ON CONFLICT(id) DO NOTHING", params![request.id, encoded])?;
        let saved = self.get(&request.id)?;
        ensure!(
            saved.request == request,
            "transfer id conflicts with immutable request"
        );
        Ok(saved)
    }

    pub fn get(&self, id: &str) -> Result<Transfer> {
        let (request, state, bytes, error, receipt): (String, String, u64, Option<String>, Option<String>) = self.connection()?.query_row(
            "SELECT request,state,completed_bytes,error_code,receipt FROM ctox_transfer_jobs WHERE id=?1", [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        )?;
        Ok(Transfer {
            request: serde_json::from_str(&request)?,
            state,
            completed_bytes: bytes,
            error_code: error,
            receipt: receipt.map(|r| serde_json::from_str(&r)).transpose()?,
        })
    }

    /// Read the exact completed native job from private content storage.
    /// The native caller must revalidate its original grant/account before and
    /// after this synchronous read; this API grants no import/execution rights.
    pub fn read_completed_peer_artifact<T>(
        &self,
        expected: &DownloadRequest,
        consume: impl FnOnce(&mut File) -> Result<T>,
    ) -> Result<T> {
        expected.validate()?;
        let peer = expected
            .peer_source
            .as_ref()
            .context("native peer job required")?;
        ensure!(
            peer.collection == "desktop_files",
            "unsupported checkpoint source"
        );
        peer.account_binding
            .as_ref()
            .context("original account binding required")?
            .validate()?;
        let check = || -> Result<()> {
            let saved = self.get(&expected.id)?;
            ensure!(saved.request == *expected, "transfer request changed");
            ensure!(
                saved.state == "completed" && self.desired(&expected.id)? == "run",
                "transfer is not completed and active"
            );
            ensure!(
                saved.completed_bytes == expected.size,
                "incomplete transfer receipt"
            );
            let receipt = saved.receipt.context("completed receipt missing")?;
            ensure!(
                receipt.transfer_id == expected.id
                    && receipt.sha256 == expected.sha256
                    && receipt.size == expected.size
                    && receipt.artifact == format!("objects/{}", expected.sha256)
                    && receipt.transport == "ctox-webrtc-file-v1"
                    && receipt.engine_revision.is_none(),
                "receipt differs from native job"
            );
            Ok(())
        };
        check()?;
        let path = self.artifacts.join("objects").join(&expected.sha256);
        let before = fs::symlink_metadata(&path)?;
        ensure!(
            before.is_file() && !before.file_type().is_symlink(),
            "artifact is not a regular file"
        );
        let mut file = File::open(path)?;
        let opened = file.metadata()?;
        ensure!(
            opened.is_file() && opened.len() == expected.size,
            "artifact length mismatch"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(
                before.dev() == opened.dev() && before.ino() == opened.ino(),
                "artifact changed during open"
            );
        }
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            size = size
                .checked_add(count as u64)
                .context("artifact length overflow")?;
            ensure!(size <= expected.size, "artifact exceeds expected length");
            hash.update(&buffer[..count]);
        }
        ensure!(
            size == expected.size && format!("{:x}", hash.finalize()) == expected.sha256,
            "artifact content mismatch"
        );
        file.seek(SeekFrom::Start(0))?;
        check()?;
        let result = consume(&mut file)?;
        check()?;
        Ok(result)
    }

    /// Cancellation retains partial bytes and is terminal. Pause is resumable.
    pub fn control(&self, id: &str, action: &str) -> Result<Transfer> {
        let c = self.connection()?;
        match action {
            "cancel" | "pause" => {
                let state = if action == "cancel" {
                    "cancelled"
                } else {
                    "paused"
                };
                c.execute("UPDATE ctox_transfer_jobs SET desired=?2, state=CASE WHEN state='running' THEN state ELSE ?3 END, updated_at=unixepoch() WHERE id=?1 AND desired!='cancel' AND state NOT IN ('completed','cancelled')", params![id, action, state])?;
            }
            "resume" => {
                c.execute("UPDATE ctox_transfer_jobs SET desired='run', state=CASE WHEN state='running' THEN state ELSE 'queued' END,error_code=NULL,updated_at=unixepoch() WHERE id=?1 AND desired!='cancel' AND state IN ('paused','failed','running')", [id])?;
            }
            _ => bail!("expected pause, resume or cancel"),
        }
        self.get(id)
    }

    /// Exclusive OS lease covers recovery, writes and activation; a PID/clock is not a lease.
    pub fn worker(&self) -> Result<Worker> {
        self.worker_with_sources(None, None)
    }

    /// The native host supplies an already authorized peer resolver. Persisted
    /// source claims alone cannot create a connection or select credentials.
    pub fn worker_with_peer(&self, peer: Arc<dyn PeerRangeSource>) -> Result<Worker> {
        self.worker_with_sources(Some(peer), None)
    }

    pub fn worker_with_sources(
        &self,
        peer: Option<Arc<dyn PeerRangeSource>>,
        storage: Option<Arc<dyn StorageResolver>>,
    ) -> Result<Worker> {
        let lease = worker_lease(&self.artifacts)?;
        self.connection()?.execute("UPDATE ctox_transfer_jobs SET state=CASE desired WHEN 'cancel' THEN 'cancelled' WHEN 'pause' THEN 'paused' ELSE 'queued' END, updated_at=unixepoch() WHERE state='running'", [])?;
        Ok(Worker {
            metadata: WorkerMetadata::Legacy(self.clone()),
            artifacts: self.artifacts.clone(),
            owner_stop: None,
            _lease: lease,
            run_gate: tokio::sync::Mutex::new(()),
            poll: std::sync::Mutex::new(None),
            peer,
            storage,
        })
    }

    fn desired(&self, id: &str) -> Result<String> {
        Ok(self.connection()?.query_row(
            "SELECT desired FROM ctox_transfer_jobs WHERE id=?1",
            [id],
            |r| r.get(0),
        )?)
    }
}

// Connection drops before its compatibility lease (field declaration order).
// The shared lease permits existing legacy peers but excludes a live file owner.
struct TransferConnection {
    connection: Connection,
    _legacy_lease: Option<owner_adapter::FileOwner>,
}
impl std::ops::Deref for TransferConnection {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.connection
    }
}
impl std::ops::DerefMut for TransferConnection {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
}

// The owned path calls this only inside its exclusive actor. Compatibility
// connections participate in the same lock protocol, before opening SQLite.
fn open_transfer_connection(path: &Path, owned: bool) -> Result<TransferConnection> {
    let legacy_lease = if owned {
        None
    } else {
        Some(owner_adapter::FileOwner::acquire_legacy_shared(path)?)
    };
    let connection = Connection::open(path)?;
    connection.busy_timeout(if owned {
        Duration::ZERO
    } else {
        Duration::from_secs(5)
    })?;
    if owned {
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA wal_autocheckpoint=1000; PRAGMA journal_size_limit=67108864;")?;
    }
    // Never inherit the Core writer's NORMAL setting for durable receipts.
    connection.pragma_update(None, "synchronous", "FULL")?;
    Ok(TransferConnection {
        connection,
        _legacy_lease: legacy_lease,
    })
}

/// Typed transfer operation for the existing Core file owner. `apply` runs
/// inside its transaction; submitters must reply only after that FULL commit.
pub enum TransferOwnerJob {
    Enqueue(DownloadRequest),
    Get(String),
    Control { id: String, action: String },
}

impl TransferOwnerJob {
    /// No connection is opened here. The Core owner must configure FULL before
    /// BEGIN, initialize TRANSFER_SCHEMA, and commit/rollback after this returns.
    pub fn apply(self, tx: &rusqlite::Transaction<'_>) -> Result<Transfer> {
        let synchronous: i64 = tx.pragma_query_value(None, "synchronous", |row| row.get(0))?;
        ensure!(synchronous == 2, "transfer owner requires FULL durability");
        match self {
            Self::Enqueue(request) => {
                request.validate()?;
                tx.execute("INSERT INTO ctox_transfer_jobs(id,request,state,desired) VALUES (?1,?2,'queued','run') ON CONFLICT(id) DO NOTHING", params![request.id, serde_json::to_string(&request)?])?;
                let saved = read_owned_transfer(tx, &request.id)?;
                ensure!(
                    saved.request == request,
                    "transfer id conflicts with immutable request"
                );
                Ok(saved)
            }
            Self::Get(id) => read_owned_transfer(tx, &id),
            Self::Control { id, action } => {
                match action.as_str() {
                    "cancel" | "pause" => {
                        let state = if action == "cancel" {
                            "cancelled"
                        } else {
                            "paused"
                        };
                        tx.execute("UPDATE ctox_transfer_jobs SET desired=?2, state=CASE WHEN state='running' THEN state ELSE ?3 END, updated_at=unixepoch() WHERE id=?1 AND desired!='cancel' AND state NOT IN ('completed','cancelled')", params![id, action, state])?;
                    }
                    "resume" => {
                        tx.execute("UPDATE ctox_transfer_jobs SET desired='run', state=CASE WHEN state='running' THEN state ELSE 'queued' END,error_code=NULL,updated_at=unixepoch() WHERE id=?1 AND desired!='cancel' AND state IN ('paused','failed','running')", [&id])?;
                    }
                    _ => bail!("expected pause, resume or cancel"),
                }
                read_owned_transfer(tx, &id)
            }
        }
    }
}

/// Worker metadata always executes on the same FULL Core writer as commands.
pub enum TransferWorkerJob {
    Recover,
    HasRunnable,
    Claim,
    Desired(String),
    Checkpoint {
        id: String,
        bytes: u64,
    },
    Settle(String),
    Fail {
        id: String,
        code: String,
    },
    Commit {
        request: DownloadRequest,
        receipt: Receipt,
        stop: Arc<AtomicBool>,
    },
}
pub enum TransferWorkerValue {
    Done,
    Bool(bool),
    Desired(String),
    Claim(Option<Transfer>),
}
pub type TransferWorkerReply =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<TransferWorkerValue>> + Send>>;
impl TransferWorkerJob {
    pub fn apply(self, tx: &rusqlite::Transaction<'_>) -> Result<TransferWorkerValue> {
        let sync: i64 = tx.pragma_query_value(None, "synchronous", |r| r.get(0))?;
        ensure!(sync == 2, "transfer worker requires FULL durability");
        match self {
            Self::Recover => { tx.execute("UPDATE ctox_transfer_jobs SET state=CASE desired WHEN 'cancel' THEN 'cancelled' WHEN 'pause' THEN 'paused' ELSE 'queued' END,updated_at=unixepoch() WHERE state='running'", [])?; }
            Self::HasRunnable => return Ok(TransferWorkerValue::Bool(tx.query_row("SELECT EXISTS(SELECT 1 FROM ctox_transfer_jobs WHERE state='queued' AND desired='run')", [], |r| r.get(0))?)),
            Self::Claim => {
                let id: Option<String> = tx.query_row("SELECT id FROM ctox_transfer_jobs WHERE state='queued' AND desired='run' ORDER BY created_at,id LIMIT 1", [], |r| r.get(0)).optional()?;
                let transfer = if let Some(id) = id { tx.execute("UPDATE ctox_transfer_jobs SET state='running',error_code=NULL,updated_at=unixepoch() WHERE id=?1", [&id])?; Some(read_owned_transfer(tx, &id)?) } else { None };
                return Ok(TransferWorkerValue::Claim(transfer));
            }
            Self::Desired(id) => return Ok(TransferWorkerValue::Desired(tx.query_row("SELECT desired FROM ctox_transfer_jobs WHERE id=?1", [id], |r| r.get(0))?)),
            Self::Checkpoint { id, bytes } => { tx.execute("UPDATE ctox_transfer_jobs SET completed_bytes=?2,updated_at=unixepoch() WHERE id=?1 AND state='running'", params![id, bytes])?; }
            Self::Settle(id) => { tx.execute("UPDATE ctox_transfer_jobs SET state=CASE desired WHEN 'cancel' THEN 'cancelled' WHEN 'pause' THEN 'paused' ELSE 'queued' END,updated_at=unixepoch() WHERE id=?1 AND state='running'", [id])?; }
            Self::Fail { id, code } => { tx.execute("UPDATE ctox_transfer_jobs SET state=CASE desired WHEN 'cancel' THEN 'cancelled' WHEN 'pause' THEN 'paused' ELSE 'failed' END,error_code=?2,updated_at=unixepoch() WHERE id=?1 AND state='running'", params![id,code])?; }
            Self::Commit { request, receipt, stop } => {
                let saved = read_owned_transfer(tx, &request.id)?;
                ensure!(saved.request == request, "transfer request changed before receipt");
                let desired: String = tx.query_row("SELECT desired FROM ctox_transfer_jobs WHERE id=?1", [&request.id], |r| r.get(0))?;
                if desired == "run" && !stop.load(Ordering::Acquire) {
                    tx.execute("UPDATE ctox_transfer_jobs SET state='completed',completed_bytes=?2,receipt=?3,error_code=NULL,updated_at=unixepoch() WHERE id=?1 AND state='running'", params![request.id,request.size,serde_json::to_string(&receipt)?])?;
                } else { tx.execute("UPDATE ctox_transfer_jobs SET state=CASE desired WHEN 'cancel' THEN 'cancelled' WHEN 'pause' THEN 'paused' ELSE 'queued' END,updated_at=unixepoch() WHERE id=?1", [&request.id])?; }
            }
        }
        Ok(TransferWorkerValue::Done)
    }
}

pub type TransferOwnerReply =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Transfer>> + Send>>;

/// Core composition implements this with its existing bounded writer. It must
/// execute `job.apply(tx)` and acknowledge only the FULL commit, never open a
/// second Core connection or wait synchronously on a Tokio worker.
pub trait TransferOwner: Send + Sync {
    fn submit(&self, job: TransferOwnerJob) -> TransferOwnerReply;
    fn worker(&self, _job: TransferWorkerJob) -> TransferWorkerReply {
        Box::pin(async { bail!("Core owner does not provide worker metadata jobs") })
    }
}

#[derive(Clone)]
enum TransferBackend {
    Dedicated(owner_adapter::BoundedOwner<TransferConnection>),
    Shared(Arc<dyn TransferOwner>),
}

/// Async metadata adapter. Use `from_owner` for a Core file already owned by
/// the daemon; `open` is only for a file with no other live owner.
#[derive(Clone)]
pub struct OwnedTransferStore {
    backend: TransferBackend,
}

impl Store {
    /// Consume the compatibility facade after its schema setup. Never retain
    /// a legacy worker alongside this owner on the same Core file.
    pub fn into_owner(self) -> Result<OwnedTransferStore> {
        OwnedTransferStore::open(self.db)
    }
}

impl OwnedTransferStore {
    /// Share the actual Core writer. This entry point never touches a file.
    pub fn from_owner(owner: Arc<dyn TransferOwner>) -> Self {
        Self {
            backend: TransferBackend::Shared(owner),
        }
    }

    /// Metadata-only standalone owner; no SQLite is opened on the caller.
    pub fn open(db: impl Into<PathBuf>) -> Result<Self> {
        let db = db.into();
        if let Some(parent) = db.parent() {
            fs::create_dir_all(parent)?;
        }
        let owner = owner_adapter::BoundedOwner::start(db, |path| {
            let connection = open_transfer_connection(path, true).map_err(std::io::Error::other)?;
            connection
                .execute_batch(TRANSFER_SCHEMA)
                .map_err(std::io::Error::other)?;
            Ok(connection)
        })?;
        Ok(Self {
            backend: TransferBackend::Dedicated(owner),
        })
    }

    async fn execute(&self, job: TransferOwnerJob) -> Result<Transfer> {
        match &self.backend {
            TransferBackend::Shared(owner) => owner.submit(job).await,
            TransferBackend::Dedicated(owner) => Ok(owner
                .call(move |connection| {
                    let tx = connection
                        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                        .map_err(std::io::Error::other)?;
                    let saved = job.apply(&tx).map_err(std::io::Error::other)?;
                    tx.commit().map_err(std::io::Error::other)?;
                    Ok(saved)
                })
                .await?),
        }
    }

    async fn worker_job(&self, job: TransferWorkerJob) -> Result<TransferWorkerValue> {
        match &self.backend {
            TransferBackend::Shared(owner) => owner.worker(job).await,
            TransferBackend::Dedicated(owner) => Ok(owner
                .call(move |connection| {
                    let tx = connection
                        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                        .map_err(std::io::Error::other)?;
                    let value = job.apply(&tx).map_err(std::io::Error::other)?;
                    tx.commit().map_err(std::io::Error::other)?;
                    Ok(value)
                })
                .await?),
        }
    }

    /// Composition supplies its existing Core owner and shared stop flag. No
    /// SQLite handle is created here, including recovery and idle polling.
    pub async fn worker_with_sources(
        &self,
        artifacts: PathBuf,
        peer: Option<Arc<dyn PeerRangeSource>>,
        storage: Option<Arc<dyn StorageResolver>>,
        stop: Arc<AtomicBool>,
    ) -> Result<Worker> {
        private_directory(&artifacts)?;
        let artifacts = artifacts.canonicalize()?;
        private_directory(&artifacts.join("staging"))?;
        private_directory(&artifacts.join("objects"))?;
        let lease = worker_lease(&artifacts)?;
        self.worker_job(TransferWorkerJob::Recover).await?;
        Ok(Worker {
            metadata: WorkerMetadata::Owned(self.clone()),
            artifacts,
            _lease: lease,
            run_gate: tokio::sync::Mutex::new(()),
            poll: std::sync::Mutex::new(None),
            peer,
            storage,
            owner_stop: Some(stop),
        })
    }

    pub async fn enqueue(&self, request: DownloadRequest) -> Result<Transfer> {
        self.execute(TransferOwnerJob::Enqueue(request)).await
    }

    pub async fn get(&self, id: String) -> Result<Transfer> {
        self.execute(TransferOwnerJob::Get(id)).await
    }

    pub async fn control(&self, id: String, action: String) -> Result<Transfer> {
        self.execute(TransferOwnerJob::Control { id, action }).await
    }
}

impl TransferOwner for OwnedTransferStore {
    fn submit(&self, job: TransferOwnerJob) -> TransferOwnerReply {
        let store = self.clone();
        Box::pin(async move { store.execute(job).await })
    }
    fn worker(&self, job: TransferWorkerJob) -> TransferWorkerReply {
        let store = self.clone();
        Box::pin(async move { store.worker_job(job).await })
    }
}

fn read_owned_transfer(connection: &Connection, id: &str) -> Result<Transfer> {
    let (request, state, bytes, error, receipt): (String, String, u64, Option<String>, Option<String>) = connection.query_row(
        "SELECT request,state,completed_bytes,error_code,receipt FROM ctox_transfer_jobs WHERE id=?1", [id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    )?;
    Ok(Transfer {
        request: serde_json::from_str(&request)?,
        state,
        completed_bytes: bytes,
        error_code: error,
        receipt: receipt
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
    })
}

fn worker_lease(artifacts: &Path) -> Result<File> {
    let lease_path = artifacts.join("worker.lock");
    regular_or_absent(&lease_path)?;
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lease_path)?;
    lease
        .try_lock_exclusive()
        .context("transfer worker already owns this store")?;
    Ok(lease)
}

enum WorkerMetadata {
    Legacy(Store),
    Owned(OwnedTransferStore),
}

pub struct Worker {
    metadata: WorkerMetadata,
    artifacts: PathBuf,
    owner_stop: Option<Arc<AtomicBool>>,
    _lease: File,
    run_gate: tokio::sync::Mutex<()>,
    // Idle polling reads through one kept connection: the worker polls every
    // 250 ms, and opening Core each time parsed its whole schema and took the
    // write lock even with an empty queue (thesen 08.10.2026: ~20 % of a core).
    poll: std::sync::Mutex<Option<TransferConnection>>,
    peer: Option<Arc<dyn PeerRangeSource>>,
    storage: Option<Arc<dyn StorageResolver>>,
}

enum SourceOutcome {
    Ready(PathBuf),
    Interrupted,
    Failed,
}

impl Worker {
    async fn desired(&self, id: &str) -> Result<String> {
        match &self.metadata {
            WorkerMetadata::Legacy(store) => store.desired(id),
            WorkerMetadata::Owned(owner) => match owner
                .worker_job(TransferWorkerJob::Desired(id.into()))
                .await?
            {
                TransferWorkerValue::Desired(value) => Ok(value),
                _ => bail!("unexpected owner reply"),
            },
        }
    }
    async fn get(&self, id: &str) -> Result<Transfer> {
        match &self.metadata {
            WorkerMetadata::Legacy(store) => store.get(id),
            WorkerMetadata::Owned(owner) => owner.get(id.into()).await,
        }
    }
    async fn checkpoint(&self, id: &str, bytes: u64) -> Result<()> {
        match &self.metadata {
            WorkerMetadata::Legacy(store) => {
                store.connection()?.execute("UPDATE ctox_transfer_jobs SET completed_bytes=?2,updated_at=unixepoch() WHERE id=?1 AND state='running'", params![id,bytes])?;
            }
            WorkerMetadata::Owned(owner) => {
                owner
                    .worker_job(TransferWorkerJob::Checkpoint {
                        id: id.into(),
                        bytes,
                    })
                    .await?;
            }
        }
        Ok(())
    }
    async fn has_runnable_job(&self) -> Result<bool> {
        match &self.metadata {
            WorkerMetadata::Owned(owner) => {
                match owner.worker_job(TransferWorkerJob::HasRunnable).await? {
                    TransferWorkerValue::Bool(value) => Ok(value),
                    _ => bail!("unexpected owner reply"),
                }
            }
            WorkerMetadata::Legacy(store) => {
                let mut poll = self.poll.lock().unwrap_or_else(|error| error.into_inner());
                if poll.is_none() {
                    *poll = Some(store.connection()?);
                }
                let found = poll.as_ref().expect("poll connection").query_row("SELECT EXISTS(SELECT 1 FROM ctox_transfer_jobs WHERE state='queued' AND desired='run')", [], |row| row.get::<_,bool>(0));
                match found {
                    Ok(found) => Ok(found),
                    Err(error) => {
                        *poll = None;
                        Err(error.into())
                    }
                }
            }
        }
    }
    pub async fn run_next(&self, stop: &AtomicBool) -> Result<bool> {
        // Owned workers use the same flag at every phase, including the commit.
        let stop = self.owner_stop.as_deref().unwrap_or(stop);
        let _run = self.run_gate.lock().await;
        if stop.load(Ordering::Acquire) || !self.has_runnable_job().await? {
            return Ok(false);
        }
        let request = match &self.metadata {
            WorkerMetadata::Owned(owner) => match owner.worker_job(TransferWorkerJob::Claim).await?
            {
                TransferWorkerValue::Claim(Some(transfer)) => transfer.request,
                TransferWorkerValue::Claim(None) => return Ok(false),
                _ => bail!("unexpected owner reply"),
            },
            WorkerMetadata::Legacy(store) => {
                let mut c = store.connection()?;
                let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let id:Option<String> = tx.query_row("SELECT id FROM ctox_transfer_jobs WHERE state='queued' AND desired='run' ORDER BY created_at,id LIMIT 1", [], |r|r.get(0)).optional()?;
                let Some(id) = id else { return Ok(false) };
                tx.execute("UPDATE ctox_transfer_jobs SET state='running',error_code=NULL,updated_at=unixepoch() WHERE id=?1", [&id])?;
                let request = read_owned_transfer(&tx, &id)?.request;
                tx.commit()?;
                request
            }
        };
        if let Err(error) = self.download(&request, stop).await {
            let code = if error.to_string() == "content identity mismatch" {
                "CONTENT_IDENTITY_MISMATCH"
            } else if let Some(failure) = error.downcast_ref::<PeerReadFailure>() {
                failure.code()
            } else {
                "TRANSFER_FAILED"
            };
            match &self.metadata {
                WorkerMetadata::Owned(owner) => {
                    owner
                        .worker_job(TransferWorkerJob::Fail {
                            id: request.id,
                            code: code.into(),
                        })
                        .await?;
                }
                WorkerMetadata::Legacy(store) => {
                    store.connection()?.execute("UPDATE ctox_transfer_jobs SET state=CASE desired WHEN 'cancel' THEN 'cancelled' WHEN 'pause' THEN 'paused' ELSE 'failed' END,error_code=?2,updated_at=unixepoch() WHERE id=?1 AND state='running'",params![request.id,code])?;
                }
            }
        }
        Ok(true)
    }

    async fn download(&self, request: &DownloadRequest, stop: &AtomicBool) -> Result<()> {
        if request.storage.is_some() {
            return self.transfer_storage(request, stop).await;
        }
        let staging = self.artifacts.join("staging").join(&request.id);
        private_directory(&staging)?;
        let object = self.artifacts.join("objects").join(&request.sha256);
        if request.peer_source.is_some() && !self.authorize_peer(request, stop).await? {
            return self.settle_interruption(&request.id).await;
        }
        // Recover publication-before-receipt crashes without redownloading, but never trust existence.
        if object.try_exists()? {
            verify_file(&object, request)?;
            if request.peer_source.is_some() && !self.authorize_peer(request, stop).await? {
                return self.settle_interruption(&request.id).await;
            }
            return self.publish_receipt(request, &object, stop).await;
        }
        if request.peer_source.is_some() {
            let partial = self.download_peer(request, &staging, stop).await?;
            let Some(partial) = partial else {
                return self.settle_interruption(&request.id).await;
            };
            if !self.authorize_peer(request, stop).await? {
                return self.settle_interruption(&request.id).await;
            }
            return self.publish_partial(request, &partial, &object, stop).await;
        }
        // Shared ranges have their own immutable-request staging area. A failed
        // assembly never contaminates the independent single-mirror fallbacks.
        if request.sources.len() > 1 {
            let combined = staging.join("combined");
            private_directory(&combined)?;
            match self
                .download_source(request, &request.sources, &combined, stop)
                .await?
            {
                SourceOutcome::Ready(partial) => {
                    return self.publish_partial(request, &partial, &object, stop).await;
                }
                SourceOutcome::Interrupted => return self.settle_interruption(&request.id).await,
                SourceOutcome::Failed => {}
            }
        }
        for (source_index, source) in request.sources.iter().enumerate() {
            let source_dir = staging.join(format!("source-{source_index}"));
            private_directory(&source_dir)?;
            match self
                .download_source(request, std::slice::from_ref(source), &source_dir, stop)
                .await?
            {
                SourceOutcome::Ready(partial) => {
                    return self.publish_partial(request, &partial, &object, stop).await;
                }
                SourceOutcome::Interrupted => return self.settle_interruption(&request.id).await,
                SourceOutcome::Failed => continue,
            }
        }
        bail!("all sources failed identity or transport validation")
    }

    async fn publish_partial(
        &self,
        request: &DownloadRequest,
        partial: &Path,
        object: &Path,
        stop: &AtomicBool,
    ) -> Result<()> {
        File::open(partial)?.sync_all()?;
        match fs::hard_link(partial, object) {
            Ok(()) => sync_directory(object.parent().unwrap())?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                verify_file(object, request)?
            }
            Err(e) => return Err(e.into()),
        }
        fs::remove_file(partial)?;
        sync_directory(partial.parent().unwrap())?;
        self.publish_receipt(request, object, stop).await
    }

    async fn settle_interruption(&self, id: &str) -> Result<()> {
        match &self.metadata {
            WorkerMetadata::Owned(owner) => {
                owner
                    .worker_job(TransferWorkerJob::Settle(id.into()))
                    .await?;
            }
            WorkerMetadata::Legacy(store) => {
                store.connection()?.execute("UPDATE ctox_transfer_jobs SET state=CASE desired WHEN 'cancel' THEN 'cancelled' WHEN 'pause' THEN 'paused' ELSE 'queued' END,updated_at=unixepoch() WHERE id=?1 AND state='running'", [id])?;
            }
        }
        Ok(())
    }

    async fn download_source(
        &self,
        request: &DownloadRequest,
        sources: &[String],
        staging: &Path,
        stop: &AtomicBool,
    ) -> Result<SourceOutcome> {
        if stop.load(Ordering::Acquire) || self.desired(&request.id).await? != "run" {
            return Ok(SourceOutcome::Interrupted);
        }
        let partial = staging.join("payload");
        regular_or_absent(&partial)?;
        regular_or_absent(&staging.join("payload.aria2"))?;
        // A complete rejected download must never become the next resume prefix.
        if partial.try_exists()? && fs::metadata(&partial)?.len() >= request.size {
            if verify_file(&partial, request).is_ok() {
                return Ok(SourceOutcome::Ready(partial));
            }
            quarantine_partial(staging)?;
        }
        let mut opts = OptionSet::with_defaults();
        let connections = if sources.len() > 1 { "2" } else { "1" };
        for (key, value) in [
            ("file-allocation", "none"),
            ("split", connections),
            ("max-connection-per-server", connections),
            ("min-split-size", "65536"),
            ("uri-selector", "inorder"),
            ("continue", "true"),
            ("always-resume", "false"),
            ("auto-file-renaming", "false"),
            ("allow-overwrite", "true"),
            ("auto-save-interval", "0"),
            ("disk-cache", "0"),
            ("max-tries", "1"),
            ("connect-timeout", "10"),
            ("timeout", "30"),
            ("no-netrc", "true"),
            ("http-accept-gzip", "false"),
            ("use-head", "true"),
            ("out", "payload"),
        ] {
            opts.set(key, value);
        }
        opts.set("ctox-expected-length", request.size.to_string());
        if sources.len() > 1 {
            opts.set("checksum", format!("sha-256={}", request.sha256));
        }
        let progress = HttpProgress::new();
        let (cancel, receive) = watch::channel(false);
        let download = http::download(HttpJob {
            uris: sources.to_vec(),
            dest: partial.clone(),
            opts,
            progress: progress.clone(),
            piece_length: 64 * 1024,
            cancel: receive,
        });
        tokio::pin!(download);
        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        let mut interrupted = false;
        let result = loop {
            tokio::select! {
                result = &mut download => break result,
                _ = ticker.tick() => {
                    if stop.load(Ordering::Acquire) || self.desired(&request.id).await? != "run" {
                        interrupted = true;
                        let _ = cancel.send(true);
                    }
                    let bytes = progress.completed.load(Ordering::Relaxed).min(request.size);
                    self.checkpoint(&request.id, bytes).await?;
                }
            }
        };
        // Do not drop a live engine future or release its lease when cancellation is requested.
        // The pinned engine owns all range futures; its auto-save task is disabled.
        if interrupted || stop.load(Ordering::Acquire) || self.desired(&request.id).await? != "run"
        {
            return Ok(SourceOutcome::Interrupted);
        }
        if result.is_err() {
            if partial.try_exists()?
                && fs::metadata(&partial)?.len() >= request.size
                && verify_file(&partial, request).is_err()
            {
                quarantine_partial(staging)?;
            }
            return Ok(SourceOutcome::Failed);
        }
        if verify_file(&partial, request).is_err() {
            quarantine_partial(staging)?;
            return Ok(SourceOutcome::Failed);
        }
        Ok(SourceOutcome::Ready(partial))
    }

    async fn publish_receipt(
        &self,
        request: &DownloadRequest,
        object: &Path,
        stop: &AtomicBool,
    ) -> Result<()> {
        verify_file(object, request)?;
        File::open(object)?.sync_all()?;
        sync_directory(object.parent().unwrap())?;
        let receipt = Receipt {
            transfer_id: request.id.clone(),
            sha256: request.sha256.clone(),
            size: request.size,
            artifact: format!("objects/{}", request.sha256),
            engine_revision: (request.peer_source.is_none() && request.storage.is_none())
                .then(|| ENGINE_REVISION.into()),
            transport: if request.storage.is_some() {
                "ctox-storage-v1"
            } else if request.peer_source.is_some() {
                "ctox-webrtc-file-v1"
            } else {
                "aria2-http"
            }
            .into(),
        };
        self.commit_receipt(request, receipt, stop).await
    }

    async fn commit_receipt(
        &self,
        request: &DownloadRequest,
        receipt: Receipt,
        stop: &AtomicBool,
    ) -> Result<()> {
        let store = match &self.metadata {
            WorkerMetadata::Owned(owner) => {
                let stop = self
                    .owner_stop
                    .as_ref()
                    .context("owner worker stop flag missing")?
                    .clone();
                owner
                    .worker_job(TransferWorkerJob::Commit {
                        request: request.clone(),
                        receipt,
                        stop,
                    })
                    .await?;
                return Ok(());
            }
            WorkerMetadata::Legacy(store) => store,
        };
        let mut c = store.connection()?;
        let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let desired: String = tx.query_row(
            "SELECT desired FROM ctox_transfer_jobs WHERE id=?1",
            [&request.id],
            |r| r.get(0),
        )?;
        if desired == "run" && !stop.load(Ordering::Acquire) {
            tx.execute("UPDATE ctox_transfer_jobs SET state='completed',completed_bytes=?2,receipt=?3,error_code=NULL,updated_at=unixepoch() WHERE id=?1 AND state='running'", params![request.id, request.size, serde_json::to_string(&receipt)?])?;
        } else {
            tx.execute("UPDATE ctox_transfer_jobs SET state=CASE desired WHEN 'cancel' THEN 'cancelled' WHEN 'pause' THEN 'paused' ELSE 'queued' END,updated_at=unixepoch() WHERE id=?1", [&request.id])?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn quarantine_partial(staging: &Path) -> Result<()> {
    let partial = staging.join("payload");
    // A bounded set of rejected inputs is retained, never silently overwritten.
    // Exhaustion needs operator cleanup rather than unbounded disk growth.
    for index in 0..16 {
        let rejected = staging.join(format!("rejected-{index}"));
        match fs::hard_link(&partial, &rejected) {
            Ok(()) => {
                fs::remove_file(&partial)?;
                let control = staging.join("payload.aria2");
                match fs::remove_file(control) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
                sync_directory(staging)?;
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    bail!("rejected transfer storage requires operator cleanup")
}

fn private_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) => ensure!(
            m.is_dir() && !m.file_type().is_symlink(),
            "artifact path is not a real directory"
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
        }
        Err(e) => return Err(e.into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn regular_or_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) => ensure!(
            m.is_file() && !m.file_type().is_symlink(),
            "artifact path is not a regular file"
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn verify_file(path: &Path, request: &DownloadRequest) -> Result<()> {
    regular_or_absent(path)?;
    let mut f = File::open(path)?;
    ensure!(
        f.metadata()?.len() == request.size,
        "content identity mismatch"
    );
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    ensure!(
        format!("{:x}", hash.finalize()) == request.sha256,
        "content identity mismatch"
    );
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        bail!("durable directory activation is not certified on this platform")
    }
}

/// The daemon owns this guard. UI lifetime has no bearing on the worker or its durable jobs.
fn storage_busy(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(failure, _))
                if matches!(
                    failure.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                )
        )
    })
}

pub struct DaemonWorker {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<()>>>,
}
impl DaemonWorker {
    pub fn start(store: Store) -> Result<Self> {
        Self::start_worker(store.worker()?)
    }
    pub fn start_with_peer(store: Store, peer: Arc<dyn PeerRangeSource>) -> Result<Self> {
        Self::start_worker(store.worker_with_peer(peer)?)
    }
    pub fn start_with_sources(
        store: Store,
        peer: Option<Arc<dyn PeerRangeSource>>,
        storage: Option<Arc<dyn StorageResolver>>,
    ) -> Result<Self> {
        Self::start_worker(store.worker_with_sources(peer, storage)?)
    }
    fn start_worker(worker: Worker) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let thread = std::thread::Builder::new()
            .name("ctox-transfers".into())
            .spawn(move || {
                runtime.block_on(async {
                    let outcome = loop {
                        if stopped.load(Ordering::Acquire) {
                            break Ok(());
                        }
                        match worker.run_next(&stopped).await {
                            Ok(true) => continue,
                            Ok(false) => {}
                            // A busy Core store is contention, not a storage failure: the
                            // worker used to stop with "restart required" in almost every
                            // service process on thesen (08.10.2026).
                            Err(error) if storage_busy(&error) => {}
                            Err(error) => {
                                eprintln!("ctox transfer worker storage failure; restart required");
                                break Err(error);
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    };
                    let cleanup = if let Some(peer) = &worker.peer {
                        match tokio::time::timeout(Duration::from_secs(30), peer.shutdown()).await {
                            Ok(result) => result,
                            Err(error) => Err(error.into()),
                        }
                    } else {
                        Ok(())
                    };
                    if cleanup.is_err() {
                        eprintln!("ctox transfer peer cleanup failed or timed out");
                    }
                    outcome.and(cleanup)
                })
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    pub fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .map_or(true, |thread| thread.is_finished())
    }

    /// Stop admission, drain the owned worker/peer, and surface terminal errors.
    pub fn shutdown(mut self) -> Result<()> {
        self.stop_and_join()
    }

    fn stop_and_join(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("transfer worker panicked"))??;
        }
        Ok(())
    }
}
impl Drop for DaemonWorker {
    fn drop(&mut self) {
        if self.stop_and_join().is_err() {
            eprintln!("ctox transfer worker storage or cleanup failure; restart required");
        }
    }
}

#[cfg(test)]
mod owner_contract_tests {
    use super::*;

    #[tokio::test]
    async fn transfer_owner_full_committed_and_immutable() {
        let directory = tempfile::tempdir().unwrap();
        let native = OwnedTransferStore::open(directory.path().join("core.sqlite")).unwrap();
        let owner = match &native.backend {
            TransferBackend::Dedicated(owner) => owner.clone(),
            _ => unreachable!(),
        };
        // The injected facade must use this already-owned file, not reopen it.
        let store = OwnedTransferStore::from_owner(Arc::new(native));
        let request = DownloadRequest {
            id: "fixture".into(),
            sources: vec!["http://127.0.0.1/fixture".into()],
            peer_source: None,
            storage: None,
            sha256: "0".repeat(64),
            size: 0,
        };
        let saved = store.enqueue(request.clone()).await.unwrap();
        assert_eq!(saved.request, request);
        assert!(
            Store::open(
                directory.path().join("core.sqlite"),
                directory.path().join("artifacts")
            )
            .is_err(),
            "legacy connection must not open an exclusively owned file"
        );
        owner
            .call(|connection| {
                assert_eq!(
                    connection
                        .pragma_query_value(None, "synchronous", |row| row.get::<_, i64>(0))
                        .map_err(std::io::Error::other)?,
                    2
                );
                assert_eq!(
                    connection
                        .pragma_query_value(None, "wal_autocheckpoint", |row| row.get::<_, i64>(0))
                        .map_err(std::io::Error::other)?,
                    1000
                );
                assert!(connection.is_autocommit());
                Ok(())
            })
            .await
            .unwrap();
        let mut changed = request.clone();
        changed.sha256 = "1".repeat(64);
        assert!(store.enqueue(changed).await.is_err());
        assert_eq!(
            store.get(request.id.clone()).await.unwrap().request,
            request
        );
        assert_eq!(
            store
                .control(request.id.clone(), "cancel".into())
                .await
                .unwrap()
                .state,
            "cancelled"
        );
        assert_eq!(
            store
                .control(request.id, "resume".into())
                .await
                .unwrap()
                .state,
            "cancelled"
        );
        let mut closed = owner.closed_signal();
        drop(store);
        drop(owner);
        tokio::time::timeout(Duration::from_secs(5), async {
            while !*closed.borrow_and_update() {
                closed.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        let legacy = Store::open(
            directory.path().join("core.sqlite"),
            directory.path().join("artifacts"),
        )
        .unwrap();
        let connection = legacy.connection().unwrap();
        let duplicate = OwnedTransferStore::open(directory.path().join("core.sqlite")).unwrap();
        assert!(
            duplicate.get("fixture".into()).await.is_err(),
            "exclusive owner must wait for all legacy connections to close"
        );
        drop(duplicate);
        drop(connection);
        assert!(owner_adapter::FileOwner::acquire(&directory.path().join("core.sqlite")).is_ok());
    }
}
