// ref: internal/runtime/executor/claude_executor.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::io::Read;
use std::pin::Pin;
use std::time::Duration;

use crate::sdk::cliproxy::executor::{Headers, RequestScopedError};

use crate::internal::auth::claude::SecretString;
use crate::internal::translator::common::SseDecoder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;
use zeroize::Zeroizing;

const ANTHROPIC_SCHEME: &str = "https";
const ANTHROPIC_AUTHORITY: &str = "api.anthropic.com";

/// OAuth cancellation is request-scoped: a dropped downstream caller must not
/// rotate or cool the selected subscription credential.
#[derive(Debug)]
pub struct ClaudeOAuthCancellationError {
    cause: std::io::Error,
}

impl ClaudeOAuthCancellationError {
    pub fn from_cancelled(oauth: bool, cancelled: bool) -> Option<Self> {
        (oauth && cancelled).then(|| Self {
            cause: std::io::Error::new(std::io::ErrorKind::Interrupted, "request cancelled"),
        })
    }
}

impl fmt::Display for ClaudeOAuthCancellationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cause.fmt(formatter)
    }
}
impl std::error::Error for ClaudeOAuthCancellationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
impl RequestScopedError for ClaudeOAuthCancellationError {
    fn is_request_scoped(&self) -> bool {
        true
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ClaudeDeviceProfile {
    user_agent: String,
    package_version: String,
    runtime_version: String,
    os: String,
    arch: String,
}

impl ClaudeDeviceProfile {
    pub fn new(
        user_agent: impl Into<String>,
        package_version: impl Into<String>,
        runtime_version: impl Into<String>,
        os: impl Into<String>,
        arch: impl Into<String>,
    ) -> Result<Self, ClaudeTargetError> {
        let profile = Self {
            user_agent: user_agent.into(),
            package_version: package_version.into(),
            runtime_version: runtime_version.into(),
            os: os.into(),
            arch: arch.into(),
        };
        if [
            profile.user_agent.as_str(),
            profile.package_version.as_str(),
            profile.runtime_version.as_str(),
            profile.os.as_str(),
            profile.arch.as_str(),
        ]
        .into_iter()
        .any(|value| !is_safe_header_value(value))
        {
            return Err(ClaudeTargetError::InvalidFingerprint);
        }
        Ok(profile)
    }

    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    pub fn package_version(&self) -> &str {
        &self.package_version
    }

    pub fn runtime_version(&self) -> &str {
        &self.runtime_version
    }

    pub fn os(&self) -> &str {
        &self.os
    }

    pub fn arch(&self) -> &str {
        &self.arch
    }
}

impl Default for ClaudeDeviceProfile {
    fn default() -> Self {
        Self {
            user_agent: "claude-cli/2.1.280 (external, cli)".to_owned(),
            package_version: "0.112.1".to_owned(),
            runtime_version: "v26.3.0".to_owned(),
            os: "MacOS".to_owned(),
            arch: "arm64".to_owned(),
        }
    }
}

impl fmt::Debug for ClaudeDeviceProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeDeviceProfile")
            .field("user_agent", &self.user_agent)
            .field("package_version", &self.package_version)
            .field("runtime_version", &self.runtime_version)
            .field("os", &self.os)
            .field("arch", &self.arch)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ClaudeRequestFingerprint {
    session_id: String,
    client_request_id: String,
    device: ClaudeDeviceProfile,
}

impl ClaudeRequestFingerprint {
    fn for_session(session_id: String, device: ClaudeDeviceProfile) -> Self {
        Self {
            session_id,
            client_request_id: Uuid::new_v4().to_string(),
            device,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn client_request_id(&self) -> &str {
        &self.client_request_id
    }

    pub fn device(&self) -> &ClaudeDeviceProfile {
        &self.device
    }
}

impl fmt::Debug for ClaudeRequestFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeRequestFingerprint")
            .field("session_id", &"[REDACTED]")
            .field("client_request_id", &"[REDACTED]")
            .field("device", &self.device)
            .finish()
    }
}

fn is_safe_header_value(value: &str) -> bool {
    !value.trim().is_empty()
        && value
            .bytes()
            .all(|byte| byte == b'\t' || (0x20..=0x7e).contains(&byte))
}

/// Typed replacement for upstream's implicit `Attributes["api_key"]`
/// distinction. CTOX decides credential mode in configuration, never by
/// inspecting token text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeCredentialMode {
    ApiKey,
    OAuth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeUpstreamTarget {
    scheme: String,
    authority: String,
}

impl ClaudeUpstreamTarget {
    pub fn new(
        scheme: impl Into<String>,
        authority: impl Into<String>,
    ) -> Result<Self, ClaudeTargetError> {
        let scheme = scheme.into();
        let authority = authority.into();
        if scheme.trim().is_empty() || authority.trim().is_empty() {
            return Err(ClaudeTargetError::Empty);
        }
        if !is_valid_scheme(&scheme) || !authority.bytes().all(is_authority_byte) {
            return Err(ClaudeTargetError::Invalid);
        }
        Ok(Self { scheme, authority })
    }

    pub fn is_anthropic_api(&self) -> bool {
        self.scheme.eq_ignore_ascii_case(ANTHROPIC_SCHEME)
            && self.authority.eq_ignore_ascii_case(ANTHROPIC_AUTHORITY)
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn authority(&self) -> &str {
        &self.authority
    }
}

fn is_valid_scheme(scheme: &str) -> bool {
    let mut bytes = scheme.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

fn is_authority_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeAuthorizationHeader {
    Authorization,
    XApiKey,
}

impl ClaudeAuthorizationHeader {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Authorization => "Authorization",
            Self::XApiKey => "x-api-key",
        }
    }
}

/// Mutually exclusive authorization mutation for one Claude request.
///
/// The value remains a redacted, zeroizing secret. Only the concrete HTTP
/// transport is expected to call `expose_header_value`.
#[derive(Clone, PartialEq, Eq)]
pub struct ClaudePreparedAuthorization {
    set_header: ClaudeAuthorizationHeader,
    remove_header: ClaudeAuthorizationHeader,
    value: SecretString,
}

impl ClaudePreparedAuthorization {
    /// ref: internal/runtime/executor/claude_executor.go:207-230
    pub fn prepare(
        target: &ClaudeUpstreamTarget,
        mode: ClaudeCredentialMode,
        credential: &SecretString,
    ) -> Result<Self, ClaudeTargetError> {
        if target.is_anthropic_api() && mode == ClaudeCredentialMode::ApiKey {
            return Ok(Self {
                set_header: ClaudeAuthorizationHeader::XApiKey,
                remove_header: ClaudeAuthorizationHeader::Authorization,
                value: credential.clone(),
            });
        }

        let bearer = format!("Bearer {}", credential.expose_secret());
        Ok(Self {
            set_header: ClaudeAuthorizationHeader::Authorization,
            remove_header: ClaudeAuthorizationHeader::XApiKey,
            value: SecretString::new(bearer).map_err(|_| ClaudeTargetError::InvalidCredential)?,
        })
    }

    pub fn set_header(&self) -> ClaudeAuthorizationHeader {
        self.set_header
    }

    pub fn remove_header(&self) -> ClaudeAuthorizationHeader {
        self.remove_header
    }

    pub fn expose_header_value(&self) -> &str {
        self.value.expose_secret()
    }
}

impl fmt::Debug for ClaudePreparedAuthorization {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudePreparedAuthorization")
            .field("set_header", &self.set_header)
            .field("remove_header", &self.remove_header)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeTargetError {
    Empty,
    Invalid,
    InvalidCredential,
    InvalidFingerprint,
}

impl fmt::Display for ClaudeTargetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("Claude upstream target is empty"),
            Self::Invalid => formatter.write_str("Claude upstream target is invalid"),
            Self::InvalidCredential => formatter.write_str("Claude credential is invalid"),
            Self::InvalidFingerprint => {
                formatter.write_str("Claude request fingerprint is invalid")
            }
        }
    }
}

