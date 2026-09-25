//! Server-push dispatcher for `rxdb.rows.fetch`.
//!
//! Mirrors [`super::file_fetch_handler`] for knowledge-table row windows. A
//! registered source returns one window; this handler keeps row order and
//! slices that window into `rxdb.rows.chunk` frames whose serialized params
//! stay under `CTOX_ROWS_MAX_BYTES_PER_CHUNK`.
//!
//! # Unknown tables
//!
//! The source signals an unknown or archived table by returning [`RxError`]
//! with code [`ROWS_FETCH_ERROR_TABLE_NOT_FOUND`]. `NOT_FOUND` and `ENOENT`
//! map to the same non-retryable code. Every other source error becomes
//! retryable [`ROWS_FETCH_ERROR_SOURCE`]. That mapping lives only in
//! [`rows_fetch_error_code_for_rx_error`], the same shape as
//! `query_fetch_error_code_for_rx_error`.
//!
//! A collection with no registered source is also `ROWS_TABLE_NOT_FOUND`
//! (not retryable): retrying will not create a source.
//!
//! # Rate limit
//!
//! File fetch has no token bucket. E8 still requires `RATE_LIMITED`, so this
//! registry copies the per-peer bucket from query fetch (burst from the
//! in-flight cap, 16 tokens/s refill, TTL sweep). Authz and cancellation
//! follow file fetch: registry callback AND
//! `WebRTCConnectionHandler::is_collection_authorized_for_peer`, inflight
//! keyed by connection identity, cancel observed between chunks. The source
//! itself is one `spawn_blocking` call and is not interrupted mid-read.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::rx_error::{new_rx_error, RxError, RxResult};

use super::protocol_contract_generated::{
    CTOX_QUERY_MAX_RUNTIME_MS, CTOX_ROWS_MAX_BYTES_PER_CHUNK, CTOX_ROWS_MAX_ROWS_PER_WINDOW,
    CTOX_ROWS_RPC_CANCEL, CTOX_ROWS_RPC_CHUNK, CTOX_ROWS_RPC_ERROR, CTOX_ROWS_RPC_FETCH,
};
use super::query_fetch_handler::{
    send_fetch_accepted, send_fetch_error_frame, send_fetch_message, send_fetch_response,
    FetchErrorFrame, FetchInflight,
};
#[cfg(test)]
use super::webrtc_types::WebRTCWireFrame;
use super::webrtc_types::{WebRTCConnectionHandler, WebRTCMessage, WEBRTC_BUFFERED_HIGH_WATER};

/// One demand-loaded window. `row_count` is the full table length, not
/// `rows.len()`.
#[derive(Debug, Clone, PartialEq)]
pub struct RowsWindow {
    pub rows: Vec<Value>,
    pub row_count: i64,
    pub content_hash: String,
    pub schema_hash: String,
}

/// `params[0]` of `rxdb.rows.fetch`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RowsFetchRequest {
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "collectionName")]
    pub collection_name: String,
    #[serde(rename = "tableId")]
    pub table_id: String,
    pub offset: u64,
    pub limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct RowsFetchChunk {
    #[serde(rename = "requestId")]
    request_id: String,
    seq: u32,
    #[serde(rename = "final")]
    is_final: bool,
    #[serde(rename = "tableId")]
    table_id: String,
    offset: u64,
    #[serde(rename = "rowCount")]
    row_count: i64,
    #[serde(rename = "contentHash")]
    content_hash: String,
    #[serde(rename = "schemaHash")]
    schema_hash: String,
    rows: Vec<Value>,
}

pub const ROWS_FETCH_ERROR_TABLE_NOT_FOUND: &str = "ROWS_TABLE_NOT_FOUND";
pub const ROWS_FETCH_ERROR_SOURCE: &str = "ROWS_SOURCE_ERROR";
pub const ROWS_FETCH_ERROR_UNAUTHORIZED: &str = "UNAUTHORIZED";
pub const ROWS_FETCH_ERROR_STREAM_LIMIT: &str = "STREAM_LIMIT_EXCEEDED";
pub const ROWS_FETCH_ERROR_RATE_LIMITED: &str = "RATE_LIMITED";
pub const ROWS_FETCH_ERROR_REMOTE_TIMEOUT: &str = "REMOTE_TIMEOUT";

/// `(table_id, offset, limit) -> window`. `table_id` has already had a
/// leading `table:` prefix removed, and `limit` is clamped to
/// `CTOX_ROWS_MAX_ROWS_PER_WINDOW`.
pub type RowsWindowFn = dyn Fn(&str, usize, usize) -> RxResult<RowsWindow> + Send + Sync;
pub type RowsAuthCheckFn = dyn Fn(&str, &str) -> bool + Send + Sync;

