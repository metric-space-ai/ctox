//! Session- and turn-scoped helpers for talking to model provider APIs.
//!
//! `ModelClient` is intended to live for the lifetime of a Codex session and holds the stable
//! configuration and state needed to talk to a provider (auth, provider selection, conversation id,
//! and feature-gated request behavior).
//!
//! Per-turn settings (model selection, reasoning controls, telemetry context, and turn metadata)
//! are passed explicitly to streaming and unary methods so that the turn lifetime is visible at the
//! call site.
//!
//! A [`ModelClientSession`] is created per turn and is used to stream one or more Responses API
//! requests during that turn. It caches a Responses WebSocket connection (opened lazily) and stores
//! per-turn state such as the `x-codex-turn-state` token used for sticky routing.
//!
//! WebSocket prewarm is a v2-only `response.create` with `generate=false`; it waits for completion
//! so the next request can reuse the same connection and `previous_response_id`.
//!
//! Turn execution performs prewarm as a best-effort step before the first stream request so the
//! subsequent request can reuse the same connection.
//!
//! ## Retry-Budget Tradeoff
//!
//! WebSocket prewarm is treated as the first websocket connection attempt for a turn. If it
//! fails, normal stream retry/fallback logic handles recovery on the same turn.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use crate::api_bridge::CoreAuthProvider;
use crate::api_bridge::auth_provider_from_auth;
use crate::api_bridge::map_api_error;
use crate::auth::UnauthorizedRecovery;
use crate::auth_env_telemetry::AuthEnvTelemetry;
use crate::auth_env_telemetry::collect_auth_env_telemetry;
use ctox_api::AuthProvider as ApiAuthProvider;
use ctox_api::CompactClient as ApiCompactClient;
use ctox_api::CompactionInput as ApiCompactionInput;
use ctox_api::MemoriesClient as ApiMemoriesClient;
use ctox_api::MemorySummarizeInput as ApiMemorySummarizeInput;
use ctox_api::MemorySummarizeOutput as ApiMemorySummarizeOutput;
use ctox_api::RawMemory as ApiRawMemory;
use ctox_api::RequestTelemetry;
use ctox_api::ReqwestTransport;
use ctox_api::ResponseCreateWsRequest;
use ctox_api::ResponsesApiRequest;
use ctox_api::ResponsesClient as ApiResponsesClient;
use ctox_api::ResponsesOptions as ApiResponsesOptions;
use ctox_api::ResponsesWebsocketClient as ApiWebSocketResponsesClient;
use ctox_api::ResponsesWebsocketConnection as ApiWebSocketConnection;
use ctox_api::SseTelemetry;
use ctox_api::TransportError;
use ctox_api::WebsocketTelemetry;
use ctox_api::build_conversation_headers;
use ctox_api::common::Reasoning;
use ctox_api::common::ResponsesWsRequest;
use ctox_api::create_text_param_for_request;
use ctox_api::error::ApiError;
use ctox_api::requests::responses::Compression;
use ctox_api::sse::responses::ResponsesStreamEvent;
use ctox_api::sse::responses::process_responses_event;
use ctox_client::HttpTransport as _;
use ctox_client::Request as ApiRequest;
use ctox_otel::SessionTelemetry;

use ctox_protocol::ThreadId;
use ctox_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use ctox_protocol::config_types::ServiceTier;
use ctox_protocol::config_types::Verbosity as VerbosityConfig;
use ctox_protocol::models::ContentItem;
use ctox_protocol::models::LocalShellAction;
use ctox_protocol::models::ResponseItem;
use ctox_protocol::openai_models::ModelInfo;
use ctox_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use ctox_protocol::protocol::{SessionSource, TokenUsage};
use eventsource_stream::Event;
use eventsource_stream::EventStreamError;
use futures::StreamExt;
use http::HeaderMap as ApiHeaderMap;
use http::HeaderValue;
use http::StatusCode as HttpStatusCode;
use reqwest::StatusCode;
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use std::path::Path;
use std::time::Duration;
use std::time::Instant;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::TryRecvError;
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::tungstenite::Message;
use tracing::debug;
use tracing::instrument;
use tracing::trace;
use tracing::warn;

use crate::AuthManager;
use crate::auth::AuthMode;
use crate::auth::CodexAuth;
use crate::auth::RefreshTokenError;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;
use crate::config::Config;
use crate::default_client::build_reqwest_client;
use crate::error::CodexErr;
use crate::error::Result;
use crate::flags::CODEX_RS_SSE_FIXTURE;
use crate::local_responses_transport::connect_local_responses_transport;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::WireApi;
use crate::response_debug_context::extract_response_debug_context;
use crate::response_debug_context::extract_response_debug_context_from_api_error;
use crate::response_debug_context::telemetry_api_error_message;
use crate::response_debug_context::telemetry_transport_error_message;
use crate::tools::spec::create_tools_json_for_responses_api;
use crate::util::FeedbackRequestTags;
use crate::util::emit_feedback_auth_recovery_tags;
use crate::util::emit_feedback_request_tags_with_auth_env;

pub const OPENAI_BETA_HEADER: &str = "OpenAI-Beta";
pub const X_CODEX_TURN_STATE_HEADER: &str = "x-codex-turn-state";
pub const X_CODEX_TURN_METADATA_HEADER: &str = "x-codex-turn-metadata";
pub const X_RESPONSESAPI_INCLUDE_TIMING_METRICS_HEADER: &str =
    "x-responsesapi-include-timing-metrics";
const RESPONSES_WEBSOCKETS_V2_BETA_HEADER_VALUE: &str = "responses_websockets=2026-02-06";
const RESPONSES_ENDPOINT: &str = "/responses";
const RESPONSES_COMPACT_ENDPOINT: &str = "/responses/compact";
const MEMORIES_SUMMARIZE_ENDPOINT: &str = "/memories/trace_summarize";
#[cfg(test)]
pub(crate) const WEBSOCKET_CONNECT_TIMEOUT: Duration =
    Duration::from_millis(crate::model_provider_info::DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_MS);
pub fn ws_version_from_features(config: &Config) -> bool {
    config
        .features
        .enabled(crate::features::Feature::ResponsesWebsockets)
        || config
            .features
            .enabled(crate::features::Feature::ResponsesWebsocketsV2)
}

/// Session-scoped state shared by all [`ModelClient`] clones.
///
/// This is intentionally kept minimal so `ModelClient` does not need to hold a full `Config`. Most
/// configuration is per turn and is passed explicitly to streaming/unary methods.
#[derive(Debug)]
struct ModelClientState {
    auth_manager: Option<Arc<AuthManager>>,
    conversation_id: ThreadId,
    provider: ModelProviderInfo,
    auth_env_telemetry: AuthEnvTelemetry,
    session_source: SessionSource,
    model_verbosity: Option<VerbosityConfig>,
    responses_websockets_enabled_by_feature: bool,
    enable_request_compression: bool,
    include_timing_metrics: bool,
    beta_features_header: Option<String>,
    disable_websockets: AtomicBool,
    cached_websocket_session: StdMutex<WebsocketSession>,
}

/// Resolved API client setup for a single request attempt.
///
/// Keeping this as a single bundle ensures prewarm and normal request paths
/// share the same auth/provider setup flow.
struct CurrentClientSetup {
    auth: Option<CodexAuth>,
    api_provider: ctox_api::Provider,
    api_auth: CoreAuthProvider,
}

#[derive(Clone, Copy)]
struct RequestRouteTelemetry {
    endpoint: &'static str,
}

impl RequestRouteTelemetry {
    fn for_endpoint(endpoint: &'static str) -> Self {
        Self { endpoint }
    }
}

/// A session-scoped client for model-provider API calls.
///
/// This holds configuration and state that should be shared across turns within a Codex session
/// (auth, provider selection, conversation id, feature-gated request behavior, and transport
/// fallback state).
///
/// WebSocket fallback is session-scoped: once a turn activates the HTTP fallback, subsequent turns
/// will also use HTTP for the remainder of the session.
///
/// Turn-scoped settings (model selection, reasoning controls, telemetry context, and turn
/// metadata) are passed explicitly to the relevant methods to keep turn lifetime visible at the
/// call site.
#[derive(Debug, Clone)]
pub struct ModelClient {
    state: Arc<ModelClientState>,
}

/// A turn-scoped streaming session created from a [`ModelClient`].
///
/// The session establishes a Responses WebSocket connection lazily and reuses it across multiple
/// requests within the turn. It also caches per-turn state:
///
/// - The last full request, so subsequent calls can reuse incremental websocket request payloads
///   only when the current request is an incremental extension of the previous one.
/// - The `x-codex-turn-state` sticky-routing token, which must be replayed for all requests within
///   the same turn.
///
/// Create a fresh `ModelClientSession` for each Codex turn. Reusing it across turns would replay
/// the previous turn's sticky-routing token into the next turn, which violates the client/server
/// contract and can cause routing bugs.
pub struct ModelClientSession {
    client: ModelClient,
    websocket_session: WebsocketSession,
    /// Turn state for sticky routing.
    ///
    /// This is an `OnceLock` that stores the turn state value received from the server
    /// on turn start via the `x-codex-turn-state` response header. Once set, this value
    /// should be sent back to the server in the `x-codex-turn-state` request header for
    /// all subsequent requests within the same turn to maintain sticky routing.
    ///
    /// This is a contract between the client and server: we receive it at turn start,
    /// keep sending it unchanged between turn requests (e.g., for retries, incremental
    /// appends, or continuation requests), and must not send it between different turns.
    turn_state: Arc<OnceLock<String>>,
}

#[derive(Debug, Clone)]
struct LastResponse {
    response_id: String,
    items_added: Vec<ResponseItem>,
}

#[derive(Debug, Default)]
struct WebsocketSession {
    connection: Option<ApiWebSocketConnection>,
    last_request: Option<ResponsesApiRequest>,
    last_response: Option<LastResponse>,
    last_response_rx: Option<oneshot::Receiver<LastResponse>>,
    connection_reused: StdMutex<bool>,
}

impl WebsocketSession {
    fn set_connection_reused(&self, connection_reused: bool) {
        *self
            .connection_reused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = connection_reused;
    }

