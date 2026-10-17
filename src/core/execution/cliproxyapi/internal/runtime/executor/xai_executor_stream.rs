// ref: internal/runtime/executor/xai_executor_stream.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::sdk::cliproxy::auth::Auth;
use crate::sdk::cliproxy::executor::{Options, Request};

use super::xai_executor::{xai_status_error, XaiHttpRequest, XaiStreamResponse, XaiUpstreamTarget};
use super::xai_executor_execute::{sanitize_compact_body, XaiExecutionError, XaiExecutor};
use super::xai_executor_request::{apply_xai_chat_headers, xai_chat_base_url, xai_credentials};
use super::xai_executor_response::{normalize_sse_stream, InternalXSearchResponseFilter};
use super::xai_reasoning_replay::{
    apply_reasoning_replay, cache_reasoning_replay_from_completed, XaiReasoningReplayScope,
    XaiReasoningReplayStore,
};

pub struct XaiProcessedStream {
    pub headers: crate::sdk::cliproxy::executor::Headers,
    pub chunks: mpsc::Receiver<Result<Vec<u8>, XaiExecutionError>>,
}

impl XaiExecutor {
    pub async fn execute_stream(
        &self,
        auth: Option<&Auth>,
        request: &Request,
        options: &Options,
    ) -> Result<XaiProcessedStream, XaiExecutionError> {
        if options.alt == "responses/compact" {
            return Err(XaiExecutionError::StreamingCompactUnsupported);
        }
        if xai_input_has_item_type(&request.payload, "compaction_trigger") {
            return self
                .execute_compaction_trigger_stream(auth, request, options)
                .await;
        }
        let credentials = xai_credentials(auth);
        let target = XaiUpstreamTarget::new(&xai_chat_base_url(auth))
            .map_err(|_| XaiExecutionError::InvalidTarget)?;
        let mut prepared = self.prepare_request(request, options, true)?;
        let replay_scope = self.reasoning_replay_scope(options, &credentials.token);
        if let (Some(store), Some(scope)) = (self.replay_store().map(Arc::as_ref), &replay_scope) {
            prepared.body = apply_reasoning_replay(store, Some(scope), &prepared.body);
        }
        let mut headers = crate::sdk::cliproxy::executor::Headers::new();
        apply_xai_chat_headers(
            &mut headers,
            auth,
            &credentials.token,
            true,
            &prepared.session_id,
        );
        let upstream = XaiHttpRequest {
            url: target.url("/responses"),
            headers,
            body: prepared.body.into(),
        };
        let mut response = self
            .stream_transport()?
            .execute_stream(&upstream, self.timeout())
            .await
            .map_err(XaiExecutionError::Transport)?;
        if !(200..300).contains(&response.status) {
            let mut body = Vec::new();
            while let Some(Ok(chunk)) = response.next_chunk().await {
                body.extend(chunk);
            }
            return Err(XaiExecutionError::Status(xai_status_error(
                response.status,
                &body,
            )));
        }
        Ok(process_stream(
            response,
            prepared.filter_internal_x_search,
            prepared.client_declared_tools,
            prepared.namespace_tools,
            self.replay_store().cloned(),
            replay_scope,
        ))
    }

    async fn execute_compaction_trigger_stream(
        &self,
        auth: Option<&Auth>,
        request: &Request,
        options: &Options,
    ) -> Result<XaiProcessedStream, XaiExecutionError> {
        let mut compact_options = options.clone();
        compact_options.alt = "responses/compact".into();
        compact_options.stream = false;
        let response = self.execute(auth, request, &compact_options).await?;

        let mut compact_body = request.payload.clone();
        sanitize_compact_body(&mut compact_body);
        let frames =
            build_xai_compaction_stream_frames(&compact_body, &response.payload, &request.model);
        let mut headers = response.headers;
        headers.insert("Content-Type".into(), vec!["text/event-stream".into()]);
        let (sender, receiver) = mpsc::channel(frames.len().max(1));
        for frame in frames {
            if sender.send(Ok(frame)).await.is_err() {
                break;
            }
        }
        drop(sender);
        Ok(XaiProcessedStream {
            headers,
            chunks: receiver,
        })
    }
}

#[must_use]
pub fn xai_input_has_item_type(body: &[u8], item_type: &str) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("input").and_then(Value::as_array).cloned())
        .is_some_and(|input| {
            input
                .iter()
                .any(|item| item.get("type").and_then(Value::as_str) == Some(item_type))
        })
}

#[must_use]
pub fn build_xai_compaction_stream_frames(
    compact_request_body: &[u8],
    compact_data: &[u8],
    fallback_model: &str,
) -> Vec<Vec<u8>> {
    let compact = serde_json::from_slice::<Value>(compact_data).unwrap_or_else(|_| json!({}));
    let response_id = xai_compaction_response_id(&compact, compact_data);
    let created_at = compact
        .get("created_at")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let completed_at = compact
        .get("completed_at")
        .and_then(Value::as_i64)
        .unwrap_or(created_at);
    let item = xai_compaction_output_item(&compact, &response_id);

    let created_response = xai_compaction_base_response(
        compact_request_body,
        &compact,
        fallback_model,
        &response_id,
        created_at,
        "in_progress",
    );
    let in_progress_response = created_response.clone();
    let mut completed_response = xai_compaction_base_response(
        compact_request_body,
        &compact,
        fallback_model,
        &response_id,
        created_at,
        "completed",
    );
    if let Some(object) = completed_response.as_object_mut() {
        object.insert("completed_at".into(), json!(completed_at));
        object.insert("output".into(), json!([item.clone()]));
        if let Some(usage) = compact.get("usage") {
            object.insert("usage".into(), usage.clone());
        }
    }

    [
        (
            "response.created",
            json!({"type":"response.created","sequence_number":0,"response":created_response}),
        ),
        (
            "response.in_progress",
            json!({"type":"response.in_progress","sequence_number":1,"response":in_progress_response}),
        ),
        (
            "response.output_item.added",
            json!({"type":"response.output_item.added","sequence_number":2,"output_index":0,"item":item.clone()}),
        ),
        (
            "keepalive",
            json!({"type":"keepalive","sequence_number":3}),
        ),
        (
            "response.output_item.done",
            json!({"type":"response.output_item.done","sequence_number":4,"output_index":0,"item":item}),
        ),
        (
            "response.completed",
            json!({"type":"response.completed","sequence_number":5,"response":completed_response}),
        ),
    ]
    .into_iter()
    .map(|(event, payload)| xai_sse_frame(event, &payload))
    .collect()
}

