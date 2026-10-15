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

#[derive(Debug, Clone, PartialEq)]
struct ParsedToolCall {
    name: String,
    arguments: Value,
    raw_tool_call: String,
}

/// Render a full prompt from the Responses request. Returns the
/// tokenizer-ready UTF-8 string.
fn render_chat_prompt(req: &ResponsesCreateRequest) -> Result<String> {
    let mut turns: Vec<ChatTurn> = Vec::new();

    // Qwen3.5's official tokenizer template puts tool schemas and the
    // operator's system instructions in one leading system turn.
    if !req.tools.is_empty() {
        let mut system =
            String::from("# Tools\n\nYou have access to the following functions:\n\n<tools>");
        for tool in &req.tools {
            system.push('\n');
            system.push_str(&serde_json::to_string(tool)?);
        }
        system.push_str(
            "\n</tools>\n\nIf you choose to call a function ONLY reply in the following format with NO suffix:\n\n<tool_call>\n<function=example_function_name>\n<parameter=example_parameter_1>\nvalue_1\n</parameter>\n<parameter=example_parameter_2>\nThis is the value for the second parameter\nthat can span\nmultiple lines\n</parameter>\n</function>\n</tool_call>\n\n<IMPORTANT>\nReminder:\n- Function calls MUST follow the specified format: an inner <function=...></function> block must be nested within <tool_call></tool_call> XML tags\n- Required parameters MUST be specified\n- You may provide optional reasoning for your function call in natural language BEFORE the function call, but NOT after\n- If work remains and a function can perform it, call that function in the same response; never end after only stating what you intend to do\n- Answer without a function call only when the requested task is complete or no available function can help\n</IMPORTANT>",
        );
        if !req.instructions.is_empty() {
            system.push_str("\n\n");
            system.push_str(&req.instructions);
        }
        if !req.tool_choice.is_empty() && req.tool_choice != "auto" {
            if req.tool_choice == "required" {
                system.push_str("\n\nYou must call one of the provided functions.");
            } else if let Some(name) = req.tool_choice.strip_prefix("required:") {
                system.push_str("\n\nYou must call the function named ");
                system.push_str(name);
                system.push_str(". Do not answer without calling it.");
            } else if req.tool_choice == "none" {
                system.push_str("\n\nDo not call a function; answer directly.");
            }
        }
        turns.push(ChatTurn {
            role: "system".into(),
            text: system,
        });
    } else if !req.instructions.is_empty() {
        turns.push(ChatTurn {
            role: "system".into(),
            text: req.instructions.clone(),
        });
    }

    // 2. Each `input` item → one turn (where it maps).
    for item in &req.input {
        if let Some(turn) = input_item_to_turn(item)? {
            if turn.role == "assistant" {
                if let Some(previous) = turns.last_mut().filter(|last| last.role == "assistant") {
                    let continuation = turn
                        .text
                        .strip_prefix("<think>\n\n</think>\n\n")
                        .unwrap_or(&turn.text);
                    if previous.text.ends_with("</tool_call>")
                        && continuation.starts_with("<tool_call>")
                    {
                        previous.text.push('\n');
                    }
                    previous.text.push_str(continuation);
                    continue;
                }
            }
            turns.push(turn);
        }
    }
    if !reasoning_requested(req) {
        // Apply the control token consistently to every ordinary user turn.
        // Mutating only the latest turn would rewrite an earlier turn when a
        // follow-up arrives, breaking exact-prefix KV/SSM reuse.
        for user_turn in turns
            .iter_mut()
            .filter(|turn| turn.role == "user" && !turn.text.starts_with("<tool_response>"))
        {
            if !user_turn.text.contains("/no_think") {
                if !user_turn.text.ends_with('\n') && !user_turn.text.is_empty() {
                    user_turn.text.push('\n');
                }
                user_turn.text.push_str("/no_think");
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
    out
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
    text[..cut].to_string()
}

/// Try to turn one Responses input item into a chat turn.
fn input_item_to_turn(item: &Value) -> Result<Option<ChatTurn>> {
    let obj = match item.as_object() {
        Some(o) => o,
        None => return Ok(None),
    };
    let ty = obj.get("type").and_then(Value::as_str).unwrap_or("message");
    if ty == "function_call" {
        let name = obj.get("name").and_then(Value::as_str).unwrap_or("");
        let arguments = obj.get("arguments").cloned().unwrap_or(Value::Null);
        let arguments = match arguments {
            Value::String(raw) => serde_json::from_str(&raw).unwrap_or(Value::String(raw)),
            value => value,
        };
        return Ok(Some(ChatTurn {
            role: "assistant".into(),
            // The no-think generation prompt already contained this empty
            // block before the model emitted the tool XML. Reproduce it in
            // history so the prior request's complete token sequence becomes
            // an exact prefix and its resident KV/SSM state can be reused.
            text: format!(
                "<think>\n\n</think>\n\n{}",
                obj.get("_ctox_raw_tool_call")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| render_tool_call(name, &arguments))
            ),
        }));
    }
    if ty == "function_call_output" {
        let output = obj.get("output").map(value_to_text).unwrap_or_default();
        return Ok(Some(ChatTurn {
            role: "user".into(),
            text: format!("<tool_response>\n{output}\n</tool_response>"),
        }));
    }
    if ty != "message" {
        return Ok(None);
    }
    let role = obj
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user")
        .to_string();

    // `content` can be either a string or an array of content parts.
    let mut text = match obj.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => flatten_content_parts(parts),
        _ => String::new(),
    };
    if role == "assistant" && !text.starts_with("<think>") {
        text = format!("<think>\n\n</think>\n\n{text}");
    }

    Ok(Some(ChatTurn { role, text }))
}

fn value_to_text(value: &Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn render_tool_call(name: &str, arguments: &Value) -> String {
    let mut out = format!("<tool_call>\n<function={name}>\n");
    if let Some(arguments) = arguments.as_object() {
        for (key, value) in arguments {
            out.push_str("<parameter=");
            out.push_str(key);
            out.push_str(">\n");
            out.push_str(&value_to_text(value));
            out.push_str("\n</parameter>\n");
        }
    }
    out.push_str("</function>\n</tool_call>");
    out
}

fn parse_tool_calls(text: &str) -> (String, Vec<ParsedToolCall>) {
    let mut calls = Vec::new();
    let mut visible = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<tool_call>") {
        visible.push_str(&rest[..start]);
        let after_open = &rest[start + "<tool_call>".len()..];
        let Some(end) = after_open.find("</tool_call>") else {
            visible.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let block = &after_open[..end];
        if let Some(mut call) = parse_tool_call_block(block) {
            call.raw_tool_call =
                rest[start..start + "<tool_call>".len() + end + "</tool_call>".len()].to_string();
            calls.push(call);
        } else {
            visible
                .push_str(&rest[start..start + "<tool_call>".len() + end + "</tool_call>".len()]);
        }
        rest = &after_open[end + "</tool_call>".len()..];
    }
    visible.push_str(rest);
    (visible, calls)
}

fn parse_tool_call_block(block: &str) -> Option<ParsedToolCall> {
    let block = block.trim();
    let function_start = block.find("<function=")? + "<function=".len();
    let name_end = block[function_start..].find('>')? + function_start;
    let name = block[function_start..name_end].trim();
    if name.is_empty() {
        return None;
    }
    let function_body = &block[name_end + 1..];
    let function_end = function_body
        .rfind("</function>")
        .unwrap_or(function_body.len());
    let mut parameters = serde_json::Map::new();
    let mut rest = &function_body[..function_end];
    while let Some(parameter_start) = rest.find("<parameter=") {
        let key_start = parameter_start + "<parameter=".len();
        let Some(key_len) = rest[key_start..].find('>') else {
            break;
        };
        let key_end = key_start + key_len;
        let key = rest[key_start..key_end].trim();
        let value_start = key_end + 1;
        let Some(value_len) = rest[value_start..].find("</parameter>") else {
            break;
        };
        let value_end = value_start + value_len;
        // Remove only the single formatting newline inserted by
        // `render_tool_call` on each side. `trim()` would corrupt legitimate
        // leading indentation or trailing newlines in Edit/Write payloads and
        // make the next prompt differ from the resident token prefix.
        let raw = &rest[value_start..value_end];
        let raw = raw.strip_prefix('\n').unwrap_or(raw);
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        let value = serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()));
        if !key.is_empty() {
            parameters.insert(key.to_string(), value);
        }
        rest = &rest[value_end + "</parameter>".len()..];
    }
    Some(ParsedToolCall {
        name: name.to_string(),
        arguments: Value::Object(parameters),
        raw_tool_call: String::new(),
    })
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
    /// Exact token sequence whose KV/SSM state is currently resident in
    /// `target_cache`. Reused only when the whole sequence is a prefix of the
    /// next rendered prompt, which makes append-only tool turns safe.
    pub cached_tokens: &'a mut Vec<i32>,
    pub model_id: &'a str,
    pub gen_config: GenConfig,
    pub sink: &'a mut S,
}

