use anyhow::Context;
use anyhow::Result;
use crossterm::cursor;
use crossterm::event;
use crossterm::event::Event as TerminalEvent;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use crossterm::execute;
use crossterm::terminal;
use crossterm::terminal::ClearType;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use qrcode::types::Color as QrColor;
use qrcode::QrCode;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use self::text_editor::EditorAction;
use self::text_editor::ExitReason;
use self::text_editor::TextEditor;
use crate::channels;
use crate::communication::adapters as communication_adapters;
use crate::context_health;
use crate::execution::models::vision_preprocessor;
use crate::governance;
use crate::inference::engine;
use crate::inference::gateway::LoadObservation;
use crate::inference::gateway::RuntimeTelemetry;
use crate::inference::model_registry;
use crate::inference::runtime_control;
use crate::inference::runtime_env;
use crate::inference::runtime_kernel;
use crate::inference::runtime_plan;
use crate::inference::runtime_state;
use crate::inference::supervisor;
use crate::inference::turn_loop;
use crate::lcm;
use crate::live_context;
use crate::secrets;
use crate::service;

mod render;
mod text_editor;

const DEFAULT_CHAT_SOURCE: &str = "local";
const DEFAULT_API_PROVIDER: &str = "local";
const OPENAI_AUTH_MODE_KEY: &str = "CTOX_OPENAI_AUTH_MODE";
const DEFAULT_OPENAI_AUTH_MODE: &str = "api_key";
const OPENAI_AUTH_MODE_CHOICES: &[&str] = &["api_key", "chatgpt_subscription"];
const WORK_HOURS_CHOICES: &[&str] = &["off", "on"];
const DEFAULT_LOCAL_RUNTIME: &str = "candle";
const DEFAULT_CHAT_PRESET: &str = "Quality";
const DEFAULT_CHAT_SKILL_PRESET: &str = "Standard";
const DEFAULT_CTO_OPERATING_MODE_PROMPT: &str =
    include_str!("../../../assets/prompts/ctox_cto_operating_mode.md");
const CTOX_CTO_OPERATING_MODE_KEY: &str = "CTOX_CTO_OPERATING_MODE_PROMPT";
const DEFAULT_COMMUNICATION_PATH: &str = "tui";
const CHAT_PRESET_CHOICES: &[&str] = &["Quality", "Performance"];
const CHAT_SKILL_PRESET_CHOICES: &[&str] = &["Standard", "Simple"];
const API_PROVIDER_CHOICES: &[&str] = &[
    "local",
    "openai",
    "anthropic",
    "openrouter",
    "minimax",
    "azure_foundry",
];
const AZURE_FOUNDRY_ENDPOINT_KEY: &str = "CTOX_AZURE_FOUNDRY_ENDPOINT";
const AZURE_FOUNDRY_DEPLOYMENT_ID_KEY: &str = "CTOX_AZURE_FOUNDRY_DEPLOYMENT_ID";
const AZURE_FOUNDRY_TOKEN_KEY: &str = "AZURE_FOUNDRY_API_KEY";
const LOCAL_RUNTIME_CHOICES: &[&str] = &["candle"];
const NO_GPU_LOCAL_CHAT_MODEL_CHOICES: &[&str] = &[];
const NO_GPU_LOCAL_CHAT_FAMILY_CHOICES: &[&str] = &[];
const COMMUNICATION_PATH_CHOICES: &[&str] = &["tui", "email", "jami", "teams", "whatsapp"];
const DEFAULT_REMOTE_BRIDGE_MODE: &str = "disabled";
const REMOTE_BRIDGE_MODE_CHOICES: &[&str] = &["disabled", "remote-webrtc"];
const EMAIL_PROVIDER_CHOICES: &[&str] = &["imap", "graph", "ews"];
const EMAIL_EWS_AUTH_CHOICES: &[&str] = &["basic", "oauth2"];
const UI_REFRESH_INTERVAL_ACTIVE: Duration = Duration::from_millis(350);
const UI_REFRESH_INTERVAL_SETTINGS: Duration = Duration::from_millis(700);
const SERVICE_REFRESH_INTERVAL_ACTIVE: Duration = Duration::from_millis(500);
const SERVICE_REFRESH_INTERVAL_SETTINGS: Duration = Duration::from_millis(1200);
const CHAT_REFRESH_INTERVAL_ACTIVE: Duration = Duration::from_millis(500);
const CHAT_REFRESH_INTERVAL_BACKGROUND: Duration = Duration::from_secs(3);
const COMMUNICATION_REFRESH_INTERVAL_ACTIVE: Duration = Duration::from_secs(1);

fn default_active_model() -> &'static str {
    model_registry::default_local_chat_model()
}

fn default_local_chat_family_label() -> &'static str {
    model_registry::default_local_chat_family_label()
}
const COMMUNICATION_REFRESH_INTERVAL_BACKGROUND: Duration = Duration::from_secs(5);
const SKILL_REFRESH_INTERVAL_ACTIVE: Duration = Duration::from_secs(2);
const HARNESS_FLOW_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
const GPU_REFRESH_INTERVAL_ACTIVE: Duration = Duration::from_secs(2);
const GPU_REFRESH_INTERVAL_SETTINGS: Duration = Duration::from_secs(4);
const PROXY_REFRESH_INTERVAL_ACTIVE: Duration = Duration::from_millis(700);
const PROXY_REFRESH_INTERVAL_SETTINGS: Duration = Duration::from_millis(1200);

fn default_bin_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/bin")
}

fn resolved_install_root_for_settings(root: &Path) -> Option<PathBuf> {
    crate::install::version_info(root)
        .ok()
        .and_then(|info| info.install_root)
}

fn resolved_state_root_for_settings(root: &Path) -> PathBuf {
    crate::install::version_info(root)
        .map(|info| info.state_root)
        .unwrap_or_else(|_| root.join("runtime"))
}

fn resolved_cache_root_for_settings(root: &Path) -> PathBuf {
    crate::install::version_info(root)
        .map(|info| info.cache_root)
        .unwrap_or_else(|_| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".cache/ctox")
        })
}

fn persisted_path_setting(root: &Path, key: &str, fallback: PathBuf) -> String {
    runtime_env::env_or_config(root, key)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.display().to_string())
}

fn refresh_due(last_refresh_at: &mut Option<Instant>, interval: Duration) -> bool {
    let now = Instant::now();
    match last_refresh_at {
        Some(last) if now.duration_since(*last) < interval => false,
        _ => {
            *last_refresh_at = Some(now);
            true
        }
    }
}

fn local_gpu_available(root: &Path) -> bool {
    if let Some(spec) = runtime_env::env_or_config(root, "CTOX_TEST_GPU_TOTALS_MB") {
        return spec
            .split(';')
            .filter_map(|chunk| chunk.split_once(':'))
            .any(|(_, total)| total.trim().parse::<u64>().ok().is_some());
    }
    Command::new("nvidia-smi")
        .args(["--query-gpu=index", "--format=csv,noheader,nounits"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| !String::from_utf8_lossy(&output.stdout).trim().is_empty())
        .unwrap_or(false)
}

fn supported_local_chat_model_choices_with_gpu(
    root: &Path,
    env_map: &BTreeMap<String, String>,
    gpu_available: bool,
) -> Vec<&'static str> {
    if gpu_available {
        runtime_plan::local_models_satisfying_context_policy(
            root,
            engine::SUPPORTED_CHAT_MODELS
                .iter()
                .filter(|model| engine::supports_local_chat_runtime(model)),
            env_map,
        )
    } else {
        runtime_plan::local_models_satisfying_context_policy(
            root,
            NO_GPU_LOCAL_CHAT_MODEL_CHOICES.iter(),
            env_map,
        )
    }
}

fn supported_local_chat_model_choices(
    root: &Path,
    env_map: &BTreeMap<String, String>,
) -> Vec<&'static str> {
    supported_local_chat_model_choices_with_gpu(root, env_map, local_gpu_available(root))
}

fn supported_local_chat_family_choices_with_gpu(
    root: &Path,
    env_map: &BTreeMap<String, String>,
    gpu_available: bool,
) -> Vec<&'static str> {
    let mut choices = runtime_plan::local_chat_family_choices(root, env_map);
    if !gpu_available {
        choices.retain(|choice| {
            NO_GPU_LOCAL_CHAT_FAMILY_CHOICES
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(choice))
        });
    }
    choices
}

fn supported_local_chat_family_choices(
    root: &Path,
    env_map: &BTreeMap<String, String>,
) -> Vec<&'static str> {
    supported_local_chat_family_choices_with_gpu(root, env_map, local_gpu_available(root))
}

fn selected_local_chat_family(env_map: &BTreeMap<String, String>) -> Option<String> {
    runtime_env::configured_chat_model_family_from_map(env_map)
        .and_then(|value| {
            engine::parse_chat_model_family(&value).map(|family| family.label().to_string())
        })
        .or_else(|| {
            runtime_env::configured_chat_model_from_map(env_map).and_then(|model| {
                engine::chat_model_family_for_model(&model).map(|family| family.label().to_string())
            })
        })
}

fn api_provider_key_env_var(provider: &str) -> &'static str {
    runtime_state::api_key_env_var_for_provider(provider)
}

fn api_key_configured(env_map: &BTreeMap<String, String>, provider: &str) -> bool {
    env_map
        .get(api_provider_key_env_var(provider))
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn openai_subscription_auth_enabled(env_map: &BTreeMap<String, String>) -> bool {
    env_map
        .get(OPENAI_AUTH_MODE_KEY)
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| {
            matches!(
                value.as_str(),
                "chatgpt_subscription" | "subscription" | "codex_subscription" | "chatgpt"
            )
        })
}

fn provider_auth_configured(env_map: &BTreeMap<String, String>, provider: &str) -> bool {
    api_key_configured(env_map, provider)
        || (provider.eq_ignore_ascii_case("openai") && openai_subscription_auth_enabled(env_map))
}

fn infer_chat_source(env_map: &BTreeMap<String, String>) -> String {
    if env_map
        .get("CTOX_CHAT_SOURCE")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("api"))
    {
        return "api".to_string();
    }
    let provider = infer_api_provider(env_map);
    let configured_model = runtime_env::configured_chat_model_from_map(env_map);
    if configured_model.as_deref().is_some_and(|model| {
        (engine::is_api_chat_model(model))
            || (!provider.eq_ignore_ascii_case("local")
                && engine::api_provider_supports_model(&provider, model))
    }) && (!provider.eq_ignore_ascii_case("local")
        || configured_model
            .as_deref()
            .is_some_and(|model| engine::is_api_chat_model(model)))
    {
        "api".to_string()
    } else {
        DEFAULT_CHAT_SOURCE.to_string()
    }
}

fn infer_local_runtime(env_map: &BTreeMap<String, String>) -> String {
    runtime_state::infer_local_runtime_kind_from_env_map(env_map)
        .as_env_value()
        .to_string()
}

fn infer_api_provider(env_map: &BTreeMap<String, String>) -> String {
    let explicit_source_api = env_map
        .get("CTOX_CHAT_SOURCE")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("api"));
    let explicit_provider = env_map
        .get("CTOX_API_PROVIDER")
        .map(|value| runtime_state::normalize_api_provider(value).to_string());
    let model_provider = runtime_env::configured_chat_model_from_map(env_map)
        .filter(|value| explicit_source_api || engine::is_api_chat_model(value))
        .map(|value| engine::default_api_provider_for_model(value.as_str()).to_string());
    match (explicit_provider, model_provider) {
        (Some(explicit), Some(model_provider))
            if explicit.eq_ignore_ascii_case("local")
                && (explicit_source_api
                    || !runtime_env::configured_chat_model_from_map(env_map)
                        .as_deref()
                        .is_some_and(engine::supports_local_chat_runtime)) =>
        {
            model_provider
        }
        (Some(explicit), _) => explicit,
        (None, Some(model_provider)) => model_provider,
        (None, None) => env_map
            .get(AZURE_FOUNDRY_ENDPOINT_KEY)
            .filter(|value| !value.trim().is_empty())
            .map(|_| "azure_foundry".to_string())
            .unwrap_or_else(|| DEFAULT_API_PROVIDER.to_string()),
    }
}

fn supported_chat_model_choices(
    root: &Path,
    env_map: &BTreeMap<String, String>,
) -> Vec<&'static str> {
    supported_chat_model_choices_with_gpu(root, env_map, local_gpu_available(root))
}

fn supported_chat_model_choices_with_gpu(
    root: &Path,
    env_map: &BTreeMap<String, String>,
    gpu_available: bool,
) -> Vec<&'static str> {
    if infer_api_provider(env_map).eq_ignore_ascii_case("azure_foundry") {
        return Vec::new();
    }
    let mut choices = supported_local_chat_model_choices_with_gpu(root, env_map, gpu_available);
    for model in supported_api_chat_model_choices(env_map) {
        if !choices
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(model))
        {
            choices.push(model);
        }
    }
    choices
}

fn supported_api_chat_model_choices(env_map: &BTreeMap<String, String>) -> Vec<&'static str> {
    let explicit_api_source = env_map
        .get("CTOX_CHAT_SOURCE")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("api"));
    let provider = infer_api_provider(env_map);
    let selected_provider_model = runtime_env::configured_chat_model_from_map(env_map)
        .as_deref()
        .map(|model| engine::api_provider_supports_model(&provider, model))
        .unwrap_or(false);
    if !provider.eq_ignore_ascii_case("local")
        && (provider_auth_configured(env_map, &provider)
            || explicit_api_source
            || selected_provider_model)
    {
        if provider.eq_ignore_ascii_case("azure_foundry") {
            return Vec::new();
        }
        if provider.eq_ignore_ascii_case("anthropic") {
            return engine::SUPPORTED_ANTHROPIC_API_CHAT_MODELS.to_vec();
        }
        if provider.eq_ignore_ascii_case("openrouter") {
            return engine::SUPPORTED_OPENROUTER_API_CHAT_MODELS.to_vec();
        }
        if provider.eq_ignore_ascii_case("openai") {
            return engine::SUPPORTED_OPENAI_API_CHAT_MODELS.to_vec();
        }
    }
    Vec::new()
}

fn supported_boost_model_choices(
    root: &Path,
    env_map: &BTreeMap<String, String>,
) -> Vec<&'static str> {
    if infer_api_provider(env_map).eq_ignore_ascii_case("azure_foundry") {
        return Vec::new();
    }
    let mut choices = supported_local_chat_model_choices(root, env_map);
    for model in supported_api_chat_model_choices(env_map) {
        if !choices
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(model))
        {
            choices.push(model);
        }
    }
    choices
}

fn choice_contains(choices: &[&'static str], value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && choices
            .iter()
            .any(|choice| choice.eq_ignore_ascii_case(trimmed))
}

fn supported_embedding_model_choices() -> Vec<&'static str> {
    engine::SUPPORTED_EMBEDDING_MODELS.to_vec()
}

fn supported_stt_model_choices() -> Vec<&'static str> {
    engine::SUPPORTED_STT_MODELS.to_vec()
}

fn supported_tts_model_choices() -> Vec<&'static str> {
    engine::SUPPORTED_TTS_MODELS.to_vec()
}
const DEFAULT_MAX_CONTEXT: usize = 131_072;
const MAX_CONTEXT_WINDOW: usize = 262_144;
const DEFAULT_COMPACTION_THRESHOLD_PERCENT: usize = 75;
const DEFAULT_COMPACTION_MIN_TOKENS: usize = 12_288;

