// ref: internal/runtime/executor/helps/devin_wire_test.go:205-235,630-836,948-992
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::devin_proto::{
    parse_devin_frame, parse_devin_response_dimension_groups, parse_devin_tool_call_delta,
    parse_devin_usage_field, DevinDimensionUsage, DevinProtoErrorKind,
};

fn varint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        bytes.push(value as u8 | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
}

fn tag(bytes: &mut Vec<u8>, number: u32, kind: u8) {
    varint(bytes, u64::from(number) << 3 | u64::from(kind));
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

fn header(bytes: &mut Vec<u8>, key: &[u8], value: &[u8]) {
    let mut entry = Vec::new();
    data(&mut entry, 1, key);
    data(&mut entry, 2, value);
    data(bytes, 8, &entry);
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

#[test]
fn candidate_devin_proto_frame_preserves_fragments_and_upstream_fields() {
    let mut payload = Vec::new();
    data(&mut payload, 1, b"bot-uuid-123");
    number(&mut payload, 2, 7);
    let mut timestamp = Vec::new();
    number(&mut timestamp, 1, 1_791_072_000);
    number(&mut timestamp, 2, 123);
    data(&mut payload, 2, &timestamp);
    // Fragments may end mid-codepoint even inside one protobuf message.
    let text = "Hello 世界 🚀".as_bytes();
    for fragment in text.chunks(2) {
        data(&mut payload, 3, fragment);
    }
    data(&mut payload, 9, b"Let me ");
    data(&mut payload, 9, b"think...");
    data(&mut payload, 10, &[0, 0xff, 0x80]);
    data(&mut payload, 10, b"CAQS-signature-bytes");
    data(&mut payload, 21, b"anthropic");
    data(&mut payload, 17, b"message-uuid");
    number(&mut payload, 4, 11);
    number(&mut payload, 5, 2);
    tag(&mut payload, 12, 1);
    payload.extend_from_slice(&0.125_f64.to_bits().to_le_bytes());
    data(&mut payload, 28, b"dimension-wire");
    data(&mut payload, 111, b"opaque future data");
    number(&mut payload, 112, 9);
    tag(&mut payload, 113, 5);
    payload.extend_from_slice(&77_u32.to_le_bytes());
    let result = parse_devin_frame(&payload).unwrap();
    assert_eq!(result.output_id, b"bot-uuid-123");
    assert_eq!(result.timestamp, 1_791_072_000);
    assert_eq!(result.content_text, text);
    assert_eq!(result.thinking_text, b"Let me think...");
    assert_eq!(
        result.delta_signature,
        [0, 0xff, 0x80]
            .into_iter()
            .chain(b"CAQS-signature-bytes".iter().copied())
            .collect::<Vec<_>>()
    );
    assert_eq!(result.delta_signature_type, b"anthropic");
    assert_eq!(result.message_id, b"message-uuid");
    assert_eq!(
        (result.delta_tokens, result.stop_reason, result.latency),
        (11, 2, 0.125)
    );
    assert_eq!(
        result.response_dimension_groups,
        [b"dimension-wire".to_vec()]
    );
    assert_eq!(result.unknown_field_numbers, [111]);
    let debug = format!("{result:?}");
    for sensitive in ["Hello", "think...", "CAQS", "uuid", "dimension-wire"] {
        assert!(!debug.contains(sensitive));
    }

    // Raw bytes also survive across separate frames, without replacement chars.
    let mut a = Vec::new();
    let mut b = Vec::new();
    data(&mut a, 3, &[0xf0, 0x9f]);
    data(&mut b, 3, &[0x9a, 0x80]);
    let mut joined = parse_devin_frame(&a).unwrap().content_text;
    joined.extend(parse_devin_frame(&b).unwrap().content_text);
    assert_eq!(String::from_utf8(joined).unwrap(), "🚀");
}

#[test]
fn candidate_devin_proto_tool_deltas_keep_custom_and_invalid_json_fields() {
    let mut tool = Vec::new();
    data(&mut tool, 1, b"call_999");
    data(&mut tool, 2, b"custom_bash");
    data(&mut tool, 3, br#"{"cmd":"pwd"}"#);
    data(&mut tool, 4, b"pwd && ls");
    data(&mut tool, 5, b"syntax error near unexpected token");
    number(&mut tool, 6, 1);
    // Tool submessages skip valid unknown groups, unlike top-level frames.
    tag(&mut tool, 100, 3);
    number(&mut tool, 101, 42);
    tag(&mut tool, 100, 4);
    let result = parse_devin_tool_call_delta(&tool).unwrap();
    assert_eq!(result.id, b"call_999");
    assert_eq!(result.name, b"custom_bash");
    assert_eq!(result.arguments, br#"{"cmd":"pwd"}"#);
    assert_eq!(result.invalid_json_str, b"pwd && ls");
    assert_eq!(
        result.invalid_json_err,
        b"syntax error near unexpected token"
    );
    assert!(result.is_custom_tool_call);
    let debug = format!("{result:?}");
    assert!(!debug.contains("pwd") && !debug.contains("syntax") && !debug.contains("call_999"));

    let mut frame = Vec::new();
    data(&mut frame, 6, &[0x0a, 0x80]); // Bad child must not discard valid sibling.
    data(&mut frame, 6, &tool);
    data(&mut frame, 3, b"after tool");
    let parsed = parse_devin_frame(&frame).unwrap();
    assert_eq!(parsed.tool_call_deltas, [result]);
    assert_eq!(parsed.content_text, b"after tool");

    data(&mut tool, 3, b"last arguments fragment");
    number(&mut tool, 6, 0);
    let repeated = parse_devin_tool_call_delta(&tool).unwrap();
    assert_eq!(repeated.arguments, b"last arguments fragment");
    assert!(!repeated.is_custom_tool_call);
}

#[test]
fn candidate_devin_proto_usage_keeps_cache_write_separate_and_provider_headers() {
    let mut wire = Vec::new();
    for (field, value) in [(2, 3), (4, 58), (3, 39), (5, 19_179), (6, 66)] {
        number(&mut wire, field, value);
    }
    header(&mut wire, b"openai-version", b"2020-10-01");
    header(
        &mut wire,
        b"x-request-id",
        b"req_5bb00ad48ae048119e3420bddf36257f",
    );
    header(&mut wire, b"openai-processing-ms", b"419");
    data(&mut wire, 9, b"gpt-5-6-luna-low");
    let usage = parse_devin_usage_field(&wire);
    assert_eq!(
        (
            usage.prompt_tokens,
            usage.cache_write_tokens,
            usage.completion_tokens,
            usage.cached_tokens,
            usage.status_code
        ),
        (3, 58, 39, 19_179, 66)
    );
    assert_eq!(usage.request_id, b"req_5bb00ad48ae048119e3420bddf36257f");
    assert_eq!(usage.model_name, b"gpt-5-6-luna-low");
    assert_eq!(
        usage.headers.get(b"openai-version".as_slice()).unwrap(),
        b"2020-10-01"
    );
    assert_eq!(
        usage
            .headers
            .get(b"openai-processing-ms".as_slice())
            .unwrap(),
        b"419"
    );
    let mut owned = usage.clone();
    owned.headers.get_mut(b"openai-version".as_slice()).unwrap()[0] = b'9';
    assert_eq!(
        usage.headers.get(b"openai-version".as_slice()).unwrap(),
        b"2020-10-01"
    );
    assert!(!format!("{usage:?}").contains("req_5bb"));
    assert!(!format!("{usage:?}").contains("2020-10"));

    let mut anthropic = Vec::new();
    for (field, value) in [(2, 4), (3, 109), (5, 577)] {
        number(&mut anthropic, field, value);
    }
    header(
        &mut anthropic,
        b"Request-Id",
        b"req_011Cf1JivhJrXDq9ycq7cEtH",
    );
    let usage = parse_devin_usage_field(&anthropic);
    assert_eq!(
        (
            usage.prompt_tokens,
            usage.completion_tokens,
            usage.cached_tokens
        ),
        (4, 109, 577)
    );
    assert_eq!(usage.request_id, b"req_011Cf1JivhJrXDq9ycq7cEtH");
    assert!(usage.headers.contains_key(b"Request-Id".as_slice()));
}

#[test]
fn candidate_devin_proto_dimensions_support_inner_envelope_and_first_matching_group() {
    let mut group = Vec::new();
    data(&mut group, 1, b"Token Usage");
    metric(&mut group, b"input_tokens", 575.0);
    metric(&mut group, b"output_tokens", 5.0);
    metric(&mut group, b"cached_input_tokens", 128.0);
    let expected = DevinDimensionUsage {
        prompt_tokens: 575,
        completion_tokens: 5,
        cached_tokens: 128,
        found: true,
    };
    let mut root = Vec::new();
    data(&mut root, 28, &group);
    assert_eq!(
        parse_devin_response_dimension_groups(&[group.clone()]),
        expected
    );
    assert_eq!(
        parse_devin_response_dimension_groups(&[root.clone()]),
        expected
    );
    let mut unrelated = Vec::new();
    data(&mut unrelated, 1, b"Latency Metrics");
    metric(&mut unrelated, b"input_tokens", 999.0);
    assert_eq!(
        parse_devin_response_dimension_groups(&[unrelated.clone()]),
        DevinDimensionUsage::default()
    );
    assert_eq!(
        parse_devin_response_dimension_groups(&[unrelated, group.clone()]),
        expected
    );
    let mut later = Vec::new();
    data(&mut later, 1, b"tOkEn UsAgE");
    metric(&mut later, b"input_tokens", 91.75);
    metric(&mut later, b"Input_Tokens", 100.0); // Keys are exact, titles case-insensitive.
    metric(&mut later, b"input_tokens", 92.5);
    metric(&mut later, b"output_tokens", -2.75);
    assert_eq!(
        parse_devin_response_dimension_groups(&[later.clone()]),
        DevinDimensionUsage {
            prompt_tokens: 92,
            completion_tokens: -2,
            cached_tokens: 0,
            found: true
        }
    );
    assert_eq!(
        parse_devin_response_dimension_groups(&[group, later]),
        expected
    );
}

#[test]
fn candidate_devin_proto_rejects_truncated_overflow_and_invalid_top_level_wire() {
    use DevinProtoErrorKind::{
        InvalidTag, Overflow, RecursionLimit, Truncated, UnmatchedGroup, UnsupportedWireType,
    };
    let cases = [
        (vec![0], InvalidTag),
        (vec![0x80], Truncated),
        (vec![0x80; 10], Overflow),
        (vec![0x0a, 0x80], Truncated),
        (vec![0x0a, 3, b'x'], Truncated),
        (vec![0x08, 0x80], Truncated),
        (vec![0x09, 1, 2], Truncated),
        (vec![0x0d, 1, 2], Truncated),
        (vec![0x0e], UnsupportedWireType(6)),
        (vec![0x0f], UnsupportedWireType(7)),
        (vec![0x0b, 0x0c], UnsupportedWireType(3)),
    ];
    for (wire, kind) in cases {
        let error = parse_devin_frame(&wire).unwrap_err();
        assert_eq!(error.kind, kind, "{wire:?}");
        assert!(error.offset <= wire.len());
    }
    let mut oversized_tag = Vec::new();
    varint(&mut oversized_tag, 1_u64 << 32);
    assert_eq!(
        parse_devin_frame(&oversized_tag).unwrap_err().kind,
        InvalidTag
    );
    assert_eq!(
        parse_devin_tool_call_delta(&[0x0b, 0x14]).unwrap_err().kind,
        UnmatchedGroup
    );
    let nested = vec![0x0b; 10_001];
    assert_eq!(
        parse_devin_tool_call_delta(&nested).unwrap_err().kind,
        RecursionLimit
    );
    assert_eq!(parse_devin_frame(&[]).unwrap(), Default::default());
}

#[test]
fn candidate_devin_proto_partial_usage_and_timestamp_match_upstream_fallbacks() {
    let mut usage = Vec::new();
    number(&mut usage, 2, i64::MAX as u64);
    number(&mut usage, 2, 1);
    number(&mut usage, 4, u64::MAX);
    number(&mut usage, 4, 4);
    number(&mut usage, 3, 3);
    number(&mut usage, 3, 8);
    data(&mut usage, 8, b"legacy-request-id");
    header(&mut usage, b"REQUEST-ID", b"authoritative-request-id");
    data(&mut usage, 8, b"later legacy id");
    header(&mut usage, b"empty-value", b"");
    usage.push(0x80); // Upstream returns usage parsed before malformed tail.
    let parsed = parse_devin_usage_field(&usage);
    assert_eq!(parsed.prompt_tokens, i64::MIN);
    assert_eq!(parsed.cache_write_tokens, 3);
    assert_eq!(parsed.completion_tokens, 8);
    assert_eq!(parsed.request_id, b"authoritative-request-id");
    assert_eq!(
        parsed.headers.get(b"empty-value".as_slice()),
        Some(&Vec::new())
    );

    let mut bad_legacy = Vec::new();
    data(&mut bad_legacy, 8, &[0, 0xff]);
    assert!(parse_devin_usage_field(&bad_legacy).request_id.is_empty());
    let mut partial_header = Vec::new();
    data(&mut partial_header, 1, b"X-Request-ID");
    data(&mut partial_header, 2, b"partial-header-request-id");
    partial_header.push(0x80);
    let mut valid_envelope = Vec::new();
    data(&mut valid_envelope, 8, &partial_header);
    assert_eq!(
        parse_devin_usage_field(&valid_envelope).request_id,
        b"partial-header-request-id"
    );

    let mut timestamp = Vec::new();
    number(&mut timestamp, 1, 42);
    data(&mut timestamp, 9, b"stop timestamp scan here");
    number(&mut timestamp, 1, 999);
    let mut frame = Vec::new();
    data(&mut frame, 2, &timestamp);
    data(&mut frame, 7, &usage);
    let parsed = parse_devin_frame(&frame).unwrap();
    assert_eq!(parsed.timestamp, 42);
    assert_eq!(parsed.usage.unwrap().prompt_tokens, i64::MIN);
}
