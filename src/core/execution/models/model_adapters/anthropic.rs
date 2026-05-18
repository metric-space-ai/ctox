use crate::inference::engine;
use anyhow::Context;
use serde_json::json;
use serde_json::Value;

use super::ResponsesTransportKind;

const COMPACT_INSTRUCTIONS: &str = "You are Codex running through CTOX on an Anthropic Messages runtime. Be concise and tool-accurate. Emit either one tool call or one final answer per turn. Prefer exec_command for shell work and apply_patch for file edits. Do not restate instructions.";
const DEFAULT_MAX_TOKENS: u64 = 4096;
const DEFAULT_MODEL: &str = "claude-opus-4-7";
const DISALLOWED_FUNCTION_TOOLS: &[&str] = &["apply_patch", "spawn_agent", "send_input"];

pub fn adapter_id() -> &'static str {
    "anthropic"
}

pub fn transport_kind() -> ResponsesTransportKind {
    ResponsesTransportKind::AnthropicMessages
}

pub fn upstream_path() -> &'static str {
    "/v1/messages"
}

pub fn compact_instructions() -> &'static str {
    COMPACT_INSTRUCTIONS
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
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .map(normalize_anthropic_model_id)
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let instructions = payload
        .get("instructions")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let (system_parts, mut messages) = build_messages(
        &engine::normalize_responses_input(payload.get("input")),
        instructions.as_deref(),
    );
    if messages.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": [{"type": "text", "text": ""}],
        }));
    }

    let mut request = serde_json::Map::new();
    request.insert("model".to_string(), Value::String(model));
    request.insert("messages".to_string(), Value::Array(messages));
    request.insert(
        "max_tokens".to_string(),
        payload
            .get("max_output_tokens")
            .and_then(Value::as_u64)
            .map(Value::from)
            .unwrap_or_else(|| Value::from(DEFAULT_MAX_TOKENS)),
    );
    if !system_parts.is_empty() {
        request.insert(
            "system".to_string(),
            Value::String(system_parts.join("\n\n")),
        );
    }
    for key in ["temperature", "top_p", "stop_sequences"] {
        if let Some(value) = payload.get(key) {
            request.insert(key.to_string(), value.clone());
        }
    }
    if let Some(tool_choice) = rewrite_tool_choice(payload.get("tool_choice")) {
        request.insert("tool_choice".to_string(), tool_choice);
    }
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
    if !tools.is_empty() {
        request.insert("tools".to_string(), Value::Array(tools));
    }
    request.insert("stream".to_string(), Value::Bool(false));

    serde_json::to_vec(&Value::Object(request))
        .context("failed to encode Anthropic messages payload")
}

pub fn rewrite_success_response(
    raw: &[u8],
    fallback_model: Option<&str>,
    exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse Anthropic messages response")?;
    let normalized_payload = normalize_usage(payload);
    let mut builder =
        engine::responses_turn_builder(&normalized_payload, fallback_model, DEFAULT_MODEL);
    if let Some(exact_text) = exact_text_override.filter(|text| !text.trim().is_empty()) {
        builder.push_message_text(exact_text.to_string());
    } else if let Some(content) = normalized_payload.get("content").and_then(Value::as_array) {
        for block in content {
            let block_type = block
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match block_type {
                "text" => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        builder.push_message_text(text);
                    }
                }
                "tool_use" => {
                    let call_id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("call_ctox_gateway");
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let arguments = block
                        .get("input")
                        .map(Value::to_string)
                        .unwrap_or_else(|| "{}".to_string());
                    builder.push_function_call(call_id, name, arguments);
                }
                "thinking" => {
                    if let Some(text) = block
                        .get("thinking")
                        .or_else(|| block.get("text"))
                        .and_then(Value::as_str)
                    {
                        builder.push_reasoning(text);
                    }
                }
                _ => {}
            }
        }
    }
    let response_payload = builder.build();
    serde_json::to_vec(&response_payload).context("failed to encode Anthropic responses payload")
}

fn normalize_anthropic_model_id(model: &str) -> String {
    model
        .trim()
        .strip_prefix("anthropic/")
        .unwrap_or_else(|| model.trim())
        .replace('.', "-")
}

fn build_messages(items: &[Value], instructions: Option<&str>) -> (Vec<String>, Vec<Value>) {
    let mut system_parts = Vec::new();
    if let Some(instructions) = instructions {
        system_parts.push(instructions.to_string());
    }
    let mut messages: Vec<Value> = Vec::new();

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
                match role {
                    "system" | "developer" => {
                        let text = engine::extract_message_content_text(object.get("content"));
                        if !text.trim().is_empty() {
                            system_parts.push(text);
                        }
                    }
                    "assistant" => push_message(
                        &mut messages,
                        "assistant",
                        anthropic_content_blocks(object.get("content"), false),
                    ),
                    _ => push_message(
                        &mut messages,
                        "user",
                        anthropic_content_blocks(object.get("content"), true),
                    ),
                }
            }
            "function_call" => {
                let call_id = object
                    .get("call_id")
                    .and_then(Value::as_str)
                    .or_else(|| object.get("id").and_then(Value::as_str))
                    .unwrap_or("call_ctox_gateway");
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let input = object
                    .get("arguments")
                    .and_then(Value::as_str)
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .unwrap_or_else(|| json!({}));
                push_message(
                    &mut messages,
                    "assistant",
                    vec![json!({
                        "type": "tool_use",
                        "id": call_id,
                        "name": name,
                        "input": input,
                    })],
                );
            }
            "function_call_output" => {
                let call_id = object
                    .get("call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_ctox_gateway");
                let output = engine::extract_function_call_output_text(object.get("output"));
                push_message(
                    &mut messages,
                    "user",
                    vec![json!({
                        "type": "tool_result",
                        "tool_use_id": call_id,
                        "content": output,
                    })],
                );
            }
            _ => {}
        }
    }
    (system_parts, messages)
}

