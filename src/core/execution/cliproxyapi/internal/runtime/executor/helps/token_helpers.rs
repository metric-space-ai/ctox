// ref: internal/runtime/executor/helps/token_helpers.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;

use tiktoken_rs::{cl100k_base_singleton, o200k_base_singleton};

pub const MAX_OPENAI_TOKEN_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_OPENAI_TOKEN_TEXT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenAiTokenizer {
    Cl100k,
    O200k,
}

impl OpenAiTokenizer {
    fn count(self, text: &str) -> usize {
        match self {
            Self::Cl100k => cl100k_base_singleton()
                .encode_with_special_tokens(text)
                .len(),
            Self::O200k => o200k_base_singleton()
                .encode_with_special_tokens(text)
                .len(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenAiTokenCountError {
    PayloadTooLarge,
    TextTooLarge,
    CountOverflow,
}

impl fmt::Display for OpenAiTokenCountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PayloadTooLarge => "OpenAI token payload exceeds limit",
            Self::TextTooLarge => "OpenAI token text exceeds limit",
            Self::CountOverflow => "OpenAI token count exceeds i64",
        })
    }
}

impl std::error::Error for OpenAiTokenCountError {}

/// Returns a tokenizer suitable for an OpenAI-style model id.
#[must_use]
pub fn tokenizer_for_model(model: &str) -> OpenAiTokenizer {
    let sanitized = model.trim().to_ascii_lowercase();
    if sanitized.is_empty()
        || sanitized.starts_with("gpt-4")
            && !sanitized.starts_with("gpt-4.1")
            && !sanitized.starts_with("gpt-4o")
        || sanitized.starts_with("gpt-3.5")
        || sanitized.starts_with("gpt-3")
    {
        OpenAiTokenizer::Cl100k
    } else {
        // gpt-5, gpt-4.1, gpt-4o, o1/o3/o4, and unknown future models use
        // the upstream o200k-family fallback.
        OpenAiTokenizer::O200k
    }
}

/// Approximates prompt tokens for OpenAI chat-completions payloads.
pub fn count_openai_chat_tokens(
    encoder: OpenAiTokenizer,
    payload: &[u8],
) -> Result<i64, OpenAiTokenCountError> {
    if payload.len() > MAX_OPENAI_TOKEN_PAYLOAD_BYTES {
        return Err(OpenAiTokenCountError::PayloadTooLarge);
    }
    if payload.is_empty() {
        return Ok(0);
    }
    let Ok(document) = std::str::from_utf8(payload) else {
        return Ok(0);
    };
    let root = gjson::parse(document);
    let mut segments = Segments::default();
    collect_openai_messages(&root.get("messages"), &mut segments);
    collect_openai_tools(&root.get("tools"), &mut segments);
    collect_openai_functions(&root.get("functions"), &mut segments);
    collect_openai_tool_choice(&root.get("tool_choice"), &mut segments);
    collect_openai_response_format(&root.get("response_format"), &mut segments);
    segments.add(root.get("input").str());
    segments.add(root.get("prompt").str());
    if segments.overflow {
        return Err(OpenAiTokenCountError::TextTooLarge);
    }
    let joined = segments.values.join("\n");
    let joined = joined.trim();
    if joined.is_empty() {
        return Ok(0);
    }
    i64::try_from(encoder.count(joined)).map_err(|_| OpenAiTokenCountError::CountOverflow)
}

pub fn count_openai_chat_tokens_for_model(
    model: &str,
    payload: &[u8],
) -> Result<i64, OpenAiTokenCountError> {
    count_openai_chat_tokens(tokenizer_for_model(model), payload)
}

/// Returns the minimal usage structure understood by downstream translators.
#[must_use]
pub fn build_openai_usage_json(count: i64) -> Vec<u8> {
    format!(
        r#"{{"usage":{{"prompt_tokens":{count},"completion_tokens":0,"total_tokens":{count}}}}}"#
    )
    .into_bytes()
}

#[derive(Default)]
struct Segments {
    values: Vec<String>,
    bytes: usize,
    overflow: bool,
}

impl Segments {
    fn add(&mut self, value: &str) {
        if self.overflow {
            return;
        }
        let value = value.trim();
        if value.is_empty() {
            return;
        }
        let separator = usize::from(!self.values.is_empty());
        let Some(next) = self
            .bytes
            .checked_add(separator)
            .and_then(|bytes| bytes.checked_add(value.len()))
        else {
            self.overflow = true;
            return;
        };
        if next > MAX_OPENAI_TOKEN_TEXT_BYTES {
            self.overflow = true;
            return;
        }
        self.bytes = next;
        self.values.push(value.to_owned());
    }
}

fn collect_openai_messages(messages: &gjson::Value<'_>, segments: &mut Segments) {
    if messages.kind() != gjson::Kind::Array {
        return;
    }
    messages.each(|_, message| {
        segments.add(message.get("role").str());
        segments.add(message.get("name").str());
        collect_openai_content(&message.get("content"), segments);
        collect_openai_tool_calls(&message.get("tool_calls"), segments);
        collect_openai_function_call(&message.get("function_call"), segments);
        !segments.overflow
    });
}

fn collect_openai_content(content: &gjson::Value<'_>, segments: &mut Segments) {
    if !content.exists() {
        return;
    }
    if content.kind() == gjson::Kind::String {
        segments.add(content.str());
        return;
    }
    if content.kind() == gjson::Kind::Array {
        content.each(|_, part| {
            match part.get("type").str() {
                "text" | "input_text" | "output_text" => segments.add(part.get("text").str()),
                "image_url" => segments.add(part.get("image_url.url").str()),
                "input_audio" | "output_audio" | "audio" => segments.add(part.get("id").str()),
                "tool_result" => {
                    segments.add(part.get("name").str());
                    collect_openai_content(&part.get("content"), segments);
                }
                _ if part.kind() == gjson::Kind::Array => collect_openai_content(&part, segments),
                _ if matches!(part.kind(), gjson::Kind::Object | gjson::Kind::Array) => {
                    segments.add(part.json());
                }
                _ => segments.add(part.str()),
            }
            !segments.overflow
        });
        return;
    }
    if matches!(content.kind(), gjson::Kind::Object | gjson::Kind::Array) {
        segments.add(content.json());
    }
}

