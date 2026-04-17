use anyhow::Context;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::execution::models::model_registry;
use crate::inference::runtime_kernel;
use crate::inference::turn_contract;

const TURBOQUANT2_CACHE_TYPE: &str = "turboquant2";
const TURBOQUANT3_CACHE_TYPE: &str = "turboquant3";
const TURBOQUANT4_CACHE_TYPE: &str = "turboquant4";

const DISALLOWED_ENGINE_FUNCTION_TOOLS: &[&str] = &["spawn_agent", "send_input"];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalModelFamily {
    GptOss,
    Qwen35Vision,
    Gemma4Vision,
    NemotronCascade,
    Glm47Flash,
    Qwen3Embedding,
    WhisperTranscriptionCpu,
    VoxtralTranscription,
    PiperSpeech,
    Qwen3Speech,
    VoxtralSpeech,
    /// Qwen3-VL auxiliary vision model used by the vision preprocessor to
    /// describe images for primary LLMs that cannot natively accept image
    /// input. Loaded via the ctox-engine (Candle) vision loader
    /// (`Qwen3VLForConditionalGeneration`).
    Qwen3VisionAuxiliary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ChatModelFamily {
    GptOss,
    Qwen35,
    Gemma4,
    NemotronCascade,
    Glm47Flash,
    MiniMax,
    Mistral,
    Kimi,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuxiliaryRole {
    Embedding,
    Stt,
    Tts,
    /// Vision-describing auxiliary model. Used by the vision preprocessor to
    /// turn image content blocks into textual descriptions when the primary
    /// LLM is not natively vision-capable. Tools must be able to evaluate
    /// images regardless of which primary model is loaded.
    Vision,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComputeTarget {
    Gpu,
    Cpu,
}

impl ComputeTarget {
    pub fn as_env_value(self) -> &'static str {
        match self {
            Self::Gpu => "gpu",
            Self::Cpu => "cpu",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuxiliaryBackendKind {
    MistralRs,
    Speaches,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct AuxiliaryModelSelection {
    pub role: AuxiliaryRole,
    pub choice: &'static str,
    pub request_model: &'static str,
    pub backend_kind: AuxiliaryBackendKind,
    pub compute_target: ComputeTarget,
    pub default_port: u16,
}

impl AuxiliaryModelSelection {
    pub fn gpu_reserve_mb(self) -> u64 {
        if self.compute_target == ComputeTarget::Cpu {
            return 0;
        }
        match self.role {
            AuxiliaryRole::Embedding => 1100,
            AuxiliaryRole::Stt => 4200,
            AuxiliaryRole::Tts => 1400,
            // Qwen3-VL-2B with Q4K quant: ~1.2 GB weights + ~2 GB KV cache
            // + vision encoder headroom. Conservative reserve of 3.5 GB.
            AuxiliaryRole::Vision => 3500,
        }
    }
}

impl FromStr for LocalModelFamily {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        model_registry::parse_local_model_family(value)
            .ok_or_else(|| anyhow::anyhow!("unsupported clean-room model family: {}", value.trim()))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceLayoutPaths {
    pub tools_root: PathBuf,
    pub agent_runtime_root: PathBuf,
    pub codex_exec_binary: PathBuf,
    pub model_runtime_root: PathBuf,
    pub model_runtime_binary: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceTreeSpec {
    pub name: String,
    #[serde(rename = "sourceOriginUrl", alias = "repoUrl")]
    pub source_origin_url: String,
    #[serde(rename = "sourceSnapshot", default)]
    pub source_snapshot: Option<String>,
    #[serde(rename = "integrationMode", default = "default_integration_mode")]
    pub integration_mode: String,
    #[serde(rename = "requiredPaths", default)]
    pub required_paths: Vec<String>,
    #[serde(rename = "lineageNotes", default)]
    pub lineage_notes: Vec<String>,
    #[serde(rename = "targetDir")]
    pub target_dir: String,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceOriginsManifest {
    pub version: u32,
    pub goal: String,
    #[serde(rename = "sourceTrees", alias = "dependencies")]
    pub source_trees: Vec<SourceTreeSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceTreeStatus {
    pub name: String,
    pub target_dir: PathBuf,
    pub purpose: String,
    pub source_origin_url: String,
    pub source_snapshot: Option<String>,
    pub integration_mode: String,
    pub present: bool,
    pub source_owned_in_repo: bool,
    pub missing_paths: Vec<PathBuf>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceLayoutStatusOutcome {
    pub manifest_path: PathBuf,
    pub ready: bool,
    pub results: Vec<SourceTreeStatus>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EngineRuntimeConfig {
    pub family: LocalModelFamily,
    pub model: String,
    pub port: u16,
    pub proxy_port: Option<u16>,
    pub max_seq_len: Option<u32>,
    pub max_seqs: u32,
    pub max_batch_size: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EngineFamilyProfile {
    pub family: LocalModelFamily,
    pub launcher_mode: String,
    pub arch: Option<String>,
    pub paged_attn: String,
    pub pa_cache_type: Option<String>,
    pub pa_memory_fraction: Option<String>,
    pub pa_context_len: Option<u32>,
    pub max_seq_len: u32,
    pub max_batch_size: u32,
    pub max_seqs: u32,
    pub isq: Option<String>,
    pub tensor_parallel_backend: Option<String>,
    pub disable_nccl: bool,
    pub target_world_size: Option<u32>,
    pub preferred_gpu_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalModelProfile {
    pub runtime: EngineRuntimeConfig,
    pub family_profile: EngineFamilyProfile,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanRoomBaselinePlan {
    pub source_layout: SourceLayoutPaths,
    pub runtime: EngineRuntimeConfig,
    pub family_profile: EngineFamilyProfile,
    pub engine_command: Vec<String>,
    pub bridge_mode: String,
}

pub use crate::execution::models::model_registry::SUPPORTED_ANTHROPIC_API_CHAT_MODELS;
pub use crate::execution::models::model_registry::SUPPORTED_CHAT_MODELS;
pub use crate::execution::models::model_registry::SUPPORTED_EMBEDDING_MODELS;
pub use crate::execution::models::model_registry::SUPPORTED_LOCAL_CHAT_FAMILIES;
pub use crate::execution::models::model_registry::SUPPORTED_OPENAI_API_CHAT_MODELS;
pub use crate::execution::models::model_registry::SUPPORTED_MINIMAX_API_CHAT_MODELS;
pub use crate::execution::models::model_registry::SUPPORTED_OPENROUTER_API_CHAT_MODELS;
pub use crate::execution::models::model_registry::SUPPORTED_STT_MODELS;
pub use crate::execution::models::model_registry::SUPPORTED_TTS_MODELS;

impl ChatModelFamily {
    pub fn label(self) -> &'static str {
        model_registry::chat_family_catalog_entry(self)
            .map(|entry| entry.label)
            .expect("chat family registry must cover every ChatModelFamily")
    }

    pub fn selector(self) -> &'static str {
        model_registry::chat_family_catalog_entry(self)
            .map(|entry| entry.selector)
            .expect("chat family registry must cover every ChatModelFamily")
    }

    pub fn variants(self) -> &'static [&'static str] {
        model_registry::chat_family_catalog_entry(self)
            .map(|entry| entry.variants)
            .expect("chat family registry must cover every ChatModelFamily")
    }
}

pub fn parse_chat_model_family(value: &str) -> Option<ChatModelFamily> {
    model_registry::parse_chat_model_family(value)
}

pub fn chat_model_family_for_model(model: &str) -> Option<ChatModelFamily> {
    model_registry::chat_model_family_for_model(model)
}

/// True when the model can natively accept image content blocks. Consulted
/// by the vision preprocessor to decide whether images must be described
/// via the Vision aux before reaching the primary model. See
/// [`model_registry::model_supports_vision`] for resolution details.
pub fn model_supports_vision(model: &str) -> bool {
    model_registry::model_supports_vision(model)
}

pub fn auxiliary_model_selection(
    role: AuxiliaryRole,
    configured_model: Option<&str>,
) -> AuxiliaryModelSelection {
    model_registry::auxiliary_model_selection(role, configured_model)
}

pub fn supported_local_model_profiles() -> Vec<LocalModelProfile> {
    model_registry::supported_local_model_profiles()
}

pub fn model_profile_for_model(model: &str) -> anyhow::Result<LocalModelProfile> {
    let normalized = normalize_supported_model(model);
    model_registry::model_profile_for_model(normalized)
        .ok_or_else(|| anyhow::anyhow!("unsupported local model profile: {normalized}"))
}

fn default_integration_mode() -> String {
    "hard_fork".to_string()
}

pub fn load_source_origins_manifest(root: &Path) -> anyhow::Result<SourceOriginsManifest> {
    let path = root.join("contracts/source_origins_manifest.json");
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read source origins manifest at {path:?}"))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse source origins manifest at {path:?}"))
}

pub fn discover_source_layout_paths(root: &Path) -> SourceLayoutPaths {
    let tools_root = root.join("tools");
    let agent_runtime_root = tools_root.join("agent-runtime");
    let model_runtime_root = tools_root.join("model-runtime");
    SourceLayoutPaths {
        tools_root,
        codex_exec_binary: agent_runtime_root.join("target/release/codex-exec"),
        agent_runtime_root,
        model_runtime_binary: model_runtime_root.join("target/release/ctox-engine"),
        model_runtime_root,
    }
}

pub fn default_runtime_config(family: LocalModelFamily) -> EngineRuntimeConfig {
    model_registry::default_runtime_config(family)
        .expect("local family registry must cover every LocalModelFamily")
}

pub fn runtime_config_for_model(model: &str) -> anyhow::Result<EngineRuntimeConfig> {
    Ok(model_profile_for_model(model)?.runtime)
}

pub fn is_openai_api_chat_model(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    SUPPORTED_OPENAI_API_CHAT_MODELS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&normalized))
}

pub fn is_openrouter_api_chat_model(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    SUPPORTED_OPENROUTER_API_CHAT_MODELS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&normalized))
}

pub fn is_anthropic_api_chat_model(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    SUPPORTED_ANTHROPIC_API_CHAT_MODELS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&normalized))
}

pub fn is_minimax_api_chat_model(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    SUPPORTED_MINIMAX_API_CHAT_MODELS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&normalized))
}

pub fn supports_local_chat_runtime(model: &str) -> bool {
    runtime_config_for_model(model).is_ok()
}

pub fn is_api_chat_model(model: &str) -> bool {
    is_openai_api_chat_model(model)
        || is_anthropic_api_chat_model(model)
        || is_minimax_api_chat_model(model)
        || (is_openrouter_api_chat_model(model) && !supports_local_chat_runtime(model))
}

pub fn api_provider_supports_model(provider: &str, model: &str) -> bool {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openrouter" => is_openrouter_api_chat_model(model),
        "anthropic" => is_anthropic_api_chat_model(model),
        "minimax" => is_minimax_api_chat_model(model),
        "openai" => is_openai_api_chat_model(model),
        _ => false,
    }
}

pub fn default_api_provider_for_model(model: &str) -> &'static str {
    if is_anthropic_api_chat_model(model) {
        "anthropic"
    } else if is_minimax_api_chat_model(model) {
        "minimax"
    } else if is_openrouter_api_chat_model(model) {
        "openrouter"
    } else {
        "openai"
    }
}

pub fn uses_ctox_proxy_model(model: &str) -> bool {
    supports_local_chat_runtime(model)
}

fn normalize_supported_model(model: &str) -> &str {
    let trimmed = model.trim();
    match model_registry::canonical_model_id(trimmed) {
        Some(canonical) => canonical,
        None => trimmed,
    }
}

pub fn default_family_profile(family: LocalModelFamily) -> EngineFamilyProfile {
    model_registry::default_family_profile(family)
        .expect("local family registry must cover every LocalModelFamily")
}

pub fn runtime_profile_for_model(model: &str) -> anyhow::Result<EngineFamilyProfile> {
    Ok(model_profile_for_model(model)?.family_profile)
}

pub fn resolve_pa_cache_type(
    default: Option<&str>,
    _env_map: &BTreeMap<String, String>,
) -> Option<String> {
    default.map(str::to_string)
}

pub fn resolve_model_pa_cache_type(
    model: &str,
    default: Option<&str>,
    env_map: &BTreeMap<String, String>,
) -> Option<String> {
    let resolved = resolve_pa_cache_type(default, env_map);
    match resolved {
        Some(cache_type) if !model_supports_pa_cache_type(model, &cache_type) => default
            .filter(|cache_type| model_supports_pa_cache_type(model, cache_type))
            .map(str::to_string),
        other => other,
    }
}

