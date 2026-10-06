// ref: internal/runtime/executor/devin_executor_test.go:394-507,774-842,1061-1151,1328-1353,1413-1438,3108-3161
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::devin_executor_response::{
    consume_devin_frames_to_interactions, DevinAggregateContext, DevinAggregateError,
    DevinInteractionAccumulator, MAX_DEVIN_TOOL_CALLS,
};
use super::helps::devin_wire::{ConnectFrame, ConnectFrameError, CONNECT_FLAG_END_STREAM};
use serde_json::Value;
use std::io::Cursor;

fn varint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        bytes.push(value as u8 | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
}
fn tag(bytes: &mut Vec<u8>, field: u32, kind: u8) {
    varint(bytes, u64::from(field) << 3 | u64::from(kind));
}
fn number(bytes: &mut Vec<u8>, field: u32, value: u64) {
    tag(bytes, field, 0);
    varint(bytes, value);
}
fn data(bytes: &mut Vec<u8>, field: u32, value: &[u8]) {
    tag(bytes, field, 2);
    varint(bytes, value.len() as u64);
    bytes.extend_from_slice(value);
}
fn tool(id: &[u8], name: &[u8], arguments: &[u8], legacy: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    data(&mut bytes, 1, id);
    data(&mut bytes, 2, name);
    data(&mut bytes, 3, arguments);
    data(&mut bytes, 4, legacy);
    let mut frame = Vec::new();
    data(&mut frame, 6, &bytes);
    frame
}
fn envelope(bytes: &mut Vec<u8>, flag: u8, payload: &[u8]) {
    bytes.push(flag);
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
}
fn complete(frames: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for frame in frames {
        envelope(&mut bytes, 0, frame);
    }
    envelope(&mut bytes, CONNECT_FLAG_END_STREAM, b"{}");
    bytes
}
fn context() -> DevinAggregateContext<'static> {
    DevinAggregateContext {
        model: "devin/swe-2",
        original_request: b"",
    }
}
fn accept(state: &mut DevinInteractionAccumulator, payload: Vec<u8>) {
    assert!(!state.accept(ConnectFrame { flag: 0, payload }).unwrap());
}
fn eos(state: &mut DevinInteractionAccumulator) {
    assert!(state
        .accept(ConnectFrame {
            flag: CONNECT_FLAG_END_STREAM,
            payload: b"{}".to_vec(),
        })
        .unwrap());
}
fn json(payload: &[u8]) -> Value {
    serde_json::from_slice(payload).unwrap()
}
fn metric(bytes: &mut Vec<u8>, key: &[u8], value: f32) {
    let mut scalar = Vec::new();
    tag(&mut scalar, 2, 5);
    scalar.extend_from_slice(&value.to_bits().to_le_bytes());
    let mut entry = Vec::new();
    data(&mut entry, 4, &scalar);
    data(&mut entry, 5, key);
    data(bytes, 2, &entry);
}
fn dimensions(prompt: f32, output: f32, cached: f32) -> Vec<u8> {
    let mut group = Vec::new();
    data(&mut group, 1, b"Token Usage");
    metric(&mut group, b"input_tokens", prompt);
    metric(&mut group, b"output_tokens", output);
    metric(&mut group, b"cached_input_tokens", cached);
    let mut frame = Vec::new();
    data(&mut frame, 28, &group);
    frame
}
fn header(bytes: &mut Vec<u8>, name: &[u8], value: &[u8]) {
    let mut entry = Vec::new();
    data(&mut entry, 1, name);
    data(&mut entry, 2, value);
    data(bytes, 8, &entry);
}

