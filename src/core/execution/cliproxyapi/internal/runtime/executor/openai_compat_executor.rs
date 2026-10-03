// ref: internal/runtime/executor/openai_compat_executor.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! OpenAI-compatible provider executor.
//!
//! Upstream obtains HTTP, configuration, and translator authority from Go
//! process globals/context. This port keeps the same routing and mutation
//! behavior but requires an injected translator registry and per-request host
//! HTTP client. No credentials or stream state escape the request.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::internal::thinking::parse_suffix;
use crate::internal::util::{apply_custom_headers_from_attrs, HeaderRequest};
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamChunk, ExecutorStreamResponse, HostHttpClient, HttpRequest, PluginExecutionError,
    PluginFuture, ProviderExecutor,
};
use crate::sdk::translator::{Format, Registry, TranslationContext, TranslationState};

use super::openai_responses_signature::sanitize_openai_responses_reasoning_encrypted_content;

#[cfg(test)]
#[path = "openai_compat_resolved_compat_test.rs"]
mod candidate_resolved_model_capability_tests;

pub const OPENAI_COMPAT_IMAGE_HANDLER_TYPE: &str = "openai-image";
pub const OPENAI_COMPAT_IMAGES_GENERATIONS_PATH: &str = "/images/generations";
pub const OPENAI_COMPAT_IMAGES_EDITS_PATH: &str = "/images/edits";
pub const OPENAI_COMPAT_DEFAULT_IMAGE_ENDPOINT: &str = OPENAI_COMPAT_IMAGES_GENERATIONS_PATH;
pub const OPENAI_COMPAT_MAX_STREAM_LINE_BYTES: usize = 52_428_800;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OpenAiCompatibilityModel {
    pub name: String,
    pub alias: String,
    pub input_modalities: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenAiCompatPayloadModelRule {
    pub name: String,
    pub protocol: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenAiCompatPayloadRule {
    pub models: Vec<OpenAiCompatPayloadModelRule>,
    pub params: Map<String, Value>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenAiCompatibility {
    pub name: String,
    pub disabled: bool,
    pub support_prompt_cache_key: bool,
    pub models: Vec<OpenAiCompatibilityModel>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenAiCompatConfig {
    pub compatibility: Vec<OpenAiCompatibility>,
    pub payload_overrides: Vec<OpenAiCompatPayloadRule>,
}

#[derive(Clone)]
pub struct OpenAiCompatExecutor {
    provider: String,
    config: Arc<OpenAiCompatConfig>,
    registry: Arc<Registry>,
}

impl OpenAiCompatExecutor {
    #[must_use]
    pub fn new(
        provider: impl Into<String>,
        config: Arc<OpenAiCompatConfig>,
        registry: Arc<Registry>,
    ) -> Self {
        Self {
            provider: provider.into(),
            config,
            registry,
        }
    }

    #[must_use]
    pub fn identifier(&self) -> &str {
        &self.provider
    }

    fn resolve_credentials(attributes: &BTreeMap<String, String>) -> (&str, &str) {
        (
            attributes
                .get("base_url")
                .map(String::as_str)
                .unwrap_or("")
                .trim(),
            attributes
                .get("api_key")
                .map(String::as_str)
                .unwrap_or("")
                .trim(),
        )
    }

    fn resolve_compat_config<'a>(
        &'a self,
        attributes: &BTreeMap<String, String>,
        provider: &str,
    ) -> Option<&'a OpenAiCompatibility> {
        let from_config = attributes
            .get("source")
            .is_some_and(|source| source.trim().starts_with("config:"));
        if from_config {
            if let Some(index) = attributes
                .get("config_index")
                .and_then(|index| index.trim().parse::<usize>().ok())
            {
                if let Some(config) = self.config.compatibility.get(index) {
                    if !config.disabled {
                        return Some(config);
                    }
                }
            }
        }
        let candidates = [
            attributes.get("compat_name").map(String::as_str),
            attributes.get("provider_key").map(String::as_str),
            Some(provider),
        ];
        self.config.compatibility.iter().find(|config| {
            !config.disabled
                && candidates.into_iter().flatten().any(|candidate| {
                    !candidate.trim().is_empty()
                        && candidate.trim().eq_ignore_ascii_case(config.name.trim())
                })
        })
    }

    pub fn prepare_request(
        &self,
        request: &mut HttpRequest,
        attributes: &BTreeMap<String, String>,
    ) {
        let (_, api_key) = Self::resolve_credentials(attributes);
        if !api_key.is_empty() {
            set_header(
                &mut request.headers,
                "Authorization",
                format!("Bearer {api_key}"),
            );
        }
        let mut header_request = HeaderRequest {
            headers: std::mem::take(&mut request.headers),
            ..HeaderRequest::default()
        };
        apply_custom_headers_from_attrs(&mut header_request, attributes);
        request.headers = header_request.headers;
    }

    #[must_use]
    pub fn override_model(&self, payload: &[u8], model: &str) -> Vec<u8> {
        if payload.is_empty() || model.is_empty() {
            return payload.to_vec();
        }
        set_json_string(payload, "model", model)
    }

    #[must_use]
    pub fn refresh(&self, attributes: &BTreeMap<String, String>) -> BTreeMap<String, String> {
        attributes.clone()
    }

    pub(crate) fn translate_request(
        &self,
        request: &ExecutorRequest,
        to: &Format,
        stream: bool,
    ) -> Vec<u8> {
        let from = Format::from(request.source_format.as_str());
        let base_model = parse_suffix(&request.model).model_name;
        let context = TranslationContext::default();
        let client =
            crate::internal::client::codex::optimize_multi_agent_v2::MultiAgentV2Context::default();
        let metadata = |_: &str| None;
        let processor = super::helps::RegistryCodexMultiAgentV2Processor {
            registry: &self.registry,
            context: &context,
            client: &client,
            model_metadata: &metadata,
            orphan_delegation_compatibility: false,
        };
        let mut translated =
            super::helps::translate_request_with_api_key_model_compatibility_for_executor(
                &processor,
                &request.headers,
                "openai-compat",
                &from,
                to,
                &base_model,
                &request.payload,
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
            );
        translated = apply_model_suffix_effort(&translated, &request.model);
        translated =
            self.apply_payload_overrides(&translated, requested_model(request), to.as_str());
        if self
            .resolve_compat_config(&request.auth_attributes, &request.auth_provider)
            .is_some_and(|compat| {
                should_normalize_openai_tool_results_for_model(
                    compat,
                    &base_model,
                    requested_model(request),
                )
            })
        {
            translated = normalize_openai_tool_results_text_only(&translated);
        }
        translated
    }

    fn apply_payload_overrides(&self, payload: &[u8], model: &str, protocol: &str) -> Vec<u8> {
        let mut config = super::helps::PayloadApplyConfig::default();
        config.rules.override_values = self
            .config
            .payload_overrides
            .iter()
            .map(|rule| super::helps::PayloadRule {
                models: rule
                    .models
                    .iter()
                    .map(|candidate| super::helps::PayloadModelRule {
                        name: candidate.name.clone(),
                        protocol: candidate.protocol.clone(),
                        ..super::helps::PayloadModelRule::default()
                    })
                    .collect(),
                params: rule
                    .params
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            })
            .collect();
        super::helps::apply_payload_config_with_root(
            &config, model, protocol, "", payload, None, model, "",
        )
    }

    pub(crate) fn apply_prompt_cache_key(
        &self,
        request: &ExecutorRequest,
        translated: &[u8],
    ) -> Vec<u8> {
        let Some(compat) =
            self.resolve_compat_config(&request.auth_attributes, &request.auth_provider)
        else {
            return translated.to_vec();
        };
        if !compat.support_prompt_cache_key {
            return translated.to_vec();
        }
        for payload in [
            request.payload.as_slice(),
            request.original_request.as_slice(),
            translated,
        ] {
            if let Some(key) = json_string(payload, "prompt_cache_key") {
                if !key.trim().is_empty() {
                    return set_json_string(translated, "prompt_cache_key", key.trim());
                }
            }
        }
        let session_id = provider_session_id(request);
        if session_id.is_empty() {
            return translated.to_vec();
        }
        let model = json_string(translated, "model")
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| parse_suffix(&request.model).model_name);
        let provider = if self.provider.trim().is_empty() {
            compat.name.trim()
        } else {
            self.provider.trim()
        };
        let identity = [
            "cli-proxy-api:openai-compat:prompt-cache".to_owned(),
            provider.to_ascii_lowercase(),
            model.trim().to_ascii_lowercase(),
            request.source_format.trim().to_ascii_lowercase(),
            session_id,
        ]
        .join("\0");
        set_json_string(
            translated,
            "prompt_cache_key",
            &Uuid::new_v5(&Uuid::NAMESPACE_OID, identity.as_bytes()).to_string(),
        )
    }

    async fn execute_non_stream(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        let client = require_http_client(request.http_client.clone())?;
        let endpoint_path = openai_compat_image_endpoint_path(&request);
        let (base_url, _) = Self::resolve_credentials(&request.auth_attributes);
        if base_url.is_empty() {
            return Err(Arc::new(OpenAiCompatError::status(
                401,
                "missing provider baseURL",
            )));
        }
        let base_model = parse_suffix(&request.model).model_name;
        let is_image = !endpoint_path.is_empty();
        let to = if request.alt == "responses/compact" {
            Format::from("openai-response")
        } else {
            Format::from("openai")
        };
        let endpoint = if is_image {
            endpoint_path
        } else if request.alt == "responses/compact" {
            "/responses/compact"
        } else {
            "/chat/completions"
        };
        let (mut payload, content_type) = if is_image {
            prepare_openai_compat_images_payload(
                &request.payload,
                &base_model,
                header_value(&request.headers, "Content-Type").unwrap_or(""),
                false,
            )?
        } else {
            let mut translated = self.translate_request(&request, &to, request.stream);
            if request.alt == "responses/compact" {
                translated = remove_json_field(&translated, "stream");
                translated = sanitize_openai_responses_reasoning_encrypted_content(
                    "openai compat executor",
                    &translated,
                )
                .into_owned();
            } else {
                translated = self.apply_prompt_cache_key(&request, &translated);
            }
            (translated, "application/json".to_owned())
        };
        if payload.is_empty() {
            payload = request.payload.clone();
        }
        let mut upstream = HttpRequest {
            method: "POST".into(),
            url: join_url(base_url, endpoint),
            body: payload.clone(),
            ..HttpRequest::default()
        };
        set_header(&mut upstream.headers, "Content-Type", content_type);
        set_header(
            &mut upstream.headers,
            "User-Agent",
            "cli-proxy-openai-compat",
        );
        self.prepare_request(&mut upstream, &request.auth_attributes);
        let response = client.execute(upstream).await?;
        if !(200..300).contains(&response.status_code) {
            return Err(Arc::new(OpenAiCompatError::status(
                response.status_code,
                super::helps::summarize_error_body(
                    header_value(&response.headers, "Content-Type").unwrap_or(""),
                    &response.body,
                ),
            )));
        }
        let output = if is_image || request.alt == "responses/compact" {
            response.body
        } else {
            let from = to;
            let response_format = response_format(&request);
            let mut state: TranslationState = None;
            self.registry.translate_non_stream(
                &TranslationContext::default(),
                &from,
                &response_format,
                &request.model,
                &request.original_request,
                &payload,
                &response.body,
                &mut state,
            )
        };
        Ok(ExecutorResponse {
            payload: output,
            headers: response.headers,
            ..ExecutorResponse::default()
        })
    }

    async fn execute_stream_inner(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorStreamResponse, PluginExecutionError> {
        let client = require_http_client(request.http_client.clone())?;
        let endpoint_path = openai_compat_image_endpoint_path(&request);
        let (base_url, _) = Self::resolve_credentials(&request.auth_attributes);
        if base_url.is_empty() {
            return Err(Arc::new(OpenAiCompatError::status(
                401,
                "missing provider baseURL",
            )));
        }
        let base_model = parse_suffix(&request.model).model_name;
        let is_image = !endpoint_path.is_empty();
        let (mut payload, content_type) = if is_image {
            prepare_openai_compat_images_payload(
                &request.payload,
                &base_model,
                header_value(&request.headers, "Content-Type").unwrap_or(""),
                true,
            )?
        } else {
            let mut translated = self.translate_request(&request, &Format::from("openai"), true);
            if request.alt != "responses/compact" {
                translated = self.apply_prompt_cache_key(&request, &translated);
            }
            (
                set_json_bool(&translated, "stream_options.include_usage", true),
                "application/json".into(),
            )
        };
        if payload.is_empty() {
            payload = request.payload.clone();
        }
        let endpoint = if is_image {
            endpoint_path
        } else {
            "/chat/completions"
        };
        let mut upstream = HttpRequest {
            method: "POST".into(),
            url: join_url(base_url, endpoint),
            body: payload.clone(),
            ..HttpRequest::default()
        };
        set_header(&mut upstream.headers, "Content-Type", content_type);
        set_header(&mut upstream.headers, "Accept", "text/event-stream");
        set_header(&mut upstream.headers, "Cache-Control", "no-cache");
        set_header(
            &mut upstream.headers,
            "User-Agent",
            "cli-proxy-openai-compat",
        );
        self.prepare_request(&mut upstream, &request.auth_attributes);
        let mut response = client.execute_stream(upstream).await?;
        if !(200..300).contains(&response.status_code) {
            let body = collect_http_stream_body(&mut response.chunks).await;
            return Err(Arc::new(OpenAiCompatError::status(
                response.status_code,
                super::helps::summarize_error_body(
                    header_value(&response.headers, "Content-Type").unwrap_or(""),
                    &body,
                ),
            )));
        }
        if is_image {
            return Ok(ExecutorStreamResponse {
                headers: response.headers,
                chunks: bridge_raw_stream(response.chunks),
            });
        }
        let response_headers = response.headers.clone();
        let mut response_chunks = response.chunks;
        let registry = self.registry.clone();
        let model = request.model.clone();
        let original = request.original_request.clone();
        let from = Format::from("openai");
        let response_format = response_format(&request);
        let source_format = Format::from(request.source_format.as_str());
        let mut claude_input_tokens = super::helps::ClaudeInputTokenState::new(
            &source_format,
            &from,
            &response_format,
            &original,
        );
        let (sender, receiver) = mpsc::channel(16);
        tokio::spawn(async move {
            let mut pending = Vec::new();
            let mut state: TranslationState = None;
            while let Some(chunk) = response_chunks.recv().await {
                if let Some(error) = chunk.error {
                    let _ = sender
                        .send(ExecutorStreamChunk {
                            payload: Vec::new(),
                            error: Some(error),
                        })
                        .await;
                    return;
                }
                pending.extend_from_slice(&chunk.payload);
                if pending.len() > OPENAI_COMPAT_MAX_STREAM_LINE_BYTES {
                    let _ = sender
                        .send(stream_error(
                            502,
                            "OpenAI-compatible stream line is too large",
                        ))
                        .await;
                    return;
                }
                while let Some(index) = pending.iter().position(|byte| *byte == b'\n') {
                    let line = pending.drain(..=index).collect::<Vec<_>>();
                    if !process_stream_line(
                        &sender,
                        &registry,
                        &from,
                        &response_format,
                        &model,
                        &original,
                        &payload,
                        &line,
                        &mut state,
                        &mut claude_input_tokens,
                    )
                    .await
                    {
                        return;
                    }
                }
            }
            if !pending.is_empty()
                && !process_stream_line(
                    &sender,
                    &registry,
                    &from,
                    &response_format,
                    &model,
                    &original,
                    &payload,
                    &pending,
                    &mut state,
                    &mut claude_input_tokens,
                )
                .await
            {
                return;
            }
            let _ = process_stream_line(
                &sender,
                &registry,
                &from,
                &response_format,
                &model,
                &original,
                &payload,
                b"data: [DONE]",
                &mut state,
                &mut claude_input_tokens,
            )
            .await;
        });
        Ok(ExecutorStreamResponse {
            headers: response_headers,
            chunks: receiver,
        })
    }
}

impl ProviderExecutor for OpenAiCompatExecutor {
    fn identifier(&self) -> &str {
        self.identifier()
    }

    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(self.execute_non_stream(request))
    }

    fn execute_stream<'a>(
        &'a self,
        request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(self.execute_stream_inner(request))
    }

    fn count_tokens<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move {
            let base_model = parse_suffix(&request.model).model_name;
            let to = Format::from("openai");
            let translated = self.translate_request(&request, &to, false);
            let count = super::helps::count_openai_chat_tokens_for_model(&base_model, &translated)
                .map_err(plugin_error)?;
            let usage = super::helps::build_openai_usage_json(count);
            let payload = self.registry.translate_token_count(
                &TranslationContext::default(),
                &to,
                &response_format(&request),
                count,
                &usage,
            );
            Ok(ExecutorResponse {
                payload,
                ..ExecutorResponse::default()
            })
        })
    }

    fn http_request<'a>(
        &'a self,
        request: ExecutorHttpRequest,
    ) -> PluginFuture<'a, ExecutorHttpResponse> {
        Box::pin(async move {
            let client = require_http_client(request.http_client.clone())?;
            let mut upstream = HttpRequest {
                method: request.method,
                url: request.url,
                headers: request.headers,
                body: request.body,
            };
            self.prepare_request(&mut upstream, &request.attributes);
            let response = client.execute(upstream).await?;
            Ok(ExecutorHttpResponse {
                status_code: response.status_code,
                headers: response.headers,
                body: response.body,
            })
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenAiCompatError {
    pub status_code: u16,
    pub message: String,
    pub retry_after: Option<Duration>,
}

impl OpenAiCompatError {
    pub fn status(status_code: u16, message: impl Into<String>) -> Self {
        Self {
            status_code,
            message: message.into(),
            retry_after: None,
        }
    }
}

impl fmt::Display for OpenAiCompatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.message.is_empty() {
            write!(formatter, "status {}", self.status_code)
        } else {
            formatter.write_str(&self.message)
        }
    }
}
impl std::error::Error for OpenAiCompatError {}

