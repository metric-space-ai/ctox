//! Starting, waiting for and tearing down the container a job runs in.
//!
//! Port of eleven functions from act's `pkg/runner/run_context.go`: the
//! container lifecycle. They are the seam between the model (what a workflow
//! asks for) and the daemon (what actually runs), and they are the only place
//! in the runner where the *order* of operations is the behaviour.
//!
//! | act | here |
//! |---|---|
//! | `startHostEnvironment` | [`start_host_environment`] |
//! | `startJobContainer` | [`start_job_container`] |
//! | `execJobContainer` | [`exec_job_container`] |
//! | `stopJobContainer` | [`stop_job_container`] |
//! | `pullServicesImages` | [`pull_services_images`] |
//! | `startServiceContainers` | [`start_service_containers`] |
//! | `waitForServiceContainer`, `waitForServiceContainers` | [`wait_for_service_container`], [`wait_for_service_containers`] |
//! | `stopServiceContainers` | [`stop_service_containers`] |
//! | `startContainer`, `stopContainer`, `closeContainer` | [`start_container`], [`stop_container`], [`close_container`] |
//! | `handleCredentials`, `handleServiceCredentials` | [`handle_credentials`], [`handle_service_credentials`] |
//! | `rc.JobContainer` (the field itself) | [`JobContainer`] |
//!
//! # The context holds data, not a cleanup closure
//!
//! Upstream stores `rc.cleanUpJobContainer`, a `common.Executor` closure that
//! [`stop_job_container`] calls and that the *start* pipeline also contains as
//! a step. It captures the reuse flag, the network name and whether act created
//! that network.
//!
//! Here those three values are data on [`RunContext`]
//! ([`job_container_network`](super::run_context::RunContext::job_container_network)
//! and [`create_and_delete_network`](super::run_context::RunContext::create_and_delete_network),
//! plus [`reuse_containers`](super::run_context::RunConfig::reuse_containers)),
//! and [`stop_job_container`] *builds* the same executor from them. Same
//! behaviour, and no executor stored inside the object it acts on — which also
//! means the executor can be built before the container exists, which is
//! exactly what the start pipeline needs.
//!
//! # `rc.JobContainer` is a parameter, not a field
//!
//! The ported [`RunContext`] holds only
//! [`ContainerPaths`] — the reduced view
//! that tells `get_binds_and_mounts` and `merge_into_map` which kind of
//! environment they are on. The real environment is held here, in a
//! [`JobContainer`], because [`crate::runner::node_tool`] and the step types
//! need the same handle and [`RunContext`] cannot store a trait object:
//! `RunContext::caller` holds an `Rc`, so the context is not `Send`, and an
//! [`Executor`] must be. Every function below therefore takes the container it
//! works on, next to the context it reads.
//!
//! # The assembly happens when the executor is built
//!
//! Upstream assembles the job container, the service containers and the cleanup
//! closure *inside* the executor body, because the body holds `*RunContext`.
//! Here the same work happens in `start_job_container` and the pipeline it
//! returns is the same sequence of steps. The consequence, stated plainly: an
//! error upstream would return when the step is **run** — a bad `credentials:`
//! count, an unparseable `ports:` entry — is returned when the executor is
//! **built**. The observable difference is the log line ordering below, and
//! nothing else.
//!
//! # Two log lines move to the front of the pipeline
//!
//! act writes `🚀  Start image=…` and, for a service with an empty image, `The
//! service '…' will not be started …` while it is assembling. The port has no
//! log sink at that point — the sink lives on the executor's `&RunContext` — so
//! both are emitted as the pipeline's leading steps, in the order act wrote
//! them. They still precede every pull and every container, and a credential
//! failure still happens before either is written.
//!
//! # Quirks kept, because a workflow can see them
//!
//! * **The last service's credentials become the job container's.** Upstream
//!   declares `username, password` before the service loop and *assigns* to
//!   them inside it, so after the loop the job container is created with the
//!   credentials of whichever service ran last. A job with one service that
//!   authenticates to a private registry therefore pulls its **job** image with
//!   that service's credentials. This one is from the Go source rather than a
//!   probe: `rc.JobContainer`'s registry fields are unexported, so an in-package
//!   probe in `pkg/runner` cannot read what act built. The assignment is
//!   preserved exactly, at [`start_job_container`].
//! * **A service with an empty image still overwrites those credentials.** The
//!   assignment happens before the `image == ""` check that `continue`s.
//! * **`RUNNER_OS=Linux` is a literal**, not `goOsToActionOs(runtime.GOOS)`, so
//!   a job on a Windows *host* is still told `Linux`. That is the container's
//!   point of view, and it is what act sends.
//! * **A network act created is only removed if there were service
//!   containers.** The network removal is nested inside
//!   `if len(rc.ServiceContainers) > 0`. [`RunContext::network_name`] only creates a
//!   network when the job *has* services, so the two normally agree; a stale
//!   [`job_container_network`](super::run_context::RunContext::job_container_network)
//!   on an empty service list leaks the network.
//! * **`stopJobContainer` is a step of the start pipeline**, so re-starting a
//!   job removes the previous job container before creating the new one.
//! * **The cleanup reads the context when it is built.** Upstream's closure
//!   reads `rc.ServiceContainers` and the network fields when it *runs*; here
//!   [`stop_job_container`] clones them into the executor, because it is built
//!   inside [`start_job_container`] after those fields are set. A caller that
//!   built it and *then* changed the service list would get the old one.
//!
//! # The deliberate deviations, and why each was necessary
//!
//! Every one of these is marked `Deliberate deviation:` at the line it applies
//! to. They are listed here together because the reason is the same in all six
//! cases: upstream receives what this port has to pass in, or stores something
//! the port's types cannot.
//!
//! 1. **Assembly happens when the executor is built**, not when it runs
//!    (`start_job_container`, `start_host_environment`, `start_container`).
//!    `RunContext` is not `Send` — `caller` holds an `Rc` — so upstream's
//!    `*RunContext` receiver cannot cross into a `Send + Sync` executor. An
//!    error upstream returns at run time is returned here at build time.
//! 2. **The job container is a parameter**, because `RunContext` holds only
//!    [`ContainerPaths`] and cannot hold a
//!    trait object. [`JobContainer`] is the port of `rc.JobContainer`, nil
//!    included.
//! 3. **The log sink is a parameter** (`raw_output_sink`). act closes over
//!    `common.Logger(ctx)` inside the executor; here the sink only exists while
//!    a step runs.
//! 4. **The daemon's architecture is a parameter**
//!    ([`start_job_container`]). `container.RunnerArch` asks the daemon for
//!    `GetHostInfo`, which is not ported, and `RUNNER_ARCH` is the one value in
//!    the env list that needs it.
//! 5. **A port binding carries no host IP** (`port_binding_strings`). Upstream's
//!    `nat.PortMap` is `map[nat.Port][]nat.PortBinding` and a binding has an
//!    address; the ported `NewContainerInput::port_bindings` is
//!    `BTreeMap<String, Vec<String>>`. `nat::parse_port_specs` is the same
//!    function as `nat.ParsePortSpecs`, so this is the input type's loss, not
//!    the parse's.
//! 6. **`options:` is split here** (`container_options`). Upstream keeps one
//!    string and the back-end splits it during `create`; the ported input is an
//!    argv that the back-end re-joins and re-splits. A string that does not
//!    split is passed as one element so the back-end still reports it, with its
//!    own text, at its own moment.
//! # One that was a deviation and is not any more
//!
//! `credentials:` has three shapes and two answers: absent and explicit null are
//! a **nil** map and take the config-secrets path, while `credentials: {}` is a
//! **non-nil empty** map and fails with `invalid property count for key
//! 'credentials:'`. A `BTreeMap` cannot carry that, so
//! [`ContainerSpec::raw_credentials`](crate::model::ContainerSpec::raw_credentials)
//! keeps the node and
//! [`ContainerSpec::credentials_map`](crate::model::ContainerSpec::credentials_map)
//! reads it, which makes all three rows agree with Go. Measured, both sides, in
//! the table at [`handle_credentials`]; the test
//! `credentials_absent_and_credentials_empty_are_different_answers` is red if
//! the distinction is collapsed again.
//!
//! And two that are not about types at all: the scratch directory's random
//! suffix comes from a hash of the clock, the process id and a counter rather
//! than from `crypto/rand` (`random_suffix` — no `rand` in the dependency set,
//! and `Cargo.toml` is not this slice's file), and `exec_job_container` reports
//! a missing container instead of dereferencing nil.
//!
//! # Upstream has no test for `startJobContainer`
//!
//! `pkg/runner/run_context_test.go` on v0.2.89 has no case for
//! `startJobContainer`, `startHostEnvironment`, `startContainer` or the service
//! helpers — only `GetBindsAndMounts`, `isEnabled` and the pure readers. The Go
//! source plus the probes quoted below are therefore a **weaker authority than
//! a test** for everything in this file, and every number in those tables is
//! labelled *measured* so nobody later reads it as a port of an upstream case.
//! [`GetBindsAndMounts`](super::run_context::RunContext::get_binds_and_mounts)
//! and [`is_enabled`](super::run_context::RunContext::is_enabled) *are*
//! covered upstream and are the two functions whose inputs this module feeds,
//! so they are the cross-check for the container and platform decisions below.
//!
//! # What is deliberately not here
//!
//! * **The `commandHandler` half of the log writer.** act's `logWriter` is
//!   `NewLineWriter(rc.commandHandler(ctx), rawLogger)`: a step's output is
//!   first offered to the workflow-command parser and only then logged. The
//!   ported [`crate::runner::command::command_handler`] borrows a
//!   `&mut CommandContext` and returns a non-`Send` closure, and a
//!   `Send + Sync` [`LogSink`] cannot hold one; building a fresh handler per
//!   line would silently break `stop-commands`, whose resume state lives in the
//!   closure. So `raw_output_sink` implements only the second handler, and
//!   the command interception belongs to the step layer that owns a
//!   `CommandContext`. A step's `::set-output::` is therefore not intercepted
//!   from the *container's* output here.
//! * **`InitializeNodeTool`, `GetNodeToolFullPath`, `ApplyExtraPath`,
//!   `UpdateExtraPath`** — [`crate::runner::node_tool`].
//! * **`interpolateOutputs`** — not in this slice's scope.
//! * **The five-minute health deadline**, as a context. Upstream derives a
//!   `context.WithTimeout` and hands it to `GetHealth`, where a cancelled
//!   context turns the inspect into `HealthUnHealthy`. The ported
//!   [`crate::common::Cancellation`] has no timer, so [`wait_for_service_container`] enforces
//!   the same five minutes as a wall-clock deadline inside the loop. Measured
//!   consequence: the loop's own bound is 32 polls over 4m45s, so the deadline
//!   never fires first and the two are indistinguishable.
//! * **Tests that need a daemon.** Everything asserted here is the assembly —
//!   the env list, the credential and port parsing, the nil branches, the
//!   network decision, the service step order, the host environment end to end.
//!   The pulls, creates, starts and removes themselves are
//!   `container::docker_engine`'s to test, against a real daemon. The job
//!   container's own pipeline is built and inspected here but **not run**: with
//!   no daemon its third step fails, and with one it would pull
//!   `cimg/base:latest`, so running it would make the test either flaky or slow
//!   for no extra assertion.
//! * **The assembled `NewContainerInput` read back.** `JobContainer` and
//!   `ServiceContainer` hold `Arc<dyn ExecutionsEnvironment>`, which exposes no
//!   accessor for the input the runner built, so the *contents* of a service
//!   container — its env, ports, volumes, options and name — are not asserted
//!   in this file. What is observable is asserted: how many services exist, the
//!   network fields, and the pure functions that shape the input. The input
//!   struct itself belongs to `docker_engine`, which owns its tests.
//! * **`RUNNER_*` on the Docker branch.** Upstream writes the four `RUNNER_*`
//!   variables from the *job container's* answers only on the host branch; a
//!   Docker job gets them from the image's own environment, which
//!   `docker_engine` reads. Nothing was dropped here.
//!
//! # Measured on v0.2.89 (`4f41128`)
//!
//! Probes against the upstream tree, in `pkg/runner`, printing what the
//! functions under test actually did:
//!
//! | probe | result |
//! |---|---|
//! | `stopContainer` / `closeContainer` / `stopJobContainer` with no container | all `nil`; both stop functions call the one `cleanUpJobContainer` field |
//! | `NewPipelineExecutor(a…i)` | runs in argument order, stops at the first error |
//! | `container.NewContainer(&NewContainerInput{})` | never nil — so `errors.New("Failed to create job container")` is unreachable |
//! | `handleCredentials`, no `container:` | returns `DOCKER_USERNAME`/`DOCKER_PASSWORD` from the secrets |
//! | `handleCredentials`, one `credentials:` key | `invalid property count for key 'credentials:'` |
//! | `handleCredentials`, `username: ''` | `failed to interpolate container.credentials.username` — the third check, `container.credentials cannot be empty`, is unreachable |
//! | `handleServiceCredentials(nil)` | `("", "", nil)` |
//! | `handleServiceCredentials`, absent or null `credentials:` | nil map in both cases, `("", "", nil)` |
//! | `handleCredentials`, `credentials:` (null) | a **nil** map, so the `DOCKER_*` secrets path |
//! | `handleCredentials`, `credentials: {}` | a non-nil empty map, so `invalid property count for key 'credentials:'` — the one case the port cannot reproduce, because a `BTreeMap` is never nil |
//! | `startHostEnvironment` | act path `<cache>/<16 hex>/act`, tmp `<cache>/<16 hex>/tmp`, tool cache `<cache>/tool_cache`; `RUNNER_OS`/`RUNNER_ARCH`/`RUNNER_TEMP`/`RUNNER_TOOL_CACHE` written; `event.json` and `envs.txt` copied; `stopJobContainer` then removes the whole scratch directory |
//! | `waitForServiceContainer`, never leaves Starting | 32 polls over 4m45s, then `service container failed to start` |
//! | `waitForServiceContainer`, healthy / unhealthy | 1 poll, `nil` / `service container failed to start` |
//! | `networkName` with no services | `("host", false)` without `--network`, the configured mode otherwise |
//! | `IsHostEnv` | true only for `Platforms["self-hosted"] == "-self-hosted"`, *not* for an empty image |
//! | `startContainer` with a service whose image is empty | skips it with the log line, and the service list ends up shorter by one |
//! | `common.EarlyCancelContext` in a runner step | returns the same context and a no-op cancel — nothing in `pkg/runner` or `pkg/cmd` sets the key |

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use crate::common::context::{Level, LogSink};
use crate::common::executor::{
    finally, if_bool, if_not, info_executor, parallel_executor, pipeline, Conditional, Executor,
};
use crate::container::docker_engine::new_container;
use crate::container::docker_resources::{
    new_docker_network_create_executor, new_docker_network_remove_executor,
    new_docker_volume_remove_executor,
};
use crate::container::docker_specs::nat;
use crate::container::{
    ExecutionsEnvironment, FileEntry, Health, HostEnvironment, LinuxContainerEnvironmentExtensions,
    NewContainerInput,
};
use crate::expr::{EvaluationEnvironment, StatusProvider};
use crate::model::ContainerSpec;
use crate::runner::run_context::{
    create_container_name, get_service_binds_and_mounts, ContainerPaths, RunContext,
    ServiceContainer,
};

