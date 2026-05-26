//! Port of `src/plugins/replication-webrtc/` — phase-4 WebRTC transport.
//!
//! The user-facing `replicateWebRTC()` entry point is available through the
//! Rust-native `replicate_web_rtc*` helpers. CTOX uses `webrtc-rs` for the
//! daemon peer and keeps browser-side `simple-peer` inside the upstream JS
//! bundle.

pub mod connection_handler_rs;
pub mod file_fetch_handler;
pub mod index_mod;
pub(super) mod protocol_contract_generated;
pub mod query_fetch_handler;
pub mod signaling_client;
pub mod signaling_protocol;
pub mod v1_5_status;
pub mod webrtc_helper;
pub mod webrtc_types;

pub use connection_handler_rs::{WebRTCRsConfig, WebRTCRsConnectionHandler, WebRTCRsPeer};
pub use index_mod::{
    replicate_web_rtc, replicate_web_rtc_rs, replicate_web_rtc_with_options,
    RxWebRTCReplicationPool, RxWebRTCReplicationState, SyncOptionsWebRTC, SyncOptionsWebRTCRs,
};
pub use signaling_client::SignalingClient;
pub use signaling_protocol::{
    ClientToServer, PeerId, RoomId, ServerToClient, PEER_ID_LENGTH, SIMPLE_PEER_PING_INTERVAL_MS,
};
pub use webrtc::peer_connection::RTCIceServer;
pub use webrtc_helper::{is_master_in_webrtc_replication, send_message_and_await_answer};
pub use webrtc_types::{
    PeerWithMessage, PeerWithResponse, WebRTCConnectionHandler, WebRTCMessage, WebRTCResponse,
    WebRTCWireFrame,
};
