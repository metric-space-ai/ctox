// Origin: CTOX
// License: Apache-2.0

//! Qwen3.6-35B-A3B local backend.
//!
//! This is the initial ggml/CUDA baseline for CTOX harness routing:
//! a Rust Unix-domain-socket IPC server that speaks the same
//! line-delimited Responses contract as the native Qwen3.5 backend,
//! then invokes the local `llama-cli` binary against a cached GGUF.
//! It intentionally exposes no TCP or HTTP listener.

use std::io;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, error, info, warn};

const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
const IDLE_TIMEOUT: Duration = Duration::from_secs(900);
const DEFAULT_MAX_OUTPUT_TOKENS: usize = 2048;
const QWEN_TOOL_PROTOCOL_INSTRUCTIONS: &str = r#"# Tools

You may call one or more functions to assist with the user query.

You are provided with function signatures within <tools></tools> XML tags:
<tools>
{tools}
</tools>

For each function call, return a JSON object with function name and arguments within <tool_call></tool_call> XML tags:
<tool_call>
<function=exec_command>
<parameter=cmd>
printf hello
</parameter>
</function>
</tool_call>

Local CTOX requirements:
- If the next action requires a command, filesystem change, runtime inspection, benchmark run, ticket/state update, or artifact verification, your entire assistant message must be exactly one tool call and no prose.
- Use the exact tool name from the available tools.
- Do not use Markdown fences for tool calls.
- Do not write a <think> block. Emit the tool call directly.
- After tool output is returned, either call another tool or give the final answer only when the requested durable result has been verified."#;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "qwen36-35b-a3b-ggml-server",
    about = "Local Unix-socket IPC backend for Qwen3.6-35B-A3B via llama.cpp/ggml CUDA"
)]
struct Args {
    #[arg(long)]
    model: PathBuf,
    #[arg(long)]
    llama_cli: PathBuf,
    #[arg(long)]
    socket: PathBuf,
    #[arg(long, default_value = "Qwen/Qwen3.6-35B-A3B")]
    model_id: String,
    #[arg(long, default_value_t = 131072)]
    ctx: usize,
    #[arg(long, default_value_t = 99)]
    gpu_layers: i32,
    #[arg(long, default_value = "layer")]
    split_mode: String,
    #[arg(long, default_value = "1,1,1,1")]
    tensor_split: String,
    #[arg(long, default_value_t = 16)]
    threads: usize,
    #[arg(long, default_value_t = 2048)]
    batch: usize,
    #[arg(long, default_value_t = 512)]
    ubatch: usize,
    #[arg(long, default_value_t = 0.6)]
    temperature: f32,
    #[arg(long)]
    #[allow(dead_code)]
    request_model_alias: Option<String>,
}

#[derive(Clone)]
struct Engine {
    args: Args,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    ensure_file(&args.model, "model GGUF")?;
    ensure_file(&args.llama_cli, "llama-cli")?;

    let engine = Arc::new(Engine { args });
    serve(engine).await
}

async fn serve(engine: Arc<Engine>) -> Result<()> {
    let sock = &engine.args.socket;
    if let Some(parent) = sock.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create_dir_all {}", parent.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mut perms) = std::fs::metadata(parent).map(|m| m.permissions()) {
                perms.set_mode(0o700);
                let _ = std::fs::set_permissions(parent, perms);
            }
        }
    }
    remove_stale_socket(sock)?;
    let listener = UnixListener::bind(sock).with_context(|| format!("bind {}", sock.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(sock)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(sock, perms)?;
    }

    info!(
        socket = %sock.display(),
        model = %engine.args.model_id,
        gguf = %engine.args.model.display(),
        llama_cli = %engine.args.llama_cli.display(),
        "qwen36 ggml server listening"
    );

    let mut sig_int = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut sig_term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _addr)) => {
                        if !peer_uid_authorized(&stream) {
                            warn!("rejecting connection: peer UID mismatch");
                            drop(stream);
                            continue;
                        }
                        let engine = engine.clone();
                        tokio::spawn(async move {
                            if let Err(err) = handle_connection(engine, stream).await {
                                debug!("connection closed: {err:#}");
                            }
                        });
                    }
                    Err(err) => error!("accept failed: {err}; continuing"),
                }
            }
            _ = sig_int.recv() => {
                info!("SIGINT received; draining");
                break;
            }
            _ = sig_term.recv() => {
                info!("SIGTERM received; draining");
                break;
            }
        }
    }

    drop(listener);
    let _ = std::fs::remove_file(sock);
    info!("shutdown complete");
    Ok(())
}

