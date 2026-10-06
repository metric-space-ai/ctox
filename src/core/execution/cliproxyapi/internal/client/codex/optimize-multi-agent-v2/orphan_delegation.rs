// ref: internal/client/codex/optimize-multi-agent-v2/orphan_delegation.go:25-121 @ e2bff010
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::translator::common::set_raw_path;
use std::collections::BTreeMap;

/// Only opted-in collab_spawn requests downgrade unmatched CTOX delegation
/// outputs. Pair counts consume once, including outputs for non-target tools.
#[must_use]
pub fn rewrite_orphan_delegation_input(payload: &[u8], subagent: &str, enabled: bool) -> Vec<u8> {
    if !enabled || payload.is_empty() || !subagent.eq_ignore_ascii_case("collab_spawn") {
        return payload.to_vec();
    }
    let input = crate::internal::util::get_gjson_bytes_no_copy(payload, "input");
    if input.kind() != gjson::Kind::Array {
        return payload.to_vec();
    }
    let mut available = BTreeMap::<String, usize>::new();
    input.each(|_, item| {
        if item.get("type").str() == "function_call" {
            let value = item.get("call_id");
            let id = match value.kind() {
                gjson::Kind::String => value.str().to_owned(),
                gjson::Kind::Null => String::new(),
                _ => value.json().to_owned(),
            };
            if !id.trim().is_empty() {
                *available.entry(id).or_default() += 1;
            }
        }
        true
    });
    let mut updated = payload.to_vec();
    let mut index = 0;
    input.each(|_, item| {
        let current = index;
        index += 1;
        if item.get("type").str() != "function_call_output" { return true; }
        let value = item.get("call_id");
        let id = match value.kind() {
            gjson::Kind::String => value.str().to_owned(),
            gjson::Kind::Null => String::new(),
            _ => value.json().to_owned(),
        };
        if !id.trim().is_empty() {
            if let Some(count) = available.get_mut(&id).filter(|count| **count > 0) {
                *count -= 1;
                return true;
            }
        }
        if item.get("namespace").str() != "codex_app" { return true; }
        let label = match item.get("name").str() {
            "create_thread" => "codex_app__create_thread",
            "send_message_to_thread" => "codex_app__send_message_to_thread",
            _ => return true,
        };
        let output = item.get("output");
        let text = if !output.exists() { "" } else if output.kind() == gjson::Kind::String {
            output.str()
        } else { output.json() };
        let text = serde_json::to_string(&format!("Tool output from {label}:\n{text}"))
            .expect("string JSON");
        let message = format!(r#"{{"type":"message","role":"user","content":[{{"type":"input_text","text":{text}}}]}}"#);
        updated = set_raw_path(&updated, &format!("input.{current}"), message.as_bytes());
        true
    });
    updated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_orphan_delegation_requires_opt_in_and_exact_case_insensitive_header() {
        let body = br#"{"input":[{"type":"function_call_output","namespace":"codex_app","name":"create_thread","output":"x"}]}"#;
        for (enabled, header) in [
            (false, "collab_spawn"),
            (true, ""),
            (true, " collab_spawn "),
        ] {
            assert_eq!(rewrite_orphan_delegation_input(body, header, enabled), body);
        }
        let out = rewrite_orphan_delegation_input(body, "COLLAB_SPAWN", true);
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["input"][0]["role"], "user");
        assert_eq!(
            value["input"][0]["content"][0]["text"],
            "Tool output from codex_app__create_thread:\nx"
        );
    }

    #[test]
    fn candidate_orphan_delegation_consumes_pairs_before_the_target_filter() {
        let body = br#"{"input":[{"type":"function_call","call_id":"x"},{"type":"function_call_output","call_id":"x","namespace":"other","name":"create_thread"},{"type":"function_call_output","call_id":"x","namespace":"codex_app","name":"send_message_to_thread","output":{"n":1.00}},{"type":"function_call_output","namespace":"codex_app","name":"delete_thread","output":"keep"}]}"#;
        let out = rewrite_orphan_delegation_input(body, "collab_spawn", true);
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["input"][0]["type"], "function_call");
        assert_eq!(value["input"][1]["type"], "function_call_output");
        assert_eq!(
            value["input"][2]["content"][0]["text"],
            "Tool output from codex_app__send_message_to_thread:\n{\"n\":1.00}"
        );
        assert_eq!(value["input"][3]["type"], "function_call_output");
    }

    #[test]
    fn candidate_orphan_delegation_preserves_duplicate_roots_and_raw_siblings() {
        let body = br#"{ "kept":1.00, "input":[{"type":"function_call_output","namespace":"codex_app","name":"create_thread","output":"first"},{"type":"message","number":2.00}], "input":[{"type":"message","text":"last"}] }"#;
        let out = rewrite_orphan_delegation_input(body, "collab_spawn", true);
        let text = std::str::from_utf8(&out).unwrap();
        assert!(text.starts_with(r#"{ "kept":1.00, "input":["#));
        assert!(text.contains(r#"{"type":"message","number":2.00}"#));
        assert!(text.ends_with(r#", "input":[{"type":"message","text":"last"}] }"#));
        assert!(text.contains("Tool output from codex_app__create_thread"));
    }
}
