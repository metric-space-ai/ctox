// ref: internal/runtime/executor/claude_executor_execute.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::claude_executor::{
    ClaudeCredentialMode, ClaudeDeviceProfile, ClaudeMessagesRequest, ClaudeMessagesResponse,
    ClaudeMessagesStreamResponse, ClaudeMessagesStreamingTransport, ClaudeMessagesTransport,
    ClaudeMessagesTransportFailure, ClaudeTargetError, ClaudeUpstreamTarget, ClaudeUsageSink,
};
use super::claude_executor_auth::{
    ClaudePrepareAuthError, ClaudeRequestAuthPreparer, ClaudeSubscriptionAuth,
    ClaudeSubscriptionAuthError,
};
use super::claude_executor_buffered::{
    claude_buffered_response_message_id, claude_usage_from_stream_buffer,
    parse_claude_buffered_response_usage, prepare_claude_buffered_response,
    restore_claude_stream_tool_names,
};
use super::claude_executor_cloaking::{
    try_apply_claude_cloaking, ClaudeCallerSystemBlockError, ClaudeCloakPolicy,
};
use super::claude_executor_diagnostics::{
    begin_claude_diagnostics_request, commit_claude_diagnostics,
    inject_claude_diagnostics_with_state, observe_claude_stream_line,
    ClaudeDiagnosticsRequestState,
};
use super::claude_executor_request::{
    claude_request_uses_fast_mode, claude_requested_betas, extract_and_remove_claude_betas,
    prepare_claude_upstream_body_with_identity,
};
use super::claude_executor_tokens::prepare_claude_first_party_token_count_body;
use super::claude_executor_tool_state::{
    thread_alias_keys, ClaudeOAuthToolAliasStore, THREAD_NOT_FOUND_BODY,
};
use super::helps::{
    apply_claude_credential_metadata, claude_agent_session_uuid_for_request,
    detect_claude_code_request, normalize_codex_tool_integer_types_for_executor,
    observe_plugin_executor_stream, ClaudeCodeRequestDetection, ClaudeCredentialIdentityError,
    ClaudeDeviceProfileCache as ClaudeHelperDeviceProfileCache, ClaudeHeaderDefaults,
    ClaudeIdentityKvStore, ClaudeIdentityStoreError, SessionIdCache, SessionIdCacheError,
    StreamUsageBuffer,
};
use crate::internal::registry::lookup_model_info;
use crate::sdk::cliproxy::auth::{
    AccountCandidate, AccountExecutionResult, AccountRouter, AccountRoutingError,
    AccountSelectionError, Auth, CooldownConductor, UnauthorizedReplayDecision,
    UnauthorizedReplayState,
};
use crate::sdk::cliproxy::executor::{ExecutionMetadata, Headers, JsonMetadata};
use serde_json::Value;

pub trait AccountStateClock: Send + Sync {
    fn now_ms(&self) -> i64;
}

#[derive(Debug)]
struct SystemAccountStateClock;

impl AccountStateClock for SystemAccountStateClock {
    fn now_ms(&self) -> i64 {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        i64::try_from(millis).unwrap_or(i64::MAX)
    }
}

#[derive(Clone)]
struct AccountStateBinding {
    auth_id: String,
    conductor: Arc<CooldownConductor>,
    clock: Arc<dyn AccountStateClock>,
}

struct ClaudeRequestAuthPreparationBinding {
    auth: tokio::sync::Mutex<Auth>,
    preparer: Arc<ClaudeRequestAuthPreparer>,
}

/// Request-scoped native-client evidence prepared by the provider adapter.
/// It never mutates the account-owned executor and therefore cannot leak one
/// caller's headers, identity, or cloak decision into another request.
#[derive(Clone)]
pub struct ClaudeExecutionRequestContext {
    auth_id: String,
    headers: Headers,
    auth_metadata: BTreeMap<String, Value>,
    auth_attributes: BTreeMap<String, String>,
    detection: ClaudeCodeRequestDetection,
    session_id: String,
    client_user_agent: String,
    header_defaults: ClaudeHeaderDefaults,
}

struct ClaudeProviderRequestContextInput<'a> {
    auth_id: String,
    headers: Headers,
    original_payload: &'a [u8],
    translated_payload: &'a [u8],
    auth_metadata: BTreeMap<String, Value>,
    auth_attributes: BTreeMap<String, String>,
    request_metadata: &'a JsonMetadata,
}

impl fmt::Debug for ClaudeExecutionRequestContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeExecutionRequestContext")
            .field("auth_id", &self.auth_id)
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field(
                "auth_metadata_keys",
                &self.auth_metadata.keys().collect::<Vec<_>>(),
            )
            .field(
                "auth_attribute_keys",
                &self.auth_attributes.keys().collect::<Vec<_>>(),
            )
            .field("detection", &self.detection)
            .field("session_id", &"[REDACTED]")
            .field("client_user_agent", &self.client_user_agent)
            .field("header_defaults", &self.header_defaults)
            .finish()
    }
}

impl ClaudeExecutionRequestContext {
    pub fn from_provider_request(
        auth_id: impl Into<String>,
        headers: Headers,
        original_payload: &[u8],
        translated_payload: &[u8],
        auth_metadata: BTreeMap<String, Value>,
        auth_attributes: BTreeMap<String, String>,
    ) -> Self {
        Self::from_provider_request_with_metadata(
            auth_id,
            headers,
            original_payload,
            translated_payload,
            auth_metadata,
            auth_attributes,
            &JsonMetadata::new(),
        )
    }

    pub fn from_provider_request_with_metadata(
        auth_id: impl Into<String>,
        headers: Headers,
        original_payload: &[u8],
        translated_payload: &[u8],
        auth_metadata: BTreeMap<String, Value>,
        auth_attributes: BTreeMap<String, String>,
        request_metadata: &JsonMetadata,
    ) -> Self {
        Self::from_provider_request_kind(
            ClaudeProviderRequestContextInput {
                auth_id: auth_id.into(),
                headers,
                original_payload,
                translated_payload,
                auth_metadata,
                auth_attributes,
                request_metadata,
            },
            false,
        )
    }

    pub fn from_provider_count_tokens_request(
        auth_id: impl Into<String>,
        headers: Headers,
        original_payload: &[u8],
        translated_payload: &[u8],
        auth_metadata: BTreeMap<String, Value>,
        auth_attributes: BTreeMap<String, String>,
    ) -> Self {
        Self::from_provider_count_tokens_request_with_metadata(
            auth_id,
            headers,
            original_payload,
            translated_payload,
            auth_metadata,
            auth_attributes,
            &JsonMetadata::new(),
        )
    }

    pub fn from_provider_count_tokens_request_with_metadata(
        auth_id: impl Into<String>,
        headers: Headers,
        original_payload: &[u8],
        translated_payload: &[u8],
        auth_metadata: BTreeMap<String, Value>,
        auth_attributes: BTreeMap<String, String>,
        request_metadata: &JsonMetadata,
    ) -> Self {
        Self::from_provider_request_kind(
            ClaudeProviderRequestContextInput {
                auth_id: auth_id.into(),
                headers,
                original_payload,
                translated_payload,
                auth_metadata,
                auth_attributes,
                request_metadata,
            },
            true,
        )
    }

    fn from_provider_request_kind(
        input: ClaudeProviderRequestContextInput<'_>,
        count_tokens: bool,
    ) -> Self {
        let header_defaults = claude_header_defaults(&input.auth_metadata, &input.auth_attributes);
        let detection = detect_claude_code_request(
            Some(&input.headers),
            input.original_payload,
            count_tokens,
            &header_defaults,
        );
        let execution_metadata = claude_execution_metadata(input.request_metadata);
        let session_id = claude_agent_session_uuid_for_request(
            Some(&input.headers),
            input.original_payload,
            input.translated_payload,
            detection.confirmed,
            &[&execution_metadata],
        );
        Self::new(
            input.auth_id,
            input.headers,
            input.auth_metadata,
            input.auth_attributes,
            detection,
            session_id,
        )
    }

    pub fn new(
        auth_id: impl Into<String>,
        headers: Headers,
        auth_metadata: BTreeMap<String, Value>,
        auth_attributes: BTreeMap<String, String>,
        detection: ClaudeCodeRequestDetection,
        session_id: impl Into<String>,
    ) -> Self {
        let client_user_agent = header_value(&headers, "User-Agent");
        let header_defaults = claude_header_defaults(&auth_metadata, &auth_attributes);
        Self {
            auth_id: auth_id.into(),
            headers,
            auth_metadata,
            auth_attributes,
            detection,
            session_id: session_id.into(),
            client_user_agent,
            header_defaults,
        }
    }
}

