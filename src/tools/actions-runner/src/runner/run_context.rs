//! The state every step reads and writes: one job's execution.
//!
//! Port of act's `pkg/runner/run_context.go` (1,177 lines), **partly** — this
//! file is the pure core, and the container lifecycle that surrounds it.
//!
//! # What is here and what is not
//!
//! [`RunContext`] and the functions that need nothing but it and the model:
//! [`merge_maps`], [`create_container_name`], [`trim_to_len`],
//! [`get_docker_daemon_socket_mount_path`], [`nested_map_lookup`],
//! [`RunContext::get_job_context`], [`RunContext::get_steps_context`],
//! [`RunContext::get_env`], [`RunContext::job_container_name`],
//! [`RunContext::network_name`], [`set_action_runtime_vars`],
//! [`RunContext::get_github_context`], and [`RunContext::get_binds_and_mounts`].
//!
//! The methods are spelled with their `RunContext::` prefix because a bare
//! name does not resolve from a module-level doc comment, and rustdoc says so
//! instead of quietly rendering plain text.
//!
//! `GetBindsAndMounts` is the one executor that made the cut, because it is the
//! only one that is pure: it decides what to mount and starts nothing. The rest
//! of the lifecycle — `startJobContainer`, `startServiceContainers`,
//! `execJobContainer`, `GetServiceBindsAndMounts`, `InitializeNodeTool`,
//! `ApplyExtraPath` — needs the `Config` fields the CLI fills in and the
//! container trait's runtime half, and they are what `step.go` and the five step
//! types are written against.
//!
//! `getGithubContext` came with `pkg/model/github_context.go` and the git-backed
//! `SetRepositoryAndOwner` / `SetRef` / `SetSha`; those live in
//! [`crate::model`] and are reached through [`GitLookups`], so `model` does not
//! depend back on `runner`.
//!
//! # The `Env` / `GlobalEnv` split is load-bearing
//!
//! act keeps two maps because `Env` is *dirtied* by a step's own environment
//! while `GlobalEnv` is what the **next** step sees. `set-env` writes both;
//! `add-path` writes only the path. A port that collapsed them would leak one
//! step's environment into another.
//!
//! # Container names are hashed, not truncated
//!
//! Docker names allow `[a-zA-Z0-9_.-]` but act is stricter: it replaces every
//! non-alphanumeric character with `-`, collapses `--`, trims to 63 characters
//! to leave room for a SHA-256, and **appends the hash of the full name**. Two
//! long job names that share a 63-character prefix therefore still get distinct
//! containers. The length limit is the reason a *long* workflow name works at
//! all, and the hash is the reason two *similarly named* ones do not collide.

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;
// `Arc`, not `Rc`: `ExecutionsEnvironment` is `Send + Sync` because the
// executor algebra stores closures that are, and a service container has to be
// usable from one.
use std::sync::Arc;

use serde_json::Value as JsonValue;

pub use crate::model::nested_map_lookup;
use crate::model::GitLookups;
use crate::model::{GithubContext, JobContext, Run, StepResult, StepStatus};

/// A rename of one step's output onto another step's, so that a `set-output` in
/// a reusable workflow lands on the step that consumed it.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MappableOutput {
    /// The step the command was written in.
    pub step_id: String,
    /// The output name the command was written with.
    pub output_name: String,
}

impl MappableOutput {
    /// A mapping from `(step_id, output_name)`.
    pub fn new(step_id: impl Into<String>, output_name: impl Into<String>) -> Self {
        Self {
            step_id: step_id.into(),
            output_name: output_name.into(),
        }
    }
}

impl fmt::Display for MappableOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.step_id, self.output_name)
    }
}

/// A service container, boxed so [`RunContext`] can stay `Debug`.
///
/// `ExecutionsEnvironment` is a trait without a `Debug` bound, so storing it
/// directly would force `RunContext` to give up `Debug` — and it is printed in
/// test failures and in a couple of log lines. A newtype costs one wrapper and
/// keeps the bound; the placeholder is honest, since a container's identity is
/// its name, which lives on the config side.
#[derive(Clone)]
pub struct ServiceContainer(Arc<dyn crate::container::ExecutionsEnvironment>);

impl ServiceContainer {
    /// Boxes a container.
    pub fn new(
        container: Arc<dyn crate::container::ExecutionsEnvironment>,
    ) -> Self {
        Self(container)
    }

    /// The container itself.
    pub fn inner(&self) -> &Arc<dyn crate::container::ExecutionsEnvironment> {
        &self.0
    }
}

impl std::fmt::Debug for ServiceContainer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ServiceContainer(..)")
    }
}

/// The job's execution state.
#[derive(Debug, Clone, Default)]
pub struct RunContext {
    /// The job's name, as the workflow spelled it.
    pub name: String,
    /// The runner configuration. Not yet ported; the fields the functions in
    /// this module read are inlined on [`RunConfig`].
    pub config: RunConfig,
    /// The matrix combination this run is one cell of.
    pub matrix: BTreeMap<String, JsonValue>,
    /// Which workflow and which job inside it.
    pub run: Option<Run>,
    /// The event payload, as JSON.
    pub event_json: String,
    /// The current step's environment. Dirtied by the step itself; see the
    /// module docs.
    pub env: BTreeMap<String, String>,
    /// What the *next* step will see.
    pub global_env: BTreeMap<String, String>,
    /// `PATH` entries added by `add-path`, most recent first.
    pub extra_path: Vec<String>,
    /// The step currently running.
    pub current_step: String,
    /// What every step that has finished produced.
    pub step_results: BTreeMap<String, StepResult>,
    /// `save-state` values, per step, for its `post` phase.
    pub intra_action_state: BTreeMap<String, BTreeMap<String, String>>,
    /// Output renames, keyed by the written `(step, output)`.
    pub output_mappings: BTreeMap<MappableOutput, MappableOutput>,
    /// The action directory, for `GITHUB_ACTION_PATH`.
    pub action_path: String,
    /// The `caller` of a reusable workflow, if this run is one.
    pub caller: Option<Box<Caller>>,
    /// The paths the job container contributes, when it is a **host**
    /// environment rather than a Docker container.
    ///
    /// Upstream decides with a type switch —
    /// `rc.JobContainer.(*container.HostEnvironment)` — and the two branches
    /// differ enough that it matters which one a caller is on: the host
    /// environment returns early and never looks at `container.volumes` at
    /// all. `None` is the Docker branch, which is also the branch the tests
    /// that never build a container take.
    pub job_container: Option<ContainerPaths>,
    /// The values `add-mask` registered, to be redacted from the log.
    pub masks: Vec<String>,
    /// Whether the job was cancelled.
    pub cancelled: bool,
    // -- the container lifecycle -------------------------------------------
    /// `ServiceContainers`, in the order the job declared them.
    ///
    /// A vector rather than a map because upstream appends in the order Go's map
    /// range happened to produce, and the order decides which container is
    /// reported first when two fail. Sorted by service id here, so it is stable.
    pub service_containers: Vec<ServiceContainer>,
    /// `nodeToolFullPath`, the memoised result of the `process.execPath` probe.
    ///
    /// Memoised upstream and here, for the same reason: it costs a process spawn
    /// inside the container.
    pub node_tool_full_path: String,
    /// The network act created for this job, empty when it created none.
    ///
    /// Data rather than a closure. Upstream stores a `cleanUpJobContainer`
    /// closure capturing the reuse flag and the network name; everything that
    /// closure needs is here, so
    /// [`crate::runner::job_container::stop_job_container`] builds the executor
    /// from it. Same result, and no executor stored inside the context it acts
    /// on.
    pub job_container_network: String,
    /// Whether act created [`Self::job_container_network`] and must remove it.
    pub create_and_delete_network: bool,
}

/// The reusable workflow that called this run, if any.
///
/// Upstream keeps this unexported; the only use is the prefix on
/// [`RunContext::name`], and that has to be reachable from the step types.
#[derive(Debug, Clone, Default)]
pub struct Caller {
    /// The run context of the calling job.
    pub run_context: Rc<RunContext>,
}

/// The configuration the functions in this module read.
///
/// Upstream's `Config` has 43 fields, most of them filled in by the CLI and
/// consumed by the container lifecycle. This is the subset the ported code
/// touches, named for itself rather than pretending to be the whole `Config`.
/// It grows as the lifecycle arrives.
#[derive(Debug, Clone, Default)]
pub struct RunConfig {
    /// The environment for containers. Also the source of most `GITHUB_*`
    /// values, which is why it is read by the context builder.
    pub env: BTreeMap<String, String>,
    /// Secrets, by name.
    pub secrets: BTreeMap<String, String>,
    /// Repository variables, by name.
    pub vars: BTreeMap<String, String>,
    /// The working directory.
    pub workdir: String,
    /// The name of the main branch.
    pub default_branch: String,
    /// The artifact server's address, port and storage path.
    pub artifact_server_addr: String,
    /// The artifact server's port.
    pub artifact_server_port: String,
    /// The artifact server's storage path; empty disables the runtime vars.
    pub artifact_server_path: String,
    /// The Docker daemon socket path.
    pub container_daemon_socket: String,
    /// The network mode for job containers, the value of `--network`.
    pub container_network_mode: String,
    /// Whether the working directory is bind-mounted into the job container
    /// rather than shadowed by a volume.
    pub bind_workdir: bool,
    /// The image for each `runs-on:` label, keyed by the lower-cased label.
    ///
    /// This map is the whole of `-P`: a label with no entry here has no image
    /// and a job asking for it is **skipped**, not failed. That is why a
    /// self-hosted label falls through to the next entry in a list.
    pub platforms: BTreeMap<String, String>,
    /// `container.options:` for jobs that do not set their own.
    pub container_options: String,
    /// Where downloaded actions are cached, when the operator chose a place.
    pub action_cache_dir: String,
    /// The GitHub instance, `github.com` unless set otherwise.
    pub github_instance: String,
    /// The git remote name in the local repository.
    pub remote_name: String,
    /// The user that triggered the event.
    pub actor: String,
    /// The name of the event being run.
    pub event_name: String,
    /// The GitHub token.
    pub token: String,
    // -- the container lifecycle -------------------------------------------
    /// `--privileged`.
    pub privileged: bool,
    /// `--userns`, the user namespace mode.
    pub userns_mode: String,
    /// The image platform to request, `--platform`. Empty means the daemon's
    /// default, which is what a plain `runs-on: ubuntu-latest` wants.
    pub container_architecture: String,
    /// `capAdd` from the job's `container.options:`.
    pub container_cap_add: Vec<String>,
    /// `capDrop` from the job's `container.options:`.
    pub container_cap_drop: Vec<String>,
    /// `--pull`, always fetch the image even when it is present.
    pub force_pull: bool,
    /// Leave the job container and its volumes behind after the run.
    pub reuse_containers: bool,
    /// Log a step's raw output at info level instead of debug.
    ///
    /// A one-line flag with an outsized effect on a CI product: at `false` the
    /// build log holds almost nothing, because every line of compiler output is
    /// `raw_output` and lands in debug.
    pub log_output: bool,
}

impl RunContext {
    /// `AddMask`: a plain append. The value is redacted from the log by the job
    /// logger, which is not ported yet.
    pub fn add_mask(&mut self, mask: impl Into<String>) {
        self.masks.push(mask.into());
    }

    /// `String()`: the name a container and a `needs` entry are built from.
    ///
    /// A reusable workflow is prefixed with the calling job, and that prefix is
    /// **required** rather than cosmetic: it is what makes the container name
    /// unique between a caller and the workflow it calls.
    pub fn display_name(&self) -> String {
        let name = match &self.run {
            Some(run) => format!("{}/{}", run.workflow.name, self.name),
            // Upstream dereferences `rc.Run` unconditionally, so a nil Run is a
            // nil panic there. Reporting no name is the useful behaviour here.
            None => self.name.clone(),
        };
        match &self.caller {
            Some(caller) => format!("{}/{}", caller.run_context.name, name),
            None => name,
        }
    }

    /// `GetEnv`: the environment the job starts from.
    ///
    /// Built lazily and then cached, merging in this order — workflow `env:`,
    /// job `env:`, config `env:` — so the **last** one wins and a value from
    /// `Config` overwrites the workflow's. `ACT=true` is then forced on top, so
    /// a workflow cannot turn off the marker act puts there to identify its own
    /// processes.
    pub fn get_env(&mut self) -> BTreeMap<String, String> {
        if self.env.is_empty() {
            let mut merged = BTreeMap::new();
            if let Some(run) = &self.run {
                merged = merge_maps([
                    run.workflow.env.clone(),
                    run.job().map(|job| job.environment(run.document())).unwrap_or_default(),
                    self.config.env.clone(),
                ]);
            }
            self.env = merged;
        }
        self.env.insert("ACT".to_string(), "true".to_string());
        self.env.clone()
    }

