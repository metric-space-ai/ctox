//! Local operator surface. Remote/Business OS admission belongs at its existing policy gate.
use anyhow::{bail, Result};
use ctox_transfers::{DaemonWorker, DownloadRequest, Store};
use std::path::Path;

fn store(root: &Path) -> Result<Store> {
    Store::open(
        crate::paths::core_db(root),
        crate::paths::runtime_dir(root).join("transfers"),
    )
}

pub fn start_daemon(root: &Path) -> Result<DaemonWorker> {
    DaemonWorker::start_with_sources(
        store(root)?,
        Some(crate::transfers_native::daemon_peer(root)),
        Some(crate::transfers_storage::resolver(root)),
    )
}

/// Called by native account bootstrap once its saved-target credential providers
/// are available. UI lifetime does not own this worker or its transport sessions.
pub(crate) fn start_daemon_with_native_accounts(
    root: &Path,
    host: std::sync::Arc<dyn ctox_sync::business_data_session::BusinessDataSessionHost>,
    providers: std::collections::BTreeMap<String, ctox_sync::native::NativeSessionTargetProvider>,
) -> Result<DaemonWorker> {
    DaemonWorker::start_with_sources(
        store(root)?,
        Some(std::sync::Arc::new(
            crate::transfers_peer::NativeTransferPeerResolver::new(host, providers),
        )),
        Some(crate::transfers_storage::resolver(root)),
    )
}

/// Restore native providers lazily, so a target enrolled after boot is visible
/// on its first job or explicit resume. Each provider still reads live authority.
pub(crate) fn start_daemon_with_account_host(
    root: &Path,
    host: std::sync::Arc<crate::native_transfer_accounts::NativeTransferAccountHost>,
) -> Result<DaemonWorker> {
    let resolver = crate::transfers_peer::NativeTransferPeerResolver::with_account_host(host);
    DaemonWorker::start_with_sources(
        store(root)?,
        Some(std::sync::Arc::new(resolver)),
        Some(crate::transfers_storage::resolver(root)),
    )
}

fn run_worker_window(root: &Path, seconds: &str) -> Result<()> {
    let seconds: u64 = seconds.parse()?;
    if !(1..=3600).contains(&seconds) {
        bail!("transfer run requires 1 to 3600 seconds");
    }
    let worker = start_daemon(root)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while std::time::Instant::now() < deadline {
        if worker.is_finished() {
            worker.shutdown()?;
            bail!("transfer worker stopped before the requested window ended");
        }
        std::thread::sleep(
            std::time::Duration::from_millis(100)
                .min(deadline.saturating_duration_since(std::time::Instant::now())),
        );
    }
    worker.shutdown()?;
    println!(
        "{}",
        serde_json::json!({"state":"stopped", "runSeconds":seconds})
    );
    Ok(())
}

pub fn handle(root: &Path, args: &[String]) -> Result<()> {
    if args.first().map(String::as_str) == Some("run") && args.len() == 2 {
        return run_worker_window(root, &args[1]);
    }
    if args.first().map(String::as_str) == Some("source-identity") && args.len() == 1 {
        println!(
            "{}",
            serde_json::json!({"sourcePublicIdentity": crate::sync_host::signing_identity(root)?.public_identity()})
        );
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("pair") && args.len() == 4 {
        let paired = crate::transfers_native::pair(root, &args[1], &args[2], Path::new(&args[3]))?;
        println!("{}", serde_json::to_string_pretty(&paired)?);
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("publish") && args.len() == 2 {
        let published = crate::business_os::publish_native_file(root, Path::new(&args[1]))?;
        println!("{}", serde_json::to_string_pretty(&published)?);
        return Ok(());
    }
    let store = store(root)?;
    let transfer = match args.first().map(String::as_str) {
        Some("storage-upload") if args.len() == 10 => crate::transfers_storage::enqueue(root, &store, args, ctox_transfers::StorageDirection::Upload)?,
        Some("storage-download") if args.len() == 9 => crate::transfers_storage::enqueue(root, &store, args, ctox_transfers::StorageDirection::Download)?,
        Some("download") if args.len() >= 5 => store.enqueue(DownloadRequest {
            id: args[1].clone(),
            sha256: args[2].clone(),
            size: args[3].parse()?,
            sources: args[4..].to_vec(),
            storage: None,
            peer_source: None,
        })?,
        Some("peer-download") if args.len() == 6 => crate::transfers_native::enqueue_peer(
            root,
            &store,
            crate::transfers_native::PeerDownload {
                id: args[1].clone(), target_id: args[2].clone(), sha256: args[3].clone(),
                size: args[4].parse()?, file_id: args[5].clone(),
            },
        )?,
        Some("status") if args.len() == 2 => store.get(&args[1])?,
        Some(action @ ("pause" | "resume" | "cancel")) if args.len() == 2 => {
            store.control(&args[1], action)?
        }
        _ => bail!("usage: ctox transfer run SECONDS | source-identity | publish FILE | pair TARGET SOURCE_PUBLIC_IDENTITY INVITE_FILE | download ID SHA256 SIZE URL [MIRROR...] | peer-download ID TARGET SHA256 SIZE FILE_ID | storage-upload ID OWNER COMPUTER ENDPOINT PURPOSE SHA256 SIZE RELATIVE_PATH FILE | storage-download ID OWNER COMPUTER ENDPOINT PURPOSE SHA256 SIZE RELATIVE_PATH | status ID | pause ID | resume ID | cancel ID"),
    };
    println!("{}", serde_json::to_string_pretty(&transfer)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_worker_window_does_not_open_state() {
        let root = tempfile::tempdir().unwrap();
        for seconds in ["0", "3601", "-1", "invalid"] {
            assert!(run_worker_window(root.path(), seconds).is_err());
        }
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn store_obeys_isolated_state_root() {
        const CHILD_ROOT: &str = "CTOX_TEST_TRANSFER_STATE_ROOT_CHILD";
        // Set the process-wide runtime override only in a dedicated child,
        // never in the parent test process shared with other native tests.
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let root = std::path::PathBuf::from(root);
            let state = std::path::PathBuf::from(std::env::var_os("CTOX_STATE_ROOT").unwrap());
            // The bounded foreground command starts no unrelated service and
            // must finish draining its worker before returning.
            run_worker_window(&root, "1").unwrap();
            let opened = store(&root).unwrap();
            opened
                .enqueue(DownloadRequest {
                    id: "isolated-state".into(),
                    sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                        .into(),
                    size: 0,
                    sources: vec!["http://127.0.0.1:9/not-requested".into()],
                    storage: None,
                    peer_source: None,
                })
                .unwrap();
            drop(opened);
            assert!(state.join("ctox.sqlite3").is_file());
            assert!(state.join("transfers").is_dir());
            assert!(!root.join("runtime").exists());
            assert_eq!(
                store(&root)
                    .unwrap()
                    .get("isolated-state")
                    .unwrap()
                    .request
                    .size,
                0
            );
            return;
        }

        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("shared-bundle");
        std::fs::create_dir(&root).unwrap();
        for name in ["instance-a", "instance-b"] {
            let state = fixture.path().join(name);
            std::fs::create_dir(&state).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "transfers_cli::tests::store_obeys_isolated_state_root",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env(CHILD_ROOT, &root)
                .env("CTOX_STATE_ROOT", &state)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "child failed: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            // Enqueuing the same ID in each instance must remain independent.
            assert!(state.join("transfers").is_dir());
        }
        assert!(!root.join("runtime").exists());
    }
}
