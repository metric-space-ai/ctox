// ref: internal/translator/openai/claude/openai_claude_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

//! Faithful Rust port of upstream `ConvertClaudeRequestToOpenAI`.
//!
//! The Go implementation builds the OpenAI Chat Completions body incrementally
//! through `sjson.SetBytes` on raw JSON; this port walks an equivalent
//! `serde_json::Value` tree but preserves every ordering, omission, and
//! scalar/array choice the upstream code produces on the wire. Behaviorally
//! load-bearing details that are not negotiable:
//!
//! * Anthropic `thinking.budget_tokens`/`adaptive`/`disabled` becomes
//!   `reasoning_effort` (string, no `thinking` object). Adaptive pulls the
//!   optional `output_config.effort` passthrough first and falls back to
//!   `xhigh`. Disabled maps the zero-budget case through `convert_budget_to_level`.
//! * `tools` with no `properties` get an empty `properties: {}` injected
//!   recursively into every object schema so the OpenAI wire remains valid.
//! * Claude `tool_use` is only emitted for `role == "assistant"` and Claude
//!   `thinking` only contributes to `reasoning_content` for assistant messages
//!   that carry a signature that `compatible_signature_for_provider(GPT, ...)`
//!   accepts. This is the upstream security gate against injection of foreign
//!   reasoning state.
//! * Tool-result messages must be emitted immediately after the assistant
//!   `tool_calls` that produced them; the upstream comment notes this OpenAI
//!   adjacency rule and we follow it verbatim by flushing `tool_result`
//!   messages before the current message's own body when role is `assistant`.
//! * `redacted_thinking` is never mapped to `reasoning_content` and unsigned
//!   thinking is dropped.

use serde_json::{json, Map, Value};

use crate::internal::signature::{compatible_signature_for_provider, SignatureProvider};
use crate::internal::thinking::{convert_budget_to_level, ThinkingLevel};
use crate::internal::translator::common::{
    claude_message_system_reminder_text, join_raw_array, new_raw_array_items, set_raw_array_items,
};
use crate::internal::util::claude_attribution::is_claude_code_attribution_system_text;
use crate::internal::util::{
    strip_unsupported_schema_patterns, SCHEMA_MAP_KEYWORDS, SCHEMA_VALUE_KEYWORDS,
};

