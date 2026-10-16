use std::collections::BTreeMap;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use tempfile::Builder;

use crate::GhostCommit;
use crate::GitToolingError;
use crate::operations::apply_repo_prefix_to_force_include;
use crate::operations::ensure_git_repository;
use crate::operations::normalize_relative_path;
use crate::operations::repo_subdir;
use crate::operations::resolve_head;
use crate::operations::resolve_repository_root;
use crate::operations::run_git_for_status;
use crate::operations::run_git_for_stdout;
use crate::operations::run_git_for_stdout_all;

/// Default commit message used for ghost commits when none is provided.
const DEFAULT_COMMIT_MESSAGE: &str = "codex snapshot";
/// Default threshold for ignoring large untracked directories.
const DEFAULT_IGNORE_LARGE_UNTRACKED_DIRS: i64 = 200;
/// Default threshold (10 MiB) for excluding large untracked files from ghost snapshots.
const DEFAULT_IGNORE_LARGE_UNTRACKED_FILES: i64 = 10 * 1024 * 1024;
/// Directories that should always be ignored when capturing ghost snapshots,
/// even if they are not listed in .gitignore.
///
/// These are typically large dependency or build trees that are not useful
/// for undo and can cause snapshots to grow without bound.
const DEFAULT_IGNORED_DIR_NAMES: &[&str] = &[
    "node_modules",
    ".venv",
    "venv",
    "env",
    ".env",
    "dist",
    "build",
    ".pytest_cache",
    ".mypy_cache",
    ".cache",
    ".tox",
    "__pycache__",
];

/// Options to control ghost commit creation.
pub struct CreateGhostCommitOptions<'a> {
    pub repo_path: &'a Path,
    pub message: Option<&'a str>,
    pub force_include: Vec<PathBuf>,
    pub ghost_snapshot: GhostSnapshotConfig,
}

/// Options to control ghost commit restoration.
pub struct RestoreGhostCommitOptions<'a> {
    pub repo_path: &'a Path,
    pub ghost_snapshot: GhostSnapshotConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhostSnapshotConfig {
    pub ignore_large_untracked_files: Option<i64>,
    pub ignore_large_untracked_dirs: Option<i64>,
    pub disable_warnings: bool,
}

impl Default for GhostSnapshotConfig {
    fn default() -> Self {
        Self {
            ignore_large_untracked_files: Some(DEFAULT_IGNORE_LARGE_UNTRACKED_FILES),
            ignore_large_untracked_dirs: Some(DEFAULT_IGNORE_LARGE_UNTRACKED_DIRS),
            disable_warnings: false,
        }
    }
}

/// Summary produced alongside a ghost snapshot.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GhostSnapshotReport {
    pub large_untracked_dirs: Vec<LargeUntrackedDir>,
    pub ignored_untracked_files: Vec<IgnoredUntrackedFile>,
}

/// Directory containing a large amount of untracked content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeUntrackedDir {
    pub path: PathBuf,
    pub file_count: i64,
}

/// Untracked file excluded from the snapshot because of its size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoredUntrackedFile {
    pub path: PathBuf,
    pub byte_size: i64,
}

impl<'a> CreateGhostCommitOptions<'a> {
    /// Creates options scoped to the provided repository path.
    pub fn new(repo_path: &'a Path) -> Self {
        Self {
            repo_path,
            message: None,
            force_include: Vec::new(),
            ghost_snapshot: GhostSnapshotConfig::default(),
        }
    }

    /// Sets a custom commit message for the ghost commit.
    pub fn message(mut self, message: &'a str) -> Self {
        self.message = Some(message);
        self
    }

    pub fn ghost_snapshot(mut self, ghost_snapshot: GhostSnapshotConfig) -> Self {
        self.ghost_snapshot = ghost_snapshot;
        self
    }

    /// Exclude untracked files larger than `bytes` from the snapshot commit.
    ///
    /// These files are still treated as untracked for preservation purposes (i.e. they will not be
    /// deleted by undo), but they will not be captured in the snapshot tree.
    pub fn ignore_large_untracked_files(mut self, bytes: i64) -> Self {
        if bytes > 0 {
            self.ghost_snapshot.ignore_large_untracked_files = Some(bytes);
        } else {
            self.ghost_snapshot.ignore_large_untracked_files = None;
        }
        self
    }

