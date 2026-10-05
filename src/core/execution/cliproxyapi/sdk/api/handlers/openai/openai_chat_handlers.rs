// ref: sdk/api/handlers/openai/openai_handlers.go:440-751 @ a4acc9f752bd46571f737a10c04bf413656ab06b
// Port-Status: candidate — CTOX Responses-shaped provider bridge
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, VecDeque};

use serde_json::{json, Value};

use crate::internal::translator::codex::openai::chat_completions::{
    convert_codex_response_to_openai_chat_non_stream, convert_codex_response_to_openai_chat_stream,
    convert_openai_chat_request_to_codex, CodexToChatStreamState,
};
use crate::internal::translator::common::{SseDecoder, SseEvent};
use crate::internal::translator::openai::openai::responses::convert_openai_responses_request_to_openai_chat_completions;

use super::openai_handlers::{
    convert_chat_completions_response_to_completions,
    convert_chat_completions_stream_chunk_to_completions,
    convert_completions_request_to_chat_completions, should_treat_as_responses_format,
};
use super::openai_responses_handlers::{
    OpenAiResponsesAntigravityStream, OpenAiResponsesCodexStream, OpenAiResponsesHttpResponse,
    OpenAiResponsesRouteHandler, OpenAiResponsesRouteResponse, OpenAiResponsesStreamBootstrap,
};

/// Adapts the public OpenAI routes to CTOX's existing, account-owned Responses
/// contract. Provider selection and request headers stay with the same owner.
pub(crate) async fn handle_openai_chat_route<H: OpenAiResponsesRouteHandler + ?Sized>(
    handler: &H,
    provider: Option<&str>,
    headers: &BTreeMap<String, Vec<String>>,
    body: &[u8],
    completions: bool,
) -> OpenAiChatRouteResponse {
    let supplied_responses = !completions && should_treat_as_responses_format(body);
    let chat = if completions {
        convert_completions_request_to_chat_completions(body)
    } else if supplied_responses {
        let root: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        convert_openai_responses_request_to_openai_chat_completions(
            root.get("model")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            body,
            root.get("stream").and_then(Value::as_bool) == Some(true),
        )
    } else {
        body.to_vec()
    };

    let root = match serde_json::from_slice::<Value>(&chat) {
        Ok(Value::Object(root)) => root,
        _ => return buffered_error(400, "request body must be a JSON object"),
    };
    let Some(model) = root
        .get("model")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
    else {
        return buffered_error(400, "model is required");
    };
    let stream = root.get("stream").and_then(Value::as_bool) == Some(true);
    let canonical = if supplied_responses {
        body.to_vec()
    } else {
        if !root.get("messages").is_some_and(Value::is_array) {
            return buffered_error(400, "messages must be an array");
        }
        let mut canonical: Value =
            serde_json::from_slice(&convert_openai_chat_request_to_codex(model, &chat, stream))
                .unwrap_or(Value::Null);
        // The shared translator supplies the structured input/tools/text
        // contract. Preserve caller-owned sampling, tracing, cache and usage
        // fields instead of dropping them at the public HTTP boundary.
        for key in [
            "temperature",
            "top_p",
            "parallel_tool_calls",
            "store",
            "metadata",
            "user",
            "service_tier",
            "prompt_cache_key",
            "prompt_cache_retention",
            "safety_identifier",
            "stream_options",
            "session_id",
            "conversation_id",
            "seed",
            "stop",
            "frequency_penalty",
            "presence_penalty",
            "logprobs",
            "top_logprobs",
            "n",
            "echo",
        ] {
            if let Some(value) = root.get(key) {
                canonical[key] = value.clone();
            }
        }
        if !root.contains_key("reasoning_effort") {
            if let Some(object) = canonical.as_object_mut() {
                object.remove("reasoning");
            }
        }
        if let Some(limit) = root
            .get("max_completion_tokens")
            .or_else(|| root.get("max_tokens"))
        {
            canonical["max_output_tokens"] = limit.clone();
        }
        serde_json::to_vec(&canonical).unwrap_or_default()
    };
    let response = handler
        .handle_provider_route_with_headers(provider, headers, &canonical)
        .await;
    let source = match response {
        OpenAiResponsesRouteResponse::Buffered(response) => {
            if !(200..300).contains(&response.status()) {
                return OpenAiChatRouteResponse::Buffered(response);
            }
            if !stream {
                let Ok(value) = serde_json::from_slice::<Value>(response.body()) else {
                    return buffered_error(502, "upstream returned an invalid response");
                };
                if value.get("choices").is_some() {
                    let payload = if completions {
                        convert_chat_completions_response_to_completions(response.body())
                    } else {
                        response.body().to_vec()
                    };
                    return OpenAiChatRouteResponse::Buffered(OpenAiResponsesHttpResponse::json(
                        response.status(),
                        payload,
                    ));
                }
                if value.get("type").is_none()
                    && !matches!(
                        value.get("status").and_then(Value::as_str),
                        Some("completed" | "incomplete")
                    )
                {
                    return buffered_error(502, "upstream returned an invalid response");
                }
                let event = if value.get("type").is_some() {
                    value
                } else {
                    json!({"type":if value.get("status").and_then(Value::as_str) == Some("incomplete") {
                        "response.incomplete"
                    } else { "response.completed" }, "response":value})
                };
                let payload = serde_json::to_vec(&event).unwrap_or_default();
                let mut converted =
                    convert_codex_response_to_openai_chat_non_stream(&chat, &canonical, &payload);
                if converted.is_empty() {
                    return buffered_error(502, "upstream returned an invalid response");
                }
                if completions {
                    converted = convert_chat_completions_response_to_completions(&converted);
                }
                return OpenAiChatRouteResponse::Buffered(OpenAiResponsesHttpResponse::json(
                    response.status(),
                    converted,
                ));
            }
            if !response.content_type().starts_with("text/event-stream") {
                return buffered_error(502, "upstream did not return an event stream");
            }
            ChatStreamSource::Buffered(Some(response.body().to_vec()))
        }
        OpenAiResponsesRouteResponse::Stream(source) => ChatStreamSource::Claude(source),
        OpenAiResponsesRouteResponse::CodexStream(source) => ChatStreamSource::Codex(source),
        OpenAiResponsesRouteResponse::AntigravityStream(source) => {
            ChatStreamSource::Antigravity(source)
        }
    };
    if !stream {
        return buffered_error(502, "upstream returned an unexpected event stream");
    }
    let mut stream = OpenAiChatStream {
        source,
        decoder: SseDecoder::new(),
        state: CodexToChatStreamState::default(),
        model: model.to_owned(),
        chat,
        canonical,
        completions,
        pending: VecDeque::new(),
        saw_finish_reason: false,
        saw_event: false,
        failed: false,
        ended: false,
    };
    // Preserve the JSON error envelope when no client-visible chunk has yet
    // committed SSE headers. No background producer or detached forwarder.
    stream.fill_pending().await;
    if stream.pending.front().is_some_and(|chunk| chunk.1) {
        return buffered_error(502, "upstream stream failed before output");
    }
    OpenAiChatRouteResponse::Stream(Box::new(stream))
}

