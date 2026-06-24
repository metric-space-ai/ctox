use anyhow::{bail, Context};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::business_os::store::BusinessCommand;

const CODEX_STATUS_TIMEOUT_SECS: u64 = 10;
const CODEX_EXEC_TIMEOUT_SECS: u64 = 600;
const CODEX_AUTH_DOCS_URL: &str = "https://developers.openai.com/codex/auth";
const CODEX_INSTALL_DOCS_URL: &str = "https://developers.openai.com/codex/cli";
const CLAUDE_STATUS_TIMEOUT_SECS: u64 = 15;
const CLAUDE_EXEC_TIMEOUT_SECS: u64 = 600;
const CLAUDE_DOCS_URL: &str = "https://docs.anthropic.com/en/docs/claude-code/cli-reference";
const CLAUDE_INSTALL_DOCS_URL: &str = "https://code.claude.com/docs/en/quickstart";
const AGY_STATUS_TIMEOUT_SECS: u64 = 15;
const AGY_EXEC_TIMEOUT_SECS: u64 = 600;
const AGY_DOCS_URL: &str = "https://antigravity.google/docs/cli-overview";
const AGY_INSTALL_DOCS_URL: &str = "https://antigravity.google/docs/cli-install";
const INSTALL_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Provider {
    Antigravity,
    Claude,
    Codex,
    Mock,
}

impl Provider {
    fn parse(raw: &str) -> anyhow::Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "antigravity" | "agy" | "google-antigravity" => Ok(Self::Antigravity),
            "claude" | "claude-code" | "anthropic-claude" => Ok(Self::Claude),
            "codex" | "openai-codex" => Ok(Self::Codex),
            "mock" | "contract" => Ok(Self::Mock),
            other => bail!("unsupported coding agent provider '{other}'"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Antigravity => "antigravity",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Mock => "mock",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Antigravity => "Google Antigravity",
            Self::Claude => "Anthropic Claude Code",
            Self::Codex => "OpenAI Codex",
            Self::Mock => "CTOX Mock Coding Agent",
        }
    }

    fn binary_names(self) -> &'static [&'static str] {
        match self {
            Self::Antigravity => &["agy", "antigravity"],
            Self::Claude => &["claude"],
            Self::Codex => &["codex"],
            Self::Mock => &[],
        }
    }

    fn app_bundle_names(self) -> &'static [&'static str] {
        match self {
            Self::Antigravity => &["Antigravity.app", "Google Antigravity.app"],
            Self::Claude => &["Claude.app", "Claude Code.app"],
            Self::Codex => &["Codex.app", "OpenAI Codex.app"],
            Self::Mock => &[],
        }
    }
}

#[derive(Debug)]
struct ParsedCommand {
    provider: Provider,
    args: Vec<String>,
}

#[derive(Debug)]
struct CodingSession {
    session_id: String,
    provider: Provider,
    workspace_root: String,
    status: String,
    title: String,
    last_prompt: String,
    external_session_id: String,
    metadata: Value,
    updated_at_ms: i64,
}

#[derive(Debug)]
struct CodingGrant {
    provider: Provider,
    path: String,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Debug)]
struct CodingEvent {
    event_id: String,
    session_id: String,
    role: String,
    text: String,
    status: String,
    created_at_ms: i64,
}

#[derive(Debug)]
struct ProcessRun {
    exit_code: i32,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

#[derive(Debug, Default)]
struct CodexRunSummary {
    thread_id: Option<String>,
    final_message: Option<String>,
    usage: Value,
    turn_status: Option<String>,
    event_count: usize,
}

#[derive(Debug)]
struct CodexRun {
    process: ProcessRun,
    summary: CodexRunSummary,
}

#[derive(Debug, Default)]
struct ClaudeRunSummary {
    session_id: Option<String>,
    result: Option<String>,
    type_name: Option<String>,
    subtype: Option<String>,
    is_error: Option<bool>,
    api_error_status: Option<String>,
    stop_reason: Option<String>,
    terminal_reason: Option<String>,
    total_cost_usd: Value,
    usage: Value,
    model_usage: Value,
    permission_denials: Value,
    uuid: Option<String>,
}

#[derive(Debug)]
struct ClaudeRun {
    process: ProcessRun,
    summary: ClaudeRunSummary,
}

#[derive(Debug)]
struct AgyRun {
    process: ProcessRun,
    conversation_id: Option<String>,
    log: String,
}

#[derive(Debug)]
struct CodexAuthStatus {
    ready: bool,
    state: String,
    binary: Option<String>,
    stdout: String,
    stderr: String,
    exit_code: i32,
    timed_out: bool,
    message: String,
}

impl CodexAuthStatus {
    fn to_json(&self) -> Value {
        json!({
            "status": self.state,
            "ready": self.ready,
            "binary": self.binary,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "exit_code": self.exit_code,
            "timed_out": self.timed_out,
            "message": self.message,
            "auth_docs": CODEX_AUTH_DOCS_URL,
        })
    }
}

#[derive(Debug)]
struct AgyAuthStatus {
    ready: bool,
    state: String,
    binary: Option<String>,
    version: Option<String>,
    models: Vec<String>,
    stdout: String,
    stderr: String,
    exit_code: i32,
    timed_out: bool,
    message: String,
}

impl AgyAuthStatus {
    fn to_json(&self) -> Value {
        json!({
            "status": self.state,
            "ready": self.ready,
            "binary": self.binary,
            "version": self.version,
            "models": self.models,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "exit_code": self.exit_code,
            "timed_out": self.timed_out,
            "message": self.message,
            "docs": AGY_DOCS_URL,
        })
    }
}

#[derive(Debug)]
struct ClaudeAuthStatus {
    ready: bool,
    state: String,
    binary: Option<String>,
    version: Option<String>,
    auth_method: Option<String>,
    api_provider: Option<String>,
    subscription_type: Option<String>,
    stdout: String,
    stderr: String,
    exit_code: i32,
    timed_out: bool,
    message: String,
}

impl ClaudeAuthStatus {
    fn to_json(&self) -> Value {
        json!({
            "status": self.state,
            "ready": self.ready,
            "binary": self.binary,
            "version": self.version,
            "auth_method": self.auth_method,
            "api_provider": self.api_provider,
            "subscription_type": self.subscription_type,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "exit_code": self.exit_code,
            "timed_out": self.timed_out,
            "message": self.message,
            "docs": CLAUDE_DOCS_URL,
        })
    }
}

pub(crate) fn is_coding_agent_command(command_type: &str) -> bool {
    command_type == "ctox.coding_agent.execute" || command_type.starts_with("ctox.coding_agent.")
}

pub(crate) fn handle_cli(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let outcome = execute_cli(root, args)?;
    println!("{}", serde_json::to_string_pretty(&outcome)?);
    if outcome.get("ok").and_then(Value::as_bool) == Some(false) {
        let message = outcome
            .get("stderr")
            .and_then(Value::as_str)
            .or_else(|| outcome.get("error").and_then(Value::as_str))
            .unwrap_or("coding agent command failed");
        bail!("{message}");
    }
    Ok(())
}

pub(crate) fn handle_business_command(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    match command.command_type.as_str() {
        "ctox.coding_agent.execute" => {
            let args = command
                .payload
                .get("args")
                .and_then(Value::as_array)
                .context("ctox.coding_agent.execute payload.args is required")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .context("ctox.coding_agent.execute args must be strings")
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            execute_compat(root, &args)
        }
        "ctox.coding_agent.status" => {
            let provider = payload_provider(&command.payload)?;
            status_outcome(root, provider)
        }
        "ctox.coding_agent.install" => {
            let provider = payload_provider(&command.payload)?;
            install_outcome(root, provider, payload_bool(&command.payload, "apply"))
        }
        "ctox.coding_agent.auth.start" => {
            let provider = payload_provider(&command.payload)?;
            auth_start_outcome(root, provider)
        }
        "ctox.coding_agent.auth.status" => {
            let provider = payload_provider(&command.payload)?;
            auth_status_outcome(root, provider)
        }
        "ctox.coding_agent.workspace.grant" => {
            let provider = payload_provider(&command.payload)?;
            let path = payload_string(&command.payload, "path")?;
            grant_workspace(root, provider, &path)
        }
        "ctox.coding_agent.workspace.list" => {
            let provider = payload_provider(&command.payload)?;
            list_grants_outcome(root, provider)
        }
        "ctox.coding_agent.workspace.revoke" => {
            let provider = payload_provider(&command.payload)?;
            let path = payload_string(&command.payload, "path")?;
            revoke_workspace(root, provider, &path)
        }
        "ctox.coding_agent.lifecycle.start" => {
            let provider = payload_provider(&command.payload)?;
            lifecycle_outcome(provider, "start")
        }
        "ctox.coding_agent.lifecycle.stop" => {
            let provider = payload_provider(&command.payload)?;
            lifecycle_outcome(provider, "stop")
        }
        "ctox.coding_agent.lifecycle.headless" => {
            let provider = payload_provider(&command.payload)?;
            lifecycle_outcome(provider, "headless")
        }
        "ctox.coding_agent.session.create" => {
            let provider = payload_provider(&command.payload)?;
            let workspace = payload_string(&command.payload, "workspace_root")
                .or_else(|_| payload_string(&command.payload, "workspace"))?;
            let prompt = payload_string(&command.payload, "prompt")?;
            create_session(root, provider, &workspace, &prompt)
        }
        "ctox.coding_agent.session.prompt" => {
            let provider = payload_provider(&command.payload)?;
            let session_id = payload_string(&command.payload, "session_id")?;
            let prompt = payload_string(&command.payload, "prompt")?;
            prompt_session(root, provider, &session_id, &prompt)
        }
        "ctox.coding_agent.session.list" => {
            let provider = payload_provider(&command.payload)?;
            let workspace = command
                .payload
                .get("workspace_root")
                .or_else(|| command.payload.get("workspace"))
                .and_then(Value::as_str)
                .map(str::to_string);
            list_sessions_outcome(root, provider, workspace.as_deref())
        }
        "ctox.coding_agent.session.get" => {
            let provider = payload_provider(&command.payload)?;
            let session_id = payload_string(&command.payload, "session_id")?;
            get_session_outcome(root, provider, &session_id)
        }
        "ctox.coding_agent.session.stop" => {
            let provider = payload_provider(&command.payload)?;
            let session_id = payload_string(&command.payload, "session_id")?;
            stop_session(root, provider, &session_id)
        }
        other => Ok(error_outcome(
            Provider::Mock,
            "unsupported",
            format!("unsupported coding agent command type '{other}'"),
        )),
    }
}

fn execute_cli(root: &Path, args: &[String]) -> anyhow::Result<Value> {
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        return Ok(help_outcome());
    }

    let mut owned = Vec::with_capacity(args.len() + 2);
    let (provider, command_args) = parse_cli_args(args)?;

    match command_args.first().map(String::as_str) {
        Some("status") => status_outcome(root, provider),
        Some("providers") => providers_outcome(root),
        Some("install") => {
            install_outcome(root, provider, install_apply_requested(&command_args[1..]))
        }
        Some("auth") => execute_auth(root, provider, &command_args[1..]),
        Some("config") => {
            owned.push("--app".to_string());
            owned.push(provider.as_str().to_string());
            owned.extend(command_args.iter().cloned());
            execute_compat(root, &owned)
        }
        Some("session") => {
            owned.push("--app".to_string());
            owned.push(provider.as_str().to_string());
            owned.extend(command_args.iter().cloned());
            execute_compat(root, &owned)
        }
        Some("workspace") => match command_args.get(1).map(String::as_str) {
            Some("list") => list_grants_outcome(root, provider),
            Some("grant") => {
                let path = parse_workspace_path_arg(&command_args[2..])
                    .context("usage: ctox coding-agent workspace grant [--path] <path>")?;
                grant_workspace(root, provider, path)
            }
            Some("revoke") => {
                let path = parse_workspace_path_arg(&command_args[2..])
                    .context("usage: ctox coding-agent workspace revoke [--path] <path>")?;
                revoke_workspace(root, provider, path)
            }
            _ => Ok(error_outcome(
                provider,
                "workspace",
                "usage: ctox coding-agent workspace list|grant|revoke [--path] <path>",
            )),
        },
        Some(other) => Ok(error_outcome(
            provider,
            other,
            format!("unsupported coding agent CLI subcommand '{other}'"),
        )),
        None => Ok(help_outcome()),
    }
}

fn parse_cli_args(args: &[String]) -> anyhow::Result<(Provider, Vec<String>)> {
    let mut provider = Provider::Mock;
    let mut rest = Vec::new();
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--provider" | "--app" | "-a" => {
                let raw = args.get(idx + 1).context("provider value is required")?;
                provider = Provider::parse(raw)?;
                idx += 2;
            }
            value => {
                rest.push(value.to_string());
                idx += 1;
            }
        }
    }
    Ok((provider, rest))
}

fn parse_workspace_path_arg(args: &[String]) -> Option<&str> {
    match args {
        [flag, path, ..] if flag.as_str() == "--path" || flag.as_str() == "-p" => {
            Some(path.as_str())
        }
        [path, ..] => Some(path.as_str()),
        [] => None,
    }
}

fn execute_compat(root: &Path, args: &[String]) -> anyhow::Result<Value> {
    let parsed = parse_compat_args(args)?;
    match parsed.args.first().map(String::as_str) {
        Some("status") => status_outcome(root, parsed.provider),
        Some("install") => install_outcome(
            root,
            parsed.provider,
            install_apply_requested(&parsed.args[1..]),
        ),
        Some("start") => lifecycle_outcome(parsed.provider, "start"),
        Some("stop") => lifecycle_outcome(parsed.provider, "stop"),
        Some("headless") => lifecycle_outcome(parsed.provider, "headless"),
        Some("signup") | Some("login") => auth_start_outcome(root, parsed.provider),
        Some("auth") => execute_auth(root, parsed.provider, &parsed.args[1..]),
        Some("config") => execute_config(root, parsed.provider, &parsed.args[1..]),
        Some("session") => execute_session(root, parsed.provider, &parsed.args[1..]),
        Some(other) => Ok(error_outcome(
            parsed.provider,
            other,
            format!("unsupported coding agent command '{other}'"),
        )),
        None => Ok(error_outcome(
            parsed.provider,
            "execute",
            "coding agent command requires a subcommand",
        )),
    }
}

