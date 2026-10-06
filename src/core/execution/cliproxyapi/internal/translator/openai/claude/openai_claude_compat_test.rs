// ref: internal/translator/openai/claude/openai_claude_compat_test.go @ e2bff0107bb307337aaa19018ccddd55f64253d5
// Port-Status: candidate; execution pending
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{convert_claude_request_to_openai, convert_claude_request_to_openai_with_compat};
use serde_json::{json, Value};

#[test]
fn compatibility_preserves_unsigned_and_foreign_thinking_with_tools() {
    for stream in [false, true] {
        for signature in ["", "claude#opaque", "unknown-opaque-token"] {
            let request = serde_json::to_vec(&json!({"messages":[{
                "role":"assistant", "content":[
                    {"type":"thinking", "thinking":"reason", "signature":signature},
                    {"type":"text", "text":"Reading files."},
                    {"type":"tool_use", "id":"call_1", "name":"Read", "input":{"path":"main.go"}},
                ]
            }]}))
            .unwrap();
            let compatible: Value = serde_json::from_slice(
                &convert_claude_request_to_openai_with_compat("deepseek-v4", &request, stream),
            )
            .unwrap();
            assert_eq!(compatible["messages"][0]["reasoning_content"], "reason");
            assert_eq!(compatible["messages"][0]["tool_calls"][0]["id"], "call_1");
            assert_eq!(
                compatible["messages"][0]["tool_calls"][0]["function"]["name"],
                "Read"
            );
            let strict: Value = serde_json::from_slice(&convert_claude_request_to_openai(
                "deepseek-v4",
                &request,
                stream,
            ))
            .unwrap();
            assert!(strict["messages"][0].get("reasoning_content").is_none());
            assert_eq!(strict["messages"][0]["tool_calls"][0]["id"], "call_1");
        }
    }
}

#[test]
fn compatibility_does_not_fabricate_reasoning_for_tools_or_redacted_or_user_content() {
    for content in [
        json!([{"type":"tool_use", "id":"call_1", "name":"Read", "input":{}}]),
        json!([{"type":"redacted_thinking", "data":"opaque"}, {"type":"text", "text":"hello"}]),
    ] {
        let request =
            serde_json::to_vec(&json!({"messages":[{"role":"assistant", "content":content}]}))
                .unwrap();
        let output: Value = serde_json::from_slice(&convert_claude_request_to_openai_with_compat(
            "deepseek-v4",
            &request,
            false,
        ))
        .unwrap();
        assert!(output["messages"][0].get("reasoning_content").is_none());
    }
    let request = br#"{"messages":[{"role":"user","content":[{"type":"thinking","thinking":"reason","signature":""},{"type":"text","text":"hello"}]}]}"#;
    let output: Value = serde_json::from_slice(&convert_claude_request_to_openai_with_compat(
        "deepseek-v4",
        request,
        false,
    ))
    .unwrap();
    assert!(output["messages"][0].get("reasoning_content").is_none());
}

#[test]
fn compatibility_keeps_tool_result_adjacency_and_message_order() {
    let request = br#"{"messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"reason","signature":""},{"type":"tool_use","id":"call_1","name":"Read","input":{}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_1","content":"contents"},{"type":"text","text":"continue"}]}]}"#;
    let output: Value = serde_json::from_slice(&convert_claude_request_to_openai_with_compat(
        "deepseek-v4",
        request,
        true,
    ))
    .unwrap();
    assert_eq!(output["messages"][0]["role"], "assistant");
    assert_eq!(output["messages"][1]["role"], "tool");
    assert_eq!(output["messages"][1]["tool_call_id"], "call_1");
    assert_eq!(output["messages"][2]["role"], "user");
}

#[test]
fn compatibility_no_op_keeps_invalid_or_non_object_bytes_identical() {
    for payload in [b"  {bad-json}\n".as_slice(), b" [ 1, 2 ]\n".as_slice()] {
        for stream in [false, true] {
            assert_eq!(
                convert_claude_request_to_openai_with_compat("deepseek-v4", payload, stream),
                payload
            );
        }
    }
}
