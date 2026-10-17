// ref: internal/thinking/{types,errors,text,suffix}.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::json;

use super::*;

#[test]
fn thinking_error_is_structured_but_displays_only_message() {
    let error = ThinkingError::with_model(
        ErrorCode::ThinkingNotSupported,
        "thinking not supported for this model",
        "claude-haiku",
    );
    assert_eq!(error.to_string(), "thinking not supported for this model");
    assert_eq!(error.code.as_str(), "THINKING_NOT_SUPPORTED");
    assert_eq!(error.model, "claude-haiku");
    assert!(error.details.is_none());
    assert_eq!(error.status_code(), 400);
}

#[test]
fn extracts_all_supported_thinking_text_shapes_in_precedence_order() {
    assert_eq!(get_thinking_text(&json!({"thinking": "simple"})), "simple");
    assert_eq!(
        get_thinking_text(&json!({"thinking": {"text": "wrapped"}})),
        "wrapped"
    );
    assert_eq!(
        get_thinking_text(&json!({"thinking": {"thinking": "nested"}})),
        "nested"
    );
    assert_eq!(
        get_thinking_text(&json!({"thought": true, "text": "gemini"})),
        "gemini"
    );
    assert_eq!(
        get_thinking_text(&json!({"text": "direct", "thinking": "fallback"})),
        "direct"
    );
    assert_eq!(get_thinking_text(&json!({"thinking": 17})), "");
}

#[test]
fn suffix_extraction_matches_last_open_parenthesis_behavior() {
    assert_eq!(
        parse_suffix("claude-sonnet-4-5(16384)"),
        SuffixResult {
            model_name: "claude-sonnet-4-5".into(),
            has_suffix: true,
            raw_suffix: "16384".into(),
        }
    );
    assert_eq!(
        parse_suffix("model(prefix)(high)").model_name,
        "model(prefix)"
    );
    assert_eq!(parse_suffix("model()").raw_suffix, "");
    assert!(!parse_suffix("model(high").has_suffix);
    assert!(!parse_suffix("model").has_suffix);
    assert_eq!(
        parse_suffix("model)"),
        SuffixResult {
            model_name: "model)".into(),
            ..SuffixResult::default()
        }
    );
}

#[test]
fn suffix_interpreters_keep_special_values_separate() {
    assert_eq!(parse_numeric_suffix("08192"), Some(8192));
    assert_eq!(parse_numeric_suffix("0"), Some(0));
    assert_eq!(parse_numeric_suffix("-1"), None);
    assert_eq!(parse_numeric_suffix("+1"), Some(1));
    assert_eq!(parse_numeric_suffix(" 1"), None);
    assert_eq!(parse_numeric_suffix("9223372036854775808"), None);

    assert_eq!(parse_special_suffix("NONE"), Some(ThinkingMode::None));
    assert_eq!(parse_special_suffix("Auto"), Some(ThinkingMode::Auto));
    assert_eq!(parse_special_suffix("-1"), Some(ThinkingMode::Auto));
    assert_eq!(parse_special_suffix(""), None);

    for level in ["minimal", "low", "medium", "high", "xhigh", "max"] {
        assert_eq!(
            parse_level_suffix(&level.to_ascii_uppercase())
                .as_ref()
                .map(ThinkingLevel::as_str),
            Some(level)
        );
    }
    assert_eq!(parse_level_suffix("none"), None);
    assert_eq!(parse_level_suffix("auto"), None);
    assert_eq!(parse_level_suffix("ultra"), None);
}