fn parse_compat_args(args: &[String]) -> anyhow::Result<ParsedCommand> {
    let mut provider = Provider::Antigravity;
    let mut rest = Vec::new();
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--app" | "--provider" | "-a" => {
                let raw = args.get(idx + 1).context("--app requires a provider")?;
                provider = Provider::parse(raw)?;
                idx += 2;
            }
            value => {
                rest.push(value.to_string());
                idx += 1;
            }
        }
    }
    Ok(ParsedCommand {
        provider,
        args: rest,
    })
}

fn execute_auth(root: &Path, provider: Provider, args: &[String]) -> anyhow::Result<Value> {
    match args.first().map(String::as_str) {
        Some("status") => auth_status_outcome(root, provider),
        Some("start") | None => auth_start_outcome(root, provider),
        Some(other) if other.starts_with('-') => auth_start_outcome(root, provider),
        Some(other) => Ok(error_outcome(
            provider,
            "auth",
            format!("unsupported auth command '{other}'; use auth start or auth status"),
        )),
    }
}

fn execute_config(root: &Path, provider: Provider, args: &[String]) -> anyhow::Result<Value> {
    match args.first().map(String::as_str) {
        Some("get-grants") => list_grants_outcome(root, provider),
        Some("grant") => {
            let path =
                parse_workspace_path_arg(&args[1..]).context("config grant requires a path")?;
            grant_workspace(root, provider, path)
        }
        Some("revoke") => {
            let path =
                parse_workspace_path_arg(&args[1..]).context("config revoke requires a path")?;
            revoke_workspace(root, provider, path)
        }
        Some(other) => Ok(error_outcome(
            provider,
            "config",
            format!("unsupported config command '{other}'"),
        )),
        None => Ok(error_outcome(
            provider,
            "config",
            "config requires get-grants, grant, or revoke",
        )),
    }
}

fn execute_session(root: &Path, provider: Provider, args: &[String]) -> anyhow::Result<Value> {
    match args.first().map(String::as_str) {
        Some("create") => {
            let (workspace, prompt) = parse_session_create_args(&args[1..])?;
            create_session(root, provider, &workspace, &prompt)
        }
        Some("prompt") => {
            let (session_id, prompt) = parse_session_prompt_args(&args[1..])?;
            prompt_session(root, provider, &session_id, &prompt)
        }
        Some("list") => {
            let workspace = parse_optional_workspace_arg(&args[1..])?;
            list_sessions_outcome(root, provider, workspace.as_deref())
        }
        Some("get") => {
            let session_id =
                parse_session_id_arg(&args[1..]).context("session get requires a session id")?;
            get_session_outcome(root, provider, session_id)
        }
        Some("stop") => {
            let session_id =
                parse_session_id_arg(&args[1..]).context("session stop requires a session id")?;
            stop_session(root, provider, session_id)
        }
        Some(other) => Ok(error_outcome(
            provider,
            "session",
            format!("unsupported session command '{other}'"),
        )),
        None => Ok(error_outcome(
            provider,
            "session",
            "session requires create, prompt, list, get, or stop",
        )),
    }
}

fn parse_session_create_args(args: &[String]) -> anyhow::Result<(String, String)> {
    let mut workspace = None;
    let mut prompt = Vec::new();
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "-p" | "--project" | "--workspace" => {
                workspace = Some(
                    args.get(idx + 1)
                        .context("session create -p requires a workspace path")?
                        .to_string(),
                );
                idx += 2;
            }
            "--prompt" | "--message" => {
                prompt.push(
                    args.get(idx + 1)
                        .context("session create --prompt requires text")?
                        .to_string(),
                );
                idx += 2;
            }
            value => {
                prompt.push(value.to_string());
                idx += 1;
            }
        }
    }
    let workspace = workspace.context("session create requires -p <workspace>")?;
    let prompt = join_prompt(&prompt)?;
    Ok((workspace, prompt))
}

fn parse_session_prompt_args(args: &[String]) -> anyhow::Result<(String, String)> {
    let mut session_id = None;
    let mut prompt = Vec::new();
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--session" | "--session-id" => {
                session_id = Some(
                    args.get(idx + 1)
                        .context("session prompt --session requires an id")?
                        .to_string(),
                );
                idx += 2;
            }
            "--prompt" | "--message" => {
                prompt.push(
                    args.get(idx + 1)
                        .context("session prompt --prompt requires text")?
                        .to_string(),
                );
                idx += 2;
            }
            value if session_id.is_none() => {
                session_id = Some(value.to_string());
                idx += 1;
            }
            value => {
                prompt.push(value.to_string());
                idx += 1;
            }
        }
    }
    let session_id = session_id.context("session prompt requires a session id")?;
    let prompt = join_prompt(&prompt)?;
    Ok((session_id, prompt))
}

fn parse_session_id_arg(args: &[String]) -> Option<&str> {
    match args {
        [flag, id, ..] if flag.as_str() == "--session" || flag.as_str() == "--session-id" => {
            Some(id.as_str())
        }
        [id, ..] => Some(id.as_str()),
        [] => None,
    }
}

fn parse_optional_workspace_arg(args: &[String]) -> anyhow::Result<Option<String>> {
    let mut workspace = None;
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--workspace" | "--project" | "-p" => {
                workspace = Some(
                    args.get(idx + 1)
                        .context("session list --workspace requires a path")?
                        .to_string(),
                );
                idx += 2;
            }
            _ => {
                idx += 1;
            }
        }
    }
    Ok(workspace)
}

fn join_prompt(args: &[String]) -> anyhow::Result<String> {
    let prompt = args.join(" ").trim().to_string();
    if prompt.is_empty() {
        bail!("prompt text is required");
    }
    Ok(prompt)
}

fn install_apply_requested(args: &[String]) -> bool {
    args.iter()
        .any(|arg| matches!(arg.as_str(), "--apply" | "--yes" | "--confirm"))
}

fn install_outcome(root: &Path, provider: Provider, apply: bool) -> anyhow::Result<Value> {
    if provider == Provider::Mock {
        return Ok(success_outcome(
            provider,
            "install",
            "Mock provider is built in.\n".to_string(),
            json!({"installed": true, "mode": "mock"}),
        ));
    }

    let binary = find_executable(provider.binary_names());
    let app_bundle = find_app_bundle(provider.app_bundle_names());
    let installed = binary.is_some() || app_bundle.is_some();
    let status = status_outcome(root, provider)?;
    let plan = provider_install_plan(provider);
    if installed {
        return Ok(success_outcome(
            provider,
            "install",
            format!(
                "{} is already discoverable. Run `ctox coding-agent auth start --provider {}` if authorization is still needed.\n",
                provider.label(),
                provider.as_str(),
            ),
            json!({
                "installed": true,
                "binary": binary,
                "app_bundle": app_bundle,
                "install_plan": plan,
                "status": status.get("data").cloned().unwrap_or(Value::Null),
            }),
        ));
    }

    if !apply {
        return Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "install",
            "stdout": "",
            "stderr": format!("{} is not installed or not discoverable. Re-run with --apply to execute the provider installer.", provider.label()),
            "exit_code": 127,
            "data": {
                "installed": false,
                "apply_required": true,
                "install_plan": plan,
            }
        }));
    }

    match run_provider_installer(provider) {
        Ok(run) => {
            let status_after = status_outcome(root, provider)?;
            let installed_after = status_after
                .pointer("/data/installed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let installer_ok = run.exit_code == 0 && !run.timed_out;
            let ok = installer_ok && installed_after;
            let stdout = if ok {
                format!(
                    "{} installer completed and the provider is discoverable.\n",
                    provider.label()
                )
            } else {
                format!(
                    "{} installer ran, but the provider is not ready yet.\n",
                    provider.label()
                )
            };
            let stderr = if ok {
                String::new()
            } else if run.timed_out {
                format!(
                    "{} installer timed out after {INSTALL_TIMEOUT_SECS}s",
                    provider.label()
                )
            } else if !installer_ok {
                tail_text(&redact_provider_output(&run.stderr), 2000)
            } else {
                "Installer completed but the CLI is still not discoverable. Restart the shell or add the installer target directory to PATH.".to_string()
            };
            Ok(json!({
                "ok": ok,
                "provider": provider.as_str(),
                "operation": "install",
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": if ok { 0 } else if run.timed_out { 124 } else if run.exit_code != 0 { run.exit_code } else { 127 },
                "data": {
                    "installed": installed_after,
                    "install_plan": plan,
                    "installer": {
                        "exit_code": run.exit_code,
                        "timed_out": run.timed_out,
                        "stdout_tail": tail_text(&redact_provider_output(&run.stdout), 4000),
                        "stderr_tail": tail_text(&redact_provider_output(&run.stderr), 4000),
                    },
                    "status": status_after.get("data").cloned().unwrap_or(Value::Null),
                }
            }))
        }
        Err(error) => Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "install",
            "stdout": "",
            "stderr": error.to_string(),
            "exit_code": 1,
            "data": {
                "installed": false,
                "install_plan": plan,
            }
        })),
    }
}

fn provider_install_plan(provider: Provider) -> Value {
    json!({
        "docs": provider_install_docs_url(provider),
        "command": provider_install_shell_command(provider),
        "apply_supported": !cfg!(windows),
        "requires_user_intent": true,
    })
}

fn provider_install_docs_url(provider: Provider) -> &'static str {
    match provider {
        Provider::Antigravity => AGY_INSTALL_DOCS_URL,
        Provider::Claude => CLAUDE_INSTALL_DOCS_URL,
        Provider::Codex => CODEX_INSTALL_DOCS_URL,
        Provider::Mock => "",
    }
}

fn provider_install_shell_command(provider: Provider) -> &'static str {
    match provider {
        Provider::Antigravity => "curl -fsSL https://antigravity.google/cli/install.sh | bash",
        Provider::Claude => "curl -fsSL https://claude.ai/install.sh | bash",
        Provider::Codex => {
            "curl -fsSL https://chatgpt.com/codex/install.sh | CODEX_NON_INTERACTIVE=1 sh"
        }
        Provider::Mock => "",
    }
}

fn run_provider_installer(provider: Provider) -> anyhow::Result<ProcessRun> {
    if cfg!(windows) {
        bail!("automatic provider installation is not implemented for Windows yet; use the provider installation docs");
    }
    let command = provider_install_shell_command(provider);
    if command.trim().is_empty() {
        bail!("{} has no external installer", provider.label());
    }
    run_process(
        "/bin/sh",
        &["-c".to_string(), command.to_string()],
        Duration::from_secs(INSTALL_TIMEOUT_SECS),
    )
}

fn auth_start_outcome(root: &Path, provider: Provider) -> anyhow::Result<Value> {
    match provider {
        Provider::Mock => Ok(success_outcome(
            provider,
            "auth.start",
            "Mock provider requires no authorization.\n".to_string(),
            json!({"status": "ready", "mode": "mock"}),
        )),
        Provider::Antigravity => agy_auth_start_outcome(root),
        Provider::Claude => claude_auth_start_outcome(root),
        Provider::Codex => {
            let status = codex_auth_status(root)?;
            if status.ready {
                return Ok(success_outcome(
                    provider,
                    "auth.start",
                    "Codex is already authenticated.\n".to_string(),
                    status.to_json(),
                ));
            }
            Ok(success_outcome(
                provider,
                "auth.start",
                "Codex authorization requires the provider-owned login flow. Run `codex login` or `codex login --device-auth`, then retry `ctox coding-agent auth status --provider codex`.\n".to_string(),
                json!({
                    "status": "needs_user",
                    "binary": status.binary,
                    "auth_docs": CODEX_AUTH_DOCS_URL,
                    "commands": [
                        "codex login",
                        "codex login --device-auth",
                        "ctox coding-agent auth status --provider codex"
                    ],
                    "message": status.message,
                }),
            ))
        }
    }
}

fn auth_status_outcome(root: &Path, provider: Provider) -> anyhow::Result<Value> {
    match provider {
        Provider::Mock => Ok(success_outcome(
            provider,
            "auth.status",
            "Mock provider is authorized.\n".to_string(),
            json!({"status": "ready", "mode": "mock"}),
        )),
        Provider::Antigravity => agy_auth_status_outcome(root),
        Provider::Claude => claude_auth_status_outcome(root),
        Provider::Codex => {
            let status = codex_auth_status(root)?;
            let stdout = if status.ready {
                "Codex auth status: ready\n".to_string()
            } else {
                format!("Codex auth status: {}\n", status.state)
            };
            Ok(success_outcome(
                provider,
                "auth.status",
                stdout,
                status.to_json(),
            ))
        }
    }
}

fn status_outcome(root: &Path, provider: Provider) -> anyhow::Result<Value> {
    if provider == Provider::Antigravity {
        return agy_status_outcome(root);
    }
    if provider == Provider::Claude {
        return claude_status_outcome(root);
    }
    if provider == Provider::Codex {
        return codex_status_outcome(root);
    }

    let binary = find_executable(provider.binary_names());
    let app_bundle = find_app_bundle(provider.app_bundle_names());
    let installed = provider == Provider::Mock || binary.is_some() || app_bundle.is_some();
    let controllable = provider == Provider::Mock;
    let stdout = if installed {
        if controllable {
            format!(
                "Provider: {}\nElectron App: RUNNING (PID: 1)\nLanguage Server: RUNNING (PID: 1)\nActive Port: ctox-mock\nResources: SQLite session contract\nUptime: ready\nMode: mock\n",
                provider.label(),
            )
        } else {
            format!(
                "Provider: {}\nElectron App: STOPPED\nLanguage Server: STOPPED\nActive Port: N/A\nResources: Detected, adapter pending\nUptime: N/A\nMode: discover-only\n",
                provider.label(),
            )
        }
    } else {
        String::new()
    };
    let mut data = json!({
        "provider": provider.as_str(),
        "label": provider.label(),
        "installed": installed,
        "controllable": controllable,
        "mode": if controllable { "mock" } else { "discover-only" },
        "binary": binary,
        "app_bundle": app_bundle,
        "sessions": session_count(root, provider).unwrap_or(0),
    });
    if installed {
        Ok(success_outcome(provider, "status", stdout, data))
    } else {
        if let Some(obj) = data.as_object_mut() {
            obj.insert(
                "message".to_string(),
                Value::String(format!(
                    "{} is not installed or not discoverable",
                    provider.label()
                )),
            );
        }
        Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "status",
            "stdout": "",
            "stderr": format!("{} is not installed or not discoverable", provider.label()),
            "exit_code": 127,
            "data": data,
        }))
    }
}

