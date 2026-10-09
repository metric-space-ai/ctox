// Origin: CTOX
// License: AGPL-3.0-only

//! Network-free Git working-copy transfer artifacts (RFC §13).

use anyhow::{anyhow, bail, ensure, Context};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

pub const GIT_PACK_FAILED: &str = "git_pack_failed";
pub const ARTIFACT_MISSING: &str = "artifact_missing";
pub const UNSUPPORTED_FILE_TYPE: &str = "unsupported_file_type";
pub const TARGET_WORKING_COPY_DIRTY: &str = "target_working_copy_dirty";
pub const APPLY_HASH_MISMATCH: &str = "apply_hash_mismatch";
pub const INVALID_MANIFEST_PATH: &str = "invalid_manifest_path";
pub const MANIFEST_ENTRY_LIMIT_EXCEEDED: &str = "manifest_entry_limit_exceeded";

const BUNDLE_NAME: &str = "bundle.gitbundle";
const PATCH_NAME: &str = "tracked.patch";
const INDEX_PATCH_NAME: &str = "index.patch";
const UNTRACKED_NAME: &str = "untracked.tar";
const MANIFEST_NAME: &str = "manifest.json";
const MAX_MANIFEST_ENTRIES: usize = 1_000_000;
const TAR_BLOCK: usize = 512;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitManifestFile {
    pub path: String,
    pub mode: u32,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitManifestProof {
    pub head: String,
    pub branch: Option<String>,
    pub base_commit: String,
    pub patch_sha256: String,
    pub untracked_sha256: String,
    pub dirty: bool,
    /// Absent in legacy packs, which stage the combined working-tree patch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<GitIndexProof>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitIndexProof {
    pub tree: String,
    pub patch_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitPackManifest {
    pub files: Vec<GitManifestFile>,
    pub git: GitManifestProof,
    pub manifest_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitApplyReport {
    pub observed_head: String,
    pub observed_manifest_sha256: String,
    pub applied_patch: bool,
    pub untracked_files: usize,
}

#[derive(Serialize)]
struct CanonicalManifest<'a> {
    files: &'a [GitManifestFile],
    git: &'a GitManifestProof,
}

/// Packs a usable local Git repository without consulting a remote or a store.
pub fn pack_git_working_copy(
    source_dir: &Path,
    artifacts_dir: &Path,
) -> anyhow::Result<GitPackManifest> {
    let head = git_text(source_dir, &["rev-parse", "--verify", "HEAD"])
        .map_err(|error| anyhow!("{GIT_PACK_FAILED}: {error:#}"))?;
    validate_object_id(&head, "HEAD")?;
    let branch_output = git_output(source_dir, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let branch = if branch_output.status.success() {
        let value = output_text(&branch_output, "git symbolic-ref")?;
        ensure!(
            !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control),
            "{GIT_PACK_FAILED}: invalid branch name"
        );
        Some(value)
    } else if branch_output.status.code() == Some(1) {
        None
    } else {
        return Err(command_error("git symbolic-ref", &branch_output));
    };

    let artifacts_dir = prepare_artifacts_directory(source_dir, artifacts_dir)?;

    let files = collect_manifest_files(source_dir)?;
    ensure!(
        files.len() <= MAX_MANIFEST_ENTRIES,
        "{MANIFEST_ENTRY_LIMIT_EXCEEDED}: more than {MAX_MANIFEST_ENTRIES} files"
    );
    let untracked_paths = git_paths(
        source_dir,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;

    let bundle_path = artifacts_dir.join(BUNDLE_NAME);
    let patch_path = artifacts_dir.join(PATCH_NAME);
    let index_patch_path = artifacts_dir.join(INDEX_PATCH_NAME);
    let untracked_path = artifacts_dir.join(UNTRACKED_NAME);
    let manifest_path = artifacts_dir.join(MANIFEST_NAME);
    // Unlink old artifact entries rather than truncating their targets: even an
    // external artifact directory can contain links into the source worktree.
    for artifact in [
        &bundle_path,
        &patch_path,
        &index_patch_path,
        &untracked_path,
        &manifest_path,
    ] {
        match fs::remove_file(artifact) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("{GIT_PACK_FAILED}: replace {}", artifact.display()))
            }
        }
    }

    let mut bundle_args = vec![
        "bundle".to_owned(),
        "create".to_owned(),
        path_arg(&bundle_path)?,
        "HEAD".to_owned(),
    ];
    if let Some(branch) = &branch {
        bundle_args.push(format!("refs/heads/{branch}"));
    }
    run_git_owned(source_dir, &bundle_args)
        .map_err(|error| anyhow!("{GIT_PACK_FAILED}: {error:#}"))?;

    let patch_output = git_output(
        source_dir,
        &[
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "HEAD",
            "--",
        ],
    )?;
    ensure!(
        patch_output.status.success(),
        "{GIT_PACK_FAILED}: {}",
        command_error("git diff", &patch_output)
    );
    write_pack_artifact(&patch_path, &patch_output.stdout)?;
    // A tree identity binds staging independently from the working files. An
    // unresolved index cannot produce a tree and is rejected before publication.
    let index_tree = git_text(source_dir, &["write-tree"])?;
    validate_object_id(&index_tree, "index.tree")?;
    // Intent-to-add has no tree representation. Refuse it instead of silently
    // turning it into an ordinary untracked file at the destination.
    let visible = git_paths(
        source_dir,
        &[
            "diff",
            "--cached",
            "--ita-visible-in-index",
            "--name-only",
            "-z",
            "HEAD",
            "--",
        ],
    )?;
    let invisible = git_paths(
        source_dir,
        &[
            "diff",
            "--cached",
            "--ita-invisible-in-index",
            "--name-only",
            "-z",
            "HEAD",
            "--",
        ],
    )?;
    ensure!(
        visible == invisible,
        "{GIT_PACK_FAILED}: intent-to-add index entries are unsupported"
    );
    let index_patch = git_output(
        source_dir,
        &[
            "diff",
            "--cached",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "HEAD",
            "--",
        ],
    )?;
    ensure!(
        index_patch.status.success(),
        "{GIT_PACK_FAILED}: index diff failed"
    );
    write_pack_artifact(&index_patch_path, &index_patch.stdout)?;
    write_untracked_tar(source_dir, &untracked_paths, &untracked_path)?;

    let patch_sha256 = sha256_file(&patch_path)?;
    let untracked_sha256 = sha256_file(&untracked_path)?;
    let dirty = !files.is_empty();
    let mut manifest = GitPackManifest {
        files,
        git: GitManifestProof {
            head: head.clone(),
            branch,
            base_commit: head,
            patch_sha256,
            untracked_sha256,
            dirty,
            index: Some(GitIndexProof {
                tree: index_tree,
                patch_sha256: sha256_file(&index_patch_path)?,
            }),
        },
        manifest_sha256: String::new(),
    };
    manifest.manifest_sha256 = sha256_bytes(&canonical_manifest_bytes(&manifest)?);
    write_pack_artifact(&manifest_path, &canonical_manifest_bytes(&manifest)?)?;
    // A successful pack is an artifact-publication boundary: callers may now
    // record it durably or hand it to another peer. Flush every artifact before
    // confirming the pack, then persist the directory entries and parent link.
    for artifact in [
        &bundle_path,
        &patch_path,
        &index_patch_path,
        &untracked_path,
        &manifest_path,
    ] {
        File::open(artifact)?
            .sync_all()
            .with_context(|| format!("{GIT_PACK_FAILED}: flush {}", artifact.display()))?;
    }
    sync_materialized_directory(&artifacts_dir)?;
    if let Some(parent) = artifacts_dir.parent() {
        sync_materialized_directory(parent)?;
    }
    Ok(manifest)
}

