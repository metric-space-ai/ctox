// ref: internal/runtime/executor/helps/kimi_responses.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial
// License: MIT (upstream); modifications AGPL-3.0-only

//! Reorders Kimi Responses input so parallel tool outputs stay contiguous.
//!
//! The executor calls `normalize_apply_patch_responses_request` before this
//! reorder. The response-event bridge is still unported.

use std::collections::HashMap;

use gjson::Kind;

/// Moves intervening non-tool items behind the outputs of the current call batch.
///
/// Unchanged input, invalid JSON, and a missing `input` array keep their
/// original bytes.
#[must_use]
pub fn normalize_kimi_responses_input(body: &[u8]) -> Vec<u8> {
    if body.is_empty() {
        return Vec::new();
    }
    let Ok(document) = std::str::from_utf8(body) else {
        return body.to_vec();
    };
    if serde_json::from_str::<serde_json::Value>(document).is_err() {
        return body.to_vec();
    }
    let parsed = gjson::parse(document);
    let input = parsed.get("input");
    if !input.exists() || input.kind() != Kind::Array {
        return body.to_vec();
    }
    let mut items = Vec::new();
    let mut failed = false;
    input.each(|_, item| {
        let raw = item.json();
        if raw.as_bytes().is_empty() {
            failed = true;
            return false;
        }
        let Ok(text) = std::str::from_utf8(raw.as_bytes()) else {
            failed = true;
            return false;
        };
        items.push(text.to_owned());
        true
    });
    if failed || items.is_empty() {
        return body.to_vec();
    }
    let Some(reordered) = reorder_responses_input(&items) else {
        return body.to_vec();
    };
    let Some((start, end)) = super::codex_tool_schema::first_object_field_span(document, "input")
    else {
        return body.to_vec();
    };
    let mut updated = String::with_capacity(document.len() + 16);
    updated.push_str(&document[..start]);
    updated.push('[');
    updated.push_str(&reordered.join(","));
    updated.push(']');
    updated.push_str(&document[end..]);
    updated.into_bytes()
}

fn reorder_responses_input(items: &[String]) -> Option<Vec<String>> {
    let mut result = Vec::with_capacity(items.len());
    let mut reordered = false;
    let mut index = 0;
    while index < items.len() {
        if !is_responses_tool_call(&items[index]) {
            result.push(items[index].clone());
            index += 1;
            continue;
        }
        let start_calls = index;
        let mut end_calls = index;
        let mut call_ids: HashMap<String, i32> = HashMap::new();
        let mut call_id_count = 0;
        while end_calls < items.len() && is_responses_tool_call(&items[end_calls]) {
            let call_id = extract_responses_call_id(&items[end_calls]);
            if !call_id.is_empty() {
                *call_ids.entry(call_id).or_insert(0) += 1;
                call_id_count += 1;
            }
            end_calls += 1;
        }
        result.extend(items[start_calls..end_calls].iter().cloned());
        if call_id_count == 0 {
            index = end_calls;
            continue;
        }
        let mut needed = call_ids.clone();
        let mut remaining_needed = call_id_count;
        let mut last_matching_idx = None;
        let mut cursor = end_calls;
        while cursor < items.len() && remaining_needed > 0 {
            if is_responses_tool_call(&items[cursor]) {
                break;
            }
            if is_responses_tool_output(&items[cursor]) {
                let call_id = extract_responses_call_id(&items[cursor]);
                if let Some(count) = needed.get_mut(&call_id) {
                    if *count > 0 {
                        *count -= 1;
                        remaining_needed -= 1;
                        last_matching_idx = Some(cursor);
                    }
                }
            }
            cursor += 1;
        }
        let Some(last_matching_idx) = last_matching_idx else {
            index = end_calls;
            continue;
        };
        if remaining_needed != 0 || last_matching_idx < end_calls {
            index = end_calls;
            continue;
        }
        let mut matching_outputs = Vec::new();
        let mut intervening = Vec::new();
        let mut consumed = call_ids.clone();
        for item in &items[end_calls..=last_matching_idx] {
            if is_responses_tool_output(item) {
                let call_id = extract_responses_call_id(item);
                if let Some(count) = consumed.get_mut(&call_id) {
                    if *count > 0 {
                        *count -= 1;
                        matching_outputs.push(item.clone());
                        continue;
                    }
                }
            }
            intervening.push(item.clone());
        }
        if !intervening.is_empty() {
            reordered = true;
        }
        result.extend(matching_outputs);
        result.extend(intervening);
        index = last_matching_idx + 1;
    }
    reordered.then_some(result)
}

fn is_responses_tool_call(raw: &str) -> bool {
    matches!(
        item_type(raw).as_str(),
        "function_call" | "custom_tool_call"
    )
}

