use ctox_sync::{checkpoint::CheckpointStore, contracts::*};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Cursor};
fn blob(bytes: &[u8]) -> ArtifactRef {
    ArtifactRef {
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size_bytes: bytes.len() as u64,
    }
}
fn manifest() -> CheckpointManifest {
    CheckpointManifest {
        version: 2,
        sequence: 1,
        session: SessionManifest {
            version: 1,
            scope_id: "scope".into(),
            session_id: "session".into(),
            harness: "codex".into(),
            harness_version: "pinned".into(),
            model_route_id: "route".into(),
            gateway_account_id: "account-reference".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::new(),
            credential_references: ["gateway-account".into()].into_iter().collect(),
        },
        workspace_state: GitWorkspaceState {
            base_commit: "a".repeat(40),
            index_patch: blob(b"index patch"),
            worktree_patch: blob(b"worktree patch"),
            required_untracked: vec![WorkspaceEntry {
                path: "config/local.json".into(),
                kind: WorkspaceEntryKind::File,
                artifact: blob(b"required untracked"),
                executable: false,
            }],
            deleted_paths: ["src/removed.rs".into()].into_iter().collect(),
        },
        history: vec![blob(b"complete journal")],
        attachments: vec![blob(b"attachment")],
        workspace: vec![WorkspaceEntry {
            path: "src/main.rs".into(),
            kind: WorkspaceEntryKind::File,
            artifact: blob(b"uncommitted source"),
            executable: false,
        }],
        provider_state: vec![WorkspaceEntry {
            path: "rollout.jsonl".into(),
            kind: WorkspaceEntryKind::File,
            artifact: blob(b"provider checkpoint"),
            executable: false,
        }],
        pending_effects: vec![],
    }
}
fn populate(store: &CheckpointStore) {
    for bytes in [
        b"complete journal".as_slice(),
        b"attachment",
        b"uncommitted source",
        b"provider checkpoint",
        b"index patch",
        b"worktree patch",
        b"required untracked",
    ] {
        store.ingest_blob(&blob(bytes), Cursor::new(bytes)).unwrap();
    }
}
#[test]
fn full_session_is_verified_and_restored_without_touching_existing_work() {
    let root = tempfile::tempdir().unwrap();
    let store = CheckpointStore::open(root.path().join("store"), 1024).unwrap();
    populate(&store);
    let m = manifest();
    let digest = store.publish(&m).unwrap();
    assert_eq!(store.publish(&m).unwrap(), digest);
    let target = root.path().join("restored");
    assert_eq!(store.restore(&digest, &target).unwrap(), m);
    assert_eq!(
        fs::read(target.join("workspace/src/main.rs")).unwrap(),
        b"uncommitted source"
    );
    assert_eq!(
        fs::read(target.join("provider/rollout.jsonl")).unwrap(),
        b"provider checkpoint"
    );
    assert_eq!(
        fs::read(target.join("workspace/config/local.json")).unwrap(),
        b"required untracked"
    );
    assert_eq!(
        fs::read(target.join("git/index.patch")).unwrap(),
        b"index patch"
    );
    assert_eq!(
        fs::read_to_string(target.join("git/base-commit")).unwrap(),
        format!("{}\n", m.workspace_state.base_commit)
    );
    assert_eq!(
        fs::read(target.join("history").join(&m.history[0].sha256)).unwrap(),
        b"complete journal"
    );
    assert_eq!(
        fs::read(target.join("attachments").join(&m.attachments[0].sha256)).unwrap(),
        b"attachment"
    );
    fs::write(target.join("workspace/src/main.rs"), "new user edit").unwrap();
    assert!(store.restore(&digest, &target).is_err());
    assert_eq!(
        fs::read_to_string(target.join("workspace/src/main.rs")).unwrap(),
        "new user edit"
    );
}
#[test]
fn missing_or_corrupt_copy_cannot_be_advertised_as_protected() {
    let root = tempfile::tempdir().unwrap();
    let store = CheckpointStore::open(root.path().join("store"), 1024).unwrap();
    assert!(store.publish(&manifest()).is_err());
    populate(&store);
    let m = manifest();
    let digest = store.publish(&m).unwrap();
    fs::write(
        root.path().join("store/blobs").join(&m.history[0].sha256),
        "tampered",
    )
    .unwrap();
    assert!(store.load(&digest).is_err());
    assert!(!root.path().join("target").exists());
    assert!(store.restore(&digest, &root.path().join("target")).is_err());
    assert!(!root.path().join("target").exists());
}
#[test]
fn durable_copy_receipt_requires_intact_data_and_matching_execution() {
    use ctox_sync::authority::auth::SigningIdentity;
    let root = tempfile::tempdir().unwrap();
    let store = CheckpointStore::open(root.path().join("store"), 1024).unwrap();
    let key = SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap();
    let m = manifest();
    let spec = ExecutionSpec {
        job_id: "job".into(),
        session_id: m.session.session_id.clone(),
        scope_id: m.session.scope_id.clone(),
        harness: m.session.harness.clone(),
        harness_version: m.session.harness_version.clone(),
        model_route_id: m.session.model_route_id.clone(),
        gateway_account_id: m.session.gateway_account_id.clone(),
        model_id: m.session.model_id.clone(),
        required_capabilities: m.session.required_capabilities.clone(),
    };
    let ownership = ExecutionOwnership {
        node_id: 1,
        generation: 1,
    };
    assert!(key
        .acknowledge_checkpoint(&store, 2, &spec, &ownership, &"a".repeat(64))
        .is_err());
    populate(&store);
    let digest = store.publish(&m).unwrap();
    let receipt = key
        .acknowledge_checkpoint(&store, 2, &spec, &ownership, &digest)
        .unwrap();
    assert_eq!(receipt.checkpoint_digest, digest);
    assert_eq!(receipt.sequence, m.sequence);
    assert_eq!(receipt.node_id, 2);
    for field in [
        "scope",
        "session",
        "harness",
        "version",
        "account",
        "route",
        "model",
        "capabilities",
    ] {
        let mut mismatch = spec.clone();
        match field {
            "scope" => mismatch.scope_id.push_str("-other"),
            "session" => mismatch.session_id.push_str("-other"),
            "harness" => mismatch.harness.push_str("-other"),
            "version" => mismatch.harness_version.push_str("-other"),
            "account" => mismatch.gateway_account_id.push_str("-other"),
            "route" => mismatch.model_route_id.push_str("-other"),
            "model" => mismatch.model_id.push_str("-other"),
            _ => {
                mismatch.required_capabilities.insert("unavailable".into());
            }
        }
        assert!(
            key.acknowledge_checkpoint(&store, 2, &mismatch, &ownership, &digest)
                .is_err(),
            "{field}"
        );
    }
    let mut unresolved = m.clone();
    unresolved.pending_effects.push(PendingEffect {
        effect_id: "unknown-outcome".into(),
        idempotency_key: None,
        description: "requires reconciliation".into(),
    });
    let unresolved_digest = store.publish(&unresolved).unwrap();
    assert!(key
        .acknowledge_checkpoint(&store, 2, &spec, &ownership, &unresolved_digest)
        .is_err());
    let journal = root.path().join("store/blobs").join(&m.history[0].sha256);
    fs::write(&journal, "corrupt").unwrap();
    assert!(key
        .acknowledge_checkpoint(&store, 2, &spec, &ownership, &digest)
        .is_err());
    fs::remove_file(&journal).unwrap();
    assert!(key
        .acknowledge_checkpoint(&store, 2, &spec, &ownership, &digest)
        .is_err());
}

