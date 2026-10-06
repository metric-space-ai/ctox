// ref: sdk/cliproxy/executor/types.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc;

use crate::sdk::translator::{Format, TranslationContext};

use super::{ExecutionLifecycle, ExecutionTransportContext};

pub const REQUESTED_MODEL_METADATA_KEY: &str = "requested_model";
pub const REQUEST_PATH_METADATA_KEY: &str = "request_path";
pub const DISALLOW_FREE_AUTH_METADATA_KEY: &str = "disallow_free_auth";
pub const AUTH_SELECTION_MODEL_METADATA_KEY: &str = "auth_selection_model";
pub const REASONING_EFFORT_METADATA_KEY: &str = "reasoning_effort";
pub const SERVICE_TIER_METADATA_KEY: &str = "service_tier";
pub const GENERATE_METADATA_KEY: &str = "generate";
pub const PINNED_AUTH_METADATA_KEY: &str = "pinned_auth_id";
pub const SELECTED_AUTH_METADATA_KEY: &str = "selected_auth_id";
pub const SELECTED_AUTH_CALLBACK_METADATA_KEY: &str = "selected_auth_callback";
pub const SELECTED_AUTH_INDEX_METADATA_KEY: &str = "selected_auth_index";
pub const SELECTED_AUTH_INDEX_CALLBACK_METADATA_KEY: &str = "selected_auth_index_callback";
pub const EXECUTION_SESSION_METADATA_KEY: &str = "execution_session_id";
pub const DERIVED_SESSION_ID_METADATA_KEY: &str = "derived_session_id";
pub const CALLER_SCOPE_METADATA_KEY: &str = "caller_scope";

pub type Headers = BTreeMap<String, Vec<String>>;
pub type QueryValues = BTreeMap<String, Vec<String>>;
pub type JsonMetadata = BTreeMap<String, Value>;
pub type ExecutionError = Arc<dyn Error + Send + Sync + 'static>;
pub type SelectedAuthCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;
pub type SelectedAuthIndexCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// Typed execution hints. Callback-bearing fields stay outside the JSON-like
/// extension map so they cannot be serialized, logged, or confused with data.
#[derive(Clone, Default)]
pub struct ExecutionMetadata {
    pub requested_model: Option<String>,
    pub request_path: Option<String>,
    pub disallow_free_auth: bool,
    pub auth_selection_model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
    pub generate: Option<bool>,
    pub pinned_auth_id: Option<String>,
    pub selected_auth_id: Option<String>,
    pub selected_auth_callback: Option<SelectedAuthCallback>,
    pub selected_auth_index: Option<String>,
    pub selected_auth_index_callback: Option<SelectedAuthIndexCallback>,
    pub execution_session_id: Option<String>,
    pub derived_session_id: Option<String>,
    pub caller_scope: Option<String>,
    /// Private, immutable capability selected for one configured credential.
    /// It is typed so it can never be serialized into extension metadata.
    pub resolved_api_key_model_info: Option<Arc<crate::internal::modelconfig::ModelInfo>>,
    /// Selected Codex OAuth plan capability, separate from API-key authority.
    pub resolved_codex_oauth_model_info: Option<Arc<crate::internal::modelconfig::ModelInfo>>,
    pub extensions: JsonMetadata,
}

impl ExecutionMetadata {
    /// Missing or true means generation is enabled; only explicit false turns
    /// it off, matching the upstream metadata contract.
    #[must_use]
    pub fn generate_enabled(&self) -> bool {
        self.generate.unwrap_or(true)
    }

    pub fn notify_selected_auth(&self, auth_id: &str, auth_index: &str) {
        if let Some(callback) = &self.selected_auth_callback {
            callback(auth_id);
        }
        if let Some(callback) = &self.selected_auth_index_callback {
            callback(auth_index);
        }
    }
}

impl fmt::Debug for ExecutionMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionMetadata")
            .field("requested_model", &self.requested_model)
            .field("request_path", &self.request_path)
            .field("disallow_free_auth", &self.disallow_free_auth)
            .field("auth_selection_model", &self.auth_selection_model)
            .field("reasoning_effort", &self.reasoning_effort)
            .field("service_tier", &self.service_tier)
            .field("generate", &self.generate)
            .field("pinned_auth_id", &self.pinned_auth_id)
            .field("selected_auth_id", &self.selected_auth_id)
            .field(
                "selected_auth_callback",
                &self.selected_auth_callback.is_some(),
            )
            .field("selected_auth_index", &self.selected_auth_index)
            .field(
                "selected_auth_index_callback",
                &self.selected_auth_index_callback.is_some(),
            )
            .field("execution_session_id", &self.execution_session_id)
            .field("derived_session_id", &self.derived_session_id)
            .field("caller_scope", &self.caller_scope)
            .field(
                "has_resolved_api_key_model_info",
                &self.resolved_api_key_model_info.is_some(),
            )
            .field(
                "has_resolved_codex_oauth_model_info",
                &self.resolved_codex_oauth_model_info.is_some(),
            )
            .field(
                "extension_keys",
                &self.extensions.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// Translated provider request.
#[derive(Clone, Default)]
pub struct Request {
    pub model: String,
    pub payload: Vec<u8>,
    pub format: Format,
    pub metadata: ExecutionMetadata,
}

impl fmt::Debug for Request {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Request")
            .field("model", &self.model)
            .field("payload_len", &self.payload.len())
            .field("format", &self.format)
            .field("metadata", &self.metadata)
            .finish()
    }
}

