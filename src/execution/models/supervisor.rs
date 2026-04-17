use anyhow::Context;
use anyhow::Result;
#[cfg(unix)]
use libc::getrlimit;
#[cfg(unix)]
use libc::rlimit;
#[cfg(unix)]
use libc::setrlimit;
#[cfg(unix)]
use libc::signal;
#[cfg(unix)]
use libc::RLIMIT_NOFILE;
#[cfg(unix)]
use libc::SIGHUP;
#[cfg(unix)]
use libc::SIGPIPE;
#[cfg(unix)]
use libc::SIG_IGN;
use sha2::Digest;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use crate::inference::engine;
use crate::inference::model_registry;
use crate::inference::runtime_contract;
use crate::inference::runtime_control;
use crate::inference::runtime_engine_guard;
use crate::inference::runtime_env;
use crate::inference::runtime_gpu_manager;
use crate::inference::runtime_kernel;
use crate::inference::runtime_plan;
use crate::inference::runtime_state;
const SUPERVISOR_POLL_SECS: u64 = 12;
const PERSISTENT_BACKEND_SHUTDOWN_TIMEOUT_SECS: u64 = 15;
const PERSISTENT_BACKEND_SHUTDOWN_POLL_MILLIS: u64 = 150;
const RUNTIME_SWITCH_COMMAND_FRAGMENT: &str = "runtime switch";
const QUANT_ARTIFACT_BUILD_LOCKS_RELATIVE_DIR: &str = "runtime/uqff_cache_locks";
const CHAT_QUANT_ARTIFACT_PENDING_SUFFIX: &str = ".pending";
const MANAGED_ENGINE_FROM_CONFIG_COMMAND: &str =
    "tools/model-runtime/target/release/ctox-engine from-config";
const MANAGED_ENGINE_QUANTIZE_COMMAND: &str =
    "tools/model-runtime/target/release/ctox-engine quantize";
const MANAGED_LITERT_SERVE_COMMAND_FRAGMENT: &str = "serve-litert-bridge";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagedLauncherKind {
    Engine,
    LiteRt,
}

impl ManagedLauncherKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::LiteRt => "litert",
        }
    }
}

#[derive(Debug, Clone)]
struct ManagedBackendSpec {
    display_model: String,
    request_model: String,
    port: u16,
    socket_path: Option<String>,
    health_path: &'static str,
    launcher_kind: ManagedLauncherKind,
    compute_target: Option<engine::ComputeTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedBackendLaunchSpec {
    version: u32,
    role: String,
    display_model: String,
    request_model: String,
    port: u16,
    socket_path: Option<String>,
    health_path: String,
    launcher_kind: String,
    compute_target: Option<String>,
    visible_devices: Option<String>,
    engine_config: ManagedEngineLaunchConfig,
    litert_config: Option<ManagedLiteRtLaunchConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ManagedEngineLaunchConfig {
    binary_path: Option<String>,
    log_path: Option<String>,
    arch: Option<String>,
    isq: Option<String>,
    isq_organization: Option<String>,
    paged_attn: Option<String>,
    pa_cache_type: Option<String>,
    pa_memory_fraction: Option<String>,
    pa_context_len: Option<u32>,
    tensor_parallel_backend: Option<String>,
    mn_local_world_size: Option<u32>,
    max_batch_size: Option<u32>,
    max_seqs: Option<u32>,
    max_seq_len: Option<u32>,
    device_layers: Option<String>,
    topology: Option<String>,
    allow_device_layers_with_topology: bool,
    nm_device_ordinal: Option<u32>,
    base_device_ordinal: Option<u32>,
    moe_experts_backend: Option<String>,
    from_uqff: Option<String>,
    write_uqff: Option<String>,
    disable_nccl: bool,
    disable_flash_attn: bool,
    no_mmap: bool,
    language_model_only: bool,
    isq_singlethread: bool,
    isq_cpu_threads: Option<u32>,
    parallel_immediate_isq: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ManagedLiteRtLaunchConfig {
    bridge_binary_path: Option<String>,
    cli_path: Option<String>,
    log_path: Option<String>,
    backend: String,
    context_tokens: u32,
    validated_context_tokens: u32,
    model_reference: String,
    model_file: Option<String>,
    huggingface_repo: Option<String>,
    huggingface_token: Option<String>,
    speculative_decoding: String,
    verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LiteRtArtifactSpec {
    huggingface_repo: &'static str,
    model_file: &'static str,
    validated_context_tokens: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagedBackendRole {
    Chat,
    Embedding,
    Stt,
    Tts,
    Vision,
}

const MANAGED_BACKEND_ROLES: [ManagedBackendRole; 5] = [
    ManagedBackendRole::Chat,
    ManagedBackendRole::Embedding,
    ManagedBackendRole::Stt,
    ManagedBackendRole::Tts,
    ManagedBackendRole::Vision,
];

impl ManagedBackendRole {
    fn as_env_value(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::Stt => "stt",
            Self::Tts => "tts",
            Self::Vision => "vision",
        }
    }

    fn pid_file_name(self) -> &'static str {
        match self {
            Self::Chat => "ctox_chat_backend.pid",
            Self::Embedding => "ctox_embedding_backend.pid",
            Self::Stt => "ctox_stt_backend.pid",
            Self::Tts => "ctox_tts_backend.pid",
            Self::Vision => "ctox_vision_backend.pid",
        }
    }

    fn log_file_name(self) -> &'static str {
        match self {
            Self::Chat => "ctox_chat_backend.log",
            Self::Embedding => "ctox_embedding_backend.log",
            Self::Stt => "ctox_stt_backend.log",
            Self::Tts => "ctox_tts_backend.log",
            Self::Vision => "ctox_vision_backend.log",
        }
    }

    fn is_auxiliary(self) -> bool {
        self != Self::Chat
    }

    fn spec(self, root: &Path) -> ManagedBackendSpec {
        if let Some(resolved) = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok() {
            if let Some(binding) = resolved_binding_for_role(&resolved, self) {
                return managed_spec_from_binding(binding);
            }
        }
        match self {
            Self::Chat => {
                let runtime_state = runtime_state::load_or_resolve_runtime_state(root).ok();
                let runtime = runtime_state
                    .as_ref()
                    .and_then(|state| {
                        state
                            .engine_model
                            .clone()
                            .or_else(|| state.active_model.clone())
                            .or_else(|| state.base_model.clone())
                    })
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| {
                        engine::default_runtime_config(engine::LocalModelFamily::GptOss).model
                    });
                let fallback_runtime =
                    engine::runtime_config_for_model(&runtime).unwrap_or_else(|_| {
                        engine::default_runtime_config(engine::LocalModelFamily::GptOss)
                    });
                let port = runtime_state
                    .as_ref()
                    .and_then(|state| state.engine_port)
                    .unwrap_or(fallback_runtime.port);
                ManagedBackendSpec {
                    display_model: runtime.clone(),
                    request_model: runtime,
                    port,
                    socket_path: Some(
                        runtime_kernel::managed_runtime_socket_path(
                            root,
                            runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
                        )
                        .display()
                        .to_string(),
                    ),
                    health_path: "/health",
                    launcher_kind: ManagedLauncherKind::Engine,
                    compute_target: None,
                }
            }
            Self::Embedding => {
                let runtime_state = runtime_state::load_or_resolve_runtime_state(root).ok();
                let auxiliary_state = runtime_state.as_ref().map(|state| {
                    runtime_state::auxiliary_runtime_state_for_role(
                        state,
                        engine::AuxiliaryRole::Embedding,
                    )
                });
                let configured_model =
                    auxiliary_state.and_then(|state| state.configured_model.clone());
                let selection = runtime_kernel::preferred_auxiliary_selection_for_host(
                    root,
                    engine::AuxiliaryRole::Embedding,
                    configured_model.as_deref(),
                );
                let port = auxiliary_state
                    .and_then(|state| state.port)
                    .unwrap_or(selection.default_port);
                ManagedBackendSpec {
                    display_model: selection.choice.to_string(),
                    request_model: selection.request_model.to_string(),
                    port,
                    socket_path: Some(
                        runtime_kernel::managed_runtime_socket_path(
                            root,
                            runtime_kernel::InferenceWorkloadRole::Embedding,
                        )
                        .display()
                        .to_string(),
                    ),
                    health_path: "/health",
                    launcher_kind: ManagedLauncherKind::Engine,
                    compute_target: Some(selection.compute_target),
                }
            }
            Self::Stt => {
                let runtime_state = runtime_state::load_or_resolve_runtime_state(root).ok();
                let auxiliary_state = runtime_state.as_ref().map(|state| {
                    runtime_state::auxiliary_runtime_state_for_role(
                        state,
                        engine::AuxiliaryRole::Stt,
                    )
                });
                let configured_model =
                    auxiliary_state.and_then(|state| state.configured_model.clone());
                let selection = runtime_kernel::preferred_auxiliary_selection_for_host(
                    root,
                    engine::AuxiliaryRole::Stt,
                    configured_model.as_deref(),
                );
                let port = auxiliary_state
                    .and_then(|state| state.port)
                    .unwrap_or(selection.default_port);
                ManagedBackendSpec {
                    display_model: selection.choice.to_string(),
                    request_model: selection.request_model.to_string(),
                    port,
                    socket_path: Some(
                        runtime_kernel::managed_runtime_socket_path(
                            root,
                            runtime_kernel::InferenceWorkloadRole::Transcription,
                        )
                        .display()
                        .to_string(),
                    ),
                    health_path: "/health",
                    launcher_kind: ManagedLauncherKind::Engine,
                    compute_target: Some(selection.compute_target),
                }
            }
            Self::Tts => {
                let runtime_state = runtime_state::load_or_resolve_runtime_state(root).ok();
                let auxiliary_state = runtime_state.as_ref().map(|state| {
                    runtime_state::auxiliary_runtime_state_for_role(
                        state,
                        engine::AuxiliaryRole::Tts,
                    )
                });
                let configured_model =
                    auxiliary_state.and_then(|state| state.configured_model.clone());
                let selection = runtime_kernel::preferred_auxiliary_selection_for_host(
                    root,
                    engine::AuxiliaryRole::Tts,
                    configured_model.as_deref(),
                );
                let port = auxiliary_state
                    .and_then(|state| state.port)
                    .unwrap_or(selection.default_port);
                ManagedBackendSpec {
                    display_model: selection.choice.to_string(),
                    request_model: selection.request_model.to_string(),
                    port,
                    socket_path: Some(
                        runtime_kernel::managed_runtime_socket_path(
                            root,
                            runtime_kernel::InferenceWorkloadRole::Speech,
                        )
                        .display()
                        .to_string(),
                    ),
                    health_path: "/health",
                    launcher_kind: ManagedLauncherKind::Engine,
                    compute_target: Some(selection.compute_target),
                }
            }
            Self::Vision => {
                let runtime_state = runtime_state::load_or_resolve_runtime_state(root).ok();
                let auxiliary_state = runtime_state.as_ref().map(|state| {
                    runtime_state::auxiliary_runtime_state_for_role(
                        state,
                        engine::AuxiliaryRole::Vision,
                    )
                });
                let configured_model =
                    auxiliary_state.and_then(|state| state.configured_model.clone());
                let selection = runtime_kernel::preferred_auxiliary_selection_for_host(
                    root,
                    engine::AuxiliaryRole::Vision,
                    configured_model.as_deref(),
                );
                let port = auxiliary_state
                    .and_then(|state| state.port)
                    .unwrap_or(selection.default_port);
                ManagedBackendSpec {
                    display_model: selection.choice.to_string(),
                    request_model: selection.request_model.to_string(),
                    port,
                    socket_path: Some(
                        runtime_kernel::managed_runtime_socket_path(
                            root,
                            runtime_kernel::InferenceWorkloadRole::Vision,
                        )
                        .display()
                        .to_string(),
                    ),
                    health_path: "/health",
                    launcher_kind: ManagedLauncherKind::Engine,
                    compute_target: Some(selection.compute_target),
                }
            }
        }
    }
}

fn resolved_binding_for_role<'a>(
    resolved: &'a runtime_kernel::InferenceRuntimeKernel,
    role: ManagedBackendRole,
) -> Option<&'a runtime_kernel::ResolvedRuntimeBinding> {
    match role {
        ManagedBackendRole::Chat => resolved.primary_generation.as_ref(),
        ManagedBackendRole::Embedding => resolved.embedding.as_ref(),
        ManagedBackendRole::Stt => resolved.transcription.as_ref(),
        ManagedBackendRole::Tts => resolved.speech.as_ref(),
        ManagedBackendRole::Vision => resolved.vision.as_ref(),
    }
}

fn managed_spec_from_binding(
    binding: &runtime_kernel::ResolvedRuntimeBinding,
) -> ManagedBackendSpec {
    ManagedBackendSpec {
        display_model: binding.display_model.clone(),
        request_model: binding.request_model.clone(),
        port: binding.port,
        socket_path: binding.socket_path.clone(),
        health_path: binding.health_path,
        launcher_kind: match binding.launcher_kind {
            runtime_kernel::RuntimeLauncherKind::Engine => ManagedLauncherKind::Engine,
            runtime_kernel::RuntimeLauncherKind::LiteRt => ManagedLauncherKind::LiteRt,
        },
        compute_target: binding.compute_target,
    }
}

pub fn start_backend_supervisor(root: PathBuf) {
    thread::spawn(move || loop {
        if let Err(err) = ensure_persistent_backends(&root) {
            eprintln!("ctox backend supervisor error: {err:#}");
        }
        thread::sleep(Duration::from_secs(SUPERVISOR_POLL_SECS));
    });
}

pub fn ensure_persistent_backends(root: &Path) -> Result<()> {
    for role in MANAGED_BACKEND_ROLES {
        if !managed_backend_enabled(root, role) {
            let _ = stop_process(root, backend_pid_path(root, role));
            let _ = stop_processes_on_port(root, role.spec(root).port);
            release_backend_runtime_ownership(root, role);
            continue;
        }
        if let Err(err) = ensure_backend_process(root, role, false) {
            if role.is_auxiliary() {
                eprintln!(
                    "ctox auxiliary backend {} unavailable; continuing without it: {err:#}",
                    role.as_env_value()
                );
                continue;
            }
            return Err(err);
        }
    }
    let _ = runtime_control::reconcile_runtime_switch_transaction(root);
    Ok(())
}

pub fn ensure_boundary_proxy_process(root: &Path) -> Result<()> {
    ensure_proxy_process(root)
}

pub fn restart_boundary_proxy_process(root: &Path) -> Result<()> {
    stop_process(root, proxy_pid_path(root))?;
    release_proxy_runtime_ownership(root);
    ensure_proxy_process(root)
}

pub fn ensure_chat_backend_ready(root: &Path, force_restart: bool) -> Result<()> {
    ensure_backend_process(root, ManagedBackendRole::Chat, force_restart)
}

pub fn ensure_auxiliary_backends_best_effort(root: PathBuf) {
    thread::spawn(move || {
        for role in [
            ManagedBackendRole::Embedding,
            ManagedBackendRole::Stt,
            ManagedBackendRole::Tts,
            ManagedBackendRole::Vision,
        ] {
            if !managed_backend_enabled(&root, role) {
                release_backend_runtime_ownership(&root, role);
                continue;
            }
            if let Err(err) = ensure_backend_process(&root, role, false) {
                eprintln!(
                    "ctox auxiliary backend {} unavailable after switch; continuing without it: {err:#}",
                    role.as_env_value()
                );
            }
        }
    });
}

pub fn release_chat_backend(root: &Path, port: Option<u16>) -> Result<()> {
    if let Some(port) = port {
        stop_processes_on_port(root, port)?;
    }
    stop_process(root, backend_pid_path(root, ManagedBackendRole::Chat))?;
    release_backend_runtime_ownership(root, ManagedBackendRole::Chat);
    let _ = clear_backend_startup_locks(root);
    let _ = clear_managed_socket_file(runtime_kernel::managed_runtime_socket_path(
        root,
        runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
    ));
    wait_for_managed_runtime_fleet_idle(
        root,
        Duration::from_secs(PERSISTENT_BACKEND_SHUTDOWN_TIMEOUT_SECS),
    )
}

pub fn release_managed_runtime_fleet(root: &Path) -> Result<()> {
    let ownership = runtime_contract::load_runtime_ownership_state(root).unwrap_or_default();
    for workload in ownership.workloads {
        if let Some(port) = workload.port {
            let _ = stop_processes_on_port(root, port);
        }
    }
    for role in MANAGED_BACKEND_ROLES {
        let _ = stop_processes_on_port(root, role.spec(root).port);
        let _ = stop_process(root, backend_pid_path(root, role));
        release_backend_runtime_ownership(root, role);
    }
    let _ = kill_workspace_managed_runtime_groups(root);
    clear_managed_backend_pid_files(root);
    clear_managed_runtime_socket_files(root);
    let _ = clear_backend_startup_locks(root);
    wait_for_managed_runtime_fleet_idle(
        root,
        Duration::from_secs(PERSISTENT_BACKEND_SHUTDOWN_TIMEOUT_SECS),
    )
}

pub fn ensure_auxiliary_backend_ready(
    root: &Path,
    role: engine::AuxiliaryRole,
    force_restart: bool,
) -> Result<()> {
    let role = match role {
        engine::AuxiliaryRole::Embedding => ManagedBackendRole::Embedding,
        engine::AuxiliaryRole::Stt => ManagedBackendRole::Stt,
        engine::AuxiliaryRole::Tts => ManagedBackendRole::Tts,
        engine::AuxiliaryRole::Vision => ManagedBackendRole::Vision,
    };
    ensure_backend_process(root, role, force_restart)
}

pub fn ensure_auxiliary_backend_launchable(root: &Path, role: engine::AuxiliaryRole) -> Result<()> {
    let resolved_runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok();
    let has_nonlocal_binding = resolved_runtime
        .as_ref()
        .and_then(|runtime| runtime.binding_for_auxiliary_role(role))
        .is_some_and(|binding| {
            let base_url = binding.base_url.trim().to_ascii_lowercase();
            !base_url.is_empty()
                && !base_url.starts_with("http://127.0.0.1:")
                && !base_url.starts_with("http://localhost:")
        });
    if has_nonlocal_binding {
        return Ok(());
    }

    let binary = engine::discover_source_layout_paths(root).model_runtime_binary;
    if binary.is_file() {
        return Ok(());
    }

    let role_label = match role {
        engine::AuxiliaryRole::Embedding => "embedding",
        engine::AuxiliaryRole::Stt => "stt",
        engine::AuxiliaryRole::Tts => "tts",
        engine::AuxiliaryRole::Vision => "vision",
    };
    anyhow::bail!(
        "{role_label} backend requires ctox-engine at {}. Run the explicit engine rebuild workflow first or configure an external {role_label} runtime.",
        binary.display()
    );
}

fn managed_backend_enabled(root: &Path, role: ManagedBackendRole) -> bool {
    if let Some(resolved) = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok() {
        return match role {
            ManagedBackendRole::Chat => resolved.primary_generation.is_some(),
            ManagedBackendRole::Embedding => resolved.embedding.is_some(),
            ManagedBackendRole::Stt => resolved.transcription.is_some(),
            ManagedBackendRole::Tts => resolved.speech.is_some(),
            ManagedBackendRole::Vision => resolved.vision.is_some(),
        };
    }
    match role {
        ManagedBackendRole::Chat => runtime_state::load_or_resolve_runtime_state(root)
            .map(|state| state.source.is_local())
            .unwrap_or_else(|_| {
                runtime_env::env_or_config(root, "CTOX_CHAT_SOURCE")
                    .map(|value| value.trim().eq_ignore_ascii_case("local"))
                    .unwrap_or(true)
            }),
        ManagedBackendRole::Embedding => runtime_state::load_or_resolve_runtime_state(root)
            .map(|state| state.embedding.enabled)
            .unwrap_or_else(|_| runtime_env::auxiliary_backend_enabled(root, "EMBEDDING")),
        ManagedBackendRole::Stt => runtime_state::load_or_resolve_runtime_state(root)
            .map(|state| state.transcription.enabled)
            .unwrap_or_else(|_| runtime_env::auxiliary_backend_enabled(root, "STT")),
        ManagedBackendRole::Tts => runtime_state::load_or_resolve_runtime_state(root)
            .map(|state| state.speech.enabled)
            .unwrap_or_else(|_| runtime_env::auxiliary_backend_enabled(root, "TTS")),
        ManagedBackendRole::Vision => runtime_state::load_or_resolve_runtime_state(root)
            .map(|state| state.vision.enabled)
            .unwrap_or_else(|_| runtime_env::auxiliary_backend_enabled(root, "VISION")),
    }
}

