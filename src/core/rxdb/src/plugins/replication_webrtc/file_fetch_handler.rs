//! Server-push dispatcher for V1.5 `rxdb.file.fetch`.
//!
//! Mirrors `query_fetch_handler` but for binary file streams. Files are
//! materialized as a series of `rxdb.file.chunk` messages each carrying a
//! base64-encoded byte segment. Range fetching is supported so a browser
//! that already has chunks 0..49 can resume from 50.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::rx_error::{new_rx_error, RxError, RxResult};

use super::protocol_contract_generated::{
    CTOX_FILE_MAX_BYTES_PER_CHUNK, CTOX_FILE_RPC_CANCEL, CTOX_FILE_RPC_CHUNK, CTOX_FILE_RPC_ERROR,
    CTOX_FILE_RPC_FETCH, CTOX_QUERY_MAX_RUNTIME_MS,
};
use super::webrtc_types::{
    WebRTCConnectionHandler, WebRTCMessage, WebRTCResponse, WebRTCWireFrame,
    WEBRTC_BUFFERED_HIGH_WATER,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileFetchRequest {
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    #[serde(rename = "fileId")]
    pub file_id: String,
    #[serde(default)]
    pub range: Option<FileRange>,
    /// Sequences the client already has; the server skips these.
    #[serde(rename = "knownSequences", default)]
    pub known_sequences: Vec<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileRange {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFetchChunk {
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub sequence: u32,
    #[serde(rename = "bytesBase64")]
    pub bytes_base64: String,
    pub hash: Option<String>,
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled: Option<bool>,
}

pub const FILE_FETCH_ERROR_NOT_FOUND: &str = "FILE_NOT_FOUND";
pub const FILE_FETCH_ERROR_UNAUTHORIZED: &str = "UNAUTHORIZED";
pub const FILE_FETCH_ERROR_RATE_LIMITED: &str = "RATE_LIMITED";
pub const FILE_FETCH_ERROR_FEATURE_DISABLED: &str = "FEATURE_DISABLED";
pub const FILE_FETCH_ERROR_REMOTE_TIMEOUT: &str = "REMOTE_TIMEOUT";

/// Source-of-bytes callback. The business-os layer registers one per
/// supported collection; given a (collection, fileId, optional range) it
/// produces the raw bytes to stream.
pub type FileSourceFn = dyn Fn(&str, &str, Option<&FileRange>) -> RxResult<Vec<u8>> + Send + Sync;

pub struct FileFetchRegistry {
    sources: Mutex<HashMap<String, Arc<FileSourceFn>>>,
    inflight: Mutex<HashMap<String, Arc<AtomicBool>>>,
    inflight_count: AtomicU64,
    max_inflight: u64,
    feature_enabled: AtomicBool,
}

impl FileFetchRegistry {
    pub fn new(max_inflight: u64) -> Self {
        Self {
            sources: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            inflight_count: AtomicU64::new(0),
            max_inflight,
            feature_enabled: AtomicBool::new(true),
        }
    }

    pub fn register_source(&self, collection: &str, source: Arc<FileSourceFn>) {
        self.sources.lock().insert(collection.to_string(), source);
    }

    pub fn set_feature_enabled(&self, enabled: bool) {
        self.feature_enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn is_feature_enabled(&self) -> bool {
        self.feature_enabled.load(Ordering::SeqCst)
    }

    fn get_source(&self, collection: &str) -> Option<Arc<FileSourceFn>> {
        self.sources.lock().get(collection).cloned()
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        if let Some(flag) = self.inflight.lock().get(request_id) {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    fn try_acquire(&self, request_id: &str) -> Option<Arc<AtomicBool>> {
        if self.inflight_count.load(Ordering::SeqCst) >= self.max_inflight {
            return None;
        }
        self.inflight_count.fetch_add(1, Ordering::SeqCst);
        let flag = Arc::new(AtomicBool::new(false));
        self.inflight.lock().insert(request_id.to_string(), Arc::clone(&flag));
        Some(flag)
    }

    fn release(&self, request_id: &str) {
        self.inflight.lock().remove(request_id);
        self.inflight_count.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn parse_file_fetch_request(message: &WebRTCMessage) -> RxResult<FileFetchRequest> {
    let first = message.params.first().cloned().unwrap_or(Value::Null);
    serde_json::from_value(first).map_err(|err| {
        new_rx_error(
            "FILE_FETCH_PARSE",
            Some(json!({ "message": format!("invalid rxdb.file.fetch payload: {err}") })),
        )
    })
}

pub fn parse_file_cancel_request(message: &WebRTCMessage) -> RxResult<String> {
    message
        .params
        .first()
        .and_then(|v| v.get("requestId").and_then(Value::as_str))
        .map(str::to_owned)
        .ok_or_else(|| {
            new_rx_error(
                "FILE_CANCEL_PARSE",
                Some(json!({ "message": "rxdb.file.cancel requires requestId" })),
            )
        })
}

pub async fn run_file_fetch<H: WebRTCConnectionHandler>(
    registry: Arc<FileFetchRegistry>,
    handler: Arc<H>,
    peer: H::Peer,
    peer_identity: String,
    message: WebRTCMessage,
) -> RxResult<()> {
    tracing::info!(peer = %peer_identity, msg_id = %message.id, "rxdb.file.fetch begin");

    if !registry.is_feature_enabled() {
        let request_id = parse_file_fetch_request(&message)
            .map(|r| r.request_id)
            .unwrap_or_else(|_| "unknown".to_string());
        send_file_error(
            handler.as_ref(),
            &peer,
            &message.id,
            &request_id,
            FILE_FETCH_ERROR_FEATURE_DISABLED,
            "V1.5 file fetch is disabled on this peer",
            false,
        )
        .await;
        return Ok(());
    }

    let request = match parse_file_fetch_request(&message) {
        Ok(r) => r,
        Err(err) => {
            let ack = WebRTCResponse {
                id: message.id.clone(),
                result: Value::Null,
                error: Some(err.to_string()),
            };
            let _ = handler.send(&peer, WebRTCWireFrame::Response(ack)).await;
            return Err(err);
        }
    };

    let source = match registry.get_source(&request.collection_name) {
        Some(s) => s,
        None => {
            send_file_error(
                handler.as_ref(),
                &peer,
                &message.id,
                &request.request_id,
                FILE_FETCH_ERROR_NOT_FOUND,
                "no file source registered for this collection",
                false,
            )
            .await;
            return Ok(());
        }
    };

    let cancel_flag = match registry.try_acquire(&request.request_id) {
        Some(f) => f,
        None => {
            send_file_error(
                handler.as_ref(),
                &peer,
                &message.id,
                &request.request_id,
                FILE_FETCH_ERROR_RATE_LIMITED,
                "max in-flight file streams reached",
                true,
            )
            .await;
            return Ok(());
        }
    };

    let ack = WebRTCResponse {
        id: message.id.clone(),
        result: json!({ "accepted": true, "requestId": request.request_id }),
        error: None,
    };
    let _ = handler.send(&peer, WebRTCWireFrame::Response(ack)).await;

    let outcome = stream_file(handler.as_ref(), &peer, &request, &source, &cancel_flag).await;
    registry.release(&request.request_id);
    outcome
}

async fn stream_file<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    request: &FileFetchRequest,
    source: &Arc<FileSourceFn>,
    cancel_flag: &Arc<AtomicBool>,
) -> RxResult<()> {
    let runtime_deadline = Instant::now() + Duration::from_millis(CTOX_QUERY_MAX_RUNTIME_MS as u64);
    let bytes = match source(&request.collection_name, &request.file_id, request.range.as_ref()) {
        Ok(b) => b,
        Err(err) => {
            send_file_error(
                handler,
                peer,
                "",
                &request.request_id,
                FILE_FETCH_ERROR_NOT_FOUND,
                &format!("file source error: {err}"),
                false,
            )
            .await;
            return Ok(());
        }
    };

    let chunk_size = CTOX_FILE_MAX_BYTES_PER_CHUNK as usize;
    let known: std::collections::HashSet<u32> = request.known_sequences.iter().copied().collect();
    let total_chunks = ((bytes.len() + chunk_size - 1) / chunk_size).max(1) as u32;
    let mut sequence: u32 = 0;
    let mut sent_any = false;

    while (sequence as usize) * chunk_size < bytes.len() {
        if cancel_flag.load(Ordering::SeqCst) {
            send_file_chunk(handler, peer, request, sequence, &[], true, true).await;
            return Ok(());
        }
        if Instant::now() >= runtime_deadline {
            send_file_error(
                handler,
                peer,
                "",
                &request.request_id,
                FILE_FETCH_ERROR_REMOTE_TIMEOUT,
                "file transfer exceeded CTOX_QUERY_MAX_RUNTIME_MS",
                true,
            )
            .await;
            return Ok(());
        }
        let mut backoff_ms = 4u64;
        while handler.buffered_bytes(peer) > WEBRTC_BUFFERED_HIGH_WATER {
            if cancel_flag.load(Ordering::SeqCst) {
                send_file_chunk(handler, peer, request, sequence, &[], true, true).await;
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms = (backoff_ms * 2).min(64);
        }

        let start = (sequence as usize) * chunk_size;
        let end = (start + chunk_size).min(bytes.len());
        let complete = end >= bytes.len();
        if !known.contains(&sequence) {
            send_file_chunk(handler, peer, request, sequence, &bytes[start..end], complete, false).await;
            sent_any = true;
        }
        sequence += 1;
    }
    if !sent_any {
        // Browser already has every chunk; still send an empty terminal so it
        // can resolve its promise.
        send_file_chunk(handler, peer, request, total_chunks.saturating_sub(1), &[], true, false).await;
    }
    Ok(())
}

async fn send_file_chunk<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    request: &FileFetchRequest,
    sequence: u32,
    bytes: &[u8],
    complete: bool,
    cancelled: bool,
) {
    let hash = sha256_hex(bytes);
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let frame = WebRTCMessage {
        id: format!("{}-f{}", request.request_id, sequence),
        method: CTOX_FILE_RPC_CHUNK.to_string(),
        params: vec![serde_json::to_value(FileFetchChunk {
            request_id: request.request_id.clone(),
            sequence,
            bytes_base64: b64,
            hash: Some(hash),
            complete,
            cancelled: if cancelled { Some(true) } else { None },
        })
        .unwrap_or(Value::Null)],
    };
    let _ = handler.send(peer, WebRTCWireFrame::Message(frame)).await;
}

async fn send_file_error<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    ack_id: &str,
    request_id: &str,
    code: &str,
    message: &str,
    retryable: bool,
) {
    if !ack_id.is_empty() {
        let ack = WebRTCResponse {
            id: ack_id.to_string(),
            result: Value::Null,
            error: Some(format!("{code}: {message}")),
        };
        let _ = handler.send(peer, WebRTCWireFrame::Response(ack)).await;
    }
    let frame = WebRTCMessage {
        id: format!("{}-error", request_id),
        method: CTOX_FILE_RPC_ERROR.to_string(),
        params: vec![json!({
            "requestId": request_id,
            "code": code,
            "message": message,
            "retryable": retryable,
        })],
    };
    let _ = handler.send(peer, WebRTCWireFrame::Message(frame)).await;
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{:02x}", byte);
    }
    hex
}

pub fn file_fetch_method() -> &'static str {
    CTOX_FILE_RPC_FETCH
}
pub fn file_cancel_method() -> &'static str {
    CTOX_FILE_RPC_CANCEL
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rxjs_compat::{RxStream, RxSubject};
    use async_trait::async_trait;
    use parking_lot::Mutex as TokioMutex;
    use std::sync::atomic::AtomicUsize;

    use super::super::webrtc_types::{PeerWithMessage, PeerWithResponse};

    #[derive(Clone, Default, Debug)]
    struct MockPeer(&'static str);
    impl PartialEq for MockPeer { fn eq(&self, other: &Self) -> bool { self.0 == other.0 } }
    impl Eq for MockPeer {}
    impl std::hash::Hash for MockPeer {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) { self.0.hash(state) }
    }

    struct MockHandler { sent: Arc<TokioMutex<Vec<WebRTCWireFrame>>>, buffered: Arc<AtomicUsize> }
    impl MockHandler {
        fn new() -> Self { Self { sent: Arc::new(TokioMutex::new(Vec::new())), buffered: Arc::new(AtomicUsize::new(0)) } }
    }
    #[async_trait]
    impl WebRTCConnectionHandler for MockHandler {
        type Peer = MockPeer;
        fn connect_stream(&self) -> RxStream<Self::Peer> { RxSubject::<Self::Peer>::new().subscribe() }
        fn disconnect_stream(&self) -> RxStream<Self::Peer> { RxSubject::<Self::Peer>::new().subscribe() }
        fn message_stream(&self) -> RxStream<PeerWithMessage<Self::Peer>> { RxSubject::<PeerWithMessage<Self::Peer>>::new().subscribe() }
        fn response_stream(&self) -> RxStream<PeerWithResponse<Self::Peer>> { RxSubject::<PeerWithResponse<Self::Peer>>::new().subscribe() }
        fn error_stream(&self) -> RxStream<RxError> { RxSubject::<RxError>::new().subscribe() }
        async fn send(&self, _peer: &Self::Peer, frame: WebRTCWireFrame) -> Result<(), RxError> {
            self.sent.lock().push(frame); Ok(())
        }
        async fn close(&self) -> Result<(), RxError> { Ok(()) }
        fn buffered_bytes(&self, _peer: &Self::Peer) -> usize { self.buffered.load(Ordering::SeqCst) }
        fn peer_identity(&self, peer: &Self::Peer) -> String { peer.0.to_string() }
    }

    fn make_request(id: &str, collection: &str, file_id: &str, known: Vec<u32>) -> WebRTCMessage {
        WebRTCMessage {
            id: format!("msg-{id}"),
            method: CTOX_FILE_RPC_FETCH.to_string(),
            params: vec![json!({
                "requestId": id,
                "collectionName": collection,
                "fileId": file_id,
                "knownSequences": known,
            })],
        }
    }

    #[tokio::test]
    async fn streams_file_as_chunks_with_hash() {
        let registry = Arc::new(FileFetchRegistry::new(4));
        registry.register_source(
            "desktop_files",
            Arc::new(|_c, _f, _r| Ok(vec![42u8; 800_000])), // ~800 KB → multiple chunks
        );
        let handler = Arc::new(MockHandler::new());
        run_file_fetch(registry, Arc::clone(&handler), MockPeer("p1"), "p1".into(),
            make_request("f1", "desktop_files", "file-1", vec![]))
            .await.unwrap();
        let frames = handler.sent.lock();
        let chunks: Vec<_> = frames.iter().filter_map(|f| match f {
            WebRTCWireFrame::Message(m) if m.method == CTOX_FILE_RPC_CHUNK =>
                serde_json::from_value::<FileFetchChunk>(m.params[0].clone()).ok(),
            _ => None,
        }).collect();
        assert!(chunks.len() >= 3, "800 KB at 256 KB/chunk → ≥3 (got {})", chunks.len());
        assert!(chunks.last().unwrap().complete);
        assert!(chunks.iter().all(|c| c.hash.is_some()), "all chunks must carry hash");
    }

    #[tokio::test]
    async fn known_sequences_skipped() {
        let registry = Arc::new(FileFetchRegistry::new(4));
        registry.register_source(
            "desktop_files",
            Arc::new(|_c, _f, _r| Ok(vec![1u8; 800_000])),
        );
        let handler = Arc::new(MockHandler::new());
        run_file_fetch(registry, Arc::clone(&handler), MockPeer("p1"), "p1".into(),
            make_request("f2", "desktop_files", "file-1", vec![0, 1])) // pretend client has seq 0+1
            .await.unwrap();
        let frames = handler.sent.lock();
        let chunks: Vec<_> = frames.iter().filter_map(|f| match f {
            WebRTCWireFrame::Message(m) if m.method == CTOX_FILE_RPC_CHUNK =>
                serde_json::from_value::<FileFetchChunk>(m.params[0].clone()).ok(),
            _ => None,
        }).collect();
        assert!(chunks.iter().all(|c| c.sequence >= 2), "seq 0,1 must be skipped");
    }

    #[tokio::test]
    async fn unregistered_collection_returns_not_found() {
        let registry = Arc::new(FileFetchRegistry::new(4));
        let handler = Arc::new(MockHandler::new());
        run_file_fetch(registry, Arc::clone(&handler), MockPeer("p1"), "p1".into(),
            make_request("f3", "no_such", "x", vec![])).await.unwrap();
        let frames = handler.sent.lock();
        assert!(frames.iter().any(|f| matches!(
            f, WebRTCWireFrame::Message(m) if m.method == CTOX_FILE_RPC_ERROR
                && m.params[0].get("code").and_then(Value::as_str) == Some(FILE_FETCH_ERROR_NOT_FOUND)
        )));
    }
}