    /// Supplies the entire force-include path list at once.
    pub fn force_include<I>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = PathBuf>,
    {
        self.force_include = paths.into_iter().collect();
        self
    }

    /// Adds a single path to the force-include list.
    pub fn push_force_include<P>(mut self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.force_include.push(path.into());
        self
    }
}

impl<'a> RestoreGhostCommitOptions<'a> {
    /// Creates restore options scoped to the provided repository path.
    pub fn new(repo_path: &'a Path) -> Self {
        Self {
            repo_path,
            ghost_snapshot: GhostSnapshotConfig::default(),
        }
    }

    pub fn ghost_snapshot(mut self, ghost_snapshot: GhostSnapshotConfig) -> Self {
        self.ghost_snapshot = ghost_snapshot;
        self
    }

    /// Exclude untracked files larger than `bytes` from undo cleanup.
    ///
    /// These files are treated as "always preserve" to avoid deleting large local artifacts.
    pub fn ignore_large_untracked_files(mut self, bytes: i64) -> Self {
        if bytes > 0 {
            self.ghost_snapshot.ignore_large_untracked_files = Some(bytes);
        } else {
            self.ghost_snapshot.ignore_large_untracked_files = None;
        }
        self
    }

    /// Ignore untracked directories that contain at least `file_count` untracked files.
    pub fn ignore_large_untracked_dirs(mut self, file_count: i64) -> Self {
        if file_count > 0 {
            self.ghost_snapshot.ignore_large_untracked_dirs = Some(file_count);
        } else {
            self.ghost_snapshot.ignore_large_untracked_dirs = None;
        }
        self
    }
}

fn detect_large_untracked_dirs(
    files: &[PathBuf],
    dirs: &[PathBuf],
    threshold: Option<i64>,
) -> Vec<LargeUntrackedDir> {
    let Some(threshold) = threshold else {
        return Vec::new();
    };
    if threshold <= 0 {
        return Vec::new();
    }

    let mut counts: BTreeMap<PathBuf, i64> = BTreeMap::new();

    let mut sorted_dirs: Vec<&PathBuf> = dirs.iter().collect();
    sorted_dirs.sort_by(|a, b| {
        let a_components = a.components().count();
        let b_components = b.components().count();
        b_components.cmp(&a_components).then_with(|| a.cmp(b))
    });

    for file in files {
        let mut key: Option<PathBuf> = None;
        for dir in &sorted_dirs {
            if file.starts_with(dir.as_path()) {
                key = Some((*dir).clone());
                break;
            }
        }
        let key = key.unwrap_or_else(|| {
            file.parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
        });
        let entry = counts.entry(key).or_insert(0);
        *entry += 1;
    }

    let mut result: Vec<LargeUntrackedDir> = counts
        .into_iter()
        .filter(|(_, count)| *count >= threshold)
        .map(|(path, file_count)| LargeUntrackedDir { path, file_count })
        .collect();
    result.sort_by(|a, b| {
        b.file_count
            .cmp(&a.file_count)
            .then_with(|| a.path.cmp(&b.path))
    });
    result
}

fn to_session_relative_path(path: &Path, repo_prefix: Option<&Path>) -> PathBuf {
    match repo_prefix {
        Some(prefix) => path
            .strip_prefix(prefix)
            .map(PathBuf::from)
            .unwrap_or_else(|_| path.to_path_buf()),
        None => path.to_path_buf(),
    }
}

/// Create a ghost commit capturing the current state of the repository's working tree.
pub fn create_ghost_commit(
    options: &CreateGhostCommitOptions<'_>,
) -> Result<GhostCommit, GitToolingError> {
    create_ghost_commit_with_report(options).map(|(commit, _)| commit)
}

