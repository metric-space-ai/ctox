// ref: internal/runtime/executor/claude_executor_stream.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::sync::Arc;

use crate::sdk::cliproxy::auth::AuthError;
use crate::sdk::cliproxy::executor::RequestTerminatedError;
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamChunk, ExecutorStreamResponse, Headers, HttpRequest, PluginExecutionError,
    PluginFuture, ProviderExecutor,
};

use super::{
    claude_fast_direct_response_error, claude_request_uses_fast_mode, claude_requested_betas,
    claude_token_count_response, decode_claude_response_body, parse_claude_usage,
    ClaudeAccountPoolError, ClaudeExecutionError, ClaudeExecutionRequestContext,
    ClaudeMessagesTransportFailure, ClaudeSubscriptionAccountPool, ClaudeTokenCountError,
    ClaudeUpstreamTarget,
};

/// Manager-facing adapter for the native Claude subscription executor.
///
/// The payload is already translated to Claude format by the owning execution
/// pipeline. Selection is auth-exact: this adapter never performs a second
/// scheduler pass or account failover after the manager chose `AuthID`.
pub struct ClaudeProviderExecutor {
    pool: Arc<ClaudeSubscriptionAccountPool>,
}

impl ClaudeProviderExecutor {
    #[must_use]
    pub fn new(pool: Arc<ClaudeSubscriptionAccountPool>) -> Self {
        Self { pool }
    }

    fn validate_identity(
        &self,
        request: &ExecutorRequest,
    ) -> Result<(), ClaudeProviderExecutorError> {
        if !request.auth_provider.eq_ignore_ascii_case("claude") {
            return Err(ClaudeProviderExecutorError::ProviderMismatch);
        }
        if request.auth_id.trim().is_empty() {
            return Err(ClaudeProviderExecutorError::MissingAuthId);
        }
        if !self.pool.contains_auth(request.auth_id.as_str()) {
            return Err(ClaudeProviderExecutorError::UnknownAuthId);
        }
        if request.model.trim().is_empty() {
            return Err(ClaudeProviderExecutorError::MissingModel);
        }
        Ok(())
    }

    fn validate_request(
        &self,
        request: &ExecutorRequest,
        expected_stream: bool,
    ) -> Result<(), ClaudeProviderExecutorError> {
        self.validate_identity(request)?;
        if request.stream != expected_stream {
            return Err(ClaudeProviderExecutorError::StreamContractMismatch);
        }
        if request.alt == "responses/compact" {
            return Err(ClaudeProviderExecutorError::UnsupportedCompact);
        }
        Ok(())
    }
}

impl fmt::Debug for ClaudeProviderExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeProviderExecutor")
            .field("provider", &"claude")
            .field("pool", &"[REDACTED]")
            .finish()
    }
}

