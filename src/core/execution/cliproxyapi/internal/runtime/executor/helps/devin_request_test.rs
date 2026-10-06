// ref: internal/runtime/executor/helps/devin_wire_test.go:37-204,333-372,455-565,830-924
// ref: internal/translator/common/devin_tools_test.go:1-99
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::build_sensitive_word_matcher;
use super::devin_request::*;
use std::sync::Arc;

#[derive(Debug, PartialEq)]
enum Value {
    Number(u64),
    Data(Vec<u8>),
    Float(f64),
}

fn take_varint(wire: &[u8], position: &mut usize) -> u64 {
    let mut result = 0_u64;
    for shift in (0..70).step_by(7) {
        let byte = wire[*position];
        *position += 1;
        result |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return result;
        }
    }
    panic!("invalid fixture varint");
}

fn fields(wire: &[u8]) -> Vec<(u32, Value)> {
    let mut position = 0;
    let mut result = Vec::new();
    while position < wire.len() {
        let tag = take_varint(wire, &mut position);
        let value = match tag & 7 {
            0 => Value::Number(take_varint(wire, &mut position)),
            1 => {
                let bits = u64::from_le_bytes(wire[position..position + 8].try_into().unwrap());
                position += 8;
                Value::Float(f64::from_bits(bits))
            }
            2 => {
                let length = take_varint(wire, &mut position) as usize;
                let value = wire[position..position + length].to_vec();
                position += length;
                Value::Data(value)
            }
            _ => panic!("unexpected request fixture wire type"),
        };
        result.push(((tag >> 3) as u32, value));
    }
    result
}

fn data(wire: &[u8], field: u32) -> Vec<u8> {
    fields(wire)
        .into_iter()
        .find_map(|(number, value)| match value {
            Value::Data(bytes) if number == field => Some(bytes),
            _ => None,
        })
        .unwrap_or_default()
}

fn number(wire: &[u8], field: u32) -> Option<u64> {
    fields(wire)
        .into_iter()
        .find_map(|(number, value)| match value {
            Value::Number(value) if number == field => Some(value),
            _ => None,
        })
}

fn input<'a>(prompts: &'a [DevinPrompt], tools: &'a [DevinTool]) -> DevinChatRequest<'a> {
    DevinChatRequest {
        session_token: "token-123",
        device_seed: "device-seed-1",
        chat_model_uid: "swe-2-high",
        system_prompt: "you are a helpful assistant",
        prompts,
        tools,
        temperature: Some(0.7),
        max_tokens: 4_000,
        session_id: "session-1",
        cascade_id: "cascade-1",
        matcher: None,
    }
}

#[test]
fn candidate_devin_request_metadata_fingerprint_and_trace_match_protocol() {
    let fingerprint = generate_devin_device_fingerprint("seed-1");
    assert_eq!(fingerprint.len(), DEVIN_FINGERPRINT_HEX_LEN);
    assert!(fingerprint
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    assert!(
        fingerprint.starts_with("d357188a574c72ec26aae785af2ad8a233ed3b03b1242519c9aa16d986f075f7")
    );
    assert!(fingerprint.ends_with("9e56134aa0e183bf6580db99494e"));
    assert_eq!(fingerprint, generate_devin_device_fingerprint("seed-1"));
    assert_ne!(fingerprint, generate_devin_device_fingerprint("seed-2"));
    let random_a = generate_devin_device_fingerprint("");
    let random_b = generate_devin_device_fingerprint("");
    assert_eq!(random_a.len(), 732);
    assert_ne!(random_a, random_b);
    let metadata = build_devin_client_metadata_bytes("token-123", "seed-1", "linux");
    assert_eq!(
        fields(&metadata)
            .iter()
            .map(|(number, _)| *number)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 7, 12, 31]
    );
    for (field, expected) in [
        (1, "chisel"),
        (2, "3000.10.21"),
        (3, "token-123"),
        (4, "en"),
        (5, "linux"),
        (7, "3000.10.21"),
        (12, "chisel"),
    ] {
        assert_eq!(data(&metadata, field), expected.as_bytes());
    }
    assert_eq!(data(&metadata, 31), fingerprint.as_bytes());
    let trace_a = generate_devin_sentry_trace();
    let trace_b = generate_devin_sentry_trace();
    assert_ne!(trace_a, trace_b);
    let pieces: Vec<_> = trace_a.split('-').collect();
    assert_eq!(
        pieces.iter().map(|part| part.len()).collect::<Vec<_>>(),
        [32, 16, 1]
    );
    assert_eq!(pieces[2], "1");
    assert!(pieces[..2].iter().all(|part| part
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())));
    #[cfg(target_os = "macos")]
    assert_eq!(
        data(&build_devin_client_metadata_bytes("", "seed", ""), 5),
        b"darwin"
    );
}

