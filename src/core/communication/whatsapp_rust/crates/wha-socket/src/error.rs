use thiserror::Error;
use wha_crypto::CryptoError;

#[derive(Debug, Error)]
pub enum SocketError {
    #[error("frame too large ({0} bytes)")]
    FrameTooLarge(usize),
    #[error("frame socket is closed")]
    Closed,
    #[error("frame socket is already open")]
    AlreadyOpen,
    #[error("dial failed: {0}")]
    DialFailed(String),
    #[error("websocket: {0}")]
    Ws(String),
    #[error("noise: {0}")]
    Noise(String),
    #[error("crypto: {0}")]
    Crypto(#[from] CryptoError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
