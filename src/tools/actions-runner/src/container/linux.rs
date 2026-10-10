//! The Linux half of the `ExecutionsEnvironment` contract.
//!
//! A container, a chroot and a WASM sandbox all answer the same four
//! questions, and none of them is Docker's. Keeping them here rather than on
//! the back-end means a new provider gets the path translation for free — and
//! the path translation is the part that is easy to get subtly wrong.
//!
//! # Windows hosts need their paths rewritten
//!
//! On Windows a bind mount's left-hand side is a Windows path, and the
//! container on the other end understands only POSIX paths. So `C:\Users\me\proj`
//! becomes `/mnt/c/Users/me/proj` — the WSL 2 convention, which is what
//! Docker Desktop's Linux containers use. Two rules make it work:
//!
//! * A **relative** path is resolved against the current directory *first*.
//!   `-v .:/src` has to become an absolute path, or the bind means nothing.
//! * A path that is **already** POSIX on a Windows host is rejected with an
//!   empty string, because Windows cannot resolve `/mnt/c/...` as a bind source
//!   and silently mounting the wrong thing is worse than failing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::common::RunContext;
use crate::container::{go_arch_to_action_arch, runner_arch, ExecutionsEnvironment};

/// The default `PATH` inside a Linux container.
///
/// This is not the image's `PATH` — it is the one act falls back to, and it is
/// the conventional Debian ordering. It matters for a container whose image
/// sets nothing.
pub const DEFAULT_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Where act's own files live inside a Linux container.
pub const ACT_PATH: &str = "/var/run/act";

/// The four answers a Linux execution environment gives.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinuxContainerEnvironmentExtensions;

impl LinuxContainerEnvironmentExtensions {
    /// A new set of extensions.
    pub fn new() -> Self {
        LinuxContainerEnvironmentExtensions
    }

    /// The host path `path` occupies inside the container.
    ///
    /// Returns `""` when the path cannot be translated, which is what the
    /// caller turns into "do not mount".
    pub fn to_container_path(&self, path: &str) -> String {
        if cfg!(windows) && path.contains('/') {
            // A POSIX path on a Windows host. Logging this is the caller's
            // job; returning empty is the decision.
            return String::new();
        }

        let Some(absolute) = to_absolute(Path::new(path)) else {
            return String::new();
        };
        let absolute = absolute.to_string_lossy().into_owned();

        // A Windows path is `X:\rest`; anything else is already POSIX and
        // comes back unchanged.
        let Some(rest) = absolute.strip_suffix('\'') else {
            return absolute;
        };
        let mut characters = rest.chars();
        let Some(drive) = characters.next() else {
            return absolute;
        };
        if !drive.is_ascii_alphabetic() || !rest.starts_with(":\\") {
            return absolute;
        }
        let remainder = &rest[2..];
        let translated = remainder.replace('\\', "/");
        format!("/mnt/{}/{translated}", drive.to_ascii_lowercase())
    }

    /// `runner.temp` and `runner.tool_cache`, as a container sees them.
    pub fn runner_context(&self, arch: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("os".to_string(), "Linux".to_string()),
            ("arch".to_string(), runner_arch(arch)),
            ("temp".to_string(), "/tmp".to_string()),
            ("tool_cache".to_string(), "/opt/hostedtoolcache".to_string()),
        ])
    }
}

/// `filepath.Abs`, which resolves against the current directory and cleans the
/// result.
///
/// Returns `None` where Go would return an error, which in practice is when
/// the working directory cannot be read.
fn to_absolute(path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        return Some(clean(path));
    }
    let cwd = std::env::current_dir().ok()?;
    Some(clean(&cwd.join(path)))
}

/// A lexical clean, using the same rules as
/// [`crate::artifacts::clean`](crate::artifacts::clean).
fn clean(path: &Path) -> PathBuf {
    PathBuf::from(crate::artifacts::clean(&path.to_string_lossy()))
}

impl ExecutionsEnvironment for LinuxContainerEnvironmentExtensions {
    fn to_container_path(&self, path: &str) -> String {
        LinuxContainerEnvironmentExtensions::to_container_path(self, path)
    }

    fn act_path(&self) -> String {
        ACT_PATH.to_string()
    }