fn codex_status_outcome(root: &Path) -> anyhow::Result<Value> {
    let provider = Provider::Codex;
    let binary = find_executable(provider.binary_names());
    let app_bundle = find_app_bundle(provider.app_bundle_names());
    let installed = binary.is_some() || app_bundle.is_some();
    if !installed {
        return Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "status",
            "stdout": "",
            "stderr": format!("{} is not installed or not discoverable", provider.label()),
            "exit_code": 127,
            "data": {
                "provider": provider.as_str(),
                "label": provider.label(),
                "installed": false,
                "controllable": false,
                "mode": "missing",
                "binary": binary,
                "app_bundle": app_bundle,
                "auth": {"status": "missing_binary"},
                "sessions": session_count(root, provider).unwrap_or(0),
            },
        }));
    }

    let auth = codex_auth_status(root)?;
    let version = binary
        .as_deref()
        .and_then(|path| {
            run_process(
                path,
                &[String::from("--version")],
                Duration::from_secs(CODEX_STATUS_TIMEOUT_SECS),
            )
            .ok()
        })
        .and_then(|run| {
            (run.exit_code == 0)
                .then(|| run.stdout.trim().to_string())
                .filter(|value| !value.is_empty())
        });
    let controllable = auth.ready && binary.is_some();
    let mode = if controllable {
        "codex-cli"
    } else if binary.is_some() {
        "needs-auth"
    } else {
        "discover-only"
    };
    let stdout = if controllable {
        format!(
            "Provider: {}\nElectron App: RUNNING (PID: 1)\nLanguage Server: RUNNING (PID: 1)\nActive Port: codex-cli\nResources: Codex CLI{}; SQLite session contract\nUptime: ready\nMode: codex-cli\nAuth: ready\n",
            provider.label(),
            version
                .as_deref()
                .map(|value| format!(" {value}"))
                .unwrap_or_default()
        )
    } else {
        format!(
            "Provider: {}\nElectron App: STOPPED\nLanguage Server: STOPPED\nActive Port: N/A\nResources: {}\nUptime: N/A\nMode: {mode}\nAuth: {}\n",
            provider.label(),
            if binary.is_some() {
                "Codex CLI detected; authorization required"
            } else {
                "Codex app detected; CLI not found"
            },
            auth.state
        )
    };
    Ok(success_outcome(
        provider,
        "status",
        stdout,
        json!({
            "provider": provider.as_str(),
            "label": provider.label(),
            "installed": installed,
            "controllable": controllable,
            "mode": mode,
            "binary": binary,
            "app_bundle": app_bundle,
            "version": version,
            "auth": auth.to_json(),
            "sessions": session_count(root, provider).unwrap_or(0),
        }),
    ))
}

fn codex_auth_status(_root: &Path) -> anyhow::Result<CodexAuthStatus> {
    let provider = Provider::Codex;
    let binary = find_executable(provider.binary_names());
    let Some(path) = binary.as_deref() else {
        return Ok(CodexAuthStatus {
            ready: false,
            state: "missing_binary".to_string(),
            binary,
            stdout: String::new(),
            stderr: format!("{} CLI is not installed or not on PATH", provider.label()),
            exit_code: 127,
            timed_out: false,
            message: "Install Codex before starting authorization.".to_string(),
        });
    };
    let run = run_process(
        path,
        &[String::from("login"), String::from("status")],
        Duration::from_secs(CODEX_STATUS_TIMEOUT_SECS),
    )?;
    let stdout = tail_text(&run.stdout, 2000);
    let stderr = tail_text(&run.stderr, 2000);
    let ready = run.exit_code == 0 && !run.timed_out;
    let state = if ready {
        "ready"
    } else if run.timed_out {
        "timeout"
    } else {
        "needs_user"
    };
    let message = if ready {
        stdout
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("Codex is authenticated.")
            .trim()
            .to_string()
    } else if run.timed_out {
        "Codex login status timed out.".to_string()
    } else {
        let detail = stderr.trim();
        if detail.is_empty() {
            "Codex is not authenticated. Run `codex login` or `codex login --device-auth`."
                .to_string()
        } else {
            detail.to_string()
        }
    };
    Ok(CodexAuthStatus {
        ready,
        state: state.to_string(),
        binary,
        stdout,
        stderr,
        exit_code: run.exit_code,
        timed_out: run.timed_out,
        message,
    })
}

fn claude_auth_start_outcome(root: &Path) -> anyhow::Result<Value> {
    let provider = Provider::Claude;
    let status = claude_auth_status(root)?;
    if status.ready {
        return Ok(success_outcome(
            provider,
            "auth.start",
            "Claude Code auth is ready.\n".to_string(),
            status.to_json(),
        ));
    }
    Ok(success_outcome(
        provider,
        "auth.start",
        "Claude Code authorization requires the provider-owned login flow. Run `claude auth login`, complete the browser prompt, then retry auth status.\n".to_string(),
        json!({
            "status": "needs_user",
            "binary": status.binary,
            "docs": CLAUDE_DOCS_URL,
            "commands": [
                "claude auth login",
                "claude auth status",
                "ctox coding-agent auth status --provider claude"
            ],
            "message": status.message,
        }),
    ))
}

fn claude_auth_status_outcome(root: &Path) -> anyhow::Result<Value> {
    let provider = Provider::Claude;
    let status = claude_auth_status(root)?;
    let stdout = if status.ready {
        "Claude Code auth status: ready\n".to_string()
    } else {
        format!("Claude Code auth status: {}\n", status.state)
    };
    Ok(success_outcome(
        provider,
        "auth.status",
        stdout,
        status.to_json(),
    ))
}

fn claude_status_outcome(root: &Path) -> anyhow::Result<Value> {
    let provider = Provider::Claude;
    let binary = find_executable(provider.binary_names());
    let app_bundle = find_app_bundle(provider.app_bundle_names());
    let installed = binary.is_some() || app_bundle.is_some();
    if !installed {
        return Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "status",
            "stdout": "",
            "stderr": format!("{} is not installed or not discoverable", provider.label()),
            "exit_code": 127,
            "data": {
                "provider": provider.as_str(),
                "label": provider.label(),
                "installed": false,
                "controllable": false,
                "mode": "missing",
                "binary": binary,
                "app_bundle": app_bundle,
                "auth": {"status": "missing_binary"},
                "sessions": session_count(root, provider).unwrap_or(0),
            },
        }));
    }

    let auth = claude_auth_status(root)?;
    let controllable = auth.ready && binary.is_some();
    let mode = if controllable {
        "claude-code-cli"
    } else if binary.is_some() {
        "needs-auth"
    } else {
        "discover-only"
    };
    let stdout = if controllable {
        format!(
            "Provider: {}\nElectron App: RUNNING (PID: 1)\nLanguage Server: RUNNING (PID: 1)\nActive Port: claude-cli\nResources: Claude Code CLI{}; SQLite session contract\nUptime: ready\nMode: claude-code-cli\nAuth: ready\n",
            provider.label(),
            auth.version
                .as_deref()
                .map(|value| format!(" {value}"))
                .unwrap_or_default()
        )
    } else {
        format!(
            "Provider: {}\nElectron App: STOPPED\nLanguage Server: STOPPED\nActive Port: N/A\nResources: {}\nUptime: N/A\nMode: {mode}\nAuth: {}\n",
            provider.label(),
            if binary.is_some() {
                "Claude Code CLI detected; authorization required"
            } else {
                "Claude desktop app detected; CLI not found"
            },
            auth.state
        )
    };
    Ok(success_outcome(
        provider,
        "status",
        stdout,
        json!({
            "provider": provider.as_str(),
            "label": provider.label(),
            "installed": installed,
            "controllable": controllable,
            "mode": mode,
            "binary": binary,
            "app_bundle": app_bundle,
            "version": auth.version.clone(),
            "auth": auth.to_json(),
            "sessions": session_count(root, provider).unwrap_or(0),
        }),
    ))
}

fn claude_auth_status(_root: &Path) -> anyhow::Result<ClaudeAuthStatus> {
    let provider = Provider::Claude;
    let binary = find_executable(provider.binary_names());
    let Some(path) = binary.as_deref() else {
        return Ok(ClaudeAuthStatus {
            ready: false,
            state: "missing_binary".to_string(),
            binary,
            version: None,
            auth_method: None,
            api_provider: None,
            subscription_type: None,
            stdout: String::new(),
            stderr: format!("{} CLI is not installed or not on PATH", provider.label()),
            exit_code: 127,
            timed_out: false,
            message: "Install Claude Code before starting authorization.".to_string(),
        });
    };
    let version = run_process(
        path,
        &[String::from("--version")],
        Duration::from_secs(CLAUDE_STATUS_TIMEOUT_SECS),
    )
    .ok()
    .and_then(|run| {
        (run.exit_code == 0)
            .then(|| run.stdout.trim().to_string())
            .filter(|value| !value.is_empty())
    });
    let run = run_process(
        path,
        &[String::from("auth"), String::from("status")],
        Duration::from_secs(CLAUDE_STATUS_TIMEOUT_SECS),
    )?;
    let auth_json = serde_json::from_str::<Value>(run.stdout.trim()).unwrap_or(Value::Null);
    let logged_in = auth_json
        .get("loggedIn")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let ready = run.exit_code == 0 && !run.timed_out && logged_in;
    let state = if ready {
        "ready"
    } else if run.timed_out {
        "timeout"
    } else {
        "needs_user"
    };
    let stdout = tail_text(&redact_provider_output(&run.stdout), 2000);
    let stderr = tail_text(&redact_provider_output(&run.stderr), 2000);
    let auth_method = auth_json
        .get("authMethod")
        .and_then(Value::as_str)
        .map(str::to_string);
    let api_provider = auth_json
        .get("apiProvider")
        .and_then(Value::as_str)
        .map(str::to_string);
    let subscription_type = auth_json
        .get("subscriptionType")
        .and_then(Value::as_str)
        .map(str::to_string);
    let message = if ready {
        format!(
            "Claude Code is authenticated via {}.",
            auth_method.as_deref().unwrap_or("provider auth")
        )
    } else if run.timed_out {
        "Claude Code auth status timed out.".to_string()
    } else {
        let detail = stderr.trim();
        if detail.is_empty() {
            "Claude Code is not authenticated. Run `claude auth login`.".to_string()
        } else {
            detail.to_string()
        }
    };
    Ok(ClaudeAuthStatus {
        ready,
        state: state.to_string(),
        binary,
        version,
        auth_method,
        api_provider,
        subscription_type,
        stdout,
        stderr,
        exit_code: run.exit_code,
        timed_out: run.timed_out,
        message,
    })
}

fn agy_auth_start_outcome(root: &Path) -> anyhow::Result<Value> {
    let provider = Provider::Antigravity;
    let status = agy_auth_status(root)?;
    if status.ready {
        return Ok(success_outcome(
            provider,
            "auth.start",
            "Antigravity CLI auth is ready.\n".to_string(),
            status.to_json(),
        ));
    }
    Ok(success_outcome(
        provider,
        "auth.start",
        "Antigravity authorization requires the provider-owned login flow. Run `agy` or `agy --print \"auth smoke\"` and complete the browser/keyring prompt, then retry auth status.\n".to_string(),
        json!({
            "status": "needs_user",
            "binary": status.binary,
            "docs": AGY_DOCS_URL,
            "commands": [
                "agy",
                "agy --print \"auth smoke\"",
                "ctox coding-agent auth status --provider antigravity"
            ],
            "message": status.message,
        }),
    ))
}

fn agy_auth_status_outcome(root: &Path) -> anyhow::Result<Value> {
    let provider = Provider::Antigravity;
    let status = agy_auth_status(root)?;
    let stdout = if status.ready {
        "Antigravity auth status: ready\n".to_string()
    } else {
        format!("Antigravity auth status: {}\n", status.state)
    };
    Ok(success_outcome(
        provider,
        "auth.status",
        stdout,
        status.to_json(),
    ))
}

fn agy_status_outcome(root: &Path) -> anyhow::Result<Value> {
    let provider = Provider::Antigravity;
    let binary = find_executable(provider.binary_names());
    let app_bundle = find_app_bundle(provider.app_bundle_names());
    let installed = binary.is_some() || app_bundle.is_some();
    if !installed {
        return Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "status",
            "stdout": "",
            "stderr": format!("{} is not installed or not discoverable", provider.label()),
            "exit_code": 127,
            "data": {
                "provider": provider.as_str(),
                "label": provider.label(),
                "installed": false,
                "controllable": false,
                "mode": "missing",
                "binary": binary,
                "app_bundle": app_bundle,
                "auth": {"status": "missing_binary"},
                "sessions": session_count(root, provider).unwrap_or(0),
            },
        }));
    }

    let auth = agy_auth_status(root)?;
    let controllable = auth.ready && binary.is_some();
    let mode = if controllable {
        "antigravity-cli"
    } else if binary.is_some() {
        "needs-auth"
    } else {
        "discover-only"
    };
    let stdout = if controllable {
        format!(
            "Provider: {}\nElectron App: RUNNING (PID: 1)\nLanguage Server: RUNNING (PID: 1)\nActive Port: agy-cli\nResources: Antigravity CLI{}; SQLite session contract\nUptime: ready\nMode: antigravity-cli\nAuth: ready\n",
            provider.label(),
            auth.version
                .as_deref()
                .map(|value| format!(" {value}"))
                .unwrap_or_default()
        )
    } else {
        format!(
            "Provider: {}\nElectron App: STOPPED\nLanguage Server: STOPPED\nActive Port: N/A\nResources: {}\nUptime: N/A\nMode: {mode}\nAuth: {}\n",
            provider.label(),
            if binary.is_some() {
                "Antigravity CLI detected; authorization required"
            } else {
                "Antigravity app detected; CLI not found"
            },
            auth.state
        )
    };
    Ok(success_outcome(
        provider,
        "status",
        stdout,
        json!({
            "provider": provider.as_str(),
            "label": provider.label(),
            "installed": installed,
            "controllable": controllable,
            "mode": mode,
            "binary": binary,
            "app_bundle": app_bundle,
            "version": auth.version.clone(),
            "auth": auth.to_json(),
            "sessions": session_count(root, provider).unwrap_or(0),
        }),
    ))
}

