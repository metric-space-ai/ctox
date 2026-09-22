//! Bounded native consumer of the existing rxdb.query.fetch/chunk contract.
//! The caller supplies an already authenticated connection; no HTTP or policy fallback.
use super::{
    protocol_contract_generated::{
        CTOX_QUERY_DEFAULT_WINDOW_LIMIT, CTOX_QUERY_MAX_BYTES_PER_CHUNK,
        CTOX_QUERY_MAX_IN_FLIGHT_STREAMS, CTOX_QUERY_MAX_RUNTIME_MS, CTOX_QUERY_RPC_CANCEL,
        CTOX_QUERY_RPC_CHUNK, CTOX_QUERY_RPC_ERROR, CTOX_QUERY_RPC_FETCH,
    },
    query_fetch_handler::{QueryFetchChunk, QueryFetchRequest},
    RxWebRTCReplicationPool, WebRTCConnectionHandler, WebRTCMessage, WebRTCWireFrame,
};
use crate::rx_error::{new_rx_error, RxError};
use base64::Engine;
use futures::StreamExt;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

/// One UI page, bounded independently of the number of pages in a collection.
/// Larger data must use smaller windows/projection or the existing file contract.
pub const MAX_NATIVE_QUERY_PAGE_BYTES: usize =
    CTOX_QUERY_MAX_BYTES_PER_CHUNK as usize * CTOX_QUERY_MAX_IN_FLIGHT_STREAMS as usize;

#[derive(Debug)]
pub struct QueryPage {
    pub documents: Vec<Value>,
    /// Opaque existing wire hint, not a consensus checkpoint or ownership proof.
    pub authoritative_revision: Option<String>,
}

fn failure(reason: &str) -> RxError {
    new_rx_error(
        "RC_WEBRTC_QUERY",
        Some(json!({"code":"native_query_failed", "reason":reason})),
    )
}

struct CancelOnDrop<H: WebRTCConnectionHandler + 'static> {
    pool: Arc<RxWebRTCReplicationPool<H>>,
    peer: H::Peer,
    id: String,
    armed: bool,
}
impl<H: WebRTCConnectionHandler + 'static> Drop for CancelOnDrop<H> {
    fn drop(&mut self) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        if !self.armed || !self.pool.connection_handler.is_peer_current(&self.peer) {
            return;
        }
        let handler = self.pool.connection_handler.clone();
        let peer = self.peer.clone();
        let id = self.id.clone();
        // Pool cancellation owns and joins this work. It cannot outlive the
        // session as a detached network task or target a replacement connection.
        self.pool.spawn_auxiliary_tracked(async move {
            if handler.is_peer_current(&peer) {
                let _ = tokio::time::timeout(
                    Duration::from_secs(1),
                    handler.send(
                        &peer,
                        WebRTCWireFrame::Message(WebRTCMessage {
                            id: format!("{id}|cancel"),
                            method: CTOX_QUERY_RPC_CANCEL.into(),
                            params: vec![json!({"requestId":id})],
                            collection: None,
                        }),
                    ),
                )
                .await;
            }
        });
    }
}

fn decode_documents(chunk: &mut QueryFetchChunk) -> Result<Vec<Value>, RxError> {
    let limit = CTOX_QUERY_MAX_BYTES_PER_CHUNK as usize;
    match (chunk.compressed.as_deref(), chunk.compressed_base64.take()) {
        (None, None) => {
            if serde_json::to_vec(&chunk.documents)
                .map_err(|_| failure("invalid_documents"))?
                .len()
                > limit
            {
                return Err(failure("chunk_too_large"));
            }
            Ok(std::mem::take(&mut chunk.documents))
        }
        (Some("deflate"), Some(encoded)) if chunk.documents.is_empty() => {
            if encoded.len() > 4 * limit.div_ceil(3) {
                return Err(failure("chunk_too_large"));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| failure("invalid_compression"))?;
            let mut decoded = vec![0; limit + 1];
            let mut decoder = flate2::Decompress::new(false);
            let status = decoder
                .decompress(&bytes, &mut decoded, flate2::FlushDecompress::Finish)
                .map_err(|_| failure("invalid_compression"))?;
            if decoder.total_out() > limit as u64 {
                return Err(failure("chunk_too_large"));
            }
            if status != flate2::Status::StreamEnd || decoder.total_in() != bytes.len() as u64 {
                return Err(failure("invalid_compression"));
            }
            decoded.truncate(decoder.total_out() as usize);
            serde_json::from_slice(&decoded).map_err(|_| failure("invalid_documents"))
        }
        _ => Err(failure("invalid_compression")),
    }
}

