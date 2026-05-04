use thiserror::Error;
use wha_binary::BinaryError;
use wha_crypto::CryptoError;
use wha_socket::SocketError;
use wha_store::StoreError;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("not connected")]
    NotConnected,
    #[error("not logged in")]
    NotLoggedIn,
    #[error("already connected")]
    AlreadyConnected,
    #[error("iq timed out")]
    IqTimedOut,
    #[error("iq disconnected")]
    IqDisconnected,
    #[error("iq error: code={code} text={text}")]
    Iq { code: u16, text: String },
    #[error("handshake: {0}")]
    Handshake(String),
    #[error("malformed iq response: {0}")]
    Malformed(String),
    #[error("binary: {0}")]
    Binary(#[from] BinaryError),
    #[error("socket: {0}")]
    Socket(#[from] SocketError),
    #[error("store: {0}")]
    Store(#[from] StoreError),
    #[error("proto: {0}")]
    Proto(String),
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("download: {0}")]
    Download(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not yet implemented: {0}")]
    NotImplemented(&'static str),
    #[error("no signal session for {0}")]
    NoSession(String),
    #[error("other: {0}")]
    Other(String),
    #[error("decrypt: {0}")]
    Decrypt(String),
}

impl From<CryptoError> for ClientError {
    fn from(e: CryptoError) -> Self {
        ClientError::Crypto(e.to_string())
    }
}

impl From<prost::EncodeError> for ClientError {
    fn from(e: prost::EncodeError) -> Self { ClientError::Proto(e.to_string()) }
}

impl From<prost::DecodeError> for ClientError {
    fn from(e: prost::DecodeError) -> Self { ClientError::Proto(e.to_string()) }
}

impl From<wha_signal::SignalProtocolError> for ClientError {
    fn from(e: wha_signal::SignalProtocolError) -> Self {
        ClientError::Decrypt(format!("{e}"))
    }
}
