// ref: internal/translator/openai/interactions/responses/interactions_openai_responses_response_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    convert_interactions_response_to_openai_responses_non_stream,
    convert_interactions_response_to_openai_responses_stream,
    convert_openai_responses_response_to_interactions_non_stream,
    convert_openai_responses_response_to_interactions_stream,
};
use crate::sdk::translator::{TranslationContext, TranslationState};
use serde_json::{json, Value};

type NonStreamTransform =
    fn(&TranslationContext, &str, &[u8], &[u8], &[u8], &mut TranslationState) -> Vec<u8>;

fn convert(transform: NonStreamTransform, raw: &[u8]) -> Value {
    let mut state = None;
    serde_json::from_slice(&transform(
        &TranslationContext::default(),
        "gpt-test",
        b"{}",
        b"{}",
        raw,
        &mut state,
    ))
    .unwrap()
}

#[test]
fn converts_interactions_response_non_stream() {
    let out = convert(
        convert_interactions_response_to_openai_responses_non_stream,
        br#"{"id":"interaction_1","steps":[
          {"type":"model_output","id":"msg_1","content":[{"text":"ok"}]},
          {"type":"thought","signature":"sig_1","content":[{"text":"thinking"}]},
          {"type":"function_call","name":"lookup","call_id":"call_1","arguments":{"q":"x"}}
        ],"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3,"cached_tokens":1,"reasoning_tokens":1}}"#,
    );
    assert_eq!(out["id"], "interaction_1");
    assert_eq!(out["output"][0]["content"][0]["text"], "ok");
    assert_eq!(out["output"][1]["encrypted_content"], "sig_1");
    assert_eq!(out["output"][1]["summary"][0]["text"], "thinking");
    assert_eq!(out["output"][2]["call_id"], "call_1");
    assert_eq!(out["output"][2]["arguments"], r#"{"q":"x"}"#);
    assert_eq!(out["usage"]["total_tokens"], 3);
    assert_eq!(out["usage"]["input_tokens_details"]["cached_tokens"], 1);
    assert_eq!(out["usage"]["output_tokens_details"]["reasoning_tokens"], 1);
}

#[test]
fn reads_nested_interaction_and_metadata_usage() {
    let out = convert(
        convert_interactions_response_to_openai_responses_non_stream,
        br#"{"interaction":{"id":"nested_1","model":"provider-model","steps":[{"type":"model_output","content":"ok"}]},"metadata":{"total_usage":{"total_input_tokens":2,"total_output_tokens":6,"total_thought_tokens":3,"total_cached_tokens":1,"total_tokens":11}}}"#,
    );
    assert_eq!(out["id"], "nested_1");
    assert_eq!(out["model"], "gpt-test");
    assert_eq!(out["output"][0]["content"][0]["text"], "ok");
    assert_eq!(out["usage"]["input_tokens"], 2);
    assert_eq!(out["usage"]["output_tokens"], 6);
    assert_eq!(out["usage"]["total_tokens"], 11);
}

#[test]
fn converts_responses_response_non_stream() {
    let out = convert(
        convert_openai_responses_response_to_interactions_non_stream,
        br#"{"id":"resp_1","output":[
          {"type":"message","content":[{"type":"output_text","text":"ok"}]},
          {"type":"reasoning","summary":[{"type":"summary_text","text":"thinking"}]},
          {"type":"function_call","name":"lookup","call_id":"call_1","arguments":{"q":"x"}}
        ],"usage":{"input_tokens":11,"output_tokens":13,"total_tokens":24,"input_tokens_details":{"cached_tokens":5},"output_tokens_details":{"reasoning_tokens":7}}}"#,
    );
    assert_eq!(out["id"], "resp_1");
    assert_eq!(out["steps"][0]["type"], "model_output");
    assert_eq!(out["steps"][0]["content"][0]["text"], "ok");
    assert_eq!(
        out["steps"][1],
        json!({"type":"thought", "content":[{"type":"text", "text":"thinking"}]})
    );
    assert_eq!(out["steps"][2]["type"], "function_call");
    assert_eq!(out["steps"][2]["call_id"], "call_1");
    assert_eq!(out["steps"][2]["arguments"], json!({"q":"x"}));
    assert_eq!(out["usage"]["input_tokens"], 11);
    assert_eq!(out["usage"]["total_input_tokens"], 11);
    assert_eq!(out["usage"]["reasoning_tokens"], 7);
    assert_eq!(out["usage"]["cached_tokens"], 5);
}

#[test]
fn parses_string_function_arguments_in_responses_response() {
    let out = convert(
        convert_openai_responses_response_to_interactions_non_stream,
        br#"{"id":"resp_1","output":[{"type":"function_call","name":"lookup","call_id":"call_1","arguments":"{\"q\":\"x\"}"}]}"#,
    );
    assert_eq!(out["steps"][0]["arguments"]["q"], "x");
}

