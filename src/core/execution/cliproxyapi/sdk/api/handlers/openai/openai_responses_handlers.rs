// ref: sdk/api/handlers/openai/openai_responses_handlers.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::internal::runtime::executor::{
    normalize_codex_tool_integer_types_for_executor, AntigravityAccountPoolError,
    AntigravitySubscriptionAccountPool, AntigravityTrackedResponsesStream, ClaudeAccountPoolError,
    ClaudeSubscriptionAccountPool, ClaudeTrackedMessagesStreamResponse, CodexAccountPoolError,
    CodexSubscriptionAccountPool, CodexTrackedResponsesStreamResponse,
};
use crate::internal::translator::antigravity::openai::responses::convert_openai_responses_request_to_antigravity;
use crate::internal::translator::claude::openai::responses::{
    convert_claude_response_to_openai_responses_non_stream,
    convert_openai_responses_request_to_claude, ClaudeResponsesStreamDecoder,
};
use crate::sdk::api::handlers::responses_sse_framer::{
    is_codex_responses_client, responses_transport_failure_frame, ResponsesSseFramer,
};
use crate::sdk::translator::TranslationContext;

/// First executable `/v1/responses` vertical slice.
///
/// As in upstream's non-stream execution path, a translated Claude response
/// is requested from Anthropic as SSE and aggregated before returning JSON to
/// a non-stream OpenAI Responses client. This keeps one response converter as
/// the authority for both future streaming and the current buffered route.
pub struct OpenAiResponsesClaudeHandler {
    pool: Arc<ClaudeSubscriptionAccountPool>,
}

impl OpenAiResponsesClaudeHandler {
    pub fn new(pool: Arc<ClaudeSubscriptionAccountPool>) -> Self {
        Self { pool }
    }

    async fn handle_non_stream(
        &self,
        body: &[u8],
        headers: &BTreeMap<String, Vec<String>>,
    ) -> OpenAiResponsesHttpResponse {
        let request = match parse_request(body) {
            Ok(request) => request,
            Err(message) => return OpenAiResponsesHttpResponse::error(400, message),
        };
        if request.stream {
            return OpenAiResponsesHttpResponse::error(
                400,
                "streaming is not available on this route yet",
            );
        }

        let translated = normalize_codex_tool_integer_types_for_executor(
            &convert_openai_responses_request_to_claude(&request.model, body, true),
            headers,
            "claude",
        );
        let outcome = match self
            .pool
            .execute_configured_with_request_context(
                &request.model,
                translated.clone(),
                true,
                body,
                headers,
            )
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => return pool_error_response(error),
        };
        let upstream = outcome.outcome().response();
        if !(200..300).contains(&upstream.status()) {
            return OpenAiResponsesHttpResponse::error(
                normalized_upstream_status(upstream.status()),
                "Claude upstream rejected the request",
            );
        }
        if !valid_claude_event_stream(upstream.body()) {
            return OpenAiResponsesHttpResponse::error(
                502,
                "Claude upstream returned an invalid event stream",
            );
        }

        let response = convert_claude_response_to_openai_responses_non_stream(
            body,
            &translated,
            upstream.body(),
        );
        if !valid_openai_response(&response) {
            return OpenAiResponsesHttpResponse::error(502, "Claude response translation failed");
        }
        OpenAiResponsesHttpResponse::json(200, response)
    }

    pub async fn handle_route(&self, body: &[u8]) -> OpenAiResponsesRouteResponse {
        self.handle_route_with_headers(body, &BTreeMap::new()).await
    }

    pub async fn handle_route_with_headers(
        &self,
        body: &[u8],
        headers: &BTreeMap<String, Vec<String>>,
    ) -> OpenAiResponsesRouteResponse {
        let request = match parse_request(body) {
            Ok(request) => request,
            Err(message) => {
                return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    400, message,
                ))
            }
        };
        if !request.stream {
            return OpenAiResponsesRouteResponse::Buffered(
                self.handle_non_stream(body, headers).await,
            );
        }

        let translated = normalize_codex_tool_integer_types_for_executor(
            &convert_openai_responses_request_to_claude(&request.model, body, true),
            headers,
            "claude",
        );
        let outcome = match self
            .pool
            .execute_stream_configured_with_request_context(
                &request.model,
                translated.clone(),
                body,
                headers,
            )
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                return OpenAiResponsesRouteResponse::Buffered(pool_error_response(error))
            }
        };
        let status = outcome.outcome().response().status();
        if !(200..300).contains(&status) {
            return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                normalized_upstream_status(status),
                "Claude upstream rejected the request",
            ));
        }
        let upstream = outcome.into_outcome().into_response();
        match OpenAiResponsesStreamBootstrap::new(
            request.model,
            body.to_vec(),
            translated,
            upstream,
            is_codex_responses_client(headers),
        )
        .await
        {
            Ok(stream) => OpenAiResponsesRouteResponse::Stream(Box::new(stream)),
            Err(()) => OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                502,
                "Claude response translation produced no bootstrap event",
            )),
        }
    }
}

impl std::fmt::Debug for OpenAiResponsesClaudeHandler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesClaudeHandler")
            .field("pool", &"ClaudeSubscriptionAccountPool")
            .finish()
    }
}

/// Responses route backed by the Antigravity subscription pool. Provider
/// selection stays explicit; the request model is passed through unchanged and
/// is never used to select credentials.
pub struct OpenAiResponsesAntigravityHandler {
    pool: Arc<AntigravitySubscriptionAccountPool>,
}

