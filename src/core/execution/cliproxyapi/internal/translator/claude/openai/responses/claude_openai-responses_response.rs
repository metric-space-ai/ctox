// ref: internal/translator/claude/openai/responses/claude_openai-responses_response.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::translator::common::sse_event_data;
use crate::internal::translator::common::SseDecoder;
use crate::sdk::translator::TranslationContext;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) const CLAUDE_RESPONSES_REDACTED_THINKING_PREFIX: &str = "claude-redacted-thinking:";

fn claude_reasoning_carrier(content_block: &Value) -> String {
    if content_block.get("type").and_then(Value::as_str) == Some("redacted_thinking") {
        return content_block
            .get("data")
            .and_then(Value::as_str)
            .filter(|data| !data.is_empty())
            .map(|data| format!("{CLAUDE_RESPONSES_REDACTED_THINKING_PREFIX}{data}"))
            .unwrap_or_default();
    }
    content_block
        .get("signature")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

#[derive(Clone, Debug, Default)]
struct ToolState {
    id: String,
    name: String,
    arguments: String,
    output_index: usize,
}

#[derive(Clone, Debug, Default)]
struct UsageState {
    input: u64,
    output: u64,
    cached: u64,
    cache_creation: u64,
    seen: bool,
}

impl UsageState {
    fn merge(&mut self, value: Option<&Value>) {
        let Some(value) = value else { return };
        self.seen = true;
        if let Some(v) = value.get("input_tokens").and_then(Value::as_u64) {
            self.input = v;
        }
        if let Some(v) = value.get("output_tokens").and_then(Value::as_u64) {
            self.output = v;
        }
        if let Some(v) = value.get("cache_read_input_tokens").and_then(Value::as_u64) {
            self.cached = v;
        }
        if let Some(v) = value
            .get("cache_creation_input_tokens")
            .and_then(Value::as_u64)
        {
            self.cache_creation = v;
        }
    }
    fn response(&self) -> Value {
        let input = self.input + self.cached + self.cache_creation;
        json!({
            "input_tokens":input, "input_tokens_details":{"cached_tokens":self.cached},
            "output_tokens":self.output, "output_tokens_details":{}, "total_tokens":input + self.output
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ClaudeToResponsesState {
    sequence: u64,
    response_id: String,
    stop_reason: String,
    text: String,
    text_item_id: String,
    text_output_index: Option<usize>,
    reasoning: String,
    reasoning_signature: String,
    reasoning_item_id: String,
    reasoning_block_index: Option<usize>,
    reasoning_output_index: Option<usize>,
    reasoning_done: bool,
    tools: BTreeMap<usize, ToolState>,
    annotations: Vec<Value>,
    usage: UsageState,
    next_output_index: usize,
    text_done: bool,
    message_count: usize,
    reasoning_chars: usize,
    finished_items: BTreeMap<usize, Value>,
    terminal: bool,
}

#[derive(Debug, Default)]
pub struct ClaudeResponsesStreamDecoder {
    decoder: SseDecoder,
    state: ClaudeToResponsesState,
}

impl ClaudeResponsesStreamDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push(
        &mut self,
        context: &TranslationContext,
        model: &str,
        original_request: &[u8],
        request: &[u8],
        bytes: &[u8],
    ) -> Vec<Vec<u8>> {
        if context.is_cancelled() {
            return Vec::new();
        }
        let events = self.decoder.push(bytes);
        self.translate_events(context, model, original_request, request, events)
    }

    pub fn finish(
        &mut self,
        context: &TranslationContext,
        model: &str,
        original_request: &[u8],
        request: &[u8],
    ) -> Vec<Vec<u8>> {
        if context.is_cancelled() {
            return Vec::new();
        }
        let events = self.decoder.finish();
        self.translate_events(context, model, original_request, request, events)
    }

    fn translate_events(
        &mut self,
        context: &TranslationContext,
        model: &str,
        original_request: &[u8],
        request: &[u8],
        events: Vec<crate::internal::translator::common::SseEvent>,
    ) -> Vec<Vec<u8>> {
        let mut output = Vec::new();
        for event in events {
            if context.is_cancelled() {
                break;
            }
            output.extend(convert_claude_response_to_openai_responses(
                model,
                original_request,
                request,
                &event.data,
                &mut self.state,
            ));
        }
        output
    }
}

pub fn convert_claude_response_to_openai_responses_non_stream(
    original_request: &[u8],
    request: &[u8],
    raw_sse: &[u8],
) -> Vec<u8> {
    #[derive(Debug)]
    enum OutputKind {
        Message,
        FunctionCall,
        Reasoning,
    }
    #[derive(Debug)]
    struct OutputItem {
        kind: OutputKind,
        id: String,
        call_id: String,
        name: String,
        text: String,
        signature: String,
        annotations: Vec<Value>,
        arguments: String,
    }

    let mut response_id = String::new();
    let mut stop_reason = String::new();
    let mut usage = UsageState::default();
    let mut items = Vec::<OutputItem>::new();
    let mut block_to_item = BTreeMap::<usize, usize>::new();
    let mut message_count = 0_usize;
    let mut active_message = None;
    let mut pending_annotations = Vec::<Value>::new();
    for line in raw_sse.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(payload) = line.strip_prefix(b"data:") else {
            continue;
        };
        let payload = payload.strip_prefix(b" ").unwrap_or(payload);
        let Ok(event) = serde_json::from_slice::<Value>(payload) else {
            continue;
        };
        match event.get("type").and_then(Value::as_str).unwrap_or("") {
            "message_start" => {
                response_id = event
                    .pointer("/message/id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                usage.merge(event.pointer("/message/usage"));
                if let Some(reason) = event.pointer("/message/stop_reason").and_then(Value::as_str) {
                    stop_reason = reason.to_owned();
                }
            }
            "content_block_start" => {
                let block_index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let block = &event["content_block"];
                let kind = match block.get("type").and_then(Value::as_str).unwrap_or("") {
                    "text" => OutputKind::Message,
                    "tool_use" => OutputKind::FunctionCall,
                    "thinking" | "redacted_thinking" => OutputKind::Reasoning,
                    _ => continue,
                };
                let mut item = OutputItem {
                    kind,
                    id: String::new(),
                    call_id: String::new(),
                    name: String::new(),
                    text: String::new(),
                    signature: String::new(),
                    annotations: Vec::new(),
                    arguments: String::new(),
                };
                match item.kind {
                    OutputKind::Message => {
                        item.id = format!("msg_{response_id}_{message_count}");
                        message_count += 1;
                        item.annotations.append(&mut pending_annotations);
                        active_message = Some(items.len());
                    }
                    OutputKind::FunctionCall => {
                        active_message = None;
                        item.call_id = block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        item.id = format!("fc_{}", item.call_id);
                        item.name = block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                    }
                    OutputKind::Reasoning => {
                        active_message = None;
                        item.id = format!("rs_{response_id}_{block_index}");
                        item.signature = claude_reasoning_carrier(block);
                    }
                }
                block_to_item.insert(block_index, items.len());
                items.push(item);
            }
            "content_block_delta" => {
                let block_index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let item_index = block_to_item.get(&block_index).copied();
                let delta = &event["delta"];
                match delta.get("type").and_then(Value::as_str).unwrap_or("") {
                    "text_delta" => {
                        if let Some(item) = item_index.and_then(|index| items.get_mut(index)) {
                            if matches!(item.kind, OutputKind::Message) {
                                item.text.push_str(
                                    delta.get("text").and_then(Value::as_str).unwrap_or(""),
                                );
                            }
                        }
                    }
                    "input_json_delta" => {
                        if let Some(item) = item_index.and_then(|index| items.get_mut(index)) {
                            if matches!(item.kind, OutputKind::FunctionCall) {
                                item.arguments.push_str(
                                    delta
                                        .get("partial_json")
                                        .and_then(Value::as_str)
                                        .unwrap_or(""),
                                );
                            }
                        }
                    }
                    "thinking_delta" => {
                        if let Some(item) = item_index.and_then(|index| items.get_mut(index)) {
                            if matches!(item.kind, OutputKind::Reasoning) {
                                item.text.push_str(
                                    delta.get("thinking").and_then(Value::as_str).unwrap_or(""),
                                );
                            }
                        }
                    }
                    "signature_delta" => {
                        if let Some(item) = item_index.and_then(|index| items.get_mut(index)) {
                            if matches!(item.kind, OutputKind::Reasoning) {
                                if let Some(signature) =
                                    delta.get("signature").and_then(Value::as_str)
                                {
                                    if !signature.is_empty() {
                                        item.signature = signature.to_owned();
                                    }
                                }
                            }
                        }
                    }
                    "citations_delta" => {
                        if let Some(citation) =
                            delta.get("citation").filter(|value| !value.is_null())
                        {
                            if let Some(item) = item_index.and_then(|index| items.get_mut(index)) {
                                if matches!(item.kind, OutputKind::Message) {
                                    item.annotations.push(citation.clone());
                                    continue;
                                }
                            }
                            if let Some(item) =
                                active_message.and_then(|index| items.get_mut(index))
                            {
                                item.annotations.push(citation.clone());
                            } else {
                                pending_annotations.push(citation.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                usage.merge(event.get("usage"));
                if let Some(reason) = event.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    stop_reason = reason.to_owned();
                }
            }
            _ => {}
        }
    }
    let request = pick_request(original_request, request);
    let mut state = ClaudeToResponsesState {
        response_id,
        stop_reason,
        usage,
        ..ClaudeToResponsesState::default()
    };
    let mut output = Vec::with_capacity(items.len());
    let mut reasoning_chars = 0_usize;
    for item in items {
        match item.kind {
            OutputKind::Reasoning => {
                reasoning_chars += item.text.len();
                output.push(json!({
                    "id": item.id,
                    "type": "reasoning",
                    "encrypted_content": item.signature,
                    "summary": [{"type":"summary_text", "text":item.text}]
                }));
            }
            OutputKind::Message => {
                let mut message = message_item(&item.id, &item.text);
                message["status"] = Value::String(claude_responses_terminal_state(&state.stop_reason).1.into());
                message["content"][0]["annotations"] = Value::Array(item.annotations);
                output.push(message);
            }
            OutputKind::FunctionCall => {
                let arguments = if item.arguments.is_empty() {
                    "{}".to_owned()
                } else {
                    item.arguments
                };
                let (name, namespace) = restore_namespace(request, &item.name);
                let mut call = json!({
                    "id": item.id,
                    "type":"function_call",
                    "status":"completed",
                    "arguments":arguments,
                    "call_id":item.call_id,
                    "name":name
                });
                if let Some(namespace) = namespace {
                    call["namespace"] = Value::String(namespace);
                }
                output.push(call);
            }
        }
    }
    state.reasoning_chars = reasoning_chars;
    let (_, status, details) = claude_responses_terminal_state(&state.stop_reason);
    let mut response = response_shell(&state, "", status, request);
    response["incomplete_details"] = details;
    response["output"] = Value::Array(output);
    let reasoning_tokens = reasoning_chars / 4;
    if reasoning_tokens > 0 {
        response["usage"]["output_tokens_details"]["reasoning_tokens"] =
            Value::from(reasoning_tokens as u64);
    }
    serde_json::to_vec(&response).unwrap_or_else(|_| b"{}".to_vec())
}

pub fn convert_claude_response_to_openai_responses(
    _model: &str,
    original_request: &[u8],
    request: &[u8],
    raw_sse: &[u8],
    state: &mut ClaudeToResponsesState,
) -> Vec<Vec<u8>> {
    let payload = raw_sse
        .strip_prefix(b"data:")
        .map(|v| v.strip_prefix(b" ").unwrap_or(v))
        .unwrap_or(raw_sse);
    let Ok(event) = serde_json::from_slice::<Value>(payload) else {
        return Vec::new();
    };
    let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");
    let mut output = Vec::new();
    match event_type {
        "message_start" => {
            state.apply(&event);
            let response = json!({
                "id": state.response_id,
                "object": "response",
                "created_at": 0,
                "status": "in_progress",
                "background": false,
                "error": null,
                "output": []
            });
            output.push(state.emit(
                "response.created",
                json!({"type":"response.created", "response":response}),
            ));
            let response = json!({
                "id": state.response_id,
                "object": "response",
                "created_at": 0,
                "status": "in_progress"
            });
            output.push(state.emit(
                "response.in_progress",
                json!({"type":"response.in_progress", "response":response}),
            ));
        }
        "content_block_start" => {
            let index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            let block = event.get("content_block").cloned().unwrap_or(Value::Null);
            match block.get("type").and_then(Value::as_str).unwrap_or("") {
                "text" => {
                    if state.text_output_index.is_none() {
                        let output_index = state.allocate();
                        state.text_output_index = Some(output_index);
                        state.text_done = false;
                        state.text_item_id =
                            format!("msg_{}_{}", state.response_id, state.message_count);
                        let item = json!({"id":state.text_item_id,"type":"message","status":"in_progress","content":[],"role":"assistant"});
                        output.push(state.emit("response.output_item.added", json!({"type":"response.output_item.added", "output_index":output_index, "item":item})));
                        output.push(state.emit("response.content_part.added", json!({"type":"response.content_part.added", "item_id":state.text_item_id, "output_index":output_index, "content_index":0, "part":{"type":"output_text", "annotations":[], "logprobs":[], "text":""}})));
                    }
                }
                "tool_use" => {
                    output.extend(finish_text(state));
                    let output_index = state.allocate();
                    let tool = ToolState {
                        id: block["id"].as_str().unwrap_or("").into(),
                        name: block["name"].as_str().unwrap_or("").into(),
                        arguments: String::new(),
                        output_index,
                    };
                    state.tools.insert(index, tool.clone());
                    let (name, namespace) =
                        restore_namespace(pick_request(original_request, request), &tool.name);
                    let mut item = json!({"id":format!("fc_{}", tool.id), "type":"function_call", "status":"in_progress", "arguments":"", "call_id":tool.id, "name":name});
                    if let Some(namespace) = namespace {
                        item["namespace"] = Value::String(namespace);
                    }
                    output.push(state.emit("response.output_item.added", json!({"type":"response.output_item.added", "output_index":output_index, "item":item})));
                }
                "thinking" | "redacted_thinking" => {
                    output.extend(finish_text(state));
                    let output_index = state.allocate();
                    state.reasoning_block_index = Some(index);
                    state.reasoning_output_index = Some(output_index);
                    state.reasoning_done = false;
                    state.reasoning.clear();
                    state.reasoning_item_id = format!("rs_{}_{}", state.response_id, index);
                    state.reasoning_signature = claude_reasoning_carrier(&block);
                    let item = reasoning_item(state, "in_progress");
                    output.push(state.emit("response.output_item.added", json!({"type":"response.output_item.added", "output_index":output_index, "item":item})));
                    output.push(state.emit("response.reasoning_summary_part.added", json!({"type":"response.reasoning_summary_part.added", "item_id":state.reasoning_item_id, "output_index":output_index, "summary_index":0, "part":{"type":"summary_text", "text":""}})));
                }
                _ => {}
            }
        }
        "content_block_delta" => {
            let index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            let delta = event.get("delta").cloned().unwrap_or(Value::Null);
            match delta.get("type").and_then(Value::as_str).unwrap_or("") {
                "text_delta" => {
                    let text = delta.get("text").and_then(Value::as_str).unwrap_or("");
                    state.text.push_str(text);
                    output.push(state.emit("response.output_text.delta", json!({"type":"response.output_text.delta", "item_id":state.text_item_id, "output_index":state.text_output_index.unwrap_or(0), "content_index":0, "delta":text, "logprobs":[]})));
                }
                "input_json_delta" => {
                    let partial = delta
                        .get("partial_json")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if let Some(tool) = state.tools.get_mut(&index) {
                        tool.arguments.push_str(partial);
                    }
                    if let Some(tool) = state.tools.get(&index).cloned() {
                        output.push(state.emit("response.function_call_arguments.delta", json!({"type":"response.function_call_arguments.delta", "item_id":format!("fc_{}",tool.id), "output_index":tool.output_index, "delta":partial})));
                    }
                }
                "thinking_delta" => {
                    let text = delta.get("thinking").and_then(Value::as_str).unwrap_or("");
                    state.reasoning.push_str(text);
                    output.push(state.emit("response.reasoning_summary_text.delta", json!({"type":"response.reasoning_summary_text.delta", "item_id":state.reasoning_item_id, "output_index":state.reasoning_output_index.unwrap_or(0), "summary_index":0, "delta":text})));
                }
                "signature_delta" => {
                    state.reasoning_signature = delta
                        .get("signature")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into();
                }
                "citations_delta" => {
                    if let Some(citation) = delta.get("citation") {
                        state.annotations.push(citation.clone());
                    }
                }
                _ => {}
            }
            state.usage.merge(event.get("usage"));
        }
        "content_block_stop" => {
            let index = event.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            if let Some(tool) = state.tools.get(&index).cloned() {
                let args = if tool.arguments.is_empty() {
                    "{}"
                } else {
                    &tool.arguments
                };
                output.push(state.emit("response.function_call_arguments.done", json!({"type":"response.function_call_arguments.done", "item_id":format!("fc_{}",tool.id), "output_index":tool.output_index, "arguments":args})));
                let (name, namespace) =
                    restore_namespace(pick_request(original_request, request), &tool.name);
                let mut item = json!({"id":format!("fc_{}",tool.id), "type":"function_call", "status":"completed", "arguments":args, "call_id":tool.id, "name":name});
                if let Some(namespace) = namespace {
                    item["namespace"] = Value::String(namespace);
                }
                state.finished_items.insert(tool.output_index, item.clone());
                output.push(state.emit("response.output_item.done", json!({"type":"response.output_item.done", "output_index":tool.output_index, "item":item})));
            } else if state.reasoning_block_index == Some(index) {
                output.extend(finish_reasoning(state));
            }
        }
        "message_delta" => state.apply(&event),
        "message_stop" => {
            if state.terminal {
                return Vec::new();
            }
            output.extend(finish_text(state));
            let completed = stream_completed(state, pick_request(original_request, request));
            let event_type = claude_responses_terminal_state(&state.stop_reason).0;
            output.push(state.emit(event_type, json!({"type":event_type, "response":completed})));
            state.terminal = true;
        }
        "error" => {
            if state.terminal {
                return Vec::new();
            }
            output.extend(finish_text(state));
            let provider_error = event.get("error").cloned().unwrap_or(Value::Null);
            let error_type = provider_error
                .get("type")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("upstream_error");
            let message = provider_error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Claude upstream request failed");
            let mut failed = stream_completed(state, pick_request(original_request, request));
            failed["status"] = Value::String("failed".into());
            failed["error"] = json!({"type":error_type, "code":error_type, "message":message});
            output.push(state.emit(
                "response.failed",
                json!({"type":"response.failed", "response":failed}),
            ));
            state.terminal = true;
        }
        _ => {}
    }
    output
}

fn finish_text(state: &mut ClaudeToResponsesState) -> Vec<Vec<u8>> {
    let Some(index) = state.text_output_index else {
        return Vec::new();
    };
    if state.text_done {
        return Vec::new();
    }
    state.text_done = true;
    let item_id = state.text_item_id.clone();
    let text = state.text.clone();
    let annotations = state.annotations.clone();
    let mut item = message_item(&item_id, &text);
    item["content"][0]["annotations"] = Value::Array(annotations.clone());
    state.finished_items.insert(index, item.clone());
    state.message_count += 1;
    state.text_output_index = None;
    state.text_item_id.clear();
    state.text.clear();
    state.annotations.clear();
    vec![
        state.emit("response.output_text.done", json!({"type":"response.output_text.done", "item_id":item_id, "output_index":index, "content_index":0, "text":text, "logprobs":[]})),
        state.emit("response.content_part.done", json!({"type":"response.content_part.done", "item_id":item_id, "output_index":index, "content_index":0, "part":{"type":"output_text", "annotations":annotations, "logprobs":[], "text":text}})),
        state.emit("response.output_item.done", json!({"type":"response.output_item.done", "output_index":index, "item":item})),
    ]
}

fn finish_reasoning(state: &mut ClaudeToResponsesState) -> Vec<Vec<u8>> {
    let Some(index) = state.reasoning_output_index else {
        return Vec::new();
    };
    if state.reasoning_done {
        return Vec::new();
    }
    state.reasoning_done = true;
    let item_id = state.reasoning_item_id.clone();
    let text = state.reasoning.clone();
    let item = json!({"id":item_id,"type":"reasoning","encrypted_content":state.reasoning_signature,"summary":[{"type":"summary_text","text":text}]});
    state.finished_items.insert(index, item.clone());
    state.reasoning_chars += text.len();
    state.reasoning_output_index = None;
    state.reasoning_block_index = None;
    state.reasoning.clear();
    state.reasoning_signature.clear();
    vec![
        state.emit("response.reasoning_summary_text.done", json!({"type":"response.reasoning_summary_text.done", "item_id":item_id, "output_index":index, "summary_index":0, "text":text})),
        state.emit("response.reasoning_summary_part.done", json!({"type":"response.reasoning_summary_part.done", "item_id":item_id, "output_index":index, "summary_index":0, "part":{"type":"summary_text", "text":text}})),
        state.emit("response.output_item.done", json!({"type":"response.output_item.done", "output_index":index, "item":item})),
    ]
}

// ref: internal/translator/claude/openai/responses/claude_openai-responses_response.go:166-192 @ 2044a01f
fn claude_responses_terminal_state(stop_reason: &str) -> (&'static str, &'static str, Value) {
    if stop_reason.trim().eq_ignore_ascii_case("max_tokens") {
        ("response.incomplete", "incomplete", json!({"reason":"max_output_tokens"}))
    } else if stop_reason.trim().eq_ignore_ascii_case("pause_turn") {
        // Server-tool iteration pauses are unfinished, without a Responses token-limit reason.
        ("response.incomplete", "incomplete", Value::Null)
    } else {
        ("response.completed", "completed", Value::Null)
    }
}

fn stream_completed(state: &ClaudeToResponsesState, request: &[u8]) -> Value {
    let (_, status, details) = claude_responses_terminal_state(&state.stop_reason);
    let mut response = json!({
        "id": state.response_id,
        "object": "response",
        "created_at": 0,
        "status": status,
        "incomplete_details": details,
        "background": false,
        "error": null
    });
    if let Ok(req) = serde_json::from_slice::<Value>(request) {
        copy_response_request_fields(&mut response, &req);
    }

    let mut indexed = state.finished_items.clone();
    for tool in state.tools.values() {
        let args = if tool.arguments.is_empty() {
            "{}"
        } else {
            &tool.arguments
        };
        let (name, namespace) = restore_namespace(request, &tool.name);
        let mut item = json!({"id":format!("fc_{}",tool.id),"type":"function_call","status":"completed","arguments":args,"call_id":tool.id,"name":name});
        if let Some(namespace) = namespace {
            item["namespace"] = Value::String(namespace);
        }
        indexed.insert(tool.output_index, item);
    }
    for item in indexed.values_mut() {
        if item.get("type").and_then(Value::as_str) == Some("message") {
            item["status"] = Value::String(status.into());
        }
    }
    if !indexed.is_empty() {
        response["output"] = Value::Array(indexed.into_values().collect());
    }
    let reasoning_tokens = state.reasoning_chars as u64 / 4;
    if state.usage.seen || reasoning_tokens > 0 {
        response["usage"] = state.usage.response();
        if reasoning_tokens > 0 {
            response["usage"]["output_tokens_details"]["reasoning_tokens"] =
                Value::from(reasoning_tokens);
        } else if let Some(usage) = response["usage"].as_object_mut() {
            usage.remove("output_tokens_details");
        }
    }
    response
}

impl ClaudeToResponsesState {
    fn next_seq(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }
    fn allocate(&mut self) -> usize {
        let value = self.next_output_index;
        self.next_output_index += 1;
        value
    }
    fn emit(&mut self, event: &str, mut payload: Value) -> Vec<u8> {
        payload["sequence_number"] = Value::from(self.next_seq());
        sse_event_data(event, &serde_json::to_vec(&payload).unwrap_or_default())
    }
    fn apply(&mut self, event: &Value) {
        match event.get("type").and_then(Value::as_str).unwrap_or("") {
            "message_start" => {
                let sequence = self.sequence;
                *self = Self::default();
                self.sequence = sequence;
                let msg = &event["message"];
                self.response_id = msg["id"].as_str().unwrap_or("").into();
                self.usage.merge(msg.get("usage"));
                if let Some(reason) = msg.get("stop_reason").and_then(Value::as_str) {
                    self.stop_reason = reason.to_owned();
                }
            }
            "message_delta" => {
                self.usage.merge(event.get("usage"));
                if let Some(reason) = event.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop_reason = reason.to_owned();
                }
            }
            "content_block_start" => {
                let index = event["index"].as_u64().unwrap_or(0) as usize;
                let block = &event["content_block"];
                match block["type"].as_str().unwrap_or("") {
                    "text" => {
                        self.text_item_id = format!("msg_{}_0", self.response_id);
                        self.text_done = false;
                        self.text_output_index.get_or_insert_with(|| {
                            let v = self.next_output_index;
                            self.next_output_index += 1;
                            v
                        });
                    }
                    "thinking" | "redacted_thinking" => {
                        self.reasoning_block_index = Some(index);
                        self.reasoning_item_id = format!("rs_{}_{}", self.response_id, index);
                        self.reasoning_done = false;
                        self.reasoning_signature = claude_reasoning_carrier(block);
                        self.reasoning_output_index.get_or_insert_with(|| {
                            let v = self.next_output_index;
                            self.next_output_index += 1;
                            v
                        });
                    }
                    "tool_use" => {
                        let output_index = self.next_output_index;
                        self.next_output_index += 1;
                        self.tools.insert(
                            index,
                            ToolState {
                                id: block["id"].as_str().unwrap_or("").into(),
                                name: block["name"].as_str().unwrap_or("").into(),
                                arguments: String::new(),
                                output_index,
                            },
                        );
                    }
                    _ => {}
                }
            }
            "content_block_delta" => {
                let index = event["index"].as_u64().unwrap_or(0) as usize;
                let d = &event["delta"];
                match d["type"].as_str().unwrap_or("") {
                    "text_delta" => self.text.push_str(d["text"].as_str().unwrap_or("")),
                    "thinking_delta" => self
                        .reasoning
                        .push_str(d["thinking"].as_str().unwrap_or("")),
                    "signature_delta" => {
                        self.reasoning_signature = d["signature"].as_str().unwrap_or("").into()
                    }
                    "input_json_delta" => {
                        if let Some(t) = self.tools.get_mut(&index) {
                            t.arguments
                                .push_str(d["partial_json"].as_str().unwrap_or(""))
                        }
                    }
                    "citations_delta" => {
                        if !d["citation"].is_null() {
                            self.annotations.push(d["citation"].clone())
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn response_shell(
    state: &ClaudeToResponsesState,
    model: &str,
    status: &str,
    request: &[u8],
) -> Value {
    let mut response = json!({"id":state.response_id,"object":"response","created_at":0,"status":status,"background":false,"error":null,"incomplete_details":null,"output":[],"usage":state.usage.response()});
    if let Ok(req) = serde_json::from_slice::<Value>(request) {
        copy_response_request_fields(&mut response, &req);
    }
    if response.get("model").is_none() && !model.is_empty() {
        response["model"] = Value::String(model.into());
    }
    response
}

fn copy_response_request_fields(response: &mut Value, request: &Value) {
    for key in [
        "instructions",
        "max_output_tokens",
        "max_tool_calls",
        "model",
        "parallel_tool_calls",
        "previous_response_id",
        "prompt_cache_key",
        "reasoning",
        "safety_identifier",
        "service_tier",
        "store",
        "temperature",
        "text",
        "tool_choice",
        "tools",
        "top_logprobs",
        "top_p",
        "truncation",
        "user",
        "metadata",
    ] {
        if let Some(value) = request.get(key) {
            response[key] = value.clone();
        }
    }
}
fn message_item(id: &str, text: &str) -> Value {
    json!({"id":id,"type":"message","status":"completed","content":[{"type":"output_text","annotations":[],"logprobs":[],"text":text}],"role":"assistant"})
}
fn reasoning_item(state: &ClaudeToResponsesState, status: &str) -> Value {
    let summary = if state.reasoning.is_empty() {
        Vec::new()
    } else {
        vec![json!({"type":"summary_text","text":state.reasoning})]
    };
    let mut item = json!({"id":state.reasoning_item_id,"type":"reasoning","encrypted_content":state.reasoning_signature,"summary":summary});
    if !status.is_empty() {
        item["status"] = Value::String(status.into());
    }
    item
}
fn pick_request<'a>(original: &'a [u8], translated: &'a [u8]) -> &'a [u8] {
    if serde_json::from_slice::<Value>(original).is_ok() {
        original
    } else if serde_json::from_slice::<Value>(translated).is_ok() {
        translated
    } else {
        b""
    }
}
fn restore_namespace(request: &[u8], qualified: &str) -> (String, Option<String>) {
    let Ok(root) = serde_json::from_slice::<Value>(request) else {
        return (qualified.into(), None);
    };
    for tool in root
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if tool["type"] == "namespace" {
            let namespace = tool["name"].as_str().unwrap_or("");
            for child in tool["tools"].as_array().into_iter().flatten() {
                let name = child["name"]
                    .as_str()
                    .or_else(|| child.pointer("/function/name").and_then(Value::as_str))
                    .unwrap_or("");
                let candidate = if namespace.ends_with("__") {
                    format!("{namespace}{name}")
                } else {
                    format!("{namespace}__{name}")
                };
                if candidate == qualified {
                    return (name.into(), Some(namespace.into()));
                }
            }
        }
    }
    (qualified.into(), None)
}
