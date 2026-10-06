use anyhow::{ensure, Result};
use ctox_transfers::*;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

#[derive(Clone)]
struct Remote {
    files: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    store: Store,
    pause_after_write: Arc<AtomicBool>,
    authorized: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
}
impl StorageResolver for Remote {
    fn authorize(&self, _: &DownloadRequest) -> Result<()> {
        ensure!(self.authorized.load(Ordering::Acquire), "revoked");
        Ok(())
    }
    fn connect(&self, _: &DownloadRequest) -> Result<Box<dyn StorageConnection>> {
        self.closed.store(false, Ordering::Release);
        Ok(Box::new(self.clone()))
    }
}
impl StorageConnection for Remote {
    fn length(&mut self, path: &str) -> Result<Option<u64>> {
        Ok(self.files.lock().unwrap().get(path).map(|v| v.len() as u64))
    }
    fn read(&mut self, path: &str, offset: u64, length: usize) -> Result<Vec<u8>> {
        let files = self.files.lock().unwrap();
        let bytes = &files[path];
        Ok(bytes[offset as usize..offset as usize + length].to_vec())
    }
    fn create(&mut self, path: &str) -> Result<()> {
        let mut files = self.files.lock().unwrap();
        ensure!(!files.contains_key(path), "exists");
        files.insert(path.into(), vec![]);
        Ok(())
    }
    fn truncate(&mut self, path: &str, length: u64) -> Result<()> {
        self.files
            .lock()
            .unwrap()
            .get_mut(path)
            .unwrap()
            .resize(length as usize, 0);
        Ok(())
    }
    fn write(&mut self, path: &str, offset: u64, bytes: &[u8]) -> Result<()> {
        {
            let mut files = self.files.lock().unwrap();
            let target = files.get_mut(path).unwrap();
            target.resize(offset as usize + bytes.len(), 0);
            target[offset as usize..].copy_from_slice(bytes);
        }
        if self.pause_after_write.swap(false, Ordering::AcqRel) {
            self.store.control("upload", "pause")?;
        }
        Ok(())
    }
    fn publish(&mut self, staging: &str, destination: &str) -> Result<()> {
        let mut files = self.files.lock().unwrap();
        ensure!(!files.contains_key(destination), "exists");
        let bytes = files.remove(staging).unwrap();
        files.insert(destination.into(), bytes);
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        Ok(())
    }
}
fn fixture() -> (tempfile::TempDir, Store, Remote, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(
        dir.path().join("state.sqlite"),
        dir.path().join("artifacts"),
    )
    .unwrap();
    let remote = Remote {
        files: Default::default(),
        store: store.clone(),
        pause_after_write: Arc::new(AtomicBool::new(false)),
        authorized: Arc::new(AtomicBool::new(true)),
        closed: Arc::new(AtomicBool::new(true)),
    };
    let bytes = (0..(2 * 1024 * 1024 + 17))
        .map(|i| (i % 251) as u8)
        .collect();
    (dir, store, remote, bytes)
}
fn request(bytes: &[u8], direction: StorageDirection) -> DownloadRequest {
    DownloadRequest {
        id: if direction == StorageDirection::Upload {
            "upload"
        } else {
            "download"
        }
        .into(),
        sources: vec![],
        peer_source: None,
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size: bytes.len() as u64,
        storage: Some(StorageTransfer {
            owner_user_id: "owner".into(),
            computer_id: "computer".into(),
            endpoint_ref: "nas".into(),
            endpoint_revision: "a".repeat(64),
            purpose: "artifacts".into(),
            relative_path: "result.bin".into(),
            direction,
        }),
    }
}
fn stage(dir: &std::path::Path, store: &Store, bytes: &[u8], req: &DownloadRequest) {
    let path = dir.join("source");
    std::fs::write(&path, bytes).unwrap();
    store
        .stage_storage_upload(&path, &req.sha256, req.size)
        .unwrap();
}