fn agy_auth_status(_root: &Path) -> anyhow::Result<AgyAuthStatus> {
    let provider = Provider::Antigravity;
    let binary = find_executable(provider.binary_names());
    let Some(path) = binary.as_deref() else {
        return Ok(AgyAuthStatus {
            ready: false,
            state: "missing_binary".to_string(),
            binary,
            version: None,
            models: Vec::new(),
            stdout: String::new(),
            stderr: format!("{} CLI is not installed or not on PATH", provider.label()),
            exit_code: 127,
            timed_out: false,
            message: "Install Antigravity CLI before starting authorization.".to_string(),
        });
    };
    let version = run_process(
        path,
        &[String::from("--version")],
        Duration::from_secs(AGY_STATUS_TIMEOUT_SECS),
    )
    .ok()
    .and_then(|run| {
        (run.exit_code == 0)
            .then(|| run.stdout.trim().to_string())
            .filter(|value| !value.is_empty())
    });
    let run = run_process(
        path,
        &[String::from("models")],
        Duration::from_secs(AGY_STATUS_TIMEOUT_SECS),
    )?;
    let stdout = tail_text(&redact_provider_output(&run.stdout), 2000);
    let stderr = tail_text(&redact_provider_output(&run.stderr), 2000);
    let models = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let ready = run.exit_code == 0 && !run.timed_out && !models.is_empty();
    let state = if ready {
        "ready"
    } else if run.timed_out {
        "timeout"
    } else {
        "needs_user"
    };
    let message = if ready {
        format!(
            "Antigravity CLI is authenticated; {} models available.",
            models.len()
        )
    } else if run.timed_out {
        "Antigravity models command timed out.".to_string()
    } else {
        let detail = stderr.trim();
        if detail.is_empty() {
            "Antigravity CLI is not authenticated. Run `agy` and complete provider login."
                .to_string()
        } else {
            detail.to_string()
        }
    };
    Ok(AgyAuthStatus {
        ready,
        state: state.to_string(),
        binary,
        version,
        models,
        stdout,
        stderr,
        exit_code: run.exit_code,
        timed_out: run.timed_out,
        message,
    })
}

fn providers_outcome(root: &Path) -> anyhow::Result<Value> {
    let providers = [
        Provider::Antigravity,
        Provider::Claude,
        Provider::Codex,
        Provider::Mock,
    ]
    .into_iter()
    .map(|provider| {
        let outcome = status_outcome(root, provider)?;
        Ok(outcome.get("data").cloned().unwrap_or(Value::Null))
    })
    .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(json!({
        "ok": true,
        "operation": "providers",
        "stdout": "",
        "stderr": "",
        "exit_code": 0,
        "data": { "providers": providers },
    }))
}

fn lifecycle_outcome(provider: Provider, operation: &str) -> anyhow::Result<Value> {
    if provider == Provider::Mock {
        return Ok(success_outcome(
            provider,
            operation,
            format!("{operation}: mock provider is ready\n"),
            json!({"mode": "mock"}),
        ));
    }
    Ok(error_outcome(
        provider,
        operation,
        format!(
            "{} lifecycle control is not implemented yet; CTOX only reports discovery for this provider",
            provider.label()
        ),
    ))
}

fn list_grants_outcome(root: &Path, provider: Provider) -> anyhow::Result<Value> {
    let grants = list_grants(root, provider)?;
    let stdout = if grants.is_empty() {
        "No workspace grants configured.\n".to_string()
    } else {
        grants
            .iter()
            .map(|path| format!("  * {path}\n"))
            .collect::<String>()
    };
    Ok(success_outcome(
        provider,
        "config.get-grants",
        stdout,
        json!({"grants": grants}),
    ))
}

fn grant_workspace(root: &Path, provider: Provider, path: &str) -> anyhow::Result<Value> {
    let normalized = normalize_workspace_path(path)?;
    let conn = open_conn(root)?;
    let now = now_ms();
    conn.execute(
        "INSERT INTO coding_agent_workspace_grants
            (provider, path, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT(provider, path) DO UPDATE SET updated_at_ms = excluded.updated_at_ms",
        params![provider.as_str(), normalized, now],
    )?;
    sync_provider_projection(root, provider)?;
    Ok(success_outcome(
        provider,
        "config.grant",
        format!("Granted workspace access: {normalized}\n"),
        json!({"path": normalized}),
    ))
}

fn revoke_workspace(root: &Path, provider: Provider, path: &str) -> anyhow::Result<Value> {
    let normalized = normalize_workspace_path(path)?;
    let conn = open_conn(root)?;
    let now = now_ms();
    conn.execute(
        "DELETE FROM coding_agent_workspace_grants WHERE provider = ?1 AND path = ?2",
        params![provider.as_str(), normalized],
    )?;
    project_workspace_grant(root, provider, &normalized, now, now, false)?;
    Ok(success_outcome(
        provider,
        "config.revoke",
        format!("Revoked workspace access: {normalized}\n"),
        json!({"path": normalized}),
    ))
}

fn create_session(
    root: &Path,
    provider: Provider,
    workspace: &str,
    prompt: &str,
) -> anyhow::Result<Value> {
    if provider == Provider::Antigravity {
        return create_agy_session(root, workspace, prompt);
    }
    if provider == Provider::Claude {
        return create_claude_session(root, workspace, prompt);
    }
    if provider == Provider::Codex {
        return create_codex_session(root, workspace, prompt);
    }

    let workspace = normalize_workspace_path(workspace)?;
    ensure_workspace_granted(root, provider, &workspace)?;
    let now = now_ms();
    let session_id = format!("ca_{}_{}", provider.as_str(), Uuid::new_v4().simple());
    let title = prompt.chars().take(80).collect::<String>();
    let conn = open_conn(root)?;
    conn.execute(
        "INSERT INTO coding_agent_sessions
            (session_id, provider, workspace_root, status, title, last_prompt, external_session_id, metadata_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, 'running', ?4, ?5, '', '{}', ?6, ?6)",
        params![session_id, provider.as_str(), workspace, title, prompt, now],
    )?;
    insert_event(&conn, &session_id, "user", prompt, "accepted", now)?;
    let response = format!(
        "{} accepted the task in CTOX mock mode. Real desktop-provider adapters can now target this persisted contract.",
        provider.label()
    );
    insert_event(
        &conn,
        &session_id,
        "assistant",
        &response,
        "completed",
        now + 1,
    )?;
    let stdout = format!(
        "Created session {session_id}\n[{}] User: {prompt}\n[{}] Assistant: {response}\n",
        format_time(now),
        format_time(now + 1)
    );
    sync_provider_projection(root, provider)?;
    Ok(success_outcome(
        provider,
        "session.create",
        stdout,
        json!({
            "session_id": session_id,
            "workspace_root": workspace,
            "status": "running",
            "mode": "mock-contract",
        }),
    ))
}

fn prompt_session(
    root: &Path,
    provider: Provider,
    session_id: &str,
    prompt: &str,
) -> anyhow::Result<Value> {
    if provider == Provider::Antigravity {
        return prompt_agy_session(root, session_id, prompt);
    }
    if provider == Provider::Claude {
        return prompt_claude_session(root, session_id, prompt);
    }
    if provider == Provider::Codex {
        return prompt_codex_session(root, session_id, prompt);
    }

    let conn = open_conn(root)?;
    let session = get_session(&conn, provider, session_id)?.with_context(|| {
        format!(
            "session '{session_id}' was not found for provider {}",
            provider.as_str()
        )
    })?;
    if session.status != "running" {
        return Ok(error_outcome(
            provider,
            "session.prompt",
            format!("session '{session_id}' is not running"),
        ));
    }
    let now = now_ms();
    insert_event(&conn, session_id, "user", prompt, "accepted", now)?;
    let response = format!(
        "{} recorded the follow-up in CTOX mock mode for workspace {}.",
        provider.label(),
        session.workspace_root
    );
    insert_event(
        &conn,
        session_id,
        "assistant",
        &response,
        "completed",
        now + 1,
    )?;
    conn.execute(
        "UPDATE coding_agent_sessions
         SET last_prompt = ?1, updated_at_ms = ?2
         WHERE session_id = ?3",
        params![prompt, now + 1, session_id],
    )?;
    sync_provider_projection(root, provider)?;
    Ok(success_outcome(
        provider,
        "session.prompt",
        format!(
            "[{}] User: {prompt}\n[{}] Assistant: {response}\n",
            format_time(now),
            format_time(now + 1)
        ),
        json!({
            "session_id": session_id,
            "status": "running",
            "mode": "mock-contract",
        }),
    ))
}

fn create_codex_session(root: &Path, workspace: &str, prompt: &str) -> anyhow::Result<Value> {
    let provider = Provider::Codex;
    let workspace = normalize_workspace_path(workspace)?;
    ensure_workspace_granted(root, provider, &workspace)?;
    let auth = codex_auth_status(root)?;
    if !auth.ready {
        return Ok(error_outcome(
            provider,
            "session.create",
            format!(
                "Codex is not authenticated ({status}). Run `ctox coding-agent auth start --provider codex` first.",
                status = auth.state
            ),
        ));
    }

    let started = now_ms();
    let session_id = format!("ca_codex_{}", Uuid::new_v4().simple());
    let title = prompt.chars().take(80).collect::<String>();
    let conn = open_conn(root)?;
    conn.execute(
        "INSERT INTO coding_agent_sessions
            (session_id, provider, workspace_root, status, title, last_prompt, external_session_id, metadata_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, 'running', ?4, ?5, '', '{}', ?6, ?6)",
        params![session_id, provider.as_str(), workspace, title, prompt, started],
    )?;
    insert_event(&conn, &session_id, "user", prompt, "accepted", started)?;

    let run = run_codex_exec(&workspace, prompt)?;
    let finished = now_ms();
    let success = run.process.exit_code == 0 && !run.process.timed_out;
    let thread_id = run.summary.thread_id.clone().unwrap_or_default();
    let response = codex_assistant_text(&run);
    insert_event(
        &conn,
        &session_id,
        "assistant",
        &response,
        if success { "completed" } else { "failed" },
        finished,
    )?;
    let session_status = if success || !thread_id.is_empty() {
        "running"
    } else {
        "failed"
    };
    let metadata = codex_run_metadata(&run);
    conn.execute(
        "UPDATE coding_agent_sessions
         SET status = ?1,
             external_session_id = ?2,
             metadata_json = ?3,
             updated_at_ms = ?4
         WHERE session_id = ?5",
        params![
            session_status,
            thread_id,
            serde_json::to_string(&metadata)?,
            finished,
            session_id
        ],
    )?;
    let stdout = format!(
        "Created session {session_id}\n[{}] User: {prompt}\n[{}] Assistant: {response}\n",
        format_time(started),
        format_time(finished)
    );
    let data = json!({
        "session_id": session_id,
        "workspace_root": workspace,
        "status": session_status,
        "mode": "codex-cli",
        "external_session_id": thread_id,
        "metadata": metadata,
    });
    sync_provider_projection(root, provider)?;
    if success {
        Ok(success_outcome(provider, "session.create", stdout, data))
    } else {
        Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "session.create",
            "stdout": stdout,
            "stderr": codex_error_text(&run),
            "exit_code": run.process.exit_code,
            "data": data,
        }))
    }
}

fn prompt_codex_session(root: &Path, session_id: &str, prompt: &str) -> anyhow::Result<Value> {
    let provider = Provider::Codex;
    let conn = open_conn(root)?;
    let session = get_session(&conn, provider, session_id)?.with_context(|| {
        format!(
            "session '{session_id}' was not found for provider {}",
            provider.as_str()
        )
    })?;
    if session.status != "running" {
        return Ok(error_outcome(
            provider,
            "session.prompt",
            format!("session '{session_id}' is not running"),
        ));
    }
    if session.external_session_id.trim().is_empty() {
        return Ok(error_outcome(
            provider,
            "session.prompt",
            format!("session '{session_id}' has no Codex thread id to resume"),
        ));
    }
    let auth = codex_auth_status(root)?;
    if !auth.ready {
        return Ok(error_outcome(
            provider,
            "session.prompt",
            format!(
                "Codex is not authenticated ({status}). Run `ctox coding-agent auth start --provider codex` first.",
                status = auth.state
            ),
        ));
    }

    let started = now_ms();
    insert_event(&conn, session_id, "user", prompt, "accepted", started)?;
    let run = run_codex_resume(
        &session.workspace_root,
        &session.external_session_id,
        prompt,
    )?;
    let finished = now_ms();
    let success = run.process.exit_code == 0 && !run.process.timed_out;
    let thread_id = run
        .summary
        .thread_id
        .clone()
        .unwrap_or_else(|| session.external_session_id.clone());
    let response = codex_assistant_text(&run);
    insert_event(
        &conn,
        session_id,
        "assistant",
        &response,
        if success { "completed" } else { "failed" },
        finished,
    )?;
    let metadata = codex_run_metadata(&run);
    conn.execute(
        "UPDATE coding_agent_sessions
         SET status = 'running',
             last_prompt = ?1,
             external_session_id = ?2,
             metadata_json = ?3,
             updated_at_ms = ?4
         WHERE session_id = ?5",
        params![
            prompt,
            thread_id,
            serde_json::to_string(&metadata)?,
            finished,
            session_id
        ],
    )?;
    let stdout = format!(
        "[{}] User: {prompt}\n[{}] Assistant: {response}\n",
        format_time(started),
        format_time(finished)
    );
    let data = json!({
        "session_id": session_id,
        "status": "running",
        "mode": "codex-cli",
        "external_session_id": thread_id,
        "metadata": metadata,
    });
    sync_provider_projection(root, provider)?;
    if success {
        Ok(success_outcome(provider, "session.prompt", stdout, data))
    } else {
        Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "session.prompt",
            "stdout": stdout,
            "stderr": codex_error_text(&run),
            "exit_code": run.process.exit_code,
            "data": data,
        }))
    }
}

