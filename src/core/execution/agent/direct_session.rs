// Origin: CTOX
// License: AGPL-3.0-only
//
// Direct session: in-process ctox-core integration via InProcessAppServerClient.
// One persistent client for the normal CTOX worker lane. Sequential work
// slices and their continuity refreshes reuse the same durable thread.

use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
#[cfg(target_os = "linux")]
use std::collections::HashSet;
use std::collections::{BTreeMap, HashMap};
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
    ClientRequest, JSONRPCNotification, ServerNotification, ThreadCompactStartParams,
    ThreadCompactStartResponse, TurnInterruptParams, TurnInterruptResponse, TurnStartParams,
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
use ctox_protocol::plan_tool::{PlanItemArg, StepStatus, UpdatePlanArgs};
use ctox_protocol::protocol::{
    AskForApproval, CodexErrorInfo, EventMsg, SandboxPolicy, SessionSource,
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

pub(crate) use super::session_continuity::SessionPoisoned;
use super::session_continuity::{
    bind_session_thread, start_bound_turn, RequestIdSeq, SessionControlTimeouts, SessionThreadSpec,
};
#[path = "direct_session_reply.rs"]
mod reply_capture;
use reply_capture::DirectSessionReplyCapture;

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
// session runtime is wedged (ctox#21). 60 s rather than 30 s: thread/start
// opens the core SQLite store (multi-MB process-mining schema) twice, which on
// customer on-prem hosts with ~4x slower cores measured past 30 s (05.10.2026).
const DIRECT_SESSION_TURN_START_TIMEOUT_SECS: u64 = 60;

fn queue_turn_terminal_event(event: &InProcessServerEvent, thread_id: &str, turn_id: &str) -> bool {
    match event {
        InProcessServerEvent::ServerNotification(ServerNotification::TurnCompleted(done)) => {
            done.thread_id == thread_id && done.turn.id == turn_id
        }
        InProcessServerEvent::LegacyNotification(notification)
            if legacy_notification_thread_id(notification) == Some(thread_id) =>
        {
            match try_extract_event_msg(notification) {
                Some(EventMsg::TurnComplete(done)) => done.turn_id == turn_id,
                Some(EventMsg::TurnAborted(done)) => done.turn_id.as_deref() == Some(turn_id),
                _ => false,
            }
        }
        _ => false,
    }
}

/// Keep draining while the exact scoped interrupt is in flight. Only a
/// matching terminal event proves this turn stopped; an RPC ack does not.
async fn interrupt_cancelled_queue_turn(
    client: &mut InProcessAppServerClient,
    seq: &mut RequestIdSeq,
    thread_id: &str,
    turn_id: &str,
) -> bool {
    let handle = client.request_handle();
    let interrupt = handle.request_typed::<TurnInterruptResponse>(ClientRequest::TurnInterrupt {
        request_id: seq.next(),
        params: TurnInterruptParams {
            thread_id: thread_id.to_owned(),
            turn_id: turn_id.to_owned(),
        },
    });
    tokio::pin!(interrupt);
    let deadline =
        tokio::time::Instant::now() + Duration::from_secs(DIRECT_SESSION_INTERRUPT_TIMEOUT_SECS);
    let mut acknowledged = false;
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => return false,
            result = &mut interrupt, if !acknowledged => {
                if result.is_err() {
                    return false;
                }
                acknowledged = true;
            }
            event = client.next_event() => {
                let Some(event) = event else { return false };
                if queue_turn_terminal_event(&event, thread_id, turn_id) {
                    return true;
                }
            }
        }
    }
}

fn production_session_control_timeouts() -> SessionControlTimeouts {
    SessionControlTimeouts {
        list: Duration::from_secs(DIRECT_SESSION_CONTROL_REQUEST_TIMEOUT_SECS),
        resume: Duration::from_secs(DIRECT_SESSION_TURN_START_TIMEOUT_SECS),
        start: Duration::from_secs(DIRECT_SESSION_TURN_START_TIMEOUT_SECS),
        turn_start: Duration::from_secs(DIRECT_SESSION_TURN_START_TIMEOUT_SECS),
    }
}

const EXACT_PROMPT_SAFE_INPUT_BUDGET_NUMERATOR: i64 = 3;
const EXACT_PROMPT_SAFE_INPUT_BUDGET_DENOMINATOR: i64 = 4;
const CTOX_PERSISTENT_WORKER_THREAD_NAME: &str = "ctox-service-worker";
const BUSINESS_OS_MCP_ADDR_KEY: &str = "CTOX_BUSINESS_OS_MCP_ADDR";
const BUSINESS_OS_MCP_DEFAULT_ADDR: &str = "127.0.0.1:8788";
#[cfg(unix)]
#[path = "native_guest_mcp.rs"]
mod native_guest_mcp;

const BUSINESS_OS_MCP_SESSION_SERVER_NAME: &str = "ctox-business-os";
const BUSINESS_OS_MCP_SESSION_TOOLS: &[&str] = &[
    "business_os.get_module",
    "business_os.list_entities",
    "business_os.query_records",
    "business_os.search_records",
    "business_os.get_record",
    "business_os.get_record_context",
    "business_os.list_module_actions",
    "business_os.propose_action",
    "business_os.execute_action",
    // Befund 07.09.2026: the research writeback contract (mechanism
    // business_command, command_type outbound.lead.research_writeback) can only
    // be fulfilled through this tool. Without it in the session allowlist every
    // research task ends in "no successful writeback receipt".
    "business_os.execute_writeback",
    "business_os.get_command_status",
    "business_os.workjet_worker_dispatch",
    // These tools still require the signed, currently leased registered
    // Supervisor in the native MCP handler. The harness filter must not hide
    // them from that execution when a scheduled meeting needs preparation.
    "business_os.jour_fixe_read",
    "business_os.jour_fixe_update",
    "business_os.list_runs",
    "business_os.get_run",
];
#[cfg(test)]
static DIRECT_SESSION_EVENT_DESERIALIZE_CALLS: AtomicUsize = AtomicUsize::new(0);

fn direct_session_arg0_paths() -> Arg0DispatchPaths {
    Arg0DispatchPaths {
        #[cfg(target_os = "linux")]
        ctox_linux_sandbox_exe: std::env::current_exe().ok(),
        #[cfg(not(target_os = "linux"))]
        ctox_linux_sandbox_exe: None,
        main_execve_wrapper_exe: None,
    }
}

fn business_os_mcp_thread_config(
    configured_addr: &str,
    token: &str,
    command_session_token: &str,
) -> Result<HashMap<String, JsonValue>> {
    let token = token.trim();
    anyhow::ensure!(!token.is_empty(), "Business OS MCP token is empty");
    let command_session_token = command_session_token.trim();
    anyhow::ensure!(
        !command_session_token.is_empty(),
        "Business OS MCP command-session token is empty"
    );
    let configured_addr = configured_addr.trim();
    anyhow::ensure!(
        !configured_addr.is_empty(),
        "Business OS MCP address is empty"
    );
    anyhow::ensure!(
        !configured_addr.contains("://") && !configured_addr.contains('/'),
        "Business OS MCP address must be a local bind address"
    );
    let client_addr = if let Some(port) = configured_addr.strip_prefix("0.0.0.0:") {
        format!("127.0.0.1:{port}")
    } else if let Some(port) = configured_addr.strip_prefix("[::]:") {
        format!("[::1]:{port}")
    } else {
        configured_addr.to_string()
    };
    let mut config = HashMap::new();
    config.insert(
        "mcp_servers".to_string(),
        serde_json::json!({
            (BUSINESS_OS_MCP_SESSION_SERVER_NAME): {
                "url": format!("http://{client_addr}/mcp"),
                "http_headers": {
                    "Authorization": format!("Bearer {token}"),
                    "X-CTOX-Business-Command-Session": command_session_token
                },
                "enabled": true,
                "required": true,
                // 10 s was too short under business-os.sqlite3 write contention:
                // seven research runs failed on 25.09.2026 with "timed out
                // handshaking with MCP server after 10s" while a browser synced.
                "startup_timeout_sec": 45,
                "tool_timeout_sec": 120,
                "enabled_tools": BUSINESS_OS_MCP_SESSION_TOOLS
            }
        }),
    );
    // Connected ChatGPT Apps are unrelated to this internal command session.
    config.insert("features.apps".to_string(), JsonValue::Bool(false));
    Ok(config)
}

struct NativeMcpStartupExpectation {
    nonce: String,
    token: String,
    endpoint: String,
    context: JsonValue,
}

impl NativeMcpStartupExpectation {
    fn prepare(root: &Path, config: &mut HashMap<String, JsonValue>) -> Result<Self> {
        let servers = config
            .get_mut("mcp_servers")
            .and_then(JsonValue::as_object_mut)
            .context("native Core has no managed MCP configuration")?;
        anyhow::ensure!(
            servers.len() == 1,
            "native Core has foreign configured MCP servers"
        );
        let server = servers
            .get_mut(BUSINESS_OS_MCP_SESSION_SERVER_NAME)
            .context("native Core has no original Business OS MCP server")?;
        let endpoint = server["url"]
            .as_str()
            .context("native MCP has no HTTP endpoint")?
            .to_owned();
        let url = url::Url::parse(&endpoint)?;
        let ip = url
            .host_str()
            .context("native MCP endpoint has no host")?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()?;
        anyhow::ensure!(
            ip.is_loopback()
                && url.scheme() == "http"
                && url.path() == "/mcp"
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "native Core requires the numeric local MCP listener"
        );
        let headers = server["http_headers"]
            .as_object_mut()
            .context("native MCP has no command headers")?;
        let token = headers
            .get("X-CTOX-Business-Command-Session")
            .and_then(JsonValue::as_str)
            .context("native MCP has no command session")?
            .to_owned();
        let context =
            crate::business_os::mcp_channel::verify_internal_command_session_token(root, &token)?;
        let nonce = uuid::Uuid::new_v4().to_string();
        headers.insert(
            crate::business_os::mcp_channel::native_startup::HEADER.into(),
            JsonValue::String(nonce.clone()),
        );
        Ok(Self {
            nonce,
            token,
            endpoint,
            context,
        })
    }
}

fn configure_managed_linux_sandbox(cli_overrides: &mut Vec<(String, toml::Value)>) {
    #[cfg(target_os = "linux")]
    {
        const KEY: &str = "features.use_legacy_landlock";
        cli_overrides.retain(|(key, _)| key != KEY);
        cli_overrides.push((KEY.to_string(), toml::Value::Boolean(true)));
    }

    #[cfg(not(target_os = "linux"))]
    let _ = cli_overrides;
}

#[cfg(target_os = "linux")]
fn push_canonical_readable_directory(
    roots: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    path: &Path,
) {
    let Ok(canonical) = std::fs::canonicalize(path) else {
        return;
    };
    if canonical.is_dir() && seen.insert(canonical.clone()) {
        roots.push(canonical);
    }
}

#[cfg(target_os = "linux")]
fn resolve_symlink_chain(path: &Path) -> Option<PathBuf> {
    const MAX_SYMLINK_DEPTH: usize = 40;

    let mut current = path.to_path_buf();
    let mut seen = HashSet::new();
    for _ in 0..MAX_SYMLINK_DEPTH {
        if !seen.insert(current.clone()) {
            return None;
        }
        match std::fs::read_link(&current) {
            Ok(target) => {
                current = if target.is_absolute() {
                    target
                } else {
                    current.parent()?.join(target)
                };
            }
            Err(_) => return std::fs::canonicalize(current).ok(),
        }
    }
    None
}

#[cfg(target_os = "linux")]
/// Adds the directory that holds the symlink-resolved `ctox` wrapper (for
/// example `~/.local/bin` behind `/usr/local/bin/ctox`).
fn push_ctox_wrapper_target_directory(
    roots: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    wrapper: &Path,
) {
    let Some(target) = resolve_symlink_chain(wrapper) else {
        return;
    };
    if let Some(parent) = target.parent() {
        push_canonical_readable_directory(roots, seen, parent);
    }
}

