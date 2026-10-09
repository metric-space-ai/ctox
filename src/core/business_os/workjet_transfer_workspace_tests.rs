use super::*;
use ctox_transfers::{PeerAccountBinding, PeerSource};

fn git_command(path: &Path, arguments: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn repository(root: &Path) -> PathBuf {
    let repository = root.join("repository");
    fs::create_dir(&repository).unwrap();
    git_command(&repository, &["init", "-b", "main"]);
    git_command(&repository, &["config", "user.name", "Workspace test"]);
    git_command(
        &repository,
        &["config", "user.email", "workspace-test@example.invalid"],
    );
    fs::write(repository.join("tracked"), b"top\nbase\nbottom\n").unwrap();
    git_command(&repository, &["add", "."]);
    git_command(&repository, &["commit", "-m", "base"]);
    repository
}

fn workspace(root: &Path, source: &Path) -> Workspace {
    let artifacts = root.join("artifacts");
    let manifest = git::pack_git_working_copy(source, &artifacts).unwrap();
    Workspace {
        version: 1,
        thread_id: "thread-one".into(),
        move_id: "move-one".into(),
        source_instance_id: "source-instance".into(),
        source_public_identity: format!("ed25519:{}", "a".repeat(64)),
        manifest,
        status_sha256: status_sha256(source).unwrap(),
        artifacts: NAMES
            .iter()
            .map(|name| Artifact {
                name: (*name).into(),
                file_id: format!("file-{name}"),
                sha256: git::sha256_file(&artifacts.join(name)).unwrap(),
                size: fs::metadata(artifacts.join(name)).unwrap().len(),
            })
            .collect(),
    }
}

#[test]
fn linked_worktree_roundtrip_retains_staged_unstaged_untracked_without_git_pointer() {
    let scratch = tempfile::tempdir().unwrap();
    let repository = repository(scratch.path());
    let source = scratch.path().join("linked");
    git_command(
        &repository,
        &["worktree", "add", "-b", "worker", source.to_str().unwrap()],
    );
    assert!(source.join(".git").is_file());
    fs::write(source.join("tracked"), b"top\nstaged\nbottom\n").unwrap();
    git_command(&source, &["add", "tracked"]);
    fs::write(source.join("tracked"), b"top\nunstaged\nbottom\n").unwrap();
    fs::write(source.join("new-staged"), [0, 1, 255]).unwrap();
    git_command(&source, &["add", "new-staged"]);
    fs::remove_file(source.join("new-staged")).unwrap();
    fs::write(source.join("untracked"), [255, 0, 42]).unwrap();
    let original = git_command(
        &source,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    );
    let first = workspace(scratch.path(), &source);
    git::verify_git_working_copy(&source, &first.manifest).unwrap();
    let target = scratch.path().join("target");
    git::apply_git_working_copy(&scratch.path().join("artifacts"), &first.manifest, &target)
        .unwrap();
    assert!(target.join(".git").is_dir());
    assert_eq!(
        original,
        git_command(
            &target,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"]
        )
    );
    let second_artifacts = scratch.path().join("return-artifacts");
    let second = git::pack_git_working_copy(&target, &second_artifacts).unwrap();
    assert_eq!(first.manifest, second);
    let returned = scratch.path().join("returned");
    git::apply_git_working_copy(&second_artifacts, &second, &returned).unwrap();
    git_command(&returned, &["config", "diff.context", "0"]);
    git::verify_git_working_copy(&returned, &first.manifest).unwrap();
    assert_eq!(first.status_sha256, status_sha256(&returned).unwrap());
    assert!(returned.join(".git").is_dir());
    fs::write(returned.join("tracked"), b"changed after import\n").unwrap();
    assert!(git::verify_git_working_copy(&returned, &first.manifest).is_err());
}

#[test]
fn descriptor_rejects_other_thread_and_path_like_artifact() {
    let scratch = tempfile::tempdir().unwrap();
    let repository = repository(scratch.path());
    let mut descriptor = workspace(scratch.path(), &repository);
    validate(&descriptor, "thread-one").unwrap();
    assert!(validate(&descriptor, "thread-two").is_err());
    descriptor.artifacts[0].name = "../bundle.gitbundle".into();
    assert!(validate(&descriptor, "thread-one").is_err());
}

#[test]
fn original_peer_bindings_reject_source_hash_and_account_epoch_changes() {
    let scratch = tempfile::tempdir().unwrap();
    let repository = repository(scratch.path());
    let workspace = workspace(scratch.path(), &repository);
    let operation = Incoming {
        workspace,
        source_target: "source-account".into(),
        target: scratch.path().join("target"),
    };
    let transfers = Store::open(
        scratch.path().join("jobs.sqlite3"),
        scratch.path().join("transfers"),
    )
    .unwrap();
    for (index, artifact) in operation.workspace.artifacts.iter().enumerate() {
        transfers
            .enqueue(DownloadRequest {
                id: job_id(&operation.workspace, index),
                sources: vec![],
                storage: None,
                sha256: artifact.sha256.clone(),
                size: artifact.size,
                peer_source: Some(PeerSource {
                    instance_id: operation.workspace.source_instance_id.clone(),
                    public_key: operation.workspace.source_public_identity.clone(),
                    collection: "desktop_files".into(),
                    file_id: artifact.file_id.clone(),
                    account_binding: Some(PeerAccountBinding {
                        target_id: operation.source_target.clone(),
                        account_epoch: 1,
                        grant_id: format!("grant-{index}"),
                        principal_sha256: "b".repeat(64),
                    }),
                }),
            })
            .unwrap();
    }
    requests(&transfers, &operation).unwrap();
    let mut changed = operation.clone();
    changed.workspace.source_instance_id = "another-source".into();
    assert!(requests(&transfers, &changed).is_err());
    changed = operation.clone();
    changed.workspace.artifacts[0].sha256 = "c".repeat(64);
    assert!(requests(&transfers, &changed).is_err());
    let connection = rusqlite::Connection::open(scratch.path().join("jobs.sqlite3")).unwrap();
    let mut request = transfers
        .get(&job_id(&operation.workspace, 4))
        .unwrap()
        .request;
    request
        .peer_source
        .as_mut()
        .unwrap()
        .account_binding
        .as_mut()
        .unwrap()
        .account_epoch = 2;
    connection
        .execute(
            "UPDATE ctox_transfer_jobs SET request=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&request).unwrap(), request.id],
        )
        .unwrap();
    assert!(requests(&transfers, &operation).is_err());
}

#[test]
fn record_and_publication_never_replace_existing_data() {
    let scratch = tempfile::tempdir().unwrap();
    let record = scratch.path().join("record.json");
    write_new(&record, &serde_json::json!({"original": true})).unwrap();
    assert!(write_new(&record, &serde_json::json!({"original": false})).is_err());
    assert_eq!(
        read::<Value>(&record).unwrap(),
        serde_json::json!({"original": true})
    );
    let source = scratch.path().join("checkout");
    let target = scratch.path().join("existing");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(target.join("sentinel"), b"keep").unwrap();
    assert!(publish_checkout(&source, &target).is_err());
    assert_eq!(fs::read(target.join("sentinel")).unwrap(), b"keep");
    assert!(source.exists());
    fs::remove_file(target.join("sentinel")).unwrap();
    assert!(publish_checkout(&source, &target).is_err());
}

#[test]
fn move_lease_releases_explicitly_and_rejects_second_writer() {
    let scratch = tempfile::tempdir().unwrap();
    let first = lease(scratch.path()).unwrap();
    assert!(lease(scratch.path()).is_err());
    drop(first);
    lease(scratch.path()).unwrap();
}
