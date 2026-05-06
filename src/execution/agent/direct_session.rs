// Origin: CTOX
// License: Apache-2.0
//
// Direct session: in-process ctox-core integration via InProcessAppServerClient.
// One persistent client per CTOX mission-turn-loop. Multiple sequential turns
// (main turn + continuity refreshes) reuse the same client and thread.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ctox_app_server_client::{
    InProcessAppServerClient, InProcessClientStartArgs, InProcessServerEvent,
    DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
};
use ctox_app_server_protocol::{
    ClientRequest, JSONRPCNotification, RequestId, ServerNotification, ThreadCompactStartParams,
    ThreadCompactStartResponse, ThreadStartParams, ThreadStartResponse, ThreadUnsubscribeParams,
    ThreadUnsubscribeResponse, TurnInterruptParams, TurnInterruptResponse, TurnStartParams,
    TurnStartResponse,
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
use ctox_protocol::protocol::{AskForApproval, EventMsg, SandboxPolicy, SessionSource};
use ctox_protocol::user_input::UserInput;
use ctox_utils_absolute_path::AbsolutePathBuf;

use crate::api_costs::{self, ApiCallTelemetry, ApiTokenUsage};
use crate::context::compact::{CompactDecision, CompactMode, CompactPolicy, CompactTrigger};
use crate::inference::engine;
use crate::inference::runtime_kernel;
use crate::inference::runtime_state;
use crate::secrets;

const OPENAI_AUTH_MODE_KEY: &str = "CTOX_OPENAI_AUTH_MODE";
const OPENAI_AUTH_MODE_CHATGPT_SUBSCRIPTION: &str = "chatgpt_subscription";
const DIRECT_SESSION_CONTROL_REQUEST_TIMEOUT_SECS: u64 = 5;
const DIRECT_SESSION_MIDTASK_COMPACT_TIMEOUT_SECS: u64 = 90;
const DIRECT_SESSION_INTERRUPT_TIMEOUT_SECS: u64 = 2;
const CTOX_DIRECT_SESSION_BASE_INSTRUCTIONS: &str = r#"You are an agent working inside CTOX.

Complete a work step only when the required durable outcome exists in CTOX runtime state. A final answer, summary, note file, or statement such as "sent", "done", or "closed" is not evidence by itself.

When the request requires filesystem changes, command execution, runtime inspection, benchmark execution, ticket/state updates, or artifact verification, use the available terminal/shell tools to do the work. Do not substitute a code block, plan, or textual description for executing the step.

If the work requires an artifact, verify the artifact before finishing. For proactive outbound email, produce the final send-ready body first and do not run reviewed-send before review feedback. When a reviewed-send continuation prompt provides the exact approved body and command, execute only that command and verify the accepted outbound row. Do not create review rows or approval digests manually.

If an API, provider, tool, or runtime call fails or is rate-limited, do not claim completion. Retry only when appropriate; otherwise keep the work open with the blocker recorded.

When review feedback is returned, continue the same main work step whenever possible. Do not create review-driven self-work or subtask cascades. Spawn a new task only for a distinct bounded work step, and include a clear parent or thread anchor.

Use plain English in your own reasoning and replies. Do not expose internal source-code labels when a normal phrase is clearer; for example, say "work step" or "agent run" instead of "slice"."#;

fn compose_base_instructions(extra: Option<&str>) -> String {
    match extra.map(str::trim).filter(|value| !value.is_empty()) {
        Some(extra) => format!("{CTOX_DIRECT_SESSION_BASE_INSTRUCTIONS}\n\n{extra}"),
        None => CTOX_DIRECT_SESSION_BASE_INSTRUCTIONS.to_string(),
    }
}

#[derive(Debug, Clone)]
struct TerminalBenchPreflightGuard {
    run_dir: String,
    required_files: Vec<String>,
    requires_runtime_refs: bool,
    first_exec_seen: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TerminalBenchPreflightSpec {
    pub(crate) run_dir: String,
    pub(crate) required_files: Vec<String>,
    pub(crate) requires_runtime_refs: bool,
}

impl TerminalBenchPreflightGuard {
    fn from_spec(spec: TerminalBenchPreflightSpec) -> Option<Self> {
        let run_dir = spec.run_dir.trim().to_string();
        let mut required_files = Vec::new();
        for path in spec.required_files {
            let path = path.trim();
            if path.is_empty() || !path.contains(&run_dir) {
                continue;
            }
            if required_files.iter().any(|existing| existing == path) {
                continue;
            }
            required_files.push(path.to_string());
        }
        if run_dir.is_empty() || required_files.is_empty() {
            return None;
        }
        Some(Self {
            run_dir,
            required_files,
            requires_runtime_refs: spec.requires_runtime_refs,
            first_exec_seen: false,
        })
    }

    fn from_prompt(prompt: &str) -> Option<Self> {
        let lower = prompt.to_ascii_lowercase();
        if !(lower.contains("terminal-bench")
            && lower.contains("only required durable files for this controller turn"))
            && !prompt.contains("HARNESS TERMINAL-BENCH PREFLIGHT")
        {
            return None;
        }
        let relevant_prompt = prompt
            .rfind("HARNESS TERMINAL-BENCH PREFLIGHT")
            .or_else(|| prompt.rfind("Only required durable files for this controller turn"))
            .map(|idx| &prompt[idx..])
            .unwrap_or(prompt);
        let run_dir = extract_terminal_bench_run_dir(relevant_prompt)?;
        let required_files = extract_terminal_bench_required_files(relevant_prompt, &run_dir);
        if required_files.is_empty() {
            return None;
        }
        Some(Self {
            run_dir,
            required_files,
            requires_runtime_refs: lower.contains("preparation queue")
                || lower.contains("preparation queue/tickets")
                || lower.contains("queue:system::*")
                || lower.contains("preparation-tickets.jsonl"),
            first_exec_seen: false,
        })
    }

    fn violation_for_first_exec(&mut self, command: &str) -> Option<String> {
        if self.first_exec_seen {
            return None;
        }
        self.first_exec_seen = true;
        if terminal_bench_first_preflight_command_is_valid(
            command,
            &self.run_dir,
            &self.required_files,
            self.requires_runtime_refs,
        ) {
            return None;
        }
        Some(terminal_bench_preflight_violation_feedback(
            command,
            &self.run_dir,
            &self.required_files,
            self.requires_runtime_refs,
        ))
    }
}

fn terminal_bench_preflight_violation_feedback(
    command: &str,
    run_dir: &str,
    required_files: &[String],
    requires_runtime_refs: bool,
) -> String {
    let required_files = required_files
        .iter()
        .map(|path| format!("- {path}"))
        .collect::<Vec<_>>()
        .join("\n");
    let runtime_refs = if requires_runtime_refs {
        "The same script must also persist either real `ctox queue add` output/message keys or an explicit `blocker` value explaining the exact CLI blocker."
    } else {
        "The same script must persist enough controller state to continue the run."
    };
    format!(
        "terminal-bench preflight violation: the first shell command did not create and verify the required current-run artifacts. First command: {}.\n\nWhy this failed: the first worker shell call must be one complete artifact bootstrap script, not a partial mkdir/cat/help/discovery step. It must mention and create every required file, create `{run_dir}/tasks`, include `test -f` checks for every required file, and finish only if those checks pass. {runtime_refs}\n\nRequired next worker action: run exactly one `exec_command` shell script now. Do not answer in prose. Do not call `ctox --help`, inspect old runs, inspect install trees, browse, or split this over multiple tool calls before the files below are verified.\n\nRequired files:\n{required_files}\n\nAcceptance checklist for the next shell script:\n- sets `RUN_DIR=\"{run_dir}\"`\n- runs `mkdir -p \"$RUN_DIR/tasks\"`\n- writes non-empty initial content to every required file above\n- records queue refs with `ctox queue add` or records an explicit `blocker` in controller.json/logbook.md\n- runs `test -f` for every required file above in the same shell call\n- exits nonzero if any required file is missing\n\nThe harness will not create files, create tickets, or mark this complete for the worker.",
        clip_text_local(command, 360),
    )
}

fn clip_text_local(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut clipped = collapsed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    clipped.push('…');
    clipped
}

fn extract_terminal_bench_run_dir(text: &str) -> Option<String> {
    let marker = "/terminal-bench-2/runs/";
    let start = text.find(marker)?;
    let prefix_start = text[..start]
        .rfind(|ch: char| ch.is_whitespace() || matches!(ch, '`' | '"' | '\'' | '(' | '[' | '<'))
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let tail = &text[start + marker.len()..];
    let run_id_len = tail
        .find(|ch: char| {
            ch.is_whitespace() || matches!(ch, '/' | '`' | '"' | '\'' | ')' | ']' | '>' | ',' | ';')
        })
        .unwrap_or(tail.len());
    if run_id_len == 0 {
        return None;
    }
    Some(text[prefix_start..start + marker.len() + run_id_len].to_string())
}

fn extract_terminal_bench_required_files(text: &str, run_dir: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in text.lines() {
        let Some(start) = line.find(run_dir) else {
            continue;
        };
        let candidate = line[start..]
            .trim()
            .trim_matches(|ch: char| matches!(ch, '`' | '"' | '\'' | ',' | ';' | '.'));
        if candidate.contains("/tasks/") || candidate.contains("<task-id>") {
            continue;
        }
        if files.iter().any(|existing| existing == candidate) {
            continue;
        }
        files.push(candidate.to_string());
    }
    files
}

fn terminal_bench_first_preflight_command_is_valid(
    command: &str,
    run_dir: &str,
    required_files: &[String],
    requires_runtime_refs: bool,
) -> bool {
    let lower = command.to_ascii_lowercase();
    let run_dir_lower = run_dir.to_ascii_lowercase();
    if !lower.contains(&run_dir_lower) {
        return false;
    }
    if !(lower.contains("mkdir") && lower.contains("/tasks")) {
        return false;
    }
    if !lower.contains("test -f") {
        return false;
    }
    let creates_files = [
        "cat >", "cat <<", "tee ", "touch ", "printf ", "python", "perl ", "jq ", "install ", ">>",
        ">",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    if !creates_files {
        return false;
    }
    for path in required_files {
        let basename = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path)
            .to_ascii_lowercase();
        if !basename.is_empty() && !lower.contains(&basename) {
            return false;
        }
    }
    if requires_runtime_refs && !(lower.contains("queue add") || lower.contains("blocker")) {
        return false;
    }
    true
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
    selected_api_provider.is_some_and(|provider| provider.eq_ignore_ascii_case("openai"))
        && openai_chatgpt_subscription_auth_enabled(settings)
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
// PersistentSession — lives across turns within a mission-turn-loop iteration
// ---------------------------------------------------------------------------

/// Holds a running InProcessAppServerClient + thread. Created once per
/// mission-turn-loop iteration, reused for the main turn AND all continuity
/// refresh calls. Solves the resource-exhaustion hang that occurred when
/// spawning a new client per call.
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
}

impl PersistentSession {
    /// Start a persistent session: creates tokio runtime, app-server client,
    /// and thread. Call this ONCE per mission-turn-loop iteration.
    pub fn start(root: &Path, settings: &BTreeMap<String, String>) -> Result<Self> {
        Self::start_with_instructions(root, settings, None, false)
    }

    /// Start a persistent session with explicit base instructions and optional
    /// compaction disablement. Review runs use this to create an isolated
    /// external-review thread with its own system prompt and without normal
    /// long-run compaction behavior.
    pub fn start_with_instructions(
        root: &Path,
        settings: &BTreeMap<String, String>,
        base_instructions: Option<&str>,
        disable_compaction: bool,
    ) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .context("failed to start tokio runtime")?;

        let composed_base_instructions = compose_base_instructions(base_instructions);
        let (client, thread_id, cwd, seq, model, model_provider, api_provider, reasoning_effort) =
            rt.block_on(async {
                Self::start_client_and_thread(root, settings, &composed_base_instructions).await
            })?;

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
            policy.trigger = CompactTrigger::Off;
            policy.emergency_fill_ratio = 2.0;
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
        self.run_turn_with_terminal_bench_preflight(prompt, timeout, None)
    }

    pub(crate) fn run_turn_with_terminal_bench_preflight(
        &mut self,
        prompt: &str,
        timeout: Option<Duration>,
        terminal_bench_preflight: Option<TerminalBenchPreflightSpec>,
    ) -> Result<String> {
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("session already shut down"))?;
        let thread_id = self.thread_id.clone();
        let cwd = self.cwd.clone();
        let model = self.model.clone();
        let model_provider = self.model_provider.clone();
        let api_provider = self.api_provider.clone();
        let reasoning_effort = self.reasoning_effort;
        let prompt = prompt.to_string();
        let root = self.root.clone();
        let base_instructions = self.base_instructions.clone();
        let terminal_bench_preflight_guard =
            terminal_bench_preflight.and_then(TerminalBenchPreflightGuard::from_spec);

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
                &thread_id,
                &cwd,
                &model,
                model_provider.as_deref(),
                api_provider.as_deref(),
                reasoning_effort,
                &root,
                &prompt,
                &base_instructions,
                timeout,
                &mut self.seq,
                &mut self.policy,
                &mut self.ctx_log,
                terminal_bench_preflight_guard,
            )
            .await
        });

        result
    }

    /// Shut down the client and runtime cleanly.
    pub fn shutdown(mut self) {
        self.shutdown_inner("shutting down");
    }

    // --- Internal async helpers ---

    async fn start_client_and_thread(
        root: &Path,
        settings: &BTreeMap<String, String>,
        base_instructions: &str,
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
        let model = settings
            .get("CTOX_CHAT_MODEL")
            .or_else(|| settings.get("CODEX_MODEL"))
            .cloned()
            .or(runtime_model)
            .unwrap_or_else(|| "gpt-5.4-mini".to_string());
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
        let cwd: PathBuf = root.to_path_buf();

        let codex_home =
            find_codex_home().map_err(|err| anyhow::anyhow!("find_codex_home: {err}"))?;
        let use_chatgpt_subscription_auth =
            use_openai_chatgpt_subscription_auth(settings, selected_api_provider.as_deref());

        let selected_api_key_name = selected_api_provider
            .as_deref()
            .map(runtime_state::api_key_env_var_for_provider);
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

        let auth_manager = if let Some(ref key) = api_key {
            AuthManager::from_runtime_auth(
                ctox_core::CodexAuth::from_api_key(key),
                codex_home.clone(),
            )
        } else {
            AuthManager::shared(
                codex_home.clone(),
                false,
                config_toml.cli_auth_credentials_store.unwrap_or_default(),
            )
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
        if api_provider.is_some() && local_provider.is_none() && api_key.is_none() {
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
            sandbox_mode: Some(SandboxMode::DangerFullAccess),
            ephemeral: Some(true),
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
        let thread_manager = Arc::new(ThreadManager::new(
            config.as_ref(),
            auth_manager.clone(),
            SessionSource::Exec,
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
            session_source: SessionSource::Exec,
            enable_ctox_api_key_env: false,
            client_name: "ctox-direct".to_string(),
            client_version: env!("CTOX_BUILD_VERSION").to_string(),
            experimental_api: true,
            opt_out_notification_methods: vec![],
            channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
        };

        eprintln!("[ctox direct-session] starting InProcessAppServerClient...");
        let client = InProcessAppServerClient::start(start_args)
            .await
            .map_err(|err| anyhow::anyhow!("client start: {err}"))?;
        eprintln!("[ctox direct-session] client started");

        let mut seq = RequestIdSeq::new();
        let thread_resp: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: seq.next(),
                params: ThreadStartParams {
                    model: Some(model.clone()),
                    model_provider: selected_provider_id.clone(),
                    cwd: Some(cwd.to_string_lossy().to_string()),
                    approval_policy: Some(AskForApproval::Never.into()),
                    sandbox: Some(ctox_app_server_protocol::SandboxMode::DangerFullAccess),
                    base_instructions: Some(base_instructions.to_string()),
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .map_err(|err| anyhow::anyhow!("thread/start: {err}"))?;

        let thread_id = thread_resp.thread.id.clone();
        eprintln!("[ctox direct-session] thread started: {}", thread_id);

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
        _old_thread_id: &str,
        cwd: &Path,
        model: &str,
        model_provider: Option<&str>,
        api_provider: Option<&str>,
        reasoning_effort: Option<ReasoningEffort>,
        root: &Path,
        prompt: &str,
        base_instructions: &str,
        timeout: Option<Duration>,
        seq: &mut RequestIdSeq,
        policy: &mut CompactPolicy,
        ctx_log: &mut ContextLogger,
        terminal_bench_preflight_guard: Option<TerminalBenchPreflightGuard>,
    ) -> Result<String> {
        // Create a fresh thread per turn — reuse the same CLIENT but not
        // the same thread. After TurnComplete, the thread may not accept
        // new TurnStart requests in all ctox-core versions.
        let thread_resp: ThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: seq.next(),
                params: ThreadStartParams {
                    model: Some(model.to_string()),
                    model_provider: model_provider.map(str::to_string),
                    cwd: Some(cwd.to_string_lossy().to_string()),
                    approval_policy: Some(AskForApproval::Never.into()),
                    sandbox: Some(ctox_app_server_protocol::SandboxMode::DangerFullAccess),
                    base_instructions: Some(base_instructions.to_string()),
                    ephemeral: Some(true),
                    ..ThreadStartParams::default()
                },
            })
            .await
            .map_err(|err| anyhow::anyhow!("thread/start: {err}"))?;
        let thread_id = thread_resp.thread.id;
        eprintln!("[ctox direct-session] new thread for turn: {}", thread_id);

        // TurnStart
        let turn_resp: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: seq.next(),
                params: TurnStartParams {
                    thread_id: thread_id.to_string(),
                    input: vec![UserInput::Text {
                        text: prompt.to_string(),
                        text_elements: Vec::new(),
                    }
                    .into()],
                    cwd: Some(cwd.to_path_buf()),
                    approval_policy: Some(AskForApproval::Never.into()),
                    approvals_reviewer: None,
                    sandbox_policy: Some(SandboxPolicy::DangerFullAccess.into()),
                    model: None,
                    service_tier: None,
                    effort: reasoning_effort,
                    summary: None,
                    personality: None,
                    output_schema: None,
                    collaboration_mode: None,
                },
            })
            .await
            .map_err(|err| anyhow::anyhow!("turn/start: {err}"))?;
        let turn_id = turn_resp.turn.id;

        // Event loop
        let mut final_message: Option<String> = None;
        let mut forced_followup_fired = false;
        let mut terminal_bench_preflight_guard = terminal_bench_preflight_guard
            .or_else(|| TerminalBenchPreflightGuard::from_prompt(prompt));
        let turn_started_at = Instant::now();
        let mut last_usage_event_at = turn_started_at;
        let mut last_recorded_cumulative_usage: Option<ApiTokenUsage> = None;
        let deadline = timeout.map(|d| tokio::time::Instant::now() + d);

        loop {
            let event = match deadline {
                Some(d) => tokio::select! {
                    ev = client.next_event() => ev,
                    _ = tokio::time::sleep_until(d) => {
                        let interrupt_req = ClientRequest::TurnInterrupt {
                            request_id: seq.next(),
                            params: TurnInterruptParams {
                                thread_id: thread_id.to_string(),
                                turn_id: turn_id.to_string(),
                            },
                        };
                        let _ = tokio::time::timeout(
                            Duration::from_secs(DIRECT_SESSION_INTERRUPT_TIMEOUT_SECS),
                            client.request_typed::<TurnInterruptResponse>(interrupt_req),
                        ).await;
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
                        if let EventMsg::ExecCommandBegin(ev) = &msg {
                            let command = ev.command.join(" ");
                            if let Some(feedback) = terminal_bench_preflight_guard
                                .as_mut()
                                .and_then(|guard| guard.violation_for_first_exec(&command))
                            {
                                ctx_log.log(
                                    "terminal_bench_preflight_violation",
                                    &format!(
                                        "\"turn_id\":\"{}\",\"call_id\":\"{}\",\"feedback\":{}",
                                        turn_id,
                                        ev.call_id,
                                        json_string(&feedback)
                                    ),
                                );
                                let interrupt_req = ClientRequest::TurnInterrupt {
                                    request_id: seq.next(),
                                    params: TurnInterruptParams {
                                        thread_id: thread_id.to_string(),
                                        turn_id: turn_id.to_string(),
                                    },
                                };
                                let _ = tokio::time::timeout(
                                    Duration::from_secs(DIRECT_SESSION_INTERRUPT_TIMEOUT_SECS),
                                    client.request_typed::<TurnInterruptResponse>(interrupt_req),
                                )
                                .await;
                                anyhow::bail!("{feedback}");
                            }
                        }
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
                                let result = api_costs::record_api_model_usage_with_telemetry(
                                    root,
                                    provider,
                                    model,
                                    Some(&turn_id),
                                    ApiTokenUsage {
                                        input_tokens: usage.input_tokens,
                                        cached_input_tokens: usage.cached_input_tokens,
                                        output_tokens: usage.output_tokens,
                                        reasoning_output_tokens: usage.reasoning_output_tokens,
                                        total_tokens: usage.total_tokens,
                                    },
                                    Some(ApiCallTelemetry {
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
                                );
                                if let Err(err) = result {
                                    eprintln!("[ctox direct-session] cost tracking failed: {err}");
                                }
                            }
                        }

                        if let CompactDecision::Compact { reason } = policy.evaluate(&msg) {
                            eprintln!(
                                "[ctox direct-session] compact mode={:?} reason={}",
                                policy.mode,
                                reason.log_summary()
                            );
                            match policy.mode {
                                CompactMode::MidTask => {
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
                                            policy.note_compacted();
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
                                CompactMode::ForcedFollowup => {
                                    let unsub_req = ClientRequest::ThreadUnsubscribe {
                                        request_id: seq.next(),
                                        params: ThreadUnsubscribeParams {
                                            thread_id: thread_id.to_string(),
                                        },
                                    };
                                    let _ = tokio::time::timeout(
                                        direct_session_control_request_timeout(deadline),
                                        client
                                            .request_typed::<ThreadUnsubscribeResponse>(unsub_req),
                                    )
                                    .await;
                                    let signal = root.join("runtime/compact-followup-requested");
                                    let _ =
                                        std::fs::create_dir_all(signal.parent().unwrap_or(root));
                                    if let Ok(mut f) = std::fs::OpenOptions::new()
                                        .create(true)
                                        .append(true)
                                        .open(&signal)
                                    {
                                        let _ = writeln!(
                                            f,
                                            "{}\t{}\t{}",
                                            0,
                                            thread_id,
                                            reason.log_summary()
                                        );
                                    }
                                    policy.note_compacted();
                                    forced_followup_fired = true;
                                    break;
                                }
                            }
                        }
                        match msg {
                            EventMsg::AgentMessage(am) => {
                                final_message = Some(am.message.clone());
                            }
                            EventMsg::TurnComplete(tc) if tc.turn_id == turn_id => break,
                            EventMsg::Error(ref err) => {
                                let msg_str = format!("{:?}", err);
                                // Compaction errors are non-fatal — the model
                                // may not produce the exact JSON format ctox-core
                                // expects for compaction. Log and continue.
                                if msg_str.contains("compaction")
                                    || msg_str.contains("revisedTitle")
                                {
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

        ctx_log.log(
            "turn_end",
            &format!(
                "\"forced_followup\":{forced_followup_fired},\"reply_chars\":{}",
                final_message.as_ref().map(|m| m.len()).unwrap_or(0)
            ),
        );

        if forced_followup_fired {
            Ok(final_message.unwrap_or_default())
        } else {
            final_message.ok_or_else(|| anyhow::anyhow!("turn completed without assistant message"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!use_openai_chatgpt_subscription_auth(
            &settings,
            Some("anthropic")
        ));
        assert!(!use_openai_chatgpt_subscription_auth(&settings, None));
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
    fn base_instructions_include_durable_outcome_contract() {
        let instructions = compose_base_instructions(None);
        assert!(instructions.contains("required durable outcome exists"));
        assert!(instructions.contains("do not run reviewed-send before review feedback"));
        assert!(instructions.contains("Do not create review rows or approval digests manually"));
        assert!(instructions.contains("accepted outbound row"));
        assert!(instructions.contains("Do not create review-driven self-work"));
    }

    #[test]
    fn base_instructions_preserve_extra_review_prompt() {
        let instructions = compose_base_instructions(Some("Act as the external reviewer."));
        assert!(instructions.contains("required durable outcome exists"));
        assert!(instructions.contains("Act as the external reviewer."));
    }

    #[test]
    fn terminal_bench_preflight_guard_rejects_first_discovery_command() {
        let run_dir = "/home/metricspace/CTOX/runtime/terminal-bench-2/runs/test-run";
        let prompt = format!(
            "HARNESS TERMINAL-BENCH PREFLIGHT\n\
Only required durable files for this controller turn:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/preparation-tickets.jsonl\n\
- {run_dir}/run-queue.jsonl\n\
- {run_dir}/results.jsonl\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/logbook.md\n\
- {run_dir}/blogpost-notes.md\n\
The controller must create preparation queue/tickets and record queue:system::* keys."
        );
        let mut guard = TerminalBenchPreflightGuard::from_prompt(&prompt).unwrap();

        let violation = guard
            .violation_for_first_exec("ls -la /home/metricspace/.local/share/ctox/install/current");

        assert!(violation
            .as_deref()
            .unwrap_or_default()
            .contains("terminal-bench preflight violation"));
        assert!(violation.unwrap().contains("first shell command"));
        assert!(guard
            .violation_for_first_exec("ls -la /home/metricspace")
            .is_none());
    }

    #[test]
    fn terminal_bench_preflight_guard_rejects_discovery_from_explicit_spec() {
        let run_dir = "/home/metricspace/CTOX/runtime/terminal-bench-2/runs/current-run";
        let mut guard = TerminalBenchPreflightGuard::from_spec(TerminalBenchPreflightSpec {
            run_dir: run_dir.to_string(),
            required_files: vec![
                format!("{run_dir}/controller.json"),
                format!("{run_dir}/ticket-map.jsonl"),
                format!("{run_dir}/logbook.md"),
            ],
            requires_runtime_refs: true,
        })
        .unwrap();

        let violation = guard.violation_for_first_exec(
            "ls -la /home/metricspace/.local/share/ctox/install/current/runtime",
        );

        assert!(violation
            .as_deref()
            .unwrap_or_default()
            .contains("terminal-bench preflight violation"));
    }

    #[test]
    fn terminal_bench_preflight_guard_accepts_artifact_creation_script() {
        let run_dir = "/home/metricspace/CTOX/runtime/terminal-bench-2/runs/test-run";
        let prompt = format!(
            "HARNESS TERMINAL-BENCH PREFLIGHT\n\
Only required durable files for this controller turn:\n\
- {run_dir}/controller.json\n\
- {run_dir}/ticket-map.jsonl\n\
- {run_dir}/preparation-tickets.jsonl\n\
- {run_dir}/run-queue.jsonl\n\
- {run_dir}/results.jsonl\n\
- {run_dir}/knowledge.md\n\
- {run_dir}/logbook.md\n\
- {run_dir}/blogpost-notes.md\n\
The controller must create preparation queue/tickets and record queue:system::* keys."
        );
        let mut guard = TerminalBenchPreflightGuard::from_prompt(&prompt).unwrap();
        let command = format!(
            "RUN_DIR={run_dir}; mkdir -p \"$RUN_DIR/tasks\"; \
touch \"$RUN_DIR/controller.json\" \"$RUN_DIR/ticket-map.jsonl\" \
\"$RUN_DIR/preparation-tickets.jsonl\" \"$RUN_DIR/run-queue.jsonl\" \
\"$RUN_DIR/results.jsonl\" \"$RUN_DIR/knowledge.md\" \"$RUN_DIR/logbook.md\" \
\"$RUN_DIR/blogpost-notes.md\"; ctox queue add --title prep-runtime --prompt x; \
test -f \"$RUN_DIR/controller.json\" && test -f \"$RUN_DIR/ticket-map.jsonl\" && \
test -f \"$RUN_DIR/preparation-tickets.jsonl\" && test -f \"$RUN_DIR/run-queue.jsonl\" && \
test -f \"$RUN_DIR/results.jsonl\" && test -f \"$RUN_DIR/knowledge.md\" && \
test -f \"$RUN_DIR/logbook.md\" && test -f \"$RUN_DIR/blogpost-notes.md\""
        );

        assert!(guard.violation_for_first_exec(&command).is_none());
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
}

impl Drop for PersistentSession {
    fn drop(&mut self) {
        self.shutdown_inner("dropping");
    }
}

impl PersistentSession {
    fn shutdown_inner(&mut self, action: &str) {
        let Some(runtime) = self.runtime.take() else {
            return;
        };
        if let Some(client) = self.client.take() {
            let tid = self.thread_id.clone();
            eprintln!("[ctox direct-session] {action} persistent session thread_id={tid}");
            client.abort_now();
            eprintln!("[ctox direct-session] persistent session aborted thread_id={tid}");
        }
        runtime.shutdown_timeout(Duration::from_secs(2));
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

fn try_extract_event_msg(notif: &JSONRPCNotification) -> Option<EventMsg> {
    let method = notif
        .method
        .strip_prefix("codex/event/")
        .unwrap_or(&notif.method)
        .to_string();
    let value = notif.params.clone()?;
    let serde_json::Value::Object(mut obj) = value else {
        return None;
    };
    let mut payload = if let Some(serde_json::Value::Object(msg_obj)) = obj.get("msg") {
        serde_json::Value::Object(msg_obj.clone())
    } else {
        obj.remove("conversationId");
        serde_json::Value::Object(obj)
    };
    if let serde_json::Value::Object(ref mut map) = payload {
        map.insert("type".to_string(), serde_json::Value::String(method));
    }
    serde_json::from_value(payload).ok()
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
            self.last_total_tokens, self.last_context_window, self.items_this_turn, self.session_kind
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
