// ref: internal/translator/codex/claude/codex_claude_request_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};

use super::convert_claude_request_to_codex;

#[test]
fn maps_system_reminders_tools_web_search_and_policy_fields() {
    let output: Value = serde_json::from_slice(&convert_claude_request_to_codex(
        "gpt-5.4",
        br#"{"system":[{"type":"text","text":"x-anthropic-billing-header: tenant"},{"type":"text","text":"rules"}],"messages":[{"role":"system","content":"repo rules"},{"role":"user","content":"search"}],"tools":[{"type":"web_search_20260209","name":"browser_search","allowed_domains":["example.com"],"user_location":{"city":"Berlin"}},{"name":"local","input_schema":{"type":"object"}}],"tool_choice":{"type":"tool","name":"browser_search","disable_parallel_tool_use":true},"speed":"fast"}"#,
        false,
    ))
    .unwrap();
    assert_eq!(output["input"][0]["role"], "developer");
    assert_eq!(output["input"][0]["content"][0]["text"], "rules");
    assert_eq!(output["input"][1]["role"], "user");
    assert_eq!(output["tools"][0]["type"], "web_search");
    assert_eq!(
        output["tools"][0]["filters"]["allowed_domains"][0],
        "example.com"
    );
    assert_eq!(output["tool_choice"]["type"], "web_search");
    assert_eq!(output["parallel_tool_calls"], false);
    assert_eq!(output["service_tier"], "priority");
    assert_eq!(output["reasoning"]["effort"], "medium");
}

#[test]
fn preserves_order_signature_and_shortens_matching_call_ids() {
    let signature = gpt_signature();
    let long_id = format!("toolu_{}", "a".repeat(70));
    let request = json!({
        "messages":[
            {"role":"assistant","content":[
                {"type":"text","text":"before"},
                {"type":"thinking","thinking":"do not replay","signature":signature},
                {"type":"tool_use","id":long_id,"name":"lookup","input":{"q":"x"}}
            ]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":long_id,"content":"ok"}]}
        ],
        "tools":[{"name":"lookup","input_schema":{"type":"object"}}]
    });
    let output: Value = serde_json::from_slice(&convert_claude_request_to_codex(
        "gpt-5.4",
        &serde_json::to_vec(&request).unwrap(),
        false,
    ))
    .unwrap();
    assert_eq!(output["input"][0]["type"], "message");
    assert_eq!(output["input"][1]["type"], "reasoning");
    assert_eq!(output["input"][1]["summary"], json!([]));
    assert_eq!(output["input"][1]["content"], Value::Null);
    assert_eq!(output["input"][2]["type"], "function_call");
    assert_eq!(output["input"][3]["type"], "function_call_output");
    let call_id = output["input"][2]["call_id"].as_str().unwrap();
    assert!(call_id.len() <= 64);
    assert_eq!(output["input"][3]["call_id"], call_id);
    assert!(!String::from_utf8(serde_json::to_vec(&output).unwrap())
        .unwrap()
        .contains("do not replay"));
}

#[test]
fn non_string_tool_name_is_normalized_without_panicking() {
    let output: Value = serde_json::from_slice(&convert_claude_request_to_codex(
        "gpt-5",
        br#"{"messages":[{"role":"user","content":"go"}],"tools":[{"name":42,"input_schema":{"type":"object"}}]}"#,
        false,
    ))
    .unwrap();
    assert_eq!(output["tools"][0]["name"], "42");
    assert_eq!(output["tools"][0]["parameters"]["properties"], json!({}));
}

#[test]
fn tool_parameters_drop_dialect_markers_and_unsupported_patterns() {
    let output: Value = serde_json::from_slice(&convert_claude_request_to_codex(
        "gpt-5.4",
        br#"{
            "messages":[{"role":"user","content":"go"}],
            "tools":[{"name":"lookup","input_schema":{
                "type":"object",
                "$schema":"draft",
                "$id":"schema",
                "properties":{
                    "field":{"type":"string","pattern":"\\p{L}+"},
                    "asset_id":{"type":"string","pattern":"^[0-9a-f]{32}$"},
                    "regex_config":{"default":{"pattern":"\\p{L}+"}}
                }
            }}]
        }"#,
        false,
    ))
    .unwrap();
    let parameters = &output["tools"][0]["parameters"];
    assert!(parameters.get("$schema").is_none());
    assert!(parameters.get("$id").is_none());
    assert!(parameters["properties"]["field"].get("pattern").is_none());
    assert_eq!(
        parameters["properties"]["asset_id"]["pattern"],
        "^[0-9a-f]{32}$"
    );
    assert_eq!(
        parameters["properties"]["regex_config"]["default"]["pattern"],
        r"\p{L}+"
    );
    assert!(parameters["properties"].is_object());
}

fn gpt_signature() -> String {
    let mut payload = vec![0_u8; 1 + 8 + 16 + 16 + 32];
    payload[0] = 0x80;
    payload[8] = 1;
    for (index, byte) in payload.iter_mut().enumerate().skip(9) {
        *byte = index as u8;
    }
    general_purpose::URL_SAFE.encode(payload)
}
