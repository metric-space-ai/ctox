//! Wire-format types for the simple-peer signaling protocol.
//!
//! Derived from `src/plugins/replication-webrtc/signaling-server.ts` —
//! the upstream RxDB signaling server. CTOX is always a **client** that
//! connects to such a server (gap-item N6 — the matching client is
//! [`crate::plugins::replication_webrtc::signaling_client`]).
//!
//! Message kinds:
//! - Server → client: `init { yourPeerId }`, `joined { otherPeerIds }`,
//!   relayed `signal { room, senderPeerId, receiverPeerId, data }`.
//! - Client → server: `join { room }`, `signal { senderPeerId,
//!   receiverPeerId, room, data }`, `ping`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ref: rxdb/src/plugins/replication-webrtc/signaling-server.ts:15
pub const PEER_ID_LENGTH: usize = 12;

/// Server-issued unique peer identity (12 chars).
pub type PeerId = String;
/// Caller-chosen room identifier — must be 6..100 chars in upstream
/// (`validateIdString`).
pub type RoomId = String;

/// Per-peer descriptor the CTOX signaling server includes on `joined`
/// broadcasts. Carries the role metadata the peers declared in their
/// signaling-URL query (`role=browser|ctox_instance|…`). Optional on the
/// wire — an upstream-shaped server sends only `otherPeerIds`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalingPeerDescriptor {
    #[serde(default)]
    pub peer_id: PeerId,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub instance_id: String,
    #[serde(default)]
    pub client: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

// ref: rxdb/src/plugins/replication-webrtc/signaling-server.ts:91 + 178-181
/// Frame received from the signaling server.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerToClient {
    /// Issued on connect; carries the peer's server-assigned id.
    Init {
        #[serde(rename = "yourPeerId")]
        your_peer_id: PeerId,
    },
    /// Broadcast to room members when membership changes.
    Joined {
        #[serde(rename = "otherPeerIds")]
        other_peer_ids: Vec<PeerId>,
        /// CTOX extension: role/protocol descriptors for every room member.
        #[serde(default)]
        peers: Vec<SignalingPeerDescriptor>,
    },
    /// Relayed from another peer; opaque `data` is the simple-peer SDP/ICE payload.
    Signal {
        room: RoomId,
        #[serde(rename = "senderPeerId")]
        sender_peer_id: PeerId,
        #[serde(rename = "receiverPeerId")]
        receiver_peer_id: PeerId,
        data: Value,
    },
    /// Control-plane rejection (expired/invalid token, protocol mismatch,
    /// instance mismatch). The CTOX signaling server sends this right before
    /// closing the socket; surfacing it is the only way a peer can tell a
    /// rejected join apart from a network blip.
    #[serde(rename_all = "camelCase")]
    CtoxError {
        #[serde(default)]
        scope: String,
        #[serde(default)]
        code: String,
        #[serde(default)]
        reason: String,
    },
    /// Catch-all so unknown frames don't tear down the connection.
    #[serde(other)]
    Unknown,
}

// ref: rxdb/src/plugins/replication-webrtc/signaling-server.ts:107-167
/// Frame sent from the client to the server.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientToServer {
    /// Join a room.
    Join { room: RoomId },
    /// Send signaling data to a specific other peer.
    Signal {
        room: RoomId,
        #[serde(rename = "senderPeerId")]
        sender_peer_id: PeerId,
        #[serde(rename = "receiverPeerId")]
        receiver_peer_id: PeerId,
        data: Value,
    },
    /// Keepalive (must be sent < `SIMPLE_PEER_PING_INTERVAL` apart).
    Ping,
}

// ref: rxdb/src/plugins/replication-webrtc/connection-handler-simple-peer.ts SIMPLE_PEER_PING_INTERVAL
/// Server drops the connection if it does not receive a ping inside this window.
pub const SIMPLE_PEER_PING_INTERVAL_MS: u64 = 2 * 60 * 1000;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn client_signal_serializes_with_browser_shape() {
        let value = serde_json::to_value(ClientToServer::Signal {
            room: "room-a".to_string(),
            sender_peer_id: "sender".to_string(),
            receiver_peer_id: "receiver".to_string(),
            data: json!({ "type": "offer", "sdp": "v=0" }),
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "type": "signal",
                "room": "room-a",
                "senderPeerId": "sender",
                "receiverPeerId": "receiver",
                "data": { "type": "offer", "sdp": "v=0" }
            })
        );
    }

    #[test]
    fn server_signal_deserializes_browser_shape() {
        let frame: ServerToClient = serde_json::from_value(json!({
            "type": "signal",
            "room": "room-a",
            "senderPeerId": "sender",
            "receiverPeerId": "receiver",
            "data": { "candidate": "candidate:1" }
        }))
        .unwrap();

        match frame {
            ServerToClient::Signal {
                room,
                sender_peer_id,
                receiver_peer_id,
                data,
            } => {
                assert_eq!(room, "room-a");
                assert_eq!(sender_peer_id, "sender");
                assert_eq!(receiver_peer_id, "receiver");
                assert_eq!(data, json!({ "candidate": "candidate:1" }));
            }
            other => panic!("expected signal frame, got {other:?}"),
        }
    }
}