fn claude_execution_metadata(metadata: &JsonMetadata) -> ExecutionMetadata {
    let value = |key: &str| {
        metadata
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    ExecutionMetadata {
        execution_session_id: value("execution_session_id"),
        derived_session_id: value("derived_session_id"),
        ..ExecutionMetadata::default()
    }
}

fn header_value(headers: &Headers, name: &str) -> String {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(|value| value.trim().to_owned())
        .unwrap_or_default()
}

pub(super) fn claude_header_defaults(
    metadata: &BTreeMap<String, Value>,
    attributes: &BTreeMap<String, String>,
) -> ClaudeHeaderDefaults {
    let value = |key: &str| {
        attributes
            .get(key)
            .map(String::as_str)
            .or_else(|| metadata.get(key).and_then(Value::as_str))
            .map(str::trim)
            .unwrap_or_default()
            .to_owned()
    };
    ClaudeHeaderDefaults {
        user_agent: value("claude_header_user_agent"),
        package_version: value("claude_header_package_version"),
        runtime_version: value("claude_header_runtime_version"),
        os: value("claude_header_os"),
        arch: value("claude_header_arch"),
        stabilize_device_profile: None,
    }
}

/// Reserved Codex client fields become integers before OAuth tool-name remap.
/// Remap changes the tool name and would hide `exec_command` from the matcher.
/// A missing request context has no client headers, so the body stays unchanged.
fn normalize_claude_codex_integer_schemas(
    body: &[u8],
    context: Option<&ClaudeExecutionRequestContext>,
) -> Vec<u8> {
    let Some(context) = context else {
        return body.to_vec();
    };
    normalize_codex_tool_integer_types_for_executor(&body, &context.headers, "claude")
}

/// Bounded Claude subscription execution path with exactly one unauthorized
/// refresh/replay.
pub struct ClaudeSubscriptionMessagesExecutor {
    auth: Arc<ClaudeSubscriptionAuth>,
    transport: Arc<dyn ClaudeMessagesTransport>,
    stream_transport: Option<Arc<dyn ClaudeMessagesStreamingTransport>>,
    timeout: Duration,
    account_state: Option<AccountStateBinding>,
    device_profile: Option<ClaudeDeviceProfile>,
    device_profiles: Arc<ClaudeHelperDeviceProfileCache>,
    session_ids: Arc<SessionIdCache>,
    session_id_store: Option<Arc<dyn ClaudeIdentityKvStore>>,
    cloak_policy: ClaudeCloakPolicy,
    cloak_user_id: String,
    usage_sink: Option<Arc<dyn ClaudeUsageSink>>,
    request_auth_preparation: Option<Arc<ClaudeRequestAuthPreparationBinding>>,
    oauth_tool_aliases: Arc<ClaudeOAuthToolAliasStore>,
}

impl ClaudeSubscriptionMessagesExecutor {
    pub fn new(
        auth: Arc<ClaudeSubscriptionAuth>,
        transport: Arc<dyn ClaudeMessagesTransport>,
        timeout: Duration,
    ) -> Self {
        Self {
            auth,
            transport,
            stream_transport: None,
            timeout,
            account_state: None,
            device_profile: None,
            device_profiles: Arc::new(ClaudeHelperDeviceProfileCache::new()),
            session_ids: Arc::new(SessionIdCache::new()),
            session_id_store: None,
            cloak_policy: ClaudeCloakPolicy::oauth_default(),
            cloak_user_id: super::helps::generate_fake_user_id(),
            usage_sink: None,
            request_auth_preparation: None,
            oauth_tool_aliases: Arc::new(ClaudeOAuthToolAliasStore::default()),
        }
    }

    pub fn with_account_state(
        self,
        auth_id: impl Into<String>,
        conductor: Arc<CooldownConductor>,
    ) -> Result<Self, ClaudeExecutionError> {
        self.with_account_state_clock(auth_id, conductor, Arc::new(SystemAccountStateClock))
    }

    pub fn with_account_state_clock(
        mut self,
        auth_id: impl Into<String>,
        conductor: Arc<CooldownConductor>,
        clock: Arc<dyn AccountStateClock>,
    ) -> Result<Self, ClaudeExecutionError> {
        let auth_id = auth_id.into();
        if auth_id.trim().is_empty() {
            return Err(ClaudeExecutionError::AccountStateConfiguration);
        }
        self.account_state = Some(AccountStateBinding {
            auth_id,
            conductor,
            clock,
        });
        Ok(self)
    }

    pub fn account_state_auth_id(&self) -> Option<&str> {
        self.account_state
            .as_ref()
            .map(|binding| binding.auth_id.as_str())
    }

    pub fn with_device_profile(mut self, profile: ClaudeDeviceProfile) -> Self {
        self.device_profile = Some(profile);
        self
    }

    pub fn with_cloak_policy(mut self, policy: ClaudeCloakPolicy) -> Self {
        self.cloak_policy = policy;
        self
    }

    pub fn with_usage_sink(mut self, sink: Arc<dyn ClaudeUsageSink>) -> Self {
        self.usage_sink = Some(sink);
        self
    }

    /// Activates Candidate request-auth preparation on the specialized CTOX
    /// pool without creating a second credential authority. The binding keeps
    /// only non-secret account/device metadata; access tokens remain owned by
    /// `ClaudeSubscriptionAuth` and are borrowed for preparation.
    pub fn with_request_auth_preparer(
        mut self,
        auth_id: impl Into<String>,
        preparer: Arc<ClaudeRequestAuthPreparer>,
    ) -> Result<Self, ClaudeExecutionError> {
        let auth_id = auth_id.into();
        if auth_id.trim().is_empty() {
            return Err(ClaudeExecutionError::AccountStateConfiguration);
        }
        let mut auth = Auth::default();
        auth.id = auth_id;
        auth.provider = "claude".to_owned();
        self.request_auth_preparation = Some(Arc::new(ClaudeRequestAuthPreparationBinding {
            auth: tokio::sync::Mutex::new(auth),
            preparer,
        }));
        Ok(self)
    }

    pub fn with_stream_transport(
        mut self,
        transport: Arc<dyn ClaudeMessagesStreamingTransport>,
    ) -> Self {
        self.stream_transport = Some(transport);
        self
    }

    /// Injects the authority that scopes Claude Code session IDs. Sharing the
    /// cache preserves upstream process-wide reuse without a mutable global;
    /// an optional durable store preserves reuse across executor instances.
    pub fn with_session_id_authority(
        mut self,
        cache: Arc<SessionIdCache>,
        store: Option<Arc<dyn ClaudeIdentityKvStore>>,
    ) -> Self {
        self.session_ids = cache;
        self.session_id_store = store;
        self
    }

    pub async fn execute(
        &self,
        target: ClaudeUpstreamTarget,
        body: Vec<u8>,
        stream: bool,
    ) -> Result<ClaudeExecutionOutcome, ClaudeExecutionError> {
        self.execute_for_model(target, None, body, stream).await
    }

    pub async fn prepare_first_party_count_tokens_request(
        &self,
        target: ClaudeUpstreamTarget,
        model: &str,
        body: Vec<u8>,
        context: Option<&ClaudeExecutionRequestContext>,
    ) -> Result<ClaudeMessagesRequest, ClaudeExecutionError> {
        let credentials = self.auth.load().await.map_err(ClaudeExecutionError::Auth)?;
        let _prepared_auth = self
            .prepare_request_auth(credentials.access_token().expose_secret())
            .await?;
        let session_id =
            self.resolve_session_id(context, credentials.access_token().expose_secret())?;
        let mut cloak_policy = self.cloak_policy.clone();
        if let Some(context) = context {
            cloak_policy.verified_claude_code = context.detection.confirmed;
            cloak_policy
                .client_user_agent
                .clone_from(&context.client_user_agent);
            if !context.detection.entrypoint.is_empty() {
                cloak_policy
                    .entrypoint
                    .clone_from(&context.detection.entrypoint);
            }
        }
        let prepared = prepare_claude_first_party_token_count_body(
            &body,
            model,
            &cloak_policy,
            credentials.access_token().expose_secret(),
        )
        .map_err(ClaudeExecutionError::CallerSystemBlock)?;
        let mut request = ClaudeMessagesRequest::new_with_session(
            target,
            ClaudeCredentialMode::OAuth,
            credentials.access_token(),
            prepared.body,
            false,
            session_id,
        )
        .map_err(ClaudeExecutionError::Request)?
        .with_upstream_metadata(prepared.requested_betas, HashMap::new());
        if let Some(profile) =
            self.resolve_device_profile(context, credentials.access_token().expose_secret())?
        {
            request = request
                .with_device_profile(profile)
                .map_err(ClaudeExecutionError::Request)?;
        }
        Ok(request)
    }

    /// Executes first-party token counting through the account's native
    /// Messages transport. Credential refresh and account outcome persistence
    /// deliberately mirror Messages, while the transport selects the distinct
    /// count-token endpoint and measured header order.
    pub async fn execute_count_tokens_for_model_with_context(
        &self,
        target: ClaudeUpstreamTarget,
        model: &str,
        body: Vec<u8>,
        context: Option<&ClaudeExecutionRequestContext>,
    ) -> Result<ClaudeExecutionOutcome, ClaudeExecutionError> {
        if !target.is_anthropic_api() {
            return Err(ClaudeExecutionError::Request(ClaudeTargetError::Invalid));
        }
        let request = self
            .prepare_first_party_count_tokens_request(target, model, body, context)
            .await?;
        let first = self
            .transport
            .execute_count_tokens(&request, self.timeout)
            .await
            .map_err(ClaudeExecutionError::Transport)?;
        let mut replay = UnauthorizedReplayState::default();
        if replay.observe(first.status(), true) != UnauthorizedReplayDecision::RefreshAndReplay {
            let state_persisted = self.record_account_outcome(Some(model), &first).await;
            return Ok(ClaudeExecutionOutcome::new(
                first,
                replay,
                state_persisted,
                false,
            ));
        }

        crate::internal::api::account_selection::record_upstream_status(401, &[]);
        let refreshed = self
            .auth
            .refresh_after_status(401)
            .await
            .map_err(ClaudeExecutionError::Auth)?;
        let retry = request
            .retry_with_credential(refreshed.credentials().access_token())
            .map_err(ClaudeExecutionError::Request)?;
        let response = self
            .transport
            .execute_count_tokens(&retry, self.timeout)
            .await
            .map_err(ClaudeExecutionError::Transport)?;
        let _ = replay.observe(response.status(), true);
        let state_persisted = self.record_account_outcome(Some(model), &response).await;
        Ok(ClaudeExecutionOutcome::new(
            response,
            replay,
            state_persisted,
            false,
        ))
    }

    pub async fn execute_for_model(
        &self,
        target: ClaudeUpstreamTarget,
        model: Option<&str>,
        body: Vec<u8>,
        stream: bool,
    ) -> Result<ClaudeExecutionOutcome, ClaudeExecutionError> {
        self.execute_for_model_with_context(target, model, body, stream, None)
            .await
    }

    pub async fn execute_for_model_with_context(
        &self,
        target: ClaudeUpstreamTarget,
        model: Option<&str>,
        body: Vec<u8>,
        stream: bool,
        context: Option<&ClaudeExecutionRequestContext>,
    ) -> Result<ClaudeExecutionOutcome, ClaudeExecutionError> {
        let credentials = self.auth.load().await.map_err(ClaudeExecutionError::Auth)?;
        let prepared_auth = self
            .prepare_request_auth(credentials.access_token().expose_secret())
            .await?;
        let session_id =
            self.resolve_session_id(context, credentials.access_token().expose_secret())?;
        let fast_request = body_requests_fast_mode(&body);
        let model_info = model.and_then(|model| lookup_model_info(model, "claude"));
        let mut cloak_policy = self.cloak_policy.clone();
        if let Some(context) = context {
            cloak_policy.verified_claude_code = context.detection.confirmed;
            cloak_policy
                .client_user_agent
                .clone_from(&context.client_user_agent);
            if !context.detection.entrypoint.is_empty() {
                cloak_policy
                    .entrypoint
                    .clone_from(&context.detection.entrypoint);
            }
        }
        let cloaked = cloak_policy.should_cloak_request();
        let credential_identity = self
            .account_state_auth_id()
            .unwrap_or_else(|| credentials.access_token().expose_secret());
        let diagnostics_state = if cloaked && target.is_anthropic_api() {
            begin_claude_diagnostics_request(credential_identity, &session_id)
        } else {
            ClaudeDiagnosticsRequestState::default()
        };
        if cloaked && target.is_anthropic_api() {
            // ref: internal/runtime/executor/claude_executor_cloaking.go:214-244 @ 2044a01f
            cloak_policy.current_date =
                Some(diagnostics_state.pin_date(&cloak_policy.resolved_current_date()));
        }
        let body = try_apply_claude_cloaking(
            &body,
            model.unwrap_or_default(),
            &cloak_policy,
            Some(&self.cloak_user_id),
        )
        .map_err(ClaudeExecutionError::CallerSystemBlock)?;
        let (body, diagnostics_state) = if cloaked && target.is_anthropic_api() {
            inject_claude_diagnostics_with_state(&body, diagnostics_state)
        } else {
            (body, diagnostics_state)
        };

        let body = self.apply_request_credential_identity(
            context,
            prepared_auth.as_ref(),
            &body,
            &session_id,
        )?;
        let body = normalize_claude_codex_integer_schemas(&body, context);
        // ref: claude_executor_tool_state.go:95-122 @ 16d98881
        // Resolve before tool registry augmentation; empty declarations need
        // the previous message's aliases, including a known empty mapping.
        let alias_state_active = cloaked && target.is_anthropic_api();
        let continuation_aliases = if alias_state_active {
            match self.oauth_tool_aliases.resolve(&body) {
                Ok(aliases) => aliases,
                Err(()) => {
                    return Ok(ClaudeExecutionOutcome::new(
                        ClaudeMessagesResponse::new(404, THREAD_NOT_FOUND_BODY.to_vec()),
                        UnauthorizedReplayState::default(),
                        Some(true),
                        true,
                    ));
                }
            }
        } else {
            None
        };
        let thread_alias_keys = alias_state_active
            .then(|| thread_alias_keys(&body))
            .flatten();
        let (body, betas, reverse_map) = prepare_claude_upstream_body_with_identity(
            &body,
            model_info.as_ref(),
            credentials.access_token().expose_secret(),
            true,
        );
        let reverse_map = continuation_aliases.unwrap_or(reverse_map);
        let mut request = ClaudeMessagesRequest::new_with_session(
            target,
            ClaudeCredentialMode::OAuth,
            credentials.access_token(),
            body,
            stream,
            session_id,
        )
        .map_err(ClaudeExecutionError::Request)?
        .with_upstream_metadata(betas, reverse_map);
        if let Some(profile) =
            self.resolve_device_profile(context, credentials.access_token().expose_secret())?
        {
            request = request
                .with_device_profile(profile)
                .map_err(ClaudeExecutionError::Request)?;
        }
        let first = prepare_claude_buffered_response(
            self.transport
                .execute(&request, self.timeout)
                .await
                .map_err(ClaudeExecutionError::Transport)?,
            &request,
        );
        let mut replay = UnauthorizedReplayState::default();
        if replay.observe(first.status(), true) != UnauthorizedReplayDecision::RefreshAndReplay {
            self.publish_usage(model, &first, request.stream());
            if (200..300).contains(&first.status()) {
                let message_id =
                    claude_buffered_response_message_id(first.body(), request.stream());
                if !request.stream() || !message_id.is_empty() {
                    commit_claude_diagnostics(&diagnostics_state, &message_id);
                    self.oauth_tool_aliases.remember(
                        thread_alias_keys.as_deref(),
                        request.tool_name_reverse_map(),
                        &message_id,
                    );
                }
            }
            let request_scoped = (fast_request && !(200..300).contains(&first.status()))
                || crate::internal::clienterror::is_claude_thread_not_found(
                    first.status(),
                    first.body(),
                );
            let state_persisted = if request_scoped {
                Some(true)
            } else {
                self.record_account_outcome(model, &first).await
            };
            return Ok(ClaudeExecutionOutcome::new(
                first,
                replay,
                state_persisted,
                request_scoped,
            ));
        }

        crate::internal::api::account_selection::record_upstream_status(401, &[]);
        let refreshed = self
            .auth
            .refresh_after_status(401)
            .await
            .map_err(ClaudeExecutionError::Auth)?;
        let retry = request
            .retry_with_credential(refreshed.credentials().access_token())
            .map_err(ClaudeExecutionError::Request)?;
        let response = prepare_claude_buffered_response(
            self.transport
                .execute(&retry, self.timeout)
                .await
                .map_err(ClaudeExecutionError::Transport)?,
            &retry,
        );
        let _ = replay.observe(response.status(), true);
        self.publish_usage(model, &response, retry.stream());
        if (200..300).contains(&response.status()) {
            let message_id = claude_buffered_response_message_id(response.body(), retry.stream());
            if !retry.stream() || !message_id.is_empty() {
                commit_claude_diagnostics(&diagnostics_state, &message_id);
                self.oauth_tool_aliases.remember(
                    thread_alias_keys.as_deref(),
                    retry.tool_name_reverse_map(),
                    &message_id,
                );
            }
        }
        let request_scoped = (fast_request && !(200..300).contains(&response.status()))
            || crate::internal::clienterror::is_claude_thread_not_found(
                response.status(),
                response.body(),
            );
        let state_persisted = if request_scoped {
            Some(true)
        } else {
            self.record_account_outcome(model, &response).await
        };
        Ok(ClaudeExecutionOutcome::new(
            response,
            replay,
            state_persisted,
            request_scoped,
        ))
    }

    pub async fn execute_stream_for_model(
        &self,
        target: ClaudeUpstreamTarget,
        model: Option<&str>,
        body: Vec<u8>,
    ) -> Result<ClaudeStreamExecutionOutcome, ClaudeExecutionError> {
        self.execute_stream_for_model_with_context(target, model, body, None)
            .await
    }

    pub async fn execute_stream_for_model_with_context(
        &self,
        target: ClaudeUpstreamTarget,
        model: Option<&str>,
        body: Vec<u8>,
        context: Option<&ClaudeExecutionRequestContext>,
    ) -> Result<ClaudeStreamExecutionOutcome, ClaudeExecutionError> {
        let transport = self
            .stream_transport
            .as_ref()
            .ok_or(ClaudeExecutionError::StreamingUnavailable)?;
        let credentials = self.auth.load().await.map_err(ClaudeExecutionError::Auth)?;
        let prepared_auth = self
            .prepare_request_auth(credentials.access_token().expose_secret())
            .await?;
        let session_id =
            self.resolve_session_id(context, credentials.access_token().expose_secret())?;
        let fast_request = body_requests_fast_mode(&body);
        let model_info = model.and_then(|model| lookup_model_info(model, "claude"));
        let mut cloak_policy = self.cloak_policy.clone();
        if let Some(context) = context {
            cloak_policy.verified_claude_code = context.detection.confirmed;
            cloak_policy
                .client_user_agent
                .clone_from(&context.client_user_agent);
            if !context.detection.entrypoint.is_empty() {
                cloak_policy
                    .entrypoint
                    .clone_from(&context.detection.entrypoint);
            }
        }
        let cloaked = cloak_policy.should_cloak_request();
        let credential_identity = self
            .account_state_auth_id()
            .unwrap_or_else(|| credentials.access_token().expose_secret());
        let diagnostics_state = if cloaked && target.is_anthropic_api() {
            begin_claude_diagnostics_request(credential_identity, &session_id)
        } else {
            ClaudeDiagnosticsRequestState::default()
        };
        if cloaked && target.is_anthropic_api() {
            // ref: internal/runtime/executor/claude_executor_cloaking.go:214-244 @ 2044a01f
            cloak_policy.current_date =
                Some(diagnostics_state.pin_date(&cloak_policy.resolved_current_date()));
        }
        let body = try_apply_claude_cloaking(
            &body,
            model.unwrap_or_default(),
            &cloak_policy,
            Some(&self.cloak_user_id),
        )
        .map_err(ClaudeExecutionError::CallerSystemBlock)?;
        let (body, diagnostics_state) = if cloaked && target.is_anthropic_api() {
            inject_claude_diagnostics_with_state(&body, diagnostics_state)
        } else {
            (body, diagnostics_state)
        };

        let body = self.apply_request_credential_identity(
            context,
            prepared_auth.as_ref(),
            &body,
            &session_id,
        )?;
        let body = normalize_claude_codex_integer_schemas(&body, context);
        // ref: claude_executor_tool_state.go:95-122 @ 16d98881
        // Resolve before tool registry augmentation; empty declarations need
        // the previous message's aliases, including a known empty mapping.
        let alias_state_active = cloaked && target.is_anthropic_api();
        let continuation_aliases = if alias_state_active {
            match self.oauth_tool_aliases.resolve(&body) {
                Ok(aliases) => aliases,
                Err(()) => {
                    return Ok(self.missing_thread_stream_outcome(model, diagnostics_state));
                }
            }
        } else {
            None
        };
        let thread_alias_keys = alias_state_active
            .then(|| thread_alias_keys(&body))
            .flatten();
        let (body, betas, reverse_map) = prepare_claude_upstream_body_with_identity(
            &body,
            model_info.as_ref(),
            credentials.access_token().expose_secret(),
            true,
        );
        let reverse_map = continuation_aliases.unwrap_or(reverse_map);
        let mut request = ClaudeMessagesRequest::new_with_session(
            target,
            ClaudeCredentialMode::OAuth,
            credentials.access_token(),
            body,
            true,
            session_id,
        )
        .map_err(ClaudeExecutionError::Request)?
        .with_upstream_metadata(betas, reverse_map);
        if let Some(profile) =
            self.resolve_device_profile(context, credentials.access_token().expose_secret())?
        {
            request = request
                .with_device_profile(profile)
                .map_err(ClaudeExecutionError::Request)?;
        }
        let mut response = transport
            .execute_stream(&request, self.timeout)
            .await
            .map_err(ClaudeExecutionError::Transport)?;
        let mut replay = UnauthorizedReplayState::default();
        if replay.observe(response.status(), true) == UnauthorizedReplayDecision::RefreshAndReplay {
            let refreshed = self
                .auth
                .refresh_after_status(401)
                .await
                .map_err(ClaudeExecutionError::Auth)?;
            let retry = request
                .retry_with_credential(refreshed.credentials().access_token())
                .map_err(ClaudeExecutionError::Request)?;
            response = transport
                .execute_stream(&retry, self.timeout)
                .await
                .map_err(ClaudeExecutionError::Transport)?;
            let _ = replay.observe(response.status(), true);
        }

        if (200..300).contains(&response.status())
            && response.bootstrap_message_start().await.is_err()
        {
            response = ClaudeMessagesStreamResponse::synthetic(502);
        }
        crate::internal::api::account_selection::record_upstream_status(
            response.status(),
            response.error_body(),
        );
        let request_scoped = (fast_request && !(200..300).contains(&response.status()))
            || crate::internal::clienterror::is_claude_thread_not_found(
                response.status(),
                response.error_body(),
            );
        let state_persisted = if request_scoped {
            Some(true)
        } else {
            self.record_account_status(model, response.status(), response.retry_after())
                .await
        };
        Ok(ClaudeStreamExecutionOutcome {
            response,
            attempts: replay.attempts(),
            refreshed: replay.refreshed(),
            state_persisted,
            failure_binding: self.account_state.clone(),
            model: model.map(str::to_owned),
            tool_name_reverse_map: request.tool_name_reverse_map().clone(),
            usage_sink: self.usage_sink.clone(),
            diagnostics_state,
            request_scoped,
            oauth_tool_aliases: self.oauth_tool_aliases.clone(),
            thread_alias_keys,
        })
    }

    fn missing_thread_stream_outcome(
        &self,
        model: Option<&str>,
        diagnostics_state: ClaudeDiagnosticsRequestState,
    ) -> ClaudeStreamExecutionOutcome {
        let replay = UnauthorizedReplayState::default();
        ClaudeStreamExecutionOutcome {
            response: ClaudeMessagesStreamResponse::synthetic(404)
                .with_error_body(THREAD_NOT_FOUND_BODY.to_vec()),
            attempts: replay.attempts(),
            refreshed: replay.refreshed(),
            state_persisted: Some(true),
            failure_binding: None,
            model: model.map(str::to_owned),
            tool_name_reverse_map: HashMap::new(),
            usage_sink: None,
            diagnostics_state,
            request_scoped: true,
            oauth_tool_aliases: self.oauth_tool_aliases.clone(),
            thread_alias_keys: None,
        }
    }

    fn resolve_session_id(
        &self,
        context: Option<&ClaudeExecutionRequestContext>,
        access_token: &str,
    ) -> Result<String, ClaudeExecutionError> {
        if let Some(session_id) = context
            .map(|context| context.session_id.trim())
            .filter(|session_id| !session_id.is_empty())
        {
            return Ok(session_id.to_owned());
        }
        self.session_ids
            .cached_session_id_required(self.session_id_store.as_deref(), access_token)
            .map_err(ClaudeExecutionError::SessionId)
    }

    async fn prepare_request_auth(
        &self,
        access_token: &str,
    ) -> Result<Option<Auth>, ClaudeExecutionError> {
        let Some(binding) = self.request_auth_preparation.as_ref() else {
            return Ok(None);
        };
        let mut auth = binding.auth.lock().await;
        binding
            .preparer
            .prepare_with_access_token(&mut auth, access_token)
            .await
            .map_err(ClaudeExecutionError::PrepareAuth)?;
        Ok(Some(auth.clone()))
    }

    fn apply_request_credential_identity(
        &self,
        context: Option<&ClaudeExecutionRequestContext>,
        prepared_auth: Option<&Auth>,
        body: &[u8],
        session_id: &str,
    ) -> Result<Vec<u8>, ClaudeExecutionError> {
        if context.is_none() && prepared_auth.is_none() {
            return Ok(body.to_vec());
        }
        let mut auth = prepared_auth.cloned().unwrap_or_default();
        if let Some(context) = context {
            auth.id.clone_from(&context.auth_id);
            auth.provider = "claude".to_owned();
            auth.metadata.extend(context.auth_metadata.clone());
            auth.attributes.extend(context.auth_attributes.clone());
        }
        if auth.metadata.is_empty() && auth.attributes.is_empty() {
            return Ok(body.to_vec());
        }
        apply_claude_credential_metadata(body, &mut auth, session_id)
            .map(|(body, _)| body)
            .map_err(ClaudeExecutionError::CredentialIdentity)
    }

    fn resolve_device_profile(
        &self,
        context: Option<&ClaudeExecutionRequestContext>,
        access_token: &str,
    ) -> Result<Option<ClaudeDeviceProfile>, ClaudeExecutionError> {
        if self.device_profile.is_some() {
            return Ok(self.device_profile.clone());
        }
        let Some(context) = context else {
            return Ok(None);
        };
        let profile = self
            .device_profiles
            .resolve_required(
                None,
                Some(&context.auth_id),
                access_token,
                Some(&context.headers),
                &context.header_defaults,
            )
            .map_err(ClaudeExecutionError::IdentityStore)?;
        ClaudeDeviceProfile::new(
            profile.user_agent,
            profile.package_version,
            profile.runtime_version,
            profile.os,
            profile.arch,
        )
        .map(Some)
        .map_err(ClaudeExecutionError::Request)
    }

    async fn record_account_outcome(
        &self,
        model: Option<&str>,
        response: &ClaudeMessagesResponse,
    ) -> Option<bool> {
        crate::internal::api::account_selection::record_upstream_status(
            response.status(),
            response.body(),
        );
        self.record_account_status(model, response.status(), response.retry_after())
            .await
    }

    fn publish_usage(&self, model: Option<&str>, response: &ClaudeMessagesResponse, stream: bool) {
        if !(200..300).contains(&response.status()) {
            return;
        }
        if let (Some(sink), Some(usage)) = (
            &self.usage_sink,
            parse_claude_buffered_response_usage(response.body(), stream),
        ) {
            sink.publish(model, usage);
        }
    }

    async fn record_account_status(
        &self,
        model: Option<&str>,
        status: u16,
        retry_after: Option<Duration>,
    ) -> Option<bool> {
        let binding = self.account_state.as_ref()?;
        let conductor = Arc::clone(&binding.conductor);
        let result = AccountExecutionResult {
            provider: "claude".to_owned(),
            auth_id: binding.auth_id.clone(),
            model: model.map(str::to_owned),
            status,
            retry_delay_ms: retry_after
                .map(|delay| u64::try_from(delay.as_millis()).unwrap_or(u64::MAX)),
            observed_at_ms: binding.clock.now_ms(),
        };
        Some(
            tokio::task::spawn_blocking(move || conductor.record(result))
                .await
                .is_ok_and(|result| result.is_ok()),
        )
    }
}

fn body_requests_fast_mode(body: &[u8]) -> bool {
    let (extra, body_without_betas) = extract_and_remove_claude_betas(body);
    claude_request_uses_fast_mode(&body_without_betas, &claude_requested_betas("", &extra))
}

impl fmt::Debug for ClaudeSubscriptionMessagesExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeSubscriptionMessagesExecutor")
            .field("auth", &"[REDACTED]")
            .field("transport", &"ClaudeMessagesTransport")
            .field(
                "stream_transport",
                &self.stream_transport.as_ref().map(|_| "attached"),
            )
            .field("session_ids", &self.session_ids)
            .field("durable_session_ids", &self.session_id_store.is_some())
            .field("timeout", &self.timeout)
            .field(
                "account_state",
                &self.account_state.as_ref().map(|_| "attached"),
            )
            .field("device_profile", &self.device_profile)
            .field("request_device_profiles", &"attached")
            .field("cloak_policy", &self.cloak_policy)
            .field("usage_sink", &self.usage_sink.as_ref().map(|_| "attached"))
            .finish()
    }
}

