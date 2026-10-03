// ref: sdk/api/handlers/claude/code_handlers_error_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use super::{claude_error_response, ClaudeMessagesHttpResponse};

// ref: sdk/api/handlers/claude/code_handlers_error_test.go:18-93 @ d7914afd
#[test]
fn candidate_v13_claude_status_mapping_preserves_retry_classification() {
    for (status, expected) in [
        (400, "invalid_request_error"),
        (401, "authentication_error"),
        (402, "billing_error"),
        (403, "permission_error"),
        (404, "not_found_error"),
        (408, "timeout_error"),
        (413, "request_too_large"),
        (429, "rate_limit_error"),
        (500, "api_error"),
        (502, "api_error"),
        (503, "api_error"),
        (504, "timeout_error"),
        (529, "overloaded_error"),
    ] {
        assert_eq!(
            claude_error_response(status, None).error.error_type,
            expected,
            "{status}"
        );
    }
    assert_eq!(
        claude_error_response(408, None).error.message,
        "Request Timeout"
    );
    assert_eq!(
        claude_error_response(504, None).error.message,
        "Gateway Timeout"
    );
}

#[test]
fn candidate_v13_claude_incomplete_stream_error_keeps_json_status_and_message() {
    const MESSAGE: &str = "stream error: stream disconnected before completion: stream closed before response.completed";
    for status in [408, 504] {
        let response = ClaudeMessagesHttpResponse::upstream_error(status, MESSAGE);
        assert_eq!(response.status(), status);
        assert_eq!(response.content_type(), "application/json");
        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body["type"], "error");
        assert_eq!(body["error"]["type"], "timeout_error");
        assert_eq!(body["error"]["message"], MESSAGE);
    }
    // An explicit upstream classification remains authoritative.
    let response = claude_error_response(
        408,
        Some(r#"{"error":{"type":"permission_error","message":"quota permission denied"}}"#),
    );
    assert_eq!(response.error.error_type, "permission_error");
    assert_eq!(response.error.message, "quota permission denied");
}

#[test]
fn claude_error_extracts_openai_style_upstream_json() {
    let response = claude_error_response(
        400,
        Some(
            r#"{"error":{"message":"Your input exceeds the context window of this model. Please adjust your input and try again.","type":"invalid_request_error","code":"context_too_large"}}"#,
        ),
    );

    assert_eq!(response.response_type, "error");
    assert_eq!(response.error.error_type, "invalid_request_error");
    assert_eq!(
        response.error.message,
        "Your input exceeds the context window of this model. Please adjust your input and try again."
    );
}

#[test]
fn claude_error_extracts_claude_style_upstream_json() {
    let response = claude_error_response(
        429,
        Some(
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"This request would exceed your account's rate limit. Please try again later."},"request_id":"req_123"}"#,
        ),
    );

    assert_eq!(response.error.error_type, "rate_limit_error");
    assert_eq!(
        response.error.message,
        "This request would exceed your account's rate limit. Please try again later."
    );
}

#[test]
fn write_claude_error_response_uses_claude_envelope() {
    let response = ClaudeMessagesHttpResponse::upstream_error(
        400,
        r#"{"error":{"message":"Your input exceeds the context window of this model. Please adjust your input and try again.","type":"invalid_request_error","code":"context_too_large"}}"#,
    );

    assert_eq!(response.status(), 400);
    assert_eq!(response.content_type(), "application/json");
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body["type"], "error");
    assert_eq!(body["error"]["type"], "invalid_request_error");
    assert_eq!(
        body["error"]["message"],
        "Your input exceeds the context window of this model. Please adjust your input and try again."
    );
}

#[test]
fn status_mapping_and_nested_code_fallback_match_claude_contract() {
    let overloaded = claude_error_response(529, None);
    assert_eq!(overloaded.error.error_type, "overloaded_error");
    assert_eq!(overloaded.error.message, "Overloaded");

    let code_only = claude_error_response(
        400,
        Some(r#"{"error":{"type":"invalid_request_error","code":"context_too_large"}}"#),
    );
    assert_eq!(code_only.error.error_type, "invalid_request_error");
    assert_eq!(code_only.error.message, "context_too_large");
}
