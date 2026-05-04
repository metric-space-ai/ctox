//! Async HTTP client for WhatsApp media upload/download, backed by `reqwest`.
//!
//! `wha-client` defines two transport traits — [`wha_client::download::HttpClient`]
//! for `GET`s and [`wha_client::upload::UploadHttpClient`] for `POST`s — so the
//! protocol crate stays free of a hard dependency on any specific HTTP backend.
//! This crate provides [`ReqwestHttpClient`], which implements **both** traits
//! against `reqwest`.
//!
//! Wiring is direct: the orchestrator constructs a `ReqwestHttpClient` and
//! passes it (as a `&dyn UploadHttpClient` or via a `SharedHttpClient` =
//! `Arc<dyn HttpClient>`) into `Client::upload_media` /
//! `Client::download` / `Client::download_to_file`.
//!
//! Errors surface in two layers: [`HttpError`] is the local, fully-typed error
//! enum used by `ReqwestHttpClient`'s inherent helpers; the trait impls map
//! every [`HttpError`] into [`wha_client::ClientError::Download`] so callers
//! see a single uniform error type.

use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

use wha_client::download::HttpClient;
use wha_client::error::ClientError;
use wha_client::upload::UploadHttpClient;

// Re-export the traits so downstream users importing `wha_http` get one
// canonical name to refer to.
pub use wha_client::download::HttpClient as DownloadHttpClient;
pub use wha_client::upload::UploadHttpClient as UploadClient;

/// Error type produced by the inherent [`ReqwestHttpClient`] helpers.
///
/// The trait implementations convert these into [`ClientError::Download`] so
/// the wider stack only has to know about one error enum.
#[derive(Debug, Error)]
pub enum HttpError {
    /// The URL the caller provided couldn't be parsed by `reqwest`.
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    /// The remote returned a non-2xx HTTP status.
    #[error("http {status}: {body}")]
    Status { status: u16, body: String },
    /// Any other reqwest-level failure (DNS, TLS, body read, timeout, ...).
    #[error("reqwest: {0}")]
    Reqwest(String),
}

impl From<HttpError> for ClientError {
    fn from(value: HttpError) -> Self {
        ClientError::Download(value.to_string())
    }
}

/// Map a `reqwest::Error` to our local [`HttpError`].
///
/// `reqwest::Error` is opaque, so we project the few flags we care about and
/// lose the rest into `Reqwest(stringified)`.
fn map_reqwest_error(err: reqwest::Error) -> HttpError {
    if err.is_builder() {
        return HttpError::InvalidUrl(err.to_string());
    }
    if let Some(status) = err.status() {
        return HttpError::Status {
            status: status.as_u16(),
            body: err.to_string(),
        };
    }
    HttpError::Reqwest(err.to_string())
}

/// `reqwest`-backed implementation of the wha-client HTTP traits.
///
/// Construction never fails: the underlying `reqwest::Client` is built with a
/// 60-second timeout and the workspace's TLS feature
/// (`rustls-tls-native-roots`).
pub struct ReqwestHttpClient {
    client: reqwest::Client,
}

impl ReqwestHttpClient {
    /// Build a new client with sensible defaults.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("reqwest::Client default build cannot fail with the configured features");
        Self { client }
    }

    /// Build from an existing `reqwest::Client`. Useful for sharing connection
    /// pools or installing custom middleware in higher layers.
    pub fn from_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Borrow the underlying `reqwest::Client`.
    pub fn inner(&self) -> &reqwest::Client {
        &self.client
    }

    /// Lower-level GET that returns the raw bytes or [`HttpError`].
    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, HttpError> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(HttpError::Status {
                status: status.as_u16(),
                body,
            });
        }
        let bytes = resp.bytes().await.map_err(map_reqwest_error)?;
        Ok(bytes.to_vec())
    }

    /// Lower-level POST that returns the raw bytes or [`HttpError`].
    pub async fn post_bytes(
        &self,
        url: &str,
        body: Vec<u8>,
        headers: &[(&str, &str)],
    ) -> Result<Vec<u8>, HttpError> {
        let mut req = self.client.post(url).body(body);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = req.send().await.map_err(map_reqwest_error)?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(HttpError::Status {
                status: status.as_u16(),
                body,
            });
        }
        let bytes = resp.bytes().await.map_err(map_reqwest_error)?;
        Ok(bytes.to_vec())
    }
}

impl Default for ReqwestHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpClient for ReqwestHttpClient {
    async fn get(&self, url: &str) -> Result<Vec<u8>, ClientError> {
        self.get_bytes(url).await.map_err(Into::into)
    }
}

#[async_trait]
impl UploadHttpClient for ReqwestHttpClient {
    async fn post(
        &self,
        url: &str,
        body: Vec<u8>,
        headers: &[(&str, &str)],
    ) -> Result<Vec<u8>, ClientError> {
        self.post_bytes(url, body, headers).await.map_err(Into::into)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: constructing the client must not panic.
    #[test]
    fn reqwest_client_constructs() {
        let _client = ReqwestHttpClient::new();
        let _client_default = ReqwestHttpClient::default();
    }

    /// A malformed URL hits the builder path inside reqwest and surfaces as
    /// [`HttpError::InvalidUrl`] (or [`HttpError::Reqwest`] if reqwest decides
    /// to classify it differently). Either way it must NOT be a `Status`
    /// error, and the trait impl must wrap it as `ClientError::Download`.
    #[tokio::test]
    async fn error_mapping() {
        let client = ReqwestHttpClient::new();
        let result = client.get_bytes("not a url at all").await;
        let err = result.expect_err("malformed URL must error");
        match err {
            HttpError::InvalidUrl(_) | HttpError::Reqwest(_) => {}
            HttpError::Status { .. } => panic!("malformed URL should not produce a Status error"),
        }

        // The trait-level call must funnel that into ClientError::Download.
        let trait_err = HttpClient::get(&client, "http://").await.unwrap_err();
        match trait_err {
            ClientError::Download(_) => {}
            other => panic!("expected ClientError::Download, got {other:?}"),
        }
    }

    /// Compile-time check: the struct really does implement both traits as
    /// `dyn`-safe trait objects. If either signature drifts, this stops
    /// compiling.
    #[test]
    fn implements_both_traits_as_dyn() {
        let c = ReqwestHttpClient::new();
        let _g: &dyn HttpClient = &c;
        let _u: &dyn UploadHttpClient = &c;
    }

    /// Network-gated end-to-end GET. Run with `cargo test -p wha-http -- --ignored`.
    #[tokio::test]
    #[ignore = "network test; run with --ignored"]
    async fn real_get_works() {
        let client = ReqwestHttpClient::new();
        let body = client
            .get_bytes("https://example.com/")
            .await
            .expect("GET https://example.com should succeed");
        assert!(!body.is_empty());
    }
}
