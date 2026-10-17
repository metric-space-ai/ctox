//! The slice of go-git's `plumbing/format/index` that act's `filecollector`
//! uses, backed by [`gix`].
//!
//! `pkg/filecollector` asks a repository two things: whether a path is tracked
//! (`Index.Entry`) and whether any tracked file lives below a directory
//! (`Index.Glob` with a `**` suffix). It reads neither the blob ids nor the
//! stat data, so this module keeps only the two fields that are consulted —
//! the path and the file mode — and re-implements `Glob` on top of
//! [`gomatch::match_full_path`] rather than pulling in go-git's whole index
//! decoder.
//!
//! Upstream is go-git (Apache-2.0, Copyright (c) 2015 go-git authors).

use std::io;
use std::path::{Path, PathBuf};
use std::str;

use crate::gomatch::{self, MatchResult};

/// go-git's `filemode.Submodule`, i.e. a gitlink entry.
pub const SUBMODULE_MODE: u32 = 0o160000;

/// Why an index could not be read.
///
/// act calls `git.PlainOpen(...).Storer.Index()` and discards the error: a
/// directory that is not a git repository simply has no index, which makes
/// every path look untracked. The error is therefore carried rather than
/// swallowed here, and the caller decides.
#[derive(Debug)]
pub enum IndexError {
    /// No git directory was found at or above `path`.
    NotARepository(PathBuf),
    /// The index file could not be read or parsed.
    Unreadable {
        /// The index file that failed.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotARepository(path) => {
                write!(f, "repository does not exist: {}", path.display())
            }
            Self::Unreadable { path, source } => {
                write!(f, "unable to read index {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for IndexError {}

/// One tracked path, reduced to what `filecollector` looks at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// Path relative to the repository root, always `/`-separated.
    pub path: String,
    /// The raw git mode, e.g. `0o100644`.
    pub mode: u32,
}

impl IndexEntry {
    /// True when this entry is a gitlink, i.e. a submodule.
    pub fn is_submodule(&self) -> bool {
        self.mode == SUBMODULE_MODE
    }
}

/// The tracked paths of one repository.
#[derive(Debug, Clone, Default)]
pub struct GitIndex {
    entries: Vec<IndexEntry>,
}

impl GitIndex {
    /// Opens the index of the repository containing `path`.
    ///
    /// Mirrors go-git's `git.PlainOpen(path).Storer.Index()`: the search
    /// starts at `path` and walks up towards the filesystem root until a
    /// `.git` directory, a `.git` file pointing elsewhere, or a bare git
    /// directory is found.
    pub fn open(path: &Path) -> Result<Self, IndexError> {
        let git_dir =
            resolve_git_dir(path).ok_or_else(|| IndexError::NotARepository(path.to_path_buf()))?;
        let index_path = git_dir.join("index");
        Self::read(&index_path)
    }

    /// Reads an index file directly.
    ///
    /// The object hash width is not recorded in the index itself but in the
    /// repository config, and it changes the size of every entry, so a SHA-256
    /// repository cannot be read with the SHA-1 default. The first attempt
    /// assumes SHA-1 and a failure is retried as SHA-256.
    pub fn read(index_path: &Path) -> Result<Self, IndexError> {
        let attempt = |kind| {
            gix::index::File::at(index_path, kind, false, Default::default()).map_err(|err| {
                IndexError::Unreadable {
                    path: index_path.to_path_buf(),
                    source: io::Error::other(err.to_string()),
                }
            })
        };

        let file = match attempt(gix::hash::Kind::Sha1) {
            Ok(file) => file,
            Err(sha1_error) => attempt(gix::hash::Kind::Sha256).map_err(|_| sha1_error)?,
        };
        let entries = file
            .entries()
            .iter()
            .map(|entry| IndexEntry {
                path: str::from_utf8(entry.path(&file))
                    .unwrap_or_default()
                    .to_string(),
                mode: entry.mode.bits(),
            })
            .collect();
        Ok(GitIndex { entries })
    }

    /// All tracked entries, in index order.
    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }

    /// The entry for `path`, or `None` when it is not tracked.
    ///
    /// Port of go-git's `Index.Entry`, including its linear scan.
    pub fn entry(&self, path: &str) -> Option<&IndexEntry> {
        let path = to_slash(path);
        self.entries.iter().find(|entry| entry.path == path)
    }

    /// Every entry whose path matches `pattern`.
    ///
    /// Port of go-git's `Index.Glob`. The matcher is path-*in*sensitive, so
    /// `vendor/**` reaches into nested directories. A malformed pattern is an
    /// error rather than a miss, exactly as upstream: `filecollector` treats
    /// both the same way, by skipping the directory.
    pub fn glob(&self, pattern: &str) -> Result<Vec<&IndexEntry>, GlobError> {
        let pattern = to_slash(pattern);
        let mut matches = Vec::new();
        for entry in &self.entries {
            match gomatch::match_full_path(&pattern, &entry.path) {
                MatchResult::Matched => matches.push(entry),
                MatchResult::NoMatch => {}
                MatchResult::BadPattern => return Err(GlobError(pattern)),
            }
        }
        Ok(matches)
    }
}

/// The pattern in a [`GitIndex::glob`] call was malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobError(pub String);

impl std::fmt::Display for GlobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "syntax error in pattern: {}", self.0)
    }
}

