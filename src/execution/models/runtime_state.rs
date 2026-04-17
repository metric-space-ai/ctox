use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use crate::inference::engine;
use crate::inference::model_adapters;
use crate::inference::runtime_plan;

const DEFAULT_RUNTIME_STATE_RELATIVE_PATH: &str = "runtime/inference_runtime.json";
const DEFAULT_PROXY_HOST: &str = "127.0.0.1";
const DEFAULT_PROXY_PORT: u16 = 12434;
const DEFAULT_LOCAL_ENGINE_PORT: u16 = 1234;
const DEFAULT_OPENAI_RESPONSES_BASE_URL: &str = "https://api.openai.com";
const DEFAULT_ANTHROPIC_RESPONSES_BASE_URL: &str = "https://api.anthropic.com/v1";
const DEFAULT_OPENROUTER_RESPONSES_BASE_URL: &str = "https://openrouter.ai/api/v1";
// MiniMax exposes an OpenAI-compatible chat-completions surface at
// https://api.minimax.io/v1/chat/completions. Keys issued on
// platform.minimax.io authenticate here as Bearer tokens just like
// OpenAI's. The base URL deliberately omits the trailing `/v1` because
// CTOX' gateway concatenates the adapter-emitted upstream_path
// (`/v1/chat/completions`) onto this base — same convention as
// DEFAULT_OPENAI_RESPONSES_BASE_URL.
const DEFAULT_MINIMAX_RESPONSES_BASE_URL: &str = "https://api.minimax.io";
const API_PROVIDER_LOCAL: &str = "local";
const LOCAL_RUNTIME_CANDLE: &str = "candle";
const LOCAL_RUNTIME_LITERT: &str = "litert";
const API_PROVIDER_OPENAI: &str = "openai";
const API_PROVIDER_ANTHROPIC: &str = "anthropic";
const API_PROVIDER_OPENROUTER: &str = "openrouter";
const API_PROVIDER_MINIMAX: &str = "minimax";

fn default_auxiliary_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BoostRuntimeState {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub active_until_epoch: Option<u64>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AdapterRuntimeTuning {
    #[serde(default)]
    pub reasoning_cap: Option<String>,
    #[serde(default)]
    pub max_output_tokens_cap: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InferenceSource {
    Local,
    Api,
}

impl InferenceSource {
    pub fn as_env_value(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Api => "api",
        }
    }

    pub fn is_local(self) -> bool {
        self == Self::Local
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChatSkillPreset {
    #[default]
    Standard,
    Simple,
}

impl ChatSkillPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Simple => "Simple",
        }
    }

    pub fn from_label(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "simple" => Self::Simple,
            _ => Self::Standard,
        }
    }
}

pub fn default_local_runtime_kind() -> LocalRuntimeKind {
    LocalRuntimeKind::Candle
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalRuntimeKind {
    Candle,
    #[serde(rename = "litert")]
    LiteRt,
}

impl LocalRuntimeKind {
    pub fn as_env_value(self) -> &'static str {
        match self {
            Self::Candle => LOCAL_RUNTIME_CANDLE,
            Self::LiteRt => LOCAL_RUNTIME_LITERT,
        }
    }

    pub fn is_candle(self) -> bool {
        self == Self::Candle
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuxiliaryRuntimeState {
    #[serde(default = "default_auxiliary_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub configured_model: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub base_url: Option<String>,
}

impl Default for AuxiliaryRuntimeState {
    fn default() -> Self {
        Self {
            enabled: true,
            configured_model: None,
            port: None,
            base_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InferenceRuntimeState {
    pub version: u32,
    pub source: InferenceSource,
    #[serde(default = "default_local_runtime_kind")]
    pub local_runtime: LocalRuntimeKind,
    #[serde(default)]
    pub base_model: Option<String>,
    pub requested_model: Option<String>,
    pub active_model: Option<String>,
    pub engine_model: Option<String>,
    pub engine_port: Option<u16>,
    pub realized_context_tokens: Option<u32>,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub upstream_base_url: String,
    pub local_preset: Option<String>,
    #[serde(default)]
    pub boost: BoostRuntimeState,
    #[serde(default)]
    pub adapter_tuning: AdapterRuntimeTuning,
    #[serde(default)]
    pub embedding: AuxiliaryRuntimeState,
    #[serde(default)]
    pub transcription: AuxiliaryRuntimeState,
    #[serde(default)]
    pub speech: AuxiliaryRuntimeState,
    /// Qwen3-VL-2B-Instruct (or equivalent) auxiliary vision model used by
    /// the vision preprocessor to turn image content blocks into textual
    /// descriptions for primary LLMs that cannot natively accept images.
    /// Ensures tools can always evaluate images regardless of the primary
    /// model's capabilities.
    #[serde(default)]
    pub vision: AuxiliaryRuntimeState,
}

impl InferenceRuntimeState {
    pub fn base_or_selected_model(&self) -> Option<&str> {
        self.base_model
            .as_deref()
            .or(self.requested_model.as_deref())
            .or(self.active_model.as_deref())
    }

    pub fn active_or_selected_model(&self) -> Option<&str> {
        self.active_model
            .as_deref()
            .or(self.requested_model.as_deref())
            .or(self.base_model.as_deref())
    }
}

pub fn runtime_state_path(root: &Path) -> PathBuf {
    root.join(DEFAULT_RUNTIME_STATE_RELATIVE_PATH)
}

pub fn default_proxy_host() -> &'static str {
    DEFAULT_PROXY_HOST
}

pub fn default_proxy_port() -> u16 {
    DEFAULT_PROXY_PORT
}

pub fn default_local_engine_port() -> u16 {
    DEFAULT_LOCAL_ENGINE_PORT
}

pub fn default_api_upstream_base_url() -> &'static str {
    DEFAULT_OPENAI_RESPONSES_BASE_URL
}

pub fn default_api_upstream_base_url_for_provider(provider: &str) -> &'static str {
    match normalize_api_provider(provider) {
        API_PROVIDER_ANTHROPIC => DEFAULT_ANTHROPIC_RESPONSES_BASE_URL,
        API_PROVIDER_OPENROUTER => DEFAULT_OPENROUTER_RESPONSES_BASE_URL,
        API_PROVIDER_MINIMAX => DEFAULT_MINIMAX_RESPONSES_BASE_URL,
        _ => DEFAULT_OPENAI_RESPONSES_BASE_URL,
    }
}

pub fn normalize_api_provider(provider: &str) -> &'static str {
    match provider.trim().to_ascii_lowercase().as_str() {
        API_PROVIDER_ANTHROPIC => API_PROVIDER_ANTHROPIC,
        API_PROVIDER_OPENROUTER => API_PROVIDER_OPENROUTER,
        API_PROVIDER_MINIMAX => API_PROVIDER_MINIMAX,
        API_PROVIDER_OPENAI => API_PROVIDER_OPENAI,
        API_PROVIDER_LOCAL => API_PROVIDER_LOCAL,
        _ => API_PROVIDER_OPENAI,
    }
}

pub fn normalize_local_runtime_kind(runtime: &str) -> &'static str {
    match runtime.trim().to_ascii_lowercase().as_str() {
        LOCAL_RUNTIME_LITERT => LOCAL_RUNTIME_LITERT,
        _ => LOCAL_RUNTIME_CANDLE,
    }
}

