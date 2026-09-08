//! **gap-item N5** — Rust-native WebRTC connection handler.
//!
//! Replaces upstream's `connection-handler-simple-peer.ts` (which wraps the
//! `simple-peer` NPM package). CTOX uses `webrtc-rs` for RTCPeerConnection /
//! DataChannel and the same simple-peer signaling server contract as the
//! browser bundle.
//!
//! Wire format on the DataChannel: one JSON `WebRTCWireFrame` per message,
//! matching upstream `JSON.stringify(messageOrResponse)` semantics.

#[path = "frame_contract_generated.rs"]
mod frame_contract_generated;

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::UdpSocket;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use ice::mdns::MulticastDnsMode;
use parking_lot::Mutex;
use serde_json::Value;
use tokio_stream::StreamExt;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    register_default_interceptors, MediaEngine, PeerConnection, PeerConnectionBuilder,
    PeerConnectionEventHandler, RTCConfigurationBuilder, RTCIceCandidateInit, RTCIceServer,
    RTCPeerConnectionState, RTCSessionDescription, Registry, SettingEngine,
};
use webrtc::runtime::default_runtime;

use crate::plugins::replication_webrtc::protocol_contract_generated::{
    CTOX_PRESENCE_MAX_ENTRIES_PER_PEER, CTOX_PRESENCE_RPC_UPDATE, CTOX_PRESENCE_STREAM_ID,
    CTOX_PRESENCE_TTL_MS,
};
use crate::plugins::replication_webrtc::signaling_client::SignalingClient;
use crate::plugins::replication_webrtc::signaling_protocol::{PeerId, RoomId, ServerToClient};
use crate::plugins::replication_webrtc::webrtc_types::{
    PeerWithMessage, PeerWithResponse, WebRTCConnectionHandler, WebRTCMessage, WebRTCResponse,
    WebRTCWireFrame, CTOX_BROWSER_INPUT_RESPONSE_COLLECTION, CTOX_BROWSER_LIVE_RESPONSE_COLLECTION,
};
use crate::rx_error::{new_rx_error, RxError, RxResult};
use crate::rxjs_compat::{RxStream, RxSubject};
use frame_contract_generated::{
    CTOX_FRAME_PROTOCOL, FRAME_ACK_WINDOW, MAX_CHUNK_BYTES, MAX_FRAME_RETRIES,
    MAX_INLINE_FRAME_BYTES, MAX_TRANSFER_BYTES,
};

const FRAME_ACK_TIMEOUT: Duration = Duration::from_secs(30);
const FRAME_RESUME_TIMEOUT: Duration = Duration::from_secs(1);
const SEND_FRAME_PAUSE: Duration = Duration::from_millis(1);
static AUXILIARY_TRANSFER_COUNTER: AtomicU64 = AtomicU64::new(1);
// Phase 1 (constant real-time stream): native -> browser SCTP send-buffer
// watermarks. webrtc-rs exposes no buffered-amount *getter*, only threshold
// *events* (OnBufferedAmountHigh / OnBufferedAmountLow), so flow control is
// driven off these thresholds. Never overrunning the SCTP send buffer is what
// keeps the channel real-time and stops the browser from killing the
// DataChannel when a large transfer (e.g. documents + blob chunks) is sent.
const DATA_CHANNEL_BUFFERED_HIGH_WATER: u32 = 1024 * 1024; // 1 MiB
const DATA_CHANNEL_BUFFERED_LOW_WATER: u32 = 256 * 1024; // 256 KiB
                                                         // Upper bound on how long a sender waits for the buffer to drain below the low
                                                         // watermark before giving up (matches the ack timeout so a wedged peer fails
                                                         // rather than hanging forever).
const SEND_CAPACITY_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const SEND_BUFFER_STALLED_ERROR_CODE: &str = "ctox_webrtc_send_buffer_stalled";
const MAX_PEER_SEND_QUEUE_FRAMES: usize = 1024;
const MAX_PEER_SEND_QUEUE_BYTES: usize = 16 * 1024 * 1024;
// Bound on the completed-frame-ack cache. Mirrors the browser twin
// (`webrtc-native.mjs`: `COMPLETED_FRAME_ACK_TTL_MS`/512). An entry lets a
// `resume` probe that arrives AFTER a chunked transfer already completed still
// receive a final ack (docs §6.3). Without a bound the map grew one entry per
// completed transfer for the peer's whole lifetime. The TTL must cover a
// realistic resume-after-complete window; 60s matches the browser.
const COMPLETED_FRAME_ACK_TTL_MS: u64 = 60_000;
const COMPLETED_FRAME_ACK_CAP: usize = 512;
// Inbound transfer reservations are bounded independently by count and by the
// memory they can consume. The byte estimate includes both the advertised
// payload and the `Vec<Option<String>>` slots allocated at `start` time.
const MAX_INCOMING_TRANSFERS_PER_PEER: usize = 8;
const MAX_INCOMING_TRANSFERS_TOTAL: usize = 64;
const MAX_INCOMING_ALLOCATED_BYTES_PER_PEER: usize = 32 * 1024 * 1024;
const MAX_INCOMING_ALLOCATED_BYTES_TOTAL: usize = 128 * 1024 * 1024;
const FAIR_SEND_SCHEDULE: [SendPriority; 7] = [
    SendPriority::High,
    SendPriority::High,
    SendPriority::High,
    SendPriority::High,
    SendPriority::Normal,
    SendPriority::Normal,
    SendPriority::Low,
];
// Phase 1 hard size invariant: the SCTP message ceiling for an RTCDataChannel is
// 16 KiB. A single `send_text` larger than this is dropped by / kills the channel
// in browsers (the exact failure the transport plan flags as the channel-killer).
// Every frame put on the wire via `send_json_text` MUST serialize to <= this.
const MAX_SERIALIZED_FRAME_BYTES: usize = 16384;
const DEFAULT_UDP_BIND_ADDR: &str = "0.0.0.0:0";
const UDP_BIND_ADDR_ENV: &str = "CTOX_WEBRTC_UDP_BIND_ADDR";

/// Phase 2: transport-control wire method by which a browser tells the native
/// peer which collections are currently foreground/subscribed. Params shape:
/// `[[collectionName, …]]` (a single array argument). Frames whose `collection`
/// is in the most-recently-reported set are sent at High priority.
pub const ACTIVE_COLLECTIONS_METHOD: &str = "rxdb.activeCollections";

/// Peer identifier assigned by the shared signaling server.
pub type WebRTCRsPeer = PeerId;

struct EnqueuedSend {
    available: Arc<tokio::sync::Notify>,
    result: tokio::sync::oneshot::Receiver<Result<(), RxError>>,
}

fn stale_connection_error(peer: &WebRTCRsConnection) -> RxError {
    new_rx_error(
        "RC_WEBRTC_PEER",
        Some(serde_json::json!({
            "message": "unknown, closed or superseded peer connection",
            "peer": peer.peer_id(), "generation": peer.generation(),
            EXPECTED_PEER_TEARDOWN_PARAM: true,
        })),
    )
}

/// A local transport handle. Signaling routes may be reused; a connection may not.
/// This is not a wire identity or a proof of membership.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WebRTCRsConnection {
    peer_id: PeerId,
    generation: u64,
}
impl WebRTCRsConnection {
    pub(crate) fn new(peer_id: PeerId, generation: u64) -> Self {
        Self {
            peer_id,
            generation,
        }
    }
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
impl std::fmt::Display for WebRTCRsConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.peer_id, self.generation)
    }
}

impl WebRTCRsConnectionHandler {
    /// Resolve a routing hint to the currently open local connection.
    pub fn connection_for_peer(&self, peer_id: &str) -> Option<WebRTCRsConnection> {
        if self.closed.load(Ordering::SeqCst) {
            return None;
        }
        self.peers
            .lock()
            .get(peer_id)
            .filter(|entry| entry.data_channel_open)
            .map(|entry| WebRTCRsConnection::new(peer_id.to_owned(), entry.generation))
    }

    fn is_current_connection(&self, connection: &WebRTCRsConnection) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }
        self.peers
            .lock()
            .get(&connection.peer_id)
            .is_some_and(|entry| {
                entry.generation == connection.generation && entry.data_channel_open
            })
    }

    /// Policy hooks may perform blocking store work. Never hold the room-wide
    /// lifecycle lock while invoking one. Revalidate both connection generation
    /// and captured credential afterward so a late result cannot authorize a
    /// replaced connection or a peer whose token changed during the lookup.
    fn evaluate_current_peer_policy<T>(
        &self,
        peer: &WebRTCRsConnection,
        evaluate: impl FnOnce(&str) -> T,
    ) -> Option<T> {
        let token = {
            let _lifecycle = self.peer_lifecycle.lock();
            if !self.is_current_connection(peer) {
                return None;
            }
            self.peer_capability_tokens
                .lock()
                .get(&peer.peer_id)
                .cloned()
        };
        let result = evaluate(token.as_deref().unwrap_or_default());
        let _lifecycle = self.peer_lifecycle.lock();
        if !self.is_current_connection(peer)
            || self.peer_capability_tokens.lock().get(&peer.peer_id) != token.as_ref()
        {
            return None;
        }
        Some(result)
    }
}

pub type CollectionAuthzHook = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;
pub type CollectionEagerPullHook = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;
pub type CollectionLiveChangeHook = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;
pub type DocumentReadFilter = Arc<dyn Fn(&Value) -> bool + Send + Sync>;
/// Per-peer read policy. Field restrictions apply to query inputs and every
/// outgoing document path, not merely to the browser's display.
#[derive(Clone)]
pub struct DocumentReadPolicy {
    pub filter: DocumentReadFilter,
    pub fields: Option<Vec<String>>,
}
pub type DocumentReadAuthzHook = Arc<dyn Fn(&str, &str) -> DocumentReadPolicy + Send + Sync>;

pub type DocumentWriteAuthzHook = Arc<dyn Fn(&str, &str, &Value) -> bool + Send + Sync>;

/// One peer's last presence report (ctox-presence-v1). Entries are opaque JSON
/// objects the browser sent (`{collection, recordId, actor, …}`); the hub only
/// relays them, it never interprets or persists them. `updated_at_ms` is
/// re-stamped on every report — including an entry-identical refresh — so the
/// TTL measures silence, not change.
#[derive(Clone, Debug, PartialEq)]
struct PeerPresenceReport {
    entries: Vec<Value>,
    updated_at_ms: u64,
}

#[derive(Clone)]
pub struct WebRTCRsConfig {
    pub signaling: Arc<SignalingClient>,
    pub peer_role: super::NativePeerRole,
    pub room: RoomId,
    pub ice_servers: Vec<RTCIceServer>,
    pub data_channel_label: String,
    pub udp_bind_addr: String,
}

impl WebRTCRsConfig {
    pub fn new(signaling: Arc<SignalingClient>, room: impl Into<RoomId>) -> Self {
        Self {
            signaling,
            room: room.into(),
            peer_role: super::NativePeerRole::CtoxInstance,
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                ..Default::default()
            }],
            data_channel_label: "rxdb".to_string(),
            udp_bind_addr: default_udp_bind_addr(),
        }
    }
}

struct PeerEntry {
    generation: u64,
    // An offer addressed to a new local signaling identity belongs to a new
    // connection, even while the old identity's DataChannel is still open.
    local_peer_id: Option<PeerId>,
    peer_connection: Arc<dyn PeerConnection>,
    data_channel: Option<Arc<dyn DataChannel>>,
    data_channel_open: bool,
    auxiliary_data_channels: HashMap<String, Arc<dyn DataChannel>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

/// Phase 1: per-peer SCTP send-buffer backpressure signal, driven by the data
/// channel's OnBufferedAmountHigh / OnBufferedAmountLow events (webrtc-rs has
/// no buffered-amount getter). `high` is set when buffered data crosses the
/// high watermark and cleared — waking `low_notify` — when it drops below the
/// low watermark, so senders pause instead of overrunning the channel and
/// being killed by the browser.
struct PeerBackpressure {
    high: std::sync::atomic::AtomicBool,
    low_notify: tokio::sync::Notify,
}

impl PeerBackpressure {
    fn new() -> Self {
        Self {
            high: std::sync::atomic::AtomicBool::new(false),
            low_notify: tokio::sync::Notify::new(),
        }
    }

