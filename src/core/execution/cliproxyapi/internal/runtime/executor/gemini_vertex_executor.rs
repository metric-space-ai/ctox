// ref: internal/runtime/executor/gemini_vertex_executor.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Vertex Gemini/Imagen executor with injected service-account token minting.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, Map, Value};
use tokio::sync::mpsc;

use crate::internal::thinking::parse_suffix;
use crate::internal::util::{apply_custom_headers_from_attrs, HeaderRequest};
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamChunk, ExecutorStreamResponse, Headers, HostHttpClient, HttpRequest,
    PluginExecutionError, PluginFuture, ProviderExecutor,
};
use crate::sdk::translator::{Format, Registry, TranslationContext, TranslationState};

use super::gemini_executor::fix_gemini_image_aspect_ratio;

pub const VERTEX_API_VERSION: &str = "v1";
pub const VERTEX_DEFAULT_BASE_URL: &str = "https://aiplatform.googleapis.com";
pub const VERTEX_DEFAULT_LOCATION: &str = "us-central1";
pub const VERTEX_CLOUD_PLATFORM_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

pub trait VertexAccessTokenProvider: Send + Sync {
    fn access_token<'a>(&'a self, service_account: &'a Value) -> PluginFuture<'a, String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VertexExecutorError {
    Cancelled,
    MissingHttpClient,
    MissingCredentials,
    MissingProject,
    MissingServiceAccount,
    MissingTokenProvider,
    EmptyAccessToken,
    InvalidJson(String),
    MissingPrompt,
    UnsupportedCompact,
    Upstream { status: u16, body: String },
    MissingTokenCount,
}
impl fmt::Display for VertexExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("vertex executor: request cancelled"),
            Self::MissingHttpClient => formatter.write_str("vertex executor: HTTP client missing"),
            Self::MissingCredentials => {
                formatter.write_str("vertex executor: missing auth metadata")
            }
            Self::MissingProject => {
                formatter.write_str("vertex executor: missing project_id in credentials")
            }
            Self::MissingServiceAccount => {
                formatter.write_str("vertex executor: missing service_account in credentials")
            }
            Self::MissingTokenProvider => {
                formatter.write_str("vertex executor: service-account token provider missing")
            }
            Self::EmptyAccessToken => formatter.write_str("vertex executor: missing access token"),
            Self::InvalidJson(error) => write!(formatter, "vertex executor: invalid JSON: {error}"),
            Self::MissingPrompt => formatter.write_str("imagen: no prompt found in request"),
            Self::UnsupportedCompact => formatter.write_str("/responses/compact not supported"),
            Self::Upstream { status, body } => {
                write!(formatter, "vertex upstream {status}: {body}")
            }
            Self::MissingTokenCount => {
                formatter.write_str("vertex executor: totalTokens missing in response")
            }
        }
    }
}
impl std::error::Error for VertexExecutorError {}

pub struct GeminiVertexExecutor {
    registry: Arc<Registry>,
    token_provider: Option<Arc<dyn VertexAccessTokenProvider>>,
    cancellation: TranslationContext,
    response_sequence: AtomicU64,
    request_processor: Option<Arc<dyn super::helps::CodexMultiAgentV2Processor + Send + Sync>>,
}

impl GeminiVertexExecutor {
    #[must_use]
    pub fn new(
        registry: Arc<Registry>,
        token_provider: Option<Arc<dyn VertexAccessTokenProvider>>,
    ) -> Self {
        Self {
            registry,
            token_provider,
            cancellation: TranslationContext::default(),
            response_sequence: AtomicU64::new(1),
            request_processor: None,
        }
    }

    #[must_use]
    pub fn with_context(
        registry: Arc<Registry>,
        token_provider: Option<Arc<dyn VertexAccessTokenProvider>>,
        cancellation: TranslationContext,
    ) -> Self {
        Self {
            registry,
            token_provider,
            cancellation,
            response_sequence: AtomicU64::new(1),
            request_processor: None,
        }
    }

    /// Bind the host's existing client configuration and model catalog owner.
    #[must_use]
    pub fn with_request_processor(
        mut self,
        processor: Arc<dyn super::helps::CodexMultiAgentV2Processor + Send + Sync>,
    ) -> Self {
        self.request_processor = Some(processor);
        self
    }

