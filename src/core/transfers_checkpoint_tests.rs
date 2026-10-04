use super::*;
use ctox_sync::contracts::{
    GitWorkspaceState, SessionManifest, WorkspaceEntry, WorkspaceEntryKind,
};
use ctox_transfers::{PeerAccountBinding, PeerSource};
use std::{
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

type Reply<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;
struct MemoryPeer {
    jobs: BTreeMap<String, (DownloadRequest, Vec<u8>)>,
    allowed: AtomicBool,
    checks: AtomicUsize,
    revoke_at: AtomicUsize,
}
impl PeerRangeSource for MemoryPeer {
    fn authorize<'a>(&'a self, request: &'a DownloadRequest) -> Reply<'a, ()> {
        Box::pin(async move {
            let check = self.checks.fetch_add(1, Ordering::SeqCst) + 1;
            if check == self.revoke_at.load(Ordering::SeqCst) {
                self.allowed.store(false, Ordering::SeqCst);
            }
            ensure!(
                self.allowed.load(Ordering::SeqCst)
                    && self
                        .jobs
                        .get(&request.id)
                        .is_some_and(|(original, _)| original == request),
                "revoked or changed job"
            );
            Ok(())
        })
    }
    fn read_range<'a>(
        &'a self,
        request: &'a DownloadRequest,
        offset: u64,
        length: u64,
    ) -> Reply<'a, Vec<u8>> {
        Box::pin(async move {
            self.authorize(request).await?;
            let bytes = &self.jobs.get(&request.id).unwrap().1;
            Ok(bytes[offset as usize..(offset + length) as usize].to_vec())
        })
    }
}
struct Fixture {
    root: tempfile::TempDir,
    transfers: Store,
    checkpoints: CheckpointStore,
    peer: Arc<MemoryPeer>,
    digest: String,
    ids: BTreeMap<String, String>,
}
impl Fixture {
    fn request(&self) -> GuestCheckpointTransfer<'_> {
        GuestCheckpointTransfer {
            guest_id: "guest",
            execution_job_id: "job",
            ownership: Ownership {
                node_id: 1,
                generation: 1,
            },
            checkpoint_digest: &self.digest,
            manifest_transfer_id: "manifest",
            artifact_transfer_ids: &self.ids,
        }
    }
}
fn reference(bytes: &[u8]) -> ArtifactRef {
    ArtifactRef {
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size_bytes: bytes.len() as u64,
    }
}
fn job(id: &str, bytes: &[u8], target: &str) -> DownloadRequest {
    DownloadRequest {
        id: id.into(),
        sources: vec![],
        sha256: reference(bytes).sha256,
        size: bytes.len() as u64,
        peer_source: Some(PeerSource {
            instance_id: "source".into(),
            public_key: "pinned-source".into(),
            collection: "desktop_files".into(),
            file_id: id.into(),
            account_binding: Some(PeerAccountBinding {
                target_id: target.into(),
                account_epoch: 7,
                grant_id: format!("grant-{id}"),
                principal_sha256: "a".repeat(64),
            }),
        }),
    }
}
async fn fixture(foreign_artifact: bool) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let transfers = Store::open(
        root.path().join("jobs.sqlite3"),
        root.path().join("transfers"),
    )
    .unwrap();
    let checkpoints = CheckpointStore::open(root.path().join("checkpoints"), 1024 * 1024).unwrap();
    let session_id = "11111111-2222-4333-8444-555555555555";
    let metadata = serde_json::json!({
        "timestamp": "2026-10-04T08:00:00Z", "type": "session_meta",
        "payload": {
            "id": session_id, "timestamp": "2026-10-04T08:00:00Z",
            "cwd": "/original/workspace", "originator": "codex_cli_rs",
            "cli_version": "1.0.0", "source": "exec", "model_provider": "test-provider",
            "base_instructions": {"text": "test"}, "capability_profile": "workspace_worker"
        }
    });
    let event = serde_json::json!({
        "timestamp": "2026-10-04T08:00:00Z", "type": "event_msg",
        "payload": {"type": "user_message", "message": "retained history"}
    });
    let journal = format!("{metadata}\n{event}\n");
    let contents: [&[u8]; 6] = [
        journal.as_bytes(),
        b"staged patch",
        b"working patch",
        b"dirty work",
        b"untracked",
        b"provider state",
    ];
    let entry = |path: &str, bytes: &[u8]| WorkspaceEntry {
        path: path.into(),
        kind: WorkspaceEntryKind::File,
        artifact: reference(bytes),
        executable: false,
    };
    let manifest = CheckpointManifest {
        version: 2,
        sequence: 4,
        session: SessionManifest {
            version: 1,
            scope_id: "scope".into(),
            session_id: session_id.into(),
            harness: "codex".into(),
            harness_version: "1".into(),
            model_route_id: "route".into(),
            gateway_account_id: "account".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::new(),
            credential_references: BTreeSet::new(),
        },
        workspace_state: GitWorkspaceState {
            base_commit: "a".repeat(40),
            index_patch: reference(contents[1]),
            worktree_patch: reference(contents[2]),
            required_untracked: vec![entry("notes/new.txt", contents[4])],
            deleted_paths: BTreeSet::new(),
        },
        history: vec![reference(contents[0])],
        attachments: vec![],
        workspace: vec![entry("nested/work.txt", contents[3])],
        provider_state: vec![entry("state.json", contents[5])],
        pending_effects: vec![],
    };
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let digest = reference(&manifest_bytes).sha256;
    let mut ids = BTreeMap::new();
    let mut jobs = BTreeMap::new();
    for (index, bytes) in contents.iter().enumerate() {
        let id = format!("artifact{index}");
        let target = if foreign_artifact && index == 0 {
            "another-enrollment"
        } else {
            "enrolled-source"
        };
        ids.insert(reference(bytes).sha256, id.clone());
        jobs.insert(id.clone(), (job(&id, bytes, target), bytes.to_vec()));
    }
    jobs.insert(
        "manifest".into(),
        (
            job("manifest", &manifest_bytes, "enrolled-source"),
            manifest_bytes,
        ),
    );
    for (request, _) in jobs.values() {
        transfers.enqueue(request.clone()).unwrap();
    }
    let peer = Arc::new(MemoryPeer {
        jobs,
        allowed: AtomicBool::new(true),
        checks: AtomicUsize::new(0),
        revoke_at: AtomicUsize::new(usize::MAX),
    });
    let worker = transfers.worker_with_peer(peer.clone()).unwrap();
    while worker.run_next(&AtomicBool::new(false)).await.unwrap() {}
    drop(worker);
    peer.checks.store(0, Ordering::SeqCst);
    Fixture {
        root,
        transfers,
        checkpoints,
        peer,
        digest,
        ids,
    }
}

