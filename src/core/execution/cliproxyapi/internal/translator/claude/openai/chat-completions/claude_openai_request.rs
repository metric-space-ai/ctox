// ref: internal/translator/claude/openai/chat-completions/claude_openai_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::registry::lookup_model_info;
use crate::internal::thinking::{convert_level_to_budget, has_level, map_to_claude_effort};
use crate::internal::translator::common::{
    attach_cache_control as attach_cache_control_bytes,
    attach_message_cache_control as attach_message_cache_control_bytes,
    attach_tool_message_cache_control as attach_tool_message_cache_control_bytes,
};
use serde_json::{json, Map, Value};

/// Converts the independently gated OpenAI Chat Completions request surface
/// into Claude Messages JSON. Responses remain unavailable until their own
/// non-stream and stream parity gates land.
pub fn convert_openai_chat_request_to_claude(
    model_name: &str,
    input: &[u8],
    stream: bool,
) -> Vec<u8> {
    let Ok(root) = serde_json::from_slice::<Value>(input) else {
        return input.to_vec();
    };
    let Some(object) = root.as_object() else {
        return input.to_vec();
    };
    let mut output = Map::new();
    output.insert("model".into(), Value::String(model_name.to_owned()));
    output.insert(
        "max_tokens".into(),
        object.get("max_tokens").cloned().unwrap_or(json!(32000)),
    );
    output.insert("messages".into(), Value::Array(Vec::new()));
    // Upstream generates a process-local pseudonymous user identifier. The
    // value is deliberately opaque and normalized in differential fixtures.
    output.insert("metadata".into(), json!({"user_id":"ctox_rust_port"}));
    output.insert("stream".into(), Value::Bool(stream));

    if let Some(top_p) = object.get("top_p") {
        output.insert("top_p".into(), top_p.clone());
    }
    if let Some(stop) = object.get("stop") {
        let values = match stop {
            Value::Array(values) => values
                .iter()
                .filter_map(Value::as_str)
                .map(|value| Value::String(value.to_owned()))
                .collect::<Vec<_>>(),
            Value::String(value) => vec![Value::String(value.clone())],
            _ => Vec::new(),
        };
        if !values.is_empty() {
            output.insert("stop_sequences".into(), Value::Array(values));
        }
    }
    apply_reasoning_effort(&mut output, object.get("reasoning_effort"), model_name);

    let mut system = Vec::new();
    let mut messages = Vec::new();
    let mut previous_role = "";
    for message in object
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        match role {
            "system" | "developer" => append_system_blocks(&mut system, message),
            "user" | "assistant" => {
                let mut content = convert_message_content(message.get("content"));
                if role == "assistant" {
                    for call in message
                        .get("tool_calls")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        if call.get("type").and_then(Value::as_str) != Some("function") {
                            continue;
                        }
                        let id = sanitize_tool_id(
                            call.get("id")
                                .and_then(Value::as_str)
                                .unwrap_or("toolu_ctox"),
                        );
                        let name = call
                            .pointer("/function/name")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let arguments = call
                            .pointer("/function/arguments")
                            .and_then(Value::as_str)
                            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                            .filter(Value::is_object)
                            .unwrap_or_else(|| json!({}));
                        content.push(
                            json!({"type":"tool_use", "id":id, "name":name, "input":arguments}),
                        );
                    }
                }
                let mut converted = json!({"role":role, "content":content});
                attach_message_cache_control(&mut converted, message);
                messages.push(converted);
            }
            "tool" => {
                let id = sanitize_tool_id(
                    message
                        .get("tool_call_id")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                );
                let result = json!({
                    "type":"tool_result",
                    "tool_use_id":id,
                    "content":convert_tool_result_content(message.get("content"))
                });
                // Anthropic rejects cache_control inside tool_result.content.
                // Hoist part-level or message-level control onto the block
                // before parallel tool results are grouped.
                let mut wrapped = json!({"role":"user", "content":[result]});
                attach_tool_message_cache_control(&mut wrapped, message);
                if previous_role == "tool" {
                    let result = wrapped["content"][0].clone();
                    if let Some(parts) = messages
                        .last_mut()
                        .and_then(|value| value.get_mut("content"))
                        .and_then(Value::as_array_mut)
                    {
                        parts.push(result);
                    }
                } else {
                    messages.push(wrapped);
                }
            }
            _ => {}
        }
        previous_role = role;
    }
    if messages.is_empty() && !system.is_empty() {
        messages.push(json!({"role":"user", "content":[{"type":"text", "text":""}]}));
    }
    if !system.is_empty() {
        output.insert("system".into(), Value::Array(system));
    }
    if !messages.is_empty() {
        output.insert("messages".into(), Value::Array(messages));
    }

    let mut tools = Vec::new();
    for tool in object
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if tool.get("type").and_then(Value::as_str) != Some("function") {
            continue;
        }
        let function = tool.get("function").unwrap_or(&Value::Null);
        let mut converted = json!({
            "name":function.get("name").and_then(Value::as_str).unwrap_or(""),
            "description":function.get("description").and_then(Value::as_str).unwrap_or("")
        });
        if let Some(schema) = function
            .get("parameters")
            .or_else(|| function.get("parametersJsonSchema"))
        {
            converted["input_schema"] = normalize_schema(Some(schema));
        }
        attach_cache_control(&mut converted, tool);
        if converted.get("cache_control").is_none() {
            attach_cache_control(&mut converted, function);
        }
        tools.push(converted);
    }
    if !tools.is_empty() {
        output.insert("tools".into(), Value::Array(tools));
    }
    if let Some(choice) = convert_tool_choice(object.get("tool_choice")) {
        output.insert("tool_choice".into(), choice);
    }

    serde_json::to_vec(&Value::Object(output)).unwrap_or_else(|_| input.to_vec())
}

