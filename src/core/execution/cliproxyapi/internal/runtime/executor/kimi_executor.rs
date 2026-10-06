// ref: internal/runtime/executor/kimi_executor.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Stateless Kimi executor with injected Claude delegation, transport, replay
//! cache, clock, refresh transport, and immutable device identity.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;

use chrono::{SecondsFormat, TimeZone, Utc};
use serde_json::{Map, Value};
use tokio::sync::mpsc;

use crate::internal::auth::kimi::KIMI_API_BASE_URL;
use crate::internal::buildinfo::BuildInfo;
use crate::internal::cache::KimiThinkingReplayCache;
use crate::internal::thinking::{
    map_to_claude_effort, parse_level_suffix, parse_numeric_suffix, parse_special_suffix,
    parse_suffix, ClaudeApplier, KimiApplier, ProviderApplier, ThinkingConfig, ThinkingLevel,
    ThinkingMode,
};
use crate::internal::util::{apply_custom_headers_from_attrs, HeaderRequest};
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamChunk, ExecutorStreamResponse, HostHttpClient, HttpRequest, PluginExecutionError,
    PluginFuture, ProviderExecutor,
};
use crate::sdk::translator::{Format, Registry, TranslationContext, TranslationState};

use super::kimi_thinking_replay::{
    cache_kimi_thinking_replay_response, clear_kimi_thinking_replay_content,
    prepare_kimi_thinking_replay_request, should_clear_kimi_thinking_replay_after_error,
    wrap_kimi_thinking_replay_stream,
};

pub const KIMI_CHAT_COMPLETIONS_URL: &str = "https://api.kimi.com/coding/v1/chat/completions";
pub const KIMI_MESSAGES_COUNT_TOKENS_URL: &str =
    "https://api.kimi.com/coding/v1/messages/count_tokens?beta=true";
pub const KIMI_MAX_STREAM_LINE_BYTES: usize = 1_048_576;

pub trait KimiClaudeDelegate: Send + Sync {
    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse>;
    fn execute_stream<'a>(
        &'a self,
        request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse>;
    fn count_tokens<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse>;
}

pub trait KimiRefreshTransport: Send + Sync {
    fn refresh<'a>(
        &'a self,
        refresh_token: &'a str,
        device_id: &'a str,
    ) -> PluginFuture<'a, KimiRefreshToken>;
}