fn create_claude_session(root: &Path, workspace: &str, prompt: &str) -> anyhow::Result<Value> {
    let provider = Provider::Claude;
    let workspace = normalize_workspace_path(workspace)?;
    ensure_workspace_granted(root, provider, &workspace)?;
    let auth = claude_auth_status(root)?;
    if !auth.ready {
        return Ok(error_outcome(
            provider,
            "session.create",
            format!(
                "Claude Code is not authenticated ({status}). Run `ctox coding-agent auth start --provider claude` first.",
                status = auth.state
            ),
        ));
    }

    let started = now_ms();
    let session_id = format!("ca_claude_{}", Uuid::new_v4().simple());
    let title = prompt.chars().take(80).collect::<String>();
    let conn = open_conn(root)?;
    conn.execute(
        "INSERT INTO coding_agent_sessions
            (session_id, provider, workspace_root, status, title, last_prompt, external_session_id, metadata_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, 'running', ?4, ?5, '', '{}', ?6, ?6)",
        params![session_id, provider.as_str(), workspace, title, prompt, started],
    )?;
    insert_event(&conn, &session_id, "user", prompt, "accepted", started)?;

    let run = run_claude_print(&workspace, prompt, None)?;
    let finished = now_ms();
    let success = claude_run_success(&run);
    let external_session_id = run.summary.session_id.clone().unwrap_or_default();
    let response = claude_assistant_text(&run);
    insert_event(
        &conn,
        &session_id,
        "assistant",
        &response,
        if success { "completed" } else { "failed" },
        finished,
    )?;
    let session_status = if success || !external_session_id.is_empty() {
        "running"
    } else {
        "failed"
    };
    let metadata = claude_run_metadata(&run);
    conn.execute(
        "UPDATE coding_agent_sessions
         SET status = ?1,
             external_session_id = ?2,
             metadata_json = ?3,
             updated_at_ms = ?4
         WHERE session_id = ?5",
        params![
            session_status,
            external_session_id,
            serde_json::to_string(&metadata)?,
            finished,
            session_id
        ],
    )?;
    let stdout = format!(
        "Created session {session_id}\n[{}] User: {prompt}\n[{}] Assistant: {response}\n",
        format_time(started),
        format_time(finished)
    );
    let data = json!({
        "session_id": session_id,
        "workspace_root": workspace,
        "status": session_status,
        "mode": "claude-code-cli",
        "external_session_id": external_session_id,
        "metadata": metadata,
    });
    sync_provider_projection(root, provider)?;
    if success {
        Ok(success_outcome(provider, "session.create", stdout, data))
    } else {
        Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "session.create",
            "stdout": stdout,
            "stderr": claude_error_text(&run),
            "exit_code": run.process.exit_code,
            "data": data,
        }))
    }
}

fn prompt_claude_session(root: &Path, session_id: &str, prompt: &str) -> anyhow::Result<Value> {
    let provider = Provider::Claude;
    let conn = open_conn(root)?;
    let session = get_session(&conn, provider, session_id)?.with_context(|| {
        format!(
            "session '{session_id}' was not found for provider {}",
            provider.as_str()
        )
    })?;
    if session.status != "running" {
        return Ok(error_outcome(
            provider,
            "session.prompt",
            format!("session '{session_id}' is not running"),
        ));
    }
    if session.external_session_id.trim().is_empty() {
        return Ok(error_outcome(
            provider,
            "session.prompt",
            format!("session '{session_id}' has no Claude Code session id to resume"),
        ));
    }
    let auth = claude_auth_status(root)?;
    if !auth.ready {
        return Ok(error_outcome(
            provider,
            "session.prompt",
            format!(
                "Claude Code is not authenticated ({status}). Run `ctox coding-agent auth start --provider claude` first.",
                status = auth.state
            ),
        ));
    }

    let started = now_ms();
    insert_event(&conn, session_id, "user", prompt, "accepted", started)?;
    let run = run_claude_print(
        &session.workspace_root,
        prompt,
        Some(&session.external_session_id),
    )?;
    let finished = now_ms();
    let success = claude_run_success(&run);
    let external_session_id = run
        .summary
        .session_id
        .clone()
        .unwrap_or_else(|| session.external_session_id.clone());
    let response = claude_assistant_text(&run);
    insert_event(
        &conn,
        session_id,
        "assistant",
        &response,
        if success { "completed" } else { "failed" },
        finished,
    )?;
    let metadata = claude_run_metadata(&run);
    conn.execute(
        "UPDATE coding_agent_sessions
         SET status = 'running',
             last_prompt = ?1,
             external_session_id = ?2,
             metadata_json = ?3,
             updated_at_ms = ?4
         WHERE session_id = ?5",
        params![
            prompt,
            external_session_id,
            serde_json::to_string(&metadata)?,
            finished,
            session_id
        ],
    )?;
    let stdout = format!(
        "[{}] User: {prompt}\n[{}] Assistant: {response}\n",
        format_time(started),
        format_time(finished)
    );
    let data = json!({
        "session_id": session_id,
        "status": "running",
        "mode": "claude-code-cli",
        "external_session_id": external_session_id,
        "metadata": metadata,
    });
    sync_provider_projection(root, provider)?;
    if success {
        Ok(success_outcome(provider, "session.prompt", stdout, data))
    } else {
        Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "session.prompt",
            "stdout": stdout,
            "stderr": claude_error_text(&run),
            "exit_code": run.process.exit_code,
            "data": data,
        }))
    }
}

fn create_agy_session(root: &Path, workspace: &str, prompt: &str) -> anyhow::Result<Value> {
    let provider = Provider::Antigravity;
    let workspace = normalize_workspace_path(workspace)?;
    ensure_workspace_granted(root, provider, &workspace)?;
    let auth = agy_auth_status(root)?;
    if !auth.ready {
        return Ok(error_outcome(
            provider,
            "session.create",
            format!(
                "Antigravity is not authenticated ({status}). Run `ctox coding-agent auth start --provider antigravity` first.",
                status = auth.state
            ),
        ));
    }

    let started = now_ms();
    let session_id = format!("ca_antigravity_{}", Uuid::new_v4().simple());
    let title = prompt.chars().take(80).collect::<String>();
    let conn = open_conn(root)?;
    conn.execute(
        "INSERT INTO coding_agent_sessions
            (session_id, provider, workspace_root, status, title, last_prompt, external_session_id, metadata_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, 'running', ?4, ?5, '', '{}', ?6, ?6)",
        params![session_id, provider.as_str(), workspace, title, prompt, started],
    )?;
    insert_event(&conn, &session_id, "user", prompt, "accepted", started)?;

    let run = run_agy_print(root, &workspace, prompt, None)?;
    let finished = now_ms();
    let success = run.process.exit_code == 0 && !run.process.timed_out;
    let conversation_id = run.conversation_id.clone().unwrap_or_default();
    let response = agy_assistant_text(&run);
    insert_event(
        &conn,
        &session_id,
        "assistant",
        &response,
        if success { "completed" } else { "failed" },
        finished,
    )?;
    let session_status = if success || !conversation_id.is_empty() {
        "running"
    } else {
        "failed"
    };
    let metadata = agy_run_metadata(&run);
    conn.execute(
        "UPDATE coding_agent_sessions
         SET status = ?1,
             external_session_id = ?2,
             metadata_json = ?3,
             updated_at_ms = ?4
         WHERE session_id = ?5",
        params![
            session_status,
            conversation_id,
            serde_json::to_string(&metadata)?,
            finished,
            session_id
        ],
    )?;
    let stdout = format!(
        "Created session {session_id}\n[{}] User: {prompt}\n[{}] Assistant: {response}\n",
        format_time(started),
        format_time(finished)
    );
    let data = json!({
        "session_id": session_id,
        "workspace_root": workspace,
        "status": session_status,
        "mode": "antigravity-cli",
        "external_session_id": conversation_id,
        "metadata": metadata,
    });
    sync_provider_projection(root, provider)?;
    if success {
        Ok(success_outcome(provider, "session.create", stdout, data))
    } else {
        Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "session.create",
            "stdout": stdout,
            "stderr": agy_error_text(&run),
            "exit_code": run.process.exit_code,
            "data": data,
        }))
    }
}

fn prompt_agy_session(root: &Path, session_id: &str, prompt: &str) -> anyhow::Result<Value> {
    let provider = Provider::Antigravity;
    let conn = open_conn(root)?;
    let session = get_session(&conn, provider, session_id)?.with_context(|| {
        format!(
            "session '{session_id}' was not found for provider {}",
            provider.as_str()
        )
    })?;
    if session.status != "running" {
        return Ok(error_outcome(
            provider,
            "session.prompt",
            format!("session '{session_id}' is not running"),
        ));
    }
    if session.external_session_id.trim().is_empty() {
        return Ok(error_outcome(
            provider,
            "session.prompt",
            format!("session '{session_id}' has no Antigravity conversation id to resume"),
        ));
    }
    let auth = agy_auth_status(root)?;
    if !auth.ready {
        return Ok(error_outcome(
            provider,
            "session.prompt",
            format!(
                "Antigravity is not authenticated ({status}). Run `ctox coding-agent auth start --provider antigravity` first.",
                status = auth.state
            ),
        ));
    }

    let started = now_ms();
    insert_event(&conn, session_id, "user", prompt, "accepted", started)?;
    let run = run_agy_print(
        root,
        &session.workspace_root,
        prompt,
        Some(&session.external_session_id),
    )?;
    let finished = now_ms();
    let success = run.process.exit_code == 0 && !run.process.timed_out;
    let conversation_id = run
        .conversation_id
        .clone()
        .unwrap_or_else(|| session.external_session_id.clone());
    let response = agy_assistant_text_for_session(&run, &conn, &session)?;
    insert_event(
        &conn,
        session_id,
        "assistant",
        &response,
        if success { "completed" } else { "failed" },
        finished,
    )?;
    let metadata = agy_run_metadata(&run);
    conn.execute(
        "UPDATE coding_agent_sessions
         SET status = 'running',
             last_prompt = ?1,
             external_session_id = ?2,
             metadata_json = ?3,
             updated_at_ms = ?4
         WHERE session_id = ?5",
        params![
            prompt,
            conversation_id,
            serde_json::to_string(&metadata)?,
            finished,
            session_id
        ],
    )?;
    let stdout = format!(
        "[{}] User: {prompt}\n[{}] Assistant: {response}\n",
        format_time(started),
        format_time(finished)
    );
    let data = json!({
        "session_id": session_id,
        "status": "running",
        "mode": "antigravity-cli",
        "external_session_id": conversation_id,
        "metadata": metadata,
    });
    sync_provider_projection(root, provider)?;
    if success {
        Ok(success_outcome(provider, "session.prompt", stdout, data))
    } else {
        Ok(json!({
            "ok": false,
            "provider": provider.as_str(),
            "operation": "session.prompt",
            "stdout": stdout,
            "stderr": agy_error_text(&run),
            "exit_code": run.process.exit_code,
            "data": data,
        }))
    }
}

fn list_sessions_outcome(
    root: &Path,
    provider: Provider,
    workspace: Option<&str>,
) -> anyhow::Result<Value> {
    let workspace = workspace.map(normalize_workspace_path).transpose()?;
    let sessions = list_sessions(root, provider, workspace.as_deref())?;
    let mut stdout = String::from("SHORT ID | ID | UPDATED | PROMPT\n");
    stdout.push_str("=== | === | === | ===\n");
    for session in &sessions {
        stdout.push_str(&format!(
            "{} | {} | {} | {}\n",
            short_session_id(&session.session_id),
            session.session_id,
            format_time(session.updated_at_ms),
            session.last_prompt.replace('\n', " ")
        ));
    }
    Ok(success_outcome(
        provider,
        "session.list",
        stdout,
        json!({
            "sessions": sessions.iter().map(session_json).collect::<Vec<_>>(),
        }),
    ))
}

fn get_session_outcome(root: &Path, provider: Provider, session_id: &str) -> anyhow::Result<Value> {
    let conn = open_conn(root)?;
    let session = get_session(&conn, provider, session_id)?.with_context(|| {
        format!(
            "session '{session_id}' was not found for provider {}",
            provider.as_str()
        )
    })?;
    let events = list_events(&conn, session_id)?;
    let stdout = events
        .iter()
        .map(|event| {
            format!(
                "[{}] {}: {}\n",
                format_time(event.created_at_ms),
                role_label(&event.role),
                event.text.replace('\n', " ")
            )
        })
        .collect::<String>();
    Ok(success_outcome(
        provider,
        "session.get",
        stdout,
        json!({
            "session": session_json(&session),
            "events": events.iter().map(event_json).collect::<Vec<_>>(),
        }),
    ))
}

fn stop_session(root: &Path, provider: Provider, session_id: &str) -> anyhow::Result<Value> {
    let conn = open_conn(root)?;
    let existing = get_session(&conn, provider, session_id)?;
    if existing.is_none() {
        return Ok(error_outcome(
            provider,
            "session.stop",
            format!("session '{session_id}' was not found"),
        ));
    }
    let now = now_ms();
    conn.execute(
        "UPDATE coding_agent_sessions
         SET status = 'stopped', updated_at_ms = ?1
         WHERE provider = ?2 AND session_id = ?3",
        params![now, provider.as_str(), session_id],
    )?;
    sync_provider_projection(root, provider)?;
    Ok(success_outcome(
        provider,
        "session.stop",
        format!("Stopped session {session_id}\n"),
        json!({"session_id": session_id, "status": "stopped"}),
    ))
}

fn ensure_workspace_granted(
    root: &Path,
    provider: Provider,
    workspace: &str,
) -> anyhow::Result<()> {
    let conn = open_conn(root)?;
    let exists: Option<String> = conn
        .query_row(
            "SELECT path FROM coding_agent_workspace_grants WHERE provider = ?1 AND path = ?2",
            params![provider.as_str(), workspace],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        bail!(
            "workspace '{workspace}' is not granted for provider {}; grant it first",
            provider.as_str()
        );
    }
    Ok(())
}