fn append_system_blocks(output: &mut Vec<Value>, message: &Value) {
    let start = output.len();
    match message.get("content") {
        Some(Value::String(text)) if !text.is_empty() => {
            let mut part = json!({"type":"text", "text":text});
            attach_cache_control(&mut part, message);
            output.push(part);
        }
        Some(Value::Array(parts)) => {
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("text") {
                    let mut converted = json!({
                        "type":"text",
                        "text":part.get("text").and_then(Value::as_str).unwrap_or("")
                    });
                    attach_cache_control(&mut converted, part);
                    output.push(converted);
                }
            }
            if output.len() > start {
                if let Some(last) = output.last_mut() {
                    if last.get("cache_control").is_none() {
                        attach_cache_control(last, message);
                    }
                }
            }
        }
        _ => {}
    }
}

fn convert_message_content(content: Option<&Value>) -> Vec<Value> {
    match content {
        Some(Value::String(text)) if !text.is_empty() => vec![json!({"type":"text", "text":text})],
        Some(Value::Array(parts)) => parts.iter().filter_map(convert_content_part).collect(),
        _ => Vec::new(),
    }
}

fn convert_content_part(part: &Value) -> Option<Value> {
    let mut converted = convert_content_part_raw(part)?;
    attach_cache_control(&mut converted, part);
    Some(converted)
}

fn convert_content_part_raw(part: &Value) -> Option<Value> {
    let kind = part.get("type").and_then(Value::as_str)?;
    match kind {
        "text" => Some(
            json!({"type":"text", "text":part.get("text").and_then(Value::as_str).unwrap_or("")}),
        ),
        "image_url" => media_part(part.pointer("/image_url/url")?.as_str()?, "image", false),
        "file" => media_part(part.pointer("/file/file_data")?.as_str()?, "document", true),
        _ => None,
    }
}

fn media_part(raw: &str, target: &str, data_only: bool) -> Option<Value> {
    if raw.is_empty() {
        return None;
    }
    if !raw.starts_with("data:") {
        return (!data_only).then(|| json!({"type":target, "source":{"type":"url", "url":raw}}));
    }
    let (metadata, data) = raw.split_once(',')?;
    let media_type = metadata
        .strip_prefix("data:")?
        .split(';')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("application/octet-stream");
    Some(json!({"type":target, "source":{"type":"base64", "media_type":media_type, "data":data}}))
}

fn convert_tool_result_content(content: Option<&Value>) -> Value {
    match content {
        None => Value::String(String::new()),
        Some(Value::String(text)) => Value::String(text.clone()),
        Some(Value::Array(parts)) => {
            // Raw conversion matches upstream: tool-result parts must not
            // carry cache_control. The caller hoists it onto the block.
            let converted: Vec<Value> = parts
                .iter()
                .filter_map(|part| {
                    part.as_str()
                        .map(|text| json!({"type":"text", "text":text}))
                        .or_else(|| convert_content_part_raw(part))
                })
                .collect();
            if converted.is_empty() && !parts.is_empty() {
                Value::Array(parts.clone())
            } else {
                Value::Array(converted)
            }
        }
        Some(value @ Value::Object(_)) => convert_content_part_raw(value)
            .map(|part| Value::Array(vec![part]))
            .unwrap_or_else(|| value.clone()),
        Some(value) => value.clone(),
    }
}

fn attach_cache_control(target: &mut Value, source: &Value) {
    let Ok(encoded) = serde_json::to_vec(target) else {
        return;
    };
    let updated = attach_cache_control_bytes(&encoded, source);
    if updated != encoded {
        if let Ok(value) = serde_json::from_slice(&updated) {
            *target = value;
        }
    }
}

fn attach_tool_message_cache_control(message: &mut Value, source: &Value) {
    let Ok(encoded) = serde_json::to_vec(message) else {
        return;
    };
    let updated = attach_tool_message_cache_control_bytes(&encoded, source);
    if updated != encoded {
        if let Ok(value) = serde_json::from_slice(&updated) {
            *message = value;
        }
    }
}

