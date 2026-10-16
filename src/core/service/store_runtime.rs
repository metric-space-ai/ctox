//! Composition of the Phase-3 store owners. Registration is explicit: an
//! unported legacy file must not acquire a second, nominal v4 owner.
use ctox_store::{
    config::{AdmissionBudget, ConnectionConfig},
    ipc::{DaemonIpc, Dispatcher, JobKind, Request, StoreEndpoint},
    publication,
    read_pool::ReadPool,
    registry::Registry,
    schema,
    writer::{WriterHandle, WriterOptions},
    DbId, JobHandle, JobOptions, ReadJob, StoreError,
};
use rusqlite::Transaction;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tokio::runtime::Handle;

type Initializer = Arc<dyn Fn(&Transaction<'_>) -> Result<(), StoreError> + Send + Sync>;

#[derive(Clone)]
pub(crate) struct Database {
    pub writer: WriterHandle,
    pub readers: ReadPool,
}

/// Time the existing reader boundary without changing admission/execution
/// budgets. The two outer intervals include scheduling/WAL setup and validation;
/// they are deliberately NOT described as SQL CPU or pure queue wait.
/// Labels are native constants; never emit SQL, identifiers or record contents.
pub(super) async fn read_snapshot<T: Send + 'static>(
    database: &Database,
    stage: &'static str,
    options: JobOptions,
    operation: impl FnOnce(&rusqlite::Connection) -> Result<T, StoreError> + Send + 'static,
) -> Result<T, StoreError> {
    let submitted = std::time::Instant::now();
    let timing = Arc::new(Mutex::new(None));
    let observed = timing.clone();
    let result = database
        .readers
        .read_job(ReadJob::new(options, move |tx| {
            let before = submitted.elapsed();
            let entered = std::time::Instant::now();
            let result = operation(tx);
            *observed.lock().unwrap_or_else(|p| p.into_inner()) = Some((before, entered.elapsed()));
            result
        }))
        .await;
    let elapsed = submitted.elapsed();
    if result.is_err() || elapsed >= std::time::Duration::from_millis(20) {
        let measured = *timing.lock().unwrap_or_else(|p| p.into_inner());
        let outcome = match &result {
            Ok(_) => "ok",
            Err(StoreError::Busy) => "busy",
            Err(StoreError::DeadlineExceeded) => "deadline",
            Err(StoreError::Cancelled) => "cancelled",
            Err(StoreError::Closed) => "closed",
            Err(_) => "failed",
        };
        let (before, callback) = measured.unwrap_or_default();
        eprintln!(
            "ctox_owner_read_timing={}",
            serde_json::json!({"stage":stage,"outcome":outcome,"callback_completed":measured.is_some(),"pre_callback_us":measured.map(|m|m.0.as_micros()),"callback_us":measured.map(|m|m.1.as_micros()),"post_callback_us":measured.map(|_|elapsed.saturating_sub(before+callback).as_micros()),"total_us":elapsed.as_micros()})
        );
    }
    result
}

