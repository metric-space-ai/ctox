// ref: internal/runtime/executor/codex_executor_auth.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use crate::internal::auth::codex::{
    CodexCredentialHandles, CodexRefreshCoordinator, CodexRefreshError, CodexRefreshTransport,
    CodexSecretStore, CodexStoredCredentials, RefreshClock, SecretStoreError,
};

/// Host-integrated Codex subscription refresh lifecycle.
///
/// Upstream mutates token strings in an untyped auth metadata map. This port
/// loads and atomically rotates a typed three-token snapshot in the CTOX secret
/// store. The detached task preserves upstream's `context.WithoutCancel`
/// behavior: request cancellation cannot strand half-rotated credentials.
/// ref: internal/runtime/executor/codex_executor_auth.go:14-51
pub struct CodexSubscriptionAuth {
    handles: CodexCredentialHandles,
    store: Arc<dyn CodexSecretStore>,
    transport: Arc<dyn CodexRefreshTransport>,
    clock: Arc<dyn RefreshClock>,
    coordinator: Arc<CodexRefreshCoordinator>,
    max_attempts: usize,
}

impl CodexSubscriptionAuth {
    pub fn new(
        handles: CodexCredentialHandles,
        store: Arc<dyn CodexSecretStore>,
        transport: Arc<dyn CodexRefreshTransport>,
        clock: Arc<dyn RefreshClock>,
        coordinator: Arc<CodexRefreshCoordinator>,
    ) -> Self {
        Self {
            handles,
            store,
            transport,
            clock,
            coordinator,
            max_attempts: 3,
        }
    }

    pub fn with_max_attempts(mut self, max_attempts: usize) -> Self {
        self.max_attempts = max_attempts.max(1);
        self
    }

    pub async fn load(&self) -> Result<CodexStoredCredentials, CodexSubscriptionAuthError> {
        let store = Arc::clone(&self.store);
        let handles = self.handles.clone();
        tokio::task::spawn_blocking(move || store.load_credentials(&handles))
            .await
            .map_err(|_| CodexSubscriptionAuthError::Task)?
            .map_err(CodexSubscriptionAuthError::Store)
    }

    pub async fn refresh(&self) -> Result<CodexRefreshOutcome, CodexSubscriptionAuthError> {
        let store = Arc::clone(&self.store);
        let transport = Arc::clone(&self.transport);
        let clock = Arc::clone(&self.clock);
        let coordinator = Arc::clone(&self.coordinator);
        let handles = self.handles.clone();
        let max_attempts = self.max_attempts;

        tokio::spawn(async move {
            let load_store = Arc::clone(&store);
            let load_handles = handles.clone();
            let current = tokio::task::spawn_blocking(move || {
                load_store.load_credentials_for_refresh(&load_handles)
            })
            .await
            .map_err(|_| CodexSubscriptionAuthError::Task)?
            .map_err(CodexSubscriptionAuthError::Store)?;
            let refreshed_at = clock.now();
            let token = coordinator
                .refresh(
                    transport.as_ref(),
                    clock.as_ref(),
                    current.credentials.clone(),
                    max_attempts,
                )
                .await
                .map_err(CodexSubscriptionAuthError::Refresh)?;
            let credentials = CodexStoredCredentials::new(
                token.id_token().clone(),
                token.access_token().clone(),
                token.refresh_token().clone(),
            );
            let persist_store = Arc::clone(&store);
            let persist_handles = handles.clone();
            let persist_credentials = credentials.clone();
            tokio::task::spawn_blocking(move || {
                persist_store.store_refreshed_credentials(
                    &persist_handles,
                    &current,
                    &persist_credentials,
                )
            })
            .await
            .map_err(|_| CodexSubscriptionAuthError::Task)?
            .map_err(CodexSubscriptionAuthError::Store)?;

            Ok(CodexRefreshOutcome {
                credentials,
                account_id: token.account_id().to_owned(),
                email: token.email().to_owned(),
                expires_at: token.expires_at(),
                refreshed_at,
            })
        })
        .await
        .map_err(|_| CodexSubscriptionAuthError::Task)?
    }

    pub async fn refresh_after_status(
        &self,
        status: u16,
    ) -> Result<CodexRefreshOutcome, CodexSubscriptionAuthError> {
        if status != 401 {
            return Err(CodexSubscriptionAuthError::NotUnauthorized(status));
        }
        self.refresh().await
    }
}

impl fmt::Debug for CodexSubscriptionAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexSubscriptionAuth")
            .field("handles", &self.handles)
            .field("credentials", &"[REDACTED]")
            .field("max_attempts", &self.max_attempts)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexRefreshOutcome {
    credentials: CodexStoredCredentials,
    account_id: String,
    email: String,
    expires_at: SystemTime,
    refreshed_at: SystemTime,
}

