// ref: internal/translator/common/cache_control_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    attach_cache_control, attach_message_cache_control, attach_tool_message_cache_control,
};
use serde_json::{json, Value};

#[test]
fn attach_cache_control_copies_object_and_ignores_missing() {
    let output: Value = serde_json::from_slice(&attach_cache_control(
        br#"{"type":"text","text":"hi"}"#,
        &json!({"text":"hi","cache_control":{"type":"ephemeral","ttl":"5m"}}),
    ))
    .unwrap();
    assert_eq!(output["cache_control"]["type"], "ephemeral");
    assert_eq!(output["cache_control"]["ttl"], "5m");

    let original = br#"{"type":"text","text":"hi"}"#;
    assert_eq!(
        attach_cache_control(original, &json!({"text":"hi"})),
        original
    );
}

#[test]
fn message_cache_control_promotes_string_and_respects_last_part() {
    let output: Value = serde_json::from_slice(&attach_message_cache_control(
        br#"{"role":"user","content":"hi"}"#,
        &json!({"role":"user","content":"hi","cache_control":{"type":"ephemeral"}}),
    ))
    .unwrap();
    assert_eq!(output["content"][0]["type"], "text");
    assert_eq!(output["content"][0]["text"], "hi");
    assert_eq!(output["content"][0]["cache_control"]["type"], "ephemeral");

    let original = br#"{"role":"user","content":[{"type":"text","text":"hi","cache_control":{"type":"ephemeral"}}]}"#;
    let output = attach_message_cache_control(
        original,
        &json!({"cache_control":{"type":"ephemeral","ttl":"1h"}}),
    );
    let output: Value = serde_json::from_slice(&output).unwrap();
    assert!(output["content"][0]["cache_control"].get("ttl").is_none());
}

#[test]
fn cache_control_requires_exact_ephemeral_type() {
    let original = br#"{"type":"text","text":"hi"}"#;
    for source in [
        json!({"cache_control":{"ttl":"5m"}}),
        json!({"cache_control":{"type":"invalid"}}),
        json!({"cache_control":{"type":" ephemeral "}}),
        json!({"cache_control":{"type":123}}),
        json!({"cache_control":"ephemeral"}),
    ] {
        assert_eq!(attach_cache_control(original, &source), original);
        assert_eq!(
            attach_message_cache_control(br#"{"role":"user","content":"hi"}"#, &source),
            br#"{"role":"user","content":"hi"}"#
        );
    }
}

#[test]
fn tool_message_cache_control_hoists_part_level_over_message_level() {
    let output: Value = serde_json::from_slice(&attach_tool_message_cache_control(
        br#"{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_1","content":[{"type":"text","text":"4"}]}]}"#,
        &json!({"role":"tool","content":[{"type":"text","text":"4","cache_control":{"type":"ephemeral"}}],"cache_control":{"type":"ephemeral","ttl":"1h"}}),
    ))
    .unwrap();
    assert_eq!(output["content"][0]["cache_control"]["type"], "ephemeral");
    assert!(output["content"][0]["cache_control"].get("ttl").is_none());
}

#[test]
fn invalid_part_cache_control_does_not_block_message_level() {
    for part in [
        json!({}),
        json!({"type":"invalid"}),
        json!({"type":" ephemeral "}),
        json!({"type":123}),
    ] {
        let output: Value = serde_json::from_slice(&attach_tool_message_cache_control(
            br#"{"role":"user","content":[{"type":"text","text":"just text"},{"type":"tool_result","tool_use_id":"call_1","content":"4"}]}"#,
            &json!({"content":[{"type":"text","cache_control":part}],"cache_control":{"type":"ephemeral","ttl":"1h"}}),
        ))
        .unwrap();
        assert_eq!(output["content"][1]["cache_control"]["type"], "ephemeral");
        assert_eq!(output["content"][1]["cache_control"]["ttl"], "1h");
        assert!(output["content"][0].get("cache_control").is_none());
    }
}

#[test]
fn tool_message_without_tool_result_stays_untouched() {
    let original = br#"{"role":"user","content":[{"type":"text","text":"just text"}]}"#;
    assert_eq!(
        attach_tool_message_cache_control(
            original,
            &json!({"role":"tool","cache_control":{"type":"ephemeral"}})
        ),
        original
    );
    assert_eq!(
        attach_tool_message_cache_control(
            br#"{"role":"user","content":"4"}"#,
            &json!({"cache_control":{"type":"ephemeral"}})
        ),
        br#"{"role":"user","content":"4"}"#
    );
}