pub struct ClaudeExecutionOutcome {
    response: ClaudeMessagesResponse,
    attempts: u8,
    refreshed: bool,
    state_persisted: Option<bool>,
    request_scoped: bool,
}

pub struct ClaudeStreamExecutionOutcome {
    response: ClaudeMessagesStreamResponse,
    attempts: u8,
    refreshed: bool,
    state_persisted: Option<bool>,
    failure_binding: Option<AccountStateBinding>,
    model: Option<String>,
    tool_name_reverse_map: HashMap<String, String>,
    usage_sink: Option<Arc<dyn ClaudeUsageSink>>,
    diagnostics_state: ClaudeDiagnosticsRequestState,
    request_scoped: bool,
    oauth_tool_aliases: Arc<ClaudeOAuthToolAliasStore>,
    thread_alias_keys: Option<Vec<String>>,
}

impl ClaudeStreamExecutionOutcome {
    pub fn response(&self) -> &ClaudeMessagesStreamResponse {
        &self.response
    }

    pub fn response_mut(&mut self) -> &mut ClaudeMessagesStreamResponse {
        &mut self.response
    }

    pub fn into_response(self) -> ClaudeTrackedMessagesStreamResponse {
        ClaudeTrackedMessagesStreamResponse {
            response: self.response,
            failure_binding: self.failure_binding,
            model: self.model,
            failure_recorded: false,
            tool_name_reverse_map: self.tool_name_reverse_map,
            usage_sink: self.usage_sink,
            stream_line_buffer: Vec::new(),
            stream_eof: false,
            pending_transport_failure: None,
            usage_buffer: StreamUsageBuffer::default(),
            usage_published: false,
            diagnostics_message_id: String::new(),
            diagnostics_completed: false,
            diagnostics_committed: false,
            diagnostics_state: self.diagnostics_state,
            oauth_tool_aliases: self.oauth_tool_aliases,
            thread_alias_keys: self.thread_alias_keys,
        }
    }

    pub fn attempts(&self) -> u8 {
        self.attempts
    }

    pub fn refreshed(&self) -> bool {
        self.refreshed
    }

    pub fn state_persisted(&self) -> Option<bool> {
        self.state_persisted
    }

    pub fn request_scoped(&self) -> bool {
        self.request_scoped
    }
}

impl fmt::Debug for ClaudeStreamExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeStreamExecutionOutcome")
            .field("response", &self.response)
            .field("attempts", &self.attempts)
            .field("refreshed", &self.refreshed)
            .field("state_persisted", &self.state_persisted)
            .field(
                "failure_binding",
                &self.failure_binding.as_ref().map(|_| "attached"),
            )
            .finish()
    }
}

pub struct ClaudeTrackedMessagesStreamResponse {
    response: ClaudeMessagesStreamResponse,
    failure_binding: Option<AccountStateBinding>,
    model: Option<String>,
    failure_recorded: bool,
    tool_name_reverse_map: HashMap<String, String>,
    stream_line_buffer: Vec<u8>,
    stream_eof: bool,
    pending_transport_failure: Option<ClaudeMessagesTransportFailure>,
    usage_buffer: StreamUsageBuffer,
    usage_published: bool,
    usage_sink: Option<Arc<dyn ClaudeUsageSink>>,
    diagnostics_state: ClaudeDiagnosticsRequestState,
    diagnostics_message_id: String,
    diagnostics_completed: bool,
    diagnostics_committed: bool,
    oauth_tool_aliases: Arc<ClaudeOAuthToolAliasStore>,
    thread_alias_keys: Option<Vec<String>>,
}

impl ClaudeTrackedMessagesStreamResponse {
    pub fn status(&self) -> u16 {
        self.response.status()
    }

    pub async fn next_chunk(&mut self) -> Option<Result<Vec<u8>, ClaudeMessagesTransportFailure>> {
        loop {
            if self.diagnostics_completed {
                self.stream_line_buffer.clear();
                self.pending_transport_failure = None;
                self.stream_eof = true;
                self.finish_stream_usage();
                return None;
            }
            if let Some(boundary) = complete_sse_frame_len(&self.stream_line_buffer) {
                let frame: Vec<u8> = self.stream_line_buffer.drain(..boundary).collect();
                observe_plugin_executor_stream("claude", &frame, &mut self.usage_buffer);
                self.observe_diagnostics(&frame);
                return Some(Ok(restore_claude_stream_tool_names(
                    &frame,
                    &self.tool_name_reverse_map,
                )));
            }
            if self.stream_eof {
                if self.stream_line_buffer.is_empty() {
                    self.finish_stream_usage();
                    if let Some(error) = self.pending_transport_failure.take() {
                        if error != ClaudeMessagesTransportFailure::Cancelled {
                            self.record_terminal_failure().await;
                        }
                        return Some(Err(error));
                    }
                    return None;
                }
                let line = std::mem::take(&mut self.stream_line_buffer);
                observe_plugin_executor_stream("claude", &line, &mut self.usage_buffer);
                self.observe_diagnostics(&line);
                return Some(Ok(restore_claude_stream_tool_names(
                    &line,
                    &self.tool_name_reverse_map,
                )));
            }
            match self.response.next_chunk().await {
                Some(Ok(chunk)) => self.stream_line_buffer.extend_from_slice(&chunk),
                Some(Err(error)) => {
                    self.stream_eof = true;
                    self.pending_transport_failure = Some(error);
                }
                None => self.stream_eof = true,
            }
        }
    }

    fn finish_stream_usage(&mut self) {
        if self.usage_published {
            return;
        }
        self.usage_published = true;
        if let (Some(sink), Some(usage)) = (
            &self.usage_sink,
            claude_usage_from_stream_buffer(&self.usage_buffer),
        ) {
            sink.publish(self.model.as_deref(), usage);
        }
    }

    fn observe_diagnostics(&mut self, frame: &[u8]) {
        for line in frame.split(|byte| *byte == b'\n') {
            observe_claude_stream_line(
                line,
                &mut self.diagnostics_message_id,
                &mut self.diagnostics_completed,
            );
        }
        if self.diagnostics_completed && !self.diagnostics_committed {
            commit_claude_diagnostics(&self.diagnostics_state, &self.diagnostics_message_id);
            self.oauth_tool_aliases.remember(
                self.thread_alias_keys.as_deref(),
                &self.tool_name_reverse_map,
                &self.diagnostics_message_id,
            );
            self.diagnostics_committed = true;
        }
        if self.diagnostics_completed {
            self.finish_stream_usage();
        }
    }

    pub async fn record_terminal_failure(&mut self) {
        if self.failure_recorded || self.diagnostics_completed {
            return;
        }
        self.failure_recorded = true;
        let Some(binding) = self.failure_binding.as_ref() else {
            return;
        };
        let conductor = Arc::clone(&binding.conductor);
        let result = AccountExecutionResult {
            provider: "claude".to_owned(),
            auth_id: binding.auth_id.clone(),
            model: self.model.clone(),
            status: 502,
            retry_delay_ms: None,
            observed_at_ms: binding.clock.now_ms(),
        };
        let _ = tokio::task::spawn_blocking(move || conductor.record(result)).await;
    }
}

impl Drop for ClaudeTrackedMessagesStreamResponse {
    fn drop(&mut self) {
        if !self.usage_published {
            observe_plugin_executor_stream(
                "claude",
                &self.stream_line_buffer,
                &mut self.usage_buffer,
            );
        }
        self.finish_stream_usage();
    }
}

fn complete_sse_frame_len(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| index + 2)
        .or_else(|| {
            buffer
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
        })
}

