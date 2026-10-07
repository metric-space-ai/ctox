// Origin: CTOX
// License: AGPL-3.0-only

//! Frozen, transport-neutral source intake for native registered-computer builds.
//! Caller supplies a disposable staging root outside the worktree and prevents
//! concurrent edits during capture. Transport reads the frozen tree, never the
//! live checkout or the client's Git credentials.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Output},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: String,
    pub executable: bool,
    pub symlink_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicGithubBase {
    /// Validated owner/repository, without a URL, token or user information.
    pub repository: String,
    /// A commit already fetchable anonymously by the build host.
    pub revision: String,
}

/// Owned frozen files and optional local-commit bundle. Dropping removes only
/// the unique staging directory created by capture, never the original source.
#[derive(Debug)]
pub struct CapturedBuildSource {
    staging: tempfile::TempDir,
    pub head_revision: String,
    pub source_id: String,
    pub files: Vec<SourceFile>,
    pub deleted_paths: Vec<String>,
    pub public_base: Option<PublicGithubBase>,
    pub overlay_paths: Vec<String>,
    pub bundle: Option<PathBuf>,
}

impl CapturedBuildSource {
    pub fn tree(&self) -> PathBuf {
        self.staging.path().join("tree")
    }
}

fn git_output(root: &Path, args: &[&str]) -> Result<Output> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("run local source Git command")?;
    anyhow::ensure!(
        output.status.success(),
        "source Git command failed: {}",
        args[0]
    );
    Ok(output)
}

fn paths(output: &[u8]) -> Result<BTreeSet<String>> {
    output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            let path = std::str::from_utf8(path).context("source path is not UTF-8")?;
            anyhow::ensure!(
                Path::new(path)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
                    && !path.split('/').any(|part| part == ".git"),
                "unsafe source path"
            );
            Ok(path.to_owned())
        })
        .collect()
}