fn backend_contract_role(role: ManagedBackendRole) -> runtime_contract::BackendRole {
    match role {
        ManagedBackendRole::Chat => runtime_contract::BackendRole::Chat,
        ManagedBackendRole::Embedding => runtime_contract::BackendRole::Embedding,
        ManagedBackendRole::Stt => runtime_contract::BackendRole::Stt,
        ManagedBackendRole::Tts => runtime_contract::BackendRole::Tts,
        ManagedBackendRole::Vision => runtime_contract::BackendRole::Vision,
    }
}

fn release_proxy_runtime_ownership(root: &Path) {
    let _ = runtime_contract::release_proxy_runtime_residency(root);
}

fn release_backend_runtime_ownership(root: &Path, role: ManagedBackendRole) {
    let _ = runtime_contract::release_backend_runtime_residency(root, backend_contract_role(role));
}

fn sync_proxy_runtime_ownership(
    root: &Path,
    host: &str,
    port: u16,
    pid: Option<u32>,
    phase: runtime_contract::RuntimeResidencyPhase,
) -> Result<()> {
    runtime_contract::sync_proxy_runtime_residency(
        root,
        runtime_contract::ProxyRuntimeResidency {
            phase,
            pid,
            host: host.to_string(),
            port,
            health_path: "/ctox/telemetry".to_string(),
            updated_at_epoch_secs: runtime_contract::current_epoch_secs(),
        },
    )
}

fn backend_runtime_descriptor(
    role: ManagedBackendRole,
    spec: &ManagedBackendSpec,
) -> runtime_gpu_manager::RuntimeWorkloadDescriptor {
    runtime_gpu_manager::RuntimeWorkloadDescriptor {
        role: backend_contract_role(role),
        model: spec.request_model.clone(),
        port: spec.port,
        health_path: spec.health_path.to_string(),
        launcher_kind: runtime_kernel::RuntimeLauncherKind::Engine,
        compute_target: spec.compute_target,
    }
}

pub fn shutdown_persistent_backends(root: &Path) -> Result<()> {
    let mut failures = Vec::new();

    if let Err(err) = stop_process(root, proxy_pid_path(root)) {
        failures.push(format!("proxy stop: {err}"));
    }
    release_proxy_runtime_ownership(root);
    for role in MANAGED_BACKEND_ROLES {
        if let Err(err) = stop_process(root, backend_pid_path(root, role)) {
            failures.push(format!("{} stop: {err}", role.as_env_value()));
        }
        release_backend_runtime_ownership(root, role);
    }
    for port in managed_runtime_ports(root)? {
        if let Err(err) = stop_processes_on_port(root, port) {
            failures.push(format!("tcp/{port} stop: {err}"));
        }
    }
    if let Err(err) = kill_workspace_managed_runtime_groups(root) {
        failures.push(format!("workspace residue kill: {err}"));
    }
    clear_managed_pid_files(root);
    clear_managed_socket_files(root);
    if let Err(err) = clear_backend_startup_locks(root) {
        failures.push(format!("startup lock cleanup: {err}"));
    }
    release_proxy_runtime_ownership(root);
    for role in MANAGED_BACKEND_ROLES {
        release_backend_runtime_ownership(root, role);
    }
    if wait_for_persistent_backends_idle(
        root,
        Duration::from_secs(PERSISTENT_BACKEND_SHUTDOWN_TIMEOUT_SECS),
    )? {
        return Ok(());
    }
    let mut residue = persistent_backend_residue(root)?;
    failures.append(&mut residue);
    anyhow::bail!(
        "persistent backends failed to stop cleanly: {}",
        failures.join("; ")
    );
}

pub fn persistent_backends_idle(root: &Path) -> Result<bool> {
    Ok(persistent_backend_residue(root)?.is_empty())
}

pub fn persistent_backend_alerts(root: &Path) -> Result<Vec<String>> {
    persistent_backend_residue(root)
}

fn persistent_backend_residue(root: &Path) -> Result<Vec<String>> {
    persistent_backend_residue_with_scope(root, true)
}

fn managed_runtime_fleet_residue(root: &Path) -> Result<Vec<String>> {
    persistent_backend_residue_with_scope(root, false)
}

fn persistent_backend_residue_with_scope(root: &Path, include_proxy: bool) -> Result<Vec<String>> {
    let mut residue = Vec::new();
    let pid_paths = if include_proxy {
        managed_pid_paths(root)
    } else {
        managed_backend_pid_paths(root)
    };
    for pid_path in pid_paths {
        if let Some(pid) = read_pid(&pid_path) {
            if process_is_alive(pid) {
                residue.push(format!("alive pid {pid} at {}", pid_path.display()));
            } else {
                residue.push(format!("stale pid file {}", pid_path.display()));
            }
        }
    }
    for socket_path in managed_socket_paths(root) {
        if socket_path.exists() && !socket_listener_accepts(&socket_path) {
            residue.push(format!("stale socket {}", socket_path.display()));
        }
    }
    let ports = if include_proxy {
        managed_runtime_ports(root)?
    } else {
        managed_backend_runtime_ports(root)?
    };
    for port in ports {
        let listeners = listening_pids_for_port(root, port)?;
        if !listeners.is_empty() {
            residue.push(format!("tcp/{port} listeners {listeners:?}"));
        }
    }
    for path in backend_startup_lock_paths(root)? {
        residue.push(format!("startup lock {}", path.display()));
    }
    let managed_groups = workspace_managed_runtime_groups(root)?;
    if !managed_groups.is_empty() {
        residue.push(format!("managed runtime groups {managed_groups:?}"));
    }
    let managed_pids = workspace_managed_runtime_pids(root)?;
    if !managed_pids.is_empty() {
        residue.push(format!("managed runtime pids {managed_pids:?}"));
    }
    Ok(residue)
}

fn wait_for_persistent_backends_idle(root: &Path, timeout: Duration) -> Result<bool> {
    wait_for_runtime_residue_cleared(root, timeout, persistent_backends_idle)
}

fn wait_for_managed_runtime_fleet_idle(root: &Path, timeout: Duration) -> Result<()> {
    if wait_for_runtime_residue_cleared(root, timeout, managed_runtime_fleet_idle)? {
        return Ok(());
    }
    let residue = managed_runtime_fleet_residue(root)?;
    anyhow::bail!(
        "managed runtime fleet failed to stop cleanly: {}",
        residue.join("; ")
    );
}

fn wait_for_runtime_residue_cleared<F>(root: &Path, timeout: Duration, mut check: F) -> Result<bool>
where
    F: FnMut(&Path) -> Result<bool>,
{
    let deadline = Instant::now() + timeout;
    loop {
        if check(root)? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(
            PERSISTENT_BACKEND_SHUTDOWN_POLL_MILLIS,
        ));
    }
}

fn managed_runtime_fleet_idle(root: &Path) -> Result<bool> {
    Ok(managed_runtime_fleet_residue(root)?.is_empty())
}

fn managed_pid_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![proxy_pid_path(root)];
    for role in MANAGED_BACKEND_ROLES {
        paths.push(backend_pid_path(root, role));
    }
    paths
}

fn clear_managed_pid_files(root: &Path) {
    for path in managed_pid_paths(root) {
        let _ = std::fs::remove_file(path);
    }
}

fn managed_backend_pid_paths(root: &Path) -> Vec<PathBuf> {
    MANAGED_BACKEND_ROLES
        .iter()
        .copied()
        .map(|role| backend_pid_path(root, role))
        .collect()
}

fn clear_managed_backend_pid_files(root: &Path) {
    for path in managed_backend_pid_paths(root) {
        let _ = std::fs::remove_file(path);
    }
}

fn managed_socket_paths(root: &Path) -> Vec<PathBuf> {
    vec![
        runtime_kernel::managed_runtime_socket_path(
            root,
            runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
        ),
        runtime_kernel::managed_runtime_socket_path(
            root,
            runtime_kernel::InferenceWorkloadRole::Embedding,
        ),
        runtime_kernel::managed_runtime_socket_path(
            root,
            runtime_kernel::InferenceWorkloadRole::Transcription,
        ),
        runtime_kernel::managed_runtime_socket_path(
            root,
            runtime_kernel::InferenceWorkloadRole::Speech,
        ),
    ]
}

fn clear_managed_socket_files(root: &Path) {
    for path in managed_socket_paths(root) {
        let _ = std::fs::remove_file(path);
    }
}

fn clear_managed_runtime_socket_files(root: &Path) {
    for path in managed_socket_paths(root) {
        let _ = clear_managed_socket_file(path);
    }
}

fn clear_managed_socket_file(path: PathBuf) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(&path)
        .with_context(|| format!("failed to remove managed socket {}", path.display()))
}

fn backend_startup_lock_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let runtime_dir = root.join("runtime");
    if !runtime_dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&runtime_dir)
        .with_context(|| format!("failed to read runtime dir {}", runtime_dir.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to inspect runtime entry in {}",
                runtime_dir.display()
            )
        })?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if file_name.starts_with("backend_startup_") && file_name.ends_with(".lock") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn clear_backend_startup_locks(root: &Path) -> Result<()> {
    for path in backend_startup_lock_paths(root)? {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

fn managed_runtime_ports(root: &Path) -> Result<Vec<u16>> {
    let mut ports = Vec::new();
    if let Some(proxy_port) = managed_proxy_port(root) {
        push_unique_port(&mut ports, proxy_port);
    }
    append_managed_backend_runtime_ports(root, &mut ports)?;
    if let Ok(ownership) = runtime_contract::load_runtime_ownership_state(root) {
        if let Some(proxy) = ownership.proxy {
            push_unique_port(&mut ports, proxy.port);
        }
    }
    ports.sort_unstable();
    ports.dedup();
    Ok(ports)
}

fn managed_backend_runtime_ports(root: &Path) -> Result<Vec<u16>> {
    let mut ports = Vec::new();
    append_managed_backend_runtime_ports(root, &mut ports)?;
    ports.sort_unstable();
    ports.dedup();
    Ok(ports)
}

fn append_managed_backend_runtime_ports(root: &Path, ports: &mut Vec<u16>) -> Result<()> {
    if let Some(state) = runtime_state::load_or_resolve_runtime_state(root).ok() {
        if let Some(port) = state.engine_port {
            push_unique_port(ports, port);
        }
        let embedding_selection = engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Embedding,
            state.embedding.configured_model.as_deref(),
        );
        let stt_selection = engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Stt,
            state.transcription.configured_model.as_deref(),
        );
        let tts_selection = engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Tts,
            state.speech.configured_model.as_deref(),
        );
        push_unique_port(
            ports,
            state
                .embedding
                .port
                .unwrap_or(embedding_selection.default_port),
        );
        push_unique_port(
            ports,
            state
                .transcription
                .port
                .unwrap_or(stt_selection.default_port),
        );
        push_unique_port(
            ports,
            state.speech.port.unwrap_or(tts_selection.default_port),
        );
    } else {
        push_unique_port(ports, runtime_state::default_local_engine_port());
        push_unique_port(
            ports,
            engine::auxiliary_model_selection(engine::AuxiliaryRole::Embedding, None).default_port,
        );
        push_unique_port(
            ports,
            engine::auxiliary_model_selection(engine::AuxiliaryRole::Stt, None).default_port,
        );
        push_unique_port(
            ports,
            engine::auxiliary_model_selection(engine::AuxiliaryRole::Tts, None).default_port,
        );
    }
    if let Ok(ownership) = runtime_contract::load_runtime_ownership_state(root) {
        for workload in ownership.workloads {
            if let Some(port) = workload.port {
                push_unique_port(ports, port);
            }
        }
    }
    for profile in engine::supported_local_model_profiles() {
        push_unique_port(ports, profile.runtime.port);
    }
    for path in backend_startup_lock_paths(root)? {
        if let Some(port) = startup_lock_port(&path) {
            push_unique_port(ports, port);
        }
    }
    Ok(())
}

pub fn boundary_proxy_is_managed(root: &Path) -> bool {
    if read_pid(&proxy_pid_path(root))
        .filter(|pid| process_is_alive(*pid))
        .is_some()
    {
        return true;
    }
    runtime_contract::load_runtime_ownership_state(root)
        .ok()
        .and_then(|state| state.proxy)
        .is_some()
}

fn managed_proxy_port(root: &Path) -> Option<u16> {
    if !boundary_proxy_is_managed(root) {
        return None;
    }
    runtime_state::load_or_resolve_runtime_state(root)
        .ok()
        .map(|state| state.proxy_port)
        .or_else(|| Some(runtime_state::default_proxy_port()))
}

fn push_unique_port(ports: &mut Vec<u16>, port: u16) {
    if !ports.contains(&port) {
        ports.push(port);
    }
}

fn startup_lock_port(path: &Path) -> Option<u16> {
    let file_name = path.file_name()?.to_str()?;
    let suffix = file_name
        .strip_prefix("backend_startup_")?
        .strip_suffix(".lock")?;
    suffix.parse::<u16>().ok()
}

pub(crate) fn backend_startup_wait_secs_for_model(active_model: Option<&str>) -> u64 {
    active_model
        .and_then(model_registry::backend_startup_wait_secs)
        .unwrap_or(120)
}

fn backend_startup_wait_secs_for_spec(role: ManagedBackendRole, spec: &ManagedBackendSpec) -> u64 {
    let baseline = backend_startup_wait_secs_for_model(Some(&spec.request_model));
    if role != ManagedBackendRole::Chat && spec.compute_target == Some(engine::ComputeTarget::Cpu) {
        baseline.max(600)
    } else {
        baseline
    }
}

#[derive(Debug)]
pub(crate) struct BackendStartupLease {
    path: PathBuf,
}

impl Drop for BackendStartupLease {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Debug)]
struct QuantArtifactBuildLease {
    path: PathBuf,
}

impl Drop for QuantArtifactBuildLease {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn backend_startup_lock_path(root: &Path, port: u16) -> PathBuf {
    root.join("runtime")
        .join(format!("backend_startup_{port}.lock"))
}

fn quant_artifact_build_lock_path(root: &Path, artifact_path: &Path) -> PathBuf {
    let key = format!(
        "{:x}",
        sha2::Sha256::digest(artifact_path.display().to_string())
    );
    root.join(QUANT_ARTIFACT_BUILD_LOCKS_RELATIVE_DIR)
        .join(format!("{key}.lock"))
}

fn acquire_quant_artifact_build_lease(
    root: &Path,
    artifact_path: &Path,
) -> Result<Option<QuantArtifactBuildLease>> {
    let path = quant_artifact_build_lock_path(root, artifact_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create quant artifact lock dir {}",
                parent.display()
            )
        })?;
    }
    for _ in 0..2 {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut handle) => {
                writeln!(handle, "pid={}", std::process::id())?;
                writeln!(handle, "artifact={}", artifact_path.display())?;
                return Ok(Some(QuantArtifactBuildLease { path }));
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                if lock_file_is_stale(root, &path) {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                return Ok(None);
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "failed to create quant artifact build lock {}",
                        path.display()
                    )
                });
            }
        }
    }
    Ok(None)
}

fn managed_engine_process_command(command: &str) -> bool {
    command.contains(MANAGED_ENGINE_FROM_CONFIG_COMMAND)
}

fn managed_engine_quantize_command(command: &str) -> bool {
    command.contains(MANAGED_ENGINE_QUANTIZE_COMMAND)
}

fn managed_litert_process_command(command: &str) -> bool {
    command.contains(MANAGED_LITERT_SERVE_COMMAND_FRAGMENT)
}

fn lock_file_pid(path: &Path) -> Option<u32> {
    let raw = std::fs::read_to_string(path).ok()?;
    raw.lines().find_map(|line| {
        line.strip_prefix("pid=")
            .and_then(|value| value.trim().parse::<u32>().ok())
    })
}

fn lock_file_is_stale(root: &Path, path: &Path) -> bool {
    match lock_file_pid(path) {
        Some(pid) => {
            if pid == std::process::id() {
                return false;
            }
            if !process_is_alive(pid) {
                return true;
            }
            let expired = std::fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| modified.elapsed().ok())
                .map(|elapsed| elapsed > Duration::from_secs(15 * 60))
                .unwrap_or(false);
            if expired {
                return true;
            }
            let Some(command) = process_command(root, pid).ok().flatten() else {
                return true;
            };
            let root_display = root.display().to_string();
            let owned_by_ctox = command.contains(&root_display)
                && ((command.contains("ctox")
                    && (command.contains(RUNTIME_SWITCH_COMMAND_FRAGMENT)
                        || command.contains("service --foreground")))
                    || managed_engine_process_command(&command)
                    || managed_litert_process_command(&command)
                    || managed_engine_quantize_command(&command));
            !owned_by_ctox
        }
        None => true,
    }
}

pub(crate) fn acquire_backend_startup_lease(
    root: &Path,
    port: u16,
    active_model: &str,
) -> Result<Option<BackendStartupLease>> {
    let path = backend_startup_lock_path(root, port);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime dir {}", parent.display()))?;
    }

    for _ in 0..2 {
        match try_create_backend_startup_lease(&path, port, active_model) {
            Ok(lease) => return Ok(Some(lease)),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                if lock_file_is_stale(root, &path) {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                return Ok(None);
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("failed to create backend startup lock {}", path.display())
                });
            }
        }
    }

    Ok(None)
}

fn try_create_backend_startup_lease(
    path: &Path,
    port: u16,
    active_model: &str,
) -> IoResult<BackendStartupLease> {
    let mut handle = OpenOptions::new().write(true).create_new(true).open(path)?;
    writeln!(handle, "pid={}", std::process::id())?;
    writeln!(handle, "port={port}")?;
    writeln!(handle, "model={active_model}")?;
    Ok(BackendStartupLease {
        path: path.to_path_buf(),
    })
}

fn ensure_proxy_process(root: &Path) -> Result<()> {
    let resolved = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok();
    let state = runtime_state::load_or_resolve_runtime_state(root).ok();
    let host = resolved
        .as_ref()
        .map(|runtime| runtime.proxy.listen_host.clone())
        .or_else(|| state.as_ref().map(|runtime| runtime.proxy_host.clone()))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = resolved
        .as_ref()
        .map(|runtime| runtime.proxy.listen_port)
        .or_else(|| state.as_ref().map(|runtime| runtime.proxy_port))
        .unwrap_or(12434);
    let health_url = format!("http://{host}:{port}/ctox/telemetry");
    let pid_path = proxy_pid_path(root);
    if health_check(&health_url) {
        if let Some(pid) = read_pid(&pid_path).filter(|pid| process_is_alive(*pid)) {
            sync_proxy_runtime_ownership(
                root,
                &host,
                port,
                Some(pid),
                runtime_contract::RuntimeResidencyPhase::Active,
            )?;
        } else if let Some(listener_pid) = listening_pids_for_port(root, port)?.into_iter().next() {
            std::fs::write(&pid_path, format!("{listener_pid}\n")).with_context(|| {
                format!("failed to write proxy pid file {}", pid_path.display())
            })?;
            sync_proxy_runtime_ownership(
                root,
                &host,
                port,
                Some(listener_pid),
                runtime_contract::RuntimeResidencyPhase::Active,
            )?;
        }
        return Ok(());
    }

    if let Some(pid) = read_pid(&pid_path).filter(|pid| process_is_alive(*pid)) {
        let listeners = listening_pids_for_port(root, port)?;
        if listeners.contains(&pid) {
            sync_proxy_runtime_ownership(
                root,
                &host,
                port,
                Some(pid),
                runtime_contract::RuntimeResidencyPhase::Starting,
            )?;
            return Ok(());
        }
        stop_process(root, pid_path.clone())?;
        release_proxy_runtime_ownership(root);
    }
    if let Some(listener_pid) = listening_pids_for_port(root, port)?.into_iter().next() {
        std::fs::write(&pid_path, format!("{listener_pid}\n"))
            .with_context(|| format!("failed to write proxy pid file {}", pid_path.display()))?;
        sync_proxy_runtime_ownership(
            root,
            &host,
            port,
            Some(listener_pid),
            runtime_contract::RuntimeResidencyPhase::Starting,
        )?;
        return Ok(());
    }
    stop_process(root, pid_path.clone())?;
    release_proxy_runtime_ownership(root);

    let runtime_dir = root.join("runtime");
    std::fs::create_dir_all(&runtime_dir)
        .with_context(|| format!("failed to create runtime dir {}", runtime_dir.display()))?;
    let log_path = runtime_dir.join("ctox_proxy.log");
    let log_file = open_log_file(&log_path)?;
    let log_file_err = log_file
        .try_clone()
        .with_context(|| format!("failed to clone proxy log {}", log_path.display()))?;
    let exe = std::env::current_exe().context("failed to resolve current CTOX executable")?;
    let mut command = Command::new(&exe);
    command
        .arg("serve-responses-proxy")
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err));
    apply_clean_child_env(&mut command);
    if let Some(codex_home) = inherited_env_value("CODEX_HOME") {
        command.env("CODEX_HOME", codex_home);
    }
    command.env("CTOX_ROOT", root);
    configure_managed_child_process_with_parent_death(&mut command, true);
    let child = command
        .spawn()
        .context("failed to spawn CTOX responses proxy")?;
    std::fs::write(&pid_path, format!("{}\n", child.id()))
        .with_context(|| format!("failed to write proxy pid file {}", pid_path.display()))?;
    sync_proxy_runtime_ownership(
        root,
        &host,
        port,
        Some(child.id()),
        runtime_contract::RuntimeResidencyPhase::Starting,
    )?;
    Ok(())
}

