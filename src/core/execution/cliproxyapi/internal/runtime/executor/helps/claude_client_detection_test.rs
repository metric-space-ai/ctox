// ref: internal/runtime/executor/helps/claude_client_detection_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::claude_client_detection::detect_claude_code_request;
use super::claude_device_profile::ClaudeHeaderDefaults;
use crate::sdk::api::handlers::header_filter::HeaderMap;

const VALID_USER_ID: &str = r#"{"device_id":"0000000000000000000000000000000000000000000000000000000000000000","account_uuid":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","session_id":"11111111-2222-4333-8444-555555555555"}"#;

fn payload(user_id: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"metadata": {"user_id": user_id}})).unwrap()
}

fn confirmed_headers(user_agent: &str) -> HeaderMap {
    HeaderMap::from([
        ("User-Agent".to_owned(), vec![user_agent.to_owned()]),
        ("X-App".to_owned(), vec!["cli".to_owned()]),
        (
            "Anthropic-Beta".to_owned(),
            vec!["claude-code-20250219,interleaved-thinking-2025-05-14".to_owned()],
        ),
    ])
}

#[test]
fn requires_all_message_signals_and_allows_count_tokens_without_metadata() {
    let defaults = ClaudeHeaderDefaults::default();
    let headers = confirmed_headers("claude-cli/2.1.280 (external, cli)");
    let detection =
        detect_claude_code_request(Some(&headers), &payload(VALID_USER_ID), false, &defaults);
    assert!(detection.confirmed && detection.strong_signals && detection.native_client);
    assert!(
        detection.x_app_cli
            && detection.user_agent
            && detection.betas_present
            && detection.metadata_user_id
    );
    let count = detect_claude_code_request(Some(&headers), br#"{"messages":[]}"#, true, &defaults);
    assert!(count.confirmed);
    assert!(!count.metadata_user_id);
}

#[test]
fn honors_configured_measured_baseline() {
    let headers = confirmed_headers("claude-cli/2.2.0 (external, cli)");
    assert!(
        !detect_claude_code_request(
            Some(&headers),
            &payload(VALID_USER_ID),
            false,
            &ClaudeHeaderDefaults::default()
        )
        .confirmed
    );
    let defaults = ClaudeHeaderDefaults {
        user_agent: "claude-cli/2.2.0 (external, cli)".to_owned(),
        package_version: "0.95.0".to_owned(),
        runtime_version: "v26.4.0".to_owned(),
        ..ClaudeHeaderDefaults::default()
    };
    assert!(
        detect_claude_code_request(Some(&headers), &payload(VALID_USER_ID), false, &defaults)
            .confirmed
    );
}

#[test]
fn classifies_native_and_non_native_entrypoints() {
    let defaults = ClaudeHeaderDefaults::default();
    for (entrypoint, subclient, native) in [
        ("cli", "claude-code-cli", true),
        ("sdk-cli", "claude-code-cli-sdk", true),
        ("claude-vscode", "claude-code-vscode", true),
        ("sdk-ts", "claude-code-sdk-ts", false),
        ("claude-desktop", "claude-desktop", false),
        ("copied-client", "", false),
    ] {
        let user_agent = format!("claude-cli/2.1.280 (external, {entrypoint})");
        let detection = detect_claude_code_request(
            Some(&confirmed_headers(&user_agent)),
            &payload(VALID_USER_ID),
            false,
            &defaults,
        );
        assert!(detection.strong_signals);
        assert_eq!(detection.entrypoint, entrypoint);
        assert_eq!(detection.subclient, subclient);
        assert_eq!(detection.native_client, native);
        assert_eq!(detection.confirmed, native);
    }
}

#[test]
fn rejects_malformed_or_missing_signals() {
    let defaults = ClaudeHeaderDefaults::default();
    let mut headers = confirmed_headers("claude-cli/2.1.280 (external, cli)");
    headers.remove("X-App");
    assert!(
        !detect_claude_code_request(Some(&headers), &payload(VALID_USER_ID), false, &defaults)
            .confirmed
    );
    let headers = confirmed_headers("claude-cli/2.1.280 (external, cli)");
    assert!(
        !detect_claude_code_request(Some(&headers), &payload("user_legacy"), false, &defaults)
            .confirmed
    );
    let headers = confirmed_headers("claude-cli/not-a-version (external, cli)");
    assert!(
        !detect_claude_code_request(Some(&headers), &payload(VALID_USER_ID), false, &defaults)
            .confirmed
    );
}