    /// `getJobContext`: `job.status`.
    ///
    /// `"cancelled"` wins over everything, then a single failing step decides
    /// the job, and otherwise it is `"success"`. Note that it reads
    /// `conclusion`, not `outcome` — a `continue-on-error` step has outcome
    /// `failure` and conclusion `success`, and the job still passes.
    pub fn get_job_context(&self) -> JobContext {
        let status = if self.cancelled {
            "cancelled"
        } else if self
            .step_results
            .values()
            .any(|result| result.conclusion == StepStatus::Failure)
        {
            "failure"
        } else {
            "success"
        };
        JobContext {
            status: status.to_string(),
            ..JobContext::default()
        }
    }

    /// `getStepsContext`: the `steps` context, which is the step results.
    pub fn get_steps_context(&self) -> &BTreeMap<String, StepResult> {
        &self.step_results
    }

    /// The id of the job being run, which lives on the [`Run`] and nowhere
    /// else.
    ///
    /// Upstream has no `RunContext.JobID` at all: both readers — the network
    /// name's suffix and `github.job` — take it from `rc.Run.JobID`. A copy on
    /// the run context would be a second source of truth for one value, and the
    /// two would drift the first time a caller set only one of them.
    pub fn job_id(&self) -> String {
        self.run
            .as_ref()
            .map(|run| run.job_id.clone())
            .unwrap_or_default()
    }

    /// `jobContainerName`: the name of this job's container.
    pub fn job_container_name(&self) -> String {
        create_container_name(&["act", &self.display_name()])
    }

    /// `networkName`: the network the job container is put on, and whether act
    /// has to create it.
    ///
    /// A network is only created when the job actually has services — otherwise
    /// `"host"` is used, or whatever `--network` said.
    pub fn network_name(&self) -> (String, bool) {
        let has_services = self
            .run
            .as_ref()
            .and_then(|run| run.job())
            .is_some_and(|job| !job.services.is_empty());
        if has_services {
            return (
                format!("{}-{}-network", self.job_container_name(), self.job_id()),
                true,
            );
        }
        if self.config.container_network_mode.is_empty() {
            return ("host".to_string(), false);
        }
        (self.config.container_network_mode.clone(), false)
    }

    /// `GetBindsAndMounts`: what the job container gets mounted.
    ///
    /// Two lists, and the distinction is Docker's rather than act's: a
    /// **bind** is a host path Docker takes ownership of, a **mount** is a
    /// named volume or an anonymous one. Upstream returns
    /// `([]string, map[string]string)` and the caller feeds each half to a
    /// different `--volume` argument shape.
    ///
    /// # Which side of the fork a `volumes:` entry lands on
    ///
    /// ```go
    /// if !strings.Contains(v, ":") || filepath.IsAbs(v) { bind } else { mount }
    /// ```
    ///
    /// The colon is checked on the **whole** string and `IsAbs` too, so a host
    /// path that itself contains a colon is still a bind. Measured on darwin:
    ///
    /// | `volumes:` entry | contains `:` | `IsAbs` | lands on |
    /// |---|---|---|---|
    /// | `/volume` | no | true | bind |
    /// | `/path/to/file/on/host:/volume` | yes | **true** | bind |
    /// | `volume-id:/volume` | yes | false | mount `volume-id` → `/volume` |
    ///
    /// The middle row is the one that reads wrong: a Docker named volume is
    /// `name:/path`, and a *host* path is absolute, so `IsAbs` on the joined
    /// string is what decides. Reading the colon first and the path second —
    /// which is what the code looks like it does — would mount a host file
    /// instead of binding it, and the file would be empty.
    ///
    /// # The workdir bind's platform modifier, and a collision that never fires
    ///
    /// A bound workdir gets `:delegated` on darwin and `:z` when SELinux is on.
    /// Upstream writes them into one variable with two `if`s and no `else`, so
    /// a host that is *both* would keep only `:z`. No such host exists —
    /// darwin has no SELinux and Linux is not darwin — so the port keeps the
    /// same shape and the collision stays unreachable rather than becoming a
    /// documented difference.
    pub fn get_binds_and_mounts(&self) -> (Vec<String>, BTreeMap<String, String>) {
        let name = self.job_container_name();
        let extensions = crate::container::linux::LinuxContainerEnvironmentExtensions::new();
        let workdir = self.config.workdir.as_str();

        // Upstream writes this default back into `Config`, so a later reader of
        // the same config sees it. `RunConfig` is owned here and the value is
        // only ever read once, so the effective socket is what matters.
        let daemon_socket = if self.config.container_daemon_socket.is_empty() {
            "/var/run/docker.sock"
        } else {
            self.config.container_daemon_socket.as_str()
        };

        let mut binds: Vec<String> = Vec::new();
        let mut mounts: BTreeMap<String, String> = BTreeMap::new();

        // `-` means "do not mount a daemon socket at all", which is how a
        // remote daemon is addressed.
        if daemon_socket != "-" {
            let daemon_path = get_docker_daemon_socket_mount_path(daemon_socket);
            binds.push(format!("{daemon_path}:/var/run/docker.sock"));
        }

        // A host environment has no container to mount anything into; the binds
        // exist for the nested Docker calls a step may make, and the function
        // returns before `volumes:` is looked at.
        if let Some(host) = &self.job_container {
            binds.push(format!(
                "{}:{}",
                host.act_path,
                crate::container::linux::ACT_PATH
            ));
            binds.push(format!(
                "{}:{}",
                host.workdir,
                extensions.to_container_path(workdir)
            ));
            return (binds, mounts);
        }

        mounts.insert("act-toolcache".to_string(), "/opt/hostedtoolcache".to_string());
        mounts.insert(format!("{name}-env"), crate::container::linux::ACT_PATH.to_string());

        if let Some(spec) = self
            .run
            .as_ref()
            .and_then(|run| run.job().and_then(|job| job.container(run.document())))
        {
            for volume in &spec.volumes {
                split_volume(volume, &mut binds, &mut mounts);
            }
        }

        if self.config.bind_workdir {
            let mut modifier = "";
            if cfg!(target_os = "macos") {
                modifier = ":delegated";
            }
            if selinux_enabled() {
                modifier = ":z";
            }
            binds.push(format!(
                "{workdir}:{}{modifier}",
                extensions.to_container_path(workdir)
            ));
        } else {
            mounts.insert(name, extensions.to_container_path(workdir));
        }

        (binds, mounts)
    }
    /// `runsOnPlatformNames`: the runner labels this job asks for, after
    /// evaluating the `runs-on:` node.
    ///
    /// `runs-on:` is a **node**, not a string, and it is evaluated in place
    /// before being read — so `runs-on: ${{ matrix.os }}` and
    /// `runs-on: ${{ fromJSON('["ubuntu-latest"]') }}` both resolve. Upstream
    /// writes the result back into the job's own node; a copy is equivalent
    /// here, because a resolved `runs-on:` no longer contains `${{` and
    /// evaluating it again is a no-op.
    ///
    /// A missing `runs-on:` and a `runs-on:` that is a **mapping** both give
    /// an empty list, and a `runs-on:` whose expression does not parse gives
    /// one too — three different failures, one answer, and the distinction is
    /// only visible in a log line upstream writes.
    /// `runsOnPlatformNames`: the runner labels this job asks for, after
    /// evaluating the `runs-on:` node.
    ///
    /// `runs-on:` is a **node**, not a string, and it is evaluated in place
    /// before being read — so `runs-on: ${{ matrix.os }}` and
    /// `runs-on: ${{ fromJSON('["ubuntu-latest"]') }}` both resolve. Upstream
    /// writes the result back into the job's own node; a copy is equivalent
    /// here, because a resolved `runs-on:` no longer contains `${{` and
    /// evaluating it again is a no-op.
    ///
    /// A missing `runs-on:` and a `runs-on:` that is a **mapping** both give
    /// an empty list, and a `runs-on:` whose expression does not parse gives
    /// one too — three different failures, one answer, and the distinction is
    /// only visible in a log line upstream writes.
    pub fn runs_on_platform_names(
        &self,
        environment: &crate::expr::EvaluationEnvironment,
        status: &dyn crate::expr::StatusProvider,
    ) -> Vec<String> {
        let Some(run) = &self.run else {
            return Vec::new();
        };
        let Some(job) = run.job() else {
            return Vec::new();
        };
        let Some(runs_on) = job.raw_runs_on else {
            return Vec::new();
        };
        let mut document = run.document().clone();
        let status = crate::runner::expression::evaluate_yaml_node(
            &mut document,
            environment,
            status,
            crate::expr::EvaluationContext::Job,
            runs_on,
        );
        if status.is_err() {
            return Vec::new();
        }
        // Re-read from the *evaluated* copy: that is the whole point of the
        // call, and reading the original would make every case a no-op.
        let Some(evaluated) = document.node(runs_on) else {
            return Vec::new();
        };
        match evaluated.kind {
            crate::yaml_node::NodeKind::Scalar => match &evaluated.value {
                text if text.is_empty() => Vec::new(),
                text => vec![text.clone()],
            },
            crate::yaml_node::NodeKind::Sequence => document
                .string_slice(runs_on)
                .unwrap_or_default(),
            // A mapping is not a set of labels. GitHub's `{ group, labels }`
            // form is not something act resolves here, and upstream returns
            // nothing for it too.
            _ => Vec::new(),
        }
    }

    /// `runsOnImage`: the first `runs-on:` label that has an image configured.
    ///
    /// The lookup is **case-insensitive** on the label, so a workflow writing
    /// `runs-on: Ubuntu-Latest` finds the `ubuntu-latest` image. The first
    /// match wins, which is what makes a `runs-on: [self-hosted,
    /// ubuntu-latest]` fall through to the second entry.
    pub fn runs_on_image(
        &self,
        environment: &crate::expr::EvaluationEnvironment,
        status: &dyn crate::expr::StatusProvider,
    ) -> String {
        for label in self.runs_on_platform_names(environment, status) {
            let key = label.to_lowercase();
            if let Some(image) = self.config.platforms.get(&key) {
                if !image.is_empty() {
                    return image.clone();
                }
            }
        }
        String::new()
    }

    /// `containerImage`: the job's own `container:` image, interpolated.
    pub fn container_image(
        &self,
        environment: &crate::expr::EvaluationEnvironment,
        status: &dyn crate::expr::StatusProvider,
    ) -> String {
        let Some(run) = &self.run else {
            return String::new();
        };
        let Some(job) = run.job() else {
            return String::new();
        };
        let Some(spec) = job.container(run.document()) else {
            return String::new();
        };
        crate::runner::expression::interpolate(
            environment,
            status,
            crate::expr::EvaluationContext::Job,
            &spec.image,
        )
        .unwrap_or_default()
    }

    /// `platformImage`: the image the job container runs.
    ///
    /// A `container:` block **wins** over `runs-on:` — that is the difference
    /// between `container: node:18` on a job that says `runs-on: ubuntu-latest`
    /// and one that says `runs-on: node:18`. The `container:` image is taken
    /// even when it interpolates to an empty string only by accident, because
    /// the fallback is on the *interpolated* value: a `container:` whose image
    /// is an expression evaluating to `""` falls through to `runs-on:`.
    pub fn platform_image(
        &self,
        environment: &crate::expr::EvaluationEnvironment,
        status: &dyn crate::expr::StatusProvider,
    ) -> String {
        let from_container = self.container_image(environment, status);
        if !from_container.is_empty() {
            return from_container;
        }
        self.runs_on_image(environment, status)
    }

    /// `options`: the job's `container.options:`, or the configured default.
    pub fn container_options(
        &self,
        environment: &crate::expr::EvaluationEnvironment,
        status: &dyn crate::expr::StatusProvider,
    ) -> String {
        let from_job = self
            .run
            .as_ref()
            .and_then(|run| run.job())
            .zip(self.run.as_ref())
            .and_then(|(job, run)| job.container(run.document()))
            .map(|spec| {
                crate::runner::expression::interpolate(
                    environment,
                    status,
                    crate::expr::EvaluationContext::Job,
                    &spec.options,
                )
                .unwrap_or_default()
            });
        match from_job {
            Some(options) if !options.is_empty() => options,
            _ => self.config.container_options.clone(),
        }
    }

    // ------------------------------------------------------------ is_enabled --