fn apply_managed_backend_bootstrap_env(command: &mut Command) {
    apply_clean_child_env(command);
    for key in MANAGED_BACKEND_BOOTSTRAP_ENV_KEYS {
        if let Some(value) = inherited_env_value(key) {
            command.env(key, value);
        }
    }
}

fn build_managed_backend_launch_spec(
    root: &Path,
    role: ManagedBackendRole,
    spec: &ManagedBackendSpec,
    admission: &runtime_gpu_manager::GpuAdmission,
) -> Result<ManagedBackendLaunchSpec> {
    let (engine_config, litert_config) = match spec.launcher_kind {
        ManagedLauncherKind::Engine => {
            ensure_chat_quant_artifact_for_launch(root, role)?;
            (
                build_managed_engine_launch_config(root, role, spec, admission)?,
                None,
            )
        }
        ManagedLauncherKind::LiteRt => (
            ManagedEngineLaunchConfig::default(),
            Some(build_managed_litert_launch_config(
                root, role, spec, admission,
            )?),
        ),
    };
    Ok(ManagedBackendLaunchSpec {
        version: 2,
        role: role.as_env_value().to_string(),
        display_model: spec.display_model.clone(),
        request_model: spec.request_model.clone(),
        port: spec.port,
        socket_path: spec.socket_path.clone(),
        health_path: spec.health_path.to_string(),
        launcher_kind: spec.launcher_kind.as_str().to_string(),
        compute_target: spec
            .compute_target
            .map(|target| target.as_env_value().to_string()),
        visible_devices: admission.visible_devices.clone(),
        engine_config,
        litert_config,
    })
}

fn sanitize_config_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '_',
        })
        .collect()
}

fn managed_engine_runtime_config_path(root: &Path, spec: &ManagedBackendSpec) -> PathBuf {
    let socket_component = spec
        .socket_path
        .as_deref()
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .map(sanitize_config_path_component)
        .unwrap_or_else(|| "tcp".to_string());
    let model_digest = format!("{:x}", sha2::Sha256::digest(spec.request_model.as_bytes()));
    root.join("runtime")
        .join("managed_engine_configs")
        .join(format!(
            "{}_{}_{}_{}.toml",
            spec.launcher_kind.as_str(),
            spec.port,
            socket_component,
            &model_digest[..12]
        ))
}

fn managed_litert_runtime_config_path(root: &Path, spec: &ManagedBackendSpec) -> PathBuf {
    let socket_component = spec
        .socket_path
        .as_deref()
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .map(sanitize_config_path_component)
        .unwrap_or_else(|| "tcp".to_string());
    let model_digest = format!("{:x}", sha2::Sha256::digest(spec.request_model.as_bytes()));
    root.join("runtime")
        .join("managed_litert_configs")
        .join(format!(
            "{}_{}_{}_{}.json",
            spec.launcher_kind.as_str(),
            spec.port,
            socket_component,
            &model_digest[..12]
        ))
}

fn toml_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn render_engine_paged_cache_type(cache_type: &str) -> &str {
    match cache_type {
        "auto" => "Auto",
        "f8e4m3" => "F8E4M3",
        "turboquant2" => "TurboQuant2",
        "turboquant3" => "TurboQuant3",
        "turboquant4" => "TurboQuant4",
        other => other,
    }
}

fn managed_engine_model_kind(
    role: &str,
    request_model: &str,
    language_model_only: bool,
) -> &'static str {
    match role {
        "embedding" => "embedding",
        "tts" => "speech",
        // The ctox-engine exposes a single "vision" bucket for every
        // non-text input modality — Whisper/Voxtral STT feed through the
        // same vision pipeline kind as Qwen3-VL / Gemma4 image input.
        // That's not a misnomer but an engine-side design choice.
        "stt" => "vision",
        "vision" => "vision",
        "chat" if language_model_only => "text",
        _ => engine::model_profile_for_model(request_model)
            .ok()
            .map(
                |profile| match profile.family_profile.launcher_mode.trim() {
                    "embedding" => "embedding",
                    "speech" => "speech",
                    "vision" => "vision",
                    _ => "text",
                },
            )
            .unwrap_or_else(|| {
                if is_qwen35_vision_request_model(request_model) {
                    "vision"
                } else {
                    "text"
                }
            }),
    }
}

fn render_managed_engine_runtime_config(launch_spec: &ManagedBackendLaunchSpec) -> String {
    let mut out = String::new();
    out.push_str("command = \"serve\"\n\n");
    if let Some(log_path) = launch_spec.engine_config.log_path.as_deref() {
        out.push_str("[global]\n");
        out.push_str(&format!("log = \"{}\"\n\n", toml_escape(log_path)));
    }

    out.push_str("[server]\n");
    out.push_str("transport = \"local_socket\"\n");
    if let Some(socket_path) = launch_spec.socket_path.as_deref() {
        out.push_str(&format!("socket_path = \"{}\"\n", toml_escape(socket_path)));
    }
    out.push('\n');

    out.push_str("[runtime]\n");
    out.push_str(&format!(
        "max_seqs = {}\n\n",
        launch_spec.engine_config.max_seqs.unwrap_or(1)
    ));

    out.push_str("[paged_attn]\n");
    out.push_str(&format!(
        "mode = \"{}\"\n",
        toml_escape(
            launch_spec
                .engine_config
                .paged_attn
                .as_deref()
                .unwrap_or("auto")
        )
    ));
    if let Some(context_len) = launch_spec.engine_config.max_seq_len {
        out.push_str(&format!("context_len = {}\n", context_len));
    } else if let Some(context_len) = launch_spec.engine_config.pa_context_len {
        out.push_str(&format!("context_len = {}\n", context_len));
    }
    if let Some(memory_fraction) = launch_spec.engine_config.pa_memory_fraction.as_deref() {
        out.push_str(&format!("memory_fraction = {}\n", memory_fraction));
    }
    if let Some(cache_type) = launch_spec.engine_config.pa_cache_type.as_deref() {
        out.push_str(&format!(
            "cache_type = \"{}\"\n",
            toml_escape(render_engine_paged_cache_type(cache_type))
        ));
    }
    out.push('\n');

    out.push_str("[[models]]\n");
    out.push_str(&format!(
        "kind = \"{}\"\n",
        managed_engine_model_kind(
            launch_spec.role.as_str(),
            launch_spec.request_model.as_str(),
            launch_spec.engine_config.language_model_only,
        )
    ));
    out.push_str(&format!(
        "model_id = \"{}\"\n",
        toml_escape(launch_spec.request_model.as_str())
    ));
    if let Some(arch) = launch_spec.engine_config.arch.as_deref() {
        out.push_str(&format!("arch = \"{}\"\n", toml_escape(arch)));
    }
    out.push('\n');

    out.push_str("[models.quantization]\n");
    if let Some(isq) = launch_spec.engine_config.isq.as_deref() {
        out.push_str(&format!("in_situ_quant = \"{}\"\n", toml_escape(isq)));
    }
    if let Some(from_uqff) = launch_spec.engine_config.from_uqff.as_deref() {
        out.push_str(&format!("from_uqff = \"{}\"\n", toml_escape(from_uqff)));
    }
    if let Some(write_uqff) = launch_spec.engine_config.write_uqff.as_deref() {
        out.push_str(&format!("write_uqff = \"{}\"\n", toml_escape(write_uqff)));
    }
    if let Some(isq_org) = launch_spec.engine_config.isq_organization.as_deref() {
        out.push_str(&format!(
            "isq_organization = \"{}\"\n",
            toml_escape(isq_org)
        ));
    }
    out.push('\n');

    out.push_str("[models.device]\n");
    if launch_spec
        .compute_target
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("cpu"))
    {
        out.push_str("cpu = true\n");
    }
    if let Some(device_layers) = launch_spec.engine_config.device_layers.as_deref() {
        let items = device_layers
            .split(';')
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("\"{}\"", toml_escape(value.trim())))
            .collect::<Vec<_>>()
            .join(", ");
        if !items.is_empty() {
            out.push_str(&format!("device_layers = [{}]\n", items));
        }
    }
    if let Some(topology) = launch_spec.engine_config.topology.as_deref() {
        out.push_str(&format!("topology = \"{}\"\n", toml_escape(topology)));
    }
    if let Some(max_seq_len) = launch_spec.engine_config.max_seq_len {
        out.push_str(&format!("max_seq_len = {}\n", max_seq_len));
    }
    if let Some(max_batch_size) = launch_spec.engine_config.max_batch_size {
        out.push_str(&format!("max_batch_size = {}\n", max_batch_size));
    }

    out
}

fn persist_managed_engine_runtime_config(
    root: &Path,
    spec: &ManagedBackendSpec,
    launch_spec: &ManagedBackendLaunchSpec,
) -> Result<PathBuf> {
    let path = managed_engine_runtime_config_path(root, spec);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create managed engine runtime config dir {}",
                parent.display()
            )
        })?;
    }
    std::fs::write(&path, render_managed_engine_runtime_config(launch_spec)).with_context(
        || {
            format!(
                "failed to write managed engine runtime config {}",
                path.display()
            )
        },
    )?;
    Ok(path)
}

fn render_managed_litert_runtime_config(launch_spec: &ManagedBackendLaunchSpec) -> Result<String> {
    let Some(config) = launch_spec.litert_config.as_ref() else {
        anyhow::bail!("missing litert launch config for managed LiteRT backend");
    };
    let json = serde_json::json!({
        "version": launch_spec.version,
        "role": launch_spec.role,
        "cli_path": config.cli_path,
        "model_reference": config.model_reference,
        "model_file": config.model_file,
        "huggingface_repo": config.huggingface_repo,
        "huggingface_token": config.huggingface_token,
        "backend": config.backend,
        "context_tokens": config.context_tokens,
        "validated_context_tokens": config.validated_context_tokens,
        "port": launch_spec.port,
        "socket_path": launch_spec.socket_path,
        "health_path": launch_spec.health_path,
        "speculative_decoding": config.speculative_decoding,
        "verbose": config.verbose,
        "visible_devices": launch_spec.visible_devices,
        "compute_target": launch_spec.compute_target,
        "log_path": config.log_path,
    });
    serde_json::to_string_pretty(&json).context("failed to encode managed LiteRT runtime config")
}

fn persist_managed_litert_runtime_config(
    root: &Path,
    spec: &ManagedBackendSpec,
    launch_spec: &ManagedBackendLaunchSpec,
) -> Result<PathBuf> {
    let path = managed_litert_runtime_config_path(root, spec);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create managed litert runtime config dir {}",
                parent.display()
            )
        })?;
    }
    std::fs::write(&path, render_managed_litert_runtime_config(launch_spec)?).with_context(
        || {
            format!(
                "failed to write managed litert runtime config {}",
                path.display()
            )
        },
    )?;
    Ok(path)
}

fn ambient_env_flag_enabled(key: &str) -> bool {
    std::env::var(key).ok().is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn build_managed_engine_launch_config(
    root: &Path,
    role: ManagedBackendRole,
    spec: &ManagedBackendSpec,
    admission: &runtime_gpu_manager::GpuAdmission,
) -> Result<ManagedEngineLaunchConfig> {
    let env_map = collect_managed_backend_env(root, role, spec, admission)?;
    Ok(ManagedEngineLaunchConfig {
        binary_path: env_map.get("CTOX_ENGINE_BINARY").cloned(),
        log_path: env_map.get("CTOX_ENGINE_LOG").cloned(),
        arch: env_map.get("CTOX_ENGINE_ARCH").cloned(),
        isq: env_map.get("CTOX_ENGINE_ISQ").cloned(),
        isq_organization: env_map.get("CTOX_ENGINE_ISQ_ORGANIZATION").cloned(),
        paged_attn: env_map.get("CTOX_ENGINE_PAGED_ATTN").cloned(),
        pa_cache_type: env_map.get("CTOX_ENGINE_PA_CACHE_TYPE").cloned(),
        pa_memory_fraction: env_map.get("CTOX_ENGINE_PA_MEMORY_FRACTION").cloned(),
        pa_context_len: env_map
            .get("CTOX_ENGINE_PA_CONTEXT_LEN")
            .and_then(|value| value.parse::<u32>().ok()),
        tensor_parallel_backend: env_map.get("CTOX_ENGINE_TENSOR_PARALLEL_BACKEND").cloned(),
        mn_local_world_size: env_map
            .get("CTOX_ENGINE_MN_LOCAL_WORLD_SIZE")
            .and_then(|value| value.parse::<u32>().ok()),
        max_batch_size: env_map
            .get("CTOX_ENGINE_MAX_BATCH_SIZE")
            .and_then(|value| value.parse::<u32>().ok()),
        max_seqs: env_map
            .get("CTOX_ENGINE_MAX_SEQS")
            .and_then(|value| value.parse::<u32>().ok()),
        max_seq_len: env_map
            .get("CTOX_ENGINE_MAX_SEQ_LEN")
            .and_then(|value| value.parse::<u32>().ok()),
        device_layers: env_map
            .get("CTOX_ENGINE_NUM_DEVICE_LAYERS")
            .cloned()
            .or_else(|| env_map.get("CTOX_ENGINE_DEVICE_LAYERS").cloned()),
        topology: env_map.get("CTOX_ENGINE_TOPOLOGY").cloned(),
        allow_device_layers_with_topology: env_map
            .get("CTOX_ENGINE_ALLOW_DEVICE_LAYERS_WITH_TOPOLOGY")
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            }),
        nm_device_ordinal: env_map
            .get("CTOX_ENGINE_NM_DEVICE_ORDINAL")
            .and_then(|value| value.parse::<u32>().ok()),
        base_device_ordinal: env_map
            .get("CTOX_ENGINE_BASE_DEVICE_ORDINAL")
            .and_then(|value| value.parse::<u32>().ok()),
        moe_experts_backend: env_map.get("CTOX_ENGINE_MOE_EXPERTS_BACKEND").cloned(),
        from_uqff: env_map.get("CTOX_ENGINE_FROM_UQFF").cloned(),
        write_uqff: env_map.get("CTOX_ENGINE_WRITE_UQFF").cloned(),
        disable_nccl: env_map
            .get("CTOX_ENGINE_DISABLE_NCCL")
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            || env_map
                .get("CTOX_ENGINE_TENSOR_PARALLEL_BACKEND")
                .is_some_and(|value| value.eq_ignore_ascii_case("disabled")),
        disable_flash_attn: env_map
            .get("CTOX_ENGINE_DISABLE_FLASH_ATTN")
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            }),
        no_mmap: env_map.get("CTOX_ENGINE_NO_MMAP").is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        }),
        language_model_only: env_map
            .get("CTOX_ENGINE_LANGUAGE_MODEL_ONLY")
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            }),
        isq_singlethread: env_map
            .get("CTOX_ENGINE_ISQ_SINGLETHREAD")
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            }),
        isq_cpu_threads: env_map
            .get("CTOX_ENGINE_ISQ_CPU_THREADS")
            .and_then(|value| value.parse::<u32>().ok()),
        parallel_immediate_isq: env_map
            .get("CTOX_ENGINE_PARALLEL_IMMEDIATE_ISQ")
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            }),
    })
}

fn build_managed_litert_launch_config(
    root: &Path,
    role: ManagedBackendRole,
    spec: &ManagedBackendSpec,
    admission: &runtime_gpu_manager::GpuAdmission,
) -> Result<ManagedLiteRtLaunchConfig> {
    let runtime_state = runtime_state::load_or_resolve_runtime_state(root)?;
    let backend = runtime_env::env_or_config(root, "CTOX_LITERT_BACKEND")
        .filter(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "cpu" | "gpu"))
        .unwrap_or_else(|| {
            if admission
                .visible_devices
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
            {
                "gpu".to_string()
            } else if spec.compute_target == Some(engine::ComputeTarget::Gpu) {
                "gpu".to_string()
            } else {
                "cpu".to_string()
            }
        });
    let role_prefix = role.as_env_value().to_ascii_uppercase();
    let default_artifact = default_litert_artifact_for_model(&spec.request_model);
    let validated_context_tokens = default_artifact
        .map(|artifact| artifact.validated_context_tokens)
        .unwrap_or(131_072);
    let context_tokens = runtime_state
        .realized_context_tokens
        .unwrap_or(validated_context_tokens);
    let mut model_file =
        runtime_env::env_or_config(root, &format!("CTOX_{role_prefix}_LITERT_MODEL_FILE"))
            .or_else(|| runtime_env::env_or_config(root, "CTOX_LITERT_MODEL_FILE"));
    let mut huggingface_repo =
        runtime_env::env_or_config(root, &format!("CTOX_{role_prefix}_LITERT_HUGGINGFACE_REPO"))
            .or_else(|| runtime_env::env_or_config(root, "CTOX_LITERT_HUGGINGFACE_REPO"));
    let huggingface_token = runtime_env::env_or_config(
        root,
        &format!("CTOX_{role_prefix}_LITERT_HUGGINGFACE_TOKEN"),
    )
    .or_else(|| runtime_env::env_or_config(root, "CTOX_LITERT_HUGGINGFACE_TOKEN"))
    .or_else(|| std::env::var("HF_TOKEN").ok())
    .or_else(|| std::env::var("HUGGING_FACE_HUB_TOKEN").ok());
    if model_file.is_none() {
        model_file = default_artifact.map(|artifact| artifact.model_file.to_string());
    }
    if huggingface_repo.is_none() {
        huggingface_repo = default_artifact.map(|artifact| artifact.huggingface_repo.to_string());
    }
    if model_file.is_none() && huggingface_repo.is_none() {
        if spec.request_model.trim() == "google/gemma-4-E2B-it" {
            anyhow::bail!(
                "managed LiteRT backend does not have an active default artifact mapping for {}; the published E2B LiteRT bundle is currently disabled after host forensics found a stale MTP build. Rebuild the artifact and set CTOX_LITERT_MODEL_FILE or CTOX_LITERT_HUGGINGFACE_REPO explicitly.",
                spec.request_model
            );
        }
        anyhow::bail!(
            "managed LiteRT backend does not have a qualified artifact mapping for {}",
            spec.request_model
        );
    }
    if context_tokens > validated_context_tokens {
        anyhow::bail!(
            "managed LiteRT backend for {} is only validated to {} tokens, but CTOX requested {}",
            spec.request_model,
            validated_context_tokens,
            context_tokens
        );
    }
    Ok(ManagedLiteRtLaunchConfig {
        bridge_binary_path: runtime_env::env_or_config(root, "CTOX_LITERT_BRIDGE_BINARY"),
        cli_path: runtime_env::env_or_config(root, "CTOX_LITERT_CLI"),
        log_path: runtime_env::env_or_config(root, "CTOX_LITERT_BRIDGE_LOG"),
        backend,
        context_tokens,
        validated_context_tokens,
        model_reference: spec.request_model.clone(),
        model_file,
        huggingface_repo,
        huggingface_token,
        speculative_decoding: runtime_env::env_or_config(root, "CTOX_LITERT_SPECULATIVE_DECODING")
            .filter(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "auto" | "true" | "false"
                )
            })
            .unwrap_or_else(|| "auto".to_string()),
        verbose: runtime_env::env_or_config(root, "CTOX_LITERT_VERBOSE")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false),
    })
}

