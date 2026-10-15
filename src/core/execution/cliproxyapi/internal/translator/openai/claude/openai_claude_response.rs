// ref: internal/translator/openai/claude/openai_claude_response.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

//! Faithful Rust port of upstream `ConvertOpenAIResponseToClaude`,
//! `ConvertOpenAIResponseToClaudeNonStream`, and `ClaudeTokenCount`.
//!
//! Streaming behavior is the most subtle part of the upstream code. We
//! preserve the same ordering of events, the same suppress-when-empty rules
//! for tool calls (null/non-string/empty name, missing id), the same belated
//! emit at finish_reason / [DONE] time, and the same `SawToolCall` flag that
//! decides whether the final `stop_reason` becomes `tool_calls` (when we
//! actually emitted a `content_block_start` of type `tool_use`) or the
//! raw OpenAI reason otherwise.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::internal::translator::common::{append_sse_event, claude_input_tokens_json};
use crate::internal::util::sanitize_claude_tool_id;
use crate::sdk::translator::TranslationContext;

/// Mirrors the upstream `ConvertOpenAIResponseToAnthropicParams` struct.
/// All state lives in the request-local `TranslationState`; we never use
/// process globals or hidden authority.
pub struct OpenAIToClaudeStreamState {
    pub message_id: String,
    pub model: String,
    pub created_at: i64,
    pub tool_name_map: Option<BTreeMap<String, String>>,
    /// True once at least one `tool_use` `content_block_start` has been
    /// emitted on the wire. Upstream documents that using raw upstream
    /// `tool_calls` presence here can produce `stop_reason=tool_use` with zero
    /// announced tool blocks, so we gate the wire `tool_use` reason on the
    /// announced count instead.
    pub saw_tool_call: bool,
    pub content_accumulator: String,
    tool_calls_accumulator: BTreeMap<usize, ToolCallAccumulator>,
    pub text_content_block_started: bool,
    pub thinking_content_block_started: bool,
    pub finish_reason: String,
    pub content_blocks_stopped: bool,
    pub message_delta_sent: bool,
    pub message_started: bool,
    pub message_stop_sent: bool,
    pub tool_call_block_indexes: BTreeMap<usize, usize>,
    pub text_content_block_index: i64,
    pub thinking_content_block_index: i64,
    pub next_content_block_index: usize,
}

impl Default for OpenAIToClaudeStreamState {
    fn default() -> Self {
        Self {
            message_id: String::new(),
            model: String::new(),
            created_at: 0,
            tool_name_map: None,
            saw_tool_call: false,
            content_accumulator: String::new(),
            tool_calls_accumulator: BTreeMap::new(),
            text_content_block_started: false,
            thinking_content_block_started: false,
            finish_reason: String::new(),
            content_blocks_stopped: false,
            message_delta_sent: false,
            message_started: false,
            message_stop_sent: false,
            tool_call_block_indexes: BTreeMap::new(),
            text_content_block_index: -1,
            thinking_content_block_index: -1,
            next_content_block_index: 0,
        }
    }
}

#[derive(Default, Clone)]
struct ToolCallAccumulator {
    id: String,
    name: String,
    arguments: String,
    start_emitted: bool,
}

const DATA_PREFIX: &[u8] = b"data:";
const MESSAGE_START_EVENT: &str = "message_start";
const CONTENT_BLOCK_START_EVENT: &str = "content_block_start";
const CONTENT_BLOCK_DELTA_EVENT: &str = "content_block_delta";
const CONTENT_BLOCK_STOP_EVENT: &str = "content_block_stop";
const MESSAGE_DELTA_EVENT: &str = "message_delta";
const MESSAGE_STOP_EVENT: &str = "message_stop";

/// Streaming entry point. Mirrors upstream `ConvertOpenAIResponseToClaude`.
pub fn convert_openai_response_to_claude(
    _context: &TranslationContext,
    _model_name: &str,
    original_request_raw_json: &[u8],
    _request_raw_json: &[u8],
    raw_json: &[u8],
    state: &mut OpenAIToClaudeStreamState,
) -> Vec<Vec<u8>> {
    if !raw_json.starts_with(DATA_PREFIX) {
        return Vec::new();
    }
    let trimmed = trim_ascii(&raw_json[DATA_PREFIX.len()..]);
    if trimmed == b"[DONE]" {
        return convert_openai_done_to_anthropic(state);
    }

    if state.tool_name_map.is_none() {
        state.tool_name_map = tool_name_map_from_claude_request(original_request_raw_json);
    }

    let stream_requested = gjson::get(
        std::str::from_utf8(original_request_raw_json).unwrap_or(""),
        "stream",
    );
    let stream_is_false = stream_requested.kind() == gjson::Kind::False;
    // Some OpenAI-compatible providers omit `stream` from the echoed/original
    // request even though the wire is unambiguously an SSE delta. Respect an
    // explicit false, otherwise let the response shape disambiguate it.
    let payload_is_stream_delta = serde_json::from_slice::<Value>(trimmed)
        .ok()
        .and_then(|root| root.pointer("/choices/0/delta").cloned())
        .is_some();
    if stream_is_false || (!stream_requested.exists() && !payload_is_stream_delta) {
        return convert_openai_non_streaming_to_anthropic(trimmed);
    }
    convert_openai_streaming_chunk_to_anthropic(trimmed, state)
}

