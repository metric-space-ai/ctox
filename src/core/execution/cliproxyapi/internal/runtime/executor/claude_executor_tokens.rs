// ref: internal/runtime/executor/claude_executor_tokens.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;

use serde_json::{value::RawValue, Value};

use super::claude_executor_cloaking::{
    validate_claude_caller_system_blocks, ClaudeCallerSystemBlockError,
};
use super::helps::{
    build_sensitive_word_matcher, count_claude_input_tokens, obfuscate_sensitive_words,
    ClaudeInputTokenError,
};
use super::{
    claude_count_tokens_betas, enforce_claude_cache_control_limit, extract_and_remove_claude_betas,
    normalize_claude_cache_control_ttl, prepare_claude_first_party_count_tokens_body,
    relocate_claude_system_prompt_for_count_tokens, remap_claude_oauth_tool_names_with_secret,
    ClaudeCloakPolicy, ClaudeCredentialMode, ClaudeMessagesRequest,
};
use crate::internal::signature::sanitize_claude_messages_for_claude_upstream;
use crate::internal::translator::common::claude_input_tokens_json;
use crate::sdk::cliproxy::executor::Headers;

const CLAUDE_TOKEN_COUNTING_BETA: &str = "token-counting-2024-11-01";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeFirstPartyTokenCountBody {
    pub body: Vec<u8>,
    pub requested_betas: Vec<String>,
    pub cloaked: bool,
}

/// Renders the Claude Code count-token header profile from the same
/// account-scoped fingerprint as Messages execution. The host HTTP client may
/// change transport implementation, but not session/device identity.
pub fn claude_first_party_token_count_headers(request: &ClaudeMessagesRequest) -> Headers {
    let fingerprint = request.fingerprint();
    let device = fingerprint.device();
    let authorization = request.authorization();
    let mut betas = claude_count_tokens_betas(request.mode() == ClaudeCredentialMode::OAuth)
        .split(',')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for beta in request.betas() {
        let beta = beta.trim();
        if !beta.is_empty() && !betas.iter().any(|existing| existing == beta) {
            betas.push(beta.to_owned());
        }
    }
    let mut headers = Headers::from([
        (
            "content-type".to_owned(),
            vec!["application/json".to_owned()],
        ),
        (
            "anthropic-version".to_owned(),
            vec!["2023-06-01".to_owned()],
        ),
        ("anthropic-beta".to_owned(), vec![betas.join(",")]),
        ("x-app".to_owned(), vec!["cli".to_owned()]),
        ("x-stainless-retry-count".to_owned(), vec!["0".to_owned()]),
        ("x-stainless-runtime".to_owned(), vec!["node".to_owned()]),
        ("x-stainless-lang".to_owned(), vec!["js".to_owned()]),
        ("x-stainless-timeout".to_owned(), vec!["600".to_owned()]),
        (
            "x-claude-code-session-id".to_owned(),
            vec![fingerprint.session_id().to_owned()],
        ),
        (
            "user-agent".to_owned(),
            vec![device.user_agent().to_owned()],
        ),
        (
            "x-stainless-package-version".to_owned(),
            vec![device.package_version().to_owned()],
        ),
        (
            "x-stainless-runtime-version".to_owned(),
            vec![device.runtime_version().to_owned()],
        ),
        ("x-stainless-os".to_owned(), vec![device.os().to_owned()]),
        (
            "x-stainless-arch".to_owned(),
            vec![device.arch().to_owned()],
        ),
        ("accept".to_owned(), vec!["application/json".to_owned()]),
        (
            "accept-encoding".to_owned(),
            vec!["gzip, deflate, br, zstd".to_owned()],
        ),
        (
            authorization.set_header().as_str().to_ascii_lowercase(),
            vec![authorization.expose_header_value().to_owned()],
        ),
    ]);
    if let Some(request_id) = request.client_request_id_for_target() {
        headers.insert(
            "x-client-request-id".to_owned(),
            vec![request_id.to_owned()],
        );
    }
    headers
}