fn default_litert_artifact_for_model(model: &str) -> Option<LiteRtArtifactSpec> {
    match model.trim() {
        "google/gemma-4-E4B-it" => Some(LiteRtArtifactSpec {
            huggingface_repo: "metricspace/gemma4-E4B-it-litert-128k-mtp",
            model_file: "model.litertlm",
            validated_context_tokens: 131_072,
        }),
        _ => None,
    }
}

fn is_qwen35_vision_request_model(model: &str) -> bool {
    model.starts_with("Qwen/Qwen3.5-")
}

fn resolve_managed_engine_binary(
    root: &Path,
    launch_spec: &ManagedBackendLaunchSpec,
) -> Result<PathBuf> {
    let configured = launch_spec
        .engine_config
        .binary_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file());
    let binary = configured
        .unwrap_or_else(|| engine::discover_source_layout_paths(root).model_runtime_binary);
    if !binary.is_file() {
        anyhow::bail!(
            "ctox-engine binary missing at {}. Build it with: \
             `cd tools/model-runtime && cargo build --release --bin ctox-engine`",
            binary.display()
        );
    }
    runtime_engine_guard::ensure_engine_binary_matches_host(
        root,
        &binary,
        launch_spec
            .compute_target
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("cpu")),
    )?;
    Ok(binary)
}

fn resolve_managed_litert_bridge_binary(
    root: &Path,
    launch_spec: &ManagedBackendLaunchSpec,
) -> Result<PathBuf> {
    let Some(config) = launch_spec.litert_config.as_ref() else {
        anyhow::bail!("missing litert launch config for managed LiteRT backend");
    };
    let binary = config
        .bridge_binary_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| std::env::current_exe().ok().filter(|path| path.is_file()))
        .or_else(|| {
            let candidate = root.join("target/release/ctox");
            candidate.is_file().then_some(candidate)
        })
        .context("managed LiteRT backend could not resolve the ctox bridge binary")?;
    if !binary.is_file() {
        anyhow::bail!(
            "configured LiteRT bridge binary is missing: {}",
            binary.display()
        );
    }
    Ok(binary)
}

fn configure_managed_engine_runtime_env(
    command: &mut Command,
    launch_spec: &ManagedBackendLaunchSpec,
) {
    let parallel_immediate_isq = launch_spec.engine_config.parallel_immediate_isq;
    let compute_target = launch_spec.compute_target.as_deref().unwrap_or("gpu");
    if compute_target.eq_ignore_ascii_case("cpu") {
        command.env_remove("CUDA_VISIBLE_DEVICES");
    } else if let Some(visible_devices) = launch_spec
        .visible_devices
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        command.env("CUDA_VISIBLE_DEVICES", visible_devices);
    }

    if launch_spec.engine_config.disable_nccl
        || launch_spec
            .engine_config
            .tensor_parallel_backend
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("disabled"))
    {
        command.env("ENGINE_NO_NCCL", "1");
    }
    if let Some(world_size) = launch_spec.engine_config.mn_local_world_size {
        command.env("ENGINE_MN_LOCAL_WORLD_SIZE", world_size.to_string());
    }
    if let Some(device_ordinal) = launch_spec.engine_config.nm_device_ordinal {
        command.env("ENGINE_NM_DEVICE_ORDINAL", device_ordinal.to_string());
    }
    if let Some(device_ordinal) = launch_spec.engine_config.base_device_ordinal {
        command.env("ENGINE_BASE_DEVICE_ORDINAL", device_ordinal.to_string());
    }
    if let Some(moe_backend) = launch_spec.engine_config.moe_experts_backend.as_deref() {
        command.env("ENGINE_MOE_EXPERTS_BACKEND", moe_backend);
    }
    if launch_spec.engine_config.disable_flash_attn {
        command.env("ENGINE_DISABLE_FLASH_ATTN", "1");
    }
    if launch_spec.engine_config.no_mmap {
        command.env("ENGINE_NO_MMAP", "1");
    }
    if launch_spec.engine_config.language_model_only {
        command.env("ENGINE_LANGUAGE_MODEL_ONLY", "1");
    }
    if launch_spec.engine_config.isq_singlethread && !parallel_immediate_isq {
        command.env("ENGINE_ISQ_SINGLETHREAD", "1");
    }
    if let Some(threads) = launch_spec.engine_config.isq_cpu_threads {
        command.env("ENGINE_ISQ_CPU_THREADS", threads.to_string());
    }
    if parallel_immediate_isq {
        command.env("ENGINE_PARALLEL_IMMEDIATE_ISQ", "1");
    }
    if let Some(max_seq_len) = launch_spec.engine_config.max_seq_len {
        command.env("ENGINE_MAX_SEQ_LEN_OVERRIDE", max_seq_len.to_string());
    }
    command.env("ENGINE_SKIP_DUMMY_RUN", "1");
}

fn spawn_managed_engine_backend(
    root: &Path,
    spec: &ManagedBackendSpec,
    launch_spec: &ManagedBackendLaunchSpec,
    stdout: Stdio,
    stderr: Stdio,
) -> Result<Child> {
    let engine_binary = resolve_managed_engine_binary(root, launch_spec)?;
    let config_path = persist_managed_engine_runtime_config(root, spec, launch_spec)?;
    let mut command = Command::new(&engine_binary);
    command.arg("from-config").arg("--file").arg(config_path);
    command.current_dir(root);
    command.stdin(Stdio::null()).stdout(stdout).stderr(stderr);
    apply_clean_child_env(&mut command);
    configure_managed_engine_runtime_env(&mut command, launch_spec);
    configure_managed_child_process(&mut command);
    command.spawn().with_context(|| {
        format!(
            "failed to spawn ctox-engine for managed backend {}",
            launch_spec.display_model
        )
    })
}

fn spawn_managed_litert_backend(
    root: &Path,
    spec: &ManagedBackendSpec,
    launch_spec: &ManagedBackendLaunchSpec,
    stdout: Stdio,
    stderr: Stdio,
) -> Result<Child> {
    let bridge_binary = resolve_managed_litert_bridge_binary(root, launch_spec)?;
    let config_path = persist_managed_litert_runtime_config(root, spec, launch_spec)?;
    let mut command = Command::new(&bridge_binary);
    command
        .arg("serve-litert-bridge")
        .arg("--config")
        .arg(config_path);
    command.current_dir(root);
    command.stdin(Stdio::null()).stdout(stdout).stderr(stderr);
    apply_clean_child_env(&mut command);
    configure_managed_child_process(&mut command);
    command.spawn().with_context(|| {
        format!(
            "failed to spawn LiteRT bridge for managed backend {}",
            launch_spec.display_model
        )
    })
}

fn spawn_managed_backend(
    root: &Path,
    spec: &ManagedBackendSpec,
    launch_spec: &ManagedBackendLaunchSpec,
    stdout: Stdio,
    stderr: Stdio,
) -> Result<Child> {
    match spec.launcher_kind {
        ManagedLauncherKind::Engine => {
            spawn_managed_engine_backend(root, spec, launch_spec, stdout, stderr)
        }
        ManagedLauncherKind::LiteRt => {
            spawn_managed_litert_backend(root, spec, launch_spec, stdout, stderr)
        }
    }
}

fn ensure_backend_process(
    root: &Path,
    role: ManagedBackendRole,
    force_restart: bool,
) -> Result<()> {
    if role == ManagedBackendRole::Chat
        && runtime_env::env_or_config(root, "CTOX_CHAT_SOURCE")
            .map(|value| value.trim().eq_ignore_ascii_case("api"))
            .unwrap_or(false)
    {
        stop_process(root, backend_pid_path(root, role))?;
        release_backend_runtime_ownership(root, role);
        return Ok(());
    }

    if role == ManagedBackendRole::Chat {
        runtime_plan::reconcile_chat_runtime_plan(root)?;
    }

    let spec = role.spec(root);
    if spec.request_model.trim().is_empty() {
        return Ok(());
    }
    let descriptor = backend_runtime_descriptor(role, &spec);
    let admission = runtime_gpu_manager::resolve_gpu_admission(root, &descriptor)?;
    let pid_path = backend_pid_path(root, role);
    if role != ManagedBackendRole::Chat
        && spec.compute_target == Some(engine::ComputeTarget::Gpu)
        && admission
            .visible_devices
            .as_deref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
    {
        stop_processes_on_port(root, spec.port)?;
        stop_process(root, pid_path.clone())?;
        release_backend_runtime_ownership(root, role);
        return Ok(());
    }
    if force_restart {
        stop_processes_on_port(root, spec.port)?;
        stop_process(root, pid_path.clone())?;
        let _ = stop_duplicate_socket_backed_backend_processes(root, &spec, None);
        release_backend_runtime_ownership(root, role);
    }
    dedupe_socket_backed_backend_processes(root, &spec, &pid_path)?;
    if let Some(matched_pid) = ready_backend_process_pid(root, &spec, &pid_path)? {
        std::fs::write(&pid_path, format!("{matched_pid}\n"))
            .with_context(|| format!("failed to write backend pid file {}", pid_path.display()))?;
        let _ = stop_duplicate_socket_backed_backend_processes(root, &spec, Some(matched_pid));
        runtime_gpu_manager::sync_workload_runtime_residency(
            root,
            &descriptor,
            &admission,
            Some(matched_pid),
            runtime_contract::RuntimeResidencyPhase::Active,
        )?;
        return Ok(());
    }
    let mut matching_startup_in_progress = false;
    if read_pid(&pid_path)
        .filter(|pid| process_is_alive(*pid))
        .is_some()
    {
        let pid = read_pid(&pid_path);
        if let Some(pid) = pid {
            if backend_process_owns_ready_transport(root, &spec, pid)? {
                runtime_gpu_manager::sync_workload_runtime_residency(
                    root,
                    &descriptor,
                    &admission,
                    Some(pid),
                    runtime_contract::RuntimeResidencyPhase::Active,
                )?;
                return Ok(());
            }
            if backend_process_matches_launch_spec(root, &spec, pid)?
                && backend_startup_in_progress(root, spec.port)
            {
                runtime_gpu_manager::sync_workload_runtime_residency(
                    root,
                    &descriptor,
                    &admission,
                    Some(pid),
                    runtime_contract::RuntimeResidencyPhase::Starting,
                )?;
                matching_startup_in_progress = true;
            }
        }
        if !matching_startup_in_progress {
            stop_process(root, pid_path.clone())?;
            let _ = stop_duplicate_socket_backed_backend_processes(root, &spec, None);
            release_backend_runtime_ownership(root, role);
        }
    }
    if let Some(matched_pid) = ready_backend_process_pid(root, &spec, &pid_path)? {
        std::fs::write(&pid_path, format!("{matched_pid}\n"))
            .with_context(|| format!("failed to write backend pid file {}", pid_path.display()))?;
        let _ = stop_duplicate_socket_backed_backend_processes(root, &spec, Some(matched_pid));
        runtime_gpu_manager::sync_workload_runtime_residency(
            root,
            &descriptor,
            &admission,
            Some(matched_pid),
            runtime_contract::RuntimeResidencyPhase::Starting,
        )?;
        return Ok(());
    }
    let startup_wait_secs = backend_startup_wait_secs_for_spec(role, &spec);
    let startup_started = Instant::now();
    let Some(_lease) = acquire_backend_startup_lease(root, spec.port, &spec.request_model)? else {
        while startup_started.elapsed() < Duration::from_secs(startup_wait_secs) {
            if let Some(matched_pid) = ready_backend_process_pid(root, &spec, &pid_path)? {
                std::fs::write(&pid_path, format!("{matched_pid}\n")).with_context(|| {
                    format!("failed to write backend pid file {}", pid_path.display())
                })?;
                let _ =
                    stop_duplicate_socket_backed_backend_processes(root, &spec, Some(matched_pid));
                runtime_gpu_manager::sync_workload_runtime_residency(
                    root,
                    &descriptor,
                    &admission,
                    Some(matched_pid),
                    runtime_contract::RuntimeResidencyPhase::Active,
                )?;
                return Ok(());
            }
            thread::sleep(Duration::from_millis(250));
        }
        anyhow::bail!(
            "backend for {} on port {} did not become healthy while waiting for existing startup",
            spec.display_model,
            spec.port
        );
    };

    stop_process(root, pid_path.clone())?;
    let _ = stop_duplicate_socket_backed_backend_processes(root, &spec, None);
    release_backend_runtime_ownership(root, role);

    let runtime_dir = root.join("runtime");
    std::fs::create_dir_all(&runtime_dir)
        .with_context(|| format!("failed to create runtime dir {}", runtime_dir.display()))?;
    let log_path = runtime_dir.join(role.log_file_name());
    let log_file = open_log_file(&log_path)?;
    let log_file_err = log_file
        .try_clone()
        .with_context(|| format!("failed to clone backend log {}", log_path.display()))?;
    let launch_spec = build_managed_backend_launch_spec(root, role, &spec, &admission)?;
    if role != ManagedBackendRole::Chat {
        if spec.compute_target == Some(engine::ComputeTarget::Gpu) {
            admission
                .visible_devices
                .clone()
                .filter(|value| !value.trim().is_empty())
                .with_context(|| {
                    format!(
                        "no dedicated GPU allocation is available for {} backend {}",
                        role.as_env_value(),
                        spec.display_model
                    )
                })?;
        }
    }
    if spec.compute_target == Some(engine::ComputeTarget::Gpu) || role == ManagedBackendRole::Chat {
        runtime_gpu_manager::prepare_workload_launch(root, &descriptor, &admission)?;
    }
    let mut child = spawn_managed_backend(
        root,
        &spec,
        &launch_spec,
        Stdio::from(log_file),
        Stdio::from(log_file_err),
    )
    .with_context(|| {
        format!(
            "failed to spawn {} backend for {}",
            role.as_env_value(),
            spec.display_model
        )
    })?;
    std::fs::write(&pid_path, format!("{}\n", child.id()))
        .with_context(|| format!("failed to write backend pid file {}", pid_path.display()))?;
    runtime_gpu_manager::sync_workload_runtime_residency(
        root,
        &descriptor,
        &admission,
        Some(child.id()),
        runtime_contract::RuntimeResidencyPhase::Starting,
    )?;
    match wait_for_backend_ready(
        root,
        role,
        &spec,
        &descriptor,
        &admission,
        &pid_path,
        &log_path,
        Some(&mut child),
    ) {
        Ok(()) => {
            let _ = finalize_chat_quant_artifact(root, role);
            let _ = maybe_prime_chat_quant_artifact(root, role);
            Ok(())
        }
        Err(err) => {
            let _ = stop_process(root, pid_path.clone());
            let _ = stop_processes_on_port(root, spec.port);
            let _ = stop_duplicate_socket_backed_backend_processes(root, &spec, None);
            let _ = kill_workspace_managed_runtime_groups(root);
            let _ = wait_for_managed_runtime_fleet_idle(
                root,
                Duration::from_secs(PERSISTENT_BACKEND_SHUTDOWN_TIMEOUT_SECS),
            );
            release_backend_runtime_ownership(root, role);
            let _ = cleanup_failed_chat_quant_artifact(root, role);
            let _ = clear_failed_local_chat_runtime_projection(root, role);
            Err(err)
        }
    }
}

fn dedupe_socket_backed_backend_processes(
    root: &Path,
    spec: &ManagedBackendSpec,
    pid_path: &Path,
) -> Result<()> {
    if spec.socket_path.is_none() {
        return Ok(());
    }
    let matching = matching_socket_pids_for_backend(root, spec)?;
    if matching.len() <= 1 {
        return Ok(());
    }
    let preserve_pid = read_pid(pid_path)
        .filter(|pid| matching.contains(pid))
        .or_else(|| matching.first().copied());
    stop_duplicate_socket_backed_backend_processes(root, spec, preserve_pid)
}

fn maybe_prime_chat_quant_artifact(root: &Path, role: ManagedBackendRole) -> Result<()> {
    if role != ManagedBackendRole::Chat {
        return Ok(());
    }
    let Some(plan) = runtime_plan::load_persisted_chat_runtime_plan(root)? else {
        return Ok(());
    };
    let Some(runtime_isq) = plan.runtime_isq.as_deref() else {
        return Ok(());
    };
    let Some(artifact_path) = runtime_plan::chat_quant_artifact_write_path(root, &plan) else {
        return Ok(());
    };
    if runtime_plan::available_chat_quant_artifact(root, &plan).is_some() {
        return Ok(());
    }
    let Some(lease) = acquire_quant_artifact_build_lease(root, &artifact_path)? else {
        return Ok(());
    };
    let root = root.to_path_buf();
    let quant = runtime_isq.to_string();
    thread::spawn(move || {
        let _lease = lease;
        let _ = build_chat_quant_artifact(&root, &plan, &quant, &artifact_path);
    });
    Ok(())
}

fn ensure_chat_quant_artifact_for_launch(root: &Path, role: ManagedBackendRole) -> Result<()> {
    if role != ManagedBackendRole::Chat {
        return Ok(());
    }
    let debug_invoke = runtime_env::env_or_config(root, "CTOX_DEBUG_INVOKE_MODEL")
        .map(|value| {
            let trimmed = value.trim();
            trimmed == "1"
                || trimmed.eq_ignore_ascii_case("true")
                || trimmed.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false);
    let Some(plan) = runtime_plan::load_persisted_chat_runtime_plan(root)? else {
        return Ok(());
    };
    let Some(_runtime_isq) = plan.runtime_isq.as_deref() else {
        return Ok(());
    };
    let available_artifact = runtime_plan::available_chat_quant_artifact(root, &plan);
    if debug_invoke {
        eprintln!(
            "ctox chat quant check model={} runtime_isq={} require_prebuilt={} artifact={}",
            plan.model,
            plan.runtime_isq.as_deref().unwrap_or("<none>"),
            plan.require_prebuilt_uqff_for_chat_start,
            available_artifact
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<missing>".to_string())
        );
    }
    if available_artifact.is_some() {
        return Ok(());
    }
    if plan.require_prebuilt_uqff_for_chat_start {
        let Some(artifact_path) = runtime_plan::chat_quant_artifact_write_path(root, &plan) else {
            anyhow::bail!(
                "required chat quant artifact path missing for {}",
                plan.model
            );
        };
        if let Some(lease) = acquire_quant_artifact_build_lease(root, &artifact_path)? {
            let _lease = lease;
            let runtime_isq = plan.runtime_isq.as_deref().unwrap_or_default().to_string();
            if debug_invoke {
                eprintln!(
                    "ctox chat quant build-start model={} runtime_isq={} artifact={}",
                    plan.model,
                    runtime_isq,
                    artifact_path.display()
                );
            }
            build_chat_quant_artifact(root, &plan, &runtime_isq, &artifact_path)?;
        } else {
            if debug_invoke {
                eprintln!(
                    "ctox chat quant wait-existing-build model={} artifact={}",
                    plan.model,
                    artifact_path.display()
                );
            }
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(900) {
                if runtime_plan::available_chat_quant_artifact(root, &plan).is_some() {
                    if debug_invoke {
                        eprintln!(
                            "ctox chat quant wait-complete model={} artifact={}",
                            plan.model,
                            artifact_path.display()
                        );
                    }
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(250));
            }
            anyhow::bail!(
                "timed out waiting for required chat quant artifact for {}",
                plan.model
            );
        }
        if runtime_plan::available_chat_quant_artifact(root, &plan).is_some() {
            return Ok(());
        }
        anyhow::bail!(
            "required chat quant artifact build for {} did not produce a reusable artifact",
            plan.model
        );
    }
    // Missing chat quant artifacts must not block the baseline launch path.
    // We still build them best-effort after a successful start via
    // `maybe_prime_chat_quant_artifact`, but direct launch falls back to
    // immediate ISQ instead of synchronously waiting on cache generation.
    let _ = cleanup_failed_chat_quant_artifact(root, role);
    Ok(())
}

fn pending_chat_quant_artifact_dir(
    root: &Path,
    plan: &runtime_plan::ChatRuntimePlan,
) -> Option<PathBuf> {
    let final_path = runtime_plan::chat_quant_artifact_path(root, plan)?;
    let final_dir = final_path.parent()?;
    let dir_name = final_dir.file_name()?.to_string_lossy();
    Some(final_dir.with_file_name(format!("{dir_name}{CHAT_QUANT_ARTIFACT_PENDING_SUFFIX}")))
}

fn pending_chat_quant_artifact_path(
    root: &Path,
    plan: &runtime_plan::ChatRuntimePlan,
) -> Option<PathBuf> {
    let pending_dir = pending_chat_quant_artifact_dir(root, plan)?;
    let runtime_isq = plan.runtime_isq.as_deref()?.trim();
    if runtime_isq.is_empty() {
        return None;
    }
    Some(pending_dir.join(format!("{}.uqff", runtime_isq.to_ascii_lowercase())))
}

fn configure_chat_quant_artifact_launch_env(
    root: &Path,
    plan: &runtime_plan::ChatRuntimePlan,
    env_map: &mut BTreeMap<String, String>,
) -> Result<()> {
    env_map.remove("CTOX_ENGINE_WRITE_UQFF");
    if let Some(pending_dir) = pending_chat_quant_artifact_dir(root, plan) {
        let _ = std::fs::remove_dir_all(pending_dir);
    }
    Ok(())
}

fn finalize_chat_quant_artifact(root: &Path, role: ManagedBackendRole) -> Result<()> {
    if role != ManagedBackendRole::Chat {
        return Ok(());
    }
    let Some(plan) = runtime_plan::load_persisted_chat_runtime_plan(root)? else {
        return Ok(());
    };
    let Some(final_path) = runtime_plan::chat_quant_artifact_path(root, &plan) else {
        return Ok(());
    };
    let Some(final_dir) = final_path.parent() else {
        return Ok(());
    };
    let Some(pending_dir) = pending_chat_quant_artifact_dir(root, &plan) else {
        return Ok(());
    };
    let Some(pending_path) = pending_chat_quant_artifact_path(root, &plan) else {
        return Ok(());
    };
    if runtime_plan::available_chat_quant_artifact(root, &plan).is_some() {
        let _ = std::fs::remove_dir_all(&pending_dir);
        return Ok(());
    }
    let pending_first_shard = pending_path
        .file_stem()
        .map(|stem| pending_dir.join(format!("{}-0.uqff", stem.to_string_lossy())))
        .unwrap_or_else(|| pending_dir.join("q4k-0.uqff"));
    let Ok(metadata) = std::fs::metadata(&pending_first_shard) else {
        return Ok(());
    };
    if !metadata.is_file() || metadata.len() == 0 {
        let _ = std::fs::remove_dir_all(&pending_dir);
        return Ok(());
    }
    if let Some(parent) = final_dir.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create quant artifact dir {}", parent.display()))?;
    }
    let _ = std::fs::remove_dir_all(final_dir);
    std::fs::rename(&pending_dir, final_dir).with_context(|| {
        format!(
            "failed to finalize quant artifact dir {} -> {}",
            pending_dir.display(),
            final_dir.display()
        )
    })?;
    Ok(())
}

