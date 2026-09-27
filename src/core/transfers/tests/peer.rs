use ctox_transfers::{DownloadRequest, PeerRangeSource, PeerSource, Store};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

struct PendingPeer {
    entered: tokio::sync::Notify,
    dropped: Arc<AtomicBool>,
}
impl PeerRangeSource for PendingPeer {
    fn authorize<'a>(
        &'a self,
        _: &'a DownloadRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn read_range<'a>(
        &'a self,
        _: &'a DownloadRequest,
        _: u64,
        _: u64,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<u8>>> + Send + 'a>> {
        Box::pin(async move {
            struct Dropped(Arc<AtomicBool>);
            impl Drop for Dropped {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::SeqCst);
                }
            }
            let _guard = Dropped(self.dropped.clone());
            self.entered.notify_one();
            std::future::pending().await
        })
    }
}

#[tokio::test]
async fn peer_cancel_drops_inflight_range_before_releasing_worker() {
    let temp = tempfile::tempdir().unwrap();
    let store = store(&temp);
    store.enqueue(request(b"pending")).unwrap();
    let dropped = Arc::new(AtomicBool::new(false));
    let peer = Arc::new(PendingPeer {
        entered: tokio::sync::Notify::new(),
        dropped: dropped.clone(),
    });
    let worker = store.worker_with_peer(peer.clone()).unwrap();
    let stop = AtomicBool::new(false);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let (result, ()) = tokio::join!(worker.run_next(&stop), async {
            peer.entered.notified().await;
            store.control("peer-transfer", "cancel").unwrap();
        });
        result.unwrap();
    })
    .await
    .unwrap();
    assert!(dropped.load(Ordering::SeqCst));
    let result = store.get("peer-transfer").unwrap();
    assert_eq!(result.state, "cancelled");
    assert_eq!(result.completed_bytes, 0);
    assert!(result.receipt.is_none());
}

struct Peer {
    body: Vec<u8>,
    fail_second: AtomicBool,
    reads: Mutex<Vec<u64>>,
}
impl PeerRangeSource for Peer {
    fn authorize<'a>(
        &'a self,
        _: &'a DownloadRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn read_range<'a>(
        &'a self,
        _: &'a DownloadRequest,
        offset: u64,
        length: u64,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<u8>>> + Send + 'a>> {
        Box::pin(async move {
            self.reads.lock().unwrap().push(offset);
            anyhow::ensure!(
                !(offset > 0 && self.fail_second.swap(false, Ordering::SeqCst)),
                "disconnected"
            );
            Ok(self.body[offset as usize..(offset + length) as usize].to_vec())
        })
    }
}
fn request(body: &[u8]) -> DownloadRequest {
    DownloadRequest {
        id: "peer-transfer".into(),
        sources: vec![],
        peer_source: Some(PeerSource {
            instance_id: "trusted-instance".into(),
            public_key: "enrolled-key".into(),
            collection: "desktop_files".into(),
            file_id: "source-file".into(),
        }),
        sha256: format!("{:x}", Sha256::digest(body)),
        size: body.len() as u64,
    }
}
fn store(temp: &tempfile::TempDir) -> Store {
    Store::open(
        temp.path().join("ctox.sqlite3"),
        temp.path().join("transfers"),
    )
    .unwrap()
}

#[tokio::test]
async fn peer_interruption_reopens_at_flushed_offset_and_discards_uncommitted_tail() {
    let temp = tempfile::tempdir().unwrap();
    let body = vec![0x3a; 1024 * 1024 + 37];
    let peer = Arc::new(Peer {
        body: body.clone(),
        fail_second: AtomicBool::new(true),
        reads: Mutex::new(vec![]),
    });
    let store = store(&temp);
    store.enqueue(request(&body)).unwrap();
    store
        .worker_with_peer(peer.clone())
        .unwrap()
        .run_next(&AtomicBool::new(false))
        .await
        .unwrap();
    let failed = store.get("peer-transfer").unwrap();
    assert_eq!(failed.state, "failed");
    assert_eq!(failed.completed_bytes, 1024 * 1024);
    // Simulate bytes written after the last durable range checkpoint.
    use std::io::Write;
    let partial = temp
        .path()
        .join("transfers/staging/peer-transfer/peer/payload");
    std::fs::OpenOptions::new()
        .append(true)
        .open(&partial)
        .unwrap()
        .write_all(b"uncommitted garbage")
        .unwrap();
    drop(store);
    let reopened = Store::open(
        temp.path().join("ctox.sqlite3"),
        temp.path().join("transfers"),
    )
    .unwrap();
    reopened.control("peer-transfer", "resume").unwrap();
    reopened
        .worker_with_peer(peer.clone())
        .unwrap()
        .run_next(&AtomicBool::new(false))
        .await
        .unwrap();
    let result = reopened.get("peer-transfer").unwrap();
    assert_eq!(result.state, "completed");
    let receipt = result.receipt.unwrap();
    assert_eq!(receipt.transport, "ctox-webrtc-file-v1");
    assert!(receipt.engine_revision.is_none());
    assert_eq!(
        std::fs::read(temp.path().join("transfers").join(receipt.artifact)).unwrap(),
        body
    );
    assert_eq!(
        *peer.reads.lock().unwrap(),
        vec![0, 1024 * 1024, 1024 * 1024]
    );
}

#[tokio::test]
async fn peer_hash_mismatch_quarantines_and_missing_resolver_never_publishes() {
    let temp = tempfile::tempdir().unwrap();
    let store = store(&temp);
    store.enqueue(request(b"good")).unwrap();
    store
        .worker()
        .unwrap()
        .run_next(&AtomicBool::new(false))
        .await
        .unwrap();
    assert_eq!(store.get("peer-transfer").unwrap().state, "failed");
    let peer = Arc::new(Peer {
        body: b"evil".to_vec(),
        fail_second: AtomicBool::new(false),
        reads: Mutex::new(vec![]),
    });
    store.control("peer-transfer", "resume").unwrap();
    store
        .worker_with_peer(peer)
        .unwrap()
        .run_next(&AtomicBool::new(false))
        .await
        .unwrap();
    let result = store.get("peer-transfer").unwrap();
    assert_eq!(result.state, "failed");
    assert!(result.receipt.is_none());
    assert_eq!(result.completed_bytes, 0);
    assert_eq!(
        std::fs::read(
            temp.path()
                .join("transfers/staging/peer-transfer/peer/rejected-0")
        )
        .unwrap(),
        b"evil"
    );
}

#[tokio::test]
async fn peer_empty_content_requires_a_remote_read_and_source_binding_is_immutable() {
    let temp = tempfile::tempdir().unwrap();
    let store = store(&temp);
    let req = request(b"");
    store.enqueue(req.clone()).unwrap();
    let mut changed = req.clone();
    changed.peer_source.as_mut().unwrap().instance_id = "other".into();
    assert!(store.enqueue(changed).is_err());
    let mut mixed = req;
    mixed.sources.push("https://example.com/blob".into());
    assert!(store.enqueue(mixed).is_err());
    let peer = Arc::new(Peer {
        body: vec![],
        fail_second: AtomicBool::new(false),
        reads: Mutex::new(vec![]),
    });
    store
        .worker_with_peer(peer.clone())
        .unwrap()
        .run_next(&AtomicBool::new(false))
        .await
        .unwrap();
    assert_eq!(store.get("peer-transfer").unwrap().state, "completed");
    assert_eq!(*peer.reads.lock().unwrap(), vec![0]);
}
