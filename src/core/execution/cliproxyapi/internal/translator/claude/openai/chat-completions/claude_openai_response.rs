// ref: internal/translator/claude/openai/chat-completions/claude_openai_response.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

#[derive(Default)]
struct Usage {
    input: i64,
    output: i64,
    cache_creation: i64,
    cache_read: i64,
    seen: bool,
}

impl Usage {
    fn merge(&mut self, value: Option<&Value>) {
        let Some(value) = value else { return };
        self.seen = true;
        for (key, target) in [
            ("input_tokens", &mut self.input),
            ("output_tokens", &mut self.output),
            ("cache_creation_input_tokens", &mut self.cache_creation),
            ("cache_read_input_tokens", &mut self.cache_read),
        ] {
            if let Some(number) = value.get(key).and_then(Value::as_i64) {
                *target = number;
            }
        }
    }
}

#[derive(Default)]
struct ToolCall {
    index: usize,
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
pub struct ClaudeToChatStreamState {
    created_at: i64,
    response_id: String,
    finish_reason: String,
    usage: Usage,
    tools: BTreeMap<usize, ToolCall>,
    trailing_usage_sent: bool,
    next_tool_call_index: usize,
}

pub fn convert_claude_response_to_openai_chat_stream(
    model_name: &str,
    _original_request: &[u8],
    _request: &[u8],
    raw: &[u8],
    state: &mut ClaudeToChatStreamState,
) -> Vec<Vec<u8>> {
    let Some(payload) = raw.strip_prefix(b"data:") else {
        return Vec::new();
    };
    let Ok(event) = serde_json::from_slice::<Value>(trim_ascii(payload)) else {
        return Vec::new();
    };
    let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");
    if event_type == "error" {
        let Some(error) = event.get("error") else {
            return Vec::new();
        };
        return encode_stream_value(json!({
            "error": {
                "message": error.get("message").and_then(Value::as_str).unwrap_or(""),
                "type": error.get("type").and_then(Value::as_str).unwrap_or("")
            }
        }));
    }

    let mut output = json!({
        "id":state.response_id,
        "object":"chat.completion.chunk",
        "created":state.created_at,
        "model":model_name,
        "choices":[{"index":0,"delta":{},"finish_reason":null}]
    });
    match event_type {
        "message_start" => {
            if let Some(message) = event.get("message") {
                state.response_id = message
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                state.created_at = unix_time();
                state.tools.clear();
                state.next_tool_call_index = 0;
                state.usage.merge(message.get("usage"));
                output["id"] = Value::String(state.response_id.clone());
                output["created"] = Value::from(state.created_at);
                output["choices"][0]["delta"]["role"] = Value::String("assistant".into());
            }
            encode_stream_value(output)
        }
        "content_block_start" => {
            let Some(block) = event.get("content_block") else {
                return Vec::new();
            };
            if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                let index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let tool_index = state.next_tool_call_index;
                state.next_tool_call_index += 1;
                state.tools.insert(
                    index,
                    ToolCall {
                        index: tool_index,
                        id: block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        name: block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        arguments: String::new(),
                    },
                );
            }
            Vec::new()
        }
        "content_block_delta" => {
            let Some(delta) = event.get("delta") else {
                return Vec::new();
            };
            match delta.get("type").and_then(Value::as_str).unwrap_or("") {
                "text_delta" if delta.get("text").is_some() => {
                    output["choices"][0]["delta"]["content"] = Value::String(
                        delta
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                    );
                    encode_stream_value(output)
                }
                "thinking_delta" if delta.get("thinking").is_some() => {
                    output["choices"][0]["delta"]["reasoning_content"] = Value::String(
                        delta
                            .get("thinking")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                    );
                    encode_stream_value(output)
                }
                "input_json_delta" => {
                    let index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    if let (Some(tool), Some(arguments)) = (
                        state.tools.get_mut(&index),
                        delta.get("partial_json").and_then(Value::as_str),
                    ) {
                        tool.arguments.push_str(arguments);
                    }
                    Vec::new()
                }
                _ => Vec::new(),
            }
        }
        "content_block_stop" => {
            let index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            let Some(mut tool) = state.tools.remove(&index) else {
                return Vec::new();
            };
            if tool.arguments.is_empty() {
                tool.arguments = "{}".to_owned();
            }
            output["choices"][0]["delta"]["tool_calls"] = json!([{
                "index":tool.index,
                "id":tool.id,
                "type":"function",
                "function":{"name":tool.name,"arguments":tool.arguments}
            }]);
            encode_stream_value(output)
        }
        "message_delta" => {
            if let Some(reason) = event.pointer("/delta/stop_reason").and_then(Value::as_str) {
                state.finish_reason = map_stop_reason(reason).to_owned();
                output["choices"][0]["finish_reason"] = Value::String(state.finish_reason.clone());
            }
            if event.get("usage").is_some() {
                state.usage.merge(event.get("usage"));
                let prompt =
                    state.usage.input + state.usage.cache_creation + state.usage.cache_read;
                output["usage"] = json!({
                    "prompt_tokens":prompt,
                    "completion_tokens":state.usage.output,
                    "total_tokens":prompt + state.usage.output,
                    "prompt_tokens_details":{
                        "cached_tokens":state.usage.cache_read,
                        "cached_creation_tokens":state.usage.cache_creation,
                        "cache_write_tokens":state.usage.cache_creation
                    }
                });
            }
            encode_stream_value(output)
        }
        // ref: upstream claude_openai_response.go:264-290 @ 16d98881d4bb37adaa827599e4be8f5154e81646
        "message_stop" if state.usage.seen && !state.trailing_usage_sent => {
            state.trailing_usage_sent = true;
            let prompt = state.usage.input + state.usage.cache_creation + state.usage.cache_read;
            output["choices"] = json!([]);
            output["usage"] = json!({
                "prompt_tokens":prompt,
                "completion_tokens":state.usage.output,
                "total_tokens":prompt + state.usage.output,
                "prompt_tokens_details":{
                    "cached_tokens":state.usage.cache_read,
                    "cached_creation_tokens":state.usage.cache_creation,
                    "cache_write_tokens":state.usage.cache_creation
                }
            });
            encode_stream_value(output)
        }
        "message_stop" | "ping" => Vec::new(),
        _ => Vec::new(),
    }
}

