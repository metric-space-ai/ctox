// ref: internal/translator/interactions/claude/interactions_claude_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Map, Value};

pub fn convert_claude_request_to_interactions(
    model_name: &str,
    input_raw_json: &[u8],
    stream: bool,
) -> Vec<u8> {
    convert_claude_request_to_interactions_impl(model_name, input_raw_json, stream, false)
}

/// ref: internal/translator/interactions/claude/interactions_claude_request.go:15-21
/// Candidate e2bff010: compatibility endpoints preserve empty thought steps.
pub fn convert_claude_request_to_interactions_with_compat(
    model_name: &str,
    input_raw_json: &[u8],
    stream: bool,
) -> Vec<u8> {
    convert_claude_request_to_interactions_impl(model_name, input_raw_json, stream, true)
}

fn convert_claude_request_to_interactions_impl(
    model_name: &str,
    input_raw_json: &[u8],
    stream: bool,
    preserve_empty_thinking_blocks: bool,
) -> Vec<u8> {
    let root = serde_json::from_slice::<Value>(input_raw_json).unwrap_or(Value::Null);
    let mut out = Map::new();
    out.insert(
        "model".into(),
        Value::String(first_nonempty(&[
            model_name,
            root.get("model")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ])),
    );
    out.insert("input".into(), Value::Array(Vec::new()));
    if let Some(stream_value) = root
        .get("stream")
        .and_then(Value::as_bool)
        .or(stream.then_some(true))
    {
        out.insert("stream".into(), Value::Bool(stream_value));
    }
    if let Some(system) = root.get("system").and_then(claude_text) {
        if !system.is_empty() {
            out.insert("system_instruction".into(), Value::String(system));
        }
    }
    copy_generation_config(&root, &mut out);
    if let Some(messages) = root.get("messages").and_then(Value::as_array) {
        out.insert(
            "input".into(),
            Value::Array(
                messages
                    .iter()
                    .flat_map(|message| convert_message(message, preserve_empty_thinking_blocks))
                    .collect(),
            ),
        );
    }
    copy_tools(&root, &mut out);
    serde_json::to_vec(&Value::Object(out)).unwrap_or_default()
}

fn copy_generation_config(root: &Value, out: &mut Map<String, Value>) {
    let mut config = Map::new();
    for (source, target) in [
        ("max_tokens", "max_output_tokens"),
        ("temperature", "temperature"),
        ("top_p", "top_p"),
        ("stop_sequences", "stop_sequences"),
    ] {
        if let Some(value) = root.get(source) {
            config.insert(target.into(), value.clone());
        }
    }
    if let Some(thinking) = root.get("thinking") {
        match thinking
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "disabled" => {
                config.insert("thinking_level".into(), Value::String("none".into()));
            }
            "enabled" => {
                if let Some(budget) = thinking.get("budget_tokens") {
                    config.insert("thinking_config".into(), json!({"thinking_budget":budget}));
                } else {
                    config.insert("thinking_level".into(), Value::String("high".into()));
                }
            }
            "adaptive" => {
                config.insert("thinking_level".into(), Value::String("auto".into()));
            }
            _ => {}
        }
    }
    if let Some(effort) = root
        .pointer("/output_config/effort")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        config.insert(
            "thinking_level".into(),
            Value::String(effort.to_ascii_lowercase()),
        );
    }
    if let Some(choice) = root.get("tool_choice").and_then(convert_tool_choice) {
        config.insert("tool_choice".into(), choice);
    }
    if !config.is_empty() {
        out.insert("generation_config".into(), Value::Object(config));
    }
}

fn convert_tool_choice(choice: &Value) -> Option<Value> {
    if let Some(kind) = choice.as_str() {
        return match kind.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Value::String("auto".into())),
            "any" | "required" => Some(Value::String("required".into())),
            _ => None,
        };
    }
    let kind = choice
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match kind.as_str() {
        "auto" => Some(Value::String("auto".into())),
        "any" | "required" => Some(Value::String("required".into())),
        "tool" => choice
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| json!({"type":"function","name":name})),
        _ => None,
    }
}