    /// `isEnabled`: whether the job runs at all.
    ///
    /// Four gates, in this order, and the order is the behaviour — a job with a
    /// broken `if:` and no image reports the `if:` error, not the missing
    /// image.
    ///
    /// 1. the `if:` condition, with `success()` as the implicit check;
    /// 2. the job's type, and a malformed one is an error rather than a skip;
    /// 3. `result("skipped")` and a **false** for a falsy condition;
    /// 4. an image, but **only** for a plain job — a reusable-workflow call
    ///    returns here without one, which is why a `uses:` job runs on a host
    ///    with no platform configured.
    ///
    /// Point 4 is the subtle one: `jobType != Default` returns *before* the
    /// image is looked at, so a reusable workflow call is never skipped for a
    /// missing image even though it is not a container.
    ///
    /// # The two error paths
    ///
    /// Upstream wraps the `if:` failure in a sentence with a two-space indent
    /// and a `❌`, and returns the job-type error unwrapped. Both texts are
    /// reproduced because the first one is what a user sees in the log.
    pub fn is_enabled(
        &mut self,
        environment: &crate::expr::EvaluationEnvironment,
        status: &dyn crate::expr::StatusProvider,
    ) -> Result<bool, String> {
        let (condition, job_type) = {
            let Some(run) = self.run.as_ref() else {
                return Ok(false);
            };
            let Some(job) = run.job() else {
                return Ok(false);
            };
            // `job.If.Value` is the node's raw text, so a YAML boolean `false`
            // arrives as the four characters "false" — which the evaluator
            // then reads as the string, not as a boolean. That is upstream's
            // path and it is preserved rather than tidied.
            let doc = run.document();
            let condition = job.raw_if.and_then(|id| doc.scalar(id)).unwrap_or_default();
            let job_type = job.job_type();
            (condition, job_type)
        };

        let run_job = crate::runner::expression::eval_bool(
            environment,
            status,
            crate::expr::EvaluationContext::Job,
            &condition,
            crate::expr::DefaultStatusCheck::Success,
        )
        .map_err(|error| {
            format!("  ❌  Error in if-expression: \"if: {condition}\" ({error})")
        })?;

        // Upstream evaluates the condition and the job type, then reports the
        // condition's failure first. A job that is both malformed and skipped
        // therefore reports the condition, so the order below is the order the
        // two errors are chosen in, not the order they are computed in.
        let job_type = job_type.map_err(|error| error.to_string())?;

        if !run_job {
            self.set_result("skipped");
            return Ok(false);
        }

        if job_type != crate::model::JobType::Default {
            return Ok(true);
        }

        if self.platform_image(environment, status).is_empty() {
            return Ok(false);
        }
        Ok(true)
    }

    /// `result`: records what the job concluded, which `needs.<job>.result` and
    /// a dependant's `success()` both read.
    pub fn set_result(&mut self, result: &str) {
        if let Some(run) = self.run.as_mut() {
            if let Some(job) = run.job_mut() {
                job.result = result.to_string();
            }
        }
    }

    /// `matrix`: the one matrix cell this run is.
    pub fn matrix(&self) -> &BTreeMap<String, JsonValue> {
        &self.matrix
    }

    /// `withGithubEnv`: the twenty-odd `GITHUB_*` variables a step reads, plus
    /// `CI`, written **into** the environment it is handed.
    ///
    /// Mutating the caller's map rather than returning a new one is upstream's
    /// shape and it matters: the map is the step's own, and a step's
    /// `env:` block is folded into it afterwards, so this is one map throughout
    /// a step's life rather than a copy per stage.
    ///
    /// Two steps come after the twenty variables and both are easy to miss:
    /// the runtime variables, but **only** when an artifact server path is
    /// configured — without one there is no results service to point at — and
    /// then `ImageOS`, taken from the first non-empty `runs-on:` label.
    pub fn with_github_env(
        &self,
        environment: &crate::expr::EvaluationEnvironment,
        status: &dyn crate::expr::StatusProvider,
        github: &GithubContext,
        env: &mut BTreeMap<String, String>,
    ) {
        let set = |env: &mut BTreeMap<String, String>, key: &str, value: &str| {
            env.insert(key.to_string(), value.to_string());
        };
        set(env, "CI", "true");
        set(env, "GITHUB_WORKFLOW", &github.workflow);
        set(env, "GITHUB_RUN_ATTEMPT", &github.run_attempt);
        set(env, "GITHUB_RUN_ID", &github.run_id);
        set(env, "GITHUB_RUN_NUMBER", &github.run_number);
        set(env, "GITHUB_ACTION", &github.action);
        set(env, "GITHUB_ACTION_PATH", &github.action_path);
        set(env, "GITHUB_ACTION_REPOSITORY", &github.action_repository);
        set(env, "GITHUB_ACTION_REF", &github.action_ref);
        set(env, "GITHUB_ACTIONS", "true");
        set(env, "GITHUB_ACTOR", &github.actor);
        set(env, "GITHUB_REPOSITORY", &github.repository);
        set(env, "GITHUB_EVENT_NAME", &github.event_name);
        set(env, "GITHUB_EVENT_PATH", &github.event_path);
        set(env, "GITHUB_WORKSPACE", &github.workspace);
        set(env, "GITHUB_SHA", &github.sha);
        set(env, "GITHUB_REF", &github.ref_);
        set(env, "GITHUB_REF_NAME", &github.ref_name);
        set(env, "GITHUB_REF_TYPE", &github.ref_type);
        set(env, "GITHUB_JOB", &github.job);
        set(env, "GITHUB_REPOSITORY_OWNER", &github.repository_owner);
        set(env, "GITHUB_RETENTION_DAYS", &github.retention_days);
        set(env, "RUNNER_PERFLOG", &github.runner_perflog);
        set(env, "RUNNER_TRACKING_ID", &github.runner_tracking_id);
        set(env, "GITHUB_BASE_REF", &github.base_ref);
        set(env, "GITHUB_HEAD_REF", &github.head_ref);
        set(env, "GITHUB_SERVER_URL", &github.server_url);
        set(env, "GITHUB_API_URL", &github.api_url);
        set(env, "GITHUB_GRAPHQL_URL", &github.graphql_url);

        if !self.config.artifact_server_path.is_empty() {
            let config_env = self.config.env.clone();
            set_action_runtime_vars(
                &self.config.artifact_server_addr,
                &self.config.artifact_server_port,
                &config_env,
                env,
            );
        }

        for platform_name in self.runs_on_platform_names(environment, status) {
            if !platform_name.is_empty() {
                env.insert(
                    "ImageOS".to_string(),
                    crate::runner::step::image_os(&platform_name),
                );
            }
        }
    }

    /// `IsHostEnv`: a job with no image at all, on the self-hosted platform.
    ///
    /// The comparison is case-insensitive and only `-self-hosted` counts, so
    /// `runs-on: self-hosted` is **not** this — act has a real image for that.
    pub fn is_host_env(
        &self,
        environment: &crate::expr::EvaluationEnvironment,
        status: &dyn crate::expr::StatusProvider,
    ) -> bool {
        self.container_image(environment, status).is_empty()
            && self.runs_on_image(environment, status).eq_ignore_ascii_case("-self-hosted")
    }

    /// `ActionCacheDir`: where downloaded actions are kept between runs.
    ///
    /// The configured directory wins, then `XDG_CACHE_HOME`, then `~/.cache`,
    /// then the current directory, and the temp directory as a last resort.
    /// Every step of that is a guess about a machine, so the order is the
    /// contract and it is spelled out rather than collapsed.
    pub fn action_cache_dir(&self) -> String {
        if !self.config.action_cache_dir.is_empty() {
            return self.config.action_cache_dir.clone();
        }
        let base = match std::env::var("XDG_CACHE_HOME") {
            Ok(value) if !value.is_empty() => value,
            _ => match home_dir() {
                Some(home) => join_path(&home, ".cache"),
                None => match std::env::current_dir() {
                    Ok(cwd) => cwd.to_string_lossy().into_owned(),
                    // "Almost impossible to get here", upstream says, and
                    // agrees that the temp directory is a good fallback.
                    Err(_) => std::env::temp_dir().to_string_lossy().into_owned(),
                },
            },
        };
        join_path(&base, "act")
    }
}


impl RunContext {
    /// `getGithubContext`: assemble the `github` context for this run.
    ///
    /// The order of the steps is load-bearing and is upstream's:
    ///
    /// 1. the fields, read from the config and the run's own environment;
    /// 2. the container's paths, which **override** the config's workdir;
    /// 3. the defaults, so `github.run_id` is never empty;
    /// 4. the event payload, which then feeds the ref and the sha;
    /// 5. base/head ref, repository and owner, ref, sha, ref type and name;
    /// 6. the server URLs, github.com first and the instance's if not;
    /// 7. finally the three URLs the environment may override again.
    ///
    /// Steps 3 before 4 is why a `push` event's `ref` is used but `run_id` still
    /// gets its default, and step 6 after 5 is why `ref_type` is set before the
    /// URLs are decided.
    pub fn get_github_context(
        &self,
        git: &GitLookups,
    ) -> anyhow::Result<GithubContext> {
        let config_env = &self.config.env;
        let workflow_name = self
            .run
            .as_ref()
            .map(|run| run.workflow.name.clone())
            .unwrap_or_default();
        let mut github = GithubContext {
            event: serde_json::Map::new(),
            workflow: workflow_name,
            run_attempt: config_env.get("GITHUB_RUN_ATTEMPT").cloned().unwrap_or_default(),
            run_id: config_env.get("GITHUB_RUN_ID").cloned().unwrap_or_default(),
            run_number: config_env
                .get("GITHUB_RUN_NUMBER")
                .cloned()
                .unwrap_or_default(),
            actor: self.config.actor.clone(),
            event_name: self.config.event_name.clone(),
            action: self.current_step.clone(),
            token: self.config.token.clone(),
            job: self.job_id(),
            action_path: self.action_path.clone(),
            action_repository: self
                .env
                .get("GITHUB_ACTION_REPOSITORY")
                .cloned()
                .unwrap_or_default(),
            action_ref: self.env.get("GITHUB_ACTION_REF").cloned().unwrap_or_default(),
            repository_owner: config_env
                .get("GITHUB_REPOSITORY_OWNER")
                .cloned()
                .unwrap_or_default(),
            retention_days: config_env
                .get("GITHUB_RETENTION_DAYS")
                .cloned()
                .unwrap_or_default(),
            runner_perflog: config_env
                .get("RUNNER_PERFLOG")
                .cloned()
                .unwrap_or_default(),
            runner_tracking_id: config_env
                .get("RUNNER_TRACKING_ID")
                .cloned()
                .unwrap_or_default(),
            repository: config_env
                .get("GITHUB_REPOSITORY")
                .cloned()
                .unwrap_or_default(),
            ref_: config_env.get("GITHUB_REF").cloned().unwrap_or_default(),
            sha: config_env.get("SHA_REF").cloned().unwrap_or_default(),
            ref_name: config_env
                .get("GITHUB_REF_NAME")
                .cloned()
                .unwrap_or_default(),
            ref_type: config_env
                .get("GITHUB_REF_TYPE")
                .cloned()
                .unwrap_or_default(),
            base_ref: config_env
                .get("GITHUB_BASE_REF")
                .cloned()
                .unwrap_or_default(),
            head_ref: config_env
                .get("GITHUB_HEAD_REF")
                .cloned()
                .unwrap_or_default(),
            workspace: config_env
                .get("GITHUB_WORKSPACE")
                .cloned()
                .unwrap_or_default(),
            ..GithubContext::default()
        };

        // A container overrides the workdir the config named, because the
        // config's path is the host's and the job sees a different one.
        if let Some(container) = &self.job_container {
            github.event_path = format!("{}/workflow/event.json", container.act_path);
            github.workspace = container.workdir.clone();
        }

        // The five defaults. They are applied *before* the event is read, so an
        // event cannot supply `run_id` — a workflow that ships one silently
        // does not get it.
        for (field, fallback) in [
            (&mut github.run_attempt, "1"),
            (&mut github.run_id, "1"),
            (&mut github.run_number, "1"),
            (&mut github.retention_days, "0"),
            (&mut github.runner_perflog, "/dev/null"),
        ] {
            if field.is_empty() {
                *field = fallback.to_string();
            }
        }
        // Backwards compatibility for configs that want a default rather than
        // being run as a command.
        if github.actor.is_empty() {
            github.actor = "nektos/act".to_string();
        }

        if !self.event_json.is_empty() {
            // A malformed event is a log line upstream, not a failure: the
            // defaults above already stand, and a run with an empty event is
            // better than no run.
            match serde_json::from_str::<serde_json::Value>(&self.event_json) {
                Ok(serde_json::Value::Object(event)) => github.event = event,
                Ok(_) => {}
                Err(_) => {}
            }
        }

        github.set_base_and_head_ref();
        let repo_path = self.config.workdir.clone();
        github.set_repository_and_owner(
            &*git.find_repo,
            &self.config.github_instance,
            &self.config.remote_name,
            &repo_path,
        );
        if github.ref_.is_empty() {
            github.set_ref(&self.config.default_branch, &repo_path, git);
        }
        if github.sha.is_empty() {
            github.set_sha(&repo_path, git);
        }
        github.set_ref_type_and_name();

        // Defaults first, the instance's if it is not github.com, and the
        // environment last so an operator can point a run somewhere else.
        github.server_url = "https://github.com".to_string();
        github.api_url = "https://api.github.com".to_string();
        github.graphql_url = "https://api.github.com/graphql".to_string();
        if self.config.github_instance != "github.com" {
            github.server_url = format!("https://{}", self.config.github_instance);
            github.api_url = format!("https://{}/api/v3", self.config.github_instance);
            github.graphql_url = format!("https://{}/api/graphql", self.config.github_instance);
        }
        for (key, field) in [
            ("GITHUB_SERVER_URL", &mut github.server_url),
            ("GITHUB_API_URL", &mut github.api_url),
            ("GITHUB_GRAPHQL_URL", &mut github.graphql_url),
        ] {
            if let Some(value) = config_env.get(key).filter(|value| !value.is_empty()) {
                *field = value.clone();
            }
        }

        Ok(github)
    }
}

