// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! [`RetryConfig`] connection retry policy

use crate::PutPayload;
use crate::client::backoff::{Backoff, BackoffConfig};
use crate::client::builder::HttpRequestBuilder;
use crate::client::{HttpClient, HttpError, HttpErrorKind, HttpRequest, HttpResponse};
use futures_util::future::BoxFuture;
use http::{Method, Uri};
use reqwest::StatusCode;
use reqwest::header::LOCATION;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use std::time::{Duration, Instant};
use tracing::info;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use web_time::{Duration, Instant};

/// Retry request error
#[derive(Debug)]
pub struct RetryError(Box<RetryErrorImpl>);

/// Box error to avoid large error variant
#[derive(Debug)]
struct RetryErrorImpl {
    method: Method,
    uri: Option<Uri>,
    retries: usize,
    max_retries: usize,
    elapsed: Duration,
    retry_timeout: Duration,
    inner: RequestError,
}

impl std::fmt::Display for RetryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error performing {} ", self.0.method)?;
        match &self.0.uri {
            Some(uri) => write!(f, "{uri} ")?,
            None => write!(f, "REDACTED ")?,
        }
        write!(f, "in {:?}", self.0.elapsed)?;
        if self.0.retries != 0 {
            write!(
                f,
                ", after {} retries, max_retries: {}, retry_timeout: {:?} ",
                self.0.retries, self.0.max_retries, self.0.retry_timeout
            )?;
        }
        write!(f, " - {}", self.0.inner)
    }
}

impl std::error::Error for RetryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0.inner)
    }
}

/// Context of the retry loop
///
/// Most use-cases should use [`RetryExt`] and [`RetryableRequestBuilder`], however,
/// [`RetryContext`] allows preserving retry state across multiple [`RetryableRequest`]
pub(crate) struct RetryContext {
    backoff: Backoff,
    retries: usize,
    max_retries: usize,
    retry_timeout: Duration,
    start: Instant,
}

impl RetryContext {
    pub(crate) fn new(config: &RetryConfig) -> Self {
        Self {
            max_retries: config.max_retries,
            retry_timeout: config.retry_timeout,
            backoff: Backoff::new(&config.backoff),
            retries: 0,
            start: Instant::now(),
        }
    }

    pub(crate) fn exhausted(&self) -> bool {
        self.retries >= self.max_retries || self.start.elapsed() > self.retry_timeout
    }

    pub(crate) fn backoff(&mut self) -> Duration {
        self.retries += 1;
        self.backoff.next()
    }
}

/// The reason a request failed
#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error(
        "Received redirect without LOCATION, this normally indicates an incorrectly configured region"
    )]
    BareRedirect,

    #[error("Server returned non-2xx status code: {status}: {}", body.as_deref().unwrap_or(""))]
    Status {
        status: StatusCode,
        body: Option<String>,
    },

    #[error("Server returned error response: {body}")]
    Response { status: StatusCode, body: String },

    // We need to use `{0}` here rather than `error(transparent)` to ensure that
    // `Error::source` returns `HttpError` rather than the underlying
    // `reqwest::Error` which would happen with `error(transparent)`.
    #[error("{0}")]
    Http(#[from] HttpError),
}

impl RetryError {
    /// Returns the underlying [`RequestError`]
    pub fn inner(&self) -> &RequestError {
        &self.0.inner
    }

    /// Returns the status code associated with this error if any
    pub fn status(&self) -> Option<StatusCode> {
        match self.inner() {
            RequestError::Status { status, .. } | RequestError::Response { status, .. } => {
                Some(*status)
            }
            RequestError::BareRedirect | RequestError::Http(_) => None,
        }
    }

    /// Returns the error body if any
    pub fn body(&self) -> Option<&str> {
        match self.inner() {
            RequestError::Status { body, .. } => body.as_deref(),
            RequestError::Response { body, .. } => Some(body),
            RequestError::BareRedirect | RequestError::Http(_) => None,
        }
    }

