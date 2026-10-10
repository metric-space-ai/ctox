//! The upstream acceptance tests for `pkg/filecollector`.
//!
//! Both tests in `pkg/filecollector/file_collector_test.go` are ported here
//! 1:1, plus tests for the branches upstream leaves to the filesystem double
//! it happens to use.
//!
//! The upstream tests run against `memoryFs`, a go-billy `memfs` wrapper whose
//! `Walk` differs from Go's real `filepath.Walk` in one decisive way: it
//! reports the walk root as `<root>/.` rather than `<root>`. That is not
//! cosmetic. `FileCollector` skips a directory whose last path component is
//! `.`, so under the memory filesystem the root is never subjected to the
//! ignore filter, while under `filepath.Walk` it is. A workspace directory
//! whose own name matches a `.gitignore` pattern is therefore copied in full
//! by act but skipped by the test double's shape. [`DotRootFs`] reproduces the
//! double so the upstream assertions hold verbatim, and
//! `real_walk_filters_the_root_by_name` pins the production behaviour
//! separately.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

use ctox_actions_runner::filecollector::{
    Cancellation, CopyCollector, DefaultFs, FileCollector, Fs, TarCollector, WalkOutcome,
};
use ctox_actions_runner::filecollector::FileInfo;
use ctox_actions_runner::git_index::{GitIndex, IndexError};
use ctox_actions_runner::gitignore;

/// A [`Fs`] that mirrors act's test double, `memoryFs`.
///
/// It differs from Go's real `filepath.Walk` in two ways that matter to
/// `FileCollector`, so both are reproduced exactly:
///
/// * the walk root is reported as `<root>/.` — which is the only place a `.`
///   component ever appears, and the one branch of the collector that exists
///   solely because of it; and
/// * the root is reported *once*, not again when the children are listed.
///   go-billy's `Memory.ReadDir` plus `memoryFs.walk` iterate a snapshot of the
///   directory without re-emitting the directory itself.
///
/// The index, the reads and the link reads are the real filesystem, which is
/// what the production call site uses.
struct DotRootFs {
    inner: DefaultFs,
}

impl DotRootFs {
    fn walk_children(
        &self,
        dir: &Path,
        visit: &mut dyn FnMut(&Path, std::io::Result<FileInfo>) -> WalkOutcome,
    ) -> std::io::Result<()> {
        let mut names: Vec<PathBuf> = fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().into())
            .collect();
        names.sort();
        for name in names {
            let child = dir.join(&name);
            let info = FileInfo::lstat(&child)?;
            let is_dir = info.mode.is_dir();
            let outcome = visit(&child, Ok(info));
            if is_dir && outcome != WalkOutcome::SkipDir {
                self.walk_children(&child, visit)?;
            }
        }
        Ok(())
    }
}

impl Fs for DotRootFs {
    fn walk(
        &self,
        root: &Path,
        visit: &mut dyn FnMut(&Path, std::io::Result<FileInfo>) -> WalkOutcome,
    ) -> std::io::Result<()> {
        let info = FileInfo::lstat(root)?;
        // `PathBuf::push` appends verbatim, so this is literally `<root>/.`
        // and not the cleaned `<root>`.
        let with_dot = root.join(".");
        assert!(
            with_dot.to_string_lossy().ends_with("/."),
            "the root must be reported with a trailing dot"
        );
        if visit(&with_dot, Ok(info)) == WalkOutcome::SkipDir {
            return Ok(());
        }
        self.walk_children(root, visit)
    }

    fn open_git_index(&self, path: &Path) -> Result<GitIndex, IndexError> {
        self.inner.open_git_index(path)
    }

    fn open(&self, path: &Path) -> std::io::Result<fs::File> {
        self.inner.open(path)
    }

    fn read_link(&self, path: &Path) -> std::io::Result<PathBuf> {
        self.inner.read_link(path)
    }
}

/// Runs a git command in `dir` with the user's global configuration removed,
/// so the fixtures do not depend on what the developer has configured.
fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git must be available to run the filecollector tests");
    assert!(
        status.success(),
        "git {args:?} failed in {}",
        dir.display()
    );
}

fn write(repo: &Path, name: &str, contents: &str) {
    let path = repo.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

/// One entry read back out of the finished tar.
#[derive(Debug)]
struct TarEntry {
    name: String,
    link_name: Option<String>,
    mode: u32,
    contents: Vec<u8>,
}

fn read_tar(data: &[u8]) -> Vec<TarEntry> {
    let mut archive = tar::Archive::new(Cursor::new(data));
    let mut entries = Vec::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_string_lossy().into_owned();
        let link_name = entry
            .link_name()
            .ok()
            .flatten()
            .map(|p| p.to_string_lossy().into_owned());
        let mode = entry.header().mode().unwrap();
        let mut contents = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut contents).unwrap();
        entries.push(TarEntry {
            name,
            link_name,
            mode,
            contents,
        });
    }
    entries
}

