use ctox_transfers::{DownloadRequest, PeerAccountBinding, PeerRangeSource, PeerSource, Store};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

type Check<'a, T> = Pin<Box<dyn Future<Output = anyhow::Result<T>> + Send + 'a>>;

struct ScopedPeer {
    admitted: DownloadRequest,
    body: Vec<u8>,
    allowed: AtomicBool,
    revoke_after_read: bool,
    checks: Mutex<Vec<String>>,
    reads: AtomicUsize,
}
impl PeerRangeSource for ScopedPeer {
    fn authorize<'a>(&'a self, request: &'a DownloadRequest) -> Check<'a, ()> {
        Box::pin(async move {
            self.checks.lock().unwrap().push(request.id.clone());
            anyhow::ensure!(
                request == &self.admitted && self.allowed.load(Ordering::SeqCst),
                "job is not currently admitted"
            );
            Ok(())
        })
    }
    fn read_range<'a>(
        &'a self,
        request: &'a DownloadRequest,
        offset: u64,
        length: u64,
    ) -> Check<'a, Vec<u8>> {
        Box::pin(async move {
            self.authorize(request).await?;
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.revoke_after_read {
                self.allowed.store(false, Ordering::SeqCst);
            }
            Ok(self.body[offset as usize..(offset + length) as usize].to_vec())
        })
    }
}
fn request(id: &str, body: &[u8]) -> DownloadRequest {
    DownloadRequest {
        id: id.into(),
        sources: vec![],
        peer_source: Some(PeerSource {
            account_binding: None,
            instance_id: "instance".into(),
            public_key: "enrolled-key".into(),
            collection: "desktop_files".into(),
            file_id: "file".into(),
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
fn peer(request: DownloadRequest, body: &[u8], revoke_after_read: bool) -> Arc<ScopedPeer> {
    Arc::new(ScopedPeer {
        admitted: request,
        body: body.to_vec(),
        allowed: AtomicBool::new(true),
        revoke_after_read,
        checks: Mutex::new(vec![]),
        reads: AtomicUsize::new(0),
    })
}

#[test]
fn original_account_binding_survives_reopen_and_cannot_be_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let original = PeerAccountBinding {
        target_id: "enrolled-target".into(),
        grant_id: "issued-transfer-grant".into(),
        account_epoch: 7,
        principal_sha256: "a".repeat(64),
    };
    let mut req = request("bound", b"account-bound content");
    req.peer_source.as_mut().unwrap().account_binding = Some(original.clone());
    store(&temp).enqueue(req.clone()).unwrap();
    let reopened = store(&temp);
    assert_eq!(reopened.get("bound").unwrap().request, req);
    reopened.control("bound", "pause").unwrap();
    reopened.control("bound", "resume").unwrap();
    assert_eq!(reopened.get("bound").unwrap().request, req);
    for binding in [
        PeerAccountBinding {
            grant_id: "different-grant".into(),
            ..original.clone()
        },
        PeerAccountBinding {
            target_id: "other-target".into(),
            ..original.clone()
        },
        PeerAccountBinding {
            account_epoch: 8,
            ..original.clone()
        },
        PeerAccountBinding {
            principal_sha256: "b".repeat(64),
            ..original.clone()
        },
    ] {
        let mut changed = req.clone();
        changed.peer_source.as_mut().unwrap().account_binding = Some(binding);
        assert!(reopened.enqueue(changed).is_err());
        assert_eq!(reopened.get("bound").unwrap().request, req);
    }
    let mut invalid = req.clone();
    invalid.id = "invalid".into();
    invalid
        .peer_source
        .as_mut()
        .unwrap()
        .account_binding
        .as_mut()
        .unwrap()
        .principal_sha256 = "not-a-digest".into();
    assert!(reopened.enqueue(invalid).is_err());
    assert!(reopened.get("invalid").is_err());
    // Old persisted bindings remain readable but cannot acquire authority by
    // silently selecting or minting a new grant during restart.
    let mut legacy = serde_json::to_value(&original).unwrap();
    legacy.as_object_mut().unwrap().remove("grant_id");
    let legacy: PeerAccountBinding = serde_json::from_value(legacy).unwrap();
    assert!(legacy.validate().is_err());
    let mut missing_grant = req;
    missing_grant.id = "missing-grant".into();
    missing_grant.peer_source.as_mut().unwrap().account_binding = Some(legacy);
    assert!(reopened.enqueue(missing_grant).is_err());
    assert!(reopened.get("missing-grant").is_err());
}

#[tokio::test]
async fn cached_content_requires_current_authority_for_the_exact_job() {
    let temp = tempfile::tempdir().unwrap();
    let store = store(&temp);
    let body = b"shared content";
    let first = request("first", body);
    let provider = peer(first.clone(), body, false);
    store.enqueue(first.clone()).unwrap();
    let stop = AtomicBool::new(false);
    store
        .worker_with_peer(provider.clone())
        .unwrap()
        .run_next(&stop)
        .await
        .unwrap();
    assert_eq!(store.get("first").unwrap().state, "completed");
    let second = request("second", body);
    store.enqueue(second.clone()).unwrap();
    // An identical source and hash do not transfer job/account authority.
    store
        .worker_with_peer(provider.clone())
        .unwrap()
        .run_next(&stop)
        .await
        .unwrap();
    let denied = store.get("second").unwrap();
    assert_eq!(denied.state, "failed");
    assert!(denied.receipt.is_none());
    assert_eq!(provider.reads.load(Ordering::SeqCst), 1);
    assert_eq!(provider.checks.lock().unwrap().last().unwrap(), "second");
    store.control("second", "resume").unwrap();
    store.worker().unwrap().run_next(&stop).await.unwrap();
    assert_eq!(store.get("second").unwrap().state, "failed");
    assert!(store.get("second").unwrap().receipt.is_none());
    // With fresh admission for the second job, verified cache reuse still works.
    let admitted = peer(second, body, false);
    store.control("second", "resume").unwrap();
    store
        .worker_with_peer(admitted.clone())
        .unwrap()
        .run_next(&stop)
        .await
        .unwrap();
    assert_eq!(store.get("second").unwrap().state, "completed");
    assert_eq!(admitted.reads.load(Ordering::SeqCst), 0);
    assert!(admitted.checks.lock().unwrap().len() >= 2);
}

#[tokio::test]
async fn revocation_blocks_publication_and_fully_checkpointed_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let store = store(&temp);
    let body = b"fully checkpointed";
    let req = request("resumed", body);
    let provider = peer(req.clone(), body, true);
    store.enqueue(req.clone()).unwrap();
    let stop = AtomicBool::new(false);
    store
        .worker_with_peer(provider.clone())
        .unwrap()
        .run_next(&stop)
        .await
        .unwrap();
    let denied = store.get("resumed").unwrap();
    assert_eq!(denied.state, "failed");
    assert_eq!(denied.completed_bytes, req.size);
    assert!(denied.receipt.is_none());
    assert!(!temp
        .path()
        .join("transfers/objects")
        .join(&req.sha256)
        .exists());
    drop(store);
    let reopened = Store::open(
        temp.path().join("ctox.sqlite3"),
        temp.path().join("transfers"),
    )
    .unwrap();
    reopened.control("resumed", "resume").unwrap();
    reopened
        .worker_with_peer(provider.clone())
        .unwrap()
        .run_next(&stop)
        .await
        .unwrap();
    assert_eq!(reopened.get("resumed").unwrap().state, "failed");
    assert!(reopened.get("resumed").unwrap().receipt.is_none());
    assert_eq!(provider.reads.load(Ordering::SeqCst), 1);
    provider.allowed.store(true, Ordering::SeqCst);
    reopened.control("resumed", "resume").unwrap();
    reopened
        .worker_with_peer(provider.clone())
        .unwrap()
        .run_next(&stop)
        .await
        .unwrap();
    let done = reopened.get("resumed").unwrap();
    assert_eq!(done.state, "completed");
    assert_eq!(provider.reads.load(Ordering::SeqCst), 1);
    assert_eq!(
        std::fs::read(
            temp.path()
                .join("transfers")
                .join(done.receipt.unwrap().artifact)
        )
        .unwrap(),
        body
    );
}

struct PendingAdmission {
    entered: tokio::sync::Notify,
    dropped: Arc<AtomicBool>,
}
impl PeerRangeSource for PendingAdmission {
    fn authorize<'a>(&'a self, _: &'a DownloadRequest) -> Check<'a, ()> {
        Box::pin(async move {
            struct Guard(Arc<AtomicBool>);
            impl Drop for Guard {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::SeqCst);
                }
            }
            let _guard = Guard(self.dropped.clone());
            self.entered.notify_one();
            std::future::pending().await
        })
    }
    fn read_range<'a>(&'a self, _: &'a DownloadRequest, _: u64, _: u64) -> Check<'a, Vec<u8>> {
        Box::pin(async { panic!("unadmitted range must never start") })
    }
}
#[tokio::test]
async fn cancellation_drops_pending_admission_before_releasing_worker() {
    let temp = tempfile::tempdir().unwrap();
    let store = store(&temp);
    store.enqueue(request("pending", b"pending")).unwrap();
    let dropped = Arc::new(AtomicBool::new(false));
    let provider = Arc::new(PendingAdmission {
        entered: tokio::sync::Notify::new(),
        dropped: dropped.clone(),
    });
    let worker = store.worker_with_peer(provider.clone()).unwrap();
    let stop = AtomicBool::new(false);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let (result, ()) = tokio::join!(worker.run_next(&stop), async {
            provider.entered.notified().await;
            store.control("pending", "cancel").unwrap();
        });
        result.unwrap();
    })
    .await
    .unwrap();
    assert!(dropped.load(Ordering::SeqCst));
    let done = store.get("pending").unwrap();
    assert_eq!(done.state, "cancelled");
    assert_eq!(done.completed_bytes, 0);
    assert!(done.receipt.is_none());
}