impl ProviderExecutor for ClaudeProviderExecutor {
    fn identifier(&self) -> &str {
        "claude"
    }

    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move {
            self.validate_request(&request, false)
                .map_err(plugin_error)?;
            let context = claude_execution_request_context(&request);
            let fast_request = executor_request_is_fast(&request);
            let payload = if fast_request {
                ensure_fast_mode_body(&request.payload)
            } else {
                request.payload
            };
            let outcome = self
                .pool
                .execute_selected_with_context(
                    request.auth_id.as_str(),
                    request.model.as_str(),
                    payload,
                    false,
                    Some(&context),
                )
                .await
                .map_err(plugin_pool_error)?;
            let response = outcome.outcome().response();
            if !(200..300).contains(&response.status()) {
                if let Some(body) = crate::internal::clienterror::claude_thread_replay_body(
                    response.status(),
                    response.body(),
                ) {
                    return Err(Arc::new(RequestTerminatedError {
                        http_status: response.status(),
                        headers: response.headers().clone(),
                        body,
                    }) as PluginExecutionError);
                }
                if fast_request {
                    return Err(Arc::new(claude_fast_direct_response_error(
                        response.status(),
                        response.headers().clone(),
                        response.body(),
                    )) as PluginExecutionError);
                }
                return Err(plugin_error(ClaudeProviderExecutorError::UpstreamStatus(
                    response.status(),
                )));
            }
            let payload = response.body().to_vec();
            let metadata = parse_claude_usage(&payload)
                .and_then(|usage| serde_json::to_value(usage).ok())
                .map(|usage| [("usage".to_owned(), usage)].into_iter().collect())
                .unwrap_or_default();
            Ok(ExecutorResponse {
                payload,
                headers: response.headers().clone(),
                metadata,
            })
        })
    }

    fn execute_stream<'a>(
        &'a self,
        request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(async move {
            self.validate_request(&request, true)
                .map_err(plugin_error)?;
            let context = claude_execution_request_context(&request);
            let fast_request = executor_request_is_fast(&request);
            let payload = if fast_request {
                ensure_fast_mode_body(&request.payload)
            } else {
                request.payload
            };
            let outcome = self
                .pool
                .execute_stream_selected_with_context(
                    request.auth_id.as_str(),
                    request.model.as_str(),
                    payload,
                    Some(&context),
                )
                .await
                .map_err(plugin_pool_error)?;
            let status = outcome.outcome().response().status();
            if !(200..300).contains(&status) {
                let response = outcome.outcome().response();
                if let Some(body) = crate::internal::clienterror::claude_thread_replay_body(
                    status,
                    response.error_body(),
                ) {
                    return Err(Arc::new(RequestTerminatedError {
                        http_status: status,
                        headers: response.headers().clone(),
                        body,
                    }) as PluginExecutionError);
                }
                if fast_request {
                    let response = outcome.outcome().response();
                    return Err(Arc::new(claude_fast_direct_response_error(
                        status,
                        response.headers().clone(),
                        response.error_body(),
                    )) as PluginExecutionError);
                }
                return Err(plugin_error(ClaudeProviderExecutorError::UpstreamStatus(
                    status,
                )));
            }
            let headers = outcome.outcome().response().headers().clone();
            let mut upstream = outcome.into_outcome().into_response();
            let (sender, receiver) = tokio::sync::mpsc::channel(8);
            tokio::spawn(async move {
                while let Some(chunk) = upstream.next_chunk().await {
                    let downstream = match chunk {
                        Ok(payload) => ExecutorStreamChunk {
                            payload,
                            error: None,
                        },
                        Err(error) => ExecutorStreamChunk {
                            payload: Vec::new(),
                            error: Some(plugin_error(
                                ClaudeProviderExecutorError::StreamTransport(error),
                            )),
                        },
                    };
                    let terminal = downstream.error.is_some();
                    if sender.send(downstream).await.is_err() || terminal {
                        break;
                    }
                }
            });
            Ok(ExecutorStreamResponse {
                headers,
                chunks: receiver,
            })
        })
    }

    fn count_tokens<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move {
            self.validate_identity(&request).map_err(plugin_error)?;
            if let Some(target) = self.pool.selected_target(&request.auth_id) {
                if target.is_anthropic_api() {
                    let context = claude_count_tokens_request_context(&request);
                    let outcome = self
                        .pool
                        .execute_count_tokens_selected_with_context(
                            &request.auth_id,
                            &request.model,
                            request.payload,
                            Some(&context),
                        )
                        .await
                        .map_err(plugin_pool_error)?;
                    let response = outcome.outcome().response();
                    if !(200..300).contains(&response.status()) {
                        return Err(Arc::new(RequestTerminatedError {
                            http_status: response.status(),
                            headers: response.headers().clone(),
                            body: response.body().to_vec(),
                        }) as PluginExecutionError);
                    }
                    return Ok(ExecutorResponse {
                        payload: response.body().to_vec(),
                        headers: response.headers().clone(),
                        metadata: Default::default(),
                    });
                }
            }
            let payload = claude_token_count_response(&request.payload)
                .map_err(|error| plugin_error(ClaudeProviderExecutorError::TokenCount(error)))?;
            Ok(ExecutorResponse {
                payload,
                headers: Headers::new(),
                metadata: Default::default(),
            })
        })
    }

    fn http_request<'a>(
        &'a self,
        request: ExecutorHttpRequest,
    ) -> PluginFuture<'a, ExecutorHttpResponse> {
        Box::pin(async move {
            if !request.auth_provider.eq_ignore_ascii_case("claude") {
                return Err(plugin_error(ClaudeProviderExecutorError::ProviderMismatch));
            }
            if request.auth_id.trim().is_empty() {
                return Err(plugin_error(ClaudeProviderExecutorError::MissingAuthId));
            }
            if !self.pool.contains_auth(&request.auth_id) {
                return Err(plugin_error(ClaudeProviderExecutorError::UnknownAuthId));
            }
            let client = request
                .http_client
                .clone()
                .ok_or_else(|| plugin_error(ClaudeProviderExecutorError::MissingHttpClient))?;
            let url = url::Url::parse(&request.url)
                .map_err(|_| plugin_error(ClaudeProviderExecutorError::InvalidHttpUrl))?;
            let authority = url
                .host_str()
                .map(|host| match url.port() {
                    Some(port) => format!("{host}:{port}"),
                    None => host.to_owned(),
                })
                .ok_or_else(|| plugin_error(ClaudeProviderExecutorError::InvalidHttpUrl))?;
            let target = ClaudeUpstreamTarget::new(url.scheme(), authority)
                .map_err(|_| plugin_error(ClaudeProviderExecutorError::InvalidHttpUrl))?;
            let authorization = self
                .pool
                .prepare_selected_authorization(&request.auth_id, &target)
                .await
                .map_err(|error| plugin_error(ClaudeProviderExecutorError::Pool(error)))?;
            let mut headers = request.headers;
            remove_header_case_insensitive(&mut headers, authorization.remove_header().as_str());
            remove_header_case_insensitive(&mut headers, authorization.set_header().as_str());
            headers.insert(
                authorization.set_header().as_str().to_owned(),
                vec![authorization.expose_header_value().to_owned()],
            );
            let response = client
                .execute(HttpRequest {
                    method: request.method,
                    url: request.url,
                    headers,
                    body: request.body,
                })
                .await?;
            let content_encoding =
                header_value_case_insensitive(&response.headers, "Content-Encoding");
            let body = decode_claude_response_body(&response.body, content_encoding)
                .map_err(|error| -> PluginExecutionError { Arc::new(error) })?;
            Ok(ExecutorHttpResponse {
                status_code: response.status_code,
                headers: response.headers,
                body,
            })
        })
    }
}

