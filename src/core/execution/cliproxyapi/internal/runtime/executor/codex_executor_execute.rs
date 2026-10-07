// ref: internal/runtime/executor/codex_executor_execute.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::internal::auth::codex::parse_jwt_token;
use crate::internal::translator::common::SseDecoder;
use crate::sdk::cliproxy::auth::{
    AccountCandidate, AccountExecutionResult, AccountRouter, AccountRoutingError, CooldownConductor,
};

use super::claude_executor_execute::AccountStateClock;

use super::codex_executor::{
    CodexResponsesRequest, CodexResponsesResponse, CodexResponsesStreamResponse,
    CodexResponsesStreamingTransport, CodexResponsesTransport, CodexResponsesTransportFailure,
    CodexUpstreamTarget,
};
use super::codex_executor_auth::{CodexSubscriptionAuth, CodexSubscriptionAuthError};
use super::codex_executor_reasoning::{CodexReasoningReplayCache, CodexReasoningReplayScope};
use super::codex_executor_request::{
    apply_codex_cloaking_headers, apply_codex_identity_confuse_body,
    apply_codex_identity_confuse_headers, prepare_codex_compact_body, prepare_codex_responses_body,
    CodexHeaderPolicy, CodexIdentityConfuseState, CodexIdentityPolicy, CodexRequestError,
    CodexRequestPolicy,
};
use super::codex_executor_terminal::{CodexTerminalAccumulator, CodexTerminalEvent};
use super::codex_executor_tokens::{codex_token_count_response, CodexTokenCountError};
use super::codex_openai_images::{prepare_codex_direct_image_request, CodexImageError};

pub struct CodexSubscriptionResponsesExecutor {
    auth: Arc<CodexSubscriptionAuth>,
    transport: Arc<dyn CodexResponsesTransport>,
    stream_transport: Option<Arc<dyn CodexResponsesStreamingTransport>>,
    timeout: Duration,
    plan_type: String,
    disable_image_generation: bool,
    reasoning: Option<Arc<CodexReasoningReplayCache>>,
    identity: Option<(String, CodexIdentityPolicy)>,
    header_policy: CodexHeaderPolicy,
}

impl CodexSubscriptionResponsesExecutor {
    pub fn new(
        auth: Arc<CodexSubscriptionAuth>,
        transport: Arc<dyn CodexResponsesTransport>,
        timeout: Duration,
    ) -> Result<Self, CodexExecutionError> {
        if timeout.is_zero() {
            return Err(CodexExecutionError::InvalidTimeout);
        }
        Ok(Self {
            auth,
            transport,
            stream_transport: None,
            timeout,
            plan_type: String::new(),
            disable_image_generation: false,
            reasoning: None,
            identity: None,
            header_policy: CodexHeaderPolicy::default(),
        })
    }

    pub fn with_plan_type(mut self, plan_type: impl Into<String>) -> Self {
        self.plan_type = plan_type.into();
        self
    }

    pub fn with_stream_transport(
        mut self,
        transport: Arc<dyn CodexResponsesStreamingTransport>,
    ) -> Self {
        self.stream_transport = Some(transport);
        self
    }

    pub fn disable_image_generation(mut self, disabled: bool) -> Self {
        self.disable_image_generation = disabled;
        self
    }

    pub fn with_reasoning_replay(mut self, cache: Arc<CodexReasoningReplayCache>) -> Self {
        self.reasoning = Some(cache);
        self
    }

    pub fn with_identity_policy(
        mut self,
        auth_id: impl Into<String>,
        policy: CodexIdentityPolicy,
    ) -> Self {
        self.identity = Some((auth_id.into(), policy));
        self
    }

    pub fn with_header_policy(mut self, policy: CodexHeaderPolicy) -> Self {
        self.header_policy = policy;
        self
    }

    /// Counts an already Responses-shaped request locally. Like upstream this
    /// does not load subscription credentials or issue an upstream request.
    pub fn count_tokens(
        &self,
        model: &str,
        body: &[u8],
    ) -> Result<CodexExecutionOutcome, CodexExecutionError> {
        let payload =
            codex_token_count_response(model, body).map_err(CodexExecutionError::TokenCount)?;
        Ok(CodexExecutionOutcome {
            payload,
            attempts: 0,
        })
    }

    fn apply_identity_and_headers(
        &self,
        user_payload: &[u8],
        upstream_body: Vec<u8>,
    ) -> (Vec<u8>, CodexIdentityConfuseState, BTreeMap<String, String>) {
        let (body, mut state) = match &self.identity {
            Some((auth_id, policy)) => {
                apply_codex_identity_confuse_body(policy, auth_id, user_payload, &upstream_body)
            }
            None => (upstream_body, CodexIdentityConfuseState::default()),
        };
        let mut headers = BTreeMap::new();
        apply_codex_cloaking_headers(&mut headers, &self.header_policy);
        if let Some((auth_id, _)) = &self.identity {
            apply_codex_identity_confuse_headers(&mut headers, auth_id, &mut state);
        }
        (body, state, headers)
    }

    pub async fn execute(
        &self,
        target: &CodexUpstreamTarget,
        model: &str,
        body: &[u8],
        responses_lite: bool,
    ) -> Result<CodexExecutionOutcome, CodexExecutionError> {
        let prepared_body = prepare_codex_responses_body(
            body,
            CodexRequestPolicy {
                model,
                plan_type: &self.plan_type,
                responses_lite,
                disable_image_generation: self.disable_image_generation,
            },
        )
        .map_err(CodexExecutionError::Request)?;
        let client_session_id = serde_json::from_slice::<Value>(&prepared_body)
            .ok()
            .and_then(|value| {
                value
                    .get("prompt_cache_key")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            });
        let replay_scope = client_session_id.as_deref().and_then(|session| {
            CodexReasoningReplayScope::from_request(model, session, &prepared_body)
        });
        let prepared_body = match (&self.reasoning, &replay_scope) {
            (Some(cache), Some(scope)) => cache.apply(scope, &prepared_body),
            _ => prepared_body,
        };
        let (prepared_body, identity_state, request_headers) =
            self.apply_identity_and_headers(body, prepared_body);
        let session_id = serde_json::from_slice::<Value>(&prepared_body)
            .ok()
            .and_then(|value| {
                value
                    .get("prompt_cache_key")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            });

        let mut credentials = self.auth.load().await.map_err(CodexExecutionError::Auth)?;
        for attempt in 1..=2 {
            let account_id = parse_jwt_token(credentials.id_token().expose_secret())
                .map(|claims| claims.account_id().to_owned())
                .unwrap_or_default();
            let request = CodexResponsesRequest::new(
                target,
                credentials.access_token().clone(),
                account_id,
                session_id.clone(),
                prepared_body.clone(),
            )
            .with_headers(request_headers.clone());
            let response = self
                .transport
                .execute(&request, self.timeout)
                .await
                .map_err(CodexExecutionError::Transport)?;
            if response.status() == 401 && attempt == 1 {
                crate::internal::api::account_selection::record_upstream_status(401, &[]);
                credentials = self
                    .auth
                    .refresh_after_status(401)
                    .await
                    .map_err(CodexExecutionError::Auth)?
                    .credentials()
                    .clone();
                continue;
            }
            if !(200..300).contains(&response.status()) {
                crate::internal::api::account_selection::record_upstream_status(
                    response.status(),
                    response.body(),
                );
                if let (Some(cache), Some(scope)) = (&self.reasoning, &replay_scope) {
                    cache.clear_on_invalid_signature(scope, response.status(), response.body());
                }
                return Err(CodexExecutionError::Http {
                    status: response.status(),
                    retry_delay_ms: parse_retry_after_delay_ms(response.retry_after()),
                });
            }
            let payload = identity_state.expose_response(&aggregate_codex_response(response)?);
            if let (Some(cache), Some(scope)) = (&self.reasoning, replay_scope.clone()) {
                cache.commit_completed(scope, &payload);
            }
            return Ok(CodexExecutionOutcome {
                payload,
                attempts: attempt,
            });
        }
        Err(CodexExecutionError::ReplayExhausted)
    }