    pub fn error(self, store: &'static str, path: String) -> crate::Error {
        match self.status() {
            Some(StatusCode::NOT_FOUND) => crate::Error::NotFound {
                path,
                source: Box::new(self),
            },
            Some(StatusCode::NOT_MODIFIED) => crate::Error::NotModified {
                path,
                source: Box::new(self),
            },
            Some(StatusCode::PRECONDITION_FAILED) => crate::Error::Precondition {
                path,
                source: Box::new(self),
            },
            Some(StatusCode::CONFLICT) => crate::Error::AlreadyExists {
                path,
                source: Box::new(self),
            },
            Some(StatusCode::FORBIDDEN) => crate::Error::PermissionDenied {
                path,
                source: Box::new(self),
            },
            Some(StatusCode::UNAUTHORIZED) => crate::Error::Unauthenticated {
                path,
                source: Box::new(self),
            },
            _ => crate::Error::Generic {
                store,
                source: Box::new(self),
            },
        }
    }
}

impl From<RetryError> for std::io::Error {
    fn from(err: RetryError) -> Self {
        use std::io::ErrorKind;
        let kind = match err.status() {
            Some(StatusCode::NOT_FOUND) => ErrorKind::NotFound,
            Some(StatusCode::BAD_REQUEST) => ErrorKind::InvalidInput,
            Some(StatusCode::UNAUTHORIZED) | Some(StatusCode::FORBIDDEN) => {
                ErrorKind::PermissionDenied
            }
            _ => match err.inner() {
                RequestError::Http(h) => match h.kind() {
                    HttpErrorKind::Timeout => ErrorKind::TimedOut,
                    HttpErrorKind::Connect => ErrorKind::NotConnected,
                    _ => ErrorKind::Other,
                },
                _ => ErrorKind::Other,
            },
        };
        Self::new(kind, err)
    }
}

pub(crate) type Result<T, E = RetryError> = std::result::Result<T, E>;

/// The configuration for how to respond to request errors
///
/// The following categories of error will be retried:
///
/// * 5xx server errors
/// * Connection errors
/// * Dropped connections
/// * Timeouts for [safe] / read-only requests
///
/// Requests will be retried up to some limit, using exponential
/// backoff with jitter. See [`BackoffConfig`] for more information
///
/// [safe]: https://datatracker.ietf.org/doc/html/rfc7231#section-4.2.1
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// The backoff configuration
    pub backoff: BackoffConfig,

    /// The maximum number of times to retry a request
    ///
    /// Set to 0 to disable retries
    pub max_retries: usize,

    /// The maximum length of time from the initial request
    /// after which no further retries will be attempted
    ///
    /// This not only bounds the length of time before a server
    /// error will be surfaced to the application, but also bounds
    /// the length of time a request's credentials must remain valid.
    ///
    /// As requests are retried without renewing credentials or
    /// regenerating request payloads, this number should be kept
    /// below 5 minutes to avoid errors due to expired credentials
    /// and/or request payloads
    pub retry_timeout: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            backoff: Default::default(),
            max_retries: 10,
            retry_timeout: Duration::from_secs(3 * 60),
        }
    }
}

fn body_contains_error(response_body: &str) -> bool {
    response_body.contains("InternalError") || response_body.contains("SlowDown")
}

/// Combines a [`RetryableRequest`] with a [`RetryContext`]
pub(crate) struct RetryableRequestBuilder {
    request: RetryableRequest,
    context: RetryContext,
}

impl RetryableRequestBuilder {
    /// Set whether this request is idempotent
    ///
    /// An idempotent request will be retried on timeout even if the request
    /// method is not [safe](https://datatracker.ietf.org/doc/html/rfc7231#section-4.2.1)
    pub(crate) fn idempotent(mut self, idempotent: bool) -> Self {
        self.request.idempotent = Some(idempotent);
        self
    }