fn is_responses_tool_output(raw: &str) -> bool {
    matches!(
        item_type(raw).as_str(),
        "function_call_output" | "custom_tool_call_output"
    )
}

fn item_type(raw: &str) -> String {
    let item = gjson::parse(raw);
    let kind = item.get("type");
    kind.str().trim().to_owned()
}

fn extract_responses_call_id(raw: &str) -> String {
    let item = gjson::parse(raw);
    for key in ["call_id", "tool_call_id", "callId"] {
        let value = item.get(key);
        let text = value.str().trim();
        if !text.is_empty() {
            return text.to_owned();
        }
    }
    let id_value = item.get("id");
    let id = id_value.str().trim();
    if id.starts_with("fco_") {
        return String::new();
    }
    id.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_types(body: &[u8]) -> Vec<(String, String, String)> {
        let value: serde_json::Value = serde_json::from_slice(body).unwrap();
        value["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                (
                    item.get("type")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    item.get("call_id")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    item.get("role")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn interleaved_developer_message_moves_behind_parallel_outputs() {
        let input = br#"{
            "model": "kimi-k3",
            "input": [
                {"type":"function_call","call_id":"view_image:31","name":"view_image","arguments":"{}"},
                {"type":"function_call","call_id":"view_image:32","name":"view_image","arguments":"{}"},
                {"type":"function_call_output","call_id":"view_image:31","output":"ok31"},
                {"type":"message","role":"developer","content":[{"type":"input_text","text":"keep following instructions"}]},
                {"type":"function_call_output","call_id":"view_image:32","output":"ok32"}
            ]
        }"#;
        let normalized = normalize_kimi_responses_input(input);
        let items = input_types(&normalized);
        assert_eq!(items.len(), 5);
        assert_eq!(items[0].1, "view_image:31");
        assert_eq!(items[1].1, "view_image:32");
        assert_eq!(
            (items[2].0.as_str(), items[2].1.as_str()),
            ("function_call_output", "view_image:31")
        );
        assert_eq!(
            (items[3].0.as_str(), items[3].1.as_str()),
            ("function_call_output", "view_image:32")
        );
        assert_eq!(
            (items[4].0.as_str(), items[4].2.as_str()),
            ("message", "developer")
        );
    }

    #[test]
    fn contiguous_and_incomplete_inputs_keep_their_bytes() {
        let contiguous = br#"{"model":"kimi-k3","input":[{"type":"function_call","call_id":"c1","name":"t"},{"type":"function_call_output","call_id":"c1","output":"ok"},{"type":"message","role":"developer"}]}"#;
        assert_eq!(normalize_kimi_responses_input(contiguous), contiguous);
        let missing = br#"{"model":"kimi-k3","input":[{"type":"function_call","call_id":"c1","name":"t"},{"type":"function_call","call_id":"c2","name":"t"},{"type":"function_call_output","call_id":"c1","output":"ok"},{"type":"message","role":"developer"}]}"#;
        assert_eq!(normalize_kimi_responses_input(missing), missing);
        let plain =
            br#"{"model":"kimi-k3","input":[{"type":"message","role":"user","content":"hi"}]}"#;
        assert_eq!(normalize_kimi_responses_input(plain), plain);
        assert_eq!(normalize_kimi_responses_input(b"not-json"), b"not-json");
        assert_eq!(normalize_kimi_responses_input(b"{}"), b"{}");
    }

    #[test]
    fn custom_tool_outputs_and_a_second_pass_stay_stable() {
        let input = br#"{
            "model": "kimi-k3",
            "input": [
                {"type":"custom_tool_call","call_id":"ctc_1","name":"custom_lookup"},
                {"type":"message","role":"developer","content":"note"},
                {"type":"custom_tool_call_output","call_id":"ctc_1","output":"lookup_result"}
            ]
        }"#;
        let first = normalize_kimi_responses_input(input);
        let items = input_types(&first);
        assert_eq!(
            (items[1].0.as_str(), items[1].1.as_str()),
            ("custom_tool_call_output", "ctc_1")
        );
        assert_eq!(
            (items[2].0.as_str(), items[2].2.as_str()),
            ("message", "developer")
        );
        assert_eq!(normalize_kimi_responses_input(&first), first);
    }

    #[test]
    fn call_id_falls_through_tool_call_id_and_skips_fco_ids() {
        let input = br#"{"input":[
            {"type":"function_call","tool_call_id":"from-tool","id":"fco_hidden"},
            {"type":"message","role":"user","content":"wait"},
            {"type":"function_call_output","callId":"from-tool","id":"fco_output","output":"ok"}
        ]}"#;
        let items = input_types(&normalize_kimi_responses_input(input));
        assert_eq!(items[1].0, "function_call_output");
        assert_eq!(items[2].0, "message");
    }
}
