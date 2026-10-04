impl GuestReadinessOwner for TestOwner {
    fn with_live_guest(
        &self,
        _: &GuestImportReceipt,
        _: &mut dyn FnMut(GuestReadyObservation) -> io::Result<()>,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no registered guest process",
        ))
    }
}

/// Component lifecycle owner. It deliberately does not claim a real QEMU child.
struct LiveTestOwner {
    inner: Arc<TestOwner>,
    effect: GuestProcessEffect,
    endpoint: GuestLiveEndpoint,
}
impl GuestRestoreOwner for LiveTestOwner {
    fn resolve_destination(
        &self,
        guest: &str,
        spec: &ExecutionSpec,
        ownership: &Ownership,
    ) -> io::Result<GuestRestoreDestination> {
        self.inner.resolve_destination(guest, spec, ownership)
    }
    fn with_current_fence(
        &self,
        expected: &GuestRestoreDestination,
        spec: &ExecutionSpec,
        ownership: &Ownership,
        publish: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        self.inner
            .with_current_fence(expected, spec, ownership, publish)
    }
}
impl GuestReadinessOwner for LiveTestOwner {
    fn with_live_guest(
        &self,
        imported: &GuestImportReceipt,
        publish: &mut dyn FnMut(GuestReadyObservation) -> io::Result<()>,
    ) -> io::Result<()> {
        self.inner.with_current_fence(
            &imported.destination,
            &imported.spec,
            &imported.ownership,
            &mut || {
                publish(GuestReadyObservation {
                    endpoint: self.endpoint.clone(),
                    process_effect: self.effect.clone(),
                })
            },
        )
    }
}
async fn registered_live_process(f: &Fixture) -> (GuestImportReceipt, LiveTestOwner) {
    let receipt = commit_guest_restore(&f.authority, &*f.authority.owner, stage(f).await.unwrap())
        .await
        .unwrap();
    let effect = GuestProcessEffect {
        effect_id: component_process_effect_id(),
        job_id: receipt.spec.job_id.clone(),
        ownership: receipt.ownership.clone(),
        controller_id: receipt.destination.controller_id.clone(),
        controller_generation: receipt.destination.controller_generation,
        process_instance_id: "registered-native-child".into(),
    };
    let result = f
        .authority
        .submit(Request {
            request_id: "native-process-begin".into(),
            actor: 1,
            command: Command::BeginEffect {
                job_id: effect.job_id.clone(),
                ownership: effect.ownership.clone(),
                effect_id: effect.effect_id.clone(),
            },
        })
        .await
        .unwrap();
    assert!(matches!(result, Receipt::Applied(_)));
    let owner = LiveTestOwner {
        inner: f.authority.owner.clone(),
        endpoint: GuestLiveEndpoint {
            process_instance_id: effect.process_instance_id.clone(),
            guest_session_id: "actual-component-guest-session".into(),
            endpoint_id: "actual-component-endpoint".into(),
        },
        effect,
    };
    (receipt, owner)
}
fn component_process_effect_id() -> String {
    // Distinct native-generated fixture effect; never the import effect.
    format!(
        "native-process-{:x}",
        Sha256::digest(b"component child lifetime")
    )
}