fn write_pack_artifact(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    File::options()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .with_context(|| format!("{GIT_PACK_FAILED}: create artifact {}", path.display()))
}

/// Resolve the existing ancestor before appending missing components, so a
/// symlink alias cannot hide output beneath the source or shared Git storage.
fn resolve_future_directory(path: &Path) -> anyhow::Result<PathBuf> {
    ensure!(
        !path.as_os_str().is_empty(),
        "{GIT_PACK_FAILED}: empty artifact path"
    );
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut ancestor = absolute.as_path();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or_else(|| {
                            anyhow!("{GIT_PACK_FAILED}: invalid unresolved artifact path")
                        })?
                        .to_owned(),
                );
                ancestor = ancestor.parent().ok_or_else(|| {
                    anyhow!("{GIT_PACK_FAILED}: artifact path has no existing ancestor")
                })?;
            }
            Err(error) => return Err(error).context("resolve artifact ancestor"),
        }
    }
    let mut resolved = fs::canonicalize(ancestor)?;
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn prepare_artifacts_directory(source: &Path, requested: &Path) -> anyhow::Result<PathBuf> {
    let source = fs::canonicalize(source)?;
    let top = fs::canonicalize(git_text(&source, &["rev-parse", "--show-toplevel"])?)?;
    ensure!(
        source == top,
        "{GIT_PACK_FAILED}: source must be the Git worktree root"
    );
    let git_dir = fs::canonicalize(git_text(&source, &["rev-parse", "--absolute-git-dir"])?)?;
    let common = PathBuf::from(git_text(&source, &["rev-parse", "--git-common-dir"])?);
    let common = fs::canonicalize(if common.is_absolute() {
        common
    } else {
        source.join(common)
    })?;
    let protected = [source, git_dir, common];
    let verify = |path: &Path| -> anyhow::Result<()> {
        ensure!(
            protected.iter().all(|root| !path.starts_with(root)),
            "{GIT_PACK_FAILED}: artifacts must be outside the source worktree and Git metadata"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            // A case-insensitive filesystem can retain different path spelling
            // for the same directory. Compare ancestor identities as well.
            for root in &protected {
                let expected = fs::metadata(root)?;
                for ancestor in path.ancestors() {
                    match fs::metadata(ancestor) {
                        Ok(actual) => ensure!(actual.dev() != expected.dev() || actual.ino() != expected.ino(),
                            "{GIT_PACK_FAILED}: artifacts must be outside the source worktree and Git metadata"),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                        Err(error) => return Err(error).context("verify artifact ancestor identity"),
                    }
                }
            }
        }
        Ok(())
    };
    let output = resolve_future_directory(requested)?;
    verify(&output)?;
    fs::create_dir_all(&output).context("create transfer artifact directory")?;
    let output = fs::canonicalize(output)?;
    // Recheck after creation before replacing any artifact if a parent changed.
    verify(&output)?;
    Ok(output)
}

/// Applies artifacts into a sibling partial directory, verifies them, then renames it.
pub fn apply_git_working_copy(
    artifacts_dir: &Path,
    manifest: &GitPackManifest,
    target_dir: &Path,
) -> anyhow::Result<GitApplyReport> {
    validate_target_empty(target_dir)?;
    let temp_dir = transfer_temp_path(target_dir)?;
    fs::create_dir(&temp_dir).with_context(|| {
        format!(
            "failed to create transfer temporary directory {}",
            temp_dir.display()
        )
    })?;

    match apply_in_temp(artifacts_dir, manifest, &temp_dir) {
        Ok(report) => {
            sync_materialized_tree(&temp_dir)?;
            if target_dir.exists() {
                fs::remove_dir(target_dir).with_context(|| {
                    format!("failed to remove empty target {}", target_dir.display())
                })?;
            }
            fs::rename(&temp_dir, target_dir).with_context(|| {
                format!(
                    "failed to atomically rename {} to {}",
                    temp_dir.display(),
                    target_dir.display()
                )
            })?;
            let parent = target_dir
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            sync_materialized_directory(parent)?;
            Ok(report)
        }
        Err(error) => Err(anyhow!("{error:#}; temporary_dir={}", temp_dir.display())),
    }
}

/// Flush every materialized file and then directories in child-before-parent order.
fn sync_materialized_tree(root: &Path) -> anyhow::Result<()> {
    // Include Git objects, refs and index, not just the dirty-file manifest.
    // Never follow symlinks: their entries are made durable by the parent sync.
    let mut pending = vec![root.to_path_buf()];
    let mut directories = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                File::open(entry.path())?.sync_all()?;
            } else {
                ensure!(
                    kind.is_symlink(),
                    "{UNSUPPORTED_FILE_TYPE}: materialized entry"
                );
            }
        }
        directories.push(directory);
    }
    for directory in directories.into_iter().rev() {
        sync_materialized_directory(&directory)?;
    }
    Ok(())
}

fn sync_materialized_directory(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?
            .sync_all()
            .context("flush materialized directory")
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        bail!("durable Git target publication is not certified on this platform")
    }
}

/// Verify an existing workspace without rewriting its index or copying Git metadata.
pub fn verify_git_working_copy(source: &Path, manifest: &GitPackManifest) -> anyhow::Result<()> {
    validate_manifest(manifest)?;
    ensure!(
        git_text(source, &["rev-parse", "--verify", "HEAD"])? == manifest.git.head
            && current_branch(source)? == manifest.git.branch
            && collect_manifest_files(source)? == manifest.files,
        "{APPLY_HASH_MISMATCH}: workspace HEAD, branch or file contents changed"
    );
    let index = manifest
        .git
        .index
        .as_ref()
        .context("workspace requires index proof")?;
    ensure!(
        git_text(source, &["write-tree"])? == index.tree,
        "{APPLY_HASH_MISMATCH}: workspace index changed"
    );
    let patch = git_output(
        source,
        &[
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "HEAD",
            "--",
        ],
    )?;
    ensure!(
        patch.status.success() && sha256_bytes(&patch.stdout) == manifest.git.patch_sha256,
        "{APPLY_HASH_MISMATCH}: workspace patch changed"
    );
    Ok(())
}

/// Returns a lowercase SHA-256 digest for a regular file's bytes.
pub fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("{ARTIFACT_MISSING}: failed to open {}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex_digest(digest.finalize().as_slice()))
}

/// Serializes the manifest payload with recursively sorted object keys and no whitespace.
pub fn canonical_manifest_bytes(manifest: &GitPackManifest) -> anyhow::Result<Vec<u8>> {
    let value = serde_json::to_value(CanonicalManifest {
        files: &manifest.files,
        git: &manifest.git,
    })?;
    let canonical = canonicalize_value(value);
    Ok(serde_json::to_vec(&canonical)?)
}

