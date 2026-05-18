//! Common value types shared by every other `wha-*` crate.
//!
//! This is a pure-data crate — no I/O, no async, no protobuf. It exists so that
//! `wha-binary`, `wha-store`, `wha-client` and `wha-signal` can all agree on
//! the same `Jid`/`MessageId`/error newtypes without depending on each other.

pub mod botmap;
pub mod error;
pub mod jid;

pub use botmap::BotMap;
pub use error::{ParseJidError, WhatsAppError};
pub use jid::{Jid, MessageId, MessageServerId, Server};