impl OpenAiResponsesAntigravityHandler {
    pub fn new(pool: Arc<AntigravitySubscriptionAccountPool>) -> Self {
        Self { pool }
    }

    pub async fn handle_route(&self, body: &[u8]) -> OpenAiResponsesRouteResponse {
        self.handle_route_with_headers(body, &BTreeMap::new()).await
    }

    pub async fn handle_route_with_headers(
        &self,
        body: &[u8],
        headers: &BTreeMap<String, Vec<String>>,
    ) -> OpenAiResponsesRouteResponse {
        let request = match parse_request(body) {
            Ok(request) => request,
            Err(message) => {
                return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    400, message,
                ));
            }
        };
        let translated = normalize_codex_tool_integer_types_for_executor(
            &convert_openai_responses_request_to_antigravity(&request.model, body, request.stream),
            headers,
            "antigravity",
        );
        if request.stream {
            let outcome = match self
                .pool
                .execute_stream_configured_with_client_headers(
                    &request.model,
                    body.to_vec(),
                    translated,
                    headers,
                )
                .await
            {
                Ok(outcome) => outcome,
                Err(error) => {
                    return OpenAiResponsesRouteResponse::Buffered(
                        antigravity_pool_error_response(error),
                    );
                }
            };
            return OpenAiResponsesRouteResponse::AntigravityStream(Box::new(
                OpenAiResponsesAntigravityStream::new(outcome.into_response())
                    .with_codex_client(is_codex_responses_client(headers)),
            ));
        }
        let outcome = match self
            .pool
            .execute_configured_with_client_headers(
                &request.model,
                body.to_vec(),
                translated,
                headers,
            )
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                return OpenAiResponsesRouteResponse::Buffered(antigravity_pool_error_response(
                    error,
                ));
            }
        };
        let payload = outcome.outcome().payload();
        if !valid_openai_response(payload) {
            return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                502,
                "Antigravity response translation failed",
            ));
        }
        OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::json(
            200,
            payload.to_vec(),
        ))
    }
}

impl std::fmt::Debug for OpenAiResponsesAntigravityHandler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesAntigravityHandler")
            .field("pool", &"AntigravitySubscriptionAccountPool")
            .finish()
    }
}

/// Codex already speaks the internal Responses-shaped contract, so this
/// handler performs account routing and validation without format translation.
pub struct OpenAiResponsesCodexHandler {
    pool: Arc<CodexSubscriptionAccountPool>,
    responses_lite: bool,
}

impl OpenAiResponsesCodexHandler {
    pub fn new(pool: Arc<CodexSubscriptionAccountPool>) -> Self {
        Self {
            pool,
            responses_lite: false,
        }
    }

    pub fn with_responses_lite(mut self, enabled: bool) -> Self {
        self.responses_lite = enabled;
        self
    }

    pub async fn handle_route(&self, body: &[u8]) -> OpenAiResponsesRouteResponse {
        self.handle_route_with_headers(body, &BTreeMap::new()).await
    }

    pub async fn handle_route_with_headers(
        &self,
        body: &[u8],
        headers: &BTreeMap<String, Vec<String>>,
    ) -> OpenAiResponsesRouteResponse {
        let request = match parse_request(body) {
            Ok(request) => request,
            Err(message) => {
                return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    400, message,
                ))
            }
        };
        if request.stream {
            let outcome = match self
                .pool
                .execute_stream_configured(&request.model, body.to_vec(), self.responses_lite)
                .await
            {
                Ok(outcome) => outcome,
                Err(error) => {
                    return OpenAiResponsesRouteResponse::Buffered(codex_pool_error_response(error))
                }
            };
            return OpenAiResponsesRouteResponse::CodexStream(Box::new(
                OpenAiResponsesCodexStream::new(outcome.into_response())
                    .with_codex_client(is_codex_responses_client(headers)),
            ));
        }
        let outcome = match self
            .pool
            .execute_configured(&request.model, body.to_vec(), self.responses_lite)
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                return OpenAiResponsesRouteResponse::Buffered(codex_pool_error_response(error))
            }
        };
        let payload = outcome.outcome().payload();
        if !valid_codex_response(payload) {
            return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                502,
                "Codex upstream returned an invalid response",
            ));
        }
        OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::json(
            200,
            payload.to_vec(),
        ))
    }
}

impl std::fmt::Debug for OpenAiResponsesCodexHandler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesCodexHandler")
            .field("pool", &"CodexSubscriptionAccountPool")
            .field("responses_lite", &self.responses_lite)
            .finish()
    }
}

/// Server dispatch contract. Provider selection is a separate input from the
/// Responses body and therefore cannot be inferred from `model`.
pub trait OpenAiResponsesRouteHandler: Send + Sync {
    fn handle_provider_route<'a>(
        &'a self,
        provider: Option<&'a str>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>>;

    fn handle_provider_route_with_headers<'a>(
        &'a self,
        provider: Option<&'a str>,
        headers: &'a BTreeMap<String, Vec<String>>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        let _ = headers;
        self.handle_provider_route(provider, body)
    }
}