fn collect_openai_tool_calls(calls: &gjson::Value<'_>, segments: &mut Segments) {
    if calls.kind() != gjson::Kind::Array {
        return;
    }
    calls.each(|_, call| {
        segments.add(call.get("id").str());
        segments.add(call.get("type").str());
        let function = call.get("function");
        if function.exists() {
            segments.add(function.get("name").str());
            segments.add(function.get("description").str());
            segments.add(function.get("arguments").str());
            let parameters = function.get("parameters");
            if parameters.exists() {
                segments.add(parameters.json());
            }
        }
        !segments.overflow
    });
}

fn collect_openai_function_call(call: &gjson::Value<'_>, segments: &mut Segments) {
    if call.exists() {
        segments.add(call.get("name").str());
        segments.add(call.get("arguments").str());
    }
}

fn collect_openai_tools(tools: &gjson::Value<'_>, segments: &mut Segments) {
    if !tools.exists() {
        return;
    }
    if tools.kind() == gjson::Kind::Array {
        tools.each(|_, tool| {
            append_tool_payload(&tool, segments);
            !segments.overflow
        });
    } else {
        append_tool_payload(tools, segments);
    }
}

fn collect_openai_functions(functions: &gjson::Value<'_>, segments: &mut Segments) {
    if functions.kind() != gjson::Kind::Array {
        return;
    }
    functions.each(|_, function| {
        segments.add(function.get("name").str());
        segments.add(function.get("description").str());
        let parameters = function.get("parameters");
        if parameters.exists() {
            segments.add(parameters.json());
        }
        !segments.overflow
    });
}

fn collect_openai_tool_choice(choice: &gjson::Value<'_>, segments: &mut Segments) {
    if !choice.exists() {
        return;
    }
    if choice.kind() == gjson::Kind::String {
        segments.add(choice.str());
    } else {
        segments.add(choice.json());
    }
}

fn collect_openai_response_format(format: &gjson::Value<'_>, segments: &mut Segments) {
    if !format.exists() {
        return;
    }
    segments.add(format.get("type").str());
    segments.add(format.get("name").str());
    for path in ["json_schema", "schema"] {
        let schema = format.get(path);
        if schema.exists() {
            segments.add(schema.json());
        }
    }
}

fn append_tool_payload(tool: &gjson::Value<'_>, segments: &mut Segments) {
    if !tool.exists() {
        return;
    }
    segments.add(tool.get("type").str());
    segments.add(tool.get("name").str());
    segments.add(tool.get("description").str());
    let function = tool.get("function");
    if function.exists() {
        segments.add(function.get("name").str());
        segments.add(function.get("description").str());
        let parameters = function.get("parameters");
        if parameters.exists() {
            segments.add(parameters.json());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

#[test]
    fn count_collects_every_upstream_segment_family() {
        let payload = br#"{"messages":[{"role":"user","name":"n","content":[{"type":"text","text":"hello"},{"type":"image_url","image_url":{"url":"https://image"}},{"type":"audio","id":"audio-1"},{"type":"tool_result","name":"lookup","content":"result"}]},{"role":"assistant","tool_calls":[{"id":"call-1","type":"function","function":{"name":"f","description":"d","arguments":"{}","parameters":{"type":"object"}}}],"function_call":{"name":"legacy","arguments":"{}"}}],"tools":[{"type":"function","name":"outer","description":"tool","function":{"name":"inner","description":"inner-d","parameters":{"type":"object"}}}],"functions":[{"name":"old","description":"old-d","parameters":{"type":"object"}}],"tool_choice":{"type":"function","name":"f"},"response_format":{"type":"json_schema","name":"response","json_schema":{"type":"object"},"schema":{"type":"object"}},"input":"input","prompt":"prompt"}"#;
        let count = count_openai_chat_tokens(OpenAiTokenizer::O200k, payload).unwrap();
        assert!(count > 20);
        assert_eq!(
            count,
            count_openai_chat_tokens_for_model("gpt-4o", payload).unwrap()
        );
    }

#[test]
    fn empty_invalid_and_bounded_inputs_are_safe() {
        assert_eq!(
            count_openai_chat_tokens(OpenAiTokenizer::Cl100k, b"").unwrap(),
            0
        );
        assert_eq!(
            count_openai_chat_tokens(OpenAiTokenizer::Cl100k, b"not-json").unwrap(),
            0
        );
        assert_eq!(
            count_openai_chat_tokens(OpenAiTokenizer::Cl100k, b"\xff").unwrap(),
            0
        );
        assert_eq!(
            count_openai_chat_tokens(
                OpenAiTokenizer::Cl100k,
                &vec![b'x'; MAX_OPENAI_TOKEN_PAYLOAD_BYTES + 1],
            ),
            Err(OpenAiTokenCountError::PayloadTooLarge)
        );
    }

#[test]
    fn usage_json_is_minimal_and_signed_count_compatible() {
        assert_eq!(
            build_openai_usage_json(7),
            br#"{"usage":{"prompt_tokens":7,"completion_tokens":0,"total_tokens":7}}"#
        );
        assert_eq!(
            build_openai_usage_json(-1),
            br#"{"usage":{"prompt_tokens":-1,"completion_tokens":0,"total_tokens":-1}}"#
        );
    }
}
