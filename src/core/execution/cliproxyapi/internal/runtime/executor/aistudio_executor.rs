// ref: internal/runtime/executor/aistudio_executor.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! AI Studio executor. The websocket relay is represented by the injected
//! `HostHttpClient`; CTOX's relay adapter owns connection/session lifecycle.

use std::fmt;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc;

use crate::internal::thinking::parse_suffix;
use crate::internal::util::{apply_custom_headers_from_attrs, HeaderRequest};
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamChunk, ExecutorStreamResponse, Headers, HostHttpClient, HttpRequest,
    PluginExecutionError, PluginFuture, ProviderExecutor,
};
use crate::sdk::translator::{Format, Registry, TranslationContext, TranslationState};

use super::gemini_executor::{fix_gemini_image_aspect_ratio, GEMINI_API_VERSION, GEMINI_BASE_URL};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiStudioExecutorError {
    Cancelled,
    MissingRelay,
    MissingAuth,
    EmptyUrl,
    UnsupportedCompact,
    InvalidJson(String),
    Upstream { status: u16, body: String },
    MissingTokenCount,
}

impl fmt::Display for AiStudioExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("aistudio executor: request cancelled"),
            Self::MissingRelay => formatter.write_str("aistudio executor: ws relay is nil"),
            Self::MissingAuth => formatter.write_str("aistudio executor: missing auth"),
            Self::EmptyUrl => formatter.write_str("aistudio executor: request URL is empty"),
            Self::UnsupportedCompact => formatter.write_str("/responses/compact not supported"),
            Self::InvalidJson(error) => {
                write!(formatter, "aistudio executor: invalid JSON: {error}")
            }
            Self::Upstream { status, body } => {
                write!(formatter, "aistudio upstream {status}: {body}")
            }
            Self::MissingTokenCount => {
                formatter.write_str("wsrelay: totalTokens missing in response")
            }
        }
    }
}
impl std::error::Error for AiStudioExecutorError {}

pub struct AiStudioExecutor {
    provider: String,
    registry: Arc<Registry>,
    relay: Arc<dyn HostHttpClient>,
    cancellation: TranslationContext,
}

impl AiStudioExecutor {
    #[must_use]
    pub fn new(
        provider: impl Into<String>,
        registry: Arc<Registry>,
        relay: Arc<dyn HostHttpClient>,
    ) -> Self {
        Self {
            provider: provider.into().to_ascii_lowercase(),
            registry,
            relay,
            cancellation: TranslationContext::default(),
        }
    }