fn convert_message(message: &Value, preserve_empty_thinking_blocks: bool) -> Vec<Value> {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let default_step_type = if role == "assistant" {
        "model_output"
    } else {
        "user_input"
    };
    let Some(content) = message.get("content") else {
        return Vec::new();
    };
    if let Some(text) = content.as_str() {
        return vec![json!({"type":default_step_type,"content":[{"type":"text","text":text}]})];
    }
    let Some(parts) = content.as_array() else {
        return Vec::new();
    };
    let mut items = Vec::new();
    let mut step_content = Vec::new();
    let flush = |items: &mut Vec<Value>, step_content: &mut Vec<Value>| {
        if !step_content.is_empty() {
            items.push(json!({
                "type":default_step_type,
                "content":std::mem::take(step_content),
            }));
        }
    };
    for part in parts {
        match part
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "text" => {
                if let Some(text) = part
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                {
                    step_content.push(json!({"type":"text","text":text}));
                }
            }
            "thinking" => {
                flush(&mut items, &mut step_content);
                let text = part
                    .get("thinking")
                    .map(|value| match value {
                        Value::Null => String::new(),
                        Value::String(text) => text.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();
                if !text.is_empty() || preserve_empty_thinking_blocks {
                    items.push(json!({"type":"thought","content":[{"type":"text","text":text}]}));
                }
            }
            kind @ ("image" | "document") => {
                if let Some(media) = media_part(part, kind) {
                    step_content.push(media);
                }
            }
            "tool_use" => {
                flush(&mut items, &mut step_content);
                items.push(tool_use(part));
            }
            "tool_result" => {
                flush(&mut items, &mut step_content);
                items.push(tool_result(part));
            }
            _ => {}
        }
    }
    flush(&mut items, &mut step_content);
    items
}

fn media_part(part: &Value, kind: &str) -> Option<Value> {
    let source = part.get("source")?;
    let mime = source.get("media_type")?.as_str()?;
    let data = source.get("data")?.as_str()?;
    (!mime.is_empty() && !data.is_empty())
        .then(|| json!({"type":kind,"mime_type":mime,"data":data}))
}

fn tool_use(part: &Value) -> Value {
    let mut step = json!({
        "type":"function_call",
        "name":part.get("name").and_then(Value::as_str).unwrap_or_default(),
        "arguments":part.get("input").filter(|value| value.is_object()).cloned().unwrap_or_else(|| json!({})),
    });
    if let Some(id) = part
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        step["id"] = Value::String(id.into());
        step["call_id"] = Value::String(id.into());
    }
    step
}

fn tool_result(part: &Value) -> Value {
    let mut step = json!({"type":"function_result","call_id":"","result":""});
    if let Some(id) = part
        .get("tool_use_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        step["id"] = Value::String(id.into());
        step["call_id"] = Value::String(id.into());
    }
    if let Some(result) = part.get("content") {
        step["result"] = match result {
            Value::String(text) => Value::String(text.clone()),
            Value::Array(parts) => Value::Array(
                parts
                    .iter()
                    .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
                    .map(|part| {
                        json!({"type":"text","text":part.get("text").and_then(Value::as_str).unwrap_or_default()})
                    })
                    .collect(),
            ),
            value => value.clone(),
        };
    }
    step
}

fn copy_tools(root: &Value, out: &mut Map<String, Value>) {
    let tools = root
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            let name = tool.get("name")?.as_str()?.trim();
            if name.is_empty() {
                return None;
            }
            let mut item = Map::new();
            item.insert("type".into(), Value::String("function".into()));
            item.insert("name".into(), Value::String(name.into()));
            if let Some(description) = tool.get("description") {
                item.insert(
                    "description".into(),
                    Value::String(description.as_str().unwrap_or_default().into()),
                );
            }
            item.insert(
                "parameters".into(),
                tool.get("input_schema")
                    .filter(|value| value.is_object())
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            );
            Some(Value::Object(item))
        })
        .collect::<Vec<_>>();
    if !tools.is_empty() {
        out.insert("tools".into(), Value::Array(tools));
    }
}

fn claude_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Object(object) => object
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Value::Array(items) => Some(
            items
                .iter()
                .filter_map(claude_text)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

fn first_nonempty(values: &[&str]) -> String {
    values
        .iter()
        .find(|value| !value.trim().is_empty())
        .copied()
        .unwrap_or_default()
        .to_owned()
}