#[must_use]
pub fn openai_compat_image_endpoint_path(request: &ExecutorRequest) -> &'static str {
    if request.source_format != OPENAI_COMPAT_IMAGE_HANDLER_TYPE {
        return "";
    }
    let path = request
        .metadata
        .get("request_path")
        .and_then(Value::as_str)
        .unwrap_or("");
    if path.ends_with(OPENAI_COMPAT_IMAGES_EDITS_PATH) {
        OPENAI_COMPAT_IMAGES_EDITS_PATH
    } else if path.ends_with(OPENAI_COMPAT_IMAGES_GENERATIONS_PATH) {
        OPENAI_COMPAT_IMAGES_GENERATIONS_PATH
    } else {
        OPENAI_COMPAT_DEFAULT_IMAGE_ENDPOINT
    }
}

pub fn prepare_openai_compat_images_payload(
    payload: &[u8],
    model: &str,
    content_type: &str,
    stream: bool,
) -> Result<(Vec<u8>, String), PluginExecutionError> {
    if let Ok(mut root) = serde_json::from_slice::<Value>(payload) {
        if let Some(object) = root.as_object_mut() {
            if !model.trim().is_empty() {
                object.insert("model".into(), Value::String(model.trim().to_owned()));
            }
            if stream {
                object.insert("stream".into(), Value::Bool(true));
            } else {
                object.remove("stream");
            }
        }
        return Ok((
            serde_json::to_vec(&root).map_err(plugin_error)?,
            "application/json".into(),
        ));
    }
    let Some(boundary) = multipart_boundary(content_type) else {
        if content_type
            .split(';')
            .next()
            .is_some_and(|media| media.trim().to_ascii_lowercase().starts_with("multipart/"))
        {
            return Err(plugin_error("multipart boundary is missing"));
        }
        return Ok((payload.to_vec(), content_type.trim().to_owned()));
    };
    rewrite_openai_compat_images_multipart_payload(payload, model, &boundary, stream)
}