fn default_compaction_min_tokens(context_tokens: usize) -> usize {
    runtime_plan::scale_compaction_tokens(
        DEFAULT_COMPACTION_MIN_TOKENS as u32,
        context_tokens as u32,
    ) as usize
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Chat,
    Skills,
    Costs,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsView {
    Model,
    Communication,
    Secrets,
    Paths,
    Update,
    BusinessOs,
    HarnessMining,
    HarnessFlow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingUpdateAction {
    Upgrade,
    EngineRebuild,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UpdateViewState {
    pub info_json: String,
    pub check_json: String,
    pub last_action_line: String,
    pub pending: Option<PendingUpdateAction>,
}

#[derive(Debug, Clone)]
struct SettingItem {
    key: &'static str,
    label: &'static str,
    value: String,
    saved_value: String,
    secret: bool,
    choices: Vec<&'static str>,
    help: &'static str,
    kind: SettingKind,
}

#[derive(Debug, Clone)]
struct SecretItem {
    scope: String,
    name: String,
    description: Option<String>,
    metadata: Value,
    created_at: String,
    updated_at: String,
    value: String,
    saved_value: String,
}

#[derive(Debug, Clone)]
struct SettingsTextEditorState {
    key: &'static str,
    label: &'static str,
    editor: TextEditor,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct HeaderState {
    chat_source: String,
    model: String,
    base_model: String,
    service_running: bool,
    boost_model: Option<String>,
    boost_active: bool,
    boost_remaining_seconds: Option<u64>,
    boost_reason: Option<String>,
    max_context: usize,
    realized_context: usize,
    configured_context: usize,
    compact_at: usize,
    compact_percent: usize,
    compact_min_tokens: usize,
    current_tokens: usize,
    tokens_per_second: Option<f64>,
    avg_tokens_per_second: Option<f64>,
    last_input_tokens: Option<u64>,
    last_output_tokens: Option<u64>,
    last_total_tokens: Option<u64>,
    today_api_cost_microusd: i64,
    today_api_cost_events: u64,
    today_api_unpriced_events: u64,
    gpu_cards: Vec<GpuCardState>,
    gpu_loading_cards: Vec<GpuCardState>,
    gpu_error_cards: Vec<GpuCardState>,
    gpu_target_cards: Vec<GpuCardState>,
    backend_warmup: bool,
    expected_aux_models: Vec<String>,
    estimate_mode: bool,
    chat_plan: Option<runtime_plan::ChatRuntimePlan>,
}

impl Default for HeaderState {
    fn default() -> Self {
        Self {
            chat_source: DEFAULT_CHAT_SOURCE.to_string(),
            model: default_active_model().to_string(),
            base_model: default_active_model().to_string(),
            service_running: false,
            boost_model: None,
            boost_active: false,
            boost_remaining_seconds: None,
            boost_reason: None,
            max_context: DEFAULT_MAX_CONTEXT,
            realized_context: DEFAULT_MAX_CONTEXT,
            configured_context: DEFAULT_MAX_CONTEXT,
            compact_at: DEFAULT_MAX_CONTEXT * DEFAULT_COMPACTION_THRESHOLD_PERCENT / 100,
            compact_percent: DEFAULT_COMPACTION_THRESHOLD_PERCENT,
            compact_min_tokens: DEFAULT_COMPACTION_MIN_TOKENS,
            current_tokens: 0,
            tokens_per_second: None,
            avg_tokens_per_second: None,
            last_input_tokens: None,
            last_output_tokens: None,
            last_total_tokens: None,
            today_api_cost_microusd: 0,
            today_api_cost_events: 0,
            today_api_unpriced_events: 0,
            gpu_cards: Vec::new(),
            gpu_loading_cards: Vec::new(),
            gpu_error_cards: Vec::new(),
            gpu_target_cards: Vec::new(),
            backend_warmup: false,
            expected_aux_models: Vec::new(),
            estimate_mode: false,
            chat_plan: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct GpuModelUsage {
    model: String,
    short_label: String,
    used_mb: u64,
}

#[derive(Debug, Clone, Default)]
struct GpuCardState {
    index: usize,
    name: String,
    used_mb: u64,
    total_mb: u64,
    utilization: u64,
    allocations: Vec<GpuModelUsage>,
}

#[derive(Debug, Clone, Default)]
struct RuntimeHealthState {
    runtime_ready: bool,
    embedding_ready: Option<bool>,
    stt_ready: Option<bool>,
    tts_ready: Option<bool>,
}

impl RuntimeHealthState {
    fn degraded_components(&self) -> Vec<&'static str> {
        let mut parts = Vec::new();
        if !self.runtime_ready {
            parts.push("runtime");
        }
        if self.embedding_ready == Some(false) {
            parts.push("embed");
        }
        if self.stt_ready == Some(false) {
            parts.push("stt");
        }
        if self.tts_ready == Some(false) {
            parts.push("tts");
        }
        parts
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ModelPerfStats {
    samples: u64,
    avg_tokens_per_second: f64,
    last_tokens_per_second: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct JamiResolvedEnvelope {
    #[serde(default)]
    ok: bool,
    #[serde(rename = "resolvedAccount")]
    resolved_account: Option<JamiResolvedAccount>,
    #[serde(default)]
    error: Option<String>,
    #[serde(rename = "dbusEnvFile", default)]
    dbus_env_file: Option<String>,
    #[serde(default)]
    checks: Vec<JamiDoctorCheck>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
struct JamiResolvedAccount {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(rename = "accountType")]
    account_type: String,
    username: String,
    #[serde(rename = "shareUri")]
    share_uri: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(default)]
    provisioned: bool,
}

#[derive(Debug, Clone, Default)]
struct JamiResolveOutcome {
    account: Option<JamiResolvedAccount>,
    error: Option<String>,
    dbus_env_file: Option<String>,
    checks: Vec<JamiDoctorCheck>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
struct JamiDoctorCheck {
    name: String,
    ok: bool,
    #[serde(default)]
    detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingKind {
    Env,
    ServiceToggle,
}

/// A single image the user has queued up for the next chat submission.
/// Stored as an absolute path; byte size is captured for display only.
#[derive(Debug, Clone)]
struct PendingImage {
    path: PathBuf,
    size_bytes: u64,
}

const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "heic", "heif",
];
const MAX_IMAGE_ATTACHMENT_BYTES: u64 = 20 * 1024 * 1024;

/// Capture the current system clipboard as a PNG file and return the path
/// on success. Uses platform-native CLIs so we don't take a dependency on
/// `arboard` / objc-crate chains — this keeps the CTOX build portable and
/// avoids another hard-failure surface for macOS sandboxing.
///
/// - macOS: `pbpaste -Prefer png` writes raw PNG bytes to stdout.
/// - Linux/Wayland: `wl-paste --type image/png`.
/// - Linux/X11: `xclip -selection clipboard -t image/png -o`.
///
/// Returns None if no image is on the clipboard or the platform CLI is
/// missing / returned empty output.
fn capture_clipboard_image_to_tempfile() -> Option<PathBuf> {
    use std::process::Command;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let tempfile = std::env::temp_dir().join(format!("ctox-clipboard-{timestamp}.png"));

    #[cfg(target_os = "macos")]
    let output = Command::new("pbpaste").args(["-Prefer", "png"]).output();

    #[cfg(all(unix, not(target_os = "macos")))]
    let output = {
        // Prefer Wayland if available, else fall back to xclip.
        let wl = Command::new("wl-paste")
            .args(["--type", "image/png"])
            .output();
        match wl {
            Ok(result) if result.status.success() && !result.stdout.is_empty() => Ok(result),
            _ => Command::new("xclip")
                .args(["-selection", "clipboard", "-t", "image/png", "-o"])
                .output(),
        }
    };

    #[cfg(not(unix))]
    let output: std::io::Result<std::process::Output> = Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "clipboard image paste is only supported on macOS/Linux in this build",
    ));

    let data = match output {
        Ok(result) if result.status.success() && !result.stdout.is_empty() => result.stdout,
        _ => return None,
    };
    // Sanity-check: PNG signature.
    if data.len() < 8 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    std::fs::write(&tempfile, &data).ok()?;
    Some(tempfile)
}

/// Parse a pasted-string or slash-command argument into a candidate image
/// attachment. Returns Some only if the string points to an existing,
/// readable file with an image extension within the size limit.
fn try_resolve_image_attachment(input: &str, cwd: &Path) -> Option<PendingImage> {
    let trimmed = input.trim().trim_matches(|c| c == '"' || c == '\'');
    if trimmed.is_empty() {
        return None;
    }
    let raw = PathBuf::from(trimmed);
    let candidate = if raw.is_absolute() {
        raw
    } else {
        cwd.join(raw)
    };
    let canonical = std::fs::canonicalize(&candidate).ok()?;
    let metadata = std::fs::metadata(&canonical).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let extension = canonical
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)?;
    if !IMAGE_EXTENSIONS.iter().any(|ext| *ext == extension) {
        return None;
    }
    if metadata.len() > MAX_IMAGE_ATTACHMENT_BYTES {
        return None;
    }
    Some(PendingImage {
        path: canonical,
        size_bytes: metadata.len(),
    })
}

struct App {
    root: PathBuf,
    db_path: PathBuf,
    page: Page,
    chat_input: String,
    chat_messages: Vec<lcm::MessageRecord>,
    draft_queue: VecDeque<String>,
    activity_log: Vec<String>,
    communication_feed: Vec<channels::CommunicationFeedItem>,
    status_line: String,
    spinner_phase: usize,
    header: HeaderState,
    prompt_context_breakdown: Option<live_context::PromptContextBreakdown>,
    context_health: Option<context_health::ContextHealthSnapshot>,
    mission_state: Option<lcm::MissionStateRecord>,
    settings_items: Vec<SettingItem>,
    settings_selected: usize,
    settings_view: SettingsView,
    secret_items: Vec<SecretItem>,
    secrets_selected: usize,
    update_view: UpdateViewState,
    settings_text_editor: Option<SettingsTextEditorState>,
    settings_menu_open: bool,
    settings_menu_index: usize,
    jami_qr_lines: Vec<String>,
    last_jami_qr_key: String,
    last_jami_refresh_at: Option<Instant>,
    jami_runtime_account: Option<JamiResolvedAccount>,
    settings_dirty: bool,
    service_status: service::ServiceStatus,
    last_service_refresh_at: Option<Instant>,
    request_in_flight: bool,
    runtime_switch_in_flight: bool,
    runtime_switch_rx: Option<Receiver<Result<String, String>>>,
    pending_runtime_transition_cards: Option<Vec<GpuCardState>>,
    model_perf_stats: BTreeMap<String, ModelPerfStats>,
    last_recorded_response_at: Option<String>,
    gpu_cards: Vec<GpuCardState>,
    last_gpu_refresh_at: Option<Instant>,
    runtime_telemetry: Option<RuntimeTelemetry>,
    last_runtime_refresh_at: Option<Instant>,
    runtime_health: RuntimeHealthState,
    chat_preset_bundle: Option<runtime_plan::ChatPresetBundle>,
    skill_catalog: Vec<SkillCatalogEntry>,
    skills_selected: usize,
    last_chat_refresh_at: Option<Instant>,
    last_communication_refresh_at: Option<Instant>,
    last_skill_catalog_refresh_at: Option<Instant>,
    harness_flow_text: String,
    harness_flow_scroll: u16,
    last_harness_flow_refresh_at: Option<Instant>,
    /// Images the user has attached to the next chat submission. Populated
    /// by the `/image <path>` slash-command and by file-path drag-and-drop
    /// pastes. On submit, each pending image becomes a
    /// `[[ctox:image:/absolute/path]]` marker prefixed to the prompt text.
    pending_images: Vec<PendingImage>,
}

#[derive(Debug, Clone, Default)]
struct SkillCatalogEntry {
    name: String,
    class: SkillClass,
    state: SkillState,
    cluster: String,
    skill_path: PathBuf,
    description: String,
    helper_tools: Vec<String>,
    resources: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SkillClass {
    CodexCore,
    #[default]
    CtoxCore,
    InstalledPacks,
    Personal,
}

impl SkillClass {
    fn label(self) -> &'static str {
        match self {
            Self::CodexCore => "Codex Core",
            Self::CtoxCore => "CTOX Core",
            Self::InstalledPacks => "Installed Packs",
            Self::Personal => "Personal",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::CtoxCore => 0,
            Self::CodexCore => 1,
            Self::InstalledPacks => 2,
            Self::Personal => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SkillState {
    #[default]
    Stable,
    Authored,
    Generated,
    Draft,
}

impl SkillState {
    fn label(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Authored => "authored",
            Self::Generated => "generated",
            Self::Draft => "draft",
        }
    }
}

pub fn run_tui(root: &Path) -> Result<()> {
    let db_path = crate::persistence::sqlite_path(root);
    let _ = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;

    let mut stdout = io::stdout();
    let _guard = TerminalGuard::enter(&mut stdout)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to initialize TUI terminal")?;
    let mut app = App::new(root.to_path_buf(), db_path);
    app.refresh()?;
    terminal
        .draw(|frame| render::draw(frame, &app))
        .context("failed to draw initial TUI frame")?;

    let mut last_refresh = Instant::now();
    loop {
        if event::poll(Duration::from_millis(125)).context("failed to poll terminal events")? {
            match event::read().context("failed to read terminal event")? {
                TerminalEvent::Key(key_event) => {
                    if app.handle_key_event(key_event)? {
                        break;
                    }
                }
                TerminalEvent::Resize(_, _) => {}
                TerminalEvent::Paste(text) => app.handle_paste(&text),
                _ => {}
            }
        }

        let refresh_interval = if app.page == Page::Settings {
            UI_REFRESH_INTERVAL_SETTINGS
        } else {
            UI_REFRESH_INTERVAL_ACTIVE
        };
        if last_refresh.elapsed() >= refresh_interval {
            app.refresh()?;
            last_refresh = Instant::now();
        }

        app.poll_worker()?;
        app.spinner_phase = (app.spinner_phase + 1) % 4;
        terminal
            .draw(|frame| render::draw(frame, &app))
            .context("failed to draw TUI frame")?;
    }

    Ok(())
}

/// Headless smoke-test renderer: creates a `TestBackend`, renders one frame,
/// and prints the buffer contents to stdout. No real terminal required.
pub fn run_tui_smoke(root: &Path, page_name: &str, width: u16, height: u16) -> Result<()> {
    use ratatui::backend::TestBackend;

    let db_path = crate::persistence::sqlite_path(root);
    let _ = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;

    let mut app = App::new(root.to_path_buf(), db_path);
    let mut skip_initial_refresh = false;
    match page_name {
        "chat" => app.page = Page::Chat,
        "skills" => app.page = Page::Skills,
        "cost" | "costs" => {
            app.page = Page::Costs;
            skip_initial_refresh = true;
        }
        "settings" => app.page = Page::Settings,
        "business-os" | "settings-business-os" => {
            app.page = Page::Settings;
            app.switch_settings_view(SettingsView::BusinessOs);
            skip_initial_refresh = true;
        }
        "harness-flow" | "settings-harness-flow" => {
            app.page = Page::Settings;
            app.switch_settings_view(SettingsView::HarnessFlow);
            app.refresh_harness_flow();
            skip_initial_refresh = true;
        }
        other => {
            anyhow::bail!(
                "unknown page: {other} (expected chat, skills, costs, settings, business-os, harness-flow)"
            )
        }
    }
    if !skip_initial_refresh {
        app.refresh()?;
    }

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).context("failed to create test terminal")?;
    terminal
        .draw(|frame| render::draw(frame, &app))
        .context("failed to render smoke frame")?;

    let buf = terminal.backend().buffer().clone();
    for y in 0..height {
        let mut line = String::with_capacity(width as usize);
        for x in 0..width {
            let cell = &buf[(x, y)];
            line.push_str(cell.symbol());
        }
        println!("{}", line.trim_end());
    }
    Ok(())
}

/// Headless key-event injection: creates a `TestBackend`, sends a sequence of
/// key events, renders after each, and returns the final buffer as a string.
/// Used for automated TUI interaction testing.
pub fn run_tui_inject(
    root: &Path,
    page_name: &str,
    width: u16,
    height: u16,
    keys: &[KeyCode],
) -> Result<String> {
    use ratatui::backend::TestBackend;

    let db_path = crate::persistence::sqlite_path(root);
    let _ = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;

    let mut app = App::new(root.to_path_buf(), db_path);
    match page_name {
        "chat" => app.page = Page::Chat,
        "skills" => app.page = Page::Skills,
        "cost" | "costs" => app.page = Page::Costs,
        "settings" => app.page = Page::Settings,
        other => anyhow::bail!("unknown page: {other}"),
    }
    app.refresh()?;

    for &key in keys {
        let event = KeyEvent::new(key, KeyModifiers::NONE);
        let quit = app.handle_key_event(event)?;
        if quit {
            break;
        }
    }
    app.refresh()?;

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).context("failed to create test terminal")?;
    terminal
        .draw(|frame| render::draw(frame, &app))
        .context("failed to render frame after injection")?;

    let buf = terminal.backend().buffer().clone();
    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }
    Ok(output)
}

impl App {
    fn new(root: PathBuf, db_path: PathBuf) -> Self {
        let service_status =
            service::service_status_snapshot(&root).unwrap_or_else(|_| service::ServiceStatus {
                running: false,
                busy: false,
                pid: None,
                listen_addr: "127.0.0.1:12435".to_string(),
                autostart_enabled: false,
                manager: "process".to_string(),
                pending_count: 0,
                pending_previews: Vec::new(),
                blocked_count: 0,
                blocked_previews: Vec::new(),
                current_goal_preview: None,
                active_source_label: None,
                recent_events: Vec::new(),
                last_error: None,
                last_completed_at: None,
                last_reply_chars: None,
                monitor_last_check_at: None,
                monitor_alerts: Vec::new(),
                monitor_last_error: None,
                last_agent_outcome: None,
                work_hours: service::working_hours::snapshot(&root),
            });
        let mut app = Self {
            root: root.clone(),
            db_path,
            page: Page::Chat,
            chat_input: String::new(),
            chat_messages: Vec::new(),
            draft_queue: VecDeque::new(),
            activity_log: Vec::new(),
            communication_feed: Vec::new(),
            status_line: "Tab chat/skills/settings · Ctrl-C quit · Enter open/save".to_string(),
            spinner_phase: 0,
            header: HeaderState::default(),
            prompt_context_breakdown: None,
            context_health: None,
            mission_state: None,
            settings_items: load_settings_items(&root),
            settings_selected: 0,
            settings_view: SettingsView::Model,
            secret_items: load_secret_items(&root),
            secrets_selected: 0,
            update_view: UpdateViewState::default(),
            settings_text_editor: None,
            settings_menu_open: false,
            settings_menu_index: 0,
            jami_qr_lines: Vec::new(),
            last_jami_qr_key: String::new(),
            last_jami_refresh_at: None,
            jami_runtime_account: None,
            settings_dirty: false,
            service_status,
            last_service_refresh_at: None,
            request_in_flight: false,
            runtime_switch_in_flight: false,
            runtime_switch_rx: None,
            pending_runtime_transition_cards: None,
            model_perf_stats: load_model_perf_stats(&root),
            last_recorded_response_at: None,
            gpu_cards: Vec::new(),
            last_gpu_refresh_at: None,
            runtime_telemetry: None,
            last_runtime_refresh_at: None,
            runtime_health: RuntimeHealthState::default(),
            chat_preset_bundle: None,
            skill_catalog: load_skill_catalog(&root),
            skills_selected: 0,
            last_chat_refresh_at: None,
            last_communication_refresh_at: None,
            last_skill_catalog_refresh_at: None,
            harness_flow_text: String::new(),
            harness_flow_scroll: 0,
            last_harness_flow_refresh_at: None,
            pending_images: Vec::new(),
        };
        if let Some(first) = app.visible_setting_indices().first().copied() {
            app.settings_selected = first;
        }
        if !app.secret_items.is_empty() {
            app.secrets_selected = 0;
        }
        app
    }

    fn handle_paste(&mut self, text: &str) {
        match self.page {
            Page::Chat => {
                // Terminals forward drag-and-drop as a paste whose payload
                // is the absolute path of the file being dropped. If that
                // path points to an existing image file, attach it as a
                // pending image instead of splatting the path into the
                // chat input. Multi-file drops arrive space-separated on
                // most terminals; we resolve each candidate independently.
                let candidates: Vec<&str> = if text.contains('\n') {
                    text.split('\n').collect()
                } else {
                    text.split_ascii_whitespace().collect()
                };
                let mut attached_any = false;
                if candidates.len() >= 1 {
                    for candidate in &candidates {
                        if let Some(image) = try_resolve_image_attachment(candidate, &self.root) {
                            self.pending_images.push(image);
                            attached_any = true;
                        }
                    }
                }
                if attached_any {
                    self.status_line = format!(
                        "📎 attached {} image(s) — total pending {}",
                        candidates
                            .iter()
                            .filter(|c| try_resolve_image_attachment(c, &self.root).is_some())
                            .count(),
                        self.pending_images.len()
                    );
                } else {
                    self.chat_input.push_str(text);
                }
            }
            Page::Skills => {}
            Page::Costs => {}
            Page::Settings => {
                if let Some(item) = self.settings_items.get_mut(self.settings_selected) {
                    item.value.push_str(text);
                    self.settings_dirty = true;
                    self.refresh_dynamic_setting_choices();
                }
            }
        }
    }

    /// Handle a `/image <path>` slash-command: validate the path and
    /// attach to the pending-images queue. Returns true if the input was
    /// recognised as an image command (the caller should then skip normal
    /// submission).
    fn handle_image_command(&mut self, input: &str) -> bool {
        let trimmed = input.trim();
        let rest = if let Some(rest) = trimmed.strip_prefix("/image") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix("/img") {
            rest.trim()
        } else {
            return false;
        };
        if rest.is_empty() {
            if self.pending_images.is_empty() {
                self.status_line =
                    "Usage: /image <absolute-or-relative-path-to-image-file>".to_string();
            } else {
                self.status_line = format!(
                    "{} image(s) pending. Usage: /image <path> to attach more, Ctrl-X to clear.",
                    self.pending_images.len()
                );
            }
            return true;
        }
        if rest == "clear" {
            let cleared = self.pending_images.len();
            self.pending_images.clear();
            self.status_line = format!("Cleared {cleared} pending image(s).");
            return true;
        }
        match try_resolve_image_attachment(rest, &self.root) {
            Some(image) => {
                let display_path = image.path.display().to_string();
                let kib = image.size_bytes / 1024;
                self.pending_images.push(image);
                self.status_line = format!(
                    "📎 attached image {display_path} ({kib} KiB) — {} pending",
                    self.pending_images.len()
                );
            }
            None => {
                self.status_line = format!(
                    "Could not attach `{rest}` — needs to be an existing image file \
                    (png/jpg/jpeg/gif/webp/bmp/tiff/heic) under {} MB.",
                    MAX_IMAGE_ATTACHMENT_BYTES / (1024 * 1024)
                );
            }
        }
        true
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<bool> {
        if self.page == Page::Settings && self.settings_text_editor.is_some() {
            self.handle_settings_text_editor_key(key_event)?;
            return Ok(false);
        }
        if key_event.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key_event.code, KeyCode::Char('c') | KeyCode::Char('q'))
        {
            return Ok(true);
        }
        if key_event.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key_event.code, KeyCode::Char('s'))
        {
            if self.page == Page::Settings && self.settings_view == SettingsView::Secrets {
                self.save_current_secret()?;
            } else {
                self.save_settings()?;
            }
            return Ok(false);
        }
        // Ctrl-X on the Chat page clears any pending image attachments.
        // Non-destructive to typed text — only drops queued images.
        if self.page == Page::Chat
            && key_event.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key_event.code, KeyCode::Char('x'))
            && !self.pending_images.is_empty()
        {
            let cleared = self.pending_images.len();
            self.pending_images.clear();
            self.status_line = format!("Cleared {cleared} pending image attachment(s).");
            return Ok(false);
        }
        // Ctrl-I on the Chat page: try to paste an image from the system
        // clipboard. Uses pbpaste / wl-paste / xclip under the hood so we
        // don't pull in arboard. On terminals that already deliver image
        // content via bracketed paste, handle_paste handles the file-path
        // case; Ctrl-I handles the raw-image case (e.g. macOS screenshot
        // clipboard, Cmd+Ctrl+Shift+4, Snipping Tool equivalents).
        if self.page == Page::Chat
            && key_event.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key_event.code, KeyCode::Char('i'))
        {
            match capture_clipboard_image_to_tempfile() {
                Some(path) => {
                    let resolved =
                        try_resolve_image_attachment(path.to_str().unwrap_or_default(), &self.root);
                    match resolved {
                        Some(image) => {
                            let display = image.path.display().to_string();
                            let kib = image.size_bytes / 1024;
                            self.pending_images.push(image);
                            self.status_line = format!(
                                "📎 pasted clipboard image → {display} ({kib} KiB) — {} pending",
                                self.pending_images.len()
                            );
                        }
                        None => {
                            self.status_line =
                                "Clipboard image captured but validation failed.".to_string();
                        }
                    }
                }
                None => {
                    self.status_line = "No PNG image on the clipboard (or platform paste tool missing). Use /image <path> or drag-and-drop.".to_string();
                }
            }
            return Ok(false);
        }

        match key_event.code {
            KeyCode::Tab => {
                self.settings_menu_open = false;
                match self.page {
                    Page::Chat => self.page = Page::Skills,
                    Page::Skills => self.page = Page::Costs,
                    Page::Costs => {
                        self.page = Page::Settings;
                        self.switch_settings_view(SettingsView::Model);
                    }
                    Page::Settings => {
                        if self.settings_view == SettingsView::HarnessFlow {
                            self.page = Page::Chat;
                            self.switch_settings_view(SettingsView::Model);
                        } else {
                            self.switch_settings_view(next_settings_view(self.settings_view));
                        }
                    }
                }
                return Ok(false);
            }
            KeyCode::BackTab => {
                self.settings_menu_open = false;
                match self.page {
                    Page::Chat => {
                        self.page = Page::Settings;
                        self.switch_settings_view(SettingsView::HarnessFlow);
                    }
                    Page::Skills => self.page = Page::Chat,
                    Page::Costs => self.page = Page::Skills,
                    Page::Settings => {
                        if self.settings_view == SettingsView::Model {
                            self.page = Page::Costs;
                        } else {
                            self.switch_settings_view(previous_settings_view(self.settings_view));
                        }
                    }
                }
                return Ok(false);
            }
            _ => {}
        }

        match self.page {
            Page::Chat => self.handle_chat_key(key_event)?,
            Page::Skills => self.handle_skills_key(key_event),
            Page::Costs => {}
            Page::Settings => self.handle_settings_key(key_event)?,
        }

        Ok(false)
    }

    fn handle_chat_key(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Enter => self.submit_chat_request()?,
            KeyCode::Backspace => {
                self.chat_input.pop();
            }
            KeyCode::Char(ch) if !key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.chat_input.push(ch);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_settings_key(&mut self, key_event: KeyEvent) -> Result<()> {
        if self.settings_view == SettingsView::Update {
            return self.handle_update_view_key(key_event);
        }
        if self.settings_view == SettingsView::Secrets {
            return self.handle_secrets_key(key_event);
        }
        if self.settings_view == SettingsView::HarnessFlow {
            return self.handle_harness_flow_key(key_event);
        }
        if self.settings_menu_open {
            match key_event.code {
                KeyCode::Up => self.move_settings_menu(-1),
                KeyCode::Down => self.move_settings_menu(1),
                KeyCode::Enter => self.commit_settings_menu_choice()?,
                KeyCode::Esc | KeyCode::Left => self.settings_menu_open = false,
                _ => {}
            }
            return Ok(());
        }
        match key_event.code {
            KeyCode::Up => {
                self.move_settings_selection(-1);
            }
            KeyCode::Down => {
                self.move_settings_selection(1);
            }
            KeyCode::Char('[') | KeyCode::PageUp => {
                self.switch_settings_view(previous_settings_view(self.settings_view))
            }
            KeyCode::Char(']') | KeyCode::PageDown => {
                self.switch_settings_view(next_settings_view(self.settings_view))
            }
            KeyCode::Left if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.switch_settings_view(previous_settings_view(self.settings_view));
            }
            KeyCode::Right if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.switch_settings_view(next_settings_view(self.settings_view));
            }
            KeyCode::Left => self.cycle_setting(false)?,
            KeyCode::Right => self.cycle_setting(true)?,
            KeyCode::Backspace => {
                if let Some(item) = self.current_setting_mut() {
                    if item.kind == SettingKind::Env {
                        item.value.pop();
                        self.settings_dirty = true;
                        self.refresh_dynamic_setting_choices();
                    }
                }
            }
            KeyCode::Enter => self.activate_selected_setting()?,
            KeyCode::Char(ch) if !key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(item) = self.current_setting_mut() {
                    if item.kind == SettingKind::Env && item.choices.is_empty() {
                        item.value.push(ch);
                        self.settings_dirty = true;
                        self.refresh_dynamic_setting_choices();
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_settings_text_editor_key(&mut self, key_event: KeyEvent) -> Result<()> {
        let Some(editor_state) = self.settings_text_editor.as_mut() else {
            return Ok(());
        };
        match editor_state.editor.handle_key(key_event) {
            EditorAction::Continue => {}
            EditorAction::Exit(ExitReason::Cancelled) => {
                self.status_line = format!("Cancelled editing {}.", editor_state.label);
                self.settings_text_editor = None;
            }
            EditorAction::Exit(ExitReason::Saved) => {
                let key = editor_state.key;
                let label = editor_state.label;
                let text = editor_state.editor.text();
                self.settings_text_editor = None;
                if let Some(item) = self.settings_items.iter_mut().find(|item| item.key == key) {
                    item.value = text;
                    self.settings_dirty = true;
                }
                self.refresh_dynamic_setting_choices();
                self.save_settings()?;
                self.status_line = format!("Saved {} to runtime state.", label);
            }
        }
        Ok(())
    }

    fn handle_skills_key(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up => self.move_skills_selection(-1),
            KeyCode::Down => self.move_skills_selection(1),
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.skill_catalog = load_skill_catalog(&self.root);
                if self.skills_selected >= self.skill_catalog.len() {
                    self.skills_selected = self.skill_catalog.len().saturating_sub(1);
                }
                self.status_line = format!("Reloaded {} skill entries.", self.skill_catalog.len());
            }
            _ => {}
        }
    }

    fn handle_secrets_key(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Up => self.move_secrets_selection(-1),
            KeyCode::Down => self.move_secrets_selection(1),
            KeyCode::Backspace => {
                if let Some(item) = self.current_secret_mut() {
                    item.value.pop();
                }
            }
            KeyCode::Enter => self.save_current_secret()?,
            KeyCode::Char('r') | KeyCode::Char('R') => self.reload_secrets(),
            KeyCode::Char(ch) if !key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(item) = self.current_secret_mut() {
                    item.value.push(ch);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn cycle_setting(&mut self, forward: bool) -> Result<()> {
        let Some(item) = self.current_setting_mut() else {
            return Ok(());
        };
        if item.kind != SettingKind::Env || item.choices.is_empty() {
            return Ok(());
        }
        let current_index = item
            .choices
            .iter()
            .position(|choice| choice.eq_ignore_ascii_case(item.value.trim()))
            .unwrap_or(0);
        let next_index = if forward {
            (current_index + 1) % item.choices.len()
        } else if current_index == 0 {
            item.choices.len() - 1
        } else {
            current_index - 1
        };
        item.value = item.choices[next_index].to_string();
        self.settings_dirty = true;
        self.refresh_dynamic_setting_choices();
        Ok(())
    }

    fn activate_selected_setting(&mut self) -> Result<()> {
        match self.current_setting().map(|item| {
            (
                item.key,
                item.label,
                item.value.clone(),
                item.kind,
                self.setting_is_dirty(item),
                item.choices.clone(),
            )
        }) {
            Some((key, label, value, _, _, _)) if key == CTOX_CTO_OPERATING_MODE_KEY => {
                self.open_settings_text_editor(key, label, &value);
                Ok(())
            }
            Some((_, _, _, kind, dirty, _)) if kind == SettingKind::Env && dirty => {
                self.save_settings()
            }
            Some((_, _, value, kind, _, choices)) if kind == SettingKind::Env => {
                if choices.is_empty() {
                    Ok(())
                } else {
                    self.settings_menu_index = choices
                        .iter()
                        .position(|choice| choice.eq_ignore_ascii_case(value.trim()))
                        .unwrap_or(0);
                    self.settings_menu_open = true;
                    Ok(())
                }
            }
            Some((_, _, _, kind, _, _)) if kind == SettingKind::ServiceToggle => {
                self.toggle_service()
            }
            Some(_) => Ok(()),
            None => Ok(()),
        }
    }

    fn open_settings_text_editor(&mut self, key: &'static str, label: &'static str, value: &str) {
        self.settings_text_editor = Some(SettingsTextEditorState {
            key,
            label,
            editor: TextEditor::scratch(value),
        });
        self.status_line = format!("Editing {label}. Ctrl-X saves to runtime state, Esc cancels.");
    }

    fn toggle_service(&mut self) -> Result<()> {
        self.status_line = if self.service_status.running {
            service::stop_background(&self.root)?
        } else {
            service::start_background(&self.root)?
        };
        self.push_local_activity(self.status_line.clone());
        self.last_service_refresh_at = None;
        self.refresh()?;
        Ok(())
    }

    fn submit_chat_request(&mut self) -> Result<()> {
        let raw = self.chat_input.trim().to_string();

        // Slash-command for attaching images: intercepted before any
        // submission. Leaves chat_input intact so the user can continue
        // typing their actual prompt after attaching.
        if raw.starts_with("/image") || raw.starts_with("/img") {
            if self.handle_image_command(&raw) {
                self.chat_input.clear();
                return Ok(());
            }
        }

        if raw.is_empty() && self.pending_images.is_empty() {
            self.status_line = "Chat input is empty.".to_string();
            return Ok(());
        }
        if !self.service_status.running {
            self.status_line =
                "CTOX loop is not running. Start it in Settings or with `ctox start`.".to_string();
            return Ok(());
        }

        // Compose the outgoing prompt: for each pending image emit the
        // canonical ctox:image marker on its own line, then the user's
        // text.
        let mut prompt = String::new();
        for image in &self.pending_images {
            prompt.push_str(&vision_preprocessor::encode_image_marker(&image.path));
            prompt.push('\n');
        }
        if raw.is_empty() {
            // Attachments without narrative — give the model a sensible default.
            prompt.push_str("Please describe the attached image(s).");
        } else {
            prompt.push_str(&raw);
        }
        let prepared_prompt = service::prepare_chat_prompt(&self.root, &prompt)?;
        prompt = prepared_prompt.prompt;

        let attachment_count = self.pending_images.len();

        if self.request_in_flight {
            self.draft_queue.push_back(prompt.clone());
            self.chat_input.clear();
            self.pending_images.clear();
            self.status_line = format!(
                "Prompt queued locally. {} draft(s) waiting.",
                self.draft_queue.len()
            );
            self.push_local_activity(format!(
                "Queued local draft: {}",
                summarize_inline(&prompt, 72)
            ));
            return Ok(());
        }
        self.chat_input.clear();
        self.pending_images.clear();
        service::submit_chat_prompt(&self.root, &prompt)?;
        self.status_line = if attachment_count > 0 {
            format!("CTOX loop accepted the request (with {attachment_count} image attachment(s)).")
        } else {
            "CTOX loop accepted the request.".to_string()
        };
        self.push_local_activity(format!(
            "Submitted prompt: {}",
            summarize_inline(&prompt, 72)
        ));
        self.request_in_flight = true;
        Ok(())
    }

    fn poll_worker(&mut self) -> Result<()> {
        if let Some(receiver) = &self.runtime_switch_rx {
            match receiver.try_recv() {
                Ok(Ok(message)) => {
                    self.runtime_switch_in_flight = false;
                    self.runtime_switch_rx = None;
                    self.pending_runtime_transition_cards = None;
                    self.status_line = message;
                    self.invalidate_runtime_observations();
                    self.refresh()?;
                }
                Ok(Err(err)) => {
                    self.runtime_switch_in_flight = false;
                    self.runtime_switch_rx = None;
                    self.pending_runtime_transition_cards = None;
                    self.status_line = format!("Settings saved, but runtime switch failed: {err}");
                    self.invalidate_runtime_observations();
                    self.refresh()?;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.runtime_switch_in_flight = false;
                    self.runtime_switch_rx = None;
                    self.pending_runtime_transition_cards = None;
                    self.status_line =
                        "Settings saved, but runtime switch worker disappeared.".to_string();
                    self.invalidate_runtime_observations();
                }
            }
        }
        Ok(())
    }

    fn invalidate_runtime_observations(&mut self) {
        self.last_gpu_refresh_at = None;
        self.last_runtime_refresh_at = None;
        self.runtime_telemetry = None;
    }

    fn refresh_service_status(&mut self) {
        let previous = self.service_status.clone();
        self.service_status = service::service_status_snapshot(&self.root).unwrap_or_else(|_| {
            service::ServiceStatus {
                running: false,
                busy: false,
                pid: None,
                listen_addr: "127.0.0.1:12435".to_string(),
                autostart_enabled: false,
                manager: "process".to_string(),
                pending_count: 0,
                pending_previews: Vec::new(),
                blocked_count: 0,
                blocked_previews: Vec::new(),
                current_goal_preview: None,
                active_source_label: None,
                recent_events: Vec::new(),
                last_error: None,
                last_completed_at: None,
                last_reply_chars: None,
                monitor_last_check_at: None,
                monitor_alerts: Vec::new(),
                monitor_last_error: None,
                last_agent_outcome: None,
                work_hours: service::working_hours::snapshot(&self.root),
            }
        });
        self.request_in_flight = self.service_status.running && self.service_status.busy;
        if previous.busy && !self.service_status.busy {
            self.status_line = match self.service_status.last_error.as_deref() {
                Some(err) => format!("CTOX loop failed: {err}"),
                None => format!(
                    "CTOX loop completed reply{}.",
                    self.service_status
                        .last_reply_chars
                        .map(|count| format!(" with {count} chars"))
                        .unwrap_or_default()
                ),
            };
            if self.chat_input.trim().is_empty() {
                if let Some(next) = self.draft_queue.pop_front() {
                    self.chat_input = next;
                    self.push_local_activity("Moved next queued draft into composer".to_string());
                    self.status_line = format!("{} Draft ready in composer.", self.status_line);
                }
            }
        } else if !previous.running && self.service_status.running {
            self.status_line = format!(
                "CTOX loop connected at {}.",
                self.service_status.listen_addr
            );
        } else if previous.running && !self.service_status.running {
            self.status_line = "CTOX loop stopped.".to_string();
        }
        self.sync_activity_log();
    }

    fn service_summary(&self) -> String {
        let persist = if self.service_status.autostart_enabled {
            "autostart on"
        } else {
            "autostart off"
        };
        if self.service_status.running {
            if self.service_status.work_hours.enabled
                && !self.service_status.work_hours.inside_window
            {
                return format!(
                    "paused outside {}-{} ({}, {})",
                    self.service_status.work_hours.start,
                    self.service_status.work_hours.end,
                    self.service_status.manager,
                    persist
                );
            }
            let degraded = self.runtime_health.degraded_components();
            if !degraded.is_empty() {
                format!(
                    "degraded on {} ({} down, {}, {})",
                    self.service_status.listen_addr,
                    degraded.join("+"),
                    self.service_status.manager,
                    persist
                )
            } else if self.service_status.busy {
                format!(
                    "running on {} (busy, {}, {})",
                    self.service_status.listen_addr, self.service_status.manager, persist
                )
            } else {
                format!(
                    "running on {} (idle, {}, {})",
                    self.service_status.listen_addr, self.service_status.manager, persist
                )
            }
        } else {
            format!(
                "stopped ({}, {}, {})",
                self.service_status.listen_addr, self.service_status.manager, persist
            )
        }
    }

    fn rendered_setting_value(&self, item: &SettingItem) -> String {
        match item.kind {
            SettingKind::Env => {
                let preview_value = if item.key == CTOX_CTO_OPERATING_MODE_KEY {
                    let line_count = item.value.lines().count().max(1);
                    if item.value.trim().is_empty() {
                        "(default CTO contract)".to_string()
                    } else {
                        format!("{line_count} lines configured")
                    }
                } else if item.secret && !item.value.trim().is_empty() {
                    mask_secret(&item.value)
                } else if item.value.trim().is_empty() {
                    "(empty)".to_string()
                } else {
                    item.value.clone()
                };
                let mut rendered = preview_value;
                if self.setting_is_dirty(item) {
                    rendered.push_str(" *");
                }
                rendered
            }
            SettingKind::ServiceToggle => self.service_summary(),
        }
    }

    #[allow(dead_code)]
    fn selected_setting_help(&self) -> String {
        self.current_setting()
            .map(|item| match item.kind {
                SettingKind::Env => {
                    let mut lines = vec![item.help.to_string()];
                    if item.key == CTOX_CTO_OPERATING_MODE_KEY {
                        lines.push(
                            "Enter opens the full-screen text editor. Ctrl-X saves.".to_string(),
                        );
                    } else if self.setting_is_dirty(item) {
                        lines.push("Pending change. Enter saves it.".to_string());
                    } else if !item.choices.is_empty() {
                        lines.push("Enter opens the choice menu.".to_string());
                    }
                    lines.join("\n")
                }
                SettingKind::ServiceToggle => {
                    let action = if self.service_status.running {
                        "stop"
                    } else {
                        "start"
                    };
                    format!(
                        "loop {}\n{}\nEnter to {}\nCtrl-C quits CTOX.",
                        if self.service_status.running {
                            "up"
                        } else {
                            "down"
                        },
                        self.service_summary(),
                        action
                    )
                }
            })
            .unwrap_or_else(|| "No setting selected.".to_string())
    }

    fn settings_env_map(&self) -> BTreeMap<String, String> {
        settings_map_from_items(&self.settings_items)
    }

    fn saved_settings_env_map(&self) -> BTreeMap<String, String> {
        settings_map_from_items(
            &self
                .settings_items
                .iter()
                .map(|item| SettingItem {
                    key: item.key,
                    label: item.label,
                    value: item.saved_value.clone(),
                    saved_value: item.saved_value.clone(),
                    secret: item.secret,
                    choices: item.choices.clone(),
                    help: item.help,
                    kind: item.kind,
                })
                .collect::<Vec<_>>(),
        )
    }

    fn visible_setting_indices(&self) -> Vec<usize> {
        self.settings_items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                (self.setting_visible(item) && self.setting_in_view(item)).then_some(idx)
            })
            .collect()
    }

    fn setting_in_view(&self, item: &SettingItem) -> bool {
        match self.settings_view {
            SettingsView::Model => matches!(
                item.key,
                "CTOX_SERVICE_TOGGLE"
                    | "CTOX_WORK_HOURS_ENABLED"
                    | "CTOX_WORK_HOURS_START"
                    | "CTOX_WORK_HOURS_END"
                    | "CTOX_API_PROVIDER"
                    | "CTOX_OPENAI_AUTH_MODE"
                    | "CTOX_AZURE_FOUNDRY_ENDPOINT"
                    | "CTOX_AZURE_FOUNDRY_DEPLOYMENT_ID"
                    | "AZURE_FOUNDRY_API_KEY"
                    | "OPENAI_API_KEY"
                    | "ANTHROPIC_API_KEY"
                    | "OPENROUTER_API_KEY"
                    | "CTOX_CHAT_MODEL_FAMILY"
                    | "CTOX_CHAT_LOCAL_PRESET"
                    | "CTOX_CHAT_MODEL_MAX_CONTEXT"
                    | "CTOX_CHAT_TURN_TIMEOUT_SECS"
                    | "CTOX_CHAT_SKILL_PRESET"
                    | "CTOX_REFRESH_OUTPUT_BUDGET_PCT"
                    | "CTOX_AUTONOMY_LEVEL"
                    | "CTOX_CTO_OPERATING_MODE_PROMPT"
                    | "CTOX_CHAT_MODEL"
                    | "CTOX_CHAT_MODEL_BOOST"
                    | "CTOX_BOOST_DEFAULT_MINUTES"
                    | "CTOX_EMBEDDING_MODEL"
                    | "CTOX_STT_MODEL"
                    | "CTOX_TTS_MODEL"
                    | "CTOX_USE_DIRECT_SESSION"
                    | "CTOX_COMPACT_TRIGGER"
                    | "CTOX_COMPACT_MODE"
                    | "CTOX_COMPACT_FIXED_INTERVAL"
                    | "CTOX_COMPACT_ADAPTIVE_THRESHOLD"
            ),
            SettingsView::Communication => matches!(
                item.key,
                "CTOX_SERVICE_TOGGLE"
                    | "CTOX_OWNER_NAME"
                    | "CTOX_OWNER_EMAIL_ADDRESS"
                    | "CTOX_FOUNDER_EMAIL_ADDRESSES"
                    | "CTOX_FOUNDER_EMAIL_ROLES"
                    | "CTOX_ALLOWED_EMAIL_DOMAIN"
                    | "CTOX_EMAIL_ADMIN_POLICIES"
                    | "CTOX_OWNER_PREFERRED_CHANNEL"
                    | "CTOX_REMOTE_BRIDGE_MODE"
                    | "CTOX_WEBRTC_SIGNALING_URL"
                    | "CTOX_WEBRTC_ROOM"
                    | "CTOX_WEBRTC_PASSWORD"
                    | "CTO_EMAIL_ADDRESS"
                    | "CTO_EMAIL_PASSWORD"
                    | "CTO_EMAIL_PROVIDER"
                    | "CTO_EMAIL_IMAP_HOST"
                    | "CTO_EMAIL_IMAP_PORT"
                    | "CTO_EMAIL_SMTP_HOST"
                    | "CTO_EMAIL_SMTP_PORT"
                    | "CTO_EMAIL_GRAPH_USER"
                    | "CTO_EMAIL_EWS_URL"
                    | "CTO_EMAIL_EWS_AUTH_TYPE"
                    | "CTO_EMAIL_EWS_USERNAME"
                    | "CTO_JAMI_ACCOUNT_ID"
                    | "CTO_JAMI_PROFILE_NAME"
                    | "CTO_WHATSAPP_DEVICE_DB"
                    | "CTO_WHATSAPP_PUSH_NAME"
                    | "CTO_WHATSAPP_SYNC_TIMEOUT_SECONDS"
                    | "CTO_TEAMS_USERNAME"
                    | "CTO_TEAMS_PASSWORD"
                    | "CTO_TEAMS_TENANT_ID"
                    | "CTO_TEAMS_TEAM_ID"
                    | "CTO_TEAMS_CHANNEL_ID"
            ),
            SettingsView::Secrets => false,
            SettingsView::Paths => matches!(
                item.key,
                "CTOX_INSTALL_ROOT"
                    | "CTOX_STATE_ROOT"
                    | "CTOX_CACHE_ROOT"
                    | "CTOX_BIN_DIR"
                    | "CTOX_SKILLS_ROOT"
                    | "CTOX_GENERATED_SKILLS_ROOT"
                    | "CTOX_TOOLS_ROOT"
                    | "CTOX_DEPENDENCIES_ROOT"
            ),
            SettingsView::Update => false,
            SettingsView::BusinessOs => false,
            SettingsView::HarnessMining => false,
            SettingsView::HarnessFlow => false,
        }
    }

    fn setting_visible(&self, item: &SettingItem) -> bool {
        if item.kind == SettingKind::ServiceToggle {
            return true;
        }
        let api_provider = self
            .value_for_setting("CTOX_API_PROVIDER")
            .unwrap_or(DEFAULT_API_PROVIDER);
        let local_runtime =
            !infer_chat_source(&self.settings_env_map()).eq_ignore_ascii_case("api");
        let local_runtime_is_candle = self
            .value_for_setting("CTOX_LOCAL_RUNTIME")
            .unwrap_or(DEFAULT_LOCAL_RUNTIME)
            .eq_ignore_ascii_case("candle");
        match item.key {
            "CTOX_WORK_HOURS_ENABLED" | "CTOX_WORK_HOURS_START" | "CTOX_WORK_HOURS_END" => true,
            "CTOX_CHAT_MODEL_BOOST"
            | "CTOX_BOOST_DEFAULT_MINUTES"
            | "CTOX_EMBEDDING_MODEL"
            | "CTOX_STT_MODEL"
            | "CTOX_TTS_MODEL"
            | "CTOX_USE_DIRECT_SESSION"
            | "CTOX_COMPACT_TRIGGER"
            | "CTOX_COMPACT_MODE"
            | "CTOX_COMPACT_FIXED_INTERVAL"
            | "CTOX_COMPACT_ADAPTIVE_THRESHOLD"
            | "CTOX_OWNER_NAME"
            | "CTOX_OWNER_EMAIL_ADDRESS"
            | "CTOX_FOUNDER_EMAIL_ADDRESSES"
            | "CTOX_FOUNDER_EMAIL_ROLES"
            | "CTOX_ALLOWED_EMAIL_DOMAIN"
            | "CTOX_EMAIL_ADMIN_POLICIES"
            | "CTOX_OWNER_PREFERRED_CHANNEL"
            | "CTOX_REMOTE_BRIDGE_MODE" => true,
            "CTOX_WEBRTC_SIGNALING_URL" | "CTOX_WEBRTC_ROOM" | "CTOX_WEBRTC_PASSWORD" => self
                .value_for_setting("CTOX_REMOTE_BRIDGE_MODE")
                .unwrap_or(DEFAULT_REMOTE_BRIDGE_MODE)
                .eq_ignore_ascii_case("remote-webrtc"),
            "CTOX_API_PROVIDER" => true,
            "CTOX_OPENAI_AUTH_MODE" => api_provider.eq_ignore_ascii_case("openai"),
            "OPENAI_API_KEY" => {
                api_provider.eq_ignore_ascii_case("openai")
                    && !self
                        .value_for_setting(OPENAI_AUTH_MODE_KEY)
                        .unwrap_or(DEFAULT_OPENAI_AUTH_MODE)
                        .eq_ignore_ascii_case("chatgpt_subscription")
            }
            "ANTHROPIC_API_KEY" => api_provider.eq_ignore_ascii_case("anthropic"),
            "OPENROUTER_API_KEY" => api_provider.eq_ignore_ascii_case("openrouter"),
            "AZURE_FOUNDRY_API_KEY"
            | "CTOX_AZURE_FOUNDRY_ENDPOINT"
            | "CTOX_AZURE_FOUNDRY_DEPLOYMENT_ID" => {
                api_provider.eq_ignore_ascii_case("azure_foundry")
            }
            "CTOX_LOCAL_RUNTIME" => local_runtime,
            "CTOX_CHAT_MODEL_FAMILY" => false,
            "CTOX_CHAT_MODEL" => !api_provider.eq_ignore_ascii_case("azure_foundry"),
            "CTOX_CHAT_LOCAL_PRESET" => !local_runtime || local_runtime_is_candle,
            "CTOX_CHAT_MODEL_MAX_CONTEXT" => local_runtime,
            "CTOX_CHAT_SKILL_PRESET" => true,
            "CTOX_REFRESH_OUTPUT_BUDGET_PCT" => true,
            "CTOX_AUTONOMY_LEVEL" => true,
            "CTOX_CTO_OPERATING_MODE_PROMPT" => true,
            "CTO_EMAIL_ADDRESS" | "CTO_EMAIL_PASSWORD" | "CTO_EMAIL_PROVIDER" => self
                .value_for_setting("CTOX_OWNER_PREFERRED_CHANNEL")
                .unwrap_or(DEFAULT_COMMUNICATION_PATH)
                .eq_ignore_ascii_case("email"),
            "CTO_EMAIL_IMAP_HOST"
            | "CTO_EMAIL_IMAP_PORT"
            | "CTO_EMAIL_SMTP_HOST"
            | "CTO_EMAIL_SMTP_PORT" => {
                self.value_for_setting("CTOX_OWNER_PREFERRED_CHANNEL")
                    .unwrap_or(DEFAULT_COMMUNICATION_PATH)
                    .eq_ignore_ascii_case("email")
                    && self
                        .value_for_setting("CTO_EMAIL_PROVIDER")
                        .unwrap_or("imap")
                        .eq_ignore_ascii_case("imap")
            }
            "CTO_EMAIL_GRAPH_USER" => {
                self.value_for_setting("CTOX_OWNER_PREFERRED_CHANNEL")
                    .unwrap_or(DEFAULT_COMMUNICATION_PATH)
                    .eq_ignore_ascii_case("email")
                    && self
                        .value_for_setting("CTO_EMAIL_PROVIDER")
                        .unwrap_or("imap")
                        .eq_ignore_ascii_case("graph")
            }
            "CTO_EMAIL_EWS_URL" | "CTO_EMAIL_EWS_AUTH_TYPE" | "CTO_EMAIL_EWS_USERNAME" => {
                self.value_for_setting("CTOX_OWNER_PREFERRED_CHANNEL")
                    .unwrap_or(DEFAULT_COMMUNICATION_PATH)
                    .eq_ignore_ascii_case("email")
                    && self
                        .value_for_setting("CTO_EMAIL_PROVIDER")
                        .unwrap_or("imap")
                        .eq_ignore_ascii_case("ews")
            }
            // Jami fields are always visible so the QR code can be used to add
            // the agent as a contact regardless of the preferred reply channel.
            "CTO_JAMI_ACCOUNT_ID" | "CTO_JAMI_PROFILE_NAME" => true,
            "CTO_WHATSAPP_DEVICE_DB"
            | "CTO_WHATSAPP_PUSH_NAME"
            | "CTO_WHATSAPP_SYNC_TIMEOUT_SECONDS" => self
                .value_for_setting("CTOX_OWNER_PREFERRED_CHANNEL")
                .unwrap_or(DEFAULT_COMMUNICATION_PATH)
                .eq_ignore_ascii_case("whatsapp"),
            "CTO_TEAMS_USERNAME"
            | "CTO_TEAMS_PASSWORD"
            | "CTO_TEAMS_TENANT_ID"
            | "CTO_TEAMS_TEAM_ID"
            | "CTO_TEAMS_CHANNEL_ID" => self
                .value_for_setting("CTOX_OWNER_PREFERRED_CHANNEL")
                .unwrap_or(DEFAULT_COMMUNICATION_PATH)
                .eq_ignore_ascii_case("teams"),
            _ => false,
        }
    }

    fn value_for_setting(&self, key: &str) -> Option<&str> {
        self.settings_items
            .iter()
            .find(|item| item.key == key)
            .map(|item| item.value.trim())
            .filter(|value| !value.is_empty())
    }

    fn current_setting(&self) -> Option<&SettingItem> {
        self.settings_items.get(self.settings_selected)
    }

    fn current_setting_mut(&mut self) -> Option<&mut SettingItem> {
        self.settings_items.get_mut(self.settings_selected)
    }

    fn setting_is_dirty(&self, item: &SettingItem) -> bool {
        item.value.trim() != item.saved_value.trim()
    }

    fn current_secret(&self) -> Option<&SecretItem> {
        self.secret_items.get(self.secrets_selected)
    }

    fn current_secret_mut(&mut self) -> Option<&mut SecretItem> {
        self.secret_items.get_mut(self.secrets_selected)
    }

    fn secret_is_dirty(&self, item: &SecretItem) -> bool {
        item.value != item.saved_value
    }

    fn move_secrets_selection(&mut self, delta: isize) {
        if self.secret_items.is_empty() {
            self.secrets_selected = 0;
            return;
        }
        let next = if delta.is_negative() {
            self.secrets_selected.saturating_sub(delta.unsigned_abs())
        } else {
            (self.secrets_selected + delta as usize).min(self.secret_items.len().saturating_sub(1))
        };
        self.secrets_selected = next;
    }

    fn reload_secrets(&mut self) {
        self.secret_items = load_secret_items(&self.root);
        if self.secrets_selected >= self.secret_items.len() {
            self.secrets_selected = self.secret_items.len().saturating_sub(1);
        }
        self.status_line = if self.secret_items.is_empty() {
            "Reloaded secret store. No encrypted secrets present.".to_string()
        } else {
            format!(
                "Reloaded {} encrypted secret record(s).",
                self.secret_items.len()
            )
        };
    }

    fn move_settings_selection(&mut self, delta: isize) {
        let visible = self.visible_setting_indices();
        if visible.is_empty() {
            return;
        }
        let current_pos = visible
            .iter()
            .position(|idx| *idx == self.settings_selected)
            .unwrap_or(0);
        let next_pos = if delta.is_negative() {
            current_pos.saturating_sub(delta.unsigned_abs())
        } else {
            (current_pos + delta as usize).min(visible.len().saturating_sub(1))
        };
        self.settings_selected = visible[next_pos];
    }

    fn switch_settings_view(&mut self, view: SettingsView) {
        if self.settings_view == view {
            return;
        }
        self.settings_view = view;
        if view == SettingsView::Secrets {
            self.reload_secrets();
        } else if let Some(first) = self.visible_setting_indices().first().copied() {
            self.settings_selected = first;
        }
        if view == SettingsView::Update {
            self.refresh_update_view_info();
        } else if view == SettingsView::HarnessFlow {
            self.harness_flow_scroll = 0;
            self.refresh_harness_flow();
        }
    }

    fn refresh_update_view_info(&mut self) {
        match crate::install::version_info(&self.root) {
            Ok(info) => {
                self.update_view.info_json =
                    serde_json::to_string_pretty(&info).unwrap_or_default();
            }
            Err(err) => {
                self.update_view.info_json = format!("error: {err}");
            }
        }
    }

    fn handle_update_view_key(&mut self, key_event: KeyEvent) -> Result<()> {
        if let Some(pending) = self.update_view.pending {
            match key_event.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.update_view.pending = None;
                    match pending {
                        PendingUpdateAction::Upgrade => {
                            self.update_view.last_action_line =
                                "running `ctox upgrade` — service will restart…".to_string();
                            match self.run_update_subprocess(&["upgrade"]) {
                                Ok(output) => {
                                    self.update_view.check_json = output;
                                    self.refresh_update_view_info();
                                    self.update_view.last_action_line = format!(
                                        "upgrade completed at {}",
                                        chrono::Local::now().format("%H:%M:%S")
                                    );
                                }
                                Err(err) => {
                                    self.update_view.last_action_line =
                                        format!("upgrade failed: {err}");
                                }
                            }
                        }
                        PendingUpdateAction::EngineRebuild => {
                            self.update_view.last_action_line =
                                "running `ctox engine rebuild` — this can take several minutes…"
                                    .to_string();
                            match self.run_update_subprocess(&["engine", "rebuild"]) {
                                Ok(output) => {
                                    self.update_view.check_json = output;
                                    self.update_view.last_action_line = format!(
                                        "engine rebuild completed at {}",
                                        chrono::Local::now().format("%H:%M:%S")
                                    );
                                }
                                Err(err) => {
                                    self.update_view.last_action_line =
                                        format!("engine rebuild failed: {err}");
                                }
                            }
                        }
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.update_view.pending = None;
                    self.update_view.last_action_line = "cancelled".to_string();
                }
                _ => {}
            }
            return Ok(());
        }
        match key_event.code {
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.update_view.last_action_line = "Running `ctox update check`…".to_string();
                match self.run_update_subprocess(&["update", "check"]) {
                    Ok(output) => {
                        self.update_view.check_json = output;
                        self.update_view.last_action_line = format!(
                            "check completed at {}",
                            chrono::Local::now().format("%H:%M:%S")
                        );
                    }
                    Err(err) => {
                        self.update_view.last_action_line = format!("check failed: {err}");
                    }
                }
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                self.update_view.pending = Some(PendingUpdateAction::Upgrade);
                self.update_view.last_action_line =
                    "press [y] to confirm upgrade (service will restart) or [n] to cancel"
                        .to_string();
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                self.update_view.pending = Some(PendingUpdateAction::EngineRebuild);
                self.update_view.last_action_line =
                    "press [y] to confirm engine rebuild (minutes long) or [n] to cancel"
                        .to_string();
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                match self.run_update_subprocess(&["doctor"]) {
                    Ok(output) => {
                        self.update_view.check_json = output;
                        self.update_view.last_action_line = "doctor report ready".to_string();
                    }
                    Err(err) => {
                        self.update_view.last_action_line = format!("doctor failed: {err}");
                    }
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.refresh_update_view_info();
                self.update_view.last_action_line = "status refreshed".to_string();
            }
            _ => {}
        }
        Ok(())
    }

    fn run_update_subprocess(&self, args: &[&str]) -> Result<String> {
        let exe = std::env::current_exe().context("failed to resolve current ctox executable")?;
        let output = std::process::Command::new(exe)
            .args(args)
            .env("CTOX_ROOT", &self.root)
            .output()
            .context("failed to spawn ctox subprocess")?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if output.status.success() {
            Ok(stdout)
        } else {
            anyhow::bail!("{}{}", stdout, stderr.trim())
        }
    }

    fn refresh_dynamic_setting_choices(&mut self) {
        self.sync_provider_selection();
        let env_map = self.settings_env_map();
        let root = self.root.clone();
        for item in &mut self.settings_items {
            match item.key {
                "CTOX_CHAT_MODEL" => {
                    item.choices = supported_chat_model_choices(&root, &env_map);
                    if !item.choices.is_empty() && !choice_contains(&item.choices, &item.value) {
                        item.value = item.choices[0].to_string();
                    }
                }
                "CTOX_CHAT_MODEL_FAMILY" => {
                    item.choices = supported_local_chat_family_choices(&root, &env_map);
                    if !item.choices.is_empty() && !choice_contains(&item.choices, &item.value) {
                        item.value = item.choices[0].to_string();
                    }
                }
                "CTOX_CHAT_MODEL_BOOST" => {
                    item.choices = supported_boost_model_choices(&root, &env_map);
                    if !item.value.trim().is_empty()
                        && !item.choices.is_empty()
                        && !choice_contains(&item.choices, &item.value)
                    {
                        item.value = item.choices[0].to_string();
                    }
                }
                "CTOX_API_PROVIDER" => {
                    item.choices = API_PROVIDER_CHOICES.to_vec();
                    if !item.choices.is_empty() && !choice_contains(&item.choices, &item.value) {
                        item.value = item.choices[0].to_string();
                    }
                }
                "CTOX_WORK_HOURS_ENABLED" => {
                    item.choices = WORK_HOURS_CHOICES.to_vec();
                    if !item.choices.is_empty() && !choice_contains(&item.choices, &item.value) {
                        item.value = item.choices[0].to_string();
                    }
                }
                "CTOX_OPENAI_AUTH_MODE" => {
                    item.choices = OPENAI_AUTH_MODE_CHOICES.to_vec();
                    if !item.choices.is_empty() && !choice_contains(&item.choices, &item.value) {
                        item.value = item.choices[0].to_string();
                    }
                }
                "CTOX_LOCAL_RUNTIME" => {
                    item.choices = LOCAL_RUNTIME_CHOICES.to_vec();
                    if !item.choices.is_empty() && !choice_contains(&item.choices, &item.value) {
                        item.value = item.choices[0].to_string();
                    }
                }
                "CTOX_CHAT_LOCAL_PRESET" => {
                    item.choices = runtime_plan::chat_preset_choices();
                    if !item.choices.is_empty() && !choice_contains(&item.choices, &item.value) {
                        item.value = item.choices[0].to_string();
                    }
                }
                "CTOX_CHAT_MODEL_MAX_CONTEXT" => {
                    item.choices = runtime_plan::supported_chat_context_choices();
                    if !item.choices.is_empty() && !choice_contains(&item.choices, &item.value) {
                        item.value = item.choices[0].to_string();
                    }
                }
                "CTOX_CHAT_SKILL_PRESET" => {
                    item.choices = CHAT_SKILL_PRESET_CHOICES.to_vec();
                    if !item.choices.is_empty() && !choice_contains(&item.choices, &item.value) {
                        item.value = item.choices[0].to_string();
                    }
                }
                _ => {}
            }
        }
    }

    fn sync_provider_selection(&mut self) {
        let provider = self
            .settings_items
            .iter()
            .find(|item| item.key == "CTOX_API_PROVIDER")
            .map(|item| runtime_state::normalize_api_provider(&item.value))
            .unwrap_or(DEFAULT_API_PROVIDER);
        let current_model = self
            .settings_items
            .iter()
            .find(|item| item.key == "CTOX_CHAT_MODEL")
            .map(|item| item.value.trim().to_string())
            .unwrap_or_default();
        let azure_deployment_id = self
            .settings_items
            .iter()
            .find(|item| item.key == AZURE_FOUNDRY_DEPLOYMENT_ID_KEY)
            .map(|item| item.value.trim().to_string())
            .unwrap_or_default();
        let current_source = self
            .settings_items
            .iter()
            .find(|item| item.key == "CTOX_CHAT_SOURCE")
            .map(|item| item.value.trim().to_string())
            .unwrap_or_else(|| DEFAULT_CHAT_SOURCE.to_string());
        let use_api_source = !provider.eq_ignore_ascii_case("local")
            && (current_source.eq_ignore_ascii_case("api")
                || engine::api_provider_supports_model(provider, &current_model)
                || (provider.eq_ignore_ascii_case("azure_foundry")
                    && !azure_deployment_id.trim().is_empty()));
        if let Some(item) = self
            .settings_items
            .iter_mut()
            .find(|item| item.key == "CTOX_API_PROVIDER")
        {
            item.value = provider.to_string();
        }
        if let Some(item) = self
            .settings_items
            .iter_mut()
            .find(|item| item.key == "CTOX_CHAT_SOURCE")
        {
            item.value = if use_api_source {
                "api".to_string()
            } else {
                "local".to_string()
            };
        }
        if provider.eq_ignore_ascii_case("azure_foundry") && !azure_deployment_id.is_empty() {
            if let Some(item) = self
                .settings_items
                .iter_mut()
                .find(|item| item.key == "CTOX_CHAT_MODEL")
            {
                item.value = azure_deployment_id;
            }
        }
    }

    fn refresh(&mut self) -> Result<()> {
        self.refresh_dynamic_setting_choices();
        let visible = self.visible_setting_indices();
        if let Some(first) = visible.first().copied() {
            if !visible.contains(&self.settings_selected) {
                self.settings_selected = first;
            }
        }
        self.refresh_service_status_if_due();
        let chat_refresh_interval = self.chat_refresh_interval();
        if self.should_refresh_chat_messages()
            && refresh_due(&mut self.last_chat_refresh_at, chat_refresh_interval)
        {
            if let Err(err) = self.refresh_chat_messages() {
                self.status_line = format!(
                    "LCM refresh error: {}",
                    summarize_inline(&err.to_string(), 96)
                );
            }
        }
        let communication_refresh_interval = self.communication_refresh_interval();
        if self.should_refresh_communication_feed()
            && refresh_due(
                &mut self.last_communication_refresh_at,
                communication_refresh_interval,
            )
        {
            self.refresh_communication_feed();
        }
        if self.page == Page::Skills
            && refresh_due(
                &mut self.last_skill_catalog_refresh_at,
                SKILL_REFRESH_INTERVAL_ACTIVE,
            )
        {
            self.refresh_skill_catalog();
        }
        self.refresh_harness_flow_if_due();
        self.refresh_gpu_cards();
        self.refresh_runtime_telemetry_if_due();
        self.refresh_header();
        self.refresh_jami_qr();
        Ok(())
    }

    fn refresh_service_status_if_due(&mut self) {
        let service_refresh_interval = self.service_refresh_interval();
        if !refresh_due(&mut self.last_service_refresh_at, service_refresh_interval) {
            return;
        }
        self.refresh_service_status();
    }

    fn refresh_runtime_telemetry_if_due(&mut self) {
        let proxy_refresh_interval = self.proxy_refresh_interval();
        if !refresh_due(&mut self.last_runtime_refresh_at, proxy_refresh_interval) {
            return;
        }
        self.runtime_telemetry = load_runtime_telemetry(&self.root).ok().flatten();
    }

    fn refresh_skill_catalog(&mut self) {
        let refreshed = load_skill_catalog(&self.root);
        if refreshed.len() != self.skill_catalog.len()
            || refreshed
                .iter()
                .zip(self.skill_catalog.iter())
                .any(|(left, right)| {
                    left.skill_path != right.skill_path
                        || left.description != right.description
                        || left.helper_tools != right.helper_tools
                        || left.resources != right.resources
                })
        {
            self.skill_catalog = refreshed;
            if self.skills_selected >= self.skill_catalog.len() {
                self.skills_selected = self.skill_catalog.len().saturating_sub(1);
            }
        }
    }

    fn refresh_harness_flow_if_due(&mut self) {
        if self.page != Page::Settings || self.settings_view != SettingsView::HarnessFlow {
            return;
        }
        if !refresh_due(
            &mut self.last_harness_flow_refresh_at,
            HARNESS_FLOW_REFRESH_INTERVAL,
        ) {
            return;
        }
        self.refresh_harness_flow();
    }

    fn refresh_harness_flow(&mut self) {
        self.harness_flow_text = match service::harness_flow::render_latest_ascii(&self.root, 132) {
            Ok(text) => text,
            Err(err) => format!(
                "Harness flow unavailable.\n\n{}",
                summarize_inline(&err.to_string(), 140)
            ),
        };
    }

    fn move_skills_selection(&mut self, delta: isize) {
        if self.skill_catalog.is_empty() {
            self.skills_selected = 0;
            return;
        }
        let next = if delta.is_negative() {
            self.skills_selected.saturating_sub(delta.unsigned_abs())
        } else {
            (self.skills_selected + delta as usize).min(self.skill_catalog.len().saturating_sub(1))
        };
        self.skills_selected = next;
    }

    fn handle_harness_flow_key(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Up => self.harness_flow_scroll = self.harness_flow_scroll.saturating_sub(1),
            KeyCode::Down => self.harness_flow_scroll = self.harness_flow_scroll.saturating_add(1),
            KeyCode::PageUp => {
                self.harness_flow_scroll = self.harness_flow_scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.harness_flow_scroll = self.harness_flow_scroll.saturating_add(10);
            }
            KeyCode::Home => self.harness_flow_scroll = 0,
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.last_harness_flow_refresh_at = None;
                self.refresh_harness_flow();
            }
            KeyCode::Char('[') => {
                self.switch_settings_view(previous_settings_view(self.settings_view))
            }
            KeyCode::Char(']') => self.switch_settings_view(next_settings_view(self.settings_view)),
            KeyCode::Left if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.switch_settings_view(previous_settings_view(self.settings_view));
            }
            KeyCode::Right if key_event.modifiers.contains(KeyModifiers::ALT) => {
                self.switch_settings_view(next_settings_view(self.settings_view));
            }
            _ => {}
        }
        Ok(())
    }

    fn refresh_gpu_cards(&mut self) {
        let gpu_refresh_interval = self.gpu_refresh_interval();
        if !refresh_due(&mut self.last_gpu_refresh_at, gpu_refresh_interval) {
            return;
        }
        if let Ok(cards) = sample_gpu_cards() {
            self.gpu_cards = cards;
        }
    }

    fn refresh_chat_messages(&mut self) -> Result<()> {
        let settings = self.saved_settings_env_map();
        let max_context = settings
            .get("CTOX_CHAT_MODEL_MAX_CONTEXT")
            .and_then(|value| runtime_plan::parse_chat_context_tokens(value))
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_MAX_CONTEXT);
        let engine = lcm::LcmEngine::open(&self.db_path, lcm::LcmConfig::default())?;
        let snapshot = engine.snapshot(turn_loop::CHAT_CONVERSATION_ID)?;
        let continuity = engine.continuity_show_all(turn_loop::CHAT_CONVERSATION_ID)?;
        let forgotten = engine.continuity_forgotten(turn_loop::CHAT_CONVERSATION_ID, None, None)?;
        let health = context_health::assess_with_forgotten(
            &snapshot,
            &continuity,
            &forgotten,
            "",
            max_context as i64,
        );
        let governance_snapshot =
            governance::prompt_snapshot(&self.root, turn_loop::CHAT_CONVERSATION_ID)
                .unwrap_or_default();
        let mission_state = engine.mission_state(turn_loop::CHAT_CONVERSATION_ID)?;
        let mission_assurance =
            engine.mission_assurance_snapshot(turn_loop::CHAT_CONVERSATION_ID)?;
        self.prompt_context_breakdown = live_context::prompt_context_breakdown_for_runtime(
            &self.root,
            &settings,
            &snapshot,
            &continuity,
            &mission_state,
            &mission_assurance,
            &governance_snapshot,
            &health,
        )
        .ok();
        self.context_health = Some(health);
        self.chat_messages = snapshot.messages;
        self.mission_state = Some(mission_state);
        Ok(())
    }

    fn refresh_communication_feed(&mut self) {
        self.communication_feed =
            channels::load_recent_communication_feed(&self.root, 10).unwrap_or_default();
    }

    fn service_refresh_interval(&self) -> Duration {
        if self.page == Page::Settings {
            SERVICE_REFRESH_INTERVAL_SETTINGS
        } else {
            SERVICE_REFRESH_INTERVAL_ACTIVE
        }
    }

    fn chat_refresh_interval(&self) -> Duration {
        if self.page == Page::Chat {
            CHAT_REFRESH_INTERVAL_ACTIVE
        } else {
            CHAT_REFRESH_INTERVAL_BACKGROUND
        }
    }

    fn communication_refresh_interval(&self) -> Duration {
        if self.page == Page::Settings && self.settings_view == SettingsView::Communication {
            COMMUNICATION_REFRESH_INTERVAL_ACTIVE
        } else {
            COMMUNICATION_REFRESH_INTERVAL_BACKGROUND
        }
    }

    fn gpu_refresh_interval(&self) -> Duration {
        if self.page == Page::Settings {
            GPU_REFRESH_INTERVAL_SETTINGS
        } else {
            GPU_REFRESH_INTERVAL_ACTIVE
        }
    }

    fn proxy_refresh_interval(&self) -> Duration {
        if self.page == Page::Settings {
            PROXY_REFRESH_INTERVAL_SETTINGS
        } else {
            PROXY_REFRESH_INTERVAL_ACTIVE
        }
    }

    fn should_refresh_chat_messages(&self) -> bool {
        self.page == Page::Chat || self.request_in_flight || self.service_status.busy
    }

    fn should_refresh_communication_feed(&self) -> bool {
        self.page == Page::Chat
            || (self.page == Page::Settings && self.settings_view == SettingsView::Communication)
    }

    fn refresh_header(&mut self) {
        let mut saved_settings = self.saved_settings_env_map();
        let mut draft_settings = self.settings_env_map();
        normalize_runtime_model_settings(&mut saved_settings);
        normalize_runtime_model_settings(&mut draft_settings);
        let estimate_mode = self.has_draft_runtime_estimate();
        let settings = if estimate_mode {
            &draft_settings
        } else {
            &saved_settings
        };
        let current_runtime_state = runtime_state::load_or_resolve_runtime_state(&self.root).ok();
        let selected_source = infer_chat_source(settings);
        let saved_source = infer_chat_source(&saved_settings);
        let display_source = if estimate_mode {
            selected_source.as_str()
        } else {
            saved_source.as_str()
        };
        let runtime_telemetry = self.runtime_telemetry.clone();
        let configured_model = runtime_env::configured_chat_model_from_map(settings)
            .or_else(|| runtime_env::configured_chat_model_from_map(&saved_settings))
            .unwrap_or_else(|| default_active_model().to_string());
        let saved_configured_model = runtime_env::configured_chat_model_from_map(&saved_settings)
            .unwrap_or_else(|| configured_model.clone());
        let live_local_model = if saved_source.eq_ignore_ascii_case("local") {
            runtime_telemetry
                .as_ref()
                .and_then(|telemetry| telemetry.active_model.clone())
                .or_else(|| runtime_env::effective_chat_model_from_map(&saved_settings))
                .or_else(|| runtime_env::configured_chat_model_from_map(&saved_settings))
        } else {
            None
        };
        let model = if display_source.eq_ignore_ascii_case("api") {
            configured_model.clone()
        } else if estimate_mode {
            configured_model.clone()
        } else {
            live_local_model
                .clone()
                .unwrap_or_else(|| saved_configured_model.clone())
        };
        let base_model = if display_source.eq_ignore_ascii_case("api") {
            configured_model.clone()
        } else {
            runtime_telemetry
                .as_ref()
                .and_then(|telemetry| telemetry.base_model.clone())
                .unwrap_or_else(|| saved_configured_model.clone())
        };

        let selected_bundle = if selected_source.eq_ignore_ascii_case("local") {
            runtime_plan::preview_chat_preset_bundle(&self.root, settings)
                .ok()
                .flatten()
        } else {
            None
        };
        let saved_bundle = if saved_source.eq_ignore_ascii_case("local") {
            runtime_plan::preview_chat_preset_bundle(&self.root, &saved_settings)
                .ok()
                .flatten()
        } else {
            None
        };
        self.chat_preset_bundle = selected_bundle.clone();

        let realized_context_from_runtime = saved_settings
            .get("CTOX_ENGINE_REALIZED_MAX_SEQ_LEN")
            .or_else(|| saved_settings.get("CTOX_CHAT_MODEL_REALIZED_CONTEXT"))
            .and_then(|value| value.trim().parse::<usize>().ok());

        let planned_context = selected_bundle
            .as_ref()
            .map(|bundle| bundle.selected_plan.max_seq_len as usize);
        let saved_context = saved_bundle
            .as_ref()
            .map(|bundle| bundle.selected_plan.max_seq_len as usize);
        let configured_context = current_runtime_state
            .as_ref()
            .and_then(|state| state.configured_context_tokens.map(|value| value as usize))
            .or_else(|| {
                settings
                    .get("CTOX_CHAT_MODEL_MAX_CONTEXT")
                    .and_then(|value| runtime_plan::parse_chat_context_tokens(value))
                    .map(|value| value as usize)
            })
            .or(planned_context)
            .or(saved_context)
            .or_else(|| {
                engine::runtime_config_for_model(&model)
                    .ok()
                    .and_then(|runtime| runtime.max_seq_len.map(|value| value as usize))
            })
            .unwrap_or(DEFAULT_MAX_CONTEXT)
            .min(MAX_CONTEXT_WINDOW);
        let realized_context = if estimate_mode {
            planned_context.unwrap_or(configured_context)
        } else {
            realized_context_from_runtime
                .or_else(|| {
                    current_runtime_state
                        .as_ref()
                        .and_then(|state| state.realized_context_tokens.map(|value| value as usize))
                })
                .or(saved_context)
                .unwrap_or(configured_context)
        }
        .min(MAX_CONTEXT_WINDOW);
        let load_observations = collect_runtime_load_observations(
            &self.root,
            runtime_telemetry.as_ref(),
            &saved_settings,
        );
        let runtime_health = runtime_health_state(&self.root, runtime_telemetry.as_ref());
        self.runtime_health = runtime_health.clone();
        let saved_runtime_models = configured_runtime_models(&saved_settings);
        let saved_target_gpu_cards =
            if let Some(plan) = saved_bundle.as_ref().map(|bundle| &bundle.selected_plan) {
                gpu_cards_from_plan(plan, &saved_settings)
            } else {
                aux_gpu_target_cards(&self.gpu_cards, &saved_settings)
            };
        let gpu_target_cards = if estimate_mode {
            if let Some(plan) = selected_bundle.as_ref().map(|bundle| &bundle.selected_plan) {
                gpu_cards_from_plan(plan, settings)
            } else {
                if selected_source.eq_ignore_ascii_case("local") {
                    estimate_gpu_cards(
                        &self.gpu_cards,
                        live_local_model.as_deref().unwrap_or(""),
                        &model,
                        settings,
                        realized_context,
                    )
                } else {
                    aux_gpu_target_cards(&self.gpu_cards, settings)
                }
            }
        } else {
            Vec::new()
        };
        let gpu_loading_cards = loading_gpu_cards_from_observations(
            &saved_target_gpu_cards,
            &self.gpu_cards,
            &load_observations,
        );
        let unhealthy_gpu_cards =
            unhealthy_backend_loading_cards(&self.gpu_cards, &saved_settings, &runtime_health);
        let mut gpu_loading_cards = merge_gpu_card_layers(
            gpu_loading_cards,
            if self.runtime_switch_in_flight {
                unhealthy_gpu_cards.clone()
            } else {
                Vec::new()
            },
        );
        if self.runtime_switch_in_flight {
            if let Some(cards) = self.pending_runtime_transition_cards.clone() {
                gpu_loading_cards = merge_gpu_card_layers(gpu_loading_cards, cards);
            }
        }
        let gpu_error_cards = if self.runtime_switch_in_flight {
            Vec::new()
        } else {
            unhealthy_gpu_cards
        };
        let healthy_aux_gpu_cards =
            healthy_backend_deployed_cards(&self.gpu_cards, &saved_settings, &runtime_health);
        let gpu_cards = merge_gpu_card_layers(
            deployed_gpu_cards_from_live(
                &self.gpu_cards,
                &saved_runtime_models,
                &load_observations,
            ),
            healthy_aux_gpu_cards,
        );
        let effective_context = configured_context
            .min(realized_context)
            .min(MAX_CONTEXT_WINDOW);
        let planned_compact_percent = selected_bundle
            .as_ref()
            .map(|bundle| bundle.selected_plan.compaction_threshold_percent as usize);
        let saved_compact_percent = saved_bundle
            .as_ref()
            .map(|bundle| bundle.selected_plan.compaction_threshold_percent as usize);
        let compact_percent = if estimate_mode {
            planned_compact_percent
                .or(saved_compact_percent)
                .unwrap_or_else(|| {
                    read_usize_setting(
                        settings,
                        "CTOX_CHAT_COMPACTION_THRESHOLD_PERCENT",
                        DEFAULT_COMPACTION_THRESHOLD_PERCENT,
                    )
                })
        } else {
            saved_compact_percent
                .or(planned_compact_percent)
                .unwrap_or_else(|| {
                    read_usize_setting(
                        &saved_settings,
                        "CTOX_CHAT_COMPACTION_THRESHOLD_PERCENT",
                        DEFAULT_COMPACTION_THRESHOLD_PERCENT,
                    )
                })
        }
        .clamp(1, 99);
        let planned_compact_min = selected_bundle
            .as_ref()
            .map(|bundle| bundle.selected_plan.compaction_min_tokens as usize);
        let saved_compact_min = saved_bundle
            .as_ref()
            .map(|bundle| bundle.selected_plan.compaction_min_tokens as usize);
        let compact_min_tokens = if estimate_mode {
            planned_compact_min
                .or(saved_compact_min)
                .unwrap_or_else(|| {
                    read_usize_setting(
                        settings,
                        "CTOX_CHAT_COMPACTION_MIN_TOKENS",
                        default_compaction_min_tokens(effective_context),
                    )
                })
        } else {
            saved_compact_min
                .or(planned_compact_min)
                .unwrap_or_else(|| {
                    read_usize_setting(
                        &saved_settings,
                        "CTOX_CHAT_COMPACTION_MIN_TOKENS",
                        default_compaction_min_tokens(effective_context),
                    )
                })
        };
        let compact_at =
            compute_compaction_threshold(effective_context, compact_percent, compact_min_tokens);
        let current_tokens = current_context_tokens(&self.db_path, effective_context).unwrap_or(0);
        self.record_runtime_model_sample(runtime_telemetry.as_ref());
        let today_api_cost =
            crate::api_costs::summary_for_day(&self.root, &crate::api_costs::today_day()).ok();
        let avg_tokens_per_second = if estimate_mode {
            selected_bundle
                .as_ref()
                .map(|bundle| bundle.selected_plan.expected_tok_s)
                .or_else(|| estimated_tokens_per_second(&model, &self.model_perf_stats))
        } else {
            self.model_perf_stats
                .get(model.trim())
                .map(|stats| stats.avg_tokens_per_second)
                .or_else(|| {
                    saved_bundle
                        .as_ref()
                        .map(|bundle| bundle.selected_plan.expected_tok_s)
                })
        };
        let expected_aux_models = expected_gpu_aux_labels(settings);
        let backend_warmup = self.service_status.running
            && !expected_aux_models.is_empty()
            && gpu_cards.iter().all(|card| card.allocations.is_empty())
            && gpu_loading_cards
                .iter()
                .all(|card| card.allocations.is_empty())
            && gpu_error_cards
                .iter()
                .all(|card| card.allocations.is_empty());
        self.header = HeaderState {
            chat_source: display_source.to_string(),
            model,
            base_model,
            service_running: self.service_status.running,
            boost_model: runtime_telemetry
                .as_ref()
                .and_then(|value| value.boost_model.clone()),
            boost_active: runtime_telemetry
                .as_ref()
                .map(|value| value.boost_active)
                .unwrap_or(false),
            boost_remaining_seconds: runtime_telemetry
                .as_ref()
                .and_then(|value| value.boost_remaining_seconds),
            boost_reason: runtime_telemetry
                .as_ref()
                .and_then(|value| value.boost_reason.clone()),
            max_context: effective_context,
            realized_context,
            configured_context,
            compact_at,
            compact_percent,
            compact_min_tokens,
            current_tokens,
            tokens_per_second: runtime_telemetry
                .as_ref()
                .and_then(|value| value.last_tokens_per_second),
            avg_tokens_per_second,
            last_input_tokens: runtime_telemetry
                .as_ref()
                .and_then(|value| value.last_input_tokens),
            last_output_tokens: runtime_telemetry
                .as_ref()
                .and_then(|value| value.last_output_tokens),
            last_total_tokens: runtime_telemetry
                .as_ref()
                .and_then(|value| value.last_total_tokens),
            today_api_cost_microusd: today_api_cost
                .as_ref()
                .map(|summary| summary.total_cost_microusd)
                .unwrap_or(0),
            today_api_cost_events: today_api_cost
                .as_ref()
                .map(|summary| summary.events)
                .unwrap_or(0),
            today_api_unpriced_events: today_api_cost
                .as_ref()
                .map(|summary| summary.unpriced_events)
                .unwrap_or(0),
            gpu_cards,
            gpu_loading_cards,
            gpu_error_cards,
            gpu_target_cards,
            backend_warmup,
            expected_aux_models,
            estimate_mode,
            chat_plan: selected_bundle.map(|bundle| bundle.selected_plan),
        };
    }

    fn has_draft_runtime_estimate(&self) -> bool {
        self.settings_items.iter().any(|item| {
            self.setting_is_dirty(item)
                && matches!(item.kind, SettingKind::Env)
                && relevant_header_estimate_setting(item.key)
        })
    }

    fn record_runtime_model_sample(&mut self, telemetry: Option<&RuntimeTelemetry>) {
        let Some(telemetry) = telemetry else {
            return;
        };
        let Some(response_at) = telemetry.last_response_at.as_deref() else {
            return;
        };
        if self.last_recorded_response_at.as_deref() == Some(response_at) {
            return;
        }
        let Some(model) = telemetry
            .active_model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return;
        };
        let Some(tps) = telemetry.last_tokens_per_second else {
            self.last_recorded_response_at = Some(response_at.to_string());
            return;
        };
        let stats = self.model_perf_stats.entry(model.to_string()).or_default();
        let next_samples = stats.samples + 1;
        let weighted_sum = stats.avg_tokens_per_second * stats.samples as f64 + tps;
        stats.samples = next_samples;
        stats.avg_tokens_per_second = weighted_sum / next_samples as f64;
        stats.last_tokens_per_second = Some(tps);
        self.last_recorded_response_at = Some(response_at.to_string());
        let _ = save_model_perf_stats(&self.root, &self.model_perf_stats);
    }

    fn save_current_secret(&mut self) -> Result<()> {
        let Some(current) = self.current_secret().cloned() else {
            self.status_line = "No secret selected.".to_string();
            return Ok(());
        };
        if !self.secret_is_dirty(&current) {
            self.status_line = format!("Secret {}/{} unchanged.", current.scope, current.name);
            return Ok(());
        }
        secrets::write_secret_record(
            &self.root,
            &current.scope,
            &current.name,
            &current.value,
            current.description.clone(),
            current.metadata.clone(),
        )?;
        if current.scope.eq_ignore_ascii_case("credentials") {
            if let Some(setting) = self
                .settings_items
                .iter_mut()
                .find(|item| item.key == current.name)
            {
                setting.value = current.value.clone();
                setting.saved_value = current.value.clone();
            }
        }
        let selected_scope = current.scope.clone();
        let selected_name = current.name.clone();
        self.reload_secrets();
        if let Some(index) = self
            .secret_items
            .iter()
            .position(|item| item.scope == selected_scope && item.name == selected_name)
        {
            self.secrets_selected = index;
        }
        self.status_line = format!("Saved encrypted secret {}/{}.", current.scope, current.name);
        Ok(())
    }

    fn save_settings(&mut self) -> Result<()> {
        let previous_env = runtime_env::load_runtime_env_map(&self.root).unwrap_or_default();
        let previous_runtime_state = runtime_state::load_or_resolve_runtime_state(&self.root)?;
        let mut operator_env_map = previous_env.clone();
        operator_env_map.retain(|key, _| !runtime_state::is_runtime_state_key(key));
        for item in &self.settings_items {
            if item.kind != SettingKind::Env {
                continue;
            }
            if is_secret_backed_runtime_setting(item.key) {
                let trimmed = item.value.trim();
                if trimmed.is_empty() {
                    secrets::delete_credential(&self.root, item.key)?;
                } else {
                    secrets::set_credential(&self.root, item.key, trimmed)?;
                }
                operator_env_map.remove(item.key);
                continue;
            }
            if runtime_state::is_runtime_state_key(item.key) {
                continue;
            }
            let trimmed = item.value.trim();
            if trimmed.is_empty() {
                operator_env_map.remove(item.key);
            } else {
                let persisted_value = if item.key == "CTOX_CHAT_MODEL_MAX_CONTEXT" {
                    runtime_plan::parse_chat_context_tokens(trimmed)
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| trimmed.to_string())
                } else {
                    trimmed.to_string()
                };
                if item.key == CTOX_CTO_OPERATING_MODE_KEY
                    && persisted_value == DEFAULT_CTO_OPERATING_MODE_PROMPT.trim()
                {
                    operator_env_map.remove(item.key);
                } else {
                    operator_env_map.insert(item.key.to_string(), persisted_value);
                }
            }
        }
        if let Some(family_label) = operator_env_map
            .get("CTOX_CHAT_MODEL_FAMILY")
            .cloned()
            .filter(|value| !value.trim().is_empty())
        {
            if let Some(family) = engine::parse_chat_model_family(&family_label) {
                operator_env_map.insert(
                    "CTOX_CHAT_MODEL_FAMILY".to_string(),
                    family.selector().to_string(),
                );
            }
        }
        operator_env_map.retain(|key, _| !is_secret_backed_runtime_setting(key));
        let work_hours_config = service::working_hours::config_from_map(&operator_env_map);
        service::working_hours::validate_config(&work_hours_config)?;

        let mut selection_env_map = operator_env_map.clone();
        if let Some(source) = self.value_for_setting("CTOX_CHAT_SOURCE") {
            selection_env_map.insert("CTOX_CHAT_SOURCE".to_string(), source.to_string());
        }
        if let Some(local_runtime) = self.value_for_setting("CTOX_LOCAL_RUNTIME") {
            selection_env_map.insert("CTOX_LOCAL_RUNTIME".to_string(), local_runtime.to_string());
        }
        if let Some(model) = self.value_for_setting("CTOX_CHAT_MODEL") {
            selection_env_map.insert("CTOX_CHAT_MODEL".to_string(), model.to_string());
        } else {
            selection_env_map.remove("CTOX_CHAT_MODEL");
        }
        if let Some(preset) = self.value_for_setting("CTOX_CHAT_LOCAL_PRESET") {
            selection_env_map.insert("CTOX_CHAT_LOCAL_PRESET".to_string(), preset.to_string());
        } else {
            selection_env_map.remove("CTOX_CHAT_LOCAL_PRESET");
        }
        if let Some(context) = self.value_for_setting("CTOX_CHAT_MODEL_MAX_CONTEXT") {
            selection_env_map.insert(
                "CTOX_CHAT_MODEL_MAX_CONTEXT".to_string(),
                context.to_string(),
            );
        } else {
            selection_env_map.remove("CTOX_CHAT_MODEL_MAX_CONTEXT");
        }
        normalize_runtime_model_settings(&mut selection_env_map);
        if infer_api_provider(&selection_env_map).eq_ignore_ascii_case("local")
            && infer_local_runtime(&selection_env_map).eq_ignore_ascii_case("candle")
        {
            if let Some(resolved_model) = runtime_plan::resolve_local_chat_model_from_settings(
                &self.root,
                &selection_env_map,
            )? {
                selection_env_map.insert("CTOX_CHAT_MODEL".to_string(), resolved_model);
            }
        }
        let next_source = infer_chat_source(&selection_env_map);
        let next_model = selection_env_map
            .get("CTOX_CHAT_MODEL")
            .cloned()
            .filter(|value| !value.trim().is_empty());
        let next_preset = selection_env_map
            .get("CTOX_CHAT_LOCAL_PRESET")
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| {
                let normalized = runtime_plan::ChatPreset::from_label(&value)
                    .label()
                    .to_string();
                if next_source.eq_ignore_ascii_case("api")
                    || infer_local_runtime(&selection_env_map).eq_ignore_ascii_case("candle")
                {
                    Some(normalized)
                } else {
                    None
                }
            });
        let next_context = selection_env_map
            .get("CTOX_CHAT_MODEL_MAX_CONTEXT")
            .and_then(|value| runtime_plan::parse_chat_context_tokens(value));

        let mut next_runtime_state = previous_runtime_state.clone();
        next_runtime_state.local_runtime =
            runtime_state::infer_local_runtime_kind_from_env_map(&selection_env_map);
        next_runtime_state.configured_context_tokens = next_context;
        next_runtime_state.boost.model = self
            .value_for_setting("CTOX_CHAT_MODEL_BOOST")
            .map(ToOwned::to_owned);
        next_runtime_state.embedding.configured_model = self
            .value_for_setting("CTOX_EMBEDDING_MODEL")
            .map(ToOwned::to_owned);
        next_runtime_state.transcription.configured_model = self
            .value_for_setting("CTOX_STT_MODEL")
            .map(ToOwned::to_owned);
        next_runtime_state.speech.configured_model = self
            .value_for_setting("CTOX_TTS_MODEL")
            .map(ToOwned::to_owned);

        runtime_env::save_runtime_state_projection(
            &self.root,
            &next_runtime_state,
            &operator_env_map,
        )?;
        let persisted_env = runtime_env::load_runtime_env_map(&self.root).unwrap_or_default();
        channels::sync_prompt_identity(&self.root, &persisted_env)?;
        let pending_switch = next_model.as_deref().and_then(|next| {
            runtime_selection_changed(
                &previous_runtime_state,
                &next_source,
                infer_local_runtime(&selection_env_map).as_str(),
                Some(next),
                next_preset.as_deref(),
                next_context,
            )
            .then(|| (next.to_string(), next_preset.clone(), next_context))
        });
        self.settings_dirty = false;
        for item in &mut self.settings_items {
            item.saved_value = item.value.clone();
        }
        self.refresh_dynamic_setting_choices();
        self.refresh_skill_catalog();
        if self.service_status.running || self.runtime_health.runtime_ready {
            let _ = supervisor::ensure_persistent_backends(&self.root);
        }
        self.invalidate_runtime_observations();
        if let Some((model, preset, context)) = pending_switch {
            let (tx, rx) = mpsc::channel();
            let root = self.root.clone();
            let status_model = model.clone();
            self.pending_runtime_transition_cards = (!self.header.gpu_target_cards.is_empty())
                .then(|| self.header.gpu_target_cards.clone());
            self.runtime_switch_in_flight = true;
            self.runtime_switch_rx = Some(rx);
            thread::spawn(move || {
                let result =
                    apply_runtime_model_selection(&root, &model, preset.as_deref(), context)
                        .map_err(|err| err.to_string());
                let _ = tx.send(result);
            });
            self.status_line = format!(
                "Settings saved. Applying runtime change to {}...",
                summarize_inline(&status_model, 48)
            );
        } else {
            self.runtime_switch_in_flight = false;
            self.runtime_switch_rx = None;
            self.pending_runtime_transition_cards = None;
            self.status_line = "Settings saved to runtime state.".to_string();
        }
        self.refresh_header();
        self.refresh_jami_qr();
        Ok(())
    }

    #[allow(dead_code)]
    fn header_lines(&self, width: usize) -> Vec<String> {
        let model = compact_model_name(&self.header.model, width);
        let loop_state = if self.service_status.running {
            if self.service_status.busy {
                "busy"
            } else {
                "idle"
            }
        } else {
            "down"
        };
        let queue_state = if self.request_in_flight {
            if self.draft_queue.is_empty() && self.service_status.pending_count == 0 {
                "coil on".to_string()
            } else {
                format!(
                    "coil on q{}",
                    self.draft_queue.len() + self.service_status.pending_count
                )
            }
        } else if self.draft_queue.is_empty() {
            "coil off".to_string()
        } else {
            format!("coil off q{}", self.draft_queue.len())
        };
        let model_line = format!(
            "loaded {}   loop {}   {}   tok/s {}",
            model,
            loop_state,
            queue_state,
            self.header
                .tokens_per_second
                .map(|value| format!("{value:.1}"))
                .unwrap_or_else(|| "-".to_string()),
        );
        let usage_line = format!(
            "used {} / {}   compact {}   live {}   io {}/{}/{}   api {}{}",
            self.header.current_tokens,
            self.header.max_context,
            self.header.compact_at,
            self.header.realized_context,
            self.header
                .last_input_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            self.header
                .last_output_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            self.header
                .last_total_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            crate::api_costs::format_usd_micros(self.header.today_api_cost_microusd),
            if self.header.today_api_unpriced_events > 0 {
                " + unpriced"
            } else {
                ""
            },
        );
        let preview_line = truncate_for_ui(&self.header_preview_line(), width);
        let status_line = truncate_for_ui(&self.status_line, width);
        let bar_width = width.saturating_sub(18).max(12);
        let marker_index = if self.header.max_context == 0 {
            0
        } else {
            ((self.header.compact_at.min(self.header.max_context) as f64
                / self.header.max_context as f64)
                * bar_width as f64)
                .floor() as usize
        };
        let fill_index = if self.header.max_context == 0 {
            0
        } else {
            ((self.header.current_tokens.min(self.header.max_context) as f64
                / self.header.max_context as f64)
                * bar_width as f64)
                .floor() as usize
        };
        let realized_index = if self.header.max_context == 0 {
            0
        } else {
            ((self.header.realized_context.min(self.header.max_context) as f64
                / self.header.max_context as f64)
                * bar_width as f64)
                .floor() as usize
        };
        let mut bar = String::with_capacity(bar_width + 2);
        bar.push('▕');
        for idx in 0..bar_width {
            if idx == marker_index.min(bar_width.saturating_sub(1)) {
                bar.push('◆');
            } else if idx == realized_index.min(bar_width.saturating_sub(1)) {
                bar.push('╎');
            } else if idx < fill_index {
                bar.push('█');
            } else {
                bar.push('░');
            }
        }
        bar.push('▏');
        vec![
            truncate_for_ui(&model_line, width),
            truncate_for_ui(&usage_line, width),
            preview_line,
            truncate_for_ui(&format!("0 {bar} {}", self.header.max_context), width),
            status_line,
        ]
    }

    #[allow(dead_code)]
    fn header_preview_line(&self) -> String {
        let family_item = self
            .settings_items
            .iter()
            .find(|item| item.key == "CTOX_CHAT_MODEL_FAMILY");
        let model_item = self
            .settings_items
            .iter()
            .find(|item| item.key == "CTOX_CHAT_MODEL");
        let preset_item = self
            .settings_items
            .iter()
            .find(|item| item.key == "CTOX_CHAT_LOCAL_PRESET");
        let channel_item = self
            .settings_items
            .iter()
            .find(|item| item.key == "CTOX_OWNER_PREFERRED_CHANNEL");

        if let Some(bundle) = &self.chat_preset_bundle {
            if family_item
                .map(|item| self.setting_is_dirty(item))
                .unwrap_or(false)
                || model_item
                    .map(|item| self.setting_is_dirty(item))
                    .unwrap_or(false)
                || preset_item
                    .map(|item| self.setting_is_dirty(item))
                    .unwrap_or(false)
            {
                let plan = &bundle.selected_plan;
                return format!(
                    "draft {}  {}  cache {}  ctx {}  compact {}% min {}k  {} tok/s",
                    compact_model_name(&plan.model, 20),
                    plan.quantization,
                    plan.effective_cache_label(),
                    plan.max_seq_len,
                    plan.compaction_threshold_percent,
                    plan.compaction_min_tokens / 1024,
                    plan.expected_tok_s.round() as i64
                );
            }
        }
        if let Some(channel_item) = channel_item {
            if self.setting_is_dirty(channel_item) {
                return format!("draft channel {}", channel_item.value.trim());
            }
        }
        "loaded state".to_string()
    }

    fn move_settings_menu(&mut self, delta: isize) {
        let Some(item) = self.current_setting() else {
            self.settings_menu_open = false;
            return;
        };
        if item.choices.is_empty() {
            self.settings_menu_open = false;
            return;
        }
        let len = item.choices.len() as isize;
        let next = (self.settings_menu_index as isize + delta).rem_euclid(len);
        self.settings_menu_index = next as usize;
    }

    fn commit_settings_menu_choice(&mut self) -> Result<()> {
        let selected_index = self.settings_menu_index;
        let Some(item) = self.current_setting_mut() else {
            self.settings_menu_open = false;
            return Ok(());
        };
        if item.choices.is_empty() {
            self.settings_menu_open = false;
            return Ok(());
        }
        let next = item
            .choices
            .get(selected_index)
            .copied()
            .unwrap_or(item.choices[0]);
        item.value = next.to_string();
        self.settings_dirty = true;
        self.settings_menu_open = false;
        self.refresh_dynamic_setting_choices();
        self.refresh_jami_qr();
        Ok(())
    }

    fn jami_details_active(&self) -> bool {
        if self.page != Page::Settings {
            return false;
        }
        // Show the Jami QR code whenever a Jami-related field is focused,
        // regardless of the preferred reply channel.
        matches!(
            self.current_setting().map(|item| item.key),
            Some("CTO_JAMI_PROFILE_NAME") | Some("CTO_JAMI_ACCOUNT_ID")
        )
    }

    fn refresh_jami_qr(&mut self) {
        if !self.jami_details_active() {
            self.jami_qr_lines.clear();
            self.last_jami_qr_key.clear();
            self.last_jami_refresh_at = None;
            self.jami_runtime_account = None;
            return;
        }
        let channel = self
            .value_for_setting("CTOX_OWNER_PREFERRED_CHANNEL")
            .unwrap_or("tui");
        let configured_jami_id = self
            .value_for_setting("CTO_JAMI_ACCOUNT_ID")
            .unwrap_or("")
            .trim()
            .to_string();
        let configured_profile_name = self
            .value_for_setting("CTO_JAMI_PROFILE_NAME")
            .unwrap_or("")
            .trim()
            .to_string();
        let refresh_key = format!("{channel}:{configured_jami_id}:{configured_profile_name}");
        let should_probe = self.last_jami_qr_key != refresh_key
            || self
                .last_jami_refresh_at
                .map(|at| at.elapsed() >= Duration::from_secs(5))
                .unwrap_or(true);
        if !should_probe && !self.jami_qr_lines.is_empty() {
            return;
        }
        let resolved =
            resolve_jami_runtime_account(&self.root, &configured_jami_id, &configured_profile_name);
        let qr_payload = resolved
            .account
            .as_ref()
            .and_then(|account| {
                if !account.share_uri.trim().is_empty() {
                    Some(account.share_uri.trim().to_string())
                } else if !account.username.trim().is_empty() {
                    Some(format!("jami:{}", account.username.trim()))
                } else {
                    None
                }
            })
            .or_else(|| (!configured_jami_id.is_empty()).then(|| configured_jami_id.clone()))
            .unwrap_or_default();
        let qr_key = refresh_key;
        if !qr_payload.is_empty()
            && self.last_jami_qr_key == qr_key
            && !self.jami_qr_lines.is_empty()
        {
            self.jami_runtime_account = resolved.account;
            return;
        }
        self.last_jami_refresh_at = Some(Instant::now());
        self.jami_runtime_account = resolved.account.clone();
        if qr_payload.is_empty() {
            if let Some(error) = resolved.error.as_deref() {
                self.jami_qr_lines = jami_error_lines(
                    error,
                    resolved.dbus_env_file.as_deref(),
                    !configured_jami_id.is_empty() || !configured_profile_name.is_empty(),
                    &resolved.checks,
                );
                self.last_jami_qr_key = qr_key;
                return;
            }
            self.jami_qr_lines = jami_missing_account_lines(
                resolved.dbus_env_file.as_deref(),
                !configured_jami_id.is_empty() || !configured_profile_name.is_empty(),
                &resolved.checks,
            );
            self.last_jami_qr_key = qr_key;
            return;
        }
        let mut lines = render_qr_lines(&qr_payload).unwrap_or_else(|| {
            vec![
                "Failed to render Jami QR.".to_string(),
                format!("uri {}", truncate_for_ui(&qr_payload, 40)),
            ]
        });
        if let Some(error) = resolved.error.as_deref() {
            lines.push(String::new());
            lines.extend(jami_error_lines(
                error,
                resolved.dbus_env_file.as_deref(),
                !configured_jami_id.is_empty() || !configured_profile_name.is_empty(),
                &resolved.checks,
            ));
        } else if resolved.account.is_none() {
            lines.push(String::new());
            lines.extend(jami_missing_account_lines(
                resolved.dbus_env_file.as_deref(),
                !configured_jami_id.is_empty() || !configured_profile_name.is_empty(),
                &resolved.checks,
            ));
        }
        self.jami_qr_lines = lines;
        self.last_jami_qr_key = qr_key;
    }

    fn push_local_activity(&mut self, event: String) {
        self.activity_log.push(event);
        if self.activity_log.len() > 32 {
            let overflow = self.activity_log.len() - 32;
            self.activity_log.drain(0..overflow);
        }
    }

    fn sync_activity_log(&mut self) {
        let mut merged = self.service_status.recent_events.clone();
        merged.extend(self.activity_log.iter().cloned());
        if merged.len() > 32 {
            merged = merged.split_off(merged.len() - 32);
        }
        self.activity_log = merged;
    }
}