pub fn infer_local_runtime_kind_from_env_map(
    env_map: &BTreeMap<String, String>,
) -> LocalRuntimeKind {
    match env_string(env_map, "CTOX_LOCAL_RUNTIME")
        .as_deref()
        .map(normalize_local_runtime_kind)
    {
        Some(LOCAL_RUNTIME_LITERT) => LocalRuntimeKind::LiteRt,
        _ => LocalRuntimeKind::Candle,
    }
}

pub fn preferred_local_runtime_kind_for_model(_model: &str) -> Option<LocalRuntimeKind> {
    None
}

pub fn validated_litert_context_cap_for_model(model: &str) -> Option<u32> {
    match model.trim() {
        "google/gemma-4-E2B-it" => Some(131_072),
        "google/gemma-4-E4B-it" => Some(131_072),
        _ => None,
    }
}

pub fn is_openai_compatible_api_upstream(upstream_base_url: &str) -> bool {
    let trimmed = upstream_base_url.trim();
    trimmed.starts_with(DEFAULT_OPENAI_RESPONSES_BASE_URL)
        || trimmed.starts_with(DEFAULT_ANTHROPIC_RESPONSES_BASE_URL)
}

pub fn api_provider_for_upstream_base_url(upstream_base_url: &str) -> &'static str {
    let trimmed = upstream_base_url.trim();
    if trimmed.is_empty() {
        return API_PROVIDER_LOCAL;
    }
    if trimmed.starts_with(DEFAULT_OPENROUTER_RESPONSES_BASE_URL) {
        API_PROVIDER_OPENROUTER
    } else if trimmed.starts_with(DEFAULT_ANTHROPIC_RESPONSES_BASE_URL) {
        API_PROVIDER_ANTHROPIC
    } else if trimmed.starts_with(DEFAULT_MINIMAX_RESPONSES_BASE_URL) {
        API_PROVIDER_MINIMAX
    } else {
        API_PROVIDER_OPENAI
    }
}

pub fn api_provider_for_runtime_state(state: &InferenceRuntimeState) -> &'static str {
    if state.source.is_local() {
        API_PROVIDER_LOCAL
    } else {
        api_provider_for_upstream_base_url(&state.upstream_base_url)
    }
}

pub fn infer_api_provider_from_env_map(env_map: &BTreeMap<String, String>) -> String {
    let explicit_source_api = env_string(env_map, "CTOX_CHAT_SOURCE")
        .map(|value| value.eq_ignore_ascii_case("api"))
        .unwrap_or(false);
    let explicit_provider = env_string(env_map, "CTOX_API_PROVIDER")
        .map(|value| normalize_api_provider(&value).to_string());
    let model_provider = env_string(env_map, "CTOX_CHAT_MODEL")
        .filter(|model| explicit_source_api || engine::is_api_chat_model(model))
        .map(|model| engine::default_api_provider_for_model(&model).to_string());
    match (explicit_provider, model_provider) {
        (Some(explicit), Some(model_provider))
            if explicit.eq_ignore_ascii_case(API_PROVIDER_LOCAL)
                && (explicit_source_api
                    || env_string(env_map, "CTOX_CHAT_MODEL")
                        .as_deref()
                        .is_some_and(|model| !engine::supports_local_chat_runtime(model))) =>
        {
            model_provider
        }
        (Some(explicit), _) => explicit,
        (None, Some(model_provider)) => model_provider,
        (None, None) => env_string(env_map, "CTOX_UPSTREAM_BASE_URL")
            .map(|value| api_provider_for_upstream_base_url(&value).to_string())
            .unwrap_or_else(|| API_PROVIDER_LOCAL.to_string()),
    }
}

pub fn api_key_env_var_for_provider(provider: &str) -> &'static str {
    match normalize_api_provider(provider) {
        API_PROVIDER_OPENROUTER => "OPENROUTER_API_KEY",
        API_PROVIDER_ANTHROPIC => "ANTHROPIC_API_KEY",
        API_PROVIDER_MINIMAX => "MINIMAX_API_KEY",
        _ => "OPENAI_API_KEY",
    }
}