pub fn rewrite_openai_compat_images_multipart_payload(
    payload: &[u8],
    model: &str,
    boundary: &str,
    stream: bool,
) -> Result<(Vec<u8>, String), PluginExecutionError> {
    let parts = parse_multipart(payload, boundary)?;
    let mut digest = Sha256::new();
    digest.update(payload);
    digest.update(model.as_bytes());
    digest.update([u8::from(stream)]);
    let suffix = digest
        .finalize()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let output_boundary = format!("ctox-{suffix}");
    let mut output = Vec::new();
    if !model.trim().is_empty() {
        append_multipart_field(&mut output, &output_boundary, "model", model.trim());
    }
    if stream {
        append_multipart_field(&mut output, &output_boundary, "stream", "true");
    }
    for part in parts {
        if matches!(part.name.as_str(), "model" | "stream") {
            continue;
        }
        output.extend_from_slice(format!("--{output_boundary}\r\n").as_bytes());
        for header in &part.headers {
            output.extend_from_slice(header.as_bytes());
            output.extend_from_slice(b"\r\n");
        }
        output.extend_from_slice(b"\r\n");
        output.extend_from_slice(&part.body);
        output.extend_from_slice(b"\r\n");
    }
    output.extend_from_slice(format!("--{output_boundary}--\r\n").as_bytes());
    Ok((
        output,
        format!("multipart/form-data; boundary={output_boundary}"),
    ))
}