impl std::error::Error for ClaudeTargetError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
}

pub fn parse_claude_usage(body: &[u8]) -> Option<ClaudeUsage> {
    let root = serde_json::from_slice::<Value>(body).ok()?;
    root.get("usage").filter(|usage| usage.is_object())?;
    Some(claude_usage_from_detail(super::helps::parse_claude_usage(
        body,
    )))
}

pub fn parse_claude_stream_usage_line(line: &[u8]) -> Option<ClaudeUsage> {
    let data = line
        .strip_prefix(b"data:")
        .map(trim_ascii_bytes)
        .filter(|data| !data.is_empty() && *data != b"[DONE]")?;
    Some(claude_usage_from_detail(
        super::helps::parse_claude_stream_usage(data)?,
    ))
}

fn claude_usage_from_detail(detail: crate::sdk::cliproxy::usage::Detail) -> ClaudeUsage {
    ClaudeUsage {
        input_tokens: detail.input_tokens,
        output_tokens: detail.output_tokens,
        cached_tokens: detail.cached_tokens,
        cache_read_tokens: detail.cache_read_tokens,
        cache_creation_tokens: detail.cache_creation_tokens,
        total_tokens: detail.total_tokens,
    }
}

fn trim_ascii_bytes(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}

pub trait ClaudeUsageSink: Send + Sync {
    fn publish(&self, model: Option<&str>, usage: ClaudeUsage);
}

