impl GuestReadinessOwner for TestOwner {
    fn with_live_guest(
        &self,
        _: &GuestImportReceipt,
        _: &mut dyn FnMut(GuestLiveEndpoint) -> io::Result<()>,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no registered guest process",
        ))
    }
}

#[tokio::test]
async fn successful_import_does_not_synthesize_guest_readiness() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    let receipt = commit_guest_restore(&f.authority, &*f.authority.owner, staged)
        .await
        .unwrap();
    let error = confirm_guest_ready(&f.authority, &*f.authority.owner, receipt)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
}

use super::*;
use crate::{
    authority::{Peer, ProtectedCheckpoint, State, WorkerMembership},
    contracts::{
        ArtifactRef, GitWorkspaceState, SessionManifest, WorkspaceEntry, WorkspaceEntryKind,
    },
};
use async_trait::async_trait;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

struct NativeFixture {
    state: Mutex<State>,
    owner: Arc<TestOwner>,
    revoke_on_begin: Mutex<bool>,
}
#[async_trait]
impl ExecutionAuthority for NativeFixture {
    fn node_id(&self) -> u64 {
        1
    }
    fn scope_id(&self) -> &str {
        "scope"
    }
    async fn worker_membership(&self, _: u64) -> io::Result<Option<WorkerMembership>> {
        Ok(None)
    }
    async fn shutdown(&self) -> io::Result<()> {
        Ok(())
    }
    async fn validate_ownership(&self, id: &str, expected: &Ownership) -> io::Result<Job> {
        let state = self.state.lock().unwrap();
        let job = state.jobs.get(id).ok_or_else(|| denied("unknown job"))?;
        if job.ownership != *expected || job.stopped {
            return Err(denied("stale execution"));
        }
        Ok(job.clone())
    }
    async fn submit(&self, request: Request) -> io::Result<Receipt> {
        let is_begin = matches!(request.command, Command::BeginEffect { .. });
        let peers = BTreeMap::from([(
            1,
            Peer {
                identity: "fixture".into(),
                executor: true,
                data_replica: true,
            },
        )]);
        let receipt = self.state.lock().unwrap().apply(&request, &peers);
        if is_begin && *self.revoke_on_begin.lock().unwrap() {
            self.owner.binding.lock().unwrap().controller_generation += 1;
        }
        Ok(receipt)
    }
}
// This is a component fixture. Production must supply the native VM/Crew guard;
// it is NOT proof of a provisioned guest or an installed service adapter.
struct TestOwner {
    binding: Mutex<GuestRestoreDestination>,
    calls: Mutex<usize>,
    omit_publication: Mutex<bool>,
}
impl GuestRestoreOwner for TestOwner {
    fn resolve_destination(
        &self,
        guest: &str,
        _: &ExecutionSpec,
        _: &Ownership,
    ) -> io::Result<GuestRestoreDestination> {
        let binding = self.binding.lock().unwrap();
        if binding.guest_id != guest {
            return Err(denied("foreign guest"));
        }
        Ok(binding.clone())
    }
    fn with_current_fence(
        &self,
        expected: &GuestRestoreDestination,
        _: &ExecutionSpec,
        _: &Ownership,
        publish: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        let binding = self.binding.lock().unwrap();
        if *binding != *expected {
            return Err(denied("revoked controller"));
        }
        *self.calls.lock().unwrap() += 1;
        if *self.omit_publication.lock().unwrap() {
            return Ok(());
        }
        publish()
    }
}
struct Fixture {
    _root: tempfile::TempDir,
    store: CheckpointStore,
    digest: String,
    authority: NativeFixture,
}
fn fixture() -> Fixture {
    let root = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let parent = root.path().join("guests");
    fs::create_dir(&parent).unwrap();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
    let store = CheckpointStore::open(root.path().join("checkpoints"), 1024 * 1024).unwrap();
    let artifact = |data: &[u8]| {
        let reference = ArtifactRef {
            sha256: format!("{:x}", Sha256::digest(data)),
            size_bytes: data.len() as u64,
        };
        store.ingest_blob(&reference, data).unwrap();
        reference
    };
    let history = artifact(b"history");
    let empty = artifact(b"");
    let state_blob = artifact(b"portable provider state");
    let file = artifact(b"retained work");
    let manifest = CheckpointManifest {
        version: 2,
        session: SessionManifest {
            version: 1,
            scope_id: "scope".into(),
            session_id: "session".into(),
            harness: "codex".into(),
            harness_version: "1".into(),
            model_route_id: "route".into(),
            gateway_account_id: "account".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::new(),
            credential_references: BTreeSet::new(),
        },
        sequence: 4,
        workspace_state: GitWorkspaceState {
            base_commit: "a".repeat(40),
            index_patch: empty.clone(),
            worktree_patch: empty,
            required_untracked: vec![],
            deleted_paths: BTreeSet::new(),
        },
        history: vec![history],
        attachments: vec![],
        workspace: vec![WorkspaceEntry {
            path: "nested/work.txt".into(),
            kind: WorkspaceEntryKind::File,
            artifact: file,
            executable: false,
        }],
        provider_state: vec![WorkspaceEntry {
            path: "state.json".into(),
            kind: WorkspaceEntryKind::File,
            artifact: state_blob,
            executable: false,
        }],
        pending_effects: vec![],
    };
    let digest = store.publish(&manifest).unwrap();
    let spec = ExecutionSpec {
        job_id: "job".into(),
        session_id: "session".into(),
        scope_id: "scope".into(),
        harness: "codex".into(),
        harness_version: "1".into(),
        model_route_id: "route".into(),
        gateway_account_id: "account".into(),
        model_id: "model".into(),
        required_capabilities: BTreeSet::new(),
    };
    let ownership = Ownership {
        node_id: 1,
        generation: 3,
    };
    let job = Job {
        spec,
        ownership,
        checkpoint: Some(ProtectedCheckpoint {
            digest: digest.clone(),
            sequence: 4,
            replicas: BTreeSet::from([1, 2]),
            receipts: vec![],
        }),
        pending_effects: BTreeSet::new(),
        completed_effects: BTreeSet::new(),
        stopped: false,
    };
    let mut state = State::default();
    state.jobs.insert("job".into(), job);
    let owner = Arc::new(TestOwner {
        binding: Mutex::new(GuestRestoreDestination {
            instance_id: "instance".into(),
            guest_id: "guest".into(),
            human_owner_id: "human".into(),
            project_id: "project".into(),
            thread_id: "thread".into(),
            worker_profile_id: "profile".into(),
            controller_id: "controller".into(),
            controller_generation: 2,
            import_parent: fs::canonicalize(parent).unwrap(),
        }),
        calls: Mutex::new(0),
        omit_publication: Mutex::new(false),
    });
    Fixture {
        _root: root,
        store,
        digest,
        authority: NativeFixture {
            state: Mutex::new(state),
            owner,
            revoke_on_begin: Mutex::new(false),
        },
    }
}
async fn stage(f: &Fixture) -> io::Result<StagedGuestRestore> {
    stage_guest_restore(
        &f.store,
        &f.authority,
        &*f.authority.owner,
        "guest",
        "job",
        Ownership {
            node_id: 1,
            generation: 3,
        },
        &f.digest,
    )
    .await
}
fn imports(f: &Fixture) -> Vec<PathBuf> {
    fs::read_dir(&f.authority.owner.binding.lock().unwrap().import_parent)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("import-")
        })
        .collect()
}

