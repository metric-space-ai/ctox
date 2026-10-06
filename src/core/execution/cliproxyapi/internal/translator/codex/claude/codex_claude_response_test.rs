// ref: internal/translator/codex/claude/codex_claude_response_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::{json, Value};

use crate::sdk::translator::TranslationContext;

use super::{
    claude_token_count, convert_codex_response_to_claude_non_stream,
    convert_codex_response_to_claude_stream, CodexToClaudeStreamState,
};

#[test]
fn aggregate_maps_text_thinking_tools_usage_and_web_search() {
    let output: Value = serde_json::from_slice(&convert_codex_response_to_claude_non_stream(
        &TranslationContext::default(),
        "gpt-5",
        b"{}",
        b"{}",
        br#"{"id":"resp","model":"gpt-5","output":[{"type":"reasoning","encrypted_content":"sig","summary":[{"type":"summary_text","text":"why"}]},{"type":"message","content":[{"type":"output_text","text":"answer"}]},{"type":"function_call","call_id":"c1","name":"Read","arguments":"{}"},{"type":"web_search_call","id":"ws1","action":{"query":"rust"},"results":[{"url":"https://example.com","title":"Example"}]}],"usage":{"input_tokens":3,"output_tokens":4,"input_tokens_details":{"cached_tokens":1}}}"#,
    ))
    .unwrap();
    assert_eq!(output["type"], "message");
    assert_eq!(output["usage"]["input_tokens"], 3);
    assert!(output["content"]
        .as_array()
        .unwrap()
        .iter()
        .any(|block| block["type"] == "tool_use"));
    assert!(output["content"]
        .as_array()
        .unwrap()
        .iter()
        .any(|block| block["type"] == "server_tool_use"));
}

#[test]
fn stream_maps_cyber_policy_error_and_web_search() {
    let context = TranslationContext::default();
    let mut state = CodexToClaudeStreamState::with_identity("msg_test");
    let error = convert_codex_response_to_claude_stream(
        &context,
        "gpt-5",
        b"{}",
        b"{}",
        br#"data: {"type":"error","error":{"type":"invalid_request","code":"cyber_policy","message":"flagged"}}"#,
        &mut state,
    );
    let error = String::from_utf8(error.concat()).unwrap();
    assert!(error.contains("invalid_request_error"));

    let web = convert_codex_response_to_claude_stream(
        &context,
        "gpt-5",
        b"{}",
        b"{}",
        br#"data: {"type":"response.output_item.done","item":{"type":"web_search_call","id":"ws1","action":{"query":"rust"},"results":[{"url":"https://example.com"}]}}"#,
        &mut state,
    );
    let web = String::from_utf8(web.concat()).unwrap();
    assert!(web.contains("server_tool_use"));
    assert!(web.contains("web_search_tool_result"));
}