fn event(raw: &[u8]) -> (String, Value) {
    let text = std::str::from_utf8(raw).unwrap();
    let name = text
        .lines()
        .find_map(|line| line.strip_prefix("event: "))
        .unwrap_or("")
        .into();
    let data = text
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .unwrap_or("");
    let value = if data == "[DONE]" {
        Value::String(data.into())
    } else {
        serde_json::from_str(data).unwrap()
    };
    (name, value)
}

#[test]
fn converts_interactions_text_function_usage_and_done_stream() {
    let context = TranslationContext::default();
    let mut state = None;
    let chunks: &[&[u8]] = &[
        br#"{"interaction":{"id":"interaction_1"},"event_type":"interaction.created"}"#,
        br#"{"index":0,"step":{"id":"msg_1","type":"model_output"},"event_type":"step.start"}"#,
        br#"{"index":0,"delta":{"text":"hello","type":"text"},"event_type":"step.delta"}"#,
        br#"{"index":0,"delta":{"text":" world","type":"text"},"event_type":"step.delta"}"#,
        br#"{"index":0,"event_type":"step.stop"}"#,
        br#"{"index":1,"step":{"id":"call_1","type":"function_call","name":"lookup","arguments":{}},"event_type":"step.start"}"#,
        br#"{"index":1,"delta":{"arguments":"{\"q\":\"x\"}","type":"arguments_delta"},"event_type":"step.delta"}"#,
        br#"{"index":1,"event_type":"step.stop"}"#,
        br#"{"interaction":{"id":"interaction_1","model":"gpt-test","usage":{"total_input_tokens":2,"total_output_tokens":6,"total_thought_tokens":3,"total_cached_tokens":1,"total_tokens":11}},"event_type":"interaction.completed"}"#,
        br#"{"event_type":"done"}"#,
    ];
    let mut events = Vec::new();
    for chunk in chunks {
        events.extend(convert_interactions_response_to_openai_responses_stream(
            &context, "gpt-test", b"", b"", chunk, &mut state,
        ));
    }
    let parsed: Vec<_> = events.iter().map(|raw| event(raw)).collect();
    let names: Vec<_> = parsed.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "response.created",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
            "response.output_text.delta",
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.output_item.added",
            "response.function_call_arguments.delta",
            "response.function_call_arguments.done",
            "response.output_item.done",
            "response.completed",
            "",
        ]
    );
    assert_eq!(parsed[5].1["text"], "hello world");
    assert_eq!(parsed[10].1["arguments"], r#"{"q":"x"}"#);
    assert_eq!(
        parsed[12].1["response"]["output"][0]["content"][0]["text"],
        "hello world"
    );
    assert_eq!(
        parsed[12].1["response"]["output"][1]["arguments"],
        r#"{"q":"x"}"#
    );
    assert_eq!(
        parsed[12].1["response"]["usage"]["output_tokens_details"]["reasoning_tokens"],
        3
    );
    assert_eq!(parsed[13].1, "[DONE]");
}

#[test]
fn converts_reasoning_and_suppresses_duplicate_function_events() {
    let context = TranslationContext::default();
    let mut state = None;
    let chunks: &[&[u8]] = &[
        br#"{"index":0,"step":{"id":"reason_1","type":"thought"},"event_type":"step.start"}"#,
        br#"{"index":0,"delta":{"content":{"text":"thinking"},"type":"thought_summary"},"event_type":"step.delta"}"#,
        br#"{"index":0,"delta":{"signature":"sig_1","type":"thought_signature"},"event_type":"step.delta"}"#,
        br#"{"index":0,"event_type":"step.stop"}"#,
        br#"{"index":1,"step":{"id":"call_1","type":"function_call","name":"lookup","arguments":{"q":"x"}},"event_type":"step.start"}"#,
        br#"{"index":1,"step":{"id":"call_1","type":"function_call","name":"lookup","arguments":{"q":"x"}},"event_type":"step.start"}"#,
        br#"{"index":1,"event_type":"step.stop"}"#,
        br#"{"index":1,"event_type":"step.stop"}"#,
    ];
    let mut parsed = Vec::new();
    for chunk in chunks {
        parsed.extend(
            convert_interactions_response_to_openai_responses_stream(
                &context, "gpt-test", b"", b"", chunk, &mut state,
            )
            .iter()
            .map(|raw| event(raw)),
        );
    }
    let reasoning = parsed
        .iter()
        .find(|(name, value)| name == "response.output_item.done" && value["output_index"] == 0)
        .unwrap();
    assert_eq!(reasoning.1["item"]["encrypted_content"], "sig_1");
    assert_eq!(reasoning.1["item"]["summary"][0]["text"], "thinking");
    assert_eq!(
        parsed
            .iter()
            .filter(|(name, value)| {
                name == "response.output_item.added" && value["output_index"] == 1
            })
            .count(),
        1
    );
    assert_eq!(
        parsed
            .iter()
            .filter(|(name, value)| {
                name == "response.output_item.done" && value["output_index"] == 1
            })
            .count(),
        1
    );
}

