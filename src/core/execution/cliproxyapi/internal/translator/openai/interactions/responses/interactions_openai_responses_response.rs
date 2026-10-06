// ref: internal/translator/openai/interactions/responses/interactions_openai_responses_response.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::interactions_openai_responses_request::{
    interactions_content_texts, interactions_function_call_to_responses,
    responses_content_part_to_interactions,
};
use crate::sdk::translator::{TranslationContext, TranslationState};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
struct InteractionsToResponsesStreamState {
    function_calls: HashMap<i64, InteractionsFunctionCallState>,
    item_ids: HashMap<i64, String>,
    item_types: HashMap<i64, String>,
    reasoning_encrypted: HashMap<i64, String>,
    reasoning_summaries: HashMap<i64, Vec<String>>,
    text_outputs: HashMap<i64, String>,
    sequence: i64,
    done: bool,
}

#[derive(Default)]
struct InteractionsFunctionCallState {
    name: String,
    arguments: String,
    initial_arguments_emitted: bool,
    arguments_done_emitted: bool,
    item_done_emitted: bool,
}

#[derive(Default)]
struct ResponsesToInteractionsStreamState {
    id: String,
    timestamp: String,
    created: bool,
    status_updated: bool,
    completed: bool,
    done: bool,
    step_index: i64,
    active_step_index: i64,
    active_step_type: String,
    active_step_open: bool,
    sent_text: HashSet<String>,
    unkeyed_text_delta: bool,
    function_args_sent: HashSet<String>,
}

pub fn convert_openai_responses_response_to_interactions_stream(
    context: &TranslationContext,
    model_name: &str,
    _original_request: &[u8],
    _request: &[u8],
    raw_json: &[u8],
    state: &mut TranslationState,
) -> Vec<Vec<u8>> {
    if context.is_cancelled() {
        return Vec::new();
    }
    let state = responses_stream_state(state);
    let payload = interactions_sse_payload(raw_json);
    if payload.is_empty() {
        return Vec::new();
    }
    if payload.trim() == "[DONE]" {
        return append_interactions_done(Vec::new(), state);
    }
    let Ok(root) = serde_json::from_str::<Value>(&payload) else {
        return Vec::new();
    };
    match root.get("type").and_then(Value::as_str).unwrap_or("") {
        "response.created" => append_interactions_created(
            Vec::new(),
            state,
            model_name,
            root.get("response").unwrap_or(&Value::Null),
            true,
        ),
        "response.output_text.delta" => {
            let mut out =
                ensure_interactions_step(Vec::new(), state, model_name, "model_output", None);
            out = append_interactions_text_delta(out, state, string(root.get("delta")), false);
            mark_text_sent(state, text_keys_from_response_event(&root));
            out
        }
        "response.reasoning_summary_text.delta" => {
            let out = ensure_interactions_step(Vec::new(), state, model_name, "thought", None);
            append_interactions_text_delta(out, state, string(root.get("delta")), true)
        }
        "response.output_item.added" => response_output_item_added(model_name, &root, state),
        "response.function_call_arguments.delta" => {
            let mut out = ensure_interactions_function_step(Vec::new(), state, model_name, &root);
            out = append_interactions_arguments_delta(out, state, string(root.get("delta")));
            mark_function_args_sent(state, function_args_keys(&root));
            out
        }
        "response.output_item.done" => response_output_item_done(model_name, &root, state),
        "response.completed" => response_completed_to_interactions(
            model_name,
            root.get("response").unwrap_or(&Value::Null),
            state,
        ),
        _ => Vec::new(),
    }
}

fn responses_stream_state(state: &mut TranslationState) -> &mut ResponsesToInteractionsStreamState {
    let replace = state
        .as_ref()
        .is_none_or(|value| !value.is::<ResponsesToInteractionsStreamState>());
    if replace {
        *state = Some(Box::new(ResponsesToInteractionsStreamState {
            id: format!("interaction_{}", uuid::Uuid::new_v4().simple()),
            timestamp: "1970-01-01T00:00:00Z".into(),
            ..ResponsesToInteractionsStreamState::default()
        }));
    }
    state
        .as_mut()
        .and_then(|value| value.downcast_mut::<ResponsesToInteractionsStreamState>())
        .expect("Responses stream state was initialized with the expected type")
}

fn response_output_item_added(
    model_name: &str,
    root: &Value,
    state: &mut ResponsesToInteractionsStreamState,
) -> Vec<Vec<u8>> {
    let item = root.get("item").unwrap_or(&Value::Null);
    match item.get("type").and_then(Value::as_str).unwrap_or("") {
        "function_call" => {
            let mut out = ensure_interactions_created(Vec::new(), state, model_name);
            out = append_interactions_step_stop(out, state);
            let call_id = first_non_empty(&[string(item.get("call_id")), string(item.get("id"))]);
            let step = json!({"type":"function_call","name":string(item.get("name")),"id":call_id,"call_id":call_id,"arguments":{}});
            append_interactions_step_start(out, state, "function_call", Some(&step))
        }
        "message" => ensure_interactions_step(Vec::new(), state, model_name, "model_output", None),
        "reasoning" => ensure_interactions_step(Vec::new(), state, model_name, "thought", None),
        _ => Vec::new(),
    }
}

