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
}

impl PeerSendQueue {
    fn push(&mut self, item: QueuedSend) {
        match item.priority {
            SendPriority::High => self.high.push_back(item),
            SendPriority::Normal => self.normal.push_back(item),
            SendPriority::Low => self.low.push_back(item),
        }
    }

    fn pop_next(&mut self) -> Option<QueuedSend> {
        self.high
            .pop_front()
            .or_else(|| self.normal.pop_front())
            .or_else(|| self.low.pop_front())
    }

    /// Phase 2: re-bucket every still-queued frame against a new
    /// active-collection set. Frames whose collection just became active jump
    /// from Normal → High; frames whose collection left the active set drop
    /// High → Normal. FIFO order WITHIN a bucket is preserved by re-pushing in
    /// the original High→Normal→Low drain order. Control frames (intrinsic
    /// High) and oversized background writes (Low) are unaffected.
    fn reprioritize(&mut self, active: &HashSet<String>) {
        let mut items: Vec<QueuedSend> = Vec::with_capacity(
            self.high.len() + self.normal.len() + self.low.len(),
        );
        items.extend(self.high.drain(..));
        items.extend(self.normal.drain(..));
        items.extend(self.low.drain(..));
        for mut item in items.into_iter() {
            item.priority = item.classify_against(active);
            self.push(item);
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
    pub queued_frames: u64,
    pub sent_scheduled_frames: u64,
    pub priority_queue_depth: usize,
    pub high_priority_queue_depth: usize,
    pub normal_priority_queue_depth: usize,
    pub low_priority_queue_depth: usize,
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
            queued_frames: 0,
            sent_scheduled_frames: 0,
            priority_queue_depth: 0,
            high_priority_queue_depth: 0,
            normal_priority_queue_depth: 0,
            low_priority_queue_depth: 0,
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
    transport_status: Arc<Mutex<WebRtcFrameTransportStatus>>,
    frame_counter: AtomicU64,
    /// Phase 1: per-peer send-buffer backpressure (see `PeerBackpressure`).
    backpressure: Arc<Mutex<HashMap<WebRTCRsPeer, Arc<PeerBackpressure>>>>,
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
            transport_status: Arc::new(Mutex::new(WebRtcFrameTransportStatus::default())),
            frame_counter: AtomicU64::new(0),
            backpressure: Arc::new(Mutex::new(HashMap::new())),
            tasks: Mutex::new(Vec::new()),
        }
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
    async fn wait_for_send_capacity(&self, peer: &WebRTCRsPeer) {
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
                // Timed out waiting for drain; stop blocking and let the
                // normal ack/resume machinery handle a genuinely stuck peer.
                break;
            }
        }
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
                    let is_initiator = own_peer_id
                        .as_ref()
                        .map(|own| remote_peer_id.as_str() > own.as_str())
                        .unwrap_or(false);
                    if let Err(err) = handler
                        .ensure_peer_connection(remote_peer_id, is_initiator)
                        .await
                    {
                        handler.error_subject.next(err);
                    }
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
        status.pending_acks = self.pending_frame_acks.lock().len();
        status.incoming_transfers = self.incoming_frames.lock().len();
        status.completed_ack_cache_size = self.completed_frame_acks.lock().len();
        let mut high = 0usize;
        let mut normal = 0usize;
        let mut low = 0usize;
        for queue in self.send_queues.lock().values() {
            high += queue.high.len();
            normal += queue.normal.len();
            low += queue.low.len();
        }
        status.priority_queue_depth = high + normal + low;
        status.high_priority_queue_depth = high;
        status.normal_priority_queue_depth = normal;
        status.low_priority_queue_depth = low;
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
            "queuedFrames": status.queued_frames,
            "sentScheduledFrames": status.sent_scheduled_frames,
            "priorityQueueDepth": status.priority_queue_depth,
            "highPriorityQueueDepth": status.high_priority_queue_depth,
            "normalPriorityQueueDepth": status.normal_priority_queue_depth,
            "lowPriorityQueueDepth": status.low_priority_queue_depth,
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
            let is_offer = data.get("type").and_then(Value::as_str) == Some("offer");
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
}