fn anthropic_content_blocks(content: Option<&Value>, include_images: bool) -> Vec<Value> {
    let blocks = engine::extract_message_content_blocks(content);
    let mut mapped = Vec::new();
    for block in blocks {
        match block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "text" => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        mapped.push(json!({"type": "text", "text": text}));
                    }
                }
            }
            "image_url" if include_images => {
                let Some(url) = block
                    .get("image_url")
                    .and_then(|value| value.get("url").or(Some(value)))
                    .and_then(Value::as_str)
                    .filter(|url| !url.trim().is_empty())
                else {
                    continue;
                };
                mapped.push(json!({
                    "type": "image",
                    "source": anthropic_image_source(url),
                }));
            }
            _ => {}
        }
    }
    if mapped.is_empty() {
        let text = engine::extract_message_content_text(content);
        if !text.trim().is_empty() {
            mapped.push(json!({"type": "text", "text": text}));
        }
    }
    mapped
}

fn anthropic_image_source(url: &str) -> Value {
    if let Some(rest) = url.strip_prefix("data:") {
        if let Some((mime, data)) = rest.split_once(";base64,") {
            return json!({
                "type": "base64",
                "media_type": mime,
                "data": data,
            });
        }
    }
    json!({
        "type": "url",
        "url": url,
    })
}

fn push_message(messages: &mut Vec<Value>, role: &str, mut content: Vec<Value>) {
    if content.is_empty() {
        return;
    }
    if let Some(last) = messages.last_mut() {
        if last.get("role").and_then(Value::as_str) == Some(role) {
            if let Some(existing) = last.get_mut("content").and_then(Value::as_array_mut) {
                existing.append(&mut content);
                return;
            }
        }
    }
    messages.push(json!({
        "role": role,
        "content": content,
    }));
}

fn rewrite_tool_choice(tool_choice: Option<&Value>) -> Option<Value> {
    match tool_choice {
        Some(Value::String(value)) if value == "none" => Some(json!({"type": "none"})),
        Some(Value::String(value)) if value == "required" => Some(json!({"type": "any"})),
        Some(Value::String(value)) if value == "auto" => Some(json!({"type": "auto"})),
        Some(Value::Object(object)) => object
            .get("name")
            .and_then(Value::as_str)
            .map(|name| json!({"type": "tool", "name": name})),
        _ => None,
    }
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
            if DISALLOWED_FUNCTION_TOOLS.contains(&name) {
                return Vec::new();
            }
            vec![json!({
                "name": name,
                "description": function
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "input_schema": function
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
            })]
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

fn normalize_usage(mut payload: Value) -> Value {
    if let Some(usage) = payload.get_mut("usage").and_then(Value::as_object_mut) {
        let input = usage
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let output = usage
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        usage
            .entry("total_tokens")
            .or_insert_with(|| Value::from(input + output));
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::rewrite_request;
    use super::rewrite_success_response;
    use serde_json::json;
    use serde_json::Value;

    #[test]
    fn rewrites_responses_request_to_anthropic_messages() {
        let request = json!({
            "model": "claude-opus-4-6",
            "instructions": "You are CTOX.",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "hello"}]
                },
                {
                    "type": "function_call_output",
                    "call_id": "toolu_1",
                    "output": "ok"
                }
            ],
            "tools": [{
                "type": "function",
                "name": "lookup",
                "description": "look up data",
                "parameters": {"type": "object", "properties": {"q": {"type": "string"}}}
            }],
            "max_output_tokens": 123,
            "tool_choice": "auto"
        });
        let rewritten: Value = serde_json::from_slice(
            &rewrite_request(&serde_json::to_vec(&request).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(rewritten["model"], "claude-opus-4-6");
        assert_eq!(rewritten["system"], "You are CTOX.");
        assert_eq!(rewritten["max_tokens"], 123);
        assert_eq!(rewritten["messages"][0]["role"], "user");
        assert_eq!(rewritten["messages"][0]["content"][0]["text"], "hello");
        assert_eq!(
            rewritten["messages"][0]["content"][1]["type"],
            "tool_result"
        );
        assert_eq!(rewritten["tools"][0]["name"], "lookup");
        assert_eq!(rewritten["tool_choice"]["type"], "auto");
    }

    #[test]
    fn rewrites_anthropic_messages_response_to_ctox_responses() {
        let response = json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "model": "claude-opus-4-6",
            "content": [
                {"type": "text", "text": "done"},
                {"type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {"q": "berlin"}}
            ],
            "usage": {"input_tokens": 11, "output_tokens": 7}
        });
        let rewritten: Value = serde_json::from_slice(
            &rewrite_success_response(
                &serde_json::to_vec(&response).unwrap(),
                Some("claude-opus-4-6"),
                None,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(rewritten["object"], "response");
        assert_eq!(rewritten["id"], "resp_msg_123");
        assert_eq!(rewritten["model"], "claude-opus-4-6");
        assert_eq!(rewritten["output"][0]["type"], "message");
        assert_eq!(rewritten["output"][0]["content"][0]["text"], "done");
        assert_eq!(rewritten["output"][1]["type"], "function_call");
        assert_eq!(rewritten["output"][1]["call_id"], "toolu_1");
        assert_eq!(rewritten["output"][1]["arguments"], "{\"q\":\"berlin\"}");
        assert_eq!(rewritten["usage"]["total_tokens"], 18);
    }
}
