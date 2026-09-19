use ctox_sync::{
    capture::{CaptureEntry, CaptureRequest},
    checkpoint::CheckpointStore,
    contracts::{ArtifactRef, PendingEffect, SessionManifest, WorkspaceEntry, WorkspaceEntryKind},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

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

fn git_stdout(root: &Path, args: &[&str]) -> Vec<u8> {
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
    output.stdout
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
        required_capabilities: Default::default(),
        credential_references: ["gateway-account".into()].into_iter().collect(),
    }
}

fn request(workspace_root: &Path) -> CaptureRequest {
    CaptureRequest {
        session: session(),
        sequence: 3,
        workspace_root: workspace_root.to_owned(),
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
    }
}

fn init_identity(root: &Path) {
    git(root, &["init", "-q"]);
    git(
        root,
        &["config", "user.email", "reconstruct@example.invalid"],
    );
    git(root, &["config", "user.name", "Reconstruct Test"]);
}

fn prepared_workspace(root: &Path) -> PathBuf {
    let workspace = root.join("workspace");
    fs::create_dir(&workspace).unwrap();
    init_identity(&workspace);
    fs::write(workspace.join("tracked.txt"), "base\n").unwrap();
    fs::write(workspace.join("removed.txt"), "remove me\n").unwrap();
    fs::write(workspace.join("data.bin"), b"bin\0base").unwrap();
    fs::write(workspace.join("file with space.txt"), "space-base\n").unwrap();
    fs::write(workspace.join("café.txt"), "cafe-base\n").unwrap();
    fs::write(workspace.join("notes.txt"), "-- existing\n").unwrap();
    git(
        &workspace,
        &[
            "add",
            "tracked.txt",
            "removed.txt",
            "data.bin",
            "file with space.txt",
            "café.txt",
            "notes.txt",
        ],
    );
    git(&workspace, &["commit", "-qm", "base"]);
    fs::write(workspace.join("tracked.txt"), "staged\n").unwrap();
    fs::write(workspace.join("file with space.txt"), "space-staged\n").unwrap();
    fs::write(workspace.join("café.txt"), "cafe-staged\n").unwrap();
    fs::write(workspace.join("notes.txt"), "++ replacement\n").unwrap();
    git(
        &workspace,
        &[
            "add",
            "tracked.txt",
            "file with space.txt",
            "café.txt",
            "notes.txt",
        ],
    );
    fs::write(workspace.join("tracked.txt"), "unstaged\n").unwrap();
    fs::write(workspace.join("file with space.txt"), "space-unstaged\n").unwrap();
    fs::write(workspace.join("café.txt"), "cafe-unstaged\n").unwrap();
    git(&workspace, &["rm", "-q", "removed.txt"]);
    fs::write(workspace.join("data.bin"), b"bin\0staged").unwrap();
    git(&workspace, &["add", "data.bin"]);
    fs::write(workspace.join("data.bin"), b"bin\0unstaged").unwrap();

    fs::create_dir(workspace.join("config")).unwrap();
    fs::write(workspace.join("config/local.json"), "{\"local\":true}\n").unwrap();
    workspace
}

#[derive(Debug, PartialEq, Eq)]
struct WorkspaceSnapshot {
    head: String,
    index: Vec<u8>,
    worktree: Vec<u8>,
    ls_files: Vec<u8>,
    untracked: BTreeMap<String, Vec<u8>>,
    status: Vec<u8>,
}