    /// Set whether this request should be retried on a 409 Conflict response.
    #[cfg(feature = "aws")]
    pub(crate) fn retry_on_conflict(mut self, retry_on_conflict: bool) -> Self {
        self.request.retry_on_conflict = retry_on_conflict;
        self
    }

    /// Set whether this request contains sensitive data
    ///
    /// This will avoid printing out the URL in error messages
    #[allow(unused)]
    pub(crate) fn sensitive(mut self, sensitive: bool) -> Self {
        self.request.sensitive = sensitive;
        self
    }

    /// Provide a [`PutPayload`]
    pub(crate) fn payload(mut self, payload: Option<PutPayload>) -> Self {
        self.request.payload = payload;
        self
    }

    #[allow(unused)]
    pub(crate) fn retry_error_body(mut self, retry_error_body: bool) -> Self {
        self.request.retry_error_body = retry_error_body;
        self
    }

    pub(crate) async fn send(mut self) -> Result<HttpResponse> {
        self.request.send(&mut self.context).await
    }
}

/// A retryable request
pub(crate) struct RetryableRequest {
    client: HttpClient,
    http: HttpRequest,

    sensitive: bool,
    idempotent: Option<bool>,
    retry_on_conflict: bool,
    payload: Option<PutPayload>,

    retry_error_body: bool,
}

impl RetryableRequest {
    #[allow(unused)]
    pub(crate) fn sensitive(self, sensitive: bool) -> Self {
        Self { sensitive, ..self }
    }

    fn err(&self, error: RequestError, ctx: &RetryContext) -> RetryError {
        RetryError(Box::new(RetryErrorImpl {
            uri: (!self.sensitive).then(|| self.http.uri().clone()),
            method: self.http.method().clone(),
            retries: ctx.retries,
            max_retries: ctx.max_retries,
            elapsed: ctx.start.elapsed(),
            retry_timeout: ctx.retry_timeout,
            inner: error,
        }))
    }

