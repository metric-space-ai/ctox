// ref: internal/runtime/executor/antigravity_executor_auth.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use crate::internal::auth::antigravity::{
    AntigravityCredentialHandles, AntigravityRefreshCoordinator, AntigravityRefreshError,
    AntigravityRefreshTransport, AntigravitySecretStore, AntigravityStoredCredentials,
    AntigravityTokenError,
};

pub trait AntigravityAuthClock: Send + Sync {
    fn now(&self) -> SystemTime;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemAntigravityAuthClock;

impl AntigravityAuthClock for SystemAntigravityAuthClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// CTOX-owned load → Google refresh → atomic snapshot persistence lifecycle.
/// The spawned task mirrors upstream's `context.WithoutCancel`: cancellation
/// of one request cannot cancel a token rotation after it has started.
pub struct AntigravitySubscriptionAuth {
    handles: AntigravityCredentialHandles,
    store: Arc<dyn AntigravitySecretStore>,
    transport: Arc<dyn AntigravityRefreshTransport>,
    clock: Arc<dyn AntigravityAuthClock>,
    coordinator: Arc<AntigravityRefreshCoordinator>,
}

impl AntigravitySubscriptionAuth {
    pub fn new(
        handles: AntigravityCredentialHandles,
        store: Arc<dyn AntigravitySecretStore>,
        transport: Arc<dyn AntigravityRefreshTransport>,
        clock: Arc<dyn AntigravityAuthClock>,
        coordinator: Arc<AntigravityRefreshCoordinator>,
    ) -> Self {
        Self {
            handles,
            store,
            transport,
            clock,
            coordinator,
        }
    }

    pub async fn load(
        &self,
    ) -> Result<AntigravityStoredCredentials, AntigravitySubscriptionAuthError> {
        let store = Arc::clone(&self.store);
        let handles = self.handles.clone();
        store
            .load_credentials_async(handles)
            .await
            .map_err(AntigravitySubscriptionAuthError::Store)
    }

    pub async fn refresh(
        &self,
    ) -> Result<AntigravityRefreshOutcome, AntigravitySubscriptionAuthError> {
        let store = Arc::clone(&self.store);
        let transport = Arc::clone(&self.transport);
        let clock = Arc::clone(&self.clock);
        let coordinator = Arc::clone(&self.coordinator);
        let handles = self.handles.clone();
        tokio::spawn(async move {
            let load_store = Arc::clone(&store);
            let load_handles = handles.clone();
            let current = load_store
                .load_credentials_async(load_handles)
                .await
                .map_err(AntigravitySubscriptionAuthError::Store)?;
            let refreshed_at = clock.now();
            let credentials = coordinator
                .refresh(transport.as_ref(), current, refreshed_at)
                .await
                .map_err(AntigravitySubscriptionAuthError::Refresh)?;
            let persist_store = Arc::clone(&store);
            let persist_handles = handles.clone();
            let persist_credentials = credentials.clone();
            persist_store
                .store_credentials_async(persist_handles, persist_credentials)
                .await
                .map_err(AntigravitySubscriptionAuthError::Store)?;
            Ok(AntigravityRefreshOutcome {
                credentials,
                refreshed_at,
            })
        })
        .await
        .map_err(|_| AntigravitySubscriptionAuthError::Task)?
    }

    pub async fn refresh_after_status(
        &self,
        status: u16,
    ) -> Result<AntigravityRefreshOutcome, AntigravitySubscriptionAuthError> {
        if status != 401 {
            return Err(AntigravitySubscriptionAuthError::NotUnauthorized(status));
        }
        self.refresh().await
    }
}

impl fmt::Debug for AntigravitySubscriptionAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravitySubscriptionAuth")
            .field("handles", &self.handles)
            .field("credentials", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravityRefreshOutcome {
    credentials: AntigravityStoredCredentials,
    refreshed_at: SystemTime,
}

impl AntigravityRefreshOutcome {
    pub fn credentials(&self) -> &AntigravityStoredCredentials {
        &self.credentials
    }

    pub fn refreshed_at(&self) -> SystemTime {
        self.refreshed_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntigravitySubscriptionAuthError {
    Store(AntigravityTokenError),
    Refresh(AntigravityRefreshError),
    NotUnauthorized(u16),
    Task,
}

impl fmt::Display for AntigravitySubscriptionAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "Antigravity credential store failed: {error}"),
            Self::Refresh(error) => write!(formatter, "Antigravity refresh failed: {error}"),
            Self::NotUnauthorized(status) => {
                write!(
                    formatter,
                    "Antigravity refresh not allowed for status {status}"
                )
            }
            Self::Task => formatter.write_str("Antigravity refresh task failed"),
        }
    }
}