const RATE_BUCKET_REFILL_INTERVAL: Duration = Duration::from_secs(1);
const RATE_BUCKET_MIN_BURST: u32 = 32;
const RATE_BUCKET_BURST_MULTIPLIER: u32 = 8;
const RATE_BUCKET_REFILL_PER_SECOND: u32 = 16;
const PEER_RATE_BUCKET_TTL: Duration = Duration::from_secs(600);
const PEER_RATE_BUCKET_SWEEP_INTERVAL: u64 = 64;

struct PeerRateBucket {
    last_refill: Instant,
    last_access: Instant,
    tokens: u32,
    max_tokens: u32,
    refill_per_second: u32,
}

impl PeerRateBucket {
    fn new(max_tokens: u32, refill_per_second: u32) -> Self {
        let now = Instant::now();
        Self {
            last_refill: now,
            last_access: now,
            tokens: max_tokens,
            max_tokens,
            refill_per_second: refill_per_second.max(1),
        }
    }

    fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        self.last_access = now;
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

fn sweep_stale_rate_buckets(map: &mut HashMap<String, PeerRateBucket>, ttl: Duration) {
    map.retain(|_, bucket| bucket.last_access.elapsed() < ttl);
}

pub struct RowsFetchRegistry {
    sources: Mutex<HashMap<String, Arc<RowsWindowFn>>>,
    inflight_count: FetchInflight,
    peer_rate_buckets: Mutex<HashMap<String, PeerRateBucket>>,
    rate_sweep_counter: AtomicU64,
    rate_burst: u32,
    rate_refill_per_second: u32,
    auth_check: Mutex<Option<Arc<RowsAuthCheckFn>>>,
}

impl RowsFetchRegistry {
    pub fn new(max_inflight: u64) -> Self {
        let stream_based_burst = (max_inflight as u32).saturating_mul(RATE_BUCKET_BURST_MULTIPLIER);
        Self {
            sources: Mutex::new(HashMap::new()),
            inflight_count: FetchInflight::new(max_inflight),
            peer_rate_buckets: Mutex::new(HashMap::new()),
            rate_sweep_counter: AtomicU64::new(0),
            rate_burst: stream_based_burst.max(RATE_BUCKET_MIN_BURST),
            rate_refill_per_second: RATE_BUCKET_REFILL_PER_SECOND,
            auth_check: Mutex::new(None),
        }
    }

    pub fn register_rows_source(&self, collection: &str, source: Arc<RowsWindowFn>) {
        self.sources.lock().insert(collection.to_string(), source);
    }

    /// Install a peer-identity -> collection authorization callback. Without
    /// a callback the registry denies rows-fetch requests.
    pub fn set_auth_check(&self, check: Arc<RowsAuthCheckFn>) {
        *self.auth_check.lock() = Some(check);
    }

    pub fn has_source(&self, collection: &str) -> bool {
        self.sources.lock().contains_key(collection)
    }

    /// Peer-global gate for `CTOX_ROWS_FETCH_CAPABILITY`. The capability is
    /// not per collection; any registered source turns it on at the next
    /// handshake.
    pub fn has_any_source(&self) -> bool {
        !self.sources.lock().is_empty()
    }

    fn check_authorized(&self, peer_identity: &str, collection: &str) -> bool {
        match self.auth_check.lock().as_ref() {
            Some(callback) => callback(peer_identity, collection),
            None => false,
        }
    }

    fn get_source(&self, collection: &str) -> Option<Arc<RowsWindowFn>> {
        self.sources.lock().get(collection).cloned()
    }

    pub fn cancel(&self, peer_identity: &str, request_id: &str) -> bool {
        self.inflight_count.cancel(peer_identity, request_id)
    }

    pub fn cancel_peer(&self, peer_identity: &str) -> usize {
        self.inflight_count.cancel_peer(peer_identity)
    }

    fn try_acquire(
        &self,
        peer_identity: &str,
        request_id: &str,
    ) -> Option<Arc<std::sync::atomic::AtomicBool>> {
        self.inflight_count.try_acquire(peer_identity, request_id)
    }

    fn release(&self, peer_identity: &str, request_id: &str) {
        self.inflight_count.release(peer_identity, request_id);
    }

