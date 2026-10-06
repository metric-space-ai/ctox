// ref: internal/runtime/executor/claude_executor_execute.go:339-375 @ 16d98881d4bb37adaa827599e4be8f5154e81646
// Port-Status: candidate
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;

use super::claude_executor::{ClaudeMessagesRequest, ClaudeMessagesResponse, ClaudeUsage};
use super::claude_executor_diagnostics::{
    claude_message_id_from_response, claude_message_id_from_sse,
};
use super::claude_executor_request::{
    restore_claude_oauth_tool_names_from_response, restore_claude_oauth_tool_names_from_stream_line,
};
use super::helps::{observe_plugin_executor_stream, StreamUsageBuffer};

/// A unary caller can still request an upstream stream and receive its complete
/// buffered SSE body. Validate it before publishing continuity or restoring names.
pub(super) fn prepare_claude_buffered_response(
    response: ClaudeMessagesResponse,
    request: &ClaudeMessagesRequest,
) -> ClaudeMessagesResponse {
    if !request.stream() || !(200..300).contains(&response.status()) {
        return response.map_body(|body| {
            restore_claude_oauth_tool_names_from_response(
                body,
                "",
                false,
                request.tool_name_reverse_map(),
            )
        });
    }
    if let Err(message) = validate_claude_buffered_stream(response.body()) {
        let mut headers = response.headers().clone();
        headers.retain(|key, _| {
            !key.eq_ignore_ascii_case("content-type")
                && !key.eq_ignore_ascii_case("content-length")
                && !key.eq_ignore_ascii_case("content-encoding")
        });
        headers.insert(
            "content-type".to_owned(),
            vec!["application/json".to_owned()],
        );
        let body = serde_json::to_vec(&serde_json::json!({
            "type": "error",
            "error": {"type": "api_error", "message": message},
        }))
        .expect("fixed Claude error envelope is serializable");
        return ClaudeMessagesResponse::new(502, body)
            .with_headers(headers)
            .with_retry_after(response.retry_after());
    }
    response
        .map_body(|body| restore_claude_stream_tool_names(body, request.tool_name_reverse_map()))
}

pub(super) fn restore_claude_stream_tool_names(
    data: &[u8],
    reverse: &HashMap<String, String>,
) -> Vec<u8> {
    let mut restored = Vec::with_capacity(data.len());
    for (index, line) in data.split(|byte| *byte == b'\n').enumerate() {
        if index > 0 {
            restored.push(b'\n');
        }
        restored.extend_from_slice(&restore_claude_oauth_tool_names_from_stream_line(
            line, "", false, reverse,
        ));
    }
    restored
}

pub(super) fn claude_buffered_response_message_id(data: &[u8], stream: bool) -> String {
    if stream {
        claude_message_id_from_sse(data)
    } else {
        claude_message_id_from_response(data)
    }
}

pub(super) fn parse_claude_buffered_response_usage(
    data: &[u8],
    stream: bool,
) -> Option<ClaudeUsage> {
    if !stream {
        return super::claude_executor::parse_claude_usage(data);
    }
    let mut buffer = StreamUsageBuffer::default();
    observe_plugin_executor_stream("claude", data, &mut buffer);
    claude_usage_from_stream_buffer(&buffer)
}

pub(super) fn claude_usage_from_stream_buffer(buffer: &StreamUsageBuffer) -> Option<ClaudeUsage> {
    buffer.detail().map(|detail| ClaudeUsage {
        input_tokens: detail.input_tokens,
        output_tokens: detail.output_tokens,
        cached_tokens: detail.cached_tokens,
        cache_read_tokens: detail.cache_read_tokens,
        cache_creation_tokens: detail.cache_creation_tokens,
        total_tokens: detail.total_tokens,
    })
}

// ref: internal/runtime/executor/claude_executor_stream.go:506-562 @ 16d98881
fn validate_claude_buffered_stream(data: &[u8]) -> Result<(), String> {
    let mut has_data = false;
    let mut has_message_start = false;
    let mut has_message_delta = false;
    for line in data.split(|byte| *byte == b'\n') {
        if line.len() >= 52_428_800 {
            return Err("bufio.Scanner: token too long".to_owned());
        }
        let Ok(line) = std::str::from_utf8(line) else {
            if line.trim_ascii().starts_with(b"data:") {
                return Err("claude executor: upstream returned malformed stream data".to_owned());
            }
            continue;
        };
        let Some(payload) = line.trim().strip_prefix("data:").map(str::trim) else {
            continue;
        };
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        has_data = true;
        if !gjson::valid(payload) {
            return Err("claude executor: upstream returned malformed stream data".to_owned());
        }
        match gjson::get(payload, "type").str() {
            "error" => {
                let message = gjson::get(payload, "error.message").str().trim().to_owned();
                let kind = gjson::get(payload, "error.type").str().trim().to_owned();
                let message = if !message.is_empty() {
                    message
                } else if !kind.is_empty() {
                    kind
                } else {
                    "unknown upstream error".to_owned()
                };
                return Err(format!(
                    "claude executor: upstream returned error event: {message}"
                ));
            }
            "message_start" => {
                if gjson::get(payload, "message.id").str().trim().is_empty()
                    || gjson::get(payload, "message.model").str().trim().is_empty()
                {
                    return Err(
                        "claude executor: upstream stream message_start is missing id or model"
                            .to_owned(),
                    );
                }
                has_message_start = true;
            }
            "message_delta" => has_message_delta = true,
            _ => {}
        }
    }
    if !has_data {
        return Err("claude executor: upstream returned empty stream response".to_owned());
    }
    if !has_message_start {
        return Err(
            "claude executor: upstream stream response is missing message_start".to_owned(),
        );
    }
    if !has_message_delta {
        return Err(
            "claude executor: upstream stream response ended before message completion".to_owned(),
        );
    }
    Ok(())
}
