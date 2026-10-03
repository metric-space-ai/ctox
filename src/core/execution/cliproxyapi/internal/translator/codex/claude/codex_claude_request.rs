// ref: internal/translator/codex/claude/codex_claude_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::internal::signature::{
    compatible_signature_for_provider, detect_signature_provider_for_block,
    inspect_grok_encrypted_content, SignatureBlockKind, SignatureProvider,
};
use crate::internal::thinking::convert_budget_to_level;
use crate::internal::translator::common::claude_message_system_reminder_text;
use crate::internal::util::{
    is_claude_code_attribution_system_text, strip_unsupported_schema_patterns,
};

const CODEX_NAME_LIMIT: usize = 64;

/// Translates Anthropic Messages input directly to the Codex Responses wire.
/// The order-sensitive message walk deliberately stays local instead of using
/// an intermediate chat shape: encrypted reasoning and function calls split
/// message items at their exact source positions.
pub fn convert_claude_request_to_codex(
    model_name: &str,
    input_raw_json: &[u8],
    stream: bool,
) -> Vec<u8> {
    convert_claude_request_to_codex_impl(model_name, input_raw_json, stream, false)
}

/// ref: internal/translator/codex/claude/codex_claude_request.go:48-54
/// Candidate e2bff010: compatibility endpoints retain empty or unknown thinking
/// signatures while recognized incompatible provider signatures remain excluded.
pub fn convert_claude_request_to_codex_with_compat(
    model_name: &str,
    input_raw_json: &[u8],
    stream: bool,
) -> Vec<u8> {
    convert_claude_request_to_codex_impl(model_name, input_raw_json, stream, true)
}

fn convert_claude_request_to_codex_impl(
    model_name: &str,
    input_raw_json: &[u8],
    _stream: bool,
    preserve_empty_thinking_blocks: bool,
) -> Vec<u8> {
    let Ok(root) = serde_json::from_slice::<Value>(input_raw_json) else {
        return input_raw_json.to_vec();
    };
    let Some(object) = root.as_object() else {
        return input_raw_json.to_vec();
    };

    let tool_names = shortened_tool_names(object.get("tools"));
    let mut input = Vec::new();
    append_system(object.get("system"), &mut input);
    if let Some(messages) = object.get("messages").and_then(Value::as_array) {
        for message in messages {
            append_message(
                model_name,
                message,
                &tool_names,
                &mut input,
                preserve_empty_thinking_blocks,
            );
        }
    }

    let tools = convert_tools(object.get("tools"), &tool_names);
    let web_search_names = declared_web_search_names(object.get("tools"));
    let mut out = json!({
        "model": model_name,
        "instructions": "",
        "input": input,
        "parallel_tool_calls": !object.get("tool_choice")
            .and_then(|value| value.get("disable_parallel_tool_use"))
            .and_then(Value::as_bool).unwrap_or(false),
        "reasoning": {"effort": reasoning_effort(object)},
        "stream": true,
        "store": false,
        "include": ["reasoning.encrypted_content"],
    });
    if object.contains_key("tools") {
        out["tools"] = Value::Array(tools);
        out["tool_choice"] =
            convert_tool_choice(object.get("tool_choice"), &tool_names, &web_search_names);
    }
    if service_tier(object).is_some() {
        out["service_tier"] = Value::String("priority".into());
    }
    serde_json::to_vec(&out).unwrap_or_else(|_| input_raw_json.to_vec())
}

fn append_system(system: Option<&Value>, input: &mut Vec<Value>) {
    let mut content = Vec::new();
    match system {
        Some(Value::String(text)) => append_system_text(text, &mut content),
        Some(Value::Array(parts)) => {
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("text") {
                    append_system_text(
                        part.get("text").and_then(Value::as_str).unwrap_or_default(),
                        &mut content,
                    );
                }
            }
        }
        _ => {}
    }
    if !content.is_empty() {
        input.push(json!({"type":"message","role":"developer","content":content}));
    }
}

fn append_system_text(text: &str, content: &mut Vec<Value>) {
    if !text.is_empty() && !is_claude_code_attribution_system_text(text) {
        content.push(json!({"type":"input_text","text":text}));
    }
}

