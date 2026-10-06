// ref: internal/client/codex/optimize-multi-agent-v2/optimize_multi_agent_v2.go:71-78,788-884 @ e2bff010
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{
    rewrite_multi_agent_input, rewrite_multi_agent_input_with_compat, MultiAgentV2Context,
};
use crate::internal::util::get_gjson_bytes_no_copy as get;

#[test]
fn candidate_input_compat_forces_messages_and_strips_internal_metadata_without_client_detection() {
    let body = br#"{ "kept":1.00,"input":[{"type":" agent_message ","author":"a","recipient":"b","internal_chat_message_metadata_passthrough":{"x":1},"content":[{"type":"encrypted_content","encrypted_content":"visible\ntext"}]},{"type":"message","role":"user","author":"c","recipient":"d","internal_chat_message_metadata_passthrough":true,"content":[]}]} "#;
    let context = MultiAgentV2Context::default();
    assert_eq!(rewrite_multi_agent_input(&context, body), body);
    let out = rewrite_multi_agent_input_with_compat(&context, body, true);
    assert!(std::str::from_utf8(&out)
        .unwrap()
        .starts_with(r#"{ "kept":1.00,"input":["#));
    assert_eq!(get(&out, "input.0.type").str(), "message");
    assert_eq!(get(&out, "input.0.role").str(), "user");
    assert_eq!(get(&out, "input.0.content.0.type").str(), "input_text");
    assert_eq!(get(&out, "input.0.content.0.text").str(), "visible\ntext");
    assert!(!get(&out, "input.0.content.0.encrypted_content").exists());
    for index in [0, 1] {
        for field in [
            "author",
            "recipient",
            "internal_chat_message_metadata_passthrough",
        ] {
            assert!(!get(&out, &format!("input.{index}.{field}")).exists());
        }
    }
}

#[test]
fn candidate_input_compat_keeps_normal_client_metadata_and_untouched_payloads() {
    let body = br#"{"input":[{"type":"agent_message","author":"a","recipient":"b","content":[{"type":"encrypted_content","encrypted_content":"x"}]}]}"#;
    let context = MultiAgentV2Context {
        enabled: true,
        user_agent: "codex-tui/1.2".into(),
        ..Default::default()
    };
    let out = rewrite_multi_agent_input(&context, body);
    assert_eq!(get(&out, "input.0.type").str(), "message");
    assert_eq!(get(&out, "input.0.author").str(), "a");
    assert_eq!(get(&out, "input.0.recipient").str(), "b");
    for untouched in [
        br#"{ "other":1.00 }"#.as_slice(),
        b"not json",
        br#"{"input":"x"}"#,
    ] {
        assert_eq!(
            rewrite_multi_agent_input_with_compat(&context, untouched, true),
            untouched
        );
    }
}

#[test]
fn candidate_input_compat_uses_first_duplicate_members_and_preserves_raw_values() {
    let body = br#"{ "input":[{"type":"agent_message","type":"function_call_output","author":"first","author":"second","number":1.00,"content":[]}],"input":[{"type":"message","text":"last"}],"kept":2.00 }"#;
    let out = rewrite_multi_agent_input_with_compat(&MultiAgentV2Context::default(), body, true);
    assert_eq!(get(&out, "input.0.type").str(), "message");
    assert_eq!(get(&out, "input.0.author").str(), "second");
    let text = std::str::from_utf8(&out).unwrap();
    assert!(text.contains(r#""number":1.00"#));
    assert!(text.contains(r#""type":"function_call_output""#));
    assert!(text.ends_with(r#"],"input":[{"type":"message","text":"last"}],"kept":2.00 }"#));
}