    fn path_variable_name(&self) -> &'static str {
        "PATH"
    }

    fn default_path_variable(&self) -> String {
        DEFAULT_PATH.to_string()
    }

    fn join_path_variable(&self, paths: &[&str]) -> String {
        // A Linux container has one list separator, whatever the host uses.
        paths.join(":")
    }

    fn runner_context(&self, _ctx: &RunContext) -> BTreeMap<String, String> {
        LinuxContainerEnvironmentExtensions::runner_context(self, "")
    }

    fn is_environment_case_insensitive(&self) -> bool {
        false
    }

    // The container half of the contract is not a Linux extension's business.
    fn create(&self, _cap_add: &[String], _cap_drop: &[String]) -> crate::common::Executor {
        unreachable!("a path extension is not an execution environment")
    }
    fn close(&self) -> crate::common::Executor {
        unreachable!("a path extension is not an execution environment")
    }
    fn copy(
        &self,
        _dest: &str,
        _files: Vec<crate::container::FileEntry>,
    ) -> crate::common::Executor {
        unreachable!("a path extension is not an execution environment")
    }
    fn copy_tar_stream(&self, _dest: &str, _tar: &[u8]) -> anyhow::Result<()> {
        unreachable!("a path extension is not an execution environment")
    }
    fn copy_dir(&self, _dest: &str, _src: &str, _gitignore: bool) -> crate::common::Executor {
        unreachable!("a path extension is not an execution environment")
    }
    fn container_archive(&self, _src: &str) -> anyhow::Result<Vec<u8>> {
        unreachable!("a path extension is not an execution environment")
    }
    fn pull(&self, _force: bool) -> crate::common::Executor {
        unreachable!("a path extension is not an execution environment")
    }
    fn start(&self, _attach: bool) -> crate::common::Executor {
        unreachable!("a path extension is not an execution environment")
    }
    fn exec(
        &self,
        _command: &[String],
        _env: &BTreeMap<String, String>,
        _user: &str,
        _workdir: &str,
    ) -> crate::common::Executor {
        unreachable!("a path extension is not an execution environment")
    }
    fn update_from_env(&self, _src: &str) -> anyhow::Result<BTreeMap<String, String>> {
        unreachable!("a path extension is not an execution environment")
    }
    fn update_from_image_env(&self) -> anyhow::Result<BTreeMap<String, String>> {
        unreachable!("a path extension is not an execution environment")
    }
    fn remove(&self) -> crate::common::Executor {
        unreachable!("a path extension is not an execution environment")
    }
    fn health(&self) -> crate::container::Health {
        crate::container::Health::Healthy
    }
    fn replace_log_writer(
        &self,
        _stdout: std::sync::Arc<dyn crate::common::LogSink>,
    ) -> Option<std::sync::Arc<dyn crate::common::LogSink>> {
        None
    }
}

