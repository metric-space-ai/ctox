// Origin: CTOX
// SPDX-License-Identifier: MIT OR AGPL-3.0-only

//! Host HTTP client for API-key provider accounts.
//!
//! The OAuth providers each carry a bespoke transport with a browser
//! fingerprint, because their upstreams inspect one. The API-key providers
//! (see [`crate::internal::config::API_KEY_PROVIDERS`]) are ordinary
//! OpenAI-compatible HTTP endpoints, so this transport stays deliberately
//! plain: no emulation profile, no redirects, no retry, no ambient proxy.
//!
//! The transport NEVER adds an authorization header of its own and never
//! copies one in from anywhere. The single credential on the wire is the one
//! the OpenAI-compat executor sets from the pool's resolved secret.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyTransportBuildError {
    InvalidProxy,
    Client,
}

impl std::fmt::Display for ApiKeyTransportBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("api-key provider transport could not be built")
    }
}

impl std::error::Error for ApiKeyTransportBuildError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiKeyTransportFailure;

impl std::fmt::Display for ApiKeyTransportFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("api-key provider upstream request failed")
    }
}

impl std::error::Error for ApiKeyTransportFailure {}

/// Upper bound on a buffered API-key upstream response. A compromised or
/// misbehaving upstream must not be able to exhaust host memory.
pub const API_KEY_MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

#[cfg(feature = "api-key-http-transport")]
mod native {
    use std::sync::Arc;
    use std::time::Duration;

    use futures_util::StreamExt;
    use tokio::sync::mpsc;
    use wreq::{Client, Proxy};

    use super::{ApiKeyTransportBuildError, ApiKeyTransportFailure, API_KEY_MAX_RESPONSE_BYTES};
    use crate::sdk::pluginapi::{
        HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk, HttpStreamResponse,
        PluginFuture,
    };

    /// One typed transport failure. The upstream's own error text is never
    /// surfaced: it can carry account identifiers.
    fn failure<E>(_: E) -> crate::sdk::pluginapi::PluginExecutionError {
        Arc::new(ApiKeyTransportFailure)
    }

    const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
    const STREAM_CHANNEL_CAPACITY: usize = 16;

    #[derive(Clone)]
    pub struct ApiKeyHttpClient {
        client: Client,
        timeout: Duration,
    }

    impl ApiKeyHttpClient {
        pub fn new(
            proxy_url: Option<&str>,
            timeout: Duration,
        ) -> Result<Self, ApiKeyTransportBuildError> {
            let mut builder = Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .retry(wreq::retry::Policy::never())
                .redirect(wreq::redirect::Policy::none());
            match proxy_url.map(str::trim).filter(|value| !value.is_empty()) {
                Some(proxy_url) => {
                    let proxy = Proxy::all(proxy_url)
                        .map_err(|_| ApiKeyTransportBuildError::InvalidProxy)?;
                    builder = builder.proxy(proxy);
                }
                None => builder = builder.no_proxy(),
            }
            Ok(Self {
                client: builder
                    .build()
                    .map_err(|_| ApiKeyTransportBuildError::Client)?,
                timeout,
            })
        }

        fn outgoing(&self, request: &HttpRequest) -> wreq::RequestBuilder {
            let mut outgoing = self
                .client
                .request(
                    request.method.parse().unwrap_or(wreq::Method::POST),
                    request.url.clone(),
                )
                .timeout(self.timeout)
                .body(request.body.clone());
            for (name, values) in &request.headers {
                for value in values {
                    outgoing = outgoing.header(name.as_str(), value.as_str());
                }
            }
            outgoing
        }
    }

    fn collect_headers(response: &wreq::Response) -> crate::sdk::pluginapi::Headers {
        let mut headers = crate::sdk::pluginapi::Headers::new();
        for (name, value) in response.headers() {
            if let Ok(value) = value.to_str() {
                headers
                    .entry(name.as_str().to_owned())
                    .or_default()
                    .push(value.to_owned());
            }
        }
        headers
    }

    impl HostHttpClient for ApiKeyHttpClient {
        fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
            Box::pin(async move {
                let response = self.outgoing(&request).send().await.map_err(failure)?;
                let status_code = response.status().as_u16();
                let headers = collect_headers(&response);
                let mut stream = response.bytes_stream();
                let mut body = Vec::new();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(failure)?;
                    if body.len().checked_add(chunk.len()).is_none_or(|n| n > API_KEY_MAX_RESPONSE_BYTES) {
                        return Err(failure(()));
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(HttpResponse {
                    status_code,
                    headers,
                    body,
                })
            })
        }

        fn execute_stream<'a>(
            &'a self,
            request: HttpRequest,
        ) -> PluginFuture<'a, HttpStreamResponse> {
            Box::pin(async move {
                let response = self.outgoing(&request).send().await.map_err(failure)?;
                let status_code = response.status().as_u16();
                let headers = collect_headers(&response);
                let (sender, receiver) = mpsc::channel(STREAM_CHANNEL_CAPACITY);
                let mut body = response.bytes_stream();
                tokio::spawn(async move {
                    while let Some(chunk) = body.next().await {
                        let message = match chunk {
                            Ok(bytes) => HttpStreamChunk {
                                payload: bytes.to_vec(),
                                error: None,
                            },
                            Err(_) => HttpStreamChunk {
                                payload: Vec::new(),
                                error: Some(Arc::new(ApiKeyTransportFailure)),
                            },
                        };
                        let terminal = message.error.is_some();
                        if sender.send(message).await.is_err() || terminal {
                            return;
                        }
                    }
                });
                Ok(HttpStreamResponse {
                    status_code,
                    headers,
                    chunks: receiver,
                })
            })
        }
    }
}

#[cfg(feature = "api-key-http-transport")]
pub use native::ApiKeyHttpClient;