fn response_output_item_done(
    model_name: &str,
    root: &Value,
    state: &mut ResponsesToInteractionsStreamState,
) -> Vec<Vec<u8>> {
    let item = root.get("item").unwrap_or(&Value::Null);
    match item.get("type").and_then(Value::as_str).unwrap_or("") {
        "function_call" => {
            let mut out = ensure_interactions_function_step(Vec::new(), state, model_name, root);
            let keys = function_args_keys(root);
            let arguments = item.get("arguments").map(|value| json_string(value, "{}"));
            if arguments.as_ref().is_some_and(|value| !value.is_empty())
                && !has_sent_function_args(state, &keys)
            {
                out = append_interactions_arguments_delta(out, state, arguments.unwrap());
            }
            append_interactions_step_stop(out, state)
        }
        "reasoning" => {
            let mut out = ensure_interactions_step(Vec::new(), state, model_name, "thought", None);
            if let Some(summary) = item.get("summary").and_then(Value::as_array) {
                for entry in summary {
                    if let Some(text) = entry
                        .get("text")
                        .and_then(Value::as_str)
                        .filter(|v| !v.is_empty())
                    {
                        out = append_interactions_text_delta(out, state, text.into(), true);
                    }
                }
            }
            append_interactions_step_stop(out, state)
        }
        "message" => {
            append_response_message_fallback(Vec::new(), model_name, item, root, state, true)
        }
        _ => Vec::new(),
    }
}

fn response_completed_to_interactions(
    model_name: &str,
    response: &Value,
    state: &mut ResponsesToInteractionsStreamState,
) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if let Some(items) = response.get("output").and_then(Value::as_array) {
        for (index, item) in items.iter().enumerate() {
            if item.get("type").and_then(Value::as_str) == Some("message") {
                let root = json!({"output_index":index,"item_id":string(item.get("id"))});
                out = append_response_message_fallback(out, model_name, item, &root, state, false);
            }
        }
    }
    out = append_interactions_step_stop(out, state);
    out = append_interactions_completed(out, state, model_name, response);
    append_interactions_done(out, state)
}

fn append_response_message_fallback(
    mut out: Vec<Vec<u8>>,
    model_name: &str,
    item: &Value,
    root: &Value,
    state: &mut ResponsesToInteractionsStreamState,
    stop: bool,
) -> Vec<Vec<u8>> {
    let item_id = string(item.get("id"));
    let output_index = root.get("output_index").and_then(Value::as_i64);
    if let Some(content) = item.get("content").and_then(Value::as_array) {
        for (content_index, part) in content.iter().enumerate() {
            if !matches!(
                part.get("type").and_then(Value::as_str),
                Some("output_text" | "text")
            ) {
                continue;
            }
            let keys = response_text_keys(&item_id, output_index, Some(content_index as i64));
            let unkeyed = response_unkeyed_text_keys(&item_id, output_index);
            if has_sent_text(state, &keys, true) || has_sent_unkeyed_text(state, &unkeyed) {
                continue;
            }
            let text = string(part.get("text"));
            if text.is_empty() {
                continue;
            }
            out = ensure_interactions_step(out, state, model_name, "model_output", None);
            out = append_interactions_text_delta(out, state, text, false);
            mark_text_sent(state, keys);
        }
    }
    if stop {
        append_interactions_step_stop(out, state)
    } else {
        out
    }
}

fn append_interactions_created(
    mut out: Vec<Vec<u8>>,
    state: &mut ResponsesToInteractionsStreamState,
    model_name: &str,
    response: &Value,
    mark_status: bool,
) -> Vec<Vec<u8>> {
    if state.created {
        return out;
    }
    state.id = first_non_empty(&[string(response.get("id")), state.id.clone()]);
    state.timestamp = response_timestamp(response, &state.timestamp);
    out.push(emit_interactions_event(
        "interaction.created",
        json!({"interaction":{"id":state.id,"status":"in_progress","object":"interaction","model":response_model(model_name,response)},"event_type":"interaction.created"}),
    ));
    state.created = true;
    if mark_status {
        out = append_interactions_status(out, state);
    }
    out
}

fn ensure_interactions_created(
    out: Vec<Vec<u8>>,
    state: &mut ResponsesToInteractionsStreamState,
    model_name: &str,
) -> Vec<Vec<u8>> {
    append_interactions_created(out, state, model_name, &Value::Null, true)
}

fn append_interactions_status(
    mut out: Vec<Vec<u8>>,
    state: &mut ResponsesToInteractionsStreamState,
) -> Vec<Vec<u8>> {
    if !state.status_updated {
        out.push(emit_interactions_event("interaction.status_update", json!({"interaction_id":state.id,"status":"in_progress","event_type":"interaction.status_update"})));
        state.status_updated = true;
    }
    out
}

fn ensure_interactions_step(
    mut out: Vec<Vec<u8>>,
    state: &mut ResponsesToInteractionsStreamState,
    model_name: &str,
    step_type: &str,
    step: Option<&Value>,
) -> Vec<Vec<u8>> {
    out = ensure_interactions_created(out, state, model_name);
    if state.active_step_open && state.active_step_type == step_type {
        return out;
    }
    out = append_interactions_step_stop(out, state);
    append_interactions_step_start(out, state, step_type, step)
}

fn append_interactions_step_start(
    mut out: Vec<Vec<u8>>,
    state: &mut ResponsesToInteractionsStreamState,
    step_type: &str,
    step: Option<&Value>,
) -> Vec<Vec<u8>> {
    let index = state.step_index;
    state.step_index += 1;
    state.active_step_index = index;
    state.active_step_type = step_type.into();
    state.active_step_open = true;
    let mut value = json!({"index":index,"step":{"type":step_type},"event_type":"step.start"});
    if step_type == "function_call" {
        let step = step.unwrap_or(&Value::Null);
        let id = first_non_empty(&[string(step.get("call_id")), string(step.get("id"))]);
        value["step"]["id"] = Value::String(id.clone());
        value["step"]["call_id"] = Value::String(id);
        value["step"]["name"] = Value::String(string(step.get("name")));
        value["step"]["arguments"] = json!({});
    }
    out.push(emit_interactions_event("step.start", value));
    out
}

