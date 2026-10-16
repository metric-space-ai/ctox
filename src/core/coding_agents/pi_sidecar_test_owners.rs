//! Real file-owner harness for the original Pi regression cases.
use std::path::Path;

pub(in crate::coding_agents) fn run<T: Send + 'static>(
    body: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    // Each body holds one of the process-wide Core admission permits (four,
    // non-waiting) for its whole duration. Under libtest's default parallelism
    // these harness tests would exhaust them and fail with `Busy`, so they take
    // turns here instead of racing the production admission bound.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    crate::persistence::run_core_blocking(runtime.handle(), body)
}

pub(in crate::coding_agents) struct Root {
    directory: tempfile::TempDir,
    handle: tokio::runtime::Handle,
    owners: Option<Owners>,
}
struct Owners {
    runtime: crate::inference::runtime_env::RuntimeOwnerFixture,
    secret: crate::secrets::SecretStore,
    writer: ctox_store::writer::WriterHandle,
    readers: ctox_store::read_pool::ReadPool,
}
impl Root {
    pub(in crate::coding_agents) fn path(&self) -> &Path {
        self.directory.path()
    }
}
pub(in crate::coding_agents) fn root() -> anyhow::Result<Root> {
    let directory = tempfile::tempdir()?;
    let handle = tokio::runtime::Handle::try_current()?;
    let owners = handle.block_on(async {
        let runtime =
            crate::inference::runtime_env::RuntimeOwnerFixture::open(directory.path()).await?;
        let path = crate::secrets::resolve_db_path(directory.path());
        std::fs::create_dir_all(path.parent().unwrap())?;
        let (writer, readers) =
            ctox_store::testkit::StoreFixture::open(ctox_store::DbId::new(&path)?)
                .await?
                .into_parts();
        writer
            .submit(ctox_store::WriteJob::new(Default::default(), |tx| {
                crate::secrets::initialize_owned_secret_schema(tx).map_err(|_| {
                    ctox_store::StoreError::Invalid("isolated secret schema".into())
                })?;
                Ok(())
            }))?
            .await?;
        let secret =
            crate::secrets::SecretStore::attach(directory.path(), writer.clone(), readers.clone())
                .await?;
        Ok::<_, anyhow::Error>(Owners {
            runtime,
            secret,
            writer,
            readers,
        })
    })?;
    Ok(Root {
        directory,
        handle,
        owners: Some(owners),
    })
}
impl Drop for Root {
    fn drop(&mut self) {
        if let Some(owners) = self.owners.take() {
            let result = self.handle.block_on(async move {
                drop(owners.secret);
                owners.readers.shutdown().await?;
                owners.writer.shutdown().await?;
                owners.runtime.close().await?;
                Ok::<_, anyhow::Error>(())
            });
            if !std::thread::panicking() {
                result.expect("isolated Pi owners close");
            }
        }
    }
}
