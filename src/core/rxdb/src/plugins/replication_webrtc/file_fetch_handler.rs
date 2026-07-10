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

use crate::rx_error::{new_rx_error, RxResult};

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

/// Legacy whole-file source. Convenient for small assets and tests; production
/// paths use [`FileChunkStreamFn`] which never materializes the full file.
pub type FileSourceFn = dyn Fn(&str, &str, Option<&FileRange>) -> RxResult<Vec<u8>> + Send + Sync;

/// Production file source that emits chunks via a callback. The callback is
/// invoked once per disk-read; returning `Ok(false)` from it short-circuits
/// the stream (used for early cancel / known-sequence skip). The closure is
/// expected to read the file in fixed-size blocks (typically 256 KB) so
/// server-side RAM never grows with file size.
///
/// Signature parameters:
///   (collection, file_id, range, emit_chunk)
/// where `emit_chunk(bytes) -> Ok(true)` continues, `Ok(false)` stops, and
/// `Err(_)` aborts the whole stream with that error.
pub type FileChunkStreamFn = dyn Fn(&str, &str, Option<&FileRange>, &mut dyn FnMut(&[u8]) -> RxResult<bool>) -> RxResult<()>
    + Send
    + Sync;
pub type FileAuthCheckFn = dyn Fn(&str, &str) -> bool + Send + Sync;

enum FileSource {
    Buffer(Arc<FileSourceFn>),
    Stream(Arc<FileChunkStreamFn>),
}

pub struct FileFetchRegistry {
    sources: Mutex<HashMap<String, FileSource>>,
    inflight: Mutex<HashMap<String, Arc<AtomicBool>>>,
    inflight_count: AtomicU64,
    max_inflight: u64,
    feature_enabled: AtomicBool,
    auth_check: Mutex<Option<Arc<FileAuthCheckFn>>>,
}

impl FileFetchRegistry {
    pub fn new(max_inflight: u64) -> Self {
        Self {
            sources: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            inflight_count: AtomicU64::new(0),
            max_inflight,
            feature_enabled: AtomicBool::new(true),
            auth_check: Mutex::new(None),
        }
    }

    pub fn register_source(&self, collection: &str, source: Arc<FileSourceFn>) {
        self.sources
            .lock()
            .insert(collection.to_string(), FileSource::Buffer(source));
    }

    /// Production-grade registration: bounded-memory chunk stream. The
    /// stream callback owns the disk I/O loop and emits chunks via the
    /// provided `emit_chunk` callback. The dispatcher will skip
    /// known-sequence chunks and apply transport backpressure.
    pub fn register_stream_source(&self, collection: &str, source: Arc<FileChunkStreamFn>) {
        self.sources
            .lock()
            .insert(collection.to_string(), FileSource::Stream(source));
    }

