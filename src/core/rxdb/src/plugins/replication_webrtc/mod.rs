//! ============================================================================
//! AGENT GUARDRAILS — ctox-rxdb data plane (read docs/ctox-rxdb.md first)
//! ============================================================================
//! This module is the CTOX side of CTOX DB, the WebRTC-ONLY data plane to
//! Business OS. Hard rules (each one has caused real regressions):
//!   1. NO HTTP fallback/bridge for collection data — ever. WebRTC only
//!      (root README.md "Data Boundary").
//!   2. The native peer is a PASSIVE RESPONDER: it never initiates
//!      RTCPeerConnections from the signaling peer list. Browsers initiate;
//!      the responder is built when their offer arrives (glare protection).
//!   3. Native is ALWAYS master toward role=browser peers; the hash election
//!      only applies between non-browser peers. Do not "simplify" this.
//!   4. Wire-contract constants are GENERATED from the fixtures under
//!      tests/fixtures/ — never hand-edit *_contract_generated.rs or the JS
//!      twins; run the build_webrtc_*_contract.mjs tools instead.
//!   5. NO new process-env toggles — runtime config flows through the SQLite
//!      runtime store (CLAUDE.md operator rule).
//!   6. Keep `cargo test --manifest-path src/core/rxdb/Cargo.toml` AND
//!      `node src/apps/business-os/rxdb/tests/run-all.mjs` green. Never
//!      delete or weaken a failing test to make a change pass.
//! ============================================================================

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
    master_change_stream_id, replicate_web_rtc, replicate_web_rtc_multi, replicate_web_rtc_rs,
    replicate_web_rtc_rs_multi, replicate_web_rtc_rs_multi_with_url_provider,
    replicate_web_rtc_with_options, RxWebRTCReplicationPool, RxWebRTCReplicationState,
    SyncOptionsWebRTC, SyncOptionsWebRTCRs,
};
pub use signaling_client::SignalingClient;
pub use signaling_protocol::{
    ClientToServer, PeerId, RoomId, ServerToClient, SignalingPeerDescriptor, PEER_ID_LENGTH,
    SIMPLE_PEER_PING_INTERVAL_MS,
};
pub use webrtc::peer_connection::RTCIceServer;
pub use webrtc_helper::{is_master_in_webrtc_replication, send_message_and_await_answer};
pub use webrtc_types::{
    PeerWithMessage, PeerWithResponse, WebRTCConnectionHandler, WebRTCMessage, WebRTCResponse,
    WebRTCWireFrame,
};
