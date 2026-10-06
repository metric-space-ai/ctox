// ref: test/thinking_conversion_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

use crate::internal::thinking::{ThinkingEngine, ThinkingRequest};

fn apply(body: &[u8], model: &str, from: &str, to: &str) -> Result<Value, String> {
    let output = ThinkingEngine::default()
        .apply_thinking(ThinkingRequest {
            body,
            model,
            from_format: from,
            to_format: to,
            provider_key: to,
        })
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&output).map_err(|error| error.to_string())
}

#[test]
fn thinking_suffixes_use_embedded_model_capabilities() {
    let codex = apply(
        br#"{"model":"gpt-5.4","input":"hi"}"#,
        "gpt-5.4(high)",
        "openai",
        "codex",
    )
    .unwrap();
    assert_eq!(codex.pointer("/reasoning/effort"), Some(&json!("high")));

    let gemini = apply(
        br#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#,
        "gemini-3.1-pro-preview(medium)",
        "openai",
        "gemini",
    )
    .unwrap();
    assert_eq!(
        gemini.pointer("/generationConfig/thinkingConfig/thinkingLevel"),
        Some(&json!("medium"))
    );

    let claude = apply(
        br#"{"messages":[{"role":"user","content":"hi"}]}"#,
        "claude-opus-5(high)",
        "openai",
        "claude",
    )
    .unwrap();
    assert_eq!(claude.pointer("/thinking/type"), Some(&json!("adaptive")));
    assert_eq!(
        claude.pointer("/output_config/effort"),
        Some(&json!("high"))
    );
}

#[test]
fn candidate_claude_effort_is_always_paired_with_adaptive_thinking() {
    for effort in ["low", "medium", "high", "max"] {
        let model = format!("claude-opus-5({effort})");
        let output = apply(
            br#"{"messages":[{"role":"user","content":"hi"}]}"#,
            &model,
            "openai",
            "claude",
        )
        .unwrap();

        assert_eq!(
            output.pointer("/output_config/effort"),
            Some(&json!(effort))
        );
        assert_eq!(output.pointer("/thinking/type"), Some(&json!("adaptive")));
    }
}

#[test]
fn unknown_suffix_is_ignored_without_corrupting_payload() {
    let output = ThinkingEngine::default()
        .apply_thinking(ThinkingRequest {
            body: br#"{"model":"gpt-5.4","input":"hi"}"#,
            model: "gpt-5.4(nonsense)",
            from_format: "openai",
            to_format: "codex",
            provider_key: "codex",
        })
        .unwrap();
    let output: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(output["input"], "hi");
    assert!(output.get("reasoning").is_none());
}

#[test]
fn antigravity_preserves_explicit_summary_visibility() {
    for (body, expected) in [
        (br#"{"request":{"generationConfig":{"thinkingConfig":{"includeThoughts":true}},"contents":[]}}"#.as_slice(), Some(true)),
        (br#"{"request":{"generationConfig":{"thinkingConfig":{"includeThoughts":false}},"contents":[]}}"#.as_slice(), Some(false)),
        (br#"{"request":{"contents":[]}}"#.as_slice(), None),
    ] {
        let output = apply(body, "gemini-3.1-pro-preview(medium)", "antigravity", "antigravity").unwrap();
        assert_eq!(output.pointer("/request/generationConfig/thinkingConfig/includeThoughts").and_then(Value::as_bool), expected);
        assert!(output.pointer("/request/generationConfig/thinkingConfig/include_thoughts").is_none());
    }
}
