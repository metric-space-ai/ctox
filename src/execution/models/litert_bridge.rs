use crate::inference::engine;
use crate::inference::local_transport::LocalStream;
use crate::inference::local_transport::LocalTransport;
use crate::inference::model_adapters;
use anyhow::Context;
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Deserialize)]
pub struct LiteRtBridgeConfig {
    #[serde(default)]
    pub cli_path: Option<String>,
    pub model_reference: String,
    #[serde(default)]
    pub model_file: Option<String>,
    #[serde(default)]
    pub huggingface_repo: Option<String>,
    #[serde(default)]
    pub huggingface_token: Option<String>,
    pub backend: String,
    pub context_tokens: u32,
    #[serde(default)]
    pub validated_context_tokens: Option<u32>,
    /// Unix-domain socket path. Used on macOS / Linux. Ignored on Windows
    /// unless `tcp_port` is also unset (in which case bind fails with a clear
    /// Unsupported error).
    #[serde(default)]
    pub socket_path: Option<String>,
    /// TCP loopback port. Required on Windows; optional on Unix where it
    /// takes precedence over `socket_path` when both are set. Enables the
    /// bridge to run behind `LocalTransport::TcpLoopback`.
    #[serde(default)]
    pub tcp_port: Option<u16>,
    #[serde(default)]
    pub speculative_decoding: Option<String>,
    #[serde(default)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct LocalSocketResponsesRequest {
    #[serde(default)]
    instructions: String,
    input: Vec<Value>,
    #[serde(default)]
    tools: Vec<Value>,
    #[serde(default)]
    tool_choice: String,
    #[serde(default)]
    parallel_tool_calls: bool,
    #[serde(default)]
    max_output_tokens: Option<usize>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    service_tier: Option<String>,
    #[serde(default)]
    prompt_cache_key: Option<String>,
    #[serde(default)]
    text: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiteRtPromptStyle {
    Gemma4,
    Qwen35,
}

pub fn serve_from_config_path(_root: &Path, config_path: &Path) -> anyhow::Result<()> {
    let raw = std::fs::read(config_path).with_context(|| {
        format!(
            "failed to read LiteRT bridge config {}",
            config_path.display()
        )
    })?;
    let config: LiteRtBridgeConfig =
        serde_json::from_slice(&raw).context("failed to parse LiteRT bridge config")?;
    serve(config)
}

fn serve(config: LiteRtBridgeConfig) -> anyhow::Result<()> {
    if let Some(validated_context_tokens) = config.validated_context_tokens {
        if config.context_tokens > validated_context_tokens {
            anyhow::bail!(
                "LiteRT bridge for {} is only validated to {} tokens, but CTOX requested {}",
                config.model_reference,
                validated_context_tokens,
                config.context_tokens
            );
        }
    }
    let _ = resolve_model_path(&config)?;
    let transport = if let Some(port) = config.tcp_port {
        LocalTransport::TcpLoopback {
            host: "127.0.0.1".to_string(),
            port,
        }
    } else {
        let socket_path = config
            .socket_path
            .as_deref()
            .map(PathBuf::from)
            .context("LiteRT bridge config requires socket_path or tcp_port")?;
        LocalTransport::UnixSocket { path: socket_path }
    };
    let mut listener = transport.bind().with_context(|| {
        format!(
            "failed to bind LiteRT transport {}",
            transport.display_label()
        )
    })?;
    let label = listener.display_label().to_string();
    loop {
        let stream = listener
            .accept()
            .with_context(|| format!("failed to accept on {label}"))?;
        if let Err(err) = handle_stream(&config, stream) {
            eprintln!("ctox litert bridge request failed: {err:#}");
        }
    }
}

fn handle_stream(config: &LiteRtBridgeConfig, mut stream: LocalStream) -> anyhow::Result<()> {
    stream
        .set_read_timeout(Some(Duration::from_secs(300)))
        .context("failed to set LiteRT bridge read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(300)))
        .context("failed to set LiteRT bridge write timeout")?;
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .context("failed to clone socket stream")?,
    );
    let mut request_line = String::new();
    let bytes_read = reader
        .read_line(&mut request_line)
        .context("failed to read LiteRT socket request")?;
    if bytes_read == 0 {
        return Ok(());
    }
    match execute_request(config, request_line.trim()) {
        Ok(lines) => {
            for line in lines {
                stream
                    .write_all(line.as_bytes())
                    .context("failed to write LiteRT socket response")?;
                stream
                    .write_all(b"\n")
                    .context("failed to terminate LiteRT socket response line")?;
            }
            stream
                .flush()
                .context("failed to flush LiteRT socket response")?;
        }
        Err(err) => {
            let payload = json!({
                "type": "response.failed",
                "response": {
                    "id": synthetic_response_id(),
                    "error": {
                        "message": err.to_string(),
                    }
                }
            });
            stream
                .write_all(payload.to_string().as_bytes())
                .context("failed to write LiteRT error response")?;
            stream
                .write_all(b"\n")
                .context("failed to terminate LiteRT error response")?;
            stream
                .flush()
                .context("failed to flush LiteRT error response")?;
        }
    }
    Ok(())
}

fn execute_request(config: &LiteRtBridgeConfig, raw_request: &str) -> anyhow::Result<Vec<String>> {
    let mut request_value: Value =
        serde_json::from_str(raw_request).context("failed to parse local socket request")?;
    request_value["model"] = Value::String(config.model_reference.clone());
    let local_request: LocalSocketResponsesRequest = serde_json::from_value(request_value.clone())
        .context("failed to decode LiteRT local socket request")?;
    let request_bytes =
        serde_json::to_vec(&request_value).context("failed to encode LiteRT request payload")?;
    let route = model_adapters::ResolvedResponsesAdapterRoute::resolve(
        Some(&config.model_reference),
        &request_bytes,
        false,
    )?
    .context("LiteRT bridge only supports models with a local responses adapter")?;
    let chat_request: Value = serde_json::from_slice(route.forwarded_body())
        .context("failed to parse forwarded chat-completions request")?;
    let prompt_style = prompt_style_for_model(&config.model_reference)?;
    let prompt = render_prompt(
        prompt_style,
        &chat_request,
        context_summary(&local_request, config),
    );
    let generated_text = run_litert_cli(config, &prompt)?;
    let chat_completion = build_chat_completion_response(&config.model_reference, &generated_text);
    let chat_response_raw = serde_json::to_vec(&chat_completion)
        .context("failed to encode synthetic LiteRT chat completion response")?;
    let responses_raw = route
        .response_plan()
        .rewrite_success_response(&chat_response_raw, Some(&config.model_reference))
        .context("failed to normalize LiteRT chat completion into responses payload")?;
    let responses_payload: Value = serde_json::from_slice(&responses_raw)
        .context("failed to parse normalized LiteRT responses payload")?;
    Ok(render_socket_events(
        &responses_payload,
        local_request.stream,
        &config.model_reference,
    ))
}

fn context_summary(request: &LocalSocketResponsesRequest, config: &LiteRtBridgeConfig) -> String {
    let mut parts = vec![
        format!("Target context budget: {} tokens.", config.context_tokens),
        format!("Conversation items in this turn: {}.", request.input.len()),
    ];
    if !request.instructions.trim().is_empty() {
        parts.push("Instructions are already included in the conversation below.".to_string());
    }
    if !request.tools.is_empty() {
        parts.push(format!(
            "{} tool definitions are available.",
            request.tools.len()
        ));
    }
    if let Some(max_output_tokens) = request.max_output_tokens {
        parts.push(format!(
            "Requested max output tokens: {}.",
            max_output_tokens
        ));
    }
    if !request.tool_choice.trim().is_empty() {
        parts.push(format!("Tool choice hint: {}.", request.tool_choice.trim()));
    }
    if let Some(service_tier) = request.service_tier.as_deref() {
        parts.push(format!("Service tier hint: {}.", service_tier.trim()));
    }
    if let Some(prompt_cache_key) = request.prompt_cache_key.as_deref() {
        parts.push(format!("Prompt cache key: {}.", prompt_cache_key.trim()));
    }
    if request.parallel_tool_calls {
        parts.push(
            "Parallel tool calls are allowed, but emit at most one tool call in this turn."
                .to_string(),
        );
    }
    if request.text.is_some() {
        parts.push("The caller provided text controls; keep output compact and schema-compliant when possible.".to_string());
    }
    parts.join(" ")
}

fn build_chat_completion_response(model: &str, text: &str) -> Value {
    json!({
        "id": synthetic_response_id(),
        "object": "chat.completion",
        "created": current_unix_ts(),
        "model": model,
        "choices": [
            {
                "index": 0,
                "finish_reason": "stop",
                "message": {
                    "role": "assistant",
                    "content": text,
                }
            }
        ],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0
        }
    })
}