fn buffered_error(status: u16, message: &str) -> OpenAiChatRouteResponse {
    OpenAiChatRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(status, message))
}

#[derive(Debug)]
pub(crate) enum OpenAiChatRouteResponse {
    Buffered(OpenAiResponsesHttpResponse),
    Stream(Box<OpenAiChatStream>),
}

impl OpenAiChatRouteResponse {
    pub(crate) fn status_and_content_type(&self) -> (u16, &'static str) {
        match self {
            Self::Buffered(response) => (response.status(), response.content_type()),
            Self::Stream(_) => (200, "text/event-stream"),
        }
    }
}

enum ChatStreamSource {
    Buffered(Option<Vec<u8>>),
    Claude(Box<OpenAiResponsesStreamBootstrap>),
    Codex(Box<OpenAiResponsesCodexStream>),
    Antigravity(Box<OpenAiResponsesAntigravityStream>),
}

impl ChatStreamSource {
    async fn next_chunk(&mut self) -> Option<Vec<u8>> {
        match self {
            Self::Buffered(body) => body.take(),
            Self::Claude(source) => source.next_chunk().await.map(delimit_event),
            Self::Codex(source) => source.next_chunk().await,
            Self::Antigravity(source) => source.next_chunk().await.map(delimit_event),
        }
    }
}

fn delimit_event(mut event: Vec<u8>) -> Vec<u8> {
    event.extend_from_slice(b"\n\n");
    event
}

pub(crate) struct OpenAiChatStream {
    source: ChatStreamSource,
    decoder: SseDecoder,
    state: CodexToChatStreamState,
    model: String,
    chat: Vec<u8>,
    canonical: Vec<u8>,
    completions: bool,
    pending: VecDeque<(Vec<u8>, bool)>,
    saw_finish_reason: bool,
    saw_event: bool,
    failed: bool,
    ended: bool,
}

