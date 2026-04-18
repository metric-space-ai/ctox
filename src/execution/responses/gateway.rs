use anyhow::Context;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tiny_http::Header;
use tiny_http::Method;
use tiny_http::Response;
use tiny_http::Server;
use tiny_http::StatusCode;

use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::model_adapters::ResolvedResponsesAdapterRoute;
use crate::inference::model_adapters::ResponsesAdapterResponsePlan;
use crate::inference::model_adapters::ResponsesTransportKind;
use crate::inference::runtime_control;
use crate::inference::runtime_env;
use crate::inference::runtime_kernel;
use crate::inference::runtime_state;
use crate::inference::supervisor;
use crate::execution::models::vision_preprocessor;
use crate::inference::web_search;

const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
    "host",
    "content-length",
];
const OPENAI_RESPONSES_BASE_URL: &str = "https://api.openai.com";

fn uses_remote_api_upstream(upstream_base_url: &str) -> bool {
    runtime_state::is_openai_compatible_api_upstream(upstream_base_url)
        || upstream_base_url.starts_with(runtime_state::default_api_upstream_base_url_for_provider(
            "openrouter",
        ))
        || upstream_base_url.starts_with(runtime_state::default_api_upstream_base_url_for_provider(
            "minimax",
        ))
}
const DEFAULT_BOOST_MINUTES: u64 = 20;

struct CompletionTemplateRelayResponse {
    status_code: u16,
    response_headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProxyConfig {
    pub root: PathBuf,
    pub listen_host: String,
    pub listen_port: u16,
    pub upstream_base_url: String,
    pub upstream_socket_path: Option<String>,
    pub active_model: Option<String>,
    pub embedding_base_url: String,
    pub embedding_model: Option<String>,
    pub transcription_base_url: String,
    pub transcription_model: Option<String>,
    pub speech_base_url: String,
    pub speech_model: Option<String>,
}

impl ProxyConfig {
    pub fn resolve_with_root(root: &std::path::Path) -> Self {
        let resolved = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok();
        let runtime_state = runtime_state::load_or_resolve_runtime_state(root).ok();
        let active_model = resolved
            .as_ref()
            .and_then(|resolved| resolved.proxy.active_model.clone())
            .or_else(|| {
                runtime_state
                    .as_ref()
                    .and_then(|state| state.active_or_selected_model().map(ToOwned::to_owned))
            })
            .or_else(|| Some(runtime_state::default_primary_model()));
        let (embedding_base_url, embedding_model) = resolved
            .as_ref()
            .map(|resolved| {
                (
                    resolved.proxy.embedding_base_url.clone(),
                    resolved.proxy.embedding_model.clone(),
                )
            })
            .unwrap_or_else(|| auxiliary_proxy_target(root, engine::AuxiliaryRole::Embedding));
        let (transcription_base_url, transcription_model) = resolved
            .as_ref()
            .map(|resolved| {
                (
                    resolved.proxy.transcription_base_url.clone(),
                    resolved.proxy.transcription_model.clone(),
                )
            })
            .unwrap_or_else(|| auxiliary_proxy_target(root, engine::AuxiliaryRole::Stt));
        let (speech_base_url, speech_model) = resolved
            .as_ref()
            .map(|resolved| {
                (
                    resolved.proxy.speech_base_url.clone(),
                    resolved.proxy.speech_model.clone(),
                )
            })
            .unwrap_or_else(|| auxiliary_proxy_target(root, engine::AuxiliaryRole::Tts));
        Self {
            root: root.to_path_buf(),
            listen_host: resolved
                .as_ref()
                .map(|resolved| resolved.proxy.listen_host.clone())
                .or_else(|| runtime_state.as_ref().map(|state| state.proxy_host.clone()))
                .unwrap_or_else(|| "127.0.0.1".to_string()),
            listen_port: resolved
                .as_ref()
                .map(|resolved| resolved.proxy.listen_port)
                .or_else(|| runtime_state.as_ref().map(|state| state.proxy_port))
                .unwrap_or(12434),
            upstream_base_url: resolved
                .as_ref()
                .map(|resolved| resolved.proxy.upstream_base_url.clone())
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
                    Some(_) => runtime_state::local_upstream_base_url(
                        runtime_state::default_local_engine_port(),
                    ),
                    None => runtime_state::local_upstream_base_url(
                        runtime_state::default_local_engine_port(),
                    ),
                }),
            upstream_socket_path: resolved.as_ref().and_then(|resolved| {
                resolved
                    .primary_generation
                    .as_ref()
                    .and_then(|binding| binding.socket_path.clone())
            }),
            active_model,
            embedding_base_url,
            embedding_model,
            transcription_base_url,
            transcription_model,
            speech_base_url,
            speech_model,
        }
    }

    pub fn listen_addr(&self) -> String {
        format!("{}:{}", self.listen_host, self.listen_port)
    }

    /// Construct the `LocalTransport` that addresses the primary-generation
    /// upstream when the backend is running as a local IPC endpoint.
    /// Returns `None` for HTTP-only upstreams (remote APIs) — callers fall
    /// back to `upstream_base_url` in that case.
    pub fn upstream_transport(&self) -> Option<LocalTransport> {
        self.upstream_socket_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|path| LocalTransport::UnixSocket {
                path: std::path::PathBuf::from(path),
            })
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
}

fn auxiliary_proxy_target(
    root: &std::path::Path,
    role: engine::AuxiliaryRole,
) -> (String, Option<String>) {
    let auxiliary_state = runtime_state::load_or_resolve_runtime_state(root)
        .ok()
        .map(|state| runtime_state::auxiliary_runtime_state_for_role(&state, role).clone())
        .unwrap_or_default();
    if !auxiliary_state.enabled {
        return (String::new(), None);
    }
    let selection =
        engine::auxiliary_model_selection(role, auxiliary_state.configured_model.as_deref());
    let base_url = auxiliary_state.base_url.unwrap_or_else(|| {
        format!(
            "http://127.0.0.1:{}",
            auxiliary_state.port.unwrap_or(selection.default_port)
        )
    });
    (base_url, Some(selection.request_model.to_string()))
}

fn local_models_payload(config: &ProxyConfig) -> Value {
    let data = config
        .active_model
        .as_ref()
        .map(|model| {
            vec![serde_json::json!({
                "id": model,
                "object": "model",
                "owned_by": "ctox",
            })]
        })
        .unwrap_or_default();
    serde_json::json!({
        "object": "list",
        "data": data,
    })
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

#[derive(Debug, Clone)]
struct ProxyState {
    root: PathBuf,
    config: ProxyConfig,
    last_known_good: Option<ProxyConfig>,
    last_switch_error: Option<String>,
    recovery_count: u64,
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

fn load_boost_lease_state(root: &std::path::Path, active_model: Option<&str>) -> BoostLeaseState {
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
    root: &std::path::Path,
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

pub fn serve_proxy(config: ProxyConfig) -> anyhow::Result<()> {
    let server = Server::http(config.listen_addr())
        .map_err(|err| anyhow::anyhow!("failed to bind CTOX responses proxy: {err}"))?;
    let shared = Arc::new(Mutex::new(ProxyState {
        root: config.root.clone(),
        last_known_good: Some(config.clone()),
        last_switch_error: None,
        recovery_count: 0,
        config,
    }));
    let initial_config = {
        shared
            .lock()
            .expect("proxy state lock poisoned")
            .config
            .clone()
    };
    let telemetry = Arc::new(Mutex::new(RuntimeTelemetry {
        active_model: initial_config.active_model.clone(),
        base_model: runtime_state::load_or_resolve_runtime_state(&initial_config.root)
            .ok()
            .and_then(|state| {
                state
                    .base_model
                    .or(state.requested_model)
                    .or(state.active_model)
            }),
        boost_model: runtime_state::load_or_resolve_runtime_state(&initial_config.root)
            .ok()
            .and_then(|state| state.boost.model),
        boost_active: false,
        boost_active_until_epoch: None,
        boost_remaining_seconds: None,
        boost_reason: runtime_state::load_or_resolve_runtime_state(&initial_config.root)
            .ok()
            .and_then(|state| state.boost.reason),
        upstream_base_url: Some(initial_config.upstream_base_url.clone()),
        last_known_good_model: initial_config.active_model.clone(),
        backend_healthy: true,
        last_switch_status: Some("ready".to_string()),
        last_switch_error: None,
        recovery_count: 0,
        ..RuntimeTelemetry::default()
    }));
    let response_state = Arc::new(Mutex::new(HashMap::<String, Value>::new()));

    for request in server.incoming_requests() {
        let config = Arc::clone(&shared);
        let telemetry = Arc::clone(&telemetry);
        let response_state = Arc::clone(&response_state);
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handle_request(&config, &telemetry, &response_state, request)
        })) {
            Ok(Ok(())) => {}
            Ok(Err(err)) => eprintln!("ctox proxy error: {err}"),
            Err(panic_payload) => {
                let panic_message = if let Some(message) = panic_payload.downcast_ref::<&str>() {
                    (*message).to_string()
                } else if let Some(message) = panic_payload.downcast_ref::<String>() {
                    message.clone()
                } else {
                    "unknown panic".to_string()
                };
                eprintln!("ctox proxy request panic: {panic_message}");
            }
        }
    }

    Ok(())
}