fn list_grants(root: &Path, provider: Provider) -> anyhow::Result<Vec<String>> {
    let conn = open_conn(root)?;
    let mut stmt = conn.prepare(
        "SELECT path FROM coding_agent_workspace_grants
         WHERE provider = ?1
         ORDER BY path COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![provider.as_str()], |row| row.get::<_, String>(0))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn list_sessions(
    root: &Path,
    provider: Provider,
    workspace: Option<&str>,
) -> anyhow::Result<Vec<CodingSession>> {
    let conn = open_conn(root)?;
    if let Some(workspace) = workspace {
        let mut stmt = conn.prepare(
            "SELECT session_id, provider, workspace_root, status, title, last_prompt, external_session_id, metadata_json, updated_at_ms
             FROM coding_agent_sessions
             WHERE provider = ?1 AND workspace_root = ?2
             ORDER BY updated_at_ms DESC",
        )?;
        let rows = stmt.query_map(params![provider.as_str(), workspace], session_from_row)?;
        return rows.collect::<Result<Vec<_>, _>>().map_err(Into::into);
    }
    let mut stmt = conn.prepare(
        "SELECT session_id, provider, workspace_root, status, title, last_prompt, external_session_id, metadata_json, updated_at_ms
         FROM coding_agent_sessions
         WHERE provider = ?1
         ORDER BY updated_at_ms DESC",
    )?;
    let rows = stmt.query_map(params![provider.as_str()], session_from_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn session_count(root: &Path, provider: Provider) -> anyhow::Result<i64> {
    let conn = open_conn(root)?;
    conn.query_row(
        "SELECT COUNT(*) FROM coding_agent_sessions WHERE provider = ?1",
        params![provider.as_str()],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn get_session(
    conn: &Connection,
    provider: Provider,
    session_id: &str,
) -> anyhow::Result<Option<CodingSession>> {
    conn.query_row(
        "SELECT session_id, provider, workspace_root, status, title, last_prompt, external_session_id, metadata_json, updated_at_ms
         FROM coding_agent_sessions
         WHERE provider = ?1 AND session_id = ?2",
        params![provider.as_str(), session_id],
        session_from_row,
    )
    .optional()
    .map_err(Into::into)
}

fn sync_provider_projection(root: &Path, provider: Provider) -> anyhow::Result<()> {
    let conn = open_conn(root)?;
    let mut grant_stmt = conn.prepare(
        "SELECT provider, path, created_at_ms, updated_at_ms
         FROM coding_agent_workspace_grants
         WHERE provider = ?1",
    )?;
    let grants = grant_stmt.query_map(params![provider.as_str()], |row| {
        let provider_raw: String = row.get(0)?;
        Ok(CodingGrant {
            provider: Provider::parse(&provider_raw).unwrap_or(Provider::Mock),
            path: row.get(1)?,
            created_at_ms: row.get(2)?,
            updated_at_ms: row.get(3)?,
        })
    })?;
    for grant in grants {
        let grant = grant?;
        project_workspace_grant(
            root,
            grant.provider,
            &grant.path,
            grant.created_at_ms,
            grant.updated_at_ms,
            true,
        )?;
    }

    let sessions = list_sessions(root, provider, None)?;
    for session in &sessions {
        project_session(root, session)?;
        for event in list_events(&conn, &session.session_id)? {
            project_event(root, session.provider, &event)?;
        }
    }
    Ok(())
}

fn project_workspace_grant(
    root: &Path,
    provider: Provider,
    path: &str,
    created_at_ms: i64,
    updated_at_ms: i64,
    active: bool,
) -> anyhow::Result<()> {
    let record_id = workspace_grant_record_id(provider, path);
    crate::business_os::store::upsert_projection_record(
        root,
        "coding_agent_workspace_grants",
        &record_id,
        updated_at_ms,
        json!({
            "id": record_id,
            "provider": provider.as_str(),
            "path": path,
            "active": active,
            "status": if active { "active" } else { "revoked" },
            "created_at_ms": created_at_ms,
            "updated_at_ms": updated_at_ms,
            "is_deleted": false,
        }),
    )
}

fn project_session(root: &Path, session: &CodingSession) -> anyhow::Result<()> {
    let record_id = session.session_id.clone();
    let mut payload = session_json(session);
    if let Some(object) = payload.as_object_mut() {
        object.insert("id".to_string(), Value::String(record_id.clone()));
        object.insert("is_deleted".to_string(), Value::Bool(false));
    }
    crate::business_os::store::upsert_projection_record(
        root,
        "coding_agent_sessions",
        &record_id,
        session.updated_at_ms,
        payload,
    )
}

fn project_event(root: &Path, provider: Provider, event: &CodingEvent) -> anyhow::Result<()> {
    let record_id = event.event_id.clone();
    crate::business_os::store::upsert_projection_record(
        root,
        "coding_agent_events",
        &record_id,
        event.created_at_ms,
        json!({
            "id": record_id,
            "event_id": event.event_id,
            "session_id": event.session_id,
            "provider": provider.as_str(),
            "role": event.role,
            "text": event.text,
            "status": event.status,
            "created_at_ms": event.created_at_ms,
            "updated_at_ms": event.created_at_ms,
            "is_deleted": false,
        }),
    )
}

fn workspace_grant_record_id(provider: Provider, path: &str) -> String {
    format!("ca_grant_{}_{}", provider.as_str(), stable_hash_hex(path))
}

fn stable_hash_hex(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn list_events(conn: &Connection, session_id: &str) -> anyhow::Result<Vec<CodingEvent>> {
    let mut stmt = conn.prepare(
        "SELECT event_id, session_id, role, text, status, created_at_ms
         FROM coding_agent_events
         WHERE session_id = ?1
         ORDER BY seq ASC",
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok(CodingEvent {
            event_id: row.get(0)?,
            session_id: row.get(1)?,
            role: row.get(2)?,
            text: row.get(3)?,
            status: row.get(4)?,
            created_at_ms: row.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn insert_event(
    conn: &Connection,
    session_id: &str,
    role: &str,
    text: &str,
    status: &str,
    created_at_ms: i64,
) -> anyhow::Result<()> {
    let seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM coding_agent_events WHERE session_id = ?1",
        params![session_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO coding_agent_events
            (event_id, session_id, seq, role, text, status, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            format!("ca_evt_{}", Uuid::new_v4().simple()),
            session_id,
            seq,
            role,
            text,
            status,
            created_at_ms
        ],
    )?;
    Ok(())
}

fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CodingSession> {
    let provider_raw: String = row.get(1)?;
    let provider = Provider::parse(&provider_raw).unwrap_or(Provider::Mock);
    Ok(CodingSession {
        session_id: row.get(0)?,
        provider,
        workspace_root: row.get(2)?,
        status: row.get(3)?,
        title: row.get(4)?,
        last_prompt: row.get(5)?,
        external_session_id: row.get(6)?,
        metadata: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or(Value::Null),
        updated_at_ms: row.get(8)?,
    })
}

fn open_conn(root: &Path) -> anyhow::Result<Connection> {
    let db_path = crate::paths::core_db(root);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime directory {}", parent.display()))?;
    }
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open core db {}", db_path.display()))?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS coding_agent_workspace_grants (
            provider TEXT NOT NULL,
            path TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (provider, path)
        );

        CREATE TABLE IF NOT EXISTS coding_agent_sessions (
            session_id TEXT PRIMARY KEY,
            provider TEXT NOT NULL,
            workspace_root TEXT NOT NULL,
            status TEXT NOT NULL,
            title TEXT NOT NULL,
            last_prompt TEXT NOT NULL,
            external_session_id TEXT NOT NULL DEFAULT '',
            metadata_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_coding_agent_sessions_provider_workspace
            ON coding_agent_sessions(provider, workspace_root, updated_at_ms);

        CREATE TABLE IF NOT EXISTS coding_agent_events (
            event_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            role TEXT NOT NULL,
            text TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            UNIQUE(session_id, seq)
        );
        CREATE INDEX IF NOT EXISTS idx_coding_agent_events_session
            ON coding_agent_events(session_id, seq);
        ",
    )?;
    ensure_column(
        conn,
        "coding_agent_sessions",
        "external_session_id",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "coding_agent_sessions",
        "metadata_json",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let columns = rows.collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|existing| existing == column) {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn payload_provider(payload: &Value) -> anyhow::Result<Provider> {
    payload
        .get("provider")
        .or_else(|| payload.get("app"))
        .and_then(Value::as_str)
        .map(Provider::parse)
        .transpose()?
        .context("provider is required")
}

fn payload_string(payload: &Value, key: &str) -> anyhow::Result<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .with_context(|| format!("payload.{key} is required"))
}

fn payload_bool(payload: &Value, key: &str) -> bool {
    payload.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn normalize_workspace_path(raw: &str) -> anyhow::Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("workspace path is required");
    }
    if trimmed.contains('\0') {
        bail!("workspace path must not contain NUL bytes");
    }
    let path = Path::new(trimmed);
    if !path.is_absolute() {
        bail!("workspace path must be absolute: {trimmed}");
    }
    Ok(path.to_string_lossy().to_string())
}

fn success_outcome(provider: Provider, operation: &str, stdout: String, data: Value) -> Value {
    json!({
        "ok": true,
        "provider": provider.as_str(),
        "operation": operation,
        "stdout": stdout,
        "stderr": "",
        "exit_code": 0,
        "data": data,
    })
}

fn error_outcome(provider: Provider, operation: &str, message: impl Into<String>) -> Value {
    let message = message.into();
    json!({
        "ok": false,
        "provider": provider.as_str(),
        "operation": operation,
        "stdout": "",
        "stderr": message,
        "exit_code": 1,
    })
}

fn help_outcome() -> Value {
    json!({
        "ok": true,
        "operation": "help",
        "stdout": "ctox coding-agent [--provider mock|codex|claude|antigravity] status|providers|install [--apply]|auth|workspace|config|session\n",
        "stderr": "",
        "exit_code": 0,
    })
}

fn session_json(session: &CodingSession) -> Value {
    json!({
        "session_id": session.session_id,
        "provider": session.provider.as_str(),
        "workspace_root": session.workspace_root,
        "status": session.status,
        "title": session.title,
        "last_prompt": session.last_prompt,
        "external_session_id": session.external_session_id,
        "metadata": session.metadata.clone(),
        "updated_at_ms": session.updated_at_ms,
    })
}

fn event_json(event: &CodingEvent) -> Value {
    json!({
        "event_id": event.event_id,
        "session_id": event.session_id,
        "role": event.role,
        "text": event.text,
        "status": event.status,
        "created_at_ms": event.created_at_ms,
    })
}

fn role_label(role: &str) -> &'static str {
    match role {
        "assistant" => "Assistant",
        "tool" => "Tool",
        _ => "User",
    }
}

fn short_session_id(session_id: &str) -> String {
    session_id
        .rsplit('_')
        .next()
        .unwrap_or(session_id)
        .chars()
        .take(8)
        .collect()
}

fn format_time(ms: i64) -> String {
    ms.to_string()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn run_agy_print(
    _root: &Path,
    workspace: &str,
    prompt: &str,
    conversation_id: Option<&str>,
) -> anyhow::Result<AgyRun> {
    let binary = agy_binary()?;
    let log_path = std::env::temp_dir().join(format!("ctox-agy-{}.log", Uuid::new_v4().simple()));
    let args = agy_print_args(&log_path, prompt, conversation_id);
    let process = run_process_in_dir(
        &binary,
        &args,
        Duration::from_secs(AGY_EXEC_TIMEOUT_SECS),
        Some(Path::new(workspace)),
    )?;
    let log = std::fs::read_to_string(&log_path).unwrap_or_default();
    let _ = std::fs::remove_file(&log_path);
    let parsed_conversation_id =
        parse_agy_conversation_id(&log).or_else(|| conversation_id.map(str::to_string));
    Ok(AgyRun {
        process,
        conversation_id: parsed_conversation_id,
        log,
    })
}

fn agy_binary() -> anyhow::Result<String> {
    find_executable(Provider::Antigravity.binary_names())
        .context("Google Antigravity CLI is not installed or not on PATH")
}

fn agy_print_args(log_path: &Path, prompt: &str, conversation_id: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "--log-file".to_string(),
        log_path.to_string_lossy().to_string(),
        "--print-timeout".to_string(),
        format!("{AGY_EXEC_TIMEOUT_SECS}s"),
    ];
    if let Some(conversation_id) = conversation_id {
        args.push("--conversation".to_string());
        args.push(conversation_id.to_string());
    }
    args.extend(["--print".to_string(), prompt.to_string()]);
    args
}

fn parse_agy_conversation_id(log: &str) -> Option<String> {
    for line in log.lines().rev() {
        if let Some(raw) = line.split("conversation=").nth(1) {
            let id = raw
                .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ')')
                .next()
                .unwrap_or("")
                .trim();
            if looks_like_uuid(id) {
                return Some(id.to_string());
            }
        }
        if let Some(raw) = line.split("Created conversation ").nth(1) {
            let id = raw.split_whitespace().next().unwrap_or("").trim();
            if looks_like_uuid(id) {
                return Some(id.to_string());
            }
        }
        if let Some(raw) = line
            .split("GetConversationDetail: found conversation ")
            .nth(1)
        {
            let id = raw.split_whitespace().next().unwrap_or("").trim();
            if looks_like_uuid(id) {
                return Some(id.to_string());
            }
        }
    }
    None
}

fn looks_like_uuid(value: &str) -> bool {
    value.len() == 36
        && value.chars().enumerate().all(|(idx, ch)| {
            if matches!(idx, 8 | 13 | 18 | 23) {
                ch == '-'
            } else {
                ch.is_ascii_hexdigit()
            }
        })
}

fn agy_assistant_text(run: &AgyRun) -> String {
    let stdout = run.process.stdout.trim();
    if !stdout.is_empty() {
        return stdout.to_string();
    }
    if run.process.exit_code == 0 && !run.process.timed_out {
        return "Antigravity CLI completed without an assistant message.".to_string();
    }
    format!("Antigravity CLI failed: {}", agy_error_text(run))
}

fn agy_assistant_text_for_session(
    run: &AgyRun,
    conn: &Connection,
    session: &CodingSession,
) -> anyhow::Result<String> {
    let response = run.process.stdout.trim().to_string();
    if response.is_empty() {
        return Ok(agy_assistant_text(run));
    }
    let prior_stdout_tail = session
        .metadata
        .pointer("/antigravity/stdout_tail")
        .and_then(Value::as_str);
    let events = list_events(conn, &session.session_id)?;
    let response = strip_agy_prior_output(&response, prior_stdout_tail, &events);
    if response.is_empty() {
        Ok(agy_assistant_text(run))
    } else {
        Ok(response)
    }
}

fn strip_agy_prior_output(
    response: &str,
    prior_stdout_tail: Option<&str>,
    prior_events: &[CodingEvent],
) -> String {
    let mut response = response.trim().to_string();
    if response.is_empty() {
        return response;
    }

    if let Some(prior_stdout_tail) = prior_stdout_tail
        .map(str::trim)
        .filter(|prior| !prior.is_empty() && !prior.starts_with("..."))
    {
        if response == prior_stdout_tail {
            return String::new();
        }
        if let Some(rest) = response.strip_prefix(prior_stdout_tail) {
            response = rest.trim_start_matches(['\n', '\r']).trim().to_string();
        }
    }

    let mut prior_texts = prior_events
        .iter()
        .filter(|event| event.role == "assistant")
        .map(|event| event.text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();
    prior_texts.sort_by_key(|text| std::cmp::Reverse(text.len()));
    loop {
        let mut changed = false;
        for prior in &prior_texts {
            if response == *prior {
                return String::new();
            }
            if let Some(rest) = response.strip_prefix(prior) {
                response = rest.trim_start_matches(['\n', '\r']).trim().to_string();
                changed = true;
                break;
            }
        }
        if !changed {
            break;
        }
    }

    strip_agy_prior_lines(&response, prior_events)
}

fn strip_agy_prior_lines(response: &str, prior_events: &[CodingEvent]) -> String {
    let response_lines = response
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if response_lines.is_empty() {
        return String::new();
    }

    let mut prior_lines = Vec::new();
    for event in prior_events
        .iter()
        .filter(|event| event.role == "assistant")
    {
        for line in event
            .text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            if !prior_lines.contains(&line) {
                prior_lines.push(line);
            }
        }
    }

    let mut idx = 0;
    while idx < response_lines.len()
        && idx < prior_lines.len()
        && response_lines[idx] == prior_lines[idx]
    {
        idx += 1;
    }
    response_lines[idx..].join("\n")
}

fn agy_error_text(run: &AgyRun) -> String {
    if run.process.timed_out {
        return format!("Antigravity CLI timed out after {AGY_EXEC_TIMEOUT_SECS}s");
    }
    let stderr = run.process.stderr.trim();
    if !stderr.is_empty() {
        return tail_text(&redact_provider_output(stderr), 2000);
    }
    let log = run.log.trim();
    if !log.is_empty() {
        return tail_text(&redact_provider_output(log), 2000);
    }
    format!(
        "Antigravity CLI exited with status {}",
        run.process.exit_code
    )
}

fn agy_run_metadata(run: &AgyRun) -> Value {
    json!({
        "antigravity": {
            "conversation_id": run.conversation_id.clone(),
            "exit_code": run.process.exit_code,
            "timed_out": run.process.timed_out,
            "stdout_tail": tail_text(&redact_provider_output(&run.process.stdout), 4000),
            "stderr_tail": tail_text(&redact_provider_output(&run.process.stderr), 4000),
            "log_tail": tail_text(&redact_provider_output(&run.log), 4000),
        }
    })
}

fn run_claude_print(
    workspace: &str,
    prompt: &str,
    resume_session_id: Option<&str>,
) -> anyhow::Result<ClaudeRun> {
    let binary = claude_binary()?;
    let args = claude_print_args(prompt, resume_session_id);
    let process = run_process_in_dir(
        &binary,
        &args,
        Duration::from_secs(CLAUDE_EXEC_TIMEOUT_SECS),
        Some(Path::new(workspace)),
    )?;
    let summary = parse_claude_json(&process.stdout);
    Ok(ClaudeRun { process, summary })
}

fn claude_binary() -> anyhow::Result<String> {
    find_executable(Provider::Claude.binary_names())
        .context("Anthropic Claude Code CLI is not installed or not on PATH")
}

fn claude_print_args(prompt: &str, resume_session_id: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        "--output-format".to_string(),
        "json".to_string(),
        "--permission-mode".to_string(),
        "acceptEdits".to_string(),
    ];
    if let Some(resume_session_id) = resume_session_id {
        args.push("--resume".to_string());
        args.push(resume_session_id.to_string());
    }
    args.push(prompt.to_string());
    args
}

fn parse_claude_json(stdout: &str) -> ClaudeRunSummary {
    let mut summary = ClaudeRunSummary::default();
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return summary;
    }
    let value = serde_json::from_str::<Value>(trimmed).unwrap_or_else(|_| {
        trimmed
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str::<Value>(line).ok())
            .unwrap_or(Value::Null)
    });
    summary.session_id = value
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    summary.result = value
        .get("result")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    summary.type_name = value
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_string);
    summary.subtype = value
        .get("subtype")
        .and_then(Value::as_str)
        .map(str::to_string);
    summary.is_error = value.get("is_error").and_then(Value::as_bool);
    summary.api_error_status = value
        .get("api_error_status")
        .and_then(Value::as_str)
        .map(str::to_string);
    summary.stop_reason = value
        .get("stop_reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    summary.terminal_reason = value
        .get("terminal_reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    summary.total_cost_usd = value.get("total_cost_usd").cloned().unwrap_or(Value::Null);
    summary.usage = value.get("usage").cloned().unwrap_or(Value::Null);
    summary.model_usage = value.get("modelUsage").cloned().unwrap_or(Value::Null);
    summary.permission_denials = value
        .get("permission_denials")
        .cloned()
        .unwrap_or(Value::Null);
    summary.uuid = value
        .get("uuid")
        .and_then(Value::as_str)
        .map(str::to_string);
    summary
}

fn claude_run_success(run: &ClaudeRun) -> bool {
    run.process.exit_code == 0
        && !run.process.timed_out
        && run.summary.type_name.as_deref() == Some("result")
        && run.summary.subtype.as_deref() == Some("success")
        && run.summary.is_error != Some(true)
}

fn claude_assistant_text(run: &ClaudeRun) -> String {
    if let Some(message) = run.summary.result.as_deref() {
        return message.to_string();
    }
    if claude_run_success(run) {
        return "Claude Code completed without an assistant message.".to_string();
    }
    format!("Claude Code failed: {}", claude_error_text(run))
}

fn claude_error_text(run: &ClaudeRun) -> String {
    if run.process.timed_out {
        return format!("Claude Code CLI timed out after {CLAUDE_EXEC_TIMEOUT_SECS}s");
    }
    let stderr = run.process.stderr.trim();
    if !stderr.is_empty() {
        return tail_text(&redact_provider_output(stderr), 2000);
    }
    if let Some(status) = run.summary.api_error_status.as_deref() {
        return format!("Claude Code API error: {status}");
    }
    let stdout = run.process.stdout.trim();
    if !stdout.is_empty() {
        return tail_text(&redact_provider_output(stdout), 2000);
    }
    format!(
        "Claude Code CLI exited with status {}",
        run.process.exit_code
    )
}

fn claude_run_metadata(run: &ClaudeRun) -> Value {
    json!({
        "claude": {
            "session_id": run.summary.session_id.clone(),
            "type": run.summary.type_name.clone(),
            "subtype": run.summary.subtype.clone(),
            "is_error": run.summary.is_error,
            "api_error_status": run.summary.api_error_status.clone(),
            "stop_reason": run.summary.stop_reason.clone(),
            "terminal_reason": run.summary.terminal_reason.clone(),
            "total_cost_usd": run.summary.total_cost_usd.clone(),
            "usage": run.summary.usage.clone(),
            "model_usage": run.summary.model_usage.clone(),
            "permission_denials": run.summary.permission_denials.clone(),
            "uuid": run.summary.uuid.clone(),
            "exit_code": run.process.exit_code,
            "timed_out": run.process.timed_out,
            "stdout_tail": tail_text(&redact_provider_output(&run.process.stdout), 4000),
            "stderr_tail": tail_text(&redact_provider_output(&run.process.stderr), 4000),
        }
    })
}

fn run_codex_exec(workspace: &str, prompt: &str) -> anyhow::Result<CodexRun> {
    let binary = codex_binary()?;
    let args = codex_exec_args(workspace, prompt);
    let process = run_process(&binary, &args, Duration::from_secs(CODEX_EXEC_TIMEOUT_SECS))?;
    let summary = parse_codex_jsonl(&process.stdout);
    Ok(CodexRun { process, summary })
}

fn run_codex_resume(workspace: &str, thread_id: &str, prompt: &str) -> anyhow::Result<CodexRun> {
    let binary = codex_binary()?;
    let args = codex_resume_args(workspace, thread_id, prompt);
    let process = run_process(&binary, &args, Duration::from_secs(CODEX_EXEC_TIMEOUT_SECS))?;
    let summary = parse_codex_jsonl(&process.stdout);
    Ok(CodexRun { process, summary })
}

fn codex_binary() -> anyhow::Result<String> {
    find_executable(Provider::Codex.binary_names())
        .context("OpenAI Codex CLI is not installed or not on PATH")
}

fn codex_exec_args(workspace: &str, prompt: &str) -> Vec<String> {
    vec![
        "-a".to_string(),
        "never".to_string(),
        "exec".to_string(),
        "--json".to_string(),
        "--sandbox".to_string(),
        "workspace-write".to_string(),
        "--cd".to_string(),
        workspace.to_string(),
        "--skip-git-repo-check".to_string(),
        prompt.to_string(),
    ]
}

fn codex_resume_args(workspace: &str, thread_id: &str, prompt: &str) -> Vec<String> {
    vec![
        "-a".to_string(),
        "never".to_string(),
        "-s".to_string(),
        "workspace-write".to_string(),
        "-C".to_string(),
        workspace.to_string(),
        "exec".to_string(),
        "resume".to_string(),
        "--json".to_string(),
        "--skip-git-repo-check".to_string(),
        thread_id.to_string(),
        prompt.to_string(),
    ]
}

fn parse_codex_jsonl(stdout: &str) -> CodexRunSummary {
    let mut summary = CodexRunSummary::default();
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        summary.event_count += 1;
        match value.get("type").and_then(Value::as_str).unwrap_or("") {
            "thread.started" => {
                summary.thread_id = value
                    .get("thread_id")
                    .or_else(|| value.pointer("/thread/id"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or(summary.thread_id);
            }
            "item.completed" => {
                if let Some(item) = value.get("item") {
                    if matches!(
                        item.get("type").and_then(Value::as_str),
                        Some("agent_message" | "message")
                    ) {
                        if let Some(text) = extract_codex_text(item) {
                            summary.final_message = Some(text);
                        }
                    }
                }
            }
            "turn.completed" => {
                summary.turn_status = value
                    .get("status")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| Some("completed".to_string()));
                summary.usage = value
                    .get("usage")
                    .cloned()
                    .or_else(|| value.pointer("/metrics/usage").cloned())
                    .unwrap_or(Value::Null);
            }
            "turn.failed" => {
                summary.turn_status = Some("failed".to_string());
            }
            _ => {}
        }
    }
    summary
}

fn extract_codex_text(item: &Value) -> Option<String> {
    if let Some(text) = item.get("text").and_then(Value::as_str) {
        let trimmed = text.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    let content = item.get("content").and_then(Value::as_array)?;
    let parts = content
        .iter()
        .filter_map(|part| {
            part.get("text")
                .or_else(|| part.get("content"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn codex_assistant_text(run: &CodexRun) -> String {
    if let Some(message) = run
        .summary
        .final_message
        .as_deref()
        .map(str::trim)
        .filter(|message| !message.is_empty())
    {
        return message.to_string();
    }
    if run.process.exit_code == 0 && !run.process.timed_out {
        return "Codex CLI completed without an assistant message.".to_string();
    }
    format!("Codex CLI failed: {}", codex_error_text(run))
}

fn codex_error_text(run: &CodexRun) -> String {
    if run.process.timed_out {
        return format!("Codex CLI timed out after {CODEX_EXEC_TIMEOUT_SECS}s");
    }
    let stderr = run.process.stderr.trim();
    if !stderr.is_empty() {
        return tail_text(stderr, 2000);
    }
    let stdout = run.process.stdout.trim();
    if !stdout.is_empty() {
        return tail_text(stdout, 2000);
    }
    format!("Codex CLI exited with status {}", run.process.exit_code)
}

fn codex_run_metadata(run: &CodexRun) -> Value {
    json!({
        "codex": {
            "thread_id": run.summary.thread_id.clone(),
            "turn_status": run.summary.turn_status.clone(),
            "usage": run.summary.usage.clone(),
            "event_count": run.summary.event_count,
            "exit_code": run.process.exit_code,
            "timed_out": run.process.timed_out,
            "stdout_tail": tail_text(&redact_provider_output(&run.process.stdout), 4000),
            "stderr_tail": tail_text(&redact_provider_output(&run.process.stderr), 4000),
        }
    })
}

fn run_process(program: &str, args: &[String], timeout: Duration) -> anyhow::Result<ProcessRun> {
    run_process_in_dir(program, args, timeout, None)
}

fn run_process_in_dir(
    program: &str,
    args: &[String],
    timeout: Duration,
    current_dir: Option<&Path>,
) -> anyhow::Result<ProcessRun> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to spawn {program}"))?;
    let stdout_pipe = child.stdout.take().context("failed to capture stdout")?;
    let stderr_pipe = child.stderr.take().context("failed to capture stderr")?;
    let stdout_handle = read_pipe(stdout_pipe);
    let stderr_handle = read_pipe(stderr_pipe);
    let started = Instant::now();
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (status, false);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            break (child.wait()?, true);
        }
        thread::sleep(Duration::from_millis(50));
    };
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    Ok(ProcessRun {
        exit_code: status.code().unwrap_or(if timed_out { 124 } else { 1 }),
        stdout: String::from_utf8_lossy(&stdout).to_string(),
        stderr: String::from_utf8_lossy(&stderr).to_string(),
        timed_out,
    })
}

fn read_pipe<R: Read + Send + 'static>(mut pipe: R) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = pipe.read_to_end(&mut buffer);
        buffer
    })
}

fn tail_text(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    let suffix = text
        .chars()
        .rev()
        .take(max_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("...{suffix}")
}

fn redact_provider_output(text: &str) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut token = String::new();
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !token.is_empty() {
                redacted.push_str(&redact_provider_token(&token));
                token.clear();
            }
            redacted.push(ch);
        } else {
            token.push(ch);
        }
    }
    if !token.is_empty() {
        redacted.push_str(&redact_provider_token(&token));
    }
    redacted
}

fn redact_provider_token(token: &str) -> String {
    let trimmed = token.trim_matches(|ch: char| {
        matches!(
            ch,
            '"' | '\'' | ',' | ';' | ')' | '(' | '[' | ']' | '{' | '}'
        )
    });
    if let Some(redacted) = redact_email_substrings(token) {
        return redacted;
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some(email) = lower
        .starts_with("email=")
        .then(|| trimmed.split_once('=').map(|(_, value)| value))
        .flatten()
        .filter(|value| value.contains('@') && value.contains('.'))
    {
        return token.replace(email, "[redacted-email]");
    }
    if lower.starts_with("bearer")
        || lower.starts_with("token=")
        || lower.starts_with("access_token=")
        || lower.starts_with("refresh_token=")
    {
        return token.replace(trimmed, "[redacted-secret]");
    }
    token.to_string()
}

fn redact_email_substrings(token: &str) -> Option<String> {
    let mut redacted = String::new();
    let mut changed = false;
    let mut cursor = 0;
    for (idx, ch) in token.char_indices() {
        if ch != '@' || idx < cursor {
            continue;
        }
        let start = token[..idx]
            .char_indices()
            .rev()
            .find_map(|(pos, ch)| (!is_email_char(ch)).then_some(pos + ch.len_utf8()))
            .unwrap_or(0);
        let end = token[idx + ch.len_utf8()..]
            .char_indices()
            .find_map(|(offset, ch)| (!is_email_char(ch)).then_some(idx + ch.len_utf8() + offset))
            .unwrap_or(token.len());
        let candidate = &token[start..end];
        if !candidate.contains('.') {
            continue;
        }
        redacted.push_str(&token[cursor..start]);
        redacted.push_str("[redacted-email]");
        cursor = end;
        changed = true;
    }
    if changed {
        redacted.push_str(&token[cursor..]);
        Some(redacted)
    } else {
        None
    }
}

fn is_email_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '@' | '.' | '-' | '_' | '+')
}

fn find_executable(names: &[&str]) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    let mut dirs = std::env::var_os("PATH")
        .map(|path_var| std::env::split_paths(&path_var).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/bin"));
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));

    for dir in dirs {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn find_app_bundle(names: &[&str]) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    for root in roots {
        for name in names {
            let candidate = root.join(name);
            if candidate.is_dir() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn mock_provider_grants_workspace_and_records_session() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let workspace = workspace.to_string_lossy().to_string();

        let grant = grant_workspace(root, Provider::Mock, &workspace)?;
        assert_eq!(grant.get("ok").and_then(Value::as_bool), Some(true));

        let created = create_session(root, Provider::Mock, &workspace, "Implement the test task")?;
        let session_id = created
            .pointer("/data/session_id")
            .and_then(Value::as_str)
            .context("session id missing")?
            .to_string();

        let prompted = prompt_session(root, Provider::Mock, &session_id, "Continue")?;
        assert_eq!(prompted.get("ok").and_then(Value::as_bool), Some(true));

        let session = get_session_outcome(root, Provider::Mock, &session_id)?;
        let stdout = session.get("stdout").and_then(Value::as_str).unwrap_or("");
        assert!(stdout.contains("User: Implement the test task"));
        assert!(stdout.contains("User: Continue"));
        Ok(())
    }

    #[test]
    fn projection_records_are_written_to_business_os_store() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let workspace = workspace.to_string_lossy().to_string();

        grant_workspace(root, Provider::Mock, &workspace)?;
        let created = create_session(root, Provider::Mock, &workspace, "Projection smoke")?;
        let session_id = created
            .pointer("/data/session_id")
            .and_then(Value::as_str)
            .context("session id missing")?
            .to_string();
        prompt_session(root, Provider::Mock, &session_id, "Projection follow-up")?;
        revoke_workspace(root, Provider::Mock, &workspace)?;

        let conn = Connection::open(root.join("runtime").join("business-os.sqlite3"))?;
        let session_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'coding_agent_sessions'",
            [],
            |row| row.get(0),
        )?;
        let event_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'coding_agent_events'",
            [],
            |row| row.get(0),
        )?;
        let grant_payload: String = conn.query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'coding_agent_workspace_grants'
             LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        let grant_doc: Value = serde_json::from_str(&grant_payload)?;

        assert_eq!(session_count, 1);
        assert!(event_count >= 4);
        assert_eq!(
            grant_doc.get("status").and_then(Value::as_str),
            Some("revoked")
        );
        assert_eq!(
            grant_doc.get("active").and_then(Value::as_bool),
            Some(false)
        );
        Ok(())
    }

    #[test]
    fn compatibility_execute_supports_legacy_session_create_shape() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let workspace = workspace.to_string_lossy().to_string();
        grant_workspace(root, Provider::Mock, &workspace)?;

        let outcome = execute_compat(
            root,
            &[
                "--app".to_string(),
                "mock".to_string(),
                "session".to_string(),
                "create".to_string(),
                "-p".to_string(),
                workspace,
                "Initial prompt".to_string(),
            ],
        )?;

        assert_eq!(outcome.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            outcome.get("operation").and_then(Value::as_str),
            Some("session.create")
        );
        Ok(())
    }

    #[test]
    fn cli_provider_flag_can_follow_subcommand() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let workspace = workspace.to_string_lossy().to_string();

        let grant = execute_cli(
            root,
            &[
                "workspace".to_string(),
                "grant".to_string(),
                "--provider".to_string(),
                "mock".to_string(),
                workspace,
            ],
        )?;

        assert_eq!(grant.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            grant.get("operation").and_then(Value::as_str),
            Some("config.grant")
        );
        Ok(())
    }

    #[test]
    fn install_apply_requires_explicit_flag() {
        assert!(!install_apply_requested(&["install".to_string()]));
        assert!(install_apply_requested(&["--apply".to_string()]));
        assert!(install_apply_requested(&["--yes".to_string()]));

        let codex_plan = provider_install_plan(Provider::Codex);
        assert_eq!(
            codex_plan.get("docs").and_then(Value::as_str),
            Some(CODEX_INSTALL_DOCS_URL)
        );
        assert!(codex_plan
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .contains("chatgpt.com/codex/install.sh"));

        let agy_plan = provider_install_plan(Provider::Antigravity);
        assert_eq!(
            agy_plan.get("docs").and_then(Value::as_str),
            Some(AGY_INSTALL_DOCS_URL)
        );

        let claude_plan = provider_install_plan(Provider::Claude);
        assert_eq!(
            claude_plan.get("docs").and_then(Value::as_str),
            Some(CLAUDE_INSTALL_DOCS_URL)
        );
    }

    #[test]
    fn workspace_list_business_command_returns_structured_grants() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let workspace = root.join("workspace with spaces");
        std::fs::create_dir_all(&workspace)?;
        let workspace = workspace.to_string_lossy().to_string();
        grant_workspace(root, Provider::Mock, &workspace)?;

        let command = BusinessCommand {
            origin: crate::business_os::store::CommandOrigin::TrustedLocal,
            id: Some("cmd_workspace_list".to_string()),
            module: "coding-agents".to_string(),
            command_type: "ctox.coding_agent.workspace.list".to_string(),
            record_id: None,
            payload: json!({"provider": "mock"}),
            client_context: Value::Null,
        };
        let outcome = handle_business_command(root, &command)?;

        assert_eq!(outcome.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            outcome.pointer("/data/grants/0").and_then(Value::as_str),
            Some(workspace.as_str())
        );
        Ok(())
    }

    #[test]
    fn relative_workspace_paths_are_rejected() {
        let error = normalize_workspace_path("relative/path").unwrap_err();
        assert!(error.to_string().contains("absolute"));
    }

    #[test]
    fn codex_jsonl_parser_extracts_thread_and_final_message() {
        let summary = parse_codex_jsonl(
            r#"{"type":"thread.started","thread_id":"thread_123"}
{"type":"item.completed","item":{"type":"agent_message","text":"done"}}
{"type":"turn.completed","usage":{"total_tokens":7}}"#,
        );

        assert_eq!(summary.thread_id.as_deref(), Some("thread_123"));
        assert_eq!(summary.final_message.as_deref(), Some("done"));
        assert_eq!(summary.turn_status.as_deref(), Some("completed"));
        assert_eq!(
            summary
                .usage
                .pointer("/total_tokens")
                .and_then(Value::as_i64),
            Some(7)
        );
        assert_eq!(summary.event_count, 3);
    }

    #[test]
    fn codex_exec_args_keep_approval_before_exec() {
        let args = codex_exec_args("/tmp/project", "do work");
        assert_eq!(&args[0..3], ["-a", "never", "exec"]);
        assert!(args
            .windows(2)
            .any(|window| window == ["--cd", "/tmp/project"]));
        assert!(args
            .windows(2)
            .any(|window| window == ["--sandbox", "workspace-write"]));
        assert_eq!(args.last().map(String::as_str), Some("do work"));

        let resume = codex_resume_args("/tmp/project", "thread_123", "continue");
        assert!(resume
            .windows(2)
            .any(|window| window == ["-C", "/tmp/project"]));
        assert!(resume
            .windows(2)
            .any(|window| window == ["-s", "workspace-write"]));
        assert!(resume
            .windows(3)
            .any(|window| window == ["exec", "resume", "--json"]));
        assert_eq!(resume[resume.len() - 2], "thread_123");
        assert_eq!(resume.last().map(String::as_str), Some("continue"));
    }

    #[test]
    fn claude_json_parser_extracts_session_and_result() {
        let summary = parse_claude_json(
            r#"{"type":"result","subtype":"success","is_error":false,"result":"done","session_id":"550e8400-e29b-41d4-a716-446655440000","total_cost_usd":0.01,"usage":{"input_tokens":3},"permission_denials":[],"terminal_reason":"completed"}"#,
        );

        assert_eq!(
            summary.session_id.as_deref(),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
        assert_eq!(summary.result.as_deref(), Some("done"));
        assert_eq!(summary.subtype.as_deref(), Some("success"));
        assert_eq!(summary.is_error, Some(false));
        assert_eq!(
            summary
                .usage
                .pointer("/input_tokens")
                .and_then(Value::as_i64),
            Some(3)
        );
    }

    #[test]
    fn claude_print_args_resume_by_session() {
        let args = claude_print_args("continue", Some("550e8400-e29b-41d4-a716-446655440000"));
        assert!(args
            .windows(2)
            .any(|window| window == ["-p", "--output-format"]));
        assert!(args
            .windows(2)
            .any(|window| window == ["--output-format", "json"]));
        assert!(args
            .windows(2)
            .any(|window| window == ["--permission-mode", "acceptEdits"]));
        assert!(args
            .windows(2)
            .any(|window| window == ["--resume", "550e8400-e29b-41d4-a716-446655440000"]));
        assert_eq!(args.last().map(String::as_str), Some("continue"));
    }

    #[test]
    fn agy_log_parser_extracts_conversation_id() {
        let log = r#"
I0612 20:59:13.299257 server.go:753] Created conversation 03d49c8c-91f0-4f00-8315-19d14548ad37
I0612 20:59:13.308288 printmode.go:147] Print mode: conversation=03d49c8c-91f0-4f00-8315-19d14548ad37, sending message
"#;

        assert_eq!(
            parse_agy_conversation_id(log).as_deref(),
            Some("03d49c8c-91f0-4f00-8315-19d14548ad37")
        );
    }

    #[test]
    fn agy_print_args_resume_by_conversation() {
        let args = agy_print_args(
            Path::new("/tmp/agy.log"),
            "continue",
            Some("conversation-1"),
        );
        assert!(args
            .windows(2)
            .any(|window| window == ["--log-file", "/tmp/agy.log"]));
        assert!(args
            .windows(2)
            .any(|window| window == ["--conversation", "conversation-1"]));
        assert!(args
            .windows(2)
            .any(|window| window == ["--print", "continue"]));
    }

    #[test]
    fn agy_resume_output_strips_prior_stdout_history() {
        let prior_events = vec![
            CodingEvent {
                event_id: "event-1".to_string(),
                session_id: "session-1".to_string(),
                role: "assistant".to_string(),
                text: "CTOX_AGY_WRAPPER_SMOKE".to_string(),
                status: "completed".to_string(),
                created_at_ms: 1,
            },
            CodingEvent {
                event_id: "event-2".to_string(),
                session_id: "session-1".to_string(),
                role: "assistant".to_string(),
                text: "CTOX_AGY_WRAPPER_SMOKE\nCTOX_AGY_RESUME_SMOKE".to_string(),
                status: "completed".to_string(),
                created_at_ms: 2,
            },
        ];
        let clean = strip_agy_prior_output(
            "CTOX_AGY_WRAPPER_SMOKE\nCTOX_AGY_RESUME_SMOKE\nCTOX_AGY_RESUME2_SMOKE\n",
            Some("CTOX_AGY_WRAPPER_SMOKE\nCTOX_AGY_RESUME_SMOKE\n"),
            &prior_events,
        );

        assert_eq!(clean, "CTOX_AGY_RESUME2_SMOKE");

        let fallback = strip_agy_prior_output(
            "CTOX_AGY_WRAPPER_SMOKE\nCTOX_AGY_RESUME_SMOKE\nCTOX_AGY_RESUME2_SMOKE\n",
            Some("...truncated"),
            &prior_events,
        );
        assert_eq!(fallback, "CTOX_AGY_RESUME2_SMOKE");
    }

    #[test]
    fn provider_output_redaction_masks_email_and_tokens() {
        let redacted = redact_provider_output(
            "OAuth as person@example.com email=other@example.com token=abc access_token=def",
        );
        assert!(redacted.contains("[redacted-email]"));
        assert!(redacted.contains("[redacted-secret]"));
        assert!(!redacted.contains("person@example.com"));
        assert!(!redacted.contains("other@example.com"));
        assert!(!redacted.contains("token=abc"));

        let org_name = redact_provider_output(r#""orgName": "person@example.com's Organization""#);
        assert!(org_name.contains("[redacted-email]'s Organization"));
        assert!(!org_name.contains("person@example.com"));

        let multiline = redact_provider_output("Model A\nModel B");
        assert_eq!(multiline, "Model A\nModel B");
    }
}
