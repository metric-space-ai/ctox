// ref: internal/pluginhost/rpc_schema.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: complete upstream JSON schema inside bounded CTOX process frames
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::sdk::{
    pluginabi::Envelope,
    pluginapi::{ExecutorModelScope, Headers, Metadata},
};

/// CTOX process protocol version. This is deliberately separate from the
/// upstream plugin schema version carried by `RpcLifecycleRequest`.
pub const PROCESS_PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_REQUEST_ID_BYTES: usize = 64;
pub const MAX_METHOD_BYTES: usize = 128;

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RpcLifecycleRequest {
    #[serde(with = "base64_bytes")]
    pub config_yaml: Vec<u8>,
    pub schema_version: u32,
}

impl fmt::Debug for RpcLifecycleRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RpcLifecycleRequest")
            .field("config_yaml", &"[REDACTED]")
            .field("config_yaml_bytes", &self.config_yaml.len())
            .field("schema_version", &self.schema_version)
            .finish()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RpcRegistration {
    pub schema_version: u32,
    pub metadata: Metadata,
    pub capabilities: RpcCapabilities,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RpcCapabilities {
    pub model_registrar: bool,
    pub model_provider: bool,
    pub auth_provider: bool,
    pub frontend_auth_provider: bool,
    pub frontend_auth_provider_exclusive: bool,
    pub scheduler: bool,
    pub model_router: bool,
    pub executor: bool,
    pub executor_model_scope: ExecutorModelScope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executor_input_formats: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executor_output_formats: Vec<String>,
    pub request_translator: bool,
    pub request_normalizer: bool,
    pub request_interceptor: bool,
    pub request_lifecycle_plugin: bool,
    pub response_translator: bool,
    pub response_before_translator: bool,
    pub response_after_translator: bool,
    pub response_interceptor: bool,
    #[serde(rename = "response_stream_interceptor")]
    pub stream_chunk_interceptor: bool,
    pub thinking_applier: bool,
    pub usage_plugin: bool,
    pub quota_provider: bool,
    pub command_line_plugin: bool,
    pub management_api: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RpcIdentifierResponse {
    pub identifier: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcEmptyResponse {}

/// All upstream request wrappers that add only `host_callback_id` share this
/// representation in Rust. Flattening preserves the exact Go embedded-struct
/// JSON shape while avoiding a dozen type-identical wrappers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcCallbackRequest<Request> {
    #[serde(flatten)]
    pub request: Request,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_callback_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcExecutorStreamRequest {
    #[serde(flatten)]
    pub request: crate::sdk::pluginapi::ExecutorRequest,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stream_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_callback_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RpcExecutorStreamResponse {
    #[serde(skip_serializing_if = "Headers::is_empty")]
    pub headers: Headers,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chunks: Vec<RpcExecutorStreamChunk>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct RpcExecutorStreamChunk {
    #[serde(with = "base64_bytes")]
    pub payload: Vec<u8>,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Err")]
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RpcManagementRegistrationResponse {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<RpcManagementRoute>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<RpcManagementRoute>,
}

/// Process-safe descriptor for upstream management routes. Handler function
/// pointers never cross the ABI; the host binds an RPC handler after decode.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RpcManagementRoute {
    pub method: String,
    pub path: String,
    pub menu: String,
    pub description: String,
}

/// The safe replacement for the upstream in-process call boundary. Payloads
/// remain upstream-compatible JSON, while correlation, deadlines and explicit
/// cancellation live in a versioned outer process frame.
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "body",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ProcessMessage {
    Request {
        protocol_version: u32,
        request_id: String,
        method: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deadline_unix_ms: Option<u64>,
        payload: Box<RawValue>,
    },
    Response {
        protocol_version: u32,
        request_id: String,
        envelope: Envelope,
    },
    Cancel {
        protocol_version: u32,
        request_id: String,
    },
    StreamChunk {
        protocol_version: u32,
        request_id: String,
        sequence: u64,
        payload: Box<RawValue>,
    },
    StreamEnd {
        protocol_version: u32,
        request_id: String,
        next_sequence: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<crate::sdk::pluginabi::Error>,
    },
}

impl fmt::Debug for ProcessMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("ProcessMessage");
        match self {
            Self::Request {
                protocol_version,
                request_id,
                method,
                deadline_unix_ms,
                payload,
            } => debug
                .field("kind", &"request")
                .field("protocol_version", protocol_version)
                .field("request_id", request_id)
                .field("method", method)
                .field("deadline_unix_ms", deadline_unix_ms)
                .field("payload", &"[REDACTED]")
                .field("payload_bytes", &payload.get().len()),
            Self::Response {
                protocol_version,
                request_id,
                envelope,
            } => debug
                .field("kind", &"response")
                .field("protocol_version", protocol_version)
                .field("request_id", request_id)
                .field("ok", &envelope.ok)
                .field("has_result", &envelope.result.is_some())
                .field("has_error", &envelope.error.is_some()),
            Self::Cancel {
                protocol_version,
                request_id,
            } => debug
                .field("kind", &"cancel")
                .field("protocol_version", protocol_version)
                .field("request_id", request_id),
            Self::StreamChunk {
                protocol_version,
                request_id,
                sequence,
                payload,
            } => debug
                .field("kind", &"stream_chunk")
                .field("protocol_version", protocol_version)
                .field("request_id", request_id)
                .field("sequence", sequence)
                .field("payload", &"[REDACTED]")
                .field("payload_bytes", &payload.get().len()),
            Self::StreamEnd {
                protocol_version,
                request_id,
                next_sequence,
                error,
            } => debug
                .field("kind", &"stream_end")
                .field("protocol_version", protocol_version)
                .field("request_id", request_id)
                .field("next_sequence", next_sequence)
                .field("has_error", &error.is_some()),
        };
        debug.finish()
    }
}

impl ProcessMessage {
    pub fn validate(&self) -> Result<(), ProcessCodecError> {
        let (protocol_version, request_id) = match self {
            Self::Request {
                protocol_version,
                request_id,
                method,
                ..
            } => {
                if method.is_empty()
                    || method.len() > MAX_METHOD_BYTES
                    || !method.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._".contains(&byte)
                    })
                {
                    return Err(ProcessCodecError::InvalidMethod);
                }
                (*protocol_version, request_id)
            }
            Self::Response {
                protocol_version,
                request_id,
                ..
            }
            | Self::Cancel {
                protocol_version,
                request_id,
            }
            | Self::StreamChunk {
                protocol_version,
                request_id,
                ..
            }
            | Self::StreamEnd {
                protocol_version,
                request_id,
                ..
            } => (*protocol_version, request_id),
        };
        if protocol_version != PROCESS_PROTOCOL_VERSION {
            return Err(ProcessCodecError::UnsupportedVersion);
        }
        if request_id.is_empty() || request_id.len() > MAX_REQUEST_ID_BYTES {
            return Err(ProcessCodecError::InvalidRequestId);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessCodecError {
    FrameTooLarge,
    TruncatedFrame,
    TrailingBytes,
    InvalidJson,
    UnsupportedVersion,
    InvalidRequestId,
    InvalidMethod,
}

impl fmt::Display for ProcessCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::FrameTooLarge => "plugin frame exceeds the configured limit",
            Self::TruncatedFrame => "plugin frame is truncated",
            Self::TrailingBytes => "plugin frame contains trailing bytes",
            Self::InvalidJson => "plugin frame contains invalid JSON",
            Self::UnsupportedVersion => "plugin process protocol version is unsupported",
            Self::InvalidRequestId => "plugin request identifier is invalid",
            Self::InvalidMethod => "plugin method is invalid",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProcessCodecError {}

pub fn encode_process_frame(message: &ProcessMessage) -> Result<Vec<u8>, ProcessCodecError> {
    message.validate()?;
    let payload = serde_json::to_vec(message).map_err(|_| ProcessCodecError::InvalidJson)?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(ProcessCodecError::FrameTooLarge);
    }
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn decode_process_frame(frame: &[u8]) -> Result<ProcessMessage, ProcessCodecError> {
    if frame.len() < 4 {
        return Err(ProcessCodecError::TruncatedFrame);
    }
    let declared = u32::from_be_bytes(frame[..4].try_into().expect("four-byte prefix")) as usize;
    if declared > MAX_FRAME_BYTES {
        return Err(ProcessCodecError::FrameTooLarge);
    }
    let expected = 4usize
        .checked_add(declared)
        .ok_or(ProcessCodecError::FrameTooLarge)?;
    if frame.len() < expected {
        return Err(ProcessCodecError::TruncatedFrame);
    }
    if frame.len() > expected {
        return Err(ProcessCodecError::TrailingBytes);
    }
    let message: ProcessMessage =
        serde_json::from_slice(&frame[4..]).map_err(|_| ProcessCodecError::InvalidJson)?;
    message.validate()?;
    Ok(message)
}

pub fn encode_upstream_json<T: Serialize>(value: &T) -> Result<Box<RawValue>, ProcessCodecError> {
    serde_json::value::to_raw_value(value).map_err(|_| ProcessCodecError::InvalidJson)
}

pub fn decode_upstream_json<T: DeserializeOwned>(value: &RawValue) -> Result<T, ProcessCodecError> {
    serde_json::from_str(value.get()).map_err(|_| ProcessCodecError::InvalidJson)
}

mod base64_bytes {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&BASE64_STANDARD.encode(value))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        BASE64_STANDARD
            .decode(encoded)
            .map_err(serde::de::Error::custom)
    }
}