impl OpenAiResponsesRouteHandler for OpenAiResponsesClaudeHandler {
    fn handle_provider_route<'a>(
        &'a self,
        provider: Option<&'a str>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        Box::pin(async move {
            if provider.is_some_and(|provider| !provider.eq_ignore_ascii_case("claude")) {
                return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    400,
                    "requested provider is not configured",
                ));
            }
            self.handle_route(body).await
        })
    }

    fn handle_provider_route_with_headers<'a>(
        &'a self,
        provider: Option<&'a str>,
        headers: &'a BTreeMap<String, Vec<String>>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        Box::pin(async move {
            if provider.is_some_and(|provider| !provider.eq_ignore_ascii_case("claude")) {
                return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    400,
                    "requested provider is not configured",
                ));
            }
            self.handle_route_with_headers(body, headers).await
        })
    }
}

impl OpenAiResponsesRouteHandler for OpenAiResponsesAntigravityHandler {
    fn handle_provider_route<'a>(
        &'a self,
        provider: Option<&'a str>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        Box::pin(async move {
            if provider.is_some_and(|provider| !provider.eq_ignore_ascii_case("antigravity")) {
                return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    400,
                    "requested provider is not configured",
                ));
            }
            self.handle_route(body).await
        })
    }

    fn handle_provider_route_with_headers<'a>(
        &'a self,
        provider: Option<&'a str>,
        headers: &'a BTreeMap<String, Vec<String>>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        Box::pin(async move {
            if provider.is_some_and(|provider| !provider.eq_ignore_ascii_case("antigravity")) {
                return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    400,
                    "requested provider is not configured",
                ));
            }
            self.handle_route_with_headers(body, headers).await
        })
    }
}

impl OpenAiResponsesRouteHandler for OpenAiResponsesCodexHandler {
    fn handle_provider_route<'a>(
        &'a self,
        provider: Option<&'a str>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        Box::pin(async move {
            if provider.is_some_and(|provider| !provider.eq_ignore_ascii_case("codex")) {
                return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    400,
                    "requested provider is not configured",
                ));
            }
            self.handle_route(body).await
        })
    }

    fn handle_provider_route_with_headers<'a>(
        &'a self,
        provider: Option<&'a str>,
        headers: &'a BTreeMap<String, Vec<String>>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        Box::pin(async move {
            if provider.is_some_and(|provider| !provider.eq_ignore_ascii_case("codex")) {
                return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    400,
                    "requested provider is not configured",
                ));
            }
            self.handle_route_with_headers(body, headers).await
        })
    }
}

impl<T> OpenAiResponsesRouteHandler for Arc<T>
where
    T: OpenAiResponsesRouteHandler + ?Sized,
{
    fn handle_provider_route<'a>(
        &'a self,
        provider: Option<&'a str>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        (**self).handle_provider_route(provider, body)
    }

    fn handle_provider_route_with_headers<'a>(
        &'a self,
        provider: Option<&'a str>,
        headers: &'a BTreeMap<String, Vec<String>>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        (**self).handle_provider_route_with_headers(provider, headers, body)
    }
}

/// Explicit allow-list router for `/v1/responses` providers.
pub struct OpenAiResponsesProviderRouter {
    default_provider: String,
    claude: Option<Arc<OpenAiResponsesClaudeHandler>>,
    codex: Option<Arc<OpenAiResponsesCodexHandler>>,
    antigravity: Option<Arc<OpenAiResponsesAntigravityHandler>>,
}

impl OpenAiResponsesProviderRouter {
    pub fn new(
        default_provider: impl Into<String>,
        claude: Option<Arc<OpenAiResponsesClaudeHandler>>,
        codex: Option<Arc<OpenAiResponsesCodexHandler>>,
        antigravity: Option<Arc<OpenAiResponsesAntigravityHandler>>,
    ) -> Result<Self, ProviderRouterError> {
        let default_provider = default_provider.into().trim().to_ascii_lowercase();
        let configured = match default_provider.as_str() {
            "claude" => claude.is_some(),
            "codex" => codex.is_some(),
            "antigravity" => antigravity.is_some(),
            _ => false,
        };
        if !configured {
            return Err(ProviderRouterError::Configuration);
        }
        Ok(Self {
            default_provider,
            claude,
            codex,
            antigravity,
        })
    }
}

impl OpenAiResponsesRouteHandler for OpenAiResponsesProviderRouter {
    fn handle_provider_route<'a>(
        &'a self,
        provider: Option<&'a str>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        Box::pin(async move {
            let provider = provider.unwrap_or(&self.default_provider).trim();
            if provider.eq_ignore_ascii_case("claude") {
                if let Some(handler) = self.claude.as_ref() {
                    return handler.handle_route(body).await;
                }
            } else if provider.eq_ignore_ascii_case("codex") {
                if let Some(handler) = self.codex.as_ref() {
                    return handler.handle_route(body).await;
                }
            } else if provider.eq_ignore_ascii_case("antigravity") {
                if let Some(handler) = self.antigravity.as_ref() {
                    return handler.handle_route(body).await;
                }
            }
            OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                400,
                "requested provider is not configured",
            ))
        })
    }

    fn handle_provider_route_with_headers<'a>(
        &'a self,
        provider: Option<&'a str>,
        headers: &'a BTreeMap<String, Vec<String>>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        Box::pin(async move {
            let provider = provider.unwrap_or(&self.default_provider).trim();
            if provider.eq_ignore_ascii_case("claude") {
                if let Some(handler) = self.claude.as_ref() {
                    return handler.handle_route_with_headers(body, headers).await;
                }
            } else if provider.eq_ignore_ascii_case("codex") {
                if let Some(handler) = self.codex.as_ref() {
                    return handler.handle_route_with_headers(body, headers).await;
                }
            } else if provider.eq_ignore_ascii_case("antigravity") {
                if let Some(handler) = self.antigravity.as_ref() {
                    return handler.handle_route_with_headers(body, headers).await;
                }
            }
            OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                400,
                "requested provider is not configured",
            ))
        })
    }
}

