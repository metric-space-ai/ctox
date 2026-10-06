// ref: internal/signature/claude_messages_sanitize.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;

use super::{compatible_signature_for_provider, SignatureProvider};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClaudeSignatureSanitizeReport {
    pub preserved: usize,
    pub dropped_blocks: usize,
    pub dropped_signatures: usize,
}

pub fn sanitize_claude_messages_for_claude_upstream(
    payload: &[u8],
    preserve_empty_thinking_blocks: bool,
) -> (Vec<u8>, ClaudeSignatureSanitizeReport) {
    let Ok(mut root) = serde_json::from_slice::<Value>(payload) else {
        return (payload.to_vec(), ClaudeSignatureSanitizeReport::default());
    };
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return (payload.to_vec(), ClaudeSignatureSanitizeReport::default());
    };
    let mut report = ClaudeSignatureSanitizeReport::default();
    let mut modified = false;
    messages.retain_mut(|message| {
        let Some(parts) = message.get_mut("content").and_then(Value::as_array_mut) else {
            return true;
        };
        let before = parts.len();
        parts.retain_mut(|part| match part.get("type").and_then(Value::as_str) {
            Some("tool_use") => {
                if strip_tool_signature_fields(part) {
                    modified = true;
                    report.dropped_signatures += 1;
                }
                true
            }
            Some("thinking") => {
                if preserve_empty_thinking_blocks {
                    report.preserved += 1;
                    return true;
                }
                let raw = part
                    .get("signature")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let Some(normalized) =
                    compatible_signature_for_provider(SignatureProvider::Claude, raw)
                else {
                    modified = true;
                    report.dropped_blocks += 1;
                    return false;
                };
                report.preserved += 1;
                if normalized != raw {
                    if let Some(object) = part.as_object_mut() {
                        object.insert("signature".to_owned(), Value::String(normalized));
                        modified = true;
                    }
                }
                true
            }
            _ => true,
        });
        if parts.len() != before {
            modified = true;
        }
        if parts.is_empty() {
            modified = true;
            false
        } else {
            true
        }
    });
    if modified {
        (
            serde_json::to_vec(&root).unwrap_or_else(|_| payload.to_vec()),
            report,
        )
    } else {
        (payload.to_vec(), report)
    }
}

fn strip_tool_signature_fields(part: &mut Value) -> bool {
    let Some(object) = part.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    for key in [
        "signature",
        "thoughtSignature",
        "thought_signature",
        "model",
    ] {
        changed |= object.remove(key).is_some();
    }
    if let Some(extra) = object
        .get_mut("extra_content")
        .and_then(Value::as_object_mut)
    {
        if let Some(google) = extra.get_mut("google").and_then(Value::as_object_mut) {
            changed |= google.remove("thought_signature").is_some();
        }
        if extra
            .get("google")
            .and_then(Value::as_object)
            .is_some_and(|google| google.is_empty())
        {
            extra.remove("google");
            changed = true;
        }
    }
    if object
        .get("extra_content")
        .and_then(Value::as_object)
        .is_some_and(|extra| extra.is_empty())
    {
        object.remove("extra_content");
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_upstream_drops_foreign_thinking_and_tool_provenance() {
        let input = br#"{"messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"x","signature":"gpt#encrypted"},{"type":"tool_use","id":"t","name":"run","input":{},"signature":"foreign","model":"gemini","extra_content":{"google":{"thought_signature":"foreign"}}},{"type":"text","text":"ok"}]}]}"#;
        let (output, report) = sanitize_claude_messages_for_claude_upstream(input, false);
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["messages"][0]["content"].as_array().unwrap().len(), 2);
        assert!(value["messages"][0]["content"][0]
            .get("signature")
            .is_none());
        assert!(value["messages"][0]["content"][0].get("model").is_none());
        assert_eq!(report.dropped_blocks, 1);
        assert_eq!(report.dropped_signatures, 1);
    }

    #[test]
    fn no_signature_history_is_byte_identical() {
        let input = br#"{ "messages": [{"role":"user","content":[{"type":"text","text":"x"}]}] }"#;
        assert_eq!(
            sanitize_claude_messages_for_claude_upstream(input, false).0,
            input
        );
    }

    #[test]
    fn preserve_empty_thinking_blocks_keeps_opaque_signatures() {
        let input = br#"{"messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"","signature":"opaque-signature"}]}]}"#;
        let (output, report) = sanitize_claude_messages_for_claude_upstream(input, true);
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            value["messages"][0]["content"][0]["signature"],
            "opaque-signature"
        );
        assert_eq!(report.preserved, 1);
        assert_eq!(report.dropped_blocks, 0);
    }
}