    fn set_high(&self) {
        self.high.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn clear_high(&self) {
        self.high.store(false, std::sync::atomic::Ordering::SeqCst);
        // Wake every sender parked on the low-water notification.
        self.low_notify.notify_waiters();
    }

    fn is_high(&self) -> bool {
        self.high.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// FIX 5: result of a once-only per-peer connection build. `RxError` is
/// `Clone`, so concurrent followers awaiting the same `OnceCell` all receive
/// the same outcome.
type BuildOutcome = Result<Arc<dyn PeerConnection>, RxError>;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TransferStateKey {
    peer: PeerId,
    transfer_id: String,
}

impl TransferStateKey {
    fn new(peer: &PeerId, transfer_id: &str) -> Self {
        Self {
            peer: peer.clone(),
            transfer_id: transfer_id.to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PendingFrameAckKey {
    transfer: TransferStateKey,
    ack_seq: usize,
}

impl PendingFrameAckKey {
    fn new(peer: &PeerId, transfer_id: &str, ack_seq: usize) -> Self {
        Self {
            transfer: TransferStateKey::new(peer, transfer_id),
            ack_seq,
        }
    }
}

struct IncomingFrame {
    peer: PeerId,
    attempt: u64,
    total_frames: usize,
    total_bytes: usize,
    received_bytes: usize,
    next_ack_seq: usize,
    received: Vec<Option<String>>,
}

struct CompletedFrameAck {
    peer: PeerId,
    ack_seq: usize,
    received_frames: usize,
    /// Wall-clock insertion time (`now_ms`) used for TTL eviction. See
    /// `record_completed_frame_ack`/`prune_completed_frame_acks`.
    inserted_at_ms: u64,
}

struct PendingFrameAck {
    sender: tokio::sync::oneshot::Sender<()>,
    sent_at_ms: u64,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SendPriority {
    High,
    Normal,
    Low,
}

impl SendPriority {
    fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }
}

struct QueuedSend {
    text: String,
    priority: SendPriority,
    /// Phase 2: the collection this frame belongs to (from the wire
    /// `collection` field), retained so the queue can re-bucket the frame when
    /// the peer's active-collection set changes (`rxdb.activeCollections`).
    /// `None` for control / handshake frames that are not collection-scoped.
    collection: Option<String>,
    /// Phase 2: whether the frame's INTRINSIC priority is High regardless of
    /// the active set (control frames: responses + handshake). Such frames must
    /// never be demoted when the active-collection set changes.
    intrinsic_high: bool,
    /// Phase 2: whether the frame is an oversized `masterWrite` that should
    /// stay Low (a large background transfer) even if its collection is active.
    oversized_write: bool,
    queued_at_ms: u64,
    result: tokio::sync::oneshot::Sender<Result<(), RxError>>,
}

impl QueuedSend {
    /// Phase 2: (re)classify this frame's priority against the supplied
    /// active-collection set. Control frames stay High; oversized background
    /// writes stay Low; otherwise a frame whose collection is active is High
    /// and everything else is Normal. Centralizing this here keeps `push` and
    /// the `rxdb.activeCollections`-driven re-bucket in lockstep.
    fn classify_against(&self, active: &HashSet<String>) -> SendPriority {
        if self.intrinsic_high {
            return SendPriority::High;
        }
        if self.oversized_write {
            return SendPriority::Low;
        }
        match &self.collection {
            Some(name) if active.contains(name) => SendPriority::High,
            _ => SendPriority::Normal,
        }
    }
}

#[derive(Default)]
struct PeerSendQueue {
    high: VecDeque<QueuedSend>,
    normal: VecDeque<QueuedSend>,
    low: VecDeque<QueuedSend>,
    draining: bool,
    drain_available: Arc<tokio::sync::Notify>,
    queued_bytes: usize,
    schedule_cursor: usize,
    consecutive_browser_live: usize,
}

impl PeerSendQueue {
    fn push(&mut self, item: QueuedSend) {
        self.queued_bytes = self.queued_bytes.saturating_add(item.text.len());
        match item.priority {
            SendPriority::High
                if matches!(
                    item.collection.as_deref(),
                    Some(CTOX_BROWSER_LIVE_RESPONSE_COLLECTION)
                        | Some(CTOX_BROWSER_INPUT_RESPONSE_COLLECTION)
                ) =>
            {
                // Interactive Browser responses must not wait behind a cold
                // start's backlog of ordinary High-priority master responses.
                // They still use the single ACK-safe drainer, so an in-flight
                // framed transfer is never interleaved or corrupted.
                self.high.push_front(item)
            }
            SendPriority::High => self.high.push_back(item),
            SendPriority::Normal => self.normal.push_back(item),
            SendPriority::Low => self.low.push_back(item),
        }
    }

    fn pop_next(&mut self) -> Option<QueuedSend> {
        for _ in 0..FAIR_SEND_SCHEDULE.len() {
            let priority = FAIR_SEND_SCHEDULE[self.schedule_cursor % FAIR_SEND_SCHEDULE.len()];
            self.schedule_cursor = (self.schedule_cursor + 1) % FAIR_SEND_SCHEDULE.len();
            let item = match priority {
                SendPriority::High => self.pop_high_fair(),
                SendPriority::Normal => {
                    let item = self.normal.pop_front();
                    if item.is_some() {
                        self.consecutive_browser_live = 0;
                    }
                    item
                }
                SendPriority::Low => {
                    let item = self.low.pop_front();
                    if item.is_some() {
                        self.consecutive_browser_live = 0;
                    }
                    item
                }
            };
            if let Some(item) = item {
                self.queued_bytes = self.queued_bytes.saturating_sub(item.text.len());
                return Some(item);
            }
        }
        None
    }

    /// Pop one small High-priority frame that is safe to interleave between
    /// windows of an already-running chunked transfer. A second chunked frame
    /// must stay queued: nesting two transfer state machines on the same data
    /// channel would make their ACK/retry lifecycles contend. This mirrors the
    /// browser transport's `drainHighPriorityInlineFrames` behavior.
    fn pop_high_priority_inline(&mut self) -> Option<QueuedSend> {
        let index = self
            .high
            .iter()
            .position(|item| item.text.len() <= MAX_INLINE_FRAME_BYTES)?;
        let item = self.high.remove(index)?;
        self.queued_bytes = self.queued_bytes.saturating_sub(item.text.len());
        Some(item)
    }

    fn pop_high_fair(&mut self) -> Option<QueuedSend> {
        // Human input is sparse and bounded, and must not inherit the fairness
        // delay deliberately imposed on the continuous JPEG stream. It may
        // overtake both a queued frame and one ordinary sync response. Keep the
        // live streak unchanged so the next frame still yields to sync work.
        if let Some(index) = self.high.iter().position(|item| {
            item.collection.as_deref() == Some(CTOX_BROWSER_INPUT_RESPONSE_COLLECTION)
        }) {
            return self.high.remove(index);
        }
        let front_is_browser_live = self.high.front().is_some_and(|item| {
            item.collection.as_deref() == Some(CTOX_BROWSER_LIVE_RESPONSE_COLLECTION)
        });
        let item = if front_is_browser_live && self.consecutive_browser_live > 0 {
            self.high
                .iter()
                .position(|item| {
                    item.collection.as_deref() != Some(CTOX_BROWSER_LIVE_RESPONSE_COLLECTION)
                })
                .and_then(|index| self.high.remove(index))
                .or_else(|| self.high.pop_front())
        } else {
            self.high.pop_front()
        };
        if item.as_ref().is_some_and(|item| {
            item.collection.as_deref() == Some(CTOX_BROWSER_LIVE_RESPONSE_COLLECTION)
        }) {
            self.consecutive_browser_live = self.consecutive_browser_live.saturating_add(1);
        } else if item.is_some() {
            self.consecutive_browser_live = 0;
        }
        item
    }

    /// Phase 2: re-bucket every still-queued frame against a new
    /// active-collection set. Frames whose collection just became active jump
    /// from Normal → High; frames whose collection left the active set drop
    /// High → Normal. FIFO order WITHIN a bucket is preserved by re-pushing in
    /// the original High→Normal→Low drain order. Control frames (intrinsic
    /// High) and oversized background writes (Low) are unaffected.
    fn reprioritize(&mut self, active: &HashSet<String>) {
        let mut items: Vec<QueuedSend> =
            Vec::with_capacity(self.high.len() + self.normal.len() + self.low.len());
        items.extend(self.high.drain(..));
        items.extend(self.normal.drain(..));
        items.extend(self.low.drain(..));
        self.queued_bytes = 0;
        for mut item in items.into_iter() {
            item.priority = item.classify_against(active);
            self.push(item);
        }
    }
}

/// Cancellation guard for `drain_send_queue`: if the draining task is aborted
/// mid-send, `Drop` re-opens the drain slot so the next sender resumes the
/// queue instead of parking forever behind a `draining` flag nobody owns.
struct DrainResetGuard {
    queues: Arc<Mutex<HashMap<WebRTCRsPeer, PeerSendQueue>>>,
    peer: WebRTCRsPeer,
    available: Arc<tokio::sync::Notify>,
    in_flight: Arc<std::sync::atomic::AtomicBool>,
    armed: bool,
}

/// A receipt may be delivered by inline preemption while this drainer is
/// transmitting a different queued message. Finishing the caller is safe only
/// between complete messages; otherwise select! drops that foreign transfer.
struct QueuedTransferGuard(Arc<std::sync::atomic::AtomicBool>);

impl QueuedTransferGuard {
    fn new(active: &Arc<std::sync::atomic::AtomicBool>) -> Self {
        active.store(true, Ordering::SeqCst);
        Self(Arc::clone(active))
    }
}

impl Drop for QueuedTransferGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Drop for DrainResetGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Some(queue) = self.queues.lock().get_mut(&self.peer) {
            // The same peer ID may already own a replacement connection.
            if Arc::ptr_eq(&queue.drain_available, &self.available) {
                queue.draining = false;
            }
        }
        self.available.notify_waiters();
    }
}

/// Every sender waits for its own receipt and can take over an abandoned drain.
/// There is no detached sender task: dropping a request releases the turn and
/// wakes the other requests already queued on this connection.
async fn await_queued_send<F, D>(
    queues: &Arc<Mutex<HashMap<WebRTCRsPeer, PeerSendQueue>>>,
    peer: &WebRTCRsPeer,
    available: Arc<tokio::sync::Notify>,
    mut receipt: tokio::sync::oneshot::Receiver<Result<(), RxError>>,
    mut drain: F,
) -> Result<Result<(), RxError>, tokio::sync::oneshot::error::RecvError>
where
    F: FnMut(DrainResetGuard) -> D,
    D: std::future::Future<Output = ()>,
{
    loop {
        let notified = available.notified();
        tokio::pin!(notified);
        // Register before checking the slot so cancellation cannot lose a wakeup.
        notified.as_mut().enable();
        let in_flight = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let guard = {
            let mut queues_lock = queues.lock();
            queues_lock.get_mut(peer).and_then(|queue| {
                if queue.draining || !Arc::ptr_eq(&queue.drain_available, &available) {
                    None
                } else {
                    queue.draining = true;
                    Some(DrainResetGuard {
                        queues: Arc::clone(queues),
                        peer: peer.clone(),
                        available: Arc::clone(&available),
                        in_flight: Arc::clone(&in_flight),
                        armed: true,
                    })
                }
            })
        };
        if let Some(guard) = guard {
            tokio::select! {
                biased;
                result = std::future::poll_fn(|cx| {
                    if in_flight.load(Ordering::SeqCst) {
                        std::task::Poll::Pending
                    } else {
                        std::future::Future::poll(std::pin::Pin::new(&mut receipt), cx)
                    }
                }) => return result,
                () = drain(guard) => {}
            }
        } else {
            tokio::select! {
                result = &mut receipt => return result,
                () = &mut notified => {}
            }
        }
    }
}

/// Drop-based accounting for one outbound chunked transfer. This guard is kept
/// alive across every await in `send_framed_text`, so errors and task aborts both
/// release the active-transfer count and any ACK waiter owned by the operation.
struct FramedTransferGuard {
    transport_status: Arc<Mutex<WebRtcFrameTransportStatus>>,
    pending_frame_acks: Arc<Mutex<HashMap<PendingFrameAckKey, PendingFrameAck>>>,
    transfer: TransferStateKey,
}

impl FramedTransferGuard {
    fn new(handler: &WebRTCRsConnectionHandler, peer: &PeerId, transfer_id: &str) -> Self {
        handler.record_status(|status| {
            status.active_transfers = status.active_transfers.saturating_add(1);
        });
        Self {
            transport_status: Arc::clone(&handler.transport_status),
            pending_frame_acks: Arc::clone(&handler.pending_frame_acks),
            transfer: TransferStateKey::new(peer, transfer_id),
        }
    }
}

impl Drop for FramedTransferGuard {
    fn drop(&mut self) {
        self.pending_frame_acks
            .lock()
            .retain(|key, _| key.transfer != self.transfer);
        let pending_acks = self.pending_frame_acks.lock().len();
        let mut status = self.transport_status.lock();
        status.active_transfers = status.active_transfers.saturating_sub(1);
        status.pending_acks = pending_acks;
        status.updated_at_ms = now_ms();
    }
}

#[derive(Clone, Debug)]
pub struct WebRtcFrameTransportStatus {
    pub protocol: &'static str,
    pub max_inline_frame_bytes: usize,
    pub max_chunk_bytes: usize,
    pub max_transfer_bytes: usize,
    pub ack_window: usize,
    pub active_transfers: usize,
    pub pending_acks: usize,
    pub incoming_transfers: usize,
    pub completed_ack_cache_size: usize,
    pub sent_frames: u64,
    pub sent_bytes: u64,
    pub received_frames: u64,
    pub received_bytes: u64,
    pub retry_count: u64,
    pub resume_request_count: u64,
    pub resume_ack_count: u64,
    pub backpressure_wait_count: u64,
    pub backpressure_stall_count: u64,
    pub queued_frames: u64,
    pub sent_scheduled_frames: u64,
    pub priority_queue_depth: usize,
    pub high_priority_queue_depth: usize,
    pub normal_priority_queue_depth: usize,
    pub low_priority_queue_depth: usize,
    pub queued_bytes: usize,
    pub rejected_frames: u64,
    pub oldest_queued_age_ms: u64,
    pub peer_count: usize,
    pub open_data_channels: usize,
    pub signaling_socket_connected: bool,
    pub signaling_join_accepted: bool,
    pub turn_configured: bool,
    pub credentialed_turn_ready: bool,
    pub last_send_priority: &'static str,
    pub last_ack_lag_ms: u64,
    pub last_buffered_amount: u64,
    pub updated_at_ms: u64,
}

impl Default for WebRtcFrameTransportStatus {
    fn default() -> Self {
        Self {
            protocol: CTOX_FRAME_PROTOCOL,
            max_inline_frame_bytes: MAX_INLINE_FRAME_BYTES,
            max_chunk_bytes: MAX_CHUNK_BYTES,
            max_transfer_bytes: MAX_TRANSFER_BYTES,
            ack_window: FRAME_ACK_WINDOW,
            active_transfers: 0,
            pending_acks: 0,
            incoming_transfers: 0,
            completed_ack_cache_size: 0,
            sent_frames: 0,
            sent_bytes: 0,
            received_frames: 0,
            received_bytes: 0,
            retry_count: 0,
            resume_request_count: 0,
            resume_ack_count: 0,
            backpressure_wait_count: 0,
            backpressure_stall_count: 0,
            queued_frames: 0,
            sent_scheduled_frames: 0,
            priority_queue_depth: 0,
            high_priority_queue_depth: 0,
            normal_priority_queue_depth: 0,
            low_priority_queue_depth: 0,
            queued_bytes: 0,
            rejected_frames: 0,
            oldest_queued_age_ms: 0,
            peer_count: 0,
            open_data_channels: 0,
            signaling_socket_connected: false,
            signaling_join_accepted: false,
            turn_configured: false,
            credentialed_turn_ready: false,
            last_send_priority: "normal",
            last_ack_lag_ms: 0,
            last_buffered_amount: 0,
            updated_at_ms: now_ms(),
        }
    }
}

/// Publish a failed best-effort server push through the existing WebRTC error
/// relay without turning normal peer teardown into error-stream noise.
///
/// These two exact errors are produced when the peer was already removed or a
/// queued sender was released by `remove_peer_inner`. At the server-push sites
/// the peer had previously been open, so they mean the recipient disappeared;
/// every serialization, queue-budget, backpressure, and data-channel failure is
/// still published.
/// Marker on send errors that mean "the peer went away", set at the two sites
/// that can only fire during teardown. Best-effort senders suppress those and
/// publish everything else. It is a structured field rather than a message
/// match so the policy cannot silently break when an error text is reworded.
pub(crate) const EXPECTED_PEER_TEARDOWN_PARAM: &str = "expectedPeerTeardown";

pub(crate) fn publish_best_effort_send_error(error_subject: &RxSubject<RxError>, error: RxError) {
    let expected_peer_close = error
        .parameters()
        .get(EXPECTED_PEER_TEARDOWN_PARAM)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !expected_peer_close {
        error_subject.next(error);
    }
}

/// WebRTC connection-handler implementation backed by `webrtc-rs`.
pub struct WebRTCRsConnectionHandler {
    peer_role: super::NativePeerRole,
    connect_subject: RxSubject<WebRTCRsConnection>,
    disconnect_subject: RxSubject<WebRTCRsConnection>,
    message_subject: RxSubject<PeerWithMessage<WebRTCRsConnection>>,
    response_subject: RxSubject<PeerWithResponse<WebRTCRsConnection>>,
    error_subject: RxSubject<RxError>,
    peers: Arc<Mutex<HashMap<WebRTCRsPeer, PeerEntry>>>,
    peer_lifecycle: Arc<Mutex<()>>,
    /// FIX 5: per-peer in-flight build slots. `ensure_peer_connection` is
    /// called concurrently from the peer-list task and `handle_signal`; both
    /// could see an empty `peers` map, build a connection, and the second
    /// insert would overwrite the first — orphaning the initiator's
    /// DataChannel/offer. We register an `OnceCell` under the `peers` lock
    /// before awaiting the build, so a second caller for the same peer awaits
    /// the winner's result instead of building a duplicate.
    building: Arc<Mutex<HashMap<WebRTCRsPeer, Arc<tokio::sync::OnceCell<BuildOutcome>>>>>,
    signaling: Option<Arc<SignalingClient>>,
    ice_servers: Vec<RTCIceServer>,
    data_channel_label: String,
    udp_bind_addr: String,
    incoming_frames: Arc<Mutex<HashMap<TransferStateKey, IncomingFrame>>>,
    completed_frame_acks: Arc<Mutex<HashMap<TransferStateKey, CompletedFrameAck>>>,
    pending_frame_acks: Arc<Mutex<HashMap<PendingFrameAckKey, PendingFrameAck>>>,
    send_queues: Arc<Mutex<HashMap<WebRTCRsPeer, PeerSendQueue>>>,
    /// Phase 2: per-peer "active collection" set. A browser sends
    /// `rxdb.activeCollections` (params: `[[collectionNames]]`) whenever its
    /// foreground/subscribed collections change; frames whose `collection` is
    /// in this set are sent at High priority so the foreground collection's
    /// data jumps ahead of background bulk transfers on the shared DataChannel.
    active_collections: Arc<Mutex<HashMap<WebRTCRsPeer, HashSet<String>>>>,
    /// Presence hub (ctox-presence-v1): the last `rxdb.presence.update` report
    /// per peer, IN MEMORY ONLY. Presence is advisory UX state ("X is editing
    /// this record"), never persisted, never authoritative for policy, and
    /// must not touch SQLite — idle stays idle. Aggregates of all OTHER peers'
    /// live entries are pushed as `presence$` response frames on change,
    /// on peer close, and once after the TTL sweep.
    presence: Arc<Mutex<HashMap<WebRTCRsPeer, PeerPresenceReport>>>,
    /// TTL-sweep arming flag: at most one pending sweep task (no presence =>
    /// no task). See `schedule_presence_sweep` for why this is NOT a
    /// per-update generation counter.
    presence_sweep_armed: Arc<std::sync::atomic::AtomicBool>,
    /// The pending presence sweep is owned explicitly so `close` can abort it
    /// instead of leaving a sleeping task holding the handler alive.
    presence_sweep_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Terminal lifecycle flag checked before arming, broadcasting, or re-arming
    /// a presence sweep.
    closed: Arc<std::sync::atomic::AtomicBool>,
    /// Set when a peer with visible presence was removed outside the normal
    /// broadcast paths (abrupt disconnect -> `remove_peer`); the next sweep
    /// broadcasts the corrected aggregate even when nothing expired.
    presence_dirty: Arc<std::sync::atomic::AtomicBool>,
    transport_status: Arc<Mutex<WebRtcFrameTransportStatus>>,
    frame_counter: AtomicU64,
    peer_generation_counter: AtomicU64,
    /// Phase 1: per-peer send-buffer backpressure (see `PeerBackpressure`).
    backpressure: Arc<Mutex<HashMap<WebRTCRsPeer, Arc<PeerBackpressure>>>>,
    /// #12c per-collection sync read-authz. When `collection_authz` is set,
    /// `is_collection_authorized_for_peer` consults it with the capability token
    /// the peer presented at handshake (captured into `peer_capability_tokens`).
    /// `None` => no enforcement (default), so replication behavior is unchanged.
    collection_authz: Arc<Mutex<Option<CollectionAuthzHook>>>,
    collection_eager_pull: Arc<Mutex<Option<CollectionEagerPullHook>>>,
    collection_live_change: Arc<Mutex<Option<CollectionLiveChangeHook>>>,
    collection_write_authz: Arc<Mutex<Option<CollectionAuthzHook>>>,
    document_read_authz: Arc<Mutex<Option<DocumentReadAuthzHook>>>,
    document_write_authz: Arc<Mutex<Option<DocumentWriteAuthzHook>>>,
    peer_capability_tokens: Arc<Mutex<HashMap<WebRTCRsPeer, String>>>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl WebRTCRsConnectionHandler {
    /// Empty handler useful for unit tests or callers that install peers later.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::empty(None, Vec::new(), "rxdb", DEFAULT_UDP_BIND_ADDR))
    }

    pub async fn new_with_signaling(config: WebRTCRsConfig) -> RxResult<Arc<Self>> {
        let signaling = config.signaling.clone();
        let room = config.room.clone();
        let handler = Self::prepare_with_signaling(config).await?;
        if let Err(error) = signaling.join(room).await {
            let _ = handler.close().await;
            return Err(error);
        }
        Ok(handler)
    }

    /// Install signaling receivers without advertising room membership yet.
    /// Replication hosts install their pool/admission streams before calling
    /// `SignalingClient::join`, so an immediate offer cannot overtake setup.
    pub async fn prepare_with_signaling(config: WebRTCRsConfig) -> RxResult<Arc<Self>> {
        let mut handler = Self::empty(
            Some(Arc::clone(&config.signaling)),
            config.ice_servers,
            &config.data_channel_label,
            &config.udp_bind_addr,
        );
        handler.peer_role = config.peer_role;
        let handler = Arc::new(handler);
        wait_for_own_peer_id(&config.signaling).await?;
        handler.start_signaling_tasks();
        Ok(handler)
    }

    /// Explicit connection request from a configured native execution group.
    /// The general signaling peer-list loop remains passive toward browsers.
    /// Both ends may call this; the lower server-issued ID alone makes the offer.
    /// This chooses a transport initiator, never an execution owner. The caller
    /// must authenticate its configured peer key over the resulting channel.
    pub async fn connect_native_execution_peer(
        self: &Arc<Self>,
        remote_peer_id: PeerId,
    ) -> RxResult<()> {
        if self.closed.load(Ordering::Acquire) {
            return Err(new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "closed native handler cannot start execution connections"
                })),
            ));
        }

        let signaling = self.signaling.as_ref().ok_or_else(|| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({
                    "message": "native execution connection requires signaling"
                })),
            )
        })?;
        let own_peer_id = signaling.own_peer_id().ok_or_else(|| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({
                    "message": "native execution connection has no current signaling identity"
                })),
            )
        })?;
        if signaling
            .peer_role(&remote_peer_id)
            .as_deref()
            .and_then(super::NativePeerRole::from_wire)
            .is_none()
        {
            return Err(new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "explicit native execution connection rejects browser or unknown peer roles"
                })),
            ));
        }
        if own_peer_id == remote_peer_id {
            return Err(new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "native execution connection cannot target itself"
                })),
            ));
        }
        if own_peer_id < remote_peer_id {
            self.ensure_peer_connection(remote_peer_id, true).await?;
        }
        Ok(())
    }

    fn empty(
        signaling: Option<Arc<SignalingClient>>,
        ice_servers: Vec<RTCIceServer>,
        data_channel_label: &str,
        udp_bind_addr: &str,
    ) -> Self {
        Self {
            connect_subject: RxSubject::new(),
            peer_role: super::NativePeerRole::CtoxInstance,
            disconnect_subject: RxSubject::new(),
            message_subject: RxSubject::new(),
            response_subject: RxSubject::new(),
            error_subject: RxSubject::new(),
            peers: Arc::new(Mutex::new(HashMap::new())),
            peer_lifecycle: Arc::new(Mutex::new(())),
            building: Arc::new(Mutex::new(HashMap::new())),
            signaling,
            ice_servers,
            data_channel_label: data_channel_label.to_string(),
            udp_bind_addr: udp_bind_addr.to_string(),
            incoming_frames: Arc::new(Mutex::new(HashMap::new())),
            completed_frame_acks: Arc::new(Mutex::new(HashMap::new())),
            pending_frame_acks: Arc::new(Mutex::new(HashMap::new())),
            send_queues: Arc::new(Mutex::new(HashMap::new())),
            active_collections: Arc::new(Mutex::new(HashMap::new())),
            presence: Arc::new(Mutex::new(HashMap::new())),
            presence_sweep_armed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            presence_sweep_task: Mutex::new(None),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            presence_dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            transport_status: Arc::new(Mutex::new(WebRtcFrameTransportStatus::default())),
            frame_counter: AtomicU64::new(0),
            peer_generation_counter: AtomicU64::new(0),
            backpressure: Arc::new(Mutex::new(HashMap::new())),
            collection_authz: Arc::new(Mutex::new(None)),
            collection_eager_pull: Arc::new(Mutex::new(None)),
            collection_live_change: Arc::new(Mutex::new(None)),
            collection_write_authz: Arc::new(Mutex::new(None)),
            document_read_authz: Arc::new(Mutex::new(None)),
            document_write_authz: Arc::new(Mutex::new(None)),
            peer_capability_tokens: Arc::new(Mutex::new(HashMap::new())),
            tasks: Mutex::new(Vec::new()),
        }
    }

    /// #12c: install the per-collection read-authz hook. Set once right after
    /// construction, before any peer connects. `None` disables enforcement.
    pub fn set_collection_authz(&self, hook: Option<CollectionAuthzHook>) {
        *self.collection_authz.lock() = hook;
    }

    /// Install a server-authoritative gate for ordinary masterChangesSince
    /// collection scans. Bounded query/file demand methods remain available.
    pub fn set_collection_eager_pull(&self, hook: Option<CollectionEagerPullHook>) {
        *self.collection_eager_pull.lock() = hook;
    }

    /// Install a separate server-authoritative gate for bounded live master
    /// changes. This is deliberately independent from historical full pulls:
    /// an active command watcher may consume one permission-filtered terminal
    /// document while the command ledger remains demand-only.
    pub fn set_collection_live_change(&self, hook: Option<CollectionLiveChangeHook>) {
        *self.collection_live_change.lock() = hook;
    }

    /// Optional per-collection write gate. Native-owned collections can keep
    /// read replication enabled while forcing browser mutations through
    /// explicit command records.
    pub fn set_collection_write_authz(&self, hook: Option<CollectionAuthzHook>) {
        *self.collection_write_authz.lock() = hook;
    }

    /// Optional per-document read gate for user-scoped collections. When absent
    /// the master returns the original unfiltered document batches.
    pub fn set_document_read_authz(&self, hook: Option<DocumentReadAuthzHook>) {
        *self.document_read_authz.lock() = hook;
    }

    /// Optional per-document write gate. Unlike collection grants this validates
    /// the server-authoritative owner/tenant boundary of each pushed document.
    pub fn set_document_write_authz(&self, hook: Option<DocumentWriteAuthzHook>) {
        *self.document_write_authz.lock() = hook;
    }

    /// Phase 1: fetch (or lazily create) the backpressure signal for a peer.
    fn peer_backpressure(&self, peer: &WebRTCRsPeer) -> Arc<PeerBackpressure> {
        let mut map = self.backpressure.lock();
        Arc::clone(
            map.entry(peer.clone())
                .or_insert_with(|| Arc::new(PeerBackpressure::new())),
        )
    }

    fn is_current_send_queue(&self, peer: &str, available: &Arc<tokio::sync::Notify>) -> bool {
        self.send_queues
            .lock()
            .get(peer)
            .is_some_and(|queue| Arc::ptr_eq(&queue.drain_available, available))
    }

    /// Pause this queue's sender until its buffer drains or its deadline expires.
    /// A replaced queue cannot wait on or tear down its successor's connection.
    async fn wait_for_send_capacity(
        &self,
        peer: &WebRTCRsPeer,
        available: &Arc<tokio::sync::Notify>,
    ) -> RxResult<()> {
        let bp = {
            let _lifecycle = self.peer_lifecycle.lock();
            if !self.is_current_send_queue(peer, available) {
                return Err(superseded_send_queue_error(peer));
            }
            self.peer_backpressure(peer)
        };
        while bp.is_high() {
            let notified = bp.low_notify.notified();
            // Re-check after arming the waiter to avoid missing a clear that
            // raced between the load above and arming `notified`.
            if !bp.is_high() {
                break;
            }
            if tokio::time::timeout(SEND_CAPACITY_WAIT_TIMEOUT, notified)
                .await
                .is_err()
            {
                let error = new_rx_error(
                    SEND_BUFFER_STALLED_ERROR_CODE,
                    Some(serde_json::json!({
                        "message": "WebRTC send buffer remained above the high-water mark",
                        "peer": peer,
                        "timeoutMs": SEND_CAPACITY_WAIT_TIMEOUT.as_millis(),
                        "retryable": true,
                    })),
                );
                // A timed-out capacity wait is a transport failure, not
                // permission to keep filling SCTP. Removing the peer closes
                // the channel, drops every queued sender and clears all
                // per-peer backpressure state before the error is returned.
                if !remove_peer_with_error(self, peer, available, error.clone()) {
                    return Err(superseded_send_queue_error(peer));
                }
                self.record_status(|status| {
                    status.backpressure_stall_count =
                        status.backpressure_stall_count.saturating_add(1);
                    status.rejected_frames = status.rejected_frames.saturating_add(1);
                });
                self.error_subject.next(error.clone());
                return Err(error);
            }
        }
        if self.is_current_send_queue(peer, available) {
            Ok(())
        } else {
            Err(superseded_send_queue_error(peer))
        }
    }

    fn clear_peer_transfer_state(&self, peer: &WebRTCRsPeer) {
        self.pending_frame_acks
            .lock()
            .retain(|key, _| key.transfer.peer != *peer);
        self.incoming_frames
            .lock()
            .retain(|key, _| key.peer != *peer);
        self.completed_frame_acks
            .lock()
            .retain(|key, _| key.peer != *peer);
        self.refresh_dynamic_transport_status();
    }

    /// Insert a completed-frame-ack entry and opportunistically bound the cache
    /// (TTL + size), mirroring the browser twin's `cleanupCompletedFrameAcks`.
    /// The freshly inserted entry is always retained: its age is 0 (< TTL) and
    /// cap-eviction only removes the oldest entries.
    fn record_completed_frame_ack(&self, transfer_id: String, ack: CompletedFrameAck) {
        let key = TransferStateKey::new(&ack.peer, &transfer_id);
        let mut cache = self.completed_frame_acks.lock();
        cache.insert(key, ack);
        Self::prune_completed_frame_acks(&mut cache, now_ms());
    }

    fn completed_frame_ack_for(
        &self,
        transfer_id: &str,
        peer: &WebRTCRsPeer,
    ) -> Option<(i64, usize)> {
        self.completed_frame_acks
            .lock()
            .get(&TransferStateKey::new(peer, transfer_id))
            .map(|completed| (completed.ack_seq as i64, completed.received_frames))
    }

    /// Evict completed-frame-ack entries older than `COMPLETED_FRAME_ACK_TTL_MS`
    /// and cap the total at `COMPLETED_FRAME_ACK_CAP`, dropping the oldest first.
    /// A `resume` within the TTL window still finds its ack (docs §6.3); only
    /// entries past the window — which a legitimately-delayed resume no longer
    /// needs — are removed.
    fn prune_completed_frame_acks(
        cache: &mut HashMap<TransferStateKey, CompletedFrameAck>,
        now_ms: u64,
    ) {
        cache.retain(|_, ack| {
            now_ms.saturating_sub(ack.inserted_at_ms) < COMPLETED_FRAME_ACK_TTL_MS
        });
        while cache.len() > COMPLETED_FRAME_ACK_CAP {
            let oldest_key = cache
                .iter()
                .min_by_key(|(_, ack)| ack.inserted_at_ms)
                .map(|(key, _)| key.clone());
            match oldest_key {
                Some(key) => {
                    cache.remove(&key);
                }
                None => break,
            }
        }
    }

    fn take_pending_frame_acks(
        &self,
        peer: &WebRTCRsPeer,
        transfer_id: &str,
        ack_seq: Option<usize>,
    ) -> Vec<PendingFrameAck> {
        let transfer = TransferStateKey::new(peer, transfer_id);
        let mut pending = self.pending_frame_acks.lock();
        match ack_seq {
            Some(ack_seq) => pending
                .remove(&PendingFrameAckKey { transfer, ack_seq })
                .into_iter()
                .collect(),
            None => {
                let keys: Vec<PendingFrameAckKey> = pending
                    .keys()
                    .filter(|key| key.transfer == transfer)
                    .cloned()
                    .collect();
                keys.into_iter()
                    .filter_map(|key| pending.remove(&key))
                    .collect()
            }
        }
    }

    fn register_incoming_transfer(
        &self,
        peer: &WebRTCRsPeer,
        transfer_id: &str,
        attempt: u64,
        total_frames: usize,
        total_bytes: usize,
    ) -> RxResult<()> {
        if total_frames == 0
            || total_frames > 100_000
            || total_bytes > MAX_TRANSFER_BYTES
            || (total_frames > 1 && total_bytes == 0)
        {
            return Err(new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "invalid WebRTC transport frame allocation",
                    "transferId": transfer_id,
                    "totalFrames": total_frames,
                    "totalBytes": total_bytes,
                    "maxBytes": MAX_TRANSFER_BYTES,
                    "peer": peer,
                })),
            ));
        }

        let key = TransferStateKey::new(peer, transfer_id);
        let requested_bytes = incoming_frame_allocation_bytes(total_frames, total_bytes);
        let mut incoming = self.incoming_frames.lock();
        let replaced_bytes = incoming
            .get(&key)
            .map(|entry| incoming_frame_allocation_bytes(entry.total_frames, entry.total_bytes))
            .unwrap_or_default();
        let peer_count = incoming
            .keys()
            .filter(|existing| existing.peer == *peer && **existing != key)
            .count();
        let total_count = incoming
            .len()
            .saturating_sub(usize::from(incoming.contains_key(&key)));
        let peer_bytes = incoming
            .iter()
            .filter(|(existing, _)| existing.peer == *peer && **existing != key)
            .map(|(_, entry)| {
                incoming_frame_allocation_bytes(entry.total_frames, entry.total_bytes)
            })
            .fold(0usize, usize::saturating_add);
        let total_allocated_bytes = incoming
            .values()
            .map(|entry| incoming_frame_allocation_bytes(entry.total_frames, entry.total_bytes))
            .fold(0usize, usize::saturating_add)
            .saturating_sub(replaced_bytes);

        let exceeds_budget = peer_count.saturating_add(1) > MAX_INCOMING_TRANSFERS_PER_PEER
            || total_count.saturating_add(1) > MAX_INCOMING_TRANSFERS_TOTAL
            || peer_bytes.saturating_add(requested_bytes) > MAX_INCOMING_ALLOCATED_BYTES_PER_PEER
            || total_allocated_bytes.saturating_add(requested_bytes)
                > MAX_INCOMING_ALLOCATED_BYTES_TOTAL;
        if exceeds_budget {
            drop(incoming);
            self.record_status(|status| {
                status.rejected_frames = status.rejected_frames.saturating_add(1);
            });
            return Err(new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "incoming WebRTC transfer budget exceeded",
                    "transferId": transfer_id,
                    "peer": peer,
                    "peerTransfers": peer_count,
                    "totalTransfers": total_count,
                    "peerAllocatedBytes": peer_bytes,
                    "totalAllocatedBytes": total_allocated_bytes,
                    "requestedAllocatedBytes": requested_bytes,
                    "maxPeerTransfers": MAX_INCOMING_TRANSFERS_PER_PEER,
                    "maxTotalTransfers": MAX_INCOMING_TRANSFERS_TOTAL,
                    "maxPeerAllocatedBytes": MAX_INCOMING_ALLOCATED_BYTES_PER_PEER,
                    "maxTotalAllocatedBytes": MAX_INCOMING_ALLOCATED_BYTES_TOTAL,
                })),
            ));
        }

        incoming.insert(
            key.clone(),
            IncomingFrame {
                peer: peer.clone(),
                attempt,
                total_frames,
                total_bytes,
                received_bytes: 0,
                next_ack_seq: usize::min(FRAME_ACK_WINDOW - 1, total_frames - 1),
                received: vec![None; total_frames],
            },
        );
        drop(incoming);
        self.completed_frame_acks.lock().remove(&key);
        self.refresh_dynamic_transport_status();
        Ok(())
    }

    fn start_signaling_tasks(self: &Arc<Self>) {
        let Some(signaling) = self.signaling.as_ref().cloned() else {
            return;
        };

        let handler = Arc::clone(self);
        let signaling_for_peers = Arc::clone(&signaling);
        let mut peer_list_stream = signaling.peer_list_stream();
        let peer_task = tokio::spawn(async move {
            while let Some(peer_ids) = peer_list_stream.next().await {
                let own_peer_id = signaling_for_peers.own_peer_id();
                for remote_peer_id in peer_ids {
                    if Some(remote_peer_id.as_str()) == own_peer_id.as_deref()
                        || handler.peers.lock().contains_key(&remote_peer_id)
                    {
                        continue;
                    }
                    // Business OS browsers initiate RTC connections. The native
                    // peer must not pre-register a passive PeerConnection from
                    // the peer-list alone: doing so can make the later browser
                    // offer hit the fast path in `ensure_peer_connection` and
                    // never receive an answer. The responder is created when
                    // the actual offer arrives in `handle_signal`.
                }
            }
        });
        self.tasks.lock().push(peer_task);

        let handler = Arc::clone(self);
        let mut signal_stream = signaling.server_messages_stream();
        let signal_task = tokio::spawn(async move {
            while let Some(frame) = signal_stream.next().await {
                let ServerToClient::Signal {
                    sender_peer_id,
                    data,
                    ..
                } = frame
                else {
                    continue;
                };
                if let Err(err) = handler.handle_signal(sender_peer_id, data).await {
                    handler.error_subject.next(err);
                }
            }
        });
        self.tasks.lock().push(signal_task);
    }

    pub fn frame_transport_status(&self) -> WebRtcFrameTransportStatus {
        let mut status = self.transport_status.lock().clone();
        let peers = self.peers.lock();
        status.peer_count = peers.len();
        status.open_data_channels = peers
            .values()
            .filter(|entry| entry.data_channel_open)
            .count();
        drop(peers);
        status.signaling_socket_connected = self
            .signaling
            .as_ref()
            .is_some_and(|signaling| signaling.socket_connected());
        status.signaling_join_accepted = self
            .signaling
            .as_ref()
            .is_some_and(|signaling| signaling.join_accepted());
        status.turn_configured = self.ice_servers.iter().any(|server| {
            server
                .urls
                .iter()
                .any(|url| url.starts_with("turn:") || url.starts_with("turns:"))
        });
        status.credentialed_turn_ready = self.ice_servers.iter().any(|server| {
            server
                .urls
                .iter()
                .any(|url| url.starts_with("turn:") || url.starts_with("turns:"))
                && !server.username.trim().is_empty()
                && !server.credential.trim().is_empty()
        });
        status.pending_acks = self.pending_frame_acks.lock().len();
        status.incoming_transfers = self.incoming_frames.lock().len();
        status.completed_ack_cache_size = self.completed_frame_acks.lock().len();
        let mut high = 0usize;
        let mut normal = 0usize;
        let mut low = 0usize;
        let mut queued_bytes = 0usize;
        let mut oldest_queued_at_ms: Option<u64> = None;
        for queue in self.send_queues.lock().values() {
            high += queue.high.len();
            normal += queue.normal.len();
            low += queue.low.len();
            queued_bytes = queued_bytes.saturating_add(queue.queued_bytes);
            for item in queue.high.iter().chain(&queue.normal).chain(&queue.low) {
                oldest_queued_at_ms = Some(
                    oldest_queued_at_ms
                        .map(|current| current.min(item.queued_at_ms))
                        .unwrap_or(item.queued_at_ms),
                );
            }
        }
        status.priority_queue_depth = high + normal + low;
        status.high_priority_queue_depth = high;
        status.normal_priority_queue_depth = normal;
        status.low_priority_queue_depth = low;
        status.queued_bytes = queued_bytes;
        status.oldest_queued_age_ms = oldest_queued_at_ms
            .map(|queued_at| now_ms().saturating_sub(queued_at))
            .unwrap_or_default();
        status
    }

    pub fn frame_transport_status_json(&self) -> Value {
        let status = self.frame_transport_status();
        serde_json::json!({
            "protocol": status.protocol,
            "maxInlineFrameBytes": status.max_inline_frame_bytes,
            "maxChunkBytes": status.max_chunk_bytes,
            "maxTransferBytes": status.max_transfer_bytes,
            "ackWindow": status.ack_window,
            "activeTransfers": status.active_transfers,
            "pendingAcks": status.pending_acks,
            "incomingTransfers": status.incoming_transfers,
            "completedAckCacheSize": status.completed_ack_cache_size,
            "sentFrames": status.sent_frames,
            "sentBytes": status.sent_bytes,
            "receivedFrames": status.received_frames,
            "receivedBytes": status.received_bytes,
            "retryCount": status.retry_count,
            "resumeRequestCount": status.resume_request_count,
            "resumeAckCount": status.resume_ack_count,
            "backpressureWaitCount": status.backpressure_wait_count,
            "backpressureStallCount": status.backpressure_stall_count,
            "queuedFrames": status.queued_frames,
            "sentScheduledFrames": status.sent_scheduled_frames,
            "priorityQueueDepth": status.priority_queue_depth,
            "highPriorityQueueDepth": status.high_priority_queue_depth,
            "normalPriorityQueueDepth": status.normal_priority_queue_depth,
            "lowPriorityQueueDepth": status.low_priority_queue_depth,
            "queuedBytes": status.queued_bytes,
            "rejectedFrames": status.rejected_frames,
            "oldestQueuedAgeMs": status.oldest_queued_age_ms,
            "peerCount": status.peer_count,
            "openDataChannels": status.open_data_channels,
            "signalingSocketConnected": status.signaling_socket_connected,
            "signalingJoinAccepted": status.signaling_join_accepted,
            "turnConfigured": status.turn_configured,
            "credentialedTurnReady": status.credentialed_turn_ready,
            "lastSendPriority": status.last_send_priority,
            "lastAckLagMs": status.last_ack_lag_ms,
            "lastBufferedAmount": status.last_buffered_amount,
            "updatedAtMs": status.updated_at_ms,
        })
    }

    async fn ensure_peer_connection(
        self: &Arc<Self>,
        remote_peer_id: PeerId,
        initiator: bool,
    ) -> RxResult<Arc<dyn PeerConnection>> {
        // Fast path: a fully-built peer already exists.
        if let Some(existing) = self
            .peers
            .lock()
            .get(&remote_peer_id)
            .map(|entry| Arc::clone(&entry.peer_connection))
        {
            return Ok(existing);
        }

        // FIX 5: atomic check-and-insert. Under the `peers` lock (held just
        // long enough to also touch `building`), claim or join the per-peer
        // build slot BEFORE awaiting the connection build. The first caller to
        // arrive becomes the winner and runs the build; any concurrent caller
        // for the same peer becomes a follower and awaits the winner's result
        // via the shared `OnceCell` instead of building a duplicate that would
        // overwrite (and orphan) the winner's DataChannel/offer.
        let (cell, is_winner) = {
            // Re-check `peers` while we still hold its lock, so a connection
            // completed between the fast-path read and here is observed.
            let peers = self.peers.lock();
            if let Some(existing) = peers
                .get(&remote_peer_id)
                .map(|entry| Arc::clone(&entry.peer_connection))
            {
                return Ok(existing);
            }
            let mut building = self.building.lock();
            match building.get(&remote_peer_id) {
                Some(cell) => (Arc::clone(cell), false),
                None => {
                    let cell = Arc::new(tokio::sync::OnceCell::new());
                    building.insert(remote_peer_id.clone(), Arc::clone(&cell));
                    (cell, true)
                }
            }
        };

        // All callers (winner + followers) await the same `OnceCell`. The
        // initializer closure runs exactly once — for the winner. Followers
        // block until the winner finishes and observe the cached outcome.
        let outcome = cell
            .get_or_init(|| {
                let handler = Arc::clone(self);
                let remote_peer_id = remote_peer_id.clone();
                async move {
                    handler
                        .build_and_register_peer(remote_peer_id, initiator)
                        .await
                }
            })
            .await
            .clone();

        // The winner is responsible for clearing the in-flight slot once the
        // build has resolved (success or failure). On failure this lets a
        // later attempt rebuild; on success the `peers` map now answers the
        // fast path.
        if is_winner {
            self.building.lock().remove(&remote_peer_id);
        }

        outcome
    }

    /// FIX 5: the once-only build body extracted from `ensure_peer_connection`.
    /// Runs the connection build, registers the `PeerEntry`, and performs the
    /// initiator-side DataChannel + offer setup. Identical to the previous
    /// inline logic — only relocated so it can be driven by a per-peer
    /// `OnceCell` initializer.
    async fn build_and_register_peer(
        self: &Arc<Self>,
        remote_peer_id: PeerId,
        initiator: bool,
    ) -> RxResult<Arc<dyn PeerConnection>> {
        let signaling = self.signaling.as_ref().cloned().ok_or_else(|| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({ "message": "missing signaling client" })),
            )
        })?;

        let local_peer_id = signaling.own_peer_id();
        let generation = self
            .peer_generation_counter
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        let pc = build_peer_connection(
            Arc::clone(self),
            Arc::clone(&signaling),
            remote_peer_id.clone(),
            generation,
        )
        .await?;
        {
            let _lifecycle = self.peer_lifecycle.lock();
            self.peers.lock().insert(
                remote_peer_id.clone(),
                PeerEntry {
                    generation,
                    local_peer_id,
                    peer_connection: Arc::clone(&pc),
                    data_channel: None,
                    data_channel_open: false,
                    auxiliary_data_channels: HashMap::new(),
                    tasks: Vec::new(),
                },
            );
        }

        if initiator {
            let data_channel = pc
                .create_data_channel(&self.data_channel_label, None)
                .await
                .map_err(|e| webrtc_error("create data channel", e))?;
            install_data_channel(
                Arc::clone(self),
                remote_peer_id.clone(),
                generation,
                data_channel,
            );
            let offer = pc
                .create_offer(None)
                .await
                .map_err(|e| webrtc_error("create offer", e))?;
            pc.set_local_description(offer)
                .await
                .map_err(|e| webrtc_error("set local offer", e))?;
            if let Some(local_description) = pc.local_description().await {
                signaling
                    .send_signal(
                        remote_peer_id,
                        serde_json::to_value(local_description).unwrap_or(Value::Null),
                    )
                    .await?;
            }
        }

        Ok(pc)
    }

    async fn handle_signal(self: &Arc<Self>, remote_peer_id: PeerId, data: Value) -> RxResult<()> {
        let is_offer = data.get("type").and_then(Value::as_str) == Some("offer");
        if is_offer {
            self.remove_obsolete_peer_before_offer(&remote_peer_id);
        }
        let pc = self
            .ensure_peer_connection(remote_peer_id.clone(), false)
            .await?;
        if data.get("sdp").is_some() {
            let description: RTCSessionDescription =
                serde_json::from_value(data.clone()).map_err(|e| {
                    new_rx_error(
                        "RC_WEBRTC_SIGNAL",
                        Some(serde_json::json!({
                            "message": format!("decode SDP signal failed: {e}"),
                            "signal": data,
                        })),
                    )
                })?;
            pc.set_remote_description(description)
                .await
                .map_err(|e| webrtc_error("set remote description", e))?;
            if is_offer {
                let answer = pc
                    .create_answer(None)
                    .await
                    .map_err(|e| webrtc_error("create answer", e))?;
                pc.set_local_description(answer)
                    .await
                    .map_err(|e| webrtc_error("set local answer", e))?;
                if let (Some(signaling), Some(local_description)) =
                    (self.signaling.as_ref(), pc.local_description().await)
                {
                    signaling
                        .send_signal(
                            remote_peer_id,
                            serde_json::to_value(local_description).unwrap_or(Value::Null),
                        )
                        .await?;
                }
            }
        } else if data.get("candidate").is_some() {
            let candidate = decode_simple_peer_ice_candidate(&data).map_err(|e| {
                new_rx_error(
                    "RC_WEBRTC_SIGNAL",
                    Some(serde_json::json!({
                        "message": format!("decode ICE signal failed: {e}"),
                        "signal": data,
                    })),
                )
            })?;
            pc.add_ice_candidate(candidate)
                .await
                .map_err(|e| webrtc_error("add ice candidate", e))?;
        }
        Ok(())
    }

    fn remove_obsolete_peer_before_offer(self: &Arc<Self>, remote_peer_id: &str) {
        let current_local_peer_id = self.signaling.as_ref().and_then(|s| s.own_peer_id());
        let removal = {
            let peers = self.peers.lock();
            let Some(entry) = peers.get(remote_peer_id) else {
                return;
            };
            if current_local_peer_id.is_some() && entry.local_peer_id != current_local_peer_id {
                Some(PeerRemoval::Generation(entry.generation))
            } else if should_rebuild_peer_for_inbound_offer(true, entry.data_channel_open) {
                Some(PeerRemoval::Unopened(entry.generation))
            } else {
                None
            }
        };
        if let Some(removal) = removal {
            tracing::warn!(
                target: "ctox_rxdb::webrtc_rs",
                peer = %remote_peer_id,
                "replacing obsolete WebRTC responder before answering renewed offer"
            );
            remove_peer_inner(self, remote_peer_id, removal, None);
        }
    }
}

