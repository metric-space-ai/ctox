//! Error type for the [`crate::Account`] facade.

use thiserror::Error;

/// All errors a library consumer can hit. Wraps the lower-level errors from
/// `wha-client` and `wha-store` plus a few facade-specific ones.
#[derive(Debug, Error)]
pub enum Error {
    /// `connect()` was called but the previous call hasn't finished, or some
    /// other API expectation was violated.
    #[error("configuration error: {0}")]
    Config(String),

    /// A method was called that requires a live connection
    /// ([`crate::Account::connect`] hasn't completed yet, or the connection
    /// dropped).
    #[error("not connected — call Account::connect first")]
    NotConnected,

    /// Underlying client error (IQ failed, decrypt failed, websocket dropped,
    /// etc.).
    #[error(transparent)]
    Client(#[from] wha_client::ClientError),

    /// Underlying store error (SQLite open failed, blob malformed, etc.).
    #[error(transparent)]
    Store(#[from] wha_store::StoreError),

    /// We were disconnected during pairing or login.
    #[error("disconnected: {0}")]
    Disconnected(String),

    /// Unexpected internal invariant violated. File a bug if you see this.
    #[error("internal: {0}")]
    Internal(String),
}

/// Library result type, alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;