    fn connection_reused(&self) -> bool {
        *self
            .connection_reused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

enum WebsocketStreamOutcome {
    Stream(ResponseStream),
    FallbackToHttp,
}

impl ModelClient {
    #[allow(clippy::too_many_arguments)]
    /// Creates a new session-scoped `ModelClient`.
    ///
    /// All arguments are expected to be stable for the lifetime of a Codex session. Per-turn values
    /// are passed to [`ModelClientSession::stream`] (and other turn-scoped methods) explicitly.
    pub fn new(
        auth_manager: Option<Arc<AuthManager>>,
        conversation_id: ThreadId,
        provider: ModelProviderInfo,
        session_source: SessionSource,
        model_verbosity: Option<VerbosityConfig>,
        responses_websockets_enabled_by_feature: bool,
        enable_request_compression: bool,
        include_timing_metrics: bool,
        beta_features_header: Option<String>,
    ) -> Self {
        let ctox_api_key_env_enabled = auth_manager
            .as_ref()
            .is_some_and(|manager| manager.ctox_api_key_env_enabled());
        let auth_env_telemetry = collect_auth_env_telemetry(&provider, ctox_api_key_env_enabled);
        Self {
            state: Arc::new(ModelClientState {
                auth_manager,
                conversation_id,
                provider,
                auth_env_telemetry,
                session_source,
                model_verbosity,
                responses_websockets_enabled_by_feature,
                enable_request_compression,
                include_timing_metrics,
                beta_features_header,
                disable_websockets: AtomicBool::new(false),
                cached_websocket_session: StdMutex::new(WebsocketSession::default()),
            }),
        }
    }

    /// Creates a fresh turn-scoped streaming session.
    ///
    /// This constructor does not perform network I/O itself; the session opens a websocket lazily
    /// when the first stream request is issued.
    pub fn new_session(&self) -> ModelClientSession {
        ModelClientSession {
            client: self.clone(),
            websocket_session: self.take_cached_websocket_session(),
            turn_state: Arc::new(OnceLock::new()),
        }
    }

    fn take_cached_websocket_session(&self) -> WebsocketSession {
        let mut cached_websocket_session = self
            .state
            .cached_websocket_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *cached_websocket_session)
    }

    fn store_cached_websocket_session(&self, websocket_session: WebsocketSession) {
        *self
            .state
            .cached_websocket_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = websocket_session;
    }

    pub(crate) fn force_http_fallback(
        &self,
        session_telemetry: &SessionTelemetry,
        model_info: &ModelInfo,
    ) -> bool {
        let websocket_enabled = self.responses_websocket_enabled(model_info);
        let activated =
            websocket_enabled && !self.state.disable_websockets.swap(true, Ordering::Relaxed);
        if activated {
            warn!("falling back to HTTP");
            session_telemetry.counter(
                "codex.transport.fallback_to_http",
                /*inc*/ 1,
                &[("from_wire_api", "responses_websocket")],
            );
        }

        self.store_cached_websocket_session(WebsocketSession::default());
        activated
    }

    /// Compacts the current conversation history using the Compact endpoint.
    ///
    /// This is a unary call (no streaming) that returns a new list of
    /// `ResponseItem`s representing the compacted transcript.
    ///
    /// The model selection and telemetry context are passed explicitly to keep `ModelClient`
    /// session-scoped.
    pub async fn compact_conversation_history(
        &self,
        prompt: &Prompt,
        model_info: &ModelInfo,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
        session_telemetry: &SessionTelemetry,
    ) -> Result<Vec<ResponseItem>> {
        if prompt.input.is_empty() {
            return Ok(Vec::new());
        }
        let client_setup = self.current_client_setup().await?;
        let transport = ReqwestTransport::new(build_reqwest_client());
        let request_telemetry = Self::build_request_telemetry(
            session_telemetry,
            AuthRequestTelemetryContext::new(
                client_setup.auth.as_ref().map(CodexAuth::auth_mode),
                &client_setup.api_auth,
                PendingUnauthorizedRetry::default(),
            ),
            RequestRouteTelemetry::for_endpoint(RESPONSES_COMPACT_ENDPOINT),
            self.state.auth_env_telemetry.clone(),
        );
        let client =
            ApiCompactClient::new(transport, client_setup.api_provider, client_setup.api_auth)
                .with_telemetry(Some(request_telemetry));

        let instructions = prompt.base_instructions.text.clone();
        let input = prompt.get_formatted_input();
        let tools = create_tools_json_for_responses_api(&prompt.tools)?;
        let reasoning = Self::build_reasoning(model_info, effort, summary);
        let verbosity = if model_info.support_verbosity {
            self.state.model_verbosity.or(model_info.default_verbosity)
        } else {
            if self.state.model_verbosity.is_some() {
                warn!(
                    "model_verbosity is set but ignored as the model does not support verbosity: {}",
                    model_info.slug
                );
            }
            None
        };
        let text = create_text_param_for_request(verbosity, &prompt.output_schema);
        let payload = ApiCompactionInput {
            model: &model_info.slug,
            input: &input,
            instructions: &instructions,
            tools,
            parallel_tool_calls: prompt.parallel_tool_calls,
            reasoning,
            text,
        };

        let mut extra_headers = self.build_subagent_headers();
        extra_headers.extend(build_conversation_headers(Some(
            self.state.conversation_id.to_string(),
        )));
        client
            .compact_input(&payload, extra_headers)
            .await
            .map_err(map_api_error)
    }

    /// Builds memory summaries for each provided normalized raw memory.
    ///
    /// This is a unary call (no streaming) to `/v1/memories/trace_summarize`.
    ///
    /// The model selection, reasoning effort, and telemetry context are passed explicitly to keep
    /// `ModelClient` session-scoped.
    pub async fn summarize_memories(
        &self,
        raw_memories: Vec<ApiRawMemory>,
        model_info: &ModelInfo,
        effort: Option<ReasoningEffortConfig>,
        session_telemetry: &SessionTelemetry,
    ) -> Result<Vec<ApiMemorySummarizeOutput>> {
        if raw_memories.is_empty() {
            return Ok(Vec::new());
        }

        let client_setup = self.current_client_setup().await?;
        let transport = ReqwestTransport::new(build_reqwest_client());
        let request_telemetry = Self::build_request_telemetry(
            session_telemetry,
            AuthRequestTelemetryContext::new(
                client_setup.auth.as_ref().map(CodexAuth::auth_mode),
                &client_setup.api_auth,
                PendingUnauthorizedRetry::default(),
            ),
            RequestRouteTelemetry::for_endpoint(MEMORIES_SUMMARIZE_ENDPOINT),
            self.state.auth_env_telemetry.clone(),
        );
        let client =
            ApiMemoriesClient::new(transport, client_setup.api_provider, client_setup.api_auth)
                .with_telemetry(Some(request_telemetry));

        let payload = ApiMemorySummarizeInput {
            model: model_info.slug.clone(),
            raw_memories,
            reasoning: effort.map(|effort| Reasoning {
                effort: Some(effort),
                summary: None,
                exclude: None,
            }),
        };

        client
            .summarize_input(&payload, self.build_subagent_headers())
            .await
            .map_err(map_api_error)
    }

    fn build_subagent_headers(&self) -> ApiHeaderMap {
        let mut extra_headers = ApiHeaderMap::new();
        if let SessionSource::SubAgent(sub) = &self.state.session_source {
            let subagent = match sub {
                crate::protocol::SubAgentSource::Review => "review".to_string(),
                crate::protocol::SubAgentSource::Compact => "compact".to_string(),
                crate::protocol::SubAgentSource::MemoryConsolidation => {
                    "memory_consolidation".to_string()
                }
                crate::protocol::SubAgentSource::ThreadSpawn { .. } => "collab_spawn".to_string(),
                crate::protocol::SubAgentSource::Other(label) => label.clone(),
            };
            if let Ok(val) = HeaderValue::from_str(&subagent) {
                extra_headers.insert("x-openai-subagent", val);
            }
        }
        extra_headers
    }

    /// Builds request telemetry for unary API calls (e.g., Compact endpoint).
    fn build_request_telemetry(
        session_telemetry: &SessionTelemetry,
        auth_context: AuthRequestTelemetryContext,
        request_route_telemetry: RequestRouteTelemetry,
        auth_env_telemetry: AuthEnvTelemetry,
    ) -> Arc<dyn RequestTelemetry> {
        let telemetry = Arc::new(ApiTelemetry::new(
            session_telemetry.clone(),
            auth_context,
            request_route_telemetry,
            auth_env_telemetry,
        ));
        let request_telemetry: Arc<dyn RequestTelemetry> = telemetry;
        request_telemetry
    }

    fn build_reasoning(
        model_info: &ModelInfo,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
    ) -> Option<Reasoning> {
        if model_info.supports_reasoning_summaries {
            Some(Reasoning {
                effort: effort.or(model_info.default_reasoning_level),
                summary: if summary == ReasoningSummaryConfig::None {
                    None
                } else {
                    Some(summary)
                },
                exclude: None,
            })
        } else {
            None
        }
    }

    fn openrouter_kimi_reasoning_override(&self, model_info: &ModelInfo) -> Option<Reasoning> {
        let model = model_info.slug.trim().to_ascii_lowercase();
        if !model.starts_with("moonshotai/kimi-k2.") {
            return None;
        }
        let provider = &self.state.provider;
        let provider_name = provider.name.trim().to_ascii_lowercase();
        let base_url = provider
            .base_url
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if !provider_name.contains("openrouter") && !base_url.contains("openrouter.ai") {
            return None;
        }
        Some(Reasoning {
            effort: Some(ReasoningEffortConfig::None),
            summary: None,
            exclude: Some(true),
        })
    }

    /// Returns whether the Responses-over-WebSocket transport is active for this session.
    ///
    /// This combines provider capability and feature gating; both must be true for websocket paths
    /// to be eligible.
    ///
    /// If websockets are only enabled via model preference (no explicit feature flag), prefer the
    /// current v2 behavior.
    pub fn responses_websocket_enabled(&self, model_info: &ModelInfo) -> bool {
        if !self.state.provider.supports_websockets
            || self.state.disable_websockets.load(Ordering::Relaxed)
        {
            return false;
        }

        self.state.responses_websockets_enabled_by_feature || model_info.prefer_websockets
    }

    /// Returns auth + provider configuration resolved from the current session auth state.
    ///
    /// This centralizes setup used by both prewarm and normal request paths so they stay in
    /// lockstep when auth/provider resolution changes.
    async fn current_client_setup(&self) -> Result<CurrentClientSetup> {
        let auth = match self.state.auth_manager.as_ref() {
            Some(manager) => manager.auth().await,
            None => None,
        };
        let api_provider = self
            .state
            .provider
            .to_api_provider(auth.as_ref().map(CodexAuth::auth_mode))?;
        let api_auth = auth_provider_from_auth(auth.clone(), &self.state.provider)?;
        Ok(CurrentClientSetup {
            auth,
            api_provider,
            api_auth,
        })
    }

    /// Opens a websocket connection using the same header and telemetry wiring as normal turns.
    ///
    /// Both startup prewarm and in-turn `needs_new` reconnects call this path so handshake
    /// behavior remains consistent across both flows.
    #[allow(clippy::too_many_arguments)]
    async fn connect_websocket(
        &self,
        session_telemetry: &SessionTelemetry,
        api_provider: ctox_api::Provider,
        api_auth: CoreAuthProvider,
        turn_state: Option<Arc<OnceLock<String>>>,
        turn_metadata_header: Option<&str>,
        auth_context: AuthRequestTelemetryContext,
        request_route_telemetry: RequestRouteTelemetry,
    ) -> std::result::Result<ApiWebSocketConnection, ApiError> {
        let headers = self.build_websocket_headers(turn_state.as_ref(), turn_metadata_header);
        let websocket_telemetry = ModelClientSession::build_websocket_telemetry(
            session_telemetry,
            auth_context,
            request_route_telemetry,
            self.state.auth_env_telemetry.clone(),
        );
        let websocket_connect_timeout = self.state.provider.websocket_connect_timeout();
        let start = Instant::now();
        let result = match tokio::time::timeout(
            websocket_connect_timeout,
            ApiWebSocketResponsesClient::new(api_provider, api_auth).connect(
                headers,
                crate::default_client::default_headers(),
                turn_state,
                Some(websocket_telemetry),
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(ApiError::Transport(TransportError::Timeout)),
        };
        let error_message = result.as_ref().err().map(telemetry_api_error_message);
        let response_debug = result
            .as_ref()
            .err()
            .map(extract_response_debug_context_from_api_error)
            .unwrap_or_default();
        let status = result.as_ref().err().and_then(api_error_http_status);
        session_telemetry.record_websocket_connect(
            start.elapsed(),
            status,
            error_message.as_deref(),
            auth_context.auth_header_attached,
            auth_context.auth_header_name,
            auth_context.retry_after_unauthorized,
            auth_context.recovery_mode,
            auth_context.recovery_phase,
            request_route_telemetry.endpoint,
            /*connection_reused*/ false,
            response_debug.request_id.as_deref(),
            response_debug.cf_ray.as_deref(),
            response_debug.auth_error.as_deref(),
            response_debug.auth_error_code.as_deref(),
        );
        emit_feedback_request_tags_with_auth_env(
            &FeedbackRequestTags {
                endpoint: request_route_telemetry.endpoint,
                auth_header_attached: auth_context.auth_header_attached,
                auth_header_name: auth_context.auth_header_name,
                auth_mode: auth_context.auth_mode,
                auth_retry_after_unauthorized: Some(auth_context.retry_after_unauthorized),
                auth_recovery_mode: auth_context.recovery_mode,
                auth_recovery_phase: auth_context.recovery_phase,
                auth_connection_reused: Some(false),
                auth_request_id: response_debug.request_id.as_deref(),
                auth_cf_ray: response_debug.cf_ray.as_deref(),
                auth_error: response_debug.auth_error.as_deref(),
                auth_error_code: response_debug.auth_error_code.as_deref(),
                auth_recovery_followup_success: auth_context
                    .retry_after_unauthorized
                    .then_some(result.is_ok()),
                auth_recovery_followup_status: auth_context
                    .retry_after_unauthorized
                    .then_some(status)
                    .flatten(),
            },
            &self.state.auth_env_telemetry,
        );
        result
    }

    /// Builds websocket handshake headers for both prewarm and turn-time reconnect.
    ///
    /// Callers should pass the current turn-state lock when available so sticky-routing state is
    /// replayed on reconnect within the same turn.
    fn build_websocket_headers(
        &self,
        turn_state: Option<&Arc<OnceLock<String>>>,
        turn_metadata_header: Option<&str>,
    ) -> ApiHeaderMap {
        let turn_metadata_header = parse_turn_metadata_header(turn_metadata_header);
        let conversation_id = self.state.conversation_id.to_string();
        let mut headers = build_responses_headers(
            self.state.beta_features_header.as_deref(),
            turn_state,
            turn_metadata_header.as_ref(),
        );
        if let Ok(header_value) = HeaderValue::from_str(&conversation_id) {
            headers.insert("x-client-request-id", header_value);
        }
        headers.extend(build_conversation_headers(Some(conversation_id)));
        headers.insert(
            OPENAI_BETA_HEADER,
            HeaderValue::from_static(RESPONSES_WEBSOCKETS_V2_BETA_HEADER_VALUE),
        );
        if self.state.include_timing_metrics {
            headers.insert(
                X_RESPONSESAPI_INCLUDE_TIMING_METRICS_HEADER,
                HeaderValue::from_static("true"),
            );
        }
        headers
    }
}

impl Drop for ModelClientSession {
    fn drop(&mut self) {
        let websocket_session = std::mem::take(&mut self.websocket_session);
        self.client
            .store_cached_websocket_session(websocket_session);
    }
}

impl ModelClientSession {
    fn reset_websocket_session(&mut self) {
        self.websocket_session.connection = None;
        self.websocket_session.last_request = None;
        self.websocket_session.last_response = None;
        self.websocket_session.last_response_rx = None;
        self.websocket_session
            .set_connection_reused(/*connection_reused*/ false);
    }

    fn build_responses_request(
        &self,
        provider: &ctox_api::Provider,
        prompt: &Prompt,
        model_info: &ModelInfo,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
        service_tier: Option<ServiceTier>,
    ) -> Result<ResponsesApiRequest> {
        let instructions = &prompt.base_instructions.text;
        let input = prompt.get_formatted_input();
        let tools = create_tools_json_for_responses_api(&prompt.tools)?;
        let default_reasoning_effort = model_info.default_reasoning_level;
        let supports_managed_local_reasoning =
            self.client.state.provider.requires_socket_transport()
                && model_info.slug.starts_with("openai/gpt-oss-");
        let reasoning = self
            .client
            .openrouter_kimi_reasoning_override(model_info)
            .or_else(|| {
                if model_info.supports_reasoning_summaries || supports_managed_local_reasoning {
                    Some(Reasoning {
                        effort: effort.or(default_reasoning_effort),
                        summary: if summary == ReasoningSummaryConfig::None {
                            None
                        } else {
                            Some(summary)
                        },
                        exclude: None,
                    })
                } else {
                    None
                }
            });
        let include = if reasoning.is_some() && model_info.supports_reasoning_summaries {
            vec!["reasoning.encrypted_content".to_string()]
        } else {
            Vec::new()
        };
        let verbosity = if model_info.support_verbosity {
            self.client
                .state
                .model_verbosity
                .or(model_info.default_verbosity)
        } else {
            if self.client.state.model_verbosity.is_some() {
                warn!(
                    "model_verbosity is set but ignored as the model does not support verbosity: {}",
                    model_info.slug
                );
            }
            None
        };
        let text = create_text_param_for_request(verbosity, &prompt.output_schema);
        let prompt_cache_key = Some(self.client.state.conversation_id.to_string());
        let request = ResponsesApiRequest {
            model: model_info.slug.clone(),
            instructions: instructions.clone(),
            previous_response_id: None,
            input,
            tools,
            tool_choice: "auto".to_string(),
            parallel_tool_calls: prompt.parallel_tool_calls,
            reasoning,
            max_output_tokens: managed_local_responses_max_output_tokens(
                &self.client.state.provider,
                model_info,
            ),
            store: provider.is_azure_responses_endpoint(),
            stream: true,
            include,
            service_tier: match service_tier {
                Some(ServiceTier::Fast) => Some("priority".to_string()),
                Some(service_tier) => Some(service_tier.to_string()),
                None => None,
            },
            prompt_cache_key,
            text,
        };
        Ok(request)
    }

    #[allow(clippy::too_many_arguments)]
    /// Builds shared Responses API transport options and request-body options.
    ///
    /// Keeping option construction in one place ensures request-scoped headers are consistent
    /// regardless of transport choice.
    fn build_responses_options(
        &self,
        turn_metadata_header: Option<&str>,
        compression: Compression,
    ) -> ApiResponsesOptions {
        let turn_metadata_header = parse_turn_metadata_header(turn_metadata_header);
        let conversation_id = self.client.state.conversation_id.to_string();
        ApiResponsesOptions {
            conversation_id: Some(conversation_id),
            session_source: Some(self.client.state.session_source.clone()),
            extra_headers: build_responses_headers(
                self.client.state.beta_features_header.as_deref(),
                Some(&self.turn_state),
                turn_metadata_header.as_ref(),
            ),
            compression,
            turn_state: Some(Arc::clone(&self.turn_state)),
        }
    }

    fn get_incremental_items(
        &self,
        request: &ResponsesApiRequest,
        last_response: Option<&LastResponse>,
        allow_empty_delta: bool,
    ) -> Option<Vec<ResponseItem>> {
        // Checks whether the current request is an incremental extension of the previous request.
        // We only reuse an incremental input delta when non-input request fields are unchanged and
        // `input` is a strict
        // extension of the previous known input. Server-returned output items are treated as part
        // of the baseline so we do not resend them.
        let previous_request = self.websocket_session.last_request.as_ref()?;
        let mut previous_without_input = previous_request.clone();
        previous_without_input.input.clear();
        previous_without_input.previous_response_id = None;
        let mut request_without_input = request.clone();
        request_without_input.input.clear();
        request_without_input.previous_response_id = None;
        if previous_without_input != request_without_input {
            trace!(
                "incremental request failed, properties didn't match {previous_without_input:?} != {request_without_input:?}"
            );
            return None;
        }

        let mut baseline = previous_request.input.clone();
        if let Some(last_response) = last_response {
            baseline.extend(last_response.items_added.clone());
        }

        let baseline_len = baseline.len();
        if request.input.starts_with(&baseline)
            && (allow_empty_delta || baseline_len < request.input.len())
        {
            Some(request.input[baseline_len..].to_vec())
        } else {
            trace!("incremental request failed, items didn't match");
            None
        }
    }

    fn prepare_http_request(&mut self, request: &ResponsesApiRequest) -> ResponsesApiRequest {
        let Some(last_response) = self.get_last_response() else {
            return request.clone();
        };
        let Some(incremental_items) = self.get_incremental_items(
            request,
            Some(&last_response),
            /*allow_empty_delta*/ true,
        ) else {
            return request.clone();
        };

        if last_response.response_id.is_empty() {
            trace!("incremental request failed, no previous response id");
            return request.clone();
        }

        ResponsesApiRequest {
            previous_response_id: Some(last_response.response_id),
            input: incremental_items,
            ..request.clone()
        }
    }

    fn get_last_response(&mut self) -> Option<LastResponse> {
        if let Some(last_response) = self.websocket_session.last_response.clone() {
            return Some(last_response);
        }

        let mut receiver = self.websocket_session.last_response_rx.take()?;
        match receiver.try_recv() {
            Ok(last_response) => {
                self.websocket_session.last_response = Some(last_response.clone());
                Some(last_response)
            }
            Err(TryRecvError::Empty) => {
                self.websocket_session.last_response_rx = Some(receiver);
                None
            }
            Err(TryRecvError::Closed) => None,
        }
    }

    fn prepare_websocket_request(
        &mut self,
        payload: ResponseCreateWsRequest,
        request: &ResponsesApiRequest,
    ) -> ResponsesWsRequest {
        let Some(last_response) = self.get_last_response() else {
            return ResponsesWsRequest::ResponseCreate(payload);
        };
        let Some(incremental_items) = self.get_incremental_items(
            request,
            Some(&last_response),
            /*allow_empty_delta*/ true,
        ) else {
            return ResponsesWsRequest::ResponseCreate(payload);
        };

        if last_response.response_id.is_empty() {
            trace!("incremental request failed, no previous response id");
            return ResponsesWsRequest::ResponseCreate(payload);
        }

        ResponsesWsRequest::ResponseCreate(ResponseCreateWsRequest {
            previous_response_id: Some(last_response.response_id),
            input: incremental_items,
            ..payload
        })
    }

    /// Opportunistically preconnects a websocket for this turn-scoped client session.
    ///
    /// This performs only connection setup; it never sends prompt payloads.
    pub async fn preconnect_websocket(
        &mut self,
        session_telemetry: &SessionTelemetry,
        model_info: &ModelInfo,
    ) -> std::result::Result<(), ApiError> {
        if !self.client.responses_websocket_enabled(model_info) {
            return Ok(());
        }
        if self.websocket_session.connection.is_some() {
            return Ok(());
        }

        let client_setup = self.client.current_client_setup().await.map_err(|err| {
            ApiError::Stream(format!(
                "failed to build websocket prewarm client setup: {err}"
            ))
        })?;
        let auth_context = AuthRequestTelemetryContext::new(
            client_setup.auth.as_ref().map(CodexAuth::auth_mode),
            &client_setup.api_auth,
            PendingUnauthorizedRetry::default(),
        );
        let connection = self
            .client
            .connect_websocket(
                session_telemetry,
                client_setup.api_provider,
                client_setup.api_auth,
                Some(Arc::clone(&self.turn_state)),
                /*turn_metadata_header*/ None,
                auth_context,
                RequestRouteTelemetry::for_endpoint(RESPONSES_ENDPOINT),
            )
            .await?;
        self.websocket_session.connection = Some(connection);
        self.websocket_session
            .set_connection_reused(/*connection_reused*/ false);
        Ok(())
    }
    /// Returns a websocket connection for this turn.
    #[instrument(
        name = "model_client.websocket_connection",
        level = "info",
        skip_all,
        fields(
            provider = %self.client.state.provider.name,
            wire_api = %self.client.state.provider.wire_api,
            transport = "responses_websocket",
            api.path = "responses",
            turn.has_metadata_header = params.turn_metadata_header.is_some()
        )
    )]
    async fn websocket_connection(
        &mut self,
        params: WebsocketConnectParams<'_>,
    ) -> std::result::Result<&ApiWebSocketConnection, ApiError> {
        let WebsocketConnectParams {
            session_telemetry,
            api_provider,
            api_auth,
            turn_metadata_header,
            options,
            auth_context,
            request_route_telemetry,
        } = params;
        let needs_new = match self.websocket_session.connection.as_ref() {
            Some(conn) => conn.is_closed().await,
            None => true,
        };

        if needs_new {
            self.websocket_session.last_request = None;
            self.websocket_session.last_response = None;
            self.websocket_session.last_response_rx = None;
            let turn_state = options
                .turn_state
                .clone()
                .unwrap_or_else(|| Arc::clone(&self.turn_state));
            let new_conn = match self
                .client
                .connect_websocket(
                    session_telemetry,
                    api_provider,
                    api_auth,
                    Some(turn_state),
                    turn_metadata_header,
                    auth_context,
                    request_route_telemetry,
                )
                .await
            {
                Ok(new_conn) => new_conn,
                Err(err) => {
                    if matches!(err, ApiError::Transport(TransportError::Timeout)) {
                        self.reset_websocket_session();
                    }
                    return Err(err);
                }
            };
            self.websocket_session.connection = Some(new_conn);
            self.websocket_session
                .set_connection_reused(/*connection_reused*/ false);
        } else {
            self.websocket_session
                .set_connection_reused(/*connection_reused*/ true);
        }

        self.websocket_session
            .connection
            .as_ref()
            .ok_or(ApiError::Stream(
                "websocket connection is unavailable".to_string(),
            ))
    }