#[tokio::test]
async fn readiness_retains_registered_process_effect_and_denies_takeover() {
    let f = fixture();
    let (imported, owner) = registered_live_process(&f).await;
    // The alternative executor is eligible for this protected checkpoint.
    // This remains a deterministic authority fixture, not signed-copy/quorum proof.
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
        .replicas
        .insert(2);
    let ready = confirm_guest_ready(&f.authority, &owner, imported.clone())
        .await
        .unwrap();
    assert_eq!(ready.process_effect, owner.effect);
    let pending = f.authority.state.lock().unwrap().jobs["job"]
        .pending_effects
        .clone();
    assert_eq!(pending, BTreeSet::from([owner.effect.effect_id.clone()]));
    assert!(stage(&f).await.is_err());
    let takeover = |id: &str| Request {
        request_id: id.into(),
        actor: 2,
        command: Command::TakeOver {
            job_id: imported.spec.job_id.clone(),
            expected: imported.ownership.clone(),
            checkpoint_digest: imported.checkpoint_digest.clone(),
            owner: 2,
        },
    };
    let blocked = f
        .authority
        .submit(takeover("takeover-with-live-child"))
        .await
        .unwrap();
    assert!(
        matches!(
            blocked,
            Receipt::Rejected(crate::authority::Rejection::ReconciliationRequired)
        ),
        "eligible distinct executor must be denied by the pending effect: {blocked:?}"
    );
    assert_eq!(
        f.authority.state.lock().unwrap().jobs["job"].pending_effects,
        pending
    );
    // Simulate the lifecycle owner's already-proven exact stop. Production must
    // obtain that proof from the actual retained child, never from QMP/readiness.
    let completed = f
        .authority
        .submit(Request {
            request_id: "confirmed-component-child-stop".into(),
            actor: 1,
            command: Command::CompleteEffect {
                job_id: imported.spec.job_id.clone(),
                ownership: imported.ownership.clone(),
                effect_id: owner.effect.effect_id.clone(),
            },
        })
        .await
        .unwrap();
    assert!(matches!(completed, Receipt::Applied(_)));
    let moved = f
        .authority
        .submit(takeover("takeover-after-confirmed-stop"))
        .await
        .unwrap();
    let Receipt::Applied(job) = moved else {
        panic!("eligible takeover must proceed only after stop");
    };
    assert_eq!(job.ownership.node_id, 2);
    assert_eq!(job.ownership.generation, imported.ownership.generation + 1);
    assert!(confirm_guest_ready(&f.authority, &owner, imported)
        .await
        .is_err());
}

#[tokio::test]
async fn readiness_rejects_missing_completed_foreign_and_extra_process_effects() {
    for case in 0..4 {
        let f = fixture();
        let (imported, mut owner) = registered_live_process(&f).await;
        match case {
            0 => {
                f.authority
                    .state
                    .lock()
                    .unwrap()
                    .jobs
                    .get_mut("job")
                    .unwrap()
                    .pending_effects
                    .clear();
            }
            1 => {
                let reply = f
                    .authority
                    .submit(Request {
                        request_id: "premature-process-complete".into(),
                        actor: 1,
                        command: Command::CompleteEffect {
                            job_id: imported.spec.job_id.clone(),
                            ownership: imported.ownership.clone(),
                            effect_id: owner.effect.effect_id.clone(),
                        },
                    })
                    .await
                    .unwrap();
                assert!(matches!(reply, Receipt::Applied(_)));
            }
            2 => {
                owner.effect.effect_id = "foreign-pending-effect".into();
            }
            _ => {
                f.authority
                    .state
                    .lock()
                    .unwrap()
                    .jobs
                    .get_mut("job")
                    .unwrap()
                    .pending_effects
                    .insert("unknown-old-effect".into());
            }
        }
        assert!(
            confirm_guest_ready(&f.authority, &owner, imported)
                .await
                .is_err(),
            "case {case} must not produce readiness"
        );
    }
}

#[tokio::test]
async fn readiness_rejects_foreign_process_controller_and_execution_registration() {
    for case in 0..6 {
        let f = fixture();
        let (imported, mut owner) = registered_live_process(&f).await;
        match case {
            0 => owner.effect.job_id = "foreign-job".into(),
            1 => owner.effect.ownership.generation += 1,
            2 => owner.effect.controller_id = "foreign-controller".into(),
            3 => owner.effect.controller_generation += 1,
            4 => owner.effect.process_instance_id = "foreign-child".into(),
            _ => owner.effect.effect_id = imported.effect_id.clone(),
        }
        assert!(
            confirm_guest_ready(&f.authority, &owner, imported)
                .await
                .is_err(),
            "case {case} must not produce readiness"
        );
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

#[tokio::test]
async fn cancelled_admission_retains_unknown_effect_without_publication() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    let private_path = staged.staging.path().to_owned();
    *f.authority.hold_begin.lock().unwrap() = true;
    assert!(tokio::time::timeout(
        std::time::Duration::from_millis(20),
        commit_guest_restore(&f.authority, &*f.authority.owner, staged),
    )
    .await
    .is_err());
    assert!(!private_path.exists());
    assert!(imports(&f).is_empty());
    assert_eq!(
        f.authority.state.lock().unwrap().jobs["job"]
            .pending_effects
            .len(),
        1
    );
    assert!(stage(&f).await.is_err());
}

#[tokio::test]
async fn uncertain_completion_preserves_published_state_and_pending_effect() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    *f.authority.fail_complete.lock().unwrap() = true;
    assert!(
        commit_guest_restore(&f.authority, &*f.authority.owner, staged)
            .await
            .is_err()
    );
    let published = imports(&f);
    assert_eq!(published.len(), 1);
    assert_eq!(
        fs::read(published[0].join("workspace/nested/work.txt")).unwrap(),
        b"retained work"
    );
    let state = f.authority.state.lock().unwrap();
    assert_eq!(state.jobs["job"].pending_effects.len(), 1);
    assert!(state.jobs["job"].completed_effects.is_empty());
}

