//! Isolated Git workspace reconstruction from a verified portable checkpoint.
//!
//! [`CheckpointStore::reconstruct_workspace`] materializes staged, unstaged,
//! deleted and required-untracked state into a new target. It never mutates
//! the source repository, never overwrites an existing target, never fetches
//! remotes, and never starts a provider. [`CheckpointStore::restore`] remains
//! the artifact-only extractor.
//!
//! Git `120000` entries must materialize as real symlinks. A
//! `core.symlinks=false` checkout or apply that writes regular files fails
//! closed instead of returning a misleading tree. Non-Unix hosts are not
//! certified for symlink reconstruction.

use crate::{
    checkpoint::{validate_link, validate_path, CheckpointStore},
    contracts::{CheckpointManifest, WorkspaceEntry, WorkspaceEntryKind},
};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs::{self, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::timeout,
};

const GIT_COMMAND_DEADLINE: Duration = Duration::from_secs(10);
const MAX_GIT_STDERR_BYTES: usize = 64 * 1024;
const MAX_PACK_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SYMLINK_HOPS: u32 = 32;
const MAX_SYMLINK_WORK: u32 = 64;
const MAX_GIT_INDEX_LISTING_BYTES: u64 = 8 * 1024 * 1024;

struct Isolation {
    _dir: TempDir,
    hooks: PathBuf,
    template: PathBuf,
    config: PathBuf,
    patches: PathBuf,
    tmp: PathBuf,
}

struct PatchSymlinkChanges {
    added: Vec<String>,
    removed: Vec<String>,
}

impl Isolation {
    fn new() -> io::Result<Self> {
        let dir = TempDir::new()?;
        let hooks = dir.path().join("hooks");
        let template = dir.path().join("template");
        let patches = dir.path().join("patches");
        fs::create_dir(&hooks)?;
        fs::create_dir(&template)?;
        fs::create_dir(&patches)?;
        let config = dir.path().join("config");
        fs::write(&config, b"")?;
        let tmp = dir.path().join("tmp");
        fs::create_dir(&tmp)?;
        Ok(Self {
            _dir: dir,
            hooks,
            template,
            config,
            patches,
            tmp,
        })
    }
}