/// Non-streaming entry point. Mirrors upstream
/// `ConvertOpenAIResponseToClaudeNonStream`.
pub fn convert_openai_response_to_claude_non_stream(
    _context: &TranslationContext,
    _model_name: &str,
    original_request_raw_json: &[u8],
    _request_raw_json: &[u8],
    raw_json: &[u8],
    _state: &mut OpenAIToClaudeStreamState,
) -> Vec<u8> {
    let root = serde_json::from_slice::<Value>(raw_json).unwrap_or(Value::Null);
    let tool_name_map = tool_name_map_from_claude_request(original_request_raw_json);
    let mut out: Vec<u8> =
        br#"{"id":"","type":"message","role":"assistant","model":"","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":0,"output_tokens":0}}"#
            .to_vec();
    out = upsert_top_level_string(
        &out,
        "id",
        root.get("id").and_then(Value::as_str).unwrap_or(""),
    );
    out = upsert_top_level_string(
        &out,
        "model",
        root.get("model").and_then(Value::as_str).unwrap_or(""),
    );

    let mut has_tool_call = false;
    let mut stop_reason_set = false;

    if let Some(choices) = root.get("choices").and_then(Value::as_array) {
        if let Some(choice) = choices.first() {
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                out = upsert_top_level_string(
                    &out,
                    "stop_reason",
                    &map_openai_finish_reason_to_anthropic(reason),
                );
                stop_reason_set = true;
            }

            if let Some(message) = choice.get("message") {
                // Anthropic exposes thinking before the visible answer. This
                // also matches the streaming block order.
                if let Some(reasoning) = message.get("reasoning_content") {
                    for reasoning_text in collect_openai_reasoning_texts(reasoning) {
                        if reasoning_text.is_empty() {
                            continue;
                        }
                        let raw = format!(
                            r#"{{"type":"thinking","thinking":"{}"}}"#,
                            escape_json_string(&reasoning_text)
                        );
                        out = set_raw_bytes(&out, "content.-1", raw.as_bytes());
                    }
                }

                if let Some(content_result) = message.get("content") {
                    if let Some(parts) = content_result.as_array() {
                        let mut text_builder = String::new();
                        let mut thinking_builder = String::new();

                        let flush_text = |builder: &mut String, out: &mut Vec<u8>| {
                            if builder.is_empty() {
                                return;
                            }
                            let raw = format!(
                                r#"{{"type":"text","text":"{}"}}"#,
                                escape_json_string(builder)
                            );
                            *out = set_raw_bytes(out, "content.-1", raw.as_bytes());
                            builder.clear();
                        };

                        for item in parts {
                            let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
                            match kind {
                                "text" => {
                                    flush_text(&mut text_builder, &mut out);
                                    text_builder.push_str(
                                        item.get("text").and_then(Value::as_str).unwrap_or(""),
                                    );
                                }
                                "tool_calls" => {
                                    flush_text(&mut text_builder, &mut out);
                                    if !thinking_builder.is_empty() {
                                        let raw = format!(
                                            r#"{{"type":"thinking","thinking":"{}"}}"#,
                                            escape_json_string(&thinking_builder)
                                        );
                                        out = set_raw_bytes(&out, "content.-1", raw.as_bytes());
                                        thinking_builder.clear();
                                    }
                                    if let Some(tool_calls) =
                                        item.get("tool_calls").and_then(Value::as_array)
                                    {
                                        for tc in tool_calls {
                                            has_tool_call = true;
                                            let id =
                                                tc.get("id").and_then(Value::as_str).unwrap_or("");
                                            let name = tc
                                                .get("function")
                                                .and_then(|v| v.get("name"))
                                                .and_then(Value::as_str)
                                                .unwrap_or("");
                                            let mapped_name =
                                                map_tool_name(tool_name_map.as_ref(), name);
                                            let args_str = fix_json(
                                                tc.get("function")
                                                    .and_then(|v| v.get("arguments"))
                                                    .and_then(Value::as_str)
                                                    .unwrap_or(""),
                                            );
                                            out = append_tool_use_block(
                                                &out,
                                                id,
                                                &mapped_name,
                                                &args_str,
                                            );
                                        }
                                    }
                                }
                                "reasoning" => {
                                    // Flush any text first.
                                    flush_text(&mut text_builder, &mut out);
                                    if let Some(thinking) = item.get("text").and_then(Value::as_str)
                                    {
                                        thinking_builder.push_str(thinking);
                                    }
                                }
                                _ => {
                                    flush_text(&mut text_builder, &mut out);
                                    if !thinking_builder.is_empty() {
                                        let raw = format!(
                                            r#"{{"type":"thinking","thinking":"{}"}}"#,
                                            escape_json_string(&thinking_builder)
                                        );
                                        out = set_raw_bytes(&out, "content.-1", raw.as_bytes());
                                        thinking_builder.clear();
                                    }
                                }
                            }
                        }
                        if !thinking_builder.is_empty() {
                            let raw = format!(
                                r#"{{"type":"thinking","thinking":"{}"}}"#,
                                escape_json_string(&thinking_builder)
                            );
                            out = set_raw_bytes(&out, "content.-1", raw.as_bytes());
                        }
                        if !text_builder.is_empty() {
                            let raw = format!(
                                r#"{{"type":"text","text":"{}"}}"#,
                                escape_json_string(&text_builder)
                            );
                            out = set_raw_bytes(&out, "content.-1", raw.as_bytes());
                        }
                    } else if let Some(text) = content_result.as_str() {
                        if !text.is_empty() {
                            let raw = format!(
                                r#"{{"type":"text","text":"{}"}}"#,
                                escape_json_string(text)
                            );
                            out = set_raw_bytes(&out, "content.-1", raw.as_bytes());
                        }
                    }
                }

                if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
                    for tool_call in tool_calls {
                        has_tool_call = true;
                        let id = tool_call.get("id").and_then(Value::as_str).unwrap_or("");
                        let name = tool_call
                            .get("function")
                            .and_then(|v| v.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let mapped_name = map_tool_name(tool_name_map.as_ref(), name);
                        let args_str = fix_json(
                            tool_call
                                .get("function")
                                .and_then(|v| v.get("arguments"))
                                .and_then(Value::as_str)
                                .unwrap_or(""),
                        );
                        out = append_tool_use_block(&out, id, &mapped_name, &args_str);
                    }
                }
            }
        }
    }

    if let Some(usage) = root.get("usage") {
        let (input_tokens, output_tokens, cached_tokens) = extract_openai_usage(usage);
        out = upsert_path_integer(&out, &["usage", "input_tokens"], input_tokens);
        out = upsert_path_integer(&out, &["usage", "output_tokens"], output_tokens);
        if cached_tokens > 0 {
            out = upsert_path_integer(&out, &["usage", "cache_read_input_tokens"], cached_tokens);
        }
    }

    if !stop_reason_set {
        let reason = if has_tool_call {
            "tool_use"
        } else {
            "end_turn"
        };
        out = upsert_top_level_string(&out, "stop_reason", reason);
    }

    out
}

