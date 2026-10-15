// ref: internal/runtime/executor/claude_executor_wire_casing_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::claude_wire_header_name;

#[cfg(feature = "anthropic-fingerprint-transport")]
#[test]
fn native_count_tokens_transport_merges_candidate_betas() {
    use std::collections::HashMap;

    use super::claude_executor::{
        ClaudeCredentialMode, ClaudeMessagesRequest, ClaudeUpstreamTarget,
    };
    use super::claude_executor_request::merged_claude_count_tokens_beta_header;
    use crate::internal::auth::claude::SecretString;

    let request = ClaudeMessagesRequest::new_with_session(
        ClaudeUpstreamTarget::new("https", "api.anthropic.com").unwrap(),
        ClaudeCredentialMode::OAuth,
        &SecretString::new("access-token").unwrap(),
        br#"{"messages":[{"role":"user","content":"hello"}]}"#.to_vec(),
        false,
        "11111111-2222-4333-8444-555555555555",
    )
    .unwrap()
    .with_upstream_metadata(vec!["custom-beta".to_owned()], HashMap::new());

    let betas = merged_claude_count_tokens_beta_header(&request);
    assert!(betas.contains("claude-code-20250219"));
    assert!(betas.contains("token-counting-2024-11-01"));
    assert!(betas.contains("custom-beta"));
}