#[derive(Debug)]
struct MultipartPart {
    name: String,
    headers: Vec<String>,
    body: Vec<u8>,
}

fn parse_multipart(
    payload: &[u8],
    boundary: &str,
) -> Result<Vec<MultipartPart>, PluginExecutionError> {
    if boundary.trim().is_empty() {
        return Err(plugin_error("multipart boundary is missing"));
    }
    let marker = format!("--{}", boundary.trim()).into_bytes();
    let mut parts = Vec::new();
    for segment in split_bytes(payload, &marker).into_iter().skip(1) {
        let segment = segment
            .strip_prefix(b"\r\n")
            .or_else(|| segment.strip_prefix(b"\n"))
            .unwrap_or(segment);
        if segment.starts_with(b"--") {
            break;
        }
        let Some(header_end) = find_bytes(segment, b"\r\n\r\n") else {
            continue;
        };
        let headers_raw = &segment[..header_end];
        let mut body = segment[header_end + 4..].to_vec();
        while body.ends_with(b"\r\n") {
            body.truncate(body.len() - 2);
        }
        let headers = String::from_utf8_lossy(headers_raw)
            .split("\r\n")
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let name = headers
            .iter()
            .find(|header| {
                header
                    .to_ascii_lowercase()
                    .starts_with("content-disposition:")
            })
            .and_then(|header| disposition_parameter(header, "name"))
            .unwrap_or_default();
        parts.push(MultipartPart {
            name,
            headers,
            body,
        });
    }
    Ok(parts)
}