    async fn authorization(
        &self,
        request: &ExecutorRequest,
    ) -> Result<VertexAuthorization, PluginExecutionError> {
        if let Some(key) = vertex_api_key(request).filter(|key| !key.is_empty()) {
            return Ok(VertexAuthorization::ApiKey(key));
        }
        let service_account = request
            .auth_metadata
            .get("service_account")
            .ok_or_else(|| plugin_error(VertexExecutorError::MissingServiceAccount))?;
        let provider = self
            .token_provider
            .as_ref()
            .ok_or_else(|| plugin_error(VertexExecutorError::MissingTokenProvider))?;
        let token = provider.access_token(service_account).await?;
        if token.trim().is_empty() {
            return Err(plugin_error(VertexExecutorError::EmptyAccessToken));
        }
        Ok(VertexAuthorization::Bearer(token))
    }

    fn prepare_body(
        &self,
        request: &ExecutorRequest,
        stream: bool,
        count: bool,
    ) -> Result<(Vec<u8>, Format), PluginExecutionError> {
        self.ensure_active()?;
        let model = base_model(&request.model);
        if is_imagen_model(&model) && !count {
            return Ok((
                convert_to_imagen_request(&request.payload)?,
                Format::from("gemini"),
            ));
        }
        let from = source_format(request);
        let to = Format::from("gemini");
        let mut body = self.translate_body(request, &from, &to, &model, stream);
        body = fix_gemini_image_aspect_ratio(&model, &body);
        let mut value: Value = serde_json::from_slice(&body)
            .map_err(|error| plugin_error(VertexExecutorError::InvalidJson(error.to_string())))?;
        let object = value.as_object_mut().ok_or_else(|| {
            plugin_error(VertexExecutorError::InvalidJson(
                "root must be an object".into(),
            ))
        })?;
        object.insert("model".into(), Value::String(model));
        object.remove("session_id");
        strip_vertex_openai_response_tool_call_ids(object, &from);
        if count {
            object.remove("tools");
            object.remove("generationConfig");
            object.remove("safetySettings");
        }
        body = serde_json::to_vec(&value)
            .map_err(|error| plugin_error(VertexExecutorError::InvalidJson(error.to_string())))?;
        Ok((body, to))
    }

    // ref: gemini_vertex_executor.go:334-335,471-472,928,1022 @ d7914afd
    fn translate_body(
        &self,
        request: &ExecutorRequest,
        from: &Format,
        to: &Format,
        model: &str,
        stream: bool,
    ) -> Vec<u8> {
        // Vertex uses ordinary Codex translation for both credential kinds.
        let translate = |processor: &dyn super::helps::CodexMultiAgentV2Processor| {
            super::helps::translate_request_with_codex_multi_agent_v2_for_executor(
                processor,
                &request.headers,
                "vertex",
                from,
                to,
                model,
                &request.payload,
                stream,
            )
        };
        if let Some(processor) = &self.request_processor {
            return translate(processor.as_ref());
        }
        let client =
            crate::internal::client::codex::optimize_multi_agent_v2::MultiAgentV2Context::default();
        let metadata = |_: &str| None;
        translate(&super::helps::RegistryCodexMultiAgentV2Processor {
            registry: &self.registry,
            context: &self.cancellation,
            client: &client,
            model_metadata: &metadata,
            orphan_delegation_compatibility: false,
        })
    }

    async fn build_request(
        &self,
        execution: &ExecutorRequest,
        body: Vec<u8>,
        stream: bool,
        count: bool,
    ) -> Result<HttpRequest, PluginExecutionError> {
        let authorization = self.authorization(execution).await?;
        let model = base_model(&execution.model);
        let action = if count {
            "countTokens"
        } else {
            vertex_action(&model, stream)
        };
        let base = vertex_request_base_url(execution)?;
        let mut url = match authorization {
            VertexAuthorization::ApiKey(_) => {
                format!("{base}/{VERTEX_API_VERSION}/publishers/google/models/{model}:{action}")
            }
            VertexAuthorization::Bearer(_) => {
                let project = vertex_project(execution)?;
                let location = vertex_location(execution);
                format!("{base}/{VERTEX_API_VERSION}/projects/{project}/locations/{location}/publishers/google/models/{model}:{action}")
            }
        };
        if stream && !execution.alt.is_empty() {
            url.push_str("?$alt=");
            url.push_str(&execution.alt);
        }
        let mut request = HttpRequest {
            method: "POST".into(),
            url,
            body,
            ..HttpRequest::default()
        };
        request
            .headers
            .insert("Content-Type".into(), vec!["application/json".into()]);
        match authorization {
            VertexAuthorization::ApiKey(key) => {
                set_header(&mut request.headers, "x-goog-api-key", key)
            }
            VertexAuthorization::Bearer(token) => set_header(
                &mut request.headers,
                "Authorization",
                format!("Bearer {token}"),
            ),
        }
        apply_custom_headers(&mut request.headers, &execution.auth_attributes);
        Ok(request)
    }

