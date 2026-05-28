//! Server-push dispatcher for V1.5 `rxdb.query.fetch`.
//!
//! Receives one `WebRTCMessage { method: "rxdb.query.fetch", params: [req] }`,
//! validates it against the V1.5 collection registry, executes the query
//! against the bound storage, and emits one or more
//! `WebRTCMessage { method: "rxdb.query.chunk", params: [{ requestId, sequence, ... }] }`
//! frames to the same peer. The acknowledgement `WebRTCResponse` is sent
//! immediately so the browser knows the request was accepted; chunks flow
//! asynchronously.
//!
//! `rxdb.query.cancel` toggles a per-requestId flag; the streaming task
//! observes it between chunks and emits a final chunk with
//! `{ complete: true, cancelled: true }` then stops.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write as _;

/// Chunks below this raw JSON-byte size are not compressed (overhead would
/// dominate). Above this threshold deflate kicks in.
pub const COMPRESSION_THRESHOLD_BYTES: usize = 4 * 1024;

use crate::rx_collection::RxCollection;
use crate::rx_error::{new_rx_error, RxError, RxResult};
use crate::rx_query_helper::{normalize_mango_query, prepare_query};
use crate::types::MangoQuery;

use super::protocol_contract_generated::{
    CTOX_QUERY_MAX_BYTES_PER_CHUNK, CTOX_QUERY_MAX_DOCUMENTS_PER_CHUNK, CTOX_QUERY_MAX_RUNTIME_MS,
    CTOX_QUERY_RPC_CANCEL, CTOX_QUERY_RPC_CHUNK, CTOX_QUERY_RPC_ERROR, CTOX_QUERY_RPC_FETCH,
};
use super::webrtc_types::{
    WebRTCConnectionHandler, WebRTCMessage, WebRTCResponse, WebRTCWireFrame,
    WEBRTC_BUFFERED_HIGH_WATER,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryFetchRequest {
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "databaseName", default)]
    pub database_name: Option<String>,
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    #[serde(rename = "schemaVersion", default)]
    pub schema_version: u32,
    #[serde(rename = "queryFingerprint")]
    pub query_fingerprint: String,
    #[serde(default)]
    pub query: Value,
    #[serde(default)]
    pub window: Value,
    /// V1.5 production hardening: server-side projection. When provided,
    /// each chunked document is reduced to only the listed top-level keys
    /// before being sent. Cuts wire bytes 5-10x for UIs that only need a
    /// handful of fields out of fat business documents.
    #[serde(default)]
    pub projection: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFetchChunk {
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub sequence: u32,
    /// When `compressed` is None or false, `documents` carries the JSON
    /// array directly. When `compressed: "deflate"`, the `compressedBase64`
    /// field carries the same array DEFLATE-compressed + base64-encoded,
    /// and `documents` is omitted to save bytes. Browsers decode via the
    /// native `DecompressionStream("deflate")` API.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<Value>,
    pub complete: bool,
    #[serde(
        rename = "authoritativeRevision",
        skip_serializing_if = "Option::is_none"
    )]
    pub authoritative_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compressed: Option<String>,
    #[serde(
        rename = "compressedBase64",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub compressed_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryFetchError {
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

pub const QUERY_FETCH_ERROR_NOT_SUPPORTED: &str = "QUERY_NOT_SUPPORTED";
pub const QUERY_FETCH_ERROR_SCHEMA_MISMATCH: &str = "SCHEMA_MISMATCH";
pub const QUERY_FETCH_ERROR_STREAM_LIMIT: &str = "STREAM_LIMIT_EXCEEDED";
pub const QUERY_FETCH_ERROR_REMOTE_TIMEOUT: &str = "REMOTE_TIMEOUT";
pub const QUERY_FETCH_ERROR_UNAUTHORIZED: &str = "UNAUTHORIZED";
pub const QUERY_FETCH_ERROR_RATE_LIMITED: &str = "RATE_LIMITED";
pub const QUERY_FETCH_ERROR_FEATURE_DISABLED: &str = "FEATURE_DISABLED";

/// Per-peer rate-limit token bucket. This intentionally does not mirror the
/// concurrent stream limit: a browser startup may legitimately open many
/// short demand-load windows, while only a few streams run at the same time.
const RATE_BUCKET_REFILL_INTERVAL: Duration = Duration::from_secs(1);
const RATE_BUCKET_MIN_BURST: u32 = 32;
const RATE_BUCKET_BURST_MULTIPLIER: u32 = 8;
const RATE_BUCKET_REFILL_PER_SECOND: u32 = 16;

/// Authorization callback. The dispatcher calls this with the peer-identity
/// (opaque string from the connection handler) and the requested collection;
/// returns Ok(true) to allow, Ok(false) to deny. Default registry implementation
/// allows everything for backward compatibility; production wiring overrides this.
pub type AuthCheckFn = dyn Fn(&str, &str) -> bool + Send + Sync;

struct PeerRateBucket {
    last_refill: Instant,
    tokens: u32,
    max_tokens: u32,
    refill_per_second: u32,
}

impl PeerRateBucket {
    fn new(max_tokens: u32, refill_per_second: u32) -> Self {
        Self {
            last_refill: Instant::now(),
            tokens: max_tokens,
            max_tokens,
            refill_per_second: refill_per_second.max(1),
        }
    }

    fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        if elapsed >= RATE_BUCKET_REFILL_INTERVAL {
            let refill = (elapsed.as_secs() as u32)
                .saturating_mul(self.refill_per_second)
                .min(self.max_tokens);
            self.tokens = (self.tokens + refill).min(self.max_tokens);
            self.last_refill = now;
        }
        if self.tokens == 0 {
            false
        } else {
            self.tokens -= 1;
            true
        }
    }
}

