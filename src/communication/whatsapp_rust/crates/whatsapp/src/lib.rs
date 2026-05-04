//! # whatsapp-rust
//!
//! A Rust port of [`whatsmeow`](https://github.com/tulir/whatsmeow), the Go
//! library for the WhatsApp Web multi-device API.
//!
//! This crate is the **library entry point** designed for embedding into
//! other Rust programs (chatbots, AI agents, automation tools). Open an
//! [`Account`], drive its event loop, and answer messages — pairing, login,
//! Signal-protocol decryption, history sync, and persistent storage are all
//! handled internally.
//!
//! ## Quick start — give an AI agent a WhatsApp account
//!
//! ```no_run
//! use whatsapp::{Account, Event};
//!
//! #[tokio::main]
//! async fn main() -> whatsapp::Result<()> {
//!     // rustls 0.23 needs an explicit crypto provider when both
//!     // aws-lc-rs and ring are available.
//!     rustls::crypto::ring::default_provider().install_default().unwrap();
//!
//!     let account = Account::open("/var/lib/myagent/whatsapp.sqlite").await?;
//!     let mut events = account.connect().await?;
//!
//!     while let Some(evt) = events.recv().await {
//!         match evt {
//!             Event::Qr { code, .. } => println!("scan with phone: {code}"),
//!             Event::Paired { jid } => println!("paired as {jid}"),
//!             Event::Connected { jid } => println!("logged in as {jid}"),
//!             Event::Message(msg) => {
//!                 if let Some(text) = msg.text() {
//!                     // Plug your agent in here.
//!                     let reply = format!("you said: {text}");
//!                     account.send_text(&msg.chat, &reply).await?;
//!                 }
//!             }
//!             Event::HistorySync(sync) => {
//!                 println!("history: {} chats", sync.conversations.len());
//!             }
//!             Event::Disconnected { reason } => {
//!                 eprintln!("dropped: {reason}");
//!                 break;
//!             }
//!             _ => {}
//!         }
//!     }
//!     Ok(())
//! }
//! ```
//!
//! On first run the SQLite database at the given path is empty, so
//! `connect()` enters Phase 1 and emits an [`Event::Qr`] with the rotating
//! pairing code. After the user scans with their phone, [`Event::Paired`]
//! fires once and the account is persisted; subsequent process restarts skip
//! the QR entirely.
//!
//! ## What's available
//!
//! - **Pairing**: QR-code pairing, persistent device store (SQLite), and
//!   automatic resume across restarts.
//! - **Receive**: full Signal-protocol decryption (pkmsg + msg + skmsg),
//!   plaintext-cache for server retries, automatic `<receipt>` + `<ack>`,
//!   typed [`Event::Message`] with [`IncomingMessage::text`] / `is_media` /
//!   `is_reaction` helpers.
//! - **Send**: [`Account::send_text`], `send_image`, `send_document`,
//!   `send_reaction`, `send_reply`, `send_revoke` — multi-device fanout
//!   handled internally.
//! - **History sync**: automatic on first connect; chat history surfaces as
//!   [`Event::HistorySync`].
//! - **App-state ops**: [`Account::mark_read`] for read receipts; group
//!   ops, mute/pin/archive/star via the lower-level [`client`] re-export.
//!
//! ## Lower-level access
//!
//! For features not yet wrapped on `Account`, all the underlying crates are
//! re-exported and the live [`Client`](wha_client::Client) is reachable
//! via [`Account::client`]. See the module docs for [`client`], [`store`],
//! [`appstate`], etc.
//!
//! ## Architecture
//!
//! Every side effect — websocket, store, clock, RNG — is a trait. The
//! `whatsapp` crate is the composition root that wires up production
//! implementations; tests use in-memory fakes.
//!
//! See `_upstream/whatsmeow/` for the Go reference implementation each
//! module is anchored to.

#![doc(html_root_url = "https://docs.rs/whatsapp/0.1.0")]

mod account;
mod error;

pub use account::{Account, Event, IncomingMessage};
pub use error::{Error, Result};

// Re-export the JID and core types so users don't need to depend on
// `wha-types` directly.
pub use wha_types::{Jid, MessageId, ParseJidError, WhatsAppError};

/// Re-exports of every layered crate, for advanced consumers that need to
/// reach below the `Account` facade.
pub mod proto {
    pub use wha_proto::*;
}
pub mod client {
    pub use wha_client::*;
}
pub mod store {
    pub use wha_store::*;
    /// SQLite-backed `Device` persistence.
    pub mod sqlite {
        pub use wha_store_sqlite::*;
    }
}
pub mod appstate {
    pub use wha_appstate::*;
}
pub mod binary {
    pub use wha_binary::*;
}
pub mod crypto {
    pub use wha_crypto::*;
}
pub mod signal {
    pub use wha_signal::*;
}
pub mod socket {
    pub use wha_socket::*;
}
pub mod types {
    pub use wha_types::*;
}
