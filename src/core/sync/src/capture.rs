//! Quiescent capture of a portable execution checkpoint from a Git workspace.
//!
//! The caller owns the quiescence boundary: it must stop new turns and flush
//! the journal/provider state before invoking [`CheckpointStore::capture`].
//! This module only captures bounded Git state and caller-supplied durable
//! artifacts. It never starts a provider or applies a patch.

use crate::{
    checkpoint::{validate_manifest, validate_path, CheckpointStore},
    contracts::{
        ArtifactRef, CheckpointManifest, GitWorkspaceState, PendingEffect, SessionManifest,
        WorkspaceEntry, WorkspaceEntryKind,
    },
};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{process::Command, time::timeout};

const GIT_COMMAND_DEADLINE: Duration = Duration::from_secs(10);
const MAX_GIT_STDERR_BYTES: usize = 64 * 1024;

/// A caller-supplied file that is already inside the quiescent capture
/// boundary, such as provider state or a selected workspace file.
#[derive(Debug, Clone)]
pub struct CaptureEntry {
    pub path: String,
    pub kind: WorkspaceEntryKind,
    pub bytes: Vec<u8>,
    pub executable: bool,
}

/// Inputs that cannot be derived from Git and must be supplied by the
/// quiescent execution owner.
#[derive(Debug)]
pub struct CaptureRequest {
    pub session: SessionManifest,
    pub sequence: u64,
    pub workspace_root: PathBuf,
    pub history: Vec<Vec<u8>>,
    pub attachments: Vec<Vec<u8>>,
    pub workspace: Vec<CaptureEntry>,
    pub provider_state: Vec<CaptureEntry>,
    pub pending_effects: Vec<PendingEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureResult {
    pub digest: String,
    pub manifest: CheckpointManifest,
}

impl CheckpointStore {
    /// Capture bounded Git state and caller-supplied durable artifacts, then
    /// publish one content-addressed manifest. The caller must stop accepting
    /// new work before calling this method.
    pub async fn capture(&self, request: CaptureRequest) -> io::Result<CaptureResult> {
        let root = fs::canonicalize(&request.workspace_root)?;
        if !root.is_dir() {
            return Err(invalid_capture("workspace root is not a directory"));
        }
        let base_commit = git_line(&root, &["rev-parse", "--verify", "HEAD^{commit}"], 128).await?;
        if !commit_valid(&base_commit) {
            return Err(invalid_capture("Git returned an invalid base commit"));
        }
        let index_patch = self.ingest_bytes(
            &git_bytes(
                &root,
                &["diff", "--binary", "--no-ext-diff", "--cached"],
                self.max_blob_bytes(),
            )
            .await?,
        )?;
        let worktree_patch = self.ingest_bytes(
            &git_bytes(
                &root,
                &["diff", "--binary", "--no-ext-diff"],
                self.max_blob_bytes(),
            )
            .await?,
        )?;
        let required_untracked = self.capture_untracked(&root).await?;
        let deleted_paths = self.capture_deleted_paths(&root).await?;

        let mut history = Vec::with_capacity(request.history.len());
        for bytes in request.history {
            history.push(self.ingest_bytes(&bytes)?);
        }
        let mut attachments = Vec::with_capacity(request.attachments.len());
        for bytes in request.attachments {
            attachments.push(self.ingest_bytes(&bytes)?);
        }
        let workspace = self.capture_entries(request.workspace)?;
        let provider_state = self.capture_entries(request.provider_state)?;
        let manifest = CheckpointManifest {
            version: 2,
            session: request.session,
            sequence: request.sequence,
            workspace_state: GitWorkspaceState {
                base_commit,
                index_patch,
                worktree_patch,
                required_untracked,
                deleted_paths,
            },
            history,
            attachments,
            workspace,
            provider_state,
            pending_effects: request.pending_effects,
        };
        validate_manifest(&manifest)?;
        let digest = self.publish(&manifest)?;
        Ok(CaptureResult { digest, manifest })
    }

    fn ingest_bytes(&self, bytes: &[u8]) -> io::Result<ArtifactRef> {
        if bytes.len() as u64 > self.max_blob_bytes() {
            return Err(invalid_capture("capture artifact exceeds its blob budget"));
        }
        let artifact = ArtifactRef {
            sha256: format!("{:x}", Sha256::digest(bytes)),
            size_bytes: bytes.len() as u64,
        };
        self.ingest_blob(&artifact, io::Cursor::new(bytes))?;
        Ok(artifact)
    }

    fn capture_entries(&self, entries: Vec<CaptureEntry>) -> io::Result<Vec<WorkspaceEntry>> {
        entries
            .into_iter()
            .map(|entry| {
                validate_path(&entry.path)?;
                if entry.kind == WorkspaceEntryKind::Symlink {
                    let target = std::str::from_utf8(&entry.bytes)
                        .map_err(|_| invalid_capture("capture symlink target is not UTF-8"))?;
                    crate::checkpoint::validate_link(&entry.path, target)?;
                }
                let artifact = self.ingest_bytes(&entry.bytes)?;
                Ok(WorkspaceEntry {
                    path: entry.path,
                    kind: entry.kind,
                    artifact,
                    executable: entry.executable,
                })
            })
            .collect()
    }

