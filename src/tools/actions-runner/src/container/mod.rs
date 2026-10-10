//! Running a step: the environment a job executes in, and the contract every
//! environment satisfies.
//!
//! Port of act's `pkg/container` (3,710 lines). act supports six execution
//! back-ends — Docker, Podman, a chroot, a WASM sandbox, macOS, and one that
//! runs the command directly on the host — and this module is the seam between
//! them. CTOX only needs two to start:
//!
//! * [`host_environment::HostEnvironment`] — the command runs directly on the
//!   machine. This is what `-P ubuntu-latest=` does, and it is the one that
//!   needs no daemon at all.
//! * the Docker back-end, which arrives with the `bollard` client.
//!
//! The two Linux-specific pieces of the contract — how a host path becomes a
//! container path, and the default `PATH` a container starts with — live in
//! [`linux`] rather than in either environment, because a chroot, a WASM
//! sandbox and a container share them.
//!
//! # The trait
//!
//! `ExecutionsEnvironment` is `Container` plus the four questions the runner
//! asks about its surroundings. Splitting it this way matters: the Docker
//! back-end implements `Container`, and the four path questions come from a
//! platform extension, so a new back-end does not have to re-decide how
//! `C:\Users\…` maps to `/mnt/c/Users/…`.
//!
//! # `runner.arch` and `runner.os` are GitHub's names, not Go's
//!
//! `amd64` is `X64` in a workflow and `arm64` is `ARM64`, and
//! `darwin` is `macOS`. A workflow that branches on `runner.os` breaks on a
//! port that reports the Go spelling, so the mapping is part of the contract
//! rather than a convenience.

pub mod docker_api;
pub mod docker_auth;
pub mod docker_build;
pub mod docker_cli;
pub mod docker_merge;
pub mod docker_engine;
pub mod docker_log;
pub mod docker_opts;
pub mod docker_opts_mounts;
pub mod docker_opts_types;
pub mod docker_resources;
pub mod docker_socket;
pub mod docker_specs;
pub mod env_file;
pub mod host_environment;
pub mod image_ref;
pub mod linux;
pub mod pflags;
pub mod proc_attr;
pub mod pty_writer;
pub mod shell_quote;

use std::collections::BTreeMap;

use anyhow::Result;
use std::path::PathBuf;

use crate::common::{Executor, RunContext};

pub use host_environment::container_archive;
pub use docker_auth::{
    load_docker_auth_config, load_docker_auth_configs, registry_host, AuthConfigError,
    DockerConfigFile, RegistryAuthConfig,
};
pub use docker_opts::{
    convert_to_standard_notation, parse_device, parse_logging_opts, parse_security_opts,
    parse_storage_opts, parse_system_paths, validate_attach, validate_device,
    validate_device_cgroup_rule, DeviceMapping,
};
pub use docker_socket::{
    get_socket_and_host, is_docker_host_uri, socket_location, SocketAndHost, SocketError,
    COMMON_SOCKET_LOCATIONS,
};
pub use env_file::{parse_env_text, EnvFileError};
pub use host_environment::HostEnvironment;
pub use image_ref::{clean_image, parse_any_reference, Reference};
pub use linux::LinuxContainerEnvironmentExtensions;
pub use pty_writer::PtyWriter;

/// How well a container is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Health {
    /// Created, not yet running.
    Starting,
    /// Running.
    Healthy,
    /// Started and exited badly.
    Unhealthy,
}

/// A file to place inside a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// The path inside the container, including any directories.
    pub name: String,
    /// The Unix mode, as the `tar` header carries it.
    pub mode: u32,
    /// The contents.
    pub body: String,
}

/// What a runner needs from whatever is executing its steps.
///
/// Upstream splits this in two — `Container` and `ExecutionsEnvironment` —
/// because the Docker back-end supplies the first and a platform extension
/// supplies the second. Rust has no interface defaulting, so the platform
/// answers are plain methods on this one trait and the Docker back-end
/// delegates them.
pub trait ExecutionsEnvironment: Send + Sync {
    /// Creates the container. The capabilities are the ones the job asked for
    /// via `capAdd`/`capDrop` in `options`.
    fn create(&self, cap_add: &[String], cap_drop: &[String]) -> Executor;

