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
    run(move |root| poll_fixture(operation(root)))
}

fn poll_fixture<T, F: std::future::Future<Output = Result<T>>>(future: F) -> Result<T> {
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
    let mut future = Box::pin(future);
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

/// Use the real Store rights authority for policy-gated native commands. Grants
/// are the base release's durable rows; no role fallback or fixture allow-list.
pub(crate) fn with_policy<T>(root: &Path, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    require_sync_owner_context()?;
    injected_core(root)?;
    use crate::business_os::policy::{self, sync_rights::StoreRightsAuthority};
    let writer = ctox_store::registry::Registry.open(
        ctox_store::DbId::new(root.join("owner-test-rights.sqlite3"))?,
        ctox_store::writer::WriterOptions {
            configure: Some(Arc::new(|db| {
                ctox_store::config::ConnectionConfig::default().apply_writer(db)?;
                Ok(())
            })),
            head_seq: Some(Arc::new(|db| {
                db.query_row("SELECT v FROM _seq WHERE singleton=1", [], |r| r.get(0))
                    .map_err(Into::into)
            })),
        },
    )?;
    let runtime = tokio::runtime::Handle::current();
    let registration = runtime.block_on(async {
        let authority = StoreRightsAuthority::attach(writer.clone(), "owner-test-fixture".into())
            .await
            .map_err(anyhow::Error::msg)?;
        authority
            .replace(policy::base_release_rights(&[]), vec![])
            .await
            .map_err(anyhow::Error::msg)?;
        policy::bind_policy_authority(authority).map_err(anyhow::Error::msg)
    });
    let result = match registration {
        Ok(registration) => {
            let result = operation();
            drop(registration);
            result
        }
        Err(error) => Err(error),
    };
    runtime.block_on(writer.shutdown())?;
    result
}

pub(crate) fn run_policy<T: Send + 'static>(
    operation: impl FnOnce(Arc<tempfile::TempDir>) -> Result<T> + Send + 'static,
) -> Result<T> {
    run(move |root| {
        let work = root.clone();
        with_policy(root.path(), move || operation(work))
    })
}

// Nested fixture factories keep returning independent roots. Each root gets
// real file owners on the handed-out runtime, retained until the entire test
// finishes; a second fixture never aliases the first fixture's database.
type FixtureCleanup = Box<dyn FnOnce() -> Result<()>>;
thread_local! {
    static FIXTURE_CLEANUP: std::cell::RefCell<Option<Vec<(std::path::PathBuf, FixtureCleanup)>>> = const { std::cell::RefCell::new(None) };
}

struct FixtureScope;
impl FixtureScope {
    fn start() -> Self {
        FIXTURE_CLEANUP.with(|scope| {
            assert!(scope.borrow().is_none(), "nested owner fixture scope");
            *scope.borrow_mut() = Some(Vec::new());
        });
        Self
    }
    fn finish(&self) -> Result<()> {
        let entries = FIXTURE_CLEANUP.with(|scope| scope.borrow_mut().take());
        let mut error = None;
        for (_, close) in entries.unwrap_or_default().into_iter().rev() {
            if let Err(failure) = close() {
                error = Some(failure);
            }
        }
        error.map_or(Ok(()), Err)
    }
}
impl Drop for FixtureScope {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("owner fixture shutdown failed: {error:#}");
        }
    }
}

pub(crate) fn run_scoped<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    run(move |root| {
        with_policy(root.path(), move || {
            let scope = FixtureScope::start();
            let result = operation();
            scope.finish()?;
            result
        })
    })
}

