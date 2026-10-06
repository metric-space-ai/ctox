// ref: internal/runtime/executor/openai_responses_signature.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

//! Sanitizes reasoning history before forwarding an OpenAI Responses request.
//!
//! The Go implementation rebuilds only the `input` array once an edit is
//! required. `Cow` preserves the same allocation-free, byte-identical no-op
//! behavior while keeping all authority request-local.

use std::borrow::Cow;

use serde_json::Value;

use crate::internal::signature::{
    detect_signature_provider, is_valid_gpt_reasoning_signature, SignatureProvider,
};

#[must_use]
pub fn sanitize_openai_responses_reasoning_encrypted_content<'a>(
    provider: &str,
    body: &'a [u8],
) -> Cow<'a, [u8]> {
    sanitize_openai_responses_reasoning_encrypted_content_with_compat(provider, body, false)
}

/// Compatibility is selected by owned model capabilities, never client metadata.
#[must_use]
pub fn sanitize_openai_responses_reasoning_encrypted_content_with_compat<'a>(
    _provider: &str,
    body: &'a [u8],
    is_compat: bool,
) -> Cow<'a, [u8]> {
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return Cow::Borrowed(body);
    };
    let store = root.get("store").and_then(Value::as_bool).unwrap_or(false);
    let Some(input) = root.get_mut("input").and_then(Value::as_array_mut) else {
        return Cow::Borrowed(body);
    };

    let mut changed = false;
    for item in input {
        if item.get("type").and_then(Value::as_str).map(str::trim) != Some("reasoning") {
            continue;
        }
        let encrypted = item.get("encrypted_content");
        // ref: internal/runtime/executor/openai_responses_signature.go:168:175 @ a2976eb8a303f11b4ea5177bce9f9ff752634dfc
        let valid = encrypted.and_then(Value::as_str).is_some_and(|value| {
            value == value.trim()
                && (is_valid_gpt_reasoning_signature(value)
                    || (is_compat
                        && !value.is_empty()
                        && detect_signature_provider(value) == SignatureProvider::Unknown))
        });
        let encrypted_present = encrypted.is_some();
        let Some(object) = item.as_object_mut() else {
            continue;
        };

        if !is_compat {
            if let Some(content) = object
                .get("content")
                .and_then(Value::as_array)
                .filter(|content| !content.is_empty())
            {
                let summary_empty = match object.get("summary") {
                    None | Some(Value::Null) => true,
                    Some(Value::Array(summary)) => summary.is_empty(),
                    _ => false,
                };
                if summary_empty {
                    let summary: Vec<Value> = content
                        .iter()
                        .filter_map(|part| {
                            if part.get("type").and_then(Value::as_str).map(str::trim)
                                != Some("reasoning_text")
                            {
                                return None;
                            }
                            let text = part
                                .get("text")
                                .and_then(Value::as_str)
                                .filter(|text| !text.is_empty())?;
                            Some(serde_json::json!({"type":"summary_text","text":text}))
                        })
                        .collect();
                    if !summary.is_empty() {
                        object.insert("summary".into(), Value::Array(summary));
                    }
                }
                object.insert("content".into(), Value::Array(Vec::new()));
                changed = true;
            }
        }
        if encrypted_present && !valid {
            object.remove("encrypted_content");
            changed = true;
        }
        if !is_compat && !store && object.contains_key("id") && (!encrypted_present || !valid) {
            object.remove("id");
            changed = true;
        }
    }

    if !changed {
        return Cow::Borrowed(body);
    }
    serde_json::to_vec(&root)
        .map(Cow::Owned)
        .unwrap_or(Cow::Borrowed(body))
}
