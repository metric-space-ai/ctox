use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::inference::engine;
use crate::inference::model_adapters;
use crate::inference::runtime_contract;
use crate::inference::runtime_env;
use crate::inference::runtime_kernel;
use crate::inference::runtime_plan;
use crate::inference::runtime_state;
use crate::inference::supervisor;

const RUNTIME_SWITCH_RELATIVE_PATH: &str = "runtime/runtime_switch.json";
const RUNTIME_SWITCH_LOCK_RELATIVE_PATH: &str = "runtime/runtime_switch.lock";
const LOCAL_RUNTIME_READY_STABILITY_PASSES: usize = 3;
const LOCAL_RUNTIME_READY_STABILITY_POLL_MILLIS: u64 = 200;
const RUNTIME_SWITCH_LEASE_POLL_MILLIS: u64 = 250;
const RUNTIME_SWITCH_LEASE_WAIT_SECS: u64 = 30;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSwitchPhase {
    Requested,
    Preparing,
    Warming,
    CutoverReady,
    Committed,
    Draining,
    Released,
    Failed,
}

impl RuntimeSwitchPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::Released | Self::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeSwitchTransaction {
    pub version: u32,
    pub phase: RuntimeSwitchPhase,
    pub requested_model: String,
    pub requested_source: runtime_state::InferenceSource,
    #[serde(default = "runtime_state::default_local_runtime_kind")]
    pub requested_local_runtime: runtime_state::LocalRuntimeKind,
    pub requested_preset: Option<String>,
    #[serde(default)]
    pub previous_source: Option<runtime_state::InferenceSource>,
    #[serde(default)]
    pub previous_local_runtime: Option<runtime_state::LocalRuntimeKind>,
    #[serde(default)]
    pub previous_requested_model: Option<String>,
    pub previous_active_model: Option<String>,
    #[serde(default)]
    pub previous_preset: Option<String>,
    #[serde(default)]
    pub previous_plan: Option<runtime_plan::ChatRuntimePlan>,
    pub next_active_model: Option<String>,
    pub started_at_epoch_secs: u64,
    pub updated_at_epoch_secs: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSelectionChange {
    pub previous_state: runtime_state::InferenceRuntimeState,
    pub next_state: runtime_state::InferenceRuntimeState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSwitchExecution {
    pub change: RuntimeSelectionChange,
    pub active_model: String,
    pub upstream_base_url: String,
    pub released_previous_backend: bool,
    pub already_active: bool,
    pub phase: RuntimeSwitchPhase,
}

struct RuntimeSwitchLease {
    path: PathBuf,
}

impl Drop for RuntimeSwitchLease {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn runtime_switch_path(root: &Path) -> PathBuf {
    root.join(RUNTIME_SWITCH_RELATIVE_PATH)
}

pub fn load_runtime_switch_transaction(root: &Path) -> Result<Option<RuntimeSwitchTransaction>> {
    let path = runtime_switch_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| {
        format!(
            "failed to read runtime switch transaction {}",
            path.display()
        )
    })?;
    let txn = serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "failed to parse runtime switch transaction {}",
            path.display()
        )
    })?;
    Ok(Some(txn))
}

pub fn persist_runtime_switch_transaction(
    root: &Path,
    transaction: &RuntimeSwitchTransaction,
) -> Result<()> {
    let path = runtime_switch_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime switch dir {}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(transaction)
        .context("failed to encode runtime switch transaction")?;
    std::fs::write(&path, bytes).with_context(|| {
        format!(
            "failed to write runtime switch transaction {}",
            path.display()
        )
    })
}

pub fn apply_runtime_selection(
    root: &Path,
    model: &str,
    preset: Option<&str>,
) -> Result<RuntimeSelectionChange> {
    let previous_plan = runtime_plan::load_persisted_chat_runtime_plan(root)?;
    let change = persist_runtime_selection(root, model, preset)?;
    let now = runtime_contract::current_epoch_secs();
    persist_runtime_switch_transaction(
        root,
        &RuntimeSwitchTransaction {
            version: 3,
            phase: RuntimeSwitchPhase::Requested,
            requested_model: model.trim().to_string(),
            requested_source: change.next_state.source,
            requested_local_runtime: change.next_state.local_runtime,
            requested_preset: change.next_state.local_preset.clone(),
            previous_source: Some(change.previous_state.source),
            previous_local_runtime: Some(change.previous_state.local_runtime),
            previous_requested_model: change
                .previous_state
                .requested_model
                .clone()
                .or(change.previous_state.active_model.clone()),
            previous_active_model: change.previous_state.active_model.clone(),
            previous_preset: change.previous_state.local_preset.clone(),
            previous_plan,
            next_active_model: change.next_state.active_model.clone(),
            started_at_epoch_secs: now,
            updated_at_epoch_secs: now,
            error: None,
        },
    )?;

    Ok(change)
}

pub fn execute_runtime_switch(
    root: &Path,
    model: &str,
    preset: Option<&str>,
) -> Result<RuntimeSwitchExecution> {
    let _lease = acquire_runtime_switch_lease(root, model)?;
    let requested_model = model.trim();
    if requested_model.is_empty() {
        anyhow::bail!("runtime selection model must not be empty");
    }

    let previous_plan_digest = runtime_plan::load_persisted_chat_runtime_plan_digest(root)?;
    let change = apply_runtime_selection(root, requested_model, preset)?;
    let _ = update_runtime_switch_phase(root, RuntimeSwitchPhase::Preparing, None);

    let next_state = runtime_state::load_or_resolve_runtime_state(root)?;
    let next_plan_digest = runtime_plan::load_persisted_chat_runtime_plan_digest(root)?;
    let previous_backend_healthy = runtime_state_is_healthy(root, &change.previous_state);
    let runtime_plan_unchanged = previous_plan_digest == next_plan_digest;

    if same_runtime_target(&change.previous_state, &next_state)
        && previous_backend_healthy
        && runtime_plan_unchanged
    {
        let _ = update_runtime_switch_phase(root, RuntimeSwitchPhase::Committed, None);
        return Ok(RuntimeSwitchExecution {
            active_model: next_state
                .active_or_selected_model()
                .unwrap_or(requested_model)
                .to_string(),
            upstream_base_url: next_state.upstream_base_url.clone(),
            change,
            released_previous_backend: false,
            already_active: true,
            phase: RuntimeSwitchPhase::Committed,
        });
    }

    let force_restart = next_state.source.is_local()
        && (!previous_backend_healthy
            || change.previous_state.active_model != next_state.active_model
            || !runtime_plan_unchanged);
    let full_runtime_redeploy = should_redeploy_local_runtime_fleet(&change, &next_state);
    let released_previous_backend = should_release_previous_local_backend(&change, &next_state);

    if full_runtime_redeploy {
        let _ = update_runtime_switch_phase(root, RuntimeSwitchPhase::Draining, None);
        supervisor::release_managed_runtime_fleet(root)?;
    } else if released_previous_backend {
        let _ = update_runtime_switch_phase(root, RuntimeSwitchPhase::Draining, None);
        supervisor::release_chat_backend(root, change.previous_state.engine_port)?;
    }

    let _ = update_runtime_switch_phase(root, RuntimeSwitchPhase::Warming, None);
    if let Err(err) = ensure_selected_runtime_ready(root, &next_state, force_restart) {
        let _ = rollback_runtime_switch(root);
        let _ = supervisor::ensure_persistent_backends(root);
        let _ =
            update_runtime_switch_phase(root, RuntimeSwitchPhase::Failed, Some(&err.to_string()));
        return Err(err);
    }

    let _ = update_runtime_switch_phase(root, RuntimeSwitchPhase::CutoverReady, None);
    if let Some(committed_phase) = commit_runtime_switch_if_ready(root, &next_state)? {
        supervisor::ensure_auxiliary_backends_best_effort(root.to_path_buf());
        return Ok(RuntimeSwitchExecution {
            active_model: next_state
                .active_or_selected_model()
                .unwrap_or(requested_model)
                .to_string(),
            upstream_base_url: next_state.upstream_base_url.clone(),
            change,
            released_previous_backend,
            already_active: false,
            phase: committed_phase,
        });
    }
    let terminal_phase = wait_for_runtime_switch_commit(root, &next_state)?;
    if terminal_phase == RuntimeSwitchPhase::Committed {
        supervisor::ensure_auxiliary_backends_best_effort(root.to_path_buf());
    }

    Ok(RuntimeSwitchExecution {
        active_model: next_state
            .active_or_selected_model()
            .unwrap_or(requested_model)
            .to_string(),
        upstream_base_url: next_state.upstream_base_url.clone(),
        change,
        released_previous_backend,
        already_active: false,
        phase: terminal_phase,
    })
}

fn commit_runtime_switch_if_ready(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
) -> Result<Option<RuntimeSwitchPhase>> {
    match state.source {
        runtime_state::InferenceSource::Api => {
            let transaction =
                update_runtime_switch_phase(root, RuntimeSwitchPhase::Committed, None)?;
            Ok(transaction.map(|transaction| transaction.phase))
        }
        runtime_state::InferenceSource::Local => {
            if local_runtime_is_ready(root, state) && local_runtime_is_owned_and_active(root, state)
            {
                let transaction =
                    update_runtime_switch_phase(root, RuntimeSwitchPhase::Committed, None)?;
                Ok(transaction.map(|transaction| transaction.phase))
            } else {
                Ok(None)
            }
        }
    }
}

fn runtime_switch_lock_path(root: &Path) -> PathBuf {
    root.join(RUNTIME_SWITCH_LOCK_RELATIVE_PATH)
}

fn acquire_runtime_switch_lease(root: &Path, model: &str) -> Result<RuntimeSwitchLease> {
    let path = runtime_switch_lock_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime dir {}", parent.display()))?;
    }
    let deadline = Instant::now() + Duration::from_secs(RUNTIME_SWITCH_LEASE_WAIT_SECS);
    loop {
        match try_create_runtime_switch_lease(&path, model) {
            Ok(lease) => return Ok(lease),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                if runtime_switch_lock_is_stale(&path) {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                if Instant::now() >= deadline {
                    anyhow::bail!(
                        "another runtime switch is already in progress (lock: {})",
                        path.display()
                    );
                }
                thread::sleep(Duration::from_millis(RUNTIME_SWITCH_LEASE_POLL_MILLIS));
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("failed to acquire runtime switch lock {}", path.display())
                });
            }
        }
    }
}

fn try_create_runtime_switch_lease(
    path: &Path,
    model: &str,
) -> std::io::Result<RuntimeSwitchLease> {
    let mut handle = OpenOptions::new().write(true).create_new(true).open(path)?;
    writeln!(handle, "pid={}", std::process::id())?;
    writeln!(handle, "model={}", model.trim())?;
    Ok(RuntimeSwitchLease {
        path: path.to_path_buf(),
    })
}

fn runtime_switch_lock_is_stale(path: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return true;
    };
    let pid = raw
        .lines()
        .find_map(|line| line.strip_prefix("pid="))
        .and_then(|value| value.trim().parse::<u32>().ok());
    match pid {
        Some(pid) => !runtime_switch_owner_is_alive(pid),
        None => true,
    }
}

fn stale_runtime_switch_lock_detected(root: &Path) -> bool {
    let path = runtime_switch_lock_path(root);
    path.exists() && runtime_switch_lock_is_stale(&path)
}

fn clear_stale_runtime_switch_lock(root: &Path) {
    let path = runtime_switch_lock_path(root);
    if stale_runtime_switch_lock_detected(root) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(unix)]
fn runtime_switch_owner_is_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn runtime_switch_owner_is_alive(_pid: u32) -> bool {
    false
}

fn wait_for_runtime_switch_commit(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
) -> Result<RuntimeSwitchPhase> {
    let active_model = state
        .active_or_selected_model()
        .filter(|value| !value.trim().is_empty());
    let wait_secs = match state.source {
        runtime_state::InferenceSource::Api => 30,
        runtime_state::InferenceSource::Local => {
            supervisor::backend_startup_wait_secs_for_model(active_model)
        }
    };
    let deadline = Instant::now() + Duration::from_secs(wait_secs);
    let mut last_phase = RuntimeSwitchPhase::CutoverReady;
    while Instant::now() < deadline {
        let transaction = reconcile_runtime_switch_transaction(root)?
            .context("runtime switch transaction disappeared before commit")?;
        last_phase = transaction.phase;
        if transaction.phase == RuntimeSwitchPhase::Committed {
            return Ok(RuntimeSwitchPhase::Committed);
        }
        if transaction.phase == RuntimeSwitchPhase::Failed {
            anyhow::bail!(
                "runtime switch failed before commit{}",
                transaction
                    .error
                    .as_deref()
                    .map(|value| format!(": {value}"))
                    .unwrap_or_default()
            );
        }
        thread::sleep(Duration::from_millis(250));
    }
    anyhow::bail!(
        "runtime switch did not reach committed phase for {} within {}s (last phase: {:?})",
        active_model.unwrap_or("selected runtime"),
        wait_secs,
        last_phase
    )
}