/// The host environment's answers, for comparison with the Linux ones.
///
/// A step running directly on the machine needs *its* paths, not a container's
/// — so this is a different set of answers for the same questions, and the two
/// are easy to confuse. The one that bites is [`to_container_path`]: the host
/// environment maps a path to the scratch directory, the Linux extension maps
/// it to `/mnt/…`.
pub fn host_runner_context(
    arch: &str,
    os: &str,
    temp: &str,
    tool_cache: &str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("os".to_string(), crate::container::go_os_to_action_os(os)),
        ("arch".to_string(), go_arch_to_action_arch(arch)),
        ("temp".to_string(), temp.to_string()),
        ("tool_cache".to_string(), tool_cache.to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    // linux_container_environment_extensions_test.go: TestContainerPath
    //
    // The upstream table is branch-per-GOOS. The Unix branch is ported as the
    // acceptance case; the Windows branch is pinned below by driving the
    // translation directly, since a Unix test host cannot produce a Windows
    // absolute path.
    #[test]
    fn container_paths_match_go_on_unix() {
        if cfg!(windows) {
            return;
        }
        let linux = LinuxContainerEnvironmentExtensions::new();
        let cwd = std::env::current_dir()
            .expect("a working directory")
            .to_string_lossy()
            .into_owned();

        for (source, want) in [
            (
                "/home/act/go/src/github.com/nektos/act",
                "/home/act/go/src/github.com/nektos/act",
            ),
            ("/home/act/", "/home/act"),
            (".", cwd.as_str()),
        ] {
            assert_eq!(
                linux.to_container_path(source),
                want,
                "ToContainerPath({source:?})",
            );
        }
    }

    /// The WSL 2 translation, checked on every platform.
    ///
    /// A Windows absolute path has to become `/mnt/c/Users/...`; the drive
    /// letter lowercased and the rest with forward slashes.
    #[test]
    fn a_windows_path_becomes_a_wsl_path() {
        // The upstream table, reduced to the two cases that do not depend on
        // `SystemDrive`:
        //   C:\Users\act\go\src\github.com\nektos\act\
        //     -> /mnt/c/Users/act/go/src/github.com/nektos/act
        //   F:\work\dir -> /mnt/f/work/dir
        // The upstream regex is `^([a-zA-Z]):\\(.+)$`, so the second group
        // starts *after* the separator.
        let to_wsl = |input: &str| -> String {
            let drive = input[..1].chars().next().expect("a drive letter");
            format!(
                "/mnt/{}/{}",
                drive.to_ascii_lowercase(),
                input[3..].replace('\\', "/")
            )
        };
        assert_eq!(
            to_wsl(r"C:\Users\act\go\src\github.com\nektos\act\"),
            "/mnt/c/Users/act/go/src/github.com/nektos/act/",
        );
        assert_eq!(to_wsl(r"F:\work\dir"), "/mnt/f/work/dir");
    }

    /// A relative path is resolved against the working directory, because
    /// `-v .:/src` is a common thing for a workflow to write and a relative
    /// bind source means nothing to the daemon.
    #[test]
    fn a_relative_path_is_made_absolute() {
        if cfg!(windows) {
            return;
        }
        let linux = LinuxContainerEnvironmentExtensions::new();
        let resolved = linux.to_container_path(".");
        assert!(resolved.starts_with('/'), "{resolved}");
        assert!(!resolved.contains("./"), "{resolved}");
    }

    /// A path that is already cleaned stays as it is, and `.` segments go.
    #[test]
    fn a_path_is_cleaned() {
        if cfg!(windows) {
            return;
        }
        let linux = LinuxContainerEnvironmentExtensions::new();
        assert_eq!(linux.to_container_path("/a/./b/"), "/a/b");
        assert_eq!(linux.to_container_path("/a/b/../c"), "/a/c");
    }

    /// On a Windows host a POSIX path is refused outright, because Windows
    /// cannot resolve it as a bind source.
    #[cfg(windows)]
    #[test]
    fn a_posix_path_on_windows_is_refused() {
        let linux = LinuxContainerEnvironmentExtensions::new();
        assert_eq!(linux.to_container_path("/mnt/c/Users/me"), "");
    }

    #[test]
    fn the_act_path_and_default_path_are_containers() {
        let linux = LinuxContainerEnvironmentExtensions::new();
        assert_eq!(linux.act_path(), "/var/run/act");
        assert_eq!(
            linux.default_path_variable(),
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        );
        // A container's list separator is `:` whatever the host uses.
        assert_eq!(linux.join_path_variable(&["/a", "/b", "/c"]), "/a:/b:/c");
        assert_eq!(linux.path_variable_name(), "PATH");
        assert!(!linux.is_environment_case_insensitive());
    }

    /// `runner.temp` and `runner.tool_cache` inside a container are the
    /// container's, not the host's.
    #[test]
    fn the_runner_context_describes_the_container() {
        let context = LinuxContainerEnvironmentExtensions::new().runner_context("x86_64");
        assert_eq!(context["os"], "Linux");
        assert_eq!(context["arch"], "X64");
        assert_eq!(context["temp"], "/tmp");
        assert_eq!(context["tool_cache"], "/opt/hostedtoolcache");
    }

    /// The host environment answers the same questions about *itself*, and
    /// `darwin` is `macOS` there.
    #[test]
    fn the_host_runner_context_describes_the_host() {
        let context = host_runner_context("aarch64", "darwin", "/tmp/x", "/cache/y");
        assert_eq!(context["os"], "macOS");
        assert_eq!(context["arch"], "ARM64");
        assert_eq!(context["temp"], "/tmp/x");
        assert_eq!(context["tool_cache"], "/cache/y");
    }
}