    fn responses_request_compression(&self, auth: Option<&crate::auth::CodexAuth>) -> Compression {
        if self.client.state.enable_request_compression
            && auth.is_some_and(CodexAuth::is_chatgpt_auth)
            && self.client.state.provider.is_openai()
        {
            Compression::Zstd
        } else {
            Compression::None
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[instrument(
        name = "model_client.stream_responses_local_transport",
        level = "info",
        skip_all,
        fields(
            model = %model_info.slug,
            wire_api = %self.client.state.provider.wire_api,
            transport = "responses_local_transport",
            api.path = "responses_transport",
            turn.has_metadata_header = turn_metadata_header.is_some()
        )
    )]
    async fn stream_responses_local_transport(
        &self,
        endpoint: &str,
        local_request: LocalIpcResponsesRequest,
        model_info: &ModelInfo,
        session_telemetry: &SessionTelemetry,
        turn_metadata_header: Option<&str>,
    ) -> Result<ResponseStream> {
        let endpoint = endpoint.to_string();
        let request_dump_path = local_transport_request_dump_path(&endpoint);
        let connect_timeout = self.client.state.provider.websocket_connect_timeout();
        let idle_timeout = self.client.state.provider.stream_idle_timeout();
        let session_telemetry = session_telemetry.clone();
        let _ = turn_metadata_header;
        let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);

        tokio::spawn(async move {
            let result: std::result::Result<(), ApiError> = async {
                let (reader, mut writer) =
                    connect_local_responses_transport(&endpoint, connect_timeout)
                        .await
                        .map_err(|err| match err.kind() {
                            std::io::ErrorKind::TimedOut => {
                                ApiError::Transport(TransportError::Timeout)
                            }
                            _ => ApiError::Stream(err.to_string()),
                        })?;
                let local_ipc_request = LocalIpcRequest::ResponsesCreate(local_request);
                if let Some(path) = request_dump_path.as_ref() {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Ok(pretty_payload) = serde_json::to_vec_pretty(&local_ipc_request) {
                        let _ = std::fs::write(path, pretty_payload);
                    }
                }
                let payload = serde_json::to_vec(&local_ipc_request)
                    .map_err(|err| ApiError::Stream(err.to_string()))?;
                writer
                    .write_all(&payload)
                    .await
                    .map_err(|err| ApiError::Stream(err.to_string()))?;
                writer
                    .write_all(b"\n")
                    .await
                    .map_err(|err| ApiError::Stream(err.to_string()))?;
                writer
                    .flush()
                    .await
                    .map_err(|err| ApiError::Stream(err.to_string()))?;

                let mut reader = BufReader::new(reader);
                let mut line = String::new();
                let mut last_server_model: Option<String> = None;

                loop {
                    line.clear();
                    let bytes_read =
                        tokio::time::timeout(idle_timeout, reader.read_line(&mut line))
                            .await
                            .map_err(|_| {
                                ApiError::Stream(
                                    "idle timeout waiting for local socket response".into(),
                                )
                            })?
                            .map_err(|err| ApiError::Stream(err.to_string()))?;

                    if bytes_read == 0 {
                        return Err(ApiError::Stream(
                            "stream closed before response.completed".into(),
                        ));
                    }

                    let event_line = line.trim();
                    if event_line.is_empty() {
                        continue;
                    }

                    let event: ResponsesStreamEvent = match serde_json::from_str(event_line) {
                        Ok(event) => event,
                        Err(err) => {
                            debug!("Failed to parse local socket responses event: {err}");
                            continue;
                        }
                    };

                    if let Some(model) = event.response_model()
                        && last_server_model.as_deref() != Some(model.as_str())
                    {
                        if tx_event
                            .send(Ok(ResponseEvent::ServerModel(model.clone())))
                            .await
                            .is_err()
                        {
                            return Ok(());
                        }
                        last_server_model = Some(model);
                    }

                    match process_responses_event(event) {
                        Ok(Some(event)) => {
                            let is_completed = matches!(event, ResponseEvent::Completed { .. });
                            if let ResponseEvent::Completed {
                                token_usage: Some(usage),
                                ..
                            } = &event
                            {
                                session_telemetry.sse_event_completed(
                                    usage.input_tokens,
                                    usage.output_tokens,
                                    Some(usage.cached_input_tokens),
                                    Some(usage.reasoning_output_tokens),
                                    usage.total_tokens,
                                );
                            }
                            if tx_event.send(Ok(event)).await.is_err() {
                                return Ok(());
                            }
                            if is_completed {
                                return Ok(());
                            }
                        }
                        Ok(None) => {}
                        Err(error) => return Err(error.into_api_error()),
                    }
                }
            }
            .await;

            if let Err(err) = result {
                let mapped = map_api_error(err);
                session_telemetry.see_event_completed_failed(&mapped);
                let _ = tx_event.send(Err(mapped)).await;
            }
        });

        Ok(ResponseStream { rx_event })
    }

    /// Streams a turn via the OpenAI Responses API.
    ///
    /// Handles SSE fixtures, reasoning summaries, verbosity, and the
    /// `text` controls used for output schemas.
    #[allow(clippy::too_many_arguments)]
    #[instrument(
        name = "model_client.stream_responses_api",
        level = "info",
        skip_all,
        fields(
            model = %model_info.slug,
            wire_api = %self.client.state.provider.wire_api,
            transport = "responses_http",
            http.method = "POST",
            api.path = "responses",
            turn.has_metadata_header = turn_metadata_header.is_some()
        )
    )]
    async fn stream_responses_api(
        &mut self,
        prompt: &Prompt,
        model_info: &ModelInfo,
        session_telemetry: &SessionTelemetry,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
        service_tier: Option<ServiceTier>,
        turn_metadata_header: Option<&str>,
    ) -> Result<ResponseStream> {
        if let Some(path) = &*CODEX_RS_SSE_FIXTURE {
            warn!(path, "Streaming from fixture");
            let stream = ctox_api::stream_from_fixture(
                path,
                self.client.state.provider.stream_idle_timeout(),
            )
            .map_err(map_api_error)?;
            let (stream, _last_request_rx) = map_response_stream(stream, session_telemetry.clone());
            return Ok(stream);
        }

        let auth_manager = self.client.state.auth_manager.clone();
        let mut auth_recovery = auth_manager
            .as_ref()
            .map(super::auth::AuthManager::unauthorized_recovery);
        let mut pending_retry = PendingUnauthorizedRetry::default();
        loop {
            let client_setup = self.client.current_client_setup().await?;
            let transport = ReqwestTransport::new(build_reqwest_client());
            let request_auth_context = AuthRequestTelemetryContext::new(
                client_setup.auth.as_ref().map(CodexAuth::auth_mode),
                &client_setup.api_auth,
                pending_retry,
            );
            let (request_telemetry, sse_telemetry) = Self::build_streaming_telemetry(
                session_telemetry,
                request_auth_context,
                RequestRouteTelemetry::for_endpoint(RESPONSES_ENDPOINT),
                self.client.state.auth_env_telemetry.clone(),
            );
            let compression = self.responses_request_compression(client_setup.auth.as_ref());
            let options = self.build_responses_options(turn_metadata_header, compression);

            let request = self.build_responses_request(
                &client_setup.api_provider,
                prompt,
                model_info,
                effort,
                summary,
                service_tier,
            )?;
            let wire_request = self.prepare_http_request(&request);
            let client = ApiResponsesClient::new(
                transport,
                client_setup.api_provider,
                client_setup.api_auth,
            )
            .with_telemetry(Some(request_telemetry), Some(sse_telemetry));
            let stream_result = client.stream_request(wire_request, options).await;

            match stream_result {
                Ok(stream) => {
                    self.websocket_session.last_request = Some(request);
                    self.websocket_session.last_response = None;
                    let (stream, last_response_rx) =
                        map_response_stream(stream, session_telemetry.clone());
                    self.websocket_session.last_response_rx = Some(last_response_rx);
                    return Ok(stream);
                }
                Err(ApiError::Transport(
                    unauthorized_transport @ TransportError::Http { status, .. },
                )) if status == StatusCode::UNAUTHORIZED => {
                    pending_retry = PendingUnauthorizedRetry::from_recovery(
                        handle_unauthorized(
                            unauthorized_transport,
                            &mut auth_recovery,
                            session_telemetry,
                        )
                        .await?,
                    );
                    continue;
                }
                Err(err) => return Err(map_api_error(err)),
            }
        }
    }

    async fn stream_anthropic_messages_api(
        &self,
        prompt: &Prompt,
        model_info: &ModelInfo,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
        service_tier: Option<ServiceTier>,
    ) -> Result<ResponseStream> {
        let client_setup = self.client.current_client_setup().await?;
        let api_key = ApiAuthProvider::bearer_token(&client_setup.api_auth)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                CodexErr::InvalidRequest(
                    "Anthropic provider requires an ANTHROPIC_API_KEY credential".to_string(),
                )
            })?;
        let responses_request = self.build_responses_request(
            &client_setup.api_provider,
            prompt,
            model_info,
            effort,
            summary,
            service_tier,
        )?;
        let anthropic_request = build_anthropic_messages_request(&responses_request)?;
        let mut headers = client_setup.api_provider.headers.clone();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&api_key).map_err(|err| {
                CodexErr::InvalidRequest(format!("invalid Anthropic API key header: {err}"))
            })?,
        );
        let mut request = ApiRequest::new(
            http::Method::POST,
            client_setup.api_provider.url_for_path("messages"),
        );
        request.headers = headers;
        request.body = Some(anthropic_request);
        let transport = ReqwestTransport::new(build_reqwest_client());
        let response = transport
            .execute(request)
            .await
            .map_err(|err| map_api_error(ApiError::Transport(err)))?;
        let payload: Value = serde_json::from_slice(&response.body).map_err(|err| {
            CodexErr::Stream(format!("failed to parse Anthropic response: {err}"), None)
        })?;
        build_anthropic_response_stream(payload).await
    }

    /// Streams a turn via the Responses API over WebSocket transport.
    #[allow(clippy::too_many_arguments)]
    #[instrument(
        name = "model_client.stream_responses_websocket",
        level = "info",
        skip_all,
        fields(
            model = %model_info.slug,
            wire_api = %self.client.state.provider.wire_api,
            transport = "responses_websocket",
            api.path = "responses",
            turn.has_metadata_header = turn_metadata_header.is_some(),
            websocket.warmup = warmup
        )
    )]
    async fn stream_responses_websocket(
        &mut self,
        prompt: &Prompt,
        model_info: &ModelInfo,
        session_telemetry: &SessionTelemetry,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
        service_tier: Option<ServiceTier>,
        turn_metadata_header: Option<&str>,
        warmup: bool,
    ) -> Result<WebsocketStreamOutcome> {
        let auth_manager = self.client.state.auth_manager.clone();

        let mut auth_recovery = auth_manager
            .as_ref()
            .map(super::auth::AuthManager::unauthorized_recovery);
        let mut pending_retry = PendingUnauthorizedRetry::default();
        loop {
            let client_setup = self.client.current_client_setup().await?;
            let request_auth_context = AuthRequestTelemetryContext::new(
                client_setup.auth.as_ref().map(CodexAuth::auth_mode),
                &client_setup.api_auth,
                pending_retry,
            );
            let compression = self.responses_request_compression(client_setup.auth.as_ref());

            let options = self.build_responses_options(turn_metadata_header, compression);
            let request = self.build_responses_request(
                &client_setup.api_provider,
                prompt,
                model_info,
                effort,
                summary,
                service_tier,
            )?;
            let mut ws_payload = ResponseCreateWsRequest {
                client_metadata: build_ws_client_metadata(turn_metadata_header),
                ..ResponseCreateWsRequest::from(&request)
            };
            if warmup {
                ws_payload.generate = Some(false);
            }

            match self
                .websocket_connection(WebsocketConnectParams {
                    session_telemetry,
                    api_provider: client_setup.api_provider,
                    api_auth: client_setup.api_auth,
                    turn_metadata_header,
                    options: &options,
                    auth_context: request_auth_context,
                    request_route_telemetry: RequestRouteTelemetry::for_endpoint(
                        RESPONSES_ENDPOINT,
                    ),
                })
                .await
            {
                Ok(_) => {}
                Err(ApiError::Transport(TransportError::Http { status, .. }))
                    if status == StatusCode::UPGRADE_REQUIRED =>
                {
                    return Ok(WebsocketStreamOutcome::FallbackToHttp);
                }
                Err(ApiError::Transport(
                    unauthorized_transport @ TransportError::Http { status, .. },
                )) if status == StatusCode::UNAUTHORIZED => {
                    pending_retry = PendingUnauthorizedRetry::from_recovery(
                        handle_unauthorized(
                            unauthorized_transport,
                            &mut auth_recovery,
                            session_telemetry,
                        )
                        .await?,
                    );
                    continue;
                }
                Err(err) => return Err(map_api_error(err)),
            }

            let ws_request = self.prepare_websocket_request(ws_payload, &request);
            self.websocket_session.last_request = Some(request);
            let stream_result = self.websocket_session.connection.as_ref().ok_or_else(|| {
                map_api_error(ApiError::Stream(
                    "websocket connection is unavailable".to_string(),
                ))
            })?;
            let stream_result = stream_result
                .stream_request(ws_request, self.websocket_session.connection_reused())
                .await
                .map_err(map_api_error)?;
            let (stream, last_request_rx) =
                map_response_stream(stream_result, session_telemetry.clone());
            self.websocket_session.last_response = None;
            self.websocket_session.last_response_rx = Some(last_request_rx);
            return Ok(WebsocketStreamOutcome::Stream(stream));
        }
    }

    /// Builds request and SSE telemetry for streaming API calls.
    fn build_streaming_telemetry(
        session_telemetry: &SessionTelemetry,
        auth_context: AuthRequestTelemetryContext,
        request_route_telemetry: RequestRouteTelemetry,
        auth_env_telemetry: AuthEnvTelemetry,
    ) -> (Arc<dyn RequestTelemetry>, Arc<dyn SseTelemetry>) {
        let telemetry = Arc::new(ApiTelemetry::new(
            session_telemetry.clone(),
            auth_context,
            request_route_telemetry,
            auth_env_telemetry,
        ));
        let request_telemetry: Arc<dyn RequestTelemetry> = telemetry.clone();
        let sse_telemetry: Arc<dyn SseTelemetry> = telemetry;
        (request_telemetry, sse_telemetry)
    }

    /// Builds telemetry for the Responses API WebSocket transport.
    fn build_websocket_telemetry(
        session_telemetry: &SessionTelemetry,
        auth_context: AuthRequestTelemetryContext,
        request_route_telemetry: RequestRouteTelemetry,
        auth_env_telemetry: AuthEnvTelemetry,
    ) -> Arc<dyn WebsocketTelemetry> {
        let telemetry = Arc::new(ApiTelemetry::new(
            session_telemetry.clone(),
            auth_context,
            request_route_telemetry,
            auth_env_telemetry,
        ));
        let websocket_telemetry: Arc<dyn WebsocketTelemetry> = telemetry;
        websocket_telemetry
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prewarm_websocket(
        &mut self,
        prompt: &Prompt,
        model_info: &ModelInfo,
        session_telemetry: &SessionTelemetry,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
        service_tier: Option<ServiceTier>,
        turn_metadata_header: Option<&str>,
    ) -> Result<()> {
        if !self.client.responses_websocket_enabled(model_info) {
            return Ok(());
        }
        if self.websocket_session.last_request.is_some() {
            return Ok(());
        }

        match self
            .stream_responses_websocket(
                prompt,
                model_info,
                session_telemetry,
                effort,
                summary,
                service_tier,
                turn_metadata_header,
                /*warmup*/ true,
            )
            .await
        {
            Ok(WebsocketStreamOutcome::Stream(mut stream)) => {
                // Wait for the v2 warmup request to complete before sending the first turn request.
                while let Some(event) = stream.next().await {
                    match event {
                        Ok(ResponseEvent::Completed { .. }) => break,
                        Err(err) => return Err(err),
                        _ => {}
                    }
                }
                Ok(())
            }
            Ok(WebsocketStreamOutcome::FallbackToHttp) => {
                self.try_switch_fallback_transport(session_telemetry, model_info);
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Streams a single model request within the current turn.
    ///
    /// The caller is responsible for passing per-turn settings explicitly (model selection,
    /// reasoning settings, telemetry context, and turn metadata). This method will prefer the
    /// Responses WebSocket transport when enabled and healthy, and will fall back to the HTTP
    /// Responses API transport otherwise.
    pub async fn stream(
        &mut self,
        prompt: &Prompt,
        model_info: &ModelInfo,
        session_telemetry: &SessionTelemetry,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
        service_tier: Option<ServiceTier>,
        turn_metadata_header: Option<&str>,
    ) -> Result<ResponseStream> {
        let wire_api = self.client.state.provider.wire_api;
        match wire_api {
            WireApi::Responses => {
                let socket_transport_required =
                    self.client.state.provider.requires_socket_transport();
                match self.client.state.provider.transport_endpoint.as_deref() {
                    Some(transport_endpoint) => {
                        let client_setup = self.client.current_client_setup().await?;
                        let request = self.build_responses_request(
                            &client_setup.api_provider,
                            prompt,
                            model_info,
                            effort,
                            summary,
                            service_tier,
                        )?;
                        match build_local_ipc_request(&request) {
                            Ok(mut local_request) => {
                                if socket_transport_required {
                                    local_request.model = "default".to_string();
                                }
                                return self
                                    .stream_responses_local_transport(
                                        transport_endpoint,
                                        local_request,
                                        model_info,
                                        session_telemetry,
                                        turn_metadata_header,
                                    )
                                    .await;
                            }
                            Err(err) if socket_transport_required => {
                                return Err(CodexErr::Stream(
                                    format!(
                                        "CTOX-managed local runtime requires local IPC transport, but the request cannot be represented over that transport: {err}"
                                    ),
                                    None,
                                ));
                            }
                            Err(err) => {
                                warn!(
                                    reason = %err,
                                    "falling back to HTTP responses transport because the local IPC transport cannot represent this request"
                                );
                            }
                        }
                    }
                    None if socket_transport_required => {
                        return Err(CodexErr::Stream(
                            "CTOX-managed local runtime requires local IPC transport, but no transport endpoint is configured".to_string(),
                            None,
                        ));
                    }
                    None => {}
                }

                if self.client.responses_websocket_enabled(model_info) {
                    match self
                        .stream_responses_websocket(
                            prompt,
                            model_info,
                            session_telemetry,
                            effort,
                            summary,
                            service_tier,
                            turn_metadata_header,
                            /*warmup*/ false,
                        )
                        .await?
                    {
                        WebsocketStreamOutcome::Stream(stream) => return Ok(stream),
                        WebsocketStreamOutcome::FallbackToHttp => {
                            self.try_switch_fallback_transport(session_telemetry, model_info);
                        }
                    }
                }

                self.stream_responses_api(
                    prompt,
                    model_info,
                    session_telemetry,
                    effort,
                    summary,
                    service_tier,
                    turn_metadata_header,
                )
                .await
            }
            WireApi::AnthropicMessages => {
                self.stream_anthropic_messages_api(
                    prompt,
                    model_info,
                    effort,
                    summary,
                    service_tier,
                )
                .await
            }
        }
    }

    /// Permanently disables WebSockets for this Codex session and resets WebSocket state.
    ///
    /// This is used after exhausting the provider retry budget, to force subsequent requests onto
    /// the HTTP transport.
    ///
    /// Returns `true` if this call activated fallback, or `false` if fallback was already active.
    pub(crate) fn try_switch_fallback_transport(
        &mut self,
        session_telemetry: &SessionTelemetry,
        model_info: &ModelInfo,
    ) -> bool {
        let activated = self
            .client
            .force_http_fallback(session_telemetry, model_info);
        self.websocket_session = WebsocketSession::default();
        activated
    }
}

/// Parses per-turn metadata into an HTTP header value.
///
/// Invalid values are treated as absent so callers can compare and propagate
/// metadata with the same sanitization path used when constructing headers.
fn parse_turn_metadata_header(turn_metadata_header: Option<&str>) -> Option<HeaderValue> {
    turn_metadata_header.and_then(|value| HeaderValue::from_str(value).ok())
}

fn build_ws_client_metadata(turn_metadata_header: Option<&str>) -> Option<HashMap<String, String>> {
    let turn_metadata_header = parse_turn_metadata_header(turn_metadata_header)?;
    let turn_metadata = turn_metadata_header.to_str().ok()?.to_string();
    let mut client_metadata = HashMap::new();
    client_metadata.insert(X_CODEX_TURN_METADATA_HEADER.to_string(), turn_metadata);
    Some(client_metadata)
}

/// Builds the extra headers attached to Responses API requests.
///
/// These headers implement Codex-specific conventions:
///
/// - `x-codex-beta-features`: comma-separated beta feature keys enabled for the session.
/// - `x-codex-turn-state`: sticky routing token captured earlier in the turn.
/// - `x-codex-turn-metadata`: optional per-turn metadata for observability.
fn build_responses_headers(
    beta_features_header: Option<&str>,
    turn_state: Option<&Arc<OnceLock<String>>>,
    turn_metadata_header: Option<&HeaderValue>,
) -> ApiHeaderMap {
    let mut headers = ApiHeaderMap::new();
    if let Some(value) = beta_features_header
        && !value.is_empty()
        && let Ok(header_value) = HeaderValue::from_str(value)
    {
        headers.insert("x-codex-beta-features", header_value);
    }
    if let Some(turn_state) = turn_state
        && let Some(state) = turn_state.get()
        && let Ok(header_value) = HeaderValue::from_str(state)
    {
        headers.insert(X_CODEX_TURN_STATE_HEADER, header_value);
    }
    if let Some(header_value) = turn_metadata_header {
        headers.insert(X_CODEX_TURN_METADATA_HEADER, header_value.clone());
    }
    headers
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalIpcRequest {
    ResponsesCreate(LocalIpcResponsesRequest),
}

#[derive(Debug, Serialize)]
struct LocalIpcResponsesRequest {
    model: String,
    instructions: String,
    input: Vec<Value>,
    tools: Vec<Value>,
    tool_choice: String,
    parallel_tool_calls: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<Reasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<usize>,
    store: bool,
    stream: bool,
    include: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<ctox_api::common::TextControls>,
}

fn build_local_ipc_request(
    request: &ResponsesApiRequest,
) -> std::result::Result<LocalIpcResponsesRequest, String> {
    Ok(LocalIpcResponsesRequest {
        model: request.model.clone(),
        instructions: request.instructions.clone(),
        input: request
            .input
            .iter()
            .enumerate()
            .filter_map(|(index, item)| map_input_item_for_local_ipc(item, index))
            .collect(),
        tools: normalize_tools_for_local_ipc(&request.tools)?,
        tool_choice: request.tool_choice.clone(),
        // The local engine transport only supports the default parallel tool-call mode.
        parallel_tool_calls: true,
        reasoning: request.reasoning.clone(),
        max_output_tokens: request.max_output_tokens,
        store: request.store,
        stream: request.stream,
        include: request.include.clone(),
        service_tier: request.service_tier.clone(),
        prompt_cache_key: request.prompt_cache_key.clone(),
        text: request.text.clone(),
    })
}

fn managed_local_responses_max_output_tokens(
    provider: &crate::model_provider_info::ModelProviderInfo,
    model_info: &ModelInfo,
) -> Option<usize> {
    if !provider.requires_socket_transport() || !model_info.slug.starts_with("openai/gpt-oss-") {
        return None;
    }
    local_runtime_gpt_oss_max_output_tokens(provider)
}

#[derive(Debug, Deserialize)]
struct LocalRuntimeStateSnapshot {
    #[serde(default)]
    realized_context_tokens: Option<u32>,
    #[serde(default)]
    adapter_tuning: LocalRuntimeAdapterTuningSnapshot,
}

#[derive(Debug, Default, Deserialize)]
struct LocalRuntimeAdapterTuningSnapshot {
    #[serde(default)]
    max_output_tokens_cap: Option<u32>,
}

fn local_runtime_gpt_oss_max_output_tokens(
    provider: &crate::model_provider_info::ModelProviderInfo,
) -> Option<usize> {
    let snapshot = local_runtime_state_snapshot(provider)?;
    let cap = snapshot
        .adapter_tuning
        .max_output_tokens_cap
        .or(snapshot.realized_context_tokens);
    cap.map(|value| value as usize).filter(|value| *value > 0)
}

fn local_runtime_state_snapshot(
    provider: &crate::model_provider_info::ModelProviderInfo,
) -> Option<LocalRuntimeStateSnapshot> {
    let transport_endpoint = provider.transport_endpoint.as_deref()?;
    let ipc_path = std::path::Path::new(transport_endpoint);
    let sockets_dir = ipc_path.parent()?;
    let runtime_dir = sockets_dir.parent()?;
    if sockets_dir.file_name()? != "sockets" || runtime_dir.file_name()? != "runtime" {
        return None;
    }
    let root = runtime_dir.parent()?;
    let db_path = root.join("runtime/ctox.sqlite3");
    let conn = Connection::open(&db_path).ok()?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT state_json FROM runtime_state_store WHERE state_id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .ok()?;
    serde_json::from_str(raw.as_deref()?).ok()
}

fn local_transport_request_dump_path(transport_endpoint: &str) -> Option<std::path::PathBuf> {
    let ipc_path = Path::new(transport_endpoint);
    let sockets_dir = ipc_path.parent()?;
    let runtime_dir = sockets_dir.parent()?;
    if sockets_dir.file_name()? != "sockets" || runtime_dir.file_name()? != "runtime" {
        return None;
    }
    Some(runtime_dir.join("last_local_ipc_request.json"))
}

fn normalize_tools_for_local_ipc(tools: &[Value]) -> std::result::Result<Vec<Value>, String> {
    let mut normalized = Vec::new();
    for tool in tools {
        normalized.extend(normalize_tool_for_local_ipc(tool)?);
    }
    Ok(normalized)
}

fn normalize_tool_for_local_ipc(tool: &Value) -> std::result::Result<Vec<Value>, String> {
    let object = tool
        .as_object()
        .ok_or_else(|| "tool definition must be a JSON object".to_string())?;
    let tool_type = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| "tool definition is missing a string `type`".to_string())?;
    match tool_type {
        "function" => Ok(vec![normalize_function_tool_for_local_ipc(object)?]),
        "namespace" => {
            let mut flattened = Vec::new();
            for child in object
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| "namespace tool is missing its `tools` array".to_string())?
            {
                flattened.extend(normalize_tool_for_local_ipc(child)?);
            }
            Ok(flattened)
        }
        "custom" => {
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| "custom tool is missing `name`".to_string())?;
            let description = object
                .get("description")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("custom tool `{name}` is missing `description`"))?;
            Ok(vec![json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": description,
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "input": {
                                "type": "string",
                                "description": "Raw tool input."
                            }
                        },
                        "required": ["input"],
                        "additionalProperties": false
                    }
                }
            })])
        }
        "local_shell" => Ok(vec![json!({
            "type": "function",
            "function": {
                "name": "local_shell",
                "description": "Runs a shell command and returns its output.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Command argv vector."
                        },
                        "workdir": {
                            "type": "string",
                            "description": "Working directory for the command."
                        },
                        "timeout_ms": {
                            "type": "number",
                            "description": "Timeout in milliseconds."
                        }
                    },
                    "required": ["command"],
                    "additionalProperties": false
                }
            }
        })]),
        "tool_search" => Ok(vec![json!({
            "type": "function",
            "function": {
                "name": "tool_search",
                "description": object
                    .get("description")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "tool_search is missing `description`".to_string())?,
                "parameters": object
                    .get("parameters")
                    .cloned()
                    .ok_or_else(|| "tool_search is missing `parameters`".to_string())?
            }
        })]),
        // CTOX-managed local runtimes only expose function-style tools over the
        // Unix socket. Drop native-only transport tools instead of rejecting the
        // whole request so local Codex turns can still run with the remaining
        // representable tool contract.
        "web_search" | "image_generation" => Ok(Vec::new()),
        _ => Err(format!(
            "tool type `{tool_type}` is not supported over the CTOX local socket"
        )),
    }
}

