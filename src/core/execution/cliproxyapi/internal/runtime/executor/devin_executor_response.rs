// ref: internal/runtime/executor/devin_executor.go:462,1124-1485
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::helps::devin_proto::{
    parse_devin_frame, parse_devin_response_dimension_groups, DevinFrameResult, DevinToolCallDelta,
    DevinUsage,
};
use super::helps::devin_wire::{
    parse_devin_trailer_error, read_connect_frame, ConnectFrame, ConnectFrameError,
    DevinTrailerError, CONNECT_FLAG_END_STREAM,
};
use crate::internal::translator::common::{
    join_raw_array, set_json_string, set_raw_path, set_string_without_html_escape,
};
use crate::internal::util::{responses_tool_reverse_identity_map, valid_json_bytes};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::Read;

pub const MAX_DEVIN_TOOL_CALLS: usize = 128;
pub const MAX_DEVIN_AGGREGATE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Default, PartialEq, Eq)]
pub struct DevinAggregatedToolCall {
    pub id: Vec<u8>,
    pub name: Vec<u8>,
    pub arguments: Vec<u8>,
    pub legacy: bool,
}

impl fmt::Debug for DevinAggregatedToolCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinAggregatedToolCall")
            .field("arguments_bytes", &self.arguments.len())
            .field("legacy", &self.legacy)
            .finish()
    }
}

#[derive(Clone, Default)]
pub struct DevinAggregateObservation {
    pub frames_count: usize,
    pub content: Vec<u8>,
    pub thinking: Vec<u8>,
    pub signature: Vec<u8>,
    pub signature_type: Vec<u8>,
    pub tool_calls: Vec<DevinAggregatedToolCall>,
    pub usage: Option<DevinUsage>,
    pub unknown_fields: Vec<u32>,
    pub stop_reason: u64,
}

impl fmt::Debug for DevinAggregateObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinAggregateObservation")
            .field("frames_count", &self.frames_count)
            .field("content_bytes", &self.content.len())
            .field("thinking_bytes", &self.thinking.len())
            .field("signature_bytes", &self.signature.len())
            .field("tool_count", &self.tool_calls.len())
            .field("usage", &self.usage)
            .field("stop_reason", &self.stop_reason)
            .finish()
    }
}

#[derive(Clone)]
pub enum DevinAggregateError {
    Connect(ConnectFrameError),
    Trailer(DevinTrailerError),
    PrematureEof,
    LegacyApplyPatch,
    ResponseTooLarge,
}

impl DevinAggregateError {
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Trailer(error) => error.status_code,
            _ => 502,
        }
    }
}

impl fmt::Display for DevinAggregateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(error) => write!(f, "{error}"),
            Self::Trailer(error) => write!(f, "{error}"),
            Self::PrematureEof => {
                f.write_str("devin upstream stream terminated prematurely before EOS trailer")
            }
            Self::LegacyApplyPatch => {
                f.write_str("Devin apply_patch tool returned an invalid legacy argument payload")
            }
            Self::ResponseTooLarge => {
                f.write_str("Devin aggregate response exceeds the host byte limit")
            }
        }
    }
}

impl fmt::Debug for DevinAggregateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trailer(error) => f
                .debug_struct("Trailer")
                .field("status_code", &error.status_code)
                .finish(),
            other => write!(f, "{other}"),
        }
    }
}
impl std::error::Error for DevinAggregateError {}

#[derive(Debug)]
pub struct DevinAggregateFailure {
    pub error: DevinAggregateError,
    pub observation: DevinAggregateObservation,
}

impl fmt::Display for DevinAggregateFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}
impl std::error::Error for DevinAggregateFailure {}

pub struct DevinAggregateOutput {
    pub payload: Vec<u8>,
    pub observation: DevinAggregateObservation,
}

impl fmt::Debug for DevinAggregateOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinAggregateOutput")
            .field("payload_bytes", &self.payload.len())
            .field("observation", &self.observation)
            .finish()
    }
}

/// The winning original declaration determines the tool contract.
/// It cannot be inferred from untrusted response headers or a model's tool name.
pub struct DevinAggregateContext<'a> {
    pub model: &'a str,
    pub original_request: &'a [u8],
}