    /// Removes the container.
    fn close(&self) -> Executor;

    /// Writes `files` into the container at `dest_path`.
    fn copy(&self, dest_path: &str, files: Vec<FileEntry>) -> Executor;

    /// Unpacks a `tar` stream into the container at `dest_path`.
    fn copy_tar_stream(&self, dest_path: &str, tar_stream: &[u8]) -> anyhow::Result<()>;

    /// Copies a host directory into the container at `dest_path`.
    fn copy_dir(&self, dest_path: &str, src_path: &str, use_gitignore: bool) -> Executor;

    /// Reads `src_path` from the container as a `tar` stream.
    fn container_archive(&self, src_path: &str) -> anyhow::Result<Vec<u8>>;

    /// Fetches the image.
    fn pull(&self, force_pull: bool) -> Executor;

    /// Starts the container, optionally attached to its output.
    fn start(&self, attach: bool) -> Executor;

    /// Runs `command` inside the container.
    fn exec(&self, command: &[String], env: &BTreeMap<String, String>, user: &str, workdir: &str)
        -> Executor;

    /// The variables an env file at `src_path` contributes.
    ///
    /// Upstream wraps this in an `Executor` that mutates a map it has
    /// captured. A captured `&mut` cannot live in a `'static` step, so the
    /// port returns the variables and lets the runner merge them — the same
    /// result, without a step that exists only to write into someone else's
    /// map.
    fn update_from_env(&self, src_path: &str) -> Result<BTreeMap<String, String>>;

    /// The image's own environment.
    fn update_from_image_env(&self) -> Result<BTreeMap<String, String>>;

    /// Destroys the container and its volumes.
    fn remove(&self) -> Executor;

    /// How the container is doing.
    fn health(&self) -> Health;

    /// Swaps the log destination, returning the previous pair.
    fn replace_log_writer(
        &self,
        stdout: std::sync::Arc<dyn crate::common::LogSink>,
    ) -> Option<std::sync::Arc<dyn crate::common::LogSink>>;

    /// The host path `path` occupies inside the container.
    fn to_container_path(&self, path: &str) -> String;

    /// Where act's own files live inside the container.
    fn act_path(&self) -> String;

    /// The name of the `PATH` variable, which is `Path` on Windows.
    fn path_variable_name(&self) -> &'static str;

    /// The `PATH` the container starts with.
    fn default_path_variable(&self) -> String;

    /// Joins path entries with the platform's list separator.
    fn join_path_variable(&self, paths: &[&str]) -> String;

    /// `runner.os`, `runner.arch`, `runner.temp` and `runner.tool_cache`.
    fn runner_context(&self, ctx: &RunContext) -> BTreeMap<String, String>;

    /// Whether environment variable names are compared case-insensitively,
    /// which they are on Windows: `Path` and `PATH` are the same variable.
    fn is_environment_case_insensitive(&self) -> bool;
}

/// A container to create.
///
/// The runner's input, kept apart from the environment so a provider can
/// validate it.
#[derive(Debug, Clone, Default)]
pub struct NewContainerInput {
    /// The image reference, `name:tag`.
    pub image: String,
    /// Registry user, for a private image.
    pub username: String,
    /// Registry password or token.
    pub password: String,
    /// Overrides the image's entrypoint.
    pub entrypoint: Vec<String>,
    /// Overrides the image's command.
    pub cmd: Vec<String>,
    /// The working directory inside the container.
    pub working_dir: String,
    /// `KEY=value` pairs.
    pub env: Vec<String>,
    /// `host:container` bind specifications.
    pub binds: Vec<String>,
    /// Volume name to container path.
    pub mounts: BTreeMap<String, String>,
    /// The container's name.
    pub name: String,
    /// The network mode.
    pub network_mode: String,
    /// Whether the container runs with extended privileges.
    pub privileged: bool,
    /// The user namespace mode.
    pub userns_mode: String,
    /// `linux/amd64` and so on.
    pub platform: String,
    /// The raw `options:` string from the job, split into arguments.
    pub options: Vec<String>,
    /// Extra names the container answers to on its network.
    pub network_aliases: Vec<String>,
    /// Ports to expose, `8080/tcp`.
    pub exposed_ports: Vec<String>,
    /// `8080/tcp` to the host ports to publish it on.
    pub port_bindings: BTreeMap<String, Vec<String>>,
}

