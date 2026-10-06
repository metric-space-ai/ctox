// ref: internal/translator/openai/openai/responses/openai_openai-responses_response.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Translates an OpenAI Chat Completions response (streaming SSE chunks
//! or a single non-stream JSON body) into the OpenAI Responses event
//! stream used by CTOX clients. The streaming state is kept
//! request-local through the [`TranslationState`] slot — no
//! process-wide counters, atomic ids, or wall-clock fallbacks.

use crate::internal::translator::common::sse_event_data;
use crate::sdk::translator::{TranslationContext, TranslationState};
use serde_json::{json, Value};
use std::collections::{hash_map::DefaultHasher, HashMap, HashSet};
use std::hash::{Hash, Hasher};

use super::tools;
pub(super) use tools::{
    apply_responses_function_call_namespace_fields, pick_request_json, responses_custom_tool_names,
    responses_single_custom_tool_name, unwrap_custom_tool_input,
};

#[derive(Default)]
struct OaiToResponsesState {
    response_id: String,
    created: i64,
    started: bool,
    completed_emitted: bool,
    finish_reason: String,
    sequence_number: i64,
    msg_text_buf: HashMap<i64, String>,
    reasonings: Vec<OaiToResponsesStateReasoning>,
    func_args_buf: HashMap<String, String>,
    func_names: HashMap<String, String>,
    func_call_ids: HashMap<String, String>,
    func_output_ix: HashMap<String, i64>,
    func_args_sent: HashMap<String, i64>,
    msg_output_ix: HashMap<i64, i64>,
    next_output_ix: i64,
    msg_item_added: HashMap<i64, bool>,
    msg_content_added: HashMap<i64, bool>,
    msg_item_done: HashMap<i64, bool>,
    func_item_added: HashMap<String, bool>,
    func_item_custom: HashMap<String, bool>,
    func_args_done: HashMap<String, bool>,
    func_item_done: HashMap<String, bool>,
    custom_tool_names: HashSet<String>,
    prompt_tokens: i64,
    cached_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    reasoning_tokens: i64,
    usage_seen: bool,
    reasoning_id_field: String,
    reasoning_index_field: i64,
    reasoning_text_buffer: String,
}

#[derive(Clone)]
struct OaiToResponsesStateReasoning {
    reasoning_id: String,
    reasoning_data: String,
    output_index: i64,
}

