// ref: internal/runtime/executor/helps/devin_wire.go:523-618,662-968
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevinProtoErrorKind {
    Truncated,
    Overflow,
    InvalidTag,
    UnsupportedWireType(u8),
    UnmatchedGroup,
    RecursionLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DevinProtoError {
    pub offset: usize,
    pub kind: DevinProtoErrorKind,
}

impl fmt::Display for DevinProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Devin protobuf error at byte {}: {:?}",
            self.offset, self.kind
        )
    }
}
impl std::error::Error for DevinProtoError {}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct DevinToolCallDelta {
    pub id: Vec<u8>,
    pub name: Vec<u8>,
    pub arguments: Vec<u8>,
    pub invalid_json_str: Vec<u8>,
    pub invalid_json_err: Vec<u8>,
    pub is_custom_tool_call: bool,
}

impl fmt::Debug for DevinToolCallDelta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinToolCallDelta")
            .field("arguments_bytes", &self.arguments.len())
            .field("is_custom_tool_call", &self.is_custom_tool_call)
            .finish()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct DevinUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
    pub cache_write_tokens: i64,
    pub status_code: u64,
    pub request_id: Vec<u8>,
    pub model_name: Vec<u8>,
    pub headers: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl fmt::Debug for DevinUsage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinUsage")
            .field("prompt_tokens", &self.prompt_tokens)
            .field("completion_tokens", &self.completion_tokens)
            .field("cached_tokens", &self.cached_tokens)
            .field("cache_write_tokens", &self.cache_write_tokens)
            .field("status_code", &self.status_code)
            .field("header_count", &self.headers.len())
            .finish()
    }
}

/// Go strings may carry incomplete UTF-8. Keep wire strings as bytes until the
/// stream's UTF-8 boundary buffer or final serialization owns their conversion.
#[derive(Clone, Default, PartialEq)]
pub struct DevinFrameResult {
    pub output_id: Vec<u8>,
    pub timestamp: u64,
    pub content_text: Vec<u8>,
    pub delta_tokens: u64,
    pub stop_reason: u64,
    pub tool_call_deltas: Vec<DevinToolCallDelta>,
    pub thinking_text: Vec<u8>,
    pub delta_signature: Vec<u8>,
    pub delta_signature_type: Vec<u8>,
    pub latency: f64,
    pub message_id: Vec<u8>,
    pub usage: Option<DevinUsage>,
    pub response_dimension_groups: Vec<Vec<u8>>,
    pub unknown_field_numbers: Vec<u32>,
}

impl fmt::Debug for DevinFrameResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinFrameResult")
            .field("timestamp", &self.timestamp)
            .field("stop_reason", &self.stop_reason)
            .field("content_bytes", &self.content_text.len())
            .field("thinking_bytes", &self.thinking_text.len())
            .field("signature_bytes", &self.delta_signature.len())
            .field("tool_count", &self.tool_call_deltas.len())
            .field("usage", &self.usage)
            .finish()
    }
}

pub(crate) enum WireValue<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
    Group,
}

pub(crate) struct WireField<'a> {
    pub(crate) number: u32,
    pub(crate) value: WireValue<'a>,
}

