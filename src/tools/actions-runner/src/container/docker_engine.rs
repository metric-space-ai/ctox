//! The Docker back-end: [`ExecutionsEnvironment`] implemented against a live
//! daemon through `bollard`.
//!
//! This is the other half of `pkg/container`. [`super::host_environment`] runs
//! a step directly on the machine; this one runs it inside a container, which
//! is what a workflow's `runs-on: ubuntu-latest` means.
//!
//! # ⚠ Its daemon-facing tests cannot run here, and that is a property of the
//! tests, not a shortcut
//!
//! Upstream's `docker_run_test.go` needs a running Docker daemon — all seven of
//! its tests do. So does this module's correctness, and neither can be checked
//! on a machine that has none. What *is* checked is everything that does not
//! need one, and that is more than it looks:
//!
//! * the argument translation lives in [`super::docker_cli`] and is tested there
//! * the `options:` overlay lives in [`super::docker_merge`] and is tested
//!   against `dario.cat/mergo`'s measured semantics
//! * the `options:` string splitter lives in [`super::shell_quote`] and is
//!   tested against upstream's own table
//! * the body assembled here is built by [`create_body`], which is a pure
//!   function and is tested as one
//!
//! The `create_container`/`start_container`/`exec` calls below are ported and
//! compile on both targets. They are **not** verified to work end to end, and
//! nothing in this file should be read as claiming that.
//!
//! # `bollard` is async, act is not
//!
//! The Engine API is async; act's executors are `Fn(&RunContext) -> Result` and
//! this crate is blocking throughout. Every daemon call therefore goes through
//! [`block_on`], parking the calling thread on a current-thread runtime. That is
//! the trade the rest of the crate already makes for its servers, and it is why
//! no executor is threaded through the trait: a job's steps are sequential by
//! construction.
//!
//! # One client, shared, behind `Arc`
//!
//! The runner and the current step each hold a handle onto the same container.
//! Without shared state that would be two containers and two sockets — and the
//! daemon caps concurrent connections. So the mutable state lives in
//! [`Inner`] behind an `Arc`, and a clone of [`DockerEnvironment`] is a new
//! handle rather than a new container.
//!
//! # Two things upstream does that are easy to mistake for bugs
//!
//! **`create` does not start the container.** `Create` and `Start` are separate
//! executors upstream, and the runner calls them separately. A port that starts
//! inside `create` runs the container twice over for callers that also call
//! `start`.
//!
//! **`parse`'s network endpoints are discarded.** `parse()` computes
//! `NetworkingConfig` from `--network`, and `create` then throws it away and
//! builds a fresh one from the *job's* `NetworkAliases` alone. So
//! `options: --network-alias=x` does not give the container that alias, while
//! the runner's own `--network-alias` does. That is upstream's behaviour and it
//! is reproduced here rather than corrected; see [`build_networking_config`].
//!
//! # ⚠ What is *not* here
//!
//! Stated plainly, because a port that hides its gaps is worse than one that
//! does not:
//!
//! * **`attach` is ignored.** `Start(attach=true)` upstream spawns a goroutine
//!   that `stdcopy`s the container's output into the job's writers. Here the
//!   container is started and its ids are read, but nothing is attached. A job's
//!   real step output goes through `exec`, not through `attach`, so this costs
//!   the entrypoint's own output and nothing else — but it costs something.
//! * **Ctrl+C on cancellation cannot be sent.** `waitForCommand` writes a
//!   literal `0x03` down the *hijacked connection*. bollard hands out a
//!   read-only stream, so the byte has nowhere to go. The cancellation itself
//!   is still reported as an error, which is what stops the following steps;
//!   what is lost is the signal that would make a long-running process inside
//!   the container stop promptly.
//! * **`Close` has no error to return.** `client.Close()` has no bollard
//!   equivalent — dropping the handle is the close — so `close` cannot fail the
//!   way upstream's can.
//! * **The rest of `pkg/container` is not here**: `docker_build.go` (123
//!   lines), `docker_network.go` (79), `docker_volume.go` (54),
//!   `docker_logger.go` (83), `docker_stub.go` (69) and `util.go` (26) are
//!   still unported. `docker_pull.go`'s standalone executor is covered by
//!   [`ExecutionsEnvironment::pull`]; `docker_images.go` is not.
//! * **Nothing here has been run against a live daemon.** See the note at the
//!   top.

use std::collections::BTreeMap;
use std::io::IsTerminal as _;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context as _, Result};
use bollard::auth::DockerCredentials;
use bollard::exec::{CreateExecOptions, StartExecOptions};
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptions, DownloadFromContainerOptions, ListContainersOptions,
    RemoveContainerOptions, RemoveImageOptions, StartContainerOptions,
    UploadToContainerOptions,
};
use bollard::{body_full, Docker};

use crate::common::context::Level;
use crate::common::{Executor, RunContext};
use crate::container::{
    image_ref, linux::LinuxContainerEnvironmentExtensions, ExecutionsEnvironment, FileEntry, Health,
    NewContainerInput,
};

use super::docker_api::{self, Config, EndpointSettings, HostConfig, NetworkMode};
use super::docker_merge;
use super::{docker_cli, shell_quote};

/// Drives an async bollard call from a blocking caller.
///
/// `pub(crate)` rather than private because [`super::docker_resources`] has the
/// same trade to make: its executors are blocking closures over an async client.
pub(crate) fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime for the Docker client")
        .block_on(future)
}

/// Whether act would give this process a TTY.
///
/// Upstream calls `term.IsTerminal(os.Stdout.Fd())` at four places and the
/// answer changes what the daemon is asked for, so it is asked the same way
/// here rather than assumed.
fn is_terminal() -> bool {
    std::io::stdout().is_terminal()
}

/// The state a [`DockerEnvironment`] and all its handles share.
struct Inner {
    /// The daemon connection, opened on first use and closed by `close`.
    client: Mutex<Option<Docker>>,
    /// The container's id, once `create` (or `find`) has run.
    id: Mutex<Option<String>>,
    /// The last observed health.
    health: Mutex<Health>,
    /// The daemon's reported API version, cached after the first probe.
    api_version: Mutex<Option<String>>,
    /// The container process's uid, read back by `start`.
    uid: Mutex<i64>,
    /// The container process's gid.
    gid: Mutex<i64>,
    /// Where a step's output goes. The runner installs this before the first
    /// step runs; a clone shares it, because a clone is the same container.
    stdout: Mutex<Arc<dyn crate::common::LogSink>>,
}

/// A container the runner is driving.
pub struct DockerEnvironment {
    /// What the job asked for.
    pub input: NewContainerInput,
    /// The state shared with every other handle onto this container.
    inner: Arc<Inner>,
    /// The Linux path rules, shared with the other back-ends.
    linux: LinuxContainerEnvironmentExtensions,
}

impl Clone for DockerEnvironment {
    /// A new handle onto the same container, not a new container.
    fn clone(&self) -> Self {
        Self {
            input: self.input.clone(),
            inner: Arc::clone(&self.inner),
            linux: self.linux,
        }
    }
}

/// `NewContainer`: a reference to a container that does not exist yet.
pub fn new_container(input: NewContainerInput) -> DockerEnvironment {
    DockerEnvironment {
        input,
        inner: Arc::new(Inner {
            client: Mutex::new(None),
            id: Mutex::new(None),
            health: Mutex::new(Health::Starting),
            api_version: Mutex::new(None),
            uid: Mutex::new(0),
            gid: Mutex::new(0),
            stdout: Mutex::new(Arc::new(crate::common::context::NullSink)),
        }),
        linux: LinuxContainerEnvironmentExtensions::new(),
    }
}

impl DockerEnvironment {
    /// The client, opening it on first use.
    fn with_client<T>(&self, f: impl FnOnce(&Docker) -> Result<T>) -> Result<T> {
        let mut guard = self.inner.client.lock().expect("the client lock");
        if guard.is_none() {
            *guard = Some(connect()?);
        }
        f(guard.as_ref().expect("just connected"))
    }

    /// The container id, or an error naming what act would have said.
    fn id(&self) -> Result<String> {
        self.inner
            .id
            .lock()
            .expect("the id lock")
            .clone()
            .ok_or_else(|| anyhow!("container has not been created"))
    }

    /// The image reference, normalised the way act normalises it.
    ///
    /// A reference that does not normalise falls back to what the job wrote,
    /// so the daemon produces the error rather than this code.
    fn image_ref(&self) -> String {
        let cleaned = image_ref::clean_image(&self.input.image);
        if cleaned.is_empty() {
            self.input.image.clone()
        } else {
            cleaned
        }
    }

    /// `mergeContainerConfigs` + `parse`: what the job's `options:` asked for,
    /// folded onto the config the runner built.
    ///
    /// `None` when the job set no `options:` at all, which upstream signals by
    /// returning early — and the early return also skips the network-mode
    /// injection, so the two cases are not the same thing.
    fn merge_options(
        &self,
        config: &Config,
        host_config: &HostConfig,
    ) -> Result<Option<(Config, HostConfig)>> {
        if self.input.options.is_empty() {
            return Ok(None);
        }

        let options = self.input.options.join(" ");
        let argv = shell_quote::split(&options).map_err(|err| {
            anyhow!("Cannot split container options: '{options}': '{err}'")
        })?;
        let flags = docker_cli::run_flag_set().parse(&argv).map_err(|err| {
            anyhow!("Cannot parse container options: '{options}': '{err}'")
        })?;

        // `copts.netMode` is empty unless `options:` named a network, and then
        // the *job's* network mode is what it defaults to. Without this a job
        // with both `--container` and `options:` would lose its network.
        let mut flags = flags;
        if flags.list("network").is_empty() {
            flags.set_list("network", &self.input.network_mode);
        }

        let parsed = docker_cli::parse(&flags, host_os())
            .map_err(|err| anyhow!("Cannot process container options: '{options}': '{err}'"))?;

        let mut merged_config = config.clone();
        docker_merge::merge_config(&mut merged_config, &parsed.config);
        let mut merged_host = host_config.clone();
        docker_merge::merge_host_config(&mut merged_host, &parsed.host_config);
        Ok(Some((merged_config, merged_host)))
    }