pub fn convert_openai_chat_completions_response_to_openai_responses(
    context: &TranslationContext,
    _model_name: &str,
    original_request_raw_json: &[u8],
    request_raw_json: &[u8],
    raw_json: &[u8],
    state: &mut TranslationState,
) -> Vec<Vec<u8>> {
    if context.is_cancelled() {
        return Vec::new();
    }
    let st = oai_to_responses_state(state);
    let payload_owned;
    let payload = match strip_data_prefix(raw_json) {
        Some(stripped) => trimmed_payload(stripped),
        None => {
            payload_owned = trimmed_payload(raw_json);
            payload_owned
        }
    };
    if payload.is_empty() {
        return Vec::new();
    }
    let request_for_namespace_owned;
    let request_for_namespace: &[u8] =
        match pick_request_json(original_request_raw_json, request_raw_json) {
            Some(value) => value,
            None => {
                request_for_namespace_owned = Vec::new();
                &request_for_namespace_owned
            }
        };
    if payload == b"[DONE]" {
        if st.started && !st.completed_emitted {
            st.completed_emitted = true;
            let mut counter = st.sequence_number;
            let mut next_seq = || -> i64 {
                counter += 1;
                counter
            };
            let event = build_responses_completed_event(st, request_for_namespace, &mut next_seq);
            st.sequence_number = counter;
            return vec![event];
        }
        return Vec::new();
    }

    let root: Value = match serde_json::from_slice(payload) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    if let Some(obj) = root.get("object").and_then(Value::as_str) {
        if !obj.is_empty() && obj != "chat.completion.chunk" {
            return Vec::new();
        }
    }
    if !root
        .get("choices")
        .map(|value| value.is_array())
        .unwrap_or(false)
    {
        return Vec::new();
    }

    if let Some(usage) = root.get("usage") {
        if let Some(prompt) = usage.get("prompt_tokens").and_then(Value::as_i64) {
            st.prompt_tokens = prompt;
            st.usage_seen = true;
        }
        if let Some(cached) = usage
            .get("prompt_tokens_details")
            .and_then(|v| v.get("cached_tokens"))
            .and_then(Value::as_i64)
        {
            st.cached_tokens = cached;
            st.usage_seen = true;
        }
        if let Some(completion) = usage.get("completion_tokens").and_then(Value::as_i64) {
            st.completion_tokens = completion;
            st.usage_seen = true;
        } else if let Some(completion) = usage.get("output_tokens").and_then(Value::as_i64) {
            st.completion_tokens = completion;
            st.usage_seen = true;
        }
        if let Some(reasoning) = usage
            .get("output_tokens_details")
            .and_then(|v| v.get("reasoning_tokens"))
            .and_then(Value::as_i64)
        {
            st.reasoning_tokens = reasoning;
            st.usage_seen = true;
        } else if let Some(reasoning) = usage
            .get("completion_tokens_details")
            .and_then(|v| v.get("reasoning_tokens"))
            .and_then(Value::as_i64)
        {
            st.reasoning_tokens = reasoning;
            st.usage_seen = true;
        }
        if let Some(total) = usage.get("total_tokens").and_then(Value::as_i64) {
            st.total_tokens = total;
            st.usage_seen = true;
        }
    }

    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut counter = st.sequence_number;
    let next_seq: &mut dyn FnMut() -> i64 = &mut || {
        counter += 1;
        counter
    };

    if !st.started {
        st.response_id = root
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default();
        st.created = root.get("created").and_then(Value::as_i64).unwrap_or(0);
        st.msg_text_buf.clear();
        st.reasonings.clear();
        st.func_args_buf.clear();
        st.func_names.clear();
        st.func_call_ids.clear();
        st.func_output_ix.clear();
        st.func_args_sent.clear();
        st.msg_output_ix.clear();
        st.next_output_ix = 0;
        st.msg_item_added.clear();
        st.msg_content_added.clear();
        st.msg_item_done.clear();
        st.func_item_added.clear();
        st.func_item_custom.clear();
        st.func_args_done.clear();
        st.func_item_done.clear();
        st.custom_tool_names = responses_custom_tool_names(request_for_namespace);
        st.prompt_tokens = 0;
        st.cached_tokens = 0;
        st.completion_tokens = 0;
        st.total_tokens = 0;
        st.reasoning_tokens = 0;
        st.usage_seen = false;
        st.completed_emitted = false;
        st.finish_reason.clear();
        st.reasoning_id_field.clear();
        st.reasoning_index_field = 0;
        st.reasoning_text_buffer.clear();

        let mut created = json!({
            "type":"response.created",
            "sequence_number":0,
            "response":{
                "id":"",
                "object":"response",
                "created_at":0,
                "status":"in_progress",
                "background":false,
                "error":null,
                "output":[]
            }
        });
        created["sequence_number"] = Value::Number(next_seq().into());
        created["response"]["id"] = Value::String(st.response_id.clone());
        created["response"]["created_at"] = Value::Number(st.created.into());
        let bytes = serde_json::to_vec(&created).unwrap_or_default();
        out.push(sse_event_data("response.created", &bytes));

        let mut inprog = json!({
            "type":"response.in_progress",
            "sequence_number":0,
            "response":{"id":"","object":"response","created_at":0,"status":"in_progress"}
        });
        inprog["sequence_number"] = Value::Number(next_seq().into());
        inprog["response"]["id"] = Value::String(st.response_id.clone());
        inprog["response"]["created_at"] = Value::Number(st.created.into());
        let bytes = serde_json::to_vec(&inprog).unwrap_or_default();
        out.push(sse_event_data("response.in_progress", &bytes));
        st.started = true;
    }

    if let Some(choices) = root.get("choices").and_then(Value::as_array) {
        for choice in choices {
            let idx = choice.get("index").and_then(Value::as_i64).unwrap_or(0);
            if let Some(delta) = choice.get("delta") {
                if let Some(content) = delta.get("content") {
                    if let Some(text) = content.as_str() {
                        if !text.is_empty() {
                            if !st.reasoning_id_field.is_empty() {
                                let buffer = std::mem::take(&mut st.reasoning_text_buffer);
                                emit_reasoning_done(st, &buffer, &mut out, next_seq);
                            }
                            let msg_output_index = match st.msg_output_ix.get(&idx) {
                                Some(value) => *value,
                                None => {
                                    let value = alloc_output_index(st);
                                    st.msg_output_ix.insert(idx, value);
                                    value
                                }
                            };
                            if !st.msg_item_added.get(&idx).copied().unwrap_or(false) {
                                let mut item = json!({
                                    "type":"response.output_item.added",
                                    "sequence_number":0,
                                    "output_index":0,
                                    "item":{
                                        "id":"",
                                        "type":"message",
                                        "status":"in_progress",
                                        "content":[],
                                        "role":"assistant"
                                    }
                                });
                                item["sequence_number"] = Value::Number(next_seq().into());
                                item["output_index"] = Value::Number(msg_output_index.into());
                                item["item"]["id"] = Value::String(msg_item_id(st, idx));
                                let bytes = serde_json::to_vec(&item).unwrap_or_default();
                                out.push(sse_event_data("response.output_item.added", &bytes));
                                st.msg_item_added.insert(idx, true);
                            }
                            if !st.msg_content_added.get(&idx).copied().unwrap_or(false) {
                                let mut part = json!({
                                    "type":"response.content_part.added",
                                    "sequence_number":0,
                                    "item_id":"",
                                    "output_index":0,
                                    "content_index":0,
                                    "part":{
                                        "type":"output_text",
                                        "annotations":[],
                                        "logprobs":[],
                                        "text":""
                                    }
                                });
                                part["sequence_number"] = Value::Number(next_seq().into());
                                part["item_id"] = Value::String(msg_item_id(st, idx));
                                part["output_index"] = Value::Number(msg_output_index.into());
                                part["content_index"] = Value::Number(0.into());
                                let bytes = serde_json::to_vec(&part).unwrap_or_default();
                                out.push(sse_event_data("response.content_part.added", &bytes));
                                st.msg_content_added.insert(idx, true);
                            }

                            let mut msg = json!({
                                "type":"response.output_text.delta",
                                "sequence_number":0,
                                "item_id":"",
                                "output_index":0,
                                "content_index":0,
                                "delta":"",
                                "logprobs":[]
                            });
                            msg["sequence_number"] = Value::Number(next_seq().into());
                            msg["item_id"] = Value::String(msg_item_id(st, idx));
                            msg["output_index"] = Value::Number(msg_output_index.into());
                            msg["content_index"] = Value::Number(0.into());
                            msg["delta"] = Value::String(text.to_string());
                            let bytes = serde_json::to_vec(&msg).unwrap_or_default();
                            out.push(sse_event_data("response.output_text.delta", &bytes));
                            let entry = st.msg_text_buf.entry(idx).or_default();
                            entry.push_str(text);
                        }
                    }
                }

                let rc = if delta.get("reasoning_content").is_some() {
                    delta.get("reasoning_content")
                } else if delta.get("reasoning").is_some() {
                    delta.get("reasoning")
                } else {
                    None
                };
                if let Some(rc) = rc {
                    if let Some(text) = rc.as_str() {
                        if !text.is_empty() {
                            if st.reasoning_id_field.is_empty() {
                                st.reasoning_id_field = format!("rs_{}_{}", st.response_id, idx);
                                st.reasoning_index_field = alloc_output_index(st);
                                let mut item = json!({
                                    "type":"response.output_item.added",
                                    "sequence_number":0,
                                    "output_index":0,
                                    "item":{"id":"","type":"reasoning","status":"in_progress","summary":[]}
                                });
                                item["sequence_number"] = Value::Number(next_seq().into());
                                item["output_index"] =
                                    Value::Number(st.reasoning_index_field.into());
                                item["item"]["id"] = Value::String(st.reasoning_id_field.clone());
                                let bytes = serde_json::to_vec(&item).unwrap_or_default();
                                out.push(sse_event_data("response.output_item.added", &bytes));
                                let mut part = json!({
                                    "type":"response.reasoning_summary_part.added",
                                    "sequence_number":0,
                                    "item_id":"",
                                    "output_index":0,
                                    "summary_index":0,
                                    "part":{"type":"summary_text","text":""}
                                });
                                part["sequence_number"] = Value::Number(next_seq().into());
                                part["item_id"] = Value::String(st.reasoning_id_field.clone());
                                part["output_index"] =
                                    Value::Number(st.reasoning_index_field.into());
                                let bytes = serde_json::to_vec(&part).unwrap_or_default();
                                out.push(sse_event_data(
                                    "response.reasoning_summary_part.added",
                                    &bytes,
                                ));
                            }
                            st.reasoning_text_buffer.push_str(text);
                            let mut delta_event = json!({
                                "type":"response.reasoning_summary_text.delta",
                                "sequence_number":0,
                                "item_id":"",
                                "output_index":0,
                                "summary_index":0,
                                "delta":""
                            });
                            delta_event["sequence_number"] = Value::Number(next_seq().into());
                            delta_event["item_id"] = Value::String(st.reasoning_id_field.clone());
                            delta_event["output_index"] =
                                Value::Number(st.reasoning_index_field.into());
                            delta_event["delta"] = Value::String(text.to_string());
                            let bytes = serde_json::to_vec(&delta_event).unwrap_or_default();
                            out.push(sse_event_data(
                                "response.reasoning_summary_text.delta",
                                &bytes,
                            ));
                        }
                    }
                }

                if let Some(tcs) = delta.get("tool_calls").and_then(Value::as_array) {
                    if !st.reasoning_id_field.is_empty() {
                        let buffer = std::mem::take(&mut st.reasoning_text_buffer);
                        emit_reasoning_done(st, &buffer, &mut out, next_seq);
                    }
                    if st.msg_item_added.get(&idx).copied().unwrap_or(false)
                        && !st.msg_item_done.get(&idx).copied().unwrap_or(false)
                    {
                        close_message_item(st, idx, &mut out, next_seq);
                    }
                    for tc in tcs {
                        let tool_index = tc.get("index").and_then(Value::as_i64).unwrap_or(0);
                        let key = tool_state_key(idx, tool_index);
                        if !st.func_args_buf.contains_key(&key) {
                            st.func_args_buf.insert(key.clone(), String::new());
                            let output_index = alloc_output_index(st);
                            st.func_output_ix.insert(key.clone(), output_index);
                        }
                        if let Some(new_call_id) =
                            tc.get("id").and_then(Value::as_str).map(str::to_string)
                        {
                            if !new_call_id.is_empty() && !st.func_call_ids.contains_key(&key) {
                                st.func_call_ids.insert(key.clone(), new_call_id);
                            }
                        }
                        if let Some(name) = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(Value::as_str)
                        {
                            if !name.is_empty()
                                && !st.func_item_added.get(&key).copied().unwrap_or(false)
                            {
                                st.func_names.insert(key.clone(), name.to_string());
                            }
                        }
                        if let Some(args) = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(Value::as_str)
                        {
                            if !args.is_empty() {
                                let entry = st.func_args_buf.entry(key.clone()).or_default();
                                entry.push_str(args);
                            }
                        }
                        emit_tool_item(st, &key, false, request_for_namespace, &mut out, next_seq);
                        emit_pending_function_args(st, &key, &mut out, next_seq);
                    }
                }
            }
            if let Some(fr) = choice.get("finish_reason").and_then(Value::as_str) {
                if !fr.is_empty() {
                    st.finish_reason = fr.to_owned();
                    finalize_choice(st, idx, request_for_namespace, &mut out, next_seq);
                }
            }
        }
    }
    st.sequence_number = counter;
    out
}

