// ref: internal/translator/openai/interactions/responses/interactions_openai_responses_response_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    convert_interactions_response_to_openai_responses_non_stream,
    convert_interactions_response_to_openai_responses_stream,
    convert_openai_responses_response_to_interactions_non_stream,
    convert_openai_responses_response_to_interactions_stream,
};
use crate::sdk::translator::{TranslationContext, TranslationState};
use serde_json::{json, Value};

type NonStreamTransform =
    fn(&TranslationContext, &str, &[u8], &[u8], &[u8], &mut TranslationState) -> Vec<u8>;

fn convert(transform: NonStreamTransform, raw: &[u8]) -> Value {
    let mut state = None;
    serde_json::from_slice(&transform(
        &TranslationContext::default(),
        "gpt-test",
        b"{}",
        b"{}",
        raw,
        &mut state,
    ))
    .unwrap()
}

fn event(raw: &[u8]) -> (String, Value) {
    let text = std::str::from_utf8(raw).unwrap();
    let name = text
        .lines()
        .find_map(|line| line.strip_prefix("event: "))
        .unwrap_or("")
        .into();
    let data = text
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .unwrap_or("");
    let value = if data == "[DONE]" {
        Value::String(data.into())
    } else {
        serde_json::from_str(data).unwrap()
    };
    (name, value)
}

// ref: interactions_openai_responses_response_test.go @ d7914afd