impl std::fmt::Debug for OpenAiResponsesProviderRouter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesProviderRouter")
            .field("default_provider", &self.default_provider)
            .field("claude", &self.claude.as_ref().map(|_| "configured"))
            .field("codex", &self.codex.as_ref().map(|_| "configured"))
            .field(
                "antigravity",
                &self.antigravity.as_ref().map(|_| "configured"),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRouterError {
    Configuration,
}

impl std::fmt::Display for ProviderRouterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Responses provider router is invalid")
    }
}

impl std::error::Error for ProviderRouterError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiResponsesHttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

#[derive(Debug)]
pub enum OpenAiResponsesRouteResponse {
    Buffered(OpenAiResponsesHttpResponse),
    Stream(Box<OpenAiResponsesStreamBootstrap>),
    CodexStream(Box<OpenAiResponsesCodexStream>),
    AntigravityStream(Box<OpenAiResponsesAntigravityStream>),
}

pub struct OpenAiResponsesAntigravityStream {
    upstream: AntigravityTrackedResponsesStream,
    framer: ResponsesSseFramer,
    terminal: bool,
    emitted_failure: bool,
    codex_client: bool,
}

impl OpenAiResponsesAntigravityStream {
    fn new(upstream: AntigravityTrackedResponsesStream) -> Self {
        Self {
            upstream,
            framer: ResponsesSseFramer::new(false),
            terminal: false,
            emitted_failure: false,
            codex_client: false,
        }
    }

    fn with_codex_client(mut self, codex_client: bool) -> Self {
        self.codex_client = codex_client;
        self.framer = ResponsesSseFramer::new(codex_client);
        self
    }

    pub async fn next_chunk(&mut self) -> Option<Vec<u8>> {
        if self.terminal {
            return None;
        }
        loop {
            match self.upstream.next_event().await {
                Some(Ok(mut chunk)) => {
                    if chunk.is_empty() {
                        return Some(chunk);
                    }
                    if chunk.starts_with(b"event: response.failed\n")
                        || chunk.starts_with(b"event: error\n")
                    {
                        redact_provider_stream_failure_message(
                            &mut chunk,
                            "Antigravity upstream stream failed",
                        );
                    }
                    let emitted = self.framer.write_chunk(&chunk);
                    if self.framer.is_failure() {
                        self.terminal = true;
                        self.emitted_failure = true;
                        self.upstream.record_terminal_failure().await;
                    } else if self.framer.is_terminal() {
                        self.terminal = true;
                    }
                    if !emitted.is_empty() {
                        return Some(emitted);
                    }
                    if self.terminal {
                        return None;
                    }
                }
                Some(Err(_)) => return self.failure_chunk().await,
                None => {
                    let flushed = self.framer.flush();
                    if self.framer.is_failure() {
                        self.terminal = true;
                        self.emitted_failure = true;
                        self.upstream.record_terminal_failure().await;
                    } else if self.framer.is_terminal() {
                        self.terminal = true;
                    }
                    if !flushed.is_empty() {
                        return Some(flushed);
                    }
                    if self.terminal {
                        return None;
                    }
                    return self.failure_chunk().await;
                }
            }
        }
    }

    async fn failure_chunk(&mut self) -> Option<Vec<u8>> {
        if self.emitted_failure {
            self.terminal = true;
            return None;
        }
        let sequence = self.framer.data_frames();
        self.emitted_failure = true;
        self.terminal = true;
        self.upstream.record_terminal_failure().await;
        Some(responses_transport_failure_frame(
            "Antigravity upstream stream failed",
            self.codex_client,
            sequence,
        ))
    }
}

impl std::fmt::Debug for OpenAiResponsesAntigravityStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesAntigravityStream")
            .field("upstream", &self.upstream)
            .field("terminal", &self.terminal)
            .field("emitted_failure", &self.emitted_failure)
            .finish()
    }
}

pub struct OpenAiResponsesCodexStream {
    upstream: CodexTrackedResponsesStreamResponse,
    decoder: crate::internal::translator::common::SseDecoder,
    framer: ResponsesSseFramer,
    pending: VecDeque<Vec<u8>>,
    terminal: bool,
    emitted_failure: bool,
    codex_client: bool,
}

impl OpenAiResponsesCodexStream {
    fn new(upstream: CodexTrackedResponsesStreamResponse) -> Self {
        Self {
            upstream,
            decoder: crate::internal::translator::common::SseDecoder::new(),
            framer: ResponsesSseFramer::new(false),
            pending: VecDeque::new(),
            terminal: false,
            emitted_failure: false,
            codex_client: false,
        }
    }

    fn with_codex_client(mut self, codex_client: bool) -> Self {
        self.codex_client = codex_client;
        self.framer = ResponsesSseFramer::new(codex_client);
        self
    }

