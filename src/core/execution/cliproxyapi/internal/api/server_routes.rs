// ref: internal/api/server_routes.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::internal::auth::codex::SecretString;
use crate::internal::runtime::executor::{
    ClaudeAccountPoolError, ClaudeExecutionError, ClaudeExecutionRequestContext,
    ClaudeMessagesTransportFailure, ClaudeSubscriptionAccountPool,
};
use crate::sdk::cliproxy::auth::{Auth, AuthKind};
use crate::sdk::cliproxy::executor::Headers;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuxiliaryRouteRequest {
    pub route: ServerRoute,
    pub method: String,
    pub target: String,
    pub provider: Option<String>,
    pub headers: Headers,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuxiliaryRouteResponse {
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl AuxiliaryRouteResponse {
    #[must_use]
    pub fn json_error(status: u16, message: &str) -> Self {
        let body = serde_json::to_vec(&serde_json::json!({ "error": message }))
            .unwrap_or_else(|_| br#"{"error":"auxiliary route failed"}"#.to_vec());
        Self {
            status,
            headers: Headers::from_iter([(
                "Content-Type".to_owned(),
                vec!["application/json".to_owned()],
            )]),
            body,
        }
    }
}

pub type AuxiliaryRouteFuture<'a> =
    Pin<Box<dyn Future<Output = Option<AuxiliaryRouteResponse>> + Send + 'a>>;

/// Host-owned extension point for provider routes whose authority or transport
/// cannot be inferred by the HTTP listener. Returning `None` preserves the
/// listener's fail-closed 404 behavior.
pub trait AuxiliaryRouteHandler: Send + Sync {
    fn handle<'a>(&'a self, request: AuxiliaryRouteRequest) -> AuxiliaryRouteFuture<'a>;
}

/// Deterministic host route composition. Each handler owns only its typed
/// route; an unclaimed route continues to the next handler and finally fails
/// closed in the listener.
pub struct AuxiliaryRouteChain {
    handlers: Vec<Arc<dyn AuxiliaryRouteHandler>>,
}

impl AuxiliaryRouteChain {
    pub fn new(handlers: Vec<Arc<dyn AuxiliaryRouteHandler>>) -> Self {
        Self { handlers }
    }
}

impl AuxiliaryRouteHandler for AuxiliaryRouteChain {
    fn handle<'a>(&'a self, request: AuxiliaryRouteRequest) -> AuxiliaryRouteFuture<'a> {
        Box::pin(async move {
            for handler in &self.handlers {
                if let Some(response) = handler.handle(request.clone()).await {
                    return Some(response);
                }
            }
            None
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerRoute {
    Root,
    Health,
    Models,
    ChatCompletions,
    Completions,
    Responses,
    ResponsesCompact,
    Messages,
    CountTokens,
    Images,
    Videos,
    AlphaSearch,
    Live,
    Realtime,
    Gemini,
    Interactions,
    OAuthCallback,
    ManagementPanel,
    Management,
    NotFound,
}

/// Host-authorized first-party Claude count-token route. Selection happens
/// once through the pool scheduler; the selected lane owns strong Claude Code
/// request context, native fingerprinted transport, refresh and persistence.
pub struct ClaudeCountTokensRouteHandler {
    pool: Arc<ClaudeSubscriptionAccountPool>,
}

impl ClaudeCountTokensRouteHandler {
    pub fn new(pool: Arc<ClaudeSubscriptionAccountPool>) -> Self {
        Self { pool }
    }
}

impl AuxiliaryRouteHandler for ClaudeCountTokensRouteHandler {
    fn handle<'a>(&'a self, request: AuxiliaryRouteRequest) -> AuxiliaryRouteFuture<'a> {
        Box::pin(async move {
            if request.route != ServerRoute::CountTokens
                || request
                    .provider
                    .as_deref()
                    .is_some_and(|provider| !provider.eq_ignore_ascii_case("claude"))
            {
                return None;
            }
            let model = match claude_count_tokens_model(&request.body) {
                Some(model) => model,
                None => {
                    return Some(AuxiliaryRouteResponse::json_error(
                        400,
                        "Claude count_tokens requires a model",
                    ))
                }
            };
            let auth_id = match self.pool.select_configured_auth_id_async(&model).await {
                Ok(auth_id) => auth_id,
                Err(error) => return Some(claude_count_tokens_error_response(error)),
            };
            let context = ClaudeExecutionRequestContext::from_provider_count_tokens_request(
                auth_id.clone(),
                request.headers,
                &request.body,
                &request.body,
                Default::default(),
                Default::default(),
            );
            Some(
                match self
                    .pool
                    .execute_count_tokens_selected_with_context(
                        &auth_id,
                        &model,
                        request.body,
                        Some(&context),
                    )
                    .await
                {
                    Ok(outcome) => {
                        let upstream = outcome.outcome().response();
                        AuxiliaryRouteResponse {
                            status: upstream.status(),
                            headers: upstream.headers().clone(),
                            body: upstream.body().to_vec(),
                        }
                    }
                    Err(error) => claude_count_tokens_error_response(error),
                },
            )
        })
    }
}

fn claude_count_tokens_model(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()?
        .get("model")?
        .as_str()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
}

fn claude_count_tokens_error_response(error: ClaudeAccountPoolError) -> AuxiliaryRouteResponse {
    let (status, message) = match error {
        ClaudeAccountPoolError::Routing(_) | ClaudeAccountPoolError::Configuration => {
            (503, "Claude subscription account unavailable")
        }
        ClaudeAccountPoolError::OutcomePersistence => {
            (503, "Claude account outcome could not be persisted")
        }
        ClaudeAccountPoolError::Execution(ClaudeExecutionError::CallerSystemBlock(_))
        | ClaudeAccountPoolError::Execution(ClaudeExecutionError::CredentialIdentity(_)) => {
            (400, "Claude count_tokens request rejected")
        }
        ClaudeAccountPoolError::Execution(ClaudeExecutionError::Transport(
            ClaudeMessagesTransportFailure::Cancelled,
        )) => (499, "Claude count_tokens request cancelled"),
        ClaudeAccountPoolError::Execution(ClaudeExecutionError::Auth(_)) => {
            (503, "Claude subscription credential unavailable")
        }
        ClaudeAccountPoolError::Execution(_) => (502, "Claude count_tokens upstream failed"),
    };
    AuxiliaryRouteResponse::json_error(status, message)
}

impl ServerRoute {
    #[must_use]
    pub fn allows_method(self, method: &str) -> bool {
        match self {
            Self::Root | Self::Models | Self::ManagementPanel => method.as_bytes() == b"GET",
            Self::Health => matches!(method.as_bytes(), b"GET" | b"HEAD"),
            Self::ChatCompletions
            | Self::Completions
            | Self::ResponsesCompact
            | Self::Messages
            | Self::CountTokens
            | Self::Images
            | Self::AlphaSearch
            | Self::Interactions => method.as_bytes() == b"POST",
            Self::Responses | Self::Live | Self::Realtime | Self::Gemini | Self::Videos => {
                matches!(method.as_bytes(), b"GET" | b"POST")
            }
            Self::OAuthCallback => method.as_bytes() == b"GET",
            Self::Management => true,
            Self::NotFound => false,
        }
    }
}

#[must_use]
pub fn resolve_server_route(target: &str) -> ServerRoute {
    let path = target.split('?').next().unwrap_or(target).trim();
    match path {
        "/" => ServerRoute::Root,
        "/healthz" => ServerRoute::Health,
        "/v1/models" => ServerRoute::Models,
        "/v1/chat/completions" => ServerRoute::ChatCompletions,
        "/v1/completions" => ServerRoute::Completions,
        "/v1/responses" | "/backend-api/codex/responses" => ServerRoute::Responses,
        "/v1/responses/compact" | "/backend-api/codex/responses/compact" => {
            ServerRoute::ResponsesCompact
        }
        "/v1/messages" => ServerRoute::Messages,
        "/v1/messages/count_tokens" => ServerRoute::CountTokens,
        "/v1/images/generations" | "/v1/images/edits" => ServerRoute::Images,
        "/v1/alpha/search" | "/backend-api/codex/alpha/search" => ServerRoute::AlphaSearch,
        "/v1/live" | "/v1/realtime/calls" | "/v1/realtime" => ServerRoute::Live,
        "/management.html" => ServerRoute::ManagementPanel,
        "/anthropic/callback"
        | "/codex/callback"
        | "/antigravity/callback"
        | "/github-copilot/callback" => ServerRoute::OAuthCallback,
        "/v1beta/interactions" => ServerRoute::Interactions,
        _ if path.starts_with("/v1/videos") || path.starts_with("/openai/v1/videos") => {
            ServerRoute::Videos
        }
        _ if path.starts_with("/v1/live/")
            || path.starts_with("/v1/realtime/calls/")
            || path.starts_with("/v1/realtime/") =>
        {
            ServerRoute::Realtime
        }
        _ if path.starts_with("/v1beta/models/") => ServerRoute::Gemini,
        _ if path.starts_with("/v0/management/") || path == "/v0/management" => {
            ServerRoute::Management
        }
        _ => ServerRoute::NotFound,
    }
}

/// Business OS data collections are deliberately absent. HTTP is only a
/// provider/control-plane surface; browser data remains on RxDB/WebRTC.
#[must_use]
pub fn is_business_data_route(target: &str) -> bool {
    let path = target.split('?').next().unwrap_or(target).trim();
    path.starts_with("/business-os/data") || path.starts_with("/rxdb/")
}

const CODEX_ALPHA_SEARCH_URL: &str = "https://chatgpt.com/backend-api/codex/alpha/search";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexAlphaSearchTransportRequest {
    pub url: String,
    pub auth_id: String,
    pub model: String,
    pub headers: Headers,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexAlphaSearchResponse {
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodexAlphaSearchError {
    MissingApiKeyBaseUrl,
    RefreshUnavailable,
    Transport,
}

pub type CodexAlphaSearchFuture<'a> = Pin<
    Box<dyn Future<Output = Result<CodexAlphaSearchResponse, CodexAlphaSearchError>> + Send + 'a>,
>;

pub trait CodexAlphaSearchTransport: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: CodexAlphaSearchTransportRequest,
    ) -> CodexAlphaSearchFuture<'a>;
}

pub trait CodexAlphaSearchRefresher: Send + Sync {
    fn report_unauthorized<'a>(
        &'a self,
        current: &'a Auth,
        model: &'a str,
    ) -> CodexAlphaSearchStatusFuture<'a>;

    fn refresh_after_unauthorized<'a>(
        &'a self,
        current: &'a Auth,
        model: &'a str,
    ) -> CodexAlphaSearchSelectionFuture<'a>;

    fn report_status<'a>(
        &'a self,
        _current: &'a Auth,
        _model: &'a str,
        _status: u16,
    ) -> CodexAlphaSearchStatusFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

pub type CodexAlphaSearchSelectionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Auth, CodexAlphaSearchError>> + Send + 'a>>;
pub type CodexAlphaSearchStatusFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), CodexAlphaSearchError>> + Send + 'a>>;

/// Selects a Codex credential at the host authority boundary. The HTTP layer
/// deliberately does not discover credentials or instantiate vendor auth.
pub trait CodexAlphaSearchAuthSelector: Send + Sync {
    fn select<'a>(
        &'a self,
        model: &'a str,
        headers: &'a Headers,
        original_body: &'a [u8],
    ) -> CodexAlphaSearchSelectionFuture<'a>;
}

/// Host-injected Alpha Search execution boundary. A Home-selected OAuth
/// credential is refreshed and replayed exactly once after a 401; API-key
/// credentials remain bound to their configured compatibility base URL.
pub struct CodexAlphaSearchClient {
    transport: Arc<dyn CodexAlphaSearchTransport>,
    refresher: Option<Arc<dyn CodexAlphaSearchRefresher>>,
}

impl CodexAlphaSearchClient {
    #[must_use]
    pub fn new(transport: Arc<dyn CodexAlphaSearchTransport>) -> Self {
        Self {
            transport,
            refresher: None,
        }
    }

    #[must_use]
    pub fn with_refresher(mut self, refresher: Arc<dyn CodexAlphaSearchRefresher>) -> Self {
        self.refresher = Some(refresher);
        self
    }

