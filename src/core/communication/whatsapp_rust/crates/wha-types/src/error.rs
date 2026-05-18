use thiserror::Error;

/// Errors that bubble up from `wha-types::jid::Jid::parse`.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseJidError {
    #[error("unexpected number of dots in JID")]
    TooManyDots,
    #[error("unexpected number of colons in JID")]
    TooManyColons,
    #[error("failed to parse agent in JID: {0}")]
    BadAgent(String),
    #[error("failed to parse device in JID: {0}")]
    BadDevice(String),
}

/// Top-level error type used by the higher crates. Each error variant
/// is meant to mirror one of the sentinel errors in `whatsmeow/errors.go`,
/// rather than collapse them into a generic `String`.
#[derive(Debug, Error)]
pub enum WhatsAppError {
    #[error("not connected")]
    NotConnected,
    #[error("not logged in")]
    NotLoggedIn,
    #[error("already connected")]
    AlreadyConnected,
    #[error("client is nil")]
    ClientIsNil,
    #[error("no session")]
    NoSession,
    #[error("iq timed out")]
    IqTimedOut,
    #[error("iq disconnected")]
    IqDisconnected,
    #[error("no push name")]
    NoPushName,
    #[error("server returned error: {code} {text}")]
    Iq { code: u16, text: String },
    #[error("invalid JID: {0}")]
    Jid(#[from] ParseJidError),
    #[error("binary protocol error: {0}")]
    Binary(String),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("other: {0}")]
    Other(String),
}