    pub async fn next_chunk(&mut self) -> Option<Vec<u8>> {
        if let Some(chunk) = self.pending.pop_front() {
            return Some(chunk);
        }
        if self.terminal {
            return None;
        }
        loop {
            match self.upstream.next_chunk().await {
                Some(Ok(chunk)) => {
                    let events = self.decoder.push(&chunk);
                    let failed = self.enqueue_events(events);
                    if failed {
                        self.upstream.record_terminal_failure().await;
                    }
                    if let Some(chunk) = self.pending.pop_front() {
                        return Some(chunk);
                    }
                }
                Some(Err(_)) => return self.failure_chunk().await,
                None => {
                    let events = self.decoder.finish();
                    let failed = self.enqueue_events(events);
                    if failed {
                        self.upstream.record_terminal_failure().await;
                    }
                    let flushed = self.framer.flush();
                    if self.framer.is_terminal() {
                        self.terminal = true;
                    }
                    if self.framer.is_failure() {
                        self.upstream.record_terminal_failure().await;
                    }
                    if !flushed.is_empty() {
                        self.pending.push_back(flushed);
                    }
                    if let Some(chunk) = self.pending.pop_front() {
                        return Some(chunk);
                    }
                    return if self.terminal {
                        None
                    } else {
                        self.failure_chunk().await
                    };
                }
            }
        }
    }

    fn enqueue_events(
        &mut self,
        events: Vec<crate::internal::translator::common::SseEvent>,
    ) -> bool {
        let mut failed = false;
        for mut event in events {
            if let Ok(mut value) = serde_json::from_slice::<Value>(&event.data) {
                if matches!(
                    value.get("type").and_then(Value::as_str),
                    Some("response.failed" | "error")
                ) {
                    redact_json_messages(&mut value, "Codex upstream stream failed");
                    if let Ok(data) = serde_json::to_vec(&value) {
                        event.data = data;
                    }
                }
            }
            let encoded = self.framer.write_chunk(&encode_sse_event(&event));
            if self.framer.is_terminal() {
                self.terminal = true;
            }
            if self.framer.is_failure() {
                failed = true;
            }
            if !encoded.is_empty() {
                self.pending.push_back(encoded);
            }
        }
        failed
    }

    async fn failure_chunk(&mut self) -> Option<Vec<u8>> {
        if self.emitted_failure {
            self.terminal = true;
            return None;
        }
        let sequence = self.framer.data_frames();
        self.emitted_failure = true;
        self.terminal = true;
        self.upstream.record_terminal_failure().await;
        Some(responses_transport_failure_frame(
            "Codex upstream stream failed",
            self.codex_client,
            sequence,
        ))
    }
}

impl std::fmt::Debug for OpenAiResponsesCodexStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesCodexStream")
            .field("upstream", &self.upstream)
            .field("pending_chunks", &self.pending.len())
            .field("terminal", &self.terminal)
            .field("emitted_failure", &self.emitted_failure)
            .finish()
    }
}

fn encode_sse_event(event: &crate::internal::translator::common::SseEvent) -> Vec<u8> {
    let mut encoded = Vec::new();
    if let Some(kind) = event.event.as_deref() {
        encoded.extend_from_slice(b"event: ");
        encoded.extend_from_slice(kind.as_bytes());
        encoded.push(b'\n');
    }
    if let Some(id) = event.id.as_deref() {
        encoded.extend_from_slice(b"id: ");
        encoded.extend_from_slice(id.as_bytes());
        encoded.push(b'\n');
    }
    if let Some(retry) = event.retry_millis {
        encoded.extend_from_slice(format!("retry: {retry}\n").as_bytes());
    }
    for line in event.data.split(|byte| *byte == b'\n') {
        encoded.extend_from_slice(b"data: ");
        encoded.extend_from_slice(line);
        encoded.push(b'\n');
    }
    encoded.push(b'\n');
    encoded
}

fn redact_json_messages(value: &mut Value, replacement: &str) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if key == "message" && value.is_string() {
                    *value = Value::String(replacement.to_owned());
                } else {
                    redact_json_messages(value, replacement);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_json_messages(value, replacement);
            }
        }
        _ => {}
    }
}

pub struct OpenAiResponsesStreamBootstrap {
    model: String,
    original_request: Vec<u8>,
    translated_request: Vec<u8>,
    upstream: ClaudeTrackedMessagesStreamResponse,
    context: TranslationContext,
    decoder: ClaudeResponsesStreamDecoder,
    framer: ResponsesSseFramer,
    pending: VecDeque<Vec<u8>>,
    terminal: bool,
    failed: bool,
    ended: bool,
}

impl OpenAiResponsesStreamBootstrap {
    async fn new(
        model: String,
        original_request: Vec<u8>,
        translated_request: Vec<u8>,
        upstream: ClaudeTrackedMessagesStreamResponse,
        codex_client: bool,
    ) -> Result<Self, ()> {
        let mut stream = Self {
            model,
            original_request,
            translated_request,
            upstream,
            context: TranslationContext::default(),
            decoder: ClaudeResponsesStreamDecoder::new(),
            framer: ResponsesSseFramer::new(codex_client),
            pending: VecDeque::new(),
            terminal: false,
            failed: false,
            ended: false,
        };
        stream.fill_pending().await;
        (!stream.pending.is_empty()).then_some(stream).ok_or(())
    }

    pub async fn next_chunk(&mut self) -> Option<Vec<u8>> {
        if let Some(chunk) = self.pending.pop_front() {
            return Some(chunk);
        }
        if self.terminal || self.ended {
            return None;
        }
        self.fill_pending().await;
        self.pending.pop_front()
    }

