// ref: internal/runtime/executor/codex_websockets_executor_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use super::codex_executor_auth::CodexSubscriptionAuth;
use super::codex_executor_reasoning::CodexReasoningReplayCache;
use super::codex_websockets_connection::{
    CodexWebsocketConnection, CodexWebsocketFrame, CodexWebsocketTransport,
};
use super::codex_websockets_errors::CodexWebsocketError;
use super::codex_websockets_executor::{CodexWebsocketExecutionRequest, CodexWebsocketsExecutor};
use super::codex_websockets_session::CodexWebsocketSessionStore;
use crate::internal::auth::codex::{
    CodexCredentialHandles, CodexRefreshCoordinator, CodexRefreshHttpResponse, CodexRefreshRequest,
    CodexRefreshTransport, CodexRefreshTransportFailure, CodexSecretHandle, CodexSecretKind,
    CodexSecretStore, CodexStoredCredentials, RefreshClock, SecretStoreError, SecretString,
};

struct Store(CodexStoredCredentials);
impl CodexSecretStore for Store {
    fn load_credentials(
        &self,
        _: &CodexCredentialHandles,
    ) -> Result<CodexStoredCredentials, SecretStoreError> {
        Ok(self.0.clone())
    }
    fn store_credentials(
        &self,
        _: &CodexCredentialHandles,
        _: &CodexStoredCredentials,
    ) -> Result<(), SecretStoreError> {
        Ok(())
    }
}

struct Clock;
impl RefreshClock for Clock {
    fn now(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH
    }
    fn sleep(
        &self,
        _: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<(), CodexRefreshTransportFailure>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}

struct Refresh;
impl CodexRefreshTransport for Refresh {
    fn execute<'a>(
        &'a self,
        _: &'a CodexRefreshRequest,
        _: Duration,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<CodexRefreshHttpResponse, CodexRefreshTransportFailure>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async { Err(CodexRefreshTransportFailure::Protocol) })
    }
}

struct Connection {
    frames: VecDeque<Result<CodexWebsocketFrame, CodexWebsocketError>>,
}
impl CodexWebsocketConnection for Connection {
    fn send<'a>(
        &'a mut self,
        _: CodexWebsocketFrame,
    ) -> Pin<Box<dyn Future<Output = Result<(), CodexWebsocketError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn receive<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<CodexWebsocketFrame, CodexWebsocketError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.frames
                .pop_front()
                .unwrap_or_else(|| Err(CodexWebsocketError::protocol("closed", true)))
        })
    }
    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), CodexWebsocketError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

struct Transport {
    connections: Mutex<VecDeque<VecDeque<Result<CodexWebsocketFrame, CodexWebsocketError>>>>,
    connects: AtomicUsize,
}
impl CodexWebsocketTransport for Transport {
    fn connect<'a>(
        &'a self,
        _: &'a str,
        _: &'a super::codex_websockets_request::CodexWebsocketHeaders,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Box<dyn CodexWebsocketConnection>, CodexWebsocketError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.connects.fetch_add(1, Ordering::SeqCst);
            let frames = self
                .connections
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| CodexWebsocketError::protocol("no_connection", false))?;
            Ok(Box::new(Connection { frames }) as Box<dyn CodexWebsocketConnection>)
        })
    }
}

fn auth() -> Arc<CodexSubscriptionAuth> {
    let claims = serde_json::json!({"https://api.openai.com/auth":{"chatgpt_account_id":"acct"}});
    let jwt = format!(
        "h.{}.s",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
    );
    let handle = |name, kind| CodexSecretHandle::new("subscriptions", name, kind).unwrap();
    let handles = CodexCredentialHandles::new(
        handle("id", CodexSecretKind::IdToken),
        handle("access", CodexSecretKind::AccessToken),
        handle("refresh", CodexSecretKind::RefreshToken),
    )
    .unwrap();
    Arc::new(CodexSubscriptionAuth::new(
        handles,
        Arc::new(Store(CodexStoredCredentials::new(
            SecretString::new(jwt).unwrap(),
            SecretString::new("access").unwrap(),
            SecretString::new("refresh").unwrap(),
        ))),
        Arc::new(Refresh),
        Arc::new(Clock),
        Arc::new(CodexRefreshCoordinator::default()),
    ))
}

fn request() -> CodexWebsocketExecutionRequest {
    CodexWebsocketExecutionRequest {
        auth_id: "auth-a".to_owned(),
        session_id: "session-a".to_owned(),
        responses_url: "https://chatgpt.com/backend-api/codex/responses".to_owned(),
        body: br#"{"model":"gpt","input":[]}"#.to_vec(),
        headers: Default::default(),
        execution_lifecycle: None,
    }
}
