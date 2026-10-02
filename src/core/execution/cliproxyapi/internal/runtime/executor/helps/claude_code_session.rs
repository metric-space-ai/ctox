// ref: internal/runtime/executor/helps/claude_code_session.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::Value;
use uuid::Uuid;

use super::cache_helpers::CodexCache;

use crate::sdk::api::handlers::header_filter::HeaderMap;

pub const CLAUDE_CODE_SESSION_HEADER: &str = "X-Claude-Code-Session-Id";
pub const CLAUDE_CODE_AGENT_HEADER: &str = "X-Claude-Code-Agent-Id";
pub const CLAUDE_CODE_MAIN_AGENT_ID: &str = "main";

/// Resolves a Claude Code session ID. The caller injects the complete request
/// header view explicitly; Rust does not consult framework- or process-global
/// request context.
pub fn extract_claude_code_session_id(payload: &[u8], headers: Option<&HeaderMap>) -> String {
    let header = header_value_case_insensitive(headers, CLAUDE_CODE_SESSION_HEADER);
    if !header.is_empty() {
        return header;
    }
    extract_claude_code_session_id_from_payload(payload)
}

pub fn extract_claude_code_agent_id(headers: Option<&HeaderMap>) -> String {
    let agent = header_value_case_insensitive(headers, CLAUDE_CODE_AGENT_HEADER);
    if agent.is_empty() {
        CLAUDE_CODE_MAIN_AGENT_ID.to_owned()
    } else {
        agent
    }
}

pub fn claude_code_execution_scope(payload: &[u8], headers: Option<&HeaderMap>) -> Option<String> {
    let session_id = extract_claude_code_session_id(payload, headers);
    if session_id.is_empty() {
        return None;
    }
    Some(format!(
        "claude:{session_id}:agent:{}",
        extract_claude_code_agent_id(headers)
    ))
}

pub fn claude_code_prompt_cache(
    model_name: &str,
    payload: &[u8],
    headers: Option<&HeaderMap>,
) -> Option<CodexCache> {
    let model_name = model_name.trim();
    let execution_scope = claude_code_execution_scope(payload, headers)?;
    if model_name.is_empty() {
        return None;
    }
    let identity = [
        "cli-proxy-api:codex:claude-code",
        model_name,
        &execution_scope,
    ]
    .join("\0");
    Some(CodexCache {
        id: Uuid::new_v5(&Uuid::NAMESPACE_OID, identity.as_bytes()).to_string(),
        expire: None,
    })
}

/// Returns the first non-empty header value matching `name`, case-insensitively.
#[must_use]
pub fn header_value_case_insensitive(headers: Option<&HeaderMap>, name: &str) -> String {
    let Some(headers) = headers else {
        return String::new();
    };
    headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case(name))
        .flat_map(|(_, values)| values)
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .unwrap_or_default()
        .to_owned()
}

/// Returns every non-empty header value matching `name`, case-insensitively.
#[must_use]
pub fn header_values_case_insensitive(headers: Option<&HeaderMap>, name: &str) -> Vec<String> {
    let Some(headers) = headers else {
        return Vec::new();
    };
    headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case(name))
        .flat_map(|(_, values)| values)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn extract_claude_code_session_id_from_payload(payload: &[u8]) -> String {
    if payload.is_empty() {
        return String::new();
    }
    let Ok(root) = serde_json::from_slice::<Value>(payload) else {
        return String::new();
    };
    let Some(user_id) = root
        .get("metadata")
        .and_then(|metadata| metadata.get("user_id"))
        .and_then(Value::as_str)
    else {
        return String::new();
    };

    if let Some((_, suffix)) = user_id.rsplit_once("_session_") {
        if !suffix.is_empty()
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase() || byte == b'-')
        {
            return suffix.to_owned();
        }
    }
    if user_id.starts_with('{') {
        return serde_json::from_str::<Value>(user_id)
            .ok()
            .and_then(|value| {
                value
                    .get("session_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .map(str::to_owned)
            })
            .unwrap_or_default();
    }
    String::new()
}