/// Produces an OpenAI Chat Completions JSON request body from an Anthropic
/// Messages request, mirroring the upstream `ConvertClaudeRequestToOpenAI`
/// signature. Returns raw JSON bytes ready for the SDK to feed downstream.
pub fn convert_claude_request_to_openai(model_name: &str, input: &[u8], stream: bool) -> Vec<u8> {
    let Ok(root) = serde_json::from_slice::<Value>(input) else {
        return input.to_vec();
    };
    let Some(root_object) = root.as_object() else {
        return input.to_vec();
    };

    // Base OpenAI Chat Completions API template. Built field-by-field; only
    // present keys are emitted, matching sjson.SetBytes behavior.
    let mut output: Vec<u8> = br#"{"model":"","messages":[]}"#.to_vec();

    output = upsert_top_level_string(&output, "model", model_name);

    if let Some(max_tokens) = value_to_integer(root_object.get("max_tokens")) {
        output = upsert_top_level_integer(&output, "max_tokens", max_tokens);
    }

    if let Some(temp) = value_to_f64(root_object.get("temperature")) {
        output = upsert_top_level_float(&output, "temperature", temp);
    } else if let Some(top_p) = value_to_f64(root_object.get("top_p")) {
        output = upsert_top_level_float(&output, "top_p", top_p);
    }

    if let Some(stops) = root_object.get("stop_sequences") {
        if let Some(array) = stops.as_array() {
            let mut collected = Vec::with_capacity(array.len());
            for value in array {
                if let Some(text) = value.as_str() {
                    collected.push(text.to_owned());
                }
            }
            if !collected.is_empty() {
                if collected.len() == 1 {
                    output = upsert_top_level_string(&output, "stop", &collected[0]);
                } else {
                    let array_value =
                        Value::Array(collected.iter().cloned().map(Value::String).collect());
                    let raw = serde_json::to_vec(&array_value).unwrap_or_else(|_| b"[]".to_vec());
                    output = set_raw_bytes(&output, "stop", &raw);
                }
            }
        }
    }

    output = upsert_top_level_bool(&output, "stream", stream);

    if let Some(thinking) = root_object.get("thinking") {
        if thinking.is_object() {
            if let Some(thinking_type) = thinking.get("type").and_then(Value::as_str) {
                match thinking_type {
                    "enabled" => {
                        if let Some(budget) = value_to_integer(thinking.get("budget_tokens")) {
                            if let Some(level) = convert_budget_to_level(budget as isize) {
                                output = apply_reasoning_effort_from_level(&output, &level);
                            }
                        } else if let Some(level) = convert_budget_to_level(-1) {
                            output = apply_reasoning_effort_from_level(&output, &level);
                        }
                    }
                    "adaptive" | "auto" => {
                        let effort = root_object
                            .get("output_config")
                            .and_then(|value| value.get("effort"))
                            .and_then(Value::as_str)
                            .map(|value| value.trim().to_ascii_lowercase())
                            .filter(|value| !value.is_empty());
                        let effort = effort.unwrap_or_else(|| "xhigh".to_owned());
                        output = upsert_top_level_string(&output, "reasoning_effort", &effort);
                    }
                    "disabled" => {
                        if let Some(level) = convert_budget_to_level(0) {
                            output = apply_reasoning_effort_from_level(&output, &level);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Messages + system. Upstream pre-sizes the array to message count plus
    // possible system, then builds a Vec<Vec<u8>> of pre-encoded JSON entries
    // that are spliced in via JoinRawArray + SetRawArrayItems so the inner
    // field order and raw tool arguments are preserved exactly.
    let message_capacity = root_object
        .get("messages")
        .and_then(Value::as_array)
        .map(|items| items.len() as i64)
        .unwrap_or(0)
        + if root_object.contains_key("system") {
            1
        } else {
            0
        };
    let mut message_items: Vec<Vec<u8>> = new_raw_array_items(message_capacity).unwrap_or_default();

    if let Some(system) = root_object.get("system") {
        let mut system_content_items: Vec<Vec<u8>> = Vec::with_capacity(2);
        if let Some(text) = system.as_str() {
            if !text.is_empty() && !is_claude_code_attribution_system_text(text) {
                let mut block = json!({"type":"text","text":""});
                if let Some(map) = block.as_object_mut() {
                    map.insert("text".into(), Value::String(text.to_owned()));
                }
                if let Ok(encoded) = serde_json::to_vec(&block) {
                    system_content_items.push(encoded);
                }
            }
        } else if let Some(parts) = system.as_array() {
            for part in parts {
                if let Some(encoded) = convert_claude_content_part_bytes(part) {
                    system_content_items.push(encoded);
                }
            }
        }
        if !system_content_items.is_empty() {
            // system: { role: "system", content: <joined raw array> }
            let mut system_message: Vec<u8> = br#"{"role":"system","content":[]}"#.to_vec();
            let joined = join_raw_array(&system_content_items);
            system_message = set_raw_bytes(&system_message, "content", &joined);
            message_items.push(system_message);
        }
    }

    if let Some(messages) = root_object.get("messages") {
        if let Some(messages) = messages.as_array() {
            for message in messages {
                convert_message(message, &mut message_items);
            }
        }
    }

    if !message_items.is_empty() {
        output = set_raw_array_items(&output, "messages", &message_items);
    }

    if let Some(tools) = root_object.get("tools") {
        if let Some(tools) = tools.as_array() {
            let mut tool_items: Vec<Vec<u8>> = Vec::with_capacity(tools.len());
            for tool in tools {
                let name = tool.get("name").and_then(Value::as_str).unwrap_or("");
                let description = tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let mut open_ai_tool: Vec<u8> =
                    br#"{"type":"function","function":{"name":"","description":""}}"#.to_vec();
                open_ai_tool = upsert_path_string(&open_ai_tool, &["function", "name"], name);
                open_ai_tool =
                    upsert_path_string(&open_ai_tool, &["function", "description"], description);
                if let Some(input_schema) = tool.get("input_schema") {
                    let mut value = input_schema.clone();
                    normalize_in_place(&mut value);
                    let raw = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
                    open_ai_tool = set_raw_bytes(&open_ai_tool, "function.parameters", &raw);
                }
                tool_items.push(open_ai_tool);
            }
            if !tool_items.is_empty() {
                output = set_raw_bytes(&output, "tools", &join_raw_array(&tool_items));
            }
        }
    }

    if let Some(tool_choice) = root_object.get("tool_choice") {
        let kind = tool_choice
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("");
        match kind {
            "auto" => output = upsert_top_level_string(&output, "tool_choice", "auto"),
            "any" => output = upsert_top_level_string(&output, "tool_choice", "required"),
            "tool" => {
                let name = tool_choice
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let raw = format!(
                    r#"{{"type":"function","function":{{"name":"{}"}}}}"#,
                    escape_json_string(name)
                );
                output = set_raw_bytes(&output, "tool_choice", raw.as_bytes());
            }
            _ => output = upsert_top_level_string(&output, "tool_choice", "auto"),
        }
    }

    if let Some(user) = root_object.get("user") {
        if let Some(text) = user.as_str() {
            output = upsert_top_level_string(&output, "user", text);
        }
    }

    output
}

fn convert_message(message: &Value, message_items: &mut Vec<Vec<u8>>) {
    let role = message.get("role").and_then(Value::as_str).unwrap_or("");
    let content = message.get("content");

    if role == "system" {
        if let Some(content) = content {
            if let Some(text) = claude_message_system_reminder_text(content) {
                let mut message_json: Vec<u8> =
                    br#"{"role":"user","content":[{"type":"text","text":""}]}"#.to_vec();
                // sjson.SetBytes with `content.0.text` writes the literal path.
                message_json = upsert_path_string(&message_json, &["content", "0", "text"], &text);
                message_items.push(message_json);
            }
        }
        return;
    }

    let Some(content) = content else {
        return;
    };

    if let Some(array) = content.as_array() {
        let mut content_items: Vec<Vec<u8>> = Vec::new();
        let mut reasoning_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<Vec<u8>> = Vec::new();
        let mut tool_results: Vec<Vec<u8>> = Vec::new();

        for part in array {
            let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
            match part_type {
                "thinking" => {
                    if role != "assistant" {
                        continue;
                    }
                    if !should_map_claude_thinking_to_gpt_reasoning(part) {
                        continue;
                    }
                    let text = get_thinking_text_value(part);
                    if !text.trim().is_empty() {
                        reasoning_parts.push(text);
                    }
                }
                "redacted_thinking" => {
                    // Always drop redacted_thinking (AC2).
                }
                "text" | "image" => {
                    if let Some(encoded) = convert_claude_content_part_bytes(part) {
                        content_items.push(encoded);
                    }
                }
                "tool_use" => {
                    if role != "assistant" {
                        continue;
                    }
                    let id = part.get("id").and_then(Value::as_str).unwrap_or("");
                    let name = part.get("name").and_then(Value::as_str).unwrap_or("");
                    let arguments = part
                        .get("input")
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "{}".to_owned());
                    let raw = format!(
                        r#"{{"id":"{}","type":"function","function":{{"name":"{}","arguments":{}}}}}"#,
                        escape_json_string(id),
                        escape_json_string(name),
                        serde_json::to_string(&arguments).unwrap_or_else(|_| "\"{}\"".to_owned()),
                    );
                    tool_calls.push(raw.into_bytes());
                }
                "tool_result" => {
                    let use_id = part
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let (content_value, raw) =
                        convert_claude_tool_result_content_value(part.get("content"));
                    let mut tool_result_message: Vec<u8> =
                        br#"{"role":"tool","tool_call_id":"","content":""}"#.to_vec();
                    tool_result_message =
                        upsert_path_string(&tool_result_message, &["tool_call_id"], use_id);
                    if raw {
                        tool_result_message = set_raw_bytes(
                            &tool_result_message,
                            "content",
                            content_value.as_bytes(),
                        );
                    } else {
                        tool_result_message =
                            upsert_path_string(&tool_result_message, &["content"], &content_value);
                    }
                    tool_results.push(tool_result_message);
                }
                _ => {}
            }
        }

        let reasoning_content = if reasoning_parts.is_empty() {
            String::new()
        } else {
            reasoning_parts.join("\n\n")
        };
        let has_content = !content_items.is_empty();
        let has_reasoning = !reasoning_content.is_empty();
        let has_tool_calls = !tool_calls.is_empty();
        let has_tool_results = !tool_results.is_empty();

        // OpenAI requires tool messages immediately after the assistant
        // tool_calls that produced them. Flush the queued tool results before
        // appending the current message body.
        for item in &tool_results {
            message_items.push(item.clone());
        }

        if role == "assistant" {
            if has_content || has_reasoning || has_tool_calls {
                let mut message_json: Vec<u8> = br#"{"role":"assistant"}"#.to_vec();
                if has_content {
                    message_json =
                        set_raw_bytes(&message_json, "content", &join_raw_array(&content_items));
                } else {
                    // OpenAI requires the `content` field even when empty.
                    message_json = upsert_path_string(&message_json, &["content"], "");
                }
                if has_reasoning {
                    message_json = upsert_path_string(
                        &message_json,
                        &["reasoning_content"],
                        &reasoning_content,
                    );
                }
                if has_tool_calls {
                    message_json =
                        set_raw_bytes(&message_json, "tool_calls", &join_raw_array(&tool_calls));
                }
                message_items.push(message_json);
            }
        } else if has_content {
            let mut message_json: Vec<u8> = br#"{"role":""}"#.to_vec();
            message_json = upsert_path_string(&message_json, &["role"], role);
            message_json = set_raw_bytes(&message_json, "content", &join_raw_array(&content_items));
            message_items.push(message_json);
        } else if has_tool_results {
            // Tool results already emitted above; no additional user message needed.
        }
    } else if let Some(text) = content.as_str() {
        let mut message_json: Vec<u8> = br#"{"role":"","content":""}"#.to_vec();
        message_json = upsert_path_string(&message_json, &["role"], role);
        message_json = upsert_path_string(&message_json, &["content"], text);
        message_items.push(message_json);
    }
}

fn convert_claude_content_part_bytes(part: &Value) -> Option<Vec<u8>> {
    let part_type = part.get("type").and_then(Value::as_str)?;
    match part_type {
        "text" => {
            let text = part.get("text").and_then(Value::as_str).unwrap_or("");
            if text.trim().is_empty() || is_claude_code_attribution_system_text(text) {
                return None;
            }
            let raw = format!(r#"{{"type":"text","text":"{}"}}"#, escape_json_string(text));
            Some(raw.into_bytes())
        }
        "image" => {
            let mut image_url = String::new();
            if let Some(source) = part.get("source") {
                let source_type = source.get("type").and_then(Value::as_str).unwrap_or("");
                match source_type {
                    "base64" => {
                        let media_type = source
                            .get("media_type")
                            .and_then(Value::as_str)
                            .unwrap_or("application/octet-stream");
                        let data = source.get("data").and_then(Value::as_str).unwrap_or("");
                        if !data.is_empty() {
                            image_url = format!("data:{};base64,{}", media_type, data);
                        }
                    }
                    "url" => {
                        image_url = source
                            .get("url")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                    }
                    _ => {}
                }
            }
            if image_url.is_empty() {
                image_url = part
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
            }
            if image_url.is_empty() {
                return None;
            }
            let raw = format!(
                r#"{{"type":"image_url","image_url":{{"url":"{}"}}}}"#,
                escape_json_string(&image_url)
            );
            Some(raw.into_bytes())
        }
        _ => None,
    }
}

fn convert_claude_tool_result_content_value(content: Option<&Value>) -> (String, bool) {
    let Some(content) = content else {
        return (String::new(), false);
    };

    if let Some(text) = content.as_str() {
        return (text.to_owned(), false);
    }

    if let Some(array) = content.as_array() {
        let mut parts: Vec<String> = Vec::new();
        let mut content_items: Vec<Vec<u8>> = Vec::with_capacity(array.len());
        let mut has_image_part = false;
        for item in array {
            if let Some(text) = item.as_str() {
                parts.push(text.to_owned());
                content_items.push(
                    format!(r#"{{"type":"text","text":"{}"}}"#, escape_json_string(text))
                        .into_bytes(),
                );
            } else if item.is_object() {
                let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
                match item_type {
                    "text" => {
                        let text = item.get("text").and_then(Value::as_str).unwrap_or("");
                        parts.push(text.to_owned());
                        content_items.push(
                            format!(r#"{{"type":"text","text":"{}"}}"#, escape_json_string(text))
                                .into_bytes(),
                        );
                    }
                    "image" => {
                        if let Some(encoded) = convert_claude_content_part_bytes(item) {
                            content_items.push(encoded);
                            has_image_part = true;
                        } else {
                            parts.push(item.to_string());
                        }
                    }
                    _ => {
                        if let Some(text) = item.get("text").and_then(Value::as_str) {
                            parts.push(text.to_owned());
                        } else {
                            parts.push(item.to_string());
                        }
                    }
                }
            } else {
                parts.push(item.to_string());
            }
        }

        if has_image_part {
            let joined = join_raw_array(&content_items);
            if let Ok(text) = String::from_utf8(joined) {
                return (text, true);
            }
        }

        let joined = parts.join("\n\n");
        if !joined.trim().is_empty() {
            return (joined, false);
        }
        return (content.to_string(), false);
    }

    if content.is_object() {
        if content.get("type").and_then(Value::as_str) == Some("image") {
            if let Some(encoded) = convert_claude_content_part_bytes(content) {
                let joined = join_raw_array(&[encoded]);
                if let Ok(text) = String::from_utf8(joined) {
                    return (text, true);
                }
            }
        }
        if let Some(text) = content.get("text").and_then(Value::as_str) {
            return (text.to_owned(), false);
        }
        return (content.to_string(), false);
    }

    (content.to_string(), false)
}

/// Returns `true` when a Claude `thinking` part carries a signature that the
/// GPT reasoning provider can replay. The upstream code path delegates the
/// decision to `sigcompat.CompatibleSignatureForProvider(SignatureProviderGPT, ...)`;
/// this port uses the local `compatible_signature_for_provider` from the
/// signature crate, which preserves the same security gate.
fn should_map_claude_thinking_to_gpt_reasoning(part: &Value) -> bool {
    let Some(signature) = part.get("signature").and_then(Value::as_str) else {
        return false;
    };
    if signature.trim().is_empty() {
        return false;
    }
    compatible_signature_for_provider(SignatureProvider::Gpt, signature).is_some()
}

fn get_thinking_text_value(part: &Value) -> String {
    if let Some(text) = part.get("text").and_then(Value::as_str) {
        return text.to_owned();
    }
    if let Some(thinking) = part.get("thinking") {
        if let Some(text) = thinking.as_str() {
            return text.to_owned();
        }
        if let Some(text) = thinking.get("text").and_then(Value::as_str) {
            return text.to_owned();
        }
        if let Some(text) = thinking.get("thinking").and_then(Value::as_str) {
            return text.to_owned();
        }
    }
    String::new()
}

fn normalize_in_place(value: &mut Value) {
    strip_unsupported_schema_patterns(value);
    normalize_object_schema_properties(value);
}

fn normalize_object_schema_properties(value: &mut Value) {
    match value {
        Value::Bool(true) => *value = Value::Object(Map::new()),
        Value::Bool(false) | Value::Null | Value::Number(_) | Value::String(_) => {}
        Value::Array(items) => {
            for child in items {
                normalize_object_schema_properties(child);
            }
        }
        Value::Object(map) => {
            let is_object = map.get("type").and_then(Value::as_str) == Some("object");
            if is_object && !map.contains_key("properties") {
                map.insert("properties".into(), Value::Object(Map::new()));
            }
            if let Some(Value::Object(pattern_props)) = map.get_mut("patternProperties") {
                for child in pattern_props.values_mut() {
                    normalize_object_schema_properties(child);
                }
            }
            for key in SCHEMA_MAP_KEYWORDS {
                if *key == "patternProperties" {
                    continue;
                }
                if let Some(Value::Object(children)) = map.get_mut(*key) {
                    for child in children.values_mut() {
                        normalize_object_schema_properties(child);
                    }
                }
            }
            for key in SCHEMA_VALUE_KEYWORDS {
                let Some(child) = map.get_mut(*key) else {
                    continue;
                };
                if *key != "additionalProperties" && child.as_bool() == Some(true) {
                    *child = Value::Object(Map::new());
                    continue;
                }
                if child.is_object() || child.is_array() {
                    normalize_object_schema_properties(child);
                }
            }
        }
    }
}

fn apply_reasoning_effort_from_level(output: &[u8], level: &ThinkingLevel) -> Vec<u8> {
    let raw = level.as_str();
    if raw.is_empty() {
        return output.to_vec();
    }
    upsert_top_level_string(output, "reasoning_effort", raw)
}

fn value_to_integer(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    value.as_i64().or_else(|| value.as_f64().map(|n| n as i64))
}

fn value_to_f64(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    value.as_f64().or_else(|| value.as_i64().map(|n| n as f64))
}

fn escape_json_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                escaped.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => escaped.push(c),
        }
    }
    escaped
}

// --- byte-level JSON helpers (mirrors of sjson.SetBytes) --------------------

fn upsert_top_level_string(data: &[u8], key: &str, value: &str) -> Vec<u8> {
    upsert_path_string(data, &[key], value)
}

fn upsert_top_level_bool(data: &[u8], key: &str, value: bool) -> Vec<u8> {
    let raw = if value { "true" } else { "false" };
    upsert_path_raw(data, &[key], raw.as_bytes())
}

fn upsert_top_level_integer(data: &[u8], key: &str, value: i64) -> Vec<u8> {
    upsert_path_raw(data, &[key], value.to_string().as_bytes())
}

fn upsert_top_level_float(data: &[u8], key: &str, value: f64) -> Vec<u8> {
    let raw = if value.fract() == 0.0 && value.is_finite() {
        // Match Go's number formatting for whole-number floats.
        format!("{:.1}", value)
    } else {
        value.to_string()
    };
    upsert_path_raw(data, &[key], raw.as_bytes())
}

fn upsert_path_string(data: &[u8], path: &[&str], value: &str) -> Vec<u8> {
    let escaped = escape_json_string(value);
    let raw = format!("\"{}\"", escaped);
    upsert_path_raw(data, path, raw.as_bytes())
}

fn upsert_path_raw(data: &[u8], path: &[&str], value: &[u8]) -> Vec<u8> {
    if path.is_empty() {
        return data.to_vec();
    }
    let Ok(document) = std::str::from_utf8(data) else {
        return data.to_vec();
    };
    let joined = path.join(".");
    let parsed = gjson::get(document, &joined);
    if parsed.exists() {
        let raw = parsed.json();
        let doc_start = document.as_ptr() as usize;
        let raw_start = raw.as_ptr() as usize;
        if raw_start >= doc_start {
            let start = raw_start - doc_start;
            if start <= data.len() && raw.len() <= data.len() - start {
                let mut output = Vec::with_capacity(data.len() - raw.len() + value.len());
                output.extend_from_slice(&data[..start]);
                output.extend_from_slice(value);
                output.extend_from_slice(&data[start + raw.len()..]);
                return output;
            }
        }
    }
    // sjson.SetRawBytes also creates a missing path; use it as a fallback.
    let Ok(mut root) = serde_json::from_slice::<Value>(data) else {
        return data.to_vec();
    };
    let Ok(value) = serde_json::from_slice::<Value>(value) else {
        return data.to_vec();
    };
    let mut segments = path.iter().peekable();
    let Some(first) = segments.next() else {
        return data.to_vec();
    };
    let mut current = &mut root;
    let mut segment = *first;
    loop {
        let Some(object) = current.as_object_mut() else {
            return data.to_vec();
        };
        if segments.peek().is_none() {
            object.insert(segment.to_owned(), value);
            break;
        }
        current = object
            .entry(segment.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        segment = *segments.next().expect("peeked path segment");
    }
    serde_json::to_vec(&root).unwrap_or_else(|_| data.to_vec())
}

fn set_raw_bytes(data: &[u8], key: &str, value: &[u8]) -> Vec<u8> {
    let Ok(document) = std::str::from_utf8(data) else {
        return data.to_vec();
    };
    let Ok(value_str) = std::str::from_utf8(value) else {
        return data.to_vec();
    };
    let parsed = gjson::get(document, key);
    if parsed.exists() {
        let raw = parsed.json();
        let doc_start = document.as_ptr() as usize;
        let raw_start = raw.as_ptr() as usize;
        if raw_start >= doc_start {
            let start = raw_start - doc_start;
            if start <= data.len() && raw.len() <= data.len() - start {
                let mut output = Vec::with_capacity(data.len() - raw.len() + value.len());
                output.extend_from_slice(&data[..start]);
                output.extend_from_slice(value);
                output.extend_from_slice(&data[start + raw.len()..]);
                return output;
            }
        }
    }
    // sjson.SetRawBytes creates missing dotted paths. Inserting `key`
    // literally would yield `"function.parameters"` beside `function` and
    // silently drop tool schemas from the OpenAI request.
    if let Ok(mut root) = serde_json::from_slice::<Value>(data) {
        if let Ok(value_json) = serde_json::from_str::<Value>(value_str) {
            let segments: Vec<&str> = key.split('.').collect();
            insert_value_path(&mut root, &segments, value_json);
            return serde_json::to_vec(&root).unwrap_or_else(|_| data.to_vec());
        }
    }
    data.to_vec()
}

fn insert_value_path(root: &mut Value, path: &[&str], value: Value) {
    let Some((head, tail)) = path.split_first() else {
        *root = value;
        return;
    };
    if tail.is_empty() {
        if let Some(object) = root.as_object_mut() {
            object.insert((*head).to_owned(), value);
        }
        return;
    }
    let Some(object) = root.as_object_mut() else {
        return;
    };
    let child = object
        .entry((*head).to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    insert_value_path(child, tail, value);
}
