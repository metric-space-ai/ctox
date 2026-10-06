// ref: test/summary_intent_translation_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

use crate::internal::translator::register_all;
use crate::sdk::translator::{
    claude, codex, gemini, openai, openai_response, Format, Registry, TranslationContext,
};

struct Case {
    from: Format,
    to: Format,
    model: &'static str,
    body: &'static [u8],
    pointer: &'static str,
    expected: Option<Value>,
}

#[test]
fn summary_intent_translation_uses_registered_translators() {
    let registry = Registry::new();
    register_all(&registry);
    let cases = [
        Case { from: openai(), to: claude(), model: "claude-opus-5", body: br#"{"model":"claude-opus-5","reasoning_effort":"high","messages":[{"role":"user","content":"hi"}]}"#, pointer: "/thinking/display", expected: None },
        Case { from: openai(), to: claude(), model: "claude-opus-5", body: br#"{"model":"claude-opus-5","reasoning_effort":"none","messages":[{"role":"user","content":"hi"}]}"#, pointer: "/thinking/display", expected: None },
        Case { from: claude(), to: codex(), model: "gpt-5.4", body: br#"{"model":"gpt-5.4","max_tokens":1024,"thinking":{"type":"adaptive","display":"summarized"},"messages":[{"role":"user","content":"hi"}]}"#, pointer: "/reasoning/summary", expected: Some(json!("auto")) },
        Case { from: openai_response(), to: claude(), model: "claude-opus-5", body: br#"{"model":"claude-opus-5","reasoning":{"effort":"high","summary":"auto"},"input":"hi"}"#, pointer: "/thinking/display", expected: Some(json!("summarized")) },
        Case { from: openai_response(), to: claude(), model: "claude-opus-5", body: br#"{"model":"claude-opus-5","reasoning":{"effort":"high","summary":null},"input":"hi"}"#, pointer: "/thinking/display", expected: Some(json!("omitted")) },
        Case { from: openai(), to: gemini(), model: "gemini-3.1-pro-preview", body: br#"{"model":"gemini-3.1-pro-preview","reasoning_effort":"high","messages":[{"role":"user","content":"hi"}]}"#, pointer: "/generationConfig/thinkingConfig/includeThoughts", expected: Some(json!(true)) },
        Case { from: openai(), to: gemini(), model: "gemini-3.1-pro-preview", body: br#"{"model":"gemini-3.1-pro-preview","reasoning_effort":"high","extra_body":{"google":{"thinking_config":{"include_thoughts":false}}},"messages":[{"role":"user","content":"hi"}]}"#, pointer: "/generationConfig/thinkingConfig/includeThoughts", expected: Some(json!(false)) },
    ];
    for case in cases {
        let output = registry.translate_request(
            &TranslationContext::default(),
            &case.from,
            &case.to,
            case.model,
            case.body,
            false,
        );
        let output: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            output.pointer(case.pointer).cloned(),
            case.expected,
            "body={output}"
        );
        if case.to == claude() && output.pointer("/thinking/type") == Some(&json!("disabled")) {
            assert!(
                output.pointer("/thinking/display").is_none(),
                "body={output}"
            );
        }
    }
}

#[test]
fn native_claude_without_display_preserves_signature_history() {
    let registry = Registry::new();
    register_all(&registry);
    let body = br#"{"model":"claude-opus-5","thinking":{"type":"adaptive"},"messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"","signature":"opus-signature"}]},{"role":"user","content":"continue"}]}"#;
    let output = registry.translate_request(
        &TranslationContext::default(),
        &claude(),
        &claude(),
        "claude-opus-5",
        body,
        true,
    );
    let input: Value = serde_json::from_slice(body).unwrap();
    let output: Value = serde_json::from_slice(&output).unwrap();
    assert!(output.pointer("/thinking/display").is_none());
    assert_eq!(output["messages"], input["messages"]);
}