fn claude_execution_request_context(request: &ExecutorRequest) -> ClaudeExecutionRequestContext {
    let original_payload = if request.original_request.is_empty() {
        request.payload.as_slice()
    } else {
        request.original_request.as_slice()
    };
    ClaudeExecutionRequestContext::from_provider_request_with_metadata(
        request.auth_id.clone(),
        request.headers.clone(),
        original_payload,
        &request.payload,
        request.auth_metadata.clone(),
        request.auth_attributes.clone(),
        &request.metadata,
    )
}

fn claude_count_tokens_request_context(request: &ExecutorRequest) -> ClaudeExecutionRequestContext {
    let original_payload = if request.original_request.is_empty() {
        request.payload.as_slice()
    } else {
        request.original_request.as_slice()
    };
    ClaudeExecutionRequestContext::from_provider_count_tokens_request_with_metadata(
        request.auth_id.clone(),
        request.headers.clone(),
        original_payload,
        &request.payload,
        request.auth_metadata.clone(),
        request.auth_attributes.clone(),
        &request.metadata,
    )
}

fn remove_header_case_insensitive(headers: &mut Headers, name: &str) {
    let keys: Vec<String> = headers
        .keys()
        .filter(|key| key.eq_ignore_ascii_case(name))
        .cloned()
        .collect();
    for key in keys {
        headers.remove(&key);
    }
}