pub(crate) fn execute_cli(args: &[String]) -> anyhow::Result<Value> {
    match args.first().map(String::as_str) {
        Some("pack") => {
            let source = required_flag(args, "--source")?;
            let artifacts = required_flag(args, "--artifacts")?;
            let manifest = pack_git_working_copy(Path::new(source), Path::new(artifacts))?;
            Ok(serde_json::json!({
                "manifest": manifest,
                "hashes": {
                    "manifest_sha256": manifest.manifest_sha256,
                    "patch_sha256": manifest.git.patch_sha256,
                    "index_patch_sha256": manifest.git.index.as_ref().map(|index| &index.patch_sha256),
                    "untracked_sha256": manifest.git.untracked_sha256,
                },
                "artifacts": {
                    "bundle": Path::new(artifacts).join(BUNDLE_NAME),
                    "patch": Path::new(artifacts).join(PATCH_NAME),
                    "index_patch": Path::new(artifacts).join(INDEX_PATCH_NAME),
                    "untracked": Path::new(artifacts).join(UNTRACKED_NAME),
                    "manifest": Path::new(artifacts).join(MANIFEST_NAME),
                }
            }))
        }
        Some("apply") => {
            let artifacts = required_flag(args, "--artifacts")?;
            let target = required_flag(args, "--target")?;
            let manifest = read_manifest(Path::new(artifacts))?;
            Ok(serde_json::to_value(apply_git_working_copy(
                Path::new(artifacts),
                &manifest,
                Path::new(target),
            )?)?)
        }
        _ => bail!(
            "usage: ctox workjet-transfer pack --source <dir> --artifacts <dir> | ctox workjet-transfer apply --artifacts <dir> --target <dir>"
        ),
    }
}

fn apply_in_temp(
    artifacts_dir: &Path,
    manifest: &GitPackManifest,
    temp_dir: &Path,
) -> anyhow::Result<GitApplyReport> {
    validate_manifest(manifest)?;
    let artifacts_dir = fs::canonicalize(artifacts_dir).with_context(|| {
        format!(
            "{ARTIFACT_MISSING}: resolve artifacts directory {}",
            artifacts_dir.display()
        )
    })?;
    let bundle = artifacts_dir.join(BUNDLE_NAME);
    let patch = artifacts_dir.join(PATCH_NAME);
    let untracked = artifacts_dir.join(UNTRACKED_NAME);
    let index_patch = artifacts_dir.join(INDEX_PATCH_NAME);
    if let Some(index) = &manifest.git.index {
        ensure!(
            sha256_file(&index_patch)? == index.patch_sha256,
            "{APPLY_HASH_MISMATCH}: index patch hash differs from manifest"
        );
    }
    let actual_patch_sha256 = sha256_file(&patch)?;
    let actual_untracked_sha256 = sha256_file(&untracked)?;
    ensure!(
        actual_patch_sha256 == manifest.git.patch_sha256
            && actual_untracked_sha256 == manifest.git.untracked_sha256,
        "{APPLY_HASH_MISMATCH}: artifact hash differs from manifest"
    );

    let clone_args = vec![
        "clone".to_owned(),
        "--no-checkout".to_owned(),
        path_arg(&bundle)?,
        path_arg(temp_dir)?,
    ];
    run_git_owned(Path::new("."), &clone_args)
        .map_err(|error| anyhow!("{GIT_PACK_FAILED}: bundle clone failed: {error:#}"))?;
    if let Some(branch) = &manifest.git.branch {
        run_git_owned(
            temp_dir,
            &[
                "checkout".to_owned(),
                "-B".to_owned(),
                branch.clone(),
                manifest.git.head.clone(),
            ],
        )?;
    } else {
        run_git_owned(
            temp_dir,
            &[
                "checkout".to_owned(),
                "--detach".to_owned(),
                manifest.git.head.clone(),
            ],
        )?;
    }

    let applied_patch = fs::metadata(&patch)?.len() > 0;
    if applied_patch {
        let mut args = vec!["apply".to_owned(), "--binary".to_owned()];
        if manifest.git.index.is_none() {
            args.push("--index".to_owned());
        }
        args.push(path_arg(&patch)?);
        run_git_owned(temp_dir, &args)?;
    }
    if let Some(index) = &manifest.git.index {
        if fs::metadata(&index_patch)?.len() > 0 {
            run_git_owned(
                temp_dir,
                &[
                    "apply".to_owned(),
                    "--binary".to_owned(),
                    "--cached".to_owned(),
                    path_arg(&index_patch)?,
                ],
            )?;
        }
        ensure!(
            git_text(temp_dir, &["write-tree"])? == index.tree,
            "{APPLY_HASH_MISMATCH}: restored index tree differs"
        );
    }
    let untracked_files = extract_untracked_tar(&untracked, temp_dir)?;

    let observed_head = git_text(temp_dir, &["rev-parse", "--verify", "HEAD"])?;
    ensure!(
        observed_head == manifest.git.head,
        "{APPLY_HASH_MISMATCH}: observed HEAD differs from manifest"
    );
    // Git records only the executable bit, and clone/apply also use the target
    // process's umask. Restore the source's recorded regular-file permissions
    // before comparing the exact file manifest. Never chmod a symlink target.
    for entry in &manifest.files {
        if entry.mode & 0o170000 == 0o100000 {
            ensure_no_symlink_ancestor(temp_dir, Path::new(&entry.path))?;
            let path = temp_dir.join(&entry.path);
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                metadata.is_file(),
                "{APPLY_HASH_MISMATCH}: expected regular file {}",
                entry.path
            );
            if regular_mode(&metadata) != entry.mode {
                set_mode(&path, entry.mode)?;
            }
        }
    }
    let observed_files = collect_manifest_files(temp_dir)?;
    ensure!(
        observed_files == manifest.files,
        "{APPLY_HASH_MISMATCH}: materialized file manifest differs"
    );
    let observed_manifest = GitPackManifest {
        files: observed_files,
        git: GitManifestProof {
            head: observed_head.clone(),
            branch: current_branch(temp_dir)?,
            base_commit: manifest.git.base_commit.clone(),
            patch_sha256: actual_patch_sha256,
            untracked_sha256: actual_untracked_sha256,
            dirty: !manifest.files.is_empty(),
            index: manifest.git.index.clone(),
        },
        manifest_sha256: String::new(),
    };
    let observed_manifest_sha256 = sha256_bytes(&canonical_manifest_bytes(&observed_manifest)?);
    ensure!(
        observed_manifest.git.branch == manifest.git.branch
            && observed_manifest.git.dirty == manifest.git.dirty
            && observed_manifest_sha256 == manifest.manifest_sha256,
        "{APPLY_HASH_MISMATCH}: observed manifest hash differs"
    );
    Ok(GitApplyReport {
        observed_head,
        observed_manifest_sha256,
        applied_patch,
        untracked_files,
    })
}

fn read_manifest(artifacts_dir: &Path) -> anyhow::Result<GitPackManifest> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StoredManifest {
        files: Vec<GitManifestFile>,
        git: GitManifestProof,
    }
    let path = artifacts_dir.join(MANIFEST_NAME);
    let bytes = fs::read(&path)
        .with_context(|| format!("{ARTIFACT_MISSING}: failed to read {}", path.display()))?;
    let stored: StoredManifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid manifest {}", path.display()))?;
    let mut manifest = GitPackManifest {
        files: stored.files,
        git: stored.git,
        manifest_sha256: String::new(),
    };
    manifest.manifest_sha256 = sha256_bytes(&canonical_manifest_bytes(&manifest)?);
    validate_manifest(&manifest)?;
    ensure!(
        bytes == canonical_manifest_bytes(&manifest)?,
        "manifest.json is not canonical"
    );
    Ok(manifest)
}