#[cfg(target_os = "linux")]
fn collect_managed_worker_default_readable_roots(
    exe: Option<&Path>,
    path_entries: &[PathBuf],
    resolv_conf_target: Option<&Path>,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();

    if let Some(exe) = exe {
        if let Ok(canonical_exe) = std::fs::canonicalize(exe) {
            if let Some(bin_dir) = canonical_exe
                .parent()
                .filter(|path| path.file_name().is_some_and(|name| name == "bin"))
            {
                if let Some(release_root) = bin_dir.parent() {
                    push_canonical_readable_directory(&mut roots, &mut seen, release_root);

                    if let Some(ctox_lib_dir) = release_root
                        .parent()
                        .filter(|path| path.file_name().is_some_and(|name| name == "releases"))
                        .and_then(Path::parent)
                    {
                        let current_root = ctox_lib_dir.join("current");
                        let current_is_symlink = std::fs::symlink_metadata(&current_root)
                            .map(|metadata| metadata.file_type().is_symlink())
                            .unwrap_or(false);
                        if current_is_symlink
                            && std::fs::canonicalize(&current_root).ok().as_deref()
                                == Some(release_root)
                            && seen.insert(current_root.clone())
                        {
                            // Preserve the stable alias as well as its canonical release target.
                            // Landlock resolves the alias when installing its path-beneath rule.
                            roots.push(current_root);
                        }
                    }
                }
            }
        }
    }

    for path_entry in path_entries {
        let path_entry = if path_entry.as_os_str().is_empty() {
            Path::new(".")
        } else {
            path_entry.as_path()
        };
        if path_entry.join("ctox").is_file() {
            push_canonical_readable_directory(&mut roots, &mut seen, path_entry);
            push_ctox_wrapper_target_directory(&mut roots, &mut seen, &path_entry.join("ctox"));
        }
    }
    push_canonical_readable_directory(&mut roots, &mut seen, Path::new("/usr/local/bin"));
    // The installer links `/usr/local/bin/ctox` to the per-user wrapper in
    // `~/.local/bin/ctox`, and the daemon's PATH (systemd user unit) usually
    // does not contain `~/.local/bin`. Landlock resolves the symlink, so the
    // wrapper's real directory must be readable too, or every `ctox` execve
    // from a worker fails with EACCES even though `/usr/local/bin` is allowed.
    push_ctox_wrapper_target_directory(&mut roots, &mut seen, Path::new("/usr/local/bin/ctox"));

    for resolver_dir in ["/run/systemd/resolve", "/run/resolvconf"] {
        push_canonical_readable_directory(&mut roots, &mut seen, Path::new(resolver_dir));
    }
    if let Some(target_parent) = resolv_conf_target.and_then(Path::parent) {
        push_canonical_readable_directory(&mut roots, &mut seen, target_parent);
    }

    roots
}

#[cfg(target_os = "linux")]
fn managed_worker_default_readable_roots_for(
    exe: &Path,
    path_entries: &[PathBuf],
    resolv_conf_target: Option<&Path>,
) -> Vec<PathBuf> {
    collect_managed_worker_default_readable_roots(Some(exe), path_entries, resolv_conf_target)
}

#[cfg(target_os = "linux")]
fn managed_worker_default_readable_roots() -> Vec<PathBuf> {
    let exe = std::env::current_exe().ok();
    let path_entries = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    let resolv_conf_target = resolve_symlink_chain(Path::new("/etc/resolv.conf"));
    collect_managed_worker_default_readable_roots(
        exe.as_deref(),
        &path_entries,
        resolv_conf_target.as_deref(),
    )
}

fn managed_worker_sandbox_policy(
    read_only_sandbox: bool,
    additional_writable_roots: &[PathBuf],
    additional_readable_roots: &[PathBuf],
) -> SandboxPolicy {
    if read_only_sandbox {
        return SandboxPolicy::new_read_only_policy();
    }

    let mut policy = SandboxPolicy::new_workspace_write_policy();
    if let SandboxPolicy::WorkspaceWrite {
        writable_roots,
        network_access,
        ..
    } = &mut policy
    {
        *network_access = true;
        *writable_roots = additional_writable_roots
            .iter()
            .filter_map(|path| AbsolutePathBuf::from_absolute_path(path.clone()).ok())
            .collect();
    }

    #[cfg(not(target_os = "linux"))]
    if !additional_readable_roots.is_empty() {
        if let SandboxPolicy::WorkspaceWrite {
            read_only_access, ..
        } = &mut policy
        {
            *read_only_access = ctox_protocol::protocol::ReadOnlyAccess::Restricted {
                include_platform_defaults: true,
                readable_roots: additional_readable_roots
                    .iter()
                    .filter_map(|path| AbsolutePathBuf::from_absolute_path(path.clone()).ok())
                    .collect(),
            };
        }
    }

    #[cfg(target_os = "linux")]
    if let SandboxPolicy::WorkspaceWrite {
        read_only_access,
        exclude_tmpdir_env_var,
        exclude_slash_tmp,
        ..
    } = &mut policy
    {
        let mut readable_paths = managed_worker_default_readable_roots();
        readable_paths.extend_from_slice(additional_readable_roots);
        let mut seen = HashSet::new();
        let readable_roots = readable_paths
            .into_iter()
            .filter_map(|path| AbsolutePathBuf::from_absolute_path(path).ok())
            .filter(|path| seen.insert(path.as_path().to_path_buf()))
            .collect::<Vec<_>>();
        let preview = readable_roots
            .iter()
            .take(3)
            .map(|path| path.as_path().display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "[harness] managed worker readable roots: {} ({preview})",
            readable_roots.len()
        );
        *read_only_access = ctox_protocol::protocol::ReadOnlyAccess::Restricted {
            include_platform_defaults: true,
            readable_roots,
        };
        *exclude_tmpdir_env_var = true;
        *exclude_slash_tmp = true;
    }
    policy
}

fn configure_worker_tool_stack(
    cli_overrides: &mut Vec<(String, toml::Value)>,
    disable_active_tools: bool,
) {
    if disable_active_tools {
        return;
    }
    const REQUIRED_WORKER_OVERRIDES: &[(&str, bool)] = &[
        ("tools.ctox_web", true),
        ("features.multi_agent", false),
        ("features.enable_fanout", false),
        ("features.memory_tool", false),
    ];
    cli_overrides.retain(|(key, _)| {
        !REQUIRED_WORKER_OVERRIDES
            .iter()
            .any(|(required_key, _)| key == required_key)
    });
    cli_overrides.extend(
        REQUIRED_WORKER_OVERRIDES
            .iter()
            .map(|(key, enabled)| ((*key).to_string(), toml::Value::Boolean(*enabled))),
    );
}

fn persistent_worker_thread_name(root: &Path) -> String {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let digest = Sha256::digest(canonical_root.to_string_lossy().as_bytes());
    let suffix = digest[..6].iter().fold(String::new(), |mut value, byte| {
        value.push_str(&format!("{byte:02x}"));
        value
    });
    format!("{CTOX_PERSISTENT_WORKER_THREAD_NAME}-{}", suffix)
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
///
/// A crew persona is identity: it is appended after the complete system prompt
/// and execution contract (never above them) and becomes part of the session
/// contract, so a member change rebuilds the process-local client.
fn compose_base_instructions(
    root: &Path,
    settings: &BTreeMap<String, String>,
    override_prompt: Option<&str>,
    persona: Option<&str>,
) -> Result<String> {
    let base = if let Some(prompt) = override_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.to_string()
    } else {
        let system_prompt = live_context::render_system_prompt(root, settings)
            .context("failed to render CTOX worker system prompt")?;
        format!(
            "{}\n\n{CTOX_DIRECT_SESSION_BASE_INSTRUCTIONS}",
            system_prompt.trim_end()
        )
    };
    Ok(
        match persona.map(str::trim).filter(|value| !value.is_empty()) {
            Some(persona) => format!("{}\n\n{persona}", base.trim_end()),
            None => base,
        },
    )
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TurnRuntimeErrorClass {
    StreamDisconnected,
}

impl TurnRuntimeErrorClass {
    fn from_codex_error_info(error_info: &CodexErrorInfo) -> Option<Self> {
        matches!(
            error_info,
            CodexErrorInfo::ResponseStreamDisconnected { .. }
        )
        .then_some(Self::StreamDisconnected)
    }
}

#[derive(Debug)]
pub(crate) struct TurnRuntimeError {
    class: TurnRuntimeErrorClass,
    message: String,
}

impl TurnRuntimeError {
    pub(crate) fn new(class: TurnRuntimeErrorClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
        }
    }

    pub(crate) fn class(&self) -> TurnRuntimeErrorClass {
        self.class
    }
}

impl std::fmt::Display for TurnRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TurnRuntimeError {}

pub(crate) fn turn_runtime_error_class(error: &anyhow::Error) -> Option<TurnRuntimeErrorClass> {
    error
        .downcast_ref::<TurnRuntimeError>()
        .map(TurnRuntimeError::class)
}

type NativeGuestProviderAuthorization =
    dyn Fn(&str, &crate::channels::NativeProviderCheckpointContract) -> Result<()> + Send + Sync;

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
    thread_config: Option<HashMap<String, JsonValue>>,
    read_only_sandbox: bool,
    additional_writable_roots: Vec<PathBuf>,
    additional_readable_roots: Vec<PathBuf>,
    persistent_worker: bool,
    native_checkpoint_binding: Option<crate::channels::NativeProviderCheckpointBinding>,
    native_capture_thread: Option<Arc<ctox_core::CodexThread>>,
    #[cfg(unix)]
    native_command_session_token: Option<String>,
    #[cfg(unix)]
    native_command_context: Option<JsonValue>,
    #[cfg(unix)]
    native_provider_admission: Option<std::sync::Arc<dyn crate::channels::NativeProviderAdmission>>,
    #[cfg(unix)]
    native_guest_registry: Option<(
        std::sync::Arc<crate::business_os::NativeGuestRegistry>,
        String,
    )>,
    #[cfg(unix)]
    native_guest_execution: Option<crate::business_os::NativeGuestExecution>,
    #[cfg(unix)]
    native_capture_owner: Option<crate::channels::NativeProviderCaptureOwner>,
    /// Set when a turn ended ambiguously (e.g. `turn/start` timed out with
    /// the request still detached server-side). A poisoned session refuses
    /// further turns: reusing it could overlap or steer into the original
    /// turn and duplicate side effects (ctox#21 re-review). Owners must
    /// rebuild the session.
    poisoned: bool,
}

/// Checked native source teardown with current capture-only authority.
/// Private native objects cannot be reconstructed from a manifest or wire claim.
/// Artifact enumeration/export and the protected transport remain separate.
#[cfg(unix)]
pub(crate) struct NativeSessionCapture {
    source: crate::channels::NativeProviderCaptureOwner,
    journal: ctox_core::NativeJournalReader,
    configuration: Option<ctox_core::ThreadConfigSnapshot>,
    session_state: Option<ctox_core::NativeSessionState>,
    execution: crate::business_os::NativeGuestExecution,
    thread_id: String,
    root: PathBuf,
    command_session_token: String,
    command_context: JsonValue,
}

#[cfg(unix)]
impl NativeSessionCapture {
    pub(crate) fn with_current<T>(
        &self,
        capture: impl FnOnce(
            &ctox_sync::contracts::ExecutionSpec,
            &ctox_sync::authority::Ownership,
        ) -> Result<T>,
    ) -> Result<T> {
        self.verify_command_authority()?;
        let captured = self
            .execution
            .with_capture_authority(&self.source, |spec, ownership| {
                anyhow::ensure!(
                    spec.session_id == self.thread_id,
                    "native capture session differs from the actual producer"
                );
                capture(spec, ownership)
            })?;
        self.verify_command_authority()?;
        Ok(captured)
    }

    /// Exact native journal bytes, checked against the actual producer ID.
    /// The result is private capture input; it must not bypass the separate
    /// held publication guard or be labelled provider continuation evidence.
    pub(crate) fn read_journal(
        &self,
        limits: ctox_protocol::portable_journal::PortableJournalLimits,
    ) -> Result<(
        Vec<u8>,
        ctox_protocol::portable_journal::ValidatedPortableJournal,
    )> {
        use ctox_protocol::portable_journal::{
            validate_portable_journal, PortableArtifactRef, PortableJournalExpectation,
            PortableJournalFormat,
        };
        self.with_current(|_, _| {
            let bytes = self.journal.read_bytes(limits.max_bytes)?;
            let expected = PortableJournalExpectation {
                format: PortableJournalFormat::current(),
                session_id: ctox_protocol::ThreadId::from_string(&self.thread_id)
                    .context("native capture producer identity is invalid")?,
            };
            let artifact = PortableArtifactRef {
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                size_bytes: bytes.len() as u64,
            };
            let validated = validate_portable_journal(&bytes, &artifact, &expected, &limits)?;
            Ok((bytes, validated))
        })
    }

    /// Persist private source input before the service retires the worker lease.
    /// The receipt grants no disclosure, target receipt or provider resume.
    pub(crate) fn persist_source_journal(
        self,
    ) -> Result<crate::business_os::NativeSourceJournalReceipt> {
        self.verify_command_authority()?;
        let receipt = self.execution.persist_source_journal(
            &self.source,
            &self.journal,
            self.configuration
                .as_ref()
                .context("native capture has no final Core configuration")?,
            self.session_state
                .as_ref()
                .context("native capture has no final Core session state")?,
        )?;
        self.verify_command_authority()?;
        Ok(receipt)
    }

    fn verify_command_authority(&self) -> Result<()> {
        let current = crate::business_os::mcp_channel::verify_internal_command_session_token(
            &self.root,
            &self.command_session_token,
        )?;
        anyhow::ensure!(
            current == self.command_context,
            "native capture command authority changed"
        );
        Ok(())
    }
}

