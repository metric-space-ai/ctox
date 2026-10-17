// ref: internal/translator/claude/gemini/noop_optimization_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{lowercase_claude_tool_schema_types, normalize_claude_tool_schema};
use serde_json::Value;
use std::borrow::Cow;

#[test]
fn wrong_and_uppercase_schema_types_are_normalized() {
    let value: Value = serde_json::from_slice(&normalize_claude_tool_schema(
        br#"{"type":"object","additionalProperties":"false","$schema":123}"#,
    ))
    .unwrap();
    assert_eq!(value["additionalProperties"], false);
    let normalized = lowercase_claude_tool_schema_types(br#"{"input_schema":{"type":"OBJECT","properties":{"value":{"type":"STRING"}},"other":{"type":123}}}"#);
    let value: Value = serde_json::from_slice(&normalized).unwrap();
    assert_eq!(value.pointer("/input_schema/type").unwrap(), "object");
    assert_eq!(
        value
            .pointer("/input_schema/properties/value/type")
            .unwrap(),
        "string"
    );
    assert_eq!(value.pointer("/input_schema/other/type").unwrap(), "123");
}
