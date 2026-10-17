// ref: internal/translator/claude/interactions/interactions_claude_test.go:1-181 @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use super::{
    convert_claude_response_to_interactions, convert_claude_response_to_interactions_non_stream,
    convert_interactions_request_to_claude, ClaudeToInteractionsState,
};

fn request(raw: &str) -> Value {
    serde_json::from_slice(&convert_interactions_request_to_claude(
        "claude-test",
        raw.as_bytes(),
        false,
    ))
    .unwrap()
}

fn payload(events: &[Vec<u8>], event_type: &str) -> Option<Value> {
    events.iter().find_map(|event| {
        event.split(|byte| *byte == b'\n').find_map(|line| {
            let payload = line.strip_prefix(b"data: ")?;
            let value = serde_json::from_slice::<Value>(payload).ok()?;
            (value.get("event_type").and_then(Value::as_str) == Some(event_type)).then_some(value)
        })
    })
}