/// Token count JSON. Mirrors upstream `ClaudeTokenCount`.
pub fn claude_token_count(_context: &TranslationContext, count: i64) -> Vec<u8> {
    claude_input_tokens_json(count)
}

// -- streaming ---------------------------------------------------------------

fn convert_openai_streaming_chunk_to_anthropic(
    raw_json: &[u8],
    state: &mut OpenAIToClaudeStreamState,
) -> Vec<Vec<u8>> {
    let Ok(text) = std::str::from_utf8(raw_json) else {
        return Vec::new();
    };
    // `gjson::get(text, "")` is not equivalent to Go's `gjson.ParseBytes`:
    // it produces a missing value, so every path lookup below silently fails.
    // Parse the SSE payload as the root value before applying relative paths.
    let root = gjson::parse(text);
    let mut results: Vec<Vec<u8>> = Vec::new();

    if state.message_id.is_empty() {
        if let Some(id) = root_get_str(&root, "id") {
            state.message_id = id.to_owned();
        }
    }
    if state.model.is_empty() {
        if let Some(model) = root_get_str(&root, "model") {
            state.model = model.to_owned();
        }
    }
    if state.created_at == 0 {
        let value = root.get("created");
        if value.kind() == gjson::Kind::Number {
            state.created_at = value.i64();
        }
    }

    if let Some(delta) = root_get(&root, "choices.0.delta") {
        if delta.exists() && !state.message_started {
            // Send message_start on the very first chunk, regardless of
            // whether the delta carries a role field (matches upstream
            // comment about Copilot tool_calls-in-first-chunk).
            let mut message_start: Vec<u8> =
                br#"{"type":"message_start","message":{"id":"","type":"message","role":"assistant","model":"","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":0,"output_tokens":0}}}"#
                    .to_vec();
            message_start =
                upsert_path_string(&message_start, &["message", "id"], &state.message_id);
            message_start = upsert_path_string(&message_start, &["message", "model"], &state.model);
            results.push(encode_sse_event(MESSAGE_START_EVENT, &message_start));
            state.message_started = true;
        }

        if let Some(reasoning) = root_get(&delta, "reasoning_content") {
            if reasoning.exists() {
                for reasoning_text in collect_openai_reasoning_texts_gjson(&reasoning) {
                    if reasoning_text.is_empty() {
                        continue;
                    }
                    stop_text_content_block(state, &mut results);
                    if !state.thinking_content_block_started {
                        if state.thinking_content_block_index == -1 {
                            state.thinking_content_block_index =
                                state.next_content_block_index as i64;
                            state.next_content_block_index += 1;
                        }
                        let mut content_block_start: Vec<u8> =
                            br#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#
                                .to_vec();
                        content_block_start = upsert_path_integer(
                            &content_block_start,
                            &["index"],
                            state.thinking_content_block_index,
                        );
                        results.push(encode_sse_event(
                            CONTENT_BLOCK_START_EVENT,
                            &content_block_start,
                        ));
                        state.thinking_content_block_started = true;
                    }

                    let mut thinking_delta: Vec<u8> =
                        br#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":""}}"#
                            .to_vec();
                    thinking_delta = upsert_path_integer(
                        &thinking_delta,
                        &["index"],
                        state.thinking_content_block_index,
                    );
                    thinking_delta = upsert_path_string(
                        &thinking_delta,
                        &["delta", "thinking"],
                        &reasoning_text,
                    );
                    results.push(encode_sse_event(CONTENT_BLOCK_DELTA_EVENT, &thinking_delta));
                }
            }
        }

        if let Some(content) = root_get(&delta, "content") {
            if content.exists() && content.kind() == gjson::Kind::String {
                let text = content.str();
                if !text.is_empty() {
                    if !state.text_content_block_started {
                        stop_thinking_content_block(state, &mut results);
                        if state.text_content_block_index == -1 {
                            state.text_content_block_index = state.next_content_block_index as i64;
                            state.next_content_block_index += 1;
                        }
                        let mut content_block_start: Vec<u8> =
                                br#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#
                                    .to_vec();
                        content_block_start = upsert_path_integer(
                            &content_block_start,
                            &["index"],
                            state.text_content_block_index,
                        );
                        results.push(encode_sse_event(
                            CONTENT_BLOCK_START_EVENT,
                            &content_block_start,
                        ));
                        state.text_content_block_started = true;
                    }

                    let mut content_delta: Vec<u8> =
                            br#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":""}}"#
                                .to_vec();
                    content_delta = upsert_path_integer(
                        &content_delta,
                        &["index"],
                        state.text_content_block_index,
                    );
                    content_delta = upsert_path_string(&content_delta, &["delta", "text"], text);
                    results.push(encode_sse_event(CONTENT_BLOCK_DELTA_EVENT, &content_delta));
                    state.content_accumulator.push_str(text);
                }
            }
        }

        if let Some(tool_calls) = root_get(&delta, "tool_calls") {
            if tool_calls.exists() && tool_calls.kind() == gjson::Kind::Array {
                tool_calls.each(|_, tool_call| {
                    let index = tool_call.get("index").i64() as usize;
                    let mut accumulator = state
                        .tool_calls_accumulator
                        .remove(&index)
                        .unwrap_or_default();

                    let id_value = tool_call.get("id");
                    if id_value.kind() == gjson::Kind::String {
                        let id = id_value.str();
                        if !id.is_empty() {
                            accumulator.id = id.to_owned();
                        }
                    }
                    if let Some(function) = root_get(&tool_call, "function") {
                        if function.exists() {
                            if !accumulator.start_emitted {
                                let name_value = function.get("name");
                                if name_value.kind() == gjson::Kind::String {
                                    let name = name_value.str();
                                    if !name.is_empty() {
                                        accumulator.name =
                                            map_tool_name(state.tool_name_map.as_ref(), name);
                                    }
                                }
                            }
                            let args_value = function.get("arguments");
                            if args_value.kind() == gjson::Kind::String {
                                let args = args_value.str();
                                if !args.is_empty() {
                                    accumulator.arguments.push_str(args);
                                }
                            }
                        }
                    }

                    if !accumulator.start_emitted
                        && !accumulator.name.is_empty()
                        && !accumulator.id.is_empty()
                        && !state.content_blocks_stopped
                    {
                        emit_tool_use_start(state, index, &mut accumulator, &mut results);
                    }
                    state.tool_calls_accumulator.insert(index, accumulator);
                    true
                });
            }
        }
    }

    if let Some(finish_reason) = root_get(&root, "choices.0.finish_reason") {
        if finish_reason.kind() == gjson::Kind::String {
            let reason = finish_reason.str();
            if !reason.is_empty() {
                let mapped = match () {
                    _ if state.saw_tool_call => "tool_calls",
                    _ if reason == "tool_calls" => "stop",
                    _ => reason,
                };
                state.finish_reason = mapped.to_owned();

                if state.thinking_content_block_started {
                    let mut stop: Vec<u8> = br#"{"type":"content_block_stop","index":0}"#.to_vec();
                    stop =
                        upsert_path_integer(&stop, &["index"], state.thinking_content_block_index);
                    results.push(encode_sse_event(CONTENT_BLOCK_STOP_EVENT, &stop));
                    state.thinking_content_block_started = false;
                    state.thinking_content_block_index = -1;
                }

                stop_text_content_block(state, &mut results);

                if !state.content_blocks_stopped {
                    let indexes = tool_call_accumulator_indexes(&state.tool_calls_accumulator);
                    for index in indexes {
                        if let Some(accumulator) = state.tool_calls_accumulator.get(&index) {
                            let mut accumulator = accumulator.clone();
                            if !accumulator.start_emitted {
                                if accumulator.name.is_empty() {
                                    continue;
                                }
                                emit_tool_use_start(state, index, &mut accumulator, &mut results);
                            }
                            let block_index = tool_content_block_index(state, index) as i64;
                            if !accumulator.arguments.is_empty() {
                                let mut input_delta: Vec<u8> = br#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":""}}"#.to_vec();
                                input_delta =
                                    upsert_path_integer(&input_delta, &["index"], block_index);
                                let fixed = fix_json(&accumulator.arguments);
                                input_delta = upsert_path_string(
                                    &input_delta,
                                    &["delta", "partial_json"],
                                    &fixed,
                                );
                                results.push(encode_sse_event(
                                    CONTENT_BLOCK_DELTA_EVENT,
                                    &input_delta,
                                ));
                            }
                            let mut content_block_stop: Vec<u8> =
                                br#"{"type":"content_block_stop","index":0}"#.to_vec();
                            content_block_stop =
                                upsert_path_integer(&content_block_stop, &["index"], block_index);
                            results.push(encode_sse_event(
                                CONTENT_BLOCK_STOP_EVENT,
                                &content_block_stop,
                            ));
                            state.tool_call_block_indexes.remove(&index);
                        }
                    }
                    state.content_blocks_stopped = true;
                }
            }
        }
    }

    if !state.finish_reason.is_empty() && !state.message_delta_sent {
        if let Some(usage) = root_get(&root, "usage") {
            if usage.exists() && usage.kind() != gjson::Kind::Null {
                let (input_tokens, output_tokens, cached_tokens) =
                    extract_openai_usage_gjson(&usage);
                let mut message_delta: Vec<u8> =
                    br#"{"type":"message_delta","delta":{"stop_reason":"","stop_sequence":null},"usage":{"input_tokens":0,"output_tokens":0}}"#
                        .to_vec();
                let stop_reason =
                    map_openai_finish_reason_to_anthropic(&effective_openai_finish_reason(state));
                message_delta =
                    upsert_path_string(&message_delta, &["delta", "stop_reason"], &stop_reason);
                message_delta =
                    upsert_path_integer(&message_delta, &["usage", "input_tokens"], input_tokens);
                message_delta =
                    upsert_path_integer(&message_delta, &["usage", "output_tokens"], output_tokens);
                if cached_tokens > 0 {
                    message_delta = upsert_path_integer(
                        &message_delta,
                        &["usage", "cache_read_input_tokens"],
                        cached_tokens,
                    );
                }
                results.push(encode_sse_event(MESSAGE_DELTA_EVENT, &message_delta));
                state.message_delta_sent = true;
                emit_message_stop_if_needed(state, &mut results);
            }
        }
    }

    results
}

