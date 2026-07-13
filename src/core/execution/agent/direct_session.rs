// Origin: CTOX
// License: AGPL-3.0-only
//
// Direct session: in-process ctox-core integration via InProcessAppServerClient.
// One persistent client for the normal CTOX worker lane. Sequential work
// slices and their continuity refreshes reuse the same durable thread.

use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ctox_app_server_client::{
    InProcessAppServerClient, InProcessClientStartArgs, InProcessServerEvent,
    DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
};
use ctox_app_server_protocol::{
    ClientRequest, JSONRPCNotification, RequestId, ServerNotification, ThreadCompactStartParams,
    ThreadCompactStartResponse, ThreadListParams, ThreadListResponse, ThreadResumeParams,
    ThreadResumeResponse, ThreadSetNameParams, ThreadSetNameResponse, ThreadSortKey,
    ThreadSourceKind, ThreadStartParams, ThreadStartResponse, TurnInterruptParams,
    TurnInterruptResponse, TurnStartParams, TurnStartResponse,
};
use ctox_arg0::Arg0DispatchPaths;
use ctox_cloud_requirements::cloud_requirements_loader;
use ctox_core::config::{
    find_codex_home, load_config_as_toml_with_cli_overrides, ConfigBuilder, ConfigOverrides,
};
use ctox_core::models_manager::collaboration_mode_presets::CollaborationModesConfig;
use ctox_core::AuthManager;
use ctox_core::ThreadManager;
use ctox_feedback::CodexFeedback;
use ctox_protocol::config_types::SandboxMode;
use ctox_protocol::openai_models::ReasoningEffort;
use ctox_protocol::protocol::{
    AskForApproval, EventMsg, ReadOnlyAccess, SandboxPolicy, SessionSource, SubAgentSource,
};
use ctox_protocol::user_input::UserInput;
use ctox_utils_absolute_path::AbsolutePathBuf;

use crate::api_costs::{self, ApiCallTelemetry, ApiTokenUsage};
use crate::context::compact::{CompactDecision, CompactPolicy, CompactTrigger};
use crate::context::live_context;
use crate::inference::engine;
use crate::inference::runtime_kernel;
use crate::inference::runtime_state;
use crate::secrets;

const OPENAI_AUTH_MODE_KEY: &str = "CTOX_OPENAI_AUTH_MODE";
const OPENAI_AUTH_MODE_CHATGPT_SUBSCRIPTION: &str = "chatgpt_subscription";
const CHATGPT_AUTH_SECRET_SCOPE: &str = "ctox-auth";
const CHATGPT_AUTH_SECRET_NAME: &str = "chatgpt_subscription_auth_json";
const DIRECT_SESSION_CONTROL_REQUEST_TIMEOUT_SECS: u64 = 5;
const DIRECT_SESSION_MIDTASK_COMPACT_TIMEOUT_SECS: u64 = 90;
// Interrupt delivery is load-bearing for session health: if the interrupt
// never lands, the server-side turn keeps running after the caller bailed
// and the durable thread accumulates a dangling active turn (ctox#21). The
// previous 2s cap regularly expired while the event pipeline was catching
// up; the drain-while-interrupting loop below plus this larger cap make the
// interrupt reliable.
const DIRECT_SESSION_INTERRUPT_TIMEOUT_SECS: u64 = 10;
// turn/start loads the durable thread rollout before submitting input, so it
// gets a more generous bound than ordinary control requests — but it must be
// bounded: an unbounded await here hangs the whole prompt worker when the
// session runtime is wedged (ctox#21).
const DIRECT_SESSION_TURN_START_TIMEOUT_SECS: u64 = 30;
const EXACT_PROMPT_SAFE_INPUT_BUDGET_NUMERATOR: i64 = 3;
const EXACT_PROMPT_SAFE_INPUT_BUDGET_DENOMINATOR: i64 = 4;
const CTOX_PERSISTENT_WORKER_THREAD_NAME: &str = "ctox-service-worker";
#[cfg(test)]
static DIRECT_SESSION_EVENT_DESERIALIZE_CALLS: AtomicUsize = AtomicUsize::new(0);

fn persistent_worker_thread_name(root: &Path) -> String {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let digest = Sha256::digest(canonical_root.to_string_lossy().as_bytes());
    let suffix = digest[..6].iter().fold(String::new(), |mut value, byte| {
        value.push_str(&format!("{byte:02x}"));
        value
    });
    format!("{CTOX_PERSISTENT_WORKER_THREAD_NAME}-{}", suffix)
}

fn create_reviewer_scratch_workspace() -> Result<PathBuf> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "ctox-review-scratch-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path)
        .with_context(|| format!("failed to create reviewer scratch at {}", path.display()))?;
    Ok(path)
}

const CTOX_DIRECT_SESSION_BASE_INSTRUCTIONS: &str = r#"You are an agent working inside CTOX.

Complete a work step only when the required durable outcome exists in CTOX runtime state. A final answer, summary, note file, or statement such as "sent", "done", or "closed" is not evidence by itself.

When the request requires filesystem changes, command execution, runtime inspection, benchmark execution, ticket/state updates, or artifact verification, use the available terminal/shell tools to do the work. Do not substitute a code block, plan, or textual description for executing the step.

If the work requires an artifact, verify the artifact before finishing. For proactive outbound email, produce the final send-ready body first and do not run reviewed-send before review feedback. When a reviewed-send continuation prompt provides the exact approved body and command, execute only that command and verify the accepted outbound row. Do not create review rows or approval digests manually.

Do not create review-driven internal work.

If an API, provider, tool, or runtime call fails or is rate-limited, do not claim completion. Retry only when appropriate; otherwise keep the work open with the blocker recorded.

Use plain English in your own reasoning and replies. Do not expose internal source-code labels when a normal phrase is clearer; for example, say "work step" or "agent run" instead of "slice"."#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExactPromptTokenCount {
    pub tokens: i64,
    pub context_limit: i64,
    pub source: String,
}

pub(crate) fn exact_prompt_safe_input_budget(context_limit: i64) -> i64 {
    if context_limit <= 0 {
        return 1;
    }
    context_limit
        .saturating_mul(EXACT_PROMPT_SAFE_INPUT_BUDGET_NUMERATOR)
        .checked_div(EXACT_PROMPT_SAFE_INPUT_BUDGET_DENOMINATOR)
        .unwrap_or(1)
        .max(1)
}

pub(crate) fn exact_prompt_token_count(
    root: &Path,
    text: &str,
) -> Result<Option<ExactPromptTokenCount>> {
    exact_prompt_token_count_with_precomputed(root, text, None)
}

fn exact_prompt_token_count_with_precomputed(
    root: &Path,
    text: &str,
    precomputed: Option<&ExactPromptTokenCount>,
) -> Result<Option<ExactPromptTokenCount>> {
    if let Some(precomputed) = precomputed {
        return Ok(Some(precomputed.clone()));
    }
    let kernel = runtime_kernel::InferenceRuntimeKernel::resolve(root)
        .context("failed to resolve runtime kernel for exact token preflight")?;
    if !kernel.state.source.is_local() {
        return Ok(None);
    }
    let binding = kernel.primary_generation.as_ref().context(
        "exact token preflight unavailable: local runtime has no primary generation binding",
    )?;
    let base_url = tokenizer_base_url(binding)?;
    let tokens = count_llama_tokenize_endpoint(&base_url, text).with_context(|| {
        format!(
            "exact token preflight failed via {} for {}",
            base_url, binding.request_model
        )
    })?;
    Ok(Some(ExactPromptTokenCount {
        tokens,
        context_limit: kernel.turn_context_tokens(),
        source: format!("{} /tokenize", binding.request_model),
    }))
}

fn tokenizer_base_url(binding: &runtime_kernel::ResolvedRuntimeBinding) -> Result<String> {
    let base_url = binding.base_url.trim().trim_end_matches('/');
    if !base_url.is_empty() {
        return Ok(base_url.to_string());
    }
    if let Some(base_url) = binding.transport.http_base_url() {
        return Ok(base_url.trim_end_matches('/').to_string());
    }
    anyhow::bail!(
        "exact token preflight unavailable: {} exposes {} without HTTP tokenizer metadata",
        binding.request_model,
        binding.transport.display_label()
    )
}

fn count_llama_tokenize_endpoint(base_url: &str, text: &str) -> Result<i64> {
    let endpoint = format!("{}/tokenize", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "content": text,
        "add_special": false,
        "with_pieces": false,
    })
    .to_string();
    let response = ureq::post(&endpoint)
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(30))
        .send_string(&body);
    let response = response.map_err(|err| anyhow::anyhow!("POST {endpoint}: {err}"))?;
    let response_body = response
        .into_string()
        .map_err(|err| anyhow::anyhow!("read {endpoint} response: {err}"))?;
    parse_tokenize_count(&response_body)
}

fn parse_tokenize_count(body: &str) -> Result<i64> {
    let value: JsonValue =
        serde_json::from_str(body).context("failed to parse tokenizer response JSON")?;
    if let Some(tokens) = value.get("tokens").and_then(JsonValue::as_array) {
        return Ok(tokens.len() as i64);
    }
    for key in ["n_tokens", "token_count", "count"] {
        if let Some(count) = value.get(key).and_then(JsonValue::as_i64) {
            if count >= 0 {
                return Ok(count);
            }
        }
    }
    anyhow::bail!("tokenizer response did not contain tokens/n_tokens/token_count/count")
}

fn escape_json_fragment(value: &str) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "\"\"".to_string())
        .trim_matches('"')
        .to_string()
}

/// Compose the system prompt for a direct session.
///
/// Worker sessions (no override) receive the full CTOX system prompt rendered
/// from the runtime identity and settings, followed by the direct-session
/// execution contract. The long system prompt is the stability layer for
/// long-running worker behavior; it must reach the model, not just the
/// TUI/live-prompt diagnostics.
///
/// Sessions that pass an override (completion review, queue repair) own their
/// entire system prompt. They intentionally do not inherit the worker prompt:
/// a reviewer must not be instructed to perform worker actions.
fn compose_base_instructions(
    root: &Path,
    settings: &BTreeMap<String, String>,
    override_prompt: Option<&str>,
) -> Result<String> {
    if let Some(prompt) = override_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(prompt.to_string());
    }
    let system_prompt = live_context::render_system_prompt(root, settings)
        .context("failed to render CTOX worker system prompt")?;
    Ok(format!(
        "{}\n\n{CTOX_DIRECT_SESSION_BASE_INSTRUCTIONS}",
        system_prompt.trim_end()
    ))
}