impl fmt::Debug for ClaudeTrackedMessagesStreamResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeTrackedMessagesStreamResponse")
            .field("response", &self.response)
            .field(
                "failure_binding",
                &self.failure_binding.as_ref().map(|_| "attached"),
            )
            .field("model", &self.model)
            .field("failure_recorded", &self.failure_recorded)
            .finish()
    }
}

impl ClaudeExecutionOutcome {
    fn new(
        response: ClaudeMessagesResponse,
        replay: UnauthorizedReplayState,
        state_persisted: Option<bool>,
        request_scoped: bool,
    ) -> Self {
        Self {
            response,
            attempts: replay.attempts(),
            refreshed: replay.refreshed(),
            state_persisted,
            request_scoped,
        }
    }

    pub fn response(&self) -> &ClaudeMessagesResponse {
        &self.response
    }

    pub fn attempts(&self) -> u8 {
        self.attempts
    }

    pub fn refreshed(&self) -> bool {
        self.refreshed
    }

    pub fn state_persisted(&self) -> Option<bool> {
        self.state_persisted
    }

    pub fn request_scoped(&self) -> bool {
        self.request_scoped
    }
}

impl fmt::Debug for ClaudeExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeExecutionOutcome")
            .field("response", &self.response)
            .field("attempts", &self.attempts)
            .field("refreshed", &self.refreshed)
            .field("state_persisted", &self.state_persisted)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeExecutionError {
    Auth(ClaudeSubscriptionAuthError),
    PrepareAuth(ClaudePrepareAuthError),
    SessionId(SessionIdCacheError),
    IdentityStore(ClaudeIdentityStoreError),
    CredentialIdentity(ClaudeCredentialIdentityError),
    Request(ClaudeTargetError),
    Transport(ClaudeMessagesTransportFailure),
    AccountStateConfiguration,
    StreamingUnavailable,
    CallerSystemBlock(ClaudeCallerSystemBlockError),
}

impl fmt::Display for ClaudeExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth(error) => write!(formatter, "Claude execution auth failed: {error}"),
            Self::PrepareAuth(error) => {
                write!(formatter, "Claude request-auth preparation failed: {error}")
            }
            Self::SessionId(error) => {
                write!(formatter, "Claude session ID resolution failed: {error}")
            }
            Self::IdentityStore(error) => {
                write!(
                    formatter,
                    "Claude device profile resolution failed: {error}"
                )
            }
            Self::CredentialIdentity(error) => {
                write!(formatter, "Claude credential identity failed: {error}")
            }
            Self::Request(error) => write!(formatter, "Claude request is invalid: {error}"),
            Self::Transport(error) => write!(formatter, "Claude transport failed: {error:?}"),
            Self::AccountStateConfiguration => {
                formatter.write_str("Claude account state configuration is invalid")
            }
            Self::StreamingUnavailable => {
                formatter.write_str("Claude streaming transport is unavailable")
            }
            Self::CallerSystemBlock(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ClaudeExecutionError {}

/// Bounded multi-account Claude execution loop.
///
/// Each account executor owns its typed secret handles and outcome persistence.
/// The pool only selects an eligible account and advances after an
/// account-scoped failure.
pub struct ClaudeSubscriptionAccountPool {
    router: Arc<AccountRouter>,
    candidates: Vec<AccountCandidate>,
    executors: HashMap<String, Arc<ClaudeSubscriptionMessagesExecutor>>,
    clock: Arc<dyn AccountStateClock>,
    targets: Option<HashMap<String, ClaudeUpstreamTarget>>,
}

impl ClaudeSubscriptionAccountPool {
    #[must_use]
    pub fn contains_auth(&self, auth_id: &str) -> bool {
        self.candidates
            .iter()
            .any(|candidate| candidate.auth_id == auth_id)
    }

    pub fn selected_target(&self, auth_id: &str) -> Option<&ClaudeUpstreamTarget> {
        self.targets
            .as_ref()
            .and_then(|targets| targets.get(auth_id))
    }

    pub async fn prepare_selected_authorization(
        &self,
        auth_id: &str,
        target: &ClaudeUpstreamTarget,
    ) -> Result<super::ClaudePreparedAuthorization, ClaudeAccountPoolError> {
        let executor = self
            .executors
            .get(auth_id)
            .ok_or(ClaudeAccountPoolError::Configuration)?;
        let credentials = executor.auth.load().await.map_err(|error| {
            ClaudeAccountPoolError::Execution(ClaudeExecutionError::Auth(error))
        })?;
        super::ClaudePreparedAuthorization::prepare(
            target,
            ClaudeCredentialMode::OAuth,
            credentials.access_token(),
        )
        .map_err(|error| ClaudeAccountPoolError::Execution(ClaudeExecutionError::Request(error)))
    }

    pub async fn prepare_selected_count_tokens_request(
        &self,
        auth_id: &str,
        model: &str,
        body: Vec<u8>,
        context: Option<&ClaudeExecutionRequestContext>,
    ) -> Result<ClaudeMessagesRequest, ClaudeAccountPoolError> {
        if !self.contains_auth(auth_id) {
            return Err(ClaudeAccountPoolError::Configuration);
        }
        let executor = self
            .executors
            .get(auth_id)
            .ok_or(ClaudeAccountPoolError::Configuration)?;
        let target = self
            .targets
            .as_ref()
            .and_then(|targets| targets.get(auth_id))
            .cloned()
            .ok_or(ClaudeAccountPoolError::Configuration)?;
        if !target.is_anthropic_api() {
            return Err(ClaudeAccountPoolError::Configuration);
        }
        executor
            .prepare_first_party_count_tokens_request(target, model, body, context)
            .await
            .map_err(ClaudeAccountPoolError::Execution)
    }

    /// Selects one eligible Claude account using the same persisted scheduler
    /// and cooldown authority as Messages. The returned lane remains fixed for
    /// preparation, transport and an optional 401 replay.
    pub fn select_configured_auth_id(&self, model: &str) -> Result<String, ClaudeAccountPoolError> {
        self.router
            .select("claude", Some(model), self.clock.now_ms(), &self.candidates)
            .map(|selected| selected.auth_id)
            .map_err(ClaudeAccountPoolError::Routing)
    }

    pub async fn execute_count_tokens_selected_with_context(
        &self,
        auth_id: &str,
        model: &str,
        body: Vec<u8>,
        context: Option<&ClaudeExecutionRequestContext>,
    ) -> Result<ClaudePooledExecutionOutcome, ClaudeAccountPoolError> {
        if !self.contains_auth(auth_id) {
            return Err(ClaudeAccountPoolError::Configuration);
        }
        let executor = self
            .executors
            .get(auth_id)
            .ok_or(ClaudeAccountPoolError::Configuration)?;
        let target = self
            .targets
            .as_ref()
            .and_then(|targets| targets.get(auth_id))
            .cloned()
            .ok_or(ClaudeAccountPoolError::Configuration)?;
        let outcome = executor
            .execute_count_tokens_for_model_with_context(target, model, body, context)
            .await
            .map_err(ClaudeAccountPoolError::Execution)?;
        if outcome.state_persisted() != Some(true) {
            return Err(ClaudeAccountPoolError::OutcomePersistence);
        }
        Ok(ClaudePooledExecutionOutcome {
            selected_auth_id: auth_id.to_owned(),
            attempted_auth_ids: vec![auth_id.to_owned()],
            outcome,
        })
    }

    pub fn new(
        router: Arc<AccountRouter>,
        candidates: Vec<AccountCandidate>,
        executors: HashMap<String, Arc<ClaudeSubscriptionMessagesExecutor>>,
    ) -> Result<Self, ClaudeAccountPoolError> {
        Self::with_clock(
            router,
            candidates,
            executors,
            Arc::new(SystemAccountStateClock),
        )
    }

    pub fn with_clock(
        router: Arc<AccountRouter>,
        candidates: Vec<AccountCandidate>,
        executors: HashMap<String, Arc<ClaudeSubscriptionMessagesExecutor>>,
        clock: Arc<dyn AccountStateClock>,
    ) -> Result<Self, ClaudeAccountPoolError> {
        if candidates.is_empty() {
            return Err(ClaudeAccountPoolError::Configuration);
        }
        let mut seen = HashSet::new();
        for candidate in &candidates {
            if candidate.auth_id.trim().is_empty() || !seen.insert(candidate.auth_id.as_str()) {
                return Err(ClaudeAccountPoolError::Configuration);
            }
            let executor = executors
                .get(&candidate.auth_id)
                .ok_or(ClaudeAccountPoolError::Configuration)?;
            if executor.account_state_auth_id() != Some(candidate.auth_id.as_str()) {
                return Err(ClaudeAccountPoolError::Configuration);
            }
        }
        Ok(Self {
            router,
            candidates,
            executors,
            clock,
            targets: None,
        })
    }

    pub fn with_targets(
        mut self,
        targets: HashMap<String, ClaudeUpstreamTarget>,
    ) -> Result<Self, ClaudeAccountPoolError> {
        if self
            .candidates
            .iter()
            .any(|candidate| !targets.contains_key(&candidate.auth_id))
        {
            return Err(ClaudeAccountPoolError::Configuration);
        }
        self.targets = Some(targets);
        Ok(self)
    }

    /// Executes against the auth lane already selected by the manager.
    ///
    /// Unlike [`Self::execute_configured`], this entry point deliberately does
    /// not perform another routing pass or fail over to a different account.
    /// The manager's scheduler remains the sole authority for provider/auth
    /// selection when the pool is exposed through a `ProviderExecutor`.
    pub async fn execute_selected(
        &self,
        auth_id: &str,
        model: &str,
        body: Vec<u8>,
        stream: bool,
    ) -> Result<ClaudePooledExecutionOutcome, ClaudeAccountPoolError> {
        self.execute_selected_with_context(auth_id, model, body, stream, None)
            .await
    }

    pub async fn execute_selected_with_context(
        &self,
        auth_id: &str,
        model: &str,
        body: Vec<u8>,
        stream: bool,
        context: Option<&ClaudeExecutionRequestContext>,
    ) -> Result<ClaudePooledExecutionOutcome, ClaudeAccountPoolError> {
        if !self.contains_auth(auth_id) {
            return Err(ClaudeAccountPoolError::Configuration);
        }
        let executor = self
            .executors
            .get(auth_id)
            .ok_or(ClaudeAccountPoolError::Configuration)?;
        let target = self
            .targets
            .as_ref()
            .and_then(|targets| targets.get(auth_id))
            .cloned()
            .ok_or(ClaudeAccountPoolError::Configuration)?;
        let outcome = executor
            .execute_for_model_with_context(target, Some(model), body, stream, context)
            .await
            .map_err(ClaudeAccountPoolError::Execution)?;
        if outcome.state_persisted() != Some(true) {
            return Err(ClaudeAccountPoolError::OutcomePersistence);
        }
        Ok(ClaudePooledExecutionOutcome {
            selected_auth_id: auth_id.to_owned(),
            attempted_auth_ids: vec![auth_id.to_owned()],
            outcome,
        })
    }

    /// Streaming counterpart to [`Self::execute_selected`].
    pub async fn execute_stream_selected(
        &self,
        auth_id: &str,
        model: &str,
        body: Vec<u8>,
    ) -> Result<ClaudePooledStreamExecutionOutcome, ClaudeAccountPoolError> {
        self.execute_stream_selected_with_context(auth_id, model, body, None)
            .await
    }

    pub async fn execute_stream_selected_with_context(
        &self,
        auth_id: &str,
        model: &str,
        body: Vec<u8>,
        context: Option<&ClaudeExecutionRequestContext>,
    ) -> Result<ClaudePooledStreamExecutionOutcome, ClaudeAccountPoolError> {
        if !self.contains_auth(auth_id) {
            return Err(ClaudeAccountPoolError::Configuration);
        }
        let executor = self
            .executors
            .get(auth_id)
            .ok_or(ClaudeAccountPoolError::Configuration)?;
        let target = self
            .targets
            .as_ref()
            .and_then(|targets| targets.get(auth_id))
            .cloned()
            .ok_or(ClaudeAccountPoolError::Configuration)?;
        let outcome = executor
            .execute_stream_for_model_with_context(target, Some(model), body, context)
            .await
            .map_err(ClaudeAccountPoolError::Execution)?;
        if outcome.state_persisted() != Some(true) {
            return Err(ClaudeAccountPoolError::OutcomePersistence);
        }
        Ok(ClaudePooledStreamExecutionOutcome {
            selected_auth_id: auth_id.to_owned(),
            attempted_auth_ids: vec![auth_id.to_owned()],
            outcome,
        })
    }

    pub async fn execute(
        &self,
        target: ClaudeUpstreamTarget,
        model: &str,
        body: Vec<u8>,
        stream: bool,
    ) -> Result<ClaudePooledExecutionOutcome, ClaudeAccountPoolError> {
        self.execute_inner(Some(target), model, body, stream, None)
            .await
    }

    pub async fn execute_configured(
        &self,
        model: &str,
        body: Vec<u8>,
        stream: bool,
    ) -> Result<ClaudePooledExecutionOutcome, ClaudeAccountPoolError> {
        self.execute_inner(None, model, body, stream, None).await
    }

    /// Builds caller context only after selecting the actual account, and
    /// rebuilds it for each failover attempt. Headers and the original body
    /// remain request-owned; they cannot supply a selected auth identity.
    pub async fn execute_configured_with_request_context(
        &self,
        model: &str,
        body: Vec<u8>,
        stream: bool,
        original_body: &[u8],
        headers: &Headers,
    ) -> Result<ClaudePooledExecutionOutcome, ClaudeAccountPoolError> {
        self.execute_inner(None, model, body, stream, Some((original_body, headers)))
            .await
    }

    pub async fn execute_stream_configured(
        &self,
        model: &str,
        body: Vec<u8>,
    ) -> Result<ClaudePooledStreamExecutionOutcome, ClaudeAccountPoolError> {
        self.execute_stream_configured_inner(model, body, None)
            .await
    }

    /// Streaming counterpart with the same account-local context boundary.
    pub async fn execute_stream_configured_with_request_context(
        &self,
        model: &str,
        body: Vec<u8>,
        original_body: &[u8],
        headers: &Headers,
    ) -> Result<ClaudePooledStreamExecutionOutcome, ClaudeAccountPoolError> {
        self.execute_stream_configured_inner(model, body, Some((original_body, headers)))
            .await
    }

    async fn execute_stream_configured_inner(
        &self,
        model: &str,
        body: Vec<u8>,
        request_context: Option<(&[u8], &Headers)>,
    ) -> Result<ClaudePooledStreamExecutionOutcome, ClaudeAccountPoolError> {
        let mut remaining = crate::internal::api::account_selection::candidates(&self.candidates);
        let mut attempted_auth_ids = Vec::new();
        let mut last_execution_error = None;
        let mut last_outcome = None;

        while !remaining.is_empty() {
            let selected = match self.router.select(
                "claude", Some(model), self.clock.now_ms(), &remaining,
            ) {
                Ok(selected) => selected,
                // Preserve the prior upstream result when no eligible fallback remains.
                Err(AccountRoutingError::Selection(
                    AccountSelectionError::NotFound
                    | AccountSelectionError::Unavailable
                    | AccountSelectionError::Cooldown { .. },
                )) if !attempted_auth_ids.is_empty() => break,
                Err(error) => return Err(ClaudeAccountPoolError::Routing(error)),
            };
            crate::internal::api::account_selection::record_selected(&selected.auth_id);
            remaining.retain(|candidate| candidate.auth_id != selected.auth_id);
            attempted_auth_ids.push(selected.auth_id.clone());
            let executor = self
                .executors
                .get(&selected.auth_id)
                .ok_or(ClaudeAccountPoolError::Configuration)?;
            let target = self
                .targets
                .as_ref()
                .and_then(|targets| targets.get(&selected.auth_id))
                .cloned()
                .ok_or(ClaudeAccountPoolError::Configuration)?;
            let context = request_context.map(|(original_body, headers)| {
                ClaudeExecutionRequestContext::from_provider_request(
                    selected.auth_id.clone(),
                    headers.clone(),
                    original_body,
                    &body,
                    Default::default(),
                    Default::default(),
                )
            });
            match executor
                .execute_stream_for_model_with_context(
                    target,
                    Some(model),
                    body.clone(),
                    context.as_ref(),
                )
                .await
            {
                Ok(outcome) => {
                    if outcome.state_persisted() != Some(true) {
                        return Err(ClaudeAccountPoolError::OutcomePersistence);
                    }
                    let status = outcome.response().status();
                    if outcome.request_scoped()
                        || (200..300).contains(&status)
                        || matches!(status, 400 | 422)
                    {
                        return Ok(ClaudePooledStreamExecutionOutcome {
                            selected_auth_id: selected.auth_id,
                            attempted_auth_ids,
                            outcome,
                        });
                    }
                    last_outcome = Some((selected.auth_id, outcome));
                }
                Err(error) => last_execution_error = Some(error),
            }
        }

        if let Some((selected_auth_id, outcome)) = last_outcome {
            return Ok(ClaudePooledStreamExecutionOutcome {
                selected_auth_id,
                attempted_auth_ids,
                outcome,
            });
        }
        Err(last_execution_error.map_or(
            ClaudeAccountPoolError::Configuration,
            ClaudeAccountPoolError::Execution,
        ))
    }

    async fn execute_inner(
        &self,
        fallback_target: Option<ClaudeUpstreamTarget>,
        model: &str,
        body: Vec<u8>,
        stream: bool,
        request_context: Option<(&[u8], &Headers)>,
    ) -> Result<ClaudePooledExecutionOutcome, ClaudeAccountPoolError> {
        let mut remaining = crate::internal::api::account_selection::candidates(&self.candidates);
        let mut attempted_auth_ids = Vec::new();
        let mut last_execution_error = None;
        let mut last_outcome = None;

        while !remaining.is_empty() {
            let selected = match self.router.select(
                "claude", Some(model), self.clock.now_ms(), &remaining,
            ) {
                Ok(selected) => selected,
                // Preserve the prior upstream result when no eligible fallback remains.
                Err(AccountRoutingError::Selection(
                    AccountSelectionError::NotFound
                    | AccountSelectionError::Unavailable
                    | AccountSelectionError::Cooldown { .. },
                )) if !attempted_auth_ids.is_empty() => break,
                Err(error) => return Err(ClaudeAccountPoolError::Routing(error)),
            };
            crate::internal::api::account_selection::record_selected(&selected.auth_id);
            remaining.retain(|candidate| candidate.auth_id != selected.auth_id);
            attempted_auth_ids.push(selected.auth_id.clone());
            let executor = self
                .executors
                .get(&selected.auth_id)
                .ok_or(ClaudeAccountPoolError::Configuration)?;
            let target = self
                .targets
                .as_ref()
                .and_then(|targets| targets.get(&selected.auth_id))
                .cloned()
                .or_else(|| fallback_target.clone())
                .ok_or(ClaudeAccountPoolError::Configuration)?;
            let context = request_context.map(|(original_body, headers)| {
                ClaudeExecutionRequestContext::from_provider_request(
                    selected.auth_id.clone(),
                    headers.clone(),
                    original_body,
                    &body,
                    Default::default(),
                    Default::default(),
                )
            });
            match executor
                .execute_for_model_with_context(
                    target,
                    Some(model),
                    body.clone(),
                    stream,
                    context.as_ref(),
                )
                .await
            {
                Ok(outcome) => {
                    if outcome.state_persisted() != Some(true) {
                        return Err(ClaudeAccountPoolError::OutcomePersistence);
                    }
                    let status = outcome.response().status();
                    if outcome.request_scoped()
                        || (200..300).contains(&status)
                        || matches!(status, 400 | 422)
                    {
                        return Ok(ClaudePooledExecutionOutcome {
                            selected_auth_id: selected.auth_id,
                            attempted_auth_ids,
                            outcome,
                        });
                    }
                    last_outcome = Some((selected.auth_id, outcome));
                }
                Err(error) => last_execution_error = Some(error),
            }
        }

        if let Some((selected_auth_id, outcome)) = last_outcome {
            return Ok(ClaudePooledExecutionOutcome {
                selected_auth_id,
                attempted_auth_ids,
                outcome,
            });
        }
        Err(last_execution_error.map_or(
            ClaudeAccountPoolError::Configuration,
            ClaudeAccountPoolError::Execution,
        ))
    }
}

impl fmt::Debug for ClaudeSubscriptionAccountPool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeSubscriptionAccountPool")
            .field("router", &self.router)
            .field("candidate_count", &self.candidates.len())
            .field("executors", &"[REDACTED]")
            .finish()
    }
}

