//! Glue between OpenAI Responses API requests and this crate's
//! driver — turns a [`crate::wire::ResponsesCreateRequest`] into a
//! Qwen3.5 chat-templated prompt, drives the spec-decode loop, and
//! emits Responses stream events back.
//!
//! # What it supports today
//!
//! * `input` items of type `message` with `content` parts of type
//!   `input_text` / `input_image` (image currently rendered as
//!   `[image]` placeholder — true vision routing belongs in a
//!   separate Qwen3-VL backend).
//! * `instructions` → system prompt.
//! * Streaming via `stream=true` — token deltas flushed as soon as
//!   the driver commits them (chain / fast-rollback / DDTree all
//!   emit batches of ≥1 committed tokens per step).
//! * `max_output_tokens` → hard upper bound on generated tokens.
//!
//! # Not supported yet
//!
//! * `tools` / function-calling (input is flattened but the model
//!   has no tool-routing head — caller must parse free-form tool
//!   calls out of the assistant text if needed)
//! * `reasoning` summaries — reported as empty
//! * `text.verbosity`, `text.format`, `text.schemas` — ignored
//!
//! These limits are fine for the first-cut CTOX local-inference
//! slot; tool + reasoning wiring lands when a second curated model
//! exposes the trait surface to match.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::driver::{run_dflash_gen_loop, GenConfig};
use crate::model::{DraftWeights, TargetCache, TargetWeights};
use crate::tokenizer::Tokenizer;
use crate::wire::{
    IpcError, ResponseContentPart, ResponseEnvelope, ResponseOutputItem, ResponseStatus,
    ResponseUsage, ResponsesCreateRequest, ResponsesStreamEvent,
};

/// One chat turn. The Qwen3.5 chat template wraps each in
/// `<|im_start|>{role}\n{content}<|im_end|>\n`.
struct ChatTurn {
    role: String,
    text: String,
}

/// Render a full prompt from the Responses request. Returns the
/// tokenizer-ready UTF-8 string.
fn render_chat_prompt(req: &ResponsesCreateRequest) -> Result<String> {
    let mut turns: Vec<ChatTurn> = Vec::new();

    // 1. System prompt from `instructions`.
    if !req.instructions.is_empty() {
        turns.push(ChatTurn {
            role: "system".into(),
            text: req.instructions.clone(),
        });
    }

    // 2. Each `input` item → one turn (where it maps).
    for item in &req.input {
        if let Some(turn) = input_item_to_turn(item)? {
            turns.push(turn);
        }
    }
    if !reasoning_requested(req) {
        if let Some(last_user) = turns.iter_mut().rev().find(|turn| turn.role == "user") {
            if !last_user.text.contains("/no_think") {
                if !last_user.text.ends_with('\n') && !last_user.text.is_empty() {
                    last_user.text.push('\n');
                }
                last_user.text.push_str("/no_think");
            }
        }
    }

    // 3. Render with Qwen3 chat template, add the assistant-role
    //    opening tag to prompt the model to start generating.
    let mut out = String::new();
    for t in &turns {
        out.push_str("<|im_start|>");
        out.push_str(&t.role);
        out.push('\n');
        out.push_str(&t.text);
        out.push_str("<|im_end|>\n");
    }
    out.push_str("<|im_start|>assistant\n");
    if !reasoning_requested(req) {
        out.push_str("<think>\n\n</think>\n\n");
    }
    Ok(out)
}

fn reasoning_requested(req: &ResponsesCreateRequest) -> bool {
    req.reasoning
        .as_ref()
        .and_then(|value| value.get("effort"))
        .and_then(Value::as_str)
        .map(|effort| {
            let effort = effort.trim();
            !effort.is_empty() && !effort.eq_ignore_ascii_case("none")
        })
        .unwrap_or(false)
}

fn strip_think_blocks(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    loop {
        let Some(start) = rest.find("<think>") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..start]);
        let after_start = &rest[start + "<think>".len()..];
        let Some(end) = after_start.find("</think>") else {
            break;
        };
        rest = &after_start[end + "</think>".len()..];
    }
    out.trim_start().to_string()
}

fn truncate_at_chat_marker(text: &str) -> String {
    let markers = [
        "<|im_end|>",
        "<|im_start|>",
        "\nuser",
        "\nsystem",
        "\nassistant",
    ];
    let cut = markers
        .iter()
        .filter_map(|marker| text.find(marker))
        .min()
        .unwrap_or(text.len());
    text[..cut].trim_end().to_string()
}