fn normalize_function_tool_for_local_ipc(
    object: &serde_json::Map<String, Value>,
) -> std::result::Result<Value, String> {
    if let Some(function) = object.get("function").and_then(Value::as_object) {
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "function tool is missing `function.name`".to_string())?;
        return Ok(json!({
            "type": "function",
            "function": {
                "name": name,
                "description": function.get("description").cloned().unwrap_or(Value::String(String::new())),
                "parameters": function.get("parameters").cloned().unwrap_or_else(|| json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": true
                }))
            }
        }));
    }

    let name = object
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "function tool is missing `name`".to_string())?;
    Ok(json!({
        "type": "function",
        "function": {
            "name": name,
            "description": object.get("description").cloned().unwrap_or(Value::String(String::new())),
            "parameters": object.get("parameters").cloned().unwrap_or_else(|| json!({
                "type": "object",
                "properties": {},
                "additionalProperties": true
                }))
        }
    }))
}

fn map_input_item_for_local_ipc(item: &ResponseItem, index: usize) -> Option<Value> {
    match item {
        ResponseItem::Message { role, content, .. } => {
            let parts = content
                .iter()
                .filter_map(map_content_item_for_local_ipc)
                .collect::<Vec<_>>();
            if parts.is_empty() {
                return None;
            }
            let has_images = parts
                .iter()
                .any(|part| part.get("type").and_then(Value::as_str) == Some("input_image"));
            if role == "user" && has_images {
                return Some(json!({
                    "type": "message",
                    "role": "user",
                    "content": parts,
                }));
            }
            let text = render_message_content_for_local_ipc(content);
            if text.trim().is_empty() {
                return None;
            }
            Some(json!({
                "type": "message",
                "role": role,
                "content": text,
            }))
        }
        ResponseItem::Reasoning {
            summary, content, ..
        } => {
            let mut lines = summary
                .iter()
                .filter_map(|item| match item {
                    ctox_protocol::models::ReasoningItemReasoningSummary::SummaryText { text } => {
                        (!text.trim().is_empty()).then_some(text.clone())
                    }
                })
                .collect::<Vec<_>>();
            if let Some(content) = content {
                lines.extend(content.iter().filter_map(|item| match item {
                    ctox_protocol::models::ReasoningItemContent::ReasoningText { text }
                    | ctox_protocol::models::ReasoningItemContent::Text { text } => {
                        (!text.trim().is_empty()).then_some(text.clone())
                    }
                }));
            }
            let text = lines.join("\n");
            (!text.trim().is_empty()).then_some(json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "input_text", "text": text}],
            }))
        }
        ResponseItem::FunctionCall {
            name,
            arguments,
            call_id,
            ..
        } => Some(json!({
            "type": "function_call",
            "call_id": call_id,
            "name": name,
            "arguments": arguments,
        })),
        ResponseItem::FunctionCallOutput { call_id, output }
        | ResponseItem::CustomToolCallOutput { call_id, output } => Some(json!({
            "type": "function_call_output",
            "call_id": call_id,
            "output": render_output_for_local_ipc(output),
        })),
        ResponseItem::CustomToolCall {
            call_id,
            name,
            input,
            ..
        } => Some(json!({
            "type": "function_call",
            "call_id": call_id,
            "name": name,
            "arguments": normalize_raw_tool_arguments(input),
        })),
        ResponseItem::LocalShellCall {
            call_id,
            id,
            action,
            ..
        } => Some(json!({
            "type": "function_call",
            "call_id": call_id.clone().or_else(|| id.clone()).unwrap_or_else(|| format!("local_shell_{index}")),
            "name": "local_shell",
            "arguments": render_local_shell_arguments(action),
        })),
        ResponseItem::ToolSearchCall {
            call_id,
            execution,
            arguments,
            status,
            ..
        } => Some(json!({
            "type": "function_call",
            "call_id": call_id.clone().unwrap_or_else(|| format!("tool_search_{index}")),
            "name": "tool_search",
            "arguments": json!({
                "query": arguments.get("query").cloned().or_else(|| arguments.get("q").cloned()).unwrap_or_else(|| Value::String(arguments.to_string())),
                "limit": arguments.get("limit").cloned(),
                "execution": execution,
                "status": status,
            }).to_string(),
        })),
        ResponseItem::ToolSearchOutput {
            call_id,
            status,
            execution,
            tools,
        } => Some(json!({
            "type": "function_call_output",
            "call_id": call_id.clone().unwrap_or_else(|| format!("tool_search_{index}")),
            "output": json!({
                "status": status,
                "execution": execution,
                "tools": tools,
            }).to_string(),
        })),
        ResponseItem::WebSearchCall { action, .. } => {
            let text = action
                .as_ref()
                .map(render_web_search_action_for_local_ipc)
                .unwrap_or_else(|| "Web search call".to_string());
            Some(json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "input_text", "text": text}],
            }))
        }
        ResponseItem::ImageGenerationCall {
            revised_prompt,
            result,
            ..
        } => Some(json!({
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "input_text",
                "text": revised_prompt.clone().unwrap_or_else(|| result.clone()),
            }],
        })),
        ResponseItem::GhostSnapshot { .. }
        | ResponseItem::Compaction { .. }
        | ResponseItem::Other => None,
    }
}

