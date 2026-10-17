// ref: internal/translator/openai/gemini/openai_gemini_response_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

use super::{
    convert_openai_response_to_gemini_non_stream, convert_openai_response_to_gemini_stream,
    gemini_token_count, OpenAiToGeminiState,
};

fn decode(raw: &[u8]) -> Value {
    serde_json::from_slice(raw).expect("translator emits JSON")
}

#[test]
fn non_stream_preserves_tool_call_id() {
    let raw = br#"{"choices":[{"index":0,"message":{"role":"assistant","tool_calls":[{"id":"call_chat_1","type":"function","function":{"name":"lookup","arguments":"{\"q\":\"x\"}"}}]}}]}"#;
    let output = decode(&convert_openai_response_to_gemini_non_stream(raw));
    assert_eq!(
        output["candidates"][0]["content"]["parts"][0]["functionCall"]["id"],
        "call_chat_1"
    );
    assert_eq!(
        output["candidates"][0]["content"]["parts"][0]["functionCall"]["args"]["q"],
        "x"
    );
}

#[test]
fn stream_preserves_tool_call_id_across_fragments() {
    let mut state = OpenAiToGeminiState::default();
    assert!(convert_openai_response_to_gemini_stream(
        br#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_stream_1","type":"function","function":{"name":"lookup","arguments":"{\"q\":"}}]}}]}"#,
        &mut state,
    )
    .is_empty());
    assert!(convert_openai_response_to_gemini_stream(
        br#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"x\"}"}}]}}]}"#,
        &mut state,
    )
    .is_empty());
    let output = convert_openai_response_to_gemini_stream(
        br#"{"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#,
        &mut state,
    );
    let output = decode(output.last().expect("terminal response"));
    let call = &output["candidates"][0]["content"]["parts"][0]["functionCall"];
    assert_eq!(call["id"], "call_stream_1");
    assert_eq!(call["name"], "lookup");
    assert_eq!(call["args"]["q"], "x");
}

#[test]
fn emits_reasoning_then_content_and_accumulates_visible_text() {
    let mut state = OpenAiToGeminiState::default();
    let output = convert_openai_response_to_gemini_stream(
        br#"data: {"model":"gpt","choices":[{"delta":{"reasoning_content":[{"text":"think"}],"content":"answer"}}]}"#,
        &mut state,
    );
    assert_eq!(output.len(), 2);
    assert_eq!(
        decode(&output[0])["candidates"][0]["content"]["parts"][0],
        json!({"thought":true,"text":"think"})
    );
    assert_eq!(
        decode(&output[1])["candidates"][0]["content"]["parts"][0],
        json!({"text":"answer"})
    );
    assert_eq!(state.accumulated_content(), "answer");
}

#[test]
fn emits_usage_only_chunk_and_maps_aliases_and_details() {
    let mut state = OpenAiToGeminiState::default();
    let output = convert_openai_response_to_gemini_stream(
        br#"{"model":"gpt","choices":[],"usage":{"input_tokens":3,"output_tokens":4,"output_tokens_details":{"reasoning_tokens":2},"input_tokens_details":{"cached_tokens":1}}}"#,
        &mut state,
    );
    let output = decode(&output[0]);
    assert_eq!(output["candidates"], json!([]));
    assert_eq!(output["model"], "gpt");
    assert_eq!(output["usageMetadata"]["promptTokenCount"], 3);
    assert_eq!(output["usageMetadata"]["candidatesTokenCount"], 4);
    assert_eq!(output["usageMetadata"]["totalTokenCount"], 7);
    assert_eq!(output["usageMetadata"]["thoughtsTokenCount"], 2);
    assert_eq!(output["usageMetadata"]["cachedContentTokenCount"], 1);
}

#[test]
fn non_stream_orders_reasoning_text_tools_and_finish_reason() {
    let raw = r#"{
        "model":"gpt-test",
        "choices":[{"index":2,"finish_reason":"length","message":{
            "role":"assistant","reasoning_content":["r1",{"text":"r2"}],"content":"visible",
            "tool_calls":[{"id":"call-1","type":"function","function":{"name":"weather","arguments":"{\"location\": 北京, \"unit\": celsius}"}}]
        }}],
        "usage":{"prompt_tokens":5,"completion_tokens":6,"total_tokens":11,"completion_tokens_details":{"reasoning_tokens":2},"prompt_tokens_details":{"cached_tokens":3}}
    }"#;
    let output = decode(&convert_openai_response_to_gemini_non_stream(
        raw.as_bytes(),
    ));
    let candidate = &output["candidates"][0];
    assert_eq!(candidate["index"], 2);
    assert_eq!(candidate["finishReason"], "MAX_TOKENS");
    assert_eq!(
        candidate["content"]["parts"][0],
        json!({"thought":true,"text":"r1"})
    );
    assert_eq!(
        candidate["content"]["parts"][1],
        json!({"thought":true,"text":"r2"})
    );
    assert_eq!(candidate["content"]["parts"][2], json!({"text":"visible"}));
    assert_eq!(
        candidate["content"]["parts"][3]["functionCall"]["args"]["location"],
        "北京"
    );
    assert_eq!(
        candidate["content"]["parts"][3]["functionCall"]["args"]["unit"],
        "celsius"
    );
    assert_eq!(output["usageMetadata"]["totalTokenCount"], 11);
    assert_eq!(output["usageMetadata"]["thoughtsTokenCount"], 2);
    assert_eq!(output["usageMetadata"]["cachedContentTokenCount"], 3);
}

#[test]
fn ignores_done_invalid_json_and_non_function_stream_tools() {
    let mut state = OpenAiToGeminiState::default();
    assert!(convert_openai_response_to_gemini_stream(b" [DONE] \n", &mut state).is_empty());
    assert!(convert_openai_response_to_gemini_stream(b"not-json", &mut state).is_empty());
    assert!(convert_openai_response_to_gemini_stream(
        br#"{"choices":[{"delta":{"tool_calls":[{"index":0,"type":"custom","function":{"name":"x"}}]}}]}"#,
        &mut state,
    )
    .is_empty());
    let output = convert_openai_response_to_gemini_stream(
        br#"{"choices":[{"delta":{},"finish_reason":"content_filter"}]}"#,
        &mut state,
    );
    assert_eq!(
        decode(&output[0])["candidates"][0]["finishReason"],
        "SAFETY"
    );
}

#[test]
fn non_stream_preserves_upstream_last_choice_overwrite_behavior() {
    let output = decode(&convert_openai_response_to_gemini_non_stream(
        br#"{"choices":[
            {"index":0,"message":{"content":"first","tool_calls":[{"type":"function","function":{"name":"one","arguments":"{}"}}]}},
            {"index":1,"message":{"content":"second"}}
        ]}"#,
    ));
    let parts = output["candidates"][0]["content"]["parts"]
        .as_array()
        .expect("parts");
    assert_eq!(output["candidates"][0]["index"], 1);
    assert_eq!(parts[0], json!({"text":"second"}));
    assert_eq!(parts[1]["functionCall"]["name"], "one");
}
