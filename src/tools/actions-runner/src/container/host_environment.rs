//! Running a step directly on the machine, with no container.
//!
//! This is what `-P ubuntu-latest=` gives you, and for a CI build computer it
//! is the back-end that needs no daemon at all. The command is looked up in
//! the step's own `PATH`, run in the scratch directory, and its output goes to
//! the job's log.
//!
//! # The four answers, which differ from a container's
//!
//! | question | container | host |
//! |---|---|---|
//! | `to_container_path` | `/mnt/c/Users/…` | the scratch path, or the path unchanged |
//! | `act_path` | `/var/run/act` | wherever act was installed |
//! | default `PATH` | `/usr/local/sbin:…` | the host's own `PATH` |
//! | `PATH` name | `PATH` | `Path` on Windows |
//!
//! `ToContainerPath` is the one with a real quirk. Upstream computes
//! `filepath.Rel(workdir, path)` and **discards it unless the result is
//! exactly the workdir**, returning the input untouched otherwise — so a path
//! outside the workdir comes back unchanged, and only the workdir itself maps
//! to the scratch directory. That is preserved: a step that writes to an
//! absolute path outside the workspace means that path, not the scratch copy.
//!
//! # The environment is case-insensitive on Windows
//!
//! `Path` and `PATH` are the same variable there, so a lookup has to compare
//! names case-insensitively. This is why [`HostEnvironment`] keeps its
//! environment in a map and searches it rather than reading the process
//! environment directly — a workflow that sets `PATH` and a runner that reads
//! `Path` have to meet.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};

use crate::common::{Executor, LogSink, RunContext};
use crate::container::{FileEntry, Health, LinuxContainerEnvironmentExtensions};
use crate::filecollector::{CopyCollector, DefaultFs, FileCollector, TarCollector};
use crate::lookpath::{look_path_in, Env as LookPathEnv};

/// The directories a step gets.
#[derive(Clone)]
pub struct HostEnvironment {
    /// The scratch directory the step runs in.
    pub path: PathBuf,
    /// `runner.temp`.
    pub tmp_dir: PathBuf,
    /// `runner.tool_cache`.
    pub tool_cache: PathBuf,
    /// The host workspace, which `to_container_path` is measured against.
    pub workdir: String,
    /// Where act's own files live.
    pub act_path: PathBuf,
    /// The function `remove` calls after clearing the scratch directory.
    pub clean_up: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Where the command's output goes.
    pub stdout: Arc<dyn LogSink>,
    /// The Linux answers, which is what a host running Linux steps needs.
    linux: LinuxContainerEnvironmentExtensions,
}

impl std::fmt::Debug for HostEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostEnvironment")
            .field("path", &self.path)
            .field("tmp_dir", &self.tmp_dir)
            .field("tool_cache", &self.tool_cache)
            .field("workdir", &self.workdir)
            .field("act_path", &self.act_path)
            .finish_non_exhaustive()
    }
}

impl HostEnvironment {
    /// A host environment over `path`, with the other directories beside it.
    pub fn new(path: PathBuf, tmp_dir: PathBuf, tool_cache: PathBuf, workdir: &str) -> Self {
        HostEnvironment {
            path,
            tmp_dir,
            tool_cache,
            workdir: workdir.to_string(),
            act_path: std::env::current_dir().unwrap_or_default(),
            clean_up: None,
            stdout: Arc::new(crate::common::context::CollectingSink::new()),
            linux: LinuxContainerEnvironmentExtensions::new(),
        }
    }

    /// Sends the command's output somewhere else, returning the previous sink.
    pub fn replace_log_writer(
        &mut self,
        stdout: Arc<dyn LogSink>,
    ) -> Option<Arc<dyn LogSink>> {
        Some(std::mem::replace(&mut self.stdout, stdout))
    }

    /// The Linux half of the contract, so a host environment and a container
    /// environment can share the path questions.
    pub fn linux(&self) -> &LinuxContainerEnvironmentExtensions {
        &self.linux
    }