/// Candidate `count_tokens` wire preparation for Anthropic's first-party
/// endpoint. This deliberately differs from full Messages cloaking: synthetic
/// generation instructions are not installed, but caller system text remains
/// counted after relocation and configured sensitive words are obfuscated.
pub fn prepare_claude_first_party_token_count_body(
    body: &[u8],
    model: &str,
    policy: &ClaudeCloakPolicy,
    oauth_alias_secret: &str,
) -> Result<ClaudeFirstPartyTokenCountBody, ClaudeCallerSystemBlockError> {
    let mut body = set_claude_count_tokens_model(body, model);
    let cloaked = policy.should_cloak_request();
    if cloaked {
        if !policy.strict_mode {
            let root = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
            validate_claude_caller_system_blocks(root.get("system"))?;
        }
        body = relocate_claude_system_prompt_for_count_tokens(&body, policy.strict_mode);
        let matcher = build_sensitive_word_matcher(&policy.sensitive_words);
        body = obfuscate_sensitive_words(&body, matcher.as_ref());
    }

    body = enforce_claude_cache_control_limit(&body, 4);
    body = normalize_claude_cache_control_ttl(&body);
    let (mut requested_betas, mut body) = extract_and_remove_claude_betas(&body);
    if !requested_betas
        .iter()
        .any(|beta| beta == CLAUDE_TOKEN_COUNTING_BETA)
    {
        requested_betas.push(CLAUDE_TOKEN_COUNTING_BETA.to_owned());
    }
    if cloaked && !oauth_alias_secret.is_empty() {
        body = remap_claude_oauth_tool_names_with_secret(&body, oauth_alias_secret).0;
    }
    body = sanitize_claude_messages_for_claude_upstream(&body, false).0;
    body = prepare_claude_first_party_count_tokens_body(&body);
    Ok(ClaudeFirstPartyTokenCountBody {
        body,
        requested_betas,
        cloaked,
    })
}

fn set_claude_count_tokens_model(body: &[u8], model: &str) -> Vec<u8> {
    if model.trim().is_empty() {
        return body.to_vec();
    }
    let Ok(mut root) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(object) = root.as_object_mut() else {
        return body.to_vec();
    };
    if object.get("model").and_then(Value::as_str) == Some(model) {
        return body.to_vec();
    }
    object.insert("model".to_owned(), Value::String(model.to_owned()));
    serde_json::to_vec(&root).unwrap_or_else(|_| body.to_vec())
}

/// Validates and locally counts an already Claude-shaped request.
///
/// Translation, thinking policy and sanitization belong to the owning request
/// pipeline and must happen before this native boundary.
pub fn claude_token_count_response(body: &[u8]) -> Result<Vec<u8>, ClaudeTokenCountError> {
    let body = prepare_claude_first_party_count_tokens_body(body);
    validate_claude_token_count_request(&body)?;
    let count = count_claude_input_tokens(&body).map_err(ClaudeTokenCountError::Counting)?;
    Ok(claude_input_tokens_json(count))
}

pub fn validate_claude_token_count_request(body: &[u8]) -> Result<(), ClaudeTokenCountError> {
    let _: Box<RawValue> =
        serde_json::from_slice(body).map_err(|_| ClaudeTokenCountError::InvalidJson)?;
    let document = std::str::from_utf8(body).map_err(|_| ClaudeTokenCountError::InvalidJson)?;
    let root = gjson::parse(document);
    if root.kind() != gjson::Kind::Object {
        return Err(ClaudeTokenCountError::RootNotObject);
    }
    let messages = root.get("messages");
    if messages.kind() != gjson::Kind::Array || messages.array().is_empty() {
        return Err(ClaudeTokenCountError::MessagesRequired);
    }
    let mut error = None;
    messages.each(|_, message| {
        if message.kind() != gjson::Kind::Object {
            error = Some(ClaudeTokenCountError::MessageNotObject);
            return false;
        }
        if !matches!(message.get("role").str(), "user" | "assistant") {
            error = Some(ClaudeTokenCountError::InvalidRole);
            return false;
        }
        let content = message.get("content");
        if content.kind() == gjson::Kind::String {
            return true;
        }
        if content.kind() != gjson::Kind::Array {
            error = Some(ClaudeTokenCountError::InvalidContent);
            return false;
        }
        content.each(|_, block| {
            let block_type = block.get("type");
            if block.kind() != gjson::Kind::Object
                || block_type.kind() != gjson::Kind::String
                || block_type.str().is_empty()
            {
                error = Some(ClaudeTokenCountError::InvalidContentBlock);
                return false;
            }
            true
        });
        error.is_none()
    });
    error.map_or(Ok(()), Err)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaudeTokenCountError {
    InvalidJson,
    RootNotObject,
    MessagesRequired,
    MessageNotObject,
    InvalidRole,
    InvalidContent,
    InvalidContentBlock,
    Counting(ClaudeInputTokenError),
}

impl fmt::Display for ClaudeTokenCountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidJson => "invalid Claude token count request JSON",
            Self::RootNotObject => "Claude token count request must be a JSON object",
            Self::MessagesRequired => {
                "Claude token count request messages must be a non-empty array"
            }
            Self::MessageNotObject => "Claude token count request messages must contain objects",
            Self::InvalidRole => {
                "Claude token count request message role must be user or assistant"
            }
            Self::InvalidContent => {
                "Claude token count request message content must be a string or array"
            }
            Self::InvalidContentBlock => {
                "Claude token count request content blocks must be typed objects"
            }
            Self::Counting(_) => "Claude executor token counting failed",
        })
    }
}