pub struct DevinInteractionAccumulator {
    pre_tool_text: Vec<u8>,
    post_tool_text: Vec<u8>,
    thinking: Vec<u8>,
    signature: Vec<u8>,
    signature_type: Vec<u8>,
    tools: Vec<DevinAggregatedToolCall>,
    call_ids: HashMap<Vec<u8>, usize>,
    last_builder: Option<usize>,
    usage: Option<DevinUsage>,
    unknown_fields: Vec<u32>,
    seen_unknown: HashSet<u32>,
    stop_reason: u64,
    frames_count: usize,
    received_bytes: usize,
    maximum_bytes: usize,
    saw_eos: bool,
    terminal_error: Option<DevinAggregateError>,
}

impl Default for DevinInteractionAccumulator {
    fn default() -> Self {
        Self::with_byte_limit(MAX_DEVIN_AGGREGATE_BYTES)
    }
}

impl DevinInteractionAccumulator {
    pub(super) fn with_byte_limit(maximum_bytes: usize) -> Self {
        assert!(maximum_bytes > 0 && maximum_bytes <= MAX_DEVIN_AGGREGATE_BYTES);
        Self {
            pre_tool_text: Vec::new(),
            post_tool_text: Vec::new(),
            thinking: Vec::new(),
            signature: Vec::new(),
            signature_type: Vec::new(),
            tools: Vec::new(),
            call_ids: HashMap::new(),
            last_builder: None,
            usage: None,
            unknown_fields: Vec::new(),
            seen_unknown: HashSet::new(),
            stop_reason: 0,
            frames_count: 0,
            received_bytes: 0,
            maximum_bytes,
            saw_eos: false,
            terminal_error: None,
        }
    }

    /// Returns true exactly on a successfully received EOS trailer. Invalid
    /// protobuf data frames are skipped as upstream does; Connect errors are fatal.
    pub fn accept(&mut self, frame: ConnectFrame) -> Result<bool, DevinAggregateError> {
        if let Some(error) = &self.terminal_error {
            return Err(error.clone());
        }
        let result = self.accept_frame(frame);
        if let Err(error) = &result {
            self.terminal_error = Some(error.clone());
        }
        result
    }

    fn accept_frame(&mut self, frame: ConnectFrame) -> Result<bool, DevinAggregateError> {
        if self.saw_eos {
            return Ok(true);
        }
        self.frames_count += 1;
        self.received_bytes = self
            .received_bytes
            .checked_add(frame.payload.len())
            .ok_or(DevinAggregateError::ResponseTooLarge)?;
        if self.received_bytes > self.maximum_bytes {
            return Err(DevinAggregateError::ResponseTooLarge);
        }
        if frame.flag & CONNECT_FLAG_END_STREAM != 0 {
            if let Some(error) = parse_devin_trailer_error(&frame.payload) {
                return Err(DevinAggregateError::Trailer(error));
            }
            self.saw_eos = true;
            return Ok(true);
        }
        let Ok(parsed) = parse_devin_frame(&frame.payload) else {
            return Ok(false);
        };
        self.apply(parsed);
        Ok(false)
    }

    fn apply(&mut self, frame: DevinFrameResult) {
        if frame.stop_reason != 0 {
            self.stop_reason = frame.stop_reason;
        }
        for number in frame.unknown_field_numbers {
            if self.seen_unknown.insert(number) {
                self.unknown_fields.push(number);
            }
        }
        update_devin_usage(
            &mut self.usage,
            frame.usage,
            &frame.response_dimension_groups,
        );

        self.signature.extend(frame.delta_signature);
        if !frame.delta_signature_type.is_empty() {
            self.signature_type = frame.delta_signature_type;
        }
        self.thinking.extend(frame.thinking_text);
        for delta in frame.tool_call_deltas {
            self.apply_tool(delta);
        }
        if self.tools.is_empty() {
            self.pre_tool_text.extend(frame.content_text);
        } else {
            self.post_tool_text.extend(frame.content_text);
        }
    }

    fn apply_tool(&mut self, delta: DevinToolCallDelta) {
        let existing = if delta.id.is_empty() {
            self.last_builder
        } else {
            self.call_ids.get(&delta.id).copied()
        };
        let index = if let Some(index) = existing {
            self.last_builder = Some(index);
            if !delta.name.is_empty() {
                self.tools[index].name = delta.name;
            }
            index
        } else {
            if self.tools.len() >= MAX_DEVIN_TOOL_CALLS {
                return;
            }
            let index = self.tools.len();
            self.tools.push(DevinAggregatedToolCall {
                id: delta.id.clone(),
                name: delta.name,
                ..Default::default()
            });
            if !delta.id.is_empty() {
                self.call_ids.insert(delta.id, index);
            }
            self.last_builder = Some(index);
            index
        };
        let arguments = if delta.arguments.is_empty() {
            if !delta.invalid_json_str.is_empty() {
                self.tools[index].legacy = true;
            }
            delta.invalid_json_str
        } else {
            delta.arguments
        };
        self.tools[index].arguments.extend(arguments);
    }

