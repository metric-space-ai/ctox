// Origin: CTOX
// License: AGPL-3.0-only

pub mod aistudio_executor;
pub mod antigravity_executor;
pub mod antigravity_executor_auth;
pub mod antigravity_executor_credits;
pub mod antigravity_executor_execute;
pub mod antigravity_executor_request;
pub mod antigravity_executor_stream;
pub mod antigravity_executor_tokens;
pub mod antigravity_reasoning_replay;
pub mod claude_executor;
pub mod claude_executor_auth;
pub mod claude_executor_cloaking;
pub mod claude_executor_diagnostics;
pub mod claude_executor_execute;
pub mod claude_executor_fast_error;
pub mod claude_executor_request;
pub mod claude_executor_stream;
pub mod claude_executor_tokens;
pub mod claude_signing;
pub mod codex_executor;
pub mod codex_executor_auth;
pub mod codex_executor_execute;
pub mod codex_executor_reasoning;
pub mod codex_executor_request;
pub mod codex_executor_stream;
pub mod codex_executor_terminal;
pub mod codex_executor_tokens;
pub mod codex_openai_images;
pub mod codex_websockets_connection;
pub mod codex_websockets_errors;
pub mod codex_websockets_execute;
pub mod codex_websockets_executor;
pub mod codex_websockets_request;
pub mod codex_websockets_session;
pub mod codex_websockets_stream;
pub mod devin_executor_history;
pub mod devin_executor_response;
pub mod gemini_executor;
pub mod gemini_vertex_executor;
pub mod helps;
pub mod kimi_executor;
pub mod kimi_thinking_replay;
pub mod openai_compat_executor;
pub mod openai_responses_signature;
pub mod xai_executor;
pub mod xai_executor_auth;
pub mod xai_executor_execute;
pub mod xai_executor_media;
pub mod xai_executor_request;
pub mod xai_executor_response;
pub mod xai_executor_stream;
pub mod xai_executor_tokens;
pub mod xai_reasoning_replay;
pub mod xai_websockets_executor;

#[cfg(test)]
mod aistudio_executor_test;
#[cfg(test)]
mod antigravity_executor_buildrequest_test;
#[cfg(test)]
mod antigravity_executor_credits_test;
#[cfg(test)]
mod antigravity_executor_interactions_test;
#[cfg(test)]
mod antigravity_executor_signature_test;
#[cfg(test)]
mod antigravity_reasoning_replay_clear_test;
#[cfg(test)]
mod antigravity_reasoning_replay_test;
#[cfg(test)]
mod antigravity_refresh_test;
#[cfg(test)]
mod antigravity_schema_sanitize_test;
#[cfg(test)]
mod caching_verify_test;
#[cfg(test)]
mod claude_executor_auth_race_test;
#[cfg(test)]
mod claude_executor_auth_test;
#[cfg(test)]
mod claude_executor_beta_policy_test;
#[cfg(test)]
mod claude_executor_diagnostics_test;
#[cfg(test)]
mod claude_executor_fast_error_test;
#[cfg(test)]
mod claude_executor_request_bench_test;
#[cfg(test)]
mod claude_executor_request_remap_test;
#[cfg(test)]
mod claude_executor_thinking_signature_test;
#[cfg(test)]
mod claude_executor_wire_casing_test;
#[cfg(test)]
mod claude_signing_test;
#[cfg(test)]
mod codex_executor_cache_test;
#[cfg(test)]
mod codex_executor_compact_test;
#[cfg(test)]
mod codex_executor_imagegen_test;
#[cfg(test)]
mod codex_executor_input_ids_test;
#[cfg(test)]
mod codex_executor_instructions_test;
#[cfg(test)]
mod codex_executor_parallel_tool_calls_test;
#[cfg(test)]
mod codex_executor_reasoning_replay_cache_test;
#[cfg(test)]
mod codex_executor_retry_test;
#[cfg(test)]
mod codex_executor_signature_test;
#[cfg(test)]
mod codex_executor_spawn_agent_test;
#[cfg(test)]
mod codex_executor_stream_output_test;
#[cfg(test)]
mod codex_executor_translate_test;
#[cfg(test)]
mod codex_openai_images_extract_test;
#[cfg(test)]
mod codex_openai_images_test;
#[cfg(test)]
mod codex_websockets_executor_store_test;
#[cfg(test)]
mod codex_websockets_executor_test;
#[cfg(test)]
mod codex_websockets_spawn_agent_test;
#[cfg(test)]
mod devin_executor_history_test;
#[cfg(test)]
mod devin_executor_response_test;
#[cfg(test)]
mod executor_payload_optimization_test;
#[cfg(test)]
mod gemini_executor_test;
#[cfg(test)]
mod home_codex_terminal_test;
#[cfg(test)]
mod kimi_executor_test;
#[cfg(test)]
mod kimi_thinking_replay_test;
#[cfg(test)]
mod openai_compat_executor_compact_test;
#[cfg(test)]
mod openai_compat_executor_tool_results_test;
#[cfg(test)]
mod openai_responses_signature_test;
#[cfg(test)]
mod websocket_lifecycle_bind_test;
#[cfg(test)]
mod websocket_session_target_test;
#[cfg(test)]
mod xai_executor_test;
#[cfg(test)]
mod xai_status_err_test;
#[cfg(test)]
mod xai_websockets_executor_test;