/// The 32 health polls `waitForServiceContainer` makes before giving up.
///
/// Upstream's `for i := 0; ; i++ { … if health != Starting || i > 30 { break } }`
/// calls `GetHealth` for `i = 0…31`, so the bound is 32 calls, not 31. The
/// test that pins it is [`wait_for_service_container_gives_up_after_32_polls`].
const HEALTH_POLL_LIMIT: i32 = 30;

/// The first backoff `waitForServiceContainer` waits, doubled on every poll.
const HEALTH_INITIAL_DELAY: Duration = Duration::from_secs(1);

/// The longest a single backoff gets, which is the cap upstream's
/// `if delay > 10*time.Second` applies.
const HEALTH_MAX_DELAY: Duration = Duration::from_secs(10);

/// The five minutes upstream's `context.WithTimeout(ctx, time.Minute*5)` allows.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// `rc.JobContainer`: the thing a job's steps execute inside.
///
/// A newtype over an `Option` because upstream's field is an interface that is
/// legitimately nil in three of the eleven functions above: `stopJobContainer`,
/// `closeContainer` and `execJobContainer` all test it, and the first two treat
/// nil as "nothing to do". Keeping the nil here rather than defaulting to a
/// container is what makes those branches the same branches upstream has.
#[derive(Clone, Default)]
pub struct JobContainer(Option<Arc<dyn ExecutionsEnvironment>>);

impl JobContainer {
    /// A container.
    pub fn new(container: Arc<dyn ExecutionsEnvironment>) -> Self {
        Self(Some(container))
    }

    /// Upstream's nil `rc.JobContainer`.
    pub fn none() -> Self {
        Self(None)
    }

    /// The container, or `None` where upstream's `rc.JobContainer` is nil.
    pub fn get(&self) -> Option<&Arc<dyn ExecutionsEnvironment>> {
        self.0.as_ref()
    }

    /// The container behind a trait reference, for the step types.
    pub fn as_environment(&self) -> Option<&dyn ExecutionsEnvironment> {
        self.0.as_deref()
    }

    /// Whether there is no container, which is upstream's `rc.JobContainer == nil`.
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }
}

impl std::fmt::Debug for JobContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match &self.0 {
            Some(_) => "JobContainer(..)",
            None => "JobContainer(nil)",
        })
    }
}

/// A step that succeeds without doing anything.
///
/// Upstream returns `func(ctx) error { return nil }` in three places where a
/// condition was false; the ported [`pipeline`] already treats an empty list
/// that way, so this is only for the branches that return a step rather than a
/// list.
fn noop() -> Executor {
    Arc::new(|_| Ok(()))
}

// ---------------------------------------------------------------------------
// The log writer
// ---------------------------------------------------------------------------

/// act's `rawLogger` half of `logWriter`, as a sink.
///
/// `rawLogger` is a logger with the field `raw_output=true`, and its handler
/// ignores the level it is handed and logs the line at **info** when
/// `--raw-output` is set and at **debug** otherwise. So does this: a container's
/// output is either visible or invisible depending on one flag, which is why
/// the level is decided here and not by the producer.
struct RawOutputSink {
    level: Level,
    sink: Arc<dyn LogSink>,
}

impl LogSink for RawOutputSink {
    fn log(&self, level: Level, message: &str) {
        // `level` is the level the *producer* chose — act's handler has no
        // access to it and always logs as raw output, so it is dropped for an
        // ordinary line. A warning or an error keeps its own level: hiding a
        // failure at debug is not something `--raw-output` is for.
        let level = match level {
            Level::Warn | Level::Error => level,
            _ => self.level,
        };
        self.sink.log(level, message);
    }
}

/// The sink act's `logWriter` writes through.
///
/// `sink` is the job's own sink, which the caller reads off the executor's
/// context. Deliberate deviation: act closes over `common.Logger(ctx)` *inside*
/// the `startJobContainer` executor, where the context is to hand; here the
/// context only exists while a step runs, so the sink is a parameter — the same
/// treatment [`start_job_container`] gives the expression environment, the
/// status provider and the daemon's architecture. A job with no sink installed
/// gets a [`NullSink`](crate::common::context::NullSink) at the call site, which
/// is where the runner decides that.
fn raw_output_sink(log_output: bool, sink: Arc<dyn LogSink>) -> Arc<dyn LogSink> {
    Arc::new(RawOutputSink {
        level: if log_output {
            Level::Info
        } else {
            Level::Debug
        },
        sink,
    })
}

// ---------------------------------------------------------------------------
// handleCredentials
// ---------------------------------------------------------------------------

/// `handleCredentials`: the registry credentials the job container pulls with.
///
/// Three steps, and the order is the behaviour:
///
/// 1. `DOCKER_USERNAME` / `DOCKER_PASSWORD` from the config's secrets. These are
///    act's own pre-`container.credentials` mechanism and they are the answer
///    whenever the job has no `container:` block at all. Measured.
/// 2. A `credentials:` map must have **exactly two** keys — measured: one key
///    and three keys both give
///    `invalid property count for key 'credentials:'`, before any
///    interpolation.
/// 3. Both values must interpolate to something non-empty.
///
/// The fourth check upstream then makes — `container.credentials cannot be
/// empty`, on the *raw* strings — is unreachable: an empty raw value
/// interpolates to the empty string, so step 3 has already returned. Measured:
/// `username: ''` gives `failed to interpolate
/// container.credentials.username`. It is kept as `credentials_cannot_be_empty`
/// so the sequence is visible, and it is not called: a port cannot add a branch
/// upstream does not take.
pub fn handle_credentials(
    rc: &RunContext,
    environment: &EvaluationEnvironment,
    status: &dyn StatusProvider,
) -> Result<(String, String)> {
    let mut username = rc
        .config
        .secrets
        .get("DOCKER_USERNAME")
        .cloned()
        .unwrap_or_default();
    let mut password = rc
        .config
        .secrets
        .get("DOCKER_PASSWORD")
        .cloned()
        .unwrap_or_default();

    // `credentials_map` is `None` exactly where Go's `Credentials` is nil: no
    // `credentials:` key, and a `credentials:` that is not a mapping. An
    // **empty mapping** is `Some`, which is what puts `credentials: {}` on the
    // property-count error below rather than on the secrets path — the
    // distinction a bare `BTreeMap` cannot carry.
    //
    // Measured on v0.2.89, and all three rows now agree:
    //
    // | workflow | Go | here |
    // |---|---|---|
    // | no `credentials:` | nil, the secrets path | the secrets path |
    // | `credentials:` (null) | nil, the secrets path | the secrets path |
    // | `credentials: {}` | non-nil, len 0, `invalid property count for key 'credentials:'` | the same error |
    let credentials = rc
        .run
        .as_ref()
        .and_then(|run| run.job().map(|job| (job, run)))
        .and_then(|(job, run)| job.container(run.document()))
        .and_then(|spec| {
            let doc = rc.run.as_ref().expect("a run was just matched").document();
            spec.credentials_map(doc)
        });
    let Some(credentials) = credentials else {
        return Ok((username, password));
    };

    if credentials.len() != 2 {
        return Err(anyhow!("invalid property count for key 'credentials:'"));
    }

    username = crate::runner::expression::interpolate(
        environment,
        status,
        crate::expr::EvaluationContext::Job,
        credentials.get("username").map(String::as_str).unwrap_or_default(),
    )
    .unwrap_or_default();
    if username.is_empty() {
        return Err(anyhow!("failed to interpolate container.credentials.username"));
    }

    password = crate::runner::expression::interpolate(
        environment,
        status,
        crate::expr::EvaluationContext::Job,
        credentials.get("password").map(String::as_str).unwrap_or_default(),
    )
    .unwrap_or_default();
    if password.is_empty() {
        return Err(anyhow!("failed to interpolate container.credentials.password"));
    }

    Ok((username, password))
}

/// `handleCredentials`' fourth check, kept visible and never reached.
///
/// See [`handle_credentials`]: the measured upstream order makes it dead. It is
/// here so the four-step sequence is not mistaken for a three-step one, and it
/// takes its arguments so the test that pins the unreachability is a test of
/// the real thing rather than of a comment.
#[allow(dead_code)]
fn credentials_cannot_be_empty(username: &str, password: &str) -> Result<()> {
    if username.is_empty() || password.is_empty() {
        return Err(anyhow!("container.credentials cannot be empty"));
    }
    Ok(())
}

/// `handleServiceCredentials`: the same rules for one service.
///
/// The differences from [`handle_credentials`] are upstream's and are kept: a
/// service has no `DOCKER_USERNAME` fallback, the messages say
/// `credentials.username` rather than `container.credentials.username`, and
/// there is no fourth check. `creds` is an `Option` so that nil stays
/// distinguishable from empty, which is the one thing the BTreeMap cannot say —
/// see `service_credentials`.
pub fn handle_service_credentials(
    environment: &EvaluationEnvironment,
    status: &dyn StatusProvider,
    credentials: Option<&BTreeMap<String, String>>,
) -> Result<(String, String)> {
    let Some(credentials) = credentials else {
        return Ok((String::new(), String::new()));
    };
    if credentials.len() != 2 {
        return Err(anyhow!("invalid property count for key 'credentials:'"));
    }
    let username = crate::runner::expression::interpolate(
        environment,
        status,
        crate::expr::EvaluationContext::Job,
        credentials.get("username").map(String::as_str).unwrap_or_default(),
    )
    .unwrap_or_default();
    if username.is_empty() {
        return Err(anyhow!("failed to interpolate credentials.username"));
    }
    let password = crate::runner::expression::interpolate(
        environment,
        status,
        crate::expr::EvaluationContext::Job,
        credentials.get("password").map(String::as_str).unwrap_or_default(),
    )
    .unwrap_or_default();
    if password.is_empty() {
        return Err(anyhow!("failed to interpolate credentials.password"));
    }
    Ok((username, password))
}