fn handle_request(
    state: &Arc<Mutex<ProxyState>>,
    telemetry: &Arc<Mutex<RuntimeTelemetry>>,
    response_state: &Arc<Mutex<HashMap<String, Value>>>,
    mut request: tiny_http::Request,
) -> anyhow::Result<()> {
    refresh_proxy_state_from_runtime(state);
    if matches!(request.method(), Method::Get) && request.url() == "/ctox/telemetry" {
        let config = state
            .lock()
            .expect("proxy state lock poisoned")
            .config
            .clone();
        let backend_healthy = probe_upstream_health(&config);
        let snapshot = {
            let mut telemetry_state = telemetry.lock().expect("proxy telemetry lock poisoned");
            telemetry_state.active_model = config.active_model.clone();
            telemetry_state.upstream_base_url = Some(config.upstream_base_url.clone());
            telemetry_state.backend_healthy = backend_healthy;
            sync_boost_telemetry_fields(
                &mut telemetry_state,
                &config.root,
                config.active_model.as_deref(),
            );
            if !backend_healthy
                && telemetry_state
                    .last_switch_status
                    .as_deref()
                    .is_some_and(|status| matches!(status, "ready" | "switched" | "recovered"))
            {
                telemetry_state.last_switch_status = Some("backend_unhealthy".to_string());
            }
            telemetry_state.load_observation_path =
                load_observation_path(&config.root, &config.upstream_base_url)
                    .map(|path| path.display().to_string());
            telemetry_state.load_observation =
                read_load_observation(&config.root, &config.upstream_base_url);
            telemetry_state.clone()
        };
        let response = Response::from_string(serde_json::to_string(&snapshot)?)
            .with_status_code(StatusCode(200))
            .with_header(json_header());
        request
            .respond(response)
            .context("failed to write proxy telemetry response")?;
        return Ok(());
    }

    if matches!(request.method(), Method::Post) && request.url() == "/ctox/switch" {
        let response = Response::from_string(
            serde_json::json!({
                "ok": false,
                "error": {
                    "message": "the HTTP boundary adapter no longer owns runtime switching; use `ctox runtime switch <model> <quality|performance>`"
                }
            })
            .to_string(),
        )
        .with_status_code(StatusCode(410))
        .with_header(json_header());
        request
            .respond(response)
            .context("failed to write proxy switch response")?;
        return Ok(());
    }

    let started = Instant::now();
    let method = request.method().as_str().to_string();
    let url = request.url().to_string();
    let config = state
        .lock()
        .expect("proxy state lock poisoned")
        .config
        .clone();
    let mut body = Vec::new();
    request
        .as_reader()
        .read_to_end(&mut body)
        .context("failed to read proxy request body")?;

    let mut materialized_request =
        if matches!(request.method(), Method::Post) && url == "/v1/responses" && !body.is_empty() {
            let previous_conversation = serde_json::from_slice::<Value>(&body)
                .ok()
                .and_then(|payload| {
                    payload
                        .get("previous_response_id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                })
                .and_then(|response_id| {
                    response_state
                        .lock()
                        .expect("proxy response state lock poisoned")
                        .get(&response_id)
                        .cloned()
                });
            Some(engine::materialize_responses_request(
                &body,
                previous_conversation.as_ref(),
            )?)
        } else {
            None
        };

    let mut effective_body = materialized_request
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()?
        .unwrap_or_else(|| body.clone());
    effective_body = if matches!(request.method(), Method::Post) && !effective_body.is_empty() {
        rewrite_auxiliary_request_body(&config, &url, &effective_body)
    } else {
        effective_body
    };
    let uses_openai_upstream = matches!(request.method(), Method::Post)
        && url == "/v1/responses"
        && !body.is_empty()
        && runtime_state::is_openai_compatible_api_upstream(&config.upstream_base_url);
    let allow_openai_native_web_search_passthrough = uses_openai_upstream
        && serde_json::from_slice::<Value>(&effective_body)
            .ok()
            .map(|payload| web_search::should_passthrough_openai_web_search(&config.root, &payload))
            .unwrap_or(false);
    let mut web_search_augmentation = None;
    if matches!(request.method(), Method::Post)
        && url == "/v1/responses"
        && !effective_body.is_empty()
        && !allow_openai_native_web_search_passthrough
    {
        if let Ok(mut payload) = serde_json::from_slice::<Value>(&effective_body) {
            web_search_augmentation =
                web_search::augment_responses_request(&config.root, &mut payload)?;
            if web_search_augmentation.is_some() {
                effective_body = serde_json::to_vec(&payload)
                    .context("failed to encode web-search-augmented request")?;
                materialized_request = Some(payload);
            }
        }
    }

    // Vision preprocessor: if any input item carries an `input_image` /
    // `image_url` content block and the primary model isn't vision-capable,
    // describe the image via the Qwen3-VL-2B aux and replace the image
    // block with a text block. This guarantees tools (view_image etc.) can
    // always evaluate images regardless of the primary model's capability.
    if matches!(request.method(), Method::Post)
        && url == "/v1/responses"
        && !effective_body.is_empty()
    {
        if let Ok(mut payload) = serde_json::from_slice::<Value>(&effective_body) {
            let mutated = vision_preprocessor::preprocess_responses_payload(
                &config.root,
                config.active_model.as_deref(),
                &mut payload,
            )
            .unwrap_or_else(|err| {
                eprintln!("ctox vision preprocessor failed: {err:#}");
                false
            });
            if mutated {
                effective_body = serde_json::to_vec(&payload)
                    .context("failed to encode vision-preprocessed request")?;
                materialized_request = Some(payload);
            }
        }
    }

    if matches!(request.method(), Method::Get)
        && url == "/v1/models"
        && config.upstream_socket_path.is_some()
        && !uses_remote_api_upstream(&config.upstream_base_url)
    {
        let response = Response::from_string(local_models_payload(&config).to_string())
            .with_status_code(StatusCode(200))
            .with_header(json_header());
        request
            .respond(response)
            .context("failed to write local proxy models response")?;
        return Ok(());
    }

    if matches!(request.method(), Method::Post)
        && url == "/v1/responses"
        && !effective_body.is_empty()
        && !uses_openai_upstream
        && config.upstream_socket_path.is_some()
    {
        return relay_local_socket_response(
            &config,
            telemetry,
            response_state,
            request,
            materialized_request,
            effective_body,
            web_search_augmentation.as_ref(),
            &url,
            started,
        );
    }

    let adapter_route: Option<ResolvedResponsesAdapterRoute> =
        if matches!(request.method(), Method::Post) && url == "/v1/responses" && !body.is_empty() {
            let is_remote = uses_remote_api_upstream(&config.upstream_base_url);
            ResolvedResponsesAdapterRoute::resolve(
                config.active_model.as_deref(),
                &effective_body,
                is_remote,
            )?
        } else {
            None
        };
    let response_adapter_plan: Option<ResponsesAdapterResponsePlan> = adapter_route
        .as_ref()
        .map(ResolvedResponsesAdapterRoute::response_plan);
    eprintln!(
        "ctox proxy request method={} url={} adapter_transport={:?} adapter={}",
        method,
        url,
        response_adapter_plan
            .as_ref()
            .map(ResponsesAdapterResponsePlan::transport_kind),
        response_adapter_plan
            .as_ref()
            .map(ResponsesAdapterResponsePlan::id)
            .unwrap_or("none")
    );

    if matches!(
        url.as_str(),
        "/v1/embeddings" | "/v1/audio/transcriptions" | "/v1/audio/speech" | "/v1/audio/voices"
    ) {
        if auxiliary_backend_spec(&config, &url).is_none() {
            let response = Response::from_string(
                serde_json::json!({
                    "error": {
                        "message": format!("auxiliary backend for {} is disabled in this runtime", url)
                    }
                })
                .to_string(),
            )
            .with_status_code(StatusCode(503))
            .with_header(json_header());
            request
                .respond(response)
                .context("failed to write auxiliary backend disabled response")?;
            return Ok(());
        }
    }

    let forwarded_body = if let Some(route) = adapter_route.as_ref() {
        let rewritten = route.forwarded_body().to_vec();
        if let Ok(value) = serde_json::from_slice::<Value>(&rewritten) {
            if let Some(messages) = value.get("messages").and_then(Value::as_array) {
                let roles = messages
                    .iter()
                    .map(|message| {
                        message
                            .get("role")
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                            .to_string()
                    })
                    .collect::<Vec<_>>();
                eprintln!("ctox proxy {} message roles={roles:?}", route.id());
            }
        }
        let forwarded_text = String::from_utf8_lossy(&rewritten);
        let preview: String = forwarded_text.chars().take(4_000).collect();
        eprintln!(
            "ctox proxy {} forwarded request bytes={} preview={}",
            route.id(),
            rewritten.len(),
            preview
        );
        let request_dump_path = config.root.join("runtime/last_local_chat_request.json");
        if let Some(parent) = request_dump_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&request_dump_path, &rewritten);
        rewritten
    } else if uses_openai_upstream {
        engine::rewrite_openai_responses_request(&effective_body)?
    } else if matches!(request.method(), Method::Post) && url == "/v1/responses" && !body.is_empty()
    {
        engine::rewrite_engine_responses_request(&effective_body)?
    } else if matches!(request.method(), Method::Post) && !body.is_empty() {
        rewrite_auxiliary_request_body(&config, &url, &body)
    } else {
        body
    };

    let upstream_path = if let Some(route) = adapter_route.as_ref() {
        route.upstream_path().to_string()
    } else {
        url.clone()
    };
    eprintln!("ctox proxy upstream_path={upstream_path}");
    let upstream_url = if matches!(
        response_adapter_plan
            .as_ref()
            .map(ResponsesAdapterResponsePlan::transport_kind),
        Some(ResponsesTransportKind::CompletionTemplate)
    ) {
        config.join_url(&upstream_path)
    } else {
        config.join_routed_url(&upstream_path)
    };
    let targets_primary_local_chat_backend = matches!(request.method(), Method::Post)
        && !uses_openai_upstream
        && !matches!(
            url.as_str(),
            "/v1/embeddings" | "/v1/audio/transcriptions" | "/v1/audio/speech" | "/v1/audio/voices"
        );
    let _ = targets_primary_local_chat_backend;
    if matches!(
        response_adapter_plan
            .as_ref()
            .map(ResponsesAdapterResponsePlan::transport_kind),
        Some(ResponsesTransportKind::CompletionTemplate)
    ) {
        return relay_completion_template_response(
            state,
            &config,
            telemetry,
            response_state,
            request,
            &method,
            &upstream_url,
            materialized_request,
            forwarded_body,
            response_adapter_plan
                .as_ref()
                .expect("completion-template adapter plan should exist"),
            web_search_augmentation.as_ref(),
            &url,
            started,
        );
    }

    let agent = build_upstream_agent(&url);
    let mut upstream = agent.request(&method, &upstream_url);

    for header in request.headers() {
        let field = header.field.as_str().as_str();
        if HOP_BY_HOP_HEADERS
            .iter()
            .any(|candidate| field.eq_ignore_ascii_case(candidate))
        {
            continue;
        }
        // Local engine backends are sensitive to extra forwarded client headers
        // on /v1/responses. Keep the local bridge minimal and only preserve the
        // content type needed to parse the JSON payload. OpenAI upstream keeps
        // the broader upstream-compatible header set even when CTOX already
        // handled web_search locally.
        if !uses_openai_upstream && !field.eq_ignore_ascii_case("content-type") {
            continue;
        }
        upstream = upstream.set(field, header.value.as_str());
    }
    // For remote API upstreams (OpenAI/Anthropic/OpenRouter/MiniMax/etc.)
    // we always re-derive Authorization from the configured env key.
    // Whether the incoming agent-runtime request had its own `authorization`
    // header is irrelevant: for non-OpenAI upstreams the header-forwarding
    // loop above only preserves `content-type`, so the client header was
    // dropped on the wire anyway. Without this unconditional set, MiniMax
    // (and any other non-OpenAI upstream) gets the request without auth
    // and returns 401.
    if uses_remote_api_upstream(&config.upstream_base_url) {
        let api_key_env =
            runtime_state::api_key_env_var_for_upstream_base_url(&config.upstream_base_url);
        if let Some(api_key) = runtime_env::env_or_config(&config.root, api_key_env) {
            upstream = upstream.set("authorization", &format!("Bearer {api_key}"));
        }
    }

    let upstream_response = if forwarded_body.is_empty() {
        upstream.call()
    } else {
        upstream.send_bytes(&forwarded_body)
    };

    match upstream_response {
        Ok(response) => relay_response(
            &config,
            telemetry,
            request,
            response,
            response_adapter_plan,
            web_search_augmentation.as_ref(),
            &url,
            started,
        ),
        Err(ureq::Error::Status(_, response)) => relay_response(
            &config,
            telemetry,
            request,
            response,
            response_adapter_plan,
            web_search_augmentation.as_ref(),
            &url,
            started,
        ),
        Err(err) => {
            let response = Response::from_string(
                serde_json::json!({
                    "error": {
                        "message": err.to_string()
                    }
                })
                .to_string(),
            )
            .with_status_code(StatusCode(502))
            .with_header(json_header());
            request
                .respond(response)
                .context("failed to write proxy error response")?;
            Ok(())
        }
    }
}

fn refresh_proxy_state_from_runtime(state: &Arc<Mutex<ProxyState>>) {
    let mut guard = state.lock().expect("proxy state lock poisoned");
    let next_config = ProxyConfig::resolve_with_root(&guard.root);
    if guard.config == next_config {
        return;
    }
    guard.config = next_config.clone();
    guard.last_known_good = Some(next_config);
    guard.last_switch_error = None;
    guard.recovery_count = 0;
}

fn relay_completion_template_response(
    state: &Arc<Mutex<ProxyState>>,
    config: &ProxyConfig,
    telemetry: &Arc<Mutex<RuntimeTelemetry>>,
    response_state: &Arc<Mutex<HashMap<String, Value>>>,
    request: tiny_http::Request,
    method: &str,
    upstream_url: &str,
    materialized_request: Option<Value>,
    forwarded_body: Vec<u8>,
    response_adapter_plan: &ResponsesAdapterResponsePlan,
    web_search_augmentation: Option<&web_search::WebSearchAugmentation>,
    request_path: &str,
    started: Instant,
) -> anyhow::Result<()> {
    eprintln!(
        "ctox proxy completion-template relay start adapter={} request_path={request_path}",
        response_adapter_plan.id()
    );
    let outcome = complete_completion_template_roundtrip(
        state,
        config,
        telemetry,
        response_state,
        method,
        upstream_url,
        materialized_request,
        forwarded_body,
        response_adapter_plan,
        web_search_augmentation,
    )?;
    eprintln!(
        "ctox proxy completion-template relay roundtrip complete status={} body_bytes={}",
        outcome.status_code,
        outcome.body.len()
    );

    eprintln!("ctox proxy completion-template relay about to emit downstream response");
    relay_response_from_parts(
        config,
        telemetry,
        request,
        outcome.status_code,
        outcome.response_headers,
        outcome.body,
        Some(response_adapter_plan.clone()),
        web_search_augmentation,
        request_path,
        started,
    )?;
    eprintln!("ctox proxy completion-template relay downstream response emitted");
    Ok(())
}

fn complete_completion_template_roundtrip(
    _state: &Arc<Mutex<ProxyState>>,
    config: &ProxyConfig,
    _telemetry: &Arc<Mutex<RuntimeTelemetry>>,
    response_state: &Arc<Mutex<HashMap<String, Value>>>,
    method: &str,
    upstream_url: &str,
    materialized_request: Option<Value>,
    forwarded_body: Vec<u8>,
    response_adapter_plan: &ResponsesAdapterResponsePlan,
    web_search_augmentation: Option<&web_search::WebSearchAugmentation>,
) -> anyhow::Result<CompletionTemplateRelayResponse> {
    eprintln!(
        "ctox proxy completion-template roundtrip sending first upstream request adapter={}",
        response_adapter_plan.id()
    );
    let agent = build_upstream_agent("/v1/responses");
    let mut outcome = send_completion_template_upstream_request_with_retry(
        &agent,
        config,
        method,
        upstream_url,
        config.active_model.as_deref(),
        &forwarded_body,
    )
    .map_err(|err| anyhow::anyhow!("completion-template upstream failed: {err}"))?;
    eprintln!(
        "ctox proxy completion-template first upstream response status={} body_bytes={}",
        outcome.status_code,
        outcome.body.len()
    );

    if outcome.status_code < 400 {
        if let Some(followup_body) =
            response_adapter_plan.build_followup_request(&forwarded_body, &outcome.body)?
        {
            eprintln!(
                "ctox proxy issuing completion-template continuation adapter={}",
                response_adapter_plan.id()
            );
            outcome = send_completion_template_upstream_request_with_retry(
                &agent,
                config,
                method,
                upstream_url,
                config.active_model.as_deref(),
                &followup_body,
            )
            .map_err(|err| anyhow::anyhow!("completion-template continuation failed: {err}"))?;
            eprintln!(
                "ctox proxy continuation completion-template body={}",
                String::from_utf8_lossy(&outcome.body)
            );
            eprintln!(
                "ctox proxy completion-template continuation response status={} body_bytes={}",
                outcome.status_code,
                outcome.body.len()
            );
        }
    }

    if outcome.status_code < 400 {
        store_completion_template_response_state(
            response_state,
            materialized_request.as_ref(),
            response_adapter_plan,
            &outcome.body,
            web_search_augmentation,
        )?;
    }

    Ok(outcome)
}

fn send_completion_template_upstream_request_with_retry(
    agent: &ureq::Agent,
    config: &ProxyConfig,
    method: &str,
    upstream_url: &str,
    active_model: Option<&str>,
    body: &[u8],
) -> anyhow::Result<CompletionTemplateRelayResponse> {
    let retry_deadline = Instant::now()
        + Duration::from_secs(completion_template_upstream_startup_retry_secs(config));
    loop {
        match send_completion_template_upstream_request(
            agent,
            method,
            upstream_url,
            active_model,
            body,
        ) {
            Err(err)
                if should_retry_completion_template_upstream_connect(&err)
                    && Instant::now() < retry_deadline =>
            {
                thread::sleep(Duration::from_millis(500));
            }
            Ok(response) => {
                return completion_template_relay_response_from_ureq(response)
                    .context("failed to read upstream completion-template response");
            }
            Err(ureq::Error::Status(_, response)) => {
                return completion_template_relay_response_from_ureq(response)
                    .context("failed to read upstream completion-template error response");
            }
            Err(err) => return Err(anyhow::anyhow!(err.to_string())),
        }
    }
}

fn completion_template_upstream_startup_retry_secs(config: &ProxyConfig) -> u64 {
    let _ = config;
    0
}

fn should_retry_completion_template_upstream_connect(err: &ureq::Error) -> bool {
    match err {
        ureq::Error::Transport(transport) => {
            let text = transport.to_string();
            text.contains("Connection refused")
                || text.contains("Connect error")
                || text.contains("Connection reset by peer")
        }
        _ => false,
    }
}

fn build_upstream_agent(request_path: &str) -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new();
    if request_path == "/v1/audio/transcriptions" {
        builder = builder
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(120))
            .timeout_write(Duration::from_secs(120));
    }
    builder.build()
}