#[test]
fn token_count_uses_claude_shape() {
    assert_eq!(claude_token_count(7), br#"{"input_tokens":7}"#);
}

fn v16_web_search_sources() -> Value {
    json!([
        {"type":"url","url":"https://docs.x.ai/developers/tools/web-search","title":"xAI Docs"},
        {"type":"url","url":"https://example.com/notitle"},
        {"type":"url","url":""},
        {"type":"url","url":"   "}
    ])
}

fn v16_expected_web_search_results() -> Value {
    json!([
        {"type":"web_search_result","url":"https://docs.x.ai/developers/tools/web-search","title":"xAI Docs","page_age":null},
        {"type":"web_search_result","url":"https://example.com/notitle","title":"https://example.com/notitle","page_age":null}
    ])
}

#[test]
fn candidate_v16_codex_web_search_non_stream_action_sources() {
    let raw = json!({
        "id":"resp_sources","model":"grok-4","output":[
            {"type":"web_search_call","id":"ws_sources","action":{
                "type":"search","query":"xAI web search docs","sources":v16_web_search_sources()
            }},
            {"type":"message","content":[{"type":"output_text","text":"here are the docs"}]}
        ]
    });
    let output: Value = serde_json::from_slice(&convert_codex_response_to_claude_non_stream(
        &TranslationContext::default(),
        "grok-4",
        b"{}",
        b"{}",
        &serde_json::to_vec(&raw).unwrap(),
    ))
    .unwrap();
    let blocks = output["content"].as_array().unwrap();
    let result = blocks
        .iter()
        .find(|block| block["type"] == "web_search_tool_result")
        .unwrap();
    assert_eq!(result["tool_use_id"], "ws_sources");
    assert_eq!(result["content"], v16_expected_web_search_results());
    assert!(blocks.iter().any(|block| block["type"] == "server_tool_use"
        && block["id"] == result["tool_use_id"]
        && block["input"]["query"] == "xAI web search docs"));
    assert!(blocks
        .iter()
        .any(|block| block["type"] == "text" && block["text"] == "here are the docs"));
}

#[test]
fn candidate_v16_codex_web_search_stream_action_sources() {
    for root_sources in [false, true] {
        let mut raw = json!({
            "type":"response.output_item.done",
            "item":{"type":"web_search_call","id":"ws_sources","status":"completed",
                "action":{"type":"search","query":"xAI web search docs"}}
        });
        if root_sources {
            raw["action"] = json!({"sources":v16_web_search_sources()});
        } else {
            raw["item"]["action"]["sources"] = v16_web_search_sources();
        }
        let mut state = CodexToClaudeStreamState::with_identity("msg_sources");
        let frames = convert_codex_response_to_claude_stream(
            &TranslationContext::default(),
            "grok-4",
            b"{}",
            b"{}",
            &serde_json::to_vec(&raw).unwrap(),
            &mut state,
        );
        let events: Vec<Value> = frames
            .iter()
            .flat_map(|frame| std::str::from_utf8(frame).unwrap().lines())
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let starts: Vec<&Value> = events
            .iter()
            .filter(|event| event["type"] == "content_block_start")
            .collect();
        let results: Vec<&Value> = starts
            .iter()
            .copied()
            .filter(|event| event["content_block"]["type"] == "web_search_tool_result")
            .collect();
        assert_eq!(results.len(), 1, "root sources: {root_sources}");
        let result = results[0];
        assert_eq!(result["content_block"]["tool_use_id"], "ws_sources");
        assert_eq!(
            result["content_block"]["content"],
            v16_expected_web_search_results()
        );
        let tool = starts
            .iter()
            .copied()
            .find(|event| event["content_block"]["type"] == "server_tool_use")
            .unwrap();
        assert_eq!(
            tool["content_block"]["id"],
            result["content_block"]["tool_use_id"]
        );
        assert_ne!(tool["index"], result["index"]);
        assert!(events.iter().any(
            |event| event["type"] == "content_block_stop" && event["index"] == result["index"]
        ));
    }
}

#[test]
fn candidate_v16_codex_web_search_preserves_results_precedence_and_empty_arrays() {
    let sources = |name: &str| json!([{"url":format!("https://{name}.example")}]);
    let cases = [
        (
            "item results",
            sources("item"),
            sources("root"),
            sources("action"),
            sources("root-action"),
            "https://item.example",
        ),
        (
            "non-array item results",
            json!({}),
            sources("root"),
            sources("action"),
            sources("root-action"),
            "https://root.example",
        ),
        (
            "item action sources",
            Value::Null,
            json!("not an array"),
            sources("action"),
            sources("root-action"),
            "https://action.example",
        ),
        (
            "root action sources",
            Value::Null,
            Value::Null,
            Value::Null,
            sources("root-action"),
            "https://root-action.example",
        ),
        (
            "empty item results",
            json!([]),
            sources("root"),
            sources("action"),
            sources("root-action"),
            "",
        ),
        (
            "empty root results",
            Value::Null,
            json!([]),
            sources("action"),
            sources("root-action"),
            "",
        ),
        (
            "empty action sources",
            Value::Null,
            Value::Null,
            json!([]),
            sources("root-action"),
            "",
        ),
    ];
    for (name, item_results, root_results, item_sources, root_sources, expected_url) in cases {
        let raw = json!({
            "id":"resp_precedence","model":"grok-4",
            "results":root_results,"action":{"sources":root_sources},
            "output":[{"type":"web_search_call","id":"ws_precedence",
                "results":item_results,
                "action":{"query":"sources precedence","sources":item_sources}}]
        });
        let output: Value = serde_json::from_slice(&convert_codex_response_to_claude_non_stream(
            &TranslationContext::default(),
            "grok-4",
            b"{}",
            b"{}",
            &serde_json::to_vec(&raw).unwrap(),
        ))
        .unwrap();
        let result = output["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|block| block["type"] == "web_search_tool_result")
            .unwrap();
        if expected_url.is_empty() {
            assert_eq!(result["content"], json!([]), "{name}");
        } else {
            assert_eq!(result["content"].as_array().unwrap().len(), 1, "{name}");
            assert_eq!(result["content"][0]["url"], expected_url, "{name}");
        }
    }
}