impl CheckpointStore {
    /// Reconstruct a Git worktree from a verified checkpoint and a local
    /// repository that already contains the exact base commit objects.
    pub async fn reconstruct_workspace(
        &self,
        digest: &str,
        source_repository: &Path,
        target: &Path,
    ) -> io::Result<CheckpointManifest> {
        let manifest = self.load(digest)?;
        if !manifest.pending_effects.is_empty() {
            return Err(invalid("session requires external-effect reconciliation"));
        }
        let source = strip_windows_verbatim_prefix(
            &fs::canonicalize(source_repository)
                .map_err(|_| invalid("source repository is not a local Git directory"))?,
        );
        if !source.is_dir() {
            return Err(invalid("source repository is not a directory"));
        }
        if let Some(parent) = target.parent() {
            if let Ok(parent) = fs::canonicalize(parent) {
                let parent = strip_windows_verbatim_prefix(&parent);
                if parent == source || parent.starts_with(&source) {
                    return Err(invalid(
                        "reconstruction target may not be created inside the source repository",
                    ));
                }
            }
        }

        let isolation = Isolation::new()?;
        let source_git = match source_git_dir(&isolation, &source).await {
            Ok(path) => path,
            Err(error)
                if error.to_string().contains(
                    "source repository must be the Git workspace root or a bare repository",
                ) =>
            {
                return Err(error);
            }
            Err(_) => {
                return Err(invalid("source repository is not a local Git directory"));
            }
        };
        let base = &manifest.workspace_state.base_commit;
        verify_exact_base_commit(&isolation, &source, &source_git, base).await?;
        let object_format = source_object_format(&isolation, &source, &source_git).await?;

        match fs::create_dir(target) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(invalid("reconstruction target already exists"));
            }
            Err(error) => return Err(error),
        }
        let cleanup = target.to_path_buf();
        let result = self
            .reconstruct_into(
                &manifest,
                &isolation,
                &source,
                &source_git,
                &cleanup,
                &object_format,
            )
            .await;
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&cleanup);
            return Err(error);
        }
        Ok(manifest)
    }

    async fn reconstruct_into(
        &self,
        manifest: &CheckpointManifest,
        isolation: &Isolation,
        source: &Path,
        source_git: &Path,
        target: &Path,
        object_format: &str,
    ) -> io::Result<()> {
        let target = strip_windows_verbatim_prefix(&fs::canonicalize(target)?);
        let source_git = path_to_utf8(source_git)?;
        git(
            isolation,
            &target,
            None,
            &[
                "init",
                "-q",
                "--object-format",
                object_format,
                "--template",
                path_to_utf8(&isolation.template)?,
            ],
            4096,
        )
        .await?;
        let target_git_path = target.join(".git");
        let target_git = path_to_utf8(&target_git_path)?;
        transfer_base_objects(
            isolation,
            source,
            source_git,
            target_git,
            &manifest.workspace_state.base_commit,
            self.max_blob_bytes(),
        )
        .await?;
        git(
            isolation,
            &target,
            None,
            &[
                "--git-dir",
                target_git,
                "update-ref",
                "HEAD",
                &manifest.workspace_state.base_commit,
            ],
            4096,
        )
        .await?;
        git(
            isolation,
            &target,
            None,
            &[
                "--git-dir",
                target_git,
                "--work-tree",
                path_to_utf8(&target)?,
                "checkout",
                "-q",
                "-f",
                "HEAD",
            ],
            4096,
        )
        .await?;
        let head = git_line(
            isolation,
            &target,
            &["rev-parse", "--verify", "--end-of-options", "HEAD"],
        )
        .await?;
        if head != manifest.workspace_state.base_commit {
            return Err(invalid(
                "reconstructed HEAD does not match the checkpoint base commit",
            ));
        }
        reject_escaping_symlinks(&target)?;
        reject_index_bound_symlinks(isolation, &target, &[]).await?;

        let index_patch = read_artifact(self, &manifest.workspace_state.index_patch)?;
        let worktree_patch = read_artifact(self, &manifest.workspace_state.worktree_patch)?;
        validate_git_patch(&index_patch)?;
        validate_git_patch(&worktree_patch)?;
        let index_symlink_changes = patch_symlink_changes(&index_patch)?;
        let worktree_symlink_changes = patch_symlink_changes(&worktree_patch)?;
        let index_path = isolation.patches.join("index.patch");
        let worktree_path = isolation.patches.join("worktree.patch");
        fs::write(&index_path, &index_patch)?;
        fs::write(&worktree_path, &worktree_patch)?;
        git(
            isolation,
            &target,
            None,
            &[
                "apply",
                "--index",
                "--allow-empty",
                "--whitespace=nowarn",
                "--",
                path_to_utf8(&index_path)?,
            ],
            4096,
        )
        .await?;
        reject_escaping_symlinks(&target)?;
        reject_index_bound_symlinks(isolation, &target, &index_symlink_changes.added).await?;
        git(
            isolation,
            &target,
            None,
            &[
                "apply",
                "--allow-empty",
                "--whitespace=nowarn",
                "--",
                path_to_utf8(&worktree_path)?,
            ],
            4096,
        )
        .await?;
        reject_escaping_symlinks(&target)?;
        reject_worktree_symlinks(isolation, &target, &worktree_symlink_changes).await?;
        install_untracked(self, &manifest.workspace_state.required_untracked, &target)?;
        install_untracked(self, &manifest.workspace, &target)?;
        reject_escaping_symlinks(&target)?;
        for path in &manifest.workspace_state.deleted_paths {
            match fs::symlink_metadata(target.join(path)) {
                Ok(_) => {
                    return Err(invalid(format!(
                        "deleted path {path} is still present in the reconstructed worktree"
                    )));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

fn install_untracked(
    store: &CheckpointStore,
    entries: &[WorkspaceEntry],
    root: &Path,
) -> io::Result<()> {
    for entry in entries
        .iter()
        .filter(|entry| entry.kind == WorkspaceEntryKind::File)
    {
        let path = create_safe_path(root, &entry.path)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(if entry.executable { 0o700 } else { 0o600 });
        }
        let mut output = options.open(path)?;
        io::copy(&mut store.open_blob(&entry.artifact)?, &mut output)?;
        output.sync_all()?;
    }
    for entry in entries
        .iter()
        .filter(|entry| entry.kind == WorkspaceEntryKind::Symlink)
    {
        let path = create_safe_path(root, &entry.path)?;
        let mut text = String::new();
        store
            .open_blob(&entry.artifact)?
            .take(4097)
            .read_to_string(&mut text)?;
        validate_link(&entry.path, &text)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&text, &path)?;
        #[cfg(not(unix))]
        {
            let _ = path;
            return Err(invalid("this host has not certified symlink restoration"));
        }
    }
    Ok(())
}

fn create_safe_path(root: &Path, relative: &str) -> io::Result<PathBuf> {
    validate_path(relative)?;
    let mut path = root.to_path_buf();
    let parts = relative.split('/').collect::<Vec<_>>();
    for (index, component) in parts.iter().enumerate() {
        path.push(component);
        let last = index + 1 == parts.len();
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if last {
                    return Err(invalid("path collides with an existing workspace entry"));
                }
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(invalid("untracked path traverses a symlink"));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if last {
                    return Ok(path);
                }
                fs::create_dir(&path)?;
            }
            Err(error) => return Err(error),
        }
    }
    Err(invalid("untracked path is empty"))
}