/// Registry of V1.5-eligible collections. The business-os layer registers
/// each opted-in collection; everything else is answered with
/// `QUERY_NOT_SUPPORTED`. The default scope is `business_records`,
/// `communication_messages`, `communication_threads`.
pub struct QueryFetchRegistry {
    inner: Mutex<HashMap<String, Arc<RxCollection>>>,
    inflight: Mutex<HashMap<String, Arc<AtomicBool>>>,
    inflight_count: AtomicU64,
    max_inflight: u64,
    peer_rate_buckets: Mutex<HashMap<String, PeerRateBucket>>,
    rate_burst: u32,
    rate_refill_per_second: u32,
    feature_enabled: AtomicBool,
    auth_check: Mutex<Option<Arc<AuthCheckFn>>>,
}

impl Default for QueryFetchRegistry {
    fn default() -> Self {
        Self::new(crate::plugins::replication_webrtc::protocol_contract_generated::CTOX_QUERY_MAX_IN_FLIGHT_STREAMS as u64)
    }
}

impl QueryFetchRegistry {
    pub fn new(max_inflight: u64) -> Self {
        let stream_based_burst = (max_inflight as u32).saturating_mul(RATE_BUCKET_BURST_MULTIPLIER);
        Self {
            inner: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            inflight_count: AtomicU64::new(0),
            max_inflight,
            peer_rate_buckets: Mutex::new(HashMap::new()),
            rate_burst: stream_based_burst.max(RATE_BUCKET_MIN_BURST),
            rate_refill_per_second: RATE_BUCKET_REFILL_PER_SECOND,
            feature_enabled: AtomicBool::new(true),
            auth_check: Mutex::new(None),
        }
    }

