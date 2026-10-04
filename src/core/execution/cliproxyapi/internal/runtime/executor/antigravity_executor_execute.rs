// ref: internal/runtime/executor/antigravity_executor_execute.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use crate::internal::cache::antigravity_reasoning_replay_cache::AntigravityReasoningReplayCache;
use crate::internal::cache::SignatureKvStore;

use super::antigravity_executor::{
    AntigravityGenerateRequest, AntigravityGenerateStreamingTransport,
    AntigravityGenerateTransport, AntigravityGenerateTransportFailure, AntigravityResponsesStream,
    AntigravityUpstreamTarget,
};
use super::antigravity_executor_auth::{
    AntigravitySubscriptionAuth, AntigravitySubscriptionAuthError,
};
use super::antigravity_executor_credits::{
    decide_antigravity_429, inject_enabled_credit_types, Antigravity429DecisionKind,
    AntigravityCreditsController,
};
use super::antigravity_executor_request::{
    prepare_antigravity_generate_body, AntigravityRequestError,
};
use super::antigravity_reasoning_replay::{
    antigravity_replay_session_key, prepare_antigravity_reasoning_replay, replay_now_ms,
    AntigravityReasoningReplayAccumulator,
};
use crate::internal::translator::antigravity::claude::{
    claude_request_uses_native_web_search, convert_antigravity_response_to_claude_non_stream,
    convert_claude_request_to_antigravity_with_runtime, AntigravityClaudeRequestCapabilities,
    AntigravityClaudeRequestTranslationError,
};
use crate::internal::translator::antigravity::openai::responses::convert_antigravity_response_to_openai_responses_non_stream;

fn prepare_antigravity_upstream_body(
    translated_body: &[u8],
    client_headers: &BTreeMap<String, Vec<String>>,
    model: &str,
    project_id: &str,
) -> Result<Vec<u8>, AntigravityRequestError> {
    let normalized = super::helps::normalize_codex_tool_integer_types_for_executor(
        translated_body,
        client_headers,
        "antigravity",
    );
    prepare_antigravity_generate_body(&normalized, model, project_id)
}
use crate::sdk::cliproxy::auth::{
    AccountCandidate, AccountExecutionResult, AccountRouter, AccountRoutingError, CooldownConductor,
};

use super::claude_executor_execute::AccountStateClock;

pub struct AntigravitySubscriptionExecutor {
    auth: Arc<AntigravitySubscriptionAuth>,
    transport: Arc<dyn AntigravityGenerateTransport>,
    stream_transport: Option<Arc<dyn AntigravityGenerateStreamingTransport>>,
    replay_cache: Option<Arc<AntigravityReasoningReplayCache>>,
    credits: Option<(Arc<AntigravityCreditsController>, String)>,
    timeout: Duration,
    fingerprint_sink: Option<Arc<dyn AntigravityAccessTokenFingerprintSink>>,
}

pub trait AntigravityAccessTokenFingerprintSink: Send + Sync {
    fn update_access_token_fingerprint(&self, sha256: &str);
}

impl AntigravityAccessTokenFingerprintSink for super::helps::UsageReporter {
    fn update_access_token_fingerprint(&self, sha256: &str) {
        let _ = self.update_access_token_fingerprint_sha256(sha256);
    }
}

impl AntigravitySubscriptionExecutor {
    pub fn new(
        auth: Arc<AntigravitySubscriptionAuth>,
        transport: Arc<dyn AntigravityGenerateTransport>,
        timeout: Duration,
    ) -> Result<Self, AntigravityExecutionError> {
        if timeout.is_zero() {
            return Err(AntigravityExecutionError::InvalidTimeout);
        }
        Ok(Self {
            auth,
            transport,
            stream_transport: None,
            replay_cache: None,
            credits: None,
            timeout,
            fingerprint_sink: None,
        })
    }

    pub fn with_access_token_fingerprint_sink(
        mut self,
        sink: Arc<dyn AntigravityAccessTokenFingerprintSink>,
    ) -> Self {
        self.fingerprint_sink = Some(sink);
        self
    }

    fn publish_access_token_fingerprint(
        &self,
        credentials: &crate::internal::auth::antigravity::AntigravityStoredCredentials,
    ) {
        let Some(sink) = self.fingerprint_sink.as_ref() else {
            return;
        };
        let digest = Sha256::digest(credentials.access_token().expose_secret().as_bytes());
        sink.update_access_token_fingerprint(&format!("{digest:x}"));
    }

    pub fn with_stream_transport(
        mut self,
        transport: Arc<dyn AntigravityGenerateStreamingTransport>,
    ) -> Self {
        self.stream_transport = Some(transport);
        self
    }

    pub fn with_reasoning_replay_cache(
        mut self,
        cache: Arc<AntigravityReasoningReplayCache>,
    ) -> Self {
        self.replay_cache = Some(cache);
        self
    }

    pub fn with_credits(
        mut self,
        controller: Arc<AntigravityCreditsController>,
        auth_id: impl Into<String>,
    ) -> Self {
        self.credits = Some((controller, auth_id.into()));
        self
    }

    fn prepare_replay(
        &self,
        model: &str,
        original_request: &[u8],
        body: Vec<u8>,
    ) -> (Vec<u8>, Option<AntigravityReasoningReplayAccumulator>) {
        let Some(cache) = self.replay_cache.as_ref() else {
            return (body, None);
        };
        let Some(session_key) = antigravity_replay_session_key(original_request, &body) else {
            return (body, None);
        };
        prepare_antigravity_reasoning_replay(
            Arc::clone(cache),
            model,
            &session_key,
            &body,
            replay_now_ms(),
        )
        .map_or((body, None), |(body, accumulator)| {
            (body, Some(accumulator))
        })
    }

    pub async fn execute(
        &self,
        target: &AntigravityUpstreamTarget,
        model: &str,
        original_request: &[u8],
        translated_body: &[u8],
    ) -> Result<AntigravityExecutionOutcome, AntigravityExecutionError> {
        self.execute_with_client_headers(
            target,
            model,
            original_request,
            translated_body,
            &BTreeMap::new(),
        )
        .await
    }

    pub async fn execute_with_client_headers(
        &self,
        target: &AntigravityUpstreamTarget,
        model: &str,
        original_request: &[u8],
        translated_body: &[u8],
        client_headers: &BTreeMap<String, Vec<String>>,
    ) -> Result<AntigravityExecutionOutcome, AntigravityExecutionError> {
        let raw = self
            .execute_buffered_raw(
                target,
                model,
                original_request,
                translated_body,
                false,
                client_headers,
            )
            .await?;
        let payload = convert_antigravity_response_to_openai_responses_non_stream(
            original_request,
            &raw.translated_body,
            &raw.response_body,
        );
        if serde_json::from_slice::<serde_json::Value>(&payload)
            .ok()
            .and_then(|value| value.get("id").cloned())
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .is_none()
        {
            return Err(AntigravityExecutionError::InvalidResponse);
        }
        Ok(AntigravityExecutionOutcome {
            payload,
            attempts: raw.attempts,
        })
    }

    pub async fn execute_with_credits(
        &self,
        target: &AntigravityUpstreamTarget,
        model: &str,
        original_request: &[u8],
        translated_body: &[u8],
    ) -> Result<AntigravityExecutionOutcome, AntigravityExecutionError> {
        let raw = self
            .execute_buffered_raw(
                target,
                model,
                original_request,
                translated_body,
                true,
                &BTreeMap::new(),
            )
            .await?;
        let payload = convert_antigravity_response_to_openai_responses_non_stream(
            original_request,
            &raw.translated_body,
            &raw.response_body,
        );
        Ok(AntigravityExecutionOutcome {
            payload,
            attempts: raw.attempts,
        })
    }

    async fn execute_claude_non_stream(
        &self,
        target: &AntigravityUpstreamTarget,
        model: &str,
        original_request: &[u8],
        translated_body: &[u8],
        web_search_tool_use_id: &str,
        client_headers: &BTreeMap<String, Vec<String>>,
    ) -> Result<AntigravityExecutionOutcome, AntigravityExecutionError> {
        let raw = self
            .execute_buffered_raw(
                target,
                model,
                original_request,
                translated_body,
                false,
                client_headers,
            )
            .await?;
        let payload = convert_antigravity_response_to_claude_non_stream(
            original_request,
            &raw.translated_body,
            &raw.response_body,
            web_search_tool_use_id,
        );
        let valid = serde_json::from_slice::<serde_json::Value>(&payload)
            .ok()
            .is_some_and(|value| {
                value.get("type").and_then(serde_json::Value::as_str) == Some("message")
                    && value
                        .get("content")
                        .is_some_and(serde_json::Value::is_array)
            });
        if !valid {
            return Err(AntigravityExecutionError::InvalidResponse);
        }
        Ok(AntigravityExecutionOutcome {
            payload,
            attempts: raw.attempts,
        })
    }

