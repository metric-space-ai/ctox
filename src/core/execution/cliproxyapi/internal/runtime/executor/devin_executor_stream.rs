// ref: internal/runtime/executor/devin_executor.go:464-1123
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Incremental Devin-to-Interactions event state. Transport/translation owners
//! consume each batch immediately; this state never buffers a complete reply.
//! No completion is manufactured when the Connect EOS trailer is missing.

use super::devin_executor_response::{
    go_utf8_text, update_devin_usage, DevinAggregateError, MAX_DEVIN_AGGREGATE_BYTES,
    MAX_DEVIN_TOOL_CALLS,
};
use super::helps::devin_proto::{
    parse_devin_frame, DevinFrameResult, DevinToolCallDelta, DevinUsage,
};
use super::helps::devin_request::DevinUtf8SplitBuffer;
use super::helps::devin_wire::{parse_devin_trailer_error, ConnectFrame, CONNECT_FLAG_END_STREAM};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt;
use uuid::Uuid;

#[derive(Default)]
pub struct DevinStreamBatch {
    /// Unframed Interactions events, in the order they must be translated.
    pub events: Vec<Vec<u8>>,
    /// Only a clean EOS may authorize the transport owner to emit [DONE].
    pub complete: bool,
    pub error: Option<DevinAggregateError>,
}

impl fmt::Debug for DevinStreamBatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinStreamBatch")
            .field("event_count", &self.events.len())
            .field("complete", &self.complete)
            .field("error", &self.error)
            .finish()
    }
}

struct ActiveTool {
    id: Vec<u8>,
    name: Vec<u8>,
}
enum PendingAction {
    Tool(DevinToolCallDelta),
    Content(Vec<u8>),
}

/// Instance-owned, bounded state for one inference attempt. Prompt, thought,
/// tool arguments, credentials and signatures are never exposed by Debug.
pub struct DevinInteractionsStream {
    interaction_id: String,
    model: String,
    responses: bool,
    stream_content_early: bool,
    next_index: usize,
    thought: Option<usize>,
    last_thought: Option<usize>,
    content: Option<usize>,
    deferred_thought_stops: Vec<usize>,
    tools: BTreeMap<usize, ActiveTool>,
    calls: HashMap<Vec<u8>, usize>,
    active_call: Option<usize>,
    tool_call_count: usize,
    pending: VecDeque<PendingAction>,
    post_tool_content: Vec<Vec<u8>>,
    thinking_buffer: DevinUtf8SplitBuffer,
    content_buffer: DevinUtf8SplitBuffer,
    thought_signatures: HashMap<usize, Vec<u8>>,
    usage: Option<DevinUsage>,
    last_stop_reason: u64,
    created: bool,
    terminal: bool,
    received_bytes: usize,
    byte_limit: usize,
}

impl fmt::Debug for DevinInteractionsStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinInteractionsStream")
            .field("tool_count", &self.tool_call_count)
            .field("queued_actions", &self.pending.len())
            .field("received_bytes", &self.received_bytes)
            .field("terminal", &self.terminal)
            .finish()
    }
}

impl DevinInteractionsStream {
    #[must_use]
    pub fn new(model: &str, response_format: &str) -> Self {
        Self::with_byte_limit(model, response_format, MAX_DEVIN_AGGREGATE_BYTES)
    }

    fn with_byte_limit(model: &str, response_format: &str, byte_limit: usize) -> Self {
        assert!(byte_limit > 0 && byte_limit <= MAX_DEVIN_AGGREGATE_BYTES);
        Self {
            interaction_id: format!("interaction_{}", &Uuid::new_v4().to_string()[..12]),
            model: model.into(),
            responses: response_format == "openai-response",
            stream_content_early: matches!(response_format, "openai" | "openai-response"),
            next_index: 0,
            thought: None,
            last_thought: None,
            content: None,
            deferred_thought_stops: Vec::new(),
            tools: BTreeMap::new(),
            calls: HashMap::new(),
            active_call: None,
            tool_call_count: 0,
            pending: VecDeque::new(),
            post_tool_content: Vec::new(),
            thinking_buffer: DevinUtf8SplitBuffer::default(),
            content_buffer: DevinUtf8SplitBuffer::default(),
            thought_signatures: HashMap::new(),
            usage: None,
            last_stop_reason: 0,
            created: false,
            terminal: false,
            received_bytes: 0,
            byte_limit,
        }
    }

