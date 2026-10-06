// ref: internal/translator/codex/openai/responses/codex_openai-responses_request_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

use super::convert_openai_responses_request_to_codex;

fn convert(input: &[u8]) -> Value {
    serde_json::from_slice(&convert_openai_responses_request_to_codex(
        "gpt-5.6", input, true,
    ))
    .unwrap()
}

#[test]
fn converts_multiple_system_roles_and_preserves_other_roles() {
    let output = convert(
        br#"{"input":[{"role":"system"},{"role":"system"},{"role":"user"},{"role":"assistant"}]}"#,
    );
    assert_eq!(output["input"][0]["role"], "developer");
    assert_eq!(output["input"][1]["role"], "developer");
    assert_eq!(output["input"][2]["role"], "user");
    assert_eq!(output["input"][3]["role"], "assistant");
}

#[test]
fn enforces_required_fields_and_removes_rejected_fields() {
    let output = convert(br#"{"stream":"true","store":true,"parallel_tool_calls":false,"include":["x"],"max_output_tokens":1,"max_completion_tokens":2,"temperature":0.1,"top_p":0.9,"service_tier":"standard","truncation":"auto","context_management":[],"user":"u","input":[]}"#);
    assert_eq!(output["stream"], true);
    assert_eq!(output["store"], false);
    assert_eq!(output["parallel_tool_calls"], true);
    assert_eq!(output["include"], json!(["reasoning.encrypted_content"]));
    for key in [
        "max_output_tokens",
        "max_completion_tokens",
        "temperature",
        "top_p",
        "service_tier",
        "truncation",
        "context_management",
        "user",
    ] {
        assert!(output.get(key).is_none(), "{key} remained");
    }
}

#[test]
fn normalizes_all_web_search_alias_locations() {
    let output = convert(br#"{"input":"search","tools":[{"type":"web_search_preview_2025_03_11"}],"tool_choice":{"type":"allowed_tools","tools":[{"type":"web_search_preview"}]}}"#);
    assert_eq!(output["tools"][0]["type"], "web_search");
    assert_eq!(output["tool_choice"]["type"], "allowed_tools");
    assert_eq!(output["tool_choice"]["tools"][0]["type"], "web_search");
}

#[test]
fn preserves_requested_web_search_sources_and_normalized_bytes() {
    for include in [
        json!(["web_search_call.action.sources"]),
        json!([
            "discarded",
            "web_search_call.action.sources",
            "reasoning.encrypted_content",
            "web_search_call.action.sources"
        ]),
    ] {
        let output = convert(&serde_json::to_vec(&json!({"include":include})).unwrap());
        assert_eq!(
            output["include"],
            json!([
                "reasoning.encrypted_content",
                "web_search_call.action.sources"
            ])
        );
    }
    let input = br#" {"stream":true,"store":false,"parallel_tool_calls":true,"include":["reasoning.encrypted_content","web_search_call.action.sources"],"input":[]} "#;
    assert_eq!(
        convert_openai_responses_request_to_codex("gpt-5.6", input, true),
        input
    );
}

#[test]
fn normalized_request_preserves_bytes() {
    let input = br#" {"stream":true,"store":false,"parallel_tool_calls":true,"include":["reasoning.encrypted_content"],"service_tier":"priority","input":[]} "#;
    assert_eq!(
        convert_openai_responses_request_to_codex("gpt-5.6", input, true),
        input
    );
}
