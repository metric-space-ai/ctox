// ref: internal/runtime/executor/helps/payload_mutations_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::cell::Cell;

use serde::Serialize;
use serde_json::{json, Value};

use super::{
    join_raw_json_array, join_raw_json_strings, remove_tool_type_from_tools_array,
    set_bool_if_different, set_payload_value_if_different, set_raw_if_different,
    set_string_if_different,
};

#[test]
fn set_string_if_different_normalizes_wrong_type_without_mutating_source() {
    let input = br#"{"model":123}"#.to_vec();
    let original = input.clone();
    let output = set_string_if_different(input, "model", "123");
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap()["model"],
        "123"
    );
    assert_eq!(original, br#"{"model":123}"#);
}

#[test]
fn set_bool_if_different_normalizes_wrong_type() {
    let output = set_bool_if_different(br#"{"stream":"true"}"#.to_vec(), "stream", true);
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap()["stream"],
        true
    );
}

#[test]
fn set_raw_if_different_updates_different_raw_value() {
    let output = set_raw_if_different(
        br#"{"metadata":"executor"}"#.to_vec(),
        "metadata",
        br#"{"source":"executor"}"#,
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap()["metadata"],
        json!({"source": "executor"})
    );
}

#[test]
fn projection_override_writes_every_resolved_match() {
    let mut output = br#"{"items":[{"value":1},{"value":2}]}"#.to_vec();
    for path in ["items.0.value", "items.1.value"] {
        output = set_raw_if_different(output, path, br#"[1,2]"#);
    }
    let output = serde_json::from_slice::<Value>(&output).unwrap();
    assert_eq!(output["items"][0]["value"], json!([1, 2]));
    assert_eq!(output["items"][1]["value"], json!([1, 2]));
}

#[test]
fn projection_raw_override_writes_every_resolved_match() {
    let mut output = br#"{"items":[{"value":1},{"value":2}]}"#.to_vec();
    for path in ["items.0.value", "items.1.value"] {
        output = set_raw_if_different(output, path, br#"[1,2]"#);
    }
    let output = serde_json::from_slice::<Value>(&output).unwrap();
    for index in 0..2 {
        assert_eq!(output["items"][index]["value"], json!([1, 2]));
    }
}

#[test]
fn byte_slice_override_is_normalized_to_a_json_string_at_the_typed_boundary() {
    // Go's `any`-typed config map needs a runtime []byte special case. The
    // Rust configuration boundary is typed, so the corresponding byte value
    // is decoded before entering the string mutation helper.
    let configured = b"abc";
    let configured = std::str::from_utf8(configured).unwrap();
    let output = set_string_if_different(br#"{"value":"YWJj"}"#.to_vec(), "value", configured);
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap()["value"],
        "abc"
    );
}

#[derive(Clone, Copy)]
struct GoFloat32(f32);

impl Serialize for GoFloat32 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_f64(f64::from(self.0))
    }
}

struct CountingMarshaler<'a> {
    calls: &'a Cell<usize>,
    value: &'a str,
}

impl Serialize for CountingMarshaler<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.calls.set(self.calls.get() + 1);
        serializer.serialize_str(self.value)
    }
}

#[test]
fn set_payload_value_if_different_calls_marshaler_once() {
    for input in [br#"{"value":"old"}"#.as_slice(), br#"{"value":"new"}"#] {
        let calls = Cell::new(0);
        let value = CountingMarshaler {
            calls: &calls,
            value: "new",
        };
        let output = set_payload_value_if_different(input.to_vec(), "value", &value);
        assert_eq!(calls.get(), 1);
        assert_eq!(
            serde_json::from_slice::<Value>(&output).unwrap()["value"],
            "new"
        );
    }
}

#[test]
fn joins_raw_items_without_reencoding() {
    let items = [br#" {"x":1} "#.as_slice(), br#"true"#];
    assert_eq!(join_raw_json_array(&items), br#"[ {"x":1} ,true]"#);
    assert_eq!(
        join_raw_json_strings(&[r#"{"x":1}"#.into(), "false".into()]),
        br#"[{"x":1},false]"#
    );
}

#[test]
fn invalid_inputs_are_exact_noops() {
    let input = b"not-json \xff".to_vec();
    assert_eq!(set_string_if_different(input.clone(), "model", "x"), input);
    let input = br#"{"x":1}"#.to_vec();
    assert_eq!(set_raw_if_different(input.clone(), "x", b"not-json"), input);
}