/// Everything `docker build` needs.
#[derive(Debug, Clone, Default)]
pub struct NewDockerBuildExecutorInput {
    /// The build context's directory.
    pub context_dir: PathBuf,
    /// The Dockerfile, relative to the context.
    pub dockerfile: String,
    /// The tag to apply.
    pub image_tag: String,
    /// The target platform.
    pub platform: String,
}

/// Everything `docker pull` needs.
#[derive(Debug, Clone, Default)]
pub struct NewDockerPullExecutorInput {
    /// The image reference.
    pub image: String,
    /// Whether to re-pull even when the image is present.
    pub force_pull: bool,
    /// The target platform.
    pub platform: String,
    /// Registry user.
    pub username: String,
    /// Registry password or token.
    pub password: String,
}

/// `goArchToActionArch`: Go's `runtime.GOARCH` to GitHub's `runner.arch`.
///
/// <https://docs.github.com/en/actions/learn-github-actions/contexts#runner-context>
///
/// An unmapped value is passed through unchanged, so an architecture act has
/// never heard of reaches the workflow instead of being blank.
pub fn go_arch_to_action_arch(arch: &str) -> String {
    match arch {
        "amd64" | "x86_64" => "X64",
        "386" => "X86",
        "aarch64" => "ARM64",
        other => other,
    }
    .to_string()
}

/// `goOsToActionOs`: Go's `runtime.GOOS` to GitHub's `runner.os`.
///
/// `darwin` is `macOS`, not `Darwin` and not `MacOS` — the capitalisation is
/// what a workflow's `if:` compares against.
pub fn go_os_to_action_os(os: &str) -> String {
    match os {
        "linux" => "Linux",
        "windows" => "Windows",
        "darwin" => "macOS",
        other => other,
    }
    .to_string()
}

/// `RunnerArch`, given the daemon's reported architecture.
///
/// A different table from [`go_arch_to_action_arch`]: the daemon says `arm64`
/// where Go says `aarch64`, and this is the one a container's steps see.
pub fn runner_arch(docker_architecture: &str) -> String {
    match docker_architecture {
        "x86_64" | "amd64" => "X64",
        "386" => "X86",
        "aarch64" | "arm64" => "ARM64",
        other => other,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_architecture_names_are_githubs() {
        for (input, want) in [
            ("amd64", "X64"),
            ("x86_64", "X64"),
            ("386", "X86"),
            ("aarch64", "ARM64"),
        ] {
            assert_eq!(go_arch_to_action_arch(input), want, "{input}");
        }
        // Unknown values pass through rather than becoming blank.
        assert_eq!(go_arch_to_action_arch("riscv64"), "riscv64");
        assert_eq!(go_arch_to_action_arch(""), "");
    }

    #[test]
    fn the_daemon_architecture_table_differs_from_gos() {
        // The daemon reports `arm64`, Go reports `aarch64`; both mean ARM64.
        for (input, want) in [
            ("x86_64", "X64"),
            ("amd64", "X64"),
            ("386", "X86"),
            ("aarch64", "ARM64"),
            ("arm64", "ARM64"),
        ] {
            assert_eq!(runner_arch(input), want, "{input}");
        }
        assert_eq!(runner_arch("mips64le"), "mips64le");
    }

    #[test]
    fn the_os_names_are_githubs() {
        for (input, want) in [
            ("linux", "Linux"),
            ("windows", "Windows"),
            ("darwin", "macOS"),
        ] {
            assert_eq!(go_os_to_action_os(input), want, "{input}");
        }
        assert_eq!(go_os_to_action_os("freebsd"), "freebsd");
    }

    #[test]
    fn the_health_order_is_starting_healthy_unhealthy() {
        assert!(Health::Starting < Health::Healthy);
        assert!(Health::Healthy < Health::Unhealthy);
    }
}