#[test]
fn candidate_devin_aggregate_complete_keeps_steps_signatures_and_cached_usage() {
    let mut before = Vec::new();
    data(&mut before, 1, b"bot-uuid-123");
    data(&mut before, 3, b"Hello ");
    data(&mut before, 9, b"Let me think...");
    data(&mut before, 10, b"sealed-");
    let mut call = tool(b"call_1", b"bash", br#"{"command":"ls"}"#, b"");
    let mut usage = Vec::new();
    for (field, value) in [(2, 100), (3, 50), (5, 20)] {
        number(&mut usage, field, value);
    }
    data(&mut call, 7, &usage);
    let mut after = Vec::new();
    data(&mut after, 3, "世界 🚀".as_bytes());
    data(&mut after, 10, b"signature");
    data(&mut after, 21, b"anthropic");
    let output = consume_devin_frames_to_interactions(
        &mut Cursor::new(complete(&[before, call, after])),
        &context(),
    )
    .unwrap();
    let root = json(&output.payload);
    assert!(root["id"].as_str().unwrap().starts_with("interaction_"));
    assert_eq!(root["model"], "devin/swe-2");
    assert_eq!(root["status"], "completed");
    assert_eq!(
        root["usage"],
        serde_json::json!({
            "total_input_tokens": 120, "total_output_tokens": 50,
            "total_cached_tokens": 20, "total_tokens": 170,
        })
    );
    let steps = root["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 4);
    assert_eq!(steps[0]["type"], "thought");
    assert_eq!(steps[0]["content"][0]["text"], "Let me think...");
    assert_eq!(steps[0]["signature"], "sealed-signature");
    assert_eq!(steps[0]["thought_signature"], "sealed-signature");
    assert_eq!(steps[1]["content"][0]["text"], "Hello ");
    assert_eq!(steps[2]["type"], "function_call");
    assert_eq!(steps[2]["id"], "call_1");
    assert_eq!(steps[2]["call_id"], "call_1");
    assert_eq!(steps[2]["arguments"]["command"], "ls");
    assert_eq!(steps[3]["content"][0]["text"], "世界 🚀");
    assert_eq!(output.observation.frames_count, 4);
    assert_eq!(output.observation.content, "Hello 世界 🚀".as_bytes());
    assert_eq!(output.observation.signature_type, b"anthropic");
    let debug = format!("{output:?}");
    for private in [
        "sealed-signature",
        "command",
        "Let me",
        "call_1",
        "Hello",
        "世界",
    ] {
        assert!(!debug.contains(private), "{private}");
    }
}

#[test]
fn candidate_devin_aggregate_interleaved_calls_cap_and_owned_observations() {
    let mut state = DevinInteractionAccumulator::default();
    accept(&mut state, tool(b"call_0", b"alpha", br#"{"a":"#, b""));
    accept(&mut state, tool(b"call_1", b"beta", br#"{"b":"#, b""));
    accept(&mut state, tool(b"call_0", b"", b"1}", b""));
    accept(&mut state, tool(b"", b"", b" ", b""));
    let snapshot = state.observation();
    assert_eq!(snapshot.tool_calls[0].arguments, br#"{"a":1} "#);
    assert_eq!(snapshot.tool_calls[1].arguments, br#"{"b":"#);
    accept(&mut state, tool(b"call_1", b"", b"2}", b""));
    for index in 2..135 {
        let id = format!("call_{index}");
        accept(&mut state, tool(id.as_bytes(), b"new", b"{}", b""));
    }
    // Existing calls still accept deltas after the creation cap has been reached.
    accept(&mut state, tool(b"call_0", b"alpha_final", b"\n", b""));
    eos(&mut state);
    let output = state.finish(&context()).unwrap();
    let root = json(&output.payload);
    let calls = root["steps"].as_array().unwrap();
    assert_eq!(calls.len(), MAX_DEVIN_TOOL_CALLS);
    assert_eq!(calls[0]["name"], "alpha_final");
    assert_eq!(calls[0]["arguments"]["a"], 1);
    assert_eq!(calls[1]["arguments"]["b"], 2);
    assert_eq!(calls[127]["id"], "call_127");
    assert_eq!(
        output.observation.tool_calls[0].arguments,
        br#"{"a":1} 
"#
    );
    assert_eq!(snapshot.tool_calls.len(), 2);
    assert_eq!(snapshot.tool_calls[0].name, b"alpha");
    assert_eq!(snapshot.tool_calls[1].arguments, br#"{"b":"#);
}

#[test]
fn candidate_devin_aggregate_usage_merge_dimension_fallback_and_cache_write() {
    let mut state = DevinInteractionAccumulator::default();
    let mut first = Vec::new();
    let mut usage = Vec::new();
    for (field, value) in [(2, 3), (3, 10), (4, 14_361), (5, 50), (6, 201)] {
        number(&mut usage, field, value);
    }
    data(&mut usage, 9, b"initial-model");
    header(&mut usage, b"x-request-id", b"first-request");
    header(&mut usage, b"unchanged", b"keep");
    data(&mut first, 7, &usage);
    data(&mut first, 111, b"unknown");
    accept(&mut state, first);
    accept(&mut state, dimensions(999.0, 999.0, 999.0));
    let mut later = Vec::new();
    let mut usage = Vec::new();
    number(&mut usage, 3, 11);
    number(&mut usage, 6, 429);
    data(&mut usage, 9, b"final-model");
    header(&mut usage, b"x-request-id", b"later-request");
    data(&mut later, 7, &usage);
    data(&mut later, 111, b"again");
    data(&mut later, 112, b"next");
    accept(&mut state, later);
    eos(&mut state);
    let output = state.finish(&context()).unwrap();
    let usage = output.observation.usage.as_ref().unwrap();
    assert_eq!(
        (
            usage.prompt_tokens,
            usage.completion_tokens,
            usage.cached_tokens,
            usage.cache_write_tokens,
            usage.status_code
        ),
        (3, 11, 50, 14_361, 201)
    );
    assert_eq!(usage.model_name, b"final-model");
    assert_eq!(usage.request_id, b"later-request");
    assert_eq!(usage.headers.get(b"unchanged".as_slice()).unwrap(), b"keep");
    assert_eq!(output.observation.unknown_fields, [111, 112]);
    let root = json(&output.payload);
    assert_eq!(root["usage"]["total_input_tokens"], 53);
    assert_eq!(root["usage"]["total_output_tokens"], 11);
    assert_eq!(root["usage"]["cache_write_tokens"], 14_361);
    assert_eq!(root["usage"]["total_tokens"], 64);

    let mut state = DevinInteractionAccumulator::default();
    let mut first = Vec::new();
    let mut usage = Vec::new();
    number(&mut usage, 2, 7);
    data(&mut first, 7, &usage);
    accept(&mut state, first);
    accept(&mut state, dimensions(575.0, 5.0, 128.0));
    eos(&mut state);
    let output = state.finish(&context()).unwrap();
    let root = json(&output.payload);
    assert_eq!(root["usage"]["total_input_tokens"], 135);
    assert_eq!(root["usage"]["total_output_tokens"], 5);
    assert_eq!(root["usage"]["total_cached_tokens"], 128);
    assert_eq!(root["usage"]["total_tokens"], 140);
    assert!(root["usage"].get("cache_write_tokens").is_none());
}

#[test]
fn candidate_devin_aggregate_trailers_eof_and_transport_failures_keep_partial_evidence() {
    let mut text = Vec::new();
    data(&mut text, 3, b"private-partial-answer");
    for (code, status) in [("unauthenticated", 401), ("resource_exhausted", 429)] {
        let trailer = format!(r#"{{"error":{{"code":"{code}","message":"private-token-error"}}}}"#);
        let mut bytes = Vec::new();
        envelope(&mut bytes, 0, &text);
        envelope(&mut bytes, CONNECT_FLAG_END_STREAM, trailer.as_bytes());
        let failure =
            consume_devin_frames_to_interactions(&mut Cursor::new(bytes), &context()).unwrap_err();
        assert_eq!(failure.error.status_code(), status);
        assert_eq!(failure.observation.content, b"private-partial-answer");
        assert_eq!(failure.observation.frames_count, 2);
        let debug = format!("{failure:?}");
        assert!(!debug.contains("private-"));
        let mut state = DevinInteractionAccumulator::default();
        accept(&mut state, text.clone());
        assert!(state
            .accept(ConnectFrame {
                flag: CONNECT_FLAG_END_STREAM,
                payload: trailer.into_bytes()
            })
            .is_err());
        assert_eq!(
            state
                .accept(ConnectFrame {
                    flag: CONNECT_FLAG_END_STREAM,
                    payload: b"{}".to_vec()
                })
                .unwrap_err()
                .status_code(),
            status
        );
        assert_eq!(
            state.finish(&context()).unwrap_err().error.status_code(),
            status
        );
    }
    let mut bytes = Vec::new();
    envelope(&mut bytes, 0, &text);
    let eof = consume_devin_frames_to_interactions(&mut Cursor::new(bytes.clone()), &context())
        .unwrap_err();
    assert!(matches!(eof.error, DevinAggregateError::PrematureEof));
    assert_eq!(eof.observation.frames_count, 1);
    bytes.extend_from_slice(&[0, 0, 0, 0, 3, 0x1a]);
    let truncated =
        consume_devin_frames_to_interactions(&mut Cursor::new(bytes), &context()).unwrap_err();
    assert!(matches!(
        truncated.error,
        DevinAggregateError::Connect(ConnectFrameError::Truncated)
    ));
    assert_eq!(truncated.observation.content, b"private-partial-answer");
    let mut bytes = complete(&[vec![0x0a, 0x80], text]);
    // Bytes after successful EOS belong to a later transport and cannot alter this result.
    envelope(
        &mut bytes,
        CONNECT_FLAG_END_STREAM,
        br#"{"error":{"code":"unauthenticated"}}"#,
    );
    let output = consume_devin_frames_to_interactions(&mut Cursor::new(bytes), &context()).unwrap();
    assert_eq!(output.observation.frames_count, 3);
    assert_eq!(output.observation.content, b"private-partial-answer");
}

#[test]
fn candidate_devin_aggregate_stop_reasons_and_no_usage_do_not_fabricate_tokens() {
    for (stop, status, reason) in [
        (1, "incomplete", Some("length")),
        (3, "incomplete", Some("length")),
        (11, "incomplete", Some("content_filter")),
        (2, "completed", None),
        (4, "completed", None),
        (0, "completed", None),
    ] {
        let mut frame = Vec::new();
        number(&mut frame, 5, stop);
        data(&mut frame, 3, b"answer");
        let output =
            consume_devin_frames_to_interactions(&mut Cursor::new(complete(&[frame])), &context())
                .unwrap();
        let root = json(&output.payload);
        assert_eq!(root["status"], status, "{stop}");
        assert_eq!(root.get("finish_reason").and_then(Value::as_str), reason);
        assert_eq!(
            root["usage"],
            serde_json::json!({
                "total_input_tokens": 0, "total_output_tokens": 0, "total_cached_tokens": 0,
            })
        );
        assert!(output.observation.usage.is_none());
    }
}

#[test]
fn candidate_devin_aggregate_legacy_guard_signature_only_and_sticky_byte_budget() {
    let guarded = DevinAggregateContext {
        model: "devin/swe-2",
        original_request: br#"{"tools":[{"type":"namespace","name":"patch","tools":[{"type":"custom","name":"apply_patch"}]}]}"#,
    };
    let legacy = tool(
        b"patch",
        b"patch__apply_patch",
        b"",
        b"not valid patch JSON",
    );
    let mut state = DevinInteractionAccumulator::default();
    accept(&mut state, legacy.clone());
    // Native original declaration classification runs before the premature-EOF fallback.
    assert!(matches!(
        state.finish(&guarded).unwrap_err().error,
        DevinAggregateError::LegacyApplyPatch
    ));
    let ordinary = DevinAggregateContext {
        model: "devin/swe-2",
        original_request: br#"{"tools":[{"type":"function","name":"patch__apply_patch"}]}"#,
    };
    assert!(consume_devin_frames_to_interactions(
        &mut Cursor::new(complete(&[legacy.clone()])),
        &ordinary,
    )
    .is_ok());
    let output =
        consume_devin_frames_to_interactions(&mut Cursor::new(complete(&[legacy])), &context())
            .unwrap();
    assert_eq!(
        json(&output.payload)["steps"][0]["arguments"],
        "not valid patch JSON"
    );

    let mut signature = Vec::new();
    data(&mut signature, 10, b"late-private-signature");
    let output =
        consume_devin_frames_to_interactions(&mut Cursor::new(complete(&[signature])), &context())
            .unwrap();
    let root = json(&output.payload);
    assert_eq!(root["steps"][0]["type"], "thought");
    assert!(root["steps"][0].get("content").is_none());
    assert_eq!(
        root["steps"][0]["thought_signature"],
        "late-private-signature"
    );

    let mut state = DevinInteractionAccumulator::with_byte_limit(5);
    let mut text = Vec::new();
    data(&mut text, 3, b"ok");
    accept(&mut state, text);
    assert!(matches!(
        state
            .accept(ConnectFrame {
                flag: CONNECT_FLAG_END_STREAM,
                payload: b"{}".to_vec(),
            })
            .unwrap_err(),
        DevinAggregateError::ResponseTooLarge
    ));
    assert!(matches!(
        state
            .accept(ConnectFrame {
                flag: CONNECT_FLAG_END_STREAM,
                payload: vec![],
            })
            .unwrap_err(),
        DevinAggregateError::ResponseTooLarge
    ));
    let failure = state.finish(&context()).unwrap_err();
    assert!(matches!(
        failure.error,
        DevinAggregateError::ResponseTooLarge
    ));
    assert_eq!(failure.observation.content, b"ok");
}

#[test]
fn candidate_devin_aggregate_raw_arguments_and_utf8_boundaries_are_preserved() {
    let raw = br#"{ "n":9007199254740993, "n":9223372036854775808, "big":1e400, "s":"<&>" }"#;
    let mut first = tool(b"raw", b"exec_command", raw, b"");
    data(&mut first, 3, &[0xf0, 0x9f]);
    let mut last = tool(b"string", b"write_stdin", b"bare <&>\n", b"");
    data(&mut last, 3, &[0x9a, 0x80, 0xe2, 0x82]);
    let output = consume_devin_frames_to_interactions(
        &mut Cursor::new(complete(&[first, last])),
        &context(),
    )
    .unwrap();
    let payload = std::str::from_utf8(&output.payload).unwrap();
    let args = gjson::get(payload, "steps.0.arguments");
    assert_eq!(args.json(), std::str::from_utf8(raw).unwrap());
    assert!(payload.contains("bare <&>\\n"));
    assert!(!payload.contains("\\u003c") && !payload.contains("\\u0026"));
    let text = gjson::get(payload, "steps.2.content.0.text");
    assert_eq!(text.str(), "🚀��");
    assert_eq!(
        output.observation.content,
        [0xf0, 0x9f, 0x9a, 0x80, 0xe2, 0x82]
    );
    assert_eq!(output.observation.tool_calls[0].arguments, raw);

    let mut deep = vec![b'['; 20_000];
    deep.extend_from_slice(b"null");
    deep.extend(std::iter::repeat_n(b']', 20_000));
    let frame = tool(b"deep", b"exec_command", &deep, b"");
    let output =
        consume_devin_frames_to_interactions(&mut Cursor::new(complete(&[frame])), &context())
            .unwrap();
    assert_eq!(output.observation.tool_calls[0].arguments, deep);
    assert!(output
        .payload
        .windows(deep.len())
        .any(|window| window == deep));
}