impl std::error::Error for GlobError {}

/// Go's `filepath.ToSlash`: on Windows a backslash becomes a slash, on Unix
/// this is the identity.
fn to_slash(path: &str) -> String {
    if std::path::MAIN_SEPARATOR == '/' {
        path.to_string()
    } else {
        path.replace('\\', "/")
    }
}

/// Finds the git directory governing `path`, the way `git.PlainOpen` does.
fn resolve_git_dir(path: &Path) -> Option<PathBuf> {
    let start = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut current = Some(start.as_path());
    while let Some(dir) = current {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        if dot_git.is_file() {
            if let Some(target) = read_gitdir_file(&dot_git, dir) {
                return Some(target);
            }
        }
        // A bare repository: the directory itself is the git directory.
        if dir.join("HEAD").is_file() && dir.join("objects").is_dir() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// Parses a `.git` file, whose single line is `gitdir: <path>`.
///
/// A relative target is resolved against the directory holding the file, which
/// is how submodule and worktree checkouts record their real git directory.
fn read_gitdir_file(file: &Path, base: &Path) -> Option<PathBuf> {
    let contents = std::fs::read_to_string(file).ok()?;
    let rest = contents.trim().strip_prefix("gitdir:")?.trim();
    let target = Path::new(rest);
    Some(if target.is_absolute() {
        target.to_path_buf()
    } else {
        base.join(target)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    /// go-git's `index_test.go` `TestIndexGlob` fixtures, reached through
    /// [`GitIndex::glob`].
    #[test]
    fn glob_matches_upstream_fixtures() {
        let index = GitIndex {
            entries: vec![
                IndexEntry {
                    path: "foo/bar/bar".into(),
                    mode: 0o100644,
                },
                IndexEntry {
                    path: "foo/baz/qux".into(),
                    mode: 0o100644,
                },
                IndexEntry {
                    path: "fux".into(),
                    mode: 0o100644,
                },
            ],
        };

        let names = |pattern: &str| -> Vec<String> {
            index
                .glob(pattern)
                .unwrap()
                .into_iter()
                .map(|entry| entry.path.clone())
                .collect()
        };

        assert_eq!(names("foo/b*"), ["foo/bar/bar", "foo/baz/qux"]);
        assert_eq!(names("f*").len(), 3);
        assert_eq!(names("f*/baz/q*"), ["foo/baz/qux"]);
        assert!(names("f*/baz/z*").is_empty());
    }

    #[test]
    fn glob_rejects_a_malformed_pattern() {
        let index = GitIndex {
            entries: vec![IndexEntry {
                path: "foo".into(),
                mode: 0o100644,
            }],
        };
        assert_eq!(index.glob("f[o"), Err(GlobError("f[o".into())));
    }

    #[test]
    fn entry_finds_tracked_paths() {
        let index = GitIndex {
            entries: vec![
                IndexEntry {
                    path: "foo".into(),
                    mode: 0o100644,
                },
                IndexEntry {
                    path: "bar".into(),
                    mode: 0o100755,
                },
                IndexEntry {
                    path: "deps/sub".into(),
                    mode: SUBMODULE_MODE,
                },
            ],
        };
        assert_eq!(index.entry("foo").unwrap().mode, 0o100644);
        assert!(index.entry("missing").is_none());
        assert!(index.entry("deps/sub").unwrap().is_submodule());
        assert!(!index.entry("bar").unwrap().is_submodule());
    }

    #[test]
    fn opening_a_non_repository_fails() {
        let dir = tempfile::tempdir().unwrap();
        let err = GitIndex::open(dir.path()).unwrap_err();
        assert!(matches!(err, IndexError::NotARepository(_)), "{err}");
    }

    #[test]
    fn read_gitdir_file_resolves_relative_targets() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sub/.git");
        write(&file, "gitdir: ../real-git-dir\n");
        assert_eq!(
            read_gitdir_file(&file, &dir.path().join("sub")).unwrap(),
            dir.path().join("sub/../real-git-dir")
        );
    }
}