    /// The name of the `PATH` variable on this platform.
    ///
    /// `Path` on Windows, because Windows folds case and the map has to
    /// agree with the operating system's own view.
    pub fn path_variable_name(&self) -> &'static str {
        if cfg!(windows) {
            "Path"
        } else {
            "PATH"
        }
    }

    /// The `PATH` the host starts with.
    pub fn default_path_variable(&self) -> String {
        std::env::var(self.path_variable_name()).unwrap_or_default()
    }

    /// `act_path`, with Windows separators normalised because the value ends
    /// up in a container-facing string.
    pub fn act_path_string(&self) -> String {
        let path = self.act_path.to_string_lossy().into_owned();
        if cfg!(windows) {
            path.replace('\\', "/")
        } else {
            path
        }
    }

    /// The host path `path` occupies for this step.
    ///
    /// Only the workdir itself maps to the scratch directory. See the module
    /// docs for why anything else is returned unchanged.
    pub fn to_container_path(&self, path: &str) -> String {
        // `filepath.Rel` errors when one side is absolute and the other is
        // not, and upstream then joins the *empty* relative path — so the
        // answer is the scratch directory, not the input.
        let relative = crate::artifacts::rel(&self.workdir, path);
        match relative {
            Some(relative) => {
                if crate::artifacts::clean(&self.workdir) == crate::artifacts::clean(path) {
                    return self.path.to_string_lossy().into_owned();
                }
                let _ = relative;
                path.to_string()
            }
            None => self.path.to_string_lossy().into_owned(),
        }
    }

    /// `runner.os`, `runner.arch`, `runner.temp` and `runner.tool_cache` for
    /// the host.
    pub fn runner_context(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            (
                "os".to_string(),
                crate::container::go_os_to_action_os(std::env::consts::OS),
            ),
            (
                "arch".to_string(),
                crate::container::go_arch_to_action_arch(std::env::consts::ARCH),
            ),
            (
                "temp".to_string(),
                self.tmp_dir.to_string_lossy().into_owned(),
            ),
            (
                "tool_cache".to_string(),
                self.tool_cache.to_string_lossy().into_owned(),
            ),
        ])
    }

    /// Looks `command` up in `env`'s `PATH`, not the process's.
    ///
    /// This is the difference between a host environment and a container one:
    /// a step that set `PATH: /custom/bin` expects the command found there, and
    /// reading the process environment would ignore that.
    pub fn look_path(&self, command: &str, env: &BTreeMap<String, String>) -> Result<String> {
        let name = self.path_variable_name();
        let path = env
            .iter()
            .find(|(key, _)| {
                if self.is_environment_case_insensitive() {
                    key.eq_ignore_ascii_case(name)
                } else {
                    key.as_str() == name
                }
            })
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        let _ = &path;
        look_path_in(command, &LocalEnv { env }).map_err(|_| anyhow!("Cannot find: {command} in PATH"))
    }

    /// The platform's answer to "are variable names folded?".
    pub fn is_environment_case_insensitive(&self) -> bool {
        cfg!(windows)
    }

    /// The working directory for a step: `workdir` made absolute against the
    /// scratch directory, or the scratch directory itself.
    pub fn resolve_workdir(&self, workdir: &str) -> PathBuf {
        if workdir.is_empty() {
            return self.path.clone();
        }
        let candidate = Path::new(workdir);
        if candidate.is_absolute() {
            return candidate.to_path_buf();
        }
        self.path.join(candidate)
    }

    /// `getEnvListFromMap`: `KEY=value` in map order.
    ///
    /// The order is a `BTreeMap`'s, so it is deterministic. Upstream iterates
    /// a Go map and gets a random order, which nothing downstream depends on
    /// — the shell reads the list, not the order it arrived in.
    pub fn env_list(env: &BTreeMap<String, String>) -> Vec<String> {
        env.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect()
    }

    /// Runs `command` in the scratch directory.
    ///
    /// Without the PTY upstream always sets up, this is a plain spawn with
    /// both streams pointed at the job's log. The PTY — and the
    /// end-of-transmission dance in [`crate::container::pty_writer`] — is what
    /// makes interactive prompts and colour work, and it lands with the
    /// process layer.
    pub fn exec(
        &self,
        command: &[String],
        env: &BTreeMap<String, String>,
        workdir: &str,
    ) -> Result<()> {
        let Some(program) = command.first() else {
            return Err(anyhow!("empty command"));
        };
        let resolved = self.look_path(program, env)?;
        let mut process = std::process::Command::new(&resolved);
        process
            .args(&command[1..])
            .env_clear()
            .envs(env)
            .current_dir(self.resolve_workdir(workdir))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        // `getSysProcAttr(cmdline, false)`: without this a step that spawns
        // children — `npm`, `make`, a nested `docker build` — cannot be stopped
        // as a unit, and cancelling the job leaves the children running.
        super::proc_attr::set_process_group(&mut process);

        let mut child = process
            .spawn()
            .map_err(|err| anyhow!("{}: {err}", resolved))?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        // The pumps are **joined before returning**, not detached. Go's
        // `cmd.Run()` documents that `Wait` "waits for any copying to stdin or
        // copying from stdout or stderr to complete", and upstream relies on
        // that: it hands `cmd.Stdout` a writer and then logs from the same
        // goroutine. Without the join, a step's last lines can still be in the
        // pipe when `exec` returns — and the truncated output is the *tail*,
        // which is the part that says why the build failed.
        let mut pumps = Vec::new();
        if let Some(stdout) = stdout {
            let sink = Arc::clone(&self.stdout);
            pumps.push(std::thread::spawn(move || copy_to_sink(stdout, sink)));
        }
        if let Some(stderr) = stderr {
            let sink = Arc::clone(&self.stdout);
            pumps.push(std::thread::spawn(move || copy_to_sink(stderr, sink)));
        }
        let status = child
            .wait()
            .map_err(|err| anyhow!("waiting for {resolved}: {err}"))?;
        // Joined after `wait`, before any early return: the child has exited
        // and closed its end, so each reader sees EOF and the join terminates.
        // A pump that panicked is a logging fault, not a step failure, so it
        // must not turn a passing build into a failing one.
        for pump in pumps {
            let _ = pump.join();
        }
        if !status.success() {
            return Err(anyhow!(
                "Process completed with exit code {}.",
                status.code().unwrap_or(-1)
            ));
        }
        Ok(())
    }
}