fn load_skill_catalog(root: &Path) -> Vec<SkillCatalogEntry> {
    let _ = crate::skill_store::bootstrap_embedded_system_skills(root);
    let _ = crate::skill_store::bootstrap_from_roots(root);
    let mut catalog = crate::skill_store::list_skill_bundles(root)
        .unwrap_or_default()
        .into_iter()
        .map(|bundle| {
            let files =
                crate::skill_store::list_skill_files(root, &bundle.skill_id).unwrap_or_default();
            let helper_tools = files
                .iter()
                .filter_map(|file| file.relative_path.strip_prefix("scripts/"))
                .filter(|value| !value.contains('/'))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            let resources = summarize_skill_resources(&files);
            SkillCatalogEntry {
                name: bundle.skill_name,
                class: skill_class_from_store(&bundle.class),
                state: skill_state_from_store(&bundle.state),
                cluster: bundle.cluster,
                skill_path: bundle
                    .source_path
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(format!("sqlite://{}", bundle.skill_id))),
                description: bundle.description,
                helper_tools,
                resources,
            }
        })
        .collect::<Vec<_>>();
    catalog.sort_by(|left, right| {
        left.class
            .rank()
            .cmp(&right.class.rank())
            .then(left.cluster.cmp(&right.cluster))
            .then(left.name.cmp(&right.name))
            .then(left.skill_path.cmp(&right.skill_path))
    });
    catalog
}

