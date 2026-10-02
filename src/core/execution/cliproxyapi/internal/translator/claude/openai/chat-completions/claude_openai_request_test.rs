// ref: internal/translator/claude/openai/chat-completions/claude_openai_request_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::convert_openai_chat_request_to_claude;
use serde_json::{json, Value};

fn convert(input: Value) -> Value {
    serde_json::from_slice(&convert_openai_chat_request_to_claude(
        "claude-sonnet-4-5",
        &serde_json::to_vec(&input).unwrap(),
        false,
    ))
    .unwrap()
}

#[test]
fn sanitizes_ids_and_groups_parallel_tool_results() {
    let output = convert(json!({"messages":[
        {"role":"assistant","tool_calls":[
            {"id":"call.with space:1","type":"function","function":{"name":"a","arguments":"{}"}},
            {"id":"call:2","type":"function","function":{"name":"b","arguments":"{}"}}
        ]},
        {"role":"tool","tool_call_id":"call.with space:1","content":"one","cache_control":{"type":"ephemeral"}},
        {"role":"tool","tool_call_id":"call:2","content":"two"}
    ]}));
    assert_eq!(
        output["messages"][0]["content"][0]["id"],
        "call_with_space_1"
    );
    assert_eq!(
        output["messages"][1]["content"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        output["messages"][1]["content"][0]["tool_use_id"],
        "call_with_space_1"
    );
    assert_eq!(
        output["messages"][1]["content"][0]["cache_control"]["type"],
        "ephemeral"
    );
}

#[test]
fn drops_temperature_but_preserves_top_p_and_media_results() {
    let output = convert(json!({
        "temperature":0.2,"top_p":0.8,
        "messages":[
            {"role":"assistant","tool_calls":[{"id":"call_1","type":"function","function":{"name":"work","arguments":"{}"}}]},
            {"role":"tool","tool_call_id":"call_1","content":[
                {"type":"text","text":"ok"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,AA=="}},
                {"type":"image_url","image_url":{"url":"https://example.test/a.png"}}
            ]}
        ]
    }));
    assert!(output.get("temperature").is_none());
    assert_eq!(output["top_p"], 0.8);
    assert_eq!(
        output["messages"][1]["content"][0]["content"][1]["source"]["data"],
        "AA=="
    );
    assert_eq!(
        output["messages"][1]["content"][0]["content"][2]["source"]["type"],
        "url"
    );
}

#[test]
fn lifts_and_merges_system_messages_and_keeps_system_only_fallback() {
    let output = convert(json!({"messages":[
        {"role":"system","content":"Rule 1"},
        {"role":"system","content":[{"type":"text","text":"Rule 2"}]}
    ]}));
    assert_eq!(output["system"][0]["text"], "Rule 1");
    assert_eq!(output["system"][1]["text"], "Rule 2");
    assert_eq!(
        output["messages"][0],
        json!({"role":"user","content":[{"type":"text","text":""}]})
    );
}

#[test]
fn preserves_part_message_and_tool_cache_control_with_part_precedence() {
    let output = convert(json!({
        "messages":[{"role":"user","cache_control":{"type":"ephemeral","ttl":"1h"},"content":[
            {"type":"text","text":"cached","cache_control":{"type":"ephemeral"}},
            {"type":"text","text":"fresh"}
        ]}],
        "tools":[{"type":"function","cache_control":{"type":"ephemeral"},"function":{
            "name":"lookup","parameters":{"type":"object","properties":{}}
        }}]
    }));
    assert_eq!(
        output["messages"][0]["content"][0]["cache_control"]["type"],
        "ephemeral"
    );
    assert!(output["messages"][0]["content"][0]["cache_control"]
        .get("ttl")
        .is_none());
    assert_eq!(
        output["messages"][0]["content"][1]["cache_control"]["ttl"],
        "1h"
    );
    assert_eq!(output["tools"][0]["cache_control"]["type"], "ephemeral");
}

#[test]
fn normalizes_root_schema_unions_without_merging_required() {
    let output = convert(json!({"messages":[{"role":"user","content":"hi"}],"tools":[
        {"type":"function","function":{"name":"without_type","parameters":{"anyOf":[
            {"type":"object","properties":{"a":{"type":"string"}}},
            {"type":"object","properties":{"b":{"type":"string"}}}
        ]}}},
        {"type":"function","function":{"name":"constraint_union","parametersJsonSchema":{
            "type":"object","properties":{"a":{"type":"string"},"b":{"type":"string"}},
            "anyOf":[{"required":["a"]},{"required":["b"]}]
        }}}
    ]}));
    for schema in [
        &output["tools"][0]["input_schema"],
        &output["tools"][1]["input_schema"],
    ] {
        assert_eq!(schema["type"], "object");
        assert!(schema.get("anyOf").is_none());
        assert!(schema["properties"].get("a").is_some());
        assert!(schema["properties"].get("b").is_some());
        assert!(schema.get("required").is_none());
    }
}

#[test]
fn developer_role_becomes_top_level_system_blocks() {
    let output = convert(json!({"messages":[
        {"role":"system","content":"S1"},
        {"role":"developer","content":[
            {"type":"text","text":"D1"},
            {"type":"text","text":"D2"}
        ]},
        {"role":"user","content":"Hello"}
    ]}));

    assert_eq!(output["system"].as_array().unwrap().len(), 3);
    assert_eq!(output["system"][0]["text"], "S1");
    assert_eq!(output["system"][1]["text"], "D1");
    assert_eq!(output["system"][2]["text"], "D2");
    assert_eq!(output["messages"].as_array().unwrap().len(), 1);
    assert_eq!(output["messages"][0]["role"], "user");
}

#[test]
fn developer_message_cache_control_applies_to_last_block() {
    let output = convert(json!({"messages":[
        {"role":"developer","content":[
            {"type":"text","text":"D1"},
            {"type":"text","text":"D2"}
        ],"cache_control":{"type":"ephemeral"}},
        {"role":"user","content":"Hello"}
    ]}));

    assert!(output["system"][0].get("cache_control").is_none());
    assert_eq!(output["system"][1]["cache_control"]["type"], "ephemeral");
}

#[test]
fn tool_result_cache_control_uses_part_level_then_message_level() {
    let output = convert(json!({
        "messages":[
            {"role":"assistant","tool_calls":[
                {"id":"call_1","type":"function","function":{"name":"a","arguments":"{}"}},
                {"id":"call_2","type":"function","function":{"name":"b","arguments":"{}"}}
            ]},
            {"role":"tool","tool_call_id":"call_1","content":[
                {"type":"text","text":"4","cache_control":{"type":"ephemeral"}}
            ],"cache_control":{"type":"ephemeral","ttl":"1h"}},
            {"role":"tool","tool_call_id":"call_2","content":"two","cache_control":{"type":"persistent"}}
        ]
    }));
    let blocks = output["messages"][1]["content"].as_array().unwrap();
    assert_eq!(blocks[0]["cache_control"]["type"], "ephemeral");
    assert!(blocks[0]["cache_control"].get("ttl").is_none());
    assert!(blocks[0]["content"][0].get("cache_control").is_none());
    assert!(blocks[1].get("cache_control").is_none());
}