pub fn rollback_runtime_switch(root: &Path) -> Result<Option<RuntimeSelectionChange>> {
    let Some(transaction) = load_runtime_switch_transaction(root)? else {
        return Ok(None);
    };
    let previous_source = transaction.previous_source.unwrap_or_else(|| {
        transaction
            .previous_requested_model
            .as_deref()
            .or(transaction.previous_active_model.as_deref())
            .filter(|value| engine::is_api_chat_model(value))
            .map(|_| runtime_state::InferenceSource::Api)
            .unwrap_or(runtime_state::InferenceSource::Local)
    });
    let Some(previous_model) = transaction
        .previous_requested_model
        .as_deref()
        .or(transaction.previous_active_model.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let change = restore_runtime_selection(
        root,
        previous_source,
        transaction.previous_local_runtime,
        previous_model,
        transaction.previous_preset.as_deref(),
        transaction.previous_plan.as_ref(),
    )?;
    Ok(Some(change))
}

fn same_runtime_target(
    previous: &runtime_state::InferenceRuntimeState,
    next: &runtime_state::InferenceRuntimeState,
) -> bool {
    previous.source == next.source
        && previous.local_runtime == next.local_runtime
        && previous.active_or_selected_model() == next.active_or_selected_model()
        && previous.upstream_base_url == next.upstream_base_url
}

fn should_redeploy_local_runtime_fleet(
    change: &RuntimeSelectionChange,
    next_state: &runtime_state::InferenceRuntimeState,
) -> bool {
    change.previous_state.source.is_local()
        && (next_state.source != runtime_state::InferenceSource::Local
            || change.previous_state.active_model != next_state.active_model
            || change.previous_state.local_runtime != next_state.local_runtime)
}

fn should_release_previous_local_backend(
    change: &RuntimeSelectionChange,
    next_state: &runtime_state::InferenceRuntimeState,
) -> bool {
    !should_redeploy_local_runtime_fleet(change, next_state)
        && change.previous_state.source.is_local()
        && (change.previous_state.upstream_base_url != next_state.upstream_base_url
            || change.previous_state.active_model != next_state.active_model)
}

fn runtime_state_is_healthy(root: &Path, state: &runtime_state::InferenceRuntimeState) -> bool {
    match state.source {
        runtime_state::InferenceSource::Api => runtime_env::env_or_config(
            root,
            runtime_state::api_key_env_var_for_upstream_base_url(&state.upstream_base_url),
        )
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false),
        runtime_state::InferenceSource::Local => {
            let readiness = local_runtime_readiness(root, state);
            readiness
                .socket_path
                .as_deref()
                .map(socket_listener_accepts)
                .unwrap_or(false)
                || readiness
                    .health_url
                    .as_deref()
                    .map(probe_backend_health_url)
                    .unwrap_or(false)
                || (local_runtime_is_owned_and_active(root, state)
                    && readiness.socket_path.is_none()
                    && readiness.health_url.is_none())
        }
    }
}

fn ensure_selected_runtime_ready(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
    force_restart: bool,
) -> Result<()> {
    match state.source {
        runtime_state::InferenceSource::Api => return Ok(()),
        runtime_state::InferenceSource::Local => {}
    }

    let active_model = state
        .active_or_selected_model()
        .filter(|value| !value.trim().is_empty())
        .context("local runtime switch is missing an active model")?;
    if !force_restart && local_runtime_is_ready(root, state) {
        return Ok(());
    }

    let startup_wait_secs = supervisor::backend_startup_wait_secs_for_model(Some(active_model));
    supervisor::ensure_chat_backend_ready(root, force_restart)?;
    let mut deadline = Instant::now() + Duration::from_secs(startup_wait_secs);
    let mut nccl_retry_attempted = false;
    while Instant::now() < deadline {
        if local_runtime_is_ready(root, state) {
            return Ok(());
        }
        if !nccl_retry_attempted && maybe_retry_chat_backend_without_nccl(root, active_model)? {
            nccl_retry_attempted = true;
            deadline = Instant::now() + Duration::from_secs(startup_wait_secs);
            continue;
        }
        thread::sleep(Duration::from_secs(1));
    }

    anyhow::bail!(
        "backend for model {} is not reachable via {} after startup",
        active_model,
        local_runtime_readiness_label(root, state)
    )
}

fn local_runtime_is_ready(root: &Path, state: &runtime_state::InferenceRuntimeState) -> bool {
    local_runtime_is_stably_ready(
        root,
        state,
        LOCAL_RUNTIME_READY_STABILITY_PASSES,
        Duration::from_millis(LOCAL_RUNTIME_READY_STABILITY_POLL_MILLIS),
    )
}

fn local_runtime_is_stably_ready(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
    required_passes: usize,
    poll_interval: Duration,
) -> bool {
    if promote_local_runtime_starting_workload_if_ready(root, state, required_passes, poll_interval)
    {
        return true;
    }
    let readiness = local_runtime_readiness(root, state);
    let owned_and_active = local_runtime_is_owned_and_active(root, state);
    if let Some(socket_path) = readiness.socket_path.as_deref() {
        if owned_and_active
            && socket_listener_accepts_stably(socket_path, required_passes, poll_interval)
        {
            return true;
        }
    }
    if readiness
        .health_url
        .as_deref()
        .map(probe_backend_health_url)
        .unwrap_or(false)
    {
        return true;
    }
    owned_and_active && readiness.socket_path.is_none() && readiness.health_url.is_none()
}

fn promote_local_runtime_starting_workload_if_ready(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
    required_passes: usize,
    poll_interval: Duration,
) -> bool {
    let Some(active_model) = state
        .active_or_selected_model()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    let readiness = local_runtime_readiness(root, state);
    let socket_ready = readiness.socket_path.as_deref().is_some_and(|socket_path| {
        socket_listener_accepts_stably(socket_path, required_passes, poll_interval)
    });
    let health_ready = readiness
        .health_url
        .as_deref()
        .map(probe_backend_health_url)
        .unwrap_or(false);
    if !socket_ready && !health_ready {
        return false;
    }

    let Ok(mut ownership) = runtime_contract::load_runtime_ownership_state(root) else {
        return false;
    };
    let Some(workload) = ownership.workloads.iter_mut().find(|entry| {
        entry.role == runtime_contract::BackendRole::Chat
            && entry.phase == runtime_contract::RuntimeResidencyPhase::Starting
            && entry.model.trim().eq_ignore_ascii_case(active_model)
    }) else {
        return false;
    };
    if !local_runtime_starting_workload_is_alive(root, state, workload) {
        return false;
    }

    workload.phase = runtime_contract::RuntimeResidencyPhase::Active;
    workload.updated_at_epoch_secs = runtime_contract::current_epoch_secs();
    runtime_contract::persist_runtime_ownership_state(root, &ownership).is_ok()
}

fn local_runtime_is_owned_and_active(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
) -> bool {
    let Some(active_model) = state
        .active_or_selected_model()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    let readiness = local_runtime_readiness(root, state);
    runtime_contract::load_runtime_ownership_state(root)
        .ok()
        .and_then(|ownership| {
            ownership
                .workloads
                .into_iter()
                .find(|entry| entry.role == runtime_contract::BackendRole::Chat)
        })
        .filter(|entry| entry.phase == runtime_contract::RuntimeResidencyPhase::Active)
        .filter(|entry| entry.model.trim().eq_ignore_ascii_case(active_model))
        .is_some_and(|entry| {
            entry.pid.is_some_and(|pid| {
                runtime_process_is_alive(pid)
                    && runtime_process_matches_local_backend(
                        root,
                        state,
                        pid,
                        active_model,
                        readiness.socket_path.as_deref(),
                    )
            })
        })
}

fn local_runtime_remains_committed_under_load(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
) -> bool {
    local_runtime_is_owned_and_active(root, state)
}

fn local_runtime_starting_workload_is_alive(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
    workload: &runtime_contract::BackendRuntimeResidency,
) -> bool {
    if workload.phase != runtime_contract::RuntimeResidencyPhase::Starting {
        return false;
    }
    let Some(active_model) = state
        .active_or_selected_model()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    if !workload.model.trim().eq_ignore_ascii_case(active_model) {
        return false;
    }
    let readiness = local_runtime_readiness(root, state);
    workload.pid.is_some_and(|pid| {
        runtime_process_is_alive(pid)
            && runtime_process_matches_local_backend(
                root,
                state,
                pid,
                active_model,
                readiness.socket_path.as_deref(),
            )
    })
}

fn local_runtime_requested_workload_is_alive(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
    workload: &runtime_contract::BackendRuntimeResidency,
) -> bool {
    let Some(active_model) = state
        .active_or_selected_model()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    if !workload.model.trim().eq_ignore_ascii_case(active_model) {
        return false;
    }
    let readiness = local_runtime_readiness(root, state);
    workload.pid.is_some_and(|pid| {
        runtime_process_is_alive(pid)
            && runtime_process_matches_local_backend(
                root,
                state,
                pid,
                active_model,
                readiness.socket_path.as_deref(),
            )
    })
}

fn runtime_process_is_alive(pid: u32) -> bool {
    let exists = Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !exists {
        return false;
    }
    !runtime_process_is_zombie(pid)
}

fn runtime_process_is_zombie(pid: u32) -> bool {
    let output = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    String::from_utf8_lossy(&output.stdout)
        .chars()
        .any(|value| value == 'Z')
}

fn runtime_process_matches_local_backend(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
    pid: u32,
    active_model: &str,
    socket_path: Option<&Path>,
) -> bool {
    if pid == std::process::id() {
        return true;
    }
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if command.is_empty() || !command.contains(&root.display().to_string()) {
        return false;
    }
    if state.local_runtime == runtime_state::LocalRuntimeKind::LiteRt {
        if !command.contains("serve-litert-bridge") {
            return false;
        }
        return expected_local_litert_config_path(root, state)
            .map(|expected| command.contains(expected.display().to_string().as_str()))
            .unwrap_or(false);
    }
    if !command.contains("ctox-engine") {
        return false;
    }
    if command.contains("tools/model-runtime/target/release/ctox-engine from-config") {
        return expected_local_engine_config_path(root, state)
            .map(|expected| command.contains(expected.display().to_string().as_str()))
            .unwrap_or(false);
    }
    if command.contains(active_model) {
        return true;
    }
    socket_path
        .map(|path| command.contains(&path.display().to_string()))
        .unwrap_or(false)
}

fn expected_local_engine_config_path(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
) -> Option<PathBuf> {
    let port = state.engine_port?;
    let active_model = state.active_or_selected_model()?.trim();
    if active_model.is_empty() {
        return None;
    }
    let socket_component = runtime_kernel::managed_runtime_socket_path(
        root,
        runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
    )
    .file_name()
    .and_then(|value| value.to_str())
    .map(sanitize_managed_engine_config_path_component)
    .unwrap_or_else(|| "tcp".to_string());
    let model_digest = format!("{:x}", sha2::Sha256::digest(active_model.as_bytes()));
    Some(
        root.join("runtime")
            .join("managed_engine_configs")
            .join(format!(
                "engine_{}_{}_{}.toml",
                port,
                socket_component,
                &model_digest[..12]
            )),
    )
}

fn expected_local_litert_config_path(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
) -> Option<PathBuf> {
    let port = state.engine_port?;
    let active_model = state.active_or_selected_model()?.trim();
    if active_model.is_empty() {
        return None;
    }
    let socket_component = runtime_kernel::managed_runtime_socket_path(
        root,
        runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
    )
    .file_name()
    .and_then(|value| value.to_str())
    .map(sanitize_managed_engine_config_path_component)
    .unwrap_or_else(|| "tcp".to_string());
    let model_digest = format!("{:x}", sha2::Sha256::digest(active_model.as_bytes()));
    Some(
        root.join("runtime")
            .join("managed_litert_configs")
            .join(format!(
                "litert_{}_{}_{}.json",
                port,
                socket_component,
                &model_digest[..12]
            )),
    )
}

fn sanitize_managed_engine_config_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '_',
        })
        .collect()
}

fn local_runtime_readiness_label(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
) -> String {
    let readiness = local_runtime_readiness(root, state);
    match (
        readiness.socket_path.as_deref(),
        readiness.health_url.as_deref(),
    ) {
        (Some(socket_path), Some(health_url)) => {
            format!("socket {} or {}", socket_path.display(), health_url)
        }
        (Some(socket_path), None) => format!("socket {}", socket_path.display()),
        (None, Some(health_url)) => health_url.to_string(),
        (None, None) => "local_runtime".to_string(),
    }
}

