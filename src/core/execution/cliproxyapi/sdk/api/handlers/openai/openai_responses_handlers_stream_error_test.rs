// ref: sdk/api/handlers/openai/openai_responses_handlers_stream_error_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use crate::sdk::api::handlers::openai_responses_stream_error::build_openai_responses_stream_error_event;

#[test]
fn forwarded_terminal_error_uses_responses_stream_chunk_not_http_error_body() {
    let event = build_openai_responses_stream_error_event(500, "unexpected EOF", 0);
    assert!(event.starts_with(b"data: "));
    assert!(event.ends_with(b"\n\n"));
    let payload: Value = serde_json::from_slice(&event[6..event.len() - 2]).unwrap();
    assert_eq!(payload["type"], "error");
    assert!(payload.get("code").is_none());
    assert_eq!(payload["error"]["code"], "internal_server_error");
    assert_eq!(payload["error"]["type"], "server_error");
    assert_eq!(payload["error"]["message"], "unexpected EOF");
}