/// The git lookups the context falls back on when the event supplies neither a
/// ref nor a sha.
///
/// This is the wiring upstream gets for free from its package-level `var`s.
/// Naming it means the seam is visible: a test supplies its own pair, and this
/// is what production uses.
pub fn production_git_lookups() -> GitLookups {
    GitLookups::new(
        crate::common::git::find_git_ref,
        |path| {
            crate::common::git::find_git_revision(path).map(|(_, revision)| revision)
        },
        crate::common::git::find_github_repo,
    )
}

/// `GetServiceBindsAndMounts`: the same fork as [`RunContext::get_binds_and_mounts`],
/// without the toolcache, the env mount and the workdir.
///
/// A service container shares the job's daemon socket and gets the job's
/// volumes, but it has no workdir of its own and no act directory — the two
/// functions are the same rule with three lines of setup between them, so the
/// rule lives in `split_volume` and only the setup is duplicated. Upstream
/// writes the fork out twice; keeping one copy is the port's simplification and
/// the only way the two cannot drift.
pub fn get_service_binds_and_mounts(
    daemon_socket: &str,
    service_volumes: &[String],
) -> (Vec<String>, BTreeMap<String, String>) {
    let mut binds: Vec<String> = Vec::new();
    let mut mounts: BTreeMap<String, String> = BTreeMap::new();
    let daemon_socket = if daemon_socket.is_empty() {
        "/var/run/docker.sock"
    } else {
        daemon_socket
    };
    if daemon_socket != "-" {
        binds.push(format!(
            "{}:/var/run/docker.sock",
            get_docker_daemon_socket_mount_path(daemon_socket)
        ));
    }
    for volume in service_volumes {
        split_volume(volume, &mut binds, &mut mounts);
    }
    (binds, mounts)
}

/// The one rule both bind functions share, and the thing that reads backwards
/// in the source.
///
/// ```go
/// if !strings.Contains(v, ":") || filepath.IsAbs(v) { bind } else { mount }
/// ```
///
/// The colon and `IsAbs` are both asked about the **whole** string, so a host
/// path that itself carries a colon is still a bind. Measured on darwin:
///
/// | `volumes:` entry | contains `:` | `IsAbs` | lands on |
/// |---|---|---|---|
/// | `/volume` | no | true | bind |
/// | `/path/to/file/on/host:/volume` | yes | **true** | bind |
/// | `volume-id:/volume` | yes | false | mount `volume-id` → `/volume` |
///
/// The middle row is the one that reads wrong: a Docker named volume is
/// `name:/path` and a host path is absolute, so `IsAbs` on the joined string is
/// what decides. Reading the colon first and the path second would mount a host
/// file instead of binding it, and the step would find it empty.
fn split_volume(
    volume: &str,
    binds: &mut Vec<String>,
    mounts: &mut BTreeMap<String, String>,
) {
    if !volume.contains(':') || is_abs(volume) {
        // A bare path, or an absolute one: Docker creates it.
        binds.push(volume.to_string());
    } else {
        // `name:/path` with a non-absolute left side: an existing named volume,
        // mounted rather than created.
        let (name, path) = volume.split_once(':').expect("a colon was just checked");
        mounts.insert(name.to_string(), path.to_string());
    }
}


/// `filepath.IsAbs`, as the volume rule above needs it.
///
/// On Unix this is the first byte, and that is what was measured. Go's Windows
/// branch is the stdlib's `volumeNameLen` rule — a path needs a volume *and* a
/// separator after it, or a leading `\\` — so that is what the `cfg!(windows)`
/// side implements, since the port is cross-checked against
/// `x86_64-pc-windows-gnu` even though the measurement was taken on darwin.
fn is_abs(path: &str) -> bool {
    if !cfg!(windows) {
        return path.starts_with('/');
    }
    let Some(volume) = windows_volume_len(path) else {
        return false;
    };
    // `\\host\share\…` is absolute on its own; the volume name is the UNC
    // prefix and there is nothing after it to require.
    if path.starts_with("\\\\") {
        return true;
    }
    path[volume..].starts_with(['\\', '/'])
}

/// The length of a Windows volume name, or `None` when there is none.
///
/// A drive letter with or without a trailing separator, or a UNC share.
fn windows_volume_len(path: &str) -> Option<usize> {
    let bytes = path.as_bytes();
    // `C:`, `C:\`, `C:/`
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Some(2);
    }
    // `\\host\share` — the volume name runs to the end of the share component.
    if let Some(rest) = path.strip_prefix("\\\\") {
        let host_end = rest.find(['\\', '/']).unwrap_or(rest.len());
        if host_end == 0 {
            return None;
        }
        let after_host = &rest[host_end..];
        let trimmed = after_host.trim_start_matches(['\\', '/']);
        if trimmed.is_empty() {
            return Some(path.len());
        }
        let share_end = trimmed.find(['\\', '/']).unwrap_or(trimmed.len());
        return Some(path.len() - trimmed.len() + share_end);
    }
    None
}

/// Whether SELinux labelling is active, which is what adds `:z` to a bind.
///
/// act asks `selinux.GetEnabled()`, which probes `/sys/fs/selinux`. The answer
/// is a property of the host, not of the workflow, so it is read once and the
/// value is what a bind on this machine needs. A host that cannot answer gets
/// `false` — the same answer a non-SELinux kernel gives.
fn selinux_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::path::Path::new("/sys/fs/selinux").exists())
}

/// The user's home directory, the way `os.UserHomeDir` finds it.
fn home_dir() -> Option<String> {
    // Upstream reads `$HOME` on unix and `%USERPROFILE%` then the API on
    // Windows. An unset variable is not a home directory, so an empty value
    // falls through rather than producing `/act`.
    for name in ["HOME", "USERPROFILE"] {
        if let Ok(value) = std::env::var(name) {
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// `filepath.Join`, which joins with a separator and keeps the result clean.
fn join_path(base: &str, rest: &str) -> String {
    crate::artifacts::clean(&format!("{}/{rest}", base.trim_end_matches(['/', '\\'])))
}

impl fmt::Display for RunContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display_name())
    }
}

/// What the job container contributes to the `github` context.
///
/// Upstream reads both from `rc.JobContainer`. That is not ported, so they are
/// passed in instead — and `None` means "running on the host", which is what
/// `-P ubuntu-latest=` does and what the tests use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContainerPaths {
    /// The container's act directory, where `event.json` is written.
    pub act_path: String,
    /// The working directory as the container sees it, which is not the host's.
    pub workdir: String,
    /// `IsEnvironmentCaseInsensitive`: whether variable *names* are folded.
    ///
    /// Part of the real `JobContainer` interface, and not a detail: on Windows
    /// `Path=1` and `PATH=2` are the same variable, so a merge that treated
    /// them as two would let a step write `PATH=2` and still see `PATH=1`.
    /// [`crate::runner::step::merge_into_map`] branches on it.
    pub environment_case_insensitive: bool,
}

/// `mergeMaps`: later maps win, and a key missing from a later map keeps the
/// earlier value.
///
/// The order is the whole function. `getEnv` merges three sources in a fixed
/// order, so this is not a convenience — swapping two arguments would let a
/// workflow's own `env:` beat the operator's.
pub fn merge_maps<const N: usize>(maps: [BTreeMap<String, String>; N]) -> BTreeMap<String, String> {
    let mut merged = BTreeMap::new();
    for map in maps {
        for (key, value) in map {
            merged.insert(key, value);
        }
    }
    merged
}

/// `createContainerName`: a Docker-safe name that is still unique.
///
/// Docker allows `[a-zA-Z0-9_.-]`, but the name is built from a job name that
/// can hold anything, so every other character becomes `-`. The next step is
/// the one that is easy to get wrong, and it is not a loop: Go's
/// `strings.ReplaceAll` makes **one** left-to-right pass over non-overlapping
/// matches, so a run of dashes is *halved at most once*, not collapsed. Both
/// halves of that sentence are pinned by the table, and the second is where a
/// plausible re-implementation diverges:
///
/// | joined name | after `ReplaceAll` |
/// |---|---|
/// | `act-a---b` (from `a///b`) | `act-a--b` |
/// | `act-x-----y` | `act-x---y`, **not** `act-x-y` |
///
/// A `while let find("--")` loop agrees on the first row and collapses the
/// second, because it re-scans what the previous pass wrote. Go does not: the
/// search resumes after the replaced match, so the tail of a five-dash run is
/// never looked at again.
///
/// Then the length limit, so the hash fits: 63 characters, `-`, and 64 hex
/// characters of SHA-256 **over the name after the replacements but before the
/// trim**, which is why two jobs sharing a 63-character prefix still get
/// different containers.
pub fn create_container_name(parts: &[&str]) -> String {
    let joined = parts.join("-");
    let replaced: String = joined
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();
    let name = replace_all_non_overlapping(&replaced, "--", "-");
    let hash = sha256_hex(name.as_bytes());
    let limited = trim_to_len(&name, 63);
    let trimmed = limited.trim_matches('-');
    format!("{trimmed}-{hash}")
}

/// `strings.ReplaceAll`: every non-overlapping occurrence, left to right.
///
/// The difference from a loop is the whole point. A loop that re-scans the
/// output is a *different function* that agrees on most inputs, which is why it
/// survives a test written from a two-dash example.
fn replace_all_non_overlapping(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    let mut out = String::with_capacity(haystack.len());
    let mut rest = haystack;
    while let Some(position) = rest.find(needle) {
        out.push_str(&rest[..position]);
        out.push_str(replacement);
        // Resume *after* the match, not at its start.
        rest = &rest[position + needle.len()..];
    }
    out.push_str(rest);
    out
}

/// `trimToLen`: cut to `l` **bytes**, and a negative limit means zero.
///
/// # Deliberate deviation: the cut lands on a character boundary
///
/// Go's `s[:l]` is a byte slice and does not care where it lands, so upstream
/// will happily return a string that is not valid UTF-8 — `trimToLen("é", 1)`
/// gives back half a character. Rust's `&s[..n]` **panics** on the same input,
/// so a byte-faithful port is not merely awkward here, it is a crash.
///
/// The cut therefore lands on the nearest boundary at or before `l`. This
/// never differs from upstream on the path that exists: `createContainerName`
/// replaces every non-alphanumeric byte with `-` *before* trimming, so the
/// string it trims is ASCII by construction. The deviation is only reachable
/// by calling this directly with multi-byte input, which upstream's own call
/// sites do not do.
pub fn trim_to_len(s: &str, l: i64) -> String {
    let limit = if l < 0 { 0 } else { l as usize };
    if s.len() <= limit {
        return s.to_string();
    }
    let mut boundary = limit;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    s[..boundary].to_string()
}

/// `getDockerDaemonSocketMountPath`: the host path the container sees the
/// daemon socket at.
///
/// Measured, and the rule is not what the chain of `if`s suggests:
///
/// | configured | mounted |
/// |---|---|
/// | `npipe:////./pipe/docker_engine` | `/var/run/docker.sock` |
/// | `unix:///run/user/1000/docker.sock` | `/run/user/1000/docker.sock` |
/// | `ssh://` | `/var/run/docker.sock` |
/// | `tcp://127.0.0.1:2375` | `/var/run/docker.sock` |
/// | `git+ssh://` | `git+ssh://`, unchanged |
/// | `/var/run/docker.sock` | unchanged |
///
/// The fourth and fifth rows are the pair that a reading gets wrong. The
/// fallback fires when the scheme is **all letters** — `strings.IndexFunc`
/// looking for a non-letter returns `-1` exactly then. `ssh` and `tcp` are all
/// letters, so they fall back; `git+ssh` is not, so it is passed through
/// untouched, and the caller then binds a nonsense path. Upstream does that,
/// and the asymmetry is why the rule is a table and not a sentence.
pub fn get_docker_daemon_socket_mount_path(daemon_path: &str) -> String {
    let Some(scheme_end) = daemon_path.find("://") else {
        return daemon_path.to_string();
    };
    let scheme = &daemon_path[..scheme_end];
    if scheme.eq_ignore_ascii_case("npipe") {
        // A Linux container on a Windows host: the VM's own socket.
        return "/var/run/docker.sock".to_string();
    }
    if scheme.eq_ignore_ascii_case("unix") {
        return daemon_path[scheme_end + 3..].to_string();
    }
    if !scheme.chars().any(|character| !character.is_ascii_alphabetic()) {
        // An unknown protocol, so the default. Note the *positive* test: the
        // fallback is for schemes made only of letters.
        return "/var/run/docker.sock".to_string();
    }
    daemon_path.to_string()
}