async fn handle_connection(engine: Arc<Engine>, stream: UnixStream) -> Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);

    let mut buf = Vec::with_capacity(4096);
    let bytes = tokio::time::timeout(
        IDLE_TIMEOUT,
        read_line_capped(&mut reader, &mut buf, MAX_REQUEST_BYTES),
    )
    .await
    .map_err(|_| anyhow!("idle timeout waiting for request"))??;
    if bytes == 0 {
        return Ok(());
    }

    let request: LocalIpcRequest = match serde_json::from_slice(&buf) {
        Ok(r) => r,
        Err(err) => {
            write_json_line(
                &mut writer,
                &LocalIpcResponse::Error(IpcError {
                    code: "invalid_request".into(),
                    message: format!("invalid_request: {err}"),
                }),
            )
            .await?;
            writer.flush().await?;
            return Ok(());
        }
    };

    match request {
        LocalIpcRequest::RuntimeHealth => {
            write_json_line(
                &mut writer,
                &LocalIpcResponse::RuntimeHealth(RuntimeHealth {
                    healthy: true,
                    default_model: Some(engine.args.model_id.clone()),
                    loaded_models: vec![engine.args.model_id.clone()],
                }),
            )
            .await?;
            writer.flush().await?;
        }
        LocalIpcRequest::ResponsesCreate(req) => {
            let engine_for_task = engine.clone();
            let frames = tokio::task::spawn_blocking(move || run_turn(engine_for_task, req))
                .await
                .context("inference task panicked")??;
            writer.write_all(&frames).await?;
            writer.flush().await?;
        }
    }

    Ok(())
}