fn header_value_case_insensitive<'a>(headers: &'a Headers, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(String::as_str)
}

fn plugin_error(error: ClaudeProviderExecutorError) -> PluginExecutionError {
    Arc::new(error)
}

fn plugin_pool_error(error: ClaudeAccountPoolError) -> PluginExecutionError {
    if matches!(
        error,
        ClaudeAccountPoolError::Execution(ClaudeExecutionError::Transport(
            ClaudeMessagesTransportFailure::Cancelled
        ))
    ) {
        return Arc::new(AuthError {
            code: "request_scoped".to_owned(),
            message: "Claude OAuth request cancelled".to_owned(),
            retryable: false,
            http_status: 0,
        });
    }
    if let ClaudeAccountPoolError::Execution(ClaudeExecutionError::CallerSystemBlock(cause)) =
        &error
    {
        return Arc::new(AuthError {
            code: "request_scoped".to_owned(),
            message: cause.to_string(),
            retryable: false,
            http_status: 400,
        });
    }
    if let ClaudeAccountPoolError::Execution(ClaudeExecutionError::CredentialIdentity(cause)) =
        &error
    {
        if cause.is_request_scoped() {
            return Arc::new(AuthError {
                code: "request_scoped".to_owned(),
                message: cause.to_string(),
                retryable: false,
                http_status: cause.status_code().unwrap_or(400),
            });
        }
    }
    plugin_error(ClaudeProviderExecutorError::Pool(error))
}

fn executor_request_is_fast(request: &ExecutorRequest) -> bool {
    let incoming = request
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("anthropic-beta"))
        .flat_map(|(_, values)| values.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join(",");
    claude_request_uses_fast_mode(&request.payload, &claude_requested_betas(&incoming, &[]))
}

fn ensure_fast_mode_body(body: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<serde_json::Value>(body) else {
        return body.to_vec();
    };
    let Some(object) = root.as_object_mut() else {
        return body.to_vec();
    };
    object.insert(
        "speed".to_owned(),
        serde_json::Value::String("fast".to_owned()),
    );
    serde_json::to_vec(&root).unwrap_or_else(|_| body.to_vec())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaudeProviderExecutorError {
    ProviderMismatch,
    MissingAuthId,
    UnknownAuthId,
    MissingModel,
    StreamContractMismatch,
    UnsupportedCompact,
    Pool(ClaudeAccountPoolError),
    UpstreamStatus(u16),
    StreamTransport(ClaudeMessagesTransportFailure),
    TokenCount(ClaudeTokenCountError),
    MissingHttpClient,
    InvalidHttpUrl,
}

impl ClaudeProviderExecutorError {
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::UpstreamStatus(status) => Some(*status),
            _ => None,
        }
    }
}

impl fmt::Display for ClaudeProviderExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ProviderMismatch => "Claude executor received a different auth provider",
            Self::MissingAuthId => "Claude executor requires a selected auth ID",
            Self::UnknownAuthId => "Claude executor auth ID is not in the validated pool",
            Self::MissingModel => "Claude executor requires a model",
            Self::StreamContractMismatch => "Claude executor stream contract does not match",
            Self::UnsupportedCompact => "Claude executor does not support responses/compact",
            Self::Pool(_) => "Claude selected-account execution failed",
            Self::UpstreamStatus(_) => "Claude upstream returned a non-success status",
            Self::StreamTransport(_) => "Claude upstream stream failed",
            Self::TokenCount(_) => "Claude token counting failed",
            Self::MissingHttpClient => "Claude authenticated HTTP bridging requires a host client",
            Self::InvalidHttpUrl => "Claude authenticated HTTP bridging received an invalid URL",
        })
    }
}

impl std::error::Error for ClaudeProviderExecutorError {}

#[cfg(test)]
#[path = "claude_executor_test.rs"]
mod tests;
