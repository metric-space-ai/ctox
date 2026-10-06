//! Bounded native consumer of the existing file RPC. Callers own durable range
//! checkpoints and final content identity; successful transport grants no authority.
use super::{
    file_fetch_handler::{FileFetchChunk, FileFetchRequest},
    protocol_contract_generated::{
        CTOX_FILE_MAX_BYTES_PER_CHUNK, CTOX_FILE_RPC_CANCEL, CTOX_FILE_RPC_CHUNK,
        CTOX_FILE_RPC_ERROR, CTOX_FILE_RPC_FETCH, CTOX_QUERY_MAX_RUNTIME_MS,
    },
    RxWebRTCReplicationPool, WebRTCConnectionHandler, WebRTCMessage, WebRTCWireFrame,
};
use crate::rx_error::{new_rx_error, RxError};
use base64::Engine;
use futures::StreamExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};

pub const MAX_NATIVE_FILE_RANGE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct FileRangeBytes {
    pub offset: u64,
    pub bytes: Vec<u8>,
}

fn failure(reason: &str) -> RxError {
    new_rx_error("RC_WEBRTC_FILE", Some(json!({"reason": reason})))
}

struct CancelOnDrop<H: WebRTCConnectionHandler + 'static> {
    pool: Arc<RxWebRTCReplicationPool<H>>,
    peer: H::Peer,
    id: String,
    armed: bool,
}
impl<H: WebRTCConnectionHandler + 'static> Drop for CancelOnDrop<H> {
    fn drop(&mut self) {
        if !self.armed
            || tokio::runtime::Handle::try_current().is_err()
            || !self.pool.connection_handler.is_peer_current(&self.peer)
        {
            return;
        }
        let handler = self.pool.connection_handler.clone();
        let peer = self.peer.clone();
        let id = self.id.clone();
        self.pool.spawn_auxiliary_tracked(async move {
            if handler.is_peer_current(&peer) {
                let _ = tokio::time::timeout(
                    Duration::from_secs(1),
                    handler.send(
                        &peer,
                        WebRTCWireFrame::Message(WebRTCMessage {
                            id: format!("{id}|cancel"),
                            method: CTOX_FILE_RPC_CANCEL.into(),
                            params: vec![json!({"requestId": id})],
                            collection: None,
                        }),
                    ),
                )
                .await;
            }
        });
    }
}

/// Read one exact byte range on an already admitted connection. Every chunk is
/// independently decoded and hashed. Partial results are discarded on any error;
/// the caller can retry this range after reconnect without advancing its checkpoint.
/// The full content hash must still be checked before publishing a durable receipt.
pub async fn fetch_file_range<H: WebRTCConnectionHandler + 'static>(
    pool: Arc<RxWebRTCReplicationPool<H>>,
    peer: H::Peer,
    mut request: FileFetchRequest,
) -> Result<FileRangeBytes, RxError> {
    let range = request
        .range
        .as_ref()
        .ok_or_else(|| failure("bounded_range_required"))?;
    if range.length > MAX_NATIVE_FILE_RANGE_BYTES
        || range.offset.checked_add(range.length).is_none()
        || !request.known_sequences.is_empty()
        || request.collection_name.is_empty()
        || request.collection_name.len() > 256
        || request.file_id.is_empty()
        || request.file_id.len() > 1024
    {
        return Err(failure("invalid_range_request"));
    }
    let (offset, length) = (range.offset, range.length);
    if !pool.is_peer_ready_for_control(&peer) {
        return Err(failure("peer_not_ready"));
    }
    // Share the existing admission budget with query readers: combined native
    // demand readers cannot multiply their per-pool memory/concurrency budget.
    let _permit = pool
        .native_query_semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| failure("local_fetch_limit"))?;
    request.request_id = format!("native-file-{}", uuid::Uuid::new_v4());
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
                    method: CTOX_FILE_RPC_FETCH.into(),
                    params: vec![
                        serde_json::to_value(&request).map_err(|_| failure("invalid_request"))?
                    ],
                    collection: Some(request.collection_name.clone()),
                }),
            )
            .await?;
        let mut accepted = false;
        let mut complete = false;
        let mut sequence = 0u32;
        let mut bytes = Vec::with_capacity(length as usize);
        loop {
            if !pool.is_peer_ready_for_control(&peer) {
                return Err(failure("peer_not_ready"));
            }
            if accepted && complete {
                return Ok(FileRangeBytes { offset, bytes });
            }
            tokio::select! {
                _ = pool.cancelled() => return Err(failure("file_pool_closed")),
                item = responses.next() => {
                    let item = item.ok_or_else(|| failure("response_stream_closed"))?;
                    if item.peer != peer || item.response.id != id { continue; }
                    if accepted || item.response.error.is_some()
                        || item.response.result.get("accepted") != Some(&Value::Bool(true))
                        || item.response.result.get("requestId").and_then(Value::as_str) != Some(id.as_str()) {
                        return Err(failure("file_not_accepted"));
                    }
                    accepted = true;
                }
                item = messages.next() => {
                    let item = item.ok_or_else(|| failure("message_stream_closed"))?;
                    if item.peer != peer || !matches!(item.message.method.as_str(), CTOX_FILE_RPC_CHUNK | CTOX_FILE_RPC_ERROR) { continue; }
                    let Some(payload) = item.message.params.first() else { continue; };
                    if payload.get("requestId").and_then(Value::as_str) != Some(id.as_str()) { continue; }
                    if item.message.method == CTOX_FILE_RPC_ERROR { return Err(failure("remote_file_error")); }
                    if complete { return Err(failure("chunk_after_completion")); }
                    let chunk: FileFetchChunk = serde_json::from_value(payload.clone()).map_err(|_| failure("invalid_chunk"))?;
                    if chunk.sequence != sequence { return Err(failure("chunk_sequence_gap")); }
                    sequence = sequence.checked_add(1).ok_or_else(|| failure("chunk_sequence_overflow"))?;
                    if chunk.cancelled == Some(true) { return Err(failure("file_cancelled")); }
                    let limit = CTOX_FILE_MAX_BYTES_PER_CHUNK as usize;
                    if chunk.bytes_base64.len() > 4 * limit.div_ceil(3) { return Err(failure("chunk_too_large")); }
                    let decoded = base64::engine::general_purpose::STANDARD.decode(&chunk.bytes_base64)
                        .map_err(|_| failure("invalid_base64"))?;
                    if decoded.len() > limit || bytes.len() + decoded.len() > length as usize { return Err(failure("range_too_large")); }
                    let hash = format!("{:x}", Sha256::digest(&decoded));
                    if chunk.hash.as_deref() != Some(hash.as_str()) { return Err(failure("chunk_hash_mismatch")); }
                    if chunk.complete && !decoded.is_empty() { return Err(failure("invalid_terminal_chunk")); }
                    if !chunk.complete && decoded.is_empty() { return Err(failure("empty_data_chunk")); }
                    bytes.extend_from_slice(&decoded);
                    complete = chunk.complete;
                    if complete && bytes.len() != length as usize { return Err(failure("range_incomplete")); }
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
    .map_err(|_| failure("file_timeout"))?;
    if result.is_ok() {
        cancel.armed = false;
    }
    result
}