    pub async fn execute_compact(
        &self,
        target: &CodexUpstreamTarget,
        model: &str,
        body: &[u8],
    ) -> Result<CodexExecutionOutcome, CodexExecutionError> {
        let prepared_body =
            prepare_codex_compact_body(body, model).map_err(CodexExecutionError::Request)?;
        let (prepared_body, identity_state, request_headers) =
            self.apply_identity_and_headers(body, prepared_body);
        let session_id = serde_json::from_slice::<Value>(&prepared_body)
            .ok()
            .and_then(|value| {
                value
                    .get("prompt_cache_key")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            });
        let mut credentials = self.auth.load().await.map_err(CodexExecutionError::Auth)?;
        for attempt in 1..=2 {
            let account_id = parse_jwt_token(credentials.id_token().expose_secret())
                .map(|claims| claims.account_id().to_owned())
                .unwrap_or_default();
            let request = CodexResponsesRequest::compact(
                target,
                credentials.access_token().clone(),
                account_id,
                session_id.clone(),
                prepared_body.clone(),
            )
            .with_headers(request_headers.clone());
            let response = self
                .transport
                .execute(&request, self.timeout)
                .await
                .map_err(CodexExecutionError::Transport)?;
            if response.status() == 401 && attempt == 1 {
                crate::internal::api::account_selection::record_upstream_status(401, &[]);
                credentials = self
                    .auth
                    .refresh_after_status(401)
                    .await
                    .map_err(CodexExecutionError::Auth)?
                    .credentials()
                    .clone();
                continue;
            }
            if !(200..300).contains(&response.status()) {
                crate::internal::api::account_selection::record_upstream_status(
                    response.status(),
                    response.body(),
                );
                return Err(CodexExecutionError::Http {
                    status: response.status(),
                    retry_delay_ms: parse_retry_after_delay_ms(response.retry_after()),
                });
            }
            let payload = serde_json::from_slice::<Value>(response.body())
                .and_then(|value| serde_json::to_vec(&value))
                .map_err(|_| CodexExecutionError::InvalidCompletion)?;
            return Ok(CodexExecutionOutcome {
                payload: identity_state.expose_response(&payload),
                attempts: attempt,
            });
        }
        Err(CodexExecutionError::ReplayExhausted)
    }

    pub async fn execute_stream(
        &self,
        target: &CodexUpstreamTarget,
        model: &str,
        body: &[u8],
        responses_lite: bool,
    ) -> Result<CodexStreamExecutionOutcome, CodexExecutionError> {
        let transport = self
            .stream_transport
            .as_ref()
            .ok_or(CodexExecutionError::StreamingUnavailable)?;
        let prepared_body = prepare_codex_responses_body(
            body,
            CodexRequestPolicy {
                model,
                plan_type: &self.plan_type,
                responses_lite,
                disable_image_generation: self.disable_image_generation,
            },
        )
        .map_err(CodexExecutionError::Request)?;
        let client_session_id = serde_json::from_slice::<Value>(&prepared_body)
            .ok()
            .and_then(|value| {
                value
                    .get("prompt_cache_key")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            });
        let replay_scope = client_session_id.as_deref().and_then(|session| {
            CodexReasoningReplayScope::from_request(model, session, &prepared_body)
        });
        let prepared_body = match (&self.reasoning, &replay_scope) {
            (Some(cache), Some(scope)) => cache.apply(scope, &prepared_body),
            _ => prepared_body,
        };
        let (prepared_body, identity_state, request_headers) =
            self.apply_identity_and_headers(body, prepared_body);
        let session_id = serde_json::from_slice::<Value>(&prepared_body)
            .ok()
            .and_then(|value| {
                value
                    .get("prompt_cache_key")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            });
        let mut credentials = self.auth.load().await.map_err(CodexExecutionError::Auth)?;
        for attempt in 1..=2 {
            let account_id = parse_jwt_token(credentials.id_token().expose_secret())
                .map(|claims| claims.account_id().to_owned())
                .unwrap_or_default();
            let request = CodexResponsesRequest::new(
                target,
                credentials.access_token().clone(),
                account_id,
                session_id.clone(),
                prepared_body.clone(),
            )
            .with_headers(request_headers.clone());
            let mut response = transport
                .execute_stream(&request, self.timeout)
                .await
                .map_err(CodexExecutionError::Transport)?;
            if response.status() == 401 && attempt == 1 {
                crate::internal::api::account_selection::record_upstream_status(401, &[]);
                credentials = self
                    .auth
                    .refresh_after_status(401)
                    .await
                    .map_err(CodexExecutionError::Auth)?
                    .credentials()
                    .clone();
                continue;
            }
            if (200..300).contains(&response.status())
                && response.bootstrap_first_response_event().await.is_err()
            {
                response = CodexResponsesStreamResponse::synthetic(502);
            }
            if (200..300).contains(&response.status()) {
                if let (Some(cache), Some(scope)) = (&self.reasoning, replay_scope.clone()) {
                    response.attach_reasoning_replay(Arc::clone(cache), scope);
                }
                response.attach_identity(identity_state.clone());
            }
            return Ok(CodexStreamExecutionOutcome {
                response,
                attempts: attempt,
            });
        }
        Err(CodexExecutionError::ReplayExhausted)
    }

