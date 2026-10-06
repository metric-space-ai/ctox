// ref: internal/runtime/executor/devin_executor.go:464-1123
// ref: internal/runtime/executor/devin_executor_test.go:841-1214,1269-1989,3835-4783
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::super::helps::devin_wire::{
    wrap_connect_envelope, ConnectFrameDecoder, CONNECT_FLAG_DATA,
};
use super::*;

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        out.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    out.push(value as u8);
}
fn number(out: &mut Vec<u8>, field: u32, value: u64) {
    varint(out, u64::from(field) << 3);
    varint(out, value);
}
fn bytes(out: &mut Vec<u8>, field: u32, value: &[u8]) {
    varint(out, (u64::from(field) << 3) | 2);
    varint(out, value.len() as u64);
    out.extend_from_slice(value);
}
fn frame(fields: &[(u32, &[u8])]) -> ConnectFrame {
    let mut payload = Vec::new();
    for (field, value) in fields {
        bytes(&mut payload, *field, value);
    }
    ConnectFrame {
        flag: CONNECT_FLAG_DATA,
        payload,
    }
}
fn eos() -> ConnectFrame {
    ConnectFrame {
        flag: CONNECT_FLAG_END_STREAM,
        payload: b"{}".to_vec(),
    }
}
fn tool(id: &str, name: &str, arguments: &[u8], legacy: bool) -> Vec<u8> {
    let mut value = Vec::new();
    bytes(&mut value, 1, id.as_bytes());
    bytes(&mut value, 2, name.as_bytes());
    bytes(&mut value, if legacy { 4 } else { 3 }, arguments);
    value
}
fn values(batch: &DevinStreamBatch) -> Vec<Value> {
    batch
        .events
        .iter()
        .map(|event| serde_json::from_slice(event).unwrap())
        .collect()
}
fn kind(value: &Value) -> &str {
    value["event_type"].as_str().unwrap()
}

#[test]
fn candidate_devin_stream_text_is_visible_before_eos_and_split_utf8_is_retained() {
    let mut state = DevinInteractionsStream::new("chosen-model", "openai");
    let first = state.accept(frame(&[(3, b"A\xe2")]));
    assert!(!first.complete && first.error.is_none());
    let events = values(&first);
    assert_eq!(events.len(), 3);
    assert_eq!(kind(&events[0]), "interaction.created");
    assert_eq!(events[0]["interaction"]["model"], "chosen-model");
    assert_eq!(events[2]["delta"]["text"], "A");
    let next = state.accept(frame(&[(3, b"\x82\xac")]));
    assert_eq!(values(&next)[0]["delta"]["text"], "€");
    assert!(!next.complete);
    let finished = state.accept(eos());
    assert!(finished.complete && finished.error.is_none());
    assert_eq!(
        kind(values(&finished).last().unwrap()),
        "interaction.completed"
    );
    assert!(state.finish().events.is_empty());
    assert!(state.accept(eos()).events.is_empty());
}

#[test]
fn candidate_devin_stream_responses_retains_late_cumulative_thought_signatures() {
    let mut state = DevinInteractionsStream::new("model", "openai-response");
    let thought = state.accept(frame(&[(9, b"reason")]));
    assert_eq!(values(&thought)[1]["step"]["type"], "thought");
    let text = state.accept(frame(&[(3, b"visible")]));
    assert!(values(&text).iter().all(|event| kind(event) != "step.stop"));
    assert_eq!(values(&text)[0]["index"], 1);
    let first = state.accept(frame(&[(10, b"part-"), (21, b"anthropic")]));
    assert_eq!(values(&first)[0]["index"], 0);
    assert_eq!(values(&first)[0]["delta"]["signature"], "part-");
    let second = state.accept(frame(&[(10, b"end")]));
    assert_eq!(values(&second)[0]["delta"]["signature"], "part-end");
    let done = values(&state.accept(eos()));
    assert_eq!(
        (kind(&done[0]), done[0]["index"].as_u64()),
        ("step.stop", Some(0))
    );
    assert_eq!(
        (kind(&done[1]), done[1]["index"].as_u64()),
        ("step.stop", Some(1))
    );
    assert_eq!(kind(done.last().unwrap()), "interaction.completed");
}

#[test]
fn candidate_devin_stream_block_protocols_queue_tools_until_late_signature_then_text() {
    let mut state = DevinInteractionsStream::new("model", "claude");
    state.accept(frame(&[(9, b"thought")]));
    let first_tool = tool("first", "lookup", b"{", false);
    assert!(state.accept(frame(&[(6, &first_tool)])).events.is_empty());
    assert!(state.accept(frame(&[(3, b"final text")])).events.is_empty());
    let signature = values(&state.accept(frame(&[(10, b"signed")])));
    assert_eq!(signature[0]["delta"]["type"], "thought_signature");
    let events = values(&state.accept(eos()));
    let thought_stop = events
        .iter()
        .position(|e| kind(e) == "step.stop" && e["index"] == 0)
        .unwrap();
    let tool_start = events
        .iter()
        .position(|e| e["step"]["type"] == "function_call")
        .unwrap();
    let tool_stop = events
        .iter()
        .position(|e| kind(e) == "step.stop" && e["index"] == 1)
        .unwrap();
    let text_start = events
        .iter()
        .position(|e| e["step"]["type"] == "model_output")
        .unwrap();
    assert!(thought_stop < tool_start && tool_start < tool_stop && tool_stop < text_start);
    assert!(events.iter().any(|e| e["delta"]["text"] == "final text"));
}