fn openai_chatgpt_subscription_auth_enabled(settings: &BTreeMap<String, String>) -> bool {
    settings
        .get(OPENAI_AUTH_MODE_KEY)
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| {
            matches!(
                value.as_str(),
                OPENAI_AUTH_MODE_CHATGPT_SUBSCRIPTION
                    | "subscription"
                    | "codex_subscription"
                    | "chatgpt"
            )
        })
}

fn use_openai_chatgpt_subscription_auth(
    settings: &BTreeMap<String, String>,
    selected_api_provider: Option<&str>,
) -> bool {
    let provider_is_openai = selected_api_provider
        .map(|provider| provider.eq_ignore_ascii_case("openai"))
        .unwrap_or(true);
    provider_is_openai && openai_chatgpt_subscription_auth_enabled(settings)
}

fn direct_session_selected_model(
    settings: &BTreeMap<String, String>,
    runtime_model: Option<String>,
) -> String {
    runtime_model
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            settings
                .get("CTOX_CHAT_MODEL")
                .or_else(|| settings.get("CODEX_MODEL"))
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "gpt-5.4-mini".to_string())
}

fn restore_chatgpt_subscription_auth_from_instance(
    root: &Path,
    codex_home: &Path,
    auth_credentials_store_mode: ctox_core::auth::AuthCredentialsStoreMode,
) -> Result<bool> {
    let auth_manager =
        ctox_core::AuthManager::new(codex_home.to_path_buf(), false, auth_credentials_store_mode);
    if auth_manager
        .auth_cached()
        .as_ref()
        .is_some_and(|auth| auth.is_chatgpt_auth())
    {
        return Ok(false);
    }

    let serialized =
        match secrets::read_secret_value(root, CHATGPT_AUTH_SECRET_SCOPE, CHATGPT_AUTH_SECRET_NAME)
        {
            Ok(value) => value,
            Err(_) => return Ok(false),
        };
    let auth: ctox_core::auth::AuthDotJson =
        serde_json::from_str(&serialized).context("instance ChatGPT auth backup is invalid")?;
    if auth.tokens.is_none() {
        return Ok(false);
    }
    ctox_core::auth::save_auth(codex_home, &auth, auth_credentials_store_mode)
        .context("failed to restore ChatGPT Subscription auth into Codex auth store")?;
    Ok(true)
}

fn direct_session_reasoning_effort(
    settings: &BTreeMap<String, String>,
    model: &str,
    runtime_local_preset: Option<&str>,
) -> Option<ReasoningEffort> {
    for key in [
        "CTOX_CHAT_REASONING_EFFORT",
        "CTOX_MODEL_REASONING_EFFORT",
        "CODEX_MODEL_REASONING_EFFORT",
        "MODEL_REASONING_EFFORT",
    ] {
        if let Some(effort) = settings
            .get(key)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse::<ReasoningEffort>().ok())
        {
            return Some(effort);
        }
    }

    let preset = settings
        .get("CTOX_CHAT_LOCAL_PRESET")
        .map(String::as_str)
        .or(runtime_local_preset)
        .map(str::trim);
    if preset.is_some_and(|value| value.eq_ignore_ascii_case("performance"))
        && is_gpt_54_mini_model(model)
    {
        return Some(ReasoningEffort::Low);
    }

    None
}

fn is_gpt_54_mini_model(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    normalized == "gpt-5.4-mini" || normalized.ends_with("/gpt-5.4-mini")
}

fn direct_session_control_request_timeout(deadline: Option<tokio::time::Instant>) -> Duration {
    let default = Duration::from_secs(DIRECT_SESSION_CONTROL_REQUEST_TIMEOUT_SECS);
    direct_session_deadline_capped_timeout(default, deadline)
}

fn direct_session_midtask_compact_timeout(deadline: Option<tokio::time::Instant>) -> Duration {
    let default = Duration::from_secs(DIRECT_SESSION_MIDTASK_COMPACT_TIMEOUT_SECS);
    direct_session_deadline_capped_timeout(default, deadline)
}

fn direct_session_deadline_capped_timeout(
    default: Duration,
    deadline: Option<tokio::time::Instant>,
) -> Duration {
    let Some(deadline) = deadline else {
        return default;
    };
    let remaining = deadline
        .checked_duration_since(tokio::time::Instant::now())
        .unwrap_or_else(|| Duration::from_millis(1));
    if remaining.is_zero() {
        Duration::from_millis(1)
    } else {
        remaining.min(default)
    }
}

// ---------------------------------------------------------------------------
// PersistentSession — lives across normal worker turns and bounded helper turns
// ---------------------------------------------------------------------------

/// Holds a running InProcessAppServerClient + thread. Normal service work keeps
/// one instance across slices and resumes its rollout after restart. Isolated
/// reviewer/summarizer/special-profile callers still create bounded instances.
pub(crate) struct PersistentSession {
    runtime: Option<tokio::runtime::Runtime>,
    // These are wrapped in Option so we can take() them in shutdown.
    client: Option<InProcessAppServerClient>,
    thread_id: String,
    seq: RequestIdSeq,
    cwd: PathBuf,
    model: String,
    model_provider: Option<String>,
    api_provider: Option<String>,
    reasoning_effort: Option<ReasoningEffort>,
    policy: CompactPolicy,
    ctx_log: ContextLogger,
    root: PathBuf,
    base_instructions: String,
    disable_active_tools: bool,
    disable_mcp_servers: bool,
    read_only_sandbox: bool,
    persistent_worker: bool,
    reviewer_scratch_workspace: Option<PathBuf>,
}

impl PersistentSession {
    /// Start or resume the normal persistent worker session.
    pub fn start(root: &Path, settings: &BTreeMap<String, String>) -> Result<Self> {
        Self::start_with_instructions_and_tool_mode(
            root, settings, None, false, false, false, false, true,
        )
    }

    /// Start a worker session without configured MCP/plugin tool servers.
    ///
    /// The shell and apply_patch tools remain available; this only prevents
    /// unrelated MCP tool schemas from bloating narrow queue-job requests.
    pub(crate) fn start_without_mcp_servers_with_instructions(
        root: &Path,
        settings: &BTreeMap<String, String>,
        base_instructions: Option<&str>,
    ) -> Result<Self> {
        Self::start_with_instructions_and_tool_mode(
            root,
            settings,
            base_instructions,
            false,
            false,
            true,
            false,
            false,
        )
    }

    /// The composed base instructions this session sends with every thread.
    /// Exposed so the caller's token preflight can budget the same text the
    /// session-level preflight will count (base instructions + prompt).
    pub(crate) fn base_instructions(&self) -> &str {
        &self.base_instructions
    }

    /// Return whether stable instructions/model still match the durable
    /// runtime contract. A mismatch requires rebuilding the process-local
    /// client; startup then resumes the rollout with the new typed contract.
    pub(crate) fn matches_current_worker_contract(
        &self,
        root: &Path,
        settings: &BTreeMap<String, String>,
    ) -> Result<bool> {
        let base_instructions = compose_base_instructions(root, settings, None)?;
        let runtime_model = runtime_kernel::InferenceRuntimeKernel::resolve(root)
            .ok()
            .and_then(|runtime| {
                let state = runtime.state;
                state
                    .active_model
                    .clone()
                    .or_else(|| state.requested_model.clone())
                    .or_else(|| state.base_model.clone())
            });
        let model = direct_session_selected_model(settings, runtime_model);
        Ok(self.base_instructions == base_instructions && self.model == model)
    }

    /// Update the tool/sandbox working directory carried by the next turn.
    /// The thread remains the same; the existing typed turn-context override
    /// records the cwd change in rollout state for restart-safe resume.
    pub(crate) fn set_turn_cwd(&mut self, cwd: &Path) {
        self.cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    }

    /// Start a persistent session with explicit base instructions and optional
    /// compaction disablement. The instructions REPLACE the worker system
    /// prompt entirely. Review runs use this to create an isolated
    /// external-review thread with its own system prompt, without normal
    /// long-run compaction behavior, and without active execution tools.
    pub fn start_with_instructions(
        root: &Path,
        settings: &BTreeMap<String, String>,
        base_instructions: Option<&str>,
        disable_compaction: bool,
    ) -> Result<Self> {
        Self::start_with_instructions_and_tool_mode(
            root,
            settings,
            base_instructions,
            disable_compaction,
            disable_compaction,
            false,
            false,
            false,
        )
    }

    /// Start a review session that can inspect context with tools.
    ///
    /// Reviewers receive shell/read tools under a read-only sandbox. The tool
    /// registry removes patch, channel-send/ack/take, meeting mutation,
    /// collaboration, artifact, and agent-job tools for every read-only
    /// session, so the boundary is enforced below the prompt layer.
    pub fn start_review_with_read_only_tools(
        root: &Path,
        settings: &BTreeMap<String, String>,
        base_instructions: Option<&str>,
    ) -> Result<Self> {
        Self::start_with_instructions_and_tool_mode(
            root,
            settings,
            base_instructions,
            true,  // disable_compaction
            false, // disable_active_tools
            true,  // disable_mcp_servers
            true,  // read_only_sandbox
            false, // persistent_worker
        )
    }

    /// Start a reviewer-profile session with an authoritative empty tool set.
    ///
    /// Semantic answer review receives the complete bounded contract inline,
    /// so any restored or active tool would add cost and broaden the evidence
    /// surface without improving the verdict. The read-only reviewer sandbox
    /// and reviewer session metadata remain enforced even though no tool can
    /// be invoked.
    pub fn start_review_without_tools(
        root: &Path,
        settings: &BTreeMap<String, String>,
        base_instructions: Option<&str>,
    ) -> Result<Self> {
        Self::start_with_instructions_and_tool_mode(
            root,
            settings,
            base_instructions,
            true,  // disable_compaction
            true,  // disable_active_tools
            true,  // disable_mcp_servers
            true,  // read_only_sandbox
            false, // persistent_worker
        )
    }

    fn start_with_instructions_and_tool_mode(
        root: &Path,
        settings: &BTreeMap<String, String>,
        base_instructions: Option<&str>,
        disable_compaction: bool,
        disable_active_tools: bool,
        disable_mcp_servers: bool,
        read_only_sandbox: bool,
        persistent_worker: bool,
    ) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .context("failed to start tokio runtime")?;