fn validate_manifest(manifest: &GitPackManifest) -> anyhow::Result<()> {
    if let Some(index) = &manifest.git.index {
        validate_object_id(&index.tree, "index.tree")?;
        ensure!(
            is_lower_hex_64(&index.patch_sha256),
            "{APPLY_HASH_MISMATCH}: invalid index patch SHA-256"
        );
    }
    validate_object_id(&manifest.git.head, "git.head")?;
    validate_object_id(&manifest.git.base_commit, "git.base_commit")?;
    ensure!(
        is_lower_hex_64(&manifest.git.patch_sha256)
            && is_lower_hex_64(&manifest.git.untracked_sha256)
            && is_lower_hex_64(&manifest.manifest_sha256),
        "{APPLY_HASH_MISMATCH}: invalid SHA-256 field"
    );
    ensure!(
        manifest.files.len() <= MAX_MANIFEST_ENTRIES,
        "{MANIFEST_ENTRY_LIMIT_EXCEEDED}: too many files"
    );
    let mut previous: Option<&str> = None;
    for entry in &manifest.files {
        validate_relative_path(&entry.path)?;
        ensure!(
            is_lower_hex_64(&entry.sha256),
            "{APPLY_HASH_MISMATCH}: invalid file SHA-256"
        );
        if let Some(previous) = previous {
            ensure!(
                previous < entry.path.as_str(),
                "{APPLY_HASH_MISMATCH}: manifest files are not uniquely sorted"
            );
        }
        previous = Some(&entry.path);
    }
    let calculated = sha256_bytes(&canonical_manifest_bytes(manifest)?);
    ensure!(
        calculated == manifest.manifest_sha256,
        "{APPLY_HASH_MISMATCH}: manifest SHA-256 differs"
    );
    Ok(())
}

fn collect_manifest_files(repo: &Path) -> anyhow::Result<Vec<GitManifestFile>> {
    let tracked = git_paths(repo, &["diff", "--name-only", "-z", "HEAD", "--"])?;
    let staged = git_paths(
        repo,
        &["diff", "--cached", "--name-only", "-z", "HEAD", "--"],
    )?;
    let untracked = git_paths(repo, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut paths = BTreeSet::new();
    paths.extend(tracked);
    paths.extend(staged);
    paths.extend(untracked);
    ensure!(
        paths.len() <= MAX_MANIFEST_ENTRIES,
        "{MANIFEST_ENTRY_LIMIT_EXCEEDED}: too many files"
    );
    paths
        .into_iter()
        .map(|relative| manifest_file(repo, &relative))
        .collect()
}

fn manifest_file(root: &Path, relative: &str) -> anyhow::Result<GitManifestFile> {
    validate_relative_path(relative)?;
    let path = root.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(GitManifestFile {
                path: relative.to_owned(),
                mode: 0,
                size: 0,
                sha256: sha256_bytes(&[]),
            });
        }
        Err(error) => return Err(error).with_context(|| format!("stat {}", path.display())),
    };
    let file_type = metadata.file_type();
    if file_type.is_file() {
        Ok(GitManifestFile {
            path: relative.to_owned(),
            mode: regular_mode(&metadata),
            size: metadata.len(),
            sha256: sha256_file(&path)?,
        })
    } else if file_type.is_symlink() {
        let target = fs::read_link(&path)
            .with_context(|| format!("failed to read symlink {}", path.display()))?;
        let target = os_str_bytes(target.as_os_str())?;
        Ok(GitManifestFile {
            path: relative.to_owned(),
            mode: 0o120777,
            size: target.len() as u64,
            sha256: sha256_bytes(&target),
        })
    } else {
        bail!(
            "{UNSUPPORTED_FILE_TYPE}: {} is neither a regular file nor a symlink",
            path.display()
        )
    }
}

fn write_untracked_tar(root: &Path, paths: &[String], output: &Path) -> anyhow::Result<()> {
    let mut sorted = paths.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut out = File::options()
        .write(true)
        .create_new(true)
        .open(output)
        .with_context(|| format!("failed to create untracked archive {}", output.display()))?;
    for relative in sorted {
        validate_relative_path(&relative)?;
        let path = root.join(&relative);
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("stat untracked path {}", path.display()))?;
        if metadata.file_type().is_file() {
            write_tar_entry(
                &mut out,
                &relative,
                regular_mode(&metadata) & 0o7777,
                b'0',
                None,
                Some(&path),
                metadata.len(),
            )?;
        } else if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path)?;
            let target = os_str_bytes(target.as_os_str())?;
            write_tar_entry(&mut out, &relative, 0o777, b'2', Some(&target), None, 0)?;
        } else {
            bail!(
                "{UNSUPPORTED_FILE_TYPE}: unsupported untracked path {}",
                path.display()
            );
        }
    }
    out.write_all(&[0_u8; TAR_BLOCK * 2])?;
    out.flush()?;
    Ok(())
}

fn write_tar_entry(
    out: &mut File,
    path: &str,
    mode: u32,
    kind: u8,
    link: Option<&[u8]>,
    source: Option<&Path>,
    size: u64,
) -> anyhow::Result<()> {
    let path_bytes = path.as_bytes();
    let needs_pax_path = split_ustar_path(path_bytes).is_none();
    let needs_pax_link = link.is_some_and(|value| value.len() > 100);
    if needs_pax_path || needs_pax_link {
        let mut payload = Vec::new();
        if needs_pax_path {
            payload.extend_from_slice(&pax_record("path", path_bytes));
        }
        if let Some(link) = link.filter(|_| needs_pax_link) {
            payload.extend_from_slice(&pax_record("linkpath", link));
        }
        let pax_name = format!("PaxHeaders/{}", sha256_bytes(path_bytes));
        write_raw_tar_header(
            out,
            pax_name.as_bytes(),
            0o644,
            b'x',
            None,
            payload.len() as u64,
        )?;
        out.write_all(&payload)?;
        write_tar_padding(out, payload.len() as u64)?;
    }
    let header_path = if needs_pax_path {
        b"pax-entry"
    } else {
        path_bytes
    };
    let header_link = link.filter(|_| !needs_pax_link);
    write_raw_tar_header(out, header_path, mode, kind, header_link, size)?;
    if let Some(source) = source {
        let mut file = File::open(source)?;
        std::io::copy(&mut file, out)?;
        write_tar_padding(out, size)?;
    }
    Ok(())
}

fn write_raw_tar_header(
    out: &mut File,
    path: &[u8],
    mode: u32,
    kind: u8,
    link: Option<&[u8]>,
    size: u64,
) -> anyhow::Result<()> {
    let mut header = [0_u8; TAR_BLOCK];
    let (name, prefix) = split_ustar_path(path)
        .ok_or_else(|| anyhow!("{INVALID_MANIFEST_PATH}: tar path cannot be represented"))?;
    copy_field(&mut header[0..100], name)?;
    write_octal(&mut header[100..108], mode as u64)?;
    write_octal(&mut header[108..116], 0)?;
    write_octal(&mut header[116..124], 0)?;
    write_octal(&mut header[124..136], size)?;
    write_octal(&mut header[136..148], 0)?;
    header[148..156].fill(b' ');
    header[156] = kind;
    if let Some(link) = link {
        copy_field(&mut header[157..257], link)?;
    }
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    copy_field(&mut header[345..500], prefix)?;
    let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    let encoded = format!("{checksum:06o}\0 ");
    header[148..156].copy_from_slice(encoded.as_bytes());
    out.write_all(&header)?;
    Ok(())
}