/// Which prefix act strips from the walked paths.
///
/// act's two call sites disagree, and the difference is observable, so both are
/// available here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrefixStyle {
    /// `HostEnvironment` and `local_repository_cache`: `src_path` plus a
    /// separator. The walk root therefore strips to the empty string.
    SrcPath,
    /// `containerReference.copyDir`: `filepath.Dir(src_path)` plus a
    /// separator. The walk root keeps its own base name.
    Parent,
}

fn src_prefix(repo: &Path, style: PrefixStyle) -> String {
    let base = match style {
        PrefixStyle::SrcPath => repo.to_path_buf(),
        PrefixStyle::Parent => repo
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| repo.to_path_buf()),
    };
    format!("{}{}", base.display(), std::path::MAIN_SEPARATOR)
}

/// Collects `repo` into a tar, mirroring the `FileCollector` construction both
/// upstream tests use.
fn collect_tar(
    fs: &dyn Fs,
    repo: &Path,
    use_gitignore: bool,
) -> std::io::Result<Vec<TarEntry>> {
    collect_tar_with(fs, repo, use_gitignore, PrefixStyle::SrcPath)
}

fn collect_tar_with(
    fs: &dyn Fs,
    repo: &Path,
    use_gitignore: bool,
    style: PrefixStyle,
) -> std::io::Result<Vec<TarEntry>> {
    let ignorer = if use_gitignore {
        // act: `gitignore.ReadPatterns(osfs.New(srcPath), nil)`, whose error
        // is logged and the partial result used.
        Some(gitignore::matcher_for(repo))
    } else {
        None
    };

    let mut buffer: Vec<u8> = Vec::new();
    {
        let mut tar = tar::Builder::new(&mut buffer);
        let mut collector = TarCollector {
            tar: &mut tar,
            uid: 0,
            gid: 0,
            dst_dir: String::new(),
        };
        let mut file_collector = FileCollector {
            fs,
            ignorer,
            src_path: repo.to_path_buf(),
            src_prefix: src_prefix(repo, style),
            handler: &mut collector,
            cancellation: Cancellation::new(),
        };
        file_collector.collect_files(repo, &[])?;
        tar.finish()?;
    }
    Ok(read_tar(&buffer))
}

// ---------------------------------------------------------------------------
// file_collector_test.go: TestIgnoredTrackedfile
// ---------------------------------------------------------------------------

/// A `.gitignore` containing `.*` ignores `.env`, but `.gitignore` itself is
/// tracked, and a tracked file is always copied. `.env` must not reach the tar
/// and `.gitignore` must be the only entry in it.
///
/// `-f` is needed where upstream's go-git `Worktree.Add` was not: `git add`
/// refuses an explicitly ignored path.
#[test]
fn ignored_tracked_file() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("mygitrepo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "."]);

    write(&repo, ".gitignore", ".*\n");
    // This file shouldn't be in the tar.
    write(&repo, ".env", "test=val1\n");
    // .gitignore is in the tar after adding it to the index.
    git(&repo, &["add", "-f", ".gitignore"]);

    let entries = collect_tar(&DotRootFs { inner: DefaultFs }, &repo, true).unwrap();

    assert_eq!(entries.len(), 1, "tar must only contain one element: {entries:?}");
    assert_eq!(entries[0].name, ".gitignore");
    assert_eq!(entries[0].contents, b".*\n");
}

// ---------------------------------------------------------------------------
// file_collector_test.go: TestSymlinks
// ---------------------------------------------------------------------------

/// Symlinks are recorded as symlinks with their target, never followed: the
/// tar holds `.env` with its contents and `test.env` as a link to `.env`.
#[test]
fn symlinks_are_preserved_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("mygitrepo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "."]);

    write(&repo, ".env", "test=val1\n");
    symlink(".env", &repo.join("test.env"));
    git(&repo, &["add", ".env", "test.env"]);

    let entries = collect_tar(&DotRootFs { inner: DefaultFs }, &repo, true).unwrap();

    let env = entries.iter().find(|e| e.name == ".env").expect(".env");
    assert_eq!(env.contents, b"test=val1\n");
    assert_eq!(env.link_name, None, ".env is a regular file");

    let test_env = entries
        .iter()
        .find(|e| e.name == "test.env")
        .expect("test.env");
    assert_eq!(test_env.link_name.as_deref(), Some(".env"));
    assert!(
        test_env.contents.is_empty(),
        "a symlink entry carries no payload of its own"
    );
    // act writes Go's raw `int64(fi.Mode())`, so the symlink type bit is in the
    // mode field: `0o400000000 | perm`. That needs ten octal digits, does not
    // fit the eight-byte tar mode field, and Go's writer responds by dropping
    // the whole entry to GNU base-256 — which the `tar` crate cannot read back.
    // This port writes the permission bits instead, so the entry stays USTAR
    // and round-trips. Every extractor masks to `0o7777` anyway.
    let raw = FileInfo::lstat(&repo.join("test.env")).unwrap();
    assert_ne!(
        raw.mode.raw(),
        raw.mode.open_mode(),
        "the raw Go FileMode must still carry a type bit, or this test proves nothing"
    );
    assert_eq!(test_env.mode, raw.mode.open_mode());
}

