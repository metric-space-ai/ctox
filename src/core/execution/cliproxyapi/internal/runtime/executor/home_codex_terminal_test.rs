// ref: internal/runtime/executor/home_codex_terminal_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! CTOX owns subscription selection instead of the upstream Home dispatcher.
//! This mirror therefore proves the same terminal invariant at the injected
//! credential/transport boundary: a committed terminal failure cannot retain
//! either selection or connection state for the next request.

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

struct CountingStore {
    credentials: CodexStoredCredentials,
    loads: Arc<AtomicUsize>,
}

impl CodexSecretStore for CountingStore {
    fn load_credentials(
        &self,
        _: &CodexCredentialHandles,
    ) -> Result<CodexStoredCredentials, SecretStoreError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.credentials.clone())
    }

    fn store_credentials(
        &self,
        _: &CodexCredentialHandles,
        _: &CodexStoredCredentials,
    ) -> Result<(), SecretStoreError> {
        Ok(())
    }
}

struct FixedClock;

impl RefreshClock for FixedClock {
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

struct NoRefresh;

impl CodexRefreshTransport for NoRefresh {
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

struct ScriptedConnection {
    frames: VecDeque<Result<CodexWebsocketFrame, CodexWebsocketError>>,
}

impl CodexWebsocketConnection for ScriptedConnection {
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
                .unwrap_or_else(|| Err(CodexWebsocketError::protocol("closed", false)))
        })
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), CodexWebsocketError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

struct ScriptedTransport {
    connections: Mutex<VecDeque<VecDeque<Result<CodexWebsocketFrame, CodexWebsocketError>>>>,
    connects: AtomicUsize,
}

impl CodexWebsocketTransport for ScriptedTransport {
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
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .pop_front()
                .ok_or_else(|| CodexWebsocketError::protocol("missing_connection", false))?;
            Ok(Box::new(ScriptedConnection { frames }) as Box<dyn CodexWebsocketConnection>)
        })
    }
}

fn subscription_auth(loads: Arc<AtomicUsize>) -> Arc<CodexSubscriptionAuth> {
    let claims = serde_json::json!({
        "https://api.openai.com/auth": {"chatgpt_account_id": "account"}
    });
    let jwt = format!(
        "h.{}.s",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).expect("claims encode"))
    );
    let handle = |name, kind| {
        CodexSecretHandle::new("subscriptions", name, kind).expect("valid secret handle")
    };
    let handles = CodexCredentialHandles::new(
        handle("id", CodexSecretKind::IdToken),
        handle("access", CodexSecretKind::AccessToken),
        handle("refresh", CodexSecretKind::RefreshToken),
    )
    .expect("valid credential handles");
    Arc::new(CodexSubscriptionAuth::new(
        handles,
        Arc::new(CountingStore {
            credentials: CodexStoredCredentials::new(
                SecretString::new(jwt).expect("id token"),
                SecretString::new("access").expect("access token"),
                SecretString::new("refresh").expect("refresh token"),
            ),
            loads,
        }),
        Arc::new(NoRefresh),
        Arc::new(FixedClock),
        Arc::new(CodexRefreshCoordinator::default()),
    ))
}

fn request() -> CodexWebsocketExecutionRequest {
    CodexWebsocketExecutionRequest {
        auth_id: "home-codex".to_owned(),
        session_id: "terminal-home-session".to_owned(),
        responses_url: "https://chatgpt.com/backend-api/codex/responses".to_owned(),
        body: br#"{"model":"gpt-5-codex","input":[]}"#.to_vec(),
        headers: Default::default(),
        execution_lifecycle: None,
    }
}
