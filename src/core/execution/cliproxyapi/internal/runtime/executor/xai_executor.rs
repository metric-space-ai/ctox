// ref: internal/runtime/executor/xai_executor.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tokio::sync::mpsc;
use zeroize::Zeroizing;

use crate::sdk::cliproxy::executor::Headers;

pub const DEFAULT_XAI_API_BASE_URL: &str = "https://api.x.ai/v1";
pub const DEFAULT_XAI_CHAT_BASE_URL: &str = crate::internal::auth::xai::types::CLI_CHAT_PROXY_BASE_URL;
pub const XAI_TOKEN_AUTH_HEADER: &str = "X-XAI-Token-Auth";
pub const XAI_TOKEN_AUTH_VALUE: &str = "xai-grok-cli";
pub const XAI_CLIENT_VERSION_HEADER: &str = "x-grok-client-version";
// ref: internal/runtime/executor/xai_executor.go:51-54 @ e2bff010
// The subscription chat proxy rejects versions older than 1.0.13 with HTTP 426.
pub const XAI_CLIENT_VERSION_VALUE: &str = "1.0.44";
pub const XAI_CLIENT_IDENTIFIER_HEADER: &str = "x-grok-client-identifier";
pub const XAI_CLIENT_IDENTIFIER_VALUE: &str = "grok-shell";
pub const XAI_AUTHENTICATE_RESPONSE_HEADER: &str = "x-authenticateresponse";
pub const XAI_AUTHENTICATE_RESPONSE_VALUE: &str = "authenticate-response";
pub const MAX_XAI_BODY_BYTES: usize = 52_428_800;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XaiUpstreamTarget {
    base_url: String,
}

impl XaiUpstreamTarget {
    pub fn new(raw: &str) -> Result<Self, XaiTargetError> {
        let parsed = url::Url::parse(raw.trim()).map_err(|_| XaiTargetError::InvalidUrl)?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(XaiTargetError::InvalidUrl);
        }
        Ok(Self {
            base_url: parsed.as_str().trim_end_matches('/').to_owned(),
        })
    }

    #[must_use]
    pub fn api_default() -> Self {
        Self {
            base_url: DEFAULT_XAI_API_BASE_URL.to_owned(),
        }
    }

    #[must_use]
    pub fn url(&self, path: &str) -> String {
        format!(
            "{}{}",
            self.base_url,
            if path.starts_with('/') {
                path.to_owned()
            } else {
                format!("/{path}")
            }
        )
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XaiTargetError {
    InvalidUrl,
}

impl fmt::Display for XaiTargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("xAI upstream URL is invalid")
    }
}
impl std::error::Error for XaiTargetError {}

pub struct XaiHttpRequest {
    pub url: String,
    pub headers: Headers,
    pub body: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for XaiHttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XaiHttpRequest")
            .field("url", &self.url)
            .field("headers", &"[REDACTED]")
            .field("body", &"[REDACTED]")
            .finish()
    }
}

pub struct XaiHttpResponse {
    pub status: u16,
    pub headers: Headers,
    pub body: Zeroizing<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XaiTransportFailure {
    Timeout,
    Connect,
    Protocol,
    Cancelled,
    MessageTooBig,
}

pub type XaiTransportFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, XaiTransportFailure>> + Send + 'a>>;

pub trait XaiHttpTransport: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: &'a XaiHttpRequest,
        timeout: Duration,
    ) -> XaiTransportFuture<'a, XaiHttpResponse>;
}

pub struct XaiStreamResponse {
    pub status: u16,
    pub headers: Headers,
    pub chunks: mpsc::Receiver<Result<Vec<u8>, XaiTransportFailure>>,
}

impl XaiStreamResponse {
    pub async fn next_chunk(&mut self) -> Option<Result<Vec<u8>, XaiTransportFailure>> {
        self.chunks.recv().await
    }
}

pub trait XaiStreamingTransport: Send + Sync {
    fn execute_stream<'a>(
        &'a self,
        request: &'a XaiHttpRequest,
        timeout: Duration,
    ) -> XaiTransportFuture<'a, XaiStreamResponse>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XaiStatusError {
    pub status: u16,
    pub message: String,
    pub retry_after: Option<Duration>,
}

impl fmt::Display for XaiStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "xAI upstream status {}: {}", self.status, self.message)
    }
}
impl std::error::Error for XaiStatusError {}

#[must_use]
pub fn xai_status_error(status: u16, body: &[u8]) -> XaiStatusError {
    let value = serde_json::from_slice::<serde_json::Value>(body).ok();
    let message = value
        .as_ref()
        .and_then(|v| {
            v.pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .or_else(|| v.get("error").and_then(serde_json::Value::as_str))
        })
        .unwrap_or_else(|| std::str::from_utf8(body).unwrap_or("xAI request failed"))
        .trim()
        .to_owned();
    let exhausted = status == 429
        && message.to_ascii_lowercase().contains("free")
        && (message.to_ascii_lowercase().contains("usage")
            || message.to_ascii_lowercase().contains("quota"));
    XaiStatusError {
        status,
        message,
        retry_after: exhausted.then_some(Duration::from_secs(24 * 60 * 60)),
    }
}

pub(crate) fn header_set(headers: &mut Headers, name: &str, value: impl Into<String>) {
    if let Some(existing) = headers
        .keys()
        .find(|key| key.eq_ignore_ascii_case(name))
        .cloned()
    {
        headers.remove(&existing);
    }
    headers.insert(name.to_owned(), vec![value.into()]);
}