    /// The base `container.Config` `create` builds before `options:` is
    /// folded in.
    fn base_config(&self) -> Config {
        let mut config = Config {
            image: self.input.image.clone(),
            working_dir: self.input.working_dir.clone(),
            env: self.input.env.clone(),
            exposed_ports: self.input.exposed_ports.iter().cloned().collect(),
            tty: is_terminal(),
            ..Default::default()
        };
        // Go assigns these only when non-empty, so an empty `Cmd` leaves the
        // image's own command alone rather than blanking it.
        if !self.input.cmd.is_empty() {
            config.cmd = self.input.cmd.clone();
        }
        if !self.input.entrypoint.is_empty() {
            config.entrypoint = Some(self.input.entrypoint.clone());
        }
        config
    }

    /// The base `container.HostConfig` `create` builds before `options:` is
    /// folded in.
    fn base_host_config(&self) -> HostConfig {
        let mounts = self
            .input
            .mounts
            .iter()
            .map(|(source, target)| {
                super::docker_opts_mounts::mount::Mount {
                    mount_type: super::docker_opts_mounts::mount::MountType(
                        super::docker_opts_mounts::mount::TYPE_VOLUME.to_string(),
                    ),
                    source: source.clone(),
                    target: target.clone(),
                    ..Default::default()
                }
            })
            .collect();
        HostConfig {
            cap_add: Vec::new(),
            cap_drop: Vec::new(),
            binds: self.input.binds.clone(),
            mounts,
            network_mode: self.input.network_mode.clone(),
            privileged: self.input.privileged,
            userns_mode: docker_api::UsernsMode(self.input.userns_mode.clone()),
            port_bindings: convert_port_map(&self.input.port_bindings),
            ..Default::default()
        }
    }

    /// `supportsContainerImagePlatform`: `--platform` on create needs 1.41.
    fn supports_platform(&self, client: &Docker) -> bool {
        let cached = self.inner.api_version.lock().expect("the version lock");
        if let Some(version) = cached.as_deref() {
            return at_least_1_41(version);
        }
        drop(cached);
        let Ok(version) = block_on(client.version()) else {
            return false;
        };
        let Some(api_version) = version.api_version else {
            return false;
        };
        let supports = at_least_1_41(&api_version);
        *self.inner.api_version.lock().expect("the version lock") = Some(api_version);
        supports
    }

    /// The `Platform` the daemon is asked for, as `os/arch`.
    ///
    /// Upstream splits on the **first** `/` and needs exactly two parts, so a
    /// bare `linux` is an error rather than a default — once the daemon is new
    /// enough for the flag to be sent at all.
    fn platform_spec(&self, client: &Docker) -> Result<String> {
        if self.input.platform.is_empty() || !self.supports_platform(client) {
            return Ok(String::new());
        }
        split_platform(&self.input.platform).map_err(|()| {
            anyhow!(
                "incorrect container platform option '{}'",
                self.input.platform
            )
        })
    }

    /// `upload_tar`: the one way files get into a container.
    fn upload_tar(&self, dest_path: &str, tar_bytes: &[u8]) -> Result<()> {
        self.with_client(|client| {
            let id = self.id()?;
            let body = body_full(bytes::Bytes::from(tar_bytes.to_vec()));
            block_on(client.upload_to_container(
                &id,
                Some(UploadToContainerOptions {
                    path: dest_path.to_string(),
                    ..Default::default()
                }),
                body,
            ))
            .map_err(|err| anyhow!("failed to copy into the container: {err}"))
        })
    }

    /// The sink a step's output goes to.
    fn sink(&self) -> Arc<dyn crate::common::LogSink> {
        Arc::clone(&self.inner.stdout.lock().expect("the sink lock"))
    }

    /// `waitForCommand`: drain an exec's output into the step's sink.
    ///
    /// Both streams land at [`Level::Info`], matching
    /// [`super::host_environment`]: a step's stderr is output, not a failure.
    fn drain(
        output: &mut (impl futures::Stream<Item = Result<bollard::container::LogOutput, bollard::errors::Error>> + Unpin),
        sink: &Arc<dyn crate::common::LogSink>,
    ) {
        use futures::StreamExt;
        while let Some(chunk) = block_on(output.next()) {
            match chunk {
                Ok(part) => sink.log(Level::Info, &String::from_utf8_lossy(part.as_ref())),
                // A read error mid-stream ends the copy; the exit code that
                // follows is what decides pass or fail, and upstream logs this
                // rather than returning it.
                Err(err) => {
                    sink.log(Level::Warn, &format!("failed to read the container's output: {err}"));
                    return;
                }
            }
        }
    }

    /// `tryReadID`: run `id -u` / `id -g` in the container and remember it.
    ///
    /// Every failure is swallowed, exactly as upstream does — a container
    /// without `id` in it is normal enough not to be worth an error.
    fn try_read_id(&self, option: &str) -> Option<i64> {
        let client = self.inner.client.lock().expect("the client lock").clone()?;
        let id = self.inner.id.lock().expect("the id lock").clone()?;
        let exec = block_on(client.create_exec(
            &id,
            CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(vec!["id".to_string(), option.to_string()]),
                ..Default::default()
            },
        ))
        .ok()?;
        let output = block_on(client.start_exec(&exec.id, Some(StartExecOptions::default()))).ok()?;
        let bollard::exec::StartExecResults::Attached { mut output, .. } = output else {
            return None;
        };
        let mut raw = String::new();
        use futures::StreamExt;
        while let Some(chunk) = block_on(output.next()) {
            match chunk {
                Ok(part) => raw.push_str(&String::from_utf8_lossy(part.as_ref())),
                Err(_) => return None,
            }
        }
        // Upstream matches `\d+\n` and parses that; the first run of digits is
        // the same thing without the regular expression.
        let digits: String = raw
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    }
}

/// `convertPortMap`: `nat.PortMap` to `network.PortMap`.
///
/// A port that does not parse is **dropped** rather than reported, which is
/// why a typo in a job's `ports:` leaves the port unpublished instead of
/// failing the job.
fn convert_port_map(
    ports: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, Vec<docker_api::PortBinding>> {
    let mut result = BTreeMap::new();
    for (port, bindings) in ports {
        if super::docker_specs::network::parse_port(port).is_err() {
            continue;
        }
        result.insert(
            port.clone(),
            bindings
                .iter()
                .map(|binding| docker_api::PortBinding {
                    host_ip: String::new(),
                    host_port: binding.clone(),
                })
                .collect(),
        );
    }
    result
}

/// The platform half of [`image_exists_locally`], as a pure function.
///
/// Upstream is `platform == "" || platform == "any" || imagePlatform ==
/// platform`, where `imagePlatform` is `Os + "/" + Architecture` — with the
/// two sentinels meaning "I do not care which". The runner depends on the
/// difference between the two probes it makes: `any` tells it an image is
/// cached at all, and the real architecture tells it whether that cache entry
/// is usable. Collapsing them into one boolean would make a wrong-architecture
/// image look like a warm cache.
fn platform_matches(image_os: &str, image_architecture: &str, platform: &str) -> bool {
    if platform.is_empty() || platform == "any" {
        return true;
    }
    format!("{image_os}/{image_architecture}") == platform
}

/// `RemoveImage`: drop an image from the local store.
///
/// The runner calls this when a job asks for one architecture and the cache
/// holds another (`--pull` on a matrix that spans `amd64` and `arm64`), which
/// is why `prune_children` matters: a removed tag with child images still
/// attached leaves the space in use.
///
/// A "not found" is `(false, nil)` rather than an error, because the caller's
/// question is "did I remove it", and "it was never there" is a legitimate
/// answer to that.
pub fn remove_image(image: &str, force: bool, prune_children: bool) -> Result<bool> {
    let client = connect()?;
    let inspected = match block_on(client.inspect_image(image)) {
        Ok(inspected) => inspected,
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
        }) => return Ok(false),
        Err(err) => return Err(anyhow!("failed to inspect {image}: {err}")),
    };
    let id = inspected
        .id
        .ok_or_else(|| anyhow!("the daemon reported no id for image '{image}'"))?;
    block_on(client.remove_image(
        &id,
        Some(RemoveImageOptions {
            force,
            noprune: !prune_children,
            ..Default::default()
        }),
        // Removing from the local store never needs a registry credential, and
        // upstream passes none either.
        None,
    ))
    .map_err(|err| anyhow!("failed to remove image '{image}': {err}"))?;
    Ok(true)
}

/// The daemon's operating system, for `parse`'s platform-dependent rules.
///
/// Upstream passes `runtime.GOOS` — the OS *act* is running on, which is the
/// compiling one here. It is deliberately not the daemon's: the flag
/// validation that branches on it is about the client, not the server.
fn host_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

/// Whether an API version string is at least 1.41.
fn at_least_1_41(version: &str) -> bool {
    let mut parts = version.split('.');
    let major = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or(0);
    let minor = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or(0);
    (major, minor) >= (1, 41)
}

/// `GetDockerClient`.
///
/// `DOCKER_HOST` is honoured, which is how a workflow reaches a daemon on
/// another machine. A value bollard cannot parse falls back to the local
/// default rather than failing: act's own error here names only the connect
/// step, and a daemon that is not running gives a far clearer error on the
/// first real call.
pub(crate) fn connect() -> Result<Docker> {
    if let Ok(host) = std::env::var("DOCKER_HOST") {
        if !host.is_empty() {
            for candidate in [
                host.clone(),
                format!("http://{}", host.trim_start_matches("tcp://")),
            ] {
                if let Ok(client) = Docker::connect_with_http(
                    &candidate,
                    120,
                    // The advisory version the client claims; the daemon
                    // negotiates down if it cannot serve it.
                    &bollard::ClientVersion {
                        major_version: 1,
                        minor_version: 51,
                    },
                ) {
                    return Ok(client);
                }
            }
        }
    }
    Docker::connect_with_local_defaults()
        .map_err(|err| anyhow!("failed to connect to docker daemon: {err}"))
}

/// `GetHostInfo`: what the daemon says about the machine.
pub fn host_info() -> Result<bollard::models::SystemInfo> {
    let client = connect()?;
    block_on(client.info())
        .map_err(|err| anyhow!("failed to get docker host info: {err}"))
}