fn send_completion_template_upstream_request(
    agent: &ureq::Agent,
    method: &str,
    upstream_url: &str,
    active_model: Option<&str>,
    body: &[u8],
) -> Result<ureq::Response, ureq::Error> {
    let mut request = agent.request(method, upstream_url);
    request = request.set("content-type", "application/json");
    if let Some(active_model) = active_model {
        request = request.set("x-ctox-active-model", active_model);
    }
    if body.is_empty() {
        request.call()
    } else {
        request.send_bytes(body)
    }
}

fn completion_template_relay_response_from_ureq(
    response: ureq::Response,
) -> anyhow::Result<CompletionTemplateRelayResponse> {
    const COMPLETION_TEMPLATE_READ_RETRY_WINDOW: Duration = Duration::from_secs(2);

    let status_code = response.status();
    let expected_body_len = response
        .header("Content-Length")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0);
    let response_headers = response
        .headers_names()
        .into_iter()
        .filter(|header_name| {
            !HOP_BY_HOP_HEADERS
                .iter()
                .any(|candidate| header_name.eq_ignore_ascii_case(candidate))
        })
        .flat_map(|header_name| {
            response
                .all(&header_name)
                .into_iter()
                .map(move |header_value| (header_name.clone(), header_value.to_string()))
        })
        .collect();
    let mut body = Vec::new();
    let mut reader = response.into_reader();
    let mut chunk = [0_u8; 8192];
    let read_retry_deadline = Instant::now() + COMPLETION_TEMPLATE_READ_RETRY_WINDOW;
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => {
                if expected_body_len.is_some_and(|expected| body.len() < expected)
                    && Instant::now() < read_retry_deadline
                {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                break;
            }
            Ok(read) => {
                body.extend_from_slice(&chunk[..read]);
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::InvalidInput
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                if expected_body_len.is_some_and(|expected| body.len() >= expected) {
                    break;
                }
                if expected_body_len.is_some() && !body.is_empty() {
                    break;
                }
                if Instant::now() < read_retry_deadline {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                if !body.is_empty() {
                    break;
                }
                return Err(err).context("failed to read completion-template response body");
            }
            Err(_err) if !body.is_empty() => {
                break;
            }
            Err(err) => {
                return Err(err).context("failed to read completion-template response body")
            }
        }
    }
    Ok(CompletionTemplateRelayResponse {
        status_code,
        response_headers,
        body,
    })
}