    pub async fn execute(
        &self,
        selected: &Auth,
        model: &str,
        source_headers: &Headers,
        body: &[u8],
    ) -> Result<CodexAlphaSearchResponse, CodexAlphaSearchError> {
        let mut current = selected.clone();
        let mut response = self
            .transport
            .execute(alpha_search_request(&current, model, source_headers, body)?)
            .await?;
        if response.status == 401 {
            if let Some(refresher) = &self.refresher {
                refresher.report_unauthorized(&current, model).await?;
                current = refresher
                    .refresh_after_unauthorized(&current, model)
                    .await?;
                response = self
                    .transport
                    .execute(alpha_search_request(&current, model, source_headers, body)?)
                    .await?;
                if response.status == 401 {
                    refresher.report_unauthorized(&current, model).await?;
                } else {
                    refresher
                        .report_status(&current, model, response.status)
                        .await?;
                }
            } else {
                return Ok(response);
            }
        } else if let Some(refresher) = &self.refresher {
            refresher
                .report_status(&current, model, response.status)
                .await?;
        }
        Ok(response)
    }
}

#[derive(Clone)]
pub struct CodexAlphaSearchCredentials {
    pub access_token: SecretString,
    pub account_id: String,
}

impl std::fmt::Debug for CodexAlphaSearchCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodexAlphaSearchCredentials")
            .field("access_token", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .finish()
    }
}

pub type CodexAlphaSearchCredentialsFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<CodexAlphaSearchCredentials, CodexAlphaSearchError>> + Send + 'a,
    >,
>;

pub trait CodexAlphaSearchCredentialSource: Send + Sync {
    fn credentials<'a>(&'a self, auth_id: &'a str) -> CodexAlphaSearchCredentialsFuture<'a>;
}

#[cfg(feature = "codex-http-transport")]
mod native_alpha_search {
    use std::collections::HashMap;
    use std::time::Duration;

    use wreq::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
    use wreq::{Client, Proxy};

    use super::*;

    const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

    #[derive(Clone)]
    pub struct CodexAlphaSearchHttpTransport {
        clients: HashMap<String, Client>,
        credentials: Arc<dyn CodexAlphaSearchCredentialSource>,
        timeout: Duration,
    }

    impl CodexAlphaSearchHttpTransport {
        pub fn new(
            proxy_urls: &HashMap<String, Option<String>>,
            credentials: Arc<dyn CodexAlphaSearchCredentialSource>,
            timeout: Duration,
        ) -> Result<Self, CodexAlphaSearchError> {
            let mut clients = HashMap::new();
            for (auth_id, proxy_url) in proxy_urls {
                let mut builder = Client::builder()
                    .connect_timeout(CONNECT_TIMEOUT)
                    .retry(wreq::retry::Policy::never())
                    .redirect(wreq::redirect::Policy::none());
                if let Some(proxy_url) = proxy_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    builder = builder.proxy(
                        Proxy::all(proxy_url).map_err(|_| CodexAlphaSearchError::Transport)?,
                    );
                } else {
                    builder = builder.no_proxy();
                }
                clients.insert(
                    auth_id.clone(),
                    builder
                        .build()
                        .map_err(|_| CodexAlphaSearchError::Transport)?,
                );
            }
            Ok(Self {
                clients,
                credentials,
                timeout,
            })
        }
    }

    impl CodexAlphaSearchTransport for CodexAlphaSearchHttpTransport {
        fn execute<'a>(
            &'a self,
            request: CodexAlphaSearchTransportRequest,
        ) -> CodexAlphaSearchFuture<'a> {
            Box::pin(async move {
                let client = self
                    .clients
                    .get(&request.auth_id)
                    .ok_or(CodexAlphaSearchError::Transport)?;
                let credentials = self.credentials.credentials(&request.auth_id).await?;
                let mut outgoing = client
                    .post(&request.url)
                    .header(CONTENT_TYPE, "application/json")
                    .header(ACCEPT, "application/json")
                    .header(
                        AUTHORIZATION,
                        format!("Bearer {}", credentials.access_token.expose_secret()),
                    )
                    .timeout(self.timeout)
                    .body(request.body);
                if !credentials.account_id.trim().is_empty() {
                    outgoing = outgoing.header("Chatgpt-Account-Id", credentials.account_id);
                }
                for (name, values) in request.headers {
                    if name.eq_ignore_ascii_case("authorization")
                        || name.eq_ignore_ascii_case("content-length")
                        || name.eq_ignore_ascii_case("transfer-encoding")
                    {
                        continue;
                    }
                    for value in values {
                        outgoing = outgoing.header(&name, value);
                    }
                }
                let response = outgoing
                    .send()
                    .await
                    .map_err(|_| CodexAlphaSearchError::Transport)?;
                let status = response.status().as_u16();
                let mut headers = Headers::new();
                for (name, value) in response.headers() {
                    if name.as_str().eq_ignore_ascii_case("content-length")
                        || name.as_str().eq_ignore_ascii_case("content-encoding")
                        || name.as_str().eq_ignore_ascii_case("transfer-encoding")
                        || name.as_str().eq_ignore_ascii_case("connection")
                    {
                        continue;
                    }
                    if let Ok(value) = value.to_str() {
                        headers
                            .entry(name.as_str().to_owned())
                            .or_default()
                            .push(value.to_owned());
                    }
                }
                let body = response
                    .bytes()
                    .await
                    .map_err(|_| CodexAlphaSearchError::Transport)?
                    .to_vec();
                Ok(CodexAlphaSearchResponse {
                    status,
                    headers,
                    body,
                })
            })
        }
    }
}