#[derive(Clone)]
pub struct ClaudeMessagesRequest {
    target: ClaudeUpstreamTarget,
    mode: ClaudeCredentialMode,
    authorization: ClaudePreparedAuthorization,
    body: Zeroizing<Vec<u8>>,
    stream: bool,
    fingerprint: ClaudeRequestFingerprint,
    betas: Vec<String>,
    tool_name_reverse_map: HashMap<String, String>,
}

impl ClaudeMessagesRequest {
    pub fn new(
        target: ClaudeUpstreamTarget,
        mode: ClaudeCredentialMode,
        credential: &SecretString,
        body: Vec<u8>,
        stream: bool,
    ) -> Result<Self, ClaudeTargetError> {
        Self::new_with_session(
            target,
            mode,
            credential,
            body,
            stream,
            Uuid::new_v4().to_string(),
        )
    }

    pub fn new_with_session(
        target: ClaudeUpstreamTarget,
        mode: ClaudeCredentialMode,
        credential: &SecretString,
        body: Vec<u8>,
        stream: bool,
        session_id: impl Into<String>,
    ) -> Result<Self, ClaudeTargetError> {
        let authorization = ClaudePreparedAuthorization::prepare(&target, mode, credential)?;
        let session_id = session_id.into();
        if !is_safe_header_value(&session_id) {
            return Err(ClaudeTargetError::InvalidFingerprint);
        }
        let fingerprint =
            ClaudeRequestFingerprint::for_session(session_id, ClaudeDeviceProfile::default());
        Ok(Self {
            target,
            mode,
            authorization,
            body: Zeroizing::new(body),
            stream,
            fingerprint,
            betas: Vec::new(),
            tool_name_reverse_map: HashMap::new(),
        })
    }

    pub fn target(&self) -> &ClaudeUpstreamTarget {
        &self.target
    }

    pub fn authorization(&self) -> &ClaudePreparedAuthorization {
        &self.authorization
    }

    pub fn mode(&self) -> ClaudeCredentialMode {
        self.mode
    }

    pub fn body(&self) -> &[u8] {
        self.body.as_slice()
    }

    pub fn stream(&self) -> bool {
        self.stream
    }

    pub fn fingerprint(&self) -> &ClaudeRequestFingerprint {
        &self.fingerprint
    }

