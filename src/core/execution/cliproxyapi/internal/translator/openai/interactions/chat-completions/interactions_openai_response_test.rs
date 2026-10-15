// ref: internal/translator/openai/interactions/chat-completions/interactions_openai_response_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use gjson;
use serde_json::Value;

use super::{
    convert_interactions_response_to_openai, convert_interactions_response_to_openai_non_stream,
    convert_openai_response_to_interactions, convert_openai_response_to_interactions_non_stream,
    InteractionsToOpenAIChatStreamState, OpenAIToInteractionsStreamState,
};
use crate::sdk::translator::{TranslationContext, TranslationState};

fn count_events(events: &[Vec<u8>], event_type: &str) -> usize {
    events
        .iter()
        .filter(|event| interactions_event_name(event) == event_type)
        .count()
}

fn find_event_payload<'a>(events: &'a [Vec<u8>], event_type: &str) -> Option<&'a [u8]> {
    events
        .iter()
        .find(|event| interactions_event_name(event) == event_type)
        .and_then(|event| interactions_sse_payload(event))
}

fn interactions_event_name(event: &[u8]) -> String {
    let text = std::str::from_utf8(event).unwrap_or("");
    if let Some(rest) = text.strip_prefix("event: ") {
        if let Some(line) = rest.lines().next() {
            return line.to_owned();
        }
    }
    if let Some(payload) = interactions_sse_payload(event) {
        let value: Value = serde_json::from_slice(payload).unwrap_or(Value::Null);
        if let Some(name) = value.get("event_type").and_then(Value::as_str) {
            return name.to_owned();
        }
    }
    String::new()
}

fn json_at(bytes: &[u8], pointer: &str) -> Value {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|value| value.pointer(pointer).cloned())
        .unwrap_or(Value::Null)
}

fn interactions_sse_payload(event: &[u8]) -> Option<&[u8]> {
    let marker = b"\ndata: ";
    event
        .windows(marker.len())
        .position(|window| window == marker)
        .map(|index| &event[index + marker.len()..])
}

fn find_openai_chat_chunk<'a>(chunks: &'a [Vec<u8>], path: &str) -> Option<&'a [u8]> {
    chunks
        .iter()
        .find(|chunk| gjson::get(std::str::from_utf8(chunk).unwrap_or(""), path).exists())
        .map(Vec::as_slice)
}

fn find_openai_chat_chunk_value<'a>(
    chunks: &'a [Vec<u8>],
    path: &str,
    want: &str,
) -> Option<&'a [u8]> {
    chunks.iter().find_map(|chunk| {
        let parsed = gjson::get(std::str::from_utf8(chunk).unwrap_or(""), path);
        if parsed.exists() && parsed.str() == want {
            Some(chunk.as_slice())
        } else {
            None
        }
    })
}