    async fn execute_inner(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        reject_compact(&request)?;
        let client = require_client(&request)?;
        let model = base_model(&request.model);
        let imagen = is_imagen_model(&model);
        let (body, to) = self.prepare_body(&request, false, false)?;
        let upstream = self
            .build_request(&request, body.clone(), false, false)
            .await?;
        let response = client.execute(upstream).await?;
        ensure_success(response.status_code, &response.body)?;
        self.ensure_active()?;
        let raw = if imagen {
            convert_imagen_to_gemini_response(
                &response.body,
                &model,
                self.response_sequence.fetch_add(1, Ordering::Relaxed),
            )
        } else {
            response.body
        };
        let mut state: TranslationState = None;
        let payload = self.registry.translate_non_stream(
            &self.cancellation,
            &to,
            &response_format(&request),
            &request.model,
            original_request(&request),
            &body,
            &raw,
            &mut state,
        );
        Ok(ExecutorResponse {
            payload,
            headers: response.headers,
            ..ExecutorResponse::default()
        })
    }

    async fn execute_stream_inner(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorStreamResponse, PluginExecutionError> {
        reject_compact(&request)?;
        if is_imagen_model(&base_model(&request.model)) {
            let response = self.execute_inner(request).await?;
            let (sender, receiver) = mpsc::channel(1);
            sender
                .send(ExecutorStreamChunk {
                    payload: response.payload,
                    error: None,
                })
                .await
                .map_err(|_| plugin_error(VertexExecutorError::Cancelled))?;
            drop(sender);
            return Ok(ExecutorStreamResponse {
                headers: response.headers,
                chunks: receiver,
            });
        }
        let client = require_client(&request)?;
        let (body, to) = self.prepare_body(&request, true, false)?;
        let upstream = self
            .build_request(&request, body.clone(), true, false)
            .await?;
        let response = client.execute_stream(upstream).await?;
        ensure_success(response.status_code, &[])?;
        let headers = response.headers;
        let mut incoming = response.chunks;
        let (sender, receiver) = mpsc::channel(16);
        let registry = Arc::clone(&self.registry);
        let context = self.cancellation.clone();
        let target = response_format(&request);
        let model = request.model.clone();
        let original = original_request(&request).to_vec();
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
                    &to,
                    &target,
                    &model,
                    &original,
                    &body,
                    &chunk.payload,
                    &mut state,
                ) {
                    if sender
                        .send(ExecutorStreamChunk {
                            payload,
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
        let client = require_client(&request)?;
        let (body, to) = self.prepare_body(&request, false, true)?;
        let response = client
            .execute(self.build_request(&request, body, false, true).await?)
            .await?;
        ensure_success(response.status_code, &response.body)?;
        let count = serde_json::from_slice::<Value>(&response.body)
            .ok()
            .and_then(|value| value.get("totalTokens").and_then(Value::as_i64))
            .filter(|count| *count >= 0)
            .ok_or_else(|| plugin_error(VertexExecutorError::MissingTokenCount))?;
        let payload = self.registry.translate_token_count(
            &self.cancellation,
            &to,
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
            Err(plugin_error(VertexExecutorError::Cancelled))
        } else {
            Ok(())
        }
    }
}

impl ProviderExecutor for GeminiVertexExecutor {
    fn identifier(&self) -> &str {
        "vertex"
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
            let client = request
                .http_client
                .clone()
                .ok_or_else(|| plugin_error(VertexExecutorError::MissingHttpClient))?;
            let execution = ExecutorRequest {
                auth_metadata: request.metadata,
                auth_attributes: request.attributes,
                ..ExecutorRequest::default()
            };
            let auth = self.authorization(&execution).await?;
            let mut upstream = HttpRequest {
                method: request.method,
                url: request.url,
                headers: request.headers,
                body: request.body,
            };
            match auth {
                VertexAuthorization::ApiKey(key) => {
                    set_header(&mut upstream.headers, "x-goog-api-key", key)
                }
                VertexAuthorization::Bearer(token) => set_header(
                    &mut upstream.headers,
                    "Authorization",
                    format!("Bearer {token}"),
                ),
            }
            let response = client.execute(upstream).await?;
            Ok(ExecutorHttpResponse {
                status_code: response.status_code,
                headers: response.headers,
                body: response.body,
            })
        })
    }
}

enum VertexAuthorization {
    ApiKey(String),
    Bearer(String),
}

#[must_use]
pub fn is_imagen_model(model: &str) -> bool {
    model.to_ascii_lowercase().contains("imagen")
}
#[must_use]
pub fn vertex_action(model: &str, stream: bool) -> &'static str {
    if is_imagen_model(model) {
        "predict"
    } else if stream {
        "streamGenerateContent"
    } else {
        "generateContent"
    }
}