/// Checks translator-owned terminal state before synthesizing a terminator at EOF.
/// ref: internal/translator/openai/openai/responses/openai_openai-responses_response.go:1154:1181 @ a2976eb8a303f11b4ea5177bce9f9ff752634dfc
pub fn can_finalize_response_stream(state: &TranslationState) -> bool {
    let Some(st) = state
        .as_ref()
        .and_then(|state| state.downcast_ref::<OaiToResponsesState>())
    else {
        return false;
    };
    st.started
        && !st.completed_emitted
        && !st.finish_reason.is_empty()
        && (!st.msg_item_added.is_empty() || !st.func_item_added.is_empty())
        && st
            .msg_item_added
            .keys()
            .all(|index| st.msg_item_done.get(index) == Some(&true))
        && st
            .func_item_added
            .keys()
            .all(|key| st.func_item_done.get(key) == Some(&true))
        && st.reasoning_id_field.is_empty()
}

fn oai_to_responses_state(state: &mut TranslationState) -> &mut OaiToResponsesState {
    let needs_state = match state.as_ref() {
        Some(value) => !value.is::<OaiToResponsesState>(),
        None => true,
    };
    if needs_state {
        *state = Some(Box::new(OaiToResponsesState::default()));
    }
    state
        .as_mut()
        .and_then(|value| value.downcast_mut::<OaiToResponsesState>())
        .expect("oai->responses state was initialized with the expected type")
}

fn alloc_output_index(st: &mut OaiToResponsesState) -> i64 {
    let ix = st.next_output_ix;
    st.next_output_ix += 1;
    ix
}

fn tool_state_key(output_index: i64, tool_index: i64) -> String {
    format!("{output_index}:{tool_index}")
}

fn msg_item_id(st: &OaiToResponsesState, idx: i64) -> String {
    format!("msg_{}_{idx}", st.response_id)
}

fn close_message_item(
    st: &mut OaiToResponsesState,
    idx: i64,
    out: &mut Vec<Vec<u8>>,
    next_seq: &mut dyn FnMut() -> i64,
) {
    let msg_output_index = st.msg_output_ix.get(&idx).copied().unwrap_or(0);
    let full_text = st.msg_text_buf.get(&idx).cloned().unwrap_or_default();
    let mut done = json!({
        "type":"response.output_text.done",
        "sequence_number":0,
        "item_id":"",
        "output_index":0,
        "content_index":0,
        "text":"",
        "logprobs":[]
    });
    done["sequence_number"] = Value::Number(next_seq().into());
    done["item_id"] = Value::String(msg_item_id(st, idx));
    done["output_index"] = Value::Number(msg_output_index.into());
    done["content_index"] = Value::Number(0.into());
    done["text"] = Value::String(full_text.clone());
    let bytes = serde_json::to_vec(&done).unwrap_or_default();
    out.push(sse_event_data("response.output_text.done", &bytes));
    let mut part_done = json!({
        "type":"response.content_part.done",
        "sequence_number":0,
        "item_id":"",
        "output_index":0,
        "content_index":0,
        "part":{
            "type":"output_text",
            "annotations":[],
            "logprobs":[],
            "text":""
        }
    });
    part_done["sequence_number"] = Value::Number(next_seq().into());
    part_done["item_id"] = Value::String(msg_item_id(st, idx));
    part_done["output_index"] = Value::Number(msg_output_index.into());
    part_done["content_index"] = Value::Number(0.into());
    part_done["part"]["text"] = Value::String(full_text.clone());
    let bytes = serde_json::to_vec(&part_done).unwrap_or_default();
    out.push(sse_event_data("response.content_part.done", &bytes));
    let mut item_done = json!({
        "type":"response.output_item.done",
        "sequence_number":0,
        "output_index":0,
        "item":{
            "id":"",
            "type":"message",
            "status":"completed",
            "content":[{"type":"output_text","annotations":[],"logprobs":[],"text":""}],
            "role":"assistant"
        }
    });
    item_done["sequence_number"] = Value::Number(next_seq().into());
    item_done["output_index"] = Value::Number(msg_output_index.into());
    item_done["item"]["id"] = Value::String(msg_item_id(st, idx));
    item_done["item"]["content"][0]["text"] = Value::String(full_text);
    let bytes = serde_json::to_vec(&item_done).unwrap_or_default();
    out.push(sse_event_data("response.output_item.done", &bytes));
    st.msg_item_done.insert(idx, true);
}