#[test]
fn candidate_devin_stream_early_text_cannot_overtake_a_queued_tool() {
    for format in ["openai", "openai-response"] {
        let mut state = DevinInteractionsStream::new("model", format);
        state.accept(frame(&[(9, b"thought")]));
        let call = tool("call", "tool", b"{}", false);
        assert!(state.accept(frame(&[(6, &call)])).events.is_empty());
        assert!(state.accept(frame(&[(3, b"after tool")])).events.is_empty());
        let events = values(&state.accept(eos()));
        let tool_stop = events
            .iter()
            .position(|e| kind(e) == "step.stop" && e["index"] == 1)
            .unwrap();
        let text_start = events
            .iter()
            .position(|e| e["step"]["type"] == "model_output")
            .unwrap();
        assert!(tool_stop < text_start);
    }
}

#[test]
fn candidate_devin_stream_interleaved_ids_update_names_and_preserve_the_128_call_limit() {
    let mut state = DevinInteractionsStream::new("model", "interactions");
    let mut events = Vec::new();
    for (id, name, arguments, legacy) in [
        ("a", "", b"{".as_slice(), false),
        ("b", "second", b"{}".as_slice(), false),
        ("a", "first", b"}".as_slice(), false),
        ("", "", b"<>&".as_slice(), true),
    ] {
        events.extend(values(
            &state.accept(frame(&[(6, &tool(id, name, arguments, legacy))])),
        ));
    }
    let update = events
        .iter()
        .find(|e| e["step"]["name"] == "first")
        .unwrap();
    assert_eq!(update["index"], 0);
    let legacy = events
        .iter()
        .find(|e| e["delta"]["invalid_json_str"] == true)
        .unwrap();
    assert_eq!(legacy["index"], 0);
    assert_eq!(legacy["delta"]["arguments"], "<>&");
    let mut bounded = DevinInteractionsStream::new("model", "openai");
    let mut starts = 0;
    for index in 0..140 {
        let call = tool(&format!("id-{index}"), "call", b"{}", false);
        starts += values(&bounded.accept(frame(&[(6, &call)])))
            .iter()
            .filter(|event| event["step"]["type"] == "function_call")
            .count();
    }
    assert_eq!(starts, MAX_DEVIN_TOOL_CALLS);
    let existing = tool("id-0", "ignored rename", b"tail", false);
    let output = values(&bounded.accept(frame(&[(6, &existing)])));
    assert_eq!(output.len(), 1);
    assert_eq!(output[0]["index"], 0);
    assert_eq!(output[0]["delta"]["arguments"], "tail");
    assert_eq!(
        values(&bounded.accept(eos()))
            .iter()
            .filter(|event| kind(event) == "step.stop")
            .count(),
        MAX_DEVIN_TOOL_CALLS
    );
}

#[test]
fn candidate_devin_stream_new_thinking_flushes_queued_actions_before_a_new_thought() {
    let mut state = DevinInteractionsStream::new("model", "claude");
    state.accept(frame(&[(9, b"first thought")]));
    let call = tool("call", "lookup", b"{}", false);
    state.accept(frame(&[(6, &call)]));
    state.accept(frame(&[(3, b"post-tool reply")]));
    let events = values(&state.accept(frame(&[(9, b"second thought")])));
    assert_eq!(
        (kind(&events[0]), events[0]["index"].as_u64()),
        ("step.stop", Some(0))
    );
    let call_start = events
        .iter()
        .position(|e| e["step"]["type"] == "function_call")
        .unwrap();
    let thought_start = events
        .iter()
        .position(|e| e["step"]["type"] == "thought")
        .unwrap();
    assert!(call_start < thought_start);
    assert_eq!(events[thought_start]["index"], 2);
    let done = values(&state.accept(eos()));
    let tool_stop = done
        .iter()
        .position(|e| kind(e) == "step.stop" && e["index"] == 1)
        .unwrap();
    let text = done
        .iter()
        .position(|e| e["delta"]["text"] == "post-tool reply")
        .unwrap();
    assert!(tool_stop < text);
}

