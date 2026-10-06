use ctox_cliproxyapi::internal::translator::common::{
    claude_input_tokens_json, gemini_token_count_json, join_raw_array, set_top_level_string,
    sse_event_data, JsonField, RawJson, SseDecoder,
};
use ctox_cliproxyapi::protocol::{ContentPart, StreamEvent, Usage};

#[test]
fn upstream_token_count_shapes_are_byte_exact() {
    assert_eq!(claude_input_tokens_json(42), br#"{"input_tokens":42}"#);
    assert_eq!(
        gemini_token_count_json(42),
        br#"{"totalTokens":42,"promptTokensDetails":[{"modality":"TEXT","tokenCount":42}]}"#,
    );
}

#[test]
fn raw_array_join_does_not_reencode_tool_arguments() {
    let items = vec![br#"{"n":1.00}"#.to_vec(), br#"{"s":"\\u0061"}"#.to_vec()];
    assert_eq!(join_raw_array(&items), br#"[{"n":1.00},{"s":"\\u0061"}]"#);
}

#[test]
fn model_noop_preserves_whitespace_and_bytes() {
    let input = br#"{ "model" : "gpt-5", "input": [ ] }"#;
    assert_eq!(set_top_level_string(input, "model", "gpt-5"), input);
    assert_eq!(
        set_top_level_string(b"not-json", "model", "gpt-5"),
        b"not-json"
    );
}

#[test]
fn missing_null_and_value_are_distinct() {
    assert_eq!(JsonField::<u8>::from_value(None), JsonField::Missing);
    assert_eq!(JsonField::<u8>::from_value(Some(None)), JsonField::Null);
    assert_eq!(JsonField::from_value(Some(Some(4_u8))), JsonField::Value(4));
}

#[test]
fn sse_decoder_handles_fragmentation_crlf_and_multiline_data() {
    let mut decoder = SseDecoder::new();
    assert!(decoder.push(b"event: response.delta\r\nda").is_empty());
    let events = decoder.push(b"ta: one\r\ndata: two\r\n\r\n");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event.as_deref(), Some("response.delta"));
    assert_eq!(events[0].data, b"one\ntwo");
}

#[test]
fn sse_decoder_ignores_invalid_retry_and_clears_empty_event_fields() {
    let mut decoder = SseDecoder::new();
    let events =
        decoder.push(b"retry: 1500\nretry: nope\ndata: first\n\nevent: stale\n\ndata: second\r");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].retry_millis, Some(1500));
    let finished = decoder.finish();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].event, None);
    assert_eq!(finished[0].data, b"second");
}

#[test]
fn sse_encoder_matches_upstream_shape() {
    assert_eq!(
        sse_event_data("response.done", br#"{"ok":true}"#),
        b"event: response.done\ndata: {\"ok\":true}\n\n"
    );
}

#[test]
fn protocol_keeps_raw_tool_arguments_and_ordered_stream_events() {
    let arguments = RawJson::parse(br#"{"n":1.00}"#.to_vec()).unwrap();
    let item = ContentPart::ToolCall {
        id: "call_1".into(),
        name: "calc".into(),
        arguments,
    };
    let events = [
        StreamEvent::OutputItemStarted {
            index: 0,
            item: item.clone(),
        },
        StreamEvent::OutputItemFinished { index: 0, item },
        StreamEvent::Usage(Usage {
            input_tokens: 3,
            output_tokens: 2,
            ..Usage::default()
        }),
    ];
    assert!(matches!(
        events[0],
        StreamEvent::OutputItemStarted { index: 0, .. }
    ));
    assert!(matches!(events[2], StreamEvent::Usage(_)));
}