    fn try_rate_consume(&self, peer_identity: &str) -> bool {
        let mut map = self.peer_rate_buckets.lock();
        let entry = map
            .entry(peer_identity.to_string())
            .or_insert_with(|| PeerRateBucket::new(self.rate_burst, self.rate_refill_per_second));
        let allowed = entry.try_consume();
        if self.rate_sweep_counter.fetch_add(1, Ordering::Relaxed) + 1
            >= PEER_RATE_BUCKET_SWEEP_INTERVAL
        {
            self.rate_sweep_counter.store(0, Ordering::Relaxed);
            sweep_stale_rate_buckets(&mut map, PEER_RATE_BUCKET_TTL);
        }
        allowed
    }
}

pub fn parse_rows_fetch_request(message: &WebRTCMessage) -> RxResult<RowsFetchRequest> {
    let first = message.params.first().cloned().unwrap_or(Value::Null);
    serde_json::from_value(first).map_err(|err| {
        new_rx_error(
            "ROWS_FETCH_PARSE",
            Some(json!({ "message": format!("invalid rxdb.rows.fetch payload: {err}") })),
        )
    })
}

pub fn parse_rows_cancel_request(message: &WebRTCMessage) -> RxResult<String> {
    message
        .params
        .first()
        .and_then(|value| value.get("requestId").and_then(Value::as_str))
        .map(str::to_owned)
        .ok_or_else(|| {
            new_rx_error(
                "ROWS_CANCEL_PARSE",
                Some(json!({ "message": "rxdb.rows.cancel requires requestId" })),
            )
        })
}

/// Strip one leading `table:` catalog prefix. Any other id is unchanged.
pub fn normalize_table_id(table_id: &str) -> &str {
    table_id.strip_prefix("table:").unwrap_or(table_id)
}

pub fn clamp_rows_limit(limit: u64) -> usize {
    let max = u64::from(CTOX_ROWS_MAX_ROWS_PER_WINDOW);
    usize::try_from(limit.min(max)).unwrap_or(usize::MAX)
}

fn rows_fetch_error_code_for_rx_error(err: &RxError) -> (&'static str, bool) {
    match err.code() {
        ROWS_FETCH_ERROR_TABLE_NOT_FOUND | "NOT_FOUND" | "ENOENT" => {
            (ROWS_FETCH_ERROR_TABLE_NOT_FOUND, false)
        }
        ROWS_FETCH_ERROR_UNAUTHORIZED | "PERMISSION_DENIED" | "EACCES" | "EPERM" => {
            (ROWS_FETCH_ERROR_UNAUTHORIZED, false)
        }
        _ => (ROWS_FETCH_ERROR_SOURCE, true),
    }
}

struct NormalizedRowsRequest {
    request_id: String,
    collection_name: String,
    table_id: String,
    offset: u64,
    offset_for_source: usize,
    limit: usize,
}

fn normalize_rows_request(request: RowsFetchRequest) -> NormalizedRowsRequest {
    NormalizedRowsRequest {
        request_id: request.request_id,
        collection_name: request.collection_name,
        table_id: normalize_table_id(&request.table_id).to_string(),
        offset: request.offset,
        offset_for_source: usize::try_from(request.offset).unwrap_or(usize::MAX),
        limit: clamp_rows_limit(request.limit),
    }
}

pub async fn run_rows_fetch<H: WebRTCConnectionHandler>(
    registry: Arc<RowsFetchRegistry>,
    handler: Arc<H>,
    peer: H::Peer,
    peer_identity: String,
    message: WebRTCMessage,
) -> RxResult<()> {
    tracing::info!(peer = %peer_identity, msg_id = %message.id, "rxdb.rows.fetch begin");

    let request = match parse_rows_fetch_request(&message) {
        Ok(request) => request,
        Err(err) => {
            send_fetch_response(
                handler.as_ref(),
                &peer,
                &message.id,
                Value::Null,
                Some(err.to_string()),
            )
            .await;
            return Err(err);
        }
    };

    if !registry.check_authorized(&peer_identity, &request.collection_name)
        || !handler.is_collection_authorized_for_peer(&peer, &request.collection_name)
    {
        send_rows_error(
            handler.as_ref(),
            &peer,
            &message.id,
            &request.request_id,
            ROWS_FETCH_ERROR_UNAUTHORIZED,
            "peer is not authorized for this collection",
            false,
        )
        .await;
        return Ok(());
    }

    let source = match registry.get_source(&request.collection_name) {
        Some(source) => source,
        None => {
            send_rows_error(
                handler.as_ref(),
                &peer,
                &message.id,
                &request.request_id,
                ROWS_FETCH_ERROR_TABLE_NOT_FOUND,
                "no rows source registered for this collection",
                false,
            )
            .await;
            return Ok(());
        }
    };

    if !registry.try_rate_consume(&peer_identity) {
        send_rows_error(
            handler.as_ref(),
            &peer,
            &message.id,
            &request.request_id,
            ROWS_FETCH_ERROR_RATE_LIMITED,
            "per-peer rows-fetch rate limit reached",
            true,
        )
        .await;
        return Ok(());
    }

    let connection_identity = handler.connection_identity(&peer);
    let cancel_flag = match registry.try_acquire(&connection_identity, &request.request_id) {
        Some(flag) => flag,
        None => {
            send_rows_error(
                handler.as_ref(),
                &peer,
                &message.id,
                &request.request_id,
                ROWS_FETCH_ERROR_STREAM_LIMIT,
                "max in-flight rows streams reached",
                true,
            )
            .await;
            return Ok(());
        }
    };

    let request = normalize_rows_request(request);
    send_fetch_accepted(handler.as_ref(), &peer, &message.id, &request.request_id).await;

    let outcome = stream_rows(handler.as_ref(), &peer, &request, source, &cancel_flag).await;
    registry.release(&connection_identity, &request.request_id);
    outcome
}

async fn stream_rows<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    request: &NormalizedRowsRequest,
    source: Arc<RowsWindowFn>,
    cancel_flag: &Arc<std::sync::atomic::AtomicBool>,
) -> RxResult<()> {
    let runtime_deadline = Instant::now() + Duration::from_millis(CTOX_QUERY_MAX_RUNTIME_MS as u64);
    let table_id = request.table_id.clone();
    let offset = request.offset_for_source;
    let limit = request.limit;
    let joined = tokio::task::spawn_blocking(move || source(&table_id, offset, limit)).await;
    let window = match joined {
        Ok(result) => result,
        Err(err) => Err(new_rx_error(
            "ROWS_FETCH_JOIN",
            Some(json!({ "message": format!("rows source worker failed: {err}") })),
        )),
    };
    let window = match window {
        Ok(window) => window,
        Err(err) => {
            let (code, retryable) = rows_fetch_error_code_for_rx_error(&err);
            send_rows_error(
                handler,
                peer,
                "",
                &request.request_id,
                code,
                &format!("rows source error: {err}"),
                retryable,
            )
            .await;
            return Ok(());
        }
    };

    if cancel_flag.load(Ordering::SeqCst) {
        return Ok(());
    }

    let row_json = window
        .rows
        .iter()
        .map(|row| serde_json::to_vec(row).unwrap_or_else(|_| b"null".to_vec()))
        .collect::<Vec<_>>();
    let packed = pack_row_chunks(&row_json, |seq, is_final| {
        empty_chunk_len(request, &window, seq, is_final)
    });

    for chunk in &packed {
        if !wait_until_sendable(handler, peer, cancel_flag, runtime_deadline, request).await? {
            return Ok(());
        }
        let rows = window.rows[chunk.start..chunk.end].to_vec();
        let frame = RowsFetchChunk {
            request_id: request.request_id.clone(),
            seq: chunk.seq,
            is_final: chunk.is_final,
            table_id: request.table_id.clone(),
            offset: request.offset,
            row_count: window.row_count,
            content_hash: window.content_hash.clone(),
            schema_hash: window.schema_hash.clone(),
            rows,
        };
        debug_assert!(
            chunk_fits_budget(&frame, &row_json[chunk.start..chunk.end]),
            "packed rows chunk exceeded CTOX_ROWS_MAX_BYTES_PER_CHUNK"
        );
        send_rows_chunk(handler, peer, request, &frame).await;
    }
    Ok(())
}

