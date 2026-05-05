use crate::inference::engine;
use anyhow::Context;
use regex::Regex;
use serde_json::json;
use serde_json::Value;

use super::ResponsesTransportKind;

const LOCAL_COMPACT_INSTRUCTIONS: &str = "You are Codex running through CTOX on a local responses-backed runtime. Be concise and tool-accurate. Emit either one tool call or one final answer per turn. Prefer exec_command for shell work and apply_patch for file edits. Do not restate instructions. If the task requires creating or modifying files, running builds or tests, or proving a result inside a workspace, your next completion must be a tool call, not a final answer. You must not claim success, emit an exact marker, or give a final answer until tool output has verified the required result. When the user asks for an exact marker or short final answer, return only that required text after any needed tool calls and verification.";
const QWEN_TOOL_PROTOCOL_INSTRUCTIONS: &str = r#"Local CTOX tool-call protocol:
- Available tools: {tool_names}
- If the next action requires a command, filesystem change, runtime inspection, benchmark run, ticket/state update, or artifact verification, your entire assistant message must be exactly one tool call and no prose.
- Emit tool calls in this XML format:
<tool_call>
<function=exec_command>
<parameter=cmd>
printf hello
</parameter>
</function>
</tool_call>
- Use the real tool name from the available tools list. Use parameter names from the tool schema, for example cmd for exec_command.
- Do not use Markdown fences for tool calls. Do not describe the command instead of calling the tool.
- After tool output is returned, either call another tool or give the final answer only when the requested durable result has been verified."#;

pub fn adapter_id() -> &'static str {
    "qwen3_5"
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
        "Qwen/Qwen3.5-35B-A3B" | "Qwen/Qwen3.6-35B-A3B" => {
            1_536.min(realized_context.saturating_sub(256)).max(1_024)
        }
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
        .unwrap_or("Qwen/Qwen3.5-4B")
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
    let (messages, tools) = build_request_parts(&payload, messages);
    let messages = inject_tool_protocol_message(messages, &tools);

    let mut request = serde_json::Map::new();
    request.insert("model".to_string(), Value::String(model));
    request.insert("messages".to_string(), Value::Array(messages));
    if !tools.is_empty() {
        request.insert("tools".to_string(), Value::Array(tools));
    }
    let enable_thinking = payload
        .get("reasoning")
        .and_then(|value| value.get("effort"))
        .and_then(Value::as_str)
        .is_some();
    request.insert("enable_thinking".to_string(), Value::Bool(enable_thinking));
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
        .context("failed to encode Qwen chat-completions payload")
}

pub fn rewrite_success_response(
    raw: &[u8],
    fallback_model: Option<&str>,
    _exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse chat completion response")?;
    let mut builder = engine::responses_turn_builder(&payload, fallback_model, "Qwen/Qwen3.5-4B");
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
                    builder.push_message_text(plain_text);
                }
                for tool_call in xml_tool_calls {
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
                    let name = tool_call
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let arguments = tool_call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(Value::as_str)
                        .unwrap_or("{}");
                    let call_id = tool_call
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("call_ctox_gateway");
                    builder.push_function_call(call_id, name, arguments);
                }
            }
        }
    }
    let response_payload = builder.build();
    serde_json::to_vec(&response_payload).context("failed to encode Qwen responses payload")
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

fn inject_tool_protocol_message(mut messages: Vec<Value>, tools: &[Value]) -> Vec<Value> {
    if tools.is_empty() {
        return messages;
    }
    let tool_names = tool_names(tools);
    let tool_names = if tool_names.is_empty() {
        "exec_command".to_string()
    } else {
        tool_names.join(", ")
    };
    let instructions = QWEN_TOOL_PROTOCOL_INSTRUCTIONS.replace("{tool_names}", &tool_names);

    if let Some(system) = messages
        .iter_mut()
        .find(|message| message.get("role").and_then(Value::as_str) == Some("system"))
    {
        let existing = system
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let merged = if existing.is_empty() {
            instructions
        } else {
            format!("{existing}\n\n{instructions}")
        };
        if let Some(object) = system.as_object_mut() {
            object.insert("content".to_string(), Value::String(merged));
        }
    } else {
        messages.insert(
            0,
            json!({
                "role": "system",
                "content": instructions,
            }),
        );
    }
    messages
}

