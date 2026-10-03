// Origin: CTOX
// License: AGPL-3.0-only

pub mod antigravity_credits;
mod api_key_model_capabilities;
pub mod auto_refresh_loop;
pub mod classification;
pub mod conductor;
pub mod conductor_cooldown;
pub mod conductor_execution;
#[cfg(test)]
mod conductor_generic_execution_test;
pub mod conductor_home;
pub mod conductor_home_execution;
pub mod conductor_lifecycle;
pub mod conductor_models;
pub mod conductor_refresh;
pub mod conductor_selection;
pub mod conductor_stream;
pub mod config_apikey;
pub mod cooldown_state;
pub mod credential_policy;
pub mod custom_headers;
pub mod error_events;
pub mod errors;
pub mod home_concurrency;
pub mod home_in_flight_publisher;
mod home_model_capabilities;
pub mod home_result;
pub mod home_selection;
pub mod home_session_alias;
pub mod oauth_model_alias;
pub mod persist_policy;
pub mod response_model_rewriter;
pub mod scheduler;
pub mod selector;
pub mod session_cache;
pub mod status;
pub mod store;
pub mod token_fingerprint;
pub mod types;
pub mod weight;

pub use antigravity_credits::{
    AntigravityCreditsClock, AntigravityCreditsHint, AntigravityCreditsHints,
    AntigravityCreditsRequest, AntigravityCreditsStore, AntigravityCreditsStoreError,
};
pub use api_key_model_capabilities::{
    resolved_api_key_model_info, resolved_model_info, ApiKeyModelRoutingSnapshot,
};
pub use auto_refresh_loop::{
    next_refresh_check_at, AuthRefresherResolver, AutoRefreshClock, AutoRefreshConfig,
    AutoRefreshWorker, RefreshSchedule, SystemAutoRefreshClock,
};
pub use classification::{AuthKind, AuthSourceKind};
pub use conductor::{
    AuthManager, AuthManagerError, AuthPreparationError, AuthPreparer, ExecutionSessionCloser,
    ManagerRefreshPublicationSink, ProviderDispatchError, ProviderExecutorRegistration,
    ProviderExecutorRegistrationError, ProviderExecutorRegistry, UnauthorizedReplayDecision,
    UnauthorizedReplayState, CLOSE_ALL_EXECUTION_SESSIONS_ID,
};
pub use conductor_cooldown::{AccountExecutionResult, CooldownConductor};
#[cfg(test)]
pub(crate) use conductor_execution::plugin_error_status;
pub(crate) use conductor_execution::{
    is_claude_oauth_request_cancellation, is_request_scoped_plugin_error,
    is_unauthorized_plugin_error,
};
pub use conductor_execution::{
    publish_selected_auth_metadata, usage_context_with_requested_model_alias, AccountRouter,
    AccountRoutingError, GenericAuthRuntime, GenericConductorClock, GenericExecutionError,
    SystemGenericConductorClock,
};
pub use conductor_home::{
    HomeAuthRuntime, HomeClock, HomeDispatchBundle, HomeDispatchError, HomeSelectedAuthPublisher,
    HomeSelectionRequest, HomeTransportFailure, SystemHomeClock,
};
pub use conductor_home_execution::{prepare_executor_request, HomeExecutionError};
pub use conductor_lifecycle::{
    AuthLifecycle, AuthLifecycleError, AuthLifecycleRefreshError, ModelResumeSink,
};
pub use conductor_models::{
    finish_force_mapped_stream_chunks, rewrite_force_mapped_response,
    rewrite_force_mapped_stream_chunk,
};
pub use conductor_refresh::{
    access_token, has_refresh_credential, has_unauthorized_auth_failure, last_refresh_time,
    parse_duration, preferred_refresh_interval, should_refresh, AuthRefresher, HomeRefreshError,
    RefreshCancellation, RefreshCoordinator, RefreshExecutorError, RefreshOutcome,
    RefreshTransactionError,
};
pub use conductor_selection::{
    AuthSchedulerView, SchedulerCapabilities, SchedulerCapabilitySource, SchedulerViewError,
};
pub(crate) use conductor_stream::stream_tail_is_availability_neutral;
pub use conductor_stream::{
    is_request_terminated_error, should_attempt_antigravity_credits_fallback,
};
pub use config_apikey::is_config_api_key_auth;
pub use cooldown_state::{
    CooldownErrorState, CooldownQuotaState, CooldownStateRecord, CooldownStateStore,
    CooldownStoreError,
};
pub use credential_policy::{
    credential_policy_allows, normalize_credential_policy, CREDENTIAL_POLICY_CODEX_ALPHA_SEARCH_V1,
};
pub use custom_headers::{
    apply_custom_headers_from_metadata, extract_custom_headers_from_metadata,
};
pub use error_events::{
    build_error_event_payload, AuthExecutionResult, ErrorEventClock, ErrorEventPublisher,
    ErrorEventSink, ErrorEventSinkError,
};
pub use errors::AuthError;
pub use home_concurrency::{
    canonical_home_concurrency_model_key, decode_home_concurrency, decode_home_dispatch_error,
    install_home_concurrency_scope, validate_home_concurrency_tuple,
    verify_home_concurrency_identity, HomeConcurrencyBusyError, HomeConcurrencyError,
    HomeConcurrencyTuple, HomeDispatchStatusError,
};
pub use home_in_flight_publisher::{
    encode_home_in_flight_freeze, HomeInFlightPublisher, HomeInFlightPublisherConfig,
    HomeInFlightTransport, HomePublisherConfigError,
};
pub use home_selection::{HomeAttemptLease, HomeDispatchSelection};
pub use home_session_alias::{HomeSessionAliasCache, DEFAULT_HOME_SESSION_ALIAS_TTL};
pub use oauth_model_alias::{
    model_alias_channel, model_alias_lookup_candidates, oauth_model_alias_channel,
    oauth_model_aliases_from_attributes, preserve_resolved_model_suffix,
    resolve_model_alias_pool_from_config_models, resolve_model_alias_result_from_config_models,
    resolve_upstream_model_from_aliases, sanitize_oauth_model_aliases,
    set_oauth_model_aliases_attribute, ModelAliasEntry, OAuthModelAliasResult,
    OAuthModelAliasTable,
};
pub use persist_policy::{should_persist, AuthMutationOptions, PersistenceIntent};
pub use response_model_rewriter::{
    normalize_glued_sse_events, rewrite_model_in_response, rewrite_sse_payload_lines,
    StreamRewriteOptions, StreamRewriter,
};
pub use scheduler::{
    canonical_model_key, AuthScheduler, ScheduledAccount, SchedulerPickOptions, SchedulerStrategy,
};
pub use selector::{
    AccountCandidate, AccountSelectionError, FillFirstSelector, RoundRobinSelector,
    WeightedRoundRobinSelector,
};
pub use session_cache::{
    compact_home_session_aliases, compact_session_aliases, SessionCache, SessionClock,
    SystemSessionClock,
};
pub use status::AuthStatus;
pub use store::{AuthStore, AuthStoreError};
pub use token_fingerprint::{
    access_token_sha256, notify_access_token_fingerprint, AccessTokenFingerprintObservation,
    AccessTokenFingerprintObserver,
};
pub use types::{
    provider_refresh_lead, register_refresh_lead_provider, Auth, ModelState, PostAuthContext,
    PostAuthError, PostAuthHook, QuotaState, RecentRequestBucket, RefreshLeadRuntime, RequestInfo,
    SharedAuthRuntime, ATTRIBUTE_AUTH_INDEX_SEED,
};
pub use weight::validate_auth_weight;