    #[must_use]
    pub fn usage(&self) -> Option<&DevinUsage> {
        self.usage.as_ref()
    }

    /// Each frame is processed independently. Invalid protobuf data frames are
    /// skipped; trailer failures and the shared response byte bound are terminal.
    pub fn accept(&mut self, frame: ConnectFrame) -> DevinStreamBatch {
        if self.terminal {
            return DevinStreamBatch::default();
        }
        self.received_bytes = match self.received_bytes.checked_add(frame.payload.len()) {
            Some(size) if size <= self.byte_limit => size,
            _ => return self.fail(DevinAggregateError::ResponseTooLarge, "stream_read_error"),
        };
        if frame.flag & CONNECT_FLAG_END_STREAM != 0 {
            if let Some(error) = parse_devin_trailer_error(&frame.payload) {
                let code = error.status_code.to_string();
                return self.fail(DevinAggregateError::Trailer(error), &code);
            }
            let mut events = Vec::new();
            self.close_open_steps(&mut events);
            self.complete_event(&mut events);
            self.terminal = true;
            return batch(events, true, None);
        }
        let Ok(frame) = parse_devin_frame(&frame.payload) else {
            return DevinStreamBatch::default();
        };
        let mut events = Vec::new();
        self.apply(frame, &mut events);
        batch(events, false, None)
    }

    /// The async transport calls this on EOF, never on consumer cancellation.
    /// A late EOF after a completed/failed attempt cannot emit another terminal.
    pub fn finish(&mut self) -> DevinStreamBatch {
        if self.terminal {
            return DevinStreamBatch::default();
        }
        self.fail(DevinAggregateError::PrematureEof, "stream_truncated")
    }

    /// The framing/HTTP owner forwards its actual failure through this method.
    pub fn fail(&mut self, error: DevinAggregateError, code: &str) -> DevinStreamBatch {
        if self.terminal {
            return DevinStreamBatch::default();
        }
        let mut events = Vec::new();
        self.close_open_steps(&mut events);
        self.emit(
            &mut events,
            json!({
                "event_type": "response.failed",
                "error": {"message": error.to_string(), "code": code}
            }),
        );
        self.terminal = true;
        batch(events, false, Some(error))
    }

    fn apply(&mut self, frame: DevinFrameResult, events: &mut Vec<Value>) {
        // ref: devin_executor.go:834-906 — preserve positive usage observations.
        if frame.stop_reason != 0 {
            self.last_stop_reason = frame.stop_reason;
        }
        update_devin_usage(
            &mut self.usage,
            frame.usage,
            &frame.response_dimension_groups,
        );

        if !frame.thinking_text.is_empty() {
            if !self.pending.is_empty() {
                self.flush_pending(events);
            }
            let chunk = self.thinking_buffer.feed(&frame.thinking_text);
            if !chunk.is_empty() {
                self.stop_content(events);
                if self.thought.is_none() {
                    self.thought = Some(self.next_index);
                    self.last_thought = self.thought;
                    self.emit(
                        events,
                        json!({
                            "event_type":"step.start", "index":self.next_index,
                            "step":{"type":"thought"}
                        }),
                    );
                }
                let text = go_utf8_text(&chunk);
                self.emit(
                    events,
                    json!({
                        "event_type":"step.delta", "index":self.last_thought.unwrap_or(0),
                        "delta":{"type":"thought_summary", "text":text,
                            "content":{"type":"text","text":text}}
                    }),
                );
            }
        }

        // ref: devin_executor.go:945-979 — Responses signatures are cumulative;
        // their thought item remains open even when text starts streaming.
        if !frame.delta_signature.is_empty() {
            if self.last_thought.is_none() && self.content.is_none() {
                self.last_thought = Some(self.next_index);
                self.thought = self.last_thought;
                self.emit(
                    events,
                    json!({
                        "event_type":"step.start", "index":self.next_index,
                        "step":{"type":"thought"}
                    }),
                );
            }
            let index = self.last_thought.unwrap_or(0);
            let signature = if self.responses {
                let bytes = self.thought_signatures.entry(index).or_default();
                bytes.extend_from_slice(&frame.delta_signature);
                go_utf8_text(bytes)
            } else {
                go_utf8_text(&frame.delta_signature)
            };
            let mut event = json!({
                "event_type":"step.delta", "index":index,
                "delta":{"type":"thought_signature","signature":signature}
            });
            if !frame.delta_signature_type.is_empty() {
                event["delta"]["signature_type"] =
                    Value::String(go_utf8_text(&frame.delta_signature_type));
            }
            self.emit(events, event);
        }

        for tool in frame.tool_call_deltas {
            if self.thought.is_some() {
                self.pending.push_back(PendingAction::Tool(tool));
            } else {
                self.emit_tool(events, tool);
            }
        }
        if !frame.content_text.is_empty() {
            let chunk = self.content_buffer.feed(&frame.content_text);
            if !chunk.is_empty() {
                if self.thought.is_some()
                    && (!self.stream_content_early || !self.pending.is_empty())
                {
                    self.pending.push_back(PendingAction::Content(chunk));
                } else {
                    self.emit_content(events, chunk);
                }
            }
        }
    }