fn snapshot(root: &Path) -> WorkspaceSnapshot {
    let diff = [
        "diff",
        "--binary",
        "--no-ext-diff",
        "--no-textconv",
        "--no-relative",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ];
    let mut cached = diff.to_vec();
    cached.push("--cached");
    let untracked_names = git_stdout(root, &["ls-files", "--others", "--exclude-standard", "-z"]);
    let mut untracked = BTreeMap::new();
    for raw in untracked_names.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let path = std::str::from_utf8(raw).unwrap();
        untracked.insert(path.to_owned(), fs::read(root.join(path)).unwrap());
    }
    WorkspaceSnapshot {
        head: String::from_utf8(git_stdout(root, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .to_owned(),
        index: git_stdout(root, &cached),
        worktree: git_stdout(root, &diff),
        ls_files: git_stdout(root, &["ls-files", "-s", "-z"]),
        untracked,
        status: git_stdout(root, &["status", "--porcelain=v1", "-z"]),
    }
}

fn blob(bytes: &[u8]) -> ArtifactRef {
    ArtifactRef {
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size_bytes: bytes.len() as u64,
    }
}

#[tokio::test]
async fn reconstruct_roundtrips_staged_unstaged_binary_deletion_and_untracked() {
    let root = tempfile::tempdir().unwrap();
    let workspace = prepared_workspace(root.path());
    let before = snapshot(&workspace);
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let captured = store.capture(request(&workspace)).await.unwrap();
    assert_eq!(captured.manifest.workspace_state.base_commit, before.head);
    assert!(captured
        .manifest
        .workspace_state
        .deleted_paths
        .contains("removed.txt"));

    fs::write(workspace.join("later.txt"), "must not be reconstructed\n").unwrap();
    git(&workspace, &["add", "later.txt"]);
    git(&workspace, &["commit", "-qm", "later"]);
    let later_head = String::from_utf8(git_stdout(&workspace, &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .to_owned();
    assert_ne!(later_head, before.head);

    let target = root.path().join("reconstructed");
    store
        .reconstruct_workspace(&captured.digest, &workspace, &target)
        .await
        .unwrap();
    let restored = snapshot(&target);
    assert_eq!(restored, before);
    assert_eq!(git_stdout(&target, &["show", ":tracked.txt"]), b"staged\n");
    assert_eq!(fs::read(target.join("tracked.txt")).unwrap(), b"unstaged\n");
    assert_eq!(git_stdout(&target, &["show", ":data.bin"]), b"bin\0staged");
    assert_eq!(fs::read(target.join("data.bin")).unwrap(), b"bin\0unstaged");
    assert!(!target.join("removed.txt").exists());
    assert_eq!(
        git_stdout(&target, &["show", ":file with space.txt"]),
        b"space-staged\n"
    );
    assert_eq!(
        fs::read(target.join("file with space.txt")).unwrap(),
        b"space-unstaged\n"
    );
    assert_eq!(
        git_stdout(&target, &["show", ":café.txt"]),
        b"cafe-staged\n"
    );
    assert_eq!(
        fs::read(target.join("café.txt")).unwrap(),
        b"cafe-unstaged\n"
    );

    assert_eq!(
        git_stdout(&target, &["show", ":notes.txt"]),
        b"++ replacement\n"
    );
    assert_eq!(
        fs::read(target.join("notes.txt")).unwrap(),
        b"++ replacement\n"
    );

    assert_eq!(
        fs::read(target.join("config/local.json")).unwrap(),
        b"{\"local\":true}\n"
    );
    assert!(!target.join("later.txt").exists());
    assert_eq!(
        String::from_utf8(git_stdout(&workspace, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim(),
        later_head
    );
    assert!(workspace.join("later.txt").is_file());
}

#[tokio::test]
async fn reconstruct_rejects_missing_or_wrong_base_without_substituting_head() {
    let root = tempfile::tempdir().unwrap();
    let workspace = prepared_workspace(root.path());
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let captured = store.capture(request(&workspace)).await.unwrap();

    let other = root.path().join("other");
    fs::create_dir(&other).unwrap();
    init_identity(&other);
    fs::write(other.join("tracked.txt"), "other\n").unwrap();
    git(&other, &["add", "tracked.txt"]);
    git(&other, &["commit", "-qm", "other"]);
    fs::write(other.join("canary.txt"), "source must stay\n").unwrap();

    let target = root.path().join("reconstructed");
    let error = store
        .reconstruct_workspace(&captured.digest, &other, &target)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not contain the checkpoint base commit"),
        "{error}"
    );
    assert!(!target.exists());
    assert_eq!(
        fs::read_to_string(other.join("canary.txt")).unwrap(),
        "source must stay\n"
    );

    let empty = root.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let missing = store
        .reconstruct_workspace(&captured.digest, &empty, &target)
        .await
        .unwrap_err();
    assert!(
        missing
            .to_string()
            .contains("source repository is not a local Git directory"),
        "{missing}"
    );
    assert!(!target.exists());
}

#[tokio::test]
async fn reconstruct_rejects_existing_target_corrupt_artifacts_and_pending_effects() {
    let root = tempfile::tempdir().unwrap();
    let workspace = prepared_workspace(root.path());
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let captured = store.capture(request(&workspace)).await.unwrap();
    let mut pending = request(&workspace);
    pending.pending_effects.push(PendingEffect {
        effect_id: "publish".into(),
        idempotency_key: None,
        description: "unconfirmed remote action".into(),
    });
    let blocked = store.capture(pending).await.unwrap();

    let existing = root.path().join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("canary.txt"), "keep me\n").unwrap();
    let error = store
        .reconstruct_workspace(&captured.digest, &workspace, &existing)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("reconstruction target already exists"),
        "{error}"
    );
    assert_eq!(
        fs::read_to_string(existing.join("canary.txt")).unwrap(),
        "keep me\n"
    );

    let target = root.path().join("reconstructed");
    let error = store
        .reconstruct_workspace(&blocked.digest, &workspace, &target)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("session requires external-effect reconciliation"),
        "{error}"
    );
    assert!(!target.exists());

    fs::write(
        root.path()
            .join("store/blobs")
            .join(&captured.manifest.workspace_state.index_patch.sha256),
        b"tampered patch",
    )
    .unwrap();
    assert!(store
        .reconstruct_workspace(&captured.digest, &workspace, &target)
        .await
        .is_err());
    assert!(!target.exists());
    assert_eq!(
        fs::read_to_string(existing.join("canary.txt")).unwrap(),
        "keep me\n"
    );
}

#[tokio::test]
async fn reconstruct_rejects_malicious_patch_paths_and_escaping_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let workspace = prepared_workspace(root.path());
    fs::write(workspace.join("canary.txt"), "untouched\n").unwrap();
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let captured = store.capture(request(&workspace)).await.unwrap();

    let evil_patch = b"diff --git a/../../escape.txt b/../../escape.txt\n\
new file mode 100644\n\
index 0000000..9daeafb\n\
--- /dev/null\n\
+++ b/../../escape.txt\n\
@@ -0,0 +1 @@\n\
+pwned\n";
    let evil = blob(evil_patch);
    store
        .ingest_blob(&evil, Cursor::new(evil_patch.as_slice()))
        .unwrap();
    let empty = blob(b"");
    store.ingest_blob(&empty, Cursor::new(b"")).unwrap();
    let mut malicious = captured.manifest.clone();
    malicious.workspace_state.index_patch = evil;
    malicious.workspace_state.worktree_patch = empty.clone();
    let digest = store.publish(&malicious).unwrap();
    let target = root.path().join("reconstructed");
    let error = store
        .reconstruct_workspace(&digest, &workspace, &target)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("unsafe portable session path")
            || error.to_string().contains("git patch path"),
        "{error}"
    );
    assert!(!target.exists());
    assert!(!root.path().join("escape.txt").exists());
    assert_eq!(
        fs::read_to_string(workspace.join("canary.txt")).unwrap(),
        "untouched\n"
    );

    let overflow_patch = b"diff --git \"a/\\777evil\" \"b/\\777evil\"\n\
new file mode 100644\n\
--- /dev/null\n\
+++ \"b/\\777evil\"\n";
    let overflow = blob(overflow_patch);
    store
        .ingest_blob(&overflow, Cursor::new(overflow_patch.as_slice()))
        .unwrap();
    let mut overflowed = captured.manifest.clone();
    overflowed.workspace_state.index_patch = overflow;
    overflowed.workspace_state.worktree_patch = empty.clone();
    let digest = store.publish(&overflowed).unwrap();
    let error = store
        .reconstruct_workspace(&digest, &workspace, &target)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("git patch path octal escape is out of range"),
        "{error}"
    );
    assert!(!target.exists());
    assert_eq!(
        fs::read_to_string(workspace.join("canary.txt")).unwrap(),
        "untouched\n"
    );

    #[cfg(unix)]
    {
        let symlink_patch = escaping_symlink_index_patch();
        let patch_text = String::from_utf8_lossy(&symlink_patch);
        assert!(
            patch_text.contains("\\ No newline at end of file"),
            "{patch_text}"
        );
        assert!(patch_text.contains("+.."), "{patch_text}");
        let link = blob(&symlink_patch);
        store
            .ingest_blob(&link, Cursor::new(symlink_patch.as_slice()))
            .unwrap();
        let mut linked = captured.manifest.clone();
        linked.workspace_state.index_patch = link;
        linked.workspace_state.worktree_patch = empty.clone();
        linked.workspace_state.deleted_paths.clear();
        linked.workspace_state.required_untracked.clear();
        linked.workspace.clear();
        let digest = store.publish(&linked).unwrap();
        let error = store
            .reconstruct_workspace(&digest, &workspace, &target)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("symlink escapes workspace"),
            "{error}"
        );
        assert!(!target.exists());
        assert_eq!(
            fs::read_to_string(workspace.join("canary.txt")).unwrap(),
            "untouched\n"
        );

        let chain_patch = chained_escaping_symlink_index_patch();
        let chain_text = String::from_utf8_lossy(&chain_patch);
        assert!(chain_text.contains("new file mode 120000"), "{chain_text}");
        assert!(
            chain_text.contains("\\ No newline at end of file"),
            "{chain_text}"
        );
        assert!(chain_text.contains("+."), "{chain_text}");
        assert!(chain_text.contains("+a/.."), "{chain_text}");
        let chain = blob(&chain_patch);
        store
            .ingest_blob(&chain, Cursor::new(chain_patch.as_slice()))
            .unwrap();
        let mut chained = captured.manifest.clone();
        chained.workspace_state.index_patch = chain;
        chained.workspace_state.worktree_patch = empty.clone();
        chained.workspace_state.deleted_paths.clear();
        chained.workspace_state.required_untracked.clear();
        chained.workspace.clear();
        let digest = store.publish(&chained).unwrap();
        let error = store
            .reconstruct_workspace(&digest, &workspace, &target)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("symlink escapes workspace"),
            "{error}"
        );
        assert!(!target.exists());
        assert_eq!(
            fs::read_to_string(workspace.join("canary.txt")).unwrap(),
            "untouched\n"
        );

        let cycle_patch = cyclic_symlink_index_patch();
        let cycle = blob(&cycle_patch);
        store
            .ingest_blob(&cycle, Cursor::new(cycle_patch.as_slice()))
            .unwrap();
        let mut cyclic = captured.manifest.clone();
        cyclic.workspace_state.index_patch = cycle;
        cyclic.workspace_state.worktree_patch = empty;
        cyclic.workspace_state.deleted_paths.clear();
        cyclic.workspace_state.required_untracked.clear();
        cyclic.workspace.clear();
        let digest = store.publish(&cyclic).unwrap();
        let error = store
            .reconstruct_workspace(&digest, &workspace, &target)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("session symlink chain is cyclic"),
            "{error}"
        );
        assert!(!target.exists());
        assert_eq!(
            fs::read_to_string(workspace.join("canary.txt")).unwrap(),
            "untouched\n"
        );
    }
}