/// Names and paths are resolved by native code, never accepted over IPC.
struct Inspect {
    native_root: Arc<Mutex<Option<std::path::PathBuf>>>,
    databases: Arc<Mutex<HashMap<String, Database>>>,
    runtime: Handle,
    closing: Arc<AtomicBool>,
    queue: Arc<Mutex<Option<Arc<super::queue_store::QueueStore>>>>,
    http: Arc<Mutex<Option<Arc<super::store_http::HttpSurface>>>>,
}
impl Dispatcher for Inspect {
    fn dispatch(
        &self,
        request: Request,
        options: JobOptions,
    ) -> Result<JobHandle<Vec<u8>>, StoreError> {
        if self.closing.load(Ordering::Acquire) {
            return Err(StoreError::Closed);
        }
        if request.method == "http.listen" {
            return self
                .http
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
                .ok_or(StoreError::Closed)?
                .dispatch(request, options);
        }
        if request.kind == JobKind::Read && request.method == "sync.identity" {
            if !request.payload.is_empty() {
                return Err(StoreError::Invalid(
                    "identity request accepts no payload".into(),
                ));
            }
            let root = self
                .native_root
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
                .ok_or(StoreError::Closed)?;
            let (reply, receive) = tokio::sync::oneshot::channel();
            self.runtime.spawn(async move {
                let result = crate::persistence::run_core_sync(move || {
                    let identity = crate::sync_host::signing_identity(&root)?;
                    Ok(serde_json::to_vec(
                        &serde_json::json!({"identity":identity.public_identity()}),
                    )?)
                })
                .await
                .map_err(|_| StoreError::Invalid("native identity unavailable".into()));
                let _ = reply.send(result);
            });
            return Ok(JobHandle::from_receiver(receive));
        }
        if request.method == "queue.ack" {
            let queue = self
                .queue
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
                .ok_or(StoreError::Closed)?;
            return queue.dispatch(request, options);
        }
        if request.kind == JobKind::Read && request.method == "queue.projection.read" {
            let chunk: super::queue_projection::ChunkRequest =
                serde_json::from_slice(&request.payload)
                    .map_err(|_| StoreError::Invalid("invalid projection request".into()))?;
            let core = self
                .databases
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get("core")
                .cloned()
                .ok_or(StoreError::Closed)?;
            let (reply, receive) = tokio::sync::oneshot::channel();
            self.runtime.spawn(async move {
                let result = read_snapshot(&core, "queue.projection.read", options, move |db| {
                    super::queue_projection::read_chunk(db, &chunk)
                })
                .await;
                let _ = reply.send(result);
            });
            return Ok(JobHandle::from_receiver(receive));
        }
        if request.kind != JobKind::Read || request.method != "store.inspect" {
            return Err(StoreError::Invalid("unknown store operation".into()));
        }
        let name: String = serde_json::from_slice(&request.payload)
            .map_err(|_| StoreError::Invalid("invalid store name".into()))?;
        let database = self
            .databases
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&name)
            .cloned()
            .ok_or(StoreError::Closed)?;
        options.check()?;
        let (reply, receive) = tokio::sync::oneshot::channel();
        // Only the existing read pool admits SQLite work. The dispatcher owns
        // no connection and cannot turn a Read request into a writer job.
        self.runtime.spawn(async move {
            let generation = database.writer.generation().0;
            let result = database
                .readers
                .read_job(ReadJob::new(options, move |conn| {
                    let query_only: bool =
                        conn.pragma_query_value(None, "query_only", |r| r.get(0))?;
                    serde_json::to_vec(
                        &serde_json::json!({"generation":generation,"queryOnly":query_only}),
                    )
                    .map_err(|_| StoreError::Invalid("store inspection encoding failed".into()))
                }))
                .await;
            let _ = reply.send(result);
        });
        Ok(JobHandle::from_receiver(receive))
    }
}

pub(crate) struct DaemonStores {
    native_root: Arc<Mutex<Option<std::path::PathBuf>>>,
    registry: Registry,
    databases: Arc<Mutex<HashMap<String, Database>>>,
    ipc: Option<DaemonIpc>,
    closing: Arc<AtomicBool>,
    native_stores: Vec<super::store_rxdb::NativeStoreRegistration>,
    queue: Arc<Mutex<Option<Arc<super::queue_store::QueueStore>>>>,
    runtime: Handle,
    inputs: Option<super::store_inputs::InputRegistration>,
    http: Arc<Mutex<Option<Arc<super::store_http::HttpSurface>>>>,
}
impl DaemonStores {
    /// Acquire process/lifecycle ownership BEFORE opening a database file.
    pub fn start(endpoint: StoreEndpoint, runtime: Handle) -> Result<Self, StoreError> {
        let native_root = Arc::new(Mutex::new(None));
        let databases = Arc::new(Mutex::new(HashMap::new()));
        let closing = Arc::new(AtomicBool::new(false));
        let queue = Arc::new(Mutex::new(None));
        let http = Arc::new(Mutex::new(None));
        let ipc = DaemonIpc::start(
            endpoint,
            Arc::new(Inspect {
                native_root: native_root.clone(),
                databases: databases.clone(),
                runtime: runtime.clone(),
                closing: closing.clone(),
                queue: queue.clone(),
                http: http.clone(),
            }),
        )?;
        Ok(Self {
            native_root,
            registry: Registry,
            databases,
            ipc: Some(ipc),
            closing,
            native_stores: Vec::new(),
            queue,
            runtime,
            inputs: None,
            http,
        })
    }