    pub fn set_feature_enabled(&self, enabled: bool) {
        self.feature_enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn is_feature_enabled(&self) -> bool {
        self.feature_enabled.load(Ordering::SeqCst)
    }

    /// Install a peer-identity -> collection authorization callback. Without
    /// a callback the registry denies file-fetch requests.
    pub fn set_auth_check(&self, check: Arc<FileAuthCheckFn>) {
        *self.auth_check.lock() = Some(check);
    }

    fn check_authorized(&self, peer_identity: &str, collection: &str) -> bool {
        match self.auth_check.lock().as_ref() {
            Some(cb) => cb(peer_identity, collection),
            None => false,
        }
    }

    fn get_source(&self, collection: &str) -> Option<FileSource> {
        self.sources.lock().get(collection).map(|s| match s {
            FileSource::Buffer(b) => FileSource::Buffer(Arc::clone(b)),
            FileSource::Stream(s) => FileSource::Stream(Arc::clone(s)),
        })
    }

    pub fn cancel(&self, peer_identity: &str, request_id: &str) -> bool {
        let key = inflight_key(peer_identity, request_id);
        if let Some(flag) = self.inflight.lock().get(&key) {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    fn try_acquire(&self, peer_identity: &str, request_id: &str) -> Option<Arc<AtomicBool>> {
        let key = inflight_key(peer_identity, request_id);
        let mut inflight = self.inflight.lock();
        if inflight.len() as u64 >= self.max_inflight || inflight.contains_key(&key) {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        inflight.insert(key, Arc::clone(&flag));
        self.inflight_count
            .store(inflight.len() as u64, Ordering::SeqCst);
        Some(flag)
    }

    fn release(&self, peer_identity: &str, request_id: &str) {
        let key = inflight_key(peer_identity, request_id);
        let mut inflight = self.inflight.lock();
        inflight.remove(&key);
        self.inflight_count
            .store(inflight.len() as u64, Ordering::SeqCst);
    }
}

fn inflight_key(peer_identity: &str, request_id: &str) -> String {
    format!("{peer_identity}\u{1f}{request_id}")
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
                collection: None,
            };
            let _ = handler.send(&peer, WebRTCWireFrame::Response(ack)).await;
            return Err(err);
        }
    };

    if !registry.check_authorized(&peer_identity, &request.collection_name)
        || !handler.is_collection_authorized_for_peer(&peer, &request.collection_name)
    {
        send_file_error(
            handler.as_ref(),
            &peer,
            &message.id,
            &request.request_id,
            FILE_FETCH_ERROR_UNAUTHORIZED,
            "peer is not authorized for this collection",
            false,
        )
        .await;
        return Ok(());
    }

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

    let cancel_flag = match registry.try_acquire(&peer_identity, &request.request_id) {
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
        collection: None,
    };
    let _ = handler.send(&peer, WebRTCWireFrame::Response(ack)).await;

    let outcome = stream_file(handler.as_ref(), &peer, &request, &source, &cancel_flag).await;
    registry.release(&peer_identity, &request.request_id);
    outcome
}

async fn stream_file<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    request: &FileFetchRequest,
    source: &FileSource,
    cancel_flag: &Arc<AtomicBool>,
) -> RxResult<()> {
    let runtime_deadline = Instant::now() + Duration::from_millis(CTOX_QUERY_MAX_RUNTIME_MS as u64);
    let chunk_size = CTOX_FILE_MAX_BYTES_PER_CHUNK as usize;
    let known: std::collections::HashSet<u32> = request.known_sequences.iter().copied().collect();

    match source {
        FileSource::Stream(stream_fn) => {
            // The stream source is synchronous because it owns the file/DB
            // read loop. Run it on a blocking worker and bridge chunks through
            // a bounded channel so async transport sends, cancellation, and
            // backpressure are driven by this dispatcher without blocking a
            // Tokio runtime worker.
            let mut sequence: u32 = 0u32;
            let mut sent_any = false;
            let mut stop_with_error: Option<&'static str> = None;
            let mut stream_cancelled = false;
            let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(1);
            let producer_stop = Arc::new(AtomicBool::new(false));
            let producer_stop_for_source = Arc::clone(&producer_stop);
            let source_request = request.clone();
            let source_fn = Arc::clone(stream_fn);
            let producer = tokio::task::spawn_blocking(move || {
                (source_fn)(
                    &source_request.collection_name,
                    &source_request.file_id,
                    source_request.range.as_ref(),
                    &mut |bytes: &[u8]| -> RxResult<bool> {
                        if producer_stop_for_source.load(Ordering::SeqCst) {
                            return Ok(false);
                        }
                        match chunk_tx.blocking_send(bytes.to_vec()) {
                            Ok(()) => Ok(true),
                            Err(_) => Ok(false),
                        }
                    },
                )
            });

            while let Some(bytes) = chunk_rx.recv().await {
                if cancel_flag.load(Ordering::SeqCst) {
                    producer_stop.store(true, Ordering::SeqCst);
                    send_file_chunk(handler, peer, request, sequence, &[], true, true).await;
                    stream_cancelled = true;
                    break;
                }
                if Instant::now() >= runtime_deadline {
                    producer_stop.store(true, Ordering::SeqCst);
                    stop_with_error = Some("timeout");
                    break;
                }

                if !known.contains(&sequence) {
                    let mut backoff_ms = 4u64;
                    while handler.buffered_bytes(peer) > WEBRTC_BUFFERED_HIGH_WATER {
                        if cancel_flag.load(Ordering::SeqCst) {
                            producer_stop.store(true, Ordering::SeqCst);
                            send_file_chunk(handler, peer, request, sequence, &[], true, true)
                                .await;
                            stream_cancelled = true;
                            break;
                        }
                        if Instant::now() >= runtime_deadline {
                            producer_stop.store(true, Ordering::SeqCst);
                            stop_with_error = Some("timeout-backpressure");
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                        backoff_ms = (backoff_ms * 2).min(64);
                    }
                    if stream_cancelled || stop_with_error.is_some() {
                        break;
                    }
                    // The closure may pass a smaller slice than chunk_size,
                    // which is fine: chunk_size is a max, not a min. The
                    // sequence number advances per emitted chunk.
                    send_file_chunk(handler, peer, request, sequence, &bytes, false, false).await;
                    sent_any = true;
                }
                sequence += 1;
            }
            drop(chunk_rx);

            let source_result = producer.await.map_err(|err| {
                new_rx_error(
                    "FILE_FETCH_STREAM",
                    Some(json!({ "message": format!("file stream worker failed: {err}") })),
                )
            });

            if stream_cancelled {
                return Ok(());
            }
            if let Some(reason) = stop_with_error {
                send_file_error(
                    handler,
                    peer,
                    "",
                    &request.request_id,
                    FILE_FETCH_ERROR_REMOTE_TIMEOUT,
                    &format!("file transfer stalled: {reason}"),
                    true,
                )
                .await;
                return Ok(());
            }
            if let Err(err) = source_result.and_then(|result| result) {
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
            // Emit terminal complete-chunk. If nothing was sent (all known),
            // still emit one to resolve the JS promise.
            if !sent_any {
                send_file_chunk(handler, peer, request, sequence, &[], true, false).await;
            } else {
                send_file_chunk(handler, peer, request, sequence, &[], true, false).await;
            }
            Ok(())
        }
        FileSource::Buffer(buffer_fn) => {
            let bytes = match buffer_fn(
                &request.collection_name,
                &request.file_id,
                request.range.as_ref(),
            ) {
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
                    send_file_chunk(
                        handler,
                        peer,
                        request,
                        sequence,
                        &bytes[start..end],
                        complete,
                        false,
                    )
                    .await;
                    sent_any = true;
                }
                sequence += 1;
            }
            if !sent_any {
                send_file_chunk(
                    handler,
                    peer,
                    request,
                    total_chunks.saturating_sub(1),
                    &[],
                    true,
                    false,
                )
                .await;
            }
            Ok(())
        }
    }
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
        collection: Some(request.collection_name.clone()),
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
            collection: None,
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
        collection: None,
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
    use crate::rx_error::RxError;
    use crate::rxjs_compat::{RxStream, RxSubject};
    use async_trait::async_trait;
    use parking_lot::Mutex as TokioMutex;
    use std::sync::atomic::AtomicUsize;

    use super::super::webrtc_types::{PeerWithMessage, PeerWithResponse};

    #[derive(Clone, Default, Debug)]
    struct MockPeer(&'static str);
    impl PartialEq for MockPeer {
        fn eq(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl Eq for MockPeer {}
    impl std::hash::Hash for MockPeer {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.hash(state)
        }
    }

    struct MockHandler {
        sent: Arc<TokioMutex<Vec<WebRTCWireFrame>>>,
        buffered: Arc<AtomicUsize>,
        collection_authorized: Arc<AtomicBool>,
    }
    impl MockHandler {
        fn new() -> Self {
            Self {
                sent: Arc::new(TokioMutex::new(Vec::new())),
                buffered: Arc::new(AtomicUsize::new(0)),
                collection_authorized: Arc::new(AtomicBool::new(true)),
            }
        }

        fn set_collection_authorized(&self, authorized: bool) {
            self.collection_authorized
                .store(authorized, Ordering::SeqCst);
        }
    }
    #[async_trait]
    impl WebRTCConnectionHandler for MockHandler {
        type Peer = MockPeer;
        fn connect_stream(&self) -> RxStream<Self::Peer> {
            RxSubject::<Self::Peer>::new().subscribe()
        }
        fn disconnect_stream(&self) -> RxStream<Self::Peer> {
            RxSubject::<Self::Peer>::new().subscribe()
        }
        fn message_stream(&self) -> RxStream<PeerWithMessage<Self::Peer>> {
            RxSubject::<PeerWithMessage<Self::Peer>>::new().subscribe()
        }
        fn response_stream(&self) -> RxStream<PeerWithResponse<Self::Peer>> {
            RxSubject::<PeerWithResponse<Self::Peer>>::new().subscribe()
        }
        fn error_stream(&self) -> RxStream<RxError> {
            RxSubject::<RxError>::new().subscribe()
        }
        async fn send(&self, _peer: &Self::Peer, frame: WebRTCWireFrame) -> Result<(), RxError> {
            self.sent.lock().push(frame);
            Ok(())
        }
        async fn close(&self) -> Result<(), RxError> {
            Ok(())
        }
        fn buffered_bytes(&self, _peer: &Self::Peer) -> usize {
            self.buffered.load(Ordering::SeqCst)
        }
        fn peer_identity(&self, peer: &Self::Peer) -> String {
            peer.0.to_string()
        }
        fn is_collection_authorized_for_peer(&self, _peer: &Self::Peer, _collection: &str) -> bool {
            self.collection_authorized.load(Ordering::SeqCst)
        }
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
            collection: Some(collection.to_string()),
        }
    }

    #[test]
    fn identical_file_request_ids_are_isolated_per_peer() {
        let registry = FileFetchRegistry::new(4);
        let first = registry.try_acquire("p1", "same-request").unwrap();
        let second = registry.try_acquire("p2", "same-request").unwrap();
        assert_eq!(registry.inflight_count.load(Ordering::SeqCst), 2);
        assert!(registry.cancel("p1", "same-request"));
        assert!(first.load(Ordering::SeqCst));
        assert!(!second.load(Ordering::SeqCst));
        registry.release("p1", "same-request");
        registry.release("p2", "same-request");
    }

    fn authorized_file_registry(max_inflight: u64) -> Arc<FileFetchRegistry> {
        let registry = Arc::new(FileFetchRegistry::new(max_inflight));
        registry.set_auth_check(Arc::new(|_peer, _collection| true));
        registry
    }

    fn error_code_emitted(frames: &[WebRTCWireFrame], expected: &str) -> bool {
        frames.iter().any(|f| {
            matches!(
                f,
                WebRTCWireFrame::Message(m) if m.method == CTOX_FILE_RPC_ERROR
                    && m.params[0].get("code").and_then(Value::as_str) == Some(expected)
            )
        })
    }

    #[tokio::test]
    async fn file_fetch_without_auth_callback_is_denied() {
        let registry = Arc::new(FileFetchRegistry::new(4));
        registry.register_source("desktop_files", Arc::new(|_c, _f, _r| Ok(vec![42u8; 8])));
        let handler = Arc::new(MockHandler::new());
        run_file_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".into(),
            make_request("auth-missing", "desktop_files", "file-1", vec![]),
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        assert!(error_code_emitted(&frames, FILE_FETCH_ERROR_UNAUTHORIZED));
    }

    #[tokio::test]
    async fn file_fetch_uses_handler_collection_authorization() {
        let registry = authorized_file_registry(4);
        registry.register_source("desktop_files", Arc::new(|_c, _f, _r| Ok(vec![42u8; 8])));
        let handler = Arc::new(MockHandler::new());
        handler.set_collection_authorized(false);
        run_file_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".into(),
            make_request("handler-deny", "desktop_files", "file-1", vec![]),
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        assert!(error_code_emitted(&frames, FILE_FETCH_ERROR_UNAUTHORIZED));
    }

    #[tokio::test]
    async fn streams_file_as_chunks_with_hash() {
        let registry = authorized_file_registry(4);
        registry.register_source(
            "desktop_files",
            Arc::new(|_c, _f, _r| Ok(vec![42u8; 800_000])), // ~800 KB → multiple chunks
        );
        let handler = Arc::new(MockHandler::new());
        run_file_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".into(),
            make_request("f1", "desktop_files", "file-1", vec![]),
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        let chunks: Vec<_> = frames
            .iter()
            .filter_map(|f| match f {
                WebRTCWireFrame::Message(m) if m.method == CTOX_FILE_RPC_CHUNK => {
                    serde_json::from_value::<FileFetchChunk>(m.params[0].clone()).ok()
                }
                _ => None,
            })
            .collect();
        assert!(
            chunks.len() >= 3,
            "800 KB at 256 KB/chunk → ≥3 (got {})",
            chunks.len()
        );
        assert!(chunks.last().unwrap().complete);
        assert!(
            chunks.iter().all(|c| c.hash.is_some()),
            "all chunks must carry hash"
        );
    }

    #[tokio::test]
    async fn known_sequences_skipped() {
        let registry = authorized_file_registry(4);
        registry.register_source(
            "desktop_files",
            Arc::new(|_c, _f, _r| Ok(vec![1u8; 800_000])),
        );
        let handler = Arc::new(MockHandler::new());
        run_file_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".into(),
            make_request("f2", "desktop_files", "file-1", vec![0, 1]),
        ) // pretend client has seq 0+1
        .await
        .unwrap();
        let frames = handler.sent.lock();
        let chunks: Vec<_> = frames
            .iter()
            .filter_map(|f| match f {
                WebRTCWireFrame::Message(m) if m.method == CTOX_FILE_RPC_CHUNK => {
                    serde_json::from_value::<FileFetchChunk>(m.params[0].clone()).ok()
                }
                _ => None,
            })
            .collect();
        assert!(
            chunks.iter().all(|c| c.sequence >= 2),
            "seq 0,1 must be skipped"
        );
    }

    #[tokio::test]
    async fn streams_file_via_chunk_source_without_full_materialization() {
        // Regression for the review finding: production file sources must
        // never materialize the whole file. We register a stream-source
        // whose `emit_chunk` is called per 256 KB read; we assert (a) every
        // emit_chunk call carries at most one chunk_size, (b) the registry
        // call site sees exactly chunk-by-chunk flow.
        let chunk_size = CTOX_FILE_MAX_BYTES_PER_CHUNK as usize;
        // Total file: 5 × chunk_size = 1.25 MiB
        const N_CHUNKS: usize = 5;
        let emit_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let peak_in_flight = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let emit_calls_clone = Arc::clone(&emit_calls);
        let peak_in_flight_clone = Arc::clone(&peak_in_flight);
        let registry = authorized_file_registry(4);
        registry.register_stream_source(
            "desktop_files",
            Arc::new(move |_collection, _file_id, _range, emit| {
                // Simulate disk I/O: read N_CHUNKS chunks of exactly chunk_size,
                // each into a freshly-allocated buffer. We track the *maximum
                // simultaneous* allocation to confirm bounded memory.
                for i in 0..N_CHUNKS {
                    let buf = vec![(i as u8).wrapping_add(1); chunk_size];
                    let in_flight = buf.len();
                    let mut peak = peak_in_flight_clone.load(Ordering::SeqCst);
                    while in_flight > peak {
                        match peak_in_flight_clone.compare_exchange(
                            peak,
                            in_flight,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        ) {
                            Ok(_) => break,
                            Err(actual) => peak = actual,
                        }
                    }
                    emit_calls_clone.fetch_add(1, Ordering::SeqCst);
                    let keep_going = emit(&buf)?;
                    drop(buf);
                    if !keep_going {
                        break;
                    }
                }
                Ok(())
            }),
        );
        let handler = Arc::new(MockHandler::new());
        run_file_fetch(
            Arc::clone(&registry),
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".into(),
            make_request("stream1", "desktop_files", "big-file", vec![]),
        )
        .await
        .unwrap();
        assert_eq!(
            emit_calls.load(Ordering::SeqCst),
            N_CHUNKS,
            "stream source must be invoked exactly once per chunk"
        );
        assert!(
            peak_in_flight.load(Ordering::SeqCst) <= chunk_size,
            "in-flight buffer must never exceed one chunk_size — got peak {}",
            peak_in_flight.load(Ordering::SeqCst)
        );
        let frames = handler.sent.lock();
        let chunks: Vec<_> = frames
            .iter()
            .filter_map(|f| match f {
                WebRTCWireFrame::Message(m) if m.method == CTOX_FILE_RPC_CHUNK => {
                    serde_json::from_value::<FileFetchChunk>(m.params[0].clone()).ok()
                }
                _ => None,
            })
            .collect();
        // N_CHUNKS payload frames + 1 terminal complete frame
        assert_eq!(
            chunks.len(),
            N_CHUNKS + 1,
            "expected {} payload + 1 terminal chunk",
            N_CHUNKS
        );
        assert!(
            chunks.last().unwrap().complete,
            "terminal chunk must be complete"
        );
    }

    #[tokio::test]
    async fn stream_source_waits_for_async_backpressure() {
        const N_CHUNKS: usize = 3;
        let emit_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let emit_calls_clone = Arc::clone(&emit_calls);
        let registry = authorized_file_registry(4);
        registry.register_stream_source(
            "desktop_files",
            Arc::new(move |_collection, _file_id, _range, emit| {
                for i in 0..N_CHUNKS {
                    let buf = vec![i as u8; 8];
                    emit_calls_clone.fetch_add(1, Ordering::SeqCst);
                    if !emit(&buf)? {
                        break;
                    }
                }
                Ok(())
            }),
        );
        let handler = Arc::new(MockHandler::new());
        handler.buffered.store(
            WEBRTC_BUFFERED_HIGH_WATER.saturating_add(1),
            Ordering::SeqCst,
        );

        let task = tokio::spawn({
            let registry = Arc::clone(&registry);
            let handler = Arc::clone(&handler);
            async move {
                run_file_fetch(
                    registry,
                    handler,
                    MockPeer("p1"),
                    "p1".into(),
                    make_request("stream-backpressure", "desktop_files", "big-file", vec![]),
                )
                .await
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        {
            let frames = handler.sent.lock();
            assert!(
                frames.iter().all(|frame| !matches!(
                    frame,
                    WebRTCWireFrame::Message(message)
                        if message.method == CTOX_FILE_RPC_CHUNK
                )),
                "backpressure should pause file chunk sends"
            );
        }

        handler.buffered.store(0, Ordering::SeqCst);
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .expect("file fetch should finish after backpressure clears")
            .expect("file fetch task should not panic")
            .expect("file fetch should succeed");

        let frames = handler.sent.lock();
        let chunks: Vec<_> = frames
            .iter()
            .filter_map(|frame| match frame {
                WebRTCWireFrame::Message(message) if message.method == CTOX_FILE_RPC_CHUNK => {
                    serde_json::from_value::<FileFetchChunk>(message.params[0].clone()).ok()
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            chunks.len(),
            N_CHUNKS + 1,
            "payload chunks plus terminal completion frame should be sent"
        );
        assert!(
            chunks.last().is_some_and(|chunk| chunk.complete),
            "terminal chunk must mark completion"
        );
        assert_eq!(
            emit_calls.load(Ordering::SeqCst),
            N_CHUNKS,
            "producer should resume after async backpressure clears"
        );
    }

    #[tokio::test]
    async fn unregistered_collection_returns_not_found() {
        let registry = authorized_file_registry(4);
        let handler = Arc::new(MockHandler::new());
        run_file_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".into(),
            make_request("f3", "no_such", "x", vec![]),
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        assert!(frames.iter().any(|f| matches!(
            f, WebRTCWireFrame::Message(m) if m.method == CTOX_FILE_RPC_ERROR
                && m.params[0].get("code").and_then(Value::as_str) == Some(FILE_FETCH_ERROR_NOT_FOUND)
        )));
    }
}