    async fn fill_pending(&mut self) {
        while self.pending.is_empty() && !self.terminal && !self.ended {
            match self.upstream.next_chunk().await {
                Some(Ok(chunk)) => {
                    let output = self.decoder.push(
                        &self.context,
                        &self.model,
                        &self.original_request,
                        &self.translated_request,
                        &chunk,
                    );
                    self.enqueue(output);
                    if self.failed {
                        self.upstream.record_terminal_failure().await;
                    }
                }
                Some(Err(_)) => self.enqueue_transport_failure(),
                None => {
                    let output = self.decoder.finish(
                        &self.context,
                        &self.model,
                        &self.original_request,
                        &self.translated_request,
                    );
                    self.enqueue(output);
                    let flushed = self.framer.flush();
                    if self.framer.is_terminal() {
                        self.terminal = true;
                    }
                    if self.framer.is_failure() {
                        self.failed = true;
                    }
                    if !flushed.is_empty() {
                        self.pending.push_back(flushed);
                    }
                    if self.failed {
                        self.upstream.record_terminal_failure().await;
                    }
                    if !self.terminal {
                        self.upstream.record_terminal_failure().await;
                        self.enqueue_transport_failure();
                    }
                    self.ended = true;
                }
            }
        }
    }

    fn enqueue(&mut self, chunks: Vec<Vec<u8>>) {
        for mut chunk in chunks {
            redact_stream_failure_message(&mut chunk);
            let emitted = self.framer.write_chunk(&chunk);
            if self.framer.is_terminal() {
                self.terminal = true;
            }
            if self.framer.is_failure() {
                self.failed = true;
            }
            if !emitted.is_empty() {
                self.pending.push_back(emitted);
            }
        }
    }

    fn enqueue_transport_failure(&mut self) {
        let event = b"data: {\"type\":\"error\",\"error\":{\"type\":\"upstream_stream_error\",\"message\":\"Claude upstream stream failed\"}}\n\n";
        let output = self.decoder.push(
            &self.context,
            &self.model,
            &self.original_request,
            &self.translated_request,
            event,
        );
        self.enqueue(output);
        self.terminal = true;
    }
}

impl std::fmt::Debug for OpenAiResponsesStreamBootstrap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesStreamBootstrap")
            .field("model", &self.model)
            .field("requests", &"[REDACTED]")
            .field("upstream", &self.upstream)
            .field("pending_chunks", &self.pending.len())
            .field("terminal", &self.terminal)
            .field("failed", &self.failed)
            .field("ended", &self.ended)
            .finish()
    }
}

impl OpenAiResponsesHttpResponse {
    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn content_type(&self) -> &'static str {
        self.content_type
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn buffered(status: u16, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type,
            body,
        }
    }

    pub fn json(status: u16, body: Vec<u8>) -> Self {
        Self::buffered(status, "application/json", body)
    }

    pub fn event_stream(status: u16, body: Vec<u8>) -> Self {
        Self::buffered(status, "text/event-stream", body)
    }

    pub fn error(status: u16, message: &str) -> Self {
        let payload = json!({
            "error": {
                "message": message,
                "type": if status == 400 { "invalid_request_error" } else { "server_error" }
            }
        });
        Self::json(
            status,
            serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec()),
        )
    }
}

#[derive(Debug)]
struct ResponsesRequest {
    model: String,
    stream: bool,
}

fn parse_request(body: &[u8]) -> Result<ResponsesRequest, &'static str> {
    let root = serde_json::from_slice::<Value>(body).map_err(|_| "invalid JSON request body")?;
    let object = root
        .as_object()
        .ok_or("request body must be a JSON object")?;
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .ok_or("model is required")?;
    Ok(ResponsesRequest {
        model: model.to_owned(),
        stream: object.get("stream").and_then(Value::as_bool) == Some(true),
    })
}

fn pool_error_response(error: ClaudeAccountPoolError) -> OpenAiResponsesHttpResponse {
    let (status, message) = match error {
        ClaudeAccountPoolError::Routing(error) => return claude_routing_error_response(error),
        ClaudeAccountPoolError::Execution(_) => (502, "Claude upstream transport failed"),
        ClaudeAccountPoolError::OutcomePersistence => {
            (503, "Claude account outcome could not be persisted")
        }
        ClaudeAccountPoolError::Configuration => (500, "Claude runtime is not configured"),
    };
    OpenAiResponsesHttpResponse::error(status, message)
}

fn claude_routing_error_response(
    error: crate::sdk::cliproxy::auth::AccountRoutingError,
) -> OpenAiResponsesHttpResponse {
    use crate::sdk::cliproxy::auth::{AccountRoutingError, AccountSelectionError};

    let (code, message, retry_at_ms) = match error {
        AccountRoutingError::Store(_)
        | AccountRoutingError::Selection(AccountSelectionError::State) => (
            "gateway_account_state_unavailable",
            "Claude account state could not be read or saved. Check local storage access.",
            None,
        ),
        AccountRoutingError::Selection(AccountSelectionError::NotFound) => (
            "gateway_account_not_found",
            "No Claude account is configured for this request. Check Settings > Models.",
            None,
        ),
        AccountRoutingError::Selection(AccountSelectionError::Unavailable) => (
            "gateway_account_unavailable",
            "No Claude account is eligible for this model. Check enabled accounts and reported limits in Settings > Models.",
            None,
        ),
        AccountRoutingError::Selection(AccountSelectionError::Cooldown { retry_after_ms }) => (
            "gateway_account_cooldown",
            "Claude accounts are in a gateway retry cooldown. Retry after the recorded deadline.",
            Some(retry_after_ms),
        ),
    };
    let mut error = json!({
        "message": message,
        "type": "server_error",
        "code": code,
        "source": "gateway",
    });
    // The selector supplies an absolute Unix-millisecond deadline, not a duration.
    if let Some(deadline) = retry_at_ms {
        error["retry_at_ms"] = json!(deadline);
    }
    OpenAiResponsesHttpResponse::json(
        503,
        serde_json::to_vec(&json!({ "error": error })).unwrap_or_else(|_| b"{}".to_vec()),
    )
}