fn skill_class_from_store(value: &str) -> SkillClass {
    match value.trim() {
        "codex_core" => SkillClass::CodexCore,
        "installed_packs" => SkillClass::InstalledPacks,
        "personal" => SkillClass::Personal,
        _ => SkillClass::CtoxCore,
    }
}

fn skill_state_from_store(value: &str) -> SkillState {
    match value.trim() {
        "authored" => SkillState::Authored,
        "generated" => SkillState::Generated,
        "draft" => SkillState::Draft,
        _ => SkillState::Stable,
    }
}

fn summarize_skill_resources(files: &[crate::skill_store::SkillFileView]) -> Vec<String> {
    let mut groups: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for prefix in ["references/", "assets/", "templates/", "agents/"] {
        groups.insert(prefix, Vec::new());
    }
    for file in files {
        for prefix in ["references/", "assets/", "templates/", "agents/"] {
            if let Some(stripped) = file.relative_path.strip_prefix(prefix) {
                let name = stripped.split('/').next().unwrap_or(stripped).to_string();
                let group = groups.get_mut(prefix).expect("group inserted");
                if !group.contains(&name) {
                    group.push(name);
                }
            }
        }
    }
    let mut out = Vec::new();
    for (prefix, mut entries) in groups {
        if entries.is_empty() {
            continue;
        }
        entries.sort();
        let label = prefix.trim_end_matches('/');
        let preview = entries
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if entries.len() > 5 {
            format!(" (+{} more)", entries.len() - 5)
        } else {
            String::new()
        };
        out.push(format!("{label}: {preview}{suffix}"));
    }
    out
}

fn apply_runtime_model_selection(
    root: &Path,
    model: &str,
    preset: Option<&str>,
    context: Option<u32>,
) -> Result<String> {
    let mut runtime_state = runtime_state::load_or_resolve_runtime_state(root)?;
    runtime_state.boost.active_until_epoch = None;
    runtime_state.boost.reason = None;
    let mut operator_env_map = runtime_env::load_runtime_env_map(root).unwrap_or_default();
    operator_env_map.retain(|key, _| !runtime_state::is_runtime_state_key(key));
    runtime_env::save_runtime_state_projection(root, &runtime_state, &operator_env_map)?;
    let context = context.map(|value| value.to_string());
    let outcome = runtime_control::execute_runtime_switch_with_context(
        root,
        model,
        preset,
        context.as_deref(),
    )?;
    Ok(format!(
        "Settings saved and runtime updated: {} ({}).",
        outcome.active_model,
        format!("{:?}", outcome.phase).to_ascii_lowercase()
    ))
}