    pub fn set_feature_enabled(&self, enabled: bool) {
        self.feature_enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn is_feature_enabled(&self) -> bool {
        self.feature_enabled.load(Ordering::SeqCst)
    }

    /// Install a peer-identity → collection authorization callback. Without
    /// a callback the registry allows any peer to query any registered
    /// collection (legacy behavior). Production wiring MUST set this.
    pub fn set_auth_check(&self, check: Arc<AuthCheckFn>) {
        *self.auth_check.lock() = Some(check);
    }

    fn check_authorized(&self, peer_identity: &str, collection: &str) -> bool {
        match self.auth_check.lock().as_ref() {
            Some(cb) => cb(peer_identity, collection),
            None => true,
        }
    }

    fn try_rate_consume(&self, peer_identity: &str) -> bool {
        let mut map = self.peer_rate_buckets.lock();
        let entry = map
            .entry(peer_identity.to_string())
            .or_insert_with(|| PeerRateBucket::new(self.rate_burst, self.rate_refill_per_second));
        entry.try_consume()
    }

    pub fn register(&self, collection: Arc<RxCollection>) {
        let key = collection_key(&collection.name);
        self.inner.lock().insert(key, collection);
    }

    pub fn get(&self, collection_name: &str) -> Option<Arc<RxCollection>> {
        self.inner
            .lock()
            .get(&collection_key(collection_name))
            .cloned()
    }

    pub fn is_registered(&self, collection_name: &str) -> bool {
        self.inner
            .lock()
            .contains_key(&collection_key(collection_name))
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        if let Some(flag) = self.inflight.lock().get(request_id) {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    pub fn count_inflight(&self) -> u64 {
        self.inflight_count.load(Ordering::SeqCst)
    }

    pub fn max_inflight(&self) -> u64 {
        self.max_inflight
    }

    fn try_acquire(&self, request_id: &str) -> Option<Arc<AtomicBool>> {
        let count = self.inflight_count.load(Ordering::SeqCst);
        if count >= self.max_inflight {
            return None;
        }
        self.inflight_count.fetch_add(1, Ordering::SeqCst);
        let flag = Arc::new(AtomicBool::new(false));
        self.inflight
            .lock()
            .insert(request_id.to_string(), Arc::clone(&flag));
        Some(flag)
    }

    fn release(&self, request_id: &str) {
        self.inflight.lock().remove(request_id);
        self.inflight_count.fetch_sub(1, Ordering::SeqCst);
    }
}

fn collection_key(collection_name: &str) -> String {
    collection_name.to_string()
}

/// Parses a raw `WebRTCMessage` into a typed `QueryFetchRequest`. Returns
/// `Err` if the params shape is wrong.
pub fn parse_query_fetch_request(message: &WebRTCMessage) -> RxResult<QueryFetchRequest> {
    let first = message.params.first().cloned().unwrap_or(Value::Null);
    serde_json::from_value(first).map_err(|err| {
        new_rx_error(
            "QUERY_FETCH_PARSE",
            Some(json!({ "message": format!("invalid rxdb.query.fetch payload: {err}") })),
        )
    })
}

/// Parses a raw `WebRTCMessage` for a cancel request, returning the requestId.
pub fn parse_query_cancel_request(message: &WebRTCMessage) -> RxResult<String> {
    let payload = message.params.first().cloned().unwrap_or(Value::Null);
    payload
        .get("requestId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            new_rx_error(
                "QUERY_CANCEL_PARSE",
                Some(json!({ "message": "rxdb.query.cancel requires requestId" })),
            )
        })
}

/// Sends an immediate acknowledgement and then streams chunks. The caller's
/// task awaits this future; chunks are emitted via `handler.send`. The future
/// resolves when the stream completes (or is cancelled).
///
/// `peer_identity` is the stable identity used for authz + rate-limiting.
/// The connection handler typically derives this from the WebRTC session.
pub async fn run_query_fetch<H: WebRTCConnectionHandler>(
    registry: Arc<QueryFetchRegistry>,
    handler: Arc<H>,
    peer: H::Peer,
    peer_identity: String,
    message: WebRTCMessage,
) -> RxResult<()> {
    tracing::info!(peer = %peer_identity, msg_id = %message.id, "rxdb.query.fetch begin");

    if !registry.is_feature_enabled() {
        let request_id = parse_query_fetch_request(&message)
            .map(|r| r.request_id)
            .unwrap_or_else(|_| "unknown".to_string());
        send_error(
            handler.as_ref(),
            &peer,
            &message.id,
            &request_id,
            QUERY_FETCH_ERROR_FEATURE_DISABLED,
            "V1.5 query fetch is disabled on this peer",
            false,
        )
        .await;
        return Ok(());
    }

    let request = match parse_query_fetch_request(&message) {
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

    if !registry.check_authorized(&peer_identity, &request.collection_name) {
        send_error(
            handler.as_ref(),
            &peer,
            &message.id,
            &request.request_id,
            QUERY_FETCH_ERROR_UNAUTHORIZED,
            "peer is not authorized for this collection",
            false,
        )
        .await;
        return Ok(());
    }

    let collection = match registry.get(&request.collection_name) {
        Some(c) => c,
        None => {
            send_error(
                handler.as_ref(),
                &peer,
                &message.id,
                &request.request_id,
                QUERY_FETCH_ERROR_NOT_SUPPORTED,
                "collection is not V1.5-enabled",
                false,
            )
            .await;
            return Ok(());
        }
    };

    if !registry.try_rate_consume(&peer_identity) {
        send_error(
            handler.as_ref(),
            &peer,
            &message.id,
            &request.request_id,
            QUERY_FETCH_ERROR_RATE_LIMITED,
            "per-peer query-fetch rate limit reached",
            true,
        )
        .await;
        return Ok(());
    }

    let cancel_flag = match registry.try_acquire(&request.request_id) {
        Some(flag) => flag,
        None => {
            send_error(
                handler.as_ref(),
                &peer,
                &message.id,
                &request.request_id,
                QUERY_FETCH_ERROR_STREAM_LIMIT,
                "max in-flight query streams reached",
                true,
            )
            .await;
            return Ok(());
        }
    };

    // Immediate ack for the original request id so the JS-side request map
    // resolves. The browser then awaits chunk frames via its own router.
    let ack = WebRTCResponse {
        id: message.id.clone(),
        result: json!({ "accepted": true, "requestId": request.request_id }),
        error: None,
    };
    let _ = handler.send(&peer, WebRTCWireFrame::Response(ack)).await;

    let outcome = stream_chunks(handler.as_ref(), &peer, &collection, &request, &cancel_flag).await;
    registry.release(&request.request_id);
    outcome
}

async fn stream_chunks<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    collection: &Arc<RxCollection>,
    request: &QueryFetchRequest,
    cancel_flag: &Arc<AtomicBool>,
) -> RxResult<()> {
    let schema = collection
        .schema
        .as_ref()
        .ok_or_else(|| new_rx_error("QU_SCHEMA", Some(json!({ "collection": collection.name }))))?;
    if request.schema_version > 0 && schema.version() != request.schema_version as i32 {
        send_error(
            handler,
            peer,
            "",
            &request.request_id,
            QUERY_FETCH_ERROR_SCHEMA_MISMATCH,
            &format!(
                "server schema version {} != client {}",
                schema.version(),
                request.schema_version
            ),
            false,
        )
        .await;
        return Ok(());
    }

    let mango: MangoQuery = match serde_json::from_value(request.query.clone()) {
        Ok(q) => q,
        Err(err) => {
            send_error(
                handler,
                peer,
                "",
                &request.request_id,
                QUERY_FETCH_ERROR_NOT_SUPPORTED,
                &format!("invalid query payload: {err}"),
                false,
            )
            .await;
            return Ok(());
        }
    };
    let normalized = normalize_mango_query(&schema.json_schema, mango);
    let prepared = prepare_query(&schema.json_schema, normalized)?;

    let max_doc_chunk = CTOX_QUERY_MAX_DOCUMENTS_PER_CHUNK as usize;
    let max_byte_chunk = CTOX_QUERY_MAX_BYTES_PER_CHUNK as usize;
    let runtime_deadline = Instant::now() + Duration::from_millis(CTOX_QUERY_MAX_RUNTIME_MS as u64);
    let revision = request.query_fingerprint.clone();
    let projection: Option<Vec<String>> = request.projection.clone();
    let apply_projection = |doc: Value| -> Value {
        match &projection {
            Some(fields) if !fields.is_empty() => match doc {
                Value::Object(map) => {
                    let mut out = serde_json::Map::new();
                    for field in fields {
                        if let Some(v) = map.get(field) {
                            out.insert(field.clone(), v.clone());
                        }
                    }
                    Value::Object(out)
                }
                other => other,
            },
            _ => doc,
        }
    };

    // Streaming path: pull from the storage instance one cursor-bounded
    // batch at a time, then split each batch into wire chunks that respect
    // BOTH the doc-count and byte-cap. Memory bound on the server side is
    // O(chunk_size × doc_bytes), not O(total_matches).
    //
    // The state we need across the closure invocations:
    //   - sequence: the next wire-chunk sequence number
    //   - pending: a small lookahead buffer so we can decide whether the
    //              current chunk can fit "one more" doc without exceeding
    //              max_byte_chunk
    //   - any_emitted: ensure an empty-result query still gets one terminal
    //                  chunk so the browser-side request resolves
    let mut sequence: u32 = 0;
    let mut pending: Vec<Value> = Vec::new();
    let mut any_emitted = false;
    let cancel_seen = Arc::new(AtomicBool::new(false));
    let timeout_seen = Arc::new(AtomicBool::new(false));

    // Helper closures share state via captures. We collect the wire frames
    // into a queue and send them outside the streaming callback so the
    // SQLite connection lock is released before the async send.
    let mut frames_to_send: Vec<(u32, Vec<Value>, bool)> = Vec::new();

    let cancel_for_cb = Arc::clone(cancel_flag);
    let cancel_seen_for_cb = Arc::clone(&cancel_seen);
    let timeout_seen_for_cb = Arc::clone(&timeout_seen);
    let storage_result = collection
        .storage_instance
        .query_stream_into(&prepared, max_doc_chunk, &mut |batch| {
            if cancel_for_cb.load(Ordering::SeqCst) {
                cancel_seen_for_cb.store(true, Ordering::SeqCst);
                return Ok(false);
            }
            if Instant::now() >= runtime_deadline {
                timeout_seen_for_cb.store(true, Ordering::SeqCst);
                return Ok(false);
            }
            // Apply server-side projection BEFORE chunk-budgeting so byte
            // accounting reflects what actually goes on the wire.
            let projected: Vec<Value> = batch.into_iter().map(&apply_projection).collect();
            pending.extend(projected);
            // Drain `pending` into wire-sized chunks. We hold back the last
            // partial chunk in case the NEXT storage batch can fill it up
            // without exceeding the doc-count or byte-cap. The final flush
            // happens after the streaming callback returns.
            while pending.len() >= max_doc_chunk {
                let mut chunk_docs: Vec<Value> = Vec::new();
                let mut chunk_bytes: usize = 64;
                while let Some(doc) = pending.first() {
                    if chunk_docs.len() >= max_doc_chunk {
                        break;
                    }
                    let doc_bytes = doc.to_string().len();
                    if !chunk_docs.is_empty() && chunk_bytes + doc_bytes > max_byte_chunk {
                        break;
                    }
                    chunk_bytes += doc_bytes + 1;
                    chunk_docs.push(pending.remove(0));
                    if chunk_bytes >= max_byte_chunk {
                        break;
                    }
                }
                if chunk_docs.is_empty() {
                    break;
                }
                frames_to_send.push((sequence, chunk_docs, false));
                sequence += 1;
                any_emitted = true;
            }
            Ok(true)
        })
        .await;
    storage_result?;

    if cancel_seen.load(Ordering::SeqCst) {
        send_chunk(
            handler,
            peer,
            request,
            sequence,
            Vec::new(),
            true,
            &revision,
            true,
        )
        .await;
        return Ok(());
    }
    if timeout_seen.load(Ordering::SeqCst) {
        send_error(
            handler,
            peer,
            "",
            &request.request_id,
            QUERY_FETCH_ERROR_REMOTE_TIMEOUT,
            "query runtime exceeded CTOX_QUERY_MAX_RUNTIME_MS",
            true,
        )
        .await;
        return Ok(());
    }

    // Flush remaining pending docs as the terminal chunk(s).
    while !pending.is_empty() {
        let mut chunk_docs: Vec<Value> = Vec::new();
        let mut chunk_bytes: usize = 64;
        while let Some(doc) = pending.first() {
            if chunk_docs.len() >= max_doc_chunk {
                break;
            }
            let doc_bytes = doc.to_string().len();
            if !chunk_docs.is_empty() && chunk_bytes + doc_bytes > max_byte_chunk {
                break;
            }
            chunk_bytes += doc_bytes + 1;
            chunk_docs.push(pending.remove(0));
            if chunk_bytes >= max_byte_chunk {
                break;
            }
        }
        if chunk_docs.is_empty() {
            break;
        }
        let complete = pending.is_empty();
        frames_to_send.push((sequence, chunk_docs, complete));
        sequence += 1;
        any_emitted = true;
    }
    if !any_emitted {
        // Empty result still emits one terminal chunk so the JS side resolves.
        send_chunk(
            handler,
            peer,
            request,
            0,
            Vec::new(),
            true,
            &revision,
            false,
        )
        .await;
        return Ok(());
    }
    if let Some(last) = frames_to_send.last_mut() {
        last.2 = true;
    }

    // Emit frames with backpressure awareness.
    for (seq, docs, complete) in frames_to_send.into_iter() {
        if cancel_flag.load(Ordering::SeqCst) {
            send_chunk(
                handler,
                peer,
                request,
                seq,
                Vec::new(),
                true,
                &revision,
                true,
            )
            .await;
            return Ok(());
        }
        let mut backoff_ms = 4u64;
        while handler.buffered_bytes(peer) > WEBRTC_BUFFERED_HIGH_WATER {
            if cancel_flag.load(Ordering::SeqCst) {
                send_chunk(
                    handler,
                    peer,
                    request,
                    seq,
                    Vec::new(),
                    true,
                    &revision,
                    true,
                )
                .await;
                return Ok(());
            }
            if Instant::now() >= runtime_deadline {
                send_error(
                    handler,
                    peer,
                    "",
                    &request.request_id,
                    QUERY_FETCH_ERROR_REMOTE_TIMEOUT,
                    "stalled on backpressure beyond CTOX_QUERY_MAX_RUNTIME_MS",
                    true,
                )
                .await;
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms = (backoff_ms * 2).min(64);
        }
        send_chunk(
            handler, peer, request, seq, docs, complete, &revision, false,
        )
        .await;
    }
    Ok(())
}

async fn send_chunk<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    request: &QueryFetchRequest,
    sequence: u32,
    documents: Vec<Value>,
    complete: bool,
    revision: &str,
    cancelled: bool,
) {
    let chunk = build_chunk(
        request.request_id.clone(),
        sequence,
        documents,
        complete,
        Some(revision.to_string()),
        cancelled,
    );
    let frame = WebRTCMessage {
        id: format!("{}-c{}", request.request_id, sequence),
        method: CTOX_QUERY_RPC_CHUNK.to_string(),
        params: vec![serde_json::to_value(chunk).unwrap_or(Value::Null)],
    };
    let _ = handler.send(peer, WebRTCWireFrame::Message(frame)).await;
}

/// Decode a chunk's documents regardless of whether the envelope carries them
/// inline or as deflate+base64. Used by tests and any in-process consumer
/// that wants to verify the wire payload end-to-end.
pub fn decode_chunk_documents(chunk: &QueryFetchChunk) -> RxResult<Vec<Value>> {
    if !chunk.documents.is_empty() {
        return Ok(chunk.documents.clone());
    }
    match chunk.compressed.as_deref() {
        Some("deflate") => {
            use flate2::read::DeflateDecoder;
            use std::io::Read as _;
            let b64 = chunk.compressed_base64.as_deref().unwrap_or("");
            let raw = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|err| {
                    new_rx_error(
                        "CHUNK_DECODE",
                        Some(json!({ "message": format!("invalid base64: {err}") })),
                    )
                })?;
            let mut decoder = DeflateDecoder::new(raw.as_slice());
            let mut out = String::new();
            decoder.read_to_string(&mut out).map_err(|err| {
                new_rx_error(
                    "CHUNK_DECODE",
                    Some(json!({ "message": format!("deflate decode: {err}") })),
                )
            })?;
            serde_json::from_str(&out).map_err(|err| {
                new_rx_error(
                    "CHUNK_DECODE",
                    Some(json!({ "message": format!("inner json: {err}") })),
                )
            })
        }
        Some(other) => Err(new_rx_error(
            "CHUNK_DECODE",
            Some(json!({ "message": format!("unsupported compression: {other}") })),
        )),
        None => Ok(Vec::new()),
    }
}