    pub async fn execute_direct_image(
        &self,
        target: &CodexUpstreamTarget,
        route_model: &str,
        request_path: &str,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Result<CodexExecutionOutcome, CodexExecutionError> {
        let prepared = prepare_codex_direct_image_request(
            body,
            route_model,
            request_path,
            content_type,
            false,
        )
        .map_err(CodexExecutionError::Image)?;
        let (prepared_body, identity_state, request_headers) =
            self.apply_identity_and_headers(body, prepared.body);
        let mut credentials = self.auth.load().await.map_err(CodexExecutionError::Auth)?;
        for attempt in 1..=2 {
            let account_id = parse_jwt_token(credentials.id_token().expose_secret())
                .map(|claims| claims.account_id().to_owned())
                .unwrap_or_default();
            let request = CodexResponsesRequest::direct_image(
                target,
                prepared.endpoint_path,
                credentials.access_token().clone(),
                account_id,
                prepared_body.clone(),
                prepared.content_type.clone(),
                false,
            )
            .map_err(|_| CodexExecutionError::Image(CodexImageError::UnsupportedEndpoint))?
            .with_headers(request_headers.clone());
            let response = self
                .transport
                .execute(&request, self.timeout)
                .await
                .map_err(CodexExecutionError::Transport)?;
            if response.status() == 401 && attempt == 1 {
                crate::internal::api::account_selection::record_upstream_status(401, &[]);
                credentials = self
                    .auth
                    .refresh_after_status(401)
                    .await
                    .map_err(CodexExecutionError::Auth)?
                    .credentials()
                    .clone();
                continue;
            }
            if !(200..300).contains(&response.status()) {
                crate::internal::api::account_selection::record_upstream_status(
                    response.status(),
                    response.body(),
                );
                return Err(CodexExecutionError::Http {
                    status: response.status(),
                    retry_delay_ms: parse_retry_after_delay_ms(response.retry_after()),
                });
            }
            return Ok(CodexExecutionOutcome {
                payload: identity_state.expose_response(response.body()),
                attempts: attempt,
            });
        }
        Err(CodexExecutionError::ReplayExhausted)
    }

    pub async fn execute_direct_image_stream(
        &self,
        target: &CodexUpstreamTarget,
        route_model: &str,
        request_path: &str,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Result<CodexStreamExecutionOutcome, CodexExecutionError> {
        let transport = self
            .stream_transport
            .as_ref()
            .ok_or(CodexExecutionError::StreamingUnavailable)?;
        let prepared =
            prepare_codex_direct_image_request(body, route_model, request_path, content_type, true)
                .map_err(CodexExecutionError::Image)?;
        let (prepared_body, identity_state, request_headers) =
            self.apply_identity_and_headers(body, prepared.body);
        let mut credentials = self.auth.load().await.map_err(CodexExecutionError::Auth)?;
        for attempt in 1..=2 {
            let account_id = parse_jwt_token(credentials.id_token().expose_secret())
                .map(|claims| claims.account_id().to_owned())
                .unwrap_or_default();
            let request = CodexResponsesRequest::direct_image(
                target,
                prepared.endpoint_path,
                credentials.access_token().clone(),
                account_id,
                prepared_body.clone(),
                prepared.content_type.clone(),
                true,
            )
            .map_err(|_| CodexExecutionError::Image(CodexImageError::UnsupportedEndpoint))?
            .with_headers(request_headers.clone());
            let mut response = transport
                .execute_stream(&request, self.timeout)
                .await
                .map_err(CodexExecutionError::Transport)?;
            // Direct image endpoints may stream provider-native payloads rather
            // than Responses events. Keep this invariant at the executor
            // boundary as well as in the native HTTP transport so alternate
            // transports cannot accidentally require `response.completed`.
            response.set_passthrough();
            if response.status() == 401 && attempt == 1 {
                crate::internal::api::account_selection::record_upstream_status(401, &[]);
                credentials = self
                    .auth
                    .refresh_after_status(401)
                    .await
                    .map_err(CodexExecutionError::Auth)?
                    .credentials()
                    .clone();
                continue;
            }
            if !(200..300).contains(&response.status()) {
                crate::internal::api::account_selection::record_upstream_status(
                    response.status(),
                    &[],
                );
                return Err(CodexExecutionError::Http {
                    status: response.status(),
                    retry_delay_ms: parse_retry_after_delay_ms(response.retry_after()),
                });
            }
            response.attach_identity(identity_state.clone());
            return Ok(CodexStreamExecutionOutcome {
                response,
                attempts: attempt,
            });
        }
        Err(CodexExecutionError::ReplayExhausted)
    }
}

impl fmt::Debug for CodexSubscriptionResponsesExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexSubscriptionResponsesExecutor")
            .field("auth", &"CodexSubscriptionAuth([REDACTED])")
            .field("transport", &"CodexResponsesTransport")
            .field(
                "stream_transport",
                &self.stream_transport.as_ref().map(|_| "attached"),
            )
            .field("timeout", &self.timeout)
            .field("plan_type", &self.plan_type)
            .field("disable_image_generation", &self.disable_image_generation)
            .field("reasoning", &self.reasoning)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct CodexExecutionOutcome {
    payload: Vec<u8>,
    attempts: usize,
}

pub struct CodexStreamExecutionOutcome {
    response: CodexResponsesStreamResponse,
    attempts: usize,
}

impl CodexStreamExecutionOutcome {
    pub fn response(&self) -> &CodexResponsesStreamResponse {
        &self.response
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }

    fn into_tracked(
        self,
        auth_id: String,
        model: String,
        conductor: Arc<CooldownConductor>,
        clock: Arc<dyn AccountStateClock>,
    ) -> CodexTrackedResponsesStreamResponse {
        CodexTrackedResponsesStreamResponse {
            response: self.response,
            auth_id,
            model,
            conductor,
            clock,
            failure_recorded: false,
        }
    }
}

impl fmt::Debug for CodexStreamExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexStreamExecutionOutcome")
            .field("response", &self.response)
            .field("attempts", &self.attempts)
            .finish()
    }
}

impl fmt::Debug for CodexExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexExecutionOutcome")
            .field("payload", &"[REDACTED]")
            .field("attempts", &self.attempts)
            .finish()
    }
}