fn run_turn(engine: Arc<Engine>, req: ResponsesCreateRequest) -> Result<Vec<u8>> {
    let mut sink = BufferSink::default();
    let response_id = format!("resp_{}", uuid::Uuid::new_v4().simple());
    let message_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
    let created_at = chrono::Utc::now().timestamp();
    let mut seq = 0_u64;

    let mut next_seq = || {
        let out = seq;
        seq += 1;
        out
    };

    let mut envelope = ResponseEnvelope {
        id: response_id.clone(),
        object: "response",
        created_at,
        status: ResponseStatus::InProgress,
        model: engine.args.model_id.clone(),
        output: Vec::new(),
        usage: None,
        error: None,
    };

    if req.stream {
        sink.send(&ResponsesStreamEvent::Created {
            response: envelope.clone(),
            sequence_number: next_seq(),
        })?;
        sink.send(&ResponsesStreamEvent::InProgress {
            response: envelope.clone(),
            sequence_number: next_seq(),
        })?;
        sink.send(&ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            item: ResponseOutputItem::Message {
                id: message_id.clone(),
                status: ResponseStatus::InProgress,
                role: "assistant",
                content: Vec::new(),
            },
            sequence_number: next_seq(),
        })?;
        sink.send(&ResponsesStreamEvent::ContentPartAdded {
            item_id: message_id.clone(),
            output_index: 0,
            content_index: 0,
            part: ResponseContentPart::OutputText {
                text: String::new(),
                annotations: Vec::new(),
            },
            sequence_number: next_seq(),
        })?;
    }

    let prompt = render_chat_prompt(&req);
    let max_out = req
        .max_output_tokens
        .unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS)
        .min(DEFAULT_MAX_OUTPUT_TOKENS);
    let text = match run_llama(&engine.args, &prompt, max_out) {
        Ok(text) => text,
        Err(err) => {
            let failed = ResponseEnvelope {
                id: response_id,
                object: "response",
                created_at,
                status: ResponseStatus::Failed,
                model: engine.args.model_id.clone(),
                output: Vec::new(),
                usage: None,
                error: Some(IpcError {
                    code: "inference_error".into(),
                    message: err.to_string(),
                }),
            };
            sink.send(&ResponsesStreamEvent::Failed {
                response: failed,
                sequence_number: next_seq(),
            })?;
            return Ok(sink.buf);
        }
    };

    let parsed_tool_call = parse_qwen_tool_code(&text);

    if req.stream && !text.is_empty() && parsed_tool_call.is_none() {
        sink.send(&ResponsesStreamEvent::OutputTextDelta {
            item_id: message_id.clone(),
            output_index: 0,
            content_index: 0,
            delta: text.clone(),
            sequence_number: next_seq(),
        })?;
        sink.send(&ResponsesStreamEvent::OutputTextDone {
            item_id: message_id.clone(),
            output_index: 0,
            content_index: 0,
            text: text.clone(),
            sequence_number: next_seq(),
        })?;
        let done_part = ResponseContentPart::OutputText {
            text: text.clone(),
            annotations: Vec::new(),
        };
        sink.send(&ResponsesStreamEvent::ContentPartDone {
            item_id: message_id.clone(),
            output_index: 0,
            content_index: 0,
            part: done_part.clone(),
            sequence_number: next_seq(),
        })?;
        sink.send(&ResponsesStreamEvent::OutputItemDone {
            output_index: 0,
            item: ResponseOutputItem::Message {
                id: message_id.clone(),
                status: ResponseStatus::Completed,
                role: "assistant",
                content: vec![done_part],
            },
            sequence_number: next_seq(),
        })?;
    } else if req.stream {
        if let Some(tool_call) = parsed_tool_call.clone() {
            sink.send(&ResponsesStreamEvent::OutputItemAdded {
                output_index: 1,
                item: ResponseOutputItem::FunctionCall {
                    id: tool_call.id.clone(),
                    call_id: tool_call.call_id.clone(),
                    name: tool_call.name.clone(),
                    arguments: String::new(),
                    status: ResponseStatus::InProgress,
                },
                sequence_number: next_seq(),
            })?;
            sink.send(&ResponsesStreamEvent::FunctionCallArgumentsDelta {
                item_id: tool_call.id.clone(),
                output_index: 1,
                delta: tool_call.arguments.clone(),
                sequence_number: next_seq(),
            })?;
            sink.send(&ResponsesStreamEvent::FunctionCallArgumentsDone {
                item_id: tool_call.id.clone(),
                output_index: 1,
                arguments: tool_call.arguments.clone(),
                sequence_number: next_seq(),
            })?;
            sink.send(&ResponsesStreamEvent::OutputItemDone {
                output_index: 1,
                item: ResponseOutputItem::FunctionCall {
                    id: tool_call.id,
                    call_id: tool_call.call_id,
                    name: tool_call.name,
                    arguments: tool_call.arguments,
                    status: ResponseStatus::Completed,
                },
                sequence_number: next_seq(),
            })?;
        }
    }

    let output_tokens = estimate_tokens(&text);
    let input_tokens = estimate_tokens(&prompt);
    envelope.status = ResponseStatus::Completed;
    envelope.output = if let Some(tool_call) = parsed_tool_call {
        vec![ResponseOutputItem::FunctionCall {
            id: tool_call.id,
            call_id: tool_call.call_id,
            name: tool_call.name,
            arguments: tool_call.arguments,
            status: ResponseStatus::Completed,
        }]
    } else {
        vec![ResponseOutputItem::Message {
            id: message_id,
            status: ResponseStatus::Completed,
            role: "assistant",
            content: vec![ResponseContentPart::OutputText {
                text,
                annotations: Vec::new(),
            }],
        }]
    };
    envelope.usage = Some(ResponseUsage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
        cached_input_tokens: Some(0),
        reasoning_output_tokens: Some(0),
    });

    sink.send(&ResponsesStreamEvent::Completed {
        response: envelope,
        sequence_number: next_seq(),
    })?;
    Ok(sink.buf)
}