    pub(crate) fn install_http(
        &self,
        http: Arc<super::store_http::HttpSurface>,
    ) -> Result<(), StoreError> {
        let mut slot = self.http.lock().unwrap_or_else(|p| p.into_inner());
        if slot.is_some() {
            return Err(StoreError::AlreadyOwned);
        }
        *slot = Some(http);
        Ok(())
    }

    /// Called serially by the daemon's composition root. A single pool is kept
    /// alongside each writer; all domain/RxDB adapters receive these clones.
    pub async fn register(
        &mut self,
        name: &str,
        id: DbId,
        sync: bool,
        initialize: Initializer,
    ) -> Result<Database, StoreError> {
        self.register_with_budget(name, id, sync, initialize, AdmissionBudget::default())
            .await
    }

    /// Deliberately-held WAL snapshots test ordering, not production latency.
    /// The production register() entry always retains its 20ms execution budget.
    #[cfg(test)]
    pub(super) async fn register_snapshot_fixture(
        &mut self,
        name: &str,
        id: DbId,
        sync: bool,
        initialize: Initializer,
    ) -> Result<Database, StoreError> {
        self.register_with_budget(
            name,
            id,
            sync,
            initialize,
            AdmissionBudget {
                wait: std::time::Duration::from_secs(3),
                execution: std::time::Duration::from_secs(3),
                ..AdmissionBudget::default()
            },
        )
        .await
    }

    async fn register_with_budget(
        &mut self,
        name: &str,
        id: DbId,
        sync: bool,
        initialize: Initializer,
        read_budget: AdmissionBudget,
    ) -> Result<Database, StoreError> {
        if self.closing.load(Ordering::Acquire) {
            return Err(StoreError::Closed);
        }
        if self
            .databases
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains_key(name)
        {
            return Err(StoreError::AlreadyOwned);
        }
        let config = ConnectionConfig::default();
        let writer = self.registry.open(
            id,
            WriterOptions {
                configure: Some(Arc::new(move |conn| {
                    let tx = rusqlite::Transaction::new_unchecked(
                        conn,
                        rusqlite::TransactionBehavior::Immediate,
                    )?;
                    publication::install(&tx)?;
                    if sync {
                        schema::initialize(&tx)?;
                    }
                    initialize(&tx)?;
                    tx.commit()?;
                    Ok(())
                })),
                head_seq: sync.then(|| {
                    Arc::new(|conn: &rusqlite::Connection| -> Result<u64, StoreError> {
                        Ok(
                            conn.query_row("SELECT v FROM _seq WHERE singleton=1", [], |row| {
                                row.get(0)
                            })?,
                        )
                    }) as ctox_store::writer::HeadSeqHook
                }),
            },
        )?;
        if let Err(error) = writer.ready().await {
            let _ = writer.shutdown().await;
            return Err(error);
        }
        let readers =
            match ReadPool::for_database(writer.db_id(), writer.generation(), config, read_budget)
                .await
            {
                Ok(readers) => readers,
                Err(error) => {
                    let _ = writer.shutdown().await;
                    return Err(error);
                }
            };
        let database = Database { writer, readers };
        self.databases
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(name.to_owned(), database.clone());
        Ok(database)
    }