pub use aistudio_executor::{AiStudioExecutor, AiStudioExecutorError};
pub use antigravity_executor::{
    AntigravityGenerateRequest, AntigravityGenerateResponse, AntigravityGenerateStreamResponse,
    AntigravityGenerateStreamingTransport, AntigravityGenerateTransport,
    AntigravityGenerateTransportFailure, AntigravityResponsesStream, AntigravityTargetError,
    AntigravityUpstreamTarget, ANTIGRAVITY_GENERATE_PATH, ANTIGRAVITY_MODELS_PATH,
    ANTIGRAVITY_STREAM_PATH, ANTIGRAVITY_USER_AGENT, DEFAULT_ANTIGRAVITY_BASE_URL,
};
pub use antigravity_executor_auth::{
    AntigravityAuthClock, AntigravityRefreshOutcome, AntigravitySubscriptionAuth,
    AntigravitySubscriptionAuthError, SystemAntigravityAuthClock,
};
pub use antigravity_executor_credits::*;
pub use antigravity_executor_execute::{
    AntigravityAccessTokenFingerprintSink, AntigravityAccountPoolError, AntigravityExecutionError,
    AntigravityExecutionOutcome, AntigravityPooledExecutionOutcome,
    AntigravityPooledStreamExecutionOutcome, AntigravityStreamExecutionOutcome,
    AntigravitySubscriptionAccountPool, AntigravitySubscriptionExecutor,
    AntigravityTrackedResponsesStream,
};
#[cfg(feature = "antigravity-http-transport")]
pub use antigravity_executor_request::AntigravityGenerateHttpTransport;
pub use antigravity_executor_request::{
    prepare_antigravity_generate_body, AntigravityRequestError,
};
pub use antigravity_executor_tokens::{AntigravityTokenCountError, AntigravityTokenCounter};
pub use claude_executor::{
    decode_claude_response_body, parse_claude_stream_usage_line, parse_claude_usage,
    ClaudeAuthorizationHeader, ClaudeCredentialMode, ClaudeDeviceProfile, ClaudeMessagesRequest,
    ClaudeMessagesResponse, ClaudeMessagesStreamResponse, ClaudeMessagesStreamingTransport,
    ClaudeMessagesTransport, ClaudeMessagesTransportFailure, ClaudeOAuthCancellationError,
    ClaudePreparedAuthorization, ClaudeRequestFingerprint, ClaudeResponseDecodeError,
    ClaudeResponseEncoding, ClaudeTargetError, ClaudeUpstreamTarget, ClaudeUsage, ClaudeUsageSink,
};
pub use claude_executor_auth::{
    prepare_claude_request_auth, should_prepare_claude_request_auth, ClaudeOAuthProfile,
    ClaudeOAuthProfileFetcher, ClaudePrepareAuthError, ClaudeRefreshOutcome,
    ClaudeRequestAuthPreparer, ClaudeSubscriptionAuth, ClaudeSubscriptionAuthError,
    CLAUDE_ACCOUNT_PROFILE_CHECKED_AT_KEY,
};
pub use claude_executor_cloaking::{
    apply_claude_cloaking, claude_cch_fallback_billing_header, compute_claude_fingerprint,
    count_claude_cache_controls, enforce_claude_cache_control_limit, ensure_claude_cache_control,
    generate_claude_billing_header, inject_claude_system_instructions, inject_fake_claude_user_id,
    normalize_claude_cache_control_ttl, parse_claude_entrypoint,
    relocate_claude_system_prompt_for_count_tokens, sanitize_forwarded_claude_system_prompt,
    ClaudeCallerSystemBlockError, ClaudeCloakPolicy,
};
pub use claude_executor_diagnostics::{
    claude_message_id_from_response, claude_message_id_from_sse, commit_claude_diagnostics,
    inject_claude_diagnostics, observe_claude_stream_line, ClaudeDiagnosticsRequestState,
};
pub use claude_executor_execute::{
    AccountStateClock, ClaudeAccountPoolError, ClaudeExecutionError, ClaudeExecutionOutcome,
    ClaudeExecutionRequestContext, ClaudePooledExecutionOutcome,
    ClaudePooledStreamExecutionOutcome, ClaudeStreamExecutionOutcome,
    ClaudeSubscriptionAccountPool, ClaudeSubscriptionMessagesExecutor,
    ClaudeTrackedMessagesStreamResponse,
};
pub use claude_executor_fast_error::{
    classify_claude_upstream_error, claude_body_indicates_fast_mode_credits,
    claude_fast_direct_response_error, wrap_claude_fast_request_error, ClaudeEntitlementError,
    ClaudeFastRequestError,
};
#[cfg(feature = "anthropic-fingerprint-transport")]
pub use claude_executor_request::ClaudeMessagesHttpTransport;
pub use claude_executor_request::{
    append_claude_fast_mode_beta, apply_claude_tool_prefix, claude_code_cli_betas,
    claude_count_tokens_betas, claude_request_uses_fast_mode, claude_requested_betas,
    claude_wire_header_name, disable_claude_thinking_if_tool_choice_forced,
    ensure_claude_model_max_tokens, extract_and_remove_claude_betas,
    normalize_claude_sampling_for_upstream, prepare_claude_first_party_count_tokens_body,
    prepare_claude_oauth_tool_names_for_upstream, prepare_claude_upstream_body,
    prepare_claude_upstream_body_with_identity, rebuild_mid_claude_system_messages,
    remap_claude_oauth_tool_names, remap_claude_oauth_tool_names_with_secret,
    restore_claude_oauth_tool_names_from_response,
    restore_claude_oauth_tool_names_from_stream_line, reverse_remap_claude_oauth_tool_names,
    reverse_remap_claude_oauth_tool_names_from_stream_line, sanitize_claude_web_search_domains,
    strip_claude_tool_prefix_from_response, strip_claude_tool_prefix_from_stream_line,
    with_claude_oauth_credential_betas,
};
pub use claude_executor_stream::{ClaudeProviderExecutor, ClaudeProviderExecutorError};
pub use claude_executor_tokens::{
    claude_first_party_token_count_headers, claude_token_count_response,
    prepare_claude_first_party_token_count_body, validate_claude_token_count_request,
    ClaudeFirstPartyTokenCountBody, ClaudeTokenCountError,
};
pub use claude_signing::{
    claude_cch_signing_enabled, finalize_anthropic_messages_body_cch, normalize_claude_cch_input,
    sign_anthropic_messages_body, ClaudeCchUpstreamKind,
};
pub use codex_executor::{
    CodexResponsesRequest, CodexResponsesResponse, CodexResponsesStreamResponse,
    CodexResponsesStreamingTransport, CodexResponsesTransport, CodexResponsesTransportFailure,
    CodexTargetError, CodexUpstreamTarget, CODEX_ORIGINATOR, CODEX_USER_AGENT,
    DEFAULT_CODEX_BASE_URL,
};
pub use codex_executor_auth::{
    CodexRefreshOutcome, CodexSubscriptionAuth, CodexSubscriptionAuthError,
};
pub use codex_executor_execute::{
    CodexAccountPoolError, CodexExecutionError, CodexExecutionOutcome, CodexPooledExecutionOutcome,
    CodexPooledStreamExecutionOutcome, CodexStreamExecutionOutcome, CodexSubscriptionAccountPool,
    CodexSubscriptionResponsesExecutor, CodexTrackedResponsesStreamResponse,
};
pub use codex_executor_reasoning::{
    codex_reasoning_replay_session_key, insert_codex_reasoning_replay_items,
    CodexReasoningReplayCache, CodexReasoningReplayScope,
};
#[cfg(feature = "codex-http-transport")]
pub use codex_executor_request::CodexResponsesHttpTransport;
pub use codex_executor_request::{
    apply_codex_cloaking_headers, apply_codex_identity_confuse_body,
    apply_codex_identity_confuse_headers, prepare_codex_compact_body, prepare_codex_responses_body,
    CodexHeaderPolicy, CodexIdentityConfuseState, CodexIdentityPolicy, CodexRequestError,
    CodexRequestPolicy,
};
pub use codex_executor_stream::{CodexSseTerminalStream, CodexStreamTerminal};
pub use codex_executor_terminal::{
    codex_terminal_status, parse_codex_retry_after, parse_codex_terminal_error,
    CodexIncompleteStreamError, CodexTerminalAccumulator, CodexTerminalError, CodexTerminalEvent,
    CODEX_INCOMPLETE_STREAM_MESSAGE,
};
pub use codex_executor_tokens::{
    codex_token_count_response, count_codex_input_tokens, CodexTokenCountError,
};
pub use codex_openai_images::{
    build_codex_image_sse_frame, build_codex_images_api_response, codex_direct_image_model,
    codex_is_images_endpoint_path, codex_openai_image_base_model, extract_codex_image_results,
    prepare_codex_direct_image_request, prepare_codex_openai_image_request,
    CodexDirectImagePreparedRequest, CodexImageAction, CodexImageError, CodexImagePreparedRequest,
    CodexImageResponseFormat, CodexImageResult, CODEX_DIRECT_IMAGE_EDIT_PATH,
    CODEX_DIRECT_IMAGE_GENERATION_PATH, CODEX_IMAGE_EDIT_PATH, CODEX_IMAGE_GENERATION_PATH,
};
pub use codex_websockets_connection::{
    build_codex_responses_websocket_url, build_codex_websocket_request_body,
    map_codex_websocket_close, normalize_codex_websocket_parallel_tool_calls,
    CodexWebsocketConnection, CodexWebsocketFrame, CodexWebsocketTransport,
};
pub use codex_websockets_errors::{
    encode_codex_websocket_as_sse, is_codex_websocket_connection_limit_error,
    normalize_codex_websocket_completion, parse_codex_websocket_error, CodexWebsocketError,
};
pub use codex_websockets_execute::codex_websocket_result_committed;
pub use codex_websockets_executor::{
    codex_replay_session_from_request, codex_websockets_enabled, CodexWebsocketExecutionRequest,
    CodexWebsocketExecutionResult, CodexWebsocketsExecutor,
};
pub use codex_websockets_request::{
    apply_codex_prompt_cache_headers, apply_codex_websocket_headers, codex_session_header_value,
    ensure_codex_websocket_session_header, CodexWebsocketHeaderDefaults, CodexWebsocketHeaders,
};
pub use codex_websockets_session::{CodexWebsocketSession, CodexWebsocketSessionStore};
pub use codex_websockets_stream::CodexWebsocketStream;
pub use gemini_executor::{GeminiExecutor, GeminiExecutorConfig, GeminiExecutorError};
pub use gemini_vertex_executor::{
    GeminiVertexExecutor, VertexAccessTokenProvider, VertexExecutorError,
};
pub use helps::normalize_codex_tool_integer_types_for_executor;
pub use openai_compat_executor::{
    normalize_openai_tool_results_text_only, openai_compat_image_endpoint_path,
    prepare_openai_compat_images_payload, rewrite_openai_compat_images_multipart_payload,
    should_normalize_openai_tool_results_for_model, OpenAiCompatConfig, OpenAiCompatError,
    OpenAiCompatExecutor, OpenAiCompatPayloadModelRule, OpenAiCompatPayloadRule,
    OpenAiCompatibility, OpenAiCompatibilityModel, OPENAI_COMPAT_DEFAULT_IMAGE_ENDPOINT,
    OPENAI_COMPAT_IMAGES_EDITS_PATH, OPENAI_COMPAT_IMAGES_GENERATIONS_PATH,
    OPENAI_COMPAT_IMAGE_HANDLER_TYPE, OPENAI_COMPAT_MAX_STREAM_LINE_BYTES,
};
pub use openai_responses_signature::sanitize_openai_responses_reasoning_encrypted_content;
pub use xai_executor::*;
pub use xai_executor_auth::*;
pub use xai_executor_execute::*;
pub use xai_executor_media::*;
pub use xai_executor_request::*;
pub use xai_executor_response::*;
pub use xai_executor_stream::*;
pub use xai_executor_tokens::*;
pub use xai_reasoning_replay::*;
pub use xai_websockets_executor::*;