fn convert_openai_done_to_anthropic(state: &mut OpenAIToClaudeStreamState) -> Vec<Vec<u8>> {
    let mut results: Vec<Vec<u8>> = Vec::new();

    if state.thinking_content_block_started {
        let mut stop: Vec<u8> = br#"{"type":"content_block_stop","index":0}"#.to_vec();
        stop = upsert_path_integer(&stop, &["index"], state.thinking_content_block_index);
        results.push(encode_sse_event(CONTENT_BLOCK_STOP_EVENT, &stop));
        state.thinking_content_block_started = false;
        state.thinking_content_block_index = -1;
    }

    stop_text_content_block(state, &mut results);

    if !state.content_blocks_stopped {
        let indexes = tool_call_accumulator_indexes(&state.tool_calls_accumulator);
        for index in indexes {
            if let Some(accumulator) = state.tool_calls_accumulator.get(&index) {
                let mut accumulator = accumulator.clone();
                if !accumulator.start_emitted {
                    if accumulator.name.is_empty() {
                        continue;
                    }
                    emit_tool_use_start(state, index, &mut accumulator, &mut results);
                }
                let block_index = tool_content_block_index(state, index) as i64;
                if !accumulator.arguments.is_empty() {
                    let mut input_delta: Vec<u8> = br#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":""}}"#.to_vec();
                    input_delta = upsert_path_integer(&input_delta, &["index"], block_index);
                    let fixed = fix_json(&accumulator.arguments);
                    input_delta =
                        upsert_path_string(&input_delta, &["delta", "partial_json"], &fixed);
                    results.push(encode_sse_event(CONTENT_BLOCK_DELTA_EVENT, &input_delta));
                }
                let mut content_block_stop: Vec<u8> =
                    br#"{"type":"content_block_stop","index":0}"#.to_vec();
                content_block_stop =
                    upsert_path_integer(&content_block_stop, &["index"], block_index);
                results.push(encode_sse_event(
                    CONTENT_BLOCK_STOP_EVENT,
                    &content_block_stop,
                ));
                state.tool_call_block_indexes.remove(&index);
            }
        }
        state.content_blocks_stopped = true;
    }

    if !state.finish_reason.is_empty() && !state.message_delta_sent {
        let mut message_delta: Vec<u8> =
            br#"{"type":"message_delta","delta":{"stop_reason":"","stop_sequence":null},"usage":{"input_tokens":0,"output_tokens":0}}"#
                .to_vec();
        let stop_reason =
            map_openai_finish_reason_to_anthropic(&effective_openai_finish_reason(state));
        message_delta = upsert_path_string(&message_delta, &["delta", "stop_reason"], &stop_reason);
        results.push(encode_sse_event(MESSAGE_DELTA_EVENT, &message_delta));
        state.message_delta_sent = true;
    }

    emit_message_stop_if_needed(state, &mut results);
    results
}