pub fn convert_to_imagen_request(payload: &[u8]) -> Result<Vec<u8>, PluginExecutionError> {
    let value: Value = serde_json::from_slice(payload)
        .map_err(|error| plugin_error(VertexExecutorError::InvalidJson(error.to_string())))?;
    let prompt = value
        .pointer("/contents/0/parts/0/text")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("messages")
                .and_then(Value::as_array)
                .and_then(|messages| {
                    messages.iter().find_map(|message| {
                        message
                            .get("content")
                            .and_then(Value::as_str)
                            .filter(|text| !text.is_empty())
                    })
                })
        })
        .or_else(|| value.get("prompt").and_then(Value::as_str))
        .filter(|text| !text.is_empty())
        .ok_or_else(|| plugin_error(VertexExecutorError::MissingPrompt))?;
    let mut instance = Map::new();
    instance.insert("prompt".into(), Value::String(prompt.into()));
    if let Some(value) = value.get("negativePrompt") {
        instance.insert("negativePrompt".into(), value.clone());
    }
    let mut parameters = Map::new();
    parameters.insert(
        "sampleCount".into(),
        value.get("sampleCount").cloned().unwrap_or(Value::from(1)),
    );
    if let Some(value) = value.get("aspectRatio") {
        parameters.insert("aspectRatio".into(), value.clone());
    }
    serde_json::to_vec(&json!({"instances":[instance],"parameters":parameters}))
        .map_err(|error| plugin_error(VertexExecutorError::InvalidJson(error.to_string())))
}

#[must_use]
pub fn convert_imagen_to_gemini_response(data: &[u8], model: &str, sequence: u64) -> Vec<u8> {
    let Ok(value) = serde_json::from_slice::<Value>(data) else {
        return data.to_vec();
    };
    let Some(predictions) = value.get("predictions").and_then(Value::as_array) else {
        return data.to_vec();
    };
    let parts = predictions
        .iter()
        .filter_map(|prediction| {
            let image = prediction
                .get("bytesBase64Encoded")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())?;
            let mime = prediction
                .get("mimeType")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("image/png");
            Some(json!({"inlineData":{"mimeType":mime,"data":image}}))
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&json!({
        "candidates":[{"content":{"parts":parts,"role":"model"},"finishReason":"STOP"}],
        "responseId":format!("imagen-{sequence}"),"modelVersion":model,
        "usageMetadata":{"promptTokenCount":0,"candidatesTokenCount":0,"totalTokenCount":0}
    }))
    .unwrap_or_else(|_| data.to_vec())
}

#[must_use]
pub fn vertex_base_url(location: &str) -> String {
    let location = location.trim();
    if location.is_empty() || location == "global" {
        VERTEX_DEFAULT_BASE_URL.into()
    } else {
        format!("https://{location}-aiplatform.googleapis.com")
    }
}