fn cleanup_failed_chat_quant_artifact(root: &Path, role: ManagedBackendRole) -> Result<()> {
    if role != ManagedBackendRole::Chat {
        return Ok(());
    }
    let Some(plan) = runtime_plan::load_persisted_chat_runtime_plan(root)? else {
        return Ok(());
    };
    if let Some(pending_dir) = pending_chat_quant_artifact_dir(root, &plan) {
        let _ = std::fs::remove_dir_all(pending_dir);
    }
    Ok(())
}

fn build_chat_quant_artifact(
    root: &Path,
    plan: &runtime_plan::ChatRuntimePlan,
    runtime_isq: &str,
    artifact_path: &Path,
) -> Result<()> {
    let pending_dir = pending_chat_quant_artifact_dir(root, plan);
    let pending_path = pending_chat_quant_artifact_path(root, plan);
    if let Some(dir) = pending_dir.as_deref() {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create quant artifact dir {}", dir.display()))?;
    } else if let Some(parent) = artifact_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create quant artifact dir {}", parent.display()))?;
    }
    let output_path = pending_path.as_deref().unwrap_or(artifact_path);
    let log_name = format!(
        "quantize_{}_{}.log",
        plan.model
            .chars()
            .map(|ch| match ch {
                'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
                _ => '_',
            })
            .collect::<String>(),
        runtime_isq.to_ascii_lowercase()
    );
    let log_path = root.join("runtime").join(log_name);
    let log_file = open_log_file(&log_path)?;
    let log_file_err = log_file
        .try_clone()
        .with_context(|| format!("failed to clone quant artifact log {}", log_path.display()))?;
    let engine_binary = engine::discover_source_layout_paths(root).model_runtime_binary;
    if !engine_binary.is_file() {
        anyhow::bail!(
            "ctox-engine binary missing for quant artifact build: {}",
            engine_binary.display()
        );
    }
    let mut command = Command::new(&engine_binary);
    command
        .arg("quantize")
        .arg("auto")
        .arg("-m")
        .arg(&plan.model)
        .arg("--isq")
        .arg(runtime_isq)
        .arg("--output")
        .arg(output_path)
        .arg("--max-seq-len")
        .arg(plan.max_seq_len.to_string())
        .arg("--max-batch-size")
        .arg(plan.max_batch_size.to_string())
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err));
    apply_managed_backend_bootstrap_env(&mut command);
    if !plan.cuda_visible_devices.trim().is_empty() {
        command.env("CUDA_VISIBLE_DEVICES", plan.cuda_visible_devices.as_str());
    }
    if plan.disable_nccl {
        command.env("ENGINE_NO_NCCL", "1");
    }
    if plan.disable_flash_attn {
        command.env("ENGINE_DISABLE_FLASH_ATTN", "1");
    }
    if plan.force_no_mmap {
        command.env("ENGINE_NO_MMAP", "1");
    }
    let parallel_immediate_isq = ambient_env_flag_enabled("CTOX_ENGINE_PARALLEL_IMMEDIATE_ISQ");
    if plan.isq_singlethread && !parallel_immediate_isq {
        command.env("ENGINE_ISQ_SINGLETHREAD", "1");
    }
    if let Some(cpu_threads) = plan.isq_cpu_threads {
        command.env("ENGINE_ISQ_CPU_THREADS", cpu_threads.to_string());
    }
    if parallel_immediate_isq {
        command.env("ENGINE_PARALLEL_IMMEDIATE_ISQ", "1");
    }
    if let Some(device_layers) = &plan.device_layers {
        command.arg("-n").arg(device_layers);
    }
    if let Some(topology) = &plan.topology {
        let topology_path = if Path::new(topology).is_absolute() {
            PathBuf::from(topology)
        } else {
            root.join(topology)
        };
        command.arg("--topology").arg(topology_path);
    }
    configure_managed_child_process(&mut command);
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start quant artifact build for {}", plan.model))?;
    let status = child.wait().with_context(|| {
        format!(
            "failed to wait for quant artifact build for {} (pid {})",
            plan.model,
            child.id()
        )
    })?;
    if status.success() {
        let _ = finalize_chat_quant_artifact(root, ManagedBackendRole::Chat);
        Ok(())
    } else {
        let _ = cleanup_failed_chat_quant_artifact(root, ManagedBackendRole::Chat);
        let _ = std::fs::remove_file(artifact_path);
        anyhow::bail!(
            "quant artifact build for {} exited with status {} (log: {})",
            plan.model,
            status,
            log_path.display()
        );
    }
}

fn wait_for_backend_ready(
    root: &Path,
    role: ManagedBackendRole,
    spec: &ManagedBackendSpec,
    descriptor: &runtime_gpu_manager::RuntimeWorkloadDescriptor,
    admission: &runtime_gpu_manager::GpuAdmission,
    pid_path: &Path,
    log_path: &Path,
    mut child: Option<&mut Child>,
) -> Result<()> {
    let startup_wait_secs = backend_startup_wait_secs_for_spec(role, spec);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(startup_wait_secs) {
        if let Some(child_handle) = child.as_deref_mut() {
            if let Some(status) = child_handle.try_wait().with_context(|| {
                format!(
                    "failed to poll {} backend child for {}",
                    role.as_env_value(),
                    spec.display_model
                )
            })? {
                let detail = managed_backend_failure_detail(log_path, Some(status));
                anyhow::bail!(
                    "{} backend for {} exited before becoming ready{}",
                    role.as_env_value(),
                    spec.display_model,
                    detail
                        .map(|value| format!(" ({value})"))
                        .unwrap_or_default()
                );
            }
        }
        if let Some(matched_pid) = ready_backend_process_pid(root, spec, pid_path)? {
            std::fs::write(pid_path, format!("{matched_pid}\n")).with_context(|| {
                format!("failed to write backend pid file {}", pid_path.display())
            })?;
            let _ = stop_duplicate_socket_backed_backend_processes(root, spec, Some(matched_pid));
            runtime_gpu_manager::sync_workload_runtime_residency(
                root,
                descriptor,
                admission,
                Some(matched_pid),
                runtime_contract::RuntimeResidencyPhase::Active,
            )?;
            return Ok(());
        }
        if let Some(pid) = read_pid(pid_path) {
            if !process_is_alive(pid) {
                let detail = managed_backend_failure_detail(log_path, None);
                anyhow::bail!(
                    "{} backend for {} exited before becoming ready{}",
                    role.as_env_value(),
                    spec.display_model,
                    detail
                        .map(|value| format!(" ({value})"))
                        .unwrap_or_default()
                );
            }
        }
        thread::sleep(Duration::from_millis(250));
    }

    anyhow::bail!(
        "{} backend for {} did not become ready within {}s",
        role.as_env_value(),
        spec.display_model,
        startup_wait_secs
    )
}

fn clear_failed_local_chat_runtime_projection(root: &Path, role: ManagedBackendRole) -> Result<()> {
    if role != ManagedBackendRole::Chat {
        return Ok(());
    }
    runtime_plan::store_persisted_chat_runtime_plan(root, None)?;
    runtime_plan::store_persisted_runtime_fleet_plan(root, None)?;

    let mut env_map = runtime_env::load_runtime_env_map(root).unwrap_or_default();
    runtime_plan::clear_chat_plan_env(&mut env_map);
    match runtime_state::load_or_resolve_runtime_state(root) {
        Ok(mut state) => {
            if state.source == runtime_state::InferenceSource::Local {
                state.engine_model = None;
                state.realized_context_tokens = None;
            }
            runtime_env::save_runtime_state_projection(root, &state, &env_map)?;
        }
        Err(_) => {
            runtime_env::save_runtime_env_map(root, &env_map)?;
        }
    }
    Ok(())
}

fn managed_backend_failure_detail(
    log_path: &Path,
    exit_status: Option<ExitStatus>,
) -> Option<String> {
    let exit_status_detail = exit_status.map(|status| status.to_string());
    let raw = std::fs::read_to_string(log_path).ok()?;
    let lines = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(3)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return Some(match exit_status_detail {
            Some(status) => format!("{status}; inspect {}", log_path.display()),
            None => format!("inspect {}", log_path.display()),
        });
    }
    let mut recent = lines;
    recent.reverse();
    match exit_status_detail {
        Some(status) => Some(format!("{status}; recent log: {}", recent.join(" | "))),
        None => Some(format!("recent log: {}", recent.join(" | "))),
    }
}

fn ready_backend_process_pid(
    root: &Path,
    spec: &ManagedBackendSpec,
    pid_path: &Path,
) -> Result<Option<u32>> {
    if let Some(socket_path) = spec.socket_path.as_deref() {
        let socket_path = Path::new(socket_path);
        if !socket_listener_accepts_stably(
            socket_path,
            BACKEND_READY_STABILITY_PASSES,
            Duration::from_millis(BACKEND_READY_STABILITY_POLL_MILLIS),
        ) {
            return Ok(None);
        }
        if let Some(pid) = read_pid(pid_path).filter(|pid| process_is_alive(*pid)) {
            if socket_backed_process_matches_spec(root, spec, pid)? {
                return Ok(Some(pid));
            }
        }
        return matching_socket_pid_for_backend(root, spec);
    }
    let health_url = format!("http://127.0.0.1:{}{}", spec.port, spec.health_path);
    if !health_check(&health_url) {
        return Ok(None);
    }
    matching_listener_pid_for_backend(root, spec.port, spec)
}

fn backend_process_owns_ready_transport(
    root: &Path,
    spec: &ManagedBackendSpec,
    pid: u32,
) -> Result<bool> {
    if !process_is_alive(pid) {
        return Ok(false);
    }
    if let Some(socket_path) = spec.socket_path.as_deref() {
        let socket_path = Path::new(socket_path);
        if socket_listener_accepts_stably(
            socket_path,
            BACKEND_READY_STABILITY_PASSES,
            Duration::from_millis(BACKEND_READY_STABILITY_POLL_MILLIS),
        ) {
            return socket_backed_process_matches_spec(root, spec, pid);
        }
        // A committed local socket backend can be busy serving a turn and briefly
        // reject new probe connects. Preserve the live workload as long as the
        // process still matches the expected launch spec and the socket file is
        // still present.
        if socket_path.exists() && socket_backed_process_matches_spec(root, spec, pid)? {
            return Ok(true);
        }
        return Ok(false);
    }
    Ok(listening_pids_for_port(root, spec.port)?.contains(&pid))
}

fn backend_process_matches_launch_spec(
    root: &Path,
    spec: &ManagedBackendSpec,
    pid: u32,
) -> Result<bool> {
    if !process_is_alive(pid) {
        return Ok(false);
    }
    if spec.socket_path.is_some() {
        return socket_backed_process_matches_spec(root, spec, pid);
    }

    let Some(command) = process_command(root, pid)? else {
        return Ok(false);
    };
    if command.contains(MANAGED_ENGINE_FROM_CONFIG_COMMAND) {
        let expected = managed_engine_runtime_config_path(root, spec);
        return Ok(command.contains(expected.display().to_string().as_str()));
    }
    if managed_litert_process_command(&command) {
        let expected = managed_litert_runtime_config_path(root, spec);
        return Ok(command.contains(expected.display().to_string().as_str()));
    }
    let expected_short_port = format!("-p {}", spec.port);
    let expected_long_port = format!("--port {}", spec.port);
    if !command.contains(&expected_short_port) && !command.contains(&expected_long_port) {
        return Ok(false);
    }
    if spec.launcher_kind == ManagedLauncherKind::Engine {
        return Ok(command.contains(spec.request_model.as_str()));
    }
    Ok(true)
}

fn socket_backed_process_matches_spec(
    root: &Path,
    spec: &ManagedBackendSpec,
    pid: u32,
) -> Result<bool> {
    let Some(socket_path) = spec.socket_path.as_deref() else {
        return Ok(false);
    };
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .current_dir(root)
        .output()
        .context("failed to inspect socket-backed backend process")?;
    if !output.status.success() {
        return Ok(false);
    }
    let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if command.is_empty() {
        return Ok(false);
    }
    if command.contains(MANAGED_ENGINE_FROM_CONFIG_COMMAND) {
        let expected = managed_engine_runtime_config_path(root, spec);
        return Ok(command.contains(expected.display().to_string().as_str()));
    }
    if managed_litert_process_command(&command) {
        let expected = managed_litert_runtime_config_path(root, spec);
        return Ok(command.contains(expected.display().to_string().as_str()));
    }
    if !command.contains(socket_path) {
        return Ok(false);
    }
    if spec.launcher_kind == ManagedLauncherKind::Engine
        && !command.contains(spec.request_model.as_str())
    {
        return Ok(false);
    }
    Ok(true)
}

fn matching_socket_pid_for_backend(root: &Path, spec: &ManagedBackendSpec) -> Result<Option<u32>> {
    Ok(matching_socket_pids_for_backend(root, spec)?
        .into_iter()
        .next())
}

fn matching_socket_pids_for_backend(root: &Path, spec: &ManagedBackendSpec) -> Result<Vec<u32>> {
    let Some(socket_path) = spec.socket_path.as_deref() else {
        return Ok(Vec::new());
    };
    let output = Command::new("ps")
        .args(["-axo", "pid=,command="])
        .current_dir(root)
        .output()
        .context("failed to inspect running processes for socket-backed backend")?;
    if !output.status.success() {
        anyhow::bail!("failed to inspect running processes for socket-backed backend");
    }
    let mut pids = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let Some(pid_raw) = parts.next() else {
            continue;
        };
        let Some(command) = parts.next() else {
            continue;
        };
        let Ok(pid) = pid_raw.trim().parse::<u32>() else {
            continue;
        };
        if command.contains(MANAGED_ENGINE_FROM_CONFIG_COMMAND) {
            let expected = managed_engine_runtime_config_path(root, spec);
            if command.contains(expected.display().to_string().as_str()) && process_is_alive(pid) {
                pids.push(pid);
            }
            continue;
        }
        if !command.contains(socket_path) {
            continue;
        }
        if spec.launcher_kind == ManagedLauncherKind::Engine
            && !command.contains(spec.request_model.as_str())
        {
            continue;
        }
        if process_is_alive(pid) {
            pids.push(pid);
        }
    }
    pids.sort_unstable();
    pids.dedup();
    Ok(pids)
}

fn stop_duplicate_socket_backed_backend_processes(
    root: &Path,
    spec: &ManagedBackendSpec,
    preserve_pid: Option<u32>,
) -> Result<()> {
    for pid in matching_socket_pids_for_backend(root, spec)? {
        if Some(pid) == preserve_pid || pid == std::process::id() {
            continue;
        }
        terminate_managed_process(root, pid)
            .with_context(|| format!("failed to stop duplicate socket-backed pid {pid}"))?;
        thread::sleep(Duration::from_millis(150));
        if process_is_alive(pid) {
            force_kill_managed_process(root, pid).with_context(|| {
                format!("failed to force-stop duplicate socket-backed pid {pid}")
            })?;
        }
    }
    Ok(())
}

fn backend_startup_in_progress(root: &Path, port: u16) -> bool {
    let path = backend_startup_lock_path(root, port);
    path.exists() && !lock_file_is_stale(root, &path)
}

#[cfg(unix)]
fn socket_listener_accepts(path: &Path) -> bool {
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
    true
}

const BACKEND_READY_STABILITY_PASSES: usize = 3;
const BACKEND_READY_STABILITY_POLL_MILLIS: u64 = 200;

const MANAGED_BACKEND_BOOTSTRAP_ENV_KEYS: &[&str] = &[
    "CODEX_HOME",
    "CTOX_ENV",
    "CTOX_CUDA_HOME",
    "OPENAI_API_KEY",
    "OPENROUTER_API_KEY",
    "HF_TOKEN",
    "HF_HOME",
    "XDG_CACHE_HOME",
    "HOME",
    "PATH",
    "LD_LIBRARY_PATH",
    "LIBRARY_PATH",
    "CPATH",
    "CPLUS_INCLUDE_PATH",
    "CUDARC_CUDA_VERSION",
    "CUDA_HOME",
    "CUDA_PATH",
    "CUDA_ROOT",
    "CUDA_TOOLKIT_ROOT_DIR",
    "CUDA_BIN_PATH",
    "NVCC",
    "CUDACXX",
];

const MANAGED_CHAT_RUNTIME_STATE_KEYS: &[&str] = &[
    "CTOX_CHAT_SOURCE",
    "CTOX_CHAT_MODEL_BASE",
    "CTOX_CHAT_MODEL",
    "CTOX_ACTIVE_MODEL",
    "CTOX_ENGINE_MODEL",
    "CTOX_ENGINE_PORT",
    "CTOX_ENGINE_REALIZED_MODEL",
    "CTOX_ENGINE_REALIZED_MAX_SEQ_LEN",
    "CTOX_CHAT_MODEL_REALIZED_CONTEXT",
    "CTOX_CHAT_MODEL_MAX_CONTEXT",
    "CTOX_UPSTREAM_BASE_URL",
    "CTOX_CHAT_LOCAL_PRESET",
];

const SHARED_MANAGED_BACKEND_OVERRIDE_KEYS: &[&str] = &[
    "CTOX_ENGINE_FEATURES",
    "CTOX_ENGINE_BINARY",
    "CTOX_ENGINE_LOG",
    "CTOX_ENGINE_ARCH",
];