#[tokio::test]
async fn verified_import_is_private_until_fenced_commit() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    assert!(imports(&f).is_empty());
    assert_eq!(
        fs::read(staged.staged_directory().join("workspace/nested/work.txt")).unwrap(),
        b"retained work"
    );
    let receipt = commit_guest_restore(&f.authority, &*f.authority.owner, staged)
        .await
        .unwrap();
    assert_eq!(receipt.ownership.generation, 3);
    assert_eq!(receipt.destination.controller_generation, 2);
    assert_eq!(receipt.sequence, 4);
    assert_eq!(receipt.checkpoint_digest, f.digest);
    assert_eq!(imports(&f), vec![receipt.imported_directory.clone()]);
    assert_eq!(
        fs::read(receipt.imported_directory.join("workspace/nested/work.txt")).unwrap(),
        b"retained work"
    );
    let state = f.authority.state.lock().unwrap();
    let job = &state.jobs["job"];
    assert!(job.pending_effects.is_empty());
    assert!(job.completed_effects.contains(&receipt.effect_id));
}

#[tokio::test]
async fn controller_change_during_effect_admission_denies_publication_and_retains_pending() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    *f.authority.revoke_on_begin.lock().unwrap() = true;
    assert!(
        commit_guest_restore(&f.authority, &*f.authority.owner, staged)
            .await
            .is_err()
    );
    assert!(imports(&f).is_empty());
    assert_eq!(*f.authority.owner.calls.lock().unwrap(), 0);
    assert_eq!(
        f.authority.state.lock().unwrap().jobs["job"]
            .pending_effects
            .len(),
        1
    );
    assert!(stage(&f).await.is_err()); // no automatic second import
}