/// Request IDs are generated here so concurrent callers cannot consume each
/// other's chunks. An ack alone is never a completed page; partial results are
/// discarded on timeout, cancellation, sequence gaps or a retired connection.
pub async fn fetch_query_page<H: WebRTCConnectionHandler + 'static>(
    pool: Arc<RxWebRTCReplicationPool<H>>,
    peer: H::Peer,
    mut request: QueryFetchRequest,
) -> Result<QueryPage, RxError> {
    let limit = request
        .window
        .get("limit")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0 && *n <= CTOX_QUERY_DEFAULT_WINDOW_LIMIT as u64)
        .ok_or_else(|| failure("page_limit_required"))? as usize;
    if !pool.is_peer_ready_for_control(&peer) {
        return Err(failure("peer_not_ready"));
    }
    let _permit = pool
        .native_query_semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| failure("local_query_limit"))?;
    request.request_id = format!("native-query-{}", uuid::Uuid::new_v4());
    let id = request.request_id.clone();
    let mut messages = pool.connection_handler.message_stream();
    let mut responses = pool.connection_handler.response_stream();
    let mut disconnected = pool.connection_handler.disconnect_stream();
    let mut cancel = CancelOnDrop {
        pool: pool.clone(),
        peer: peer.clone(),
        id: id.clone(),
        armed: true,
    };
    let exchange = async {
        pool.connection_handler
            .send(
                &peer,
                WebRTCWireFrame::Message(WebRTCMessage {
                    id: id.clone(),
                    method: CTOX_QUERY_RPC_FETCH.into(),
                    params: vec![
                        serde_json::to_value(&request).map_err(|_| failure("invalid_request"))?
                    ],
                    collection: Some(request.collection_name.clone()),
                }),
            )
            .await?;
        let mut accepted = false;
        let mut complete = false;
        let mut sequence = 0;
        let mut total_bytes = 0;
        let mut page = QueryPage {
            documents: Vec::new(),
            authoritative_revision: None,
        };
        loop {
            if !pool.is_peer_ready_for_control(&peer) {
                return Err(failure("peer_not_ready"));
            }
            if accepted && complete {
                return Ok(page);
            }
            tokio::select! {
                _ = pool.cancelled() => return Err(failure("query_pool_closed")),
                item = responses.next() => {
                    let item = item.ok_or_else(|| failure("response_stream_closed"))?;
                    if item.peer != peer || item.response.id != id { continue; }
                    if accepted || item.response.error.is_some()
                        || item.response.result.get("accepted") != Some(&Value::Bool(true))
                        || item.response.result.get("requestId").and_then(Value::as_str) != Some(id.as_str()) {
                        return Err(failure("query_not_accepted"));
                    }
                    accepted = true;
                }
                item = messages.next() => {
                    let item = item.ok_or_else(|| failure("message_stream_closed"))?;
                    if item.peer != peer || !matches!(item.message.method.as_str(), CTOX_QUERY_RPC_CHUNK | CTOX_QUERY_RPC_ERROR) { continue; }
                    let Some(payload) = item.message.params.first() else { continue; };
                    if payload.get("requestId").and_then(Value::as_str) != Some(id.as_str()) { continue; }
                    if item.message.method == CTOX_QUERY_RPC_ERROR { return Err(failure("remote_query_error")); }
                    if complete { return Err(failure("chunk_after_completion")); }
                    let mut chunk: QueryFetchChunk = serde_json::from_value(payload.clone()).map_err(|_| failure("invalid_chunk"))?;
                    if chunk.sequence != sequence { return Err(failure("chunk_sequence_gap")); }
                    sequence += 1;
                    if chunk.cancelled == Some(true) { return Err(failure("query_cancelled")); }
                    let documents = decode_documents(&mut chunk)?;
                    total_bytes += serde_json::to_vec(&documents).map_err(|_| failure("invalid_documents"))?.len();
                    if total_bytes > MAX_NATIVE_QUERY_PAGE_BYTES || page.documents.len() + documents.len() > limit {
                        return Err(failure("page_too_large"));
                    }
                    page.documents.extend(documents);
                    complete = chunk.complete;
                    if complete { page.authoritative_revision = chunk.authoritative_revision; }
                }
                gone = disconnected.next() => {
                    let gone = gone.ok_or_else(|| failure("disconnect_stream_closed"))?;
                    if gone == peer { return Err(failure("peer_disconnected")); }
                }
            }
        }
    };
    let result = tokio::time::timeout(
        Duration::from_millis(CTOX_QUERY_MAX_RUNTIME_MS as u64 + 1000),
        exchange,
    )
    .await
    .map_err(|_| failure("query_timeout"))?;
    if result.is_ok() {
        cancel.armed = false;
    }
    result
}
