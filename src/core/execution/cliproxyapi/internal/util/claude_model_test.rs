// ref: internal/util/claude_model_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::claude_model::{is_claude_model, is_claude_thinking_model};

#[test]
fn pinned_claude_thinking_model_cases_match_upstream() {
    for (model, expected) in [
        ("claude-sonnet-4-5-thinking", true),
        ("claude-opus-4-5-thinking", true),
        ("claude-opus-4-6-thinking", true),
        ("Claude-Sonnet-4-5-Thinking", true),
        ("Claude-THINKING-Model", true),
        ("claude-sonnet-4-5", false),
        ("claude-opus-4-5", false),
        ("claude-3-5-sonnet-20240620", false),
        ("gemini-3-pro-preview", false),
        ("gemini-3-pro-thinking", false),
        ("gpt-4o", false),
        ("", false),
        ("thinking-model", false),
        ("claude-model", false),
    ] {
        assert_eq!(is_claude_thinking_model(model), expected, "model={model}");
    }
}

#[test]
fn heuristic_is_substring_based_like_upstream() {
    assert!(is_claude_thinking_model("preCLAUDE-mid-unthinking-post"));
    assert!(!is_claude_thinking_model("claud-thinking"));
}

#[test]
fn claude_model_heuristic_matches_upstream() {
    for (model, expected) in [
        ("claude-sonnet-4-6", true),
        ("claude-opus-4-6-thinking", true),
        ("Claude-Sonnet-4-5", true),
        ("gemini-3-pro-preview", false),
        ("", false),
    ] {
        assert_eq!(is_claude_model(model), expected, "model={model}");
    }
}
