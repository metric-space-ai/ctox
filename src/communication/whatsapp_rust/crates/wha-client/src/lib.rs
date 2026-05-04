//! High-level async [`Client`] — the entry point most users want.
//!
//! The crate is the **reference vertical slice** for the port: it shows how
//! `wha-binary` (XML codec), `wha-crypto` (Noise primitives), `wha-proto`
//! (handshake messages), `wha-socket` (frame transport), `wha-store`
//! (Device + key state) and `wha-signal` (Signal addresses) compose into a
//! full request/response client.
//!
//! Every periphery feature (pair, send, recv, retry, appstate, …) plugs in by
//! adding a module that consumes [`Client::send_iq`] / [`Client::on_node`].

pub mod appstate;
pub mod armadillo_message;
pub mod broadcast;
pub mod call;
pub mod client;
pub mod connection_events;
pub mod download;
pub mod error;
pub mod events;
pub mod group;
pub mod handshake;
pub mod history_sync;
pub mod internals;
pub mod keepalive;
pub mod media_retry;
pub mod msgsecret;
pub mod newsletter;
pub mod notification;
pub mod pair;
pub mod pair_code;
pub mod payload;
pub mod prekeys;
pub mod presence_receipt;
pub mod privacy_settings;
pub mod qr_channel;
pub mod recv_group;
pub mod recv_message;
pub mod reporting_token;
pub mod request;
pub mod retry;
pub mod send;
pub mod tc_token;
pub mod send_encrypt;
pub mod send_message;
pub mod send_fb;
pub mod send_group;
pub mod upload;
pub mod usync;
pub mod user_push;
pub mod version;

pub use client::Client;
pub use error::ClientError;
pub use events::{ConnectFailureReason, Event};
pub use payload::build_client_payload;
pub use request::{InfoQuery, IqType};