    pub fn betas(&self) -> &[String] {
        &self.betas
    }

    pub fn tool_name_reverse_map(&self) -> &HashMap<String, String> {
        &self.tool_name_reverse_map
    }

    pub fn with_upstream_metadata(
        mut self,
        betas: Vec<String>,
        tool_name_reverse_map: HashMap<String, String>,
    ) -> Self {
        self.betas = betas;
        self.tool_name_reverse_map = tool_name_reverse_map;
        self
    }

    pub fn client_request_id_for_target(&self) -> Option<&str> {
        self.target
            .is_anthropic_api()
            .then(|| self.fingerprint.client_request_id())
    }

    pub fn with_device_profile(
        mut self,
        device: ClaudeDeviceProfile,
    ) -> Result<Self, ClaudeTargetError> {
        self.fingerprint.device = device;
        Ok(self)
    }

    pub fn endpoint(&self) -> String {
        format!(
            "{}://{}/v1/messages?beta=true",
            self.target.scheme, self.target.authority
        )
    }

    pub fn retry_with_credential(
        &self,
        credential: &SecretString,
    ) -> Result<Self, ClaudeTargetError> {
        let mut retry = Self::new_with_session(
            self.target.clone(),
            self.mode,
            credential,
            self.body.to_vec(),
            self.stream,
            self.fingerprint.session_id.clone(),
        )?;
        retry.fingerprint.device = self.fingerprint.device.clone();
        retry.betas = self.betas.clone();
        retry.tool_name_reverse_map = self.tool_name_reverse_map.clone();
        Ok(retry)
    }
}

impl fmt::Debug for ClaudeMessagesRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeMessagesRequest")
            .field("target", &self.target)
            .field("mode", &self.mode)
            .field("authorization", &self.authorization)
            .field("body", &"[REDACTED]")
            .field("stream", &self.stream)
            .field("fingerprint", &self.fingerprint)
            .finish()
    }
}

pub struct ClaudeMessagesResponse {
    status: u16,
    retry_after: Option<Duration>,
    headers: Headers,
    body: Zeroizing<Vec<u8>>,
}

impl ClaudeMessagesResponse {
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            retry_after: None,
            headers: Headers::new(),
            body: Zeroizing::new(body),
        }
    }

    pub fn with_retry_after(mut self, retry_after: Option<Duration>) -> Self {
        self.retry_after = retry_after;
        self
    }

    pub fn with_headers(mut self, headers: Headers) -> Self {
        self.headers = headers;
        self
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn body(&self) -> &[u8] {
        self.body.as_slice()
    }

    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    pub fn retry_after(&self) -> Option<Duration> {
        self.retry_after
    }

    pub(crate) fn map_body(mut self, map: impl FnOnce(&[u8]) -> Vec<u8>) -> Self {
        self.body = Zeroizing::new(map(self.body.as_slice()));
        self
    }
}

impl fmt::Debug for ClaudeMessagesResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeMessagesResponse")
            .field("status", &self.status)
            .field("retry_after", &self.retry_after)
            .field("headers", &self.headers)
            .field("body", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeMessagesTransportFailure {
    Timeout,
    Connect,
    Protocol,
    Cancelled,
    ResponseDecode(ClaudeResponseEncoding),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeResponseEncoding {
    Gzip,
    Deflate,
    Brotli,
    Zstd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeResponseDecodeError {
    pub encoding: ClaudeResponseEncoding,
    pub message: String,
}

impl fmt::Display for ClaudeResponseDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "failed to decode Claude {:?} response: {}",
            self.encoding, self.message
        )
    }
}

impl std::error::Error for ClaudeResponseDecodeError {}