#[test]
fn candidate_devin_request_full_history_tool_images_signature_and_completion_fields() {
    let prompts = [
        DevinPrompt {
            message_id: "u1".into(),
            source: 1,
            content: "hello 世界".into(),
            images: vec![
                DevinImage {
                    base64_data: "  iVBORw0  ".into(),
                    mime_type: "  ".into(),
                },
                DevinImage {
                    base64_data: "  ".into(),
                    mime_type: "image/jpeg".into(),
                },
            ],
            ..Default::default()
        },
        DevinPrompt {
            message_id: "a1".into(),
            source: 2,
            content: "hi there".into(),
            thinking: "thinking step".into(),
            signature: vec![0, 0xff, 0x80],
            signature_type: "anthropic".into(),
            tool_calls: vec![DevinToolCall {
                id: "call-1".into(),
                name: "get_weather".into(),
                arguments: r#"{"city":"Berlin"}"#.into(),
            }],
            ..Default::default()
        },
        DevinPrompt {
            message_id: "t1".into(),
            source: 4,
            content: r#"{"result":"ok"}"#.into(),
            tool_call_id: "call-1".into(),
            ..Default::default()
        },
    ];
    let tools = [DevinTool {
        name: "get_weather".into(),
        description: "lookup weather".into(),
        parameters: br#"{"type":"object"}"#.to_vec(),
    }];
    let turns = DevinSessionTurns::default();
    let wire = build_devin_get_chat_message_request(&input(&prompts, &tools), &turns);
    assert_eq!(
        fields(&wire)
            .iter()
            .map(|(field, _)| *field)
            .collect::<Vec<_>>(),
        [1, 2, 3, 3, 3, 7, 8, 10, 15, 16, 20, 21]
    );
    let history: Vec<_> = fields(&wire)
        .into_iter()
        .filter_map(|(field, value)| match value {
            Value::Data(bytes) if field == 3 => Some(bytes),
            _ => None,
        })
        .collect();
    assert_eq!(data(&history[0], 1), b"u1");
    assert_eq!(number(&history[0], 2), Some(1));
    assert_eq!(data(&history[0], 3), "hello 世界".as_bytes());
    assert_eq!(
        fields(&history[0])
            .iter()
            .filter(|(field, _)| *field == 10)
            .count(),
        1
    );
    let image = data(&history[0], 10);
    assert_eq!(data(&image, 1), b"iVBORw0");
    assert_eq!(data(&image, 2), b"image/png");
    let tool = data(&history[1], 6);
    assert_eq!(data(&tool, 1), b"call-1");
    assert_eq!(data(&tool, 2), b"get_weather");
    assert_eq!(data(&tool, 3), br#"{"city":"Berlin"}"#);
    assert_eq!(data(&history[1], 11), b"thinking step");
    assert_eq!(data(&history[1], 12), [0, 0xff, 0x80]);
    assert_eq!(data(&history[1], 18), b"anthropic");
    assert_eq!(number(&history[2], 2), Some(4));
    assert_eq!(data(&history[2], 7), b"call-1");
    assert_eq!(number(&wire, 7), Some(5));
    assert_eq!(
        fields(&data(&wire, 8)),
        [
            (1, Value::Number(1)),
            (2, Value::Number(4_000)),
            (3, Value::Number(400)),
            (5, Value::Float(0.7)),
            (7, Value::Number(40)),
            (8, Value::Float(0.95_f32 as f64))
        ]
    );
    assert_eq!(data(&wire, 16), b"cascade-1");
    assert_eq!(number(&wire, 20), Some(1));
    assert_eq!(data(&wire, 21), b"swe-2-high");
    let session = data(&wire, 15);
    assert_eq!(number(&session, 2), None);
    assert_eq!(number(&session, 3), Some(4));
    assert_eq!(number(&session, 4), None);
}

#[test]
fn candidate_devin_request_defaults_generate_valid_ids_and_keep_cache_identity() {
    let prompts = [DevinPrompt {
        source: 0,
        ..Default::default()
    }];
    let turns = DevinSessionTurns::default();
    let mut request = input(&prompts, &[]);
    request.session_id = "";
    request.cascade_id = "";
    request.max_tokens = -1;
    request.temperature = None;
    let wire = build_devin_get_chat_message_request(&request, &turns);
    let session = data(&data(&wire, 15), 1);
    assert!(uuid::Uuid::parse_str(std::str::from_utf8(&session).unwrap()).is_ok());
    assert_eq!(data(&wire, 16), session);
    let history = data(&wire, 3);
    assert!(uuid::Uuid::parse_str(std::str::from_utf8(&data(&history, 1)).unwrap()).is_ok());
    assert_eq!(number(&history, 2), Some(1));
    // Upstream boundary test uses the original source, before source=0 defaults.
    assert_eq!(number(&data(&wire, 15), 4), None);
    let completion = fields(&data(&wire, 8));
    assert!(completion.contains(&(2, Value::Number(128_000))));
    assert!(completion.contains(&(5, Value::Float(1.0))));
    request.session_id = "same-session";
    let a = build_devin_get_chat_message_request(&request, &turns);
    let b = build_devin_get_chat_message_request(&request, &turns);
    assert_eq!(data(&a, 16), b"same-session");
    assert_eq!(data(&a, 16), data(&b, 16));
    assert_eq!(number(&data(&b, 15), 2), Some(1));
}

#[test]
fn candidate_devin_request_system_filter_does_not_modify_history_or_arguments() {
    let matcher = build_sensitive_word_matcher(&["SECRET_TOKEN".into()]).unwrap();
    let raw = "x-anthropic-billing-header: cc_version=2.1.260;\r\nYou are Claude Code, Anthropic's official CLI for Claude.\r\nSystem prompt containing SECRET_TOKEN\r\n  Keep the project instructions.  ";
    assert_eq!(
        sanitize_devin_system_prompt(raw, Some(&matcher)),
        "Keep the project instructions."
    );
    let prompts = [DevinPrompt {
        source: 2,
        content: "calling tool with SECRET_TOKEN in content".into(),
        tool_calls: vec![DevinToolCall {
            id: "call-1".into(),
            name: "test_tool".into(),
            arguments: r#"{"key":"SECRET_TOKEN"}"#.into(),
        }],
        ..Default::default()
    }];
    let mut request = input(&prompts, &[]);
    request.system_prompt = raw;
    request.matcher = Some(&matcher);
    let wire = build_devin_get_chat_message_request(&request, &DevinSessionTurns::default());
    assert_eq!(data(&wire, 2), b"Keep the project instructions.");
    let history = data(&wire, 3);
    assert_eq!(
        data(&history, 3),
        b"calling tool with SECRET_TOKEN in content"
    );
    assert_eq!(data(&data(&history, 6), 3), br#"{"key":"SECRET_TOKEN"}"#);
    assert_eq!(sanitize_devin_system_prompt("  \r\n  ", None), "");
}

#[test]
fn candidate_devin_request_turn_counters_are_bounded_isolated_and_atomic() {
    let turns = DevinSessionTurns::with_capacity(2);
    assert_eq!(turns.next(" a "), 0);
    assert_eq!(turns.next("a"), 1);
    assert_eq!(turns.next("b"), 0);
    assert_eq!(turns.next("a"), 2); // touch a, so b is evicted next
    assert_eq!(turns.next("c"), 0);
    assert_eq!(turns.next("a"), 3);
    assert_eq!(turns.next("b"), 0);
    turns.reset(" a ");
    assert_eq!(turns.next("a"), 0);
    assert_eq!(turns.next("  "), 0);
    assert_eq!(DevinSessionTurns::default().next("a"), 0);
    let shared = Arc::new(DevinSessionTurns::default());
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let turns = shared.clone();
            std::thread::spawn(move || (0..25).map(|_| turns.next("shared")).collect::<Vec<_>>())
        })
        .collect();
    let mut values: Vec<_> = threads
        .into_iter()
        .flat_map(|thread| thread.join().unwrap())
        .collect();
    values.sort_unstable();
    assert_eq!(values, (0..50).collect::<Vec<_>>());

    let prompts = [DevinPrompt {
        source: 1,
        ..Default::default()
    }];
    let cache = DevinSessionTurns::default();
    let first = build_devin_get_chat_message_request(&input(&prompts, &[]), &cache);
    let second = build_devin_get_chat_message_request(&input(&prompts, &[]), &cache);
    assert_eq!(number(&data(&first, 15), 2), None);
    assert_eq!(number(&data(&second, 15), 2), Some(1));
    assert_eq!(number(&data(&first, 15), 4), Some(14));
    assert_eq!(number(&data(&second, 15), 4), Some(14));
    let adjacent_users = [
        DevinPrompt {
            source: 1,
            ..Default::default()
        },
        DevinPrompt {
            source: 1,
            ..Default::default()
        },
    ];
    let third = build_devin_get_chat_message_request(&input(&adjacent_users, &[]), &cache);
    assert_eq!(number(&data(&third, 15), 2), Some(2));
    assert_eq!(number(&data(&third, 15), 4), None);
}

