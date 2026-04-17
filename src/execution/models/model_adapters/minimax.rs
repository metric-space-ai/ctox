use crate::inference::engine;
use anyhow::Context;
use serde_json::json;
use serde_json::Value;

use super::ResponsesTransportKind;

const LOCAL_COMPACT_INSTRUCTIONS: &str = "You are Codex running through CTOX on a local responses-backed runtime. Be concise and tool-accurate. Emit either one tool call or one final answer per turn. Prefer exec_command for shell work and apply_patch for file edits. Do not restate instructions. If the task requires creating or modifying files, running builds or tests, or proving a result inside a workspace, your next completion must be a tool call, not a final answer. You must not claim success, emit an exact marker, or give a final answer until tool output has verified the required result. When the user asks for an exact marker or short final answer, return only that required text after any needed tool calls and verification.";

pub fn adapter_id() -> &'static str {
    "minimax"
}

pub fn transport_kind() -> ResponsesTransportKind {
    ResponsesTransportKind::ChatCompletions
}

pub fn upstream_path() -> &'static str {
    "/v1/chat/completions"
}

pub fn compact_instructions() -> &'static str {
    LOCAL_COMPACT_INSTRUCTIONS
}

pub fn reasoning_effort_override() -> Option<&'static str> {
    None
}

pub fn unified_exec_enabled() -> bool {
    false
}

pub fn uses_ctox_web_stack() -> bool {
    false
}

pub fn compact_limit(_model: &str, realized_context: usize) -> usize {
    ((realized_context as f64) * 3.0 / 4.0).round() as usize
}

pub fn runtime_tuning(
    _preset: crate::inference::runtime_plan::ChatPreset,
    _max_output_tokens: u32,
) -> crate::inference::runtime_state::AdapterRuntimeTuning {
    crate::inference::runtime_state::AdapterRuntimeTuning::default()
}

pub fn rewrite_request(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse responses request")?;
    // Always use the canonical MiniMax model name, regardless of what codex-core
    // puts in the request body. codex-core's Models-Manager may remap unknown
    // model names to its internal defaults (e.g. "gpt-5.3-codex"), which the
    // MiniMax API rejects. The gateway already routed to this adapter based on
    // CTOX's active_model, so the correct model is always MiniMax-M2.7.
    let incoming_model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("");
    let model = if incoming_model.to_ascii_lowercase().contains("minimax") {
        incoming_model.to_string()
    } else {
        // Not a recognized MiniMax model name — use the canonical one.
        "MiniMax-M2.7".to_string()
    };
    let instructions = payload
        .get("instructions")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let messages = build_chat_messages(
        &engine::normalize_responses_input(payload.get("input")),
        instructions.as_deref(),
    );
    let (messages, tools) = build_request_parts(&payload, messages);

    let mut request = serde_json::Map::new();
    request.insert("model".to_string(), Value::String(model));
    request.insert("messages".to_string(), Value::Array(messages));
    if !tools.is_empty() {
        request.insert("tools".to_string(), Value::Array(tools));
    }
    if let Some(value) = payload.get("tool_choice") {
        request.insert("tool_choice".to_string(), value.clone());
    }
    // MiniMax M2.7 has mandatory reasoning — always enable thinking.
    let enable_thinking = payload
        .get("reasoning")
        .and_then(|value| value.get("effort"))
        .and_then(Value::as_str)
        .is_some();
    request.insert("enable_thinking".to_string(), Value::Bool(enable_thinking));
    for key in [
        "temperature",
        "top_p",
        "presence_penalty",
        "frequency_penalty",
        "max_output_tokens",
    ] {
        if let Some(value) = payload.get(key) {
            let mapped_key = if key == "max_output_tokens" {
                "max_tokens"
            } else {
                key
            };
            // MiniMax accepts temperature in (0.0, 1.0]; OpenAI clients
            // routinely send 1.2+ which the API rejects. Clamp.
            if mapped_key == "temperature" {
                if let Some(temp) = value.as_f64() {
                    let clamped = temp.clamp(0.001, 1.0);
                    request.insert(
                        mapped_key.to_string(),
                        serde_json::json!(clamped),
                    );
                    continue;
                }
            }
            request.insert(mapped_key.to_string(), value.clone());
        }
    }
    // M2.7's chain-of-thought needs significant headroom (model card says
    // up to 128k CoT tokens). Default codex-exec request usually has no
    // max_tokens set or sets a small cap, which cuts the model off mid-
    // <think> and produces empty agent replies. Floor at the CTOX standard
    // 131_072 unless the request explicitly asked for more.
    let current_max = request
        .get("max_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if current_max < 131_072 {
        request.insert("max_tokens".to_string(), serde_json::json!(131_072u64));
    }
    request.insert("stream".to_string(), Value::Bool(false));
    if payload.get("parallel_tool_calls") == Some(&Value::Bool(false)) {
        request.insert("parallel_tool_calls".to_string(), Value::Bool(true));
    } else if let Some(value) = payload.get("parallel_tool_calls") {
        request.insert("parallel_tool_calls".to_string(), value.clone());
    }

    serde_json::to_vec(&Value::Object(request))
        .context("failed to encode MiniMax chat-completions payload")
}