fn map_content_item_for_local_ipc(item: &ContentItem) -> Option<Value> {
    match item {
        ContentItem::InputText { text } | ContentItem::OutputText { text } => Some(json!({
            "type": "input_text",
            "text": text,
        })),
        ContentItem::InputImage { image_url } => Some(json!({
            "type": "input_image",
            "image_url": image_url,
        })),
    }
}

fn render_message_content_for_local_ipc(content: &[ContentItem]) -> String {
    let mut chunks = Vec::new();
    for item in content {
        match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                if !text.trim().is_empty() {
                    chunks.push(text.clone());
                }
            }
            ContentItem::InputImage { .. } => {
                chunks.push("[image omitted for non-user message]".to_string());
            }
        }
    }
    chunks.join("\n")
}

fn render_output_for_local_ipc(
    output: &ctox_protocol::models::FunctionCallOutputPayload,
) -> String {
    output
        .body
        .to_text()
        .or_else(|| serde_json::to_string(&output.body).ok())
        .unwrap_or_default()
}

fn normalize_raw_tool_arguments(raw: &str) -> String {
    let parsed = serde_json::from_str::<Value>(raw).ok();
    if parsed.as_ref().is_some_and(Value::is_object) {
        raw.to_string()
    } else {
        json!({ "input": raw }).to_string()
    }
}