fn store_completion_template_response_state(
    response_state: &Arc<Mutex<HashMap<String, Value>>>,
    materialized_request: Option<&Value>,
    response_adapter_plan: &ResponsesAdapterResponsePlan,
    completion_body: &[u8],
    web_search_augmentation: Option<&web_search::WebSearchAugmentation>,
) -> anyhow::Result<()> {
    let Some(materialized_request) = materialized_request else {
        return Ok(());
    };
    let mut response_payload: Value = serde_json::from_slice(
        &response_adapter_plan.rewrite_success_response(completion_body, None)?,
    )
    .context("failed to parse rewritten responses payload for proxy state")?;
    if let Some(augmentation) = web_search_augmentation {
        response_payload = serde_json::from_slice(&web_search::augment_responses_output(
            &serde_json::to_vec(&response_payload)?,
            augmentation,
        )?)
        .context("failed to parse augmented web-search responses payload for proxy state")?;
    }
    if let Some(response_id) = response_payload.get("id").and_then(Value::as_str) {
        let conversation =
            engine::extend_conversation_with_response(materialized_request, &response_payload)?;
        response_state
            .lock()
            .expect("proxy response state lock poisoned")
            .insert(response_id.to_string(), conversation);
    }
    Ok(())
}

#[allow(dead_code)]
fn build_completion_template_terminal_sse(
    completion_body: &[u8],
    response_adapter_plan: &ResponsesAdapterResponsePlan,
    web_search_augmentation: Option<&web_search::WebSearchAugmentation>,
    status_code: u16,
) -> anyhow::Result<Vec<u8>> {
    if status_code >= 400 {
        return Ok(sse_error_frame(&extract_completion_template_error_message(
            completion_body,
        )));
    }
    let json_payload = response_adapter_plan.rewrite_success_response(completion_body, None)?;
    let json_payload = if let Some(augmentation) = web_search_augmentation {
        web_search::augment_responses_output(&json_payload, augmentation)?
    } else {
        json_payload
    };
    engine::rewrite_responses_payload_to_sse(&json_payload)
}

#[allow(dead_code)]
fn extract_completion_template_error_message(body: &[u8]) -> String {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string())
}

#[allow(dead_code)]
fn sse_error_frame(message: &str) -> Vec<u8> {
    format!(
        "event: error\ndata: {}\n\ndata: [DONE]\n\n",
        serde_json::json!({
            "error": {
                "message": message
            }
        })
    )
    .into_bytes()
}