fn reject_escaping_symlinks(root: &Path) -> io::Result<()> {
    fn walk(root: &Path, rel: &str, work: &mut u32) -> io::Result<()> {
        let dir = if rel.is_empty() {
            root.to_path_buf()
        } else {
            path_from_components(root, &rel_components(rel)?)
        };
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("workspace path is not UTF-8"))?;
            if rel.is_empty() && name.eq_ignore_ascii_case(".git") {
                continue;
            }
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            validate_path(&child)?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                let target = fs::read_link(entry.path())?;
                let target = target
                    .to_str()
                    .ok_or_else(|| invalid("workspace symlink target is not UTF-8"))?;
                validate_link(&child, target)?;
                resolve_symlink_chain(root, &child, target, work)?;
            } else if metadata.is_dir() {
                walk(root, &child, work)?;
            }
        }
        Ok(())
    }
    let mut work = 0;
    walk(root, "", &mut work)
}

fn resolve_symlink_chain(
    root: &Path,
    link_rel: &str,
    target: &str,
    work: &mut u32,
) -> io::Result<()> {
    let mut stack = vec![link_rel.to_string()];
    let _ = follow_symlink_target(
        root,
        parent_components(link_rel)?,
        target,
        &mut stack,
        0,
        work,
    )?;
    Ok(())
}

fn rel_components(rel: &str) -> io::Result<Vec<String>> {
    if rel.is_empty() {
        return Ok(Vec::new());
    }
    let mut comps = Vec::new();
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(invalid("unsafe portable session path component"));
        }
        comps.push(part.to_string());
    }
    Ok(comps)
}

fn parent_components(rel: &str) -> io::Result<Vec<String>> {
    let mut comps = rel_components(rel)?;
    if comps.pop().is_none() {
        return Err(invalid("workspace symlink path is empty"));
    }
    Ok(comps)
}

fn follow_symlink_target(
    root: &Path,
    mut comps: Vec<String>,
    target: &str,
    stack: &mut Vec<String>,
    hops: u32,
    work: &mut u32,
) -> io::Result<Vec<String>> {
    if hops >= MAX_SYMLINK_HOPS {
        return Err(invalid("session symlink chain exceeds resolution bound"));
    }
    if target.is_empty() || target.starts_with('/') {
        return Err(invalid("unsafe session symlink"));
    }
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if comps.pop().is_none() {
                    return Err(invalid("session symlink escapes workspace"));
                }
            }
            name => {
                comps.push(name.to_string());
                let path = path_from_components(root, &comps);
                match fs::symlink_metadata(&path) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        let rel = comps.join("/");
                        if stack.iter().any(|active| active == &rel) {
                            return Err(invalid("session symlink chain is cyclic"));
                        }
                        let next = fs::read_link(&path)?;
                        let next = next
                            .to_str()
                            .ok_or_else(|| invalid("workspace symlink target is not UTF-8"))?;
                        validate_link(&rel, next)?;
                        let mut parent = comps;
                        parent.pop();
                        if *work >= MAX_SYMLINK_WORK {
                            return Err(invalid(
                                "session symlink resolution exceeds reconstruction budget",
                            ));
                        }
                        *work += 1;
                        stack.push(rel);
                        comps = follow_symlink_target(root, parent, next, stack, hops + 1, work)?;
                        stack.pop();
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
        }
    }
    Ok(comps)
}

fn path_from_components(root: &Path, comps: &[String]) -> PathBuf {
    let mut path = root.to_path_buf();
    for comp in comps {
        path.push(comp);
    }
    path
}

async fn reject_index_bound_symlinks(
    isolation: &Isolation,
    target: &Path,
    extra_paths: &[String],
) -> io::Result<()> {
    let mut paths = stream_index_symlink_paths(isolation, target).await?;
    #[cfg(not(unix))]
    if !paths.is_empty() || !extra_paths.is_empty() {
        return Err(invalid("this host has not certified symlink restoration"));
    }
    for path in extra_paths {
        if !paths.iter().any(|existing| existing == path) {
            paths.push(path.clone());
        }
    }
    reject_unmaterialized_symlink_paths(target, &paths)
}

async fn reject_worktree_symlinks(
    isolation: &Isolation,
    target: &Path,
    worktree: &PatchSymlinkChanges,
) -> io::Result<()> {
    let index_paths = stream_index_symlink_paths(isolation, target).await?;
    #[cfg(not(unix))]
    if !index_paths.is_empty() || !worktree.added.is_empty() {
        return Err(invalid("this host has not certified symlink restoration"));
    }
    let mut expect = index_paths;
    for path in &worktree.removed {
        expect.retain(|existing| existing != path);
    }
    for path in &worktree.added {
        if !expect.iter().any(|existing| existing == path) {
            expect.push(path.clone());
        }
    }
    reject_unmaterialized_symlink_paths(target, &expect)
}

async fn stream_index_symlink_paths(
    isolation: &Isolation,
    target: &Path,
) -> io::Result<Vec<String>> {
    let mut command = git_command(isolation, target);
    command.args(["ls-files", "-s", "-z"]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("Git stdout pipe was not created"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| invalid("Git stderr pipe was not created"))?;
    let result = timeout(GIT_COMMAND_DEADLINE, async {
        tokio::try_join!(
            read_index_symlink_paths(&mut stdout),
            read_git_stderr(&mut stderr),
            child.wait(),
        )
    })
    .await;
    let (paths, stderr, status) = match result {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error);
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(invalid(
                "Git command exceeded its deadline (ls-files -s -z)",
            ));
        }
    };
    if !status.success() {
        return Err(invalid(format!(
            "Git command failed with status {}: {}",
            status, stderr
        )));
    }
    Ok(paths)
}