#[test]
fn candidate_devin_request_tools_follow_qualified_filter_and_description_contract() {
    for (namespace, name, expected) in [
        ("mcp__codex_app", "automation_update", true),
        ("MCP__CODEX_APP", "AUTOMATION_UPDATE", true),
        (" mcp__codex_app ", " automation_update ", true),
        ("", "mcp__codex_app__automation_update", true),
        ("other_namespace", "automation_update", false),
        ("", "automation_update", false),
        ("mcp__codex_app", "exec_command", false),
    ] {
        assert_eq!(
            is_devin_codex_app_automation_update(namespace, name),
            expected
        );
    }
    let exec =
        "Runs a command in a bash shell, returning output or a session ID for ongoing interaction.";
    let stdin = "Writes characters to an existing unified exec session and returns recent output.";
    assert_eq!(sanitize_devin_tool_description("exec_command", exec), "Runs a command in a bash shell, returning output or an session ID for ongoing interaction.");
    let fixed = sanitize_devin_tool_description(" mcp__codex_app__exec_command ", exec);
    assert_eq!(
        sanitize_devin_tool_description("exec_command", &fixed),
        fixed
    );
    assert_eq!(sanitize_devin_tool_description("other_tool", stdin), stdin);
    assert_eq!(
        sanitize_devin_tool_description("WRITE_STDIN", &stdin.to_uppercase()),
        "Writes characters to a existing unified exec session and returns recent output."
    );
    assert_eq!(
        sanitize_devin_tool_description("write_stdin", stdin.trim_end_matches('.')),
        "Writes characters to a existing unified exec session and returns recent output"
    );
    assert_eq!(
        sanitize_devin_tool_description(
            "exec_command",
            "RETURNING OUTPUT OR A ſESSION ID FOR ONGOING INTERACTION"
        ),
        "returning output or an session ID for ongoing interaction"
    );
    // Preserve upstream's exact-match fast-path and idempotence precedence.
    let mixed = format!("{exec} RETURNING OUTPUT OR A SESSION ID FOR ONGOING INTERACTION");
    let fixed = sanitize_devin_tool_description("exec_command", &mixed);
    assert!(fixed.ends_with("RETURNING OUTPUT OR A SESSION ID FOR ONGOING INTERACTION"));
    let tools = [
        DevinTool {
            name: "mcp__codex_app__automation_update".into(),
            ..Default::default()
        },
        DevinTool {
            name: "".into(),
            ..Default::default()
        },
        DevinTool {
            name: "exec_command".into(),
            description: exec.into(),
            ..Default::default()
        },
        DevinTool {
            name: "write_stdin".into(),
            description: stdin.into(),
            ..Default::default()
        },
        DevinTool {
            name: "subagent".into(),
            description: "Takes a task_id parameter identifying the task".into(),
            ..Default::default()
        },
    ];
    let wire =
        build_devin_get_chat_message_request(&input(&[], &tools), &DevinSessionTurns::default());
    let encoded: Vec<_> = fields(&wire)
        .into_iter()
        .filter_map(|(field, value)| match value {
            Value::Data(bytes) if field == 10 => Some(bytes),
            _ => None,
        })
        .collect();
    assert_eq!(encoded.len(), 3);
    assert_eq!(data(&encoded[0], 1), b"exec_command");
    assert!(String::from_utf8(data(&encoded[0], 2))
        .unwrap()
        .contains("an session ID"));
    assert_eq!(
        data(&encoded[1], 2),
        b"Writes characters to a existing unified exec session and returns recent output."
    );
    assert_eq!(
        data(&encoded[2], 2),
        b"Takes a taskId parameter identifying the task"
    );
}