    pub fn observation(&self) -> DevinAggregateObservation {
        let mut content = self.pre_tool_text.clone();
        content.extend_from_slice(&self.post_tool_text);
        DevinAggregateObservation {
            frames_count: self.frames_count,
            content,
            thinking: self.thinking.clone(),
            signature: self.signature.clone(),
            signature_type: self.signature_type.clone(),
            tool_calls: self
                .tools
                .iter()
                .filter(|tool| {
                    !tool.id.is_empty() || !tool.name.is_empty() || !tool.arguments.is_empty()
                })
                .cloned()
                .collect(),
            usage: self.usage.clone(),
            unknown_fields: self.unknown_fields.clone(),
            stop_reason: self.stop_reason,
        }
    }

    fn failure(&self, error: DevinAggregateError) -> DevinAggregateFailure {
        DevinAggregateFailure {
            error,
            observation: self.observation(),
        }
    }

    pub fn finish(
        self,
        context: &DevinAggregateContext<'_>,
    ) -> Result<DevinAggregateOutput, DevinAggregateFailure> {
        if let Some(error) = &self.terminal_error {
            return Err(self.failure(error.clone()));
        }
        if self.tools.iter().any(|tool| tool.legacy) {
            let original_tools = responses_tool_reverse_identity_map(context.original_request);
            if self.tools.iter().any(|tool| {
                tool.legacy
                    && original_tools
                        .get(&go_utf8_text(&tool.name))
                        .is_some_and(|identity| identity.apply_patch)
            }) {
                return Err(self.failure(DevinAggregateError::LegacyApplyPatch));
            }
        }
        if !self.saw_eos {
            return Err(self.failure(DevinAggregateError::PrematureEof));
        }
        let observation = self.observation();
        let mut steps = Vec::new();
        if !self.thinking.is_empty() || !self.signature.is_empty() {
            let mut thought = br#"{"type":"thought"}"#.to_vec();
            if !self.thinking.is_empty() {
                thought = set_raw_path(&thought, "content", &text_content(&self.thinking));
            }
            if !self.signature.is_empty() {
                let signature = go_utf8_text(&self.signature);
                thought = set_json_string(&thought, "signature", &signature);
                thought = set_json_string(&thought, "thought_signature", &signature);
            }
            steps.push(thought);
        }
        if !self.pre_tool_text.is_empty() {
            steps.push(text_step(&self.pre_tool_text));
        }
        for tool in &observation.tool_calls {
            let mut call =
                br#"{"type":"function_call","name":"","id":"","call_id":"","arguments":{}}"#
                    .to_vec();
            call = set_json_string(&call, "name", &go_utf8_text(&tool.name));
            call = set_json_string(&call, "id", &go_utf8_text(&tool.id));
            call = set_json_string(&call, "call_id", &go_utf8_text(&tool.id));
            if !tool.arguments.is_empty() {
                let arguments = go_utf8_text(&tool.arguments);
                call = if valid_json_bytes(arguments.as_bytes()) {
                    set_raw_path(&call, "arguments", arguments.as_bytes())
                } else {
                    set_string_without_html_escape(&call, "arguments", &arguments)
                };
            }
            steps.push(call);
        }
        if !self.post_tool_text.is_empty() {
            steps.push(text_step(&self.post_tool_text));
        }
        let mut payload = br#"{"id":"","model":"","status":"completed","steps":[],"usage":{"total_input_tokens":0,"total_output_tokens":0,"total_cached_tokens":0}}"#.to_vec();
        let id = format!("interaction_{}", &uuid::Uuid::new_v4().to_string()[..12]);
        payload = set_json_string(&payload, "id", &id);
        payload = set_json_string(&payload, "model", context.model);
        if let Some(reason) = match self.stop_reason {
            1 | 3 => Some("length"),
            11 => Some("content_filter"),
            _ => None,
        } {
            payload = set_json_string(&payload, "status", "incomplete");
            payload = set_json_string(&payload, "finish_reason", reason);
        }
        payload = set_raw_path(&payload, "steps", &join_raw_array(&steps));
        if let Some(usage) = &self.usage {
            let input = usage.prompt_tokens.wrapping_add(usage.cached_tokens);
            let total = input.wrapping_add(usage.completion_tokens);
            payload = set_raw_path(
                &payload,
                "usage.total_input_tokens",
                input.to_string().as_bytes(),
            );
            payload = set_raw_path(
                &payload,
                "usage.total_output_tokens",
                usage.completion_tokens.to_string().as_bytes(),
            );
            payload = set_raw_path(
                &payload,
                "usage.total_cached_tokens",
                usage.cached_tokens.to_string().as_bytes(),
            );
            if usage.cache_write_tokens > 0 {
                payload = set_raw_path(
                    &payload,
                    "usage.cache_write_tokens",
                    usage.cache_write_tokens.to_string().as_bytes(),
                );
            }
            payload = set_raw_path(&payload, "usage.total_tokens", total.to_string().as_bytes());
        }
        Ok(DevinAggregateOutput {
            payload,
            observation,
        })
    }
}