#[cfg(test)]
mod antigravity_credits_test;
#[cfg(test)]
mod api_key_model_alias_test;
#[cfg(test)]
mod api_key_model_capabilities_test;
#[cfg(test)]
mod auto_refresh_loop_test;
#[cfg(test)]
mod candidate_token_fingerprint_test;
#[cfg(test)]
mod classification_test;
#[cfg(test)]
mod codex_forcemap_ws_forward_test;
#[cfg(test)]
mod codex_oauth_model_capabilities_test;
#[cfg(test)]
mod conductor_availability_test;
#[cfg(test)]
mod conductor_claude_cancellation_test;
#[cfg(test)]
mod conductor_credits_candidates_test;
#[cfg(test)]
mod conductor_executor_replace_test;
#[cfg(test)]
mod conductor_fast_error_test;
#[cfg(test)]
mod conductor_force_mapping_test;
#[cfg(test)]
mod conductor_oauth_alias_suspension_test;
#[cfg(test)]
mod conductor_overrides_test;
#[cfg(test)]
mod conductor_recent_requests_test;
#[cfg(test)]
mod conductor_remove_test;
#[cfg(test)]
mod conductor_scheduler_refresh_test;
#[cfg(test)]
mod conductor_unauthorized_refresh_test;
#[cfg(test)]
mod conductor_update_test;
#[cfg(test)]
mod conductor_usage_test;
#[cfg(test)]
mod conductor_weight_validation_test;
#[cfg(test)]
mod config_apikey_test;
#[cfg(test)]
mod cooldown_backoff_test;
#[cfg(test)]
mod cooldown_state_test;
#[cfg(test)]
mod custom_headers_test;
#[cfg(test)]
mod error_events_test;
#[cfg(test)]
mod errors_compat_test;
#[cfg(test)]
mod force_mapping_live_fixtures_test;
#[cfg(test)]
mod home_concurrency_test;
#[cfg(test)]
mod home_dispatch_headers_test;
#[cfg(test)]
mod home_execution_paths_test;
#[cfg(test)]
mod home_fallback_audit_test;
#[cfg(test)]
mod home_force_mapping_test;
#[cfg(test)]
mod home_in_flight_publisher_test;
#[cfg(test)]
mod home_retry_loop_test;
#[cfg(test)]
mod home_selected_auth_callback_test;
#[cfg(test)]
mod home_selection_attempt_test;
#[cfg(test)]
mod home_selection_test;
#[cfg(test)]
mod home_session_alias_test;
#[cfg(test)]
mod home_unauthorized_refresh_test;
#[cfg(test)]
mod home_websocket_reuse_test;
#[cfg(test)]
mod oauth_model_alias_test;
#[cfg(test)]
mod openai_compat_pool_test;
#[cfg(test)]
mod persist_policy_test;
#[cfg(test)]
mod request_auth_prepare_test;
#[cfg(test)]
mod request_termination_test;
#[cfg(test)]
mod response_model_rewriter_antigravity_sim_test;
#[cfg(test)]
mod response_model_rewriter_test;
#[cfg(test)]
mod scheduler_benchmark_test;
#[cfg(test)]
mod scheduler_test;
#[cfg(test)]
mod selected_auth_metadata_test;
#[cfg(test)]
mod selector_test;
#[cfg(test)]
mod types_test;
#[cfg(test)]
mod weight_test;