fn extract_untracked_tar(archive_path: &Path, target: &Path) -> anyhow::Result<usize> {
    let mut bytes = fs::read(archive_path)?;
    ensure!(
        bytes.len() >= TAR_BLOCK * 2 && bytes.len() % TAR_BLOCK == 0,
        "{APPLY_HASH_MISMATCH}: invalid tar length"
    );
    let mut offset = 0_usize;
    let mut pax_path: Option<Vec<u8>> = None;
    let mut pax_link: Option<Vec<u8>> = None;
    let mut extracted = 0_usize;
    while offset + TAR_BLOCK <= bytes.len() {
        let header = &bytes[offset..offset + TAR_BLOCK];
        offset += TAR_BLOCK;
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        verify_tar_checksum(header)?;
        let size = parse_octal(&header[124..136])?;
        let padded = size
            .checked_add((TAR_BLOCK as u64 - size % TAR_BLOCK as u64) % TAR_BLOCK as u64)
            .context("tar size overflow")? as usize;
        ensure!(
            offset
                .checked_add(padded)
                .is_some_and(|end| end <= bytes.len()),
            "{APPLY_HASH_MISMATCH}: truncated tar entry"
        );
        let data = &bytes[offset..offset + size as usize];
        offset += padded;
        let kind = header[156];
        if kind == b'x' {
            let attributes = parse_pax(data)?;
            pax_path = attributes.get("path").cloned();
            pax_link = attributes.get("linkpath").cloned();
            continue;
        }
        let path_bytes = pax_path.take().unwrap_or_else(|| tar_header_path(header));
        let path = std::str::from_utf8(&path_bytes)
            .map_err(|_| anyhow!("{INVALID_MANIFEST_PATH}: tar path is not UTF-8"))?;
        validate_relative_path(path)?;
        ensure_no_symlink_ancestor(target, Path::new(path))?;
        let output = target.join(path);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        match kind {
            0 | b'0' => {
                let mut file = File::create(&output)?;
                file.write_all(data)?;
                set_mode(&output, parse_octal(&header[100..108])? as u32)?;
            }
            b'2' => {
                let link_bytes = pax_link
                    .take()
                    .unwrap_or_else(|| field_bytes(&header[157..257]));
                create_symlink(&link_bytes, &output)?;
            }
            _ => bail!("{UNSUPPORTED_FILE_TYPE}: tar entry type {kind}"),
        }
        extracted += 1;
    }
    bytes.fill(0);
    Ok(extracted)
}

fn validate_target_empty(target: &Path) -> anyhow::Result<()> {
    if !target.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(target)?;
    ensure!(
        metadata.is_dir() && fs::read_dir(target)?.next().is_none(),
        "{TARGET_WORKING_COPY_DIRTY}: target {} is not an empty directory",
        target.display()
    );
    Ok(())
}

fn transfer_temp_path(target: &Path) -> anyhow::Result<PathBuf> {
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let name = target
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .unwrap_or("target");
    for attempt in 0..100_u32 {
        let generation = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let candidate = parent.join(format!(
            "{name}.workjet-transfer-{generation}-{}-{attempt}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!("failed to allocate transfer temporary directory")
}

fn current_branch(repo: &Path) -> anyhow::Result<Option<String>> {
    let output = git_output(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    if output.status.success() {
        Ok(Some(output_text(&output, "git symbolic-ref")?))
    } else if output.status.code() == Some(1) {
        Ok(None)
    } else {
        Err(command_error("git symbolic-ref", &output))
    }
}

fn git_paths(repo: &Path, args: &[&str]) -> anyhow::Result<Vec<String>> {
    let output = git_output(repo, args)?;
    ensure!(
        output.status.success(),
        "{GIT_PACK_FAILED}: {}",
        command_error("git", &output)
    );
    let mut paths = Vec::new();
    for raw in output.stdout.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let path = std::str::from_utf8(raw)
            .map_err(|_| anyhow!("{INVALID_MANIFEST_PATH}: Git path is not UTF-8"))?
            .to_owned();
        validate_relative_path(&path)?;
        paths.push(path);
    }
    Ok(paths)
}

fn git_text(repo: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = git_output(repo, args)?;
    ensure!(output.status.success(), "{}", command_error("git", &output));
    output_text(&output, "git")
}

fn git_output(repo: &Path, args: &[&str]) -> anyhow::Result<Output> {
    Command::new("git")
        .current_dir(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .with_context(|| format!("failed to execute git {}", args.join(" ")))
}

fn run_git_owned(repo: &Path, args: &[String]) -> anyhow::Result<()> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .with_context(|| format!("failed to execute git {}", args.join(" ")))?;
    ensure!(output.status.success(), "{}", command_error("git", &output));
    Ok(())
}

fn command_error(command: &str, output: &Output) -> anyhow::Error {
    anyhow!(
        "{command} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

fn output_text(output: &Output, command: &str) -> anyhow::Result<String> {
    Ok(std::str::from_utf8(&output.stdout)
        .with_context(|| format!("{command} returned non-UTF-8 output"))?
        .trim()
        .to_owned())
}

fn validate_object_id(value: &str, field: &str) -> anyhow::Result<()> {
    ensure!(
        matches!(value.len(), 40 | 64)
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{GIT_PACK_FAILED}: {field} is not a lowercase Git object ID"
    );
    Ok(())
}

fn validate_relative_path(path: &str) -> anyhow::Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() <= 4096
            && !path.chars().any(char::is_control)
            && !Path::new(path).is_absolute()
            && Path::new(path)
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "{INVALID_MANIFEST_PATH}: path must be relative UTF-8 without traversal: {path:?}"
    );
    Ok(())
}

fn required_flag<'a>(args: &'a [String], flag: &str) -> anyhow::Result<&'a str> {
    let mut found = None;
    let mut index = 1;
    while index < args.len() {
        let current = args[index].as_str();
        ensure!(
            matches!(current, "--source" | "--artifacts" | "--target"),
            "unexpected argument '{current}'"
        );
        let value = args
            .get(index + 1)
            .with_context(|| format!("{current} value is required"))?;
        if current == flag {
            found = Some(value.as_str());
        }
        index += 2;
    }
    found.with_context(|| format!("{flag} is required"))
}

fn canonicalize_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_value).collect()),
        Value::Object(values) => {
            let sorted = values
                .into_iter()
                .map(|(key, value)| (key, canonicalize_value(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect::<Map<_, _>>())
        }
        scalar => scalar,
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes).as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(unix)]
fn regular_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    0o100000 | (metadata.mode() & 0o7777)
}

#[cfg(not(unix))]
fn regular_mode(_metadata: &fs::Metadata) -> u32 {
    0o100644
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o7777))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn os_str_bytes(value: &OsStr) -> anyhow::Result<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    Ok(value.as_bytes().to_vec())
}

#[cfg(not(unix))]
fn os_str_bytes(value: &OsStr) -> anyhow::Result<Vec<u8>> {
    Ok(value
        .to_str()
        .context("symlink target is not UTF-8")?
        .as_bytes()
        .to_vec())
}

#[cfg(unix)]
fn create_symlink(target: &[u8], output: &Path) -> anyhow::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    symlink(OsStr::from_bytes(target), output)?;
    Ok(())
}

#[cfg(windows)]
fn create_symlink(target: &[u8], output: &Path) -> anyhow::Result<()> {
    use std::os::windows::fs::symlink_file;
    let target = std::str::from_utf8(target).context("symlink target is not UTF-8")?;
    symlink_file(target, output)?;
    Ok(())
}