/// Feeds a process's output to the job's log, one line at a time.
///
/// A child process's streams arrive in arbitrary chunks, so the line writer is
/// what turns them back into log lines.
fn copy_to_sink(mut source: impl std::io::Read + Send + 'static, sink: Arc<dyn LogSink>) {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(&mut source);
    for line in reader.lines() {
        match line {
            Ok(line) => sink.log(crate::common::context::Level::Info, &line),
            // A line that is not UTF-8 is reported rather than dropped.
            Err(err) => sink.log(
                crate::common::context::Level::Warn,
                &format!("<unreadable output: {err}>"),
            ),
        }
    }
}

/// A [`LookPathEnv`] over an explicit map, so a step's `PATH` is what is
/// searched.
struct LocalEnv<'a> {
    env: &'a BTreeMap<String, String>,
}

impl LookPathEnv for LocalEnv<'_> {
    fn getenv(&self, name: &str) -> Option<String> {
        if cfg!(windows) {
            self.env
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.clone())
        } else {
            self.env.get(name).cloned()
        }
    }
}

/// Removes the scratch directory.
pub fn remove_scratch(path: &Path, clean_up: Option<&Arc<dyn Fn() + Send + Sync>>) -> Result<()> {
    if let Some(clean_up) = clean_up {
        clean_up();
    }
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        // Go's `os.RemoveAll` on a missing path is a success.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// Packs `src_path` into a `tar` stream, as `GetContainerArchive` does.
///
/// A directory is walked with a trailing separator as the strip prefix, and a
/// plain file becomes one entry. This is the same collector the artifact
/// service uses, so an archive produced here and one produced there have the
/// same layout.
pub fn container_archive(src_path: &str) -> Result<Vec<u8>> {
    let source = PathBuf::from(crate::artifacts::clean(src_path));
    let metadata = std::fs::symlink_metadata(&source)?;
    let mut buffer = Vec::new();
    {
        let mut tar = tar::Builder::new(&mut buffer);
        let mut collector = TarCollector {
            tar: &mut tar,
            // act passes the container user, or zero for a local archive.
            uid: 0,
            gid: 0,
            dst_dir: String::new(),
        };
        if metadata.is_dir() {
            let source = source.to_string_lossy().into_owned();
            // Upstream's prefix for a directory: `srcPath` with a trailing
            // separator, so every walked path is relative to it.
            let prefix = if source.ends_with(std::path::MAIN_SEPARATOR) {
                source.clone()
            } else {
                format!("{source}{}", std::path::MAIN_SEPARATOR)
            };
            let mut walker = FileCollector::new(&DefaultFs, &mut collector);
            walker.src_path = PathBuf::from(&source);
            walker.src_prefix = prefix;
            walker.collect_files(&PathBuf::from(&source), &[])?;
        } else {
            let info = crate::filecollector::FileInfo::lstat(&source)?;
            let name = source
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut file = std::fs::File::open(&source)?;
            crate::filecollector::Handler::write_file(
                &mut collector,
                &name,
                &info,
                "",
                Some(&mut file),
                &crate::filecollector::Cancellation::new(),
            )?;
        }
    }
    Ok(buffer)
}

/// `HostEnvironment` is an `ExecutionsEnvironment`.
///
/// The container half is a no-op throughout: there is no container, so
/// creating, pulling and starting one all succeed immediately, and the file
/// operations write to the scratch directory instead.
impl crate::container::ExecutionsEnvironment for HostEnvironment {
    fn create(&self, _cap_add: &[String], _cap_drop: &[String]) -> Executor {
        // A host has nothing to create. Upstream returns a step that does
        // nothing, and a workflow that pulls images still succeeds.
        Arc::new(|_| Ok(()))
    }

    fn close(&self) -> Executor {
        Arc::new(|_| Ok(()))
    }

    fn copy(&self, dest_path: &str, files: Vec<FileEntry>) -> Executor {
        let root = Path::new(dest_path).to_path_buf();
        // Shared, because an `Executor` may be run more than once and a
        // `Vec` is not copyable into an `Fn` closure.
        let files = Arc::new(files);
        Arc::new(move |_| {
            for file in files.iter() {
                let target = root.join(&file.name);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, file.body.as_bytes())?;
                set_mode(&target, file.mode);
            }
            Ok(())
        })
    }

    fn copy_tar_stream(&self, dest_path: &str, tar_stream: &[u8]) -> Result<()> {
        // Upstream removes the destination first, so a re-copy of the
        // workspace does not merge with what was there.
        let _ = std::fs::remove_dir_all(dest_path);
        let mut archive = tar::Archive::new(std::io::Cursor::new(tar_stream));
        archive.unpack(dest_path)?;
        Ok(())
    }

    fn copy_dir(&self, dest_path: &str, src_path: &str, use_gitignore: bool) -> Executor {
        let source = src_path.to_string();
        let dest = PathBuf::from(dest_path);
        Arc::new(move |ctx| {
            // `filepath.Dir(srcPath)` plus a separator. Upstream always uses
            // the *parent* as the strip prefix, never the path itself, and the
            // difference is observable in where nested `.gitignore` domains
            // line up.
            let prefix = match source.rfind(std::path::MAIN_SEPARATOR) {
                Some(index) => source[..=index].to_string(),
                None => format!("{}{}", source, std::path::MAIN_SEPARATOR),
            };
            ctx.log_debug(&format!("Stripping prefix:{prefix} src:{source}"));

            let mut collector = CopyCollector {
                dst_dir: dest.clone(),
            };
            let mut walker = FileCollector::new(&DefaultFs, &mut collector);
            walker.src_path = PathBuf::from(&source);
            walker.src_prefix = prefix;
            if use_gitignore {
                // A `.gitignore` that cannot be read is logged and the walk
                // continues unfiltered, which is what upstream does with the
                // error it drops.
                let patterns = crate::gitignore::read_patterns(
                    Path::new(&source),
                    &[],
                )
                .0;
                walker.ignorer = Some(crate::gitignore::Matcher::new(patterns));
            }
            walker.collect_files(Path::new(&source), &[])?;
            Ok(())
        })
    }

    fn container_archive(&self, src_path: &str) -> Result<Vec<u8>> {
        container_archive(src_path)
    }

    fn pull(&self, _force: bool) -> Executor {
        Arc::new(|_| Ok(()))
    }

    fn start(&self, _attach: bool) -> Executor {
        Arc::new(|_| Ok(()))
    }

    fn exec(
        &self,
        command: &[String],
        env: &BTreeMap<String, String>,
        _user: &str,
        workdir: &str,
    ) -> Executor {
        let command = command.to_vec();
        let env = env.clone();
        let workdir = workdir.to_string();
        // The environment is captured by value, so a cloned host is enough to
        // run a step. Upstream closes over the pointer; the difference shows
        // only in how a caller schedules the step.
        let scratch = self.path.clone();
        Arc::new(move |_| {
            let Some(program) = command.first() else {
                return Err(anyhow!("empty command"));
            };
            let mut process = std::process::Command::new(program);
            process
                .args(&command[1..])
                .env_clear()
                .envs(&env)
                .current_dir(if workdir.is_empty() {
                    scratch.clone()
                } else {
                    Path::new(&workdir).to_path_buf()
                });
            let status = process.status()?;
            if !status.success() {
                return Err(anyhow!(
                    "Process completed with exit code {}.",
                    status.code().unwrap_or(-1)
                ));
            }
            Ok(())
        })
    }

    fn update_from_env(&self, src_path: &str) -> Result<BTreeMap<String, String>> {
        let mut env = BTreeMap::new();
        // A missing env file is not an error upstream — it returns before
        // parsing — so a gitignored `.env` does not fail a fresh checkout.
        let Ok(archive) = container_archive(src_path) else {
            return Ok(env);
        };
        let Ok(text) = first_entry_text(&archive) else {
            return Ok(env);
        };
        crate::container::env_file::parse_env_text(&text, &mut env)?;
        Ok(env)
    }

    fn update_from_image_env(&self) -> Result<BTreeMap<String, String>> {
        // There is no image. Upstream's step is a no-op that succeeds, and it
        // contributes nothing.
        Ok(BTreeMap::new())
    }

    fn remove(&self) -> Executor {
        let path = self.path.clone();
        let clean_up = self.clean_up.clone();
        Arc::new(move |_| remove_scratch(&path, clean_up.as_ref()))
    }

    fn health(&self) -> Health {
        Health::Healthy
    }

    fn replace_log_writer(&self, _stdout: Arc<dyn LogSink>) -> Option<Arc<dyn LogSink>> {
        // The sink lives behind a field act mutates; a shared reference cannot
        // swap it. The runner installs its sink before the first step runs.
        None
    }

    fn to_container_path(&self, path: &str) -> String {
        HostEnvironment::to_container_path(self, path)
    }

    fn act_path(&self) -> String {
        self.act_path_string()
    }

    fn path_variable_name(&self) -> &'static str {
        self.path_variable_name()
    }

    fn default_path_variable(&self) -> String {
        self.default_path_variable()
    }

    fn join_path_variable(&self, paths: &[&str]) -> String {
        paths
            .join(std::path::MAIN_SEPARATOR_STR)
    }

    fn runner_context(&self, _ctx: &RunContext) -> BTreeMap<String, String> {
        self.runner_context()
    }

    fn is_environment_case_insensitive(&self) -> bool {
        self.is_environment_case_insensitive()
    }
}