        let composed_base_instructions =
            compose_base_instructions(root, settings, base_instructions)?;
        let reviewer_scratch_workspace = read_only_sandbox
            .then(create_reviewer_scratch_workspace)
            .transpose()?;
        let session_cwd = reviewer_scratch_workspace.as_deref().unwrap_or(root);
        let start_result = rt.block_on(async {
            Self::start_client_and_thread(
                root,
                session_cwd,
                settings,
                &composed_base_instructions,
                disable_active_tools,
                disable_mcp_servers,
                read_only_sandbox,
                persistent_worker,
            )
            .await
        });
        let (client, thread_id, cwd, seq, model, model_provider, api_provider, reasoning_effort) =
            match start_result {
                Ok(started) => started,
                Err(err) => {
                    if let Some(scratch) = reviewer_scratch_workspace.as_ref() {
                        let _ = std::fs::remove_dir_all(scratch);
                    }
                    return Err(err);
                }
            };

        let mut policy = CompactPolicy::from_settings(
            settings.get("CTOX_COMPACT_TRIGGER").map(String::as_str),
            settings.get("CTOX_COMPACT_MODE").map(String::as_str),
            settings
                .get("CTOX_COMPACT_FIXED_INTERVAL")
                .map(String::as_str),
            settings
                .get("CTOX_COMPACT_ADAPTIVE_THRESHOLD")
                .map(String::as_str),
            settings
                .get("CTOX_COMPACT_EMERGENCY_RATIO")
                .map(String::as_str),
            settings
                .get("CTOX_CHAT_MODEL_MAX_CONTEXT")
                .map(String::as_str),
        );
        if disable_compaction {
            // Reviewer sessions: turn off the adaptive output/input drift
            // trigger (the reviewer's reads/writes look very different from
            // a regular agent and would mis-fire), but keep the emergency
            // fill ratio at its default. A reviewer prompt that climbs near
            // the context limit must still be compacted via
            // ThreadCompactStart — otherwise it crashes the inference call
            // with exceed_context_size_error. The reviewer pathway does not
            // run the lcm context-engine rebuild (that only happens at the
            // start of a mission worker cycle in turn_loop.rs), so this
            // ThreadCompactStart only affects the harness-internal
            // conversation buffer, exactly as intended for a review run.
            policy.trigger = CompactTrigger::Off;
        }
        let ctx_log = ContextLogger::open(root);
        let mut ctx_log = ctx_log.with_session_kind(if disable_compaction {
            "review"
        } else {
            "mission"
        });
        ctx_log.log(
            "session_started",
            &format!(
                "\"session_kind\":\"{}\",\"thread_id\":\"{}\"",
                ctx_log.session_kind, thread_id
            ),
        );

        eprintln!(
            "[ctox direct-session] persistent session started thread_id={}",
            thread_id
        );