    /// Publish the existing canonical handles once, only after both owners are ready.
    /// The daemon retains the registration until before admission closes on drain.
    pub async fn publish_inputs(&mut self, root: &std::path::Path) -> Result<(), StoreError> {
        if self.closing.load(Ordering::Acquire) {
            return Err(StoreError::Closed);
        }
        if self.inputs.is_some() {
            return Err(StoreError::AlreadyOwned);
        }
        let checked_root = root.to_owned();
        let (core_id, sync_id) = tokio::task::spawn_blocking(move || {
            Ok::<_, StoreError>((
                DbId::new(crate::paths::core_db(&checked_root))?,
                DbId::new(crate::paths::sync_db(&checked_root))?,
            ))
        })
        .await
        .map_err(|_| StoreError::Panicked)??;
        if self.closing.load(Ordering::Acquire) {
            return Err(StoreError::Closed);
        }
        let databases = self.databases.lock().unwrap_or_else(|p| p.into_inner());
        let core = databases.get("core").cloned().ok_or(StoreError::Closed)?;
        let sync = databases.get("sync").cloned().ok_or(StoreError::Closed)?;
        if core.writer.db_id() != &core_id || sync.writer.db_id() != &sync_id {
            return Err(StoreError::Invalid(
                "composition root/file identity mismatch".into(),
            ));
        }
        self.inputs = Some(super::store_inputs::InputRegistration::new(
            root,
            core,
            sync,
            self.runtime.clone(),
            self.closing.clone(),
            databases.clone(),
        )?);
        *self.native_root.lock().unwrap_or_else(|p| p.into_inner()) = Some(root.to_owned());
        Ok(())
    }

    /// A standalone foreground surface retains the same Core/Secrets owners
    /// without a Sync engine. Its same-user public identity reader still needs
    /// the trusted root; IPC clients never supply a root or key reference.
    pub(crate) fn expose_identity_root(&mut self, root: &std::path::Path) {
        *self.native_root.lock().unwrap_or_else(|p| p.into_inner()) = Some(root.to_owned());
    }

    pub fn retire_inputs(&mut self) {
        self.inputs.take();
    }

    pub async fn install_native_rxdb(&mut self, database: Database) -> Result<(), StoreError> {
        let native =
            super::store_rxdb::RxdbJobs::install_native(database, self.runtime.clone()).await?;
        self.native_stores.push(native);
        Ok(())
    }

    pub async fn install_queue(
        &mut self,
        root: std::path::PathBuf,
        core: Database,
        sync: Database,
    ) -> Result<(), StoreError> {
        if self
            .queue
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some()
        {
            return Err(StoreError::AlreadyOwned);
        }
        let owner = super::queue_store::QueueStore::install(
            root,
            core,
            sync,
            self.runtime.clone(),
            self.closing.clone(),
        )
        .await?;
        *self.queue.lock().unwrap_or_else(|p| p.into_inner()) = Some(owner);
        Ok(())
    }

    /// On a drain error the caller retains this owner and its daemon lease.
    /// Retry after the bounded outstanding job completes; never release the
    /// lifecycle lock and advertise offline maintenance while it is running.
    pub async fn shutdown(&mut self) -> Result<(), StoreError> {
        let http = self.http.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(http) = http {
            http.shutdown().await?;
        }
        // Retire lookup/admission first. Existing jobs are then drained by the
        // pools/writers; retain the daemon lock until all file owners stop.
        self.closing.store(true, Ordering::Release);
        self.native_root
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        self.inputs.take();
        self.native_stores.clear();
        let databases = self
            .databases
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        for database in databases.values() {
            database.writer.stop_admission();
        }
        let queue = self.queue.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(queue) = queue {
            queue.stop();
            queue.drain().await?;
        }
        self.queue.lock().unwrap_or_else(|p| p.into_inner()).take();
        for database in databases.into_values() {
            database
                .readers
                .shutdown()
                .await
                .map_err(StoreError::from)?;
            database.writer.shutdown().await?;
        }
        self.databases
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        if let Some(ipc) = self.ipc.take() {
            ipc.shutdown()?.await?;
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
#[path = "store_runtime_tests.rs"]
mod tests;