async fn read_index_symlink_paths<R>(reader: &mut R) -> io::Result<Vec<String>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut paths = Vec::new();
    let mut pending = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut scanned = 0_u64;
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        scanned = scanned.saturating_add(count as u64);
        if scanned > MAX_GIT_INDEX_LISTING_BYTES {
            return Err(invalid(
                "Git index listing exceeds the reconstruction metadata budget",
            ));
        }
        pending.extend_from_slice(&buffer[..count]);
        drain_index_symlink_records(&mut pending, &mut paths)?;
    }
    if !pending.is_empty() {
        return Err(invalid("Git ls-files stage output is malformed"));
    }
    Ok(paths)
}

fn drain_index_symlink_records(pending: &mut Vec<u8>, paths: &mut Vec<String>) -> io::Result<()> {
    while let Some(end) = pending.iter().position(|byte| *byte == 0) {
        let mut record = pending.drain(..=end).collect::<Vec<_>>();
        record.pop();
        if record.is_empty() {
            continue;
        }
        if let Some(path) = git_symlink_index_record(&record)? {
            paths.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
fn git_symlink_index_paths(output: &[u8]) -> io::Result<Vec<String>> {
    let mut paths = Vec::new();
    let mut pending = output.to_vec();
    drain_index_symlink_records(&mut pending, &mut paths)?;
    if !pending.is_empty() && !pending.iter().all(|byte| *byte == 0) {
        return Err(invalid("Git ls-files stage output is malformed"));
    }
    Ok(paths)
}

fn git_symlink_index_record(record: &[u8]) -> io::Result<Option<String>> {
    let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
        return Err(invalid("Git ls-files stage output is malformed"));
    };
    let meta = std::str::from_utf8(&record[..tab])
        .map_err(|_| invalid("Git ls-files stage output is not UTF-8"))?;
    let mode = meta.split(' ').next().unwrap_or("");
    let path = std::str::from_utf8(&record[tab + 1..])
        .map_err(|_| invalid("Git ls-files path is not UTF-8"))?;
    if path.is_empty() {
        return Err(invalid("Git ls-files path is empty"));
    }
    validate_path(path)?;
    if mode == "120000" {
        Ok(Some(path.to_string()))
    } else {
        Ok(None)
    }
}

fn patch_symlink_changes(patch: &[u8]) -> io::Result<PatchSymlinkChanges> {
    if patch.is_empty() {
        return Ok(PatchSymlinkChanges {
            added: Vec::new(),
            removed: Vec::new(),
        });
    }
    if patch.contains(&0) {
        return Err(invalid("git patch contains a NUL byte"));
    }
    let text = std::str::from_utf8(patch).map_err(|_| invalid("git patch is not UTF-8"))?;
    let mut in_hunk = false;
    let mut current: Option<String> = None;
    let mut new_file_mode: Option<String> = None;
    let mut deleted_file_mode: Option<String> = None;
    let mut old_mode: Option<String> = None;
    let mut new_mode: Option<String> = None;
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            record_symlink_mode_changes(
                current.as_deref(),
                new_file_mode.as_deref(),
                deleted_file_mode.as_deref(),
                old_mode.as_deref(),
                new_mode.as_deref(),
                &mut added,
                &mut removed,
            );
            in_hunk = false;
            new_file_mode = None;
            deleted_file_mode = None;
            old_mode = None;
            new_mode = None;
            let (_, right) = parse_diff_git_paths(rest)?;
            let relative = right
                .strip_prefix("b/")
                .or_else(|| right.strip_prefix("a/"))
                .ok_or_else(|| invalid("git patch path must use a/ and b/ prefixes"))?;
            validate_path(relative)?;
            current = Some(relative.to_string());
        } else if line.starts_with("@@") {
            in_hunk = true;
        } else if in_hunk {
            continue;
        } else if let Some(mode) = line.strip_prefix("new file mode ") {
            new_file_mode = Some(mode.to_string());
        } else if let Some(mode) = line.strip_prefix("deleted file mode ") {
            deleted_file_mode = Some(mode.to_string());
        } else if let Some(mode) = line.strip_prefix("old mode ") {
            old_mode = Some(mode.to_string());
        } else if let Some(mode) = line.strip_prefix("new mode ") {
            new_mode = Some(mode.to_string());
        }
    }
    record_symlink_mode_changes(
        current.as_deref(),
        new_file_mode.as_deref(),
        deleted_file_mode.as_deref(),
        old_mode.as_deref(),
        new_mode.as_deref(),
        &mut added,
        &mut removed,
    );
    Ok(PatchSymlinkChanges { added, removed })
}