pub fn resolve_model_paged_attn(model: &str, default: &str, cache_type: Option<&str>) -> String {
    if default != "off" {
        return default.to_string();
    }
    if cache_type.is_some() && model_supports_paged_attention_cache(model) {
        return "auto".to_string();
    }
    default.to_string()
}

pub fn model_supports_paged_attention_cache(model: &str) -> bool {
    model_profile_for_model(model)
        .map(|profile| {
            matches!(
                profile.runtime.family,
                LocalModelFamily::GptOss
                    | LocalModelFamily::Qwen35Vision
                    | LocalModelFamily::Gemma4Vision
                    | LocalModelFamily::NemotronCascade
                    | LocalModelFamily::Glm47Flash
            )
        })
        .unwrap_or(false)
}

pub fn model_supports_pa_cache_type(model: &str, cache_type: &str) -> bool {
    let normalized = cache_type.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "auto" | "f8e4m3" => model_supports_paged_attention_cache(model),
        TURBOQUANT2_CACHE_TYPE | TURBOQUANT4_CACHE_TYPE => false,
        TURBOQUANT3_CACHE_TYPE => model_profile_for_model(model)
            .map(|profile| {
                matches!(
                    profile.runtime.family,
                    LocalModelFamily::GptOss
                        | LocalModelFamily::Qwen35Vision
                        | LocalModelFamily::NemotronCascade
                        | LocalModelFamily::Glm47Flash
                )
            })
            .unwrap_or(false),
        _ => false,
    }
}

pub fn build_engine_command(
    source_layout: &SourceLayoutPaths,
    runtime: &EngineRuntimeConfig,
) -> Vec<String> {
    let family_profile = runtime_profile_for_model(&runtime.model)
        .unwrap_or_else(|_| default_family_profile(runtime.family));
    let mut command = vec![source_layout.model_runtime_binary.display().to_string()];
    match runtime.family {
        LocalModelFamily::GptOss
        | LocalModelFamily::NemotronCascade
        | LocalModelFamily::Glm47Flash => {
            command.extend([
                "serve".to_string(),
                "--port".to_string(),
                runtime.port.to_string(),
                "--max-seqs".to_string(),
                runtime.max_seqs.to_string(),
                "--max-batch-size".to_string(),
                runtime.max_batch_size.to_string(),
                "--paged-attn".to_string(),
                family_profile.paged_attn,
                "-m".to_string(),
                runtime.model.clone(),
            ]);
            if let Some(arch) = family_profile.arch {
                command.extend(["-a".to_string(), arch]);
            }
            if let Some(max_seq_len) = runtime.max_seq_len {
                command.extend(["--max-seq-len".to_string(), max_seq_len.to_string()]);
            }
        }
        LocalModelFamily::Qwen35Vision
        | LocalModelFamily::Gemma4Vision
        | LocalModelFamily::Qwen3VisionAuxiliary => {
            command.extend([
                "serve".to_string(),
                "-p".to_string(),
                runtime.port.to_string(),
                "vision".to_string(),
                "-m".to_string(),
                runtime.model.clone(),
            ]);
        }
        LocalModelFamily::Qwen3Embedding => {
            command.extend([
                "serve".to_string(),
                "-p".to_string(),
                runtime.port.to_string(),
                "embedding".to_string(),
                "-m".to_string(),
                runtime.model.clone(),
            ]);
        }
        LocalModelFamily::WhisperTranscriptionCpu | LocalModelFamily::VoxtralTranscription => {
            command.extend([
                "serve".to_string(),
                "-p".to_string(),
                runtime.port.to_string(),
                "vision".to_string(),
                "-m".to_string(),
                runtime.model.clone(),
            ]);
        }
        LocalModelFamily::PiperSpeech | LocalModelFamily::Qwen3Speech => {
            command.extend([
                "serve".to_string(),
                "-p".to_string(),
                runtime.port.to_string(),
            ]);
            if let Some(isq) = family_profile.isq.clone() {
                command.extend(["--isq".to_string(), isq]);
            }
            command.extend([
                "speech".to_string(),
                "-m".to_string(),
                runtime.model.clone(),
            ]);
        }
        LocalModelFamily::VoxtralSpeech => {
            command.extend([
                "serve".to_string(),
                "-p".to_string(),
                runtime.port.to_string(),
            ]);
            if let Some(isq) = family_profile.isq.clone() {
                command.extend(["--isq".to_string(), isq]);
            }
            command.extend([
                "speech".to_string(),
                "-m".to_string(),
                runtime.model.clone(),
            ]);
            if let Some(arch) = family_profile.arch.clone() {
                command.extend(["-a".to_string(), arch]);
            }
        }
    }
    command
}

pub fn build_clean_room_baseline_plan(
    root: &Path,
    family: LocalModelFamily,
    _prompt: String,
) -> CleanRoomBaselinePlan {
    let source_layout = discover_source_layout_paths(root);
    let runtime = default_runtime_config(family);
    let family_profile = default_family_profile(family);
    let engine_command = build_engine_command(&source_layout, &runtime);
    let bridge_mode = model_registry::bridge_mode_for_family(family)
        .expect("local family registry must cover every LocalModelFamily")
        .to_string();
    CleanRoomBaselinePlan {
        source_layout,
        runtime,
        family_profile,
        engine_command,
        bridge_mode,
    }
}

fn escape_toml_inline_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn source_layout_status(root: &Path) -> anyhow::Result<SourceLayoutStatusOutcome> {
    let manifest = load_source_origins_manifest(root)?;
    let mut results = Vec::new();
    let mut ready = true;
    for source_tree in manifest.source_trees {
        let target_dir = root.join(&source_tree.target_dir);
        let present = target_dir.is_dir();
        let nested_git_metadata = target_dir.join(".git").exists();
        let source_owned_in_repo = present && !nested_git_metadata;
        let mut missing_paths = Vec::new();
        for required_path in &source_tree.required_paths {
            let absolute = root.join(required_path);
            if !absolute.exists() {
                missing_paths.push(absolute);
            }
        }
        let mut notes = source_tree.lineage_notes.clone();
        if !present {
            notes
                .push("integrated hard-fork source tree is missing from this checkout".to_string());
        }
        if nested_git_metadata {
            notes.push(
                "nested .git metadata detected; integrated hard-fork trees must remain source-owned inside CTOX".to_string(),
            );
        }
        if source_tree.source_snapshot.is_none() {
            notes.push(
                "exact seed snapshot is not normalized here; treat the checked-in tree as authoritative CTOX fork state".to_string(),
            );
        }
        if source_tree.integration_mode != "hard_fork" {
            notes.push(format!(
                "unexpected integration mode `{}`; CTOX expects integrated hard forks here",
                source_tree.integration_mode
            ));
        }
        let source_tree_ready = present
            && source_owned_in_repo
            && missing_paths.is_empty()
            && source_tree.integration_mode == "hard_fork";
        ready &= source_tree_ready;
        results.push(SourceTreeStatus {
            name: source_tree.name,
            target_dir,
            purpose: source_tree.purpose,
            source_origin_url: source_tree.source_origin_url,
            source_snapshot: source_tree.source_snapshot,
            integration_mode: source_tree.integration_mode,
            present,
            source_owned_in_repo,
            missing_paths,
            notes,
        });
    }
    Ok(SourceLayoutStatusOutcome {
        manifest_path: root.join("contracts/source_origins_manifest.json"),
        ready,
        results,
    })
}

pub fn rewrite_engine_responses_request(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut payload: Value =
        serde_json::from_slice(raw).context("failed to parse responses request")?;

    if let Some(tools) = payload.get_mut("tools").and_then(Value::as_array_mut) {
        let mut rewritten = Vec::new();
        for tool in tools.drain(..) {
            if let Some(tool) = rewrite_tool(tool) {
                rewritten.push(tool);
            }
        }
        *tools = rewritten;
    }

    if payload.get("parallel_tool_calls") == Some(&Value::Bool(false)) {
        payload["parallel_tool_calls"] = Value::Bool(true);
    }
    if let Some(object) = payload.as_object_mut() {
        object.remove("max_tool_calls");
    }

    serde_json::to_vec(&payload).context("failed to encode rewritten responses request")
}

pub fn rewrite_openai_responses_request(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut payload: Value =
        serde_json::from_slice(raw).context("failed to parse responses request")?;

    if let Some(tools) = payload.get_mut("tools").and_then(Value::as_array_mut) {
        let mut rewritten = Vec::new();
        for tool in tools.drain(..) {
            rewritten.extend(rewrite_openai_tool(tool));
        }
        *tools = rewritten;
    }

    serde_json::to_vec(&payload).context("failed to encode OpenAI responses request")
}

pub fn rewrite_responses_payload_to_sse(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse responses payload")?;
    let response_id = payload
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("resp_ctox_proxy");
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let usage = payload.get("usage").cloned().unwrap_or_else(|| {
        json!({
            "input_tokens": 0,
            "output_tokens": 0,
            "total_tokens": 0
        })
    });
    let mut frames = Vec::new();
    frames.push((
        "response.created",
        json!({
            "type": "response.created",
            "response": {
                "id": response_id,
                "model": model
            }
        })
        .to_string(),
    ));
    if let Some(items) = payload.get("output").and_then(Value::as_array) {
        for item in items {
            push_response_output_item_frames(&mut frames, item);
        }
    }
    frames.push((
        "response.completed",
        json!({
            "type": "response.completed",
            "response": {
                "id": response_id,
                "model": model,
                "usage": {
                    "input_tokens": usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0),
                    "output_tokens": usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0),
                    "total_tokens": usage.get("total_tokens").and_then(Value::as_u64).unwrap_or(0)
                }
            }
        })
        .to_string(),
    ));
    Ok(frames
        .into_iter()
        .map(|(event, frame)| format!("event: {event}\ndata: {frame}\n\n"))
        .chain(std::iter::once("data: [DONE]\n\n".to_string()))
        .collect::<String>()
        .into_bytes())
}

fn push_response_output_item_frames(frames: &mut Vec<(&'static str, String)>, item: &Value) {
    if let Some(partial_item) = partial_output_item_for_sse(item) {
        frames.push((
            "response.output_item.added",
            json!({
                "type": "response.output_item.added",
                "item": partial_item,
            })
            .to_string(),
        ));
    }

    frames.push((
        "response.output_item.done",
        json!({
            "type": "response.output_item.done",
            "item": item
        })
        .to_string(),
    ));
}

fn partial_output_item_for_sse(item: &Value) -> Option<Value> {
    let item_type = item.get("type").and_then(Value::as_str)?;
    if item_type != "web_search_call" {
        return None;
    }

    let id = item.get("id").and_then(Value::as_str)?;
    let status = item
        .get("status")
        .and_then(Value::as_str)
        .filter(|status| !status.is_empty())
        .unwrap_or("in_progress");
    let partial_status = match status {
        "completed" | "failed" => "in_progress",
        other => other,
    };

    Some(json!({
        "type": item_type,
        "id": id,
        "status": partial_status,
    }))
}

#[allow(dead_code)]
fn responses_input_item_to_chat_message(item: &Value) -> Option<Value> {
    let object = item.as_object()?;
    let item_type = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    match item_type {
        "message" => {
            let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
            let mapped_role = match role {
                "developer" => "system",
                other => other,
            };
            let text = extract_message_content_text(object.get("content"));
            Some(json!({
                "role": mapped_role,
                "content": text,
            }))
        }
        "function_call" => {
            let call_id = object
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or("call_ctox_proxy");
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let arguments = object
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            Some(json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments
                    }
                }]
            }))
        }
        "function_call_output" => {
            let call_id = object
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or("call_ctox_proxy");
            let output = extract_function_call_output_text(object.get("output"));
            Some(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output,
            }))
        }
        _ => None,
    }
}

pub fn responses_request_streams(raw: &[u8]) -> anyhow::Result<bool> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse responses request")?;
    Ok(payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false))
}