#[derive(Debug, Clone)]
struct LocalRuntimeReadiness {
    socket_path: Option<PathBuf>,
    health_url: Option<String>,
}

fn local_runtime_readiness(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
) -> LocalRuntimeReadiness {
    let fallback_socket_path = runtime_kernel::managed_runtime_socket_path(
        root,
        runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
    );
    let socket_path = Some(fallback_socket_path);
    let health_url = if state.source.is_local() {
        None
    } else {
        Some(format!("{}/health", state.upstream_base_url.trim_end_matches('/')))
    };

    LocalRuntimeReadiness {
        socket_path,
        health_url,
    }
}

#[cfg(unix)]
fn socket_listener_accepts(path: &Path) -> bool {
    use std::os::unix::net::UnixStream;

    if !path.exists() {
        return false;
    }
    UnixStream::connect(path).is_ok()
}

fn socket_listener_accepts_stably(path: &Path, attempts: usize, poll_interval: Duration) -> bool {
    let attempts = attempts.max(1);
    for idx in 0..attempts {
        if !socket_listener_accepts(path) {
            return false;
        }
        if idx + 1 < attempts {
            thread::sleep(poll_interval);
        }
    }
    true
}

#[cfg(not(unix))]
fn socket_listener_accepts(_path: &Path) -> bool {
    false
}

fn maybe_retry_chat_backend_without_nccl(root: &Path, active_model: &str) -> Result<bool> {
    let Some(plan) = runtime_plan::load_persisted_chat_runtime_plan(root)? else {
        return Ok(false);
    };
    if plan.disable_nccl || plan.tensor_parallel_backend.as_deref() != Some("nccl") {
        return Ok(false);
    }
    let Some(signature) = chat_backend_nccl_failure_signature(root)? else {
        return Ok(false);
    };
    runtime_plan::persist_nccl_capability_override(
        root,
        active_model,
        "chat backend startup failed under NCCL",
        &signature,
    )?;
    runtime_plan::reconcile_chat_runtime_plan(root)?;
    supervisor::ensure_chat_backend_ready(root, true)?;
    Ok(true)
}

fn chat_backend_nccl_failure_signature(root: &Path) -> Result<Option<String>> {
    let path = root.join("runtime").join("ctox_chat_backend.log");
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read chat backend log {}", path.display()))?;
    const NCCL_FAILURE_SIGNATURES: &[&str] = &[
        "ncclInvalidUsage",
        "ncclUnhandledCudaError",
        "ncclSystemError",
        "ncclUnhandledSystemError",
        "ncclInternalError",
        "ncclInvalidArgument",
    ];
    for line in raw.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if NCCL_FAILURE_SIGNATURES
            .iter()
            .any(|signature| trimmed.contains(signature))
        {
            return Ok(Some(trimmed.to_string()));
        }
    }
    Ok(None)
}

fn probe_backend_health_url(health_url: &str) -> bool {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(1))
        .timeout_read(Duration::from_secs(2))
        .timeout_write(Duration::from_secs(2))
        .build();

    match agent.get(health_url).call() {
        Ok(response) => response.status() < 500,
        Err(ureq::Error::Status(code, _)) => code < 500,
        Err(_) => false,
    }
}

fn persist_runtime_selection(
    root: &Path,
    model: &str,
    preset: Option<&str>,
) -> Result<RuntimeSelectionChange> {
    let requested_model = model.trim();
    if requested_model.is_empty() {
        anyhow::bail!("runtime selection model must not be empty");
    }

    let previous_state = runtime_state::load_or_resolve_runtime_state(root)?;
    let mut env_map = runtime_env::effective_operator_env_map(root).unwrap_or_default();
    overlay_process_runtime_selection_env(&mut env_map);
    let inferred_provider = runtime_state::infer_api_provider_from_env_map(&env_map);
    let requested_source = if (!inferred_provider.eq_ignore_ascii_case("local")
        && engine::api_provider_supports_model(&inferred_provider, requested_model))
        || engine::is_api_chat_model(requested_model)
    {
        runtime_state::InferenceSource::Api
    } else {
        runtime_state::InferenceSource::Local
    };
    let normalized_preset = preset
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            runtime_plan::ChatPreset::from_label(value)
                .label()
                .to_string()
        });
    let explicit_local_runtime = explicit_local_runtime_override(&env_map);

    sanitize_runtime_selection_env(&mut env_map);
    let mut next_state = build_selected_runtime_state(
        &previous_state,
        requested_source,
        explicit_local_runtime,
        requested_model,
        normalized_preset.as_deref(),
    );
    apply_selection_runtime_projection(root, &mut env_map, &mut next_state, None)?;

    Ok(RuntimeSelectionChange {
        previous_state,
        next_state,
    })
}

fn restore_runtime_selection(
    root: &Path,
    source: runtime_state::InferenceSource,
    local_runtime: Option<runtime_state::LocalRuntimeKind>,
    model: &str,
    preset: Option<&str>,
    previous_plan: Option<&runtime_plan::ChatRuntimePlan>,
) -> Result<RuntimeSelectionChange> {
    let requested_model = model.trim();
    if requested_model.is_empty() {
        anyhow::bail!("runtime selection model must not be empty");
    }

    let previous_state = runtime_state::load_or_resolve_runtime_state(root)?;
    let normalized_preset = preset
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            runtime_plan::ChatPreset::from_label(value)
                .label()
                .to_string()
        });

    let mut env_map = runtime_env::effective_operator_env_map(root).unwrap_or_default();
    overlay_process_runtime_selection_env(&mut env_map);
    sanitize_runtime_selection_env(&mut env_map);
    let mut next_state = build_selected_runtime_state(
        &previous_state,
        source,
        local_runtime,
        requested_model,
        normalized_preset.as_deref(),
    );
    apply_selection_runtime_projection(root, &mut env_map, &mut next_state, previous_plan)?;

    Ok(RuntimeSelectionChange {
        previous_state,
        next_state,
    })
}

fn sanitize_runtime_selection_env(env_map: &mut BTreeMap<String, String>) {
    env_map.remove("CTOX_BOOST_ACTIVE_UNTIL_EPOCH");
    env_map.remove("CTOX_BOOST_REASON");
    env_map.remove("CTOX_CHAT_MODEL_FAMILY");
    env_map.remove("CTOX_CHAT_MODEL");
    env_map.remove("CTOX_CHAT_MODEL_BASE");
}

fn overlay_process_runtime_selection_env(env_map: &mut BTreeMap<String, String>) {
    for key in [
        "CTOX_CHAT_SOURCE",
        "CTOX_LOCAL_RUNTIME",
        "CTOX_API_PROVIDER",
        "CTOX_UPSTREAM_BASE_URL",
        "CTOX_CHAT_MODEL",
        "CTOX_CHAT_MODEL_BASE",
    ] {
        let Ok(value) = std::env::var(key) else {
            continue;
        };
        if value.trim().is_empty() {
            continue;
        }
        env_map.insert(key.to_string(), value);
    }
}

fn explicit_local_runtime_override(
    env_map: &BTreeMap<String, String>,
) -> Option<runtime_state::LocalRuntimeKind> {
    env_map.get("CTOX_LOCAL_RUNTIME").map(
        |value| match runtime_state::normalize_local_runtime_kind(value) {
            "litert" => runtime_state::LocalRuntimeKind::LiteRt,
            _ => runtime_state::LocalRuntimeKind::Candle,
        },
    )
}

fn build_selected_runtime_state(
    previous_state: &runtime_state::InferenceRuntimeState,
    source: runtime_state::InferenceSource,
    local_runtime_override: Option<runtime_state::LocalRuntimeKind>,
    requested_model: &str,
    preset: Option<&str>,
) -> runtime_state::InferenceRuntimeState {
    let mut next_state = previous_state.clone();
    next_state.version = previous_state.version.max(7);
    next_state.source = source;
    next_state.local_runtime = local_runtime_override
        .or_else(|| runtime_state::preferred_local_runtime_kind_for_model(requested_model))
        .unwrap_or_else(|| {
            if previous_state.source.is_local() {
                previous_state.local_runtime
            } else {
                runtime_state::default_local_runtime_kind()
            }
        });
    next_state.base_model = Some(requested_model.to_string());
    next_state.requested_model = Some(requested_model.to_string());
    next_state.active_model = Some(requested_model.to_string());
    next_state.local_preset = preset.map(ToOwned::to_owned);
    next_state.boost.active_until_epoch = None;
    next_state.boost.reason = None;
    next_state.adapter_tuning = runtime_state::AdapterRuntimeTuning::default();
    next_state.proxy_host = if previous_state.proxy_host.trim().is_empty() {
        runtime_state::default_proxy_host().to_string()
    } else {
        previous_state.proxy_host.clone()
    };
    next_state.proxy_port = if previous_state.proxy_port == 0 {
        runtime_state::default_proxy_port()
    } else {
        previous_state.proxy_port
    };

    match source {
        runtime_state::InferenceSource::Api => {
            next_state.engine_model = None;
            next_state.engine_port = None;
            next_state.realized_context_tokens = None;
            next_state.upstream_base_url = if previous_state.source
                == runtime_state::InferenceSource::Api
                && !previous_state.upstream_base_url.trim().is_empty()
            {
                previous_state.upstream_base_url.clone()
            } else {
                runtime_state::default_api_upstream_base_url().to_string()
            };
        }
        runtime_state::InferenceSource::Local => {
            let engine_port = engine::runtime_config_for_model(requested_model)
                .ok()
                .map(|runtime| runtime.port)
                .unwrap_or_else(runtime_state::default_local_engine_port);
            next_state.engine_model = Some(requested_model.to_string());
            next_state.engine_port = Some(engine_port);
            next_state.realized_context_tokens =
                if next_state.local_runtime == runtime_state::LocalRuntimeKind::LiteRt {
                    runtime_state::validated_litert_context_cap_for_model(requested_model)
                } else {
                    None
                };
            next_state.upstream_base_url = runtime_state::local_upstream_base_url(engine_port);
        }
    }

    next_state
}

fn apply_selection_runtime_projection(
    root: &Path,
    env_map: &mut BTreeMap<String, String>,
    next_state: &mut runtime_state::InferenceRuntimeState,
    previous_plan: Option<&runtime_plan::ChatRuntimePlan>,
) -> Result<()> {
    match next_state.source {
        runtime_state::InferenceSource::Api => {
            runtime_plan::clear_chat_plan_env(env_map);
            // Derive the API provider from the freshly-selected model first;
            // env_map at this point may still hold a stale CTOX_CHAT_MODEL
            // from a previous selection or no model at all (clean install),
            // and infer_api_provider_from_env_map would then fall back to
            // OpenAI even if the new model is e.g. MiniMax-M2.7.
            let api_provider = next_state
                .active_model
                .as_deref()
                .filter(|model| engine::is_api_chat_model(model))
                .map(|model| engine::default_api_provider_for_model(model).to_string())
                .unwrap_or_else(|| runtime_state::infer_api_provider_from_env_map(env_map));
            next_state.upstream_base_url = env_map
                .get("CTOX_UPSTREAM_BASE_URL")
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
                .filter(|value| {
                    let trimmed = value.trim().to_ascii_lowercase();
                    !(trimmed.starts_with("http://127.0.0.1")
                        || trimmed.starts_with("http://localhost"))
                })
                .map(str::to_string)
                .unwrap_or_else(|| {
                    runtime_state::default_api_upstream_base_url_for_provider(&api_provider)
                        .to_string()
                });
            env_map.insert("CTOX_API_PROVIDER".to_string(), api_provider);
            runtime_state::apply_runtime_state_to_env_map(env_map, next_state);
            runtime_plan::store_persisted_chat_runtime_plan(root, None)?;
            runtime_plan::store_persisted_runtime_fleet_plan(root, None)?;
        }
        runtime_state::InferenceSource::Local => {
            seed_requested_local_runtime_env(env_map, next_state);
            if next_state.local_runtime == runtime_state::LocalRuntimeKind::LiteRt {
                let request_model = next_state
                    .requested_model
                    .as_deref()
                    .or(next_state.active_model.as_deref())
                    .or(next_state.base_model.as_deref())
                    .context("LiteRT runtime selection missing requested model")?;
                let validated_context =
                    runtime_state::validated_litert_context_cap_for_model(request_model)
                        .context("LiteRT runtime selection missing a qualified artifact mapping")?;
                if validated_context < 131_072 {
                    anyhow::bail!(
                        "LiteRT artifact for {} is only validated to {} tokens; CTOX local runtime requires 131072",
                        request_model,
                        validated_context
                    );
                }
                runtime_plan::clear_chat_plan_env(env_map);
                runtime_plan::store_persisted_chat_runtime_plan(root, None)?;
                runtime_plan::store_persisted_runtime_fleet_plan(root, None)?;
                next_state.local_preset = None;
                next_state.realized_context_tokens = Some(validated_context);
            } else {
                let selected_plan = if let Some(plan) = previous_plan {
                    runtime_plan::clear_chat_plan_env(env_map);
                    runtime_plan::apply_chat_runtime_plan_env(root, plan, env_map)?;
                    runtime_plan::store_persisted_chat_runtime_plan(root, Some(plan))?;
                    plan.clone()
                } else {
                    clear_stale_persisted_plan_for_selection(root, next_state)?;
                    runtime_plan::apply_chat_runtime_plan(root, env_map)?
                        .context("failed to resolve local runtime plan for requested model")?
                };
                apply_local_runtime_plan(next_state, &selected_plan);
                let fleet_plan =
                    runtime_plan::resolve_runtime_fleet_plan(root, env_map, Some(&selected_plan))?;
                apply_runtime_fleet_plan(next_state, &fleet_plan);
                runtime_plan::store_persisted_runtime_fleet_plan(root, Some(&fleet_plan))?;
            }
            runtime_state::apply_runtime_state_to_env_map(env_map, next_state);
        }
    }

    runtime_env::save_runtime_state_projection(root, next_state, env_map)
}