fn emit_reasoning_done(
    st: &mut OaiToResponsesState,
    text: &str,
    out: &mut Vec<Vec<u8>>,
    next_seq: &mut dyn FnMut() -> i64,
) {
    let mut text_done = json!({
        "type":"response.reasoning_summary_text.done",
        "sequence_number":0,
        "item_id":"",
        "output_index":0,
        "summary_index":0,
        "text":""
    });
    text_done["sequence_number"] = Value::Number(next_seq().into());
    text_done["item_id"] = Value::String(st.reasoning_id_field.clone());
    text_done["output_index"] = Value::Number(st.reasoning_index_field.into());
    text_done["text"] = Value::String(text.to_string());
    let bytes = serde_json::to_vec(&text_done).unwrap_or_default();
    out.push(sse_event_data(
        "response.reasoning_summary_text.done",
        &bytes,
    ));
    let mut part_done = json!({
        "type":"response.reasoning_summary_part.done",
        "sequence_number":0,
        "item_id":"",
        "output_index":0,
        "summary_index":0,
        "part":{"type":"summary_text","text":""}
    });
    part_done["sequence_number"] = Value::Number(next_seq().into());
    part_done["item_id"] = Value::String(st.reasoning_id_field.clone());
    part_done["output_index"] = Value::Number(st.reasoning_index_field.into());
    part_done["part"]["text"] = Value::String(text.to_string());
    let bytes = serde_json::to_vec(&part_done).unwrap_or_default();
    out.push(sse_event_data(
        "response.reasoning_summary_part.done",
        &bytes,
    ));
    let mut output_item_done = json!({
        "type":"response.output_item.done",
        "item":{
            "id":"",
            "type":"reasoning",
            "encrypted_content":"",
            "summary":[{"type":"summary_text","text":""}]
        },
        "output_index":0,
        "sequence_number":0
    });
    output_item_done["sequence_number"] = Value::Number(next_seq().into());
    output_item_done["item"]["id"] = Value::String(st.reasoning_id_field.clone());
    output_item_done["output_index"] = Value::Number(st.reasoning_index_field.into());
    output_item_done["item"]["summary"][0]["text"] = Value::String(text.to_string());
    let bytes = serde_json::to_vec(&output_item_done).unwrap_or_default();
    out.push(sse_event_data("response.output_item.done", &bytes));
    st.reasonings.push(OaiToResponsesStateReasoning {
        reasoning_id: st.reasoning_id_field.clone(),
        reasoning_data: text.to_string(),
        output_index: st.reasoning_index_field,
    });
    st.reasoning_id_field.clear();
}

fn emit_tool_item(
    st: &mut OaiToResponsesState,
    key: &str,
    force: bool,
    request_for_namespace: &[u8],
    out: &mut Vec<Vec<u8>>,
    next_seq: &mut dyn FnMut() -> i64,
) {
    if st.func_item_added.get(key).copied().unwrap_or(false) {
        return;
    }
    let call_id = st.func_call_ids.get(key).cloned().unwrap_or_default();
    let mut name = st.func_names.get(key).cloned().unwrap_or_default();
    if !force && (call_id.is_empty() || name.is_empty()) {
        return;
    }
    if name.is_empty() {
        if let Some((custom_name, _)) = responses_single_custom_tool_name(request_for_namespace) {
            name = custom_name.clone();
            st.func_names.insert(key.to_string(), custom_name);
        }
    }
    let call_id_owned;
    let call_id_value: String = if call_id.is_empty() {
        call_id_owned = format!("call_{}_{}", st.response_id, key.replace(':', "_"));
        st.func_call_ids
            .insert(key.to_string(), call_id_owned.clone());
        call_id_owned
    } else {
        call_id
    };
    let output_index = st.func_output_ix.get(key).copied().unwrap_or(0);
    let is_custom_tool = st.custom_tool_names.contains(&name);
    st.func_item_custom.insert(key.to_string(), is_custom_tool);
    if is_custom_tool {
        let mut payload = json!({
            "type":"response.output_item.added",
            "sequence_number":0,
            "output_index":0,
            "item":{
                "id":"",
                "type":"custom_tool_call",
                "status":"in_progress",
                "input":"",
                "call_id":"",
                "name":""
            }
        });
        payload["sequence_number"] = Value::Number(next_seq().into());
        payload["output_index"] = Value::Number(output_index.into());
        payload["item"]["id"] = Value::String(format!("ctc_{call_id_value}"));
        payload["item"]["call_id"] = Value::String(call_id_value.clone());
        payload["item"]["name"] = Value::String(name.clone());
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        out.push(sse_event_data("response.output_item.added", &bytes));
    } else {
        let mut payload = json!({
            "type":"response.output_item.added",
            "sequence_number":0,
            "output_index":0,
            "item":{
                "id":"",
                "type":"function_call",
                "status":"in_progress",
                "arguments":"",
                "call_id":"",
                "name":""
            }
        });
        payload["sequence_number"] = Value::Number(next_seq().into());
        payload["output_index"] = Value::Number(output_index.into());
        payload["item"]["id"] = Value::String(format!("fc_{call_id_value}"));
        payload["item"]["call_id"] = Value::String(call_id_value.clone());
        let item = payload["item"].clone();
        payload["item"] =
            apply_responses_function_call_namespace_fields(item, request_for_namespace, &name, "");
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        out.push(sse_event_data("response.output_item.added", &bytes));
    }
    st.func_item_added.insert(key.to_string(), true);
}