impl CodexExecutionOutcome {
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexExecutionError {
    InvalidTimeout,
    Request(CodexRequestError),
    Image(CodexImageError),
    TokenCount(CodexTokenCountError),
    Auth(CodexSubscriptionAuthError),
    Transport(CodexResponsesTransportFailure),
    Http {
        status: u16,
        retry_delay_ms: Option<u64>,
    },
    Terminal {
        status: u16,
        code: Option<String>,
    },
    IncompleteStream,
    InvalidCompletion,
    ReplayExhausted,
    StreamingUnavailable,
}

impl fmt::Display for CodexExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTimeout => formatter.write_str("Codex request timeout is invalid"),
            Self::Request(_) => formatter.write_str("Codex request is invalid"),
            Self::Image(error) => write!(formatter, "Codex image request failed: {error}"),
            Self::TokenCount(error) => write!(formatter, "Codex token counting failed: {error}"),
            Self::Auth(error) => write!(formatter, "Codex authentication failed: {error}"),
            Self::Transport(kind) => write!(formatter, "Codex transport failed: {kind:?}"),
            Self::Http { status, .. } => {
                write!(formatter, "Codex upstream returned status {status}")
            }
            Self::Terminal { status, code } => {
                write!(formatter, "Codex stream failed with status {status}")?;
                if let Some(code) = code {
                    write!(formatter, " ({code})")?;
                }
                Ok(())
            }
            Self::IncompleteStream => {
                formatter.write_str("Codex stream closed before response completion")
            }
            Self::InvalidCompletion => formatter.write_str("Codex completion is invalid"),
            Self::ReplayExhausted => formatter.write_str("Codex unauthorized replay exhausted"),
            Self::StreamingUnavailable => {
                formatter.write_str("Codex streaming transport is unavailable")
            }
        }
    }
}

impl std::error::Error for CodexExecutionError {}

impl CodexExecutionError {
    fn account_status(&self) -> Option<(u16, Option<u64>)> {
        match self {
            Self::Http {
                status,
                retry_delay_ms,
            } => Some((*status, *retry_delay_ms)),
            Self::Terminal { status, .. } => Some((*status, None)),
            Self::IncompleteStream => Some((408, None)),
            Self::Transport(_) => Some((502, None)),
            Self::Request(_) => Some((400, None)),
            Self::Image(_) => Some((400, None)),
            Self::TokenCount(_) => Some((400, None)),
            Self::InvalidCompletion => Some((502, None)),
            Self::Auth(_)
            | Self::InvalidTimeout
            | Self::ReplayExhausted
            | Self::StreamingUnavailable => None,
        }
    }
}

fn parse_retry_after_delay_ms(raw: Option<&str>) -> Option<u64> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(seconds) = raw.parse::<u64>() {
        return seconds.checked_mul(1_000);
    }
    let deadline = httpdate::parse_http_date(raw).ok()?;
    let delay = deadline.duration_since(SystemTime::now()).ok()?;
    u64::try_from(delay.as_millis()).ok()
}

/// Bounded, persisted-state Codex subscription account selection and failover.
///
/// The pool records every account-scoped outcome before returning or trying a
/// different account. A failed state write is therefore a hard failure rather
/// than an implicit downgrade to in-memory routing.
pub struct CodexSubscriptionAccountPool {
    router: Arc<AccountRouter>,
    conductor: Arc<CooldownConductor>,
    candidates: Vec<AccountCandidate>,
    executors: HashMap<String, Arc<CodexSubscriptionResponsesExecutor>>,
    targets: HashMap<String, CodexUpstreamTarget>,
    clock: Arc<dyn AccountStateClock>,
}

impl CodexSubscriptionAccountPool {
    pub fn with_clock(
        router: Arc<AccountRouter>,
        conductor: Arc<CooldownConductor>,
        candidates: Vec<AccountCandidate>,
        executors: HashMap<String, Arc<CodexSubscriptionResponsesExecutor>>,
        targets: HashMap<String, CodexUpstreamTarget>,
        clock: Arc<dyn AccountStateClock>,
    ) -> Result<Self, CodexAccountPoolError> {
        if candidates.is_empty() {
            return Err(CodexAccountPoolError::Configuration);
        }
        let mut seen = HashSet::new();
        for candidate in &candidates {
            if !candidate.provider.eq_ignore_ascii_case("codex")
                || candidate.auth_id.trim().is_empty()
                || !seen.insert(candidate.auth_id.as_str())
                || !executors.contains_key(&candidate.auth_id)
                || !targets.contains_key(&candidate.auth_id)
            {
                return Err(CodexAccountPoolError::Configuration);
            }
        }
        Ok(Self {
            router,
            conductor,
            candidates,
            executors,
            targets,
            clock,
        })
    }