    pub(crate) async fn send(self, ctx: &mut RetryContext) -> Result<HttpResponse> {
        loop {
            let mut request = self.http.clone();

            if let Some(payload) = &self.payload {
                *request.body_mut() = payload.clone().into();
            }

            match self.client.execute(request).await {
                Ok(r) => {
                    let status = r.status();
                    if status.is_success() {
                        // For certain S3 requests, 200 response may contain `InternalError` or
                        // `SlowDown` in the message. These responses should be handled similarly
                        // to r5xx errors.
                        // More info here: https://repost.aws/knowledge-center/s3-resolve-200-internalerror
                        if !self.retry_error_body {
                            return Ok(r);
                        }

                        let (parts, body) = r.into_parts();
                        let body = match body.text().await {
                            Ok(body) => body,
                            Err(e) => return Err(self.err(RequestError::Http(e), ctx)),
                        };

                        if !body_contains_error(&body) {
                            // Success response and no error, clone and return response
                            return Ok(HttpResponse::from_parts(parts, body.into()));
                        } else {
                            // Retry as if this was a 5xx response
                            if ctx.exhausted() {
                                return Err(self.err(RequestError::Response { body, status }, ctx));
                            }

                            let sleep = ctx.backoff();
                            info!(
                                "Encountered a response status of {} but body contains Error, backing off for {} seconds, retry {} of {}",
                                status,
                                sleep.as_secs_f32(),
                                ctx.retries,
                                ctx.max_retries,
                            );
                            tokio::time::sleep(sleep).await;
                        }
                    } else if status == StatusCode::NOT_MODIFIED {
                        return Err(self.err(RequestError::Status { status, body: None }, ctx));
                    } else if status.is_redirection() {
                        let is_bare_redirect = !r.headers().contains_key(LOCATION);
                        return match is_bare_redirect {
                            true => Err(self.err(RequestError::BareRedirect, ctx)),
                            false => Err(self.err(
                                RequestError::Status {
                                    body: None,
                                    status: r.status(),
                                },
                                ctx,
                            )),
                        };
                    } else {
                        let status = r.status();
                        if ctx.exhausted()
                            || !(status.is_server_error()
                                || status == StatusCode::TOO_MANY_REQUESTS
                                || status == StatusCode::REQUEST_TIMEOUT
                                || (self.retry_on_conflict && status == StatusCode::CONFLICT))
                        {
                            let source = match r.into_body().text().await {
                                Ok(body) => RequestError::Status {
                                    status,
                                    body: Some(body),
                                },
                                Err(e) => RequestError::Http(e),
                            };
                            return Err(self.err(source, ctx));
                        };

                        let sleep = ctx.backoff();
                        info!(
                            "Encountered server error with status {}, backing off for {} seconds, retry {} of {}",
                            status,
                            sleep.as_secs_f32(),
                            ctx.retries,
                            ctx.max_retries,
                        );
                        tokio::time::sleep(sleep).await;
                    }
                }
                Err(e) => {
                    let is_idempotent = self
                        .idempotent
                        .unwrap_or_else(|| self.http.method().is_safe());

                    let do_retry = match e.kind() {
                        HttpErrorKind::Connect | HttpErrorKind::Request => true, // Request not sent, can retry
                        HttpErrorKind::Timeout | HttpErrorKind::Interrupted => is_idempotent,
                        HttpErrorKind::Unknown | HttpErrorKind::Decode => false,
                    };

                    if ctx.exhausted() || !do_retry {
                        return Err(self.err(RequestError::Http(e), ctx));
                    }
                    let sleep = ctx.backoff();
                    info!(
                        "Encountered transport error of kind {:?}, backing off for {} seconds, retry {} of {}: {}",
                        e.kind(),
                        sleep.as_secs_f32(),
                        ctx.retries,
                        ctx.max_retries,
                        e,
                    );
                    tokio::time::sleep(sleep).await;
                }
            }
        }
    }
}

pub(crate) trait RetryExt {
    /// Return a [`RetryableRequestBuilder`]
    fn retryable(self, config: &RetryConfig) -> RetryableRequestBuilder;

    /// Return a [`RetryableRequest`]
    fn retryable_request(self) -> RetryableRequest;

    /// Dispatch a request with the given retry configuration
    ///
    /// # Panic
    ///
    /// This will panic if the request body is a stream
    fn send_retry(self, config: &RetryConfig) -> BoxFuture<'static, Result<HttpResponse>>;
}

impl RetryExt for HttpRequestBuilder {
    fn retryable(self, config: &RetryConfig) -> RetryableRequestBuilder {
        RetryableRequestBuilder {
            request: self.retryable_request(),
            context: RetryContext::new(config),
        }
    }

    fn retryable_request(self) -> RetryableRequest {
        let (client, request) = self.into_parts();
        let request = request.expect("request must be valid");

        RetryableRequest {
            client,
            http: request,
            idempotent: None,
            payload: None,
            sensitive: false,
            retry_on_conflict: false,
            retry_error_body: false,
        }
    }

    fn send_retry(self, config: &RetryConfig) -> BoxFuture<'static, Result<HttpResponse>> {
        let request = self.retryable(config);
        Box::pin(async move { request.send().await })
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use crate::RetryConfig;
    use crate::client::mock_server::MockServer;
    use crate::client::retry::{RequestError, RetryContext, RetryExt, body_contains_error};
    use crate::client::{HttpClient, HttpError, HttpErrorKind, HttpResponse};
    use http::StatusCode;
    use hyper::Response;
    use hyper::header::LOCATION;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper_util::rt::TokioIo;
    use reqwest::{Client, Method};
    use std::convert::Infallible;
    use std::error::Error;
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio::time::timeout;

}
