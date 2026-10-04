// Origin: CTOX
// License: AGPL-3.0-only

mod antigravity_grounding_urls;
mod apply_patch;
mod apply_patch_responses;
mod cache_helpers;
mod claude_builtin_tools;
mod claude_client_detection;
mod claude_code_session;
mod claude_credential_identity;
mod claude_device_profile;
mod claude_diagnostics;
mod claude_input_tokens;
mod claude_mcp_alias;
mod claude_upstream;
mod cloak_obfuscate;
mod cloak_utils;
mod codex_input_ids;
mod codex_multi_agent_v2;
mod codex_tool_schema;
mod derived_session;
pub mod devin_models;
pub mod devin_proto;
pub mod devin_request;
pub mod devin_wire;
mod home_refresh;
mod json_retry_helpers;
mod kimi_responses;
mod logging_helpers;
mod model_capabilities;
mod openai_compat_tool_results;
pub mod payload_helpers;
mod payload_mutations;
mod proxy_helpers;
mod session_id_cache;
mod thinking;
mod thinking_providers;
mod token_helpers;
mod usage_helpers;
mod user_id_cache;
mod utls_client;
mod vertex_payload_helpers;

pub(crate) use usage_helpers::UsageReporter;

#[cfg(test)]
mod antigravity_grounding_urls_test;
#[cfg(test)]
mod cache_helpers_test;
#[cfg(test)]
mod claude_builtin_tools_test;
#[cfg(test)]
mod claude_client_detection_test;
#[cfg(test)]
mod claude_code_session_test;
#[cfg(test)]
mod claude_credential_identity_race_test;
#[cfg(test)]
mod claude_credential_identity_test;
#[cfg(test)]
mod claude_device_profile_test;
#[cfg(test)]
mod claude_diagnostics_test;
#[cfg(test)]
mod claude_input_tokens_test;
#[cfg(test)]
mod claude_mcp_alias_test;
#[cfg(test)]
mod claude_upstream_test;
#[cfg(test)]
mod codex_input_ids_test;
#[cfg(test)]
mod codex_multi_agent_v2_compat_test;
#[cfg(test)]
mod codex_tool_schema_test;
#[cfg(test)]
mod derived_session_test;
#[cfg(test)]
mod devin_models_test;
#[cfg(test)]
mod devin_proto_test;
#[cfg(test)]
mod devin_request_test;
#[cfg(test)]
mod devin_wire_test;
#[cfg(test)]
mod home_refresh_test;
#[cfg(test)]
mod logging_helpers_test;
#[cfg(test)]
mod model_capabilities_test;
#[cfg(test)]
mod openai_compat_tool_results_test;
#[cfg(test)]
mod payload_helpers_disable_image_generation_test;
#[cfg(test)]
mod payload_mutations_test;
#[cfg(test)]
mod proxy_helpers_test;
#[cfg(test)]
mod session_id_cache_test;
#[cfg(test)]
mod thinking_test;
#[cfg(test)]
mod usage_helpers_test;
#[cfg(test)]
mod usage_stream_benchmark_test;
#[cfg(test)]
mod user_id_cache_test;
#[cfg(test)]
mod utls_client_resumption_test;
#[cfg(test)]
mod utls_client_test;
#[cfg(test)]
mod vertex_payload_helpers_test;