fn emit_tool_item_force(
    st: &mut OaiToResponsesState,
    key: &str,
    request_for_namespace: &[u8],
    out: &mut Vec<Vec<u8>>,
    next_seq: &mut dyn FnMut() -> i64,
) {
    if st.func_item_added.get(key).copied().unwrap_or(false) {
        return;
    }
    let call_id = st.func_call_ids.get(key).cloned().unwrap_or_default();
    let mut name = st.func_names.get(key).cloned().unwrap_or_default();
    if name.is_empty() {
        if let Some((custom_name, _)) = responses_single_custom_tool_name(request_for_namespace) {
            name = custom_name.clone();
            st.func_names.insert(key.to_string(), custom_name);
        }
    }
    let call_id_owned;
    let call_id_value: String = if call_id.is_empty() {
        call_id_owned = format!("call_{}_{}", st.response_id, key.replace(':', "_"));
        st.func_call_ids
            .insert(key.to_string(), call_id_owned.clone());
        call_id_owned
    } else {
        call_id
    };
    let output_index = st.func_output_ix.get(key).copied().unwrap_or(0);
    let is_custom_tool = st.custom_tool_names.contains(&name);
    st.func_item_custom.insert(key.to_string(), is_custom_tool);
    if is_custom_tool {
        let mut payload = json!({
            "type":"response.output_item.added",
            "sequence_number":0,
            "output_index":0,
            "item":{
                "id":"",
                "type":"custom_tool_call",
                "status":"in_progress",
                "input":"",
                "call_id":"",
                "name":""
            }
        });
        payload["sequence_number"] = Value::Number(next_seq().into());
        payload["output_index"] = Value::Number(output_index.into());
        payload["item"]["id"] = Value::String(format!("ctc_{call_id_value}"));
        payload["item"]["call_id"] = Value::String(call_id_value.clone());
        payload["item"]["name"] = Value::String(name.clone());
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        out.push(sse_event_data("response.output_item.added", &bytes));
    } else {
        let mut payload = json!({
            "type":"response.output_item.added",
            "sequence_number":0,
            "output_index":0,
            "item":{
                "id":"",
                "type":"function_call",
                "status":"in_progress",
                "arguments":"",
                "call_id":"",
                "name":""
            }
        });
        payload["sequence_number"] = Value::Number(next_seq().into());
        payload["output_index"] = Value::Number(output_index.into());
        payload["item"]["id"] = Value::String(format!("fc_{call_id_value}"));
        payload["item"]["call_id"] = Value::String(call_id_value.clone());
        let item = payload["item"].clone();
        payload["item"] =
            apply_responses_function_call_namespace_fields(item, request_for_namespace, &name, "");
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        out.push(sse_event_data("response.output_item.added", &bytes));
    }
    st.func_item_added.insert(key.to_string(), true);
}

fn emit_pending_function_args(
    st: &mut OaiToResponsesState,
    key: &str,
    out: &mut Vec<Vec<u8>>,
    next_seq: &mut dyn FnMut() -> i64,
) {
    if !st.func_item_added.get(key).copied().unwrap_or(false) {
        return;
    }
    if st.func_item_custom.get(key).copied().unwrap_or(false) {
        return;
    }
    let Some(args) = st.func_args_buf.get(key) else {
        return;
    };
    let sent = st.func_args_sent.get(key).copied().unwrap_or(0);
    if (args.len() as i64) <= sent {
        return;
    }
    let delta = &args[sent as usize..];
    let call_id = st.func_call_ids.get(key).cloned().unwrap_or_default();
    let mut payload = json!({
        "type":"response.function_call_arguments.delta",
        "sequence_number":0,
        "item_id":"",
        "output_index":0,
        "delta":""
    });
    payload["sequence_number"] = Value::Number(next_seq().into());
    payload["item_id"] = Value::String(format!("fc_{call_id}"));
    payload["output_index"] =
        Value::Number(st.func_output_ix.get(key).copied().unwrap_or(0).into());
    payload["delta"] = Value::String(delta.to_string());
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    out.push(sse_event_data(
        "response.function_call_arguments.delta",
        &bytes,
    ));
    st.func_args_sent.insert(key.to_string(), args.len() as i64);
}