impl std::fmt::Debug for OpenAiChatStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiChatStream")
            .field("completions", &self.completions)
            .field("pending_chunks", &self.pending.len())
            .field("saw_finish_reason", &self.saw_finish_reason)
            .field("failed", &self.failed)
            .field("ended", &self.ended)
            .finish()
    }
}

impl OpenAiChatStream {
    pub(crate) async fn next_chunk(&mut self) -> Option<Vec<u8>> {
        if self.pending.is_empty() {
            self.fill_pending().await;
        }
        self.pending.pop_front().map(|chunk| chunk.0)
    }

    async fn fill_pending(&mut self) {
        while self.pending.is_empty() && !self.ended {
            match self.source.next_chunk().await {
                Some(chunk) => {
                    let events = self.decoder.push(&chunk);
                    self.enqueue(events);
                }
                None => {
                    let events = self.decoder.finish();
                    self.enqueue(events);
                    self.close();
                }
            }
        }
    }

    fn enqueue(&mut self, events: Vec<SseEvent>) {
        for event in events {
            if self.ended {
                break;
            }
            if event.data == b"[DONE]" {
                self.close();
                break;
            }
            self.saw_event = true;
            let Ok(value) = serde_json::from_slice::<Value>(&event.data) else {
                self.fail("upstream stream returned invalid JSON");
                break;
            };
            let kind = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if value.get("error").is_some_and(|error| !error.is_null())
                || matches!(kind, "response.failed" | "error")
                || event
                    .event
                    .as_deref()
                    .is_some_and(|v| matches!(v, "response.failed" | "error"))
            {
                self.fail("upstream stream failed");
                break;
            }
            // Already-compatible chunks are forwarded unchanged. Canonical
            // Responses events use the existing request-local translator state.
            let chunks = if value.get("choices").is_some() {
                vec![event.data]
            } else {
                let mut data = b"data: ".to_vec();
                data.extend_from_slice(&event.data);
                convert_codex_response_to_openai_chat_stream(
                    &self.model,
                    &self.chat,
                    &self.canonical,
                    &data,
                    &mut self.state,
                )
            };
            for chunk in chunks {
                let data = chunk.strip_prefix(b"data:").unwrap_or(&chunk);
                let data = trim_ascii(data);
                let payload = if self.completions {
                    match convert_chat_completions_stream_chunk_to_completions(data) {
                        Some(payload) => payload,
                        None => continue,
                    }
                } else {
                    data.to_vec()
                };
                self.saw_finish_reason |= chunk_has_finish_reason(&payload);
                self.pending.push_back((frame_data(&payload), false));
            }
        }
    }

    fn close(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        if self.failed {
            return;
        }
        // ref: upstream openai_handlers.go:707-751 @ a4acc9f7.
        // Includes the first chunk and any later usage-only chunks. Null,
        // absent and empty finish reasons never certify a complete response.
        if self.saw_finish_reason || !self.saw_event {
            self.pending
                .push_back((b"data: [DONE]\n\n".to_vec(), false));
        } else {
            self.fail("upstream stream closed before any chunk carried finish_reason");
        }
    }

    fn fail(&mut self, message: &str) {
        if self.failed {
            return;
        }
        self.failed = true;
        self.ended = true;
        let error = json!({"error":{"message":message,"type":"server_error","code":502}});
        let payload = serde_json::to_vec(&error).unwrap_or_default();
        self.pending.push_back((frame_data(&payload), true));
    }
}

// ref: upstream openai_handlers.go:707-713 @ a4acc9f7
fn chunk_has_finish_reason(chunk: &[u8]) -> bool {
    let chunk = trim_ascii(chunk);
    let chunk = trim_ascii(chunk.strip_prefix(b"data:").unwrap_or(chunk));
    serde_json::from_slice::<Value>(chunk)
        .ok()
        .and_then(|root| root.get("choices").and_then(Value::as_array).cloned())
        .is_some_and(|choices| {
            choices.iter().any(|choice| {
                choice
                    .get("finish_reason")
                    .is_some_and(|reason| !reason.is_null() && !value_string(reason).is_empty())
            })
        })
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

fn frame_data(payload: &[u8]) -> Vec<u8> {
    let mut framed = Vec::with_capacity(payload.len() + 8);
    for line in payload.split(|byte| *byte == b'\n') {
        framed.extend_from_slice(b"data: ");
        framed.extend_from_slice(line);
        framed.push(b'\n');
    }
    framed.push(b'\n');
    framed
}

fn value_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => String::new(),
        value => value.to_string(),
    }
}
