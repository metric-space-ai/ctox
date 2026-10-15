//! The foreground composition root. Legacy V3 background loops are not started:
//! their Connection-returning APIs cannot coexist with an owning v4 writer.
use super::{
    queue_store,
    store_inputs::{store_inputs, StoreInputs},
    store_runtime::DaemonStores,
};
use ctox_store::{DbId, StoreError};
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
};
use tokio::runtime::Handle;

pub(crate) type EngineFuture<T> = Pin<Box<dyn Future<Output = Result<T, StoreError>> + Send>>;
/// Each native subsystem supplies its real implementation after integration.
/// No database, policy, signer, or executor can be supplied by IPC clients.
pub(crate) trait EngineFactory: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn start(&self, inputs: Arc<StoreInputs>) -> EngineFuture<Box<dyn RunningEngine>>;
}
pub(crate) trait RunningEngine: Send + 'static {
    fn shutdown(&mut self) -> EngineFuture<()>;
}
#[derive(Default)]
struct Factories {
    starting: bool,
    factories: Vec<Arc<dyn EngineFactory>>,
}
static FACTORIES: OnceLock<Mutex<Factories>> = OnceLock::new();
/// Called once by native registration code, before run_foreground. Hot-plugged
/// apps use the already bound app engine; they do not register native engines.
pub(crate) fn bind_engine(factory: Arc<dyn EngineFactory>) -> Result<(), StoreError> {
    let mut state = FACTORIES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if state.starting {
        return Err(StoreError::Closed);
    }
    if state.factories.iter().any(|f| f.name() == factory.name()) {
        return Err(StoreError::AlreadyOwned);
    }
    state.factories.push(factory);
    Ok(())
}

pub(crate) struct Foreground {
    stores: DaemonStores,
    pub inputs: Arc<StoreInputs>,
    engines: Vec<Box<dyn RunningEngine>>,
}
impl Foreground {
    /// Real boot operation used by run_foreground and the isolated daemon probe.
    /// All file identity/I/O work happens on the bounded blocking executor.
    pub async fn start(root: PathBuf, runtime: Handle) -> Result<Self, StoreError> {
        let paths = tokio::task::spawn_blocking({
            let root = root.clone();
            move || {
                std::fs::create_dir_all(crate::paths::runtime_dir(&root))?;
                Ok::<_, StoreError>((
                    DbId::new(crate::paths::core_db(&root))?,
                    DbId::new(crate::paths::sync_db(&root))?,
                ))
            }
        })
        .await
        .map_err(|_| StoreError::Panicked)??;
        let endpoint = queue_store::endpoint(&root);
        let mut stores =
            tokio::task::spawn_blocking(move || DaemonStores::start(endpoint, runtime))
                .await
                .map_err(|_| StoreError::Panicked)??;
        let result = async {
            let core = stores
                .register(
                    "core",
                    paths.0,
                    false,
                    Arc::new(|tx| {
                        crate::channels::ensure_owned_schema(tx).map_err(|_| {
                            StoreError::Invalid("Core schema initialization failed".into())
                        })?;
                        Ok(())
                    }),
                )
                .await?;
            let sync = stores
                .register("sync", paths.1, true, Arc::new(|_| Ok(())))
                .await?;
            stores.install_native_rxdb(sync.clone()).await?;
            stores.install_queue(root.clone(), core, sync).await?;
            stores.publish_inputs(&root).await?;
            store_inputs(&root)
        }
        .await;
        let inputs = match result {
            Ok(inputs) => inputs,
            Err(error) => {
                stores.shutdown().await?;
                return Err(error);
            }
        };
        let mut owner = Self {
            stores,
            inputs,
            engines: Vec::new(),
        };
        let factories = {
            let mut state = FACTORIES
                .get_or_init(Mutex::default)
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            state.starting = true;
            state.factories.clone()
        };
        for factory in factories {
            match factory.start(owner.inputs.clone()).await {
                Ok(engine) => owner.engines.push(engine),
                Err(error) => {
                    owner.shutdown().await?;
                    return Err(error);
                }
            }
        }
        Ok(owner)
    }
    /// Stop app/command ingress and owned effects before closing writer admission.
    /// A failed engine drain retains stores, their file locks and IPC custody.
    pub async fn shutdown(&mut self) -> Result<(), StoreError> {
        self.stores.retire_inputs();
        while let Some(engine) = self.engines.last_mut() {
            engine.shutdown().await?;
            self.engines.pop();
        }
        self.stores.shutdown().await?;
        FACTORIES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .starting = false;
        Ok(())
    }
}

pub(crate) async fn run(root: PathBuf) -> anyhow::Result<()> {
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut owner = Foreground::start(root.clone(), Handle::current()).await?;
    // Publish process metadata only after acquiring the daemon/file custody.
    // A rejected second start must not overwrite the running owner's PID.
    let pid_root = root.clone();
    let pid_result = tokio::task::spawn_blocking(move || {
        let path = super::service_loop::service_pid_path(&pid_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        super::service_loop::write_pid_file(&pid_root, std::process::id())
    })
    .await;
    match pid_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            owner.shutdown().await?;
            return Err(error);
        }
        Err(error) => {
            owner.shutdown().await?;
            return Err(error.into());
        }
    }

    eprintln!(
        "ctox v4 store daemon ready: Core/Sync owners and typed store IPC; engine bindings={}",
        owner.engines.len()
    );
    #[cfg(unix)]
    let signal_result =
        tokio::select! { result=tokio::signal::ctrl_c()=>result, _=terminate.recv()=>Ok(()) };
    #[cfg(not(unix))]
    let signal_result = tokio::signal::ctrl_c().await;
    // Retire the PID while the daemon lease is still held; a successor cannot
    // publish its own PID between our identity check and unlink.
    let pid_result = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let path = super::service_loop::service_pid_path(&root);
        if std::fs::read_to_string(&path)
            .ok()
            .is_some_and(|s| s.trim() == std::process::id().to_string())
        {
            std::fs::remove_file(path)?;
        }
        Ok(())
    })
    .await;
    let shutdown_result = owner.shutdown().await;
    pid_result??;
    signal_result?;
    shutdown_result?;
    FACTORIES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .starting = false;
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "store_startup_tests.rs"]
mod tests;
