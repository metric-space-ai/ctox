//! Port of `src/plugins/replication-webrtc/webrtc-types.ts`.
//!
//! T1 deviations:
//! - `RxReplicationState` / `ReplicationOptions` / `ReplicationPullOptions` /
//!   `ReplicationPushOptions` come from `plugins/replication/index.ts` (T1,
//!   unported). We model the WebRTC types without those for now; the
//!   `SyncOptionsWebRTC`-equivalent gets re-added when the user-facing
//!   replication wrapper lands.
//! - Upstream `send(peer, message: WebRTCMessage | WebRTCResponse)` is split
//!   into [`WebRTCWireFrame`] (one enum, two variants) so that
//!   `WebRTCConnectionHandler::send` has a single nominal type.
//! - `Subscription[]` cleanup arrays are not modelled — Rust uses `JoinHandle`s
//!   that callers store and `.abort()` on cleanup.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::rx_error::RxError;
use crate::rxjs_compat::RxStream;
use crate::types::RxStorageDefaultCheckpoint;

// ref: rxdb/src/plugins/replication-webrtc/webrtc-types.ts:16
pub type WebRTCReplicationCheckpoint = RxStorageDefaultCheckpoint;

// ref: rxdb/src/plugins/replication-webrtc/webrtc-types.ts:19-21
//
// Phase 3 (single multiplexed stream): every plain replication frame now
// carries an optional `collection` so that one DataChannel can carry every
// collection at once. The browser tags `masterChangesSince` / `masterWrite`
// with the source collection; the native demux loop routes the frame to that
// collection's master handler / fork state. The field is `#[serde(default)]`
// + skipped-when-`None` so V1 peers (and handshake / demand-fetch frames that
// already self-describe their collection in `params`) stay wire-compatible.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WebRTCMessage {
    pub id: String,
    /// One of the `RxReplicationHandler` method names (`masterChangesSince`,
    /// `masterWrite`) or the special `"token"` handshake.
    pub method: String,
    #[serde(default)]
    pub params: Vec<Value>,
    /// Phase 3 multiplex routing key — the collection this frame belongs to.
    /// `None` for handshake / control frames that are not collection-scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
}

// ref: rxdb/src/plugins/replication-webrtc/webrtc-types.ts:22
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WebRTCResponse {
    pub id: String,
    #[serde(default)]
    pub result: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Phase 3 multiplex routing key — set on the `masterChangeStream$`
    /// server-push response so the fork side knows which collection's pull
    /// stream to feed. `None` for request/answer responses (matched by `id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
}

/// Single nominal "wire frame" for messages going out over a peer — either a
/// new request ([`WebRTCMessage`]) or an answer to a prior request
/// ([`WebRTCResponse`]).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum WebRTCWireFrame {
    Message(WebRTCMessage),
    Response(WebRTCResponse),
}

impl From<WebRTCMessage> for WebRTCWireFrame {
    fn from(m: WebRTCMessage) -> Self {
        WebRTCWireFrame::Message(m)
    }
}
impl From<WebRTCResponse> for WebRTCWireFrame {
    fn from(r: WebRTCResponse) -> Self {
        WebRTCWireFrame::Response(r)
    }
}

// ref: rxdb/src/plugins/replication-webrtc/webrtc-types.ts:23-26
#[derive(Debug, Clone)]
pub struct PeerWithMessage<P: Clone> {
    pub peer: P,
    pub message: WebRTCMessage,
}

// ref: rxdb/src/plugins/replication-webrtc/webrtc-types.ts:27-30
#[derive(Debug, Clone)]
pub struct PeerWithResponse<P: Clone> {
    pub peer: P,
    pub response: WebRTCResponse,
}

// ref: rxdb/src/plugins/replication-webrtc/webrtc-types.ts:32-40
/// A connection-handler abstracts the actual transport (simple-peer / WebRTC,
/// p2pcf, webtorrent in upstream; webrtc-rs in CTOX). Implementations expose
/// connect/disconnect/message/response streams and a send method.
#[async_trait]
pub trait WebRTCConnectionHandler: Send + Sync {
    type Peer: Clone + Eq + std::hash::Hash + std::fmt::Debug + Send + Sync + 'static;