fn record_symlink_mode_changes(
    current: Option<&str>,
    new_file_mode: Option<&str>,
    deleted_file_mode: Option<&str>,
    old_mode: Option<&str>,
    new_mode: Option<&str>,
    added: &mut Vec<String>,
    removed: &mut Vec<String>,
) {
    let Some(path) = current else {
        return;
    };
    let becomes = new_file_mode == Some("120000") || new_mode == Some("120000");
    let leaves = deleted_file_mode == Some("120000")
        || (old_mode == Some("120000") && new_mode.is_some() && new_mode != Some("120000"));
    if becomes && !added.iter().any(|existing| existing == path) {
        added.push(path.to_string());
    }
    if leaves && !removed.iter().any(|existing| existing == path) {
        removed.push(path.to_string());
    }
}

fn reject_unmaterialized_symlink_paths(root: &Path, paths: &[String]) -> io::Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    #[cfg(not(unix))]
    {
        let _ = root;
        return Err(invalid("this host has not certified symlink restoration"));
    }
    #[cfg(unix)]
    {
        for rel in paths {
            let path = path_from_components(root, &rel_components(rel)?);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {}
                Ok(_) => {
                    return Err(invalid("this host has not certified symlink restoration"));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Err(invalid("this host has not certified symlink restoration"));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

fn validate_git_patch(patch: &[u8]) -> io::Result<()> {
    if patch.is_empty() {
        return Ok(());
    }
    if patch.contains(&0) {
        return Err(invalid("git patch contains a NUL byte"));
    }
    let text = std::str::from_utf8(patch).map_err(|_| invalid("git patch is not UTF-8"))?;
    let mut in_hunk = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            in_hunk = false;
            let (left, right) = parse_diff_git_paths(rest)?;
            validate_diff_path(&left)?;
            validate_diff_path(&right)?;
        } else if line.starts_with("@@") {
            in_hunk = true;
        } else if in_hunk {
            continue;
        } else if let Some(path) = line.strip_prefix("rename from ") {
            validate_path(&unquote_patch_path(path)?)?;
        } else if let Some(path) = line.strip_prefix("rename to ") {
            validate_path(&unquote_patch_path(path)?)?;
        } else if let Some(path) = line.strip_prefix("copy from ") {
            validate_path(&unquote_patch_path(path)?)?;
        } else if let Some(path) = line.strip_prefix("copy to ") {
            validate_path(&unquote_patch_path(path)?)?;
        } else if let Some(value) = line.strip_prefix("--- ") {
            validate_patch_header_path(value)?;
        } else if let Some(value) = line.strip_prefix("+++ ") {
            validate_patch_header_path(value)?;
        }
    }
    Ok(())
}

fn validate_patch_header_path(value: &str) -> io::Result<()> {
    let path = value.split('\t').next().unwrap_or(value);
    if path == "/dev/null" {
        return Ok(());
    }
    validate_diff_path(&unquote_patch_path(path)?)
}

fn unquote_patch_path(value: &str) -> io::Result<String> {
    if !value.starts_with('"') {
        return Ok(value.to_string());
    }
    let (parsed, rest) = parse_one_diff_path(value)?;
    if !rest.is_empty() {
        return Err(invalid("git patch path has trailing data"));
    }
    Ok(parsed)
}

fn validate_diff_path(path: &str) -> io::Result<()> {
    let relative = path
        .strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .ok_or_else(|| invalid("git patch path must use a/ and b/ prefixes"))?;
    if relative.is_empty() {
        return Err(invalid("git patch path is empty"));
    }
    validate_path(relative)
}

fn parse_diff_git_paths(rest: &str) -> io::Result<(String, String)> {
    let (left, rest) = parse_one_diff_path(rest)?;
    let rest = rest
        .strip_prefix(' ')
        .ok_or_else(|| invalid("git patch is missing the destination path"))?;
    let (right, rest) = parse_one_diff_path(rest)?;
    if !rest.is_empty() {
        return Err(invalid("git patch diff header has trailing path data"));
    }
    Ok((left, right))
}

fn find_unquoted_b_prefix(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut found = None;
    let mut index = 0;
    while index + 3 <= bytes.len() {
        if bytes[index] == b' ' && bytes[index + 1] == b'b' && bytes[index + 2] == b'/' {
            let left = &rest[..index];
            let right = &rest[index + 1..];
            if left.starts_with("a/") && right.starts_with("b/") {
                found = Some(index);
            }
        }
        index += 1;
    }
    found
}

fn parse_one_diff_path(input: &str) -> io::Result<(String, &str)> {
    let Some(rest) = input.strip_prefix('"') else {
        if input.starts_with("b/") {
            return Ok((input.to_string(), ""));
        }
        if input.starts_with("a/") {
            if let Some(index) = input.find(" \"") {
                return Ok((input[..index].to_string(), &input[index..]));
            }
            let split = find_unquoted_b_prefix(input)
                .ok_or_else(|| invalid("git patch is missing the destination path"))?;
            return Ok((input[..split].to_string(), &input[split..]));
        }
        return Err(invalid("git patch path must use a/ and b/ prefixes"));
    };
    let mut out = Vec::new();
    let bytes = rest.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let path =
                    String::from_utf8(out).map_err(|_| invalid("git patch path is not UTF-8"))?;
                return Ok((path, &rest[index + 1..]));
            }
            b'\\' => {
                index += 1;
                if index >= bytes.len() {
                    return Err(invalid("unterminated git patch path escape"));
                }
                match bytes[index] {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    b'\\' => out.push(b'\\'),
                    b'"' => out.push(b'"'),
                    b'0'..=b'7' => {
                        let mut value = u16::from(bytes[index] - b'0');
                        let mut consumed = 1;
                        while consumed < 3
                            && index + consumed < bytes.len()
                            && (b'0'..=b'7').contains(&bytes[index + consumed])
                        {
                            value = value * 8 + u16::from(bytes[index + consumed] - b'0');
                            consumed += 1;
                        }
                        let encoded = u8::try_from(value)
                            .map_err(|_| invalid("git patch path octal escape is out of range"))?;
                        out.push(encoded);
                        index += consumed - 1;
                    }
                    _ => return Err(invalid("unsupported git patch path escape")),
                }
            }
            byte => out.push(byte),
        }
        index += 1;
    }
    Err(invalid("unterminated quoted git patch path"))
}