    async fn capture_untracked(&self, root: &Path) -> io::Result<Vec<WorkspaceEntry>> {
        let status = git_bytes(
            root,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            self.max_blob_bytes(),
        )
        .await?;
        let mut entries = Vec::new();
        for record in status
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
        {
            if !record.starts_with(b"?? ") {
                continue;
            }
            let path = std::str::from_utf8(&record[3..])
                .map_err(|_| invalid_capture("Git returned a non-UTF-8 untracked path"))?;
            let (kind, bytes, executable) =
                read_workspace_entry(root, path, self.max_blob_bytes())?;
            entries.push(WorkspaceEntry {
                path: path.to_owned(),
                kind,
                artifact: self.ingest_bytes(&bytes)?,
                executable,
            });
        }
        Ok(entries)
    }

    async fn capture_deleted_paths(&self, root: &Path) -> io::Result<BTreeSet<String>> {
        let mut deleted = BTreeSet::new();
        for args in [
            &["diff", "--name-only", "--diff-filter=D", "-z", "--cached"][..],
            &["diff", "--name-only", "--diff-filter=D", "-z"][..],
        ] {
            let output = git_bytes(root, args, self.max_blob_bytes()).await?;
            for raw in output.split(|byte| *byte == 0) {
                if raw.is_empty() {
                    continue;
                }
                let path = std::str::from_utf8(raw)
                    .map_err(|_| invalid_capture("Git returned a non-UTF-8 deleted path"))?;
                validate_path(path)?;
                deleted.insert(path.to_owned());
            }
        }
        Ok(deleted)
    }
}

async fn git_line(root: &Path, args: &[&str], max_output_bytes: u64) -> io::Result<String> {
    let output = git_bytes(root, args, max_output_bytes).await?;
    let line = std::str::from_utf8(&output)
        .map_err(|_| invalid_capture("Git returned non-UTF-8 metadata"))?
        .trim();
    if line.is_empty() || line.contains('\n') {
        return Err(invalid_capture("Git returned an invalid single-line value"));
    }
    Ok(line.to_owned())
}

async fn git_bytes(root: &Path, args: &[&str], max_output_bytes: u64) -> io::Result<Vec<u8>> {
    let mut command = Command::new("git");
    command
        .arg("--no-pager")
        .arg("--no-optional-locks")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = timeout(GIT_COMMAND_DEADLINE, command.output())
        .await
        .map_err(|_| invalid_capture("Git command exceeded its deadline"))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .chars()
            .take(MAX_GIT_STDERR_BYTES)
            .collect::<String>();
        return Err(invalid_capture(format!(
            "Git command failed with status {}: {}",
            output.status, detail
        )));
    }
    if output.stdout.len() as u64 > max_output_bytes {
        return Err(invalid_capture("Git output exceeds the capture budget"));
    }
    Ok(output.stdout)
}

fn read_workspace_entry(
    root: &Path,
    relative: &str,
    max_blob_bytes: u64,
) -> io::Result<(WorkspaceEntryKind, Vec<u8>, bool)> {
    let path = safe_workspace_path(root, relative)?;
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(path)?;
        let target = target
            .to_str()
            .ok_or_else(|| invalid_capture("untracked symlink target is not UTF-8"))?;
        crate::checkpoint::validate_link(relative, target)?;
        return Ok((
            WorkspaceEntryKind::Symlink,
            target.as_bytes().to_vec(),
            false,
        ));
    }
    if !metadata.is_file() || metadata.len() > max_blob_bytes {
        return Err(invalid_capture(
            "untracked entry is not a bounded regular file",
        ));
    }
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(max_blob_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_blob_bytes {
        return Err(invalid_capture("untracked file exceeds its blob budget"));
    }
    #[cfg(unix)]
    let executable = metadata.permissions().mode() & 0o111 != 0;
    #[cfg(not(unix))]
    let executable = false;
    Ok((WorkspaceEntryKind::File, bytes, executable))
}

fn safe_workspace_path(root: &Path, relative: &str) -> io::Result<PathBuf> {
    validate_path(relative)?;
    let mut path = root.to_path_buf();
    let components = relative.split('/').collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        path.push(component);
        let metadata = fs::symlink_metadata(&path)?;
        if index + 1 != components.len()
            && (metadata.file_type().is_symlink() || !metadata.is_dir())
        {
            return Err(invalid_capture("untracked path traverses a symlink"));
        }
    }
    Ok(path)
}

fn commit_valid(commit: &str) -> bool {
    (commit.len() == 40 || commit.len() == 64)
        && commit
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid_capture(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