#[cfg(feature = "codex-http-transport")]
pub use native_alpha_search::CodexAlphaSearchHttpTransport;

/// Buffered HTTP adapter for the standalone Alpha Search endpoint. Search is
/// already in Codex wire format and therefore bypasses protocol translation.
pub struct CodexAlphaSearchRouteHandler {
    selector: Arc<dyn CodexAlphaSearchAuthSelector>,
    client: Arc<CodexAlphaSearchClient>,
}

impl CodexAlphaSearchRouteHandler {
    #[must_use]
    pub fn new(
        selector: Arc<dyn CodexAlphaSearchAuthSelector>,
        client: Arc<CodexAlphaSearchClient>,
    ) -> Self {
        Self { selector, client }
    }
}

impl AuxiliaryRouteHandler for CodexAlphaSearchRouteHandler {
    fn handle<'a>(&'a self, request: AuxiliaryRouteRequest) -> AuxiliaryRouteFuture<'a> {
        Box::pin(async move {
            if request.route != ServerRoute::AlphaSearch {
                return None;
            }

            let (model, session_id) = alpha_search_routing_fields(&request.body);
            let mut selection_headers = request.headers.clone();
            if let Some(session_id) = session_id {
                selection_headers.insert("X-Session-ID".to_owned(), vec![session_id]);
            }
            let selected = match self
                .selector
                .select(&model, &selection_headers, &request.body)
                .await
            {
                Ok(selected) => selected,
                Err(error) => return Some(alpha_search_error_response(error)),
            };
            let upstream_body = sanitize_codex_alpha_search_body(&request.body);
            Some(
                match self
                    .client
                    .execute(&selected, &model, &request.headers, &upstream_body)
                    .await
                {
                    Ok(response) => AuxiliaryRouteResponse {
                        status: response.status,
                        headers: response.headers,
                        body: response.body,
                    },
                    Err(error) => alpha_search_error_response(error),
                },
            )
        })
    }
}