    pub async fn execute_configured(
        &self,
        model: &str,
        body: Vec<u8>,
        responses_lite: bool,
    ) -> Result<CodexPooledExecutionOutcome, CodexAccountPoolError> {
        let mut remaining = crate::internal::api::account_selection::candidates(&self.candidates);
        let mut attempted_auth_ids = Vec::new();
        let mut last_error = None;

        while !remaining.is_empty() {
            let selected = self
                .router
                .select("codex", Some(model), self.clock.now_ms(), &remaining)
                .map_err(CodexAccountPoolError::Routing)?;
            crate::internal::api::account_selection::record_selected(&selected.auth_id);
            remaining.retain(|candidate| candidate.auth_id != selected.auth_id);
            attempted_auth_ids.push(selected.auth_id.clone());
            let executor = self
                .executors
                .get(&selected.auth_id)
                .ok_or(CodexAccountPoolError::Configuration)?;
            let target = self
                .targets
                .get(&selected.auth_id)
                .ok_or(CodexAccountPoolError::Configuration)?;

            match executor.execute(target, model, &body, responses_lite).await {
                Ok(outcome) => {
                    crate::internal::api::account_selection::record_upstream_status(200, &[]);
                    self.record(&selected.auth_id, model, 200, None).await?;
                    return Ok(CodexPooledExecutionOutcome {
                        selected_auth_id: selected.auth_id,
                        attempted_auth_ids,
                        outcome,
                    });
                }
                Err(error) => {
                    let Some((status, retry_delay_ms)) = error.account_status() else {
                        return Err(CodexAccountPoolError::Execution(error));
                    };
                    self.record(&selected.auth_id, model, status, retry_delay_ms)
                        .await?;
                    if matches!(status, 400 | 422) {
                        return Err(CodexAccountPoolError::Execution(error));
                    }
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.map_or(
            CodexAccountPoolError::Configuration,
            CodexAccountPoolError::Execution,
        ))
    }

    pub async fn execute_stream_configured(
        &self,
        model: &str,
        body: Vec<u8>,
        responses_lite: bool,
    ) -> Result<CodexPooledStreamExecutionOutcome, CodexAccountPoolError> {
        let mut remaining = crate::internal::api::account_selection::candidates(&self.candidates);
        let mut attempted_auth_ids = Vec::new();
        let mut last_error = None;

        while !remaining.is_empty() {
            let selected = self
                .router
                .select("codex", Some(model), self.clock.now_ms(), &remaining)
                .map_err(CodexAccountPoolError::Routing)?;
            crate::internal::api::account_selection::record_selected(&selected.auth_id);
            remaining.retain(|candidate| candidate.auth_id != selected.auth_id);
            attempted_auth_ids.push(selected.auth_id.clone());
            let executor = self
                .executors
                .get(&selected.auth_id)
                .ok_or(CodexAccountPoolError::Configuration)?;
            let target = self
                .targets
                .get(&selected.auth_id)
                .ok_or(CodexAccountPoolError::Configuration)?;
            match executor
                .execute_stream(target, model, &body, responses_lite)
                .await
            {
                Ok(outcome) => {
                    let status = outcome.response().status();
                    crate::internal::api::account_selection::record_upstream_status(status, &[]);
                    let retry_delay_ms =
                        parse_retry_after_delay_ms(outcome.response().retry_after());
                    self.record(&selected.auth_id, model, status, retry_delay_ms)
                        .await?;
                    if (200..300).contains(&status) {
                        let tracked = outcome.into_tracked(
                            selected.auth_id.clone(),
                            model.to_owned(),
                            Arc::clone(&self.conductor),
                            Arc::clone(&self.clock),
                        );
                        return Ok(CodexPooledStreamExecutionOutcome {
                            selected_auth_id: selected.auth_id,
                            attempted_auth_ids,
                            response: tracked,
                        });
                    }
                    last_error = Some(CodexExecutionError::Http {
                        status,
                        retry_delay_ms,
                    });
                    if matches!(status, 400 | 422) {
                        break;
                    }
                }
                Err(error) => {
                    let Some((status, retry_delay_ms)) = error.account_status() else {
                        return Err(CodexAccountPoolError::Execution(error));
                    };
                    self.record(&selected.auth_id, model, status, retry_delay_ms)
                        .await?;
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.map_or(
            CodexAccountPoolError::Configuration,
            CodexAccountPoolError::Execution,
        ))
    }

    async fn record(
        &self,
        auth_id: &str,
        model: &str,
        status: u16,
        retry_delay_ms: Option<u64>,
    ) -> Result<(), CodexAccountPoolError> {
        let conductor = Arc::clone(&self.conductor);
        let result = AccountExecutionResult {
            provider: "codex".to_owned(),
            auth_id: auth_id.to_owned(),
            model: Some(model.to_owned()),
            status,
            retry_delay_ms,
            observed_at_ms: self.clock.now_ms(),
        };
        tokio::task::spawn_blocking(move || conductor.record(result))
            .await
            .map_err(|_| CodexAccountPoolError::OutcomePersistence)?
            .map_err(|_| CodexAccountPoolError::OutcomePersistence)?;
        Ok(())
    }
}

impl fmt::Debug for CodexSubscriptionAccountPool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexSubscriptionAccountPool")
            .field("router", &self.router)
            .field("candidate_count", &self.candidates.len())
            .field("executors", &"[REDACTED]")
            .field("targets", &self.targets)
            .finish()
    }
}

pub struct CodexPooledExecutionOutcome {
    selected_auth_id: String,
    attempted_auth_ids: Vec<String>,
    outcome: CodexExecutionOutcome,
}

pub struct CodexPooledStreamExecutionOutcome {
    selected_auth_id: String,
    attempted_auth_ids: Vec<String>,
    response: CodexTrackedResponsesStreamResponse,
}

impl CodexPooledStreamExecutionOutcome {
    pub fn selected_auth_id(&self) -> &str {
        &self.selected_auth_id
    }

    pub fn attempted_auth_ids(&self) -> &[String] {
        &self.attempted_auth_ids
    }

    pub fn into_response(self) -> CodexTrackedResponsesStreamResponse {
        self.response
    }
}

impl fmt::Debug for CodexPooledStreamExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexPooledStreamExecutionOutcome")
            .field("selected_auth_id", &self.selected_auth_id)
            .field("attempted_auth_ids", &self.attempted_auth_ids)
            .field("response", &self.response)
            .finish()
    }
}

pub struct CodexTrackedResponsesStreamResponse {
    response: CodexResponsesStreamResponse,
    auth_id: String,
    model: String,
    conductor: Arc<CooldownConductor>,
    clock: Arc<dyn AccountStateClock>,
    failure_recorded: bool,
}

impl CodexTrackedResponsesStreamResponse {
    pub async fn next_chunk(&mut self) -> Option<Result<Vec<u8>, CodexResponsesTransportFailure>> {
        let next = self.response.next_chunk().await;
        if matches!(next, Some(Err(_))) {
            self.record_terminal_failure().await;
        }
        next
    }

    pub async fn record_terminal_failure(&mut self) {
        if self.failure_recorded {
            return;
        }
        self.failure_recorded = true;
        let conductor = Arc::clone(&self.conductor);
        let result = AccountExecutionResult {
            provider: "codex".to_owned(),
            auth_id: self.auth_id.clone(),
            model: Some(self.model.clone()),
            status: 502,
            retry_delay_ms: None,
            observed_at_ms: self.clock.now_ms(),
        };
        let _ = tokio::task::spawn_blocking(move || conductor.record(result)).await;
    }
}

impl fmt::Debug for CodexTrackedResponsesStreamResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexTrackedResponsesStreamResponse")
            .field("response", &self.response)
            .field("auth_id", &self.auth_id)
            .field("model", &self.model)
            .field("failure_recorded", &self.failure_recorded)
            .finish()
    }
}

impl CodexPooledExecutionOutcome {
    pub fn selected_auth_id(&self) -> &str {
        &self.selected_auth_id
    }

    pub fn attempted_auth_ids(&self) -> &[String] {
        &self.attempted_auth_ids
    }

    pub fn outcome(&self) -> &CodexExecutionOutcome {
        &self.outcome
    }
}

impl fmt::Debug for CodexPooledExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexPooledExecutionOutcome")
            .field("selected_auth_id", &self.selected_auth_id)
            .field("attempted_auth_ids", &self.attempted_auth_ids)
            .field("outcome", &self.outcome)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexAccountPoolError {
    Configuration,
    Routing(AccountRoutingError),
    Execution(CodexExecutionError),
    OutcomePersistence,
}