fn render_local_shell_arguments(action: &LocalShellAction) -> String {
    match action {
        LocalShellAction::Exec(exec) => json!({
            "command": exec.command,
            "workdir": exec.working_directory,
            "timeout_ms": exec.timeout_ms,
        })
        .to_string(),
    }
}

fn render_web_search_action_for_local_ipc(
    action: &ctox_protocol::models::WebSearchAction,
) -> String {
    match action {
        ctox_protocol::models::WebSearchAction::Search { query, queries } => query
            .clone()
            .or_else(|| queries.as_ref().map(|items| items.join(", ")))
            .unwrap_or_else(|| "Web search".to_string()),
        ctox_protocol::models::WebSearchAction::OpenPage { url } => url
            .clone()
            .map(|url| format!("Opened page: {url}"))
            .unwrap_or_else(|| "Opened page".to_string()),
        ctox_protocol::models::WebSearchAction::FindInPage { url, pattern } => {
            match (url, pattern) {
                (Some(url), Some(pattern)) => format!("Find in page {url}: {pattern}"),
                (Some(url), None) => format!("Find in page {url}"),
                (None, Some(pattern)) => format!("Find in page: {pattern}"),
                (None, None) => "Find in page".to_string(),
            }
        }
        ctox_protocol::models::WebSearchAction::Other => "Web search call".to_string(),
    }
}