impl CodexRefreshOutcome {
    pub fn credentials(&self) -> &CodexStoredCredentials {
        &self.credentials
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn expires_at(&self) -> SystemTime {
        self.expires_at
    }

    pub fn refreshed_at(&self) -> SystemTime {
        self.refreshed_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexSubscriptionAuthError {
    Store(SecretStoreError),
    Refresh(CodexRefreshError),
    NotUnauthorized(u16),
    Task,
}

impl fmt::Display for CodexSubscriptionAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "Codex credential store failed: {error}"),
            Self::Refresh(error) => write!(formatter, "Codex refresh failed: {error}"),
            Self::NotUnauthorized(status) => {
                write!(formatter, "Codex refresh not allowed for status {status}")
            }
            Self::Task => formatter.write_str("Codex refresh task failed"),
        }
    }
}

impl std::error::Error for CodexSubscriptionAuthError {}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    use super::*;
    use crate::internal::auth::codex::{
        CodexRefreshHttpResponse, CodexRefreshRequest, CodexRefreshTransportFailure,
        CodexSecretHandle, CodexSecretKind, SecretString,
    };

    struct MemoryStore {
        credentials: Mutex<CodexStoredCredentials>,
        writes: AtomicUsize,
    }

    impl MemoryStore {
        fn new() -> Self {
            Self {
                credentials: Mutex::new(CodexStoredCredentials::new(
                    SecretString::new("id-old").unwrap(),
                    SecretString::new("access-old").unwrap(),
                    SecretString::new("refresh-old").unwrap(),
                )),
                writes: AtomicUsize::new(0),
            }
        }
    }

    impl CodexSecretStore for MemoryStore {
        fn load_credentials(
            &self,
            _handles: &CodexCredentialHandles,
        ) -> Result<CodexStoredCredentials, SecretStoreError> {
            Ok(self
                .credentials
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone())
        }

        fn store_credentials(
            &self,
            _handles: &CodexCredentialHandles,
            credentials: &CodexStoredCredentials,
        ) -> Result<(), SecretStoreError> {
            *self
                .credentials
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = credentials.clone();
            self.writes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FixedClock(SystemTime);

    impl RefreshClock for FixedClock {
        fn now(&self) -> SystemTime {
            self.0
        }

        fn sleep(
            &self,
            _duration: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<(), CodexRefreshTransportFailure>> + Send + '_>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    struct TestTransport;

    impl CodexRefreshTransport for TestTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a CodexRefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<CodexRefreshHttpResponse, CodexRefreshTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async {
                Ok(CodexRefreshHttpResponse::new(
                    200,
                    br#"{"access_token":"access-new","refresh_token":"refresh-new","expires_in":3600}"#.to_vec(),
                ))
            })
        }
    }

    fn handles() -> CodexCredentialHandles {
        let handle =
            |name, kind| CodexSecretHandle::new("provider-subscriptions", name, kind).unwrap();
        CodexCredentialHandles::new(
            handle("codex-id", CodexSecretKind::IdToken),
            handle("codex-access", CodexSecretKind::AccessToken),
            handle("codex-refresh", CodexSecretKind::RefreshToken),
        )
        .unwrap()
    }

    fn auth(store: Arc<MemoryStore>) -> CodexSubscriptionAuth {
        CodexSubscriptionAuth::new(
            handles(),
            store,
            Arc::new(TestTransport),
            Arc::new(FixedClock(
                SystemTime::UNIX_EPOCH + Duration::from_secs(10_000),
            )),
            Arc::new(CodexRefreshCoordinator::default()),
        )
    }

    #[tokio::test]
    async fn refresh_atomically_persists_the_three_token_snapshot() {
        let store = Arc::new(MemoryStore::new());
        let outcome = auth(Arc::clone(&store)).refresh().await.unwrap();
        assert_eq!(
            outcome.credentials().access_token().expose_secret(),
            "access-new"
        );
        assert_eq!(
            outcome.credentials().refresh_token().expose_secret(),
            "refresh-new"
        );
        assert_eq!(outcome.credentials().id_token().expose_secret(), "id-old");
        assert_eq!(store.writes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn only_unauthorized_status_can_trigger_refresh() {
        let store = Arc::new(MemoryStore::new());
        let auth = auth(Arc::clone(&store));
        assert_eq!(
            auth.refresh_after_status(429).await.unwrap_err(),
            CodexSubscriptionAuthError::NotUnauthorized(429)
        );
        assert_eq!(store.writes.load(Ordering::SeqCst), 0);
        auth.refresh_after_status(401).await.unwrap();
        assert_eq!(store.writes.load(Ordering::SeqCst), 1);
    }
}