pub fn rewrite_success_response(
    raw: &[u8],
    fallback_model: Option<&str>,
    _exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse chat completion response")?;
    let mut builder =
        engine::responses_turn_builder(&payload, fallback_model, "minimax/minimax-m2.7");
    if let Some(choices) = payload.get("choices").and_then(Value::as_array) {
        for choice in choices {
            let message = choice.get("message").and_then(Value::as_object);
            if let Some(text) = message
                .and_then(|msg| msg.get("content"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|text| !text.is_empty())
            {
                builder.push_message_text(text);
            }
            // MiniMax uses reasoning_content for thinking output
            if let Some(reasoning) = message
                .and_then(|msg| {
                    msg.get("reasoning_content")
                        .or_else(|| msg.get("reasoning"))
                })
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|text| !text.is_empty())
            {
                builder.push_reasoning(reasoning);
            }
            if let Some(tool_calls) = message
                .and_then(|msg| msg.get("tool_calls"))
                .and_then(Value::as_array)
            {
                for tool_call in tool_calls {
                    let function = tool_call.get("function").unwrap_or(tool_call);
                    let name = function
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let arguments = function
                        .get("arguments")
                        .cloned()
                        .unwrap_or_else(|| json!({}));
                    let call_id = tool_call
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("call_ctox_proxy");
                    builder.push_function_call(
                        call_id,
                        name,
                        if arguments.is_string() {
                            arguments.as_str().unwrap_or("{}").to_string()
                        } else {
                            arguments.to_string()
                        },
                    );
                }
            }
        }
    }
    let response_payload = builder.build();
    serde_json::to_vec(&response_payload).context("failed to encode MiniMax responses payload")
}

fn build_request_parts(payload: &Value, messages: Vec<Value>) -> (Vec<Value>, Vec<Value>) {
    let mut merged_system_parts = Vec::new();
    let mut merged_messages = Vec::new();
    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let content = message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if role == "system" {
            if !content.is_empty() {
                merged_system_parts.push(content);
            }
        } else {
            merged_messages.push(message);
        }
    }
    let mut messages = Vec::new();
    if !merged_system_parts.is_empty() {
        messages.push(json!({
            "role": "system",
            "content": merged_system_parts.join("\n\n"),
        }));
    }
    messages.extend(merged_messages);

    let tools = payload
        .get("tools")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .flat_map(|tool| rewrite_tool(tool.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    (messages, tools)
}

fn build_chat_messages(items: &[Value], instructions: Option<&str>) -> Vec<Value> {
    let mut messages = Vec::new();
    if let Some(instructions) = instructions {
        messages.push(json!({
            "role": "system",
            "content": instructions,
        }));
    }

    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
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
                let text = engine::extract_message_content_text(object.get("content"));
                messages.push(json!({
                    "role": mapped_role,
                    "content": text,
                }));
            }
            "function_call" => {
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let arguments = object
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                let call_id = object
                    .get("call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_ctox_proxy");
                messages.push(json!({
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": arguments
                        }
                    }]
                }));
            }
            "function_call_output" => {
                let call_id = object
                    .get("call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_ctox_proxy");
                let output = engine::extract_function_call_output_text(object.get("output"));
                messages.push(json!({
                    "role": "tool",
                    "content": output,
                    "tool_call_id": call_id,
                }));
            }
            _ => {}
        }
    }

    messages
}

fn rewrite_tool(tool: Value) -> Vec<Value> {
    let Some(object) = tool.as_object() else {
        return Vec::new();
    };
    let Some(tool_type) = object.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };
    match tool_type {
        "function" => {
            let function = object
                .get("function")
                .and_then(Value::as_object)
                .unwrap_or(object);
            let Some(name) = function.get("name").and_then(Value::as_str) else {
                return Vec::new();
            };
            if name == "apply_patch" {
                return Vec::new();
            }
            engine::rewrite_tool(Value::Object(object.clone()))
                .into_iter()
                .collect()
        }
        "namespace" => object
            .get("tools")
            .and_then(Value::as_array)
            .map(|children| {
                children
                    .iter()
                    .flat_map(|child| rewrite_tool(child.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}