fn convert_openai_non_streaming_to_anthropic(raw_json: &[u8]) -> Vec<Vec<u8>> {
    let Ok(root) = serde_json::from_slice::<Value>(raw_json) else {
        return Vec::new();
    };
    let mut out: Vec<u8> =
        br#"{"id":"","type":"message","role":"assistant","model":"","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":0,"output_tokens":0}}"#
            .to_vec();
    out = upsert_top_level_string(
        &out,
        "id",
        root.get("id").and_then(Value::as_str).unwrap_or(""),
    );
    out = upsert_top_level_string(
        &out,
        "model",
        root.get("model").and_then(Value::as_str).unwrap_or(""),
    );

    if let Some(choices) = root.get("choices").and_then(Value::as_array) {
        if let Some(choice) = choices.first() {
            if let Some(reasoning_node) = choice
                .get("message")
                .and_then(|v| v.get("reasoning_content"))
            {
                for reasoning_text in collect_openai_reasoning_texts(reasoning_node) {
                    if reasoning_text.is_empty() {
                        continue;
                    }
                    let raw = format!(
                        r#"{{"type":"thinking","thinking":"{}"}}"#,
                        escape_json_string(&reasoning_text)
                    );
                    out = set_raw_bytes(&out, "content.-1", raw.as_bytes());
                }
            }
            if let Some(content) = choice.get("message").and_then(|v| v.get("content")) {
                if let Some(text) = content.as_str() {
                    if !text.is_empty() {
                        let raw =
                            format!(r#"{{"type":"text","text":"{}"}}"#, escape_json_string(text));
                        out = set_raw_bytes(&out, "content.-1", raw.as_bytes());
                    }
                }
            }
            if let Some(tool_calls) = choice
                .get("message")
                .and_then(|v| v.get("tool_calls"))
                .and_then(Value::as_array)
            {
                for tool_call in tool_calls {
                    let id = tool_call.get("id").and_then(Value::as_str).unwrap_or("");
                    let name = tool_call
                        .get("function")
                        .and_then(|v| v.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let args_str = fix_json(
                        tool_call
                            .get("function")
                            .and_then(|v| v.get("arguments"))
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    );
                    out = append_tool_use_block(&out, id, name, &args_str);
                }
            }
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                out = upsert_top_level_string(
                    &out,
                    "stop_reason",
                    &map_openai_finish_reason_to_anthropic(reason),
                );
            }
        }
    }

    if let Some(usage) = root.get("usage") {
        let (input_tokens, output_tokens, cached_tokens) = extract_openai_usage(usage);
        out = upsert_path_integer(&out, &["usage", "input_tokens"], input_tokens);
        out = upsert_path_integer(&out, &["usage", "output_tokens"], output_tokens);
        if cached_tokens > 0 {
            out = upsert_path_integer(&out, &["usage", "cache_read_input_tokens"], cached_tokens);
        }
    }

    vec![out]
}

// -- helpers -----------------------------------------------------------------

fn effective_openai_finish_reason(state: &OpenAIToClaudeStreamState) -> String {
    if state.saw_tool_call {
        return "tool_calls".to_owned();
    }
    state.finish_reason.clone()
}

fn map_openai_finish_reason_to_anthropic(openai_reason: &str) -> String {
    match openai_reason {
        "stop" => "end_turn".to_owned(),
        "length" => "max_tokens".to_owned(),
        "tool_calls" => "tool_use".to_owned(),
        "content_filter" => "end_turn".to_owned(),
        "function_call" => "tool_use".to_owned(),
        _ => "end_turn".to_owned(),
    }
}

fn tool_content_block_index(
    state: &mut OpenAIToClaudeStreamState,
    openai_tool_index: usize,
) -> usize {
    if let Some(index) = state.tool_call_block_indexes.get(&openai_tool_index) {
        return *index;
    }
    let index = state.next_content_block_index;
    state.next_content_block_index += 1;
    state
        .tool_call_block_indexes
        .insert(openai_tool_index, index);
    index
}

fn collect_openai_reasoning_texts_gjson(node: &gjson::Value) -> Vec<String> {
    let mut texts = Vec::new();
    if !node.exists() {
        return texts;
    }
    if node.kind() == gjson::Kind::Array {
        node.each(|_, value| {
            texts.extend(collect_openai_reasoning_texts_gjson(&value));
            true
        });
        return texts;
    }
    match node.kind() {
        gjson::Kind::String => {
            let text = node.str();
            if !text.is_empty() {
                texts.push(text.to_owned());
            }
        }
        gjson::Kind::Array | gjson::Kind::Object => {
            let text_node = node.get("text");
            if text_node.kind() == gjson::Kind::String {
                let text = text_node.str();
                if !text.is_empty() {
                    texts.push(text.to_owned());
                }
            } else {
                let raw = node.json();
                if !raw.is_empty() && !raw.starts_with('{') && !raw.starts_with('[') {
                    texts.push(raw.to_owned());
                }
            }
        }
        _ => {}
    }
    texts
}

fn collect_openai_reasoning_texts(node: &Value) -> Vec<String> {
    match node {
        Value::String(text) => (!text.is_empty())
            .then(|| text.clone())
            .into_iter()
            .collect(),
        Value::Array(items) => items
            .iter()
            .flat_map(collect_openai_reasoning_texts)
            .collect(),
        Value::Object(_) => node
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(|text| vec![text.to_owned()])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn stop_thinking_content_block(state: &mut OpenAIToClaudeStreamState, results: &mut Vec<Vec<u8>>) {
    if !state.thinking_content_block_started {
        return;
    }
    let mut stop: Vec<u8> = br#"{"type":"content_block_stop","index":0}"#.to_vec();
    stop = upsert_path_integer(&stop, &["index"], state.thinking_content_block_index);
    results.push(encode_sse_event(CONTENT_BLOCK_STOP_EVENT, &stop));
    state.thinking_content_block_started = false;
    state.thinking_content_block_index = -1;
}

fn stop_text_content_block(state: &mut OpenAIToClaudeStreamState, results: &mut Vec<Vec<u8>>) {
    if !state.text_content_block_started {
        return;
    }
    let mut stop: Vec<u8> = br#"{"type":"content_block_stop","index":0}"#.to_vec();
    stop = upsert_path_integer(&stop, &["index"], state.text_content_block_index);
    results.push(encode_sse_event(CONTENT_BLOCK_STOP_EVENT, &stop));
    state.text_content_block_started = false;
    state.text_content_block_index = -1;
}

fn emit_message_stop_if_needed(state: &mut OpenAIToClaudeStreamState, results: &mut Vec<Vec<u8>>) {
    if state.message_stop_sent {
        return;
    }
    results.push(encode_sse_event(
        MESSAGE_STOP_EVENT,
        br#"{"type":"message_stop"}"#,
    ));
    state.message_stop_sent = true;
}

fn emit_tool_use_start(
    state: &mut OpenAIToClaudeStreamState,
    openai_tool_index: usize,
    accumulator: &mut ToolCallAccumulator,
    results: &mut Vec<Vec<u8>>,
) {
    stop_thinking_content_block(state, results);
    stop_text_content_block(state, results);

    let block_index = tool_content_block_index(state, openai_tool_index);
    let mut content_block_start: Vec<u8> =
        br#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"","name":"","input":{}}}"#
            .to_vec();
    content_block_start = upsert_path_integer(&content_block_start, &["index"], block_index as i64);
    content_block_start = upsert_path_string(
        &content_block_start,
        &["content_block", "id"],
        &sanitize_claude_tool_id(&accumulator.id),
    );
    content_block_start = upsert_path_string(
        &content_block_start,
        &["content_block", "name"],
        &accumulator.name,
    );
    results.push(encode_sse_event(
        CONTENT_BLOCK_START_EVENT,
        &content_block_start,
    ));
    accumulator.start_emitted = true;
    state.saw_tool_call = true;
}

fn tool_call_accumulator_indexes(
    accumulators: &BTreeMap<usize, ToolCallAccumulator>,
) -> Vec<usize> {
    let mut indexes: Vec<usize> = accumulators.keys().copied().collect();
    indexes.sort();
    indexes
}

fn extract_openai_usage_gjson(usage: &gjson::Value) -> (i64, i64, i64) {
    if !usage.exists() || usage.kind() == gjson::Kind::Null {
        return (0, 0, 0);
    }
    let input = usage.get("prompt_tokens").i64();
    let output = usage.get("completion_tokens").i64();
    let cached = usage.get("prompt_tokens_details.cached_tokens").i64();
    let adjusted_input = if cached > 0 {
        if input >= cached {
            input - cached
        } else {
            0
        }
    } else {
        input
    };
    (adjusted_input, output, cached)
}

fn extract_openai_usage(usage: &Value) -> (i64, i64, i64) {
    let input = usage
        .get("prompt_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let output = usage
        .get("completion_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let cached = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    (input.saturating_sub(cached), output, cached)
}

fn append_tool_use_block(out: &[u8], id: &str, name: &str, args_str: &str) -> Vec<u8> {
    let sanitized_id = sanitize_claude_tool_id(id);
    let args_owned = if args_str.is_empty() { "{}" } else { args_str };
    let input_value = if !args_owned.is_empty() && gjson::valid(args_owned) {
        let parsed = gjson::parse(args_owned);
        if parsed.kind() == gjson::Kind::Object {
            parsed.json().to_owned()
        } else {
            "{}".to_owned()
        }
    } else {
        "{}".to_owned()
    };
    let raw = format!(
        r#"{{"type":"tool_use","id":"{}","name":"{}","input":{}}}"#,
        escape_json_string(&sanitized_id),
        escape_json_string(name),
        input_value
    );
    set_raw_bytes(out, "content.-1", raw.as_bytes())
}

fn encode_sse_event(event: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(event.len() + payload.len() + 16);
    append_sse_event(&mut out, event, payload, 2);
    out
}

fn trim_ascii(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}

fn root_get<'a>(root: &'a gjson::Value<'_>, path: &'a str) -> Option<gjson::Value<'a>> {
    let value = root.get(path);
    if value.exists() {
        Some(value)
    } else {
        None
    }
}

fn root_get_str<'a>(root: &'a gjson::Value<'_>, path: &'a str) -> Option<String> {
    let value = root.get(path);
    if value.kind() == gjson::Kind::String {
        Some(value.str().to_owned())
    } else {
        None
    }
}

fn escape_json_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                escaped.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => escaped.push(c),
        }
    }
    escaped
}