fn head(root: &Path) -> Result<String> {
    let output = git_output(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let revision = std::str::from_utf8(&output.stdout)?.trim().to_owned();
    validate_revision(&revision)?;
    Ok(revision)
}

fn validate_revision(revision: &str) -> Result<()> {
    anyhow::ensure!(
        revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "source commit must be a full SHA1"
    );
    Ok(())
}

fn validate_public_base(root: &Path, base: &PublicGithubBase, revision: &str) -> Result<()> {
    let parts = base.repository.split('/').collect::<Vec<_>>();
    anyhow::ensure!(
        parts.len() == 2
            && parts.iter().all(|part| {
                !part.is_empty()
                    && *part != "."
                    && *part != ".."
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            }),
        "public source requires an owner/repository, without credentials"
    );
    validate_revision(&base.revision)?;
    git_output(
        root,
        &["merge-base", "--is-ancestor", &base.revision, revision],
    )?;
    Ok(())
}

fn frame(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}

/// Freeze Git-tracked and non-ignored untracked files, including deletions,
/// executable bits and literal symlinks. Content identity includes untracked
/// bytes (not just their names), preventing reuse of a stale build/cache.
///
/// A public base is supplied only after the adapter proves anonymous fetching.
/// Public delivery uses that base plus bundle and frozen overlay; private
/// delivery uses the frozen full tree. Neither mode exports .git/config, tokens,
/// ignored caches or client authentication state. Submodules require separate
/// explicit source intake and fail closed here.
pub fn capture(
    root: &Path,
    staging_root: &Path,
    public_base: Option<PublicGithubBase>,
) -> Result<CapturedBuildSource> {
    let root = root.canonicalize().context("source worktree")?;
    let staging_root = staging_root.canonicalize().context("source staging root")?;
    anyhow::ensure!(
        !staging_root.starts_with(&root),
        "staging must be outside source"
    );
    let revision = head(&root)?;
    if let Some(base) = &public_base {
        validate_public_base(&root, base, &revision)?;
    }
    let mut names = paths(
        &git_output(
            &root,
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
        )?
        .stdout,
    )?;
    let changed = paths(&git_output(&root, &["diff", "--name-only", "-z", "HEAD", "--"])?.stdout)?;
    let untracked =
        paths(&git_output(&root, &["ls-files", "-z", "--others", "--exclude-standard"])?.stdout)?;
    // Include staged deletions: ls-files --cached no longer lists those paths.
    names.extend(changed.iter().cloned());
    let overlay_names = changed.union(&untracked).cloned().collect::<BTreeSet<_>>();
    let staging = tempfile::Builder::new()
        .prefix("ctox-build-source-")
        .tempdir_in(&staging_root)?;
    let tree = staging.path().join("tree");
    fs::create_dir(&tree)?;
    let mut hash = Sha256::new();
    frame(&mut hash, b"ctox.build-source.v1");
    frame(&mut hash, revision.as_bytes());
    let mut files = Vec::new();
    let mut deleted_paths = Vec::new();
    for name in &names {
        let relative = Path::new(name);
        // A replaced parent symlink must not make intake read outside source.
        let mut parent = root.clone();
        for component in relative.parent().into_iter().flat_map(Path::components) {
            parent.push(component);
            if let Ok(metadata) = fs::symlink_metadata(&parent) {
                anyhow::ensure!(
                    !metadata.file_type().is_symlink(),
                    "source parent is a symlink"
                );
            }
        }
        let source = root.join(relative);
        let metadata = match fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                deleted_paths.push(name.clone());
                frame(&mut hash, name.as_bytes());
                frame(&mut hash, b"deleted");
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let destination = tree.join(relative);
        fs::create_dir_all(destination.parent().context("source parent")?)?;
        frame(&mut hash, name.as_bytes());
        let (executable, symlink_target) = if metadata.file_type().is_symlink() {
            let target = fs::read_link(&source)?;
            let target = target
                .to_str()
                .context("symlink target is not UTF-8")?
                .to_owned();
            frame(&mut hash, b"symlink");
            frame(&mut hash, target.as_bytes());
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &destination)?;
            #[cfg(not(unix))]
            anyhow::bail!("literal source symlinks require Unix source intake");
            (false, Some(target))
        } else {
            anyhow::ensure!(
                metadata.is_file(),
                "source contains a submodule or unsupported file"
            );
            #[cfg(unix)]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let executable = false;
            frame(
                &mut hash,
                if executable {
                    b"executable"
                } else {
                    b"regular"
                },
            );
            let mut input = fs::File::open(&source)?;
            let mut output = fs::File::create(&destination)?;
            let mut file_hash = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                output.write_all(&buffer[..count])?;
                file_hash.update(&buffer[..count]);
            }
            output.flush()?;
            frame(&mut hash, &file_hash.finalize());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    &destination,
                    fs::Permissions::from_mode(if executable { 0o700 } else { 0o600 }),
                )?;
            }
            (executable, None)
        };
        files.push(SourceFile {
            path: name.clone(),
            executable,
            symlink_target,
        });
    }
    let mut bundle = None;
    if let Some(base) = &public_base {
        if base.revision != revision {
            let path = staging.path().join("commits.bundle");
            let bundle_path = path.to_str().context("bundle path")?;
            // A temporary named ref pins captured HEAD even if the live ref moves.
            let reference = format!("refs/ctox-build-source/{}", uuid::Uuid::new_v4());
            git_output(&root, &["update-ref", &reference, &revision, ""])?;
            let range = format!("{}..{}", base.revision, reference);
            let bundled = git_output(&root, &["bundle", "create", bundle_path, &range]);
            let removed = git_output(&root, &["update-ref", "-d", &reference, &revision]);
            bundled?;
            removed?;
            bundle = Some(path);
        }
    }
    anyhow::ensure!(
        head(&root)? == revision,
        "source HEAD changed during capture"
    );
    let overlay_paths = files
        .iter()
        .filter(|file| overlay_names.contains(&file.path))
        .map(|file| file.path.clone())
        .collect();
    Ok(CapturedBuildSource {
        staging,
        head_revision: revision,
        source_id: format!("{:x}", hash.finalize()),
        files,
        deleted_paths,
        public_base,
        overlay_paths,
        bundle,
    })
}