/// Compute a report describing the working tree for a ghost snapshot without creating a commit.
pub fn capture_ghost_snapshot_report(
    options: &CreateGhostCommitOptions<'_>,
) -> Result<GhostSnapshotReport, GitToolingError> {
    ensure_git_repository(options.repo_path)?;

    let repo_root = resolve_repository_root(options.repo_path)?;
    let repo_prefix = repo_subdir(repo_root.as_path(), options.repo_path);
    let force_include = prepare_force_include(repo_prefix.as_deref(), &options.force_include)?;
    let existing_untracked = capture_existing_untracked(
        repo_root.as_path(),
        repo_prefix.as_deref(),
        options.ghost_snapshot.ignore_large_untracked_files,
        options.ghost_snapshot.ignore_large_untracked_dirs,
        &force_include,
    )?;

    let warning_ignored_files = existing_untracked
        .ignored_untracked_files
        .iter()
        .map(|file| IgnoredUntrackedFile {
            path: to_session_relative_path(file.path.as_path(), repo_prefix.as_deref()),
            byte_size: file.byte_size,
        })
        .collect::<Vec<_>>();
    let warning_ignored_dirs = existing_untracked
        .ignored_large_untracked_dirs
        .iter()
        .map(|dir| LargeUntrackedDir {
            path: to_session_relative_path(dir.path.as_path(), repo_prefix.as_deref()),
            file_count: dir.file_count,
        })
        .collect::<Vec<_>>();

    Ok(GhostSnapshotReport {
        large_untracked_dirs: warning_ignored_dirs,
        ignored_untracked_files: warning_ignored_files,
    })
}

/// Create a ghost commit capturing the current state of the repository's working tree along with a report.
pub fn create_ghost_commit_with_report(
    options: &CreateGhostCommitOptions<'_>,
) -> Result<(GhostCommit, GhostSnapshotReport), GitToolingError> {
    ensure_git_repository(options.repo_path)?;

    let repo_root = resolve_repository_root(options.repo_path)?;
    let repo_prefix = repo_subdir(repo_root.as_path(), options.repo_path);
    let parent = resolve_head(repo_root.as_path())?;
    let force_include = prepare_force_include(repo_prefix.as_deref(), &options.force_include)?;
    let status_snapshot = capture_status_snapshot(
        repo_root.as_path(),
        repo_prefix.as_deref(),
        options.ghost_snapshot.ignore_large_untracked_files,
        options.ghost_snapshot.ignore_large_untracked_dirs,
        &force_include,
    )?;
    let existing_untracked = status_snapshot.untracked;

    let warning_ignored_files = existing_untracked
        .ignored_untracked_files
        .iter()
        .map(|file| IgnoredUntrackedFile {
            path: to_session_relative_path(file.path.as_path(), repo_prefix.as_deref()),
            byte_size: file.byte_size,
        })
        .collect::<Vec<_>>();
    let large_untracked_dirs = existing_untracked
        .ignored_large_untracked_dirs
        .iter()
        .map(|dir| LargeUntrackedDir {
            path: to_session_relative_path(dir.path.as_path(), repo_prefix.as_deref()),
            file_count: dir.file_count,
        })
        .collect::<Vec<_>>();
    let index_tempdir = Builder::new().prefix("ctox-git-index-").tempdir()?;
    let index_path = index_tempdir.path().join("index");
    let base_env = vec![(
        OsString::from("GIT_INDEX_FILE"),
        OsString::from(index_path.as_os_str()),
    )];
    // Use a temporary index so snapshotting does not disturb the user's index state.
    // Example plumbing sequence:
    //   GIT_INDEX_FILE=/tmp/index git read-tree HEAD
    //   GIT_INDEX_FILE=/tmp/index git add --all -- <paths>
    //   GIT_INDEX_FILE=/tmp/index git write-tree
    //   GIT_INDEX_FILE=/tmp/index git commit-tree <tree> -p <parent> -m "codex snapshot"

    // Pre-populate the temporary index with HEAD so unchanged tracked files
    // are included in the snapshot tree.
    if let Some(parent_sha) = parent.as_deref() {
        run_git_for_status(
            repo_root.as_path(),
            vec![OsString::from("read-tree"), OsString::from(parent_sha)],
            Some(base_env.as_slice()),
        )?;
    }

    let mut index_paths = status_snapshot.tracked_paths;
    index_paths.extend(existing_untracked.untracked_files_for_index.iter().cloned());
    let index_paths = dedupe_paths(index_paths);
    // Stage tracked + new files into the temp index so write-tree reflects the working tree.
    // We use `git add --all` to make deletions show up in the snapshot tree too.
    add_paths_to_index(repo_root.as_path(), base_env.as_slice(), &index_paths)?;
    if !force_include.is_empty() {
        let mut args = Vec::with_capacity(force_include.len() + 2);
        args.push(OsString::from("add"));
        args.push(OsString::from("--force"));
        args.extend(
            force_include
                .iter()
                .map(|path| OsString::from(path.as_os_str())),
        );
        run_git_for_status(repo_root.as_path(), args, Some(base_env.as_slice()))?;
    }

    let tree_id = run_git_for_stdout(
        repo_root.as_path(),
        vec![OsString::from("write-tree")],
        Some(base_env.as_slice()),
    )?;

    let mut commit_env = base_env;
    commit_env.extend(default_commit_identity());
    let message = options.message.unwrap_or(DEFAULT_COMMIT_MESSAGE);
    let commit_args = {
        let mut result = vec![OsString::from("commit-tree"), OsString::from(&tree_id)];
        if let Some(parent) = parent.as_deref() {
            result.extend([OsString::from("-p"), OsString::from(parent)]);
        }
        result.extend([OsString::from("-m"), OsString::from(message)]);
        result
    };

    // `git commit-tree` writes a detached commit object without updating refs,
    // which keeps snapshots out of the user's branch history.
    // Retrieve commit ID.
    let commit_id = run_git_for_stdout(
        repo_root.as_path(),
        commit_args,
        Some(commit_env.as_slice()),
    )?;

    let ghost_commit = GhostCommit::new(
        commit_id,
        parent,
        merge_preserved_untracked_files(
            existing_untracked.files,
            &existing_untracked.ignored_untracked_files,
        ),
        merge_preserved_untracked_dirs(
            existing_untracked.dirs,
            &existing_untracked.ignored_large_untracked_dirs,
        ),
    );

    Ok((
        ghost_commit,
        GhostSnapshotReport {
            large_untracked_dirs,
            ignored_untracked_files: warning_ignored_files,
        },
    ))
}

