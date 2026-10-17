// ref: internal/runtime/executor/helps/claude_builtin_tools_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;

use super::claude_builtin_tools::{
    augment_claude_builtin_tool_registry, is_claude_server_tool_type,
};

#[test]
fn only_known_typed_builtins_from_body_augment_the_registry() {
    let registry = augment_claude_builtin_tool_registry(
        br#"{
            "tools": [
                {"type": "web_search_20250305", "name": "web_search"},
                {"type": "custom", "name": "client_custom"},
                {"type": "custom_builtin_20250401", "name": "unknown_typed"},
                {"name": "Read"}
            ]
        }"#,
        None,
    );

    assert_eq!(registry.get("web_search"), Some(&true));
    for name in ["client_custom", "unknown_typed", "Read"] {
        assert!(!registry.contains_key(name));
    }
}

#[test]
fn supplied_registry_is_not_seeded_and_invalid_shapes_are_noops() {
    let mut supplied = HashMap::from([("existing".to_owned(), false)]);
    let unchanged = augment_claude_builtin_tool_registry(b"not-json", Some(supplied.clone()));
    assert_eq!(unchanged, supplied);

    supplied = augment_claude_builtin_tool_registry(
        br#"{"tools":{"type":"builtin","name":"ignored"}}"#,
        Some(supplied),
    );
    assert_eq!(supplied, HashMap::from([("existing".to_owned(), false)]));
    assert!(!supplied.contains_key("web_search"));
}

#[test]
fn empty_type_or_name_is_ignored_without_overwriting_other_entries() {
    let supplied = HashMap::from([("existing".to_owned(), false)]);
    let registry = augment_claude_builtin_tool_registry(
        br#"{"tools":[{"type":"","name":"empty-type"},{"type":"builtin","name":""},{"type":"web_search_20250305","name":"existing"}]}"#,
        Some(supplied),
    );
    assert!(!registry.contains_key("empty-type"));
    assert!(!registry.contains_key(""));
    assert_eq!(registry.get("existing"), Some(&true));
}
