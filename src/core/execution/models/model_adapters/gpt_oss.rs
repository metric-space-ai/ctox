use crate::inference::engine;
use crate::inference::runtime_state;
use crate::inference::turn_contract;
use anyhow::Context;
use regex::Regex;
use serde_json::json;
use serde_json::Value;
use std::path::PathBuf;

use super::ResponsesTransportKind;

const DEFAULT_HARMONY_REASONING_CAP: &str = "low";
const DEFAULT_RUNTIME_OUTPUT_BUDGET: usize = 131_072;
const DEFAULT_HARMONY_MIN_OUTPUT_TOKENS: usize = 128;
const HARMONY_STOP_MARKERS: &[&str] = &["<|return|>", "<|call|>"];
const LOCAL_COMPACT_INSTRUCTIONS: &str = "You are Codex running through CTOX on a local responses-backed runtime. Be concise and tool-accurate. Emit either one tool call or one final answer per turn. Prefer exec_command for shell work and apply_patch for file edits. Do not restate instructions. If the task requires creating or modifying files, running builds or tests, or proving a result inside a workspace, your next completion must be a tool call, not a final answer. You must not claim success, emit an exact marker, or give a final answer until tool output has verified the required result. When the user asks for an exact marker or short final answer, return only that required text after any needed tool calls and verification.";

pub fn adapter_id() -> &'static str {
    "gpt_oss"
}

pub fn transport_kind() -> ResponsesTransportKind {
    ResponsesTransportKind::CompletionTemplate
}

pub fn upstream_path() -> &'static str {
    "/v1/completions"
}

pub fn compact_instructions() -> &'static str {
    LOCAL_COMPACT_INSTRUCTIONS
}

pub fn reasoning_effort_override() -> Option<&'static str> {
    Some("low")
}

pub fn unified_exec_enabled() -> bool {
    true
}

pub fn uses_ctox_web_stack() -> bool {
    true
}

pub fn compact_limit(model: &str, realized_context: usize) -> usize {
    match model {
        "openai/gpt-oss-120b" => 1_280.min(realized_context.saturating_sub(512)).max(1_024),
        _ => ((realized_context as f64) * 3.0 / 4.0).round() as usize,
    }
}