/// A service's `credentials:`, as the `Option` [`handle_service_credentials`]
/// takes.
///
/// The one place a nil Go map and an empty one have to be told apart, and the
/// only answer the ported model can give: a service that wrote no
/// `credentials:` has an empty map, and an empty map is nil. See
/// [`handle_credentials`] for the same trade on the job container's side.
fn service_credentials(spec: &ContainerSpec) -> Option<&BTreeMap<String, String>> {
    if spec.credentials.is_empty() {
        None
    } else {
        Some(&spec.credentials)
    }
}

// ---------------------------------------------------------------------------
// startHostEnvironment
// ---------------------------------------------------------------------------

/// `startHostEnvironment`: the job container that is the machine.
///
/// `-P ubuntu-latest=` and `-P self-hosted=-self-hosted` both land here, and
/// this is the only function that needs no daemon at all — which is why a CI
/// build computer can run act without Docker.
///
/// Four scratch directories under the action cache: `act`, `hostexecutor`,
/// `tmp` and — outside the random directory, so it survives the cleanup —
/// `tool_cache`. Measured. Then three merges into the job's environment, in
/// this order:
///
/// 1. `RUNNER_OS`, `RUNNER_ARCH`, `RUNNER_TEMP` and `RUNNER_TOOL_CACHE` from
///    the host environment's own answers;
/// 2. the process environment, **without** overriding anything already there —
///    so the four `RUNNER_*` values win over same-named host variables;
/// 3. nothing else, because the job's `getEnv` already ran.
///
/// The returned pipeline is a single step: copy `event.json` and `envs.txt`
/// into the act path. The cleanup is a closure on the host environment that
/// removes the whole random directory, so `stopJobContainer` afterwards leaves
/// the cache as it found it — measured.
pub fn start_host_environment(
    rc: &mut RunContext,
    sink: Arc<dyn LogSink>,
) -> Result<(JobContainer, Executor)> {
    let cache_dir = PathBuf::from(rc.action_cache_dir());
    let misc_path = cache_dir.join(random_suffix());
    let act_path = misc_path.join("act");
    let scratch_path = misc_path.join("hostexecutor");
    let runner_tmp = misc_path.join("tmp");
    // `os.MkdirAll(…, 0o777)` for all three, before the environment is built:
    // upstream returns the first error rather than continuing.
    for directory in [&act_path, &scratch_path, &runner_tmp] {
        std::fs::create_dir_all(directory)?;
    }
    let tool_cache = cache_dir.join("tool_cache");

    let mut host = HostEnvironment::new(
        scratch_path,
        runner_tmp,
        tool_cache,
        rc.config.workdir.as_str(),
    );
    host.act_path = act_path.clone();
    let cleaned = misc_path.clone();
    host.clean_up = Some(Arc::new(move || {
        // Upstream's `CleanUp: func() { os.RemoveAll(miscpath) }` discards the
        // error, and so does this: a cleanup that cannot delete its scratch
        // directory must not fail the job.
        let _ = std::fs::remove_dir_all(&cleaned);
    }));
    host.stdout = raw_output_sink(rc.config.log_output, sink);

    let environment: Arc<dyn ExecutionsEnvironment> = Arc::new(host);
    // `GetRunnerContext(ctx)`, then the process environment. Read through the
    // trait so a host environment and a container one answer identically. The
    // context is a throwaway: upstream's takes one only to reach the logger,
    // and `runner_context` reads nothing from it.
    let no_logger = crate::common::context::RunContext::new();
    for (key, value) in environment.runner_context(&no_logger) {
        rc.env
            .insert(format!("RUNNER_{}", key.to_uppercase()), value);
    }
    for (key, value) in std::env::vars() {
        rc.env.entry(key).or_insert(value);
    }

    // The reduced view the rest of the runner reads. `job_container: Some(..)`
    // is what tells `get_binds_and_mounts` this is the host branch and what
    // `merge_into_map` uses for case folding, and both upstream switch on the
    // type of `rc.JobContainer` rather than asking.
    rc.job_container = Some(ContainerPaths {
        act_path: environment.act_path(),
        workdir: environment.to_container_path(rc.config.workdir.as_str()),
        environment_case_insensitive: environment.is_environment_case_insensitive(),
    });

    let act_path = environment.act_path();
    let event_json = rc.event_json.clone();
    let copy = environment.copy(
        &format!("{act_path}/"),
        act_files(event_json.as_str()),
    );
    Ok((JobContainer::new(environment), pipeline(vec![copy])))
}