fn upsert_top_level_string(data: &[u8], key: &str, value: &str) -> Vec<u8> {
    upsert_path_string(data, &[key], value)
}

fn upsert_path_string(data: &[u8], path: &[&str], value: &str) -> Vec<u8> {
    let escaped = escape_json_string(value);
    let raw = format!("\"{}\"", escaped);
    upsert_path_raw(data, path, raw.as_bytes())
}

fn upsert_path_integer(data: &[u8], path: &[&str], value: i64) -> Vec<u8> {
    upsert_path_raw(data, path, value.to_string().as_bytes())
}

fn upsert_path_raw(data: &[u8], path: &[&str], value: &[u8]) -> Vec<u8> {
    if path.is_empty() {
        return data.to_vec();
    }
    let Ok(document) = std::str::from_utf8(data) else {
        return data.to_vec();
    };
    let joined = path.join(".");
    let parsed = gjson::get(document, &joined);
    if parsed.exists() {
        let raw = parsed.json();
        let doc_start = document.as_ptr() as usize;
        let raw_start = raw.as_ptr() as usize;
        if raw_start >= doc_start {
            let start = raw_start - doc_start;
            if start <= data.len() && raw.len() <= data.len() - start {
                let mut output = Vec::with_capacity(data.len() - raw.len() + value.len());
                output.extend_from_slice(&data[..start]);
                output.extend_from_slice(value);
                output.extend_from_slice(&data[start + raw.len()..]);
                return output;
            }
        }
    }
    if let Ok(mut root) = serde_json::from_slice::<Value>(data) {
        if let Ok(value_json) = serde_json::from_slice::<Value>(value) {
            insert_value_path(&mut root, path, value_json);
            return serde_json::to_vec(&root).unwrap_or_else(|_| data.to_vec());
        }
    }
    data.to_vec()
}