fn append_interactions_text_delta(
    mut out: Vec<Vec<u8>>,
    state: &ResponsesToInteractionsStreamState,
    text: String,
    thought: bool,
) -> Vec<Vec<u8>> {
    let value = if thought {
        json!({"index":state.active_step_index,"delta":{"content":{"text":text,"type":"text"},"type":"thought_summary"},"event_type":"step.delta"})
    } else {
        json!({"index":state.active_step_index,"delta":{"text":text,"type":"text"},"event_type":"step.delta"})
    };
    out.push(emit_interactions_event("step.delta", value));
    out
}

fn append_interactions_arguments_delta(
    mut out: Vec<Vec<u8>>,
    state: &ResponsesToInteractionsStreamState,
    arguments: String,
) -> Vec<Vec<u8>> {
    out.push(emit_interactions_event("step.delta", json!({"index":state.active_step_index,"delta":{"arguments":arguments,"type":"arguments_delta"},"event_type":"step.delta"})));
    out
}

fn append_interactions_step_stop(
    mut out: Vec<Vec<u8>>,
    state: &mut ResponsesToInteractionsStreamState,
) -> Vec<Vec<u8>> {
    if state.active_step_open {
        out.push(emit_interactions_event(
            "step.stop",
            json!({"index":state.active_step_index,"event_type":"step.stop"}),
        ));
        state.active_step_open = false;
        state.active_step_type.clear();
    }
    out
}

fn ensure_interactions_function_step(
    mut out: Vec<Vec<u8>>,
    state: &mut ResponsesToInteractionsStreamState,
    model_name: &str,
    root: &Value,
) -> Vec<Vec<u8>> {
    if state.active_step_open && state.active_step_type == "function_call" {
        return out;
    }
    let item = root.get("item").unwrap_or(root);
    let id = first_non_empty(&[
        string(item.get("call_id")),
        string(item.get("id")),
        string(root.get("call_id")),
        string(root.get("item_id")),
    ]);
    let step = json!({"type":"function_call","name":string(item.get("name")),"id":id,"call_id":id,"arguments":{}});
    out = ensure_interactions_created(out, state, model_name);
    out = append_interactions_step_stop(out, state);
    append_interactions_step_start(out, state, "function_call", Some(&step))
}

fn append_interactions_completed(
    mut out: Vec<Vec<u8>>,
    state: &mut ResponsesToInteractionsStreamState,
    model_name: &str,
    response: &Value,
) -> Vec<Vec<u8>> {
    if state.completed {
        return out;
    }
    state.timestamp = response_timestamp(response, &state.timestamp);
    let usage = response
        .get("usage")
        .map(interactions_usage_from_responses)
        .unwrap_or_else(|| json!({}));
    out.push(emit_interactions_event("interaction.completed", json!({"interaction":{"id":state.id,"status":"completed","usage":usage,"created":state.timestamp,"updated":state.timestamp,"service_tier":"standard","object":"interaction","model":response_model(model_name,response)},"event_type":"interaction.completed"})));
    state.completed = true;
    out
}

fn append_interactions_done(
    mut out: Vec<Vec<u8>>,
    state: &mut ResponsesToInteractionsStreamState,
) -> Vec<Vec<u8>> {
    if !state.done {
        out.push(b"event: done\ndata: [DONE]\n\n".to_vec());
        state.done = true;
    }
    out
}

fn emit_interactions_event(event: &str, payload: Value) -> Vec<u8> {
    format!(
        "event: {event}\ndata: {}\n\n",
        serde_json::to_string(&payload).expect("JSON event serialization cannot fail")
    )
    .into_bytes()
}

fn text_keys_from_response_event(root: &Value) -> Vec<String> {
    let content = root.get("content_index").and_then(Value::as_i64);
    if content.is_none() {
        response_unkeyed_text_keys(
            &string(root.get("item_id")),
            root.get("output_index").and_then(Value::as_i64),
        )
    } else {
        response_text_keys(
            &string(root.get("item_id")),
            root.get("output_index").and_then(Value::as_i64),
            content,
        )
    }
}

fn response_text_keys(item_id: &str, output: Option<i64>, content: Option<i64>) -> Vec<String> {
    let Some(content) = content else {
        return Vec::new();
    };
    let mut keys = Vec::new();
    if !item_id.is_empty() {
        keys.push(format!("item:{item_id}:content:{content}"));
    }
    if let Some(output) = output {
        keys.push(format!("output:{output}:content:{content}"));
    }
    keys.push(format!("content:{content}"));
    keys
}

fn response_unkeyed_text_keys(item_id: &str, output: Option<i64>) -> Vec<String> {
    let mut keys = Vec::new();
    if !item_id.is_empty() {
        keys.push(format!("item:{item_id}"));
    }
    if let Some(output) = output {
        keys.push(format!("output:{output}"));
    }
    keys
}