fn encode_stream_value(value: Value) -> Vec<Vec<u8>> {
    serde_json::to_vec(&value).map_or_else(|_| Vec::new(), |value| vec![value])
}

fn unix_time() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

// ref: internal/translator/claude/openai/chat-completions/claude_openai_response.go:341-495 @ 16d98881d4bb37adaa827599e4be8f5154e81646
pub fn convert_claude_response_to_openai_chat_non_stream(
    _original_request: &[u8],
    _request: &[u8],
    raw: &[u8],
) -> Vec<u8> {
    let (raw, _) = crate::internal::translator::common::claude_messages_json_to_sse(raw);
    let mut message_id = String::new();
    let mut model = String::new();
    let mut created_at = 0;
    let mut stop_reason = String::new();
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut usage = Usage::default();
    let mut tools = BTreeMap::<usize, ToolCall>::new();

    for line in raw.split(|byte| *byte == b'\n') {
        let Some(payload) = line.strip_prefix(b"data:") else {
            continue;
        };
        let Ok(event) = serde_json::from_slice::<Value>(trim_ascii(payload)) else {
            continue;
        };
        match event.get("type").and_then(Value::as_str).unwrap_or("") {
            "message_start" => {
                if let Some(message) = event.get("message") {
                    message_id = message
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    model = message
                        .get("model")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    created_at = unix_time();
                    usage.merge(message.get("usage"));
                }
            }
            "content_block_start" => {
                let Some(block) = event.get("content_block") else {
                    continue;
                };
                if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                    let index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    tools.insert(
                        index,
                        ToolCall {
                            index: 0,
                            id: block
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned(),
                            name: block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned(),
                            arguments: String::new(),
                        },
                    );
                }
            }
            "content_block_delta" => {
                let Some(delta) = event.get("delta") else {
                    continue;
                };
                match delta.get("type").and_then(Value::as_str).unwrap_or("") {
                    "text_delta" => {
                        text.push_str(delta.get("text").and_then(Value::as_str).unwrap_or(""))
                    }
                    "thinking_delta" => reasoning
                        .push_str(delta.get("thinking").and_then(Value::as_str).unwrap_or("")),
                    "input_json_delta" => {
                        let index =
                            event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                        if let Some(tool) = tools.get_mut(&index) {
                            tool.arguments.push_str(
                                delta
                                    .get("partial_json")
                                    .and_then(Value::as_str)
                                    .unwrap_or(""),
                            );
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(tool) = tools.get_mut(&index) {
                    if tool.arguments.is_empty() {
                        tool.arguments = "{}".to_owned();
                    }
                }
            }
            "message_delta" => {
                stop_reason = event
                    .pointer("/delta/stop_reason")
                    .and_then(Value::as_str)
                    .unwrap_or(&stop_reason)
                    .to_owned();
                usage.merge(event.get("usage"));
            }
            _ => {}
        }
    }

    let mut message = json!({"role":"assistant", "content":text});
    if !reasoning.is_empty() {
        message["reasoning_content"] = Value::String(reasoning);
    }
    if !tools.is_empty() {
        message["tool_calls"] = Value::Array(
            tools
                .into_values()
                .map(|tool| {
                    json!({"id":tool.id, "type":"function", "function":{"name":tool.name, "arguments":tool.arguments}})
                })
                .collect(),
        );
    }
    let finish_reason = if message.get("tool_calls").is_some() {
        "tool_calls"
    } else {
        map_stop_reason(&stop_reason)
    };
    let prompt = usage.input + usage.cache_creation + usage.cache_read;
    let mut output = json!({
        "id":message_id,
        "object":"chat.completion",
        "created":created_at,
        "model":model,
        "choices":[{"index":0, "message":message, "finish_reason":finish_reason}],
        "usage":{"prompt_tokens":prompt, "completion_tokens":usage.output, "total_tokens":prompt + usage.output}
    });
    if usage.seen {
        output["usage"]["prompt_tokens_details"] = json!({
            "cached_tokens":usage.cache_read,
            "cached_creation_tokens":usage.cache_creation,
            "cache_write_tokens":usage.cache_creation
        });
    }
    serde_json::to_vec(&output).unwrap_or_else(|_| raw.to_vec())
}

fn map_stop_reason(reason: &str) -> &'static str {
    match reason {
        "tool_use" => "tool_calls",
        "max_tokens" => "length",
        _ => "stop",
    }
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

#[cfg(test)]
mod tests {
    use super::{
        convert_claude_response_to_openai_chat_non_stream,
        convert_claude_response_to_openai_chat_stream, ClaudeToChatStreamState,
    };
    use serde_json::Value;

    #[test]
    fn aggregates_reasoning_tools_usage_and_creation_time() {
        let raw = br#"data: {"type":"message_start","message":{"id":"msg_1","model":"claude","usage":{"input_tokens":2,"cache_read_input_tokens":3}}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"why"}}
data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call_1","name":"run"}}
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"ok\":true}"}}
data: {"type":"content_block_stop","index":2}
data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":4}}
"#;

        let output: Value = serde_json::from_slice(
            &convert_claude_response_to_openai_chat_non_stream(b"{}", b"{}", raw),
        )
        .expect("valid OpenAI Chat response");

        assert_eq!(output["id"], "msg_1");
        assert!(output["created"].as_i64().unwrap_or_default() > 0);
        assert_eq!(output["choices"][0]["message"]["reasoning_content"], "why");
        assert!(output["choices"][0]["message"].get("reasoning").is_none());
        assert_eq!(
            output["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            r#"{"ok":true}"#
        );
        assert_eq!(output["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(output["usage"]["prompt_tokens"], 5);
        assert_eq!(output["usage"]["completion_tokens"], 4);
        assert_eq!(output["usage"]["total_tokens"], 9);
        assert_eq!(output["usage"]["prompt_tokens_details"]["cached_tokens"], 3);
    }

    #[test]
    fn stream_carries_state_until_completed_tool_and_usage() {
        let mut state = ClaudeToChatStreamState::default();
        let mut convert = |raw: &[u8]| {
            convert_claude_response_to_openai_chat_stream(
                "claude-client-model",
                b"{}",
                b"{}",
                raw,
                &mut state,
            )
        };
        let start = convert(br#"data: {"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":2,"cache_read_input_tokens":3}}}"#);
        let start: Value = serde_json::from_slice(&start[0]).unwrap();
        assert_eq!(start["model"], "claude-client-model");
        assert!(start["created"].as_i64().unwrap_or_default() > 0);

        assert!(convert(br#"data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call_1","name":"run"}}"#).is_empty());
        assert!(convert(br#"data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"ok\":"}}"#).is_empty());
        assert!(convert(br#"data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"true}"}}"#).is_empty());
        let tool = convert(br#"data: {"type":"content_block_stop","index":2}"#);
        let tool: Value = serde_json::from_slice(&tool[0]).unwrap();
        assert_eq!(
            tool["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"],
            r#"{"ok":true}"#
        );

        let finish = convert(br#"data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":4}}"#);
        let finish: Value = serde_json::from_slice(&finish[0]).unwrap();
        assert_eq!(finish["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(finish["usage"]["prompt_tokens"], 5);
        assert_eq!(finish["usage"]["total_tokens"], 9);
    }
}
