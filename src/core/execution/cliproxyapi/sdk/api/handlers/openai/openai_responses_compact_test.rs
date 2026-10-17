// ref: sdk/api/handlers/openai/openai_responses_compact_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;

#[test]
fn compact_rejects_stream_but_accepts_buffered_request() {
    let stream = parse_request(br#"{"model":"gpt-5","stream":true}"#).unwrap();
    assert!(stream.stream);
    let buffered = parse_request(br#"{"model":"gpt-5","input":"hello"}"#).unwrap();
    assert!(!buffered.stream);
    assert_eq!(buffered.model, "gpt-5");
}
