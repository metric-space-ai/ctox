// ref: internal/translator/claude/openai/responses/claude_openai-responses_response_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    convert_claude_response_to_openai_responses,
    convert_claude_response_to_openai_responses_non_stream, ClaudeToResponsesState,
};
use serde_json::{json, Value};

fn decode(chunk: &[u8]) -> Value {
    let payload = chunk
        .splitn(2, |byte| *byte == b'\n')
        .nth(1)
        .and_then(|line| line.strip_prefix(b"data: "))
        .expect("CTOX Responses events have event and data lines");
    serde_json::from_slice(payload).unwrap()
}

fn send(state: &mut ClaudeToResponsesState, request: &Value, event: Value) -> Vec<Value> {
    convert_claude_response_to_openai_responses(
        "claude-test",
        &serde_json::to_vec(request).unwrap(),
        b"{}",
        &serde_json::to_vec(&event).unwrap(),
        state,
    )
    .into_iter()
    .map(|chunk| decode(&chunk))
    .collect()
}

fn started(state: &mut ClaudeToResponsesState) {
    send(
        state,
        &json!({}),
        json!({"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":1}}}),
    );
}

#[test]
fn terminal_stop_reasons_keep_pause_and_limits_incomplete_in_both_transports() {
    for (reason, event_type, status, details) in [
        (
            "pause_turn",
            "response.incomplete",
            "incomplete",
            Value::Null,
        ),
        (
            " PAUSE_TURN ",
            "response.incomplete",
            "incomplete",
            Value::Null,
        ),
        (
            "max_tokens",
            "response.incomplete",
            "incomplete",
            json!({"reason":"max_output_tokens"}),
        ),
        (
            " MAX_TOKENS ",
            "response.incomplete",
            "incomplete",
            json!({"reason":"max_output_tokens"}),
        ),
        ("end_turn", "response.completed", "completed", Value::Null),
        ("tool_use", "response.completed", "completed", Value::Null),
    ] {
        let events = [
            json!({"type":"message_start","message":{"id":"msg_stop","usage":{"input_tokens":5}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"partial"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":reason},"usage":{"output_tokens":2}}),
            json!({"type":"message_stop"}),
        ];
        let mut state = ClaudeToResponsesState::default();
        let streamed: Vec<Value> = events
            .iter()
            .flat_map(|event| send(&mut state, &json!({}), event.clone()))
            .collect();
        let terminal = streamed.last().unwrap();
        assert_eq!(terminal["type"], event_type, "{reason}");
        assert_eq!(terminal["response"]["status"], status, "{reason}");
        assert_eq!(
            terminal["response"]["incomplete_details"], details,
            "{reason}"
        );
        assert_eq!(
            terminal["response"]["output"][0]["status"], status,
            "{reason}"
        );
        let sse = events
            .iter()
            .map(|event| format!("data: {event}\n\n"))
            .collect::<String>();
        let response: Value = serde_json::from_slice(
            &convert_claude_response_to_openai_responses_non_stream(b"{}", b"{}", sse.as_bytes()),
        )
        .unwrap();
        assert_eq!(response["status"], status, "{reason}");
        assert_eq!(response["incomplete_details"], details, "{reason}");
        assert_eq!(response["output"][0]["status"], status, "{reason}");
        assert_eq!(response["usage"]["total_tokens"], 7, "{reason}");
    }
}

#[test]
fn thinking_includes_signature_without_passing_through_a_signature_delta_event() {
    let mut state = ClaudeToResponsesState::default();
    started(&mut state);
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","signature":"initial"}}),
    );
    let delta = send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"final"}}),
    );
    assert!(delta.is_empty());
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"why"}}),
    );
    let stopped = send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_stop","index":0}),
    );
    let done = stopped
        .iter()
        .find(|event| event["type"] == "response.output_item.done")
        .unwrap();
    assert_eq!(done["item"]["encrypted_content"], "final");
    assert_eq!(done["item"]["summary"][0]["text"], "why");
}

#[test]
fn redacted_thinking_becomes_marked_reasoning_in_streaming_and_non_stream() {
    const DATA: &str = "EroBCkYIBRgCKkA";
    const CARRIER: &str = "claude-redacted-thinking:EroBCkYIBRgCKkA";
    let mut state = ClaudeToResponsesState::default();
    started(&mut state);
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"redacted_thinking","data":DATA}}),
    );
    let stopped = send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_stop","index":0}),
    );
    let done = stopped
        .iter()
        .find(|event| event["type"] == "response.output_item.done")
        .unwrap();
    assert_eq!(done["item"]["encrypted_content"], CARRIER);
    let completed = send(&mut state, &json!({}), json!({"type":"message_stop"}));
    assert_eq!(
        completed.last().unwrap()["response"]["output"][0]["encrypted_content"],
        CARRIER
    );

    let raw = format!(
        "data: {{\"type\":\"message_start\",\"message\":{{\"id\":\"msg_1\"}}}}\n\
         data: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"redacted_thinking\",\"data\":\"{DATA}\"}}}}\n\
         data: {{\"type\":\"content_block_stop\",\"index\":0}}\n\
         data: {{\"type\":\"message_stop\"}}"
    );
    let value: Value = serde_json::from_slice(
        &convert_claude_response_to_openai_responses_non_stream(b"{}", b"{}", raw.as_bytes()),
    )
    .unwrap();
    assert_eq!(value["output"][0]["type"], "reasoning");
    assert_eq!(value["output"][0]["encrypted_content"], CARRIER);
}