fn should_rebuild_peer_for_inbound_offer(peer_exists: bool, data_channel_open: bool) -> bool {
    peer_exists && !data_channel_open
}

#[async_trait]
impl WebRTCConnectionHandler for WebRTCRsConnectionHandler {
    type Peer = WebRTCRsConnection;

    fn local_peer_role(&self) -> super::NativePeerRole {
        self.peer_role
    }

    fn connect_stream(&self) -> RxStream<Self::Peer> {
        self.connect_subject.subscribe()
    }
    fn disconnect_stream(&self) -> RxStream<Self::Peer> {
        self.disconnect_subject.subscribe()
    }
    fn message_stream(&self) -> RxStream<PeerWithMessage<Self::Peer>> {
        self.message_subject.subscribe()
    }
    fn response_stream(&self) -> RxStream<PeerWithResponse<Self::Peer>> {
        self.response_subject.subscribe()
    }
    fn error_stream(&self) -> RxStream<RxError> {
        self.error_subject.subscribe()
    }

    async fn send(&self, peer: &Self::Peer, frame: WebRTCWireFrame) -> Result<(), RxError> {
        let text = serde_json::to_string(&frame).map_err(|e| {
            new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": format!("serialize WebRTC frame failed: {e}"),
                    "peer": peer.peer_id(),
                    "connectionGeneration": peer.generation(),
                })),
            )
        })?;
        // Phase 2: derive the collection-aware priority class. Control frames
        // are intrinsically High; oversized writes are Low; everything else is
        // High when its collection is in the peer's active set, else Normal.
        let class = classify_send_frame(&frame, &text);
        let (data_channel, queued) = {
            // Resolve and enqueue under the same lifetime lock. A sender holding
            // an old handle must never enqueue onto its successor's queue.
            let _lifecycle = self.peer_lifecycle.lock();
            let data_channel = self
                .peers
                .lock()
                .get(&peer.peer_id)
                .filter(|entry| entry.generation == peer.generation && entry.data_channel_open)
                .and_then(|entry| entry.data_channel.clone())
                .ok_or_else(|| stale_connection_error(peer))?;
            let queued = self.enqueue_text(&peer.peer_id, class)?;
            (data_channel, queued)
        };
        self.finish_send(&peer.peer_id, data_channel, queued).await
    }

    async fn send_auxiliary(
        &self,
        peer: &Self::Peer,
        label: &str,
        frame: WebRTCWireFrame,
    ) -> Result<(), RxError> {
        let data_channel = {
            let _lifecycle = self.peer_lifecycle.lock();
            self.peers
                .lock()
                .get(&peer.peer_id)
                .filter(|entry| entry.generation == peer.generation && entry.data_channel_open)
                .and_then(|entry| entry.auxiliary_data_channels.get(label).cloned())
                .ok_or_else(|| {
                    new_rx_error(
                        "RC_WEBRTC_PEER",
                        Some(serde_json::json!({
                            "message": "unknown or unopened auxiliary data channel",
                            "peer": peer.peer_id(),
                            "connectionGeneration": peer.generation(),
                            "label": label,
                            EXPECTED_PEER_TEARDOWN_PARAM: true,
                        })),
                    )
                })?
        };
        send_auxiliary_wire_frame(&data_channel, frame).await
    }

    async fn close(&self) -> Result<(), RxError> {
        self.closed.store(true, Ordering::SeqCst);
        self.presence_sweep_armed.store(false, Ordering::SeqCst);
        let presence_task = self.presence_sweep_task.lock().take();
        if let Some(task) = presence_task {
            task.abort();
            let _ = task.await;
        }
        self.presence.lock().clear();
        self.presence_dirty.store(false, Ordering::SeqCst);
        let tasks = std::mem::take(&mut *self.tasks.lock());
        for task in tasks {
            task.abort();
        }
        let peers = std::mem::take(&mut *self.peers.lock());
        for (peer, mut entry) in peers {
            for task in entry.tasks.drain(..) {
                task.abort();
            }
            if let Some(data_channel) = entry.data_channel {
                let _ = data_channel.close().await;
            }
            let _ = entry.peer_connection.close().await;
            self.disconnect_subject
                .next(WebRTCRsConnection::new(peer, entry.generation));
        }
        if let Some(signaling) = &self.signaling {
            signaling.close().await;
        }
        self.send_queues.lock().clear();
        self.backpressure.lock().clear();
        self.pending_frame_acks.lock().clear();
        self.incoming_frames.lock().clear();
        self.completed_frame_acks.lock().clear();
        self.active_collections.lock().clear();
        self.refresh_send_queue_status();
        Ok(())
    }

    /// Phase 1: report the peer over the high watermark so the V1.5 demand
    /// dispatchers (query/file fetch) actually engage their backpressure
    /// backoff. webrtc-rs gives no exact byte count, so we report a value above
    /// the high water when buffered (and 0 otherwise) — enough for the
    /// `buffered_bytes > WEBRTC_BUFFERED_HIGH_WATER` guards to fire.
    fn buffered_bytes(&self, peer: &Self::Peer) -> usize {
        let _connection_lifecycle = self.peer_lifecycle.lock();
        if !self.is_current_connection(peer) {
            return 0;
        }
        let peer = &peer.peer_id;
        match self.backpressure.lock().get(peer) {
            Some(bp) if bp.is_high() => DATA_CHANNEL_BUFFERED_HIGH_WATER as usize + 1,
            _ => 0,
        }
    }

    /// Phase 1: the signaling peer id is already a stable string; use it
    /// directly for authz / rate-limit keying instead of the opaque Debug form.
    fn peer_identity(&self, peer: &Self::Peer) -> String {
        peer.peer_id.clone()
    }

    fn connection_identity(&self, peer: &Self::Peer) -> String {
        peer.to_string()
    }

    fn is_peer_current(&self, peer: &Self::Peer) -> bool {
        self.is_current_connection(peer)
    }

    fn is_collection_active_for_peer(&self, peer: &Self::Peer, collection: &str) -> bool {
        let _connection_lifecycle = self.peer_lifecycle.lock();
        if !self.is_current_connection(peer) {
            return false;
        }
        let peer = &peer.peer_id;
        // Fail-open contract: a peer that has NEVER reported an active set is
        // treated as all-active. Master-change relays are DROPPED for
        // inactive collections, so a fail-closed default silently lost every
        // event in the handshake→first-`rxdb.activeCollections` window (the
        // browser stayed stale forever because pulls are event-driven). Once
        // the peer reports a set it is authoritative; (re-)activation
        // catch-up is covered by the resync push in the message loop.
        self.active_collections
            .lock()
            .get(peer)
            .map(|active| active.contains(collection))
            .unwrap_or(true)
    }

    fn is_inactive_live_change_authorized_for_peer(
        &self,
        peer: &Self::Peer,
        collection: &str,
    ) -> bool {
        let hook = self.collection_live_change.lock().clone();
        self.evaluate_current_peer_policy(peer, |token| match hook {
            None => false,
            Some(check) => check(token, collection),
        })
        .unwrap_or(false)
    }

    fn set_peer_capability_token(&self, peer: &Self::Peer, token: String) {
        let _connection_lifecycle = self.peer_lifecycle.lock();
        if !self.is_current_connection(peer) {
            return;
        }
        let peer = &peer.peer_id;
        self.peer_capability_tokens
            .lock()
            .insert(peer.clone(), token);
    }

    fn peer_capability_token(&self, peer: &Self::Peer) -> Option<String> {
        let _connection_lifecycle = self.peer_lifecycle.lock();
        if !self.is_current_connection(peer) {
            return None;
        }
        let peer = &peer.peer_id;
        self.peer_capability_tokens.lock().get(peer).cloned()
    }

    /// #12c: fail-open when no authz hook is installed (the default — behavior
    /// unchanged). When installed, an unknown peer maps to an empty token so the
    /// hook still decides (it treats an empty/invalid token as least privilege).
    fn is_collection_authorized_for_peer(&self, peer: &Self::Peer, collection: &str) -> bool {
        let hook = self.collection_authz.lock().clone();
        self.evaluate_current_peer_policy(peer, |token| match hook {
            None => true,
            Some(check) => check(token, collection),
        })
        .unwrap_or(false)
    }

    fn is_eager_collection_pull_authorized_for_peer(
        &self,
        peer: &Self::Peer,
        collection: &str,
    ) -> bool {
        let hook = self.collection_eager_pull.lock().clone();
        self.evaluate_current_peer_policy(peer, |token| match hook {
            None => true,
            Some(check) => check(token, collection),
        })
        .unwrap_or(false)
    }

    /// Fail-open write authorization unless a caller installs a write hook.
    fn is_collection_write_authorized_for_peer(&self, peer: &Self::Peer, collection: &str) -> bool {
        let hook = self.collection_write_authz.lock().clone();
        self.evaluate_current_peer_policy(peer, |token| match hook {
            None => true,
            Some(check) => check(token, collection),
        })
        .unwrap_or(false)
    }

    fn document_filter_for_peer(
        &self,
        peer: &Self::Peer,
        collection: &str,
    ) -> Option<Arc<dyn Fn(&Value) -> bool + Send + Sync>> {
        let hook = self.document_read_authz.lock().clone();
        self.evaluate_current_peer_policy(peer, |token| {
            hook.map(|read_policy| read_policy(token, collection).filter)
        })
        .unwrap_or_else(|| Some(Arc::new(|_| false)))
    }

    fn document_fields_for_peer(&self, peer: &Self::Peer, collection: &str) -> Option<Vec<String>> {
        let hook = self.document_read_authz.lock().clone();
        self.evaluate_current_peer_policy(peer, |token| {
            hook.and_then(|read_policy| read_policy(token, collection).fields)
        })
        .unwrap_or_else(|| Some(Vec::new()))
    }

    fn are_documents_write_authorized_for_peer(
        &self,
        peer: &Self::Peer,
        collection: &str,
        params: &[Value],
    ) -> bool {
        let hook = self.document_write_authz.lock().clone();
        self.evaluate_current_peer_policy(peer, |token| {
            let Some(check) = hook else { return true };
            params
                .first()
                .and_then(Value::as_array)
                .is_some_and(|rows| {
                    rows.iter().all(|row| {
                        row.get("newDocumentState")
                            .is_some_and(|document| check(token, collection, document))
                    })
                })
        })
        .unwrap_or(false)
    }

    fn filter_master_change_for_peer(
        &self,
        peer: &Self::Peer,
        collection: &str,
        change: crate::types::RxReplicationMasterChange,
    ) -> Option<crate::types::RxReplicationMasterChange> {
        let live_change_allowed = self
            .is_eager_collection_pull_authorized_for_peer(peer, collection)
            || self.is_inactive_live_change_authorized_for_peer(peer, collection);
        if !live_change_allowed {
            return None;
        }
        let Some(filter) = self.document_filter_for_peer(peer, collection) else {
            return Some(change);
        };
        match change {
            crate::types::RxReplicationMasterChange::Resync => {
                Some(crate::types::RxReplicationMasterChange::Resync)
            }
            crate::types::RxReplicationMasterChange::Documents(mut documents) => {
                documents.documents.retain(|document| filter(document));
                if let Some(fields) = self.document_fields_for_peer(peer, collection) {
                    for document in &mut documents.documents {
                        super::webrtc_types::retain_readable_fields(document, &fields);
                    }
                }
                if documents.documents.is_empty() {
                    None
                } else {
                    Some(crate::types::RxReplicationMasterChange::Documents(
                        documents,
                    ))
                }
            }
        }
    }

    /// Tear down ONE peer's transport. Emits the disconnect event, so the
    /// replication pool cleans up the peer state and the remote sees its
    /// channel close and reconnects — used by the pool to convert a failed
    /// handshake into a clean reconnect cycle instead of a half-dead peer.
    async fn close_peer(&self, peer: &Self::Peer) {
        remove_peer_generation(self, &peer.peer_id, peer.generation);
    }
}

