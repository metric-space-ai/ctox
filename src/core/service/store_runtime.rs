//! Composition of the Phase-3 store owners. Registration is explicit: an
//! unported legacy file must not acquire a second, nominal v4 owner.
use ctox_store::{
    change_log,
    config::{AdmissionBudget, ConnectionConfig},
    ipc::{DaemonIpc, Dispatcher, JobKind, Request, StoreEndpoint},
    publication,
    read_pool::ReadPool,
    registry::Registry,
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

/// Names and paths are resolved by native code, never accepted over IPC.
struct Inspect {
    databases: Arc<Mutex<HashMap<String, Database>>>,
    runtime: Handle,
    closing: Arc<AtomicBool>,
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
    registry: Registry,
    databases: Arc<Mutex<HashMap<String, Database>>>,
    ipc: Option<DaemonIpc>,
    closing: Arc<AtomicBool>,
}
impl DaemonStores {
    /// Acquire process/lifecycle ownership BEFORE opening a database file.
    pub fn start(endpoint: StoreEndpoint, runtime: Handle) -> Result<Self, StoreError> {
        let databases = Arc::new(Mutex::new(HashMap::new()));
        let closing = Arc::new(AtomicBool::new(false));
        let ipc = DaemonIpc::start(
            endpoint,
            Arc::new(Inspect {
                databases: databases.clone(),
                runtime,
                closing: closing.clone(),
            }),
        )?;
        Ok(Self {
            registry: Registry,
            databases,
            ipc: Some(ipc),
            closing,
        })
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
                        change_log::initialize(&tx)?;
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
        let readers = match ReadPool::for_database(
            writer.db_id(),
            writer.generation(),
            config,
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
        let database = Database { writer, readers };
        self.databases
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(name.to_owned(), database.clone());
        Ok(database)
    }

    /// On a drain error the caller retains this owner and its daemon lease.
    /// Retry after the bounded outstanding job completes; never release the
    /// lifecycle lock and advertise offline maintenance while it is running.
    pub async fn shutdown(&mut self) -> Result<(), StoreError> {
        // Retire lookup/admission first. Existing jobs are then drained by the
        // pools/writers; retain the daemon lock until all file owners stop.
        self.closing.store(true, Ordering::Release);
        let databases = self
            .databases
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        for database in databases.values() {
            database.writer.stop_admission();
        }
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