fn codex_pool_error_response(error: CodexAccountPoolError) -> OpenAiResponsesHttpResponse {
    let (status, message) = match error {
        CodexAccountPoolError::Routing(_) => (503, "no Codex account is currently available"),
        CodexAccountPoolError::Execution(error) => match error {
            crate::internal::runtime::executor::CodexExecutionError::Http { status, .. }
            | crate::internal::runtime::executor::CodexExecutionError::Terminal {
                status, ..
            } => (
                normalized_upstream_status(status),
                "Codex upstream rejected the request",
            ),
            _ => (502, "Codex upstream transport failed"),
        },
        CodexAccountPoolError::OutcomePersistence => {
            (503, "Codex account outcome could not be persisted")
        }
        CodexAccountPoolError::Configuration => (500, "Codex runtime is not configured"),
    };
    OpenAiResponsesHttpResponse::error(status, message)
}

fn antigravity_pool_error_response(
    error: AntigravityAccountPoolError,
) -> OpenAiResponsesHttpResponse {
    let (status, message) = match error {
        AntigravityAccountPoolError::Routing(_) => {
            (503, "no Antigravity account is currently available")
        }
        AntigravityAccountPoolError::Execution(error) => match error {
            crate::internal::runtime::executor::AntigravityExecutionError::Http {
                status, ..
            } => (
                normalized_upstream_status(status),
                "Antigravity upstream rejected the request",
            ),
            _ => (502, "Antigravity upstream transport failed"),
        },
        AntigravityAccountPoolError::OutcomePersistence => {
            (503, "Antigravity account outcome could not be persisted")
        }
        AntigravityAccountPoolError::CapabilityUnavailable => (
            503,
            "no Antigravity account supports the requested capability",
        ),
        AntigravityAccountPoolError::Translation(_) => (
            503,
            "Antigravity request translation dependency is unavailable",
        ),
        AntigravityAccountPoolError::Configuration => {
            (500, "Antigravity runtime is not configured")
        }
    };
    OpenAiResponsesHttpResponse::error(status, message)
}

fn normalized_upstream_status(status: u16) -> u16 {
    if (400..600).contains(&status) {
        status
    } else {
        502
    }
}

fn valid_claude_event_stream(body: &[u8]) -> bool {
    let mut message_start = false;
    let mut message_stop = false;
    let mut terminal_error = false;
    for line in body.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(payload) = line.strip_prefix(b"data:") else {
            continue;
        };
        let payload = payload.strip_prefix(b" ").unwrap_or(payload);
        let Ok(event) = serde_json::from_slice::<Value>(payload) else {
            return false;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") => message_start = true,
            Some("message_stop") => message_stop = true,
            Some("error") => terminal_error = true,
            _ => {}
        }
    }
    message_start && (message_stop || terminal_error)
}

fn valid_openai_response(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body).is_ok_and(|root| {
        root.get("object").and_then(Value::as_str) == Some("response")
            && root.get("status").and_then(Value::as_str) == Some("completed")
            && root
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty())
    })
}

fn valid_codex_response(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body).is_ok_and(|root| {
        root.as_object().is_some()
            && root
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty())
            && matches!(
                root.get("status").and_then(Value::as_str),
                Some("completed" | "incomplete")
            )
    })
}

fn redact_provider_stream_failure_message(chunk: &mut Vec<u8>, replacement: &str) {
    let Some(data_at) = chunk
        .windows(b"data: ".len())
        .position(|window| window == b"data: ")
    else {
        return;
    };
    let payload = trim_sse_payload(&chunk[data_at + b"data: ".len()..]);
    let Ok(mut value) = serde_json::from_slice::<Value>(payload) else {
        return;
    };
    redact_json_messages(&mut value, replacement);
    if let Ok(payload) = serde_json::to_vec(&value) {
        let mut redacted = chunk[..data_at + b"data: ".len()].to_vec();
        redacted.extend_from_slice(&payload);
        redacted.extend_from_slice(b"\n\n");
        *chunk = redacted;
    }
}

fn trim_sse_payload(mut payload: &[u8]) -> &[u8] {
    while payload.last().is_some_and(u8::is_ascii_whitespace) {
        payload = &payload[..payload.len() - 1];
    }
    payload
}