impl WebRTCRsConnectionHandler {
    /// Phase 2: apply an inbound `rxdb.activeCollections` control frame. Parses
    /// the collection-name array from `params[0]`, replaces the peer's active
    /// set, and re-buckets anything still queued for that peer so foreground
    /// frames jump ahead immediately. Idempotent: an unchanged set is a no-op.
    ///
    /// Returns the collections that this update RE-ACTIVATED (present in the
    /// new set, absent from the previously reported one). Master-change relays
    /// for inactive collections are dropped, so a re-activated collection may
    /// have missed events — the message loop pushes a resync master-change for
    /// each returned name so the browser runs a checkpoint catch-up pull.
    /// The first report from a peer returns nothing: before it the peer was
    /// fail-open all-active (see `is_collection_active_for_peer`), so no
    /// events were dropped.
    fn apply_active_collections(
        &self,
        peer: &WebRTCRsPeer,
        message: &WebRTCMessage,
    ) -> Vec<String> {
        let names: HashSet<String> = message
            .params
            .first()
            .and_then(Value::as_array)
            .map(|arr: &Vec<Value>| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect::<HashSet<String>>()
            })
            .unwrap_or_default();
        let newly_activated: Vec<String>;
        {
            let mut active_map = self.active_collections.lock();
            match active_map.entry(peer.clone()) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    if *entry.get() == names {
                        return Vec::new();
                    }
                    newly_activated = names
                        .iter()
                        .filter(|name| !entry.get().contains(*name))
                        .cloned()
                        .collect();
                    *entry.get_mut() = names.clone();
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    // First report: the peer was fail-open all-active until
                    // now, nothing was dropped, nothing to resync.
                    entry.insert(names.clone());
                    newly_activated = Vec::new();
                }
            }
        }
        // Re-bucket the existing queue against the new active set so a frame
        // already waiting for a now-foreground collection is promoted.
        if let Some(queue) = self.send_queues.lock().get_mut(peer) {
            queue.reprioritize(&names);
        }
        self.refresh_send_queue_status();
        newly_activated
    }

    /// Apply an inbound `rxdb.presence.update` control frame (params:
    /// `[[entryObject, …]]`). Stores the report in the in-memory presence map
    /// and returns whether the peer's visible entry set CHANGED (an
    /// entry-identical refresh re-stamps the TTL clock but does not warrant a
    /// broadcast). Non-object entries are dropped; the entry count is capped
    /// at the contract's `maxEntriesPerPeer`.
    fn apply_presence(&self, peer: &WebRTCRsPeer, message: &WebRTCMessage) -> bool {
        let entries: Vec<Value> = message
            .params
            .first()
            .and_then(Value::as_array)
            .map(|arr: &Vec<Value>| {
                arr.iter()
                    .filter(|value| value.is_object())
                    .take(CTOX_PRESENCE_MAX_ENTRIES_PER_PEER)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut presence = self.presence.lock();
        if entries.is_empty() {
            return presence
                .remove(peer)
                .is_some_and(|report| !report.entries.is_empty());
        }
        let changed = presence
            .get(peer)
            .map(|report| report.entries != entries)
            .unwrap_or(true);
        presence.insert(
            peer.clone(),
            PeerPresenceReport {
                entries,
                updated_at_ms: now_ms(),
            },
        );
        changed
    }

    /// The aggregate presence a recipient should see: every OTHER peer's
    /// entries whose report is within the TTL. Sorted by serialized form so
    /// the payload is deterministic (the map iteration order is not).
    fn presence_entries_excluding(&self, recipient: &WebRTCRsPeer, now_ms: u64) -> Vec<Value> {
        let presence = self.presence.lock();
        let mut out: Vec<Value> = Vec::new();
        for (peer, report) in presence.iter() {
            if peer == recipient {
                continue;
            }
            if now_ms.saturating_sub(report.updated_at_ms) > CTOX_PRESENCE_TTL_MS {
                continue;
            }
            out.extend(report.entries.iter().cloned());
        }
        out.sort_by_cached_key(|entry| entry.to_string());
        out
    }

    /// Drop reports past the TTL. Returns whether anything was removed (the
    /// sweep only broadcasts when it actually pruned something).
    fn prune_expired_presence(&self, now_ms: u64) -> bool {
        let mut presence = self.presence.lock();
        let before = presence.len();
        presence.retain(|_, report| {
            now_ms.saturating_sub(report.updated_at_ms) <= CTOX_PRESENCE_TTL_MS
        });
        presence.len() != before
    }

    /// Remove ONE peer's presence (channel close / peer removal). Returns
    /// whether it had visible entries, i.e. whether the remaining peers need
    /// a broadcast to drop its hints.
    fn remove_peer_presence(&self, peer: &str) -> bool {
        self.presence
            .lock()
            .remove(peer)
            .is_some_and(|report| !report.entries.is_empty())
    }

    /// Push the current aggregate to ONE peer (join snapshot on data-channel
    /// open). Best-effort like the broadcast.
    async fn push_presence_snapshot_to(self: &Arc<Self>, recipient: &WebRTCRsConnection) {
        let entries = self.presence_entries_excluding(&recipient.peer_id, now_ms());
        if entries.is_empty() {
            return;
        }
        let response = WebRTCResponse {
            id: CTOX_PRESENCE_STREAM_ID.to_string(),
            result: serde_json::json!({ "entries": entries }),
            error: None,
            collection: None,
        };
        if let Err(error) = self
            .send(recipient, WebRTCWireFrame::Response(response))
            .await
        {
            publish_best_effort_send_error(&self.error_subject, error);
        }
    }

    /// Push the current presence aggregate to every open peer as a
    /// `presence$` response frame. Each recipient gets everyone's entries but
    /// its own. Best-effort: a send failure surfaces through the normal
    /// transport error path and must not stall the loop.
    async fn broadcast_presence(self: &Arc<Self>) {
        let now = now_ms();
        let recipients: Vec<WebRTCRsConnection> = self
            .peers
            .lock()
            .iter()
            .filter(|(_, entry)| entry.data_channel_open)
            .map(|(peer, entry)| WebRTCRsConnection::new(peer.clone(), entry.generation))
            .collect();
        for recipient in recipients {
            let entries = self.presence_entries_excluding(&recipient.peer_id, now);
            let response = WebRTCResponse {
                id: CTOX_PRESENCE_STREAM_ID.to_string(),
                result: serde_json::json!({ "entries": entries }),
                error: None,
                collection: None,
            };
            if let Err(error) = self
                .send(&recipient, WebRTCWireFrame::Response(response))
                .await
            {
                publish_best_effort_send_error(&self.error_subject, error);
            }
        }
    }

    /// Arm the TTL sweep. Idle discipline: at most ONE sweep task exists,
    /// and only while presence entries exist. The first design superseded
    /// the pending task on every update via a generation counter — with
    /// peers refreshing every 20s that postponed the sweep FOREVER, so a
    /// killed tab's entries never expired (found by the two-browser E2E
    /// mode). Now the armed task always fires after TTL+1s: it prunes
    /// expired reports, broadcasts when it pruned something or a peer
    /// removal marked the aggregate dirty, and re-arms only while entries
    /// remain. An empty map arms nothing and clears nothing.
    fn schedule_presence_sweep(self: &Arc<Self>) {
        let mut task_slot = self.presence_sweep_task.lock();
        if self.closed.load(Ordering::SeqCst) || self.presence.lock().is_empty() {
            return;
        }
        if self.presence_sweep_armed.swap(true, Ordering::SeqCst) {
            return; // a sweep task is already pending
        }
        let handler = Arc::clone(self);
        *task_slot = Some(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(CTOX_PRESENCE_TTL_MS + 1_000)).await;
            handler.presence_sweep_armed.store(false, Ordering::SeqCst);
            handler.presence_sweep_task.lock().take();
            if handler.closed.load(Ordering::SeqCst) {
                return;
            }
            let pruned = handler.prune_expired_presence(now_ms());
            let dirty = handler.presence_dirty.swap(false, Ordering::SeqCst);
            if (pruned || dirty) && !handler.closed.load(Ordering::SeqCst) {
                handler.broadcast_presence().await;
            }
            if !handler.closed.load(Ordering::SeqCst) {
                handler.schedule_presence_sweep();
            }
        }));
    }

    fn enqueue_text(
        &self,
        peer: &WebRTCRsPeer,
        class: SendFrameClass,
    ) -> Result<EnqueuedSend, RxError> {
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        // Phase 2: resolve the priority against THIS peer's active-collection
        // set at enqueue time. A later `rxdb.activeCollections` update will
        // re-bucket anything still queued via `PeerSendQueue::reprioritize`.
        let priority = {
            let active_map = self.active_collections.lock();
            let empty = HashSet::new();
            let active = active_map.get(peer).unwrap_or(&empty);
            class.classify(active)
        };
        let queued_bytes = class.text.len();
        let drain_available = {
            let mut queues = self.send_queues.lock();
            let queue = queues.entry(peer.clone()).or_default();
            let queued_frames = queue.high.len() + queue.normal.len() + queue.low.len();
            if queued_frames >= MAX_PEER_SEND_QUEUE_FRAMES
                || queue.queued_bytes.saturating_add(queued_bytes) > MAX_PEER_SEND_QUEUE_BYTES
            {
                self.record_status(|status| {
                    status.rejected_frames = status.rejected_frames.saturating_add(1);
                });
                return Err(new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(serde_json::json!({
                        "message": "WebRTC per-peer send queue budget exceeded",
                        "peer": peer,
                        "queuedFrames": queued_frames,
                        "queuedBytes": queue.queued_bytes,
                        "maxFrames": MAX_PEER_SEND_QUEUE_FRAMES,
                        "maxBytes": MAX_PEER_SEND_QUEUE_BYTES,
                    })),
                ));
            }
            queue.push(QueuedSend {
                text: class.text,
                priority,
                collection: class.collection,
                intrinsic_high: class.intrinsic_high,
                oversized_write: class.oversized_write,
                queued_at_ms: now_ms(),
                result: result_tx,
            });
            Arc::clone(&queue.drain_available)
        };
        self.record_status(|status| {
            status.queued_frames = status.queued_frames.saturating_add(1);
            status.last_send_priority = priority.as_str();
        });
        self.refresh_send_queue_status();
        Ok(EnqueuedSend {
            available: drain_available,
            result: result_rx,
        })
    }

    async fn finish_send(
        &self,
        peer: &WebRTCRsPeer,
        data_channel: Arc<dyn DataChannel>,
        queued: EnqueuedSend,
    ) -> Result<(), RxError> {
        await_queued_send(
            &self.send_queues,
            peer,
            queued.available,
            queued.result,
            |guard| self.drain_send_queue(peer, Arc::clone(&data_channel), guard),
        )
        .await
        .map_err(|_| {
            new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "WebRTC send queue result dropped",
                    "peer": peer,
                    EXPECTED_PEER_TEARDOWN_PARAM: true,
                })),
            )
        })?
    }

    #[cfg(test)]
    async fn send_queued_text(
        &self,
        peer: &WebRTCRsPeer,
        data_channel: Arc<dyn DataChannel>,
        class: SendFrameClass,
    ) -> Result<(), RxError> {
        self.finish_send(peer, data_channel, self.enqueue_text(peer, class)?)
            .await
    }

    async fn drain_send_queue(
        &self,
        peer: &WebRTCRsPeer,
        data_channel: Arc<dyn DataChannel>,
        mut reset_guard: DrainResetGuard,
    ) {
        // The drain slot is already claimed. Install its cancellation guard
        // BEFORE yielding; a quorum/RPC deadline can cancel this very first poll.
        tokio::task::yield_now().await;
        loop {
            let item = {
                let mut queues = self.send_queues.lock();
                let Some(queue) = queues.get_mut(peer) else {
                    // Queue removed (peer torn down) — nothing left to drain.
                    reset_guard.armed = false;
                    return;
                };
                if !Arc::ptr_eq(&queue.drain_available, &reset_guard.available) {
                    return;
                }
                match queue.pop_next() {
                    Some(item) => item,
                    None => {
                        queue.draining = false;
                        reset_guard.armed = false;
                        break;
                    }
                }
            };
            let transfer_guard = QueuedTransferGuard::new(&reset_guard.in_flight);
            self.refresh_send_queue_status();
            self.record_status(|status| {
                status.sent_scheduled_frames = status.sent_scheduled_frames.saturating_add(1);
                status.last_send_priority = item.priority.as_str();
            });
            let result = if item.text.len() > MAX_INLINE_FRAME_BYTES {
                self.send_framed_text(
                    peer,
                    Arc::clone(&data_channel),
                    item.text,
                    &reset_guard.available,
                )
                .await
            } else {
                match self
                    .wait_for_send_capacity(peer, &reset_guard.available)
                    .await
                {
                    Ok(()) => data_channel
                        .send_text(&item.text)
                        .await
                        .map_err(|e| webrtc_error("send data channel frame", e)),
                    Err(error) => Err(error),
                }
            };
            let _ = item.result.send(result);
            drop(transfer_guard);
            // Let a sender consume its receipt before starting somebody else's
            // transfer. Returning its result must not cancel that later transfer.
            tokio::task::yield_now().await;
            if !self.peers.lock().contains_key(peer) {
                break;
            }
        }
        self.refresh_send_queue_status();
    }

    async fn send_framed_text(
        &self,
        peer: &WebRTCRsPeer,
        data_channel: Arc<dyn DataChannel>,
        text: String,
        available: &Arc<tokio::sync::Notify>,
    ) -> Result<(), RxError> {
        let transfer_id = format!(
            "{}|frame|{}",
            peer,
            self.frame_counter.fetch_add(1, Ordering::SeqCst)
        );
        if text.len() > MAX_TRANSFER_BYTES {
            return Err(new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "WebRTC frame transfer exceeds max bytes",
                    "transferId": transfer_id,
                    "totalBytes": text.len(),
                    "maxBytes": MAX_TRANSFER_BYTES,
                    "peer": peer,
                })),
            ));
        }
        let chunks = split_chunks_for_frame(&text, &transfer_id);
        let _transfer_guard = FramedTransferGuard::new(self, peer, &transfer_id);

        let mut transfer_attempt = 0usize;
        let start = transport_start_frame(&transfer_id, transfer_attempt, chunks.len(), text.len());
        send_json_text(&data_channel, &start).await?;
        self.record_sent_transport_frame(&start);

        for window_start in (0..chunks.len()).step_by(FRAME_ACK_WINDOW) {
            self.drain_high_priority_inline_frames(peer, &data_channel, available)
                .await?;
            let window_end = usize::min(window_start + FRAME_ACK_WINDOW, chunks.len()) - 1;
            let ack_key = PendingFrameAckKey::new(peer, &transfer_id, window_end);
            let mut attempt = transfer_attempt;
            let mut restart_from_zero = false;
            loop {
                if restart_from_zero {
                    let restart =
                        transport_start_frame(&transfer_id, attempt, chunks.len(), text.len());
                    send_json_text(&data_channel, &restart).await?;
                    self.record_sent_transport_frame(&restart);
                }
                let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
                self.pending_frame_acks.lock().insert(
                    ack_key.clone(),
                    PendingFrameAck {
                        sender: ack_tx,
                        sent_at_ms: now_ms(),
                    },
                );
                self.refresh_dynamic_transport_status();

                for (seq, data) in chunks
                    .iter()
                    .enumerate()
                    .take(window_end + 1)
                    .skip(if restart_from_zero { 0 } else { window_start })
                {
                    // Phase 1: pace on the SCTP send buffer so a large transfer
                    // never bursts past what the channel can deliver in real
                    // time (which would overrun the buffer and get the channel
                    // killed by the browser).
                    if let Err(error) = self.wait_for_send_capacity(peer, available).await {
                        self.pending_frame_acks.lock().remove(&ack_key);
                        self.refresh_dynamic_transport_status();
                        return Err(error);
                    }
                    let chunk = transport_chunk_frame(&transfer_id, attempt, seq, data);
                    if let Err(error) = send_json_text(&data_channel, &chunk).await {
                        self.pending_frame_acks.lock().remove(&ack_key);
                        self.refresh_dynamic_transport_status();
                        return Err(error);
                    }
                    self.record_sent_transport_frame(&chunk);
                    tokio::time::sleep(SEND_FRAME_PAUSE).await;
                }

                // A chunked cold-start response can take many ACK windows. Do
                // not let it head-of-line-block small control/masterWrite
                // responses that arrive while we wait for the browser ACK.
                // Polling at 50 ms preserves the original ACK deadline while
                // keeping interactive round trips bounded.
                let deadline = tokio::time::Instant::now() + FRAME_ACK_TIMEOUT;
                tokio::pin!(ack_rx);
                let ack_result = loop {
                    self.drain_high_priority_inline_frames(peer, &data_channel, available)
                        .await?;
                    let now = tokio::time::Instant::now();
                    if now >= deadline {
                        break None;
                    }
                    tokio::select! {
                        result = &mut ack_rx => break Some(result),
                        _ = tokio::time::sleep(
                            Duration::from_millis(50).min(deadline.saturating_duration_since(now))
                        ) => {}
                    }
                };
                match ack_result {
                    Some(Ok(())) => break,
                    Some(Err(_)) => {
                        self.pending_frame_acks.lock().remove(&ack_key);
                        self.refresh_dynamic_transport_status();
                        return Err(new_rx_error(
                            "RC_WEBRTC_PEER",
                            Some(serde_json::json!({
                                "message": "WebRTC frame ack sender dropped",
                                "transferId": transfer_id,
                                "ackSeq": window_end,
                                "peer": peer,
                            })),
                        ));
                    }
                    None => {
                        self.pending_frame_acks.lock().remove(&ack_key);
                        self.refresh_dynamic_transport_status();
                        if self
                            .request_frame_resume(
                                peer,
                                Arc::clone(&data_channel),
                                &transfer_id,
                                window_end,
                                attempt,
                            )
                            .await?
                        {
                            break;
                        }
                        if attempt >= MAX_FRAME_RETRIES {
                            return Err(new_rx_error(
                                "RC_WEBRTC_PEER",
                                Some(serde_json::json!({
                                    "message": "timed out waiting for WebRTC frame ack",
                                    "transferId": transfer_id,
                                    "ackSeq": window_end,
                                    "attempt": attempt,
                                    "peer": peer,
                                })),
                            ));
                        }
                        attempt += 1;
                        transfer_attempt = attempt;
                        restart_from_zero = true;
                        self.record_status(|status| {
                            status.retry_count = status.retry_count.saturating_add(1);
                        });
                        tokio::time::sleep(Duration::from_millis(
                            u64::try_from(usize::min(250 * attempt, 1000)).unwrap_or(1000),
                        ))
                        .await;
                    }
                }
            }
        }
        Ok(())
    }

    async fn drain_high_priority_inline_frames(
        &self,
        peer: &WebRTCRsPeer,
        data_channel: &Arc<dyn DataChannel>,
        available: &Arc<tokio::sync::Notify>,
    ) -> Result<(), RxError> {
        loop {
            let item = {
                let mut queues = self.send_queues.lock();
                queues
                    .get_mut(peer)
                    .filter(|queue| Arc::ptr_eq(&queue.drain_available, available))
                    .and_then(PeerSendQueue::pop_high_priority_inline)
            };
            let Some(item) = item else {
                break;
            };
            self.refresh_send_queue_status();
            self.record_status(|status| {
                status.sent_scheduled_frames = status.sent_scheduled_frames.saturating_add(1);
                status.last_send_priority = SendPriority::High.as_str();
            });
            let send_result = match self.wait_for_send_capacity(peer, available).await {
                Ok(()) => data_channel
                    .send_text(&item.text)
                    .await
                    .map_err(|error| webrtc_error("send interleaved high-priority frame", error)),
                Err(error) => Err(error),
            };
            let failure = send_result.as_ref().err().map(|error| error.to_string());
            let _ = item.result.send(send_result);
            if let Some(message) = failure {
                return Err(new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(serde_json::json!({
                        "message": format!(
                            "interleaved high-priority frame failed during chunked transfer: {message}"
                        ),
                        "peer": peer,
                    })),
                ));
            }
        }
        Ok(())
    }

    async fn request_frame_resume(
        &self,
        peer: &WebRTCRsPeer,
        data_channel: Arc<dyn DataChannel>,
        transfer_id: &str,
        ack_seq: usize,
        attempt: usize,
    ) -> Result<bool, RxError> {
        let ack_key = PendingFrameAckKey::new(peer, transfer_id, ack_seq);
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        self.pending_frame_acks.lock().insert(
            ack_key.clone(),
            PendingFrameAck {
                sender: ack_tx,
                sent_at_ms: now_ms(),
            },
        );
        self.refresh_dynamic_transport_status();
        let resume = transport_resume_frame(transfer_id, attempt, ack_seq);
        if let Err(error) = send_json_text(&data_channel, &resume).await {
            self.pending_frame_acks.lock().remove(&ack_key);
            self.refresh_dynamic_transport_status();
            return Err(error);
        }
        self.record_sent_transport_frame(&resume);
        self.record_status(|status| {
            status.resume_request_count = status.resume_request_count.saturating_add(1);
        });
        match tokio::time::timeout(FRAME_RESUME_TIMEOUT, ack_rx).await {
            Ok(Ok(())) => Ok(true),
            Ok(Err(_)) => Err(new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "WebRTC frame resume ack sender dropped",
                    "transferId": transfer_id,
                    "ackSeq": ack_seq,
                    "peer": peer,
                })),
            )),
            Err(_) => {
                self.pending_frame_acks.lock().remove(&ack_key);
                self.refresh_dynamic_transport_status();
                Ok(false)
            }
        }
    }

    async fn handle_transport_frame(
        &self,
        peer: &WebRTCRsPeer,
        data_channel: Arc<dyn DataChannel>,
        frame: Value,
    ) -> RxResult<Option<String>> {
        self.record_received_transport_frame(&frame);
        let kind = frame.get("kind").and_then(Value::as_str).unwrap_or("");
        let transfer_id = frame
            .get("transferId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        if kind == "ack" {
            let ack_seq_i64 = frame.get("ackSeq").and_then(Value::as_i64);
            let ack_seq = ack_seq_i64.and_then(|v| usize::try_from(v).ok());
            let pending_acks = match (ack_seq, ack_seq_i64.is_none()) {
                (Some(seq), _) => self.take_pending_frame_acks(peer, &transfer_id, Some(seq)),
                (None, true) => self.take_pending_frame_acks(peer, &transfer_id, None),
                (None, false) => Vec::new(),
            };
            let is_resume = frame
                .get("resume")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            for pending in pending_acks {
                self.record_ack_lag(pending.sent_at_ms);
                if is_resume {
                    self.record_status(|status| {
                        status.resume_ack_count = status.resume_ack_count.saturating_add(1);
                    });
                }
                let _ = pending.sender.send(());
            }
            self.refresh_dynamic_transport_status();
            return Ok(None);
        }

        if transfer_id.is_empty() {
            return Err(new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "WebRTC transport frame missing transferId",
                    "peer": peer,
                    "kind": kind,
                })),
            ));
        }

        if kind == "start" {
            let total_frames = frame
                .get("totalFrames")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let total_bytes = frame.get("totalBytes").and_then(Value::as_u64).unwrap_or(0) as usize;
            self.register_incoming_transfer(
                peer,
                &transfer_id,
                frame.get("attempt").and_then(Value::as_u64).unwrap_or(0),
                total_frames,
                total_bytes,
            )?;
            return Ok(None);
        }

        if kind == "resume" {
            let completed_ack = self.completed_frame_ack_for(&transfer_id, peer);
            if let Some((ack_seq, received_frames)) = completed_ack {
                send_transport_ack(
                    &data_channel,
                    &transfer_id,
                    ack_seq,
                    received_frames,
                    true,
                    true,
                )
                .await?;
                self.record_status(|status| {
                    status.resume_ack_count = status.resume_ack_count.saturating_add(1);
                });
                return Ok(None);
            }

            let resume_ack = {
                let incoming = self.incoming_frames.lock();
                incoming
                    .get(&TransferStateKey::new(peer, &transfer_id))
                    .and_then(|entry| {
                        if entry.peer != *peer {
                            return None;
                        }
                        Some((
                            highest_contiguous_seq(&entry.received)
                                .map(|seq| seq as i64)
                                .unwrap_or(-1),
                            entry
                                .received
                                .iter()
                                .filter(|chunk| chunk.is_some())
                                .count(),
                        ))
                    })
            };
            if let Some((ack_seq, received_frames)) = resume_ack {
                send_transport_ack(
                    &data_channel,
                    &transfer_id,
                    ack_seq,
                    received_frames,
                    false,
                    true,
                )
                .await?;
                self.record_status(|status| {
                    status.resume_ack_count = status.resume_ack_count.saturating_add(1);
                });
            }
            return Ok(None);
        }

        if kind != "chunk" {
            return Err(new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "unknown WebRTC transport frame kind",
                    "transferId": transfer_id,
                    "kind": kind,
                    "peer": peer,
                })),
            ));
        }

        let seq = frame.get("seq").and_then(Value::as_u64).ok_or_else(|| {
            new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "WebRTC transport chunk missing seq",
                    "transferId": transfer_id,
                    "peer": peer,
                })),
            )
        })? as usize;
        let data = frame
            .get("data")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let attempt = frame.get("attempt").and_then(Value::as_u64).unwrap_or(0);

        let completed_ack = self.completed_frame_ack_for(&transfer_id, peer);
        if let Some((ack_seq, received_frames)) = completed_ack {
            // The sender can repeat the final window when our ACK was lost.
            // Re-ACK without decoding the already delivered payload again.
            send_transport_ack(
                &data_channel,
                &transfer_id,
                ack_seq,
                received_frames,
                true,
                false,
            )
            .await?;
            return Ok(None);
        }

        let frame_status = {
            let transfer_key = TransferStateKey::new(peer, &transfer_id);
            let mut incoming = self.incoming_frames.lock();
            let entry = incoming.get_mut(&transfer_key).ok_or_else(|| {
                new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(serde_json::json!({
                        "message": "WebRTC transport chunk arrived without start",
                        "transferId": transfer_id,
                        "peer": peer,
                    })),
                )
            })?;
            if entry.attempt != attempt {
                return Err(new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(serde_json::json!({
                        "message": "stale WebRTC transport chunk attempt",
                        "transferId": transfer_id,
                        "seq": seq,
                        "attempt": attempt,
                        "expectedAttempt": entry.attempt,
                        "peer": peer,
                    })),
                ));
            }
            if entry.peer != *peer || seq >= entry.total_frames {
                return Err(new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(serde_json::json!({
                        "message": "invalid WebRTC transport chunk",
                        "transferId": transfer_id,
                        "seq": seq,
                        "peer": peer,
                    })),
                ));
            }
            let replaced_bytes = entry.received[seq]
                .as_ref()
                .map(|chunk| chunk.len())
                .unwrap_or_default();
            let received_bytes = entry
                .received_bytes
                .saturating_sub(replaced_bytes)
                .saturating_add(data.len());
            if received_bytes > entry.total_bytes {
                return Err(new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(serde_json::json!({
                        "message": "WebRTC transport chunks exceed advertised bytes",
                        "transferId": transfer_id,
                        "advertisedBytes": entry.total_bytes,
                        "receivedBytes": received_bytes,
                        "peer": peer,
                    })),
                ));
            }
            entry.received_bytes = received_bytes;
            entry.received[seq] = Some(data);
            if entry.received.iter().any(Option::is_none) {
                let contiguous_seq = highest_contiguous_seq(&entry.received);
                if contiguous_seq
                    .map(|seq| seq >= entry.next_ack_seq && seq < entry.total_frames - 1)
                    .unwrap_or(false)
                {
                    let ack_seq = contiguous_seq.expect("checked above");
                    entry.next_ack_seq =
                        usize::min(ack_seq + FRAME_ACK_WINDOW, entry.total_frames - 1);
                    FrameReceiveStatus::WindowAck { ack_seq }
                } else {
                    FrameReceiveStatus::Pending
                }
            } else {
                let entry = incoming.remove(&transfer_key).expect("entry exists");
                let mut text = String::new();
                for chunk in entry.received {
                    text.push_str(&chunk.unwrap_or_default());
                }
                if text.len() != entry.total_bytes {
                    return Err(new_rx_error(
                        "RC_WEBRTC_PEER",
                        Some(serde_json::json!({
                            "message": "WebRTC transport frame size mismatch",
                            "transferId": transfer_id,
                            "expectedBytes": entry.total_bytes,
                            "actualBytes": text.len(),
                            "peer": peer,
                        })),
                    ));
                }
                FrameReceiveStatus::Complete {
                    text,
                    ack_seq: entry.total_frames - 1,
                }
            }
        };

        match frame_status {
            FrameReceiveStatus::Pending => Ok(None),
            FrameReceiveStatus::WindowAck { ack_seq } => {
                send_transport_ack(
                    &data_channel,
                    &transfer_id,
                    ack_seq as i64,
                    ack_seq + 1,
                    false,
                    false,
                )
                .await?;
                Ok(None)
            }
            FrameReceiveStatus::Complete { text, ack_seq } => {
                self.record_completed_frame_ack(
                    transfer_id.clone(),
                    CompletedFrameAck {
                        peer: peer.clone(),
                        ack_seq,
                        received_frames: ack_seq + 1,
                        inserted_at_ms: now_ms(),
                    },
                );
                self.refresh_dynamic_transport_status();
                send_transport_ack(
                    &data_channel,
                    &transfer_id,
                    ack_seq as i64,
                    ack_seq + 1,
                    true,
                    false,
                )
                .await?;
                Ok(Some(text))
            }
        }
    }

    fn record_sent_transport_frame(&self, frame: &Value) {
        let frame_bytes = serde_json::to_string(frame)
            .map(|text| text.len() as u64)
            .unwrap_or(0);
        self.record_status(|status| {
            status.sent_frames = status.sent_frames.saturating_add(1);
            status.sent_bytes = status.sent_bytes.saturating_add(frame_bytes);
        });
    }

    fn record_received_transport_frame(&self, frame: &Value) {
        let frame_bytes = serde_json::to_string(frame)
            .map(|text| text.len() as u64)
            .unwrap_or(0);
        self.record_status(|status| {
            status.received_frames = status.received_frames.saturating_add(1);
            status.received_bytes = status.received_bytes.saturating_add(frame_bytes);
        });
    }

    fn record_ack_lag(&self, sent_at_ms: u64) {
        let lag = now_ms().saturating_sub(sent_at_ms);
        self.record_status(|status| {
            status.last_ack_lag_ms = lag;
        });
    }

    fn refresh_dynamic_transport_status(&self) {
        let pending_acks = self.pending_frame_acks.lock().len();
        let incoming_transfers = self.incoming_frames.lock().len();
        let completed_ack_cache_size = self.completed_frame_acks.lock().len();
        self.record_status(|status| {
            status.pending_acks = pending_acks;
            status.incoming_transfers = incoming_transfers;
            status.completed_ack_cache_size = completed_ack_cache_size;
        });
    }

    fn refresh_send_queue_status(&self) {
        let mut high = 0usize;
        let mut normal = 0usize;
        let mut low = 0usize;
        let mut queued_bytes = 0usize;
        let mut oldest_queued_at_ms: Option<u64> = None;
        for queue in self.send_queues.lock().values() {
            high += queue.high.len();
            normal += queue.normal.len();
            low += queue.low.len();
            queued_bytes = queued_bytes.saturating_add(queue.queued_bytes);
            for item in queue.high.iter().chain(&queue.normal).chain(&queue.low) {
                oldest_queued_at_ms = Some(
                    oldest_queued_at_ms
                        .map(|current| current.min(item.queued_at_ms))
                        .unwrap_or(item.queued_at_ms),
                );
            }
        }
        self.record_status(|status| {
            status.priority_queue_depth = high + normal + low;
            status.high_priority_queue_depth = high;
            status.normal_priority_queue_depth = normal;
            status.low_priority_queue_depth = low;
            status.queued_bytes = queued_bytes;
            status.oldest_queued_age_ms = oldest_queued_at_ms
                .map(|queued_at| now_ms().saturating_sub(queued_at))
                .unwrap_or_default();
        });
    }

    fn record_status(&self, update: impl FnOnce(&mut WebRtcFrameTransportStatus)) {
        let mut status = self.transport_status.lock();
        update(&mut status);
        status.updated_at_ms = now_ms();
    }
}

enum FrameReceiveStatus {
    Pending,
    WindowAck { ack_seq: usize },
    Complete { text: String, ack_seq: usize },
}

async fn send_transport_ack(
    data_channel: &Arc<dyn DataChannel>,
    transfer_id: &str,
    ack_seq: i64,
    received_frames: usize,
    final_ack: bool,
    resume: bool,
) -> RxResult<()> {
    let ack = transport_ack_frame(transfer_id, ack_seq, received_frames, final_ack, resume);
    send_json_text(data_channel, &ack).await
}

fn transport_start_frame(
    transfer_id: &str,
    attempt: usize,
    total_frames: usize,
    total_bytes: usize,
) -> Value {
    serde_json::json!({
        "ctoxFrame": CTOX_FRAME_PROTOCOL,
        "kind": "start",
        "transferId": transfer_id,
        "windowSize": FRAME_ACK_WINDOW,
        "attempt": attempt,
        "totalFrames": total_frames,
        "totalBytes": total_bytes,
    })
}

fn transport_chunk_frame(transfer_id: &str, attempt: usize, seq: usize, data: &str) -> Value {
    serde_json::json!({
        "ctoxFrame": CTOX_FRAME_PROTOCOL,
        "kind": "chunk",
        "transferId": transfer_id,
        "attempt": attempt,
        "seq": seq,
        "data": data,
    })
}

fn transport_ack_frame(
    transfer_id: &str,
    ack_seq: i64,
    received_frames: usize,
    final_ack: bool,
    resume: bool,
) -> Value {
    serde_json::json!({
        "ctoxFrame": CTOX_FRAME_PROTOCOL,
        "kind": "ack",
        "transferId": transfer_id,
        "ackSeq": ack_seq,
        "receivedFrames": received_frames,
        "final": final_ack,
        "resume": resume,
    })
}