fn function_args_keys(root: &Value) -> Vec<String> {
    let item = root.get("item").unwrap_or(&Value::Null);
    let mut keys = Vec::new();
    for id in [
        root.get("item_id"),
        root.get("call_id"),
        item.get("call_id"),
        item.get("id"),
    ]
    .into_iter()
    .map(string)
    .filter(|id| !id.is_empty())
    {
        let key = format!("item:{id}");
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    if let Some(index) = root.get("output_index").and_then(Value::as_i64) {
        keys.push(format!("output:{index}"));
    }
    keys
}

fn mark_text_sent(state: &mut ResponsesToInteractionsStreamState, keys: Vec<String>) {
    if keys.is_empty() {
        state.unkeyed_text_delta = true;
    } else {
        state.sent_text.extend(keys);
    }
}

fn has_sent_text(
    state: &ResponsesToInteractionsStreamState,
    keys: &[String],
    has_content: bool,
) -> bool {
    (!has_content && state.unkeyed_text_delta)
        || keys.iter().any(|key| state.sent_text.contains(key))
}

fn has_sent_unkeyed_text(state: &ResponsesToInteractionsStreamState, keys: &[String]) -> bool {
    if keys.is_empty() {
        state.unkeyed_text_delta
    } else {
        keys.iter().any(|key| state.sent_text.contains(key))
    }
}

fn mark_function_args_sent(state: &mut ResponsesToInteractionsStreamState, keys: Vec<String>) {
    state.function_args_sent.extend(keys);
}

fn has_sent_function_args(state: &ResponsesToInteractionsStreamState, keys: &[String]) -> bool {
    keys.iter()
        .any(|key| state.function_args_sent.contains(key))
}

fn response_timestamp(response: &Value, fallback: &str) -> String {
    for key in ["updated_at", "created_at", "updated", "created"] {
        if let Some(value) = response.get(key) {
            if let Some(timestamp) = value.as_str().filter(|value| !value.is_empty()) {
                return timestamp.to_owned();
            }
            if let Some(seconds) = value.as_i64() {
                return unix_seconds_rfc3339(seconds);
            }
        }
    }
    fallback.to_owned()
}

fn unix_seconds_rfc3339(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400) as u64;
    // Howard Hinnant's civil_from_days algorithm, with Unix epoch offset.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

pub fn convert_interactions_response_to_openai_responses_stream(
    context: &TranslationContext,
    model_name: &str,
    _original_request: &[u8],
    _request: &[u8],
    raw_json: &[u8],
    state: &mut TranslationState,
) -> Vec<Vec<u8>> {
    if context.is_cancelled() {
        return Vec::new();
    }
    let state = interactions_stream_state(state);
    let payload = interactions_sse_payload(raw_json);
    if payload.is_empty() {
        return Vec::new();
    }
    if payload.trim() == "[DONE]" {
        return emit_done_once(state);
    }
    let Ok(root) = serde_json::from_str::<Value>(&payload) else {
        return Vec::new();
    };
    match root.get("event_type").and_then(Value::as_str).unwrap_or("") {
        "interaction.created" => vec![responses_created_event(model_name, &root, state)],
        "step.start" => interactions_step_start_to_responses(&root, state),
        "step.delta" => interactions_step_delta_to_responses(&root, state),
        "step.stop" => interactions_step_stop_to_responses(&root, state),
        "interaction.completed" | "finish" => {
            vec![responses_completed_event(model_name, &root, state)]
        }
        "done" => emit_done_once(state),
        _ => Vec::new(),
    }
}

fn interactions_stream_state(
    state: &mut TranslationState,
) -> &mut InteractionsToResponsesStreamState {
    let replace = state
        .as_ref()
        .is_none_or(|value| !value.is::<InteractionsToResponsesStreamState>());
    if replace {
        *state = Some(Box::new(InteractionsToResponsesStreamState::default()));
    }
    state
        .as_mut()
        .and_then(|value| value.downcast_mut::<InteractionsToResponsesStreamState>())
        .expect("Interactions stream state was initialized with the expected type")
}

fn emit_done_once(state: &mut InteractionsToResponsesStreamState) -> Vec<Vec<u8>> {
    if state.done {
        Vec::new()
    } else {
        state.done = true;
        vec![b"data: [DONE]".to_vec()]
    }
}

fn responses_created_event(
    model_name: &str,
    root: &Value,
    state: &mut InteractionsToResponsesStreamState,
) -> Vec<u8> {
    let id = first_non_empty(&[
        string(get_path(root, &["interaction", "id"])),
        string(root.get("id")),
    ]);
    let sequence = next_sequence(state);
    emit_responses_event(
        "response.created",
        json!({
            "type":"response.created",
            "response":{"id":id,"object":"response","status":"in_progress","model":model_name},
            "sequence_number":sequence,
        }),
    )
}

fn interactions_step_start_to_responses(
    root: &Value,
    state: &mut InteractionsToResponsesStreamState,
) -> Vec<Vec<u8>> {
    let index = root.get("index").and_then(Value::as_i64).unwrap_or(0);
    let step = root.get("step").unwrap_or(&Value::Null);
    let step_type = string(step.get("type"));
    let item_id = first_non_empty(&[
        string(step.get("id")),
        string(step.get("call_id")),
        format!("item_{index}"),
    ]);
    state.item_ids.insert(index, item_id.clone());
    state.item_types.insert(index, step_type.clone());
    match step_type.as_str() {
        "model_output" => {
            let added_sequence = next_sequence(state);
            let part_sequence = next_sequence(state);
            vec![
                emit_responses_event(
                    "response.output_item.added",
                    json!({"type":"response.output_item.added","output_index":index,"item":{"id":item_id,"type":"message","status":"in_progress","role":"assistant","content":[]},"sequence_number":added_sequence}),
                ),
                emit_responses_event(
                    "response.content_part.added",
                    json!({"type":"response.content_part.added","output_index":index,"content_index":0,"item_id":item_id,"part":{"type":"output_text","text":""},"sequence_number":part_sequence}),
                ),
            ]
        }
        "thought" => {
            let encrypted = state
                .reasoning_encrypted
                .get(&index)
                .cloned()
                .unwrap_or_default();
            // ref: interactions_openai_responses_response.go:425-437 @ d7914afd
            let sequence = next_sequence(state);
            let part_sequence = next_sequence(state);
            vec![
                emit_responses_event(
                    "response.output_item.added",
                    json!({"type":"response.output_item.added","output_index":index,"item":{"id":item_id,"type":"reasoning","status":"in_progress","encrypted_content":encrypted,"summary":[]},"sequence_number":sequence}),
                ),
                emit_responses_event(
                    "response.reasoning_summary_part.added",
                    json!({"type":"response.reasoning_summary_part.added","item_id":item_id,"output_index":index,"summary_index":0,"part":{"type":"summary_text","text":""},"sequence_number":part_sequence}),
                ),
            ]
        }
        "function_call" => {
            if state.function_calls.contains_key(&index) {
                return Vec::new();
            }
            let arguments = step
                .get("arguments")
                .filter(|value| !value.is_null())
                .map(|value| json_string(value, "{}"))
                .filter(|value| value.trim() != "{}")
                .unwrap_or_default();
            let name = string(step.get("name"));
            state.function_calls.insert(
                index,
                InteractionsFunctionCallState {
                    name: name.clone(),
                    arguments: arguments.clone(),
                    ..InteractionsFunctionCallState::default()
                },
            );
            let sequence = next_sequence(state);
            let mut events = vec![emit_responses_event(
                "response.output_item.added",
                json!({"type":"response.output_item.added","output_index":index,"item":{"id":item_id,"type":"function_call","call_id":item_id,"name":name,"arguments":""},"sequence_number":sequence}),
            )];
            if !arguments.is_empty() {
                events.push(function_arguments_delta(index, &item_id, &arguments, state));
                if let Some(call) = state.function_calls.get_mut(&index) {
                    call.initial_arguments_emitted = true;
                }
            }
            events
        }
        _ => Vec::new(),
    }
}

fn interactions_step_delta_to_responses(
    root: &Value,
    state: &mut InteractionsToResponsesStreamState,
) -> Vec<Vec<u8>> {
    let index = root.get("index").and_then(Value::as_i64).unwrap_or(0);
    let delta = root.get("delta").unwrap_or(&Value::Null);
    match delta.get("type").and_then(Value::as_str).unwrap_or("") {
        "thought_summary" => {
            let text = first_non_empty(&[
                string(get_path(delta, &["content", "text"])),
                string(delta.get("text")),
            ]);
            if !text.is_empty() {
                state
                    .reasoning_summaries
                    .entry(index)
                    .or_default()
                    .push(text.clone());
            }
            let item_id = state.item_ids.get(&index).cloned().unwrap_or_default();
            let sequence = next_sequence(state);
            vec![emit_responses_event(
                "response.reasoning_summary_text.delta",
                json!({"type":"response.reasoning_summary_text.delta","item_id":item_id,"output_index":index,"summary_index":0,"delta":text,"sequence_number":sequence}),
            )]
        }
        "thought_signature" => {
            if let Some(signature) = delta.get("signature").and_then(Value::as_str) {
                if !signature.is_empty() {
                    state.reasoning_encrypted.insert(index, signature.into());
                }
            }
            Vec::new()
        }
        "arguments_delta" => {
            let arguments = string(delta.get("arguments"));
            if state
                .function_calls
                .get(&index)
                .is_some_and(|call| call.item_done_emitted)
            {
                return Vec::new();
            }
            if let Some(call) = state.function_calls.get_mut(&index) {
                call.arguments.push_str(&arguments);
            }
            let item_id = state.item_ids.get(&index).cloned().unwrap_or_default();
            vec![function_arguments_delta(index, &item_id, &arguments, state)]
        }
        _ => {
            let text = string(delta.get("text"));
            if !text.is_empty() {
                state.text_outputs.entry(index).or_default().push_str(&text);
            }
            let item_id = state.item_ids.get(&index).cloned().unwrap_or_default();
            let sequence = next_sequence(state);
            vec![emit_responses_event(
                "response.output_text.delta",
                json!({"type":"response.output_text.delta","output_index":index,"content_index":0,"item_id":item_id,"delta":text,"sequence_number":sequence}),
            )]
        }
    }
}

fn function_arguments_delta(
    index: i64,
    item_id: &str,
    arguments: &str,
    state: &mut InteractionsToResponsesStreamState,
) -> Vec<u8> {
    let sequence = next_sequence(state);
    emit_responses_event(
        "response.function_call_arguments.delta",
        json!({"type":"response.function_call_arguments.delta","output_index":index,"item_id":item_id,"delta":arguments,"sequence_number":sequence}),
    )
}

fn function_arguments_done(
    index: i64,
    item_id: &str,
    arguments: &str,
    state: &mut InteractionsToResponsesStreamState,
) -> Vec<u8> {
    let sequence = next_sequence(state);
    emit_responses_event(
        "response.function_call_arguments.done",
        json!({"type":"response.function_call_arguments.done","output_index":index,"item_id":item_id,"arguments":arguments,"sequence_number":sequence}),
    )
}

fn interactions_step_stop_to_responses(
    root: &Value,
    state: &mut InteractionsToResponsesStreamState,
) -> Vec<Vec<u8>> {
    let index = root.get("index").and_then(Value::as_i64).unwrap_or(0);
    let item_id = state.item_ids.get(&index).cloned().unwrap_or_default();
    match state
        .item_types
        .get(&index)
        .map(String::as_str)
        .unwrap_or("")
    {
        "model_output" => {
            let text = state.text_outputs.get(&index).cloned().unwrap_or_default();
            let text_sequence = next_sequence(state);
            let part_sequence = next_sequence(state);
            let item_sequence = next_sequence(state);
            vec![
                emit_responses_event(
                    "response.output_text.done",
                    json!({"type":"response.output_text.done","output_index":index,"content_index":0,"item_id":item_id,"text":text,"logprobs":[],"sequence_number":text_sequence}),
                ),
                emit_responses_event(
                    "response.content_part.done",
                    json!({"type":"response.content_part.done","output_index":index,"content_index":0,"item_id":item_id,"part":{"type":"output_text","text":text},"sequence_number":part_sequence}),
                ),
                emit_responses_event(
                    "response.output_item.done",
                    json!({"type":"response.output_item.done","output_index":index,"item":{"id":item_id,"type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":text}]},"sequence_number":item_sequence}),
                ),
            ]
        }
        "function_call" => {
            let (arguments, name, already_done, arguments_done) = state
                .function_calls
                .get(&index)
                .map(|call| {
                    (
                        if call.arguments.is_empty() {
                            "{}".into()
                        } else {
                            call.arguments.clone()
                        },
                        call.name.clone(),
                        call.item_done_emitted,
                        call.arguments_done_emitted,
                    )
                })
                .unwrap_or_else(|| ("{}".into(), String::new(), false, false));
            if already_done {
                return Vec::new();
            }
            let mut events = Vec::new();
            if !arguments_done {
                events.push(function_arguments_done(index, &item_id, &arguments, state));
            }
            let sequence = next_sequence(state);
            events.push(emit_responses_event(
                "response.output_item.done",
                json!({"type":"response.output_item.done","output_index":index,"item":{"id":item_id,"type":"function_call","call_id":item_id,"name":name,"arguments":arguments},"sequence_number":sequence}),
            ));
            if let Some(call) = state.function_calls.get_mut(&index) {
                call.arguments_done_emitted = true;
                call.item_done_emitted = true;
            }
            events
        }
        "thought" => {
            // ref: interactions_openai_responses_response.go:685-711 @ d7914afd
            let text = state
                .reasoning_summaries
                .get(&index)
                .map(|parts| parts.concat())
                .unwrap_or_default();
            let text_sequence = next_sequence(state);
            let part_sequence = next_sequence(state);
            let item_sequence = next_sequence(state);
            let item = responses_reasoning_item(index, state);
            vec![
                emit_responses_event(
                    "response.reasoning_summary_text.done",
                    json!({"type":"response.reasoning_summary_text.done","item_id":item_id,"output_index":index,"summary_index":0,"text":text,"sequence_number":text_sequence}),
                ),
                emit_responses_event(
                    "response.reasoning_summary_part.done",
                    json!({"type":"response.reasoning_summary_part.done","item_id":item_id,"output_index":index,"summary_index":0,"part":{"type":"summary_text","text":text},"sequence_number":part_sequence}),
                ),
                emit_responses_event(
                    "response.output_item.done",
                    json!({"type":"response.output_item.done","output_index":index,"item":item,"sequence_number":item_sequence}),
                ),
            ]
        }
        _ => {
            let item = responses_reasoning_item(index, state);
            let sequence = next_sequence(state);
            vec![emit_responses_event(
                "response.output_item.done",
                json!({"type":"response.output_item.done","output_index":index,"item":item,"sequence_number":sequence}),
            )]
        }
    }
}