fn load_settings_items(root: &Path) -> Vec<SettingItem> {
    let env_map = runtime_env::effective_runtime_env_map(root).unwrap_or_default();
    let current_runtime_state = runtime_state::load_or_resolve_runtime_state(root).ok();
    let inferred_chat_source = current_runtime_state
        .as_ref()
        .map(|state| state.source.as_env_value().to_string())
        .unwrap_or_else(|| infer_chat_source(&env_map));
    let inferred_api_provider = infer_api_provider(&env_map);
    let inferred_local_runtime = current_runtime_state
        .as_ref()
        .map(|state| state.local_runtime.as_env_value().to_string())
        .unwrap_or_else(|| infer_local_runtime(&env_map));
    let mut choices_env_map = env_map.clone();
    choices_env_map.insert("CTOX_CHAT_SOURCE".to_string(), inferred_chat_source.clone());
    choices_env_map.insert(
        "CTOX_LOCAL_RUNTIME".to_string(),
        inferred_local_runtime.clone(),
    );
    choices_env_map.insert(
        "CTOX_API_PROVIDER".to_string(),
        inferred_api_provider.clone(),
    );
    let chat_model_choices = supported_chat_model_choices(root, &choices_env_map);
    let chat_family_choices = supported_local_chat_family_choices(root, &choices_env_map);
    let active_family = selected_local_chat_family(&env_map)
        .or_else(|| {
            current_runtime_state
                .as_ref()
                .and_then(|state| state.base_or_selected_model())
                .and_then(engine::chat_model_family_for_model)
                .map(|family| family.label().to_string())
        })
        .filter(|value| choice_contains(&chat_family_choices, value))
        .or_else(|| {
            chat_family_choices
                .first()
                .map(|value| (*value).to_string())
        })
        .unwrap_or_else(|| default_local_chat_family_label().to_string());
    let active_model = current_runtime_state
        .as_ref()
        .and_then(|state| state.base_or_selected_model().map(ToOwned::to_owned))
        .or_else(|| runtime_env::configured_chat_model_from_map(&env_map))
        .filter(|value| {
            choice_contains(&chat_model_choices, value)
                || (inferred_chat_source.eq_ignore_ascii_case("api")
                    && engine::api_provider_supports_model(&inferred_api_provider, value))
        })
        .or_else(|| chat_model_choices.first().map(|value| (*value).to_string()))
        .unwrap_or_else(|| default_active_model().to_string());
    let boost_choices = supported_boost_model_choices(root, &choices_env_map);
    let boost_model = current_runtime_state
        .as_ref()
        .and_then(|state| state.boost.model.clone())
        .or_else(|| env_map.get("CTOX_CHAT_MODEL_BOOST").cloned())
        .unwrap_or_default();
    let azure_foundry_endpoint = env_map
        .get(AZURE_FOUNDRY_ENDPOINT_KEY)
        .cloned()
        .or_else(|| {
            current_runtime_state
                .as_ref()
                .filter(|state| {
                    runtime_state::api_provider_for_runtime_state(state)
                        .eq_ignore_ascii_case("azure_foundry")
                })
                .map(|state| state.upstream_base_url.clone())
        })
        .unwrap_or_default();
    let azure_foundry_deployment_id = env_map
        .get(AZURE_FOUNDRY_DEPLOYMENT_ID_KEY)
        .cloned()
        .or_else(|| {
            inferred_api_provider
                .eq_ignore_ascii_case("azure_foundry")
                .then(|| active_model.clone())
        })
        .unwrap_or_default();
    let boost_minutes = env_map
        .get("CTOX_BOOST_DEFAULT_MINUTES")
        .cloned()
        .unwrap_or_else(|| "20".to_string());
    let resolved_install_root = runtime_env::env_or_config(root, "CTOX_INSTALL_ROOT")
        .filter(|value| !value.trim().is_empty())
        .or_else(|| resolved_install_root_for_settings(root).map(|path| path.display().to_string()))
        .unwrap_or_default();
    let resolved_state_root = persisted_path_setting(
        root,
        "CTOX_STATE_ROOT",
        resolved_state_root_for_settings(root),
    );
    let resolved_cache_root = persisted_path_setting(
        root,
        "CTOX_CACHE_ROOT",
        resolved_cache_root_for_settings(root),
    );
    let resolved_bin_dir = persisted_path_setting(root, "CTOX_BIN_DIR", default_bin_dir());
    let resolved_skills_root = persisted_path_setting(
        root,
        "CTOX_SKILLS_ROOT",
        resolved_state_root_for_settings(root).join("skills"),
    );
    let resolved_generated_skills_root = persisted_path_setting(
        root,
        "CTOX_GENERATED_SKILLS_ROOT",
        resolved_state_root_for_settings(root).join("generated-skills"),
    );
    let resolved_tools_root = persisted_path_setting(
        root,
        "CTOX_TOOLS_ROOT",
        resolved_state_root_for_settings(root).join("tools"),
    );
    let resolved_dependencies_root = persisted_path_setting(
        root,
        "CTOX_DEPENDENCIES_ROOT",
        resolved_state_root_for_settings(root).join("dependencies"),
    );
    vec![
        SettingItem {
            key: "CTOX_SERVICE_TOGGLE",
            label: "CTOX Loop",
            value: String::new(),
            saved_value: String::new(),
            secret: false,
            choices: Vec::new(),
            help: "Start or stop the CTOX background loop from within the TUI.",
            kind: SettingKind::ServiceToggle,
        },
        SettingItem {
            key: service::working_hours::ENABLED_KEY,
            label: "Work Hours",
            value: env_map
                .get(service::working_hours::ENABLED_KEY)
                .cloned()
                .unwrap_or_else(|| "off".to_string()),
            saved_value: env_map
                .get(service::working_hours::ENABLED_KEY)
                .cloned()
                .unwrap_or_else(|| "off".to_string()),
            secret: false,
            choices: WORK_HOURS_CHOICES.to_vec(),
            help: "When on, CTOX only accepts and starts work inside the configured local-time window.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: service::working_hours::START_KEY,
            label: "Work Start",
            value: env_map
                .get(service::working_hours::START_KEY)
                .cloned()
                .unwrap_or_else(|| service::working_hours::DEFAULT_START.to_string()),
            saved_value: env_map
                .get(service::working_hours::START_KEY)
                .cloned()
                .unwrap_or_else(|| service::working_hours::DEFAULT_START.to_string()),
            secret: false,
            choices: Vec::new(),
            help: "Local start time in HH:MM, for example 08:00.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: service::working_hours::END_KEY,
            label: "Work End",
            value: env_map
                .get(service::working_hours::END_KEY)
                .cloned()
                .unwrap_or_else(|| service::working_hours::DEFAULT_END.to_string()),
            saved_value: env_map
                .get(service::working_hours::END_KEY)
                .cloned()
                .unwrap_or_else(|| service::working_hours::DEFAULT_END.to_string()),
            secret: false,
            choices: Vec::new(),
            help: "Local end time in HH:MM, for example 18:00. Overnight windows like 22:00-06:00 are valid.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_API_PROVIDER",
            label: "Provider",
            value: inferred_api_provider.clone(),
            saved_value: inferred_api_provider,
            secret: false,
            choices: API_PROVIDER_CHOICES.to_vec(),
            help: "Choose whether the base model should come from the local runtime or a remote API provider.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: OPENAI_AUTH_MODE_KEY,
            label: "OpenAI Auth",
            value: env_map
                .get(OPENAI_AUTH_MODE_KEY)
                .cloned()
                .unwrap_or_else(|| DEFAULT_OPENAI_AUTH_MODE.to_string()),
            saved_value: env_map
                .get(OPENAI_AUTH_MODE_KEY)
                .cloned()
                .unwrap_or_else(|| DEFAULT_OPENAI_AUTH_MODE.to_string()),
            secret: false,
            choices: OPENAI_AUTH_MODE_CHOICES.to_vec(),
            help: "Choose api_key for billed OpenAI API credentials or chatgpt_subscription for Codex/ChatGPT OAuth credentials stored by Codex.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: AZURE_FOUNDRY_ENDPOINT_KEY,
            label: "Foundry Endpoint",
            value: azure_foundry_endpoint.clone(),
            saved_value: azure_foundry_endpoint,
            secret: false,
            choices: Vec::new(),
            help: "Azure Foundry resource endpoint, for example https://name.openai.azure.com. CTOX appends /openai/v1 when needed.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: AZURE_FOUNDRY_DEPLOYMENT_ID_KEY,
            label: "Deployment ID",
            value: azure_foundry_deployment_id.clone(),
            saved_value: azure_foundry_deployment_id,
            secret: false,
            choices: Vec::new(),
            help: "Azure Foundry deployment ID. CTOX uses this as the model name for requests.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: AZURE_FOUNDRY_TOKEN_KEY,
            label: "Foundry Token",
            value: env_map
                .get(AZURE_FOUNDRY_TOKEN_KEY)
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get(AZURE_FOUNDRY_TOKEN_KEY)
                .cloned()
                .unwrap_or_default(),
            secret: true,
            choices: Vec::new(),
            help: "Azure Foundry API token. Stored in the encrypted credential store.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_LOCAL_RUNTIME",
            label: "Local Runtime",
            value: inferred_local_runtime.clone(),
            saved_value: inferred_local_runtime,
            secret: false,
            choices: LOCAL_RUNTIME_CHOICES.to_vec(),
            help: "Local runtime family for the selected local chat model. Candle is the only supported local inference engine.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "OPENAI_API_KEY",
            label: "OpenAI Token",
            value: env_map.get("OPENAI_API_KEY").cloned().unwrap_or_default(),
            saved_value: env_map.get("OPENAI_API_KEY").cloned().unwrap_or_default(),
            secret: true,
            choices: Vec::new(),
            help: "OpenAI API token. When present, OpenAI models become available where supported.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "ANTHROPIC_API_KEY",
            label: "Anthropic Token",
            value: env_map
                .get("ANTHROPIC_API_KEY")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("ANTHROPIC_API_KEY")
                .cloned()
                .unwrap_or_default(),
            secret: true,
            choices: Vec::new(),
            help: "Anthropic API token. When present, Claude models become available where supported.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "OPENROUTER_API_KEY",
            label: "OpenRouter Token",
            value: env_map
                .get("OPENROUTER_API_KEY")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("OPENROUTER_API_KEY")
                .cloned()
                .unwrap_or_default(),
            secret: true,
            choices: Vec::new(),
            help: "OpenRouter API token. When present, OpenRouter responses models become available where supported.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_CHAT_MODEL_FAMILY",
            label: "Model Family",
            value: active_family.clone(),
            saved_value: selected_local_chat_family(&env_map)
                .or_else(|| {
                    current_runtime_state
                        .as_ref()
                        .and_then(|state| state.base_or_selected_model())
                        .and_then(engine::chat_model_family_for_model)
                        .map(|family| family.label().to_string())
                })
                .filter(|value| choice_contains(&chat_family_choices, value))
                .or_else(|| {
                    chat_family_choices
                        .first()
                        .map(|value| (*value).to_string())
                })
                .unwrap_or_else(|| default_local_chat_family_label().to_string()),
            secret: false,
            choices: chat_family_choices,
            help: "Choose the local model family. CTOX resolves one hardware-qualified concrete variant from this family, and the preset then retunes that same model for quality or throughput.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_CHAT_MODEL",
            label: "Base Model",
            value: active_model,
            saved_value: current_runtime_state
                .as_ref()
                .and_then(|state| state.base_or_selected_model().map(ToOwned::to_owned))
                .or_else(|| runtime_env::configured_chat_model_from_map(&env_map))
                .filter(|value| choice_contains(&chat_model_choices, value))
                .or_else(|| chat_model_choices.first().map(|value| (*value).to_string()))
                .unwrap_or_else(|| default_active_model().to_string()),
            secret: false,
            choices: chat_model_choices,
            help: "Selected base chat model for the chosen provider.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_CHAT_LOCAL_PRESET",
            label: "Chat Preset",
            value: runtime_plan::ChatPreset::from_label(
                current_runtime_state
                    .as_ref()
                    .and_then(|state| state.local_preset.as_deref())
                    .or_else(|| env_map.get("CTOX_CHAT_LOCAL_PRESET").map(String::as_str))
                    .unwrap_or(DEFAULT_CHAT_PRESET),
            )
            .label()
            .to_string(),
            saved_value: runtime_plan::ChatPreset::from_label(
                current_runtime_state
                    .as_ref()
                    .and_then(|state| state.local_preset.as_deref())
                    .or_else(|| env_map.get("CTOX_CHAT_LOCAL_PRESET").map(String::as_str))
                    .unwrap_or(DEFAULT_CHAT_PRESET),
            )
            .label()
            .to_string(),
            secret: false,
            choices: CHAT_PRESET_CHOICES.to_vec(),
            help: "Choose the active chat preset. For local Candle runtimes it retunes planning and compaction; for GPT-OSS and GPT-5.4-family models it also fixes reasoning effort as part of the preset.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_CHAT_MODEL_MAX_CONTEXT",
            label: "Context Window",
            value: runtime_plan::format_chat_context_choice(
                current_runtime_state
                    .as_ref()
                    .and_then(|state| state.configured_context_tokens)
                    .or_else(|| {
                        env_map
                            .get("CTOX_CHAT_MODEL_MAX_CONTEXT")
                            .and_then(|value| runtime_plan::parse_chat_context_tokens(value))
                    })
                    .unwrap_or_else(runtime_plan::default_chat_context_tokens),
            ),
            saved_value: runtime_plan::format_chat_context_choice(
                current_runtime_state
                    .as_ref()
                    .and_then(|state| state.configured_context_tokens)
                    .or_else(|| {
                        env_map
                            .get("CTOX_CHAT_MODEL_MAX_CONTEXT")
                            .and_then(|value| runtime_plan::parse_chat_context_tokens(value))
                    })
                    .unwrap_or_else(runtime_plan::default_chat_context_tokens),
            ),
            secret: false,
            choices: runtime_plan::supported_chat_context_choices(),
            help: "Choose the target chat context window. CTOX derives runtime planning, compaction floors, and refresh budgets from this setting.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_CHAT_TURN_TIMEOUT_SECS",
            label: "Turn Timeout",
            value: env_map
                .get("CTOX_CHAT_TURN_TIMEOUT_SECS")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTOX_CHAT_TURN_TIMEOUT_SECS")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: vec!["180", "300", "600", "900", "1200", "1800", "2400", "3600"],
            help: "Maximum wall-clock seconds for a single chat turn. Empty falls back to the built-in default (900s for local Candle, 180s for remote APIs). Raise to 1800s+ for Azure-Foundry-backed owner-visible flows where reviewed-founder-send pipelines can wall-clock 10-30 minutes per turn.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_CHAT_SKILL_PRESET",
            label: "Skill Preset",
            value: runtime_state::ChatSkillPreset::from_label(
                env_map
                    .get("CTOX_CHAT_SKILL_PRESET")
                    .map(String::as_str)
                    .unwrap_or(DEFAULT_CHAT_SKILL_PRESET),
            )
            .label()
            .to_string(),
            saved_value: runtime_state::ChatSkillPreset::from_label(
                env_map
                    .get("CTOX_CHAT_SKILL_PRESET")
                    .map(String::as_str)
                    .unwrap_or(DEFAULT_CHAT_SKILL_PRESET),
            )
            .label()
            .to_string(),
            secret: false,
            choices: CHAT_SKILL_PRESET_CHOICES.to_vec(),
            help: "Choose the agent behavior preset. Standard keeps the full CTOX agent contract; Simple switches to a lighter small-step mode for weaker models.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_REFRESH_OUTPUT_BUDGET_PCT",
            label: "Refresh Budget",
            value: env_map
                .get("CTOX_REFRESH_OUTPUT_BUDGET_PCT")
                .cloned()
                .unwrap_or_else(|| "15".to_string()),
            saved_value: env_map
                .get("CTOX_REFRESH_OUTPUT_BUDGET_PCT")
                .cloned()
                .unwrap_or_else(|| "15".to_string()),
            secret: false,
            choices: vec!["5", "10", "15", "20", "25"],
            help: "Maximum assistant output tokens between continuity refreshes, as a percent of the model context window. Higher values let the model run longer multi-turn without refresh (KV-cache-friendly); lower values guard against self-feeding drift on long generations. State-transition boundaries always refresh regardless of this setting.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_AUTONOMY_LEVEL",
            label: "Autonomy",
            value: env_map
                .get("CTOX_AUTONOMY_LEVEL")
                .cloned()
                .unwrap_or_else(|| "balanced".to_string()),
            saved_value: env_map
                .get("CTOX_AUTONOMY_LEVEL")
                .cloned()
                .unwrap_or_else(|| "balanced".to_string()),
            secret: false,
            choices: vec!["progressive", "balanced", "defensive"],
            help: "How eagerly CTOX asks for owner approval before acting. Progressive: execute directly, auto-close approval-gate items (use for unattended / non-interactive runs). Balanced (default): approval-gate only for genuinely high-impact moves. Defensive: ask for approval on anything touching infrastructure, external services, or irreversible state, and nag faster.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_CTO_OPERATING_MODE_PROMPT",
            label: "CTO Contract",
            value: env_map
                .get("CTOX_CTO_OPERATING_MODE_PROMPT")
                .cloned()
                .unwrap_or_else(|| DEFAULT_CTO_OPERATING_MODE_PROMPT.trim().to_string()),
            saved_value: env_map
                .get("CTOX_CTO_OPERATING_MODE_PROMPT")
                .cloned()
                .unwrap_or_else(|| DEFAULT_CTO_OPERATING_MODE_PROMPT.trim().to_string()),
            secret: false,
            choices: Vec::new(),
            help: "Editable CTO operating contract injected into the system prompt. Use Enter to open the full-screen editor and tune how proactive, product-minded, and research-driven CTOX should be.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_CHAT_MODEL_BOOST",
            label: "Boost Model",
            value: boost_model.clone(),
            saved_value: boost_model,
            secret: false,
            choices: boost_choices,
            help: "Optional stronger model for temporary boost leases. CTOX can request this model when it is genuinely stuck and then fall back automatically after the lease expires.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_BOOST_DEFAULT_MINUTES",
            label: "Boost TTL",
            value: boost_minutes.clone(),
            saved_value: boost_minutes,
            secret: false,
            choices: vec!["10", "15", "20", "30", "45", "60"],
            help: "Default lifetime in minutes for an automatic boost lease before the runtime falls back to the base model.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_CHAT_SOURCE",
            label: "Chat Source",
            value: inferred_chat_source.clone(),
            saved_value: inferred_chat_source,
            secret: false,
            choices: vec!["local", "api"],
            help: "Internal source selector kept in sync with Provider.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_EMBEDDING_MODEL",
            label: "Embed Model",
            value: engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Embedding,
                current_runtime_state
                    .as_ref()
                    .and_then(|state| state.embedding.configured_model.as_deref())
                    .or_else(|| env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str)),
            )
            .choice
            .to_string(),
            saved_value: engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Embedding,
                current_runtime_state
                    .as_ref()
                    .and_then(|state| state.embedding.configured_model.as_deref())
                    .or_else(|| env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str)),
            )
            .choice
            .to_string(),
            secret: false,
            choices: supported_embedding_model_choices(),
            help: "Persistent embedding sidecar. GPU keeps the current fast path; CPU keeps embeddings available on hosts without CUDA.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_STT_MODEL",
            label: "STT Model",
            value: engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Stt,
                current_runtime_state
                    .as_ref()
                    .and_then(|state| state.transcription.configured_model.as_deref())
                    .or_else(|| env_map.get("CTOX_STT_MODEL").map(String::as_str)),
            )
            .choice
            .to_string(),
            saved_value: engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Stt,
                current_runtime_state
                    .as_ref()
                    .and_then(|state| state.transcription.configured_model.as_deref())
                    .or_else(|| env_map.get("CTOX_STT_MODEL").map(String::as_str)),
            )
            .choice
            .to_string(),
            secret: false,
            choices: supported_stt_model_choices(),
            help: "Speech-to-text sidecar. Only Voxtral Mini 4B Realtime is supported for transcript quality; legacy CPU STT choices fall back to Voxtral.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_TTS_MODEL",
            label: "TTS Model",
            value: engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Tts,
                current_runtime_state
                    .as_ref()
                    .and_then(|state| state.speech.configured_model.as_deref())
                    .or_else(|| env_map.get("CTOX_TTS_MODEL").map(String::as_str)),
            )
            .choice
            .to_string(),
            saved_value: engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Tts,
                current_runtime_state
                    .as_ref()
                    .and_then(|state| state.speech.configured_model.as_deref())
                    .or_else(|| env_map.get("CTOX_TTS_MODEL").map(String::as_str)),
            )
            .choice
            .to_string(),
            secret: false,
            choices: supported_tts_model_choices(),
            help: "Text-to-speech sidecar. GPU now defaults to native Voxtral with Q4K; CPU standardizes on Piper over Speaches, with language-specific voices such as German, French, and English.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_OWNER_NAME",
            label: "Owner Name",
            value: env_map
                .get("CTOX_OWNER_NAME")
                .cloned()
                .unwrap_or_else(|| "Michael Welsch".to_string()),
            saved_value: env_map
                .get("CTOX_OWNER_NAME")
                .cloned()
                .unwrap_or_else(|| "Michael Welsch".to_string()),
            secret: false,
            choices: Vec::new(),
            help: "Owner name used in prompts and communication identity.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_OWNER_EMAIL_ADDRESS",
            label: "Owner E-Mail",
            value: env_map
                .get("CTOX_OWNER_EMAIL_ADDRESS")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTOX_OWNER_EMAIL_ADDRESS")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Owner mailbox for full administrative mail authority. Additional admins and general domain access are configured in the next fields.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_FOUNDER_EMAIL_ADDRESSES",
            label: "Founder E-Mails",
            value: env_map
                .get("CTOX_FOUNDER_EMAIL_ADDRESSES")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTOX_FOUNDER_EMAIL_ADDRESSES")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Comma/newline list of founder mailboxes that should bypass the employee domain filter and be treated as high-priority strategic senders.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_FOUNDER_EMAIL_ROLES",
            label: "Founder Roles",
            value: env_map
                .get("CTOX_FOUNDER_EMAIL_ROLES")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTOX_FOUNDER_EMAIL_ROLES")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Comma/newline list of founder role mappings in the form email=role. Example: michael@example.com=CEO / Founder",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_ALLOWED_EMAIL_DOMAIN",
            label: "Allowed Domain",
            value: env_map
                .get("CTOX_ALLOWED_EMAIL_DOMAIN")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTOX_ALLOWED_EMAIL_DOMAIN")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Employee/support mail domain. Founder/owner/admin addresses are matched explicitly and do not need to live on this domain. Leave blank to derive it from the owner e-mail domain.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_EMAIL_ADMIN_POLICIES",
            label: "Mail Admins",
            value: env_map
                .get("CTOX_EMAIL_ADMIN_POLICIES")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTOX_EMAIL_ADMIN_POLICIES")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Comma/newline list like admin@domain:sudo, helpdesk@domain:nosudo. Owner keeps full rights; listed admins may do admin work by mail, with optional sudo.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_REMOTE_BRIDGE_MODE",
            label: "Desktop Remote",
            value: env_map
                .get("CTOX_REMOTE_BRIDGE_MODE")
                .cloned()
                .unwrap_or_else(|| DEFAULT_REMOTE_BRIDGE_MODE.to_string()),
            saved_value: env_map
                .get("CTOX_REMOTE_BRIDGE_MODE")
                .cloned()
                .unwrap_or_else(|| DEFAULT_REMOTE_BRIDGE_MODE.to_string()),
            secret: false,
            choices: REMOTE_BRIDGE_MODE_CHOICES.to_vec(),
            help: "Expose this CTOX installation to the desktop wrapper. Enable Remote-WebRTC only when this instance should accept restricted remote TUI sessions.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_WEBRTC_SIGNALING_URL",
            label: "Signaling Server",
            value: env_map
                .get("CTOX_WEBRTC_SIGNALING_URL")
                .cloned()
                .unwrap_or_else(|| "wss://api.metricspace.org/signal".to_string()),
            saved_value: env_map
                .get("CTOX_WEBRTC_SIGNALING_URL")
                .cloned()
                .unwrap_or_else(|| "wss://api.metricspace.org/signal".to_string()),
            secret: false,
            choices: Vec::new(),
            help: "WebRTC signaling endpoint used by the CTOX desktop bridge.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_WEBRTC_ROOM",
            label: "Remote Room",
            value: env_map.get("CTOX_WEBRTC_ROOM").cloned().unwrap_or_default(),
            saved_value: env_map.get("CTOX_WEBRTC_ROOM").cloned().unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Room name that the CTOX desktop app joins for this installation.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_WEBRTC_PASSWORD",
            label: "Remote Password",
            value: env_map
                .get("CTOX_WEBRTC_PASSWORD")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTOX_WEBRTC_PASSWORD")
                .cloned()
                .unwrap_or_default(),
            secret: true,
            choices: Vec::new(),
            help: "Shared secret for the restricted CTOX desktop WebRTC session.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_OWNER_PREFERRED_CHANNEL",
            label: "Communication",
            value: env_map
                .get("CTOX_OWNER_PREFERRED_CHANNEL")
                .cloned()
                .unwrap_or_else(|| DEFAULT_COMMUNICATION_PATH.to_string()),
            saved_value: env_map
                .get("CTOX_OWNER_PREFERRED_CHANNEL")
                .cloned()
                .unwrap_or_else(|| DEFAULT_COMMUNICATION_PATH.to_string()),
            secret: false,
            choices: COMMUNICATION_PATH_CHOICES.to_vec(),
            help: "Preferred communication path for CTOX replies.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_EMAIL_ADDRESS",
            label: "E-Mail Address",
            value: env_map
                .get("CTO_EMAIL_ADDRESS")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_EMAIL_ADDRESS")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Mailbox address for e-mail communication.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_EMAIL_PASSWORD",
            label: "E-Mail Password",
            value: env_map
                .get("CTO_EMAIL_PASSWORD")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_EMAIL_PASSWORD")
                .cloned()
                .unwrap_or_default(),
            secret: true,
            choices: Vec::new(),
            help: "Mailbox password for IMAP/SMTP communication.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_EMAIL_PROVIDER",
            label: "E-Mail Protocol",
            value: env_map
                .get("CTO_EMAIL_PROVIDER")
                .cloned()
                .unwrap_or_else(|| "imap".to_string()),
            saved_value: env_map
                .get("CTO_EMAIL_PROVIDER")
                .cloned()
                .unwrap_or_else(|| "imap".to_string()),
            secret: false,
            choices: EMAIL_PROVIDER_CHOICES.to_vec(),
            help: "Select the e-mail access protocol.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_EMAIL_IMAP_HOST",
            label: "IMAP Host",
            value: env_map
                .get("CTO_EMAIL_IMAP_HOST")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_EMAIL_IMAP_HOST")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "IMAP hostname for mailbox sync.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_EMAIL_IMAP_PORT",
            label: "IMAP Port",
            value: env_map
                .get("CTO_EMAIL_IMAP_PORT")
                .cloned()
                .unwrap_or_else(|| "993".to_string()),
            saved_value: env_map
                .get("CTO_EMAIL_IMAP_PORT")
                .cloned()
                .unwrap_or_else(|| "993".to_string()),
            secret: false,
            choices: Vec::new(),
            help: "IMAP port.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_EMAIL_SMTP_HOST",
            label: "SMTP Host",
            value: env_map
                .get("CTO_EMAIL_SMTP_HOST")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_EMAIL_SMTP_HOST")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "SMTP hostname for outgoing mail.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_EMAIL_SMTP_PORT",
            label: "SMTP Port",
            value: env_map
                .get("CTO_EMAIL_SMTP_PORT")
                .cloned()
                .unwrap_or_else(|| "587".to_string()),
            saved_value: env_map
                .get("CTO_EMAIL_SMTP_PORT")
                .cloned()
                .unwrap_or_else(|| "587".to_string()),
            secret: false,
            choices: Vec::new(),
            help: "SMTP port.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_EMAIL_GRAPH_USER",
            label: "Graph User",
            value: env_map
                .get("CTO_EMAIL_GRAPH_USER")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_EMAIL_GRAPH_USER")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Microsoft Graph mailbox user or principal.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_EMAIL_EWS_URL",
            label: "EWS URL",
            value: env_map
                .get("CTO_EMAIL_EWS_URL")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_EMAIL_EWS_URL")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Exchange Web Services endpoint.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_EMAIL_EWS_AUTH_TYPE",
            label: "EWS Auth",
            value: env_map
                .get("CTO_EMAIL_EWS_AUTH_TYPE")
                .cloned()
                .unwrap_or_else(|| "basic".to_string()),
            saved_value: env_map
                .get("CTO_EMAIL_EWS_AUTH_TYPE")
                .cloned()
                .unwrap_or_else(|| "basic".to_string()),
            secret: false,
            choices: EMAIL_EWS_AUTH_CHOICES.to_vec(),
            help: "Authentication mode for EWS.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_EMAIL_EWS_USERNAME",
            label: "EWS User",
            value: env_map
                .get("CTO_EMAIL_EWS_USERNAME")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_EMAIL_EWS_USERNAME")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Username for EWS authentication.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_JAMI_PROFILE_NAME",
            label: "Jami Name",
            value: env_map
                .get("CTO_JAMI_PROFILE_NAME")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_JAMI_PROFILE_NAME")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Displayed Jami profile name.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_JAMI_ACCOUNT_ID",
            label: "Jami Account",
            value: env_map
                .get("CTO_JAMI_ACCOUNT_ID")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_JAMI_ACCOUNT_ID")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Jami account id or share URI.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_WHATSAPP_DEVICE_DB",
            label: "WA Device DB",
            value: env_map
                .get("CTO_WHATSAPP_DEVICE_DB")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_WHATSAPP_DEVICE_DB")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "SQLite device store for WhatsApp linked-device pairing.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_WHATSAPP_PUSH_NAME",
            label: "WA Push Name",
            value: env_map
                .get("CTO_WHATSAPP_PUSH_NAME")
                .cloned()
                .unwrap_or_else(|| "CTOX".to_string()),
            saved_value: env_map
                .get("CTO_WHATSAPP_PUSH_NAME")
                .cloned()
                .unwrap_or_else(|| "CTOX".to_string()),
            secret: false,
            choices: Vec::new(),
            help: "Display name advertised by the WhatsApp linked device.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_WHATSAPP_SYNC_TIMEOUT_SECONDS",
            label: "WA Sync Sec",
            value: env_map
                .get("CTO_WHATSAPP_SYNC_TIMEOUT_SECONDS")
                .cloned()
                .unwrap_or_else(|| "8".to_string()),
            saved_value: env_map
                .get("CTO_WHATSAPP_SYNC_TIMEOUT_SECONDS")
                .cloned()
                .unwrap_or_else(|| "8".to_string()),
            secret: false,
            choices: Vec::new(),
            help: "Seconds a WhatsApp sync call listens for inbound events.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_TEAMS_USERNAME",
            label: "Teams User",
            value: env_map
                .get("CTO_TEAMS_USERNAME")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_TEAMS_USERNAME")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Microsoft 365 email (e.g. user@company.com).",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_TEAMS_PASSWORD",
            label: "Teams Pass",
            value: env_map
                .get("CTO_TEAMS_PASSWORD")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_TEAMS_PASSWORD")
                .cloned()
                .unwrap_or_default(),
            secret: true,
            choices: Vec::new(),
            help: "Microsoft 365 password.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_TEAMS_TENANT_ID",
            label: "Tenant (opt.)",
            value: env_map
                .get("CTO_TEAMS_TENANT_ID")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_TEAMS_TENANT_ID")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Azure AD tenant ID (optional, auto-detected from email domain).",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_TEAMS_TEAM_ID",
            label: "Team ID (opt.)",
            value: env_map
                .get("CTO_TEAMS_TEAM_ID")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_TEAMS_TEAM_ID")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Team ID to sync channel messages from (optional).",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTO_TEAMS_CHANNEL_ID",
            label: "Channel (opt.)",
            value: env_map
                .get("CTO_TEAMS_CHANNEL_ID")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTO_TEAMS_CHANNEL_ID")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "Channel ID within the team to monitor (optional).",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_INSTALL_ROOT",
            label: "Install Root",
            value: resolved_install_root.clone(),
            saved_value: resolved_install_root,
            secret: false,
            choices: Vec::new(),
            help: "Managed release root for the live CTOX installation. Keep this explicit so the service does not spread across arbitrary shell defaults.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_STATE_ROOT",
            label: "State Root",
            value: resolved_state_root.clone(),
            saved_value: resolved_state_root,
            secret: false,
            choices: Vec::new(),
            help: "Primary mutable runtime root. SQLite, generated skills, queues, logs, and other live CTOX state should remain underneath this directory.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_CACHE_ROOT",
            label: "Cache Root",
            value: resolved_cache_root.clone(),
            saved_value: resolved_cache_root,
            secret: false,
            choices: Vec::new(),
            help: "Download and build cache root for install/update flows.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_BIN_DIR",
            label: "Binary Dir",
            value: resolved_bin_dir.clone(),
            saved_value: resolved_bin_dir,
            secret: false,
            choices: Vec::new(),
            help: "Directory that should contain the public CTOX launchers such as `ctox` and `ctox-desktop`.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_SKILLS_ROOT",
            label: "Skills Root",
            value: resolved_skills_root.clone(),
            saved_value: resolved_skills_root,
            secret: false,
            choices: Vec::new(),
            help: "Mutable installed/authored skill bundles root. CTOX now prefers this explicit path instead of roaming through `~/.codex` or other home-directory conventions.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_GENERATED_SKILLS_ROOT",
            label: "Generated Skills",
            value: resolved_generated_skills_root.clone(),
            saved_value: resolved_generated_skills_root,
            secret: false,
            choices: Vec::new(),
            help: "Runtime-generated skill bundle root. Keep generated skills under the CTOX state tree instead of scattering them elsewhere on the host.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_TOOLS_ROOT",
            label: "Tools Root",
            value: resolved_tools_root.clone(),
            saved_value: resolved_tools_root,
            secret: false,
            choices: Vec::new(),
            help: "Canonical root for CTOX-managed helper tools and bundled operational binaries.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_DEPENDENCIES_ROOT",
            label: "Dependencies",
            value: resolved_dependencies_root.clone(),
            saved_value: resolved_dependencies_root,
            secret: false,
            choices: Vec::new(),
            help: "Canonical root for heavyweight downloaded dependencies and model-side assets that must not leak across the wider system.",
            kind: SettingKind::Env,
        },
        // --- Phase 5: compact policy knobs ---------------------------------
        SettingItem {
            key: "CTOX_USE_DIRECT_SESSION",
            label: "Direct Session",
            value: env_map
                .get("CTOX_USE_DIRECT_SESSION")
                .cloned()
                .unwrap_or_else(|| "false".to_string()),
            saved_value: env_map
                .get("CTOX_USE_DIRECT_SESSION")
                .cloned()
                .unwrap_or_else(|| "false".to_string()),
            secret: false,
            choices: vec!["false", "true"],
            help: "Run inference via the in-process ctox-core library.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_COMPACT_TRIGGER",
            label: "Compact Trigger",
            value: env_map
                .get("CTOX_COMPACT_TRIGGER")
                .cloned()
                .unwrap_or_else(|| "off".to_string()),
            saved_value: env_map
                .get("CTOX_COMPACT_TRIGGER")
                .cloned()
                .unwrap_or_else(|| "off".to_string()),
            secret: false,
            choices: vec!["off", "adaptive", "fixed"],
            help: "When to compact: off / adaptive (token-usage threshold) / fixed (every N turns). Requires Direct Session.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_COMPACT_MODE",
            label: "Compact Mode",
            value: env_map
                .get("CTOX_COMPACT_MODE")
                .cloned()
                .unwrap_or_else(|| "mid-task".to_string()),
            saved_value: env_map
                .get("CTOX_COMPACT_MODE")
                .cloned()
                .unwrap_or_else(|| "mid-task".to_string()),
            secret: false,
            choices: vec!["mid-task", "forced-followup"],
            help: "How to compact: mid-task (ThreadCompactStart, same thread) / forced-followup (unsubscribe + enqueue follow-up slice).",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_COMPACT_FIXED_INTERVAL",
            label: "Compact Fixed N",
            value: env_map
                .get("CTOX_COMPACT_FIXED_INTERVAL")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTOX_COMPACT_FIXED_INTERVAL")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "For fixed trigger: compact every N completed turns.",
            kind: SettingKind::Env,
        },
        SettingItem {
            key: "CTOX_COMPACT_ADAPTIVE_THRESHOLD",
            label: "Compact Threshold",
            value: env_map
                .get("CTOX_COMPACT_ADAPTIVE_THRESHOLD")
                .cloned()
                .unwrap_or_default(),
            saved_value: env_map
                .get("CTOX_COMPACT_ADAPTIVE_THRESHOLD")
                .cloned()
                .unwrap_or_default(),
            secret: false,
            choices: Vec::new(),
            help: "For adaptive trigger: token-usage ratio (0.0-1.0) that fires a compact. Default 0.70.",
            kind: SettingKind::Env,
        },
    ]
}

