// ref: sdk/api/handlers/openai_responses_stream_error_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use super::{
    build_openai_responses_stream_error_chunk, build_openai_responses_stream_failed_chunk,
};

#[test]
fn builds_nested_responses_stream_error_chunk() {
    let chunk = build_openai_responses_stream_error_chunk(500, "unexpected EOF", 0);
    let payload: Value = serde_json::from_slice(&chunk).unwrap();
    assert_eq!(payload["type"], "error");
    assert!(payload.get("code").is_none());
    assert_eq!(payload["error"]["code"], "internal_server_error");
    assert_eq!(payload["error"]["type"], "server_error");
    assert_eq!(payload["error"]["message"], "unexpected EOF");
    assert!(payload["error"]["param"].is_null());
    assert_eq!(payload["sequence_number"], 0);
}

#[test]
fn extracts_nested_http_error_body() {
    let chunk = build_openai_responses_stream_error_chunk(
        500,
        r#"{"error":{"message":"oops","type":"server_error","code":"internal_server_error"}}"#,
        0,
    );
    let payload: Value = serde_json::from_slice(&chunk).unwrap();
    assert_eq!(payload["type"], "error");
    assert_eq!(payload["error"]["code"], "internal_server_error");
    assert_eq!(payload["error"]["message"], "oops");
    assert_eq!(payload["error"]["type"], "server_error");
}

#[test]
fn preserves_nested_error_object() {
    let error_text = r#"{"error":{"type":"invalid_request","code":"cyber_policy","message":"This content was flagged for possible cybersecurity risk.","param":null}}"#;
    let chunk = build_openai_responses_stream_error_chunk(400, error_text, 2);
    let payload: Value = serde_json::from_slice(&chunk).unwrap();
    assert_eq!(payload["type"], "error");
    assert_eq!(payload["sequence_number"], 2);
    assert_eq!(payload["error"]["type"], "invalid_request");
    assert_eq!(payload["error"]["code"], "cyber_policy");
    assert_eq!(
        payload["error"]["message"],
        "This content was flagged for possible cybersecurity risk."
    );
    assert!(payload["error"]["param"].is_null());
}

#[test]
fn preserves_empty_and_custom_error_fields() {
    let empty = build_openai_responses_stream_error_chunk(400, r#"{"error":{}}"#, 0);
    let empty: Value = serde_json::from_slice(&empty).unwrap();
    assert_eq!(empty["error"], serde_json::json!({}));

    let custom = build_openai_responses_stream_error_chunk(
        400,
        r#"{"error":{"type":"custom_type","code":"custom_code","custom_key":"custom_val","is_flag":true,"count":42}}"#,
        5,
    );
    let custom: Value = serde_json::from_slice(&custom).unwrap();
    assert_eq!(custom["sequence_number"], 5);
    assert_eq!(custom["error"]["custom_key"], "custom_val");
    assert_eq!(custom["error"]["is_flag"], true);
    assert_eq!(custom["error"]["count"], 42);
}

#[test]
fn payload_sequence_number_overrides_argument() {
    let chunk = build_openai_responses_stream_error_chunk(
        400,
        r#"{"error":{"type":"invalid_request","code":"blocked"},"sequence_number":7}"#,
        2,
    );
    let payload: Value = serde_json::from_slice(&chunk).unwrap();
    assert_eq!(payload["sequence_number"], 7);
}

#[test]
fn failed_chunk_preserves_nested_error() {
    let chunk = build_openai_responses_stream_failed_chunk(
        400,
        r#"{"error":{"type":"invalid_request","code":"cyber_policy","message":"blocked","param":null}}"#,
        0,
    );
    let payload: Value = serde_json::from_slice(&chunk).unwrap();
    assert_eq!(payload["type"], "response.failed");
    assert_eq!(payload["sequence_number"], 0);
    assert_eq!(payload["response"]["status"], "failed");
    assert_eq!(payload["response"]["error"]["type"], "invalid_request");
    assert_eq!(payload["response"]["error"]["code"], "cyber_policy");
    assert_eq!(payload["response"]["error"]["message"], "blocked");
}

#[test]
fn failed_chunk_uses_payload_sequence_number() {
    let chunk = build_openai_responses_stream_failed_chunk(
        400,
        r#"{"error":{"type":"invalid_request","code":"cyber_policy","message":"blocked"},"sequence_number":7}"#,
        2,
    );
    let payload: Value = serde_json::from_slice(&chunk).unwrap();
    assert_eq!(payload["sequence_number"], 7);
}

#[test]
fn preserves_large_integer_precision() {
    let error_text =
        r#"{"error":{"type":"invalid_request","code":"blocked","request_id":9007199254740993}}"#;
    let chunk = build_openai_responses_stream_error_chunk(400, error_text, 0);
    let raw = String::from_utf8(chunk).unwrap();
    assert!(raw.contains("9007199254740993"), "{raw}");
    assert!(!raw.contains("9007199254740992"), "{raw}");

    let failed = build_openai_responses_stream_failed_chunk(400, error_text, 0);
    let failed = String::from_utf8(failed).unwrap();
    assert!(failed.contains("9007199254740993"), "{failed}");
    assert!(!failed.contains("9007199254740992"), "{failed}");
}

#[test]
fn request_timeout_is_retryable_server_error() {
    let chunk =
        build_openai_responses_stream_error_chunk(408, "stream disconnected before completion", 0);
    let payload: Value = serde_json::from_slice(&chunk).unwrap();
    assert_eq!(payload["type"], "error");
    assert_eq!(payload["error"]["code"], "request_timeout");
    assert_eq!(payload["error"]["type"], "server_error");

    let failed =
        build_openai_responses_stream_failed_chunk(408, "stream disconnected before completion", 0);
    let failed: Value = serde_json::from_slice(&failed).unwrap();
    assert_eq!(failed["response"]["error"]["code"], "request_timeout");
    assert_eq!(failed["response"]["error"]["type"], "server_error");
}

#[test]
fn synthesizes_scalar_top_level_code_and_message() {
    let chunk = build_openai_responses_stream_error_chunk(
        429,
        r#"{"type":"error","message":"slow down","code":429,"sequence_number":7}"#,
        0,
    );
    let payload: Value = serde_json::from_slice(&chunk).unwrap();
    assert_eq!(payload["error"]["code"], "429");
    assert_eq!(payload["error"]["message"], "slow down");
    assert_eq!(payload["error"]["type"], "invalid_request_error");
    assert_eq!(payload["sequence_number"], 7);
}