#[tokio::test]
async fn checkpoint_ingests_real_completed_jobs_and_preserves_separate_state() {
    let f = fixture(false).await;
    let originals =
        ingest_transferred_checkpoint(&f.transfers, &f.checkpoints, &*f.peer, &f.request())
            .await
            .unwrap();
    assert_eq!(originals.len(), 7);
    let restored = f.root.path().join("restored");
    f.checkpoints.restore(&f.digest, &restored).unwrap();
    let history = &f.checkpoints.load(&f.digest).unwrap().history[0];
    assert_eq!(
        std::fs::read(restored.join("history").join(&history.sha256)).unwrap(),
        f.peer.jobs["artifact0"].1
    );
    for (path, expected) in [
        ("git/index.patch", "staged patch"),
        ("git/worktree.patch", "working patch"),
        ("workspace/nested/work.txt", "dirty work"),
        ("workspace/notes/new.txt", "untracked"),
        ("provider/state.json", "provider state"),
    ] {
        assert_eq!(
            std::fs::read_to_string(restored.join(path)).unwrap(),
            expected
        );
    }
    assert!(originals.iter().all(|request| request
        .peer_source
        .as_ref()
        .unwrap()
        .account_binding
        .as_ref()
        .unwrap()
        .grant_id
        == format!("grant-{}", request.id)));
}