#[derive(Debug, Clone)]
struct ParsedToolCall {
    id: String,
    call_id: String,
    name: String,
    arguments: String,
}

fn parse_qwen_tool_code(text: &str) -> Option<ParsedToolCall> {
    if let Some(tool_call) = parse_qwen_xml_tool_call(text) {
        return Some(tool_call);
    }

    let marker = "<|tool_code_";
    let after_marker = if let Some(marker_pos) = text.find(marker) {
        &text[marker_pos..]
    } else {
        text.trim()
    };
    let fence_start = after_marker.find("```")?;
    let after_fence = &after_marker[fence_start + 3..];
    let newline = after_fence.find('\n')?;
    let language = after_fence[..newline].trim().to_ascii_lowercase();
    let body_start = newline + 1;
    let body = &after_fence[body_start..];
    let fence_end = body.find("```")?;
    let code = body[..fence_end].trim();
    if code.is_empty() {
        return None;
    }
    if !text.contains(marker)
        && !(text.trim_start().starts_with("```") && text.trim_end().ends_with("```"))
    {
        return None;
    }
    let cmd = match language.as_str() {
        "python" | "py" => format!("python3 - <<'PY'\n{code}\nPY"),
        "bash" | "sh" | "shell" | "" => code.to_string(),
        _ => return None,
    };
    let arguments = serde_json::json!({
        "cmd": cmd,
        "yield_time_ms": 1000,
        "max_output_tokens": 20000,
    })
    .to_string();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    Some(ParsedToolCall {
        id: format!("fc_{suffix}"),
        call_id: format!("call_{suffix}"),
        name: "exec_command".to_string(),
        arguments,
    })
}

fn parse_qwen_xml_tool_call(text: &str) -> Option<ParsedToolCall> {
    let tool_re = Regex::new(r"(?s)<tool_call>\s*(.*?)\s*</tool_call>").ok()?;
    let fn_re = Regex::new(r"(?s)<function=([A-Za-z0-9_.-]+)>\s*(.*?)\s*</function>").ok()?;
    let param_re = Regex::new(r"(?s)<parameter=([A-Za-z0-9_.-]+)>\s*(.*?)\s*</parameter>").ok()?;
    let tool_body = tool_re.captures(text)?.get(1)?.as_str();
    if let Some(tool_call) = parse_qwen_json_tool_call(tool_body) {
        return Some(tool_call);
    }
    let fn_caps = fn_re.captures(tool_body)?;
    let name = fn_caps.get(1)?.as_str().trim().to_string();
    let params_body = fn_caps.get(2)?.as_str();
    let mut params = serde_json::Map::new();
    for caps in param_re.captures_iter(params_body) {
        let key = caps.get(1)?.as_str().trim();
        if key.is_empty() {
            continue;
        }
        let value = caps.get(2).map(|m| m.as_str()).unwrap_or("").trim();
        params.insert(key.to_string(), Value::String(value.to_string()));
    }
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    Some(ParsedToolCall {
        id: format!("fc_{suffix}"),
        call_id: format!("call_{suffix}"),
        name,
        arguments: Value::Object(params).to_string(),
    })
}

fn parse_qwen_json_tool_call(tool_body: &str) -> Option<ParsedToolCall> {
    let value: Value = serde_json::from_str(tool_body.trim()).ok()?;
    let obj = value.as_object()?;
    let name = obj
        .get("name")
        .or_else(|| {
            obj.get("function")
                .and_then(|function| function.get("name"))
        })
        .and_then(Value::as_str)?
        .trim();
    if name.is_empty() {
        return None;
    }
    let arguments = obj
        .get("arguments")
        .or_else(|| obj.get("parameters"))
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    let arguments = match arguments {
        Value::String(raw) => serde_json::from_str::<Value>(&raw)
            .unwrap_or(Value::String(raw))
            .to_string(),
        value => value.to_string(),
    };
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    Some(ParsedToolCall {
        id: format!("fc_{suffix}"),
        call_id: format!("call_{suffix}"),
        name: name.to_string(),
        arguments,
    })
}

