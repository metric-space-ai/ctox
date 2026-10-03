// ref: internal/translator/common/cache_control.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

fn is_valid_cache_control(cache: Option<&Value>) -> bool {
    let Some(cache) = cache else {
        return false;
    };
    if cache.is_null() || !cache.is_object() {
        return false;
    }
    cache.get("type").and_then(Value::as_str) == Some("ephemeral")
}

fn valid_cache_control(cache: Option<&Value>) -> Option<&Value> {
    cache.filter(|value| is_valid_cache_control(Some(value)))
}

pub fn attach_cache_control(dst: &[u8], src: &Value) -> Vec<u8> {
    let Some(cache) = valid_cache_control(src.get("cache_control")) else {
        return dst.to_vec();
    };
    let Ok(mut output) = serde_json::from_slice::<Value>(dst) else {
        return dst.to_vec();
    };
    let Some(object) = output.as_object_mut() else {
        return dst.to_vec();
    };
    object.insert("cache_control".into(), cache.clone());
    serde_json::to_vec(&output).unwrap_or_else(|_| dst.to_vec())
}

pub fn attach_message_cache_control(message: &[u8], src: &Value) -> Vec<u8> {
    let Some(cache) = valid_cache_control(src.get("cache_control")) else {
        return message.to_vec();
    };
    let Ok(mut output) = serde_json::from_slice::<Value>(message) else {
        return message.to_vec();
    };
    let Some(content) = output.get_mut("content") else {
        return message.to_vec();
    };
    match content {
        Value::Array(parts) => {
            let Some(last) = parts.last_mut() else {
                return message.to_vec();
            };
            if last.get("cache_control").is_some() || !last.is_object() {
                return message.to_vec();
            }
            last["cache_control"] = cache.clone();
        }
        Value::String(text) => {
            *content = json!([{"type":"text","text":text,"cache_control":cache}]);
        }
        _ => return message.to_vec(),
    }
    serde_json::to_vec(&output).unwrap_or_else(|_| message.to_vec())
}

/// Hoists part-level or message-level `cache_control` onto the first
/// `tool_result` block. Part-level control wins. Anthropic rejects
/// `cache_control` inside `tool_result.content`.
pub fn attach_tool_message_cache_control(message: &[u8], src: &Value) -> Vec<u8> {
    let cache = extract_first_part_cache_control(src)
        .or_else(|| valid_cache_control(src.get("cache_control")).cloned());
    let Some(cache) = cache else {
        return message.to_vec();
    };
    let Ok(mut output) = serde_json::from_slice::<Value>(message) else {
        return message.to_vec();
    };
    let Some(parts) = output.get_mut("content").and_then(Value::as_array_mut) else {
        return message.to_vec();
    };
    let Some(target) = parts.iter_mut().find(|block| {
        block.get("type").and_then(Value::as_str) == Some("tool_result") && block.is_object()
    }) else {
        return message.to_vec();
    };
    let Some(object) = target.as_object_mut() else {
        return message.to_vec();
    };
    object.insert("cache_control".to_owned(), cache);
    serde_json::to_vec(&output).unwrap_or_else(|_| message.to_vec())
}

fn extract_first_part_cache_control(src: &Value) -> Option<Value> {
    let content = match src.get("content") {
        Some(content) if !content.is_null() => content,
        _ => src,
    };
    match content {
        Value::Array(parts) => parts
            .iter()
            .find_map(|part| valid_cache_control(part.get("cache_control")).cloned()),
        Value::Object(_) => valid_cache_control(content.get("cache_control")).cloned(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{attach_cache_control, attach_message_cache_control};
    use serde_json::{json, Value};

    #[test]
    fn copies_promotes_and_respects_part_precedence() {
        let output: Value = serde_json::from_slice(&attach_cache_control(
            br#"{"type":"text","text":"hi"}"#,
            &json!({"cache_control":{"type":"ephemeral","ttl":"5m"}}),
        ))
        .unwrap();
        assert_eq!(output["cache_control"]["ttl"], "5m");

        let output: Value = serde_json::from_slice(&attach_message_cache_control(
            br#"{"role":"user","content":"hi"}"#,
            &json!({"cache_control":{"type":"ephemeral"}}),
        ))
        .unwrap();
        assert_eq!(output["content"][0]["text"], "hi");
        assert_eq!(output["content"][0]["cache_control"]["type"], "ephemeral");

        let original = br#"{"content":[{"type":"text","cache_control":{"type":"ephemeral"}}]}"#;
        assert_eq!(
            attach_message_cache_control(original, &json!({"cache_control":{"ttl":"1h"}})),
            original
        );
    }

    #[test]
    fn all_noop_paths_preserve_bytes() {
        let original = b" { \"content\" : \"hi\" } ";
        assert_eq!(attach_cache_control(original, &json!({})), original);
        assert_eq!(
            attach_message_cache_control(original, &json!({"cache_control":null})),
            original
        );
    }
}