fn responses_completed_event(
    model_name: &str,
    root: &Value,
    state: &mut InteractionsToResponsesStreamState,
) -> Vec<u8> {
    let interaction = root.get("interaction").unwrap_or(&Value::Null);
    let id = first_non_empty(&[string(interaction.get("id")), string(root.get("id"))]);
    let model = first_non_empty(&[string(interaction.get("model")), model_name.into()]);
    let output = completed_output(state);
    let usage = interactions_usage(root)
        .map(responses_usage_from_interactions)
        .unwrap_or_else(|| json!({}));
    let sequence = next_sequence(state);
    emit_responses_event(
        "response.completed",
        json!({"type":"response.completed","response":{"id":id,"object":"response","status":"completed","model":model,"output":output,"usage":usage},"sequence_number":sequence}),
    )
}

fn completed_output(state: &InteractionsToResponsesStreamState) -> Vec<Value> {
    let Some(max_index) = state.item_types.keys().max().copied() else {
        return Vec::new();
    };
    (0..=max_index)
        .filter_map(|index| match state.item_types.get(&index).map(String::as_str) {
            Some("model_output") => {
                let text = state.text_outputs.get(&index).cloned().unwrap_or_default();
                let content = if text.is_empty() { Vec::new() } else { vec![json!({"type":"output_text","text":text})] };
                Some(json!({"id":state.item_ids.get(&index).cloned().unwrap_or_default(),"type":"message","status":"completed","role":"assistant","content":content}))
            }
            Some("thought") => Some(responses_reasoning_item(index, state)),
            Some("function_call") => {
                let call = state.function_calls.get(&index);
                let arguments = call.map(|call| if call.arguments.is_empty() {"{}".into()} else {call.arguments.clone()}).unwrap_or_else(|| "{}".into());
                Some(json!({"id":state.item_ids.get(&index).cloned().unwrap_or_default(),"type":"function_call","call_id":state.item_ids.get(&index).cloned().unwrap_or_default(),"name":call.map(|call| call.name.clone()).unwrap_or_default(),"arguments":arguments}))
            }
            _ => None,
        })
        .collect()
}