/// Selected-auth request snapshot before executor translation.
#[derive(Clone, Default)]
pub struct RequestAfterAuthInterceptRequest {
    pub source_format: Format,
    pub to_format: Format,
    pub model: String,
    pub requested_model: String,
    pub stream: bool,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub metadata: ExecutionMetadata,
}

impl fmt::Debug for RequestAfterAuthInterceptRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestAfterAuthInterceptRequest")
            .field("source_format", &self.source_format)
            .field("to_format", &self.to_format)
            .field("model", &self.model)
            .field("requested_model", &self.requested_model)
            .field("stream", &self.stream)
            .field("headers", &self.headers)
            .field("body_len", &self.body.len())
            .field("metadata", &self.metadata)
            .finish()
    }
}

/// Mutations returned by a selected-auth interceptor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RequestAfterAuthInterceptResponse {
    pub headers: Headers,
    pub body: Vec<u8>,
    pub clear_headers: Vec<String>,
    pub terminate: bool,
    pub status_code: u16,
    pub response_headers: Headers,
    pub response_body: Vec<u8>,
}

pub type RequestAfterAuthInterceptor = Arc<
    dyn Fn(
            &TranslationContext,
            RequestAfterAuthInterceptRequest,
        ) -> RequestAfterAuthInterceptResponse
        + Send
        + Sync
        + 'static,
>;

/// Plugin-defined downstream response that skips upstream execution.
#[derive(Clone)]
pub struct RequestTerminatedError {
    pub http_status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl RequestTerminatedError {
    #[must_use]
    pub fn status_code(&self) -> u16 {
        self.http_status
    }

    #[must_use]
    pub fn response_headers(&self) -> Headers {
        self.headers.clone()
    }

    #[must_use]
    pub fn response_body(&self) -> Vec<u8> {
        self.body.clone()
    }
}

impl fmt::Debug for RequestTerminatedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestTerminatedError")
            .field("http_status", &self.http_status)
            .field("headers", &self.headers)
            .field("body_len", &self.body.len())
            .finish()
    }
}

impl fmt::Display for RequestTerminatedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("request terminated by plugin")
    }
}

impl Error for RequestTerminatedError {}

/// Behavior shared by streaming and non-streaming executor calls.
#[derive(Clone, Default)]
pub struct Options {
    pub stream: bool,
    pub alt: String,
    pub headers: Headers,
    pub query: QueryValues,
    pub original_request: Vec<u8>,
    pub source_format: Format,
    pub response_format: Format,
    pub metadata: ExecutionMetadata,
    pub request_after_auth_interceptor: Option<RequestAfterAuthInterceptor>,
    pub execution_lifecycle: Option<Arc<dyn ExecutionLifecycle>>,
    pub transport_context: ExecutionTransportContext,
}

impl fmt::Debug for Options {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Options")
            .field("stream", &self.stream)
            .field("alt", &self.alt)
            .field("headers", &self.headers)
            .field("query", &self.query)
            .field("original_request_len", &self.original_request.len())
            .field("source_format", &self.source_format)
            .field("response_format", &self.response_format)
            .field("metadata", &self.metadata)
            .field(
                "request_after_auth_interceptor",
                &self.request_after_auth_interceptor.is_some(),
            )
            .field("execution_lifecycle", &self.execution_lifecycle.is_some())
            .field("transport_context", &self.transport_context)
            .finish()
    }
}

#[must_use]
pub fn response_format_or_source(options: &Options) -> Format {
    if options.response_format.as_str().is_empty() {
        options.source_format.clone()
    } else {
        options.response_format.clone()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Response {
    pub payload: Vec<u8>,
    pub metadata: JsonMetadata,
    pub headers: Headers,
}

pub struct StreamChunk {
    pub payload: Vec<u8>,
    pub error: Option<ExecutionError>,
}

impl fmt::Debug for StreamChunk {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StreamChunk")
            .field("payload_len", &self.payload.len())
            .field("error", &self.error.as_ref().map(ToString::to_string))
            .finish()
    }
}

pub struct StreamResult {
    pub headers: Headers,
    pub chunks: mpsc::Receiver<StreamChunk>,
}

/// HTTP-like status carried by a provider or request-policy error.
pub trait StatusError: Error {
    fn status_code(&self) -> u16;
}

/// Failure tied to the request rather than the selected credential.
pub trait RequestScopedError: Error {
    fn is_request_scoped(&self) -> bool;
}

impl StatusError for RequestTerminatedError {
    fn status_code(&self) -> u16 {
        RequestTerminatedError::status_code(self)
    }
}