impl PersistentSession {
    /// Start or resume the normal persistent worker session.
    pub fn start(
        root: &Path,
        settings: &BTreeMap<String, String>,
        persona: Option<&str>,
    ) -> Result<Self> {
        Self::start_with_instructions_and_tool_mode(
            root, settings, None, persona, false, false, false, None, false, true,
        )
    }

    /// Start an isolated task session with only the policy-gated local
    /// Business OS MCP server added to the thread configuration.
    pub(crate) fn start_with_business_os_mcp(
        root: &Path,
        settings: &BTreeMap<String, String>,
        command_session_token: &str,
        persona: Option<&str>,
    ) -> Result<Self> {
        let addr = settings
            .get(BUSINESS_OS_MCP_ADDR_KEY)
            .map(String::as_str)
            .unwrap_or(BUSINESS_OS_MCP_DEFAULT_ADDR);
        let token = crate::business_os::mcp_channel::mcp_operator_auth_token(root)?;
        let thread_config = business_os_mcp_thread_config(addr, &token, command_session_token)?;
        #[cfg(unix)]
        let command_context =
            crate::business_os::mcp_channel::verify_internal_command_session_token(
                root,
                command_session_token,
            )?;
        let mut session = Self::start_with_instructions_and_tool_mode(
            root,
            settings,
            None,
            persona,
            false,
            false,
            false,
            Some(thread_config),
            false,
            false,
        )?;
        #[cfg(unix)]
        {
            session.native_command_context = Some(command_context);
        }
        Ok(session)
    }

    /// A native guest producer must install its real admission owner before a
    /// turn. Ordinary queue workflows do not acquire a guest permit here.
    #[cfg(unix)]
    pub(crate) fn require_native_provider_admission(
        &mut self,
        admission: std::sync::Arc<dyn crate::channels::NativeProviderAdmission>,
    ) -> Result<()> {
        anyhow::ensure!(
            self.native_checkpoint_binding.is_some() && !self.poisoned,
            "native admission requires a fresh durable account-bound guest session"
        );
        anyhow::ensure!(
            self.native_provider_admission.is_none(),
            "native admission owner already installed"
        );
        self.native_provider_admission = Some(admission);
        Ok(())
    }

    /// Construct the actual native guest Core with pinned account and verified
    /// command provenance. Protected targets load their original Core UUID from
    /// the retained receiver; fresh guests start a new durable session.
    /// This creates no model turn and does not itself grant Raft/guest execution.
    #[cfg(unix)]
    pub(crate) fn start_native_guest_with_business_os_mcp(
        root: &Path,
        settings: &BTreeMap<String, String>,
        command_session_token: &str,
        persona: Option<&str>,
        registry: std::sync::Arc<crate::business_os::NativeGuestRegistry>,
        guest_id: &str,
    ) -> Result<Self> {
        let context = crate::business_os::mcp_channel::verify_internal_command_session_token(
            root,
            command_session_token,
        )?;
        anyhow::ensure!(
            registry.select_command_context(root, &context)?.as_deref() == Some(guest_id),
            "native guest differs from the current command assignment"
        );
        registry.require_live_transport()?;
        let addr = settings
            .get(BUSINESS_OS_MCP_ADDR_KEY)
            .map(String::as_str)
            .unwrap_or(BUSINESS_OS_MCP_DEFAULT_ADDR);
        let token = crate::business_os::mcp_channel::mcp_operator_auth_token(root)?;
        let config = business_os_mcp_thread_config(addr, &token, command_session_token)?;
        let account_registry = Arc::clone(&registry);
        let account_guest = guest_id.to_owned();
        let account_context = context.clone();
        let account_authority: Arc<NativeGuestProviderAuthorization> =
            Arc::new(move |model, contract| {
                account_registry.authorize_provider_start(
                    &account_guest,
                    &account_context,
                    model,
                    contract,
                )
            });
        let mut session = Self::start_with_native_mode(
            root,
            settings,
            None,
            persona,
            false,
            false,
            false,
            Some(config),
            false,
            false,
            Some(account_authority),
            Some((registry.clone(), guest_id.to_owned(), context.clone())),
        )?;
        let current = crate::business_os::mcp_channel::verify_internal_command_session_token(
            root,
            command_session_token,
        )?;
        anyhow::ensure!(
            current == context,
            "native guest command authority changed during startup"
        );
        anyhow::ensure!(
            registry.select_command_context(root, &current)?.as_deref() == Some(guest_id),
            "native guest command assignment changed during startup"
        );
        registry.require_live_transport()?;
        session.native_command_context = Some(current);
        session.native_command_session_token = Some(command_session_token.to_owned());
        session.require_native_provider_admission(registry.admission(guest_id)?)?;
        // The service host already owns the guarded source on its exact peer.
        // Cloning this registry does not prolong a stopped native transport.
        session.native_guest_registry = Some((registry, guest_id.to_owned()));
        Ok(session)
    }

