// ref: internal/translator/gemini/claude/gemini_claude_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{Map, Value};

use crate::internal::translator::antigravity::claude::convert_claude_request_for_direct_gemini;

/// Direct Gemini shares media/tool conversion with Antigravity while retaining
/// its own upstream thinking replay policy.
pub fn convert_claude_request_to_gemini(model_name: &str, input: &[u8], stream: bool) -> Vec<u8> {
    convert_claude_request_to_gemini_impl(model_name, input, stream, false)
}

/// ref: gemini/claude/gemini_claude_request.go:39-137 @ e2bff010
/// Compatibility endpoints preserve thinking blocks with a replayable Gemini
/// signature or its explicit bypass sentinel.
pub fn convert_claude_request_to_gemini_with_compat(
    model_name: &str,
    input: &[u8],
    stream: bool,
) -> Vec<u8> {
    convert_claude_request_to_gemini_impl(model_name, input, stream, true)
}

fn convert_claude_request_to_gemini_impl(
    model_name: &str,
    input: &[u8],
    stream: bool,
    preserve_thinking: bool,
) -> Vec<u8> {
    if !matches!(serde_json::from_slice::<Value>(input), Ok(Value::Object(_))) {
        return input.to_vec();
    }
    let wrapped =
        convert_claude_request_for_direct_gemini(model_name, input, stream, preserve_thinking);
    let root = serde_json::from_slice::<Value>(&wrapped).unwrap_or(Value::Null);
    let Some(mut request) = root.get("request").and_then(Value::as_object).cloned() else {
        return input.to_vec();
    };
    normalize_direct_gemini_media(&mut request);
    request.insert("model".into(), Value::String(model_name.to_owned()));
    serde_json::to_vec(&Value::Object(request)).unwrap_or_else(|_| input.to_vec())
}

fn normalize_direct_gemini_media(request: &mut Map<String, Value>) {
    for parts in request
        .get_mut("contents")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(|content| content.get_mut("parts"))
        .filter_map(Value::as_array_mut)
    {
        let mut normalized = Vec::with_capacity(parts.len());
        for mut part in parts.drain(..) {
            let embedded = part
                .pointer_mut("/functionResponse")
                .and_then(Value::as_object_mut)
                .and_then(|response| response.remove("parts"))
                .and_then(|parts| parts.as_array().cloned())
                .unwrap_or_default();
            normalize_inline_data(&mut part);
            normalized.push(part);
            for mut image in embedded {
                normalize_inline_data(&mut image);
                normalized.push(image);
            }
        }
        *parts = normalized;
    }
}

fn normalize_inline_data(part: &mut Value) {
    let Some(mut inline) = part
        .as_object_mut()
        .and_then(|part| part.remove("inlineData"))
    else {
        return;
    };
    if let Some(object) = inline.as_object_mut() {
        if let Some(mime) = object.remove("mimeType") {
            object.insert("mime_type".into(), mime);
        }
    }
    part.as_object_mut()
        .expect("Gemini part is an object")
        .insert("inline_data".into(), inline);
}

pub(super) fn translated_request_envelope(request: &[u8]) -> Vec<u8> {
    let direct = serde_json::from_slice::<Value>(request).unwrap_or(Value::Null);
    let model = direct
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut native = direct.as_object().cloned().unwrap_or_else(Map::new);
    native.remove("model");
    serde_json::to_vec(&serde_json::json!({"model":model,"request":native})).unwrap_or_default()
}