fn redact_stream_failure_message(chunk: &mut Vec<u8>) {
    let Some(line_end) = chunk.iter().position(|byte| *byte == b'\n') else {
        return;
    };
    let (head, data) = chunk.split_at(line_end);
    let data = &data[1..];
    if head != b"event: response.failed" {
        return;
    }
    let Some(payload) = data.strip_prefix(b"data: ") else {
        return;
    };
    let Ok(mut value) = serde_json::from_slice::<Value>(payload) else {
        return;
    };
    let Some(message) = value.pointer_mut("/response/error/message") else {
        return;
    };
    *message = Value::String("Claude upstream stream failed".to_owned());
    if let Ok(payload) = serde_json::to_vec(&value) {
        let mut redacted = b"event: response.failed\ndata: ".to_vec();
        redacted.extend_from_slice(&payload);
        *chunk = redacted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_gateway_selection_errors_have_distinct_non_upstream_codes() {
        use crate::sdk::cliproxy::auth::{
            AccountRoutingError, AccountSelectionError, CooldownStoreError,
        };

        for (reason, code) in [
            (
                AccountRoutingError::Store(CooldownStoreError::Read),
                "gateway_account_state_unavailable",
            ),
            (
                AccountRoutingError::Store(CooldownStoreError::Write),
                "gateway_account_state_unavailable",
            ),
            (
                AccountRoutingError::Store(CooldownStoreError::InvalidRecord),
                "gateway_account_state_unavailable",
            ),
            (
                AccountRoutingError::Selection(AccountSelectionError::State),
                "gateway_account_state_unavailable",
            ),
            (
                AccountRoutingError::Selection(AccountSelectionError::NotFound),
                "gateway_account_not_found",
            ),
            (
                AccountRoutingError::Selection(AccountSelectionError::Unavailable),
                "gateway_account_unavailable",
            ),
        ] {
            let response = pool_error_response(ClaudeAccountPoolError::Routing(reason));
            assert_eq!(response.status(), 503);
            let body: Value = serde_json::from_slice(response.body()).unwrap();
            assert_eq!(body["error"]["source"], "gateway");
            assert_eq!(body["error"]["code"], code);
            assert!(body["error"].get("retry_at_ms").is_none());
            assert!(!body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Sign in"));
        }
    }

    #[test]
    fn claude_gateway_cooldown_reports_absolute_deadline() {
        use crate::sdk::cliproxy::auth::{AccountRoutingError, AccountSelectionError};

        let response = pool_error_response(ClaudeAccountPoolError::Routing(
            AccountRoutingError::Selection(AccountSelectionError::Cooldown {
                retry_after_ms: 1_791_645_000_000,
            }),
        ));
        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(response.status(), 503);
        assert_eq!(body["error"]["code"], "gateway_account_cooldown");
        assert_eq!(body["error"]["source"], "gateway");
        assert_eq!(body["error"]["retry_at_ms"], 1_791_645_000_000_i64);
        assert!(!body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("denied"));
    }

    #[test]
    fn request_requires_json_object_and_model() {
        assert_eq!(
            parse_request(b"not-json").unwrap_err(),
            "invalid JSON request body"
        );
        assert_eq!(
            parse_request(b"[]").unwrap_err(),
            "request body must be a JSON object"
        );
        assert_eq!(parse_request(b"{}").unwrap_err(), "model is required");
    }

    #[test]
    fn event_stream_requires_start_and_stop() {
        assert!(!valid_claude_event_stream(
            br#"data: {"type":"message_start","message":{"id":"msg_1"}}"#
        ));
        assert!(valid_claude_event_stream(
            b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\n\
              data: {\"type\":\"message_stop\"}\n\n"
        ));
        assert!(valid_claude_event_stream(
            b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\n\
              data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\n\n"
        ));
    }

    #[test]
    fn upstream_error_envelope_does_not_echo_provider_body() {
        let response =
            OpenAiResponsesHttpResponse::error(429, "Claude upstream rejected the request");
        let body: Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(response.status(), 429);
        assert_eq!(body["error"]["type"], "server_error");
        assert!(!String::from_utf8_lossy(response.body()).contains("token"));
    }

    #[test]
    fn terminal_stream_error_message_is_redacted_after_commit() {
        let mut chunk = b"event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"overloaded_error\",\"message\":\"token access-secret\"}}}".to_vec();
        redact_stream_failure_message(&mut chunk);
        let text = String::from_utf8(chunk).unwrap();
        assert!(text.contains("overloaded_error"));
        assert!(text.contains("Claude upstream stream failed"));
        assert!(!text.contains("access-secret"));
    }

    #[test]
    fn codex_terminal_event_is_reframed_after_recursive_message_redaction() {
        let mut value = serde_json::json!({
            "type": "response.failed",
            "response": {
                "error": {
                    "code": "upstream_error",
                    "message": "token codex-access-secret",
                    "nested": {"message": "refresh-secret"}
                }
            }
        });
        redact_json_messages(&mut value, "Codex upstream stream failed");
        let event = crate::internal::translator::common::SseEvent {
            event: Some("response.failed".to_owned()),
            data: serde_json::to_vec(&value).unwrap(),
            id: None,
            retry_millis: None,
        };
        let encoded = String::from_utf8(encode_sse_event(&event)).unwrap();
        assert!(encoded.contains("upstream_error"));
        assert!(encoded.contains("Codex upstream stream failed"));
        assert!(!encoded.contains("access-secret"));
        assert!(!encoded.contains("refresh-secret"));
    }
}

#[cfg(test)]
#[path = "openai_responses_compact_test.rs"]
mod openai_responses_compact_test;

#[cfg(test)]
#[path = "openai_responses_handlers_stream_test.rs"]
mod openai_responses_handlers_stream_test;

#[cfg(test)]
#[path = "openai_responses_signature_test.rs"]
mod openai_responses_signature_test;