fn build_anthropic_messages_request(request: &ResponsesApiRequest) -> Result<Value> {
    let mut system_parts = Vec::new();
    if !request.instructions.trim().is_empty() {
        system_parts.push(request.instructions.trim().to_string());
    }
    let mut messages = Vec::new();
    for item in &request.input {
        match item {
            ResponseItem::Message { role, content, .. } => match role.as_str() {
                "system" | "developer" => {
                    let text = content_items_to_text(content);
                    if !text.trim().is_empty() {
                        system_parts.push(text);
                    }
                }
                "assistant" => push_anthropic_message(
                    &mut messages,
                    "assistant",
                    anthropic_content_blocks(content, false),
                ),
                _ => push_anthropic_message(
                    &mut messages,
                    "user",
                    anthropic_content_blocks(content, true),
                ),
            },
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            }
            | ResponseItem::CustomToolCall {
                name,
                input: arguments,
                call_id,
                ..
            } => {
                let input = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}));
                push_anthropic_message(
                    &mut messages,
                    "assistant",
                    vec![json!({
                        "type": "tool_use",
                        "id": call_id,
                        "name": name,
                        "input": input,
                    })],
                );
            }
            ResponseItem::FunctionCallOutput { call_id, output }
            | ResponseItem::CustomToolCallOutput { call_id, output } => {
                push_anthropic_message(
                    &mut messages,
                    "user",
                    vec![json!({
                        "type": "tool_result",
                        "tool_use_id": call_id,
                        "content": output.body.to_text().unwrap_or_default(),
                    })],
                );
            }
            _ => {}
        }
    }
    if messages.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": [{"type": "text", "text": ""}],
        }));
    }

    let mut body = serde_json::Map::new();
    body.insert("model".to_string(), Value::String(request.model.clone()));
    body.insert("messages".to_string(), Value::Array(messages));
    body.insert(
        "max_tokens".to_string(),
        Value::from(request.max_output_tokens.unwrap_or(4096)),
    );
    if !system_parts.is_empty() {
        body.insert(
            "system".to_string(),
            Value::String(system_parts.join("\n\n")),
        );
    }
    let tools = request
        .tools
        .iter()
        .flat_map(|tool| rewrite_anthropic_tool(tool.clone()))
        .collect::<Vec<_>>();
    if !tools.is_empty() {
        body.insert("tools".to_string(), Value::Array(tools));
    }
    if let Some(tool_choice) = rewrite_anthropic_tool_choice(&request.tool_choice) {
        body.insert("tool_choice".to_string(), tool_choice);
    }
    body.insert("stream".to_string(), Value::Bool(false));
    Ok(Value::Object(body))
}

fn content_items_to_text(content: &[ContentItem]) -> String {
    content
        .iter()
        .filter_map(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                Some(text.as_str())
            }
            ContentItem::InputImage { .. } => None,
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn anthropic_content_blocks(content: &[ContentItem], include_images: bool) -> Vec<Value> {
    let mut blocks = Vec::new();
    for item in content {
        match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                if !text.trim().is_empty() {
                    blocks.push(json!({"type": "text", "text": text}));
                }
            }
            ContentItem::InputImage { image_url } if include_images => {
                blocks.push(json!({
                    "type": "image",
                    "source": anthropic_image_source(image_url),
                }));
            }
            ContentItem::InputImage { .. } => {}
        }
    }
    blocks
}

fn anthropic_image_source(url: &str) -> Value {
    if let Some(rest) = url.strip_prefix("data:") {
        if let Some((mime, data)) = rest.split_once(";base64,") {
            return json!({
                "type": "base64",
                "media_type": mime,
                "data": data,
            });
        }
    }
    json!({
        "type": "url",
        "url": url,
    })
}

fn push_anthropic_message(messages: &mut Vec<Value>, role: &str, mut content: Vec<Value>) {
    if content.is_empty() {
        return;
    }
    if let Some(last) = messages.last_mut() {
        if last.get("role").and_then(Value::as_str) == Some(role) {
            if let Some(existing) = last.get_mut("content").and_then(Value::as_array_mut) {
                existing.append(&mut content);
                return;
            }
        }
    }
    messages.push(json!({
        "role": role,
        "content": content,
    }));
}

fn rewrite_anthropic_tool_choice(tool_choice: &str) -> Option<Value> {
    match tool_choice {
        "none" => Some(json!({"type": "none"})),
        "required" => Some(json!({"type": "any"})),
        "auto" => Some(json!({"type": "auto"})),
        _ => None,
    }
}

fn rewrite_anthropic_tool(tool: Value) -> Vec<Value> {
    let Some(object) = tool.as_object() else {
        return Vec::new();
    };
    let Some(tool_type) = object.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };
    match tool_type {
        "function" => {
            let function = object
                .get("function")
                .and_then(Value::as_object)
                .unwrap_or(object);
            let Some(name) = function.get("name").and_then(Value::as_str) else {
                return Vec::new();
            };
            vec![json!({
                "name": name,
                "description": function
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "input_schema": function
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
            })]
        }
        "namespace" => object
            .get("tools")
            .and_then(Value::as_array)
            .map(|children| {
                children
                    .iter()
                    .flat_map(|child| rewrite_anthropic_tool(child.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

async fn build_anthropic_response_stream(payload: Value) -> Result<ResponseStream> {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(32);
    let response_id = payload
        .get("id")
        .and_then(Value::as_str)
        .map(|value| format!("resp_{value}"))
        .unwrap_or_else(|| "resp_anthropic_messages".to_string());
    let message_id = payload
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("msg_anthropic_messages")
        .to_string();
    let mut text_parts = Vec::new();
    let mut output_items = Vec::new();
    if let Some(content) = payload.get("content").and_then(Value::as_array) {
        for block in content {
            match block
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
            {
                "text" => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        if !text.trim().is_empty() {
                            text_parts.push(text.to_string());
                        }
                    }
                }
                "tool_use" => {
                    let call_id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("call_anthropic_messages")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let arguments = block
                        .get("input")
                        .map(Value::to_string)
                        .unwrap_or_else(|| "{}".to_string());
                    output_items.push(ResponseItem::FunctionCall {
                        id: Some(call_id.clone()),
                        name,
                        namespace: None,
                        arguments,
                        call_id,
                    });
                }
                _ => {}
            }
        }
    }
    if !text_parts.is_empty() {
        output_items.insert(
            0,
            ResponseItem::Message {
                id: Some(message_id),
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: text_parts.join(""),
                }],
                end_turn: Some(true),
                phase: None,
            },
        );
    }
    let usage = payload.get("usage").map(|usage| {
        let input_tokens = usage
            .get("input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let output_tokens = usage
            .get("output_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        TokenUsage {
            input_tokens,
            cached_input_tokens: 0,
            output_tokens,
            reasoning_output_tokens: 0,
            total_tokens: input_tokens + output_tokens,
        }
    });
    let _ = tx_event.send(Ok(ResponseEvent::Created)).await;
    for text in text_parts {
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputTextDelta(text)))
            .await;
    }
    for item in output_items {
        let _ = tx_event.send(Ok(ResponseEvent::OutputItemDone(item))).await;
    }
    let _ = tx_event
        .send(Ok(ResponseEvent::Completed {
            response_id,
            token_usage: usage,
        }))
        .await;
    Ok(ResponseStream { rx_event })
}