fn text_content(text: &[u8]) -> Vec<u8> {
    let block = set_json_string(br#"{"type":"text","text":""}"#, "text", &go_utf8_text(text));
    join_raw_array(&[block])
}

fn text_step(text: &[u8]) -> Vec<u8> {
    set_raw_path(
        br#"{"type":"model_output"}"#,
        "content",
        &text_content(text),
    )
}

/// Merge observed usage without replacing known counts with missing values.
pub(super) fn update_devin_usage(
    current: &mut Option<DevinUsage>,
    incoming: Option<DevinUsage>,
    dimensions: &[Vec<u8>],
) {
    // ref: devin_executor.go:843-906 — later positive counts replace; zero
    // dimensions only fill missing values; the first HTTP status is retained.
    if let Some(usage) = incoming {
        if let Some(previous) = current {
            if usage.prompt_tokens > 0 {
                previous.prompt_tokens = usage.prompt_tokens;
            }
            if usage.completion_tokens > 0 {
                previous.completion_tokens = usage.completion_tokens;
            }
            if usage.cached_tokens > 0 {
                previous.cached_tokens = usage.cached_tokens;
            }
            if usage.cache_write_tokens > 0 {
                previous.cache_write_tokens = usage.cache_write_tokens;
            }
            if !usage.request_id.is_empty() {
                previous.request_id = usage.request_id;
            }
            if !usage.model_name.is_empty() {
                previous.model_name = usage.model_name;
            }
            previous.headers.extend(usage.headers);
        } else {
            *current = Some(usage);
        }
    }
    if !dimensions.is_empty()
        && current.as_ref().is_none_or(|usage| {
            usage.prompt_tokens == 0 || usage.completion_tokens == 0 || usage.cached_tokens == 0
        })
    {
        let dimensions = parse_devin_response_dimension_groups(dimensions);
        if dimensions.found {
            let usage = current.get_or_insert_with(DevinUsage::default);
            if usage.prompt_tokens == 0 {
                usage.prompt_tokens = dimensions.prompt_tokens;
            }
            if usage.completion_tokens == 0 {
                usage.completion_tokens = dimensions.completion_tokens;
            }
            if usage.cached_tokens == 0 {
                usage.cached_tokens = dimensions.cached_tokens;
            }
        }
    }
}

/// Go's JSON writer replaces each invalid UTF-8 byte independently. Rust's
/// from_utf8_lossy may replace several bytes at once, so preserve Go's boundary.
pub(crate) fn go_utf8_text(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut position = 0;
    while position < bytes.len() {
        match std::str::from_utf8(&bytes[position..]) {
            Ok(text) => {
                result.push_str(text);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                result.push_str(std::str::from_utf8(&bytes[position..position + valid]).unwrap());
                position += valid;
                let invalid = error.error_len().unwrap_or(bytes.len() - position);
                for _ in 0..invalid {
                    result.push('\u{fffd}');
                }
                position += invalid;
            }
        }
    }
    result
}

pub fn consume_devin_frames_to_interactions(
    body: &mut impl Read,
    context: &DevinAggregateContext<'_>,
) -> Result<DevinAggregateOutput, DevinAggregateFailure> {
    let mut state = DevinInteractionAccumulator::default();
    loop {
        let frame = match read_connect_frame(body) {
            Ok(frame) => frame,
            Err(ConnectFrameError::EndOfInput) => break,
            Err(error) => return Err(state.failure(DevinAggregateError::Connect(error))),
        };
        match state.accept(frame) {
            Ok(true) => break,
            Ok(false) => {}
            Err(error) => return Err(state.failure(error)),
        }
    }
    state.finish(context)
}