pub struct ClaudePooledExecutionOutcome {
    selected_auth_id: String,
    attempted_auth_ids: Vec<String>,
    outcome: ClaudeExecutionOutcome,
}

pub struct ClaudePooledStreamExecutionOutcome {
    selected_auth_id: String,
    attempted_auth_ids: Vec<String>,
    outcome: ClaudeStreamExecutionOutcome,
}

impl ClaudePooledStreamExecutionOutcome {
    pub fn selected_auth_id(&self) -> &str {
        &self.selected_auth_id
    }

    pub fn attempted_auth_ids(&self) -> &[String] {
        &self.attempted_auth_ids
    }

    pub fn outcome(&self) -> &ClaudeStreamExecutionOutcome {
        &self.outcome
    }

    pub fn outcome_mut(&mut self) -> &mut ClaudeStreamExecutionOutcome {
        &mut self.outcome
    }

    pub fn into_outcome(self) -> ClaudeStreamExecutionOutcome {
        self.outcome
    }
}

impl fmt::Debug for ClaudePooledStreamExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudePooledStreamExecutionOutcome")
            .field("selected_auth_id", &self.selected_auth_id)
            .field("attempted_auth_ids", &self.attempted_auth_ids)
            .field("outcome", &self.outcome)
            .finish()
    }
}

impl ClaudePooledExecutionOutcome {
    pub fn selected_auth_id(&self) -> &str {
        &self.selected_auth_id
    }

    pub fn attempted_auth_ids(&self) -> &[String] {
        &self.attempted_auth_ids
    }

    pub fn outcome(&self) -> &ClaudeExecutionOutcome {
        &self.outcome
    }
}

impl fmt::Debug for ClaudePooledExecutionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudePooledExecutionOutcome")
            .field("selected_auth_id", &self.selected_auth_id)
            .field("attempted_auth_ids", &self.attempted_auth_ids)
            .field("outcome", &self.outcome)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeAccountPoolError {
    Configuration,
    Routing(AccountRoutingError),
    Execution(ClaudeExecutionError),
    OutcomePersistence,
}

impl fmt::Display for ClaudeAccountPoolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration => formatter.write_str("Claude account pool is invalid"),
            Self::Routing(error) => write!(formatter, "Claude account routing failed: {error}"),
            Self::Execution(error) => write!(formatter, "Claude pooled execution failed: {error}"),
            Self::OutcomePersistence => {
                formatter.write_str("Claude account outcome persistence failed")
            }
        }
    }
}