/// Default hard cap — keeps a runaway driver from producing 10 min
/// of output on a silly prompt.
pub const DEFAULT_MAX_OUTPUT_TOKENS: usize = 32_768;

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
    let common_prefix_tokens = ctx
        .cached_tokens
        .iter()
        .zip(prompt_ids.iter())
        .take_while(|(cached, prompt)| cached == prompt)
        .count();
    let previous_cached_tokens = ctx.cached_tokens.len();
    let cached_input_tokens = if previous_cached_tokens > 0
        && previous_cached_tokens < prompt_ids.len()
        && common_prefix_tokens == previous_cached_tokens
    {
        ctx.cached_tokens.len()
    } else {
        0
    };
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
    }

    // 3. Drive generation with the server-selected decode strategy.
    // The production server defaults this to the A6000-verified
    // fast-rollback + DDTree mode; tests can still pass a simpler
    // config explicitly.
    let cfg = ctx.gen_config;
    let mut all_out: Vec<i32> = Vec::with_capacity(prompt_ids.len() + max_out);
    // SAFETY: the server owns the live backend and keeps its weights/cache
    // borrowed for this complete generation call.
    let stats = unsafe {
        run_dflash_gen_loop(
            ctx.target_weights,
            ctx.draft_weights,
            ctx.target_cache,
            ctx.backend,
            &prompt_ids,
            max_out as i32,
            &mut all_out,
            cfg,
            cached_input_tokens,
        )
    }
    .map_err(|e| anyhow!("run_dflash_gen_loop: {e}"))?;
    tracing::info!(
        input_tokens,
        output_tokens = stats.n_generated,
        prefill_s = stats.prefill_s,
        wall_s = stats.wall_s,
        decode_tok_s = stats.decode_tok_s,
        draft_steps = stats.n_draft_steps,
        accepted = stats.n_accept_sum,
        cached_input_tokens,
        common_prefix_tokens,
        previous_cached_tokens,
        fast_rollback = cfg.fast_rollback,
        ddtree = cfg.ddtree,
        ddtree_budget = cfg.ddtree_budget,
        "qwen35-27b responses turn complete"
    );
    *ctx.cached_tokens = all_out.clone();

    let output_ids = &all_out[prompt_ids.len()..];
    let output_tokens = output_ids.len() as u32;
    let mut generated_text = ctx
        .tokenizer
        .decode(output_ids)
        .unwrap_or_else(|_| String::new());
    if !reasoning_requested(req) {
        generated_text = strip_think_blocks(&generated_text);
    }
    generated_text = truncate_at_chat_marker(&generated_text);
    let (full_text, tool_calls) = if req.tools.is_empty() {
        (generated_text, Vec::new())
    } else {
        parse_tool_calls(&generated_text)
    };

    // 4. Emit streaming output items. The model can return visible text,
    // one or more structured function calls, or both.
    let mut output_items = Vec::new();
    let mut output_index = 0u32;
    if req.stream && !full_text.is_empty() {
        let added_item = ResponseOutputItem::Message {
            id: message_id.clone(),
            status: ResponseStatus::InProgress,
            role: "assistant",
            content: Vec::new(),
        };
        ctx.sink.send(ResponsesStreamEvent::OutputItemAdded {
            output_index,
            item: added_item,
            sequence_number: next_seq(),
        })?;
        ctx.sink.send(ResponsesStreamEvent::ContentPartAdded {
            item_id: message_id.clone(),
            output_index,
            content_index: 0,
            part: ResponseContentPart::OutputText {
                text: String::new(),
                annotations: Vec::new(),
            },
            sequence_number: next_seq(),
        })?;
        ctx.sink.send(ResponsesStreamEvent::OutputTextDelta {
            item_id: message_id.clone(),
            output_index,
            content_index: 0,
            delta: full_text.clone(),
            sequence_number: next_seq(),
        })?;
        ctx.sink.send(ResponsesStreamEvent::OutputTextDone {
            item_id: message_id.clone(),
            output_index,
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
            output_index,
            content_index: 0,
            part: done_part.clone(),
            sequence_number: next_seq(),
        })?;
        ctx.sink.send(ResponsesStreamEvent::OutputItemDone {
            output_index,
            item: ResponseOutputItem::Message {
                id: message_id.clone(),
                status: ResponseStatus::Completed,
                role: "assistant",
                content: vec![done_part],
            },
            sequence_number: next_seq(),
        })?;
    }
    if !full_text.is_empty() {
        output_items.push(ResponseOutputItem::Message {
            id: message_id,
            status: ResponseStatus::Completed,
            role: "assistant",
            content: vec![ResponseContentPart::OutputText {
                text: full_text,
                annotations: Vec::new(),
            }],
        });
        output_index += 1;
    }

    for call in tool_calls {
        let item_id = format!("fc_{}", uuid::Uuid::new_v4().simple());
        let call_id = format!("call_{}", uuid::Uuid::new_v4().simple());
        let arguments = serde_json::to_string(&call.arguments)?;
        if req.stream {
            ctx.sink.send(ResponsesStreamEvent::OutputItemAdded {
                output_index,
                item: ResponseOutputItem::FunctionCall {
                    id: item_id.clone(),
                    call_id: call_id.clone(),
                    name: call.name.clone(),
                    arguments: String::new(),
                    _ctox_raw_tool_call: None,
                    status: ResponseStatus::InProgress,
                },
                sequence_number: next_seq(),
            })?;
            ctx.sink
                .send(ResponsesStreamEvent::FunctionCallArgumentsDelta {
                    item_id: item_id.clone(),
                    output_index,
                    delta: arguments.clone(),
                    sequence_number: next_seq(),
                })?;
            ctx.sink
                .send(ResponsesStreamEvent::FunctionCallArgumentsDone {
                    item_id: item_id.clone(),
                    output_index,
                    arguments: arguments.clone(),
                    sequence_number: next_seq(),
                })?;
            ctx.sink.send(ResponsesStreamEvent::OutputItemDone {
                output_index,
                item: ResponseOutputItem::FunctionCall {
                    id: item_id.clone(),
                    call_id: call_id.clone(),
                    name: call.name.clone(),
                    arguments: arguments.clone(),
                    _ctox_raw_tool_call: Some(call.raw_tool_call.clone()),
                    status: ResponseStatus::Completed,
                },
                sequence_number: next_seq(),
            })?;
        }
        output_items.push(ResponseOutputItem::FunctionCall {
            id: item_id,
            call_id,
            name: call.name,
            arguments,
            _ctox_raw_tool_call: Some(call.raw_tool_call),
            status: ResponseStatus::Completed,
        });
        output_index += 1;
    }
    let _ = output_index;

    // 5. Fill envelope for final completed event / non-streaming reply.
    envelope.status = ResponseStatus::Completed;
    envelope.output = output_items;
    envelope.usage = Some(ResponseUsage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens + output_tokens,
        cached_input_tokens: Some(cached_input_tokens as u32),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn request(input: Vec<Value>, tools: Vec<Value>) -> ResponsesCreateRequest {
        ResponsesCreateRequest {
            model: "qwen35-27b-q4km-dflash".into(),
            instructions: "Be exact.".into(),
            input,
            tools,
            tool_choice: "auto".into(),
            parallel_tool_calls: true,
            reasoning: None,
            max_output_tokens: Some(64),
            store: false,
            stream: true,
            include: Vec::new(),
            service_tier: None,
            prompt_cache_key: None,
            text: None,
        }
    }

    #[test]
    fn renders_qwen35_tools_and_multiturn_results() {
        let req = request(
            vec![
                serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type":"input_text", "text":"Read Cargo.toml"}]
                }),
                serde_json::json!({
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"Cargo.toml\"}"
                }),
                serde_json::json!({
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "[package]"
                }),
            ],
            vec![serde_json::json!({
                "type":"function", "name":"read_file",
                "description":"Read a file",
                "parameters":{"type":"object","properties":{"path":{"type":"string"}}}
            })],
        );
        let prompt = render_chat_prompt(&req).unwrap();
        assert!(prompt.contains("# Tools\n\nYou have access"));
        assert!(prompt.contains("<function=read_file>"));
        assert!(prompt.contains("<parameter=path>\nCargo.toml\n</parameter>"));
        assert!(prompt.contains("<tool_response>\n[package]\n</tool_response>"));
        assert!(prompt.ends_with("<|im_start|>assistant\n<think>\n\n</think>\n\n"));
    }

    #[test]
    fn parses_parallel_qwen35_xml_tool_calls() {
        let generated = concat!(
            "<tool_call>\n<function=read_file>\n",
            "<parameter=path>\nsrc/main.rs\n</parameter>\n",
            "</function>\n</tool_call>\n",
            "<tool_call>\n<function=search>\n",
            "<parameter=query>\n{\"pattern\":\"TODO\"}\n</parameter>\n",
            "</function>\n</tool_call>"
        );
        let (text, calls) = parse_tool_calls(generated);
        assert!(text.trim().is_empty());
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].arguments["path"], "src/main.rs");
        assert_eq!(calls[1].name, "search");
        assert_eq!(calls[1].arguments["query"]["pattern"], "TODO");
    }

    #[test]
    fn preserves_generated_parameter_order_for_exact_prefix_reuse() {
        let generated = concat!(
            "<tool_call>\n<function=Edit>\n",
            "<parameter=replace_all>\nfalse\n</parameter>\n",
            "<parameter=file_path>\n/tmp/example.py\n</parameter>\n",
            "<parameter=old_string>\nold\n</parameter>\n",
            "<parameter=new_string>\nnew\n</parameter>\n",
            "</function>\n</tool_call>"
        );
        let (text, calls) = parse_tool_calls(generated);
        assert!(text.is_empty());
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].raw_tool_call, generated);
        assert_eq!(
            render_tool_call(&calls[0].name, &calls[0].arguments),
            generated
        );
    }

    #[test]
    fn preserves_parameter_whitespace_for_exact_prefix_reuse() {
        let generated = concat!(
            "<tool_call>\n<function=Write>\n",
            "<parameter=content>\n",
            "  indented first line\ntrailing blank line\n\n",
            "</parameter>\n</function>\n</tool_call>"
        );
        let (_, calls) = parse_tool_calls(generated);
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].arguments["content"],
            "  indented first line\ntrailing blank line\n"
        );
        assert_eq!(
            render_tool_call(&calls[0].name, &calls[0].arguments),
            generated
        );
    }

    #[test]
    fn preserves_visible_whitespace_for_exact_prefix_reuse() {
        assert_eq!(strip_think_blocks("  answer  "), "  answer  ");
        assert_eq!(
            truncate_at_chat_marker("  answer  <|im_end|>ignored"),
            "  answer  "
        );
    }

    #[test]
    fn keeps_no_think_control_stable_across_followup_users() {
        let req = request(
            vec![
                serde_json::json!({"type":"message","role":"user","content":"first"}),
                serde_json::json!({"type":"message","role":"assistant","content":"answer"}),
                serde_json::json!({"type":"message","role":"user","content":"second"}),
            ],
            Vec::new(),
        );
        let prompt = render_chat_prompt(&req).unwrap();
        assert!(prompt.contains("first\n/no_think<|im_end|>"));
        assert!(prompt.contains("second\n/no_think<|im_end|>"));
    }

    #[test]
    fn merges_text_and_function_call_from_one_assistant_turn() {
        let req = request(
            vec![
                serde_json::json!({"type":"message","role":"user","content":"inspect"}),
                serde_json::json!({"type":"message","role":"assistant","content":"I will inspect it.\n"}),
                serde_json::json!({"type":"function_call","name":"read_file","arguments":"{\"path\":\"Cargo.toml\"}"}),
                serde_json::json!({"type":"function_call_output","output":"[package]"}),
            ],
            vec![serde_json::json!({
                "type":"function", "name":"read_file",
                "parameters":{"type":"object"}
            })],
        );
        let prompt = render_chat_prompt(&req).unwrap();
        assert!(prompt.contains(
            "<|im_start|>assistant\n<think>\n\n</think>\n\nI will inspect it.\n<tool_call>"
        ));
        assert!(!prompt.contains("I will inspect it.\n<|im_end|>\n<|im_start|>assistant"));
    }
}