impl std::error::Error for ClaudeTokenCountError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::auth::claude::SecretString;
    use crate::internal::runtime::executor::{ClaudeDeviceProfile, ClaudeUpstreamTarget};

    #[test]
    fn native_local_count_matches_pinned_o200k_fixture() {
        let payload = br#"{
            "system":"client system instructions",
            "messages":[{"role":"user","content":[{"type":"text","text":"hello"}]}]
        }"#;

        assert_eq!(
            claude_token_count_response(payload).unwrap(),
            br#"{"input_tokens":7}"#
        );
    }

    #[test]
    fn pinned_invalid_request_table_is_request_validation_not_zero_count() {
        let cases: &[(&[u8], ClaudeTokenCountError)] = &[
            (b"not-json", ClaudeTokenCountError::InvalidJson),
            (b"[]", ClaudeTokenCountError::RootNotObject),
            (b"{}", ClaudeTokenCountError::MessagesRequired),
            (
                br#"{"messages":[]}"#,
                ClaudeTokenCountError::MessagesRequired,
            ),
            (
                br#"{"messages":"invalid"}"#,
                ClaudeTokenCountError::MessagesRequired,
            ),
            (
                br#"{"messages":[42]}"#,
                ClaudeTokenCountError::MessageNotObject,
            ),
            (
                br#"{"messages":[{"role":"system","content":"hello"}]}"#,
                ClaudeTokenCountError::InvalidRole,
            ),
            (
                br#"{"messages":[{"role":"user","content":42}]}"#,
                ClaudeTokenCountError::InvalidContent,
            ),
            (
                br#"{"messages":[{"role":"user","content":[42]}]}"#,
                ClaudeTokenCountError::InvalidContentBlock,
            ),
            (
                br#"{"messages":[{"role":"user","content":[{"text":"hello"}]}]}"#,
                ClaudeTokenCountError::InvalidContentBlock,
            ),
        ];

        for (body, expected) in cases {
            assert_eq!(
                validate_claude_token_count_request(body),
                Err(expected.clone())
            );
        }
    }

    #[test]
    fn first_party_generation_metadata_is_not_counted() {
        let with_metadata = br#"{"messages":[{"role":"user","content":"hello"}],"metadata":{"user_id":"secret"},"context_management":{"edits":[]},"diagnostics":{"previous_message_id":"msg_1"}}"#;
        let baseline = br#"{"messages":[{"role":"user","content":"hello"}]}"#;
        assert_eq!(
            claude_token_count_response(with_metadata).unwrap(),
            claude_token_count_response(baseline).unwrap()
        );
    }

    #[test]
    fn candidate_count_tokens_relocates_system_without_generation_identity() {
        let mut policy = ClaudeCloakPolicy::oauth_default();
        policy.sensitive_words = vec!["classified".to_owned()];
        let payload = br#"{
            "model":"caller-model",
            "system":[
                {"type":"text","text":"first classified guidance"},
                {"type":"text","text":"second guidance"}
            ],
            "messages":[{"role":"user","content":"hello classified"}],
            "metadata":{"user_id":"credential-identity"},
            "context_management":{"edits":[]},
            "diagnostics":{"previous_message_id":"msg_1"},
            "betas":["custom-beta"]
        }"#;

        let prepared = prepare_claude_first_party_token_count_body(
            payload,
            "claude-opus-5",
            &policy,
            "account-secret",
        )
        .unwrap();
        let root: Value = serde_json::from_slice(&prepared.body).unwrap();

        assert!(prepared.cloaked);
        assert_eq!(root["model"], "claude-opus-5");
        assert!(root.get("system").is_none());
        assert!(root.get("metadata").is_none());
        assert!(root.get("context_management").is_none());
        assert!(root.get("diagnostics").is_none());
        assert!(root.get("betas").is_none());
        assert_eq!(root["messages"].as_array().unwrap().len(), 3);
        let encoded = String::from_utf8(prepared.body).unwrap();
        assert!(!encoded.contains("classified"));
        assert!(!encoded.contains("You are Claude Code"));
        assert_eq!(
            prepared.requested_betas,
            vec!["custom-beta", CLAUDE_TOKEN_COUNTING_BETA]
        );
    }

    #[test]
    fn candidate_count_tokens_native_bypass_and_strict_drop_match_wire_policy() {
        let payload = br#"{"model":"sonnet","system":"caller system","messages":[{"role":"user","content":"hello"}],"metadata":{"user_id":"identity"}}"#;
        let mut native = ClaudeCloakPolicy::oauth_default();
        native.verified_claude_code = true;
        let native = prepare_claude_first_party_token_count_body(
            payload,
            "sonnet",
            &native,
            "account-secret",
        )
        .unwrap();
        let native_root: Value = serde_json::from_slice(&native.body).unwrap();
        assert!(!native.cloaked);
        assert_eq!(native_root["system"], "caller system");
        assert!(native_root.get("metadata").is_none());

        let mut strict = ClaudeCloakPolicy::oauth_default();
        strict.strict_mode = true;
        let strict = prepare_claude_first_party_token_count_body(
            payload,
            "sonnet",
            &strict,
            "account-secret",
        )
        .unwrap();
        let strict_root: Value = serde_json::from_slice(&strict.body).unwrap();
        assert!(strict.cloaked);
        assert!(strict_root.get("system").is_none());
        assert_eq!(strict_root["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn candidate_count_tokens_legacy_system_reminders_stay_separate() {
        let payload = br#"{"model":"claude-opus-4-6","system":[{"type":"text","text":"first"},{"type":"text","text":"second"}],"messages":[{"role":"user","content":"hello"}]}"#;
        let prepared = prepare_claude_first_party_token_count_body(
            payload,
            "claude-opus-4-6",
            &ClaudeCloakPolicy::oauth_default(),
            "secret",
        )
        .unwrap();
        let root: Value = serde_json::from_slice(&prepared.body).unwrap();
        let content = root["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 3);
        assert!(content[0]["text"].as_str().unwrap().contains("first"));
        assert!(content[1]["text"].as_str().unwrap().contains("second"));
        assert_eq!(content[2]["text"], "hello");
    }

    #[test]
    fn candidate_count_tokens_headers_bind_session_profile_and_authorization() {
        let credential = SecretString::new("oauth-secret").unwrap();
        let request = ClaudeMessagesRequest::new_with_session(
            ClaudeUpstreamTarget::new("https", "api.anthropic.com").unwrap(),
            ClaudeCredentialMode::OAuth,
            &credential,
            br#"{"messages":[{"role":"user","content":"hi"}]}"#.to_vec(),
            false,
            "11111111-2222-4333-8444-555555555555",
        )
        .unwrap()
        .with_upstream_metadata(vec!["custom-beta".to_owned()], Default::default())
        .with_device_profile(
            ClaudeDeviceProfile::new(
                "claude-cli/2.2.0 (external, cli)",
                "0.95.0",
                "v26.4.0",
                "MacOS",
                "arm64",
            )
            .unwrap(),
        )
        .unwrap();

        let headers = claude_first_party_token_count_headers(&request);
        assert_eq!(
            headers["x-claude-code-session-id"],
            vec!["11111111-2222-4333-8444-555555555555"]
        );
        assert_eq!(
            headers["user-agent"],
            vec!["claude-cli/2.2.0 (external, cli)"]
        );
        assert_eq!(headers["x-stainless-package-version"], vec!["0.95.0"]);
        assert_eq!(headers["authorization"], vec!["Bearer oauth-secret"]);
        assert!(headers["anthropic-beta"][0].contains(CLAUDE_TOKEN_COUNTING_BETA));
        assert!(headers["anthropic-beta"][0].contains("custom-beta"));
        assert!(headers.contains_key("x-client-request-id"));
    }
}