fn render_socket_events(
    responses_payload: &Value,
    _stream_requested: bool,
    fallback_model: &str,
) -> Vec<String> {
    let response_id = responses_payload
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("resp_ctox_litert");
    let model = responses_payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(fallback_model);
    let usage = responses_payload
        .get("usage")
        .cloned()
        .unwrap_or_else(|| json!({"input_tokens":0,"output_tokens":0,"total_tokens":0}));
    let mut lines = vec![json!({
        "type": "response.created",
        "response": {
            "id": response_id,
            "model": model,
        }
    })
    .to_string()];
    if let Some(items) = responses_payload.get("output").and_then(Value::as_array) {
        for item in items {
            lines.push(
                json!({
                    "type": "response.output_item.done",
                    "item": item,
                })
                .to_string(),
            );
        }
    }
    lines.push(
        json!({
            "type": "response.completed",
            "response": {
                "id": response_id,
                "model": model,
                "usage": usage,
            }
        })
        .to_string(),
    );
    lines
}

fn resolve_model_path(config: &LiteRtBridgeConfig) -> anyhow::Result<PathBuf> {
    if let Some(model_file) = config.model_file.as_deref() {
        let candidate = PathBuf::from(model_file);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    let model_id = imported_model_id(config);
    let imported_path = lite_rt_models_root().join(&model_id).join("model.litertlm");
    if imported_path.is_file() {
        return Ok(imported_path);
    }
    let cli = resolve_litert_cli(config)?;
    let model_file = config
        .model_file
        .as_deref()
        .context("LiteRT bridge missing model_file for import")?;
    let repo = config
        .huggingface_repo
        .as_deref()
        .context("LiteRT bridge missing huggingface_repo for import")?;
    let status = Command::new(&cli)
        .arg("import")
        .arg("--from-huggingface-repo")
        .arg(repo)
        .args(
            config
                .huggingface_token
                .as_deref()
                .map_or_else(Vec::new, |token| vec!["--huggingface-token", token]),
        )
        .arg(model_file)
        .arg(&model_id)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to run LiteRT import via {}", cli.display()))?;
    if !status.success() {
        anyhow::bail!(
            "LiteRT import failed for {} from {} ({status})",
            model_file,
            repo
        );
    }
    Ok(imported_path)
}

fn run_litert_cli(config: &LiteRtBridgeConfig, prompt: &str) -> anyhow::Result<String> {
    let model_path = resolve_model_path(config)?;
    let cli = resolve_litert_cli(config)?;
    let mut command = Command::new(&cli);
    command
        .arg("run")
        .arg(model_path)
        .arg("--backend")
        .arg(config.backend.trim());
    if let Some(mode) = config.speculative_decoding.as_deref() {
        command.arg("--enable-speculative-decoding").arg(mode);
    }
    if config.verbose {
        command.arg("--verbose");
    }
    command.arg("--prompt").arg(prompt);
    let output = command
        .output()
        .with_context(|| format!("failed to execute LiteRT CLI {}", cli.display()))?;
    let stdout = String::from_utf8(output.stdout).context("LiteRT CLI returned non-utf8 output")?;
    let stdout = stdout.trim().to_string();
    if !output.status.success() {
        if litert_cli_exit_is_recoverable(&output.status, &stdout) {
            return Ok(stdout);
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "LiteRT CLI run failed ({}){}",
            output.status,
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        );
    }
    Ok(stdout)
}

#[cfg(unix)]
fn litert_cli_exit_is_recoverable(status: &std::process::ExitStatus, stdout: &str) -> bool {
    !stdout.trim().is_empty() && status.signal() == Some(11)
}

#[cfg(not(unix))]
fn litert_cli_exit_is_recoverable(_status: &std::process::ExitStatus, _stdout: &str) -> bool {
    false
}

fn resolve_litert_cli(config: &LiteRtBridgeConfig) -> anyhow::Result<PathBuf> {
    let configured = config
        .cli_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| path.is_file());
    if let Some(path) = configured {
        return Ok(path);
    }
    for candidate in [
        PathBuf::from("/tmp/litertlm-venv/bin/litert-lm"),
        PathBuf::from("/usr/local/bin/litert-lm"),
        PathBuf::from("/opt/homebrew/bin/litert-lm"),
    ] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    let discovered = Command::new("which")
        .arg("litert-lm")
        .output()
        .context("failed to discover litert-lm binary")?;
    if discovered.status.success() {
        let candidate = String::from_utf8_lossy(&discovered.stdout)
            .trim()
            .to_string();
        if !candidate.is_empty() {
            return Ok(PathBuf::from(candidate));
        }
    }
    anyhow::bail!("LiteRT CLI not found; configure CTOX_LITERT_CLI or install litert-lm on PATH")
}