impl std::error::Error for ClaudeAccountPoolError {}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::time::SystemTime;

    use serde_json::Value;
    use tokio::sync::mpsc;

    use crate::internal::auth::claude::{
        ClaudeCredentialHandles, ClaudeRefreshCoordinator, ClaudeRefreshTransport,
        ClaudeSecretHandle, ClaudeSecretKind, ClaudeSecretStore, ClaudeStoredCredentials,
        RefreshClock, RefreshHttpResponse, RefreshRequest, RefreshTransportFailure,
        SecretStoreError, SecretString,
    };
    use crate::sdk::cliproxy::auth::{CooldownStateRecord, CooldownStateStore, CooldownStoreError};

    use super::*;

    struct MemoryStore(Mutex<ClaudeStoredCredentials>);

    impl MemoryStore {
        fn new() -> Self {
            Self(Mutex::new(ClaudeStoredCredentials::new(
                SecretString::new("access-old").unwrap(),
                SecretString::new("refresh-old").unwrap(),
            )))
        }
    }

    impl ClaudeSecretStore for MemoryStore {
        fn load_credentials(
            &self,
            _handles: &ClaudeCredentialHandles,
        ) -> Result<ClaudeStoredCredentials, SecretStoreError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn store_credentials(
            &self,
            _handles: &ClaudeCredentialHandles,
            credentials: &ClaudeStoredCredentials,
        ) -> Result<(), SecretStoreError> {
            *self.0.lock().unwrap() = credentials.clone();
            Ok(())
        }
    }

    struct FixedClock;

    impl RefreshClock for FixedClock {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_000)
        }

        fn sleep(
            &self,
            _duration: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<(), RefreshTransportFailure>> + Send + '_>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    struct RefreshTransport;

    impl ClaudeRefreshTransport for RefreshTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a RefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<RefreshHttpResponse, RefreshTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async {
                Ok(RefreshHttpResponse::new(
                    200,
                    None,
                    None,
                    br#"{"access_token":"access-new","refresh_token":"refresh-new","expires_in":3600}"#.to_vec(),
                ))
            })
        }
    }

    struct SequenceTransport {
        statuses: Mutex<Vec<u16>>,
        authorizations: Mutex<Vec<String>>,
        session_ids: Mutex<Vec<String>>,
        bodies: Mutex<Vec<Vec<u8>>>,
        response_body: Vec<u8>,
        failure: Option<ClaudeMessagesTransportFailure>,
        retry_after: Option<Duration>,
    }

    impl SequenceTransport {
        fn statuses(statuses: Vec<u16>) -> Self {
            Self {
                statuses: Mutex::new(statuses.into_iter().rev().collect()),
                authorizations: Mutex::new(Vec::new()),
                session_ids: Mutex::new(Vec::new()),
                bodies: Mutex::new(Vec::new()),
                response_body: b"{}".to_vec(),
                failure: None,
                retry_after: None,
            }
        }

        fn statuses_with_retry(statuses: Vec<u16>, retry_after: Duration) -> Self {
            Self {
                statuses: Mutex::new(statuses.into_iter().rev().collect()),
                authorizations: Mutex::new(Vec::new()),
                session_ids: Mutex::new(Vec::new()),
                bodies: Mutex::new(Vec::new()),
                response_body: b"{}".to_vec(),
                failure: None,
                retry_after: Some(retry_after),
            }
        }

        fn failing(failure: ClaudeMessagesTransportFailure) -> Self {
            Self {
                statuses: Mutex::new(Vec::new()),
                authorizations: Mutex::new(Vec::new()),
                session_ids: Mutex::new(Vec::new()),
                bodies: Mutex::new(Vec::new()),
                response_body: b"{}".to_vec(),
                failure: Some(failure),
                retry_after: None,
            }
        }

        fn responding(body: &[u8]) -> Self {
            Self {
                statuses: Mutex::new(vec![200]),
                authorizations: Mutex::new(Vec::new()),
                session_ids: Mutex::new(Vec::new()),
                bodies: Mutex::new(Vec::new()),
                response_body: body.to_vec(),
                failure: None,
                retry_after: None,
            }
        }
    }

    impl ClaudeMessagesTransport for SequenceTransport {
        fn execute<'a>(
            &'a self,
            request: &'a ClaudeMessagesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<ClaudeMessagesResponse, ClaudeMessagesTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                if let Some(failure) = self.failure {
                    return Err(failure);
                }
                self.authorizations
                    .lock()
                    .unwrap()
                    .push(request.authorization().expose_header_value().to_owned());
                self.session_ids
                    .lock()
                    .unwrap()
                    .push(request.fingerprint().session_id().to_owned());
                self.bodies.lock().unwrap().push(request.body().to_vec());
                let status = self.statuses.lock().unwrap().pop().unwrap();
                Ok(
                    ClaudeMessagesResponse::new(status, self.response_body.clone())
                        .with_retry_after(self.retry_after),
                )
            })
        }
    }

    struct FixedStreamingTransport {
        status: u16,
        chunks: Vec<Result<Vec<u8>, ClaudeMessagesTransportFailure>>,
    }

    impl ClaudeMessagesStreamingTransport for FixedStreamingTransport {
        fn execute_stream<'a>(
            &'a self,
            _request: &'a ClaudeMessagesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            ClaudeMessagesStreamResponse,
                            ClaudeMessagesTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                let (sender, receiver) = mpsc::channel(8);
                let chunks = self.chunks.clone();
                tokio::spawn(async move {
                    for chunk in chunks {
                        if sender.send(chunk).await.is_err() {
                            return;
                        }
                    }
                });
                Ok(ClaudeMessagesStreamResponse::new(
                    self.status,
                    None,
                    receiver,
                ))
            })
        }
    }

    #[derive(Default)]
    struct MemoryCooldownStore(Mutex<Vec<CooldownStateRecord>>);

    impl CooldownStateStore for MemoryCooldownStore {
        fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn save(&self, records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
            *self.0.lock().unwrap() = records.to_vec();
            Ok(())
        }
    }

    struct FixedAccountClock;

    impl AccountStateClock for FixedAccountClock {
        fn now_ms(&self) -> i64 {
            10_000
        }
    }

    fn handles() -> ClaudeCredentialHandles {
        ClaudeCredentialHandles::new(
            ClaudeSecretHandle::new("subscriptions", "access", ClaudeSecretKind::AccessToken)
                .unwrap(),
            ClaudeSecretHandle::new("subscriptions", "refresh", ClaudeSecretKind::RefreshToken)
                .unwrap(),
        )
        .unwrap()
    }

    fn executor(transport: Arc<SequenceTransport>) -> ClaudeSubscriptionMessagesExecutor {
        let auth = Arc::new(ClaudeSubscriptionAuth::new(
            handles(),
            Arc::new(MemoryStore::new()),
            Arc::new(RefreshTransport),
            Arc::new(FixedClock),
            Arc::new(ClaudeRefreshCoordinator::default()),
        ));
        ClaudeSubscriptionMessagesExecutor::new(auth, transport, Duration::from_secs(30))
    }

    fn target() -> ClaudeUpstreamTarget {
        ClaudeUpstreamTarget::new("https", "api.anthropic.com").unwrap()
    }

    fn alias_create_body() -> Vec<u8> {
        br#"{"model":"sonnet","thread":{"type":"create"},"messages":[{"role":"user","content":"read"}],"tools":[{"name":"Read","input_schema":{"type":"object"}}]}"#.to_vec()
    }

    fn alias_continuation_body() -> Vec<u8> {
        br#"{"model":"sonnet","thread":{"type":"continue","previous_message_id":"msg-alias"},"messages":[{"role":"user","content":"continue"}]}"#.to_vec()
    }

    fn alias_response_body() -> Vec<u8> {
        let alias = super::super::helps::claude_mcp_tool_alias("access-old", "Read", 0);
        serde_json::to_vec(&serde_json::json!({"id":"msg-alias","type":"message","content":[{"type":"tool_use","id":"call","name":alias,"input":{}}]})).unwrap()
    }

    const CONFIGURED_CONTEXT_SSE: &[u8] = b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-context\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"sonnet\",\"content\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n\
data: {\"type\":\"message_stop\"}\n\n";

    fn configured_context_body(input: &str, stream: bool) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "model": "sonnet", "input": input, "stream": stream,
            "metadata": { "user_id": serde_json::json!({
                "device_id": "0".repeat(64),
                "account_uuid": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "session_id": "11111111-2222-4333-8444-555555555555"
            }).to_string() }
        }))
        .unwrap()
    }

    fn configured_context_headers(session: &str) -> Headers {
        Headers::from([
            (
                "User-Agent".into(),
                vec!["claude-cli/2.1.280 (external, cli)".into()],
            ),
            ("X-App".into(), vec!["cli".into()]),
            ("Anthropic-Beta".into(), vec!["claude-code-20250219".into()]),
            ("x-claude-code-session-id".into(), vec![session.into()]),
            ("x-claude-code-agent-id".into(), vec!["writer".into()]),
        ])
    }

    #[tokio::test]
    async fn configured_context_responses_isolates_parallel_sessions_and_reuses_continuation() {
        use crate::sdk::api::handlers::openai::openai_responses_handlers::{
            OpenAiResponsesClaudeHandler, OpenAiResponsesRouteResponse,
        };
        let first = thread_fixture_transport(vec![200, 200, 200], CONFIGURED_CONTEXT_SSE);
        let second = thread_fixture_transport(vec![200], CONFIGURED_CONTEXT_SSE);
        let (pool, _) = thread_fixture_pool(first.clone(), second.clone(), None);
        let handler = OpenAiResponsesClaudeHandler::new(Arc::new(pool));
        let body_a = configured_context_body("request-a", false);
        let body_b = configured_context_body("request-b", false);
        let headers_a = configured_context_headers("session-a");
        let headers_b = configured_context_headers("session-b");
        let (a, b) = tokio::join!(
            handler.handle_route_with_headers(&body_a, &headers_a),
            handler.handle_route_with_headers(&body_b, &headers_b),
        );
        for response in [
            a,
            b,
            handler.handle_route_with_headers(&body_a, &headers_a).await,
        ] {
            let OpenAiResponsesRouteResponse::Buffered(response) = response else {
                panic!("expected buffered Responses reply");
            };
            assert_eq!(response.status(), 200);
        }
        let sessions = first.session_ids.lock().unwrap();
        let bodies = first.bodies.lock().unwrap();
        assert_eq!(sessions.len(), 3);
        let a_sessions = bodies
            .iter()
            .zip(sessions.iter())
            .filter(|(body, _)| String::from_utf8_lossy(body).contains("request-a"))
            .map(|(_, session)| session)
            .collect::<Vec<_>>();
        let b_sessions = bodies
            .iter()
            .zip(sessions.iter())
            .filter(|(body, _)| String::from_utf8_lossy(body).contains("request-b"))
            .map(|(_, session)| session)
            .collect::<Vec<_>>();
        assert_eq!(a_sessions.len(), 2);
        assert_eq!(b_sessions.len(), 1);
        assert_eq!(a_sessions[0], a_sessions[1]);
        assert_ne!(a_sessions[0], b_sessions[0]);
        assert!(second.authorizations.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn configured_context_responses_keeps_session_on_401_refresh_replay() {
        use crate::sdk::api::handlers::openai::openai_responses_handlers::{
            OpenAiResponsesClaudeHandler, OpenAiResponsesRouteResponse,
        };
        let first = thread_fixture_transport(vec![401, 200], CONFIGURED_CONTEXT_SSE);
        let second = thread_fixture_transport(vec![200], CONFIGURED_CONTEXT_SSE);
        let (pool, _) = thread_fixture_pool(first.clone(), second.clone(), None);
        let handler = OpenAiResponsesClaudeHandler::new(Arc::new(pool));
        let response = handler
            .handle_route_with_headers(
                &configured_context_body("request-refresh", false),
                &configured_context_headers("session-refresh"),
            )
            .await;
        let OpenAiResponsesRouteResponse::Buffered(response) = response else {
            panic!("expected buffered Responses reply");
        };
        assert_eq!(response.status(), 200);
        let sessions = first.session_ids.lock().unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0], sessions[1]);
        assert_eq!(
            *first.authorizations.lock().unwrap(),
            ["Bearer access-old", "Bearer access-new"]
        );
        assert!(second.authorizations.lock().unwrap().is_empty());
    }

    struct ConfiguredContextStreamTransport {
        status: u16,
        sessions: Mutex<Vec<String>>,
    }

    impl ClaudeMessagesStreamingTransport for ConfiguredContextStreamTransport {
        fn execute_stream<'a>(
            &'a self,
            request: &'a ClaudeMessagesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            ClaudeMessagesStreamResponse,
                            ClaudeMessagesTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.sessions
                    .lock()
                    .unwrap()
                    .push(request.fingerprint().session_id().to_owned());
                let (sender, receiver) = mpsc::channel(1);
                if self.status == 200 {
                    sender
                        .try_send(Ok(CONFIGURED_CONTEXT_SSE.to_vec()))
                        .unwrap();
                }
                drop(sender);
                Ok(ClaudeMessagesStreamResponse::new(
                    self.status,
                    None,
                    receiver,
                ))
            })
        }
    }

    #[tokio::test]
    async fn configured_context_responses_stream_retains_session_on_real_account_failover() {
        use crate::sdk::api::handlers::openai::openai_responses_handlers::{
            OpenAiResponsesClaudeHandler, OpenAiResponsesRouteResponse,
        };
        let first_stream = Arc::new(ConfiguredContextStreamTransport {
            status: 429,
            sessions: Mutex::new(Vec::new()),
        });
        let second_stream = Arc::new(ConfiguredContextStreamTransport {
            status: 200,
            sessions: Mutex::new(Vec::new()),
        });
        let (pool, cooldowns) = thread_fixture_pool(
            thread_fixture_transport(vec![200], CONFIGURED_CONTEXT_SSE),
            thread_fixture_transport(vec![200], CONFIGURED_CONTEXT_SSE),
            Some((first_stream.clone(), second_stream.clone())),
        );
        let handler = OpenAiResponsesClaudeHandler::new(Arc::new(pool));
        let response = handler
            .handle_route_with_headers(
                &configured_context_body("request-stream", true),
                &configured_context_headers("session-stream"),
            )
            .await;
        let OpenAiResponsesRouteResponse::Stream(mut stream) = response else {
            panic!("expected streaming Responses reply");
        };
        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk);
        }
        assert!(String::from_utf8_lossy(&output).contains("response.completed"));
        let first_sessions = first_stream.sessions.lock().unwrap();
        let second_sessions = second_stream.sessions.lock().unwrap();
        assert_eq!(first_sessions.len(), 1);
        assert_eq!(second_sessions.len(), 1);
        assert_eq!(first_sessions[0], second_sessions[0]);
        let records = cooldowns.0.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].auth_id, "account-a");
        assert_eq!(
            records[0].last_error.as_ref().unwrap().http_status,
            Some(429)
        );
    }
    #[tokio::test]
    async fn candidate_claude_thread_alias_unary_restores_names_after_create() {
        let transport = thread_fixture_transport(vec![200, 200], &alias_response_body());
        let executor = executor(transport.clone());
        for body in [alias_create_body(), alias_continuation_body()] {
            let outcome = executor
                .execute_for_model(target(), Some("sonnet"), body, false)
                .await
                .unwrap();
            assert_eq!(outcome.response().status(), 200);
            let response: Value = serde_json::from_slice(outcome.response().body()).unwrap();
            assert_eq!(response["content"][0]["name"], "Read");
            assert!(!outcome.refreshed());
        }
        assert_eq!(transport.authorizations.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn candidate_claude_thread_alias_missing_is_local_and_request_scoped() {
        let first = Arc::new(SequenceTransport::statuses(vec![200]));
        let second = Arc::new(SequenceTransport::statuses(vec![200]));
        let stream_a = thread_fixture_stream(200, b"");
        let stream_b = thread_fixture_stream(200, b"");
        let (pool, cooldowns) = thread_fixture_pool(
            first.clone(),
            second.clone(),
            Some((stream_a.clone(), stream_b.clone())),
        );
        let unary = pool
            .execute(target(), "sonnet", alias_continuation_body(), false)
            .await
            .unwrap();
        assert_eq!(unary.selected_auth_id(), "account-a");
        assert_eq!(unary.attempted_auth_ids(), ["account-a"]);
        assert_eq!(unary.outcome().response().status(), 404);
        assert!(unary.outcome().request_scoped());
        assert!(crate::internal::clienterror::is_claude_thread_not_found(
            404,
            unary.outcome().response().body()
        ));
        let stream = pool
            .execute_stream_configured("sonnet", alias_continuation_body())
            .await
            .unwrap();
        assert_eq!(stream.selected_auth_id(), "account-a");
        assert_eq!(stream.attempted_auth_ids(), ["account-a"]);
        assert_eq!(stream.outcome().response().status(), 404);
        assert!(stream.outcome().request_scoped());
        assert!(crate::internal::clienterror::is_claude_thread_not_found(
            404,
            stream.outcome().response().error_body()
        ));
        assert!(first.authorizations.lock().unwrap().is_empty());
        assert!(second.authorizations.lock().unwrap().is_empty());
        assert_eq!(stream_a.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(stream_b.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(cooldowns.0.lock().unwrap().is_empty());
    }

    struct AliasCompletionTransport {
        complete: bool,
    }

    impl ClaudeMessagesStreamingTransport for AliasCompletionTransport {
        fn execute_stream<'a>(
            &'a self,
            _: &'a ClaudeMessagesRequest,
            _: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            ClaudeMessagesStreamResponse,
                            ClaudeMessagesTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                let (sender, receiver) = mpsc::channel(4);
                sender.try_send(Ok(b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-alias\"}}\n\n".to_vec())).unwrap();
                let alias = super::super::helps::claude_mcp_tool_alias("access-old", "Read", 0);
                let tool = serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","name":alias}});
                sender
                    .try_send(Ok(format!("data: {tool}\n\n").into_bytes()))
                    .unwrap();
                if self.complete {
                    sender
                        .try_send(Ok(b"data: {\"type\":\"message_stop\"}\n\n".to_vec()))
                        .unwrap();
                }
                drop(sender);
                Ok(ClaudeMessagesStreamResponse::new(200, None, receiver))
            })
        }
    }

    #[tokio::test]
    async fn candidate_claude_thread_alias_stream_publishes_only_completed_message() {
        for complete in [false, true] {
            let transport = thread_fixture_transport(vec![200], &alias_response_body());
            let account = executor(transport.clone())
                .with_stream_transport(Arc::new(AliasCompletionTransport { complete }));
            let outcome = account
                .execute_stream_for_model(target(), Some("sonnet"), alias_create_body())
                .await
                .unwrap();
            let mut stream = outcome.into_response();
            let mut saw_restored_tool = false;
            while let Some(frame) = stream.next_chunk().await {
                let frame = String::from_utf8(frame.unwrap()).unwrap();
                saw_restored_tool |= frame.contains("\"name\":\"Read\"");
            }
            assert!(saw_restored_tool);
            let continuation = account
                .execute_for_model(target(), Some("sonnet"), alias_continuation_body(), false)
                .await
                .unwrap();
            if complete {
                assert_eq!(continuation.response().status(), 200);
                let response: Value =
                    serde_json::from_slice(continuation.response().body()).unwrap();
                assert_eq!(response["content"][0]["name"], "Read");
                assert_eq!(transport.authorizations.lock().unwrap().len(), 1);
            } else {
                assert_eq!(continuation.response().status(), 404);
                assert!(continuation.request_scoped());
                assert!(transport.authorizations.lock().unwrap().is_empty());
            }
            let other_account =
                executor(thread_fixture_transport(vec![200], &alias_response_body()));
            assert_eq!(
                other_account
                    .execute_for_model(target(), Some("sonnet"), alias_continuation_body(), false)
                    .await
                    .unwrap()
                    .response()
                    .status(),
                404
            );
        }
    }

    #[derive(Default)]
    struct BufferedAliasUsageSink(Mutex<Vec<super::super::claude_executor::ClaudeUsage>>);

    impl ClaudeUsageSink for BufferedAliasUsageSink {
        fn publish(&self, _: Option<&str>, usage: super::super::claude_executor::ClaudeUsage) {
            self.0.lock().unwrap().push(usage);
        }
    }

    fn alias_buffered_sse_body(stopped: bool) -> Vec<u8> {
        let alias = super::super::helps::claude_mcp_tool_alias("access-old", "Read", 0);
        let start = serde_json::json!({"type":"message_start","message":{
            "id":"msg-alias","model":"claude-sonnet","usage":{
                "input_tokens":7,"output_tokens":0,
                "cache_read_input_tokens":11,"cache_creation_input_tokens":13
            }
        }});
        let tool = serde_json::json!({"type":"content_block_start","index":0,
            "content_block":{"type":"tool_use","id":"call","name":alias,"input":{}}});
        let delta = serde_json::json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},
            "usage":{"output_tokens":5}});
        let mut body = format!(
            ": keepalive\r\n\nevent: message_start\ndata: {start}\n\nevent: content_block_start\ndata: {tool}\n\ndata: {delta}\n\n"
        );
        if stopped {
            body.push_str("data: {\"type\":\"message_stop\"}\n\ndata: [DONE]\n\n");
        }
        body.into_bytes()
    }

    #[tokio::test]
    async fn candidate_claude_thread_alias_buffered_sse_continuation_and_usage() {
        let transport = thread_fixture_transport(vec![200, 200], &alias_buffered_sse_body(true));
        let usage = Arc::new(BufferedAliasUsageSink::default());
        let account = executor(transport.clone()).with_usage_sink(usage.clone());
        for body in [alias_create_body(), alias_continuation_body()] {
            let outcome = account
                .execute_for_model(target(), Some("sonnet"), body, true)
                .await
                .unwrap();
            assert_eq!(outcome.response().status(), 200);
            let output = std::str::from_utf8(outcome.response().body()).unwrap();
            assert!(output.starts_with(": keepalive\r\n\nevent: message_start\n"));
            assert!(output.contains("\"name\":\"Read\""));
            assert!(output.ends_with("data: [DONE]\n\n"));
            assert!(!outcome.refreshed());
        }
        let measurements = usage.0.lock().unwrap();
        assert_eq!(measurements.len(), 2, "publish once per buffered response");
        for measured in measurements.iter() {
            assert_eq!(measured.input_tokens, 7);
            assert_eq!(measured.output_tokens, 5);
            assert_eq!(measured.cache_read_tokens, 11);
            assert_eq!(measured.cache_creation_tokens, 13);
            assert_eq!(measured.total_tokens, 36);
        }
        assert_eq!(transport.authorizations.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn candidate_claude_thread_alias_buffered_sse_retry_and_partial_state() {
        for stopped in [false, true] {
            let transport =
                thread_fixture_transport(vec![401, 200, 200], &alias_buffered_sse_body(stopped));
            let account = executor(transport.clone());
            let first = account
                .execute_for_model(target(), Some("sonnet"), alias_create_body(), true)
                .await
                .unwrap();
            assert_eq!(first.response().status(), 200);
            assert!(first.refreshed());
            assert!(std::str::from_utf8(first.response().body())
                .unwrap()
                .contains("\"name\":\"Read\""));
            let continuation = account
                .execute_for_model(target(), Some("sonnet"), alias_continuation_body(), true)
                .await
                .unwrap();
            assert_eq!(
                continuation.response().status(),
                if stopped { 200 } else { 404 }
            );
            if stopped {
                assert!(std::str::from_utf8(continuation.response().body())
                    .unwrap()
                    .contains("\"name\":\"Read\""));
            } else {
                assert!(continuation.request_scoped());
            }
            let authorizations = transport.authorizations.lock().unwrap();
            assert_eq!(authorizations.len(), if stopped { 3 } else { 2 });
            assert_eq!(authorizations[0], "Bearer access-old");
            assert_eq!(authorizations[1], "Bearer access-new");
            let sessions = transport.session_ids.lock().unwrap();
            assert_eq!(sessions[0], sessions[1], "401 replay keeps its cache lane");
        }
    }

    #[tokio::test]
    async fn candidate_claude_thread_alias_buffered_sse_invalid_response_is_not_success() {
        let valid = alias_buffered_sse_body(true);
        let invalid = [
            (b": keepalive\n\ndata: [DONE]\n\n".to_vec(), "empty stream response"),
            (b"data: invalid\n\n".to_vec(), "malformed stream data"),
            (b"data: {\"type\":\"message_delta\"}\n\n".to_vec(), "missing message_start"),
            (b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-alias\"}}\n\n".to_vec(), "missing id or model"),
            (b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-alias\",\"model\":\"sonnet\"}}\n\n".to_vec(), "ended before message completion"),
            ([valid.clone(), b"data: {\"type\":\"error\",\"error\":{\"message\":\"upstream failed\"}}\n\n".to_vec()].concat(), "error event: upstream failed"),
            ([valid, b"data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\n\n".to_vec()].concat(), "error event: overloaded_error"),
        ];
        for (body, expected_message) in invalid {
            let transport = thread_fixture_transport(vec![200], &body);
            let usage = Arc::new(BufferedAliasUsageSink::default());
            let account = executor(transport.clone()).with_usage_sink(usage.clone());
            let outcome = account
                .execute_for_model(target(), Some("sonnet"), alias_create_body(), true)
                .await
                .unwrap();
            assert_eq!(outcome.response().status(), 502);
            let error: Value = serde_json::from_slice(outcome.response().body()).unwrap();
            assert_eq!(error["error"]["type"], "api_error");
            assert!(error["error"]["message"]
                .as_str()
                .unwrap()
                .contains(expected_message));
            assert_eq!(
                outcome.response().headers()["content-type"],
                ["application/json"]
            );
            assert!(!outcome.refreshed());
            assert!(usage.0.lock().unwrap().is_empty());
            let continuation = account
                .execute_for_model(target(), Some("sonnet"), alias_continuation_body(), true)
                .await
                .unwrap();
            assert_eq!(continuation.response().status(), 404);
            assert!(continuation.request_scoped());
            assert_eq!(transport.authorizations.lock().unwrap().len(), 1);
        }
    }

    struct AliasUsageStreamTransport {
        stopped: bool,
        terminal_delimited: bool,
        failure: Option<ClaudeMessagesTransportFailure>,
    }

    impl ClaudeMessagesStreamingTransport for AliasUsageStreamTransport {
        fn execute_stream<'a>(
            &'a self,
            _: &'a ClaudeMessagesRequest,
            _: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            ClaudeMessagesStreamResponse,
                            ClaudeMessagesTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                let (sender, receiver) = mpsc::channel(2);
                let mut body = alias_buffered_sse_body(self.stopped);
                if self.stopped && !self.terminal_delimited {
                    let suffix = b"\n\ndata: [DONE]\n\n";
                    assert!(body.ends_with(suffix));
                    body.truncate(body.len() - suffix.len());
                }
                sender.try_send(Ok(body)).unwrap();
                if let Some(error) = self.failure {
                    sender.try_send(Err(error)).unwrap();
                }
                drop(sender);
                Ok(ClaudeMessagesStreamResponse::new(200, None, receiver))
            })
        }
    }

    #[tokio::test]
    async fn candidate_claude_thread_alias_stream_terminal_usage_and_cache_lane() {
        for (failure, terminal_delimited) in [
            (ClaudeMessagesTransportFailure::Timeout, true),
            (ClaudeMessagesTransportFailure::Cancelled, true),
            (ClaudeMessagesTransportFailure::Protocol, false),
        ] {
            let transport = thread_fixture_transport(vec![200], &alias_response_body());
            let usage = Arc::new(BufferedAliasUsageSink::default());
            let cooldowns = Arc::new(MemoryCooldownStore::default());
            let account = executor(transport.clone())
                .with_usage_sink(usage.clone())
                .with_account_state_clock(
                    "account-a",
                    Arc::new(CooldownConductor::new(cooldowns.clone())),
                    Arc::new(FixedAccountClock),
                )
                .unwrap()
                .with_stream_transport(Arc::new(AliasUsageStreamTransport {
                    stopped: true,
                    terminal_delimited,
                    failure: Some(failure),
                }));
            let outcome = account
                .execute_stream_for_model(target(), Some("sonnet"), alias_create_body())
                .await
                .unwrap();
            let previous_cooldowns = cooldowns.0.lock().unwrap().clone();
            let mut stream = outcome.into_response();
            let mut restored_name = false;
            while let Some(frame) = stream.next_chunk().await {
                let frame = std::str::from_utf8(frame.as_ref().unwrap()).unwrap();
                restored_name |= frame.contains("\"name\":\"Read\"");
            }
            assert!(
                restored_name,
                "event-prefixed tool frames restore client names"
            );
            stream.record_terminal_failure().await;
            assert!(
                *cooldowns.0.lock().unwrap() == previous_cooldowns,
                "transport failure after message_stop cannot cool a completed account"
            );
            drop(stream);
            let measurements = usage.0.lock().unwrap();
            assert_eq!(measurements.len(), 1);
            assert_eq!(measurements[0].input_tokens, 7);
            assert_eq!(measurements[0].output_tokens, 5);
            assert_eq!(measurements[0].cache_read_tokens, 11);
            assert_eq!(measurements[0].cache_creation_tokens, 13);
            assert_eq!(measurements[0].total_tokens, 36);
            drop(measurements);
            let next = account
                .execute_for_model(target(), Some("sonnet"), alias_continuation_body(), false)
                .await
                .unwrap();
            assert_eq!(next.response().status(), 200);
            assert_eq!(transport.authorizations.lock().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn candidate_claude_thread_alias_partial_stream_usage_is_measured_once() {
        for mode in 0..3 {
            let usage = Arc::new(BufferedAliasUsageSink::default());
            let account = executor(thread_fixture_transport(vec![200], &alias_response_body()))
                .with_usage_sink(usage.clone())
                .with_stream_transport(Arc::new(AliasUsageStreamTransport {
                    stopped: false,
                    terminal_delimited: true,
                    failure: (mode == 1).then_some(ClaudeMessagesTransportFailure::Cancelled),
                }));
            let outcome = account
                .execute_stream_for_model(target(), Some("sonnet"), alias_create_body())
                .await
                .unwrap();
            let mut stream = outcome.into_response();
            while let Some(frame) = stream.next_chunk().await {
                match frame {
                    Ok(frame) => {
                        if mode == 2
                            && std::str::from_utf8(&frame)
                                .unwrap()
                                .contains("message_delta")
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        assert_eq!(mode, 1);
                        assert_eq!(error, ClaudeMessagesTransportFailure::Cancelled);
                    }
                }
            }
            drop(stream);
            let measurements = usage.0.lock().unwrap();
            assert_eq!(
                measurements.len(),
                1,
                "EOF, cancellation and drop publish once"
            );
            assert_eq!(measurements[0].input_tokens, 7);
            assert_eq!(measurements[0].output_tokens, 5);
            assert_eq!(measurements[0].cache_read_tokens, 11);
            assert_eq!(measurements[0].cache_creation_tokens, 13);
            assert_eq!(measurements[0].total_tokens, 36);
            drop(measurements);
            let next = account
                .execute_for_model(target(), Some("sonnet"), alias_continuation_body(), false)
                .await
                .unwrap();
            assert_eq!(
                next.response().status(),
                404,
                "partial usage does not publish continuity"
            );
        }
    }

    const THREAD_MISSING_BODY: &[u8] = br#"{"type":"error","error":{"type":"not_found_error","message":"No thread state was found for the requested previous_message_id."}}"#;

    struct ThreadBootstrapTransport {
        status: u16,
        body: Vec<u8>,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ClaudeMessagesStreamingTransport for ThreadBootstrapTransport {
        fn execute_stream<'a>(
            &'a self,
            _: &'a ClaudeMessagesRequest,
            _: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            ClaudeMessagesStreamResponse,
                            ClaudeMessagesTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let (sender, receiver) = mpsc::channel(1);
                if self.status == 200 {
                    sender.try_send(Ok(b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"thread-fixture\"}}\n\n".to_vec())).unwrap();
                }
                drop(sender);
                Ok(
                    ClaudeMessagesStreamResponse::new(self.status, None, receiver)
                        .with_error_body(self.body.clone()),
                )
            })
        }
    }

    fn thread_fixture_pool(
        first: Arc<SequenceTransport>,
        second: Arc<SequenceTransport>,
        streams: Option<(
            Arc<dyn ClaudeMessagesStreamingTransport>,
            Arc<dyn ClaudeMessagesStreamingTransport>,
        )>,
    ) -> (ClaudeSubscriptionAccountPool, Arc<MemoryCooldownStore>) {
        let cooldowns = Arc::new(MemoryCooldownStore::default());
        let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
        let mut account_a = executor(first);
        let mut account_b = executor(second);
        if let Some((stream_a, stream_b)) = streams {
            account_a = account_a.with_stream_transport(stream_a);
            account_b = account_b.with_stream_transport(stream_b);
        }
        let account_a = Arc::new(
            account_a
                .with_account_state_clock(
                    "account-a",
                    conductor.clone(),
                    Arc::new(FixedAccountClock),
                )
                .unwrap(),
        );
        let account_b = Arc::new(
            account_b
                .with_account_state_clock("account-b", conductor, Arc::new(FixedAccountClock))
                .unwrap(),
        );
        let candidates = ["account-a", "account-b"]
            .into_iter()
            .map(|auth_id| AccountCandidate {
                auth_id: auth_id.to_owned(),
                provider: "claude".to_owned(),
                priority: 0,
                weight: 1,
                websocket_enabled: false,
                supported_models: Vec::new(),
                disabled: false,
            })
            .collect();
        let pool = ClaudeSubscriptionAccountPool::with_clock(
            Arc::new(AccountRouter::with_strategy(
                cooldowns.clone(),
                crate::sdk::cliproxy::auth::SchedulerStrategy::FillFirst,
            )),
            candidates,
            HashMap::from([
                ("account-a".to_owned(), account_a),
                ("account-b".to_owned(), account_b),
            ]),
            Arc::new(FixedAccountClock),
        )
        .unwrap()
        .with_targets(HashMap::from([
            ("account-a".to_owned(), target()),
            ("account-b".to_owned(), target()),
        ]))
        .unwrap();
        (pool, cooldowns)
    }

    fn thread_fixture_transport(statuses: Vec<u16>, body: &[u8]) -> Arc<SequenceTransport> {
        let mut transport = SequenceTransport::statuses(statuses);
        transport.response_body = body.to_vec();
        Arc::new(transport)
    }

    fn thread_fixture_stream(status: u16, body: &[u8]) -> Arc<ThreadBootstrapTransport> {
        Arc::new(ThreadBootstrapTransport {
            status,
            body: body.to_vec(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    #[tokio::test]
    async fn candidate_claude_thread_pool_unary_preserves_account_only_for_missing_thread() {
        for (status, body, neutral) in [
            (404, THREAD_MISSING_BODY, true),
            (
                404,
                br#"{"error":{"type":"not_found_error","message":"model not found"}}"#.as_slice(),
                false,
            ),
            (500, THREAD_MISSING_BODY, false),
        ] {
            let first = thread_fixture_transport(vec![status], body);
            let second = Arc::new(SequenceTransport::statuses(vec![200]));
            let (pool, cooldowns) = thread_fixture_pool(first.clone(), second.clone(), None);
            let outcome = pool
                .execute(target(), "sonnet", b"{}".to_vec(), false)
                .await
                .unwrap();
            if neutral {
                assert_eq!(outcome.selected_auth_id(), "account-a");
                assert_eq!(outcome.attempted_auth_ids(), ["account-a"]);
                assert_eq!(outcome.outcome().response().body(), body);
                assert!(outcome.outcome().request_scoped());
                assert!(second.authorizations.lock().unwrap().is_empty());
                assert!(cooldowns.0.lock().unwrap().is_empty());
            } else {
                assert_eq!(outcome.selected_auth_id(), "account-b");
                assert_eq!(outcome.attempted_auth_ids(), ["account-a", "account-b"]);
                assert_eq!(cooldowns.0.lock().unwrap().len(), 1);
            }
        }
    }

    #[tokio::test]
    async fn candidate_claude_thread_pool_after_401_keeps_refreshed_credential_and_cache_lane() {
        let first = thread_fixture_transport(vec![401, 404], THREAD_MISSING_BODY);
        let second = Arc::new(SequenceTransport::statuses(vec![200]));
        let (pool, cooldowns) = thread_fixture_pool(first.clone(), second.clone(), None);
        let outcome = pool
            .execute(target(), "sonnet", b"{}".to_vec(), false)
            .await
            .unwrap();
        assert_eq!(outcome.selected_auth_id(), "account-a");
        assert_eq!(outcome.attempted_auth_ids(), ["account-a"]);
        assert!(outcome.outcome().request_scoped());
        assert!(second.authorizations.lock().unwrap().is_empty());
        let headers = first.authorizations.lock().unwrap();
        assert_eq!(headers.len(), 2);
        assert!(headers[0].contains("access-old"));
        assert!(headers[1].contains("access-new"));
        assert!(cooldowns.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn candidate_claude_thread_pool_stream_bootstrap_preserves_account_and_real_failover() {
        for (body, neutral) in [
            (THREAD_MISSING_BODY, true),
            (
                br#"{"error":{"type":"not_found_error","message":"model not found"}}"#.as_slice(),
                false,
            ),
        ] {
            let first = thread_fixture_stream(404, body);
            let second = thread_fixture_stream(200, b"");
            let (pool, cooldowns) = thread_fixture_pool(
                Arc::new(SequenceTransport::statuses(vec![200])),
                Arc::new(SequenceTransport::statuses(vec![200])),
                Some((first.clone(), second.clone())),
            );
            let outcome = pool
                .execute_stream_configured("sonnet", b"{}".to_vec())
                .await
                .unwrap();
            assert_eq!(first.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
            if neutral {
                assert_eq!(outcome.selected_auth_id(), "account-a");
                assert_eq!(outcome.attempted_auth_ids(), ["account-a"]);
                assert!(outcome.outcome().request_scoped());
                assert_eq!(outcome.outcome().response().error_body(), body);
                assert_eq!(second.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
                assert!(cooldowns.0.lock().unwrap().is_empty());
            } else {
                assert_eq!(outcome.selected_auth_id(), "account-b");
                assert_eq!(second.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
                assert_eq!(cooldowns.0.lock().unwrap().len(), 1);
            }
        }
    }

    #[tokio::test]
    async fn candidate_claude_thread_pool_adapter_returns_replay_body_for_unary_and_bootstrap() {
        use crate::sdk::cliproxy::executor::RequestTerminatedError;
        use crate::sdk::pluginapi::{ExecutorRequest, ProviderExecutor};
        for stream in [false, true] {
            let first = thread_fixture_transport(vec![404], THREAD_MISSING_BODY);
            let second = Arc::new(SequenceTransport::statuses(vec![200]));
            let stream_a = thread_fixture_stream(404, THREAD_MISSING_BODY);
            let stream_b = thread_fixture_stream(200, b"");
            let (pool, cooldowns) = thread_fixture_pool(
                first,
                second.clone(),
                Some((stream_a.clone(), stream_b.clone())),
            );
            let adapter =
                crate::internal::runtime::executor::ClaudeProviderExecutor::new(Arc::new(pool));
            let request = ExecutorRequest {
                auth_id: "account-a".to_owned(),
                auth_provider: "claude".to_owned(),
                model: "sonnet".to_owned(),
                payload: b"{}".to_vec(),
                stream,
                ..ExecutorRequest::default()
            };
            let error = if stream {
                match adapter.execute_stream(request).await {
                    Err(error) => error,
                    Ok(_) => panic!("Missing thread must fail before SSE begins"),
                }
            } else {
                adapter.execute(request).await.unwrap_err()
            };
            let direct = error.downcast_ref::<RequestTerminatedError>().unwrap();
            assert_eq!(direct.http_status, 404);
            let value: Value = serde_json::from_slice(&direct.body).unwrap();
            assert_eq!(value["error"]["details"]["error_code"], "thread_not_found");
            assert!(
                crate::sdk::cliproxy::auth::conductor_execution::is_request_scoped_plugin_error(
                    &error
                )
            );
            assert!(second.authorizations.lock().unwrap().is_empty());
            assert_eq!(stream_b.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
            assert!(cooldowns.0.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn candidate_claude_thread_wrapped_auth_error_remains_request_scoped() {
        #[derive(Debug)]
        struct Wrapped(crate::sdk::cliproxy::auth::AuthError);
        impl std::fmt::Display for Wrapped {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("wrapped upstream failure")
            }
        }
        impl std::error::Error for Wrapped {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        for (status, expected) in [(404, true), (500, false)] {
            let error: crate::sdk::pluginapi::PluginExecutionError =
                Arc::new(Wrapped(crate::sdk::cliproxy::auth::AuthError {
                    code: "upstream".to_owned(),
                    message: String::from_utf8(THREAD_MISSING_BODY.to_vec()).unwrap(),
                    http_status: status,
                    ..Default::default()
                }));
            assert_eq!(
                crate::sdk::cliproxy::auth::conductor_execution::is_request_scoped_plugin_error(
                    &error
                ),
                expected
            );
            assert_eq!(
                crate::sdk::cliproxy::auth::conductor_execution::plugin_error_status(&error),
                status
            );
        }
    }

    #[tokio::test]
    async fn oauth_tool_names_are_request_local_and_round_trip() {
        let glob_alias = super::super::helps::claude_mcp_tool_alias("access-old", "glob", 0);
        let transport = Arc::new(SequenceTransport::responding(
            format!(
                r#"{{"content":[{{"type":"tool_use","id":"toolu_1","name":"{glob_alias}","input":{{}}}}]}}"#
            )
            .as_bytes(),
        ));
        let outcome = executor(Arc::clone(&transport))
            .execute_for_model(
                target(),
                Some("unknown-claude-model"),
                br#"{"model":"unknown-claude-model","messages":[{"role":"user","content":"go"}],"tools":[{"name":"Bash"},{"name":"glob"}]}"#.to_vec(),
                false,
            )
            .await
            .unwrap();

        let upstream: Value = serde_json::from_slice(&transport.bodies.lock().unwrap()[0]).unwrap();
        assert!(upstream["tools"][0]["name"]
            .as_str()
            .unwrap()
            .starts_with("mcp__"));
        assert_eq!(upstream["tools"][1]["name"], glob_alias);
        let downstream: Value = serde_json::from_slice(outcome.response().body()).unwrap();
        assert_eq!(downstream["content"][0]["name"], "glob");
    }

    #[tokio::test]
    async fn fragmented_sse_tool_names_are_restored_only_after_complete_line() {
        let (sender, receiver) = mpsc::channel(4);
        sender
            .send(Ok(b"data: {\"type\":\"content_block_start\",\"content_block\":{\"type\":\"tool_use\",\"name\":\"Ba".to_vec()))
            .await
            .unwrap();
        sender.send(Ok(b"sh\"}}\n\n".to_vec())).await.unwrap();
        drop(sender);
        let mut stream = ClaudeTrackedMessagesStreamResponse {
            response: ClaudeMessagesStreamResponse::new(200, None, receiver),
            failure_binding: None,
            model: None,
            failure_recorded: false,
            tool_name_reverse_map: HashMap::from([("Bash".to_owned(), "bash".to_owned())]),
            stream_line_buffer: Vec::new(),
            stream_eof: false,
            usage_sink: None,
            pending_transport_failure: None,
            usage_buffer: StreamUsageBuffer::default(),
            usage_published: false,
            diagnostics_state: ClaudeDiagnosticsRequestState::default(),
            diagnostics_message_id: String::new(),
            diagnostics_completed: false,
            diagnostics_committed: false,
            oauth_tool_aliases: Arc::new(ClaudeOAuthToolAliasStore::default()),
            thread_alias_keys: None,
        };
        let line = stream.next_chunk().await.unwrap().unwrap();
        assert!(String::from_utf8(line)
            .unwrap()
            .contains("\"name\":\"bash\""));
        assert!(stream.next_chunk().await.is_none());
    }

    #[tokio::test]
    async fn unauthorized_refreshes_persists_and_replays_once() {
        let transport = Arc::new(SequenceTransport::statuses(vec![401, 200]));
        let outcome = executor(Arc::clone(&transport))
            .execute(target(), b"{}".to_vec(), false)
            .await
            .unwrap();
        assert_eq!(outcome.response().status(), 200);
        assert_eq!(outcome.attempts(), 2);
        assert!(outcome.refreshed());
        assert_eq!(
            *transport.authorizations.lock().unwrap(),
            ["Bearer access-old", "Bearer access-new"]
        );
        let session_ids = transport.session_ids.lock().unwrap();
        assert_eq!(session_ids.len(), 2);
        assert_eq!(session_ids[0], session_ids[1]);
    }

    #[tokio::test]
    async fn injected_session_cache_reuses_identity_across_executor_instances() {
        let session_ids = Arc::new(SessionIdCache::new());
        let first_transport = Arc::new(SequenceTransport::statuses(vec![200]));
        let second_transport = Arc::new(SequenceTransport::statuses(vec![200]));
        executor(Arc::clone(&first_transport))
            .with_session_id_authority(Arc::clone(&session_ids), None)
            .execute(target(), b"{}".to_vec(), false)
            .await
            .unwrap();
        executor(Arc::clone(&second_transport))
            .with_session_id_authority(session_ids, None)
            .execute(target(), b"{}".to_vec(), false)
            .await
            .unwrap();
        assert_eq!(
            first_transport.session_ids.lock().unwrap()[0],
            second_transport.session_ids.lock().unwrap()[0]
        );
    }

    #[tokio::test]
    async fn second_unauthorized_is_returned_without_refresh_loop() {
        let transport = Arc::new(SequenceTransport::statuses(vec![401, 401, 200]));
        let outcome = executor(Arc::clone(&transport))
            .execute(target(), b"{}".to_vec(), false)
            .await
            .unwrap();
        assert_eq!(outcome.response().status(), 401);
        assert_eq!(outcome.attempts(), 2);
        assert_eq!(transport.authorizations.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn transport_failure_does_not_trigger_refresh() {
        let transport = Arc::new(SequenceTransport::failing(
            ClaudeMessagesTransportFailure::Timeout,
        ));
        let error = executor(transport)
            .execute(target(), b"{}".to_vec(), false)
            .await
            .unwrap_err();
        assert_eq!(
            error,
            ClaudeExecutionError::Transport(ClaudeMessagesTransportFailure::Timeout)
        );
    }

    #[tokio::test]
    async fn final_quota_response_persists_provider_retry_for_selected_account() {
        let transport = Arc::new(SequenceTransport::statuses_with_retry(
            vec![429],
            Duration::from_secs(7),
        ));
        let cooldowns = Arc::new(MemoryCooldownStore::default());
        let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
        let executor = executor(transport)
            .with_account_state_clock("account-a", conductor, Arc::new(FixedAccountClock))
            .unwrap();

        let outcome = executor
            .execute_for_model(target(), Some("sonnet"), b"{}".to_vec(), false)
            .await
            .unwrap();
        assert_eq!(outcome.response().status(), 429);
        assert_eq!(outcome.state_persisted(), Some(true));
        let records = cooldowns.0.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].auth_id, "account-a");
        assert_eq!(records[0].model.as_deref(), Some("sonnet"));
        assert_eq!(records[0].next_retry_after_ms, Some(17_000));
        assert!(records[0].last_error.as_ref().unwrap().message.is_empty());
    }

    #[tokio::test]
    async fn account_pool_retries_a_second_account_after_persisted_quota_failure() {
        let cooldowns = Arc::new(MemoryCooldownStore::default());
        let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
        let account_a = Arc::new(
            executor(Arc::new(SequenceTransport::statuses_with_retry(
                vec![429],
                Duration::from_secs(7),
            )))
            .with_account_state_clock(
                "account-a",
                Arc::clone(&conductor),
                Arc::new(FixedAccountClock),
            )
            .unwrap(),
        );
        let account_b = Arc::new(
            executor(Arc::new(SequenceTransport::statuses(vec![200])))
                .with_account_state_clock(
                    "account-b",
                    Arc::clone(&conductor),
                    Arc::new(FixedAccountClock),
                )
                .unwrap(),
        );
        let candidates = ["account-a", "account-b"]
            .into_iter()
            .map(|auth_id| AccountCandidate {
                auth_id: auth_id.to_owned(),
                provider: "claude".to_owned(),
                priority: 0,
                weight: 1,
                websocket_enabled: false,
                supported_models: Vec::new(),
                disabled: false,
            })
            .collect::<Vec<_>>();
        let executors = HashMap::from([
            ("account-a".to_owned(), account_a),
            ("account-b".to_owned(), account_b),
        ]);
        let router = Arc::new(AccountRouter::new(cooldowns.clone()));
        let pool = ClaudeSubscriptionAccountPool::with_clock(
            router,
            candidates,
            executors,
            Arc::new(FixedAccountClock),
        )
        .unwrap();

        let outcome = pool
            .execute(target(), "sonnet", b"{}".to_vec(), false)
            .await
            .unwrap();
        assert_eq!(outcome.selected_auth_id(), "account-b");
        assert_eq!(outcome.attempted_auth_ids(), ["account-a", "account-b"]);
        assert_eq!(outcome.outcome().response().status(), 200);
        let records = cooldowns.0.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].auth_id, "account-a");
        assert_eq!(records[0].next_retry_after_ms, Some(17_000));
    }

    #[tokio::test]
    async fn stream_pool_retries_before_message_start_and_returns_bootstrapped_account() {
        let cooldowns = Arc::new(MemoryCooldownStore::default());
        let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
        let before_start_error = Arc::new(FixedStreamingTransport {
            status: 200,
            chunks: vec![Ok(
                b"data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\n\n"
                    .to_vec(),
            )],
        });
        let success = Arc::new(FixedStreamingTransport {
            status: 200,
            chunks: vec![
                Ok(
                    b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_b\"}}\n\n"
                        .to_vec(),
                ),
                Ok(b"data: {\"type\":\"message_stop\"}\n\n".to_vec()),
            ],
        });
        let account_a = Arc::new(
            executor(Arc::new(SequenceTransport::statuses(vec![200])))
                .with_stream_transport(before_start_error)
                .with_account_state_clock(
                    "account-a",
                    Arc::clone(&conductor),
                    Arc::new(FixedAccountClock),
                )
                .unwrap(),
        );
        let account_b = Arc::new(
            executor(Arc::new(SequenceTransport::statuses(vec![200])))
                .with_stream_transport(success)
                .with_account_state_clock(
                    "account-b",
                    Arc::clone(&conductor),
                    Arc::new(FixedAccountClock),
                )
                .unwrap(),
        );
        let candidates = ["account-a", "account-b"]
            .into_iter()
            .map(|auth_id| AccountCandidate {
                auth_id: auth_id.to_owned(),
                provider: "claude".to_owned(),
                priority: 0,
                weight: 1,
                websocket_enabled: false,
                supported_models: Vec::new(),
                disabled: false,
            })
            .collect::<Vec<_>>();
        let targets = HashMap::from([
            ("account-a".to_owned(), target()),
            ("account-b".to_owned(), target()),
        ]);
        let pool = ClaudeSubscriptionAccountPool::with_clock(
            Arc::new(AccountRouter::new(cooldowns.clone())),
            candidates,
            HashMap::from([
                ("account-a".to_owned(), account_a),
                ("account-b".to_owned(), account_b),
            ]),
            Arc::new(FixedAccountClock),
        )
        .unwrap()
        .with_targets(targets)
        .unwrap();

        let mut outcome = pool
            .execute_stream_configured("sonnet", b"{}".to_vec())
            .await
            .unwrap();
        assert_eq!(outcome.selected_auth_id(), "account-b");
        assert_eq!(outcome.attempted_auth_ids(), ["account-a", "account-b"]);
        let first = outcome
            .outcome_mut()
            .response_mut()
            .next_chunk()
            .await
            .unwrap()
            .unwrap();
        assert!(String::from_utf8_lossy(&first).contains("message_start"));
        let records = cooldowns.0.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].auth_id, "account-a");
        assert_eq!(
            records[0].last_error.as_ref().unwrap().http_status,
            Some(502)
        );
    }

    #[tokio::test]
    async fn post_bootstrap_transport_failure_cools_account_for_future_requests() {
        let cooldowns = Arc::new(MemoryCooldownStore::default());
        let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
        let streaming = Arc::new(FixedStreamingTransport {
            status: 200,
            chunks: vec![
                Ok(
                    b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_a\"}}\n\n"
                        .to_vec(),
                ),
                Err(ClaudeMessagesTransportFailure::Protocol),
            ],
        });
        let executor = executor(Arc::new(SequenceTransport::statuses(vec![200])))
            .with_stream_transport(streaming)
            .with_account_state_clock(
                "account-a",
                Arc::clone(&conductor),
                Arc::new(FixedAccountClock),
            )
            .unwrap();
        let outcome = executor
            .execute_stream_for_model(target(), Some("sonnet"), b"{}".to_vec())
            .await
            .unwrap();
        assert!(cooldowns.0.lock().unwrap().is_empty());
        let mut stream = outcome.into_response();
        assert!(stream.next_chunk().await.unwrap().is_ok());
        assert_eq!(
            stream.next_chunk().await.unwrap(),
            Err(ClaudeMessagesTransportFailure::Protocol)
        );
        let records = cooldowns.0.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].auth_id, "account-a");
        assert_eq!(records[0].model.as_deref(), Some("sonnet"));
        assert_eq!(
            records[0].last_error.as_ref().unwrap().http_status,
            Some(502)
        );
    }
}
