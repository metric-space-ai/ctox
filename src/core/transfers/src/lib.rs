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
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::watch;

mod peer;
pub use peer::{PeerAccountBinding, PeerRangeSource, PeerSource};

pub const ENGINE_REVISION: &str = "8364bcd7902dbd853a0f746c3dc937bbaadec561";

/// Local authorized callers supply immutable content identity, never arbitrary engine options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DownloadRequest {
    pub id: String,
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_source: Option<PeerSource>,
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
        store.connection()?.execute_batch(
            "CREATE TABLE IF NOT EXISTS ctox_transfer_jobs (
                id TEXT PRIMARY KEY, request TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('queued','running','paused','cancelled','failed','completed')),
                desired TEXT NOT NULL CHECK(desired IN ('run','pause','cancel')),
                completed_bytes INTEGER NOT NULL DEFAULT 0,
                error_code TEXT, receipt TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at INTEGER NOT NULL DEFAULT (unixepoch())
             );"
        )?;
        Ok(store)
    }

    fn connection(&self) -> Result<Connection> {
        let c = Connection::open(&self.db)?;
        c.busy_timeout(Duration::from_secs(5))?;
        // FULL also covers a receipt committed after the object and directory were flushed.
        c.pragma_update(None, "synchronous", "FULL")?;
        Ok(c)
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
        self.worker_with_source(None)
    }

    /// The native host supplies an already authorized peer resolver. Persisted
    /// source claims alone cannot create a connection or select credentials.
    pub fn worker_with_peer(&self, peer: Arc<dyn PeerRangeSource>) -> Result<Worker> {
        self.worker_with_source(Some(peer))
    }

    fn worker_with_source(&self, peer: Option<Arc<dyn PeerRangeSource>>) -> Result<Worker> {
        let lease_path = self.artifacts.join("worker.lock");
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
        self.connection()?.execute("UPDATE ctox_transfer_jobs SET state=CASE desired WHEN 'cancel' THEN 'cancelled' WHEN 'pause' THEN 'paused' ELSE 'queued' END, updated_at=unixepoch() WHERE state='running'", [])?;
        Ok(Worker {
            store: self.clone(),
            _lease: lease,
            run_gate: tokio::sync::Mutex::new(()),
            peer,
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

pub struct Worker {
    store: Store,
    _lease: File,
    run_gate: tokio::sync::Mutex<()>,
    peer: Option<Arc<dyn PeerRangeSource>>,
}

enum SourceOutcome {
    Ready(PathBuf),
    Interrupted,
    Failed,
}

impl Worker {
    /// One bounded attempt. Failed jobs require explicit resume; no unbounded retries.
    pub async fn run_next(&self, stop: &AtomicBool) -> Result<bool> {
        let _run = self.run_gate.lock().await;
        if stop.load(Ordering::Acquire) {
            return Ok(false);
        }
        let mut c = self.store.connection()?;
        let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let id: Option<String> = tx.query_row("SELECT id FROM ctox_transfer_jobs WHERE state='queued' AND desired='run' ORDER BY created_at,id LIMIT 1", [], |r| r.get(0)).optional()?;
        let Some(id) = id else {
            return Ok(false);
        };
        tx.execute("UPDATE ctox_transfer_jobs SET state='running',error_code=NULL,updated_at=unixepoch() WHERE id=?1", [&id])?;
        tx.commit()?;
        let request = self.store.get(&id)?.request;
        let result = self.download(&request, stop).await;
        if let Err(error) = result {
            // Raw engine errors may include URLs or remote response material; persist codes only.
            let code = if error.to_string() == "content identity mismatch" {
                "CONTENT_IDENTITY_MISMATCH"
            } else {
                "TRANSFER_FAILED"
            };
            self.store.connection()?.execute("UPDATE ctox_transfer_jobs SET state=CASE desired WHEN 'cancel' THEN 'cancelled' WHEN 'pause' THEN 'paused' ELSE 'failed' END,error_code=?2,updated_at=unixepoch() WHERE id=?1 AND state='running'", params![id, code])?;
        }
        Ok(true)
    }

    async fn download(&self, request: &DownloadRequest, stop: &AtomicBool) -> Result<()> {
        let staging = self.store.artifacts.join("staging").join(&request.id);
        private_directory(&staging)?;
        let object = self.store.artifacts.join("objects").join(&request.sha256);
        if request.peer_source.is_some() && !self.authorize_peer(request, stop).await? {
            return self.settle_interruption(&request.id);
        }
        // Recover publication-before-receipt crashes without redownloading, but never trust existence.
        if object.try_exists()? {
            verify_file(&object, request)?;
            if request.peer_source.is_some() && !self.authorize_peer(request, stop).await? {
                return self.settle_interruption(&request.id);
            }
            return self.publish_receipt(request, &object, stop);
        }
        if request.peer_source.is_some() {
            let partial = self.download_peer(request, &staging, stop).await?;
            let Some(partial) = partial else {
                return self.settle_interruption(&request.id);
            };
            if !self.authorize_peer(request, stop).await? {
                return self.settle_interruption(&request.id);
            }
            return self.publish_partial(request, &partial, &object, stop);
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
                    return self.publish_partial(request, &partial, &object, stop);
                }
                SourceOutcome::Interrupted => return self.settle_interruption(&request.id),
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
                    return self.publish_partial(request, &partial, &object, stop);
                }
                SourceOutcome::Interrupted => return self.settle_interruption(&request.id),
                SourceOutcome::Failed => continue,
            }
        }
        bail!("all sources failed identity or transport validation")
    }

    fn publish_partial(
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
        self.publish_receipt(request, object, stop)
    }

    fn settle_interruption(&self, id: &str) -> Result<()> {
        self.store.connection()?.execute("UPDATE ctox_transfer_jobs SET state=CASE desired WHEN 'cancel' THEN 'cancelled' WHEN 'pause' THEN 'paused' ELSE 'queued' END,updated_at=unixepoch() WHERE id=?1 AND state='running'", [id])?;
        Ok(())
    }

    async fn download_source(
        &self,
        request: &DownloadRequest,
        sources: &[String],
        staging: &Path,
        stop: &AtomicBool,
    ) -> Result<SourceOutcome> {
        if stop.load(Ordering::Acquire) || self.store.desired(&request.id)? != "run" {
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
                    if stop.load(Ordering::Acquire) || self.store.desired(&request.id)? != "run" {
                        interrupted = true;
                        let _ = cancel.send(true);
                    }
                    let bytes = progress.completed.load(Ordering::Relaxed).min(request.size);
                    self.store.connection()?.execute("UPDATE ctox_transfer_jobs SET completed_bytes=?2,updated_at=unixepoch() WHERE id=?1 AND state='running'", params![request.id, bytes])?;
                }
            }
        };
        // Do not drop a live engine future or release its lease when cancellation is requested.
        // The pinned engine owns all range futures; its auto-save task is disabled.
        if interrupted || stop.load(Ordering::Acquire) || self.store.desired(&request.id)? != "run"
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

    fn publish_receipt(
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
            engine_revision: request
                .peer_source
                .is_none()
                .then(|| ENGINE_REVISION.into()),
            transport: if request.peer_source.is_some() {
                "ctox-webrtc-file-v1"
            } else {
                "aria2-http"
            }
            .into(),
        };
        let mut c = self.store.connection()?;
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
pub struct DaemonWorker {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl DaemonWorker {
    pub fn start(store: Store) -> Result<Self> {
        Self::start_worker(store.worker()?)
    }
    pub fn start_with_peer(store: Store, peer: Arc<dyn PeerRangeSource>) -> Result<Self> {
        Self::start_worker(store.worker_with_peer(peer)?)
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
                    while !stopped.load(Ordering::Acquire) {
                        match worker.run_next(&stopped).await {
                            Ok(true) => continue,
                            Ok(false) => {}
                            Err(_) => {
                                eprintln!("ctox transfer worker storage failure; restart required");
                                break;
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                });
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for DaemonWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