fn symlink(target: &str, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link).unwrap();
}

// ---------------------------------------------------------------------------
// Branches upstream leaves to its filesystem double
// ---------------------------------------------------------------------------

/// The walk root is subject to the ignore filter. With the prefix act uses
/// alongside an ignorer — `filepath.Dir(src_path)`, which is what
/// `containerReference.copyDir` and `HostEnvironment.CopyDir` both pass — the
/// root is reduced to its own base name, so a workspace directory called
/// `.hidden` is pruned outright by a `.*` rule.
#[test]
fn the_walk_root_is_filtered_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join(".hidden");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "."]);
    write(&repo, "keep.txt", "kept\n");
    write(&repo, ".gitignore", ".*\n");
    git(&repo, &["add", "-f", "keep.txt"]);

    let entries = collect_tar_with(&DefaultFs, &repo, true, PrefixStyle::Parent).unwrap();
    assert!(
        entries.is_empty(),
        "the root `.hidden` matches `.*` and is pruned with everything under it: {entries:?}"
    );
}

/// The reason act does not hit that: the other prefix — `src_path` itself, used
/// by `HostEnvironment.GetTarStream` and `local_repository_cache` — is only
/// ever paired with a nil ignorer, and without one nothing is filtered. Under
/// that prefix `TrimPrefix` does not even match the walk root, so the matched
/// path would be the whole absolute path, ancestors included.
#[test]
fn act_never_pairs_the_src_path_prefix_with_an_ignorer() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join(".hidden");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "."]);
    write(&repo, "keep.txt", "kept\n");
    write(&repo, ".gitignore", ".*\n");
    write(&repo, ".env", "test=val1\n");
    git(&repo, &["add", "-f", "keep.txt"]);

    // No ignorer, the `src_path` prefix: what act actually does.
    let entries =
        collect_tar_with(&DotRootFs { inner: DefaultFs }, &repo, false, PrefixStyle::SrcPath)
            .unwrap();
    let names: Vec<&str> = entries
        .iter()
        .filter(|e| !e.name.starts_with(".git/"))
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, [".env", ".gitignore", "keep.txt"]);

    // The combination act never uses would prune the tree, because the
    // absolute path is what the rules are tested against.
    let entries = collect_tar_with(&DefaultFs, &repo, true, PrefixStyle::SrcPath).unwrap();
    assert!(
        entries.is_empty(),
        "an ignorer plus the `src_path` prefix tests the whole absolute path: {entries:?}"
    );
}

/// An ignored directory is still entered when it holds a tracked file, so a
/// build sees the `node_modules` git knows about. Without a tracked file the
/// whole directory is pruned.
#[test]
fn ignored_directory_survives_only_with_a_tracked_file() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("mygitrepo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "."]);

    write(&repo, ".gitignore", "build/\n");
    write(&repo, "build/output.txt", "artifact\n");
    write(&repo, "build/ignored.txt", "noise\n");
    write(&repo, "keep.txt", "kept\n");
    // Only `build/output.txt` is tracked; `build/ignored.txt` is not.
    git(&repo, &["add", "-f", "build/output.txt", "keep.txt"]);

    let entries = collect_tar(&DotRootFs { inner: DefaultFs }, &repo, true).unwrap();
    let names: Vec<&str> = entries
        .iter()
        .filter(|e| !e.name.starts_with(".git/"))
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(
        names,
        [".gitignore", "build/output.txt", "keep.txt"],
        "the ignored directory is entered, but its untracked file is still dropped"
    );
}

/// An ignored directory with nothing tracked inside it is pruned entirely, and
/// the probe looks one level down, so a deeply nested tracked file counts.
#[test]
fn ignored_directory_is_pruned_when_nothing_is_tracked() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("mygitrepo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "."]);

    write(&repo, ".gitignore", "build/\n");
    write(&repo, "build/a/b/c.txt", "deep\n");
    git(&repo, &["add", "-f", ".gitignore"]);

    let entries = collect_tar(&DotRootFs { inner: DefaultFs }, &repo, true).unwrap();
    let names: Vec<&str> = entries
        .iter()
        .filter(|e| !e.name.starts_with(".git/"))
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, [".gitignore"]);
}