pub trait KimiClock: Send + Sync {
    fn now_ms(&self) -> i64;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KimiRefreshToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_unix: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KimiDeviceProfile {
    pub hostname: String,
    pub device_model: String,
    pub device_id: String,
    pub build: BuildInfo,
}

impl Default for KimiDeviceProfile {
    fn default() -> Self {
        Self {
            hostname: "unknown".into(),
            device_model: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            device_id: "cli-proxy-api-device".into(),
            build: BuildInfo::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KimiPayloadRule {
    pub models: Vec<String>,
    pub source_format: String,
    pub request_path: String,
    pub params: Map<String, Value>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KimiExecutorConfig {
    pub payload_overrides: Vec<KimiPayloadRule>,
}

pub struct KimiExecutor {
    config: Arc<KimiExecutorConfig>,
    registry: Arc<Registry>,
    claude: Arc<dyn KimiClaudeDelegate>,
    replay_cache: Arc<KimiThinkingReplayCache>,
    clock: Arc<dyn KimiClock>,
    refresh_transport: Option<Arc<dyn KimiRefreshTransport>>,
    device: KimiDeviceProfile,
}

impl KimiExecutor {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        config: Arc<KimiExecutorConfig>,
        registry: Arc<Registry>,
        claude: Arc<dyn KimiClaudeDelegate>,
        replay_cache: Arc<KimiThinkingReplayCache>,
        clock: Arc<dyn KimiClock>,
        refresh_transport: Option<Arc<dyn KimiRefreshTransport>>,
        device: KimiDeviceProfile,
    ) -> Self {
        Self {
            config,
            registry,
            claude,
            replay_cache,
            clock,
            refresh_transport,
            device,
        }
    }

    #[must_use]
    pub fn delegated_claude_is_configured(&self) -> bool {
        Arc::strong_count(&self.claude) > 0
    }

    pub fn prepare_request(
        &self,
        request: &mut HttpRequest,
        auth_metadata: &BTreeMap<String, Value>,
        auth_attributes: &BTreeMap<String, String>,
    ) {
        let token = kimi_credentials(auth_metadata, auth_attributes);
        if !token.is_empty() {
            set_header(
                &mut request.headers,
                "Authorization",
                format!("Bearer {token}"),
            );
        }
        apply_custom_headers(&mut request.headers, auth_attributes);
    }

    async fn execute_inner(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        if request.source_format.eq_ignore_ascii_case("claude") {
            return self.execute_claude(request).await;
        }
        let client = require_http_client(request.http_client.clone())?;
        let mut body = self.prepare_openai_body(&request, false)?;
        body = normalize_kimi_tool_message_links(&body)?;
        let mut upstream = HttpRequest {
            method: "POST".into(),
            url: KIMI_CHAT_COMPLETIONS_URL.into(),
            body: body.clone(),
            ..HttpRequest::default()
        };
        self.apply_kimi_headers_with_auth(&mut upstream, &request, false);
        apply_custom_headers(&mut upstream.headers, &request.auth_attributes);
        let response = client.execute(upstream).await?;
        if !(200..300).contains(&response.status_code) {
            return Err(plugin_error(KimiExecutorError::upstream(
                response.status_code,
                String::from_utf8_lossy(&response.body),
            )));
        }
        let response_format = response_format(&request);
        let mut state: TranslationState = None;
        let payload = self.registry.translate_non_stream(
            &TranslationContext::default(),
            &Format::from("openai"),
            &response_format,
            &request.model,
            &request.original_request,
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

    async fn execute_claude(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        let now_ms = self.clock.now_ms();
        let (mut prepared, scope) =
            prepare_kimi_thinking_replay_request(&self.replay_cache, now_ms, request);
        prepared = self.prepare_claude_delegate_request(prepared)?;
        match self.claude.execute(prepared.clone()).await {
            Ok(mut response) => {
                response.payload = restore_response_model(&response.payload, &prepared.model);
                cache_kimi_thinking_replay_response(
                    &self.replay_cache,
                    now_ms,
                    &scope,
                    &response.payload,
                );
                Ok(response)
            }
            Err(error) => {
                if scope.replay_applied
                    && should_clear_kimi_thinking_replay_after_error(error.as_ref())
                {
                    clear_kimi_thinking_replay_content(&self.replay_cache, now_ms, &scope);
                }
                Err(error)
            }
        }
    }

    async fn execute_stream_inner(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorStreamResponse, PluginExecutionError> {
        if request.source_format.eq_ignore_ascii_case("claude") {
            return self.execute_claude_stream(request).await;
        }
        let client = require_http_client(request.http_client.clone())?;
        let mut body = self.prepare_openai_body(&request, true)?;
        body = set_json_value(&body, "stream_options.include_usage", Value::Bool(true))?;
        body = normalize_kimi_tool_message_links(&body)?;
        let mut upstream = HttpRequest {
            method: "POST".into(),
            url: KIMI_CHAT_COMPLETIONS_URL.into(),
            body: body.clone(),
            ..HttpRequest::default()
        };
        self.apply_kimi_headers_with_auth(&mut upstream, &request, true);
        apply_custom_headers(&mut upstream.headers, &request.auth_attributes);
        let mut response = client.execute_stream(upstream).await?;
        if !(200..300).contains(&response.status_code) {
            let error_body = collect_stream_body(&mut response.chunks).await;
            return Err(plugin_error(KimiExecutorError::upstream(
                response.status_code,
                String::from_utf8_lossy(&error_body),
            )));
        }
        let headers = response.headers.clone();
        let registry = Arc::clone(&self.registry);
        let response_format = response_format(&request);
        let model = request.model.clone();
        let original = request.original_request.clone();
        let mut source = response.chunks;
        let (sender, receiver) = mpsc::channel(16);
        tokio::spawn(async move {
            let mut pending = Vec::new();
            let mut state: TranslationState = None;
            while let Some(chunk) = source.recv().await {
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
                if pending.len() > KIMI_MAX_STREAM_LINE_BYTES {
                    let _ = sender
                        .send(stream_error(KimiExecutorError::StreamLineTooLarge))
                        .await;
                    return;
                }
                while let Some(index) = pending.iter().position(|byte| *byte == b'\n') {
                    let line = pending.drain(..=index).collect::<Vec<_>>();
                    if !translate_stream_line(
                        &sender,
                        &registry,
                        &response_format,
                        &model,
                        &original,
                        &body,
                        &line,
                        &mut state,
                    )
                    .await
                    {
                        return;
                    }
                }
            }
            if !pending.is_empty() {
                let _ = translate_stream_line(
                    &sender,
                    &registry,
                    &response_format,
                    &model,
                    &original,
                    &body,
                    &pending,
                    &mut state,
                )
                .await;
            }
            let _ = translate_stream_line(
                &sender,
                &registry,
                &response_format,
                &model,
                &original,
                &body,
                b"[DONE]",
                &mut state,
            )
            .await;
        });
        Ok(ExecutorStreamResponse {
            headers,
            chunks: receiver,
        })
    }

    async fn execute_claude_stream(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorStreamResponse, PluginExecutionError> {
        let now_ms = self.clock.now_ms();
        let requested_model = request.model.clone();
        let (mut prepared, scope) =
            prepare_kimi_thinking_replay_request(&self.replay_cache, now_ms, request);
        prepared = self.prepare_claude_delegate_request(prepared)?;
        match self.claude.execute_stream(prepared).await {
            Ok(response) => {
                let response = rewrite_claude_stream_model(response, requested_model);
                Ok(wrap_kimi_thinking_replay_stream(
                    Arc::clone(&self.replay_cache),
                    now_ms,
                    response,
                    scope,
                ))
            }
            Err(error) => {
                if scope.replay_applied
                    && should_clear_kimi_thinking_replay_after_error(error.as_ref())
                {
                    clear_kimi_thinking_replay_content(&self.replay_cache, now_ms, &scope);
                }
                Err(error)
            }
        }
    }

    fn prepare_openai_body(
        &self,
        request: &ExecutorRequest,
        stream: bool,
    ) -> Result<Vec<u8>, PluginExecutionError> {
        let from = Format::from(request.source_format.as_str());
        let to = Format::from("openai");
        let base_model = parse_suffix(&request.model).model_name;
        let mut payload = request.payload.clone();
        if request
            .source_format
            .eq_ignore_ascii_case("openai-response")
        {
            payload = super::helps::normalize_apply_patch_responses_request(&payload, None)
                .map_err(|message| {
                    plugin_error(KimiExecutorError::InvalidJson(message.to_owned()))
                })?;
            payload = super::helps::normalize_kimi_responses_input(&payload);
        }
        let mut body = self.registry.translate_request(
            &TranslationContext::default(),
            &from,
            &to,
            &base_model,
            &payload,
            stream,
        );
        body = super::helps::normalize_codex_tool_integer_types_for_executor(
            &body,
            &request.headers,
            "kimi",
        );
        body = set_json_value(
            &body,
            "model",
            Value::String(normalize_kimi_upstream_model(&base_model)),
        )?;
        body = apply_kimi_thinking(&body, &request.model)?;
        Ok(self.apply_payload_overrides(body, request))
    }

    fn prepare_claude_delegate_request(
        &self,
        mut request: ExecutorRequest,
    ) -> Result<ExecutorRequest, PluginExecutionError> {
        let canonical = normalize_kimi_upstream_model(&parse_suffix(&request.model).model_name);
        request.payload = set_json_value(&request.payload, "model", Value::String(canonical))?;
        request.payload = apply_claude_thinking(&request.payload, &request.model)?;
        request
            .auth_attributes
            .insert("base_url".into(), KIMI_API_BASE_URL.into());
        Ok(request)
    }

    fn apply_payload_overrides(&self, mut body: Vec<u8>, request: &ExecutorRequest) -> Vec<u8> {
        let request_path = request
            .metadata
            .get("request_path")
            .and_then(Value::as_str)
            .unwrap_or("");
        for rule in &self.config.payload_overrides {
            if !rule.models.is_empty()
                && !rule.models.iter().any(|model| {
                    parse_suffix(model)
                        .model_name
                        .eq_ignore_ascii_case(&parse_suffix(&request.model).model_name)
                })
            {
                continue;
            }
            if !rule.source_format.trim().is_empty()
                && !rule
                    .source_format
                    .trim()
                    .eq_ignore_ascii_case(&request.source_format)
            {
                continue;
            }
            if !rule.request_path.trim().is_empty() && rule.request_path.trim() != request_path {
                continue;
            }
            for (path, value) in &rule.params {
                body = set_json_value(&body, path, value.clone()).unwrap_or(body);
            }
        }
        body
    }

    fn apply_kimi_headers_with_auth(
        &self,
        request: &mut HttpRequest,
        executor_request: &ExecutorRequest,
        stream: bool,
    ) {
        let token = kimi_credentials(
            &executor_request.auth_metadata,
            &executor_request.auth_attributes,
        );
        let device_id = resolve_kimi_device_id(executor_request)
            .unwrap_or_else(|| self.device.device_id.clone());
        set_header(&mut request.headers, "Content-Type", "application/json");
        set_header(
            &mut request.headers,
            "Authorization",
            format!("Bearer {token}"),
        );
        set_header(
            &mut request.headers,
            "User-Agent",
            format!("CLIProxyAPI/{}", self.device.build.version),
        );
        set_header(&mut request.headers, "X-Msh-Platform", "CLIProxyAPI");
        set_header(
            &mut request.headers,
            "X-Msh-Version",
            self.device.build.version.clone(),
        );
        set_header(
            &mut request.headers,
            "X-Msh-Device-Name",
            self.device.hostname.clone(),
        );
        set_header(
            &mut request.headers,
            "X-Msh-Device-Model",
            self.device.device_model.clone(),
        );
        set_header(&mut request.headers, "X-Msh-Device-Id", device_id);
        set_header(
            &mut request.headers,
            "Accept",
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        );
    }

    pub async fn refresh(
        &self,
        mut auth_metadata: BTreeMap<String, Value>,
        auth_attributes: &BTreeMap<String, String>,
        storage_json: &[u8],
    ) -> Result<BTreeMap<String, Value>, PluginExecutionError> {
        let Some(refresh_token) = auth_metadata
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
        else {
            return Ok(auth_metadata);
        };
        let Some(transport) = &self.refresh_transport else {
            return Err(plugin_error(KimiExecutorError::RefreshUnavailable));
        };
        let device_id = auth_metadata
            .get("device_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| json_owned_string(storage_json, "device_id"))
            .or_else(|| auth_attributes.get("device_id").cloned())
            .unwrap_or_else(|| self.device.device_id.clone())
            .trim()
            .to_owned();
        let token = transport.refresh(refresh_token, &device_id).await?;
        auth_metadata.insert("access_token".into(), Value::String(token.access_token));
        if !token.refresh_token.is_empty() {
            auth_metadata.insert("refresh_token".into(), Value::String(token.refresh_token));
        }
        if let Some(expires_at) = token.expires_at_unix {
            if let Some(timestamp) = Utc.timestamp_opt(expires_at, 0).single() {
                auth_metadata.insert(
                    "expired".into(),
                    Value::String(timestamp.to_rfc3339_opts(SecondsFormat::Secs, true)),
                );
            }
        }
        auth_metadata.insert("type".into(), Value::String("kimi".into()));
        if let Some(timestamp) = Utc.timestamp_millis_opt(self.clock.now_ms()).single() {
            auth_metadata.insert(
                "last_refresh".into(),
                Value::String(timestamp.to_rfc3339_opts(SecondsFormat::Secs, true)),
            );
        }
        Ok(auth_metadata)
    }
}

impl ProviderExecutor for KimiExecutor {
    fn identifier(&self) -> &str {
        "kimi"
    }

    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(self.execute_inner(request))
    }

    fn execute_stream<'a>(
        &'a self,
        request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(self.execute_stream_inner(request))
    }

    fn count_tokens<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move {
            let request = self.prepare_claude_delegate_request(request)?;
            self.claude.count_tokens(request).await
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
            self.prepare_request(&mut upstream, &request.metadata, &request.attributes);
            let response = client.execute(upstream).await?;
            Ok(ExecutorHttpResponse {
                status_code: response.status_code,
                headers: response.headers,
                body: response.body,
            })
        })
    }
}

impl fmt::Debug for KimiExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KimiExecutor")
            .field("provider", &"kimi")
            .field("device", &"[INJECTED]")
            .field("refresh", &self.refresh_transport.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KimiExecutorError {
    InvalidJson(String),
    UpstreamStatus { status: u16, message: String },
    MissingHttpClient,
    StreamLineTooLarge,
    RefreshUnavailable,
}

impl KimiExecutorError {
    fn upstream(status: u16, message: impl Into<String>) -> Self {
        Self::UpstreamStatus {
            status,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::UpstreamStatus { status, .. } => Some(*status),
            _ => None,
        }
    }
}

impl fmt::Display for KimiExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(formatter, "kimi executor: {message}"),
            Self::UpstreamStatus { status, message } if message.is_empty() => {
                write!(formatter, "status {status}")
            }
            Self::UpstreamStatus { message, .. } => formatter.write_str(message),
            Self::MissingHttpClient => formatter.write_str("kimi executor: HTTP client is missing"),
            Self::StreamLineTooLarge => formatter.write_str("kimi stream line exceeds 1 MiB"),
            Self::RefreshUnavailable => {
                formatter.write_str("kimi executor: refresh transport is not configured")
            }
        }
    }
}

impl std::error::Error for KimiExecutorError {}

pub fn normalize_kimi_tool_message_links(body: &[u8]) -> Result<Vec<u8>, PluginExecutionError> {
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return Ok(body.to_vec());
    };
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return Ok(body.to_vec());
    };
    let mut pending = VecDeque::new();
    let mut latest_reasoning: Option<String> = None;
    let mut changed = false;
    messages.retain(|message| {
        let keep = !should_drop_kimi_assistant_message(message);
        changed |= !keep;
        keep
    });
    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        match role {
            "assistant" => {
                if let Some(reasoning) = message
                    .get("reasoning_content")
                    .and_then(Value::as_str)
                    .filter(|reasoning| !reasoning.trim().is_empty())
                {
                    latest_reasoning = Some(reasoning.to_owned());
                }
                let tool_ids = message
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .map(|calls| {
                        calls
                            .iter()
                            .filter_map(|call| call.get("id").and_then(Value::as_str))
                            .map(str::trim)
                            .filter(|id| !id.is_empty())
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if !tool_ids.is_empty() {
                    let missing_reasoning = message
                        .get("reasoning_content")
                        .and_then(Value::as_str)
                        .is_none_or(|reasoning| reasoning.trim().is_empty());
                    if missing_reasoning {
                        let fallback =
                            fallback_assistant_reasoning(message, latest_reasoning.as_deref());
                        message
                            .as_object_mut()
                            .expect("messages contain objects")
                            .insert("reasoning_content".into(), Value::String(fallback));
                        changed = true;
                    }
                    pending.extend(tool_ids);
                }
            }
            "tool" => {
                let mut id = message
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned);
                if id.is_none() {
                    id = message
                        .get("call_id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned);
                }
                if id.is_none() && pending.len() == 1 {
                    id = pending.front().cloned();
                }
                if message
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .is_none_or(|value| value.trim().is_empty())
                {
                    if let Some(id) = &id {
                        message
                            .as_object_mut()
                            .expect("messages contain objects")
                            .insert("tool_call_id".into(), Value::String(id.clone()));
                        changed = true;
                    }
                }
                if let Some(id) = id {
                    if let Some(index) = pending.iter().position(|pending| pending == &id) {
                        pending.remove(index);
                    }
                }
            }
            _ => {}
        }
    }
    if changed {
        serde_json::to_vec(&root)
            .map_err(|error| plugin_error(KimiExecutorError::InvalidJson(error.to_string())))
    } else {
        Ok(body.to_vec())
    }
}

fn should_drop_kimi_assistant_message(message: &Value) -> bool {
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return false;
    }
    if has_kimi_tool_calls(message)
        || has_kimi_legacy_function_call(message)
        || has_kimi_assistant_reasoning(message)
    {
        return false;
    }
    is_kimi_assistant_content_empty(message.get("content"))
}

fn has_kimi_tool_calls(message: &Value) -> bool {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .is_some_and(|calls| !calls.is_empty())
}

fn has_kimi_legacy_function_call(message: &Value) -> bool {
    message.get("function_call").is_some_and(|call| {
        !call.is_null() && call.as_object().is_none_or(|object| !object.is_empty())
    })
}

fn has_kimi_assistant_reasoning(message: &Value) -> bool {
    message
        .get("reasoning_content")
        .and_then(Value::as_str)
        .is_some_and(|reasoning| !reasoning.trim().is_empty())
}

fn is_kimi_assistant_content_empty(content: Option<&Value>) -> bool {
    match content {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => text.trim().is_empty(),
        Some(Value::Array(parts)) => parts.iter().all(is_kimi_assistant_content_part_empty),
        Some(_) => false,
    }
}

fn is_kimi_assistant_content_part_empty(part: &Value) -> bool {
    match part {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        Value::Object(object) => {
            if let Some(text) = object.get("text") {
                return text.as_str().is_none_or(|text| text.trim().is_empty());
            }
            object.get("type").and_then(Value::as_str) == Some("text") || object.is_empty()
        }
        _ => false,
    }
}

fn fallback_assistant_reasoning(message: &Value, latest: Option<&str>) -> String {
    if let Some(latest) = latest.filter(|latest| !latest.trim().is_empty()) {
        return latest.to_owned();
    }
    match message.get("content") {
        Some(Value::String(text)) if !text.trim().is_empty() => text.trim().to_owned(),
        Some(Value::Array(parts)) => {
            let text = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                "[reasoning unavailable]".into()
            } else {
                text
            }
        }
        _ => "[reasoning unavailable]".into(),
    }
}