#[test]
fn candidate_devin_stream_failures_never_create_success_or_duplicate_terminals() {
    let trailer = || ConnectFrame {
        flag: CONNECT_FLAG_END_STREAM,
        payload: br#"{"error":{"code":"resource_exhausted","message":"limited"}}"#.to_vec(),
    };
    let mut empty = DevinInteractionsStream::new("model", "interactions");
    let first = empty.accept(trailer());
    assert!(first.events.is_empty() && !first.complete);
    assert!(matches!(first.error, Some(DevinAggregateError::Trailer(_))));
    assert!(empty.finish().events.is_empty());
    let mut visible = DevinInteractionsStream::new("model", "interactions");
    visible.accept(frame(&[(3, b"visible")]));
    let failure = visible.accept(trailer());
    let events = values(&failure);
    assert_eq!(kind(events.last().unwrap()), "response.failed");
    assert!(events
        .iter()
        .all(|event| kind(event) != "interaction.completed"));
    assert!(!failure.complete && failure.error.as_ref().unwrap().status_code() == 429);
    assert!(visible.accept(eos()).events.is_empty());
    let mut truncated = DevinInteractionsStream::new("model", "openai");
    assert!(truncated
        .accept(ConnectFrame {
            flag: 0,
            payload: vec![0]
        })
        .events
        .is_empty());
    let eof = truncated.finish();
    assert!(matches!(eof.error, Some(DevinAggregateError::PrematureEof)));
    assert!(eof.events.is_empty() && !eof.complete);
}

#[test]
fn candidate_devin_stream_usage_keeps_known_cache_counts_and_the_original_status() {
    let mut first = Vec::new();
    for (field, value) in [(2, 20), (3, 3), (4, 2), (5, 7), (6, 201)] {
        number(&mut first, field, value);
    }
    bytes(&mut first, 8, b"first-request");
    bytes(&mut first, 9, b"upstream-model");
    let mut next = Vec::new();
    for (field, value) in [(2, 0), (3, 5), (5, 0), (6, 500)] {
        number(&mut next, field, value);
    }
    bytes(&mut next, 9, b"actual-model");
    let mut state = DevinInteractionsStream::new("requested", "openai-response");
    state.accept(frame(&[(7, &first)]));
    state.accept(frame(&[(7, &next)]));
    let usage = state.usage().unwrap();
    assert_eq!(
        (
            usage.prompt_tokens,
            usage.completion_tokens,
            usage.cached_tokens,
            usage.cache_write_tokens,
            usage.status_code
        ),
        (20, 5, 7, 2, 201)
    );
    assert_eq!(usage.request_id, b"first-request");
    assert_eq!(usage.model_name, b"actual-model");
    let events = values(&state.accept(eos()));
    let usage = &events.last().unwrap()["interaction"]["usage"];
    assert_eq!(usage["total_input_tokens"], 27);
    assert_eq!(usage["total_output_tokens"], 5);
    assert_eq!(usage["total_tokens"], 32);
    assert_eq!(usage["total_cached_tokens"], 7);
    assert_eq!(usage["cache_write_tokens"], 2);
}

#[test]
fn candidate_devin_stream_byte_bound_and_debug_preserve_failure_and_privacy() {
    let mut state = DevinInteractionsStream::with_byte_limit("private-model", "openai", 8);
    let first = state.accept(frame(&[(3, b"x")]));
    assert!(!format!("{first:?}").contains("private-model"));
    let failed = state.accept(frame(&[(3, b"private-prompt")]));
    assert!(matches!(
        failed.error,
        Some(DevinAggregateError::ResponseTooLarge)
    ));
    assert!(!failed.complete);
    assert_eq!(kind(values(&failed).last().unwrap()), "response.failed");
    let debug = format!("{state:?}");
    assert!(!debug.contains("private-prompt") && !debug.contains("private-model"));
    assert!(state.finish().events.is_empty());
}

#[test]
fn candidate_devin_stream_framing_errors_preserve_early_output_and_stop_reason_semantics() {
    let payload = frame(&[(3, b"early")]).payload;
    let mut wire = wrap_connect_envelope(&payload).unwrap();
    wire.extend_from_slice(&[4, 0, 0, 0, 0]);
    let mut decoder = ConnectFrameDecoder::default();
    let mut state = DevinInteractionsStream::new("model", "openai");
    let mut output = Vec::new();
    let error = decoder
        .feed(&wire, |frame| output.extend(values(&state.accept(frame))))
        .unwrap_err();
    assert!(output.iter().any(|event| event["delta"]["text"] == "early"));
    let failure = state.fail(DevinAggregateError::Connect(error), "stream_read_error");
    assert!(!failure.complete && failure.error.is_some());
    assert!(values(&failure)
        .iter()
        .all(|event| kind(event) != "interaction.completed"));
    for (stop, reason) in [
        (0, None),
        (1, Some("length")),
        (3, Some("length")),
        (11, Some("content_filter")),
    ] {
        let mut state = DevinInteractionsStream::new("model", "interactions");
        let mut payload = Vec::new();
        number(&mut payload, 5, stop);
        state.accept(ConnectFrame { flag: 0, payload });
        let done = state.accept(eos());
        assert!(done.complete && done.error.is_none());
        let events = values(&done);
        let item = &events.last().unwrap()["interaction"];
        assert_eq!(item["finish_reason"].as_str(), reason);
        assert_eq!(
            item["status"],
            if reason.is_some() {
                "incomplete"
            } else {
                "completed"
            }
        );
    }
}
