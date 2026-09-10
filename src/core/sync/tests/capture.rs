use ctox_sync::{
    capture::{CaptureEntry, CaptureRequest},
    checkpoint::CheckpointStore,
    contracts::{SessionManifest, WorkspaceEntryKind},
};
use std::{collections::BTreeSet, fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn session() -> SessionManifest {
    SessionManifest {
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
    }
}

#[tokio::test]
async fn capture_records_git_deltas_untracked_files_and_deletions() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    git(&workspace, &["init", "-q"]);
    git(
        &workspace,
        &["config", "user.email", "capture@example.invalid"],
    );
    git(&workspace, &["config", "user.name", "Capture Test"]);
    fs::write(workspace.join("tracked.txt"), "base\n").unwrap();
    fs::write(workspace.join("removed.txt"), "remove me\n").unwrap();
    git(&workspace, &["add", "tracked.txt", "removed.txt"]);
    git(&workspace, &["commit", "-qm", "base"]);

    fs::write(workspace.join("tracked.txt"), "staged\n").unwrap();
    git(&workspace, &["add", "tracked.txt"]);
    fs::write(workspace.join("tracked.txt"), "unstaged\n").unwrap();
    git(&workspace, &["rm", "-q", "removed.txt"]);
    fs::create_dir(workspace.join("config")).unwrap();
    fs::write(workspace.join("config/local.json"), "{\"local\":true}\n").unwrap();

    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let result = store
        .capture(CaptureRequest {
            session: session(),
            sequence: 7,
            workspace_root: workspace.clone(),
            history: vec![b"journal\n".to_vec()],
            attachments: vec![b"attachment\n".to_vec()],
            workspace: vec![],
            provider_state: vec![CaptureEntry {
                path: "provider.json".into(),
                kind: WorkspaceEntryKind::File,
                bytes: b"provider state\n".to_vec(),
                executable: false,
            }],
            pending_effects: vec![],
        })
        .await
        .unwrap();

    assert_eq!(result.manifest.sequence, 7);
    assert_eq!(result.manifest.workspace_state.base_commit.len(), 40);
    assert!(result.manifest.workspace_state.index_patch.size_bytes > 0);
    assert!(result.manifest.workspace_state.worktree_patch.size_bytes > 0);
    assert!(result
        .manifest
        .workspace_state
        .required_untracked
        .iter()
        .any(|entry| entry.path == "config/local.json"));
    assert!(result
        .manifest
        .workspace_state
        .deleted_paths
        .contains("removed.txt"));
    assert_eq!(store.load(&result.digest).unwrap(), result.manifest);

    let restored = root.path().join("restored");
    store.restore(&result.digest, &restored).unwrap();
    assert_eq!(
        fs::read_to_string(restored.join("workspace/config/local.json")).unwrap(),
        "{\"local\":true}\n"
    );
    assert!(restored.join("git/index.patch").is_file());
    assert!(restored.join("git/worktree.patch").is_file());
    assert_eq!(
        fs::read_to_string(restored.join("provider/provider.json")).unwrap(),
        "provider state\n"
    );
}

#[tokio::test]
async fn capture_fails_closed_outside_a_git_workspace() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let store = CheckpointStore::open(root.path().join("store"), 1024).unwrap();
    let result = store
        .capture(CaptureRequest {
            session: session(),
            sequence: 1,
            workspace_root: workspace,
            history: vec![b"journal".to_vec()],
            attachments: vec![],
            workspace: vec![],
            provider_state: vec![CaptureEntry {
                path: "provider.json".into(),
                kind: WorkspaceEntryKind::File,
                bytes: b"provider".to_vec(),
                executable: false,
            }],
            pending_effects: vec![],
        })
        .await;
    assert!(result.is_err());
}
