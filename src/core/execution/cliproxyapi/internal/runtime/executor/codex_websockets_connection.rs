// ref: internal/runtime/executor/codex_websockets_connection.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use super::codex_websockets_errors::CodexWebsocketError;
use super::codex_websockets_request::CodexWebsocketHeaders;
use crate::sdk::cliproxy::executor::ResourceCloseFn;

pub const CODEX_WEBSOCKET_CLOSE_NORMAL: u16 = 1000;
pub const CODEX_WEBSOCKET_CLOSE_MESSAGE_TOO_BIG: u16 = 1009;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexWebsocketFrame {
    Text(Vec<u8>),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close { code: u16 },
}

pub type CodexWebsocketActionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), CodexWebsocketError>> + Send + 'a>>;
pub type CodexWebsocketFrameFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CodexWebsocketFrame, CodexWebsocketError>> + Send + 'a>>;
pub type CodexWebsocketConnectFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Box<dyn CodexWebsocketConnection>, CodexWebsocketError>>
            + Send
            + 'a,
    >,
>;

pub trait CodexWebsocketConnection: Send {
    fn send<'a>(&'a mut self, frame: CodexWebsocketFrame) -> CodexWebsocketActionFuture<'a>;
    fn receive<'a>(&'a mut self) -> CodexWebsocketFrameFuture<'a>;
    fn close<'a>(&'a mut self) -> CodexWebsocketActionFuture<'a>;

    /// Returns a synchronous, close-once lifecycle capability for this exact
    /// connection. A concrete transport must provide it before accepting an
    /// execution lifecycle; async `close` remains the executor-owned fallback.
    fn lifecycle_closer(&self) -> Option<ResourceCloseFn> {
        None
    }
}

pub trait CodexWebsocketTransport: Send + Sync {
    fn connect<'a>(
        &'a self,
        url: &'a str,
        headers: &'a CodexWebsocketHeaders,
    ) -> CodexWebsocketConnectFuture<'a>;
}

pub fn build_codex_responses_websocket_url(http_url: &str) -> Result<String, CodexWebsocketError> {
    let mut url = url::Url::parse(http_url)
        .map_err(|_| CodexWebsocketError::protocol("invalid_url", false))?;
    let scheme = match url.scheme() {
        "https" => "wss",
        "http" => "ws",
        "wss" => "wss",
        "ws" => "ws",
        _ => return Err(CodexWebsocketError::protocol("invalid_url_scheme", false)),
    };
    url.set_scheme(scheme)
        .map_err(|_| CodexWebsocketError::protocol("invalid_url_scheme", false))?;
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err(CodexWebsocketError::protocol(
            "invalid_url_authority",
            false,
        ));
    }
    Ok(url.into())
}

pub fn build_codex_websocket_request_body(body: &[u8]) -> Vec<u8> {
    let body = super::helps::normalize_codex_tool_schemas(body);
    let body = super::helps::sanitize_codex_input_item_ids(&body);
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(object) = value.as_object_mut() else {
        return body;
    };
    object.insert(
        "type".to_owned(),
        Value::String("response.create".to_owned()),
    );
    object.remove("stream");
    serde_json::to_vec(&value).unwrap_or(body)
}

pub fn normalize_codex_websocket_parallel_tool_calls(
    body: &[u8],
    downstream_websocket: bool,
) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    if downstream_websocket {
        return body.to_vec();
    }
    if let Some(object) = value.as_object_mut() {
        object.insert("parallel_tool_calls".to_owned(), Value::Bool(false));
    }
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

pub fn map_codex_websocket_close(code: u16) -> CodexWebsocketError {
    match code {
        CODEX_WEBSOCKET_CLOSE_NORMAL => CodexWebsocketError::protocol("connection_closed", true),
        CODEX_WEBSOCKET_CLOSE_MESSAGE_TOO_BIG => CodexWebsocketError {
            status: 413,
            code: Some("websocket_message_too_big".to_owned()),
            retryable: false,
            request_scoped: true,
            headers: Default::default(),
        },
        _ => CodexWebsocketError::protocol("connection_closed", true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_and_request_body_match_responses_websocket_contract() {
        assert_eq!(
            build_codex_responses_websocket_url("https://chatgpt.com/backend-api/codex/responses")
                .unwrap(),
            "wss://chatgpt.com/backend-api/codex/responses"
        );
        let body: Value = serde_json::from_slice(&build_codex_websocket_request_body(
            br#"{"model":"gpt","stream":true}"#,
        ))
        .unwrap();
        assert_eq!(body["type"], "response.create");
        assert!(body.get("stream").is_none());
    }
}
