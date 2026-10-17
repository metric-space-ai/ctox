//! The foreground composition root. Legacy V3 background loops are not started:
//! their Connection-returning APIs cannot coexist with an owning v4 writer.
use super::{
    queue_store,
    store_inputs::{store_inputs, StoreInputs},
    store_runtime::DaemonStores,
};
use ctox_store::{DbId, StoreError};
use std::sync::atomic::{AtomicBool, Ordering};
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
#[cfg(all(test, unix))]
static TEST_ROOTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
#[cfg(all(test, unix))]
pub(crate) struct ForegroundTestScope {
    _lock: tokio::sync::MutexGuard<'static, ()>,
}
#[cfg(all(test, unix))]
pub(crate) async fn isolate_test_roots() -> ForegroundTestScope {
    ForegroundTestScope {
        _lock: TEST_ROOTS.lock().await,
    }
}
#[cfg(all(test, unix))]
impl Drop for ForegroundTestScope {
    fn drop(&mut self) {
        // The production factory registry is process-wide. Contract fixtures
        // isolate and restore it rather than depending on test-filter ordering.
        *FACTORIES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Factories::default();
    }
}
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
    native_owners: Option<NativeOwners>,
}
/// Lifetimes for the real weak-routed legacy adapters. They reuse only the
/// registered file's writer/readers, never an additional connection or pool.
struct NativeOwners {
    _policy: crate::business_os::policy::PolicyAuthorityRegistration,
    _core: Arc<crate::persistence::CorePersistence>,
    _lcm: crate::lcm::owner_adapter::OwnerAdapter,
    _runtime: crate::runtime_env::RuntimeStore,
    _secrets: crate::secrets::SecretStore,
    _instance_identity: Arc<ctox_sync::authority::auth::SigningIdentity>,
    _instance_signer: Arc<dyn ctox_store::change_log::subscription::protocol::InstanceSigner>,
    signer_active: Arc<AtomicBool>,
    #[cfg(unix)]
    session: Option<super::store_native::NativeSessionOwner>,
}
impl NativeOwners {
    async fn attach(inputs: &Arc<StoreInputs>) -> Result<Self, StoreError> {
        let failure = |_| StoreError::Invalid("native owner attachment failed".into());
        let core = crate::persistence::install_core_owner(
            &inputs.root,
            inputs.core.writer.clone(),
            inputs.core.readers.clone(),
        )
        .await
        .map_err(failure)?;
        let lcm = crate::lcm::attach_lcm_store(
            &inputs.root,
            inputs.core.writer.clone(),
            inputs.core.readers.clone(),
        )
        .await?;
        let runtime = inputs.databases.get("runtime").ok_or(StoreError::Closed)?;
        let runtime = crate::runtime_env::RuntimeStore::attach(
            &inputs.root,
            runtime.writer.clone(),
            runtime.readers.clone(),
        )
        .await?;
        let secrets = inputs.databases.get("secrets").ok_or(StoreError::Closed)?;
        let secrets = crate::secrets::SecretStore::attach(
            &inputs.root,
            secrets.writer.clone(),
            secrets.readers.clone(),
        )
        .await?;
        // Legacy imports are explicit maintenance operations, not normal boot:
        // replaying legacy Runtime credentials can overwrite the current issuer.
        // Retain the canonical files here without copying old Core settings.
        let identity = secrets
            .instance_signing_identity()
            .await
            .map_err(|_| StoreError::Invalid("instance signing identity unavailable".into()))?;
        let instance_root = inputs.root.clone();
        let instance_id = tokio::task::spawn_blocking(move || {
            crate::business_os::store::stable_instance_id(&instance_root)
        })
        .await
        .map_err(|_| StoreError::Panicked)?
        .map_err(|_| StoreError::Invalid("canonical instance ID unavailable".into()))?;
        // Required production binding, before any engine/session admission.
        // The same Store generation owns policy and sync publication; failure
        // is fatal, never a silent deny-all startup or an implicit default grant.
        let rights = crate::business_os::policy::sync_rights::StoreRightsAuthority::attach(
            inputs.sync.writer.clone(),
            instance_id.clone(),
        )
        .await
        .map_err(|_| StoreError::Invalid("Store policy authority attachment failed".into()))?;
        let policy = crate::business_os::policy::bind_policy_authority(rights.clone())
            .map_err(|_| StoreError::Invalid("Store policy authority binding failed".into()))?;
        let signer_active = Arc::new(AtomicBool::new(true));
        let signer: Arc<dyn ctox_store::change_log::subscription::protocol::InstanceSigner> =
            Arc::new(super::store_native::InstanceSigning::new(
                identity.clone(),
                instance_id,
                signer_active.clone(),
            ));
        let mut native = Self {
            _policy: policy,
            _core: core,
            _lcm: lcm,
            _runtime: runtime,
            _secrets: secrets,
            _instance_identity: identity.clone(),
            _instance_signer: signer.clone(),
            signer_active: signer_active.clone(),
            #[cfg(unix)]
            session: None,
        };
        inputs.publish_instance_signer(signer.clone())?;
        inputs
            .verifier
            .set(super::store_verifier::CoreVerifier::new(
                inputs,
                rights.clone(),
            )?)
            .map_err(|_| StoreError::AlreadyOwned)?;
        inputs
            .browser
            .set(super::store_browser::BrowserAdmission::new(inputs, rights)?)
            .map_err(|_| StoreError::AlreadyOwned)?;
        inputs
            .browser_ingress
            .set(super::store_browser_ingress::BrowserIngress::new(inputs))
            .map_err(|_| StoreError::AlreadyOwned)?;
        #[cfg(unix)]
        {
            native.session = super::store_native::NativeSessionOwner::start(
                inputs.clone(),
                &native._secrets,
                identity,
                signer,
                signer_active,
            )
            .await?;
        }
        Ok(native)
    }
    async fn shutdown(&mut self) -> Result<(), StoreError> {
        self.signer_active.store(false, Ordering::Release);
        #[cfg(unix)]
        if let Some(session) = self.session.as_mut() {
            session.shutdown().await?;
        }
        Ok(())
    }
}
impl Drop for NativeOwners {
    fn drop(&mut self) {
        self.signer_active.store(false, Ordering::Release);
    }
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
                    DbId::new(crate::paths::runtime_config_db(&root))?,
                    DbId::new(crate::paths::secrets_db(&root))?,
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
            stores.register("runtime", paths.2, false, Arc::new(|tx| {
                tx.execute_batch("CREATE TABLE IF NOT EXISTS runtime_env_kv(env_key TEXT PRIMARY KEY, env_value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS runtime_state_store(state_id INTEGER PRIMARY KEY CHECK(state_id=1),state_json TEXT NOT NULL);")?;
                Ok(())
            })).await?;
            stores.register("secrets", paths.3, false, Arc::new(|tx| {
                crate::secrets::initialize_owned_secret_schema(tx).map_err(|_|StoreError::Invalid("Secrets schema initialization failed".into()))?;
                Ok(())
            })).await?;
            // E34: the canonical BOS identity registry has its own retained owner.
            // No profile email, guessed alias, cross-store ATTACH or second writer.
            let bos_root = root.clone();
            let bos_id = tokio::task::spawn_blocking(move || DbId::new(crate::paths::business_os_db(&bos_root)))
                .await.map_err(|_|StoreError::Panicked)??;
            stores.register("bos", bos_id, false, Arc::new(|_|Ok(()))).await?;
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
            native_owners: None,
        };
        match NativeOwners::attach(&owner.inputs).await {
            Ok(native) => owner.native_owners = Some(native),
            Err(error) => {
                owner.drain().await?;
                return Err(error);
            }
        }
        let http = match super::store_http::HttpSurface::new(owner.inputs.clone()).await {
            Ok(http) => http,
            Err(error) => {
                owner.drain().await?;
                return Err(error);
            }
        };
        owner.stores.install_http(http)?;
        let app_engine =
            crate::business_os::app_platform::runtime_host::attach_native(owner.inputs.clone())
                .await; // APP-PLATFORM-4 attach
        match app_engine {
            Ok(Some(engine)) => owner.engines.push(engine),
            Ok(None) => (),
            Err(error) => {
                owner.shutdown().await?;
                return Err(error);
            }
        }
        // Base/app activation precedes the one-time identity repair; external
        // engine ingress is admitted only after its Sync commit and Core marker.
        let bos = owner
            .inputs
            .databases
            .get("bos")
            .ok_or(StoreError::Closed)?;
        if let Err(error) = ctox_store::import::owner_alias::startup(
            &owner.inputs.core.writer,
            &owner.inputs.sync.writer,
            &bos.readers,
        )
        .await
        {
            owner.shutdown().await?;
            return Err(error);
        }
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
                    owner.drain().await?;
                    return Err(error);
                }
            }
        }
        if let Err(error) = owner
            .inputs
            .browser_ingress
            .get()
            .ok_or(StoreError::Closed)?
            .start_owned(&owner.inputs.runtime)
        {
            owner.shutdown().await?;
            return Err(error);
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
        if let Some(browser) = self.inputs.browser_ingress.get() {
            browser.shutdown().await?;
        }
        if let Some(native) = self.native_owners.as_mut() {
            native.shutdown().await?;
        }
        self.native_owners.take();
        self.stores.shutdown().await?;
        FACTORIES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .starting = false;
        Ok(())
    }
    /// Transfer custody to a runtime-owned cleanup task. A cancelled startup or
    /// signal waiter must not drop a still-running engine and release its files.
    /// Retry drain failures without opening writer admission or dropping owners.
    async fn drain(mut self) -> Result<(), StoreError> {
        tokio::spawn(async move {
            loop {
                let drained = futures_util::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(
                    self.shutdown(),
                ))
                .await;
                match drained {
                    Ok(Ok(())) => return Ok(()),
                    Ok(Err(_)) | Err(_) => {
                        eprintln!("ctox v4 drain pending; retaining engine/store custody");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        })
        .await
        .map_err(|_| StoreError::Panicked)?
    }
    async fn wait_native_stopped(&mut self) -> std::io::Result<()> {
        #[cfg(unix)]
        if let Some(session) = self
            .native_owners
            .as_mut()
            .and_then(|native| native.session.as_mut())
        {
            return session.wait_stopped().await;
        }
        std::future::pending().await
    }
}

pub(crate) async fn run(root: PathBuf) -> anyhow::Result<()> {
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut owner = Foreground::start(root.clone(), Handle::current()).await?;
    // Static assets do not wait for the network. Owner startup already owns
    // bring-up; launch reads readiness and the fatal watcher below drains a
    // failed peer. No HTTP request can initiate or retry signaling.
    // E36: the ordinary daemon owns HTTP too. Read the established typed
    // runtime-store settings through its existing reader, never a CLI opener.
    let http_start = async {
        let runtime_db = owner
            .inputs
            .databases
            .get("runtime")
            .ok_or(StoreError::Closed)?;
        let settings = runtime_db
            .readers
            .read(|tx| {
                use rusqlite::OptionalExtension;
                let read = |key: &str| {
                    tx.query_row(
                        "SELECT env_value FROM runtime_env_kv WHERE env_key=?1",
                        [key],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()
                };
                Ok((
                    read("CTOX_BUSINESS_OS_WEB_AUTOSTART")?,
                    read("CTOX_BUSINESS_OS_WEB_ADDR")?,
                ))
            })
            .await?;
        if !matches!(settings.0.as_deref(), Some("0" | "false" | "off")) {
            let addr = settings.1.unwrap_or_else(|| "127.0.0.1:8765".into());
            let root = root.clone();
            tokio::task::spawn_blocking(move || super::store_http::serve(&root, &addr)).await??;
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Err(error) = http_start {
        owner.drain().await?;
        return Err(error);
    }
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
            owner.drain().await?;
            return Err(error);
        }
        Err(error) => {
            owner.drain().await?;
            return Err(error.into());
        }
    }

    eprintln!(
        "ctox v4 store daemon ready: Core/Sync owners and typed store IPC; engine bindings={}",
        owner.engines.len()
    );
    let browser = owner
        .inputs
        .browser_ingress
        .get()
        .ok_or(StoreError::Closed)?
        .clone();
    #[cfg(unix)]
    let signal_result = tokio::select! {
        result=tokio::signal::ctrl_c()=>result,
        _=terminate.recv()=>Ok(()),
        result=owner.wait_native_stopped()=>result,
        result=browser.wait_failed()=>result,
    };
    #[cfg(not(unix))]
    let signal_result = tokio::select! {
        result=tokio::signal::ctrl_c()=>result,
        result=owner.wait_native_stopped()=>result,
        result=browser.wait_failed()=>result,
    };
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
    let shutdown_result = owner.drain().await;
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