/// `setActionRuntimeVars`: the two variables every action reads to reach the
/// results service.
///
/// `ACTIONS_RUNTIME_URL` and `ACTIONS_RESULTS_URL` are the same value, and
/// `ACTIONS_RUNTIME_TOKEN` is a JWT scoped to the run. The environment wins
/// over the config for both, so an operator can point the job at a different
/// results service without changing the workflow.
///
/// Upstream reads `os.Getenv` for both overrides. libtest runs tests in
/// parallel and the process environment is global, so a test that *set* those
/// two variables would race every other test that reads them — the same defect
/// that made `common::draw` flaky once already. The env read therefore stays in
/// this function and the work is done by [`set_action_runtime_vars_with`],
/// which takes the two overrides as values and is what the tests drive. Upstream
/// stays transliterated; only the seam moves.
pub fn set_action_runtime_vars(
    artifact_server_addr: &str,
    artifact_server_port: &str,
    config_env: &BTreeMap<String, String>,
    env: &mut BTreeMap<String, String>,
) {
    let url_override = std::env::var("ACTIONS_RUNTIME_URL").ok();
    let token_override = std::env::var("ACTIONS_RUNTIME_TOKEN").ok();
    set_action_runtime_vars_with(
        artifact_server_addr,
        artifact_server_port,
        config_env,
        url_override.as_deref(),
        token_override.as_deref(),
        env,
    );
}

/// [`set_action_runtime_vars`] with the two process-environment overrides passed
/// in instead of read, so a test can drive every branch deterministically.
///
/// An override of `None` **or** the empty string means "not set", because
/// upstream tests `os.Getenv(...) == ""` and Go cannot tell an unset variable
/// from an empty one. That rule lives here rather than in the caller so the seam
/// is complete: a value arriving through it behaves exactly as it would have
/// arriving from the process environment, and no caller has to remember to
/// filter.
pub fn set_action_runtime_vars_with(
    artifact_server_addr: &str,
    artifact_server_port: &str,
    config_env: &BTreeMap<String, String>,
    url_override: Option<&str>,
    token_override: Option<&str>,
    env: &mut BTreeMap<String, String>,
) {
    let runtime_url = match url_override.filter(|value| !value.is_empty()) {
        Some(value) => value.to_string(),
        None => format!("http://{artifact_server_addr}:{artifact_server_port}/"),
    };
    env.insert("ACTIONS_RUNTIME_URL".to_string(), runtime_url.clone());
    env.insert("ACTIONS_RESULTS_URL".to_string(), runtime_url);

    let token = match token_override.filter(|value| !value.is_empty()) {
        Some(value) => value.to_string(),
        None => {
            // The `1` is the *absent* case, not the unparseable case.
            // `runID, _ = strconv.ParseInt(rid, 10, 64)` assigns the value
            // `ParseInt` returned alongside its error, and that value is `0`
            // for a syntax error — the same `=`-overwrites-the-default shape as
            // `GetMaxParallel`, and just as invisible in the happy path.
            // Measured on v0.2.89: `"45"`→45, `""`/`" 45"`/`"lots"`→0,
            // `"99999999999999999999"`→i64::MAX (a range error clamps, so this
            // is not `parse().unwrap_or(1)` — see [`crate::gostrconv`]).
            let run_id = match config_env.get("GITHUB_RUN_ID") {
                Some(raw) => crate::gostrconv::parse_int(raw),
                None => 1,
            };
            crate::common::create_authorization_token(run_id, run_id, run_id)
                .unwrap_or_default()
        }
    };
    env.insert("ACTIONS_RUNTIME_TOKEN".to_string(), token);
}

