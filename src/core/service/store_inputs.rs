//! Borrow the actual daemon owners; this module never opens a file or pool.
//! Runtime engines attach to these handles after their own implementation lands.
use super::store_runtime::Database;
use ctox_store::{
    writer::{CommitReceiver, Lifecycle},
    StoreError,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
};
use tokio::runtime::Handle;

static INPUTS: OnceLock<Mutex<HashMap<PathBuf, Weak<StoreInputs>>>> = OnceLock::new();

pub(crate) struct StoreInputs {
    pub root: PathBuf,
    pub core: Database,
    pub sync: Database,
    pub runtime: Handle,
    closing: Arc<AtomicBool>,
    retired: AtomicBool,
    sync_reads: Arc<tokio::sync::Semaphore>,
}

pub(crate) struct InputRegistration {
    key: PathBuf,
    inputs: Arc<StoreInputs>,
}
impl InputRegistration {
    pub(super) fn new(
        root: &Path,
        core: Database,
        sync: Database,
        runtime: Handle,
        closing: Arc<AtomicBool>,
    ) -> Result<Self, StoreError> {
        let key = crate::paths::runtime_dir(root);
        if core.writer.db_id() == sync.writer.db_id() {
            return Err(StoreError::Invalid(
                "Core and Sync must be distinct registered files".into(),
            ));
        }
        let inputs = Arc::new(StoreInputs {
            root: root.to_owned(),
            core,
            sync,
            runtime,
            closing,
            retired: AtomicBool::new(false),
            sync_reads: Arc::new(tokio::sync::Semaphore::new(64)),
        });
        inputs.check()?;
        let mut registry = INPUTS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if registry.get(&key).and_then(Weak::upgrade).is_some() {
            return Err(StoreError::AlreadyOwned);
        }
        registry.insert(key.clone(), Arc::downgrade(&inputs));
        Ok(Self { key, inputs })
    }
}
impl Drop for InputRegistration {
    fn drop(&mut self) {
        self.inputs.retired.store(true, Ordering::Release);
        let mut registry = INPUTS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if registry
            .get(&self.key)
            .is_some_and(|w| w.ptr_eq(&Arc::downgrade(&self.inputs)))
        {
            registry.remove(&self.key);
        }
    }
}

/// Synchronous, memory-only getter usable from the foreground composition root.
/// A missing or retired registration never creates a replacement owner.
pub(crate) fn store_inputs(root: &Path) -> Result<Arc<StoreInputs>, StoreError> {
    let inputs = INPUTS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&crate::paths::runtime_dir(root))
        .and_then(Weak::upgrade)
        .ok_or(StoreError::Closed)?;
    inputs.check()?;
    Ok(inputs)
}

pub(crate) struct ActivationSnapshot<T> {
    pub value: T,
    pub head_seq: u64,
    /// Feed is attached BEFORE the activation read. Drain commits newer than
    /// head_seq, including a commit made while the WAL snapshot was being read.
    pub commits: CommitReceiver,
}
impl StoreInputs {
    fn check(&self) -> Result<(), StoreError> {
        if self.retired.load(Ordering::Acquire) || self.closing.load(Ordering::Acquire) {
            return Err(StoreError::Closed);
        }
        for db in [&self.core, &self.sync] {
            if db.writer.lifecycle() != Lifecycle::Ready {
                return Err(StoreError::Closed);
            }
            if db.writer.generation().0 != db.readers.generation() {
                return Err(StoreError::Invalid("owner/read generation mismatch".into()));
            }
        }
        Ok(())
    }
    /// Core supplies the actual activation/manifest decoder. No guessed catalog,
    /// actor, policy, fence, or timestamp witness is minted by this accessor.
    pub(crate) async fn activation_snapshot<T: Send + 'static>(
        &self,
        read: impl FnOnce(&rusqlite::Transaction<'_>) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<ActivationSnapshot<T>, StoreError> {
        self.check()?;
        let commits = self.sync.writer.subscribe_commits();
        let (head_seq, value) = self
            .sync
            .readers
            .read(move |tx| {
                let head_seq = tx.query_row("SELECT v FROM _seq WHERE singleton=1", [], |r| {
                    r.get::<_, u64>(0)
                })?;
                let value = read(tx)?;
                Ok((head_seq, value))
            })
            .await?;
        self.check()?;
        Ok(ActivationSnapshot {
            value,
            head_seq,
            commits,
        })
    }
}

#[cfg(all(test, unix))]
#[path = "store_inputs_tests.rs"]
mod tests;

/// Compatibility for existing service OS threads only. Async callers await the
/// owner's read pool directly. The operation is trusted native code, not SQL
/// accepted over the local transport or from a handler.
pub(crate) fn owned_read<T: Send + 'static>(
    root: &Path,
    sync: bool,
    read: impl FnOnce(&rusqlite::Connection) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    anyhow::ensure!(Handle::try_current().is_err(),"owned synchronous read requires a service OS thread; runtime callers must await the read pool");
    anyhow::ensure!(
        !std::thread::current()
            .name()
            .is_some_and(|name| name.starts_with("ctox-store-")),
        "store callbacks must not synchronously wait on another store queue"
    );
    let inputs = store_inputs(root)?;
    let permit = inputs
        .sync_reads
        .clone()
        .try_acquire_owned()
        .map_err(|_| StoreError::Busy)?;
    let database = if sync {
        inputs.sync.clone()
    } else {
        inputs.core.clone()
    };
    let (reply, receive) = tokio::sync::oneshot::channel();
    inputs.runtime.spawn(async move {
        let _permit = permit;
        let result = database
            .readers
            .read_job(ctox_store::ReadJob::new(Default::default(), move |conn| {
                read(conn).map_err(|error| match error.downcast::<rusqlite::Error>() {
                    Ok(error) => error.into(),
                    Err(error) => StoreError::Invalid(error.to_string()),
                })
            }))
            .await;
        let _ = reply.send(result);
    });
    Ok(ctox_store::JobHandle::from_receiver(receive).blocking_wait()?)
}