pub fn api_key_env_var_for_upstream_base_url(upstream_base_url: &str) -> &'static str {
    api_key_env_var_for_provider(api_provider_for_upstream_base_url(upstream_base_url))
}

pub fn local_upstream_base_url(port: u16) -> String {
    let _ = port;
    String::new()
}

pub fn load_runtime_state(root: &Path) -> Result<Option<InferenceRuntimeState>> {
    let path = runtime_state_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read(&path)
        .with_context(|| format!("failed to read runtime state {}", path.display()))?;
    let mut state: InferenceRuntimeState = serde_json::from_slice(&raw)
        .with_context(|| format!("failed to decode runtime state {}", path.display()))?;
    if migrate_runtime_state(root, &mut state)? {
        persist_runtime_state(root, &state)?;
    }
    Ok(Some(state))
}

pub fn persist_runtime_state(root: &Path, state: &InferenceRuntimeState) -> Result<()> {
    let path = runtime_state_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime state dir {}", parent.display()))?;
    }
    let bytes =
        serde_json::to_vec_pretty(state).context("failed to encode inference runtime state")?;
    std::fs::write(&path, bytes)
        .with_context(|| format!("failed to write runtime state {}", path.display()))
}

pub fn load_or_resolve_runtime_state(root: &Path) -> Result<InferenceRuntimeState> {
    if let Some(state) = load_runtime_state(root)? {
        return Ok(state);
    }
    let env_map = load_runtime_env_map_for_resolution(root)?;
    let state = derive_runtime_state_from_env_map(root, &env_map)?;
    persist_runtime_state(root, &state)?;
    Ok(state)
}

pub fn sync_runtime_state_from_env_map(
    root: &Path,
    env_map: &BTreeMap<String, String>,
) -> Result<InferenceRuntimeState> {
    let state = derive_runtime_state_from_env_map(root, env_map)?;
    crate::inference::runtime_env::save_runtime_state_projection(root, &state, env_map)?;
    Ok(state)
}

pub fn derive_runtime_state_from_env_map(
    root: &Path,
    env_map: &BTreeMap<String, String>,
) -> Result<InferenceRuntimeState> {
    derive_runtime_state(root, env_map)
}

pub fn is_runtime_state_key(key: &str) -> bool {
    matches!(
        key,
        "CTOX_CHAT_SOURCE"
            | "CTOX_LOCAL_RUNTIME"
            | "CTOX_API_PROVIDER"
            | "CTOX_CHAT_MODEL_BASE"
            | "CTOX_CHAT_MODEL"
            | "CTOX_CHAT_MODEL_BOOST"
            | "CTOX_ACTIVE_MODEL"
            | "CTOX_BOOST_ACTIVE_UNTIL_EPOCH"
            | "CTOX_BOOST_REASON"
            | "CTOX_ENGINE_MODEL"
            | "CTOX_ENGINE_PORT"
            | "CTOX_ENGINE_REALIZED_MODEL"
            | "CTOX_ENGINE_REALIZED_MAX_SEQ_LEN"
            | "CTOX_CHAT_MODEL_REALIZED_CONTEXT"
            | "CTOX_CHAT_MODEL_MAX_CONTEXT"
            | "CTOX_PROXY_HOST"
            | "CTOX_PROXY_PORT"
            | "CTOX_UPSTREAM_BASE_URL"
            | "CTOX_CHAT_LOCAL_PRESET"
            | "CTOX_LOCAL_ADAPTER_REASONING_CAP"
            | "CTOX_LOCAL_ADAPTER_MAX_OUTPUT_TOKENS_CAP"
            | "CTOX_EMBEDDING_MODEL"
            | "CTOX_EMBEDDING_PORT"
            | "CTOX_EMBEDDING_BASE_URL"
            | "CTOX_DISABLE_EMBEDDING_BACKEND"
            | "CTOX_STT_MODEL"
            | "CTOX_STT_PORT"
            | "CTOX_STT_BASE_URL"
            | "CTOX_DISABLE_STT_BACKEND"
            | "CTOX_TTS_MODEL"
            | "CTOX_TTS_PORT"
            | "CTOX_TTS_BASE_URL"
            | "CTOX_DISABLE_TTS_BACKEND"
            | "CTOX_VISION_MODEL"
            | "CTOX_VISION_PORT"
            | "CTOX_VISION_BASE_URL"
            | "CTOX_DISABLE_VISION_BACKEND"
    )
}

pub fn auxiliary_runtime_state_for_role(
    state: &InferenceRuntimeState,
    role: engine::AuxiliaryRole,
) -> &AuxiliaryRuntimeState {
    match role {
        engine::AuxiliaryRole::Embedding => &state.embedding,
        engine::AuxiliaryRole::Stt => &state.transcription,
        engine::AuxiliaryRole::Tts => &state.speech,
        engine::AuxiliaryRole::Vision => &state.vision,
    }
}