fn load_secret_items(root: &Path) -> Vec<SecretItem> {
    let mut items = secrets::list_secret_records(root, None)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|record| {
            let value =
                secrets::read_secret_value(root, &record.scope, &record.secret_name).ok()?;
            Some(SecretItem {
                scope: record.scope,
                name: record.secret_name,
                description: record.description,
                metadata: record.metadata,
                created_at: record.created_at,
                updated_at: record.updated_at,
                value: value.clone(),
                saved_value: value,
            })
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.scope
            .cmp(&right.scope)
            .then_with(|| left.name.cmp(&right.name))
    });
    items
}

fn settings_map_from_items(items: &[SettingItem]) -> BTreeMap<String, String> {
    let mut env_map = BTreeMap::new();
    for item in items {
        if item.kind != SettingKind::Env {
            continue;
        }
        let trimmed = item.value.trim();
        if trimmed.is_empty() {
            continue;
        }
        env_map.insert(item.key.to_string(), trimmed.to_string());
    }
    env_map
}

fn is_secret_backed_runtime_setting(key: &str) -> bool {
    matches!(
        key,
        "OPENAI_API_KEY" | "ANTHROPIC_API_KEY" | "OPENROUTER_API_KEY" | "AZURE_FOUNDRY_API_KEY"
    )
}

fn current_context_tokens(db_path: &Path, max_context: usize) -> Result<usize> {
    let engine = lcm::LcmEngine::open(db_path, lcm::LcmConfig::default())?;
    let decision =
        engine.evaluate_compaction(turn_loop::CHAT_CONVERSATION_ID, max_context as i64)?;
    Ok(decision.current_tokens.max(0) as usize)
}

fn load_runtime_telemetry(root: &Path) -> Result<Option<RuntimeTelemetry>> {
    Ok(Some(crate::inference::gateway::current_runtime_telemetry(
        root,
    )))
}

fn read_usize_setting(settings: &BTreeMap<String, String>, key: &str, default: usize) -> usize {
    settings
        .get(key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn compute_compaction_threshold(
    max_context: usize,
    threshold_percent: usize,
    min_tokens: usize,
) -> usize {
    let percent_threshold = max_context.saturating_mul(threshold_percent.clamp(1, 99)) / 100;
    percent_threshold.max(min_tokens.min(max_context))
}

fn relevant_header_estimate_setting(key: &str) -> bool {
    matches!(
        key,
        "CTOX_CHAT_SOURCE"
            | "CTOX_LOCAL_RUNTIME"
            | "CTOX_API_PROVIDER"
            | "CTOX_OPENAI_AUTH_MODE"
            | "CTOX_AZURE_FOUNDRY_ENDPOINT"
            | "CTOX_AZURE_FOUNDRY_DEPLOYMENT_ID"
            | "AZURE_FOUNDRY_API_KEY"
            | "OPENAI_API_KEY"
            | "ANTHROPIC_API_KEY"
            | "OPENROUTER_API_KEY"
            | "CTOX_CHAT_MODEL"
            | "CTOX_CHAT_LOCAL_PRESET"
            | "CTOX_CHAT_MODEL_MAX_CONTEXT"
            | "CTOX_CHAT_SKILL_PRESET"
    )
}

fn runtime_selection_changed(
    previous_state: &runtime_state::InferenceRuntimeState,
    desired_source: &str,
    desired_local_runtime: &str,
    desired_model: Option<&str>,
    desired_preset: Option<&str>,
    desired_context: Option<u32>,
) -> bool {
    let previous_model = previous_state
        .base_or_selected_model()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let desired_model = desired_model
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if previous_state.source.as_env_value() != desired_source {
        return true;
    }
    if previous_state.source.is_local()
        && previous_state.local_runtime.as_env_value() != desired_local_runtime
    {
        return true;
    }
    if previous_model != desired_model {
        return true;
    }
    if previous_state.configured_context_tokens != desired_context {
        return true;
    }
    if previous_state.source.is_local() {
        let previous_preset = previous_state
            .local_preset
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let desired_preset = desired_preset
            .map(str::trim)
            .filter(|value| !value.is_empty());
        return previous_preset != desired_preset;
    }
    false
}

#[allow(dead_code)]
fn is_model_runtime_setting(key: &str) -> bool {
    matches!(
        key,
        "CTOX_CHAT_SOURCE"
            | "CTOX_LOCAL_RUNTIME"
            | "CTOX_API_PROVIDER"
            | "CTOX_OPENAI_AUTH_MODE"
            | "CTOX_AZURE_FOUNDRY_ENDPOINT"
            | "CTOX_AZURE_FOUNDRY_DEPLOYMENT_ID"
            | "AZURE_FOUNDRY_API_KEY"
            | "OPENAI_API_KEY"
            | "ANTHROPIC_API_KEY"
            | "OPENROUTER_API_KEY"
            | "CTOX_CHAT_MODEL_FAMILY"
            | "CTOX_CHAT_MODEL"
            | "CTOX_CHAT_LOCAL_PRESET"
            | "CTOX_CHAT_MODEL_MAX_CONTEXT"
            | "CTOX_EMBEDDING_MODEL"
            | "CTOX_STT_MODEL"
            | "CTOX_TTS_MODEL"
    )
}

fn normalize_runtime_model_settings(env_map: &mut BTreeMap<String, String>) {
    let api_provider = infer_api_provider(env_map);
    let local_runtime = infer_local_runtime(env_map);
    if api_provider.eq_ignore_ascii_case("azure_foundry") {
        if let Some(deployment_id) = env_map
            .get(AZURE_FOUNDRY_DEPLOYMENT_ID_KEY)
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                env_map
                    .get("CTOX_CHAT_MODEL")
                    .cloned()
                    .filter(|value| !engine::supports_local_chat_runtime(value))
            })
        {
            env_map.insert("CTOX_CHAT_MODEL".to_string(), deployment_id);
        }
        if let Some(base_url) = env_map
            .get(AZURE_FOUNDRY_ENDPOINT_KEY)
            .and_then(|endpoint| runtime_state::azure_foundry_responses_base_url(endpoint))
            .or_else(|| {
                env_map
                    .get("CTOX_UPSTREAM_BASE_URL")
                    .cloned()
                    .filter(|value| !value.trim().is_empty())
            })
        {
            env_map.insert("CTOX_UPSTREAM_BASE_URL".to_string(), base_url);
        }
        env_map.insert("CTOX_API_PROVIDER".to_string(), api_provider);
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "api".to_string());
        env_map.remove("CTOX_CHAT_LOCAL_PRESET");
        env_map.remove("CTOX_ENGINE_MAX_SEQ_LEN");
        env_map.remove("CTOX_ENGINE_ISQ");
        env_map.remove("CTOX_ENGINE_DISABLE_NCCL");
        env_map.remove("CTOX_ENGINE_CUDA_VISIBLE_DEVICES");
        env_map.remove("CTOX_ENGINE_DEVICE_LAYERS");
        env_map.remove("CTOX_ENGINE_MAX_SEQS");
        runtime_plan::clear_chat_plan_env(env_map);
        return;
    }
    let configured_model = runtime_env::configured_chat_model_from_map(env_map);
    let explicit_api_source = env_map
        .get("CTOX_CHAT_SOURCE")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("api"));
    let api_model_selected = configured_model
        .as_deref()
        .map(engine::is_api_chat_model)
        .unwrap_or(false);
    let use_api_base =
        !api_provider.eq_ignore_ascii_case("local") && (explicit_api_source || api_model_selected);
    if use_api_base {
        env_map.insert("CTOX_API_PROVIDER".to_string(), api_provider.clone());
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "api".to_string());
        env_map.insert(
            "CTOX_UPSTREAM_BASE_URL".to_string(),
            runtime_state::default_api_upstream_base_url_for_provider(&api_provider).to_string(),
        );
        env_map.remove("CTOX_CHAT_LOCAL_PRESET");
        env_map.remove("CTOX_ENGINE_MAX_SEQ_LEN");
        env_map.remove("CTOX_ENGINE_ISQ");
        env_map.remove("CTOX_ENGINE_DISABLE_NCCL");
        env_map.remove("CTOX_ENGINE_CUDA_VISIBLE_DEVICES");
        env_map.remove("CTOX_ENGINE_DEVICE_LAYERS");
        env_map.remove("CTOX_ENGINE_MAX_SEQS");
        runtime_plan::clear_chat_plan_env(env_map);
        return;
    }

    env_map.insert("CTOX_API_PROVIDER".to_string(), api_provider);
    env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
    env_map.insert("CTOX_LOCAL_RUNTIME".to_string(), local_runtime.clone());
    if local_runtime.eq_ignore_ascii_case("candle") {
        env_map
            .entry("CTOX_CHAT_LOCAL_PRESET".to_string())
            .or_insert_with(|| DEFAULT_CHAT_PRESET.to_string());
    } else {
        env_map.remove("CTOX_CHAT_LOCAL_PRESET");
        runtime_plan::clear_chat_plan_env(env_map);
    }
}

fn sample_gpu_cards() -> Result<Vec<GpuCardState>> {
    let gpu_output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,uuid,name,memory.used,memory.total,utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .context("failed to run nvidia-smi gpu query")?;
    if !gpu_output.status.success() {
        anyhow::bail!("nvidia-smi gpu query failed");
    }

    let mut cards = Vec::new();
    let mut uuid_to_index = HashMap::new();
    for line in String::from_utf8_lossy(&gpu_output.stdout).lines() {
        let parts = line.split(',').map(|part| part.trim()).collect::<Vec<_>>();
        if parts.len() < 6 {
            continue;
        }
        let index = parts[0].parse::<usize>().unwrap_or(0);
        uuid_to_index.insert(parts[1].to_string(), index);
        cards.push(GpuCardState {
            index,
            name: parts[2].to_string(),
            used_mb: parts[3].parse::<u64>().unwrap_or(0),
            total_mb: parts[4].parse::<u64>().unwrap_or(0),
            utilization: parts[5].parse::<u64>().unwrap_or(0),
            allocations: Vec::new(),
        });
    }

    let proc_output = Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=gpu_uuid,pid,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let Ok(proc_output) = proc_output else {
        return Ok(cards);
    };
    if !proc_output.status.success() {
        return Ok(cards);
    }

    let mut pid_to_gpu = Vec::new();
    let mut pids = Vec::new();
    for line in String::from_utf8_lossy(&proc_output.stdout).lines() {
        let parts = line.split(',').map(|part| part.trim()).collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        let Some(&gpu_index) = uuid_to_index.get(parts[0]) else {
            continue;
        };
        let Ok(pid) = parts[1].parse::<u32>() else {
            continue;
        };
        let used_mb = parts[2].parse::<u64>().unwrap_or(0);
        pid_to_gpu.push((pid, gpu_index, used_mb));
        pids.push(pid.to_string());
    }
    if pids.is_empty() {
        return Ok(cards);
    }

    let ps_output = Command::new("ps")
        .args(["-o", "pid=,command=", "-p", &pids.join(",")])
        .output()
        .context("failed to run ps for gpu processes")?;
    let mut pid_to_command = HashMap::new();
    for line in String::from_utf8_lossy(&ps_output.stdout).lines() {
        let trimmed = line.trim_start();
        let mut split = trimmed.splitn(2, ' ');
        let Some(pid_raw) = split.next() else {
            continue;
        };
        let Some(command) = split.next() else {
            continue;
        };
        if let Ok(pid) = pid_raw.trim().parse::<u32>() {
            pid_to_command.insert(pid, command.trim().to_string());
        }
    }

    for (pid, gpu_index, used_mb) in pid_to_gpu {
        let Some(command) = pid_to_command.get(&pid) else {
            continue;
        };
        let Some(model) = model_name_from_process_command(command) else {
            continue;
        };
        let short_label = short_gpu_label(&model);
        if let Some(card) = cards.iter_mut().find(|card| card.index == gpu_index) {
            if let Some(existing) = card
                .allocations
                .iter_mut()
                .find(|allocation| allocation.model == model)
            {
                existing.used_mb = existing.used_mb.saturating_add(used_mb);
            } else {
                card.allocations.push(GpuModelUsage {
                    model,
                    short_label,
                    used_mb,
                });
            }
        }
    }

    cards.sort_by_key(|card| card.index);
    for card in &mut cards {
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
    }
    Ok(cards)
}

fn model_name_from_process_command(command: &str) -> Option<String> {
    model_registry::process_command_model_name(command).map(str::to_string)
}

fn estimated_tokens_per_second(
    model: &str,
    perf_stats: &BTreeMap<String, ModelPerfStats>,
) -> Option<f64> {
    perf_stats
        .get(model.trim())
        .map(|stats| stats.avg_tokens_per_second)
        .or_else(|| model_registry::estimated_tokens_per_second(model))
}

fn gpu_cards_from_plan(
    plan: &runtime_plan::ChatRuntimePlan,
    env_map: &BTreeMap<String, String>,
) -> Vec<GpuCardState> {
    plan.gpu_allocations
        .iter()
        .map(|allocation| {
            let mut allocations = Vec::new();
            if allocation.aux_reserve_mb > 0 {
                allocations.extend(estimated_aux_model_usages(
                    allocation.aux_reserve_mb,
                    env_map,
                ));
            }
            if allocation.chat_budget_mb > 0 {
                allocations.push(GpuModelUsage {
                    model: plan.model.clone(),
                    short_label: short_gpu_label(&plan.model),
                    used_mb: allocation.chat_budget_mb,
                });
            }
            GpuCardState {
                index: allocation.gpu_index,
                name: allocation.name.clone(),
                used_mb: allocation
                    .desktop_reserve_mb
                    .saturating_add(allocation.aux_reserve_mb)
                    .saturating_add(allocation.chat_budget_mb),
                total_mb: allocation.total_mb,
                utilization: 0,
                allocations,
            }
        })
        .collect()
}

fn estimated_aux_model_usages(
    total_aux_mb: u64,
    env_map: &BTreeMap<String, String>,
) -> Vec<GpuModelUsage> {
    if total_aux_mb == 0 {
        return Vec::new();
    }
    let models = [
        (
            engine::AuxiliaryRole::Embedding,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Embedding,
                env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
            ),
        ),
        (
            engine::AuxiliaryRole::Stt,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Stt,
                env_map.get("CTOX_STT_MODEL").map(String::as_str),
            ),
        ),
        (
            engine::AuxiliaryRole::Tts,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Tts,
                env_map.get("CTOX_TTS_MODEL").map(String::as_str),
            ),
        ),
    ]
    .into_iter()
    .filter_map(|(role, selection)| {
        let reserve_mb = selection.gpu_reserve_mb();
        (reserve_mb > 0).then_some((role, selection.request_model, reserve_mb))
    })
    .collect::<Vec<_>>();
    if models.is_empty() {
        return Vec::new();
    }
    let total_weight = models
        .iter()
        .map(|(_, _, weight)| *weight)
        .sum::<u64>()
        .max(1);
    let mut remaining = total_aux_mb;
    let mut usages = Vec::with_capacity(models.len());
    for (index, (role, model, weight)) in models.iter().enumerate() {
        let share = if index + 1 == models.len() {
            remaining
        } else {
            total_aux_mb.saturating_mul(*weight) / total_weight
        };
        remaining = remaining.saturating_sub(share);
        usages.push(GpuModelUsage {
            model: (*model).to_string(),
            short_label: auxiliary_role_label(*role).to_string(),
            used_mb: share,
        });
    }
    usages.retain(|usage| usage.used_mb > 0);
    usages
}

fn estimate_gpu_cards(
    live_cards: &[GpuCardState],
    loaded_model: &str,
    draft_model: &str,
    settings: &BTreeMap<String, String>,
    live_context: usize,
) -> Vec<GpuCardState> {
    let mut cards = live_cards.to_vec();
    let target_isq = settings
        .get("CTOX_ENGINE_ISQ")
        .map(String::as_str)
        .unwrap_or("Q4K");
    let target_context = settings
        .get("CTOX_ENGINE_MAX_SEQ_LEN")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(live_context.max(2048));
    let estimated_total_mb = estimate_chat_model_memory_mb(draft_model, target_isq, target_context);

    let mut aux_by_gpu: HashMap<usize, u64> = HashMap::new();
    for card in &cards {
        let aux_used = card
            .allocations
            .iter()
            .filter(|allocation| allocation.model != loaded_model)
            .map(|allocation| allocation.used_mb)
            .sum::<u64>();
        aux_by_gpu.insert(card.index, aux_used);
    }

    let weights = parse_device_layer_weights(settings.get("CTOX_ENGINE_DEVICE_LAYERS"));
    let total_weight = weights.values().copied().sum::<u64>();
    let selected_gpu_indices = if weights.is_empty() {
        cards.iter().map(|card| card.index).collect::<Vec<_>>()
    } else {
        weights.keys().copied().collect::<Vec<_>>()
    };
    let gpu_count = selected_gpu_indices.len().max(1) as u64;

    for card in &mut cards {
        let aux_used = aux_by_gpu.get(&card.index).copied().unwrap_or(0);
        card.allocations
            .retain(|allocation| allocation.model != loaded_model);
        let chat_used = if let Some(weight) = weights.get(&card.index) {
            if total_weight == 0 {
                estimated_total_mb / gpu_count
            } else {
                estimated_total_mb.saturating_mul(*weight) / total_weight
            }
        } else if weights.is_empty() {
            estimated_total_mb / gpu_count
        } else {
            0
        };
        if chat_used > 0 {
            card.allocations.push(GpuModelUsage {
                model: draft_model.to_string(),
                short_label: short_gpu_label(draft_model),
                used_mb: chat_used,
            });
        }
        card.used_mb = aux_used.saturating_add(chat_used);
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
    }
    cards
}

fn configured_runtime_models(env_map: &BTreeMap<String, String>) -> Vec<String> {
    let mut models = Vec::new();
    if infer_chat_source(env_map).eq_ignore_ascii_case("local") {
        if let Some(value) = env_map
            .get("CTOX_CHAT_MODEL")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            models.push(value.to_string());
        }
    }
    for selection in [
        engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Embedding,
            env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
        ),
        engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Stt,
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        ),
        engine::auxiliary_model_selection(
            engine::AuxiliaryRole::Tts,
            env_map.get("CTOX_TTS_MODEL").map(String::as_str),
        ),
    ] {
        if !models
            .iter()
            .any(|existing| existing == selection.request_model)
        {
            models.push(selection.request_model.to_string());
        }
    }
    models
}

fn load_observation_for_port(root: &Path, port: u16) -> Option<LoadObservation> {
    let path = root
        .join("runtime")
        .join(format!("load_observation_{port}.json"));
    let raw = std::fs::read(path).ok()?;
    serde_json::from_slice(&raw).ok()
}

fn collect_runtime_load_observations(
    root: &Path,
    telemetry: Option<&RuntimeTelemetry>,
    env_map: &BTreeMap<String, String>,
) -> Vec<LoadObservation> {
    let mut observations = Vec::new();
    let mut seen = HashSet::new();

    if let Some(observation) = telemetry.and_then(|value| value.load_observation.clone()) {
        if seen.insert((
            observation.port,
            observation.model.clone(),
            observation.role.clone(),
        )) {
            observations.push(observation);
        }
    }

    for (role, port_key, model_value) in [
        (
            engine::AuxiliaryRole::Embedding,
            "CTOX_EMBEDDING_PORT",
            env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Stt,
            "CTOX_STT_PORT",
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Tts,
            "CTOX_TTS_PORT",
            env_map.get("CTOX_TTS_MODEL").map(String::as_str),
        ),
    ] {
        let selection = engine::auxiliary_model_selection(role, model_value);
        if selection.compute_target == engine::ComputeTarget::Cpu {
            continue;
        }
        let port = env_map
            .get(port_key)
            .and_then(|value| value.trim().parse::<u16>().ok())
            .unwrap_or(selection.default_port);
        let Some(observation) = load_observation_for_port(root, port) else {
            continue;
        };
        let key = (
            observation.port,
            observation.model.clone(),
            observation.role.clone(),
        );
        if seen.insert(key) {
            observations.push(observation);
        }
    }

    observations
}

fn auxiliary_backend_ready(
    resolved_runtime: Option<&runtime_kernel::InferenceRuntimeKernel>,
    role: engine::AuxiliaryRole,
) -> Option<bool> {
    let binding = resolved_runtime?.binding_for_auxiliary_role(role)?;
    Some(binding.transport.probe())
}

fn runtime_health_state(root: &Path, telemetry: Option<&RuntimeTelemetry>) -> RuntimeHealthState {
    let resolved_runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok();
    let chat_source_is_api = runtime_kernel::InferenceRuntimeKernel::resolve(root)
        .ok()
        .map(|k| {
            matches!(
                k.state.source,
                crate::execution::models::runtime_state::InferenceSource::Api
            )
        })
        .unwrap_or(false);
    let runtime_ready = telemetry
        .map(|value| value.backend_healthy)
        .unwrap_or_else(|| {
            crate::inference::gateway::current_runtime_telemetry(root).backend_healthy
        });
    // In pure API mode local auxiliaries are optional; keep them unknown
    // instead of degraded when no local runtime is expected.
    let skip_aux = chat_source_is_api;
    RuntimeHealthState {
        runtime_ready,
        embedding_ready: if skip_aux {
            None
        } else {
            auxiliary_backend_ready(resolved_runtime.as_ref(), engine::AuxiliaryRole::Embedding)
        },
        stt_ready: if skip_aux {
            None
        } else {
            auxiliary_backend_ready(resolved_runtime.as_ref(), engine::AuxiliaryRole::Stt)
        },
        tts_ready: if skip_aux {
            None
        } else {
            auxiliary_backend_ready(resolved_runtime.as_ref(), engine::AuxiliaryRole::Tts)
        },
    }
}

fn ui_now_epoch_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn load_observation_is_loading(observation: &LoadObservation) -> bool {
    if observation.startup_healthy {
        return false;
    }
    let observed_until = observation.observed_until_epoch;
    observed_until > 0 && ui_now_epoch_seconds().saturating_sub(observed_until) <= 30
}

fn push_gpu_allocation(
    cards: &mut Vec<GpuCardState>,
    gpu_index: usize,
    gpu_name: &str,
    total_mb: u64,
    model: &str,
    short_label: &str,
    used_mb: u64,
) {
    if used_mb == 0 {
        return;
    }
    let card_index = if let Some(position) = cards.iter().position(|card| card.index == gpu_index) {
        position
    } else {
        cards.push(GpuCardState {
            index: gpu_index,
            name: gpu_name.to_string(),
            used_mb: 0,
            total_mb,
            utilization: 0,
            allocations: Vec::new(),
        });
        cards.len().saturating_sub(1)
    };
    let card = cards.get_mut(card_index).expect("gpu card inserted");
    if card.total_mb == 0 {
        card.total_mb = total_mb;
    }
    if card.name.is_empty() {
        card.name = gpu_name.to_string();
    }
    if let Some(existing) = card
        .allocations
        .iter_mut()
        .find(|allocation| allocation.model == model)
    {
        existing.used_mb = existing.used_mb.max(used_mb);
    } else {
        card.allocations.push(GpuModelUsage {
            model: model.to_string(),
            short_label: short_label.to_string(),
            used_mb,
        });
    }
    card.used_mb = card.used_mb.saturating_add(used_mb);
}

fn normalize_gpu_cards(cards: &mut Vec<GpuCardState>) {
    cards.sort_by_key(|card| card.index);
    for card in cards.iter_mut() {
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
        card.used_mb = card
            .allocations
            .iter()
            .map(|allocation| allocation.used_mb)
            .sum();
    }
    cards.retain(|card| !card.allocations.is_empty() || card.total_mb > 0);
}

fn merge_gpu_card_layers(
    mut base_cards: Vec<GpuCardState>,
    overlay_cards: Vec<GpuCardState>,
) -> Vec<GpuCardState> {
    for card in overlay_cards {
        for allocation in card.allocations {
            push_gpu_allocation(
                &mut base_cards,
                card.index,
                &card.name,
                card.total_mb,
                &allocation.model,
                &allocation.short_label,
                allocation.used_mb,
            );
        }
    }
    normalize_gpu_cards(&mut base_cards);
    base_cards
}

fn deployed_gpu_cards_from_live(
    live_cards: &[GpuCardState],
    allowed_models: &[String],
    observations: &[LoadObservation],
) -> Vec<GpuCardState> {
    let loading_models = observations
        .iter()
        .filter(|observation| load_observation_is_loading(observation))
        .map(|observation| observation.model.as_str())
        .collect::<HashSet<_>>();
    let mut cards = filter_gpu_cards_to_models(live_cards, allowed_models);
    for card in cards.iter_mut() {
        card.allocations
            .retain(|allocation| !loading_models.contains(allocation.model.as_str()));
        card.used_mb = card
            .allocations
            .iter()
            .map(|allocation| allocation.used_mb)
            .sum();
    }
    cards
}

fn loading_gpu_cards_from_observations(
    target_cards: &[GpuCardState],
    live_cards: &[GpuCardState],
    observations: &[LoadObservation],
) -> Vec<GpuCardState> {
    let mut cards = Vec::new();
    for observation in observations
        .iter()
        .filter(|observation| load_observation_is_loading(observation))
    {
        let short_label = short_gpu_label(&observation.model);
        let mut matched_target = false;
        for card in target_cards {
            for allocation in card
                .allocations
                .iter()
                .filter(|allocation| allocation.model == observation.model)
            {
                push_gpu_allocation(
                    &mut cards,
                    card.index,
                    &card.name,
                    card.total_mb,
                    &allocation.model,
                    &short_label,
                    allocation.used_mb,
                );
                matched_target = true;
            }
        }
        if matched_target {
            continue;
        }
        let mut matched_live = false;
        for card in live_cards {
            for allocation in card
                .allocations
                .iter()
                .filter(|allocation| allocation.model == observation.model)
            {
                push_gpu_allocation(
                    &mut cards,
                    card.index,
                    &card.name,
                    card.total_mb,
                    &allocation.model,
                    &short_label,
                    allocation.used_mb,
                );
                matched_live = true;
            }
        }
        if matched_live {
            continue;
        }
        for gpu in &observation.gpus {
            let observed_mb = gpu
                .current_delta_mb
                .max(gpu.peak_delta_mb)
                .max(gpu.final_used_mb.saturating_sub(gpu.baseline_used_mb));
            push_gpu_allocation(
                &mut cards,
                gpu.gpu_index,
                &gpu.name,
                gpu.total_mb,
                &observation.model,
                &short_label,
                observed_mb,
            );
        }
    }
    normalize_gpu_cards(&mut cards);
    cards
}

