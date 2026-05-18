use crate::inference::engine;
use anyhow::Context;
use regex::Regex;
use serde_json::json;
use serde_json::Value;

use super::ResponsesTransportKind;

const LOCAL_COMPACT_INSTRUCTIONS: &str = "You are Codex running through CTOX on a local responses-backed runtime. Be concise and tool-accurate. Emit either one tool call or one final answer per turn. Prefer exec_command for shell work and apply_patch for file edits. Do not restate instructions. If the task requires creating or modifying files, running builds or tests, or proving a result inside a workspace, your next completion must be a tool call, not a final answer. You must not claim success, emit an exact marker, or give a final answer until tool output has verified the required result. When the user asks for an exact marker or short final answer, return only that required text after any needed tool calls and verification.";

pub fn adapter_id() -> &'static str {
    "glm47_flash"
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
        "zai-org/GLM-4.7-Flash" => 1_280.min(realized_context.saturating_sub(384)).max(896),
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
        .unwrap_or("zai-org/GLM-4.7-Flash")
        .to_string();
    let exact_text_override =
        engine::extract_exact_text_override_from_materialized_request(&payload);
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
    let messages = apply_exact_text_prompt(messages, exact_text_override.as_deref());

    let mut request = serde_json::Map::new();
    request.insert("model".to_string(), Value::String(model));
    request.insert("messages".to_string(), Value::Array(messages));
    if !tools.is_empty() {
        request.insert("tools".to_string(), Value::Array(tools));
    }
    let (mut enable_thinking, reasoning_effort) = reasoning_config(&payload);
    if exact_text_override.is_some() {
        enable_thinking = false;
    }
    request.insert("enable_thinking".to_string(), Value::Bool(enable_thinking));
    if let Some(reasoning_effort) = reasoning_effort {
        request.insert(
            "reasoning_effort".to_string(),
            Value::String(reasoning_effort),
        );
    }
    if let Some(value) = payload.get("tool_choice") {
        request.insert("tool_choice".to_string(), value.clone());
    }
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
            request.insert(mapped_key.to_string(), value.clone());
        }
    }
    request.insert("stream".to_string(), Value::Bool(false));

    serde_json::to_vec(&Value::Object(request))
        .context("failed to encode GLM chat-completions payload")
}