#[tokio::test]
async fn reconstruct_rejects_imported_git_metadata_paths() {
    let root = tempfile::tempdir().unwrap();
    let workspace = prepared_workspace(root.path());
    fs::write(workspace.join("canary.txt"), "untouched\n").unwrap();
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let captured = store.capture(request(&workspace)).await.unwrap();

    let hook = b"#!/bin/sh\necho pwned\n";
    let hook_ref = blob(hook);
    store
        .ingest_blob(&hook_ref, Cursor::new(hook.as_slice()))
        .unwrap();
    let attrs = b"* text=auto\n";
    let attrs_ref = blob(attrs);
    store
        .ingest_blob(&attrs_ref, Cursor::new(attrs.as_slice()))
        .unwrap();

    let mut imported = captured.manifest.clone();
    imported.workspace.push(WorkspaceEntry {
        path: ".git/hooks/pre-commit".into(),
        kind: WorkspaceEntryKind::File,
        artifact: hook_ref,
        executable: true,
    });
    imported
        .workspace_state
        .required_untracked
        .push(WorkspaceEntry {
            path: ".GIT/info/attributes".into(),
            kind: WorkspaceEntryKind::File,
            artifact: attrs_ref,
            executable: false,
        });
    let target = root.path().join("reconstructed");
    match store.publish(&imported) {
        Err(error) => {
            assert!(
                error
                    .to_string()
                    .contains("unsafe portable session path component"),
                "{error}"
            );
        }
        Ok(digest) => {
            let error = store
                .reconstruct_workspace(&digest, &workspace, &target)
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("unsafe portable session path component"),
                "{error}"
            );
        }
    }
    assert!(!target.exists());
    assert!(!target.join(".git/hooks/pre-commit").exists());
    assert!(!workspace.join(".git/hooks/pre-commit").exists());
    assert_eq!(
        fs::read_to_string(workspace.join("canary.txt")).unwrap(),
        "untouched\n"
    );

    let evil_patch = b"diff --git a/.git/hooks/pre-commit b/.git/hooks/pre-commit\n\
new file mode 100755\n\
index 0000000..9daeafb\n\
--- /dev/null\n\
+++ b/.git/hooks/pre-commit\n\
@@ -0,0 +1,2 @@\n\
+#!/bin/sh\n\
+echo pwned\n";
    let evil = blob(evil_patch);
    store
        .ingest_blob(&evil, Cursor::new(evil_patch.as_slice()))
        .unwrap();
    let empty = blob(b"");
    store.ingest_blob(&empty, Cursor::new(b"")).unwrap();
    let mut patched = captured.manifest.clone();
    patched.workspace_state.index_patch = evil;
    patched.workspace_state.worktree_patch = empty;
    let digest = store.publish(&patched).unwrap();
    let error = store
        .reconstruct_workspace(&digest, &workspace, &target)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsafe portable session path component"),
        "{error}"
    );
    assert!(!target.exists());
    assert!(!workspace.join(".git/hooks/pre-commit").exists());
    assert_eq!(
        fs::read_to_string(workspace.join("canary.txt")).unwrap(),
        "untouched\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn reconstruct_allows_repeated_safe_symlink_references() {
    let root = tempfile::tempdir().unwrap();
    let workspace = prepared_workspace(root.path());
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let captured = store.capture(request(&workspace)).await.unwrap();
    let empty = blob(b"");
    store.ingest_blob(&empty, Cursor::new(b"")).unwrap();
    let repeat_patch = repeated_safe_symlink_index_patch();
    let repeat_text = String::from_utf8_lossy(&repeat_patch);
    assert!(
        repeat_text.contains("new file mode 120000"),
        "{repeat_text}"
    );
    assert!(repeat_text.contains("+."), "{repeat_text}");
    assert!(repeat_text.contains("+a/a"), "{repeat_text}");
    let repeat = blob(&repeat_patch);
    store
        .ingest_blob(&repeat, Cursor::new(repeat_patch.as_slice()))
        .unwrap();
    let mut repeated = captured.manifest.clone();
    repeated.workspace_state.index_patch = repeat;
    repeated.workspace_state.worktree_patch = empty;
    repeated.workspace_state.deleted_paths.clear();
    repeated.workspace_state.required_untracked.clear();
    repeated.workspace.clear();
    let digest = store.publish(&repeated).unwrap();
    let target = root.path().join("reconstructed");
    store
        .reconstruct_workspace(&digest, &workspace, &target)
        .await
        .unwrap();
    assert_eq!(fs::read_link(target.join("a")).unwrap(), PathBuf::from("."));
    assert_eq!(
        fs::read_link(target.join("b")).unwrap(),
        PathBuf::from("a/a")
    );
    assert_eq!(
        fs::canonicalize(target.join("b")).unwrap(),
        fs::canonicalize(&target).unwrap()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn reconstruct_rejects_symlink_resolution_over_total_work_budget() {
    let root = tempfile::tempdir().unwrap();
    let workspace = prepared_workspace(root.path());
    fs::write(workspace.join("canary.txt"), "untouched\n").unwrap();
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let captured = store.capture(request(&workspace)).await.unwrap();
    let empty = blob(b"");
    store.ingest_blob(&empty, Cursor::new(b"")).unwrap();
    let budget_patch = exponential_symlink_index_patch();
    let budget = blob(&budget_patch);
    store
        .ingest_blob(&budget, Cursor::new(budget_patch.as_slice()))
        .unwrap();
    let mut excessive = captured.manifest.clone();
    excessive.workspace_state.index_patch = budget;
    excessive.workspace_state.worktree_patch = empty;
    excessive.workspace_state.deleted_paths.clear();
    excessive.workspace_state.required_untracked.clear();
    excessive.workspace.clear();
    let digest = store.publish(&excessive).unwrap();
    let target = root.path().join("reconstructed");
    let error = store
        .reconstruct_workspace(&digest, &workspace, &target)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("session symlink resolution exceeds reconstruction budget"),
        "{error}"
    );
    assert!(!target.exists());
    assert_eq!(
        fs::read_to_string(workspace.join("canary.txt")).unwrap(),
        "untouched\n"
    );
}

#[tokio::test]
async fn reconstruct_accepts_index_listing_larger_than_64kib() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    init_identity(&workspace);
    fs::create_dir(workspace.join("bulk")).unwrap();
    for i in 0..1200 {
        fs::write(workspace.join(format!("bulk/{i:04}.txt")), format!("{i}\n")).unwrap();
    }
    git(&workspace, &["add", "bulk"]);
    git(&workspace, &["commit", "-qm", "bulk"]);
    let listing = git_stdout(&workspace, &["ls-files", "-s", "-z"]);
    assert!(
        listing.len() > 64 * 1024,
        "listing was {} bytes",
        listing.len()
    );
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let captured = store.capture(request(&workspace)).await.unwrap();
    let target = root.path().join("reconstructed");
    store
        .reconstruct_workspace(&captured.digest, &workspace, &target)
        .await
        .unwrap();
    assert_eq!(
        git_stdout(&target, &["ls-files", "-s", "-z"]).len(),
        listing.len()
    );
    assert_eq!(fs::read(target.join("bulk/0000.txt")).unwrap(), b"0\n");
    assert_eq!(fs::read(target.join("bulk/1199.txt")).unwrap(), b"1199\n");
}

