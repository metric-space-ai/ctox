// ref: internal/runtime/executor/helps/devin_wire.go:193-251,971-1029
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::io::{self, Read};

use flate2::read::MultiGzDecoder;
use serde::Deserialize;

pub const CONNECT_FLAG_DATA: u8 = 0;
pub const CONNECT_FLAG_COMPRESSED: u8 = 1;
pub const CONNECT_FLAG_END_STREAM: u8 = 2;
pub const MAX_CONNECT_FRAME_SIZE: usize = 16 * 1024 * 1024;
pub const MAX_DECOMPRESSED_FRAME_SIZE: usize = 64 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct ConnectFrame {
    /// Retain the original flag, including compression and end-stream bits.
    pub flag: u8,
    pub payload: Vec<u8>,
}

impl fmt::Debug for ConnectFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectFrame")
            .field("flag", &self.flag)
            .field("payload_bytes", &self.payload.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectFrameError {
    EndOfInput,
    Truncated,
    InvalidFlag(u8),
    FrameTooLarge,
    DecompressedFrameTooLarge,
    InvalidCompression,
    Read(io::ErrorKind),
}

impl fmt::Display for ConnectFrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndOfInput => f.write_str("end of Connect stream"),
            Self::Truncated => f.write_str("incomplete Connect frame"),
            Self::InvalidFlag(flag) => write!(f, "invalid Connect frame flag: 0x{flag:02x}"),
            Self::FrameTooLarge => f.write_str("Connect frame exceeds maximum size"),
            Self::DecompressedFrameTooLarge => {
                f.write_str("decompressed Connect frame exceeds maximum size")
            }
            Self::InvalidCompression => f.write_str("invalid gzip Connect frame"),
            Self::Read(kind) => write!(f, "Connect frame read failed: {kind:?}"),
        }
    }
}

impl std::error::Error for ConnectFrameError {}

fn frame_header(header: &[u8; 5], limit: usize) -> Result<(u8, usize), ConnectFrameError> {
    let flag = header[0];
    if flag > (CONNECT_FLAG_COMPRESSED | CONNECT_FLAG_END_STREAM) {
        return Err(ConnectFrameError::InvalidFlag(flag));
    }
    let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
    if length > limit {
        return Err(ConnectFrameError::FrameTooLarge);
    }
    Ok((flag, length))
}

fn decode_payload(
    flag: u8,
    payload: Vec<u8>,
    limit: usize,
) -> Result<ConnectFrame, ConnectFrameError> {
    if flag & CONNECT_FLAG_COMPRESSED == 0 {
        return Ok(ConnectFrame { flag, payload });
    }
    // Go's gzip.Reader accepts concatenated members. Limit the entire inflated
    // body, including every member, and force CRC/trailer validation on EOF.
    let decoder = MultiGzDecoder::new(payload.as_slice());
    let mut bounded = decoder.take(limit as u64 + 1);
    let mut decoded = Vec::with_capacity(
        (payload.len().saturating_mul(4))
            .clamp(4096, 4 * 1024 * 1024)
            .min(limit),
    );
    bounded
        .read_to_end(&mut decoded)
        .map_err(|_| ConnectFrameError::InvalidCompression)?;
    if decoded.len() > limit {
        return Err(ConnectFrameError::DecompressedFrameTooLarge);
    }
    Ok(ConnectFrame {
        flag,
        payload: decoded,
    })
}

/// Reads exactly one upstream frame and leaves subsequent frames in the reader.
pub fn read_connect_frame(reader: &mut impl Read) -> Result<ConnectFrame, ConnectFrameError> {
    let mut header = [0; 5];
    loop {
        match reader.read(&mut header[..1]) {
            Ok(0) => return Err(ConnectFrameError::EndOfInput),
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(ConnectFrameError::Read(error.kind())),
        }
    }
    reader.read_exact(&mut header[1..]).map_err(read_error)?;
    let (flag, length) = frame_header(&header, MAX_CONNECT_FRAME_SIZE)?;
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload).map_err(read_error)?;
    decode_payload(flag, payload, MAX_DECOMPRESSED_FRAME_SIZE)
}

fn read_error(error: io::Error) -> ConnectFrameError {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        ConnectFrameError::Truncated
    } else {
        ConnectFrameError::Read(error.kind())
    }
}

pub fn wrap_connect_envelope(payload: &[u8]) -> Result<Vec<u8>, ConnectFrameError> {
    wrap_connect_envelope_with_flag(CONNECT_FLAG_DATA, payload)
}