fn vertex_api_key(request: &ExecutorRequest) -> Option<String> {
    request.auth_attributes.get("api_key").cloned().or_else(|| {
        request
            .auth_metadata
            .get("access_token")
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}
fn vertex_project(request: &ExecutorRequest) -> Result<String, PluginExecutionError> {
    request
        .auth_metadata
        .get("project_id")
        .or_else(|| request.auth_metadata.get("project"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| plugin_error(VertexExecutorError::MissingProject))
}
fn vertex_location(request: &ExecutorRequest) -> String {
    request
        .auth_metadata
        .get("location")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(VERTEX_DEFAULT_LOCATION)
        .into()
}
fn vertex_request_base_url(request: &ExecutorRequest) -> Result<String, PluginExecutionError> {
    if let Some(base) = request
        .auth_attributes
        .get("base_url")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Ok(base.trim_end_matches('/').into())
    } else if vertex_api_key(request).is_some() {
        Ok(VERTEX_DEFAULT_BASE_URL.into())
    } else {
        Ok(vertex_base_url(&vertex_location(request)))
    }
}
fn strip_vertex_openai_response_tool_call_ids(body: &mut Map<String, Value>, from: &Format) {
    if !matches!(from.as_str(), "responses" | "openai-response") {
        return;
    }
    if let Some(contents) = body.get_mut("contents").and_then(Value::as_array_mut) {
        for content in contents {
            if let Some(parts) = content.get_mut("parts").and_then(Value::as_array_mut) {
                for part in parts {
                    if let Some(object) = part.as_object_mut() {
                        object.remove("id");
                        object.remove("call_id");
                    }
                }
            }
        }
    }
}
fn base_model(model: &str) -> String {
    parse_suffix(model).model_name
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
        Err(plugin_error(VertexExecutorError::UnsupportedCompact))
    } else {
        Ok(())
    }
}
fn require_client(
    request: &ExecutorRequest,
) -> Result<Arc<dyn HostHttpClient>, PluginExecutionError> {
    request
        .http_client
        .clone()
        .ok_or_else(|| plugin_error(VertexExecutorError::MissingHttpClient))
}
fn ensure_success(status: u16, body: &[u8]) -> Result<(), PluginExecutionError> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(plugin_error(VertexExecutorError::Upstream {
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
fn set_header(headers: &mut Headers, name: &str, value: String) {
    if let Some(key) = headers
        .keys()
        .find(|key| key.eq_ignore_ascii_case(name))
        .cloned()
    {
        headers.remove(&key);
    }
    headers.insert(name.into(), vec![value]);
}
fn plugin_error(error: VertexExecutorError) -> PluginExecutionError {
    Arc::new(error)
}

#[cfg(test)]
#[path = "gemini_vertex_executor_candidate_test.rs"]
mod candidate_google_requests;

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{
        convert_imagen_to_gemini_response, convert_to_imagen_request, is_imagen_model,
        vertex_action, vertex_base_url,
    };

    #[test]
    fn selects_vertex_actions_by_model_and_streaming() {
        assert_eq!(vertex_action("imagen-3", false), "predict");
        assert_eq!(vertex_action("gemini-3", false), "generateContent");
        assert_eq!(vertex_action("gemini-3", true), "streamGenerateContent");
        assert!(is_imagen_model("Publishers/Google/IMAGEN-3"));
    }

    #[test]
    fn builds_regional_and_global_vertex_base_urls() {
        assert_eq!(
            vertex_base_url("us-east1"),
            "https://us-east1-aiplatform.googleapis.com"
        );
        assert_eq!(
            vertex_base_url("global"),
            "https://aiplatform.googleapis.com"
        );
        assert_eq!(vertex_base_url(""), "https://aiplatform.googleapis.com");
    }

    #[test]
    fn converts_gemini_request_to_imagen_shape() {
        let converted = convert_to_imagen_request(br#"{"contents":[{"parts":[{"text":"a fox"}]}],"aspectRatio":"16:9","sampleCount":2,"negativePrompt":"blur"}"#).unwrap();
        let value: Value = serde_json::from_slice(&converted).unwrap();
        assert_eq!(
            value.pointer("/instances/0/prompt").and_then(Value::as_str),
            Some("a fox")
        );
        assert_eq!(
            value
                .pointer("/instances/0/negativePrompt")
                .and_then(Value::as_str),
            Some("blur")
        );
        assert_eq!(
            value
                .pointer("/parameters/sampleCount")
                .and_then(Value::as_i64),
            Some(2)
        );
    }

    #[test]
    fn converts_imagen_predictions_to_gemini_candidates() {
        let converted = convert_imagen_to_gemini_response(
            br#"{"predictions":[{"bytesBase64Encoded":"abc","mimeType":"image/webp"}]}"#,
            "imagen-3",
            42,
        );
        let value: Value = serde_json::from_slice(&converted).unwrap();
        assert_eq!(
            value
                .pointer("/candidates/0/content/parts/0/inlineData/data")
                .and_then(Value::as_str),
            Some("abc")
        );
        assert_eq!(value["responseId"], "imagen-42");
        assert_eq!(value["modelVersion"], "imagen-3");
    }

    #[test]
    fn leaves_non_imagen_response_unchanged() {
        let raw = br#"{"candidates":[]}"#;
        assert_eq!(convert_imagen_to_gemini_response(raw, "gemini", 1), raw);
    }
}
