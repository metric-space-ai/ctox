//! Queue ACKs use owned jobs and a persisted Core -> Sync outbox. System
//! projection authority is the current daemon custody of both registered files;
//! it does not impersonate an account or authorize a caller's queue transition.
use super::store_runtime::Database;
use crate::channels::TerminalPolicyGrant;
use ctox_store::{
    ipc::{self, JobKind, Request, StoreEndpoint},
    publication::{
        self, AuthorityError, AuthorityPermit, CurrentAuthority, Delivery, Publication, Receiver,
        Rejection, RetryPolicy,
    },
    writer::Lifecycle,
    JobHandle, JobOptions, StoreError, WriteJob,
};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
    time::{Duration, Instant},
};
use tokio::{
    runtime::Handle,
    sync::{watch, Notify, RwLock, Semaphore},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AckRequest {
    pub operation_id: String,
    pub keys: Vec<String>,
    pub status: String,
    pub failure: Option<String>,
    pub reason: Option<String>,
    pub attempt: Option<String>,
}
impl AckRequest {
    fn validate(&self) -> Result<(), StoreError> {
        if self.operation_id.is_empty()
            || self.operation_id.len() > 100
            || self.keys.len() > 500
            || self.keys.iter().any(|k| k.is_empty() || k.len() > 150)
        {
            return Err(StoreError::Invalid("invalid queue acknowledgement".into()));
        }
        Ok(())
    }
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
fn options() -> JobOptions {
    JobOptions {
        deadline: Some(Instant::now() + Duration::from_secs(30)),
        ..Default::default()
    }
}
static OWNERS: OnceLock<Mutex<HashMap<PathBuf, Weak<QueueStore>>>> = OnceLock::new();

pub(crate) fn endpoint(root: &Path) -> StoreEndpoint {
    let directory = crate::paths::runtime_dir(root).join("store");
    #[cfg(unix)]
    {
        StoreEndpoint::UnixSocket {
            path: directory.join("store.sock"),
        }
    }
    #[cfg(windows)]
    {
        StoreEndpoint::NamedPipe {
            name: format!(
                r"\\.\pipe\ctox-store-{:x}",
                Sha256::digest(directory.to_string_lossy().as_bytes())
            ),
            directory,
        }
    }
}
/// A synchronous API can wait for an owned result on a service/CLI OS thread.
/// Runtime callers must use ack_async; they cannot enter a blocking SQL proxy.
pub(crate) fn ack(
    root: &Path,
    keys: &[String],
    status: &str,
    failure: Option<&str>,
    reason: Option<&str>,
    attempt: Option<&str>,
    grant: Option<TerminalPolicyGrant>,
) -> anyhow::Result<usize> {
    anyhow::ensure!(
        Handle::try_current().is_err(),
        "queue ACK on runtime requires ack_async"
    );
    let request = AckRequest {
        operation_id: attempt
            .map(|a| format!("queue-ack-{:x}", Sha256::digest(a.as_bytes())))
            .unwrap_or_else(|| format!("queue-ack-{}", uuid::Uuid::new_v4())),
        keys: keys.to_vec(),
        status: status.into(),
        failure: failure.map(str::to_owned),
        reason: reason.map(str::to_owned),
        attempt: attempt.map(str::to_owned),
    };
    request.validate()?;
    let key = ctox_store::DbId::new(crate::paths::core_db(root))?;
    let owner = OWNERS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(key.path())
        .and_then(Weak::upgrade);
    if let Some(owner) = owner {
        return Ok(owner.submit(request, grant, options())?.blocking_wait()?);
    }
    anyhow::ensure!(
        grant.is_none(),
        "trusted queue grants require the daemon-local owner"
    );
    let payload = serde_json::to_vec(&request)?;
    let reply = ipc::request(
        endpoint(root),
        Request {
            request_id: 1,
            kind: JobKind::Write,
            method: "queue.ack".into(),
            payload,
        },
        options(),
    )?
    .blocking_wait()?;
    Ok(serde_json::from_slice(&reply)?)
}

/// Runtime callers await the typed result directly; SQLite remains on its
/// owner thread and IPC stays on its bounded same-user transport pool.
pub(crate) async fn ack_async(
    root: PathBuf,
    request: AckRequest,
    grant: Option<TerminalPolicyGrant>,
) -> anyhow::Result<usize> {
    request.validate()?;
    let path = crate::paths::core_db(&root);
    let key = tokio::task::spawn_blocking(move || ctox_store::DbId::new(path)).await??;
    let owner = OWNERS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(key.path())
        .and_then(Weak::upgrade);
    if let Some(owner) = owner {
        return Ok(owner.submit(request, grant, options())?.await?);
    }
    anyhow::ensure!(
        grant.is_none(),
        "trusted queue grants require daemon-local owner"
    );
    let payload = serde_json::to_vec(&request)?;
    let reply = ipc::request(
        endpoint(&root),
        Request {
            request_id: 1,
            kind: JobKind::Write,
            method: "queue.ack".into(),
            payload,
        },
        options(),
    )?
    .await?;
    Ok(serde_json::from_slice(&reply)?)
}

struct QueueAuthority {
    core: Database,
    sync: Database,
    closing: Arc<AtomicBool>,
    gate: Arc<RwLock<()>>,
}
impl CurrentAuthority for QueueAuthority {
    fn acquire(
        &self,
        tx: &Transaction<'_>,
        publication: &Publication,
    ) -> Result<AuthorityPermit, AuthorityError> {
        let permit = self
            .gate
            .clone()
            .try_read_owned()
            .map_err(|_| AuthorityError::Unavailable)?;
        if self.closing.load(Ordering::Acquire)
            || self.core.writer.lifecycle() != Lifecycle::Ready
            || self.sync.writer.lifecycle() != Lifecycle::Ready
        {
            // Local drain/restart is not a policy revocation. Preserve the
            // durable intent for the next owner instead of dead-lettering it.
            return Err(AuthorityError::Unavailable);
        }
        if publication.target != "sync.queue"
            || publication.kind != "queue.projection.v4"
            || publication.subject != "ctox.daemon.queue"
        {
            return Err(AuthorityError::Rejected(Rejection::Forbidden));
        }
        let body: Value = serde_json::from_slice(&publication.payload)
            .map_err(|_| AuthorityError::Rejected(Rejection::Invalid))?;
        for (field, name) in [
            ("queue", "ctox_queue_tasks"),
            ("command", "business_commands"),
        ] {
            if body.get(field).is_none_or(Value::is_null) {
                continue;
            }
            let schema = ctox_store::schema::active_collection(tx, name)
                .map_err(|_| AuthorityError::Unavailable)?
                .ok_or(AuthorityError::Unavailable)?;
            // These are native platform facts. Changing the registered owner
            // revokes this publisher; application/user actions have their own
            // admission policy before Core changes and are never granted here.
            if schema.definition.app_id != "ctox-platform" {
                return Err(AuthorityError::Rejected(Rejection::Forbidden));
            }
        }
        Ok(AuthorityPermit::holding(permit))
    }
}
fn apply_document(
    tx: &Transaction<'_>,
    collection: &str,
    mut body: Value,
) -> Result<(), StoreError> {
    let schema =
        ctox_store::schema::active_collection(tx, collection)?.ok_or(StoreError::Closed)?;
    let id = body
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| StoreError::Invalid("projection identity missing".into()))?
        .to_owned();
    // The registry validates physical identifiers; no path/table travels in IPC.
    let table = format!("\"{}\"", schema.table.replace('"', "\"\""));
    let previous: Option<(String, Option<String>)> = tx
        .query_row(
            &format!("SELECT rev,data FROM {table} WHERE id=?1"),
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((_, Some(raw))) = &previous {
        let old: Value = serde_json::from_str(raw)
            .map_err(|_| StoreError::Invalid("invalid stored projection".into()))?;
        let version = |v: &Value| {
            v.get("queue_projection_version")
                .and_then(Value::as_i64)
                .unwrap_or(-1)
        };
        if version(&old) >= version(&body) {
            return Ok(());
        }
        // Preserve only the pre-existing client extras with the canonical
        // server-owned field classification, not stale authoritative fields.
        crate::business_os::store::retain_owner_projection_extras(&mut body, &old);
    }
    let height = previous
        .as_ref()
        .and_then(|r| r.0.split_once('-'))
        .and_then(|r| r.0.parse::<u64>().ok())
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| StoreError::Invalid("revision overflow".into()))?;
    let rev = format!("{height}-{}", uuid::Uuid::new_v4());
    tx.execute(&format!("INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,?3,?4,0) ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,schema_ver=excluded.schema_ver,data=excluded.data,deleted=0"),params![id,rev,schema.definition.schema_ver,body.to_string()])?;
    Ok(())
}
fn apply(tx: &Transaction<'_>, publication: &Publication) -> Result<(), StoreError> {
    let body: Value = serde_json::from_slice(&publication.payload)
        .map_err(|_| StoreError::Invalid("invalid queue projection".into()))?;
    let queue = body
        .get("queue")
        .filter(|v| v.is_object())
        .ok_or_else(|| StoreError::Invalid("queue projection missing".into()))?;
    apply_document(tx, "ctox_queue_tasks", queue.clone())?;
    if let Some(command) = body.get("command").filter(|v| !v.is_null()) {
        apply_document(tx, "business_commands", command.clone())?;
    }
    Ok(())
}

pub(crate) struct QueueStore {
    root: PathBuf,
    core: Database,
    runtime: Handle,
    closing: Arc<AtomicBool>,
    gate: Arc<RwLock<()>>,
    wake: Arc<Notify>,
    admission: Arc<Semaphore>,
    driver: Mutex<Option<tokio::task::JoinHandle<()>>>,
    // Local notification AFTER the source outbox receipt commits. Core has no
    // published sequence; consumers read the persisted op_id status on wake.
    publication_progress: watch::Sender<Option<Delivery>>,
}
impl QueueStore {
    pub async fn install(
        root: PathBuf,
        core: Database,
        sync: Database,
        runtime: Handle,
        closing: Arc<AtomicBool>,
    ) -> Result<Arc<Self>, StoreError> {
        if core.writer.db_id() == sync.writer.db_id() {
            return Err(StoreError::Invalid(
                "queue and sync must have distinct owners".into(),
            ));
        }
        let check_root = root.clone();
        let (core_id, sync_id) = tokio::task::spawn_blocking(move || {
            Ok::<_, StoreError>((
                ctox_store::DbId::new(crate::paths::core_db(&check_root))?,
                ctox_store::DbId::new(crate::paths::sync_db(&check_root))?,
            ))
        })
        .await
        .map_err(|_| StoreError::Panicked)??;
        if &core_id != core.writer.db_id() || &sync_id != sync.writer.db_id() {
            return Err(StoreError::Invalid(
                "queue owners belong to another instance".into(),
            ));
        }
        ctox_store::rxdb_schema_adapter::RxdbOwner::attach(
            core.writer.clone(),
            core.readers.clone(),
        )
        .await?;
        ctox_store::rxdb_schema_adapter::RxdbOwner::attach(
            sync.writer.clone(),
            sync.readers.clone(),
        )
        .await?;
        let gate = Arc::new(RwLock::new(()));
        let wake = Arc::new(Notify::new());
        let (publication_progress, _) = watch::channel(None);
        let owner = Arc::new(Self {
            root,
            core: core.clone(),
            runtime: runtime.clone(),
            closing: closing.clone(),
            gate: gate.clone(),
            wake: wake.clone(),
            admission: Arc::new(Semaphore::new(1024)),
            driver: Mutex::new(None),
            publication_progress: publication_progress.clone(),
        });
        {
            let mut owners = OWNERS
                .get_or_init(Mutex::default)
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            if owners
                .get(core.writer.db_id().path())
                .and_then(Weak::upgrade)
                .is_some()
            {
                return Err(StoreError::AlreadyOwned);
            }
            owners.insert(
                core.writer.db_id().path().to_path_buf(),
                Arc::downgrade(&owner),
            );
        }
        // The exclusive new daemon owns A; only its own abandoned publication
        // leases are recoverable immediately. Inbox dedupe still fences B.
        core.writer.submit(WriteJob::new(options(),|tx| {tx.execute_batch("CREATE TABLE IF NOT EXISTS _queue_ack_ops(op_id TEXT PRIMARY KEY,request_json TEXT NOT NULL,result INTEGER NOT NULL)")?;tx.execute("UPDATE _publication_outbox SET state='pending',lease_until_ms=0 WHERE target='sync.queue' AND kind='queue.projection.v4' AND state='inflight'",[])?;Ok(())}))?.await?;
        let authority: Arc<dyn CurrentAuthority> = Arc::new(QueueAuthority {
            core: core.clone(),
            sync: sync.clone(),
            closing: closing.clone(),
            gate,
        });
        let task=runtime.spawn(async move {
            loop {
                if closing.load(Ordering::Acquire){break;}
                loop {
                    let outcome=publication::deliver_one(&core.writer,&sync.writer,"sync.queue".into(),options(),Receiver{authority:authority.clone(),apply:Arc::new(apply)},now(),RetryPolicy::default()).await;
                    match outcome {
                        Ok(Delivery::Idle)=>break,Ok(delivery)=>{publication_progress.send_replace(Some(delivery));},
                        Err(StoreError::Busy)=>{tokio::select!{_=wake.notified()=>{},_=tokio::time::sleep(Duration::from_millis(10))=>{}}},
                        Err(StoreError::Closed) if closing.load(Ordering::Acquire)=>return,
                        Err(_)=>{eprintln!("ctox queue publication stopped: owner job failed");return;}
                    }
                    if closing.load(Ordering::Acquire){return;}
                }
                let due=super::store_runtime::read_snapshot(&core,"queue.outbox.schedule",options(),|conn|Ok(conn.query_row("SELECT MIN(CASE state WHEN 'pending' THEN next_attempt_ms ELSE lease_until_ms END) FROM _publication_outbox WHERE target='sync.queue' AND kind='queue.projection.v4' AND state IN ('pending','inflight')",[],|r|r.get::<_,Option<i64>>(0))?)).await;
                let due=match due {Ok(due)=>due,Err(StoreError::Busy|StoreError::DeadlineExceeded) if !closing.load(Ordering::Acquire)=>{tokio::select!{_=wake.notified()=>{},_=tokio::time::sleep(Duration::from_millis(10))=>{}};continue;},Err(_)=>{if !closing.load(Ordering::Acquire){eprintln!("ctox queue publication stopped: outbox scheduling read failed");}return}};
                if closing.load(Ordering::Acquire){break;}
                match due {
                    Some(due)=>{tokio::select!{_ = wake.notified()=>{},_ = tokio::time::sleep(Duration::from_millis(due.saturating_sub(now()).max(1) as u64))=>{}}},
                    None=>wake.notified().await,
                }
            }
        });
        *owner.driver.lock().unwrap_or_else(|p| p.into_inner()) = Some(task);
        Ok(owner)
    }
    pub fn submit(
        &self,
        request: AckRequest,
        grant: Option<TerminalPolicyGrant>,
        options: JobOptions,
    ) -> Result<JobHandle<usize>, StoreError> {
        request.validate()?;
        if self.closing.load(Ordering::Acquire) {
            return Err(StoreError::Closed);
        }
        if self
            .driver
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .is_none_or(|task| task.is_finished())
        {
            return Err(StoreError::Closed);
        }
        let permit = self
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| StoreError::Busy)?;
        let root = self.root.clone();
        let wake = self.wake.clone();
        let cancellation = options.cancellation.clone();
        let mut pending = self.core.writer.submit(WriteJob::new(options, move |tx| {
            crate::channels::ack_owned_in_transaction(&root, tx, &request, grant)
                .map_err(|e| StoreError::Invalid(format!("queue acknowledgement rejected: {e}")))
        }))?;
        let (mut reply, receive) = tokio::sync::oneshot::channel();
        self.runtime.spawn(async move {
            let _permit = permit;
            let result = tokio::select! {
                result=&mut pending=>result,
                _=reply.closed()=>{cancellation.cancel();pending.await}
            };
            if result.is_ok() {
                wake.notify_one();
            }
            let _ = reply.send(result);
        });
        Ok(JobHandle::from_receiver(receive))
    }
    pub fn dispatch(
        &self,
        request: Request,
        options: JobOptions,
    ) -> Result<JobHandle<Vec<u8>>, StoreError> {
        if request.kind != JobKind::Write || request.method != "queue.ack" {
            return Err(StoreError::Invalid("unknown queue operation".into()));
        }
        let body: AckRequest = serde_json::from_slice(&request.payload)
            .map_err(|_| StoreError::Invalid("invalid queue request".into()))?;
        let pending = self.submit(body, None, options)?;
        let (reply, receive) = tokio::sync::oneshot::channel();
        self.runtime.spawn(async move {
            let result = pending.await.and_then(|n| {
                serde_json::to_vec(&n)
                    .map_err(|_| StoreError::Invalid("queue response encoding".into()))
            });
            let _ = reply.send(result);
        });
        Ok(JobHandle::from_receiver(receive))
    }
    pub fn stop(&self) {
        self.closing.store(true, Ordering::Release);
        self.wake.notify_one();
    }
    pub async fn drain(&self) -> Result<(), StoreError> {
        let task = self.driver.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(task) = task {
            task.await.map_err(|_| StoreError::Panicked)?;
        }
        let _fence = self.gate.write().await;
        Ok(())
    }
}
impl Drop for QueueStore {
    fn drop(&mut self) {
        if let Some(task) = self.driver.lock().unwrap_or_else(|p| p.into_inner()).take() {
            task.abort();
        }
        let mut owners = OWNERS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if owners
            .get(self.core.writer.db_id().path())
            .is_some_and(|w| w.as_ptr() == self as *const _)
        {
            owners.remove(self.core.writer.db_id().path());
        }
    }
}

#[cfg(all(test, unix))]
#[path = "queue_store_tests.rs"]
mod tests;