pub fn owned_runtime_env_value(state: &InferenceRuntimeState, key: &str) -> Option<String> {
    match key {
        "CTOX_CHAT_SOURCE" => Some(state.source.as_env_value().to_string()),
        "CTOX_LOCAL_RUNTIME" => Some(state.local_runtime.as_env_value().to_string()),
        "CTOX_API_PROVIDER" => Some(api_provider_for_runtime_state(state).to_string()),
        "CTOX_CHAT_MODEL_BASE" => state
            .base_model
            .clone()
            .or_else(|| state.requested_model.clone())
            .or_else(|| state.active_model.clone()),
        "CTOX_CHAT_MODEL" => state
            .requested_model
            .clone()
            .or_else(|| state.active_model.clone()),
        "CTOX_CHAT_MODEL_BOOST" => state.boost.model.clone(),
        "CTOX_ACTIVE_MODEL" => state.active_model.clone(),
        "CTOX_BOOST_ACTIVE_UNTIL_EPOCH" => state
            .boost
            .active_until_epoch
            .map(|value| value.to_string()),
        "CTOX_BOOST_REASON" => state.boost.reason.clone(),
        "CTOX_ENGINE_MODEL" | "CTOX_ENGINE_REALIZED_MODEL" => state.engine_model.clone(),
        "CTOX_ENGINE_PORT" => state.engine_port.map(|value| value.to_string()),
        "CTOX_ENGINE_REALIZED_MAX_SEQ_LEN"
        | "CTOX_CHAT_MODEL_REALIZED_CONTEXT"
        | "CTOX_CHAT_MODEL_MAX_CONTEXT" => {
            state.realized_context_tokens.map(|value| value.to_string())
        }
        "CTOX_PROXY_HOST" => Some(state.proxy_host.clone()),
        "CTOX_PROXY_PORT" => Some(state.proxy_port.to_string()),
        "CTOX_UPSTREAM_BASE_URL" => {
            if state.source.is_local() {
                None
            } else {
                Some(state.upstream_base_url.clone())
            }
        }
        "CTOX_CHAT_LOCAL_PRESET" => state.local_preset.clone(),
        key if key == model_adapters::adapter_reasoning_cap_env_key() => {
            state.adapter_tuning.reasoning_cap.clone()
        }
        key if key == model_adapters::adapter_max_output_tokens_cap_env_key() => state
            .adapter_tuning
            .max_output_tokens_cap
            .map(|value| value.to_string()),
        "CTOX_EMBEDDING_MODEL" => state.embedding.configured_model.clone(),
        "CTOX_EMBEDDING_PORT" => state.embedding.port.map(|value| value.to_string()),
        "CTOX_EMBEDDING_BASE_URL" => state.embedding.base_url.clone(),
        "CTOX_DISABLE_EMBEDDING_BACKEND" => {
            if state.embedding.enabled {
                None
            } else {
                Some("1".to_string())
            }
        }
        "CTOX_STT_MODEL" => state.transcription.configured_model.clone(),
        "CTOX_STT_PORT" => state.transcription.port.map(|value| value.to_string()),
        "CTOX_STT_BASE_URL" => state.transcription.base_url.clone(),
        "CTOX_DISABLE_STT_BACKEND" => {
            if state.transcription.enabled {
                None
            } else {
                Some("1".to_string())
            }
        }
        "CTOX_TTS_MODEL" => state.speech.configured_model.clone(),
        "CTOX_TTS_PORT" => state.speech.port.map(|value| value.to_string()),
        "CTOX_TTS_BASE_URL" => state.speech.base_url.clone(),
        "CTOX_DISABLE_TTS_BACKEND" => {
            if state.speech.enabled {
                None
            } else {
                Some("1".to_string())
            }
        }
        "CTOX_VISION_MODEL" => state.vision.configured_model.clone(),
        "CTOX_VISION_PORT" => state.vision.port.map(|value| value.to_string()),
        "CTOX_VISION_BASE_URL" => state.vision.base_url.clone(),
        "CTOX_DISABLE_VISION_BACKEND" => {
            if state.vision.enabled {
                None
            } else {
                Some("1".to_string())
            }
        }
        _ => None,
    }
}

pub fn apply_runtime_state_to_env_map(
    env_map: &mut BTreeMap<String, String>,
    state: &InferenceRuntimeState,
) {
    for key in [
        "CTOX_CHAT_SOURCE",
        "CTOX_LOCAL_RUNTIME",
        "CTOX_CHAT_MODEL_BASE",
        "CTOX_CHAT_MODEL",
        "CTOX_CHAT_MODEL_BOOST",
        "CTOX_ACTIVE_MODEL",
        "CTOX_BOOST_ACTIVE_UNTIL_EPOCH",
        "CTOX_BOOST_REASON",
        "CTOX_ENGINE_MODEL",
        "CTOX_ENGINE_PORT",
        "CTOX_ENGINE_REALIZED_MODEL",
        "CTOX_ENGINE_REALIZED_MAX_SEQ_LEN",
        "CTOX_CHAT_MODEL_REALIZED_CONTEXT",
        "CTOX_CHAT_MODEL_MAX_CONTEXT",
        "CTOX_PROXY_HOST",
        "CTOX_PROXY_PORT",
        "CTOX_API_PROVIDER",
        "CTOX_UPSTREAM_BASE_URL",
        "CTOX_CHAT_LOCAL_PRESET",
        "CTOX_EMBEDDING_MODEL",
        "CTOX_EMBEDDING_PORT",
        "CTOX_EMBEDDING_BASE_URL",
        "CTOX_DISABLE_EMBEDDING_BACKEND",
        "CTOX_STT_MODEL",
        "CTOX_STT_PORT",
        "CTOX_STT_BASE_URL",
        "CTOX_DISABLE_STT_BACKEND",
        "CTOX_TTS_MODEL",
        "CTOX_TTS_PORT",
        "CTOX_TTS_BASE_URL",
        "CTOX_DISABLE_TTS_BACKEND",
        "CTOX_VISION_MODEL",
        "CTOX_VISION_PORT",
        "CTOX_VISION_BASE_URL",
        "CTOX_DISABLE_VISION_BACKEND",
    ] {
        if let Some(value) = owned_runtime_env_value(state, key) {
            env_map.insert(key.to_string(), value);
        } else {
            env_map.remove(key);
        }
    }
}