fn unhealthy_backend_models(
    env_map: &BTreeMap<String, String>,
    runtime_health: &RuntimeHealthState,
) -> HashSet<String> {
    let mut models = HashSet::new();
    for (role, ready, model_value) in [
        (
            engine::AuxiliaryRole::Embedding,
            runtime_health.embedding_ready,
            env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Stt,
            runtime_health.stt_ready,
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Tts,
            runtime_health.tts_ready,
            env_map.get("CTOX_TTS_MODEL").map(String::as_str),
        ),
    ] {
        if ready == Some(false) {
            let selection = engine::auxiliary_model_selection(role, model_value);
            models.insert(selection.request_model.to_string());
        }
    }
    models
}

fn unhealthy_backend_loading_cards(
    live_cards: &[GpuCardState],
    env_map: &BTreeMap<String, String>,
    runtime_health: &RuntimeHealthState,
) -> Vec<GpuCardState> {
    let unhealthy_models = unhealthy_backend_models(env_map, runtime_health);
    if unhealthy_models.is_empty() {
        return Vec::new();
    }
    let target_cards = aux_gpu_target_cards(live_cards, env_map);
    let mut cards = Vec::new();
    for card in &target_cards {
        for allocation in card
            .allocations
            .iter()
            .filter(|allocation| unhealthy_models.contains(&allocation.model))
        {
            push_gpu_allocation(
                &mut cards,
                card.index,
                &card.name,
                card.total_mb,
                &allocation.model,
                &allocation.short_label,
                allocation.used_mb,
            );
        }
    }
    normalize_gpu_cards(&mut cards);
    cards
}

fn healthy_backend_models(
    env_map: &BTreeMap<String, String>,
    runtime_health: &RuntimeHealthState,
) -> HashSet<String> {
    let mut models = HashSet::new();
    for (role, ready, model_value) in [
        (
            engine::AuxiliaryRole::Embedding,
            runtime_health.embedding_ready,
            env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Stt,
            runtime_health.stt_ready,
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Tts,
            runtime_health.tts_ready,
            env_map.get("CTOX_TTS_MODEL").map(String::as_str),
        ),
    ] {
        if ready == Some(true) {
            let selection = engine::auxiliary_model_selection(role, model_value);
            models.insert(selection.request_model.to_string());
        }
    }
    models
}

fn healthy_backend_deployed_cards(
    live_cards: &[GpuCardState],
    env_map: &BTreeMap<String, String>,
    runtime_health: &RuntimeHealthState,
) -> Vec<GpuCardState> {
    let healthy_models = healthy_backend_models(env_map, runtime_health);
    if healthy_models.is_empty() {
        return Vec::new();
    }
    let target_cards = aux_gpu_target_cards(live_cards, env_map);
    let mut cards = Vec::new();
    for card in &target_cards {
        for allocation in card.allocations.iter().filter(|allocation| {
            healthy_models.contains(&allocation.model)
                && !live_cards.iter().any(|live_card| {
                    live_card
                        .allocations
                        .iter()
                        .any(|candidate| candidate.model == allocation.model)
                })
        }) {
            push_gpu_allocation(
                &mut cards,
                card.index,
                &card.name,
                card.total_mb,
                &allocation.model,
                &allocation.short_label,
                allocation.used_mb,
            );
        }
    }
    normalize_gpu_cards(&mut cards);
    cards
}

fn parse_csv_gpu_indices(raw: Option<&String>) -> Vec<usize> {
    raw.into_iter()
        .flat_map(|value| value.split(','))
        .filter_map(|chunk| chunk.trim().parse::<usize>().ok())
        .collect()
}

fn even_shares(total: u64, count: usize) -> Vec<u64> {
    if count == 0 {
        return Vec::new();
    }
    let base = total / count as u64;
    let remainder = total % count as u64;
    (0..count)
        .map(|index| base + u64::from(index < remainder as usize))
        .collect()
}

fn auxiliary_visible_devices_for_role(
    env_map: &BTreeMap<String, String>,
    role: engine::AuxiliaryRole,
) -> Vec<usize> {
    let role_specific_key = match role {
        engine::AuxiliaryRole::Embedding => "CTOX_EMBEDDING_CUDA_VISIBLE_DEVICES",
        engine::AuxiliaryRole::Stt => "CTOX_STT_CUDA_VISIBLE_DEVICES",
        engine::AuxiliaryRole::Tts => "CTOX_TTS_CUDA_VISIBLE_DEVICES",
        engine::AuxiliaryRole::Vision => "CTOX_VISION_CUDA_VISIBLE_DEVICES",
    };
    let explicit = parse_csv_gpu_indices(env_map.get(role_specific_key));
    if !explicit.is_empty() {
        return explicit;
    }
    let shared = parse_csv_gpu_indices(env_map.get("CTOX_AUXILIARY_CUDA_VISIBLE_DEVICES"));
    if !shared.is_empty() {
        return shared;
    }
    vec![0]
}

fn aux_gpu_target_cards(
    live_cards: &[GpuCardState],
    env_map: &BTreeMap<String, String>,
) -> Vec<GpuCardState> {
    let mut cards = live_cards
        .iter()
        .map(|card| GpuCardState {
            index: card.index,
            name: card.name.clone(),
            used_mb: 0,
            total_mb: card.total_mb,
            utilization: 0,
            allocations: Vec::new(),
        })
        .collect::<Vec<_>>();

    for (role, model_value) in [
        (
            engine::AuxiliaryRole::Embedding,
            env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Stt,
            env_map.get("CTOX_STT_MODEL").map(String::as_str),
        ),
        (
            engine::AuxiliaryRole::Tts,
            env_map.get("CTOX_TTS_MODEL").map(String::as_str),
        ),
    ] {
        let selection = engine::auxiliary_model_selection(role, model_value);
        let reserve_mb = selection.gpu_reserve_mb();
        if reserve_mb == 0 {
            continue;
        }
        let gpu_indices = auxiliary_visible_devices_for_role(env_map, role);
        if gpu_indices.is_empty() {
            continue;
        }
        let shares = even_shares(reserve_mb, gpu_indices.len());
        for (position, gpu_index) in gpu_indices.iter().enumerate() {
            let share = *shares.get(position).unwrap_or(&0);
            if share == 0 {
                continue;
            }
            if !cards.iter().any(|card| card.index == *gpu_index) {
                cards.push(GpuCardState {
                    index: *gpu_index,
                    name: String::new(),
                    used_mb: 0,
                    total_mb: 0,
                    utilization: 0,
                    allocations: Vec::new(),
                });
            }
            let card = cards
                .iter_mut()
                .find(|card| card.index == *gpu_index)
                .expect("gpu card inserted");
            card.allocations.push(GpuModelUsage {
                model: selection.request_model.to_string(),
                short_label: auxiliary_role_label(role).to_string(),
                used_mb: share,
            });
            card.used_mb = card.used_mb.saturating_add(share);
        }
    }

    cards.retain(|card| !card.allocations.is_empty());
    cards.sort_by_key(|card| card.index);
    for card in &mut cards {
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
    }
    cards
}

fn filter_gpu_cards_to_models(
    live_cards: &[GpuCardState],
    allowed_models: &[String],
) -> Vec<GpuCardState> {
    if allowed_models.is_empty() {
        return live_cards.to_vec();
    }
    let mut cards = live_cards.to_vec();
    for card in &mut cards {
        card.allocations.retain(|allocation| {
            allowed_models
                .iter()
                .any(|model| model == &allocation.model)
        });
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
        card.used_mb = card
            .allocations
            .iter()
            .map(|allocation| allocation.used_mb)
            .sum();
    }
    cards
}

#[allow(dead_code)]
fn overlay_load_observation_gpu_cards(
    live_cards: &[GpuCardState],
    observations: &[LoadObservation],
    env_map: &BTreeMap<String, String>,
) -> Vec<GpuCardState> {
    let mut cards = live_cards.to_vec();
    if observations.is_empty() {
        return cards;
    }

    for observation in observations {
        if observation.model.trim().is_empty() || engine::is_api_chat_model(&observation.model) {
            continue;
        }
        if infer_chat_source(env_map).eq_ignore_ascii_case("api") && observation.role == "chat" {
            continue;
        }

        let short_label = short_gpu_label(&observation.model);
        for gpu in &observation.gpus {
            let observed_mb = gpu
                .current_delta_mb
                .max(gpu.final_used_mb.saturating_sub(gpu.baseline_used_mb))
                .max(gpu.peak_delta_mb.min(gpu.current_delta_mb));
            if observed_mb == 0 {
                continue;
            }
            let Some(card) = cards.iter_mut().find(|card| card.index == gpu.gpu_index) else {
                cards.push(GpuCardState {
                    index: gpu.gpu_index,
                    name: gpu.name.clone(),
                    used_mb: observed_mb,
                    total_mb: gpu.total_mb,
                    utilization: 0,
                    allocations: vec![GpuModelUsage {
                        model: observation.model.clone(),
                        short_label: short_label.clone(),
                        used_mb: observed_mb,
                    }],
                });
                continue;
            };
            if let Some(existing) = card
                .allocations
                .iter_mut()
                .find(|allocation| allocation.model == observation.model)
            {
                existing.used_mb = existing.used_mb.max(observed_mb);
            } else {
                card.allocations.push(GpuModelUsage {
                    model: observation.model.clone(),
                    short_label: short_label.clone(),
                    used_mb: observed_mb,
                });
            }
            card.used_mb = card.used_mb.max(observed_mb);
            if card.total_mb == 0 {
                card.total_mb = gpu.total_mb;
            }
            if card.name.is_empty() {
                card.name = gpu.name.clone();
            }
        }
    }

    cards.sort_by_key(|card| card.index);
    for card in &mut cards {
        card.allocations
            .sort_by(|left, right| right.used_mb.cmp(&left.used_mb));
        let allocation_total = card
            .allocations
            .iter()
            .map(|allocation| allocation.used_mb)
            .sum::<u64>();
        card.used_mb = card.used_mb.max(allocation_total);
    }
    cards
}

fn previous_settings_view(view: SettingsView) -> SettingsView {
    match view {
        SettingsView::Model => SettingsView::HarnessFlow,
        SettingsView::Communication => SettingsView::Model,
        SettingsView::Secrets => SettingsView::Communication,
        SettingsView::Paths => SettingsView::Secrets,
        SettingsView::Update => SettingsView::Paths,
        SettingsView::BusinessOs => SettingsView::Update,
        SettingsView::HarnessMining => SettingsView::BusinessOs,
        SettingsView::HarnessFlow => SettingsView::HarnessMining,
    }
}

fn next_settings_view(view: SettingsView) -> SettingsView {
    match view {
        SettingsView::Model => SettingsView::Communication,
        SettingsView::Communication => SettingsView::Secrets,
        SettingsView::Secrets => SettingsView::Paths,
        SettingsView::Paths => SettingsView::Update,
        SettingsView::Update => SettingsView::BusinessOs,
        SettingsView::BusinessOs => SettingsView::HarnessMining,
        SettingsView::HarnessMining => SettingsView::HarnessFlow,
        SettingsView::HarnessFlow => SettingsView::Model,
    }
}

fn expected_gpu_aux_labels(env_map: &BTreeMap<String, String>) -> Vec<String> {
    [
        (
            engine::AuxiliaryRole::Embedding,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Embedding,
                env_map.get("CTOX_EMBEDDING_MODEL").map(String::as_str),
            ),
        ),
        (
            engine::AuxiliaryRole::Stt,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Stt,
                env_map.get("CTOX_STT_MODEL").map(String::as_str),
            ),
        ),
        (
            engine::AuxiliaryRole::Tts,
            engine::auxiliary_model_selection(
                engine::AuxiliaryRole::Tts,
                env_map.get("CTOX_TTS_MODEL").map(String::as_str),
            ),
        ),
    ]
    .into_iter()
    .filter_map(|(role, selection)| {
        (selection.compute_target == engine::ComputeTarget::Gpu)
            .then_some(auxiliary_role_label(role).to_string())
    })
    .collect()
}

fn auxiliary_role_label(role: engine::AuxiliaryRole) -> &'static str {
    match role {
        engine::AuxiliaryRole::Embedding => "embed",
        engine::AuxiliaryRole::Stt => "stt",
        engine::AuxiliaryRole::Tts => "tts",
        engine::AuxiliaryRole::Vision => "vision",
    }
}

fn short_gpu_label(model: &str) -> String {
    model_registry::gpu_short_label(model)
        .map(str::to_string)
        .unwrap_or_else(|| compact_model_name(model, 32))
}

fn parse_device_layer_weights(raw: Option<&String>) -> BTreeMap<usize, u64> {
    let mut weights = BTreeMap::new();
    let Some(raw) = raw else {
        return weights;
    };
    for chunk in raw.trim_matches('\'').split(';') {
        let Some((gpu, weight)) = chunk.split_once(':') else {
            continue;
        };
        let Ok(gpu_index) = gpu.trim().parse::<usize>() else {
            continue;
        };
        let Ok(weight_value) = weight.trim().parse::<u64>() else {
            continue;
        };
        weights.insert(gpu_index, weight_value);
    }
    weights
}

fn estimate_chat_model_memory_mb(model: &str, isq: &str, target_context: usize) -> u64 {
    let base_mb = model_registry::estimated_chat_base_memory_mb(model).unwrap_or(12_000) as f64;
    let isq_factor = match isq.trim().to_ascii_uppercase().as_str() {
        "Q2K" => 0.72,
        "Q3K" => 0.84,
        "Q4K" => 1.0,
        "Q5K" => 1.12,
        "Q6K" => 1.24,
        "Q8K" => 1.40,
        "FP8" => 1.30,
        _ => 1.0,
    };
    let context_factor = 0.88 + 0.12 * ((target_context.max(1024) as f64) / 4096.0).clamp(0.5, 4.0);
    (base_mb * isq_factor * context_factor).round() as u64
}

#[allow(dead_code)]
fn estimate_max_context_window(
    cards: &[GpuCardState],
    loaded_model: &str,
    draft_model: &str,
    settings: &BTreeMap<String, String>,
    live_context: usize,
) -> usize {
    if cards.is_empty() {
        return MAX_CONTEXT_WINDOW;
    }
    let target_isq = settings
        .get("CTOX_ENGINE_ISQ")
        .map(String::as_str)
        .unwrap_or("Q4K");
    let base_context = live_context.max(2048);
    let base_memory_mb =
        estimate_chat_model_memory_mb(draft_model, target_isq, base_context).max(1);
    let weights = parse_device_layer_weights(settings.get("CTOX_ENGINE_DEVICE_LAYERS"));
    let selected_gpu_indices = if weights.is_empty() {
        cards.iter().map(|card| card.index).collect::<Vec<_>>()
    } else {
        weights.keys().copied().collect::<Vec<_>>()
    };
    let selected_cards = cards
        .iter()
        .filter(|card| selected_gpu_indices.contains(&card.index))
        .collect::<Vec<_>>();
    if selected_cards.is_empty() {
        return MAX_CONTEXT_WINDOW;
    }
    let total_budget_mb = selected_cards
        .iter()
        .map(|card| ((card.total_mb as f64) * 0.96).floor() as u64)
        .sum::<u64>();
    let aux_usage_mb = selected_cards
        .iter()
        .map(|card| {
            card.allocations
                .iter()
                .filter(|allocation| allocation.model != loaded_model)
                .map(|allocation| allocation.used_mb)
                .sum::<u64>()
        })
        .sum::<u64>();
    if total_budget_mb <= aux_usage_mb {
        return 2048;
    }
    let usable_chat_budget_mb = total_budget_mb - aux_usage_mb;
    let low_context = 2048usize;
    let high_context = 8192usize;
    let low_memory_mb =
        estimate_chat_model_memory_mb(draft_model, target_isq, low_context).max(base_memory_mb);
    let high_memory_mb =
        estimate_chat_model_memory_mb(draft_model, target_isq, high_context).max(low_memory_mb + 1);
    let slope_mb_per_token =
        (high_memory_mb.saturating_sub(low_memory_mb)) as f64 / (high_context - low_context) as f64;
    let fixed_overhead_mb =
        (low_memory_mb as f64 - slope_mb_per_token * low_context as f64).max(0.0);
    let budget_f = usable_chat_budget_mb as f64;
    let estimated_context = if budget_f <= fixed_overhead_mb {
        low_context
    } else {
        ((budget_f - fixed_overhead_mb) / slope_mb_per_token).round() as usize
    };
    estimated_context.clamp(2048, MAX_CONTEXT_WINDOW)
}

fn render_qr_lines(payload: &str) -> Option<Vec<String>> {
    let code = QrCode::new(payload.as_bytes()).ok()?;
    let width = code.width();
    let colors = code.to_colors();
    let pad = 6usize;
    let mut matrix = vec![vec![false; width + pad * 2]; width + pad * 2];
    for y in 0..width {
        for x in 0..width {
            matrix[y + pad][x + pad] = matches!(colors[y * width + x], QrColor::Dark);
        }
    }
    if matrix.len() % 2 != 0 {
        matrix.push(vec![false; matrix[0].len()]);
    }
    let mut lines = Vec::with_capacity(matrix.len() / 2);
    for y in (0..matrix.len()).step_by(2) {
        let top = &matrix[y];
        let bottom = &matrix[y + 1];
        let mut line = String::with_capacity(top.len());
        for x in 0..top.len() {
            let ch = match (top[x], bottom[x]) {
                (false, false) => ' ',
                (true, false) => '▀',
                (false, true) => '▄',
                (true, true) => '█',
            };
            line.push(ch);
        }
        lines.push(line);
    }
    Some(lines)
}

fn resolve_jami_runtime_account(
    root: &Path,
    configured_account_id: &str,
    configured_profile_name: &str,
) -> JamiResolveOutcome {
    let adapter = communication_adapters::jami();
    let resolved = adapter.resolve_account(
        root,
        &communication_adapters::JamiResolveAccountCommandRequest {
            account_id: Some(configured_account_id),
            profile_name: Some(configured_profile_name),
        },
    );
    let parsed = match resolved {
        Ok(value) => serde_json::from_value::<JamiResolvedEnvelope>(value),
        Err(err) => {
            return JamiResolveOutcome {
                account: None,
                error: Some(format!("failed to resolve jami adapter state: {err}")),
                dbus_env_file: None,
                checks: Vec::new(),
            };
        }
    };
    match parsed {
        Ok(parsed) => JamiResolveOutcome {
            account: parsed.resolved_account,
            error: if parsed.ok {
                parsed.error
            } else {
                parsed.error
            },
            dbus_env_file: parsed.dbus_env_file,
            checks: parsed.checks,
        },
        Err(err) => JamiResolveOutcome {
            account: None,
            error: Some(format!("failed to parse jami adapter output: {err}")),
            dbus_env_file: None,
            checks: Vec::new(),
        },
    }
}

fn jami_missing_account_lines(
    dbus_env_file: Option<&str>,
    has_config: bool,
    checks: &[JamiDoctorCheck],
) -> Vec<String> {
    let mut lines = vec!["No live Jami RING account is available yet.".to_string()];
    if has_config {
        lines.push(
            "Configured account/profile could not be resolved to an active share URI.".to_string(),
        );
    } else {
        lines.push("No Jami account id or profile is configured yet, so the TUI cannot derive a QR target.".to_string());
    }
    if let Some(path) = dbus_env_file.filter(|value| !value.trim().is_empty()) {
        lines.push(format!("dbus {}", truncate_for_ui(path, 40)));
    }
    lines.extend(jami_doctor_hint_lines(checks));
    lines.push("Verify the Jami daemon is running and that a RING account exists.".to_string());
    lines
}

fn jami_error_lines(
    error: &str,
    dbus_env_file: Option<&str>,
    has_config: bool,
    checks: &[JamiDoctorCheck],
) -> Vec<String> {
    let mut lines = vec!["Jami runtime is not ready.".to_string()];
    lines.push(format!("blocker {}", truncate_for_ui(error, 68)));
    if error.contains("DBUS_SESSION_BUS_ADDRESS") || error.contains("session bus") {
        lines.push(
            "Missing a user DBus session: start the Linux user bus or export DBUS_SESSION_BUS_ADDRESS before starting the Jami daemon."
                .to_string(),
        );
    }
    if let Some(path) = dbus_env_file.filter(|value| !value.trim().is_empty()) {
        lines.push(format!("dbus {}", truncate_for_ui(path, 40)));
    } else {
        lines.push("No Jami DBus env file is loaded yet.".to_string());
    }
    if has_config {
        lines.push(
            "Configured Jami account/profile is present, but runtime resolution still failed."
                .to_string(),
        );
    } else {
        lines.push("No configured Jami account/profile is available to fall back to.".to_string());
    }
    lines.extend(jami_doctor_hint_lines(checks));
    lines.push(
        "Start or repair the Jami daemon first; then reopen the Jami settings view.".to_string(),
    );
    lines
}

fn jami_doctor_hint_lines(checks: &[JamiDoctorCheck]) -> Vec<String> {
    let mut lines = Vec::new();
    for check in checks.iter().filter(|check| !check.ok) {
        match check.name.as_str() {
            "automation_backend" => lines.push(
                "hint macOS Jami is not automatable through the current DBus adapter; use a manual share URI for QR or move Jami automation to a Linux runtime".to_string(),
            ),
            "dbus_env_file" => lines.push("hint start the CTOX Jami daemon so it writes CTO_JAMI_DBUS_ENV_FILE".to_string()),
            "dbus_session" => lines.push("hint ensure a Linux user DBus session is available before starting Jami".to_string()),
            "jami_runtime" => lines.push("hint brew install --cask jami".to_string()),
            "configured_identity" => lines.push("hint set CTO_JAMI_ACCOUNT_ID or CTO_JAMI_PROFILE_NAME".to_string()),
            _ => {}
        }
    }
    lines
}

fn model_perf_stats_path(root: &Path) -> PathBuf {
    root.join("runtime/model_perf_stats.json")
}

fn load_model_perf_stats(root: &Path) -> BTreeMap<String, ModelPerfStats> {
    let path = model_perf_stats_path(root);
    std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, ModelPerfStats>>(&bytes).ok())
        .unwrap_or_default()
}

fn save_model_perf_stats(root: &Path, stats: &BTreeMap<String, ModelPerfStats>) -> Result<()> {
    let path = model_perf_stats_path(root);
    let bytes = serde_json::to_vec_pretty(stats).context("failed to encode model perf stats")?;
    std::fs::write(path, bytes).context("failed to write model perf stats")?;
    Ok(())
}

fn mask_secret(value: &str) -> String {
    if value.chars().count() <= 4 {
        return "*".repeat(value.chars().count());
    }
    let tail: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!(
        "{}{}",
        "*".repeat(value.chars().count().saturating_sub(4)),
        tail
    )
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter(stdout: &mut io::Stdout) -> Result<Self> {
        terminal::enable_raw_mode().context("failed to enable raw mode")?;
        execute!(
            stdout,
            EnterAlternateScreen,
            terminal::Clear(ClearType::All),
            cursor::Hide
        )?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = terminal::disable_raw_mode();
        let _ = execute!(stdout, cursor::Show, LeaveAlternateScreen);
    }
}

fn summarize_inline(value: &str, max_chars: usize) -> String {
    truncate_for_ui(value, max_chars)
}

fn truncate_for_ui(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut out = collapsed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    out.push('…');
    out
}

fn compact_model_name(model: &str, width: usize) -> String {
    let short = model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .replace("openai/", "")
        .replace("Qwen/", "");
    if width < 72 {
        truncate_for_ui(&short, 18)
    } else {
        truncate_for_ui(&short, 30)
    }
}