fn transport_resume_frame(transfer_id: &str, attempt: usize, ack_seq: usize) -> Value {
    serde_json::json!({
        "ctoxFrame": CTOX_FRAME_PROTOCOL,
        "kind": "resume",
        "transferId": transfer_id,
        "attempt": attempt,
        "ackSeq": ack_seq,
    })
}

struct RsPeerConnectionEvents {
    handler: Arc<WebRTCRsConnectionHandler>,
    signaling: Arc<SignalingClient>,
    remote_peer_id: PeerId,
    generation: u64,
}

#[async_trait]
impl PeerConnectionEventHandler for RsPeerConnectionEvents {
    async fn on_ice_candidate(&self, event: webrtc::peer_connection::RTCPeerConnectionIceEvent) {
        match event.candidate.to_json() {
            Ok(candidate) => {
                let data = simple_peer_ice_signal(candidate);
                if let Err(err) = self
                    .signaling
                    .send_signal(self.remote_peer_id.clone(), data)
                    .await
                {
                    self.handler.error_subject.next(err);
                }
            }
            Err(err) => self
                .handler
                .error_subject
                .next(webrtc_error("serialize ice candidate", err)),
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        // FIX 5: `Disconnected` is a TRANSIENT ICE state that very often
        // recovers on its own (e.g. brief network blips, NAT rebinding). Only
        // `Failed` and `Closed` are terminal and warrant tearing the peer
        // down. Tearing down on `Disconnected` orphaned otherwise-recoverable
        // peers and forced full re-handshakes. We keep `Disconnected` logged
        // for observability but do not remove the peer.
        match state {
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                remove_peer_generation(&self.handler, &self.remote_peer_id, self.generation);
            }
            RTCPeerConnectionState::Disconnected => {
                tracing::debug!(
                    peer = %self.remote_peer_id,
                    "webrtc peer Disconnected (transient); keeping connection for recovery"
                );
            }
            _ => {}
        }
    }

    async fn on_data_channel(&self, data_channel: Arc<dyn DataChannel>) {
        let label = data_channel.label().await.unwrap_or_default();
        if matches!(
            label.as_str(),
            "ctox-browser-live-v1" | "ctox.workjet.device.v1"
        ) {
            install_auxiliary_data_channel(
                Arc::clone(&self.handler),
                self.remote_peer_id.clone(),
                self.generation,
                label,
                data_channel,
            );
            return;
        }
        install_data_channel(
            Arc::clone(&self.handler),
            self.remote_peer_id.clone(),
            self.generation,
            data_channel,
        );
    }
}

fn install_auxiliary_data_channel(
    handler: Arc<WebRTCRsConnectionHandler>,
    remote_peer_id: PeerId,
    generation: u64,
    label: String,
    data_channel: Arc<dyn DataChannel>,
) {
    {
        let mut peers = handler.peers.lock();
        let Some(entry) = peers.get_mut(&remote_peer_id) else {
            return;
        };
        if entry.generation != generation {
            return;
        }
        entry
            .auxiliary_data_channels
            .insert(label.clone(), Arc::clone(&data_channel));
    }
    let message_subject = handler.message_subject.clone();
    let error_subject = handler.error_subject.clone();
    let handler_task = Arc::clone(&handler);
    let peer_task = remote_peer_id.clone();
    let label_task = label.clone();
    let task = tokio::spawn(async move {
        while let Some(event) = data_channel.poll().await {
            if !is_current_peer_generation(&handler_task, &peer_task, generation) {
                break;
            }
            match event {
                DataChannelEvent::OnMessage(message) => {
                    let text = String::from_utf8_lossy(&message.data).to_string();
                    match serde_json::from_str::<WebRTCMessage>(&text) {
                        Ok(message) => message_subject.next(PeerWithMessage {
                            peer: WebRTCRsConnection::new(peer_task.clone(), generation),
                            message,
                        }),
                        Err(error) => {
                            error_subject.next(decode_error("auxiliary message", error, &text))
                        }
                    }
                }
                DataChannelEvent::OnClose | DataChannelEvent::OnClosing => break,
                DataChannelEvent::OnError => error_subject.next(new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(serde_json::json!({
                        "message": "auxiliary data channel error",
                        "peer": peer_task,
                        "label": label_task,
                    })),
                )),
                DataChannelEvent::OnOpen
                | DataChannelEvent::OnBufferedAmountHigh
                | DataChannelEvent::OnBufferedAmountLow => {}
            }
        }
        if let Some(entry) = handler_task.peers.lock().get_mut(&peer_task) {
            if entry.generation == generation {
                entry.auxiliary_data_channels.remove(&label_task);
            }
        }
    });
    if let Some(entry) = handler.peers.lock().get_mut(&remote_peer_id) {
        if entry.generation == generation {
            entry.tasks.push(task);
            return;
        }
    }
    task.abort();
}

async fn build_peer_connection(
    handler: Arc<WebRTCRsConnectionHandler>,
    signaling: Arc<SignalingClient>,
    remote_peer_id: PeerId,
    generation: u64,
) -> RxResult<Arc<dyn PeerConnection>> {
    let event_handler = Arc::new(RsPeerConnectionEvents {
        handler: Arc::clone(&handler),
        signaling,
        remote_peer_id,
        generation,
    });

    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .map_err(|e| webrtc_error("register default codecs", e))?;
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .map_err(|e| webrtc_error("register default interceptors", e))?;
    let runtime = default_runtime().ok_or_else(|| {
        new_rx_error(
            "RC_WEBRTC_PEER",
            Some(serde_json::json!({ "message": "no async runtime for webrtc-rs" })),
        )
    })?;
    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(handler.ice_servers.clone())
        .build();
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_multicast_dns_mode(MulticastDnsMode::QueryOnly);

    let pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_setting_engine(setting_engine)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(event_handler)
        .with_runtime(runtime)
        .with_udp_addrs(advertisable_udp_bind_addrs(&handler.udp_bind_addr))
        .build()
        .await
        .map_err(|e| webrtc_error("build peer connection", e))?;
    Ok(Arc::new(pc))
}

fn install_data_channel(
    handler: Arc<WebRTCRsConnectionHandler>,
    remote_peer_id: PeerId,
    generation: u64,
    data_channel: Arc<dyn DataChannel>,
) {
    let backpressure_for_task = {
        let _lifecycle = handler.peer_lifecycle.lock();
        let mut peers = handler.peers.lock();
        if let Some(entry) = peers.get_mut(&remote_peer_id) {
            if entry.generation == generation {
                // A channel has exactly one event consumer per connection lifetime.
                // webrtc 0.20.0-alpha.1 also announces a locally created channel
                // through on_data_channel with a second, already-closed receiver.
                // Keep the original handle: polling the duplicate would immediately
                // retire this otherwise live connection on stream EOF.
                if entry
                    .data_channel
                    .as_ref()
                    .is_some_and(|installed| installed.id() == data_channel.id())
                {
                    return;
                }
                entry.data_channel = Some(Arc::clone(&data_channel));
            } else {
                return;
            }
        } else {
            return;
        }
        handler.peer_backpressure(&remote_peer_id)
    };

    let message_subject = handler.message_subject.clone();
    let response_subject = handler.response_subject.clone();
    let connect_subject = handler.connect_subject.clone();
    let error_subject = handler.error_subject.clone();
    let handler_for_task = Arc::clone(&handler);
    let data_channel_for_task = Arc::clone(&data_channel);
    let peer_for_task = remote_peer_id.clone();
    // Phase 1: register the per-peer backpressure signal and arm the SCTP
    // buffered-amount thresholds so the channel emits OnBufferedAmountHigh/Low
    // and senders can pace instead of overrunning the buffer.
    let task = tokio::spawn(async move {
        let _ = data_channel
            .set_buffered_amount_low_threshold(DATA_CHANNEL_BUFFERED_LOW_WATER)
            .await;
        let _ = data_channel
            .set_buffered_amount_high_threshold(DATA_CHANNEL_BUFFERED_HIGH_WATER)
            .await;
        while let Some(event) = data_channel.poll().await {
            if !is_current_peer_generation(&handler_for_task, &peer_for_task, generation) {
                break;
            }
            match event {
                DataChannelEvent::OnOpen => {
                    if let Some(entry) = handler_for_task.peers.lock().get_mut(&peer_for_task) {
                        if entry.generation == generation {
                            entry.data_channel_open = true;
                        }
                    }
                    // Presence join snapshot: broadcasts fire on CHANGE, peer
                    // close and TTL sweep — a peer that connects while other
                    // peers already publish presence would otherwise see
                    // nothing until the next change (found by the two-browser
                    // E2E mode). Push the current aggregate to the newly
                    // opened peer; a no-presence room sends nothing.
                    if !handler_for_task.presence.lock().is_empty() {
                        let handler_presence = Arc::clone(&handler_for_task);
                        let peer_presence =
                            WebRTCRsConnection::new(peer_for_task.clone(), generation);
                        tokio::spawn(async move {
                            handler_presence
                                .push_presence_snapshot_to(&peer_presence)
                                .await;
                        });
                    }
                    connect_subject
                        .next(WebRTCRsConnection::new(peer_for_task.clone(), generation));
                }
                DataChannelEvent::OnMessage(msg) => {
                    let text = String::from_utf8_lossy(&msg.data).to_string();
                    let value = match serde_json::from_str::<Value>(&text) {
                        Ok(value) => value,
                        Err(err) => {
                            error_subject.next(decode_error("frame", err, &text));
                            continue;
                        }
                    };
                    let value = if is_ctox_transport_frame(&value) {
                        match handler_for_task
                            .handle_transport_frame(
                                &peer_for_task,
                                Arc::clone(&data_channel_for_task),
                                value,
                            )
                            .await
                        {
                            Ok(Some(reassembled)) => {
                                match serde_json::from_str::<Value>(&reassembled) {
                                    Ok(value) => value,
                                    Err(err) => {
                                        error_subject.next(decode_error(
                                            "reassembled frame",
                                            err,
                                            &reassembled,
                                        ));
                                        continue;
                                    }
                                }
                            }
                            Ok(None) => continue,
                            Err(err) => {
                                error_subject.next(err);
                                continue;
                            }
                        }
                    } else {
                        value
                    };
                    // Frame reassembly can await. Bind subsequent local control
                    // state writes to the same connection generation.
                    let _lifecycle = handler_for_task.peer_lifecycle.lock();
                    if !is_current_peer_generation(&handler_for_task, &peer_for_task, generation) {
                        break;
                    }
                    if value.get("result").is_some() || value.get("error").is_some() {
                        match serde_json::from_value::<WebRTCResponse>(value) {
                            Ok(response) => response_subject.next(PeerWithResponse {
                                peer: WebRTCRsConnection::new(peer_for_task.clone(), generation),
                                response,
                            }),
                            Err(err) => error_subject.next(decode_error("response", err, &text)),
                        }
                    } else {
                        match serde_json::from_value::<WebRTCMessage>(value) {
                            Ok(message) => {
                                // Phase 2: `rxdb.activeCollections` is a
                                // transport-control frame, not a replication
                                // request. The browser sends it whenever its
                                // foreground/subscribed collections change.
                                // Apply it to the per-peer active set + re-bucket
                                // anything still queued, and do NOT forward it to
                                // the pool's message stream.
                                if message.method == ACTIVE_COLLECTIONS_METHOD {
                                    let newly_activated = handler_for_task
                                        .apply_active_collections(&peer_for_task, &message);
                                    // Re-activated collections may have missed
                                    // master-change relays while inactive
                                    // (relays are dropped, pulls are event-
                                    // driven). Push one resync master-change
                                    // per re-activated collection so the
                                    // browser runs a checkpoint catch-up pull.
                                    if !newly_activated.is_empty() {
                                        let handler_resync = Arc::clone(&handler_for_task);
                                        let peer_resync = WebRTCRsConnection::new(
                                            peer_for_task.clone(),
                                            generation,
                                        );
                                        tokio::spawn(async move {
                                            for collection in newly_activated {
                                                let resp = WebRTCResponse {
                                                    id: crate::plugins::replication_webrtc::index_mod::master_change_stream_id(&collection),
                                                    result: serde_json::json!({ "resync": true }),
                                                    error: None,
                                                    collection: Some(collection),
                                                };
                                                if let Err(error) = handler_resync
                                                    .send(
                                                        &peer_resync,
                                                        WebRTCWireFrame::Response(resp),
                                                    )
                                                    .await
                                                {
                                                    publish_best_effort_send_error(
                                                        &handler_resync.error_subject,
                                                        error,
                                                    );
                                                }
                                            }
                                        });
                                    }
                                } else if message.method == CTOX_PRESENCE_RPC_UPDATE {
                                    // Presence is a transport-control frame like
                                    // `rxdb.activeCollections`: apply it to the
                                    // in-memory hub and do NOT forward it to the
                                    // pool's message stream. Broadcast only on a
                                    // visible change (refreshes just re-stamp
                                    // the TTL clock).
                                    let changed =
                                        handler_for_task.apply_presence(&peer_for_task, &message);
                                    if changed {
                                        let handler_presence = Arc::clone(&handler_for_task);
                                        tokio::spawn(async move {
                                            handler_presence.broadcast_presence().await;
                                        });
                                    }
                                    handler_for_task.schedule_presence_sweep();
                                } else {
                                    message_subject.next(PeerWithMessage {
                                        peer: WebRTCRsConnection::new(
                                            peer_for_task.clone(),
                                            generation,
                                        ),
                                        message,
                                    });
                                }
                            }
                            Err(err) => error_subject.next(decode_error("message", err, &text)),
                        }
                    }
                }
                DataChannelEvent::OnBufferedAmountHigh => {
                    // Phase 1: SCTP send buffer crossed the high watermark —
                    // pause senders so we keep the stream real-time.
                    backpressure_for_task.set_high();
                }
                DataChannelEvent::OnBufferedAmountLow => {
                    // Phase 1: buffer drained — let senders resume.
                    backpressure_for_task.clear_high();
                }
                DataChannelEvent::OnClose => {
                    break;
                }
                DataChannelEvent::OnError => {
                    error_subject.next(new_rx_error(
                        "RC_WEBRTC_PEER",
                        Some(serde_json::json!({
                            "message": "data channel error",
                            "peer": peer_for_task,
                        })),
                    ));
                }
                _ => {}
            }
        }
        // Channel ended: retire the whole connection so discovery can build a
        // fresh route instead of reusing a closed peer connection.
        if let Some(presence_changed) = finish_data_channel_generation(
            &handler_for_task,
            &peer_for_task,
            generation,
            &backpressure_for_task,
        ) {
            if presence_changed {
                let handler_presence = Arc::clone(&handler_for_task);
                tokio::spawn(async move {
                    handler_presence.broadcast_presence().await;
                });
            }
        }
    });

    if let Some(entry) = handler.peers.lock().get_mut(&remote_peer_id) {
        if entry.generation == generation {
            entry.tasks.push(task);
        } else {
            task.abort();
        }
    } else {
        task.abort();
    }
}

/// Retire the connection atomically with connection replacement.
/// Both OnClose and stream EOF use this path; retired tasks cannot clear successors.
fn finish_data_channel_generation(
    handler: &WebRTCRsConnectionHandler,
    peer: &WebRTCRsPeer,
    generation: u64,
    backpressure: &Arc<PeerBackpressure>,
) -> Option<bool> {
    // A retired task may still have senders waiting on its own signal.
    backpressure.clear_high();
    if !remove_peer_inner(handler, peer, PeerRemoval::Generation(generation), None) {
        return None;
    }
    Some(handler.presence_dirty.load(Ordering::SeqCst))
}

fn superseded_send_queue_error(peer: &str) -> RxError {
    new_rx_error(
        "RC_WEBRTC_PEER",
        Some(serde_json::json!({
            "message": "WebRTC send queue was closed or superseded",
            "peer": peer,
            EXPECTED_PEER_TEARDOWN_PARAM: true,
        })),
    )
}

enum PeerRemoval<'a> {
    Generation(u64),
    Unopened(u64),
    SendQueue(&'a Arc<tokio::sync::Notify>),
}

fn remove_peer_with_error(
    handler: &WebRTCRsConnectionHandler,
    peer: &str,
    available: &Arc<tokio::sync::Notify>,
    error: RxError,
) -> bool {
    remove_peer_inner(
        handler,
        peer,
        PeerRemoval::SendQueue(available),
        Some(error),
    )
}

fn remove_peer_generation(handler: &WebRTCRsConnectionHandler, peer: &str, generation: u64) {
    remove_peer_inner(handler, peer, PeerRemoval::Generation(generation), None);
}

fn is_current_peer_generation(
    handler: &WebRTCRsConnectionHandler,
    peer: &str,
    generation: u64,
) -> bool {
    peer_generation_matches(
        handler.peers.lock().get(peer).map(|entry| entry.generation),
        Some(generation),
    )
}

fn peer_generation_matches(current: Option<u64>, expected: Option<u64>) -> bool {
    match expected {
        Some(expected) => current == Some(expected),
        None => true,
    }
}

fn remove_peer_inner(
    handler: &WebRTCRsConnectionHandler,
    peer: &str,
    condition: PeerRemoval<'_>,
    error: Option<RxError>,
) -> bool {
    let removed_entry = {
        let _lifecycle = handler.peer_lifecycle.lock();
        let removed_entry = {
            let mut peers = handler.peers.lock();
            let matches = match condition {
                PeerRemoval::Generation(generation) => peers
                    .get(peer)
                    .is_some_and(|entry| entry.generation == generation),
                PeerRemoval::Unopened(generation) => peers.get(peer).is_some_and(|entry| {
                    entry.generation == generation && !entry.data_channel_open
                }),
                PeerRemoval::SendQueue(available) => handler.is_current_send_queue(peer, available),
            };
            if !matches {
                return false;
            }
            peers.remove(peer)
        };

        // Clear every per-peer registry before a replacement generation can
        // register. Otherwise delayed teardown from the old connection can
        // erase the new generation's capability token or send state.
        handler.active_collections.lock().remove(peer);
        if handler.remove_peer_presence(peer) {
            handler.presence_dirty.store(true, Ordering::SeqCst);
        }
        handler.peer_capability_tokens.lock().remove(peer);
        if let Some(bp) = handler.backpressure.lock().remove(peer) {
            bp.clear_high();
        }
        if let Some(mut queue) = handler.send_queues.lock().remove(peer) {
            if let Some(error) = error {
                for item in queue.high.drain(..) {
                    let _ = item.result.send(Err(error.clone()));
                }
                for item in queue.normal.drain(..) {
                    let _ = item.result.send(Err(error.clone()));
                }
                for item in queue.low.drain(..) {
                    let _ = item.result.send(Err(error.clone()));
                }
            }
        }
        handler.clear_peer_transfer_state(&peer.to_string());
        handler.refresh_send_queue_status();
        removed_entry
    };

    if let Some(mut entry) = removed_entry {
        for task in entry.tasks.drain(..) {
            task.abort();
        }
        let peer_id = peer.to_string();
        tokio::spawn(async move {
            if let Some(data_channel) = entry.data_channel {
                let _ = data_channel.close().await;
            }
            let _ = entry.peer_connection.close().await;
        });
        handler
            .disconnect_subject
            .next(WebRTCRsConnection::new(peer_id, entry.generation));
    }
    true
}

