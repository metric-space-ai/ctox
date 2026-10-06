// ref: internal/util/claude_tool_id_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::claude_tool_id::{
    gemini_claude_tool_use_id, is_gemini_claude_tool_use_id, sanitize_claude_function_name,
    sanitize_claude_tool_id,
};

#[test]
fn stable_gemini_id_is_bound_to_native_call_name_and_canonical_args() {
    let first = gemini_claude_tool_use_id(
        "native-call-1",
        "Edit",
        r#"{"file_path":"/tmp/a","old_string":"x","new_string":"y"}"#,
    );
    let reordered = gemini_claude_tool_use_id(
        "native-call-1",
        "Edit",
        r#"{"new_string":"y","old_string":"x","file_path":"/tmp/a"}"#,
    );
    assert_eq!(first, reordered);
    assert!(is_gemini_claude_tool_use_id(&first));
    assert_ne!(
        first,
        gemini_claude_tool_use_id(
            "native-call-1",
            "Edit",
            r#"{"file_path":"/tmp/a","old_string":"x","new_string":"z"}"#,
        )
    );
    assert!(gemini_claude_tool_use_id("", "Edit", "{}").is_empty());
}

#[test]
fn claude_tool_ids_replace_only_non_protocol_characters() {
    assert_eq!(sanitize_claude_tool_id("tool/a:b c"), "tool_a_b_c");
    assert!(sanitize_claude_tool_id("")
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')));
    assert!(!is_gemini_claude_tool_use_id("toolu_client_value"));
}

#[test]
fn claude_function_names_match_upstream_sanitizer() {
    let cases = [
        ("valid_name", "valid_name"),
        ("name.with.dots", "name_with_dots"),
        ("name:with:colons", "name_with_colons"),
        ("name-with-dashes", "name-with-dashes"),
        ("mcp.server.special:get_time", "mcp_server_special_get_time"),
        ("server/action", "server_action"),
        ("name!with@invalid#chars", "name_with_invalid_chars"),
        ("name with spaces", "name_with_spaces"),
        ("name_with_你好_chars", "name_with____chars"),
        ("", ""),
        ("a", "a"),
        ("@", "_"),
        ("123name", "123name"),
        ("-name", "-name"),
        (
            "this_is_a_very_long_name_that_exactly_reaches_sixty_four_charact",
            "this_is_a_very_long_name_that_exactly_reaches_sixty_four_charact",
        ),
        (
            "this_is_a_very_long_name_that_exactly_reaches_sixty_four_charactX",
            "this_is_a_very_long_name_that_exactly_reaches_sixty_four_charact",
        ),
    ];
    for (input, expected) in cases {
        let got = sanitize_claude_function_name(input);
        assert_eq!(got, expected, "input={input}");
        assert!(got.len() <= 64, "input={input} len={}", got.len());
    }
}