#[tokio::test]
async fn upload_pause_reopen_resume_and_download_roundtrip() {
    let (dir, store, remote, bytes) = fixture();
    let req = request(&bytes, StorageDirection::Upload);
    stage(dir.path(), &store, &bytes, &req);
    store.enqueue(req.clone()).unwrap();
    remote.pause_after_write.store(true, Ordering::Release);
    let worker = store
        .worker_with_sources(None, Some(Arc::new(remote.clone())))
        .unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    let paused = store.get("upload").unwrap();
    assert_eq!(paused.state, "paused");
    assert_eq!(paused.completed_bytes, 1024 * 1024);
    assert!(remote.closed.load(Ordering::Acquire));
    assert!(!remote.files.lock().unwrap().contains_key("result.bin"));
    drop(worker);
    let reopened = Store::open(
        dir.path().join("state.sqlite"),
        dir.path().join("artifacts"),
    )
    .unwrap();
    reopened.control("upload", "resume").unwrap();
    let worker = reopened
        .worker_with_sources(None, Some(Arc::new(remote.clone())))
        .unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    let done = reopened.get("upload").unwrap();
    assert_eq!(done.state, "completed");
    assert_eq!(done.request, req);
    assert_eq!(done.receipt.unwrap().artifact, "storage:nas/result.bin");
    assert_eq!(remote.files.lock().unwrap()["result.bin"], bytes);
    drop(worker);
    // Separate recipient store proves actual source reads rather than upload cache reuse.
    let recipient = Store::open(
        dir.path().join("recipient.sqlite"),
        dir.path().join("recipient"),
    )
    .unwrap();
    let req = request(&bytes, StorageDirection::Download);
    recipient.enqueue(req.clone()).unwrap();
    let worker = recipient
        .worker_with_sources(None, Some(Arc::new(remote)))
        .unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    assert_eq!(recipient.get("download").unwrap().state, "completed");
    assert_eq!(
        std::fs::read(dir.path().join("recipient/objects").join(req.sha256)).unwrap(),
        bytes
    );
}

#[tokio::test]
async fn changed_remote_prefix_and_existing_target_are_never_overwritten() {
    let (dir, store, remote, bytes) = fixture();
    let req = request(&bytes, StorageDirection::Upload);
    stage(dir.path(), &store, &bytes, &req);
    store.enqueue(req).unwrap();
    remote.pause_after_write.store(true, Ordering::Release);
    let worker = store
        .worker_with_sources(None, Some(Arc::new(remote.clone())))
        .unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    remote.files.lock().unwrap().values_mut().next().unwrap()[0] ^= 255;
    store.control("upload", "resume").unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    assert_eq!(store.get("upload").unwrap().state, "failed");
    assert!(!remote.files.lock().unwrap().contains_key("result.bin"));
    remote
        .files
        .lock()
        .unwrap()
        .insert("result.bin".into(), b"existing unrelated file".to_vec());
    store.control("upload", "resume").unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    assert_eq!(store.get("upload").unwrap().state, "failed");
    assert_eq!(
        remote.files.lock().unwrap()["result.bin"],
        b"existing unrelated file"
    );
}

#[tokio::test]
async fn cancellation_terminal_and_revoked_registry_denies_resume() {
    let (dir, store, remote, bytes) = fixture();
    let req = request(&bytes, StorageDirection::Upload);
    stage(dir.path(), &store, &bytes, &req);
    store.enqueue(req).unwrap();
    remote.pause_after_write.store(true, Ordering::Release);
    let worker = store
        .worker_with_sources(None, Some(Arc::new(remote.clone())))
        .unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    remote.authorized.store(false, Ordering::Release);
    store.control("upload", "resume").unwrap();
    worker.run_next(&AtomicBool::new(false)).await.unwrap();
    assert_eq!(store.get("upload").unwrap().state, "failed");
    store.control("upload", "cancel").unwrap();
    store.control("upload", "resume").unwrap();
    assert_eq!(store.get("upload").unwrap().state, "cancelled");
    assert!(!remote.files.lock().unwrap().contains_key("result.bin"));
    assert!(!worker.run_next(&AtomicBool::new(false)).await.unwrap());
}

#[test]
fn rejects_traversal_mixed_sources_and_retargeted_resume() {
    let (_dir, store, _remote, bytes) = fixture();
    for path in [
        "../escape",
        "/absolute",
        "a/../b",
        "a\\b",
        "a//b",
        "a:stream",
        ".ctox-transfer-fake.part",
        "x\nput",
    ] {
        assert!(validate_relative_path(path).is_err(), "{path}");
    }
    let mut req = request(&bytes, StorageDirection::Download);
    store.enqueue(req.clone()).unwrap();
    req.storage.as_mut().unwrap().endpoint_revision = "b".repeat(64);
    assert!(store.enqueue(req.clone()).is_err());
    req.id = "mixed".into();
    req.sources.push("https://example.org/file".into());
    assert!(store.enqueue(req).is_err());
}
