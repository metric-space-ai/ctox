use crate::inference::engine;
use anyhow::Context;
use regex::Regex;
use serde_json::json;
use serde_json::Value;

use super::ResponsesTransportKind;

const LOCAL_COMPACT_INSTRUCTIONS: &str = "You are Codex running through CTOX on a local responses-backed runtime. Be concise and tool-accurate. Emit either one tool call or one final answer per turn. Prefer exec_command for shell work and apply_patch for file edits. Do not restate instructions. If the task requires creating or modifying files, running builds or tests, or proving a result inside a workspace, your next completion must be a tool call, not a final answer. You must not claim success, emit an exact marker, or give a final answer until tool output has verified the required result. When the user asks for an exact marker or short final answer, return only that required text after any needed tool calls and verification.";

pub fn adapter_id() -> &'static str {
    "gemma4"
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

pub fn compact_limit(model: &str, realized_context: usize) -> usize {
    match model {
        "google/gemma-4-31B-it" => 1_536.min(realized_context.saturating_sub(384)).max(1_024),
        "google/gemma-4-26B-A4B-it" => 1_792.min(realized_context.saturating_sub(384)).max(1_152),
        _ => ((realized_context as f64) * 3.0 / 4.0).round() as usize,
    }
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
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("google/gemma-4-26B-A4B-it")
        .to_string();
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

    let mut request = serde_json::Map::new();
    request.insert("model".to_string(), Value::String(model));
    request.insert("messages".to_string(), Value::Array(messages));
    if !tools.is_empty() {
        request.insert("tools".to_string(), Value::Array(tools));
    }
    let (enable_thinking, reasoning_effort) = reasoning_config(&payload);
    request.insert("enable_thinking".to_string(), Value::Bool(enable_thinking));
    if let Some(reasoning_effort) = reasoning_effort {
        request.insert(
            "reasoning_effort".to_string(),
            Value::String(reasoning_effort),
        );
    }
    for key in [
        "tool_choice",
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
            request.insert(mapped_key.to_string(), value.clone());
        }
    }
    request.insert("stream".to_string(), Value::Bool(false));
    if payload.get("parallel_tool_calls") == Some(&Value::Bool(false)) {
        request.insert("parallel_tool_calls".to_string(), Value::Bool(true));
    } else if let Some(value) = payload.get("parallel_tool_calls") {
        request.insert("parallel_tool_calls".to_string(), value.clone());
    }

    serde_json::to_vec(&Value::Object(request))
        .context("failed to encode Gemma 4 chat-completions payload")
}

pub fn rewrite_success_response(
    raw: &[u8],
    fallback_model: Option<&str>,
    _exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse chat completion response")?;
    let mut builder =
        engine::responses_turn_builder(&payload, fallback_model, "google/gemma-4-26B-A4B-it");
    let mut synthetic_tool_call_index = 0usize;
    if let Some(choices) = payload.get("choices").and_then(Value::as_array) {
        for choice in choices {
            let message = choice.get("message").and_then(Value::as_object);
            if let Some(text) = message
                .and_then(|msg| msg.get("content"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|text| !text.is_empty())
            {
                let (inline_reasoning, visible_text, gemma_tool_calls) =
                    parse_response_content(&text);
                if let Some(reasoning) = inline_reasoning.filter(|text| !text.is_empty()) {
                    builder.push_reasoning(reasoning);
                }
                if let Some(plain_text) = visible_text.filter(|text| !text.is_empty()) {
                    builder.push_message_text(plain_text);
                }
                for tool_call in gemma_tool_calls {
                    builder.push_function_call(
                        tool_call.call_id,
                        tool_call.name,
                        tool_call.arguments,
                    );
                }
            }
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
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            let call_id = format!("call_ctox_proxy_{synthetic_tool_call_index}");
                            synthetic_tool_call_index += 1;
                            call_id
                        });
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
    serde_json::to_vec(&response_payload).context("failed to encode Gemma 4 responses payload")
}

fn reasoning_config(payload: &Value) -> (bool, Option<String>) {
    let effort = payload
        .get("reasoning")
        .and_then(|value| value.get("effort"))
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| matches!(value.as_str(), "low" | "medium" | "high"));
    (effort.is_some(), effort)
}