fn map_response_stream<S>(
    api_stream: S,
    session_telemetry: SessionTelemetry,
) -> (ResponseStream, oneshot::Receiver<LastResponse>)
where
    S: futures::Stream<Item = std::result::Result<ResponseEvent, ApiError>>
        + Unpin
        + Send
        + 'static,
{
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);
    let (tx_last_response, rx_last_response) = oneshot::channel::<LastResponse>();

    tokio::spawn(async move {
        let mut logged_error = false;
        let mut tx_last_response = Some(tx_last_response);
        let mut items_added: Vec<ResponseItem> = Vec::new();
        let mut api_stream = api_stream;
        while let Some(event) = api_stream.next().await {
            match event {
                Ok(ResponseEvent::OutputItemDone(item)) => {
                    items_added.push(item.clone());
                    if tx_event
                        .send(Ok(ResponseEvent::OutputItemDone(item)))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Ok(ResponseEvent::Completed {
                    response_id,
                    token_usage,
                }) => {
                    if let Some(usage) = &token_usage {
                        session_telemetry.sse_event_completed(
                            usage.input_tokens,
                            usage.output_tokens,
                            Some(usage.cached_input_tokens),
                            Some(usage.reasoning_output_tokens),
                            usage.total_tokens,
                        );
                    }
                    if let Some(sender) = tx_last_response.take() {
                        let _ = sender.send(LastResponse {
                            response_id: response_id.clone(),
                            items_added: std::mem::take(&mut items_added),
                        });
                    }
                    if tx_event
                        .send(Ok(ResponseEvent::Completed {
                            response_id,
                            token_usage,
                        }))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Ok(event) => {
                    if tx_event.send(Ok(event)).await.is_err() {
                        return;
                    }
                }
                Err(err) => {
                    let mapped = map_api_error(err);
                    if !logged_error {
                        session_telemetry.see_event_completed_failed(&mapped);
                        logged_error = true;
                    }
                    if tx_event.send(Err(mapped)).await.is_err() {
                        return;
                    }
                }
            }
        }
    });

    (ResponseStream { rx_event }, rx_last_response)
}

/// Handles a 401 response by optionally refreshing ChatGPT tokens once.
///
/// When refresh succeeds, the caller should retry the API call; otherwise
/// the mapped `CodexErr` is returned to the caller.
#[derive(Clone, Copy, Debug)]
struct UnauthorizedRecoveryExecution {
    mode: &'static str,
    phase: &'static str,
}

#[derive(Clone, Copy, Debug, Default)]
struct PendingUnauthorizedRetry {
    retry_after_unauthorized: bool,
    recovery_mode: Option<&'static str>,
    recovery_phase: Option<&'static str>,
}

impl PendingUnauthorizedRetry {
    fn from_recovery(recovery: UnauthorizedRecoveryExecution) -> Self {
        Self {
            retry_after_unauthorized: true,
            recovery_mode: Some(recovery.mode),
            recovery_phase: Some(recovery.phase),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AuthRequestTelemetryContext {
    auth_mode: Option<&'static str>,
    auth_header_attached: bool,
    auth_header_name: Option<&'static str>,
    retry_after_unauthorized: bool,
    recovery_mode: Option<&'static str>,
    recovery_phase: Option<&'static str>,
}

impl AuthRequestTelemetryContext {
    fn new(
        auth_mode: Option<AuthMode>,
        api_auth: &CoreAuthProvider,
        retry: PendingUnauthorizedRetry,
    ) -> Self {
        Self {
            auth_mode: auth_mode.map(|mode| match mode {
                AuthMode::ApiKey => "ApiKey",
                AuthMode::Chatgpt => "Chatgpt",
            }),
            auth_header_attached: api_auth.auth_header_attached(),
            auth_header_name: api_auth.auth_header_name(),
            retry_after_unauthorized: retry.retry_after_unauthorized,
            recovery_mode: retry.recovery_mode,
            recovery_phase: retry.recovery_phase,
        }
    }
}

struct WebsocketConnectParams<'a> {
    session_telemetry: &'a SessionTelemetry,
    api_provider: ctox_api::Provider,
    api_auth: CoreAuthProvider,
    turn_metadata_header: Option<&'a str>,
    options: &'a ApiResponsesOptions,
    auth_context: AuthRequestTelemetryContext,
    request_route_telemetry: RequestRouteTelemetry,
}

async fn handle_unauthorized(
    transport: TransportError,
    auth_recovery: &mut Option<UnauthorizedRecovery>,
    session_telemetry: &SessionTelemetry,
) -> Result<UnauthorizedRecoveryExecution> {
    let debug = extract_response_debug_context(&transport);
    if let Some(recovery) = auth_recovery
        && recovery.has_next()
    {
        let mode = recovery.mode_name();
        let phase = recovery.step_name();
        return match recovery.next().await {
            Ok(step_result) => {
                session_telemetry.record_auth_recovery(
                    mode,
                    phase,
                    "recovery_succeeded",
                    debug.request_id.as_deref(),
                    debug.cf_ray.as_deref(),
                    debug.auth_error.as_deref(),
                    debug.auth_error_code.as_deref(),
                    /*recovery_reason*/ None,
                    step_result.auth_state_changed(),
                );
                emit_feedback_auth_recovery_tags(
                    mode,
                    phase,
                    "recovery_succeeded",
                    debug.request_id.as_deref(),
                    debug.cf_ray.as_deref(),
                    debug.auth_error.as_deref(),
                    debug.auth_error_code.as_deref(),
                );
                Ok(UnauthorizedRecoveryExecution { mode, phase })
            }
            Err(RefreshTokenError::Permanent(failed)) => {
                session_telemetry.record_auth_recovery(
                    mode,
                    phase,
                    "recovery_failed_permanent",
                    debug.request_id.as_deref(),
                    debug.cf_ray.as_deref(),
                    debug.auth_error.as_deref(),
                    debug.auth_error_code.as_deref(),
                    /*recovery_reason*/ None,
                    /*auth_state_changed*/ None,
                );
                emit_feedback_auth_recovery_tags(
                    mode,
                    phase,
                    "recovery_failed_permanent",
                    debug.request_id.as_deref(),
                    debug.cf_ray.as_deref(),
                    debug.auth_error.as_deref(),
                    debug.auth_error_code.as_deref(),
                );
                Err(CodexErr::RefreshTokenFailed(failed))
            }
            Err(RefreshTokenError::Transient(other)) => {
                session_telemetry.record_auth_recovery(
                    mode,
                    phase,
                    "recovery_failed_transient",
                    debug.request_id.as_deref(),
                    debug.cf_ray.as_deref(),
                    debug.auth_error.as_deref(),
                    debug.auth_error_code.as_deref(),
                    /*recovery_reason*/ None,
                    /*auth_state_changed*/ None,
                );
                emit_feedback_auth_recovery_tags(
                    mode,
                    phase,
                    "recovery_failed_transient",
                    debug.request_id.as_deref(),
                    debug.cf_ray.as_deref(),
                    debug.auth_error.as_deref(),
                    debug.auth_error_code.as_deref(),
                );
                Err(CodexErr::Io(other))
            }
        };
    }

    let (mode, phase, recovery_reason) = match auth_recovery.as_ref() {
        Some(recovery) => (
            recovery.mode_name(),
            recovery.step_name(),
            Some(recovery.unavailable_reason()),
        ),
        None => ("none", "none", Some("auth_manager_missing")),
    };
    session_telemetry.record_auth_recovery(
        mode,
        phase,
        "recovery_not_run",
        debug.request_id.as_deref(),
        debug.cf_ray.as_deref(),
        debug.auth_error.as_deref(),
        debug.auth_error_code.as_deref(),
        recovery_reason,
        /*auth_state_changed*/ None,
    );
    emit_feedback_auth_recovery_tags(
        mode,
        phase,
        "recovery_not_run",
        debug.request_id.as_deref(),
        debug.cf_ray.as_deref(),
        debug.auth_error.as_deref(),
        debug.auth_error_code.as_deref(),
    );

    Err(map_api_error(ApiError::Transport(transport)))
}

fn api_error_http_status(error: &ApiError) -> Option<u16> {
    match error {
        ApiError::Transport(TransportError::Http { status, .. }) => Some(status.as_u16()),
        _ => None,
    }
}

struct ApiTelemetry {
    session_telemetry: SessionTelemetry,
    auth_context: AuthRequestTelemetryContext,
    request_route_telemetry: RequestRouteTelemetry,
    auth_env_telemetry: AuthEnvTelemetry,
}

impl ApiTelemetry {
    fn new(
        session_telemetry: SessionTelemetry,
        auth_context: AuthRequestTelemetryContext,
        request_route_telemetry: RequestRouteTelemetry,
        auth_env_telemetry: AuthEnvTelemetry,
    ) -> Self {
        Self {
            session_telemetry,
            auth_context,
            request_route_telemetry,
            auth_env_telemetry,
        }
    }
}

impl RequestTelemetry for ApiTelemetry {
    fn on_request(
        &self,
        attempt: u64,
        status: Option<HttpStatusCode>,
        error: Option<&TransportError>,
        duration: Duration,
    ) {
        let error_message = error.map(telemetry_transport_error_message);
        let status = status.map(|s| s.as_u16());
        let debug = error
            .map(extract_response_debug_context)
            .unwrap_or_default();
        self.session_telemetry.record_api_request(
            attempt,
            status,
            error_message.as_deref(),
            duration,
            self.auth_context.auth_header_attached,
            self.auth_context.auth_header_name,
            self.auth_context.retry_after_unauthorized,
            self.auth_context.recovery_mode,
            self.auth_context.recovery_phase,
            self.request_route_telemetry.endpoint,
            debug.request_id.as_deref(),
            debug.cf_ray.as_deref(),
            debug.auth_error.as_deref(),
            debug.auth_error_code.as_deref(),
        );
        emit_feedback_request_tags_with_auth_env(
            &FeedbackRequestTags {
                endpoint: self.request_route_telemetry.endpoint,
                auth_header_attached: self.auth_context.auth_header_attached,
                auth_header_name: self.auth_context.auth_header_name,
                auth_mode: self.auth_context.auth_mode,
                auth_retry_after_unauthorized: Some(self.auth_context.retry_after_unauthorized),
                auth_recovery_mode: self.auth_context.recovery_mode,
                auth_recovery_phase: self.auth_context.recovery_phase,
                auth_connection_reused: None,
                auth_request_id: debug.request_id.as_deref(),
                auth_cf_ray: debug.cf_ray.as_deref(),
                auth_error: debug.auth_error.as_deref(),
                auth_error_code: debug.auth_error_code.as_deref(),
                auth_recovery_followup_success: self
                    .auth_context
                    .retry_after_unauthorized
                    .then_some(error.is_none()),
                auth_recovery_followup_status: self
                    .auth_context
                    .retry_after_unauthorized
                    .then_some(status)
                    .flatten(),
            },
            &self.auth_env_telemetry,
        );
    }
}

impl SseTelemetry for ApiTelemetry {
    fn on_sse_poll(
        &self,
        result: &std::result::Result<
            Option<std::result::Result<Event, EventStreamError<TransportError>>>,
            tokio::time::error::Elapsed,
        >,
        duration: Duration,
    ) {
        self.session_telemetry.log_sse_event(result, duration);
    }
}

impl WebsocketTelemetry for ApiTelemetry {
    fn on_ws_request(&self, duration: Duration, error: Option<&ApiError>, connection_reused: bool) {
        let error_message = error.map(telemetry_api_error_message);
        let status = error.and_then(api_error_http_status);
        let debug = error
            .map(extract_response_debug_context_from_api_error)
            .unwrap_or_default();
        self.session_telemetry.record_websocket_request(
            duration,
            error_message.as_deref(),
            connection_reused,
        );
        emit_feedback_request_tags_with_auth_env(
            &FeedbackRequestTags {
                endpoint: self.request_route_telemetry.endpoint,
                auth_header_attached: self.auth_context.auth_header_attached,
                auth_header_name: self.auth_context.auth_header_name,
                auth_mode: self.auth_context.auth_mode,
                auth_retry_after_unauthorized: Some(self.auth_context.retry_after_unauthorized),
                auth_recovery_mode: self.auth_context.recovery_mode,
                auth_recovery_phase: self.auth_context.recovery_phase,
                auth_connection_reused: Some(connection_reused),
                auth_request_id: debug.request_id.as_deref(),
                auth_cf_ray: debug.cf_ray.as_deref(),
                auth_error: debug.auth_error.as_deref(),
                auth_error_code: debug.auth_error_code.as_deref(),
                auth_recovery_followup_success: self
                    .auth_context
                    .retry_after_unauthorized
                    .then_some(error.is_none()),
                auth_recovery_followup_status: self
                    .auth_context
                    .retry_after_unauthorized
                    .then_some(status)
                    .flatten(),
            },
            &self.auth_env_telemetry,
        );
    }

    fn on_ws_event(
        &self,
        result: &std::result::Result<Option<std::result::Result<Message, Error>>, ApiError>,
        duration: Duration,
    ) {
        self.session_telemetry
            .record_websocket_event(result, duration);
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