pub(crate) fn rewrite_tool(tool: Value) -> Option<Value> {
    let object = tool.as_object()?;
    let tool_type = object.get("type")?.as_str()?;
    match tool_type {
        "function" => {
            if let Some(function) = object.get("function").and_then(Value::as_object) {
                let name = function.get("name").and_then(Value::as_str)?;
                if DISALLOWED_ENGINE_FUNCTION_TOOLS.contains(&name) {
                    return None;
                }
                return Some(Value::Object(object.clone()));
            }

            let name = object.get("name").and_then(Value::as_str)?;
            if DISALLOWED_ENGINE_FUNCTION_TOOLS.contains(&name) {
                return None;
            }
            let mut function_payload = serde_json::Map::new();
            for (key, value) in object {
                if key == "type" || key == "function" {
                    continue;
                }
                function_payload.insert(key.clone(), value.clone());
            }
            Some(serde_json::json!({
                "type": "function",
                "function": function_payload,
            }))
        }
        "namespace" => {
            let children = object.get("tools")?.as_array()?;
            let rewritten_children: Vec<Value> = children
                .iter()
                .filter_map(|child| rewrite_tool(child.clone()))
                .collect();
            if rewritten_children.is_empty() {
                return None;
            }
            let mut rewritten = object.clone();
            rewritten.insert("tools".to_string(), Value::Array(rewritten_children));
            Some(Value::Object(rewritten))
        }
        _ => None,
    }
}

fn rewrite_openai_tool(tool: Value) -> Vec<Value> {
    let Some(object) = tool.as_object() else {
        return Vec::new();
    };
    let Some(tool_type) = object.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };
    match tool_type {
        "web_search" => vec![Value::Object(object.clone())],
        "function" => {
            let function = object
                .get("function")
                .and_then(Value::as_object)
                .unwrap_or(object);
            let Some(name) = function.get("name").and_then(Value::as_str) else {
                return Vec::new();
            };
            if DISALLOWED_ENGINE_FUNCTION_TOOLS.contains(&name) {
                return Vec::new();
            }
            let mut flattened = serde_json::Map::new();
            flattened.insert("type".to_string(), Value::String("function".to_string()));
            for key in ["name", "description", "parameters", "strict"] {
                if let Some(value) = function.get(key) {
                    flattened.insert(key.to_string(), value.clone());
                }
            }
            vec![Value::Object(flattened)]
        }
        "namespace" => object
            .get("tools")
            .and_then(Value::as_array)
            .map(|children| {
                children
                    .iter()
                    .flat_map(|child| rewrite_openai_tool(child.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub(crate) fn extract_message_content_text(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    if let Some(entries) = content.as_array() {
        let mut parts = Vec::new();
        for entry in entries {
            if let Some(text) = entry.get("text").and_then(Value::as_str) {
                if !text.trim().is_empty() {
                    parts.push(text.to_string());
                }
            }
        }
        return parts.join("\n");
    }
    String::new()
}

/// Normalise a message's `content` field into an OpenAI chat-compat block
/// array, preserving both text and image blocks. String inputs are lifted
/// to `[{type:"text",text:"..."}]`. Array inputs keep their items but are
/// normalised:
///
/// - `{type:"input_text"|"text", text:"..."}` → `{type:"text", text:"..."}`
/// - `{type:"input_image", image_url:"...", image_data?, mime_type?}` →
///   `{type:"image_url", image_url:{url:"..."}}` (base64 payloads are
///   converted to data-URIs).
/// - `{type:"image_url", image_url:{url:"..."}}` → kept as-is.
/// - Other types are dropped.
///
/// Used by adapters for vision-capable model families (Qwen 3.5, Gemma 4,
/// Mistral) so image content reaches the ctox-engine unchanged. Text-only
/// adapters continue to call `extract_message_content_text` which strips
/// images (they can't consume them anyway; the vision preprocessor will
/// have substituted image blocks with text upstream if the primary isn't
/// vision-capable).
pub(crate) fn extract_message_content_blocks(content: Option<&Value>) -> Vec<Value> {
    let Some(content) = content else {
        return Vec::new();
    };
    if let Some(text) = content.as_str() {
        if text.trim().is_empty() {
            return Vec::new();
        }
        return vec![serde_json::json!({"type":"text","text":text})];
    }
    let Some(entries) = content.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(object) = entry.as_object() else {
            continue;
        };
        let ty = object.get("type").and_then(Value::as_str).unwrap_or("");
        match ty {
            "text" | "input_text" | "output_text" => {
                if let Some(text) = object.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        out.push(serde_json::json!({"type":"text","text":text}));
                    }
                }
            }
            "input_image" => {
                if let Some(url) = object.get("image_url").and_then(Value::as_str) {
                    if !url.trim().is_empty() {
                        out.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": {"url": url},
                        }));
                        continue;
                    }
                }
                if let Some(data) = object.get("image_data").and_then(Value::as_str) {
                    if !data.trim().is_empty() {
                        let mime = object
                            .get("mime_type")
                            .and_then(Value::as_str)
                            .unwrap_or("image/png");
                        out.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": {"url": format!("data:{mime};base64,{data}")},
                        }));
                    }
                }
            }
            "image_url" => {
                // Already OpenAI chat-compat shape — pass through as-is.
                // Supports both {url:".."} object and plain string for robustness.
                if let Some(inner) = object.get("image_url") {
                    if inner.is_object() {
                        out.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": inner.clone(),
                        }));
                    } else if let Some(url) = inner.as_str() {
                        if !url.trim().is_empty() {
                            out.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": {"url": url},
                            }));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// True when a block array contains at least one image entry. Adapters use
/// this to decide whether to forward the full block array (vision path) or
/// fall back to flat-text extraction (legacy path).
pub(crate) fn message_blocks_contain_image(blocks: &[Value]) -> bool {
    blocks.iter().any(|block| {
        block
            .get("type")
            .and_then(Value::as_str)
            .map(|ty| ty == "image_url" || ty == "input_image")
            .unwrap_or(false)
    })
}

pub(crate) fn extract_function_call_output_text(output: Option<&Value>) -> String {
    fn render(value: &Value) -> Option<String> {
        match value {
            Value::String(text) => Some(text.clone()),
            Value::Array(items) => {
                let parts = items
                    .iter()
                    .filter_map(render)
                    .filter(|text| !text.trim().is_empty())
                    .collect::<Vec<_>>();
                if parts.is_empty() {
                    None
                } else {
                    Some(parts.join("\n"))
                }
            }
            Value::Object(map) => {
                if let Some(text) = map.get("text").and_then(Value::as_str) {
                    return Some(text.to_string());
                }
                if let Some(content) = map.get("content") {
                    return render(content);
                }
                serde_json::to_string(value).ok()
            }
            _ => serde_json::to_string(value).ok(),
        }
    }
    output.and_then(render).unwrap_or_default()
}

pub(crate) fn normalize_responses_input(input: Option<&Value>) -> Vec<Value> {
    match input {
        None => Vec::new(),
        Some(Value::String(text)) => vec![json!({
            "type": "message",
            "role": "user",
            "content": [{ "type": "input_text", "text": text }]
        })],
        Some(Value::Array(items)) => items.clone(),
        Some(other) => vec![json!({
            "type": "message",
            "role": "user",
            "content": [{ "type": "input_text", "text": other.to_string() }]
        })],
    }
}

pub fn extract_exact_text_override_from_materialized_request(
    request_payload: &Value,
) -> Option<String> {
    let latest_user_text = normalize_responses_input(request_payload.get("input"))
        .into_iter()
        .rev()
        .find_map(|item| {
            let object = item.as_object()?;
            if object
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("message")
                != "message"
            {
                return None;
            }
            if object.get("role").and_then(Value::as_str).unwrap_or("user") != "user" {
                return None;
            }
            let text = extract_message_content_text(object.get("content"));
            if text.trim().is_empty() {
                None
            } else {
                Some(text)
            }
        })?;
    let exact_text_override = extract_explicit_exact_text_request(&latest_user_text)?;
    if should_preserve_tool_capability_for_exact_text_request(request_payload, &latest_user_text) {
        return None;
    }
    Some(exact_text_override)
}

fn should_preserve_tool_capability_for_exact_text_request(
    request_payload: &Value,
    latest_user_text: &str,
) -> bool {
    let has_tools = request_payload
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty());
    if !has_tools {
        return false;
    }

    let lower = latest_user_text.to_ascii_lowercase();
    let workspace_bound = lower.contains("work only inside this workspace")
        || lower.contains("work only in this workspace")
        || lower.contains("workspace:")
        || lower.contains("workspace root")
        || lower.contains("workspace_root")
        || latest_user_text.contains("/home/")
        || latest_user_text.contains("/tmp/")
        || latest_user_text.contains("/Users/");
    let has_action_verb = [
        "create ",
        "edit ",
        "modify ",
        "implement ",
        "build ",
        "compile ",
        "run ",
        "test ",
        "verify ",
        "fix ",
        "debug ",
        "refactor ",
        "rename ",
        "patch ",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    let has_strong_verification_marker = [
        "cmake",
        "cargo ",
        "pytest",
        "npm ",
        "pnpm ",
        "make ",
        "./build/",
        "do not answer before",
        "on successful run",
        "must print exactly",
        "must output exactly",
        "verify the binary",
        "create at least these files",
        ".cpp",
        ".cc",
        ".cxx",
        ".h",
        ".hpp",
        "cmakelists.txt",
    ]
    .iter()
    .any(|needle| lower.contains(needle));

    (workspace_bound && has_action_verb) || has_strong_verification_marker
}

