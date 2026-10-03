// ref: internal/translator/claude/openai/chat-completions/claude_openai_compat_test.go @ e2bff0107bb307337aaa19018ccddd55f64253d5
// Port-Status: candidate; execution pending
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    convert_openai_chat_request_to_claude, convert_openai_chat_request_to_claude_with_compat,
};
use serde_json::{json, Value};

#[test]
fn compatibility_prepends_assistant_reasoning_before_text_and_tools() {
    let request = br#"{"messages":[{"role":"assistant","content":"answer","reasoning_content":"reason","tool_calls":[{"id":"call_1","type":"function","function":{"name":"Read","arguments":"{}"}}]}]}"#;
    for stream in [false, true] {
        let compatible: Value = serde_json::from_slice(
            &convert_openai_chat_request_to_claude_with_compat("deepseek-v4", request, stream),
        )
        .unwrap();
        assert_eq!(
            compatible["messages"][0]["content"][0],
            json!({"type":"thinking", "thinking":"reason", "signature":""})
        );
        assert_eq!(compatible["messages"][0]["content"][1]["text"], "answer");
        assert_eq!(compatible["messages"][0]["content"][2]["type"], "tool_use");
        let strict: Value = serde_json::from_slice(&convert_openai_chat_request_to_claude(
            "deepseek-v4",
            request,
            stream,
        ))
        .unwrap();
        assert!(strict["messages"][0]["content"]
            .as_array()
            .unwrap()
            .iter()
            .all(|part| part["type"] != "thinking"));
    }
}

#[test]
fn compatibility_ignores_empty_non_string_or_user_reasoning_content() {
    for (role, reasoning) in [
        ("assistant", json!(" ")),
        ("assistant", Value::Null),
        ("assistant", json!(123)),
        ("user", json!("reason")),
    ] {
        let request = serde_json::to_vec(
            &json!({"messages":[{"role":role, "content":"answer", "reasoning_content":reasoning}]}),
        )
        .unwrap();
        assert_eq!(
            convert_openai_chat_request_to_claude_with_compat("deepseek-v4", &request, false),
            convert_openai_chat_request_to_claude("deepseek-v4", &request, false)
        );
    }
}

#[test]
fn compatibility_no_op_keeps_invalid_or_non_object_bytes_identical() {
    for payload in [b"  {bad-json}\n".as_slice(), b" [ 1, 2 ]\n".as_slice()] {
        assert_eq!(
            convert_openai_chat_request_to_claude_with_compat("deepseek-v4", payload, false),
            payload
        );
    }
}
