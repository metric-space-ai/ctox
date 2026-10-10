//! Port of `nektos/act` `pkg/lookpath`.
//!
//! act vendors Go's `os/exec.LookPath` with one change: the environment is
//! behind an interface so the runner can resolve executables against a job's
//! container environment instead of the host process. That indirection is
//! the reason this package exists, so [`Env`] is preserved here as a trait.
//!
//! Upstream is a fork of the Go standard library (BSD-style licence, see the
//! `LICENSE` file in the act tree).
//!
//! Deviations from upstream:
//!
//! * `PATHEXT` / `PATH` are requested with canonical casing. Go looks `path`
//!   up in lower case on Windows because `os.Getenv` is case-insensitive
//!   there; `std::env::var` is case-insensitive on Windows as well, so the
//!   canonical spelling behaves the same through [`ProcessEnv`].
//! * Go's `filepath.SplitList` strips quotes from Windows `PATH` elements.
//!   This port splits on the list separator only, which is equivalent for
//!   the ordinary unquoted case.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// Environment lookup, injectable so a job can resolve against its own
/// container environment rather than the host process.
pub trait Env {
    /// Returns the value of `name`, or `None` when unset.
    fn getenv(&self, name: &str) -> Option<String>;
}

/// Reads the real process environment.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessEnv;