#[tokio::test]
async fn existing_destination_is_never_replaced() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    let identity = format!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}",
        staged.spec.job_id,
        staged.destination.instance_id,
        staged.destination.guest_id,
        staged.destination.controller_id,
        staged.destination.controller_generation,
        staged.ownership.generation,
        staged.digest
    );
    let effect_id = format!("guest-import:{:x}", Sha256::digest(identity.as_bytes()));
    let target = staged
        .destination
        .import_parent
        .join(format!("import-{:x}", Sha256::digest(effect_id.as_bytes())));
    fs::create_dir(&target).unwrap();
    fs::write(target.join("user-content"), b"preserve").unwrap();
    assert!(
        commit_guest_restore(&f.authority, &*f.authority.owner, staged)
            .await
            .is_err()
    );
    assert_eq!(fs::read(target.join("user-content")).unwrap(), b"preserve");
    assert!(!target.join("workspace").exists());
    assert_eq!(
        f.authority.state.lock().unwrap().jobs["job"]
            .pending_effects
            .len(),
        1
    );
}

#[tokio::test]
async fn staged_payload_mutations_deny_publication_and_retain_pending_effect() {
    for mutation in 0..8 {
        let f = fixture();
        let staged = stage(&f).await.unwrap();
        let root = staged.staged_directory();
        let work = root.join("workspace/nested/work.txt");
        match mutation {
            0 => fs::write(&work, b"changed work").unwrap(),
            1 => fs::write(root.join("workspace/extra"), b"unexpected").unwrap(),
            2 => fs::create_dir(root.join("workspace/extra")).unwrap(),
            3 => fs::remove_file(&work).unwrap(),
            4 => fs::remove_dir(root.join("attachments")).unwrap(),
            5 => {
                fs::remove_file(&work).unwrap();
                std::os::unix::fs::symlink("../../../outside", &work).unwrap();
            }
            6 => fs::set_permissions(&work, fs::Permissions::from_mode(0o700)).unwrap(),
            7 => fs::write(root.join("checkpoint.json"), b"{}").unwrap(),
            _ => unreachable!(),
        }
        let error = commit_guest_restore(&f.authority, &*f.authority.owner, staged)
            .await
            .unwrap_err();
        assert_eq!(
            error.kind(),
            io::ErrorKind::PermissionDenied,
            "mutation {mutation}"
        );
        assert!(imports(&f).is_empty(), "mutation {mutation}");
        let state = f.authority.state.lock().unwrap();
        assert_eq!(
            state.jobs["job"].pending_effects.len(),
            1,
            "mutation {mutation}"
        );
        assert!(
            state.jobs["job"].completed_effects.is_empty(),
            "mutation {mutation}"
        );
    }
}

#[tokio::test]
async fn checkpoint_advanced_after_staging_denies_before_effect_admission() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
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
        .sequence += 1;
    assert!(
        commit_guest_restore(&f.authority, &*f.authority.owner, staged)
            .await
            .is_err()
    );
    assert!(imports(&f).is_empty());
    assert_eq!(*f.authority.owner.calls.lock().unwrap(), 0);
    assert!(f.authority.state.lock().unwrap().jobs["job"]
        .pending_effects
        .is_empty());
}

