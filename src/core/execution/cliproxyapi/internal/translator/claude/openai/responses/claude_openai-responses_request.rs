// ref: internal/translator/claude/openai/responses/claude_openai-responses_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::registry::lookup_model_info;
use crate::internal::signature::{compatible_signature_for_provider, SignatureProvider};
use crate::internal::thinking::{convert_level_to_budget, has_level, map_to_claude_effort};
use serde_json::{json, Map, Value};

use super::response::CLAUDE_RESPONSES_REDACTED_THINKING_PREFIX;

pub fn convert_openai_responses_request_to_claude(
    model_name: &str,
    input: &[u8],
    stream: bool,
) -> Vec<u8> {
    convert_openai_responses_request_to_claude_impl(model_name, input, stream, false)
}

/// ref: internal/translator/claude/openai/responses/claude_openai-responses_request.go:40-46
/// Candidate e2bff010: preserve empty and opaque reasoning for compatibility endpoints.
pub fn convert_openai_responses_request_to_claude_with_compat(
    model_name: &str,
    input: &[u8],
    stream: bool,
) -> Vec<u8> {
    convert_openai_responses_request_to_claude_impl(model_name, input, stream, true)
}

fn convert_openai_responses_request_to_claude_impl(
    model_name: &str,
    input: &[u8],
    stream: bool,
    preserve_empty_thinking_blocks: bool,
) -> Vec<u8> {
    let Ok(root) = serde_json::from_slice::<Value>(input) else {
        return input.to_vec();
    };
    let mut messages = Vec::new();
    let mut system_blocks = Vec::new();
    let mut pending_tools: Vec<(String, Value)> = Vec::new();
    let mut pending_reasoning = Vec::new();

    if let Some(instructions) = root
        .get("instructions")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
    {
        append_system_text(&mut system_blocks, instructions, None);
    }

    for item in root
        .get("input")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| is_system_level_role(item.get("role").and_then(Value::as_str)))
    {
        let start = system_blocks.len();
        match item.get("content") {
            Some(Value::String(text)) => append_system_text(&mut system_blocks, text, None),
            Some(Value::Array(parts)) => {
                for part in parts {
                    match part.get("type").and_then(Value::as_str).unwrap_or("") {
                        "input_text" | "output_text" | "text" => append_system_text(
                            &mut system_blocks,
                            part.get("text").and_then(Value::as_str).unwrap_or(""),
                            part.get("cache_control"),
                        ),
                        unsupported if !unsupported.trim().is_empty() => {
                            system_blocks.push(json!({"type":unsupported.trim()}));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if system_blocks.len() > start {
            if let Some(cache) = item.get("cache_control").filter(|value| value.is_object()) {
                let last = system_blocks.last_mut().expect("system block was appended");
                if last.get("cache_control").is_none() {
                    last["cache_control"] = cache.clone();
                }
            }
        }
    }

    for item in root
        .get("input")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if is_system_level_role(item.get("role").and_then(Value::as_str)) {
            continue;
        }
        let kind = item.get("type").and_then(Value::as_str).unwrap_or_else(|| {
            if item.get("role").is_some() {
                "message"
            } else {
                ""
            }
        });
        match kind {
            "message" => {
                let role = normalized_role(item.get("role").and_then(Value::as_str));
                if role != "assistant" {
                    flush_reasoning(&mut messages, &mut pending_reasoning);
                    flush_tools(&mut messages, &mut pending_tools);
                }
                if let Some(mut message) = convert_message(item, role) {
                    if role == "assistant" && !pending_reasoning.is_empty() {
                        prepend_reasoning(&mut message, &mut pending_reasoning);
                    }
                    messages.push(message);
                }
            }
            "reasoning" => {
                if let Some(part) = convert_reasoning(item, preserve_empty_thinking_blocks) {
                    pending_reasoning.push(part);
                }
            }
            "function_call" => {
                let call_id =
                    sanitize_tool_id(item.get("call_id").and_then(Value::as_str).unwrap_or(""));
                let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                let arguments = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .filter(Value::is_object)
                    .unwrap_or_else(|| json!({}));
                let mut content = std::mem::take(&mut pending_reasoning);
                content
                    .push(json!({"type":"tool_use", "id":call_id, "name":name, "input":arguments}));
                pending_tools.push((
                    call_id.clone(),
                    json!({"role":"assistant", "content":content}),
                ));
            }
            "function_call_output" => {
                flush_reasoning(&mut messages, &mut pending_reasoning);
                let call_id =
                    sanitize_tool_id(item.get("call_id").and_then(Value::as_str).unwrap_or(""));
                flush_tool_for(&mut messages, &mut pending_tools, &call_id);
                messages.push(json!({"role":"user", "content":[{
                    "type":"tool_result", "tool_use_id":call_id,
                    "content":convert_tool_result_content(item.get("output"))
                }]}));
            }
            _ => {}
        }
    }
    flush_reasoning(&mut messages, &mut pending_reasoning);
    flush_tools(&mut messages, &mut pending_tools);
    // ref: internal/translator/claude/openai/responses/claude_openai-responses_request.go:532-550 @ 16d98881d4bb37adaa827599e4be8f5154e81646
    let had_messages = !messages.is_empty();
    if !preserve_empty_thinking_blocks {
        strip_trailing_claude_thinking_blocks(&mut messages);
    }
    // Interrupted tools must be answered before checking for assistant prefill.
    messages = repair_claude_tool_pairing(messages);
    if !preserve_empty_thinking_blocks
        && claude_model_rejects_assistant_prefill(model_name)
        && messages
            .last()
            .and_then(|message| message.get("role"))
            .and_then(Value::as_str)
            .is_some_and(|role| role.trim().eq_ignore_ascii_case("assistant"))
    {
        messages.pop();
    }
    if messages.is_empty() && (!system_blocks.is_empty() || had_messages) {
        messages.push(json!({"role":"user", "content":[{"type":"text", "text":""}]}));
    }

    let mut output = Map::new();
    output.insert("model".into(), Value::String(model_name.into()));
    output.insert(
        "max_tokens".into(),
        root.get("max_output_tokens")
            .cloned()
            .unwrap_or(json!(32000)),
    );
    output.insert("messages".into(), Value::Array(messages));
    if !system_blocks.is_empty() {
        output.insert("system".into(), Value::Array(system_blocks));
    }
    output.insert("metadata".into(), json!({"user_id":"ctox_rust_port"}));
    output.insert("stream".into(), Value::Bool(stream));
    apply_reasoning_effort(&mut output, root.pointer("/reasoning/effort"), model_name);

    let mut included_names = Vec::new();
    let mut tools = Vec::new();
    for tool in root
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        convert_tool(tool, &mut tools, &mut included_names);
    }
    if !tools.is_empty() {
        output.insert("tools".into(), Value::Array(tools));
    }
    if let Some(choice) = convert_tool_choice(root.get("tool_choice"), &included_names) {
        output.insert("tool_choice".into(), choice);
    }

    serde_json::to_vec(&Value::Object(output)).unwrap_or_else(|_| input.to_vec())
}

// ref: internal/translator/claude/openai/responses/claude_openai-responses_request.go:678-771 @ 16d98881d4bb37adaa827599e4be8f5154e81646
fn strip_trailing_claude_thinking_blocks(messages: &mut Vec<Value>) {
    let Some(last) = messages.last_mut() else {
        return;
    };
    if !last
        .get("role")
        .and_then(Value::as_str)
        .is_some_and(|role| role.trim().eq_ignore_ascii_case("assistant"))
    {
        return;
    }
    let Some(parts) = last.get_mut("content").and_then(Value::as_array_mut) else {
        return;
    };
    let original_len = parts.len();
    while parts.last().is_some_and(|part| {
        matches!(
            part.get("type").and_then(Value::as_str).map(str::trim),
            Some("thinking" | "redacted_thinking")
        )
    }) {
        parts.pop();
    }
    if parts.len() == original_len {
        return;
    }
    if parts.is_empty() {
        messages.pop();
        return;
    }
    let replacement = (parts.len() == 1
        && parts[0].get("type").and_then(Value::as_str) == Some("text")
        && parts[0].get("cache_control").is_none()
        && parts[0].get("citations").is_none())
    .then(|| {
        Value::String(
            parts[0]
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        )
    });
    if let Some(replacement) = replacement {
        last["content"] = replacement;
    }
}

fn claude_model_rejects_assistant_prefill(model_name: &str) -> bool {
    let normalized = model_name.trim().to_lowercase();
    let normalized = normalized.rsplit('/').next().unwrap_or("");
    let normalized = normalized
        .strip_prefix("claude-")
        .unwrap_or(normalized)
        .replace('.', "-");
    let tokens: Vec<_> = normalized.split('-').collect();
    if tokens[0] == "fable" {
        return true;
    }
    if tokens.len() < 2 || !matches!(tokens[0], "opus" | "sonnet") {
        return false;
    }
    let parse_version = |token: &str| {
        // Eight-digit snapshot dates must never be treated as versions.
        if token.is_empty()
            || token.len() >= 8
            || !token.bytes().all(|digit| digit.is_ascii_digit())
        {
            -1
        } else {
            token.parse::<i32>().unwrap_or(-1)
        }
    };
    let major = parse_version(tokens[1]);
    major >= 5
        || (tokens[0] == "sonnet"
            && major == 4
            && tokens.len() > 2
            && parse_version(tokens[2]) >= 6)
}

// ref: internal/translator/claude/openai/responses/claude_openai-responses_request.go:998-1175 @ 16d98881d4bb37adaa827599e4be8f5154e81646
fn repair_claude_tool_pairing(mut messages: Vec<Value>) -> Vec<Value> {
    use std::collections::HashSet;
    let mut previous_tool_use_ids = HashSet::new();
    let mut output = Vec::with_capacity(messages.len() + 1);
    for i in 0..messages.len() {
        let mut message = messages[i].clone();
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if role == "user" {
            normalize_claude_tool_result_message(&mut message, &previous_tool_use_ids);
        }
        output.push(message);
        previous_tool_use_ids.clear();
        if role != "assistant" {
            continue;
        }
        let tool_use_ids: Vec<String> = output
            .last()
            .unwrap()
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("tool_use"))
            .filter_map(|part| part.get("id").and_then(Value::as_str))
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect();
        previous_tool_use_ids.extend(tool_use_ids.iter().cloned());
        if tool_use_ids.is_empty() {
            continue;
        }
        let has_next_user = messages
            .get(i + 1)
            .and_then(|message| message.get("role"))
            .and_then(Value::as_str)
            == Some("user");
        let answered: HashSet<&str> = messages
            .get(i + 1)
            .filter(|_| has_next_user)
            .and_then(|message| message.get("content"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("tool_result"))
            .filter_map(|part| part.get("tool_use_id").and_then(Value::as_str))
            .collect();
        let mut synthesized: Vec<Value> = tool_use_ids
            .iter()
            .filter(|id| !answered.contains(id.as_str()))
            .map(|id| {
                json!({"type":"tool_result", "tool_use_id":id, "is_error":true,
                "content":"Tool call was interrupted before any output was recorded."})
            })
            .collect();
        if synthesized.is_empty() {
            continue;
        }
        if has_next_user {
            match messages[i + 1].get("content") {
                Some(Value::Array(parts)) => synthesized.extend(parts.iter().cloned()),
                Some(Value::String(text)) => synthesized.push(json!({"type":"text", "text":text})),
                _ => {}
            }
            messages[i + 1] = json!({"role":"user", "content":synthesized});
        } else {
            output.push(json!({"role":"user", "content":synthesized}));
        }
    }
    output
}

fn normalize_claude_tool_result_message(
    message: &mut Value,
    answered_ids: &std::collections::HashSet<String>,
) {
    let Some(parts) = message.get("content").and_then(Value::as_array) else {
        return;
    };
    let mut results = Vec::new();
    let mut others = Vec::new();
    let mut seen_other = false;
    let mut changed = false;
    for part in parts {
        if part.get("type").and_then(Value::as_str) == Some("tool_result") {
            if !answered_ids.contains(
                part.get("tool_use_id")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            ) {
                changed = true;
                seen_other = true;
                let text_parts = claude_tool_result_text_parts(part);
                if text_parts.is_empty() {
                    others.push(json!({"type":"text", "text":"Tool result was empty."}));
                } else {
                    others.extend(text_parts);
                }
            } else {
                results.push(part.clone());
                changed |= seen_other;
            }
        } else {
            seen_other = true;
            others.push(part.clone());
        }
    }
    if changed {
        results.extend(others);
        *message = json!({"role":"user", "content":results});
    }
}

fn claude_tool_result_text_parts(block: &Value) -> Vec<Value> {
    let Some(content) = block.get("content") else {
        return Vec::new();
    };
    if let Value::Array(parts) = content {
        return parts
            .iter()
            .cloned()
            .filter_map(|mut part| {
                if part
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .is_empty()
                {
                    if let Some(object) = part.as_object_mut() {
                        object.insert("type".to_owned(), Value::String("text".to_owned()));
                    }
                }
                let visible = part.get("type").and_then(Value::as_str) != Some("text")
                    || part
                        .get("text")
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.trim().is_empty());
                visible.then_some(part)
            })
            .collect();
    }
    let text = match content {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    if text.trim().is_empty() {
        Vec::new()
    } else {
        vec![json!({"type":"text", "text":text})]
    }
}

fn is_system_level_role(role: Option<&str>) -> bool {
    role.is_some_and(|role| {
        matches!(
            role.trim().to_ascii_lowercase().as_str(),
            "system" | "developer"
        )
    })
}

fn append_system_text(output: &mut Vec<Value>, text: &str, cache_control: Option<&Value>) {
    if text.is_empty() {
        return;
    }
    let mut block = json!({"type":"text", "text":text});
    if let Some(cache) = cache_control.filter(|value| value.is_object()) {
        block["cache_control"] = cache.clone();
    }
    output.push(block);
}

fn apply_reasoning_effort(
    output: &mut Map<String, Value>,
    effort: Option<&Value>,
    model_name: &str,
) {
    let Some(effort) = effort
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
    else {
        return;
    };
    let capability = lookup_model_info(model_name, "claude").and_then(|model| model.thinking);
    if let Some(capability) = capability.filter(|thinking| !thinking.levels.is_empty()) {
        match effort.as_str() {
            "none" => {
                output.insert("thinking".into(), json!({"type":"disabled"}));
            }
            "auto" => {
                output.insert("thinking".into(), json!({"type":"adaptive"}));
            }
            _ => {
                let supports_max = has_level(capability.levels, "max");
                let mapped = map_to_claude_effort(&effort, supports_max);
                if let Some(mapped) = mapped {
                    output.insert("thinking".into(), json!({"type":"adaptive"}));
                    output.insert("output_config".into(), json!({"effort":mapped}));
                }
            }
        }
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

// ref: internal/translator/claude/openai/responses/claude_openai-responses_request.go:766-797
fn convert_reasoning(item: &Value, preserve_empty_thinking_blocks: bool) -> Option<Value> {
    let encrypted = item
        .get("encrypted_content")
        .map(|value| match value {
            Value::Null => String::new(),
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    if let Some(data) = encrypted
        .trim()
        .strip_prefix(CLAUDE_RESPONSES_REDACTED_THINKING_PREFIX)
        .map(str::trim)
    {
        return (!data.is_empty()).then(|| json!({"type":"redacted_thinking", "data":data}));
    }
    let signature = compatible_signature_for_provider(SignatureProvider::Claude, &encrypted)
        .or_else(|| preserve_empty_thinking_blocks.then_some(encrypted.clone()))?;
    let thinking = reasoning_parts_text(item.get("summary"))
        .filter(|text| !text.is_empty())
        .or_else(|| reasoning_parts_text(item.get("content")))
        .unwrap_or_default();
    Some(json!({"type":"thinking", "thinking":thinking, "signature":signature}))
}

fn reasoning_parts_text(parts: Option<&Value>) -> Option<String> {
    Some(
        parts?
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| part.as_str())
            })
            .collect::<String>(),
    )
}

fn prepend_reasoning(message: &mut Value, pending: &mut Vec<Value>) {
    let mut content = std::mem::take(pending);
    match message.get_mut("content") {
        Some(Value::String(text)) => {
            content.push(json!({"type":"text", "text":std::mem::take(text)}));
        }
        Some(Value::Array(parts)) => content.append(parts),
        _ => {}
    }
    message["content"] = Value::Array(content);
}

fn convert_message(item: &Value, role: &str) -> Option<Value> {
    let content = item.get("content")?;
    if let Some(text) = content.as_str() {
        return Some(json!({"role":role, "content":text}));
    }
    let mut parts = Vec::new();
    for part in content.as_array().into_iter().flatten() {
        if let Some(converted) = convert_content_part(part) {
            parts.push(converted);
        }
    }
    if parts.is_empty() {
        return None;
    }
    let has_cache = parts.iter().any(|part| part.get("cache_control").is_some());
    if parts.len() == 1
        && !has_cache
        && parts[0].get("type").and_then(Value::as_str) == Some("text")
    {
        return Some(json!({"role":role, "content":parts[0]["text"]}));
    }
    Some(json!({"role":role, "content":parts}))
}

fn convert_content_part(part: &Value) -> Option<Value> {
    let kind = part.get("type").and_then(Value::as_str)?;
    let mut converted = match kind {
        "input_text" | "output_text" => json!({"type":"text", "text":part.get("text")?.as_str()?}),
        "input_image" => media_part(part, "image_url", "image", false)?,
        "input_file" => media_part(part, "file_data", "document", true)?,
        _ => return None,
    };
    if let Some(cache) = part.get("cache_control").filter(|v| v.is_object()) {
        converted
            .as_object_mut()?
            .insert("cache_control".into(), cache.clone());
    }
    Some(converted)
}

fn media_part(part: &Value, field: &str, target_type: &str, file: bool) -> Option<Value> {
    let raw = part.get(field).or_else(|| part.get("url"))?.as_str()?;
    if !file && !raw.starts_with("data:") {
        return Some(json!({"type":target_type, "source":{"type":"url", "url":raw}}));
    }
    let (media_type, data) = raw
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(";base64,"))
        .map_or(("application/octet-stream", raw), |(kind, data)| {
            (kind, data)
        });
    if data.is_empty() {
        return None;
    }
    Some(
        json!({"type":target_type, "source":{"type":"base64", "media_type":media_type, "data":data}}),
    )
}

fn convert_tool_result_content(output: Option<&Value>) -> Value {
    let Some(output) = output else {
        return Value::String(String::new());
    };
    if let Some(parts) = output.as_array() {
        let converted: Vec<Value> = parts.iter().filter_map(convert_content_part).collect();
        if converted.len() == 1 && converted[0].get("type").and_then(Value::as_str) == Some("text")
        {
            return converted[0]
                .get("text")
                .cloned()
                .unwrap_or(Value::String(String::new()));
        }
        if !converted.is_empty() {
            return Value::Array(converted);
        }
    }
    output.as_str().map_or_else(
        || Value::String(output.to_string()),
        |v| Value::String(v.into()),
    )
}

fn convert_tool(tool: &Value, output: &mut Vec<Value>, names: &mut Vec<String>) {
    let kind = tool
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("function");
    if kind == "custom" && tool.get("name").and_then(Value::as_str) == Some("apply_patch") {
        return;
    }
    if matches!(
        kind,
        "image_generation" | "file_search" | "code_interpreter" | "computer_use_preview"
    ) {
        return;
    }
    if kind == "namespace" {
        let namespace = tool.get("name").and_then(Value::as_str).unwrap_or("");
        for child in tool
            .get("tools")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let child_name = tool_name(child);
            let qualified = qualify_tool_name(namespace, child_name);
            if let Some(value) = function_tool(child, &qualified) {
                names.push(qualified);
                output.push(value);
            }
        }
        return;
    }
    if kind == "web_search" {
        if tool.get("external_web_access").and_then(Value::as_bool) == Some(false) {
            return;
        }
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("web_search");
        let mut value = json!({"type":"web_search_20250305", "name":name});
        for key in ["max_uses", "user_location"] {
            if let Some(field) = tool.get(key) {
                value[key] = field.clone();
            }
        }
        if let Some(domains) = tool.pointer("/filters/allowed_domains") {
            value["allowed_domains"] = domains.clone();
        }
        names.push(name.into());
        output.push(value);
        return;
    }
    let name = tool_name(tool);
    if let Some(value) = function_tool(tool, name) {
        names.push(name.into());
        output.push(value);
    }
}

fn function_tool(tool: &Value, name: &str) -> Option<Value> {
    if name.trim().is_empty() {
        return None;
    }
    let mut value = json!({"name":name, "description":tool_description(tool), "input_schema":normalize_schema(tool_parameters(tool))});
    if let Some(cache) = tool
        .get("cache_control")
        .or_else(|| tool.get("function").and_then(|v| v.get("cache_control")))
    {
        value["cache_control"] = cache.clone();
    }
    Some(value)
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
    for union in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = object.remove(union).and_then(|v| v.as_array().cloned()) {
            for branch in branches {
                if let Some(branch_props) = branch.get("properties").and_then(Value::as_object) {
                    for (name, value) in branch_props {
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

fn convert_tool_choice(choice: Option<&Value>, names: &[String]) -> Option<Value> {
    match choice? {
        Value::String(value) if value == "auto" => Some(json!({"type":"auto"})),
        Value::String(value) if value == "required" && !names.is_empty() => {
            Some(json!({"type":"any"}))
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("function") => {
            let name = object
                .get("name")
                .or_else(|| object.get("function").and_then(|v| v.get("name")))?
                .as_str()?;
            names
                .iter()
                .find(|candidate| {
                    candidate.as_str() == name || candidate.ends_with(&format!("__{name}"))
                })
                .map(|mapped| json!({"type":"tool", "name":mapped}))
        }
        _ => None,
    }
}

fn tool_name(tool: &Value) -> &str {
    tool.get("name")
        .or_else(|| tool.pointer("/function/name"))
        .and_then(Value::as_str)
        .unwrap_or("")
}
fn tool_description(tool: &Value) -> &str {
    tool.get("description")
        .or_else(|| tool.pointer("/function/description"))
        .and_then(Value::as_str)
        .unwrap_or("")
}
fn tool_parameters(tool: &Value) -> Option<&Value> {
    ["parameters", "parametersJsonSchema", "input_schema"]
        .iter()
        .find_map(|key| tool.get(*key))
        .or_else(|| tool.pointer("/function/parameters"))
}
fn qualify_tool_name(namespace: &str, child: &str) -> String {
    if namespace.is_empty() || child.starts_with("mcp__") || child.starts_with(namespace) {
        child.into()
    } else if namespace.ends_with("__") {
        format!("{namespace}{child}")
    } else {
        format!("{namespace}__{child}")
    }
}
fn normalized_role(role: Option<&str>) -> &str {
    match role {
        Some("assistant") => "assistant",
        _ => "user",
    }
}
fn sanitize_tool_id(id: &str) -> String {
    let value: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if value.is_empty() {
        "toolu_ctox".into()
    } else {
        value
    }
}
fn flush_tools(messages: &mut Vec<Value>, pending: &mut Vec<(String, Value)>) {
    messages.extend(pending.drain(..).map(|(_, value)| value));
}

fn flush_reasoning(messages: &mut Vec<Value>, pending: &mut Vec<Value>) {
    if !pending.is_empty() {
        messages.push(json!({"role":"assistant", "content":std::mem::take(pending)}));
    }
}
fn flush_tool_for(messages: &mut Vec<Value>, pending: &mut Vec<(String, Value)>, id: &str) {
    if let Some(index) = pending.iter().position(|(candidate, _)| candidate == id) {
        messages.push(pending.remove(index).1);
    } else {
        flush_tools(messages, pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose, Engine as _};

    fn append_bytes(target: &mut Vec<u8>, field: u8, value: &[u8]) {
        target.push((field << 3) | 2);
        target.push(value.len() as u8);
        target.extend_from_slice(value);
    }

    fn claude_signature() -> String {
        let mut channel = vec![0x08, 0x0c, 0x10, 0x02];
        append_bytes(&mut channel, 6, b"claude-sonnet-4-6");
        let mut container = Vec::new();
        append_bytes(&mut container, 1, &channel);
        let mut payload = Vec::new();
        append_bytes(&mut payload, 2, &container);
        payload.extend_from_slice(&[0x18, 0x01]);
        general_purpose::STANDARD.encode(payload)
    }

    fn gpt_signature() -> String {
        let mut payload = vec![0_u8; 1 + 8 + 16 + 16 + 32];
        payload[0] = 0x80;
        payload[8] = 1;
        for (index, byte) in payload.iter_mut().enumerate().skip(9) {
            *byte = index as u8;
        }
        general_purpose::URL_SAFE.encode(payload)
    }

    #[test]
    fn merges_valid_reasoning_into_following_assistant_message() {
        let input = json!({"input":[
            {"type":"reasoning","encrypted_content":claude_signature(),"summary":[{"type":"summary_text","text":"internal"}]},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"visible"}]}
        ]});
        let output = convert_openai_responses_request_to_claude(
            "claude-test",
            &serde_json::to_vec(&input).unwrap(),
            false,
        );
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["messages"][0]["content"][0]["type"], "thinking");
        assert_eq!(value["messages"][0]["content"][0]["thinking"], "internal");
        assert_eq!(value["messages"][0]["content"][1]["text"], "visible");
    }

    #[test]
    fn drops_gpt_reasoning_before_claude_request() {
        let input = json!({"input":[
            {"type":"reasoning","encrypted_content":gpt_signature(),"summary":[{"type":"summary_text","text":"drop"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"continue"}]}
        ]});
        let output = convert_openai_responses_request_to_claude(
            "claude-test",
            &serde_json::to_vec(&input).unwrap(),
            false,
        );
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["messages"][0]["role"], "user");
        assert_eq!(value["messages"][0]["content"], "continue");
    }

    // ref: internal/translator/claude/openai/responses/claude_openai-responses_request_test.go @ 16d98881d4bb37adaa827599e4be8f5154e81646
    fn history_convert(model: &str, input: &Value, stream: bool, compat: bool) -> Value {
        serde_json::from_slice(&convert_openai_responses_request_to_claude_impl(
            model,
            &serde_json::to_vec(input).unwrap(),
            stream,
            compat,
        ))
        .unwrap()
    }

    #[test]
    fn candidate_claude_responses_history_rejects_current_model_prefill() {
        let input = json!({"input":[
            {"type":"message","role":"user","content":"prompt"},
            {"type":"message","role":"assistant","content":"prefill"}
        ]});
        for model in [
            "claude-fable-5",
            "claude-opus-5",
            "claude-sonnet-4-6",
            "fable",
            "opus-5",
            "sonnet-4.6",
            "anthropic/claude-opus-5-thinking",
            "claude-sonnet-4.6",
            "claude-sonnet-4-7",
            "claude-sonnet-4-10",
            "claude-sonnet-5",
            "claude-opus-6",
            "claude-opus-5.1",
            "claude-sonnet-4-6-20260217",
            " ANTHROPIC/CLAUDE-OPUS-5-THINKING ",
        ] {
            for stream in [false, true] {
                let output = history_convert(model, &input, stream, false);
                assert_eq!(
                    output["messages"],
                    json!([{"role":"user","content":"prompt"}]),
                    "{model}"
                );
                assert_eq!(output["model"], model);
            }
        }
    }

    #[test]
    fn candidate_claude_responses_history_preserves_supported_prefill() {
        let input = json!({"input":[{"role":"user","content":"prompt"},
            {"role":"assistant","content":"prefill"}]});
        for model in [
            "claude-sonnet-4-5",
            "claude-haiku-4-5",
            "claude-3-opus-20240229",
            "claude-opus-20240229",
            "claude-sonnet-4-20260217",
            "claude-sonnet-4.5",
            "not-a-fable-model",
            "my-custom-opus-5-wrapper",
            "my-sonnet-4-6-wrapper",
            "claude-fabled-5",
            "claude-opus-5foo",
            "claude-sonnet-4-6foo",
            "fable/gpt-4o",
            "opus",
            "sonnet",
            "",
        ] {
            for stream in [false, true] {
                let output = history_convert(model, &input, stream, false);
                assert_eq!(output["messages"].as_array().unwrap().len(), 2, "{model}");
                assert_eq!(output["messages"][1]["content"], "prefill");
            }
        }
    }

    #[test]
    fn candidate_claude_responses_history_compat_keeps_prefill_and_thinking() {
        for stream in [false, true] {
            let prefill = history_convert(
                "claude-opus-5",
                &json!({"input":[
                    {"role":"user","content":"prompt"}, {"role":"assistant","content":"prefill"}
                ]}),
                stream,
                true,
            );
            assert_eq!(prefill["messages"][1]["content"], "prefill");
            let thinking = history_convert(
                "claude-opus-5",
                &json!({"input":[
                    {"role":"user","content":"prompt"},
                    {"type":"reasoning","encrypted_content":"opaque-signature","summary":[]}
                ]}),
                stream,
                true,
            );
            assert_eq!(thinking["messages"][1]["content"][0]["type"], "thinking");
        }
    }

    #[test]
    fn candidate_claude_responses_history_strips_only_terminal_thinking() {
        let output = history_convert(
            "claude-sonnet-4-5",
            &json!({"input":[
                {"type":"reasoning","encrypted_content":claude_signature(),"summary":[{"text":"earlier"}]},
                {"role":"assistant","content":"answer"}, {"role":"user","content":"continue"},
                {"type":"reasoning","encrypted_content":claude_signature(),"summary":[{"text":"unfinished"}]}
            ]}),
            false,
            false,
        );
        assert_eq!(output["messages"].as_array().unwrap().len(), 2);
        assert_eq!(output["messages"][0]["content"][0]["thinking"], "earlier");
        assert_eq!(output["messages"][0]["content"][1]["text"], "answer");
        assert_eq!(output["messages"][1]["content"], "continue");
    }

    #[test]
    fn candidate_claude_responses_history_thinking_strip_preserves_metadata() {
        for protected in [
            json!({"cache_control":{"type":"ephemeral"}}),
            json!({"citations":[]}),
        ] {
            let mut text = json!({"type":"text","text":"visible"});
            text.as_object_mut()
                .unwrap()
                .extend(protected.as_object().unwrap().clone());
            let mut messages = vec![json!({"role":"assistant","content":[text.clone(),
                {"type":"thinking","thinking":"unfinished"}]})];
            strip_trailing_claude_thinking_blocks(&mut messages);
            assert_eq!(messages[0]["content"], json!([text]));
        }
        let mut messages = vec![json!({"role":"assistant","content":[
            {"type":"text","text":"visible"}, {"type":"redacted_thinking","data":"unfinished"}]})];
        strip_trailing_claude_thinking_blocks(&mut messages);
        assert_eq!(messages[0]["content"], "visible");
    }

    #[test]
    fn candidate_claude_responses_history_answers_interrupted_tools_before_prefill() {
        let input = json!({"input":[{"role":"user","content":"prompt"},
            {"type":"function_call","call_id":"call_1","name":"Read","arguments":"{\"path\":\"file\"}"}]});
        for model in ["claude-opus-5", "claude-sonnet-4-5"] {
            for stream in [false, true] {
                for compat in [false, true] {
                    let output = history_convert(model, &input, stream, compat);
                    assert_eq!(output["messages"].as_array().unwrap().len(), 3);
                    assert_eq!(output["messages"][1]["content"][0]["id"], "call_1");
                    assert_eq!(output["messages"][1]["content"][0]["input"]["path"], "file");
                    let result = &output["messages"][2]["content"][0];
                    assert_eq!(result["tool_use_id"], "call_1");
                    assert_eq!(result["is_error"], true);
                    assert_eq!(
                        result["content"],
                        "Tool call was interrupted before any output was recorded."
                    );
                }
            }
        }
    }

    #[test]
    fn candidate_claude_responses_history_orders_results_before_user_content() {
        let messages = vec![
            json!({"role":"assistant","content":[
            {"type":"tool_use","id":"one"}, {"type":"tool_use","id":"two"}]}),
            json!({"role":"user","content":[{"type":"text","text":"continue"},
                {"type":"tool_result","tool_use_id":"two","content":"actual"},
                {"type":"image","source":{"type":"url","url":"https://example.invalid/image"}}]}),
        ];
        let output = repair_claude_tool_pairing(messages);
        assert_eq!(output.len(), 2);
        assert_eq!(output[1]["content"][0]["tool_use_id"], "one");
        assert_eq!(output[1]["content"][0]["is_error"], true);
        assert_eq!(output[1]["content"][1]["tool_use_id"], "two");
        assert_eq!(output[1]["content"][1]["content"], "actual");
        assert_eq!(output[1]["content"][2]["text"], "continue");
        assert_eq!(output[1]["content"][3]["type"], "image");
    }

    #[test]
    fn candidate_claude_responses_history_folds_orphans_and_marks_empty_results() {
        for (text, expected) in [
            ("late result", "late result"),
            ("", "Tool result was empty."),
        ] {
            let output = history_convert(
                "claude-opus-5",
                &json!({"input":[
                    {"type":"function_call_output","call_id":"missing","output":text}
                ]}),
                false,
                false,
            );
            assert_eq!(
                output["messages"],
                json!([{"role":"user","content":[{"type":"text","text":expected}]}])
            );
        }
        let output = repair_claude_tool_pairing(vec![json!({"role":"user","content":[
            {"type":"tool_result","tool_use_id":"missing","content":[
                {"text":" "}, {"text":"visible"},
                {"type":"image","source":{"type":"url","url":"https://example.invalid/image"}}]}]})]);
        assert_eq!(output[0]["content"].as_array().unwrap().len(), 2);
        assert_eq!(
            output[0]["content"][0],
            json!({"type":"text","text":"visible"})
        );
        assert_eq!(output[0]["content"][1]["type"], "image");
    }

    #[test]
    fn candidate_claude_responses_history_retains_minimal_turn_after_removal() {
        let output = history_convert(
            "claude-opus-5",
            &json!({"input":[
            {"role":"assistant","content":"prefill"}]}),
            false,
            false,
        );
        assert_eq!(
            output["messages"],
            json!([{"role":"user","content":[{"type":"text","text":""}]}])
        );
        let empty = history_convert("claude-opus-5", &json!({}), false, false);
        assert_eq!(empty["messages"], json!([]));
    }
}