/// SHA-256 as lower-case hex.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Workflow;
    use crate::yaml_node::Document;

    /// A run of one job out of a one-job workflow, built from YAML.
    fn run(job_yaml: &str) -> Run {
        let source = format!(
            "name: test-workflow\njobs:\n  test:\n    name: test\n{}",
            job_yaml
                .lines()
                .map(|line| format!("    {line}\n"))
                .collect::<String>()
        );
        let doc = Rc::new(Document::parse(&source).expect("the fixture parses"));
        let workflow = Workflow::from_document("test.yml", &doc).expect("the fixture decodes");
        Run::new(workflow, doc, "test")
    }

    /// A run context with no job container, which is the Docker branch.
    fn rc_with(config: RunConfig, job: Option<Run>) -> RunContext {
        RunContext {
            config,
            run: job,
            ..RunContext::default()
        }
    }

    // ---------------------------------------------------------------- names --

    /// Every row of the name table, as Go answered it.
    ///
    /// The two dash rows are the point of this test. A `while let find("--")`
    /// loop agrees with the three-dash row and disagrees with the five-dash
    /// one, which is why the five-dash case is here and not an accident.
    #[test]
    fn a_container_name_is_docker_safe_and_still_unique() {
        let cases: &[(&[&str], &str)] = &[
            (
                &["act", "test-workflow/test"],
                "act-test-workflow-test-de204e055cd8f0b7cf8985e94557f1b9605cc586b3a77c31717ab7f4d2bd694f",
            ),
            (
                &["act", "a///b"],
                "act-a--b-9ee7963df80b48044802f840396106b6adb2c1721dca13ec5bad113cf5f8e894",
            ),
            (
                &["act", "x-----y"],
                "act-x---y-6a6230677aaca103f35e4e372283254b7500ebfaabadf184ea61ebba61b27270",
            ),
            (
                &["act", "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"],
                "act-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz-ccfaa936a565229d526cad9cf10e8cb1d00255db81b2ee62e29438cfa517e545",
            ),
            (
                &["act", "-lead-and-trail-"],
                "act-lead-and-trail-0597fe9be129ab7f373c814b0ac776f44e755c2a29ada352ffc7535c3c9ec1b7",
            ),
        ];
        for (parts, want) in cases {
            assert_eq!(create_container_name(parts), *want, "parts: {parts:?}");
        }
    }

    /// The name is Docker-safe whatever the job is called, and the hash is
    /// what keeps two long names apart.
    #[test]
    fn the_container_name_contains_only_docker_safe_characters() {
        let name = create_container_name(&["act", "test-workflow/test"]);
        assert!(name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_'));
        let prefix = "act-test-workflow-test";
        assert_eq!(name.len(), prefix.len() + 1 + 64, "{name}");
    }

    /// `trimToLen` clamps a negative limit to zero and cuts on a character
    /// boundary.
    ///
    /// The boundary is a **deviation**, documented on the function: Go returns
    /// half a character where this returns none. It is here so the deviation is
    /// asserted rather than discovered — a direct call on a multi-byte string
    /// is the only way to reach it, and upstream would answer with a different
    /// string.
    #[test]
    fn trimming_clamps_and_counts_bytes() {
        assert_eq!(trim_to_len("abc", 5), "abc");
        assert_eq!(trim_to_len("abc", 3), "abc");
        assert_eq!(trim_to_len("abc", 2), "ab");
        assert_eq!(trim_to_len("abc", 0), "");
        assert_eq!(trim_to_len("abc", -1), "", "a negative limit is zero");
        // Go returns the first byte of `é`, which is not a character. This
        // returns the empty string — see the deviation note.
        assert_eq!(trim_to_len("é", 1), "");
        assert_eq!(trim_to_len("é", 2), "é", "a limit past the end is the whole string");
        // The path that actually exists is ASCII, so there the two agree.
        assert_eq!(trim_to_len("abcdef", 3), "abc");
    }

    // --------------------------------------------------------- bind vs mount --

    /// `is_abs` as Go answered it, including the two rows that decide the fork.
    #[test]
    fn is_abs_follows_go_and_not_what_the_string_looks_like() {
        let cases: &[(&str, bool)] = &[
            ("/volume", true),
            ("/path/to/file/on/host:/volume", true),
            ("volume-id:/volume", false),
            ("volume-id", false),
            ("/", true),
            ("", false),
            ("relative:/x", false),
            ("./x:/y", false),
            ("~/x:/y", false),
            ("c:/x:/y", false),
        ];
        for (path, want) in cases {
            assert_eq!(is_abs(path), *want, "path {path:?}");
        }
    }

    /// The three volume cases from upstream's `ContainerVolumeMountTest`.
    ///
    /// The middle one is the whole point: a host path that contains a colon is
    /// still absolute, so it is **bound**, not mounted. Reading the colon first
    /// and the path second would put a named volume where a host file belongs,
    /// and the step would find it empty.
    #[test]
    fn a_volume_entry_lands_on_a_bind_or_a_mount_by_whether_the_whole_string_is_absolute() {
        struct Case {
            label: &'static str,
            volume: &'static str,
            want_bind: &'static str,
            want_mounts: &'static [(&'static str, &'static str)],
        }
        // Upstream's own table spells "no assertion wanted" as an empty
        // `wantbind`; carrying that as `""` and then asserting *absence* is a
        // stronger claim than upstream makes. So the bind cases assert
        // presence, and only the named-volume case — the one the fork exists
        // for — asserts absence.
        let cases = [
            Case {
                label: "BindAnonymousVolume",
                volume: "/volume",
                want_bind: "/volume",
                want_mounts: &[],
            },
            Case {
                label: "BindHostFile",
                volume: "/path/to/file/on/host:/volume",
                want_bind: "/path/to/file/on/host:/volume",
                want_mounts: &[],
            },
            Case {
                label: "MountExistingVolume",
                volume: "volume-id:/volume",
                want_bind: "",
                want_mounts: &[("volume-id", "/volume")],
            },
        ];

        for case in &cases {
            let rc = rc_with(
                RunConfig {
                    workdir: "/mnt/linux".to_string(),
                    ..RunConfig::default()
                },
                Some(run(&format!("container:\n  volumes: ['{}']\n", case.volume))),
            );
            let (binds, mounts) = rc.get_binds_and_mounts();
            if case.want_bind.is_empty() {
                assert!(
                    !binds.iter().any(|bind| bind == case.volume),
                    "{} is a mount, not a bind: {binds:?}",
                    case.label
                );
            } else {
                assert!(
                    binds.iter().any(|bind| bind == case.want_bind),
                    "{}: {binds:?}",
                    case.label
                );
            }
            for (key, value) in case.want_mounts.iter() {
                assert_eq!(
                    mounts.get(*key).map(String::as_str),
                    Some(*value),
                    "{}",
                    case.label
                );
            }
        }
    }

    /// The daemon socket is bound unless it is `-`, and the default is
    /// `/var/run/docker.sock`.
    #[test]
    fn the_daemon_socket_is_bound_unless_it_is_a_dash() {
        let mut rc = rc_with(RunConfig::default(), None);
        let (binds, _) = rc.get_binds_and_mounts();
        assert!(
            binds
                .iter()
                .any(|bind| bind == "/var/run/docker.sock:/var/run/docker.sock"),
            "the default: {binds:?}"
        );

        rc.config.container_daemon_socket = "-".to_string();
        let (binds, _) = rc.get_binds_and_mounts();
        assert!(
            !binds.iter().any(|bind| bind.ends_with(":/var/run/docker.sock")),
            "a dash means no socket at all: {binds:?}"
        );

        rc.config.container_daemon_socket = "unix:///run/user/1000/docker.sock".to_string();
        let (binds, _) = rc.get_binds_and_mounts();
        assert!(
            binds
                .iter()
                .any(|bind| bind == "/run/user/1000/docker.sock:/var/run/docker.sock"),
            "the unix scheme is stripped: {binds:?}"
        );
    }

    /// The socket path rule, as the Go probe answered it.
    ///
    /// `ssh` and `tcp` are all letters and fall back to the default; `git+ssh`
    /// is not, and is passed through untouched. That asymmetry is the reason
    /// the table exists — a reading that checks "is it a known scheme" gets two
    /// of the six rows right by accident.
    #[test]
    fn the_daemon_socket_path_follows_its_scheme() {
        let cases: &[(&str, &str)] = &[
            ("npipe:////./pipe/docker_engine", "/var/run/docker.sock"),
            ("unix:///run/user/1000/docker.sock", "/run/user/1000/docker.sock"),
            ("ssh://", "/var/run/docker.sock"),
            ("tcp://127.0.0.1:2375", "/var/run/docker.sock"),
            ("git+ssh://", "git+ssh://"),
            ("/var/run/docker.sock", "/var/run/docker.sock"),
            ("-", "-"),
        ];
        for (configured, want) in cases {
            assert_eq!(
                get_docker_daemon_socket_mount_path(configured),
                *want,
                "configured: {configured:?}"
            );
        }
    }

    /// A host environment returns before `volumes:` is looked at.
    ///
    /// Upstream's type switch is what makes the two branches differ, and the
    /// consequence is not obvious from reading the two blocks: a job that
    /// declares `volumes:` and runs on the host gets none of them mounted. That
    /// looks like a bug and is what upstream does, so it is pinned here rather
    /// than left to be discovered later.
    #[test]
    fn a_host_environment_ignores_the_jobs_volumes_entirely() {
        let mut rc = rc_with(
            RunConfig {
                workdir: "/mnt/linux".to_string(),
                ..RunConfig::default()
            },
            Some(run("container:\n  volumes: ['/volume']\n")),
        );
        let (binds, mounts) = rc.get_binds_and_mounts();
        assert!(
            binds.iter().any(|bind| bind == "/volume"),
            "the docker branch binds it: {binds:?}"
        );
        assert!(mounts.contains_key("act-toolcache"), "{mounts:?}");

        rc.job_container = Some(ContainerPaths {
            act_path: "/host/act".to_string(),
            workdir: "/host/scratch".to_string(),
            environment_case_insensitive: false,
        });
        let (binds, mounts) = rc.get_binds_and_mounts();
        assert!(
            !binds.iter().any(|bind| bind == "/volume"),
            "the host branch never reads volumes: {binds:?}"
        );
        assert!(
            !mounts.contains_key("act-toolcache"),
            "and returns before the toolcache mount: {mounts:?}"
        );
        assert!(
            binds.contains(&"/host/act:/var/run/act".to_string()),
            "{binds:?}"
        );
    }

    /// The workdir half of `TestRunContext_GetBindsAndMounts`, restricted to the
    /// cases the upstream test actually runs on this platform.
    #[test]
    fn the_workdir_becomes_a_bind_or_a_mount_as_configured() {
        for (workdir, want) in [("/mnt/linux", "/mnt/linux"), ("/mnt/path with spaces/linux", "/mnt/path with spaces/linux")] {
            for bind_workdir in [true, false] {
                let rc = rc_with(
                    RunConfig {
                        workdir: workdir.to_string(),
                        bind_workdir,
                        ..RunConfig::default()
                    },
                    None,
                );
                let (binds, mounts) = rc.get_binds_and_mounts();
                if bind_workdir {
                    let mut full = format!("{workdir}:{want}");
                    if cfg!(target_os = "macos") {
                        full.push_str(":delegated");
                    }
                    assert!(binds.contains(&full), "workdir {workdir:?}: {binds:?}");
                } else {
                    let key = rc.job_container_name();
                    assert_eq!(
                        mounts.get(&key).map(String::as_str),
                        Some(want),
                        "workdir {workdir:?}"
                    );
                }
            }
        }
    }

    // ------------------------------------------------------- github context --

    /// Git lookups that always fail, so the context falls back exactly the way
    /// it does outside a checkout.
    fn no_git() -> GitLookups {
        GitLookups::new(
            |_| Err(anyhow::anyhow!("not a repository")),
            |_| Err(anyhow::anyhow!("not a repository")),
            |_, _, _| Err(anyhow::anyhow!("not a repository")),
        )
    }

    /// A run context with a one-job workflow and no job container, which is all
    /// the github context needs.
    fn ghc_run_context(event: &str, json: &str, instance: &str, env: &[(&str, &str)]) -> RunContext {
        RunContext {
            event_json: json.to_string(),
            current_step: "step".to_string(),
            config: RunConfig {
                event_name: event.to_string(),
                workdir: String::new(),
                github_instance: instance.to_string(),
                env: env
                    .iter()
                    .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                    .collect(),
                ..RunConfig::default()
            },
            run: Some(Run::new(
                Workflow {
                    name: "GitHubContextTest".to_string(),
                    ..Workflow::default()
                },
                Rc::new(Document::parse("jobs: {}\n").expect("an empty document")),
                "job1",
            )),
            ..RunContext::default()
        }
    }

    /// Upstream's `TestGetGitHubContextRef`, all eleven cases.
    ///
    /// The workdir is empty, so the git fallback cannot answer and every `ref`
    /// comes from the event. That is what makes this a table about events
    /// rather than about repositories.
    ///
    /// The `delete` row is worth reading twice: the ref comes from the event's
    /// own `repository.default_branch`, not from the configured default branch,
    /// and a *missing* `repository` would be created rather than left alone.
    /// That is `withDefaultBranch`, which I first read backwards.
    #[test]
    fn the_github_context_ref_follows_the_event() {
        let cases: &[(&str, &str, &str)] = &[
            ("push", r#"{"ref":"0000000000000000000000000000000000000000"}"#, "0000000000000000000000000000000000000000"),
            ("create", r#"{"ref":"0000000000000000000000000000000000000000"}"#, "0000000000000000000000000000000000000000"),
            ("workflow_dispatch", r#"{"ref":"0000000000000000000000000000000000000000"}"#, "0000000000000000000000000000000000000000"),
            ("delete", r#"{"repository":{"default_branch": "main"}}"#, "refs/heads/main"),
            ("pull_request", r#"{"number":123}"#, "refs/pull/123/merge"),
            ("pull_request_review", r#"{"number":123}"#, "refs/pull/123/merge"),
            ("pull_request_review_comment", r#"{"number":123}"#, "refs/pull/123/merge"),
            ("pull_request_target", r#"{"pull_request":{"base":{"ref": "main"}}}"#, "refs/heads/main"),
            ("deployment", r#"{"deployment": {"ref": "tag-name"}}"#, "tag-name"),
            ("deployment_status", r#"{"deployment": {"ref": "tag-name"}}"#, "tag-name"),
            ("release", r#"{"release": {"tag_name": "tag-name"}}"#, "refs/tags/tag-name"),
        ];

        for (event, json, want) in cases {
            let rc = ghc_run_context(event, json, "github.com", &[]);
            let github = rc.get_github_context(&no_git()).expect("builds");
            assert_eq!(github.ref_, *want, "event: {event}");
        }
    }

    /// Upstream's `TestGetGitHubContext`: the five defaults, the actor and the
    /// job id.
    ///
    /// The repository is asserted as `nektos/act` because the git lookup fails
    /// and that is the fallback — not because the directory is a checkout. A
    /// test that passed inside act's own clone and failed outside it would be
    /// measuring the wrong thing.
    #[test]
    fn the_github_context_carries_its_defaults() {
        let rc = ghc_run_context("push", "", "github.com", &[]);
        let github = rc.get_github_context(&no_git()).expect("builds");

        assert_eq!(github.run_id, "1", "RunID");
        assert_eq!(github.run_number, "1", "RunNumber");
        assert_eq!(github.retention_days, "0", "RetentionDays");
        assert_eq!(github.runner_perflog, "/dev/null", "RunnerPerflog");
        assert_eq!(github.actor, "nektos/act", "Actor");
        assert_eq!(github.repository, "nektos/act", "the git fallback");
        assert_eq!(github.repository_owner, "nektos", "split on the first slash");
        assert_eq!(github.job, "job1", "Job");
        assert_eq!(github.workflow, "GitHubContextTest", "Workflow");
        assert_eq!(github.action, "step", "Action is the current step");
    }

    /// The defaults are applied **before** the event is read, so an event can
    /// neither supply `run_id` nor take it away.
    #[test]
    fn a_defaults_are_applied_before_the_event_is_read() {
        let rc = ghc_run_context(
            "push",
            r#"{"run_id": "99", "run_number": "77", "retention_days": "30"}"#,
            "github.com",
            &[],
        );
        let github = rc.get_github_context(&no_git()).expect("builds");
        assert_eq!(github.run_id, "1", "the default wins over the event");
        assert_eq!(github.run_number, "1");
        assert_eq!(github.retention_days, "0");
    }

    /// The server URLs: github.com, then the instance, then the environment.
    ///
    /// Three steps in a fixed order, and the last is the one an operator
    /// normally uses — so an environment override has to beat the instance
    /// logic, or a mirror could not be pointed at a proxy.
    #[test]
    fn the_server_urls_follow_the_instance_and_then_the_environment() {
        let github = ghc_run_context("push", "", "github.com", &[])
            .get_github_context(&no_git())
            .expect("builds");
        assert_eq!(github.server_url, "https://github.com");
        assert_eq!(github.api_url, "https://api.github.com");
        assert_eq!(github.graphql_url, "https://api.github.com/graphql");

        let github = ghc_run_context("push", "", "ghe.example.com", &[])
            .get_github_context(&no_git())
            .expect("builds");
        assert_eq!(github.server_url, "https://ghe.example.com");
        assert_eq!(github.api_url, "https://ghe.example.com/api/v3");
        assert_eq!(github.graphql_url, "https://ghe.example.com/api/graphql");

        let github = ghc_run_context(
            "push",
            "",
            "ghe.example.com",
            &[
                ("GITHUB_SERVER_URL", "https://proxy.internal"),
                ("GITHUB_API_URL", "https://proxy.internal/api"),
            ],
        )
        .get_github_context(&no_git())
        .expect("builds");
        assert_eq!(github.server_url, "https://proxy.internal");
        assert_eq!(github.api_url, "https://proxy.internal/api");
        assert_eq!(
            github.graphql_url,
            "https://ghe.example.com/api/graphql",
            "an unset override leaves the instance's value"
        );
    }

    /// `ref_type` and `ref_name` are decided **after** the ref, and a pull
    /// request has an empty `ref_type` — it is neither a branch nor a tag.
    ///
    /// A ref matching none of the three prefixes leaves both empty, which
    /// looks like a missing value and is exactly what GitHub reports for a
    /// deployment on a branch.
    #[test]
    fn the_ref_type_and_name_follow_the_three_prefixes_and_nothing_else() {
        let cases: &[(&str, &str, &str)] = &[
            ("refs/heads/main", "branch", "main"),
            ("refs/tags/v1.2.3", "tag", "v1.2.3"),
            ("refs/pull/123/merge", "", "123/merge"),
            ("tag-name", "", ""),
        ];
        for (reference, want_type, want_name) in cases {
            let json = format!(
                r#"{{"ref":{},"number":123}}"#,
                serde_json::to_string(reference).expect("a quoted ref")
            );
            let rc = ghc_run_context("push", &json, "github.com", &[]);
            let github = rc.get_github_context(&no_git()).expect("builds");
            assert_eq!(github.ref_, *reference, "the ref itself");
            assert_eq!(github.ref_type, *want_type, "ref {reference:?}");
            assert_eq!(github.ref_name, *want_name, "ref {reference:?}");
        }
    }

    /// The service half of the same rule: the socket, the volumes, and nothing
    /// else. A service container gets no workdir and no toolcache mount, which
    /// is the only difference from the job container.
    #[test]
    fn a_service_container_gets_the_socket_and_its_volumes_and_nothing_else() {
        let (binds, mounts) = get_service_binds_and_mounts(
            "",
            &["/volume".to_string(), "named:/somewhere".to_string()],
        );
        assert!(
            binds.contains(&"/var/run/docker.sock:/var/run/docker.sock".to_string()),
            "{binds:?}"
        );
        assert!(binds.contains(&"/volume".to_string()), "{binds:?}");
        assert_eq!(mounts.get("named").map(String::as_str), Some("/somewhere"));
        assert!(!mounts.contains_key("act-toolcache"), "{mounts:?}");
    }

    // ------------------------------------------------------------ get_env --

    /// A run of one job out of a workflow that also has workflow-level `env:`.
    /// [`run`] cannot build this: it puts everything under `jobs:`.
    fn run_with_workflow_env(workflow_env: &str, config_env: &[(&str, &str)]) -> RunContext {
        let source = format!(
            "name: test-workflow\n{workflow_env}jobs:\n  test:\n    name: test\n    runs-on: ubuntu-latest\n"
        );
        let doc = Rc::new(Document::parse(&source).expect("the fixture parses"));
        let workflow = Workflow::from_document("test.yml", &doc).expect("the fixture decodes");
        let config = RunConfig {
            env: config_env
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
            ..RunConfig::default()
        };
        rc_with(config, Some(Run::new(workflow, doc, "test")))
    }

    /// Upstream `TestRunContextGetEnv`: `Config.Env` overwrites `Workflow.Env`.
    #[test]
    fn config_env_overwrites_workflow_env() {
        let mut rc = run_with_workflow_env(
            "env:\n  OVERWRITTEN: \"false\"\n",
            &[("OVERWRITTEN", "true")],
        );
        let env = rc.get_env();
        assert_eq!(env.get("OVERWRITTEN").map(String::as_str), Some("true"));
    }

    /// Upstream `TestRunContextGetEnv`: with no overlapping key the workflow's
    /// value survives, so the merge is not a replacement.
    #[test]
    fn workflow_env_survives_when_nothing_overwrites_it() {
        let mut rc = run_with_workflow_env(
            "env:\n  OVERWRITTEN: \"false\"\n",
            &[("SOME_OTHER_VAR", "true")],
        );
        let env = rc.get_env();
        assert_eq!(env.get("OVERWRITTEN").map(String::as_str), Some("false"));
        assert_eq!(env.get("SOME_OTHER_VAR").map(String::as_str), Some("true"));
    }

    /// The full precedence chain, which upstream's two cases only bracket:
    /// `mergeMaps(workflow, job, config)` applies each map in turn, so a later
    /// one wins and the job level sits *between* the other two.
    #[test]
    fn job_env_sits_between_the_workflow_and_the_config() {
        let source = concat!(
            "name: test-workflow\n",
            "env:\n  LEVEL: workflow\n  ONLY_WORKFLOW: yes\n",
            "jobs:\n  test:\n    name: test\n    runs-on: ubuntu-latest\n",
            "    env:\n      LEVEL: job\n      ONLY_JOB: yes\n",
        );
        let doc = Rc::new(Document::parse(source).expect("the fixture parses"));
        let workflow = Workflow::from_document("test.yml", &doc).expect("the fixture decodes");
        let config = RunConfig {
            env: [("LEVEL".to_string(), "config".to_string())]
                .into_iter()
                .collect(),
            ..RunConfig::default()
        };
        let mut rc = rc_with(config, Some(Run::new(workflow, doc, "test")));
        let env = rc.get_env();
        assert_eq!(env.get("LEVEL").map(String::as_str), Some("config"));
        assert_eq!(env.get("ONLY_JOB").map(String::as_str), Some("yes"));
        assert_eq!(env.get("ONLY_WORKFLOW").map(String::as_str), Some("yes"));
    }

    /// `ACT=true` is stamped on **every** call, after the merge, and so it
    /// beats all three levels — including a workflow that sets `ACT: false`.
    /// Upstream writes it into the memoised map, so it survives later merges.
    #[test]
    fn act_is_stamped_after_the_merge_and_cannot_be_overridden() {
        let mut rc = run_with_workflow_env("env:\n  ACT: \"false\"\n", &[]);
        assert_eq!(rc.get_env().get("ACT").map(String::as_str), Some("true"));
    }

    /// The merge runs **once**. Upstream guards on `rc.Env == nil`, and after
    /// the first call the map holds at least `ACT`, so a second call is a read.
    ///
    /// Observable: a `Config.Env` key added *after* the first call never
    /// appears. This is the memoisation, not a merge order.
    #[test]
    fn the_merge_is_memoised_so_later_config_keys_do_not_appear() {
        let mut rc = run_with_workflow_env("env:\n  WORKFLOW_VAR: \"1\"\n", &[]);
        assert_eq!(rc.get_env().get("WORKFLOW_VAR").map(String::as_str), Some("1"));
        rc.config
            .env
            .insert("ADDED_LATER".to_string(), "2".to_string());
        let second = rc.get_env();
        assert!(
            !second.contains_key("ADDED_LATER"),
            "the merge must not run twice: {second:?}"
        );
        assert_eq!(second.get("ACT").map(String::as_str), Some("true"));
    }

    // ------------------------------------------------------ get_job_context --

    /// Not upstream: `getJobContext` has no test in `pkg/runner` at v0.2.89.
    /// These pin the three branches against the Go source, which is a weaker
    /// authority than a test would be — stated here so nobody later mistakes
    /// them for a port of an upstream case.
    mod job_context {
        use super::*;

        fn rc_with_step(name: &str, conclusion: StepStatus) -> RunContext {
            let mut rc = rc_with(RunConfig::default(), Some(super::run("runs-on: ubuntu-latest\n")));
            rc.step_results.insert(
                name.to_string(),
                StepResult {
                    conclusion,
                    ..StepResult::default()
                },
            );
            rc
        }

        #[test]
        fn no_steps_and_no_cancellation_is_success() {
            let rc = rc_with(RunConfig::default(), Some(super::run("runs-on: ubuntu-latest\n")));
            assert_eq!(rc.get_job_context().status, "success");
        }

        #[test]
        fn a_failed_step_makes_the_job_fail() {
            let rc = rc_with_step("boom", StepStatus::Failure);
            assert_eq!(rc.get_job_context().status, "failure");
        }

        /// The reason it reads `conclusion` and not `outcome`: a
        /// `continue-on-error` step has outcome `failure` and conclusion
        /// `success`, and the job still passes.
        #[test]
        fn a_skipped_or_successful_step_leaves_the_job_successful() {
            for status in [StepStatus::Success, StepStatus::Skipped] {
                let rc = rc_with_step("step", status);
                assert_eq!(
                    rc.get_job_context().status,
                    "success",
                    "conclusion {status:?} must not fail the job"
                );
            }
        }

        /// Cancellation is checked *before* the steps, so a cancelled job with
        /// a failed step is still `"cancelled"`.
        #[test]
        fn cancellation_beats_a_failed_step() {
            let mut rc = rc_with_step("boom", StepStatus::Failure);
            rc.cancelled = true;
            assert_eq!(rc.get_job_context().status, "cancelled");
        }
    }

    // --------------------------------------------------------- network_name --

    /// Not upstream: `networkName` has no test in `pkg/runner` at v0.2.89.
    /// Same weaker authority, stated for the same reason.
    mod network {
        use super::*;

        fn rc(job_yaml: &str, mode: &str) -> RunContext {
            let config = RunConfig {
                container_network_mode: mode.to_string(),
                ..RunConfig::default()
            };
            rc_with(config, Some(super::run(job_yaml)))
        }

        /// No services and no `--network` means Docker's default, and `false`
        /// says *act did not create this network*.
        #[test]
        fn without_services_and_without_a_mode_the_network_is_hosts_and_not_ours() {
            let rc = rc("runs-on: ubuntu-latest\n", "");
            assert_eq!(rc.network_name(), ("host".to_string(), false));
        }

        /// A configured mode is used verbatim and still is not act's to create.
        #[test]
        fn a_configured_mode_is_passed_through_and_not_created_by_act() {
            let rc = rc("runs-on: ubuntu-latest\n", "bridge");
            assert_eq!(rc.network_name(), ("bridge".to_string(), false));
        }

        /// Services are what make act build a network, so `true` appears here
        /// and in no other branch. The name is
        /// `<container>-<jobID>-network`.
        #[test]
        fn a_job_with_services_gets_a_network_act_creates() {
            let rc = rc(
                "runs-on: ubuntu-latest\nservices:\n  db:\n    image: postgres\n",
                "bridge",
            );
            let (name, created) = rc.network_name();
            assert!(created, "a service forces act to create the network");
            assert_eq!(
                name,
                format!("{}-test-network", rc.job_container_name()),
                "the container name, the job id, then -network"
            );
            assert!(name.ends_with("-network"), "{name}");
        }

        /// The service branch is checked *before* the mode, so a configured
        /// `--network` does not suppress act's own network.
        #[test]
        fn services_win_over_a_configured_mode() {
            let rc = rc(
                "runs-on: ubuntu-latest\nservices:\n  db:\n    image: postgres\n",
                "host",
            );
            let (_, created) = rc.network_name();
            assert!(created, "the mode must not short-circuit the service branch");
        }
    }

    // ------------------------------------------- set_action_runtime_vars ----

    mod runtime_vars {
        use super::*;
        use crate::base64url;

        /// The three URL cases and the token, with the process environment
        /// neutralised by passing the overrides in.
        fn vars(
            config_env: &[(&str, &str)],
            url: Option<&str>,
            token: Option<&str>,
        ) -> BTreeMap<String, String> {
            let config_env: BTreeMap<String, String> = config_env
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect();
            let mut env = BTreeMap::new();
            set_action_runtime_vars_with(
                "myhost",
                "8000",
                &config_env,
                url,
                token,
                &mut env,
            );
            env
        }

        /// The `scp` claim, decoded from the token without verifying it —
        /// `ParseUnverified` in upstream's own test.
        fn scp(token: &str) -> String {
            let claims = token.split('.').nth(1).expect("a JWT has three parts");
            let bytes = base64url::decode(claims);
            let json: serde_json::Value =
                serde_json::from_slice(&bytes).expect("the claims are JSON");
            json["scp"]
                .as_str()
                .expect("an scp claim")
                .to_string()
        }

        /// Upstream `TestSetRuntimeVariables`: both URLs are the artifact
        /// server, and the token is a real three-part JWT.
        #[test]
        fn the_urls_come_from_the_artifact_server_and_the_token_is_a_jwt() {
            let env = vars(&[], None, None);
            assert_eq!(
                env.get("ACTIONS_RUNTIME_URL").map(String::as_str),
                Some("http://myhost:8000/")
            );
            assert_eq!(
                env.get("ACTIONS_RESULTS_URL").map(String::as_str),
                Some("http://myhost:8000/")
            );
            let token = env.get("ACTIONS_RUNTIME_TOKEN").expect("a token");
            assert_eq!(token.split('.').count(), 3, "not a JWT: {token}");
        }

        /// Upstream `TestSetRuntimeVariablesWithRunID`: `GITHUB_RUN_ID: 45`
        /// produces the scope `Actions.Results:45:45`, because upstream passes
        /// the same number as the task, run and job id.
        #[test]
        fn the_run_id_reaches_all_three_id_fields_of_the_scope() {
            let env = vars(&[("GITHUB_RUN_ID", "45")], None, None);
            let token = env.get("ACTIONS_RUNTIME_TOKEN").expect("a token");
            assert_eq!(scp(token), "Actions.Results:45:45");
        }

        /// With no `GITHUB_RUN_ID` the default is 1, and it reaches all three
        /// fields too.
        #[test]
        fn an_absent_run_id_defaults_to_one() {
            let env = vars(&[], None, None);
            let token = env.get("ACTIONS_RUNTIME_TOKEN").expect("a token");
            assert_eq!(scp(token), "Actions.Results:1:1");
        }

        /// The measured correction. `runID, _ = strconv.ParseInt(rid, 10, 64)`
        /// assigns the *value returned with the error*, so an unreadable run id
        /// is 0 — not the 1 that a Rust `unwrap_or(1)` produces. The default
        /// belongs to the absent case alone.
        ///
        /// Measured on v0.2.89 against the upstream block; upstream's own test
        /// covers only `"45"` and cannot see this.
        #[test]
        fn an_unparseable_run_id_is_zero_and_not_the_default() {
            for raw in ["", " 45", "45 ", "lots", "4.5", "0x2d", "-"] {
                let env = vars(&[("GITHUB_RUN_ID", raw)], None, None);
                let token = env.get("ACTIONS_RUNTIME_TOKEN").expect("a token");
                assert_eq!(
                    scp(token),
                    format!("Actions.Results:0:0"),
                    "GITHUB_RUN_ID={raw:?} must be 0 upstream, not 1"
                );
            }
        }

        /// The same correction, other direction: a *range* error clamps, so the
        /// value is neither 0 nor 1. This is the row that rules out
        /// `parse().unwrap_or(0)` as well.
        #[test]
        fn an_out_of_range_run_id_clamps_to_the_bound() {
            let env = vars(&[("GITHUB_RUN_ID", "99999999999999999999")], None, None);
            let token = env.get("ACTIONS_RUNTIME_TOKEN").expect("a token");
            assert_eq!(scp(token), "Actions.Results:9223372036854775807:9223372036854775807");
        }

        /// The environment wins over the artifact server, for the URL.
        #[test]
        fn a_url_override_replaces_the_artifact_server() {
            let env = vars(&[], Some("http://elsewhere:9999/"), None);
            assert_eq!(
                env.get("ACTIONS_RUNTIME_URL").map(String::as_str),
                Some("http://elsewhere:9999/")
            );
            assert_eq!(
                env.get("ACTIONS_RESULTS_URL").map(String::as_str),
                Some("http://elsewhere:9999/"),
                "RESULTS_URL is the same value, override included"
            );
        }

        /// An empty override means *not set* — Go's `os.Getenv` cannot tell an
        /// unset variable from an empty one, and upstream compares against
        /// `""`. An override must therefore not be reachable as an empty URL.
        #[test]
        fn an_empty_override_is_treated_as_unset() {
            let env = vars(&[], Some(""), Some(""));
            assert_eq!(
                env.get("ACTIONS_RUNTIME_URL").map(String::as_str),
                Some("http://myhost:8000/"),
                "an empty ACTIONS_RUNTIME_URL must fall back to the server"
            );
            let token = env.get("ACTIONS_RUNTIME_TOKEN").expect("a token");
            assert_eq!(scp(token), "Actions.Results:1:1", "and so must the token");
        }

        /// A token override is passed through verbatim and no run id is parsed,
        /// so a nonsense `GITHUB_RUN_ID` cannot change it.
        #[test]
        fn a_token_override_is_used_verbatim() {
            let env = vars(
                &[("GITHUB_RUN_ID", "lots")],
                None,
                Some("preexisting.token.value"),
            );
            assert_eq!(
                env.get("ACTIONS_RUNTIME_TOKEN").map(String::as_str),
                Some("preexisting.token.value")
            );
        }
    }

    // ------------------------------------------------------------ is_enabled --

    /// Upstream `createIfTestRunContext`, and the reason the whole
    /// `TestRunContextIsEnabled` fixture is built the way it is.
    ///
    /// The `platforms` map is what makes a plain job runnable at all: without
    /// `ubuntu-latest` in it, `isEnabled` would skip every non-reusable job for
    /// a missing image and the `if:` conditions would never be the thing under
    /// test. And `ExprEval` is built from the context *before* the first call,
    /// so `isEnabled` reads an already-assembled environment.
    mod enabled {
        use super::*;
        use crate::model::GithubContext;
        use crate::runner::expression::{new_expression_evaluator_with_env, RunStatus};

        /// `(job id, job yaml, its result)`.
        type JobSpec = (&'static str, &'static str, &'static str);

        fn rc_with_jobs(jobs: &[JobSpec], current: &str) -> RunContext {
            let mut source = String::from("name: test-workflow\njobs:\n");
            for (id, yaml, _) in jobs {
                // The id heads the first line only; every later line of the
                // same job is indented under it. Writing the id again would
                // declare the job twice and silently drop `runs-on:`.
                for (index, line) in yaml.lines().enumerate() {
                    if index == 0 {
                        source.push_str("  ");
                        source.push_str(id);
                        source.push_str(":\n");
                    }
                    source.push_str("    ");
                    source.push_str(line);
                    source.push('\n');
                }
            }
            let doc = Rc::new(Document::parse(&source).expect("the fixture parses"));
            let mut workflow =
                Workflow::from_document("test.yml", &doc).expect("the fixture decodes");
            for (id, _, result) in jobs {
                if let Some(job) = workflow.jobs.get_mut(*id) {
                    job.result = (*result).to_string();
                }
            }
            let config = RunConfig {
                workdir: ".".to_string(),
                platforms: [("ubuntu-latest".to_string(), "ubuntu-latest".to_string())]
                    .into_iter()
                    .collect(),
                ..RunConfig::default()
            };
            rc_with(config, Some(Run::new(workflow, doc, current)))
        }

        /// The three-step dance upstream does with `rc.ExprEval`: build the
        /// environment from the context, snapshot the status, then ask.
        fn is_enabled(mut rc: RunContext) -> Result<bool, String> {
            let env = rc.get_env();
            let github: GithubContext = rc.get_github_context(&no_git()).expect("builds");
            let environment = new_expression_evaluator_with_env(&rc, &env, &github);
            let status = RunStatus::new(&rc);
            rc.is_enabled(&environment, &status)
        }

        // success()

        /// Upstream case 1: a lone job with `if: success()` and no `needs:`.
        /// The implicit check is the condition itself, so it passes.
        #[test]
        fn success_with_no_needs_passes() {
            let rc = rc_with_jobs(&[("job1", "runs-on: ubuntu-latest\nif: success()", "")], "job1");
            assert!(is_enabled(rc).expect("no error"));
        }

        /// Upstream case 2: the whole point of `success()` — a needed job that
        /// failed takes its dependant with it.
        #[test]
        fn success_is_false_when_a_needed_job_failed() {
            let rc = rc_with_jobs(
                &[
                    ("job1", "runs-on: ubuntu-latest", "failure"),
                    ("job2", "runs-on: ubuntu-latest\nneeds: [job1]\nif: success()", ""),
                ],
                "job2",
            );
            assert!(!is_enabled(rc).expect("no error"));
        }

        /// Upstream case 3: the same shape with a successful need.
        #[test]
        fn success_is_true_when_a_needed_job_succeeded() {
            let rc = rc_with_jobs(
                &[
                    ("job1", "runs-on: ubuntu-latest", "success"),
                    ("job2", "runs-on: ubuntu-latest\nneeds: [job1]\nif: success()", ""),
                ],
                "job2",
            );
            assert!(is_enabled(rc).expect("no error"));
        }

        /// Upstream case 4: a *failed* job that is **not** needed. The
        /// condition is about this job's own needs, so the failure elsewhere in
        /// the workflow is irrelevant.
        #[test]
        fn success_ignores_a_failure_in_a_job_that_is_not_needed() {
            let rc = rc_with_jobs(
                &[
                    ("job1", "runs-on: ubuntu-latest", "failure"),
                    ("job2", "runs-on: ubuntu-latest\nif: success()", ""),
                ],
                "job2",
            );
            assert!(is_enabled(rc).expect("no error"));
        }

        // failure()

        /// Upstream case 5: `if: failure()` with no `needs:` at all.
        /// `jobFailure` walks an empty list, so it is false.
        #[test]
        fn failure_is_false_with_no_needs() {
            let rc = rc_with_jobs(&[("job1", "runs-on: ubuntu-latest\nif: failure()", "")], "job1");
            assert!(!is_enabled(rc).expect("no error"));
        }

        /// Upstream case 6: `if: failure()` is true exactly when a needed job
        /// failed.
        #[test]
        fn failure_is_true_when_a_needed_job_failed() {
            let rc = rc_with_jobs(
                &[
                    ("job1", "runs-on: ubuntu-latest", "failure"),
                    ("job2", "runs-on: ubuntu-latest\nneeds: [job1]\nif: failure()", ""),
                ],
                "job2",
            );
            assert!(is_enabled(rc).expect("no error"));
        }

        /// Upstream case 7: a needed job that succeeded, so `failure()` is
        /// false and the job is skipped rather than run.
        #[test]
        fn failure_is_false_when_the_needed_job_succeeded() {
            let rc = rc_with_jobs(
                &[
                    ("job1", "runs-on: ubuntu-latest", "success"),
                    ("job2", "runs-on: ubuntu-latest\nneeds: [job1]\nif: failure()", ""),
                ],
                "job2",
            );
            assert!(!is_enabled(rc).expect("no error"));
        }

        /// Upstream case 8: `if: failure()` with an unrelated failed job and no
        /// `needs:`. False, for the same reason as case 5.
        #[test]
        fn failure_ignores_a_failure_in_a_job_that_is_not_needed() {
            let rc = rc_with_jobs(
                &[
                    ("job1", "runs-on: ubuntu-latest", "failure"),
                    ("job2", "runs-on: ubuntu-latest\nif: failure()", ""),
                ],
                "job2",
            );
            assert!(!is_enabled(rc).expect("no error"));
        }

        // always()

        /// Upstream cases 9 and 12: `always()` reads nothing, so the condition
        /// is true no matter what the needs did.
        #[test]
        fn always_is_true_whatever_the_needs_did() {
            for (id, result) in [("job1", ""), ("job1", "failure"), ("job1", "success")] {
                let rc = rc_with_jobs(&[(id, "runs-on: ubuntu-latest\nif: always()", result)], "job1");
                assert!(is_enabled(rc).expect("no error"), "result {result:?}");
            }
        }

        /// Upstream cases 10 and 11: the same through a `needs:`.
        #[test]
        fn always_is_true_through_a_needed_job_in_either_state() {
            for result in ["failure", "success"] {
                let rc = rc_with_jobs(
                    &[
                        ("job1", "runs-on: ubuntu-latest", result),
                        ("job2", "runs-on: ubuntu-latest\nneeds: [job1]\nif: always()", ""),
                    ],
                    "job2",
                );
                assert!(is_enabled(rc).expect("no error"), "result {result:?}");
            }
        }

        // reusable workflow calls

        /// Upstream case 13: a `uses:` job returns **before** the image check.
        ///
        /// This is the case that would break if the image lookup were not left
        /// behind the `jobType != Default` return: a reusable workflow call is
        /// not a container and needs no image, so skipping it for a missing one
        /// would be wrong. The fixture's `platforms` map would hide the
        /// difference, so this test removes it on purpose.
        #[test]
        fn a_reusable_workflow_call_runs_without_an_image() {
            let source = "name: test-workflow\njobs:\n  job1:\n    uses: ./.github/workflows/reusable.yml\n";
            let doc = Rc::new(Document::parse(source).expect("the fixture parses"));
            let workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
            // No platforms at all: the only reason this job can still run is
            // that the image check is skipped.
            let rc = rc_with(RunConfig::default(), Some(Run::new(workflow, doc, "job1")));
            assert!(is_enabled(rc).expect("no error"));
        }

        /// Upstream case 14: the same job with `if: false` is skipped. The
        /// condition is checked *before* the job type, so the `if:` still
        /// decides even for a call.
        #[test]
        fn a_reusable_workflow_call_still_obeys_its_if() {
            let source = "name: test-workflow\njobs:\n  job1:\n    uses: ./.github/workflows/reusable.yml\n    if: false\n";
            let doc = Rc::new(Document::parse(source).expect("the fixture parses"));
            let workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
            let rc = rc_with(RunConfig::default(), Some(Run::new(workflow, doc, "job1")));
            assert!(!is_enabled(rc).expect("no error"));
        }

        /// A plain job with no image is skipped. The fixture always configures
        /// `ubuntu-latest`, so without this the `platformImage` branch would
        /// never be exercised at all.
        ///
        /// Measured on v0.2.89: `enabled=false`, and `result` stays `""`.
        #[test]
        fn a_plain_job_without_an_image_is_skipped() {
            let (enabled, result) = plain_job_image_probe(None);
            assert!(!enabled.expect("no error"), "no configured platform means no image");
            assert_eq!(result, "", "measured: the image path does not record a result");
        }

        /// A `platforms:` entry that maps to the **empty** string is the same as
        /// no entry. `runsOnImage` skips an empty image explicitly, so a mapping
        /// like `ubuntu-latest=` — which is what `-P ubuntu-latest=` produces —
        /// leaves the job unrunnable rather than giving it a nameless image.
        ///
        /// Measured on v0.2.89: `enabled=false`.
        #[test]
        fn a_platform_mapped_to_the_empty_string_counts_as_no_image() {
            let platforms = [("ubuntu-latest".to_string(), String::new())]
                .into_iter()
                .collect();
            let (enabled, _) = plain_job_image_probe(Some(platforms));
            assert!(!enabled.expect("no error"), "an empty image is not an image");
        }

        /// The two skips are not the same skip, and this is the test that says
        /// so. A falsy `if:` records `result("skipped")`; a missing image
        /// returns `false` and records **nothing**.
        ///
        /// Both were measured on v0.2.89, and the difference is not cosmetic:
        /// a dependant reading `needs.<job>.result` sees `"skipped"` in one
        /// case and `""` in the other, and `success()` treats `""` as a
        /// failure. Upstream only writes the result on the `if:` branch.
        #[test]
        fn skipping_for_a_missing_image_does_not_record_skipped() {
            let source = "name: test-workflow\njobs:\n  job1:\n    runs-on: ubuntu-latest\n";
            let doc = Rc::new(Document::parse(source).expect("the fixture parses"));
            let workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
            let mut rc = rc_with(RunConfig::default(), Some(Run::new(workflow, doc, "job1")));
            assert!(!is_enabled_ref(&mut rc).expect("no error"));
            assert_eq!(current_result(&rc), "", "measured: empty, not \"skipped\"");
        }

        /// Builds a lone `runs-on: ubuntu-latest` job with the given
        /// `platforms`, and reports `is_enabled` together with the result the
        /// job ended up recording.
        fn plain_job_image_probe(
            platforms: Option<BTreeMap<String, String>>,
        ) -> (Result<bool, String>, String) {
            let source = "name: test-workflow\njobs:\n  job1:\n    runs-on: ubuntu-latest\n";
            let doc = Rc::new(Document::parse(source).expect("the fixture parses"));
            let workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
            let config = RunConfig {
                platforms: platforms.unwrap_or_default(),
                ..RunConfig::default()
            };
            let mut rc = rc_with(config, Some(Run::new(workflow, doc, "job1")));
            let outcome = is_enabled_ref(&mut rc);
            (outcome, current_result(&rc))
        }

        fn current_result(rc: &RunContext) -> String {
            rc.run
                .as_ref()
                .and_then(|run| run.job())
                .map(|job| job.result.clone())
                .unwrap_or_default()
        }

        /// Not upstream: a skipped job records `result("skipped")`, which is
        /// what `needs.<job>.result` reports and what a dependant's
        /// `success()` reads. Not recording it leaves a dependant permanently
        /// unable to decide.
        #[test]
        fn a_skipped_job_records_its_result() {
            let source = "name: test-workflow\njobs:\n  job1:\n    runs-on: ubuntu-latest\n    if: false\n";
            let doc = Rc::new(Document::parse(source).expect("the fixture parses"));
            let workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
            let mut rc = rc_with(RunConfig::default(), Some(Run::new(workflow, doc, "job1")));
            assert!(!is_enabled_ref(&mut rc).expect("no error"));
            assert_eq!(current_result(&rc), "skipped", "measured: \"skipped\"");
        }

        /// [`is_enabled`] split in two, so a test can inspect the context
        /// afterwards — [`is_enabled`] takes `&mut self` and hands it back.
        fn is_enabled_ref(rc: &mut RunContext) -> Result<bool, String> {
            let env = rc.get_env();
            let github = rc.get_github_context(&no_git()).expect("builds");
            let environment = new_expression_evaluator_with_env(rc, &env, &github);
            let status = RunStatus::new(rc);
            rc.is_enabled(&environment, &status)
        }

        /// Not upstream: a malformed `if:` is the error the user sees, and the
        /// text is load-bearing — two leading spaces and a `❌` included.
        #[test]
        fn a_broken_if_reports_the_upstream_sentence() {
            let source = "name: test-workflow\njobs:\n  job1:\n    runs-on: ubuntu-latest\n    if: this is ( not valid\n";
            let doc = Rc::new(Document::parse(source).expect("the fixture parses"));
            let workflow = Workflow::from_document("test.yml", &doc).expect("decodes");
            let rc = rc_with(RunConfig::default(), Some(Run::new(workflow, doc, "job1")));
            let error = is_enabled(rc).expect_err("a broken if is an error");
            assert!(
                error.starts_with("  ❌  Error in if-expression: \"if: this is ( not valid\" ("),
                "got: {error}"
            );
        }
    }
}