impl WebRTCRsConnectionHandler {
    /// Phase 2: apply an inbound `rxdb.activeCollections` control frame. Parses
    /// the collection-name array from `params[0]`, replaces the peer's active
    /// set, and re-buckets anything still queued for that peer so foreground
    /// frames jump ahead immediately. Idempotent: an unchanged set is a no-op.
    fn apply_active_collections(&self, peer: &WebRTCRsPeer, message: &WebRTCMessage) {
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
        {
            let mut active_map = self.active_collections.lock();
            let entry = active_map.entry(peer.clone()).or_default();
            if *entry == names {
                return;
            }
            *entry = names.clone();
        }
        // Re-bucket the existing queue against the new active set so a frame
        // already waiting for a now-foreground collection is promoted.
        if let Some(queue) = self.send_queues.lock().get_mut(peer) {
            queue.reprioritize(&names);
        }
        self.refresh_send_queue_status();
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
        let should_drain = {
            let mut queues = self.send_queues.lock();
            let queue = queues.entry(peer.clone()).or_default();
            let should_drain = !queue.draining;
            queue.push(QueuedSend {
                text: class.text,
                priority,
                collection: class.collection,
                intrinsic_high: class.intrinsic_high,
                oversized_write: class.oversized_write,
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
        loop {
            let item = {
                let mut queues = self.send_queues.lock();
                let Some(queue) = queues.get_mut(peer) else {
                    return;
                };
                match queue.pop_next() {
                    Some(item) => item,
                    None => {
                        queue.draining = false;
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
                data_channel
                    .send_text(&item.text)
                    .await
                    .map_err(|e| webrtc_error("send data channel frame", e))
            };
            let _ = item.result.send(result);
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

        let start = transport_start_frame(&transfer_id, 0, chunks.len(), text.len());
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
            let mut attempt = 0usize;
            loop {
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
                    .skip(window_start)
                {
                    // Phase 1: pace on the SCTP send buffer so a large transfer
                    // never bursts past what the channel can deliver in real
                    // time (which would overrun the buffer and get the channel
                    // killed by the browser).
                    self.wait_for_send_capacity(peer).await;
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
        for queue in self.send_queues.lock().values() {
            high += queue.high.len();
            normal += queue.normal.len();
            low += queue.low.len();
        }
        self.record_status(|status| {
            status.priority_queue_depth = high + normal + low;
            status.high_priority_queue_depth = high;
            status.normal_priority_queue_depth = normal;
            status.low_priority_queue_depth = low;
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
        .with_udp_addrs(vec![handler.udp_bind_addr.clone()])
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
                DataChannelEvent::OnOpen => connect_subject.next(peer_for_task.clone()),
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
                                    handler_for_task
                                        .apply_active_collections(&peer_for_task, &message);
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
        handler_for_task
            .backpressure
            .lock()
            .remove(&peer_for_task);
        remove_peer(&handler_for_task, &peer_for_task);
    });

    if let Some(entry) = handler.peers.lock().get_mut(&remote_peer_id) {
        entry.tasks.push(task);
    }
}

fn remove_peer(handler: &Arc<WebRTCRsConnectionHandler>, peer: &str) {
    if let Some(mut entry) = handler.peers.lock().remove(peer) {
        for task in entry.tasks.drain(..) {
            task.abort();
        }
        // Phase 2: drop the per-peer active-collection set so it cannot leak
        // across reconnects (a new connection re-reports its active set).
        handler.active_collections.lock().remove(peer);
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
            "\u{1}".repeat(200_000),                 // all C0 controls -> 6x expansion
            "\"\\".repeat(150_000),                  // all quotes+backslashes -> 2x
            "aäb🙂c\u{7}\"".repeat(40_000),          // mixed multibyte + escapes
            "x".repeat(500_000),                     // plain ASCII (no expansion)
            String::new(),                            // empty
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
        for ch in ['a', '"', '\\', '\n', '\t', '\u{08}', '\u{0c}', '\u{1}', 'ä', '🙂'] {
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
    fn default_handler_binds_udp_on_all_ipv4_interfaces() {
        let handler = WebRTCRsConnectionHandler::new();
        assert_eq!(handler.udp_bind_addr, DEFAULT_UDP_BIND_ADDR);
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
            result: high_tx,
        });
        queue.push(QueuedSend {
            text: "{}".to_string(),
            priority: SendPriority::Low,
            collection: None,
            intrinsic_high: false,
            oversized_write: true,
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
            .expect("wait_for_send_capacity did not release after OnBufferedAmountLow");
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
            .expect("wait_for_send_capacity blocked despite no backpressure");
        });
    }
}
