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

fn fixture_model() -> String {
    let c = crate::internal::registry::embedded_models_catalog().unwrap();
    crate::internal::registry::models_for_channel(&c, "claude")
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .id
}
fn converted(root: &Value) -> Value {
    serde_json::from_slice(&super::convert_openai_responses_request_to_claude(
        &fixture_model(),
        &serde_json::to_vec(root).unwrap(),
        false,
    ))
    .unwrap()
}
fn request() -> Value {
    json!({"input":"Hi","tools":[{"type":"function","name":"exec_command","parameters":{"type":"object"}},{"type":"tool_search","execution":"client"},{"type":"function","name":"calendar_create","defer_loading":true}]})
}
#[test]
fn tool_search_translator_first_discovery_appends_without_changing_cached_tools() {
    let first = converted(&request());
    let mut root = request();
    root["input"] = json!([{"type":"message","role":"user","content":"Hi"},{"type":"tool_search_call","execution":"client","call_id":"fixture-search","arguments":{"query":"calendar"}},{"type":"tool_search_output","execution":"client","call_id":"fixture-search","tools":[{"type":"function","name":"calendar_create","parameters":{"type":"object"},"cache_control":{"type":"ephemeral"}}]}]);
    let second = converted(&root);
    let before = first["tools"].as_array().unwrap();
    let after = second["tools"].as_array().unwrap();
    assert_eq!(
        before
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["exec_command", "tool_search", ANCHOR]
    );
    assert_eq!(&after[..before.len()], before);
    assert_eq!(after.last().unwrap()["name"], "calendar_create");
    assert_eq!(after.last().unwrap()["defer_loading"], true);
    assert!(after.last().unwrap().get("cache_control").is_none());
    let text = serde_json::to_string(&second["messages"]).unwrap();
    assert!(text.contains("tool_reference"));
    assert!(text.contains("calendar_create"));
    assert!(!text.contains(ANCHOR));
}
#[test]
fn tool_search_translator_anchor_yields_to_a_normalized_caller_name() {
    let mut root = request();
    root["tools"].as_array_mut().unwrap().push(
        json!({"type":"function","name":"tool.search.deferred.anchor","description":"Caller tool"}),
    );
    let out = converted(&root);
    let matching = out["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["name"] == ANCHOR)
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0]["description"], "Caller tool");
}
fn response_events(args: &str) -> Vec<Value> {
    vec![
        json!({"type":"message_start","message":{"id":"fixture-message","model":fixture_model()}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"fixture-search","name":"tool_search","input":{}}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":args}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}),
        json!({"type":"message_stop"}),
    ]
}
fn event_json(chunk: &[u8]) -> Value {
    let payload = chunk
        .splitn(2, |b| *b == b'\n')
        .nth(1)
        .unwrap()
        .strip_prefix(b"data: ")
        .unwrap();
    serde_json::from_slice(payload).unwrap()
}
#[test]
fn tool_search_response_unary_and_stream_preserve_client_dispatch_shape() {
    let req = serde_json::to_vec(&request()).unwrap();
    let events = response_events(r#"{"query":"calendar"}"#);
    let raw = events
        .iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect::<String>();
    let unary: Value = serde_json::from_slice(
        &super::convert_claude_response_to_openai_responses_non_stream(&req, b"{}", raw.as_bytes()),
    )
    .unwrap();
    assert_eq!(unary["output"][0]["type"], "tool_search_call");
    assert_eq!(unary["output"][0]["arguments"], json!({"query":"calendar"}));
    let mut state = super::ClaudeToResponsesState::default();
    let output = events
        .iter()
        .flat_map(|v| {
            super::convert_claude_response_to_openai_responses(
                &fixture_model(),
                &req,
                b"{}",
                &serde_json::to_vec(v).unwrap(),
                &mut state,
            )
        })
        .map(|v| event_json(&v))
        .collect::<Vec<_>>();
    assert!(!output.iter().any(|v| v["type"]
        .as_str()
        .unwrap()
        .contains("function_call_arguments")));
    let done = output
        .iter()
        .find(|v| v["type"] == "response.output_item.done")
        .unwrap();
    assert_eq!(done["item"]["type"], "tool_search_call");
    assert_eq!(done["item"]["execution"], "client");
    assert_eq!(done["item"]["arguments"], json!({"query":"calendar"}));
    assert_eq!(
        output.last().unwrap()["response"]["output"][0],
        done["item"]
    );
}
#[test]
fn tool_search_response_truncated_arguments_remain_incomplete_at_done() {
    let req = serde_json::to_vec(&request()).unwrap();
    let mut state = super::ClaudeToResponsesState::default();
    let output = response_events("{\"query\":")
        .iter()
        .flat_map(|v| {
            super::convert_claude_response_to_openai_responses(
                &fixture_model(),
                &req,
                b"{}",
                &serde_json::to_vec(v).unwrap(),
                &mut state,
            )
        })
        .map(|v| event_json(&v))
        .collect::<Vec<_>>();
    assert_eq!(
        output.last().unwrap()["response"]["output"][0]["status"],
        "incomplete"
    );
}

#[test]
fn tool_search_discovered_namespace_and_custom_calls_keep_their_caller_identity() {
    let mut root = request();
    root["input"] = json!([{"type":"message","role":"user","content":"Hi"},{"type":"tool_search_call","execution":"client","call_id":"fixture-search","arguments":"{\"query\":\"calendar\"}"},{"type":"tool_search_output","execution":"client","call_id":"fixture-search","tools":[{"type":"namespace","name":"calendar","tools":[{"type":"function","name":"lookup","parameters":{"type":"object"}}]},{"type":"custom","name":"write_patch"}]}]);
    let converted = converted(&root);
    let tools = converted["tools"].as_array().unwrap();
    assert!(tools
        .iter()
        .any(|v| v["name"] == "calendar__lookup" && v["defer_loading"] == true));
    assert!(tools
        .iter()
        .any(|v| v["name"] == "write_patch" && v["defer_loading"] == true));
    let events = vec![
        json!({"type":"message_start","message":{"id":"fixture-message","model":fixture_model()}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"fixture-call","name":"calendar__lookup","input":{}}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{}"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_stop"}),
    ];
    let req = serde_json::to_vec(&root).unwrap();
    let raw = events
        .iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect::<String>();
    let out: Value = serde_json::from_slice(
        &super::convert_claude_response_to_openai_responses_non_stream(&req, b"{}", raw.as_bytes()),
    )
    .unwrap();
    assert_eq!(out["output"][0]["name"], "lookup");
    assert_eq!(out["output"][0]["namespace"], "calendar");
}
#[test]
fn tool_search_custom_stream_emits_custom_items_and_input_events() {
    let mut root = request();
    root["input"] = json!([{"type":"tool_search_output","execution":"client","call_id":"fixture-search","tools":[{"type":"custom","name":"write_patch"}]}]);
    let req = serde_json::to_vec(&root).unwrap();
    let mut events = response_events("{}");
    events[1]["content_block"]["name"] = json!("write_patch");
    events.remove(2);
    let fragments = ["{\"input\":\"he", "llo\\n", "world\"}"];
    for (i, fragment) in fragments.iter().enumerate() {
        events.insert(2+i,json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":fragment}}));
    }
    let mut state = super::ClaudeToResponsesState::default();
    let out = events
        .iter()
        .flat_map(|v| {
            super::convert_claude_response_to_openai_responses(
                &fixture_model(),
                &req,
                b"{}",
                &serde_json::to_vec(v).unwrap(),
                &mut state,
            )
        })
        .map(|v| event_json(&v))
        .collect::<Vec<_>>();
    assert!(!out.iter().any(|v| v["type"]
        .as_str()
        .unwrap()
        .contains("function_call_arguments")));
    let delta = out
        .iter()
        .filter(|v| v["type"] == "response.custom_tool_call_input.delta")
        .filter_map(|v| v["delta"].as_str())
        .collect::<String>();
    assert_eq!(delta, "hello\nworld");
    let done = out
        .iter()
        .find(|v| v["type"] == "response.output_item.done")
        .unwrap();
    assert_eq!(done["item"]["type"], "custom_tool_call");
    assert_eq!(done["item"]["input"], "hello\nworld");
    assert_eq!(out.last().unwrap()["response"]["output"][0], done["item"]);
    let raw = events
        .iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect::<String>();
    let unary: Value = serde_json::from_slice(
        &super::convert_claude_response_to_openai_responses_non_stream(&req, b"{}", raw.as_bytes()),
    )
    .unwrap();
    assert_eq!(unary["output"][0]["type"], "custom_tool_call");
    assert_eq!(unary["output"][0]["input"], "hello\nworld");
}
#[test]
fn tool_search_orphan_and_duplicate_outputs_never_forge_an_executed_search() {
    let mut root = request();
    root["input"] = json!([{"type":"tool_search_output","execution":"client","call_id":"missing","tools":[{"type":"function","name":"lookup"}]},{"type":"tool_search_output","execution":"client","call_id":"missing","tools":[{"type":"function","name":"lookup"}]}]);
    let out = converted(&root);
    let text = serde_json::to_string(&out["messages"]).unwrap();
    assert!(!text.contains("tool_result"));
    assert!(!text.contains("tool_use"));
    assert!(text.contains("Tools now available"));
    assert_eq!(out["messages"].as_array().unwrap().len(), 1);
}

#[test]
fn tool_search_named_choice_uses_the_same_collision_safe_name_as_its_definition() {
    let mut root = request();
    root["tools"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"type":"function","name":"tool.search"}));
    root["tool_choice"] = json!({"type":"function","name":"tool.search"});
    let out = converted(&root);
    let mapped = ToolSearchPlan::new(&root).name("tool.search");
    assert_ne!(mapped, SEARCH);
    assert_eq!(out["tool_choice"]["name"], mapped);
    assert!(out["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["name"] == mapped));
    let schema = search_schema(&json!({"parameters":"malformed"}));
    assert_eq!(schema["properties"]["limit"]["type"], "number");
    assert_eq!(schema["additionalProperties"], false);
}
#[test]
fn tool_search_custom_unicode_fragments_never_publish_a_corrupted_input() {
    let mut root = request();
    root["input"] = json!([{"type":"tool_search_output","execution":"client","tools":[{"type":"custom","name":"write_patch"}]}]);
    let req = serde_json::to_vec(&root).unwrap();
    let mut events = response_events("{}");
    events[1]["content_block"]["name"] = json!("write_patch");
    events.remove(2);
    for (i, fragment) in ["{\"input\":\"A\\uD83", "D\\uDE", "00B\"}"]
        .iter()
        .enumerate()
    {
        events.insert(2+i,json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":fragment}}));
    }
    let mut state = super::ClaudeToResponsesState::default();
    let out = events
        .iter()
        .flat_map(|v| {
            super::convert_claude_response_to_openai_responses(
                &fixture_model(),
                &req,
                b"{}",
                &serde_json::to_vec(v).unwrap(),
                &mut state,
            )
        })
        .map(|v| event_json(&v))
        .collect::<Vec<_>>();
    let deltas = out
        .iter()
        .filter(|v| v["type"] == "response.custom_tool_call_input.delta")
        .filter_map(|v| v["delta"].as_str())
        .collect::<String>();
    assert_eq!(deltas, "A😀B");
    assert_eq!(
        out.last().unwrap()["response"]["output"][0]["input"],
        "A😀B"
    );
}