    fn connect_stream(&self) -> RxStream<Self::Peer>;
    fn disconnect_stream(&self) -> RxStream<Self::Peer>;
    fn message_stream(&self) -> RxStream<PeerWithMessage<Self::Peer>>;
    fn response_stream(&self) -> RxStream<PeerWithResponse<Self::Peer>>;
    fn error_stream(&self) -> RxStream<RxError>;

    async fn send(&self, peer: &Self::Peer, frame: WebRTCWireFrame) -> Result<(), RxError>;

    async fn close(&self) -> Result<(), RxError>;

    /// Force-close ONE peer's transport (peer connection + data channel) so
    /// both sides observe a disconnect and rebuild cleanly. Used when the
    /// replication layer must abandon a peer whose transport is up but whose
    /// handshake failed — leaving the transport open used to park the peer in
    /// a half-dead state (channel open, no replication) until an unrelated
    /// network event tore it down. Default is a no-op for handlers that do
    /// not model per-peer transport.
    async fn close_peer(&self, _peer: &Self::Peer) {}

    /// V1.5 server-push backpressure hook. Returns the number of bytes
    /// currently buffered for the given peer (analogous to WebRTC's
    /// `RTCDataChannel.bufferedAmount`). Implementations that do not yet
    /// support backpressure may return 0; the dispatcher then falls back
    /// to a small inter-chunk yield.
    fn buffered_bytes(&self, _peer: &Self::Peer) -> usize {
        0
    }

    /// V1.5 stable peer identity for authz + rate-limiting. Default impl
    /// uses Debug formatting (works for any Peer type but is opaque).
    /// Production handlers should override with the actual peer-id string.
    fn peer_identity(&self, peer: &Self::Peer) -> String {
        format!("{:?}", peer)
    }

    /// Whether a collection is currently foreground/active for this peer.
    /// Generic handlers default to true to preserve the upstream-style
    /// broadcast behavior; the CTOX WebRTC handler overrides this from the
    /// `rxdb.activeCollections` control plane.
    fn is_collection_active_for_peer(&self, _peer: &Self::Peer, _collection: &str) -> bool {
        true
    }

    /// #12c: record the capability token a peer presented in its handshake
    /// `peerSession`. Generic handlers no-op; the CTOX handler stores it for the
    /// per-collection authz gate below.
    fn set_peer_capability_token(&self, _peer: &Self::Peer, _token: String) {}

    /// #12c: whether `peer` may replicate `collection`. Generic handlers default
    /// to true (no enforcement); the CTOX handler consults the role bound to the
    /// peer's captured capability token when authz is enabled.
    fn is_collection_authorized_for_peer(&self, _peer: &Self::Peer, _collection: &str) -> bool {
        true
    }

    /// Optional write gate for native-owned collections. Generic handlers keep
    /// the upstream behavior and allow writes.
    fn is_collection_write_authorized_for_peer(
        &self,
        _peer: &Self::Peer,
        _collection: &str,
    ) -> bool {
        true
    }

    /// Optional per-peer document filter for master responses and live changes.
    /// Returning `None` drops the whole change event for that peer.
    fn filter_master_change_for_peer(
        &self,
        _peer: &Self::Peer,
        _collection: &str,
        change: crate::types::RxReplicationMasterChange,
    ) -> Option<crate::types::RxReplicationMasterChange> {
        Some(change)
    }

    /// Optional predicate used by `masterChangesSince` responses.
    fn document_filter_for_peer(
        &self,
        _peer: &Self::Peer,
        _collection: &str,
    ) -> Option<Arc<dyn Fn(&Value) -> bool + Send + Sync>> {
        None
    }
}

/// Soft threshold above which the V1.5 dispatcher yields and waits before
/// sending the next chunk. Matches typical WebRTC SCTP send-queue depth.
pub const WEBRTC_BUFFERED_HIGH_WATER: usize = 1024 * 1024; // 1 MiB

// ref: rxdb/src/plugins/replication-webrtc/webrtc-types.ts:42-44
/// Factory type for connection handlers. Upstream is generic over a
/// `SyncOptionsWebRTC` arg; we leave the argument shape to the concrete
/// handler since the full options type depends on phase-6.
pub type WebRTCConnectionHandlerCreator<H> = Arc<
    dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Arc<H>, RxError>> + Send>>
        + Send
        + Sync,
>;

// `SyncOptionsWebRTC`, `RxWebRTCReplicationState`, `WebRTCPeerState` depend on
// `RxReplicationState` from `plugins/replication/index.ts` and on `RxCollection`
// from phase-6. They land when those are available.