fn finalize_choice(
    st: &mut OaiToResponsesState,
    _idx: i64,
    request_for_namespace: &[u8],
    out: &mut Vec<Vec<u8>>,
    next_seq: &mut dyn FnMut() -> i64,
) {
    if !st.msg_item_added.is_empty() {
        let mut indices: Vec<i64> = st.msg_item_added.keys().copied().collect();
        indices.sort_by_key(|i| st.msg_output_ix.get(i).copied().unwrap_or(0));
        for i in indices {
            if st.msg_item_added.get(&i).copied().unwrap_or(false)
                && !st.msg_item_done.get(&i).copied().unwrap_or(false)
            {
                close_message_item(st, i, out, next_seq);
            }
        }
    }

    if !st.reasoning_id_field.is_empty() {
        let buffer = std::mem::take(&mut st.reasoning_text_buffer);
        emit_reasoning_done(st, &buffer, out, next_seq);
    }

    if !st.func_args_buf.is_empty() {
        let mut keys: Vec<String> = st.func_args_buf.keys().cloned().collect();
        keys.sort_by(|a, b| {
            let left = st.func_output_ix.get(a).copied().unwrap_or(0);
            let right = st.func_output_ix.get(b).copied().unwrap_or(0);
            left.cmp(&right).then_with(|| a.cmp(b))
        });
        for key in keys {
            emit_tool_item_force(st, &key, request_for_namespace, out, next_seq);
            emit_pending_function_args(st, &key, out, next_seq);
            let call_id = match st.func_call_ids.get(&key) {
                Some(value) if !value.is_empty() => value.clone(),
                _ => continue,
            };
            if st.func_item_done.get(&key).copied().unwrap_or(false) {
                continue;
            }
            let output_index = st.func_output_ix.get(&key).copied().unwrap_or(0);
            let args = st.func_args_buf.get(&key).cloned().unwrap_or_default();
            let args = if args.is_empty() {
                "{}".to_string()
            } else {
                args
            };
            if st.func_item_custom.get(&key).copied().unwrap_or(false) {
                let input = unwrap_custom_tool_input(&args);
                let mut input_done = json!({
                    "type":"response.custom_tool_call_input.done",
                    "sequence_number":0,
                    "item_id":"",
                    "output_index":0,
                    "input":""
                });
                input_done["sequence_number"] = Value::Number(next_seq().into());
                input_done["item_id"] = Value::String(format!("ctc_{call_id}"));
                input_done["output_index"] = Value::Number(output_index.into());
                input_done["input"] = Value::String(input.clone());
                let bytes = serde_json::to_vec(&input_done).unwrap_or_default();
                out.push(sse_event_data(
                    "response.custom_tool_call_input.done",
                    &bytes,
                ));

                let mut item_done = json!({
                    "type":"response.output_item.done",
                    "sequence_number":0,
                    "output_index":0,
                    "item":{
                        "id":"",
                        "type":"custom_tool_call",
                        "status":"completed",
                        "input":"",
                        "call_id":"",
                        "name":""
                    }
                });
                item_done["sequence_number"] = Value::Number(next_seq().into());
                item_done["output_index"] = Value::Number(output_index.into());
                item_done["item"]["id"] = Value::String(format!("ctc_{call_id}"));
                item_done["item"]["input"] = Value::String(input);
                item_done["item"]["call_id"] = Value::String(call_id.clone());
                item_done["item"]["name"] =
                    Value::String(st.func_names.get(&key).cloned().unwrap_or_default());
                let bytes = serde_json::to_vec(&item_done).unwrap_or_default();
                out.push(sse_event_data("response.output_item.done", &bytes));
                st.func_item_done.insert(key.clone(), true);
                st.func_args_done.insert(key.clone(), true);
            } else {
                let mut fc_done = json!({
                    "type":"response.function_call_arguments.done",
                    "sequence_number":0,
                    "item_id":"",
                    "output_index":0,
                    "arguments":""
                });
                fc_done["sequence_number"] = Value::Number(next_seq().into());
                fc_done["item_id"] = Value::String(format!("fc_{call_id}"));
                fc_done["output_index"] = Value::Number(output_index.into());
                fc_done["arguments"] = Value::String(args.clone());
                let bytes = serde_json::to_vec(&fc_done).unwrap_or_default();
                out.push(sse_event_data(
                    "response.function_call_arguments.done",
                    &bytes,
                ));

                let mut item_done = json!({
                    "type":"response.output_item.done",
                    "sequence_number":0,
                    "output_index":0,
                    "item":{
                        "id":"",
                        "type":"function_call",
                        "status":"completed",
                        "arguments":"",
                        "call_id":"",
                        "name":""
                    }
                });
                item_done["sequence_number"] = Value::Number(next_seq().into());
                item_done["output_index"] = Value::Number(output_index.into());
                item_done["item"]["id"] = Value::String(format!("fc_{call_id}"));
                item_done["item"]["arguments"] = Value::String(args);
                item_done["item"]["call_id"] = Value::String(call_id.clone());
                let item = item_done["item"].clone();
                item_done["item"] = apply_responses_function_call_namespace_fields(
                    item,
                    request_for_namespace,
                    st.func_names
                        .get(&key)
                        .cloned()
                        .unwrap_or_default()
                        .as_str(),
                    "",
                );
                let bytes = serde_json::to_vec(&item_done).unwrap_or_default();
                out.push(sse_event_data("response.output_item.done", &bytes));
                st.func_item_done.insert(key.clone(), true);
                st.func_args_done.insert(key.clone(), true);
            }
        }
    }
}

fn build_responses_completed_event(
    st: &OaiToResponsesState,
    request_raw_json: &[u8],
    next_seq: &mut dyn FnMut() -> i64,
) -> Vec<u8> {
    let mut completed = json!({
        "type":"response.completed",
        "sequence_number":0,
        "response":{
            "id":"",
            "object":"response",
            "created_at":0,
            "status":"completed",
            "background":false,
            "error":null
        }
    });
    completed["sequence_number"] = Value::Number(next_seq().into());
    completed["response"]["id"] = Value::String(st.response_id.clone());
    completed["response"]["created_at"] = Value::Number(st.created.into());
    if let Some(root) = parse_request(request_raw_json) {
        copy_string_field(
            &root,
            "instructions",
            &mut completed,
            "response.instructions",
        );
        copy_int_field(
            &root,
            "max_output_tokens",
            &mut completed,
            "response.max_output_tokens",
        );
        copy_int_field(
            &root,
            "max_tool_calls",
            &mut completed,
            "response.max_tool_calls",
        );
        copy_string_field(&root, "model", &mut completed, "response.model");
        copy_bool_field(
            &root,
            "parallel_tool_calls",
            &mut completed,
            "response.parallel_tool_calls",
        );
        copy_string_field(
            &root,
            "previous_response_id",
            &mut completed,
            "response.previous_response_id",
        );
        copy_string_field(
            &root,
            "prompt_cache_key",
            &mut completed,
            "response.prompt_cache_key",
        );
        copy_raw_field(&root, "reasoning", &mut completed, "response.reasoning");
        copy_string_field(
            &root,
            "safety_identifier",
            &mut completed,
            "response.safety_identifier",
        );
        copy_string_field(
            &root,
            "service_tier",
            &mut completed,
            "response.service_tier",
        );
        copy_bool_field(&root, "store", &mut completed, "response.store");
        copy_float_field(&root, "temperature", &mut completed, "response.temperature");
        copy_raw_field(&root, "text", &mut completed, "response.text");
        copy_raw_field(&root, "tool_choice", &mut completed, "response.tool_choice");
        copy_raw_field(&root, "tools", &mut completed, "response.tools");
        copy_int_field(
            &root,
            "top_logprobs",
            &mut completed,
            "response.top_logprobs",
        );
        copy_float_field(&root, "top_p", &mut completed, "response.top_p");
        copy_string_field(&root, "truncation", &mut completed, "response.truncation");
        copy_raw_field(&root, "user", &mut completed, "response.user");
        copy_raw_field(&root, "metadata", &mut completed, "response.metadata");
    }
    let mut entries: Vec<(i64, Value)> = Vec::new();
    for r in &st.reasonings {
        let item = json!({
            "id":r.reasoning_id,
            "type":"reasoning",
            "summary":[{"type":"summary_text","text":r.reasoning_data}]
        });
        entries.push((r.output_index, item));
    }
    for i in st.msg_item_added.keys() {
        let text = st.msg_text_buf.get(i).cloned().unwrap_or_default();
        let output_index = st.msg_output_ix.get(i).copied().unwrap_or(0);
        let item = json!({
            "id":msg_item_id(st, *i),
            "type":"message",
            "status":"completed",
            "content":[{"type":"output_text","annotations":[],"logprobs":[],"text":text}],
            "role":"assistant"
        });
        entries.push((output_index, item));
    }
    for (key, args) in &st.func_args_buf {
        let call_id = st.func_call_ids.get(key).cloned().unwrap_or_default();
        let name = st.func_names.get(key).cloned().unwrap_or_default();
        let output_index = st.func_output_ix.get(key).copied().unwrap_or(0);
        let item = if st.func_item_custom.get(key).copied().unwrap_or(false) {
            json!({
                "id":format!("ctc_{call_id}"),
                "type":"custom_tool_call",
                "status":"completed",
                "input":unwrap_custom_tool_input(args),
                "call_id":call_id,
                "name":name
            })
        } else {
            let mut item = json!({
                "id":format!("fc_{call_id}"),
                "type":"function_call",
                "status":"completed",
                "arguments":args,
                "call_id":call_id,
                "name":name
            });
            item =
                apply_responses_function_call_namespace_fields(item, request_raw_json, &name, "");
            item
        };
        entries.push((output_index, item));
    }
    entries.sort_by_key(|(index, _)| *index);
    if !entries.is_empty() {
        let array: Vec<Value> = entries.into_iter().map(|(_, value)| value).collect();
        completed["response"]["output"] = Value::Array(array);
    }
    if st.usage_seen {
        completed["response"]["usage"]["input_tokens"] = Value::Number(st.prompt_tokens.into());
        completed["response"]["usage"]["input_tokens_details"]["cached_tokens"] =
            Value::Number(st.cached_tokens.into());
        completed["response"]["usage"]["output_tokens"] =
            Value::Number(st.completion_tokens.into());
        if st.reasoning_tokens > 0 {
            completed["response"]["usage"]["output_tokens_details"]["reasoning_tokens"] =
                Value::Number(st.reasoning_tokens.into());
        }
        let total = if st.total_tokens == 0 {
            st.prompt_tokens + st.completion_tokens
        } else {
            st.total_tokens
        };
        completed["response"]["usage"]["total_tokens"] = Value::Number(total.into());
    }
    let bytes = serde_json::to_vec(&completed).unwrap_or_default();
    sse_event_data("response.completed", &bytes)
}