pub fn runtime_tuning(
    preset: crate::inference::runtime_plan::ChatPreset,
    max_output_tokens: u32,
) -> crate::inference::runtime_state::AdapterRuntimeTuning {
    crate::inference::runtime_state::AdapterRuntimeTuning {
        reasoning_cap: Some(
            match preset {
                crate::inference::runtime_plan::ChatPreset::Quality => "high",
                crate::inference::runtime_plan::ChatPreset::Performance => "low",
            }
            .to_string(),
        ),
        max_output_tokens_cap: Some(max_output_tokens),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HarmonyToolSpec {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) parameters: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
struct HarmonyProxyRequest {
    model: String,
    system_prompt: String,
    conversation_items: Vec<Value>,
    reasoning_effort: String,
    max_output_tokens: usize,
    stream: bool,
    tools: Vec<HarmonyToolSpec>,
    tool_payloads: Vec<Value>,
    tool_choice: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
struct HarmonyFunctionCall {
    call_id: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Clone, PartialEq)]
enum HarmonyResponseItem {
    Message(String),
    FunctionCall(HarmonyFunctionCall),
}

pub fn rewrite_request(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse responses request")?;
    let request = parse_proxy_request(raw)?;
    let completion_payload = serde_json::json!({
        "model": request.model,
        "prompt": build_prompt(
            &request.system_prompt,
            &request.conversation_items,
            &request.reasoning_effort,
            &request.tools
        ),
        "max_tokens": request.max_output_tokens,
        "stop": stop_value(payload.get("stop")),
        "temperature": 0.0,
        "stream": false,
        "tools": request.tool_payloads,
        "tool_choice": request.tool_choice.unwrap_or_else(|| json!("auto"))
    });
    serde_json::to_vec(&completion_payload).context("failed to encode GPT-OSS completion payload")
}

pub(crate) fn rewrite_chat_request(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse responses request")?;
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("openai/gpt-oss-120b")
        .to_string();
    let instructions = payload
        .get("instructions")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let messages = build_chat_messages(
        &engine::normalize_responses_input(payload.get("input")),
        instructions.as_deref(),
    );
    let mut merged_system_parts = Vec::new();
    let mut merged_messages = Vec::new();
    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let content = message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if role == "system" {
            if !content.is_empty() {
                merged_system_parts.push(content);
            }
        } else {
            merged_messages.push(message);
        }
    }
    let mut messages = Vec::new();
    if !merged_system_parts.is_empty() {
        messages.push(json!({
            "role": "system",
            "content": merged_system_parts.join("\n\n"),
        }));
    }
    messages.extend(merged_messages);

    let exact_text_override =
        engine::extract_exact_text_override_from_materialized_request(&payload);
    let tools = if exact_text_override.is_some() {
        Vec::new()
    } else {
        payload
            .get("tools")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .flat_map(|tool| rewrite_chat_tool(tool.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    let requested_reasoning_effort = payload
        .get("reasoning")
        .and_then(|value| value.get("effort"))
        .and_then(Value::as_str);
    let requested_max_output_tokens = payload
        .get("max_output_tokens")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or_else(|| default_output_budget(&model));
    let reasoning_effort =
        effective_reasoning_effort(&model, requested_reasoning_effort.unwrap_or("low"));
    let enable_thinking = thinking_enabled(&reasoning_effort);
    let mut max_tokens = effective_max_output_tokens(&model, requested_max_output_tokens);
    if enable_thinking {
        max_tokens = ensure_thinking_output_floor(&model, max_tokens);
    }

    let mut request = serde_json::Map::new();
    request.insert("model".to_string(), Value::String(model));
    request.insert("messages".to_string(), Value::Array(messages));
    if !tools.is_empty() {
        request.insert("tools".to_string(), Value::Array(tools));
    }
    request.insert("enable_thinking".to_string(), Value::Bool(enable_thinking));
    if enable_thinking {
        request.insert(
            "reasoning_effort".to_string(),
            Value::String(reasoning_effort),
        );
    }
    request.insert(
        "max_tokens".to_string(),
        Value::Number(serde_json::Number::from(max_tokens as u64)),
    );
    for key in [
        "temperature",
        "top_p",
        "presence_penalty",
        "frequency_penalty",
    ] {
        if let Some(value) = payload.get(key) {
            request.insert(key.to_string(), value.clone());
        }
    }
    if exact_text_override.is_none() {
        if let Some(value) = payload.get("tool_choice") {
            request.insert("tool_choice".to_string(), value.clone());
        }
    }
    request.insert("stream".to_string(), Value::Bool(false));
    if payload.get("parallel_tool_calls") == Some(&Value::Bool(false)) {
        request.insert("parallel_tool_calls".to_string(), Value::Bool(true));
    } else if let Some(value) = payload.get("parallel_tool_calls") {
        request.insert("parallel_tool_calls".to_string(), value.clone());
    }

    serde_json::to_vec(&Value::Object(request))
        .context("failed to encode GPT-OSS chat-completions payload")
}

pub fn rewrite_success_response(
    raw: &[u8],
    fallback_model: Option<&str>,
    exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse completion response")?;
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .or(fallback_model)
        .unwrap_or("openai/gpt-oss-120b")
        .to_string();
    let response_id = payload
        .get("id")
        .and_then(Value::as_str)
        .map(|value| format!("resp_{value}"))
        .unwrap_or_else(|| "resp_ctox_gateway".to_string());
    let created_at = payload
        .get("created")
        .and_then(Value::as_u64)
        .unwrap_or_else(engine::current_unix_ts);
    let raw_text = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let items = parse_response_items(raw_text);
    let exact_text_override = exact_text_override
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned);
    let (output, output_text) = if let Some(exact_text) = exact_text_override {
        (
            vec![turn_output_item_from_harmony_item(
                HarmonyResponseItem::Message(exact_text.clone()),
            )],
            Some(exact_text),
        )
    } else {
        let output_text = items.iter().find_map(|item| match item {
            HarmonyResponseItem::Message(text) if !text.trim().is_empty() => Some(text.clone()),
            _ => None,
        });
        let output = items
            .into_iter()
            .map(turn_output_item_from_harmony_item)
            .collect::<Vec<_>>();
        (output, output_text)
    };
    let response_payload = turn_contract::TurnResponse::completed(
        response_id,
        model,
        created_at,
        engine::current_unix_ts(),
        output,
        output_text,
        None,
        turn_contract::TurnUsage::from_usage_payload(payload.get("usage")),
    );
    serde_json::to_vec(&response_payload).context("failed to encode responses-compatible payload")
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn rewrite_success_response_to_sse(
    raw: &[u8],
    fallback_model: Option<&str>,
    exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    let json_payload = rewrite_success_response(raw, fallback_model, exact_text_override)?;
    engine::rewrite_responses_payload_to_sse(&json_payload)
}

pub(crate) fn rewrite_chat_success_response(
    raw: &[u8],
    fallback_model: Option<&str>,
    exact_text_override: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse GPT-OSS chat completion response")?;
    let mut builder =
        engine::responses_turn_builder(&payload, fallback_model, "openai/gpt-oss-120b");
    if let Some(choices) = payload.get("choices").and_then(Value::as_array) {
        for choice in choices {
            let message = choice.get("message").and_then(Value::as_object);
            let mut choice_reasoning = Vec::new();
            // OpenRouter returns reasoning in `reasoning_content` or `reasoning`
            // depending on the provider.
            if let Some(reasoning) = message
                .and_then(|msg| {
                    msg.get("reasoning_content")
                        .or_else(|| msg.get("reasoning"))
                })
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                choice_reasoning.push(reasoning.to_string());
            }
            if exact_text_override.is_none() {
                if let Some(text) = message
                    .and_then(|msg| msg.get("content"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .filter(|text| !text.is_empty())
                {
                    let (inline_reasoning, visible_text) = split_chat_reasoning_and_content(&text);
                    if let Some(inline_reasoning) = inline_reasoning
                        .map(|text| text.trim().to_string())
                        .filter(|text| !text.is_empty())
                    {
                        if !choice_reasoning
                            .iter()
                            .any(|existing| existing == &inline_reasoning)
                        {
                            choice_reasoning.push(inline_reasoning);
                        }
                    }
                    let (plain_text, xml_tool_calls) = parse_chat_tool_calls(&visible_text);
                    if let Some(plain_text) = plain_text.filter(|text| !text.is_empty()) {
                        builder.push_message_text(plain_text);
                    }
                    for tool_call in xml_tool_calls {
                        builder.push_function_call(
                            tool_call.call_id,
                            tool_call.name,
                            tool_call.arguments,
                        );
                    }
                }
                if let Some(tool_calls) = message
                    .and_then(|msg| msg.get("tool_calls"))
                    .and_then(Value::as_array)
                {
                    for tool_call in tool_calls {
                        let function = tool_call.get("function").unwrap_or(tool_call);
                        let name = function
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let arguments = function
                            .get("arguments")
                            .cloned()
                            .unwrap_or_else(|| json!({}));
                        let call_id = tool_call
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("call_ctox_gateway");
                        builder.push_function_call(
                            call_id,
                            name,
                            if arguments.is_string() {
                                arguments.as_str().unwrap_or("{}").to_string()
                            } else {
                                arguments.to_string()
                            },
                        );
                    }
                }
            }
            for reasoning in choice_reasoning {
                builder.push_reasoning(reasoning);
            }
        }
    }

    if let Some(exact_text) = exact_text_override.map(str::to_string) {
        builder.replace_output_with_message(exact_text);
    }
    let response_payload = builder.build();
    serde_json::to_vec(&response_payload).context("failed to encode GPT-OSS chat responses payload")
}

pub fn build_followup_request(
    initial_request_raw: &[u8],
    first_completion_raw: &[u8],
) -> anyhow::Result<Option<Vec<u8>>> {
    if !completion_needs_followup(first_completion_raw)? {
        return Ok(None);
    }

    let mut request: Value = serde_json::from_slice(initial_request_raw)
        .context("failed to parse initial completion request")?;
    let first_payload: Value = serde_json::from_slice(first_completion_raw)
        .context("failed to parse first completion response")?;
    let first_text = first_payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let prompt = request
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    request["prompt"] = Value::String(format!("{prompt}{first_text}<|end|><|return|>"));
    Ok(Some(serde_json::to_vec(&request)?))
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn should_use_proxy(raw: &[u8]) -> anyhow::Result<bool> {
    Ok(parse_proxy_request(raw)
        .map(|request| is_model_id(&request.model))
        .unwrap_or(false))
}

pub(crate) fn completion_needs_followup(raw: &[u8]) -> anyhow::Result<bool> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse completion response")?;
    let raw_text = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let items = parse_response_items(raw_text);
    Ok(items.is_empty() && raw_text.contains("<|channel|>analysis<|message|>"))
}

pub(crate) fn thinking_enabled(reasoning_effort: &str) -> bool {
    sanitize_reasoning_effort(reasoning_effort) != "none"
}

pub(crate) fn effective_reasoning_effort(model_id: &str, requested: &str) -> String {
    if !is_model_id(model_id) {
        return sanitize_reasoning_effort(requested).to_string();
    }
    let cap = harmony_reasoning_cap();
    cap_reasoning_effort(requested, Some(cap.as_str()))
}

pub(crate) fn effective_max_output_tokens(model_id: &str, requested: usize) -> usize {
    if !is_model_id(model_id) {
        return requested;
    }
    cap_max_output_tokens(requested, harmony_max_output_tokens_cap())
}

pub(crate) fn ensure_thinking_output_floor(model_id: &str, requested: usize) -> usize {
    if !is_model_id(model_id) {
        return requested;
    }
    requested.max(DEFAULT_HARMONY_MIN_OUTPUT_TOKENS)
}

pub(crate) fn default_output_budget(model_id: &str) -> usize {
    if !is_model_id(model_id) {
        return DEFAULT_HARMONY_MIN_OUTPUT_TOKENS;
    }
    current_runtime_state()
        .and_then(|state| state.realized_context_tokens.map(|value| value as usize))
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_RUNTIME_OUTPUT_BUDGET)
}

pub(crate) fn build_prompt(
    system_prompt: &str,
    conversation_items: &[Value],
    reasoning_effort: &str,
    tools: &[HarmonyToolSpec],
) -> String {
    let current_date = "2026-03-22";
    let reasoning_effort = sanitize_reasoning_effort(reasoning_effort);
    let developer_block = build_developer_block(system_prompt, tools);
    let system_tool_hint = if tools.is_empty() {
        ""
    } else {
        "\nCalls to these tools must go to the commentary channel: 'functions'."
    };
    let assistant_prefix = "<|start|>assistant";
    let conversation = render_conversation(conversation_items);
    format!(
        "<|start|>system<|message|>You are ChatGPT, a large language model trained by OpenAI.\n\
Knowledge cutoff: 2024-06\n\
Current date: {current_date}\n\n\
Reasoning: {reasoning_effort}\n\n\
# Valid channels: analysis, commentary, final. Channel must be included for every message.{system_tool_hint}<|end|>\
<|start|>developer<|message|>{developer_block}<|end|>\
{conversation}\
{assistant_prefix}",
        current_date = current_date,
        reasoning_effort = reasoning_effort,
        system_tool_hint = system_tool_hint,
        developer_block = developer_block,
        conversation = conversation,
    )
}

fn stop_value(existing: Option<&Value>) -> Value {
    let mut sequences = match existing {
        Some(Value::String(sequence)) => vec![sequence.clone()],
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    for marker in HARMONY_STOP_MARKERS {
        if !sequences.iter().any(|candidate| candidate == marker) {
            sequences.push((*marker).to_string());
        }
    }

    Value::Array(sequences.into_iter().map(Value::String).collect())
}

fn parse_proxy_request(raw: &[u8]) -> anyhow::Result<HarmonyProxyRequest> {
    let payload: Value =
        serde_json::from_slice(raw).context("failed to parse responses request")?;
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("openai/gpt-oss-120b")
        .to_string();
    let system_prompt = payload
        .get("instructions")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let conversation_items = engine::normalize_responses_input(payload.get("input"));
    let requested_reasoning_effort = payload
        .get("reasoning")
        .and_then(|value| value.get("effort"))
        .and_then(Value::as_str)
        .unwrap_or("medium");
    let exact_text_override =
        engine::extract_exact_text_override_from_materialized_request(&payload);
    let reasoning_effort = effective_reasoning_effort(&model, requested_reasoning_effort);
    let requested_max_output_tokens = payload
        .get("max_output_tokens")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or_else(|| default_output_budget(&model));
    let capped_max_output_tokens = effective_max_output_tokens(&model, requested_max_output_tokens);
    let max_output_tokens = if thinking_enabled(&reasoning_effort) {
        ensure_thinking_output_floor(&model, capped_max_output_tokens)
    } else {
        capped_max_output_tokens
    };
    let stream = payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let tool_payloads = if exact_text_override.is_some() {
        Vec::new()
    } else {
        payload
            .get("tools")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|tool| engine::rewrite_tool(tool.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let tools = tool_payloads
        .iter()
        .filter_map(parse_tool_spec)
        .collect::<Vec<_>>();
    let tool_choice = if exact_text_override.is_some() {
        None
    } else {
        payload.get("tool_choice").cloned()
    };
    Ok(HarmonyProxyRequest {
        model,
        system_prompt,
        conversation_items,
        reasoning_effort,
        max_output_tokens,
        stream,
        tools,
        tool_payloads,
        tool_choice,
    })
}

fn parse_tool_spec(tool: &Value) -> Option<HarmonyToolSpec> {
    let tool_type = tool.get("type").and_then(Value::as_str)?;
    if tool_type != "function" {
        return None;
    }
    let function = tool.get("function").unwrap_or(tool);
    let name = function.get("name").and_then(Value::as_str)?.to_string();
    Some(HarmonyToolSpec {
        name,
        description: function
            .get("description")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        parameters: function.get("parameters").cloned(),
    })
}

fn is_model_id(model_id: &str) -> bool {
    let lowered = model_id.trim().to_ascii_lowercase();
    lowered == "gpt-oss-120b" || lowered == "openai/gpt-oss-120b" || lowered.contains("gpt-oss")
}

fn sanitize_reasoning_effort(value: &str) -> &str {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => "none",
        "minimal" | "low" => "low",
        "medium" => "medium",
        "high" => "high",
        _ => "medium",
    }
}

fn reasoning_effort_rank(value: &str) -> u8 {
    match sanitize_reasoning_effort(value) {
        "none" => 0,
        "low" => 1,
        "medium" => 2,
        "high" => 3,
        _ => 2,
    }
}

fn cap_reasoning_effort(requested: &str, cap: Option<&str>) -> String {
    let requested = sanitize_reasoning_effort(requested);
    let Some(cap) = cap.map(sanitize_reasoning_effort) else {
        return requested.to_string();
    };
    if reasoning_effort_rank(requested) <= reasoning_effort_rank(cap) {
        requested.to_string()
    } else {
        cap.to_string()
    }
}

fn cap_max_output_tokens(requested: usize, cap: Option<usize>) -> usize {
    let Some(cap) = cap.filter(|value| *value > 0) else {
        return requested;
    };
    requested.min(cap)
}

fn current_runtime_root() -> PathBuf {
    std::env::current_dir()
        .ok()
        .filter(|path| crate::persistence::sqlite_path(path).exists())
        .or_else(|| std::env::var("CTOX_ROOT").ok().map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn current_runtime_state() -> Option<runtime_state::InferenceRuntimeState> {
    runtime_state::load_or_resolve_runtime_state(&current_runtime_root()).ok()
}

pub(crate) fn harmony_reasoning_cap() -> String {
    current_runtime_state()
        .and_then(|state| state.adapter_tuning.reasoning_cap)
        .filter(|value| !value.trim().is_empty())
        .map(|value| sanitize_reasoning_effort(&value).to_string())
        .unwrap_or_else(|| DEFAULT_HARMONY_REASONING_CAP.to_string())
}

pub(crate) fn harmony_max_output_tokens_cap() -> Option<usize> {
    current_runtime_state()
        .and_then(|state| {
            state
                .adapter_tuning
                .max_output_tokens_cap
                .map(|value| value as usize)
        })
        .filter(|value| *value > 0)
}

fn build_developer_block(system_prompt: &str, tools: &[HarmonyToolSpec]) -> String {
    let mut block = String::from("# Instructions\n\n");
    let trimmed = system_prompt.trim();
    if !trimmed.is_empty() {
        block.push_str(trimmed);
        block.push_str("\n\n");
    }
    block.push_str("# Response Contract\n\n");
    block.push_str("Emit exactly one next step per completion.\n");
    block.push_str("Answer on the final channel unless you are emitting one tool call.\n");
    block.push_str("Do not emit analysis or commentary as plain text in the final answer.\n");
    block.push_str(
        "If the user asks for exact text, an exact marker, or nothing else, return exactly that text and nothing else.\n",
    );
    block.push('\n');
    if !tools.is_empty() {
        block.push_str("# Tools\n\n");
        block.push_str("Either emit one tool call on the commentary channel or one final answer on the final channel.\n");
        block.push_str("Do not emit multiple tool calls in a single completion.\n");
        block.push_str("After emitting a tool call, stop immediately.\n");
        block.push_str(
            "Use only the provided function tool definitions from the request metadata.\n",
        );
        block.push_str("Available tools: ");
        block.push_str(
            &tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        );
        block.push_str("\n\n");
        block.push_str("namespace functions {\n");
        for tool in tools {
            block.push_str(&render_tool_signature(tool));
        }
        block.push_str("}\n");
    }
    block.trim_end().to_string()
}

fn render_conversation(conversation_items: &[Value]) -> String {
    if conversation_items.is_empty() {
        return "<|start|>user<|message|><|end|>".to_string();
    }

    let mut rendered = String::new();
    for item in conversation_items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let item_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("message");
        match item_type {
            "message" => {
                let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
                let text = engine::extract_message_content_text(object.get("content"));
                if text.trim().is_empty() {
                    continue;
                }
                match role {
                    "assistant" => {
                        rendered.push_str("<|start|>assistant<|channel|>final<|message|>");
                        rendered.push_str(text.trim());
                        rendered.push_str("<|end|>");
                    }
                    "tool" => {
                        rendered.push_str("<|start|>tool<|message|>");
                        rendered.push_str(text.trim());
                        rendered.push_str("<|end|>");
                    }
                    _ => {
                        rendered.push_str("<|start|>user<|message|>");
                        rendered.push_str(text.trim());
                        rendered.push_str("<|end|>");
                    }
                }
            }
            "function_call" => {
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let arguments = object
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                if name.is_empty() {
                    continue;
                }
                rendered.push_str("<|start|>assistant to=functions.");
                rendered.push_str(name);
                rendered.push_str("<|channel|>commentary json<|message|>");
                rendered.push_str(arguments.trim());
                rendered.push_str("<|call|>");
            }
            "function_call_output" => {
                let text = engine::extract_function_call_output_text(object.get("output"));
                if text.trim().is_empty() {
                    continue;
                }
                rendered.push_str("<|start|>tool<|message|>");
                rendered.push_str(text.trim());
                rendered.push_str("<|end|>");
            }
            _ => {}
        }
    }

    if rendered.is_empty() {
        "<|start|>user<|message|><|end|>".to_string()
    } else {
        rendered
    }
}

fn json_schema_to_typescript(schema: &Value) -> String {
    if schema.get("type").and_then(Value::as_str) == Some("object")
        && schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|properties| properties.is_empty())
            .unwrap_or(true)
    {
        return "(_: {}) => any".to_string();
    }

    let object = json_schema_object_to_typescript(schema);
    if object.trim().is_empty() {
        "() => any".to_string()
    } else {
        format!("(_: {object}) => any")
    }
}

fn json_schema_object_to_typescript(schema: &Value) -> String {
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let props = schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .map(|(key, value)| {
                    let optional = if required.iter().any(|item| item == key) {
                        ""
                    } else {
                        "?"
                    };
                    let mut line = String::new();
                    if let Some(description) = value.get("description").and_then(Value::as_str) {
                        line.push_str("// ");
                        line.push_str(description.trim());
                        line.push('\n');
                    }
                    line.push_str(&format!(
                        "{key}{optional}: {}",
                        json_schema_type_to_typescript(value)
                    ));
                    if let Some(default) = value.get("default") {
                        line.push_str(&format!(", // default: {}", default));
                    }
                    line
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    if props.trim().is_empty() {
        "{}".to_string()
    } else {
        format!("{{\n{props}\n}}")
    }
}

fn json_schema_type_to_typescript(schema: &Value) -> String {
    match schema.get("enum").and_then(Value::as_array) {
        Some(items) if !items.is_empty() => items
            .iter()
            .filter_map(|item| match item {
                Value::String(text) => Some(format!("\"{text}\"")),
                Value::Number(number) => Some(number.to_string()),
                Value::Bool(flag) => Some(flag.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" | "),
        _ => match schema.get("type").and_then(Value::as_str) {
            Some("string") => "string".to_string(),
            Some("integer") | Some("number") => "number".to_string(),
            Some("boolean") => "boolean".to_string(),
            Some("array") => {
                let item_ty = schema
                    .get("items")
                    .map(json_schema_type_to_typescript)
                    .unwrap_or_else(|| "any".to_string());
                format!("{item_ty}[]")
            }
            Some("object") => schema
                .get("properties")
                .map(|_| json_schema_object_to_typescript(schema))
                .unwrap_or_else(|| "object".to_string()),
            _ => "any".to_string(),
        },
    }
}

fn render_tool_signature(tool: &HarmonyToolSpec) -> String {
    let mut rendered = String::new();
    if let Some(description) = &tool.description {
        rendered.push_str("// ");
        rendered.push_str(description.trim());
        rendered.push('\n');
    }
    rendered.push_str("type ");
    rendered.push_str(&tool.name);
    rendered.push_str(" = ");
    rendered.push_str(
        &tool
            .parameters
            .as_ref()
            .map(json_schema_to_typescript)
            .unwrap_or_else(|| "() => any".to_string()),
    );
    rendered.push_str(";\n");
    rendered
}

#[cfg_attr(not(test), allow(dead_code))]
fn build_chat_messages(items: &[Value], instructions: Option<&str>) -> Vec<Value> {
    let mut messages = Vec::new();
    if let Some(instructions) = instructions {
        messages.push(json!({
            "role": "system",
            "content": instructions,
        }));
    }

    let mut pending_assistant: Option<serde_json::Map<String, Value>> = None;
    let flush_pending_assistant =
        |pending_assistant: &mut Option<serde_json::Map<String, Value>>,
         messages: &mut Vec<Value>| {
            if let Some(assistant) = pending_assistant.take() {
                messages.push(Value::Object(assistant));
            }
        };

    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let item_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("message");
        match item_type {
            "message" => {
                let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
                let mapped_role = match role {
                    "developer" => "system",
                    other => other,
                };
                let text = engine::extract_message_content_text(object.get("content"));
                if mapped_role == "assistant" {
                    flush_pending_assistant(&mut pending_assistant, &mut messages);
                    let mut assistant = serde_json::Map::new();
                    assistant.insert("role".to_string(), Value::String("assistant".to_string()));
                    let (reasoning, content) = split_chat_reasoning_and_content(&text);
                    assistant.insert("content".to_string(), Value::String(content));
                    if let Some(reasoning) = reasoning {
                        assistant.insert("reasoning_content".to_string(), Value::String(reasoning));
                    }
                    pending_assistant = Some(assistant);
                } else {
                    flush_pending_assistant(&mut pending_assistant, &mut messages);
                    messages.push(json!({
                        "role": mapped_role,
                        "content": text,
                    }));
                }
            }
            "function_call" => {
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let arguments = object
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                let assistant = pending_assistant.get_or_insert_with(|| {
                    let mut assistant = serde_json::Map::new();
                    assistant.insert("role".to_string(), Value::String("assistant".to_string()));
                    assistant.insert("content".to_string(), Value::String(String::new()));
                    assistant
                });
                let existing = assistant
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let rendered = render_chat_tool_call(name, arguments);
                let combined = if existing.trim().is_empty() {
                    rendered
                } else {
                    format!("{existing}{rendered}")
                };
                assistant.insert("content".to_string(), Value::String(combined));
            }
            "function_call_output" => {
                flush_pending_assistant(&mut pending_assistant, &mut messages);
                let call_id = object
                    .get("call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_ctox_gateway");
                let output = engine::extract_function_call_output_text(object.get("output"));
                messages.push(json!({
                    "role": "user",
                    "content": format!("<tool_response>\n{}\n</tool_response>", output.trim_end()),
                    "tool_call_id": call_id,
                }));
            }
            _ => {}
        }
    }

    flush_pending_assistant(&mut pending_assistant, &mut messages);
    messages
}

#[cfg_attr(not(test), allow(dead_code))]
fn rewrite_chat_tool(tool: Value) -> Vec<Value> {
    let Some(object) = tool.as_object() else {
        return Vec::new();
    };
    let Some(tool_type) = object.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };
    match tool_type {
        "function" => {
            let function = object
                .get("function")
                .and_then(Value::as_object)
                .unwrap_or(object);
            let Some(name) = function.get("name").and_then(Value::as_str) else {
                return Vec::new();
            };
            if name == "apply_patch" {
                return Vec::new();
            }
            engine::rewrite_tool(Value::Object(object.clone()))
                .into_iter()
                .collect()
        }
        "namespace" => object
            .get("tools")
            .and_then(Value::as_array)
            .map(|children| {
                children
                    .iter()
                    .flat_map(|child| rewrite_chat_tool(child.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn render_chat_tool_call(name: &str, raw_arguments: &str) -> String {
    let arguments = serde_json::from_str::<Value>(raw_arguments)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let mut rendered = String::new();
    rendered.push_str("<tool_call>\n");
    rendered.push_str("<function=");
    rendered.push_str(name.trim());
    rendered.push_str(">\n");
    for (key, value) in arguments {
        rendered.push_str("<parameter=");
        rendered.push_str(key.trim());
        rendered.push('>');
        let value_text = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| value.to_string());
        rendered.push_str(&value_text);
        rendered.push_str("</parameter>\n");
    }
    rendered.push_str("</function>\n</tool_call>\n");
    rendered
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChatXmlToolCall {
    call_id: String,
    name: String,
    arguments: String,
}

#[cfg_attr(not(test), allow(dead_code))]
fn split_chat_reasoning_and_content(text: &str) -> (Option<String>, String) {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("<think>") {
        if let Some((reasoning, content)) = rest.split_once("</think>") {
            let reasoning = reasoning.trim().to_string();
            let content = content.trim().to_string();
            return (
                if reasoning.is_empty() {
                    None
                } else {
                    Some(reasoning)
                },
                content,
            );
        }
    }
    (None, trimmed.to_string())
}

#[cfg_attr(not(test), allow(dead_code))]
fn parse_chat_tool_calls(text: &str) -> (Option<String>, Vec<ChatXmlToolCall>) {
    let tool_call_re = Regex::new(
        r"(?s)<tool_call>\s*<function=([A-Za-z0-9_-]+)>\s*(.*?)\s*</function>\s*</tool_call>",
    )
    .expect("valid GPT-OSS chat tool call regex");
    let param_re = Regex::new(r"(?s)<parameter=([A-Za-z0-9_-]+)>\s*(.*?)\s*</parameter>")
        .expect("valid GPT-OSS chat parameter regex");
    let mut tool_calls = Vec::new();
    let mut plain_text = String::new();
    let mut last_end = 0usize;
    for (index, captures) in tool_call_re.captures_iter(text).enumerate() {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        plain_text.push_str(&text[last_end..matched.start()]);
        last_end = matched.end();
        let name = captures
            .get(1)
            .map(|capture| capture.as_str().trim().to_string())
            .unwrap_or_default();
        let body = captures
            .get(2)
            .map(|capture| capture.as_str())
            .unwrap_or_default();
        let mut arguments = serde_json::Map::new();
        for param in param_re.captures_iter(body) {
            let Some(param_name) = param.get(1).map(|capture| capture.as_str().trim()) else {
                continue;
            };
            let value = param
                .get(2)
                .map(|capture| capture.as_str().trim().to_string())
                .unwrap_or_default();
            arguments.insert(param_name.to_string(), Value::String(value));
        }
        tool_calls.push(ChatXmlToolCall {
            call_id: format!("call_ctox_gateway_{index}"),
            name,
            arguments: Value::Object(arguments).to_string(),
        });
    }
    plain_text.push_str(&text[last_end..]);
    let plain_text = plain_text.trim().to_string();
    (
        if plain_text.is_empty() {
            None
        } else {
            Some(plain_text)
        },
        tool_calls,
    )
}

fn sanitize_completion_text(raw: &str) -> String {
    let mut text = raw.trim().to_string();
    if let Some(idx) = text.rfind("<|message|>") {
        text = text[idx + "<|message|>".len()..].to_string();
    }
    if let Some(idx) = text.find("<|return|>") {
        text.truncate(idx);
    }
    if let Some(idx) = text.find("<|end|>") {
        text.truncate(idx);
    }
    if let Some(idx) = text.find("<|start|>") {
        text.truncate(idx);
    }
    sanitize_channel_leakage(&text)
}

fn parse_response_items(raw_text: &str) -> Vec<HarmonyResponseItem> {
    let mut items = Vec::new();
    if let Some(call) = parse_function_call(raw_text) {
        items.push(HarmonyResponseItem::FunctionCall(call));
        return items;
    }
    let text = extract_message_text(raw_text);
    if !text.trim().is_empty() {
        items.push(HarmonyResponseItem::Message(text));
    }
    items
}

fn parse_function_call(raw_text: &str) -> Option<HarmonyFunctionCall> {
    let message_token = "<|message|>";
    let message_start = raw_text.find(message_token)?;
    let header = &raw_text[..message_start];
    if !header.contains("<|channel|>commentary") {
        return None;
    }

    let recipient_idx = header.find("to=")?;
    let recipient_start = recipient_idx + "to=".len();
    let recipient_end = header[recipient_start..]
        .find(|c: char| c == '<' || c.is_whitespace())
        .map(|offset| recipient_start + offset)
        .unwrap_or(header.len());
    let name = header[recipient_start..recipient_end]
        .trim()
        .strip_prefix("functions.")
        .unwrap_or_else(|| header[recipient_start..recipient_end].trim());
    if name.is_empty() {
        return None;
    }
    let message_start = message_start + message_token.len();
    let message_end = raw_text[message_start..]
        .find("<|call|>")
        .map(|offset| message_start + offset)
        .or_else(|| {
            raw_text[message_start..]
                .find("<|end|>")
                .map(|offset| message_start + offset)
        })
        .unwrap_or(raw_text.len());
    let arguments = raw_text[message_start..message_end].trim();
    if arguments.is_empty() {
        return None;
    }
    Some(HarmonyFunctionCall {
        call_id: format!("call_ctox_{}", engine::current_unix_ts()),
        name: name.to_string(),
        arguments: normalize_function_call_arguments(name, arguments),
    })
}

pub(crate) fn normalize_function_call_arguments(name: &str, arguments: &str) -> String {
    let mut value = match serde_json::from_str::<Value>(arguments) {
        Ok(value) => value,
        Err(_) => match name {
            "exec_command" => parse_relaxed_exec_command_arguments(arguments)
                .unwrap_or_else(|| Value::String(arguments.to_string())),
            "write_stdin" => parse_relaxed_write_stdin_arguments(arguments)
                .unwrap_or_else(|| Value::String(arguments.to_string())),
            _ => Value::String(arguments.to_string()),
        },
    };

    if name == "exec_command" {
        if let Some(object) = value.as_object_mut() {
            if let Some(cmd_items) = object.get("cmd").and_then(Value::as_array) {
                if let Some(rewritten) = normalize_exec_command_array(cmd_items) {
                    for (key, rewritten_value) in rewritten {
                        object.insert(key, rewritten_value);
                    }
                } else {
                    let joined = cmd_items
                        .iter()
                        .map(|item| {
                            item.as_str()
                                .map(shell_escape)
                                .unwrap_or_else(|| shell_escape(&item.to_string()))
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    object.insert("cmd".to_string(), Value::String(joined));
                }
            }
        }
    }

    match value {
        Value::Object(_) => serde_json::to_string(&value).unwrap_or_else(|_| arguments.to_string()),
        Value::String(text) => {
            serde_json::to_string(&json!({ "cmd": text })).unwrap_or_else(|_| arguments.to_string())
        }
        _ => serde_json::to_string(&value).unwrap_or_else(|_| arguments.to_string()),
    }
}

fn shell_escape(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn normalize_exec_command_array(cmd_items: &[Value]) -> Option<Vec<(String, Value)>> {
    let first = cmd_items.first()?.as_str()?;
    if matches!(first, "bash" | "/bin/bash" | "sh" | "/bin/sh")
        && cmd_items.get(1).and_then(Value::as_str) == Some("-lc")
    {
        if let Some(script) = cmd_items.get(2).and_then(Value::as_str) {
            return Some(vec![
                ("cmd".to_string(), Value::String(script.to_string())),
                (
                    "shell".to_string(),
                    Value::String(if first.contains("bash") { "bash" } else { "sh" }.to_string()),
                ),
                ("login".to_string(), Value::Bool(false)),
            ]);
        }
    }
    if first == "apply_patch" {
        if let Some(patch) = cmd_items.get(1).and_then(Value::as_str) {
            return Some(vec![(
                "cmd".to_string(),
                Value::String(format!("apply_patch <<'PATCH'\n{patch}\nPATCH")),
            )]);
        }
    }
    None
}

fn parse_relaxed_exec_command_arguments(arguments: &str) -> Option<Value> {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return None;
    }

    if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
        return Some(json!({ "cmd": trimmed }));
    }

    let cmd = extract_relaxed_json_string_field(trimmed, "cmd")
        .or_else(|| extract_relaxed_json_array_field(trimmed, "cmd").map(Value::Array))?;

    let mut object = serde_json::Map::new();
    object.insert("cmd".to_string(), cmd);

    for key in [
        "workdir",
        "shell",
        "justification",
        "sandbox_permissions",
        "prefix_rule",
    ] {
        if let Some(value) = extract_relaxed_json_string_field(trimmed, key) {
            object.insert(key.to_string(), value);
        } else if let Some(value) = extract_relaxed_json_array_field(trimmed, key) {
            object.insert(key.to_string(), Value::Array(value));
        }
    }

    for key in ["yield_time_ms", "max_output_tokens"] {
        if let Some(value) = extract_relaxed_json_integer_field(trimmed, key) {
            object.insert(key.to_string(), Value::Number(value.into()));
        }
    }

    for key in ["login", "tty"] {
        if let Some(value) = extract_relaxed_json_bool_field(trimmed, key) {
            object.insert(key.to_string(), Value::Bool(value));
        }
    }

    Some(Value::Object(object))
}

fn parse_relaxed_write_stdin_arguments(arguments: &str) -> Option<Value> {
    let trimmed = arguments.trim();
    if trimmed.is_empty() || !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
        return None;
    }

    let session_id = extract_relaxed_json_integer_field(trimmed, "session_id")?;
    let mut object = serde_json::Map::new();
    object.insert("session_id".to_string(), Value::Number(session_id.into()));

    if let Some(value) = extract_relaxed_json_string_field(trimmed, "chars") {
        object.insert("chars".to_string(), value);
    }
    if let Some(value) = extract_relaxed_json_integer_field(trimmed, "yield_time_ms") {
        object.insert("yield_time_ms".to_string(), Value::Number(value.into()));
    }
    if let Some(value) = extract_relaxed_json_integer_field(trimmed, "max_output_tokens") {
        object.insert("max_output_tokens".to_string(), Value::Number(value.into()));
    }

    Some(Value::Object(object))
}

fn extract_relaxed_json_string_field(source: &str, key: &str) -> Option<Value> {
    let key_re = Regex::new(&format!(r#""{}"\s*:"#, regex::escape(key))).ok()?;
    let key_match = key_re.find(source)?;
    let remainder = &source[key_match.end()..];
    let value = remainder.trim_start();
    if !value.starts_with('"') {
        return None;
    }
    let value = &value[1..];
    let value_end = find_relaxed_string_end(value);
    let content = value[..value_end].trim_end_matches('"');
    Some(Value::String(unescape_relaxed_string(content)))
}

fn extract_relaxed_json_array_field(source: &str, key: &str) -> Option<Vec<Value>> {
    let key_re = Regex::new(&format!(r#""{}"\s*:"#, regex::escape(key))).ok()?;
    let key_match = key_re.find(source)?;
    let remainder = &source[key_match.end()..];
    let value = remainder.trim_start();
    if !value.starts_with('[') {
        return None;
    }
    let end = find_matching_bracket(value, '[', ']')?;
    let array_text = &value[..=end];
    serde_json::from_str::<Vec<Value>>(array_text).ok()
}

fn extract_relaxed_json_integer_field(source: &str, key: &str) -> Option<i64> {
    let re = Regex::new(&format!(r#""{}"\s*:\s*(-?\d+)"#, regex::escape(key))).ok()?;
    re.captures(source)?.get(1)?.as_str().parse::<i64>().ok()
}

fn extract_relaxed_json_bool_field(source: &str, key: &str) -> Option<bool> {
    let re = Regex::new(&format!(r#""{}"\s*:\s*(true|false)"#, regex::escape(key))).ok()?;
    Some(re.captures(source)?.get(1)?.as_str() == "true")
}

fn find_relaxed_string_end(source: &str) -> usize {
    let mut escaped = false;
    let bytes = source.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        let ch = bytes[idx] as char;
        if escaped {
            escaped = false;
            idx += 1;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            idx += 1;
            continue;
        }
        if ch == '"' {
            let tail = source[idx + 1..].trim_start();
            if tail.starts_with(',') || tail.starts_with('}') {
                return idx;
            }
        }
        idx += 1;
    }
    source.len()
}

fn find_matching_bracket(source: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, ch) in source.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn unescape_relaxed_string(text: &str) -> String {
    serde_json::from_str::<String>(&format!(
        "\"{}\"",
        text.replace('\\', "\\\\").replace('"', "\\\"")
    ))
    .unwrap_or_else(|_| text.to_string())
}

fn extract_message_text(raw_text: &str) -> String {
    let final_token = "<|channel|>final<|message|>";
    let commentary_token = "<|channel|>commentary<|message|>";
    for token in [final_token, commentary_token] {
        let matches = raw_text.match_indices(token).collect::<Vec<_>>();
        for (start, _) in matches.into_iter().rev() {
            let content_start = start + token.len();
            let content_end = raw_text[content_start..]
                .find("<|end|>")
                .map(|offset| content_start + offset)
                .unwrap_or(raw_text.len());
            let text = sanitize_completion_text(&raw_text[content_start..content_end]);
            if !text.trim().is_empty() {
                return text;
            }
        }
    }
    if let Some(text) = extract_plaintext_final(raw_text) {
        return text;
    }
    sanitize_completion_text(raw_text)
}

fn extract_plaintext_final(raw_text: &str) -> Option<String> {
    for marker in ["assistantfinal", "final"] {
        let matches = raw_text.match_indices(marker).collect::<Vec<_>>();
        for (idx, _) in matches.into_iter().rev() {
            if marker == "final" && idx > 0 {
                let preceding = raw_text[..idx].chars().last();
                if preceding
                    .map(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/'))
                    .unwrap_or(false)
                {
                    continue;
                }
            }
            let payload_start = idx + marker.len();
            let remainder = &raw_text[payload_start..];
            let next_marker = [
                "assistantanalysis",
                "assistantcommentary",
                "assistantfinal",
                "<|start|>",
                "<|end|>",
                "<|return|>",
            ]
            .iter()
            .filter_map(|candidate| remainder.find(candidate))
            .min()
            .unwrap_or(remainder.len());
            let candidate = sanitize_completion_text(&remainder[..next_marker]);
            if !candidate.trim().is_empty() {
                return Some(candidate);
            }
        }
    }
    None
}

fn sanitize_channel_leakage(raw: &str) -> String {
    let mut text = raw.trim().to_string();
    let saw_plaintext_harmony = contains_plaintext_marker(&text);

    loop {
        let mut stripped_any = false;
        for prefix in [
            "assistantfinal",
            "final",
            "assistantcommentary",
            "commentary",
            "assistantanalysis",
            "analysis",
        ] {
            if let Some(stripped) = text.strip_prefix(prefix) {
                text = stripped.trim_start().to_string();
                stripped_any = true;
            }
        }
        if !stripped_any {
            break;
        }
    }

    if let Some(idx) = find_plaintext_marker(&text) {
        text.truncate(idx);
    }

    if saw_plaintext_harmony
        || text.ends_with("assistant")
        || text.ends_with("analysis")
        || text.ends_with("commentary")
        || text.ends_with("final")
    {
        text = trim_trailing_incomplete_token(&text).to_string();
    }

    text.trim().to_string()
}

fn find_plaintext_marker(text: &str) -> Option<usize> {
    let markers = [
        "assistantanalysis",
        "assistantfinal",
        "assistantcommentary",
        "analysis",
        "final",
        "commentary",
    ];
    markers
        .iter()
        .filter_map(|marker| text.find(marker).map(|idx| (idx, *marker)))
        .filter(|(idx, marker)| {
            if *idx == 0 {
                return false;
            }
            let preceding = text[..*idx].chars().last();
            let following = text[idx + marker.len()..].chars().next();
            let preceding_is_payload = preceding
                .map(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ')' | ']' | '"' | '\''))
                .unwrap_or(false);
            let following_looks_like_channel = following
                .map(|ch| ch.is_ascii_uppercase() || ch == '<' || ch == '{' || ch == '[')
                .unwrap_or(false);
            preceding_is_payload && following_looks_like_channel
        })
        .map(|(idx, _)| idx)
        .min()
}

fn contains_plaintext_marker(text: &str) -> bool {
    [
        "assistantanalysis",
        "assistantfinal",
        "assistantcommentary",
        "analysis",
        "final",
        "commentary",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn trim_trailing_incomplete_token(text: &str) -> &str {
    text.strip_suffix("assistant")
        .or_else(|| text.strip_suffix("analysis"))
        .or_else(|| text.strip_suffix("commentary"))
        .or_else(|| text.strip_suffix("final"))
        .unwrap_or(text)
}

fn turn_output_item_from_harmony_item(item: HarmonyResponseItem) -> turn_contract::TurnOutputItem {
    match item {
        HarmonyResponseItem::Message(text) => {
            turn_contract::TurnOutputItem::assistant_message(text)
        }
        HarmonyResponseItem::FunctionCall(call) => {
            turn_contract::TurnOutputItem::function_call(call.call_id, call.name, call.arguments)
        }
    }
}
