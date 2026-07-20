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
    WebRTCWireFrame,
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

pub type CollectionAuthzHook = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;
pub type DocumentReadFilter = Arc<dyn Fn(&Value) -> bool + Send + Sync>;
pub type DocumentReadAuthzHook = Arc<dyn Fn(&str, &str) -> DocumentReadFilter + Send + Sync>;
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
    peer_connection: Arc<dyn PeerConnection>,
    data_channel: Option<Arc<dyn DataChannel>>,
    data_channel_open: bool,
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

struct IncomingFrame {
    peer: PeerId,
    attempt: u64,
    total_frames: usize,
    total_bytes: usize,
    next_ack_seq: usize,
    received: Vec<Option<String>>,
}

struct CompletedFrameAck {
    peer: PeerId,
    ack_seq: usize,
    received_frames: usize,
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
    queued_bytes: usize,
    schedule_cursor: usize,
}

impl PeerSendQueue {
    fn push(&mut self, item: QueuedSend) {
        self.queued_bytes = self.queued_bytes.saturating_add(item.text.len());
        match item.priority {
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
                SendPriority::High => self.high.pop_front(),
                SendPriority::Normal => self.normal.pop_front(),
                SendPriority::Low => self.low.pop_front(),
            };
            if let Some(item) = item {
                self.queued_bytes = self.queued_bytes.saturating_sub(item.text.len());
                return Some(item);
            }
        }
        None
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
    armed: bool,
}

impl Drop for DrainResetGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Some(queue) = self.queues.lock().get_mut(&self.peer) {
            queue.draining = false;
        }
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