pub(crate) fn run_scoped_async<T, F>(operation: impl FnOnce() -> F + Send + 'static) -> Result<T>
where
    T: Send + 'static,
    F: std::future::Future<Output = Result<T>>,
{
    run_scoped(move || poll_fixture(operation()))
}

/// A fresh root for a legacy nested fixture factory. No ambient root, direct
/// SQLite opener or independent admission flag is introduced here.
pub(crate) fn scoped_tempdir() -> Result<Arc<tempfile::TempDir>> {
    require_sync_owner_context()?;
    anyhow::ensure!(
        FIXTURE_CLEANUP.with(|scope| scope.borrow().is_some()),
        "fixture requires run_scoped"
    );
    retain_scoped_root(isolated_root()?)
}

pub(crate) fn retain_scoped_root(root: Arc<tempfile::TempDir>) -> Result<Arc<tempfile::TempDir>> {
    require_sync_owner_context()?;
    anyhow::ensure!(
        FIXTURE_CLEANUP.with(|scope| scope.borrow().is_some()),
        "fixture requires run_scoped"
    );
    let runtime = tokio::runtime::Handle::current();
    let mut stores = crate::service::store_runtime::DaemonStores::start(
        crate::service::queue_store::endpoint(root.path()),
        runtime.clone(),
    )?;
    let owners = runtime.block_on(async {
        let core = stores.register("core", ctox_store::DbId::new(crate::paths::core_db(root.path()))?, false, Arc::new(|_| Ok(()))).await?;
        let core_owner = install_core_owner(root.path(), core.writer.clone(), core.readers.clone()).await?;
        let lcm = crate::lcm::attach_lcm_store(root.path(), core.writer, core.readers).await?;
        let config = stores.register("runtime", ctox_store::DbId::new(crate::paths::runtime_config_db(root.path()))?, false,
            Arc::new(|tx| { tx.execute_batch("CREATE TABLE IF NOT EXISTS runtime_env_kv(env_key TEXT PRIMARY KEY,env_value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS runtime_state_store(state_id INTEGER PRIMARY KEY CHECK(state_id=1),state_json TEXT NOT NULL);")?; Ok(()) })).await?;
        let runtime_owner = crate::runtime_env::RuntimeStore::attach(root.path(), config.writer, config.readers).await?;
        let secrets = stores.register("secrets", ctox_store::DbId::new(crate::paths::secrets_db(root.path()))?, false,
            Arc::new(|tx| {crate::secrets::initialize_owned_secret_schema(tx).map_err(|_|ctox_store::StoreError::Invalid("fixture secret schema initialization failed".into()))?; Ok(())})).await?;
        let secrets_owner = crate::secrets::SecretStore::attach(root.path(), secrets.writer, secrets.readers).await?;
        Ok::<_, anyhow::Error>((core_owner, lcm, runtime_owner, secrets_owner))
    });
    match owners {
        Ok(owners) => {
            let retained_root = root.clone();
            FIXTURE_CLEANUP.with(|scope| {
                scope.borrow_mut().as_mut().unwrap().push((
                    root.path().to_owned(),
                    Box::new(move || {
                        drop(owners);
                        let result = runtime.block_on(stores.shutdown()).map_err(Into::into);
                        drop(retained_root);
                        result
                    }),
                ))
            });
            Ok(root)
        }
        Err(error) => {
            runtime.block_on(stores.shutdown())?;
            Err(error)
        }
    }
}

/// Retain the real queue owner and publication outbox for routing/ACK tests.
pub(crate) fn with_queue<T>(root: &Path, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    require_sync_owner_context()?;
    let core = injected_core(root)?;
    let runtime = tokio::runtime::Handle::current();
    let mut stores = crate::service::store_runtime::DaemonStores::start(
        crate::service::queue_store::endpoint(&root.join("queue-fixture")),
        runtime.clone(),
    )?;
    let initialized = runtime.block_on(async {
        let sync = stores
            .register(
                "sync",
                ctox_store::DbId::new(crate::paths::sync_db(root))?,
                true,
                Arc::new(|_| Ok(())),
            )
            .await?;
        stores
            .install_queue(
                root.to_owned(),
                crate::service::store_runtime::Database {
                    writer: core.writer.clone(),
                    readers: core.readers.clone(),
                },
                sync,
            )
            .await?;
        Ok::<_, anyhow::Error>(())
    });
    let result = initialized.and_then(|_| operation());
    runtime.block_on(stores.shutdown())?;
    result
}

pub(crate) fn run_queue<T: Send + 'static>(
    operation: impl FnOnce(Arc<tempfile::TempDir>) -> Result<T> + Send + 'static,
) -> Result<T> {
    run(move |root| {
        let work = root.clone();
        with_queue(root.path(), move || operation(work))
    })
}

#[test]
fn nested_fixture_roots_remain_independent_and_retained() -> Result<()> {
    run_scoped(|| {
        let first = scoped_tempdir()?;
        let second = scoped_tempdir()?;
        assert_ne!(first.path(), second.path());
        store_text_value(first.path(), "owner-root", Some("first"))?;
        store_text_value(second.path(), "owner-root", Some("second"))?;
        assert_eq!(
            load_text_value(first.path(), "owner-root")?.as_deref(),
            Some("first")
        );
        assert_eq!(
            load_text_value(second.path(), "owner-root")?.as_deref(),
            Some("second")
        );
        crate::runtime_env::set_runtime_env_value(first.path(), "OWNER_ROOT", "first")?;
        crate::runtime_env::set_runtime_env_value(second.path(), "OWNER_ROOT", "second")?;
        assert_eq!(
            crate::runtime_env::load_runtime_env_map(first.path())?
                .get("OWNER_ROOT")
                .map(String::as_str),
            Some("first")
        );
        assert_eq!(
            crate::runtime_env::load_runtime_env_map(second.path())?
                .get("OWNER_ROOT")
                .map(String::as_str),
            Some("second")
        );
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

/// Process-restart tests explicitly retire and drain their old file owner.
pub(crate) fn release_scoped_root(root: &Path) -> Result<()> {
    let close = FIXTURE_CLEANUP.with(|scope| {
        let mut scope = scope.borrow_mut();
        let entries = scope.as_mut().context("fixture requires run_scoped")?;
        let index = entries
            .iter()
            .position(|(path, _)| path == root)
            .context("fixture root not retained")?;
        Ok::<_, anyhow::Error>(entries.remove(index).1)
    })?;
    close()
}