async fn wait_for_own_peer_id(signaling: &Arc<SignalingClient>) -> RxResult<PeerId> {
    for _ in 0..100 {
        if let Some(peer_id) = signaling.own_peer_id() {
            return Ok(peer_id);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(new_rx_error(
        "RC_WEBRTC_SIGNAL",
        Some(serde_json::json!({ "message": "timed out waiting for signaling init" })),
    ))
}

fn decode_error(kind: &str, err: serde_json::Error, text: &str) -> RxError {
    new_rx_error(
        "RC_WEBRTC_PEER",
        Some(serde_json::json!({
            "message": format!("decode WebRTC {kind} failed: {err}"),
            "frame": text,
        })),
    )
}

fn webrtc_error(context: &str, err: impl std::fmt::Display) -> RxError {
    new_rx_error(
        "RC_WEBRTC_PEER",
        Some(serde_json::json!({
            "message": format!("{context}: {err}"),
        })),
    )
}

fn simple_peer_ice_signal(candidate: RTCIceCandidateInit) -> Value {
    serde_json::json!({
        "type": "candidate",
        "candidate": candidate,
    })
}

fn decode_simple_peer_ice_candidate(
    data: &Value,
) -> Result<RTCIceCandidateInit, serde_json::Error> {
    let candidate_value = match data.get("candidate") {
        Some(candidate) if candidate.is_object() => candidate.clone(),
        Some(candidate) if candidate.is_string() => data.clone(),
        _ => data.clone(),
    };
    serde_json::from_value(candidate_value)
}

fn is_ctox_transport_frame(value: &Value) -> bool {
    value.get("ctoxFrame").and_then(Value::as_str) == Some(CTOX_FRAME_PROTOCOL)
}

fn incoming_frame_allocation_bytes(total_frames: usize, total_bytes: usize) -> usize {
    total_bytes.saturating_add(total_frames.saturating_mul(std::mem::size_of::<Option<String>>()))
}

fn highest_contiguous_seq(received: &[Option<String>]) -> Option<usize> {
    let mut highest = None;
    for (index, value) in received.iter().enumerate() {
        if value.is_none() {
            return highest;
        }
        highest = Some(index);
    }
    highest
}

/// Phase 2: the result of classifying an outbound frame for the
/// collection-aware send queue. Carries the serialized `text` plus the
/// metadata needed to (re)bucket the frame whenever the peer's active-
/// collection set changes.
struct SendFrameClass {
    text: String,
    collection: Option<String>,
    /// Control / handshake frames that are always High regardless of the
    /// active set (responses incl. master-change pushes; `ctoxProtocol` /
    /// `token`).
    intrinsic_high: bool,
    /// Oversized `masterWrite` — a large background bulk write that stays Low
    /// so it never stalls foreground collections, even if its collection is
    /// active.
    oversized_write: bool,
}

impl SendFrameClass {
    /// Resolve the concrete [`SendPriority`] against an active-collection set.
    /// Mirrors [`QueuedSend::classify_against`] so the enqueue-time class and
    /// the re-bucket path agree.
    fn classify(&self, active: &HashSet<String>) -> SendPriority {
        if self.intrinsic_high {
            return SendPriority::High;
        }
        if self.oversized_write {
            return SendPriority::Low;
        }
        match &self.collection {
            Some(name) if active.contains(name) => SendPriority::High,
            _ => SendPriority::Normal,
        }
    }
}

/// Phase 2: classify an outbound frame into a [`SendFrameClass`]. The concrete
/// priority is resolved later against the peer's active-collection set, but the
/// intrinsic dimensions (control vs. data, oversized write) are fixed here.
fn classify_send_frame(frame: &WebRTCWireFrame, text: &str) -> SendFrameClass {
    match frame {
        WebRTCWireFrame::Response(response) => SendFrameClass {
            text: text.to_string(),
            collection: response.collection.clone(),
            intrinsic_high: true,
            oversized_write: false,
        },
        WebRTCWireFrame::Message(message) => {
            let intrinsic_high = matches!(message.method.as_str(), "ctoxProtocol" | "token");
            let oversized_write =
                message.method == "masterWrite" && text.len() > MAX_INLINE_FRAME_BYTES;
            SendFrameClass {
                text: text.to_string(),
                collection: message.collection.clone(),
                intrinsic_high,
                oversized_write,
            }
        }
    }
}

fn default_udp_bind_addr() -> String {
    std::env::var(UDP_BIND_ADDR_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_UDP_BIND_ADDR.to_string())
}

fn advertisable_udp_bind_addrs(configured: &str) -> Vec<String> {
    let configured = configured.trim();
    if !configured.is_empty() && configured != DEFAULT_UDP_BIND_ADDR {
        return vec![configured.to_string()];
    }

    // Binding the rtc socket to 0.0.0.0 makes the current rtc crate publish
    // 0.0.0.0 as its host ICE candidate. That candidate is unusable, and two
    // peers on the same LAN then depend on NAT hairpin support. A connected UDP
    // socket discovers the interface chosen by the OS without sending traffic.
    let local = UdpSocket::bind(DEFAULT_UDP_BIND_ADDR)
        .and_then(|socket| {
            socket.connect("1.1.1.1:80")?;
            socket.local_addr()
        })
        .ok()
        .filter(|addr| !addr.ip().is_unspecified())
        .map(|addr| format!("{}:0", addr.ip()));

    vec![local.unwrap_or_else(|| "127.0.0.1:0".to_string())]
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

async fn send_json_text(data_channel: &Arc<dyn DataChannel>, value: &Value) -> RxResult<()> {
    let text = serde_json::to_string(value).map_err(|e| {
        new_rx_error(
            "RC_WEBRTC_PEER",
            Some(serde_json::json!({
                "message": format!("serialize WebRTC transport frame failed: {e}"),
            })),
        )
    })?;
    // Phase 1 hard size invariant (defense-in-depth): never put a message larger
    // than the SCTP ceiling on the wire. `split_chunks_for_frame` already bounds
    // the *serialized* chunk size, so this should be unreachable in practice — it
    // exists to turn any future regression into a loud, contained error instead of
    // a silently dropped/killed channel.
    if text.len() > MAX_SERIALIZED_FRAME_BYTES {
        return Err(new_rx_error(
            "RC_WEBRTC_PEER",
            Some(serde_json::json!({
                "message": "serialized WebRTC transport frame exceeds SCTP message limit",
                "bytes": text.len(),
                "maxBytes": MAX_SERIALIZED_FRAME_BYTES,
            })),
        ));
    }
    data_channel
        .send_text(&text)
        .await
        .map_err(|e| webrtc_error("send WebRTC transport frame", e))
}

async fn send_auxiliary_wire_frame(
    data_channel: &Arc<dyn DataChannel>,
    frame: WebRTCWireFrame,
) -> RxResult<()> {
    let text = serde_json::to_string(&frame).map_err(|error| {
        new_rx_error(
            "RC_WEBRTC_PEER",
            Some(serde_json::json!({
                "message": format!("serialize auxiliary WebRTC frame failed: {error}"),
            })),
        )
    })?;
    if text.len() <= 12_000 {
        return data_channel
            .send_text(&text)
            .await
            .map_err(|error| webrtc_error("send auxiliary WebRTC frame", error));
    }
    let mut chunks = Vec::<String>::new();
    let mut chunk = String::new();
    let mut bytes = 0usize;
    for character in text.chars() {
        let width = character.len_utf8();
        if bytes + width > 8_000 && !chunk.is_empty() {
            chunks.push(std::mem::take(&mut chunk));
            bytes = 0;
        }
        chunk.push(character);
        bytes += width;
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    // Multiple Browser renders can issue the same large request in the same
    // millisecond. `timestamp + chunk count` then collided and the browser
    // merged chunks from two different responses under one key: native
    // counters said "completed", while both callers timed out. The monotonic
    // suffix is process-unique and keeps every reassembly stream disjoint.
    let transfer_id = next_auxiliary_transfer_id();
    let total = chunks.len();
    for (seq, data) in chunks.into_iter().enumerate() {
        send_json_text(
            data_channel,
            &serde_json::json!({
                "ctoxAuxFrame": "ctox-aux-frame-v1",
                "transferId": transfer_id,
                "seq": seq,
                "total": total,
                "data": data,
            }),
        )
        .await?;
        tokio::task::yield_now().await;
    }
    Ok(())
}

fn next_auxiliary_transfer_id() -> String {
    format!(
        "browser-live-{}-{}",
        now_ms(),
        AUXILIARY_TRANSFER_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Byte length of `ch` as it appears inside a serde_json string value (excluding
/// the surrounding quotes). Mirrors serde_json's default escaping: the two-char
/// short escapes (`\" \\ \b \f \n \r \t`), `\u00XX` for the remaining C0 controls,
/// and raw UTF-8 bytes for everything else.
fn json_escaped_char_len(ch: char) -> usize {
    match ch {
        '"' | '\\' | '\u{08}' | '\u{0c}' | '\n' | '\r' | '\t' => 2,
        c if (c as u32) < 0x20 => 6,
        c => c.len_utf8(),
    }
}

/// Split `text` so that EACH chunk, once wrapped by `transport_chunk_frame` and
/// serialized, is <= `MAX_SERIALIZED_FRAME_BYTES`. The previous splitter bounded
/// the *raw* UTF-8 chunk size, but the chunk is then placed in a JSON string whose
/// escaping (`"`, `\`, control chars) can multiply its serialized length — so an
/// escape-heavy 10 KiB chunk could serialize to far more than 16 KiB and overrun
/// the channel. This bounds the serialized frame directly, regardless of content.
fn split_chunks_for_frame(text: &str, transfer_id: &str) -> Vec<String> {
    // Conservative wrapper overhead: serialize an empty-data frame with worst-case
    // numeric widths so the data budget always leaves room for the real frame.
    let overhead = transport_chunk_frame(transfer_id, usize::MAX, usize::MAX, "")
        .to_string()
        .len();
    let budget = MAX_SERIALIZED_FRAME_BYTES
        .saturating_sub(overhead + 64)
        .max(1);
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_escaped = 0usize;
    for ch in text.chars() {
        let ch_escaped = json_escaped_char_len(ch);
        if cur_escaped + ch_escaped > budget && !cur.is_empty() {
            chunks.push(std::mem::take(&mut cur));
            cur_escaped = 0;
        }
        cur.push(ch);
        cur_escaped += ch_escaped;
    }
    if !cur.is_empty() || chunks.is_empty() {
        chunks.push(cur);
    }
    chunks
}

#[cfg(test)]
mod tests {
    // `WebRTCMessage` / `WebRTCResponse` are now imported at module scope
    // (used by `apply_active_collections` + the send path) and reach the tests
    // through this glob.
    use super::*;

    #[tokio::test]
    async fn best_effort_send_error_policy_suppresses_close_and_publishes_transport_failure() {
        let subject = RxSubject::new();
        let mut errors = subject.subscribe();
        publish_best_effort_send_error(
            &subject,
            new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "unknown or unopened peer",
                    EXPECTED_PEER_TEARDOWN_PARAM: true,
                })),
            ),
        );
        publish_best_effort_send_error(
            &subject,
            new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "WebRTC send queue result dropped",
                    EXPECTED_PEER_TEARDOWN_PARAM: true,
                })),
            ),
        );
        publish_best_effort_send_error(
            &subject,
            new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "send data channel frame: transport failed"
                })),
            ),
        );

        let published = tokio::time::timeout(Duration::from_secs(1), errors.next())
            .await
            .expect("genuine transport failure must be published")
            .expect("error subject must remain open");
        assert_eq!(
            published
                .parameters()
                .get("message")
                .and_then(Value::as_str),
            Some("send data channel frame: transport failed"),
            "the expected peer-close error must not precede the transport failure"
        );
    }

    fn test_send_queue(
        handler: &WebRTCRsConnectionHandler,
        route: &str,
    ) -> Arc<tokio::sync::Notify> {
        handler
            .send_queues
            .lock()
            .entry(route.to_owned())
            .or_default()
            .drain_available
            .clone()
    }

    async fn install_test_connection(
        handler: &WebRTCRsConnectionHandler,
        route: &str,
        generation: u64,
    ) -> WebRTCRsConnection {
        struct Events;
        #[async_trait]
        impl PeerConnectionEventHandler for Events {}
        let pc = PeerConnectionBuilder::new()
            .with_handler(Arc::new(Events))
            .with_udp_addrs(advertisable_udp_bind_addrs("127.0.0.1:0"))
            .build()
            .await
            .unwrap();
        let entry = PeerEntry {
            generation,
            local_peer_id: None,
            peer_connection: Arc::new(pc),
            data_channel: None,
            data_channel_open: true,
            auxiliary_data_channels: HashMap::new(),
            tasks: vec![],
        };
        assert!(handler
            .peers
            .lock()
            .insert(route.to_owned(), entry)
            .is_none());
        handler
            .connection_for_peer(route)
            .expect("fixture connection is current")
    }

    fn invoke_policy_probe(
        handler: &WebRTCRsConnectionHandler,
        peer: &WebRTCRsConnection,
        probe: usize,
    ) -> bool {
        match probe {
            0 => handler.is_collection_authorized_for_peer(peer, "records"),
            1 => handler.is_collection_write_authorized_for_peer(peer, "records"),
            2 => handler.is_eager_collection_pull_authorized_for_peer(peer, "records"),
            3 => handler.is_inactive_live_change_authorized_for_peer(peer, "records"),
            4 => handler.document_filter_for_peer(peer, "records").unwrap()(&Value::Null),
            5 => {
                handler.document_fields_for_peer(peer, "records").unwrap()
                    == vec!["visible".to_owned()]
            }
            6 => handler.are_documents_write_authorized_for_peer(
                peer,
                "records",
                &[serde_json::json!([{"newDocumentState": {"id": "record"}}])],
            ),
            _ => unreachable!(),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn slow_policy_hooks_release_lifecycle_and_fence_late_results() {
        // Ordering, not a latency benchmark: maintenance must finish while
        // the hook is deliberately held, and stale decisions must be denied.
        for probe in 0..7 {
            for change in 0..3 {
                let handler = Arc::new(WebRTCRsConnectionHandler::new());
                let peer = install_test_connection(&handler, "policy-peer", 1).await;
                let other = install_test_connection(&handler, "other-peer", 1).await;
                handler.set_peer_capability_token(&peer, "original".into());
                let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
                let entered_tx = Mutex::new(Some(entered_tx));
                let (release, proceed) = std::sync::mpsc::channel();
                let proceed = Mutex::new(proceed);
                let check: CollectionAuthzHook = Arc::new(move |token, _| {
                    assert_eq!(token, "original");
                    if let Some(entered) = entered_tx.lock().take() {
                        let _ = entered.send(());
                    }
                    proceed.lock().recv_timeout(Duration::from_secs(5)).is_ok()
                });
                handler.set_collection_authz(Some(check.clone()));
                handler.set_collection_write_authz(Some(check.clone()));
                handler.set_collection_eager_pull(Some(check.clone()));
                handler.set_collection_live_change(Some(check.clone()));
                let read_check = check.clone();
                handler.set_document_read_authz(Some(Arc::new(move |token, collection| {
                    let allowed = read_check(token, collection);
                    DocumentReadPolicy {
                        filter: Arc::new(move |_| allowed),
                        fields: Some(if allowed {
                            vec!["visible".to_owned()]
                        } else {
                            vec![]
                        }),
                    }
                })));
                handler.set_document_write_authz(Some(Arc::new(move |token, collection, _| {
                    check(token, collection)
                })));
                let lookup_handler = handler.clone();
                let lookup_peer = peer.clone();
                let lookup = tokio::task::spawn_blocking(move || {
                    invoke_policy_probe(&lookup_handler, &lookup_peer, probe)
                });
                tokio::time::timeout(Duration::from_secs(2), entered_rx)
                    .await
                    .expect("policy hook must start")
                    .unwrap();
                let maintenance_handler = handler.clone();
                let maintenance_peer = peer.clone();
                let mut maintenance = tokio::task::spawn_blocking(move || match change {
                    0 => maintenance_handler
                        .set_peer_capability_token(&other, "other-updated".into()),
                    1 => maintenance_handler
                        .set_peer_capability_token(&maintenance_peer, "replacement".into()),
                    2 => {
                        // Advance the fixture connection generation while its
                        // previous handle still owns the in-flight lookup.
                        let _lifecycle = maintenance_handler.peer_lifecycle.lock();
                        maintenance_handler
                            .peers
                            .lock()
                            .get_mut(&maintenance_peer.peer_id)
                            .unwrap()
                            .generation += 1;
                    }
                    _ => unreachable!(),
                });
                let completed_while_held =
                    tokio::time::timeout(Duration::from_secs(1), &mut maintenance).await;
                let released_early = completed_while_held.is_ok();
                let _ = release.send(());
                if let Ok(result) = completed_while_held {
                    result.unwrap();
                } else {
                    maintenance.await.unwrap();
                }
                let allowed = lookup.await.unwrap();
                handler.close().await.unwrap();
                assert!(
                    released_early,
                    "probe {probe}, change {change}: policy blocked peer lifecycle"
                );
                assert_eq!(
                    allowed,
                    change == 0,
                    "probe {probe}, change {change}: stale token/generation decision escaped"
                );
            }
        }
    }

    /// #12c: per-collection authz is fail-open until a hook is installed, then
    /// enforces using the peer's captured capability token.
    #[tokio::test]
    async fn collection_authz_is_fail_open_then_enforces_with_token() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = install_test_connection(&handler, "peer-1", 1).await;
        // Default (no hook): authorized for anything.
        assert!(handler.is_collection_authorized_for_peer(&peer, "business_credentials"));
        // Hook that only allows a peer presenting token "tok-abc".
        handler.set_collection_authz(Some(Arc::new(|token: &str, _collection: &str| {
            token == "tok-abc"
        })));
        // No token captured yet => empty token => denied.
        assert!(!handler.is_collection_authorized_for_peer(&peer, "anything"));
        // Capture the peer's handshake token => authorized.
        handler.set_peer_capability_token(&peer, "tok-abc".to_string());
        assert!(handler.is_collection_authorized_for_peer(&peer, "anything"));
        // A different peer without the token stays denied.
        let other = install_test_connection(&handler, "peer-2", 1).await;
        assert!(!handler.is_collection_authorized_for_peer(&other, "anything"));
        // Removing enforcement returns to fail-open.
        handler.set_collection_authz(None);
        assert!(handler.is_collection_authorized_for_peer(&other, "anything"));
        handler.close().await.unwrap();
    }

    #[tokio::test]
    async fn eager_pull_gate_is_fail_open_and_drops_live_master_changes() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = install_test_connection(&handler, "peer-1", 1).await;
        assert!(handler.is_eager_collection_pull_authorized_for_peer(&peer, "sellify_activities"));
        handler.set_collection_eager_pull(Some(Arc::new(|_token: &str, collection: &str| {
            collection != "sellify_activities"
        })));
        assert!(!handler.is_eager_collection_pull_authorized_for_peer(&peer, "sellify_activities"));
        assert!(handler
            .filter_master_change_for_peer(
                &peer,
                "sellify_activities",
                crate::types::RxReplicationMasterChange::Resync,
            )
            .is_none());
        assert!(handler.is_eager_collection_pull_authorized_for_peer(&peer, "business_commands"));

        handler.set_collection_eager_pull(Some(Arc::new(|_token: &str, _collection: &str| false)));
        handler.set_collection_live_change(Some(Arc::new(|_token: &str, collection: &str| {
            collection == "business_commands"
        })));
        assert!(!handler.is_eager_collection_pull_authorized_for_peer(&peer, "business_commands"));
        assert!(handler.is_inactive_live_change_authorized_for_peer(&peer, "business_commands"));
        assert!(!handler.is_inactive_live_change_authorized_for_peer(&peer, "sellify_activities"));
        assert!(handler
            .filter_master_change_for_peer(
                &peer,
                "business_commands",
                crate::types::RxReplicationMasterChange::Documents(
                    crate::types::DocumentsWithCheckpoint {
                        documents: vec![serde_json::json!({"id": "command-1"})],
                        checkpoint: serde_json::json!({"id": "command-1", "lwt": 1}),
                    },
                ),
            )
            .is_some());
        assert!(handler
            .filter_master_change_for_peer(
                &peer,
                "sellify_activities",
                crate::types::RxReplicationMasterChange::Resync,
            )
            .is_none());
        handler.close().await.unwrap();
    }

    #[tokio::test]
    async fn crew_live_changes_apply_the_authenticated_field_policy() {
        let handler = WebRTCRsConnectionHandler::new();
        let fixture: Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/crew-identity.json"))
                .unwrap();
        let public: Vec<String> = serde_json::from_value(fixture["public_fields"].clone()).unwrap();
        handler.set_document_read_authz(Some(Arc::new(move |token, _collection| {
            let readable = matches!(token, "admin" | "founder" | "user");
            DocumentReadPolicy {
                filter: Arc::new(move |_| readable),
                fields: if matches!(token, "admin" | "founder") {
                    None
                } else {
                    Some(public.clone())
                },
            }
        })));
        for role in ["admin", "founder", "user", "invalid"] {
            let peer = install_test_connection(&handler, role, 1).await;
            handler.set_peer_capability_token(&peer, role.to_string());
            let result = handler.filter_master_change_for_peer(
                &peer,
                "ctox_crew_members",
                crate::types::RxReplicationMasterChange::Documents(
                    crate::types::DocumentsWithCheckpoint {
                        documents: vec![fixture["member"].clone()],
                        checkpoint: serde_json::json!({"id":"crew-milo","lwt":1}),
                    },
                ),
            );
            if role == "invalid" {
                assert!(result.is_none());
                continue;
            }
            let Some(crate::types::RxReplicationMasterChange::Documents(result)) = result else {
                panic!("missing change")
            };
            assert_eq!(result.documents[0].get("soul").is_some(), role != "user");
            assert_eq!(result.documents[0]["name"], "Milo");
        }
        // A delayed request from a retired connection cannot inherit the
        // replacement's administrator field policy on the same signaling route.
        let retired = install_test_connection(&handler, "reused-route", 1).await;
        handler.set_peer_capability_token(&retired, "user".into());
        assert!(handler
            .document_fields_for_peer(&retired, "ctox_crew_members")
            .is_some_and(|fields| !fields.contains(&"soul".into())));
        handler.close_peer(&retired).await;
        let replacement = install_test_connection(&handler, "reused-route", 2).await;
        handler.set_peer_capability_token(&replacement, "admin".into());
        assert_eq!(
            handler.document_fields_for_peer(&retired, "ctox_crew_members"),
            Some(Vec::new())
        );
        assert!(!handler
            .document_filter_for_peer(&retired, "ctox_crew_members")
            .unwrap()(&fixture["member"]));
        assert_eq!(
            handler.document_fields_for_peer(&replacement, "ctox_crew_members"),
            None
        );
        handler.close().await.unwrap();
    }

    #[tokio::test]
    async fn write_and_document_authz_hooks_are_fail_open_then_enforced() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = install_test_connection(&handler, "peer-1", 1).await;
        assert!(handler.is_collection_write_authorized_for_peer(&peer, "user_threads"));
        assert!(handler
            .document_filter_for_peer(&peer, "user_threads")
            .is_none());
        assert!(handler.are_documents_write_authorized_for_peer(
            &peer,
            "browser_input_events",
            &[serde_json::json!([{ "newDocumentState": { "owner_user_id": "alice" } }])],
        ));

        handler.set_collection_write_authz(Some(Arc::new(|token: &str, collection: &str| {
            token == "tok-abc" && collection == "business_commands"
        })));
        let document_filter_preparations = Arc::new(AtomicU64::new(0));
        let document_filter_preparations_for_hook = Arc::clone(&document_filter_preparations);
        handler.set_document_read_authz(Some(Arc::new(move |token: &str, _collection: &str| {
            document_filter_preparations_for_hook.fetch_add(1, Ordering::Relaxed);
            let authorized = token == "tok-abc";
            DocumentReadPolicy {
                filter: Arc::new(move |document: &Value| {
                    authorized && document.get("user_id").and_then(Value::as_str) == Some("alice")
                }),
                fields: None,
            }
        })));
        handler.set_document_write_authz(Some(Arc::new(
            |token: &str, collection: &str, document: &Value| {
                token == "tok-abc"
                    && collection == "browser_input_events"
                    && document.get("owner_user_id").and_then(Value::as_str) == Some("alice")
            },
        )));
        assert!(!handler.is_collection_write_authorized_for_peer(&peer, "business_commands"));
        handler.set_peer_capability_token(&peer, "tok-abc".to_string());
        assert!(handler.is_collection_write_authorized_for_peer(&peer, "business_commands"));
        assert!(!handler.is_collection_write_authorized_for_peer(&peer, "user_threads"));
        let filter = handler
            .document_filter_for_peer(&peer, "user_notifications")
            .expect("document filter");
        assert!(filter(&serde_json::json!({ "user_id": "alice" })));
        assert!(!filter(&serde_json::json!({ "user_id": "bob" })));
        assert_eq!(
            document_filter_preparations.load(Ordering::Relaxed),
            1,
            "one query filter must authorize the token once, not once per document"
        );
        assert!(handler.are_documents_write_authorized_for_peer(
            &peer,
            "browser_input_events",
            &[serde_json::json!([{ "newDocumentState": { "owner_user_id": "alice" } }])],
        ));
        assert!(!handler.are_documents_write_authorized_for_peer(
            &peer,
            "browser_input_events",
            &[serde_json::json!([{ "newDocumentState": { "owner_user_id": "bob" } }])],
        ));
        handler.close().await.unwrap();
    }

    #[tokio::test]
    async fn retired_connection_cannot_mutate_or_close_its_replacement() {
        let handler = WebRTCRsConnectionHandler::new();
        let old = install_test_connection(&handler, "reused-route", 1).await;
        handler.set_peer_capability_token(&old, "old-token".into());
        handler.close_peer(&old).await;
        let replacement = install_test_connection(&handler, "reused-route", 2).await;
        handler.set_peer_capability_token(&replacement, "new-token".into());
        assert_eq!(
            handler.peer_identity(&old),
            handler.peer_identity(&replacement)
        );
        assert_ne!(
            handler.connection_identity(&old),
            handler.connection_identity(&replacement)
        );
        assert!(!handler.is_peer_current(&old));
        assert!(handler.is_peer_current(&replacement));
        handler.set_peer_capability_token(&old, "late-old-token".into());
        assert_eq!(
            handler.peer_capability_token(&replacement).as_deref(),
            Some("new-token")
        );
        assert!(handler.peer_capability_token(&old).is_none());
        assert!(!handler.is_collection_authorized_for_peer(&old, "records"));
        assert!(!handler.is_collection_write_authorized_for_peer(&old, "records"));
        assert!(!handler.document_filter_for_peer(&old, "records").unwrap()(
            &serde_json::json!({})
        ));
        let result = handler
            .send(
                &old,
                WebRTCWireFrame::Message(WebRTCMessage {
                    id: "late-send".into(),
                    method: "token".into(),
                    params: vec![],
                    collection: None,
                }),
            )
            .await;
        assert!(
            result.is_err(),
            "old handle must fail before enqueueing a frame"
        );
        assert!(handler.send_queues.lock().is_empty());
        handler.close_peer(&old).await;
        assert_eq!(
            handler.connection_for_peer("reused-route"),
            Some(replacement.clone())
        );
        assert_eq!(
            handler.peer_capability_token(&replacement).as_deref(),
            Some("new-token")
        );
        handler.close().await.unwrap();
    }

    /// REGRESSION (52a1bf45): when the task draining a peer's send queue is
    /// aborted mid-send, the guard's Drop must re-open the drain slot.
    #[test]
    fn drain_reset_guard_reopens_slot_on_drop() {
        let queues: Arc<Mutex<HashMap<WebRTCRsPeer, PeerSendQueue>>> =
            Arc::new(Mutex::new(HashMap::new()));
        queues.lock().entry("p1".to_string()).or_default().draining = true;
        let available = queues.lock()["p1"].drain_available.clone();
        drop(DrainResetGuard {
            queues: Arc::clone(&queues),
            peer: "p1".to_string(),
            available: available.clone(),
            in_flight: Arc::default(),
            armed: true,
        });
        assert!(
            !queues.lock().get("p1").unwrap().draining,
            "armed guard must clear `draining` on drop"
        );
        queues.lock().get_mut("p1").unwrap().draining = true;
        drop(DrainResetGuard {
            queues: Arc::clone(&queues),
            peer: "p1".to_string(),
            available,
            in_flight: Arc::default(),
            armed: false,
        });
        assert!(
            queues.lock().get("p1").unwrap().draining,
            "disarmed guard must leave the flag alone"
        );
    }

    #[tokio::test]
    async fn cancelling_first_send_poll_releases_the_claimed_drain_slot() {
        use std::future::Future;
        use std::task::{Context, Waker};
        struct Events;
        #[async_trait]
        impl PeerConnectionEventHandler for Events {}
        let pc = PeerConnectionBuilder::new()
            .with_handler(Arc::new(Events))
            .with_udp_addrs(advertisable_udp_bind_addrs("127.0.0.1:0"))
            .build()
            .await
            .unwrap();
        let channel = pc
            .create_data_channel("cancel-fixture", None)
            .await
            .unwrap();
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "cancelled-first-sender".to_owned();
        let frame = SendFrameClass {
            text: "{}".into(),
            collection: None,
            intrinsic_high: true,
            oversized_write: false,
        };
        let mut send = Box::pin(handler.send_queued_text(&peer, channel, frame));
        let pending = send
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending();
        // Drop exactly at the deliberate scheduler yield, before sending bytes.
        drop(send);
        let reopened = !handler.send_queues.lock().get(&peer).unwrap().draining;
        pc.close().await.unwrap();
        assert!(pending, "first poll must reach the cooperative yield");
        assert!(
            reopened,
            "cancelled first sender must not strand later sends behind a claimed drain slot"
        );
    }

    #[test]
    fn old_drain_guard_cannot_release_a_reconnected_peers_queue() {
        let queues = Arc::new(Mutex::new(HashMap::new()));
        let peer = "reconnected".to_owned();
        let old = PeerSendQueue::default();
        let guard = DrainResetGuard {
            queues: queues.clone(),
            peer: peer.clone(),
            available: old.drain_available.clone(),
            in_flight: Arc::default(),
            armed: true,
        };
        queues.lock().insert(peer.clone(), old);
        let replacement = PeerSendQueue {
            draining: true,
            ..Default::default()
        };
        queues.lock().insert(peer.clone(), replacement);
        drop(guard);
        assert!(
            queues.lock()[&peer].draining,
            "old cleanup must not release the new drainer"
        );
    }

    #[test]
    fn old_sender_cannot_claim_a_reconnected_peers_queue() {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};
        let queues = Arc::new(Mutex::new(HashMap::new()));
        let peer = "reconnected".to_owned();
        let old_available = PeerSendQueue::default().drain_available;
        queues.lock().insert(peer.clone(), PeerSendQueue::default());
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut sender = Box::pin(await_queued_send(
            &queues,
            &peer,
            old_available,
            rx,
            |_guard| async { panic!("old sender must not send on the replacement queue") },
        ));
        let mut context = Context::from_waker(Waker::noop());
        assert!(sender.as_mut().poll(&mut context).is_pending());
        assert!(!queues.lock()[&peer].draining);
        drop(tx);
        assert!(matches!(
            sender.as_mut().poll(&mut context),
            Poll::Ready(Err(_))
        ));
    }

    #[tokio::test]
    async fn old_drain_cannot_take_replacement_frames() {
        struct Events;
        #[async_trait]
        impl PeerConnectionEventHandler for Events {}
        let pc = PeerConnectionBuilder::new()
            .with_handler(Arc::new(Events))
            .with_udp_addrs(advertisable_udp_bind_addrs("127.0.0.1:0"))
            .build()
            .await
            .unwrap();
        let channel = pc
            .create_data_channel("reconnect-fixture", None)
            .await
            .unwrap();
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "reconnected".to_owned();
        let old_available = PeerSendQueue::default().drain_available;
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let mut replacement = PeerSendQueue {
            draining: true,
            ..Default::default()
        };
        replacement.push(QueuedSend {
            text: "{}".into(),
            priority: SendPriority::High,
            collection: None,
            intrinsic_high: true,
            oversized_write: false,
            queued_at_ms: now_ms(),
            result: tx,
        });
        handler.send_queues.lock().insert(peer.clone(), replacement);
        let guard = DrainResetGuard {
            queues: handler.send_queues.clone(),
            peer: peer.clone(),
            available: old_available.clone(),
            in_flight: Arc::default(),
            armed: true,
        };
        let finished = tokio::time::timeout(
            Duration::from_secs(1),
            handler.drain_send_queue(&peer, channel.clone(), guard),
        )
        .await;
        let priority_result = handler
            .drain_high_priority_inline_frames(&peer, &channel, &old_available)
            .await;
        pc.close().await.unwrap();
        finished.expect("a replaced drainer must finish without sending");
        priority_result.unwrap();
        let queues = handler.send_queues.lock();
        assert_eq!(
            queues[&peer].high.len(),
            1,
            "the replacement frame must remain queued"
        );
        assert!(
            queues[&peer].draining,
            "the replacement keeps its drain turn"
        );
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn interleaved_own_receipt_preserves_the_other_collections_transfer() {
        // Exercise the real queue, framing, priority interleave and ACK parser.
        // Only the SCTP link is replaced: each text frame is recorded and ACKed.
        struct AckChannel {
            inner: Arc<dyn DataChannel>,
            handler: Arc<WebRTCRsConnectionHandler>,
            peer: String,
            frames: Mutex<Vec<Value>>,
            totals: Mutex<HashMap<String, usize>>,
        }
        #[async_trait]
        impl DataChannel for AckChannel {
            async fn label(&self) -> webrtc::error::Result<String> {
                self.inner.label().await
            }
            async fn ordered(&self) -> webrtc::error::Result<bool> {
                self.inner.ordered().await
            }
            async fn max_packet_life_time(&self) -> webrtc::error::Result<Option<u16>> {
                self.inner.max_packet_life_time().await
            }
            async fn max_retransmits(&self) -> webrtc::error::Result<Option<u16>> {
                self.inner.max_retransmits().await
            }
            async fn protocol(&self) -> webrtc::error::Result<String> {
                self.inner.protocol().await
            }
            async fn negotiated(&self) -> webrtc::error::Result<bool> {
                self.inner.negotiated().await
            }
            fn id(&self) -> webrtc::data_channel::RTCDataChannelId {
                self.inner.id()
            }
            async fn ready_state(
                &self,
            ) -> webrtc::error::Result<webrtc::data_channel::RTCDataChannelState> {
                self.inner.ready_state().await
            }
            async fn buffered_amount_high_threshold(&self) -> webrtc::error::Result<u32> {
                self.inner.buffered_amount_high_threshold().await
            }
            async fn set_buffered_amount_high_threshold(
                &self,
                n: u32,
            ) -> webrtc::error::Result<()> {
                self.inner.set_buffered_amount_high_threshold(n).await
            }
            async fn buffered_amount_low_threshold(&self) -> webrtc::error::Result<u32> {
                self.inner.buffered_amount_low_threshold().await
            }
            async fn set_buffered_amount_low_threshold(&self, n: u32) -> webrtc::error::Result<()> {
                self.inner.set_buffered_amount_low_threshold(n).await
            }
            async fn send(&self, data: bytes::BytesMut) -> webrtc::error::Result<()> {
                self.inner.send(data).await
            }
            async fn poll(&self) -> Option<DataChannelEvent> {
                self.inner.poll().await
            }
            async fn close(&self) -> webrtc::error::Result<()> {
                self.inner.close().await
            }
            async fn send_text(&self, text: &str) -> webrtc::error::Result<()> {
                let frame: Value = serde_json::from_str(text).unwrap();
                self.frames.lock().push(frame.clone());
                let id = frame["transferId"].as_str().unwrap_or("");
                match frame["kind"].as_str() {
                    Some("start") => {
                        self.totals
                            .lock()
                            .insert(id.into(), frame["totalFrames"].as_u64().unwrap() as usize);
                    }
                    Some("chunk") => {
                        let seq = frame["seq"].as_u64().unwrap() as usize;
                        let total = self.totals.lock()[id];
                        if (seq + 1).is_multiple_of(FRAME_ACK_WINDOW) || seq + 1 == total {
                            self.handler
                                .handle_transport_frame(
                                    &self.peer,
                                    self.inner.clone(),
                                    serde_json::json!({
                                        "ctoxFrame": CTOX_FRAME_PROTOCOL, "kind": "ack",
                                        "transferId": id, "ackSeq": seq,
                                        "receivedFrames": seq + 1, "final": seq + 1 == total,
                                    }),
                                )
                                .await
                                .unwrap();
                        }
                    }
                    _ => {}
                }
                Ok(())
            }
        }

        let handler = WebRTCRsConnectionHandler::new();
        let peer = "cross-collection-receipt".to_owned();
        install_test_connection(&handler, &peer, 1).await;
        let pc = handler.peers.lock()[&peer].peer_connection.clone();
        let inner = pc
            .create_data_channel("receipt-regression", None)
            .await
            .unwrap();
        let channel = Arc::new(AckChannel {
            inner,
            handler: handler.clone(),
            peer: peer.clone(),
            frames: Mutex::new(Vec::new()),
            totals: Mutex::new(HashMap::new()),
        });
        let large = serde_json::json!({
            "id": "large-pull", "collection": "user_thread_states",
            "result": {"documents": [{"id": "thread", "content": "Große Übertragung".repeat(20_000)}]},
        }).to_string();
        let queued_large = handler
            .enqueue_text(
                &peer,
                SendFrameClass {
                    text: large.clone(),
                    collection: Some("user_thread_states".into()),
                    intrinsic_high: true,
                    oversized_write: false,
                },
            )
            .unwrap();
        let small = serde_json::json!({
            "id": "small-command", "collection": "business_commands", "result": [],
        })
        .to_string();
        let queued_small = handler
            .enqueue_text(
                &peer,
                SendFrameClass {
                    text: small,
                    collection: Some("business_commands".into()),
                    intrinsic_high: true,
                    oversized_write: false,
                },
            )
            .unwrap();

        // The small caller owns the drain, but the large response is first in
        // its priority bucket. Its own receipt is completed by inline preemption.
        tokio::time::timeout(
            Duration::from_secs(5),
            handler.finish_send(&peer, channel.clone(), queued_small),
        )
        .await
        .expect("small command receipt must settle")
        .unwrap();
        let large_outcome = tokio::time::timeout(
            Duration::from_secs(5),
            handler.finish_send(&peer, channel.clone(), queued_large),
        )
        .await;
        let frames = channel.frames.lock().clone();
        handler.close().await.unwrap();
        large_outcome
            .expect("large response must settle")
            .expect("an interleaved receipt must not cancel another collection's response");
        let content = frames
            .iter()
            .filter(|frame| frame["kind"] == "chunk")
            .map(|frame| frame["data"].as_str().unwrap())
            .collect::<String>();
        assert_eq!(
            content, large,
            "every byte of the framed response must arrive"
        );
        let small_index = frames
            .iter()
            .position(|frame| frame["id"] == "small-command")
            .unwrap();
        let last_chunk = frames
            .iter()
            .rposition(|frame| frame["kind"] == "chunk")
            .unwrap();
        assert!(
            small_index < last_chunk,
            "small commands must still preempt large transfers"
        );
        assert_eq!(handler.transport_status.lock().retry_count, 0);
    }

    #[tokio::test]
    async fn queued_send_returns_its_receipt_before_starting_the_next_transfer() {
        let queues = Arc::new(Mutex::new(HashMap::new()));
        let peer = "own-receipt".to_owned();
        let queue = PeerSendQueue::default();
        let available = queue.drain_available.clone();
        queues.lock().insert(peer.clone(), queue);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut tx = Some(tx);
        let next_started = AtomicU64::new(0);
        let result = await_queued_send(&queues, &peer, available, rx, |guard| {
            let tx = tx.take().expect("only one drain turn is needed");
            let next_started = &next_started;
            async move {
                let _guard = guard;
                tx.send(Ok(())).unwrap();
                tokio::task::yield_now().await;
                next_started.fetch_add(1, Ordering::SeqCst);
                std::future::pending::<()>().await;
            }
        });
        tokio::time::timeout(Duration::from_secs(1), result)
            .await
            .expect("a sent frame must not wait for the rest of the queue")
            .unwrap()
            .unwrap();
        assert_eq!(next_started.load(Ordering::SeqCst), 0);
        assert!(!queues.lock()[&peer].draining);
    }

    #[test]
    fn cancelling_drain_wakes_an_already_queued_sender_without_a_new_request() {
        use std::future::Future;
        use std::task::{Context, Wake, Waker};
        struct CountWake(AtomicU64);
        impl Wake for CountWake {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let queues = Arc::new(Mutex::new(HashMap::new()));
        let peer = "handover".to_owned();
        let queue = PeerSendQueue::default();
        let available = queue.drain_available.clone();
        queues.lock().insert(peer.clone(), queue);
        let (_first_tx, first_rx) = tokio::sync::oneshot::channel();
        let mut first = Box::pin(await_queued_send(
            &queues,
            &peer,
            available.clone(),
            first_rx,
            |guard| async move {
                let _guard = guard;
                std::future::pending::<()>().await;
            },
        ));
        assert!(first
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        let (second_tx, second_rx) = tokio::sync::oneshot::channel();
        let mut second_tx = Some(second_tx);
        let mut second = Box::pin(await_queued_send(
            &queues,
            &peer,
            available,
            second_rx,
            |guard| {
                let tx = second_tx.take().unwrap();
                async move {
                    let _guard = guard;
                    tx.send(Ok(())).unwrap();
                    std::future::pending::<()>().await;
                }
            },
        ));
        let wake = Arc::new(CountWake(AtomicU64::new(0)));
        let waker = Waker::from(wake.clone());
        let mut context = Context::from_waker(&waker);
        assert!(second.as_mut().poll(&mut context).is_pending());
        assert_eq!(wake.0.load(Ordering::SeqCst), 0);
        drop(first);
        assert!(
            wake.0.load(Ordering::SeqCst) > 0,
            "the existing waiter must be scheduled"
        );
        assert!(second.as_mut().poll(&mut context).is_pending());
        assert!(matches!(
            second.as_mut().poll(&mut context),
            std::task::Poll::Ready(Ok(Ok(())))
        ));
        assert!(!queues.lock()[&peer].draining);
    }

    #[test]
    fn classifies_wire_frames_by_result_or_error_field() {
        let response = serde_json::to_value(WebRTCWireFrame::Response(WebRTCResponse {
            id: "r1".to_string(),
            result: Value::Null,
            error: None,
            collection: None,
        }))
        .unwrap();
        let message = serde_json::to_value(WebRTCWireFrame::Message(WebRTCMessage {
            id: "m1".to_string(),
            method: "token".to_string(),
            params: Vec::new(),
            collection: None,
        }))
        .unwrap();

        assert!(response.get("result").is_some() || response.get("error").is_some());
        assert!(message.get("result").is_none() && message.get("error").is_none());
    }

    #[test]
    fn auxiliary_chunk_transfer_ids_are_unique_for_parallel_responses() {
        let first = next_auxiliary_transfer_id();
        let second = next_auxiliary_transfer_id();
        assert_ne!(first, second);
        assert!(first.starts_with("browser-live-"));
        assert!(second.starts_with("browser-live-"));
    }

    #[test]
    fn wraps_ice_candidates_for_simple_peer_signal_shape() {
        let signal = simple_peer_ice_signal(RTCIceCandidateInit {
            candidate: "candidate:1 1 udp 1 127.0.0.1 123 typ host".to_string(),
            sdp_mid: Some("0".to_string()),
            sdp_mline_index: Some(0),
            username_fragment: Some("ufrag".to_string()),
            url: None,
        });

        assert_eq!(
            signal.get("type").and_then(Value::as_str),
            Some("candidate")
        );
        assert_eq!(
            signal
                .get("candidate")
                .and_then(|candidate| candidate.get("sdpMid"))
                .and_then(Value::as_str),
            Some("0")
        );
        assert_eq!(
            signal
                .get("candidate")
                .and_then(|candidate| candidate.get("sdpMLineIndex"))
                .and_then(Value::as_u64),
            Some(0)
        );
    }

    #[test]
    fn decodes_simple_peer_candidate_wrapper() {
        let signal = serde_json::json!({
            "type": "candidate",
            "candidate": {
                "candidate": "candidate:1 1 udp 1 127.0.0.1 123 typ host",
                "sdpMid": "0",
                "sdpMLineIndex": 0,
                "usernameFragment": "ufrag"
            }
        });

        let candidate = decode_simple_peer_ice_candidate(&signal).unwrap();

        assert_eq!(candidate.sdp_mid.as_deref(), Some("0"));
        assert_eq!(candidate.sdp_mline_index, Some(0));
        assert_eq!(candidate.username_fragment.as_deref(), Some("ufrag"));
    }

    #[test]
    fn splits_transport_chunks_on_utf8_boundaries() {
        // The raw-byte splitter this once covered is gone; `split_chunks_for_frame`
        // superseded it and bounds the *serialized* size instead. The invariant
        // worth keeping is that multi-byte text still reassembles exactly — a
        // splitter that cut mid-codepoint would corrupt every non-ASCII payload.
        let text = "aaäbb🙂cc".repeat(4096);
        let chunks = split_chunks_for_frame(&text, "ctox-core-peer-abcdef0123456789|frame|4242");

        assert!(chunks.len() > 1, "input must be large enough to split");
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn split_chunks_for_frame_bounds_serialized_size_even_for_escape_heavy_content() {
        let transfer_id = "ctox-core-peer-abcdef0123456789|frame|4242";
        // Worst case for JSON escaping: every byte is a control char (`\u00XX`, 6x)
        // or a quote/backslash (2x). A raw-byte chunker would have produced frames
        // far over the 16 KiB SCTP ceiling for this content.
        let payloads = [
            "\u{1}".repeat(200_000),        // all C0 controls -> 6x expansion
            "\"\\".repeat(150_000),         // all quotes+backslashes -> 2x
            "aäb🙂c\u{7}\"".repeat(40_000), // mixed multibyte + escapes
            "x".repeat(500_000),            // plain ASCII (no expansion)
            String::new(),                  // empty
        ];
        for payload in payloads {
            let chunks = split_chunks_for_frame(&payload, transfer_id);
            // Reassembly is lossless and order-preserving.
            assert_eq!(chunks.concat(), payload, "reassembly must equal original");
            // Every wrapped+serialized chunk frame stays within the SCTP ceiling.
            for (seq, data) in chunks.iter().enumerate() {
                let frame = transport_chunk_frame(transfer_id, 0, seq, data);
                let serialized = serde_json::to_string(&frame).unwrap();
                assert!(
                    serialized.len() <= MAX_SERIALIZED_FRAME_BYTES,
                    "serialized chunk frame {} bytes exceeds {} ceiling",
                    serialized.len(),
                    MAX_SERIALIZED_FRAME_BYTES
                );
            }
        }
    }

    #[test]
    fn json_escaped_char_len_matches_serde() {
        for ch in [
            'a', '"', '\\', '\n', '\t', '\u{08}', '\u{0c}', '\u{1}', 'ä', '🙂',
        ] {
            let s = ch.to_string();
            let serialized = serde_json::to_string(&s).unwrap();
            // serde wraps the value in quotes; strip them to compare inner length.
            let inner = serialized.len() - 2;
            assert_eq!(
                json_escaped_char_len(ch),
                inner,
                "escaped length mismatch for {ch:?}"
            );
        }
    }

    #[test]
    fn detects_ctox_transport_frames() {
        assert!(is_ctox_transport_frame(&serde_json::json!({
            "ctoxFrame": CTOX_FRAME_PROTOCOL,
            "kind": "start",
            "transferId": "t1"
        })));
        assert!(!is_ctox_transport_frame(&serde_json::json!({
            "id": "m1",
            "method": "token"
        })));
    }

    #[test]
    fn frame_protocol_fixture_matches_rust_constants() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/webrtc-frame-protocol.json"
        ))
        .unwrap();

        assert_eq!(
            fixture.get("protocol").and_then(Value::as_str),
            Some(CTOX_FRAME_PROTOCOL)
        );
        assert_eq!(
            fixture.get("maxInlineFrameBytes").and_then(Value::as_u64),
            Some(MAX_INLINE_FRAME_BYTES as u64)
        );
        assert_eq!(
            fixture.get("maxChunkBytes").and_then(Value::as_u64),
            Some(MAX_CHUNK_BYTES as u64)
        );
        assert_eq!(
            fixture.get("maxTransferBytes").and_then(Value::as_u64),
            Some(MAX_TRANSFER_BYTES as u64)
        );
        assert_eq!(
            fixture.get("ackWindow").and_then(Value::as_u64),
            Some(FRAME_ACK_WINDOW as u64)
        );
        assert_eq!(
            fixture.get("maxFrameRetries").and_then(Value::as_u64),
            Some(MAX_FRAME_RETRIES as u64)
        );
        for kind in ["start", "chunk", "ack", "resume"] {
            let frame = &fixture["frames"][kind];
            assert_eq!(
                frame.get("ctoxFrame").and_then(Value::as_str),
                Some(CTOX_FRAME_PROTOCOL)
            );
            assert_eq!(frame.get("kind").and_then(Value::as_str), Some(kind));
        }
        assert_eq!(
            fixture["frames"]["start"]
                .get("windowSize")
                .and_then(Value::as_u64),
            Some(FRAME_ACK_WINDOW as u64)
        );
        assert_eq!(
            fixture["frames"]["ack"]
                .get("receivedFrames")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            fixture["frames"]["ack"]
                .get("resume")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            fixture["frames"]["resume"]
                .get("ackSeq")
                .and_then(Value::as_u64),
            fixture["frames"]["ack"]
                .get("ackSeq")
                .and_then(Value::as_u64)
        );
    }

    #[test]
    fn frame_transport_status_exposes_protocol_counters() {
        let handler = WebRTCRsConnectionHandler::new();

        handler.record_sent_transport_frame(&serde_json::json!({
            "ctoxFrame": CTOX_FRAME_PROTOCOL,
            "kind": "chunk",
            "transferId": "t1",
            "seq": 0,
            "data": "abc"
        }));
        handler.record_received_transport_frame(&serde_json::json!({
            "ctoxFrame": CTOX_FRAME_PROTOCOL,
            "kind": "ack",
            "transferId": "t1",
            "ackSeq": 0
        }));
        let (ack_tx, _ack_rx) = tokio::sync::oneshot::channel();
        handler.pending_frame_acks.lock().insert(
            PendingFrameAckKey::new(&"peer-1".to_string(), "t1", 0),
            PendingFrameAck {
                sender: ack_tx,
                sent_at_ms: now_ms(),
            },
        );

        let status = handler.frame_transport_status();
        assert_eq!(status.protocol, CTOX_FRAME_PROTOCOL);
        assert_eq!(status.max_transfer_bytes, MAX_TRANSFER_BYTES);
        assert_eq!(status.ack_window, FRAME_ACK_WINDOW);
        assert_eq!(status.pending_acks, 1);
        assert_eq!(status.sent_frames, 1);
        assert_eq!(status.received_frames, 1);
        assert!(status.sent_bytes > 0);
        assert!(status.received_bytes > 0);

        let json = handler.frame_transport_status_json();
        assert_eq!(
            json.get("protocol").and_then(Value::as_str),
            Some(CTOX_FRAME_PROTOCOL)
        );
        assert_eq!(json.get("pendingAcks").and_then(Value::as_u64), Some(1));
        assert_eq!(json.get("sentFrames").and_then(Value::as_u64), Some(1));
        assert_eq!(json.get("receivedFrames").and_then(Value::as_u64), Some(1));
    }

    #[test]
    fn classifies_send_priority_for_scheduler() {
        let empty = HashSet::new();
        let token = WebRTCWireFrame::Message(WebRTCMessage {
            id: "m1".to_string(),
            method: "token".to_string(),
            params: Vec::new(),
            collection: None,
        });
        let response = WebRTCWireFrame::Response(WebRTCResponse {
            id: "r1".to_string(),
            result: Value::Null,
            error: None,
            collection: None,
        });
        let large_write = WebRTCWireFrame::Message(WebRTCMessage {
            id: "m2".to_string(),
            method: "masterWrite".to_string(),
            params: Vec::new(),
            collection: Some("documents".to_string()),
        });

        // Control frames stay High regardless of the active set.
        assert_eq!(
            classify_send_frame(&token, "{}").classify(&empty),
            SendPriority::High
        );
        assert_eq!(
            classify_send_frame(&response, "{}").classify(&empty),
            SendPriority::High
        );
        // An oversized masterWrite is Low even when its collection is active.
        let active_docs: HashSet<String> = ["documents".to_string()].into_iter().collect();
        assert_eq!(
            classify_send_frame(&large_write, &"x".repeat(MAX_INLINE_FRAME_BYTES + 1))
                .classify(&active_docs),
            SendPriority::Low
        );
    }

    #[test]
    fn active_collection_frame_is_high_priority_others_normal() {
        // Phase 2: a normal-sized masterWrite/masterChangesSince for the active
        // (foreground) collection is High; for a background collection it is
        // Normal. This is what lets the foreground collection's data jump ahead
        // of background bulk on the shared DataChannel.
        let active: HashSet<String> = ["documents".to_string()].into_iter().collect();
        let foreground = WebRTCWireFrame::Message(WebRTCMessage {
            id: "f".to_string(),
            method: "masterChangesSince".to_string(),
            params: Vec::new(),
            collection: Some("documents".to_string()),
        });
        let background = WebRTCWireFrame::Message(WebRTCMessage {
            id: "b".to_string(),
            method: "masterChangesSince".to_string(),
            params: Vec::new(),
            collection: Some("customer_accounts".to_string()),
        });
        assert_eq!(
            classify_send_frame(&foreground, "{}").classify(&active),
            SendPriority::High
        );
        assert_eq!(
            classify_send_frame(&background, "{}").classify(&active),
            SendPriority::Normal
        );
    }

    #[tokio::test]
    async fn active_collection_predicate_tracks_control_plane_state() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = install_test_connection(&handler, "peer-1", 1).await;

        // Fail-open before the first report: relays for inactive collections
        // are DROPPED, so an unreported peer must count as all-active or the
        // handshake→first-report window silently loses events forever.
        assert!(handler.is_collection_active_for_peer(&peer, "documents"));

        let msg = WebRTCMessage {
            id: "ac".to_string(),
            method: ACTIVE_COLLECTIONS_METHOD.to_string(),
            params: vec![serde_json::json!(["documents", "business_commands"])],
            collection: None,
        };
        // The first report transitions from fail-open: nothing was dropped
        // before it, so nothing needs a resync.
        let newly_activated = handler.apply_active_collections(&peer.peer_id, &msg);
        assert!(newly_activated.is_empty());

        assert!(handler.is_collection_active_for_peer(&peer, "documents"));
        assert!(handler.is_collection_active_for_peer(&peer, "business_commands"));
        assert!(!handler.is_collection_active_for_peer(&peer, "ctox_ticket_self_work_notes"));
        handler.close().await.unwrap();
    }

    /// REGRESSION (gating catch-up): relays for inactive collections are
    /// dropped and browser pulls are purely event-driven, so RE-ACTIVATING a
    /// collection must surface which names need a resync push — otherwise a
    /// collection that was inactive while the master wrote (rxdb-soak
    /// workspace-large-file-viewer-restart: desktop_files inactive during
    /// ctox.file.materialize) stays stale in the browser forever.
    #[test]
    fn apply_active_collections_reports_reactivated_names() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-1".to_string();
        let report = |names: serde_json::Value| WebRTCMessage {
            id: "ac".to_string(),
            method: ACTIVE_COLLECTIONS_METHOD.to_string(),
            params: vec![names],
            collection: None,
        };

        assert!(handler
            .apply_active_collections(&peer, &report(serde_json::json!(["business_commands"])))
            .is_empty());
        // Re-activation after a reported set without the collection: resync.
        let activated = handler.apply_active_collections(
            &peer,
            &report(serde_json::json!(["business_commands", "desktop_files"])),
        );
        assert_eq!(activated, vec!["desktop_files".to_string()]);
        // Unchanged set: idempotent no-op.
        assert!(handler
            .apply_active_collections(
                &peer,
                &report(serde_json::json!(["business_commands", "desktop_files"])),
            )
            .is_empty());
        // Dropping a collection re-activates nothing.
        assert!(handler
            .apply_active_collections(&peer, &report(serde_json::json!(["desktop_files"])))
            .is_empty());
        // ...but bringing it back resyncs it.
        let reactivated = handler.apply_active_collections(
            &peer,
            &report(serde_json::json!(["business_commands", "desktop_files"])),
        );
        assert_eq!(reactivated, vec!["business_commands".to_string()]);
    }

    fn presence_report(entries: serde_json::Value) -> WebRTCMessage {
        WebRTCMessage {
            id: "pr".to_string(),
            method: CTOX_PRESENCE_RPC_UPDATE.to_string(),
            params: vec![entries],
            collection: None,
        }
    }

    #[test]
    fn apply_presence_stores_caps_and_detects_change() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-1".to_string();

        // First report with entries: a visible change.
        let entry = serde_json::json!({
            "collection": "customer_accounts",
            "recordId": "acct-1",
            "actorId": "user-a",
        });
        assert!(handler.apply_presence(&peer, &presence_report(serde_json::json!([entry]))));

        // Entry-identical refresh: TTL clock re-stamped, but NOT a change —
        // refreshes must not fan a broadcast to every peer every refresh tick.
        assert!(!handler.apply_presence(&peer, &presence_report(serde_json::json!([entry]))));

        // Non-object entries are dropped; the count is capped at the contract
        // maximum so a hostile peer cannot balloon the aggregate frame.
        let mut many = Vec::new();
        for index in 0..(CTOX_PRESENCE_MAX_ENTRIES_PER_PEER + 8) {
            many.push(serde_json::json!({ "recordId": format!("r-{index}") }));
        }
        many.push(serde_json::json!("not-an-object"));
        assert!(handler.apply_presence(&peer, &presence_report(serde_json::json!(many))));
        let stored = handler.presence.lock().get(&peer).cloned().unwrap();
        assert_eq!(stored.entries.len(), CTOX_PRESENCE_MAX_ENTRIES_PER_PEER);
        assert!(stored.entries.iter().all(Value::is_object));

        // An empty report clears the peer's presence (tab navigated away).
        assert!(handler.apply_presence(&peer, &presence_report(serde_json::json!([]))));
        assert!(handler.presence.lock().get(&peer).is_none());
        // Clearing an already-clear peer is not a change.
        assert!(!handler.apply_presence(&peer, &presence_report(serde_json::json!([]))));
    }

    #[test]
    fn presence_aggregate_excludes_recipient_and_expired_reports() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer_a = "peer-a".to_string();
        let peer_b = "peer-b".to_string();
        let peer_c = "peer-c".to_string();
        let entry = |actor: &str| serde_json::json!({ "actorId": actor, "recordId": "r-1" });

        assert!(handler.apply_presence(&peer_a, &presence_report(serde_json::json!([entry("a")]))));
        assert!(handler.apply_presence(&peer_b, &presence_report(serde_json::json!([entry("b")]))));
        assert!(handler.apply_presence(&peer_c, &presence_report(serde_json::json!([entry("c")]))));
        let now = now_ms();

        // Each recipient sees everyone's entries but its own.
        let for_a = handler.presence_entries_excluding(&peer_a, now);
        assert_eq!(for_a.len(), 2);
        assert!(!for_a.iter().any(|e| e["actorId"] == "a"));

        // A report older than the TTL is invisible to recipients...
        handler
            .presence
            .lock()
            .get_mut(&peer_b)
            .unwrap()
            .updated_at_ms = now - CTOX_PRESENCE_TTL_MS - 1;
        let for_a = handler.presence_entries_excluding(&peer_a, now);
        assert_eq!(for_a.len(), 1);
        assert_eq!(for_a[0]["actorId"], "c");

        // ...and the sweep prunes it; a second sweep finds nothing.
        assert!(handler.prune_expired_presence(now));
        assert!(!handler.prune_expired_presence(now));
        assert!(handler.presence.lock().get(&peer_b).is_none());

        // Peer close drops presence and reports whether survivors need a push.
        assert!(handler.remove_peer_presence(&peer_c));
        assert!(!handler.remove_peer_presence(&peer_c));
        assert!(handler
            .presence_entries_excluding(&peer_b, now)
            .iter()
            .all(|e| e["actorId"] == "a"));
    }

    #[test]
    fn apply_active_collections_reprioritizes_queued_frames() {
        // Phase 2: a frame for a background collection is enqueued Normal, then
        // `rxdb.activeCollections` promotes that collection — the still-queued
        // frame must be re-bucketed to High and drain ahead of older Normal
        // frames for other collections.
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-1".to_string();
        let make = |collection: &str| {
            let (tx, _rx) = tokio::sync::oneshot::channel();
            (
                QueuedSend {
                    text: "{}".to_string(),
                    priority: SendPriority::Normal,
                    collection: Some(collection.to_string()),
                    intrinsic_high: false,
                    oversized_write: false,
                    queued_at_ms: now_ms(),
                    result: tx,
                },
                _rx,
            )
        };
        let (docs_item, _docs_rx) = make("documents");
        let (cust_item, _cust_rx) = make("customer_accounts");
        {
            let mut queues = handler.send_queues.lock();
            let queue = queues.entry(peer.clone()).or_default();
            queue.push(cust_item);
            queue.push(docs_item);
            // Both Normal; nothing in High yet.
            assert_eq!(queue.high.len(), 0);
            assert_eq!(queue.normal.len(), 2);
        }
        // Browser reports `documents` as the active/foreground collection.
        let msg = WebRTCMessage {
            id: "ac".to_string(),
            method: ACTIVE_COLLECTIONS_METHOD.to_string(),
            params: vec![serde_json::json!(["documents"])],
            collection: None,
        };
        handler.apply_active_collections(&peer, &msg);
        let mut queues = handler.send_queues.lock();
        let queue = queues.get_mut(&peer).expect("queue exists");
        // `documents` promoted to High; the other stays Normal.
        assert_eq!(queue.high.len(), 1);
        assert_eq!(queue.normal.len(), 1);
        let next = queue.pop_next().expect("a frame");
        assert_eq!(next.collection.as_deref(), Some("documents"));
        assert_eq!(next.priority, SendPriority::High);
    }

    #[test]
    fn browser_live_response_jumps_ahead_once_without_starving_sync_work() {
        let mut queue = PeerSendQueue::default();
        let make = |collection: &str| {
            let (tx, _rx) = tokio::sync::oneshot::channel();
            QueuedSend {
                text: "{}".to_string(),
                priority: SendPriority::High,
                collection: Some(collection.to_string()),
                intrinsic_high: true,
                oversized_write: false,
                queued_at_ms: now_ms(),
                result: tx,
            }
        };
        queue.push(make("business_commands"));
        queue.push(make("browser_sessions"));
        queue.push(make(CTOX_BROWSER_LIVE_RESPONSE_COLLECTION));
        queue.push(make(CTOX_BROWSER_LIVE_RESPONSE_COLLECTION));

        let next = queue.pop_next().expect("browser live response");
        assert_eq!(
            next.collection.as_deref(),
            Some(CTOX_BROWSER_LIVE_RESPONSE_COLLECTION)
        );
        queue.push(make(CTOX_BROWSER_INPUT_RESPONSE_COLLECTION));
        let next = queue.pop_next().expect("interactive input response");
        assert_eq!(
            next.collection.as_deref(),
            Some(CTOX_BROWSER_INPUT_RESPONSE_COLLECTION)
        );
        let next = queue.pop_next().expect("waiting sync response");
        assert_eq!(next.collection.as_deref(), Some("business_commands"));
    }

    #[test]
    fn chunked_transfer_preemption_selects_only_small_high_priority_frames() {
        let mut queue = PeerSendQueue::default();
        let make = |text: String, collection: &str| {
            let (tx, _rx) = tokio::sync::oneshot::channel();
            QueuedSend {
                text,
                priority: SendPriority::High,
                collection: Some(collection.to_string()),
                intrinsic_high: true,
                oversized_write: false,
                queued_at_ms: now_ms(),
                result: tx,
            }
        };
        let large_len = MAX_INLINE_FRAME_BYTES + 1;
        queue.push(make("x".repeat(large_len), "large-response"));
        queue.push(make("{}".to_string(), "interactive-response"));

        let selected = queue
            .pop_high_priority_inline()
            .expect("small high-priority response must preempt the chunked transfer");
        assert_eq!(selected.collection.as_deref(), Some("interactive-response"));
        assert_eq!(queue.high.len(), 1);
        assert_eq!(
            queue
                .high
                .front()
                .and_then(|item| item.collection.as_deref()),
            Some("large-response")
        );
        assert_eq!(queue.queued_bytes, large_len);
        assert!(queue.pop_high_priority_inline().is_none());
    }

    #[test]
    fn default_handler_keeps_the_managed_udp_bind_sentinel() {
        let handler = WebRTCRsConnectionHandler::new();
        assert_eq!(handler.udp_bind_addr, DEFAULT_UDP_BIND_ADDR);
    }

    #[test]
    fn managed_udp_bind_never_advertises_an_unspecified_host_candidate() {
        let addresses = advertisable_udp_bind_addrs(DEFAULT_UDP_BIND_ADDR);
        assert_eq!(addresses.len(), 1);
        assert_ne!(addresses[0], DEFAULT_UDP_BIND_ADDR);
        assert!(!addresses[0].starts_with("0.0.0.0:"));
    }

    #[test]
    fn explicit_udp_bind_address_is_preserved() {
        assert_eq!(
            advertisable_udp_bind_addrs("192.0.2.42:0"),
            vec!["192.0.2.42:0".to_string()]
        );
    }

    #[test]
    fn inbound_offer_rebuilds_only_unopened_responder_peer() {
        assert!(!should_rebuild_peer_for_inbound_offer(false, false));
        assert!(should_rebuild_peer_for_inbound_offer(true, false));
        assert!(!should_rebuild_peer_for_inbound_offer(true, true));
    }

    #[test]
    fn stale_peer_generation_cannot_remove_replacement() {
        assert!(peer_generation_matches(Some(2), Some(2)));
        assert!(!peer_generation_matches(Some(2), Some(1)));
        assert!(!peer_generation_matches(None, Some(1)));
        assert!(peer_generation_matches(Some(2), None));
        assert!(peer_generation_matches(None, None));
    }

    #[test]
    fn frame_transport_status_exposes_send_queue_depths() {
        let handler = WebRTCRsConnectionHandler::new();
        let (high_tx, _high_rx) = tokio::sync::oneshot::channel();
        let (low_tx, _low_rx) = tokio::sync::oneshot::channel();
        let mut queue = PeerSendQueue::default();
        queue.push(QueuedSend {
            text: "{}".to_string(),
            priority: SendPriority::High,
            collection: None,
            intrinsic_high: true,
            oversized_write: false,
            queued_at_ms: now_ms(),
            result: high_tx,
        });
        queue.push(QueuedSend {
            text: "{}".to_string(),
            priority: SendPriority::Low,
            collection: None,
            intrinsic_high: false,
            oversized_write: true,
            queued_at_ms: now_ms(),
            result: low_tx,
        });
        handler
            .send_queues
            .lock()
            .insert("peer-1".to_string(), queue);

        let status = handler.frame_transport_status();
        assert_eq!(status.priority_queue_depth, 2);
        assert_eq!(status.high_priority_queue_depth, 1);
        assert_eq!(status.low_priority_queue_depth, 1);

        let json = handler.frame_transport_status_json();
        assert_eq!(
            json.get("priorityQueueDepth").and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            json.get("highPriorityQueueDepth").and_then(Value::as_u64),
            Some(1)
        );
    }

    #[test]
    fn weighted_send_queue_gives_low_priority_bounded_progress() {
        let mut queue = PeerSendQueue::default();
        for index in 0..12 {
            let (tx, _rx) = tokio::sync::oneshot::channel();
            queue.push(QueuedSend {
                text: format!("high-{index}"),
                priority: SendPriority::High,
                collection: None,
                intrinsic_high: true,
                oversized_write: false,
                queued_at_ms: now_ms(),
                result: tx,
            });
        }
        let (low_tx, _low_rx) = tokio::sync::oneshot::channel();
        queue.push(QueuedSend {
            text: "low".to_string(),
            priority: SendPriority::Low,
            collection: None,
            intrinsic_high: false,
            oversized_write: true,
            queued_at_ms: now_ms(),
            result: low_tx,
        });
        let mut low_position = None;
        for position in 0..FAIR_SEND_SCHEDULE.len() {
            let item = queue.pop_next().expect("scheduled item");
            if item.priority == SendPriority::Low {
                low_position = Some(position);
                break;
            }
        }
        assert!(
            low_position.is_some(),
            "low priority must progress within one weighted schedule cycle"
        );
    }

    #[test]
    fn rust_transport_frame_builders_match_shared_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/webrtc-frame-protocol.json"
        ))
        .unwrap();
        let transfer_id = fixture["frames"]["start"]
            .get("transferId")
            .and_then(Value::as_str)
            .unwrap();

        let start = transport_start_frame(transfer_id, 0, 3, 30000);
        assert_eq!(start, fixture["frames"]["start"]);

        let chunk = transport_chunk_frame(transfer_id, 0, 1, "payload-fragment");
        assert_eq!(chunk, fixture["frames"]["chunk"]);

        let ack = transport_ack_frame(transfer_id, 1, 2, false, false);
        assert_eq!(ack, fixture["frames"]["ack"]);

        let resume = transport_resume_frame(transfer_id, 0, 1);
        assert_eq!(resume, fixture["frames"]["resume"]);
    }

    // Phase 1: native SCTP send-buffer backpressure must gate the sender while
    // the channel is over the high watermark and release promptly on the low
    // event — never deadlock. Drives the event-driven flow control directly
    // (no real data channel needed).
    #[tokio::test]
    async fn backpressure_gates_send_capacity_and_releases_on_low() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = install_test_connection(&handler, "peer-1", 1).await;
        let available = test_send_queue(&handler, peer.peer_id());

        // No backpressure registered yet → nothing buffered, capacity free.
        assert_eq!(handler.buffered_bytes(&peer), 0);

        let bp = handler.peer_backpressure(&peer.peer_id);
        bp.set_high();
        // While buffered we report above the high watermark so the demand
        // dispatchers' `buffered_bytes > high_water` guards engage.
        assert!(handler.buffered_bytes(&peer) > DATA_CHANNEL_BUFFERED_HIGH_WATER as usize);

        let bp_for_clear = Arc::clone(&bp);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            bp_for_clear.clear_high();
        });
        // Must block while high, then return well under the 30s wait cap
        // once the low event clears it.
        tokio::time::timeout(
            Duration::from_secs(2),
            handler.wait_for_send_capacity(&peer.peer_id, &available),
        )
        .await
        .expect("wait_for_send_capacity did not release after OnBufferedAmountLow")
        .expect("capacity wait should succeed after OnBufferedAmountLow");

        assert_eq!(handler.buffered_bytes(&peer), 0);
        handler.close().await.unwrap();
    }

    #[test]
    fn wait_for_send_capacity_returns_immediately_without_backpressure() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-2".to_string();
        let available = test_send_queue(&handler, &peer);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            tokio::time::timeout(
                Duration::from_millis(100),
                handler.wait_for_send_capacity(&peer, &available),
            )
            .await
            .expect("wait_for_send_capacity blocked despite no backpressure")
            .expect("capacity wait should succeed without backpressure");
        });
    }

    #[tokio::test(start_paused = true)]
    async fn send_capacity_timeout_is_terminal_and_typed() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-stalled".to_string();
        handler.peer_backpressure(&peer).set_high();
        let (ack_tx, _ack_rx) = tokio::sync::oneshot::channel();
        handler.pending_frame_acks.lock().insert(
            PendingFrameAckKey::new(&peer, "peer-stalled|frame|1", 0),
            PendingFrameAck {
                sender: ack_tx,
                sent_at_ms: now_ms(),
            },
        );
        handler.incoming_frames.lock().insert(
            TransferStateKey::new(&peer, "incoming-1"),
            IncomingFrame {
                peer: peer.clone(),
                attempt: 0,
                total_frames: 1,
                total_bytes: 1,
                received_bytes: 0,
                next_ack_seq: 0,
                received: vec![None],
            },
        );
        handler.completed_frame_acks.lock().insert(
            TransferStateKey::new(&peer, "completed-1"),
            CompletedFrameAck {
                peer: peer.clone(),
                ack_seq: 0,
                received_frames: 1,
                inserted_at_ms: now_ms(),
            },
        );
        let (queued_tx, queued_rx) = tokio::sync::oneshot::channel();
        handler
            .send_queues
            .lock()
            .entry(peer.clone())
            .or_default()
            .push(QueuedSend {
                text: "queued-after-stall".to_string(),
                priority: SendPriority::Normal,
                collection: None,
                intrinsic_high: false,
                oversized_write: false,
                queued_at_ms: now_ms(),
                result: queued_tx,
            });

        let available = test_send_queue(&handler, &peer);
        let wait = handler.wait_for_send_capacity(&peer, &available);
        tokio::pin!(wait);
        assert!(matches!(
            futures::poll!(&mut wait),
            std::task::Poll::Pending
        ));
        tokio::time::advance(SEND_CAPACITY_WAIT_TIMEOUT + Duration::from_millis(1)).await;
        let error = wait.await.expect_err("stalled capacity wait must fail");

        assert_eq!(error.code(), SEND_BUFFER_STALLED_ERROR_CODE);
        assert_eq!(handler.frame_transport_status().backpressure_stall_count, 1);
        assert_eq!(handler.frame_transport_status().rejected_frames, 1);
        assert!(!handler.peer_backpressure(&peer).is_high());
        assert!(handler.pending_frame_acks.lock().is_empty());
        assert!(handler.incoming_frames.lock().is_empty());
        assert!(handler.completed_frame_acks.lock().is_empty());
        let queued_error = queued_rx
            .await
            .expect("queued result sender must be resolved")
            .expect_err("queued send must be rejected");
        assert_eq!(queued_error.code(), SEND_BUFFER_STALLED_ERROR_CODE);
    }

    #[tokio::test(start_paused = true)]
    async fn stale_capacity_wait_preserves_replacement_queue_and_token() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "reused-route".to_owned();
        let old_available = test_send_queue(&handler, &peer);
        handler.peer_backpressure(&peer).set_high();
        let wait = handler.wait_for_send_capacity(&peer, &old_available);
        tokio::pin!(wait);
        assert!(matches!(
            futures::poll!(&mut wait),
            std::task::Poll::Pending
        ));

        let replacement = PeerSendQueue {
            draining: true,
            ..Default::default()
        };
        let new_available = replacement.drain_available.clone();
        handler.send_queues.lock().insert(peer.clone(), replacement);
        let new_bp = Arc::new(PeerBackpressure::new());
        new_bp.set_high();
        handler
            .backpressure
            .lock()
            .insert(peer.clone(), new_bp.clone());
        handler
            .peer_capability_tokens
            .lock()
            .insert(peer.clone(), "replacement-token".into());

        // A sender entering after replacement must not begin waiting on the
        // new connection's buffer with its old queue ownership.
        assert!(handler
            .wait_for_send_capacity(&peer, &old_available)
            .await
            .is_err());
        // Also cover a capacity wait that was already parked on the old signal.
        tokio::time::advance(SEND_CAPACITY_WAIT_TIMEOUT + Duration::from_millis(1)).await;
        let error = wait.await.expect_err("retired sender must fail");
        assert_eq!(error.parameters()[EXPECTED_PEER_TEARDOWN_PARAM], true);
        assert!(handler.is_current_send_queue(&peer, &new_available));
        assert!(handler.send_queues.lock()[&peer].draining);
        assert!(new_bp.is_high());
        assert_eq!(
            handler.peer_capability_tokens.lock()[&peer],
            "replacement-token"
        );
        assert_eq!(handler.frame_transport_status().backpressure_stall_count, 0);
    }

    #[tokio::test]
    async fn stale_offer_cleanup_cannot_remove_an_open_or_replaced_connection() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = install_test_connection(&handler, "offer-route", 2).await;
        assert!(!remove_peer_inner(
            &handler,
            peer.peer_id(),
            PeerRemoval::Unopened(2),
            None
        ));
        assert_eq!(
            handler.connection_for_peer(peer.peer_id()),
            Some(peer.clone())
        );
        handler
            .peers
            .lock()
            .get_mut(peer.peer_id())
            .unwrap()
            .data_channel_open = false;
        assert!(!remove_peer_inner(
            &handler,
            peer.peer_id(),
            PeerRemoval::Unopened(1),
            None
        ));
        assert!(handler.peers.lock().contains_key(peer.peer_id()));
        assert!(remove_peer_inner(
            &handler,
            peer.peer_id(),
            PeerRemoval::Unopened(2),
            None
        ));
        assert!(!handler.peers.lock().contains_key(peer.peer_id()));
        handler.close().await.unwrap();
    }

    #[tokio::test]
    async fn retired_channel_exit_preserves_replacement_presence_and_backpressure() {
        let handler = WebRTCRsConnectionHandler::new();
        let connection = install_test_connection(&handler, "channel-route", 2).await;
        let route = connection.peer_id().to_owned();
        let old_signal = Arc::new(PeerBackpressure::new());
        let current_signal = handler.peer_backpressure(&route);
        current_signal.set_high();
        let report = presence_report(serde_json::json!([{ "recordId": "replacement-record" }]));
        assert!(handler.apply_presence(&route, &report));

        // Model the old task resuming after the replacement registered, after
        // an earlier generation check has already succeeded on the old task.
        assert_eq!(
            finish_data_channel_generation(&handler, &route, 1, &old_signal),
            None
        );
        assert_eq!(handler.connection_for_peer(&route), Some(connection));
        assert!(Arc::ptr_eq(
            &handler.backpressure.lock()[&route],
            &current_signal
        ));
        assert!(current_signal.is_high());
        assert_eq!(
            handler.presence.lock()[&route].entries[0]["recordId"],
            "replacement-record"
        );

        // Current termination still retires its own state and permits a fresh offer.
        assert_eq!(
            finish_data_channel_generation(&handler, &route, 2, &current_signal),
            Some(true)
        );
        assert!(handler.connection_for_peer(&route).is_none());
        assert!(!handler.backpressure.lock().contains_key(&route));
        assert!(!handler.presence.lock().contains_key(&route));
        assert!(
            !handler.peers.lock().contains_key(&route),
            "a terminated channel must not remain a cached native connection"
        );
        assert!(
            handler
                .ensure_peer_connection(route.clone(), true)
                .await
                .is_err(),
            "without signaling, a fresh connection must fail instead of returning the closed peer"
        );
        handler.close().await.unwrap();
    }

    #[test]
    fn completed_frame_ack_cache_evicts_aged_but_keeps_fresh_within_window() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-resume".to_string();
        let now = now_ms();

        // A stale entry, past the TTL, and a fresh entry still inside the
        // resume-after-complete window.
        handler.completed_frame_acks.lock().insert(
            TransferStateKey::new(&peer, "aged"),
            CompletedFrameAck {
                peer: peer.clone(),
                ack_seq: 3,
                received_frames: 4,
                inserted_at_ms: now.saturating_sub(COMPLETED_FRAME_ACK_TTL_MS + 1_000),
            },
        );
        handler.completed_frame_acks.lock().insert(
            TransferStateKey::new(&peer, "fresh"),
            CompletedFrameAck {
                peer: peer.clone(),
                ack_seq: 7,
                received_frames: 8,
                inserted_at_ms: now,
            },
        );

        // A newly completed transfer triggers the opportunistic prune.
        handler.record_completed_frame_ack(
            "newest".to_string(),
            CompletedFrameAck {
                peer: peer.clone(),
                ack_seq: 1,
                received_frames: 2,
                inserted_at_ms: now,
            },
        );

        let cache = handler.completed_frame_acks.lock();
        // Aged entry is gone; a delayed resume for it correctly gets no ack.
        assert!(
            !cache.contains_key(&TransferStateKey::new(&peer, "aged")),
            "aged entry must be evicted past TTL"
        );
        // The fresh entry survives, so a resume within the window still finds
        // its final ack (docs §6.3).
        let fresh = cache
            .get(&TransferStateKey::new(&peer, "fresh"))
            .expect("fresh entry must survive prune");
        assert_eq!(fresh.ack_seq, 7);
        assert_eq!(fresh.received_frames, 8);
        assert_eq!(fresh.peer, peer);
        assert!(cache.contains_key(&TransferStateKey::new(&peer, "newest")));
    }

    #[test]
    fn completed_frame_ack_lookup_only_replays_for_the_original_peer() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-completed".to_string();
        handler.record_completed_frame_ack(
            "transfer-completed".to_string(),
            CompletedFrameAck {
                peer: peer.clone(),
                ack_seq: 5,
                received_frames: 6,
                inserted_at_ms: now_ms(),
            },
        );

        assert_eq!(
            handler.completed_frame_ack_for("transfer-completed", &peer),
            Some((5, 6))
        );
        assert_eq!(
            handler.completed_frame_ack_for("transfer-completed", &"other-peer".to_string()),
            None
        );
    }

    #[test]
    fn completed_frame_ack_cache_caps_size_dropping_oldest_first() {
        let now = now_ms();
        let mut cache: HashMap<TransferStateKey, CompletedFrameAck> = HashMap::new();
        // One over the cap; each entry gets a distinct (recent) timestamp so the
        // oldest is unambiguous and none are TTL-evicted.
        for i in 0..=COMPLETED_FRAME_ACK_CAP {
            cache.insert(
                TransferStateKey::new(&"peer".to_string(), &format!("t{i}")),
                CompletedFrameAck {
                    peer: "peer".to_string(),
                    ack_seq: i,
                    received_frames: i + 1,
                    // i == 0 is the oldest (largest subtraction).
                    inserted_at_ms: now.saturating_sub((COMPLETED_FRAME_ACK_CAP - i) as u64),
                },
            );
        }
        assert_eq!(cache.len(), COMPLETED_FRAME_ACK_CAP + 1);

        WebRTCRsConnectionHandler::prune_completed_frame_acks(&mut cache, now);

        assert_eq!(cache.len(), COMPLETED_FRAME_ACK_CAP);
        // The oldest entry is the one dropped to honor the cap.
        assert!(
            !cache.contains_key(&TransferStateKey::new(&"peer".to_string(), "t0")),
            "oldest entry must be evicted first"
        );
        assert!(cache.contains_key(&TransferStateKey::new(
            &"peer".to_string(),
            &format!("t{COMPLETED_FRAME_ACK_CAP}"),
        )));
    }

    /// Regression for A0.4 finding 1: an ACK from one peer must never release
    /// another peer's waiter, even when both use the same caller-controlled
    /// transfer id and sequence.
    #[tokio::test]
    async fn frame_ack_waiters_are_scoped_by_peer() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer_a = "peer-a".to_string();
        let peer_b = "peer-b".to_string();
        let (ack_a_tx, ack_a_rx) = tokio::sync::oneshot::channel();
        let (ack_b_tx, mut ack_b_rx) = tokio::sync::oneshot::channel();
        handler.pending_frame_acks.lock().insert(
            PendingFrameAckKey::new(&peer_a, "shared-transfer", 3),
            PendingFrameAck {
                sender: ack_a_tx,
                sent_at_ms: now_ms(),
            },
        );
        handler.pending_frame_acks.lock().insert(
            PendingFrameAckKey::new(&peer_b, "shared-transfer", 3),
            PendingFrameAck {
                sender: ack_b_tx,
                sent_at_ms: now_ms(),
            },
        );

        let released = handler.take_pending_frame_acks(&peer_a, "shared-transfer", Some(3));
        assert_eq!(released.len(), 1);
        for pending in released {
            let _ = pending.sender.send(());
        }
        ack_a_rx.await.expect("peer A waiter released");
        assert!(matches!(
            futures::poll!(&mut ack_b_rx),
            std::task::Poll::Pending
        ));
        assert!(handler
            .pending_frame_acks
            .lock()
            .contains_key(&PendingFrameAckKey::new(&peer_b, "shared-transfer", 3)));
    }

    /// Regression for A0.4 finding 2: starts reserve peer-scoped state and are
    /// rejected observably when zero-byte or count/byte budgets are exceeded.
    #[test]
    fn incoming_transfer_allocation_enforces_peer_and_global_budgets() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer_a = "peer-a".to_string();
        let peer_b = "peer-b".to_string();

        let zero_error = handler
            .register_incoming_transfer(&peer_a, "zero", 0, 2, 0)
            .expect_err("multi-frame zero-byte transfer must be rejected");
        assert!(zero_error.to_string().contains("allocation"));

        handler
            .register_incoming_transfer(&peer_a, "shared", 1, 1, 1)
            .expect("peer A reservation");
        handler
            .register_incoming_transfer(&peer_b, "shared", 2, 1, 1)
            .expect("peer B may use the same transfer id");
        assert_eq!(handler.incoming_frames.lock().len(), 2);
        assert_eq!(
            handler
                .incoming_frames
                .lock()
                .get(&TransferStateKey::new(&peer_a, "shared"))
                .map(|entry| entry.attempt),
            Some(1)
        );

        let count_handler = WebRTCRsConnectionHandler::new();
        for index in 0..MAX_INCOMING_TRANSFERS_PER_PEER {
            count_handler
                .register_incoming_transfer(&peer_a, &format!("count-{index}"), 0, 1, 1)
                .expect("within per-peer count budget");
        }
        let per_peer_count_error = count_handler
            .register_incoming_transfer(&peer_a, "count-over", 0, 1, 1)
            .expect_err("per-peer transfer count must be bounded");
        assert!(per_peer_count_error.to_string().contains("budget exceeded"));

        let global_count_handler = WebRTCRsConnectionHandler::new();
        for index in 0..MAX_INCOMING_TRANSFERS_TOTAL {
            global_count_handler
                .register_incoming_transfer(
                    &format!("global-peer-{index}"),
                    &format!("global-{index}"),
                    0,
                    1,
                    1,
                )
                .expect("within global count budget");
        }
        global_count_handler
            .register_incoming_transfer(&"global-over".to_string(), "global-over", 0, 1, 1)
            .expect_err("global transfer count must be bounded");

        let per_peer_bytes_handler = WebRTCRsConnectionHandler::new();
        for index in 0..3 {
            per_peer_bytes_handler.incoming_frames.lock().insert(
                TransferStateKey::new(&peer_a, &format!("bytes-{index}")),
                IncomingFrame {
                    peer: peer_a.clone(),
                    attempt: 0,
                    total_frames: 1,
                    total_bytes: MAX_TRANSFER_BYTES,
                    received_bytes: 0,
                    next_ack_seq: 0,
                    received: Vec::new(),
                },
            );
        }
        per_peer_bytes_handler
            .register_incoming_transfer(&peer_a, "bytes-over", 0, 1, MAX_TRANSFER_BYTES)
            .expect_err("per-peer allocated-byte budget must be bounded");

        let global_bytes_handler = WebRTCRsConnectionHandler::new();
        for index in 0..15 {
            let peer = format!("byte-peer-{index}");
            global_bytes_handler.incoming_frames.lock().insert(
                TransferStateKey::new(&peer, &format!("byte-transfer-{index}")),
                IncomingFrame {
                    peer,
                    attempt: 0,
                    total_frames: 1,
                    total_bytes: MAX_TRANSFER_BYTES,
                    received_bytes: 0,
                    next_ack_seq: 0,
                    received: Vec::new(),
                },
            );
        }
        global_bytes_handler
            .register_incoming_transfer(
                &"byte-peer-over".to_string(),
                "byte-transfer-over",
                0,
                1,
                MAX_TRANSFER_BYTES,
            )
            .expect_err("aggregate allocated-byte budget must be bounded");
        assert!(
            global_bytes_handler
                .frame_transport_status()
                .rejected_frames
                > 0
        );
    }

    /// Regression for A0.4 finding 3: dropping the operation future (task abort)
    /// must restore accounting without relying on a manual return-path decrement.
    #[test]
    fn framed_transfer_guard_cleans_accounting_and_ack_waiters_on_drop() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-guard".to_string();
        let transfer_id = "guard-transfer";
        let guard = FramedTransferGuard::new(&handler, &peer, transfer_id);
        let (ack_tx, _ack_rx) = tokio::sync::oneshot::channel();
        handler.pending_frame_acks.lock().insert(
            PendingFrameAckKey::new(&peer, transfer_id, 0),
            PendingFrameAck {
                sender: ack_tx,
                sent_at_ms: now_ms(),
            },
        );
        assert_eq!(handler.frame_transport_status().active_transfers, 1);

        drop(guard);

        assert_eq!(handler.frame_transport_status().active_transfers, 0);
        assert!(handler.pending_frame_acks.lock().is_empty());
    }

    /// Regression for A0.4 finding 6: closing a handler aborts the sleeping
    /// presence sweep and makes re-arming terminally impossible.
    #[tokio::test]
    async fn close_aborts_presence_sweep_and_prevents_rearm() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-presence".to_string();
        assert!(handler.apply_presence(
            &peer,
            &presence_report(serde_json::json!([{ "recordId": "r1" }]))
        ));
        handler.schedule_presence_sweep();
        assert!(handler.presence_sweep_armed.load(Ordering::SeqCst));
        assert!(handler.presence_sweep_task.lock().is_some());

        handler.close().await.expect("close handler");
        handler.schedule_presence_sweep();

        assert!(handler.closed.load(Ordering::SeqCst));
        assert!(!handler.presence_sweep_armed.load(Ordering::SeqCst));
        assert!(handler.presence_sweep_task.lock().is_none());
        assert!(handler.presence.lock().is_empty());
    }
}