#[tokio::test]
async fn completed_cache_does_not_bypass_revocation() {
    let f = fixture(false).await;
    f.peer.allowed.store(false, Ordering::SeqCst);
    assert!(
        ingest_transferred_checkpoint(&f.transfers, &f.checkpoints, &*f.peer, &f.request())
            .await
            .is_err()
    );
    assert!(f.checkpoints.load(&f.digest).is_err());
}

#[tokio::test]
async fn account_change_between_manifest_checks_blocks_publication() {
    let f = fixture(false).await;
    f.peer.revoke_at.store(2, Ordering::SeqCst);
    assert!(
        ingest_transferred_checkpoint(&f.transfers, &f.checkpoints, &*f.peer, &f.request())
            .await
            .is_err()
    );
    assert!(f.checkpoints.load(&f.digest).is_err());
}

#[tokio::test]
async fn individually_authorized_foreign_account_cannot_join_checkpoint() {
    let f = fixture(true).await;
    assert!(
        ingest_transferred_checkpoint(&f.transfers, &f.checkpoints, &*f.peer, &f.request())
            .await
            .is_err()
    );
    assert!(f.checkpoints.load(&f.digest).is_err());
}

#[tokio::test]
async fn extra_or_missing_artifact_jobs_are_rejected() {
    let mut f = fixture(false).await;
    let missing = f.ids.keys().next().unwrap().clone();
    f.ids.remove(&missing);
    assert!(
        ingest_transferred_checkpoint(&f.transfers, &f.checkpoints, &*f.peer, &f.request())
            .await
            .is_err()
    );
    f.ids.insert("f".repeat(64), "extra".into());
    assert!(
        ingest_transferred_checkpoint(&f.transfers, &f.checkpoints, &*f.peer, &f.request())
            .await
            .is_err()
    );
    assert!(f.checkpoints.load(&f.digest).is_err());
}

#[tokio::test]
async fn tampered_completed_bytes_and_receipt_paths_are_rejected() {
    let f = fixture(false).await;
    let manifest = f.transfers.get("manifest").unwrap();
    let path = f
        .root
        .path()
        .join("transfers")
        .join(manifest.receipt.as_ref().unwrap().artifact.clone());
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, vec![b'x'; bytes.len()]).unwrap();
    assert!(
        ingest_transferred_checkpoint(&f.transfers, &f.checkpoints, &*f.peer, &f.request())
            .await
            .is_err()
    );
    std::fs::write(&path, bytes).unwrap();
    let mut receipt = manifest.receipt.unwrap();
    receipt.artifact = "../unrelated".into();
    let db = rusqlite::Connection::open(f.root.path().join("jobs.sqlite3")).unwrap();
    db.execute(
        "UPDATE ctox_transfer_jobs SET receipt=?1 WHERE id='manifest'",
        [serde_json::to_string(&receipt).unwrap()],
    )
    .unwrap();
    assert!(
        ingest_transferred_checkpoint(&f.transfers, &f.checkpoints, &*f.peer, &f.request())
            .await
            .is_err()
    );
    assert!(f.checkpoints.load(&f.digest).is_err());
}

#[tokio::test]
async fn protected_digest_must_match_the_original_manifest_job() {
    let f = fixture(false).await;
    let digest = "f".repeat(64);
    let mut request = f.request();
    request.checkpoint_digest = &digest;
    assert!(
        ingest_transferred_checkpoint(&f.transfers, &f.checkpoints, &*f.peer, &request)
            .await
            .is_err()
    );
    assert_eq!(f.peer.checks.load(Ordering::SeqCst), 0);
}