/// Restore the working tree to match the provided ghost commit.
pub fn restore_ghost_commit(repo_path: &Path, commit: &GhostCommit) -> Result<(), GitToolingError> {
    restore_ghost_commit_with_options(&RestoreGhostCommitOptions::new(repo_path), commit)
}

/// Restore the working tree using the provided options.
pub fn restore_ghost_commit_with_options(
    options: &RestoreGhostCommitOptions<'_>,
    commit: &GhostCommit,
) -> Result<(), GitToolingError> {
    ensure_git_repository(options.repo_path)?;

    let repo_root = resolve_repository_root(options.repo_path)?;
    let repo_prefix = repo_subdir(repo_root.as_path(), options.repo_path);
    let current_untracked = capture_existing_untracked(
        repo_root.as_path(),
        repo_prefix.as_deref(),
        options.ghost_snapshot.ignore_large_untracked_files,
        options.ghost_snapshot.ignore_large_untracked_dirs,
        &[],
    )?;
    restore_to_commit_inner(repo_root.as_path(), repo_prefix.as_deref(), commit.id())?;
    remove_new_untracked(
        repo_root.as_path(),
        commit.preexisting_untracked_files(),
        commit.preexisting_untracked_dirs(),
        current_untracked,
    )
}

/// Restore the working tree to match the given commit ID.
pub fn restore_to_commit(repo_path: &Path, commit_id: &str) -> Result<(), GitToolingError> {
    ensure_git_repository(repo_path)?;

    let repo_root = resolve_repository_root(repo_path)?;
    let repo_prefix = repo_subdir(repo_root.as_path(), repo_path);
    restore_to_commit_inner(repo_root.as_path(), repo_prefix.as_deref(), commit_id)
}

/// Restores the working tree and index to the given commit using `git restore`.
/// The repository root and optional repository-relative prefix limit the restore scope.
fn restore_to_commit_inner(
    repo_root: &Path,
    repo_prefix: Option<&Path>,
    commit_id: &str,
) -> Result<(), GitToolingError> {
    // `git restore` resets the working tree to the snapshot commit.
    // We intentionally avoid --staged to preserve user's staged changes.
    // While this might leave some Codex-staged changes in the index (if Codex ran `git add`),
    // it prevents data loss for users who use the index as a save point.
    // Data safety > cleanliness.
    // Example:
    //   git restore --source <commit> --worktree -- <prefix>
    let mut restore_args = vec![
        OsString::from("restore"),
        OsString::from("--source"),
        OsString::from(commit_id),
        OsString::from("--worktree"),
        OsString::from("--"),
    ];
    if let Some(prefix) = repo_prefix {
        restore_args.push(prefix.as_os_str().to_os_string());
    } else {
        restore_args.push(OsString::from("."));
    }

    run_git_for_status(repo_root, restore_args, /*env*/ None)?;
    Ok(())
}

