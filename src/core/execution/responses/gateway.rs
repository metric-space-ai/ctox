use anyhow::Context;
use serde::Deserialize;
use serde::Serialize;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::runtime_control;
use crate::inference::runtime_env;
use crate::inference::runtime_kernel;
use crate::inference::runtime_state;

const DEFAULT_BOOST_MINUTES: u64 = 20;

/// Window during which a previously computed `RuntimeTelemetry` snapshot is
/// reused. The compute path includes `InferenceRuntimeKernel::resolve` plus
/// `merge_credentials_into_env_map` per auxiliary role — and SQLite re-parses
/// the schema on every fresh connection's first query, so even with the
/// bulk-credential fix the compute still costs a few hundred ms on a contended
/// daemon. The TUI calls this on a ~700ms refresh cadence; the previous
/// 1500ms TTL was shorter than the compute time, so every check expired —
/// 5s gives the cache room to actually hit. State-changing call sites must
/// call `invalidate_runtime_telemetry_cache` after the change.
const RUNTIME_TELEMETRY_CACHE_TTL: Duration = Duration::from_secs(5);

fn uses_remote_api_upstream(upstream_base_url: &str) -> bool {
    runtime_state::is_openai_compatible_api_upstream(upstream_base_url)
        || upstream_base_url.starts_with(runtime_state::default_api_upstream_base_url_for_provider(
            "openrouter",
        ))
        || upstream_base_url.starts_with(runtime_state::default_api_upstream_base_url_for_provider(
            "minimax",
        ))
        || runtime_state::api_provider_for_upstream_base_url(upstream_base_url)
            .eq_ignore_ascii_case("azure_foundry")
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GatewayConfig {
    pub root: PathBuf,
    pub listen_host: String,
    pub listen_port: u16,
    pub upstream_base_url: String,
    pub upstream_transport: Option<LocalTransport>,
    pub active_model: Option<String>,
    pub embedding_base_url: String,
    pub embedding_transport: Option<LocalTransport>,
    pub embedding_model: Option<String>,
    pub transcription_base_url: String,
    pub transcription_transport: Option<LocalTransport>,
    pub transcription_model: Option<String>,
    pub speech_base_url: String,
    pub speech_transport: Option<LocalTransport>,
    pub speech_model: Option<String>,
}

impl GatewayConfig {
    pub fn resolve_with_root(root: &Path) -> Self {
        let resolved = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok();
        let runtime_state = runtime_state::load_or_resolve_runtime_state(root).ok();
        let active_model = resolved
            .as_ref()
            .and_then(|resolved| resolved.gateway.active_model.clone())
            .or_else(|| {
                runtime_state
                    .as_ref()
                    .and_then(|state| state.active_or_selected_model().map(ToOwned::to_owned))
            })
            .or_else(|| Some(runtime_state::default_primary_model()));
        let (embedding_base_url, embedding_transport, embedding_model) = resolved
            .as_ref()
            .map(|resolved| {
                (
                    resolved.gateway.embedding_base_url.clone(),
                    resolved
                        .embedding
                        .as_ref()
                        .map(|binding| binding.transport.clone()),
                    resolved.gateway.embedding_model.clone(),
                )
            })
            .unwrap_or_else(|| auxiliary_gateway_target(root, engine::AuxiliaryRole::Embedding));
        let (transcription_base_url, transcription_transport, transcription_model) = resolved
            .as_ref()
            .map(|resolved| {
                (
                    resolved.gateway.transcription_base_url.clone(),
                    resolved
                        .transcription
                        .as_ref()
                        .map(|binding| binding.transport.clone()),
                    resolved.gateway.transcription_model.clone(),
                )
            })
            .unwrap_or_else(|| auxiliary_gateway_target(root, engine::AuxiliaryRole::Stt));
        let (speech_base_url, speech_transport, speech_model) = resolved
            .as_ref()
            .map(|resolved| {
                (
                    resolved.gateway.speech_base_url.clone(),
                    resolved
                        .speech
                        .as_ref()
                        .map(|binding| binding.transport.clone()),
                    resolved.gateway.speech_model.clone(),
                )
            })
            .unwrap_or_else(|| auxiliary_gateway_target(root, engine::AuxiliaryRole::Tts));
        Self {
            root: root.to_path_buf(),
            listen_host: runtime_state::default_loopback_host().to_string(),
            listen_port: 12434,
            upstream_base_url: resolved
                .as_ref()
                .map(|resolved| resolved.gateway.upstream_base_url.clone())
                .or_else(|| {
                    runtime_state
                        .as_ref()
                        .map(|state| state.upstream_base_url.clone())
                })
                .unwrap_or_else(|| match active_model.as_deref() {
                    Some(model) if engine::is_api_chat_model(model) => {
                        runtime_state::default_api_upstream_base_url_for_provider(
                            engine::default_api_provider_for_model(model),
                        )
                        .to_string()
                    }
                    _ => runtime_state::local_upstream_base_url(
                        runtime_state::default_local_engine_port(),
                    ),
                }),
            upstream_transport: resolved
                .as_ref()
                .and_then(|resolved| resolved.primary_generation.as_ref())
                .map(|binding| binding.transport.clone())
                .or_else(|| {
                    active_model.as_deref().and_then(|model| {
                        engine::supports_local_chat_runtime(model).then(|| {
                            runtime_kernel::managed_runtime_transport(
                                root,
                                runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
                            )
                        })
                    })
                }),
            active_model,
            embedding_base_url,
            embedding_transport,
            embedding_model,
            transcription_base_url,
            transcription_transport,
            transcription_model,
            speech_base_url,
            speech_transport,
            speech_model,
        }
    }

    pub fn listen_addr(&self) -> String {
        format!("{}:{}", self.listen_host, self.listen_port)
    }

    pub fn upstream_transport(&self) -> Option<LocalTransport> {
        self.upstream_transport.clone()
    }

    pub fn join_url(&self, request_url: &str) -> String {
        format!(
            "{}{}",
            self.upstream_base_url.trim_end_matches('/'),
            request_url
        )
    }

    pub fn routed_base_url(&self, request_url: &str) -> &str {
        match request_url {
            "/v1/embeddings" => &self.embedding_base_url,
            "/v1/audio/transcriptions" => &self.transcription_base_url,
            "/v1/audio/speech" | "/v1/audio/voices" => &self.speech_base_url,
            _ => &self.upstream_base_url,
        }
    }

    pub fn routed_model(&self, request_url: &str) -> Option<&str> {
        match request_url {
            "/v1/embeddings" => self.embedding_model.as_deref(),
            "/v1/audio/transcriptions" => self.transcription_model.as_deref(),
            "/v1/audio/speech" | "/v1/audio/voices" => self.speech_model.as_deref(),
            _ => self.active_model.as_deref(),
        }
    }

    pub fn join_routed_url(&self, request_url: &str) -> String {
        format!(
            "{}{}",
            self.routed_base_url(request_url).trim_end_matches('/'),
            request_url
        )
    }

    pub fn routed_transport(&self, request_url: &str) -> Option<LocalTransport> {
        match request_url {
            "/v1/embeddings" => self.embedding_transport.clone(),
            "/v1/audio/transcriptions" => self.transcription_transport.clone(),
            "/v1/audio/speech" | "/v1/audio/voices" => self.speech_transport.clone(),
            _ => self.upstream_transport.clone(),
        }
    }
}

fn auxiliary_gateway_target(
    root: &Path,
    role: engine::AuxiliaryRole,
) -> (String, Option<LocalTransport>, Option<String>) {
    let auxiliary_state = runtime_state::load_or_resolve_runtime_state(root)
        .ok()
        .map(|state| runtime_state::auxiliary_runtime_state_for_role(&state, role).clone())
        .unwrap_or_default();
    if !auxiliary_state.enabled {
        return (String::new(), None, None);
    }
    let selection =
        engine::auxiliary_model_selection(role, auxiliary_state.configured_model.as_deref());
    if let Some(base_url) = auxiliary_state
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    {
        return (base_url, None, Some(selection.request_model.to_string()));
    }
    let transport = runtime_kernel::managed_runtime_transport(root, auxiliary_workload_role(role));
    (
        String::new(),
        Some(transport),
        Some(selection.request_model.to_string()),
    )
}

fn auxiliary_workload_role(role: engine::AuxiliaryRole) -> runtime_kernel::InferenceWorkloadRole {
    match role {
        engine::AuxiliaryRole::Embedding => runtime_kernel::InferenceWorkloadRole::Embedding,
        engine::AuxiliaryRole::Stt => runtime_kernel::InferenceWorkloadRole::Transcription,
        engine::AuxiliaryRole::Tts => runtime_kernel::InferenceWorkloadRole::Speech,
        engine::AuxiliaryRole::Vision => runtime_kernel::InferenceWorkloadRole::Vision,
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, Default)]
pub struct RuntimeTelemetry {
    pub active_model: Option<String>,
    pub base_model: Option<String>,
    pub boost_model: Option<String>,
    pub boost_active: bool,
    pub boost_active_until_epoch: Option<u64>,
    pub boost_remaining_seconds: Option<u64>,
    pub boost_reason: Option<String>,
    pub upstream_base_url: Option<String>,
    pub last_known_good_model: Option<String>,
    pub backend_healthy: bool,
    pub last_switch_status: Option<String>,
    pub last_switch_error: Option<String>,
    pub recovery_count: u64,
    pub last_request_path: Option<String>,
    pub last_response_at: Option<String>,
    pub last_latency_ms: Option<u64>,
    pub last_input_tokens: Option<u64>,
    pub last_output_tokens: Option<u64>,
    pub last_total_tokens: Option<u64>,
    pub last_tokens_per_second: Option<f64>,
    pub load_observation_path: Option<String>,
    pub load_observation: Option<LoadObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoadObservationGpu {
    pub gpu_index: usize,
    pub name: String,
    pub total_mb: u64,
    pub baseline_used_mb: u64,
    pub current_used_mb: u64,
    pub peak_used_mb: u64,
    pub final_used_mb: u64,
    pub current_delta_mb: u64,
    pub peak_delta_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoadObservation {
    pub model: String,
    pub role: String,
    pub port: u16,
    pub startup_healthy: bool,
    pub sample_count: u64,
    pub started_at_epoch: u64,
    pub observed_until_epoch: u64,
    pub healthy_at_epoch: Option<u64>,
    pub gpus: Vec<LoadObservationGpu>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoostStatus {
    pub active: bool,
    pub base_model: Option<String>,
    pub boost_model: Option<String>,
    pub active_model: Option<String>,
    pub active_until_epoch: Option<u64>,
    pub remaining_seconds: Option<u64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeSwitchResponse {
    ok: bool,
    active_model: String,
    upstream_base_url: String,
    rolled_back: bool,
    message: String,
}

#[derive(Debug, Clone, Default)]
struct BoostLeaseState {
    active: bool,
    base_model: Option<String>,
    boost_model: Option<String>,
    active_until_epoch: Option<u64>,
    remaining_seconds: Option<u64>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalUnaryIpcResponse {
    RuntimeHealth {
        healthy: bool,
        default_model: Option<String>,
        loaded_models: Vec<String>,
    },
    Error {
        code: String,
        message: String,
    },
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn boost_lease_state_from_runtime_state(
    state: &runtime_state::InferenceRuntimeState,
    active_model: Option<&str>,
) -> BoostLeaseState {
    let base_model = state
        .base_model
        .clone()
        .or_else(|| state.requested_model.clone())
        .or_else(|| state.active_model.clone());
    let boost_model = state.boost.model.clone();
    let active_until_epoch = state.boost.active_until_epoch;
    let reason = state.boost.reason.clone();
    let now = now_epoch_seconds();
    let remaining_seconds = active_until_epoch
        .and_then(|until| until.checked_sub(now))
        .filter(|value| *value > 0);
    let active = remaining_seconds.is_some()
        && boost_model.is_some()
        && active_model
            .zip(boost_model.as_deref())
            .is_some_and(|(active_model, boost_model)| active_model.trim() == boost_model.trim());
    BoostLeaseState {
        active,
        base_model,
        boost_model,
        active_until_epoch,
        remaining_seconds,
        reason,
    }
}

fn load_boost_lease_state(root: &Path, active_model: Option<&str>) -> BoostLeaseState {
    runtime_state::load_or_resolve_runtime_state(root)
        .map(|state| boost_lease_state_from_runtime_state(&state, active_model))
        .unwrap_or_else(|_| BoostLeaseState {
            active: false,
            base_model: Some(runtime_state::default_primary_model()),
            boost_model: None,
            active_until_epoch: None,
            remaining_seconds: None,
            reason: None,
        })
}

fn sync_boost_telemetry_fields(
    telemetry_state: &mut RuntimeTelemetry,
    root: &Path,
    active_model: Option<&str>,
) {
    let boost = load_boost_lease_state(root, active_model);
    telemetry_state.base_model = boost.base_model.clone();
    telemetry_state.boost_model = boost.boost_model.clone();
    telemetry_state.boost_active = boost.active;
    telemetry_state.boost_active_until_epoch = boost.active_until_epoch;
    telemetry_state.boost_remaining_seconds = boost.remaining_seconds;
    telemetry_state.boost_reason = boost.reason.clone();
}

pub fn boost_status(root: &Path) -> anyhow::Result<BoostStatus> {
    let telemetry = fetch_runtime_telemetry(root)?;
    Ok(BoostStatus {
        active: telemetry.boost_active,
        base_model: telemetry.base_model,
        boost_model: telemetry.boost_model,
        active_model: telemetry.active_model,
        active_until_epoch: telemetry.boost_active_until_epoch,
        remaining_seconds: telemetry.boost_remaining_seconds,
        reason: telemetry.boost_reason,
    })
}

pub fn start_boost_lease(
    root: &Path,
    model_override: Option<&str>,
    minutes_override: Option<u64>,
    reason: Option<&str>,
) -> anyhow::Result<BoostStatus> {
    let boost_state = runtime_state::load_or_resolve_runtime_state(root)?;
    let base_model = boost_state
        .base_or_selected_model()
        .map(ToOwned::to_owned)
        .unwrap_or_else(runtime_state::default_primary_model);
    let env_map = runtime_env::load_runtime_env_map(root).unwrap_or_default();
    let boost_model = model_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| boost_state.boost.model.clone())
        .or_else(|| {
            env_map
                .get("CTOX_CHAT_MODEL_BOOST")
                .cloned()
                .filter(|value| !value.trim().is_empty())
        })
        .context("boost start requires CTOX_CHAT_MODEL_BOOST or --model")?;
    let default_minutes = env_map
        .get("CTOX_BOOST_DEFAULT_MINUTES")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_BOOST_MINUTES);
    let minutes = minutes_override
        .filter(|value| *value > 0)
        .unwrap_or(default_minutes);
    let _ = request_runtime_switch(root, &boost_model, None)?;
    persist_boost_lease(
        root,
        Some(base_model),
        Some(boost_model),
        Some(now_epoch_seconds() + minutes.saturating_mul(60)),
        reason.map(str::trim).filter(|value| !value.is_empty()),
    )?;
    boost_status(root)
}

pub fn stop_boost_lease(root: &Path) -> anyhow::Result<BoostStatus> {
    let state = runtime_state::load_or_resolve_runtime_state(root)?;
    let base_model = state
        .base_or_selected_model()
        .map(ToOwned::to_owned)
        .unwrap_or_else(runtime_state::default_primary_model);
    let _ = request_runtime_switch(root, &base_model, None)?;
    persist_boost_lease(
        root,
        Some(base_model),
        state.boost.model.clone(),
        None,
        None,
    )?;
    boost_status(root)
}

fn persist_boost_lease(
    root: &Path,
    base_model: Option<String>,
    boost_model: Option<String>,
    active_until_epoch: Option<u64>,
    reason: Option<&str>,
) -> anyhow::Result<()> {
    let mut state = runtime_state::load_or_resolve_runtime_state(root)?;
    let env_map = runtime_env::load_runtime_env_map(root).unwrap_or_default();
    if let Some(base_model) = base_model {
        state.base_model = Some(base_model);
    }
    state.boost.model = boost_model.filter(|value| !value.trim().is_empty());
    state.boost.active_until_epoch = active_until_epoch;
    state.boost.reason = reason
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    runtime_env::save_runtime_state_projection(root, &state, &env_map)
}

fn request_runtime_switch(
    root: &Path,
    model: &str,
    preset: Option<&str>,
) -> anyhow::Result<RuntimeSwitchResponse> {
    apply_runtime_switch(root, model, preset)
}

fn fetch_runtime_telemetry(root: &Path) -> anyhow::Result<RuntimeTelemetry> {
    Ok(synthesize_runtime_telemetry(root))
}

pub fn current_runtime_telemetry(root: &Path) -> RuntimeTelemetry {
    synthesize_runtime_telemetry(root)
}

fn apply_runtime_switch(
    root: &Path,
    model: &str,
    preset: Option<&str>,
) -> anyhow::Result<RuntimeSwitchResponse> {
    let outcome = runtime_control::execute_runtime_switch(root, model, preset)?;

    Ok(RuntimeSwitchResponse {
        ok: true,
        active_model: outcome.active_model.clone(),
        upstream_base_url: outcome.upstream_base_url.clone(),
        rolled_back: false,
        message: if outcome.already_active {
            format!("runtime already active on {}", outcome.active_model)
        } else {
            format!("runtime switched to {}", outcome.active_model)
        },
    })
}

fn runtime_telemetry_cache() -> &'static Mutex<Option<(Instant, PathBuf, RuntimeTelemetry)>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, PathBuf, RuntimeTelemetry)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Drop any cached telemetry. Call this after a state-changing operation —
/// runtime switch, boost start/stop, secret edit — so the next caller sees
/// fresh data instead of waiting up to `RUNTIME_TELEMETRY_CACHE_TTL`.
pub fn invalidate_runtime_telemetry_cache() {
    if let Ok(mut guard) = runtime_telemetry_cache().lock() {
        *guard = None;
    }
}

fn synthesize_runtime_telemetry(root: &Path) -> RuntimeTelemetry {
    {
        let guard = runtime_telemetry_cache()
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        if let Some((cached_at, cached_root, cached_telemetry)) = guard.as_ref() {
            if cached_root.as_path() == root && cached_at.elapsed() < RUNTIME_TELEMETRY_CACHE_TTL {
                return cached_telemetry.clone();
            }
        }
    }
    let telemetry = synthesize_runtime_telemetry_uncached(root);
    let mut guard = runtime_telemetry_cache()
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    *guard = Some((Instant::now(), root.to_path_buf(), telemetry.clone()));
    telemetry
}

fn synthesize_runtime_telemetry_uncached(root: &Path) -> RuntimeTelemetry {
    let config = GatewayConfig::resolve_with_root(root);
    let backend_healthy = probe_upstream_health(&config);
    let mut telemetry = RuntimeTelemetry {
        active_model: config.active_model.clone(),
        upstream_base_url: (!config.upstream_base_url.trim().is_empty())
            .then(|| config.upstream_base_url.clone()),
        last_known_good_model: config.active_model.clone(),
        backend_healthy,
        ..RuntimeTelemetry::default()
    };
    if let Ok(Some(transaction)) = runtime_control::load_runtime_switch_transaction(root) {
        telemetry.last_switch_status =
            Some(format!("{:?}", transaction.phase).to_ascii_lowercase());
        telemetry.last_switch_error = transaction.error.clone();
    } else if backend_healthy {
        telemetry.last_switch_status = Some("ready".to_string());
    }
    sync_boost_telemetry_fields(&mut telemetry, root, config.active_model.as_deref());
    telemetry.load_observation_path =
        load_observation_path(&config).map(|path| path.display().to_string());
    telemetry.load_observation = read_load_observation(&config);
    telemetry
}

fn complete_local_unary_ipc_roundtrip(
    transport: &LocalTransport,
    request_payload: &serde_json::Value,
) -> anyhow::Result<LocalUnaryIpcResponse> {
    let timeout = Duration::from_secs(300);
    let label = transport.display_label();
    let mut stream = transport
        .connect_blocking(timeout)
        .with_context(|| format!("failed to connect via {label}"))?;
    let mut request_body =
        serde_json::to_vec(request_payload).context("failed to encode local IPC request")?;
    request_body.push(b'\n');
    stream
        .write_all(&request_body)
        .with_context(|| format!("failed to write request via {label}"))?;
    stream
        .flush()
        .with_context(|| format!("failed to flush request via {label}"))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .with_context(|| format!("failed to read response via {label}"))?;
        if bytes_read == 0 {
            anyhow::bail!("local IPC socket closed before response");
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return serde_json::from_str(trimmed).context("failed to parse local IPC response");
    }
}

fn probe_upstream_health(config: &GatewayConfig) -> bool {
    if uses_remote_api_upstream(&config.upstream_base_url) {
        return runtime_env::env_or_config(
            &config.root,
            runtime_state::api_key_env_var_for_upstream_base_url(&config.upstream_base_url),
        )
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    }
    if let Some(transport) = config.upstream_transport() {
        return probe_local_runtime_health(&transport).unwrap_or_else(|_| transport.probe());
    }
    probe_backend_health_url(&config.join_url("/health"))
}

fn probe_local_runtime_health(transport: &LocalTransport) -> anyhow::Result<bool> {
    let response = complete_local_unary_ipc_roundtrip(
        transport,
        &serde_json::json!({
            "kind": "runtime_health",
        }),
    )?;
    match response {
        LocalUnaryIpcResponse::RuntimeHealth {
            healthy,
            default_model,
            loaded_models,
        } => {
            let _ = (default_model, loaded_models);
            Ok(healthy)
        }
        LocalUnaryIpcResponse::Error { code, message } => {
            let _ = code;
            Err(anyhow::anyhow!(message))
        }
    }
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

fn load_observation_path(config: &GatewayConfig) -> Option<PathBuf> {
    if uses_remote_api_upstream(&config.upstream_base_url) {
        return None;
    }
    let port_slug = if config.upstream_base_url.trim().is_empty() {
        runtime_state::load_or_resolve_runtime_state(&config.root)
            .ok()?
            .engine_port?
            .to_string()
    } else {
        config
            .upstream_base_url
            .rsplit(':')
            .next()?
            .trim()
            .to_string()
    };
    if port_slug.is_empty() {
        return None;
    }
    Some(
        config
            .root
            .join("runtime")
            .join(format!("load_observation_{port_slug}.json")),
    )
}

fn read_load_observation(config: &GatewayConfig) -> Option<LoadObservation> {
    let path = load_observation_path(config)?;
    let raw = std::fs::read(&path).ok()?;
    serde_json::from_slice(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    fn gateway_config(model: &str, upstream_base_url: &str) -> GatewayConfig {
        GatewayConfig {
            root: PathBuf::from("/tmp/ctox"),
            listen_host: "127.0.0.1".to_string(),
            listen_port: 12434,
            upstream_base_url: upstream_base_url.to_string(),
            upstream_transport: None,
            active_model: Some(model.to_string()),
            embedding_base_url: String::new(),
            embedding_transport: None,
            embedding_model: Some("Qwen/Qwen3-Embedding-0.6B".to_string()),
            transcription_base_url: String::new(),
            transcription_transport: None,
            transcription_model: Some("engineai/Voxtral-Mini-4B-Realtime-2602".to_string()),
            speech_base_url: String::new(),
            speech_transport: None,
            speech_model: Some("engineai/Voxtral-4B-TTS-2603".to_string()),
        }
    }

    fn persist_runtime_env_text(root: &Path, raw: &str) {
        let mut env_map = BTreeMap::new();
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let Some((key, value)) = trimmed.split_once('=') else {
                continue;
            };
            env_map.insert(key.trim().to_string(), value.trim().to_string());
        }
        runtime_env::save_runtime_env_map(root, &env_map).unwrap();
    }

    #[test]
    fn local_ipc_routes_do_not_synthesize_loopback_urls() {
        let config = gateway_config("openai/gpt-oss-120b", "");

        assert_eq!(config.join_routed_url("/v1/responses"), "/v1/responses");
        assert_eq!(config.join_routed_url("/v1/embeddings"), "/v1/embeddings");
        assert_eq!(
            config.join_routed_url("/v1/audio/transcriptions"),
            "/v1/audio/transcriptions"
        );
        assert_eq!(
            config.join_routed_url("/v1/audio/speech"),
            "/v1/audio/speech"
        );
        assert_eq!(
            config.join_routed_url("/v1/audio/voices"),
            "/v1/audio/voices"
        );
    }

    #[test]
    fn routed_transport_prefers_auxiliary_socket_when_present() {
        let config = GatewayConfig {
            embedding_transport: Some(LocalTransport::UnixSocket {
                path: PathBuf::from("/tmp/embedding.sock"),
            }),
            transcription_transport: Some(LocalTransport::UnixSocket {
                path: PathBuf::from("/tmp/transcription.sock"),
            }),
            speech_transport: Some(LocalTransport::UnixSocket {
                path: PathBuf::from("/tmp/speech.sock"),
            }),
            ..gateway_config("openai/gpt-oss-120b", "")
        };

        match config.routed_transport("/v1/embeddings").unwrap() {
            LocalTransport::UnixSocket { path } => {
                assert_eq!(path, PathBuf::from("/tmp/embedding.sock"))
            }
            other => panic!("unexpected transport: {other:?}"),
        }
        match config.routed_transport("/v1/audio/transcriptions").unwrap() {
            LocalTransport::UnixSocket { path } => {
                assert_eq!(path, PathBuf::from("/tmp/transcription.sock"))
            }
            other => panic!("unexpected transport: {other:?}"),
        }
        match config.routed_transport("/v1/audio/speech").unwrap() {
            LocalTransport::UnixSocket { path } => {
                assert_eq!(path, PathBuf::from("/tmp/speech.sock"))
            }
            other => panic!("unexpected transport: {other:?}"),
        }
    }

    #[test]
    fn api_runtime_config_clears_stale_local_chat_plan_fields() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ctox_gateway_api_runtime_config_{}_{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        persist_runtime_env_text(
            &root,
            "CTOX_CHAT_SOURCE=local\nCTOX_CHAT_RUNTIME_PLAN_ACTIVE=1\nCTOX_ENGINE_MODEL=Qwen/Qwen3.5-35B-A3B\nCTOX_ENGINE_PAGED_ATTN=auto\nCTOX_ENGINE_DEVICE_LAYERS=0:20;1:10;2:10\n",
        );

        let config = GatewayConfig {
            root: root.clone(),
            listen_host: "127.0.0.1".to_string(),
            listen_port: 12434,
            upstream_base_url: "https://api.openai.com".to_string(),
            upstream_transport: None,
            active_model: Some("gpt-5.4".to_string()),
            embedding_base_url: String::new(),
            embedding_transport: None,
            embedding_model: Some("Qwen/Qwen3-Embedding-0.6B".to_string()),
            transcription_base_url: String::new(),
            transcription_transport: None,
            transcription_model: Some("engineai/Voxtral-Mini-4B-Realtime-2602".to_string()),
            speech_base_url: String::new(),
            speech_transport: None,
            speech_model: Some("engineai/Voxtral-4B-TTS-2603".to_string()),
        };

        runtime_control::execute_runtime_switch(
            &root,
            config.active_model.as_deref().unwrap(),
            None,
        )
        .unwrap();

        let env_map = runtime_env::load_runtime_env_map(&root).unwrap();
        assert_eq!(
            env_map.get("CTOX_CHAT_SOURCE").map(String::as_str),
            Some("api")
        );
        assert_eq!(
            env_map.get("CTOX_ACTIVE_MODEL").map(String::as_str),
            Some("gpt-5.4")
        );
        assert!(!env_map.contains_key("CTOX_CHAT_RUNTIME_PLAN_ACTIVE"));
        assert!(!env_map.contains_key("CTOX_ENGINE_DEVICE_LAYERS"));
        assert!(!env_map.contains_key("CTOX_ENGINE_PAGED_ATTN"));
        assert!(!env_map.contains_key("CTOX_ENGINE_MODEL"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn local_gateway_config_prefers_runtime_port_override() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ctox_gateway_local_runtime_port_{}_{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        persist_runtime_env_text(
            &root,
            "CTOX_ACTIVE_MODEL=openai/gpt-oss-120b\nCTOX_CHAT_MODEL_BASE=openai/gpt-oss-120b\nCTOX_CHAT_SOURCE=local\nCTOX_ENGINE_PORT=2235\n",
        );

        let config = GatewayConfig::resolve_with_root(&root);

        assert_eq!(config.active_model.as_deref(), Some("openai/gpt-oss-120b"));
        assert_eq!(config.listen_port, 12434);
        assert_eq!(config.upstream_base_url, "");
        assert!(config.upstream_transport.is_some());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn local_gateway_config_ignores_stale_upstream_override() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ctox_gateway_local_upstream_override_{}_{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        persist_runtime_env_text(
            &root,
            "CTOX_ACTIVE_MODEL=openai/gpt-oss-120b\nCTOX_CHAT_MODEL_BASE=openai/gpt-oss-120b\nCTOX_CHAT_SOURCE=local\nCTOX_ENGINE_PORT=2235\nCTOX_UPSTREAM_BASE_URL=http://127.0.0.1:1235\n",
        );

        let config = GatewayConfig::resolve_with_root(&root);

        assert_eq!(config.active_model.as_deref(), Some("openai/gpt-oss-120b"));
        assert_eq!(config.listen_port, 12434);
        assert_eq!(config.upstream_base_url, "");
        assert!(config.upstream_transport.is_some());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn request_runtime_switch_updates_runtime_without_legacy_gateway_loopback() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ctox_gateway_direct_switch_{}_{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        persist_runtime_env_text(
            &root,
            "CTOX_CHAT_SOURCE=api\nCTOX_CHAT_MODEL=gpt-5.4\nCTOX_CHAT_MODEL_BASE=gpt-5.4\nCTOX_ACTIVE_MODEL=gpt-5.4\n",
        );

        let response = request_runtime_switch(&root, "gpt-5.4-mini", None).unwrap();
        let env_map = runtime_env::load_runtime_env_map(&root).unwrap();

        assert!(response.ok);
        assert_eq!(response.active_model, "gpt-5.4-mini");
        assert_eq!(
            env_map.get("CTOX_ACTIVE_MODEL").map(String::as_str),
            Some("gpt-5.4-mini")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn boost_status_works_without_legacy_gateway_process() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ctox_gateway_boost_status_{}_{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let active_until = now_epoch_seconds() + 300;
        persist_runtime_env_text(
            &root,
            &format!(
                "CTOX_CHAT_SOURCE=api\nCTOX_CHAT_MODEL_BASE=gpt-5.4\nCTOX_CHAT_MODEL=gpt-5.4-mini\nCTOX_ACTIVE_MODEL=gpt-5.4-mini\nCTOX_CHAT_MODEL_BOOST=gpt-5.4-mini\nCTOX_BOOST_ACTIVE_UNTIL_EPOCH={active_until}\nCTOX_BOOST_REASON=test\n"
            ),
        );

        let status = boost_status(&root).unwrap();

        assert!(status.active);
        assert_eq!(status.base_model.as_deref(), Some("gpt-5.4"));
        assert_eq!(status.boost_model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(status.active_model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(status.reason.as_deref(), Some("test"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
