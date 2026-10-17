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
