// ref: internal/runtime/executor/gemini_executor.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Gemini Generative Language and native Interactions executors.
//!
//! Upstream obtains its HTTP client, translator registry, configuration and
//! cancellation from package globals/context values.  The Rust port makes
//! those capabilities constructor-owned so multiple gateway hosts can run
//! independently in one CTOX process.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use serde_json::{Map, Value};
use tokio::sync::mpsc;

use crate::internal::thinking::parse_suffix;
use crate::internal::util::{
    apply_custom_headers_from_attrs, create_white_image_base64, HeaderRequest,
};
use crate::sdk::cliproxy::usage::{Detail, Manager, UsageContext};
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamChunk, ExecutorStreamResponse, Headers, HostHttpClient, HttpRequest,
    PluginExecutionError, PluginFuture, ProviderExecutor,
};
use crate::sdk::translator::{Format, Registry, TranslationContext, TranslationState};

use super::helps::UsageReporter;

pub const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com";
pub const GEMINI_API_VERSION: &str = "v1beta";
pub const GEMINI_INTERACTIONS_API_REVISION: &str = "2026-05-20";
pub const GEMINI_STREAM_SCANNER_BUFFER: usize = 52_428_800;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeminiPayloadRule {
    pub models: Vec<String>,
    pub protocol: String,
    pub from_protocol: String,
    pub defaults: Map<String, Value>,
    pub overrides: Map<String, Value>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeminiExecutorConfig {
    pub payload_rules: Vec<GeminiPayloadRule>,
    /// Model-specific output limits supplied by the host's catalog snapshot.
    pub output_token_limits: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GeminiExecutorError {
    Cancelled,
    MissingHttpClient,
    InvalidJson(String),
    Upstream { status: u16, body: String },
    UnsupportedCompact,
    MissingTokenCount,
}

impl fmt::Display for GeminiExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("gemini executor: request cancelled"),
            Self::MissingHttpClient => formatter.write_str("gemini executor: HTTP client missing"),
            Self::InvalidJson(error) => write!(formatter, "gemini executor: invalid JSON: {error}"),
            Self::Upstream { status, body } => {
                write!(formatter, "gemini upstream {status}: {body}")
            }
            Self::UnsupportedCompact => formatter.write_str("/responses/compact not supported"),
            Self::MissingTokenCount => {
                formatter.write_str("gemini executor: totalTokens missing in response")
            }
        }
    }
}

impl std::error::Error for GeminiExecutorError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeminiProtocol {
    GenerateContent,
    Interactions,
}

pub struct GeminiExecutor {
    identifier: String,
    protocol: GeminiProtocol,
    config: Arc<GeminiExecutorConfig>,
    registry: Arc<Registry>,
    cancellation: TranslationContext,
    usage_manager: Option<Arc<Manager>>,
    payload_config: Arc<super::helps::PayloadApplyConfig>,
    request_processor: Option<Arc<dyn super::helps::CodexMultiAgentV2Processor + Send + Sync>>,
}

impl GeminiExecutor {
    #[must_use]
    pub fn new(config: Arc<GeminiExecutorConfig>, registry: Arc<Registry>) -> Self {
        Self::with_context(config, registry, TranslationContext::default())
    }

    #[must_use]
    pub fn with_context(
        config: Arc<GeminiExecutorConfig>,
        registry: Arc<Registry>,
        cancellation: TranslationContext,
    ) -> Self {
        Self {
            identifier: "gemini".into(),
            protocol: GeminiProtocol::GenerateContent,
            payload_config: Arc::new(payload_config_from_legacy(&config)),
            config,
            registry,
            cancellation,
            usage_manager: None,
            request_processor: None,
        }
    }

    #[must_use]
    pub fn interactions(config: Arc<GeminiExecutorConfig>, registry: Arc<Registry>) -> Self {
        Self {
            identifier: "gemini-interactions".into(),
            protocol: GeminiProtocol::Interactions,
            payload_config: Arc::new(payload_config_from_legacy(&config)),
            config,
            registry,
            cancellation: TranslationContext::default(),
            usage_manager: None,
            request_processor: None,
        }
    }

