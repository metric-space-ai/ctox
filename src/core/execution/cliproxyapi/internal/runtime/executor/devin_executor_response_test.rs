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