fn append_message(
    model_name: &str,
    message: &Value,
    tool_names: &HashMap<String, String>,
    input: &mut Vec<Value>,
    preserve_empty_thinking_blocks: bool,
) {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let content = message.get("content").unwrap_or(&Value::Null);
    if role == "system" {
        if let Some(text) = claude_message_system_reminder_text(content) {
            input.push(json!({"type":"message","role":"user","content":[{"type":"input_text","text":text}]}));
        }
        return;
    }
    if let Some(text) = content.as_str() {
        input.push(message_item(role, vec![text_part(role, text)]));
        return;
    }
    let Some(parts) = content.as_array() else {
        return;
    };
    let mut buffered = Vec::new();
    for part in parts {
        match part.get("type").and_then(Value::as_str).unwrap_or_default() {
            "text" => buffered.push(text_part(
                role,
                part.get("text").and_then(Value::as_str).unwrap_or_default(),
            )),
            "image" => {
                if let Some(url) = image_data_url(part) {
                    buffered.push(json!({"type":"input_image","image_url":url}));
                }
            }
            "thinking" if role == "assistant" => {
                let signature = compatible_reasoning_signature(
                    model_name, part, preserve_empty_thinking_blocks,
                );
                if let Some(signature) = signature {
                    flush_message(role, &mut buffered, input);
                    input.push(json!({
                        "type":"reasoning",
                        "summary":[],
                        "content":Value::Null,
                        "encrypted_content":signature,
                    }));
                }
            }
            "tool_use" => {
                flush_message(role, &mut buffered, input);
                let raw_name = part.get("name").map(value_as_string).unwrap_or_default();
                input.push(json!({
                    "type":"function_call",
                    "call_id":shorten_call_id(part.get("id").and_then(Value::as_str).unwrap_or_default()),
                    "name":tool_names.get(&raw_name).cloned().unwrap_or_else(|| shorten_name(&raw_name)),
                    "arguments":part.get("input").cloned().unwrap_or_else(|| json!({})),
                }));
            }
            "tool_result" => {
                flush_message(role, &mut buffered, input);
                input.push(json!({
                    "type":"function_call_output",
                    "call_id":shorten_call_id(part.get("tool_use_id").and_then(Value::as_str).unwrap_or_default()),
                    "output":tool_result_output(part.get("content")),
                }));
            }
            _ => {}
        }
    }
    flush_message(role, &mut buffered, input);
}

fn flush_message(role: &str, buffered: &mut Vec<Value>, input: &mut Vec<Value>) {
    if !buffered.is_empty() {
        input.push(message_item(role, std::mem::take(buffered)));
    }
}

fn message_item(role: &str, content: Vec<Value>) -> Value {
    json!({"type":"message","role":role,"content":content})
}

fn text_part(role: &str, text: &str) -> Value {
    json!({"type":if role == "assistant" {"output_text"} else {"input_text"},"text":text})
}

fn image_data_url(part: &Value) -> Option<String> {
    let source = part.get("source")?;
    if source.get("type").and_then(Value::as_str) == Some("url") {
        return source.get("url").and_then(Value::as_str).map(str::to_owned);
    }
    let data = source
        .get("data")
        .or_else(|| source.get("base64"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?;
    let media_type = source
        .get("media_type")
        .or_else(|| source.get("mime_type"))
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream");
    Some(format!("data:{media_type};base64,{data}"))
}

fn tool_result_output(content: Option<&Value>) -> Value {
    let Some(content) = content else {
        return Value::String(String::new());
    };
    if let Some(text) = content.as_str() {
        return Value::String(text.to_owned());
    }
    if let Some(parts) = content.as_array() {
        let converted: Vec<Value> = parts
            .iter()
            .filter_map(|part| match part.get("type").and_then(Value::as_str) {
                Some("text") => Some(json!({"type":"input_text","text":part.get("text").and_then(Value::as_str).unwrap_or_default()})),
                Some("image") => image_data_url(part)
                    .map(|url| json!({"type":"input_image","image_url":url})),
                _ => None,
            })
            .collect();
        if !converted.is_empty() {
            return Value::Array(converted);
        }
    }
    Value::String(value_as_string(content))
}

// ref: internal/translator/codex/claude/codex_claude_request.go:161-185
fn compatible_reasoning_signature(
    model_name: &str,
    part: &Value,
    preserve_empty_thinking_blocks: bool,
) -> Option<String> {
    let raw = part.get("signature").map(value_as_string).unwrap_or_default();
    if let Some(signature) = compatible_signature_for_provider(SignatureProvider::Gpt, &raw) {
        return Some(signature);
    }
    // Null and absent signatures match gjson's empty-string fallback. Other
    // non-string values cannot use the unknown-signature compatibility branch.
    if preserve_empty_thinking_blocks
        && (raw.trim().is_empty()
            || (part.get("signature").is_some_and(Value::is_string)
                && detect_signature_provider_for_block(&raw, SignatureBlockKind::ClaudeThinking)
                    == SignatureProvider::Unknown))
    {
        return Some(raw);
    }
    model_name
        .trim()
        .to_ascii_lowercase()
        .contains("grok")
        .then(|| inspect_grok_encrypted_content(&raw).ok().map(|_| raw.clone()))
        .flatten()
}

fn convert_tools(tools: Option<&Value>, names: &HashMap<String, String>) -> Vec<Value> {
    tools
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|tool| {
            if is_web_search(tool) {
                let mut out = json!({"type":"web_search"});
                if let Some(domains) = tool.get("allowed_domains").filter(|v| v.is_array()) {
                    out["filters"] = json!({"allowed_domains":domains});
                }
                if let Some(location) = tool.get("user_location").filter(|v| v.is_object()) {
                    out["user_location"] = location.clone();
                }
                return out;
            }
            let raw_name = tool.get("name").map(value_as_string).unwrap_or_default();
            let mut schema = tool
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({}));
            normalize_schema(&mut schema);
            let mut out = Map::new();
            out.insert("type".into(), Value::String("function".into()));
            out.insert(
                "name".into(),
                Value::String(
                    names
                        .get(&raw_name)
                        .cloned()
                        .unwrap_or_else(|| shorten_name(&raw_name)),
                ),
            );
            if let Some(description) = tool.get("description") {
                out.insert("description".into(), description.clone());
            }
            out.insert("parameters".into(), schema);
            out.insert("strict".into(), Value::Bool(false));
            Value::Object(out)
        })
        .collect()
}

