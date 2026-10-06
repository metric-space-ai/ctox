// ref: internal/translator/codex/claude/codex_claude_response_web_search.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::internal::translator::common::append_sse_event;

pub(super) fn stream_web_search_events(raw: &[u8], next_index: &mut usize) -> Vec<Vec<u8>> {
    let payload = raw.strip_prefix(b"data: ").unwrap_or(raw).trim_ascii();
    let Ok(root) = serde_json::from_slice::<Value>(payload) else {
        return Vec::new();
    };
    if root.get("type").and_then(Value::as_str) != Some("response.output_item.done") {
        return Vec::new();
    }
    let item = root.get("item").unwrap_or(&Value::Null);
    if item.get("type").and_then(Value::as_str) != Some("web_search_call") {
        return Vec::new();
    }
    let id = web_search_id(&root, item).unwrap_or_else(|| format!("web_search_{}", *next_index));
    let query = web_search_query(&root, item);
    let results = web_search_results(&root, item);
    if query.is_none() && results.is_empty() && item.get("action").is_none() {
        return Vec::new();
    }
    let use_index = *next_index;
    *next_index += 1;
    let mut output = vec![event(
        "content_block_start",
        &json!({"type":"content_block_start","index":use_index,"content_block":{"type":"server_tool_use","id":id,"name":"web_search","input":{}}}),
    )];
    if let Some(query) = query {
        output.push(event(
            "content_block_delta",
            &json!({"type":"content_block_delta","index":use_index,"delta":{"type":"input_json_delta","partial_json":json!({"query":query}).to_string()}}),
        ));
    }
    output.push(event(
        "content_block_stop",
        &json!({"type":"content_block_stop","index":use_index}),
    ));
    let result_index = *next_index;
    *next_index += 1;
    output.push(event(
        "content_block_start",
        &json!({"type":"content_block_start","index":result_index,"content_block":{"type":"web_search_tool_result","tool_use_id":id,"content":results}}),
    ));
    output.push(event(
        "content_block_stop",
        &json!({"type":"content_block_stop","index":result_index}),
    ));
    output
}

pub(super) fn append_non_stream_web_search(translated: &[u8], raw: &[u8]) -> Vec<u8> {
    let Ok(mut output) = serde_json::from_slice::<Value>(translated) else {
        return translated.to_vec();
    };
    let Ok(root) = serde_json::from_slice::<Value>(raw) else {
        return translated.to_vec();
    };
    let Some(content) = output.get_mut("content").and_then(Value::as_array_mut) else {
        return translated.to_vec();
    };
    let mut seen = HashSet::new();
    for item in root
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("web_search_call"))
    {
        let Some(id) = web_search_id(&root, item) else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        let query = web_search_query(&root, item);
        let results = web_search_results(&root, item);
        if query.is_none() && results.is_empty() {
            continue;
        }
        content.push(json!({"type":"server_tool_use","id":id,"name":"web_search","input":query.map(|query| json!({"query":query})).unwrap_or_else(|| json!({}))}));
        content.push(json!({"type":"web_search_tool_result","tool_use_id":id,"content":results}));
    }
    serde_json::to_vec(&output).unwrap_or_else(|_| translated.to_vec())
}

fn web_search_id(root: &Value, item: &Value) -> Option<String> {
    ["id", "output_item_id", "call_id", "item_id"]
        .into_iter()
        .find_map(|key| {
            item.get(key)
                .or_else(|| root.get(key))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
}

fn web_search_query(root: &Value, item: &Value) -> Option<String> {
    ["/action/query", "/query", "/input/query"]
        .into_iter()
        .find_map(|path| {
            item.pointer(path)
                .or_else(|| root.pointer(path))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
}

fn web_search_results(root: &Value, item: &Value) -> Vec<Value> {
    // ref: internal/translator/codex/claude/codex_claude_response_web_search.go:130:147 @ a2976eb8a303f11b4ea5177bce9f9ff752634dfc
    [
        item.get("results"),
        root.get("results"),
        item.pointer("/action/sources"),
        root.pointer("/action/sources"),
    ]
    .into_iter()
    .flatten()
    .find_map(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|result| {
        let url = result
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let title = result
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(url);
        Some(json!({"type":"web_search_result","title":title,"url":url,"page_age":Value::Null}))
    })
    .collect()
}

fn event(name: &str, payload: &Value) -> Vec<u8> {
    let payload = serde_json::to_vec(payload).unwrap_or_default();
    let mut out = Vec::new();
    append_sse_event(&mut out, name, &payload, 3);
    out
}