pub use antigravity_grounding_urls::{
    is_antigravity_vertex_search_redirect, resolve_antigravity_grounding_urls,
    GroundingRedirectError, GroundingRedirectResponse, GroundingRedirectTransport,
};
pub use apply_patch::{
    apply_patch_original_request, apply_patch_requested, is_apply_patch_upstream_tool,
    APPLY_PATCH_UPSTREAM_ERROR_MESSAGE,
};
pub use apply_patch_responses::normalize_apply_patch_responses_request;
pub use cache_helpers::{codex_prompt_cache_key, CodexCache, CodexPromptCacheStore};
pub use claude_builtin_tools::{augment_claude_builtin_tool_registry, is_claude_server_tool_type};
pub use claude_client_detection::{detect_claude_code_request, ClaudeCodeRequestDetection};
pub use claude_code_session::{
    claude_code_execution_scope, claude_code_prompt_cache, extract_claude_code_agent_id,
    extract_claude_code_session_id, header_value_case_insensitive, header_values_case_insensitive,
    CLAUDE_CODE_AGENT_HEADER, CLAUDE_CODE_MAIN_AGENT_ID, CLAUDE_CODE_SESSION_HEADER,
};
pub use claude_credential_identity::{
    apply_claude_credential_metadata, claude_agent_session_uuid,
    claude_agent_session_uuid_for_request, claude_credential_account_uuid,
    ensure_claude_credential_device_pool_required, ClaudeCredentialDevicePoolStore,
    ClaudeCredentialIdentityError,
};
pub use claude_device_profile::{
    apply_claude_default_device_profile_headers, apply_claude_device_profile_headers,
    apply_claude_legacy_device_headers, claude_device_profile_stabilization_enabled,
    default_claude_device_profile, default_claude_version, map_stainless_arch, map_stainless_os,
    ClaudeDeviceProfile, ClaudeDeviceProfileCache, ClaudeHeaderDefaults,
};
pub use claude_diagnostics::{
    begin_claude_diagnostics, commit_claude_diagnostics, pin_claude_session_date,
};
pub use claude_input_tokens::{
    count_claude_input_tokens, ClaudeInputTokenError, ClaudeInputTokenFailureSink,
    ClaudeInputTokenState,
};
pub use claude_mcp_alias::{claude_mcp_tool_alias, is_claude_mcp_tool_name};
pub use claude_upstream::is_anthropic_upstream_url;
pub use cloak_obfuscate::{
    build_sensitive_word_matcher, obfuscate_sensitive_words, SensitiveWordMatcher,
};
pub use cloak_utils::{
    generate_fake_user_id, generate_fake_user_id_with_session_id, is_claude_code_client,
    is_valid_user_id, should_cloak,
};
pub use codex_input_ids::sanitize_codex_input_item_ids;
pub use codex_multi_agent_v2::{
    optimize_codex_multi_agent_v2_request, restore_codex_multi_agent_v2_response,
    rewrite_codex_multi_agent_v2_input, rewrite_codex_spawn_agent_description,
    translate_request_pair_with_api_key_model_compatibility_and_update_intent,
    translate_request_with_api_key_model_compatibility_and_update_intent_for_executor,
    translate_request_with_api_key_model_compatibility_for_executor,
    translate_request_with_codex_multi_agent_v2,
    translate_request_with_codex_multi_agent_v2_for_executor, CodexMultiAgentV2Processor,
    RegistryCodexMultiAgentV2Processor,
};
pub use codex_tool_schema::{
    is_codex_target_executor, is_codex_user_agent, normalize_codex_tool_integer_types,
    normalize_codex_tool_integer_types_for_executor, normalize_codex_tool_schemas,
};
pub use derived_session::{
    derived_antigravity_session_id, derived_session_id, derived_session_uuid, provider_session_uuid,
};
pub use home_refresh::{
    status_from_home_error_code, HomeRefreshAuthority, HomeRefreshClient, HomeRefreshClientError,
    HomeRefreshClientErrorKind, HomeRefreshDisposition, HomeStatusError,
    MAX_HOME_REFRESH_PAYLOAD_BYTES,
};
pub use json_retry_helpers::{
    delete_json_field, parse_retry_delay, RetryDelayError, MAX_RETRY_ERROR_BODY_BYTES,
};
pub use kimi_responses::normalize_kimi_responses_input;
pub use logging_helpers::{
    append_api_response_chunk, append_api_websocket_response, credits_used, mark_credits_used,
    record_api_request, record_api_response_error, record_api_response_metadata,
    record_api_websocket_error, record_api_websocket_handshake, record_api_websocket_request,
    record_api_websocket_upgrade_rejection, request_id, summarize_error_body,
    websocket_upgrade_request_url, ApiLogClock, ApiLogContext, DeferredApiRequest, LogHeaders,
    RequestLogPolicy, SystemApiLogClock, UpstreamRequestLog, MAX_DEFERRED_API_REQUEST_BODY_BYTES,
};
pub use model_capabilities::{
    apply_request_thinking, RequestThinkingEngine, RequestThinkingInput, RequestThinkingRoute,
};
pub use openai_compat_tool_results::{
    normalize_openai_tool_results_text_only, should_normalize_openai_tool_results_for_model,
    OPENAI_TOOL_RESULT_IMAGE_OMITTED_TEXT,
};
pub use payload_helpers::{
    apply_payload_config_with_request, apply_payload_config_with_root, match_model_pattern,
    payload_request_path, payload_requested_model, PayloadApplyConfig, PayloadFilterRule,
    PayloadHeaders, PayloadModelRule, PayloadRule, PayloadRules,
};
pub use payload_mutations::{
    join_raw_json_array, join_raw_json_strings, remove_tool_type_from_tools_array,
    set_bool_if_different, set_payload_value_if_different, set_raw_if_different,
    set_string_if_different,
};
pub use proxy_helpers::{
    new_proxy_aware_http_client, ProxyAwareHttpClientPlan, ProxyClientFailureSink,
    ProxyTransportSource, MAX_PROXY_URL_BYTES,
};
pub use session_id_cache::{
    claude_session_id_kv_key, session_id_cache_key, SessionIdCache, SessionIdCacheError,
    SessionIdClock, DEFAULT_SESSION_ID_CACHE_CAPACITY, SESSION_ID_TTL,
};
pub use thinking::{apply_thinking_with_source_payload, translated_request_summary_config};
pub use thinking_providers::{is_upstream_thinking_provider, THINKING_PROVIDER_MODULES};
pub use token_helpers::{
    build_openai_usage_json, count_openai_chat_tokens, count_openai_chat_tokens_for_model,
    tokenizer_for_model, OpenAiTokenCountError, OpenAiTokenizer, MAX_OPENAI_TOKEN_PAYLOAD_BYTES,
    MAX_OPENAI_TOKEN_TEXT_BYTES,
};
pub use usage_helpers::{
    has_nonzero_token_usage, json_payload, normalize_usage_detail_total,
    parse_antigravity_stream_usage, parse_antigravity_usage, parse_claude_stream_usage,
    parse_claude_usage, parse_codex_image_tool_usage, parse_codex_usage, parse_gemini_stream_usage,
    parse_gemini_usage, parse_interactions_stream_usage, parse_interactions_usage,
    parse_openai_stream_usage, parse_openai_usage, strip_usage_metadata_from_json,
    SseUsageMetadataFilter, StreamUsageBuffer, DEFAULT_STOP_TRACE_CAPACITY, DEFAULT_STOP_TRACE_TTL,
    MAX_USAGE_STREAM_CHUNK_BYTES,
};
pub use user_id_cache::{ClaudeIdentityKvStore, ClaudeIdentityStoreError, UserIdCache};
pub use utls_client::{
    claude_code_request_header_order, claude_code_transport_scope_key, new_utls_http_client,
    UtlsClientError, UtlsHttpClient, UtlsTransportFactory, UtlsTransportProfile, UtlsTransports,
    CLAUDE_CODE_CIPHER_SUITES, CLAUDE_CODE_COUNT_TOKENS_HEADER_ORDER,
    CLAUDE_CODE_MESSAGES_HEADER_ORDER, CLAUDE_CODE_OMIT_EMPTY_PSK,
    CLAUDE_CODE_ROUND_TRIPPER_CACHE_CAPACITY, CLAUDE_CODE_SESSION_CACHE_CAPACITY,
    CLAUDE_CODE_SKIP_RESUMPTION_WITHOUT_PSK_EXTENSION, CLAUDE_CODE_TLS_EXTENSIONS,
    UTLS_PROTECTED_HOSTS,
};
pub use vertex_payload_helpers::strip_vertex_openai_responses_tool_call_ids;