    fn emit(&mut self, events: &mut Vec<Value>, event: Value) {
        let kind = event["event_type"].as_str().unwrap_or_default();
        // ref: devin_executor.go:510-527 — early errors stay eligible for an
        // HTTP bootstrap error instead of creating an artificial successful SSE.
        if matches!(kind, "response.failed" | "interaction.failed") && !self.created {
            return;
        }
        if !self.created {
            self.created = true;
            events.push(json!({
                "event_type":"interaction.created",
                "interaction":{"id":self.interaction_id,"model":self.model}
            }));
        }
        events.push(event);
    }

    fn stop_thought(&mut self, events: &mut Vec<Value>, defer_for_responses: bool) {
        if let Some(index) = self.thought.take() {
            if defer_for_responses && self.responses {
                self.deferred_thought_stops.push(index);
            } else {
                self.emit(events, json!({"event_type":"step.stop", "index":index}));
            }
            self.next_index += 1;
        }
    }

    fn stop_content(&mut self, events: &mut Vec<Value>) {
        if let Some(index) = self.content.take() {
            self.emit(events, json!({"event_type":"step.stop", "index":index}));
            self.next_index += 1;
        }
    }

    fn flush_pending(&mut self, events: &mut Vec<Value>) {
        self.stop_thought(events, false);
        while let Some(action) = self.pending.pop_front() {
            match action {
                PendingAction::Tool(tool) => self.emit_tool(events, tool),
                PendingAction::Content(text) => self.emit_content(events, text),
            }
        }
    }

    fn emit_content(&mut self, events: &mut Vec<Value>, text: Vec<u8>) {
        self.stop_thought(events, true);
        if self.tool_call_count > 0 {
            self.post_tool_content.push(text);
            return;
        }
        let index = if let Some(index) = self.content {
            index
        } else {
            let index = self.next_index;
            self.content = Some(index);
            self.emit(
                events,
                json!({
                    "event_type":"step.start","index":index,"step":{"type":"model_output"}
                }),
            );
            index
        };
        self.emit(
            events,
            json!({
                "event_type":"step.delta","index":index,
                "delta":{"type":"text","text":go_utf8_text(&text)}
            }),
        );
    }