fn convert_tool_choice(
    choice: Option<&Value>,
    names: &HashMap<String, String>,
    web_search_names: &HashSet<String>,
) -> Value {
    let Some(choice) = choice else {
        return Value::String("auto".into());
    };
    let kind = choice
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| choice.as_str())
        .unwrap_or_default();
    match kind {
        "any" => Value::String("required".into()),
        "none" => Value::String("none".into()),
        "tool" => {
            let name = choice.get("name").map(value_as_string).unwrap_or_default();
            if web_search_names.contains(&name) {
                json!({"type":"web_search"})
            } else if name.is_empty() {
                Value::String("auto".into())
            } else {
                json!({"type":"function","name":names.get(&name).cloned().unwrap_or_else(|| shorten_name(&name))})
            }
        }
        _ => Value::String("auto".into()),
    }
}

fn is_web_search(tool: &Value) -> bool {
    matches!(
        tool.get("type").and_then(Value::as_str),
        Some("web_search_20250305" | "web_search_20260209")
    )
}

fn declared_web_search_names(tools: Option<&Value>) -> HashSet<String> {
    tools
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|tool| is_web_search(tool))
        .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

fn shortened_tool_names(tools: Option<&Value>) -> HashMap<String, String> {
    let raw_names: Vec<String> = tools
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|tool| tool.get("name").map(value_as_string).unwrap_or_default())
        .filter(|name| !name.is_empty())
        .collect();
    let mut used = HashSet::new();
    let mut map = HashMap::new();
    for raw in raw_names {
        let base = shorten_name(&raw);
        let mut candidate = base.clone();
        let mut index = 1;
        while used.contains(&candidate) {
            let suffix = format!("_{index}");
            candidate = truncate_utf8(&base, CODEX_NAME_LIMIT.saturating_sub(suffix.len()));
            candidate.push_str(&suffix);
            index += 1;
        }
        used.insert(candidate.clone());
        map.insert(raw, candidate);
    }
    map
}

fn shorten_name(name: &str) -> String {
    if name.len() <= CODEX_NAME_LIMIT {
        return name.to_owned();
    }
    if let Some(tail) = name
        .strip_prefix("mcp__")
        .and_then(|rest| rest.rsplit_once("__").map(|(_, tail)| tail))
    {
        return truncate_utf8(&format!("mcp__{tail}"), CODEX_NAME_LIMIT);
    }
    truncate_utf8(name, CODEX_NAME_LIMIT)
}

fn shorten_call_id(id: &str) -> String {
    if id.len() <= CODEX_NAME_LIMIT {
        return id.to_owned();
    }
    let digest = Sha256::digest(id.as_bytes());
    let suffix = format!(
        "_{}",
        digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let mut prefix = truncate_utf8(id, CODEX_NAME_LIMIT - suffix.len());
    prefix.push_str(&suffix);
    prefix
}

fn truncate_utf8(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn normalize_schema(value: &mut Value) {
    strip_unsupported_schema_patterns(value);
    normalize_schema_shape(value);
}

fn normalize_schema_shape(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("object")
                && !object.contains_key("properties")
            {
                object.insert("properties".into(), json!({}));
            }
            object.remove("$schema");
            object.remove("$id");
            for child in object.values_mut() {
                normalize_schema_shape(child);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(normalize_schema_shape),
        _ => {}
    }
}

fn reasoning_effort(root: &Map<String, Value>) -> String {
    let Some(thinking) = root.get("thinking").and_then(Value::as_object) else {
        return "medium".into();
    };
    match thinking
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "enabled" => thinking
            .get("budget_tokens")
            .and_then(Value::as_i64)
            .and_then(|budget| convert_budget_to_level(budget as isize))
            .map(|level| level.as_str().to_owned())
            .unwrap_or_else(|| "medium".into()),
        "adaptive" | "auto" => root
            .get("output_config")
            .and_then(|value| value.get("effort"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| "xhigh".into()),
        "disabled" => convert_budget_to_level(0)
            .map(|level| level.as_str().to_owned())
            .unwrap_or_else(|| "medium".into()),
        _ => "medium".into(),
    }
}

fn service_tier(root: &Map<String, Value>) -> Option<()> {
    let tier = root
        .get("service_tier")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_ascii_lowercase);
    let speed = root
        .get("speed")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_ascii_lowercase);
    matches!(tier.as_deref(), Some("fast" | "priority"))
        .then_some(())
        .or_else(|| (speed.as_deref() == Some("fast")).then_some(()))
}

fn value_as_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| match value {
            Value::Null => String::new(),
            other => other.to_string(),
        })
}