fn lite_rt_models_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".litert-lm/models")
}

fn imported_model_id(config: &LiteRtBridgeConfig) -> String {
    let import_identity = config
        .huggingface_repo
        .as_deref()
        .map(|repo| {
            format!(
                "{}-{}",
                repo,
                config.model_file.as_deref().unwrap_or("model.litertlm")
            )
        })
        .unwrap_or_else(|| config.model_reference.clone());
    let mut id = String::from("ctox-");
    for ch in import_identity.chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
        } else {
            id.push('-');
        }
    }
    while id.contains("--") {
        id = id.replace("--", "-");
    }
    id.trim_matches('-').to_string()
}

fn prompt_style_for_model(model: &str) -> anyhow::Result<LiteRtPromptStyle> {
    match engine::chat_model_family_for_model(model)
        .context("failed to resolve LiteRT prompt style from model family")?
    {
        engine::ChatModelFamily::Gemma4 => Ok(LiteRtPromptStyle::Gemma4),
        engine::ChatModelFamily::Qwen35 => Ok(LiteRtPromptStyle::Qwen35),
        other => anyhow::bail!("LiteRT bridge does not support {:?} models yet", other),
    }
}

fn render_prompt(
    style: LiteRtPromptStyle,
    chat_request: &Value,
    context_summary: String,
) -> String {
    let mut prompt = String::new();
    prompt.push_str("You are Codex running through CTOX on a local LiteRT runtime.\n");
    prompt.push_str("Emit exactly one tool call or one final answer per turn.\n");
    prompt.push_str(&context_summary);
    prompt.push('\n');
    if let Some(tools) = chat_request.get("tools").and_then(Value::as_array) {
        if !tools.is_empty() {
            prompt.push_str("\nAvailable tools (JSON schemas):\n");
            for tool in tools {
                prompt.push_str("- ");
                prompt.push_str(&tool.to_string());
                prompt.push('\n');
            }
            match style {
                LiteRtPromptStyle::Qwen35 => {
                    prompt.push_str(
                        "\nIf you need a tool, emit exactly one XML tool call in this form:\n",
                    );
                    prompt.push_str("<tool_call>\n<function=tool_name>\n<parameter=name>value</parameter>\n</function>\n</tool_call>\n");
                }
                LiteRtPromptStyle::Gemma4 => {
                    prompt.push_str(
                        "\nIf you need a tool, emit exactly one tool call in this form:\n",
                    );
                    prompt.push_str("<|tool_call>call:tool_name {\"arg\":\"value\"}<tool_call|>\n");
                }
            }
        }
    }
    prompt.push_str("\nConversation:\n");
    if let Some(messages) = chat_request.get("messages").and_then(Value::as_array) {
        for message in messages {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user")
                .to_ascii_uppercase();
            prompt.push_str("\n[");
            prompt.push_str(&role);
            prompt.push_str("]\n");
            let text = engine::extract_message_content_text(message.get("content"));
            if !text.trim().is_empty() {
                prompt.push_str(text.trim());
                prompt.push('\n');
            }
            if let Some(reasoning) = message.get("reasoning_content").and_then(Value::as_str) {
                if !reasoning.trim().is_empty() {
                    prompt.push_str("[REASONING]\n");
                    prompt.push_str(reasoning.trim());
                    prompt.push('\n');
                }
            }
        }
    }
    prompt.push_str("\n[ASSISTANT]\n");
    prompt
}

fn current_unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn synthetic_response_id() -> String {
    format!("resp_ctox_litert_{}", current_unix_ts())
}