pub fn decode_claude_response_body(
    body: &[u8],
    content_encoding: Option<&str>,
) -> Result<Vec<u8>, ClaudeResponseDecodeError> {
    let encoding = content_encoding
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .find_map(parse_response_encoding)
        .or_else(|| detect_response_encoding(body));
    let Some(encoding) = encoding else {
        return Ok(body.to_vec());
    };
    let decoded = match encoding {
        ClaudeResponseEncoding::Gzip => read_decoder(flate2::read::GzDecoder::new(body)),
        ClaudeResponseEncoding::Deflate => read_decoder(flate2::read::DeflateDecoder::new(body)),
        ClaudeResponseEncoding::Brotli => read_decoder(brotli::Decompressor::new(body, 4096)),
        ClaudeResponseEncoding::Zstd => zstd::stream::decode_all(body),
    };
    decoded.map_err(|error| ClaudeResponseDecodeError {
        encoding,
        message: error.to_string(),
    })
}

fn read_decoder(mut decoder: impl Read) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    decoder.read_to_end(&mut output)?;
    Ok(output)
}

fn parse_response_encoding(value: &str) -> Option<ClaudeResponseEncoding> {
    match value.to_ascii_lowercase().as_str() {
        "gzip" => Some(ClaudeResponseEncoding::Gzip),
        "deflate" => Some(ClaudeResponseEncoding::Deflate),
        "br" => Some(ClaudeResponseEncoding::Brotli),
        "zstd" => Some(ClaudeResponseEncoding::Zstd),
        _ => None,
    }
}

fn detect_response_encoding(body: &[u8]) -> Option<ClaudeResponseEncoding> {
    if body.starts_with(&[0x1f, 0x8b]) {
        Some(ClaudeResponseEncoding::Gzip)
    } else if body.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        Some(ClaudeResponseEncoding::Zstd)
    } else {
        None
    }
}

pub trait ClaudeMessagesTransport: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: &'a ClaudeMessagesRequest,
        timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ClaudeMessagesResponse, ClaudeMessagesTransportFailure>>
                + Send
                + 'a,
        >,
    >;

    /// Executes the first-party token-counting endpoint with the same
    /// account-scoped TLS/session authority as Messages. Implementations that
    /// do not own a conforming Anthropic transport fail closed.
    fn execute_count_tokens<'a>(
        &'a self,
        _request: &'a ClaudeMessagesRequest,
        _timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ClaudeMessagesResponse, ClaudeMessagesTransportFailure>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async { Err(ClaudeMessagesTransportFailure::Protocol) })
    }
}

pub trait ClaudeMessagesStreamingTransport: Send + Sync {
    fn execute_stream<'a>(
        &'a self,
        request: &'a ClaudeMessagesRequest,
        timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<ClaudeMessagesStreamResponse, ClaudeMessagesTransportFailure>,
                > + Send
                + 'a,
        >,
    >;
}

const MAX_STREAM_BOOTSTRAP_BYTES: usize = 1024 * 1024;

/// HTTP status plus a cancel-on-drop Claude body stream.
///
/// The bootstrap buffer is populated only after a complete `message_start`
/// event has arrived. Callers can therefore decide account failover and HTTP
/// status before making a downstream SSE response visible.
pub struct ClaudeMessagesStreamResponse {
    status: u16,
    retry_after: Option<Duration>,
    headers: Headers,
    error_body: Zeroizing<Vec<u8>>,
    bootstrap: Option<Vec<u8>>,
    chunks: mpsc::Receiver<Result<Vec<u8>, ClaudeMessagesTransportFailure>>,
}

impl ClaudeMessagesStreamResponse {
    pub fn new(
        status: u16,
        retry_after: Option<Duration>,
        chunks: mpsc::Receiver<Result<Vec<u8>, ClaudeMessagesTransportFailure>>,
    ) -> Self {
        Self {
            status,
            retry_after,
            headers: Headers::new(),
            error_body: Zeroizing::new(Vec::new()),
            bootstrap: None,
            chunks,
        }
    }

    pub fn with_headers(mut self, headers: Headers) -> Self {
        self.headers = headers;
        self
    }

