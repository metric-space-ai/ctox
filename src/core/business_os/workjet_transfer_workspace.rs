//! Thread-bound workspace moves over admitted, resumable CTOX file transfers.
//! Session/history/goal migration and safe-turn boundaries belong to Workjet.
use super::workjet_transfer_git::{self as git, GitPackManifest};
use anyhow::{bail, ensure, Context, Result};
use ctox_transfers::{DownloadRequest, Store};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const NAMES: [&str; 5] = [
    "bundle.gitbundle",
    "tracked.patch",
    "index.patch",
    "untracked.tar",
    "manifest.json",
];
const MAX_DESCRIPTOR: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Artifact {
    name: String,
    file_id: String,
    sha256: String,
    size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Workspace {
    version: u32,
    thread_id: String,
    move_id: String,
    source_instance_id: String,
    source_public_identity: String,
    manifest: GitPackManifest,
    status_sha256: String,
    artifacts: Vec<Artifact>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Outgoing {
    workspace: Workspace,
    source: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Incoming {
    workspace: Workspace,
    source_target: String,
    target: PathBuf,
}

fn id(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 96
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid workspace/thread identifier"
    );
    Ok(())
}

fn validate(workspace: &Workspace, thread: &str) -> Result<()> {
    ensure!(
        !thread.is_empty() && thread.len() <= 256 && !thread.chars().any(char::is_control),
        "invalid thread identifier"
    );
    id(&workspace.move_id)?;
    ensure!(
        workspace.version == 1 && workspace.thread_id == thread,
        "workspace belongs to another thread or contract version"
    );
    ensure!(
        workspace.manifest.git.index.is_some(),
        "workspace requires a separate index proof"
    );
    ensure!(
        workspace.status_sha256.len() == 64
            && workspace
                .status_sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "invalid Git status digest"
    );
    ensure!(
        workspace.artifacts.len() == NAMES.len(),
        "workspace artifact set incomplete"
    );
    for (artifact, name) in workspace.artifacts.iter().zip(NAMES) {
        ensure!(
            artifact.name == name && !artifact.file_id.is_empty(),
            "workspace artifact order/name differs"
        );
        ensure!(
            artifact.size <= i64::MAX as u64
                && artifact.sha256.len() == 64
                && artifact
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
            "invalid workspace artifact hash/size"
        );
    }
    let manifest = &workspace.artifacts[4];
    ensure!(
        manifest.sha256 == workspace.manifest.manifest_sha256,
        "workspace manifest digest differs"
    );
    Ok(())
}

fn directory(root: &Path, direction: &str, move_id: &str) -> Result<PathBuf> {
    id(move_id)?;
    let path = crate::paths::runtime_dir(root)
        .join("transfers/workspaces")
        .join(direction)
        .join(move_id);
    fs::create_dir_all(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(path)
}

struct MoveLease(File);
impl Drop for MoveLease {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
fn lease(directory: &Path) -> Result<MoveLease> {
    let path = directory.join("move.lock");
    if path.exists() {
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "invalid workspace lease"
        );
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock()
        .context("workspace move already has a writer")?;
    Ok(MoveLease(file))
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= MAX_DESCRIPTOR,
        "invalid workspace record"
    );
    Ok(serde_json::from_reader(File::open(path)?)?)
}

fn write_new<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path.parent().context("workspace record parent missing")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn status_sha256(source: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(source)
        .args([
            "-c",
            "core.quotePath=false",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "cannot measure workspace Git status"
    );
    Ok(format!("{:x}", Sha256::digest(output.stdout)))
}

fn export(root: &Path, thread: &str, move_id: &str, source: &Path) -> Result<Workspace> {
    ensure!(
        !thread.is_empty() && thread.len() <= 256 && !thread.chars().any(char::is_control),
        "invalid thread identifier"
    );
    let directory = directory(root, "outgoing", move_id)?;
    let _lease = lease(&directory)?;
    let record = directory.join("workspace.json");
    // A move ID identifies one frozen snapshot; it is never silently repacked.
    if record.exists() {
        let saved: Outgoing = read(&record)?;
        validate(&saved.workspace, thread)?;
        ensure!(
            saved.source == fs::canonicalize(source)?,
            "move is already bound to another source workspace"
        );
        git::verify_git_working_copy(source, &saved.workspace.manifest)?;
        ensure!(
            status_sha256(source)? == saved.workspace.status_sha256,
            "frozen workspace changed; use a new move ID"
        );
        return Ok(saved.workspace);
    }
    let before = status_sha256(source)?;
    let artifacts_dir = directory.join("artifacts");
    let manifest = git::pack_git_working_copy(source, &artifacts_dir)?;
    git::verify_git_working_copy(source, &manifest)?;
    ensure!(
        status_sha256(source)? == before,
        "workspace changed during packing; stop at a safe turn boundary"
    );
    let mut artifacts = Vec::new();
    let mut identity = None;
    for name in NAMES {
        let published = super::publish_native_file(root, &artifacts_dir.join(name))?;
        let current = (
            published.source_instance_id,
            published.source_public_identity,
        );
        if let Some(previous) = &identity {
            ensure!(previous == &current, "workspace publisher identity changed");
        } else {
            identity = Some(current);
        }
        artifacts.push(Artifact {
            name: name.into(),
            file_id: published.file_id,
            sha256: published.sha256,
            size: published.size,
        });
    }
    let (source_instance_id, source_public_identity) =
        identity.context("workspace identity missing")?;
    let workspace = Workspace {
        version: 1,
        thread_id: thread.into(),
        move_id: move_id.into(),
        source_instance_id,
        source_public_identity,
        manifest,
        status_sha256: before,
        artifacts,
    };
    validate(&workspace, thread)?;
    git::verify_git_working_copy(source, &workspace.manifest)?;
    ensure!(
        status_sha256(source)? == workspace.status_sha256,
        "source changed during publication"
    );
    write_new(
        &record,
        &Outgoing {
            workspace: workspace.clone(),
            source: fs::canonicalize(source)?,
        },
    )?;
    Ok(workspace)
}

fn job_id(workspace: &Workspace, index: usize) -> String {
    format!("workspace-{}-{index}", workspace.move_id)
}

fn store(root: &Path) -> Result<Store> {
    Store::open(
        crate::paths::core_db(root),
        crate::paths::runtime_dir(root).join("transfers"),
    )
}

fn incoming(root: &Path, thread: &str, move_id: &str) -> Result<Incoming> {
    let saved: Incoming = read(&directory(root, "incoming", move_id)?.join("workspace.json"))?;
    validate(&saved.workspace, thread)?;
    ensure!(
        saved.workspace.move_id == move_id,
        "workspace move identifier changed"
    );
    Ok(saved)
}

fn start(
    root: &Path,
    thread: &str,
    descriptor: &Path,
    source_target: &str,
    target: &Path,
) -> Result<Value> {
    let workspace: Workspace = read(descriptor)?;
    validate(&workspace, thread)?;
    ensure!(target.is_absolute(), "workspace target must be absolute");
    let operation = Incoming {
        workspace,
        source_target: source_target.into(),
        target: target.into(),
    };
    let move_dir = directory(root, "incoming", &operation.workspace.move_id)?;
    let _lease = lease(&move_dir)?;
    let path = move_dir.join("workspace.json");
    if path.exists() {
        ensure!(
            read::<Incoming>(&path)? == operation,
            "workspace move is already bound to another target or snapshot"
        );
    } else {
        // Reserve the immutable operation before admission. A retry reuses any
        // jobs admitted before an interrupted start, including original grants.
        ensure!(!target.exists(), "workspace target already exists");
        write_new(&path, &operation)?;
    }
    let transfers = store(root)?;
    for (index, artifact) in operation.workspace.artifacts.iter().enumerate() {
        crate::transfers_native::enqueue_workspace_peer(
            root,
            &transfers,
            crate::transfers_native::PeerDownload {
                id: job_id(&operation.workspace, index),
                target_id: source_target.into(),
                sha256: artifact.sha256.clone(),
                size: artifact.size,
                file_id: artifact.file_id.clone(),
            },
            &operation.workspace.source_instance_id,
            &operation.workspace.source_public_identity,
        )?;
    }
    status(root, thread, &operation.workspace.move_id, None)
}

fn requests(transfers: &Store, operation: &Incoming) -> Result<Vec<DownloadRequest>> {
    let requests = operation
        .workspace
        .artifacts
        .iter()
        .enumerate()
        .map(|(index, artifact)| {
            let request = transfers.get(&job_id(&operation.workspace, index))?.request;
            let source = request
                .peer_source
                .as_ref()
                .context("workspace requires native peer transfer")?;
            let binding = source
                .account_binding
                .as_ref()
                .context("workspace requires original native account")?;
            ensure!(
                request.sources.is_empty()
                    && request.storage.is_none()
                    && request.sha256 == artifact.sha256
                    && request.size == artifact.size
                    && source.file_id == artifact.file_id
                    && source.collection == "desktop_files"
                    && source.instance_id == operation.workspace.source_instance_id
                    && source.public_key == operation.workspace.source_public_identity
                    && binding.target_id == operation.source_target,
                "workspace transfer binding changed"
            );
            Ok(request)
        })
        .collect::<Result<Vec<_>>>()?;
    let first = requests[0]
        .peer_source
        .as_ref()
        .and_then(|peer| peer.account_binding.as_ref())
        .context("workspace account missing")?;
    ensure!(
        requests.iter().all(|request| request
            .peer_source
            .as_ref()
            .and_then(|peer| peer.account_binding.as_ref())
            .is_some_and(|binding| binding.account_epoch == first.account_epoch
                && binding.principal_sha256 == first.principal_sha256)),
        "workspace artifacts span different native account epochs/principals"
    );
    Ok(requests)
}

fn status(root: &Path, thread: &str, move_id: &str, action: Option<&str>) -> Result<Value> {
    let operation = incoming(root, thread, move_id)?;
    let transfers = store(root)?;
    let originals = requests(&transfers, &operation)?;
    let jobs = originals
        .iter()
        .map(|request| {
            if let Some(action) = action {
                transfers.control(&request.id, action)
            } else {
                transfers.get(&request.id)
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(
        serde_json::json!({ "thread_id": thread, "move_id": move_id, "target": operation.target, "jobs": jobs }),
    )
}

fn finish(root: &Path, thread: &str, move_id: &str) -> Result<Value> {
    let move_dir = directory(root, "incoming", move_id)?;
    let _lease = lease(&move_dir)?;
    let operation = incoming(root, thread, move_id)?;
    let transfers = store(root)?;
    let originals = requests(&transfers, &operation)?;
    if operation.target.exists() {
        let report: Value = read(&move_dir.join("import.json"))?;
        crate::transfers_native::consume_workspace_peers(root, &transfers, &originals, || {
            for request in &originals {
                transfers.read_completed_peer_artifact(request, |_| Ok(()))?;
            }
            git::verify_git_working_copy(&operation.target, &operation.workspace.manifest)?;
            ensure!(
                status_sha256(&operation.target)? == operation.workspace.status_sha256,
                "imported target changed"
            );
            Ok(())
        })?;
        return Ok(report);
    }
    ensure!(
        !operation.target.exists(),
        "workspace target already exists"
    );
    let parent = operation
        .target
        .parent()
        .context("workspace target parent missing")?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".ctox-workspace-")
        .tempdir_in(parent)?;
    let artifacts = stage.path().join("artifacts");
    fs::create_dir(&artifacts)?;
    let checkout = stage.path().join("checkout");
    let report =
        crate::transfers_native::consume_workspace_peers(root, &transfers, &originals, || {
            for (request, artifact) in originals.iter().zip(&operation.workspace.artifacts) {
                transfers.read_completed_peer_artifact(request, |input| {
                    let mut output = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(artifacts.join(&artifact.name))?;
                    std::io::copy(input, &mut output)?;
                    output.sync_all()?;
                    Ok(())
                })?;
            }
            let report =
                git::apply_git_working_copy(&artifacts, &operation.workspace.manifest, &checkout)?;
            ensure!(
                status_sha256(&checkout)? == operation.workspace.status_sha256,
                "restored Git status differs from source"
            );
            Ok(report)
        })?;
    // The target stays unpublished until original account/grant revalidation
    // after reconstruction. A failed import drops only our own staging tree.
    ensure!(
        !operation.target.exists(),
        "workspace target appeared during import"
    );
    let result = serde_json::json!({"thread_id": thread, "move_id": move_id, "target": operation.target, "git": report, "status_sha256": operation.workspace.status_sha256, "imported": true});
    let receipt = move_dir.join("import.json");
    if receipt.exists() {
        ensure!(
            read::<Value>(&receipt)? == result,
            "workspace import receipt changed"
        );
    } else {
        write_new(&receipt, &result)?;
    }
    publish_checkout(&checkout, &operation.target)?;
    File::open(parent)?.sync_all()?;
    Ok(result)
}

fn publish_checkout(source: &Path, target: &Path) -> Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let source = CString::new(source.as_os_str().as_bytes())?;
        let target = CString::new(target.as_os_str().as_bytes())?;
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        let result =
            unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
        if result != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (source, target);
        bail!("atomic workspace publication is supported on Linux and macOS")
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Result<&'a str> {
    let matches: Vec<_> = args.windows(2).filter(|pair| pair[0] == name).collect();
    ensure!(
        matches.len() == 1 && !matches[0][1].starts_with("--"),
        "expected exactly one {name} value"
    );
    Ok(&matches[0][1])
}

#[cfg(test)]
#[path = "workjet_transfer_workspace_tests.rs"]
mod tests;

pub(crate) fn execute_cli(root: &Path, args: &[String]) -> Result<Value> {
    if matches!(args.first().map(String::as_str), Some("pack" | "apply")) {
        return git::execute_cli(args);
    }
    let thread = flag(args, "--thread-id")?;
    match args.first().map(String::as_str) {
        Some("workspace-export") => Ok(serde_json::to_value(export(
            root,
            thread,
            flag(args, "--move-id")?,
            Path::new(flag(args, "--source")?),
        )?)?),
        Some("workspace-start") => start(
            root,
            thread,
            Path::new(flag(args, "--descriptor")?),
            flag(args, "--source-target")?,
            Path::new(flag(args, "--target")?),
        ),
        Some("workspace-status") => status(root, thread, flag(args, "--move-id")?, None),
        Some("workspace-pause" | "workspace-resume" | "workspace-cancel") => status(
            root,
            thread,
            flag(args, "--move-id")?,
            args[0].strip_prefix("workspace-"),
        ),
        Some("workspace-finish") => finish(root, thread, flag(args, "--move-id")?),
        _ => {
            bail!("expected pack|apply or workspace-export|start|status|pause|resume|cancel|finish")
        }
    }
}