fn append_multipart_field(output: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    output.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    output.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes(),
    );
}

fn multipart_boundary(content_type: &str) -> Option<String> {
    let mut parts = content_type.split(';');
    let media = parts.next()?.trim();
    if !media.to_ascii_lowercase().starts_with("multipart/") {
        return None;
    }
    parts.find_map(|part| {
        let (key, value) = part.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("boundary")
            .then(|| value.trim().trim_matches('"').trim_matches('\'').to_owned())
    })
}

fn disposition_parameter(header: &str, key: &str) -> Option<String> {
    header.split(';').skip(1).find_map(|part| {
        let (candidate, value) = part.split_once('=')?;
        candidate
            .trim()
            .eq_ignore_ascii_case(key)
            .then(|| value.trim().trim_matches('"').to_owned())
    })
}

#[must_use]
pub fn should_normalize_openai_tool_results_for_model(
    compat: &OpenAiCompatibility,
    upstream_model: &str,
    requested_model: &str,
) -> bool {
    super::helps::should_normalize_openai_tool_results_for_model(
        Some(compat),
        upstream_model,
        requested_model,
    )
}

#[must_use]
pub fn normalize_openai_tool_results_text_only(payload: &[u8]) -> Vec<u8> {
    super::helps::normalize_openai_tool_results_text_only(payload)
}