    async fn execute_buffered_raw(
        &self,
        target: &AntigravityUpstreamTarget,
        model: &str,
        original_request: &[u8],
        translated_body: &[u8],
        credits_requested: bool,
        client_headers: &BTreeMap<String, Vec<String>>,
    ) -> Result<AntigravityBufferedRawOutcome, AntigravityExecutionError> {
        let mut credentials = self
            .auth
            .load()
            .await
            .map_err(AntigravityExecutionError::Auth)?;
        self.publish_access_token_fingerprint(&credentials);
        for attempt in 1..=2 {
            let mut body = prepare_antigravity_upstream_body(
                translated_body,
                client_headers,
                model,
                credentials.project_id(),
            )
            .map_err(AntigravityExecutionError::Request)?;
            if credits_requested {
                let (controller, auth_id) = self
                    .credits
                    .as_ref()
                    .ok_or(AntigravityExecutionError::CreditsUnavailable)?;
                if controller
                    .has_credits(auth_id)
                    .map_err(|_| AntigravityExecutionError::CreditsStore)?
                {
                    body = inject_enabled_credit_types(&body).ok_or(
                        AntigravityExecutionError::Request(AntigravityRequestError::InvalidJson),
                    )?;
                }
            }
            let (body, mut replay) = self.prepare_replay(model, original_request, body);
            let request = AntigravityGenerateRequest::new(
                target,
                credentials.access_token().clone(),
                body.clone(),
            );
            let response = self
                .transport
                .execute(&request, self.timeout)
                .await
                .map_err(AntigravityExecutionError::Transport)?;
            if response.status() == 401 && attempt == 1 {
                credentials = self
                    .auth
                    .refresh_after_status(401)
                    .await
                    .map_err(AntigravityExecutionError::Auth)?
                    .credentials()
                    .clone();
                self.publish_access_token_fingerprint(&credentials);
                continue;
            }
            if !(200..300).contains(&response.status()) {
                if let Some(accumulator) = replay.as_ref() {
                    accumulator
                        .clear_on_invalid_signature(
                            response.status(),
                            response.body(),
                            replay_now_ms(),
                        )
                        .map_err(|_| AntigravityExecutionError::ReplayCache)?;
                }
                if response.status() == 429 {
                    let decision = decide_antigravity_429(response.body());
                    if decision.kind == Antigravity429DecisionKind::ShortCooldownSwitchAuth {
                        if let (Some((controller, auth_id)), Some(delay)) =
                            (&self.credits, decision.retry_after)
                        {
                            controller
                                .mark_short_cooldown(auth_id, model, replay_now_ms(), delay)
                                .map_err(|_| AntigravityExecutionError::CreditsStore)?;
                        }
                    }
                }
                return Err(AntigravityExecutionError::Http {
                    status: response.status(),
                    retry_after: response.retry_after().map(ToOwned::to_owned),
                });
            }
            if let Some(accumulator) = replay.as_mut() {
                accumulator.observe_response_payload(response.body());
            }
            if let Some(accumulator) = replay {
                let _ = accumulator.commit(replay_now_ms());
            }
            return Ok(AntigravityBufferedRawOutcome {
                response_body: response.body().to_vec(),
                translated_body: body,
                attempts: attempt,
            });
        }
        Err(AntigravityExecutionError::ReplayExhausted)
    }

    pub async fn execute_stream(
        &self,
        target: &AntigravityUpstreamTarget,
        model: &str,
        original_request: &[u8],
        translated_body: &[u8],
    ) -> Result<AntigravityStreamExecutionOutcome, AntigravityExecutionError> {
        self.execute_stream_with_client_headers(
            target,
            model,
            original_request,
            translated_body,
            &BTreeMap::new(),
        )
        .await
    }

    pub async fn execute_stream_with_client_headers(
        &self,
        target: &AntigravityUpstreamTarget,
        model: &str,
        original_request: &[u8],
        translated_body: &[u8],
        client_headers: &BTreeMap<String, Vec<String>>,
    ) -> Result<AntigravityStreamExecutionOutcome, AntigravityExecutionError> {
        self.execute_stream_mode(
            target,
            model,
            original_request,
            translated_body,
            false,
            client_headers,
        )
        .await
    }

    pub async fn execute_stream_with_credits(
        &self,
        target: &AntigravityUpstreamTarget,
        model: &str,
        original_request: &[u8],
        translated_body: &[u8],
    ) -> Result<AntigravityStreamExecutionOutcome, AntigravityExecutionError> {
        self.execute_stream_mode(
            target,
            model,
            original_request,
            translated_body,
            true,
            &BTreeMap::new(),
        )
        .await
    }

    async fn execute_stream_mode(
        &self,
        target: &AntigravityUpstreamTarget,
        model: &str,
        original_request: &[u8],
        translated_body: &[u8],
        credits_requested: bool,
        client_headers: &BTreeMap<String, Vec<String>>,
    ) -> Result<AntigravityStreamExecutionOutcome, AntigravityExecutionError> {
        let opened = self
            .open_stream(
                target,
                model,
                original_request,
                translated_body,
                credits_requested,
                client_headers,
            )
            .await?;
        let mut stream = AntigravityResponsesStream::new(
            opened.response,
            original_request.to_vec(),
            opened.translated_body,
        );
        if let Some(replay) = opened.replay {
            stream = stream.with_replay_accumulator(replay);
        }
        stream
            .bootstrap()
            .await
            .map_err(AntigravityExecutionError::Transport)?;
        Ok(AntigravityStreamExecutionOutcome {
            stream,
            attempts: opened.attempts,
        })
    }

    async fn execute_claude_stream(
        &self,
        target: &AntigravityUpstreamTarget,
        model: &str,
        original_request: &[u8],
        translated_body: &[u8],
        web_search_tool_use_id: String,
        signature_store: Option<Arc<dyn SignatureKvStore>>,
        client_headers: &BTreeMap<String, Vec<String>>,
    ) -> Result<AntigravityStreamExecutionOutcome, AntigravityExecutionError> {
        let opened = self
            .open_stream(
                target,
                model,
                original_request,
                translated_body,
                false,
                client_headers,
            )
            .await?;
        let mut stream = AntigravityResponsesStream::new_claude(
            opened.response,
            original_request.to_vec(),
            opened.translated_body,
            web_search_tool_use_id,
            signature_store,
        );
        if let Some(replay) = opened.replay {
            stream = stream.with_replay_accumulator(replay);
        }
        stream
            .bootstrap()
            .await
            .map_err(AntigravityExecutionError::Transport)?;
        Ok(AntigravityStreamExecutionOutcome {
            stream,
            attempts: opened.attempts,
        })
    }

    async fn open_stream(
        &self,
        target: &AntigravityUpstreamTarget,
        model: &str,
        original_request: &[u8],
        translated_body: &[u8],
        credits_requested: bool,
        client_headers: &BTreeMap<String, Vec<String>>,
    ) -> Result<AntigravityOpenedStream, AntigravityExecutionError> {
        let transport = self
            .stream_transport
            .as_ref()
            .ok_or(AntigravityExecutionError::StreamingUnavailable)?;
        let mut credentials = self
            .auth
            .load()
            .await
            .map_err(AntigravityExecutionError::Auth)?;
        self.publish_access_token_fingerprint(&credentials);
        for attempt in 1..=2 {
            let mut body = prepare_antigravity_upstream_body(
                translated_body,
                client_headers,
                model,
                credentials.project_id(),
            )
            .map_err(AntigravityExecutionError::Request)?;
            if credits_requested {
                let (controller, auth_id) = self
                    .credits
                    .as_ref()
                    .ok_or(AntigravityExecutionError::CreditsUnavailable)?;
                if controller
                    .has_credits(auth_id)
                    .map_err(|_| AntigravityExecutionError::CreditsStore)?
                {
                    body = inject_enabled_credit_types(&body).ok_or(
                        AntigravityExecutionError::Request(AntigravityRequestError::InvalidJson),
                    )?;
                }
            }
            let (body, replay) = self.prepare_replay(model, original_request, body);
            let request = AntigravityGenerateRequest::new_stream(
                target,
                credentials.access_token().clone(),
                body.clone(),
            );
            let response = transport
                .execute_stream(&request, self.timeout)
                .await
                .map_err(AntigravityExecutionError::Transport)?;
            if response.status() == 401 && attempt == 1 {
                credentials = self
                    .auth
                    .refresh_after_status(401)
                    .await
                    .map_err(AntigravityExecutionError::Auth)?
                    .credentials()
                    .clone();
                self.publish_access_token_fingerprint(&credentials);
                continue;
            }
            if !(200..300).contains(&response.status()) {
                return Err(AntigravityExecutionError::Http {
                    status: response.status(),
                    retry_after: response.retry_after().map(ToOwned::to_owned),
                });
            }
            return Ok(AntigravityOpenedStream {
                response,
                translated_body: body,
                replay,
                attempts: attempt,
            });
        }
        Err(AntigravityExecutionError::ReplayExhausted)
    }
}

struct AntigravityBufferedRawOutcome {
    response_body: Vec<u8>,
    translated_body: Vec<u8>,
    attempts: usize,
}

struct AntigravityOpenedStream {
    response: super::antigravity_executor::AntigravityGenerateStreamResponse,
    translated_body: Vec<u8>,
    replay: Option<AntigravityReasoningReplayAccumulator>,
    attempts: usize,
}

impl fmt::Debug for AntigravitySubscriptionExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravitySubscriptionExecutor")
            .field("auth", &"AntigravitySubscriptionAuth([REDACTED])")
            .field("transport", &"AntigravityGenerateTransport")
            .field(
                "stream_transport",
                &self.stream_transport.as_ref().map(|_| "attached"),
            )
            .field(
                "replay_cache",
                &self.replay_cache.as_ref().map(|_| "attached"),
            )
            .field("timeout", &self.timeout)
            .finish()
    }
}

pub struct AntigravityStreamExecutionOutcome {
    stream: AntigravityResponsesStream,
    attempts: usize,
}

impl AntigravityStreamExecutionOutcome {
    pub fn into_stream(self) -> AntigravityResponsesStream {
        self.stream
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
    ) -> AntigravityTrackedResponsesStream {
        AntigravityTrackedResponsesStream {
            stream: self.stream,
            auth_id,
            model,
            conductor,
            clock,
            failure_recorded: false,
        }
    }
}

impl fmt::Debug for AntigravityStreamExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityStreamExecutionOutcome")
            .field("stream", &"[REDACTED]")
            .field("attempts", &self.attempts)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AntigravityExecutionOutcome {
    payload: Vec<u8>,
    attempts: usize,
}

impl AntigravityExecutionOutcome {
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }
}

impl fmt::Debug for AntigravityExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityExecutionOutcome")
            .field("payload", &"[REDACTED]")
            .field("attempts", &self.attempts)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntigravityExecutionError {
    InvalidTimeout,
    Auth(AntigravitySubscriptionAuthError),
    Request(AntigravityRequestError),
    Transport(AntigravityGenerateTransportFailure),
    Http {
        status: u16,
        retry_after: Option<String>,
    },
    InvalidResponse,
    ReplayExhausted,
    ReplayCache,
    CreditsUnavailable,
    CreditsStore,
    StreamingUnavailable,
}