fn extract_explicit_exact_text_request(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let quoted = Regex::new(r#"(?is)^\s*(?:please\s+)?(?:reply|respond|return|output|say|print|emit|antworte(?:\s+genau)?(?:\s+mit)?)\s+(?:with\s+|exactly\s+|genau\s+|mit\s+)?(?:"([^"\r\n]{1,128})"|'([^'\r\n]{1,128})')\s*(?:and\s+nothing\s+else|und\s+nichts\s+ander(?:em|es))?[.!]?\s*$"#)
        .expect("valid exact-text quoted regex");
    if let Some(captures) = quoted.captures(trimmed) {
        return captures
            .get(1)
            .or_else(|| captures.get(2))
            .map(|capture| capture.as_str().to_string())
            .filter(|value| !value.trim().is_empty());
    }
    let bare = Regex::new(r#"(?is)^\s*(?:please\s+)?(?:reply|respond|return|output|say|print|emit)\s+(?:with\s+)?exactly\s+([^\r\n]{1,128}?)\s*(?:and\s+nothing\s+else)?[.!]?\s*$"#)
        .expect("valid exact-text bare regex");
    let german_bare = Regex::new(r#"(?is)^\s*antworte(?:\s+genau)?(?:\s+mit)?\s+([^\r\n]{1,128}?)\s*(?:und\s+nichts\s+ander(?:em|es))?[.!]?\s*$"#)
        .expect("valid exact-text german bare regex");
    bare.captures(trimmed)
        .and_then(|captures| captures.get(1))
        .map(|capture| {
            capture
                .as_str()
                .trim()
                .trim_end_matches(['.', '!', '?'])
                .trim()
                .to_string()
        })
        .filter(|value| !value.is_empty())
        .or_else(|| {
            german_bare
                .captures(trimmed)
                .and_then(|captures| captures.get(1))
                .map(|capture| {
                    capture
                        .as_str()
                        .trim()
                        .trim_end_matches(['.', '!', '?'])
                        .trim()
                        .to_string()
                })
                .filter(|value| !value.is_empty())
        })
        .or_else(|| extract_embedded_exact_text_request(trimmed))
}

fn extract_embedded_exact_text_request(text: &str) -> Option<String> {
    let quoted = Regex::new(r#"(?is)(?:please\s+)?(?:reply|respond|return|output|say|print|emit|antworte(?:\s+genau)?(?:\s+mit)?)\s+(?:with\s+|exactly\s+|genau\s+|mit\s+)?(?:"([^"\r\n]{1,128})"|'([^'\r\n]{1,128})')\s*(?:and\s+nothing\s+else|und\s+nichts\s+ander(?:em|es))?[.!]?\s*$"#)
        .expect("valid embedded exact-text quoted regex");
    let bare = Regex::new(r#"(?is)(?:please\s+)?(?:reply|respond|return|output|say|print|emit)\s+(?:with\s+)?exactly\s+([^\r\n]{1,128}?)\s*(?:and\s+nothing\s+else)?[.!]?\s*$"#)
        .expect("valid embedded exact-text bare regex");
    let german_bare = Regex::new(r#"(?is)antworte(?:\s+genau)?(?:\s+mit)?\s+([^\r\n]{1,128}?)\s*(?:und\s+nichts\s+ander(?:em|es))?[.!]?\s*$"#)
        .expect("valid embedded exact-text german bare regex");

    let clauses = split_exact_request_clauses(text);
    for clause in clauses.into_iter().rev() {
        let clause = clause.trim();
        if clause.is_empty() {
            continue;
        }
        if let Some(captures) = quoted.captures(clause) {
            let candidate = captures
                .get(1)
                .or_else(|| captures.get(2))
                .map(|capture| capture.as_str().trim().to_string())
                .filter(|value| !value.is_empty());
            if candidate.is_some() {
                return candidate;
            }
        }
        if let Some(captures) = bare.captures(clause) {
            let candidate = captures
                .get(1)
                .map(|capture| {
                    capture
                        .as_str()
                        .trim()
                        .trim_end_matches(['.', '!', '?'])
                        .trim()
                        .to_string()
                })
                .filter(|value| !value.is_empty());
            if candidate.is_some() {
                return candidate;
            }
        }
        if let Some(captures) = german_bare.captures(clause) {
            let candidate = captures
                .get(1)
                .map(|capture| {
                    capture
                        .as_str()
                        .trim()
                        .trim_end_matches(['.', '!', '?'])
                        .trim()
                        .to_string()
                })
                .filter(|value| !value.is_empty());
            if candidate.is_some() {
                return candidate;
            }
        }
    }

    None
}

fn split_exact_request_clauses(text: &str) -> Vec<&str> {
    let mut clauses = Vec::new();
    let mut start = 0usize;
    for (idx, ch) in text.char_indices() {
        if matches!(ch, '.' | '!' | '?' | '\n' | '\r') {
            let end = idx + ch.len_utf8();
            clauses.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        clauses.push(&text[start..]);
    }
    clauses
}

pub fn materialize_responses_request(
    raw: &[u8],
    previous_conversation: Option<&Value>,
) -> anyhow::Result<Value> {
    let mut payload: Value = serde_json::from_slice(raw)
        .context("failed to parse responses request for materialization")?;
    let current_items = normalize_responses_input(payload.get("input"));
    let merged_items = if let Some(previous) = previous_conversation.and_then(Value::as_array) {
        let mut merged = previous.clone();
        merged.extend(current_items);
        merged
    } else {
        current_items
    };
    if let Some(object) = payload.as_object_mut() {
        object.insert("input".to_string(), Value::Array(merged_items));
        object.remove("previous_response_id");
    }
    Ok(payload)
}

pub fn extend_conversation_with_response(
    request_payload: &Value,
    response_payload: &Value,
) -> anyhow::Result<Value> {
    let mut conversation = normalize_responses_input(request_payload.get("input"));
    if let Some(output_items) = response_payload.get("output").and_then(Value::as_array) {
        conversation.extend(output_items.iter().cloned());
    }
    Ok(Value::Array(conversation))
}

pub(crate) fn responses_turn_builder(
    payload: &Value,
    fallback_model: Option<&str>,
    default_model: &str,
) -> turn_contract::TurnResponseBuilder {
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .or(fallback_model)
        .unwrap_or(default_model)
        .to_string();
    let response_id = payload
        .get("id")
        .and_then(Value::as_str)
        .map(|value| format!("resp_{value}"))
        .unwrap_or_else(|| "resp_ctox_proxy".to_string());
    let created_at = payload
        .get("created")
        .and_then(Value::as_u64)
        .unwrap_or_else(current_unix_ts);
    turn_contract::TurnResponseBuilder::new(response_id, model, created_at, current_unix_ts())
        .with_usage(turn_contract::TurnUsage::from_usage_payload(
            payload.get("usage"),
        ))
}

pub(crate) fn current_unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[allow(dead_code)]
fn flatten_input_items(input: &Value) -> Option<String> {
    let items = input.as_array()?;
    let mut parts = Vec::new();
    for item in items {
        let object = item.as_object()?;
        let role = object
            .get("role")
            .and_then(Value::as_str)
            .or_else(|| object.get("type").and_then(Value::as_str))
            .unwrap_or("message");
        let content = object.get("content")?;
        let mut chunks = Vec::new();
        if let Some(text) = content.as_str() {
            chunks.push(text.to_string());
        } else if let Some(entries) = content.as_array() {
            for part in entries {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    if !text.is_empty() {
                        chunks.push(text.to_string());
                    }
                }
            }
        }
        if !chunks.is_empty() {
            parts.push(format!("[{role}]\n{}", chunks.join("\n")));
        }
    }
    Some(parts.join("\n\n"))
}

#[cfg(test)]
use crate::inference::model_adapters::gpt_oss::HarmonyToolSpec;

#[cfg(test)]
fn should_use_gpt_oss_harmony_proxy(raw: &[u8]) -> anyhow::Result<bool> {
    crate::inference::model_adapters::gpt_oss::should_use_proxy(raw)
}

#[cfg(test)]
fn rewrite_responses_to_qwen_chat_completions(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::qwen35::rewrite_request(raw)
}

#[cfg(test)]
fn rewrite_responses_to_nemotron_chat_completions(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::nemotron::rewrite_request(raw)
}

#[cfg(test)]
fn rewrite_responses_to_gpt_oss_chat_completions(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::gpt_oss::rewrite_chat_request(raw)
}

#[cfg(test)]
fn rewrite_responses_to_glm_chat_completions(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::glm47::rewrite_request(raw)
}

#[cfg(test)]
fn rewrite_responses_to_gemma4_chat_completions(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::gemma4::rewrite_request(raw)
}

#[cfg(test)]
fn rewrite_qwen_chat_completions_to_responses(
    raw: &[u8],
    fallback_model: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::qwen35::rewrite_success_response(raw, fallback_model, None)
}

#[cfg(test)]
fn rewrite_nemotron_chat_completions_to_responses(
    raw: &[u8],
    fallback_model: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::nemotron::rewrite_success_response(raw, fallback_model, None)
}

#[cfg(test)]
fn rewrite_glm_chat_completions_to_responses(
    raw: &[u8],
    fallback_model: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::glm47::rewrite_success_response(raw, fallback_model, None)
}

#[cfg(test)]
fn rewrite_gpt_oss_chat_completions_to_responses(
    raw: &[u8],
    fallback_model: Option<&str>,
    exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::gpt_oss::rewrite_chat_success_response(
        raw,
        fallback_model,
        exact_text_override,
    )
}

#[cfg(test)]
fn rewrite_gemma4_chat_completions_to_responses(
    raw: &[u8],
    fallback_model: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::gemma4::rewrite_success_response(raw, fallback_model, None)
}

#[cfg(test)]
fn rewrite_responses_to_gpt_oss_completion(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::gpt_oss::rewrite_request(raw)
}

#[cfg(test)]
fn rewrite_gpt_oss_completion_to_responses(
    raw: &[u8],
    fallback_model: Option<&str>,
    exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::gpt_oss::rewrite_success_response(
        raw,
        fallback_model,
        exact_text_override,
    )
}

#[cfg(test)]
fn rewrite_gpt_oss_completion_to_sse(
    raw: &[u8],
    fallback_model: Option<&str>,
    exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    crate::inference::model_adapters::gpt_oss::rewrite_success_response_to_sse(
        raw,
        fallback_model,
        exact_text_override,
    )
}

#[cfg(test)]
fn build_gpt_oss_followup_completion_request(
    initial_request_raw: &[u8],
    first_completion_raw: &[u8],
) -> anyhow::Result<Option<Vec<u8>>> {
    crate::inference::model_adapters::gpt_oss::build_followup_request(
        initial_request_raw,
        first_completion_raw,
    )
}

#[cfg(test)]
fn ensure_gpt_oss_thinking_output_floor(model_id: &str, requested: usize) -> usize {
    crate::inference::model_adapters::gpt_oss::ensure_thinking_output_floor(model_id, requested)
}

#[cfg(test)]
fn gpt_oss_harmony_reasoning_cap() -> String {
    crate::inference::model_adapters::gpt_oss::harmony_reasoning_cap()
}

#[cfg(test)]
fn gpt_oss_harmony_max_output_tokens_cap() -> Option<usize> {
    crate::inference::model_adapters::gpt_oss::harmony_max_output_tokens_cap()
}

#[cfg(test)]
fn cap_gpt_oss_reasoning_effort(requested: &str, cap: Option<&str>) -> String {
    fn sanitize(value: &str) -> &str {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => "none",
            "minimal" | "low" => "low",
            "medium" => "medium",
            "high" => "high",
            _ => "medium",
        }
    }

    fn rank(value: &str) -> u8 {
        match sanitize(value) {
            "none" => 0,
            "low" => 1,
            "medium" => 2,
            "high" => 3,
            _ => 2,
        }
    }

    let requested = sanitize(requested);
    let Some(cap) = cap.map(sanitize) else {
        return requested.to_string();
    };
    if rank(requested) <= rank(cap) {
        requested.to_string()
    } else {
        cap.to_string()
    }
}

#[cfg(test)]
fn cap_gpt_oss_max_output_tokens(requested: usize, cap: Option<usize>) -> usize {
    let Some(cap) = cap.filter(|value| *value > 0) else {
        return requested;
    };
    requested.min(cap)
}

#[cfg(test)]
fn default_gpt_oss_output_budget(model_id: &str) -> usize {
    crate::inference::model_adapters::gpt_oss::default_output_budget(model_id)
}

#[cfg(test)]
fn build_gpt_oss_harmony_prompt(
    system_prompt: &str,
    conversation_items: &[Value],
    reasoning_effort: &str,
    tools: &[HarmonyToolSpec],
) -> String {
    crate::inference::model_adapters::gpt_oss::build_prompt(
        system_prompt,
        conversation_items,
        reasoning_effort,
        tools,
    )
}

#[cfg(test)]
fn normalize_function_call_arguments(name: &str, arguments: &str) -> String {
    crate::inference::model_adapters::gpt_oss::normalize_function_call_arguments(name, arguments)
}

#[cfg(test)]
#[path = "engine_boundary_tests.rs"]
mod boundary_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn model_supports_vision_recognises_local_vision_families() {
        // Qwen 3.5 chat family is marked vision-capable in the registry.
        assert!(model_supports_vision("Qwen/Qwen3.5-27B"));
        // Gemma 4 variant likewise.
        assert!(model_supports_vision("google/gemma-4-31B-it"));
        // Qwen3-VL-2B auxiliary itself is vision-capable.
        assert!(model_supports_vision("Qwen/Qwen3-VL-2B-Instruct"));
    }

    #[test]
    fn model_supports_vision_rejects_text_only_primaries() {
        assert!(!model_supports_vision("openai/gpt-oss-20b"));
        assert!(!model_supports_vision("nvidia/Nemotron-Cascade-2-30B-A3B"));
        assert!(!model_supports_vision("zai-org/GLM-4.7-Flash"));
        assert!(!model_supports_vision("moonshotai/kimi-k2.5"));
        assert!(!model_supports_vision(""));
    }

    #[test]
    fn model_supports_vision_recognises_known_api_models() {
        assert!(model_supports_vision("anthropic/claude-sonnet-4.6"));
        assert!(model_supports_vision("gpt-5.4"));
        assert!(model_supports_vision("gpt-5.4-mini"));
        assert!(model_supports_vision("MiniMax-M2.7"));
        // Nano is excluded intentionally.
        assert!(!model_supports_vision("gpt-5.4-nano"));
    }

    #[test]
    fn extract_message_content_blocks_handles_plain_string() {
        let content = json!("hello world");
        let blocks = extract_message_content_blocks(Some(&content));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[0]["text"], "hello world");
    }

    #[test]
    fn extract_message_content_blocks_keeps_input_image_as_image_url() {
        let content = json!([
            {"type": "input_text", "text": "look"},
            {"type": "input_image", "image_url": "https://example.com/x.png"},
        ]);
        let blocks = extract_message_content_blocks(Some(&content));
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "image_url");
        assert_eq!(blocks[1]["image_url"]["url"], "https://example.com/x.png");
    }

    #[test]
    fn extract_message_content_blocks_converts_image_data_to_data_uri() {
        let content = json!([
            {"type": "input_image", "image_data": "AAAA", "mime_type": "image/jpeg"},
        ]);
        let blocks = extract_message_content_blocks(Some(&content));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "image_url");
        assert_eq!(
            blocks[0]["image_url"]["url"],
            "data:image/jpeg;base64,AAAA"
        );
    }

    #[test]
    fn extract_message_content_blocks_passthrough_openai_image_url() {
        let content = json!([
            {"type": "image_url", "image_url": {"url": "https://example.com/a.webp", "detail": "high"}},
        ]);
        let blocks = extract_message_content_blocks(Some(&content));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "image_url");
        assert_eq!(blocks[0]["image_url"]["url"], "https://example.com/a.webp");
        assert_eq!(blocks[0]["image_url"]["detail"], "high");
    }

    #[test]
    fn message_blocks_contain_image_detects_both_variants() {
        let with_input_image = vec![json!({"type": "input_image", "image_url": "x"})];
        assert!(message_blocks_contain_image(&with_input_image));
        let with_image_url = vec![json!({"type": "image_url", "image_url": {"url": "x"}})];
        assert!(message_blocks_contain_image(&with_image_url));
        let only_text = vec![json!({"type": "text", "text": "x"})];
        assert!(!message_blocks_contain_image(&only_text));
    }

    #[test]
    fn vision_aux_selection_maps_to_qwen3_vl_instruct() {
        let selection =
            auxiliary_model_selection(AuxiliaryRole::Vision, None);
        assert_eq!(selection.role, AuxiliaryRole::Vision);
        assert_eq!(selection.default_port, 1240);
        assert_eq!(selection.compute_target, ComputeTarget::Gpu);
        // gpu_reserve_mb returns the family-specific reservation.
        assert_eq!(selection.gpu_reserve_mb(), 3500);
    }

    #[test]
    fn gpt_oss_runtime_uses_engine_gpt_oss_startup() {
        let deps = discover_source_layout_paths(Path::new("/tmp/ctox"));
        let runtime = default_runtime_config(LocalModelFamily::GptOss);
        let command = build_engine_command(&deps, &runtime);
        assert_eq!(command[1], "serve");
        assert!(command.iter().any(|part| part == "gpt_oss"));
        assert!(command.iter().any(|part| part == "--max-seq-len"));
    }

    #[test]
    fn qwen_runtime_uses_engine_vision_startup() {
        let deps = discover_source_layout_paths(Path::new("/tmp/ctox"));
        let runtime = default_runtime_config(LocalModelFamily::Qwen35Vision);
        let command = build_engine_command(&deps, &runtime);
        assert_eq!(command[1], "serve");
        assert_eq!(command[2], "-p");
        assert_eq!(command[4], "vision");
        assert!(command.iter().any(|part| part == "Qwen/Qwen3.5-27B"));
    }

    #[test]
    fn gemma_runtime_uses_engine_vision_startup() {
        let deps = discover_source_layout_paths(Path::new("/tmp/ctox"));
        let runtime = default_runtime_config(LocalModelFamily::Gemma4Vision);
        let command = build_engine_command(&deps, &runtime);
        assert_eq!(command[1], "serve");
        assert_eq!(command[2], "-p");
        assert_eq!(command[4], "vision");
        assert!(command.iter().any(|part| part == "google/gemma-4-31B-it"));
    }

    #[test]
    fn auxiliary_profiles_use_dedicated_engine_subcommands() {
        let deps = discover_source_layout_paths(Path::new("/tmp/ctox"));

        let embedding = build_engine_command(
            &deps,
            &default_runtime_config(LocalModelFamily::Qwen3Embedding),
        );
        assert_eq!(embedding[4], "embedding");
        assert!(embedding
            .iter()
            .any(|part| part == "Qwen/Qwen3-Embedding-0.6B"));

        let stt = build_engine_command(
            &deps,
            &default_runtime_config(LocalModelFamily::VoxtralTranscription),
        );
        assert_eq!(stt[4], "vision");
        assert!(stt
            .iter()
            .any(|part| part == "engineai/Voxtral-Mini-4B-Realtime-2602"));

        let tts = build_engine_command(
            &deps,
            &default_runtime_config(LocalModelFamily::VoxtralSpeech),
        );
        assert!(tts[0].ends_with("ctox-engine"));
        assert_eq!(tts[1], "serve");
        assert_eq!(tts[2], "-p");
        assert!(tts.iter().any(|part| part == "speech"));
        assert!(tts.windows(2).any(|pair| pair == ["--isq", "Q4K"]));
        assert!(tts
            .iter()
            .any(|part| part == "engineai/Voxtral-4B-TTS-2603"));
    }

    #[test]
    fn auxiliary_model_choices_resolve_cpu_and_gpu_variants() {
        let embedding_cpu = auxiliary_model_selection(
            AuxiliaryRole::Embedding,
            Some("Qwen/Qwen3-Embedding-0.6B [CPU]"),
        );
        assert_eq!(embedding_cpu.request_model, "Qwen/Qwen3-Embedding-0.6B");
        assert_eq!(embedding_cpu.compute_target, ComputeTarget::Cpu);
        assert_eq!(embedding_cpu.backend_kind, AuxiliaryBackendKind::MistralRs);
        assert_eq!(embedding_cpu.gpu_reserve_mb(), 0);

        let stt_gpu = auxiliary_model_selection(
            AuxiliaryRole::Stt,
            Some("engineai/Voxtral-Mini-4B-Realtime-2602"),
        );
        assert_eq!(
            stt_gpu.choice,
            "engineai/Voxtral-Mini-4B-Realtime-2602 [GPU]"
        );
        assert_eq!(stt_gpu.compute_target, ComputeTarget::Gpu);
        assert_eq!(stt_gpu.backend_kind, AuxiliaryBackendKind::MistralRs);
        assert_eq!(stt_gpu.gpu_reserve_mb(), 4200);

        let tts_cpu = auxiliary_model_selection(
            AuxiliaryRole::Tts,
            Some("speaches-ai/piper-fr_FR-siwis-medium [CPU FR]"),
        );
        assert_eq!(
            tts_cpu.request_model,
            "speaches-ai/piper-fr_FR-siwis-medium"
        );
        assert_eq!(tts_cpu.compute_target, ComputeTarget::Cpu);
        assert_eq!(tts_cpu.backend_kind, AuxiliaryBackendKind::MistralRs);
        assert_eq!(tts_cpu.default_port, 1239);

        let tts_de = auxiliary_model_selection(
            AuxiliaryRole::Tts,
            Some("speaches-ai/piper-de_DE-thorsten-high [CPU DE]"),
        );
        assert_eq!(
            tts_de.request_model,
            "speaches-ai/piper-de_DE-thorsten-high"
        );
        assert_eq!(tts_de.compute_target, ComputeTarget::Cpu);
        assert_eq!(tts_de.backend_kind, AuxiliaryBackendKind::MistralRs);

        let tts_qwen = auxiliary_model_selection(
            AuxiliaryRole::Tts,
            Some("Qwen/Qwen3-TTS-12Hz-0.6B-Base [GPU]"),
        );
        assert_eq!(tts_qwen.choice, "Qwen/Qwen3-TTS-12Hz-0.6B-Base [GPU]");
        assert_eq!(tts_qwen.request_model, "Qwen/Qwen3-TTS-12Hz-0.6B-Base");
        assert_eq!(tts_qwen.compute_target, ComputeTarget::Gpu);
        assert_eq!(tts_qwen.backend_kind, AuxiliaryBackendKind::MistralRs);

        let tts_default = auxiliary_model_selection(AuxiliaryRole::Tts, None);
        assert_eq!(tts_default.choice, "engineai/Voxtral-4B-TTS-2603 [GPU]");
        assert_eq!(tts_default.request_model, "engineai/Voxtral-4B-TTS-2603");
        assert_eq!(tts_default.compute_target, ComputeTarget::Gpu);
    }

    #[test]
    fn cpu_aux_models_have_native_runtime_profiles() {
        let stt = runtime_profile_for_model("Systran/faster-whisper-small").unwrap();
        assert_eq!(stt.family, LocalModelFamily::WhisperTranscriptionCpu);
        assert_eq!(stt.launcher_mode, "vision");
        assert!(stt.isq.is_none());

        let tts = runtime_profile_for_model("speaches-ai/piper-en_US-lessac-medium").unwrap();
        assert_eq!(tts.family, LocalModelFamily::PiperSpeech);
        assert_eq!(tts.launcher_mode, "speech");
        assert!(tts.isq.is_none());
    }

    #[test]
    fn source_layout_paths_use_tools_roots() {
        let deps = discover_source_layout_paths(Path::new("/tmp/ctox"));
        assert_eq!(deps.tools_root, PathBuf::from("/tmp/ctox/tools"));
        assert_eq!(
            deps.codex_exec_binary,
            PathBuf::from("/tmp/ctox/tools/agent-runtime/target/release/codex-exec")
        );
        assert_eq!(
            deps.model_runtime_binary,
            PathBuf::from("/tmp/ctox/tools/model-runtime/target/release/ctox-engine")
        );
    }

    #[test]
    fn family_profiles_drive_nccl_policy() {
        let gpt_oss = runtime_profile_for_model("openai/gpt-oss-20b").unwrap();
        let qwen = runtime_profile_for_model("Qwen/Qwen3.5-27B").unwrap();
        let glm = runtime_profile_for_model("zai-org/GLM-4.7-Flash").unwrap();
        let embedding = runtime_profile_for_model("Qwen/Qwen3-Embedding-0.6B").unwrap();
        let stt = runtime_profile_for_model("engineai/Voxtral-Mini-4B-Realtime-2602").unwrap();
        let tts = runtime_profile_for_model("Qwen/Qwen3-TTS-12Hz-0.6B-Base").unwrap();
        let voxtral_tts = runtime_profile_for_model("engineai/Voxtral-4B-TTS-2603").unwrap();
        assert!(!gpt_oss.disable_nccl);
        assert_eq!(gpt_oss.tensor_parallel_backend.as_deref(), Some("nccl"));
        assert_eq!(gpt_oss.target_world_size, Some(2));
        assert_eq!(gpt_oss.preferred_gpu_count, Some(2));
        assert!(!qwen.disable_nccl);
        assert_eq!(qwen.tensor_parallel_backend, None);
        assert_eq!(qwen.target_world_size, None);
        assert_eq!(qwen.preferred_gpu_count, Some(3));
        assert!(glm.disable_nccl);
        assert_eq!(glm.tensor_parallel_backend, None);
        assert_eq!(glm.preferred_gpu_count, Some(3));
        assert!(embedding.disable_nccl);
        assert_eq!(embedding.preferred_gpu_count, Some(1));
        assert_eq!(embedding.isq, None);
        assert!(stt.disable_nccl);
        assert_eq!(stt.preferred_gpu_count, Some(1));
        assert!(tts.disable_nccl);
        assert_eq!(tts.preferred_gpu_count, Some(1));
        assert_eq!(tts.isq.as_deref(), Some("Q4K"));
        assert!(voxtral_tts.disable_nccl);
        assert_eq!(voxtral_tts.preferred_gpu_count, Some(1));
        assert_eq!(voxtral_tts.isq.as_deref(), Some("Q4K"));
    }

    #[test]
    fn clean_room_baseline_plan_exposes_family_profile() {
        let plan = build_clean_room_baseline_plan(
            Path::new("/tmp/ctox"),
            LocalModelFamily::Qwen35Vision,
            "ignored".to_string(),
        );
        assert_eq!(plan.family_profile.family, LocalModelFamily::Qwen35Vision);
        assert_eq!(plan.family_profile.launcher_mode, "vision");
        assert_eq!(plan.family_profile.tensor_parallel_backend, None);
    }

    #[test]
    fn model_specific_runtime_profiles_match_size_class() {
        let qwen_small = runtime_profile_for_model("Qwen/Qwen3.5-4B").unwrap();
        assert!(qwen_small.disable_nccl);
        assert_eq!(qwen_small.preferred_gpu_count, Some(1));
        assert_eq!(qwen_small.max_seq_len, 262_144);

        let qwen_large = runtime_profile_for_model("Qwen/Qwen3.5-35B-A3B").unwrap();
        assert!(qwen_large.disable_nccl);
        assert_eq!(qwen_large.target_world_size, None);
        assert_eq!(qwen_large.preferred_gpu_count, Some(3));
        assert_eq!(qwen_large.paged_attn, "auto");
        assert_eq!(qwen_large.pa_cache_type.as_deref(), Some("turboquant3"));

        let gemma_dense = runtime_profile_for_model("gemma-4-31b-it").unwrap();
        assert!(gemma_dense.disable_nccl);
        assert_eq!(gemma_dense.preferred_gpu_count, Some(3));
        assert_eq!(gemma_dense.max_seq_len, 131_072);
        assert_eq!(gemma_dense.pa_cache_type.as_deref(), Some("f8e4m3"));

        let nemotron = runtime_profile_for_model("nemotron-cascade-2-30b-a3b").unwrap();
        assert!(nemotron.disable_nccl);
        assert_eq!(nemotron.arch, None);
        assert_eq!(nemotron.max_seq_len, 8_192);

        let glm = runtime_profile_for_model("GLN 4.7 flash").unwrap();
        assert_eq!(glm.arch.as_deref(), Some("glm4moelite"));
        assert_eq!(glm.pa_cache_type.as_deref(), Some("turboquant3"));
    }

    #[test]
    fn pa_cache_type_is_manifest_owned() {
        let env_map = BTreeMap::new();
        assert_eq!(
            resolve_pa_cache_type(Some("f8e4m3"), &env_map),
            Some("f8e4m3".to_string())
        );
        assert_eq!(resolve_pa_cache_type(None, &env_map), None);
    }

    #[test]
    fn supported_model_retains_manifest_cache_type_even_when_env_requests_override() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_ENGINE_PA_CACHE_TYPE_OVERRIDE".to_string(),
            TURBOQUANT3_CACHE_TYPE.to_string(),
        );
        assert_eq!(
            resolve_model_pa_cache_type("openai/gpt-oss-20b", Some("f8e4m3"), &env_map),
            Some("f8e4m3".to_string())
        );
    }

    #[test]
    fn supported_model_keeps_manifest_turboquant_default() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_ENGINE_PA_CACHE_TYPE_OVERRIDE".to_string(),
            "f8e4m3".to_string(),
        );
        assert_eq!(
            resolve_model_pa_cache_type("Qwen/Qwen3.5-27B", Some("turboquant3"), &env_map),
            Some(TURBOQUANT3_CACHE_TYPE.to_string())
        );
    }

    #[test]
    fn gemma4_keeps_manifest_cache_type_even_when_env_requests_change() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_ENGINE_PA_CACHE_TYPE_OVERRIDE".to_string(),
            "turboquant3".to_string(),
        );
        // Gemma-4 uses f8e4m3 (turboquant3 is incompatible with head_dim=512).
        // Even when env overrides to turboquant3, the manifest default f8e4m3 is kept
        // because turboquant3 is not supported for Gemma4Vision family.
        assert_eq!(
            resolve_model_pa_cache_type("google/gemma-4-26B-A4B-it", Some("f8e4m3"), &env_map),
            Some("f8e4m3".to_string())
        );
    }

    #[test]
    fn cache_override_is_ignored_for_non_paged_attention_family() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_ENGINE_PA_CACHE_TYPE_OVERRIDE".to_string(),
            "f8e4m3".to_string(),
        );
        assert_eq!(
            resolve_model_pa_cache_type("Qwen/Qwen3-TTS-12Hz-0.6B-Base", None, &env_map,),
            None
        );
    }

    #[test]
    fn supported_cache_override_promotes_paged_attention_from_off() {
        assert_eq!(
            resolve_model_paged_attn("Qwen/Qwen3.5-35B-A3B", "off", Some("turboquant3")),
            "auto"
        );
    }

    #[test]
    fn harmony_prompt_inlines_function_tool_namespace() {
        let prompt = build_gpt_oss_harmony_prompt(
            "Be precise.",
            &[json!({
                "type":"message",
                "role":"user",
                "content":[{"text":"Reply with OK"}]
            })],
            "medium",
            &[HarmonyToolSpec {
                name: "exec_command".to_string(),
                description: Some("Runs a shell command".to_string()),
                parameters: Some(json!({"type":"object"})),
            }],
        );
        assert!(prompt.contains("Available tools: exec_command"));
        assert!(prompt.contains(
            "Use only the provided function tool definitions from the request metadata."
        ));
        assert!(prompt.contains("namespace functions"));
        assert!(prompt.contains("type exec_command = (_: {"));
    }

    #[test]
    fn recognizes_openai_api_chat_models() {
        assert!(is_openai_api_chat_model("gpt-5.4"));
        assert!(is_openai_api_chat_model("GPT-5.4-MINI"));
        assert!(!is_openai_api_chat_model("openai/gpt-oss-20b"));
    }

    #[test]
    fn recognizes_anthropic_api_chat_models() {
        assert!(is_anthropic_api_chat_model("anthropic/claude-sonnet-4.6"));
        assert_eq!(
            default_api_provider_for_model("anthropic/claude-sonnet-4.6"),
            "anthropic"
        );
        assert!(api_provider_supports_model(
            "anthropic",
            "anthropic/claude-sonnet-4.6"
        ));
    }

    #[test]
    fn only_local_runtime_models_use_ctox_proxy_path() {
        assert!(!uses_ctox_proxy_model("gpt-5.4"));
        assert!(!uses_ctox_proxy_model("gpt-5.4-nano"));
        assert!(uses_ctox_proxy_model("Qwen/Qwen3.5-4B"));
        assert!(!uses_ctox_proxy_model("not-a-real-model"));
    }

    #[test]
    fn responses_rewrite_wraps_and_filters_tools() {
        let payload = serde_json::json!({
            "tools": [
                {"type": "function", "name": "exec_command", "parameters": {"type": "object"}},
                {"type": "function", "name": "spawn_agent", "parameters": {"type": "object"}},
                {"type": "web_search"},
            ],
            "parallel_tool_calls": false,
            "max_tool_calls": 1,
        });
        let rewritten =
            rewrite_engine_responses_request(serde_json::to_vec(&payload).unwrap().as_slice())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(
            value["tools"],
            serde_json::json!([
                {"type":"function","function":{"name":"exec_command","parameters":{"type":"object"}}}
            ])
        );
        assert_eq!(value["parallel_tool_calls"], Value::Bool(true));
        assert!(value.get("max_tool_calls").is_none());
    }

    #[test]
    fn responses_rewrite_preserves_structured_input_and_instructions() {
        let payload = serde_json::json!({
            "instructions": "System rule",
            "input": [
                {"role": "developer", "content": [{"text": "Dev text"}]},
                {"role": "user", "content": [{"text": "User text"}]}
            ]
        });
        let rewritten =
            rewrite_engine_responses_request(serde_json::to_vec(&payload).unwrap().as_slice())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(
            value["instructions"],
            Value::String("System rule".to_string())
        );
        assert_eq!(value["input"], payload["input"]);
    }

    #[test]
    fn openai_responses_rewrite_flattens_namespace_tools() {
        let payload = serde_json::json!({
            "tools": [
                {"type":"web_search","search_context_size":"medium"},
                {
                    "type": "namespace",
                    "tools": [
                        {"type":"function","name":"exec_command","description":"run","parameters":{"type":"object"}},
                        {"type":"function","name":"spawn_agent","parameters":{"type":"object"}}
                    ]
                }
            ]
        });
        let rewritten =
            rewrite_openai_responses_request(serde_json::to_vec(&payload).unwrap().as_slice())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(
            value["tools"],
            serde_json::json!([
                {"type":"web_search","search_context_size":"medium"},
                {"type":"function","name":"exec_command","description":"run","parameters":{"type":"object"}}
            ])
        );
    }

    #[test]
    fn detects_gpt_oss_harmony_proxy_need() {
        let payload = serde_json::json!({"model":"openai/gpt-oss-20b"});
        assert!(should_use_gpt_oss_harmony_proxy(&serde_json::to_vec(&payload).unwrap()).unwrap());
    }

    #[test]
    fn detects_streaming_request_flag() {
        let payload = serde_json::json!({"model":"openai/gpt-oss-20b","stream":true});
        assert!(responses_request_streams(&serde_json::to_vec(&payload).unwrap()).unwrap());
    }

    #[test]
    fn translates_responses_request_to_gpt_oss_chat_completions() {
        let payload = serde_json::json!({
            "model": "openai/gpt-oss-20b",
            "instructions": "System rules",
            "input": [
                {"role":"user","content":[{"text":"Do the thing"}]}
            ],
            "max_output_tokens": 333,
            "reasoning": {"effort":"high"}
        });
        let rewritten =
            rewrite_responses_to_gpt_oss_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["model"], "openai/gpt-oss-20b");
        assert_eq!(value["max_tokens"], 333);
        assert_eq!(value["enable_thinking"], true);
        assert_eq!(value["reasoning_effort"], "low");
        assert_eq!(value["messages"][0]["role"], "system");
        assert_eq!(value["messages"][0]["content"], "System rules");
        assert_eq!(value["messages"][1]["role"], "user");
        assert_eq!(value["messages"][1]["content"], "Do the thing");
    }

    #[test]
    fn exact_text_requests_keep_gpt_oss_thinking_enabled() {
        let payload = serde_json::json!({
            "model": "openai/gpt-oss-20b",
            "input": [
                {"role":"user","content":[{"text":"Reply with exactly OK and nothing else."}]}
            ]
        });
        let rewritten =
            rewrite_responses_to_gpt_oss_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["max_tokens"], 131_072);
        assert_eq!(value["enable_thinking"], true);
        assert_eq!(value["reasoning_effort"], "low");
    }

    #[test]
    fn reasoning_none_disables_gpt_oss_thinking() {
        let payload = serde_json::json!({
            "model": "openai/gpt-oss-20b",
            "input": [
                {"role":"user","content":[{"text":"Reply with LOOP_OK and nothing else."}]}
            ],
            "reasoning": {"effort":"none"},
            "max_output_tokens": 16
        });
        let rewritten =
            rewrite_responses_to_gpt_oss_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["enable_thinking"], false);
        assert!(value.get("reasoning_effort").is_none());
        assert_eq!(value["max_tokens"], 16);
    }

    #[test]
    fn minimal_reasoning_keeps_gpt_oss_thinking_enabled() {
        let payload = serde_json::json!({
            "model": "openai/gpt-oss-20b",
            "input": [
                {"role":"user","content":[{"text":"Reply with LOOP_OK and nothing else."}]}
            ],
            "reasoning": {"effort":"minimal"}
        });
        let rewritten =
            rewrite_responses_to_gpt_oss_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["enable_thinking"], true);
        assert_eq!(value["reasoning_effort"], "low");
    }

    #[test]
    fn short_output_budget_keeps_gpt_oss_thinking_enabled() {
        let payload = serde_json::json!({
            "model": "openai/gpt-oss-20b",
            "input": [
                {"role":"user","content":[{"text":"Reply with LOOP_OK."}]}
            ],
            "reasoning": {"effort":"low"},
            "max_output_tokens": 16
        });
        let rewritten =
            rewrite_responses_to_gpt_oss_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["enable_thinking"], true);
        assert_eq!(value["reasoning_effort"], "low");
        assert_eq!(value["max_tokens"], 128);
    }

    #[test]
    fn caps_gpt_oss_reasoning_effort_when_requested_value_is_higher() {
        assert_eq!(
            cap_gpt_oss_reasoning_effort("medium", Some("low")),
            "low".to_string()
        );
        assert_eq!(
            cap_gpt_oss_reasoning_effort("none", Some("low")),
            "none".to_string()
        );
        assert_eq!(
            cap_gpt_oss_reasoning_effort("high", Some("medium")),
            "medium".to_string()
        );
        assert_eq!(
            cap_gpt_oss_reasoning_effort("low", Some("high")),
            "low".to_string()
        );
    }

    #[test]
    fn caps_gpt_oss_max_output_tokens_when_cap_is_present() {
        assert_eq!(cap_gpt_oss_max_output_tokens(1024, Some(384)), 384);
        assert_eq!(cap_gpt_oss_max_output_tokens(128, Some(384)), 128);
        assert_eq!(cap_gpt_oss_max_output_tokens(256, None), 256);
    }

    #[test]
    fn gpt_oss_thinking_output_floor_avoids_tiny_budgets() {
        assert_eq!(
            ensure_gpt_oss_thinking_output_floor("openai/gpt-oss-20b", 16),
            128
        );
        assert_eq!(
            ensure_gpt_oss_thinking_output_floor("openai/gpt-oss-20b", 128),
            128
        );
        assert_eq!(
            ensure_gpt_oss_thinking_output_floor("openai/gpt-oss-20b", 333),
            333
        );
        assert_eq!(
            ensure_gpt_oss_thinking_output_floor("Qwen/Qwen3.5-4B", 16),
            16
        );
    }

    #[test]
    fn gpt_oss_defaults_to_the_128k_runtime_output_budget_when_unconfigured() {
        assert_eq!(gpt_oss_harmony_reasoning_cap(), "low");
        assert_eq!(gpt_oss_harmony_max_output_tokens_cap(), None);
        assert_eq!(default_gpt_oss_output_budget("openai/gpt-oss-20b"), 131_072);
    }

    #[test]
    fn translates_responses_request_to_glm_chat_without_thinking_by_default() {
        let payload = serde_json::json!({
            "model": "zai-org/GLM-4.7-Flash",
            "instructions": "System rules",
            "input": [
                {"role":"user","content":[{"text":"Reply with CTOX_OK and nothing else."}]}
            ],
            "max_output_tokens": 64
        });
        let rewritten =
            rewrite_responses_to_glm_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["model"], "zai-org/GLM-4.7-Flash");
        assert_eq!(value["max_tokens"], 64);
        assert_eq!(value["enable_thinking"], false);
        assert!(value.get("reasoning_effort").is_none());
    }

    #[test]
    fn translates_responses_request_to_qwen_chat_without_thinking_by_default() {
        let payload = serde_json::json!({
            "model": "Qwen/Qwen3.5-4B",
            "instructions": "System rules",
            "input": [
                {"role":"user","content":[{"text":"Reply with CTOX_OK and nothing else."}]}
            ],
            "max_output_tokens": 64
        });
        let rewritten =
            rewrite_responses_to_qwen_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["model"], "Qwen/Qwen3.5-4B");
        assert_eq!(value["max_tokens"], 64);
        assert_eq!(value["enable_thinking"], false);
    }

    #[test]
    fn translates_responses_request_to_nemotron_chat_without_thinking_by_default() {
        let payload = serde_json::json!({
            "model": "nvidia/Nemotron-Cascade-2-30B-A3B",
            "instructions": "System rules",
            "input": [
                {"role":"user","content":[{"text":"Reply with CTOX_OK and nothing else."}]}
            ],
            "max_output_tokens": 64
        });
        let rewritten =
            rewrite_responses_to_nemotron_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["model"], "nvidia/Nemotron-Cascade-2-30B-A3B");
        assert_eq!(value["max_tokens"], 64);
        assert_eq!(value["enable_thinking"], false);
    }

    #[test]
    fn rewrites_nemotron_think_tags_back_to_responses_reasoning() {
        let payload = serde_json::json!({
            "id":"abc",
            "created":42,
            "model":"nvidia/Nemotron-Cascade-2-30B-A3B",
            "choices":[{"message":{"content":"<think>step by step</think>\nCTOX_OK"}}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_nemotron_chat_completions_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["status"], "completed");
        assert_eq!(value["output_text"], "CTOX_OK");
        assert_eq!(value["reasoning"], "step by step");
        assert_eq!(value["output"][0]["content"][0]["text"], "CTOX_OK");
    }

    #[test]
    fn translates_responses_request_to_qwen_chat_with_explicit_reasoning() {
        let payload = serde_json::json!({
            "model": "Qwen/Qwen3.5-4B",
            "input": [
                {"role":"user","content":[{"text":"Solve this carefully."}]}
            ],
            "reasoning": {"effort":"high"}
        });
        let rewritten =
            rewrite_responses_to_qwen_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["enable_thinking"], true);
    }

    #[test]
    fn translates_responses_request_to_glm_chat_with_explicit_reasoning() {
        let payload = serde_json::json!({
            "model": "zai-org/GLM-4.7-Flash",
            "input": [
                {"role":"user","content":[{"text":"Solve this carefully."}]}
            ],
            "reasoning": {"effort":"high"}
        });
        let rewritten =
            rewrite_responses_to_glm_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["enable_thinking"], true);
        assert_eq!(value["reasoning_effort"], "high");
    }

    #[test]
    fn translates_harmony_completion_back_to_responses() {
        let payload = serde_json::json!({
            "id":"123",
            "created":42,
            "model":"openai/gpt-oss-20b",
            "choices":[{"text":"<|start|>assistant<|channel|>final<|message|>CTOX_OK<|end|>"}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_gpt_oss_completion_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["status"], "completed");
        assert_eq!(value["output_text"], "CTOX_OK");
        assert_eq!(value["output"][0]["content"][0]["text"], "CTOX_OK");
        assert_eq!(value["usage"]["input_tokens"], 11);
        assert_eq!(value["usage"]["output_tokens"], 7);
    }

    #[test]
    fn translates_gpt_oss_chat_completions_back_to_responses() {
        let payload = serde_json::json!({
            "id":"chatcmpl-123",
            "created":42,
            "model":"openai/gpt-oss-20b",
            "choices":[{
                "message":{
                    "role":"assistant",
                    "content":"CTOX_OK"
                },
                "finish_reason":"stop"
            }],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_gpt_oss_chat_completions_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["status"], "completed");
        assert_eq!(value["output_text"], "CTOX_OK");
        assert_eq!(value["output"][0]["content"][0]["text"], "CTOX_OK");
    }

    #[test]
    fn exact_text_override_normalizes_gpt_oss_chat_completion_output() {
        let payload = serde_json::json!({
            "id":"chatcmpl-123",
            "created":42,
            "model":"openai/gpt-oss-20b",
            "choices":[{
                "message":{
                    "role":"assistant",
                    "content":"Reply A. ok. 2024"
                },
                "finish_reason":"stop"
            }],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_gpt_oss_chat_completions_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
            Some("OK"),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["output_text"], "OK");
        assert_eq!(value["output"][0]["content"][0]["text"], "OK");
    }

    #[test]
    fn harmony_prompt_without_tools_has_explicit_final_contract() {
        let prompt = build_gpt_oss_harmony_prompt(
            "",
            &[json!({
                "type":"message",
                "role":"user",
                "content":[{"text":"Reply with exactly OK."}]
            })],
            "medium",
            &[],
        );
        assert!(
            prompt.contains("Answer on the final channel unless you are emitting one tool call.")
        );
        assert!(prompt.contains("return exactly that text and nothing else"));
        assert!(prompt.ends_with("<|start|>assistant"));
        assert!(!prompt.contains("<|start|>assistant<|channel|>final<|message|>"));
    }

    #[test]
    fn translates_responses_request_to_gpt_oss_completion() {
        let payload = serde_json::json!({
            "model": "openai/gpt-oss-20b",
            "instructions": "System rules",
            "input": [
                {"role":"user","content":[{"text":"Reply with exactly LOOP_OK and nothing else."}]}
            ],
            "reasoning": {"effort":"high"},
            "max_output_tokens": 16
        });
        let rewritten =
            rewrite_responses_to_gpt_oss_completion(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["model"], "openai/gpt-oss-20b");
        assert_eq!(value["temperature"], 0.0);
        assert_eq!(value["max_tokens"], 128);
        let prompt = value["prompt"].as_str().unwrap();
        assert!(prompt.contains("Reasoning: low"));
        assert!(prompt.contains("return exactly that text and nothing else"));
        assert!(prompt.ends_with("<|start|>assistant"));
    }

    #[test]
    fn translates_responses_request_to_gpt_oss_completion_without_thinking() {
        let payload = serde_json::json!({
            "model": "openai/gpt-oss-20b",
            "instructions": "System rules",
            "input": [
                {"role":"user","content":[{"text":"Reply with exactly LOOP_OK and nothing else."}]}
            ],
            "reasoning": {"effort":"none"},
            "max_output_tokens": 16
        });
        let rewritten =
            rewrite_responses_to_gpt_oss_completion(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["model"], "openai/gpt-oss-20b");
        assert_eq!(value["max_tokens"], 16);
        let prompt = value["prompt"].as_str().unwrap();
        assert!(prompt.contains("Reasoning: none"));
        assert!(prompt.contains("return exactly that text and nothing else"));
    }

    #[test]
    fn gpt_oss_completion_rewrite_appends_harmony_stop_markers() {
        let payload = serde_json::json!({
            "model": "openai/gpt-oss-20b",
            "input": [
                {"role":"user","content":[{"text":"Reply with exactly LOOP_OK and nothing else."}]}
            ],
            "stop": "DONE"
        });
        let rewritten =
            rewrite_responses_to_gpt_oss_completion(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();

        assert_eq!(value["stop"], json!(["DONE", "<|return|>", "<|call|>"]));
    }

    #[test]
    fn extracts_exact_text_override_from_latest_user_message() {
        let payload = json!({
            "input": [
                {
                    "type":"message",
                    "role":"user",
                    "content":[{"text":"Reply with exactly WRONG."}]
                },
                {
                    "type":"message",
                    "role":"assistant",
                    "content":[{"text":"WRONG"}]
                },
                {
                    "type":"message",
                    "role":"user",
                    "content":[{"text":"Reply with exactly OK."}]
                }
            ]
        });
        assert_eq!(
            extract_exact_text_override_from_materialized_request(&payload),
            Some("OK".to_string())
        );
    }

    #[test]
    fn extracts_exact_text_override_from_embedded_user_clause() {
        let payload = json!({
            "input": [
                {
                    "type":"message",
                    "role":"user",
                    "content":[{"text":"What is 17 plus 4? Reply with exactly 21 and nothing else."}]
                }
            ]
        });
        assert_eq!(
            extract_exact_text_override_from_materialized_request(&payload),
            Some("21".to_string())
        );
    }

    #[test]
    fn extracts_exact_text_override_from_german_embedded_user_clause() {
        let payload = json!({
            "input": [
                {
                    "type":"message",
                    "role":"user",
                    "content":[{"text":"Arbeite ausschließlich hier. Antworte genau mit CTOX_SOCKET_SMOKE_OK und nichts anderem."}]
                }
            ]
        });
        assert_eq!(
            extract_exact_text_override_from_materialized_request(&payload),
            Some("CTOX_SOCKET_SMOKE_OK".to_string())
        );
    }

    #[test]
    fn does_not_extract_exact_text_override_for_workspace_build_request() {
        let payload = json!({
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "exec_command",
                        "description": "Runs a shell command.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "cmd": { "type": "string" }
                            },
                            "required": ["cmd"],
                            "additionalProperties": false
                        }
                    }
                }
            ],
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [{
                        "text": "Work only inside this workspace:\n/home/metricspace/ctox-e2e/workspace/cpp-chat-app\n\nCreate a bounded C++ verification project in this workspace.\n\nRequirements:\n- Use CMake.\n- Create at least these files: CMakeLists.txt, include/MessageQueue.h, src/MessageQueue.cpp, src/main.cpp.\n- Build it with: cmake -S . -B build && cmake --build build -j\n- Verify the binary with: ./build/ctox_cpp_smoke\n- On successful run, the program must print exactly CTOX_CPP_SMOKE_OK_1775581675\n- Do not answer before the files exist and the binary was executed successfully.\n- Keep the final answer extremely short and return exactly CTOX_CPP_SMOKE_OK_1775581675"
                    }]
                }
            ]
        });
        assert_eq!(
            extract_exact_text_override_from_materialized_request(&payload),
            None
        );
    }

    #[test]
    fn workspace_build_request_with_exact_marker_keeps_gpt_oss_tools() {
        let payload = json!({
            "model": "openai/gpt-oss-20b",
            "reasoning": {"effort":"low"},
            "tool_choice": "auto",
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "exec_command",
                        "description": "Runs a shell command.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "cmd": { "type": "string" }
                            },
                            "required": ["cmd"],
                            "additionalProperties": false
                        }
                    }
                }
            ],
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [{
                        "text": "Work only inside this workspace:\n/home/metricspace/ctox-e2e/workspace/cpp-chat-app\n\nCreate a bounded C++ verification project in this workspace.\n\nRequirements:\n- Use CMake.\n- Build it with: cmake -S . -B build && cmake --build build -j\n- Verify the binary with: ./build/ctox_cpp_smoke\n- On successful run, the program must print exactly CTOX_CPP_SMOKE_OK_1775581675\n- Do not answer before the files exist and the binary was executed successfully.\n- Keep the final answer extremely short and return exactly CTOX_CPP_SMOKE_OK_1775581675"
                    }]
                }
            ]
        });
        let rewritten =
            rewrite_responses_to_gpt_oss_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["enable_thinking"], Value::Bool(true));
        assert_eq!(value["reasoning_effort"], Value::String("low".to_string()));
        assert_eq!(value["tool_choice"], Value::String("auto".to_string()));
        assert_eq!(value["tools"].as_array().map(|tools| tools.len()), Some(1));
    }

    #[test]
    fn exact_text_override_normalizes_gpt_oss_completion_output() {
        let payload = serde_json::json!({
            "id":"123",
            "created":42,
            "model":"openai/gpt-oss-20b",
            "choices":[{"text":">**\n**If** noise reply noise"}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_gpt_oss_completion_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
            Some("OK"),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["output_text"], "OK");
        assert_eq!(value["output"][0]["content"][0]["text"], "OK");
    }

    #[test]
    fn prefers_last_explicit_harmony_final_message() {
        let payload = serde_json::json!({
            "id":"123",
            "created":42,
            "model":"openai/gpt-oss-20b",
            "choices":[{"text":"analysis to=repo_browser.open_file code<|message|>{\"path\":\"README.md\"}<|end|><|start|>assistant<|channel|>final<|message|>CTOX_LAST_FINAL<|end|>"}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_gpt_oss_completion_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["output_text"], "CTOX_LAST_FINAL");
    }

    #[test]
    fn strips_plaintext_harmony_channel_leakage_from_gpt_oss() {
        let payload = serde_json::json!({
            "id":"123",
            "created":42,
            "model":"openai/gpt-oss-20b",
            "choices":[{"text":"GPTOSS_OKanalysisThe user says assistantfinalGPTOSS_OK"}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_gpt_oss_completion_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["status"], "completed");
        assert_eq!(value["output_text"], "GPTOSS_OK");
        assert_eq!(value["output"][0]["content"][0]["text"], "GPTOSS_OK");
    }

    #[test]
    fn extracts_plaintext_final_payload_after_analysis_leakage() {
        let payload = serde_json::json!({
            "id":"123",
            "created":42,
            "model":"openai/gpt-oss-20b",
            "choices":[{"text":"analysis We should inspect the repo carefully. assistantfinalCTOX_RECOVEREDassistantcommentaryto=functions.exec_command{\"cmd\":\"printf nope\"}"}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_gpt_oss_completion_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["output_text"], "CTOX_RECOVERED");
    }

    #[test]
    fn strips_real_world_plaintext_harmony_channel_leakage_from_gpt_oss() {
        let payload = serde_json::json!({
            "id":"123",
            "created":42,
            "model":"openai/gpt-oss-20b",
            "choices":[{"text":"GPTOSS_OKassistantanalysisThe user says: \"Reply with GPTOSS_OK and nothing else.\"assistantfinalGPTOSS_OKassistantcommentaryto=functions.exec_command{\"cmd\":\"printf GPTOSS_OK\"}"}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_gpt_oss_completion_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["status"], "completed");
        assert_eq!(value["output_text"], "GPTOSS_OK");
        assert_eq!(value["output"][0]["content"][0]["text"], "GPTOSS_OK");
    }

    #[test]
    fn strips_real_world_plaintext_harmony_channel_leakage_with_trailing_assistant() {
        let payload = serde_json::json!({
            "id":"123",
            "created":42,
            "model":"openai/gpt-oss-20b",
            "choices":[{"text":"GPTOSS_OKassistantanalysisThe user says: \"Reply with GPTOSS_OK and nothing else.\" So we should output exactly \"GPTOSS_OK\" with no other text.assistantfinalGPTOSS_OKassistantcommentaryWe have complied.assistantanalysisWe are done.assistantfinalGPTOSS_OKassistant"}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_gpt_oss_completion_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["status"], "completed");
        assert_eq!(value["output_text"], "GPTOSS_OK");
        assert_eq!(value["output"][0]["content"][0]["text"], "GPTOSS_OK");
    }

    #[test]
    fn translates_harmony_tool_call_back_to_responses() {
        let payload = serde_json::json!({
            "id":"456",
            "created":42,
            "model":"openai/gpt-oss-20b",
            "choices":[{"text":"<|channel|>commentary to=functions.shell_command<|constrain|>json<|message|>{\"command\":\"printf CTOX_TOOL\"}<|call|>"}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_gpt_oss_completion_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["status"], "completed");
        assert!(value["output_text"].is_null());
        assert_eq!(value["output"][0]["type"], "function_call");
        assert_eq!(value["output"][0]["name"], "shell_command");
    }

    #[test]
    fn normalizes_relaxed_exec_command_multiline_string_arguments() {
        let raw = "{\n\"cmd\": \"apply_patch <<'PATCH'\n*** Begin Patch\n*** Add File: foo.txt\n+hello\n*** End Patch\nPATCH\"\n}";
        let normalized = normalize_function_call_arguments("exec_command", raw);
        let value: Value = serde_json::from_str(&normalized).unwrap();
        assert_eq!(value["cmd"].as_str().unwrap(), "apply_patch <<'PATCH'\n*** Begin Patch\n*** Add File: foo.txt\n+hello\n*** End Patch\nPATCH");
    }

    #[test]
    fn normalizes_relaxed_exec_command_array_arguments() {
        let raw = r#"{"cmd":["bash","-lc","printf CTOX_OK"],"yield_time_ms":1000}"#;
        let normalized = normalize_function_call_arguments("exec_command", raw);
        let value: Value = serde_json::from_str(&normalized).unwrap();
        assert_eq!(value["cmd"], "printf CTOX_OK");
        assert_eq!(value["shell"], "bash");
        assert_eq!(value["login"], false);
        assert_eq!(value["yield_time_ms"], 1000);
    }

    #[test]
    fn normalizes_apply_patch_array_to_heredoc_command() {
        let raw = r#"{"cmd":["apply_patch","*** Begin Patch\n*** Add File: hello.txt\n+hi\n*** End Patch"]}"#;
        let normalized = normalize_function_call_arguments("exec_command", raw);
        let value: Value = serde_json::from_str(&normalized).unwrap();
        assert_eq!(
            value["cmd"].as_str().unwrap(),
            "apply_patch <<'PATCH'\n*** Begin Patch\n*** Add File: hello.txt\n+hi\n*** End Patch\nPATCH"
        );
    }

    #[test]
    fn translates_harmony_completion_to_sse() {
        let payload = serde_json::json!({
            "id":"789",
            "created":42,
            "model":"openai/gpt-oss-20b",
            "choices":[{"text":"<|channel|>final<|message|>CTOX_STREAM_OK<|end|>"}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten =
            rewrite_gpt_oss_completion_to_sse(&serde_json::to_vec(&payload).unwrap(), None, None)
                .unwrap();
        let text = String::from_utf8(rewritten).unwrap();
        assert!(text.contains("\"type\":\"response.created\""));
        assert!(text.contains("\"type\":\"response.output_item.done\""));
        assert!(text.contains("\"CTOX_STREAM_OK\""));
        assert!(text.contains("\"type\":\"response.completed\""));
    }

    #[test]
    fn emits_added_and_done_frames_for_web_search_calls() {
        let payload = serde_json::json!({
            "id": "resp_ws",
            "model": "openai/gpt-oss-20b",
            "output": [{
                "type": "web_search_call",
                "id": "ws_1",
                "status": "completed",
                "action": {
                    "type": "search",
                    "query": "weather berlin"
                }
            }]
        });
        let rewritten =
            rewrite_responses_payload_to_sse(&serde_json::to_vec(&payload).unwrap()).unwrap();
        let text = String::from_utf8(rewritten).unwrap();
        assert!(text.contains("\"type\":\"response.output_item.added\""));
        assert!(text.contains("\"type\":\"web_search_call\""));
        assert!(text.contains("\"id\":\"ws_1\""));
        assert!(text.contains("\"status\":\"in_progress\""));
        assert!(text.contains("\"type\":\"response.output_item.done\""));
        assert!(text.contains("\"query\":\"weather berlin\""));
    }

    #[test]
    fn gemma4_request_translation_preserves_named_tool_outputs() {
        let payload = serde_json::json!({
            "model": "google/gemma-4-26B-A4B-it",
            "instructions": "You are helpful.",
            "input": [
                {"type":"message","role":"user","content":[{"type":"input_text","text":"Use the weather tool."}]},
                {"type":"function_call","call_id":"call_weather","name":"weather.lookup","arguments":"{\"city\":\"Berlin\"}"},
                {"type":"function_call_output","call_id":"call_weather","output":[{"type":"output_text","text":"{\"temp_c\":18}"}]}
            ],
            "tools": [
                {"type":"function","name":"weather.lookup","description":"lookup weather","parameters":{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}}
            ],
            "reasoning": {"effort":"high"}
        });
        let rewritten =
            rewrite_responses_to_gemma4_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["enable_thinking"], true);
        assert_eq!(value["reasoning_effort"], "high");
        let messages = value["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["tool_calls"][0]["id"], "call_weather");
        assert_eq!(
            messages[2]["tool_calls"][0]["function"]["name"],
            "weather.lookup"
        );
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["name"], "weather.lookup");
        assert_eq!(messages[3]["tool_call_id"], "call_weather");
        assert_eq!(messages[3]["content"], "{\"temp_c\":18}");
    }

    #[test]
    fn gemma4_request_translation_splits_assistant_thought_channel_for_hf_template() {
        let payload = serde_json::json!({
            "model": "google/gemma-4-31B-it",
            "input": [
                {
                    "type":"message",
                    "role":"assistant",
                    "content":[{"type":"output_text","text":"<|channel>thought\nNeed a quick check.\n<channel|>Weather is mild."}]
                }
            ]
        });
        let rewritten =
            rewrite_responses_to_gemma4_chat_completions(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        let messages = value["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["content"], "Weather is mild.");
        assert_eq!(messages[0]["reasoning_content"], "Need a quick check.");
    }

    #[test]
    fn gemma4_response_translation_parses_channel_reasoning_and_tool_calls() {
        let payload = serde_json::json!({
            "id":"gemma123",
            "created":42,
            "model":"google/gemma-4-26B-A4B-it",
            "choices":[{
                "message":{
                    "content":"<|channel>thought\nNeed weather lookup.<channel|><|tool_call>call:weather.lookup{city:<|\\\"|>Berlin<|\\\"|>}<tool_call|>"
                }
            }],
            "usage":{"prompt_tokens":9,"completion_tokens":4,"total_tokens":13}
        });
        let rewritten = rewrite_gemma4_chat_completions_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["status"], "completed");
        assert_eq!(value["reasoning"], "Need weather lookup.");
        assert!(value["output_text"].is_null());
        assert_eq!(value["output"][0]["type"], "function_call");
        assert_eq!(value["output"][0]["name"], "weather.lookup");
        assert_eq!(
            value["output"][0]["arguments"],
            "{city:<|\\\"|>Berlin<|\\\"|>}"
        );
        assert_eq!(value["usage"]["input_tokens"], 9);
    }

    #[test]
    fn gemma4_response_translation_keeps_visible_text_alongside_raw_tool_calls() {
        let payload = serde_json::json!({
            "id":"gemma789",
            "created":42,
            "model":"google/gemma-4-31B-it",
            "choices":[{
                "message":{
                    "content":"<|channel>thought\nNeed weather lookup.\n<channel|>Weather looks mild.<|tool_call>call:weather.lookup{\"city\":\"Berlin\"}<tool_call|>"
                }
            }],
            "usage":{"prompt_tokens":12,"completion_tokens":8,"total_tokens":20}
        });
        let rewritten = rewrite_gemma4_chat_completions_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["reasoning"], "Need weather lookup.");
        assert_eq!(value["output_text"], "Weather looks mild.");
        let output = value["output"].as_array().unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output[0]["type"], "message");
        assert_eq!(output[0]["content"][0]["text"], "Weather looks mild.");
        assert_eq!(output[1]["type"], "function_call");
        assert_eq!(output[1]["name"], "weather.lookup");
        assert_eq!(output[1]["arguments"], "{\"city\":\"Berlin\"}");
    }

    #[test]
    fn gemma4_response_translation_keeps_structured_message_fields() {
        let payload = serde_json::json!({
            "id":"gemma456",
            "created":42,
            "model":"google/gemma-4-31B-it",
            "choices":[{
                "message":{
                    "content":"Weather is mild.",
                    "reasoning_content":"I checked the provided tool result.",
                    "tool_calls":[{"id":"call_weather","function":{"name":"weather.lookup","arguments":"{\"city\":\"Berlin\"}"}}]
                }
            }],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}
        });
        let rewritten = rewrite_gemma4_chat_completions_to_responses(
            &serde_json::to_vec(&payload).unwrap(),
            None,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["output_text"], "Weather is mild.");
        assert_eq!(value["reasoning"], "I checked the provided tool result.");
        assert_eq!(value["output"][0]["type"], "message");
        assert_eq!(value["output"][1]["type"], "function_call");
        assert_eq!(value["output"][1]["call_id"], "call_weather");
    }
}