#[tokio::test]
async fn changed_checkpoint_in_admission_reply_denies_publication() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    *f.authority.changed_checkpoint_reply.lock().unwrap() = true;
    assert!(
        commit_guest_restore(&f.authority, &*f.authority.owner, staged)
            .await
            .is_err()
    );
    assert!(imports(&f).is_empty());
    assert_eq!(*f.authority.owner.calls.lock().unwrap(), 0);
    let state = f.authority.state.lock().unwrap();
    assert_eq!(state.jobs["job"].pending_effects.len(), 1);
    assert!(state.jobs["job"].completed_effects.is_empty());
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
    hold_begin: Mutex<bool>,
    fail_complete: Mutex<bool>,
    changed_checkpoint_reply: Mutex<bool>,
    replace_parent_on_begin: Mutex<bool>,
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
        if matches!(request.command, Command::CompleteEffect { .. })
            && *self.fail_complete.lock().unwrap()
        {
            return Err(io::Error::other("unknown completion outcome"));
        }
        let peers = BTreeMap::from([
            (
                1,
                Peer {
                    identity: "fixture".into(),
                    executor: true,
                    data_replica: true,
                },
            ),
            (
                2,
                Peer {
                    identity: "fixture-target".into(),
                    executor: true,
                    data_replica: true,
                },
            ),
        ]);
        let mut receipt = self.state.lock().unwrap().apply(&request, &peers);
        if is_begin && *self.changed_checkpoint_reply.lock().unwrap() {
            if let Receipt::Applied(job) = &mut receipt {
                job.checkpoint.as_mut().unwrap().sequence += 1;
            }
        }
        if is_begin && *self.revoke_on_begin.lock().unwrap() {
            self.owner.binding.lock().unwrap().controller_generation += 1;
        }
        if is_begin && *self.replace_parent_on_begin.lock().unwrap() {
            replace_import_parent(&self.owner.binding.lock().unwrap().import_parent);
        }
        let hold = is_begin && *self.hold_begin.lock().unwrap();
        if hold {
            std::future::pending::<()>().await;
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
    let metadata = serde_json::json!({
        "timestamp": "2026-10-04T10:00:00Z", "type": "session_meta",
        "payload": {
            "id": "00000000-0000-0000-0000-000000000001",
            "timestamp": "2026-10-04T10:00:00Z", "cwd": "/original/workspace",
            "originator": "codex_cli_rs", "cli_version": "1.0.0", "source": "exec",
            "model_provider": "test-provider", "base_instructions": {"text": "test"},
            "capability_profile": "workspace_worker"
        }
    });
    let event = serde_json::json!({
        "timestamp": "2026-10-04T10:00:00Z", "type": "event_msg",
        "payload": {"type": "user_message", "message": "ready"}
    });
    let journal = format!("{metadata}\n{event}\n");
    let history = artifact(journal.as_bytes());
    let empty = artifact(b"");
    let state_blob = artifact(b"portable provider state");
    let file = artifact(b"retained work");
    let manifest = CheckpointManifest {
        version: 2,
        session: SessionManifest {
            version: 1,
            scope_id: "scope".into(),
            session_id: "00000000-0000-0000-0000-000000000001".into(),
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
        session_id: "00000000-0000-0000-0000-000000000001".into(),
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
            hold_begin: Mutex::new(false),
            fail_complete: Mutex::new(false),
            changed_checkpoint_reply: Mutex::new(false),
            replace_parent_on_begin: Mutex::new(false),
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

fn replace_import_parent(parent: &Path) {
    let retired = parent.with_file_name("retired-guests");
    fs::rename(parent, &retired).unwrap();
    fs::create_dir(parent).unwrap();
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
    // Preserve the exact checkpoint bytes and staging pathname, so content and
    // pathname checks alone would wrongly accept the new native directory.
    for entry in fs::read_dir(retired).unwrap() {
        let entry = entry.unwrap();
        fs::rename(entry.path(), parent.join(entry.file_name())).unwrap();
    }
}

#[tokio::test]
async fn replaced_native_directory_before_admission_has_no_effect_or_publication() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    replace_import_parent(&staged.destination.import_parent);
    let error = commit_guest_restore(&f.authority, &*f.authority.owner, staged)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(imports(&f).is_empty());
    assert_eq!(*f.authority.owner.calls.lock().unwrap(), 0);
    assert!(f.authority.state.lock().unwrap().jobs["job"]
        .pending_effects
        .is_empty());
}

#[tokio::test]
async fn replaced_native_directory_during_admission_preserves_unknown_effect() {
    let f = fixture();
    let staged = stage(&f).await.unwrap();
    *f.authority.replace_parent_on_begin.lock().unwrap() = true;
    let error = commit_guest_restore(&f.authority, &*f.authority.owner, staged)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(imports(&f).is_empty());
    let state = f.authority.state.lock().unwrap();
    assert_eq!(state.jobs["job"].pending_effects.len(), 1);
    assert!(state.jobs["job"].completed_effects.is_empty());
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