#[must_use]
pub fn strip_kimi_prefix(model: &str) -> String {
    let model = model.trim();
    if model.len() >= 5 && model[..5].eq_ignore_ascii_case("kimi-") {
        model[5..].to_owned()
    } else {
        model.to_owned()
    }
}

#[must_use]
pub fn normalize_kimi_upstream_model(model: &str) -> String {
    let parsed = parse_suffix(model.trim());
    let mut base = parsed.model_name;
    if base.to_ascii_lowercase().ends_with("[1m]") {
        base.truncate(base.len() - 4);
    }
    let normalized = strip_kimi_prefix(base.trim()).to_ascii_lowercase();
    if parsed.has_suffix {
        format!("{normalized}({})", parsed.raw_suffix)
    } else {
        normalized
    }
}

#[must_use]
pub fn kimi_credentials(
    metadata: &BTreeMap<String, Value>,
    attributes: &BTreeMap<String, String>,
) -> String {
    metadata
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .or_else(|| attributes.get("access_token").map(String::as_str))
        .or_else(|| attributes.get("api_key").map(String::as_str))
        .unwrap_or("")
        .trim()
        .to_owned()
}

fn resolve_kimi_device_id(request: &ExecutorRequest) -> Option<String> {
    request
        .auth_metadata
        .get("device_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| json_owned_string(&request.storage_json, "device_id"))
        .or_else(|| request.auth_attributes.get("device_id").cloned())
        .map(|id| id.trim().to_owned())
        .filter(|id| !id.is_empty())
}

fn apply_kimi_thinking(body: &[u8], model: &str) -> Result<Vec<u8>, PluginExecutionError> {
    let config = suffix_thinking_config(model);
    KimiApplier::new()
        .apply(body, &config, None)
        .map_err(|error| plugin_error(KimiExecutorError::InvalidJson(error.to_string())))
}

fn apply_claude_thinking(body: &[u8], model: &str) -> Result<Vec<u8>, PluginExecutionError> {
    let mut config = suffix_thinking_config(model);
    if config.mode == ThinkingMode::Level {
        if let Some(effort) = map_to_claude_effort(config.level.as_str(), false) {
            config.level = ThinkingLevel::new(effort);
        }
    }
    ClaudeApplier::new()
        .apply(body, &config, None)
        .map_err(|error| plugin_error(KimiExecutorError::InvalidJson(error.to_string())))
}

fn suffix_thinking_config(model: &str) -> ThinkingConfig {
    let parsed = parse_suffix(model);
    if !parsed.has_suffix {
        return ThinkingConfig::default();
    }
    if let Some(level) = parse_level_suffix(&parsed.raw_suffix) {
        return ThinkingConfig {
            mode: ThinkingMode::Level,
            level,
            ..ThinkingConfig::default()
        };
    }
    if let Some(mode) = parse_special_suffix(&parsed.raw_suffix) {
        return ThinkingConfig {
            mode,
            ..ThinkingConfig::default()
        };
    }
    if let Some(budget) = parse_numeric_suffix(&parsed.raw_suffix) {
        return ThinkingConfig {
            mode: ThinkingMode::Budget,
            budget,
            ..ThinkingConfig::default()
        };
    }
    ThinkingConfig {
        mode: ThinkingMode::Level,
        level: ThinkingLevel::new(parsed.raw_suffix),
        ..ThinkingConfig::default()
    }
}

fn restore_response_model(payload: &[u8], model: &str) -> Vec<u8> {
    set_json_value(payload, "model", Value::String(model.to_owned()))
        .unwrap_or_else(|_| payload.to_vec())
}

fn rewrite_claude_stream_model(
    response: ExecutorStreamResponse,
    model: String,
) -> ExecutorStreamResponse {
    let headers = response.headers.clone();
    let mut source = response.chunks;
    let (sender, receiver) = mpsc::channel(16);
    tokio::spawn(async move {
        while let Some(mut chunk) = source.recv().await {
            if chunk.error.is_none() {
                chunk.payload = rewrite_sse_model(&chunk.payload, &model);
            }
            if sender.send(chunk).await.is_err() {
                return;
            }
        }
    });
    ExecutorStreamResponse {
        headers,
        chunks: receiver,
    }
}

fn rewrite_sse_model(payload: &[u8], model: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(payload.len());
    for line in payload.split_inclusive(|byte| *byte == b'\n') {
        let trimmed = trim_ascii(line);
        if let Some(data) = trimmed.strip_prefix(b"data:").map(trim_ascii) {
            if let Ok(mut root) = serde_json::from_slice::<Value>(data) {
                if let Some(message) = root.get_mut("message").and_then(Value::as_object_mut) {
                    if message.contains_key("model") {
                        message.insert("model".into(), Value::String(model.to_owned()));
                    }
                }
                if root.get("model").is_some() {
                    root.as_object_mut()
                        .expect("SSE event is an object")
                        .insert("model".into(), Value::String(model.to_owned()));
                }
                output.extend_from_slice(b"data: ");
                output.extend_from_slice(
                    &serde_json::to_vec(&root).unwrap_or_else(|_| data.to_vec()),
                );
                if line.ends_with(b"\n") {
                    output.push(b'\n');
                }
                continue;
            }
        }
        output.extend_from_slice(line);
    }
    output
}

#[allow(clippy::too_many_arguments)]
async fn translate_stream_line(
    sender: &mpsc::Sender<ExecutorStreamChunk>,
    registry: &Registry,
    response_format: &Format,
    model: &str,
    original: &[u8],
    translated: &[u8],
    line: &[u8],
    state: &mut TranslationState,
) -> bool {
    for payload in registry.translate_stream(
        &TranslationContext::default(),
        &Format::from("openai"),
        response_format,
        model,
        original,
        translated,
        trim_ascii(line),
        state,
    ) {
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

async fn collect_stream_body(
    chunks: &mut mpsc::Receiver<crate::sdk::pluginapi::HttpStreamChunk>,
) -> Vec<u8> {
    let mut output = Vec::new();
    while let Some(chunk) = chunks.recv().await {
        output.extend_from_slice(&chunk.payload);
    }
    output
}

fn set_json_value(body: &[u8], path: &str, value: Value) -> Result<Vec<u8>, PluginExecutionError> {
    let mut root = serde_json::from_slice::<Value>(body)
        .map_err(|error| plugin_error(KimiExecutorError::InvalidJson(error.to_string())))?;
    let keys = path.split('.').collect::<Vec<_>>();
    let mut cursor = &mut root;
    for key in &keys[..keys.len().saturating_sub(1)] {
        if !cursor.get(*key).is_some_and(Value::is_object) {
            let Some(object) = cursor.as_object_mut() else {
                return Err(plugin_error(KimiExecutorError::InvalidJson(format!(
                    "cannot set {path}"
                ))));
            };
            object.insert((*key).to_string(), Value::Object(Map::new()));
        }
        cursor = cursor
            .get_mut(*key)
            .expect("inserted JSON object must exist");
    }
    let Some(last) = keys.last() else {
        return Ok(body.to_vec());
    };
    let Some(object) = cursor.as_object_mut() else {
        return Err(plugin_error(KimiExecutorError::InvalidJson(format!(
            "cannot set {path}"
        ))));
    };
    object.insert((*last).to_owned(), value);
    serde_json::to_vec(&root)
        .map_err(|error| plugin_error(KimiExecutorError::InvalidJson(error.to_string())))
}

fn response_format(request: &ExecutorRequest) -> Format {
    if request.format.trim().is_empty() {
        Format::from(request.source_format.as_str())
    } else {
        Format::from(request.format.as_str())
    }
}

fn json_owned_string(body: &[u8], key: &str) -> Option<String> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|root| root.get(key).and_then(Value::as_str).map(str::to_owned))
}

fn set_header(headers: &mut BTreeMap<String, Vec<String>>, name: &str, value: impl Into<String>) {
    headers.retain(|key, _| !key.eq_ignore_ascii_case(name));
    headers.insert(name.to_owned(), vec![value.into()]);
}

fn apply_custom_headers(
    headers: &mut BTreeMap<String, Vec<String>>,
    attributes: &BTreeMap<String, String>,
) {
    let mut request = HeaderRequest {
        headers: std::mem::take(headers),
        ..HeaderRequest::default()
    };
    apply_custom_headers_from_attrs(&mut request, attributes);
    *headers = request.headers;
}

fn require_http_client(
    client: Option<Arc<dyn HostHttpClient>>,
) -> Result<Arc<dyn HostHttpClient>, PluginExecutionError> {
    client.ok_or_else(|| plugin_error(KimiExecutorError::MissingHttpClient))
}

fn stream_error(error: KimiExecutorError) -> ExecutorStreamChunk {
    ExecutorStreamChunk {
        payload: Vec::new(),
        error: Some(plugin_error(error)),
    }
}

fn plugin_error(error: KimiExecutorError) -> PluginExecutionError {
    Arc::new(error)
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(|byte| byte.is_ascii_whitespace()) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}