#[allow(dead_code)]
fn signed_delta(delta: i64) -> String {
    if delta > 0 {
        format!("+{delta}")
    } else {
        delta.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::runtime_env;
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ctox-tui-{label}-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn supported_chat_models_follow_selected_provider_immediately() {
        let root = temp_root("supported-chat-models");
        let local_only = supported_chat_model_choices_with_gpu(&root, &BTreeMap::new(), false);
        assert!(local_only.is_empty());
        assert!(!local_only.contains(&"openai/gpt-oss-120b"));
        assert!(!local_only.contains(&"nvidia/Nemotron-Cascade-2-30B-A3B"));
        assert!(!local_only.contains(&"zai-org/GLM-4.7-Flash"));
        assert!(!local_only.contains(&"gpt-5.4"));

        let mut openai_api = BTreeMap::new();
        openai_api.insert("CTOX_API_PROVIDER".to_string(), "openai".to_string());
        openai_api.insert("OPENAI_API_KEY".to_string(), "sk-test".to_string());
        let with_openai = supported_chat_model_choices_with_gpu(&root, &openai_api, true);
        assert!(with_openai.contains(&"gpt-5.5"));
        assert!(with_openai.contains(&"gpt-5.4-nano"));
        assert!(with_openai.contains(&"gpt-5.4-mini"));
        assert!(with_openai.contains(&"gpt-5.4"));

        let mut openai_subscription = BTreeMap::new();
        openai_subscription.insert("CTOX_API_PROVIDER".to_string(), "openai".to_string());
        openai_subscription.insert(
            "CTOX_OPENAI_AUTH_MODE".to_string(),
            "chatgpt_subscription".to_string(),
        );
        let with_subscription =
            supported_chat_model_choices_with_gpu(&root, &openai_subscription, true);
        assert!(with_subscription.contains(&"gpt-5.4-mini"));
        assert!(with_subscription.contains(&"gpt-5.4"));

        let mut anthropic_api = BTreeMap::new();
        anthropic_api.insert("CTOX_API_PROVIDER".to_string(), "anthropic".to_string());
        anthropic_api.insert("ANTHROPIC_API_KEY".to_string(), "sk-ant-test".to_string());
        let with_anthropic = supported_chat_model_choices_with_gpu(&root, &anthropic_api, false);
        assert!(with_anthropic.contains(&"claude-opus-4-7"));
        assert!(with_anthropic.contains(&"claude-opus-4-6"));
        assert!(with_anthropic.contains(&"claude-sonnet-4-7"));
        assert!(with_anthropic.contains(&"claude-sonnet-4-6"));
        assert!(!with_anthropic.contains(&"anthropic/claude-sonnet-4.6"));
        assert!(!with_anthropic.contains(&"gpt-5.4"));

        let mut azure_api = BTreeMap::new();
        azure_api.insert("CTOX_API_PROVIDER".to_string(), "azure_foundry".to_string());
        azure_api.insert(
            AZURE_FOUNDRY_DEPLOYMENT_ID_KEY.to_string(),
            "company-gpt-5".to_string(),
        );
        azure_api.insert(
            AZURE_FOUNDRY_TOKEN_KEY.to_string(),
            "azure-token".to_string(),
        );
        let with_azure = supported_chat_model_choices_with_gpu(&root, &azure_api, true);
        assert!(with_azure.is_empty());
    }

    #[test]
    fn chat_composer_title_carries_image_badge_when_pending() {
        use ratatui::backend::TestBackend;

        let root = temp_root("composer-badge");
        let db_path = crate::persistence::sqlite_path(&root);
        let _ = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default()).unwrap();

        let mut app = App::new(root.clone(), db_path);
        app.page = Page::Chat;

        // Baseline: no pending images → no 📎 in the composer title.
        let backend = TestBackend::new(140, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render::draw(frame, &app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut baseline_text = String::new();
        for y in 0..40u16 {
            for x in 0..140u16 {
                baseline_text.push_str(buf[(x, y)].symbol());
            }
            baseline_text.push('\n');
        }
        assert!(
            !baseline_text.contains("📎"),
            "composer should not show paperclip without attachments"
        );

        // With a pending image the composer title must show the badge.
        app.pending_images.push(PendingImage {
            path: PathBuf::from("/tmp/fake.png"),
            size_bytes: 1024,
        });
        let backend = TestBackend::new(140, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render::draw(frame, &app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut badge_text = String::new();
        for y in 0..40u16 {
            for x in 0..140u16 {
                badge_text.push_str(buf[(x, y)].symbol());
            }
            badge_text.push('\n');
        }
        assert!(
            badge_text.contains("📎"),
            "composer must carry the 📎 badge when pending_images is non-empty"
        );
        assert!(
            badge_text.contains("1 image"),
            "badge should include the pending count"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn image_slash_command_resolves_existing_file_and_rejects_non_image() {
        let root = temp_root("image-slash");
        let db_path = crate::persistence::sqlite_path(&root);
        let _ = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default()).unwrap();
        let mut app = App::new(root.clone(), db_path);

        // Minimal PNG written to a tempfile that lives under the workspace
        // root so try_resolve_image_attachment's relative-path handling
        // can resolve it.
        let png_path = root.join("snippet.png");
        let png_bytes: [u8; 67] = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x62, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        fs::write(&png_path, png_bytes).unwrap();

        // Absolute path attaches.
        let handled = app.handle_image_command(&format!("/image {}", png_path.display()));
        assert!(handled);
        assert_eq!(app.pending_images.len(), 1);

        // Non-image file → rejected, count unchanged.
        let txt_path = root.join("note.txt");
        fs::write(&txt_path, b"hello").unwrap();
        let handled = app.handle_image_command(&format!("/image {}", txt_path.display()));
        assert!(handled);
        assert_eq!(app.pending_images.len(), 1);

        // /image clear flushes the queue.
        let handled = app.handle_image_command("/image clear");
        assert!(handled);
        assert_eq!(app.pending_images.len(), 0);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn supported_boost_models_include_api_models_when_key_is_present() {
        let root = temp_root("supported-boost-models");
        let without_key = supported_boost_model_choices(&root, &BTreeMap::new());
        assert!(without_key.is_empty());
        assert!(!without_key.contains(&"Qwen/Qwen3.5-4B"));
        assert!(!without_key.contains(&"nvidia/Nemotron-Cascade-2-30B-A3B"));
        assert!(!without_key.contains(&"gpt-5.4"));

        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_API_PROVIDER".to_string(), "openai".to_string());
        env_map.insert("OPENAI_API_KEY".to_string(), "sk-test".to_string());
        let with_key = supported_boost_model_choices(&root, &env_map);
        assert!(!with_key.contains(&"openai/gpt-oss-120b"));
        assert!(with_key.contains(&"gpt-5.4"));
        assert!(with_key.contains(&"gpt-5.4-mini"));
    }

    #[test]
    fn model_settings_include_provider_and_communication_fields() {
        let root = temp_root("settings");
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "openai/gpt-oss-120b".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        let items = load_settings_items(&root);
        let keys: Vec<_> = items.iter().map(|item| item.key).collect();

        assert!(keys.contains(&"CTOX_CHAT_MODEL"));
        assert!(keys.contains(&"CTOX_CHAT_MODEL_BOOST"));
        assert!(keys.contains(&"CTOX_BOOST_DEFAULT_MINUTES"));
        assert!(keys.contains(&"CTOX_API_PROVIDER"));
        assert!(keys.contains(&"CTOX_OPENAI_AUTH_MODE"));
        assert!(keys.contains(&"CTOX_LOCAL_RUNTIME"));
        assert!(keys.contains(&"OPENAI_API_KEY"));
        assert!(keys.contains(&"ANTHROPIC_API_KEY"));
        assert!(!keys.contains(&"CTOX_AUXILIARY_CUDA_VISIBLE_DEVICES"));
        assert!(!keys.contains(&"CTOX_EMBEDDING_PORT"));
        assert!(!keys.contains(&"CTOX_STT_PORT"));
        assert!(!keys.contains(&"CTOX_TTS_PORT"));
        assert!(keys.contains(&"CTOX_OWNER_NAME"));
        assert!(keys.contains(&"CTOX_OWNER_EMAIL_ADDRESS"));
        assert!(keys.contains(&"CTOX_OWNER_PREFERRED_CHANNEL"));
        assert!(keys.contains(&"CTOX_WEBRTC_SIGNALING_URL"));
        assert!(keys.contains(&"CTOX_WEBRTC_ROOM"));
        assert!(keys.contains(&"CTOX_WEBRTC_PASSWORD"));
        assert!(keys.contains(&"CTO_EMAIL_PASSWORD"));
        assert!(keys.contains(&"CTO_EMAIL_PROVIDER"));
        assert!(keys.contains(&"CTO_JAMI_ACCOUNT_ID"));
        assert!(keys.contains(&"CTOX_CTO_OPERATING_MODE_PROMPT"));
    }

    #[test]
    fn app_defaults_to_model_settings_view_and_openai_key_visibility_follows_provider() {
        let root = temp_root("visibility");
        let db_path = root.join("runtime/test.sqlite3");
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "openai/gpt-oss-120b".to_string(),
        );
        env_map.insert("CTOX_API_PROVIDER".to_string(), "local".to_string());
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        let app = App::new(root.clone(), db_path);
        assert_eq!(app.settings_view, SettingsView::Model);

        let openai_key_row = app
            .settings_items
            .iter()
            .find(|item| item.key == "OPENAI_API_KEY")
            .unwrap()
            .clone();
        assert!(!app.setting_visible(&openai_key_row));

        let openai_auth_row = app
            .settings_items
            .iter()
            .find(|item| item.key == "CTOX_OPENAI_AUTH_MODE")
            .unwrap()
            .clone();
        assert!(!app.setting_visible(&openai_auth_row));

        let mut app = App::new(root.clone(), root.join("runtime/test-subscription.sqlite3"));
        let provider_idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "CTOX_API_PROVIDER")
            .unwrap();
        let auth_idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "CTOX_OPENAI_AUTH_MODE")
            .unwrap();
        app.settings_items[provider_idx].value = "openai".to_string();
        app.settings_items[auth_idx].value = "chatgpt_subscription".to_string();
        app.refresh_dynamic_setting_choices();
        let openai_key_row = app
            .settings_items
            .iter()
            .find(|item| item.key == "OPENAI_API_KEY")
            .unwrap()
            .clone();
        assert!(!app.setting_visible(&openai_key_row));
    }

    #[test]
    fn no_gpu_local_family_choices_only_include_small_qwen_and_gemma() {
        let root = temp_root("no-gpu-families");
        let choices = supported_local_chat_family_choices_with_gpu(&root, &BTreeMap::new(), false);

        assert!(choices.contains(&"Qwen 3.5"));
        assert!(choices.contains(&"Gemma 4"));
        assert!(!choices.contains(&"GPT-OSS"));
        assert!(!choices.contains(&"Nemotron Cascade 2"));
        assert!(!choices.contains(&"GLM 4.7 Flash"));
    }

    #[test]
    fn settings_view_shortcuts_switch_between_model_and_communication() {
        let root = temp_root("settings-view-shortcuts");
        let db_path = root.join("runtime/test.sqlite3");
        let mut app = App::new(root, db_path);

        app.handle_settings_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.settings_view, SettingsView::Communication);

        app.handle_settings_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.settings_view, SettingsView::Model);

        app.handle_settings_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT))
            .unwrap();
        assert_eq!(app.settings_view, SettingsView::Communication);

        app.handle_settings_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.settings_view, SettingsView::Secrets);
    }

    #[test]
    fn tab_cycles_through_settings_communication_before_leaving_page() {
        let root = temp_root("settings-tab-cycle");
        let db_path = root.join("runtime/test.sqlite3");
        let mut app = App::new(root, db_path);

        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Skills);

        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Costs);

        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::Model);

        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::Communication);

        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::Secrets);

        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::Paths);

        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::Update);

        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::BusinessOs);

        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::HarnessMining);

        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::HarnessFlow);

        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Chat);
        assert_eq!(app.settings_view, SettingsView::Model);

        app.handle_key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::HarnessFlow);

        app.handle_key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::HarnessMining);

        app.handle_key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::BusinessOs);
    }

    #[test]
    fn visible_model_settings_follow_requested_minimal_flow() {
        let root = temp_root("minimal-flow");
        let db_path = root.join("runtime/test.sqlite3");
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        let mut app = App::new(root.clone(), db_path);
        let local_keys = app
            .visible_setting_indices()
            .into_iter()
            .map(|idx| app.settings_items[idx].key)
            .collect::<Vec<_>>();
        assert_eq!(
            local_keys,
            vec![
                "CTOX_SERVICE_TOGGLE",
                "CTOX_API_PROVIDER",
                "CTOX_LOCAL_RUNTIME",
                "CTOX_CHAT_MODEL",
                "CTOX_CHAT_LOCAL_PRESET",
                "CTOX_CHAT_SKILL_PRESET",
                "CTOX_REFRESH_OUTPUT_BUDGET_PCT",
                "CTOX_AUTONOMY_LEVEL",
                "CTOX_CTO_OPERATING_MODE_PROMPT",
                "CTOX_CHAT_MODEL_BOOST",
                "CTOX_BOOST_DEFAULT_MINUTES",
                "CTOX_EMBEDDING_MODEL",
                "CTOX_STT_MODEL",
                "CTOX_TTS_MODEL",
            ]
        );

        let provider_idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "CTOX_API_PROVIDER")
            .unwrap();
        let token_idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "OPENAI_API_KEY")
            .unwrap();
        app.settings_items[provider_idx].value = "openai".to_string();
        app.settings_items[token_idx].value = "sk-test".to_string();
        app.refresh_dynamic_setting_choices();
        let api_keys = app
            .visible_setting_indices()
            .into_iter()
            .map(|idx| app.settings_items[idx].key)
            .collect::<Vec<_>>();
        assert_eq!(
            api_keys,
            vec![
                "CTOX_SERVICE_TOGGLE",
                "CTOX_API_PROVIDER",
                "CTOX_OPENAI_AUTH_MODE",
                "CTOX_LOCAL_RUNTIME",
                "OPENAI_API_KEY",
                "CTOX_CHAT_MODEL",
                "CTOX_CHAT_LOCAL_PRESET",
                "CTOX_CHAT_SKILL_PRESET",
                "CTOX_REFRESH_OUTPUT_BUDGET_PCT",
                "CTOX_AUTONOMY_LEVEL",
                "CTOX_CTO_OPERATING_MODE_PROMPT",
                "CTOX_CHAT_MODEL_BOOST",
                "CTOX_BOOST_DEFAULT_MINUTES",
                "CTOX_EMBEDDING_MODEL",
                "CTOX_STT_MODEL",
                "CTOX_TTS_MODEL",
            ]
        );

        let base_idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "CTOX_CHAT_MODEL")
            .unwrap();
        app.settings_items[base_idx].value = "gpt-5.4-mini".to_string();
        app.refresh_dynamic_setting_choices();
        let api_base_keys = app
            .visible_setting_indices()
            .into_iter()
            .map(|idx| app.settings_items[idx].key)
            .collect::<Vec<_>>();
        assert_eq!(
            api_base_keys,
            vec![
                "CTOX_SERVICE_TOGGLE",
                "CTOX_API_PROVIDER",
                "CTOX_OPENAI_AUTH_MODE",
                "OPENAI_API_KEY",
                "CTOX_CHAT_MODEL",
                "CTOX_CHAT_SKILL_PRESET",
                "CTOX_REFRESH_OUTPUT_BUDGET_PCT",
                "CTOX_AUTONOMY_LEVEL",
                "CTOX_CTO_OPERATING_MODE_PROMPT",
                "CTOX_CHAT_MODEL_BOOST",
                "CTOX_BOOST_DEFAULT_MINUTES",
                "CTOX_EMBEDDING_MODEL",
                "CTOX_STT_MODEL",
                "CTOX_TTS_MODEL",
            ]
        );
    }

    #[test]
    fn communication_settings_show_admin_and_channel_fields() {
        let root = temp_root("communication-flow");
        let db_path = root.join("runtime/test.sqlite3");
        let mut app = App::new(root, db_path);
        app.switch_settings_view(SettingsView::Communication);
        let keys = app
            .visible_setting_indices()
            .into_iter()
            .map(|idx| app.settings_items[idx].key)
            .collect::<Vec<_>>();
        assert!(keys.contains(&"CTOX_SERVICE_TOGGLE"));
        assert!(keys.contains(&"CTOX_OWNER_NAME"));
        assert!(keys.contains(&"CTOX_OWNER_EMAIL_ADDRESS"));
        assert!(keys.contains(&"CTOX_FOUNDER_EMAIL_ADDRESSES"));
        assert!(keys.contains(&"CTOX_FOUNDER_EMAIL_ROLES"));
        assert!(keys.contains(&"CTOX_ALLOWED_EMAIL_DOMAIN"));
        assert!(keys.contains(&"CTOX_EMAIL_ADMIN_POLICIES"));
        assert!(keys.contains(&"CTOX_OWNER_PREFERRED_CHANNEL"));
        assert!(keys.contains(&"CTOX_REMOTE_BRIDGE_MODE"));
        assert!(!keys.contains(&"CTOX_WEBRTC_SIGNALING_URL"));
        assert!(!keys.contains(&"CTOX_WEBRTC_ROOM"));
        assert!(!keys.contains(&"CTOX_WEBRTC_PASSWORD"));
    }

    #[test]
    fn cto_contract_setting_opens_editor_and_persists_to_runtime_store() {
        let root = temp_root("cto-contract-editor");
        let db_path = root.join("runtime/test.sqlite3");
        let mut app = App::new(root.clone(), db_path);
        let idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "CTOX_CTO_OPERATING_MODE_PROMPT")
            .unwrap();
        app.settings_selected = idx;

        app.handle_settings_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert!(app.settings_text_editor.is_some());

        {
            let editor = &mut app.settings_text_editor.as_mut().unwrap().editor;
            *editor = TextEditor::scratch("## CTO Operating Mode\n\nCustom.\n");
        }
        app.handle_key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))
            .unwrap();

        assert!(app.settings_text_editor.is_none());
        let saved = runtime_env::load_runtime_env_map(&root).unwrap();
        assert_eq!(
            saved
                .get("CTOX_CTO_OPERATING_MODE_PROMPT")
                .map(String::as_str),
            Some("## CTO Operating Mode\n\nCustom.")
        );
    }

    #[test]
    fn communication_settings_show_remote_webrtc_fields_only_when_enabled() {
        let root = temp_root("communication-remote-webrtc");
        let db_path = root.join("runtime/test.sqlite3");
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_REMOTE_BRIDGE_MODE".to_string(),
            "remote-webrtc".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        let mut app = App::new(root, db_path);
        app.switch_settings_view(SettingsView::Communication);
        let keys = app
            .visible_setting_indices()
            .into_iter()
            .map(|idx| app.settings_items[idx].key)
            .collect::<Vec<_>>();
        assert!(keys.contains(&"CTOX_REMOTE_BRIDGE_MODE"));
        assert!(keys.contains(&"CTOX_WEBRTC_SIGNALING_URL"));
        assert!(keys.contains(&"CTOX_WEBRTC_ROOM"));
        assert!(keys.contains(&"CTOX_WEBRTC_PASSWORD"));
    }

    #[test]
    fn model_settings_show_preset_immediately_after_base_model() {
        let root = temp_root("preset-order");
        let db_path = root.join("runtime/test.sqlite3");
        let app = App::new(root, db_path);
        let visible = app.visible_setting_indices();
        let model_pos = visible
            .iter()
            .position(|idx| app.settings_items[*idx].key == "CTOX_CHAT_MODEL")
            .unwrap();
        let preset_pos = visible
            .iter()
            .position(|idx| app.settings_items[*idx].key == "CTOX_CHAT_LOCAL_PRESET")
            .unwrap();
        assert_eq!(preset_pos, model_pos + 1);
        let skill_preset_pos = visible
            .iter()
            .position(|idx| app.settings_items[*idx].key == "CTOX_CHAT_SKILL_PRESET")
            .unwrap();
        assert_eq!(skill_preset_pos, preset_pos + 1);
    }

    #[test]
    fn configured_runtime_models_skip_remote_chat_model_for_api_source() {
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "api".to_string());
        env_map.insert("CTOX_CHAT_MODEL".to_string(), "gpt-5.4-mini".to_string());

        let models = configured_runtime_models(&env_map);

        assert!(!models.iter().any(|model| model == "gpt-5.4-mini"));
        assert!(models
            .iter()
            .any(|model| model == "Qwen/Qwen3-Embedding-0.6B"));
    }

    #[test]
    fn api_estimate_keeps_aux_models_without_projecting_local_chat_gpu_load() {
        let root = temp_root("api-estimate-aux");
        let db_path = root.join("runtime/test.sqlite3");
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "local".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL".to_string(),
            "openai/gpt-oss-120b".to_string(),
        );
        env_map.insert(
            "CTOX_EMBEDDING_MODEL".to_string(),
            "Qwen/Qwen3-Embedding-0.6B".to_string(),
        );
        env_map.insert(
            "CTOX_STT_MODEL".to_string(),
            "engineai/Voxtral-Mini-4B-Realtime-2602".to_string(),
        );
        env_map.insert(
            "CTOX_TTS_MODEL".to_string(),
            "engineai/Voxtral-4B-TTS-2603".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        let mut app = App::new(root, db_path);
        app.gpu_cards = vec![
            GpuCardState {
                index: 0,
                name: "RTX A4500".to_string(),
                used_mb: 12_000,
                total_mb: 20_480,
                utilization: 0,
                allocations: vec![
                    GpuModelUsage {
                        model: "openai/gpt-oss-120b".to_string(),
                        short_label: "gpt-oss-120b".to_string(),
                        used_mb: 8_386,
                    },
                    GpuModelUsage {
                        model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                        short_label: "embed".to_string(),
                        used_mb: 1_100,
                    },
                    GpuModelUsage {
                        model: "engineai/Voxtral-Mini-4B-Realtime-2602".to_string(),
                        short_label: "stt".to_string(),
                        used_mb: 1_400,
                    },
                    GpuModelUsage {
                        model: "engineai/Voxtral-4B-TTS-2603".to_string(),
                        short_label: "tts".to_string(),
                        used_mb: 1_100,
                    },
                ],
            },
            GpuCardState {
                index: 1,
                name: "RTX A4500".to_string(),
                used_mb: 8_386,
                total_mb: 20_480,
                utilization: 0,
                allocations: vec![GpuModelUsage {
                    model: "openai/gpt-oss-120b".to_string(),
                    short_label: "gpt-oss-120b".to_string(),
                    used_mb: 8_386,
                }],
            },
        ];

        let source_idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "CTOX_CHAT_SOURCE")
            .unwrap();
        let provider_idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "CTOX_API_PROVIDER")
            .unwrap();
        let model_idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "CTOX_CHAT_MODEL")
            .unwrap();
        app.settings_items[source_idx].value = "api".to_string();
        app.settings_items[provider_idx].value = "openai".to_string();
        app.settings_items[model_idx].value = "gpt-5.4-mini".to_string();

        app.refresh_dynamic_setting_choices();
        app.refresh_header();

        assert!(app.header.estimate_mode);
        assert_eq!(app.header.chat_source, "api");
        assert_eq!(app.header.model, "gpt-5.4-mini");
        assert!(app.header.gpu_target_cards.iter().all(|card| {
            card.allocations
                .iter()
                .all(|alloc| alloc.model != "openai/gpt-oss-120b")
        }));
        let allocations = app
            .header
            .gpu_target_cards
            .iter()
            .flat_map(|card| {
                card.allocations
                    .iter()
                    .map(|alloc| alloc.short_label.as_str())
            })
            .collect::<Vec<_>>();
        assert!(allocations.contains(&"embed"));
        assert!(allocations.contains(&"stt"));
        assert!(allocations.contains(&"tts"));
    }

    #[test]
    fn healthy_aux_backend_falls_back_to_planned_gpu_allocation_when_live_probe_misses_it() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_EMBEDDING_MODEL".to_string(),
            "Qwen/Qwen3-Embedding-0.6B".to_string(),
        );
        env_map.insert(
            "CTOX_STT_MODEL".to_string(),
            "engineai/Voxtral-Mini-4B-Realtime-2602".to_string(),
        );
        env_map.insert(
            "CTOX_TTS_MODEL".to_string(),
            "Qwen/Qwen3-TTS-12Hz-0.6B-Base [GPU]".to_string(),
        );

        let live_cards = vec![GpuCardState {
            index: 0,
            name: "RTX A4500".to_string(),
            used_mb: 2_996,
            total_mb: 20_480,
            utilization: 0,
            allocations: vec![
                GpuModelUsage {
                    model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                    short_label: "embed".to_string(),
                    used_mb: 1_100,
                },
                GpuModelUsage {
                    model: "engineai/Voxtral-Mini-4B-Realtime-2602".to_string(),
                    short_label: "stt".to_string(),
                    used_mb: 1_896,
                },
            ],
        }];
        let runtime_health = RuntimeHealthState {
            runtime_ready: false,
            embedding_ready: Some(true),
            stt_ready: Some(true),
            tts_ready: Some(true),
        };

        let cards = healthy_backend_deployed_cards(&live_cards, &env_map, &runtime_health);
        assert!(cards.iter().any(|card| {
            card.allocations
                .iter()
                .any(|alloc| alloc.model == "Qwen/Qwen3-TTS-12Hz-0.6B-Base")
        }));
    }

    #[test]
    fn communication_visibility_switches_email_and_jami_blocks() {
        let root = temp_root("comm-visibility");
        let db_path = root.join("runtime/test.sqlite3");
        let mut app = App::new(root, db_path);

        let owner_row = app
            .settings_items
            .iter()
            .find(|item| item.key == "CTOX_OWNER_NAME")
            .unwrap()
            .clone();
        let email_protocol_row = app
            .settings_items
            .iter()
            .find(|item| item.key == "CTO_EMAIL_PROVIDER")
            .unwrap()
            .clone();
        let email_password_row = app
            .settings_items
            .iter()
            .find(|item| item.key == "CTO_EMAIL_PASSWORD")
            .unwrap()
            .clone();
        let jami_row = app
            .settings_items
            .iter()
            .find(|item| item.key == "CTO_JAMI_ACCOUNT_ID")
            .unwrap()
            .clone();
        let ews_url_row = app
            .settings_items
            .iter()
            .find(|item| item.key == "CTO_EMAIL_EWS_URL")
            .unwrap()
            .clone();

        assert!(app.setting_visible(&owner_row));
        assert!(!app.setting_visible(&email_protocol_row));
        assert!(!app.setting_visible(&email_password_row));
        assert!(!app.setting_visible(&jami_row));

        app.settings_items
            .iter_mut()
            .find(|item| item.key == "CTOX_OWNER_PREFERRED_CHANNEL")
            .unwrap()
            .value = "email".to_string();
        assert!(app.setting_visible(&email_protocol_row));
        assert!(app.setting_visible(&email_password_row));
        assert!(!app.setting_visible(&jami_row));

        app.settings_items
            .iter_mut()
            .find(|item| item.key == "CTO_EMAIL_PROVIDER")
            .unwrap()
            .value = "ews".to_string();
        assert!(app.setting_visible(&ews_url_row));

        app.settings_items
            .iter_mut()
            .find(|item| item.key == "CTOX_OWNER_PREFERRED_CHANNEL")
            .unwrap()
            .value = "jami".to_string();
        assert!(!app.setting_visible(&email_protocol_row));
        assert!(app.setting_visible(&jami_row));
    }

    #[test]
    fn refresh_header_ignores_stale_local_active_model_when_chat_source_is_api() {
        let root = temp_root("api-header");
        let db_path = root.join("runtime/test.sqlite3");
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_CHAT_SOURCE".to_string(), "api".to_string());
        env_map.insert("CTOX_CHAT_MODEL".to_string(), "gpt-5.4-mini".to_string());
        env_map.insert(
            "CTOX_CHAT_MODEL_BASE".to_string(),
            "gpt-5.4-mini".to_string(),
        );
        env_map.insert(
            "CTOX_ACTIVE_MODEL".to_string(),
            "Qwen/Qwen3.5-35B-A3B".to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        let mut app = App::new(root, db_path);
        app.refresh_header();

        assert_eq!(app.header.chat_source, "api");
        assert_eq!(app.header.model, "gpt-5.4-mini");
        assert_eq!(app.header.base_model, "gpt-5.4-mini");
    }

    #[test]
    fn load_skill_catalog_discovers_skill_scripts_and_resources() {
        let root = temp_root("skills-catalog");
        let skill_dir = root.join("skills/.system/demo-skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::create_dir_all(skill_dir.join("references")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Demo Skill\n\nUse this skill when the operator wants a demo workflow.\n",
        )
        .unwrap();
        fs::write(skill_dir.join("scripts/demo_tool.py"), "print('ok')\n").unwrap();
        fs::write(skill_dir.join("references/example.md"), "reference\n").unwrap();

        let catalog = load_skill_catalog(&root);
        let entry = catalog
            .iter()
            .find(|entry| entry.name == "demo-skill")
            .unwrap();
        assert_eq!(
            entry.description,
            "Use this skill when the operator wants a demo workflow."
        );
        assert_eq!(entry.class, SkillClass::CtoxCore);
        assert_eq!(entry.state, SkillState::Stable);
        assert!(entry.helper_tools.iter().any(|tool| tool == "demo_tool.py"));
        assert!(entry
            .resources
            .iter()
            .any(|resource| resource.contains("references:")));
    }

    #[test]
    fn load_skill_catalog_classifies_curated_codex_and_personal_skills() {
        let root = temp_root("skills-classes");
        let curated_dir = root.join("skills/.curated/demo-pack");
        fs::create_dir_all(&curated_dir).unwrap();
        fs::write(
            curated_dir.join("SKILL.md"),
            "---\nname: demo-pack\ndescription: demo pack\n---\n",
        )
        .unwrap();

        let external_root = root.join("sandbox/managed-skills");
        let generated_root = root.join("runtime/generated-skills");
        let mut env_map = BTreeMap::new();
        env_map.insert(
            "CTOX_SKILLS_ROOT".to_string(),
            external_root.display().to_string(),
        );
        env_map.insert(
            "CTOX_GENERATED_SKILLS_ROOT".to_string(),
            generated_root.display().to_string(),
        );
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();

        let authored_dir = external_root.join("authored/openai-docs");
        fs::create_dir_all(&authored_dir).unwrap();
        fs::write(
            authored_dir.join("SKILL.md"),
            "---\nname: openai-docs\ndescription: docs\n---\n",
        )
        .unwrap();

        let personal_dir = generated_root.join("note-helper");
        fs::create_dir_all(&personal_dir).unwrap();
        fs::write(
            personal_dir.join("SKILL.md"),
            "---\nname: note-helper\ndescription: personal helper\n---\n",
        )
        .unwrap();

        let catalog = load_skill_catalog(&root);

        let curated = catalog
            .iter()
            .find(|entry| entry.name == "demo-pack")
            .unwrap();
        assert_eq!(curated.class, SkillClass::InstalledPacks);
        assert_eq!(curated.state, SkillState::Stable);

        let authored = catalog
            .iter()
            .find(|entry| entry.name == "openai-docs" && entry.class == SkillClass::Personal)
            .unwrap();
        assert_eq!(authored.state, SkillState::Authored);

        let personal = catalog
            .iter()
            .find(|entry| entry.name == "note-helper")
            .unwrap();
        assert_eq!(personal.class, SkillClass::Personal);
        assert_eq!(personal.state, SkillState::Generated);
    }

    #[test]
    fn tab_cycles_chat_skills_settings() {
        let root = temp_root("page-cycle");
        let db_path = root.join("runtime/test.sqlite3");
        let mut app = App::new(root, db_path);
        assert_eq!(app.page, Page::Chat);
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Skills);
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Costs);
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::Model);
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::Communication);
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::Secrets);
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::Paths);
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::Update);
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::BusinessOs);
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::HarnessMining);
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.settings_view, SettingsView::HarnessFlow);
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.page, Page::Chat);
    }

    #[test]
    fn secrets_view_saves_back_into_encrypted_store() {
        let root = temp_root("secret-save");
        let db_path = root.join("runtime/test.sqlite3");
        secrets::set_credential(&root, "OPENAI_API_KEY", "sk-old").unwrap();

        let mut app = App::new(root.clone(), db_path);
        app.switch_settings_view(SettingsView::Secrets);
        let idx = app
            .secret_items
            .iter()
            .position(|item| item.scope == "credentials" && item.name == "OPENAI_API_KEY")
            .unwrap();
        app.secrets_selected = idx;
        app.current_secret_mut().unwrap().value = "sk-new".to_string();
        app.save_current_secret().unwrap();

        assert_eq!(
            secrets::get_credential(&root, "OPENAI_API_KEY").as_deref(),
            Some("sk-new")
        );
    }

    #[test]
    fn jami_error_lines_surface_missing_dbus_session_explicitly() {
        let lines = jami_error_lines(
            "failed to connect to session DBus",
            None,
            false,
            &[JamiDoctorCheck {
                name: "dbus_session".to_string(),
                ok: false,
                detail: "session bus unavailable".to_string(),
            }],
        );
        let joined = lines.join("\n");
        assert!(joined.contains("Jami runtime is not ready."));
        assert!(joined.contains("user DBus session"));
        assert!(joined.contains("No configured Jami account/profile"));
        assert!(joined.contains("ensure a Linux user DBus session"));
    }

    #[test]
    fn jami_missing_account_lines_explain_missing_runtime_account() {
        let lines = jami_missing_account_lines(
            Some("/tmp/cto-jami-dbus.env"),
            true,
            &[JamiDoctorCheck {
                name: "configured_identity".to_string(),
                ok: false,
                detail: "missing".to_string(),
            }],
        );
        let joined = lines.join("\n");
        assert!(joined.contains("No live Jami RING account is available yet."));
        assert!(joined.contains("Configured account/profile could not be resolved"));
        assert!(joined.contains("/tmp/cto-jami-dbus.env"));
        assert!(joined.contains("CTO_JAMI_ACCOUNT_ID"));
    }

    #[test]
    fn jami_error_lines_include_runtime_bootstrap_hints() {
        let lines = jami_error_lines(
            "runtime unavailable",
            Some("/tmp/cto-jami-dbus.env"),
            true,
            &[
                JamiDoctorCheck {
                    name: "dbus_env_file".to_string(),
                    ok: false,
                    detail: "missing".to_string(),
                },
                JamiDoctorCheck {
                    name: "jami_runtime".to_string(),
                    ok: false,
                    detail: "missing".to_string(),
                },
            ],
        );
        let joined = lines.join("\n");
        assert!(joined.contains("CTO_JAMI_DBUS_ENV_FILE"));
        assert!(joined.contains("brew install --cask jami"));
    }

    #[test]
    fn jami_error_lines_surface_backend_mode_hint() {
        let lines = jami_error_lines(
            "backend unsupported",
            None,
            true,
            &[JamiDoctorCheck {
                name: "automation_backend".to_string(),
                ok: false,
                detail: "libwrap".to_string(),
            }],
        );
        let joined = lines.join("\n");
        assert!(joined.contains("manual share URI"));
        assert!(joined.contains("Linux runtime"));
    }

    #[test]
    fn save_settings_mirrors_chat_model_to_base_model() {
        let root = temp_root("boost-save");
        let db_path = root.join("runtime/test.sqlite3");
        let mut app = App::new(root.clone(), db_path);
        let provider_idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "CTOX_API_PROVIDER")
            .unwrap();
        let chat_idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "CTOX_CHAT_MODEL")
            .unwrap();
        app.settings_items[provider_idx].value = "openai".to_string();
        app.settings_items[chat_idx].value = "gpt-5.4-mini".to_string();
        app.save_settings().unwrap();
        if let Some(rx) = app.runtime_switch_rx.take() {
            let result = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("runtime switch result");
            assert!(result.is_ok(), "runtime switch failed: {result:?}");
        }

        let saved = runtime_env::load_runtime_env_map(&root).unwrap();
        assert_eq!(
            saved.get("CTOX_CHAT_MODEL_BASE").map(String::as_str),
            Some("gpt-5.4-mini")
        );
    }

    #[test]
    fn save_settings_persists_openai_token_only_in_secret_store() {
        let root = temp_root("settings-openai-secret");
        let db_path = root.join("runtime/test.sqlite3");
        let mut app = App::new(root.clone(), db_path);
        let token_idx = app
            .settings_items
            .iter()
            .position(|item| item.key == "OPENAI_API_KEY")
            .unwrap();
        app.settings_items[token_idx].value = "sk-secret-store".to_string();
        app.save_settings().unwrap();

        assert_eq!(
            secrets::get_credential(&root, "OPENAI_API_KEY").as_deref(),
            Some("sk-secret-store")
        );
        let conn = rusqlite::Connection::open(crate::persistence::sqlite_path(&root)).unwrap();
        let persisted = match conn.query_row(
            "SELECT env_value FROM runtime_env_kv WHERE env_key = 'OPENAI_API_KEY'",
            [],
            |row| row.get::<_, String>(0),
        ) {
            Ok(value) => Some(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(err) => panic!("unexpected sqlite error: {err}"),
        };
        let all_rows = {
            let mut stmt = conn
                .prepare("SELECT env_key, env_value FROM runtime_env_kv ORDER BY env_key ASC")
                .unwrap();
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .unwrap();
            rows.map(|row| row.unwrap()).collect::<Vec<_>>()
        };
        assert!(
            persisted.is_none(),
            "persisted={persisted:?} rows={all_rows:?}"
        );
    }

    #[test]
    fn save_settings_persists_azure_foundry_selection_and_secret() {
        let root = temp_root("settings-azure-foundry");
        let db_path = root.join("runtime/test.sqlite3");
        let mut app = App::new(root.clone(), db_path);

        for (key, value) in [
            ("CTOX_API_PROVIDER", "azure_foundry"),
            (
                AZURE_FOUNDRY_ENDPOINT_KEY,
                "https://contoso.openai.azure.com",
            ),
            (AZURE_FOUNDRY_DEPLOYMENT_ID_KEY, "company-gpt-5"),
            (AZURE_FOUNDRY_TOKEN_KEY, "azure-secret-token"),
        ] {
            app.settings_items
                .iter_mut()
                .find(|item| item.key == key)
                .unwrap()
                .value = value.to_string();
        }
        app.refresh_dynamic_setting_choices();
        app.save_settings().unwrap();

        let state = runtime_state::load_or_resolve_runtime_state(&root)
            .unwrap()
            .clone();
        assert_eq!(state.source, runtime_state::InferenceSource::Api);
        assert_eq!(state.base_model.as_deref(), Some("company-gpt-5"));
        assert_eq!(
            state.upstream_base_url.as_str(),
            "https://contoso.openai.azure.com/openai/v1"
        );
        assert_eq!(
            secrets::get_credential(&root, AZURE_FOUNDRY_TOKEN_KEY).as_deref(),
            Some("azure-secret-token")
        );
    }
}
