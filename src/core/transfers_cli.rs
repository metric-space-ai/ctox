//! Local operator surface. Remote/Business OS admission belongs at its existing policy gate.
use anyhow::{bail, Result};
use ctox_transfers::{DaemonWorker, DownloadRequest, Store};
use std::path::Path;

fn store(root: &Path) -> Result<Store> {
    Store::open(crate::paths::core_db(root), root.join("runtime/transfers"))
}

pub fn start_daemon(root: &Path) -> Result<DaemonWorker> {
    DaemonWorker::start_with_peer(store(root)?, crate::transfers_native::daemon_peer(root))
}

/// Called by native account bootstrap once its saved-target credential providers
/// are available. UI lifetime does not own this worker or its transport sessions.
pub(crate) fn start_daemon_with_native_accounts(
    root: &Path,
    host: std::sync::Arc<dyn ctox_sync::business_data_session::BusinessDataSessionHost>,
    providers: std::collections::BTreeMap<String, ctox_sync::native::NativeSessionTargetProvider>,
) -> Result<DaemonWorker> {
    DaemonWorker::start_with_peer(
        store(root)?,
        std::sync::Arc::new(crate::transfers_peer::NativeTransferPeerResolver::new(
            host, providers,
        )),
    )
}

/// Restore native providers lazily, so a target enrolled after boot is visible
/// on its first job or explicit resume. Each provider still reads live authority.
pub(crate) fn start_daemon_with_account_host(
    root: &Path,
    host: std::sync::Arc<crate::native_transfer_accounts::NativeTransferAccountHost>,
) -> Result<DaemonWorker> {
    let resolver = crate::transfers_peer::NativeTransferPeerResolver::with_account_host(host);
    DaemonWorker::start_with_peer(store(root)?, std::sync::Arc::new(resolver))
}

pub fn handle(root: &Path, args: &[String]) -> Result<()> {
    let store = store(root)?;
    let transfer = match args.first().map(String::as_str) {
        Some("download") if args.len() >= 5 => store.enqueue(DownloadRequest {
            id: args[1].clone(),
            sha256: args[2].clone(),
            size: args[3].parse()?,
            sources: args[4..].to_vec(),
            peer_source: None,
        })?,
        Some("status") if args.len() == 2 => store.get(&args[1])?,
        Some(action @ ("pause" | "resume" | "cancel")) if args.len() == 2 => {
            store.control(&args[1], action)?
        }
        _ => bail!("usage: ctox transfer download ID SHA256 SIZE URL [MIRROR...] | status ID | pause ID | resume ID | cancel ID"),
    };
    println!("{}", serde_json::to_string_pretty(&transfer)?);
    Ok(())
}