fn derive_runtime_state(
    root: &Path,
    env_map: &BTreeMap<String, String>,
) -> Result<InferenceRuntimeState> {
    let base_model = configured_base_model_from_map(env_map);
    let requested_model = env_string(env_map, "CTOX_CHAT_MODEL")
        .or_else(|| base_model.clone())
        .or_else(|| env_string(env_map, "CTOX_ACTIVE_MODEL"));
    let source = infer_source(
        env_map,
        requested_model.as_deref().or(base_model.as_deref()),
    );
    let local_runtime = infer_local_runtime_kind_from_env_map(env_map);
    let proxy_host =
        env_string(env_map, "CTOX_PROXY_HOST").unwrap_or_else(|| DEFAULT_PROXY_HOST.to_string());
    let proxy_port = env_u16(env_map, "CTOX_PROXY_PORT").unwrap_or(DEFAULT_PROXY_PORT);
    let local_preset = env_string(env_map, "CTOX_CHAT_LOCAL_PRESET");
    let plan = if source.is_local() {
        runtime_plan::load_persisted_chat_runtime_plan(root)?
    } else {
        None
    };

    let (active_model, engine_model, engine_port, realized_context_tokens, upstream_base_url) =
        match source {
            InferenceSource::Api => {
                let api_provider = infer_api_provider_from_env_map(env_map);
                let active_model = env_string(env_map, "CTOX_ACTIVE_MODEL")
                    .filter(|model| engine::is_api_chat_model(model))
                    .or_else(|| {
                        env_string(env_map, "CTOX_CHAT_MODEL")
                            .filter(|model| engine::is_api_chat_model(model))
                    })
                    .or_else(|| requested_model.clone())
                    .or_else(|| Some(default_primary_model()));
                let upstream = env_string(env_map, "CTOX_UPSTREAM_BASE_URL").unwrap_or_else(|| {
                    default_api_upstream_base_url_for_provider(&api_provider).to_string()
                });
                (active_model, None, None, None, upstream)
            }
            InferenceSource::Local => {
                let active_model = plan
                    .as_ref()
                    .map(|plan| plan.model.clone())
                    .or_else(|| env_string(env_map, "CTOX_ENGINE_MODEL"))
                    .or_else(|| env_string(env_map, "CTOX_ACTIVE_MODEL"))
                    .or_else(|| env_string(env_map, "CTOX_CHAT_MODEL"))
                    .or_else(|| requested_model.clone())
                    .or_else(|| Some(default_primary_model()));
                let engine_model = active_model.clone();
                let engine_port = env_u16(env_map, "CTOX_ENGINE_PORT")
                    .or_else(|| {
                        engine_model.as_deref().and_then(|model| {
                            engine::runtime_config_for_model(model)
                                .ok()
                                .map(|runtime| runtime.port)
                        })
                    })
                    .or_else(|| plan.as_ref().map(|_| DEFAULT_LOCAL_ENGINE_PORT))
                    .or_else(|| Some(DEFAULT_LOCAL_ENGINE_PORT));
                let realized_context_tokens = env_u32(env_map, "CTOX_CHAT_MODEL_REALIZED_CONTEXT")
                    .or_else(|| env_u32(env_map, "CTOX_ENGINE_REALIZED_MAX_SEQ_LEN"))
                    .or_else(|| plan.as_ref().map(|plan| plan.max_seq_len))
                    .or_else(|| env_u32(env_map, "CTOX_CHAT_MODEL_MAX_CONTEXT"))
                    .or_else(|| {
                        (local_runtime == LocalRuntimeKind::LiteRt)
                            .then(|| {
                                active_model
                                    .as_deref()
                                    .and_then(validated_litert_context_cap_for_model)
                            })
                            .flatten()
                    });
                let upstream =
                    local_upstream_base_url(engine_port.unwrap_or(DEFAULT_LOCAL_ENGINE_PORT));
                (
                    active_model,
                    engine_model,
                    engine_port,
                    realized_context_tokens,
                    upstream,
                )
            }
        };

    Ok(InferenceRuntimeState {
        version: 9,
        source,
        local_runtime,
        base_model,
        requested_model,
        active_model,
        engine_model,
        engine_port,
        realized_context_tokens,
        proxy_host,
        proxy_port,
        upstream_base_url,
        local_preset,
        boost: derive_boost_runtime_state(env_map),
        adapter_tuning: derive_adapter_runtime_tuning(env_map),
        embedding: derive_auxiliary_runtime_state(env_map, "EMBEDDING"),
        transcription: derive_auxiliary_runtime_state(env_map, "STT"),
        speech: derive_auxiliary_runtime_state(env_map, "TTS"),
        vision: derive_auxiliary_runtime_state(env_map, "VISION"),
    })
}