fn build_chat_messages(items: &[Value], instructions: Option<&str>) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut tool_names_by_call_id = std::collections::BTreeMap::<String, String>::new();
    if let Some(instructions) = instructions {
        messages.push(json!({
            "role": "system",
            "content": instructions,
        }));
    }

    let mut pending_assistant: Option<serde_json::Map<String, Value>> = None;
    let flush_pending_assistant =
        |pending_assistant: &mut Option<serde_json::Map<String, Value>>,
         messages: &mut Vec<Value>| {
            if let Some(assistant) = pending_assistant.take() {
                messages.push(Value::Object(assistant));
            }
        };

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
                if mapped_role == "assistant" {
                    let text = engine::extract_message_content_text(object.get("content"));
                    flush_pending_assistant(&mut pending_assistant, &mut messages);
                    let mut assistant = serde_json::Map::new();
                    assistant.insert("role".to_string(), Value::String("assistant".to_string()));
                    let (reasoning, content, tool_calls) = parse_response_content(&text);
                    let mut rendered_content = content.unwrap_or_default();
                    for tool_call in tool_calls {
                        rendered_content.push_str(&format!(
                            "<|tool_call>call:{} {}<tool_call|>",
                            tool_call.name, tool_call.arguments
                        ));
                    }
                    assistant.insert("content".to_string(), Value::String(rendered_content));
                    if let Some(reasoning) = reasoning {
                        assistant.insert("reasoning_content".to_string(), Value::String(reasoning));
                    }
                    pending_assistant = Some(assistant);
                } else {
                    // Gemma 4 Vision accepts OpenAI chat-compat image_url
                    // content blocks; forward the full block array when the
                    // user message contains images. Falls back to flat-text
                    // for plain text messages.
                    flush_pending_assistant(&mut pending_assistant, &mut messages);
                    let blocks = engine::extract_message_content_blocks(object.get("content"));
                    if engine::message_blocks_contain_image(&blocks) {
                        messages.push(json!({
                            "role": mapped_role,
                            "content": blocks,
                        }));
                    } else {
                        let text = engine::extract_message_content_text(object.get("content"));
                        messages.push(json!({
                            "role": mapped_role,
                            "content": text,
                        }));
                    }
                }
            }
            "function_call" => {
                let call_id = object
                    .get("call_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("call_ctox_proxy_{}", tool_names_by_call_id.len()));
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let arguments = object
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                if !name.trim().is_empty() {
                    tool_names_by_call_id.insert(call_id.clone(), name.to_string());
                }
                let assistant = pending_assistant.get_or_insert_with(|| {
                    let mut assistant = serde_json::Map::new();
                    assistant.insert("role".to_string(), Value::String("assistant".to_string()));
                    assistant.insert("content".to_string(), Value::String(String::new()));
                    assistant
                });
                let existing_calls = assistant
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut tool_calls = existing_calls;
                tool_calls.push(json!({
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments
                    }
                }));
                assistant.insert("tool_calls".to_string(), Value::Array(tool_calls));
            }
            "function_call_output" => {
                flush_pending_assistant(&mut pending_assistant, &mut messages);
                let output = engine::extract_function_call_output_text(object.get("output"));
                let call_id = object
                    .get("call_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string);
                let mut message = serde_json::Map::new();
                message.insert("role".to_string(), Value::String("tool".to_string()));
                message.insert("content".to_string(), Value::String(output));
                if let Some(call_id) = call_id {
                    if let Some(name) = tool_names_by_call_id.get(&call_id) {
                        message.insert("name".to_string(), Value::String(name.clone()));
                    }
                    message.insert("tool_call_id".to_string(), Value::String(call_id));
                }
                messages.push(Value::Object(message));
            }
            _ => {}
        }
    }

    flush_pending_assistant(&mut pending_assistant, &mut messages);
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct XmlToolCall {
    call_id: String,
    name: String,
    arguments: String,
}

fn parse_response_content(text: &str) -> (Option<String>, Option<String>, Vec<XmlToolCall>) {
    let tool_call_re =
        Regex::new(r"(?s)<\|tool_call>call:([A-Za-z0-9_.-]+)\s*(\{.*?\})<tool_call\|>")
            .expect("valid Gemma 4 tool call regex");
    let channel_re = Regex::new(r"(?s)<\|channel>thought\n(.*?)<channel\|>")
        .expect("valid Gemma 4 channel regex");

    let mut reasoning_parts = Vec::new();
    let mut stripped = text.to_string();
    for captures in channel_re.captures_iter(text) {
        if let Some(reasoning) = captures.get(1).map(|capture| capture.as_str().trim()) {
            if !reasoning.is_empty() {
                reasoning_parts.push(reasoning.to_string());
            }
        }
    }
    stripped = channel_re.replace_all(&stripped, "").to_string();

    let mut tool_calls = Vec::new();
    let mut plain_text = String::new();
    let mut last_end = 0usize;
    for (index, captures) in tool_call_re.captures_iter(&stripped).enumerate() {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        plain_text.push_str(&stripped[last_end..matched.start()]);
        last_end = matched.end();
        let name = captures
            .get(1)
            .map(|capture| capture.as_str().trim().to_string())
            .unwrap_or_default();
        let arguments = captures
            .get(2)
            .map(|capture| capture.as_str().trim().to_string())
            .unwrap_or_else(|| "{}".to_string());
        tool_calls.push(XmlToolCall {
            call_id: format!("call_ctox_proxy_{index}"),
            name,
            arguments,
        });
    }
    plain_text.push_str(&stripped[last_end..]);
    let plain_text = plain_text.trim().to_string();

    (
        if reasoning_parts.is_empty() {
            None
        } else {
            Some(reasoning_parts.join("\n"))
        },
        if plain_text.is_empty() {
            None
        } else {
            Some(plain_text)
        },
        tool_calls,
    )
}