fn seed_requested_local_runtime_env(
    env_map: &mut BTreeMap<String, String>,
    next_state: &runtime_state::InferenceRuntimeState,
) {
    env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
    env_map.remove("CTOX_API_PROVIDER");
    if let Some(requested_model) = next_state
        .requested_model
        .as_deref()
        .or(next_state.active_model.as_deref())
        .or(next_state.base_model.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let requested_model = requested_model.to_string();
        env_map.insert("CTOX_CHAT_MODEL".to_string(), requested_model.clone());
        env_map.insert("CTOX_CHAT_MODEL_BASE".to_string(), requested_model.clone());
        env_map.insert("CTOX_ACTIVE_MODEL".to_string(), requested_model);
    }
    if let Some(preset) = next_state
        .local_preset
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        env_map.insert("CTOX_CHAT_LOCAL_PRESET".to_string(), preset.to_string());
    }
}

fn clear_stale_persisted_plan_for_selection(
    root: &Path,
    next_state: &runtime_state::InferenceRuntimeState,
) -> Result<()> {
    let Some(persisted_plan) = runtime_plan::load_persisted_chat_runtime_plan(root)? else {
        return Ok(());
    };
    let requested_model = next_state
        .active_or_selected_model()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let requested_preset = next_state
        .local_preset
        .as_deref()
        .map(runtime_plan::ChatPreset::from_label);
    let model_matches =
        requested_model.is_some_and(|model| persisted_plan.model.eq_ignore_ascii_case(model));
    let preset_matches = requested_preset.is_none_or(|preset| persisted_plan.preset == preset);
    if model_matches && preset_matches {
        return Ok(());
    }
    runtime_plan::store_persisted_chat_runtime_plan(root, None)?;
    runtime_plan::store_persisted_runtime_fleet_plan(root, None)
}

fn apply_local_runtime_plan(
    next_state: &mut runtime_state::InferenceRuntimeState,
    plan: &runtime_plan::ChatRuntimePlan,
) {
    let engine_port = engine::runtime_config_for_model(&plan.model)
        .ok()
        .map(|runtime| runtime.port)
        .unwrap_or_else(runtime_state::default_local_engine_port);
    next_state.source = runtime_state::InferenceSource::Local;
    next_state.base_model = Some(plan.model.clone());
    next_state.requested_model = Some(plan.model.clone());
    next_state.active_model = Some(plan.model.clone());
    next_state.engine_model = Some(plan.model.clone());
    next_state.engine_port = Some(engine_port);
    next_state.realized_context_tokens = Some(plan.max_seq_len);
    next_state.local_preset = Some(plan.preset.label().to_string());
    next_state.upstream_base_url = runtime_state::local_upstream_base_url(engine_port);
    next_state.adapter_tuning = model_adapters::runtime_adapter_tuning_for_local_plan(
        &plan.model,
        plan.preset,
        plan.max_seq_len,
    );
}

fn apply_runtime_fleet_plan(
    next_state: &mut runtime_state::InferenceRuntimeState,
    plan: &runtime_plan::RuntimeFleetPlan,
) {
    apply_auxiliary_runtime_plan(
        &mut next_state.embedding,
        plan.embedding.as_ref(),
        engine::AuxiliaryRole::Embedding,
    );
    apply_auxiliary_runtime_plan(
        &mut next_state.transcription,
        plan.transcription.as_ref(),
        engine::AuxiliaryRole::Stt,
    );
    apply_auxiliary_runtime_plan(
        &mut next_state.speech,
        plan.speech.as_ref(),
        engine::AuxiliaryRole::Tts,
    );
}

fn apply_auxiliary_runtime_plan(
    target: &mut runtime_state::AuxiliaryRuntimeState,
    plan: Option<&runtime_plan::AuxiliaryRuntimePlan>,
    role: engine::AuxiliaryRole,
) {
    let Some(plan) = plan else {
        *target = runtime_state::AuxiliaryRuntimeState {
            enabled: false,
            configured_model: None,
            port: None,
            base_url: None,
        };
        return;
    };
    let host = runtime_state::default_proxy_host();
    target.enabled = true;
    target.configured_model = Some(plan.display_model.clone());
    target.port = Some(plan.port);
    target.base_url = Some(match role {
        engine::AuxiliaryRole::Embedding
        | engine::AuxiliaryRole::Stt
        | engine::AuxiliaryRole::Vision => format!("http://{host}:{}", plan.port),
        engine::AuxiliaryRole::Tts => format!("ws://{host}:{}", plan.port),
    });
}

pub fn update_runtime_switch_phase(
    root: &Path,
    phase: RuntimeSwitchPhase,
    error: Option<&str>,
) -> Result<Option<RuntimeSwitchTransaction>> {
    let Some(mut transaction) = load_runtime_switch_transaction(root)? else {
        return Ok(None);
    };
    transaction.phase = phase;
    transaction.error = error
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    transaction.updated_at_epoch_secs = runtime_contract::current_epoch_secs();
    if let Some(active_model) = runtime_state::load_runtime_state(root)?
        .and_then(|state| state.active_model)
        .filter(|value| !value.trim().is_empty())
    {
        transaction.next_active_model = Some(active_model);
    }
    persist_runtime_switch_transaction(root, &transaction)?;
    Ok(Some(transaction))
}

pub fn reconcile_runtime_switch_transaction(
    root: &Path,
) -> Result<Option<RuntimeSwitchTransaction>> {
    let Some(transaction) = load_runtime_switch_transaction(root)? else {
        return Ok(None);
    };
    if transaction.phase.is_terminal() && transaction.phase != RuntimeSwitchPhase::Committed {
        return Ok(Some(transaction));
    }

    let stale_lock_detected = stale_runtime_switch_lock_detected(root);
    if stale_lock_detected {
        clear_stale_runtime_switch_lock(root);
    }
    let current_state = runtime_state::load_runtime_state(root)?.or_else(|| {
        if transaction.requested_source == runtime_state::InferenceSource::Api {
            runtime_state::load_or_resolve_runtime_state(root).ok()
        } else {
            None
        }
    });
    if transaction.requested_source == runtime_state::InferenceSource::Local {
        if let Some(state) = current_state.as_ref() {
            let _ = promote_local_runtime_starting_workload_if_ready(
                root,
                state,
                LOCAL_RUNTIME_READY_STABILITY_PASSES,
                Duration::from_millis(LOCAL_RUNTIME_READY_STABILITY_POLL_MILLIS),
            );
        }
    }
    let ownership = runtime_contract::load_runtime_ownership_state(root).unwrap_or_default();
    let primary_workload = ownership
        .workloads
        .iter()
        .find(|entry| entry.role == runtime_contract::BackendRole::Chat);
    let requested_workload_lost = current_state.as_ref().is_some_and(|state| {
        primary_workload.is_some_and(|workload| {
            workload
                .model
                .trim()
                .eq_ignore_ascii_case(transaction.requested_model.as_str())
                && matches!(
                    workload.phase,
                    runtime_contract::RuntimeResidencyPhase::Starting
                        | runtime_contract::RuntimeResidencyPhase::Active
                )
                && !local_runtime_requested_workload_is_alive(root, state, workload)
        })
    });
    let local_runtime_committed = current_state.as_ref().is_some_and(|state| {
        state
            .active_or_selected_model()
            .map(str::trim)
            .is_some_and(|model| {
                if !model.eq_ignore_ascii_case(transaction.requested_model.as_str()) {
                    return false;
                }
                let owned_and_active = local_runtime_is_owned_and_active(root, state);
                if !owned_and_active {
                    return false;
                }
                local_runtime_is_ready(root, state)
                    || (transaction.phase == RuntimeSwitchPhase::Committed
                        && local_runtime_remains_committed_under_load(root, state))
            })
    });

    if transaction.phase == RuntimeSwitchPhase::Committed {
        return match transaction.requested_source {
            runtime_state::InferenceSource::Local if !local_runtime_committed => {
                update_runtime_switch_phase(
                    root,
                    RuntimeSwitchPhase::Failed,
                    Some(&format!(
                        "committed local runtime {} lost ownership or readiness",
                        transaction.requested_model.trim()
                    )),
                )
            }
            _ => Ok(Some(transaction)),
        };
    }

    if transaction.requested_source == runtime_state::InferenceSource::Local
        && stale_lock_detected
        && !local_runtime_committed
    {
        let requested_model = transaction.requested_model.trim();
        let requested_starting = current_state.as_ref().is_some_and(|state| {
            primary_workload.is_some_and(|workload| {
                workload.model.trim().eq_ignore_ascii_case(requested_model)
                    && workload.phase == runtime_contract::RuntimeResidencyPhase::Starting
                    && local_runtime_starting_workload_is_alive(root, state, workload)
            })
        });
        if !requested_starting {
            return update_runtime_switch_phase(
                root,
                RuntimeSwitchPhase::Failed,
                Some(&format!(
                    "runtime switch owner disappeared before commit for {}",
                    requested_model
                )),
            );
        }
    }

    if transaction.requested_source == runtime_state::InferenceSource::Local
        && !local_runtime_committed
        && requested_workload_lost
    {
        return update_runtime_switch_phase(
            root,
            RuntimeSwitchPhase::Failed,
            Some(&format!(
                "local runtime startup for {} lost process ownership before readiness",
                transaction.requested_model.trim()
            )),
        );
    }

    let next_phase = match transaction.requested_source {
        runtime_state::InferenceSource::Api => match primary_workload {
            Some(_) => RuntimeSwitchPhase::Draining,
            None => RuntimeSwitchPhase::Committed,
        },
        runtime_state::InferenceSource::Local => {
            if local_runtime_committed {
                RuntimeSwitchPhase::Committed
            } else {
                match primary_workload {
                    Some(workload)
                        if current_state.as_ref().is_some_and(|state| {
                            workload
                                .model
                                .trim()
                                .eq_ignore_ascii_case(transaction.requested_model.as_str())
                                && workload.phase
                                    == runtime_contract::RuntimeResidencyPhase::Starting
                                && local_runtime_starting_workload_is_alive(root, state, workload)
                        }) =>
                    {
                        RuntimeSwitchPhase::Warming
                    }
                    Some(workload)
                        if workload
                            .model
                            .trim()
                            .eq_ignore_ascii_case(transaction.requested_model.as_str())
                            && current_state.as_ref().is_some_and(|state| {
                                local_runtime_requested_workload_is_alive(root, state, workload)
                            }) =>
                    {
                        RuntimeSwitchPhase::Warming
                    }
                    Some(_) => RuntimeSwitchPhase::Draining,
                    None => RuntimeSwitchPhase::Preparing,
                }
            }
        }
    };

    if next_phase == transaction.phase {
        if transaction.requested_source == runtime_state::InferenceSource::Local
            && transaction.phase == RuntimeSwitchPhase::Warming
        {
            if let (Some(state), Some(workload)) = (current_state.as_ref(), primary_workload) {
                if workload
                    .model
                    .trim()
                    .eq_ignore_ascii_case(transaction.requested_model.as_str())
                    && workload.phase == runtime_contract::RuntimeResidencyPhase::Starting
                    && !local_runtime_starting_workload_is_alive(root, state, workload)
                {
                    return update_runtime_switch_phase(
                        root,
                        RuntimeSwitchPhase::Failed,
                        Some(&format!(
                            "local runtime startup for {} lost process ownership before readiness",
                            transaction.requested_model.trim()
                        )),
                    );
                }
            }
        }
        return Ok(Some(transaction));
    }
    update_runtime_switch_phase(root, next_phase, None)
}