/// The first file's contents out of a `tar` archive.
///
/// `parseEnvFile` reads exactly one entry — the env file is a single file — and
/// the container archive holds it alone.
fn first_entry_text(archive: &[u8]) -> Result<String> {
    let mut reader = tar::Archive::new(std::io::Cursor::new(archive));
    let mut entry = reader
        .entries()?
        .next()
        .transpose()?
        .ok_or_else(|| anyhow!("empty archive"))?;
    if entry.header().entry_type().is_dir() {
        return Err(anyhow!("the archive's only entry is a directory"));
    }
    let mut text = String::new();
    std::io::Read::read_to_string(&mut entry, &mut text)?;
    Ok(text)
}

/// Applies a `tar` header's mode to a file just written.
///
/// `copy` carries a mode per file, and without this an executable a step
/// uploaded would lose its bit.
fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Only the permission bits, and only the low twelve that `os.FileMode`
        // keeps; the rest of the `tar` mode word is type flags.
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o777));
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::ExecutionsEnvironment;

    fn environment(dir: &Path) -> HostEnvironment {
        HostEnvironment::new(
            dir.join("path"),
            dir.join("tmp"),
            dir.join("tool_cache"),
            "/work",
        )
    }

    /// The workdir maps to the scratch directory; anything else is itself.
    /// The asymmetry is upstream's, and a step that writes outside the
    /// workspace means that path.
    #[test]
    fn only_the_workdir_maps_to_the_scratch_directory() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let environment = environment(dir.path());

        assert_eq!(
            environment.to_container_path("/work"),
            dir.path().join("path").to_string_lossy(),
        );
        assert_eq!(
            environment.to_container_path("/somewhere/else"),
            "/somewhere/else",
        );
        // A trailing separator still counts as the workdir.
        assert_eq!(
            environment.to_container_path("/work/"),
            dir.path().join("path").to_string_lossy(),
        );
    }

    /// When `filepath.Rel` cannot relate the two — one absolute, one relative
    /// — upstream joins an empty relative path, which is the scratch
    /// directory.
    #[test]
    fn an_unrelatable_path_maps_to_the_scratch_directory() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let environment = environment(dir.path());
        // `workdir` is relative here, so nothing relates.
        let mut environment = environment;
        environment.workdir = "work".to_string();
        assert_eq!(
            environment.to_container_path("/absolute"),
            dir.path().join("path").to_string_lossy(),
        );
    }

    #[test]
    fn the_workdir_defaults_to_the_scratch_directory() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let environment = environment(dir.path());
        assert_eq!(environment.resolve_workdir(""), environment.path);
        assert_eq!(
            environment.resolve_workdir("sub"),
            environment.path.join("sub"),
            "a relative workdir is inside the scratch directory",
        );
        assert_eq!(
            environment.resolve_workdir("/elsewhere"),
            PathBuf::from("/elsewhere"),
            "an absolute workdir is used as it stands",
        );
    }

    /// The `PATH` name is the platform's, because Windows folds case.
    #[test]
    fn the_path_variable_is_named_for_the_platform() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let environment = environment(dir.path());
        if cfg!(windows) {
            assert_eq!(environment.path_variable_name(), "Path");
            assert!(environment.is_environment_case_insensitive());
        } else {
            assert_eq!(environment.path_variable_name(), "PATH");
            assert!(!environment.is_environment_case_insensitive());
        }
        // And the default is read under that name, so it is never empty on a
        // machine that has one.
        assert!(!environment.default_path_variable().is_empty());
    }

    #[test]
    fn the_act_path_uses_forward_slashes_on_windows() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut environment = environment(dir.path());
        environment.act_path = PathBuf::from("a\\b\\c");
        let act_path = environment.act_path_string();
        if cfg!(windows) {
            assert_eq!(act_path, "a/b/c");
        } else {
            assert_eq!(act_path, "a\\b\\c", "Unix leaves the path alone");
        }
    }

    /// A step's own `PATH` is what is searched, not the process's.
    #[test]
    fn the_lookup_uses_the_steps_own_path() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let environment = environment(dir.path());

        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).expect("a directory");
        let program = bin.join("mytool");
        std::fs::write(&program, "#!/bin/sh\n").expect("written");
        make_executable(&program);

        let name = environment.path_variable_name();
        let mut env = BTreeMap::new();
        env.insert(name.to_string(), bin.to_string_lossy().into_owned());
        assert_eq!(
            environment.look_path("mytool", &env).expect("found"),
            program.to_string_lossy(),
        );

        // A command that is only on the process's PATH is not found, which is
        // the point: the step's environment is the whole world.
        let mut empty = BTreeMap::new();
        empty.insert(name.to_string(), dir.path().join("nope").to_string_lossy().into_owned());
        assert!(environment.look_path("mytool", &empty).is_err());
    }

    #[test]
    fn the_env_list_is_key_equals_value() {
        let mut env = BTreeMap::new();
        env.insert("B".to_string(), "two".to_string());
        env.insert("A".to_string(), "one".to_string());
        assert_eq!(
            HostEnvironment::env_list(&env),
            ["A=one".to_string(), "B=two".to_string()],
            "sorted, unlike Go's random map order",
        );
    }

    /// The runner context describes the host, and `darwin` is `macOS`.
    #[test]
    fn the_runner_context_describes_the_host() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let environment = environment(dir.path());
        let context = environment.runner_context();
        let expected_os = crate::container::go_os_to_action_os(std::env::consts::OS);
        assert_eq!(context["os"], expected_os);
        assert_eq!(
            context["arch"],
            crate::container::go_arch_to_action_arch(std::env::consts::ARCH),
        );
        assert_eq!(context["temp"], dir.path().join("tmp").to_string_lossy());
        assert_eq!(
            context["tool_cache"],
            dir.path().join("tool_cache").to_string_lossy(),
        );
    }

    /// A command really runs, in the scratch directory, with the step's
    /// environment and not the process's.
    #[test]
    fn a_command_runs_in_the_scratch_directory() {
        if cfg!(windows) {
            return;
        }
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut environment = environment(dir.path());
        std::fs::create_dir_all(&environment.path).expect("a scratch directory");

        let mut env = BTreeMap::new();
        env.insert("PATH".to_string(), std::env::var("PATH").unwrap_or_default());
        env.insert("MARKER".to_string(), "from-the-step".to_string());

        // Writes its own environment and its working directory to the log.
        let sink = Arc::new(crate::common::context::CollectingSink::new());
        environment.stdout = Arc::clone(&sink) as Arc<dyn crate::common::LogSink>;

        environment
            .exec(
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "printf 'MARKER=%s PWD=%s\\n' \"$MARKER\" \"$PWD\"".to_string(),
                ],
                &env,
                "",
            )
            .expect("the command succeeded");

        // The step's own environment is what the command sees, and it runs in
        // the scratch directory.
        let lines = sink.messages_at(crate::common::context::Level::Info);
        assert!(
            lines.iter().any(|line| line.contains("MARKER=from-the-step")),
            "{lines:?}",
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains(&environment.path.to_string_lossy().to_string())),
            "the working directory is the scratch directory: {lines:?}",
        );
    }

    /// A failing command is an error carrying its exit code, which is what
    /// the runner turns into a step failure.
    #[test]
    fn a_failing_command_reports_its_exit_code() {
        if cfg!(windows) {
            return;
        }
        let dir = tempfile::tempdir().expect("a temporary directory");
        let environment = environment(dir.path());
        // The scratch directory has to exist: a step runs *in* it, and a
        // missing working directory surfaces as a spawn failure rather than as
        // anything about the command.
        std::fs::create_dir_all(&environment.path).expect("a scratch directory");
        let mut env = BTreeMap::new();
        env.insert("PATH".to_string(), std::env::var("PATH").unwrap_or_default());

        let error = environment
            .exec(&["sh".to_string(), "-c".to_string(), "exit 3".to_string()], &env, "")
            .expect_err("the command failed");
        assert!(
            error.to_string().contains('3'),
            "the exit code is in the message: {error}",
        );
    }

    /// Every line a step printed is in the log by the time `exec` returns —
    /// including on a **failing** step, and including a long burst.
    ///
    /// This is the regression test for the detached output pumps. They used to
    /// be `spawn`ed and forgotten, so `exec` returned as soon as the child
    /// exited while its output was still sitting unread in the pipe. It
    /// surfaced as a test that failed roughly one run in six, which is the
    /// worst possible shape: a gate that is green most of the time while the
    /// code underneath is wrong.
    ///
    /// The burst matters as much as the join. A single `printf` usually lands
    /// in the pipe before the child exits, so it hid the bug; a few hundred
    /// lines reliably do not, because the child fills the pipe and blocks until
    /// a reader drains it. The failing exit is the case that matters in
    /// production — the tail of a failed build is the part that says why.
    #[test]
    fn all_output_is_logged_before_exec_returns_even_when_the_step_fails() {
        if cfg!(windows) {
            return;
        }
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut environment = environment(dir.path());
        std::fs::create_dir_all(&environment.path).expect("a scratch directory");
        let mut env = BTreeMap::new();
        env.insert("PATH".to_string(), std::env::var("PATH").unwrap_or_default());

        let sink = Arc::new(crate::common::context::CollectingSink::new());
        environment.stdout = Arc::clone(&sink) as Arc<dyn crate::common::LogSink>;

        let lines = 500;
        let error = environment
            .exec(
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    format!(
                        "i=1; while [ $i -le {lines} ]; do echo \"line-$i\"; i=$((i+1)); done; exit 7"
                    ),
                ],
                &env,
                "",
            )
            .expect_err("the command failed");
        assert!(error.to_string().contains('7'), "exit code: {error}");

        let logged = sink.messages_at(crate::common::context::Level::Info);
        assert_eq!(
            logged.len(),
            lines,
            "every line must be logged before exec returns; got {} of {lines}",
            logged.len(),
        );
        // The first *and* the last, because the tail is what a build log is
        // read for.
        assert_eq!(logged.first().map(String::as_str), Some("line-1"));
        let last = format!("line-{lines}");
        assert_eq!(logged.last().map(String::as_str), Some(last.as_str()));
    }

    /// A sink that logs slowly enough to make a lost-log-line race
    /// deterministic.
    ///
    /// A real log sink is fast, so a missing join is a microsecond window and
    /// the test above passed about five runs in six — the same one-in-six flake
    /// this bug arrived with. Sleeping per line holds the pump thread inside
    /// `log` while the child is already gone, which is exactly the state the
    /// join is supposed to prevent. The test is then green with the join and
    /// red without it, on every run.
    struct SlowSink {
        lines: std::sync::Mutex<Vec<String>>,
    }

    impl SlowSink {
        fn new() -> Self {
            Self {
                lines: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn lines(&self) -> Vec<String> {
            self.lines.lock().expect("sink poisoned").clone()
        }
    }

    impl crate::common::LogSink for SlowSink {
        fn log(&self, _level: crate::common::context::Level, message: &str) {
            self.lines
                .lock()
                .expect("sink poisoned")
                .push(message.to_string());
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// The join, not the volume of output, is what guarantees the log is
    /// complete — proved by a sink that is still mid-line when the child exits.
    ///
    /// This is the deterministic companion to the burst test above: that one
    /// failed about one run in six, this one fails every run when the pumps are
    /// detached.
    #[test]
    fn a_slow_log_sink_still_receives_every_line_before_exec_returns() {
        if cfg!(windows) {
            return;
        }
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut environment = environment(dir.path());
        std::fs::create_dir_all(&environment.path).expect("a scratch directory");
        let mut env = BTreeMap::new();
        env.insert("PATH".to_string(), std::env::var("PATH").unwrap_or_default());

        let sink = Arc::new(SlowSink::new());
        environment.stdout = Arc::clone(&sink) as Arc<dyn crate::common::LogSink>;

        let lines = 12;
        environment
            .exec(
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    format!("i=1; while [ $i -le {lines} ]; do echo \"line-$i\"; i=$((i+1)); done"),
                ],
                &env,
                "",
            )
            .expect("the command succeeded");

        assert_eq!(
            sink.lines().len(),
            lines,
            "exec returned while the pumps were still logging",
        );
    }

    /// The archive round-trips: one entry, with the contents that were put
    /// in. This is the shape `parseEnvFile` reads.
    #[test]
    fn an_archive_holds_one_entry() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = dir.path().join("scratch");
        std::fs::create_dir_all(&source).expect("a directory");
        std::fs::write(source.join(".env"), b"A=1\n").expect("written");

        let archive = container_archive(&source.to_string_lossy()).expect("an archive");
        let mut reader = tar::Archive::new(std::io::Cursor::new(archive));
        let entries = reader.entries().expect("readable");
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.expect("readable");
            names.push(entry.path().expect("a path").to_string_lossy().into_owned());
        }
        assert!(names.contains(&".env".to_string()), "{names:?}");
    }

    /// The host environment's archive of a *single file* is one entry named
    /// after it, which is what `parseEnvFile` expects.
    #[test]
    fn a_single_file_archives_under_its_own_name() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = dir.path().join("one.env");
        std::fs::write(&source, b"ONLY=1\n").expect("written");

        let archive = container_archive(&source.to_string_lossy()).expect("an archive");
        assert_eq!(first_entry_text(&archive).expect("text"), "ONLY=1\n");
    }

    /// The env file's variables reach the step's environment, and a missing
    /// file is not a failure.
    #[test]
    fn update_from_env_merges_and_tolerates_a_missing_file() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let environment = environment(dir.path());
        let ctx = RunContext::new();

        let source = dir.path().join("step.env");
        std::fs::write(&source, b"MERGED=yes\n").expect("written");
        let env = environment
            .update_from_env(&source.to_string_lossy())
            .expect("parsed");
        assert_eq!(env["MERGED"], "yes");

        let untouched = environment
            .update_from_env(&dir.path().join("absent.env").to_string_lossy())
            .expect("a missing env file is not an error");
        assert!(untouched.is_empty());

        // A malformed file *is* an error, so a typo in `env-file:` stops the
        // step instead of silently contributing nothing.
        let broken = dir.path().join("broken.env");
        std::fs::write(&broken, b"no assignment here\n").expect("written");
        assert!(environment
            .update_from_env(&broken.to_string_lossy())
            .is_err());
        let _ = &ctx;
    }

    /// `copy` writes the files, creates their directories, and keeps an
    /// executable bit — which `common::copy_file` does not.
    #[test]
    fn copy_writes_files_with_their_modes() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let environment = environment(dir.path());
        let dest = dir.path().join("dest");
        std::fs::create_dir_all(&dest).expect("a directory");
        let ctx = RunContext::new();

        environment.copy(
            &dest.to_string_lossy(),
            vec![
                FileEntry {
                    name: "nested/plain.txt".to_string(),
                    mode: 0o644,
                    body: "text".to_string(),
                },
                #[cfg(unix)]
                FileEntry {
                    name: "run.sh".to_string(),
                    mode: 0o755,
                    body: "#!/bin/sh\n".to_string(),
                },
            ],
        )(&ctx)
        .expect("copied");

        assert_eq!(
            std::fs::read_to_string(dest.join("nested/plain.txt")).expect("readable"),
            "text",
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dest.join("run.sh"))
                .expect("readable")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
    }

    /// `copyDir` with `useGitIgnore` reads the source's `.gitignore`, so a
    /// build's ignored output does not get staged.
    #[test]
    fn copy_dir_honours_a_gitignore() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let environment = environment(dir.path());
        let ctx = RunContext::new();

        let source = dir.path().join("workspace");
        std::fs::create_dir_all(source.join("node_modules")).expect("a directory");
        std::fs::write(source.join(".gitignore"), b"node_modules/\n").expect("written");
        std::fs::write(source.join("keep.txt"), b"keep").expect("written");
        std::fs::write(source.join("node_modules/junk.js"), b"junk").expect("written");

        let dest = dir.path().join("staged");
        environment
            .copy_dir(
                &dest.to_string_lossy(),
                &source.to_string_lossy(),
                true,
            )
            (&ctx)
            .expect("copied");

        // Upstream's strip prefix is `filepath.Dir(srcPath)`, never `srcPath`
        // itself, so every file keeps its top-level directory name. That is
        // what `TestCopyDir` upstream exercises, and a container copy is
        // expected to arrive with the same layout.
        assert!(
            dest.join("workspace/keep.txt").exists(),
            "the tracked file was copied under its own directory",
        );
        assert!(
            !dest.join("workspace/node_modules").exists(),
            "the ignored directory was not",
        );
        assert!(
            !dest.join("keep.txt").exists(),
            "and not at the top level either, because the prefix is the parent",
        );
    }

    /// A host is always healthy: there is no container that could be starting
    /// or unhealthy.
    #[test]
    fn a_host_is_healthy_and_needs_no_daemon() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let environment = environment(dir.path());
        let ctx = RunContext::new();

        assert_eq!(environment.health(), Health::Healthy);
        // Every container operation is a no-op that succeeds.
        environment.create(&[], &[])(&ctx).expect("created");
        environment.pull(false)(&ctx).expect("pulled");
        environment.start(false)(&ctx).expect("started");
        environment.close()(&ctx).expect("closed");
        assert!(environment.update_from_image_env().expect("no image").is_empty());
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
    }
    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}
}