fn run_llama(args: &Args, prompt: &str, max_out: usize) -> Result<String> {
    let prompt_path = write_prompt_file(prompt)?;
    let output = Command::new(&args.llama_cli)
        .arg("-m")
        .arg(&args.model)
        .arg("-f")
        .arg(&prompt_path)
        .arg("-n")
        .arg(max_out.to_string())
        .arg("-c")
        .arg(args.ctx.to_string())
        .arg("-t")
        .arg(args.threads.to_string())
        .arg("-b")
        .arg(args.batch.to_string())
        .arg("-ub")
        .arg(args.ubatch.to_string())
        .arg("-ngl")
        .arg(args.gpu_layers.to_string())
        .arg("-sm")
        .arg(&args.split_mode)
        .arg("-ts")
        .arg(&args.tensor_split)
        .arg("-fa")
        .arg("on")
        .arg("--temp")
        .arg(args.temperature.to_string())
        .arg("--top-p")
        .arg("0.95")
        .arg("--top-k")
        .arg("20")
        .arg("--presence-penalty")
        .arg("0.0")
        .arg("--repeat-penalty")
        .arg("1.0")
        .arg("--no-display-prompt")
        .arg("--simple-io")
        .arg("-r")
        .arg("<|im_end|>")
        .arg("-r")
        .arg("<|im_start|>")
        .output()
        .with_context(|| format!("spawn {}", args.llama_cli.display()));
    let _ = std::fs::remove_file(&prompt_path);
    let output = output?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "llama-cli exited with {}: {}",
            output.status,
            last_lines(&stderr, 20)
        );
    }

    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text = strip_prompt_echo(&text, prompt);
    Ok(clean_model_output(&text))
}

fn write_prompt_file(prompt: &str) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "ctox-qwen36-prompt-{}.txt",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&path, prompt).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

fn render_chat_prompt(req: &ResponsesCreateRequest) -> String {
    let mut turns = Vec::new();
    let system_text = render_system_prompt(req);
    if !system_text.trim().is_empty() {
        turns.push(("system".to_string(), system_text));
    }
    for item in &req.input {
        if let Some((role, text)) = input_item_to_turn(item) {
            turns.push((role, text));
        }
    }
    let mut out = String::new();
    for (role, text) in turns {
        out.push_str("<|im_start|>");
        out.push_str(&role);
        out.push('\n');
        out.push_str(&text);
        out.push_str("<|im_end|>\n");
    }
    out.push_str("<|im_start|>assistant\n");
    out
}

fn render_system_prompt(req: &ResponsesCreateRequest) -> String {
    let mut parts = Vec::new();
    if !req.instructions.trim().is_empty() {
        parts.push(req.instructions.trim().to_string());
    }
    if !req.tools.is_empty() {
        parts.push(render_tool_protocol(&req.tools));
    }
    parts.join("\n\n")
}

fn render_tool_protocol(tools: &[Value]) -> String {
    let mut rendered_tools = Vec::new();
    for tool in tools {
        let Some(name) = tool_name(tool) else {
            continue;
        };
        let description = tool_description(tool).unwrap_or_default();
        let parameters = tool_parameters(tool)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "{}".to_string());
        rendered_tools.push(format!(
            "<function>\n<name>{name}</name>\n<description>{description}</description>\n<parameters>{parameters}</parameters>\n</function>"
        ));
    }
    let tools = if rendered_tools.is_empty() {
        "<function>\n<name>exec_command</name>\n<parameters>{\"type\":\"object\",\"properties\":{\"cmd\":{\"type\":\"string\"}},\"required\":[\"cmd\"]}</parameters>\n</function>".to_string()
    } else {
        rendered_tools.join("\n")
    };
    QWEN_TOOL_PROTOCOL_INSTRUCTIONS.replace("{tools}", &tools)
}

