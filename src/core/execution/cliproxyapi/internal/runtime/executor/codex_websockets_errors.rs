// ref: internal/runtime/executor/codex_websockets_errors.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexWebsocketError {
    pub status: u16,
    pub code: Option<String>,
    pub retryable: bool,
    pub request_scoped: bool,
    pub headers: BTreeMap<String, String>,
}

impl fmt::Display for CodexWebsocketError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Codex websocket failed with status {}",
            self.status
        )
    }
}

impl std::error::Error for CodexWebsocketError {}

impl CodexWebsocketError {
    pub fn protocol(code: &str, retryable: bool) -> Self {
        Self {
            status: 502,
            code: Some(code.to_owned()),
            retryable,
            request_scoped: true,
            headers: BTreeMap::new(),
        }
    }
}

pub fn parse_codex_websocket_error(payload: &[u8]) -> Option<CodexWebsocketError> {
    let value: Value = serde_json::from_slice(payload).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("error")
        && value.get("error").is_none()
        && value.get("status").is_none()
    {
        return None;
    }
    let error = value.get("error").unwrap_or(&value);
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let error_type = error
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let status = value
        .get("status")
        .or_else(|| error.get("status"))
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())
        .unwrap_or_else(|| {
            super::codex_executor_terminal::codex_terminal_status(Some(error_type), code.as_deref())
        });
    let headers = parse_headers(&value);
    let connection_limit = is_codex_websocket_connection_limit_error(payload);
    Some(CodexWebsocketError {
        status,
        code,
        retryable: connection_limit || matches!(status, 408 | 409 | 425 | 429 | 500..=599),
        request_scoped: !connection_limit,
        headers,
    })
}

pub fn is_codex_websocket_connection_limit_error(payload: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(payload) else {
        return false;
    };
    let code = value
        .pointer("/error/code")
        .or_else(|| value.get("code"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    matches!(
        code,
        "websocket_connection_limit"
            | "too_many_websocket_connections"
            | "connection_limit_exceeded"
    )
}

pub fn normalize_codex_websocket_completion(payload: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(payload) else {
        return payload.to_vec();
    };
    if value.get("type").and_then(Value::as_str) == Some("response.done") {
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "type".to_owned(),
                Value::String("response.completed".to_owned()),
            );
        }
    }
    serde_json::to_vec(&value).unwrap_or_else(|_| payload.to_vec())
}

pub fn encode_codex_websocket_as_sse(payload: &[u8]) -> Vec<u8> {
    let payload = normalize_codex_websocket_completion(payload);
    let mut output = Vec::with_capacity(payload.len() + 8);
    output.extend_from_slice(b"data: ");
    output.extend_from_slice(&payload);
    output.extend_from_slice(b"\n\n");
    output
}

fn parse_headers(value: &Value) -> BTreeMap<String, String> {
    value
        .get("headers")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|object| object.iter())
        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_owned())))
        .filter(|(key, _)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "retry-after" | "x-request-id" | "openai-processing-ms"
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
}