#[cfg(unix)]
#[tokio::test]
async fn reconstruct_roundtrips_staged_symlink_with_unstaged_deletion() {
    let root = tempfile::tempdir().unwrap();
    let workspace = prepared_workspace(root.path());
    std::os::unix::fs::symlink("tracked.txt", workspace.join("rel-link")).unwrap();
    git(&workspace, &["add", "--", "rel-link"]);
    fs::remove_file(workspace.join("rel-link")).unwrap();
    let before = snapshot(&workspace);
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let captured = store.capture(request(&workspace)).await.unwrap();
    let target = root.path().join("reconstructed");
    store
        .reconstruct_workspace(&captured.digest, &workspace, &target)
        .await
        .unwrap();
    assert_eq!(snapshot(&target), before);
    let staged =
        String::from_utf8(git_stdout(&target, &["ls-files", "-s", "--", "rel-link"])).unwrap();
    assert!(staged.starts_with("120000 "), "{staged}");
    assert!(fs::symlink_metadata(target.join("rel-link")).is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn reconstruct_roundtrips_staged_symlink_replaced_by_regular_file() {
    let root = tempfile::tempdir().unwrap();
    let workspace = prepared_workspace(root.path());
    std::os::unix::fs::symlink("tracked.txt", workspace.join("rel-link")).unwrap();
    git(&workspace, &["add", "--", "rel-link"]);
    fs::remove_file(workspace.join("rel-link")).unwrap();
    fs::write(workspace.join("rel-link"), "now a file\n").unwrap();
    let before = snapshot(&workspace);
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let captured = store.capture(request(&workspace)).await.unwrap();
    let target = root.path().join("reconstructed");
    store
        .reconstruct_workspace(&captured.digest, &workspace, &target)
        .await
        .unwrap();
    assert_eq!(snapshot(&target), before);
    let staged =
        String::from_utf8(git_stdout(&target, &["ls-files", "-s", "--", "rel-link"])).unwrap();
    assert!(staged.starts_with("120000 "), "{staged}");
    assert!(fs::symlink_metadata(target.join("rel-link"))
        .unwrap()
        .is_file());
    assert_eq!(fs::read(target.join("rel-link")).unwrap(), b"now a file\n");
}

#[tokio::test]
async fn reconstruct_installs_owner_supplied_workspace_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let workspace = prepared_workspace(root.path());
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();
    let mut req = request(&workspace);
    req.workspace.push(CaptureEntry {
        path: "session notes.txt".into(),
        kind: WorkspaceEntryKind::File,
        bytes: b"owner workspace\n".to_vec(),
        executable: false,
    });
    let captured = store.capture(req).await.unwrap();
    let target = root.path().join("reconstructed");
    store
        .reconstruct_workspace(&captured.digest, &workspace, &target)
        .await
        .unwrap();
    assert_eq!(
        fs::read(target.join("session notes.txt")).unwrap(),
        b"owner workspace\n"
    );
    assert!(git_stdout(&target, &["ls-files", "--", "session notes.txt"]).is_empty());
    assert_eq!(
        fs::read(target.join("config/local.json")).unwrap(),
        b"{\"local\":true}\n"
    );

    let mut colliding = request(&workspace);
    colliding.workspace.push(CaptureEntry {
        path: "tracked.txt".into(),
        kind: WorkspaceEntryKind::File,
        bytes: b"collision\n".to_vec(),
        executable: false,
    });
    let captured = store.capture(colliding).await.unwrap();
    let collision = root.path().join("collision");
    let error = store
        .reconstruct_workspace(&captured.digest, &workspace, &collision)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("path collides with an existing workspace entry"),
        "{error}"
    );
    assert!(!collision.exists());
}