/// WebRTC connection-handler implementation backed by `webrtc-rs`.
pub struct WebRTCRsConnectionHandler {
    connect_subject: RxSubject<WebRTCRsPeer>,
    disconnect_subject: RxSubject<WebRTCRsPeer>,
    message_subject: RxSubject<PeerWithMessage<WebRTCRsPeer>>,
    response_subject: RxSubject<PeerWithResponse<WebRTCRsPeer>>,
    error_subject: RxSubject<RxError>,
    peers: Arc<Mutex<HashMap<WebRTCRsPeer, PeerEntry>>>,
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
    incoming_frames: Arc<Mutex<HashMap<String, IncomingFrame>>>,
    completed_frame_acks: Arc<Mutex<HashMap<String, CompletedFrameAck>>>,
    pending_frame_acks: Arc<Mutex<HashMap<String, PendingFrameAck>>>,
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
    /// Set when a peer with visible presence was removed outside the normal
    /// broadcast paths (abrupt disconnect -> `remove_peer`); the next sweep
    /// broadcasts the corrected aggregate even when nothing expired.
    presence_dirty: Arc<std::sync::atomic::AtomicBool>,
    transport_status: Arc<Mutex<WebRtcFrameTransportStatus>>,
    frame_counter: AtomicU64,
    /// Phase 1: per-peer send-buffer backpressure (see `PeerBackpressure`).
    backpressure: Arc<Mutex<HashMap<WebRTCRsPeer, Arc<PeerBackpressure>>>>,
    /// #12c per-collection sync read-authz. When `collection_authz` is set,
    /// `is_collection_authorized_for_peer` consults it with the capability token
    /// the peer presented at handshake (captured into `peer_capability_tokens`).
    /// `None` => no enforcement (default), so replication behavior is unchanged.
    collection_authz: Arc<Mutex<Option<CollectionAuthzHook>>>,
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
        let handler = Arc::new(Self::empty(
            Some(Arc::clone(&config.signaling)),
            config.ice_servers,
            &config.data_channel_label,
            &config.udp_bind_addr,
        ));
        wait_for_own_peer_id(&config.signaling).await?;
        config.signaling.join(config.room).await?;
        handler.start_signaling_tasks();
        Ok(handler)
    }

    fn empty(
        signaling: Option<Arc<SignalingClient>>,
        ice_servers: Vec<RTCIceServer>,
        data_channel_label: &str,
        udp_bind_addr: &str,
    ) -> Self {
        Self {
            connect_subject: RxSubject::new(),
            disconnect_subject: RxSubject::new(),
            message_subject: RxSubject::new(),
            response_subject: RxSubject::new(),
            error_subject: RxSubject::new(),
            peers: Arc::new(Mutex::new(HashMap::new())),
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
            presence_dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            transport_status: Arc::new(Mutex::new(WebRtcFrameTransportStatus::default())),
            frame_counter: AtomicU64::new(0),
            backpressure: Arc::new(Mutex::new(HashMap::new())),
            collection_authz: Arc::new(Mutex::new(None)),
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

    /// Phase 1: pause the sender while the peer's SCTP send buffer is above the
    /// high watermark, so we never burst past what the channel can deliver in
    /// real time. Returns when the buffer has drained below the low watermark
    /// (OnBufferedAmountLow) or after `SEND_CAPACITY_WAIT_TIMEOUT` (so a wedged
    /// peer surfaces a timeout instead of hanging forever).
    async fn wait_for_send_capacity(&self, peer: &WebRTCRsPeer) -> RxResult<()> {
        let bp = self.peer_backpressure(peer);
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
                self.record_status(|status| {
                    status.backpressure_stall_count =
                        status.backpressure_stall_count.saturating_add(1);
                    status.rejected_frames = status.rejected_frames.saturating_add(1);
                });
                let error = new_rx_error(
                    SEND_BUFFER_STALLED_ERROR_CODE,
                    Some(serde_json::json!({
                        "message": "WebRTC send buffer remained above the high-water mark",
                        "peer": peer,
                        "timeoutMs": SEND_CAPACITY_WAIT_TIMEOUT.as_millis(),
                        "retryable": true,
                    })),
                );
                self.error_subject.next(error.clone());
                // A timed-out capacity wait is a transport failure, not
                // permission to keep filling SCTP. Removing the peer closes
                // the channel, drops every queued sender and clears all
                // per-peer backpressure state before the error is returned.
                remove_peer_with_error(self, peer, error.clone());
                return Err(error);
            }
        }
        Ok(())
    }

    fn clear_peer_transfer_state(&self, peer: &WebRTCRsPeer) {
        let ack_prefix = format!("{peer}|frame|");
        self.pending_frame_acks
            .lock()
            .retain(|key, _| !key.starts_with(&ack_prefix));
        self.incoming_frames
            .lock()
            .retain(|_, incoming| incoming.peer != *peer);
        self.completed_frame_acks
            .lock()
            .retain(|_, completed| completed.peer != *peer);
        self.refresh_dynamic_transport_status();
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

        let pc = build_peer_connection(
            Arc::clone(self),
            Arc::clone(&signaling),
            remote_peer_id.clone(),
        )
        .await?;
        self.peers.lock().insert(
            remote_peer_id.clone(),
            PeerEntry {
                peer_connection: Arc::clone(&pc),
                data_channel: None,
                data_channel_open: false,
                tasks: Vec::new(),
            },
        );

        if initiator {
            let data_channel = pc
                .create_data_channel(&self.data_channel_label, None)
                .await
                .map_err(|e| webrtc_error("create data channel", e))?;
            install_data_channel(Arc::clone(self), remote_peer_id.clone(), data_channel);
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
            self.remove_unopened_peer_before_offer(&remote_peer_id);
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

    fn remove_unopened_peer_before_offer(self: &Arc<Self>, remote_peer_id: &str) {
        let should_rebuild = {
            let peers = self.peers.lock();
            let Some(entry) = peers.get(remote_peer_id) else {
                return;
            };
            should_rebuild_peer_for_inbound_offer(true, entry.data_channel_open)
        };
        if should_rebuild {
            tracing::warn!(
                target: "ctox_rxdb::webrtc_rs",
                peer = %remote_peer_id,
                "dropping unopened WebRTC responder before answering renewed browser offer"
            );
            remove_peer(self, remote_peer_id);
        }
    }
}

fn should_rebuild_peer_for_inbound_offer(peer_exists: bool, data_channel_open: bool) -> bool {
    peer_exists && !data_channel_open
}

#[async_trait]
impl WebRTCConnectionHandler for WebRTCRsConnectionHandler {
    type Peer = WebRTCRsPeer;

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
        let data_channel = self
            .peers
            .lock()
            .get(peer)
            .and_then(|entry| entry.data_channel.clone())
            .ok_or_else(|| {
                new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(serde_json::json!({
                        "message": "unknown or unopened peer",
                        "peer": peer,
                    })),
                )
            })?;
        let text = serde_json::to_string(&frame).map_err(|e| {
            new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": format!("serialize WebRTC frame failed: {e}"),
                    "peer": peer,
                })),
            )
        })?;
        // Phase 2: derive the collection-aware priority class. Control frames
        // are intrinsically High; oversized writes are Low; everything else is
        // High when its collection is in the peer's active set, else Normal.
        let class = classify_send_frame(&frame, &text);
        self.send_queued_text(peer, data_channel, class).await
    }

    async fn close(&self) -> Result<(), RxError> {
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
            self.disconnect_subject.next(peer);
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
        match self.backpressure.lock().get(peer) {
            Some(bp) if bp.is_high() => DATA_CHANNEL_BUFFERED_HIGH_WATER as usize + 1,
            _ => 0,
        }
    }

    /// Phase 1: the signaling peer id is already a stable string; use it
    /// directly for authz / rate-limit keying instead of the opaque Debug form.
    fn peer_identity(&self, peer: &Self::Peer) -> String {
        peer.to_string()
    }

    fn is_collection_active_for_peer(&self, peer: &Self::Peer, collection: &str) -> bool {
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

    fn set_peer_capability_token(&self, peer: &Self::Peer, token: String) {
        self.peer_capability_tokens
            .lock()
            .insert(peer.clone(), token);
    }

    /// #12c: fail-open when no authz hook is installed (the default — behavior
    /// unchanged). When installed, an unknown peer maps to an empty token so the
    /// hook still decides (it treats an empty/invalid token as least privilege).
    fn is_collection_authorized_for_peer(&self, peer: &Self::Peer, collection: &str) -> bool {
        let hook = self.collection_authz.lock().clone();
        match hook {
            None => true,
            Some(check) => {
                let token = self
                    .peer_capability_tokens
                    .lock()
                    .get(peer)
                    .cloned()
                    .unwrap_or_default();
                check(&token, collection)
            }
        }
    }

    /// Fail-open write authorization unless a caller installs a write hook.
    fn is_collection_write_authorized_for_peer(&self, peer: &Self::Peer, collection: &str) -> bool {
        let hook = self.collection_write_authz.lock().clone();
        match hook {
            None => true,
            Some(check) => {
                let token = self
                    .peer_capability_tokens
                    .lock()
                    .get(peer)
                    .cloned()
                    .unwrap_or_default();
                check(&token, collection)
            }
        }
    }

    fn document_filter_for_peer(
        &self,
        peer: &Self::Peer,
        collection: &str,
    ) -> Option<Arc<dyn Fn(&Value) -> bool + Send + Sync>> {
        let hook = self.document_read_authz.lock().clone()?;
        let token = self
            .peer_capability_tokens
            .lock()
            .get(peer)
            .cloned()
            .unwrap_or_default();
        Some(hook(&token, collection))
    }

    fn are_documents_write_authorized_for_peer(
        &self,
        peer: &Self::Peer,
        collection: &str,
        params: &[Value],
    ) -> bool {
        let hook = self.document_write_authz.lock().clone();
        let Some(check) = hook else { return true };
        let token = self
            .peer_capability_tokens
            .lock()
            .get(peer)
            .cloned()
            .unwrap_or_default();
        params
            .first()
            .and_then(Value::as_array)
            .is_some_and(|rows| {
                rows.iter().all(|row| {
                    row.get("newDocumentState")
                        .is_some_and(|document| check(&token, collection, document))
                })
            })
    }

    fn filter_master_change_for_peer(
        &self,
        peer: &Self::Peer,
        collection: &str,
        change: crate::types::RxReplicationMasterChange,
    ) -> Option<crate::types::RxReplicationMasterChange> {
        let Some(filter) = self.document_filter_for_peer(peer, collection) else {
            return Some(change);
        };
        match change {
            crate::types::RxReplicationMasterChange::Resync => {
                Some(crate::types::RxReplicationMasterChange::Resync)
            }
            crate::types::RxReplicationMasterChange::Documents(mut documents) => {
                documents.documents.retain(|document| filter(document));
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
        remove_peer(self, peer);
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
    fn remove_peer_presence(&self, peer: &WebRTCRsPeer) -> bool {
        self.presence
            .lock()
            .remove(peer)
            .is_some_and(|report| !report.entries.is_empty())
    }

    /// Push the current aggregate to ONE peer (join snapshot on data-channel
    /// open). Best-effort like the broadcast.
    async fn push_presence_snapshot_to(self: &Arc<Self>, recipient: &WebRTCRsPeer) {
        let entries = self.presence_entries_excluding(recipient, now_ms());
        if entries.is_empty() {
            return;
        }
        let response = WebRTCResponse {
            id: CTOX_PRESENCE_STREAM_ID.to_string(),
            result: serde_json::json!({ "entries": entries }),
            error: None,
            collection: None,
        };
        let _ = self
            .send(recipient, WebRTCWireFrame::Response(response))
            .await;
    }

    /// Push the current presence aggregate to every open peer as a
    /// `presence$` response frame. Each recipient gets everyone's entries but
    /// its own. Best-effort: a send failure surfaces through the normal
    /// transport error path and must not stall the loop.
    async fn broadcast_presence(self: &Arc<Self>) {
        let now = now_ms();
        let recipients: Vec<WebRTCRsPeer> = self
            .peers
            .lock()
            .iter()
            .filter(|(_, entry)| entry.data_channel_open)
            .map(|(peer, _)| peer.clone())
            .collect();
        for recipient in recipients {
            let entries = self.presence_entries_excluding(&recipient, now);
            let response = WebRTCResponse {
                id: CTOX_PRESENCE_STREAM_ID.to_string(),
                result: serde_json::json!({ "entries": entries }),
                error: None,
                collection: None,
            };
            let _ = self
                .send(&recipient, WebRTCWireFrame::Response(response))
                .await;
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
        if self.presence.lock().is_empty() {
            return;
        }
        if self.presence_sweep_armed.swap(true, Ordering::SeqCst) {
            return; // a sweep task is already pending
        }
        let handler = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(CTOX_PRESENCE_TTL_MS + 1_000)).await;
            handler.presence_sweep_armed.store(false, Ordering::SeqCst);
            let pruned = handler.prune_expired_presence(now_ms());
            let dirty = handler.presence_dirty.swap(false, Ordering::SeqCst);
            if pruned || dirty {
                handler.broadcast_presence().await;
            }
            handler.schedule_presence_sweep();
        });
    }

    async fn send_queued_text(
        &self,
        peer: &WebRTCRsPeer,
        data_channel: Arc<dyn DataChannel>,
        class: SendFrameClass,
    ) -> Result<(), RxError> {
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
        let should_drain = {
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
            let should_drain = !queue.draining;
            queue.push(QueuedSend {
                text: class.text,
                priority,
                collection: class.collection,
                intrinsic_high: class.intrinsic_high,
                oversized_write: class.oversized_write,
                queued_at_ms: now_ms(),
                result: result_tx,
            });
            if should_drain {
                queue.draining = true;
            }
            should_drain
        };
        self.record_status(|status| {
            status.queued_frames = status.queued_frames.saturating_add(1);
            status.last_send_priority = priority.as_str();
        });
        self.refresh_send_queue_status();
        if should_drain {
            tokio::task::yield_now().await;
            self.drain_send_queue(peer, data_channel).await;
        }
        result_rx.await.map_err(|_| {
            new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": "WebRTC send queue result dropped",
                    "peer": peer,
                })),
            )
        })?
    }

    async fn drain_send_queue(&self, peer: &WebRTCRsPeer, data_channel: Arc<dyn DataChannel>) {
        // The drainer runs inside whatever task called `send_queued_text`
        // first. That task can be aborted mid-send (peer relays are aborted on
        // disconnect), which used to leave `draining == true` forever: later
        // senders for the same peer id queued behind a drainer that no longer
        // existed and parked on their result channel until process restart.
        // The guard re-opens the drain slot on cancellation; the clean-exit
        // path disarms it while holding the lock so no item can slip between
        // "queue observed empty" and "flag cleared".
        let mut reset_guard = DrainResetGuard {
            queues: Arc::clone(&self.send_queues),
            peer: peer.clone(),
            armed: true,
        };
        loop {
            let item = {
                let mut queues = self.send_queues.lock();
                let Some(queue) = queues.get_mut(peer) else {
                    // Queue removed (peer torn down) — nothing left to drain.
                    reset_guard.armed = false;
                    return;
                };
                match queue.pop_next() {
                    Some(item) => item,
                    None => {
                        queue.draining = false;
                        reset_guard.armed = false;
                        break;
                    }
                }
            };
            self.refresh_send_queue_status();
            self.record_status(|status| {
                status.sent_scheduled_frames = status.sent_scheduled_frames.saturating_add(1);
                status.last_send_priority = item.priority.as_str();
            });
            let result = if item.text.len() > MAX_INLINE_FRAME_BYTES {
                self.send_framed_text(peer, Arc::clone(&data_channel), item.text)
                    .await
            } else {
                match self.wait_for_send_capacity(peer).await {
                    Ok(()) => data_channel
                        .send_text(&item.text)
                        .await
                        .map_err(|e| webrtc_error("send data channel frame", e)),
                    Err(error) => Err(error),
                }
            };
            let _ = item.result.send(result);
            if !self.peers.lock().contains_key(peer) {
                reset_guard.armed = false;
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
        self.record_status(|status| {
            status.active_transfers = status.active_transfers.saturating_add(1);
        });

        let mut transfer_attempt = 0usize;
        let start = transport_start_frame(&transfer_id, transfer_attempt, chunks.len(), text.len());
        if let Err(error) = send_json_text(&data_channel, &start).await {
            self.record_status(|status| {
                status.active_transfers = status.active_transfers.saturating_sub(1);
            });
            return Err(error);
        }
        self.record_sent_transport_frame(&start);

        for window_start in (0..chunks.len()).step_by(FRAME_ACK_WINDOW) {
            let window_end = usize::min(window_start + FRAME_ACK_WINDOW, chunks.len()) - 1;
            let ack_key = transfer_ack_key(&transfer_id, window_end);
            let mut attempt = transfer_attempt;
            let mut restart_from_zero = false;
            loop {
                if restart_from_zero {
                    let restart =
                        transport_start_frame(&transfer_id, attempt, chunks.len(), text.len());
                    if let Err(error) = send_json_text(&data_channel, &restart).await {
                        self.record_status(|status| {
                            status.active_transfers = status.active_transfers.saturating_sub(1);
                        });
                        return Err(error);
                    }
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
                    if let Err(error) = self.wait_for_send_capacity(peer).await {
                        self.pending_frame_acks.lock().remove(&ack_key);
                        self.refresh_dynamic_transport_status();
                        self.record_status(|status| {
                            status.active_transfers = status.active_transfers.saturating_sub(1);
                        });
                        return Err(error);
                    }
                    let chunk = transport_chunk_frame(&transfer_id, attempt, seq, data);
                    if let Err(error) = send_json_text(&data_channel, &chunk).await {
                        self.pending_frame_acks.lock().remove(&ack_key);
                        self.refresh_dynamic_transport_status();
                        self.record_status(|status| {
                            status.active_transfers = status.active_transfers.saturating_sub(1);
                        });
                        return Err(error);
                    }
                    self.record_sent_transport_frame(&chunk);
                    tokio::time::sleep(SEND_FRAME_PAUSE).await;
                }

                match tokio::time::timeout(FRAME_ACK_TIMEOUT, ack_rx).await {
                    Ok(Ok(())) => break,
                    Ok(Err(_)) => {
                        self.pending_frame_acks.lock().remove(&ack_key);
                        self.refresh_dynamic_transport_status();
                        self.record_status(|status| {
                            status.active_transfers = status.active_transfers.saturating_sub(1);
                        });
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
                    Err(_) => {
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
                            self.record_status(|status| {
                                status.active_transfers = status.active_transfers.saturating_sub(1);
                            });
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
        self.record_status(|status| {
            status.active_transfers = status.active_transfers.saturating_sub(1);
        });
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
        let ack_key = transfer_ack_key(transfer_id, ack_seq);
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
            let key = ack_seq
                .map(|seq| transfer_ack_key(&transfer_id, seq))
                .unwrap_or_else(|| transfer_id.clone());
            if let Some(pending) = self.pending_frame_acks.lock().remove(&key) {
                self.record_ack_lag(pending.sent_at_ms);
                if frame
                    .get("resume")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    self.record_status(|status| {
                        status.resume_ack_count = status.resume_ack_count.saturating_add(1);
                    });
                }
                let _ = pending.sender.send(());
            } else if ack_seq_i64.is_none() {
                let keys: Vec<String> = self
                    .pending_frame_acks
                    .lock()
                    .keys()
                    .filter(|key| key.starts_with(&format!("{transfer_id}|")))
                    .cloned()
                    .collect();
                for key in keys {
                    if let Some(pending) = self.pending_frame_acks.lock().remove(&key) {
                        self.record_ack_lag(pending.sent_at_ms);
                        let _ = pending.sender.send(());
                    }
                }
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
            if total_frames == 0 || total_frames > 100_000 || total_bytes > MAX_TRANSFER_BYTES {
                return Err(new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(serde_json::json!({
                        "message": "invalid WebRTC transport frame count",
                        "transferId": transfer_id,
                        "totalFrames": total_frames,
                        "totalBytes": total_bytes,
                        "maxBytes": MAX_TRANSFER_BYTES,
                        "peer": peer,
                    })),
                ));
            }
            self.incoming_frames.lock().insert(
                transfer_id.clone(),
                IncomingFrame {
                    peer: peer.clone(),
                    attempt: frame.get("attempt").and_then(Value::as_u64).unwrap_or(0),
                    total_frames,
                    total_bytes,
                    next_ack_seq: usize::min(FRAME_ACK_WINDOW - 1, total_frames - 1),
                    received: vec![None; total_frames],
                },
            );
            self.completed_frame_acks.lock().remove(&transfer_id);
            self.refresh_dynamic_transport_status();
            return Ok(None);
        }

        if kind == "resume" {
            let completed_ack =
                self.completed_frame_acks
                    .lock()
                    .get(&transfer_id)
                    .and_then(|completed| {
                        if completed.peer != *peer {
                            return None;
                        }
                        Some((completed.ack_seq as i64, completed.received_frames, true))
                    });
            if let Some((ack_seq, received_frames, final_ack)) = completed_ack {
                send_transport_ack(
                    &data_channel,
                    &transfer_id,
                    ack_seq,
                    received_frames,
                    final_ack,
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
                incoming.get(&transfer_id).and_then(|entry| {
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

        let frame_status = {
            let mut incoming = self.incoming_frames.lock();
            let entry = incoming.get_mut(&transfer_id).ok_or_else(|| {
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
                let entry = incoming.remove(&transfer_id).expect("entry exists");
                let mut text = String::new();
                for chunk in entry.received {
                    text.push_str(&chunk.unwrap_or_default());
                }
                if entry.total_bytes != 0 && text.len() != entry.total_bytes {
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
                self.completed_frame_acks.lock().insert(
                    transfer_id.clone(),
                    CompletedFrameAck {
                        peer: peer.clone(),
                        ack_seq,
                        received_frames: ack_seq + 1,
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
                remove_peer(&self.handler, &self.remote_peer_id);
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
        install_data_channel(
            Arc::clone(&self.handler),
            self.remote_peer_id.clone(),
            data_channel,
        );
    }
}

async fn build_peer_connection(
    handler: Arc<WebRTCRsConnectionHandler>,
    signaling: Arc<SignalingClient>,
    remote_peer_id: PeerId,
) -> RxResult<Arc<dyn PeerConnection>> {
    let event_handler = Arc::new(RsPeerConnectionEvents {
        handler: Arc::clone(&handler),
        signaling,
        remote_peer_id,
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
    data_channel: Arc<dyn DataChannel>,
) {
    {
        let mut peers = handler.peers.lock();
        if let Some(entry) = peers.get_mut(&remote_peer_id) {
            entry.data_channel = Some(Arc::clone(&data_channel));
        }
    }

    let message_subject = handler.message_subject.clone();
    let response_subject = handler.response_subject.clone();
    let connect_subject = handler.connect_subject.clone();
    let disconnect_subject = handler.disconnect_subject.clone();
    let error_subject = handler.error_subject.clone();
    let handler_for_task = Arc::clone(&handler);
    let data_channel_for_task = Arc::clone(&data_channel);
    let peer_for_task = remote_peer_id.clone();
    // Phase 1: register the per-peer backpressure signal and arm the SCTP
    // buffered-amount thresholds so the channel emits OnBufferedAmountHigh/Low
    // and senders can pace instead of overrunning the buffer.
    let backpressure_for_task = handler.peer_backpressure(&remote_peer_id);
    let task = tokio::spawn(async move {
        let _ = data_channel
            .set_buffered_amount_low_threshold(DATA_CHANNEL_BUFFERED_LOW_WATER)
            .await;
        let _ = data_channel
            .set_buffered_amount_high_threshold(DATA_CHANNEL_BUFFERED_HIGH_WATER)
            .await;
        while let Some(event) = data_channel.poll().await {
            match event {
                DataChannelEvent::OnOpen => {
                    if let Some(entry) = handler_for_task.peers.lock().get_mut(&peer_for_task) {
                        entry.data_channel_open = true;
                    }
                    // Presence join snapshot: broadcasts fire on CHANGE, peer
                    // close and TTL sweep — a peer that connects while other
                    // peers already publish presence would otherwise see
                    // nothing until the next change (found by the two-browser
                    // E2E mode). Push the current aggregate to the newly
                    // opened peer; a no-presence room sends nothing.
                    if !handler_for_task.presence.lock().is_empty() {
                        let handler_presence = Arc::clone(&handler_for_task);
                        let peer_presence = peer_for_task.clone();
                        tokio::spawn(async move {
                            handler_presence
                                .push_presence_snapshot_to(&peer_presence)
                                .await;
                        });
                    }
                    connect_subject.next(peer_for_task.clone());
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
                    if value.get("result").is_some() || value.get("error").is_some() {
                        match serde_json::from_value::<WebRTCResponse>(value) {
                            Ok(response) => response_subject.next(PeerWithResponse {
                                peer: peer_for_task.clone(),
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
                                        let peer_resync = peer_for_task.clone();
                                        tokio::spawn(async move {
                                            for collection in newly_activated {
                                                let resp = WebRTCResponse {
                                                    id: crate::plugins::replication_webrtc::index_mod::master_change_stream_id(&collection),
                                                    result: serde_json::json!({ "resync": true }),
                                                    error: None,
                                                    collection: Some(collection),
                                                };
                                                let _ = handler_resync
                                                    .send(
                                                        &peer_resync,
                                                        WebRTCWireFrame::Response(resp),
                                                    )
                                                    .await;
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
                                        peer: peer_for_task.clone(),
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
                    backpressure_for_task.clear_high();
                    // Mark the entry's channel as no longer open so a renewed
                    // browser offer rebuilds the responder
                    // (`remove_unopened_peer_before_offer` keys on this) —
                    // otherwise a re-offer after a channel close hit the
                    // stale pc and could never converge.
                    if let Some(entry) = handler_for_task.peers.lock().get_mut(&peer_for_task) {
                        entry.data_channel_open = false;
                    }
                    // A closed tab's presence hints must not linger on the
                    // other peers until the TTL sweep — drop them now and
                    // push the reduced aggregate.
                    if handler_for_task.remove_peer_presence(&peer_for_task) {
                        let handler_presence = Arc::clone(&handler_for_task);
                        tokio::spawn(async move {
                            handler_presence.broadcast_presence().await;
                        });
                    }
                    disconnect_subject.next(peer_for_task.clone());
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
        // Channel ended: release any sender parked on backpressure and drop the
        // per-peer signal so it cannot leak across reconnects.
        backpressure_for_task.clear_high();
        handler_for_task.backpressure.lock().remove(&peer_for_task);
    });

    if let Some(entry) = handler.peers.lock().get_mut(&remote_peer_id) {
        entry.tasks.push(task);
    }
}

fn remove_peer(handler: &WebRTCRsConnectionHandler, peer: &str) {
    remove_peer_inner(handler, peer, None);
}

fn remove_peer_with_error(handler: &WebRTCRsConnectionHandler, peer: &str, error: RxError) {
    remove_peer_inner(handler, peer, Some(error));
}

fn remove_peer_inner(handler: &WebRTCRsConnectionHandler, peer: &str, error: Option<RxError>) {
    // Clear every per-peer registry even when the peer entry already vanished
    // in a concurrent close path. This makes teardown idempotent and ensures
    // no queue/ack/capacity waiter survives a terminal send-buffer stall.
    handler.active_collections.lock().remove(peer);
    if handler
        .presence
        .lock()
        .remove(peer)
        .is_some_and(|report| !report.entries.is_empty())
    {
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

    if let Some(mut entry) = handler.peers.lock().remove(peer) {
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
        handler.disconnect_subject.next(peer_id);
    }
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

fn transfer_ack_key(transfer_id: &str, ack_seq: usize) -> String {
    format!("{transfer_id}|{ack_seq}")
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

fn split_utf8_chunks(text: &str, max_bytes: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = usize::min(start + max_bytes, text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            end = text[start..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| start + offset)
                .unwrap_or(text.len());
        }
        chunks.push(text[start..end].to_string());
        start = end;
    }
    chunks
}

#[cfg(test)]
mod tests {
    // `WebRTCMessage` / `WebRTCResponse` are now imported at module scope
    // (used by `apply_active_collections` + the send path) and reach the tests
    // through this glob.
    use super::*;

    /// #12c: per-collection authz is fail-open until a hook is installed, then
    /// enforces using the peer's captured capability token.
    #[test]
    fn collection_authz_is_fail_open_then_enforces_with_token() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-1".to_string();
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
        let other = "peer-2".to_string();
        assert!(!handler.is_collection_authorized_for_peer(&other, "anything"));
        // Removing enforcement returns to fail-open.
        handler.set_collection_authz(None);
        assert!(handler.is_collection_authorized_for_peer(&other, "anything"));
    }

    #[test]
    fn write_and_document_authz_hooks_are_fail_open_then_enforced() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-1".to_string();
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
            Arc::new(move |document: &Value| {
                authorized && document.get("user_id").and_then(Value::as_str) == Some("alice")
            })
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
    }

    /// REGRESSION (52a1bf45): when the task draining a peer's send queue is
    /// aborted mid-send, the guard's Drop must re-open the drain slot.
    #[test]
    fn drain_reset_guard_reopens_slot_on_drop() {
        let queues: Arc<Mutex<HashMap<WebRTCRsPeer, PeerSendQueue>>> =
            Arc::new(Mutex::new(HashMap::new()));
        queues.lock().entry("p1".to_string()).or_default().draining = true;
        drop(DrainResetGuard {
            queues: Arc::clone(&queues),
            peer: "p1".to_string(),
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
            armed: false,
        });
        assert!(
            queues.lock().get("p1").unwrap().draining,
            "disarmed guard must leave the flag alone"
        );
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
        let chunks = split_utf8_chunks("aaäbb🙂cc", 4);

        assert_eq!(chunks.concat(), "aaäbb🙂cc");
        assert!(chunks.iter().all(|chunk| chunk.len() <= 4));
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
            transfer_ack_key("t1", 0),
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

    #[test]
    fn active_collection_predicate_tracks_control_plane_state() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-1".to_string();

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
        let newly_activated = handler.apply_active_collections(&peer, &msg);
        assert!(newly_activated.is_empty());

        assert!(handler.is_collection_active_for_peer(&peer, "documents"));
        assert!(handler.is_collection_active_for_peer(&peer, "business_commands"));
        assert!(!handler.is_collection_active_for_peer(&peer, "ctox_ticket_self_work_notes"));
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
    #[test]
    fn backpressure_gates_send_capacity_and_releases_on_low() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-1".to_string();

        // No backpressure registered yet → nothing buffered, capacity free.
        assert_eq!(handler.buffered_bytes(&peer), 0);

        let bp = handler.peer_backpressure(&peer);
        bp.set_high();
        // While buffered we report above the high watermark so the demand
        // dispatchers' `buffered_bytes > high_water` guards engage.
        assert!(handler.buffered_bytes(&peer) > DATA_CHANNEL_BUFFERED_HIGH_WATER as usize);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let bp_for_clear = Arc::clone(&bp);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                bp_for_clear.clear_high();
            });
            // Must block while high, then return well under the 30s wait cap
            // once the low event clears it.
            tokio::time::timeout(
                Duration::from_secs(2),
                handler.wait_for_send_capacity(&peer),
            )
            .await
            .expect("wait_for_send_capacity did not release after OnBufferedAmountLow")
            .expect("capacity wait should succeed after OnBufferedAmountLow");
        });

        assert_eq!(handler.buffered_bytes(&peer), 0);
    }

    #[test]
    fn wait_for_send_capacity_returns_immediately_without_backpressure() {
        let handler = WebRTCRsConnectionHandler::new();
        let peer = "peer-2".to_string();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            tokio::time::timeout(
                Duration::from_millis(100),
                handler.wait_for_send_capacity(&peer),
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
            transfer_ack_key("peer-stalled|frame|1", 0),
            PendingFrameAck {
                sender: ack_tx,
                sent_at_ms: now_ms(),
            },
        );
        handler.incoming_frames.lock().insert(
            "incoming-1".to_string(),
            IncomingFrame {
                peer: peer.clone(),
                attempt: 0,
                total_frames: 1,
                total_bytes: 1,
                next_ack_seq: 0,
                received: vec![None],
            },
        );
        handler.completed_frame_acks.lock().insert(
            "completed-1".to_string(),
            CompletedFrameAck {
                peer: peer.clone(),
                ack_seq: 0,
                received_frames: 1,
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

        let wait = handler.wait_for_send_capacity(&peer);
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
}
