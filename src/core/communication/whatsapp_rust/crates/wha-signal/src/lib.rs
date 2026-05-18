//! Minimal Signal-protocol primitives we need at the foundation layer.
//!
//! whatsmeow leans on `go.mau.fi/libsignal` for the heavy lifting (Double
//! Ratchet, SessionBuilder, etc.). The bits we need at the foundation
//! layer are now ported and interop-verified with libsignal:
//!
//! * [`SignalAddress`] — `(name, device_id)` pair with the canonical
//!   serialised form WhatsApp uses inside the binary protocol.
//! * [`SenderKeyName`] — group + sender pair used for sender-key sessions.
//! * [`PreKeyBundle`] — what the server hands back when fetching keys for a
//!   peer.
//! * [`ChainKey`], [`MessageKeys`] — symmetric chain-key advancement.
//! * [`RootKey`] — root-key + DH-ratchet step.
//! * [`IdentityKeyPair`] — long-term identity key wrapping `wha_crypto::KeyPair`.
//! * [`SessionState`] — Double-Ratchet session state with X3DH establishment.
//! * [`SignalMessage`] / [`PreKeySignalMessage`] — wire-format encoders
//!   and decoders (hand-rolled protobuf body, version byte, MAC tail).
//! * [`SessionCipher`] — `encrypt`/`decrypt` over a `SessionState`.

use thiserror::Error;

pub mod address;
pub mod bundle;
pub mod chain_key;
pub mod cipher;
pub mod group_cipher;
pub mod group_session;
pub mod identity;
pub mod protocol_message;
pub mod root_key;
pub mod sender_key;
pub mod sender_key_record;
pub mod session;
pub mod skipped_keys;
pub mod x3dh;

pub use address::{SenderKeyName, SignalAddress};
pub use bundle::PreKeyBundle;
pub use chain_key::{ChainKey, MessageKeys};
pub use cipher::SessionCipher;
pub use identity::IdentityKeyPair;
pub use protocol_message::{PreKeySignalMessage, SignalMessage, CURRENT_VERSION};
pub use root_key::RootKey;
pub use sender_key::{SenderChainKey, SenderKeyState};
pub use session::SessionState;

/// Errors raised by the Double-Ratchet / Signal protocol layer.
#[derive(Debug, Error)]
pub enum SignalProtocolError {
    #[error("invalid message: {0}")]
    InvalidMessage(&'static str),
    #[error("unsupported version: {0}")]
    UnsupportedVersion(u8),
    #[error("bad MAC")]
    BadMac,
    #[error("bad signature")]
    BadSignature,
    #[error("uninitialised session")]
    UninitialisedSession,
    #[error("duplicate / old counter (chain index {chain}, message counter {counter})")]
    DuplicateMessage { chain: u32, counter: u32 },
    #[error("message too far into future (skip > MAX_SKIP)")]
    TooFarIntoFuture,
    #[error("no matching message keys for skipped counter")]
    NoMatchingMessageKey,
    #[error("decrypt failed: {0}")]
    DecryptFailed(&'static str),
    #[error("crypto: {0}")]
    Crypto(#[from] wha_crypto::CryptoError),
}
