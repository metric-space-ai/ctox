//! Mirrors `_upstream/whatsmeow/appstate/errors.go`.

use thiserror::Error;

/// Errors that the appstate module can return.
#[derive(Debug, Error)]
pub enum AppStateError {
    #[error("missing value MAC of previous SET operation")]
    MissingPreviousSetValueOperation,
    #[error("mismatching LTHash")]
    MismatchedLtHash,
    #[error("mismatching patch MAC")]
    MismatchedPatchMac,
    #[error("mismatching content MAC")]
    MismatchedContentMac,
    #[error("mismatching index MAC")]
    MismatchedIndexMac,
    #[error("didn't find app state key {0}")]
    KeyNotFound(String),
    #[error("hkdf expansion failed: {0}")]
    Hkdf(String),
    #[error("aes-cbc failure: {0}")]
    Crypto(String),
    #[error("malformed protobuf: {0}")]
    Protobuf(String),
    #[error("malformed mutation value blob")]
    MalformedValueBlob,
    #[error("unknown patch name: {0}")]
    UnknownPatchName(String),
    #[error("store error: {0}")]
    Store(String),
}

impl From<wha_crypto::CryptoError> for AppStateError {
    fn from(e: wha_crypto::CryptoError) -> Self {
        AppStateError::Crypto(e.to_string())
    }
}

impl From<prost::DecodeError> for AppStateError {
    fn from(e: prost::DecodeError) -> Self {
        AppStateError::Protobuf(e.to_string())
    }
}

impl From<prost::EncodeError> for AppStateError {
    fn from(e: prost::EncodeError) -> Self {
        AppStateError::Protobuf(e.to_string())
    }
}
