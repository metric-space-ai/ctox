// Origin: CTOX
// License: AGPL-3.0-only
//! Shared real-owner fixture. Never grants admission independently of the
//! production entry, and retains the directory through owner shutdown.
use super::*;
use std::sync::Arc;

fn isolated_root() -> Result<Arc<tempfile::TempDir>> {
    let root = Arc::new(tempfile::tempdir()?);
    anyhow::ensure!(
        [
            crate::paths::core_db(root.path()),
            crate::paths::runtime_config_db(root.path()),
            crate::paths::secrets_db(root.path())
        ]
        .iter()
        .all(|path| path.starts_with(root.path())),
        "owner test fixture requires isolated state paths"
    );
    Ok(root)
}

pub(crate) fn run<T: Send + 'static>(
    operation: impl FnOnce(Arc<tempfile::TempDir>) -> Result<T> + Send + 'static,
) -> Result<T> {
    let root = isolated_root()?;
    let work = root.clone();
    run_cli_owned(root.path(), move || operation(work))
}

/// Prepare an offline legacy fixture before any owner opens it. Setup may not
/// escape its root; the operation always uses production owner admission.
pub(crate) fn run_prepared<T: Send + 'static>(
    setup: impl FnOnce(&Path) -> Result<()>,
    operation: impl FnOnce(Arc<tempfile::TempDir>) -> Result<T> + Send + 'static,
) -> Result<T> {
    let root = isolated_root()?;
    std::fs::create_dir_all(crate::paths::runtime_dir(root.path()))?;
    setup(root.path())?;
    let work = root.clone();
    run_cli_owned(root.path(), move || operation(work))
}

/// Runtime-only contracts must be able to assert that no Secrets file was
/// created. Start only the actual Core and Runtime file owners for this scope.
pub(crate) fn run_runtime<T: Send + 'static>(
    operation: impl FnOnce(Arc<tempfile::TempDir>) -> Result<T> + Send + 'static,
) -> Result<T> {
    let root = isolated_root()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(8)
        .enable_all()
        .build()?;
    let mut stores = crate::service::store_runtime::DaemonStores::start(
        crate::service::queue_store::endpoint(root.path()),
        runtime.handle().clone(),
    )?;
    let owners = runtime.block_on(async {
        let core = stores.register("core", ctox_store::DbId::new(crate::paths::core_db(root.path()))?, false,
            Arc::new(|_| Ok(()))).await?;
        let core_owner = install_core_owner(root.path(), core.writer.clone(), core.readers.clone()).await?;
        let lcm = crate::lcm::attach_lcm_store(root.path(), core.writer, core.readers).await?;
        let config = stores.register("runtime", ctox_store::DbId::new(crate::paths::runtime_config_db(root.path()))?, false,
            Arc::new(|tx| { tx.execute_batch("CREATE TABLE IF NOT EXISTS runtime_env_kv(env_key TEXT PRIMARY KEY,env_value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS runtime_state_store(state_id INTEGER PRIMARY KEY CHECK(state_id=1),state_json TEXT NOT NULL);")?; Ok(()) })).await?;
        let runtime_owner = crate::runtime_env::RuntimeStore::attach(root.path(), config.writer, config.readers).await?;
        Ok::<_, anyhow::Error>((core_owner, lcm, runtime_owner))
    });
    let result = match owners {
        Ok(owners) => {
            let work = root.clone();
            let result = run_core_blocking_at(root.path(), move || operation(work));
            drop(owners);
            result
        }
        Err(error) => Err(error),
    };
    runtime.block_on(stores.shutdown())?;
    result
}

/// Poll test futures on the admitted blocking worker, not a Tokio async
/// worker: legacy synchronous phases may block, while spawned I/O still uses
/// the handed-out multi-thread runtime. A stalled future has a fixed deadline.
pub(crate) fn run_async<T, F>(
    operation: impl FnOnce(Arc<tempfile::TempDir>) -> F + Send + 'static,
) -> Result<T>
where
    T: Send + 'static,
    F: std::future::Future<Output = Result<T>>,
{
    run(move |root| {
        struct Wake(std::thread::Thread);
        impl std::task::Wake for Wake {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
            fn wake_by_ref(self: &Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = std::task::Waker::from(Arc::new(Wake(std::thread::current())));
        let mut context = std::task::Context::from_waker(&waker);
        let mut future = Box::pin(operation(root));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "owner test future exceeded 120 seconds"
            );
            std::thread::park_timeout(std::time::Duration::from_millis(50));
        }
    })
}

#[test]
fn shared_fixture_keeps_admission_scoped_and_drives_async_io() -> Result<()> {
    assert_eq!(
        require_sync_owner_context()
            .unwrap_err()
            .downcast_ref::<CoreSyncError>(),
        Some(&CoreSyncError::Required)
    );
    run_async(|root| async move {
        require_sync_owner_context()?;
        crate::runtime_env::set_runtime_env_value(root.path(), "OWNER_FIXTURE", "retained")?;
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        assert_eq!(
            crate::runtime_env::load_runtime_env_map(root.path())?
                .get("OWNER_FIXTURE")
                .map(String::as_str),
            Some("retained")
        );
        Ok(())
    })?;
    run_runtime(|root| {
        crate::runtime_env::set_runtime_env_value(root.path(), "OWNER_RUNTIME_ONLY", "retained")?;
        assert!(!crate::secrets::secret_store_path(root.path()).exists());
        Ok(())
    })?;
    assert_eq!(
        require_sync_owner_context()
            .unwrap_err()
            .downcast_ref::<CoreSyncError>(),
        Some(&CoreSyncError::Required)
    );
    Ok(())
}
