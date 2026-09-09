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

/// Internal routing marker for Browser live responses. It is never a database
/// collection; the native send scheduler uses it to place interactive JPEG and
/// input acknowledgements ahead of ordinary replication responses.
pub const CTOX_BROWSER_LIVE_RESPONSE_COLLECTION: &str = "__ctox_browser_live__";
/// Internal scheduler marker for pointer/keyboard acknowledgements. Input must
/// be allowed to overtake a queued JPEG response and the one sync response the
/// live-frame fairness rule normally inserts; otherwise a keystroke inherits
/// multi-second collection-sync latency.
pub const CTOX_BROWSER_INPUT_RESPONSE_COLLECTION: &str = "__ctox_browser_input__";

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
    /// Equality identifies one connection lifetime, not a reusable signaling route.
    type Peer: Clone + Eq + std::hash::Hash + std::fmt::Debug + Send + Sync + 'static;

    fn connect_stream(&self) -> RxStream<Self::Peer>;
    fn disconnect_stream(&self) -> RxStream<Self::Peer>;
    fn message_stream(&self) -> RxStream<PeerWithMessage<Self::Peer>>;
    fn response_stream(&self) -> RxStream<PeerWithResponse<Self::Peer>>;
    fn error_stream(&self) -> RxStream<RxError>;

    /// Fixed by the local host before advertising the peer. Authentication and
    /// collection authorization are independent of this declared runtime role.
    fn local_peer_role(&self) -> super::NativePeerRole {
        super::NativePeerRole::CtoxInstance
    }

    /// Fresh host credentials for this connection and optional remote challenge.
    /// The host must authorize disclosure to this peer; room membership alone
    /// is not authentication. No provider means the existing anonymous envelope.
    async fn local_session_credentials(
        &self,
        _peer: &Self::Peer,
        _nonce: Option<String>,
    ) -> Result<Option<super::local_session::LocalSessionCredentials>, RxError> {
        Ok(None)
    }

    async fn send(&self, peer: &Self::Peer, frame: WebRTCWireFrame) -> Result<(), RxError>;

    async fn send_auxiliary(
        &self,
        peer: &Self::Peer,
        _label: &str,
        frame: WebRTCWireFrame,
    ) -> Result<(), RxError> {
        self.send(peer, frame).await
    }

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

    /// Cancellation/transfer key for this connection, separate from policy identity.
    fn connection_identity(&self, peer: &Self::Peer) -> String {
        self.peer_identity(peer)
    }

    /// Whether a captured handle still denotes a live connection. Transport
    /// adapters with reusable routes must reject retired generations here.
    fn is_peer_current(&self, _peer: &Self::Peer) -> bool {
        true
    }

    /// Whether a collection is currently foreground/active for this peer.
    /// Generic handlers default to true to preserve the upstream-style
    /// broadcast behavior; the CTOX WebRTC handler overrides this from the
    /// `rxdb.activeCollections` control plane.
    fn is_collection_active_for_peer(&self, _peer: &Self::Peer, _collection: &str) -> bool {
        true
    }

    /// Whether a small, server-authorized live stream may continue while the
    /// browser has not marked the collection foreground. Generic handlers do
    /// not opt into background live changes.
    fn is_inactive_live_change_authorized_for_peer(
        &self,
        _peer: &Self::Peer,
        _collection: &str,
    ) -> bool {
        false
    }

    /// #12c: record the capability token a peer presented in its handshake
    /// `peerSession`. Generic handlers no-op; the CTOX handler stores it for the
    /// per-collection authz gate below.
    fn set_peer_capability_token(&self, _peer: &Self::Peer, _token: String) {}

    /// Capability token captured during the authenticated room handshake.
    /// Auxiliary request handlers use the same identity boundary as collection
    /// replication instead of accepting actor data supplied in request params.
    fn peer_capability_token(&self, _peer: &Self::Peer) -> Option<String> {
        None
    }

    /// #12c: whether `peer` may replicate `collection`. Generic handlers default
    /// to true (no enforcement); the CTOX handler consults the role bound to the
    /// peer's captured capability token when authz is enabled.
    fn is_collection_authorized_for_peer(&self, _peer: &Self::Peer, _collection: &str) -> bool {
        true
    }

    /// Optional server-authoritative gate for ordinary full-collection pulls.
    /// Demand-query methods use their own bounded dispatcher and are not
    /// affected. Generic handlers remain fail-open.
    fn is_eager_collection_pull_authorized_for_peer(
        &self,
        _peer: &Self::Peer,
        _collection: &str,
    ) -> bool {
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

    /// Optional document-level push gate. Generic handlers preserve upstream
    /// behavior; CTOX binds browser documents to the authenticated peer.
    fn are_documents_write_authorized_for_peer(
        &self,
        _peer: &Self::Peer,
        _collection: &str,
        _params: &[serde_json::Value],
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

    /// Explicit field allowlist. Query selectors/sorts must not observe hidden
    /// fields, and outgoing documents must be projected before serialization.
    fn document_fields_for_peer(&self, peer: &Self::Peer, collection: &str) -> Option<Vec<String>>;

    /// Optional predicate used by `masterChangesSince` responses.
    fn document_filter_for_peer(
        &self,
        _peer: &Self::Peer,
        _collection: &str,
    ) -> Option<WebRTCDocumentFilter> {
        None
    }
}

/// Signaling-peer admission predicate shared by WebRTC replication options.
pub type WebRTCPeerValidator<P> = Arc<dyn Fn(&P) -> bool + Send + Sync>;

/// Result of validating a full `ctoxProtocol.peerSession` envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebRTCPeerSessionValidation {
    /// The session and any device proof are valid; its capability may be
    /// captured for collection authorization.
    Accept,
    /// The session is not revoked, but a bound capability needs the native
    /// peer's fresh challenge. Answer the passive probe without capturing it;
    /// the native outbound challenge round will decide.
    Defer,
    /// The session, capability, or proof is invalid. Close the peer.
    Reject,
}

