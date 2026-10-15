// ref: internal/runtime/executor/codex_websockets_request.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;

use serde_json::Value;

use crate::internal::auth::codex::SecretString;

use super::codex_executor::{CODEX_ORIGINATOR, CODEX_USER_AGENT};

pub type CodexWebsocketHeaders = BTreeMap<String, String>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexWebsocketHeaderDefaults {
    pub user_agent: Option<String>,
    pub originator: Option<String>,
    pub beta: Option<String>,
    pub disable_cloaking: bool,
}

pub fn apply_codex_prompt_cache_headers(
    body: &[u8],
    source_headers: &CodexWebsocketHeaders,
) -> (Vec<u8>, CodexWebsocketHeaders) {
    let mut value = serde_json::from_slice::<Value>(body).unwrap_or(Value::Null);
    let mut headers = source_headers.clone();
    if let Some(key) = value
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        headers.insert("session_id".to_owned(), key.to_owned());
    } else if let Some(session) = header_value_case_insensitive(source_headers, "session_id") {
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "prompt_cache_key".to_owned(),
                Value::String(session.to_owned()),
            );
        }
    }
    let encoded = serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec());
    (encoded, headers)
}

pub fn apply_codex_websocket_headers(
    mut headers: CodexWebsocketHeaders,
    access_token: &SecretString,
    account_id: &str,
    session_id: &str,
    defaults: &CodexWebsocketHeaderDefaults,
) -> CodexWebsocketHeaders {
    delete_header_case_insensitive(&mut headers, "authorization");
    set_header_case_preserved(
        &mut headers,
        "Authorization",
        &format!("Bearer {}", access_token.expose_secret()),
    );
    ensure_header(&mut headers, "ChatGPT-Account-ID", account_id);
    if defaults.disable_cloaking {
        if let Some(user_agent) = defaults.user_agent.as_deref() {
            ensure_header(&mut headers, "User-Agent", user_agent);
        }
        if let Some(originator) = defaults.originator.as_deref() {
            ensure_header(&mut headers, "originator", originator);
        }
    } else {
        set_header_case_preserved(
            &mut headers,
            "User-Agent",
            defaults.user_agent.as_deref().unwrap_or(CODEX_USER_AGENT),
        );
        set_header_case_preserved(
            &mut headers,
            "originator",
            defaults.originator.as_deref().unwrap_or(CODEX_ORIGINATOR),
        );
    }
    if let Some(beta) = defaults
        .beta
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        ensure_header(&mut headers, "OpenAI-Beta", beta);
    }
    if !session_id.trim().is_empty() {
        ensure_codex_websocket_session_header(&mut headers, session_id);
    }
    headers
}

pub fn ensure_codex_websocket_session_header(headers: &mut CodexWebsocketHeaders, value: &str) {
    if header_value_case_insensitive(headers, "session_id").is_none() {
        headers.insert("session_id".to_owned(), value.to_owned());
    }
}

pub fn codex_session_header_value(headers: &CodexWebsocketHeaders) -> Option<&str> {
    header_value_case_insensitive(headers, "session_id")
}

pub fn header_value_case_insensitive<'a>(
    headers: &'a CodexWebsocketHeaders,
    key: &str,
) -> Option<&'a str> {
    headers
        .iter()
        .find(|(candidate, _)| canonical_header(candidate) == canonical_header(key))
        .map(|(_, value)| value.as_str())
}

fn ensure_header(headers: &mut CodexWebsocketHeaders, key: &str, fallback: &str) {
    if header_value_case_insensitive(headers, key).is_none() && !fallback.trim().is_empty() {
        headers.insert(key.to_owned(), fallback.to_owned());
    }
}

fn set_header_case_preserved(headers: &mut CodexWebsocketHeaders, key: &str, value: &str) {
    delete_header_case_insensitive(headers, key);
    headers.insert(key.to_owned(), value.to_owned());
}

fn delete_header_case_insensitive(headers: &mut CodexWebsocketHeaders, key: &str) {
    let key = canonical_header(key);
    headers.retain(|candidate, _| canonical_header(candidate) != key);
}

fn canonical_header(value: &str) -> String {
    value.replace('-', "_").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

}