// ref: interactions_openai_responses_response.go:926-940 @ d7914afd
fn responses_reasoning_item(index: i64, state: &InteractionsToResponsesStreamState) -> Value {
    let text = state
        .reasoning_summaries
        .get(&index)
        .map(|parts| parts.concat())
        .unwrap_or_default();
    json!({
        "id":state.item_ids.get(&index).cloned().unwrap_or_default(),
        "type":"reasoning",
        "status":"completed",
        "encrypted_content":state.reasoning_encrypted.get(&index).cloned().unwrap_or_default(),
        "summary":[{"type":"summary_text","text":text}],
    })
}

fn next_sequence(state: &mut InteractionsToResponsesStreamState) -> i64 {
    state.sequence += 1;
    state.sequence
}

fn emit_responses_event(event: &str, payload: Value) -> Vec<u8> {
    format!(
        "event: {event}\ndata: {}\n\n",
        serde_json::to_string(&payload).expect("JSON event serialization cannot fail")
    )
    .into_bytes()
}

fn interactions_sse_payload(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "[DONE]" {
        return trimmed.into();
    }
    if trimmed.starts_with("data:") && !trimmed.contains('\n') {
        return trimmed[5..].trim().into();
    }
    let lines: Vec<_> = trimmed
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("data:").map(str::trim))
        .collect();
    if lines.is_empty() {
        trimmed.into()
    } else {
        lines.join("\n")
    }
}