fn attach_message_cache_control(message: &mut Value, source: &Value) {
    let Ok(encoded) = serde_json::to_vec(message) else {
        return;
    };
    let updated = attach_message_cache_control_bytes(&encoded, source);
    if updated != encoded {
        if let Ok(value) = serde_json::from_slice(&updated) {
            *message = value;
        }
    }
}

fn normalize_schema(schema: Option<&Value>) -> Value {
    let mut object = schema
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut properties = object
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    // Claude requires an object root. OpenAI permits root combinators; preserve
    // their property vocabulary while intentionally dropping alternative
    // `required` constraints, matching upstream's `EnsureObjectSchema`.
    for union in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = object
            .remove(union)
            .and_then(|value| value.as_array().cloned())
        {
            for branch in branches {
                if let Some(branch_properties) = branch.get("properties").and_then(Value::as_object)
                {
                    for (name, value) in branch_properties {
                        properties
                            .entry(name.clone())
                            .or_insert_with(|| value.clone());
                    }
                }
            }
        }
    }
    object.insert("type".into(), Value::String("object".into()));
    object.insert("properties".into(), Value::Object(properties));
    Value::Object(object)
}

fn convert_tool_choice(choice: Option<&Value>) -> Option<Value> {
    match choice? {
        Value::String(value) if value == "auto" => Some(json!({"type":"auto"})),
        Value::String(value) if value == "required" => Some(json!({"type":"any"})),
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("function") => {
            Some(json!({
                "type":"tool",
                "name":object.get("function").and_then(|value| value.get("name")).and_then(Value::as_str).unwrap_or("")
            }))
        }
        _ => None,
    }
}

fn sanitize_tool_id(raw: &str) -> String {
    let value: String = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if value.is_empty() {
        "toolu_ctox".to_owned()
    } else {
        value
    }
}

fn apply_reasoning_effort(output: &mut Map<String, Value>, effort: Option<&Value>, model: &str) {
    let Some(effort) = effort
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
    else {
        return;
    };
    let capability = lookup_model_info(model, "claude").and_then(|info| info.thinking);
    if let Some(capability) = capability.filter(|thinking| !thinking.levels.is_empty()) {
        match effort.as_str() {
            "none" => output.insert("thinking".into(), json!({"type":"disabled"})),
            "auto" => output.insert("thinking".into(), json!({"type":"adaptive"})),
            _ => {
                let supports_max = has_level(capability.levels, "max");
                let mapped = map_to_claude_effort(&effort, supports_max);
                if let Some(mapped) = mapped {
                    output.insert("thinking".into(), json!({"type":"adaptive"}));
                    output.insert("output_config".into(), json!({"effort":mapped}));
                }
                return;
            }
        };
        return;
    }
    if let Some(budget) = convert_level_to_budget(&effort) {
        let thinking = match budget {
            0 => json!({"type":"disabled"}),
            -1 => json!({"type":"enabled"}),
            budget => json!({"type":"enabled", "budget_tokens":budget}),
        };
        output.insert("thinking".into(), thinking);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_system_media_tools_results_and_controls() {
        let input = json!({
            "max_tokens": 2048,
            "top_p": 0.8,
            "stop": ["END"],
            "reasoning_effort": "low",
            "messages": [
                {"role":"system", "content":"Be exact.", "cache_control":{"type":"ephemeral"}},
                {"role":"user", "content":[
                    {"type":"text", "text":"Inspect"},
                    {"type":"image_url", "image_url":{"url":"data:image/png;base64,AA=="}}
                ]},
                {"role":"assistant", "content":null, "tool_calls":[{
                    "id":"call:1", "type":"function",
                    "function":{"name":"lookup", "arguments":"{\"id\":1}"}
                }]},
                {"role":"tool", "tool_call_id":"call:1", "content":"ok"}
            ],
            "tools":[{"type":"function", "function":{
                "name":"lookup", "description":"Lookup",
                "parameters":{"type":"object", "properties":{"id":{"type":"integer"}}}
            }}],
            "tool_choice":{"type":"function", "function":{"name":"lookup"}}
        });
        let output: Value = serde_json::from_slice(&convert_openai_chat_request_to_claude(
            "legacy-claude",
            &serde_json::to_vec(&input).unwrap(),
            true,
        ))
        .unwrap();
        assert_eq!(output["system"][0]["text"], "Be exact.");
        assert_eq!(output["messages"][1]["content"][0]["id"], "call_1");
        assert_eq!(output["messages"][2]["content"][0]["tool_use_id"], "call_1");
        assert_eq!(output["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(output["tool_choice"]["name"], "lookup");
        assert_eq!(output["thinking"]["budget_tokens"], 1024);
        assert_eq!(output["stream"], true);
    }

    #[test]
    fn invalid_json_is_a_byte_identical_noop() {
        let input = b" {not-json ";
        assert_eq!(
            convert_openai_chat_request_to_claude("claude", input, false),
            input
        );
    }
}