/// Server-authoritative admission predicate for the full ctoxProtocol payload.
/// `expected_nonce` is present only for the native-initiated challenge round.
pub type WebRTCPeerSessionValidator =
    Arc<dyn Fn(&Value, Option<&str>) -> WebRTCPeerSessionValidation + Send + Sync>;

/// Per-peer document visibility predicate shared by replication and query fetch.
pub type WebRTCDocumentFilter = Arc<dyn Fn(&Value) -> bool + Send + Sync>;

pub(crate) fn retain_readable_fields(document: &mut Value, fields: &[String]) {
    if let Some(object) = document.as_object_mut() {
        object.retain(|key, _| {
            fields.contains(key)
                || matches!(key.as_str(), "_rev" | "_meta" | "_deleted" | "_attachments")
        });
    }
}

/// masterChangesSince wraps documents; masterWrite returns bare conflict rows.
/// Apply the same authenticated field policy to both serialization shapes.
pub(crate) fn mask_master_response(response: &mut Value, fields: &[String]) {
    let documents = if response.is_array() {
        response.as_array_mut()
    } else {
        response.get_mut("documents").and_then(Value::as_array_mut)
    };
    if let Some(documents) = documents {
        for document in documents {
            retain_readable_fields(document, fields);
        }
    }
}

pub(crate) fn readable_query_fields(query: &Value, fields: &[String]) -> bool {
    fn selector(value: &Value, fields: &[String]) -> bool {
        match value {
            Value::Object(object) => object.iter().all(|(key, value)| {
                if key.starts_with('$') {
                    matches!(key.as_str(), "$and" | "$or" | "$nor" | "$not")
                        && selector(value, fields)
                } else {
                    fields
                        .iter()
                        .any(|field| key == field || key.starts_with(&format!("{field}.")))
                        || matches!(key.as_str(), "_deleted" | "_meta.lwt" | "_rev")
                }
            }),
            Value::Array(values) => values.iter().all(|value| selector(value, fields)),
            Value::String(field) => fields.contains(field),
            _ => false,
        }
    }
    query
        .get("selector")
        .is_none_or(|value| selector(value, fields))
        && query
            .get("sort")
            .is_none_or(|value| selector(value, fields))
        && query.get("index").is_none_or(|value| match value {
            Value::String(field) => fields.contains(field),
            Value::Array(values) => values.iter().all(|v| {
                v.as_str()
                    .is_some_and(|field| fields.iter().any(|f| f == field))
            }),
            _ => false,
        })
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

#[cfg(test)]
mod field_policy_tests {
    use super::*;
    #[test]
    fn crew_master_write_conflicts_and_master_changes_share_field_mask() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/crew-identity.json"))
                .unwrap();
        let fields: Vec<String> = serde_json::from_value(fixture["public_fields"].clone()).unwrap();
        let member = fixture["member"].clone();
        let mut conflicts = serde_json::json!([member]);
        let mut changes = serde_json::json!({"documents":[member],"checkpoint":{"id":"kept"}});
        mask_master_response(&mut conflicts, &fields);
        mask_master_response(&mut changes, &fields);
        assert_eq!(conflicts, changes["documents"]);
        assert_eq!(changes["checkpoint"]["id"], "kept");
        assert_eq!(conflicts[0]["name"], "Milo");
        for field in ["soul", "specialties", "stats"] {
            assert!(conflicts[0].get(field).is_none());
        }
    }
    #[test]
    fn crew_fixture_masks_private_fields_and_rejects_query_oracles() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/crew-identity.json"))
                .unwrap();
        let fields: Vec<String> = serde_json::from_value(fixture["public_fields"].clone()).unwrap();
        let mut member = fixture["member"].clone();
        retain_readable_fields(&mut member, &fields);
        for private in ["soul", "stats", "specialties"] {
            assert!(member.get(private).is_none());
        }
        assert_eq!(member["name"], "Milo");
        assert!(readable_query_fields(
            &serde_json::json!({"selector":{"archived":false},"sort":[{"name":"asc"}]}),
            &fields
        ));
        for query in [
            serde_json::json!({"selector":{"soul.sketch":{"$regex":"secret"}}}),
            serde_json::json!({"selector":{"$or":[{"name":"Milo"},{"stats.tasks_total":1}]}}),
            serde_json::json!({"sort":["soul.sketch"]}),
            serde_json::json!({"index":["stats.tasks_total"]}),
            serde_json::json!({"selector":{"$expr":{"$eq":["$soul.sketch","secret"]}}}),
        ] {
            assert!(!readable_query_fields(&query, &fields), "{query}");
        }
    }
}