/// Frozen worker source, including reconstructable Git history. Unlike ordinary
/// private Cargo source capture, this always supplies a commit bundle.
#[derive(Debug)]
pub struct CapturedWorkerSource {
    pub(crate) source: CapturedBuildSource,
    pub(crate) base_revision: String,
}

impl CapturedWorkerSource {
    pub fn source(&self) -> &CapturedBuildSource {
        &self.source
    }

    pub fn base_revision(&self) -> &str {
        &self.base_revision
    }
}

/// Capture an exact ancestor base, HEAD and working tree for a fresh worker.
/// Private bundles include the complete HEAD ancestry; no local .git directory,
/// configuration, credentials or hooks are exported. Public bases retain the
/// existing anonymous-fetch requirement. The caller fences concurrent edits.
pub fn capture_worker(
    root: &Path,
    staging_root: &Path,
    base_revision: &str,
    public_base: Option<PublicGithubBase>,
) -> Result<CapturedWorkerSource> {
    validate_revision(base_revision)?;
    if let Some(base) = &public_base {
        anyhow::ensure!(base.revision == base_revision, "worker public base differs");
    }
    let mut source = capture(root, staging_root, public_base)?;
    git_output(
        root,
        &[
            "merge-base",
            "--is-ancestor",
            base_revision,
            &source.head_revision,
        ],
    )?;
    if source.public_base.is_none() {
        let bundle = source.staging.path().join("commits.bundle");
        let reference = format!("refs/ctox-build-source/{}", uuid::Uuid::new_v4());
        git_output(root, &["update-ref", &reference, &source.head_revision, ""])?;
        let bundled = git_output(
            root,
            &[
                "bundle",
                "create",
                bundle.to_str().context("worker bundle path")?,
                &reference,
            ],
        );
        let removed = git_output(
            root,
            &["update-ref", "-d", &reference, &source.head_revision],
        );
        bundled?;
        removed?;
        source.bundle = Some(bundle);
    }
    anyhow::ensure!(
        head(root)? == source.head_revision,
        "source HEAD changed during worker capture"
    );
    Ok(CapturedWorkerSource {
        source,
        base_revision: base_revision.to_owned(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("repo");
        let staging = directory.path().join("staging");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir(&staging).unwrap();
        git_output(&root, &["init", "-q"]).unwrap();
        git_output(
            &root,
            &["config", "user.email", "source-test@example.invalid"],
        )
        .unwrap();
        git_output(&root, &["config", "user.name", "SourceFixture"]).unwrap();
        fs::write(root.join(".gitignore"), "ignored\n").unwrap();
        fs::write(root.join("tracked"), "committed").unwrap();
        git_output(&root, &["add", "."]).unwrap();
        git_output(&root, &["commit", "-qm", "base"]).unwrap();
        (directory, root, staging)
    }

    #[test]
    fn untracked_content_changes_identity_and_frozen_bytes_survive_live_edits() {
        let (_directory, root, staging) = fixture();
        fs::write(root.join("untracked"), "first").unwrap();
        fs::write(root.join("ignored"), "ignored secret").unwrap();
        let first = capture(&root, &staging, None).unwrap();
        fs::write(root.join("untracked"), "second").unwrap();
        let second = capture(&root, &staging, None).unwrap();
        assert_ne!(first.source_id, second.source_id);
        assert_eq!(fs::read(first.tree().join("untracked")).unwrap(), b"first");
        assert!(!first.tree().join("ignored").exists());
        assert!(!first.tree().join(".git").exists());
        let path = first.tree().parent().unwrap().to_owned();
        drop(first);
        assert!(!path.exists());
        assert!(root.join("tracked").exists());
    }

    #[test]
    fn public_bundle_and_overlay_reconstruct_unpushed_head_and_dirty_tree() {
        let (_directory, root, staging) = fixture();
        let base = head(&root).unwrap();
        fs::write(root.join("committed-later"), "local commit").unwrap();
        git_output(&root, &["add", "."]).unwrap();
        git_output(&root, &["commit", "-qm", "unpushed"]).unwrap();
        fs::remove_file(root.join("tracked")).unwrap();
        fs::write(root.join("name\nwith'quotes"), "dirty").unwrap();
        symlink("committed-later", root.join("link")).unwrap();
        let snapshot = capture(
            &root,
            &staging,
            Some(PublicGithubBase {
                repository: "metric-space-ai/ctox".into(),
                revision: base,
            }),
        )
        .unwrap();
        assert!(snapshot.bundle.as_ref().unwrap().is_file());
        assert_eq!(snapshot.deleted_paths, vec!["tracked"]);
        assert!(snapshot
            .overlay_paths
            .contains(&"name\nwith'quotes".to_owned()));
        assert!(snapshot.overlay_paths.contains(&"link".to_owned()));
        let rebuilt = snapshot.tree().parent().unwrap().join("rebuilt");
        fs::create_dir(&rebuilt).unwrap();
        git_output(&rebuilt, &["init", "-q"]).unwrap();
        git_output(
            &rebuilt,
            &[
                "fetch",
                "-q",
                root.to_str().unwrap(),
                &snapshot.public_base.as_ref().unwrap().revision,
            ],
        )
        .unwrap();
        git_output(
            &rebuilt,
            &[
                "bundle",
                "unbundle",
                snapshot.bundle.as_ref().unwrap().to_str().unwrap(),
            ],
        )
        .unwrap();
        git_output(
            &rebuilt,
            &["checkout", "-q", "--detach", &snapshot.head_revision],
        )
        .unwrap();
        assert_eq!(
            fs::read(rebuilt.join("committed-later")).unwrap(),
            b"local commit"
        );
        for path in &snapshot.deleted_paths {
            fs::remove_file(rebuilt.join(path)).unwrap();
        }
        for path in &snapshot.overlay_paths {
            let frozen = snapshot.tree().join(path);
            let destination = rebuilt.join(path);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            if fs::symlink_metadata(&frozen)
                .unwrap()
                .file_type()
                .is_symlink()
            {
                symlink(fs::read_link(&frozen).unwrap(), &destination).unwrap();
            } else {
                fs::copy(&frozen, &destination).unwrap();
            }
        }
        assert!(!rebuilt.join("tracked").exists());
        assert_eq!(
            fs::read(rebuilt.join("name\nwith'quotes")).unwrap(),
            b"dirty"
        );
        assert_eq!(
            fs::read_link(rebuilt.join("link")).unwrap(),
            PathBuf::from("committed-later")
        );
        let refs = git_output(&root, &["for-each-ref", "refs/ctox-build-source"]).unwrap();
        assert!(refs.stdout.is_empty());
    }

    #[test]
    fn executable_mode_and_symlink_are_identity_and_parent_symlinks_fail_closed() {
        let (_directory, root, staging) = fixture();
        let first = capture(&root, &staging, None).unwrap();
        fs::set_permissions(root.join("tracked"), fs::Permissions::from_mode(0o755)).unwrap();
        let second = capture(&root, &staging, None).unwrap();
        assert_ne!(first.source_id, second.source_id);
        assert!(
            second
                .files
                .iter()
                .find(|file| file.path == "tracked")
                .unwrap()
                .executable
        );
        fs::create_dir(root.join("parent")).unwrap();
        fs::write(root.join("parent/child"), "owned").unwrap();
        git_output(&root, &["add", "."]).unwrap();
        git_output(&root, &["commit", "-qm", "parent"]).unwrap();
        let outside = staging.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("child"), "outside").unwrap();
        fs::remove_dir_all(root.join("parent")).unwrap();
        symlink(&outside, root.join("parent")).unwrap();
        assert!(capture(&root, &staging, None).is_err());
        assert!(capture(&root, &root, None).is_err());
        assert!(paths(b"../outside\0").is_err());
    }
}