pub fn boost_status(root: &std::path::Path) -> anyhow::Result<BoostStatus> {
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
    root: &std::path::Path,
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

pub fn stop_boost_lease(root: &std::path::Path) -> anyhow::Result<BoostStatus> {
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
    root: &std::path::Path,
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
    root: &std::path::Path,
    model: &str,
    preset: Option<&str>,
) -> anyhow::Result<RuntimeSwitchResponse> {
    apply_runtime_switch(root, model, preset)
}

fn fetch_runtime_telemetry(root: &std::path::Path) -> anyhow::Result<RuntimeTelemetry> {
    Ok(synthesize_runtime_telemetry(root))
}

pub fn current_runtime_telemetry(root: &std::path::Path) -> RuntimeTelemetry {
    synthesize_runtime_telemetry(root)
}

fn apply_runtime_switch(
    root: &std::path::Path,
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

fn synthesize_runtime_telemetry(root: &std::path::Path) -> RuntimeTelemetry {
    let config = ProxyConfig::resolve_with_root(root);
    let backend_healthy = probe_upstream_health(&config);
    let mut telemetry = RuntimeTelemetry {
        active_model: config.active_model.clone(),
        upstream_base_url: Some(config.upstream_base_url.clone()),
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
    telemetry.load_observation_path = load_observation_path(root, &config.upstream_base_url)
        .map(|path| path.display().to_string());
    telemetry.load_observation = read_load_observation(root, &config.upstream_base_url);
    telemetry
}

fn relay_local_socket_response(
    config: &ProxyConfig,
    telemetry: &Arc<Mutex<RuntimeTelemetry>>,
    response_state: &Arc<Mutex<HashMap<String, Value>>>,
    request: tiny_http::Request,
    materialized_request: Option<Value>,
    request_body: Vec<u8>,
    web_search_augmentation: Option<&web_search::WebSearchAugmentation>,
    request_path: &str,
    started: Instant,
) -> anyhow::Result<()> {
    {
        let transport = config
            .upstream_transport()
            .context("missing local transport for proxy runtime")?;
        let stream_requested = engine::responses_request_streams(&request_body).unwrap_or(false);
        let terminal = match complete_local_socket_roundtrip(&transport, &request_body) {
            Ok(terminal) => terminal,
            Err(err) => {
                let message = err.to_string();
                let downstream_body = if stream_requested {
                    sse_error_frame(&message)
                } else {
                    serde_json::json!({
                        "error": { "message": message }
                    })
                    .to_string()
                    .into_bytes()
                };
                let content_type = if stream_requested {
                    "text/event-stream"
                } else {
                    "application/json"
                };
                return relay_response_from_parts(
                    config,
                    telemetry,
                    request,
                    502,
                    vec![("content-type".to_string(), content_type.to_string())],
                    downstream_body,
                    None,
                    None,
                    request_path,
                    started,
                );
            }
        };
        match terminal {
            LocalSocketTerminal::Completed(mut response_payload) => {
                if let Some(augmentation) = web_search_augmentation {
                    response_payload =
                        serde_json::from_slice(&web_search::augment_responses_output(
                            &serde_json::to_vec(&response_payload)?,
                            augmentation,
                        )?)?;
                }
                store_local_socket_response_state(
                    response_state,
                    materialized_request.as_ref(),
                    &response_payload,
                )?;
                let response_body = serde_json::to_vec(&response_payload)?;
                let downstream_body = if stream_requested {
                    engine::rewrite_responses_payload_to_sse(&response_body)?
                } else {
                    response_body
                };
                let content_type = if stream_requested {
                    "text/event-stream"
                } else {
                    "application/json"
                };
                relay_response_from_parts(
                    config,
                    telemetry,
                    request,
                    200,
                    vec![("content-type".to_string(), content_type.to_string())],
                    downstream_body,
                    None,
                    None,
                    request_path,
                    started,
                )
            }
            LocalSocketTerminal::Failed(response_payload) => {
                let message = response_payload
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("local responses socket returned a failed response")
                    .to_string();
                let status_code = 502;
                let response_body = serde_json::to_vec(&response_payload)?;
                let downstream_body = if stream_requested {
                    sse_error_frame(&message)
                } else {
                    response_body
                };
                let content_type = if stream_requested {
                    "text/event-stream"
                } else {
                    "application/json"
                };
                relay_response_from_parts(
                    config,
                    telemetry,
                    request,
                    status_code,
                    vec![("content-type".to_string(), content_type.to_string())],
                    downstream_body,
                    None,
                    None,
                    request_path,
                    started,
                )
            }
        }
    }
}

enum LocalSocketTerminal {
    Completed(Value),
    Failed(Value),
}

fn complete_local_socket_roundtrip(
    transport: &LocalTransport,
    request_body: &[u8],
) -> anyhow::Result<LocalSocketTerminal> {
    let timeout = Duration::from_secs(300);
    let label = transport.display_label();
    let mut stream = transport
        .connect_blocking(timeout)
        .with_context(|| format!("failed to connect via {label}"))?;
    stream
        .write_all(request_body)
        .with_context(|| format!("failed to write request via {label}"))?;
    stream
        .write_all(b"\n")
        .with_context(|| format!("failed to terminate request via {label}"))?;
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
            anyhow::bail!("responses socket closed before terminal event");
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let event: Value =
            serde_json::from_str(trimmed).context("failed to parse responses socket event")?;
        match event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "response.completed" => {
                let response = event
                    .get("response")
                    .cloned()
                    .context("completed socket response missing response payload")?;
                return Ok(LocalSocketTerminal::Completed(response));
            }
            "response.failed" => {
                let response = event.get("response").cloned().unwrap_or_else(|| {
                    serde_json::json!({
                        "error": {
                            "message": "local responses socket returned a failed response"
                        }
                    })
                });
                return Ok(LocalSocketTerminal::Failed(response));
            }
            _ => {}
        }
    }
}

fn store_local_socket_response_state(
    response_state: &Arc<Mutex<HashMap<String, Value>>>,
    materialized_request: Option<&Value>,
    response_payload: &Value,
) -> anyhow::Result<()> {
    let Some(materialized_request) = materialized_request else {
        return Ok(());
    };
    let Some(response_id) = response_payload.get("id").and_then(Value::as_str) else {
        return Ok(());
    };
    let conversation =
        engine::extend_conversation_with_response(materialized_request, response_payload)?;
    response_state
        .lock()
        .expect("proxy response state lock poisoned")
        .insert(response_id.to_string(), conversation);
    Ok(())
}

fn relay_response(
    config: &ProxyConfig,
    telemetry: &Arc<Mutex<RuntimeTelemetry>>,
    request: tiny_http::Request,
    response: ureq::Response,
    response_adapter_plan: Option<ResponsesAdapterResponsePlan>,
    web_search_augmentation: Option<&web_search::WebSearchAugmentation>,
    request_path: &str,
    started: Instant,
) -> anyhow::Result<()> {
    let status = StatusCode(response.status());
    let response_headers: Vec<(String, String)> = response
        .headers_names()
        .into_iter()
        .filter(|header_name| {
            !HOP_BY_HOP_HEADERS
                .iter()
                .any(|candidate| header_name.eq_ignore_ascii_case(candidate))
        })
        .flat_map(|header_name| {
            response
                .all(&header_name)
                .into_iter()
                .map(move |header_value| (header_name.clone(), header_value.to_string()))
        })
        .collect();
    let mut body = Vec::new();
    let mut reader = response.into_reader();
    reader
        .read_to_end(&mut body)
        .context("failed to read upstream proxy response body")?;
    relay_response_from_parts(
        config,
        telemetry,
        request,
        status.0,
        response_headers,
        body,
        response_adapter_plan,
        web_search_augmentation,
        request_path,
        started,
    )
}

fn relay_response_from_parts(
    config: &ProxyConfig,
    telemetry: &Arc<Mutex<RuntimeTelemetry>>,
    request: tiny_http::Request,
    status_code: u16,
    response_headers: Vec<(String, String)>,
    body: Vec<u8>,
    response_adapter_plan: Option<ResponsesAdapterResponsePlan>,
    web_search_augmentation: Option<&web_search::WebSearchAugmentation>,
    request_path: &str,
    started: Instant,
) -> anyhow::Result<()> {
    let status = StatusCode(status_code);
    eprintln!(
        "ctox proxy relay_response_from_parts status={} adapter_transport={:?} adapter={} body_bytes={}",
        status.0,
        response_adapter_plan
            .as_ref()
            .map(ResponsesAdapterResponsePlan::transport_kind),
        response_adapter_plan
            .as_ref()
            .map(ResponsesAdapterResponsePlan::id)
            .unwrap_or("none"),
        body.len()
    );
    if let Some(plan) = response_adapter_plan.as_ref() {
        eprintln!(
            "ctox proxy {} upstream status={} body={}",
            plan.id(),
            status.0,
            String::from_utf8_lossy(&body)
        );
    }
    let mut content_type_override: Option<&'static str> = None;
    let mut body = match (response_adapter_plan.as_ref(), status.0 < 400) {
        (Some(plan), true) => {
            eprintln!(
                "ctox proxy {} upstream body={}",
                plan.id(),
                String::from_utf8_lossy(&body)
            );
            let json_payload =
                plan.rewrite_success_response(&body, config.active_model.as_deref())?;
            if plan.stream() {
                let json_payload = if let Some(augmentation) = web_search_augmentation {
                    web_search::augment_responses_output(&json_payload, augmentation)?
                } else {
                    json_payload
                };
                content_type_override = Some("text/event-stream");
                engine::rewrite_responses_payload_to_sse(&json_payload)?
            } else {
                content_type_override = Some("application/json");
                json_payload
            }
        }
        _ => body,
    };
    if status.0 < 400
        && !response_adapter_plan
            .as_ref()
            .map(ResponsesAdapterResponsePlan::stream)
            .unwrap_or(false)
    {
        if let Some(augmentation) = web_search_augmentation {
            body = web_search::augment_responses_output(&body, augmentation)?;
        }
    }
    update_proxy_telemetry(
        telemetry,
        config,
        request_path,
        status.0,
        &body,
        started.elapsed().as_millis() as u64,
    );

    let mut tiny_response = Response::from_data(body).with_status_code(status);
    for (header_name, header_value) in response_headers {
        if content_type_override.is_some() && header_name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        if let Ok(header) = Header::from_bytes(header_name.as_bytes(), header_value.as_bytes()) {
            tiny_response = tiny_response.with_header(header);
        }
    }
    if let Some(content_type) = content_type_override {
        if let Ok(header) = Header::from_bytes(b"content-type", content_type.as_bytes()) {
            tiny_response = tiny_response.with_header(header);
        }
    }
    eprintln!(
        "ctox proxy writing downstream response status={} content_type_override={:?}",
        status.0, content_type_override
    );
    request
        .respond(tiny_response)
        .context("failed to write proxy response")?;
    eprintln!("ctox proxy downstream response write finished");
    Ok(())
}

fn json_header() -> Header {
    Header::from_bytes(b"content-type", b"application/json").expect("static content-type header")
}

fn rewrite_auxiliary_request_body(
    config: &ProxyConfig,
    request_path: &str,
    body: &[u8],
) -> Vec<u8> {
    let Some(model) = config.routed_model(request_path) else {
        return body.to_vec();
    };
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(object) = value.as_object_mut() else {
        return body.to_vec();
    };
    let should_override = match object.get("model") {
        None => true,
        Some(Value::String(existing)) => {
            existing.trim().is_empty()
                || existing == "default"
                || request_path == "/v1/audio/speech"
                || request_path == "/v1/embeddings"
        }
        _ => true,
    };
    if should_override {
        object.insert("model".to_string(), Value::String(model.to_string()));
    }
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

fn probe_upstream_health(config: &ProxyConfig) -> bool {
    if uses_remote_api_upstream(&config.upstream_base_url) {
        return runtime_env::env_or_config(
            &config.root,
            runtime_state::api_key_env_var_for_upstream_base_url(&config.upstream_base_url),
        )
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    }
    if let Some(transport) = config.upstream_transport() {
        return transport.probe();
    }
    probe_backend_health_url(&config.join_url("/health"))
}

fn probe_backend_health_url(health_url: &str) -> bool {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(1))
        .timeout_read(std::time::Duration::from_secs(2))
        .timeout_write(std::time::Duration::from_secs(2))
        .build();

    match agent.get(health_url).call() {
        Ok(response) => response.status() < 500,
        Err(ureq::Error::Status(code, _)) => code < 500,
        Err(_) => false,
    }
}

fn load_observation_path(root: &std::path::Path, upstream_base_url: &str) -> Option<PathBuf> {
    if uses_remote_api_upstream(upstream_base_url) {
        return None;
    }
    let port_slug = upstream_base_url.rsplit(':').next()?.trim();
    if port_slug.is_empty() {
        return None;
    }
    Some(
        root.join("runtime")
            .join(format!("load_observation_{port_slug}.json")),
    )
}

fn read_load_observation(
    root: &std::path::Path,
    upstream_base_url: &str,
) -> Option<LoadObservation> {
    let path = load_observation_path(root, upstream_base_url)?;
    let raw = std::fs::read(&path).ok()?;
    serde_json::from_slice(&raw).ok()
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
struct AuxiliaryBackendSpec<'a> {
    model: &'a str,
    base_url: &'a str,
    role: engine::AuxiliaryRole,
    health_path: &'static str,
}

impl<'a> AuxiliaryBackendSpec<'a> {
    #[cfg_attr(not(test), allow(dead_code))]
    fn health_url(self) -> String {
        format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            self.health_path
        )
    }
}