#[tokio::test]
async fn ownership_change_after_staging_has_no_effect_or_publication() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    f.authority
        .state
        .lock()
        .unwrap()
        .jobs
        .get_mut("job")
        .unwrap()
        .ownership
        .generation += 1;
    assert!(
        commit_guest_restore(&f.authority, &*f.authority.owner, staged)
            .await
            .is_err()
    );
    assert!(imports(&f).is_empty());
    assert!(f.authority.state.lock().unwrap().jobs["job"]
        .pending_effects
        .is_empty());
}

#[tokio::test]
async fn wrong_session_or_checkpoint_sequence_is_rejected_before_staging() {
    let f = fixture();
    f.authority
        .state
        .lock()
        .unwrap()
        .jobs
        .get_mut("job")
        .unwrap()
        .spec
        .gateway_account_id = "foreign".into();
    assert!(stage(&f).await.is_err());
    f.authority
        .state
        .lock()
        .unwrap()
        .jobs
        .get_mut("job")
        .unwrap()
        .spec
        .gateway_account_id = "account".into();
    f.authority
        .state
        .lock()
        .unwrap()
        .jobs
        .get_mut("job")
        .unwrap()
        .checkpoint
        .as_mut()
        .unwrap()
        .sequence = 5;
    assert!(stage(&f).await.is_err());
    assert!(imports(&f).is_empty());
}

#[tokio::test]
async fn missing_native_publication_never_completes_the_effect() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    *f.authority.owner.omit_publication.lock().unwrap() = true;
    assert!(
        commit_guest_restore(&f.authority, &*f.authority.owner, staged)
            .await
            .is_err()
    );
    assert!(imports(&f).is_empty());
    let state = f.authority.state.lock().unwrap();
    assert_eq!(state.jobs["job"].pending_effects.len(), 1);
    assert!(state.jobs["job"].completed_effects.is_empty());
}

#[tokio::test]
async fn dropped_stage_cleans_only_its_unpublished_files() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    let staging_path = staged.staging.path().to_owned();
    drop(staged);
    assert!(!staging_path.exists());
    assert!(f.store.load(&f.digest).is_ok());
    assert!(f.authority.state.lock().unwrap().jobs["job"]
        .pending_effects
        .is_empty());
}

#[tokio::test]
async fn corrupted_blob_has_no_effect_or_guest_publication() {
    let f = fixture();
    let manifest = f.store.load(&f.digest).unwrap();
    fs::write(
        f._root
            .path()
            .join("checkpoints/blobs")
            .join(&manifest.workspace[0].artifact.sha256),
        b"corrupt",
    )
    .unwrap();
    assert!(stage(&f).await.is_err());
    assert!(imports(&f).is_empty());
    assert!(f.authority.state.lock().unwrap().jobs["job"]
        .pending_effects
        .is_empty());
}

#[tokio::test]
async fn pending_external_effect_blocks_guest_staging() {
    let f = fixture();
    f.authority
        .state
        .lock()
        .unwrap()
        .jobs
        .get_mut("job")
        .unwrap()
        .pending_effects
        .insert("unknown".into());
    assert!(stage(&f).await.is_err());
    assert!(imports(&f).is_empty());
}

#[tokio::test]
async fn repeated_completed_import_cannot_replay_publication() {
    let f = fixture();
    let first = stage(&f).await.unwrap();
    let receipt = commit_guest_restore(&f.authority, &*f.authority.owner, first)
        .await
        .unwrap();
    let second = stage(&f).await.unwrap();
    assert!(
        commit_guest_restore(&f.authority, &*f.authority.owner, second)
            .await
            .is_err()
    );
    assert_eq!(*f.authority.owner.calls.lock().unwrap(), 1);
    assert_eq!(imports(&f), vec![receipt.imported_directory.clone()]);
    assert_eq!(
        fs::read(receipt.imported_directory.join("workspace/nested/work.txt")).unwrap(),
        b"retained work"
    );
}