#[derive(Default)]
struct UntrackedSnapshot {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
    untracked_files_for_index: Vec<PathBuf>,
    ignored_untracked_files: Vec<IgnoredUntrackedFile>,
    ignored_large_untracked_dirs: Vec<LargeUntrackedDir>,
    ignored_large_untracked_dir_files: Vec<PathBuf>,
}

#[derive(Default)]
struct StatusSnapshot {
    tracked_paths: Vec<PathBuf>,
    untracked: UntrackedSnapshot,
}

/// Captures the working tree status under `repo_root`, optionally limited by `repo_prefix`.
/// Returns the result as a `StatusSnapshot`.
fn capture_status_snapshot(
    repo_root: &Path,
    repo_prefix: Option<&Path>,
    ignore_large_untracked_files: Option<i64>,
    ignore_large_untracked_dirs: Option<i64>,
    force_include: &[PathBuf],
) -> Result<StatusSnapshot, GitToolingError> {
    // Ask git for the zero-delimited porcelain status so we can enumerate
    // tracked, untracked, and ignored entries (including ones filtered by prefix).
    // This keeps the snapshot consistent without multiple git invocations.
    let mut args = vec![
        OsString::from("status"),
        OsString::from("--porcelain=2"),
        OsString::from("-z"),
        OsString::from("--untracked-files=all"),
    ];
    if let Some(prefix) = repo_prefix {
        args.push(OsString::from("--"));
        args.push(prefix.as_os_str().to_os_string());
    }

    let output = run_git_for_stdout_all(repo_root, args, /*env*/ None)?;
    if output.is_empty() {
        return Ok(StatusSnapshot::default());
    }

    let mut snapshot = StatusSnapshot::default();
    let mut untracked_files_for_dir_scan: Vec<PathBuf> = Vec::new();
    let mut expect_rename_source = false;
    for entry in output.split('\0') {
        if entry.is_empty() {
            continue;
        }
        if expect_rename_source {
            let normalized = normalize_relative_path(Path::new(entry))?;
            snapshot.tracked_paths.push(normalized);
            expect_rename_source = false;
            continue;
        }

        let record_type = entry.as_bytes().first().copied().unwrap_or(b' ');
        match record_type {
            b'?' | b'!' => {
                let mut parts = entry.splitn(2, ' ');
                let code = parts.next();
                let path_part = parts.next();
                let (Some(code), Some(path_part)) = (code, path_part) else {
                    continue;
                };
                if path_part.is_empty() {
                    continue;
                }

                let normalized = normalize_relative_path(Path::new(path_part))?;
                if should_ignore_for_snapshot(&normalized) {
                    continue;
                }
                let absolute = repo_root.join(&normalized);
                let is_dir = absolute.is_dir();
                if is_dir {
                    snapshot.untracked.dirs.push(normalized);
                } else if code == "?" {
                    untracked_files_for_dir_scan.push(normalized.clone());
                    if let Some(threshold) = ignore_large_untracked_files
                        && threshold > 0
                        && !is_force_included(&normalized, force_include)
                        && let Ok(Some(byte_size)) = untracked_file_size(&absolute)
                        && byte_size > threshold
                    {
                        snapshot
                            .untracked
                            .ignored_untracked_files
                            .push(IgnoredUntrackedFile {
                                path: normalized,
                                byte_size,
                            });
                    } else {
                        snapshot.untracked.files.push(normalized.clone());
                        snapshot
                            .untracked
                            .untracked_files_for_index
                            .push(normalized);
                    }
                } else {
                    snapshot.untracked.files.push(normalized);
                }
            }
            b'1' => {
                if let Some(path) =
                    extract_status_path_after_fields(entry, /*fields_before_path*/ 8)
                {
                    let normalized = normalize_relative_path(Path::new(path))?;
                    snapshot.tracked_paths.push(normalized);
                }
            }
            b'2' => {
                if let Some(path) =
                    extract_status_path_after_fields(entry, /*fields_before_path*/ 9)
                {
                    let normalized = normalize_relative_path(Path::new(path))?;
                    snapshot.tracked_paths.push(normalized);
                }
                expect_rename_source = true;
            }
            b'u' => {
                if let Some(path) =
                    extract_status_path_after_fields(entry, /*fields_before_path*/ 10)
                {
                    let normalized = normalize_relative_path(Path::new(path))?;
                    snapshot.tracked_paths.push(normalized);
                }
            }
            _ => {}
        }
    }

    if let Some(threshold) = ignore_large_untracked_dirs
        && threshold > 0
    {
        let ignored_large_untracked_dirs = detect_large_untracked_dirs(
            &untracked_files_for_dir_scan,
            &snapshot.untracked.dirs,
            Some(threshold),
        )
        .into_iter()
        .filter(|entry| !entry.path.as_os_str().is_empty() && entry.path != Path::new("."))
        .collect::<Vec<_>>();

        if !ignored_large_untracked_dirs.is_empty() {
            let ignored_dir_paths = ignored_large_untracked_dirs
                .iter()
                .map(|entry| entry.path.as_path())
                .collect::<Vec<_>>();

            snapshot
                .untracked
                .files
                .retain(|path| !ignored_dir_paths.iter().any(|dir| path.starts_with(dir)));
            snapshot
                .untracked
                .dirs
                .retain(|path| !ignored_dir_paths.iter().any(|dir| path.starts_with(dir)));
            snapshot
                .untracked
                .untracked_files_for_index
                .retain(|path| !ignored_dir_paths.iter().any(|dir| path.starts_with(dir)));
            snapshot.untracked.ignored_untracked_files.retain(|file| {
                !ignored_dir_paths
                    .iter()
                    .any(|dir| file.path.starts_with(dir))
            });

            snapshot.untracked.ignored_large_untracked_dir_files = untracked_files_for_dir_scan
                .into_iter()
                .filter(|path| ignored_dir_paths.iter().any(|dir| path.starts_with(dir)))
                .collect();
            snapshot.untracked.ignored_large_untracked_dirs = ignored_large_untracked_dirs;
        }
    }

    Ok(snapshot)
}