#[test]
fn cancelled_interactions_stream_emits_nothing() {
    let context = TranslationContext::default();
    context.cancel();
    let mut state = None;
    assert!(convert_interactions_response_to_openai_responses_stream(
        &context,
        "gpt-test",
        b"",
        b"",
        br#"{"event_type":"done"}"#,
        &mut state,
    )
    .is_empty());
}

#[test]
fn converts_responses_stream_without_repeating_text_or_arguments() {
    let context = TranslationContext::default();
    let mut state = None;
    let chunks: &[&[u8]] = &[
        br#"{"type":"response.created","response":{"id":"resp_1","model":"gpt-test","created_at":1}}"#,
        br#"{"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"hello"}"#,
        br#"{"type":"response.output_item.done","output_index":0,"item":{"id":"msg_1","type":"message","content":[{"type":"output_text","text":"hello"}]}}"#,
        br#"{"type":"response.output_item.added","output_index":1,"item":{"id":"fc_1","call_id":"call_1","type":"function_call","name":"lookup","arguments":""}}"#,
        br#"{"type":"response.function_call_arguments.delta","output_index":1,"item_id":"fc_1","call_id":"call_1","delta":"{\"q\":\"x\"}"}"#,
        br#"{"type":"response.output_item.done","output_index":1,"item":{"id":"fc_1","call_id":"call_1","type":"function_call","name":"lookup","arguments":"{\"q\":\"x\"}"}}"#,
        br#"{"type":"response.completed","response":{"id":"resp_1","model":"gpt-test","output":[{"id":"msg_1","type":"message","content":[{"type":"output_text","text":"hello"}]}],"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3}}}"#,
    ];
    let mut parsed = Vec::new();
    for chunk in chunks {
        parsed.extend(
            convert_openai_responses_response_to_interactions_stream(
                &context, "gpt-test", b"", b"", chunk, &mut state,
            )
            .iter()
            .map(|raw| event(raw)),
        );
    }
    let text_deltas: Vec<_> = parsed
        .iter()
        .filter(|(name, value)| name == "step.delta" && value["delta"]["type"] == "text")
        .collect();
    assert_eq!(text_deltas.len(), 1);
    assert_eq!(text_deltas[0].1["delta"]["text"], "hello");
    let argument_deltas: Vec<_> = parsed
        .iter()
        .filter(|(name, value)| name == "step.delta" && value["delta"]["type"] == "arguments_delta")
        .collect();
    assert_eq!(argument_deltas.len(), 1);
    assert_eq!(argument_deltas[0].1["delta"]["arguments"], r#"{"q":"x"}"#);
    let completed = parsed
        .iter()
        .find(|(name, _)| name == "interaction.completed")
        .unwrap();
    assert_eq!(completed.1["interaction"]["id"], "resp_1");
    assert_eq!(completed.1["interaction"]["usage"]["total_tokens"], 3);
    assert_eq!(
        completed.1["interaction"]["created"],
        "1970-01-01T00:00:01Z"
    );
    assert_eq!(
        completed.1["interaction"]["updated"],
        "1970-01-01T00:00:01Z"
    );
    assert_eq!(parsed.last().unwrap().0, "done");
    assert_eq!(parsed.last().unwrap().1, "[DONE]");
}

#[test]
fn responses_completion_falls_back_to_message_text_once() {
    let context = TranslationContext::default();
    let mut state = None;
    let mut events = Vec::new();
    for chunk in [
        br#"{"type":"response.created","response":{"id":"resp_2","model":"gpt-test"}}"#.as_slice(),
        br#"{"type":"response.completed","response":{"id":"resp_2","model":"gpt-test","output":[{"id":"msg_2","type":"message","content":[{"type":"output_text","text":"fallback"}]}],"usage":{}}}"#.as_slice(),
    ] {
        events.extend(convert_openai_responses_response_to_interactions_stream(
            &context, "gpt-test", b"", b"", chunk, &mut state,
        ));
    }
    let parsed: Vec<_> = events.iter().map(|raw| event(raw)).collect();
    let deltas: Vec<_> = parsed
        .iter()
        .filter(|(name, value)| name == "step.delta" && value["delta"]["text"] == "fallback")
        .collect();
    assert_eq!(deltas.len(), 1);
}

// ref: interactions_openai_responses_response_test.go @ d7914afd
#[test]
fn candidate_interactions_v13_reasoning_summary_has_a_complete_single_part_lifecycle() {
    for parts in [vec!["Über", "blick\n", "fertig"], Vec::<&str>::new()] {
        let context = TranslationContext::default();
        let mut state = None;
        let mut inputs = vec![
            json!({"event_type":"step.start","index":7,"step":{"type":"thought","id":"reasoning_7"}}),
        ];
        for text in &parts {
            inputs.push(json!({"event_type":"step.delta","index":7,"delta":{"type":"thought_summary","text":text}}));
        }
        inputs.push(json!({"event_type":"step.stop","index":7}));
        inputs.push(
            json!({"event_type":"interaction.completed","interaction":{"id":"interaction_7"}}),
        );
        let parsed: Vec<_> = inputs
            .iter()
            .flat_map(|input| {
                convert_interactions_response_to_openai_responses_stream(
                    &context,
                    "gpt-test",
                    b"",
                    b"",
                    &serde_json::to_vec(input).unwrap(),
                    &mut state,
                )
                .iter()
                .map(|bytes| event(bytes))
                .collect::<Vec<_>>()
            })
            .collect();
        let mut names = vec![
            "response.output_item.added",
            "response.reasoning_summary_part.added",
        ];
        names.extend(std::iter::repeat("response.reasoning_summary_text.delta").take(parts.len()));
        names.extend([
            "response.reasoning_summary_text.done",
            "response.reasoning_summary_part.done",
            "response.output_item.done",
            "response.completed",
        ]);
        assert_eq!(
            parsed
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            names
        );
        for (position, (_, payload)) in parsed.iter().enumerate() {
            assert_eq!(payload["sequence_number"], position + 1);
            if position > 0 && position < parsed.len() - 2 {
                assert_eq!(payload["item_id"], "reasoning_7");
                assert_eq!(payload["output_index"], 7);
                assert_eq!(payload["summary_index"], 0);
            }
        }
        assert_eq!(
            parsed[1].1["part"],
            json!({"type":"summary_text","text":""})
        );
        let done = parsed
            .iter()
            .find(|(name, _)| name == "response.output_item.done")
            .unwrap();
        assert_eq!(done.1["item"]["status"], "completed");
        assert_eq!(
            done.1["item"]["summary"],
            json!([{"type":"summary_text","text":parts.concat()}])
        );
        assert_eq!(
            parsed.last().unwrap().1["response"]["output"][0],
            done.1["item"]
        );
        assert_eq!(parsed[parsed.len() - 4].1["text"], parts.concat());
        assert_eq!(parsed[parsed.len() - 3].1["part"]["text"], parts.concat());
    }
}

#[test]
fn candidate_interactions_v13_overlapping_output_keeps_late_signature_and_final_item_identical() {
    let context = TranslationContext::default();
    let mut state = None;
    let chunks = [
        json!({"event_type":"step.start","index":0,"step":{"type":"thought","id":"reason_0"}}),
        json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_summary","text":"first "}}),
        json!({"event_type":"step.start","index":1,"step":{"type":"model_output","id":"message_1"}}),
        json!({"event_type":"step.delta","index":1,"delta":{"type":"text","text":"answer"}}),
        json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_signature","signature":"partial"}}),
        json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_summary","text":"second"}}),
        json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_signature","signature":"partial-complete"}}),
        json!({"event_type":"step.stop","index":1}),
        json!({"event_type":"step.stop","index":0}),
        json!({"event_type":"interaction.completed","interaction":{"id":"interaction"}}),
    ];
    let mut parsed = Vec::new();
    for chunk in chunks {
        parsed.extend(
            convert_interactions_response_to_openai_responses_stream(
                &context,
                "gpt-test",
                b"",
                b"",
                &serde_json::to_vec(&chunk).unwrap(),
                &mut state,
            )
            .iter()
            .map(|bytes| event(bytes)),
        );
    }
    let item_done: Vec<_> = parsed
        .iter()
        .filter(|(name, _)| name == "response.output_item.done")
        .collect();
    assert_eq!(item_done.len(), 2);
    assert_eq!(item_done[0].1["item"]["id"], "message_1");
    assert_eq!(item_done[1].1["item"]["id"], "reason_0");
    assert_eq!(
        item_done[1].1["item"]["encrypted_content"],
        "partial-complete"
    );
    assert_eq!(
        item_done[1].1["item"]["summary"],
        json!([{"type":"summary_text","text":"first second"}])
    );
    let completed = &parsed.last().unwrap().1["response"]["output"];
    assert_eq!(completed[0], item_done[1].1["item"]);
    assert_eq!(completed[1], item_done[0].1["item"]);
}