pub fn wrap_connect_envelope_with_flag(
    flag: u8,
    payload: &[u8],
) -> Result<Vec<u8>, ConnectFrameError> {
    if flag > (CONNECT_FLAG_COMPRESSED | CONNECT_FLAG_END_STREAM) {
        return Err(ConnectFrameError::InvalidFlag(flag));
    }
    // Typed host boundary: avoid truncating the u32 wire length or constructing
    // a request larger than the matching decoder's accepted frame limit.
    if payload.len() > MAX_CONNECT_FRAME_SIZE {
        return Err(ConnectFrameError::FrameTooLarge);
    }
    let mut bytes = Vec::with_capacity(5 + payload.len());
    bytes.push(flag);
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

/// Incremental framing for async HTTP body chunks. It retains at most one
/// bounded frame and publishes preceding frames before a later framing error.
pub struct ConnectFrameDecoder {
    header: [u8; 5],
    header_len: usize,
    expected: Option<(u8, usize)>,
    payload: Vec<u8>,
    failed: Option<ConnectFrameError>,
    frame_limit: usize,
    decompressed_limit: usize,
}

impl Default for ConnectFrameDecoder {
    fn default() -> Self {
        Self::with_limits(MAX_CONNECT_FRAME_SIZE, MAX_DECOMPRESSED_FRAME_SIZE)
    }
}

impl ConnectFrameDecoder {
    pub(super) fn with_limits(frame_limit: usize, decompressed_limit: usize) -> Self {
        Self {
            header: [0; 5],
            header_len: 0,
            expected: None,
            payload: Vec::new(),
            failed: None,
            frame_limit,
            decompressed_limit,
        }
    }

    pub fn feed(
        &mut self,
        input: &[u8],
        mut emit: impl FnMut(ConnectFrame),
    ) -> Result<(), ConnectFrameError> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        let result = self.feed_inner(input, &mut emit);
        if let Err(error) = result {
            self.failed = Some(error);
            self.payload = Vec::new();
            self.expected = None;
        }
        result
    }

    fn feed_inner(
        &mut self,
        mut input: &[u8],
        emit: &mut impl FnMut(ConnectFrame),
    ) -> Result<(), ConnectFrameError> {
        while !input.is_empty() {
            if self.expected.is_none() {
                let count = input.len().min(5 - self.header_len);
                self.header[self.header_len..self.header_len + count]
                    .copy_from_slice(&input[..count]);
                self.header_len += count;
                input = &input[count..];
                if self.header_len < 5 {
                    continue;
                }
                let expected = frame_header(&self.header, self.frame_limit)?;
                self.payload = Vec::with_capacity(expected.1);
                self.expected = Some(expected);
                self.header_len = 0;
            }
            let (flag, length) = self.expected.expect("header was decoded above");
            let count = input.len().min(length - self.payload.len());
            self.payload.extend_from_slice(&input[..count]);
            input = &input[count..];
            if self.payload.len() == length {
                let payload = std::mem::take(&mut self.payload);
                self.expected = None;
                emit(decode_payload(flag, payload, self.decompressed_limit)?);
            }
        }
        Ok(())
    }

    /// Call only when the HTTP body ends, so partial headers/bodies cannot
    /// masquerade as a successful upstream completion.
    pub fn finish(&self) -> Result<(), ConnectFrameError> {
        if let Some(error) = self.failed {
            Err(error)
        } else if self.header_len != 0 || self.expected.is_some() {
            Err(ConnectFrameError::Truncated)
        } else {
            Ok(())
        }
    }
}

impl fmt::Debug for ConnectFrameDecoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectFrameDecoder")
            .field("header_bytes", &self.header_len)
            .field("payload_bytes", &self.payload.len())
            .field("failed", &self.failed)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevinTrailerError {
    pub status_code: u16,
    pub code: String,
    pub message: String,
}

impl fmt::Display for DevinTrailerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "devin upstream error ({}): {}", self.code, self.message)
    }
}

impl std::error::Error for DevinTrailerError {}

/// Preserve upstream's strict typed JSON decoding and HTTP classifications.
/// Empty, malformed or metadata-only trailers carry no typed upstream error.
pub fn parse_devin_trailer_error(payload: &[u8]) -> Option<DevinTrailerError> {
    #[derive(Deserialize)]
    struct Trailer {
        error: Option<Error>,
    }
    #[derive(Deserialize)]
    struct Error {
        #[serde(default, deserialize_with = "null_to_empty")]
        code: String,
        #[serde(default, deserialize_with = "null_to_empty")]
        message: String,
    }
    // encoding/json accepts null for a string field as its zero value.
    fn null_to_empty<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<String, D::Error> {
        Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
    }
    let error = serde_json::from_slice::<Trailer>(payload).ok()?.error?;
    let code = error.code.to_lowercase();
    let message = error.message.to_lowercase();
    let status_code = match code.as_str() {
        "invalid_argument" if !message.contains("internal error") => 400,
        "unauthenticated" => 401,
        "permission_denied" if message.contains("high demand") => 429,
        "permission_denied" => 403,
        "resource_exhausted" => 429,
        "unavailable" => 503,
        "canceled" => 499,
        "deadline_exceeded" => 504,
        "failed_precondition"
            if ["quota", "credit", "acu", "exhausted", "limit"]
                .iter()
                .any(|needle| message.contains(needle)) =>
        {
            429
        }
        "failed_precondition" => 400,
        _ => 502,
    };
    Some(DevinTrailerError {
        status_code,
        code: error.code,
        message: error.message,
    })
}