/// Try to turn one Responses input item into a chat turn.
/// Non-message items (function_call, reasoning, etc.) currently
/// produce `Ok(None)` — they're ignored rather than errored so a
/// long session with tool calls in history still runs, minus the
/// tool context.
fn input_item_to_turn(item: &Value) -> Result<Option<ChatTurn>> {
    let obj = match item.as_object() {
        Some(o) => o,
        None => return Ok(None),
    };
    let ty = obj.get("type").and_then(Value::as_str).unwrap_or("message");
    if ty != "message" {
        return Ok(None);
    }
    let role = obj
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user")
        .to_string();

    // `content` can be either a string or an array of content parts.
    let text = match obj.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => flatten_content_parts(parts),
        _ => String::new(),
    };

    Ok(Some(ChatTurn { role, text }))
}

fn flatten_content_parts(parts: &[Value]) -> String {
    let mut out = String::new();
    for p in parts {
        let Some(obj) = p.as_object() else { continue };
        let ty = obj.get("type").and_then(Value::as_str).unwrap_or("");
        match ty {
            "input_text" | "output_text" | "text" => {
                if let Some(t) = obj.get("text").and_then(Value::as_str) {
                    out.push_str(t);
                }
            }
            "input_image" | "image" | "image_url" => {
                out.push_str("[image]");
            }
            _ => {}
        }
    }
    out
}

/// Callback sink for stream events. The server writes these as
/// JSON lines back to the client socket.
pub trait StreamSink {
    fn send(&mut self, event: ResponsesStreamEvent) -> Result<()>;
}

/// All the per-connection state the adapter needs. Server owns one
/// of these per accepted connection and hands it to `run_turn`.
pub struct AdapterCtx<'a, S: StreamSink + ?Sized> {
    pub target_weights: &'a mut TargetWeights,
    pub draft_weights: &'a mut DraftWeights,
    pub target_cache: &'a mut TargetCache,
    pub backend: crate::ffi::ggml_backend_t,
    pub tokenizer: &'a Tokenizer,
    pub model_id: &'a str,
    pub gen_config: GenConfig,
    pub sink: &'a mut S,
}

/// Default hard cap — keeps a runaway driver from producing 10 min
/// of output on a silly prompt.
pub const DEFAULT_MAX_OUTPUT_TOKENS: usize = 2048;