fn tool_name(tool: &Value) -> Option<String> {
    tool.get("function")
        .and_then(|function| function.get("name"))
        .or_else(|| tool.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn tool_description(tool: &Value) -> Option<String> {
    tool.get("function")
        .and_then(|function| function.get("description"))
        .or_else(|| tool.get("description"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|description| !description.is_empty())
        .map(ToOwned::to_owned)
}

fn tool_parameters(tool: &Value) -> Option<&Value> {
    tool.get("function")
        .and_then(|function| function.get("parameters"))
        .or_else(|| tool.get("parameters"))
}

fn input_item_to_turn(item: &Value) -> Option<(String, String)> {
    let obj = item.as_object()?;
    let ty = obj.get("type").and_then(Value::as_str).unwrap_or("message");
    match ty {
        "message" => {
            let role = obj
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user")
                .to_string();
            let text = match obj.get("content") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Array(parts)) => flatten_content_parts(parts),
                _ => String::new(),
            };
            Some((role, text))
        }
        "function_call" => {
            let name = obj.get("name").and_then(Value::as_str).unwrap_or_default();
            let arguments = obj.get("arguments").and_then(Value::as_str).unwrap_or("{}");
            Some((
                "assistant".to_string(),
                render_xml_tool_call(name, arguments),
            ))
        }
        "function_call_output" => {
            let output = extract_function_output_text(obj.get("output"));
            Some((
                "user".to_string(),
                format!("<tool_response>\n{}\n</tool_response>", output.trim_end()),
            ))
        }
        _ => None,
    }
}

fn render_xml_tool_call(name: &str, raw_arguments: &str) -> String {
    let arguments = serde_json::from_str::<Value>(raw_arguments)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let mut rendered = String::new();
    rendered.push_str("<tool_call>\n");
    rendered.push_str("<function=");
    rendered.push_str(name.trim());
    rendered.push_str(">\n");
    for (key, value) in arguments {
        rendered.push_str("<parameter=");
        rendered.push_str(key.trim());
        rendered.push_str(">\n");
        let value_text = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| value.to_string());
        rendered.push_str(&value_text);
        rendered.push_str("\n</parameter>\n");
    }
    rendered.push_str("</function>\n</tool_call>");
    rendered
}

fn extract_function_output_text(output: Option<&Value>) -> String {
    match output {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => flatten_content_parts(parts),
        Some(value) => value.to_string(),
        None => String::new(),
    }
}

fn flatten_content_parts(parts: &[Value]) -> String {
    let mut out = String::new();
    for part in parts {
        let Some(obj) = part.as_object() else {
            continue;
        };
        let ty = obj.get("type").and_then(Value::as_str).unwrap_or("");
        match ty {
            "input_text" | "output_text" | "text" => {
                if let Some(text) = obj.get("text").and_then(Value::as_str) {
                    out.push_str(text);
                }
            }
            "input_image" | "image" | "image_url" => out.push_str("[image]"),
            _ => {}
        }
    }
    out
}

fn strip_prompt_echo(text: &str, prompt: &str) -> String {
    text.strip_prefix(prompt).unwrap_or(text).to_string()
}

fn clean_model_output(text: &str) -> String {
    let mut out = text.replace("\r\n", "\n");
    for marker in ["<|im_end|>", "<|im_start|>"] {
        if let Some(pos) = out.find(marker) {
            out.truncate(pos);
        }
    }
    if let Some(pos) = out.find("\n\n> EOF by user") {
        out.truncate(pos);
    }
    if let Some(pos) = out.find("\n> EOF by user") {
        out.truncate(pos);
    }
    strip_think_blocks(&out).trim().to_string()
}

fn strip_think_blocks(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    loop {
        let Some(start) = rest.find("<think>") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..start]);
        let after_start = &rest[start + "<think>".len()..];
        let Some(end) = after_start.find("</think>") else {
            break;
        };
        rest = &after_start[end + "</think>".len()..];
    }
    out
}

fn estimate_tokens(text: &str) -> u32 {
    (text.len() / 4).max(text.split_whitespace().count()) as u32
}

fn ensure_file(path: &Path, label: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        bail!(
            "{label} does not exist or is not a file: {}",
            path.display()
        )
    }
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