#[cfg(test)]
#[path = "runtime_control_boundary_tests.rs"]
mod boundary_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(unix)]
    use std::{fs, os::unix::net::UnixListener};

    fn make_temp_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ctox-runtime-control-test-{unique}"));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        root
    }

    fn test_runtime_state(
        source: runtime_state::InferenceSource,
    ) -> runtime_state::InferenceRuntimeState {
        runtime_state::InferenceRuntimeState {
            version: 1,
            source,
            local_runtime: runtime_state::LocalRuntimeKind::Candle,
            base_model: None,
            requested_model: None,
            active_model: None,
            engine_model: None,
            engine_port: None,
            realized_context_tokens: None,
            proxy_host: runtime_state::default_proxy_host().to_string(),
            proxy_port: runtime_state::default_proxy_port(),
            upstream_base_url: runtime_state::default_api_upstream_base_url().to_string(),
            local_preset: None,
            boost: runtime_state::BoostRuntimeState::default(),
            adapter_tuning: runtime_state::AdapterRuntimeTuning::default(),
            embedding: runtime_state::AuxiliaryRuntimeState::default(),
            transcription: runtime_state::AuxiliaryRuntimeState::default(),
            speech: runtime_state::AuxiliaryRuntimeState::default(),
            vision: runtime_state::AuxiliaryRuntimeState::default(),
        }
    }

    #[test]
    fn api_runtime_selection_persists_requested_switch_transaction() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();

        let change = apply_runtime_selection(&root, "gpt-5.4", None).unwrap();
        assert_eq!(
            change.next_state.source,
            runtime_state::InferenceSource::Api
        );
        assert_eq!(change.next_state.active_model.as_deref(), Some("gpt-5.4"));

        let txn = load_runtime_switch_transaction(&root)
            .unwrap()
            .expect("switch transaction");
        assert_eq!(txn.phase, RuntimeSwitchPhase::Requested);
        assert_eq!(txn.requested_model, "gpt-5.4");
        assert_eq!(txn.requested_source, runtime_state::InferenceSource::Api);
    }

    #[test]
    fn switch_phase_updates_error_state() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();
        apply_runtime_selection(&root, "gpt-5.4", None).unwrap();

        let txn = update_runtime_switch_phase(
            &root,
            RuntimeSwitchPhase::Failed,
            Some("backend launch failed"),
        )
        .unwrap()
        .expect("updated transaction");

        assert_eq!(txn.phase, RuntimeSwitchPhase::Failed);
        assert_eq!(txn.error.as_deref(), Some("backend launch failed"));
    }

    #[test]
    fn reconciliation_marks_local_switch_as_warming_when_backend_is_starting() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();
        persist_runtime_switch_transaction(
            &root,
            &RuntimeSwitchTransaction {
                version: 3,
                phase: RuntimeSwitchPhase::Requested,
                requested_model: "openai/gpt-oss-20b".to_string(),
                requested_source: runtime_state::InferenceSource::Local,
                requested_local_runtime: runtime_state::LocalRuntimeKind::Candle,
                requested_preset: Some("Quality".to_string()),
                previous_source: Some(runtime_state::InferenceSource::Local),
                previous_local_runtime: Some(runtime_state::LocalRuntimeKind::Candle),
                previous_requested_model: Some("Qwen/Qwen3.5-4B".to_string()),
                previous_active_model: Some("Qwen/Qwen3.5-4B".to_string()),
                previous_preset: Some("Quality".to_string()),
                previous_plan: None,
                next_active_model: Some("openai/gpt-oss-20b".to_string()),
                started_at_epoch_secs: runtime_contract::current_epoch_secs(),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
                error: None,
            },
        )
        .unwrap();
        std::fs::write(
            root.join("runtime")
                .join(runtime_contract::BackendRole::Chat.pid_file_name()),
            format!("{}\n", std::process::id()),
        )
        .unwrap();
        runtime_contract::sync_backend_runtime_residency(
            &root,
            runtime_contract::BackendRuntimeResidency {
                role: runtime_contract::BackendRole::Chat,
                phase: runtime_contract::RuntimeResidencyPhase::Starting,
                model: "openai/gpt-oss-20b".to_string(),
                pid: Some(std::process::id()),
                port: Some(1234),
                health_path: Some("/health".to_string()),
                launcher_kind: Some("engine".to_string()),
                compute_target: None,
                visible_devices: vec![0],
                reserved_mb_by_gpu: BTreeMap::from([(0usize, 1024u64)]),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
            },
        )
        .unwrap();
        let pid_path = root
            .join("runtime")
            .join(runtime_contract::BackendRole::Chat.pid_file_name());
        if let Some(parent) = pid_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&pid_path, format!("{}\n", std::process::id())).unwrap();

        let txn = reconcile_runtime_switch_transaction(&root)
            .unwrap()
            .expect("reconciled transaction");
        assert_eq!(txn.phase, RuntimeSwitchPhase::Warming);
    }

    #[cfg(unix)]
    #[test]
    fn reconciliation_promotes_starting_workload_to_committed_when_socket_is_ready() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();
        let mut state = runtime_state::load_or_resolve_runtime_state(&root).unwrap();
        state.source = runtime_state::InferenceSource::Local;
        state.active_model = Some("Qwen/Qwen3.5-9B".to_string());
        state.requested_model = Some("Qwen/Qwen3.5-9B".to_string());
        state.engine_model = Some("Qwen/Qwen3.5-9B".to_string());
        state.engine_port = Some(1234);
        state.upstream_base_url = "http://127.0.0.1:1234".to_string();
        runtime_env::save_runtime_state_projection(&root, &state, &BTreeMap::new()).unwrap();
        persist_runtime_switch_transaction(
            &root,
            &RuntimeSwitchTransaction {
                version: 3,
                phase: RuntimeSwitchPhase::Warming,
                requested_model: "Qwen/Qwen3.5-9B".to_string(),
                requested_source: runtime_state::InferenceSource::Local,
                requested_local_runtime: runtime_state::LocalRuntimeKind::Candle,
                requested_preset: Some("Performance".to_string()),
                previous_source: Some(runtime_state::InferenceSource::Local),
                previous_local_runtime: Some(runtime_state::LocalRuntimeKind::Candle),
                previous_requested_model: Some("Qwen/Qwen3.5-27B".to_string()),
                previous_active_model: Some("Qwen/Qwen3.5-27B".to_string()),
                previous_preset: Some("Quality".to_string()),
                previous_plan: None,
                next_active_model: Some("Qwen/Qwen3.5-9B".to_string()),
                started_at_epoch_secs: runtime_contract::current_epoch_secs(),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
                error: None,
            },
        )
        .unwrap();
        let socket_path = runtime_kernel::managed_runtime_socket_path(
            &root,
            runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
        );
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let listener = UnixListener::bind(&socket_path).unwrap();
        let accept_loop = {
            let listener = listener.try_clone().unwrap();
            thread::spawn(move || {
                for _ in 0..LOCAL_RUNTIME_READY_STABILITY_PASSES {
                    let _ = listener.accept();
                }
            })
        };
        runtime_contract::sync_backend_runtime_residency(
            &root,
            runtime_contract::BackendRuntimeResidency {
                role: runtime_contract::BackendRole::Chat,
                phase: runtime_contract::RuntimeResidencyPhase::Starting,
                model: "Qwen/Qwen3.5-9B".to_string(),
                pid: Some(std::process::id()),
                port: Some(1234),
                health_path: Some("/health".to_string()),
                launcher_kind: Some("engine".to_string()),
                compute_target: None,
                visible_devices: vec![0, 1, 2, 3],
                reserved_mb_by_gpu: BTreeMap::new(),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
            },
        )
        .unwrap();
        let pid_path = root
            .join("runtime")
            .join(runtime_contract::BackendRole::Chat.pid_file_name());
        if let Some(parent) = pid_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&pid_path, format!("{}\n", std::process::id())).unwrap();

        let txn = reconcile_runtime_switch_transaction(&root)
            .unwrap()
            .expect("reconciled transaction");
        assert_eq!(txn.phase, RuntimeSwitchPhase::Committed);

        let ownership = runtime_contract::load_runtime_ownership_state(&root).unwrap();
        let workload = ownership
            .workloads
            .into_iter()
            .find(|entry| entry.role == runtime_contract::BackendRole::Chat)
            .expect("chat workload");
        assert_eq!(
            workload.phase,
            runtime_contract::RuntimeResidencyPhase::Active
        );

        drop(listener);
        accept_loop.join().unwrap();
    }

    #[test]
    fn local_runtime_is_not_ready_from_ownership_alone() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();
        let mut state = runtime_state::load_or_resolve_runtime_state(&root).unwrap();
        state.source = runtime_state::InferenceSource::Local;
        state.active_model = Some("openai/gpt-oss-20b".to_string());
        state.requested_model = Some("openai/gpt-oss-20b".to_string());
        state.upstream_base_url = "http://127.0.0.1:1234".to_string();
        runtime_contract::sync_backend_runtime_residency(
            &root,
            runtime_contract::BackendRuntimeResidency {
                role: runtime_contract::BackendRole::Chat,
                phase: runtime_contract::RuntimeResidencyPhase::Active,
                model: "openai/gpt-oss-20b".to_string(),
                pid: Some(std::process::id()),
                port: Some(1234),
                health_path: Some("/health".to_string()),
                launcher_kind: Some("engine".to_string()),
                compute_target: None,
                visible_devices: vec![0, 1, 2, 3],
                reserved_mb_by_gpu: BTreeMap::from([(0usize, 1024u64)]),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
            },
        )
        .unwrap();
        let pid_path = root
            .join("runtime")
            .join(runtime_contract::BackendRole::Chat.pid_file_name());
        if let Some(parent) = pid_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&pid_path, format!("{}\n", std::process::id())).unwrap();

        assert!(!local_runtime_is_ready(&root, &state));
    }

    #[cfg(unix)]
    #[test]
    fn reconciliation_marks_local_switch_committed_when_runtime_socket_is_ready() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();
        let mut state = runtime_state::load_or_resolve_runtime_state(&root).unwrap();
        state.source = runtime_state::InferenceSource::Local;
        state.active_model = Some("openai/gpt-oss-20b".to_string());
        state.requested_model = Some("openai/gpt-oss-20b".to_string());
        state.engine_model = Some("openai/gpt-oss-20b".to_string());
        state.engine_port = Some(1234);
        state.upstream_base_url = "http://127.0.0.1:1234".to_string();
        runtime_env::save_runtime_state_projection(&root, &state, &BTreeMap::new()).unwrap();
        persist_runtime_switch_transaction(
            &root,
            &RuntimeSwitchTransaction {
                version: 3,
                phase: RuntimeSwitchPhase::Warming,
                requested_model: "openai/gpt-oss-20b".to_string(),
                requested_source: runtime_state::InferenceSource::Local,
                requested_local_runtime: runtime_state::LocalRuntimeKind::Candle,
                requested_preset: Some("Quality".to_string()),
                previous_source: Some(runtime_state::InferenceSource::Local),
                previous_local_runtime: Some(runtime_state::LocalRuntimeKind::Candle),
                previous_requested_model: Some("Qwen/Qwen3.5-4B".to_string()),
                previous_active_model: Some("Qwen/Qwen3.5-4B".to_string()),
                previous_preset: Some("Quality".to_string()),
                previous_plan: None,
                next_active_model: Some("openai/gpt-oss-20b".to_string()),
                started_at_epoch_secs: runtime_contract::current_epoch_secs(),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
                error: None,
            },
        )
        .unwrap();
        let socket_path = runtime_kernel::managed_runtime_socket_path(
            &root,
            runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
        );
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let listener = UnixListener::bind(&socket_path).unwrap();
        let accept_loop = {
            let listener = listener.try_clone().unwrap();
            thread::spawn(move || {
                for _ in 0..LOCAL_RUNTIME_READY_STABILITY_PASSES {
                    let _ = listener.accept();
                }
            })
        };
        runtime_contract::sync_backend_runtime_residency(
            &root,
            runtime_contract::BackendRuntimeResidency {
                role: runtime_contract::BackendRole::Chat,
                phase: runtime_contract::RuntimeResidencyPhase::Active,
                model: "openai/gpt-oss-20b".to_string(),
                pid: Some(std::process::id()),
                port: Some(1234),
                health_path: Some("/health".to_string()),
                launcher_kind: Some("engine".to_string()),
                compute_target: None,
                visible_devices: vec![0, 1, 2, 3],
                reserved_mb_by_gpu: BTreeMap::from([(0usize, 1024u64)]),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
            },
        )
        .unwrap();
        let pid_path = root
            .join("runtime")
            .join(runtime_contract::BackendRole::Chat.pid_file_name());
        if let Some(parent) = pid_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&pid_path, format!("{}\n", std::process::id())).unwrap();

        let txn = reconcile_runtime_switch_transaction(&root)
            .unwrap()
            .expect("reconciled transaction");
        assert_eq!(txn.phase, RuntimeSwitchPhase::Committed);

        accept_loop.join().unwrap();
        drop(listener);
        let _ = std::fs::remove_file(socket_path);
    }

    #[cfg(unix)]
    #[test]
    fn reconciliation_does_not_commit_local_switch_from_socket_alone() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();
        let mut state = runtime_state::load_or_resolve_runtime_state(&root).unwrap();
        state.source = runtime_state::InferenceSource::Local;
        state.active_model = Some("openai/gpt-oss-20b".to_string());
        state.requested_model = Some("openai/gpt-oss-20b".to_string());
        state.engine_model = Some("openai/gpt-oss-20b".to_string());
        state.engine_port = Some(1234);
        state.upstream_base_url = "http://127.0.0.1:1234".to_string();
        runtime_env::save_runtime_state_projection(&root, &state, &BTreeMap::new()).unwrap();
        persist_runtime_switch_transaction(
            &root,
            &RuntimeSwitchTransaction {
                version: 3,
                phase: RuntimeSwitchPhase::Warming,
                requested_model: "openai/gpt-oss-20b".to_string(),
                requested_source: runtime_state::InferenceSource::Local,
                requested_local_runtime: runtime_state::LocalRuntimeKind::Candle,
                requested_preset: Some("Quality".to_string()),
                previous_source: Some(runtime_state::InferenceSource::Local),
                previous_local_runtime: Some(runtime_state::LocalRuntimeKind::Candle),
                previous_requested_model: Some("Qwen/Qwen3.5-4B".to_string()),
                previous_active_model: Some("Qwen/Qwen3.5-4B".to_string()),
                previous_preset: Some("Quality".to_string()),
                previous_plan: None,
                next_active_model: Some("openai/gpt-oss-20b".to_string()),
                started_at_epoch_secs: runtime_contract::current_epoch_secs(),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
                error: None,
            },
        )
        .unwrap();
        let socket_path = runtime_kernel::managed_runtime_socket_path(
            &root,
            runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
        );
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let listener = UnixListener::bind(&socket_path).unwrap();

        let txn = reconcile_runtime_switch_transaction(&root)
            .unwrap()
            .expect("reconciled transaction");
        assert_ne!(txn.phase, RuntimeSwitchPhase::Committed);

        drop(listener);
        let _ = std::fs::remove_file(socket_path);
    }

    #[test]
    fn reconciliation_marks_api_switch_committed_after_local_backend_is_gone() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();
        let transaction = RuntimeSwitchTransaction {
            version: 3,
            phase: RuntimeSwitchPhase::Draining,
            requested_model: "gpt-5.4".to_string(),
            requested_source: runtime_state::InferenceSource::Api,
            requested_local_runtime: runtime_state::LocalRuntimeKind::Candle,
            requested_preset: None,
            previous_source: Some(runtime_state::InferenceSource::Local),
            previous_local_runtime: Some(runtime_state::LocalRuntimeKind::Candle),
            previous_requested_model: Some("openai/gpt-oss-20b".to_string()),
            previous_active_model: Some("openai/gpt-oss-20b".to_string()),
            previous_preset: Some("Quality".to_string()),
            previous_plan: None,
            next_active_model: Some("gpt-5.4".to_string()),
            started_at_epoch_secs: runtime_contract::current_epoch_secs(),
            updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
            error: None,
        };
        persist_runtime_switch_transaction(&root, &transaction).unwrap();

        let txn = reconcile_runtime_switch_transaction(&root)
            .unwrap()
            .expect("reconciled transaction");
        assert_eq!(txn.phase, RuntimeSwitchPhase::Committed);
    }

    #[test]
    fn committed_local_switch_downgrades_to_failed_when_runtime_disappears() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();
        let mut state = runtime_state::load_or_resolve_runtime_state(&root).unwrap();
        state.source = runtime_state::InferenceSource::Local;
        state.active_model = Some("Qwen/Qwen3.5-27B".to_string());
        state.requested_model = Some("Qwen/Qwen3.5-27B".to_string());
        state.engine_model = Some("Qwen/Qwen3.5-27B".to_string());
        state.engine_port = Some(1235);
        state.upstream_base_url = "http://127.0.0.1:1235".to_string();
        runtime_env::save_runtime_state_projection(&root, &state, &BTreeMap::new()).unwrap();
        persist_runtime_switch_transaction(
            &root,
            &RuntimeSwitchTransaction {
                version: 3,
                phase: RuntimeSwitchPhase::Committed,
                requested_model: "Qwen/Qwen3.5-27B".to_string(),
                requested_source: runtime_state::InferenceSource::Local,
                requested_local_runtime: runtime_state::LocalRuntimeKind::Candle,
                requested_preset: Some("Quality".to_string()),
                previous_source: Some(runtime_state::InferenceSource::Local),
                previous_local_runtime: Some(runtime_state::LocalRuntimeKind::Candle),
                previous_requested_model: Some("Qwen/Qwen3.5-9B".to_string()),
                previous_active_model: Some("Qwen/Qwen3.5-9B".to_string()),
                previous_preset: Some("Quality".to_string()),
                previous_plan: None,
                next_active_model: Some("Qwen/Qwen3.5-27B".to_string()),
                started_at_epoch_secs: runtime_contract::current_epoch_secs(),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
                error: None,
            },
        )
        .unwrap();

        let txn = reconcile_runtime_switch_transaction(&root)
            .unwrap()
            .expect("reconciled transaction");
        assert_eq!(txn.phase, RuntimeSwitchPhase::Failed);
        assert!(txn
            .error
            .as_deref()
            .is_some_and(|value| value.contains("lost ownership or readiness")));
    }

    #[test]
    fn committed_local_switch_stays_committed_when_owned_runtime_is_busy() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();
        let mut state = runtime_state::load_or_resolve_runtime_state(&root).unwrap();
        state.source = runtime_state::InferenceSource::Local;
        state.active_model = Some("google/gemma-4-E2B-it".to_string());
        state.requested_model = Some("google/gemma-4-E2B-it".to_string());
        state.engine_model = Some("google/gemma-4-E2B-it".to_string());
        state.engine_port = Some(1235);
        state.upstream_base_url = "http://127.0.0.1:1235".to_string();
        runtime_env::save_runtime_state_projection(&root, &state, &BTreeMap::new()).unwrap();
        persist_runtime_switch_transaction(
            &root,
            &RuntimeSwitchTransaction {
                version: 3,
                phase: RuntimeSwitchPhase::Committed,
                requested_model: "google/gemma-4-E2B-it".to_string(),
                requested_source: runtime_state::InferenceSource::Local,
                requested_local_runtime: runtime_state::LocalRuntimeKind::Candle,
                requested_preset: Some("Quality".to_string()),
                previous_source: Some(runtime_state::InferenceSource::Local),
                previous_local_runtime: Some(runtime_state::LocalRuntimeKind::Candle),
                previous_requested_model: Some("Qwen/Qwen3.5-9B".to_string()),
                previous_active_model: Some("Qwen/Qwen3.5-9B".to_string()),
                previous_preset: Some("Quality".to_string()),
                previous_plan: None,
                next_active_model: Some("google/gemma-4-E2B-it".to_string()),
                started_at_epoch_secs: runtime_contract::current_epoch_secs(),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
                error: None,
            },
        )
        .unwrap();
        runtime_contract::sync_backend_runtime_residency(
            &root,
            runtime_contract::BackendRuntimeResidency {
                role: runtime_contract::BackendRole::Chat,
                phase: runtime_contract::RuntimeResidencyPhase::Active,
                model: "google/gemma-4-E2B-it".to_string(),
                pid: Some(std::process::id()),
                port: Some(1235),
                health_path: Some("/health".to_string()),
                launcher_kind: Some("engine".to_string()),
                compute_target: None,
                visible_devices: vec![0],
                reserved_mb_by_gpu: BTreeMap::from([(0usize, 1024u64)]),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
            },
        )
        .unwrap();
        let pid_path = root
            .join("runtime")
            .join(runtime_contract::BackendRole::Chat.pid_file_name());
        if let Some(parent) = pid_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&pid_path, format!("{}\n", std::process::id())).unwrap();

        let txn = reconcile_runtime_switch_transaction(&root)
            .unwrap()
            .expect("reconciled transaction");
        assert_eq!(txn.phase, RuntimeSwitchPhase::Committed);
        assert!(txn.error.is_none());
    }

    #[test]
    fn warming_local_switch_fails_when_starting_workload_loses_process() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();
        let mut state = runtime_state::load_or_resolve_runtime_state(&root).unwrap();
        state.source = runtime_state::InferenceSource::Local;
        state.active_model = Some("zai-org/GLM-4.7-Flash".to_string());
        state.requested_model = Some("zai-org/GLM-4.7-Flash".to_string());
        state.engine_model = Some("zai-org/GLM-4.7-Flash".to_string());
        state.engine_port = Some(1236);
        state.upstream_base_url = "http://127.0.0.1:1236".to_string();
        runtime_env::save_runtime_state_projection(&root, &state, &BTreeMap::new()).unwrap();
        persist_runtime_switch_transaction(
            &root,
            &RuntimeSwitchTransaction {
                version: 3,
                phase: RuntimeSwitchPhase::Warming,
                requested_model: "zai-org/GLM-4.7-Flash".to_string(),
                requested_source: runtime_state::InferenceSource::Local,
                requested_local_runtime: runtime_state::LocalRuntimeKind::Candle,
                requested_preset: Some("Quality".to_string()),
                previous_source: Some(runtime_state::InferenceSource::Local),
                previous_local_runtime: Some(runtime_state::LocalRuntimeKind::Candle),
                previous_requested_model: Some("Qwen/Qwen3.5-27B".to_string()),
                previous_active_model: Some("Qwen/Qwen3.5-27B".to_string()),
                previous_preset: Some("Quality".to_string()),
                previous_plan: None,
                next_active_model: Some("zai-org/GLM-4.7-Flash".to_string()),
                started_at_epoch_secs: runtime_contract::current_epoch_secs(),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
                error: None,
            },
        )
        .unwrap();
        runtime_contract::sync_backend_runtime_residency(
            &root,
            runtime_contract::BackendRuntimeResidency {
                role: runtime_contract::BackendRole::Chat,
                phase: runtime_contract::RuntimeResidencyPhase::Starting,
                model: "zai-org/GLM-4.7-Flash".to_string(),
                pid: Some(999_999),
                port: Some(1236),
                health_path: Some("/health".to_string()),
                launcher_kind: Some("engine".to_string()),
                compute_target: None,
                visible_devices: vec![0, 1, 2, 3],
                reserved_mb_by_gpu: BTreeMap::from([(0usize, 1024u64)]),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
            },
        )
        .unwrap();

        let txn = reconcile_runtime_switch_transaction(&root)
            .unwrap()
            .expect("reconciled transaction");
        assert_eq!(txn.phase, RuntimeSwitchPhase::Failed);
        assert!(txn
            .error
            .as_deref()
            .is_some_and(|value| { value.contains("lost process ownership before readiness") }));
    }

    #[test]
    fn local_model_switch_redeploys_full_runtime_fleet() {
        let previous_state = runtime_state::InferenceRuntimeState {
            source: runtime_state::InferenceSource::Local,
            active_model: Some("Qwen/Qwen3.5-4B".to_string()),
            requested_model: Some("Qwen/Qwen3.5-4B".to_string()),
            ..test_runtime_state(runtime_state::InferenceSource::Local)
        };
        let next_state = runtime_state::InferenceRuntimeState {
            source: runtime_state::InferenceSource::Local,
            active_model: Some("Qwen/Qwen3.5-9B".to_string()),
            requested_model: Some("Qwen/Qwen3.5-9B".to_string()),
            ..test_runtime_state(runtime_state::InferenceSource::Local)
        };
        let change = RuntimeSelectionChange {
            previous_state,
            next_state: next_state.clone(),
        };

        assert!(should_redeploy_local_runtime_fleet(&change, &next_state));
        assert!(!should_release_previous_local_backend(&change, &next_state));
    }

    #[test]
    fn unchanged_local_model_does_not_redeploy_full_runtime_fleet() {
        let previous_state = runtime_state::InferenceRuntimeState {
            source: runtime_state::InferenceSource::Local,
            active_model: Some("Qwen/Qwen3.5-4B".to_string()),
            requested_model: Some("Qwen/Qwen3.5-4B".to_string()),
            upstream_base_url: "http://127.0.0.1:1234".to_string(),
            ..test_runtime_state(runtime_state::InferenceSource::Local)
        };
        let next_state = runtime_state::InferenceRuntimeState {
            source: runtime_state::InferenceSource::Local,
            active_model: Some("Qwen/Qwen3.5-4B".to_string()),
            requested_model: Some("Qwen/Qwen3.5-4B".to_string()),
            upstream_base_url: "http://127.0.0.1:1234".to_string(),
            ..test_runtime_state(runtime_state::InferenceSource::Local)
        };
        let change = RuntimeSelectionChange {
            previous_state,
            next_state: next_state.clone(),
        };

        assert!(!should_redeploy_local_runtime_fleet(&change, &next_state));
        assert!(!should_release_previous_local_backend(&change, &next_state));
    }

    #[test]
    fn stale_runtime_switch_lock_is_reclaimed() {
        let root = make_temp_root();
        let path = runtime_switch_lock_path(&root);
        std::fs::write(&path, "pid=999999\nmodel=stale\n").unwrap();

        let lease = acquire_runtime_switch_lease(&root, "Qwen/Qwen3.5-4B").unwrap();
        assert!(path.exists());
        drop(lease);
        assert!(!path.exists());
    }

    #[test]
    fn current_runtime_switch_lock_is_not_stale() {
        let root = make_temp_root();
        let path = runtime_switch_lock_path(&root);
        std::fs::write(
            &path,
            format!("pid={}\nmodel=current\n", std::process::id()),
        )
        .unwrap();

        assert!(!runtime_switch_lock_is_stale(&path));
    }

    #[test]
    fn stale_warming_switch_fails_when_lock_owner_is_gone_and_other_model_is_active() {
        let root = make_temp_root();
        runtime_env::save_runtime_env_map(&root, &BTreeMap::new()).unwrap();
        let mut state = runtime_state::load_or_resolve_runtime_state(&root).unwrap();
        state.source = runtime_state::InferenceSource::Local;
        state.active_model = Some("openai/gpt-oss-20b".to_string());
        state.requested_model = Some("openai/gpt-oss-20b".to_string());
        state.engine_model = Some("openai/gpt-oss-20b".to_string());
        state.engine_port = Some(1234);
        state.upstream_base_url = "http://127.0.0.1:1234".to_string();
        runtime_env::save_runtime_state_projection(&root, &state, &BTreeMap::new()).unwrap();
        persist_runtime_switch_transaction(
            &root,
            &RuntimeSwitchTransaction {
                version: 3,
                phase: RuntimeSwitchPhase::Warming,
                requested_model: "zai-org/GLM-4.7-Flash".to_string(),
                requested_source: runtime_state::InferenceSource::Local,
                requested_local_runtime: runtime_state::LocalRuntimeKind::Candle,
                requested_preset: Some("Quality".to_string()),
                previous_source: Some(runtime_state::InferenceSource::Local),
                previous_local_runtime: Some(runtime_state::LocalRuntimeKind::Candle),
                previous_requested_model: Some("openai/gpt-oss-20b".to_string()),
                previous_active_model: Some("openai/gpt-oss-20b".to_string()),
                previous_preset: Some("Quality".to_string()),
                previous_plan: None,
                next_active_model: Some("zai-org/GLM-4.7-Flash".to_string()),
                started_at_epoch_secs: runtime_contract::current_epoch_secs(),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
                error: None,
            },
        )
        .unwrap();
        let lock_path = runtime_switch_lock_path(&root);
        std::fs::write(&lock_path, "pid=999999\nmodel=zai-org/GLM-4.7-Flash\n").unwrap();
        runtime_contract::sync_backend_runtime_residency(
            &root,
            runtime_contract::BackendRuntimeResidency {
                role: runtime_contract::BackendRole::Chat,
                phase: runtime_contract::RuntimeResidencyPhase::Active,
                model: "openai/gpt-oss-20b".to_string(),
                pid: Some(std::process::id()),
                port: Some(1234),
                health_path: Some("/health".to_string()),
                launcher_kind: Some("engine".to_string()),
                compute_target: None,
                visible_devices: vec![0, 1, 2, 3],
                reserved_mb_by_gpu: BTreeMap::from([(0usize, 1024u64)]),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
            },
        )
        .unwrap();

        let txn = reconcile_runtime_switch_transaction(&root)
            .unwrap()
            .expect("reconciled transaction");
        assert_eq!(txn.phase, RuntimeSwitchPhase::Failed);
        assert!(txn.error.as_deref().is_some_and(|value| {
            value.contains("runtime switch owner disappeared before commit")
        }));
        assert!(!lock_path.exists());
    }

    #[test]
    fn local_runtime_selection_does_not_reuse_stale_persisted_plan_for_new_model() {
        let root = make_temp_root();
        let previous_plan = runtime_plan::ChatRuntimePlan {
            model: "Qwen/Qwen3.5-4B".to_string(),
            preset: runtime_plan::ChatPreset::Quality,
            quantization: "Q6K".to_string(),
            runtime_isq: Some("Q6K".to_string()),
            max_seq_len: 131_072,
            compaction_threshold_percent: 75,
            compaction_min_tokens: 12_288,
            min_context_floor_applied: true,
            paged_attn: "auto".to_string(),
            pa_cache_type: Some("turboquant3".to_string()),
            pa_memory_fraction: Some("0.80".to_string()),
            pa_context_len: None,
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 1,
            max_seqs: 1,
            cuda_visible_devices: "0,1,2".to_string(),
            device_layers: Some("0:10;1:11;2:11".to_string()),
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: Some(0),
            base_device_ordinal: Some(0),
            moe_experts_backend: None,
            disable_flash_attn: true,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            isq_singlethread: true,
            isq_cpu_threads: None,
            expected_tok_s: 100.0,
            hardware_fingerprint: "test-host".to_string(),
            theoretical_breakdown: runtime_plan::TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 60_000,
                kv_budget_cap_mb: 40_000,
                kv_budget_fraction_milli: 800,
                weight_residency_mb: 4_463,
                kv_cache_mb: 5_504,
                fixed_runtime_base_overhead_mb: 1_440,
                backend_runtime_overhead_mb: 0,
                activation_overhead_mb: 10_368,
                load_peak_overhead_mb: 476,
                safety_headroom_mb: 1_536,
                required_effective_total_budget_mb: 14_795,
                required_total_mb: 25_163,
            },
            rationale: vec!["test".to_string()],
            gpu_allocations: vec![],
        };
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert("CTOX_CHAT_MODEL".to_string(), previous_plan.model.clone());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            previous_plan.model.clone(),
        );
        env_map.insert("CTOX_ACTIVE_MODEL".to_string(), previous_plan.model.clone());
        env_map.insert("CTOX_CHAT_LOCAL_PRESET".to_string(), "Quality".to_string());
        runtime_plan::apply_chat_runtime_plan_env(&root, &previous_plan, &mut env_map).unwrap();
        runtime_plan::store_persisted_chat_runtime_plan(&root, Some(&previous_plan)).unwrap();
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        let change = apply_runtime_selection(&root, "Qwen/Qwen3.5-9B", Some("quality")).unwrap();

        assert_eq!(
            change.next_state.active_model.as_deref(),
            Some("Qwen/Qwen3.5-9B")
        );
        let persisted_state = runtime_state::load_runtime_state(&root)
            .unwrap()
            .expect("persisted state");
        assert_eq!(
            persisted_state.active_model.as_deref(),
            Some("Qwen/Qwen3.5-9B")
        );
        let persisted_plan = runtime_plan::load_persisted_chat_runtime_plan(&root)
            .unwrap()
            .expect("persisted plan");
        assert_eq!(persisted_plan.model, "Qwen/Qwen3.5-9B");
        let fleet_plan = runtime_plan::load_persisted_runtime_fleet_plan(&root)
            .unwrap()
            .expect("persisted fleet plan");
        assert_eq!(
            fleet_plan.chat.as_ref().map(|plan| plan.model.as_str()),
            Some("Qwen/Qwen3.5-9B")
        );
        assert_eq!(
            change.next_state.embedding.configured_model.as_deref(),
            fleet_plan
                .embedding
                .as_ref()
                .map(|plan| plan.display_model.as_str())
        );
    }

    #[test]
    fn rollback_runtime_switch_restores_previous_runtime_selection() {
        let root = make_temp_root();
        let previous_plan = runtime_plan::ChatRuntimePlan {
            model: "openai/gpt-oss-20b".to_string(),
            preset: runtime_plan::ChatPreset::Quality,
            quantization: "mq4".to_string(),
            runtime_isq: None,
            max_seq_len: 131_072,
            compaction_threshold_percent: 70,
            compaction_min_tokens: 12_288,
            min_context_floor_applied: true,
            paged_attn: "1".to_string(),
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: Some(131_072),
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 1,
            max_seqs: 1,
            cuda_visible_devices: "0".to_string(),
            device_layers: None,
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: None,
            base_device_ordinal: Some(0),
            moe_experts_backend: None,
            disable_flash_attn: false,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            isq_singlethread: false,
            isq_cpu_threads: None,
            expected_tok_s: 12.5,
            hardware_fingerprint: "test-host".to_string(),
            theoretical_breakdown: runtime_plan::TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 20_000,
                kv_budget_cap_mb: 8_000,
                kv_budget_fraction_milli: 1000,
                weight_residency_mb: 7_000,
                kv_cache_mb: 6_000,
                fixed_runtime_base_overhead_mb: 512,
                backend_runtime_overhead_mb: 256,
                activation_overhead_mb: 512,
                load_peak_overhead_mb: 512,
                safety_headroom_mb: 512,
                required_effective_total_budget_mb: 15_000,
                required_total_mb: 16_000,
            },
            rationale: vec!["test rollback".to_string()],
            gpu_allocations: vec![runtime_plan::PlannedGpuAllocation {
                gpu_index: 0,
                name: "GPU0".to_string(),
                total_mb: 24_576,
                desktop_reserve_mb: 1_024,
                aux_reserve_mb: 0,
                chat_budget_mb: 16_000,
                backend_overhead_mb: 512,
                activation_overhead_mb: 512,
                load_peak_overhead_mb: 512,
                repeating_weight_mb: 0,
                weight_mb: 7_000,
                kv_cache_mb: 6_000,
                free_headroom_mb: 1_000,
                chat_enabled: true,
            }],
        };
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert("CTOX_CHAT_MODEL".to_string(), previous_plan.model.clone());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            previous_plan.model.clone(),
        );
        env_map.insert("CTOX_ACTIVE_MODEL".to_string(), previous_plan.model.clone());
        env_map.insert("CTOX_CHAT_LOCAL_PRESET".to_string(), "Quality".to_string());
        runtime_plan::apply_chat_runtime_plan_env(&root, &previous_plan, &mut env_map).unwrap();
        runtime_plan::store_persisted_chat_runtime_plan(&root, Some(&previous_plan)).unwrap();
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        let change = apply_runtime_selection(&root, "gpt-5.4", None).unwrap();
        assert_eq!(
            change.next_state.source,
            runtime_state::InferenceSource::Api
        );

        let rollback = rollback_runtime_switch(&root)
            .unwrap()
            .expect("rollback change");
        assert_eq!(
            rollback.next_state.active_model.as_deref(),
            Some("openai/gpt-oss-20b")
        );
        assert_eq!(rollback.next_state.local_preset.as_deref(), Some("Quality"));
        assert_eq!(
            rollback.next_state.source,
            runtime_state::InferenceSource::Local
        );
        let restored_plan = runtime_plan::load_persisted_chat_runtime_plan(&root)
            .unwrap()
            .expect("restored plan");
        assert_eq!(restored_plan.model, "openai/gpt-oss-20b");
    }

    #[test]
    fn apply_runtime_selection_defaults_gemma_e4b_to_candle() {
        let root = make_temp_root();
        let change = apply_runtime_selection(&root, "google/gemma-4-E4B-it", None)
            .expect("Gemma4 E4B should default to Candle unless explicitly overridden");
        assert_eq!(
            change.next_state.local_runtime,
            runtime_state::LocalRuntimeKind::Candle
        );
        assert_eq!(
            change.next_state.active_model.as_deref(),
            Some("google/gemma-4-E4B-it")
        );
    }

    #[test]
    fn apply_runtime_selection_does_not_inherit_stale_process_model_env() {
        let root = make_temp_root();
        std::env::set_var("CTOX_CHAT_MODEL", "openai/gpt-oss-20b");
        std::env::set_var("CTOX_CHAT_MODEL_BASE", "openai/gpt-oss-20b");
        let change = apply_runtime_selection(&root, "google/gemma-4-E2B-it", Some("quality"))
            .expect("explicit runtime switch target should override stale process model env");
        std::env::remove_var("CTOX_CHAT_MODEL");
        std::env::remove_var("CTOX_CHAT_MODEL_BASE");

        assert_eq!(
            change.next_state.active_model.as_deref(),
            Some("google/gemma-4-E2B-it")
        );
        assert_eq!(
            runtime_plan::load_persisted_chat_runtime_plan(&root)
                .unwrap()
                .expect("persisted plan")
                .model,
            "google/gemma-4-E2B-it"
        );
    }

    #[test]
    fn apply_runtime_selection_keeps_explicit_local_model_when_previous_root_points_to_default() {
        let root = make_temp_root();
        let previous_plan = runtime_plan::ChatRuntimePlan {
            model: "openai/gpt-oss-20b".to_string(),
            preset: runtime_plan::ChatPreset::Quality,
            quantization: "mq4".to_string(),
            runtime_isq: None,
            max_seq_len: 131_072,
            compaction_threshold_percent: 70,
            compaction_min_tokens: 12_288,
            min_context_floor_applied: true,
            paged_attn: "1".to_string(),
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: Some(131_072),
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 1,
            max_seqs: 1,
            cuda_visible_devices: "0".to_string(),
            device_layers: None,
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: None,
            base_device_ordinal: Some(0),
            moe_experts_backend: None,
            disable_flash_attn: false,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            isq_singlethread: false,
            isq_cpu_threads: None,
            expected_tok_s: 12.5,
            hardware_fingerprint: "test-host".to_string(),
            theoretical_breakdown: runtime_plan::TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 20_000,
                kv_budget_cap_mb: 8_000,
                kv_budget_fraction_milli: 1000,
                weight_residency_mb: 7_000,
                kv_cache_mb: 6_000,
                fixed_runtime_base_overhead_mb: 512,
                backend_runtime_overhead_mb: 256,
                activation_overhead_mb: 512,
                load_peak_overhead_mb: 512,
                safety_headroom_mb: 512,
                required_effective_total_budget_mb: 15_000,
                required_total_mb: 16_000,
            },
            rationale: vec!["test previous default root".to_string()],
            gpu_allocations: vec![runtime_plan::PlannedGpuAllocation {
                gpu_index: 0,
                name: "GPU0".to_string(),
                total_mb: 24_576,
                desktop_reserve_mb: 1_024,
                aux_reserve_mb: 0,
                chat_budget_mb: 16_000,
                backend_overhead_mb: 512,
                activation_overhead_mb: 512,
                load_peak_overhead_mb: 512,
                repeating_weight_mb: 0,
                weight_mb: 7_000,
                kv_cache_mb: 6_000,
                free_headroom_mb: 1_000,
                chat_enabled: true,
            }],
        };
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert("CTOX_CHAT_MODEL".to_string(), previous_plan.model.clone());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            previous_plan.model.clone(),
        );
        env_map.insert("CTOX_ACTIVE_MODEL".to_string(), previous_plan.model.clone());
        env_map.insert("CTOX_CHAT_LOCAL_PRESET".to_string(), "Quality".to_string());
        runtime_plan::apply_chat_runtime_plan_env(&root, &previous_plan, &mut env_map).unwrap();
        runtime_plan::store_persisted_chat_runtime_plan(&root, Some(&previous_plan)).unwrap();
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        let change = apply_runtime_selection(&root, "google/gemma-4-E2B-it", Some("quality"))
            .expect("explicit Gemma runtime switch should override stale default local model");

        assert_eq!(
            change.next_state.active_model.as_deref(),
            Some("google/gemma-4-E2B-it")
        );
        assert_eq!(
            runtime_plan::load_persisted_chat_runtime_plan(&root)
                .unwrap()
                .expect("persisted plan")
                .model,
            "google/gemma-4-E2B-it"
        );
        let persisted_env = runtime_env::load_runtime_env_map(&root).unwrap();
        assert_eq!(
            persisted_env.get("CTOX_CHAT_MODEL").map(String::as_str),
            Some("google/gemma-4-E2B-it")
        );
        assert_eq!(
            persisted_env.get("CTOX_ACTIVE_MODEL").map(String::as_str),
            Some("google/gemma-4-E2B-it")
        );
    }

    #[test]
    fn api_runtime_selection_clears_local_runtime_projection() {
        let root = make_temp_root();
        let previous_plan = runtime_plan::ChatRuntimePlan {
            model: "openai/gpt-oss-20b".to_string(),
            preset: runtime_plan::ChatPreset::Quality,
            quantization: "mq4".to_string(),
            runtime_isq: None,
            max_seq_len: 131_072,
            compaction_threshold_percent: 70,
            compaction_min_tokens: 12_288,
            min_context_floor_applied: true,
            paged_attn: "1".to_string(),
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: Some(131_072),
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 1,
            max_seqs: 1,
            cuda_visible_devices: "0".to_string(),
            device_layers: None,
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: None,
            base_device_ordinal: Some(0),
            moe_experts_backend: None,
            disable_flash_attn: false,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            isq_singlethread: false,
            isq_cpu_threads: None,
            expected_tok_s: 12.5,
            hardware_fingerprint: "test-host".to_string(),
            theoretical_breakdown: runtime_plan::TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 20_000,
                kv_budget_cap_mb: 8_000,
                kv_budget_fraction_milli: 1000,
                weight_residency_mb: 7_000,
                kv_cache_mb: 6_000,
                fixed_runtime_base_overhead_mb: 512,
                backend_runtime_overhead_mb: 256,
                activation_overhead_mb: 512,
                load_peak_overhead_mb: 512,
                safety_headroom_mb: 512,
                required_effective_total_budget_mb: 15_000,
                required_total_mb: 16_000,
            },
            rationale: vec!["test api reset".to_string()],
            gpu_allocations: vec![runtime_plan::PlannedGpuAllocation {
                gpu_index: 0,
                name: "GPU0".to_string(),
                total_mb: 24_576,
                desktop_reserve_mb: 1_024,
                aux_reserve_mb: 0,
                chat_budget_mb: 16_000,
                backend_overhead_mb: 512,
                activation_overhead_mb: 512,
                load_peak_overhead_mb: 512,
                repeating_weight_mb: 0,
                weight_mb: 7_000,
                kv_cache_mb: 6_000,
                free_headroom_mb: 1_000,
                chat_enabled: true,
            }],
        };
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert("CTOX_CHAT_MODEL".to_string(), previous_plan.model.clone());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            previous_plan.model.clone(),
        );
        env_map.insert("CTOX_ACTIVE_MODEL".to_string(), previous_plan.model.clone());
        env_map.insert(
            "CTOX_UPSTREAM_BASE_URL".to_string(),
            "http://127.0.0.1:1234".to_string(),
        );
        runtime_plan::apply_chat_runtime_plan_env(&root, &previous_plan, &mut env_map).unwrap();
        runtime_plan::store_persisted_chat_runtime_plan(&root, Some(&previous_plan)).unwrap();
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        let change = apply_runtime_selection(&root, "gpt-5.4", None).unwrap();
        assert_eq!(
            change.next_state.source,
            runtime_state::InferenceSource::Api
        );
        assert_eq!(
            change.next_state.upstream_base_url,
            runtime_state::default_api_upstream_base_url()
        );

        let persisted = runtime_env::load_runtime_env_map(&root).unwrap();
        assert_eq!(
            persisted.get("CTOX_UPSTREAM_BASE_URL").map(String::as_str),
            Some(runtime_state::default_api_upstream_base_url())
        );
        assert!(!persisted.contains_key("CTOX_ENGINE_MODEL"));
        assert!(!persisted.contains_key("CTOX_CHAT_RUNTIME_PLAN_ACTIVE"));
        assert!(runtime_plan::load_persisted_chat_runtime_plan(&root)
            .unwrap()
            .is_none());
        assert!(runtime_plan::load_persisted_runtime_fleet_plan(&root)
            .unwrap()
            .is_none());
    }

    #[cfg(unix)]
    #[test]
    fn local_runtime_health_accepts_managed_socket_without_http_health() {
        let root = make_temp_root();
        let socket_path = runtime_kernel::managed_runtime_socket_path(
            &root,
            runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
        );
        fs::create_dir_all(socket_path.parent().expect("socket parent")).unwrap();
        let listener = UnixListener::bind(&socket_path).unwrap();

        let state = runtime_state::InferenceRuntimeState {
            version: 5,
            source: runtime_state::InferenceSource::Local,
            local_runtime: runtime_state::LocalRuntimeKind::Candle,
            base_model: Some("openai/gpt-oss-20b".to_string()),
            requested_model: Some("openai/gpt-oss-20b".to_string()),
            active_model: Some("openai/gpt-oss-20b".to_string()),
            engine_model: Some("openai/gpt-oss-20b".to_string()),
            engine_port: Some(1234),
            realized_context_tokens: Some(131_072),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: 12434,
            upstream_base_url: "http://127.0.0.1:9".to_string(),
            local_preset: Some("Quality".to_string()),
            boost: runtime_state::BoostRuntimeState::default(),
            adapter_tuning: runtime_state::AdapterRuntimeTuning::default(),
            embedding: runtime_state::AuxiliaryRuntimeState::default(),
            transcription: runtime_state::AuxiliaryRuntimeState::default(),
            speech: runtime_state::AuxiliaryRuntimeState::default(),
            vision: runtime_state::AuxiliaryRuntimeState::default(),
        };

        assert!(runtime_state_is_healthy(&root, &state));

        drop(listener);
    }

    #[cfg(unix)]
    #[test]
    fn cutover_ready_local_switch_commits_immediately_when_backend_is_ready() {
        let root = make_temp_root();
        let socket_path = runtime_kernel::managed_runtime_socket_path(
            &root,
            runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
        );
        fs::create_dir_all(socket_path.parent().expect("socket parent")).unwrap();
        let listener = UnixListener::bind(&socket_path).unwrap();

        persist_runtime_switch_transaction(
            &root,
            &RuntimeSwitchTransaction {
                version: 3,
                phase: RuntimeSwitchPhase::CutoverReady,
                requested_model: "Qwen/Qwen3.5-35B-A3B".to_string(),
                requested_source: runtime_state::InferenceSource::Local,
                requested_local_runtime: runtime_state::LocalRuntimeKind::Candle,
                requested_preset: Some("Performance".to_string()),
                previous_source: Some(runtime_state::InferenceSource::Local),
                previous_local_runtime: Some(runtime_state::LocalRuntimeKind::Candle),
                previous_requested_model: Some("openai/gpt-oss-20b".to_string()),
                previous_active_model: Some("openai/gpt-oss-20b".to_string()),
                previous_preset: Some("Quality".to_string()),
                previous_plan: None,
                next_active_model: Some("Qwen/Qwen3.5-35B-A3B".to_string()),
                started_at_epoch_secs: runtime_contract::current_epoch_secs(),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
                error: None,
            },
        )
        .unwrap();

        runtime_contract::sync_backend_runtime_residency(
            &root,
            runtime_contract::BackendRuntimeResidency {
                role: runtime_contract::BackendRole::Chat,
                phase: runtime_contract::RuntimeResidencyPhase::Active,
                model: "Qwen/Qwen3.5-35B-A3B".to_string(),
                pid: Some(std::process::id()),
                port: Some(1234),
                health_path: Some("/health".to_string()),
                launcher_kind: Some("engine".to_string()),
                compute_target: None,
                visible_devices: vec![0, 1, 2, 3],
                reserved_mb_by_gpu: BTreeMap::new(),
                updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
            },
        )
        .unwrap();

        let state = runtime_state::InferenceRuntimeState {
            version: 5,
            source: runtime_state::InferenceSource::Local,
            local_runtime: runtime_state::LocalRuntimeKind::Candle,
            base_model: Some("Qwen/Qwen3.5-35B-A3B".to_string()),
            requested_model: Some("Qwen/Qwen3.5-35B-A3B".to_string()),
            active_model: Some("Qwen/Qwen3.5-35B-A3B".to_string()),
            engine_model: Some("Qwen/Qwen3.5-35B-A3B".to_string()),
            engine_port: Some(1234),
            realized_context_tokens: Some(65_536),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: 12434,
            upstream_base_url: "http://127.0.0.1:9".to_string(),
            local_preset: Some("Performance".to_string()),
            boost: runtime_state::BoostRuntimeState::default(),
            adapter_tuning: runtime_state::AdapterRuntimeTuning::default(),
            embedding: runtime_state::AuxiliaryRuntimeState::default(),
            transcription: runtime_state::AuxiliaryRuntimeState::default(),
            speech: runtime_state::AuxiliaryRuntimeState::default(),
            vision: runtime_state::AuxiliaryRuntimeState::default(),
        };

        let phase = commit_runtime_switch_if_ready(&root, &state).unwrap();
        assert_eq!(phase, Some(RuntimeSwitchPhase::Committed));
        let persisted = load_runtime_switch_transaction(&root)
            .unwrap()
            .expect("persisted transaction");
        assert_eq!(persisted.phase, RuntimeSwitchPhase::Committed);

        drop(listener);
    }
}