/// Keys where the plan is authoritative and engine.env overrides are rejected.
/// These control critical inference behavior that must match the planner's
/// memory model and hardware analysis — silent overrides cause OOM or
/// silent performance degradation.
const PLAN_AUTHORITATIVE_KEYS: &[&str] = &[
    "CTOX_ENGINE_DISABLE_FLASH_ATTN",
    "CTOX_ENGINE_DISABLE_NCCL",
    "CTOX_ENGINE_TENSOR_PARALLEL_BACKEND",
    "CTOX_ENGINE_MN_LOCAL_WORLD_SIZE",
    "CTOX_ENGINE_PA_CACHE_TYPE",
];

const CHAT_MANAGED_BACKEND_OVERRIDE_KEYS: &[&str] = &[
    "CTOX_ENGINE_ISQ",
    "CTOX_ENGINE_PAGED_ATTN",
    "CTOX_ENGINE_PA_CACHE_TYPE",
    "CTOX_ENGINE_PA_MEMORY_FRACTION",
    "CTOX_ENGINE_PA_CONTEXT_LEN",
    "CTOX_ENGINE_DISABLE_NCCL",
    "CTOX_ENGINE_TENSOR_PARALLEL_BACKEND",
    "CTOX_ENGINE_MN_LOCAL_WORLD_SIZE",
    "CTOX_ENGINE_MAX_BATCH_SIZE",
    "CTOX_ENGINE_MAX_SEQS",
    "CTOX_ENGINE_MAX_SEQ_LEN",
    "CTOX_ENGINE_DEVICE_LAYERS",
    "CTOX_ENGINE_NUM_DEVICE_LAYERS",
    "CTOX_ENGINE_TOPOLOGY",
    "CTOX_ENGINE_ALLOW_DEVICE_LAYERS_WITH_TOPOLOGY",
    "CTOX_ENGINE_NM_DEVICE_ORDINAL",
    "CTOX_ENGINE_BASE_DEVICE_ORDINAL",
    "CTOX_ENGINE_MOE_EXPERTS_BACKEND",
    "CTOX_ENGINE_DISABLE_FLASH_ATTN",
    "CTOX_ENGINE_NO_MMAP",
    "CTOX_ENGINE_LANGUAGE_MODEL_ONLY",
    "CTOX_ENGINE_ISQ_SINGLETHREAD",
    "CTOX_ENGINE_ISQ_CPU_THREADS",
    "CTOX_ENGINE_PARALLEL_IMMEDIATE_ISQ",
    "CTOX_CHAT_SHARE_AUXILIARY_GPUS",
    "CTOX_AUXILIARY_GPU_LAYER_RESERVATION_MAP",
    "CTOX_EMBEDDING_GPU_LAYER_RESERVATION",
    "CTOX_STT_GPU_LAYER_RESERVATION",
    "CTOX_TTS_GPU_LAYER_RESERVATION",
    "CTOX_AUXILIARY_CUDA_VISIBLE_DEVICES",
    "CTOX_EMBEDDING_CUDA_VISIBLE_DEVICES",
    "CTOX_STT_CUDA_VISIBLE_DEVICES",
    "CTOX_TTS_CUDA_VISIBLE_DEVICES",
];

const AUXILIARY_ROLE_OVERRIDE_SUFFIXES: &[&str] = &[
    "ISQ",
    "MAX_SEQS",
    "MAX_BATCH_SIZE",
    "MAX_SEQ_LEN",
    "PAGED_ATTN",
    "PA_CACHE_TYPE",
    "PA_MEMORY_FRACTION",
    "DISABLE_NCCL",
];

fn collect_managed_backend_env(
    root: &Path,
    role: ManagedBackendRole,
    spec: &ManagedBackendSpec,
    admission: &runtime_gpu_manager::GpuAdmission,
) -> Result<BTreeMap<String, String>> {
    let mut env_map = BTreeMap::new();
    if role == ManagedBackendRole::Chat {
        if let Ok(state) = runtime_state::load_or_resolve_runtime_state(root) {
            for key in MANAGED_CHAT_RUNTIME_STATE_KEYS {
                if let Some(value) = runtime_state::owned_runtime_env_value(&state, key)
                    .filter(|value| !value.trim().is_empty())
                {
                    env_map.insert((*key).to_string(), value);
                }
            }
        }
    }
    for key in SHARED_MANAGED_BACKEND_OVERRIDE_KEYS {
        insert_configured_launch_env(root, &mut env_map, key);
    }
    match role {
        ManagedBackendRole::Chat => {
            if let Some(plan) = runtime_plan::load_persisted_chat_runtime_plan(root)? {
                runtime_plan::apply_chat_runtime_plan_env(root, &plan, &mut env_map)?;
                configure_chat_quant_artifact_launch_env(root, &plan, &mut env_map)?;
            }
            for key in CHAT_MANAGED_BACKEND_OVERRIDE_KEYS {
                if PLAN_AUTHORITATIVE_KEYS.contains(key) {
                    continue;
                }
                let before = env_map.get(*key).cloned();
                insert_configured_launch_env(root, &mut env_map, key);
                let after = env_map.get(*key);
                if after.map(String::as_str) != before.as_deref() {
                    eprintln!(
                        "ctox engine env override: {key} changed from {:?} to {:?}",
                        before, after
                    );
                }
            }
            env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
            env_map.insert("CTOX_CHAT_MODEL".to_string(), spec.request_model.clone());
            env_map.insert("CTOX_ACTIVE_MODEL".to_string(), spec.request_model.clone());
            env_map.insert(
                "CTOX_CHAT_MODEL_BASE".to_string(),
                spec.request_model.clone(),
            );
        }
        ManagedBackendRole::Embedding => {
            apply_auxiliary_launch_overrides(root, &mut env_map, "EMBEDDING");
        }
        ManagedBackendRole::Stt => {
            apply_auxiliary_launch_overrides(root, &mut env_map, "STT");
        }
        ManagedBackendRole::Tts => {
            apply_auxiliary_launch_overrides(root, &mut env_map, "TTS");
        }
        ManagedBackendRole::Vision => {
            apply_auxiliary_launch_overrides(root, &mut env_map, "VISION");
        }
    }
    env_map.insert("CTOX_ENGINE_MODEL".to_string(), spec.request_model.clone());
    env_map.insert("CTOX_ENGINE_PORT".to_string(), spec.port.to_string());
    if let Some(visible_devices) = admission
        .visible_devices
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        env_map.insert(
            "CTOX_ENGINE_CUDA_VISIBLE_DEVICES".to_string(),
            visible_devices.clone(),
        );
    }
    Ok(env_map)
}

fn insert_configured_launch_env(root: &Path, env_map: &mut BTreeMap<String, String>, key: &str) {
    if let Some(value) = inherited_env_value(key)
        .or_else(|| runtime_env::env_or_config(root, key))
        .filter(|value| !value.trim().is_empty())
    {
        env_map.insert(key.to_string(), value);
    }
}

fn apply_auxiliary_launch_overrides(
    root: &Path,
    env_map: &mut BTreeMap<String, String>,
    role_prefix: &str,
) {
    for suffix in AUXILIARY_ROLE_OVERRIDE_SUFFIXES {
        insert_configured_launch_env(root, env_map, &format!("CTOX_{role_prefix}_{suffix}"));
    }
}

fn apply_clean_child_env(command: &mut Command) {
    for (key, _) in std::env::vars() {
        if key.starts_with("CTOX_")
            || key.starts_with("ENGINE_")
            || key == "CODEX_HOME"
            || key == "CUDA_VISIBLE_DEVICES"
            || key == "SPEACHES_BASE_URL"
        {
            command.env_remove(key);
        }
    }
}

fn inherited_env_value(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn matching_listener_pid_for_backend(
    root: &Path,
    port: u16,
    spec: &ManagedBackendSpec,
) -> Result<Option<u32>> {
    let expected_short_port = format!("-p {port}");
    let expected_long_port = format!("--port {port}");
    for pid in listening_pids_for_port(root, port)? {
        let Some(command) = process_command(root, pid)? else {
            continue;
        };
        if command.contains(MANAGED_ENGINE_FROM_CONFIG_COMMAND) {
            let expected = managed_engine_runtime_config_path(root, spec);
            if command.contains(expected.display().to_string().as_str()) {
                return Ok(Some(pid));
            }
            continue;
        }
        if !command.contains(&expected_short_port) && !command.contains(&expected_long_port) {
            continue;
        }
        if command.contains(spec.request_model.as_str()) {
            return Ok(Some(pid));
        }
    }
    Ok(None)
}

fn listening_pids_for_port(root: &Path, port: u16) -> Result<Vec<u32>> {
    let output = match Command::new("fuser")
        .arg(format!("{port}/tcp"))
        .stderr(Stdio::null())
        .current_dir(root)
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to query listener pid for tcp/{port}"));
        }
    };
    if !output.status.success() && output.status.code() != Some(1) {
        anyhow::bail!("failed to query listener pid for tcp/{port}");
    }
    let mut pids = output
        .stdout
        .split(|byte| byte.is_ascii_whitespace())
        .filter_map(|chunk| std::str::from_utf8(chunk).ok())
        .filter_map(|chunk| chunk.trim().parse::<u32>().ok())
        .collect::<Vec<_>>();
    pids.sort_unstable();
    pids.dedup();
    Ok(pids)
}

fn process_command(root: &Path, pid: u32) -> Result<Option<String>> {
    let output = Command::new("ps")
        .args(["-ww", "-o", "command=", "-p", &pid.to_string()])
        .current_dir(root)
        .output()
        .with_context(|| format!("failed to inspect command for pid {pid}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if command.is_empty() {
        return Ok(None);
    }
    Ok(Some(command))
}

fn stop_processes_on_port(root: &Path, port: u16) -> Result<()> {
    for pid in listening_pids_for_port(root, port)? {
        if pid == std::process::id() {
            continue;
        }
        terminate_managed_process(root, pid)
            .with_context(|| format!("failed to stop listener pid {pid} on tcp/{port}"))?;
        thread::sleep(Duration::from_millis(150));
        if listening_pids_for_port(root, port)?.contains(&pid) {
            force_kill_managed_process(root, pid).with_context(|| {
                format!("failed to force-stop listener pid {pid} on tcp/{port}")
            })?;
        }
    }
    Ok(())
}

fn health_check(url: &str) -> bool {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(1))
        .timeout_read(Duration::from_secs(2))
        .timeout_write(Duration::from_secs(2))
        .build();
    match agent.get(url).call() {
        Ok(response) => response.status() < 500,
        Err(ureq::Error::Status(code, _)) => code < 500,
        Err(_) => false,
    }
}

fn proxy_pid_path(root: &Path) -> PathBuf {
    root.join("runtime/ctox_proxy.pid")
}

fn backend_pid_path(root: &Path, role: ManagedBackendRole) -> PathBuf {
    root.join("runtime").join(role.pid_file_name())
}

fn open_log_file(path: &Path) -> Result<File> {
    File::options()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open log file {}", path.display()))
}

