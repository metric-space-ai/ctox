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
