use thiserror::Error;
use wha_types::ParseJidError;

#[derive(Debug, Error)]
pub enum BinaryError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("invalid token tag {tag} at position {pos}")]
    InvalidToken { tag: u8, pos: usize },
    #[error("invalid node (zero-size or empty tag)")]
    InvalidNode,
    #[error("invalid JID type: {0}")]
    InvalidJidType(&'static str),
    #[error("non-string attribute key")]
    NonStringKey,
    #[error("invalid type for value: {0}")]
    InvalidType(&'static str),
    #[error("unknown packed-byte tag {0}")]
    UnknownPackedTag(u8),
    #[error("invalid packed-byte value {0}")]
    InvalidPackedValue(u8),
    #[error("packed string too long: {0}")]
    PackedTooLong(usize),
    #[error("frame too large ({0} bytes)")]
    FrameTooLarge(usize),
    #[error("{0} leftover bytes after decoding")]
    LeftoverBytes(usize),
    #[error("invalid attribute: {0}")]
    Attr(String),
    #[error("jid: {0}")]
    Jid(#[from] ParseJidError),
    #[error("zlib: {0}")]
    Zlib(String),
}
