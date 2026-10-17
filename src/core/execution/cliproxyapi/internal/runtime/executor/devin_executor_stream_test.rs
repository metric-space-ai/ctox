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