/// Without an ignorer every file is copied, tracked or not. This is the
/// `Ignorer == nil` path both `HostEnvironment` and `local_repository_cache`
/// use.
#[test]
fn without_an_ignorer_everything_is_copied() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("mygitrepo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "."]);

    write(&repo, ".gitignore", ".*\n");
    write(&repo, ".env", "test=val1\n");
    write(&repo, "main.go", "package main\n");
    git(&repo, &["add", "-f", "main.go"]);

    let entries = collect_tar(&DotRootFs { inner: DefaultFs }, &repo, false).unwrap();
    let names: Vec<&str> = entries
        .iter()
        .filter(|e| !e.name.starts_with(".git/"))
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, [".env", ".gitignore", "main.go"]);
}

/// A non-regular file is dropped rather than read. Only unix has fifos, and
/// the tar would not carry one either way.
#[test]
#[cfg(unix)]
fn non_regular_files_are_dropped() {
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("mygitrepo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "."]);
    write(&repo, "keep.txt", "kept\n");
    git(&repo, &["add", "-f", "keep.txt"]);

    let fifo = repo.join("pipe");
    let c_path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(
        unsafe { libc_mkfifo(c_path.as_ptr(), 0o644) },
        0,
        "the test needs a fifo"
    );

    let entries = collect_tar(&DotRootFs { inner: DefaultFs }, &repo, true).unwrap();
    let names: Vec<&str> = entries
        .iter()
        .filter(|e| !e.name.starts_with(".git/"))
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, ["keep.txt"], "the fifo is not a regular file");
}

#[cfg(unix)]
extern "C" {
    #[link_name = "mkfifo"]
    fn libc_mkfifo(path: *const std::os::raw::c_char, mode: u32) -> std::os::raw::c_int;
}

/// The host runner's copy path: the same filters, writing to a directory.
#[test]
fn copy_collector_mirrors_the_directory_layout() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("mygitrepo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "."]);

    write(&repo, ".gitignore", ".*\n");
    write(&repo, ".env", "test=val1\n");
    write(&repo, "pkg/nested/file.txt", "deep\n");
    symlink(".env", &repo.join("link.env"));
    // `.gitignore` is tracked, so it survives its own `.*` pattern.
    git(
        &repo,
        &["add", "-f", ".env", ".gitignore", "pkg/nested/file.txt", "link.env"],
    );

    let dest = dir.path().join("dest");
    {
        let mut collector = CopyCollector {
            dst_dir: dest.clone(),
        };
        let mut file_collector = FileCollector {
            fs: &DotRootFs { inner: DefaultFs },
            ignorer: Some(gitignore::matcher_for(&repo)),
            src_path: repo.clone(),
            src_prefix: src_prefix(&repo, PrefixStyle::SrcPath),
            handler: &mut collector,
            cancellation: Cancellation::new(),
        };
        file_collector.collect_files(&repo, &[]).unwrap();
    }

    assert_eq!(fs::read(dest.join(".gitignore")).unwrap(), b".*\n");
    assert_eq!(fs::read(dest.join(".env")).unwrap(), b"test=val1\n");
    assert_eq!(
        fs::read(dest.join("pkg/nested/file.txt")).unwrap(),
        b"deep\n"
    );
    assert_eq!(
        fs::read_link(dest.join("link.env")).unwrap(),
        PathBuf::from(".env")
    );
    assert!(
        !dest.join(".git").exists(),
        "the ignored .git directory is not copied"
    );
}

/// A cancelled run stops with act's `copy cancelled` error rather than
/// silently producing a partial archive.
#[test]
fn cancellation_aborts_the_walk() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("mygitrepo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "."]);
    write(&repo, "a.txt", "a\n");
    write(&repo, "b.txt", "b\n");
    git(&repo, &["add", "-f", "a.txt", "b.txt"]);

    let mut buffer: Vec<u8> = Vec::new();
    let cancellation = Cancellation::new();
    cancellation.cancel();
    {
        let mut tar = tar::Builder::new(&mut buffer);
        let mut collector = TarCollector {
            tar: &mut tar,
            uid: 0,
            gid: 0,
            dst_dir: String::new(),
        };
        let mut file_collector = FileCollector {
            fs: &DotRootFs { inner: DefaultFs },
            ignorer: None,
            src_path: repo.clone(),
            src_prefix: src_prefix(&repo, PrefixStyle::SrcPath),
            handler: &mut collector,
            cancellation: cancellation.clone(),
        };
        let err = file_collector.collect_files(&repo, &[]).unwrap_err();
        assert_eq!(err.to_string(), "copy cancelled");
    }
}