/// Build a chunk envelope, applying deflate compression when the JSON-array
/// payload exceeds COMPRESSION_THRESHOLD_BYTES. Browsers detect the
/// `compressed: "deflate"` field and decode via DecompressionStream.
pub fn build_chunk(
    request_id: String,
    sequence: u32,
    documents: Vec<Value>,
    complete: bool,
    authoritative_revision: Option<String>,
    cancelled: bool,
) -> QueryFetchChunk {
    let payload = serde_json::to_vec(&documents).unwrap_or_default();
    let mut chunk = QueryFetchChunk {
        request_id,
        sequence,
        documents: Vec::new(),
        complete,
        authoritative_revision,
        cancelled: if cancelled { Some(true) } else { None },
        compressed: None,
        compressed_base64: None,
    };
    if payload.len() >= COMPRESSION_THRESHOLD_BYTES {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        if encoder.write_all(&payload).is_ok() {
            if let Ok(compressed) = encoder.finish() {
                // Only switch to compressed form if it actually saves bytes
                // after base64 encoding (compressed * 4 / 3).
                let b64_len_estimate = (compressed.len() + 2) / 3 * 4;
                if b64_len_estimate < payload.len() {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&compressed);
                    chunk.compressed = Some("deflate".to_string());
                    chunk.compressed_base64 = Some(b64);
                    return chunk;
                }
            }
        }
    }
    chunk.documents = documents;
    chunk
}