fn auxiliary_backend_spec<'a>(
    config: &'a ProxyConfig,
    request_path: &str,
) -> Option<AuxiliaryBackendSpec<'a>> {
    match request_path {
        "/v1/embeddings" => Some(AuxiliaryBackendSpec {
            model: config.embedding_model.as_deref()?,
            base_url: &config.embedding_base_url,
            role: engine::AuxiliaryRole::Embedding,
            health_path: "/health",
        }),
        "/v1/audio/transcriptions" => {
            let selection = engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Stt,
                config.transcription_model.as_deref(),
            );
            Some(AuxiliaryBackendSpec {
                model: config.transcription_model.as_deref()?,
                base_url: &config.transcription_base_url,
                role: engine::AuxiliaryRole::Stt,
                health_path: if selection.backend_kind == engine::AuxiliaryBackendKind::Speaches {
                    "/v1/models"
                } else {
                    "/health"
                },
            })
        }
        "/v1/audio/speech" | "/v1/audio/voices" => {
            let selection = engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Tts,
                config.speech_model.as_deref(),
            );
            Some(AuxiliaryBackendSpec {
                model: config.speech_model.as_deref()?,
                base_url: &config.speech_base_url,
                role: engine::AuxiliaryRole::Tts,
                health_path: if selection.backend_kind == engine::AuxiliaryBackendKind::Speaches {
                    "/v1/models"
                } else {
                    "/health"
                },
            })
        }
        _ => None,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn ensure_auxiliary_backend_ready(config: &ProxyConfig, request_path: &str) -> anyhow::Result<()> {
    let Some(spec) = auxiliary_backend_spec(config, request_path) else {
        return Ok(());
    };
    let health_url = spec.health_url();
    if probe_backend_health_url(&health_url) {
        return Ok(());
    }
    let startup_wait_secs = supervisor::backend_startup_wait_secs_for_model(Some(spec.model));
    supervisor::ensure_auxiliary_backend_ready(&config.root, spec.role, false)?;
    for _ in 0..startup_wait_secs {
        if probe_backend_health_url(&health_url) {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }
    anyhow::bail!(
        "auxiliary backend for model {} is not reachable at {} after startup",
        spec.model,
        spec.base_url
    )
}

#[cfg_attr(not(test), allow(dead_code))]
fn same_backend(left: &ProxyConfig, right: &ProxyConfig) -> bool {
    left.upstream_base_url == right.upstream_base_url
        && left.upstream_socket_path == right.upstream_socket_path
        && left.active_model == right.active_model
}

fn update_proxy_telemetry(
    telemetry: &Arc<Mutex<RuntimeTelemetry>>,
    config: &ProxyConfig,
    request_path: &str,
    status: u16,
    body: &[u8],
    latency_ms: u64,
) {
    if status >= 400 || !(request_path == "/v1/responses" || request_path == "/v1/completions") {
        return;
    }
    let parsed = extract_usage_telemetry(body);
    let mut state = telemetry.lock().expect("proxy telemetry lock poisoned");
    state.active_model = parsed
        .as_ref()
        .and_then(|usage| usage.model.clone())
        .or_else(|| config.active_model.clone());
    state.upstream_base_url = Some(config.upstream_base_url.clone());
    state.backend_healthy = true;
    state.last_request_path = Some(request_path.to_string());
    state.last_response_at = Some(iso_now());
    state.last_latency_ms = Some(latency_ms);
    if let Some(usage) = parsed {
        state.last_input_tokens = Some(usage.input_tokens);
        state.last_output_tokens = Some(usage.output_tokens);
        state.last_total_tokens = Some(usage.total_tokens);
        state.last_tokens_per_second = if latency_ms == 0 {
            None
        } else {
            Some((usage.output_tokens as f64) / ((latency_ms as f64) / 1000.0))
        };
    }
}

#[derive(Debug)]
struct UsageTelemetry {
    model: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

fn extract_usage_telemetry(body: &[u8]) -> Option<UsageTelemetry> {
    let text = String::from_utf8_lossy(body);
    if text.trim_start().starts_with("data: ") {
        return extract_usage_from_sse(&text);
    }
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    extract_usage_from_json(&value)
}

fn extract_usage_from_sse(sse: &str) -> Option<UsageTelemetry> {
    for line in sse.lines().rev() {
        let trimmed = line.trim();
        if !trimmed.starts_with("data: ") {
            continue;
        }
        let payload = trimmed.trim_start_matches("data: ").trim();
        if payload == "[DONE]" || payload.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(payload).ok()?;
        if value.get("type").and_then(serde_json::Value::as_str) == Some("response.completed") {
            return extract_usage_from_json(value.get("response")?);
        }
    }
    None
}

fn extract_usage_from_json(value: &serde_json::Value) -> Option<UsageTelemetry> {
    let usage = value.get("usage")?;
    let input_tokens = usage
        .get("input_tokens")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            usage
                .get("prompt_tokens")
                .and_then(serde_json::Value::as_u64)
        })
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            usage
                .get("completion_tokens")
                .and_then(serde_json::Value::as_u64)
        })
        .unwrap_or(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(input_tokens + output_tokens);
    Some(UsageTelemetry {
        model: value
            .get("model")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        input_tokens,
        output_tokens,
        total_tokens,
    })
}

