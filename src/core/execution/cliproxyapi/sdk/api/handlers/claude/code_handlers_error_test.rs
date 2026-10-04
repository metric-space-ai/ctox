// ref: sdk/api/handlers/claude/code_handlers_error_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use super::{
    claude_error_response, forward_claude_stream_event, pool_error_response,
    AntigravityAccountPoolError, AntigravityExecutionError, AntigravityGenerateTransportFailure,
    ClaudeMessagesHttpResponse,
};

#[test]
fn candidate_v13_claude_timeout_before_stream_returns_json_without_committing_sse() {
    let response = pool_error_response(AntigravityAccountPoolError::Execution(
        AntigravityExecutionError::Transport(AntigravityGenerateTransportFailure::Timeout),
    ));
    assert_eq!(response.status(), 408);
    assert_eq!(response.content_type(), "application/json");
    let error: Value = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(error["type"], "error");
    assert_eq!(error["error"]["type"], "timeout_error");
    assert_eq!(error["error"]["message"], "Request Timeout");
    assert!(!response.body().starts_with(b"event:"));
}

#[test]
fn candidate_v13_claude_committed_stream_timeout_closes_after_one_error() {
    let mut terminal = false;
    let started = b"event: message_start\ndata: {\"type\":\"message_start\"}\n\n".to_vec();
    assert_eq!(
        forward_claude_stream_event(Some(Ok(started.clone())), &mut terminal),
        Some(started)
    );
    assert!(!terminal);
    let delta = b"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"partial\"}}\n\n".to_vec();
    assert_eq!(
        forward_claude_stream_event(Some(Ok(delta.clone())), &mut terminal),
        Some(delta)
    );
    let failure = forward_claude_stream_event(
        Some(Err(AntigravityGenerateTransportFailure::Timeout)),
        &mut terminal,
    )
    .unwrap();
    assert!(terminal);
    let data = failure.strip_prefix(b"event: error\ndata: ").unwrap();
    let data = data.strip_suffix(b"\n\n").unwrap();
    let error: Value = serde_json::from_slice(data).unwrap();
    assert_eq!(error["type"], "error");
    assert_eq!(error["error"]["type"], "timeout_error");
    assert_eq!(error["error"]["message"], "Request Timeout");
    assert!(forward_claude_stream_event(
        Some(Err(AntigravityGenerateTransportFailure::Timeout)),
        &mut terminal
    )
    .is_none());
    assert!(forward_claude_stream_event(
        Some(Ok(b"event: message_stop\ndata: {}\n\n".to_vec())),
        &mut terminal
    )
    .is_none());
}

#[test]
fn candidate_v13_claude_committed_stream_keeps_protocol_errors_and_clean_stop() {
    for failure in [
        AntigravityGenerateTransportFailure::Connect,
        AntigravityGenerateTransportFailure::Protocol,
    ] {
        let mut terminal = false;
        let event = forward_claude_stream_event(Some(Err(failure)), &mut terminal).unwrap();
        assert_eq!(
            event.as_slice(),
            b"event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"api_error\",\"message\":\"Antigravity upstream stream failed\"}}\n\n"
        );
        assert!(terminal);
        assert!(forward_claude_stream_event(None, &mut terminal).is_none());
    }
    let mut terminal = false;
    let stopped = b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_vec();
    assert_eq!(
        forward_claude_stream_event(Some(Ok(stopped.clone())), &mut terminal),
        Some(stopped)
    );
    assert!(terminal);
    assert!(forward_claude_stream_event(
        Some(Err(AntigravityGenerateTransportFailure::Timeout)),
        &mut terminal
    )
    .is_none());
}

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