fn migrate_runtime_state(root: &Path, state: &mut InferenceRuntimeState) -> Result<bool> {
    let env_map = load_runtime_env_map_for_resolution(root)?;
    let mut migrated = false;
    if state.version < 2 {
        state.embedding = derive_auxiliary_runtime_state(&env_map, "EMBEDDING");
        state.transcription = derive_auxiliary_runtime_state(&env_map, "STT");
        state.speech = derive_auxiliary_runtime_state(&env_map, "TTS");
        state.version = 2;
        migrated = true;
    }
    if state.version < 3 {
        state.base_model = configured_base_model_from_map(&env_map)
            .or_else(|| state.requested_model.clone())
            .or_else(|| state.active_model.clone());
        state.boost = derive_boost_runtime_state(&env_map);
        state.version = 3;
        migrated = true;
    }
    if state.version < 4 {
        state.adapter_tuning = derive_adapter_runtime_tuning(&env_map);
        state.version = 4;
        migrated = true;
    }
    if state.version < 5 {
        state.adapter_tuning.max_output_tokens_cap = None;
        state.version = 5;
        migrated = true;
    }
    if state.version < 6 {
        if state.source == InferenceSource::Api && state.upstream_base_url.trim().is_empty() {
            let provider = infer_api_provider_from_env_map(&env_map);
            state.upstream_base_url =
                default_api_upstream_base_url_for_provider(&provider).to_string();
        }
        state.version = 6;
        migrated = true;
    }
    if state.version < 7 {
        state.local_runtime = infer_local_runtime_kind_from_env_map(&env_map);
        state.version = 7;
        migrated = true;
    }
    if state.version < 8 {
        if state.source.is_local() {
            state.upstream_base_url =
                local_upstream_base_url(state.engine_port.unwrap_or(DEFAULT_LOCAL_ENGINE_PORT));
        } else if state.upstream_base_url.trim().is_empty() {
            let provider = infer_api_provider_from_env_map(&env_map);
            state.upstream_base_url =
                default_api_upstream_base_url_for_provider(&provider).to_string();
        }
        state.version = 8;
        migrated = true;
    }
    if state.version < 9 {
        state.vision = derive_auxiliary_runtime_state(&env_map, "VISION");
        state.version = 9;
        migrated = true;
    }
    if state.base_model.is_none() {
        state.base_model = state
            .requested_model
            .clone()
            .or_else(|| state.active_model.clone());
        migrated = true;
    }
    Ok(migrated)
}

fn infer_source(
    env_map: &BTreeMap<String, String>,
    requested_model: Option<&str>,
) -> InferenceSource {
    if env_string(env_map, "CTOX_CHAT_SOURCE")
        .map(|value| value.eq_ignore_ascii_case("api"))
        .unwrap_or(false)
    {
        return InferenceSource::Api;
    }
    let selected_model = env_string(env_map, "CTOX_ACTIVE_MODEL")
        .or_else(|| env_string(env_map, "CTOX_CHAT_MODEL"))
        .or(requested_model.map(str::to_string))
        .unwrap_or_default();
    let inferred_provider = infer_api_provider_from_env_map(env_map);
    if (!inferred_provider.eq_ignore_ascii_case(API_PROVIDER_LOCAL)
        && engine::api_provider_supports_model(&inferred_provider, &selected_model))
        || engine::is_api_chat_model(&selected_model)
    {
        return InferenceSource::Api;
    }
    InferenceSource::Local
}

fn configured_base_model_from_map(env_map: &BTreeMap<String, String>) -> Option<String> {
    env_string(env_map, "CTOX_CHAT_MODEL_BASE").or_else(|| env_string(env_map, "CTOX_CHAT_MODEL"))
}

fn derive_boost_runtime_state(env_map: &BTreeMap<String, String>) -> BoostRuntimeState {
    BoostRuntimeState {
        model: env_string(env_map, "CTOX_CHAT_MODEL_BOOST"),
        active_until_epoch: env_string(env_map, "CTOX_BOOST_ACTIVE_UNTIL_EPOCH")
            .and_then(|value| value.parse::<u64>().ok()),
        reason: env_string(env_map, "CTOX_BOOST_REASON"),
    }
}

fn derive_adapter_runtime_tuning(env_map: &BTreeMap<String, String>) -> AdapterRuntimeTuning {
    AdapterRuntimeTuning {
        reasoning_cap: env_string(env_map, model_adapters::adapter_reasoning_cap_env_key()),
        max_output_tokens_cap: env_u32(
            env_map,
            model_adapters::adapter_max_output_tokens_cap_env_key(),
        ),
    }
}

pub fn default_primary_model() -> String {
    engine::default_runtime_config(engine::LocalModelFamily::GptOss).model
}

fn env_string(env_map: &BTreeMap<String, String>, key: &str) -> Option<String> {
    env_map
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn env_u16(env_map: &BTreeMap<String, String>, key: &str) -> Option<u16> {
    env_string(env_map, key).and_then(|value| value.parse::<u16>().ok())
}

fn env_u32(env_map: &BTreeMap<String, String>, key: &str) -> Option<u32> {
    env_string(env_map, key).and_then(|value| value.parse::<u32>().ok())
}

fn config_flag_from_env_map(env_map: &BTreeMap<String, String>, key: &str) -> bool {
    env_string(env_map, key)
        .as_deref()
        .and_then(parse_boolish)
        .unwrap_or(false)
}

fn parse_boolish(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn is_disabled_selector(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "off" | "none" | "null" | "disabled" | "disable"
    )
}

fn derive_auxiliary_runtime_state(
    env_map: &BTreeMap<String, String>,
    role_prefix: &str,
) -> AuxiliaryRuntimeState {
    let configured_model = env_string(env_map, &format!("CTOX_{role_prefix}_MODEL"))
        .filter(|value| !is_disabled_selector(value));
    let explicit_enable = env_string(env_map, &format!("CTOX_ENABLE_{role_prefix}_BACKEND"))
        .as_deref()
        .and_then(parse_boolish);
    let enabled = if config_flag_from_env_map(env_map, "CTOX_DISABLE_AUXILIARY_BACKENDS")
        || config_flag_from_env_map(env_map, &format!("CTOX_DISABLE_{role_prefix}_BACKEND"))
    {
        false
    } else if let Some(model_value) = env_string(env_map, &format!("CTOX_{role_prefix}_MODEL")) {
        explicit_enable.unwrap_or(!is_disabled_selector(&model_value))
    } else {
        explicit_enable.unwrap_or(true)
    };

    AuxiliaryRuntimeState {
        enabled,
        configured_model,
        port: env_u16(env_map, &format!("CTOX_{role_prefix}_PORT")),
        base_url: env_string(env_map, &format!("CTOX_{role_prefix}_BASE_URL")),
    }
}

fn load_runtime_env_map_for_resolution(root: &Path) -> Result<BTreeMap<String, String>> {
    let path = root.join("runtime/engine.env");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read runtime config {}", path.display()))?;
    Ok(parse_env_map(&raw))
}

fn parse_env_map(raw: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let normalized_key = key.trim();
        if normalized_key.is_empty() {
            continue;
        }
        out.insert(normalized_key.to_string(), unescape_env_value(value.trim()));
    }
    out
}