fn ensure_no_symlink_ancestor(root: &Path, relative: &Path) -> anyhow::Result<()> {
    let mut current = root.to_path_buf();
    let components = relative.components().collect::<Vec<_>>();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        if let Component::Normal(part) = component {
            current.push(part);
            if let Ok(metadata) = fs::symlink_metadata(&current) {
                ensure!(
                    !metadata.file_type().is_symlink(),
                    "{APPLY_HASH_MISMATCH}: tar path traverses a symlink"
                );
            }
        }
    }
    Ok(())
}

fn path_arg(path: &Path) -> anyhow::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .with_context(|| format!("path is not UTF-8: {}", path.display()))
}

fn split_ustar_path(path: &[u8]) -> Option<(&[u8], &[u8])> {
    if path.len() <= 100 {
        return Some((path, &[]));
    }
    for index in (0..path.len()).rev() {
        if path[index] == b'/' && index <= 155 && path.len() - index - 1 <= 100 {
            return Some((&path[index + 1..], &path[..index]));
        }
    }
    None
}

fn copy_field(field: &mut [u8], value: &[u8]) -> anyhow::Result<()> {
    ensure!(value.len() <= field.len(), "tar field is too long");
    field[..value.len()].copy_from_slice(value);
    Ok(())
}

fn write_octal(field: &mut [u8], value: u64) -> anyhow::Result<()> {
    let digits = field.len() - 1;
    let encoded = format!("{value:0digits$o}", digits = digits);
    ensure!(encoded.len() == digits, "tar numeric field overflow");
    field[..digits].copy_from_slice(encoded.as_bytes());
    field[digits] = 0;
    Ok(())
}

fn write_tar_padding(out: &mut File, size: u64) -> anyhow::Result<()> {
    let padding = (TAR_BLOCK as u64 - size % TAR_BLOCK as u64) % TAR_BLOCK as u64;
    if padding > 0 {
        out.write_all(&vec![0_u8; padding as usize])?;
    }
    Ok(())
}

fn pax_record(key: &str, value: &[u8]) -> Vec<u8> {
    let mut length = key.len() + value.len() + 3;
    loop {
        let digits = length.to_string().len();
        let calculated = digits + 1 + key.len() + 1 + value.len() + 1;
        if calculated == length {
            break;
        }
        length = calculated;
    }
    let mut record = format!("{length} {key}=").into_bytes();
    record.extend_from_slice(value);
    record.push(b'\n');
    record
}

fn verify_tar_checksum(header: &[u8]) -> anyhow::Result<()> {
    let stored = parse_octal(&header[148..156])?;
    let calculated: u64 = header
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            if (148..156).contains(&index) {
                u64::from(b' ')
            } else {
                u64::from(*byte)
            }
        })
        .sum();
    ensure!(
        stored == calculated,
        "{APPLY_HASH_MISMATCH}: tar checksum mismatch"
    );
    Ok(())
}

fn parse_octal(field: &[u8]) -> anyhow::Result<u64> {
    let text = std::str::from_utf8(field)
        .context("tar numeric field is not ASCII")?
        .trim_matches(|character| character == '\0' || character == ' ');
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).context("invalid tar octal field")
}

fn field_bytes(field: &[u8]) -> Vec<u8> {
    field
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .collect()
}

fn tar_header_path(header: &[u8]) -> Vec<u8> {
    let name = field_bytes(&header[0..100]);
    let prefix = field_bytes(&header[345..500]);
    if prefix.is_empty() {
        name
    } else {
        let mut path = prefix;
        path.push(b'/');
        path.extend_from_slice(&name);
        path
    }
}