    /// Start a fresh worker session that cannot resume the process-wide
    /// persistent harness thread.
    pub(crate) fn start_isolated(
        root: &Path,
        settings: &BTreeMap<String, String>,
        persona: Option<&str>,
    ) -> Result<Self> {
        Self::start_with_instructions_and_tool_mode(
            root, settings, None, persona, false, false, false, None, false, false,
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
        persona: Option<&str>,
    ) -> Result<Self> {
        Self::start_with_instructions_and_tool_mode(
            root,
            settings,
            base_instructions,
            persona,
            false,
            false,
            true,
            None,
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
        persona: Option<&str>,
    ) -> Result<bool> {
        let base_instructions = compose_base_instructions(root, settings, None, persona)?;
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

    pub(crate) fn set_turn_file_system_roots(
        &mut self,
        writable_roots: &[PathBuf],
        readable_roots: &[PathBuf],
    ) {
        self.additional_writable_roots = writable_roots
            .iter()
            .map(|path| path.canonicalize().unwrap_or_else(|_| path.clone()))
            .collect();
        self.additional_readable_roots = readable_roots
            .iter()
            .map(|path| path.canonicalize().unwrap_or_else(|_| path.clone()))
            .collect();
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
            None,
            disable_compaction,
            disable_compaction,
            false,
            None,
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
            None,  // persona: reviewers never carry a crew identity
            true,  // disable_compaction
            false, // disable_active_tools
            true,  // disable_mcp_servers
            None,  // thread_config
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
            None,  // persona: reviewers never carry a crew identity
            true,  // disable_compaction
            true,  // disable_active_tools
            true,  // disable_mcp_servers
            None,  // thread_config
            true,  // read_only_sandbox
            false, // persistent_worker
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn start_with_instructions_and_tool_mode(
        root: &Path,
        settings: &BTreeMap<String, String>,
        base_instructions: Option<&str>,
        persona: Option<&str>,
        disable_compaction: bool,
        disable_active_tools: bool,
        disable_mcp_servers: bool,
        thread_config: Option<HashMap<String, JsonValue>>,
        read_only_sandbox: bool,
        persistent_worker: bool,
    ) -> Result<Self> {
        Self::start_with_native_mode(
            root,
            settings,
            base_instructions,
            persona,
            disable_compaction,
            disable_active_tools,
            disable_mcp_servers,
            thread_config,
            read_only_sandbox,
            persistent_worker,
            None,
            #[cfg(unix)]
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn start_with_native_mode(
        root: &Path,
        settings: &BTreeMap<String, String>,
        base_instructions: Option<&str>,
        persona: Option<&str>,
        disable_compaction: bool,
        disable_active_tools: bool,
        disable_mcp_servers: bool,
        thread_config: Option<HashMap<String, JsonValue>>,
        read_only_sandbox: bool,
        persistent_worker: bool,
        native_guest_authorization: Option<Arc<NativeGuestProviderAuthorization>>,
        #[cfg(unix)] native_guest_start: Option<(
            Arc<crate::business_os::NativeGuestRegistry>,
            String,
            JsonValue,
        )>,
    ) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .context("failed to start tokio runtime")?;

        let composed_base_instructions =
            compose_base_instructions(root, settings, base_instructions, persona)?;
        let start_result = rt.block_on(async {
            Self::start_client_and_thread(
                root,
                root,
                settings,
                &composed_base_instructions,
                disable_active_tools,
                disable_mcp_servers,
                thread_config.as_ref(),
                read_only_sandbox,
                persistent_worker,
                native_guest_authorization.as_deref(),
                #[cfg(unix)]
                native_guest_start.as_ref(),
            )
            .await
        });
        let (
            client,
            thread_id,
            cwd,
            seq,
            model,
            model_provider,
            api_provider,
            reasoning_effort,
            native_checkpoint_binding,
            native_capture_thread,
        ) = match start_result {
            Ok(started) => started,
            Err(err) => return Err(err),
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
            thread_config,
            read_only_sandbox,
            additional_writable_roots: Vec::new(),
            additional_readable_roots: Vec::new(),
            persistent_worker,
            native_checkpoint_binding,
            native_capture_thread,
            #[cfg(unix)]
            native_command_session_token: None,
            #[cfg(unix)]
            native_command_context: None,
            #[cfg(unix)]
            native_provider_admission: None,
            #[cfg(unix)]
            native_guest_registry: None,
            #[cfg(unix)]
            native_guest_execution: None,
            #[cfg(unix)]
            native_capture_owner: None,
            poisoned: false,
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
        self.run_turn_inner_with_context(prompt, None, timeout, exact_prompt_preflight, None)
    }

    pub(crate) fn run_turn_inner_with_context(
        &mut self,
        prompt: &str,
        developer_instructions: Option<&str>,
        timeout: Option<Duration>,
        exact_prompt_preflight: Option<ExactPromptTokenCount>,
        required_initial_tool: Option<&str>,
    ) -> Result<String> {
        let mut ignore_progress = |_event: &JsonValue| {};
        self.run_turn_inner_with_context_and_progress(
            prompt,
            developer_instructions,
            timeout,
            exact_prompt_preflight,
            &mut ignore_progress,
            required_initial_tool,
        )
    }

    pub(crate) fn run_turn_inner_with_context_and_progress(
        &mut self,
        prompt: &str,
        developer_instructions: Option<&str>,
        timeout: Option<Duration>,
        exact_prompt_preflight: Option<ExactPromptTokenCount>,
        progress: &mut dyn FnMut(&JsonValue),
        required_initial_tool: Option<&str>,
    ) -> Result<String> {
        self.run_turn_inner_with_context_progress_and_lease(
            prompt,
            developer_instructions,
            timeout,
            exact_prompt_preflight,
            progress,
            required_initial_tool,
            None,
        )
    }

    pub(crate) fn run_turn_inner_with_context_progress_and_lease(
        &mut self,
        prompt: &str,
        developer_instructions: Option<&str>,
        timeout: Option<Duration>,
        exact_prompt_preflight: Option<ExactPromptTokenCount>,
        progress: &mut dyn FnMut(&JsonValue),
        required_initial_tool: Option<&str>,
        queue_turn_lease: Option<&crate::channels::QueueTurnLeaseFence>,
    ) -> Result<String> {
        anyhow::ensure!(
            !self.poisoned,
            "session is poisoned by an earlier ambiguous turn outcome; rebuild the session"
        );
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
        let thread_config = self.thread_config.clone();
        let read_only_sandbox = self.read_only_sandbox;
        let additional_writable_roots = self.additional_writable_roots.clone();
        let additional_readable_roots = self.additional_readable_roots.clone();
        let persistent_worker = self.persistent_worker;
        #[cfg(unix)]
        let native_command_context = self.native_command_context.clone();
        #[cfg(unix)]
        let native_provider_admission = self.native_provider_admission.clone();
        #[cfg(unix)]
        let native_guest_registry = self.native_guest_registry.clone();
        let required_initial_tool = required_initial_tool.map(str::to_string);
        self.ctx_log.log(
            "turn_request",
            &format!("\"prompt_len\":{},\"timeout\":{:?}", prompt.len(), timeout),
        );

        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session runtime already shut down"))?;
        let native_checkpoint_binding = self.native_checkpoint_binding.clone();
        #[cfg(unix)]
        let native_command_session_token = self.native_command_session_token.clone();
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
                thread_config.as_ref(),
                read_only_sandbox,
                &additional_writable_roots,
                &additional_readable_roots,
                persistent_worker,
                exact_prompt_preflight,
                progress,
                required_initial_tool.as_deref(),
                queue_turn_lease,
                native_checkpoint_binding.as_ref(),
                #[cfg(unix)]
                native_command_session_token.as_deref(),
                #[cfg(unix)]
                native_command_context.as_ref(),
                #[cfg(unix)]
                native_provider_admission.as_ref(),
                #[cfg(unix)]
                native_guest_registry.as_ref(),
                #[cfg(unix)]
                &mut self.native_guest_execution,
                #[cfg(unix)]
                &mut self.native_capture_owner,
            )
            .await
        });
        // Adopt a rotated thread id so follow-up turns in this session keep
        // using the live thread.
        self.thread_id = thread_id;
        // Latch ambiguous outcomes: callers that swallow turn errors
        // (continuity refresh, summarizer) must not reuse this session.
        if let Err(err) = &result {
            if err.downcast_ref::<SessionPoisoned>().is_some() {
                self.poisoned = true;
            }
        }

        result
    }

    /// Finish owned client cleanup and return its checked shutdown result.
    /// Forced teardown cannot certify a quiescent provider checkpoint.
    pub fn shutdown(mut self) -> Result<()> {
        let has_owners = self.runtime.is_some() && self.client.is_some();
        self.shutdown_inner("shutting down")?;
        anyhow::ensure!(
            has_owners,
            "persistent session shutdown ownership is missing"
        );
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn is_native_guest(&self) -> bool {
        self.native_guest_registry.is_some()
    }

    /// Consume the actual native producer. No capture authority escapes a
    /// failed/forced teardown, stale worker/account/policy, or ambiguous turn.
    /// This does not export artifacts or certify provider continuation.
    #[cfg(unix)]
    pub(crate) fn quiesce_native_capture(mut self) -> Result<NativeSessionCapture> {
        anyhow::ensure!(
            !self.poisoned,
            "ambiguous native turn requires reconciliation"
        );
        anyhow::ensure!(
            self.runtime.is_some() && self.client.is_some(),
            "native capture requires both actual shutdown owners"
        );
        anyhow::ensure!(
            self.native_capture_owner.is_some(),
            "native turn has not retired to capture authority"
        );
        // This API owns a synchronous runtime. A nested runtime cannot supply
        // checked teardown; the existing shutdown path performs bounded cleanup.
        if tokio::runtime::Handle::try_current().is_ok() {
            let _ = self.shutdown_inner("rejecting asynchronous native capture");
            anyhow::bail!("native capture requires a synchronous owner");
        }
        let actual_thread = self
            .native_capture_thread
            .take()
            .context("native capture has no retained actual Core Session")?;
        let journal = self
            .runtime
            .as_ref()
            .expect("checked runtime owner")
            .block_on(async {
                tokio::time::timeout(
                    Duration::from_secs(DIRECT_SESSION_CONTROL_REQUEST_TIMEOUT_SECS),
                    actual_thread.retain_native_journal(),
                )
                .await
                .context("native journal retention timed out")?
                .context("actual native journal could not be retained")
            });
        let journal = match journal {
            Ok(retained) => retained,
            Err(error) => {
                let _ = self.shutdown_inner("failed native journal retention");
                return Err(error);
            }
        };
        let mut capture = NativeSessionCapture {
            journal,
            configuration: None,
            session_state: None,
            source: self
                .native_capture_owner
                .take()
                .context("native turn has not retired to capture authority")?,
            execution: self
                .native_guest_execution
                .take()
                .context("native capture has no actual admitted guest execution")?,
            thread_id: self.thread_id.clone(),
            root: self.root.clone(),
            command_session_token: self
                .native_command_session_token
                .take()
                .context("native capture has no retained signed command authorization")?,
            command_context: self
                .native_command_context
                .take()
                .context("native capture has no verified command context")?,
        };
        let before = capture.with_current(|_, _| Ok(()));
        let shutdown = self.shutdown_inner_with("quiescing native capture", |runtime| {
            runtime.block_on(async {
                tokio::time::timeout(
                    Duration::from_secs(DIRECT_SESSION_CONTROL_REQUEST_TIMEOUT_SECS),
                    actual_thread.capture_native_state(),
                )
                .await
                .context("final native state capture timed out")?
                .context("final native state capture failed")
            })
        });
        before?;
        let (configuration, session_state) =
            shutdown?.context("native state has no shutdown owner")?;
        capture.configuration = Some(configuration);
        capture.session_state = Some(session_state);
        capture.with_current(|_, _| Ok(()))?;
        // Writer receipt alone cannot certify identity, syntax or bounded size.
        // A malformed/missing/changed journal never returns a capture owner.
        capture.read_journal(ctox_protocol::portable_journal::PortableJournalLimits::default())?;
        Ok(capture)
    }

    // --- Internal async helpers ---

    async fn start_client_and_thread(
        root: &Path,
        cwd: &Path,
        settings: &BTreeMap<String, String>,
        base_instructions: &str,
        disable_active_tools: bool,
        disable_mcp_servers: bool,
        thread_config: Option<&HashMap<String, JsonValue>>,
        read_only_sandbox: bool,
        persistent_worker: bool,
        native_guest_authorization: Option<&NativeGuestProviderAuthorization>,
        #[cfg(unix)] native_guest_start: Option<&(
            Arc<crate::business_os::NativeGuestRegistry>,
            String,
            JsonValue,
        )>,
    ) -> Result<(
        InProcessAppServerClient,
        String,
        PathBuf,
        RequestIdSeq,
        String,
        Option<String>,
        Option<String>,
        Option<ReasoningEffort>,
        Option<crate::channels::NativeProviderCheckpointBinding>,
        Option<Arc<ctox_core::CodexThread>>,
    )> {
        let native_guest = native_guest_authorization.is_some();
        // Fresh per actual Core construction; never reused from a previous session.
        let mut native_thread_config =
            native_guest.then(|| thread_config.cloned().unwrap_or_default());
        let native_mcp_startup = native_thread_config
            .as_mut()
            .map(|config| NativeMcpStartupExpectation::prepare(root, config))
            .transpose()?;
        let thread_config = native_thread_config.as_ref().or(thread_config);
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
        #[cfg(unix)]
        let cwd = if let Some((registry, guest, context)) = native_guest_start {
            registry
                .continuation_workspace(guest, context)?
                .unwrap_or_else(|| cwd.to_path_buf())
        } else {
            cwd.to_path_buf()
        };
        #[cfg(not(unix))]
        let cwd = cwd.to_path_buf();

        let codex_home =
            find_codex_home().map_err(|err| anyhow::anyhow!("find_codex_home: {err}"))?;
        let use_chatgpt_subscription_auth =
            use_openai_chatgpt_subscription_auth(settings, selected_api_provider.as_deref());
        let use_provider_subscription_proxy = selected_api_provider
            .as_deref()
            .is_some_and(|provider| provider.eq_ignore_ascii_case("ctox_subscription"));

        let selected_api_key_name = selected_api_provider.as_deref().map(|provider| {
            runtime_state::api_key_env_var_for_provider_with_env_map(provider, settings)
        });
        let api_key = match selected_api_key_name {
            Some(key)
                if use_chatgpt_subscription_auth && key.eq_ignore_ascii_case("OPENAI_API_KEY") =>
            {
                None
            }
            Some(_) if use_provider_subscription_proxy => None,
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
        if native_guest {
            anyhow::ensure!(
                config_toml.chatgpt_base_url.as_deref().is_none_or(
                    |url| url.trim_end_matches('/') == "https://chatgpt.com/backend-api"
                ),
                "native account-bound guest cannot use a replacement ChatGPT endpoint"
            );
        }
        if use_chatgpt_subscription_auth {
            let _ = restore_chatgpt_subscription_auth_from_instance(
                root,
                &codex_home,
                auth_credentials_store_mode,
            );
        }

        let auth_manager = if native_guest {
            anyhow::ensure!(
                use_chatgpt_subscription_auth && !use_provider_subscription_proxy,
                "native guest requires an account-bound direct provider; proxy account selection is unresolved"
            );
            AuthManager::from_account_bound_storage(
                codex_home.clone(),
                auth_credentials_store_mode,
            )?
        } else if let Some(ref key) = api_key {
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
        let api_provider = if native_guest {
            // Native account-bound ChatGPT uses the canonical authenticated
            // harness route, never a no-auth CTOX API/proxy adapter.
            None
        } else {
            super::turn_loop::resolve_api_model_provider_spec(
                &model,
                settings,
                resolved_runtime.as_ref(),
            )
        };
        let local_provider =
            super::turn_loop::resolve_local_model_provider_spec(resolved_runtime.as_ref());
        if api_provider.is_some()
            && local_provider.is_none()
            && api_key.is_none()
            && !use_chatgpt_subscription_auth
            && !use_provider_subscription_proxy
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
        let selected_provider_id = if native_guest {
            Some("openai".to_owned())
        } else {
            local_provider
                .as_ref()
                .map(|provider| provider.provider_id.to_string())
                .or_else(|| {
                    api_provider
                        .as_ref()
                        .map(|provider| provider.provider_id.to_string())
                })
        };
        let tracking_api_provider =
            if use_chatgpt_subscription_auth || use_provider_subscription_proxy {
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
            // Completion review is a server-owned, independent read-only
            // execution profile. It is not a child agent and cannot write
            // anywhere, including the authoritative workspace/runtime tree.
            sandbox_mode: Some(if read_only_sandbox {
                SandboxMode::ReadOnly
            } else {
                SandboxMode::WorkspaceWrite
            }),
            include_apply_patch_tool: Some(true),
            ephemeral: Some(!native_guest),
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
        // This must be part of the canonical override set passed into the
        // in-process app server. Mutating only the initially built Config is
        // insufficient because per-turn configs and spawned threads rebuild
        // from these overrides.
        configure_managed_linux_sandbox(&mut cli_overrides);
        configure_worker_tool_stack(&mut cli_overrides, disable_active_tools);
        if native_guest {
            super::session_continuity::constrain_native_guest_startup(&mut cli_overrides);
            // Both fresh thread/start and protected original-session load use
            // these canonical overrides; the latter bypasses thread/start config.
            let native_config = native_thread_config
                .as_ref()
                .context("native MCP configuration missing")?;
            cli_overrides.retain(|(key, _)| key != "mcp_servers" && key != "features.apps");
            cli_overrides.push((
                "mcp_servers".into(),
                serde_json::from_value::<toml::Value>(
                    native_config
                        .get("mcp_servers")
                        .context("native MCP servers missing")?
                        .clone(),
                )?,
            ));
            cli_overrides.push(("features.apps".into(), toml::Value::Boolean(false)));
        }
        let config = ConfigBuilder::default()
            .cli_overrides(cli_overrides.clone())
            .harness_overrides(overrides)
            .cloud_requirements(cloud_requirements.clone())
            .build()
            .await
            .map_err(|err| anyhow::anyhow!("config build: {err}"))?;
        let native_checkpoint_binding = if native_guest {
            anyhow::ensure!(
                local_provider.is_none(),
                "native guest account is not a local model route"
            );
            anyhow::ensure!(
                config.model_provider.requires_openai_auth
                    && config.chatgpt_base_url.trim_end_matches('/')
                        == "https://chatgpt.com/backend-api"
                    && config.model_provider.wire_api.to_string() == "responses"
                    && config.model_provider.base_url.is_none()
                    && config.model_provider.transport_endpoint.is_none()
                    && config.model_provider.env_key.is_none()
                    && config.model_provider.experimental_bearer_token.is_none(),
                "native provider route must use its pinned direct account"
            );
            Some(
                crate::channels::NativeProviderCheckpointBinding::from_pinned_auth(
                    auth_manager.clone(),
                    &config.model_provider_id,
                )?,
            )
        } else {
            None
        };
        if let (Some(binding), Some(authorize)) =
            (&native_checkpoint_binding, native_guest_authorization)
        {
            binding.with_current_contract(|contract| authorize(&model, contract))?;
        }
        #[cfg(unix)]
        let native_resume = match (native_guest_start, &native_checkpoint_binding) {
            (Some((registry, guest, context)), Some(binding)) => {
                binding.with_current_contract(|contract| {
                    registry.prepare_core_resume(guest, context, &model, contract, &cwd)
                })?
            }
            _ => None,
        };
        let config = Arc::new(config);
        let session_source = SessionSource::Exec;
        let thread_manager = Arc::new(ThreadManager::new(
            config.as_ref(),
            auth_manager.clone(),
            session_source.clone(),
            CollaborationModesConfig::default(),
        ));

        let start_args = InProcessClientStartArgs {
            arg0_paths: direct_session_arg0_paths(),
            config: config.clone(),
            cli_overrides: cli_overrides.clone(),
            loader_overrides: Default::default(),
            cloud_requirements,
            auth_manager: Some(auth_manager.clone()),
            thread_manager: Some(thread_manager.clone()),
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
        let client = InProcessAppServerClient::start(start_args)
            .await
            .map_err(|err| anyhow::anyhow!("client start: {err}"))?;
        eprintln!("[ctox direct-session] client started");

        let mut seq = RequestIdSeq::new();
        let canonical_cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
        let persistent_thread_name = persistent_worker.then(|| persistent_worker_thread_name(root));
        let spec = SessionThreadSpec {
            model: &model,
            model_provider: selected_provider_id.as_deref(),
            cwd: &canonical_cwd,
            base_instructions,
            disable_active_tools,
            disable_mcp_servers,
            thread_config,
            persistent_worker,
            durable_guest: native_guest,
            persistent_thread_name: persistent_thread_name.as_deref(),
        };
        let timeouts = production_session_control_timeouts();
        #[cfg(unix)]
        let restored_thread_id = if let Some(resume) = native_resume {
            Some(
                resume
                    .load(
                        &thread_manager,
                        config.as_ref().clone(),
                        auth_manager.clone(),
                    )
                    .await?
                    .thread_id
                    .to_string(),
            )
        } else {
            None
        };
        #[cfg(not(unix))]
        let restored_thread_id: Option<String> = None;
        let thread_id = if let Some(original) = restored_thread_id {
            original
        } else {
            bind_session_thread(&client, &mut seq, &spec, &timeouts).await?
        };
        if let (Some(binding), Some(authorize)) =
            (&native_checkpoint_binding, native_guest_authorization)
        {
            binding.with_current_contract(|contract| authorize(&model, contract))?;
        }
        let native_capture_thread = if native_guest {
            let actual_id = ctox_protocol::ThreadId::from_string(&thread_id)
                .context("native producer returned an invalid thread identity")?;
            let actual_thread = thread_manager
                .get_thread(actual_id)
                .await
                .context("native producer has no actual loaded Core Session")?;
            // Actual object provenance before the first submission; no JSON or
            // renderer field registers this ledger, and it grants no execution.
            actual_thread.register_native_source_factory()?;
            if let Some(startup) = &native_mcp_startup {
                actual_thread
                    .reconcile_native_mcp_startup(|snapshot| {
                        let verified = (|| -> Result<()> {
                            if let (Some(binding), Some(authorize)) =
                                (&native_checkpoint_binding, native_guest_authorization)
                            {
                                binding.with_current_contract(|contract| {
                                    authorize(&model, contract)
                                })?;
                            }
                            crate::business_os::mcp_channel::native_startup::verify(
                                root,
                                &startup.nonce,
                                &startup.token,
                                &startup.endpoint,
                                &startup.context,
                                snapshot,
                            )?;
                            if let (Some(binding), Some(authorize)) =
                                (&native_checkpoint_binding, native_guest_authorization)
                            {
                                binding.with_current_contract(|contract| {
                                    authorize(&model, contract)
                                })?;
                            }
                            Ok(())
                        })();
                        verified.map_err(|error| std::io::Error::other(error.to_string()))
                    })
                    .await?;
            }
            if let (Some(binding), Some(authorize)) =
                (&native_checkpoint_binding, native_guest_authorization)
            {
                binding.with_current_contract(|contract| authorize(&model, contract))?;
            }
            Some(actual_thread)
        } else {
            None
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
            native_checkpoint_binding,
            native_capture_thread,
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
        thread_config: Option<&HashMap<String, JsonValue>>,
        read_only_sandbox: bool,
        additional_writable_roots: &[PathBuf],
        additional_readable_roots: &[PathBuf],
        persistent_worker: bool,
        exact_prompt_preflight: Option<ExactPromptTokenCount>,
        progress: &mut dyn FnMut(&JsonValue),
        required_initial_tool: Option<&str>,
        queue_turn_lease: Option<&crate::channels::QueueTurnLeaseFence>,
        native_checkpoint_binding: Option<&crate::channels::NativeProviderCheckpointBinding>,
        #[cfg(unix)] native_command_session_token: Option<&str>,
        #[cfg(unix)] native_command_context: Option<&JsonValue>,
        #[cfg(unix)] native_provider_admission: Option<
            &std::sync::Arc<dyn crate::channels::NativeProviderAdmission>,
        >,
        #[cfg(unix)] native_guest_registry: Option<&(
            std::sync::Arc<crate::business_os::NativeGuestRegistry>,
            String,
        )>,
        #[cfg(unix)] native_guest_execution: &mut Option<crate::business_os::NativeGuestExecution>,
        #[cfg(unix)] native_capture_owner: &mut Option<crate::channels::NativeProviderCaptureOwner>,
    ) -> Result<String> {
        let native_guest = native_checkpoint_binding.is_some();
        let lease_reader = queue_turn_lease
            .map(|fence| fence.open_reader())
            .transpose()?;
        if let (Some(fence), Some(reader)) = (queue_turn_lease, lease_reader.as_ref()) {
            anyhow::ensure!(
                fence.still_owned(reader)?,
                "queue turn cancelled before model invocation: native lease revoked"
            );
        }
        // Reuse the session's thread across turns. The previous fresh-thread-
        // per-turn workaround ("the thread may not accept new TurnStart
        // requests") has no backing mechanism in the current fork: turn_start
        // is load_thread + Op::UserInput with no completed-thread rejection,
        // and ephemeral threads stay registered in the in-memory manager.
        // Reuse makes a slice (main turn + continuity refreshes) one thread,
        // sends the base instructions once instead of four times, and is the
        // first building block of the long-lived worker session. Persistent
        // workers fail closed on a rejected turn/start instead of rotating.
        // Isolated sessions still rotate once if TurnStart reports the thread
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

        // TurnStart on the bound session thread. Persistent workers refuse
        // replacement threads; isolated sessions still rotate once on a
        // definitive server rejection.
        let turn_start_params = |thread_id: &str| TurnStartParams {
            thread_id: thread_id.to_string(),
            input: vec![UserInput::Text {
                text: prompt.to_string(),
                text_elements: Vec::new(),
            }
            .into()],
            required_initial_tool: required_initial_tool.map(str::to_string),
            developer_instructions: developer_instructions.map(str::to_string),
            cwd: Some(cwd.to_path_buf()),
            approval_policy: Some(AskForApproval::Never.into()),
            approvals_reviewer: None,
            sandbox_policy: Some(
                managed_worker_sandbox_policy(
                    read_only_sandbox,
                    additional_writable_roots,
                    additional_readable_roots,
                )
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
        let persistent_thread_name = persistent_worker.then(|| persistent_worker_thread_name(root));
        let spec = SessionThreadSpec {
            model,
            model_provider,
            cwd,
            base_instructions,
            disable_active_tools,
            disable_mcp_servers,
            thread_config,
            persistent_worker,
            durable_guest: native_guest,
            persistent_thread_name: persistent_thread_name.as_deref(),
        };
        let timeouts = production_session_control_timeouts();
        // Context compaction/tokenization may take time. A cancellation
        // committed during that preparation must not start a model turn.
        if let (Some(fence), Some(reader)) = (queue_turn_lease, lease_reader.as_ref()) {
            anyhow::ensure!(
                fence.still_owned(reader)?,
                "queue turn cancelled before turn start: native lease revoked"
            );
        }
        #[cfg(unix)]
        let execution = queue_turn_lease.and_then(|fence| fence.execution.as_ref());
        #[cfg(unix)]
        let mut provider_owner = execution
            .map(|execution| {
                crate::channels::NativeProviderTurnOwner::prepare_with_checkpoint(
                    execution,
                    session_thread_id,
                    model,
                    model_provider,
                    api_provider,
                    native_command_context,
                    native_checkpoint_binding,
                )
            })
            .transpose()?;
        // Register before start so an early sensitive MCP call fails closed
        // until bind_turn has the actual TurnStart response. Only a guest
        // admission installs this native path; ordinary MCP stays unchanged.
        #[cfg(unix)]
        let _native_mcp_registration = if let Some(admission) = native_provider_admission {
            let owner = provider_owner
                .as_ref()
                .context("native MCP dispatch requires the actual worker/provider owner")?;
            Some(native_guest_mcp::register(client, owner, std::sync::Arc::clone(admission)).await?)
        } else {
            None
        };
        #[cfg(target_os = "linux")]
        let mut source_boot_ready = None;
        #[cfg(unix)]
        if let Some(admission) = native_provider_admission {
            let (registry, guest_id) = native_guest_registry.ok_or_else(|| {
                SessionPoisoned("native guest has no actual lifecycle registry".into())
            })?;
            if native_guest_execution.is_some() {
                return Err(SessionPoisoned(
                    "native guest execution already bound; continuation requires lifecycle reconciliation".into()
                ).into());
            }
            let provider = provider_owner.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "guest provider admission requires the actual native worker execution"
                )
            })?;
            let token = native_command_session_token.ok_or_else(|| {
                anyhow::anyhow!("native guest has no retained command authorization")
            })?;
            let current = crate::business_os::mcp_channel::verify_internal_command_session_token(
                root, token,
            )?;
            anyhow::ensure!(
                native_command_context == Some(&current),
                "native guest command authority changed"
            );
            if let Err(error) = provider
                .binding()
                .admit_before_start(admission.as_ref())
                .await
            {
                return Err(SessionPoisoned(format!(
                    "native guest admission requires reconciliation: {error}"
                ))
                .into());
            }
            let current =
                crate::business_os::mcp_channel::verify_internal_command_session_token(root, token)
                    .map_err(|error| {
                        SessionPoisoned(format!(
                "native guest authority changed after admission; reconciliation required: {error}"
            ))
                    })?;
            if native_command_context != Some(&current) {
                return Err(SessionPoisoned(
                    "native guest command authority changed after admission; reconciliation required".into()
                ).into());
            }
            let execution = registry
                .bind_admitted_execution(provider.binding(), guest_id)
                .await
                .map_err(|error| {
                    SessionPoisoned(format!(
                        "native guest controller binding requires reconciliation: {error}"
                    ))
                })?;
            let current =
                crate::business_os::mcp_channel::verify_internal_command_session_token(root, token)
                    .map_err(|error| {
                        SessionPoisoned(format!(
                            "native guest authority changed after controller binding: {error}"
                        ))
                    })?;
            if native_command_context != Some(&current) {
                return Err(SessionPoisoned(
                    "native guest command changed before TurnStart; reconciliation required".into(),
                )
                .into());
            }
            // Observation handle only. Actual guest operations independently
            // revalidate provider/account/policy/controller and quorum ownership.
            #[cfg(target_os = "linux")]
            {
                source_boot_ready =
                    execution
                        .start_current_guest_turn()
                        .await
                        .map_err(|error| {
                            SessionPoisoned(format!(
                                "native guest preparation requires reconciliation: {error}"
                            ))
                        })?;
            }
            // Machine preparation/boot awaits cannot preserve old command authority.
            let after_boot =
                crate::business_os::mcp_channel::verify_internal_command_session_token(
                    root, token,
                )?;
            anyhow::ensure!(
                native_command_context == Some(&after_boot),
                "native guest command changed during machine boot"
            );
            execution.verify_turn_workspace(cwd)?;
            *native_guest_execution = Some(execution);
        }
        // Only the explicitly native-admitted guest lane forbids isolated
        // fallback. Ordinary isolated sessions retain their existing rotation.
        #[cfg(unix)]
        let prepared_guest = native_provider_admission.is_some();
        #[cfg(not(unix))]
        let prepared_guest = false;
        let turn_resp: TurnStartResponse = if prepared_guest {
            super::session_continuity::start_prepared_turn(
                client,
                seq,
                session_thread_id,
                turn_start_params,
                &spec,
                &timeouts,
            )
            .await?
        } else {
            start_bound_turn(
                client,
                seq,
                session_thread_id,
                turn_start_params,
                &spec,
                &timeouts,
            )
            .await?
        };
        let thread_id = session_thread_id.clone();
        let turn_id = turn_resp.turn.id;
        #[cfg(unix)]
        {
            // A normal isolated server rejection can rotate the actual thread.
            // It has no guest admission; replace its observation before binding
            // the actual turn. An admitted guest can never take this branch.
            let binding_result = (|| -> Result<()> {
                if let Some(owner) = provider_owner.as_ref() {
                    let prepared = owner.binding().with_live_provider(|facts, _| {
                        Ok(facts.provider_session_id == thread_id)
                    })?;
                    if !prepared {
                        anyhow::ensure!(!prepared_guest, "admitted provider session rotated");
                        drop(provider_owner.take());
                        provider_owner = Some(
                            crate::channels::NativeProviderTurnOwner::prepare_with_checkpoint(
                                execution.expect("provider owner requires execution"),
                                &thread_id,
                                model,
                                model_provider,
                                api_provider,
                                native_command_context,
                                native_checkpoint_binding,
                            )?,
                        );
                    }
                }
                if let Some(owner) = provider_owner.as_ref() {
                    owner.bind_turn(&thread_id, &turn_id)?;
                }
                Ok(())
            })();
            if let Err(error) = binding_result {
                let terminal =
                    interrupt_cancelled_queue_turn(client, seq, &thread_id, &turn_id).await;
                return Err(SessionPoisoned(format!(
                    "actual provider turn binding failed: {error}; terminal_observed={terminal}"
                ))
                .into());
            }
        }

        #[cfg(target_os = "linux")]
        if let Some(ready) = source_boot_ready {
            if let Err(error) = ready.commit_started(&thread_id, &turn_id).await {
                let terminal =
                    interrupt_cancelled_queue_turn(client, seq, &thread_id, &turn_id).await;
                return Err(SessionPoisoned(format!(
                    "native guest actual turn binding failed: {error}; terminal_observed={terminal}"
                ))
                .into());
            }
        }
        // Event loop
        let mut reply_capture = DirectSessionReplyCapture::default();
        let mut completion_message: Option<String> = None;
        // `AgentMessage` events carry no turn id, so an orphaned message from
        // a prior/interrupted turn still queued on this reused thread could
        // become this turn's reply (ctox#21 P1
        // review). Only trust `AgentMessage` once we have observed the
        // `TurnStarted` for OUR turn_id; everything before that belongs to an
        // earlier turn and is ignored for reply attribution.
        let mut saw_our_turn_started = false;
        #[cfg(unix)]
        let mut saw_our_turn_completed = false;
        let turn_started_at = Instant::now();
        let mut last_usage_event_at = turn_started_at;
        let mut last_recorded_cumulative_usage: Option<ApiTokenUsage> = None;
        let mut pending_api_cost_records = Vec::new();
        let mut tool_call_count = 0_u64;
        let mut activity_turn_count = 0_u64;
        let mut saw_reasoning_section_break = false;
        let mut seen_plan_updates = None;
        let deadline = timeout.map(|d| tokio::time::Instant::now() + d);
        let mut lease_tick = tokio::time::interval(Duration::from_millis(250));
        lease_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            let event = tokio::select! {
                biased;
                _ = lease_tick.tick(), if lease_reader.is_some() => {
                    let fence = queue_turn_lease.expect("reader requires lease fence");
                    let check = fence.still_owned(lease_reader.as_ref().unwrap());
                    if !matches!(check, Ok(true)) {
                        let reason = match check {
                            Ok(false) => "native lease revoked".to_string(),
                            Err(error) => format!("native lease cannot be verified: {error}"),
                            Ok(true) => unreachable!(),
                        };
                        let terminal = interrupt_cancelled_queue_turn(
                            client, seq, &thread_id, &turn_id,
                        ).await;
                        // Cancellation suppresses the reply, not the cost
                        // of model work already observed before the stop.
                        if !pending_api_cost_records.is_empty() {
                            if let Err(err) = api_costs::record_api_model_usage_batch(
                                root, &pending_api_cost_records,
                            ) {
                                eprintln!("[ctox direct-session] cost tracking failed: {err}");
                            }
                        }
                        progress(&serde_json::json!({
                            "event_kind": "worker.turn_cancelled",
                            "title": "Queue turn interrupted",
                            "body_text": reason,
                            "metadata": {"thread_id": thread_id, "turn_id": turn_id,
                                "terminal_observed": terminal},
                        }));
                        // Rebuild only this session. An interrupt acknowledgement
                        // alone is not evidence that its tools have terminated.
                        return Err(SessionPoisoned(format!(
                            "queue turn cancelled: {reason}; terminal_observed={terminal}"
                        )).into());
                    }
                    continue;
                }
                event = async { Ok::<_, anyhow::Error>(match deadline {
                Some(d) => tokio::select! {
                    ev = client.next_event() => ev,
                    _ = tokio::time::sleep_until(d) => {
                        progress(&serde_json::json!({
                            "event_kind": "worker.turn_timeout",
                            "title": "Agent turn timed out",
                            "body_text": "The configured turn deadline was reached.",
                            "metadata": {
                                "runtime": {
                                    "seconds": turn_started_at.elapsed().as_secs(),
                                },
                                "tool_call_count": tool_call_count,
                                "metrics_mode": "cumulative",
                            },
                        }));
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
                })} => event?,
            };
            let Some(event) = event else { break };
            match event {
                InProcessServerEvent::ServerRequest(_) => {}
                InProcessServerEvent::ServerNotification(notification) => {
                    // V2 notifications carry their own thread/turn identity and
                    // must not depend on seeing a legacy TurnStarted first.
                    if let Some(plan) = current_turn_plan_event(&notification, &thread_id, &turn_id)
                    {
                        if let Some(event) = direct_session_progress_event(
                            &plan,
                            &turn_id,
                            turn_started_at.elapsed(),
                            &mut tool_call_count,
                            &mut activity_turn_count,
                            &mut saw_reasoning_section_break,
                            &mut seen_plan_updates,
                        ) {
                            progress(&event);
                        }
                    }
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
                    // Events from other threads on this connection must not
                    // contaminate this turn (reply text, token accounting):
                    // legacy notifications carry the conversation/thread id
                    // at the params top level, so scope on it when present
                    // (ctox#21 re-review).
                    if let Some(foreign) = legacy_notification_thread_id(&notif)
                        .filter(|event_thread| *event_thread != thread_id)
                    {
                        eprintln!(
                            "[ctox direct-session] ignoring event for foreign thread {foreign} while waiting on {thread_id}"
                        );
                        continue;
                    }
                    if let Some(msg) = try_extract_event_msg(&notif) {
                        ctx_log.observe(&msg);
                        if let EventMsg::TurnStarted(ts) = &msg {
                            if ts.turn_id == turn_id {
                                saw_our_turn_started = true;
                                progress(&serde_json::json!({
                                    "event_kind": "worker.turn_started",
                                    "title": "Agent turn started",
                                    "body_text": "The model runtime accepted the task.",
                                    "metadata": {
                                        "turn_id": turn_id.to_string(),
                                        "runtime": { "seconds": 0 },
                                        "tool_call_count": tool_call_count,
                                        "metrics_mode": "cumulative",
                                    },
                                }));
                            }
                        } else if saw_our_turn_started {
                            if let Some(event) = direct_session_progress_event(
                                &msg,
                                &turn_id,
                                turn_started_at.elapsed(),
                                &mut tool_call_count,
                                &mut activity_turn_count,
                                &mut saw_reasoning_section_break,
                                &mut seen_plan_updates,
                            ) {
                                progress(&event);
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
                            EventMsg::TurnStarted(ref ts) if ts.turn_id == turn_id => {}
                            EventMsg::AgentMessage(am) => {
                                // Ignore replies that arrive before our turn
                                // has started — they belong to an earlier turn
                                // draining off the reused thread.
                                if saw_our_turn_started {
                                    reply_capture.observe(&am);
                                }
                            }
                            EventMsg::TurnComplete(tc) if tc.turn_id == turn_id => {
                                // Preserve a witnessed explicit final answer
                                // when the terminal item is only Crew metadata.
                                // The reducer also rejects known commentary.
                                #[cfg(unix)]
                                {
                                    saw_our_turn_completed = legacy_notification_thread_id(&notif)
                                        == Some(thread_id.as_str());
                                }
                                completion_message = tc.last_agent_message;
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
                                    if let Some(class) = err
                                        .codex_error_info
                                        .as_ref()
                                        .and_then(TurnRuntimeErrorClass::from_codex_error_info)
                                    {
                                        return Err(anyhow::Error::new(TurnRuntimeError::new(
                                            class,
                                            format!("direct session error: {msg_str}"),
                                        )));
                                    }
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

        if let (Some(fence), Some(reader)) = (queue_turn_lease, lease_reader.as_ref()) {
            anyhow::ensure!(
                fence.still_owned(reader)?,
                "queue turn cancelled before reply persistence: native lease revoked"
            );
        }
        let final_message =
            reply_capture.complete(completion_message.as_deref(), saw_our_turn_started);

        ctx_log.log(
            "turn_end",
            &format!(
                "\"reply_chars\":{}",
                final_message.as_ref().map(|m| m.len()).unwrap_or(0)
            ),
        );

        let final_message = final_message
            .ok_or_else(|| anyhow::anyhow!("turn completed without assistant message"))?;
        #[cfg(unix)]
        if prepared_guest {
            if !saw_our_turn_completed {
                return Err(SessionPoisoned(
                    "native capture requires the exact completed thread/turn witness".into(),
                )
                .into());
            }
            anyhow::ensure!(
                native_capture_owner.is_none() && native_guest_execution.is_some(),
                "native source owner already retired or lacks admitted execution"
            );
            // This follows the exact TurnComplete witness. It retires command
            // and frame consumers; checked shutdown still has to drain the
            // actual session task and journal before returning capture authority.
            *native_capture_owner = Some(
                provider_owner
                    .take()
                    .context("native terminal turn lost its actual provider owner")?
                    .retire_turn_for_capture(&thread_id, &turn_id)?,
            );
        }
        Ok(final_message)
    }
}

#[cfg(test)]
#[path = "direct_session_shutdown_tests.rs"]
mod shutdown_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_cancel_terminal_witness_requires_exact_thread_and_turn() {
        let event = |thread: &str, turn: Option<&str>| {
            InProcessServerEvent::LegacyNotification(JSONRPCNotification {
                method: "codex/event/turn_aborted".to_owned(),
                params: Some(serde_json::json!({
                    "threadId": thread,
                    "msg": EventMsg::TurnAborted(ctox_protocol::protocol::TurnAbortedEvent {
                        turn_id: turn.map(str::to_owned),
                        reason: ctox_protocol::protocol::TurnAbortReason::Interrupted,
                    }),
                })),
            })
        };
        assert!(queue_turn_terminal_event(
            &event("mine", Some("current")),
            "mine",
            "current"
        ));
        assert!(!queue_turn_terminal_event(
            &event("other", Some("current")),
            "mine",
            "current"
        ));
        assert!(!queue_turn_terminal_event(
            &event("mine", Some("previous")),
            "mine",
            "current"
        ));
        assert!(!queue_turn_terminal_event(
            &event("mine", None),
            "mine",
            "current"
        ));
    }

    fn plan_v2_notification(
        thread_id: &str,
        turn_id: &str,
        status: ctox_app_server_protocol::TurnPlanStepStatus,
    ) -> ServerNotification {
        ServerNotification::TurnPlanUpdated(ctox_app_server_protocol::TurnPlanUpdatedNotification {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            explanation: Some("Verify the recorded result".into()),
            plan: vec![ctox_app_server_protocol::TurnPlanStep {
                step: "Prüfen".into(),
                status,
            }],
        })
    }

    #[test]
    fn direct_plan_v2_persists_real_steps_before_review_without_legacy_start() -> Result<()> {
        use crate::context::lcm;
        let notification = plan_v2_notification(
            "thread-current",
            "turn-current",
            ctox_app_server_protocol::TurnPlanStepStatus::InProgress,
        );
        let msg = current_turn_plan_event(&notification, "thread-current", "turn-current")
            .expect("a scoped typed plan must not require a legacy start event");
        let mut tool_count = 0;
        let mut activity_count = 0;
        let mut seen = None;
        let event = direct_session_progress_event(
            &msg,
            "turn-current",
            Duration::ZERO,
            &mut tool_count,
            &mut activity_count,
            &mut false,
            &mut seen,
        )
        .expect("typed plan reaches the native progress contract");
        assert_eq!(event["event_kind"], "worker.plan_updated");
        let plan = &event["metadata"]["plan"];
        assert_eq!(plan["plan"][0]["status"], "in_progress");
        let steps = plan["plan"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| lcm::TaskExecutionPlanStepInput {
                label: step["step"].as_str().unwrap().to_owned(),
                status: step["status"].as_str().unwrap().to_owned(),
            })
            .collect::<Vec<_>>();
        let temp = tempfile::tempdir()?;
        let db = temp.path().join("ctox.sqlite3");
        lcm::run_record_task_execution_plan(
            &db,
            lcm::TaskExecutionPlanUpdate {
                work_key: "worker-attempt:typed-plan",
                task_id: "task-typed-plan",
                command_id: "command-typed-plan",
                attempt_id: "attempt-typed-plan",
                explanation: plan["explanation"].as_str(),
                steps: &steps,
            },
        )?;
        let review = lcm::run_prepare_task_execution_review(&db, "worker-attempt:typed-plan")?;
        assert_eq!(review["total_steps"], 1);
        assert_eq!(review["completed_steps"], 0);
        assert_eq!(review["steps"][0]["label"], "Prüfen");
        assert!(
            lcm::run_set_task_execution_review_status(
                &db,
                "worker-attempt:typed-plan",
                "completed"
            )
            .is_err(),
            "a received plan is not completed-work evidence"
        );
        assert_eq!((tool_count, activity_count), (1, 1));
        Ok(())
    }

    #[test]
    fn direct_plan_v2_rejects_foreign_thread_and_stale_turn() {
        for (thread, turn) in [("foreign", "current"), ("current", "stale")] {
            let notification = plan_v2_notification(
                thread,
                turn,
                ctox_app_server_protocol::TurnPlanStepStatus::Completed,
            );
            assert!(current_turn_plan_event(&notification, "current", "current").is_none());
        }
    }

    #[test]
    fn direct_plan_v2_and_legacy_deduplicate_in_either_order_but_keep_changes() {
        for typed_first in [true, false] {
            let notification = plan_v2_notification(
                "thread",
                "turn",
                ctox_app_server_protocol::TurnPlanStepStatus::Pending,
            );
            let typed = current_turn_plan_event(&notification, "thread", "turn").unwrap();
            let legacy = EventMsg::PlanUpdate(UpdatePlanArgs {
                explanation: Some("Verify the recorded result".into()),
                plan: vec![PlanItemArg {
                    step: "Prüfen".into(),
                    status: StepStatus::Pending,
                }],
            });
            let ordered = if typed_first {
                [&typed, &legacy]
            } else {
                [&legacy, &typed]
            };
            let mut seen = None;
            let mut tools = 0;
            let mut activities = 0;
            for (index, msg) in ordered.into_iter().enumerate() {
                assert_eq!(
                    direct_session_progress_event(
                        msg,
                        "turn",
                        Duration::ZERO,
                        &mut tools,
                        &mut activities,
                        &mut false,
                        &mut seen,
                    )
                    .is_some(),
                    index == 0
                );
            }
            let changed = current_turn_plan_event(
                &plan_v2_notification(
                    "thread",
                    "turn",
                    ctox_app_server_protocol::TurnPlanStepStatus::Completed,
                ),
                "thread",
                "turn",
            )
            .unwrap();
            let event = direct_session_progress_event(
                &changed,
                "turn",
                Duration::ZERO,
                &mut tools,
                &mut activities,
                &mut false,
                &mut seen,
            )
            .expect("a real status change must still be persisted");
            assert_eq!(event["metadata"]["plan"]["plan"][0]["status"], "completed");
            assert_eq!((tools, activities), (2, 2));
            // A genuine return to an earlier plan after a different update
            // must not be confused with the duplicate transport notification.
            assert!(direct_session_progress_event(
                &legacy,
                "turn",
                Duration::ZERO,
                &mut tools,
                &mut activities,
                &mut false,
                &mut seen,
            )
            .is_some());
            assert_eq!((tools, activities), (3, 3));
        }
    }

    #[test]
    fn direct_sessions_receive_the_managed_linux_sandbox_executable() {
        let paths = direct_session_arg0_paths();

        #[cfg(target_os = "linux")]
        assert_eq!(paths.ctox_linux_sandbox_exe, std::env::current_exe().ok());

        #[cfg(not(target_os = "linux"))]
        assert!(paths.ctox_linux_sandbox_exe.is_none());
    }

    #[test]
    fn business_os_mcp_thread_config_is_local_scoped_and_tool_bounded() {
        let config =
            business_os_mcp_thread_config("0.0.0.0:9877", "test-secret", "command-session")
                .expect("build MCP thread config");
        let server = config
            .get("mcp_servers")
            .and_then(|value| value.get(BUSINESS_OS_MCP_SESSION_SERVER_NAME))
            .expect("Business OS MCP server");

        assert_eq!(
            server.get("url").and_then(JsonValue::as_str),
            Some("http://127.0.0.1:9877/mcp")
        );
        assert_eq!(
            server.get("required").and_then(JsonValue::as_bool),
            Some(true)
        );
        assert_eq!(
            server
                .pointer("/http_headers/Authorization")
                .and_then(JsonValue::as_str),
            Some("Bearer test-secret")
        );
        assert_eq!(
            server
                .pointer("/http_headers/X-CTOX-Business-Command-Session")
                .and_then(JsonValue::as_str),
            Some("command-session")
        );
        assert_eq!(
            server
                .get("enabled_tools")
                .and_then(JsonValue::as_array)
                .map(Vec::len),
            Some(BUSINESS_OS_MCP_SESSION_TOOLS.len())
        );
        assert!(!server
            .get("enabled_tools")
            .and_then(JsonValue::as_array)
            .expect("enabled tools")
            .iter()
            .any(|tool| tool.as_str() == Some("business_os.upsert_record")));
        assert!(server
            .get("enabled_tools")
            .and_then(JsonValue::as_array)
            .expect("enabled tools")
            .iter()
            .any(|tool| tool.as_str() == Some("business_os.execute_writeback")));
        assert_eq!(
            config.get("features.apps").and_then(JsonValue::as_bool),
            Some(false)
        );
    }

    #[test]
    fn business_os_mcp_thread_config_exposes_native_jour_fixe_tools() {
        let config =
            business_os_mcp_thread_config("127.0.0.1:8788", "test-secret", "command-session")
                .expect("build scheduled Supervisor MCP config");
        let tools = config["mcp_servers"][BUSINESS_OS_MCP_SESSION_SERVER_NAME]["enabled_tools"]
            .as_array()
            .expect("explicit enabled tools");
        for name in ["business_os.jour_fixe_read", "business_os.jour_fixe_update"] {
            assert_eq!(
                tools.iter().filter(|tool| tool.as_str() == Some(name)).count(),
                1,
                "the harness must expose the native meeting tool exactly once: {name}"
            );
        }
    }

    #[test]
    fn business_os_mcp_thread_config_rejects_remote_urls_and_empty_tokens() {
        assert!(business_os_mcp_thread_config(
            "https://example.com/mcp",
            "secret",
            "command-session"
        )
        .is_err());
        assert!(business_os_mcp_thread_config("127.0.0.1:8788", " ", "command-session").is_err());
        assert!(business_os_mcp_thread_config("127.0.0.1:8788", "secret", " ").is_err());
    }

    #[test]
    fn managed_linux_sandbox_override_survives_turn_rebuilds() {
        let mut overrides = vec![(
            "features.use_legacy_landlock".to_string(),
            toml::Value::Boolean(false),
        )];

        configure_managed_linux_sandbox(&mut overrides);

        #[cfg(target_os = "linux")]
        assert_eq!(
            overrides,
            vec![(
                "features.use_legacy_landlock".to_string(),
                toml::Value::Boolean(true),
            )]
        );

        #[cfg(not(target_os = "linux"))]
        assert_eq!(
            overrides,
            vec![(
                "features.use_legacy_landlock".to_string(),
                toml::Value::Boolean(false),
            )]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn managed_worker_default_readable_roots_cover_install_wrapper_and_resolver() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("create readable-roots fixture");
        let ctox_lib = temp.path().join("x/lib/ctox");
        let release_root = ctox_lib.join("releases/r1");
        let exe = release_root.join("bin/ctox-real");
        std::fs::create_dir_all(exe.parent().expect("release bin directory"))
            .expect("create release bin directory");
        std::fs::write(&exe, b"fixture").expect("create ctox executable fixture");
        symlink("releases/r1", ctox_lib.join("current")).expect("create current symlink");

        let wrapper_dir = temp.path().join("home/.local/bin");
        std::fs::create_dir_all(&wrapper_dir).expect("create wrapper directory");
        std::fs::write(wrapper_dir.join("ctox"), b"fixture").expect("create ctox wrapper fixture");

        let resolver_dir = temp.path().join("run/systemd/resolve");
        let resolver_target = resolver_dir.join("stub-resolv.conf");
        std::fs::create_dir_all(&resolver_dir).expect("create resolver directory");
        std::fs::write(&resolver_target, b"nameserver 127.0.0.53")
            .expect("create resolver fixture");

        let roots = managed_worker_default_readable_roots_for(
            &exe,
            std::slice::from_ref(&wrapper_dir),
            Some(&resolver_target),
        );

        assert!(roots.contains(&release_root.canonicalize().expect("canonical release root")));
        assert!(roots.contains(&ctox_lib.join("current")));
        assert!(roots.contains(
            &wrapper_dir
                .canonicalize()
                .expect("canonical wrapper directory")
        ));
        assert!(roots.contains(
            &resolver_dir
                .canonicalize()
                .expect("canonical resolver directory")
        ));
    }

    #[test]
    fn managed_linux_workers_cannot_read_sibling_workspaces() {
        use ctox_protocol::protocol::ReadOnlyAccess;

        let policy = managed_worker_sandbox_policy(false, &[], &[]);

        #[cfg(target_os = "linux")]
        assert!(matches!(
            policy,
            SandboxPolicy::WorkspaceWrite {
                writable_roots,
                read_only_access: ReadOnlyAccess::Restricted {
                    include_platform_defaults: true,
                    readable_roots,
                },
                exclude_tmpdir_env_var: true,
                exclude_slash_tmp: true,
                ..
            } if writable_roots.is_empty() && !readable_roots.is_empty()
        ));

        #[cfg(not(target_os = "linux"))]
        assert!(matches!(
            policy,
            SandboxPolicy::WorkspaceWrite {
                read_only_access: ReadOnlyAccess::FullAccess,
                ..
            }
        ));
    }

    #[test]
    fn managed_worker_scope_adds_exact_app_roots() {
        use ctox_protocol::protocol::ReadOnlyAccess;

        let writable = std::env::temp_dir().join("ctox-app-authoring-target");
        let readable = std::env::temp_dir().join("ctox-app-authoring-state");
        let policy = managed_worker_sandbox_policy(
            false,
            std::slice::from_ref(&writable),
            std::slice::from_ref(&readable),
        );
        assert!(matches!(
            policy,
            SandboxPolicy::WorkspaceWrite {
                writable_roots,
                read_only_access: ReadOnlyAccess::Restricted {
                    include_platform_defaults: true,
                    readable_roots,
                },
                ..
            } if writable_roots.iter().any(|root| root.as_path() == writable)
                && readable_roots.iter().any(|root| root.as_path() == readable)
        ));
    }

    #[tokio::test]
    async fn native_guest_startup_disables_inherited_background_execution() -> Result<()> {
        use ctox_core::features::Feature;
        let home = tempfile::tempdir()?;
        std::fs::write(
            home.path().join("config.toml"),
            r#"
notify = ["operator-notification"]
chatgpt_base_url = "https://untrusted.invalid/backend"
[features]
shell_snapshot = true
shell_zsh_fork = true
ctox_hooks = true
memory_tool = true
undo = true
multi_agent = true
enable_fanout = true
"#,
        )?;
        let mut overrides = vec![
            ("features.shell_snapshot".into(), toml::Value::Boolean(true)),
            (
                "notify".into(),
                toml::Value::Array(vec![toml::Value::String("provider-notification".into())]),
            ),
        ];
        overrides.push((
            "chatgpt_base_url".into(),
            toml::Value::String("https://other.invalid/backend".into()),
        ));
        configure_worker_tool_stack(&mut overrides, false);
        super::super::session_continuity::constrain_native_guest_startup(&mut overrides);
        let config = ConfigBuilder::default()
            .codex_home(home.path().to_path_buf())
            .cli_overrides(overrides)
            .build()
            .await?;
        for feature in [
            Feature::ShellSnapshot,
            Feature::ShellZshFork,
            Feature::CodexHooks,
            Feature::MemoryTool,
            Feature::GhostCommit,
            Feature::Collab,
            Feature::SpawnCsv,
        ] {
            assert!(
                !config.features.enabled(feature),
                "ambient feature {feature:?} escaped native startup profile"
            );
        }
        assert_eq!(config.chatgpt_base_url, "https://chatgpt.com/backend-api");
        assert_eq!(config.notify, Some(Vec::new()));
        assert!(
            config.features.enabled(Feature::ShellTool),
            "authorized turn tools retain their own dispatch boundary"
        );
        Ok(())
    }

    #[test]
    fn worker_sessions_enable_web_stack_and_forbid_free_subagents() {
        let mut overrides = vec![
            ("tools.ctox_web".to_string(), toml::Value::Boolean(false)),
            (
                "features.multi_agent".to_string(),
                toml::Value::Boolean(true),
            ),
        ];

        configure_worker_tool_stack(&mut overrides, false);

        assert_eq!(
            overrides,
            vec![
                ("tools.ctox_web".to_string(), toml::Value::Boolean(true)),
                (
                    "features.multi_agent".to_string(),
                    toml::Value::Boolean(false),
                ),
                (
                    "features.enable_fanout".to_string(),
                    toml::Value::Boolean(false),
                ),
                (
                    "features.memory_tool".to_string(),
                    toml::Value::Boolean(false)
                ),
            ]
        );
    }

    #[test]
    fn tool_disabled_review_sessions_do_not_enable_worker_tools() {
        let mut overrides = Vec::new();

        configure_worker_tool_stack(&mut overrides, true);

        assert!(overrides.is_empty());
    }

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
        let instructions = compose_base_instructions(&root, &BTreeMap::new(), None, None)
            .expect("worker instructions");
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
            None,
        )
        .expect("override instructions");
        assert!(instructions.contains("Act as the external reviewer."));
        // Review/repair sessions own their full system prompt; the worker
        // contract must not leak in and instruct worker actions.
        assert!(!instructions.contains("required durable outcome exists"));
        assert!(!instructions.contains("You are CTOX, the personal CTO agent"));
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
    fn legacy_notification_thread_scoping_extracts_ids() {
        let scoped = JSONRPCNotification {
            method: "codex/event/agent_message".to_string(),
            params: Some(serde_json::json!({
                "conversationId": "thread-a",
                "msg": {"type": "agent_message", "message": "hi"}
            })),
        };
        assert_eq!(legacy_notification_thread_id(&scoped), Some("thread-a"));

        let unscoped = JSONRPCNotification {
            method: "codex/event/agent_message".to_string(),
            params: Some(serde_json::json!({
                "msg": {"type": "agent_message", "message": "hi"}
            })),
        };
        assert_eq!(legacy_notification_thread_id(&unscoped), None);

        let thread_id_key = JSONRPCNotification {
            method: "codex/event/task_complete".to_string(),
            params: Some(serde_json::json!({"threadId": "thread-b"})),
        };
        assert_eq!(
            legacy_notification_thread_id(&thread_id_key),
            Some("thread-b")
        );
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
        // Drop performs bounded cleanup but cannot issue a quiescence receipt.
        let _ = self.shutdown_inner("dropping");
    }
}

impl PersistentSession {
    fn shutdown_inner(&mut self, action: &str) -> Result<()> {
        self.shutdown_inner_with(action, |_| Ok(())).map(|_| ())
    }

    /// A capture callback runs only after checked client shutdown, while the
    /// exact owned runtime can still read final state from its retained Core thread.
    fn shutdown_inner_with<T>(
        &mut self,
        action: &str,
        after_shutdown: impl FnOnce(&tokio::runtime::Runtime) -> Result<T>,
    ) -> Result<Option<T>> {
        // Take both owners before any branch. An orphaned client must still
        // be aborted, even when its runtime is absent.
        let client = self.client.take();
        let Some(runtime) = self.runtime.take() else {
            if let Some(client) = client {
                client.abort_now();
                anyhow::bail!("persistent session runtime ownership is missing");
            }
            return Ok(None);
        };
        let tid = &self.thread_id;
        eprintln!("[ctox direct-session] {action} persistent session thread_id={tid}");
        // Blocking runtime cleanup would panic in an async owner. Abrupt
        // cleanup is allowed here, but it must never acknowledge quiescence.
        if tokio::runtime::Handle::try_current().is_ok() {
            if let Some(client) = client {
                client.abort_now();
            }
            runtime.shutdown_background();
            eprintln!(
                "[ctox direct-session] {action} from async context; forced cleanup thread_id={tid}"
            );
            anyhow::bail!("persistent session graceful shutdown requires a synchronous owner");
        }
        let result = match client {
            Some(client) => {
                // The client retains cancellation ownership if this timeout
                // drops shutdown while its actual cleanup is still in flight.
                match runtime.block_on(async {
                    tokio::time::timeout(Duration::from_secs(8), client.shutdown()).await
                }) {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(err)) => Err(anyhow::Error::from(err)
                        .context("persistent session client shutdown failed")),
                    Err(_) => Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "persistent session shutdown timed out",
                    )
                    .into()),
                }
            }
            None => Err(anyhow::anyhow!(
                "persistent session client ownership is missing"
            )),
        };
        let result = result.and_then(|()| after_shutdown(&runtime).map(Some));
        // Cleanup always finishes its bounded runtime drain before returning
        // the original client/capture result. Runtime teardown cannot replace an error.
        runtime.shutdown_timeout(Duration::from_secs(2));
        match &result {
            Ok(_) => {
                eprintln!("[ctox direct-session] persistent session shut down thread_id={tid}")
            }
            Err(err) => eprintln!(
                "[ctox direct-session] persistent session shutdown error thread_id={tid}: {err}"
            ),
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Conversation/thread id a legacy notification is scoped to, when present.
/// Legacy `codex/event/*` notifications place it at the params top level as
/// `conversationId` (some producers use `threadId`).
fn legacy_notification_thread_id(notif: &JSONRPCNotification) -> Option<&str> {
    let obj = notif.params.as_ref()?.as_object()?;
    obj.get("conversationId")
        .or_else(|| obj.get("threadId"))
        .or_else(|| obj.get("thread_id"))
        .and_then(|value| value.as_str())
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

fn current_turn_plan_event(
    notification: &ServerNotification,
    thread_id: &str,
    turn_id: &str,
) -> Option<EventMsg> {
    let ServerNotification::TurnPlanUpdated(plan) = notification else {
        return None;
    };
    if plan.thread_id != thread_id || plan.turn_id != turn_id {
        return None;
    }
    Some(EventMsg::PlanUpdate(UpdatePlanArgs {
        explanation: plan.explanation.clone(),
        plan: plan
            .plan
            .iter()
            .map(|step| PlanItemArg {
                step: step.step.clone(),
                status: match step.status {
                    ctox_app_server_protocol::TurnPlanStepStatus::Pending => StepStatus::Pending,
                    ctox_app_server_protocol::TurnPlanStepStatus::InProgress => {
                        StepStatus::InProgress
                    }
                    ctox_app_server_protocol::TurnPlanStepStatus::Completed => {
                        StepStatus::Completed
                    }
                },
            })
            .collect(),
    }))
}

fn direct_session_progress_event(
    msg: &EventMsg,
    turn_id: &str,
    elapsed: Duration,
    tool_call_count: &mut u64,
    activity_turn_count: &mut u64,
    saw_reasoning_section_break: &mut bool,
    seen_plan_updates: &mut Option<String>,
) -> Option<JsonValue> {
    let elapsed_seconds = elapsed.as_secs();
    let cumulative_metadata = |extra: JsonValue| {
        serde_json::json!({
            "turn_id": turn_id,
            "runtime": { "seconds": elapsed_seconds },
            "tool_call_count": *tool_call_count,
            "metrics_mode": "cumulative",
            "detail": extra,
        })
    };
    let tool_started =
        |tool_type: &str, tool_name: &str, call_id: String, tool_call_count: &mut u64| {
            *tool_call_count = tool_call_count.saturating_add(1);
            serde_json::json!({
                "event_kind": "worker.tool_started",
                "title": format!("Tool started: {tool_name}"),
                "body_text": "",
                "metadata": {
                    "turn_id": turn_id,
                    "runtime": { "seconds": elapsed_seconds },
                    "tool_call_count": *tool_call_count,
                    "metrics_mode": "cumulative",
                    "tool": {
                        "type": tool_type,
                        "name": tool_name,
                        "call_id": call_id,
                    },
                },
            })
        };
    let tool_completed =
        |tool_type: &str, tool_name: &str, call_id: String, success: Option<bool>| {
            serde_json::json!({
                "event_kind": "worker.tool_completed",
                "title": format!("Tool finished: {tool_name}"),
                "body_text": "",
                "metadata": {
                    "turn_id": turn_id,
                    "runtime": { "seconds": elapsed_seconds },
                    "tool_call_count": *tool_call_count,
                    "metrics_mode": "cumulative",
                    "tool": {
                        "type": tool_type,
                        "name": tool_name,
                        "call_id": call_id,
                        "success": success,
                    },
                },
            })
        };

    match msg {
        EventMsg::PlanUpdate(plan) => {
            // PlanUpdate does not expose the originating tool call id. Bind
            // the activity to the stable turn plus canonical plan payload so
            // a replayed notification is deduplicated by durable storage.
            let plan_digest = Sha256::digest(serde_json::to_vec(plan).unwrap_or_default());
            let plan_event_id = plan_digest[..8]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            // The server can emit both typed and legacy forms of one update.
            // Use the same canonical payload as the durable activity identity.
            if seen_plan_updates.as_deref() == Some(plan_event_id.as_str()) {
                return None;
            }
            *seen_plan_updates = Some(plan_event_id.clone());
            *tool_call_count = tool_call_count.saturating_add(1);
            *activity_turn_count = activity_turn_count.saturating_add(1);
            Some(serde_json::json!({
                "event_kind": "worker.plan_updated",
                "title": "Execution plan updated",
                "body_text": "",
                "metadata": {
                    "turn_id": turn_id,
                    "activity": {
                        "id": format!("{turn_id}:plan:{plan_event_id}"),
                        "kind": "tool",
                        "tool_name": "update_plan",
                        "attribute_to_current_step": false,
                    },
                    "plan": plan,
                    "runtime": { "seconds": elapsed_seconds },
                    "tool_call_count": *tool_call_count,
                    "metrics_mode": "cumulative",
                },
            }))
        }
        EventMsg::AgentReasoningSectionBreak(ev) => {
            *saw_reasoning_section_break = true;
            *activity_turn_count = activity_turn_count.saturating_add(1);
            let stable_id = if ev.item_id.trim().is_empty() {
                format!("{turn_id}:thinking:{}", *activity_turn_count)
            } else {
                format!("{turn_id}:thinking:{}:{}", ev.item_id, ev.summary_index)
            };
            Some(serde_json::json!({
                "event_kind": "worker.thinking_started",
                "title": "Thinking block started",
                "body_text": "",
                "metadata": {
                    "turn_id": turn_id,
                    "activity": {
                        "id": stable_id,
                        "kind": "thinking",
                        "attribute_to_current_step": true,
                    },
                    "runtime": { "seconds": elapsed_seconds },
                    "tool_call_count": *tool_call_count,
                    "metrics_mode": "cumulative",
                },
            }))
        }
        EventMsg::AgentReasoning(ev) if !*saw_reasoning_section_break => {
            *activity_turn_count = activity_turn_count.saturating_add(1);
            // Older providers may omit section-break events. Hashing the
            // completed summary gives replay stability without persisting or
            // emitting any reasoning text.
            let reasoning_digest = Sha256::digest(ev.text.as_bytes());
            let reasoning_event_id = reasoning_digest[..8]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            Some(serde_json::json!({
                "event_kind": "worker.thinking_started",
                "title": "Thinking block completed",
                "body_text": "",
                "metadata": {
                    "turn_id": turn_id,
                    "activity": {
                        "id": format!("{turn_id}:thinking:{reasoning_event_id}"),
                        "kind": "thinking",
                        "attribute_to_current_step": true,
                    },
                    "runtime": { "seconds": elapsed_seconds },
                    "tool_call_count": *tool_call_count,
                    "metrics_mode": "cumulative",
                },
            }))
        }
        EventMsg::TokenCount(tc) => {
            let info = tc.info.as_ref()?;
            Some(serde_json::json!({
                "event_kind": "worker.token_usage",
                "title": "Model usage updated",
                "body_text": "",
                "metadata": {
                    "turn_id": turn_id,
                    "usage": {
                        "input_tokens": info.total_token_usage.input_tokens,
                        "output_tokens": info.total_token_usage.output_tokens,
                        "reasoning_output_tokens": info.total_token_usage.reasoning_output_tokens,
                        "last_input_tokens": info.last_token_usage.input_tokens,
                        "last_output_tokens": info.last_token_usage.output_tokens,
                        "total_tokens": info.total_token_usage.total_tokens,
                    },
                    "runtime": { "seconds": elapsed_seconds },
                    "tool_call_count": *tool_call_count,
                    "metrics_mode": "cumulative",
                },
            }))
        }
        EventMsg::ExecCommandBegin(ev) => Some(tool_started(
            "exec_command",
            "Terminal",
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::ExecCommandEnd(ev) => Some(tool_completed(
            "exec_command",
            "Terminal",
            ev.call_id.to_string(),
            Some(ev.exit_code == 0),
        )),
        EventMsg::McpToolCallBegin(ev) => Some(tool_started(
            "mcp",
            &format!("{}.{}", ev.invocation.server, ev.invocation.tool),
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::McpToolCallEnd(ev) => Some(tool_completed(
            "mcp",
            &format!("{}.{}", ev.invocation.server, ev.invocation.tool),
            ev.call_id.to_string(),
            Some(ev.is_success()),
        )),
        EventMsg::DynamicToolCallRequest(ev) => Some(tool_started(
            "dynamic",
            &ev.tool,
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::DynamicToolCallResponse(ev) => Some(tool_completed(
            "dynamic",
            &ev.tool,
            ev.call_id.to_string(),
            Some(ev.success),
        )),
        EventMsg::WebSearchBegin(ev) => Some(tool_started(
            "web_search",
            "Web search",
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::WebSearchEnd(ev) => Some(tool_completed(
            "web_search",
            "Web search",
            ev.call_id.to_string(),
            Some(true),
        )),
        EventMsg::ViewImageToolCall(ev) => Some(tool_started(
            "view_image",
            "Image viewer",
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::ImageGenerationBegin(ev) => Some(tool_started(
            "image_generation",
            "Image generation",
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::PatchApplyBegin(ev) => Some(tool_started(
            "apply_patch",
            "Apply patch",
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::CollabAgentSpawnBegin(ev) => Some(tool_started(
            "collaboration",
            "Spawn agent",
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::CollabAgentInteractionBegin(ev) => Some(tool_started(
            "collaboration",
            "Agent interaction",
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::CollabWaitingBegin(ev) => Some(tool_started(
            "collaboration",
            "Wait for agents",
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::CollabCloseBegin(ev) => Some(tool_started(
            "collaboration",
            "Close agent",
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::CollabResumeBegin(ev) => Some(tool_started(
            "collaboration",
            "Resume agent",
            ev.call_id.to_string(),
            tool_call_count,
        )),
        EventMsg::TurnComplete(tc) if tc.turn_id == turn_id => Some(serde_json::json!({
            "event_kind": "worker.turn_completed",
            "title": "Agent turn completed",
            "body_text": "",
            "metadata": cumulative_metadata(serde_json::json!({
                "turn_id": turn_id,
                "has_last_message": tc.last_agent_message.is_some(),
            })),
        })),
        _ => None,
    }
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
