// ref: internal/translator/claude/openai/responses/claude_openai_responses_compat_test.go @ e2bff0107bb307337aaa19018ccddd55f64253d5
// Port-Status: candidate; execution pending
// License: MIT (upstream); modifications AGPL-3.0-only

use super::response::CLAUDE_RESPONSES_REDACTED_THINKING_PREFIX;
use super::{
    convert_openai_responses_request_to_claude,
    convert_openai_responses_request_to_claude_with_compat,
};
use serde_json::{json, Value};

#[test]
fn compatibility_preserves_empty_missing_null_and_opaque_reasoning_signatures() {
    for (encrypted, expected) in [
        (None, ""),
        (Some(Value::Null), ""),
        (Some(json!("")), ""),
        (Some(json!("opaque-deepseek-id")), "opaque-deepseek-id"),
    ] {
        let mut item =
            json!({"type":"reasoning", "summary":[{"type":"summary_text", "text":"reason"}]});
        if let Some(encrypted) = encrypted {
            item["encrypted_content"] = encrypted;
        }
        let request = serde_json::to_vec(&json!({"input":[item]})).unwrap();
        for stream in [false, true] {
            let compatible: Value =
                serde_json::from_slice(&convert_openai_responses_request_to_claude_with_compat(
                    "deepseek-v4",
                    &request,
                    stream,
                ))
                .unwrap();
            assert_eq!(
                compatible["messages"][0]["content"][0],
                json!({
                    "type":"thinking", "thinking":"reason", "signature":expected,
                })
            );
            let strict: Value = serde_json::from_slice(
                &convert_openai_responses_request_to_claude("deepseek-v4", &request, stream),
            )
            .unwrap();
            assert_eq!(strict["messages"], json!([]));
        }
    }
}

#[test]
fn compatibility_preserves_redacted_payload_and_drops_empty_redacted_payload() {
    for data in ["payload", ""] {
        let request = serde_json::to_vec(&json!({"input":[{
            "type":"reasoning", "encrypted_content":format!("{CLAUDE_RESPONSES_REDACTED_THINKING_PREFIX}{data}"),
        }]})).unwrap();
        let compatible: Value = serde_json::from_slice(
            &convert_openai_responses_request_to_claude_with_compat("deepseek-v4", &request, false),
        )
        .unwrap();
        if data.is_empty() {
            assert_eq!(compatible["messages"], json!([]));
        } else {
            assert_eq!(
                compatible["messages"][0]["content"][0],
                json!({"type":"redacted_thinking", "data":"payload"})
            );
        }
    }
}

#[test]
fn compatibility_uses_summary_once_and_merges_thinking_before_assistant_text() {
    let request = br#"{"input":[{"type":"reasoning","encrypted_content":"opaque-id","summary":[{"type":"summary_text","text":"reason"}],"content":[{"type":"reasoning_text","text":"reason"}]},{"type":"message","role":"assistant","content":[{"type":"output_text","text":"answer"}]}]}"#;
    let compatible: Value = serde_json::from_slice(
        &convert_openai_responses_request_to_claude_with_compat("deepseek-v4", request, false),
    )
    .unwrap();
    assert_eq!(compatible["messages"].as_array().unwrap().len(), 1);
    assert_eq!(
        compatible["messages"][0]["content"][0]["thinking"],
        "reason"
    );
    assert_eq!(compatible["messages"][0]["content"][1]["text"], "answer");
}

#[test]
fn compatibility_no_op_keeps_invalid_bytes_identical() {
    let request = b"  {bad-json}\n";
    assert_eq!(
        convert_openai_responses_request_to_claude_with_compat("deepseek-v4", request, false),
        request
    );
}