#[test]
fn text_reasoning_and_tool_items_use_contiguous_indices_and_finalize_in_order() {
    let mut state = ClaudeToResponsesState::default();
    started(&mut state);
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_start","index":7,"content_block":{"type":"server_tool_use"}}),
    );
    let text = send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text"}}),
    );
    assert_eq!(text[0]["output_index"], 0);
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello"}}),
    );
    let reasoning = send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"thinking"}}),
    );
    assert!(reasoning
        .iter()
        .any(|event| event["type"] == "response.output_item.done" && event["output_index"] == 0));
    assert!(reasoning
        .iter()
        .any(|event| event["type"] == "response.output_item.added" && event["output_index"] == 1));
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_stop","index":1}),
    );
    let tool = send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call_1","name":"run"}}),
    );
    assert!(tool.iter().any(|event| event["output_index"] == 2));
    let stopped = send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_stop","index":2}),
    );
    let done = stopped
        .iter()
        .find(|event| event["type"] == "response.output_item.done")
        .unwrap();
    assert_eq!(done["item"]["arguments"], "{}");
}

#[test]
fn starts_a_new_message_after_a_function_and_preserves_multiple_reasoning_items() {
    let mut state = ClaudeToResponsesState::default();
    started(&mut state);
    for index in [0, 1] {
        send(
            &mut state,
            &json!({}),
            json!({"type":"content_block_start","index":index,"content_block":{"type":"thinking","signature":format!("sig{index}")}}),
        );
        send(
            &mut state,
            &json!({}),
            json!({"type":"content_block_stop","index":index}),
        );
    }
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_start","index":2,"content_block":{"type":"text"}}),
    );
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"before"}}),
    );
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_start","index":3,"content_block":{"type":"tool_use","id":"call_1","name":"run"}}),
    );
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_stop","index":3}),
    );
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_start","index":4,"content_block":{"type":"text"}}),
    );
    send(
        &mut state,
        &json!({}),
        json!({"type":"content_block_delta","index":4,"delta":{"type":"text_delta","text":"after"}}),
    );
    let completed = send(&mut state, &json!({}), json!({"type":"message_stop"}));
    let output = &completed.last().unwrap()["response"]["output"];
    assert_eq!(output.as_array().unwrap().len(), 5);
    assert_eq!(output[0]["type"], "reasoning");
    assert_eq!(output[1]["type"], "reasoning");
    assert_eq!(output[2]["content"][0]["text"], "before");
    assert_eq!(output[3]["type"], "function_call");
    assert_eq!(output[4]["content"][0]["text"], "after");
}

#[test]
fn streaming_completion_reports_cached_and_cache_creation_tokens() {
    let mut state = ClaudeToResponsesState::default();
    send(
        &mut state,
        &json!({}),
        json!({"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":13,"cache_read_input_tokens":22000,"cache_creation_input_tokens":31}}}),
    );
    send(
        &mut state,
        &json!({}),
        json!({"type":"message_delta","usage":{"output_tokens":4}}),
    );
    let completed = send(&mut state, &json!({}), json!({"type":"message_stop"}));
    let usage = &completed.last().unwrap()["response"]["usage"];
    assert_eq!(usage["input_tokens"], 22_044);
    assert_eq!(usage["input_tokens_details"]["cached_tokens"], 22_000);
    assert_eq!(usage["total_tokens"], 22_048);
}

#[test]
fn non_stream_preserves_block_order_thinking_signature_empty_args_and_usage() {
    let raw = br#"data: {"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":2,"cache_read_input_tokens":3}}}
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","signature":"sig"}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"why"}}
data: {"type":"content_block_stop","index":0}
data: {"type":"content_block_start","index":1,"content_block":{"type":"text"}}
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"answer"}}
data: {"type":"content_block_stop","index":1}
data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call_1","name":"run"}}
data: {"type":"content_block_stop","index":2}
data: {"type":"message_delta","usage":{"output_tokens":4}}
"#;
    let value: Value = serde_json::from_slice(
        &convert_claude_response_to_openai_responses_non_stream(b"{}", b"{}", raw),
    )
    .unwrap();
    assert_eq!(value["output"][0]["type"], "reasoning");
    assert_eq!(value["output"][0]["encrypted_content"], "sig");
    assert_eq!(value["output"][1]["type"], "message");
    assert_eq!(value["output"][2]["arguments"], "{}");
    assert_eq!(value["usage"]["input_tokens"], 5);
    assert_eq!(value["usage"]["input_tokens_details"]["cached_tokens"], 3);
}

#[test]
fn restores_namespace_for_streaming_and_non_stream_function_calls() {
    let request = json!({"tools":[{"type":"namespace","name":"browser","tools":[{"type":"function","name":"open"}]}]});
    let mut state = ClaudeToResponsesState::default();
    started(&mut state);
    let added = send(
        &mut state,
        &request,
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_1","name":"browser__open"}}),
    );
    assert_eq!(added[0]["item"]["name"], "open");
    assert_eq!(added[0]["item"]["namespace"], "browser");

    let raw = br#"data: {"type":"message_start","message":{"id":"msg_1"}}
data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_1","name":"browser__open"}}
data: {"type":"content_block_stop","index":0}
"#;
    let request_bytes = serde_json::to_vec(&request).unwrap();
    let value: Value = serde_json::from_slice(
        &convert_claude_response_to_openai_responses_non_stream(&request_bytes, b"{}", raw),
    )
    .unwrap();
    assert_eq!(value["output"][0]["name"], "open");
    assert_eq!(value["output"][0]["namespace"], "browser");
}