    #[must_use]
    pub fn with_context(
        provider: impl Into<String>,
        registry: Arc<Registry>,
        relay: Arc<dyn HostHttpClient>,
        cancellation: TranslationContext,
    ) -> Self {
        Self {
            provider: provider.into().to_ascii_lowercase(),
            registry,
            relay,
            cancellation,
        }
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn prepare_request(
        &self,
        request: &mut HttpRequest,
        attributes: &std::collections::BTreeMap<String, String>,
    ) {
        apply_custom_headers(&mut request.headers, attributes);
    }

    fn translate_request(
        &self,
        request: &ExecutorRequest,
        stream: bool,
    ) -> Result<TranslatedPayload, PluginExecutionError> {
        self.ensure_active()?;
        let model = parse_suffix(&request.model).model_name;
        let from = source_format(request);
        let to = Format::from("gemini");
        let mut payload = self.registry.translate_request(
            &self.cancellation,
            &from,
            &to,
            &model,
            &request.payload,
            stream,
        );
        payload = super::helps::normalize_codex_tool_integer_types_for_executor(
            &payload,
            &request.headers,
            "aistudio",
        );
        payload = fix_gemini_image_aspect_ratio(&model, &payload);
        let mut json: Value = serde_json::from_slice(&payload)
            .map_err(|error| plugin_error(AiStudioExecutorError::InvalidJson(error.to_string())))?;
        let object = json.as_object_mut().ok_or_else(|| {
            plugin_error(AiStudioExecutorError::InvalidJson(
                "root must be an object".into(),
            ))
        })?;
        object.remove("session_id");
        if let Some(config) = object
            .get_mut("generationConfig")
            .and_then(Value::as_object_mut)
        {
            config.remove("maxOutputTokens");
            config.remove("responseMimeType");
            config.remove("responseJsonSchema");
        }
        let metadata_action = request
            .metadata
            .get("action")
            .and_then(Value::as_str)
            .filter(|value| *value == "countTokens")
            .unwrap_or("generateContent");
        let action = if stream && metadata_action != "countTokens" {
            "streamGenerateContent"
        } else {
            metadata_action
        };
        payload = serde_json::to_vec(&json)
            .map_err(|error| plugin_error(AiStudioExecutorError::InvalidJson(error.to_string())))?;
        Ok(TranslatedPayload {
            payload,
            action: action.into(),
            format: to,
        })
    }

    #[must_use]
    pub fn build_endpoint(&self, model: &str, action: &str, alt: &str) -> String {
        let base = format!("{GEMINI_BASE_URL}/{GEMINI_API_VERSION}/models/{model}:{action}");
        if action == "streamGenerateContent" {
            if alt.is_empty() {
                format!("{base}?alt=sse")
            } else {
                format!("{base}?$alt={alt}")
            }
        } else if !alt.is_empty() && action != "countTokens" {
            format!("{base}?$alt={alt}")
        } else {
            base
        }
    }

    fn upstream_request(
        &self,
        request: &ExecutorRequest,
        translated: &TranslatedPayload,
    ) -> HttpRequest {
        let model = parse_suffix(&request.model).model_name;
        let mut upstream = HttpRequest {
            method: "POST".into(),
            url: self.build_endpoint(&model, &translated.action, &request.alt),
            body: translated.payload.clone(),
            ..HttpRequest::default()
        };
        upstream
            .headers
            .insert("Content-Type".into(), vec!["application/json".into()]);
        self.prepare_request(&mut upstream, &request.auth_attributes);
        upstream
    }

    async fn execute_inner(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        reject_compact(&request)?;
        let translated = self.translate_request(&request, false)?;
        let upstream = self.upstream_request(&request, &translated);
        let response = self.relay.execute(upstream).await?;
        ensure_success(response.status_code, &response.body)?;
        self.ensure_active()?;
        let mut state: TranslationState = None;
        let payload = self.registry.translate_non_stream(
            &self.cancellation,
            &translated.format,
            &response_format(&request),
            &request.model,
            original_request(&request),
            &translated.payload,
            &response.body,
            &mut state,
        );
        Ok(ExecutorResponse {
            payload: ensure_colon_spaced_json(&payload),
            headers: response.headers,
            ..ExecutorResponse::default()
        })
    }

    async fn execute_stream_inner(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorStreamResponse, PluginExecutionError> {
        reject_compact(&request)?;
        let translated = self.translate_request(&request, true)?;
        let upstream = self.upstream_request(&request, &translated);
        let response = self.relay.execute_stream(upstream).await?;
        ensure_success(response.status_code, &[])?;
        let headers = response.headers;
        let mut incoming = response.chunks;
        let (sender, receiver) = mpsc::channel(16);
        let registry = Arc::clone(&self.registry);
        let context = self.cancellation.clone();
        let format = translated.format;
        let target = response_format(&request);
        let model = request.model.clone();
        let original = original_request(&request).to_vec();
        let body = translated.payload;
        tokio::spawn(async move {
            let mut state: TranslationState = None;
            while let Some(chunk) = incoming.recv().await {
                if context.is_cancelled() {
                    break;
                }
                if let Some(error) = chunk.error {
                    if sender
                        .send(ExecutorStreamChunk {
                            payload: Vec::new(),
                            error: Some(error),
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                    continue;
                }
                for payload in registry.translate_stream(
                    &context,
                    &format,
                    &target,
                    &model,
                    &original,
                    &body,
                    &chunk.payload,
                    &mut state,
                ) {
                    if sender
                        .send(ExecutorStreamChunk {
                            payload: ensure_colon_spaced_json(&payload),
                            error: None,
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
        Ok(ExecutorStreamResponse {
            headers,
            chunks: receiver,
        })
    }

    async fn count_tokens_inner(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        let mut translated = self.translate_request(&request, false)?;
        let object = serde_json::from_slice::<Value>(&translated.payload)
            .ok()
            .and_then(|mut value| {
                let object = value.as_object_mut()?;
                object.remove("generationConfig");
                object.remove("tools");
                object.remove("safetySettings");
                serde_json::to_vec(&value).ok()
            })
            .ok_or_else(|| {
                plugin_error(AiStudioExecutorError::InvalidJson(
                    "root must be an object".into(),
                ))
            })?;
        translated.payload = object;
        translated.action = "countTokens".into();
        let response = self
            .relay
            .execute(self.upstream_request(&request, &translated))
            .await?;
        ensure_success(response.status_code, &response.body)?;
        let count = serde_json::from_slice::<Value>(&response.body)
            .ok()
            .and_then(|value| value.get("totalTokens").and_then(Value::as_i64))
            .filter(|count| *count > 0)
            .ok_or_else(|| plugin_error(AiStudioExecutorError::MissingTokenCount))?;
        let payload = self.registry.translate_token_count(
            &self.cancellation,
            &translated.format,
            &response_format(&request),
            count,
            &response.body,
        );
        Ok(ExecutorResponse {
            payload,
            headers: response.headers,
            ..ExecutorResponse::default()
        })
    }

    fn ensure_active(&self) -> Result<(), PluginExecutionError> {
        if self.cancellation.is_cancelled() {
            Err(plugin_error(AiStudioExecutorError::Cancelled))
        } else {
            Ok(())
        }
    }
}

impl ProviderExecutor for AiStudioExecutor {
    fn identifier(&self) -> &str {
        "aistudio"
    }
    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move { self.execute_inner(request).await })
    }
    fn execute_stream<'a>(
        &'a self,
        request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(async move { self.execute_stream_inner(request).await })
    }
    fn count_tokens<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move { self.count_tokens_inner(request).await })
    }
    fn http_request<'a>(
        &'a self,
        request: ExecutorHttpRequest,
    ) -> PluginFuture<'a, ExecutorHttpResponse> {
        Box::pin(async move {
            self.ensure_active()?;
            if request.auth_id.trim().is_empty() {
                return Err(plugin_error(AiStudioExecutorError::MissingAuth));
            }
            if request.url.trim().is_empty() {
                return Err(plugin_error(AiStudioExecutorError::EmptyUrl));
            }
            let mut upstream = HttpRequest {
                method: request.method,
                url: request.url,
                headers: request.headers,
                body: request.body,
            };
            self.prepare_request(&mut upstream, &request.attributes);
            let response = self.relay.execute(upstream).await?;
            Ok(ExecutorHttpResponse {
                status_code: response.status_code,
                headers: response.headers,
                body: response.body,
            })
        })
    }
}

struct TranslatedPayload {
    payload: Vec<u8>,
    action: String,
    format: Format,
}

#[must_use]
pub fn ensure_colon_spaced_json(payload: &[u8]) -> Vec<u8> {
    let Ok(value) = serde_json::from_slice::<Value>(trim_ascii(payload)) else {
        return payload.to_vec();
    };
    let Ok(compact) = serde_json::to_vec(&value) else {
        return payload.to_vec();
    };
    let mut output = Vec::with_capacity(compact.len() + 8);
    let mut in_string = false;
    let mut escaped = false;
    for byte in compact {
        if in_string {
            output.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else if byte == b'"' {
            in_string = true;
            output.push(byte);
        } else if byte == b':' {
            output.extend_from_slice(b": ");
        } else {
            output.push(byte);
        }
    }
    output
}

fn source_format(request: &ExecutorRequest) -> Format {
    if request.source_format.trim().is_empty() {
        Format::from("gemini")
    } else {
        Format::from(request.source_format.clone())
    }
}
fn response_format(request: &ExecutorRequest) -> Format {
    if request.format.trim().is_empty() {
        source_format(request)
    } else {
        Format::from(request.format.clone())
    }
}
fn original_request(request: &ExecutorRequest) -> &[u8] {
    if request.original_request.is_empty() {
        &request.payload
    } else {
        &request.original_request
    }
}
fn reject_compact(request: &ExecutorRequest) -> Result<(), PluginExecutionError> {
    if request.alt == "responses/compact" {
        Err(plugin_error(AiStudioExecutorError::UnsupportedCompact))
    } else {
        Ok(())
    }
}
fn ensure_success(status: u16, body: &[u8]) -> Result<(), PluginExecutionError> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(plugin_error(AiStudioExecutorError::Upstream {
            status,
            body: String::from_utf8_lossy(body).into_owned(),
        }))
    }
}
fn apply_custom_headers(
    headers: &mut Headers,
    attributes: &std::collections::BTreeMap<String, String>,
) {
    let mut request = HeaderRequest {
        headers: std::mem::take(headers),
        ..HeaderRequest::default()
    };
    apply_custom_headers_from_attrs(&mut request, attributes);
    *headers = request.headers;
}
fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}
fn plugin_error(error: AiStudioExecutorError) -> PluginExecutionError {
    Arc::new(error)
}