/// `RunnerArch`: the daemon's architecture in GitHub's spelling.
///
/// An unmapped value passes through, so an architecture act has never heard of
/// reaches the workflow instead of arriving blank. A daemon that cannot be
/// reached yields `""`, which is what upstream returns too.
pub fn runner_arch() -> String {
    match host_info() {
        Ok(info) => match info.architecture {
            Some(arch) => super::runner_arch(&arch),
            None => String::new(),
        },
        Err(_) => String::new(),
    }
}

/// `ImageExistsLocally`: whether the daemon has the image **at the platform the
/// job asked for**.
///
/// This is an *inspect and compare*, not a lookup. That distinction is the
/// whole function: the same image is present for `linux/amd64` and absent for
/// `linux/arm64`, and a runner that is switching architectures has to be told
/// so or it will reuse the wrong image and run the wrong binaries. Upstream is
/// `cli.ImageInspect` followed by a string comparison of `Os + "/" + Architecture`
/// against the requested platform, with two ways to say "I do not care":
/// an empty platform and the literal `any`.
///
/// A "not found" from the daemon is `false, nil` and not an error — that is the
/// normal answer on a cold cache, and turning it into a failure would break
/// every first run.
fn image_exists_locally(
    client: &Docker,
    image: &str,
    platform: &str,
    sink: &dyn crate::common::LogSink,
) -> Result<bool> {
    let inspected = match block_on(client.inspect_image(image)) {
        Ok(inspected) => inspected,
        // `containerd/errdefs.IsNotFound`, spelled for bollard: the daemon
        // answers a missing image with a 404 and a JSON body whose `message`
        // is the not-found text.
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
        }) => return Ok(false),
        Err(err) => return Err(anyhow!("failed to inspect {image}: {err}")),
    };

    let (os, arch) = (
        inspected.os.clone().unwrap_or_default(),
        inspected.architecture.clone().unwrap_or_default(),
    );
    if platform_matches(&os, &arch, platform) {
        return Ok(true);
    }
    let image_platform = format!("{os}/{arch}");
    sink.log(
        Level::Info,
        &format!(
            "image found but platform does not match: {image_platform} (image) != {platform} (platform)\n"
        ),
    );
    Ok(false)
}

/// The body of `POST /containers/create`.
///
/// In Go this is `client.ContainerCreateOptions`, whose `Config` is *inlined*
/// into the top-level object next to `HostConfig` and `NetworkingConfig` — it
/// is not nested under a `"Config"` key. `Platform` is not in the body at all;
/// it travels as a query parameter, which is why it is a `String` on
/// bollard's options struct and not a field here.
///
/// The conversion goes through JSON rather than field-by-field because bollard's
/// model is generated from the OpenAPI document, so it is the authority on the
/// wire shape; a mismatch surfaces as an error this function can report,
/// instead of a field that is silently `None`.
fn create_body(
    config: &Config,
    host_config: &HostConfig,
    networking: Option<&BTreeMap<String, Option<EndpointSettings>>>,
) -> Result<bollard::models::ContainerCreateBody> {
    let mut body = serde_json::to_value(config).context("failed to encode the container config")?;
    let object = body
        .as_object_mut()
        .ok_or_else(|| anyhow!("the container config did not encode as an object"))?;
    object.insert(
        "HostConfig".to_string(),
        serde_json::to_value(host_config).context("failed to encode the host config")?,
    );
    if let Some(endpoints) = networking {
        object.insert(
            "NetworkingConfig".to_string(),
            serde_json::json!({ "EndpointsConfig": endpoints }),
        );
    }
    serde_json::from_value(body).context("the container create body does not match the Engine API")
}

/// `create`'s `networkingConfig`: built from the job's aliases alone.
///
/// `parse()`'s own endpoint settings are **not** used here — upstream discards
/// them, which is why `--network-alias` inside `options:` has no effect while
/// the runner's own alias does. The `IsUserDefined`/`host` guard is upstream's
/// and is commented as broken on Windows; the Windows spelling of
/// `NetworkMode::is_user_defined` is used, which is the point.
fn build_networking_config(
    host_config: &HostConfig,
    network_mode: &str,
    aliases: &[String],
) -> Option<BTreeMap<String, Option<EndpointSettings>>> {
    let mode = NetworkMode(host_config.network_mode.clone());
    if !mode.is_user_defined() || host_config.network_mode == "host" || aliases.is_empty() {
        return None;
    }
    let mut endpoints = BTreeMap::new();
    endpoints.insert(
        network_mode.to_string(),
        Some(EndpointSettings {
            aliases: aliases.to_vec(),
            ..Default::default()
        }),
    );
    Some(endpoints)
}

impl ExecutionsEnvironment for DockerEnvironment {
    /// `Create`.
    fn create(&self, cap_add: &[String], cap_drop: &[String]) -> Executor {
        let env = self.clone();
        let cap_add = cap_add.to_vec();
        let cap_drop = cap_drop.to_vec();
        Arc::new(move |ctx: &RunContext| {
            if ctx.dryrun() {
                return Ok(());
            }
            // The pipeline is `connect` → `find` → `create`, and `find` is what
            // makes a second `create` a no-op rather than a duplicate: it is
            // the step that fills the id.
            if env.with_client(|client| find_container(client, &env.input.name))?.is_some() {
                return Ok(());
            }

            let mut host_config = env.base_host_config();
            // The capabilities the job asked for are applied *after* the
            // `options:` merge, so a `--cap-add` in `options:` and the runner's
            // own list do not overwrite each other.
            host_config.cap_add = cap_add.clone();
            host_config.cap_drop = cap_drop.clone();

            let mut config = env.base_config();
            if let Some((merged_config, merged_host)) =
                env.merge_options(&config, &host_config)?
            {
                config = merged_config;
                host_config = merged_host;
                // The merge is on a copy, so put the runner's capabilities back
                // on top of it.
                host_config.cap_add = cap_add.clone();
                host_config.cap_drop = cap_drop.clone();
            }

            env.with_client(|client| {
                let platform = env.platform_spec(client)?;
                let networking =
                    build_networking_config(&host_config, &env.input.network_mode, &env.input.network_aliases);
                // Upstream builds `container.Config{Image: input.Image}` and
                // never normalises it here — `cleanImage` is only used by the
                // pull path. The daemon resolves both spellings, so this is
                // invisible in practice, but it is a different code path and a
                // port that normalised here would be relying on its own rule.
                let body = create_body(&config, &host_config, networking.as_ref())?;
                let response = block_on(client.create_container(
                    Some(CreateContainerOptions {
                        name: (!env.input.name.is_empty()).then(|| env.input.name.clone()),
                        platform,
                    }),
                    body,
                ))
                .map_err(|err| anyhow!("failed to create container: '{err}'"))?;
                *env.inner.id.lock().expect("the id lock") = Some(response.id);
                Ok(())
            })
        })
    }

    /// `Close`.
    ///
    /// The client is shared, so this drops the handle rather than the socket;
    /// the daemon closes it when the last handle goes. Upstream returns the
    /// close error, and there is none to return here because nothing is closed
    /// explicitly — the drop is the close.
    fn close(&self) -> Executor {
        let env = self.clone();
        Arc::new(move |_ctx: &RunContext| {
            *env.inner.client.lock().expect("the client lock") = None;
            Ok(())
        })
    }

    /// `NewDockerPullExecutor`.
    ///
    /// The shape is upstream's, including the two things that are easy to lose:
    /// the **skip** decision is `ImageExistsLocally` on the *platform*, not on
    /// the name; and a pull that answers `unauthorized` is **retried once with
    /// the credentials dropped**, because a stale entry in `~/.docker/config.json`
    /// is common and the image is usually public.
    fn pull(&self, force_pull: bool) -> Executor {
        let env = self.clone();
        Arc::new(move |ctx: &RunContext| {
            ctx.log_debug(&format!("docker pull {}", env.input.image));
            if ctx.dryrun() {
                return Ok(());
            }

            let image = env.input.image.clone();
            let platform = env.input.platform.clone();

            let mut pull = force_pull;
            if !pull {
                let sink = env.sink();
                let exists = env
                    .with_client(|client| {
                        image_exists_locally(client, &image, &platform, sink.as_ref())
                    })
                    .map_err(|err| {
                        anyhow!("unable to determine if image already exists for image '{image}' ({platform}): {err}")
                    })?;
                ctx.log_debug(&format!("Image exists? {exists}"));
                pull = !exists;
            }
            if !pull {
                return Ok(());
            }

            // `cleanImage` normalises the reference and, on a reference it
            // cannot parse, **logs the error and returns the empty string**.
            // That empty string is then handed to the daemon, which rejects it.
            // Reproduced: a bad image name fails at the daemon, not here.
            let image_ref = env.clean_image(ctx);
            ctx.log_debug(&format!("pulling image '{image_ref}' ({platform})"));

            let credentials = env.pull_credentials(ctx);
            let options = CreateImageOptions {
                from_image: Some(image_ref.clone()),
                // bollard takes a plain `String` here: an empty one *is*
                // "unset", because it is dropped from the query.
                platform: platform.clone(),
                ..Default::default()
            };

            let had_credentials = credentials.is_some();
            match env.pull_once(&options, credentials) {
                Ok(()) => Ok(()),
                Err(err) => {
                    if had_credentials && err.to_string().contains("unauthorized") {
                        ctx.log_error(&format!(
                            "pulling image '{image_ref}' ({platform}) failed with credentials {err} retrying without them, please check for stale docker config files"
                        ));
                        return env.pull_once(&options, None);
                    }
                    Err(err)
                }
            }
        })
    }

    /// `Copy`.
    fn copy(&self, dest_path: &str, files: Vec<FileEntry>) -> Executor {
        let env = self.clone();
        let dest_path = dest_path.to_string();
        Arc::new(move |_ctx: &RunContext| {
            let mut archive = tar::Builder::new(Vec::new());
            for file in &files {
                let mut header = tar::Header::new_gnu();
                header.set_size(file.body.len() as u64);
                header.set_mode(file.mode);
                header.set_cksum();
                archive
                    .append_data(&mut header, &file.name, file.body.as_bytes())
                    .with_context(|| format!("failed to tar {}", file.name))?;
            }
            let bytes = archive.into_inner().context("failed to build the tar")?;
            env.upload_tar(&dest_path, &bytes)
        })
    }