fn json_string(value: &Value, fallback: &str) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => fallback.into(),
        value => serde_json::to_string(value).unwrap_or_else(|_| fallback.into()),
    }
}

/// Stateless Interactions response -> OpenAI Responses response conversion.
pub fn convert_interactions_response_to_openai_responses_non_stream(
    _context: &TranslationContext,
    model_name: &str,
    _original_request: &[u8],
    _request: &[u8],
    raw_json: &[u8],
    _state: &mut TranslationState,
) -> Vec<u8> {
    let root = parse(raw_json);
    let id = first_non_empty(&[
        string(root.get("id")),
        string(get_path(&root, &["interaction", "id"])),
    ]);
    let model = response_model(model_name, &root);
    let steps = root
        .get("steps")
        .or_else(|| get_path(&root, &["interaction", "steps"]))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(interactions_step_to_responses_output)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut out = json!({
        "id":id,
        "object":"response",
        "status":"completed",
        "model":model,
        "output":steps,
    });
    if let Some(usage) = interactions_usage(&root) {
        out["usage"] = responses_usage_from_interactions(usage);
    }
    encode(out)
}

/// Stateless OpenAI Responses response -> Interactions response conversion.
pub fn convert_openai_responses_response_to_interactions_non_stream(
    _context: &TranslationContext,
    model_name: &str,
    _original_request: &[u8],
    _request: &[u8],
    raw_json: &[u8],
    _state: &mut TranslationState,
) -> Vec<u8> {
    let root = parse(raw_json);
    let steps = root
        .get("output")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(openai_responses_output_item_to_interactions_step)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut out = json!({
        "id":string(root.get("id")),
        "object":"interaction",
        "status":"completed",
        "model":response_model(model_name, &root),
        "steps":steps,
    });
    if let Some(usage) = root.get("usage") {
        out["usage"] = interactions_usage_from_responses(usage);
    }
    encode(out)
}

fn interactions_step_to_responses_output(step: &Value) -> Option<Value> {
    match step.get("type").and_then(Value::as_str).unwrap_or("") {
        "model_output" => {
            let content = match step.get("content") {
                Some(Value::String(text)) => vec![json!({"type":"output_text", "text":text})],
                Some(Value::Array(parts)) => parts
                    .iter()
                    .filter_map(|part| interactions_output_part_to_responses(part, "assistant"))
                    .collect(),
                _ => Vec::new(),
            };
            let mut item = json!({"type":"message", "role":"assistant", "content":content});
            let id = first_non_empty(&[string(step.get("id")), string(step.get("step_id"))]);
            if !id.is_empty() {
                item["id"] = Value::String(id);
            }
            Some(item)
        }
        "thought" => {
            let summary: Vec<_> = interactions_content_texts(step.get("content"))
                .into_iter()
                .map(|text| json!({"type":"summary_text", "text":text}))
                .collect();
            let mut item = json!({"type":"reasoning", "summary":summary});
            if let Some(signature) = interactions_thought_signature(step) {
                item["encrypted_content"] = Value::String(signature);
            }
            Some(item)
        }
        "function_call" => Some(interactions_function_call_to_responses(step)),
        _ => None,
    }
}

fn interactions_output_part_to_responses(part: &Value, role: &str) -> Option<Value> {
    let kind = part
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| part.get("text").map(|_| "text"))?;
    match kind {
        "text" => Some(json!({"type":"output_text", "text":string(part.get("text"))})),
        "image" => {
            let direct = first_non_empty(&[string(part.get("image_url")), string(part.get("url"))]);
            let url = if direct.is_empty() {
                let data = string(part.get("data"));
                if data.is_empty() {
                    String::new()
                } else {
                    let mime = first_non_empty(&[
                        string(part.get("mime_type")),
                        "application/octet-stream".into(),
                    ]);
                    format!("data:{mime};base64,{data}")
                }
            } else {
                direct
            };
            Some(
                json!({"type":if role == "assistant" {"output_image"} else {"input_image"}, "image_url":url}),
            )
        }
        "audio" => {
            let mime = string(part.get("mime_type"));
            let format = mime
                .split_once('/')
                .map_or(mime.as_str(), |(_, value)| value);
            Some(
                json!({"type":"output_text", "text":format!("Audio content: inline data (Format: {})", if format.is_empty() {"unknown"} else {format})}),
            )
        }
        "video" | "document" => {
            let data = string(part.get("data"));
            let mime = first_non_empty(&[
                string(part.get("mime_type")),
                "application/octet-stream".into(),
            ]);
            let mut item = json!({"type":"output_file"});
            if !data.is_empty() {
                item["file_data"] = Value::String(format!("data:{mime};base64,{data}"));
            }
            if let Some(filename) = part.get("filename").and_then(Value::as_str) {
                item["filename"] = Value::String(filename.into());
            }
            Some(item)
        }
        _ => None,
    }
}