async fn wait_until_sendable<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    cancel_flag: &Arc<std::sync::atomic::AtomicBool>,
    runtime_deadline: Instant,
    request: &NormalizedRowsRequest,
) -> RxResult<bool> {
    let mut backoff_ms = 4u64;
    loop {
        if cancel_flag.load(Ordering::SeqCst) {
            return Ok(false);
        }
        if Instant::now() >= runtime_deadline {
            send_rows_error(
                handler,
                peer,
                "",
                &request.request_id,
                ROWS_FETCH_ERROR_REMOTE_TIMEOUT,
                "rows transfer stalled: timeout",
                true,
            )
            .await;
            return Ok(false);
        }
        if handler.buffered_bytes(peer) <= WEBRTC_BUFFERED_HIGH_WATER {
            return Ok(true);
        }
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms * 2).min(64);
    }
}

fn empty_chunk_len(
    request: &NormalizedRowsRequest,
    window: &RowsWindow,
    seq: u32,
    is_final: bool,
) -> usize {
    let frame = RowsFetchChunk {
        request_id: request.request_id.clone(),
        seq,
        is_final,
        table_id: request.table_id.clone(),
        offset: request.offset,
        row_count: window.row_count,
        content_hash: window.content_hash.clone(),
        schema_hash: window.schema_hash.clone(),
        rows: Vec::new(),
    };
    match serde_json::to_vec(&frame) {
        Ok(bytes) => {
            debug_assert!(
                bytes.ends_with(br#""rows":[]}"#),
                "rows must stay the last chunk field so the size formula holds: {}",
                String::from_utf8_lossy(&bytes)
            );
            bytes.len()
        }
        Err(_) => usize::MAX / 4,
    }
}

/// Bytes inserted between the empty chunk's `[]` brackets.
fn rows_body_len(row_json: &[Vec<u8>]) -> usize {
    if row_json.is_empty() {
        return 0;
    }
    row_json.iter().map(Vec::len).sum::<usize>() + row_json.len() - 1
}

struct PackedChunk {
    seq: u32,
    is_final: bool,
    start: usize,
    end: usize,
}

fn pack_row_chunks(
    row_json: &[Vec<u8>],
    empty_len: impl Fn(u32, bool) -> usize,
) -> Vec<PackedChunk> {
    let budget = CTOX_ROWS_MAX_BYTES_PER_CHUNK as usize;
    if row_json.is_empty() {
        return vec![PackedChunk {
            seq: 0,
            is_final: true,
            start: 0,
            end: 0,
        }];
    }
    let mut packed = Vec::new();
    let mut start = 0usize;
    let mut seq = 0u32;
    while start < row_json.len() {
        // `final: false` is one byte larger than `final: true`, so packing
        // against it keeps the terminal chunk under the budget too.
        let false_len = empty_len(seq, false);
        let mut end = start;
        while end < row_json.len() {
            let candidate = end + 1;
            let len = false_len.saturating_add(rows_body_len(&row_json[start..candidate]));
            if len < budget || end == start {
                end = candidate;
                if len >= budget {
                    break;
                }
                continue;
            }
            break;
        }
        if end == start {
            break;
        }
        packed.push(PackedChunk {
            seq,
            is_final: false,
            start,
            end,
        });
        start = end;
        seq = seq.saturating_add(1);
    }
    if let Some(last) = packed.last_mut() {
        last.is_final = true;
    }
    packed
}

fn chunk_fits_budget(frame: &RowsFetchChunk, row_json: &[Vec<u8>]) -> bool {
    let budget = CTOX_ROWS_MAX_BYTES_PER_CHUNK as usize;
    let Ok(bytes) = serde_json::to_vec(frame) else {
        return false;
    };
    let predicted = {
        let mut probe = frame.clone();
        probe.rows.clear();
        let empty = serde_json::to_vec(&probe)
            .map(|encoded| encoded.len())
            .unwrap_or(usize::MAX);
        empty.saturating_add(rows_body_len(row_json))
    };
    debug_assert_eq!(
        predicted,
        bytes.len(),
        "serialized chunk length drifted from the packer estimate"
    );
    bytes.len() < budget || frame.rows.len() <= 1
}

async fn send_rows_chunk<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    request: &NormalizedRowsRequest,
    frame: &RowsFetchChunk,
) {
    send_fetch_message(
        handler,
        peer,
        format!("{}-r{}", request.request_id, frame.seq),
        CTOX_ROWS_RPC_CHUNK,
        serde_json::to_value(frame).unwrap_or(Value::Null),
        Some(request.collection_name.clone()),
    )
    .await;
}