impl fmt::Display for CodexAccountPoolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration => formatter.write_str("Codex account pool is invalid"),
            Self::Routing(error) => write!(formatter, "Codex account routing failed: {error}"),
            Self::Execution(error) => write!(formatter, "Codex pooled execution failed: {error}"),
            Self::OutcomePersistence => {
                formatter.write_str("Codex account outcome persistence failed")
            }
        }
    }
}

impl std::error::Error for CodexAccountPoolError {}

fn aggregate_codex_response(
    response: CodexResponsesResponse,
) -> Result<Vec<u8>, CodexExecutionError> {
    let mut decoder = SseDecoder::new();
    let mut events = decoder.push(response.body());
    events.extend(decoder.finish());
    let mut terminal = CodexTerminalAccumulator::default();

    for event in events {
        if event.data == b"[DONE]" {
            continue;
        }
        match terminal.ingest(&event.data, SystemTime::now()) {
            CodexTerminalEvent::Continue => {}
            CodexTerminalEvent::Completed(payload) => return Ok(payload),
            CodexTerminalEvent::Failed(error) => {
                return Err(CodexExecutionError::Terminal {
                    status: error.status,
                    code: error.code,
                })
            }
        }
    }
    Err(CodexExecutionError::IncompleteStream)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::time::SystemTime;

    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    use super::*;
    use crate::internal::auth::codex::{
        CodexCredentialHandles, CodexRefreshCoordinator, CodexRefreshHttpResponse,
        CodexRefreshRequest, CodexRefreshTransport, CodexRefreshTransportFailure,
        CodexSecretHandle, CodexSecretKind, CodexSecretStore, CodexStoredCredentials, RefreshClock,
        SecretStoreError, SecretString,
    };
    use crate::sdk::cliproxy::auth::{CooldownStateRecord, CooldownStateStore, CooldownStoreError};

    #[test]
    fn completion_reconstructs_empty_output_in_index_order() {
        let response = CodexResponsesResponse::new(
            200,
            None,
            concat!(
                "data: {\"type\":\"response.output_item.done\",\"output_index\":1,\"item\":{\"id\":\"second\"}}\n\n",
                "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"id\":\"first\"}}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"output\":[]}}\n\n"
            )
            .as_bytes()
            .to_vec(),
        );
        let payload = aggregate_codex_response(response).unwrap();
        let value: Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(value["output"][0]["id"], "first");
        assert_eq!(value["output"][1]["id"], "second");
    }

    #[test]
    fn terminal_error_keeps_only_status_and_safe_code() {
        let response = CodexResponsesResponse::new(
            200,
            None,
            br#"data: {"type":"error","error":{"type":"invalid_request_error","code":"context_length_exceeded","message":"provider detail do-not-leak"}}

"#
            .to_vec(),
        );
        let error = aggregate_codex_response(response).unwrap_err();
        assert_eq!(
            error,
            CodexExecutionError::Terminal {
                status: 400,
                code: Some("context_length_exceeded".to_owned())
            }
        );
        assert!(!format!("{error:?} {error}").contains("do-not-leak"));
    }

    #[test]
    fn missing_completion_is_explicit_request_scoped_failure() {
        let response = CodexResponsesResponse::new(
            200,
            None,
            b"data: {\"type\":\"response.created\"}\n\n".to_vec(),
        );
        assert_eq!(
            aggregate_codex_response(response),
            Err(CodexExecutionError::IncompleteStream)
        );
    }

    struct MemoryStore(Mutex<CodexStoredCredentials>);

    impl CodexSecretStore for MemoryStore {
        fn load_credentials(
            &self,
            _handles: &CodexCredentialHandles,
        ) -> Result<CodexStoredCredentials, SecretStoreError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn store_credentials(
            &self,
            _handles: &CodexCredentialHandles,
            credentials: &CodexStoredCredentials,
        ) -> Result<(), SecretStoreError> {
            *self.0.lock().unwrap() = credentials.clone();
            Ok(())
        }
    }

    struct FixedClock;

    impl RefreshClock for FixedClock {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(10_000)
        }

        fn sleep(
            &self,
            _duration: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<(), CodexRefreshTransportFailure>> + Send + '_>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    impl AccountStateClock for FixedClock {
        fn now_ms(&self) -> i64 {
            10_000
        }
    }

    struct SuccessfulRefresh(String);

    impl CodexRefreshTransport for SuccessfulRefresh {
        fn execute<'a>(
            &'a self,
            _request: &'a CodexRefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<CodexRefreshHttpResponse, CodexRefreshTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                Ok(CodexRefreshHttpResponse::new(
                    200,
                    format!(
                        r#"{{"id_token":"{}","access_token":"access-new","refresh_token":"refresh-new","expires_in":3600}}"#,
                        self.0
                    )
                    .into_bytes(),
                ))
            })
        }
    }

    struct ReplayTransport {
        requests: Mutex<Vec<(String, String)>>,
    }

    struct FixedResponsesTransport {
        status: u16,
        retry_after: Option<String>,
    }

    struct FixedStreamTransport {
        status: u16,
        chunks: Vec<Result<Vec<u8>, CodexResponsesTransportFailure>>,
    }

    type CapturedDirectImageRequest = (String, String, bool, Vec<u8>);

    #[derive(Default)]
    struct DirectImageTransport {
        captured: Mutex<Option<CapturedDirectImageRequest>>,
    }

    impl CodexResponsesTransport for DirectImageTransport {
        fn execute<'a>(
            &'a self,
            request: &'a CodexResponsesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<CodexResponsesResponse, CodexResponsesTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                *self.captured.lock().unwrap() = Some((
                    request.url().to_owned(),
                    request.content_type().to_owned(),
                    request.stream(),
                    request.body().to_vec(),
                ));
                Ok(CodexResponsesResponse::new(
                    200,
                    None,
                    br#"{"created":1,"data":[{"b64_json":"cG5n"}]}"#.to_vec(),
                ))
            })
        }
    }

    impl CodexResponsesStreamingTransport for FixedStreamTransport {
        fn execute_stream<'a>(
            &'a self,
            _request: &'a CodexResponsesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            CodexResponsesStreamResponse,
                            CodexResponsesTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                let (sender, receiver) = tokio::sync::mpsc::channel(8);
                let chunks = self.chunks.clone();
                tokio::spawn(async move {
                    for chunk in chunks {
                        if sender.send(chunk).await.is_err() {
                            break;
                        }
                    }
                });
                Ok(CodexResponsesStreamResponse::new(
                    self.status,
                    None,
                    receiver,
                ))
            })
        }
    }

    impl CodexResponsesTransport for FixedResponsesTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a CodexResponsesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<CodexResponsesResponse, CodexResponsesTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                let body = if (200..300).contains(&self.status) {
                    b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_pool\",\"status\":\"completed\",\"output\":[]}}\n\n".to_vec()
                } else {
                    b"provider failure body do-not-leak".to_vec()
                };
                Ok(CodexResponsesResponse::new(
                    self.status,
                    self.retry_after.clone(),
                    body,
                ))
            })
        }
    }

    #[derive(Default)]
    struct CooldownMemoryStore(Mutex<Vec<CooldownStateRecord>>);

    impl CooldownStateStore for CooldownMemoryStore {
        fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn save(&self, records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
            *self.0.lock().unwrap() = records.to_vec();
            Ok(())
        }
    }

    impl CodexResponsesTransport for ReplayTransport {
        fn execute<'a>(
            &'a self,
            request: &'a CodexResponsesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<CodexResponsesResponse, CodexResponsesTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                let mut requests = self.requests.lock().unwrap();
                requests.push((
                    request.access_token().expose_secret().to_owned(),
                    request.account_id().to_owned(),
                ));
                if requests.len() == 1 {
                    Ok(CodexResponsesResponse::new(
                        401,
                        None,
                        b"provider body do-not-leak".to_vec(),
                    ))
                } else {
                    Ok(CodexResponsesResponse::new(
                        200,
                        None,
                        b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"output\":[]}}\n\n".to_vec(),
                    ))
                }
            })
        }
    }

    fn jwt(account_id: &str) -> String {
        let payload = serde_json::json!({
            "https://api.openai.com/auth": {"chatgpt_account_id": account_id}
        });
        format!(
            "header.{}.signature",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap())
        )
    }

    fn codex_handles() -> CodexCredentialHandles {
        let handle = |name, kind| CodexSecretHandle::new("subscriptions", name, kind).unwrap();
        CodexCredentialHandles::new(
            handle("id", CodexSecretKind::IdToken),
            handle("access", CodexSecretKind::AccessToken),
            handle("refresh", CodexSecretKind::RefreshToken),
        )
        .unwrap()
    }

    fn account_executor(
        auth_id: &str,
        transport: Arc<dyn CodexResponsesTransport>,
    ) -> Arc<CodexSubscriptionResponsesExecutor> {
        let store = Arc::new(MemoryStore(Mutex::new(CodexStoredCredentials::new(
            SecretString::new(jwt(auth_id)).unwrap(),
            SecretString::new(format!("access-{auth_id}")).unwrap(),
            SecretString::new(format!("refresh-{auth_id}")).unwrap(),
        ))));
        let auth = Arc::new(CodexSubscriptionAuth::new(
            codex_handles(),
            store,
            Arc::new(SuccessfulRefresh(jwt(auth_id))),
            Arc::new(FixedClock),
            Arc::new(CodexRefreshCoordinator::default()),
        ));
        Arc::new(
            CodexSubscriptionResponsesExecutor::new(auth, transport, Duration::from_secs(5))
                .unwrap(),
        )
    }

    #[test]
    fn token_count_is_local_and_uses_responses_usage_shape() {
        let executor = account_executor(
            "account-token-count",
            Arc::new(FixedResponsesTransport {
                status: 500,
                retry_after: None,
            }),
        );
        let outcome = executor
            .count_tokens(
                "gpt-5.4",
                br#"{"instructions":"be concise","input":[{"type":"message","content":[{"text":"hello"}]}]}"#,
            )
            .unwrap();
        let value: Value = serde_json::from_slice(outcome.payload()).unwrap();
        let count = value["response"]["usage"]["input_tokens"].as_i64().unwrap();
        assert!(count > 0);
        assert_eq!(outcome.attempts(), 0);
        assert_eq!(value["response"]["usage"]["output_tokens"], 0);
        assert_eq!(value["response"]["usage"]["total_tokens"], count);
    }

    fn account_stream_executor(
        auth_id: &str,
        stream: Arc<dyn CodexResponsesStreamingTransport>,
    ) -> Arc<CodexSubscriptionResponsesExecutor> {
        let store = Arc::new(MemoryStore(Mutex::new(CodexStoredCredentials::new(
            SecretString::new(jwt(auth_id)).unwrap(),
            SecretString::new(format!("access-{auth_id}")).unwrap(),
            SecretString::new(format!("refresh-{auth_id}")).unwrap(),
        ))));
        let auth = Arc::new(CodexSubscriptionAuth::new(
            codex_handles(),
            store,
            Arc::new(SuccessfulRefresh(jwt(auth_id))),
            Arc::new(FixedClock),
            Arc::new(CodexRefreshCoordinator::default()),
        ));
        Arc::new(
            CodexSubscriptionResponsesExecutor::new(
                auth,
                Arc::new(FixedResponsesTransport {
                    status: 500,
                    retry_after: None,
                }),
                Duration::from_secs(5),
            )
            .unwrap()
            .with_stream_transport(stream),
        )
    }

    #[tokio::test]
    async fn unauthorized_refreshes_rebuilds_account_headers_and_replays_once() {
        let store = Arc::new(MemoryStore(Mutex::new(CodexStoredCredentials::new(
            SecretString::new(jwt("acct-old")).unwrap(),
            SecretString::new("access-old").unwrap(),
            SecretString::new("refresh-old").unwrap(),
        ))));
        let auth = Arc::new(CodexSubscriptionAuth::new(
            codex_handles(),
            store.clone(),
            Arc::new(SuccessfulRefresh(jwt("acct-new"))),
            Arc::new(FixedClock),
            Arc::new(CodexRefreshCoordinator::default()),
        ));
        let transport = Arc::new(ReplayTransport {
            requests: Mutex::new(Vec::new()),
        });
        let executor = CodexSubscriptionResponsesExecutor::new(
            auth,
            transport.clone(),
            Duration::from_secs(5),
        )
        .unwrap()
        .with_plan_type("free");
        let outcome = executor
            .execute(
                &CodexUpstreamTarget::new("https://chatgpt.example/backend-api/codex").unwrap(),
                "gpt-5.5",
                br#"{"input":"hello","prompt_cache_key":"session-7"}"#,
                false,
            )
            .await
            .unwrap();
        assert_eq!(outcome.attempts(), 2);
        assert_eq!(
            *transport.requests.lock().unwrap(),
            vec![
                ("access-old".to_owned(), "acct-old".to_owned()),
                ("access-new".to_owned(), "acct-new".to_owned())
            ]
        );
        let persisted = store.0.lock().unwrap().clone();
        assert_eq!(persisted.refresh_token().expose_secret(), "refresh-new");
        assert!(!format!("{outcome:?}").contains("access-new"));
    }

    #[tokio::test]
    async fn direct_image_executor_uses_direct_endpoint_and_json_wire_shape() {
        let transport = Arc::new(DirectImageTransport::default());
        let executor = account_executor("account-image", transport.clone());
        let outcome = executor
            .execute_direct_image(
                &CodexUpstreamTarget::new("https://chatgpt.example/backend-api/codex").unwrap(),
                "gpt-image-2",
                "/v1/images/generations",
                Some("application/json"),
                br#"{"prompt":"draw","model":"gpt-image-2"}"#,
            )
            .await
            .unwrap();
        assert_eq!(
            outcome.payload(),
            br#"{"created":1,"data":[{"b64_json":"cG5n"}]}"#
        );
        let captured = transport.captured.lock().unwrap().clone().unwrap();
        assert_eq!(
            captured.0,
            "https://chatgpt.example/backend-api/codex/images/generations"
        );
        assert_eq!(captured.1, "application/json");
        assert!(!captured.2);
        let body: Value = serde_json::from_slice(&captured.3).unwrap();
        assert_eq!(body["model"], "gpt-image-2");
        assert_eq!(body["stream"], false);
    }

    #[tokio::test]
    async fn direct_image_stream_accepts_provider_native_terminal_shape() {
        let native_chunk = br#"data: {"b64_json":"cG5n"}\n\n"#.to_vec();
        let executor = account_stream_executor(
            "account-image-stream",
            Arc::new(FixedStreamTransport {
                status: 200,
                chunks: vec![Ok(native_chunk.clone())],
            }),
        );
        let outcome = executor
            .execute_direct_image_stream(
                &CodexUpstreamTarget::new("https://chatgpt.example/backend-api/codex").unwrap(),
                "gpt-image-2",
                "/v1/images/generations",
                Some("application/json"),
                br#"{"prompt":"draw","model":"gpt-image-2","stream":true}"#,
            )
            .await
            .unwrap();
        assert_eq!(outcome.attempts(), 1);
        let mut response = outcome.response;
        assert_eq!(response.next_chunk().await.unwrap().unwrap(), native_chunk);
        assert!(response.next_chunk().await.is_none());
    }

    #[tokio::test]
    async fn account_pool_persists_retry_after_before_failing_over() {
        let state_store = Arc::new(CooldownMemoryStore::default());
        let router = Arc::new(AccountRouter::new(state_store.clone()));
        let conductor = Arc::new(CooldownConductor::new(state_store.clone()));
        let candidate = |auth_id: &str| AccountCandidate {
            auth_id: auth_id.to_owned(),
            provider: "codex".to_owned(),
            priority: 0,
            weight: 1,
            websocket_enabled: false,
            supported_models: Vec::new(),
            disabled: false,
        };
        let executors = HashMap::from([
            (
                "account-a".to_owned(),
                account_executor(
                    "account-a",
                    Arc::new(FixedResponsesTransport {
                        status: 429,
                        retry_after: Some("7".to_owned()),
                    }),
                ),
            ),
            (
                "account-b".to_owned(),
                account_executor(
                    "account-b",
                    Arc::new(FixedResponsesTransport {
                        status: 200,
                        retry_after: None,
                    }),
                ),
            ),
        ]);
        let target = CodexUpstreamTarget::new("https://chatgpt.example/backend-api/codex").unwrap();
        let targets = HashMap::from([
            ("account-a".to_owned(), target.clone()),
            ("account-b".to_owned(), target),
        ]);
        let pool = CodexSubscriptionAccountPool::with_clock(
            router,
            conductor,
            vec![candidate("account-a"), candidate("account-b")],
            executors,
            targets,
            Arc::new(FixedClock),
        )
        .unwrap();

        let outcome = pool
            .execute_configured("gpt-5.5", br#"{"input":"hello"}"#.to_vec(), false)
            .await
            .unwrap();

        assert_eq!(outcome.selected_auth_id(), "account-b");
        assert_eq!(outcome.attempted_auth_ids(), ["account-a", "account-b"]);
        let records = state_store.0.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].provider, "codex");
        assert_eq!(records[0].auth_id, "account-a");
        assert_eq!(records[0].next_retry_after_ms, Some(17_000));
        assert!(!format!("{outcome:?}").contains("access-account-b"));
    }

    #[tokio::test]
    async fn stream_pool_fails_over_before_first_event_and_cools_post_commit_failure() {
        let state_store = Arc::new(CooldownMemoryStore::default());
        let router = Arc::new(AccountRouter::new(state_store.clone()));
        let conductor = Arc::new(CooldownConductor::new(state_store.clone()));
        let candidate = |auth_id: &str| AccountCandidate {
            auth_id: auth_id.to_owned(),
            provider: "codex".to_owned(),
            priority: 0,
            weight: 1,
            websocket_enabled: false,
            supported_models: Vec::new(),
            disabled: false,
        };
        let executors = HashMap::from([
            (
                "account-a".to_owned(),
                account_stream_executor(
                    "account-a",
                    Arc::new(FixedStreamTransport {
                        status: 200,
                        chunks: vec![Err(CodexResponsesTransportFailure::Protocol)],
                    }),
                ),
            ),
            (
                "account-b".to_owned(),
                account_stream_executor(
                    "account-b",
                    Arc::new(FixedStreamTransport {
                        status: 200,
                        chunks: vec![
                            Ok(b"data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_stream\"}}\n\n".to_vec()),
                            Err(CodexResponsesTransportFailure::Protocol),
                        ],
                    }),
                ),
            ),
        ]);
        let target = CodexUpstreamTarget::new("https://chatgpt.example/backend-api/codex").unwrap();
        let targets = HashMap::from([
            ("account-a".to_owned(), target.clone()),
            ("account-b".to_owned(), target),
        ]);
        let pool = CodexSubscriptionAccountPool::with_clock(
            router,
            conductor,
            vec![candidate("account-a"), candidate("account-b")],
            executors,
            targets,
            Arc::new(FixedClock),
        )
        .unwrap();

        let outcome = pool
            .execute_stream_configured("gpt-5.5", br#"{"input":"hello"}"#.to_vec(), false)
            .await
            .unwrap();
        assert_eq!(outcome.selected_auth_id(), "account-b");
        assert_eq!(outcome.attempted_auth_ids(), ["account-a", "account-b"]);
        let mut response = outcome.into_response();
        assert!(response.next_chunk().await.unwrap().is_ok());
        assert_eq!(
            response.next_chunk().await,
            Some(Err(CodexResponsesTransportFailure::Protocol))
        );
        let records = state_store.0.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|record| record.auth_id == "account-a"));
        assert!(records.iter().any(|record| record.auth_id == "account-b"));
    }
}