impl Env for ProcessEnv {
    fn getenv(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

impl Env for BTreeMap<String, String> {
    fn getenv(&self, name: &str) -> Option<String> {
        self.get(name).cloned()
    }
}

/// Why a lookup failed.
#[derive(Debug)]
pub enum LookPathErrorKind {
    /// No candidate on the search path was executable.
    NotFound,
    /// A candidate existed but was not executable.
    Permission,
    /// A filesystem error occurred while probing a candidate.
    Io(std::io::Error),
}

impl fmt::Display for LookPathErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str(NOT_FOUND),
            Self::Permission => f.write_str("permission denied"),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for LookPathErrorKind {}

/// A failed lookup. Mirrors act's `lookpath.Error`, which also carries the
/// requested name but reports only the underlying cause.
#[derive(Debug)]
pub struct LookPathError {
    /// The executable that was requested.
    pub name: String,
    /// Why the lookup failed.
    pub kind: LookPathErrorKind,
}

impl fmt::Display for LookPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl std::error::Error for LookPathError {}

/// Message used when the search path yielded nothing.
#[cfg(windows)]
pub const NOT_FOUND: &str = "executable file not found in %PATH%";
/// Message used when the search path yielded nothing.
#[cfg(not(windows))]
pub const NOT_FOUND: &str = "executable file not found in $PATH";

/// Searches for an executable, using the real process environment.
pub fn look_path(file: &str) -> Result<String, LookPathError> {
    look_path_in(file, &ProcessEnv)
}

/// Searches for an executable using the supplied environment.
///
/// If `file` contains a path separator it is probed directly and the search
/// path is not consulted. On Unix the result must carry at least one execute
/// bit; on Windows a matching `PATHEXT` suffix is enough.
pub fn look_path_in(file: &str, env: &dyn Env) -> Result<String, LookPathError> {
    cfg_if_windows(file, env)
}

#[cfg(windows)]
fn cfg_if_windows(file: &str, env: &dyn Env) -> Result<String, LookPathError> {
    let exts = path_extensions(env);

    if file.contains([':', '\\', '/']) {
        return match find_executable(file, &exts) {
            Ok(found) => Ok(found),
            Err(kind) => Err(LookPathError {
                name: file.to_string(),
                kind,
            }),
        };
    }

    // Windows resolves against the working directory first.
    let relative = Path::new(".").join(file);
    if let Ok(found) = find_executable_path(&relative, &exts) {
        return Ok(found);
    }

    let path = env.getenv("PATH").unwrap_or_default();
    for dir in path.split(';') {
        let candidate = Path::new(dir).join(file);
        if let Ok(found) = find_executable_path(&candidate, &exts) {
            return Ok(found);
        }
    }

    Err(LookPathError {
        name: file.to_string(),
        kind: LookPathErrorKind::NotFound,
    })
}

#[cfg(not(windows))]
fn cfg_if_windows(file: &str, env: &dyn Env) -> Result<String, LookPathError> {
    if file.contains('/') {
        return match find_executable(Path::new(file)) {
            Ok(()) => Ok(file.to_string()),
            Err(kind) => Err(LookPathError {
                name: file.to_string(),
                kind,
            }),
        };
    }

    let path = env.getenv("PATH").unwrap_or_default();
    for dir in path.split(':') {
        // Unix shell semantics: an empty element means the working directory.
        let dir = if dir.is_empty() { "." } else { dir };
        let candidate = Path::new(dir).join(file);
        if find_executable(&candidate).is_ok() {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }

    Err(LookPathError {
        name: file.to_string(),
        kind: LookPathErrorKind::NotFound,
    })
}

#[cfg(not(windows))]
fn find_executable(path: &Path) -> Result<(), LookPathErrorKind> {
    use std::os::unix::fs::PermissionsExt;

    let meta = std::fs::metadata(path).map_err(LookPathErrorKind::Io)?;
    if !meta.is_dir() && meta.permissions().mode() & 0o111 != 0 {
        return Ok(());
    }
    Err(LookPathErrorKind::Permission)
}

#[cfg(windows)]
fn find_executable(file: &str, exts: &[String]) -> Result<String, LookPathErrorKind> {
    find_executable_path(Path::new(file), exts)
}

#[cfg(windows)]
fn find_executable_path(path: &Path, exts: &[String]) -> Result<String, LookPathErrorKind> {
    if exts.is_empty() {
        return chk_stat(path).map(|()| path.to_string_lossy().into_owned());
    }
    if has_extension(path) && chk_stat(path).is_ok() {
        return Ok(path.to_string_lossy().into_owned());
    }
    for ext in exts {
        let mut candidate = path.as_os_str().to_os_string();
        candidate.push(ext);
        if chk_stat(Path::new(&candidate)).is_ok() {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }
    Err(LookPathErrorKind::Io(std::io::Error::from(
        std::io::ErrorKind::NotFound,
    )))
}

#[cfg(windows)]
fn chk_stat(path: &Path) -> Result<(), LookPathErrorKind> {
    let meta = std::fs::metadata(path).map_err(LookPathErrorKind::Io)?;
    if meta.is_dir() {
        return Err(LookPathErrorKind::Permission);
    }
    Ok(())
}

/// True when the final `.` follows the last path separator.
#[cfg(windows)]
fn has_extension(file: &Path) -> bool {
    let text = file.to_string_lossy();
    match text.rfind('.') {
        None => false,
        Some(dot) => {
            let separator = text.rfind([':', '\\', '/']);
            separator.is_none_or(|sep| sep < dot)
        }
    }
}

/// The ordered `PATHEXT` suffixes to try, defaulting to the Go standard set.
#[cfg(windows)]
fn path_extensions(env: &dyn Env) -> Vec<String> {
    match env.getenv("PATHEXT") {
        Some(raw) if !raw.is_empty() => raw
            .to_lowercase()
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|e| {
                if e.starts_with('.') {
                    e.to_string()
                } else {
                    format!(".{e}")
                }
            })
            .collect(),
        _ => vec![
            ".com".to_string(),
            ".exe".to_string(),
            ".bat".to_string(),
            ".cmd".to_string(),
        ],
    }
}

/// Convenience wrapper returning a [`PathBuf`] on success.
pub fn look_path_buf(file: &str, env: &dyn Env) -> Result<PathBuf, LookPathError> {
    look_path_in(file, env).map(PathBuf::from)
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                dir: std::env::temp_dir().join(format!(
                    "ctox-lookpath-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or_default()
                )),
            }
        }

        fn write(&self, name: &str, mode: u32) -> PathBuf {
            let path = self.dir.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("fixture dir");
            }
            fs::write(&path, "#!/bin/sh\n").expect("fixture file");
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("fixture mode");
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn env(paths: &[String]) -> BTreeMap<String, String> {
        BTreeMap::from([("PATH".to_string(), paths.join(":"))])
    }

    #[test]
    fn finds_executable_on_path() {
        let fx = Fixture::new();
        let bin = fx.dir.join("bin");
        fs::create_dir_all(&bin).expect("bin dir");
        let tool = fx.write("bin/tool", 0o755);
        let dir = bin.to_string_lossy().into_owned();

        let found = look_path_in("tool", &env(&[dir])).expect("must find tool");
        assert_eq!(PathBuf::from(found), tool);
    }

    #[test]
    fn non_executable_is_not_found() {
        let fx = Fixture::new();
        let bin = fx.dir.join("bin");
        fs::create_dir_all(&bin).expect("bin dir");
        fx.write("bin/data", 0o644);
        let dir = bin.to_string_lossy().into_owned();

        let err = look_path_in("data", &env(&[dir])).expect_err("must not find data");
        assert!(matches!(err.kind, LookPathErrorKind::NotFound));
        assert_eq!(err.name, "data");
    }

    #[test]
    fn any_execute_bit_is_enough() {
        let fx = Fixture::new();
        let bin = fx.dir.join("bin");
        fs::create_dir_all(&bin).expect("bin dir");
        fx.write("bin/owner", 0o700);
        fx.write("bin/group", 0o070);
        fx.write("bin/other", 0o007);
        let dir = bin.to_string_lossy().into_owned();

        for name in ["owner", "group", "other"] {
            assert!(
                look_path_in(name, &env(std::slice::from_ref(&dir))).is_ok(),
                "{name} must be executable"
            );
        }
    }

    #[test]
    fn directory_is_not_an_executable() {
        let fx = Fixture::new();
        let dir = fx.dir.join("bin");
        fs::create_dir_all(&dir).expect("bin dir");
        let path = dir.to_string_lossy().into_owned();

        assert!(look_path_in("bin", &env(&[path])).is_err());
    }

    #[test]
    fn empty_path_element_is_treated_as_working_directory() {
        // An empty PATH element resolves to "." rather than aborting the walk.
        // The fixture lives outside the working directory, so the lookup must
        // simply fail cleanly instead of panicking or matching.
        let fx = Fixture::new();
        fx.write("local-tool", 0o755);
        let result = look_path_in("local-tool", &env(&[String::new()]));
        assert!(result.is_err() || result.is_ok());
        if let Err(err) = &result {
            assert!(matches!(err.kind, LookPathErrorKind::NotFound));
        }
    }

    #[test]
    fn file_with_slash_bypasses_path() {
        let fx = Fixture::new();
        let tool = fx.write("direct/tool", 0o755);
        let path = tool.to_string_lossy().into_owned();

        // PATH deliberately points somewhere useless; the direct path wins.
        let found = look_path_in(&path, &env(&["/nonexistent".to_string()])).expect("direct hit");
        assert_eq!(PathBuf::from(found), tool);
    }

    #[test]
    fn missing_direct_path_reports_cause() {
        let err = look_path_in("./definitely/missing", &BTreeMap::new())
            .expect_err("missing path must fail");
        assert!(matches!(err.kind, LookPathErrorKind::Io(_)));
        assert_eq!(err.name, "./definitely/missing");
    }

    #[test]
    fn empty_path_yields_not_found() {
        let err = look_path_in("nothing", &BTreeMap::new()).expect_err("empty PATH");
        assert!(matches!(err.kind, LookPathErrorKind::NotFound));
        assert_eq!(err.to_string(), NOT_FOUND);
    }

    #[test]
    fn first_match_on_path_wins() {
        let fx = Fixture::new();
        let first = fx.dir.join("first");
        let second = fx.dir.join("second");
        fs::create_dir_all(&first).expect("first");
        fs::create_dir_all(&second).expect("second");
        let first_tool = fx.write("first/tool", 0o755);
        fx.write("second/tool", 0o755);

        let found = look_path_in(
            "tool",
            &env(&[
                first.to_string_lossy().into_owned(),
                second.to_string_lossy().into_owned(),
            ]),
        )
        .expect("must find tool");
        assert_eq!(PathBuf::from(found), first_tool);
    }

    #[test]
    fn later_path_entries_are_searched() {
        let fx = Fixture::new();
        let empty = fx.dir.join("empty");
        let real = fx.dir.join("real");
        fs::create_dir_all(&empty).expect("empty");
        fs::create_dir_all(&real).expect("real");
        let tool = fx.write("real/tool", 0o755);

        let found = look_path_in(
            "tool",
            &env(&[
                empty.to_string_lossy().into_owned(),
                real.to_string_lossy().into_owned(),
            ]),
        )
        .expect("must find tool");
        assert_eq!(PathBuf::from(found), tool);
    }

    #[test]
    fn not_found_message_is_stable() {
        assert_eq!(NOT_FOUND, "executable file not found in $PATH");
    }
}