fn alpha_search_error_response(error: CodexAlphaSearchError) -> AuxiliaryRouteResponse {
    let (status, message) = match error {
        CodexAlphaSearchError::MissingApiKeyBaseUrl => {
            (503, "Codex Alpha Search API key base URL unavailable")
        }
        CodexAlphaSearchError::RefreshUnavailable => {
            (503, "Codex Alpha Search credential unavailable")
        }
        CodexAlphaSearchError::Transport => (502, "Codex Alpha Search upstream failed"),
    };
    AuxiliaryRouteResponse::json_error(status, message)
}

fn alpha_search_routing_fields(body: &[u8]) -> (String, Option<String>) {
    let payload = serde_json::from_slice::<serde_json::Value>(body).ok();
    let model = payload
        .as_ref()
        .and_then(|payload| payload.get("model"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let session_id = payload
        .as_ref()
        .and_then(|payload| payload.get("id"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    (model, session_id)
}

#[must_use]
pub fn sanitize_codex_alpha_search_body(body: &[u8]) -> Vec<u8> {
    let Ok(mut payload) =
        serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(body)
    else {
        return body.to_vec();
    };
    let removed = payload.remove("prompt_cache_key").is_some()
        | payload.remove("prompt_cache_retention").is_some();
    if !removed {
        return body.to_vec();
    }
    serde_json::to_vec(&payload).unwrap_or_else(|_| body.to_vec())
}

fn alpha_search_request(
    auth: &Auth,
    model: &str,
    source_headers: &Headers,
    body: &[u8],
) -> Result<CodexAlphaSearchTransportRequest, CodexAlphaSearchError> {
    let url = if auth.auth_kind() == Some(AuthKind::ApiKey) {
        let base = auth
            .attributes
            .get("base_url")
            .map(String::as_str)
            .unwrap_or_default()
            .trim()
            .trim_end_matches('/');
        if base.is_empty() {
            return Err(CodexAlphaSearchError::MissingApiKeyBaseUrl);
        }
        format!("{base}/alpha/search")
    } else {
        CODEX_ALPHA_SEARCH_URL.to_owned()
    };
    let mut headers = Headers::from_iter([
        (
            "Content-Type".to_owned(),
            vec!["application/json".to_owned()],
        ),
        ("Accept".to_owned(), vec!["application/json".to_owned()]),
        ("Originator".to_owned(), vec!["codex_cli_rs".to_owned()]),
    ]);
    for name in ["Version", "User-Agent", "Session_id", "X-Client-Request-Id"] {
        if let Some(values) = header_values(source_headers, name) {
            headers.insert(name.to_owned(), values);
        }
    }
    if let Some(account_id) = auth
        .metadata
        .get("account_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        headers.insert("Chatgpt-Account-Id".to_owned(), vec![account_id.to_owned()]);
    }
    Ok(CodexAlphaSearchTransportRequest {
        url,
        auth_id: auth.id.clone(),
        model: model.trim().to_owned(),
        headers,
        body: body.to_vec(),
    })
}

fn header_values(headers: &Headers, name: &str) -> Option<Vec<String>> {
    headers
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
        .map(|(_, values)| values.clone())
        .filter(|values| values.iter().any(|value| !value.trim().is_empty()))
}