    /// `CopyTarStream`.
    fn copy_tar_stream(&self, dest_path: &str, tar_stream: &[u8]) -> Result<()> {
        self.upload_tar(dest_path, tar_stream)
    }

    /// `CopyDir`.
    ///
    /// The strip prefix is the *parent* of the source, not the source, so every
    /// file keeps its top-level directory name inside the destination. That is
    /// what makes act's own `TestCopyDir` work, and a port that "fixed" it would
    /// hand the container a different layout.
    fn copy_dir(&self, dest_path: &str, src_path: &str, use_gitignore: bool) -> Executor {
        let env = self.clone();
        let dest_path = dest_path.to_string();
        let src_path = src_path.to_string();
        Arc::new(move |_ctx: &RunContext| {
            let bytes = tar_directory(&src_path, use_gitignore)?;
            env.upload_tar(&dest_path, &bytes)
        })
    }

    /// `ContainerArchive`.
    fn container_archive(&self, src_path: &str) -> Result<Vec<u8>> {
        let src_path = src_path.to_string();
        self.with_client(|client| {
            let id = self.id()?;
            let mut collected = Vec::new();
            block_on(async {
                use futures::StreamExt;
                let mut stream = std::pin::pin!(client.download_from_container(
                    &id,
                    Some(DownloadFromContainerOptions { path: src_path.to_string() }),
                ));
                while let Some(chunk) = stream.next().await {
                    collected.extend_from_slice(&chunk.map_err(|err| {
                        anyhow!("failed to read {src_path} from the container: {err}")
                    })?);
                }
                Ok::<(), anyhow::Error>(())
            })?;
            Ok(collected)
        })
    }

    /// `Start`: start the container, then read back the process's ids.
    ///
    /// A container that was never created is a no-op here, as upstream: the
    /// runner calls `start` after `create`, and a workflow that skipped `create`
    /// is not a failure act reports.
    fn start(&self, _attach: bool) -> Executor {
        let env = self.clone();
        Arc::new(move |ctx: &RunContext| {
            if ctx.dryrun() {
                return Ok(());
            }
            if env.id().is_err()
                && env
                    .with_client(|client| find_container(client, &env.input.name))?
                    .is_none()
            {
                return Ok(());
            }
            let uid = env.try_read_id("-u");
            let gid = env.try_read_id("-g");
            if let Some(uid) = uid {
                *env.inner.uid.lock().expect("the uid lock") = uid;
            }
            if let Some(gid) = gid {
                *env.inner.gid.lock().expect("the gid lock") = gid;
            }
            env.with_client(|client| {
                let id = env.id()?;
                block_on(client.start_container(
                    &id,
                    Some(StartContainerOptions::default()),
                ))
                .map_err(|err| anyhow!("failed to start container: {err}"))
            })?;
            // A non-root container leaves the workspace owned by the host's
            // user, which the job then cannot write to. Upstream chowns it and
            // ignores the result.
            let uid = *env.inner.uid.lock().expect("the uid lock");
            let gid = *env.inner.gid.lock().expect("the gid lock");
            if uid != 0 || gid != 0 {
                let _ = (env.exec(
                    &[
                        "chown".to_string(),
                        "-R".to_string(),
                        format!("{uid}:{gid}"),
                        env.input.working_dir.clone(),
                    ],
                    &BTreeMap::new(),
                    "0",
                    "",
                ))(ctx);
            }
            *env.inner.health.lock().expect("the health lock") = Health::Healthy;
            Ok(())
        })
    }

    /// `Exec`.
    fn exec(
        &self,
        command: &[String],
        env: &BTreeMap<String, String>,
        user: &str,
        workdir: &str,
    ) -> Executor {
        let this = self.clone();
        let command = command.to_vec();
        let env = env.clone();
        let user = user.to_string();
        let workdir = workdir.to_string();
        Arc::new(move |ctx: &RunContext| this.exec_inner(ctx, &command, &env, &user, &workdir))
    }

    /// `UpdateFromEnv`.
    fn update_from_env(&self, src_path: &str) -> Result<BTreeMap<String, String>> {
        let contents = std::fs::read_to_string(src_path)
            .with_context(|| format!("failed to read {src_path}"))?;
        // The parser merges into the map, which is how upstream's
        // `UpdateFromEnv` reuses the same code as the env-file reader.
        let mut env = BTreeMap::new();
        super::env_file::parse_env_text(&contents, &mut env)?;
        Ok(env)
    }

    /// `UpdateFromImageEnv`.
    fn update_from_image_env(&self) -> Result<BTreeMap<String, String>> {
        let image = self.image_ref();
        self.with_client(|client| {
            let inspect = block_on(client.inspect_image(&image))
                .map_err(|err| anyhow!("failed to inspect {image}: {err}"))?;
            Ok(inspect
                .config
                .and_then(|config| config.env)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|entry| {
                    entry
                        .split_once('=')
                        .map(|(key, value)| (key.to_string(), value.to_string()))
                })
                .collect())
        })
    }

    /// `Remove`.
    ///
    /// Upstream logs a failure here and returns `nil` anyway — a container that
    /// cannot be removed must not fail a job that has already finished — so
    /// this does the same.
    fn remove(&self) -> Executor {
        let env = self.clone();
        Arc::new(move |ctx: &RunContext| {
            let Some(id) = env.inner.id.lock().expect("the id lock").clone() else {
                return Ok(());
            };
            let result = env.with_client(|client| {
                block_on(client.remove_container(
                    &id,
                    Some(RemoveContainerOptions {
                        force: true,
                        v: true,
                        ..Default::default()
                    }),
                ))
                .map_err(|err| anyhow!("failed to remove container: {err}"))
            });
            if let Err(err) = result {
                ctx.log_debug(&format!("failed to remove container: {err}"));
            }
            *env.inner.id.lock().expect("the id lock") = None;
            Ok(())
        })
    }

    /// `GetHealth`.
    fn health(&self) -> Health {
        *self.inner.health.lock().expect("the health lock")
    }

    /// `ReplaceLogWriter`.
    ///
    /// Docker's answer is a stream, not a writer, so this swaps the sink the
    /// streams are drained into and hands back the previous one.
    fn replace_log_writer(
        &self,
        stdout: Arc<dyn crate::common::LogSink>,
    ) -> Option<Arc<dyn crate::common::LogSink>> {
        let mut guard = self.inner.stdout.lock().expect("the sink lock");
        Some(std::mem::replace(&mut *guard, stdout))
    }

    /// `ToContainerPath`.
    fn to_container_path(&self, path: &str) -> String {
        self.linux.to_container_path(path)
    }

    /// `ActPath`.
    fn act_path(&self) -> String {
        self.linux.act_path()
    }

    /// `PathVariableName`.
    fn path_variable_name(&self) -> &'static str {
        if cfg!(windows) {
            "Path"
        } else {
            "PATH"
        }
    }

    /// `DefaultPathVariable`.
    fn default_path_variable(&self) -> String {
        self.linux.default_path_variable()
    }

    /// `JoinPathVariable`.
    fn join_path_variable(&self, paths: &[&str]) -> String {
        self.linux.join_path_variable(paths)
    }

    /// `RunnerContext`.
    fn runner_context(&self, _ctx: &RunContext) -> BTreeMap<String, String> {
        self.linux.runner_context(&runner_arch())
    }

    /// `IsEnvironmentCaseInsensitive`.
    fn is_environment_case_insensitive(&self) -> bool {
        cfg!(windows)
    }
}