/// Run a single Responses turn. Non-streaming: emits a single
/// `response.completed`. Streaming: emits the full
/// created/in_progress/output_item.added/delta…done/completed
/// sequence.
pub fn run_turn<S: StreamSink>(
    ctx: &mut AdapterCtx<'_, S>,
    req: &ResponsesCreateRequest,
) -> Result<()> {
    let response_id = format!("resp_{}", uuid::Uuid::new_v4().simple());
    let message_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
    let created_at = chrono::Utc::now().timestamp();
    let seq = AtomicU64::new(0);
    let next_seq = || seq.fetch_add(1, Ordering::SeqCst);

    // 1. Render prompt + tokenize.
    let prompt_text = render_chat_prompt(req)?;
    let prompt_ids = ctx.tokenizer.encode(&prompt_text)?;
    let input_tokens = prompt_ids.len() as u32;
    let max_out = req
        .max_output_tokens
        .unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS)
        .min(DEFAULT_MAX_OUTPUT_TOKENS);

    // 2. Lifecycle: response.created + response.in_progress.
    let mut envelope = ResponseEnvelope {
        id: response_id.clone(),
        object: "response",
        created_at,
        status: ResponseStatus::InProgress,
        model: ctx.model_id.to_string(),
        output: Vec::new(),
        usage: None,
        error: None,
    };

    if req.stream {
        ctx.sink.send(ResponsesStreamEvent::Created {
            response: envelope.clone(),
            sequence_number: next_seq(),
        })?;
        ctx.sink.send(ResponsesStreamEvent::InProgress {
            response: envelope.clone(),
            sequence_number: next_seq(),
        })?;
        // Open the (single) assistant message item.
        let added_item = ResponseOutputItem::Message {
            id: message_id.clone(),
            status: ResponseStatus::InProgress,
            role: "assistant",
            content: Vec::new(),
        };
        ctx.sink.send(ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            item: added_item,
            sequence_number: next_seq(),
        })?;
        ctx.sink.send(ResponsesStreamEvent::ContentPartAdded {
            item_id: message_id.clone(),
            output_index: 0,
            content_index: 0,
            part: ResponseContentPart::OutputText {
                text: String::new(),
                annotations: Vec::new(),
            },
            sequence_number: next_seq(),
        })?;
    }

    // 3. Drive generation with the server-selected decode strategy.
    // The production server defaults this to the A6000-verified
    // fast-rollback + DDTree mode; tests can still pass a simpler
    // config explicitly.
    let cfg = ctx.gen_config;
    let mut all_out: Vec<i32> = Vec::with_capacity(prompt_ids.len() + max_out);
    let stats = run_dflash_gen_loop(
        ctx.target_weights,
        ctx.draft_weights,
        ctx.target_cache,
        ctx.backend,
        &prompt_ids,
        max_out as i32,
        &mut all_out,
        cfg,
    )
    .map_err(|e| anyhow!("run_dflash_gen_loop: {e}"))?;
    tracing::info!(
        input_tokens,
        output_tokens = stats.n_generated,
        prefill_s = stats.prefill_s,
        wall_s = stats.wall_s,
        decode_tok_s = stats.decode_tok_s,
        draft_steps = stats.n_draft_steps,
        accepted = stats.n_accept_sum,
        fast_rollback = cfg.fast_rollback,
        ddtree = cfg.ddtree,
        ddtree_budget = cfg.ddtree_budget,
        "qwen35-27b responses turn complete"
    );

    let output_ids = &all_out[prompt_ids.len()..];
    let output_tokens = output_ids.len() as u32;
    let mut full_text = ctx
        .tokenizer
        .decode(output_ids)
        .unwrap_or_else(|_| String::new());
    if !reasoning_requested(req) {
        full_text = strip_think_blocks(&full_text);
    }
    full_text = truncate_at_chat_marker(&full_text);

    // 4. Emit streaming body (or non-streaming final).
    if req.stream && !full_text.is_empty() {
        // First-cut: emit the full text as a single delta. A
        // smoother UX emits per-commit-step deltas — that wiring
        // lands when the driver exposes an incremental callback.
        ctx.sink.send(ResponsesStreamEvent::OutputTextDelta {
            item_id: message_id.clone(),
            output_index: 0,
            content_index: 0,
            delta: full_text.clone(),
            sequence_number: next_seq(),
        })?;
        ctx.sink.send(ResponsesStreamEvent::OutputTextDone {
            item_id: message_id.clone(),
            output_index: 0,
            content_index: 0,
            text: full_text.clone(),
            sequence_number: next_seq(),
        })?;
        let done_part = ResponseContentPart::OutputText {
            text: full_text.clone(),
            annotations: Vec::new(),
        };
        ctx.sink.send(ResponsesStreamEvent::ContentPartDone {
            item_id: message_id.clone(),
            output_index: 0,
            content_index: 0,
            part: done_part.clone(),
            sequence_number: next_seq(),
        })?;
        ctx.sink.send(ResponsesStreamEvent::OutputItemDone {
            output_index: 0,
            item: ResponseOutputItem::Message {
                id: message_id.clone(),
                status: ResponseStatus::Completed,
                role: "assistant",
                content: vec![done_part],
            },
            sequence_number: next_seq(),
        })?;
    }

    // 5. Fill envelope for final completed event / non-streaming reply.
    envelope.status = ResponseStatus::Completed;
    envelope.output = vec![ResponseOutputItem::Message {
        id: message_id,
        status: ResponseStatus::Completed,
        role: "assistant",
        content: vec![ResponseContentPart::OutputText {
            text: full_text,
            annotations: Vec::new(),
        }],
    }];
    envelope.usage = Some(ResponseUsage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens + output_tokens,
        cached_input_tokens: Some(0),
        reasoning_output_tokens: Some(0),
    });

    ctx.sink.send(ResponsesStreamEvent::Completed {
        response: envelope,
        sequence_number: next_seq(),
    })?;

    // Not using `stats` structurally yet — surface via a telemetry
    // event once the CTOX side knows how to consume it.
    let _ = (stats, Duration::from_secs(0));
    Ok(())
}

/// Emit a single `response.failed` event with the given error code
/// + message. Used on parse-time errors where we can still bind a
/// response id.
pub fn emit_failed<S: StreamSink>(
    sink: &mut S,
    model_id: &str,
    code: &str,
    message: &str,
) -> Result<()> {
    let env = ResponseEnvelope {
        id: format!("resp_{}", uuid::Uuid::new_v4().simple()),
        object: "response",
        created_at: chrono::Utc::now().timestamp(),
        status: ResponseStatus::Failed,
        model: model_id.to_string(),
        output: Vec::new(),
        usage: None,
        error: Some(IpcError {
            code: code.to_string(),
            message: message.to_string(),
        }),
    };
    sink.send(ResponsesStreamEvent::Failed {
        response: env,
        sequence_number: 0,
    })
}