    pub fn with_error_body(mut self, body: Vec<u8>) -> Self {
        self.error_body = Zeroizing::new(body);
        self
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn retry_after(&self) -> Option<Duration> {
        self.retry_after
    }

    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    pub fn error_body(&self) -> &[u8] {
        self.error_body.as_slice()
    }

    pub async fn next_chunk(&mut self) -> Option<Result<Vec<u8>, ClaudeMessagesTransportFailure>> {
        if let Some(bytes) = self.bootstrap.take() {
            return Some(Ok(bytes));
        }
        self.chunks.recv().await
    }

    pub(crate) async fn bootstrap_message_start(
        &mut self,
    ) -> Result<(), ClaudeMessagesTransportFailure> {
        if self.bootstrap.is_some() || !(200..300).contains(&self.status) {
            return Ok(());
        }
        let mut pending = Vec::new();
        loop {
            match self.chunks.recv().await {
                Some(Ok(chunk)) => {
                    pending.extend_from_slice(&chunk);
                    if pending.len() > MAX_STREAM_BOOTSTRAP_BYTES {
                        return Err(ClaudeMessagesTransportFailure::Protocol);
                    }
                    match inspect_bootstrap_events(&pending, false) {
                        BootstrapInspection::Pending => {}
                        BootstrapInspection::Started => {
                            self.bootstrap = Some(pending);
                            return Ok(());
                        }
                        BootstrapInspection::Failed => {
                            return Err(ClaudeMessagesTransportFailure::Protocol)
                        }
                    }
                }
                Some(Err(error)) => return Err(error),
                None => {
                    return match inspect_bootstrap_events(&pending, true) {
                        BootstrapInspection::Started => {
                            self.bootstrap = Some(pending);
                            Ok(())
                        }
                        BootstrapInspection::Pending | BootstrapInspection::Failed => {
                            Err(ClaudeMessagesTransportFailure::Protocol)
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn synthetic(status: u16) -> Self {
        let (_sender, receiver) = mpsc::channel(1);
        Self::new(status, None, receiver)
    }
}

impl fmt::Debug for ClaudeMessagesStreamResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeMessagesStreamResponse")
            .field("status", &self.status)
            .field("retry_after", &self.retry_after)
            .field("headers", &self.headers)
            .field("error_body", &"[REDACTED]")
            .field("bootstrap", &self.bootstrap.as_ref().map(|_| "buffered"))
            .field("chunks", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BootstrapInspection {
    Pending,
    Started,
    Failed,
}

fn inspect_bootstrap_events(bytes: &[u8], finish: bool) -> BootstrapInspection {
    let mut decoder = SseDecoder::new();
    let mut events = decoder.push(bytes);
    if finish {
        events.extend(decoder.finish());
    }
    for event in events {
        let Ok(value) = serde_json::from_slice::<Value>(&event.data) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("message_start") => return BootstrapInspection::Started,
            Some("error") => return BootstrapInspection::Failed,
            _ => {}
        }
    }
    BootstrapInspection::Pending
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn target(scheme: &str, authority: &str) -> ClaudeUpstreamTarget {
        ClaudeUpstreamTarget::new(scheme, authority).unwrap()
    }

    fn credential() -> SecretString {
        SecretString::new("credential-do-not-leak").unwrap()
    }

    #[tokio::test]
    async fn stream_bootstrap_waits_for_fragmented_message_start() {
        let (sender, receiver) = mpsc::channel(4);
        sender
            .send(Ok(b"data: {\"type\":\"message_sta".to_vec()))
            .await
            .unwrap();
        sender
            .send(Ok(b"rt\",\"message\":{\"id\":\"msg_1\"}}\n\n".to_vec()))
            .await
            .unwrap();
        sender
            .send(Ok(b"data: {\"type\":\"message_stop\"}\n\n".to_vec()))
            .await
            .unwrap();
        drop(sender);

        let mut response = ClaudeMessagesStreamResponse::new(200, None, receiver);
        response.bootstrap_message_start().await.unwrap();
        let bootstrap = response.next_chunk().await.unwrap().unwrap();
        assert!(String::from_utf8_lossy(&bootstrap).contains("message_start"));
        assert_eq!(
            response.next_chunk().await.unwrap().unwrap(),
            b"data: {\"type\":\"message_stop\"}\n\n"
        );
    }

    #[tokio::test]
    async fn stream_bootstrap_rejects_provider_error_before_message_start() {
        let (sender, receiver) = mpsc::channel(1);
        sender
            .send(Ok(
                b"data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\n\n"
                    .to_vec(),
            ))
            .await
            .unwrap();
        drop(sender);
        let mut response = ClaudeMessagesStreamResponse::new(200, None, receiver);
        assert_eq!(
            response.bootstrap_message_start().await,
            Err(ClaudeMessagesTransportFailure::Protocol)
        );
    }

    #[test]
    fn api_key_uses_x_api_key_only_on_exact_anthropic_https_origin() {
        let prepared = ClaudePreparedAuthorization::prepare(
            &target("HTTPS", "API.ANTHROPIC.COM"),
            ClaudeCredentialMode::ApiKey,
            &credential(),
        )
        .unwrap();
        assert_eq!(prepared.set_header(), ClaudeAuthorizationHeader::XApiKey);
        assert_eq!(
            prepared.remove_header(),
            ClaudeAuthorizationHeader::Authorization
        );
        assert_eq!(prepared.expose_header_value(), "credential-do-not-leak");
    }

    #[test]
    fn api_key_uses_bearer_for_custom_or_non_https_origin() {
        for target in [
            target("https", "gateway.example.com"),
            target("http", "api.anthropic.com"),
            target("https", "api.anthropic.com:443"),
        ] {
            let prepared = ClaudePreparedAuthorization::prepare(
                &target,
                ClaudeCredentialMode::ApiKey,
                &credential(),
            )
            .unwrap();
            assert_eq!(
                prepared.set_header(),
                ClaudeAuthorizationHeader::Authorization
            );
            assert_eq!(prepared.remove_header(), ClaudeAuthorizationHeader::XApiKey);
            assert_eq!(
                prepared.expose_header_value(),
                "Bearer credential-do-not-leak"
            );
        }
    }

    #[test]
    fn oauth_always_uses_bearer_and_debug_is_redacted() {
        let prepared = ClaudePreparedAuthorization::prepare(
            &target("https", "api.anthropic.com"),
            ClaudeCredentialMode::OAuth,
            &credential(),
        )
        .unwrap();
        assert_eq!(
            prepared.set_header(),
            ClaudeAuthorizationHeader::Authorization
        );
        let rendered = format!("{prepared:?}");
        assert!(!rendered.contains("credential-do-not-leak"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn invalid_targets_are_rejected_before_header_creation() {
        assert_eq!(
            ClaudeUpstreamTarget::new("", "api.anthropic.com"),
            Err(ClaudeTargetError::Empty)
        );
        assert_eq!(
            ClaudeUpstreamTarget::new("https", "api.anthropic.com\r\nInjected: yes"),
            Err(ClaudeTargetError::Invalid)
        );
    }

    #[test]
    fn messages_envelope_owns_zeroizing_body_and_exact_endpoint() {
        let request = ClaudeMessagesRequest::new(
            target("https", "api.anthropic.com"),
            ClaudeCredentialMode::OAuth,
            &credential(),
            br#"{"model":"claude-sonnet-4-6"}"#.to_vec(),
            true,
        )
        .unwrap();
        assert_eq!(
            request.endpoint(),
            "https://api.anthropic.com/v1/messages?beta=true"
        );
        assert!(request.stream());
        let rendered = format!("{request:?}");
        assert!(!rendered.contains("claude-sonnet-4-6"));
        assert!(!rendered.contains("credential-do-not-leak"));
    }

    #[test]
    fn fingerprint_keeps_session_stable_and_request_id_unique() {
        let session_id = Uuid::new_v4().to_string();
        let first = ClaudeMessagesRequest::new_with_session(
            target("https", "api.anthropic.com"),
            ClaudeCredentialMode::OAuth,
            &credential(),
            b"{}".to_vec(),
            false,
            session_id.clone(),
        )
        .unwrap();
        let second = ClaudeMessagesRequest::new_with_session(
            target("https", "api.anthropic.com"),
            ClaudeCredentialMode::OAuth,
            &credential(),
            b"{}".to_vec(),
            false,
            session_id,
        )
        .unwrap();
        assert_eq!(
            first.fingerprint().session_id(),
            second.fingerprint().session_id()
        );
        assert_ne!(
            first.fingerprint().client_request_id(),
            second.fingerprint().client_request_id()
        );
        assert!(Uuid::parse_str(first.fingerprint().session_id()).is_ok());
        assert!(Uuid::parse_str(first.fingerprint().client_request_id()).is_ok());
        assert_eq!(
            first.client_request_id_for_target(),
            Some(first.fingerprint().client_request_id())
        );
        let custom = ClaudeMessagesRequest::new(
            target("https", "gateway.example.com"),
            ClaudeCredentialMode::OAuth,
            &credential(),
            b"{}".to_vec(),
            false,
        )
        .unwrap();
        assert_eq!(custom.client_request_id_for_target(), None);
        let rendered = format!("{:?}", first.fingerprint());
        assert!(!rendered.contains(first.fingerprint().session_id()));
        assert!(!rendered.contains(first.fingerprint().client_request_id()));
    }

    #[test]
    fn device_profile_rejects_header_injection() {
        assert_eq!(
            ClaudeDeviceProfile::new(
                "claude-cli/2.1.63\r\nX-Evil: yes",
                "0.74.0",
                "v24.3.0",
                "MacOS",
                "arm64"
            ),
            Err(ClaudeTargetError::InvalidFingerprint)
        );
    }

    #[test]
    fn device_profile_default_matches_candidate_capture() {
        let profile = ClaudeDeviceProfile::default();
        assert_eq!(profile.user_agent(), "claude-cli/2.1.280 (external, cli)");
        assert_eq!(profile.package_version(), "0.112.1");
        assert_eq!(profile.runtime_version(), "v26.3.0");
        assert_eq!(profile.os(), "MacOS");
        assert_eq!(profile.arch(), "arm64");
    }

    #[test]
    fn claude_usage_matches_independent_cache_accounting() {
        let usage = parse_claude_usage(
            br#"{"usage":{"input_tokens":10,"output_tokens":4,"cache_read_input_tokens":3,"cache_creation_input_tokens":2}}"#,
        )
        .unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.cached_tokens, 3);
        assert_eq!(usage.total_tokens, 19);
        let stream = parse_claude_stream_usage_line(
            b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":7}}\n",
        )
        .unwrap();
        assert_eq!(stream.output_tokens, 7);
        assert_eq!(stream.total_tokens, 7);
    }

    #[test]
    fn response_decode_supports_headers_magic_and_typed_failures() {
        let plain = b"data: {\"type\":\"message_start\"}\n\n";
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(plain).unwrap();
        let gzip = gzip.finish().unwrap();
        assert_eq!(decode_claude_response_body(&gzip, None).unwrap(), plain);
        assert_eq!(
            decode_claude_response_body(&gzip, Some("gzip")).unwrap(),
            plain
        );
        let zstd = zstd::stream::encode_all(plain.as_slice(), 1).unwrap();
        assert_eq!(decode_claude_response_body(&zstd, None).unwrap(), plain);
        assert_eq!(decode_claude_response_body(plain, None).unwrap(), plain);
        assert_eq!(
            decode_claude_response_body(b"not-gzip", Some("gzip"))
                .unwrap_err()
                .encoding,
            ClaudeResponseEncoding::Gzip
        );
    }
}