pub fn convert_openai_chat_completions_response_to_openai_responses_non_stream(
    _context: &TranslationContext,
    _model_name: &str,
    original_request_raw_json: &[u8],
    request_raw_json: &[u8],
    raw_json: &[u8],
    _state: &mut TranslationState,
) -> Vec<u8> {
    let root: Value = match serde_json::from_slice(raw_json) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let request_for_namespace_owned;
    let request_for_namespace: &[u8] =
        match pick_request_json(original_request_raw_json, request_raw_json) {
            Some(value) => value,
            None => {
                request_for_namespace_owned = Vec::new();
                &request_for_namespace_owned
            }
        };

    let mut resp = json!({
        "id":"",
        "object":"response",
        "created_at":0,
        "status":"completed",
        "background":false,
        "error":null,
        "incomplete_details":null
    });

    let id = root
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| synthesized_response_id(original_request_raw_json, request_raw_json));
    resp["id"] = Value::String(id.clone());

    let created = root
        .get("created")
        .and_then(Value::as_i64)
        .filter(|value| *value != 0);
    let created_unix = created
        .unwrap_or_else(|| synthesized_request_epoch(original_request_raw_json, request_raw_json));
    resp["created_at"] = Value::Number(created_unix.into());

    if let Some(req_root) = parse_request(request_raw_json) {
        copy_string_field(&req_root, "instructions", &mut resp, "instructions");
        if req_root.get("max_output_tokens").is_some() {
            copy_int_field(
                &req_root,
                "max_output_tokens",
                &mut resp,
                "max_output_tokens",
            );
        } else if req_root.get("max_tokens").is_some() {
            copy_int_field(&req_root, "max_tokens", &mut resp, "max_output_tokens");
        }
        copy_int_field(&req_root, "max_tool_calls", &mut resp, "max_tool_calls");
        if req_root.get("model").is_some() {
            copy_string_field(&req_root, "model", &mut resp, "model");
        } else if root.get("model").is_some() {
            copy_string_field(&root, "model", &mut resp, "model");
        }
        copy_bool_field(
            &req_root,
            "parallel_tool_calls",
            &mut resp,
            "parallel_tool_calls",
        );
        copy_string_field(
            &req_root,
            "previous_response_id",
            &mut resp,
            "previous_response_id",
        );
        copy_string_field(&req_root, "prompt_cache_key", &mut resp, "prompt_cache_key");
        copy_raw_field(&req_root, "reasoning", &mut resp, "reasoning");
        copy_string_field(
            &req_root,
            "safety_identifier",
            &mut resp,
            "safety_identifier",
        );
        copy_string_field(&req_root, "service_tier", &mut resp, "service_tier");
        copy_bool_field(&req_root, "store", &mut resp, "store");
        copy_float_field(&req_root, "temperature", &mut resp, "temperature");
        copy_raw_field(&req_root, "text", &mut resp, "text");
        copy_raw_field(&req_root, "tool_choice", &mut resp, "tool_choice");
        copy_raw_field(&req_root, "tools", &mut resp, "tools");
        copy_int_field(&req_root, "top_logprobs", &mut resp, "top_logprobs");
        copy_float_field(&req_root, "top_p", &mut resp, "top_p");
        copy_string_field(&req_root, "truncation", &mut resp, "truncation");
        copy_raw_field(&req_root, "user", &mut resp, "user");
        copy_raw_field(&req_root, "metadata", &mut resp, "metadata");
    } else if root.get("model").is_some() {
        copy_string_field(&root, "model", &mut resp, "model");
    }

    let mut entries: Vec<Value> = Vec::new();
    let rc_text = root
        .pointer("/choices/0/message/reasoning_content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut include_reasoning = !rc_text.is_empty();
    if !include_reasoning {
        if let Some(req_root) = parse_request(request_raw_json) {
            include_reasoning = req_root
                .get("reasoning")
                .map(|v| !v.is_null())
                .unwrap_or(false);
        }
    }
    if include_reasoning {
        let rid = id.trim_start_matches("resp_");
        let mut item = json!({
            "id":format!("rs_{rid}"),
            "type":"reasoning",
            "encrypted_content":"",
            "summary":[]
        });
        if !rc_text.is_empty() {
            item["summary"] = json!([{"type":"summary_text","text":rc_text}]);
        }
        entries.push(item);
    }

    if let Some(choices) = root.get("choices").and_then(Value::as_array) {
        let custom_tool_names: HashSet<String> =
            if let Some(req_root) = parse_request(request_for_namespace) {
                responses_custom_tool_names_from_value(&req_root)
            } else {
                HashSet::new()
            };
        for (choice_index, choice) in choices.iter().enumerate() {
            if let Some(message) = choice.get("message") {
                if let Some(content) = message.get("content") {
                    if let Some(text) = content.as_str() {
                        if !text.is_empty() {
                            let mut item = json!({
                                "id":format!("msg_{}_{choice_index}", id),
                                "type":"message",
                                "status":"completed",
                                "content":[{"type":"output_text","annotations":[],"logprobs":[],"text":""}],
                                "role":"assistant"
                            });
                            item["content"][0]["text"] = Value::String(text.to_string());
                            entries.push(item);
                        }
                    }
                }
                if let Some(tcs) = message.get("tool_calls").and_then(Value::as_array) {
                    for (tc_index, tc) in tcs.iter().enumerate() {
                        let mut call_id = tc
                            .get("id")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .unwrap_or_default();
                        if call_id.is_empty() {
                            call_id = format!("call_{}_{}_{}", id, choice_index, tc_index);
                        }
                        let name = tc
                            .pointer("/function/name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let args = tc
                            .pointer("/function/arguments")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let is_custom_tool = custom_tool_names.contains(&name);
                        let item = if is_custom_tool {
                            json!({
                                "id":format!("ctc_{call_id}"),
                                "type":"custom_tool_call",
                                "status":"completed",
                                "input":unwrap_custom_tool_input(&args),
                                "call_id":call_id,
                                "name":name
                            })
                        } else {
                            let mut item = json!({
                                "id":format!("fc_{call_id}"),
                                "type":"function_call",
                                "status":"completed",
                                "arguments":args,
                                "call_id":call_id,
                                "name":name
                            });
                            item = apply_responses_function_call_namespace_fields(
                                item,
                                request_for_namespace,
                                &name,
                                "",
                            );
                            item
                        };
                        entries.push(item);
                    }
                }
            }
        }
    }
    if !entries.is_empty() {
        resp["output"] = Value::Array(entries);
    }

    if let Some(usage) = root.get("usage") {
        if usage.get("prompt_tokens").is_some()
            || usage.get("completion_tokens").is_some()
            || usage.get("total_tokens").is_some()
        {
            resp["usage"]["input_tokens"] = Value::Number(
                usage
                    .get("prompt_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .into(),
            );
            if let Some(cached) = usage
                .pointer("/prompt_tokens_details/cached_tokens")
                .and_then(Value::as_i64)
            {
                resp["usage"]["input_tokens_details"]["cached_tokens"] =
                    Value::Number(cached.into());
            }
            resp["usage"]["output_tokens"] = Value::Number(
                usage
                    .get("completion_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .into(),
            );
            if let Some(reasoning) = usage
                .pointer("/output_tokens_details/reasoning_tokens")
                .and_then(Value::as_i64)
            {
                resp["usage"]["output_tokens_details"]["reasoning_tokens"] =
                    Value::Number(reasoning.into());
            }
            resp["usage"]["total_tokens"] = Value::Number(
                usage
                    .get("total_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .into(),
            );
        } else {
            resp["usage"] = usage.clone();
        }
    }

    serde_json::to_vec(&resp).unwrap_or_default()
}

fn parse_request(raw: &[u8]) -> Option<Value> {
    serde_json::from_slice(raw).ok()
}

fn strip_data_prefix(raw: &[u8]) -> Option<&[u8]> {
    raw.strip_prefix(b"data:").map(trimmed_payload)
}

fn trimmed_payload(raw: &[u8]) -> &[u8] {
    match std::str::from_utf8(raw) {
        Ok(text) => text.trim().as_bytes(),
        Err(_) => raw.trim_ascii(),
    }
}

fn copy_string_field(root: &Value, from: &str, out: &mut Value, to: &str) {
    if let Some(value) = root.get(from).and_then(Value::as_str) {
        set_path(out, to, Value::String(value.to_string()));
    }
}

fn copy_int_field(root: &Value, from: &str, out: &mut Value, to: &str) {
    if let Some(value) = root.get(from).and_then(Value::as_i64) {
        set_path(out, to, Value::Number(value.into()));
    }
}

fn copy_float_field(root: &Value, from: &str, out: &mut Value, to: &str) {
    if let Some(value) = root.get(from).and_then(Value::as_f64) {
        if let Some(number) = serde_json::Number::from_f64(value) {
            set_path(out, to, Value::Number(number));
        }
    }
}

fn copy_bool_field(root: &Value, from: &str, out: &mut Value, to: &str) {
    if let Some(value) = root.get(from).and_then(Value::as_bool) {
        set_path(out, to, Value::Bool(value));
    }
}

fn copy_raw_field(root: &Value, from: &str, out: &mut Value, to: &str) {
    if let Some(value) = root.get(from) {
        if !value.is_null() {
            set_path(out, to, value.clone());
        }
    }
}

fn set_path(root: &mut Value, path: &str, value: Value) {
    let Some((head, tail)) = path.split_once('.') else {
        if let Some(object) = root.as_object_mut() {
            object.insert(path.to_string(), value);
        }
        return;
    };
    if let Some(object) = root.as_object_mut() {
        let entry = object
            .entry(head.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
        set_path(entry, tail, value);
    }
}

fn responses_custom_tool_names_from_value(root: &Value) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_custom_tool_names(root.get("tools"), "", &mut names);
    if let Some(items) = root.get("input").and_then(Value::as_array) {
        for item in items {
            if item.get("type").and_then(Value::as_str) == Some("additional_tools") {
                collect_custom_tool_names(item.get("tools"), "", &mut names);
            }
        }
    }
    names
}

fn collect_custom_tool_names(tools: Option<&Value>, namespace: &str, names: &mut HashSet<String>) {
    let Some(tools) = tools.and_then(Value::as_array) else {
        return;
    };
    for tool in tools {
        match tool.get("type").and_then(Value::as_str).unwrap_or("") {
            "custom" => {
                let base = tools::responses_tool_name(tool);
                let qualified = tools::qualify_responses_namespace_tool_name(namespace, &base);
                if !qualified.is_empty() {
                    names.insert(qualified);
                }
            }
            "namespace" => {
                let namespace_name = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                collect_custom_tool_names(tool.get("tools"), namespace_name, names);
            }
            _ => {}
        }
    }
}

fn synthesized_response_id(original_request: &[u8], request: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    for value in [original_request, request] {
        value.hash(&mut hasher);
    }
    format!("resp_ctox_{:016x}", hasher.finish())
}

fn synthesized_request_epoch(original_request: &[u8], request: &[u8]) -> i64 {
    let mut hasher = DefaultHasher::new();
    for value in [original_request, request] {
        value.hash(&mut hasher);
    }
    ((hasher.finish() as i64) / 1_000_000).max(0)
}