async fn send_error<H: WebRTCConnectionHandler>(
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
        method: CTOX_QUERY_RPC_ERROR.to_string(),
        params: vec![serde_json::to_value(QueryFetchError {
            request_id: request_id.to_string(),
            code: code.to_string(),
            message: message.to_string(),
            retryable,
        })
        .unwrap_or(Value::Null)],
    };
    let _ = handler.send(peer, WebRTCWireFrame::Message(frame)).await;
}

/// Convenience: returns `true` when `method` is one of the V1.5 RPC names.
pub fn is_query_rpc_method(method: &str) -> bool {
    method == CTOX_QUERY_RPC_FETCH || method == CTOX_QUERY_RPC_CANCEL
}

pub fn query_fetch_method() -> &'static str {
    CTOX_QUERY_RPC_FETCH
}

pub fn query_cancel_method() -> &'static str {
    CTOX_QUERY_RPC_CANCEL
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::storage_memory::get_rx_storage_memory;
    use crate::replication_protocol::default_conflict_handler::DefaultConflictHandler;
    use crate::rx_collection::RxCollection;
    use crate::rx_database::RxDatabase;
    use crate::rx_schema::create_rx_schema;
    use crate::rxjs_compat::{RxStream, RxSubject};
    use crate::types::{
        BulkWriteRow, HashFunction, HashOutput, JsonSchema, PrimaryKey, RxJsonSchema,
        RxStorageInstance, RxStorageInstanceCreationParams,
    };
    use async_trait::async_trait;
    use parking_lot::Mutex as TokioMutex;
    use std::collections::HashMap;
    use std::sync::Arc;

    use super::super::webrtc_types::{
        PeerWithMessage, PeerWithResponse, WebRTCConnectionHandler, WebRTCWireFrame,
    };

    struct TestHashFunction;
    impl HashFunction for TestHashFunction {
        fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
            Box::pin(async move { format!("hash:{input}") })
        }
    }

    fn schema() -> RxJsonSchema {
        let mut properties = HashMap::new();
        properties.insert(
            "id".to_string(),
            JsonSchema {
                schema_type: Some("string".to_string()),
                max_length: Some(100),
                ..Default::default()
            },
        );
        properties.insert(
            "age".to_string(),
            JsonSchema {
                schema_type: Some("number".to_string()),
                ..Default::default()
            },
        );
        RxJsonSchema {
            version: 0,
            primary_key: PrimaryKey::Simple("id".to_string()),
            schema_type: "object".to_string(),
            properties,
            required: vec!["id".to_string()],
            indexes: vec![],
            encrypted: Vec::new(),
            internal_indexes: Vec::new(),
            key_compression: false,
            attachments: None,
            additional_properties: false,
            extra: HashMap::new(),
        }
    }

    fn doc(id: &str, age: i64) -> Value {
        json!({
            "id": id,
            "age": age,
            "_rev": "1-x",
            "_deleted": false,
            "_meta": { "lwt": age as f64 },
            "_attachments": {}
        })
    }

    async fn seeded_collection(count: usize) -> Arc<RxCollection> {
        let hash = Arc::new(TestHashFunction);
        let raw = schema();
        let rx_schema = Arc::new(create_rx_schema(raw, hash.clone(), false).unwrap());
        let storage = get_rx_storage_memory(());
        let storage_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "tok".to_string(),
                    database_name: "db".to_string(),
                    collection_name: "business_records".to_string(),
                    schema: rx_schema.json_schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        let mut rows = Vec::with_capacity(count);
        for i in 0..count {
            rows.push(BulkWriteRow {
                previous: None,
                document: doc(&format!("doc-{i:04}"), i as i64),
            });
        }
        storage_instance.bulk_write(rows, "seed").await.unwrap();
        let database = RxDatabase::new("db", "tok", "stoken", false, hash, storage);
        RxCollection::new_with_schema(
            "business_records",
            database,
            storage_instance,
            Arc::new(DefaultConflictHandler),
            rx_schema,
        )
    }

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
        buffered: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl MockHandler {
        fn new() -> Self {
            Self {
                sent: Arc::new(TokioMutex::new(Vec::new())),
                buffered: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
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
            self.buffered.load(std::sync::atomic::Ordering::SeqCst)
        }
        fn peer_identity(&self, peer: &Self::Peer) -> String {
            peer.0.to_string()
        }
    }

    fn make_request(request_id: &str, collection: &str, schema_version: u32) -> WebRTCMessage {
        WebRTCMessage {
            id: format!("msg-{request_id}"),
            method: CTOX_QUERY_RPC_FETCH.to_string(),
            params: vec![json!({
                "requestId": request_id,
                "databaseName": "db",
                "collectionName": collection,
                "schemaVersion": schema_version,
                "queryFingerprint": "fp-test",
                "query": {
                    "selector": { "age": { "$gte": 0 } },
                    "sort": [{ "age": "asc" }],
                },
                "window": { "offset": 0, "limit": 1000 }
            })],
        }
    }

    #[tokio::test]
    async fn rejects_unregistered_collection() {
        let registry = Arc::new(QueryFetchRegistry::new(4));
        let handler = Arc::new(MockHandler::new());
        let message = make_request("r1", "not_registered", 0);
        run_query_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".to_string(),
            message,
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        let error_frame = frames
            .iter()
            .find(|f| matches!(f, WebRTCWireFrame::Message(m) if m.method == CTOX_QUERY_RPC_ERROR));
        assert!(
            error_frame.is_some(),
            "missing error frame for unregistered collection"
        );
    }

    #[tokio::test]
    async fn streams_documents_as_chunks() {
        let collection = seeded_collection(450).await;
        let registry = Arc::new(QueryFetchRegistry::new(4));
        registry.register(Arc::clone(&collection));
        let handler = Arc::new(MockHandler::new());
        let message = make_request("r2", "business_records", 0);
        run_query_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".to_string(),
            message,
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        // First frame: ack response.
        assert!(matches!(frames.first(), Some(WebRTCWireFrame::Response(_))));
        // Subsequent frames: chunk messages with method rxdb.query.chunk.
        let chunks: Vec<_> = frames
            .iter()
            .filter_map(|f| match f {
                WebRTCWireFrame::Message(m) if m.method == CTOX_QUERY_RPC_CHUNK => {
                    serde_json::from_value::<QueryFetchChunk>(m.params[0].clone()).ok()
                }
                _ => None,
            })
            .collect();
        assert!(
            chunks.len() >= 3,
            "expected at least 3 chunks for 450 docs (got {})",
            chunks.len()
        );
        let total: usize = chunks
            .iter()
            .map(|c| decode_chunk_documents(c).expect("decode").len())
            .sum();
        assert_eq!(total, 450, "all documents must be streamed");
        assert!(
            chunks.last().unwrap().complete,
            "last chunk must mark complete=true"
        );
        assert!(
            chunks[..chunks.len() - 1].iter().all(|c| !c.complete),
            "non-last chunks must have complete=false"
        );
    }

    #[tokio::test]
    async fn empty_result_still_emits_complete_chunk() {
        let collection = seeded_collection(0).await;
        let registry = Arc::new(QueryFetchRegistry::new(4));
        registry.register(Arc::clone(&collection));
        let handler = Arc::new(MockHandler::new());
        let message = make_request("r3", "business_records", 0);
        run_query_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".to_string(),
            message,
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        let chunks: Vec<_> = frames
            .iter()
            .filter_map(|f| match f {
                WebRTCWireFrame::Message(m) if m.method == CTOX_QUERY_RPC_CHUNK => {
                    serde_json::from_value::<QueryFetchChunk>(m.params[0].clone()).ok()
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            chunks.len(),
            1,
            "empty result must emit exactly one terminal chunk"
        );
        assert!(chunks[0].documents.is_empty());
        assert!(chunks[0].complete);
    }

    #[tokio::test]
    async fn cancel_marks_inflight_flag() {
        let registry = Arc::new(QueryFetchRegistry::new(4));
        let flag = registry.try_acquire("r4").unwrap();
        assert!(!flag.load(Ordering::SeqCst));
        let cancel_message = WebRTCMessage {
            id: "cancel-1".to_string(),
            method: CTOX_QUERY_RPC_CANCEL.to_string(),
            params: vec![json!({ "requestId": "r4", "reason": "client-abort" })],
        };
        let request_id = parse_query_cancel_request(&cancel_message).unwrap();
        assert_eq!(request_id, "r4");
        assert!(registry.cancel(&request_id));
        assert!(flag.load(Ordering::SeqCst));
        registry.release("r4");
    }

    #[tokio::test]
    async fn max_inflight_returns_stream_limit_error() {
        let registry = Arc::new(QueryFetchRegistry::new(1));
        let collection = seeded_collection(10).await;
        registry.register(Arc::clone(&collection));
        let _hold = registry.try_acquire("hold").unwrap();
        let handler = Arc::new(MockHandler::new());
        let message = make_request("r5", "business_records", 0);
        run_query_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".to_string(),
            message,
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        let saw_limit = frames.iter().any(|f| matches!(
            f,
            WebRTCWireFrame::Message(m) if m.method == CTOX_QUERY_RPC_ERROR
                && m.params[0].get("code").and_then(Value::as_str) == Some(QUERY_FETCH_ERROR_STREAM_LIMIT)
        ));
        assert!(saw_limit, "stream limit error must be emitted");
    }

    fn error_code_emitted(frames: &[WebRTCWireFrame], expected: &str) -> bool {
        frames.iter().any(|f| {
            matches!(
                f,
                WebRTCWireFrame::Message(m) if m.method == CTOX_QUERY_RPC_ERROR
                    && m.params[0].get("code").and_then(Value::as_str) == Some(expected)
            )
        })
    }

    #[tokio::test]
    async fn feature_flag_disabled_blocks_fetch() {
        let registry = Arc::new(QueryFetchRegistry::new(4));
        registry.set_feature_enabled(false);
        let handler = Arc::new(MockHandler::new());
        let message = make_request("rf", "business_records", 0);
        run_query_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".to_string(),
            message,
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        assert!(error_code_emitted(
            &frames,
            QUERY_FETCH_ERROR_FEATURE_DISABLED
        ));
    }

    #[tokio::test]
    async fn auth_callback_can_deny_access() {
        let collection = seeded_collection(3).await;
        let registry = Arc::new(QueryFetchRegistry::new(4));
        registry.register(Arc::clone(&collection));
        registry.set_auth_check(Arc::new(|_peer, _coll| false));
        let handler = Arc::new(MockHandler::new());
        let message = make_request("ra", "business_records", 0);
        run_query_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".to_string(),
            message,
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        assert!(error_code_emitted(&frames, QUERY_FETCH_ERROR_UNAUTHORIZED));
    }

    #[tokio::test]
    async fn rate_limit_kicks_in_after_burst() {
        let collection = seeded_collection(1).await;
        let registry = Arc::new(QueryFetchRegistry::new(2));
        registry.register(Arc::clone(&collection));
        // Exhaust the bucket: each accepted call consumes one token. The burst
        // is intentionally larger than max_inflight, but finite.
        let handler = Arc::new(MockHandler::new());
        let burst = registry.rate_burst;
        for i in 0..=burst {
            let msg = make_request(&format!("rate-{i}"), "business_records", 0);
            run_query_fetch(
                Arc::clone(&registry),
                Arc::clone(&handler),
                MockPeer("p1"),
                "p1".to_string(),
                msg,
            )
            .await
            .unwrap();
        }
        let frames = handler.sent.lock();
        assert!(error_code_emitted(&frames, QUERY_FETCH_ERROR_RATE_LIMITED));
    }

    #[tokio::test]
    async fn normal_startup_burst_is_not_rate_limited() {
        let collection = seeded_collection(1).await;
        let registry = Arc::new(QueryFetchRegistry::new(4));
        registry.register(Arc::clone(&collection));
        let handler = Arc::new(MockHandler::new());
        for i in 0..16 {
            let msg = make_request(&format!("startup-{i}"), "business_records", 0);
            run_query_fetch(
                Arc::clone(&registry),
                Arc::clone(&handler),
                MockPeer("p1"),
                "p1".to_string(),
                msg,
            )
            .await
            .unwrap();
        }
        let frames = handler.sent.lock();
        assert!(
            !error_code_emitted(&frames, QUERY_FETCH_ERROR_RATE_LIMITED),
            "legitimate startup demand-load fanout must not be rate-limited"
        );
    }

    #[tokio::test]
    async fn unsupported_collection_does_not_consume_rate_tokens() {
        let collection = seeded_collection(1).await;
        let registry = Arc::new(QueryFetchRegistry::new(2));
        registry.register(Arc::clone(&collection));
        let handler = Arc::new(MockHandler::new());

        for i in 0..registry.rate_burst {
            let msg = make_request(&format!("unsupported-{i}"), "unsupported_records", 0);
            run_query_fetch(
                Arc::clone(&registry),
                Arc::clone(&handler),
                MockPeer("p1"),
                "p1".to_string(),
                msg,
            )
            .await
            .unwrap();
        }
        handler.sent.lock().clear();

        let msg = make_request("supported-after-unsupported", "business_records", 0);
        run_query_fetch(
            Arc::clone(&registry),
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".to_string(),
            msg,
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        assert!(
            !error_code_emitted(&frames, QUERY_FETCH_ERROR_RATE_LIMITED),
            "unsupported probes must not spend query-fetch tokens"
        );
    }

    #[tokio::test]
    async fn rate_limit_is_per_peer_not_global() {
        let collection = seeded_collection(1).await;
        let registry = Arc::new(QueryFetchRegistry::new(2));
        registry.register(Arc::clone(&collection));
        let handler = Arc::new(MockHandler::new());
        // peer1 burns its bucket
        for i in 0..=registry.rate_burst {
            let _ = run_query_fetch(
                Arc::clone(&registry),
                Arc::clone(&handler),
                MockPeer("peerA"),
                "peerA".to_string(),
                make_request(&format!("a-{i}"), "business_records", 0),
            )
            .await;
        }
        // Reset handler buffer to inspect just peerB's path.
        handler.sent.lock().clear();
        // peer2 still has full bucket.
        let _ = run_query_fetch(
            Arc::clone(&registry),
            Arc::clone(&handler),
            MockPeer("peerB"),
            "peerB".to_string(),
            make_request("b-1", "business_records", 0),
        )
        .await;
        let frames = handler.sent.lock();
        assert!(
            !error_code_emitted(&frames, QUERY_FETCH_ERROR_RATE_LIMITED),
            "peerB must NOT be rate-limited"
        );
    }

    #[tokio::test]
    async fn buffered_bytes_above_water_pauses_send() {
        let collection = seeded_collection(800).await;
        let registry = Arc::new(QueryFetchRegistry::new(4));
        registry.register(Arc::clone(&collection));
        let handler = Arc::new(MockHandler::new());
        // Simulate a slow drain: buffered jumps high after every send, then
        // drains itself in a background task.
        let buffered = Arc::clone(&handler.buffered);
        let drain_task = tokio::spawn(async move {
            for _ in 0..40 {
                tokio::time::sleep(Duration::from_millis(5)).await;
                buffered.store(0, std::sync::atomic::Ordering::SeqCst);
            }
        });
        // Pre-set buffered so first chunk waits.
        handler.buffered.store(
            WEBRTC_BUFFERED_HIGH_WATER + 1,
            std::sync::atomic::Ordering::SeqCst,
        );
        let message = make_request("bp", "business_records", 0);
        let start = Instant::now();
        run_query_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".to_string(),
            message,
        )
        .await
        .unwrap();
        let elapsed = start.elapsed();
        drain_task.abort();
        // Dispatcher must have waited at least one backoff window.
        assert!(
            elapsed >= Duration::from_millis(4),
            "expected backpressure wait, got {:?}",
            elapsed
        );
        let frames = handler.sent.lock();
        let total_chunks: usize = frames
            .iter()
            .filter(
                |f| matches!(f, WebRTCWireFrame::Message(m) if m.method == CTOX_QUERY_RPC_CHUNK),
            )
            .count();
        assert!(
            total_chunks >= 4,
            "all chunks must still be emitted after backpressure resolves"
        );
    }

    #[tokio::test]
    async fn byte_cap_splits_large_documents_across_chunks() {
        // 100 docs * 1024 chars-per-string would exceed 256KB single-chunk budget.
        let hash = Arc::new(TestHashFunction);
        let raw = schema();
        let rx_schema = Arc::new(create_rx_schema(raw, hash.clone(), false).unwrap());
        let storage = get_rx_storage_memory(());
        let storage_instance: Arc<dyn RxStorageInstance> = storage
            .create_storage_instance(
                RxStorageInstanceCreationParams {
                    database_instance_token: "tok".to_string(),
                    database_name: "db".to_string(),
                    collection_name: "business_records".to_string(),
                    schema: rx_schema.json_schema.clone(),
                    options: HashMap::new(),
                    multi_instance: false,
                    dev_mode: false,
                    password: None,
                },
                (),
            )
            .await
            .unwrap();
        // 200 docs × ~3 KB = ~600 KB > 256 KB byte cap.
        let mut rows = Vec::with_capacity(200);
        let payload = "X".repeat(3000);
        for i in 0..200 {
            rows.push(BulkWriteRow {
                previous: None,
                document: json!({
                    "id": format!("doc-{i:04}"),
                    "age": i,
                    "_rev": "1-x",
                    "_deleted": false,
                    "_meta": { "lwt": i as f64 },
                    "_attachments": {},
                    "payload": payload,
                }),
            });
        }
        storage_instance.bulk_write(rows, "seed").await.unwrap();
        let database =
            crate::rx_database::RxDatabase::new("db", "tok", "stoken", false, hash, storage);
        let collection = RxCollection::new_with_schema(
            "business_records",
            database,
            storage_instance,
            Arc::new(DefaultConflictHandler),
            rx_schema,
        );
        let registry = Arc::new(QueryFetchRegistry::new(4));
        registry.register(Arc::clone(&collection));
        let handler = Arc::new(MockHandler::new());
        run_query_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".to_string(),
            make_request("bc", "business_records", 0),
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        let max_chunk_bytes: usize = frames
            .iter()
            .filter_map(|f| match f {
                WebRTCWireFrame::Message(m) if m.method == CTOX_QUERY_RPC_CHUNK => {
                    Some(serde_json::to_string(&m.params[0]).unwrap().len())
                }
                _ => None,
            })
            .max()
            .unwrap_or(0);
        // 256 KB cap plus a small grace (each chunk envelope, last doc that pushed over).
        // We allow up to 1.05x the cap because the algorithm flushes BEFORE pushing
        // a doc that would exceed; the largest chunk is bounded by docs.len * doc_bytes.
        assert!(
            max_chunk_bytes <= (CTOX_QUERY_MAX_BYTES_PER_CHUNK as usize) + 8192,
            "max chunk {} must respect byte cap {}",
            max_chunk_bytes,
            CTOX_QUERY_MAX_BYTES_PER_CHUNK
        );
        let total_chunks: usize = frames
            .iter()
            .filter(
                |f| matches!(f, WebRTCWireFrame::Message(m) if m.method == CTOX_QUERY_RPC_CHUNK),
            )
            .count();
        assert!(
            total_chunks >= 3,
            "200 × 3KB must split into ≥3 chunks (got {})",
            total_chunks
        );
    }
}