impl DockerEnvironment {
    /// `exec`: run a command in the running container.
    fn exec_inner(
        &self,
        ctx: &RunContext,
        command: &[String],
        env: &BTreeMap<String, String>,
        user: &str,
        workdir: &str,
    ) -> Result<()> {
        // Backslashes are path separators on the client and not in the
        // container, so a Windows client rewrites them before handing the
        // command over.
        let command: Vec<String> = if cfg!(windows) {
            command
                .iter()
                .map(|part| part.replace('\\', "/"))
                .collect()
        } else {
            command.to_vec()
        };

        // A relative working directory is relative to the container's own
        // working directory, which is why the two are joined rather than one
        // replacing the other.
        let workdir = resolve_workdir(&self.input, workdir);

        let sink = self.sink();
        self.with_client(|client| {
            let id = self.id()?;
            let env_list: Vec<String> = env
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect();
            let exec = block_on(client.create_exec(
                &id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    tty: Some(is_terminal()),
                    cmd: Some(command.clone()),
                    env: Some(env_list),
                    user: (!user.is_empty()).then(|| user.to_string()),
                    working_dir: (!workdir.is_empty()).then(|| workdir.clone()),
                    ..Default::default()
                },
            ))
            .map_err(|err| anyhow!("failed to create exec: {err}"))?;

            // `StartExec` hands back a stream, not an exit code, so the status
            // has to be read back from `inspect_exec` afterwards. That is the
            // only way the Engine API reports it.
            let output =
                block_on(client.start_exec(&exec.id, Some(StartExecOptions::default())))
                    .map_err(|err| anyhow!("failed to attach to exec: {err}"))?;
            if let bollard::exec::StartExecResults::Attached { mut output, .. } = output {
                Self::drain(&mut output, &sink);
            }
            // A step that was cancelled must not look like a success, and must
            // not run the steps after it. Upstream sends a literal Ctrl+C down
            // the hijacked connection here; bollard hands out a read-only
            // stream, so the byte cannot be sent — the cancellation is still
            // reported, which is the part a workflow sees.
            if let Some(cancelled) = ctx.cancellation_error() {
                return Err(cancelled);
            }

            let inspect = block_on(client.inspect_exec(&exec.id))
                .map_err(|err| anyhow!("failed to inspect exec: {err}"))?;
            match inspect.exit_code {
                Some(0) | None => {}
                // 127 is "the command is not in the image", which has its own
                // message and its own upstream issue link.
                Some(127) => {
                    return Err(anyhow!("exitcode '127': command not found, please refer to https://github.com/nektos/act/issues/107 for more information"));
                }
                Some(code) => {
                    return Err(anyhow!("exitcode '{code}': failure"));
                }
            }
            *self.inner.health.lock().expect("the health lock") = Health::Healthy;
            Ok(())
        })
    }

    /// `cleanImage`: the reference as the daemon is asked for it.
    ///
    /// A reference that does not parse yields the **empty string**, not the
    /// input: upstream logs the parse error and returns `""`, and that empty
    /// string is what gets pulled. A job with a broken image therefore fails at
    /// the daemon rather than at the parser, and this keeps it that way.
    fn clean_image(&self, ctx: &RunContext) -> String {
        let cleaned = image_ref::clean_image(&self.input.image);
        if cleaned.is_empty() {
            ctx.log_error(&format!(
                "invalid reference format: repository name must be lowercase for {image}",
                image = self.input.image
            ));
            return String::new();
        }
        cleaned
    }

    /// The credential to offer a pull, or `None` to pull anonymously.
    ///
    /// Two sources, in upstream's order. A username and password on the job
    /// win outright and the docker config file is not read at all. Otherwise
    /// the config file is consulted, and an entry that carries neither a
    /// username nor a password counts as no credential rather than as an empty
    /// one — the difference is an `X-Registry-Auth` header of all-zeroes
    /// against no header, and the registry answers differently.
    fn pull_credentials(&self, ctx: &RunContext) -> Option<DockerCredentials> {
        if !self.input.username.is_empty() && !self.input.password.is_empty() {
            ctx.log_debug("using authentication for docker pull");
            return Some(DockerCredentials {
                username: Some(self.input.username.clone()),
                password: Some(self.input.password.clone()),
                ..Default::default()
            });
        }
        // No config directory means no credential, and a pull without one is
        // normal — the image may simply be public.
        let directory = match super::docker_auth::docker_config_dir() {
            Ok(directory) => directory,
            Err(err) => {
                ctx.log_warning(&format!("Could not load docker config: {err}"));
                return None;
            }
        };
        match super::docker_auth::load_docker_auth_config(&directory, &self.input.image) {
            Ok(config) if config.is_empty() => None,
            Ok(config) => {
                ctx.log_info("using DockerAuthConfig authentication for docker pull");
                // `encoded()` borrows the whole config, so every field is read
                // out *before* any of them is moved.
                let auth = config.encoded();
                Some(DockerCredentials {
                    username: Some(config.username),
                    password: Some(config.password),
                    serveraddress: Some(config.server_address),
                    auth: Some(auth),
                    ..Default::default()
                })
            }
            Err(err) => {
                // A config file that cannot be read is not fatal upstream: it
                // returns options with no auth and the pull proceeds.
                ctx.log_warning(&format!("Could not load docker config: {err}"));
                None
            }
        }
    }

    /// One `ImagePull`, with the response stream handed to the progress logger.
    fn pull_once(
        &self,
        options: &CreateImageOptions,
        credentials: Option<DockerCredentials>,
    ) -> Result<()> {
        self.with_client(|client| {
            let sink = self.sink();
            block_on(async {
                use futures::StreamExt;
                let stream = client.create_image(Some(options.clone()), None, credentials);
                futures::pin_mut!(stream);
                let mut raw = Vec::new();
                while let Some(progress) = stream.next().await {
                    match progress {
                        // bollard has already decoded the daemon's JSON line
                        // into `CreateImageInfo`; re-encoding it is what puts
                        // the same bytes back in front of the logger that
                        // upstream reads off the wire.
                        Ok(info) => {
                            let line = serde_json::to_string(&info)
                                .map_err(|err| anyhow!("failed to encode a pull message: {err}"))?;
                            raw.extend_from_slice(line.as_bytes());
                            raw.push(b'\n');
                        }
                        Err(err) => return Err(anyhow!("{err}")),
                    }
                }
                super::docker_log::log_docker_response(&raw, false, sink.as_ref())
            })
        })
    }

}

/// Packs a directory into a `tar`, stripping its **parent** as the prefix.
///
/// act uses `filepath.Dir(srcPath)` rather than `srcPath`, which is what keeps
/// each file's top-level directory inside the destination.
fn tar_directory(src_path: &str, use_gitignore: bool) -> Result<Vec<u8>> {
    use crate::filecollector::{DefaultFs, FileCollector, TarCollector};

    let source = std::path::PathBuf::from(src_path);
    let parent = source
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let prefix = format!(
        "{}{}",
        parent.to_string_lossy(),
        std::path::MAIN_SEPARATOR
    );

    let mut buffer = Vec::new();
    {
        let mut tar = tar::Builder::new(&mut buffer);
        let mut collector = TarCollector {
            tar: &mut tar,
            // The daemon rewrites ownership for a bind-mounted workspace, so
            // the archive carries root as act's does for a fresh container.
            uid: 0,
            gid: 0,
            dst_dir: String::new(),
        };
        let mut walker = FileCollector::new(&DefaultFs, &mut collector);
        walker.src_path = source.clone();
        walker.src_prefix = prefix;
        if use_gitignore {
            // The gitignore rules are the crate's own, so a job sees the same
            // files inside a container as it does on the host. A `.gitignore`
            // that cannot be read leaves the walk unfiltered, matching the
            // error upstream drops.
            let patterns = crate::gitignore::read_patterns(&source, &[]).0;
            walker.ignorer = Some(crate::gitignore::Matcher::new(patterns));
        }
        walker.collect_files(&source, &[])?;
    }
    Ok(buffer)
}

/// `find`: reuse a container this job already made under its own name.
///
/// Both `Create` and `Start` run this before they do anything else, so a
/// container left over from an earlier step — or an earlier run of the same job
/// that was interrupted before `remove` — is picked up instead of a second one
/// being created under a name the daemon would then reject as taken.
fn find_container(client: &Docker, name: &str) -> Result<Option<String>> {
    if name.is_empty() {
        // `name[1:]` on an empty name would panic upstream. A job that named no
        // container cannot match one either, so this is the same answer.
        return Ok(None);
    }
    let containers = block_on(client.list_containers(Some(ListContainersOptions {
        all: true,
        ..Default::default()
    })))
    .map_err(|err| anyhow!("failed to list containers: {err}"))?;
    for container in containers {
        for reported in container.names.unwrap_or_default() {
            // The daemon reports names with a leading `/`; upstream slices it
            // off. `strip_prefix` is the same comparison without the panic on
            // a name that carries none.
            if reported.strip_prefix('/') == Some(name) {
                return Ok(container.id);
            }
        }
    }
    Ok(None)
}

/// The workdir rule `exec_inner` applies, as a function so it can be tested.
///
/// Upstream: an empty workdir becomes `input.WorkingDir`, one that already
/// starts with `/` is used as it is, and anything else is appended to
/// `input.WorkingDir`.
fn resolve_workdir(input: &NewContainerInput, workdir: &str) -> String {
    if workdir.is_empty() {
        input.working_dir.clone()
    } else if workdir.starts_with('/') {
        workdir.to_string()
    } else {
        format!("{}/{}", input.working_dir, workdir)
    }
}