impl std::error::Error for AntigravitySubscriptionAuthError {}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    use super::*;
    use crate::internal::auth::antigravity::{
        AntigravityRefreshHttpResponse, AntigravityRefreshRequest,
        AntigravityRefreshTransportFailure, AntigravitySecretHandle, AntigravitySecretKind,
        SecretString,
    };

    struct MemoryStore {
        credentials: Mutex<AntigravityStoredCredentials>,
        writes: AtomicUsize,
    }

    impl AntigravitySecretStore for MemoryStore {
        fn load_credentials(
            &self,
            _: &AntigravityCredentialHandles,
        ) -> Result<AntigravityStoredCredentials, AntigravityTokenError> {
            Ok(self
                .credentials
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone())
        }
        fn store_credentials(
            &self,
            _: &AntigravityCredentialHandles,
            credentials: &AntigravityStoredCredentials,
        ) -> Result<(), AntigravityTokenError> {
            *self.credentials.lock().unwrap_or_else(|e| e.into_inner()) = credentials.clone();
            self.writes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FixedClock(SystemTime);
    impl AntigravityAuthClock for FixedClock {
        fn now(&self) -> SystemTime {
            self.0
        }
    }

    struct TestTransport;
    impl AntigravityRefreshTransport for TestTransport {
        fn execute<'a>(
            &'a self,
            _: &'a AntigravityRefreshRequest,
            _: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            AntigravityRefreshHttpResponse,
                            AntigravityRefreshTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async {
                Ok(AntigravityRefreshHttpResponse::new(200, br#"{"access_token":"access-new","refresh_token":"refresh-new","expires_in":3600}"#.to_vec()))
            })
        }
    }

    fn handles() -> AntigravityCredentialHandles {
        let h = |name, kind| AntigravitySecretHandle::new("subscriptions", name, kind).unwrap();
        AntigravityCredentialHandles::new(
            h("access", AntigravitySecretKind::AccessToken),
            h("refresh", AntigravitySecretKind::RefreshToken),
            h("state", AntigravitySecretKind::State),
        )
        .unwrap()
    }

    fn store(now: SystemTime) -> Arc<MemoryStore> {
        Arc::new(MemoryStore {
            credentials: Mutex::new(
                AntigravityStoredCredentials::new(
                    SecretString::new("access-old").unwrap(),
                    SecretString::new("refresh-old").unwrap(),
                    now,
                    "project-1",
                )
                .unwrap(),
            ),
            writes: AtomicUsize::new(0),
        })
    }

    #[tokio::test]
    async fn refresh_persists_one_complete_snapshot_and_only_for_401() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let store = store(now);
        let auth = AntigravitySubscriptionAuth::new(
            handles(),
            store.clone(),
            Arc::new(TestTransport),
            Arc::new(FixedClock(now)),
            Arc::new(AntigravityRefreshCoordinator::default()),
        );
        assert_eq!(
            auth.refresh_after_status(429).await.unwrap_err(),
            AntigravitySubscriptionAuthError::NotUnauthorized(429)
        );
        let outcome = auth.refresh_after_status(401).await.unwrap();
        assert_eq!(
            outcome.credentials().access_token().expose_secret(),
            "access-new"
        );
        assert_eq!(
            outcome.credentials().refresh_token().expose_secret(),
            "refresh-new"
        );
        assert_eq!(outcome.credentials().project_id(), "project-1");
        assert_eq!(store.writes.load(Ordering::SeqCst), 1);
    }
}