async fn source_git_dir(isolation: &Isolation, source: &Path) -> io::Result<PathBuf> {
    let is_bare = git_line(isolation, source, &["rev-parse", "--is-bare-repository"]).await?;
    if is_bare != "true" {
        let prefix = git(
            isolation,
            source,
            None,
            &["rev-parse", "--show-prefix"],
            4096,
        )
        .await?;
        let prefix = std::str::from_utf8(&prefix)
            .map_err(|_| invalid("Git returned non-UTF-8 metadata"))?
            .trim();
        if !prefix.is_empty() {
            return Err(invalid(
                "source repository must be the Git workspace root or a bare repository",
            ));
        }
    }
    Ok(PathBuf::from(
        git_line(isolation, source, &["rev-parse", "--absolute-git-dir"]).await?,
    ))
}

async fn verify_exact_base_commit(
    isolation: &Isolation,
    source: &Path,
    source_git: &Path,
    base: &str,
) -> io::Result<()> {
    let source_git = path_to_utf8(source_git)?;
    let peeled = format!("{base}^{{commit}}");
    let kind = git_line(
        isolation,
        source,
        &["--git-dir", source_git, "cat-file", "-t", &peeled],
    )
    .await
    .map_err(missing_base)?;
    if kind != "commit" {
        return Err(invalid(
            "source repository does not contain the checkpoint base commit",
        ));
    }
    let resolved = git_line(
        isolation,
        source,
        &[
            "--git-dir",
            source_git,
            "rev-parse",
            "--verify",
            "--end-of-options",
            &peeled,
        ],
    )
    .await
    .map_err(missing_base)?;
    if resolved != base {
        return Err(invalid(
            "source repository resolved a different object than the checkpoint base commit",
        ));
    }
    Ok(())
}

async fn source_object_format(
    isolation: &Isolation,
    source: &Path,
    source_git: &Path,
) -> io::Result<String> {
    let source_git = path_to_utf8(source_git)?;
    let format = git_line(
        isolation,
        source,
        &["--git-dir", source_git, "rev-parse", "--show-object-format"],
    )
    .await
    .map_err(|_| invalid("source repository Git object format could not be determined"))?;
    if format != "sha1" && format != "sha256" {
        return Err(invalid(format!("unsupported Git object format {format}")));
    }
    Ok(format)
}

async fn transfer_base_objects(
    isolation: &Isolation,
    source: &Path,
    source_git: &str,
    target_git: &str,
    base: &str,
    max_blob_bytes: u64,
) -> io::Result<()> {
    let budget = MAX_PACK_BYTES.max(max_blob_bytes);
    let stdin = format!("{base}\n");
    let pack = git(
        isolation,
        source,
        Some(stdin.as_bytes()),
        &[
            "--git-dir",
            source_git,
            "pack-objects",
            "--revs",
            "--stdout",
            "--quiet",
        ],
        budget,
    )
    .await?;
    // Keep the transferred pack as a packfile. `unpack-objects` explodes one
    // loose object per blob and exceeded the 10s Git deadline on Windows for
    // the 1200-file index-listing fixture (`unpack-objects -q`).
    git(
        isolation,
        source,
        Some(&pack),
        &[
            "--git-dir",
            target_git,
            "index-pack",
            "-q",
            "--strict",
            "--stdin",
        ],
        4096,
    )
    .await?;
    let kind = git_line(
        isolation,
        source,
        &["--git-dir", target_git, "cat-file", "-t", base],
    )
    .await?;
    if kind != "commit" {
        return Err(invalid(
            "reconstructed repository is missing the checkpoint base commit",
        ));
    }
    Ok(())
}

async fn git_line(isolation: &Isolation, current_dir: &Path, args: &[&str]) -> io::Result<String> {
    let output = git(isolation, current_dir, None, args, 4096).await?;
    let line = std::str::from_utf8(&output)
        .map_err(|_| invalid("Git returned non-UTF-8 metadata"))?
        .trim();
    if line.is_empty() || line.contains('\n') {
        return Err(invalid("Git returned an invalid single-line value"));
    }
    Ok(line.to_owned())
}