impl fmt::Display for AntigravityExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTimeout => formatter.write_str("Antigravity timeout must be non-zero"),
            Self::Auth(error) => write!(formatter, "Antigravity auth failed: {error}"),
            Self::Request(_) => formatter.write_str("Antigravity request is invalid"),
            Self::Transport(error) => write!(formatter, "Antigravity transport failed: {error:?}"),
            Self::Http { status, .. } => {
                write!(formatter, "Antigravity upstream returned HTTP {status}")
            }
            Self::InvalidResponse => formatter.write_str("Antigravity response is invalid"),
            Self::ReplayExhausted => formatter.write_str("Antigravity replay budget exhausted"),
            Self::ReplayCache => formatter.write_str("Antigravity replay cache failed"),
            Self::CreditsUnavailable => {
                formatter.write_str("Antigravity credits controller unavailable")
            }
            Self::CreditsStore => formatter.write_str("Antigravity credits store failed"),
            Self::StreamingUnavailable => {
                formatter.write_str("Antigravity streaming transport is unavailable")
            }
        }
    }
}

impl std::error::Error for AntigravityExecutionError {}

impl AntigravityExecutionError {
    fn account_status(&self) -> Option<(u16, Option<u64>)> {
        match self {
            Self::Http {
                status,
                retry_after,
            } => Some((*status, parse_retry_after_delay_ms(retry_after.as_deref()))),
            Self::Transport(_) | Self::InvalidResponse => Some((502, None)),
            Self::InvalidTimeout
            | Self::Auth(_)
            | Self::Request(_)
            | Self::ReplayExhausted
            | Self::ReplayCache
            | Self::CreditsUnavailable
            | Self::CreditsStore
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

/// Persisted-state Antigravity selection and bounded cross-account failover.
pub struct AntigravitySubscriptionAccountPool {
    router: Arc<AccountRouter>,
    conductor: Arc<CooldownConductor>,
    candidates: Vec<AccountCandidate>,
    executors: HashMap<String, Arc<AntigravitySubscriptionExecutor>>,
    targets: HashMap<String, AntigravityUpstreamTarget>,
    clock: Arc<dyn AccountStateClock>,
}

impl AntigravitySubscriptionAccountPool {
    pub fn with_clock(
        router: Arc<AccountRouter>,
        conductor: Arc<CooldownConductor>,
        candidates: Vec<AccountCandidate>,
        executors: HashMap<String, Arc<AntigravitySubscriptionExecutor>>,
        targets: HashMap<String, AntigravityUpstreamTarget>,
        clock: Arc<dyn AccountStateClock>,
    ) -> Result<Self, AntigravityAccountPoolError> {
        if candidates.is_empty() {
            return Err(AntigravityAccountPoolError::Configuration);
        }
        let mut seen = HashSet::new();
        for candidate in &candidates {
            if !candidate.provider.eq_ignore_ascii_case("antigravity")
                || candidate.auth_id.trim().is_empty()
                || !seen.insert(candidate.auth_id.as_str())
                || !executors.contains_key(&candidate.auth_id)
                || !targets.contains_key(&candidate.auth_id)
            {
                return Err(AntigravityAccountPoolError::Configuration);
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
        original_request: Vec<u8>,
        translated_body: Vec<u8>,
    ) -> Result<AntigravityPooledExecutionOutcome, AntigravityAccountPoolError> {
        self.execute_configured_with_client_headers(
            model,
            original_request,
            translated_body,
            &BTreeMap::new(),
        )
        .await
    }

    pub async fn execute_configured_with_client_headers(
        &self,
        model: &str,
        original_request: Vec<u8>,
        translated_body: Vec<u8>,
        client_headers: &BTreeMap<String, Vec<String>>,
    ) -> Result<AntigravityPooledExecutionOutcome, AntigravityAccountPoolError> {
        let mut remaining = self.candidates.clone();
        let mut attempted_auth_ids = Vec::new();
        let mut last_error = None;
        while !remaining.is_empty() {
            let selected = self
                .router
                .select("antigravity", Some(model), self.clock.now_ms(), &remaining)
                .map_err(AntigravityAccountPoolError::Routing)?;
            remaining.retain(|candidate| candidate.auth_id != selected.auth_id);
            attempted_auth_ids.push(selected.auth_id.clone());
            let executor = self
                .executors
                .get(&selected.auth_id)
                .ok_or(AntigravityAccountPoolError::Configuration)?;
            let target = self
                .targets
                .get(&selected.auth_id)
                .ok_or(AntigravityAccountPoolError::Configuration)?;
            match executor
                .execute_with_client_headers(
                    target,
                    model,
                    &original_request,
                    &translated_body,
                    client_headers,
                )
                .await
            {
                Ok(outcome) => {
                    self.record(&selected.auth_id, model, 200, None).await?;
                    return Ok(AntigravityPooledExecutionOutcome {
                        selected_auth_id: selected.auth_id,
                        attempted_auth_ids,
                        outcome,
                    });
                }
                Err(error) => {
                    let Some((status, retry_delay_ms)) = error.account_status() else {
                        return Err(AntigravityAccountPoolError::Execution(error));
                    };
                    self.record(&selected.auth_id, model, status, retry_delay_ms)
                        .await?;
                    if matches!(status, 400 | 422) {
                        return Err(AntigravityAccountPoolError::Execution(error));
                    }
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.map_or(
            AntigravityAccountPoolError::Configuration,
            AntigravityAccountPoolError::Execution,
        ))
    }

    /// Executes a buffered Claude Messages request on an account selected from
    /// capability-eligible lanes. Translation happens after selection and is
    /// repeated for each failover account, so native Web Search capability and
    /// required signature reads can never come from a process-wide preflight.
    pub async fn execute_claude_non_stream_configured<F>(
        &self,
        model: &str,
        original_request: Vec<u8>,
        signature_store: Option<&dyn SignatureKvStore>,
        supports_native_web_search: F,
    ) -> Result<AntigravityPooledExecutionOutcome, AntigravityAccountPoolError>
    where
        F: Fn(&str, &str) -> bool,
    {
        self.execute_claude_non_stream_configured_with_client_headers(
            model,
            original_request,
            signature_store,
            supports_native_web_search,
            &BTreeMap::new(),
        )
        .await
    }

    pub async fn execute_claude_non_stream_configured_with_client_headers<F>(
        &self,
        model: &str,
        original_request: Vec<u8>,
        signature_store: Option<&dyn SignatureKvStore>,
        supports_native_web_search: F,
        client_headers: &BTreeMap<String, Vec<String>>,
    ) -> Result<AntigravityPooledExecutionOutcome, AntigravityAccountPoolError>
    where
        F: Fn(&str, &str) -> bool,
    {
        let uses_native_web_search = claude_request_uses_native_web_search(&original_request);
        let mut remaining = self.candidates.clone();
        if uses_native_web_search {
            remaining.retain(|candidate| supports_native_web_search(&candidate.auth_id, model));
            if remaining.is_empty() {
                return Err(AntigravityAccountPoolError::CapabilityUnavailable);
            }
        }
        let mut attempted_auth_ids = Vec::new();
        let mut last_error = None;
        while !remaining.is_empty() {
            let selected = self
                .router
                .select("antigravity", Some(model), self.clock.now_ms(), &remaining)
                .map_err(AntigravityAccountPoolError::Routing)?;
            remaining.retain(|candidate| candidate.auth_id != selected.auth_id);
            attempted_auth_ids.push(selected.auth_id.clone());
            let executor = self
                .executors
                .get(&selected.auth_id)
                .ok_or(AntigravityAccountPoolError::Configuration)?;
            let target = self
                .targets
                .get(&selected.auth_id)
                .ok_or(AntigravityAccountPoolError::Configuration)?;
            let translated = convert_claude_request_to_antigravity_with_runtime(
                model,
                &original_request,
                false,
                AntigravityClaudeRequestCapabilities {
                    native_google_search: supports_native_web_search(&selected.auth_id, model),
                },
                signature_store,
            )
            .map_err(AntigravityAccountPoolError::Translation)?;
            let web_search_tool_use_id = format!("srvtoolu_{}", uuid::Uuid::new_v4().as_simple());
            match executor
                .execute_claude_non_stream(
                    target,
                    model,
                    &original_request,
                    &translated,
                    &web_search_tool_use_id,
                    client_headers,
                )
                .await
            {
                Ok(outcome) => {
                    self.record(&selected.auth_id, model, 200, None).await?;
                    return Ok(AntigravityPooledExecutionOutcome {
                        selected_auth_id: selected.auth_id,
                        attempted_auth_ids,
                        outcome,
                    });
                }
                Err(error) => {
                    let Some((status, retry_delay_ms)) = error.account_status() else {
                        return Err(AntigravityAccountPoolError::Execution(error));
                    };
                    self.record(&selected.auth_id, model, status, retry_delay_ms)
                        .await?;
                    if matches!(status, 400 | 422) {
                        return Err(AntigravityAccountPoolError::Execution(error));
                    }
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.map_or(
            AntigravityAccountPoolError::CapabilityUnavailable,
            AntigravityAccountPoolError::Execution,
        ))
    }

    /// Streaming counterpart of `execute_claude_non_stream_configured`. The
    /// selected stream owns the durable store for response-time signature
    /// publication and keeps post-commit failure accounting on the same lane.
    pub async fn execute_claude_stream_configured<F>(
        &self,
        model: &str,
        original_request: Vec<u8>,
        signature_store: Option<Arc<dyn SignatureKvStore>>,
        supports_native_web_search: F,
    ) -> Result<AntigravityPooledStreamExecutionOutcome, AntigravityAccountPoolError>
    where
        F: Fn(&str, &str) -> bool,
    {
        self.execute_claude_stream_configured_with_client_headers(
            model,
            original_request,
            signature_store,
            supports_native_web_search,
            &BTreeMap::new(),
        )
        .await
    }

    pub async fn execute_claude_stream_configured_with_client_headers<F>(
        &self,
        model: &str,
        original_request: Vec<u8>,
        signature_store: Option<Arc<dyn SignatureKvStore>>,
        supports_native_web_search: F,
        client_headers: &BTreeMap<String, Vec<String>>,
    ) -> Result<AntigravityPooledStreamExecutionOutcome, AntigravityAccountPoolError>
    where
        F: Fn(&str, &str) -> bool,
    {
        let uses_native_web_search = claude_request_uses_native_web_search(&original_request);
        let mut remaining = self.candidates.clone();
        if uses_native_web_search {
            remaining.retain(|candidate| supports_native_web_search(&candidate.auth_id, model));
            if remaining.is_empty() {
                return Err(AntigravityAccountPoolError::CapabilityUnavailable);
            }
        }
        let mut attempted_auth_ids = Vec::new();
        let mut last_error = None;
        while !remaining.is_empty() {
            let selected = self
                .router
                .select("antigravity", Some(model), self.clock.now_ms(), &remaining)
                .map_err(AntigravityAccountPoolError::Routing)?;
            remaining.retain(|candidate| candidate.auth_id != selected.auth_id);
            attempted_auth_ids.push(selected.auth_id.clone());
            let executor = self
                .executors
                .get(&selected.auth_id)
                .ok_or(AntigravityAccountPoolError::Configuration)?;
            let target = self
                .targets
                .get(&selected.auth_id)
                .ok_or(AntigravityAccountPoolError::Configuration)?;
            let translated = convert_claude_request_to_antigravity_with_runtime(
                model,
                &original_request,
                true,
                AntigravityClaudeRequestCapabilities {
                    native_google_search: supports_native_web_search(&selected.auth_id, model),
                },
                signature_store.as_deref(),
            )
            .map_err(AntigravityAccountPoolError::Translation)?;
            let web_search_tool_use_id = format!("srvtoolu_{}", uuid::Uuid::new_v4().as_simple());
            match executor
                .execute_claude_stream(
                    target,
                    model,
                    &original_request,
                    &translated,
                    web_search_tool_use_id,
                    signature_store.clone(),
                    client_headers,
                )
                .await
            {
                Ok(outcome) => {
                    self.record(&selected.auth_id, model, 200, None).await?;
                    let response = outcome.into_tracked(
                        selected.auth_id.clone(),
                        model.to_owned(),
                        Arc::clone(&self.conductor),
                        Arc::clone(&self.clock),
                    );
                    return Ok(AntigravityPooledStreamExecutionOutcome {
                        selected_auth_id: selected.auth_id,
                        attempted_auth_ids,
                        response,
                    });
                }
                Err(error) => {
                    let Some((status, retry_delay_ms)) = error.account_status() else {
                        return Err(AntigravityAccountPoolError::Execution(error));
                    };
                    self.record(&selected.auth_id, model, status, retry_delay_ms)
                        .await?;
                    if matches!(status, 400 | 422) {
                        return Err(AntigravityAccountPoolError::Execution(error));
                    }
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.map_or(
            AntigravityAccountPoolError::CapabilityUnavailable,
            AntigravityAccountPoolError::Execution,
        ))
    }

    pub async fn execute_stream_configured(
        &self,
        model: &str,
        original_request: Vec<u8>,
        translated_body: Vec<u8>,
    ) -> Result<AntigravityPooledStreamExecutionOutcome, AntigravityAccountPoolError> {
        self.execute_stream_configured_with_client_headers(
            model,
            original_request,
            translated_body,
            &BTreeMap::new(),
        )
        .await
    }

    pub async fn execute_stream_configured_with_client_headers(
        &self,
        model: &str,
        original_request: Vec<u8>,
        translated_body: Vec<u8>,
        client_headers: &BTreeMap<String, Vec<String>>,
    ) -> Result<AntigravityPooledStreamExecutionOutcome, AntigravityAccountPoolError> {
        let mut remaining = self.candidates.clone();
        let mut attempted_auth_ids = Vec::new();
        let mut last_error = None;
        while !remaining.is_empty() {
            let selected = self
                .router
                .select("antigravity", Some(model), self.clock.now_ms(), &remaining)
                .map_err(AntigravityAccountPoolError::Routing)?;
            remaining.retain(|candidate| candidate.auth_id != selected.auth_id);
            attempted_auth_ids.push(selected.auth_id.clone());
            let executor = self
                .executors
                .get(&selected.auth_id)
                .ok_or(AntigravityAccountPoolError::Configuration)?;
            let target = self
                .targets
                .get(&selected.auth_id)
                .ok_or(AntigravityAccountPoolError::Configuration)?;
            match executor
                .execute_stream_with_client_headers(
                    target,
                    model,
                    &original_request,
                    &translated_body,
                    client_headers,
                )
                .await
            {
                Ok(outcome) => {
                    self.record(&selected.auth_id, model, 200, None).await?;
                    let response = outcome.into_tracked(
                        selected.auth_id.clone(),
                        model.to_owned(),
                        Arc::clone(&self.conductor),
                        Arc::clone(&self.clock),
                    );
                    return Ok(AntigravityPooledStreamExecutionOutcome {
                        selected_auth_id: selected.auth_id,
                        attempted_auth_ids,
                        response,
                    });
                }
                Err(error) => {
                    let Some((status, retry_delay_ms)) = error.account_status() else {
                        return Err(AntigravityAccountPoolError::Execution(error));
                    };
                    self.record(&selected.auth_id, model, status, retry_delay_ms)
                        .await?;
                    if matches!(status, 400 | 422) {
                        return Err(AntigravityAccountPoolError::Execution(error));
                    }
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.map_or(
            AntigravityAccountPoolError::Configuration,
            AntigravityAccountPoolError::Execution,
        ))
    }

    async fn record(
        &self,
        auth_id: &str,
        model: &str,
        status: u16,
        retry_delay_ms: Option<u64>,
    ) -> Result<(), AntigravityAccountPoolError> {
        let conductor = Arc::clone(&self.conductor);
        let result = AccountExecutionResult {
            provider: "antigravity".to_owned(),
            auth_id: auth_id.to_owned(),
            model: Some(model.to_owned()),
            status,
            retry_delay_ms,
            observed_at_ms: self.clock.now_ms(),
        };
        tokio::task::spawn_blocking(move || conductor.record(result))
            .await
            .map_err(|_| AntigravityAccountPoolError::OutcomePersistence)?
            .map_err(|_| AntigravityAccountPoolError::OutcomePersistence)?;
        Ok(())
    }
}

impl fmt::Debug for AntigravitySubscriptionAccountPool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravitySubscriptionAccountPool")
            .field("router", &self.router)
            .field("candidate_count", &self.candidates.len())
            .field("executors", &"[REDACTED]")
            .field("targets", &self.targets)
            .finish()
    }
}

pub struct AntigravityPooledExecutionOutcome {
    selected_auth_id: String,
    attempted_auth_ids: Vec<String>,
    outcome: AntigravityExecutionOutcome,
}

pub struct AntigravityPooledStreamExecutionOutcome {
    selected_auth_id: String,
    attempted_auth_ids: Vec<String>,
    response: AntigravityTrackedResponsesStream,
}

impl AntigravityPooledStreamExecutionOutcome {
    pub fn selected_auth_id(&self) -> &str {
        &self.selected_auth_id
    }
    pub fn attempted_auth_ids(&self) -> &[String] {
        &self.attempted_auth_ids
    }
    pub fn into_response(self) -> AntigravityTrackedResponsesStream {
        self.response
    }
}

impl fmt::Debug for AntigravityPooledStreamExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityPooledStreamExecutionOutcome")
            .field("selected_auth_id", &self.selected_auth_id)
            .field("attempted_auth_ids", &self.attempted_auth_ids)
            .field("response", &self.response)
            .finish()
    }
}

pub struct AntigravityTrackedResponsesStream {
    stream: AntigravityResponsesStream,
    auth_id: String,
    model: String,
    conductor: Arc<CooldownConductor>,
    clock: Arc<dyn AccountStateClock>,
    failure_recorded: bool,
}

impl AntigravityTrackedResponsesStream {
    pub async fn next_event(
        &mut self,
    ) -> Option<Result<Vec<u8>, AntigravityGenerateTransportFailure>> {
        let next = self.stream.next_event().await;
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
            provider: "antigravity".to_owned(),
            auth_id: self.auth_id.clone(),
            model: Some(self.model.clone()),
            status: 502,
            retry_delay_ms: None,
            observed_at_ms: self.clock.now_ms(),
        };
        let _ = tokio::task::spawn_blocking(move || conductor.record(result)).await;
    }
}

impl fmt::Debug for AntigravityTrackedResponsesStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityTrackedResponsesStream")
            .field("stream", &"[REDACTED]")
            .field("auth_id", &self.auth_id)
            .field("model", &self.model)
            .field("failure_recorded", &self.failure_recorded)
            .finish()
    }
}

impl AntigravityPooledExecutionOutcome {
    pub fn selected_auth_id(&self) -> &str {
        &self.selected_auth_id
    }

    pub fn attempted_auth_ids(&self) -> &[String] {
        &self.attempted_auth_ids
    }

    pub fn outcome(&self) -> &AntigravityExecutionOutcome {
        &self.outcome
    }
}

impl fmt::Debug for AntigravityPooledExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityPooledExecutionOutcome")
            .field("selected_auth_id", &self.selected_auth_id)
            .field("attempted_auth_ids", &self.attempted_auth_ids)
            .field("outcome", &self.outcome)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntigravityAccountPoolError {
    Configuration,
    CapabilityUnavailable,
    Routing(AccountRoutingError),
    Translation(AntigravityClaudeRequestTranslationError),
    Execution(AntigravityExecutionError),
    OutcomePersistence,
}

impl fmt::Display for AntigravityAccountPoolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration => formatter.write_str("Antigravity account pool is invalid"),
            Self::CapabilityUnavailable => {
                formatter.write_str("Antigravity account capability is unavailable")
            }
            Self::Routing(error) => write!(formatter, "Antigravity routing failed: {error}"),
            Self::Translation(error) => {
                write!(formatter, "Antigravity request translation failed: {error}")
            }
            Self::Execution(error) => write!(formatter, "Antigravity execution failed: {error}"),
            Self::OutcomePersistence => {
                formatter.write_str("Antigravity account outcome persistence failed")
            }
        }
    }
}

impl std::error::Error for AntigravityAccountPoolError {}

#[cfg(all(test, feature = "antigravity-http-transport"))]
mod tests {
    use std::collections::VecDeque;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::time::{Duration, SystemTime};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::internal::auth::antigravity::{
        AntigravityCredentialHandles, AntigravityRefreshCoordinator,
        AntigravityRefreshHttpResponse, AntigravityRefreshRequest, AntigravityRefreshTransport,
        AntigravityRefreshTransportFailure, AntigravitySecretHandle, AntigravitySecretKind,
        AntigravitySecretStore, AntigravityStoredCredentials, AntigravityTokenError, SecretString,
    };
    use crate::internal::runtime::executor::{
        AntigravityGenerateHttpTransport, SystemAntigravityAuthClock,
    };
    use crate::internal::translator::antigravity::openai::responses::convert_openai_responses_request_to_antigravity;
    use crate::sdk::cliproxy::auth::{CooldownStateRecord, CooldownStateStore, CooldownStoreError};

    struct MemoryStore(Mutex<AntigravityStoredCredentials>);

    impl AntigravitySecretStore for MemoryStore {
        fn load_credentials(
            &self,
            _: &AntigravityCredentialHandles,
        ) -> Result<AntigravityStoredCredentials, AntigravityTokenError> {
            Ok(self
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone())
        }

        fn store_credentials(
            &self,
            _: &AntigravityCredentialHandles,
            credentials: &AntigravityStoredCredentials,
        ) -> Result<(), AntigravityTokenError> {
            *self.0.lock().unwrap_or_else(|error| error.into_inner()) = credentials.clone();
            Ok(())
        }
    }

    struct FailingSignatureStore;

    impl SignatureKvStore for FailingSignatureStore {
        fn get(
            &self,
            _key: &str,
        ) -> Result<Option<Vec<u8>>, crate::internal::cache::SignatureCacheStoreError> {
            Err(crate::internal::cache::SignatureCacheStoreError::Unavailable)
        }

        fn set(
            &self,
            _key: &str,
            _value: &[u8],
            _ttl: Duration,
        ) -> Result<bool, crate::internal::cache::SignatureCacheStoreError> {
            unreachable!("request conversion does not publish signatures")
        }

        fn delete(
            &self,
            _key: &str,
        ) -> Result<bool, crate::internal::cache::SignatureCacheStoreError> {
            unreachable!("request conversion does not delete signatures")
        }

        fn expire(
            &self,
            _key: &str,
            _ttl: Duration,
        ) -> Result<bool, crate::internal::cache::SignatureCacheStoreError> {
            unreachable!("failed reads do not refresh signatures")
        }
    }

    #[derive(Default)]
    struct RecordingSignatureStore(Mutex<Vec<(String, Vec<u8>, Duration)>>);

    impl SignatureKvStore for RecordingSignatureStore {
        fn get(
            &self,
            _key: &str,
        ) -> Result<Option<Vec<u8>>, crate::internal::cache::SignatureCacheStoreError> {
            Ok(None)
        }

        fn set(
            &self,
            key: &str,
            value: &[u8],
            ttl: Duration,
        ) -> Result<bool, crate::internal::cache::SignatureCacheStoreError> {
            self.0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push((key.to_owned(), value.to_vec(), ttl));
            Ok(true)
        }

        fn delete(
            &self,
            _key: &str,
        ) -> Result<bool, crate::internal::cache::SignatureCacheStoreError> {
            Ok(false)
        }

        fn expire(
            &self,
            _key: &str,
            _ttl: Duration,
        ) -> Result<bool, crate::internal::cache::SignatureCacheStoreError> {
            Ok(true)
        }
    }

    struct UnusedRefresh;

    impl AntigravityRefreshTransport for UnusedRefresh {
        fn execute<'a>(
            &'a self,
            _: &'a AntigravityRefreshRequest,
            _: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            AntigravityRefreshHttpResponse,
                            AntigravityRefreshTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async { panic!("generate loopback must not refresh") })
        }
    }

    struct RotatingRefresh;

    impl AntigravityRefreshTransport for RotatingRefresh {
        fn execute<'a>(
            &'a self,
            _: &'a AntigravityRefreshRequest,
            _: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            AntigravityRefreshHttpResponse,
                            AntigravityRefreshTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async {
                Ok(AntigravityRefreshHttpResponse::new(
                    200,
                    br#"{"access_token":"access-new","refresh_token":"refresh-new","expires_in":3600}"#.to_vec(),
                ))
            })
        }
    }

    struct SequenceGenerateTransport {
        responses: Mutex<VecDeque<super::super::antigravity_executor::AntigravityGenerateResponse>>,
        tokens: Mutex<Vec<String>>,
        bodies: Mutex<Vec<serde_json::Value>>,
    }

    impl SequenceGenerateTransport {
        fn new(
            responses: Vec<super::super::antigravity_executor::AntigravityGenerateResponse>,
        ) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                tokens: Mutex::new(Vec::new()),
                bodies: Mutex::new(Vec::new()),
            }
        }
    }

    impl AntigravityGenerateTransport for SequenceGenerateTransport {
        fn execute<'a>(
            &'a self,
            request: &'a AntigravityGenerateRequest,
            _: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            super::super::antigravity_executor::AntigravityGenerateResponse,
                            AntigravityGenerateTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            self.tokens
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(request.access_token().expose_secret().to_owned());
            self.bodies
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(serde_json::from_slice(request.body()).unwrap());
            let response = self
                .responses
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .pop_front()
                .expect("one response per request");
            Box::pin(async move { Ok(response) })
        }
    }

    type FixedStreamChunks = Vec<Result<Vec<u8>, AntigravityGenerateTransportFailure>>;

    struct FixedStreamTransport {
        chunks: Mutex<VecDeque<FixedStreamChunks>>,
    }

    impl FixedStreamTransport {
        fn new(chunks: Vec<FixedStreamChunks>) -> Self {
            Self {
                chunks: Mutex::new(chunks.into()),
            }
        }
    }

    struct SequenceStatusStreamTransport {
        statuses: Mutex<VecDeque<u16>>,
        tokens: Mutex<Vec<String>>,
    }

    impl SequenceStatusStreamTransport {
        fn new(statuses: impl IntoIterator<Item = u16>) -> Self {
            Self {
                statuses: Mutex::new(statuses.into_iter().collect()),
                tokens: Mutex::new(Vec::new()),
            }
        }
    }

    impl AntigravityGenerateStreamingTransport for SequenceStatusStreamTransport {
        fn execute_stream<'a>(
            &'a self,
            request: &'a AntigravityGenerateRequest,
            _: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            super::super::antigravity_executor::AntigravityGenerateStreamResponse,
                            AntigravityGenerateTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            self.tokens
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(request.access_token().expose_secret().to_owned());
            let status = self
                .statuses
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .pop_front()
                .expect("one status per stream request");
            Box::pin(async move {
                let (sender, receiver) = tokio::sync::mpsc::channel(2);
                if status == 200 {
                    sender
                        .send(Ok(b"data: {\"response\":{\"responseId\":\"refresh-stream\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"ok\"}]}}]}}\n\n".to_vec()))
                        .await
                        .unwrap();
                }
                drop(sender);
                Ok(
                    super::super::antigravity_executor::AntigravityGenerateStreamResponse::new(
                        status, None, receiver,
                    ),
                )
            })
        }
    }

    impl AntigravityGenerateStreamingTransport for FixedStreamTransport {
        fn execute_stream<'a>(
            &'a self,
            _: &'a AntigravityGenerateRequest,
            _: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            super::super::antigravity_executor::AntigravityGenerateStreamResponse,
                            AntigravityGenerateTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            let chunks = self
                .chunks
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .pop_front()
                .expect("one stream response per request");
            Box::pin(async move {
                let (sender, receiver) = tokio::sync::mpsc::channel(8);
                for chunk in chunks {
                    sender.send(chunk).await.unwrap();
                }
                drop(sender);
                Ok(
                    super::super::antigravity_executor::AntigravityGenerateStreamResponse::new(
                        200, None, receiver,
                    ),
                )
            })
        }
    }

    #[derive(Default)]
    struct MemoryCooldown(Mutex<Vec<CooldownStateRecord>>);

    impl CooldownStateStore for MemoryCooldown {
        fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
            Ok(self
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone())
        }

        fn save(&self, records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
            *self.0.lock().unwrap_or_else(|error| error.into_inner()) = records.to_vec();
            Ok(())
        }
    }

    struct FixedAccountClock(i64);

    impl AccountStateClock for FixedAccountClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    fn handles() -> AntigravityCredentialHandles {
        let handle =
            |name, kind| AntigravitySecretHandle::new("subscriptions", name, kind).unwrap();
        AntigravityCredentialHandles::new(
            handle("access", AntigravitySecretKind::AccessToken),
            handle("refresh", AntigravitySecretKind::RefreshToken),
            handle("state", AntigravitySecretKind::State),
        )
        .unwrap()
    }

    fn auth_with(
        access_token: &str,
        refresh_transport: Arc<dyn AntigravityRefreshTransport>,
    ) -> (Arc<AntigravitySubscriptionAuth>, Arc<MemoryStore>) {
        let credentials = AntigravityStoredCredentials::new(
            SecretString::new(access_token).unwrap(),
            SecretString::new(format!("refresh-{access_token}")).unwrap(),
            SystemTime::now() + Duration::from_secs(3600),
            format!("project-{access_token}"),
        )
        .unwrap();
        let store = Arc::new(MemoryStore(Mutex::new(credentials)));
        let auth = Arc::new(AntigravitySubscriptionAuth::new(
            handles(),
            store.clone(),
            refresh_transport,
            Arc::new(SystemAntigravityAuthClock),
            Arc::new(AntigravityRefreshCoordinator::default()),
        ));
        (auth, store)
    }

    fn success_response(
        id: &str,
    ) -> super::super::antigravity_executor::AntigravityGenerateResponse {
        super::super::antigravity_executor::AntigravityGenerateResponse::new(
            200,
            None,
            format!(r#"{{"response":{{"responseId":"{id}","candidates":[{{"content":{{"parts":[{{"text":"ok"}}]}}}}]}}}}"#).into_bytes(),
        )
    }

    #[tokio::test]
    async fn codex_client_integer_schema_reaches_antigravity_generate_body() {
        let (auth, _) = auth_with("access-live", Arc::new(UnusedRefresh));
        let transport = Arc::new(SequenceGenerateTransport::new(vec![
            success_response("integer-schema"),
            success_response("number-schema"),
        ]));
        let executor =
            AntigravitySubscriptionExecutor::new(auth, transport.clone(), Duration::from_secs(5))
                .unwrap();
        let original = br#"{"model":"gemini-3-flash-agent","input":"hello","tools":[{"type":"function","name":"exec_command","parameters":{"type":"object","properties":{"yield_time_ms":{"type":"number"}}}}]}"#;
        let translated = convert_openai_responses_request_to_antigravity(
            "gemini-3-flash-agent",
            original,
            false,
        );
        let codex_headers = BTreeMap::from([(
            "User-Agent".to_owned(),
            vec!["Codex Desktop/1.0".to_owned()],
        )]);
        executor
            .execute_with_client_headers(
                &AntigravityUpstreamTarget::default_subscription(),
                "gemini-3-flash-agent",
                original,
                &translated,
                &codex_headers,
            )
            .await
            .unwrap();
        executor
            .execute_with_client_headers(
                &AntigravityUpstreamTarget::default_subscription(),
                "gemini-3-flash-agent",
                original,
                &translated,
                &BTreeMap::new(),
            )
            .await
            .unwrap();
        let bodies = transport
            .bodies
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert_eq!(
            bodies[0].pointer(
                "/request/tools/0/functionDeclarations/0/parameters/properties/yield_time_ms/type"
            ),
            Some(&serde_json::json!("integer"))
        );
        assert_eq!(
            bodies[1].pointer(
                "/request/tools/0/functionDeclarations/0/parameters/properties/yield_time_ms/type"
            ),
            Some(&serde_json::json!("number"))
        );
    }

    fn two_account_pool(
        executor_a: Arc<AntigravitySubscriptionExecutor>,
        executor_b: Arc<AntigravitySubscriptionExecutor>,
        state: Arc<MemoryCooldown>,
        now_ms: i64,
    ) -> AntigravitySubscriptionAccountPool {
        let candidates = ["account-a", "account-b"]
            .into_iter()
            .map(|auth_id| AccountCandidate {
                auth_id: auth_id.to_owned(),
                provider: "antigravity".to_owned(),
                priority: 0,
                weight: 1,
                websocket_enabled: false,
                supported_models: Vec::new(),
                disabled: false,
            })
            .collect();
        let target = AntigravityUpstreamTarget::default_subscription();
        AntigravitySubscriptionAccountPool::with_clock(
            Arc::new(AccountRouter::new(state.clone())),
            Arc::new(CooldownConductor::new(state)),
            candidates,
            HashMap::from([
                ("account-a".to_owned(), executor_a),
                ("account-b".to_owned(), executor_b),
            ]),
            HashMap::from([
                ("account-a".to_owned(), target.clone()),
                ("account-b".to_owned(), target),
            ]),
            Arc::new(FixedAccountClock(now_ms)),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn unauthorized_refreshes_rebuilds_bearer_request_exactly_once() {
        let (auth, store) = auth_with("access-old", Arc::new(RotatingRefresh));
        let usage_manager = Arc::new(crate::sdk::cliproxy::usage::Manager::new(1));
        let reporter = Arc::new(
            crate::internal::runtime::executor::helps::UsageReporter::new(
                usage_manager.clone(),
                crate::sdk::cliproxy::usage::UsageContext::default(),
                "antigravity",
                "AntigravitySubscriptionExecutor",
                "gemini-3-flash-agent",
                None,
                "",
            ),
        );
        let transport = Arc::new(SequenceGenerateTransport::new(vec![
            super::super::antigravity_executor::AntigravityGenerateResponse::new(
                401,
                None,
                b"provider secret must not leak".to_vec(),
            ),
            success_response("refresh-1"),
        ]));
        let executor =
            AntigravitySubscriptionExecutor::new(auth, transport.clone(), Duration::from_secs(5))
                .unwrap()
                .with_access_token_fingerprint_sink(reporter.clone());
        let original = br#"{"model":"gemini-3-flash-agent","input":"hello"}"#;
        let translated = convert_openai_responses_request_to_antigravity(
            "gemini-3-flash-agent",
            original,
            false,
        );
        let outcome = executor
            .execute(
                &AntigravityUpstreamTarget::default_subscription(),
                "gemini-3-flash-agent",
                original,
                &translated,
            )
            .await
            .unwrap();
        assert_eq!(outcome.attempts(), 2);
        assert_eq!(
            *transport
                .tokens
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
            vec!["access-old", "access-new"]
        );
        assert_eq!(
            store
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .access_token()
                .expose_secret(),
            "access-new"
        );
        let bodies = transport
            .bodies
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0]["project"], bodies[1]["project"]);
        assert_ne!(bodies[0]["requestId"], bodies[1]["requestId"]);
        let usage = reporter.build_record(
            crate::sdk::cliproxy::usage::Detail::default(),
            false,
            crate::sdk::cliproxy::usage::Failure::default(),
        );
        assert_eq!(
            usage.access_token_sha256,
            format!("{:x}", Sha256::digest(b"access-new"))
        );
        assert!(!format!("{reporter:?}").contains("access-new"));
        usage_manager.stop();
    }

    #[tokio::test]
    async fn unauthorized_stream_refresh_updates_usage_reporter_fingerprint() {
        let (auth, _) = auth_with("access-old", Arc::new(RotatingRefresh));
        let usage_manager = Arc::new(crate::sdk::cliproxy::usage::Manager::new(1));
        let reporter = Arc::new(
            crate::internal::runtime::executor::helps::UsageReporter::new(
                usage_manager.clone(),
                crate::sdk::cliproxy::usage::UsageContext::default(),
                "antigravity",
                "AntigravitySubscriptionExecutor",
                "gemini-3-flash-agent",
                None,
                "",
            ),
        );
        let stream_transport = Arc::new(SequenceStatusStreamTransport::new([401, 200]));
        let executor = AntigravitySubscriptionExecutor::new(
            auth,
            Arc::new(SequenceGenerateTransport::new(Vec::new())),
            Duration::from_secs(5),
        )
        .unwrap()
        .with_stream_transport(stream_transport.clone())
        .with_access_token_fingerprint_sink(reporter.clone());
        let original = br#"{"model":"gemini-3-flash-agent","input":"hello"}"#;
        let translated =
            convert_openai_responses_request_to_antigravity("gemini-3-flash-agent", original, true);

        let outcome = executor
            .execute_stream(
                &AntigravityUpstreamTarget::default_subscription(),
                "gemini-3-flash-agent",
                original,
                &translated,
            )
            .await
            .unwrap();
        assert_eq!(outcome.attempts(), 2);
        assert_eq!(
            *stream_transport
                .tokens
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
            vec!["access-old", "access-new"]
        );
        let usage = reporter.build_record(
            crate::sdk::cliproxy::usage::Detail::default(),
            false,
            crate::sdk::cliproxy::usage::Failure::default(),
        );
        assert_eq!(
            usage.access_token_sha256,
            format!("{:x}", Sha256::digest(b"access-new"))
        );
        assert!(!format!("{reporter:?}").contains("access-new"));
        usage_manager.stop();
    }

    #[tokio::test]
    async fn explicit_session_lane_replays_terminal_signature_on_next_turn() {
        let (auth, _) = auth_with("access-replay", Arc::new(UnusedRefresh));
        let response = |id: &str, text: &str, signature: Option<&str>| {
            super::super::antigravity_executor::AntigravityGenerateResponse::new(
                200,
                None,
                serde_json::to_vec(&serde_json::json!({
                    "response": {
                        "responseId": id,
                        "candidates": [{
                            "content": {"parts": [{
                                "text": text,
                                "thoughtSignature": signature
                            }]},
                            "finishReason": "STOP"
                        }]
                    }
                }))
                .unwrap(),
            )
        };
        let transport = Arc::new(SequenceGenerateTransport::new(vec![
            response(
                "replay-first",
                "signed answer",
                Some("native-replay-signature-123456"),
            ),
            response("replay-second", "done", None),
        ]));
        let cache = Arc::new(AntigravityReasoningReplayCache::new());
        let executor =
            AntigravitySubscriptionExecutor::new(auth, transport.clone(), Duration::from_secs(5))
                .unwrap()
                .with_reasoning_replay_cache(cache);
        let target = AntigravityUpstreamTarget::default_subscription();
        executor
            .execute(
                &target,
                "gemini-3-flash-agent",
                br#"{"model":"gemini-3-flash-agent","session_id":"conversation-a","input":"first"}"#,
                br#"{"request":{"contents":[{"role":"user","parts":[{"text":"first"}]}]}}"#,
            )
            .await
            .unwrap();
        executor
            .execute(
                &target,
                "gemini-3-flash-agent",
                br#"{"model":"gemini-3-flash-agent","session_id":"conversation-a","input":"second"}"#,
                br#"{"request":{"contents":[{"role":"user","parts":[{"text":"first"}]},{"role":"model","parts":[{"text":"signed answer"}]},{"role":"user","parts":[{"text":"second"}]}]}}"#,
            )
            .await
            .unwrap();
        let bodies = transport
            .bodies
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert_eq!(bodies.len(), 2);
        assert_eq!(
            bodies[1].pointer("/request/contents/1/parts/0/thoughtSignature"),
            Some(&serde_json::Value::String(
                "native-replay-signature-123456".to_owned()
            ))
        );
    }

    #[tokio::test]
    async fn account_pool_persists_429_then_fails_over_to_second_account() {
        let (auth_a, _) = auth_with("access-a", Arc::new(UnusedRefresh));
        let (auth_b, _) = auth_with("access-b", Arc::new(UnusedRefresh));
        let transport_a = Arc::new(SequenceGenerateTransport::new(vec![
            super::super::antigravity_executor::AntigravityGenerateResponse::new(
                429,
                Some("7".to_owned()),
                b"quota body secret".to_vec(),
            ),
        ]));
        let transport_b = Arc::new(SequenceGenerateTransport::new(vec![success_response(
            "pool-b",
        )]));
        let executor_a = Arc::new(
            AntigravitySubscriptionExecutor::new(auth_a, transport_a, Duration::from_secs(5))
                .unwrap(),
        );
        let executor_b = Arc::new(
            AntigravitySubscriptionExecutor::new(auth_b, transport_b, Duration::from_secs(5))
                .unwrap(),
        );
        let state = Arc::new(MemoryCooldown::default());
        let router = Arc::new(AccountRouter::new(state.clone()));
        let conductor = Arc::new(CooldownConductor::new(state.clone()));
        let candidates = ["account-a", "account-b"]
            .into_iter()
            .map(|auth_id| AccountCandidate {
                auth_id: auth_id.to_owned(),
                provider: "antigravity".to_owned(),
                priority: 0,
                weight: 1,
                websocket_enabled: false,
                supported_models: Vec::new(),
                disabled: false,
            })
            .collect();
        let executors = HashMap::from([
            ("account-a".to_owned(), executor_a),
            ("account-b".to_owned(), executor_b),
        ]);
        let target = AntigravityUpstreamTarget::default_subscription();
        let targets = HashMap::from([
            ("account-a".to_owned(), target.clone()),
            ("account-b".to_owned(), target),
        ]);
        let pool = AntigravitySubscriptionAccountPool::with_clock(
            router,
            conductor,
            candidates,
            executors,
            targets,
            Arc::new(FixedAccountClock(10_000)),
        )
        .unwrap();
        let original = br#"{"model":"gemini-3-flash-agent","input":"hello"}"#.to_vec();
        let translated = convert_openai_responses_request_to_antigravity(
            "gemini-3-flash-agent",
            &original,
            false,
        );
        let outcome = pool
            .execute_configured("gemini-3-flash-agent", original, translated)
            .await
            .unwrap();
        assert_eq!(outcome.selected_auth_id(), "account-b");
        assert_eq!(outcome.attempted_auth_ids(), ["account-a", "account-b"]);
        let records = state.0.lock().unwrap_or_else(|error| error.into_inner());
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].auth_id, "account-a");
        assert_eq!(records[0].next_retry_after_ms, Some(17_000));
        assert_eq!(records[0].last_error.as_ref().unwrap().message, "");
    }

    #[tokio::test]
    async fn claude_web_search_selects_only_account_with_exact_model_capability() {
        let (auth_a, _) = auth_with("search-a", Arc::new(UnusedRefresh));
        let (auth_b, _) = auth_with("search-b", Arc::new(UnusedRefresh));
        let transport_a = Arc::new(SequenceGenerateTransport::new(Vec::new()));
        let transport_b = Arc::new(SequenceGenerateTransport::new(vec![success_response(
            "search-b",
        )]));
        let executor_a = Arc::new(
            AntigravitySubscriptionExecutor::new(
                auth_a,
                transport_a.clone(),
                Duration::from_secs(5),
            )
            .unwrap(),
        );
        let executor_b = Arc::new(
            AntigravitySubscriptionExecutor::new(
                auth_b,
                transport_b.clone(),
                Duration::from_secs(5),
            )
            .unwrap(),
        );
        let pool = two_account_pool(
            executor_a,
            executor_b,
            Arc::new(MemoryCooldown::default()),
            30_000,
        );
        let model = "gemini-3.1-flash-lite";
        let request = br#"{"model":"gemini-3.1-flash-lite","messages":[{"role":"user","content":"weather"}],"tools":[{"type":"web_search_20250305","name":"web_search"}]}"#.to_vec();

        let outcome = pool
            .execute_claude_non_stream_configured(model, request, None, |auth_id, exact_model| {
                auth_id == "account-b" && exact_model == model
            })
            .await
            .unwrap();

        assert_eq!(outcome.selected_auth_id(), "account-b");
        assert_eq!(outcome.attempted_auth_ids(), ["account-b"]);
        assert!(transport_a
            .bodies
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_empty());
        let bodies = transport_b
            .bodies
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert_eq!(bodies[0]["requestType"], "web_search");
        let response: serde_json::Value =
            serde_json::from_slice(outcome.outcome().payload()).unwrap();
        assert_eq!(response["type"], "message");
    }

    #[tokio::test]
    async fn claude_web_search_failover_never_downgrades_to_incapable_account() {
        let (auth_a, _) = auth_with("search-fail-a", Arc::new(UnusedRefresh));
        let (auth_b, _) = auth_with("search-fail-b", Arc::new(UnusedRefresh));
        let transport_a = Arc::new(SequenceGenerateTransport::new(vec![
            super::super::antigravity_executor::AntigravityGenerateResponse::new(
                503,
                None,
                b"unavailable".to_vec(),
            ),
        ]));
        let transport_b = Arc::new(SequenceGenerateTransport::new(Vec::new()));
        let executor_a = Arc::new(
            AntigravitySubscriptionExecutor::new(
                auth_a,
                transport_a.clone(),
                Duration::from_secs(5),
            )
            .unwrap(),
        );
        let executor_b = Arc::new(
            AntigravitySubscriptionExecutor::new(
                auth_b,
                transport_b.clone(),
                Duration::from_secs(5),
            )
            .unwrap(),
        );
        let state = Arc::new(MemoryCooldown::default());
        let pool = two_account_pool(executor_a, executor_b, state.clone(), 40_000);
        let model = "gemini-3.1-flash-lite";
        let request = br#"{"model":"gemini-3.1-flash-lite","messages":[{"role":"user","content":"weather"}],"tools":[{"type":"web_search_20250305","name":"web_search"}]}"#.to_vec();

        let error = pool
            .execute_claude_non_stream_configured(model, request, None, |auth_id, _| {
                auth_id == "account-a"
            })
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            AntigravityAccountPoolError::Execution(AntigravityExecutionError::Http {
                status: 503,
                ..
            })
        ));
        assert!(transport_b
            .bodies
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_empty());
        let records = state.0.lock().unwrap_or_else(|error| error.into_inner());
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].auth_id, "account-a");
    }

    #[tokio::test]
    async fn claude_selected_lane_propagates_required_signature_store_failure_before_transport() {
        let (auth_a, _) = auth_with("signature-a", Arc::new(UnusedRefresh));
        let (auth_b, _) = auth_with("signature-b", Arc::new(UnusedRefresh));
        let transport_a = Arc::new(SequenceGenerateTransport::new(Vec::new()));
        let transport_b = Arc::new(SequenceGenerateTransport::new(Vec::new()));
        let executor_a = Arc::new(
            AntigravitySubscriptionExecutor::new(
                auth_a,
                transport_a.clone(),
                Duration::from_secs(5),
            )
            .unwrap(),
        );
        let executor_b = Arc::new(
            AntigravitySubscriptionExecutor::new(
                auth_b,
                transport_b.clone(),
                Duration::from_secs(5),
            )
            .unwrap(),
        );
        let pool = two_account_pool(
            executor_a,
            executor_b,
            Arc::new(MemoryCooldown::default()),
            50_000,
        );
        let request = br#"{"model":"claude-thinking-test","messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"why","signature":"client"}]}]}"#.to_vec();

        let error = pool
            .execute_claude_non_stream_configured(
                "claude-thinking-test",
                request,
                Some(&FailingSignatureStore),
                |_, _| false,
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            AntigravityAccountPoolError::Translation(
                AntigravityClaudeRequestTranslationError::SignatureCache(
                    crate::internal::cache::SignatureCacheStoreError::Unavailable
                )
            )
        ));
        assert!(transport_a
            .bodies
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_empty());
        assert!(transport_b
            .bodies
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_empty());
    }

    #[tokio::test]
    async fn claude_stream_web_search_selects_capable_account_before_bootstrap() {
        let (auth_a, _) = auth_with("stream-search-a", Arc::new(UnusedRefresh));
        let (auth_b, _) = auth_with("stream-search-b", Arc::new(UnusedRefresh));
        let executor_a = Arc::new(
            AntigravitySubscriptionExecutor::new(
                auth_a,
                Arc::new(SequenceGenerateTransport::new(Vec::new())),
                Duration::from_secs(5),
            )
            .unwrap()
            .with_stream_transport(Arc::new(FixedStreamTransport::new(Vec::new()))),
        );
        let executor_b = Arc::new(
            AntigravitySubscriptionExecutor::new(
                auth_b,
                Arc::new(SequenceGenerateTransport::new(Vec::new())),
                Duration::from_secs(5),
            )
            .unwrap()
            .with_stream_transport(Arc::new(FixedStreamTransport::new(vec![vec![Ok(
                b"data: {\"response\":{\"responseId\":\"claude-search\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"weather\"}]}}]}}\n\n"
                    .to_vec(),
            )]]))),
        );
        let pool = two_account_pool(
            executor_a,
            executor_b,
            Arc::new(MemoryCooldown::default()),
            60_000,
        );
        let model = "gemini-3.1-flash-lite";
        let request = br#"{"model":"gemini-3.1-flash-lite","messages":[{"role":"user","content":"weather"}],"tools":[{"type":"web_search_20250305","name":"web_search"}]}"#.to_vec();

        let outcome = pool
            .execute_claude_stream_configured(model, request, None, |auth_id, exact_model| {
                auth_id == "account-b" && exact_model == model
            })
            .await
            .unwrap();
        assert_eq!(outcome.selected_auth_id(), "account-b");
        assert_eq!(outcome.attempted_auth_ids(), ["account-b"]);
        let mut response = outcome.into_response();
        let first = response.next_event().await.unwrap().unwrap();
        assert!(String::from_utf8_lossy(&first).contains("message_start"));
    }

    #[tokio::test]
    async fn claude_stream_selected_lane_publishes_signature_to_durable_store() {
        let (auth_a, _) = auth_with("stream-signature-a", Arc::new(UnusedRefresh));
        let (auth_b, _) = auth_with("stream-signature-b", Arc::new(UnusedRefresh));
        let signature = "signature_12345678901234567890123456789012345678901234567890";
        let chunks = vec![
            Ok(b"data: {\"response\":{\"responseId\":\"claude-thinking\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"stream thought\",\"thought\":true}]}}]}}\n\n".to_vec()),
            Ok(format!("data: {{\"response\":{{\"candidates\":[{{\"content\":{{\"parts\":[{{\"text\":\"\",\"thoughtSignature\":\"{signature}\"}}]}}}}]}}}}\n\n").into_bytes()),
        ];
        let executor_a = Arc::new(
            AntigravitySubscriptionExecutor::new(
                auth_a,
                Arc::new(SequenceGenerateTransport::new(Vec::new())),
                Duration::from_secs(5),
            )
            .unwrap()
            .with_stream_transport(Arc::new(FixedStreamTransport::new(vec![chunks]))),
        );
        let executor_b = Arc::new(
            AntigravitySubscriptionExecutor::new(
                auth_b,
                Arc::new(SequenceGenerateTransport::new(Vec::new())),
                Duration::from_secs(5),
            )
            .unwrap()
            .with_stream_transport(Arc::new(FixedStreamTransport::new(Vec::new()))),
        );
        let pool = two_account_pool(
            executor_a,
            executor_b,
            Arc::new(MemoryCooldown::default()),
            70_000,
        );
        let store = Arc::new(RecordingSignatureStore::default());
        let runtime_store: Arc<dyn SignatureKvStore> = store.clone();
        let request =
            br#"{"model":"claude-thinking-test","messages":[{"role":"user","content":"think"}]}"#
                .to_vec();

        let outcome = pool
            .execute_claude_stream_configured(
                "claude-thinking-test",
                request,
                Some(runtime_store),
                |_, _| false,
            )
            .await
            .unwrap();
        assert_eq!(outcome.selected_auth_id(), "account-a");
        let mut response = outcome.into_response();
        let mut output = Vec::new();
        while let Some(event) = response.next_event().await {
            output.extend(event.unwrap());
        }
        assert!(String::from_utf8_lossy(&output).contains("signature_delta"));
        let writes = store.0.lock().unwrap_or_else(|error| error.into_inner());
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].1, signature.as_bytes());
        assert_eq!(writes[0].2, Duration::from_secs(3 * 60 * 60));
    }

    #[tokio::test]
    async fn stream_pool_fails_over_pre_commit_and_cools_selected_post_commit_failure() {
        let (auth_a, _) = auth_with("stream-a", Arc::new(UnusedRefresh));
        let (auth_b, _) = auth_with("stream-b", Arc::new(UnusedRefresh));
        let unused_a = Arc::new(SequenceGenerateTransport::new(Vec::new()));
        let unused_b = Arc::new(SequenceGenerateTransport::new(Vec::new()));
        let stream_a = Arc::new(FixedStreamTransport::new(vec![vec![Err(
            AntigravityGenerateTransportFailure::Protocol,
        )]]));
        let stream_b = Arc::new(FixedStreamTransport::new(vec![vec![
            Ok(b"data: {\"response\":{\"responseId\":\"pool-stream-b\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"started\"}]}}]}}\n\n".to_vec()),
            Err(AntigravityGenerateTransportFailure::Connect),
        ]]));
        let executor_a = Arc::new(
            AntigravitySubscriptionExecutor::new(auth_a, unused_a, Duration::from_secs(5))
                .unwrap()
                .with_stream_transport(stream_a),
        );
        let executor_b = Arc::new(
            AntigravitySubscriptionExecutor::new(auth_b, unused_b, Duration::from_secs(5))
                .unwrap()
                .with_stream_transport(stream_b),
        );
        let state = Arc::new(MemoryCooldown::default());
        let router = Arc::new(AccountRouter::new(state.clone()));
        let conductor = Arc::new(CooldownConductor::new(state.clone()));
        let candidates = ["account-a", "account-b"]
            .into_iter()
            .map(|auth_id| AccountCandidate {
                auth_id: auth_id.to_owned(),
                provider: "antigravity".to_owned(),
                priority: 0,
                weight: 1,
                websocket_enabled: false,
                supported_models: Vec::new(),
                disabled: false,
            })
            .collect();
        let target = AntigravityUpstreamTarget::default_subscription();
        let pool = AntigravitySubscriptionAccountPool::with_clock(
            router,
            conductor,
            candidates,
            HashMap::from([
                ("account-a".to_owned(), executor_a),
                ("account-b".to_owned(), executor_b),
            ]),
            HashMap::from([
                ("account-a".to_owned(), target.clone()),
                ("account-b".to_owned(), target),
            ]),
            Arc::new(FixedAccountClock(20_000)),
        )
        .unwrap();
        let original = br#"{"model":"gemini-3-flash-agent","input":"hello"}"#.to_vec();
        let translated = convert_openai_responses_request_to_antigravity(
            "gemini-3-flash-agent",
            &original,
            true,
        );
        let outcome = pool
            .execute_stream_configured("gemini-3-flash-agent", original, translated)
            .await
            .unwrap();
        assert_eq!(outcome.selected_auth_id(), "account-b");
        assert_eq!(outcome.attempted_auth_ids(), ["account-a", "account-b"]);
        let mut response = outcome.into_response();
        let mut saw_created = false;
        let mut saw_failure = false;
        while let Some(event) = response.next_event().await {
            match event {
                Ok(event) => {
                    saw_created |= String::from_utf8_lossy(&event).contains("response.created")
                }
                Err(_) => {
                    saw_failure = true;
                    break;
                }
            }
        }
        assert!(saw_created && saw_failure);
        let records = state.0.lock().unwrap_or_else(|error| error.into_inner());
        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|record| record.auth_id == "account-a"));
        assert!(records.iter().any(|record| record.auth_id == "account-b"));
        assert!(records.iter().all(|record| record
            .last_error
            .as_ref()
            .is_none_or(|error| error.message.is_empty())));
    }

    #[tokio::test]
    async fn real_generate_loopback_runs_request_transport_and_response_conversion() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target =
            AntigravityUpstreamTarget::new(format!("http://{}", listener.local_addr().unwrap()))
                .unwrap();
        let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let server_capture = Arc::clone(&captured);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let body_start = end + 4;
                    let headers = String::from_utf8_lossy(&request[..body_start]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= body_start + length {
                        break;
                    }
                }
            }
            *server_capture.lock().await = request;
            let body = br#"{"response":{"responseId":"wire-1","createTime":"2026-08-03T12:34:56Z","candidates":[{"content":{"parts":[{"text":"wire answer"}]}}],"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":3,"totalTokenCount":5}}}"#;
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(headers.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
        });

        let credentials = AntigravityStoredCredentials::new(
            SecretString::new("access-wire-secret").unwrap(),
            SecretString::new("refresh-wire-secret").unwrap(),
            SystemTime::now() + Duration::from_secs(3600),
            "project-wire",
        )
        .unwrap();
        let auth = Arc::new(AntigravitySubscriptionAuth::new(
            handles(),
            Arc::new(MemoryStore(Mutex::new(credentials))),
            Arc::new(UnusedRefresh),
            Arc::new(SystemAntigravityAuthClock),
            Arc::new(AntigravityRefreshCoordinator::default()),
        ));
        let executor = AntigravitySubscriptionExecutor::new(
            auth,
            Arc::new(AntigravityGenerateHttpTransport::new(None).unwrap()),
            Duration::from_secs(5),
        )
        .unwrap();
        let original = br#"{"model":"gemini-3-flash-agent","input":"hello"}"#;
        let translated = convert_openai_responses_request_to_antigravity(
            "gemini-3-flash-agent",
            original,
            false,
        );
        let outcome = executor
            .execute(&target, "gemini-3-flash-agent", original, &translated)
            .await
            .unwrap();
        server.await.unwrap();

        let response: serde_json::Value = serde_json::from_slice(outcome.payload()).unwrap();
        assert_eq!(response["id"], "resp_wire-1");
        assert_eq!(response["output"][0]["content"][0]["text"], "wire answer");
        assert_eq!(response["usage"]["total_tokens"], 5);

        let request = String::from_utf8(captured.lock().await.clone()).unwrap();
        let lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /v1internal:generateContent HTTP/1.1\r\n"));
        assert!(lower.contains("authorization: bearer access-wire-secret"));
        assert!(lower.contains("user-agent: antigravity/hub/2.2.1 darwin/arm64"));
        let body_start = request.find("\r\n\r\n").unwrap() + 4;
        let provider_body: serde_json::Value =
            serde_json::from_str(&request[body_start..]).unwrap();
        assert_eq!(provider_body["project"], "project-wire");
        assert_eq!(provider_body["model"], "gemini-3-flash-agent");
        assert_eq!(provider_body["requestType"], "agent");
        assert!(provider_body["request"].get("safetySettings").is_none());
    }
}