fn insert_value_path(root: &mut Value, path: &[&str], value: Value) {
    let Some((head, tail)) = path.split_first() else {
        *root = value;
        return;
    };
    if tail.is_empty() {
        if let Some(object) = root.as_object_mut() {
            object.insert((*head).to_owned(), value);
        }
        return;
    }
    let Some(object) = root.as_object_mut() else {
        return;
    };
    let child = object
        .entry((*head).to_owned())
        .or_insert_with(|| Value::Object(Default::default()));
    insert_value_path(child, tail, value);
}

fn set_raw_bytes(data: &[u8], key: &str, value: &[u8]) -> Vec<u8> {
    // sjson's `.-1` suffix appends to an array. gjson only reads paths and the
    // former fallback replaced the top-level `content` member with one object,
    // which discarded every preceding block. Preserve the upstream append
    // operation explicitly.
    if let Some(array_path) = key.strip_suffix(".-1") {
        if let (Ok(mut root), Ok(value_json)) = (
            serde_json::from_slice::<Value>(data),
            serde_json::from_slice::<Value>(value),
        ) {
            if let Some(array) = root.get_mut(array_path).and_then(Value::as_array_mut) {
                array.push(value_json);
                return serde_json::to_vec(&root).unwrap_or_else(|_| data.to_vec());
            }
        }
        return data.to_vec();
    }
    let Ok(document) = std::str::from_utf8(data) else {
        return data.to_vec();
    };
    let Ok(value_str) = std::str::from_utf8(value) else {
        return data.to_vec();
    };
    let parsed = gjson::get(document, key);
    if parsed.exists() {
        let raw = parsed.json();
        let doc_start = document.as_ptr() as usize;
        let raw_start = raw.as_ptr() as usize;
        if raw_start >= doc_start {
            let start = raw_start - doc_start;
            if start <= data.len() && raw.len() <= data.len() - start {
                let mut output = Vec::with_capacity(data.len() - raw.len() + value.len());
                output.extend_from_slice(&data[..start]);
                output.extend_from_slice(value);
                output.extend_from_slice(&data[start + raw.len()..]);
                return output;
            }
        }
    }
    if let Ok(mut root) = serde_json::from_slice::<Value>(data) {
        if let Ok(value_json) = serde_json::from_str::<Value>(value_str) {
            if let Some(object) = root.as_object_mut() {
                if let Some(first) = key.split('.').next() {
                    object.insert(first.to_owned(), value_json);
                    return serde_json::to_vec(&root).unwrap_or_else(|_| data.to_vec());
                }
            }
        }
    }
    data.to_vec()
}