fn openai_responses_output_item_to_interactions_step(item: &Value) -> Option<Value> {
    match item.get("type").and_then(Value::as_str).unwrap_or("") {
        "message" => {
            let content = item
                .get("content")
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(responses_content_part_to_interactions)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(json!({"type":"model_output", "content":content}))
        }
        "function_call" => {
            let call_id = first_non_empty(&[string(item.get("call_id")), string(item.get("id"))]);
            let mut step = json!({
                "type":"function_call",
                "name":string(item.get("name")),
                "arguments":json_value(item.get("arguments"), json!({})),
            });
            if !call_id.is_empty() {
                step["call_id"] = Value::String(call_id);
            }
            Some(step)
        }
        "reasoning" => {
            let content = item
                .get("summary")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|summary| summary.get("text").and_then(Value::as_str))
                        .filter(|text| !text.is_empty())
                        .map(|text| json!({"type":"text", "text":text}))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(json!({"type":"thought", "content":content}))
        }
        _ => None,
    }
}

fn interactions_thought_signature(step: &Value) -> Option<String> {
    let paths: &[&[&str]] = &[
        &["encrypted_content"],
        &["signature"],
        &["thought_signature"],
        &["thoughtSignature"],
        &["extra_content", "google", "thought_signature"],
    ];
    paths
        .iter()
        .find_map(|path| get_path(step, path).and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            step.get("content")
                .and_then(Value::as_array)
                .and_then(|parts| {
                    parts.iter().find_map(|part| {
                        paths
                            .iter()
                            .skip(1)
                            .find_map(|path| get_path(part, path).and_then(Value::as_str))
                            .filter(|value| !value.is_empty())
                            .map(str::to_owned)
                    })
                })
        })
}

fn interactions_usage(root: &Value) -> Option<&Value> {
    [
        &["interaction", "usage"][..],
        &["usage"][..],
        &["metadata", "total_usage"][..],
        &["metadata", "usage"][..],
        &["interaction", "metadata", "total_usage"][..],
        &["interaction", "metadata", "usage"][..],
    ]
    .into_iter()
    .find_map(|path| get_path(root, path))
}

fn responses_usage_from_interactions(usage: &Value) -> Value {
    let mut out = Map::new();
    copy_first_int(
        &mut out,
        "input_tokens",
        usage,
        &["input_tokens", "total_input_tokens"],
    );
    copy_first_int(
        &mut out,
        "output_tokens",
        usage,
        &["output_tokens", "total_output_tokens"],
    );
    copy_first_int(&mut out, "total_tokens", usage, &["total_tokens"]);
    if let Some(value) = first_int(usage, &["cached_tokens", "total_cached_tokens"]) {
        out.insert(
            "input_tokens_details".into(),
            json!({"cached_tokens":value}),
        );
    }
    if let Some(value) = first_int(usage, &["reasoning_tokens", "total_thought_tokens"]) {
        out.insert(
            "output_tokens_details".into(),
            json!({"reasoning_tokens":value}),
        );
    }
    Value::Object(out)
}

fn interactions_usage_from_responses(usage: &Value) -> Value {
    let mut out = Map::new();
    if let Some(value) = usage.get("input_tokens").and_then(Value::as_i64) {
        out.insert("input_tokens".into(), value.into());
        out.insert("total_input_tokens".into(), value.into());
    }
    if let Some(value) = usage.get("output_tokens").and_then(Value::as_i64) {
        out.insert("output_tokens".into(), value.into());
        out.insert("total_output_tokens".into(), value.into());
    }
    if let Some(value) = usage.get("total_tokens").and_then(Value::as_i64) {
        out.insert("total_tokens".into(), value.into());
    }
    if let Some(value) =
        get_path(usage, &["input_tokens_details", "cached_tokens"]).and_then(Value::as_i64)
    {
        out.insert("cached_tokens".into(), value.into());
        out.insert("total_cached_tokens".into(), value.into());
    }
    if let Some(value) =
        get_path(usage, &["output_tokens_details", "reasoning_tokens"]).and_then(Value::as_i64)
    {
        out.insert("reasoning_tokens".into(), value.into());
        out.insert("total_thought_tokens".into(), value.into());
    }
    Value::Object(out)
}

fn response_model(model_name: &str, root: &Value) -> String {
    first_non_empty(&[
        model_name.into(),
        string(root.get("model")),
        string(get_path(root, &["response", "model"])),
        string(get_path(root, &["interaction", "model"])),
    ])
}

fn parse(raw: &[u8]) -> Value {
    serde_json::from_slice(raw).unwrap_or_else(|_| Value::Object(Map::new()))
}

fn encode(value: Value) -> Vec<u8> {
    serde_json::to_vec(&value).expect("serde_json::Value serialization cannot fail")
}

fn get_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |current, key| current.get(key))
}

fn string(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or("").into()
}

fn first_non_empty(values: &[String]) -> String {
    values
        .iter()
        .find(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

fn json_value(value: Option<&Value>, fallback: Value) -> Value {
    match value {
        None => fallback,
        Some(Value::String(text)) => {
            serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        Some(value) => value.clone(),
    }
}

fn first_int(root: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| root.get(key).and_then(Value::as_i64))
}

fn copy_first_int(out: &mut Map<String, Value>, to: &str, root: &Value, keys: &[&str]) {
    if let Some(value) = first_int(root, keys) {
        out.insert(to.into(), value.into());
    }
}