fn parse_pax(data: &[u8]) -> anyhow::Result<BTreeMap<String, Vec<u8>>> {
    let mut result = BTreeMap::new();
    let mut offset = 0_usize;
    while offset < data.len() {
        let space = data[offset..]
            .iter()
            .position(|byte| *byte == b' ')
            .map(|position| offset + position)
            .context("invalid PAX record length")?;
        let length = std::str::from_utf8(&data[offset..space])?.parse::<usize>()?;
        ensure!(
            length > space - offset + 2 && offset + length <= data.len(),
            "invalid PAX record"
        );
        let record = &data[space + 1..offset + length - 1];
        let equals = record
            .iter()
            .position(|byte| *byte == b'=')
            .context("invalid PAX attribute")?;
        let key = std::str::from_utf8(&record[..equals])?.to_owned();
        result.insert(key, record[equals + 1..].to_vec());
        offset += length;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn repository() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "workjet/test"]);
        git(repo.path(), &["config", "user.name", "Workjet Test"]);
        git(
            repo.path(),
            &["config", "user.email", "workjet@example.test"],
        );
        fs::write(repo.path().join("tracked.txt"), b"before\n").unwrap();
        git(repo.path(), &["add", "tracked.txt"]);
        git(repo.path(), &["commit", "-m", "base"]);
        repo
    }

    fn dirty_repository() -> tempfile::TempDir {
        let repo = repository();
        fs::write(repo.path().join("tracked.txt"), b"after\n").unwrap();
        fs::create_dir(repo.path().join("nested")).unwrap();
        fs::write(repo.path().join("nested/new.txt"), b"new\n").unwrap();
        fs::write(repo.path().join("binary.bin"), [0, 1, 0, 2, 255]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("nested/new.txt", repo.path().join("link-to-new")).unwrap();
        repo
    }

    #[test]
    fn pack_rejects_source_overlap_without_replacing_worktree_files() {
        let repo = dirty_repository();
        fs::write(repo.path().join(BUNDLE_NAME), b"irreplaceable source file").unwrap();
        let before = git_output(
            repo.path(),
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )
        .unwrap()
        .stdout;
        for output in [
            repo.path().to_owned(),
            repo.path().join("new-pack/nested"),
            repo.path().join(".git/pack-output"),
        ] {
            assert!(pack_git_working_copy(repo.path(), &output)
                .unwrap_err()
                .to_string()
                .contains("artifacts must be outside"));
        }
        assert_eq!(
            fs::read(repo.path().join(BUNDLE_NAME)).unwrap(),
            b"irreplaceable source file"
        );
        assert!(!repo.path().join("new-pack").exists());
        assert!(!repo.path().join(".git/pack-output").exists());
        assert_eq!(
            git_output(
                repo.path(),
                &["status", "--porcelain=v1", "-z", "--untracked-files=all"]
            )
            .unwrap()
            .stdout,
            before
        );
    }

    #[test]
    fn pack_rejects_subdirectory_sources_and_linked_git_storage() {
        let repo = dirty_repository();
        let scratch = tempfile::tempdir().unwrap();
        let output = scratch.path().join("subdir-output");
        assert!(pack_git_working_copy(&repo.path().join("nested"), &output)
            .unwrap_err()
            .to_string()
            .contains("source must be the Git worktree root"));
        assert!(!output.exists());
        let linked = scratch.path().join("linked");
        git(
            repo.path(),
            &[
                "worktree",
                "add",
                "-b",
                "linked/test",
                linked.to_str().unwrap(),
                "HEAD",
            ],
        );
        let private = repo.path().join(".git/transfer-output");
        assert!(pack_git_working_copy(&linked, &private)
            .unwrap_err()
            .to_string()
            .contains("artifacts must be outside"));
        assert!(!private.exists());
        // A linked worktree remains supported when its output is independent.
        pack_git_working_copy(&linked, &scratch.path().join("safe/nested/artifacts")).unwrap();
        git(
            repo.path(),
            &["worktree", "remove", linked.to_str().unwrap()],
        );
    }

    #[cfg(unix)]
    #[test]
    fn pack_replaces_artifact_links_without_truncating_source() {
        use std::os::unix::fs::{symlink, MetadataExt};
        for symbolic in [false, true] {
            let repo = dirty_repository();
            let scratch = tempfile::tempdir().unwrap();
            let victim = repo.path().join("source-sentinel");
            fs::write(&victim, b"irreplaceable source bytes").unwrap();
            let before = git_output(
                repo.path(),
                &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            )
            .unwrap()
            .stdout;
            let names = [
                BUNDLE_NAME,
                PATCH_NAME,
                INDEX_PATCH_NAME,
                UNTRACKED_NAME,
                MANIFEST_NAME,
            ];
            for name in names {
                let path = scratch.path().join(name);
                if symbolic {
                    symlink(&victim, path).unwrap();
                } else {
                    fs::hard_link(&victim, path).unwrap();
                }
            }
            let packed = pack_git_working_copy(repo.path(), scratch.path()).unwrap();
            assert_eq!(fs::read(&victim).unwrap(), b"irreplaceable source bytes");
            assert_eq!(
                git_output(
                    repo.path(),
                    &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
                )
                .unwrap()
                .stdout,
                before
            );
            let source = fs::metadata(&victim).unwrap();
            for name in names {
                let artifact = fs::symlink_metadata(scratch.path().join(name)).unwrap();
                assert!(artifact.file_type().is_file());
                assert!(artifact.dev() != source.dev() || artifact.ino() != source.ino());
            }
            // Repacking a previous complete output remains supported.
            assert_eq!(
                pack_git_working_copy(repo.path(), scratch.path())
                    .unwrap()
                    .manifest_sha256,
                packed.manifest_sha256
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn pack_rejects_symlink_alias_into_source() {
        let repo = repository();
        let scratch = tempfile::tempdir().unwrap();
        let alias = scratch.path().join("alias");
        std::os::unix::fs::symlink(repo.path(), &alias).unwrap();
        assert!(pack_git_working_copy(repo.path(), &alias.join("output"))
            .unwrap_err()
            .to_string()
            .contains("artifacts must be outside"));
        assert!(!repo.path().join("output").exists());
    }

    #[test]
    fn workjet_transfer_git_preserves_index_and_worktree_independently() {
        let repo = repository();
        fs::write(repo.path().join("tracked.txt"), b"staged\n").unwrap();
        fs::write(repo.path().join("staged-new.bin"), [0, 7, 255]).unwrap();
        git(repo.path(), &["add", "."]);
        fs::write(repo.path().join("tracked.txt"), b"unstaged\n").unwrap();
        fs::remove_file(repo.path().join("staged-new.bin")).unwrap();
        fs::write(repo.path().join("untracked.txt"), b"untracked\n").unwrap();
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let target = root.path().join("target");
        let manifest = pack_git_working_copy(repo.path(), &artifacts).unwrap();
        apply_git_working_copy(&artifacts, &manifest, &target).unwrap();
        for args in [
            vec!["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            vec!["diff", "--binary", "--full-index"],
            vec!["diff", "--cached", "--binary", "--full-index"],
            vec!["log", "--format=%H", "HEAD"],
            vec!["write-tree"],
        ] {
            assert_eq!(
                git_output(repo.path(), &args).unwrap().stdout,
                git_output(&target, &args).unwrap().stdout,
                "{args:?}"
            );
        }
        assert!(!target.join("staged-new.bin").exists());
        assert_eq!(fs::read(target.join("tracked.txt")).unwrap(), b"unstaged\n");
    }

    #[test]
    fn workjet_transfer_git_index_only_change_is_dirty() {
        let repo = repository();
        fs::write(repo.path().join("tracked.txt"), b"staged\n").unwrap();
        git(repo.path(), &["add", "tracked.txt"]);
        fs::write(repo.path().join("tracked.txt"), b"before\n").unwrap();
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let target = root.path().join("target");
        let manifest = pack_git_working_copy(repo.path(), &artifacts).unwrap();
        assert!(manifest.git.dirty);
        assert_eq!(manifest.git.patch_sha256, sha256_bytes(&[]));
        apply_git_working_copy(&artifacts, &manifest, &target).unwrap();
        assert_eq!(
            git_text(repo.path(), &["write-tree"]).unwrap(),
            git_text(&target, &["write-tree"]).unwrap()
        );
        assert_eq!(fs::read(target.join("tracked.txt")).unwrap(), b"before\n");
    }

    #[test]
    fn workjet_transfer_git_rejects_index_tamper_and_tree_mismatch() {
        let repo = dirty_repository();
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let mut manifest = pack_git_working_copy(repo.path(), &artifacts).unwrap();
        fs::write(artifacts.join(INDEX_PATCH_NAME), b"tampered").unwrap();
        let target = root.path().join("target-tamper");
        assert!(apply_git_working_copy(&artifacts, &manifest, &target)
            .unwrap_err()
            .to_string()
            .contains(APPLY_HASH_MISMATCH));
        assert!(!target.exists());
        fs::write(artifacts.join(INDEX_PATCH_NAME), b"").unwrap();
        manifest.git.index.as_mut().unwrap().tree = "1".repeat(40);
        manifest.manifest_sha256 = sha256_bytes(&canonical_manifest_bytes(&manifest).unwrap());
        let target = root.path().join("target-tree");
        assert!(apply_git_working_copy(&artifacts, &manifest, &target)
            .unwrap_err()
            .to_string()
            .contains("restored index tree differs"));
        assert!(!target.exists());
    }

    #[test]
    fn workjet_transfer_git_legacy_manifest_retains_legacy_staging_semantics() {
        let repo = dirty_repository();
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let target = root.path().join("target");
        let mut manifest = pack_git_working_copy(repo.path(), &artifacts).unwrap();
        manifest.git.index = None;
        manifest.manifest_sha256 = sha256_bytes(&canonical_manifest_bytes(&manifest).unwrap());
        fs::write(
            artifacts.join(MANIFEST_NAME),
            canonical_manifest_bytes(&manifest).unwrap(),
        )
        .unwrap();
        fs::remove_file(artifacts.join(INDEX_PATCH_NAME)).unwrap();
        assert_eq!(read_manifest(&artifacts).unwrap(), manifest);
        apply_git_working_copy(&artifacts, &manifest, &target).unwrap();
        assert!(git_output(&target, &["diff", "--exit-code"])
            .unwrap()
            .status
            .success());
        assert!(!git_output(&target, &["diff", "--cached", "--exit-code"])
            .unwrap()
            .status
            .success());
    }

    #[test]
    fn workjet_transfer_git_rejects_intent_to_add() {
        let repo = repository();
        fs::write(repo.path().join("new.txt"), b"pending\n").unwrap();
        git(repo.path(), &["add", "--intent-to-add", "new.txt"]);
        let artifacts = tempfile::tempdir().unwrap();
        assert!(pack_git_working_copy(repo.path(), artifacts.path())
            .unwrap_err()
            .to_string()
            .contains("intent-to-add"));
    }

    fn is_hash(value: &str) -> bool {
        is_lower_hex_64(value)
    }

    #[test]
    fn workjet_transfer_git_pack_writes_required_deterministic_artifacts() {
        let repo = dirty_repository();
        let artifacts = tempfile::tempdir().unwrap();
        let manifest = pack_git_working_copy(repo.path(), artifacts.path()).unwrap();

        for name in [
            BUNDLE_NAME,
            PATCH_NAME,
            INDEX_PATCH_NAME,
            UNTRACKED_NAME,
            MANIFEST_NAME,
        ] {
            assert!(artifacts.path().join(name).is_file(), "missing {name}");
        }
        assert_eq!(manifest.git.branch.as_deref(), Some("workjet/test"));
        assert_eq!(manifest.git.head, manifest.git.base_commit);
        assert!(manifest.git.dirty);
        assert!(is_hash(&manifest.git.patch_sha256));
        assert!(is_hash(&manifest.git.untracked_sha256));
        assert!(is_hash(&manifest.manifest_sha256));
        assert!(manifest
            .files
            .windows(2)
            .all(|pair| pair[0].path < pair[1].path));
        assert_eq!(
            fs::read(artifacts.path().join(MANIFEST_NAME)).unwrap(),
            canonical_manifest_bytes(&manifest).unwrap()
        );
        assert!(manifest
            .files
            .iter()
            .any(|entry| entry.path == "binary.bin"));
        #[cfg(unix)]
        assert!(manifest
            .files
            .iter()
            .any(|entry| entry.path == "link-to-new" && entry.mode == 0o120777));
    }

    #[test]
    fn workjet_transfer_git_clean_tree_keeps_empty_patch_hash_and_dirty_false() {
        let repo = repository();
        let artifacts = tempfile::tempdir().unwrap();
        let manifest = pack_git_working_copy(repo.path(), artifacts.path()).unwrap();
        assert!(!manifest.git.dirty);
        assert!(manifest.files.is_empty());
        assert_eq!(
            fs::metadata(artifacts.path().join(PATCH_NAME))
                .unwrap()
                .len(),
            0
        );
        assert_eq!(manifest.git.patch_sha256, sha256_bytes(&[]));
        assert!(is_hash(&manifest.git.patch_sha256));
    }

    #[cfg(unix)]
    #[test]
    fn workjet_transfer_git_apply_preserves_tracked_permissions_and_index() {
        let repo = repository();
        git(repo.path(), &["config", "core.filemode", "true"]);
        fs::write(repo.path().join("run.sh"), b"#!/bin/sh\necho before\n").unwrap();
        set_mode(&repo.path().join("run.sh"), 0o755).unwrap();
        git(repo.path(), &["add", "run.sh"]);
        git(repo.path(), &["commit", "-m", "executable base"]);
        fs::write(repo.path().join("tracked.txt"), b"staged\n").unwrap();
        git(repo.path(), &["add", "tracked.txt"]);
        fs::write(repo.path().join("tracked.txt"), b"unstaged\n").unwrap();
        set_mode(&repo.path().join("tracked.txt"), 0o640).unwrap();
        fs::write(repo.path().join("run.sh"), b"#!/bin/sh\necho after\n").unwrap();
        set_mode(&repo.path().join("run.sh"), 0o644).unwrap();
        fs::write(repo.path().join("staged-new.txt"), b"new\n").unwrap();
        set_mode(&repo.path().join("staged-new.txt"), 0o600).unwrap();
        git(repo.path(), &["add", "staged-new.txt"]);
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let target = root.path().join("target");
        let manifest = pack_git_working_copy(repo.path(), &artifacts).unwrap();
        let report = apply_git_working_copy(&artifacts, &manifest, &target).unwrap();
        assert_eq!(report.observed_manifest_sha256, manifest.manifest_sha256);
        for entry in &manifest.files {
            assert_eq!(manifest_file(&target, &entry.path).unwrap(), *entry);
        }
        for args in [
            vec!["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            vec!["diff", "--binary", "--full-index"],
            vec!["diff", "--cached", "--binary", "--full-index"],
            vec!["write-tree"],
        ] {
            assert_eq!(
                git_output(repo.path(), &args).unwrap().stdout,
                git_output(&target, &args).unwrap().stdout,
                "{args:?}"
            );
        }
    }

    #[test]
    fn workjet_transfer_git_apply_reconstructs_branch_binary_and_symlink() {
        let repo = dirty_repository();
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let target = root.path().join("target");
        let manifest = pack_git_working_copy(repo.path(), &artifacts).unwrap();
        let report = apply_git_working_copy(&artifacts, &manifest, &target).unwrap();

        assert_eq!(report.observed_head, manifest.git.head);
        assert_eq!(report.observed_manifest_sha256, manifest.manifest_sha256);
        assert!(report.applied_patch);
        assert_eq!(
            git_text(&target, &["rev-parse", "HEAD"]).unwrap(),
            manifest.git.head
        );
        assert_eq!(current_branch(&target).unwrap(), manifest.git.branch);
        for entry in &manifest.files {
            assert_eq!(manifest_file(&target, &entry.path).unwrap(), *entry);
        }
        assert_eq!(
            fs::read(target.join("binary.bin")).unwrap(),
            [0, 1, 0, 2, 255]
        );
        #[cfg(unix)]
        assert_eq!(
            fs::read_link(target.join("link-to-new")).unwrap(),
            Path::new("nested/new.txt")
        );
    }

    #[test]
    fn workjet_transfer_git_apply_rejects_nonempty_target_without_changes() {
        let repo = dirty_repository();
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep.txt"), b"keep").unwrap();
        let manifest = pack_git_working_copy(repo.path(), &artifacts).unwrap();
        let error = apply_git_working_copy(&artifacts, &manifest, &target)
            .unwrap_err()
            .to_string();
        assert!(error.contains(TARGET_WORKING_COPY_DIRTY));
        assert_eq!(fs::read(target.join("keep.txt")).unwrap(), b"keep");
    }

    #[test]
    fn workjet_transfer_git_apply_hash_mismatch_keeps_partial_for_diagnosis() {
        let repo = dirty_repository();
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let manifest = pack_git_working_copy(repo.path(), &artifacts).unwrap();
        let archive = artifacts.join(UNTRACKED_NAME);
        let mut bytes = fs::read(&archive).unwrap();
        bytes[10] ^= 1;
        fs::write(&archive, bytes).unwrap();
        let error = apply_git_working_copy(&artifacts, &manifest, &target)
            .unwrap_err()
            .to_string();
        assert!(error.contains(APPLY_HASH_MISMATCH));
        assert!(error.contains("temporary_dir="));
        assert!(fs::read_dir(&target).unwrap().next().is_none());
        let partials = fs::read_dir(root.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains(".workjet-transfer-")
            })
            .count();
        assert_eq!(partials, 1);
    }

    #[test]
    fn workjet_transfer_git_cli_functions_return_parseable_json() {
        let repo = dirty_repository();
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let target = root.path().join("target");
        let pack_args = vec![
            "pack".to_owned(),
            "--source".to_owned(),
            repo.path().display().to_string(),
            "--artifacts".to_owned(),
            artifacts.display().to_string(),
        ];
        let packed = execute_cli(&pack_args).unwrap();
        let reparsed: Value =
            serde_json::from_str(&serde_json::to_string(&packed).unwrap()).unwrap();
        assert!(is_hash(
            reparsed["hashes"]["manifest_sha256"].as_str().unwrap()
        ));
        let apply_args = vec![
            "apply".to_owned(),
            "--artifacts".to_owned(),
            artifacts.display().to_string(),
            "--target".to_owned(),
            target.display().to_string(),
        ];
        let applied = execute_cli(&apply_args).unwrap();
        let reparsed: GitApplyReport =
            serde_json::from_str(&serde_json::to_string(&applied).unwrap()).unwrap();
        assert_eq!(reparsed.observed_head, packed["manifest"]["git"]["head"]);
    }
}