async fn send_rows_error<H: WebRTCConnectionHandler>(
    handler: &H,
    peer: &H::Peer,
    ack_id: &str,
    request_id: &str,
    code: &str,
    message: &str,
    retryable: bool,
) {
    send_fetch_error_frame(
        handler,
        peer,
        ack_id,
        FetchErrorFrame {
            request_id,
            error_method: CTOX_ROWS_RPC_ERROR,
            code,
            message,
            retryable,
        },
    )
    .await;
}

pub fn rows_fetch_method() -> &'static str {
    CTOX_ROWS_RPC_FETCH
}

pub fn rows_cancel_method() -> &'static str {
    CTOX_ROWS_RPC_CANCEL
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rx_error::RxError;
    use crate::rxjs_compat::{RxStream, RxSubject};
    use async_trait::async_trait;
    use parking_lot::Mutex as ParkingMutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize};

    use super::super::webrtc_types::{PeerWithMessage, PeerWithResponse};

    #[derive(Clone, Debug)]
    struct MockPeer(&'static str);
    impl PartialEq for MockPeer {
        fn eq(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl Eq for MockPeer {}
    impl std::hash::Hash for MockPeer {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.hash(state);
        }
    }

    struct MockHandler {
        sent: Arc<ParkingMutex<Vec<WebRTCWireFrame>>>,
        buffered: Arc<AtomicUsize>,
        collection_authorized: Arc<AtomicBool>,
        stall_after_chunk: Arc<AtomicUsize>,
        chunks_sent: Arc<AtomicUsize>,
    }

    impl MockHandler {
        fn new() -> Self {
            Self {
                sent: Arc::new(ParkingMutex::new(Vec::new())),
                buffered: Arc::new(AtomicUsize::new(0)),
                collection_authorized: Arc::new(AtomicBool::new(true)),
                stall_after_chunk: Arc::new(AtomicUsize::new(0)),
                chunks_sent: Arc::new(AtomicUsize::new(0)),
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

        fn document_fields_for_peer(&self, _: &Self::Peer, _: &str) -> Option<Vec<String>> {
            None
        }
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
            if let WebRTCWireFrame::Message(message) = &frame {
                if message.method == CTOX_ROWS_RPC_CHUNK {
                    let sent = self.chunks_sent.fetch_add(1, Ordering::SeqCst) + 1;
                    let stall_after = self.stall_after_chunk.load(Ordering::SeqCst);
                    if stall_after > 0 && sent >= stall_after {
                        self.buffered.store(
                            WEBRTC_BUFFERED_HIGH_WATER.saturating_add(1),
                            Ordering::SeqCst,
                        );
                    }
                }
            }
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

    fn authorized_rows_registry(max_inflight: u64) -> Arc<RowsFetchRegistry> {
        let registry = Arc::new(RowsFetchRegistry::new(max_inflight));
        registry.set_auth_check(Arc::new(|_peer, _collection| true));
        registry
    }

    fn make_request(
        id: &str,
        collection: &str,
        table_id: &str,
        offset: u64,
        limit: u64,
    ) -> WebRTCMessage {
        WebRTCMessage {
            id: format!("msg-{id}"),
            method: CTOX_ROWS_RPC_FETCH.to_string(),
            params: vec![json!({
                "requestId": id,
                "collectionName": collection,
                "tableId": table_id,
                "offset": offset,
                "limit": limit,
            })],
            collection: Some(collection.to_string()),
        }
    }

    fn wide_rows(count: usize) -> Vec<Value> {
        (0..count)
            .map(|index| {
                json!({
                    "index": index,
                    "payload": "x".repeat(4096),
                })
            })
            .collect()
    }

    fn chunk_frames(frames: &[WebRTCWireFrame]) -> Vec<&Value> {
        frames
            .iter()
            .filter_map(|frame| match frame {
                WebRTCWireFrame::Message(message) if message.method == CTOX_ROWS_RPC_CHUNK => {
                    message.params.first()
                }
                _ => None,
            })
            .collect()
    }

    fn decode_chunks(frames: &[WebRTCWireFrame]) -> Vec<RowsFetchChunk> {
        chunk_frames(frames)
            .into_iter()
            .map(|value| serde_json::from_value(value.clone()).expect("rows chunk"))
            .collect()
    }

    fn error_code_emitted(frames: &[WebRTCWireFrame], expected: &str) -> bool {
        frames.iter().any(|frame| {
            matches!(
                frame,
                WebRTCWireFrame::Message(message)
                    if message.method == CTOX_ROWS_RPC_ERROR
                        && message.params.first().and_then(|value| value.get("code")).and_then(Value::as_str)
                            == Some(expected)
            )
        })
    }

    fn error_retryable_emitted(frames: &[WebRTCWireFrame], expected: bool) -> bool {
        frames.iter().any(|frame| {
            matches!(
                frame,
                WebRTCWireFrame::Message(message)
                    if message.method == CTOX_ROWS_RPC_ERROR
                        && message.params.first().and_then(|value| value.get("retryable")).and_then(Value::as_bool)
                            == Some(expected)
            )
        })
    }

    #[tokio::test]
    async fn rows_fetch_streams_window_in_ordered_chunks_under_budget() {
        let rows = wide_rows(2_500);
        let rows_for_source = rows.clone();
        let registry = authorized_rows_registry(4);
        registry.register_rows_source(
            "knowledge_tables",
            Arc::new(move |_table_id, _offset, _limit| {
                Ok(RowsWindow {
                    rows: rows_for_source.clone(),
                    row_count: 5_103,
                    content_hash: "content-hash".to_string(),
                    schema_hash: "schema-hash".to_string(),
                })
            }),
        );
        let handler = Arc::new(MockHandler::new());
        run_rows_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".into(),
            make_request("wide", "knowledge_tables", "kdt-measured", 40, 2_500),
        )
        .await
        .unwrap();

        let frames = handler.sent.lock();
        let raw_chunks = chunk_frames(&frames);
        assert!(
            raw_chunks.len() > 1,
            "2500 wide rows must span more than one chunk, got {}",
            raw_chunks.len()
        );
        let budget = CTOX_ROWS_MAX_BYTES_PER_CHUNK as usize;
        for value in &raw_chunks {
            let encoded = serde_json::to_vec(value).expect("chunk json");
            assert!(
                encoded.len() < budget,
                "chunk is {} bytes, budget is {budget}",
                encoded.len()
            );
        }
        let chunks = decode_chunks(&frames);
        let mut combined = Vec::new();
        for (index, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.seq, index as u32);
            assert_eq!(chunk.is_final, index + 1 == chunks.len());
            assert_eq!(chunk.table_id, "kdt-measured");
            assert_eq!(chunk.offset, 40);
            assert_eq!(chunk.row_count, 5_103);
            assert_eq!(chunk.content_hash, "content-hash");
            assert_eq!(chunk.schema_hash, "schema-hash");
            assert_eq!(chunk.request_id, "wide");
            combined.extend(chunk.rows.iter().cloned());
        }
        assert_eq!(combined, rows);
        assert!(chunks.last().is_some_and(|chunk| chunk.is_final));
        assert!(chunks.iter().rev().skip(1).all(|chunk| !chunk.is_final));
    }

    #[tokio::test]
    async fn rows_fetch_empty_window_sends_single_final_chunk() {
        let registry = authorized_rows_registry(4);
        registry.register_rows_source(
            "knowledge_tables",
            Arc::new(|_table_id, _offset, _limit| {
                Ok(RowsWindow {
                    rows: Vec::new(),
                    row_count: 0,
                    content_hash: "empty-content".to_string(),
                    schema_hash: "empty-schema".to_string(),
                })
            }),
        );
        let handler = Arc::new(MockHandler::new());
        run_rows_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".into(),
            make_request("empty", "knowledge_tables", "kdt-empty", 0, 100),
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        let chunks = decode_chunks(&frames);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].seq, 0);
        assert!(chunks[0].is_final);
        assert!(chunks[0].rows.is_empty());
        assert_eq!(chunks[0].row_count, 0);
        assert_eq!(chunks[0].content_hash, "empty-content");
        assert_eq!(chunks[0].schema_hash, "empty-schema");
        assert!(!error_code_emitted(&frames, ROWS_FETCH_ERROR_SOURCE));
    }

    #[tokio::test]
    async fn rows_fetch_rejects_unauthorized_peer_with_unauthorized_code() {
        let called = Arc::new(AtomicBool::new(false));
        let called_for_registry = Arc::clone(&called);
        let registry = Arc::new(RowsFetchRegistry::new(4));
        registry.set_auth_check(Arc::new(|peer, _collection| peer != "intruder"));
        registry.register_rows_source(
            "knowledge_tables",
            Arc::new(move |_table_id, _offset, _limit| {
                called_for_registry.store(true, Ordering::SeqCst);
                Ok(RowsWindow {
                    rows: Vec::new(),
                    row_count: 0,
                    content_hash: String::new(),
                    schema_hash: String::new(),
                })
            }),
        );
        let handler = Arc::new(MockHandler::new());
        run_rows_fetch(
            Arc::clone(&registry),
            Arc::clone(&handler),
            MockPeer("intruder"),
            "intruder".into(),
            make_request("deny-registry", "knowledge_tables", "kdt-1", 0, 10),
        )
        .await
        .unwrap();
        {
            let frames = handler.sent.lock();
            assert!(error_code_emitted(&frames, ROWS_FETCH_ERROR_UNAUTHORIZED));
            assert!(error_retryable_emitted(&frames, false));
            assert!(chunk_frames(&frames).is_empty());
        }

        let handler_gate = Arc::new(MockHandler::new());
        handler_gate.set_collection_authorized(false);
        let called_for_handler = Arc::clone(&called);
        registry.register_rows_source(
            "knowledge_tables",
            Arc::new(move |_table_id, _offset, _limit| {
                called_for_handler.store(true, Ordering::SeqCst);
                Ok(RowsWindow {
                    rows: Vec::new(),
                    row_count: 0,
                    content_hash: String::new(),
                    schema_hash: String::new(),
                })
            }),
        );
        run_rows_fetch(
            registry,
            Arc::clone(&handler_gate),
            MockPeer("allowed-by-registry"),
            "allowed-by-registry".into(),
            make_request("deny-handler", "knowledge_tables", "kdt-1", 0, 10),
        )
        .await
        .unwrap();
        let frames = handler_gate.sent.lock();
        assert!(error_code_emitted(&frames, ROWS_FETCH_ERROR_UNAUTHORIZED));
        assert!(error_retryable_emitted(&frames, false));
        assert!(chunk_frames(&frames).is_empty());
        assert!(!called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn rows_fetch_unknown_table_maps_to_rows_table_not_found_not_retryable() {
        let registry = authorized_rows_registry(4);
        registry.register_rows_source(
            "knowledge_tables",
            Arc::new(|table_id, _offset, _limit| {
                Err(new_rx_error(
                    ROWS_FETCH_ERROR_TABLE_NOT_FOUND,
                    Some(json!({ "message": format!("unknown table {table_id}") })),
                ))
            }),
        );
        let handler = Arc::new(MockHandler::new());
        run_rows_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".into(),
            make_request("missing", "knowledge_tables", "kdt-nope", 0, 10),
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        assert!(error_code_emitted(
            &frames,
            ROWS_FETCH_ERROR_TABLE_NOT_FOUND
        ));
        assert!(error_retryable_emitted(&frames, false));
        assert!(chunk_frames(&frames).is_empty());
    }

    #[tokio::test]
    async fn rows_fetch_source_io_error_is_retryable_rows_source_error() {
        let registry = authorized_rows_registry(4);
        registry.register_rows_source(
            "knowledge_tables",
            Arc::new(|_table_id, _offset, _limit| {
                Err(new_rx_error(
                    "PARQUET_IO",
                    Some(json!({ "message": "parquet read failed" })),
                ))
            }),
        );
        let handler = Arc::new(MockHandler::new());
        run_rows_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".into(),
            make_request("io", "knowledge_tables", "kdt-1", 0, 10),
        )
        .await
        .unwrap();
        let frames = handler.sent.lock();
        assert!(error_code_emitted(&frames, ROWS_FETCH_ERROR_SOURCE));
        assert!(error_retryable_emitted(&frames, true));
        assert!(chunk_frames(&frames).is_empty());
    }

    #[tokio::test]
    async fn rows_fetch_clamps_limit_and_strips_table_prefix() {
        let seen = Arc::new(ParkingMutex::new(None));
        let seen_for_source = Arc::clone(&seen);
        let registry = authorized_rows_registry(4);
        registry.register_rows_source(
            "knowledge_tables",
            Arc::new(move |table_id, offset, limit| {
                *seen_for_source.lock() = Some((table_id.to_string(), offset, limit));
                Ok(RowsWindow {
                    rows: vec![json!({"id": "only"})],
                    row_count: 1,
                    content_hash: "c".to_string(),
                    schema_hash: "s".to_string(),
                })
            }),
        );
        let handler = Arc::new(MockHandler::new());
        run_rows_fetch(
            registry,
            Arc::clone(&handler),
            MockPeer("p1"),
            "p1".into(),
            make_request(
                "clamp",
                "knowledge_tables",
                "table:kdt-42",
                15,
                u64::from(CTOX_ROWS_MAX_ROWS_PER_WINDOW) + 4_000,
            ),
        )
        .await
        .unwrap();
        assert_eq!(
            seen.lock().clone(),
            Some((
                "kdt-42".to_string(),
                15usize,
                CTOX_ROWS_MAX_ROWS_PER_WINDOW as usize
            ))
        );
        let frames = handler.sent.lock();
        let chunks = decode_chunks(&frames);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].table_id, "kdt-42");
        assert_eq!(chunks[0].offset, 15);
        assert!(chunks[0].is_final);
    }

    #[tokio::test]
    async fn rows_fetch_cancel_stops_stream() {
        let rows = wide_rows(400);
        let total = rows.len();
        let registry = authorized_rows_registry(4);
        registry.register_rows_source(
            "knowledge_tables",
            Arc::new(move |_table_id, _offset, _limit| {
                Ok(RowsWindow {
                    rows: rows.clone(),
                    row_count: total as i64,
                    content_hash: "c".to_string(),
                    schema_hash: "s".to_string(),
                })
            }),
        );
        let handler = Arc::new(MockHandler::new());
        handler.stall_after_chunk.store(1, Ordering::SeqCst);
        let task_handler = Arc::clone(&handler);
        let task_registry = Arc::clone(&registry);
        let task = tokio::spawn(async move {
            run_rows_fetch(
                task_registry,
                task_handler,
                MockPeer("p1"),
                "p1".into(),
                make_request("cancel-1", "knowledge_tables", "kdt-1", 0, 400),
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if handler.chunks_sent.load(Ordering::SeqCst) >= 1 {
                    break;
                }
                assert!(
                    !task.is_finished(),
                    "rows fetch finished before backpressure could hold the stream"
                );
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("first chunk should be sent and then stalled");

        assert!(registry.cancel("p1", "cancel-1"));
        handler.buffered.store(0, Ordering::SeqCst);
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .expect("cancel should unblock the stalled rows stream")
            .expect("rows fetch task should not panic")
            .expect("cancelled rows fetch should return ok");

        let frames = handler.sent.lock();
        let chunks = decode_chunks(&frames);
        assert_eq!(
            chunks.len(),
            1,
            "cancel should stop after the stalled chunk"
        );
        assert!(!chunks[0].is_final);
        assert!(chunks[0].rows.len() < total);
        assert!(chunks.iter().all(|chunk| !chunk.is_final));
    }
}