// Helper for tests / sibling consumers. Currently unused by the port itself
// but mirrors the upstream `extractOpenAIUsage` shape.
#[cfg(test)]
fn usage_for_test(raw: &str) -> (i64, i64, i64) {
    let value = gjson::parse(raw);
    extract_openai_usage_gjson(&value)
}

// -- inlined helper mirrors of upstream `util/translator.go` ----------------
// The upstream Go file hosts `FixJSON`, `ToolNameMapFromClaudeRequest`, and
// `MapToolName`. The Rust sibling `util/translator.rs` does not yet expose
// them, and HARD SCOPE forbids editing it; the helpers are therefore copied
// here verbatim so the leaf is self-contained.

/// Converts non-standard JSON with single-quoted strings into valid JSON.
fn fix_json(input: &str) -> String {
    crate::internal::util::fix_json(input)
}

/// Builds a canonical-name -> original-name map extracted from a Claude
/// request. Mirrors upstream `ToolNameMapFromClaudeRequest`.
fn tool_name_map_from_claude_request(raw_json: &[u8]) -> Option<BTreeMap<String, String>> {
    let out = crate::internal::util::tool_name_map_from_claude_request(raw_json);
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn map_tool_name(tool_name_map: Option<&BTreeMap<String, String>>, name: &str) -> String {
    if name.is_empty() {
        return name.to_owned();
    }
    let Some(map) = tool_name_map else {
        return name.to_owned();
    };
    crate::internal::util::map_tool_name(map, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::translator::TranslationContext;

    #[test]
    fn extract_openai_usage_subtracts_cached_tokens() {
        let raw = r#"{"prompt_tokens":10,"completion_tokens":4,"prompt_tokens_details":{"cached_tokens":3}}"#;
        let (input, output, cached) = usage_for_test(raw);
        assert_eq!(input, 7);
        assert_eq!(output, 4);
        assert_eq!(cached, 3);
    }

    #[test]
    fn extract_openai_usage_handles_missing_and_null() {
        let raw = r#"{"prompt_tokens":2,"completion_tokens":1}"#;
        let (input, output, cached) = usage_for_test(raw);
        assert_eq!(input, 2);
        assert_eq!(output, 1);
        assert_eq!(cached, 0);
    }

    #[test]
    fn non_stream_response_emits_text_and_tool_use() {
        let context = TranslationContext::default();
        let mut state = OpenAIToClaudeStreamState::default();
        let raw = br#"{
            "id":"c1",
            "model":"m",
            "choices":[{
                "index":0,
                "message":{
                    "role":"assistant",
                    "content":null,
                    "tool_calls":[{
                        "id":"call:1",
                        "type":"function",
                        "function":{"name":"do_work","arguments":"{\"a\":1}"}
                    }]
                },
                "finish_reason":"tool_calls"
            }],
            "usage":{"prompt_tokens":5,"completion_tokens":2}
        }"#;
        let result = convert_openai_response_to_claude_non_stream(
            &context, "m", b"{}", b"{}", raw, &mut state,
        );
        let value: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(value["stop_reason"], "tool_use");
        let tool_use = value["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "tool_use")
            .unwrap();
        assert_eq!(tool_use["id"], "call_1");
        assert_eq!(tool_use["name"], "do_work");
        assert_eq!(tool_use["input"]["a"], 1);
        assert_eq!(value["usage"]["input_tokens"], 5);
        assert_eq!(value["usage"]["output_tokens"], 2);
    }

    #[test]
    fn non_stream_response_emits_thinking_and_text() {
        let context = TranslationContext::default();
        let mut state = OpenAIToClaudeStreamState::default();
        let raw = br#"{
            "id":"c2",
            "model":"m",
            "choices":[{
                "index":0,
                "message":{
                    "role":"assistant",
                    "content":"hello",
                    "reasoning_content":"because"
                },
                "finish_reason":"stop"
            }],
            "usage":{"prompt_tokens":2,"completion_tokens":1}
        }"#;
        let result = convert_openai_response_to_claude_non_stream(
            &context, "m", b"{}", b"{}", raw, &mut state,
        );
        let value: Value = serde_json::from_slice(&result).unwrap();
        let blocks = value["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "thinking");
        assert_eq!(blocks[0]["thinking"], "because");
        assert_eq!(blocks[1]["type"], "text");
        assert_eq!(blocks[1]["text"], "hello");
        assert_eq!(value["stop_reason"], "end_turn");
    }

    #[test]
    fn map_openai_finish_reason_preserves_table() {
        assert_eq!(map_openai_finish_reason_to_anthropic("stop"), "end_turn");
        assert_eq!(
            map_openai_finish_reason_to_anthropic("length"),
            "max_tokens"
        );
        assert_eq!(
            map_openai_finish_reason_to_anthropic("tool_calls"),
            "tool_use"
        );
        assert_eq!(
            map_openai_finish_reason_to_anthropic("content_filter"),
            "end_turn"
        );
        assert_eq!(
            map_openai_finish_reason_to_anthropic("function_call"),
            "tool_use"
        );
        assert_eq!(map_openai_finish_reason_to_anthropic(""), "end_turn");
    }

}
