// ref: internal/translator/claude/openai/responses/claude_openai-responses_tool_search_test.go @ 97ba1e896181b29b97d4ff1324a3af3f2a639c09
// Port-Status: candidate
// License: MIT (upstream); modifications AGPL-3.0-only
use super::tool_search::*;
use serde_json::{json, Value};
#[test]
fn tool_search_descriptor_priority_and_hidden_activation_follow_the_caller() {
    let root = json!({"tools":[{"type":"tool_search","execution":"client"},{"type":"function","name":"known"},{"type":"function","name":"later","defer_loading":true}],"input":[{"type":"tool_search_output","execution":"client","tools":[{"type":"function","name":"later"},{"type":"function","name":"known"}]}]});
    let p = ToolSearchPlan::new(&root);
    assert!(p.native);
    assert_eq!(
        p.descriptors
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        vec!["tool_search", "known", "later"]
    );
    assert_eq!(p.descriptors[1].priority, 0);
    assert_eq!(p.descriptors[2].priority, 2);
    let refs = p.output_content(&root["input"][0]);
    assert_eq!(refs, json!([{"type":"tool_reference","tool_name":"later"}]));
}
#[test]
fn tool_search_real_function_precedence_disables_native_deferral() {
    let p = ToolSearchPlan::new(
        &json!({"tools":[{"type":"function","name":"tool_search"},{"type":"tool_search","execution":"client"},{"type":"function","name":"later","defer_loading":true}]}),
    );
    assert!(p.enabled);
    assert!(!p.native);
    assert!(p.descriptors.iter().any(|d| d.name == "later"));
}
#[test]
fn tool_search_names_are_collision_safe_and_history_mapping_is_stable() {
    let root = json!({"tools":[{"type":"function","name":"tool.search"},{"type":"tool_search","execution":"client"},{"type":"function","name":"a.b"},{"type":"function","name":"a:b"}],"input":[{"type":"function_call","name":"historical.name"}]});
    let p = ToolSearchPlan::new(&root);
    assert_eq!(p.name(SEARCH), SEARCH);
    assert_ne!(p.name("tool.search"), SEARCH);
    assert_ne!(p.name("a.b"), p.name("a:b"));
    assert_eq!(p.name("historical.name"), "historical_name");
    assert_eq!(p.identity(&p.name("a.b")).0, "a.b");
}
#[test]
fn tool_search_query_schema_preserves_existing_fields_and_requires_a_string() {
    let s = search_schema(
        &json!({"parameters":{"type":"object","properties":{"query":{"type":"number"},"tag":{"type":"string"}},"required":["tag"]}}),
    );
    assert_eq!(s["properties"]["query"]["type"], "string");
    assert_eq!(s["required"], json!(["tag", "query"]));
    assert_eq!(s["properties"]["tag"]["type"], "string");
}
#[test]
fn tool_search_completed_calls_never_disguise_invalid_or_truncated_arguments() {
    for args in ["", "{", "[]", "null"] {
        let item = search_call_item("fixture-call", "completed", args);
        assert_eq!(item["status"], "incomplete");
        assert_eq!(item["arguments"], json!({}));
    }
    let item = search_call_item("fixture-call", "completed", r#"{"query":"calendar"}"#);
    assert_eq!(item["type"], "tool_search_call");
    assert_eq!(item["execution"], "client");
    assert_eq!(item["arguments"], json!({"query":"calendar"}));
    let root = ToolSearchPlan::new(&json!({"tools":[{"type":"tool_search","execution":"server"}]}));
    assert!(!root.enabled);
}