fn remove_stale_socket(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("remove stale {}", path.display())),
    }
}

#[cfg(target_os = "linux")]
fn peer_uid_authorized(stream: &UnixStream) -> bool {
    use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
    use nix::unistd::Uid;
    let borrowed = unsafe { std::os::fd::BorrowedFd::borrow_raw(stream.as_raw_fd()) };
    let Ok(cred) = getsockopt(&borrowed, PeerCredentials) else {
        return false;
    };
    Uid::from_raw(cred.uid()) == Uid::current()
}

#[cfg(not(target_os = "linux"))]
fn peer_uid_authorized(_stream: &UnixStream) -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalIpcRequest {
    ResponsesCreate(ResponsesCreateRequest),
    RuntimeHealth,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ResponsesCreateRequest {
    #[serde(default)]
    model: String,
    #[serde(default)]
    instructions: String,
    #[serde(default)]
    input: Vec<Value>,
    #[serde(default)]
    tools: Vec<Value>,
    #[serde(default)]
    tool_choice: String,
    #[serde(default)]
    parallel_tool_calls: bool,
    #[serde(default)]
    reasoning: Option<Value>,
    #[serde(default)]
    max_output_tokens: Option<usize>,
    #[serde(default)]
    store: bool,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    service_tier: Option<String>,
    #[serde(default)]
    prompt_cache_key: Option<String>,
    #[serde(default)]
    text: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalIpcResponse {
    RuntimeHealth(RuntimeHealth),
    Error(IpcError),
}

#[derive(Debug, Serialize)]
struct RuntimeHealth {
    healthy: bool,
    default_model: Option<String>,
    loaded_models: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
struct IpcError {
    code: String,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ResponsesStreamEvent {
    #[serde(rename = "response.created")]
    Created {
        response: ResponseEnvelope,
        sequence_number: u64,
    },
    #[serde(rename = "response.in_progress")]
    InProgress {
        response: ResponseEnvelope,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        output_index: u32,
        item: ResponseOutputItem,
        sequence_number: u64,
    },
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: ResponseContentPart,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        item_id: String,
        output_index: u32,
        content_index: u32,
        delta: String,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        item_id: String,
        output_index: u32,
        content_index: u32,
        text: String,
        sequence_number: u64,
    },
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        item_id: String,
        output_index: u32,
        delta: String,
        sequence_number: u64,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone {
        item_id: String,
        output_index: u32,
        arguments: String,
        sequence_number: u64,
    },
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: ResponseContentPart,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_item.done")]
    OutputItemDone {
        output_index: u32,
        item: ResponseOutputItem,
        sequence_number: u64,
    },
    #[serde(rename = "response.completed")]
    Completed {
        response: ResponseEnvelope,
        sequence_number: u64,
    },
    #[serde(rename = "response.failed")]
    Failed {
        response: ResponseEnvelope,
        sequence_number: u64,
    },
}

#[derive(Debug, Serialize, Clone)]
struct ResponseEnvelope {
    id: String,
    object: &'static str,
    created_at: i64,
    status: ResponseStatus,
    model: String,
    output: Vec<ResponseOutputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<ResponseUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<IpcError>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ResponseStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ResponseOutputItem {
    Message {
        id: String,
        status: ResponseStatus,
        role: &'static str,
        content: Vec<ResponseContentPart>,
    },
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
        status: ResponseStatus,
    },
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ResponseContentPart {
    OutputText {
        text: String,
        annotations: Vec<Value>,
    },
}

#[derive(Debug, Serialize, Clone)]
struct ResponseUsage {
    input_tokens: u32,
    output_tokens: u32,
    total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    cached_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_output_tokens: Option<u32>,
}

#[derive(Default)]
struct BufferSink {
    buf: Vec<u8>,
}

impl BufferSink {
    fn send(&mut self, event: &ResponsesStreamEvent) -> Result<()> {
        serde_json::to_writer(&mut self.buf, event)?;
        self.buf.push(b'\n');
        Ok(())
    }
}

async fn read_line_capped<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    limit: usize,
) -> Result<usize> {
    buf.clear();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(buf.len());
        }
        if let Some(pos) = available.iter().position(|b| *b == b'\n') {
            buf.extend_from_slice(&available[..pos]);
            reader.consume(pos + 1);
            return Ok(buf.len());
        }
        if buf.len() + available.len() > limit {
            return Err(anyhow!("request exceeds {limit}-byte cap"));
        }
        buf.extend_from_slice(available);
        let n = available.len();
        reader.consume(n);
    }
}

async fn write_json_line<T: Serialize, W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    value: &T,
) -> Result<()> {
    let mut buf = serde_json::to_vec(value).context("encode response frame")?;
    buf.push(b'\n');
    writer
        .write_all(&buf)
        .await
        .context("write response frame")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_request() -> ResponsesCreateRequest {
        ResponsesCreateRequest {
            model: "Qwen/Qwen3.6-35B-A3B".to_string(),
            instructions: "System rules".to_string(),
            input: vec![json!({
                "type": "message",
                "role": "user",
                "content": "Create the artifact."
            })],
            tools: vec![json!({
                "type": "function",
                "function": {
                    "name": "exec_command",
                    "description": "Run a shell command.",
                    "parameters": {
                        "type": "object",
                        "properties": {"cmd": {"type": "string"}},
                        "required": ["cmd"]
                    }
                }
            })],
            tool_choice: "auto".to_string(),
            parallel_tool_calls: true,
            reasoning: None,
            max_output_tokens: None,
            store: false,
            stream: true,
            include: Vec::new(),
            service_tier: None,
            prompt_cache_key: None,
            text: None,
        }
    }

    #[test]
    fn render_prompt_injects_qwen_xml_tool_protocol() {
        let prompt = render_chat_prompt(&test_request());

        assert!(prompt.contains("# Tools"));
        assert!(prompt.contains("<name>exec_command</name>"));
        assert!(prompt.contains("<tool_call>\n<function=exec_command>"));
        assert!(prompt.contains("<parameter=cmd>\nprintf hello\n</parameter>"));
        assert!(prompt.contains("Do not write a <think> block"));
        assert!(!prompt.contains("/no_think"));
        assert!(!prompt.contains("<think>\n\n</think>"));
    }

    #[test]
    fn parses_qwen_xml_tool_call_to_responses_function_call() {
        let parsed = parse_qwen_tool_code(
            r#"<tool_call>
<function=exec_command>
<parameter=cmd>
printf CTOX_QWEN_TOOL_OK
</parameter>
</function>
</tool_call>"#,
        )
        .unwrap();

        assert_eq!(parsed.name, "exec_command");
        assert_eq!(
            parsed.arguments,
            json!({"cmd":"printf CTOX_QWEN_TOOL_OK"}).to_string()
        );
    }

    #[test]
    fn parses_qwen_json_tool_call_to_responses_function_call() {
        let parsed = parse_qwen_tool_code(
            r#"<tool_call>
{"name":"exec_command","arguments":{"cmd":"printf CTOX_QWEN_JSON_TOOL_OK"}}
</tool_call>"#,
        )
        .unwrap();

        assert_eq!(parsed.name, "exec_command");
        assert_eq!(
            parsed.arguments,
            json!({"cmd":"printf CTOX_QWEN_JSON_TOOL_OK"}).to_string()
        );
    }

    #[test]
    fn function_history_renders_as_qwen_xml_and_tool_response() {
        let call = input_item_to_turn(&json!({
            "type": "function_call",
            "name": "exec_command",
            "arguments": "{\"cmd\":\"pwd\"}"
        }))
        .unwrap();
        assert_eq!(call.0, "assistant");
        assert!(call.1.contains("<parameter=cmd>\npwd\n</parameter>"));

        let output = input_item_to_turn(&json!({
            "type": "function_call_output",
            "output": "ok"
        }))
        .unwrap();
        assert_eq!(output.0, "user");
        assert_eq!(output.1, "<tool_response>\nok\n</tool_response>");
    }
}