fn git_command(isolation: &Isolation, current_dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    #[cfg(windows)]
    {
        for key in ["SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT", "SYSTEMDRIVE"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command.env("USERPROFILE", &isolation.tmp);
    }
    command
        .env("TMPDIR", &isolation.tmp)
        .env("TMP", &isolation.tmp)
        .env("TEMP", &isolation.tmp)
        .env("HOME", &isolation.tmp)
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", &isolation.config)
        .env("GIT_CONFIG_SYSTEM", &isolation.config)
        .env("GIT_TEMPLATE_DIR", &isolation.template)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_PROTOCOL_FROM_USER", "0")
        .env("GIT_ALLOW_PROTOCOL", "")
        .env("GIT_ASKPASS", "")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env(
            "GIT_CEILING_DIRECTORIES",
            strip_windows_verbatim_prefix(
                &fs::canonicalize(current_dir).unwrap_or_else(|_| current_dir.to_path_buf()),
            ),
        )
        .arg("--no-pager")
        .arg("--no-optional-locks")
        .arg("--no-replace-objects")
        .arg("-c")
        .arg(format!("core.hooksPath={}", isolation.hooks.display()))
        .arg("-c")
        .arg("core.fsmonitor=")
        .arg("-c")
        .arg("core.useBuiltinFSMonitor=false")
        .arg("-c")
        .arg("core.autocrlf=false")
        .arg("-c")
        .arg("core.safecrlf=false")
        .arg("-c")
        .arg("core.sshCommand=/usr/bin/false")
        .arg("-c")
        .arg("credential.helper=")
        .arg("-c")
        .arg("protocol.ext.allow=never")
        .arg("-c")
        .arg("diff.external=")
        .arg("-c")
        .arg("filter.lfs.smudge=")
        .arg("-c")
        .arg("filter.lfs.clean=")
        .arg("-c")
        .arg("filter.lfs.process=")
        .arg("-c")
        .arg("filter.lfs.required=false")
        .arg("-c")
        .arg("pack.threads=2")
        .arg("-c")
        .arg(if cfg!(unix) {
            "core.symlinks=true"
        } else {
            "core.symlinks=false"
        })
        .arg("-c")
        .arg(format!("init.templateDir={}", isolation.template.display()))
        .current_dir(strip_windows_verbatim_prefix(current_dir));
    command
}

async fn git(
    isolation: &Isolation,
    current_dir: &Path,
    stdin: Option<&[u8]>,
    args: &[&str],
    max_output_bytes: u64,
) -> io::Result<Vec<u8>> {
    let mut command = git_command(isolation, current_dir);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command.spawn()?;
    let mut stdin_pipe = child.stdin.take();
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("Git stdout pipe was not created"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| invalid("Git stderr pipe was not created"))?;
    let stdin_bytes = stdin.map(|bytes| bytes.to_vec());
    let result = timeout(GIT_COMMAND_DEADLINE, async {
        tokio::try_join!(
            async {
                if let (Some(bytes), Some(mut pipe)) = (stdin_bytes.as_deref(), stdin_pipe.take()) {
                    pipe.write_all(bytes).await?;
                    pipe.shutdown().await?;
                }
                Ok::<(), io::Error>(())
            },
            read_git_stdout(&mut stdout, max_output_bytes),
            read_git_stderr(&mut stderr),
            child.wait(),
        )
    })
    .await;
    let (_in, stdout, stderr, status) = match result {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error);
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(invalid(format!(
                "Git command exceeded its deadline ({})",
                args.join(" "),
            )));
        }
    };
    if !status.success() {
        return Err(invalid(format!(
            "Git command failed with status {}: {}",
            status, stderr
        )));
    }
    Ok(stdout)
}

async fn read_git_stdout<R>(reader: &mut R, max_output_bytes: u64) -> io::Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            return Ok(bytes);
        }
        if bytes.len() as u64 + count as u64 > max_output_bytes {
            return Err(invalid("Git output exceeds the reconstruction budget"));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

async fn read_git_stderr<R>(reader: &mut R) -> io::Result<String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            return Ok(String::from_utf8_lossy(&bytes).into_owned());
        }
        let remaining = MAX_GIT_STDERR_BYTES.saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..count.min(remaining)]);
    }
}

fn read_artifact(
    store: &CheckpointStore,
    artifact: &crate::contracts::ArtifactRef,
) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    store.open_blob(artifact)?.read_to_end(&mut bytes)?;
    if bytes.len() as u64 != artifact.size_bytes {
        return Err(invalid(
            "checkpoint artifact length changed during reconstruction",
        ));
    }
    Ok(bytes)
}

fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        let mut components = path.components();
        if let Some(Component::Prefix(prefix)) = components.next() {
            let mut stripped = match prefix.kind() {
                Prefix::VerbatimDisk(disk) => PathBuf::from(format!(r"{}:\", disk as char)),
                Prefix::VerbatimUNC(server, share) => {
                    let mut root = PathBuf::from(r"\\");
                    root.push(server);
                    root.push(share);
                    root
                }
                _ => return path.to_path_buf(),
            };
            stripped
                .extend(components.filter(|component| !matches!(component, Component::RootDir)));
            return stripped;
        }
    }
    path.to_path_buf()
}

fn path_to_utf8(path: &Path) -> io::Result<&str> {
    path.to_str().ok_or_else(|| invalid("path is not UTF-8"))
}

fn missing_base(_: io::Error) -> io::Error {
    invalid("source repository does not contain the checkpoint base commit")
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod unmaterialized_symlink_tests {
    use super::*;

    #[test]
    fn reconstruct_rejects_regular_file_recorded_as_git_symlink() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("link"), b"tracked.txt").unwrap();
        let error =
            reject_unmaterialized_symlink_paths(dir.path(), &["link".to_string()]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("has not certified symlink restoration"),
            "{error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn reconstruct_accepts_real_symlink_materialization() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("tracked.txt", dir.path().join("link")).unwrap();
        reject_unmaterialized_symlink_paths(dir.path(), &["link".to_string()]).unwrap();
    }

    #[test]
    fn reconstruct_parses_stage_output_symlink_paths() {
        let output = b"100644 0123456789abcdef0123456789abcdef01234567 0\ttracked.txt\0\
120000 abcdef0123456789abcdef0123456789abcdef01 0\tlink\0";
        assert_eq!(
            git_symlink_index_paths(output).unwrap(),
            vec!["link".to_string()]
        );
    }

    #[test]
    fn reconstruct_parses_patch_symlink_modes() {
        let patch = b"diff --git a/link b/link\n\
new file mode 120000\n\
index 0000000..a96aa0e\n\
--- /dev/null\n\
+++ b/link\n\
@@ -0,0 +1 @@\n\
+tracked.txt\n";
        let changes = patch_symlink_changes(patch).unwrap();
        assert_eq!(changes.added, vec!["link".to_string()]);
        assert!(changes.removed.is_empty());
    }

    #[test]
    fn reconstruct_parses_worktree_symlink_deletion_and_demotion() {
        let deleted = b"diff --git a/link b/link\n\
deleted file mode 120000\n\
index a96aa0e..0000000\n\
--- a/link\n\
+++ /dev/null\n\
@@ -1 +0,0 @@\n\
-tracked.txt\n";
        let changes = patch_symlink_changes(deleted).unwrap();
        assert!(changes.added.is_empty());
        assert_eq!(changes.removed, vec!["link".to_string()]);

        let demoted = b"diff --git a/link b/link\n\
old mode 120000\n\
new mode 100644\n\
index a96aa0e..9daeafb\n\
--- a/link\n\
+++ b/link\n\
@@ -1 +1 @@\n\
-tracked.txt\n\
+now a file\n";
        let changes = patch_symlink_changes(demoted).unwrap();
        assert!(changes.added.is_empty());
        assert_eq!(changes.removed, vec!["link".to_string()]);
    }

    #[test]
    fn reconstruct_filters_symlink_paths_from_index_listing_over_64kib() {
        let mut listing = Vec::new();
        for i in 0..1200 {
            listing.extend(format!("100644 {:040x} 0\tbulk/{i:04}.txt\0", i).into_bytes());
        }
        listing.extend(b"120000 abcdef0123456789abcdef0123456789abcdef01 0\tlink\0");
        assert!(
            listing.len() > 64 * 1024,
            "listing was {} bytes",
            listing.len()
        );
        assert_eq!(
            git_symlink_index_paths(&listing).unwrap(),
            vec!["link".to_string()]
        );
    }

    #[test]
    fn reconstruct_parses_mixed_quote_git_rename_headers() {
        let (left, right) = parse_diff_git_paths(r#"a/old.txt "b/\303\251.txt""#).unwrap();
        assert_eq!(left, "a/old.txt");
        assert_eq!(right, "b/é.txt");

        let (left, right) =
            parse_diff_git_paths(r#""a/\303\251.txt" b/file with space.txt"#).unwrap();
        assert_eq!(left, "a/é.txt");
        assert_eq!(right, "b/file with space.txt");
    }

    #[test]
    fn reconstruct_strips_windows_verbatim_prefixes_for_git() {
        let disk = PathBuf::from(r"\\?\C:\tmp\workspace");
        let stripped = strip_windows_verbatim_prefix(&disk);
        #[cfg(windows)]
        assert_eq!(stripped, PathBuf::from(r"C:\tmp\workspace"));
        #[cfg(not(windows))]
        assert_eq!(stripped, disk);

        let unc = PathBuf::from(r"\\?\UNC\server\share\repo");
        let stripped = strip_windows_verbatim_prefix(&unc);
        #[cfg(windows)]
        assert_eq!(stripped, PathBuf::from(r"\\server\share\repo"));
        #[cfg(not(windows))]
        assert_eq!(stripped, unc);
    }
}