#[tokio::test]
async fn reconstruct_roundtrips_mixed_quote_git_renames() {
    let root = tempfile::tempdir().unwrap();
    let store = CheckpointStore::open(root.path().join("store"), 1024 * 1024).unwrap();

    let ascii_to_unicode = root.path().join("ascii-to-unicode");
    fs::create_dir(&ascii_to_unicode).unwrap();
    init_identity(&ascii_to_unicode);
    fs::write(ascii_to_unicode.join("old.txt"), "rename-me\n").unwrap();
    git(&ascii_to_unicode, &["add", "old.txt"]);
    git(&ascii_to_unicode, &["commit", "-qm", "old"]);
    git(&ascii_to_unicode, &["mv", "old.txt", "é.txt"]);
    let captured = store.capture(request(&ascii_to_unicode)).await.unwrap();
    let index_text = String::from_utf8(
        fs::read(
            root.path()
                .join("store/blobs")
                .join(&captured.manifest.workspace_state.index_patch.sha256),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        index_text.contains(r#"diff --git a/old.txt "b/\303\251.txt""#),
        "{index_text}"
    );
    let before = snapshot(&ascii_to_unicode);
    let target = root.path().join("reconstructed-unicode");
    store
        .reconstruct_workspace(&captured.digest, &ascii_to_unicode, &target)
        .await
        .unwrap();
    assert_eq!(snapshot(&target), before);
    assert_eq!(fs::read(target.join("é.txt")).unwrap(), b"rename-me\n");
    assert!(!target.join("old.txt").exists());

    let unicode_to_spaces = root.path().join("unicode-to-spaces");
    fs::create_dir(&unicode_to_spaces).unwrap();
    init_identity(&unicode_to_spaces);
    fs::write(unicode_to_spaces.join("é.txt"), "rename-me\n").unwrap();
    git(&unicode_to_spaces, &["add", "é.txt"]);
    git(&unicode_to_spaces, &["commit", "-qm", "unicode"]);
    git(&unicode_to_spaces, &["mv", "é.txt", "file with space.txt"]);
    let captured = store.capture(request(&unicode_to_spaces)).await.unwrap();
    let index_text = String::from_utf8(
        fs::read(
            root.path()
                .join("store/blobs")
                .join(&captured.manifest.workspace_state.index_patch.sha256),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        index_text.contains(r#"diff --git "a/\303\251.txt" b/file with space.txt"#),
        "{index_text}"
    );
    let before = snapshot(&unicode_to_spaces);
    let target = root.path().join("reconstructed-spaces");
    store
        .reconstruct_workspace(&captured.digest, &unicode_to_spaces, &target)
        .await
        .unwrap();
    assert_eq!(snapshot(&target), before);
    assert_eq!(
        fs::read(target.join("file with space.txt")).unwrap(),
        b"rename-me\n"
    );
    assert!(!target.join("é.txt").exists());
}

#[cfg(unix)]
fn staged_binary_diff(root: &Path) -> Vec<u8> {
    git_stdout(
        root,
        &[
            "diff",
            "--cached",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
            "--no-relative",
            "--src-prefix=a/",
            "--dst-prefix=b/",
        ],
    )
}

#[cfg(unix)]
fn escaping_symlink_index_patch() -> Vec<u8> {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("symlink-source");
    fs::create_dir(&repo).unwrap();
    init_identity(&repo);
    std::os::unix::fs::symlink("..", repo.join("link")).unwrap();
    git(&repo, &["add", "--", "link"]);
    staged_binary_diff(&repo)
}

#[cfg(unix)]
fn chained_escaping_symlink_index_patch() -> Vec<u8> {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("symlink-chain");
    fs::create_dir(&repo).unwrap();
    init_identity(&repo);
    std::os::unix::fs::symlink(".", repo.join("a")).unwrap();
    std::os::unix::fs::symlink("a/..", repo.join("b")).unwrap();
    git(&repo, &["add", "--", "a", "b"]);
    staged_binary_diff(&repo)
}

#[cfg(unix)]
fn repeated_safe_symlink_index_patch() -> Vec<u8> {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("symlink-repeat");
    fs::create_dir(&repo).unwrap();
    init_identity(&repo);
    std::os::unix::fs::symlink(".", repo.join("a")).unwrap();
    std::os::unix::fs::symlink("a/a", repo.join("b")).unwrap();
    git(&repo, &["add", "--", "a", "b"]);
    staged_binary_diff(&repo)
}

#[cfg(unix)]
fn cyclic_symlink_index_patch() -> Vec<u8> {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("symlink-cycle");
    fs::create_dir(&repo).unwrap();
    init_identity(&repo);
    std::os::unix::fs::symlink("b", repo.join("a")).unwrap();
    std::os::unix::fs::symlink("a", repo.join("b")).unwrap();
    git(&repo, &["add", "--", "a", "b"]);
    staged_binary_diff(&repo)
}

#[cfg(unix)]
fn exponential_symlink_index_patch() -> Vec<u8> {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("symlink-budget");
    fs::create_dir(&repo).unwrap();
    init_identity(&repo);
    std::os::unix::fs::symlink(".", repo.join("s0")).unwrap();
    for i in 1..=6 {
        let prev = format!("s{}", i - 1);
        std::os::unix::fs::symlink(format!("{prev}/{prev}"), repo.join(format!("s{i}"))).unwrap();
    }
    git(
        &repo,
        &["add", "--", "s0", "s1", "s2", "s3", "s4", "s5", "s6"],
    );
    staged_binary_diff(&repo)
}