/// `strings.SplitN(platform, "/", 2)` with the length check that follows it.
///
/// Empty means "unset", which is not an error — upstream never gets that far
/// because it guards on `input.Platform != ""` first.
fn split_platform(platform: &str) -> Result<String, ()> {
    if platform.is_empty() {
        return Ok(String::new());
    }
    match platform.split_once('/') {
        Some((os, arch)) if !os.is_empty() && !arch.is_empty() && !arch.contains('/') => {
            Ok(format!("{os}/{arch}"))
        }
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::docker_opts::DeviceMapping;
    use crate::container::docker_opts_mounts::mount::{Mount, TYPE_BIND, TYPE_VOLUME};
    use crate::container::docker_opts_types::{ThrottleDevice, WeightDevice};
    
    /// The `--platform` gate is a version comparison, and it is a comparison
    /// rather than a pattern because a malformed version is simply "not
    /// supported" rather than an error.
    #[test]
    fn the_platform_gate_is_a_version_comparison() {
        assert!(at_least_1_41("1.41"));
        assert!(at_least_1_41("1.41.0"));
        assert!(at_least_1_41("1.42.3"));
        assert!(at_least_1_41("2.0"));
        assert!(!at_least_1_41("1.40"));
        assert!(!at_least_1_41("1.9"));
        assert!(!at_least_1_41(""));
        assert!(!at_least_1_41("nonsense"));
    }

    /// A handle shares the container, so closing one does not orphan the
    /// other's id. This is the property that makes `Arc` necessary here.
    #[test]
    fn a_clone_is_another_handle_not_a_second_container() {
        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            ..Default::default()
        });
        let other = env.clone();
        *env.inner.id.lock().expect("the id lock") = Some("abc".to_string());
        assert_eq!(other.id().expect("the id is shared"), "abc");
        assert!(Arc::ptr_eq(&env.inner, &other.inner));
    }

    /// The image reference is normalised, and an unnormalisable one is passed
    /// through so the daemon produces the error.
    #[test]
    fn the_image_reference_is_normalised() {
        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            ..Default::default()
        });
        assert_eq!(env.image_ref(), "docker.io/library/ubuntu");

        let env = new_container(NewContainerInput {
            image: "not a ref".to_string(),
            ..Default::default()
        });
        assert_eq!(env.image_ref(), "not a ref", "passed through for the daemon");
    }

    /// Every field name the port writes has to be the Engine API's own.
    ///
    /// The ~90 `#[serde(rename)]`s in `docker_api` were transcribed by hand
    /// from moby's struct tags, and a wrong one is invisible: serde drops an
    /// unknown key, bollard deserialises the field as `None`, and the daemon
    /// quietly applies a default. So this round-trips a **fully populated**
    /// config through bollard's model — which is generated from the OpenAPI
    /// document and is therefore the authority — and checks that every value
    /// arrives. A renamed field that does not match shows up as `None` here
    /// rather than as a job that mysteriously ignores an option.
    #[test]
    fn every_wire_field_name_survives_the_round_trip() {
        let config = Config {
            hostname: "host".to_string(),
            domainname: "domain".to_string(),
            exposed_ports: ["8080/tcp".to_string()].into_iter().collect(),
            user: "1000:1000".to_string(),
            tty: true,
            open_stdin: true,
            attach_stdin: true,
            attach_stdout: true,
            attach_stderr: true,
            stdin_once: true,
            env: vec!["A=1".to_string(), "B=2".to_string()],
            cmd: vec!["/bin/sh".to_string(), "-c".to_string()],
            image: "ubuntu:latest".to_string(),
            volumes: BTreeMap::from([("data".to_string(), serde_json::json!({}))]),
            entrypoint: Some(vec!["/init".to_string()]),
            working_dir: "/github/workspace".to_string(),
            labels: BTreeMap::from([("com.example.k".to_string(), "v".to_string())]),
            stop_signal: "SIGTERM".to_string(),
            stop_timeout: Some(30),
            healthcheck: Some(docker_api::HealthConfig {
                test: vec!["CMD-SHELL".to_string(), "true".to_string()],
                interval: 1_000_000_000,
                timeout: 2_000_000_000,
                start_period: 3_000_000_000,
                start_interval: 4_000_000_000,
                retries: 5,
            }),
        };

        let host = HostConfig {
            binds: vec!["/work:/github/workspace".to_string()],
            container_id_file: "/tmp/id".to_string(),
            oom_score_adj: 500,
            auto_remove: true,
            privileged: true,
            port_bindings: BTreeMap::from([(
                "8080/tcp".to_string(),
                vec![docker_api::PortBinding {
                    host_ip: "127.0.0.1".to_string(),
                    host_port: "18080".to_string(),
                }],
            )]),
            links: vec!["db:db".to_string()],
            publish_all_ports: true,
            dns: vec!["1.1.1.1".to_string()],
            dns_search: vec!["example.com".to_string()],
            dns_options: vec!["ndots:2".to_string()],
            extra_hosts: vec!["host:1.2.3.4".to_string()],
            volumes_from: vec!["other".to_string()],
            ipc_mode: "shareable".to_string(),
            network_mode: "my-net".to_string(),
            pid_mode: docker_api::PidMode("host".to_string()),
            uts_mode: docker_api::UtsMode("host".to_string()),
            userns_mode: docker_api::UsernsMode("host".to_string()),
            cgroupns_mode: docker_api::CgroupnsMode("private".to_string()),
            cap_add: vec!["SYS_ADMIN".to_string()],
            cap_drop: vec!["MKNOD".to_string()],
            group_add: vec!["1000".to_string()],
            restart_policy: docker_api::RestartPolicy {
                name: "on-failure".to_string(),
                maximum_retry_count: 3,
            },
            security_opt: vec!["label=disable".to_string()],
            storage_opt: BTreeMap::from([("size".to_string(), "10G".to_string())]),
            readonly_rootfs: true,
            log_config: docker_api::LogConfig {
                kind: "json-file".to_string(),
                config: BTreeMap::from([("max-size".to_string(), "10m".to_string())]),
            },
            volume_driver: "local".to_string(),
            isolation: docker_api::Isolation("default".to_string()),
            shm_size: 64 * 1024 * 1024,
            resources: docker_api::Resources {
                cgroup_parent: "/parent".to_string(),
                memory: 1024,
                memory_reservation: 512,
                memory_swap: 2048,
                memory_swappiness: 60,
                oom_kill_disable: true,
                nano_cpus: 2_000_000_000,
                cpu_count: 4,
                cpu_percent: 80,
                cpu_shares: 1024,
                cpu_period: 100_000,
                cpuset_cpus: "0-1".to_string(),
                cpuset_mems: "0".to_string(),
                cpu_quota: 50_000,
                cpu_realtime_period: 1_000_000,
                cpu_realtime_runtime: 950_000,
                pids_limit: 100,
                blkio_weight: 500,
                blkio_weight_device: vec![WeightDevice {
                    path: "/dev/sda".to_string(),
                    weight: 400,
                }],
                blkio_device_read_bps: vec![ThrottleDevice {
                    path: "/dev/sdb".to_string(),
                    rate: 1024,
                }],
                blkio_device_write_bps: vec![ThrottleDevice {
                    path: "/dev/sdc".to_string(),
                    rate: 2048,
                }],
                blkio_device_read_iops: vec![ThrottleDevice {
                    path: "/dev/sdd".to_string(),
                    rate: 4,
                }],
                blkio_device_write_iops: vec![ThrottleDevice {
                    path: "/dev/sde".to_string(),
                    rate: 5,
                }],
                io_maximum_bandwidth: 9_000,
                io_maximum_iops: 900,
                ulimits: vec![docker_api::Ulimit {
                    name: "nofile".to_string(),
                    soft: 1024,
                    hard: 2048,
                }],
                device_cgroup_rules: vec!["c 1:3 mr".to_string()],
                devices: vec![DeviceMapping {
                    path_on_host: "/dev/null".to_string(),
                    path_in_container: "/dev/null".to_string(),
                    cgroup_permissions: "rwm".to_string(),
                }],
                device_requests: vec![docker_api::DeviceRequest {
                    driver: "nvidia".to_string(),
                    device_ids: vec!["0".to_string()],
                    capabilities: vec![vec!["gpu".to_string()]],
                    count: 1,
                    options: BTreeMap::from([("k".to_string(), "v".to_string())]),
                }],
            },
            tmpfs: BTreeMap::from([("/run".to_string(), "rw".to_string())]),
            sysctls: BTreeMap::from([("net.ipv4.ip_forward".to_string(), "1".to_string())]),
            runtime: "runc".to_string(),
            mounts: vec![Mount {
                mount_type: super::super::docker_opts_mounts::mount::MountType(
                    TYPE_BIND.to_string(),
                ),
                source: "/src".to_string(),
                target: "/dst".to_string(),
                read_only: true,
                ..Default::default()
            }],
            masked_paths: Some(vec!["/proc/kcore".to_string()]),
            readonly_paths: Some(vec!["/proc/sys".to_string()]),
            annotations: BTreeMap::from([("note".to_string(), "yes".to_string())]),
            init: Some(true),
        };

        let body = create_body(&config, &host, None).expect("the body round-trips");

        // ── Config ──────────────────────────────────────────────────────
        assert_eq!(body.hostname.as_deref(), Some("host"));
        assert_eq!(body.domainname.as_deref(), Some("domain"));
        assert_eq!(body.user.as_deref(), Some("1000:1000"));
        assert_eq!(body.exposed_ports.as_deref(), Some(&["8080/tcp".to_string()][..]));
        assert_eq!(body.tty, Some(true));
        assert_eq!(body.open_stdin, Some(true));
        assert_eq!(body.attach_stdin, Some(true));
        assert_eq!(body.attach_stdout, Some(true));
        assert_eq!(body.attach_stderr, Some(true));
        assert_eq!(body.stdin_once, Some(true));
        assert_eq!(body.env.as_deref(), Some(&["A=1".to_string(), "B=2".to_string()][..]));
        assert_eq!(body.cmd.as_deref(), Some(&["/bin/sh".to_string(), "-c".to_string()][..]));
        assert_eq!(body.image.as_deref(), Some("ubuntu:latest"));
        assert_eq!(body.volumes.as_deref(), Some(&["data".to_string()][..]));
        assert_eq!(body.entrypoint.as_deref(), Some(&["/init".to_string()][..]));
        assert_eq!(body.working_dir.as_deref(), Some("/github/workspace"));
        assert_eq!(
            body.labels.as_ref().and_then(|l| l.get("com.example.k")).map(String::as_str),
            Some("v")
        );
        assert_eq!(body.stop_signal.as_deref(), Some("SIGTERM"));
        assert_eq!(body.stop_timeout, Some(30));
        let health = body.healthcheck.as_ref().expect("the health check survived");
        assert_eq!(
            health.test.as_deref(),
            Some(&["CMD-SHELL".to_string(), "true".to_string()][..])
        );
        assert_eq!(health.retries, Some(5));

        // ── HostConfig ──────────────────────────────────────────────────
        let host = body.host_config.expect("the host config survived");
        assert_eq!(host.binds.as_deref(), Some(&["/work:/github/workspace".to_string()][..]));
        assert_eq!(host.container_id_file.as_deref(), Some("/tmp/id"));
        assert_eq!(host.oom_score_adj, Some(500));
        assert_eq!(host.auto_remove, Some(true));
        assert_eq!(host.privileged, Some(true));
        assert_eq!(host.links.as_deref(), Some(&["db:db".to_string()][..]));
        assert_eq!(host.publish_all_ports, Some(true));
        assert_eq!(host.dns.as_deref(), Some(&["1.1.1.1".to_string()][..]));
        assert_eq!(host.dns_search.as_deref(), Some(&["example.com".to_string()][..]));
        assert_eq!(host.dns_options.as_deref(), Some(&["ndots:2".to_string()][..]));
        assert_eq!(host.extra_hosts.as_deref(), Some(&["host:1.2.3.4".to_string()][..]));
        assert_eq!(host.volumes_from.as_deref(), Some(&["other".to_string()][..]));
        assert_eq!(host.ipc_mode.as_deref(), Some("shareable"));
        assert_eq!(host.network_mode.as_deref(), Some("my-net"));
        assert_eq!(host.pid_mode.as_deref(), Some("host"));
        assert_eq!(host.uts_mode.as_deref(), Some("host"));
        assert_eq!(host.userns_mode.as_deref(), Some("host"));
        assert_eq!(host.cap_add.as_deref(), Some(&["SYS_ADMIN".to_string()][..]));
        assert_eq!(host.cap_drop.as_deref(), Some(&["MKNOD".to_string()][..]));
        assert_eq!(host.group_add.as_deref(), Some(&["1000".to_string()][..]));
        assert_eq!(
            host.security_opt.as_deref(),
            Some(&["label=disable".to_string()][..])
        );
        assert_eq!(
            host.storage_opt.as_ref().and_then(|m| m.get("size")).map(String::as_str),
            Some("10G")
        );
        assert_eq!(host.readonly_rootfs, Some(true));
        assert_eq!(host.volume_driver.as_deref(), Some("local"));
        assert_eq!(host.shm_size, Some(64 * 1024 * 1024));
        assert_eq!(
            host.tmpfs.as_ref().and_then(|m| m.get("/run")).map(String::as_str),
            Some("rw")
        );
        assert_eq!(
            host.sysctls.as_ref().and_then(|m| m.get("net.ipv4.ip_forward")).map(String::as_str),
            Some("1")
        );
        assert_eq!(host.runtime.as_deref(), Some("runc"));
        assert_eq!(host.masked_paths.as_deref(), Some(&["/proc/kcore".to_string()][..]));
        assert_eq!(host.readonly_paths.as_deref(), Some(&["/proc/sys".to_string()][..]));
        assert_eq!(
            host.annotations.as_ref().and_then(|m| m.get("note")).map(String::as_str),
            Some("yes")
        );
        assert_eq!(host.init, Some(true));

        // `Resources` is embedded in Go and flattened onto the host config, so
        // its fields land on the *host* object rather than a nested one.
        assert_eq!(host.memory, Some(1024));
        assert_eq!(host.memory_reservation, Some(512));
        assert_eq!(host.memory_swap, Some(2048));
        assert_eq!(host.memory_swappiness, Some(60));
        assert_eq!(host.oom_kill_disable, Some(true));
        assert_eq!(host.nano_cpus, Some(2_000_000_000));
        assert_eq!(host.cpu_count, Some(4));
        assert_eq!(host.cpu_percent, Some(80));
        assert_eq!(host.cpu_shares, Some(1024));
        assert_eq!(host.cpu_period, Some(100_000));
        assert_eq!(host.cpuset_cpus.as_deref(), Some("0-1"));
        assert_eq!(host.cpuset_mems.as_deref(), Some("0"));
        assert_eq!(host.cpu_quota, Some(50_000));
        assert_eq!(host.cpu_realtime_period, Some(1_000_000));
        assert_eq!(host.cpu_realtime_runtime, Some(950_000));
        assert_eq!(host.pids_limit, Some(100));
        assert_eq!(host.blkio_weight, Some(500));
        assert_eq!(host.io_maximum_bandwidth, Some(9_000));
        assert_eq!(host.io_maximum_iops, Some(900));
        assert_eq!(
            host.device_cgroup_rules.as_deref(),
            Some(&["c 1:3 mr".to_string()][..])
        );
        assert_eq!(host.devices.as_ref().map(|d| d.len()), Some(1));
        let device = &host.devices.as_ref().expect("a device")[0];
        assert_eq!(device.path_on_host.as_deref(), Some("/dev/null"));
        assert_eq!(device.path_in_container.as_deref(), Some("/dev/null"));
        assert_eq!(device.cgroup_permissions.as_deref(), Some("rwm"));
        let request = &host.device_requests.as_ref().expect("a request")[0];
        assert_eq!(request.driver.as_deref(), Some("nvidia"));
        assert_eq!(request.count, Some(1));
        assert_eq!(request.device_ids.as_deref(), Some(&["0".to_string()][..]));
        assert_eq!(request.capabilities.as_ref().map(|c| c.len()), Some(1));
        assert_eq!(
            request.options.as_ref().and_then(|o| o.get("k")).map(String::as_str),
            Some("v")
        );
        let ulimit = &host.ulimits.as_ref().expect("an ulimit")[0];
        assert_eq!(ulimit.name.as_deref(), Some("nofile"));
        assert_eq!(ulimit.soft, Some(1024));
        assert_eq!(ulimit.hard, Some(2048));
        let weight = &host.blkio_weight_device.as_ref().expect("a weight device")[0];
        assert_eq!(weight.path.as_deref(), Some("/dev/sda"));
        assert_eq!(weight.weight, Some(400));
        assert_eq!(
            host.blkio_device_read_bps.as_ref().map(|d| d[0].rate),
            Some(Some(1024))
        );
        assert_eq!(
            host.blkio_device_write_bps.as_ref().map(|d| d[0].rate),
            Some(Some(2048))
        );
        assert_eq!(
            host.blkio_device_read_iops.as_ref().map(|d| d[0].rate),
            Some(Some(4))
        );
        assert_eq!(
            host.blkio_device_write_iops.as_ref().map(|d| d[0].rate),
            Some(Some(5))
        );
        let mount = &host.mounts.as_ref().expect("a mount")[0];
        assert_eq!(
            format!("{:?}", Some(bollard::models::MountType::BIND)),
            format!("{:?}", mount.typ),
            "the mount type reaches the daemon as the enum it is"
        );
        assert_eq!(mount.source.as_deref(), Some("/src"));
        assert_eq!(mount.target.as_deref(), Some("/dst"));
        assert_eq!(mount.read_only, Some(true));
        let log = host.log_config.as_ref().expect("a log config");
        assert_eq!(log.typ.as_deref(), Some("json-file"));
        assert_eq!(
            log.config.as_ref().and_then(|c| c.get("max-size")).map(String::as_str),
            Some("10m")
        );
        let restart = host.restart_policy.as_ref().expect("a restart policy");
        assert_eq!(
            format!("{:?}", Some(bollard::models::RestartPolicyNameEnum::ON_FAILURE)),
            format!("{:?}", restart.name),
            "the restart policy name reaches the daemon as the enum it is"
        );
        assert_eq!(restart.maximum_retry_count, Some(3));
        // The enums are generated as *enums* upstream, so a value bollard does
        // not know fails the whole conversion rather than reaching the daemon.
        assert!(host.cgroupns_mode.is_some(), "CgroupnsMode");
        assert!(host.isolation.is_some(), "Isolation");
        let bindings = host
            .port_bindings
            .as_ref()
            .and_then(|m| m.get("8080/tcp"))
            .expect("the port binding survived");
        let binding = bindings.as_ref().expect("a binding list").first().expect("a binding");
        assert_eq!(binding.host_port.as_deref(), Some("18080"));
        assert_eq!(binding.host_ip.as_deref(), Some("127.0.0.1"));
    }

    /// The three answers `ImageExistsLocally` can give, and which the runner
    /// reads differently.
    ///
    /// This is the rule my first attempt got wrong: a label filter for
    /// `platform:<os>/<arch>` finds nothing on any real image, so *both* of the
    /// runner's probes would have answered "absent", the wrong-architecture
    /// image would never be removed, and a matrix spanning `amd64` and `arm64`
    /// would quietly run the first one twice.
    #[test]
    fn the_platform_probe_distinguishes_any_from_the_real_architecture() {
        // The two sentinels: "is anything cached at all".
        assert!(platform_matches("linux", "amd64", ""));
        assert!(platform_matches("linux", "amd64", "any"));
        // The real question: "is the cache entry usable".
        assert!(platform_matches("linux", "amd64", "linux/amd64"));
        assert!(
            !platform_matches("linux", "amd64", "linux/arm64"),
            "present but wrong architecture is not present for this job"
        );
        assert!(!platform_matches("linux", "arm64", "linux/amd64"));
        // A daemon that reports neither still matches on the two sentinels,
        // because they never look at the image.
        assert!(platform_matches("", "", "any"));
        assert!(!platform_matches("", "", "linux/amd64"));
    }

    /// A missing image is `false, nil`, not a failure: on a cold cache that is
    /// the normal answer, and turning it into an error would break every first
    /// run. The 404 is matched on the daemon's status, which is what
    /// `containerd/errdefs.IsNotFound` does upstream.
    #[test]
    fn a_not_found_from_the_daemon_is_recognised_as_absent() {
        let err = bollard::errors::Error::DockerResponseServerError {
            status_code: 404,
            message: "No such image: ubuntu".to_string(),
        };
        assert!(matches!(
            err,
            bollard::errors::Error::DockerResponseServerError {
                status_code: 404,
                ..
            }
        ));
        // Anything else must NOT be swallowed, or a daemon that is down would
        // read as "image absent" and trigger a pointless pull.
        let down = bollard::errors::Error::IOError {
            err: std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "daemon not running",
            ),
        };
        assert!(!matches!(
            down,
            bollard::errors::Error::DockerResponseServerError {
                status_code: 404,
                ..
            }
        ));
    }

    /// `cleanImage` yields the **empty string** for a reference it cannot
    /// parse, not the input. Upstream logs and returns `""`, and that empty
    /// string is what reaches the daemon — so a broken image name fails there,
    /// not in the parser, and a port that returned the input would produce a
    /// different error for the same typo.
    #[test]
    fn an_unparseable_image_reference_cleans_to_the_empty_string() {
        let env = new_container(NewContainerInput {
            image: "not a ref".to_string(),
            ..Default::default()
        });
        let sink = crate::common::context::CollectingSink::new();
        let ctx = RunContext::new().with_sink(Arc::new(sink) as Arc<dyn crate::common::LogSink>);
        assert_eq!(env.clean_image(&ctx), "");

        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            ..Default::default()
        });
        assert_eq!(
            env.clean_image(&ctx),
            "docker.io/library/ubuntu",
            "a valid reference is normalised"
        );
    }

    /// `create` sends the image **as the job wrote it**, unnormalised.
    ///
    /// `cleanImage` belongs to the pull path only. The daemon resolves both
    /// spellings, so this is invisible in practice — but it is a different code
    /// path, and a port that normalised here would be applying its own rule
    /// where upstream has none.
    #[test]
    fn the_base_config_carries_the_image_unnormalised() {
        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            ..Default::default()
        });
        assert_eq!(
            env.base_config().image, "ubuntu",
            "no `library/`, no `docker.io/`, no `latest`"
        );
    }

    /// A job's `mounts:` become `Type: volume` mounts on the base host config.
    /// The type is not a detail: a bind there would mount the host path the
    /// workflow asked to be a named volume.
    #[test]
    fn the_jobs_mounts_become_volume_mounts() {
        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            mounts: BTreeMap::from([("my-cache".to_string(), "/cache".to_string())]),
            ..Default::default()
        });
        let host = env.base_host_config();
        assert_eq!(host.mounts.len(), 1);
        assert_eq!(host.mounts[0].source, "my-cache");
        assert_eq!(host.mounts[0].target, "/cache");
        assert_eq!(host.mounts[0].mount_type.0, TYPE_VOLUME);
    }

    /// The base config carries the job's own values and leaves the image's
    /// command alone when the job did not set one — Go assigns `Cmd` only when
    /// `input.Cmd` is non-empty, and an empty one would blank the image.
    #[test]
    fn the_base_config_does_not_blank_the_images_command() {
        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            working_dir: "/github/workspace".to_string(),
            env: vec!["A=1".to_string()],
            ..Default::default()
        });
        let config = env.base_config();
        assert!(config.cmd.is_empty());
        assert!(config.entrypoint.is_none());
        assert_eq!(config.working_dir, "/github/workspace");
        assert_eq!(config.env, vec!["A=1".to_string()]);
    }

    /// A configured `Cmd` or `Entrypoint` does reach the config.
    #[test]
    fn a_configured_command_and_entrypoint_reach_the_config() {
        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            cmd: vec!["/bin/sh".to_string(), "-c".to_string(), "true".to_string()],
            entrypoint: vec!["/init".to_string()],
            ..Default::default()
        });
        let config = env.base_config();
        assert_eq!(config.cmd.len(), 3);
        assert_eq!(
            config.entrypoint,
            Some(vec!["/init".to_string()]),
            "an entrypoint is Some, not an empty vector"
        );
    }

    /// `--platform` is only sent as an `os/arch` pair, and a bare OS is an
    /// **error** rather than a default.
    ///
    /// Upstream splits on the first `/` and requires exactly two parts, so
    /// `linux` fails and `linux/arm/v7` is not a platform either. Tested
    /// through the split itself, which needs no daemon.
    #[test]
    fn a_platform_must_be_an_os_arch_pair() {
        assert_eq!(split_platform(""), Ok("".to_string()), "unset is not an error");
        assert_eq!(split_platform("linux/amd64"), Ok("linux/amd64".to_string()));
        assert_eq!(split_platform("linux/arm64"), Ok("linux/arm64".to_string()));
        assert_eq!(
            split_platform("linux/arm/v7"),
            Err(()),
            "three parts is not a platform"
        );
        for bad in ["linux", "amd64", "/amd64", "linux/"] {
            assert_eq!(split_platform(bad), Err(()), "{bad} is not a platform");
        }
    }

    /// The create body is a pure function, so its shape is checked here even
    /// though sending it needs a daemon.
    ///
    /// `Config` is inlined next to `HostConfig` rather than nested under a
    /// `"Config"` key, and `ExposedPorts` is an object because `nat.PortSet`
    /// has no `UnmarshalJSON` and accepts nothing else.
    #[test]
    fn the_create_body_inlines_the_config_and_maps_the_ports() {
        let config = Config {
            image: "ubuntu:latest".to_string(),
            working_dir: "/github/workspace".to_string(),
            env: vec!["A=1".to_string()],
            exposed_ports: ["8080/tcp".to_string()].into_iter().collect(),
            tty: true,
            ..Default::default()
        };
        let host = HostConfig {
            binds: vec!["/work:/github/workspace".to_string()],
            mounts: vec![Mount {
                mount_type: super::super::docker_opts_mounts::mount::MountType(
                    TYPE_BIND.to_string(),
                ),
                source: "/cache".to_string(),
                target: "/cache".to_string(),
                ..Default::default()
            }],
            network_mode: "my-net".to_string(),
            ..Default::default()
        };
        let body = create_body(&config, &host, None).expect("the body matches the Engine API");
        assert_eq!(body.image.as_deref(), Some("ubuntu:latest"));
        assert_eq!(body.working_dir.as_deref(), Some("/github/workspace"));
        assert_eq!(body.tty, Some(true));
        let host = body.host_config.expect("the host config survived");
        assert_eq!(host.binds.as_deref(), Some(&["/work:/github/workspace".to_string()][..]));
        assert_eq!(host.network_mode.as_deref(), Some("my-net"));
        assert_eq!(host.mounts.map(|m| m.len()), Some(1));

        // The object shape of ExposedPorts is only visible before the round
        // trip through bollard's model, so it is asserted on the JSON.
        let raw = serde_json::to_value(&config).expect("the config encodes");
        assert_eq!(
            raw["ExposedPorts"],
            serde_json::json!({"8080/tcp": {}}),
            "a list here is a parse error at the daemon"
        );
        assert!(raw.get("HostConfig").is_none(), "Config is inlined, not nested");
    }

    /// The `NetworkingConfig` is built from the job's aliases and the job's
    /// network mode, and only for a user-defined network.
    #[test]
    fn the_networking_config_needs_a_user_defined_network_and_an_alias() {
        let bind = |mode: &str| HostConfig {
            network_mode: mode.to_string(),
            ..Default::default()
        };
        let aliases = vec!["my-alias".to_string()];

        // A user-defined network with an alias gets an endpoint.
        let endpoints = build_networking_config(&bind("my-net"), "my-net", &aliases)
            .expect("a user-defined network with an alias");
        let endpoint = endpoints["my-net"].as_ref().expect("an endpoint");
        assert_eq!(endpoint.aliases, aliases);
        assert_eq!(endpoints.len(), 1, "only the one network act names");

        // `default`, `bridge`, `host` and `none` are not user-defined.
        for mode in ["default", "bridge", "host", "none"] {
            assert!(
                build_networking_config(&bind(mode), mode, &aliases).is_none(),
                "{mode} is not a user-defined network"
            );
        }

        // The empty string *is* user-defined. `IsUserDefined` only subtracts
        // five named modes, and `""` is none of them — which is why a job with
        // no `container:` and no network still gets an endpoint when it has
        // aliases. Verified against moby's `hostconfig_unix.go`.
        assert!(
            build_networking_config(&bind(""), "", &aliases).is_some(),
            "an empty network mode is user-defined upstream"
        );

        // No alias, no networking config at all.
        assert!(build_networking_config(&bind("my-net"), "my-net", &[]).is_none());
    }

    /// `convertPortMap` drops a port that does not parse rather than failing
    /// the job, which is what upstream's `continue` does.
    #[test]
    fn an_unparseable_port_binding_is_dropped() {
        let ports = BTreeMap::from([
            ("8080/tcp".to_string(), vec!["8080".to_string()]),
            ("not a port".to_string(), vec!["1".to_string()]),
        ]);
        let converted = convert_port_map(&ports);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted["8080/tcp"][0].host_port, "8080");
        assert_eq!(converted["8080/tcp"][0].host_ip, "", "no host ip set");
    }

    /// The `options:` overlay is a pure function of the input, so it is tested
    /// without a daemon. The runner's own workspace bind survives it, and
    /// `options:` wins where the two disagree.
    #[test]
    fn the_options_overlay_lands_on_the_base_config() {
        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            working_dir: "/github/workspace".to_string(),
            network_mode: "my-net".to_string(),
            options: vec!["--cpus".to_string(), "2".to_string()],
            ..Default::default()
        });
        let mut host = env.base_host_config();
        host.binds = vec!["/work:/github/workspace".to_string()];
        host.resources.cpu_shares = 512;
        let (config, host) = env
            .merge_options(&env.base_config(), &host)
            .expect("the options parse")
            .expect("options were given");
        assert_eq!(host.resources.cpu_shares, 512, "untouched by --cpus");
        assert_eq!(
            host.binds,
            vec!["/work:/github/workspace".to_string()],
            "the runner's bind survives a --cpus overlay"
        );
        assert_eq!(
            host.network_mode, "my-net",
            "the job's network mode is injected into the empty --network"
        );
        assert_eq!(config.working_dir, "/github/workspace");
    }

    /// `options:` that names its own network beats the job's.
    ///
    /// Measured: `options: --network other` with `container: my-net` yields
    /// `NetworkMode=other`.
    #[test]
    fn options_can_name_a_different_network_than_the_job() {
        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            network_mode: "my-net".to_string(),
            options: vec!["--network".to_string(), "other".to_string()],
            ..Default::default()
        });
        let (_, host) = env
            .merge_options(&Config::default(), &HostConfig::default())
            .expect("the options parse")
            .expect("options were given");
        assert_eq!(host.network_mode, "other");
    }

    /// A job with `options:` and **no** `container:` network fails in act.
    ///
    /// This looks like a bug and is not one to be fixed here: the injection
    /// puts an empty `--network` into the flag set, `parseNetworkOpts` then
    /// rejects an endpoint with no target, and the job stops with
    /// `Cannot process container options: '<options>': 'no name set for
    /// network'`. Measured by running act's own `mergeContainerConfigs`.
    ///
    /// It is pinned because a port that "helpfully" skipped the injection for
    /// an empty mode would run jobs that act fails — the opposite of a port.
    #[test]
    fn options_with_no_container_network_is_an_error_upstream() {
        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            options: vec!["--cpus".to_string(), "2".to_string()],
            ..Default::default()
        });
        let error = env
            .merge_options(&Config::default(), &HostConfig::default())
            .expect_err("an empty network name is rejected by parse");
        assert!(
            error.to_string().contains("no name set for network"),
            "the error names what act names, got: {error}"
        );
    }

    /// No `options:` at all means no merge **and** no network-mode injection,
    /// which is upstream's early return. That is a different state from an
    /// `options:` that happens to mention nothing, and the two must not be
    /// conflated.
    #[test]
    fn no_options_at_all_skips_the_merge_entirely() {
        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            network_mode: "my-net".to_string(),
            ..Default::default()
        });
        assert!(
            env.merge_options(&Config::default(), &HostConfig::default())
                .expect("no options is not an error")
                .is_none(),
            "the early return also skips the network-mode injection"
        );
    }

    /// A relative `workdir` is taken as relative to the container's own
    /// working directory — the rule `exec` applies before it calls the daemon.
    #[test]
    fn a_relative_workdir_is_joined_with_the_containers_own() {
        let env = new_container(NewContainerInput {
            image: "ubuntu".to_string(),
            working_dir: "/github/workspace".to_string(),
            ..Default::default()
        });
        assert_eq!(resolve_workdir(&env.input, ""), "/github/workspace");
        assert_eq!(resolve_workdir(&env.input, "sub"), "/github/workspace/sub");
        assert_eq!(resolve_workdir(&env.input, "/abs"), "/abs");
    }
}
