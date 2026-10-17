// ref: internal/translator/antigravity/interactions/interactions_antigravity_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use super::{
    convert_antigravity_response_to_interactions,
    convert_antigravity_response_to_interactions_non_stream,
    convert_interactions_request_to_antigravity, AntigravityToInteractionsState,
};
use crate::internal::util::sanitized_function_name_map;

fn parse(raw: &[u8]) -> Value {
    serde_json::from_slice(raw).unwrap()
}

fn event_payload(
    events: &[Vec<u8>],
    event_type: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Option<Value> {
    events.iter().find_map(|event| {
        let text = String::from_utf8_lossy(event);
        let data = text.lines().find_map(|line| line.strip_prefix("data: "))?;
        let value = serde_json::from_str::<Value>(data).ok()?;
        (value.get("event_type").and_then(Value::as_str) == Some(event_type) && predicate(&value))
            .then_some(value)
    })
}