fn iso_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    chrono::DateTime::<chrono::Utc>::from_timestamp(now as i64, 0)
        .map(|timestamp| timestamp.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    fn proxy_config(model: &str, upstream_base_url: &str) -> ProxyConfig {
        ProxyConfig {
            root: PathBuf::from("/tmp/ctox"),
            listen_host: "127.0.0.1".to_string(),
            listen_port: 12434,
            upstream_base_url: upstream_base_url.to_string(),
            upstream_socket_path: None,
            active_model: Some(model.to_string()),
            embedding_base_url: "http://127.0.0.1:1237".to_string(),
            embedding_model: Some("Qwen/Qwen3-Embedding-0.6B".to_string()),
            transcription_base_url: "http://127.0.0.1:1238".to_string(),
            transcription_model: Some("engineai/Voxtral-Mini-4B-Realtime-2602".to_string()),
            speech_base_url: "http://127.0.0.1:1239".to_string(),
            speech_model: Some("engineai/Voxtral-4B-TTS-2603".to_string()),
        }
    }

    #[test]
    fn switch_target_uses_model_family_runtime_port() {
        let gpt_oss = proxy_config("openai/gpt-oss-20b", "http://127.0.0.1:1234");
        let qwen_runtime = engine::runtime_config_for_model("Qwen/Qwen3.5-35B-A3B").unwrap();
        let qwen = ProxyConfig {
            upstream_base_url: format!("http://127.0.0.1:{}", qwen_runtime.port),
            active_model: Some(qwen_runtime.model),
            ..gpt_oss.clone()
        };

        assert_eq!(qwen.upstream_base_url, "http://127.0.0.1:1235");
        assert_ne!(gpt_oss.upstream_base_url, qwen.upstream_base_url);
    }

    #[test]
    fn same_backend_requires_matching_model_and_upstream() {
        let left = proxy_config("openai/gpt-oss-20b", "http://127.0.0.1:1234");
        let same = proxy_config("openai/gpt-oss-20b", "http://127.0.0.1:1234");
        let different_model = ProxyConfig {
            active_model: Some("Qwen/Qwen3.5-35B-A3B".to_string()),
            ..same.clone()
        };

        assert!(same_backend(&left, &same));
        assert!(!same_backend(&left, &different_model));
    }

    #[test]
    fn auxiliary_routes_select_dedicated_upstreams() {
        let config = proxy_config("openai/gpt-oss-20b", "http://127.0.0.1:1234");

        assert_eq!(
            config.join_routed_url("/v1/responses"),
            "http://127.0.0.1:1234/v1/responses"
        );
        assert_eq!(
            config.join_routed_url("/v1/embeddings"),
            "http://127.0.0.1:1237/v1/embeddings"
        );
        assert_eq!(
            config.join_routed_url("/v1/audio/transcriptions"),
            "http://127.0.0.1:1238/v1/audio/transcriptions"
        );
        assert_eq!(
            config.join_routed_url("/v1/audio/speech"),
            "http://127.0.0.1:1239/v1/audio/speech"
        );
        assert_eq!(
            config.join_routed_url("/v1/audio/voices"),
            "http://127.0.0.1:1239/v1/audio/voices"
        );
    }

    #[test]
    fn auxiliary_backend_specs_match_health_paths_and_launchers() {
        let config = proxy_config("openai/gpt-oss-20b", "http://127.0.0.1:1234");

        let embedding = auxiliary_backend_spec(&config, "/v1/embeddings").unwrap();
        assert_eq!(embedding.role, engine::AuxiliaryRole::Embedding);
        assert_eq!(embedding.health_url(), "http://127.0.0.1:1237/health");

        let stt = auxiliary_backend_spec(&config, "/v1/audio/transcriptions").unwrap();
        assert_eq!(stt.role, engine::AuxiliaryRole::Stt);
        assert_eq!(stt.health_url(), "http://127.0.0.1:1238/health");

        let tts = auxiliary_backend_spec(&config, "/v1/audio/speech").unwrap();
        assert_eq!(tts.role, engine::AuxiliaryRole::Tts);
        assert_eq!(tts.health_url(), "http://127.0.0.1:1239/health");

        let cpu_tts = ProxyConfig {
            speech_model: Some("speaches-ai/piper-en_US-lessac-medium".to_string()),
            ..config
        };
        let cpu_tts_spec = auxiliary_backend_spec(&cpu_tts, "/v1/audio/speech").unwrap();
        assert_eq!(cpu_tts_spec.health_url(), "http://127.0.0.1:1239/v1/models");
    }

    #[test]
    fn responses_requests_without_model_inherit_active_local_model() {
        let config = proxy_config("Qwen/Qwen3.5-4B", "http://127.0.0.1:1235");
        let body =
            br#"{"input":"Reply with CTOX_MATRIX_OK and nothing else.","max_output_tokens":24}"#;

        let rewritten = rewrite_auxiliary_request_body(&config, "/v1/responses", body);
        let payload: Value = serde_json::from_slice(&rewritten).unwrap();

        assert_eq!(
            payload.get("model").and_then(Value::as_str),
            Some("Qwen/Qwen3.5-4B")
        );
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
        std::fs::write(
            root.join("runtime/engine.env"),
            "CTOX_CHAT_SOURCE=local\nCTOX_CHAT_RUNTIME_PLAN_ACTIVE=1\nCTOX_ENGINE_MODEL=Qwen/Qwen3.5-27B\nCTOX_ENGINE_PAGED_ATTN=auto\nCTOX_ENGINE_DEVICE_LAYERS=0:40;1:24\n",
        )
        .unwrap();

        let config = ProxyConfig {
            root: root.clone(),
            listen_host: "127.0.0.1".to_string(),
            listen_port: 12434,
            upstream_base_url: OPENAI_RESPONSES_BASE_URL.to_string(),
            upstream_socket_path: None,
            active_model: Some("gpt-5.4".to_string()),
            embedding_base_url: "http://127.0.0.1:1237".to_string(),
            embedding_model: Some("Qwen/Qwen3-Embedding-0.6B".to_string()),
            transcription_base_url: "http://127.0.0.1:1238".to_string(),
            transcription_model: Some("engineai/Voxtral-Mini-4B-Realtime-2602".to_string()),
            speech_base_url: "http://127.0.0.1:1239".to_string(),
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
    fn local_proxy_config_prefers_runtime_port_override() {
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
        std::fs::write(
            root.join("runtime/engine.env"),
            "CTOX_ACTIVE_MODEL=openai/gpt-oss-20b\nCTOX_CHAT_MODEL_BASE=openai/gpt-oss-20b\nCTOX_CHAT_SOURCE=local\nCTOX_ENGINE_PORT=2235\nCTOX_PROXY_PORT=22434\n",
        )
        .unwrap();

        let config = ProxyConfig::resolve_with_root(&root);

        assert_eq!(config.active_model.as_deref(), Some("openai/gpt-oss-20b"));
        assert_eq!(config.listen_port, 22434);
        assert_eq!(config.upstream_base_url, "http://127.0.0.1:2235");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn local_proxy_config_ignores_stale_upstream_override() {
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
        std::fs::write(
            root.join("runtime/engine.env"),
            "CTOX_ACTIVE_MODEL=openai/gpt-oss-20b\nCTOX_CHAT_MODEL_BASE=openai/gpt-oss-20b\nCTOX_CHAT_SOURCE=local\nCTOX_ENGINE_PORT=2235\nCTOX_PROXY_PORT=22434\nCTOX_UPSTREAM_BASE_URL=http://127.0.0.1:1235\n",
        )
        .unwrap();

        let config = ProxyConfig::resolve_with_root(&root);

        assert_eq!(config.active_model.as_deref(), Some("openai/gpt-oss-20b"));
        assert_eq!(config.listen_port, 22434);
        assert_eq!(config.upstream_base_url, "http://127.0.0.1:2235");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn gemma4_adapter_roundtrip_preserves_codex_responses_shape() {
        let route = ResolvedResponsesAdapterRoute::resolve(
            Some("google/gemma-4-26B-A4B-it"),
            &serde_json::to_vec(&serde_json::json!({
                "model": "google/gemma-4-26B-A4B-it",
                "input": "bootstrap"
            }))
            .unwrap(),
            false,
        )
        .expect("gemma4 adapter route should resolve")
        .expect("gemma4 adapter should exist");
        let response_plan = route.response_plan();
        assert_eq!(route.id(), "gemma4");
        assert_eq!(route.upstream_path(), "/v1/chat/completions");

        let request = serde_json::json!({
            "model": "google/gemma-4-26B-A4B-it",
            "instructions": "You are a careful assistant.",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type":"input_text","text":"Check the weather tool."}]
                },
                {
                    "type": "function_call",
                    "call_id": "call_weather_1",
                    "name": "weather.lookup",
                    "arguments": "{\"city\":\"Berlin\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_weather_1",
                    "output": "{\"temp_c\":17}"
                }
            ],
            "tools": [{
                "type": "function",
                "name": "weather.lookup",
                "description": "Weather lookup",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "city": {"type": "string"}
                    },
                    "required": ["city"]
                }
            }],
            "reasoning": {"effort":"high"},
            "max_output_tokens": 64
        });

        let rewritten_request = ResolvedResponsesAdapterRoute::resolve(
            Some("google/gemma-4-26B-A4B-it"),
            &serde_json::to_vec(&request).unwrap(),
            false,
        )
        .expect("gemma4 request route should resolve")
        .expect("gemma4 adapter route should exist");
        let rewritten_request: Value =
            serde_json::from_slice(rewritten_request.forwarded_body()).unwrap();
        let messages = rewritten_request["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["tool_calls"][0]["id"], "call_weather_1");
        assert_eq!(
            messages[2]["tool_calls"][0]["function"]["name"],
            "weather.lookup"
        );
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["name"], "weather.lookup");
        assert_eq!(messages[3]["tool_call_id"], "call_weather_1");
        assert_eq!(rewritten_request["enable_thinking"], true);
        assert_eq!(rewritten_request["reasoning_effort"], "high");

        let upstream_response = serde_json::json!({
            "id":"gemma-chatcmpl-1",
            "object":"chat.completion",
            "created": 1_744_000_000u64,
            "model":"google/gemma-4-26B-A4B-it",
            "choices":[{
                "index":0,
                "message":{
                    "role":"assistant",
                    "content":"<|channel>thought\nNeed a weather lookup before answering.\n<channel|><|tool_call>call:weather.lookup{\"city\":\"Berlin\"}<tool_call|>"
                },
                "finish_reason":"tool_calls"
            }],
            "usage":{"prompt_tokens":32,"completion_tokens":12,"total_tokens":44}
        });

        let rewritten_response = response_plan
            .rewrite_success_response(
                &serde_json::to_vec(&upstream_response).unwrap(),
                Some("google/gemma-4-26B-A4B-it"),
            )
            .expect("gemma4 response rewrite should succeed");
        let rewritten_response: Value = serde_json::from_slice(&rewritten_response).unwrap();
        assert_eq!(rewritten_response["object"], "response");
        assert_eq!(rewritten_response["model"], "google/gemma-4-26B-A4B-it");
        assert_eq!(
            rewritten_response["reasoning"],
            "Need a weather lookup before answering."
        );
        let output = rewritten_response["output"].as_array().unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["type"], "function_call");
        assert_eq!(output[0]["name"], "weather.lookup");
        assert_eq!(output[0]["arguments"], "{\"city\":\"Berlin\"}");
        assert!(rewritten_response["output_text"].is_null());
    }

    #[test]
    fn backend_startup_lease_serializes_same_backend() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ctox_gateway_backend_lease_{}_{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();

        let first =
            supervisor::acquire_backend_startup_lease(&root, 15433, "Qwen/Qwen3.5-4B").unwrap();
        assert!(first.is_some());

        let second =
            supervisor::acquire_backend_startup_lease(&root, 15433, "Qwen/Qwen3.5-4B").unwrap();
        assert!(second.is_none());

        drop(first);

        let third =
            supervisor::acquire_backend_startup_lease(&root, 15433, "Qwen/Qwen3.5-4B").unwrap();
        assert!(third.is_some());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn request_runtime_switch_updates_runtime_without_boundary_proxy_loopback() {
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
        std::fs::write(
            root.join("runtime/engine.env"),
            "CTOX_CHAT_SOURCE=api\nCTOX_CHAT_MODEL=gpt-5.4\nCTOX_CHAT_MODEL_BASE=gpt-5.4\nCTOX_ACTIVE_MODEL=gpt-5.4\n",
        )
        .unwrap();

        let response = request_runtime_switch(&root, "gpt-5.4-mini", None).unwrap();
        let env_map = runtime_env::load_runtime_env_map(&root).unwrap();

        assert!(response.ok);
        assert_eq!(response.active_model, "gpt-5.4-mini");
        assert_eq!(
            env_map.get("CTOX_ACTIVE_MODEL").map(String::as_str),
            Some("gpt-5.4-mini")
        );
        assert!(!supervisor::boundary_proxy_is_managed(&root));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn boost_status_works_without_boundary_proxy_process() {
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
        std::fs::write(
            root.join("runtime/engine.env"),
            format!(
                "CTOX_CHAT_SOURCE=api\nCTOX_CHAT_MODEL_BASE=gpt-5.4\nCTOX_CHAT_MODEL=gpt-5.4-mini\nCTOX_ACTIVE_MODEL=gpt-5.4-mini\nCTOX_CHAT_MODEL_BOOST=gpt-5.4-mini\nCTOX_BOOST_ACTIVE_UNTIL_EPOCH={active_until}\nCTOX_BOOST_REASON=test\n"
            ),
        )
        .unwrap();

        let status = boost_status(&root).unwrap();

        assert!(status.active);
        assert_eq!(status.base_model.as_deref(), Some("gpt-5.4"));
        assert_eq!(status.boost_model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(status.active_model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(status.reason.as_deref(), Some("test"));
        assert!(!supervisor::boundary_proxy_is_managed(&root));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn proxy_state_refreshes_from_runtime_without_restart() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ctox_gateway_proxy_refresh_{}_{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        std::fs::write(
            root.join("runtime/engine.env"),
            "CTOX_CHAT_SOURCE=api\nCTOX_CHAT_MODEL=gpt-5.4\nCTOX_CHAT_MODEL_BASE=gpt-5.4\nCTOX_ACTIVE_MODEL=gpt-5.4\n",
        )
        .unwrap();

        let shared = Arc::new(Mutex::new(ProxyState {
            root: root.clone(),
            config: ProxyConfig::resolve_with_root(&root),
            last_known_good: None,
            last_switch_error: Some("stale".to_string()),
            recovery_count: 2,
        }));

        request_runtime_switch(&root, "gpt-5.4-mini", None).unwrap();
        refresh_proxy_state_from_runtime(&shared);

        let guard = shared.lock().unwrap();
        assert_eq!(guard.config.active_model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(
            guard
                .last_known_good
                .as_ref()
                .and_then(|config| config.active_model.as_deref()),
            Some("gpt-5.4-mini")
        );
        assert_eq!(guard.last_switch_error, None);
        assert_eq!(guard.recovery_count, 0);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn local_completion_template_upstream_retry_uses_internal_http_client() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut buffer = [0_u8; 4096];
            let read = std::io::Read::read(&mut stream, &mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..read]);
            assert!(request.contains("POST /v1/chat/completions HTTP/1.1"));
            assert!(request.contains("content-type: application/json"));
            assert!(request.contains("x-ctox-active-model: openai/gpt-oss-20b"));
            let body = br#"{"id":"cmpl-1","object":"chat.completion"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
            stream.flush().unwrap();
        });

        let config = proxy_config("openai/gpt-oss-20b", &format!("http://{}", addr));
        let agent = build_upstream_agent("/v1/responses");
        let outcome = send_completion_template_upstream_request_with_retry(
            &agent,
            &config,
            "POST",
            &format!("http://{}/v1/chat/completions", addr),
            config.active_model.as_deref(),
            br#"{"model":"openai/gpt-oss-20b","messages":[]}"#,
        )
        .unwrap();

        assert_eq!(outcome.status_code, 200);
        assert_eq!(
            outcome.body,
            br#"{"id":"cmpl-1","object":"chat.completion"}"#
        );
        server.join().unwrap();
    }
}
