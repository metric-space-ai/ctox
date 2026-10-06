// ref: sdk/api/handlers/openai/openai_handlers.go:135-438 @ a4acc9f752bd46571f737a10c04bf413656ab06b
// Port-Status: candidate — public OpenAI conversions
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

#[must_use]
pub fn should_treat_as_responses_format(raw_json: &[u8]) -> bool {
    let Ok(Value::Object(document)) = serde_json::from_slice(raw_json) else {
        return false;
    };
    !document.contains_key("messages")
        && (document.contains_key("input") || document.contains_key("instructions"))
}

#[must_use]
pub fn convert_completions_request_to_chat_completions(raw_json: &[u8]) -> Vec<u8> {
    let Ok(Value::Object(mut document)) = serde_json::from_slice(raw_json) else {
        return raw_json.to_vec();
    };
    let prompt = document
        .remove("prompt")
        .map(|value| compat_string(&value))
        .unwrap_or_default();
    document.insert(
        "messages".to_owned(),
        json!([{
            "role":"user","content":if prompt.is_empty() { "Complete this:" } else { &prompt }
        }]),
    );
    serde_json::to_vec(&document).unwrap_or_else(|_| raw_json.to_vec())
}

#[must_use]
pub fn convert_chat_completions_response_to_completions(raw_json: &[u8]) -> Vec<u8> {
    let Ok(root) = serde_json::from_slice::<Value>(raw_json) else {
        return raw_json.to_vec();
    };
    let mut output = completions_base(&root);
    if let Some(choices) = root.get("choices").and_then(Value::as_array) {
        output["choices"] = Value::Array(
            choices
                .iter()
                .map(|choice| {
                    let mut converted =
                        json!({"index":choice.get("index").cloned().unwrap_or(json!(0))});
                    if let Some(message) = choice.get("message").or_else(|| choice.get("delta")) {
                        if let Some(content) = message.get("content") {
                            converted["text"] = Value::String(compat_string(content));
                        }
                    }
                    if let Some(reason) = choice.get("finish_reason") {
                        converted["finish_reason"] = Value::String(compat_string(reason));
                    }
                    if let Some(logprobs) = choice.get("logprobs") {
                        converted["logprobs"] = logprobs.clone();
                    }
                    converted
                })
                .collect(),
        );
    }
    serde_json::to_vec(&output).unwrap_or_else(|_| raw_json.to_vec())
}

// ref: sdk/api/handlers/openai/openai_handlers.go:339-438 @ a4acc9f7
#[must_use]
pub fn convert_chat_completions_stream_chunk_to_completions(raw_json: &[u8]) -> Option<Vec<u8>> {
    let root = serde_json::from_slice::<Value>(raw_json).ok()?;
    let choices = root.get("choices").and_then(Value::as_array);
    let has_content = choices.is_some_and(|choices| {
        choices.iter().any(|choice| {
            choice
                .pointer("/delta/content")
                .is_some_and(|value| !compat_string(value).is_empty())
                || choice.get("finish_reason").is_some_and(|value| {
                    let reason = compat_string(value);
                    !reason.is_empty() && reason != "null"
                })
        })
    });
    if !has_content && root.get("usage").is_none() {
        return None;
    }
    let mut output = completions_base(&root);
    if let Some(choices) = choices {
        output["choices"] = Value::Array(choices.iter().map(|choice| {
            let mut converted = json!({
                "index":choice.get("index").cloned().unwrap_or(json!(0)),
                "text":choice.pointer("/delta/content").map(compat_string).unwrap_or_default()
            });
            if let Some(reason) = choice.get("finish_reason") {
                let reason = compat_string(reason);
                if reason != "null" {
                    converted["finish_reason"] = Value::String(reason);
                }
            }
            if let Some(logprobs) = choice.get("logprobs") {
                converted["logprobs"] = logprobs.clone();
            }
            converted
        }).collect());
    }
    serde_json::to_vec(&output).ok()
}

fn completions_base(root: &Value) -> Value {
    let mut output = json!({
        "id":root.get("id").cloned().unwrap_or(json!("")),
        "object":"text_completion",
        "created":root.get("created").cloned().unwrap_or(json!(0)),
        "model":root.get("model").cloned().unwrap_or(json!("")),
        "choices":[]
    });
    if let Some(usage) = root.get("usage") {
        output["usage"] = usage.clone();
    }
    output
}

fn compat_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        value => value.to_string(),
    }
}
