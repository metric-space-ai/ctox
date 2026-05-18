use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid key length: {0}")]
    InvalidKeyLength(usize),
    #[error("invalid IV length: {0}")]
    InvalidIv(usize),
    #[error("AEAD failed (auth tag mismatch or tampering)")]
    AeadFailed,
    #[error("CBC unpad failure")]
    UnpadFailed,
    #[error("HMAC mismatch")]
    HmacMismatch,
    #[error("ed25519 signature error: {0}")]
    Signature(String),
    #[error("internal: {0}")]
    Internal(String),
}