pub fn rewrite_success_response(
    raw: &[u8],
    fallback_model: Option<&str>,
    exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse chat completion response")?;
    let mut builder =
        engine::responses_turn_builder(&payload, fallback_model, "zai-org/GLM-4.7-Flash");
    let mut synthetic_tool_call_index = 0usize;
    let mut saw_visible_output = false;
    let mut saw_tool_call = false;
    if let Some(choices) = payload.get("choices").and_then(Value::as_array) {
        for choice in choices {
            let message = choice.get("message").and_then(Value::as_object);
            if let Some(text) = message
                .and_then(|msg| msg.get("content"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|text| !text.is_empty())
            {
                let (plain_text, xml_tool_calls) = parse_xml_tool_calls(&text);
                if let Some(plain_text) = plain_text.filter(|text| !text.is_empty()) {
                    saw_visible_output = true;
                    builder.push_message_text(plain_text);
                }
                for tool_call in xml_tool_calls {
                    saw_tool_call = true;
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
                    let call_id = format!("call_ctox_gateway_{synthetic_tool_call_index}");
                    synthetic_tool_call_index += 1;
                    saw_tool_call = true;
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
    if let Some(exact_text) = exact_text_override
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        if !saw_visible_output && !saw_tool_call {
            builder.push_message_text(exact_text.to_string());
        }
    }
    let response_payload = builder.build();
    serde_json::to_vec(&response_payload).context("failed to encode GLM responses payload")
}

fn apply_exact_text_prompt(
    mut messages: Vec<Value>,
    exact_text_override: Option<&str>,
) -> Vec<Value> {
    if let Some(prompt) = build_exact_text_prompt(exact_text_override) {
        prepend_system_message(&mut messages, prompt);
    }
    messages
}

fn prepend_system_message(messages: &mut Vec<Value>, prompt: String) {
    if prompt.trim().is_empty() {
        return;
    }
    if let Some(existing) = messages.first_mut().and_then(Value::as_object_mut) {
        if existing.get("role").and_then(Value::as_str) == Some("system") {
            let merged = existing
                .get("content")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| format!("{value}\n\n{prompt}"))
                .unwrap_or(prompt);
            existing.insert("content".to_string(), Value::String(merged));
            return;
        }
    }
    messages.insert(
        0,
        json!({
            "role": "system",
            "content": prompt,
        }),
    );
}

fn build_exact_text_prompt(exact_text_override: Option<&str>) -> Option<String> {
    let exact_text = exact_text_override
        .map(str::trim)
        .filter(|text| !text.is_empty())?;
    Some(format!(
        "Return exactly this final answer and nothing else: {exact_text}\nDo not emit reasoning or any additional text."
    ))
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
                let text = engine::extract_message_content_text(object.get("content"));
                if mapped_role == "assistant" {
                    flush_pending_assistant(&mut pending_assistant, &mut messages);
                    let mut assistant = serde_json::Map::new();
                    assistant.insert("role".to_string(), Value::String("assistant".to_string()));
                    let (reasoning, content) = split_reasoning_and_content(&text);
                    assistant.insert("content".to_string(), Value::String(content));
                    if let Some(reasoning) = reasoning {
                        assistant.insert("reasoning_content".to_string(), Value::String(reasoning));
                    }
                    pending_assistant = Some(assistant);
                } else {
                    flush_pending_assistant(&mut pending_assistant, &mut messages);
                    messages.push(json!({
                        "role": mapped_role,
                        "content": text,
                    }));
                }
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
                let assistant = pending_assistant.get_or_insert_with(|| {
                    let mut assistant = serde_json::Map::new();
                    assistant.insert("role".to_string(), Value::String("assistant".to_string()));
                    assistant.insert("content".to_string(), Value::String(String::new()));
                    assistant
                });
                let existing = assistant
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let rendered = format!("<tool_call>{name} {arguments}</tool_call>");
                let combined = if existing.trim().is_empty() {
                    rendered
                } else {
                    format!("{existing}{rendered}")
                };
                assistant.insert("content".to_string(), Value::String(combined));
            }
            "function_call_output" => {
                flush_pending_assistant(&mut pending_assistant, &mut messages);
                let output = engine::extract_function_call_output_text(object.get("output"));
                messages.push(json!({
                    "role": "tool",
                    "content": output,
                }));
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

fn split_reasoning_and_content(text: &str) -> (Option<String>, String) {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("<think>") {
        if let Some((reasoning, content)) = rest.split_once("</think>") {
            let reasoning = reasoning.trim().to_string();
            let content = content.trim().to_string();
            return (
                if reasoning.is_empty() {
                    None
                } else {
                    Some(reasoning)
                },
                content,
            );
        }
    }
    (None, trimmed.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct XmlToolCall {
    call_id: String,
    name: String,
    arguments: String,
}

fn parse_xml_tool_calls(text: &str) -> (Option<String>, Vec<XmlToolCall>) {
    let tool_call_re = Regex::new(r"(?s)<tool_call>\s*([A-Za-z0-9_.-]+)\s*(.*?)\s*</tool_call>")
        .expect("valid GLM tool call regex");
    let arg_re =
        Regex::new(r"(?s)<arg_key>\s*(.*?)\s*</arg_key>\s*<arg_value>\s*(.*?)\s*</arg_value>")
            .expect("valid GLM arg regex");
    let mut tool_calls = Vec::new();
    let mut plain_text = String::new();
    let mut last_end = 0usize;
    for (index, captures) in tool_call_re.captures_iter(text).enumerate() {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        plain_text.push_str(&text[last_end..matched.start()]);
        last_end = matched.end();
        let name = captures
            .get(1)
            .map(|capture| capture.as_str().trim().to_string())
            .unwrap_or_default();
        let body = captures
            .get(2)
            .map(|capture| capture.as_str())
            .unwrap_or_default();
        let mut arguments = serde_json::Map::new();
        for arg in arg_re.captures_iter(body) {
            let Some(key) = arg.get(1).map(|capture| capture.as_str().trim()) else {
                continue;
            };
            let value = arg
                .get(2)
                .map(|capture| capture.as_str().trim().to_string())
                .unwrap_or_default();
            arguments.insert(key.to_string(), Value::String(value));
        }
        tool_calls.push(XmlToolCall {
            call_id: format!("call_ctox_gateway_{index}"),
            name,
            arguments: Value::Object(arguments).to_string(),
        });
    }
    plain_text.push_str(&text[last_end..]);
    let plain_text = plain_text.replace("<|observation|>", "").trim().to_string();
    (
        if plain_text.is_empty() {
            None
        } else {
            Some(plain_text)
        },
        tool_calls,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glm_request_forwards_tools_array_and_tool_choice() {
        let payload = json!({
            "model": "zai-org/GLM-4.7-Flash",
            "input": [{"type":"message","role":"user","content":[{"type":"input_text","text":"Call get_cwd now."}]}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_cwd",
                    "description": "Return cwd",
                    "parameters": {"type":"object","properties":{},"additionalProperties":false}
                }
            }],
            "tool_choice": {
                "type": "function",
                "function": {
                    "name": "get_cwd",
                    "description": "Return cwd",
                    "parameters": {"type":"object","properties":{},"additionalProperties":false}
                }
            }
        });
        let raw = serde_json::to_vec(&payload).unwrap();
        let rewritten: Value = serde_json::from_slice(&rewrite_request(&raw).unwrap()).unwrap();
        let tools = rewritten
            .get("tools")
            .and_then(Value::as_array)
            .expect("tools array should be present");
        assert!(!tools.is_empty(), "tools array should not be empty");
        assert!(
            rewritten.get("tool_choice").is_some(),
            "tool_choice should be forwarded"
        );
    }

    #[test]
    fn glm_exact_text_override_fills_missing_message_text() {
        let payload = json!({
            "id": "chatcmpl_test",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": "Need exact output."
                }
            }]
        });
        let raw = serde_json::to_vec(&payload).unwrap();
        let rewritten: Value = serde_json::from_slice(
            &rewrite_success_response(&raw, Some("zai-org/GLM-4.7-Flash"), Some("CTOX_GLM_OK"))
                .unwrap(),
        )
        .unwrap();
        let output = rewritten["output"].as_array().unwrap();
        assert!(output
            .iter()
            .any(|item| item.get("type").and_then(Value::as_str) == Some("message")));
        assert_eq!(rewritten["output_text"], "CTOX_GLM_OK");
    }
}
