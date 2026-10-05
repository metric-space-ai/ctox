//! Foreground native execution-host lifecycle shared by service and CLI adapters.
//! The caller owns credentials, runtime configuration and its database.
use crate::{
    authority::{auth::SigningIdentity, client::ExecutionAuthority},
    host_config::{HostConfiguration, HostMember},
    native::{NativePeerRole, NativeSyncOptions, NativeSyncSession},
};
use std::{
    future::Future,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Clone)]
pub struct HostStarted {
    /// Actual private endpoint from the live listener, never an invitation field.
    pub ipc_endpoint: PathBuf,
    pub node_id: u64,
    pub scope_id: String,
}

/// Starts one existing native attachment and awaits its supervised shutdown.
/// `started` means a local listener exists, not membership or quorum readiness.
/// Hosts keep their exclusive process lease and database alive until this returns.
pub async fn run<F, S>(
    config: &HostConfiguration,
    root: &Path,
    ipc_directory: &Path,
    key: Arc<SigningIdentity>,
    options: NativeSyncOptions,
    stop: S,
    started: F,
) -> io::Result<()>
where
    F: FnOnce(HostStarted) -> io::Result<()>,
    S: Future<Output = io::Result<()>>,
{
    run_with_authority(
        config,
        root,
        ipc_directory,
        key,
        options,
        stop,
        |ready, _| started(ready),
    )
    .await
}

/// Lends the exact already-running authority to a native daemon owner. Retained
/// handles still fail closed on host shutdown; no second store/node is opened.
pub async fn run_with_authority<F, S>(
    config: &HostConfiguration,
    root: &Path,
    ipc_directory: &Path,
    key: Arc<SigningIdentity>,
    options: NativeSyncOptions,
    stop: S,
    started: F,
) -> io::Result<()>
where
    F: FnOnce(HostStarted, Arc<dyn ExecutionAuthority>) -> io::Result<()>,
    S: Future<Output = io::Result<()>>,
{
    config.validate_key(&key)?;
    let expected = match config.local {
        HostMember::Voter { .. } => NativePeerRole::CtoxInstance,
        HostMember::Worker { .. } => NativePeerRole::WorkjetExecutor,
    };
    if options.room != config.room()
        || options.peer_role != expected
        || !options.collections.is_empty()
        || !options.database.collections.lock().is_empty()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "execution host requires its configured role and control-only room",
        ));
    }
    // Validate paths and identity before any network activity or Raft write.
    let voter = if matches!(config.local, HostMember::Voter { .. }) {
        Some(config.voter_options(root, ipc_directory, &key)?)
    } else {
        None
    };
    let worker = if matches!(config.local, HostMember::Worker { .. }) {
        Some(config.worker_options(ipc_directory, &key)?)
    } else {
        None
    };
    let mut session = NativeSyncSession::start(options).await?;
    let result = async {
        let (endpoint, authority, waiting): (
            PathBuf,
            Arc<dyn ExecutionAuthority>,
            std::pin::Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>>,
        ) = if let Some(options) = voter {
            let host = session.attach_execution(options, key).await?;
            (host.ipc_endpoint().to_path_buf(), host.node().clone(), Box::pin(host.wait_stopped()))
        } else {
            let host = session.attach_worker(worker.expect("validated worker"), key).await?;
            (host.ipc_endpoint().to_path_buf(), host.node().clone(), Box::pin(host.wait_stopped()))
        };
        started(HostStarted { ipc_endpoint: endpoint, node_id: config.node_id(), scope_id: config.scope_id.clone() }, authority)?;
        tokio::select! {
            result = stop => result,
            result = waiting => result.and(Err(io::Error::other("native execution listener or discovery stopped unexpectedly"))),
        }
    }.await;
    session.shutdown().await;
    result
}
