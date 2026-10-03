// ref: internal/translator/codex/openai/responses/codex_openai-responses_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Map, Value};

pub fn convert_openai_responses_request_to_codex(
    _model_name: &str,
    input_raw_json: &[u8],
    _stream: bool,
) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(input_raw_json) else {
        return input_raw_json.to_vec();
    };
    let Some(object) = root.as_object_mut() else {
        return input_raw_json.to_vec();
    };
    let mut changed = false;

    if let Some(input) = object.get("input").and_then(Value::as_str) {
        object.insert(
            "input".to_owned(),
            json!([{"type":"message","role":"user","content":[{"type":"input_text","text":input}]}]),
        );
        changed = true;
    }

    changed |= set_required_bool(object, "stream", true);
    changed |= set_required_bool(object, "store", false);
    changed |= set_required_bool(object, "parallel_tool_calls", true);

    // ref: internal/translator/codex/openai/responses/codex_openai-responses_request.go:118-150 @ 2044a01f
    let include_sources = object.get("include").and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some("web_search_call.action.sources")));
    let required_include = if include_sources {
        json!(["reasoning.encrypted_content", "web_search_call.action.sources"])
    } else {
        json!(["reasoning.encrypted_content"])
    };
    if object.get("include") != Some(&required_include) {
        object.insert("include".to_owned(), required_include);
        changed = true;
    }

    for key in [
        "max_output_tokens",
        "max_completion_tokens",
        "temperature",
        "top_p",
        "truncation",
        "context_management",
        "user",
    ] {
        changed |= object.remove(key).is_some();
    }
    if object
        .get("service_tier")
        .is_some_and(|tier| tier.as_str() != Some("priority"))
    {
        object.remove("service_tier");
        changed = true;
    }

    if let Some(items) = object.get_mut("input").and_then(Value::as_array_mut) {
        for item in items {
            let Some(item) = item.as_object_mut() else {
                continue;
            };
            if item.get("role").and_then(Value::as_str) == Some("system") {
                item.insert("role".to_owned(), Value::String("developer".to_owned()));
                changed = true;
            }
        }
    }

    changed |= normalize_builtin_tool_array(object.get_mut("tools"));
    if let Some(tool_choice) = object.get_mut("tool_choice").and_then(Value::as_object_mut) {
        changed |= normalize_builtin_tool_type(tool_choice.get_mut("type"));
        changed |= normalize_builtin_tool_array(tool_choice.get_mut("tools"));
    }

    if !changed {
        return input_raw_json.to_vec();
    }
    serde_json::to_vec(&root).unwrap_or_else(|_| input_raw_json.to_vec())
}

fn set_required_bool(object: &mut Map<String, Value>, key: &str, required: bool) -> bool {
    if object.get(key) == Some(&Value::Bool(required)) {
        return false;
    }
    object.insert(key.to_owned(), Value::Bool(required));
    true
}

fn normalize_builtin_tool_array(value: Option<&mut Value>) -> bool {
    let Some(tools) = value.and_then(Value::as_array_mut) else {
        return false;
    };
    tools
        .iter_mut()
        .filter_map(Value::as_object_mut)
        .fold(false, |changed, tool| {
            normalize_builtin_tool_type(tool.get_mut("type")) || changed
        })
}

fn normalize_builtin_tool_type(value: Option<&mut Value>) -> bool {
    let Some(value) = value else {
        return false;
    };
    if matches!(
        value.as_str(),
        Some("web_search_preview" | "web_search_preview_2025_03_11")
    ) {
        *value = Value::String("web_search".to_owned());
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::convert_openai_responses_request_to_codex;

    #[test]
    fn normalizes_codex_required_fields_roles_and_tools() {
        let output = convert_openai_responses_request_to_codex(
            "gpt-5.6",
            br#"{"input":"hello","stream":"true","store":true,"parallel_tool_calls":false,"include":["x"],"max_output_tokens":1,"service_tier":"standard","context_management":[],"user":"u","tools":[{"type":"web_search_preview"}],"tool_choice":{"type":"web_search_preview_2025_03_11","tools":[{"type":"web_search_preview"}]}}"#,
            false,
        );
        let output: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(output["input"][0]["content"][0]["text"], "hello");
        assert_eq!(output["stream"], true);
        assert_eq!(output["store"], false);
        assert_eq!(output["parallel_tool_calls"], true);
        assert_eq!(
            output["include"],
            serde_json::json!(["reasoning.encrypted_content"])
        );
        assert_eq!(output["tools"][0]["type"], "web_search");
        assert_eq!(output["tool_choice"]["type"], "web_search");
        assert!(output.get("max_output_tokens").is_none());
        assert!(output.get("service_tier").is_none());
        assert!(output.get("context_management").is_none());
        assert!(output.get("user").is_none());
    }

    #[test]
    fn preserves_normalized_payload_bytes_and_priority_tier() {
        let input = br#" {"model":"gpt-5.6","stream":true,"store":false,"parallel_tool_calls":true,"include":["reasoning.encrypted_content"],"service_tier":"priority","input":[{"role":"user"}]} "#;
        assert_eq!(
            convert_openai_responses_request_to_codex("gpt-5.6", input, true),
            input
        );
    }
}