    fn emit_tool(&mut self, events: &mut Vec<Value>, tool: DevinToolCallDelta) {
        self.stop_thought(events, false);
        self.stop_content(events);
        let existing = if tool.id.is_empty() {
            self.active_call
        } else {
            self.calls.get(&tool.id).copied()
        };
        let index = if let Some(index) = existing {
            self.active_call = Some(index);
            let slot = self.tools.get_mut(&index).expect("active slot exists");
            let mut updated = false;
            if slot.id.is_empty() && !tool.id.is_empty() {
                slot.id = tool.id.clone();
                self.calls.insert(tool.id.clone(), index);
                updated = true;
            }
            if slot.name.is_empty() && !tool.name.is_empty() {
                slot.name = tool.name.clone();
                updated = true;
            }
            if updated {
                let event = tool_start(index, slot);
                self.emit(events, event);
            }
            index
        } else {
            if self.tool_call_count >= MAX_DEVIN_TOOL_CALLS {
                return;
            }
            self.tool_call_count += 1;
            let index = self.next_index;
            self.next_index += 1;
            let slot = ActiveTool {
                id: tool.id.clone(),
                name: tool.name.clone(),
            };
            let event = tool_start(index, &slot);
            self.tools.insert(index, slot);
            if !tool.id.is_empty() {
                self.calls.insert(tool.id.clone(), index);
            }
            self.active_call = Some(index);
            self.emit(events, event);
            index
        };
        let legacy = tool.arguments.is_empty() && !tool.invalid_json_str.is_empty();
        let arguments = if tool.arguments.is_empty() {
            tool.invalid_json_str
        } else {
            tool.arguments
        };
        if !arguments.is_empty() {
            let mut event = json!({
                "event_type":"step.delta","index":index,
                "delta":{"type":"arguments_delta","arguments":go_utf8_text(&arguments)}
            });
            if legacy {
                event["delta"]["invalid_json_str"] = Value::Bool(true);
            }
            self.emit(events, event);
        }
    }

    fn close_open_steps(&mut self, events: &mut Vec<Value>) {
        // ref: devin_executor.go:754-794 — deferred thoughts, pending blocks,
        // sorted tools, then post-tool text. Final messages never overtake tools.
        for index in std::mem::take(&mut self.deferred_thought_stops) {
            self.emit(events, json!({"event_type":"step.stop", "index":index}));
        }
        self.flush_pending(events);
        for index in self.tools.keys().copied().collect::<Vec<_>>() {
            self.emit(events, json!({"event_type":"step.stop", "index":index}));
        }
        self.tools.clear();
        self.calls.clear();
        self.active_call = None;
        let buffered = std::mem::take(&mut self.post_tool_content);
        if !buffered.is_empty() {
            let index = self.next_index;
            self.content = Some(index);
            self.emit(
                events,
                json!({
                    "event_type":"step.start", "index":index, "step":{"type":"model_output"}
                }),
            );
            for text in buffered {
                self.emit(
                    events,
                    json!({
                        "event_type":"step.delta", "index":index,
                        "delta":{"type":"text","text":go_utf8_text(&text)}
                    }),
                );
            }
        }
        self.stop_content(events);
    }

    fn complete_event(&mut self, events: &mut Vec<Value>) {
        let reason = match self.last_stop_reason {
            1 | 3 => Some("length"),
            11 => Some("content_filter"),
            _ => None,
        };
        let mut event = json!({
            "event_type":"interaction.completed",
            "interaction":{"id":self.interaction_id,"model":self.model,
                "status":if reason.is_some() {"incomplete"} else {"completed"},
                "usage":{"total_input_tokens":0,"total_output_tokens":0,"total_cached_tokens":0}}
        });
        if let Some(reason) = reason {
            event["interaction"]["finish_reason"] = Value::String(reason.into());
        }
        if let Some(usage) = &self.usage {
            // Go int64 additions wrap, including unusual upstream counters.
            let input = usage.prompt_tokens.wrapping_add(usage.cached_tokens);
            let mut value = json!({
                "total_input_tokens":input,
                "total_output_tokens":usage.completion_tokens,
                "total_cached_tokens":usage.cached_tokens,
                "total_tokens":input.wrapping_add(usage.completion_tokens)
            });
            if usage.cache_write_tokens > 0 {
                value["cache_write_tokens"] = json!(usage.cache_write_tokens);
            }
            event["interaction"]["usage"] = value;
        }
        self.emit(events, event);
    }
}

fn tool_start(index: usize, slot: &ActiveTool) -> Value {
    json!({
        "event_type":"step.start","index":index,
        "step":{"type":"function_call","name":go_utf8_text(&slot.name),
            "id":go_utf8_text(&slot.id),"call_id":go_utf8_text(&slot.id),"arguments":{}}
    })
}

fn batch(
    events: Vec<Value>,
    complete: bool,
    error: Option<DevinAggregateError>,
) -> DevinStreamBatch {
    DevinStreamBatch {
        events: events
            .into_iter()
            .map(|value| serde_json::to_vec(&value).expect("constructed event JSON"))
            .collect(),
        complete,
        error,
    }
}

#[cfg(test)]
#[path = "devin_executor_stream_test.rs"]
mod tests;