fn tool_names(tools: &[Value]) -> Vec<String> {
    let mut names = Vec::new();
    for tool in tools {
        let name = tool
            .get("function")
            .and_then(|function| function.get("name"))
            .or_else(|| tool.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty());
        if let Some(name) = name {
            if !names.iter().any(|existing| existing == name) {
                names.push(name.to_string());
            }
        }
    }
    names
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
                if mapped_role == "assistant" {
                    let text = engine::extract_message_content_text(object.get("content"));
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
                    // Qwen 3.5 Vision accepts OpenAI chat-compat image_url
                    // content blocks; forward the block array when the user
                    // message carries images, otherwise fall back to the
                    // flat-text legacy path.
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
                let rendered = render_xml_tool_call(name, arguments);
                let combined = if existing.trim().is_empty() {
                    rendered
                } else {
                    format!("{existing}{rendered}")
                };
                assistant.insert("content".to_string(), Value::String(combined));
            }
            "function_call_output" => {
                flush_pending_assistant(&mut pending_assistant, &mut messages);
                let call_id = object
                    .get("call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_ctox_gateway");
                let output = engine::extract_function_call_output_text(object.get("output"));
                messages.push(json!({
                    "role": "user",
                    "content": format!("<tool_response>\n{}\n</tool_response>", output.trim_end()),
                    "tool_call_id": call_id,
                }));
            }
            _ => {}
        }
    }

    flush_pending_assistant(&mut pending_assistant, &mut messages);
    messages
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
        rendered.push('>');
        let value_text = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| value.to_string());
        rendered.push_str(&value_text);
        rendered.push_str("</parameter>\n");
    }
    rendered.push_str("</function>\n</tool_call>\n");
    rendered
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
    let tool_call_re = Regex::new(
        r"(?s)<tool_call>\s*<function=([A-Za-z0-9_.-]+)>\s*(.*?)\s*</function>\s*</tool_call>",
    )
    .expect("valid Qwen tool call regex");
    let param_re = Regex::new(r"(?s)<parameter=([A-Za-z0-9_.-]+)>\s*(.*?)\s*</parameter>")
        .expect("valid parameter regex");
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
        for param in param_re.captures_iter(body) {
            let Some(param_name) = param.get(1).map(|capture| capture.as_str().trim()) else {
                continue;
            };
            let value = param
                .get(2)
                .map(|capture| capture.as_str().trim().to_string())
                .unwrap_or_default();
            arguments.insert(param_name.to_string(), Value::String(value));
        }
        tool_calls.push(XmlToolCall {
            call_id: format!("call_ctox_gateway_{index}"),
            name,
            arguments: Value::Object(arguments).to_string(),
        });
    }
    plain_text.push_str(&text[last_end..]);
    let plain_text = plain_text.trim().to_string();
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
    fn request_with_tools_injects_local_tool_protocol() {
        let payload = json!({
            "model": "Qwen/Qwen3.6-35B-A3B",
            "tools": [{
                "type": "function",
                "function": {
                    "name": "exec_command",
                    "parameters": {
                        "type": "object",
                        "properties": {"cmd": {"type": "string"}},
                        "required": ["cmd"]
                    }
                }
            }],
            "input": [{
                "type": "message",
                "role": "user",
                "content": [{"type":"input_text", "text":"Create REQUIRED ARTIFACT x.txt"}]
            }]
        });

        let rewritten = rewrite_request(&serde_json::to_vec(&payload).unwrap()).unwrap();
        let request: Value = serde_json::from_slice(&rewritten).unwrap();
        let system = request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|message| message["role"] == "system")
            .and_then(|message| message["content"].as_str())
            .unwrap();

        assert!(system.contains("Local CTOX tool-call protocol"));
        assert!(system.contains("Available tools: exec_command"));
        assert!(system.contains("<function=exec_command>"));
        assert!(system.contains("<parameter=cmd>\nprintf hello\n</parameter>"));
    }

    #[test]
    fn parses_qwen_xml_tool_call_with_dotted_tool_name() {
        let text = r#"<tool_call>
<function=namespace.exec_command>
<parameter=cmd>
printf CTOX_OK
</parameter>
</function>
</tool_call>"#;
        let (plain, calls) = parse_xml_tool_calls(text);

        assert_eq!(plain, None);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "namespace.exec_command");
        assert_eq!(calls[0].arguments, json!({"cmd":"printf CTOX_OK"}).to_string());
    }
}
