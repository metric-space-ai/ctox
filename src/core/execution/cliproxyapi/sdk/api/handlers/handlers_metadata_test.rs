// ref: sdk/api/handlers/handlers_metadata_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use crate::internal::config::StreamingConfig;
use crate::sdk::api::handlers::HandlerRequestContext;
use crate::sdk::cliproxy::session::caller_scope;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn error_body_preserves_json_and_maps_statuses() {
    assert_eq!(
        build_error_response_body(400, "  {\"error\":\"upstream\"} \n"),
        br#"{"error":"upstream"}"#
    );
    let response: ErrorResponse =
        serde_json::from_slice(&build_error_response_body(401, "bad key")).unwrap();
    assert_eq!(response.error.error_type, "authentication_error");
    assert_eq!(response.error.code, "invalid_api_key");
    assert_eq!(response.error.message, "bad key");
}

#[test]
fn keep_alive_and_bootstrap_values_are_clamped() {
    let mut config = SdkConfig {
        streaming: StreamingConfig {
            keepalive_seconds: 3,
            bootstrap_retries: -1,
        },
        nonstream_keepalive_interval: 7,
        ..SdkConfig::default()
    };
    assert_eq!(streaming_keep_alive_interval(Some(&config)).as_secs(), 3);
    assert_eq!(
        non_streaming_keep_alive_interval(Some(&config)).as_secs(),
        7
    );
    assert_eq!(streaming_bootstrap_retries(Some(&config)), 0);
    config.streaming.bootstrap_retries = 2;
    assert_eq!(streaming_bootstrap_retries(Some(&config)), 2);
    assert_eq!(streaming_keep_alive_interval(None), Duration::ZERO);
}

#[test]
fn reasoning_service_tier_and_generate_metadata_match_upstream_defaults() {
    let mut metadata = ExecutionMetadata::default();
    set_reasoning_effort_metadata(
        &mut metadata,
        "openai",
        "gpt-5.4(high)",
        br#"{"reasoning_effort":"low"}"#,
    );
    assert_eq!(metadata.reasoning_effort.as_deref(), Some("high"));

    set_service_tier_metadata(&mut metadata, br#"{"model":"gpt-5.4"}"#);
    assert_eq!(metadata.service_tier.as_deref(), Some("auto"));
    set_generate_metadata(&mut metadata, br#"{"generate":false}"#);
    assert_eq!(metadata.generate, Some(false));
}

#[test]
fn request_context_captures_execution_metadata_without_exposing_credentials() {
    let selected_index_calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = selected_index_calls.clone();
    let context = HandlerRequestContext {
        headers: BTreeMap::from([("idempotency-key".to_owned(), vec![" retry-1 ".to_owned()])]),
        request_path: " /v1/responses ".to_owned(),
        caller_api_key: "downstream-secret".to_owned(),
        selected_auth_index_callback: Some(Arc::new(move |_| {
            callback_calls.fetch_add(1, Ordering::Relaxed);
        })),
        ..HandlerRequestContext::default()
    }
    .with_pinned_auth_id(" account-1 ")
    .with_execution_session_id(" session-1 ")
    .with_disallow_free_auth();

    let metadata = context.execution_metadata();
    let expected_caller_scope = caller_scope("downstream-secret");
    assert_eq!(metadata.request_path.as_deref(), Some("/v1/responses"));
    assert_eq!(metadata.pinned_auth_id.as_deref(), Some("account-1"));
    assert_eq!(metadata.execution_session_id.as_deref(), Some("session-1"));
    assert_eq!(
        metadata.caller_scope.as_deref(),
        Some(expected_caller_scope.as_str())
    );
    assert_ne!(metadata.caller_scope.as_deref(), Some("downstream-secret"));
    assert!(metadata.disallow_free_auth);
    assert_eq!(
        metadata.extensions["idempotency_key"],
        serde_json::json!("retry-1")
    );
    metadata.notify_selected_auth("auth-1", "0");
    assert_eq!(selected_index_calls.load(Ordering::Relaxed), 1);
}