/// Captures the untracked and ignored entries under `repo_root`, optionally limited by `repo_prefix`.
/// Returns the result as an `UntrackedSnapshot`.
fn capture_existing_untracked(
    repo_root: &Path,
    repo_prefix: Option<&Path>,
    ignore_large_untracked_files: Option<i64>,
    ignore_large_untracked_dirs: Option<i64>,
    force_include: &[PathBuf],
) -> Result<UntrackedSnapshot, GitToolingError> {
    Ok(capture_status_snapshot(
        repo_root,
        repo_prefix,
        ignore_large_untracked_files,
        ignore_large_untracked_dirs,
        force_include,
    )?
    .untracked)
}

fn extract_status_path_after_fields(record: &str, fields_before_path: i64) -> Option<&str> {
    if fields_before_path <= 0 {
        return None;
    }
    let mut spaces = 0_i64;
    for (idx, byte) in record.as_bytes().iter().enumerate() {
        if *byte == b' ' {
            spaces += 1;
            if spaces == fields_before_path {
                return record.get((idx + 1)..).filter(|path| !path.is_empty());
            }
        }
    }
    None
}

fn should_ignore_for_snapshot(path: &Path) -> bool {
    path.components().any(|component| {
        if let Component::Normal(name) = component
            && let Some(name_str) = name.to_str()
        {
            return DEFAULT_IGNORED_DIR_NAMES
                .iter()
                .any(|ignored| ignored == &name_str);
        }
        false
    })
}

fn prepare_force_include(
    repo_prefix: Option<&Path>,
    force_include: &[PathBuf],
) -> Result<Vec<PathBuf>, GitToolingError> {
    let normalized_force = force_include
        .iter()
        .map(PathBuf::as_path)
        .map(normalize_relative_path)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(apply_repo_prefix_to_force_include(
        repo_prefix,
        &normalized_force,
    ))
}

fn is_force_included(path: &Path, force_include: &[PathBuf]) -> bool {
    force_include
        .iter()
        .any(|candidate| path.starts_with(candidate.as_path()))
}