fn apply_model_suffix_effort(payload: &[u8], model: &str) -> Vec<u8> {
    let suffix = parse_suffix(model);
    if !suffix.has_suffix || suffix.raw_suffix.trim().is_empty() {
        return payload.to_vec();
    }
    set_json_string(
        payload,
        "reasoning_effort",
        &suffix.raw_suffix.trim().to_ascii_lowercase(),
    )
}

fn provider_session_id(request: &ExecutorRequest) -> String {
    for key in ["execution_session_id", "derived_session_id"] {
        if let Some(value) = request.metadata.get(key).and_then(Value::as_str) {
            if !value.trim().is_empty() {
                return value.trim().to_owned();
            }
        }
    }
    if request.source_format.eq_ignore_ascii_case("claude") {
        if let Some(user_id) = serde_json::from_slice::<Value>(&request.payload)
            .ok()
            .and_then(|root| {
                root.pointer("/metadata/user_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
        {
            if let Ok(metadata) = serde_json::from_str::<Value>(&user_id) {
                if let Some(session_id) = metadata.get("session_id").and_then(Value::as_str) {
                    return session_id.trim().to_owned();
                }
            }
        }
    }
    String::new()
}

fn requested_model(request: &ExecutorRequest) -> &str {
    request
        .metadata
        .get("requested_model")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&request.model)
}

fn response_format(request: &ExecutorRequest) -> Format {
    if request.format.trim().is_empty() {
        Format::from(request.source_format.as_str())
    } else {
        Format::from(request.format.as_str())
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_stream_line(
    sender: &mpsc::Sender<ExecutorStreamChunk>,
    registry: &Registry,
    from: &Format,
    response_format: &Format,
    model: &str,
    original: &[u8],
    translated: &[u8],
    line: &[u8],
    state: &mut TranslationState,
    claude_input_tokens: &mut super::helps::ClaudeInputTokenState,
) -> bool {
    let line = trim_ascii(line);
    if line.is_empty()
        || line.starts_with(b":")
        || line.starts_with(b"event:")
        || line.starts_with(b"id:")
        || line.starts_with(b"retry:")
    {
        return true;
    }
    if !line.starts_with(b"data:") {
        if matches!(line.first(), Some(b'{') | Some(b'[')) {
            let _ = sender
                .send(stream_error(502, String::from_utf8_lossy(line)))
                .await;
            return false;
        }
        return true;
    }
    let chunks = registry.translate_stream(
        &TranslationContext::default(),
        from,
        response_format,
        model,
        original,
        translated,
        line,
        state,
    );
    for payload in claude_input_tokens.apply(chunks) {
        if sender
            .send(ExecutorStreamChunk {
                payload,
                error: None,
            })
            .await
            .is_err()
        {
            return false;
        }
    }
    true
}

fn bridge_raw_stream(
    mut source: mpsc::Receiver<crate::sdk::pluginapi::HttpStreamChunk>,
) -> mpsc::Receiver<ExecutorStreamChunk> {
    let (sender, receiver) = mpsc::channel(16);
    tokio::spawn(async move {
        while let Some(chunk) = source.recv().await {
            if sender
                .send(ExecutorStreamChunk {
                    payload: chunk.payload,
                    error: chunk.error,
                })
                .await
                .is_err()
            {
                return;
            }
        }
    });
    receiver
}

async fn collect_http_stream_body(
    source: &mut mpsc::Receiver<crate::sdk::pluginapi::HttpStreamChunk>,
) -> Vec<u8> {
    let mut body = Vec::new();
    while let Some(chunk) = source.recv().await {
        body.extend_from_slice(&chunk.payload);
    }
    body
}

fn stream_error(status: u16, message: impl Into<String>) -> ExecutorStreamChunk {
    ExecutorStreamChunk {
        payload: Vec::new(),
        error: Some(Arc::new(OpenAiCompatError::status(status, message))),
    }
}

fn require_http_client(
    client: Option<Arc<dyn HostHttpClient>>,
) -> Result<Arc<dyn HostHttpClient>, PluginExecutionError> {
    client.ok_or_else(|| plugin_error("openai compat executor: HTTP client is missing"))
}

fn set_header(headers: &mut BTreeMap<String, Vec<String>>, name: &str, value: impl Into<String>) {
    if let Some(existing) = headers
        .keys()
        .find(|key| key.eq_ignore_ascii_case(name))
        .cloned()
    {
        headers.remove(&existing);
    }
    headers.insert(name.to_owned(), vec![value.into()]);
}

fn header_value<'a>(headers: &'a BTreeMap<String, Vec<String>>, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(String::as_str)
}

fn join_url(base: &str, endpoint: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), endpoint)
}

fn json_string(payload: &[u8], key: &str) -> Option<String> {
    serde_json::from_slice::<Value>(payload)
        .ok()
        .and_then(|root| root.get(key).and_then(Value::as_str).map(str::to_owned))
}

fn set_json_string(payload: &[u8], path: &str, value: &str) -> Vec<u8> {
    super::helps::set_string_if_different(payload.to_vec(), path, value)
}

fn set_json_bool(payload: &[u8], path: &str, value: bool) -> Vec<u8> {
    super::helps::set_bool_if_different(payload.to_vec(), path, value)
}

fn remove_json_field(payload: &[u8], key: &str) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Value>(payload) else {
        return payload.to_vec();
    };
    root.as_object_mut().map(|object| object.remove(key));
    serde_json::to_vec(&root).unwrap_or_else(|_| payload.to_vec())
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

fn split_bytes<'a>(bytes: &'a [u8], separator: &[u8]) -> Vec<&'a [u8]> {
    let mut output = Vec::new();
    let mut remaining = bytes;
    while let Some(index) = find_bytes(remaining, separator) {
        output.push(&remaining[..index]);
        remaining = &remaining[index + separator.len()..];
    }
    output.push(remaining);
    output
}

fn find_bytes(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    (!needle.is_empty())
        .then(|| {
            bytes
                .windows(needle.len())
                .position(|window| window == needle)
        })
        .flatten()
}

fn plugin_error(error: impl fmt::Display) -> PluginExecutionError {
    Arc::new(OpenAiCompatError::status(500, error.to_string()))
}