#[test]
fn candidate_devin_request_utf8_buffer_handles_every_split_and_invalid_bytes() {
    let text = "hello 世界 🚀".as_bytes();
    for split in 0..=text.len() {
        let mut buffer = DevinUtf8SplitBuffer::default();
        let mut joined = buffer.feed(&text[..split]);
        assert!(buffer.pending_bytes().len() <= 3);
        assert!(std::str::from_utf8(&joined).is_ok());
        joined.extend(buffer.feed(&text[split..]));
        assert_eq!(joined, text);
        assert!(buffer.pending_bytes().is_empty());
    }
    let raw = [b'A', 0xff, 0xf4, 0x90, b'B', 0xf0, 0x9f, 0x9a, 0x80];
    for split in 0..=raw.len() {
        let mut buffer = DevinUtf8SplitBuffer::default();
        let mut joined = buffer.feed(&raw[..split]);
        joined.extend(buffer.feed(&raw[split..]));
        assert_eq!(joined, raw);
        assert!(buffer.pending_bytes().is_empty());
    }
    let mut buffer = DevinUtf8SplitBuffer::default();
    assert_eq!(buffer.feed(&[b'x', 0xe4, 0xb8]), b"x");
    assert_eq!(buffer.pending_bytes(), [0xe4, 0xb8]);
    assert!(buffer.feed(&[]).is_empty());
    assert_eq!(buffer.pending_bytes(), [0xe4, 0xb8]);
    assert_eq!(buffer.feed(&[0xad]), "中".as_bytes());
    assert!(buffer.pending_bytes().is_empty());
}
