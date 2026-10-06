//! Explicit protocol integration fixture, run by storage-protocol-checks.py.
use anyhow::{ensure, Result};
use ctox_transfers::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct Live {
    config: Value,
    store: Store,
    pause: AtomicBool,
}
impl StorageResolver for Live {
    fn authorize(&self, request: &DownloadRequest) -> Result<()> {
        ensure!(
            request.storage.as_ref().unwrap().endpoint_fingerprint == "a".repeat(64),
            "fixture endpoint changed"
        );
        if self.pause.load(Ordering::Acquire) && self.store.get(&request.id)?.completed_bytes > 0 {
            self.pause.store(false, Ordering::Release);
            self.store.control(&request.id, "pause")?;
        }
        Ok(())
    }
    fn connect(&self, _: &DownloadRequest) -> Result<Box<dyn StorageConnection>> {
        let c = &self.config;
        let text = |k: &str| -> String { c[k].as_str().unwrap().into() };
        match c["protocol"].as_str().unwrap() {
            "ssh" => storage_ssh::connect(storage_ssh::SshStorageOptions {
                host: text("host"),
                port: c["port"].as_u64().unwrap() as u16,
                username: text("username"),
                root: text("root"),
                host_key_sha256: text("host_key_sha256"),
                private_key: c["private_key"].as_str().unwrap(),
                passphrase: None,
            }),
            "smb" => storage_smb::connect(storage_smb::SmbStorageOptions {
                host: text("host"),
                port: c["port"].as_u64().unwrap() as u16,
                username: text("username"),
                root: text("root"),
                share: text("share"),
                password: c["password"].as_str().unwrap(),
            }),
            _ => anyhow::bail!("unsupported fixture protocol"),
        }
    }
}
fn request(id: &str, body: &[u8], direction: StorageDirection) -> DownloadRequest {
    DownloadRequest {
        id: id.into(),
        sources: vec![],
        peer_source: None,
        sha256: format!("{:x}", Sha256::digest(body)),
        size: body.len() as u64,
        storage: Some(StorageTransfer {
            owner_user_id: "fixture-owner".into(),
            computer_id: "fixture-computer".into(),
            endpoint_ref: "fixture-storage".into(),
            endpoint_fingerprint: "a".repeat(64),
            purpose: "artifacts".into(),
            relative_path: "verified-artifact.bin".into(),
            direction,
        }),
    }
}
#[tokio::test]
#[ignore = "requires bounded real SSH/SMB fixture and private STORAGE_LIVE_CONFIG"]
async fn real_storage_pause_restart_upload_download_and_no_replace() {
    let path = std::env::var_os("STORAGE_LIVE_CONFIG").expect("fixture config required");
    let config: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(
        dir.path().join("state.sqlite"),
        dir.path().join("artifacts"),
    )
    .unwrap();
    let live = Arc::new(Live {
        config,
        store: store.clone(),
        pause: AtomicBool::new(true),
    });
    let body: Vec<u8> = (0..(4 * 1024 * 1024 + 127))
        .map(|i| (i % 251) as u8)
        .collect();
    let source = dir.path().join("source.bin");
    std::fs::write(&source, &body).unwrap();
    let upload = request("live-upload", &body, StorageDirection::Upload);
    store
        .stage_storage_upload(&source, &upload.sha256, upload.size)
        .unwrap();
    store.enqueue(upload.clone()).unwrap();
    let worker = store.worker_with_sources(None, Some(live.clone())).unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    let paused = store.get(&upload.id).unwrap();
    assert_eq!(paused.state, "paused");
    assert!(paused.completed_bytes > 0 && paused.completed_bytes < upload.size);
    drop(worker);
    store.control(&upload.id, "resume").unwrap();
    let worker = store.worker_with_sources(None, Some(live.clone())).unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    let done = store.get(&upload.id).unwrap();
    assert_eq!(done.state, "completed", "{:?}", done.error_code);
    assert_eq!(done.request, upload);
    drop(worker);
    let destination = Store::open(
        dir.path().join("recipient.sqlite"),
        dir.path().join("recipient"),
    )
    .unwrap();
    let download = request("live-download", &body, StorageDirection::Download);
    destination.enqueue(download.clone()).unwrap();
    let recipient = Arc::new(Live {
        config: live.config.clone(),
        store: destination.clone(),
        pause: AtomicBool::new(false),
    });
    let worker = destination
        .worker_with_sources(None, Some(recipient))
        .unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    assert_eq!(destination.get(&download.id).unwrap().state, "completed");
    assert_eq!(
        std::fs::read(dir.path().join("recipient/objects").join(&download.sha256)).unwrap(),
        body
    );
    let other = request(
        "live-conflict",
        b"do not overwrite",
        StorageDirection::Upload,
    );
    let other_source = dir.path().join("other");
    std::fs::write(&other_source, b"do not overwrite").unwrap();
    destination
        .stage_storage_upload(&other_source, &other.sha256, other.size)
        .unwrap();
    destination.enqueue(other.clone()).unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    assert_eq!(destination.get(&other.id).unwrap().state, "failed");
    let mut remote = live.connect(&upload).unwrap();
    assert_eq!(
        remote.length("verified-artifact.bin").unwrap(),
        Some(upload.size)
    );
    #[cfg(unix)]
    if live.config["protocol"] == "ssh" {
        let root = std::path::PathBuf::from(live.config["host_root"].as_str().unwrap());
        assert!(root.starts_with("/mnt/nvme1/build-lane/artifacts/ctox-storage"));
        assert_eq!(root.file_name().unwrap(), "ssh-data");
        let saved = root.with_extension("original");
        std::fs::rename(&root, &saved).unwrap();
        // The container sees this as another directory outside the admitted root.
        std::os::unix::fs::symlink("/fixture/smb-data", &root).unwrap();
        let replaced = remote.length("verified-artifact.bin");
        std::fs::remove_file(&root).unwrap();
        std::fs::rename(&saved, &root).unwrap();
        assert!(
            replaced.is_err(),
            "an existing connection followed a replaced root"
        );
        println!("PROTOCOL_AUTHORITY: SSH root replacement rejected");
    }
    remote.close().unwrap();
    println!(
        "PROTOCOL_ACCEPTANCE: {} pause/reopen upload/download integrity and no-replace passed",
        live.config["protocol"].as_str().unwrap()
    );
}