#[test]
fn unsafe_paths_and_unresolved_effects_cannot_resume() {
    let root = tempfile::tempdir().unwrap();
    let store = CheckpointStore::open(root.path().join("store"), 1024).unwrap();
    populate(&store);
    for path in [
        "../outside",
        "/absolute",
        "C:\\outside",
        "nested/../outside",
        "CON.txt",
    ] {
        let mut m = manifest();
        m.workspace[0].path = path.into();
        assert!(store.publish(&m).is_err(), "{path}");
    }
    let mut m = manifest();
    m.pending_effects.push(PendingEffect {
        effect_id: "publish".into(),
        idempotency_key: None,
        description: "unconfirmed remote action".into(),
    });
    let digest = store.publish(&m).unwrap();
    assert!(store.restore(&digest, &root.path().join("target")).is_err());
    assert!(!root.path().join("target").exists());
}
#[test]
fn portable_git_state_rejects_incomplete_or_overlapping_reconstruction_data() {
    let root = tempfile::tempdir().unwrap();
    let store = CheckpointStore::open(root.path().join("store"), 1024).unwrap();
    populate(&store);
    let mut m = manifest();
    m.version = 1;
    assert!(store.publish(&m).is_err());

    let mut m = manifest();
    m.workspace_state.base_commit = "git-base".into();
    assert!(store.publish(&m).is_err());

    let mut m = manifest();
    m.workspace_state.deleted_paths.insert("src/main.rs".into());
    assert!(store.publish(&m).is_err());

    let mut m = manifest();
    m.workspace_state.required_untracked[0].path = "src".into();
    assert!(store.publish(&m).is_err());
}
#[test]
fn invalid_transfer_and_escaping_symlink_leave_no_valid_copy() {
    let root = tempfile::tempdir().unwrap();
    let store = CheckpointStore::open(root.path().join("store"), 1024).unwrap();
    assert!(store
        .ingest_blob(&blob(b"expected"), Cursor::new(b"wrong".as_slice()))
        .is_err());
    populate(&store);
    let mut m = manifest();
    let target = blob(b"../../outside");
    store
        .ingest_blob(&target, Cursor::new(b"../../outside"))
        .unwrap();
    m.workspace.push(WorkspaceEntry {
        path: "dir/link".into(),
        kind: WorkspaceEntryKind::Symlink,
        artifact: target,
        executable: false,
    });
    assert!(store.publish(&m).is_err());
}