fn stop_process(root: &Path, pid_path: PathBuf) -> Result<()> {
    let Some(pid) = read_pid(&pid_path) else {
        return Ok(());
    };
    if process_is_alive(pid) {
        terminate_managed_process(root, pid)
            .with_context(|| format!("failed to signal pid {pid}"))?;
        let deadline = Instant::now() + Duration::from_secs(3);
        while process_is_alive(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        if process_is_alive(pid) {
            if let Err(err) = force_kill_managed_process(root, pid) {
                if process_is_alive(pid) {
                    return Err(err).with_context(|| format!("failed to force-stop pid {pid}"));
                }
            }
            if process_is_alive(pid) {
                anyhow::bail!("failed to force-stop pid {pid}");
            }
        }
    }
    let _ = std::fs::remove_file(pid_path);
    Ok(())
}

#[cfg(unix)]
fn configure_managed_child_process(command: &mut Command) {
    configure_managed_child_process_with_parent_death(
        command,
        managed_child_parent_death_signal_enabled(),
    );
}

#[cfg(unix)]
fn configure_managed_child_process_with_parent_death(
    command: &mut Command,
    _propagate_parent_death: bool,
) {
    unsafe {
        command.pre_exec(move || {
            // Detach managed runtimes from transient operator shells/SSH sessions.
            // A plain process-group split is not enough; the backend must not keep
            // the caller's controlling session or it can die as soon as the
            // short-lived `ctox runtime switch` command exits remotely.
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            #[cfg(target_os = "linux")]
            if _propagate_parent_death {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            signal(SIGHUP, SIG_IGN);
            signal(SIGPIPE, SIG_IGN);
            let mut current = rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if getrlimit(RLIMIT_NOFILE, &mut current) == 0 {
                let target = 65_535 as libc::rlim_t;
                let raised = rlimit {
                    rlim_cur: std::cmp::min(target, current.rlim_max),
                    rlim_max: current.rlim_max,
                };
                let _ = setrlimit(RLIMIT_NOFILE, &raised);
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn configure_managed_child_process(_command: &mut Command) {}

#[cfg(not(unix))]
fn configure_managed_child_process_with_parent_death(
    _command: &mut Command,
    _propagate_parent_death: bool,
) {
}

#[cfg(unix)]
fn managed_child_parent_death_signal_enabled() -> bool {
    std::env::args()
        .nth(1)
        .map(|value| value == "service")
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn managed_child_parent_death_signal_enabled() -> bool {
    false
}

fn terminate_managed_process(root: &Path, pid: u32) -> Result<()> {
    if let Some(launcher_pid) = managed_launcher_ancestor_pid(root, pid)? {
        if launcher_pid != pid {
            return terminate_process_or_group(root, launcher_pid, "TERM");
        }
    }
    terminate_process_or_group(root, pid, "TERM")
}

fn force_kill_managed_process(root: &Path, pid: u32) -> Result<()> {
    if let Some(launcher_pid) = managed_launcher_ancestor_pid(root, pid)? {
        if launcher_pid != pid {
            return terminate_process_or_group(root, launcher_pid, "KILL");
        }
    }
    terminate_process_or_group(root, pid, "KILL")
}

fn terminate_process_or_group(root: &Path, pid: u32, signal_name: &str) -> Result<()> {
    if !signal_process_group(root, pid, signal_name)? {
        let status = Command::new("kill")
            .arg(format!("-{signal_name}"))
            .arg(pid.to_string())
            .current_dir(root)
            .status()
            .with_context(|| format!("failed to signal pid {pid}"))?;
        if !status.success() {
            anyhow::bail!("failed to signal pid {pid} with {signal_name}");
        }
    }
    Ok(())
}

fn managed_launcher_ancestor_pid(root: &Path, pid: u32) -> Result<Option<u32>> {
    const MAX_ANCESTOR_DEPTH: usize = 6;
    let mut current = pid;
    for _ in 0..MAX_ANCESTOR_DEPTH {
        let Some(parent_pid) = process_parent_pid(root, current)? else {
            return Ok(None);
        };
        let Some(command) = process_command(root, parent_pid)? else {
            current = parent_pid;
            continue;
        };
        if command_is_managed_runtime_launcher(&command) {
            return Ok(Some(parent_pid));
        }
        current = parent_pid;
    }
    Ok(None)
}

fn process_parent_pid(root: &Path, pid: u32) -> Result<Option<u32>> {
    let output = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .current_dir(root)
        .output()
        .with_context(|| format!("failed to inspect parent pid for {pid}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let parent_pid = raw.trim().parse::<u32>().ok();
    Ok(parent_pid.filter(|value| *value > 1))
}

fn process_group_id(root: &Path, pid: u32) -> Result<Option<u32>> {
    let output = Command::new("ps")
        .args(["-o", "pgid=", "-p", &pid.to_string()])
        .current_dir(root)
        .output()
        .with_context(|| format!("failed to inspect process group for {pid}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let process_group_id = raw.trim().parse::<u32>().ok();
    Ok(process_group_id.filter(|value| *value > 1))
}

#[cfg(unix)]
fn signal_process_group(root: &Path, pid: u32, signal_name: &str) -> Result<bool> {
    let Some(process_group_id) = process_group_id(root, pid)? else {
        return Ok(false);
    };
    signal_process_group_id(root, process_group_id, signal_name)
}

#[cfg(unix)]
fn signal_process_group_id(root: &Path, process_group_id: u32, signal_name: &str) -> Result<bool> {
    let status = Command::new("kill")
        .arg(format!("-{signal_name}"))
        .arg("--")
        .arg(format!("-{process_group_id}"))
        .current_dir(root)
        .status()
        .with_context(|| format!("failed to signal process group {process_group_id}"))?;
    Ok(status.success())
}

#[cfg(not(unix))]
fn signal_process_group(_root: &Path, _pid: u32, _signal_name: &str) -> Result<bool> {
    Ok(false)
}

#[cfg(not(unix))]
fn signal_process_group_id(
    _root: &Path,
    _process_group_id: u32,
    _signal_name: &str,
) -> Result<bool> {
    Ok(false)
}

fn kill_workspace_managed_runtime_groups(root: &Path) -> Result<()> {
    let groups = workspace_managed_runtime_groups(root)?;
    for group_id in groups {
        let _ = signal_process_group_id(root, group_id, "TERM");
    }
    for pid in workspace_managed_runtime_pids(root)? {
        let _ = terminate_managed_process(root, pid);
    }
    thread::sleep(Duration::from_millis(250));
    for group_id in workspace_managed_runtime_groups(root)? {
        let _ = signal_process_group_id(root, group_id, "KILL");
    }
    for pid in workspace_managed_runtime_pids(root)? {
        let _ = force_kill_managed_process(root, pid);
    }
    Ok(())
}

fn workspace_managed_runtime_groups(root: &Path) -> Result<Vec<u32>> {
    let processes = workspace_managed_runtime_processes(root)?;
    let mut groups = BTreeSet::new();
    for (_, group_id) in processes {
        groups.insert(group_id);
    }
    Ok(groups.into_iter().collect())
}

fn workspace_managed_runtime_pids(root: &Path) -> Result<Vec<u32>> {
    Ok(workspace_managed_runtime_processes(root)?
        .into_iter()
        .map(|(pid, _)| pid)
        .collect())
}

fn workspace_managed_runtime_processes(root: &Path) -> Result<Vec<(u32, u32)>> {
    let root_display = root.display().to_string();
    let current_pid = std::process::id();
    let current_group_id = process_group_id(root, current_pid).ok().flatten();
    let output = Command::new("ps")
        .args(["-axo", "pid=,pgid=,command="])
        .current_dir(root)
        .output()
        .context("failed to inspect managed runtime processes")?;
    if !output.status.success() {
        anyhow::bail!("failed to inspect managed runtime processes");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut processes = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(3, char::is_whitespace);
        let Some(_pid_raw) = parts.next() else {
            continue;
        };
        let Some(group_raw) = parts.next() else {
            continue;
        };
        let Some(command) = parts.next() else {
            continue;
        };
        let Ok(pid) = _pid_raw.trim().parse::<u32>() else {
            continue;
        };
        let Ok(group_id) = group_raw.trim().parse::<u32>() else {
            continue;
        };
        if group_id <= 1 {
            continue;
        }
        if pid == current_pid || current_group_id == Some(group_id) {
            continue;
        }
        if !command.contains(&root_display) && !process_current_dir_matches_root(pid, root) {
            continue;
        }
        if command_is_managed_runtime_launcher(command)
            || managed_engine_process_command(command)
            || managed_litert_process_command(command)
        {
            processes.push((pid, group_id));
        }
    }
    Ok(processes)
}

fn command_is_managed_runtime_launcher(command: &str) -> bool {
    command.contains(RUNTIME_SWITCH_COMMAND_FRAGMENT) || command.contains("serve-responses-proxy")
}

#[cfg(unix)]
fn process_current_dir_matches_root(pid: u32, root: &Path) -> bool {
    let Ok(process_cwd) = std::fs::read_link(format!("/proc/{pid}/cwd")) else {
        return false;
    };
    let Ok(root_canon) = root.canonicalize() else {
        return false;
    };
    let process_cwd = process_cwd.canonicalize().unwrap_or(process_cwd);
    process_cwd == root_canon
}

#[cfg(not(unix))]
fn process_current_dir_matches_root(_pid: u32, _root: &Path) -> bool {
    false
}

fn read_pid(path: &Path) -> Option<u32> {
    let raw = std::fs::read_to_string(path).ok()?;
    raw.trim().parse::<u32>().ok()
}

fn process_is_alive(pid: u32) -> bool {
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
    if process_is_zombie(pid) {
        reap_zombie_child(pid);
        return false;
    }
    true
}

fn process_is_zombie(pid: u32) -> bool {
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

#[cfg(unix)]
fn reap_zombie_child(pid: u32) {
    let mut status = 0;
    unsafe {
        let _ = libc::waitpid(pid as i32, &mut status, libc::WNOHANG);
    }
}

#[cfg(not(unix))]
fn reap_zombie_child(_pid: u32) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn production_source(source: &str) -> String {
        let mut production = String::new();
        let mut skip_next_item = false;
        let mut skipping_item = false;
        let mut item_started = false;
        let mut brace_depth = 0i32;

        for line in source.lines() {
            let trimmed = line.trim();
            if !skip_next_item && !skipping_item && trimmed == "#[cfg(test)]" {
                skip_next_item = true;
                continue;
            }
            if skip_next_item {
                skip_next_item = false;
                skipping_item = true;
            }
            if skipping_item {
                let open = line.matches('{').count() as i32;
                let close = line.matches('}').count() as i32;
                if open > 0 || close > 0 || trimmed.ends_with(';') {
                    item_started = true;
                }
                brace_depth += open - close;
                if item_started
                    && brace_depth <= 0
                    && (trimmed.ends_with(';') || open > 0 || close > 0)
                {
                    skipping_item = false;
                    item_started = false;
                    brace_depth = 0;
                }
                continue;
            }
            production.push_str(line);
            production.push('\n');
        }

        production
    }

    #[test]
    fn managed_runtime_ports_cover_core_backend_defaults_without_boundary_proxy() {
        let root = std::env::temp_dir().join(format!(
            "ctox-supervisor-managed-ports-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let ports = managed_runtime_ports(&root).unwrap();
        let port_set = ports.into_iter().collect::<BTreeSet<_>>();
        for expected in [1234_u16, 1235, 1236, 1237, 1238, 1239] {
            assert!(port_set.contains(&expected), "missing port {expected}");
        }
        assert!(!port_set.contains(&12434), "unexpected boundary proxy port");
    }

    #[test]
    fn managed_runtime_ports_include_boundary_proxy_only_when_managed() {
        let root = std::env::temp_dir().join(format!(
            "ctox-supervisor-managed-proxy-port-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        std::fs::write(proxy_pid_path(&root), format!("{}\n", std::process::id())).unwrap();

        let ports = managed_runtime_ports(&root).unwrap();
        let port_set = ports.into_iter().collect::<BTreeSet<_>>();

        assert!(
            port_set.contains(&12434),
            "missing managed boundary proxy port"
        );
    }

    #[test]
    fn api_runtime_does_not_keep_primary_generation_managed() {
        let root = temp_root("api-runtime-chat-disabled");
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "api".to_string());
        env_map.insert("CTOX_CHAT_MODEL".to_string(), "gpt-5.4".to_string());
        env_map.insert("CTOX_CHAT_MODEL_BASE".to_string(), "gpt-5.4".to_string());
        env_map.insert("CTOX_ACTIVE_MODEL".to_string(), "gpt-5.4".to_string());
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        assert!(!managed_backend_enabled(&root, ManagedBackendRole::Chat));
    }

    #[test]
    fn startup_lock_path_scan_returns_only_backend_lock_files() {
        let root = std::env::temp_dir().join(format!(
            "ctox-supervisor-lock-scan-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).unwrap();
        std::fs::write(runtime_dir.join("backend_startup_1235.lock"), "pid=1\n").unwrap();
        std::fs::write(runtime_dir.join("backend_startup_1237.lock"), "pid=2\n").unwrap();
        std::fs::write(runtime_dir.join("not_a_lock.txt"), "noop\n").unwrap();

        let scanned = backend_startup_lock_paths(&root).unwrap();
        let names = scanned
            .iter()
            .filter_map(|path| path.file_name().and_then(|value| value.to_str()))
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec!["backend_startup_1235.lock", "backend_startup_1237.lock"]
        );
    }

    fn temp_root(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ctox-supervisor-{label}-{unique}"));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        root
    }

    fn host_acceleration_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_host_acceleration_override<T>(value: &str, f: impl FnOnce() -> T) -> T {
        let _guard = host_acceleration_test_lock().lock().unwrap();
        let previous = std::env::var("CTOX_TEST_ENGINE_HOST_ACCELERATION").ok();
        std::env::set_var("CTOX_TEST_ENGINE_HOST_ACCELERATION", value);
        let result = f();
        if let Some(previous) = previous {
            std::env::set_var("CTOX_TEST_ENGINE_HOST_ACCELERATION", previous);
        } else {
            std::env::remove_var("CTOX_TEST_ENGINE_HOST_ACCELERATION");
        }
        result
    }

    fn sample_chat_plan(model: &str) -> runtime_plan::ChatRuntimePlan {
        runtime_plan::ChatRuntimePlan {
            model: model.to_string(),
            preset: runtime_plan::ChatPreset::Quality,
            quantization: "q4".to_string(),
            runtime_isq: Some("Q4K".to_string()),
            max_seq_len: 131_072,
            compaction_threshold_percent: 80,
            compaction_min_tokens: 4096,
            min_context_floor_applied: false,
            paged_attn: "on".to_string(),
            pa_cache_type: None,
            pa_memory_fraction: None,
            pa_context_len: None,
            disable_nccl: false,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 4,
            max_seqs: 4,
            cuda_visible_devices: "0,1".to_string(),
            device_layers: None,
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: None,
            base_device_ordinal: None,
            moe_experts_backend: None,
            disable_flash_attn: false,
            force_no_mmap: false,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            isq_singlethread: false,
            isq_cpu_threads: None,
            expected_tok_s: 42.0,
            hardware_fingerprint: "test".to_string(),
            theoretical_breakdown: runtime_plan::TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 1,
                kv_budget_cap_mb: 1,
                kv_budget_fraction_milli: 1,
                weight_residency_mb: 1,
                kv_cache_mb: 1,
                fixed_runtime_base_overhead_mb: 1,
                backend_runtime_overhead_mb: 1,
                activation_overhead_mb: 1,
                load_peak_overhead_mb: 1,
                safety_headroom_mb: 1,
                required_effective_total_budget_mb: 1,
                required_total_mb: 1,
            },
            rationale: vec!["platform contract ok".to_string()],
            gpu_allocations: vec![runtime_plan::PlannedGpuAllocation {
                gpu_index: 0,
                name: "gpu0".to_string(),
                total_mb: 1,
                desktop_reserve_mb: 0,
                aux_reserve_mb: 0,
                chat_budget_mb: 1,
                backend_overhead_mb: 0,
                activation_overhead_mb: 0,
                load_peak_overhead_mb: 0,
                repeating_weight_mb: 0,
                weight_mb: 0,
                kv_cache_mb: 0,
                free_headroom_mb: 0,
                chat_enabled: true,
            }],
        }
    }

    fn persist_chat_plan(root: &Path, plan: &runtime_plan::ChatRuntimePlan) {
        std::fs::write(
            root.join("runtime/chat_plan.json"),
            serde_json::to_vec_pretty(plan).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn managed_chat_launch_env_uses_plan_and_filters_unrelated_flags() {
        let root = temp_root("launch-env-chat");
        runtime_state::persist_runtime_state(
            &root,
            &runtime_state::InferenceRuntimeState {
                version: 4,
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
                upstream_base_url: "http://127.0.0.1:1234".to_string(),
                local_preset: Some("Quality".to_string()),
                boost: runtime_state::BoostRuntimeState::default(),
                adapter_tuning: runtime_state::AdapterRuntimeTuning::default(),
                embedding: runtime_state::AuxiliaryRuntimeState::default(),
                transcription: runtime_state::AuxiliaryRuntimeState::default(),
                speech: runtime_state::AuxiliaryRuntimeState::default(),
                vision: runtime_state::AuxiliaryRuntimeState::default(),
            },
        )
        .unwrap();
        std::fs::write(
            root.join("runtime/engine.env"),
            "CTOX_ENGINE_BINARY=/tmp/ctox-engine\nCTOX_ENGINE_LOG=/tmp/ctox-engine.log\nCTOX_CHAT_SHARE_AUXILIARY_GPUS=0\nCTOX_PROXY_PORT=9999\nCTOX_SHOULD_NOT_LEAK=1\n",
        )
        .unwrap();
        persist_chat_plan(&root, &sample_chat_plan("openai/gpt-oss-20b"));
        let spec = ManagedBackendSpec {
            display_model: "openai/gpt-oss-20b".to_string(),
            request_model: "openai/gpt-oss-20b".to_string(),
            port: 1234,
            socket_path: None,
            health_path: "/health",
            launcher_kind: ManagedLauncherKind::Engine,
            compute_target: None,
        };
        let env = collect_managed_backend_env(
            &root,
            ManagedBackendRole::Chat,
            &spec,
            &runtime_gpu_manager::GpuAdmission {
                visible_devices: Some("0,1".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            env.get("CTOX_CHAT_RUNTIME_PLAN_ACTIVE"),
            Some(&"1".to_string())
        );
        assert_eq!(
            env.get("CTOX_ENGINE_MODEL"),
            Some(&"openai/gpt-oss-20b".to_string())
        );
        assert_eq!(env.get("CTOX_ENGINE_PORT"), Some(&"1234".to_string()));
        assert_eq!(
            env.get("CTOX_ENGINE_CUDA_VISIBLE_DEVICES"),
            Some(&"0,1".to_string())
        );
        assert_eq!(
            env.get("CTOX_ENGINE_BINARY"),
            Some(&"/tmp/ctox-engine".to_string())
        );
        assert_eq!(
            env.get("CTOX_ENGINE_LOG"),
            Some(&"/tmp/ctox-engine.log".to_string())
        );
        assert!(!env.contains_key("CTOX_CHAT_SHARE_AUXILIARY_GPUS"));
        assert!(!env.contains_key("CTOX_PROXY_PORT"));
        assert!(!env.contains_key("CTOX_SHOULD_NOT_LEAK"));
    }

    #[test]
    fn managed_aux_launch_env_keeps_role_overrides_only() {
        let root = temp_root("launch-env-aux");
        std::fs::write(
            root.join("runtime/engine.env"),
            "CTOX_ENGINE_BINARY=/tmp/ctox-engine\nCTOX_ENGINE_LOG=/tmp/ctox-engine.log\nCTOX_STT_MAX_SEQ_LEN=4096\nCTOX_PROXY_PORT=9999\nCTOX_SHOULD_NOT_LEAK=1\n",
        )
        .unwrap();
        let spec = ManagedBackendSpec {
            display_model: "engineai/Voxtral-Mini-4B-Realtime-2602".to_string(),
            request_model: "engineai/Voxtral-Mini-4B-Realtime-2602".to_string(),
            port: 1238,
            socket_path: None,
            health_path: "/health",
            launcher_kind: ManagedLauncherKind::Engine,
            compute_target: Some(engine::ComputeTarget::Gpu),
        };
        let env = collect_managed_backend_env(
            &root,
            ManagedBackendRole::Stt,
            &spec,
            &runtime_gpu_manager::GpuAdmission {
                visible_devices: Some("2".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            env.get("CTOX_ENGINE_MODEL"),
            Some(&"engineai/Voxtral-Mini-4B-Realtime-2602".to_string())
        );
        assert_eq!(env.get("CTOX_ENGINE_PORT"), Some(&"1238".to_string()));
        assert_eq!(env.get("CTOX_STT_MAX_SEQ_LEN"), Some(&"4096".to_string()));
        assert_eq!(
            env.get("CTOX_ENGINE_BINARY"),
            Some(&"/tmp/ctox-engine".to_string())
        );
        assert_eq!(
            env.get("CTOX_ENGINE_LOG"),
            Some(&"/tmp/ctox-engine.log".to_string())
        );
        assert_eq!(
            env.get("CTOX_ENGINE_CUDA_VISIBLE_DEVICES"),
            Some(&"2".to_string())
        );
        assert!(!env.contains_key("CTOX_PROXY_PORT"));
        assert!(!env.contains_key("CTOX_SHOULD_NOT_LEAK"));
        assert!(!env.contains_key("CTOX_CHAT_RUNTIME_PLAN_ACTIVE"));
    }

    #[test]
    fn managed_backend_launch_spec_captures_typed_boundary_contract() {
        let root = temp_root("launch-spec-chat");
        persist_chat_plan(&root, &sample_chat_plan("openai/gpt-oss-20b"));
        let spec = ManagedBackendSpec {
            display_model: "openai/gpt-oss-20b".to_string(),
            request_model: "openai/gpt-oss-20b".to_string(),
            port: 1234,
            socket_path: Some("/tmp/ctox-primary.sock".to_string()),
            health_path: "/health",
            launcher_kind: ManagedLauncherKind::Engine,
            compute_target: None,
        };
        let launch_spec = build_managed_backend_launch_spec(
            &root,
            ManagedBackendRole::Chat,
            &spec,
            &runtime_gpu_manager::GpuAdmission {
                visible_devices: Some("0,1".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(launch_spec.version, 2);
        assert_eq!(launch_spec.role, "chat");
        assert_eq!(launch_spec.request_model, "openai/gpt-oss-20b");
        assert_eq!(launch_spec.port, 1234);
        assert_eq!(
            launch_spec.socket_path.as_deref(),
            Some("/tmp/ctox-primary.sock")
        );
        assert_eq!(launch_spec.launcher_kind, "engine");
        assert_eq!(launch_spec.visible_devices.as_deref(), Some("0,1"));
        assert_eq!(launch_spec.engine_config.max_seq_len, Some(131_072));
        assert_eq!(launch_spec.engine_config.isq.as_deref(), Some("Q4K"));
        assert!(launch_spec.litert_config.is_none());
    }

    #[test]
    fn managed_litert_launch_spec_rejects_unqualified_128k_artifacts() {
        let root = temp_root("launch-spec-litert");
        runtime_state::persist_runtime_state(
            &root,
            &runtime_state::InferenceRuntimeState {
                version: 7,
                source: runtime_state::InferenceSource::Local,
                local_runtime: runtime_state::LocalRuntimeKind::LiteRt,
                base_model: Some("google/gemma-4-E4B-it".to_string()),
                requested_model: Some("google/gemma-4-E4B-it".to_string()),
                active_model: Some("google/gemma-4-E4B-it".to_string()),
                engine_model: Some("google/gemma-4-E4B-it".to_string()),
                engine_port: Some(1235),
                realized_context_tokens: Some(131_072),
                proxy_host: "127.0.0.1".to_string(),
                proxy_port: 12434,
                upstream_base_url: "http://127.0.0.1:1235".to_string(),
                local_preset: None,
                boost: runtime_state::BoostRuntimeState::default(),
                adapter_tuning: runtime_state::AdapterRuntimeTuning::default(),
                embedding: runtime_state::AuxiliaryRuntimeState::default(),
                transcription: runtime_state::AuxiliaryRuntimeState::default(),
                speech: runtime_state::AuxiliaryRuntimeState::default(),
                vision: runtime_state::AuxiliaryRuntimeState::default(),
            },
        )
        .unwrap();
        let spec = ManagedBackendSpec {
            display_model: "google/gemma-4-E4B-it".to_string(),
            request_model: "google/gemma-4-E4B-it".to_string(),
            port: 1235,
            socket_path: Some("/tmp/ctox-primary.sock".to_string()),
            health_path: "/health",
            launcher_kind: ManagedLauncherKind::LiteRt,
            compute_target: Some(engine::ComputeTarget::Cpu),
        };
        let launch_spec = build_managed_backend_launch_spec(
            &root,
            ManagedBackendRole::Chat,
            &spec,
            &runtime_gpu_manager::GpuAdmission::default(),
        )
        .expect_err("current LiteRT Gemma4 artifacts are not qualified for 128k");
        assert!(
            launch_spec
                .to_string()
                .contains("only validated to 131072 tokens"),
            "unexpected error: {launch_spec:#}"
        );
    }

    #[test]
    fn e2b_default_litert_artifact_mapping_is_disabled_until_rebuilt() {
        assert!(default_litert_artifact_for_model("google/gemma-4-E2B-it").is_none());
        let artifact = default_litert_artifact_for_model("google/gemma-4-E4B-it")
            .expect("e4b mapping should remain active");
        assert_eq!(
            artifact.huggingface_repo,
            "metricspace/gemma4-E4B-it-litert-128k-mtp"
        );
    }

    #[test]
    fn managed_engine_runtime_config_renders_typed_worker_contract() {
        let root = temp_root("engine-command");
        let launch_spec = ManagedBackendLaunchSpec {
            version: 2,
            role: "chat".to_string(),
            display_model: "openai/gpt-oss-20b".to_string(),
            request_model: "openai/gpt-oss-20b".to_string(),
            port: 1234,
            socket_path: Some("/tmp/ctox-primary.sock".to_string()),
            health_path: "/health".to_string(),
            launcher_kind: "engine".to_string(),
            compute_target: Some("gpu".to_string()),
            visible_devices: Some("0,1".to_string()),
            engine_config: ManagedEngineLaunchConfig {
                log_path: Some("/tmp/ctox-engine.log".to_string()),
                arch: Some("gpt-oss".to_string()),
                isq: Some("Q4K".to_string()),
                max_seqs: Some(4),
                max_batch_size: Some(4),
                max_seq_len: Some(65_536),
                paged_attn: Some("on".to_string()),
                pa_cache_type: Some("turboquant3".to_string()),
                device_layers: Some("0:20;1:16".to_string()),
                ..Default::default()
            },
            litert_config: None,
        };

        let rendered = render_managed_engine_runtime_config(&launch_spec);
        assert!(rendered.contains("command = \"serve\""));
        assert!(rendered.contains("socket_path = \"/tmp/ctox-primary.sock\""));
        assert!(rendered.contains("model_id = \"openai/gpt-oss-20b\""));
        assert!(rendered.contains("arch = \"gpt-oss\""));
        assert!(rendered.contains("in_situ_quant = \"Q4K\""));
        assert!(rendered.contains("device_layers = [\"0:20\", \"1:16\"]"));
        assert!(rendered.contains("cache_type = \"turboquant3\""));
        assert!(!rendered.contains("run_engine.sh"));
        let config_path = persist_managed_engine_runtime_config(
            &root,
            &ManagedBackendSpec {
                display_model: launch_spec.display_model.clone(),
                request_model: launch_spec.request_model.clone(),
                port: launch_spec.port,
                socket_path: launch_spec.socket_path.clone(),
                health_path: "/health",
                launcher_kind: ManagedLauncherKind::Engine,
                compute_target: Some(engine::ComputeTarget::Gpu),
            },
            &launch_spec,
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(config_path).unwrap(), rendered);
    }

    #[test]
    fn managed_engine_runtime_config_forces_text_kind_for_language_model_only_chat() {
        let launch_spec = ManagedBackendLaunchSpec {
            version: 2,
            role: "chat".to_string(),
            display_model: "google/gemma-4-E2B-it".to_string(),
            request_model: "google/gemma-4-E2B-it".to_string(),
            port: 1234,
            socket_path: Some("/tmp/ctox-primary.sock".to_string()),
            health_path: "/health".to_string(),
            launcher_kind: "engine".to_string(),
            compute_target: Some("gpu".to_string()),
            visible_devices: Some("0,1,2".to_string()),
            engine_config: ManagedEngineLaunchConfig {
                language_model_only: true,
                max_seq_len: Some(32_000),
                ..Default::default()
            },
            litert_config: None,
        };

        let rendered = render_managed_engine_runtime_config(&launch_spec);
        assert!(rendered.contains("kind = \"text\""));
        assert!(!rendered.contains("kind = \"vision\""));
    }

    #[test]
    fn managed_engine_launch_rejects_cpu_only_binary_on_gpu_host() {
        let root = temp_root("engine-command-mismatch");
        let engine_binary = root.join("tools/model-runtime/target/release/ctox-engine");
        std::fs::create_dir_all(engine_binary.parent().unwrap()).unwrap();
        std::fs::write(
            &engine_binary,
            "#!/bin/sh\nif [ \"$1\" = \"doctor\" ] && [ \"$2\" = \"--json\" ]; then\n  printf '%s\\n' '{\"system\":{\"build\":{\"cuda\":false,\"metal\":false,\"cudnn\":false,\"nccl\":false,\"flash_attn\":false,\"flash_attn_v3\":false,\"accelerate\":false,\"mkl\":false}}}'\n  exit 0\nfi\nexit 0\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&engine_binary).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&engine_binary, perms).unwrap();
        }
        let launch_spec = ManagedBackendLaunchSpec {
            version: 2,
            role: "chat".to_string(),
            display_model: "openai/gpt-oss-20b".to_string(),
            request_model: "openai/gpt-oss-20b".to_string(),
            port: 1234,
            socket_path: Some("/tmp/ctox-primary.sock".to_string()),
            health_path: "/health".to_string(),
            launcher_kind: "engine".to_string(),
            compute_target: Some("gpu".to_string()),
            visible_devices: Some("0,1".to_string()),
            engine_config: ManagedEngineLaunchConfig {
                binary_path: Some(engine_binary.display().to_string()),
                ..Default::default()
            },
            litert_config: None,
        };

        with_host_acceleration_override("cuda", || {
            let err = resolve_managed_engine_binary(&root, &launch_spec)
                .unwrap_err()
                .to_string();
            assert!(err.contains("host requires nvidia-cuda support"));
            assert!(err.contains("cpu-only"));
        });
    }

    #[test]
    fn managed_engine_parallel_isq_override_suppresses_singlethread_env() {
        let root = temp_root("engine-command-parallel-isq");
        let engine_binary = root.join("tools/model-runtime/target/release/ctox-engine");
        std::fs::create_dir_all(engine_binary.parent().unwrap()).unwrap();
        std::fs::write(
            &engine_binary,
            "#!/bin/sh\nif [ \"$1\" = \"doctor\" ] && [ \"$2\" = \"--json\" ]; then\n  printf '%s\\n' '{\"system\":{\"build\":{\"cuda\":true,\"metal\":false,\"cudnn\":true,\"nccl\":true,\"flash_attn\":true,\"flash_attn_v3\":false,\"accelerate\":false,\"mkl\":false}}}'\n  exit 0\nfi\nexit 0\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&engine_binary).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&engine_binary, perms).unwrap();
        }

        let launch_spec = ManagedBackendLaunchSpec {
            version: 2,
            role: "chat".to_string(),
            display_model: "zai-org/GLM-4.7-Flash".to_string(),
            request_model: "zai-org/GLM-4.7-Flash".to_string(),
            port: 1234,
            socket_path: Some("/tmp/ctox-primary.sock".to_string()),
            health_path: "/health".to_string(),
            launcher_kind: "engine".to_string(),
            compute_target: Some("gpu".to_string()),
            visible_devices: Some("0,1,2,3".to_string()),
            engine_config: ManagedEngineLaunchConfig {
                binary_path: Some(engine_binary.display().to_string()),
                isq_singlethread: true,
                parallel_immediate_isq: true,
                isq_cpu_threads: Some(4),
                ..Default::default()
            },
            litert_config: None,
        };

        with_host_acceleration_override("cuda", || {
            let mut command = Command::new("env");
            apply_clean_child_env(&mut command);
            configure_managed_engine_runtime_env(&mut command, &launch_spec);
            let env_map: BTreeMap<_, _> = command
                .get_envs()
                .filter_map(|(key, value)| {
                    value.map(|value| {
                        (
                            key.to_string_lossy().into_owned(),
                            value.to_string_lossy().into_owned(),
                        )
                    })
                })
                .collect();

            assert_eq!(
                env_map
                    .get("ENGINE_PARALLEL_IMMEDIATE_ISQ")
                    .map(String::as_str),
                Some("1")
            );
            assert_eq!(
                env_map.get("ENGINE_ISQ_CPU_THREADS").map(String::as_str),
                Some("4")
            );
            assert!(!env_map.contains_key("ENGINE_ISQ_SINGLETHREAD"));
        });
    }

    #[test]
    fn collect_managed_backend_env_keeps_chat_overrides_when_plan_exists() {
        let root = temp_root("chat-env-overrides-with-plan");
        let plan = runtime_plan::ChatRuntimePlan {
            model: "zai-org/GLM-4.7-Flash".to_string(),
            preset: runtime_plan::ChatPreset::Quality,
            quantization: "Q4K".to_string(),
            runtime_isq: Some("Q4K".to_string()),
            max_seq_len: 65_536,
            compaction_threshold_percent: 75,
            compaction_min_tokens: 16_384,
            min_context_floor_applied: true,
            paged_attn: "auto".to_string(),
            pa_cache_type: Some("turboquant3".to_string()),
            pa_memory_fraction: Some("0.65".to_string()),
            pa_context_len: Some(65_536),
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 1,
            max_seqs: 1,
            cuda_visible_devices: "0,1,2,3".to_string(),
            device_layers: Some("0:11;1:12;2:12;3:12".to_string()),
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
            expected_tok_s: 30.0,
            hardware_fingerprint: "test-host".to_string(),
            theoretical_breakdown: runtime_plan::TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 80_000,
                kv_budget_cap_mb: 16_000,
                kv_budget_fraction_milli: 650,
                weight_residency_mb: 22_000,
                kv_cache_mb: 9_000,
                fixed_runtime_base_overhead_mb: 1_620,
                backend_runtime_overhead_mb: 0,
                activation_overhead_mb: 0,
                load_peak_overhead_mb: 2_232,
                safety_headroom_mb: 3_072,
                required_effective_total_budget_mb: 49_000,
                required_total_mb: 49_000,
            },
            rationale: vec!["test".to_string()],
            gpu_allocations: vec![],
        };
        runtime_plan::store_persisted_chat_runtime_plan(&root, Some(&plan)).unwrap();

        let spec = ManagedBackendSpec {
            display_model: plan.model.clone(),
            request_model: plan.model.clone(),
            port: 1234,
            socket_path: Some(root.join("runtime/primary.sock").display().to_string()),
            health_path: "/health",
            launcher_kind: ManagedLauncherKind::Engine,
            compute_target: None,
        };
        let admission = runtime_gpu_manager::GpuAdmission {
            visible_devices: Some("0,1,2,3".to_string()),
            ..Default::default()
        };

        let previous_parallel = std::env::var("CTOX_ENGINE_PARALLEL_IMMEDIATE_ISQ").ok();
        let previous_threads = std::env::var("CTOX_ENGINE_ISQ_CPU_THREADS").ok();
        std::env::set_var("CTOX_ENGINE_PARALLEL_IMMEDIATE_ISQ", "1");
        std::env::set_var("CTOX_ENGINE_ISQ_CPU_THREADS", "4");

        let env_map =
            collect_managed_backend_env(&root, ManagedBackendRole::Chat, &spec, &admission)
                .unwrap();

        if let Some(previous) = previous_parallel {
            std::env::set_var("CTOX_ENGINE_PARALLEL_IMMEDIATE_ISQ", previous);
        } else {
            std::env::remove_var("CTOX_ENGINE_PARALLEL_IMMEDIATE_ISQ");
        }
        if let Some(previous) = previous_threads {
            std::env::set_var("CTOX_ENGINE_ISQ_CPU_THREADS", previous);
        } else {
            std::env::remove_var("CTOX_ENGINE_ISQ_CPU_THREADS");
        }

        assert_eq!(
            env_map
                .get("CTOX_ENGINE_PARALLEL_IMMEDIATE_ISQ")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            env_map
                .get("CTOX_ENGINE_ISQ_CPU_THREADS")
                .map(String::as_str),
            Some("4")
        );
        assert_eq!(
            env_map
                .get("CTOX_ENGINE_ISQ_SINGLETHREAD")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(env_map.get("CTOX_ENGINE_WRITE_UQFF"), None);
    }

    #[test]
    fn finalize_chat_quant_artifact_promotes_pending_file() {
        let root = temp_root("finalize-chat-quant-artifact");
        let plan = runtime_plan::ChatRuntimePlan {
            model: "zai-org/GLM-4.7-Flash".to_string(),
            preset: runtime_plan::ChatPreset::Quality,
            quantization: "Q4K".to_string(),
            runtime_isq: Some("Q4K".to_string()),
            max_seq_len: 65_536,
            compaction_threshold_percent: 75,
            compaction_min_tokens: 16_384,
            min_context_floor_applied: true,
            paged_attn: "auto".to_string(),
            pa_cache_type: Some("turboquant3".to_string()),
            pa_memory_fraction: Some("0.65".to_string()),
            pa_context_len: Some(65_536),
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 1,
            max_seqs: 1,
            cuda_visible_devices: "0,1,2,3".to_string(),
            device_layers: Some("0:11;1:12;2:12;3:12".to_string()),
            topology: None,
            allow_device_layers_with_topology: false,
            nm_device_ordinal: Some(0),
            base_device_ordinal: Some(0),
            moe_experts_backend: None,
            disable_flash_attn: true,
            force_no_mmap: true,
            force_language_model_only: false,
            require_prebuilt_uqff_for_chat_start: false,
            isq_singlethread: true,
            isq_cpu_threads: None,
            expected_tok_s: 60.0,
            hardware_fingerprint: "test-host".to_string(),
            theoretical_breakdown: runtime_plan::TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 80_000,
                kv_budget_cap_mb: 16_000,
                kv_budget_fraction_milli: 650,
                weight_residency_mb: 22_000,
                kv_cache_mb: 9_000,
                fixed_runtime_base_overhead_mb: 1_620,
                backend_runtime_overhead_mb: 0,
                activation_overhead_mb: 0,
                load_peak_overhead_mb: 2_232,
                safety_headroom_mb: 3_072,
                required_effective_total_budget_mb: 49_000,
                required_total_mb: 49_000,
            },
            rationale: vec!["test".to_string()],
            gpu_allocations: vec![],
        };
        runtime_plan::store_persisted_chat_runtime_plan(&root, Some(&plan)).unwrap();
        let pending = pending_chat_quant_artifact_path(&root, &plan).unwrap();
        std::fs::create_dir_all(pending.parent().unwrap()).unwrap();
        let pending_first_shard = pending
            .file_stem()
            .map(|stem| {
                pending
                    .parent()
                    .unwrap()
                    .join(format!("{}-0.uqff", stem.to_string_lossy()))
            })
            .unwrap();
        std::fs::write(&pending_first_shard, b"uqff-ready").unwrap();
        std::fs::write(
            pending.parent().unwrap().join("residual.safetensors"),
            b"residual",
        )
        .unwrap();

        finalize_chat_quant_artifact(&root, ManagedBackendRole::Chat).unwrap();

        let final_path = runtime_plan::chat_quant_artifact_path(&root, &plan).unwrap();
        assert!(final_path.is_file());
        assert_eq!(std::fs::read(&final_path).unwrap(), b"uqff-ready");
        assert!(!pending.parent().unwrap().exists());
    }

    #[test]
    fn production_supervisor_avoids_shell_launchers() {
        let production = production_source(include_str!("supervisor.rs"));

        for forbidden in [
            "Command::new(\"bash\")",
            "run_engine.sh",
            "run_speaches_cpu_backend.sh",
            ".arg(\"-lc\")",
            "uvx",
            "git+https://github.com/speaches-ai/speaches.git",
        ] {
            assert!(
                !production.contains(forbidden),
                "supervisor production path still contains shell launcher detail `{forbidden}`"
            );
        }
        assert!(
            !production.contains("managed-backend-launch"),
            "supervisor production path must not retain the legacy launcher trampoline"
        );
        assert!(
            production.contains("spawn_managed_engine_backend("),
            "supervisor production path must spawn the engine worker directly from the typed launch spec"
        );
        assert!(
            production.contains(".arg(\"from-config\")"),
            "supervisor production path must boot the worker from a typed runtime config"
        );
    }

    #[test]
    fn auxiliary_launchability_fails_fast_when_engine_binary_is_missing() {
        let root = temp_root("aux-launchability-missing-engine");
        let err = ensure_auxiliary_backend_launchable(&root, engine::AuxiliaryRole::Embedding)
            .expect_err("missing ctox-engine should fail fast");
        assert!(err
            .to_string()
            .contains("embedding backend requires ctox-engine"));
    }

    #[test]
    fn wait_for_backend_ready_fails_when_child_exits_early() {
        let root = temp_root("backend-ready-early-exit");
        let pid_path = backend_pid_path(&root, ManagedBackendRole::Embedding);
        std::fs::write(&pid_path, "999999\n").unwrap();
        let log_path = root.join("runtime").join("ctox_embedding_backend.log");
        std::fs::write(
            &log_path,
            "ctox-engine binary could not be prepared for this host\n",
        )
        .unwrap();

        let spec = ManagedBackendSpec {
            display_model: "Qwen/Qwen3-Embedding-0.6B [GPU]".to_string(),
            request_model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
            port: 1237,
            socket_path: Some(root.join("runtime/embedding.sock").display().to_string()),
            health_path: "/health",
            launcher_kind: ManagedLauncherKind::Engine,
            compute_target: Some(engine::ComputeTarget::Gpu),
        };
        let descriptor = runtime_gpu_manager::RuntimeWorkloadDescriptor {
            role: runtime_contract::BackendRole::Embedding,
            model: spec.request_model.clone(),
            port: spec.port,
            health_path: spec.health_path.to_string(),
            launcher_kind: runtime_kernel::RuntimeLauncherKind::Engine,
            compute_target: spec.compute_target,
        };
        let err = wait_for_backend_ready(
            &root,
            ManagedBackendRole::Embedding,
            &spec,
            &descriptor,
            &runtime_gpu_manager::GpuAdmission::default(),
            &pid_path,
            &log_path,
            None,
        )
        .expect_err("dead child must fail immediately");

        let message = err.to_string();
        assert!(message.contains("exited before becoming ready"));
        assert!(message.contains("ctox-engine binary could not be prepared for this host"));
    }

    #[test]
    fn wait_for_backend_ready_reports_child_exit_status() {
        let root = temp_root("backend-ready-exit-status");
        let pid_path = backend_pid_path(&root, ManagedBackendRole::Embedding);
        let log_path = root.join("runtime").join("ctox_embedding_backend.log");
        std::fs::write(&log_path, "Applying ISQ on 1 threads.\n").unwrap();

        let spec = ManagedBackendSpec {
            display_model: "Qwen/Qwen3-Embedding-0.6B [GPU]".to_string(),
            request_model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
            port: 1237,
            socket_path: Some(root.join("runtime/embedding.sock").display().to_string()),
            health_path: "/health",
            launcher_kind: ManagedLauncherKind::Engine,
            compute_target: Some(engine::ComputeTarget::Gpu),
        };
        let descriptor = runtime_gpu_manager::RuntimeWorkloadDescriptor {
            role: runtime_contract::BackendRole::Embedding,
            model: spec.request_model.clone(),
            port: spec.port,
            health_path: spec.health_path.to_string(),
            launcher_kind: runtime_kernel::RuntimeLauncherKind::Engine,
            compute_target: spec.compute_target,
        };

        let mut child = Command::new("/bin/sh")
            .arg("-lc")
            .arg("exit 17")
            .spawn()
            .unwrap();
        std::fs::write(&pid_path, format!("{}\n", child.id())).unwrap();

        let err = wait_for_backend_ready(
            &root,
            ManagedBackendRole::Embedding,
            &spec,
            &descriptor,
            &runtime_gpu_manager::GpuAdmission::default(),
            &pid_path,
            &log_path,
            Some(&mut child),
        )
        .expect_err("child exit status must be reported");

        let message = err.to_string();
        assert!(message.contains("exit status"));
        assert!(message.contains("Applying ISQ on 1 threads."));
    }

    #[test]
    fn clear_failed_local_chat_runtime_projection_clears_persisted_plans() {
        let root = temp_root("clear-failed-chat-projection");
        let plan = runtime_plan::ChatRuntimePlan {
            model: "zai-org/GLM-4.7-Flash".to_string(),
            preset: runtime_plan::ChatPreset::Quality,
            quantization: "Q6K".to_string(),
            runtime_isq: Some("Q6K".to_string()),
            max_seq_len: 65_536,
            compaction_threshold_percent: 75,
            compaction_min_tokens: 16_384,
            min_context_floor_applied: true,
            paged_attn: "1".to_string(),
            pa_cache_type: Some("turboquant3".to_string()),
            pa_memory_fraction: Some("0.65".to_string()),
            pa_context_len: Some(65_536),
            disable_nccl: true,
            tensor_parallel_backend: None,
            mn_local_world_size: None,
            max_batch_size: 1,
            max_seqs: 1,
            cuda_visible_devices: "0,1,2,3".to_string(),
            device_layers: Some("0:8;1:13;2:13;3:13".to_string()),
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
            expected_tok_s: 32.0,
            hardware_fingerprint: "test-host".to_string(),
            theoretical_breakdown: runtime_plan::TheoreticalResourceBreakdown {
                contract_source: "test".to_string(),
                effective_total_budget_mb: 80_000,
                kv_budget_cap_mb: 16_000,
                kv_budget_fraction_milli: 650,
                weight_residency_mb: 27_000,
                kv_cache_mb: 9_000,
                fixed_runtime_base_overhead_mb: 1_620,
                backend_runtime_overhead_mb: 0,
                activation_overhead_mb: 0,
                load_peak_overhead_mb: 2_232,
                safety_headroom_mb: 3_072,
                required_effective_total_budget_mb: 49_000,
                required_total_mb: 49_000,
            },
            rationale: vec!["test".to_string()],
            gpu_allocations: vec![],
        };
        let fleet_plan = runtime_plan::RuntimeFleetPlan {
            version: 1,
            hardware_fingerprint: "test-host".to_string(),
            chat: Some(plan.clone()),
            embedding: None,
            transcription: None,
            speech: None,
        };
        runtime_plan::store_persisted_chat_runtime_plan(&root, Some(&plan)).unwrap();
        runtime_plan::store_persisted_runtime_fleet_plan(&root, Some(&fleet_plan)).unwrap();

        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert("CTOX_CHAT_MODEL".to_string(), plan.model.clone());
        env_map.insert("CTOX_CHAT_MODEL_BASE".to_string(), plan.model.clone());
        env_map.insert("CTOX_ACTIVE_MODEL".to_string(), plan.model.clone());
        env_map.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            plan.preset.label().to_string(),
        );
        runtime_plan::apply_chat_runtime_plan_env(&root, &plan, &mut env_map).unwrap();
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        clear_failed_local_chat_runtime_projection(&root, ManagedBackendRole::Chat).unwrap();

        assert!(runtime_plan::load_persisted_chat_runtime_plan(&root)
            .unwrap()
            .is_none());
        assert!(runtime_plan::load_persisted_runtime_fleet_plan(&root)
            .unwrap()
            .is_none());

        let persisted_env = runtime_env::load_runtime_env_map(&root).unwrap();
        assert!(!persisted_env.contains_key("CTOX_CHAT_RUNTIME_PLAN_ACTIVE"));
        assert!(!persisted_env.contains_key("CTOX_ENGINE_ISQ"));
        let persisted_state = runtime_state::load_runtime_state(&root)
            .unwrap()
            .expect("persisted state");
        assert!(persisted_state.engine_model.is_none());
        assert!(persisted_state.realized_context_tokens.is_none());
    }

    #[test]
    fn managed_runtime_launcher_matcher_tracks_runtime_switch() {
        assert!(command_is_managed_runtime_launcher(
            "/home/test/CTOX/target/release/ctox runtime switch Qwen/Qwen3.5-27B quality"
        ));
        assert!(command_is_managed_runtime_launcher(
            "/home/test/CTOX/target/release/ctox serve-responses-proxy"
        ));
        assert!(!command_is_managed_runtime_launcher(
            "/usr/bin/python unrelated.py"
        ));
    }

    #[test]
    fn managed_engine_process_matcher_tracks_only_persistent_backends() {
        assert!(managed_engine_process_command(
            "/home/test/CTOX/tools/model-runtime/target/release/ctox-engine from-config --file /tmp/engine.toml"
        ));
        assert!(!managed_engine_process_command(
            "/home/test/CTOX/tools/model-runtime/target/release/ctox-engine quantize auto -m zai-org/GLM-4.7-Flash"
        ));
        assert!(!managed_engine_process_command(
            "/usr/bin/python unrelated.py"
        ));
    }

    #[test]
    fn managed_engine_quantize_matcher_tracks_quantize_workers() {
        assert!(managed_engine_quantize_command(
            "/home/test/CTOX/tools/model-runtime/target/release/ctox-engine quantize auto -m zai-org/GLM-4.7-Flash"
        ));
        assert!(!managed_engine_quantize_command(
            "/home/test/CTOX/tools/model-runtime/target/release/ctox-engine from-config --file /tmp/engine.toml"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn current_process_cwd_matches_workspace_root() {
        let root = std::env::current_dir().unwrap();
        assert!(process_current_dir_matches_root(std::process::id(), &root));
    }
}
