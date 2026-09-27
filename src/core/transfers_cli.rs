//! Local operator surface. Remote/Business OS admission belongs at its existing policy gate.
use anyhow::{bail, Result};
use ctox_transfers::{DaemonWorker, DownloadRequest, Store};
use std::path::Path;

fn store(root: &Path) -> Result<Store> {
    Store::open(crate::paths::core_db(root), root.join("runtime/transfers"))
}

pub fn start_daemon(root: &Path) -> Result<DaemonWorker> {
    DaemonWorker::start(store(root)?)
}

pub fn handle(root: &Path, args: &[String]) -> Result<()> {
    let store = store(root)?;
    let transfer = match args.first().map(String::as_str) {
        Some("download") if args.len() >= 5 => store.enqueue(DownloadRequest {
            id: args[1].clone(),
            sha256: args[2].clone(),
            size: args[3].parse()?,
            sources: args[4..].to_vec(),
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