    #[must_use]
    pub fn with_usage_manager(mut self, manager: Arc<Manager>) -> Self {
        self.usage_manager = Some(manager);
        self
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

    #[must_use]
    pub fn with_payload_config(mut self, config: Arc<super::helps::PayloadApplyConfig>) -> Self {
        self.payload_config = config;
        self
    }

    #[must_use]
    pub fn request_to_format(&self, request: &ExecutorRequest) -> Format {
        if self.uses_native_interactions(request) {
            Format::from("interactions")
        } else {
            Format::from("gemini")
        }
    }

    #[must_use]
    pub fn uses_native_interactions(&self, request: &ExecutorRequest) -> bool {
        self.protocol == GeminiProtocol::Interactions
            && request
                .auth_provider
                .trim()
                .eq_ignore_ascii_case("gemini-interactions")
            && (request.source_format.is_empty()
                || native_interactions_source_format(&Format::from(request.source_format.clone())))
    }

    pub fn prepare_request(&self, request: &mut HttpRequest, execution: &ExecutorRequest) {
        let api_key = gemini_api_key(&execution.auth_metadata, &execution.auth_attributes);
        if !api_key.is_empty() {
            set_header(&mut request.headers, "x-goog-api-key", api_key);
            remove_header(&mut request.headers, "Authorization");
        }
        apply_custom_headers(&mut request.headers, &execution.auth_attributes);
    }

    fn prepare_body(
        &self,
        request: &ExecutorRequest,
        stream: bool,
        action: &str,
    ) -> Result<(Vec<u8>, Format), PluginExecutionError> {
        self.ensure_active()?;
        let model = base_model(&request.model);
        let from = source_format(request);
        let to = self.request_to_format(request);
        let (original, mut body) = if action == "countTokens" {
            (
                Vec::new(),
                self.translate_body(request, &from, &to, &model, stream, &request.payload),
            )
        } else {
            self.translate_pair(request, &from, &to, &model, stream)
        };
        if to.as_str() != "interactions" {
            body = fix_gemini_image_aspect_ratio(&model, &body);
            if action != "countTokens" {
                body = self.apply_request_payload(request, &from, &to, &model, &body, &original);
            }
        }
        let mut json = parse_object(&body)?;
        json.remove("session_id");
        if to.as_str() == "interactions" {
            normalize_interactions_input(&mut json);
            if json.contains_key("agent") {
                json.remove("model");
            } else {
                json.insert("model".into(), Value::String(model.clone()));
            }
            apply_interactions_thinking_suffix(&mut json, &request.model);
            if stream {
                json.insert("stream".into(), Value::Bool(true));
            }
        } else {
            json.insert("model".into(), Value::String(model.clone()));
            if action == "countTokens" {
                json.remove("tools");
                json.remove("generationConfig");
                json.remove("safetySettings");
            } else {
                cap_gemini_max_output_tokens_value(
                    &mut json,
                    &model,
                    self.config.output_token_limits.get(&model).copied(),
                );
            }
        }
        body = serde_json::to_vec(&json)
            .map_err(|error| plugin_error(GeminiExecutorError::InvalidJson(error.to_string())))?;
        if to.as_str() == "interactions" && action != "countTokens" {
            body = self.apply_request_payload(request, &from, &to, &model, &body, &original);
        }
        // ref: gemini_executor.go:167-177,293-294,697-698 @ d7914afd
        // Native Interactions uses a different wire contract and bypasses these
        // GenerateContent history/signature repairs.
        if to.as_str() != "interactions" {
            body = crate::internal::signature::sanitize_gemini_request_thought_signatures(&body);
            body = if action == "countTokens" {
                super::helps::ensure_gemini_leading_user_content(&body, "contents").into_owned()
            } else {
                super::helps::ensure_gemini_boundary_user_content(&body, "contents").into_owned()
            };
        }
        Ok((body, to))
    }

    // ref: gemini_executor.go:153-154,895-918 @ d7914afd
    fn translate_pair(
        &self,
        request: &ExecutorRequest,
        from: &Format,
        to: &Format,
        model: &str,
        stream: bool,
    ) -> (Vec<u8>, Vec<u8>) {
        let original = original_request(request);
        let same = original.len() == request.payload.len()
            && std::ptr::eq(original.as_ptr(), request.payload.as_ptr());
        let translate =
            |payload: &[u8]| self.translate_body(request, from, to, model, stream, payload);
        if to.as_str() == "interactions" {
            let working = translate(&request.payload);
            let baseline = if same {
                working.clone()
            } else {
                translate(original)
            };
            (baseline, working)
        } else {
            let baseline = translate(original);
            let working = if same {
                baseline.clone()
            } else {
                translate(&request.payload)
            };
            (baseline, working)
        }
    }

    fn apply_request_payload(
        &self,
        request: &ExecutorRequest,
        from: &Format,
        to: &Format,
        model: &str,
        body: &[u8],
        original: &[u8],
    ) -> Vec<u8> {
        let requested = request
            .metadata
            .get("requested_model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&request.model);
        let path = request
            .metadata
            .get("request_path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let source = if request.source_format.is_empty() && to.as_str() == "interactions" {
            ""
        } else {
            from.as_str()
        };
        super::helps::apply_payload_config_with_request(
            &self.payload_config,
            model,
            to.as_str(),
            source,
            "",
            body,
            Some(original),
            requested,
            path,
            &request.headers,
        )
    }

    // ref: gemini_executor.go:153-154,279-280,684,888-892 @ d7914afd
    fn translate_body(
        &self,
        request: &ExecutorRequest,
        from: &Format,
        to: &Format,
        model: &str,
        stream: bool,
        payload: &[u8],
    ) -> Vec<u8> {
        // Native Interactions input bypasses translators and plugin hooks.
        if to.as_str() == "interactions"
            && (request.source_format.is_empty() || from.as_str() == "interactions")
        {
            return payload.to_vec();
        }
        let translate = |processor: &dyn super::helps::CodexMultiAgentV2Processor| {
            super::helps::translate_request_with_api_key_model_compatibility_for_executor(
                processor,
                &request.headers,
                "gemini",
                from,
                to,
                model,
                payload,
                stream,
                request
                    .resolved_home_model_options
                    .as_ref()
                    .map(|options| options.is_compat)
                    .unwrap_or_else(|| {
                        request
                            .resolved_model_info
                            .as_ref()
                            .is_some_and(|info| info.is_compat)
                    }),
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

    fn build_request(
        &self,
        execution: &ExecutorRequest,
        body: Vec<u8>,
        to: &Format,
        action: &str,
        stream: bool,
    ) -> HttpRequest {
        let model = base_model(&execution.model);
        let base = resolve_gemini_base_url(&execution.auth_attributes);
        let url = if to.as_str() == "interactions" {
            format!("{base}/{GEMINI_API_VERSION}/interactions")
        } else {
            let action = if stream && action != "countTokens" {
                "streamGenerateContent"
            } else {
                action
            };
            let mut url = format!("{base}/{GEMINI_API_VERSION}/models/{model}:{action}");
            if stream && execution.alt.is_empty() {
                url.push_str("?alt=sse");
            } else if !execution.alt.is_empty() && action != "countTokens" {
                url.push_str("?$alt=");
                url.push_str(&execution.alt);
            }
            url
        };
        let mut upstream = HttpRequest {
            method: "POST".into(),
            url,
            body,
            ..HttpRequest::default()
        };
        set_header(
            &mut upstream.headers,
            "Content-Type",
            "application/json".into(),
        );
        self.prepare_request(&mut upstream, execution);
        if to.as_str() == "interactions" {
            apply_interactions_request_headers(&mut upstream.headers, &execution.headers);
            apply_interactions_revision_header(&mut upstream.headers);
        }
        upstream
    }

    async fn execute_inner(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        reject_compact(&request)?;
        let client = require_client(&request)?;
        let (body, to) = self.prepare_body(&request, false, request_action(&request))?;
        let upstream =
            self.build_request(&request, body.clone(), &to, request_action(&request), false);
        let response = client.execute(upstream).await?;
        self.ensure_active()?;
        ensure_success(response.status_code, &response.body)?;
        if let Some(manager) = &self.usage_manager {
            let reporter = UsageReporter::new(
                Arc::clone(manager),
                UsageContext::default(),
                "gemini",
                self.identifier.clone(),
                base_model(&request.model),
                None,
                "",
            );
            reporter.publish(gemini_usage_detail(&response.body));
        }
        let mut state: TranslationState = None;
        let payload = self.registry.translate_non_stream(
            &self.cancellation,
            &to,
            &response_format(&request),
            &request.model,
            original_request(&request),
            &body,
            &response.body,
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
        let client = require_client(&request)?;
        let (body, to) = self.prepare_body(&request, true, request_action(&request))?;
        let upstream =
            self.build_request(&request, body.clone(), &to, request_action(&request), true);
        let response = client.execute_stream(upstream).await?;
        ensure_success(response.status_code, &[])?;
        let headers = response.headers;
        let mut incoming = response.chunks;
        let (sender, receiver) = mpsc::channel(16);
        let registry = Arc::clone(&self.registry);
        let context = self.cancellation.clone();
        let model = request.model.clone();
        let original = original_request(&request).to_vec();
        let target = response_format(&request);
        let interactions = to.as_str() == "interactions";
        tokio::spawn(async move {
            let mut state: TranslationState = None;
            let mut terminal_emitted = false;
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
                let payload = if interactions || trim_ascii(&chunk.payload).starts_with(b"data:") {
                    gemini_interactions_sse_payload(&chunk.payload)
                } else {
                    chunk.payload.clone()
                };
                if interactions && target.as_str() == "interactions" {
                    let done = gemini_interactions_sse_done(&chunk.payload);
                    let mut frame = chunk.payload;
                    while frame
                        .last()
                        .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
                    {
                        frame.pop();
                    }
                    frame.extend_from_slice(b"\n\n");
                    if sender
                        .send(ExecutorStreamChunk {
                            payload: frame,
                            error: None,
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                    if done {
                        terminal_emitted = true;
                        break;
                    }
                    continue;
                }
                if interactions && gemini_interactions_sse_done(&chunk.payload) {
                    break;
                }
                if payload.is_empty() {
                    continue;
                }
                let outputs = registry.translate_stream(
                    &context, &to, &target, &model, &original, &body, &payload, &mut state,
                );
                for payload in outputs {
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
            if !context.is_cancelled() && !terminal_emitted {
                for payload in registry.translate_stream(
                    &context, &to, &target, &model, &original, &body, b"[DONE]", &mut state,
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
        let (body, to) = self.prepare_body(&request, false, "countTokens")?;
        let upstream = self.build_request(&request, body, &to, "countTokens", false);
        let response = client.execute(upstream).await?;
        ensure_success(response.status_code, &response.body)?;
        let raw: Value = serde_json::from_slice(&response.body)
            .map_err(|error| plugin_error(GeminiExecutorError::InvalidJson(error.to_string())))?;
        let count = raw
            .get("totalTokens")
            .and_then(Value::as_i64)
            .filter(|count| *count > 0)
            .ok_or_else(|| plugin_error(GeminiExecutorError::MissingTokenCount))?;
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
            Err(plugin_error(GeminiExecutorError::Cancelled))
        } else {
            Ok(())
        }
    }
}

fn gemini_usage_detail(body: &[u8]) -> Detail {
    let usage = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|body| body.get("usageMetadata").cloned())
        .unwrap_or(Value::Null);
    Detail {
        input_tokens: usage
            .get("promptTokenCount")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        output_tokens: usage
            .get("candidatesTokenCount")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        total_tokens: usage
            .get("totalTokenCount")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        ..Detail::default()
    }
}

impl ProviderExecutor for GeminiExecutor {
    fn identifier(&self) -> &str {
        &self.identifier
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
                .ok_or_else(|| plugin_error(GeminiExecutorError::MissingHttpClient))?;
            let execution = ExecutorRequest {
                auth_provider: request.auth_provider,
                auth_metadata: request.metadata,
                auth_attributes: request.attributes,
                ..ExecutorRequest::default()
            };
            let mut upstream = HttpRequest {
                method: request.method,
                url: request.url,
                headers: request.headers,
                body: request.body,
            };
            self.prepare_request(&mut upstream, &execution);
            let response = client.execute(upstream).await?;
            Ok(ExecutorHttpResponse {
                status_code: response.status_code,
                headers: response.headers,
                body: response.body,
            })
        })
    }
}

#[cfg(test)]
#[path = "gemini_executor_candidate_test.rs"]
mod candidate_google_requests;

#[must_use]
pub fn native_interactions_source_format(format: &Format) -> bool {
    matches!(
        format.as_str().to_ascii_lowercase().as_str(),
        "interactions" | "openai" | "responses" | "openai-response" | "claude" | "gemini"
    )
}

#[must_use]
pub fn gemini_interactions_sse_payload(frame: &[u8]) -> Vec<u8> {
    let trimmed = trim_ascii(frame);
    if trimmed.starts_with(b"{") {
        return trimmed.to_vec();
    }
    let mut payload = Vec::new();
    for line in frame.split(|byte| *byte == b'\n') {
        let line = trim_ascii(line);
        let Some(data) = line.strip_prefix(b"data:") else {
            continue;
        };
        let data = trim_ascii(data);
        if data.is_empty() || data == b"[DONE]" {
            continue;
        }
        if !payload.is_empty() {
            payload.push(b'\n');
        }
        payload.extend_from_slice(data);
    }
    payload
}

#[must_use]
pub fn gemini_interactions_sse_done(frame: &[u8]) -> bool {
    let trimmed = trim_ascii(frame);
    if trimmed == b"[DONE]" {
        return true;
    }
    frame.split(|byte| *byte == b'\n').any(|line| {
        let line = trim_ascii(line);
        line.eq_ignore_ascii_case(b"event: done")
            || line
                .strip_prefix(b"data:")
                .is_some_and(|data| trim_ascii(data) == b"[DONE]")
    })
}

pub fn apply_interactions_revision_header(headers: &mut Headers) {
    if header_value(headers, "Api-Revision").is_none() {
        set_header(
            headers,
            "Api-Revision",
            GEMINI_INTERACTIONS_API_REVISION.into(),
        );
    }
}

pub fn apply_interactions_request_headers(target: &mut Headers, source: &Headers) {
    if header_value(target, "Api-Revision").is_some() {
        return;
    }
    if let Some(value) = header_value(source, "Api-Revision") {
        set_header(target, "Api-Revision", value.to_owned());
    }
}

#[must_use]
pub fn cap_gemini_max_output_tokens(body: &[u8], model: &str, limit: Option<u64>) -> Vec<u8> {
    let Ok(mut json) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(object) = json.as_object_mut() else {
        return body.to_vec();
    };
    cap_gemini_max_output_tokens_value(
        object,
        model,
        limit.or_else(|| embedded_output_limit(model)),
    );
    serde_json::to_vec(&json).unwrap_or_else(|_| body.to_vec())
}

#[must_use]
pub fn fix_gemini_image_aspect_ratio(model: &str, body: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(object) = value.as_object_mut() else {
        return body.to_vec();
    };
    fix_gemini_image_aspect_ratio_value(object, model);
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

fn fix_gemini_image_aspect_ratio_value(body: &mut Map<String, Value>, model: &str) {
    if model != "gemini-2.5-flash-image-preview" {
        return;
    }
    let aspect = body
        .get("generationConfig")
        .and_then(|value| value.get("imageConfig"))
        .and_then(|value| value.get("aspectRatio"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let Some(aspect) = aspect else { return };
    let has_inline_data = body
        .get("contents")
        .and_then(Value::as_array)
        .is_some_and(|contents| {
            contents.iter().any(|content| {
                content
                    .get("parts")
                    .and_then(Value::as_array)
                    .is_some_and(|parts| parts.iter().any(|part| part.get("inlineData").is_some()))
            })
        });
    if !has_inline_data {
        if let Ok(image) = create_white_image_base64(&aspect) {
            let prefix = vec![
                json_value("text", "Based on the following requirements, create an image within the uploaded picture. The new content *MUST* completely cover the entire area of the original picture, maintaining its exact proportions, and *NO* blank areas should appear."),
                Value::Object(Map::from_iter([(
                    "inlineData".into(),
                    Value::Object(Map::from_iter([
                        ("mime_type".into(), Value::String("image/png".into())),
                        ("data".into(), Value::String(image)),
                    ])),
                )])),
            ];
            if let Some(parts) = body
                .get_mut("contents")
                .and_then(Value::as_array_mut)
                .and_then(|contents| contents.first_mut())
                .and_then(|content| content.get_mut("parts"))
                .and_then(Value::as_array_mut)
            {
                parts.splice(0..0, prefix);
            }
            if let Some(config) = body
                .get_mut("generationConfig")
                .and_then(Value::as_object_mut)
            {
                config.insert(
                    "responseModalities".into(),
                    Value::Array(vec![
                        Value::String("IMAGE".into()),
                        Value::String("TEXT".into()),
                    ]),
                );
            }
        }
    }
    if let Some(config) = body
        .get_mut("generationConfig")
        .and_then(Value::as_object_mut)
    {
        config.remove("imageConfig");
    }
}

fn json_value(key: &str, value: &str) -> Value {
    Value::Object(Map::from_iter([(key.into(), Value::String(value.into()))]))
}

fn cap_gemini_max_output_tokens_value(
    body: &mut Map<String, Value>,
    model: &str,
    limit: Option<u64>,
) {
    let Some(limit) = limit.or_else(|| embedded_output_limit(model)) else {
        return;
    };
    let Some(current) = body
        .get_mut("generationConfig")
        .and_then(Value::as_object_mut)
        .and_then(|config| config.get_mut("maxOutputTokens"))
    else {
        return;
    };
    if current.as_u64().is_some_and(|value| value > limit) {
        *current = Value::from(limit);
    }
}

fn embedded_output_limit(model: &str) -> Option<u64> {
    match model.to_ascii_lowercase().as_str() {
        "gemini-3.1-pro-preview" => Some(65_536),
        _ => None,
    }
}

fn payload_config_from_legacy(config: &GeminiExecutorConfig) -> super::helps::PayloadApplyConfig {
    let mut output = super::helps::PayloadApplyConfig::default();
    for rule in &config.payload_rules {
        let names = if rule.models.is_empty() {
            vec!["*".to_owned()]
        } else {
            rule.models.clone()
        };
        let models: Vec<_> = names
            .into_iter()
            .map(|name| super::helps::PayloadModelRule {
                name,
                protocol: rule.protocol.clone(),
                from_protocol: rule.from_protocol.clone(),
                ..Default::default()
            })
            .collect();
        if !rule.defaults.is_empty() {
            output.rules.default.push(super::helps::PayloadRule {
                models: models.clone(),
                params: rule
                    .defaults
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            });
        }
        if !rule.overrides.is_empty() {
            output
                .rules
                .override_values
                .push(super::helps::PayloadRule {
                    models,
                    params: rule
                        .overrides
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                });
        }
    }
    output
}

fn apply_interactions_thinking_suffix(body: &mut Map<String, Value>, model: &str) {
    let suffix = parse_suffix(model);
    if !suffix.has_suffix {
        return;
    }
    let generation = body
        .entry("generation_config")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(generation) = generation.as_object_mut() else {
        return;
    };
    let normalized = suffix.raw_suffix.to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
    ) {
        generation.insert("thinking_level".into(), Value::String(normalized));
    } else if let Ok(budget) = suffix.raw_suffix.parse::<u64>() {
        generation.insert("thinking_budget".into(), Value::from(budget));
    } else if normalized == "none" {
        generation.insert("thinking_budget".into(), Value::from(0));
    }
}

fn normalize_interactions_input(body: &mut Map<String, Value>) {
    let Some(input) = body.get_mut("input").and_then(Value::as_array_mut) else {
        return;
    };
    for item in input {
        let Some(item) = item.as_object_mut() else {
            continue;
        };
        let is_user_message = item.get("type").and_then(Value::as_str) == Some("message")
            && item.get("role").and_then(Value::as_str) == Some("user");
        if is_user_message {
            item.insert("type".into(), Value::String("user_input".into()));
            item.remove("role");
        }
    }
}

fn parse_object(body: &[u8]) -> Result<Map<String, Value>, PluginExecutionError> {
    serde_json::from_slice::<Value>(body)
        .map_err(|error| plugin_error(GeminiExecutorError::InvalidJson(error.to_string())))?
        .as_object()
        .cloned()
        .ok_or_else(|| {
            plugin_error(GeminiExecutorError::InvalidJson(
                "root must be an object".into(),
            ))
        })
}

fn gemini_api_key(
    metadata: &BTreeMap<String, Value>,
    attributes: &BTreeMap<String, String>,
) -> String {
    attributes
        .get("api_key")
        .map(String::as_str)
        .or_else(|| metadata.get("api_key").and_then(Value::as_str))
        .or_else(|| metadata.get("access_token").and_then(Value::as_str))
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn resolve_gemini_base_url(attributes: &BTreeMap<String, String>) -> String {
    attributes
        .get("base_url")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(GEMINI_BASE_URL)
        .trim_end_matches('/')
        .to_owned()
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
fn request_action(request: &ExecutorRequest) -> &str {
    request
        .metadata
        .get("action")
        .and_then(Value::as_str)
        .filter(|action| *action == "countTokens")
        .unwrap_or("generateContent")
}
fn reject_compact(request: &ExecutorRequest) -> Result<(), PluginExecutionError> {
    if request.alt == "responses/compact" {
        Err(plugin_error(GeminiExecutorError::UnsupportedCompact))
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
        .ok_or_else(|| plugin_error(GeminiExecutorError::MissingHttpClient))
}
fn ensure_success(status: u16, body: &[u8]) -> Result<(), PluginExecutionError> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(plugin_error(GeminiExecutorError::Upstream {
            status,
            body: String::from_utf8_lossy(body).into_owned(),
        }))
    }
}
fn apply_custom_headers(headers: &mut Headers, attributes: &BTreeMap<String, String>) {
    let mut request = HeaderRequest {
        headers: std::mem::take(headers),
        ..HeaderRequest::default()
    };
    apply_custom_headers_from_attrs(&mut request, attributes);
    *headers = request.headers;
}
fn set_header(headers: &mut Headers, name: &str, value: String) {
    remove_header(headers, name);
    headers.insert(name.into(), vec![value]);
}
fn remove_header(headers: &mut Headers, name: &str) {
    if let Some(key) = headers
        .keys()
        .find(|key| key.eq_ignore_ascii_case(name))
        .cloned()
    {
        headers.remove(&key);
    }
}
fn header_value<'a>(headers: &'a Headers, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(String::as_str)
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
fn plugin_error(error: GeminiExecutorError) -> PluginExecutionError {
    Arc::new(error)
}