        Ok(Self {
            runtime: Some(rt),
            client: Some(client),
            thread_id,
            seq,
            cwd,
            model,
            model_provider,
            api_provider,
            reasoning_effort,
            policy,
            ctx_log,
            root: root.to_path_buf(),
            base_instructions: composed_base_instructions,
            disable_active_tools,
            disable_mcp_servers,
            read_only_sandbox,
            persistent_worker,
            reviewer_scratch_workspace,
        })
    }

    /// Run a single turn on the persistent session. Can be called multiple
    /// times (main turn, then refreshes). All share the same client+thread.
    pub fn run_turn(
        &mut self,
        prompt: &str,
        timeout: Option<Duration>,
        _base_instructions: Option<&str>,
        _include_apply_patch_tool: Option<bool>,
        _conversation_id: i64,
    ) -> Result<String> {
        self.run_turn_inner(prompt, timeout, None)
    }

    pub(crate) fn run_turn_inner(
        &mut self,
        prompt: &str,
        timeout: Option<Duration>,
        exact_prompt_preflight: Option<ExactPromptTokenCount>,
    ) -> Result<String> {
        self.run_turn_inner_with_context(prompt, None, timeout, exact_prompt_preflight)
    }

    pub(crate) fn run_turn_inner_with_context(
        &mut self,
        prompt: &str,
        developer_instructions: Option<&str>,
        timeout: Option<Duration>,
        exact_prompt_preflight: Option<ExactPromptTokenCount>,
    ) -> Result<String> {
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("session already shut down"))?;
        let mut thread_id = self.thread_id.clone();
        let cwd = self.cwd.clone();
        let model = self.model.clone();
        let model_provider = self.model_provider.clone();
        let api_provider = self.api_provider.clone();
        let reasoning_effort = self.reasoning_effort;
        let prompt = prompt.to_string();
        let developer_instructions = developer_instructions.map(str::to_string);
        let root = self.root.clone();
        let base_instructions = self.base_instructions.clone();
        let disable_active_tools = self.disable_active_tools;
        let disable_mcp_servers = self.disable_mcp_servers;
        let read_only_sandbox = self.read_only_sandbox;
        let persistent_worker = self.persistent_worker;
        self.ctx_log.log(
            "turn_request",
            &format!("\"prompt_len\":{},\"timeout\":{:?}", prompt.len(), timeout),
        );

        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session runtime already shut down"))?;
        let result = runtime.block_on(async {
            Self::run_turn_async(
                client,
                &mut thread_id,
                &cwd,
                &model,
                model_provider.as_deref(),
                api_provider.as_deref(),
                reasoning_effort,
                &root,
                &prompt,
                developer_instructions.as_deref(),
                &base_instructions,
                timeout,
                &mut self.seq,
                &mut self.policy,
                &mut self.ctx_log,
                disable_active_tools,
                disable_mcp_servers,
                read_only_sandbox,
                persistent_worker,
                exact_prompt_preflight,
            )
            .await
        });
        // Adopt a rotated thread id so follow-up turns in this session keep
        // using the live thread.
        self.thread_id = thread_id;

        result
    }

    /// Shut down the client and runtime cleanly.
    pub fn shutdown(mut self) {
        self.shutdown_inner("shutting down");
    }

    // --- Internal async helpers ---

    async fn start_client_and_thread(
        root: &Path,
        cwd: &Path,
        settings: &BTreeMap<String, String>,
        base_instructions: &str,
        disable_active_tools: bool,
        disable_mcp_servers: bool,
        read_only_sandbox: bool,
        persistent_worker: bool,
    ) -> Result<(
        InProcessAppServerClient,
        String,
        PathBuf,
        RequestIdSeq,
        String,
        Option<String>,
        Option<String>,
        Option<ReasoningEffort>,
    )> {
        let resolved_runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root).ok();
        let runtime_local_preset = resolved_runtime
            .as_ref()
            .and_then(|runtime| runtime.state.local_preset.clone());
        let runtime_model = resolved_runtime.as_ref().and_then(|runtime| {
            runtime
                .state
                .active_model
                .clone()
                .or_else(|| runtime.state.requested_model.clone())
                .or_else(|| runtime.state.base_model.clone())
        });
        let model = direct_session_selected_model(settings, runtime_model);
        let reasoning_effort =
            direct_session_reasoning_effort(settings, &model, runtime_local_preset.as_deref());
        let runtime_api_provider = resolved_runtime
            .as_ref()
            .filter(|runtime| !runtime.state.source.is_local())
            .map(|runtime| {
                runtime_state::api_provider_for_runtime_state(&runtime.state).to_string()
            });
        let selected_api_provider = settings
            .get("CTOX_API_PROVIDER")
            .map(|value| runtime_state::normalize_api_provider(value).to_string())
            .filter(|provider| {
                !provider.eq_ignore_ascii_case("local")
                    && engine::api_provider_supports_model(provider, &model)
            })
            .or_else(|| {
                runtime_api_provider.filter(|provider| {
                    !provider.eq_ignore_ascii_case("local")
                        && engine::api_provider_supports_model(provider, &model)
                })
            })
            .or_else(|| {
                let explicit_api_source = settings
                    .get("CTOX_CHAT_SOURCE")
                    .is_some_and(|value| value.trim().eq_ignore_ascii_case("api"));
                (explicit_api_source || engine::is_api_chat_model(&model))
                    .then(|| engine::default_api_provider_for_model(&model).to_string())
            });
        let cwd = cwd.to_path_buf();

        let codex_home =
            find_codex_home().map_err(|err| anyhow::anyhow!("find_codex_home: {err}"))?;
        let use_chatgpt_subscription_auth =
            use_openai_chatgpt_subscription_auth(settings, selected_api_provider.as_deref());

        let selected_api_key_name = selected_api_provider.as_deref().map(|provider| {
            runtime_state::api_key_env_var_for_provider_with_env_map(provider, settings)
        });
        let api_key = match selected_api_key_name {
            Some(key)
                if use_chatgpt_subscription_auth && key.eq_ignore_ascii_case("OPENAI_API_KEY") =>
            {
                None
            }
            Some(key) => settings
                .get(key)
                .cloned()
                .or_else(|| secrets::get_credential(root, key)),
            None => settings
                .get("OPENROUTER_API_KEY")
                .or_else(|| settings.get("ANTHROPIC_API_KEY"))
                .or_else(|| settings.get("MINIMAX_API_KEY"))
                .or_else(|| settings.get("AZURE_FOUNDRY_API_KEY"))
                .cloned()
                .or_else(|| secrets::get_credential(root, "OPENROUTER_API_KEY"))
                .or_else(|| secrets::get_credential(root, "ANTHROPIC_API_KEY"))
                .or_else(|| secrets::get_credential(root, "MINIMAX_API_KEY"))
                .or_else(|| secrets::get_credential(root, "AZURE_FOUNDRY_API_KEY")),
        }
        .filter(|v| !v.trim().is_empty());
        let config_cwd =
            AbsolutePathBuf::from_absolute_path(cwd.canonicalize().unwrap_or(cwd.clone()))
                .map_err(|err| anyhow::anyhow!("cwd resolve: {err}"))?;
        let config_toml = load_config_as_toml_with_cli_overrides(&codex_home, &config_cwd, vec![])
            .await
            .map_err(|err| anyhow::anyhow!("load config.toml: {err}"))?;
        let auth_credentials_store_mode =
            config_toml.cli_auth_credentials_store.unwrap_or_default();
        if use_chatgpt_subscription_auth {
            let _ = restore_chatgpt_subscription_auth_from_instance(
                root,
                &codex_home,
                auth_credentials_store_mode,
            );
        }

        let auth_manager = if let Some(ref key) = api_key {
            AuthManager::from_runtime_auth(
                ctox_core::CodexAuth::from_api_key(key),
                codex_home.clone(),
            )
        } else {
            AuthManager::shared(codex_home.clone(), false, auth_credentials_store_mode)
        };
        if use_chatgpt_subscription_auth {
            eprintln!(
                "[ctox direct-session] OpenAI auth mode=chatgpt_subscription; OPENAI_API_KEY ignored and API cost tracking disabled"
            );
        }
        if let Some(effort) = reasoning_effort {
            eprintln!(
                "[ctox direct-session] reasoning effort override model={} effort={:?}",
                model, effort
            );
        }
        let cloud_requirements = cloud_requirements_loader(
            auth_manager.clone(),
            config_toml
                .chatgpt_base_url
                .clone()
                .unwrap_or_else(|| "https://chatgpt.com/backend-api/".to_string()),
            codex_home.clone(),
        );

        // Resolve model-provider BEFORE building overrides
        let api_provider = super::turn_loop::resolve_api_model_provider_spec(
            &model,
            settings,
            resolved_runtime.as_ref(),
        );
        let local_provider =
            super::turn_loop::resolve_local_model_provider_spec(resolved_runtime.as_ref());
        if api_provider.is_some()
            && local_provider.is_none()
            && api_key.is_none()
            && !use_chatgpt_subscription_auth
        {
            anyhow::bail!(
                "API runtime requires provider credentials from the CTOX SQLite secret store or runtime settings; auth.json and process env fallbacks are disabled"
            );
        }
        if resolved_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.state.source.is_local())
            && local_provider.is_none()
        {
            anyhow::bail!(
                "CTOX local runtime requires socket-based Responses transport; no managed socket path is available"
            );
        }
        let selected_provider_id = local_provider
            .as_ref()
            .map(|provider| provider.provider_id.to_string())
            .or_else(|| {
                api_provider
                    .as_ref()
                    .map(|provider| provider.provider_id.to_string())
            });
        let tracking_api_provider = if use_chatgpt_subscription_auth {
            None
        } else {
            local_provider
                .is_none()
                .then(|| selected_api_provider.clone())
                .flatten()
        };
        let overrides = ConfigOverrides {
            model: Some(model.clone()),
            model_context_window: resolved_runtime
                .as_ref()
                .map(|runtime| runtime.turn_context_tokens())
                .filter(|value| *value > 0),
            model_provider: selected_provider_id.clone(),
            cwd: Some(cwd.clone()),
            approval_policy: Some(AskForApproval::Never),
            // Reviewers write only inside their disposable scratch cwd. The
            // authoritative workspace and runtime tree are outside the cwd
            // and therefore read-only under WorkspaceWrite.
            sandbox_mode: Some(SandboxMode::WorkspaceWrite),
            include_apply_patch_tool: Some(true),
            ephemeral: Some(true),
            disable_mcp_servers,
            ..Default::default()
        };
        // Hand ctox-core one of the two explicit CTOX provider modes:
        // `ctox_core_local` for managed socket-backed local runtimes or
        // `ctox_core_api` for remote providers. Both stay Responses-facing
        // from CTOX's perspective; any provider-specific wire adaptation
        // happens only at the outer edge.
        let mut cli_overrides: Vec<(String, toml::Value)> = vec![];
        if let Some(ref provider) = local_provider {
            cli_overrides.extend(provider.ctox_core_cli_overrides());
            eprintln!(
                "[ctox direct-session] provider mode=ctox_core_local id={} endpoint={} wire_api={}",
                provider.provider_id, provider.transport_endpoint, provider.wire_api
            );
        }
        if let Some(ref provider) = api_provider {
            cli_overrides.extend(provider.ctox_core_cli_overrides());
            eprintln!(
                "[ctox direct-session] provider mode=ctox_core_api id={} base_url={} wire_api={}",
                provider.provider_id, provider.base_url, provider.wire_api
            );
        }
        let config = Arc::new(
            ConfigBuilder::default()
                .cli_overrides(cli_overrides.clone())
                .harness_overrides(overrides)
                .cloud_requirements(cloud_requirements.clone())
                .build()
                .await
                .map_err(|err| anyhow::anyhow!("config build: {err}"))?,
        );
        let session_source = if read_only_sandbox {
            SessionSource::SubAgent(SubAgentSource::Review)
        } else {
            SessionSource::Exec
        };
        let thread_manager = Arc::new(ThreadManager::new(
            config.as_ref(),
            auth_manager.clone(),
            session_source.clone(),
            CollaborationModesConfig::default(),
        ));

        let start_args = InProcessClientStartArgs {
            arg0_paths: Arg0DispatchPaths::default(),
            config,
            cli_overrides: cli_overrides.clone(),
            loader_overrides: Default::default(),
            cloud_requirements,
            auth_manager: Some(auth_manager),
            thread_manager: Some(thread_manager),
            feedback: CodexFeedback::new(),
            config_warnings: vec![],
            session_source,
            enable_ctox_api_key_env: false,
            client_name: "ctox-direct".to_string(),
            client_version: env!("CTOX_BUILD_VERSION").to_string(),
            experimental_api: true,
            opt_out_notification_methods: vec![],
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
        };

        eprintln!("[ctox direct-session] starting InProcessAppServerClient...");
        let mut client = InProcessAppServerClient::start(start_args)
            .await
            .map_err(|err| anyhow::anyhow!("client start: {err}"))?;
        eprintln!("[ctox direct-session] client started");

        let mut seq = RequestIdSeq::new();
        let canonical_cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
        let persistent_thread_name = persistent_worker.then(|| persistent_worker_thread_name(root));
        let resumable_thread_id = if let Some(persistent_thread_name) =
            persistent_thread_name.as_deref()
        {
            match client
                .request_typed::<ThreadListResponse>(ClientRequest::ThreadList {
                    request_id: seq.next(),
                    params: ThreadListParams {
                        cursor: None,
                        limit: Some(20),
                        sort_key: Some(ThreadSortKey::UpdatedAt),
                        model_providers: None,
                        source_kinds: Some(vec![ThreadSourceKind::Exec]),
                        archived: Some(false),
                        cwd: None,
                        search_term: Some(persistent_thread_name.to_string()),
                    },
                })
                .await
            {
                Ok(response) => response
                    .data
                    .into_iter()
                    .find(|thread| {
                        !thread.ephemeral && thread.name.as_deref() == Some(persistent_thread_name)
                    })
                    .map(|thread| thread.id),
                Err(err) => {
                    eprintln!(
                        "[ctox direct-session] persistent thread lookup failed; starting fresh: {err}"
                    );
                    None
                }
            }
        } else {
            None
        };

        let thread_id = if let Some(thread_id) = resumable_thread_id {
            match client
                .request_typed::<ThreadResumeResponse>(ClientRequest::ThreadResume {
                    request_id: seq.next(),
                    params: ThreadResumeParams {
                        thread_id: thread_id.clone(),
                        history: None,
                        path: None,
                        model: Some(model.clone()),
                        model_provider: selected_provider_id.clone(),
                        service_tier: None,
                        cwd: Some(canonical_cwd.to_string_lossy().to_string()),
                        approval_policy: Some(AskForApproval::Never.into()),
                        approvals_reviewer: None,
                        sandbox: Some(ctox_app_server_protocol::SandboxMode::WorkspaceWrite),
                        config: None,
                        base_instructions: Some(base_instructions.to_string()),
                        developer_instructions: None,
                        personality: None,
                        persist_extended_history: true,
                    },
                })
                .await
            {
                Ok(response) => {
                    let resumed_id = response.thread.id;
                    eprintln!("[ctox direct-session] thread resumed: {resumed_id}");
                    resumed_id
                }
                Err(err) => {
                    eprintln!(
                        "[ctox direct-session] thread/resume failed for {thread_id}; starting fresh: {err}"
                    );
                    start_session_thread(
                        &mut client,
                        &mut seq,
                        &model,
                        selected_provider_id.as_deref(),
                        &canonical_cwd,
                        base_instructions,
                        disable_active_tools,
                        disable_mcp_servers,
                        persistent_worker,
                        persistent_thread_name.as_deref(),
                    )
                    .await?
                }
            }
        } else {
            start_session_thread(
                &mut client,
                &mut seq,
                &model,
                selected_provider_id.as_deref(),
                &canonical_cwd,
                base_instructions,
                disable_active_tools,
                disable_mcp_servers,
                persistent_worker,
                persistent_thread_name.as_deref(),
            )
            .await?
        };

        Ok((
            client,
            thread_id,
            cwd,
            seq,
            model,
            selected_provider_id,
            tracking_api_provider,
            reasoning_effort,
        ))
    }

    async fn run_turn_async(
        client: &mut InProcessAppServerClient,
        session_thread_id: &mut String,
        cwd: &Path,
        model: &str,
        model_provider: Option<&str>,
        api_provider: Option<&str>,
        reasoning_effort: Option<ReasoningEffort>,
        root: &Path,
        prompt: &str,
        developer_instructions: Option<&str>,
        base_instructions: &str,
        timeout: Option<Duration>,
        seq: &mut RequestIdSeq,
        policy: &mut CompactPolicy,
        ctx_log: &mut ContextLogger,
        disable_active_tools: bool,
        disable_mcp_servers: bool,
        read_only_sandbox: bool,
        persistent_worker: bool,
        exact_prompt_preflight: Option<ExactPromptTokenCount>,
    ) -> Result<String> {
        // Reuse the session's thread across turns. The previous fresh-thread-
        // per-turn workaround ("the thread may not accept new TurnStart
        // requests") has no backing mechanism in the current fork: turn_start
        // is load_thread + Op::UserInput with no completed-thread rejection,
        // and ephemeral threads stay registered in the in-memory manager.
        // Reuse makes a slice (main turn + continuity refreshes) one thread,
        // sends the base instructions once instead of four times, and is the
        // first building block of the long-lived worker session. A defensive
        // fallback below still rotates the thread if TurnStart reports it
        // missing.
        let thread_id = session_thread_id.clone();

        // The old preflight only counted base instructions plus the new
        // prompt. That was sufficient while every service slice used a fresh
        // thread, but it undercounts a reused thread by its complete active
        // history. TokenCount events give us the last real model input size;
        // conservatively add the incoming prompt and compact the live thread
        // before starting the next turn when that projected request crosses
        // the same safe-input boundary as the exact tokenizer path.
        let incoming_prompt_text = match developer_instructions {
            Some(instructions) => format!("{instructions}\n\n{prompt}"),
            None => prompt.to_string(),
        };
        let incoming_prompt_tokens =
            i64::try_from(crate::lcm::estimate_tokens(&incoming_prompt_text)).unwrap_or(i64::MAX);
        let projected_history_tokens = policy
            .last_call_input_tokens
            .saturating_add(incoming_prompt_tokens);
        let history_safe_budget = exact_prompt_safe_input_budget(policy.context_window);
        if policy.last_call_input_tokens > 0 && projected_history_tokens > history_safe_budget {
            ctx_log.log(
                "history_prompt_preflight",
                &format!(
                    "\"last_input_tokens\":{},\"incoming_prompt_tokens\":{},\"projected_tokens\":{},\"safe_budget\":{},\"context_limit\":{}",
                    policy.last_call_input_tokens,
                    incoming_prompt_tokens,
                    projected_history_tokens,
                    history_safe_budget,
                    policy.context_window
                ),
            );
            client
                .request_typed::<ThreadCompactStartResponse>(ClientRequest::ThreadCompactStart {
                    request_id: seq.next(),
                    params: ThreadCompactStartParams {
                        thread_id: thread_id.clone(),
                    },
                })
                .await
                .map_err(|err| {
                    anyhow::anyhow!("history-aware pre-turn compaction failed: {err}")
                })?;
            policy.note_compacted();
            // A successful compact replaced the active history. The next
            // TokenCount event supplies the authoritative post-compact size;
            // do not reuse the stale pre-compact observation meanwhile.
            policy.last_call_input_tokens = 0;
            ctx_log.log("history_prompt_preflight_compact_ok", "\"compacted\":true");
        }

        let preflight_text = format!("{base_instructions}\n\n{incoming_prompt_text}");
        if let Some(count) = exact_prompt_token_count_with_precomputed(
            root,
            &preflight_text,
            exact_prompt_preflight.as_ref(),
        )? {
            let safe_budget = exact_prompt_safe_input_budget(count.context_limit);
            ctx_log.log(
                "exact_prompt_preflight",
                &format!(
                    "\"tokens\":{},\"safe_budget\":{},\"context_limit\":{},\"source\":\"{}\"",
                    count.tokens,
                    safe_budget,
                    count.context_limit,
                    escape_json_fragment(&count.source)
                ),
            );
            if count.tokens > safe_budget {
                anyhow::bail!(
                    "context_preflight_exact_overflow: exact prompt tokens {} exceed safe input budget {} for context window {} via {}",
                    count.tokens,
                    safe_budget,
                    count.context_limit,
                    count.source
                );
            }
        }

        // TurnStart on the session thread, with a one-shot rotation fallback
        // if the thread genuinely went away (defensive; see comment above).
        let turn_start_params = |thread_id: &str| TurnStartParams {
            thread_id: thread_id.to_string(),
            input: vec![UserInput::Text {
                text: prompt.to_string(),
                text_elements: Vec::new(),
            }
            .into()],
            developer_instructions: developer_instructions.map(str::to_string),
            cwd: Some(cwd.to_path_buf()),
            approval_policy: Some(AskForApproval::Never.into()),
            approvals_reviewer: None,
            sandbox_policy: Some(
                SandboxPolicy::WorkspaceWrite {
                    writable_roots: Vec::new(),
                    read_only_access: ReadOnlyAccess::FullAccess,
                    network_access: true,
                    // Reviewer scratch lives under the temp directory, but cwd
                    // remains explicitly writable. Do not grant the rest of
                    // the host temp tree as an additional writable surface.
                    exclude_tmpdir_env_var: read_only_sandbox,
                    exclude_slash_tmp: read_only_sandbox,
                }
                .into(),
            ),
            model: None,
            service_tier: None,
            effort: reasoning_effort,
            summary: None,
            personality: None,
            output_schema: None,
            collaboration_mode: None,
        };
        let turn_start_result: Result<TurnStartResponse> = match tokio::time::timeout(
            Duration::from_secs(DIRECT_SESSION_TURN_START_TIMEOUT_SECS),
            client.request_typed(ClientRequest::TurnStart {
                request_id: seq.next(),
                params: turn_start_params(&thread_id),
            }),
        )
        .await
        {
            Ok(result) => result.map_err(|err| anyhow::anyhow!("{err}")),
            Err(_) => Err(anyhow::anyhow!(
                "turn/start timed out after {DIRECT_SESSION_TURN_START_TIMEOUT_SECS}s"
            )),
        };
        let turn_resp: TurnStartResponse = match turn_start_result {
            Ok(resp) => resp,
            Err(err) => {
                eprintln!(
                    "[ctox direct-session] turn/start on session thread {thread_id} failed ({err}); rotating thread"
                );
                let rotated_thread_id = start_session_thread(
                    client,
                    seq,
                    model,
                    model_provider,
                    cwd,
                    base_instructions,
                    disable_active_tools,
                    disable_mcp_servers,
                    persistent_worker,
                    persistent_worker
                        .then(|| persistent_worker_thread_name(root))
                        .as_deref(),
                )
                .await
                .map_err(|err| anyhow::anyhow!("thread/start (rotation): {err}"))?;
                eprintln!("[ctox direct-session] rotated session thread: {rotated_thread_id}");
                *session_thread_id = rotated_thread_id.clone();
                match tokio::time::timeout(
                    Duration::from_secs(DIRECT_SESSION_TURN_START_TIMEOUT_SECS),
                    client.request_typed(ClientRequest::TurnStart {
                        request_id: seq.next(),
                        params: turn_start_params(&rotated_thread_id),
                    }),
                )
                .await
                {
                    Ok(result) => result.map_err(|err| anyhow::anyhow!("turn/start: {err}"))?,
                    Err(_) => anyhow::bail!(
                        "turn/start on rotated thread timed out after {DIRECT_SESSION_TURN_START_TIMEOUT_SECS}s"
                    ),
                }
            }
        };
        let thread_id = session_thread_id.clone();
        let turn_id = turn_resp.turn.id;

        // Event loop
        let mut final_message: Option<String> = None;
        let turn_started_at = Instant::now();
        let mut last_usage_event_at = turn_started_at;
        let mut last_recorded_cumulative_usage: Option<ApiTokenUsage> = None;
        let mut pending_api_cost_records = Vec::new();
        let deadline = timeout.map(|d| tokio::time::Instant::now() + d);

        loop {
            let event = match deadline {
                Some(d) => tokio::select! {
                    ev = client.next_event() => ev,
                    _ = tokio::time::sleep_until(d) => {
                        // Interrupt the server-side turn before bailing, and
                        // KEEP DRAINING events while the interrupt request is
                        // in flight. A paused consumer is exactly what lets
                        // the event pipeline back up until control requests
                        // can no longer be processed; awaiting the interrupt
                        // without draining reintroduces that wedge (ctox#21).
                        let interrupt_req = ClientRequest::TurnInterrupt {
                            request_id: seq.next(),
                            params: TurnInterruptParams {
                                thread_id: thread_id.to_string(),
                                turn_id: turn_id.to_string(),
                            },
                        };
                        let request_handle = client.request_handle();
                        let interrupt = request_handle
                            .request_typed::<TurnInterruptResponse>(interrupt_req);
                        tokio::pin!(interrupt);
                        let interrupt_deadline = tokio::time::Instant::now()
                            + Duration::from_secs(DIRECT_SESSION_INTERRUPT_TIMEOUT_SECS);
                        loop {
                            tokio::select! {
                                ev = client.next_event() => {
                                    if ev.is_none() {
                                        break;
                                    }
                                }
                                result = &mut interrupt => {
                                    if let Err(err) = result {
                                        eprintln!(
                                            "[ctox direct-session] turn/interrupt failed after timeout: {err}"
                                        );
                                    }
                                    break;
                                }
                                _ = tokio::time::sleep_until(interrupt_deadline) => {
                                    eprintln!(
                                        "[ctox direct-session] turn/interrupt not acknowledged within {DIRECT_SESSION_INTERRUPT_TIMEOUT_SECS}s"
                                    );
                                    break;
                                }
                            }
                        }
                        anyhow::bail!("direct session timeout after {:?}", timeout.unwrap());
                    }
                },
                None => client.next_event().await,
            };
            let Some(event) = event else { break };
            match event {
                InProcessServerEvent::ServerRequest(_) => {}
                InProcessServerEvent::ServerNotification(notification) => {
                    if let ServerNotification::ContextCompacted(compacted) = notification {
                        if compacted.turn_id == turn_id {
                            eprintln!(
                                "[ctox direct-session] compact completed for turn {}",
                                compacted.turn_id
                            );
                            ctx_log.log(
                                "compact_completed",
                                &format!("\"turn_id\":\"{}\"", compacted.turn_id),
                            );
                        }
                    }
                }
                InProcessServerEvent::LegacyNotification(notif) => {
                    if let Some(msg) = try_extract_event_msg(&notif) {
                        ctx_log.observe(&msg);
                        if let (Some(provider), EventMsg::TokenCount(tc)) = (api_provider, &msg) {
                            if let Some(info) = tc.info.as_ref() {
                                let usage = &info.last_token_usage;
                                let cumulative_usage = ApiTokenUsage {
                                    input_tokens: info.total_token_usage.input_tokens,
                                    cached_input_tokens: info.total_token_usage.cached_input_tokens,
                                    output_tokens: info.total_token_usage.output_tokens,
                                    reasoning_output_tokens: info
                                        .total_token_usage
                                        .reasoning_output_tokens,
                                    total_tokens: info.total_token_usage.total_tokens,
                                };
                                if last_recorded_cumulative_usage == Some(cumulative_usage) {
                                    continue;
                                }
                                last_recorded_cumulative_usage = Some(cumulative_usage);
                                let now = Instant::now();
                                let elapsed_ms =
                                    duration_millis_i64(now.duration_since(last_usage_event_at));
                                let turn_elapsed_ms =
                                    duration_millis_i64(now.duration_since(turn_started_at));
                                last_usage_event_at = now;
                                pending_api_cost_records.push(api_costs::ApiCostUsageRecord {
                                    provider: provider.to_string(),
                                    model: model.to_string(),
                                    turn_id: Some(turn_id.to_string()),
                                    usage: ApiTokenUsage {
                                        input_tokens: usage.input_tokens,
                                        cached_input_tokens: usage.cached_input_tokens,
                                        output_tokens: usage.output_tokens,
                                        reasoning_output_tokens: usage.reasoning_output_tokens,
                                        total_tokens: usage.total_tokens,
                                    },
                                    telemetry: Some(ApiCallTelemetry {
                                        elapsed_ms: Some(elapsed_ms),
                                        turn_elapsed_ms: Some(turn_elapsed_ms),
                                        output_tokens_per_second: tokens_per_second(
                                            usage.output_tokens,
                                            elapsed_ms,
                                        ),
                                        total_tokens_per_second: tokens_per_second(
                                            usage.total_tokens,
                                            elapsed_ms,
                                        ),
                                    }),
                                });
                            }
                        }

                        if let CompactDecision::Compact { reason } = policy.evaluate(&msg) {
                            eprintln!(
                                "[ctox direct-session] compact mode={:?} reason={}",
                                policy.mode,
                                reason.log_summary()
                            );
                            // ForcedFollowup is deprecated: its clean-break
                            // path wrote a signal file nothing consumed, so
                            // the promised follow-up slice never happened.
                            // Both modes now run the in-thread compaction.
                            {
                                {
                                    ctx_log.log_compact_decision("decision", &reason, policy);
                                    let compact_req = ClientRequest::ThreadCompactStart {
                                        request_id: seq.next(),
                                        params: ThreadCompactStartParams {
                                            thread_id: thread_id.to_string(),
                                        },
                                    };
                                    let compact_timeout =
                                        direct_session_midtask_compact_timeout(deadline);
                                    match tokio::time::timeout(
                                        compact_timeout,
                                        client.request_typed::<ThreadCompactStartResponse>(
                                            compact_req,
                                        ),
                                    )
                                    .await
                                    {
                                        Ok(Ok(_)) => {
                                            ctx_log.log_compact_decision(
                                                "compact_ok",
                                                &reason,
                                                policy,
                                            );
                                            policy.note_compacted();
                                        }
                                        Ok(Err(err)) => {
                                            eprintln!(
                                                "[ctox direct-session] compact failed: {err}"
                                            );
                                            ctx_log.log_compact_decision(
                                                &format!("compact_fail:{err}"),
                                                &reason,
                                                policy,
                                            );
                                            // Interrupt the still-active turn before bailing.
                                            // Bailing without an interrupt leaves the turn
                                            // running on the durable thread, which the next
                                            // slice resumes by name (ctox#21).
                                            let interrupt_req = ClientRequest::TurnInterrupt {
                                                request_id: seq.next(),
                                                params: TurnInterruptParams {
                                                    thread_id: thread_id.to_string(),
                                                    turn_id: turn_id.to_string(),
                                                },
                                            };
                                            let _ = tokio::time::timeout(
                                                Duration::from_secs(
                                                    DIRECT_SESSION_INTERRUPT_TIMEOUT_SECS,
                                                ),
                                                client.request_typed::<TurnInterruptResponse>(
                                                    interrupt_req,
                                                ),
                                            )
                                            .await;
                                            anyhow::bail!("mid-task compaction failed: {err}");
                                        }
                                        Err(_) => {
                                            eprintln!(
                                                "[ctox direct-session] compact did not complete within {:?}; interrupting turn",
                                                compact_timeout
                                            );
                                            ctx_log.log_compact_decision(
                                                "compact_timeout",
                                                &reason,
                                                policy,
                                            );
                                            // Mark the attempt so the hot CompactPolicy cannot
                                            // immediately re-fire next turn: the timeout bail
                                            // string matches no runtime-blocker cooldown (unlike
                                            // the compact-failed arm), so without this the same
                                            // doomed compaction retries in a tight loop.
                                            policy.note_compacted();
                                            let interrupt_req = ClientRequest::TurnInterrupt {
                                                request_id: seq.next(),
                                                params: TurnInterruptParams {
                                                    thread_id: thread_id.to_string(),
                                                    turn_id: turn_id.to_string(),
                                                },
                                            };
                                            let _ = tokio::time::timeout(
                                                Duration::from_secs(
                                                    DIRECT_SESSION_INTERRUPT_TIMEOUT_SECS,
                                                ),
                                                client.request_typed::<TurnInterruptResponse>(
                                                    interrupt_req,
                                                ),
                                            )
                                            .await;
                                            anyhow::bail!(
                                                "mid-task compaction timeout after {:?}",
                                                compact_timeout
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        match msg {
                            EventMsg::AgentMessage(am) => {
                                final_message = Some(am.message.clone());
                            }
                            EventMsg::TurnComplete(tc) if tc.turn_id == turn_id => {
                                // The completion event's own last message is
                                // authoritative: `AgentMessage` events carry
                                // no turn id, so `final_message` could hold a
                                // reply from a different turn on this thread.
                                if let Some(last) = tc
                                    .last_agent_message
                                    .as_ref()
                                    .filter(|last| !last.trim().is_empty())
                                {
                                    final_message = Some(last.clone());
                                }
                                break;
                            }
                            EventMsg::TurnComplete(tc) => {
                                // A completion for a different turn while we
                                // wait for ours. With one consumer per session
                                // this points at a steered/merged or leftover
                                // turn on the reused thread — log it loudly
                                // instead of silently discarding it, so a
                                // wedged wait is diagnosable from the service
                                // log (ctox#21).
                                eprintln!(
                                    "[ctox direct-session] ignoring completion for foreign turn {} while waiting for {}",
                                    tc.turn_id, turn_id
                                );
                            }
                            EventMsg::Error(ref err) => {
                                let msg_str = format!("{:?}", err);
                                let structured_compaction_parse_error = msg_str
                                    .contains("failed to parse structured compaction response")
                                    || (msg_str.contains("compaction")
                                        && msg_str.contains("expected value at line"));
                                if structured_compaction_parse_error {
                                    eprintln!(
                                        "[ctox direct-session] compaction error (fatal): {}",
                                        msg_str
                                    );
                                    ctx_log.log(
                                        "compaction_error_fatal",
                                        &format!(
                                            "\"message\":\"{}\"",
                                            msg_str
                                                .replace('"', "'")
                                                .chars()
                                                .take(200)
                                                .collect::<String>()
                                        ),
                                    );
                                    anyhow::bail!("mid-task compaction failed: {msg_str}");
                                } else if msg_str.contains("compaction")
                                    || msg_str.contains("revisedTitle")
                                {
                                    // Title-only compaction side effects are non-fatal. The
                                    // structured compaction itself must not be ignored: if it
                                    // fails, continuing can overflow the local backend context.
                                    eprintln!(
                                        "[ctox direct-session] compaction error (non-fatal): {}",
                                        msg_str
                                    );
                                    ctx_log.log(
                                        "compaction_error",
                                        &format!(
                                            "\"message\":\"{}\"",
                                            msg_str
                                                .replace('"', "'")
                                                .chars()
                                                .take(200)
                                                .collect::<String>()
                                        ),
                                    );
                                } else {
                                    anyhow::bail!("direct session error: {}", msg_str);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                InProcessServerEvent::Lagged { skipped } => {
                    eprintln!("[ctox direct-session] lagged: dropped {skipped} events");
                }
            }
        }

        if !pending_api_cost_records.is_empty() {
            if let Err(err) =
                api_costs::record_api_model_usage_batch(root, &pending_api_cost_records)
            {
                eprintln!("[ctox direct-session] cost tracking failed: {err}");
            }
        }

        ctx_log.log(
            "turn_end",
            &format!(
                "\"reply_chars\":{}",
                final_message.as_ref().map(|m| m.len()).unwrap_or(0)
            ),
        );

        final_message.ok_or_else(|| anyhow::anyhow!("turn completed without assistant message"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_worker_thread_name_is_stable_and_root_scoped() {
        let first = persistent_worker_thread_name(Path::new("/tmp/ctox-a"));
        let same = persistent_worker_thread_name(Path::new("/tmp/ctox-a"));
        let other = persistent_worker_thread_name(Path::new("/tmp/ctox-b"));

        assert_eq!(first, same);
        assert_ne!(first, other);
        assert!(first.starts_with(CTOX_PERSISTENT_WORKER_THREAD_NAME));
    }

    #[test]
    fn openai_subscription_auth_only_applies_to_openai_provider() {
        let mut settings = BTreeMap::new();
        settings.insert(
            OPENAI_AUTH_MODE_KEY.to_string(),
            OPENAI_AUTH_MODE_CHATGPT_SUBSCRIPTION.to_string(),
        );

        assert!(use_openai_chatgpt_subscription_auth(
            &settings,
            Some("openai")
        ));
        assert!(use_openai_chatgpt_subscription_auth(&settings, None));
        assert!(!use_openai_chatgpt_subscription_auth(
            &settings,
            Some("anthropic")
        ));
    }

    #[test]
    fn openai_subscription_auth_accepts_compatibility_aliases() {
        for value in [
            "subscription",
            "codex_subscription",
            "chatgpt",
            "chatgpt_subscription",
            " CHATGPT_SUBSCRIPTION ",
        ] {
            let mut settings = BTreeMap::new();
            settings.insert(OPENAI_AUTH_MODE_KEY.to_string(), value.to_string());
            assert!(
                openai_chatgpt_subscription_auth_enabled(&settings),
                "{value}"
            );
        }
    }

    #[test]
    fn selected_model_prefers_resolved_runtime_over_stale_settings() {
        let mut settings = BTreeMap::new();
        settings.insert("CTOX_CHAT_MODEL".to_string(), "gpt-5.5".to_string());
        settings.insert("CODEX_MODEL".to_string(), "gpt-5.4".to_string());

        assert_eq!(
            direct_session_selected_model(&settings, Some("MiniMax-M3".to_string())),
            "MiniMax-M3"
        );
    }

    #[test]
    fn selected_model_uses_settings_without_resolved_runtime() {
        let mut settings = BTreeMap::new();
        settings.insert("CTOX_CHAT_MODEL".to_string(), "gpt-5.4".to_string());

        assert_eq!(direct_session_selected_model(&settings, None), "gpt-5.4");
    }

    fn compose_test_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ctox-direct-session-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn worker_base_instructions_include_system_prompt_and_durable_outcome_contract() {
        let root = compose_test_root("worker-base");
        let instructions =
            compose_base_instructions(&root, &BTreeMap::new(), None).expect("worker instructions");
        // The long CTOX system prompt is the stability layer and must be present.
        assert!(instructions.contains("You are CTOX, the personal CTO agent"));
        assert!(instructions.contains("Secret handling policy:"));
        // The direct-session execution contract stays appended.
        assert!(instructions.contains("required durable outcome exists"));
        assert!(instructions.contains("do not run reviewed-send before review feedback"));
        assert!(instructions.contains("Do not create review rows or approval digests manually"));
        assert!(instructions.contains("accepted outbound row"));
        assert!(instructions.contains("Do not create review-driven internal work"));
    }

    #[test]
    fn override_base_instructions_stand_alone_without_worker_prompt() {
        let root = compose_test_root("override-base");
        let instructions = compose_base_instructions(
            &root,
            &BTreeMap::new(),
            Some("Act as the external reviewer."),
        )
        .expect("override instructions");
        assert!(instructions.contains("Act as the external reviewer."));
        // Review/repair sessions own their full system prompt; the worker
        // contract must not leak in and instruct worker actions.
        assert!(!instructions.contains("required durable outcome exists"));
        assert!(!instructions.contains("You are CTOX, the personal CTO agent"));
    }

    #[test]
    fn reviewer_scratch_workspaces_are_unique_and_removable() {
        let first = create_reviewer_scratch_workspace().expect("create first scratch");
        let second = create_reviewer_scratch_workspace().expect("create second scratch");
        assert_ne!(first, second);
        assert!(first.is_dir());
        assert!(second.is_dir());
        std::fs::remove_dir_all(&first).expect("remove first scratch");
        std::fs::remove_dir_all(&second).expect("remove second scratch");
        assert!(!first.exists());
        assert!(!second.exists());
    }

    #[test]
    fn exact_prompt_budget_keeps_generation_headroom() {
        assert_eq!(exact_prompt_safe_input_budget(131_072), 98_304);
        assert_eq!(exact_prompt_safe_input_budget(1), 1);
        assert_eq!(exact_prompt_safe_input_budget(0), 1);
    }

    #[test]
    fn exact_prompt_preflight_reuses_precomputed_count() {
        let precomputed = ExactPromptTokenCount {
            tokens: 123,
            context_limit: 456,
            source: "test-preflight".to_string(),
        };
        let count = exact_prompt_token_count_with_precomputed(
            std::path::Path::new("/definitely/not/a/ctox/runtime/root"),
            "the prompt text should not be tokenized again",
            Some(&precomputed),
        )
        .expect("precomputed exact prompt count should be accepted")
        .expect("precomputed exact prompt count should be returned");

        assert_eq!(count, precomputed);
    }

    #[test]
    fn tokenizer_response_parser_accepts_llama_tokens_array() {
        assert_eq!(
            parse_tokenize_count(r#"{"tokens":[1,2,3],"pieces":[]}"#).unwrap(),
            3
        );
    }

    #[test]
    fn tokenizer_response_parser_accepts_count_fields() {
        assert_eq!(parse_tokenize_count(r#"{"n_tokens":42}"#).unwrap(), 42);
        assert_eq!(parse_tokenize_count(r#"{"token_count":17}"#).unwrap(), 17);
        assert_eq!(parse_tokenize_count(r#"{"count":9}"#).unwrap(), 9);
    }

    #[test]
    fn tokenizer_base_url_does_not_fallback_to_stale_port_for_ipc_runtime() {
        let binding = runtime_kernel::ResolvedRuntimeBinding {
            workload: runtime_kernel::InferenceWorkloadRole::PrimaryGeneration,
            display_model: "Qwen/Qwen3.6-35B-A3B".to_string(),
            request_model: "Qwen/Qwen3.6-35B-A3B".to_string(),
            port: 1234,
            base_url: String::new(),
            transport_endpoint: Some("/tmp/primary_generation.sock".to_string()),
            transport: crate::inference::local_transport::LocalTransport::UnixSocket {
                path: PathBuf::from("/tmp/primary_generation.sock"),
            },
            health_path: "/health",
            launcher_kind: runtime_kernel::RuntimeLauncherKind::Engine,
            compute_target: None,
            visible_devices: None,
        };

        let err = tokenizer_base_url(&binding).unwrap_err().to_string();
        assert!(err.contains("without HTTP tokenizer metadata"));
        assert!(!err.contains("127.0.0.1:1234"));
    }

    #[test]
    fn performance_preset_sets_low_reasoning_for_gpt_54_mini() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            "Performance".to_string(),
        );

        assert_eq!(
            direct_session_reasoning_effort(&settings, "gpt-5.4-mini", None),
            Some(ReasoningEffort::Low)
        );
    }

    #[test]
    fn explicit_reasoning_effort_overrides_performance_default() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_CHAT_LOCAL_PRESET".to_string(),
            "Performance".to_string(),
        );
        settings.insert(
            "CTOX_CHAT_REASONING_EFFORT".to_string(),
            "minimal".to_string(),
        );

        assert_eq!(
            direct_session_reasoning_effort(&settings, "gpt-5.4-mini", None),
            Some(ReasoningEffort::Minimal)
        );
    }

    #[test]
    fn runtime_performance_preset_sets_low_reasoning_for_provider_prefixed_model() {
        let settings = BTreeMap::new();

        assert_eq!(
            direct_session_reasoning_effort(&settings, "openai/gpt-5.4-mini", Some("performance")),
            Some(ReasoningEffort::Low)
        );
    }

    #[test]
    fn quality_preset_does_not_force_low_reasoning() {
        let mut settings = BTreeMap::new();
        settings.insert("CTOX_CHAT_LOCAL_PRESET".to_string(), "Quality".to_string());

        assert_eq!(
            direct_session_reasoning_effort(&settings, "gpt-5.4-mini", None),
            None
        );
    }

    #[test]
    fn control_request_timeout_defaults_without_deadline() {
        assert_eq!(
            direct_session_control_request_timeout(None),
            Duration::from_secs(DIRECT_SESSION_CONTROL_REQUEST_TIMEOUT_SECS)
        );
    }

    #[test]
    fn control_request_timeout_is_capped_by_default() {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);

        assert_eq!(
            direct_session_control_request_timeout(Some(deadline)),
            Duration::from_secs(DIRECT_SESSION_CONTROL_REQUEST_TIMEOUT_SECS)
        );
    }

    #[test]
    fn control_request_timeout_honors_near_deadline() {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(50);
        let timeout = direct_session_control_request_timeout(Some(deadline));

        assert!(timeout > Duration::from_millis(0));
        assert!(timeout <= Duration::from_millis(50));
    }

    #[test]
    fn midtask_compact_timeout_uses_larger_budget() {
        assert_eq!(
            direct_session_midtask_compact_timeout(None),
            Duration::from_secs(DIRECT_SESSION_MIDTASK_COMPACT_TIMEOUT_SECS)
        );
        assert!(
            direct_session_midtask_compact_timeout(None)
                > direct_session_control_request_timeout(None)
        );
    }

    #[test]
    fn direct_session_ignores_stream_delta_events_before_deserialize() {
        let before = DIRECT_SESSION_EVENT_DESERIALIZE_CALLS.load(AtomicOrdering::Relaxed);
        let notif = JSONRPCNotification {
            method: "codex/event/agent_message_delta".to_string(),
            params: Some(serde_json::json!({
                "msg": {
                    "delta": "token"
                }
            })),
        };

        assert!(try_extract_event_msg(&notif).is_none());
        assert_eq!(
            DIRECT_SESSION_EVENT_DESERIALIZE_CALLS.load(AtomicOrdering::Relaxed),
            before,
            "ignored stream deltas must not clone into serde deserialization"
        );
    }

    #[test]
    fn direct_session_extracts_agent_message_events() {
        let before = DIRECT_SESSION_EVENT_DESERIALIZE_CALLS.load(AtomicOrdering::Relaxed);
        let notif = JSONRPCNotification {
            method: "codex/event/agent_message".to_string(),
            params: Some(serde_json::json!({
                "msg": {
                    "message": "done"
                }
            })),
        };

        let msg = try_extract_event_msg(&notif).expect("agent message should parse");
        assert!(matches!(msg, EventMsg::AgentMessage(ref event) if event.message == "done"));
        assert_eq!(
            DIRECT_SESSION_EVENT_DESERIALIZE_CALLS.load(AtomicOrdering::Relaxed),
            before + 1
        );
    }
}

impl Drop for PersistentSession {
    fn drop(&mut self) {
        self.shutdown_inner("dropping");
    }
}

impl PersistentSession {
    fn shutdown_inner(&mut self, action: &str) {
        if let Some(runtime) = self.runtime.take() {
            if let Some(client) = self.client.take() {
                let tid = self.thread_id.clone();
                eprintln!("[ctox direct-session] {action} persistent session thread_id={tid}");
                // Try a bounded graceful shutdown first. `abort_now` skips
                // the processor teardown (`clear_all_thread_listeners`,
                // `shutdown_threads`), which leaves the durable thread's
                // rollout with a dangling active turn that the next session
                // resumes by name (ctox#21). If graceful shutdown exceeds
                // its budget the runtime kill below still bounds teardown.
                let graceful = runtime.block_on(async {
                    tokio::time::timeout(Duration::from_secs(8), client.shutdown()).await
                });
                match graceful {
                    Ok(Ok(())) => eprintln!(
                        "[ctox direct-session] persistent session shut down thread_id={tid}"
                    ),
                    Ok(Err(err)) => eprintln!(
                        "[ctox direct-session] persistent session shutdown error thread_id={tid}: {err}"
                    ),
                    Err(_) => eprintln!(
                        "[ctox direct-session] persistent session shutdown timed out thread_id={tid}; forcing runtime teardown"
                    ),
                }
            }
            runtime.shutdown_timeout(Duration::from_secs(2));
        }
        if let Some(scratch) = self.reviewer_scratch_workspace.take() {
            if let Err(err) = std::fs::remove_dir_all(&scratch) {
                eprintln!(
                    "[ctox direct-session] failed to remove reviewer scratch {}: {err}",
                    scratch.display()
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

struct RequestIdSeq {
    next: i64,
}
impl RequestIdSeq {
    fn new() -> Self {
        Self { next: 1 }
    }
    fn next(&mut self) -> RequestId {
        let id = self.next;
        self.next += 1;
        RequestId::Integer(id)
    }
}

async fn start_session_thread(
    client: &mut InProcessAppServerClient,
    seq: &mut RequestIdSeq,
    model: &str,
    model_provider: Option<&str>,
    cwd: &Path,
    base_instructions: &str,
    disable_active_tools: bool,
    disable_mcp_servers: bool,
    persistent_worker: bool,
    persistent_thread_name: Option<&str>,
) -> Result<String> {
    let response: ThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: seq.next(),
            params: ThreadStartParams {
                model: Some(model.to_string()),
                model_provider: model_provider.map(str::to_string),
                cwd: Some(cwd.to_string_lossy().to_string()),
                approval_policy: Some(AskForApproval::Never.into()),
                sandbox: Some(ctox_app_server_protocol::SandboxMode::WorkspaceWrite),
                base_instructions: Some(base_instructions.to_string()),
                dynamic_tools: disable_active_tools.then(Vec::new),
                disable_mcp_servers: Some(disable_mcp_servers),
                ephemeral: Some(!persistent_worker),
                persist_extended_history: persistent_worker,
                ..ThreadStartParams::default()
            },
        })
        .await
        .map_err(|err| anyhow::anyhow!("thread/start: {err}"))?;
    let thread_id = response.thread.id;
    if let Some(persistent_thread_name) = persistent_thread_name {
        client
            .request_typed::<ThreadSetNameResponse>(ClientRequest::ThreadSetName {
                request_id: seq.next(),
                params: ThreadSetNameParams {
                    thread_id: thread_id.clone(),
                    name: persistent_thread_name.to_string(),
                },
            })
            .await
            .map_err(|err| anyhow::anyhow!("thread/name/set: {err}"))?;
    }
    eprintln!("[ctox direct-session] thread started: {thread_id}");
    Ok(thread_id)
}

fn try_extract_event_msg(notif: &JSONRPCNotification) -> Option<EventMsg> {
    let method_event_type = notif.method.strip_prefix("codex/event/");
    let method = method_event_type.unwrap_or(&notif.method);
    let value = notif.params.as_ref()?;
    let obj = value.as_object()?;
    let params_event_type = direct_session_params_event_type(obj);
    let event_type = method_event_type.or(params_event_type).unwrap_or(method);
    if direct_session_ignored_event_type(event_type) {
        return None;
    }
    let mut payload = if let Some(serde_json::Value::Object(msg_obj)) = obj.get("msg") {
        serde_json::Value::Object(msg_obj.clone())
    } else {
        let mut obj = obj.clone();
        obj.remove("conversationId");
        serde_json::Value::Object(obj)
    };
    if let serde_json::Value::Object(ref mut map) = payload {
        map.insert(
            "type".to_string(),
            serde_json::Value::String(event_type.to_string()),
        );
    }
    #[cfg(test)]
    DIRECT_SESSION_EVENT_DESERIALIZE_CALLS.fetch_add(1, AtomicOrdering::Relaxed);
    serde_json::from_value(payload).ok()
}

fn direct_session_params_event_type(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Option<&str> {
    obj.get("msg")
        .and_then(serde_json::Value::as_object)
        .and_then(|msg| msg.get("type"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| obj.get("type").and_then(serde_json::Value::as_str))
}

fn direct_session_ignored_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "agent_message_delta"
            | "agent_reasoning_delta"
            | "agent_reasoning_raw_content_delta"
            | "exec_command_output_delta"
            | "terminal_interaction"
            | "realtime_conversation_realtime"
    )
}

fn duration_millis_i64(duration: Duration) -> i64 {
    duration.as_millis().min(i64::MAX as u128) as i64
}

fn tokens_per_second(tokens: i64, elapsed_ms: i64) -> Option<f64> {
    if tokens <= 0 || elapsed_ms <= 0 {
        return None;
    }
    Some(tokens as f64 / (elapsed_ms as f64 / 1000.0))
}

// ---------------------------------------------------------------------------
// Context forensics logger
// ---------------------------------------------------------------------------

struct ContextLogger {
    file: Option<std::fs::File>,
    session_start: Instant,
    items_this_turn: u32,
    last_total_tokens: i64,
    last_context_window: i64,
    session_kind: &'static str,
}

impl ContextLogger {
    fn open(root: &Path) -> Self {
        let path = root.join("runtime/context-log.jsonl");
        let _ = std::fs::create_dir_all(root.join("runtime"));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        Self {
            file,
            session_start: Instant::now(),
            items_this_turn: 0,
            last_total_tokens: 0,
            last_context_window: 0,
            session_kind: "mission",
        }
    }

    fn with_session_kind(mut self, session_kind: &'static str) -> Self {
        self.session_kind = session_kind;
        self
    }

    fn log(&mut self, event: &str, extra: &str) {
        let Some(f) = self.file.as_mut() else { return };
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let elapsed = self.session_start.elapsed().as_secs();
        let _ = writeln!(
            f,
            "{{\"ts\":{ts},\"elapsed_s\":{elapsed},\"event\":\"{event}\",\
             \"total_tokens\":{},\"context_window\":{},\"items_this_turn\":{},\"session_kind\":\"{}\",{extra}}}",
            self.last_total_tokens,
            self.last_context_window,
            self.items_this_turn,
            self.session_kind
        );
    }

    fn observe(&mut self, msg: &EventMsg) {
        match msg {
            EventMsg::TurnStarted(ts) => {
                self.items_this_turn = 0;
                if let Some(w) = ts.model_context_window {
                    self.last_context_window = w;
                }
                self.log(
                    "turn_started",
                    &format!(
                        "\"turn_id\":\"{}\",\"model_context_window\":{}",
                        ts.turn_id,
                        ts.model_context_window.unwrap_or(-1)
                    ),
                );
            }
            EventMsg::TokenCount(tc) => {
                if let Some(info) = tc.info.as_ref() {
                    self.last_total_tokens = info.total_token_usage.total_tokens;
                    let last_input = info.last_token_usage.input_tokens;
                    let last_output = info.last_token_usage.output_tokens;
                    if let Some(w) = info.model_context_window {
                        self.last_context_window = w;
                    }
                    let fill_pct = if self.last_context_window > 0 {
                        (last_input as f64) / (self.last_context_window as f64)
                    } else {
                        0.0
                    };
                    self.log(
                        "token_count",
                        &format!(
                            "\"call_input\":{last_input},\"call_output\":{last_output},\
                         \"cum_output\":{},\"cum_input\":{},\
                         \"fill_pct\":{fill_pct:.3},\"window\":{}",
                            info.total_token_usage.output_tokens,
                            info.total_token_usage.input_tokens,
                            self.last_context_window,
                        ),
                    );
                }
            }
            EventMsg::TurnComplete(tc) => {
                self.log(
                    "turn_complete",
                    &format!(
                        "\"turn_id\":\"{}\",\"has_last_message\":{}",
                        tc.turn_id,
                        tc.last_agent_message.is_some()
                    ),
                );
            }
            EventMsg::AgentMessage(am) => {
                self.log("agent_message", &format!("\"chars\":{}", am.message.len()));
            }
            EventMsg::ExecCommandBegin(ev) => {
                let cmd = json_string(&ev.command.join(" "));
                let cwd = json_string(ev.cwd.to_string_lossy().as_ref());
                self.log(
                    "tool_call_begin",
                    &format!(
                        "\"tool_type\":\"exec_command\",\"call_id\":\"{}\",\"command\":{},\"cwd\":{}",
                        ev.call_id, cmd, cwd
                    ),
                );
            }
            EventMsg::ExecCommandEnd(ev) => {
                let cmd = json_string(&ev.command.join(" "));
                self.log(
                    "tool_call_end",
                    &format!(
                        "\"tool_type\":\"exec_command\",\"call_id\":\"{}\",\"command\":{},\"exit_code\":{},\"status\":\"{:?}\"",
                        ev.call_id, cmd, ev.exit_code, ev.status
                    ),
                );
            }
            EventMsg::McpToolCallBegin(ev) => {
                let tool_name = json_string(&ev.invocation.tool);
                let server = json_string(&ev.invocation.server);
                self.log(
                    "tool_call_begin",
                    &format!(
                        "\"tool_type\":\"mcp\",\"call_id\":\"{}\",\"server\":{},\"tool_name\":{}",
                        ev.call_id, server, tool_name
                    ),
                );
            }
            EventMsg::McpToolCallEnd(ev) => {
                let tool_name = json_string(&ev.invocation.tool);
                let server = json_string(&ev.invocation.server);
                self.log(
                    "tool_call_end",
                    &format!(
                        "\"tool_type\":\"mcp\",\"call_id\":\"{}\",\"server\":{},\"tool_name\":{},\"success\":{}",
                        ev.call_id,
                        server,
                        tool_name,
                        ev.is_success()
                    ),
                );
            }
            EventMsg::DynamicToolCallRequest(ev) => {
                let tool = json_string(&ev.tool);
                self.log(
                    "tool_call_begin",
                    &format!(
                        "\"tool_type\":\"dynamic\",\"call_id\":\"{}\",\"tool_name\":{}",
                        ev.call_id, tool
                    ),
                );
            }
            EventMsg::DynamicToolCallResponse(ev) => {
                let tool = json_string(&ev.tool);
                self.log(
                    "tool_call_end",
                    &format!(
                        "\"tool_type\":\"dynamic\",\"call_id\":\"{}\",\"tool_name\":{},\"success\":{}",
                        ev.call_id, tool, ev.success
                    ),
                );
            }
            EventMsg::WebSearchBegin(ev) => {
                self.log(
                    "tool_call_begin",
                    &format!(
                        "\"tool_type\":\"web_search\",\"call_id\":\"{}\"",
                        ev.call_id
                    ),
                );
            }
            EventMsg::WebSearchEnd(ev) => {
                let query = json_string(&ev.query);
                self.log(
                    "tool_call_end",
                    &format!(
                        "\"tool_type\":\"web_search\",\"call_id\":\"{}\",\"query\":{}",
                        ev.call_id, query
                    ),
                );
            }
            EventMsg::ViewImageToolCall(ev) => {
                let path = json_string(ev.path.to_string_lossy().as_ref());
                self.log(
                    "tool_call_begin",
                    &format!(
                        "\"tool_type\":\"view_image\",\"call_id\":\"{}\",\"path\":{}",
                        ev.call_id, path
                    ),
                );
            }
            _ => {
                self.items_this_turn += 1;
            }
        }
    }

    fn log_compact_decision(
        &mut self,
        phase: &str,
        reason: &crate::context::compact::CompactReason,
        policy: &CompactPolicy,
    ) {
        self.log(
            &format!("compact_{phase}"),
            &format!(
                "\"reason\":\"{}\",\"trigger\":\"{:?}\",\"mode\":\"{:?}\"",
                reason.log_summary(),
                policy.trigger,
                policy.mode
            ),
        );
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"<invalid>\"".to_string())
}
