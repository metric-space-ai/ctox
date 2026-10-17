// ref: internal/logging/gin_logger_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::gin_logger::{
    is_ai_api_path, recovery_decision, RecoveryDecision, RequestLogOutcome, RequestLogStart,
};
use super::global_logger::LogLevel;
use std::time::{Duration, SystemTime};

#[test]
fn recovery_repanics_abort_and_handles_regular_panics() {
    assert_eq!(
        recovery_decision("abort", true, "/v1", SystemTime::UNIX_EPOCH),
        RecoveryDecision::RePanicAbort
    );
    let RecoveryDecision::InternalServerError(entry) =
        recovery_decision("boom", false, "/v1", SystemTime::UNIX_EPOCH)
    else {
        panic!("expected 500 decision")
    };
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.fields.get("error").map(String::as_str), Some("boom"));
}

#[test]
fn request_summary_assigns_request_id_redacts_and_honors_skip() {
    let start = RequestLogStart::begin(
        SystemTime::UNIX_EPOCH,
        "/v1/responses",
        "api_key=secretvalue",
        "POST",
        "127.0.0.1",
    );
    assert!(start.request_id().is_some());
    let entry = start
        .finish(
            &RequestLogOutcome {
                status: 429,
                private_error: Some("quota".into()),
                credits_used: true,
                skip: false,
            },
            SystemTime::UNIX_EPOCH + Duration::from_millis(25),
        )
        .unwrap();
    assert_eq!(entry.level, LogLevel::Warn);
    assert!(entry.message.contains("api_key=secr...alue"));
    assert!(entry.message.contains("[credits]"));
    assert!(start
        .finish(
            &RequestLogOutcome {
                status: 200,
                private_error: None,
                credits_used: false,
                skip: true
            },
            SystemTime::UNIX_EPOCH
        )
        .is_none());
}