fn xai_compaction_response_id(compact: &Value, raw: &[u8]) -> String {
    if let Some(response_id) = compact
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        if response_id.starts_with("resp_") {
            return response_id.into();
        }
        return format!(
            "resp_{}",
            response_id.strip_prefix("cmp_").unwrap_or(response_id)
        );
    }
    let digest = Sha256::digest(raw);
    let digest = format!("{digest:x}");
    format!("resp_xai_compaction_{}", &digest[..16])
}

fn xai_compaction_output_item(compact: &Value, response_id: &str) -> Value {
    let mut item = compact
        .get("output")
        .and_then(Value::as_array)
        .and_then(|output| output.first())
        .cloned()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({"type":"compaction"}));
    if let Some(object) = item.as_object_mut() {
        object
            .entry("type")
            .or_insert_with(|| Value::String("compaction".into()));
        object.entry("id").or_insert_with(|| {
            Value::String(format!(
                "cmp_{}",
                response_id.strip_prefix("resp_").unwrap_or(response_id)
            ))
        });
    }
    item
}

fn xai_compaction_base_response(
    compact_request_body: &[u8],
    compact: &Value,
    fallback_model: &str,
    response_id: &str,
    created_at: i64,
    status: &str,
) -> Value {
    let model = compact
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty())
        .unwrap_or(fallback_model);
    let mut response = json!({
        "id": response_id,
        "object": "response",
        "created_at": created_at,
        "status": status,
        "background": false,
        "error": null,
        "incomplete_details": null,
        "output": [],
        "model": model,
    });
    let request = serde_json::from_slice::<Value>(compact_request_body).unwrap_or(Value::Null);
    let Some(response_object) = response.as_object_mut() else {
        return response;
    };
    let Some(request_object) = request.as_object() else {
        return response;
    };
    for field in [
        "instructions",
        "max_tool_calls",
        "parallel_tool_calls",
        "previous_response_id",
        "prompt_cache_key",
        "reasoning",
        "text",
        "tool_choice",
        "top_logprobs",
        "truncation",
        "user",
        "metadata",
    ] {
        if let Some(value) = request_object.get(field) {
            response_object.insert(field.into(), value.clone());
        }
    }
    response
}

fn xai_sse_frame(event: &str, payload: &Value) -> Vec<u8> {
    let payload = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    let mut frame = Vec::with_capacity(event.len() + payload.len() + 16);
    frame.extend_from_slice(b"event: ");
    frame.extend_from_slice(event.as_bytes());
    frame.extend_from_slice(b"\ndata: ");
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(b"\n\n");
    frame
}

fn process_stream(
    mut response: XaiStreamResponse,
    enabled: bool,
    declared: std::collections::BTreeSet<super::xai_executor_request::ClientToolKey>,
    refs: BTreeMap<String, super::xai_executor_request::NamespaceToolRef>,
    replay_store: Option<Arc<dyn XaiReasoningReplayStore>>,
    replay_scope: Option<XaiReasoningReplayScope>,
) -> XaiProcessedStream {
    let headers = response.headers.clone();
    let (sender, receiver) = mpsc::channel(32);
    tokio::spawn(async move {
        let mut filter = InternalXSearchResponseFilter::new(enabled, declared);
        while let Some(chunk) = response.next_chunk().await {
            match chunk {
                Ok(chunk) => {
                    for frame in normalize_sse_stream(&chunk, &mut filter, &refs) {
                        if let (Some(store), Some(scope), Some(data)) = (
                            replay_store.as_deref(),
                            replay_scope.as_ref(),
                            completed_sse_data(&frame),
                        ) {
                            cache_reasoning_replay_from_completed(store, Some(scope), data);
                        }
                        if sender.send(Ok(frame)).await.is_err() {
                            return;
                        }
                    }
                }
                Err(error) => {
                    let _ = sender.send(Err(XaiExecutionError::Transport(error))).await;
                    return;
                }
            }
        }
    });
    XaiProcessedStream {
        headers,
        chunks: receiver,
    }
}

fn completed_sse_data(frame: &[u8]) -> Option<&[u8]> {
    let mut event = None;
    let mut data = None;
    for line in frame.split(|byte| *byte == b'\n') {
        if let Some(value) = line.strip_prefix(b"event: ") {
            event = Some(value);
        } else if let Some(value) = line.strip_prefix(b"data: ") {
            data = Some(value);
        }
    }
    if event == Some(b"response.completed".as_slice()) {
        data
    } else {
        None
    }
}