fn untracked_file_size(path: &Path) -> io::Result<Option<i64>> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(None);
    };

    let Ok(len_i64) = i64::try_from(metadata.len()) else {
        return Ok(Some(i64::MAX));
    };
    Ok(Some(len_i64))
}

fn add_paths_to_index(
    repo_root: &Path,
    env: &[(OsString, OsString)],
    paths: &[PathBuf],
) -> Result<(), GitToolingError> {
    if paths.is_empty() {
        return Ok(());
    }

    let chunk_size = usize::try_from(64_i64).unwrap_or(1);
    for chunk in paths.chunks(chunk_size) {
        let mut args = vec![
            OsString::from("add"),
            OsString::from("--all"),
            OsString::from("--"),
        ];
        args.extend(chunk.iter().map(|path| path.as_os_str().to_os_string()));
        // Chunk the argv to avoid oversized command lines on large repos.
        run_git_for_status(repo_root, args, Some(env))?;
    }

    Ok(())
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for path in paths {
        if seen.insert(path.clone()) {
            result.push(path);
        }
    }
    result
}

fn merge_preserved_untracked_files(
    mut files: Vec<PathBuf>,
    ignored: &[IgnoredUntrackedFile],
) -> Vec<PathBuf> {
    if ignored.is_empty() {
        return files;
    }

    files.extend(ignored.iter().map(|entry| entry.path.clone()));
    files
}

fn merge_preserved_untracked_dirs(
    mut dirs: Vec<PathBuf>,
    ignored_large_dirs: &[LargeUntrackedDir],
) -> Vec<PathBuf> {
    if ignored_large_dirs.is_empty() {
        return dirs;
    }

    for entry in ignored_large_dirs {
        if dirs.iter().any(|dir| dir == &entry.path) {
            continue;
        }
        dirs.push(entry.path.clone());
    }

    dirs
}

/// Removes untracked files and directories that were not present when the snapshot was captured.
fn remove_new_untracked(
    repo_root: &Path,
    preserved_files: &[PathBuf],
    preserved_dirs: &[PathBuf],
    current: UntrackedSnapshot,
) -> Result<(), GitToolingError> {
    if current.files.is_empty() && current.dirs.is_empty() {
        return Ok(());
    }

    let preserved_file_set: HashSet<PathBuf> = preserved_files.iter().cloned().collect();
    let preserved_dirs_vec: Vec<PathBuf> = preserved_dirs.to_vec();

    for path in current.files {
        if should_preserve(&path, &preserved_file_set, &preserved_dirs_vec) {
            continue;
        }
        remove_path(&repo_root.join(&path))?;
    }

    for dir in current.dirs {
        if should_preserve(&dir, &preserved_file_set, &preserved_dirs_vec) {
            continue;
        }
        remove_path(&repo_root.join(&dir))?;
    }

    Ok(())
}

/// Determines whether an untracked path should be kept because it existed in the snapshot.
fn should_preserve(
    path: &Path,
    preserved_files: &HashSet<PathBuf>,
    preserved_dirs: &[PathBuf],
) -> bool {
    if preserved_files.contains(path) {
        return true;
    }

    preserved_dirs
        .iter()
        .any(|dir| path.starts_with(dir.as_path()))
}

/// Deletes the file or directory at the provided path, ignoring if it is already absent.
fn remove_path(path: &Path) -> Result<(), GitToolingError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_file(path)?;
            }
        }
        Err(err) => {
            if err.kind() == io::ErrorKind::NotFound {
                return Ok(());
            }
            return Err(err.into());
        }
    }
    Ok(())
}

/// Returns the default author and committer identity for ghost commits.
fn default_commit_identity() -> Vec<(OsString, OsString)> {
    vec![
        (
            OsString::from("GIT_AUTHOR_NAME"),
            OsString::from("Codex Snapshot"),
        ),
        (
            OsString::from("GIT_AUTHOR_EMAIL"),
            OsString::from("snapshot@codex.local"),
        ),
        (
            OsString::from("GIT_COMMITTER_NAME"),
            OsString::from("Codex Snapshot"),
        ),
        (
            OsString::from("GIT_COMMITTER_EMAIL"),
            OsString::from("snapshot@codex.local"),
        ),
    ]
}