pub(crate) struct WireReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> WireReader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn error(&self, kind: DevinProtoErrorKind) -> DevinProtoError {
        DevinProtoError {
            offset: self.position,
            kind,
        }
    }
    fn varint(&mut self) -> Result<u64, DevinProtoError> {
        let start = self.position;
        let mut value = 0;
        for index in 0..10 {
            let Some(&byte) = self.bytes.get(self.position) else {
                return Err(DevinProtoError {
                    offset: start,
                    kind: DevinProtoErrorKind::Truncated,
                });
            };
            self.position += 1;
            if index == 9 && byte > 1 {
                return Err(DevinProtoError {
                    offset: start,
                    kind: DevinProtoErrorKind::Overflow,
                });
            }
            value |= u64::from(byte & 0x7f) << (7 * index);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(DevinProtoError {
            offset: start,
            kind: DevinProtoErrorKind::Overflow,
        })
    }
    fn tag(&mut self) -> Result<(u32, u8), DevinProtoError> {
        let value = self.varint()?;
        let number = value >> 3;
        if number == 0 || number > (1 << 29) - 1 {
            return Err(self.error(DevinProtoErrorKind::InvalidTag));
        }
        Ok((number as u32, (value & 7) as u8))
    }
    fn take(&mut self, length: usize) -> Result<&'a [u8], DevinProtoError> {
        if length > self.bytes.len() - self.position {
            return Err(self.error(DevinProtoErrorKind::Truncated));
        }
        let bytes = &self.bytes[self.position..self.position + length];
        self.position += length;
        Ok(bytes)
    }
    fn primitive(&mut self, kind: u8) -> Result<WireValue<'a>, DevinProtoError> {
        match kind {
            0 => self.varint().map(WireValue::Varint),
            1 => Ok(WireValue::Fixed64(u64::from_le_bytes(
                self.take(8)?.try_into().unwrap(),
            ))),
            2 => {
                let length = usize::try_from(self.varint()?)
                    .map_err(|_| self.error(DevinProtoErrorKind::Overflow))?;
                Ok(WireValue::Bytes(self.take(length)?))
            }
            5 => Ok(WireValue::Fixed32(u32::from_le_bytes(
                self.take(4)?.try_into().unwrap(),
            ))),
            other => Err(self.error(DevinProtoErrorKind::UnsupportedWireType(other))),
        }
    }
    fn skip_group(&mut self, opening: u32) -> Result<(), DevinProtoError> {
        // Iterative unknown-group skipping avoids growing the Rust stack.
        let mut groups = vec![opening];
        while !groups.is_empty() {
            let (number, kind) = self.tag()?;
            match kind {
                3 => {
                    if groups.len() >= 10_000 {
                        return Err(self.error(DevinProtoErrorKind::RecursionLimit));
                    }
                    groups.push(number);
                }
                4 => {
                    if groups.pop() != Some(number) {
                        return Err(self.error(DevinProtoErrorKind::UnmatchedGroup));
                    }
                }
                other => {
                    self.primitive(other)?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn next(&mut self) -> Result<Option<WireField<'a>>, DevinProtoError> {
        if self.position == self.bytes.len() {
            return Ok(None);
        }
        let (number, kind) = self.tag()?;
        let value = if kind == 3 {
            self.skip_group(number)?;
            WireValue::Group
        } else {
            self.primitive(kind)?
        };
        Ok(Some(WireField { number, value }))
    }
}

pub fn parse_devin_frame(payload: &[u8]) -> Result<DevinFrameResult, DevinProtoError> {
    let mut result = DevinFrameResult::default();
    let mut reader = WireReader::new(payload);
    while let Some(field) = reader.next()? {
        match field.value {
            WireValue::Varint(value) => match field.number {
                2 => result.timestamp = value,
                4 => result.delta_tokens = value,
                5 => result.stop_reason = value,
                _ => {}
            },
            WireValue::Fixed64(value) => {
                if field.number == 12 {
                    result.latency = f64::from_bits(value);
                }
            }
            WireValue::Fixed32(_) => {}
            WireValue::Bytes(bytes) => match field.number {
                1 => result.output_id = bytes.to_vec(),
                2 => result.timestamp = parse_devin_timestamp(bytes),
                3 => result.content_text.extend_from_slice(bytes),
                6 => {
                    if let Ok(delta) = parse_devin_tool_call_delta(bytes) {
                        result.tool_call_deltas.push(delta);
                    }
                }
                7 => result.usage = Some(parse_devin_usage_field(bytes)),
                9 => result.thinking_text.extend_from_slice(bytes),
                10 => result.delta_signature.extend_from_slice(bytes),
                17 => result.message_id = bytes.to_vec(),
                21 => result.delta_signature_type = bytes.to_vec(),
                28 => result.response_dimension_groups.push(bytes.to_vec()),
                _ => result.unknown_field_numbers.push(field.number),
            },
            WireValue::Group => {
                return Err(reader.error(DevinProtoErrorKind::UnsupportedWireType(3)))
            }
        }
    }
    Ok(result)
}

pub fn parse_devin_tool_call_delta(bytes: &[u8]) -> Result<DevinToolCallDelta, DevinProtoError> {
    let mut result = DevinToolCallDelta::default();
    let mut reader = WireReader::new(bytes);
    while let Some(field) = reader.next()? {
        match field.value {
            WireValue::Varint(value) if field.number == 6 => {
                result.is_custom_tool_call = value != 0
            }
            WireValue::Bytes(value) => match field.number {
                1 => result.id = value.to_vec(),
                2 => result.name = value.to_vec(),
                3 => result.arguments = value.to_vec(),
                4 => result.invalid_json_str = value.to_vec(),
                5 => result.invalid_json_err = value.to_vec(),
                _ => {}
            },
            _ => {}
        }
    }
    Ok(result)
}

fn parse_devin_timestamp(bytes: &[u8]) -> u64 {
    let mut result = 0;
    let mut reader = WireReader::new(bytes);
    while let Ok(Some(field)) = reader.next() {
        let WireValue::Varint(value) = field.value else {
            break;
        };
        if field.number == 1 {
            result = value;
        }
    }
    result
}

fn parse_devin_header_field(bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let (mut key, mut value) = (Vec::new(), Vec::new());
    let mut reader = WireReader::new(bytes);
    while let Ok(Some(field)) = reader.next() {
        if let WireValue::Bytes(bytes) = field.value {
            match field.number {
                1 => key = bytes.to_vec(),
                2 => value = bytes.to_vec(),
                _ => {}
            }
        }
    }
    (key, value)
}

pub fn parse_devin_usage_field(bytes: &[u8]) -> DevinUsage {
    let mut result = DevinUsage::default();
    let mut reader = WireReader::new(bytes);
    while let Ok(Some(field)) = reader.next() {
        match field.value {
            WireValue::Varint(value) => match field.number {
                2 => result.prompt_tokens = result.prompt_tokens.wrapping_add(value as i64),
                3 => result.completion_tokens = value as i64,
                4 => {
                    result.cache_write_tokens = result.cache_write_tokens.wrapping_add(value as i64)
                }
                5 => result.cached_tokens = value as i64,
                6 => result.status_code = value,
                _ => {}
            },
            WireValue::Bytes(value) if field.number == 8 => {
                let (key, val) = parse_devin_header_field(value);
                if !key.is_empty() {
                    if (key.eq_ignore_ascii_case(b"x-request-id")
                        || key.eq_ignore_ascii_case(b"request-id"))
                        && !val.is_empty()
                    {
                        result.request_id = val.clone();
                    }
                    result.headers.insert(key, val);
                } else if !value.is_empty()
                    && value.iter().all(|byte| (0x20..=0x7e).contains(byte))
                    && result.request_id.is_empty()
                {
                    result.request_id = value.to_vec();
                }
            }
            WireValue::Bytes(value) if field.number == 9 => result.model_name = value.to_vec(),
            _ => {}
        }
    }
    result
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DevinDimensionUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
    pub found: bool,
}

pub fn parse_devin_response_dimension_groups(groups: &[Vec<u8>]) -> DevinDimensionUsage {
    for bytes in groups {
        if bytes.is_empty() {
            continue;
        }
        let mut first = WireReader::new(bytes);
        let bytes = match first.next() {
            Ok(Some(WireField {
                number: 28,
                value: WireValue::Bytes(inner),
            })) => inner,
            _ => bytes.as_slice(),
        };
        let mut title = Vec::new();
        let mut metrics = Vec::new();
        let mut reader = WireReader::new(bytes);
        while let Ok(Some(field)) = reader.next() {
            if let WireValue::Bytes(bytes) = field.value {
                if field.number == 1 {
                    title = bytes.to_vec();
                } else if field.number == 2 {
                    let (mut key, mut value) = (Vec::new(), 0.0_f32);
                    let mut metric = WireReader::new(bytes);
                    while let Ok(Some(field)) = metric.next() {
                        if let WireValue::Bytes(bytes) = field.value {
                            if field.number == 5 {
                                key = bytes.to_vec();
                            } else if field.number == 4 {
                                let mut scalar = WireReader::new(bytes);
                                while let Ok(Some(field)) = scalar.next() {
                                    if let WireField {
                                        number: 2,
                                        value: WireValue::Fixed32(bits),
                                    } = field
                                    {
                                        value = f32::from_bits(bits);
                                    }
                                }
                            }
                        }
                    }
                    if !key.is_empty() {
                        metrics.push((key, value));
                    }
                }
            }
        }
        if title.eq_ignore_ascii_case(b"Token Usage") {
            let mut result = DevinDimensionUsage::default();
            for (key, value) in metrics {
                match key.as_slice() {
                    b"input_tokens" => result.prompt_tokens = value as i64,
                    b"output_tokens" => result.completion_tokens = value as i64,
                    b"cached_input_tokens" => result.cached_tokens = value as i64,
                    _ => continue,
                }
                result.found = true;
            }
            if result.found {
                return result;
            }
        }
    }
    DevinDimensionUsage::default()
}
