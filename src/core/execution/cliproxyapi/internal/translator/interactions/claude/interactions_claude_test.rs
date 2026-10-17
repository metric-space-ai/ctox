// ref: internal/translator/interactions/claude/interactions_claude_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use super::{
    convert_claude_request_to_interactions, convert_interactions_response_to_claude,
    convert_interactions_response_to_claude_non_stream, InteractionsToClaudeStreamState,
};

fn parse(raw: &[u8]) -> Value {
    serde_json::from_slice(raw).unwrap()
}

fn event_payload(events: &[Vec<u8>], name: &str) -> Option<Value> {
    let marker = format!("event: {name}");
    events.iter().find_map(|event| {
        event
            .windows(marker.len())
            .any(|window| window == marker.as_bytes())
            .then(|| {
                event.split(|byte| *byte == b'\n').find_map(|line| {
                    serde_json::from_slice::<Value>(line.strip_prefix(b"data: ")?).ok()
                })
            })
            .flatten()
    })
}