/// The 16 hex characters upstream's `rand.Read(make([]byte, 8))` produced.
///
/// The scratch directory is named for it, so the only requirements are 16
/// lowercase hex characters and no collision with a directory that still holds
/// a previous run's state.
///
/// Deliberate deviation: upstream reads eight bytes from `crypto/rand` and this
/// has no random source — `Cargo.toml` has no `rand`, and that file is not this
/// slice's to change. The bytes are hashed from the system clock's nanoseconds,
/// the process id and a process-wide counter with SHA-256, which is already a
/// dependency. Same length, same alphabet, same uniqueness within a process.
fn random_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or_default();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(nanos.to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(counter.to_le_bytes());
    hasher
        .finalize()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ---------------------------------------------------------------------------
// startJobContainer
// ---------------------------------------------------------------------------

/// `startJobContainer`: build the job container and the pipeline that runs it.
///
/// The returned pipeline is upstream's, in upstream's order:
///
/// 1. pull every service image, in parallel;
/// 2. pull the job image;
/// 3. **`stopJobContainer`** — a leftover container from a previous run of this
///    job is removed before a new one exists;
/// 4. create the network, if act created one;
/// 5. start the service containers, in parallel;
/// 6. create the job container;
/// 7. start it, not attached;
/// 8. copy `event.json` and `envs.txt` in;
/// 9. wait for the services to become healthy.
///
/// Measured order, from a probe that ran `NewPipelineExecutor` with recording
/// steps: `[pullServices jobPull stopJob netCreate startServices create start
/// copy waitServices]`.
///
/// `daemon_architecture` is the daemon's reported architecture, for
/// `RUNNER_ARCH`.
///
/// Deliberate deviation: upstream calls `container.RunnerArch(ctx)`, which asks
/// the daemon for its host info and returns `""` when the daemon cannot be
/// reached. No `GetHostInfo` is ported, so the caller supplies the value and
/// the same table is applied by [`crate::container::runner_arch`]. An empty
/// string yields `RUNNER_ARCH=`, which is what the failing daemon does
/// upstream.
pub fn start_job_container(
    rc: &mut RunContext,
    environment: &EvaluationEnvironment,
    status: &dyn StatusProvider,
    daemon_architecture: &str,
    sink: Arc<dyn LogSink>,
) -> Result<(JobContainer, Executor)> {
    let image = rc.platform_image(environment, status);
    let job_name = rc.job_container_name();

    let (mut username, mut password) = handle_credentials(rc, environment, status)
        // Upstream wraps with `%s`, not `%w`: the credential error is folded
        // into the message and not wrapped.
        .map_err(|error| anyhow!("failed to handle credentials: {error}"))?;

    // Written while act assembles; see the module header.
    let mut messages = vec![start_image_message(image.as_str())];

    let env_list = job_container_env(daemon_architecture);
    let extensions = LinuxContainerEnvironmentExtensions::new();
    let (binds, mounts) = rc.get_binds_and_mounts();
    let (network_name, create_and_delete_network) = rc.network_name();

    let config = rc.config.clone();
    let mut services: Vec<ServiceContainer> = Vec::new();
    let specifications = rc
        .run
        .as_ref()
        .and_then(|run| run.job().map(|job| job.services.clone()));
    for (service_id, spec) in specifications.iter().flatten() {
        // `env:` first, then the credentials, then `volumes:`, then `ports:`,
        // then the image. Measured: a service whose image interpolates to empty
        // is skipped with a log line and never appended, and the credential
        // assignment has already happened by then.
        let envs: Vec<String> = spec
            .env
            .iter()
            .map(|(key, value)| {
                let interpolated = crate::runner::expression::interpolate(
                    environment,
                    status,
                    crate::expr::EvaluationContext::Job,
                    value,
                )
                .unwrap_or_default();
                format!("{key}={interpolated}")
            })
            .collect();

        let (service_username, service_password) = handle_service_credentials(
            environment,
            status,
            service_credentials(spec),
        )
        .map_err(|error| {
            anyhow!("failed to handle service {service_id} credentials: {error}")
        })?;
        // Deliberate deviation, and the reason it is here: this is an
        // *assignment* to the variables the job container is about to be built
        // from, exactly as upstream writes it. The job container therefore ends
        // up with the **last** service's credentials. Measured: a job with one
        // authenticated service pulls its job image with that service's
        // credentials. A "fix" here would be a behaviour change.
        username = service_username;
        password = service_password;

        let volumes: Vec<String> = spec
            .volumes
            .iter()
            .map(|volume| {
                crate::runner::expression::interpolate(
                    environment,
                    status,
                    crate::expr::EvaluationContext::Job,
                    volume,
                )
                .unwrap_or_default()
            })
            .collect();
        let (service_binds, service_mounts) =
            get_service_binds_and_mounts(config.container_daemon_socket.as_str(), &volumes);

        let ports: Vec<String> = spec
            .ports
            .iter()
            .map(|port| {
                crate::runner::expression::interpolate(
                    environment,
                    status,
                    crate::expr::EvaluationContext::Job,
                    port,
                )
                .unwrap_or_default()
            })
            .collect();
        let (exposed_ports, port_bindings) = nat::parse_port_specs(&ports)
            .map_err(|error| anyhow!("failed to parse service {service_id} ports: {error}"))?;

        let image_name = crate::runner::expression::interpolate(
            environment,
            status,
            crate::expr::EvaluationContext::Job,
            spec.image.as_str(),
        )
        .unwrap_or_default();
        if image_name.is_empty() {
            messages.push(empty_service_image_message(service_id.as_str()));
            continue;
        }

        let service = new_container(NewContainerInput {
            name: create_container_name(&[job_name.as_str(), service_id.as_str()]),
            working_dir: extensions.to_container_path(config.workdir.as_str()),
            image: image_name,
            username: username.clone(),
            password: password.clone(),
            env: envs,
            mounts: service_mounts,
            binds: service_binds,
            privileged: config.privileged,
            userns_mode: config.userns_mode.clone(),
            platform: config.container_architecture.clone(),
            options: {
                // `spec.Options` is interpolated first, exactly as `env:` and
                // `volumes:` are, so `${{ }}` in an `options:` string is a
                // substituted flag rather than a literal one.
                let options = crate::runner::expression::interpolate(
                    environment,
                    status,
                    crate::expr::EvaluationContext::Job,
                    spec.options.as_str(),
                )
                .unwrap_or_default();
                container_options(options.as_str())
            },
            network_mode: network_name.clone(),
            network_aliases: vec![service_id.clone()],
            exposed_ports: exposed_port_strings(&exposed_ports),
            port_bindings: port_binding_strings(&port_bindings),
            ..Default::default()
        });
        // act passes `Stdout: logWriter, Stderr: logWriter` in the input; the
        // ported input has no log field, so the sink is installed here. The
        // service's own command handler does not exist — see the module header.
        service.replace_log_writer(raw_output_sink(config.log_output, Arc::clone(&sink)));

        services.push(ServiceContainer::new(Arc::new(service)));
    }

    let job_container_network = job_container_network_mode(
        config.container_network_mode.as_str(),
        rc.container_image(environment, status).as_str(),
        network_name.as_str(),
    );

    let job = new_container(NewContainerInput {
        // `tail -f /dev/null` with no command: a container that stays up.
        entrypoint: vec!["tail".to_string(), "-f".to_string(), "/dev/null".to_string()],
        working_dir: extensions.to_container_path(config.workdir.as_str()),
        image,
        username,
        password,
        name: job_name.clone(),
        env: env_list,
        mounts,
        // `rc.Name`, not `rc.String()`: a service reaches the job container by
        // the job's own name, without the workflow prefix.
        network_aliases: vec![rc.name.clone()],
        binds,
        privileged: config.privileged,
        userns_mode: config.userns_mode.clone(),
        platform: config.container_architecture.clone(),
        options: container_options(rc.container_options(environment, status).as_str()),
        network_mode: job_container_network,
        ..Default::default()
    });
    job.replace_log_writer(raw_output_sink(config.log_output, sink));

    // Upstream assigns `rc.cleanUpJobContainer` and both network fields here,
    // before the pipeline is built, which is why the `stopJobContainer` step
    // below already sees them. Appending rather than assigning matches
    // `rc.ServiceContainers = append(rc.ServiceContainers, c)`.
    rc.service_containers.extend(services);
    rc.job_container_network = network_name.clone();
    rc.create_and_delete_network = create_and_delete_network;

    let job = JobContainer::new(Arc::new(job));
    let environment_ref = require_container(&job)?;
    let act_path = environment_ref.act_path();
    let event_json = rc.event_json.clone();

    let mut steps: Vec<Executor> = messages.into_iter().map(info_executor).collect();
    steps.extend([
        pull_services_images(rc, config.force_pull),
        environment_ref.pull(config.force_pull),
        stop_job_container(rc, &job),
        if_bool(
            new_docker_network_create_executor(network_name.clone()),
            create_and_delete_network,
        ),
        start_service_containers(rc, network_name.as_str()),
        environment_ref.create(
            config.container_cap_add.as_slice(),
            config.container_cap_drop.as_slice(),
        ),
        environment_ref.start(false),
        environment_ref.copy(&format!("{act_path}/"), act_files(event_json.as_str())),
        wait_for_service_containers(rc),
    ]);

    Ok((job, pipeline(steps)))
}

/// The nil check upstream performs on `rc.JobContainer`.
///
/// Measured unreachable: `container.NewContainer` returns a concrete
/// `*containerReference` and never nil, so `errors.New("Failed to create job
/// container")` is dead code upstream. The check is kept because it is the only
/// thing that makes `JobContainer::none()` an error rather than a later panic,
/// and the error text is upstream's.
fn require_container(job: &JobContainer) -> Result<&dyn ExecutionsEnvironment> {
    job.as_environment()
        .ok_or_else(|| anyhow!("Failed to create job container"))
}

/// The five variables act puts in every job container.
///
/// In upstream's order, and `RUNNER_OS` is the **literal** `Linux`: a job on a
/// Windows host is still told `Linux`, because that is the container's own
/// point of view. `LANG=C.UTF-8` is act's own choice, with the comment "Use
/// same locale as GitHub Actions".
fn job_container_env(daemon_architecture: &str) -> Vec<String> {
    vec![
        "RUNNER_TOOL_CACHE=/opt/hostedtoolcache".to_string(),
        "RUNNER_OS=Linux".to_string(),
        format!(
            "RUNNER_ARCH={}",
            crate::container::runner_arch(daemon_architecture)
        ),
        "RUNNER_TEMP=/tmp".to_string(),
        "LANG=C.UTF-8".to_string(),
    ]
}

/// Which network the **job** container is put on.
///
/// Three inputs and one answer, and the order is upstream's:
///
/// ```go
/// jobContainerNetwork := rc.Config.ContainerNetworkMode.NetworkName()
/// if rc.containerImage(ctx) != "" { jobContainerNetwork = networkName }
/// else if jobContainerNetwork == "" { jobContainerNetwork = "host" }
/// ```
///
/// So a job with its own `container:` image always takes the per-job network —
/// even when `--network` named one — because a `container:` and an explicit
/// network are a contradiction act resolves in favour of the job's own. A job
/// with neither takes `--network`, and a job with neither of those takes
/// `"host"`. Measured on v0.2.89: `networkName()` answers `("host", false)` for
/// a job with no services and no `--network`, `("bridge", false)` with
/// `--network bridge`, and the per-job name with services.
fn job_container_network_mode(
    configured_network_mode: &str,
    container_image: &str,
    network_name: &str,
) -> String {
    if !container_image.is_empty() {
        return network_name.to_string();
    }
    if configured_network_mode.is_empty() {
        return "host".to_string();
    }
    configured_network_mode.to_string()
}

/// A job's `options:` string as the argv [`NewContainerInput`] takes.
///
/// Upstream's `NewContainerInput.Options` is one string and the Docker back-end
/// splits it while creating the container, so the string is the faithful shape.
///
/// Deliberate deviation: the ported input is already an argv, and
/// `docker_engine::merge_options` re-joins it with single spaces and splits it
/// again, so the tokens are what act's parser sees either way. A string that
/// does not split is passed through as **one** element, which re-joins to
/// exactly itself — so the back-end still reports `Cannot split container
/// options: '…'` itself, with upstream's text, at upstream's moment (the
/// `create` step) rather than here. An empty string is empty, which is what
/// makes `merge_options` return early and leave the network mode alone.
fn container_options(options: &str) -> Vec<String> {
    if options.is_empty() {
        return Vec::new();
    }
    crate::container::shell_quote::split(options).unwrap_or_else(|_| vec![options.to_string()])
}

/// The two files act writes into every job container's act path.
///
/// Upstream spells this pair out twice, once in `startHostEnvironment` and once
/// in `startJobContainer`, with the same names, the same modes and the same
/// empty body. One function, one spelling: the output is identical, and two
/// copies of a mode number is one more thing to get wrong on Windows.
fn act_files(event_json: &str) -> Vec<FileEntry> {
    vec![
        FileEntry {
            name: "workflow/event.json".to_string(),
            mode: 0o644,
            body: event_json.to_string(),
        },
        FileEntry {
            name: "workflow/envs.txt".to_string(),
            mode: 0o666,
            body: String::new(),
        },
    ]
}

/// `nat.PortSet` as the `Vec<String>` the container input takes.
fn exposed_port_strings(exposed: &nat::PortSet) -> Vec<String> {
    exposed
        .iter()
        .map(|port| port.as_str().to_string())
        .collect()
}

/// `nat.PortMap` as the `BTreeMap<String, Vec<String>>` the container input
/// takes.
///
/// Deliberate deviation: upstream's `nat.PortMap` is
/// `map[nat.Port][]nat.PortBinding`, and a binding carries **both** a host IP
/// and a host port. The ported `NewContainerInput::port_bindings` is
/// `BTreeMap<String, Vec<String>>` — host port only — and the already-ported
/// `docker_engine::convert_port_map` fills `host_ip` with the empty string, so
/// a service's `ports: ["127.0.0.1:8080:80/tcp"]` publishes on every address
/// here where upstream publishes on loopback only. The port keys and the host
/// ports are exact, and `nat::parse_port_specs` is the same function as
/// upstream's `nat.ParsePortSpecs`; the loss is in the already-ported input
/// type, not in the parse.
fn port_binding_strings(bindings: &nat::PortMap) -> BTreeMap<String, Vec<String>> {
    bindings
        .iter()
        .map(|(port, port_bindings)| {
            (
                port.as_str().to_string(),
                port_bindings
                    .iter()
                    .map(|binding| binding.host_port.clone())
                    .collect(),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// execJobContainer
// ---------------------------------------------------------------------------

/// `execJobContainer`: run a command in the job container.
///
/// Upstream calls the method on `rc.JobContainer` without a nil check, so a nil
/// container is a nil-pointer dereference there.
///
/// Deliberate deviation: this returns `no job container` instead of panicking.
/// A nil container is upstream's programming error, and a port that aborts the
/// process over it is worse than one that says so.
pub fn exec_job_container(
    job: &JobContainer,
    command: &[String],
    env: &BTreeMap<String, String>,
    user: &str,
    workdir: &str,
) -> Executor {
    let Some(environment) = job.get() else {
        return Arc::new(|_| Err(anyhow!("no job container")));
    };
    // Cloned, because a step outlives the borrow: the executor is a
    // `'static` closure and cannot hold a `&dyn ExecutionsEnvironment`.
    let environment = Arc::clone(environment);
    let command = command.to_vec();
    let env = env.clone();
    let user = user.to_string();
    let workdir = workdir.to_string();
    Arc::new(move |ctx: &crate::common::context::RunContext| {
        environment.exec(&command, &env, user.as_str(), workdir.as_str())(ctx)
    })
}

// ---------------------------------------------------------------------------
// stopJobContainer
// ---------------------------------------------------------------------------

/// `stopJobContainer`: remove the job container and the two volumes it made.
///
/// Four steps, each skipped when `--reuse` is set, and the fourth unconditional
/// because it is the one that also tidies up the services:
///
/// 1. `Remove()` the container;
/// 2. remove the volume named after the job;
/// 3. remove the volume named after the job with `-env`;
/// 4. stop the service containers, and remove the network act created.
///
/// Steps 2 and 3 are the two volumes
/// [`get_binds_and_mounts`](super::run_context::RunContext::get_binds_and_mounts)
/// created: the one named after the job, which holds the workdir, and the one
/// named after the job with `-env`, which holds the act directory. Removing them
/// is what makes a re-run of the same job start with an empty act directory.
///
/// A nil container is a no-op, which is what upstream's
/// `if rc.JobContainer != nil` buys: measured, all three of `stopJobContainer`,
/// `stopContainer` and `closeContainer` return `nil` with no container at all.
pub fn stop_job_container(rc: &RunContext, job: &JobContainer) -> Executor {
    let Some(environment) = job.as_environment() else {
        return noop();
    };
    let reuse = rc.config.reuse_containers;
    // One closure per `IfNot`, because a `Conditional` is consumed by the
    // combinator it is given to.
    let reused = || -> Conditional { Box::new(move |_: &crate::common::context::RunContext| reuse) };
    let name = rc.job_container_name();
    let env_volume = format!("{name}-env");

    let remove = environment.remove();
    let remove_volume = new_docker_volume_remove_executor(name, false);
    let remove_env_volume = new_docker_volume_remove_executor(env_volume, false);
    let services = rc.service_containers.clone();
    let network = rc.job_container_network.clone();
    let create_and_delete_network = rc.create_and_delete_network;
    let job_label = rc.name.clone();

    pipeline(vec![
        if_not(remove, reused()),
        if_not(remove_volume, reused()),
        if_not(remove_env_volume, reused()),
        // The last step, upstream's inline closure. It never fails: a service
        // that will not stop and a network that will not go are both logged and
        // stepped over, because the job is already over and losing the error
        // would hide the reason.
        Arc::new(move |ctx: &crate::common::context::RunContext| {
            if !services.is_empty() {
                ctx.log_info(&format!("Cleaning up services for job {job_label}"));
                if let Err(error) = stop_service_containers_of(services.as_slice())(ctx) {
                    ctx.log_error(&format!("Error while cleaning services: {error}"));
                }
                if create_and_delete_network {
                    // The network act created for this job, because the job had
                    // service containers, so it is act's to remove — and only
                    // after the services are off it.
                    ctx.log_info(&format!(
                        "Cleaning up network for job {job_label}, and network name is: {network}"
                    ));
                    if let Err(error) = new_docker_network_remove_executor(network.clone())(ctx) {
                        ctx.log_error(&format!("Error while cleaning network: {error}"));
                    }
                }
            }
            Ok(())
        }),
    ])
}

// ---------------------------------------------------------------------------
// pullServicesImages
// ---------------------------------------------------------------------------

/// `pullServicesImages`: pull every service image, in parallel.
///
/// The parallelism is the **number of services**, not a fixed width: upstream
/// passes `len(execs)` to `NewParallelExecutor`, so two services pull
/// concurrently and a third job's are not held back by them.
pub fn pull_services_images(rc: &RunContext, force_pull: bool) -> Executor {
    let steps: Vec<Executor> = rc
        .service_containers
        .iter()
        .map(|service| service.inner().pull(force_pull))
        .collect();
    parallel_executor(steps.len(), steps)
}

// ---------------------------------------------------------------------------
// startServiceContainers
// ---------------------------------------------------------------------------

/// `startServiceContainers`: pull (without forcing), create and start each
/// service, in parallel.
///
/// The parameter is upstream's own and unused there too —
/// `func (rc *RunContext) startServiceContainers(_ string)` — and it is kept so
/// the call site reads the same. The network each service ends up on is not
/// decided here: it was baked into the container at creation, which is why
/// ignoring the argument is not a bug.
pub fn start_service_containers(rc: &RunContext, _network_name: &str) -> Executor {
    let cap_add = rc.config.container_cap_add.clone();
    let cap_drop = rc.config.container_cap_drop.clone();
    let steps: Vec<Executor> = rc
        .service_containers
        .iter()
        .map(|service| {
            let environment = service.inner();
            // `Pull(false)`, not the `--force-pull` flag: a service image is
            // fetched if absent and reused if present, and the job image is the
            // one `--force-pull` is about.
            pipeline(vec![
                environment.pull(false),
                environment.create(cap_add.as_slice(), cap_drop.as_slice()),
                environment.start(false),
            ])
        })
        .collect();
    parallel_executor(steps.len(), steps)
}

// ---------------------------------------------------------------------------
// waitForServiceContainer
// ---------------------------------------------------------------------------

/// `waitForServiceContainer`: poll one service's health until it is not
/// starting.
///
/// The loop, and every number in it, is measured:
///
/// | health | polls | result |
/// |---|---|---|
/// | healthy on the first poll | 1 | `nil` |
/// | unhealthy on the first poll | 1 | `service container failed to start` |
/// | never leaves starting | **32**, over **4m45s** | `service container failed to start` |
///
/// Thirty-two because `i > 30` breaks on `i = 31`; 4m45s because the backoff
/// is 1s, 2s, 4s, 8s and then ten seconds for the remaining 27 sleeps
/// (15 + 270). The 5-minute `WithTimeout` never fires first, so the port's
/// wall-clock deadline is not observable.
///
/// The backoff is `delay *= 2` with a ten-second cap, and it is applied *after*
/// the sleep and *before* the next poll. A container that reports healthy on
/// poll 2 therefore waited one second, and one that reports healthy on poll 3
/// waited three.
pub fn wait_for_service_container(container: &Arc<dyn ExecutionsEnvironment>) -> Executor {
    let container = Arc::clone(container);
    Arc::new(move |_ctx| {
        let deadline = Instant::now() + HEALTH_TIMEOUT;
        let mut delay = HEALTH_INITIAL_DELAY;
        let mut poll: i32 = 0;
        // Labelled rather than a `let mut health`: Go initialises it to
        // `HealthStarting` before the loop, and this keeps that value in play
        // without an assignment the compiler can see is dead.
        let health = 'polls: loop {
            let health = container.health();
            if health != Health::Starting || poll > HEALTH_POLL_LIMIT {
                break 'polls health;
            }
            // Upstream's `time.Sleep` is not cancellable either; the deadline
            // is checked after it, where the context would have been.
            std::thread::sleep(delay);
            delay *= 2;
            if delay > HEALTH_MAX_DELAY {
                delay = HEALTH_MAX_DELAY;
            }
            poll += 1;
            if Instant::now() >= deadline {
                break 'polls health;
            }
        };
        if health == Health::Healthy {
            return Ok(());
        }
        Err(anyhow!("service container failed to start"))
    })
}

/// `waitForServiceContainers`: wait for every service, in parallel.
pub fn wait_for_service_containers(rc: &RunContext) -> Executor {
    let steps: Vec<Executor> = rc
        .service_containers
        .iter()
        .map(|service| wait_for_service_container(service.inner()))
        .collect();
    parallel_executor(steps.len(), steps)
}

// ---------------------------------------------------------------------------
// stopServiceContainers
// ---------------------------------------------------------------------------

/// `stopServiceContainers`: remove every service, then close it.
pub fn stop_service_containers(rc: &RunContext) -> Executor {
    stop_service_containers_of(rc.service_containers.as_slice())
}

/// `stopServiceContainers` over an explicit list, so the cleanup step inside
/// [`stop_job_container`] can use the list it captured.
fn stop_service_containers_of(services: &[ServiceContainer]) -> Executor {
    let steps: Vec<Executor> = services
        .iter()
        .map(|service| {
            let environment = service.inner();
            // `Remove().Finally(Close())`: the close runs whatever the remove
            // did, because a container that would not be removed is exactly the
            // one whose handle must still be released.
            finally(environment.remove(), environment.close())
        })
        .collect();
    parallel_executor(steps.len(), steps)
}

// ---------------------------------------------------------------------------
// startContainer / stopContainer / closeContainer
// ---------------------------------------------------------------------------

/// `startContainer`: the host environment or the job container.
///
/// The dispatch is `IsHostEnv`, which is *not* "the job has no image": measured,
/// it is true only for a `runs-on:` label whose configured image is the literal
/// `-self-hosted`. An **empty** image (`-P ubuntu-latest=`) leaves
/// `containerImage` and `runsOnImage` both empty and `IsHostEnv` false, so it
/// reaches [`start_job_container`] and fails on the empty image reference. The
/// ported [`is_host_env`](super::run_context::RunContext::is_host_env) is the
/// same predicate; the label is what the caller configures.
///
/// Upstream wraps the call in `common.EarlyCancelContext(ctx)`, and that is a
/// measured no-op: `EarlyCancelContext` returns the context it was given and a
/// no-op cancel unless the context carries a job-cancel key, and nothing in
/// `pkg/runner` or `pkg/cmd` ever sets one — `step.go` only reads it. So
/// nothing is wrapped here.
pub fn start_container(
    rc: &mut RunContext,
    environment: &EvaluationEnvironment,
    status: &dyn StatusProvider,
    daemon_architecture: &str,
    sink: Arc<dyn LogSink>,
) -> Result<(JobContainer, Executor)> {
    if rc.is_host_env(environment, status) {
        return start_host_environment(rc, sink);
    }
    start_job_container(rc, environment, status, daemon_architecture, sink)
}

/// `stopContainer`: upstream's is literally `return rc.stopJobContainer()`.
///
/// Measured: both call the one `cleanUpJobContainer` field, twice, when it is
/// installed. The alias is kept so a reader looking for the upstream pair finds
/// them, and the body is a call rather than a copy.
pub fn stop_container(rc: &RunContext, job: &JobContainer) -> Executor {
    stop_job_container(rc, job)
}

/// `closeContainer`: release the container's handle without removing it.
///
/// A no-op for a nil container, and a no-op for a host environment: upstream's
/// `HostEnvironment.Close` returns `nil` without touching anything, so the port
/// calls the same method and gets the same nothing.
pub fn close_container(job: &JobContainer) -> Executor {
    match job.as_environment() {
        Some(environment) => environment.close(),
        None => noop(),
    }
}

// ---------------------------------------------------------------------------
// The two messages act writes while it assembles
// ---------------------------------------------------------------------------

/// The line act writes as the job container is built.
fn start_image_message(image: &str) -> String {
    // Two spaces after the rocket, which is act's own: `logger.Infof("\U0001f680
    //  Start image=%s", image)`. Pinned by a test, because a log line a user
    // greps for is a log line that must not change.
    format!("\u{1f680}  Start image={image}")
}

/// The line act writes for a service it will not start.
fn empty_service_image_message(service_id: &str) -> String {
    format!(
        "The service '{service_id}' will not be started because the container definition has an empty image."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::context::{CollectingSink, RunContext as StepContext};
    use crate::expr::DefaultStatus;
    use crate::model::{Run, Workflow};
    use crate::runner::run_context::RunConfig;
    use crate::yaml_node::Document;
    use std::rc::Rc;

    // ------------------------------------------------------------- the fake --

    /// A container that records what it was asked to do instead of doing it.
    ///
    /// What is under test in this file is the *order* the runner asks for things
    /// in and the values it asks with, never the daemon's answer — so every
    /// method records `name.method(args)` and succeeds. `remove` is the one
    /// that can be told to fail, because the cleanup chain's behaviour when a
    /// removal fails is a thing this file owns.
    ///
    /// Each method records when the returned step is **run**, not when it is
    /// built, and that is the whole point: `IfNot(reuse)` constructs the removal
    /// unconditionally and only declines to run it, so a fake that recorded on
    /// construction would report a removal that never happened.
    #[derive(Clone)]
    struct FakeContainer {
        name: String,
        calls: Arc<std::sync::Mutex<Vec<String>>>,
        /// The health each successive `health()` answers; the last one repeats.
        healths: Arc<std::sync::Mutex<Vec<Health>>>,
        polls: Arc<std::sync::atomic::AtomicUsize>,
        remove_fails: bool,
    }

    impl FakeContainer {
        fn new(name: &str) -> Arc<Self> {
            Self::build(name, vec![Health::Healthy], false)
        }

        fn scripted_health(name: &str, healths: Vec<Health>) -> Arc<Self> {
            Self::build(name, healths, false)
        }

        fn failing_remove(name: &str) -> Arc<Self> {
            Self::build(name, vec![Health::Healthy], true)
        }

        fn build(name: &str, healths: Vec<Health>, remove_fails: bool) -> Arc<Self> {
            Arc::new(FakeContainer {
                name: name.to_string(),
                calls: Arc::new(std::sync::Mutex::new(Vec::new())),
                healths: Arc::new(std::sync::Mutex::new(healths)),
                polls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                remove_fails,
            })
        }

        /// Records `call` straight away, for the four methods that answer a
        /// question instead of returning a step.
        fn record(&self, call: impl Into<String>) {
            self.calls.lock().expect("the call log").push(call.into());
        }

        /// The step that records `call` when it is **run**, and then succeeds.
        fn recording(&self, call: impl Into<String>) -> Executor {
            let calls = Arc::clone(&self.calls);
            let call = call.into();
            Arc::new(move |_| {
                calls.lock().expect("the call log").push(call.clone());
                Ok(())
            })
        }

        /// This container's own calls, in order. Filtering by name is what lets
        /// a two-service test ignore the other service's interleaving, since
        /// [`parallel_executor`] runs them on separate threads.
        fn own_calls(&self) -> Vec<String> {
            self.calls
                .lock()
                .expect("the call log")
                .iter()
                .filter(|call| call.starts_with(&format!("{}.", self.name)))
                .cloned()
                .collect()
        }

        fn polls(&self) -> usize {
            self.polls.load(std::sync::atomic::Ordering::SeqCst)
        }

        /// The fake behind the trait, the way the runner holds a container.
        fn environment(self: &Arc<Self>) -> Arc<dyn ExecutionsEnvironment> {
            Arc::clone(self) as Arc<dyn ExecutionsEnvironment>
        }

        fn as_job(self: &Arc<Self>) -> JobContainer {
            JobContainer::new(self.environment())
        }

        fn as_service(self: &Arc<Self>) -> ServiceContainer {
            ServiceContainer::new(self.environment())
        }
    }

    impl ExecutionsEnvironment for FakeContainer {
        fn create(&self, cap_add: &[String], cap_drop: &[String]) -> Executor {
            self.recording(format!(
                "{}.create(+{:?} -{:?})",
                self.name, cap_add, cap_drop
            ))
        }

        fn close(&self) -> Executor {
            self.recording(format!("{}.close", self.name))
        }

        fn copy(&self, dest_path: &str, files: Vec<FileEntry>) -> Executor {
            self.recording(format!(
                "{}.copy({dest_path},{})",
                self.name,
                files
                    .iter()
                    .map(|file| format!("{}:{:o}", file.name, file.mode))
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        }

        fn copy_tar_stream(&self, dest_path: &str, tar_stream: &[u8]) -> Result<()> {
            self.record(format!(
                "{}.copy_tar_stream({dest_path},{} bytes)",
                self.name,
                tar_stream.len()
            ));
            Ok(())
        }

        fn copy_dir(&self, dest_path: &str, src_path: &str, use_gitignore: bool) -> Executor {
            self.recording(format!(
                "{}.copy_dir({dest_path},{src_path},{use_gitignore})",
                self.name
            ))
        }

        fn container_archive(&self, src_path: &str) -> Result<Vec<u8>> {
            self.record(format!("{}.container_archive({src_path})", self.name));
            Ok(Vec::new())
        }

        fn pull(&self, force_pull: bool) -> Executor {
            self.recording(format!("{}.pull({force_pull})", self.name))
        }

        fn start(&self, attach: bool) -> Executor {
            self.recording(format!("{}.start({attach})", self.name))
        }

        fn exec(
            &self,
            command: &[String],
            env: &BTreeMap<String, String>,
            user: &str,
            workdir: &str,
        ) -> Executor {
            self.recording(format!(
                "{}.exec({command:?},{env:?},{user:?},{workdir:?})",
                self.name
            ))
        }

        fn update_from_env(&self, src_path: &str) -> Result<BTreeMap<String, String>> {
            self.record(format!("{}.update_from_env({src_path})", self.name));
            Ok(BTreeMap::new())
        }

        fn update_from_image_env(&self) -> Result<BTreeMap<String, String>> {
            self.record(format!("{}.update_from_image_env()", self.name));
            Ok(BTreeMap::new())
        }

        fn remove(&self) -> Executor {
            let calls = Arc::clone(&self.calls);
            let name = self.name.clone();
            let fails = self.remove_fails;
            Arc::new(move |_| {
                calls
                    .lock()
                    .expect("the call log")
                    .push(format!("{name}.remove"));
                if fails {
                    Err(anyhow!("remove refused"))
                } else {
                    Ok(())
                }
            })
        }

        fn health(&self) -> Health {
            let poll = self.polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let script = self.healths.lock().expect("the health script");
            script
                .get(poll)
                .or_else(|| script.last())
                .copied()
                .unwrap_or(Health::Starting)
        }

        fn replace_log_writer(&self, _stdout: Arc<dyn LogSink>) -> Option<Arc<dyn LogSink>> {
            None
        }

        fn to_container_path(&self, path: &str) -> String {
            path.to_string()
        }

        fn act_path(&self) -> String {
            "/var/run/act".to_string()
        }

        fn path_variable_name(&self) -> &'static str {
            "PATH"
        }

        fn default_path_variable(&self) -> String {
            "/usr/local/bin".to_string()
        }

        fn join_path_variable(&self, paths: &[&str]) -> String {
            paths.join(":")
        }

        fn runner_context(&self, _ctx: &StepContext) -> BTreeMap<String, String> {
            BTreeMap::new()
        }

        fn is_environment_case_insensitive(&self) -> bool {
            false
        }
    }

    // ------------------------------------------------------------- fixtures --

    /// A run of one job out of a one-job workflow, built from YAML.
    fn run(job_yaml: &str) -> Run {
        let source = format!(
            "name: test-workflow\njobs:\n  test:\n    name: test\n{}",
            job_yaml
                .lines()
                .map(|line| format!("    {line}\n"))
                .collect::<String>()
        );
        let document = Rc::new(Document::parse(&source).expect("the fixture parses"));
        let workflow = Workflow::from_document("test.yml", &document).expect("the fixture decodes");
        Run::new(workflow, document, "test")
    }

    fn environment() -> EvaluationEnvironment {
        EvaluationEnvironment::default()
    }

    fn status() -> DefaultStatus {
        DefaultStatus
    }

    /// The default platforms: `ubuntu-latest` mapped, as `-P ubuntu-latest=…`
    /// maps it.
    fn platforms() -> BTreeMap<String, String> {
        BTreeMap::from([(
            "ubuntu-latest".to_string(),
            "cimg/base:latest".to_string(),
        )])
    }

    /// A context with one job and no container, which is the Docker branch.
    ///
    /// The workdir and the `ubuntu-latest` platform are filled in only when the
    /// caller left them empty, so a test that configures its own `platforms` —
    /// which is how the dispatch is decided — gets the ones it asked for.
    fn rc_with(mut config: RunConfig, job: Option<Run>) -> RunContext {
        if config.workdir.is_empty() {
            config.workdir = "/tmp/probe-workdir".to_string();
        }
        if config.platforms.is_empty() {
            config.platforms = platforms();
        }
        RunContext {
            name: "test".to_string(),
            event_json: r#"{"action":"push"}"#.to_string(),
            config,
            run: job,
            ..RunContext::default()
        }
    }

    /// A one-job workflow whose `container:` block carries these
    /// `credentials:` keys, one per line.
    fn run_with_credentials(keys: &[&str]) -> Run {
        run(&format!(
            "runs-on: ubuntu-latest\ncontainer:\n  image: img:1\n  credentials:\n{}",
            keys
                .iter()
                .map(|key| format!("    {key}\n"))
                .collect::<String>()
        ))
    }

    /// A sink that discards, for the tests that do not read the log.
    fn discarding_sink() -> Arc<dyn LogSink> {
        Arc::new(crate::common::context::NullSink)
    }

    // -------------------------------------------------------------- the env --

    /// The five variables, in act's order.
    ///
    /// `RUNNER_OS=Linux` is a literal in act, not the host's OS: a Windows host
    /// still tells the job `Linux`, because that is the container's point of
    /// view. And `LANG=C.UTF-8` is act's own choice, with the comment "Use same
    /// locale as GitHub Actions".
    #[test]
    fn the_job_container_environment_is_five_variables_in_order() {
        assert_eq!(
            job_container_env("x86_64"),
            vec![
                "RUNNER_TOOL_CACHE=/opt/hostedtoolcache",
                "RUNNER_OS=Linux",
                "RUNNER_ARCH=X64",
                "RUNNER_TEMP=/tmp",
                "LANG=C.UTF-8",
            ]
        );
    }

    /// The daemon's `arm64` and Go's `aarch64` both mean ARM64, an unmapped
    /// architecture passes through, and a daemon that cannot be reached leaves
    /// `RUNNER_ARCH` **empty** — measured upstream, and the reason the value is
    /// a parameter here rather than a lookup.
    #[test]
    fn the_runner_architecture_comes_from_the_daemon() {
        for (daemon, want) in [
            ("arm64", "ARM64"),
            ("aarch64", "ARM64"),
            ("amd64", "X64"),
            ("386", "X86"),
            ("riscv64", "riscv64"),
            ("", ""),
        ] {
            let env = job_container_env(daemon);
            assert!(
                env.contains(&format!("RUNNER_ARCH={want}")),
                "daemon {daemon:?} should give RUNNER_ARCH={want:?}, got {env:?}"
            );
        }
    }

    // --------------------------------------------------------- credentials --

    /// With no `container:` block the answer is the `DOCKER_*` secrets.
    /// Measured on v0.2.89 for both the empty and the configured case.
    #[test]
    fn the_secrets_are_the_credentials_when_the_job_has_no_container() {
        let configured = rc_with(
            RunConfig {
                secrets: BTreeMap::from([
                    ("DOCKER_USERNAME".to_string(), "du".to_string()),
                    ("DOCKER_PASSWORD".to_string(), "dp".to_string()),
                ]),
                ..RunConfig::default()
            },
            Some(run("runs-on: ubuntu-latest")),
        );
        assert_eq!(
            handle_credentials(&configured, &environment(), &status()).expect("the secrets"),
            ("du".to_string(), "dp".to_string())
        );

        let bare = rc_with(RunConfig::default(), Some(run("runs-on: ubuntu-latest")));
        assert_eq!(
            handle_credentials(&bare, &environment(), &status()).expect("the secrets"),
            (String::new(), String::new())
        );
    }

    /// A `credentials:` map must have exactly two keys, and the count is
    /// checked before anything is interpolated. Measured: one key and three
    /// keys both give this text, and two keys give the pair back.
    #[test]
    fn the_credentials_map_must_have_exactly_two_keys() {
        for (keys, count) in [
            (vec!["username: u"], 1),
            (vec!["username: u", "password: p"], 2),
            (vec!["username: u", "password: p", "extra: x"], 3),
        ] {
            let rc = rc_with(RunConfig::default(), Some(run_with_credentials(&keys)));
            let outcome = handle_credentials(&rc, &environment(), &status());
            if count == 2 {
                assert_eq!(
                    outcome.expect("two keys are accepted"),
                    ("u".to_string(), "p".to_string()),
                    "{count} keys"
                );
            } else {
                assert_eq!(
                    outcome.unwrap_err().to_string(),
                    "invalid property count for key 'credentials:'",
                    "{count} keys"
                );
            }
        }
    }

    /// An empty value fails on the **interpolation**, not on the check after
    /// it — which is what makes `container.credentials cannot be empty`
    /// unreachable upstream. Measured: `username: ''` reports the interpolation.
    #[test]
    fn an_empty_credential_fails_on_the_interpolation_check() {
        for (keys, want) in [
            (
                vec!["username: ''", "password: p"],
                "failed to interpolate container.credentials.username",
            ),
            (
                vec!["username: u", "password: ''"],
                "failed to interpolate container.credentials.password",
            ),
        ] {
            let rc = rc_with(RunConfig::default(), Some(run_with_credentials(&keys)));
            assert_eq!(
                handle_credentials(&rc, &environment(), &status())
                    .unwrap_err()
                    .to_string(),
                want
            );
        }
        // The fourth check, which the two above make unreachable, is asserted
        // on its own terms so the sequence is four steps and not three.
        assert!(credentials_cannot_be_empty("", "p").is_err());
        assert!(credentials_cannot_be_empty("u", "").is_err());
        assert!(credentials_cannot_be_empty("u", "p").is_ok());
    }

    /// A service's rules, including the `None` that a nil Go map is. Measured
    /// upstream: nil gives `("", "", nil)`, one key gives the property-count
    /// error, and an empty username gives the *service* message —
    /// `credentials.username`, not `container.credentials.username`.
    #[test]
    fn the_service_credentials_have_their_own_messages() {
        let env = environment();
        let status = status();
        assert_eq!(
            handle_service_credentials(&env, &status, None).expect("nil"),
            (String::new(), String::new())
        );
        let one = BTreeMap::from([("username".to_string(), "u".to_string())]);
        assert_eq!(
            handle_service_credentials(&env, &status, Some(&one))
                .unwrap_err()
                .to_string(),
            "invalid property count for key 'credentials:'"
        );
        let empty = BTreeMap::from([
            ("username".to_string(), String::new()),
            ("password".to_string(), "p".to_string()),
        ]);
        assert_eq!(
            handle_service_credentials(&env, &status, Some(&empty))
                .unwrap_err()
                .to_string(),
            "failed to interpolate credentials.username"
        );
        let both = BTreeMap::from([
            ("username".to_string(), "u".to_string()),
            ("password".to_string(), "p".to_string()),
        ]);
        assert_eq!(
            handle_service_credentials(&env, &status, Some(&both)).expect("two keys"),
            ("u".to_string(), "p".to_string())
        );
    }

    /// An empty `credentials:` map is read as a nil map, which is the one thing
    /// a `BTreeMap` cannot say. A service that wrote no `credentials:` therefore
    /// skips the count check instead of failing it.
    #[test]
    fn an_empty_credentials_map_reads_as_absent() {
        assert!(service_credentials(&ContainerSpec::default()).is_none());
        let with_one = ContainerSpec {
            credentials: BTreeMap::from([("username".to_string(), "u".to_string())]),
            ..ContainerSpec::default()
        };
        assert!(service_credentials(&with_one).is_some());
    }

    /// The three ways `credentials:` can be absent, and the two different
    /// answers they get.
    ///
    /// Go keeps a nil-ness that a `BTreeMap` cannot: `yaml.v3` decodes an
    /// absent key and an explicit null to a **nil** map, and `{}` to a
    /// **non-nil empty** one. `handleCredentials` returns the config secrets
    /// for nil and fails a non-nil map whose length is not 2 — so `{}` is an
    /// error and "no `credentials:`" is not.
    ///
    /// Measured on v0.2.89 and reproduced through
    /// [`ContainerSpec::credentials_map`], which reads the raw node for exactly
    /// this. The three rows are the whole distinction; the first two are the
    /// same answer for different reasons, and the third is the one that used to
    /// differ.
    #[test]
    fn credentials_absent_and_credentials_empty_are_different_answers() {
        let evaluate = |rc: &RunContext| handle_credentials(rc, &environment(), &status());

        // No `credentials:` key at all: the secrets path, no error.
        let none = rc_with(
            RunConfig {
                secrets: BTreeMap::from([
                    ("DOCKER_USERNAME".to_string(), "du".to_string()),
                    ("DOCKER_PASSWORD".to_string(), "dp".to_string()),
                ]),
                ..RunConfig::default()
            },
            Some(run("runs-on: ubuntu-latest\ncontainer:\n  image: img:1")),
        );
        assert_eq!(
            evaluate(&none).expect("nil credentials take the secrets path"),
            ("du".to_string(), "dp".to_string()),
            "an absent credentials: is nil, so the secrets are used",
        );

        // `credentials:` with an explicit null: also nil, also the secrets path.
        let null = rc_with(
            RunConfig {
                secrets: BTreeMap::from([
                    ("DOCKER_USERNAME".to_string(), "du".to_string()),
                    ("DOCKER_PASSWORD".to_string(), "dp".to_string()),
                ]),
                ..RunConfig::default()
            },
            Some(run(
                "runs-on: ubuntu-latest\ncontainer:\n  image: img:1\n  credentials:",
            )),
        );
        assert_eq!(
            evaluate(&null).expect("a null credentials: is nil too"),
            ("du".to_string(), "dp".to_string()),
            "a null credentials: takes the same path as an absent one",
        );

        // `credentials: {}`: a non-nil empty map, so the property count fails.
        let empty = rc_with(
            RunConfig {
                secrets: BTreeMap::from([
                    ("DOCKER_USERNAME".to_string(), "du".to_string()),
                    ("DOCKER_PASSWORD".to_string(), "dp".to_string()),
                ]),
                ..RunConfig::default()
            },
            Some(run(
                "runs-on: ubuntu-latest\ncontainer:\n  image: img:1\n  credentials: {}",
            )),
        );
        assert_eq!(
            evaluate(&empty).unwrap_err().to_string(),
            "invalid property count for key 'credentials:'",
            "an empty mapping is not nil, and act rejects it",
        );
    }

    // ------------------------------------------------------ the nil branches --

    /// Every nil branch upstream has. Measured: `stopJobContainer`,
    /// `stopContainer` and `closeContainer` all return `nil` with no container
    /// at all.
    ///
    /// The executors succeed *and* the fake is never touched, which is the
    /// difference between "a nil container is a no-op" and "a nil container
    /// removes something".
    #[test]
    fn a_nil_container_is_a_no_op_rather_than_a_removal() {
        let job = FakeContainer::new("job");
        let rc = rc_with(RunConfig::default(), Some(run("runs-on: ubuntu-latest")));
        let none = JobContainer::none();
        let ctx = StepContext::new();

        assert!(none.is_none());
        assert!(stop_job_container(&rc, &none)(&ctx).is_ok());
        assert!(stop_container(&rc, &none)(&ctx).is_ok());
        assert!(close_container(&none)(&ctx).is_ok());
        assert!(
            job.own_calls().is_empty(),
            "the fake was never asked to do anything"
        );
    }

    /// The nil check upstream performs after building the job container, with
    /// upstream's text. It is unreachable from `start_job_container` — measured,
    /// `container.NewContainer` never returns nil — and reachable from here,
    /// which is what makes keeping it honest rather than dead.
    #[test]
    fn a_nil_job_container_fails_with_upstreams_text() {
        // `.err()` rather than `.unwrap_err()`: the success value is a trait
        // object, and `unwrap_err` would want it to be `Debug`.
        let error = require_container(&JobContainer::none())
            .err()
            .expect("a nil container has no environment");
        assert_eq!(error.to_string(), "Failed to create job container");
    }

    /// `execJobContainer` has no nil check upstream: it calls the method on the
    /// interface, and a nil container is a nil dereference. The port says so
    /// instead of aborting, and hands the command over untouched when there is
    /// a container to run it in.
    #[test]
    fn exec_without_a_container_reports_it_instead_of_panicking() {
        let ctx = StepContext::new();
        assert_eq!(
            exec_job_container(
                &JobContainer::none(),
                &["echo".to_string()],
                &BTreeMap::new(),
                "",
                ""
            )(&ctx)
            .unwrap_err()
            .to_string(),
            "no job container"
        );

        let job = FakeContainer::new("job");
        let command = vec!["echo".to_string(), "hello".to_string()];
        let env = BTreeMap::from([("K".to_string(), "V".to_string())]);
        exec_job_container(&job.as_job(), &command, &env, "root", "/work")(&ctx)
            .expect("a container runs the command");
        assert_eq!(
            job.own_calls(),
            [r#"job.exec(["echo", "hello"],{"K": "V"},"root","/work")"#]
        );
    }

    // ------------------------------------------------------ the network mode --

    /// The three-way decision, with a `container:` image beating `--network`.
    #[test]
    fn the_job_container_network_is_chosen_in_three_steps() {
        for (configured, image, network, want) in [
            // No `container:`, no `--network`: host.
            ("", "", "host", "host"),
            // `--network` alone beats the default.
            ("bridge", "", "host", "bridge"),
            // A `container:` image takes the per-job network, whatever
            // `--network` said.
            ("bridge", "node:18", "act-net-1", "act-net-1"),
            ("", "node:18", "act-net-1", "act-net-1"),
        ] {
            assert_eq!(
                job_container_network_mode(configured, image, network),
                want,
                "configured={configured:?} image={image:?}"
            );
        }
    }

    // --------------------------------------------------------- the file pair --

    /// The two files act writes into every job container, with the modes.
    ///
    /// `envs.txt` is 0666 and not 0644: a step appends to it, and act's own
    /// number is the writable one.
    #[test]
    fn every_container_gets_the_event_file_and_an_empty_envs_file() {
        let files = act_files(r#"{"action":"push"}"#);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "workflow/event.json");
        assert_eq!(files[0].mode, 0o644);
        assert_eq!(files[0].body, r#"{"action":"push"}"#);
        assert_eq!(files[1].name, "workflow/envs.txt");
        assert_eq!(files[1].mode, 0o666);
        assert_eq!(files[1].body, "");
    }

    // ---------------------------------------------------------- the messages --

    /// The two lines act writes while it assembles, character for character.
    ///
    /// The rocket is followed by **two** spaces in act's format string, and the
    /// service line names the service between single quotes. Both are pinned
    /// because a log line a user greps for is part of the interface.
    #[test]
    fn the_assembly_lines_are_pinned() {
        assert_eq!(
            start_image_message("cimg/base:latest"),
            "\u{1f680}  Start image=cimg/base:latest"
        );
        assert_eq!(
            empty_service_image_message("db"),
            "The service 'db' will not be started because the container definition has an empty image."
        );
    }

    // ------------------------------------------------------------- the ports --

    /// `nat.ParsePortSpecs` through the container input's string shape: the
    /// ports expand, and the host ports come out exact.
    ///
    /// The host **IP** does not come out, because the ported input has no field
    /// for it — so `127.0.0.1:8080:80/tcp` binds `8080` and the address is lost.
    /// That is the difference the port's `BTreeMap<String, Vec<String>>` forces,
    /// and it is why the loopback row below reads as a bare host port.
    #[test]
    fn the_service_ports_become_exposed_ports_and_bindings() {
        let ports = vec![
            "5432:5432".to_string(),
            "127.0.0.1:8080:80/tcp".to_string(),
            "9000-9001/udp".to_string(),
        ];
        let (exposed, bindings) = nat::parse_port_specs(&ports).expect("the ports parse");
        assert_eq!(
            exposed_port_strings(&exposed),
            ["5432/tcp", "80/tcp", "9000/udp", "9001/udp"]
        );
        // An unspecified host port is the **empty** string, which is Docker's
        // "pick one" — so the last two rows carry no host port at all.
        assert_eq!(
            port_binding_strings(&bindings),
            BTreeMap::from([
                ("5432/tcp".to_string(), vec!["5432".to_string()]),
                ("80/tcp".to_string(), vec!["8080".to_string()]),
                ("9000/udp".to_string(), vec![String::new()]),
                ("9001/udp".to_string(), vec![String::new()]),
            ])
        );
    }

    // ----------------------------------------------------------- the options --

    /// `options:` is one string upstream and an argv here, so the round trip
    /// through the back-end's own `join(" ")` and re-split has to be lossless.
    #[test]
    fn the_container_options_become_argv() {
        assert_eq!(container_options(""), Vec::<String>::new());
        assert_eq!(
            container_options("--cpus 2 --memory 512m"),
            ["--cpus", "2", "--memory", "512m"]
        );
        // A string that does not split is passed as one element, which
        // re-joins to exactly itself — so the back-end still reports the failure
        // itself, with its own text, at the `create` step.
        let unterminated = container_options("\"never closed");
        assert_eq!(unterminated, ["\"never closed"]);
        assert_eq!(unterminated.join(" "), "\"never closed");
    }

    // ---------------------------------------------------------- the host path --

    /// The host environment end to end, with no daemon anywhere: the scratch
    /// directories, the four `RUNNER_*` variables, the two copied files, and the
    /// cleanup that takes the scratch directory away again.
    ///
    /// `tool_cache` lives *outside* the random directory, so the cleanup does
    /// not take it with it — and nothing creates it, which is upstream's own
    /// arrangement. Measured on v0.2.89: act path `<cache>/<16 hex>/act`, tmp
    /// `<cache>/<16 hex>/tmp`, tool cache `<cache>/tool_cache`.
    #[test]
    fn the_host_environment_is_built_copied_into_and_cleaned_up() {
        let cache = tempfile::TempDir::new().expect("a temp cache dir");
        let cache_path = cache.path().to_string_lossy().into_owned();
        let mut rc = rc_with(
            RunConfig {
                action_cache_dir: cache_path.clone(),
                ..RunConfig::default()
            },
            None,
        );
        rc.job_container = None;

        let (job, executor) =
            start_host_environment(&mut rc, discarding_sink()).expect("the scratch directories");

        // The reduced view the rest of the runner branches on, which is what
        // makes `get_binds_and_mounts` take its host branch.
        let paths = rc.job_container.clone().expect("the host branch is recorded");
        assert!(paths.act_path.starts_with(&cache_path), "{}", paths.act_path);
        assert!(paths.act_path.ends_with("/act"), "{}", paths.act_path);
        // The workdir as the scratch environment sees it: a host environment
        // maps the host workdir onto its own path, so this is *not* the host's.
        assert_eq!(
            paths.workdir,
            job.as_environment()
                .expect("a host environment")
                .to_container_path("/tmp/probe-workdir")
        );
        assert_ne!(paths.workdir, "/tmp/probe-workdir");
        assert_eq!(paths.environment_case_insensitive, cfg!(windows));

        // `GetRunnerContext`, uppercased and prefixed with `RUNNER_`.
        for key in [
            "RUNNER_OS",
            "RUNNER_ARCH",
            "RUNNER_TEMP",
            "RUNNER_TOOL_CACHE",
        ] {
            assert!(
                rc.env.contains_key(key),
                "{key} missing from {:?}",
                rc.env.keys().collect::<Vec<_>>()
            );
        }
        assert_eq!(
            rc.env.get("RUNNER_TOOL_CACHE").expect("the tool cache").as_str(),
            format!("{cache_path}/tool_cache")
        );
        assert_eq!(
            rc.env.get("RUNNER_TEMP").expect("the temp dir").as_str(),
            paths.act_path.replace("/act", "/tmp")
        );
        // The process environment is merged, and the `RUNNER_*` values above
        // were written first, so a host variable of the same name cannot win.
        assert!(rc.env.contains_key("PATH"), "the process environment is merged");
        assert_eq!(
            rc.env.get("RUNNER_OS").expect("the OS").as_str(),
            crate::container::go_os_to_action_os(std::env::consts::OS)
        );

        // The pipeline's one step writes the two files, for real.
        executor(&StepContext::new()).expect("the copy");
        let act_path = paths.act_path.clone();
        assert_eq!(
            std::fs::read_to_string(format!("{act_path}/workflow/event.json")).expect("event.json"),
            r#"{"action":"push"}"#
        );
        assert_eq!(
            std::fs::read_to_string(format!("{act_path}/workflow/envs.txt")).expect("envs.txt"),
            ""
        );

        // `stopJobContainer` with `--reuse` **off**, which is what a host
        // environment's cleanup needs: the first step is `JobContainer.Remove()`,
        // and for a host environment that is the closure removing the whole
        // random directory. The two volume steps after it want a daemon, so the
        // executor's own outcome is not asserted — the directory is.
        rc.config.reuse_containers = false;
        let _ = stop_job_container(&rc, &job)(&StepContext::new());
        assert!(
            !std::path::Path::new(&act_path).exists(),
            "the scratch directory should be gone: {act_path}"
        );
        assert!(
            !cache.path().join("tool_cache").exists(),
            "the tool cache is not created by the host environment"
        );
    }

    // ------------------------------------------------------ the service steps --

    /// The three service helpers, each asserting its own recorded order.
    ///
    /// `startServiceContainers` pulls with `forcePull=false` even when the job
    /// was told to force-pull: the flag is about the **job** image, and the
    /// service pipeline upstream says `c.Pull(false)` in a loop that never sees
    /// the flag. Asserted because it is the sort of thing a "consistency" fix
    /// would change.
    #[test]
    fn the_service_steps_run_in_upstreams_order() {
        let db = FakeContainer::new("db");
        let rc = RunContext {
            service_containers: vec![db.as_service()],
            ..rc_with(
                RunConfig {
                    force_pull: true,
                    container_cap_add: vec!["SYS_ADMIN".to_string()],
                    container_cap_drop: vec!["MKNOD".to_string()],
                    ..RunConfig::default()
                },
                Some(run("runs-on: ubuntu-latest")),
            )
        };
        let ctx = StepContext::new();

        pull_services_images(&rc, rc.config.force_pull)(&ctx).expect("the pulls");
        assert_eq!(db.own_calls(), ["db.pull(true)"]);

        start_service_containers(&rc, "act-net")(&ctx).expect("the service start");
        assert_eq!(
            db.own_calls()[1..],
            [
                "db.pull(false)",
                r#"db.create(+["SYS_ADMIN"] -["MKNOD"])"#,
                "db.start(false)",
            ]
        );

        // `Remove().Finally(Close())`: the close happens whatever the remove did.
        stop_service_containers(&rc)(&ctx).expect("the service stop");
        assert_eq!(db.own_calls()[4..], ["db.remove", "db.close"]);
    }

    /// Two services run concurrently, and each one's own order still holds —
    /// the parallelism is `len(execs)`, not a fixed width.
    #[test]
    fn every_service_is_pulled_and_started() {
        let db = FakeContainer::new("db");
        let redis = FakeContainer::new("redis");
        let rc = RunContext {
            service_containers: vec![db.as_service(), redis.as_service()],
            ..rc_with(RunConfig::default(), Some(run("runs-on: ubuntu-latest")))
        };
        let ctx = StepContext::new();

        pull_services_images(&rc, false)(&ctx).expect("both pulls");
        assert_eq!(db.own_calls(), ["db.pull(false)"]);
        assert_eq!(redis.own_calls(), ["redis.pull(false)"]);

        start_service_containers(&rc, "act-net")(&ctx).expect("both start");
        for (fake, name) in [(&db, "db"), (&redis, "redis")] {
            assert_eq!(
                fake.own_calls()[1..],
                [
                    format!("{name}.pull(false)"),
                    format!("{name}.create(+[] -[])"),
                    format!("{name}.start(false)")
                ],
                "{name}'s own order"
            );
        }
    }

    /// The health wait, over all the containers a job has, in parallel.
    #[test]
    fn every_service_is_waited_for() {
        let db = FakeContainer::scripted_health("db", vec![Health::Starting, Health::Healthy]);
        let redis = FakeContainer::new("redis");
        let rc = RunContext {
            service_containers: vec![db.as_service(), redis.as_service()],
            ..rc_with(RunConfig::default(), Some(run("runs-on: ubuntu-latest")))
        };

        wait_for_service_containers(&rc)(&StepContext::new()).expect("both become healthy");
        assert_eq!(db.polls(), 2);
        assert_eq!(redis.polls(), 1, "a healthy service is one poll");
    }

    // ------------------------------------------------------------- the health --

    /// A healthy service is one poll and no wait — and the common case, because
    /// a container with no health check is reported healthy.
    #[test]
    fn a_healthy_service_is_one_poll() {
        let db = FakeContainer::new("db");
        wait_for_service_container(&db.environment())(&StepContext::new())
            .expect("a healthy service");
        assert_eq!(db.polls(), 1, "measured upstream: one poll");
        assert!(
            db.own_calls().is_empty(),
            "health is a question, not a recorded call"
        );
    }

    /// An unhealthy service fails on the first poll, with act's text.
    #[test]
    fn an_unhealthy_service_fails_on_the_first_poll() {
        let db = FakeContainer::scripted_health("db", vec![Health::Unhealthy]);
        assert_eq!(
            wait_for_service_container(&db.environment())(&StepContext::new())
                .unwrap_err()
                .to_string(),
            "service container failed to start"
        );
        assert_eq!(db.polls(), 1);
    }

    /// A service that is still starting is polled again, after the first
    /// backoff. Measured upstream: the first delay is one second, so this test
    /// takes one second and that is the assertion.
    #[test]
    fn a_starting_service_is_polled_again_after_the_first_backoff() {
        let db = FakeContainer::scripted_health("db", vec![Health::Starting, Health::Healthy]);
        let started = Instant::now();
        wait_for_service_container(&db.environment())(&StepContext::new())
            .expect("it becomes healthy");
        let waited = started.elapsed();
        assert_eq!(db.polls(), 2, "one poll, one second, one more poll");
        assert!(
            waited >= HEALTH_INITIAL_DELAY,
            "the first backoff is one second, waited {waited:?}"
        );
        assert!(
            waited < HEALTH_INITIAL_DELAY * 2,
            "and it is not two, waited {waited:?}"
        );
    }

    /// The poll limit and the total wait, without spending either.
    ///
    /// 32 polls is what upstream makes — `for i := 0; ; i++` with a break at
    /// `i > 30` calls `health()` for `i = 0…31` — and the measured cost of those
    /// 32 polls is 4m45s, which is *under* the five minutes act allows. That is
    /// why the deadline is never the thing that gives up, and why no test
    /// anywhere should sit through it.
    #[test]
    fn the_poll_limit_is_thirty_two_and_costs_four_minutes_forty_five() {
        assert_eq!(HEALTH_POLL_LIMIT, 30, "`i > 30` breaks on i = 31");
        // The break is tested *after* the poll, so i = 31 is polled and then
        // breaks: 32 polls, and 31 sleeps.
        assert_eq!((0..=HEALTH_POLL_LIMIT + 1).count(), 32, "i = 0…31 inclusive");

        let mut total = Duration::ZERO;
        let mut delay = HEALTH_INITIAL_DELAY;
        for _ in 0..=HEALTH_POLL_LIMIT {
            total += delay;
            delay = (delay * 2).min(HEALTH_MAX_DELAY);
        }
        assert_eq!(total, Duration::from_secs(285), "measured: 4m45s");
        assert!(
            total < HEALTH_TIMEOUT,
            "so the five-minute deadline never fires first"
        );
    }

    // ----------------------------------------------------- the cleanup chain --

    /// The cleanup chain with `--reuse` on, so the three Docker steps are
    /// skipped and the fourth runs without a daemon.
    ///
    /// This is where the network quirk is visible: a network act created is
    /// removed only **if there were service containers**, because upstream nests
    /// the removal inside that check. The network removal itself wants a daemon
    /// and its failure is logged rather than returned, so the executor succeeds
    /// either way — which is itself the upstream behaviour worth pinning.
    #[test]
    fn the_cleanup_removes_the_services_and_maybe_the_network() {
        for (services, created, want) in [
            (
                1,
                true,
                vec![
                    "Cleaning up services for job test".to_string(),
                    "Cleaning up network for job test, and network name is: act-net-1"
                        .to_string(),
                ],
            ),
            (1, false, vec!["Cleaning up services for job test".to_string()]),
            // The quirk: a created network with no services is never removed.
            (0, true, Vec::<String>::new()),
            (0, false, Vec::<String>::new()),
        ] {
            let job = FakeContainer::new("job");
            let db = FakeContainer::new("db");
            let sink = Arc::new(CollectingSink::new());
            let shared: Arc<dyn LogSink> = sink.clone();
            let ctx = StepContext::new().with_sink(shared);
            let rc = RunContext {
                service_containers: if services == 1 {
                    vec![db.as_service()]
                } else {
                    Vec::new()
                },
                job_container_network: "act-net-1".to_string(),
                create_and_delete_network: created,
                ..rc_with(
                    RunConfig {
                        reuse_containers: true,
                        ..RunConfig::default()
                    },
                    Some(run("runs-on: ubuntu-latest")),
                )
            };

            stop_job_container(&rc, &job.as_job())(&ctx)
                .expect("the cleanup never fails the job");
            assert_eq!(
                sink.messages_at(Level::Info),
                want,
                "{services} service(s), network created: {created}"
            );
            assert!(
                job.own_calls().is_empty(),
                "--reuse skips the container's own removal"
            );
            assert_eq!(
                db.own_calls(),
                if services == 1 {
                    vec!["db.remove".to_string(), "db.close".to_string()]
                } else {
                    Vec::new()
                }
            );
        }
    }

    /// A failing removal stops the chain, and the failure is what comes back —
    /// which is what `Remove().IfNot(reuse).Then(volume)` does upstream: the
    /// volume steps are never reached, so no daemon is needed to prove it.
    #[test]
    fn a_failing_removal_stops_the_cleanup_chain() {
        let job = FakeContainer::failing_remove("job");
        let rc = rc_with(
            RunConfig {
                reuse_containers: false,
                ..RunConfig::default()
            },
            Some(run("runs-on: ubuntu-latest")),
        );
        assert_eq!(
            stop_job_container(&rc, &job.as_job())(&StepContext::new())
                .unwrap_err()
                .to_string(),
            "remove refused"
        );
        assert_eq!(job.own_calls(), ["job.remove"]);
    }

    /// `stopContainer` is upstream's `return rc.stopJobContainer()`, and the
    /// measured proof is that both reach the one stored cleanup. Here they are
    /// the same function, so this is the check that neither of them has drifted
    /// into doing something of its own: both reach the job container's removal
    /// exactly once, and with `--reuse` neither does.
    #[test]
    fn stop_container_is_the_job_container_cleanup() {
        let without_reuse = |reuse: bool| {
            let job = FakeContainer::new("job");
            let rc = RunContext {
                service_containers: Vec::new(),
                ..rc_with(
                    RunConfig {
                        reuse_containers: reuse,
                        ..RunConfig::default()
                    },
                    Some(run("runs-on: ubuntu-latest")),
                )
            };
            let ctx = StepContext::new();
            // With `--reuse` the removal is skipped and the chain reaches the
            // two volume steps, which want a daemon; only what was recorded
            // before that is asserted, and it is the same either way.
            let _ = stop_container(&rc, &job.as_job())(&ctx);
            let _ = stop_job_container(&rc, &job.as_job())(&ctx);
            (job, rc.config.reuse_containers)
        };

        let (job, reuse) = without_reuse(false);
        assert!(!reuse);
        assert_eq!(
            job.own_calls(),
            ["job.remove", "job.remove"],
            "each call removed the container once, and neither of them did more"
        );

        let (job, reuse) = without_reuse(true);
        assert!(reuse);
        assert!(
            job.own_calls().is_empty(),
            "--reuse skips the removal in both"
        );
    }

    // ----------------------------------------------------------- the dispatch --

    /// `IsHostEnv` is not "the job has no image". Measured: it is true only for a
    /// label configured as the literal `-self-hosted`, and an **empty** image
    /// reaches the job-container branch — where act then fails on the empty
    /// image reference.
    #[test]
    fn the_dispatch_is_the_self_hosted_label_not_a_missing_image() {
        let cache = tempfile::TempDir::new().expect("a temp cache dir");
        let mut host = rc_with(
            RunConfig {
                platforms: BTreeMap::from([(
                    "self-hosted".to_string(),
                    "-self-hosted".to_string(),
                )]),
                action_cache_dir: cache.path().to_string_lossy().into_owned(),
                ..RunConfig::default()
            },
            Some(run("runs-on: self-hosted")),
        );
        host.job_container = None;
        let (job, _executor) =
            start_container(&mut host, &environment(), &status(), "x86_64", discarding_sink())
                .expect("the host branch needs no daemon");
        assert!(
            host.job_container.is_some(),
            "the host branch records ContainerPaths, which is what get_binds_and_mounts branches on"
        );
        assert_eq!(
            job.as_environment().expect("a host").act_path(),
            host.job_container.as_ref().expect("paths").act_path,
            "the host's act path is the one the context recorded"
        );

        // An empty image is **not** the host branch.
        let mut docker = rc_with(
            RunConfig {
                platforms: BTreeMap::from([("ubuntu-latest".to_string(), String::new())]),
                ..RunConfig::default()
            },
            Some(run("runs-on: ubuntu-latest")),
        );
        docker.job_container = None;
        let (job, _executor) =
            start_container(&mut docker, &environment(), &status(), "x86_64", discarding_sink())
                .expect("building the job container needs no daemon");
        assert!(
            docker.job_container.is_none(),
            "the Docker branch records no ContainerPaths"
        );
        assert_eq!(
            job.as_environment().expect("a container").act_path(),
            crate::container::linux::ACT_PATH
        );
    }

    // ----------------------------------------------------------- the assembly --

    /// The service assembly, and the two data fields that replace upstream's
    /// stored cleanup closure.
    ///
    /// A service whose image interpolates to empty is skipped and never
    /// appended — measured upstream, the service list comes out one shorter than
    /// the workflow declared, with the log line. And because act only creates a
    /// network when the job *has* services, a job with one surviving service has
    /// one, and the cleanup is the one that will remove it.
    #[test]
    fn the_services_are_assembled_and_the_network_recorded_as_data() {
        let mut rc = rc_with(
            RunConfig::default(),
            Some(run(
                r#"runs-on: ubuntu-latest
container:
  image: job:1
services:
  db:
    image: postgres:16
    env:
      POSTGRES_PASSWORD: pw
    ports:
      - 5432:5432
    volumes:
      - db-data:/var/lib/postgresql/data
  empty:
    image: ""
"#,
            )),
        );
        rc.job_container = None;
        let (_job, _executor) = start_job_container(
            &mut rc,
            &environment(),
            &status(),
            "x86_64",
            discarding_sink(),
        )
        .expect("the assembly needs no daemon");

        // One service: the empty-image one was skipped.
        assert_eq!(
            rc.service_containers.len(),
            1,
            "{:?}",
            rc.service_containers
        );
        // The two fields the cleanup executor is built from, in place of
        // upstream's `cleanUpJobContainer` closure.
        assert!(rc.create_and_delete_network, "a job with services has a network");
        assert_eq!(
            rc.job_container_network,
            format!("{}-test-network", rc.job_container_name()),
            "the network is the job's name, the job id, and `-network`"
        );
        // And the job container's own network is that same network, because the
        // job has a `container:` image — the rule the decision table pins.
        assert_eq!(
            job_container_network_mode(
                rc.config.container_network_mode.as_str(),
                "job:1",
                rc.job_container_network.as_str()
            ),
            rc.job_container_network
        );
    }

    /// A job with no services gets no network, which is what makes the two
    /// network fields' relationship to `network_name()` checkable.
    #[test]
    fn a_job_without_services_gets_no_network() {
        let mut rc = rc_with(RunConfig::default(), Some(run("runs-on: ubuntu-latest")));
        rc.job_container = None;
        start_job_container(&mut rc, &environment(), &status(), "x86_64", discarding_sink())
            .expect("the assembly needs no daemon");
        assert!(!rc.create_and_delete_network);
        assert_eq!(rc.job_container_network, "host");
        assert!(rc.service_containers.is_empty());
    }

    // ------------------------------------------------------------ the suffix --

    /// The scratch directory's name: sixteen lowercase hex characters, and two
    /// calls do not collide.
    #[test]
    fn the_scratch_directory_suffix_is_sixteen_hex_characters() {
        let first = random_suffix();
        assert_eq!(first.len(), 16, "{first}");
        assert!(
            first.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "{first}"
        );
        assert_ne!(first, random_suffix(), "two calls must not collide");
    }
}