fn unescape_env_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut output = String::new();
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    output.push(next);
                }
            } else {
                output.push(ch);
            }
        }
        output
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
#[path = "runtime_state_boundary_tests.rs"]
mod boundary_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::runtime_plan::ChatPreset;
    use crate::inference::runtime_plan::ChatRuntimePlan;
    use crate::inference::runtime_plan::PlannedGpuAllocation;
    use crate::inference::runtime_plan::TheoreticalResourceBreakdown;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    fn make_temp_root() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ctox-runtime-state-test-{unique}"));
        std::fs::create_dir_all(path.join("runtime")).unwrap();
        path
    }

    fn sample_plan(model: &str) -> ChatRuntimePlan {
        ChatRuntimePlan {
            model: model.to_string(),
            preset: ChatPreset::Quality,
            quantization: "q4".to_string(),
            runtime_isq: None,
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
            max_batch_size: 8,
            max_seqs: 8,
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
            theoretical_breakdown: TheoreticalResourceBreakdown {
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
            rationale: vec!["test".to_string()],
            gpu_allocations: vec![PlannedGpuAllocation {
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

    fn persist_plan(root: &Path, plan: &ChatRuntimePlan) {
        let path = root.join("runtime/chat_plan.json");
        let bytes = serde_json::to_vec_pretty(plan).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn sync_runtime_state_prefers_resolved_local_plan() {
        let root = make_temp_root();
        persist_plan(&root, &sample_plan("Qwen/Qwen3.5-35B-A3B"));
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "Qwen/Qwen3.5-35B-A3B".to_string(),
        );
        env_map.insert("CTOX_ACTIVE_MODEL".to_string(), "stale/value".to_string());
        let state = sync_runtime_state_from_env_map(&root, &env_map).unwrap();
        assert_eq!(state.source, InferenceSource::Local);
        assert_eq!(
            state.requested_model.as_deref(),
            Some("Qwen/Qwen3.5-35B-A3B")
        );
        assert_eq!(state.active_model.as_deref(), Some("Qwen/Qwen3.5-35B-A3B"));
        assert_eq!(
            state.engine_port,
            engine::runtime_config_for_model("Qwen/Qwen3.5-35B-A3B")
                .ok()
                .map(|runtime| runtime.port)
        );
        assert_eq!(state.realized_context_tokens, Some(131_072));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn apply_runtime_state_rewrites_legacy_runtime_keys() {
        let state = InferenceRuntimeState {
            version: 4,
            source: InferenceSource::Api,
            local_runtime: LocalRuntimeKind::Candle,
            base_model: Some("gpt-5.4".to_string()),
            requested_model: Some("gpt-5.4".to_string()),
            active_model: Some("gpt-5.4".to_string()),
            engine_model: None,
            engine_port: None,
            realized_context_tokens: None,
            proxy_host: DEFAULT_PROXY_HOST.to_string(),
            proxy_port: DEFAULT_PROXY_PORT,
            upstream_base_url: DEFAULT_OPENAI_RESPONSES_BASE_URL.to_string(),
            local_preset: None,
            boost: BoostRuntimeState::default(),
            adapter_tuning: AdapterRuntimeTuning::default(),
            embedding: AuxiliaryRuntimeState {
                enabled: true,
                configured_model: Some("Qwen/Qwen3-Embedding-0.6B [CPU]".to_string()),
                port: Some(2237),
                base_url: Some("http://127.0.0.1:2237".to_string()),
            },
            transcription: AuxiliaryRuntimeState {
                enabled: false,
                configured_model: Some("Systran/faster-whisper-small [CPU]".to_string()),
                port: Some(2238),
                base_url: Some("http://127.0.0.1:2238".to_string()),
            },
            speech: AuxiliaryRuntimeState::default(),
            vision: AuxiliaryRuntimeState::default(),
        };
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_ENGINE_MODEL".to_string(), "stale".to_string());
        env_map.insert("CTOX_ENGINE_PORT".to_string(), "1234".to_string());
        apply_runtime_state_to_env_map(&mut env_map, &state);
        assert_eq!(
            env_map.get("CTOX_CHAT_SOURCE").map(String::as_str),
            Some("api")
        );
        assert_eq!(
            env_map.get("CTOX_ACTIVE_MODEL").map(String::as_str),
            Some("gpt-5.4")
        );
        assert_eq!(
            env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
            Some("Qwen/Qwen3-Embedding-0.6B [CPU]")
        );
        assert_eq!(
            env_map.get("CTOX_EMBEDDING_PORT").map(String::as_str),
            Some("2237")
        );
        assert_eq!(
            env_map.get("CTOX_DISABLE_STT_BACKEND").map(String::as_str),
            Some("1")
        );
        assert!(!env_map.contains_key("CTOX_ENGINE_MODEL"));
        assert!(!env_map.contains_key("CTOX_ENGINE_PORT"));
    }

    #[test]
    fn sync_runtime_state_persists_auxiliary_runtime_contract() {
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_EMBEDDING_MODEL".to_string(),
            "Qwen/Qwen3-Embedding-0.6B [CPU]".to_string(),
        );
        env_map.insert("CTOX_EMBEDDING_PORT".to_string(), "2237".to_string());
        env_map.insert("CTOX_DISABLE_STT_BACKEND".to_string(), "1".to_string());
        env_map.insert(
            "CTOX_TTS_MODEL".to_string(),
            "speaches-ai/piper-en_US-lessac-medium [CPU EN]".to_string(),
        );
        env_map.insert(
            "CTOX_TTS_BASE_URL".to_string(),
            "http://127.0.0.1:2239".to_string(),
        );

        let state = sync_runtime_state_from_env_map(&root, &env_map).unwrap();

        assert!(state.embedding.enabled);
        assert_eq!(
            state.embedding.configured_model.as_deref(),
            Some("Qwen/Qwen3-Embedding-0.6B [CPU]")
        );
        assert_eq!(state.embedding.port, Some(2237));
        assert!(!state.transcription.enabled);
        assert_eq!(
            state.speech.configured_model.as_deref(),
            Some("speaches-ai/piper-en_US-lessac-medium [CPU EN]")
        );
        assert_eq!(
            state.speech.base_url.as_deref(),
            Some("http://127.0.0.1:2239")
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_state_model_helpers_prefer_base_and_active_authoritatively() {
        let state = InferenceRuntimeState {
            version: 4,
            source: InferenceSource::Local,
            local_runtime: LocalRuntimeKind::Candle,
            base_model: Some("openai/gpt-oss-20b".to_string()),
            requested_model: Some("Qwen/Qwen3.5-35B-A3B".to_string()),
            active_model: Some("gpt-5.4-mini".to_string()),
            engine_model: Some("Qwen/Qwen3.5-35B-A3B".to_string()),
            engine_port: Some(1234),
            realized_context_tokens: Some(131_072),
            proxy_host: DEFAULT_PROXY_HOST.to_string(),
            proxy_port: DEFAULT_PROXY_PORT,
            upstream_base_url: local_upstream_base_url(DEFAULT_LOCAL_ENGINE_PORT),
            local_preset: Some("quality".to_string()),
            boost: BoostRuntimeState::default(),
            adapter_tuning: AdapterRuntimeTuning::default(),
            embedding: AuxiliaryRuntimeState::default(),
            transcription: AuxiliaryRuntimeState::default(),
            speech: AuxiliaryRuntimeState::default(),
            vision: AuxiliaryRuntimeState::default(),
        };

        assert_eq!(state.base_or_selected_model(), Some("openai/gpt-oss-20b"));
        assert_eq!(state.active_or_selected_model(), Some("gpt-5.4-mini"));
    }

    #[test]
    fn sync_runtime_state_uses_validated_litert_context_cap_instead_of_assuming_128k() {
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert("CTOX_LOCAL_RUNTIME".to_string(), "litert".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "google/gemma-4-E2B-it".to_string(),
        );
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "google/gemma-4-E2B-it".to_string(),
        );

        let state = sync_runtime_state_from_env_map(&root, &env_map).unwrap();
        assert_eq!(state.local_runtime, LocalRuntimeKind::LiteRt);
        assert_eq!(state.realized_context_tokens, Some(131_072));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sync_runtime_state_persists_adapter_runtime_tuning() {
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert(
            model_adapters::adapter_reasoning_cap_env_key().to_string(),
            "low".to_string(),
        );
        env_map.insert(
            model_adapters::adapter_max_output_tokens_cap_env_key().to_string(),
            "128".to_string(),
        );

        let state = sync_runtime_state_from_env_map(&root, &env_map).unwrap();
        assert_eq!(state.adapter_tuning.reasoning_cap.as_deref(), Some("low"));
        assert_eq!(state.adapter_tuning.max_output_tokens_cap, Some(128));

        let reloaded = load_runtime_state(&root).unwrap().unwrap();
        assert_eq!(
            reloaded.adapter_tuning.reasoning_cap.as_deref(),
            Some("low")
        );
        assert_eq!(reloaded.adapter_tuning.max_output_tokens_cap, Some(128));
        let persisted_env = std::fs::read_to_string(root.join("runtime/engine.env")).unwrap();
        assert!(!persisted_env.contains(model_adapters::adapter_reasoning_cap_env_key()));
        assert!(!persisted_env.contains(model_adapters::adapter_max_output_tokens_cap_env_key()));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn infer_api_provider_prefers_selected_api_source_over_stale_local_provider_flag() {
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_API_PROVIDER".to_string(), "local".to_string());
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "api".to_string());
        env_map.insert("CTOX_CHAT_MODEL".to_string(), "qwen/qwen3.5-9b".to_string());

        assert_eq!(infer_api_provider_from_env_map(&env_map), "openrouter");
    }

    #[test]
    fn infer_api_provider_uses_openrouter_for_openrouter_base_url() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_UPSTREAM_BASE_URL".to_string(),
            "https://openrouter.ai/api/v1".to_string(),
        );

        assert_eq!(infer_api_provider_from_env_map(&env_map), "openrouter");
        assert_eq!(
            api_key_env_var_for_upstream_base_url("https://openrouter.ai/api/v1"),
            "OPENROUTER_API_KEY"
        );
    }

    #[test]
    fn infer_api_provider_uses_anthropic_for_anthropic_base_url() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_UPSTREAM_BASE_URL".to_string(),
            "https://api.anthropic.com/v1".to_string(),
        );

        assert_eq!(infer_api_provider_from_env_map(&env_map), "anthropic");
        assert_eq!(
            api_key_env_var_for_upstream_base_url("https://api.anthropic.com/v1"),
            "ANTHROPIC_API_KEY"
        );
    }
}
