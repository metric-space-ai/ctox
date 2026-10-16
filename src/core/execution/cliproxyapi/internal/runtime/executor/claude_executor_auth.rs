// ref: internal/runtime/executor/claude_executor_auth.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use crate::internal::auth::claude::{
    ClaudeCredentialHandles, ClaudeRefreshCoordinator, ClaudeRefreshTransport, ClaudeSecretStore,
    ClaudeStoredCredentials, RefreshClock, RefreshError, SecretStoreError,
};
use crate::sdk::cliproxy::auth::{Auth, AuthPreparationError, AuthPreparer};
use chrono::{DateTime, SecondsFormat, Utc};

use super::helps::{
    claude_credential_account_uuid, ensure_claude_credential_device_pool_required,
    ClaudeCredentialDevicePoolStore, ClaudeCredentialIdentityError,
};

pub const CLAUDE_ACCOUNT_PROFILE_CHECKED_AT_KEY: &str = "claude_account_profile_checked_at";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeOAuthProfile {
    pub account_uuid: String,
    pub email: String,
    pub organization_uuid: String,
    pub organization_name: String,
}

pub trait ClaudeOAuthProfileFetcher: Send + Sync {
    fn fetch<'a>(
        &'a self,
        access_token: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<ClaudeOAuthProfile, String>> + Send + 'a>>;
}

/// Binds Claude's profile/device authorities to the provider-neutral
/// conductor capability without teaching the auth manager Claude internals.
pub struct ClaudeRequestAuthPreparer {
    device_store: Option<Arc<dyn ClaudeCredentialDevicePoolStore>>,
    fetcher: Arc<dyn ClaudeOAuthProfileFetcher>,
}

impl ClaudeRequestAuthPreparer {
    pub fn new(
        device_store: Option<Arc<dyn ClaudeCredentialDevicePoolStore>>,
        fetcher: Arc<dyn ClaudeOAuthProfileFetcher>,
    ) -> Self {
        Self {
            device_store,
            fetcher,
        }
    }

    /// Specialized-pool preparation entry point. The access token remains in
    /// the typed subscription authority and is borrowed only for the profile
    /// request; `auth` stores non-secret identity/device metadata exclusively.
    pub async fn prepare_with_access_token(
        &self,
        auth: &mut Auth,
        access_token: &str,
    ) -> Result<(), ClaudePrepareAuthError> {
        prepare_claude_request_auth_with_access_token(
            auth,
            access_token,
            self.device_store.as_deref(),
            self.fetcher.as_ref(),
        )
        .await
    }
}

impl fmt::Debug for ClaudeRequestAuthPreparer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeRequestAuthPreparer")
            .field("has_device_store", &self.device_store.is_some())
            .field("has_profile_fetcher", &true)
            .finish_non_exhaustive()
    }
}

impl AuthPreparer for ClaudeRequestAuthPreparer {
    fn should_prepare(&self, auth: &Auth) -> bool {
        should_prepare_claude_request_auth(auth)
    }

    fn prepare<'a>(
        &'a self,
        auth: &'a mut Auth,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuthPreparationError>> + Send + 'a>> {
        Box::pin(async move {
            prepare_claude_request_auth(auth, self.device_store.as_deref(), self.fetcher.as_ref())
                .await
                .map_err(|error| Arc::new(error) as AuthPreparationError)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudePrepareAuthError {
    Identity(ClaudeCredentialIdentityError),
    Profile(String),
    EmptyAccountUuid,
}
impl fmt::Display for ClaudePrepareAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identity(error) => error.fmt(f),
            Self::Profile(error) => write!(f, "populate Claude OAuth account profile: {error}"),
            Self::EmptyAccountUuid => {
                f.write_str("populate Claude OAuth account profile: account UUID is empty")
            }
        }
    }
}
impl std::error::Error for ClaudePrepareAuthError {}

/// Rust's exclusive `&mut Auth` replaces the Go metadata mutex: the device
/// pool and account identity are published atomically to request preparation.
pub async fn prepare_claude_request_auth(
    auth: &mut Auth,
    device_store: Option<&dyn ClaudeCredentialDevicePoolStore>,
    fetcher: &dyn ClaudeOAuthProfileFetcher,
) -> Result<(), ClaudePrepareAuthError> {
    let token = claude_request_auth_token(auth);
    prepare_claude_request_auth_with_access_token(auth, &token, device_store, fetcher).await
}

pub async fn prepare_claude_request_auth_with_access_token(
    auth: &mut Auth,
    access_token: &str,
    device_store: Option<&dyn ClaudeCredentialDevicePoolStore>,
    fetcher: &dyn ClaudeOAuthProfileFetcher,
) -> Result<(), ClaudePrepareAuthError> {
    if !should_prepare_claude_request_auth_with_access_token(auth, access_token) {
        return Ok(());
    }
    ensure_claude_credential_device_pool_required(device_store, auth)
        .map_err(ClaudePrepareAuthError::Identity)?;
    if !claude_credential_account_uuid(auth).is_empty() {
        return Ok(());
    }
    let profile = fetcher
        .fetch(access_token)
        .await
        .map_err(ClaudePrepareAuthError::Profile)?;
    if profile.account_uuid.trim().is_empty() {
        return Err(ClaudePrepareAuthError::EmptyAccountUuid);
    }
    auth.metadata.insert(
        "account_uuid".to_owned(),
        serde_json::Value::String(profile.account_uuid),
    );
    auth.metadata
        .insert("email".to_owned(), serde_json::Value::String(profile.email));
    auth.metadata.insert(
        "organization_uuid".to_owned(),
        serde_json::Value::String(profile.organization_uuid),
    );
    auth.metadata.insert(
        "organization_name".to_owned(),
        serde_json::Value::String(profile.organization_name),
    );
    let checked_at: DateTime<Utc> = SystemTime::now().into();
    auth.metadata.insert(
        CLAUDE_ACCOUNT_PROFILE_CHECKED_AT_KEY.to_owned(),
        serde_json::Value::String(checked_at.to_rfc3339_opts(SecondsFormat::Secs, true)),
    );
    Ok(())
}

pub fn should_prepare_claude_request_auth(auth: &Auth) -> bool {
    let token = claude_request_auth_token(auth);
    should_prepare_claude_request_auth_with_access_token(auth, &token)
}

pub fn should_prepare_claude_request_auth_with_access_token(
    auth: &Auth,
    access_token: &str,
) -> bool {
    if !access_token.starts_with("sk-ant-oat") {
        return false;
    }
    let canonical_device_pool = auth
        .metadata
        .get(crate::internal::auth::claude::CLAUDE_DEVICE_IDS_METADATA_KEY)
        .is_some_and(|value| {
            crate::internal::auth::claude::has_canonical_device_id_pool(Some(value))
        });
    !canonical_device_pool || claude_credential_account_uuid(auth).is_empty()
}

fn claude_request_auth_token(auth: &Auth) -> String {
    auth.attributes
        .get("api_key")
        .map(String::as_str)
        .or_else(|| {
            auth.metadata
                .get("access_token")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Host-integrated Claude subscription refresh lifecycle.
///
/// Unlike upstream, credentials never enter an untyped metadata map. The
/// refresh operation owns a detached Tokio task so an abandoned request does
/// not cancel token rotation halfway through. The host store must rotate the
/// pair atomically.
///
/// ref: internal/runtime/executor/claude_executor_auth.go:14-51
pub struct ClaudeSubscriptionAuth {
    handles: ClaudeCredentialHandles,
    store: Arc<dyn ClaudeSecretStore>,
    transport: Arc<dyn ClaudeRefreshTransport>,
    clock: Arc<dyn RefreshClock>,
    coordinator: Arc<ClaudeRefreshCoordinator>,
    max_attempts: usize,
}

impl ClaudeSubscriptionAuth {
    pub fn new(
        handles: ClaudeCredentialHandles,
        store: Arc<dyn ClaudeSecretStore>,
        transport: Arc<dyn ClaudeRefreshTransport>,
        clock: Arc<dyn RefreshClock>,
        coordinator: Arc<ClaudeRefreshCoordinator>,
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

    pub async fn load(&self) -> Result<ClaudeStoredCredentials, ClaudeSubscriptionAuthError> {
        let store = Arc::clone(&self.store);
        let handles = self.handles.clone();
        store
            .load_credentials_async(handles)
            .await
            .map_err(ClaudeSubscriptionAuthError::Store)
    }

    pub async fn refresh(&self) -> Result<ClaudeRefreshOutcome, ClaudeSubscriptionAuthError> {
        let store = Arc::clone(&self.store);
        let transport = Arc::clone(&self.transport);
        let clock = Arc::clone(&self.clock);
        let coordinator = Arc::clone(&self.coordinator);
        let handles = self.handles.clone();
        let max_attempts = self.max_attempts;

        tokio::spawn(async move {
            let load_store = Arc::clone(&store);
            let load_handles = handles.clone();
            let current = load_store
                .load_credentials_async(load_handles)
                .await
                .map_err(ClaudeSubscriptionAuthError::Store)?;

            let refreshed_at = clock.now();
            let token = coordinator
                .refresh(
                    transport.as_ref(),
                    clock.as_ref(),
                    current.refresh_token().clone(),
                    max_attempts,
                )
                .await
                .map_err(ClaudeSubscriptionAuthError::Refresh)?;
            let credentials = ClaudeStoredCredentials::new(
                token.access_token().clone(),
                token.refresh_token().clone(),
            );
            let persist_store = Arc::clone(&store);
            let persist_handles = handles.clone();
            let persist_credentials = credentials.clone();
            persist_store
                .store_credentials_async(persist_handles, persist_credentials)
                .await
                .map_err(ClaudeSubscriptionAuthError::Store)?;

            Ok(ClaudeRefreshOutcome {
                credentials,
                email: token.email().to_owned(),
                expires_at: token.expires_at(),
                refreshed_at,
            })
        })
        .await
        .map_err(|_| ClaudeSubscriptionAuthError::Task)?
    }

    /// Refreshes on the scheduler's unauthorized path and refuses to turn
    /// unrelated provider failures into credential churn.
    pub async fn refresh_after_status(
        &self,
        status: u16,
    ) -> Result<ClaudeRefreshOutcome, ClaudeSubscriptionAuthError> {
        if status != 401 {
            return Err(ClaudeSubscriptionAuthError::NotUnauthorized(status));
        }
        self.refresh().await
    }
}

impl fmt::Debug for ClaudeSubscriptionAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeSubscriptionAuth")
            .field("handles", &self.handles)
            .field("credentials", &"[REDACTED]")
            .field("max_attempts", &self.max_attempts)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeRefreshOutcome {
    credentials: ClaudeStoredCredentials,
    email: String,
    expires_at: SystemTime,
    refreshed_at: SystemTime,
}

impl ClaudeRefreshOutcome {
    pub fn credentials(&self) -> &ClaudeStoredCredentials {
        &self.credentials
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
pub enum ClaudeSubscriptionAuthError {
    Store(SecretStoreError),
    Refresh(RefreshError),
    NotUnauthorized(u16),
    Task,
}

impl fmt::Display for ClaudeSubscriptionAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "Claude credential store failed: {error}"),
            Self::Refresh(error) => write!(formatter, "Claude refresh failed: {error}"),
            Self::NotUnauthorized(status) => {
                write!(formatter, "Claude refresh not allowed for status {status}")
            }
            Self::Task => formatter.write_str("Claude refresh task failed"),
        }
    }
}

impl std::error::Error for ClaudeSubscriptionAuthError {}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    use tokio::sync::Notify;

    use super::*;
    use crate::internal::auth::claude::{
        ClaudeSecretHandle, ClaudeSecretKind, RefreshHttpResponse, RefreshRequest,
        RefreshTransportFailure, SecretString,
    };

    struct MemoryStore {
        credentials: Mutex<ClaudeStoredCredentials>,
        writes: AtomicUsize,
        persisted: Notify,
    }

    impl MemoryStore {
        fn new() -> Self {
            Self {
                credentials: Mutex::new(ClaudeStoredCredentials::new(
                    SecretString::new("access-old-do-not-leak").unwrap(),
                    SecretString::new("refresh-old-do-not-leak").unwrap(),
                )),
                writes: AtomicUsize::new(0),
                persisted: Notify::new(),
            }
        }
    }

    impl ClaudeSecretStore for MemoryStore {
        fn load_credentials(
            &self,
            _handles: &ClaudeCredentialHandles,
        ) -> Result<ClaudeStoredCredentials, SecretStoreError> {
            Ok(self
                .credentials
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone())
        }

        fn store_credentials(
            &self,
            _handles: &ClaudeCredentialHandles,
            credentials: &ClaudeStoredCredentials,
        ) -> Result<(), SecretStoreError> {
            *self
                .credentials
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = credentials.clone();
            self.writes.fetch_add(1, Ordering::SeqCst);
            self.persisted.notify_one();
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
        ) -> Pin<Box<dyn Future<Output = Result<(), RefreshTransportFailure>> + Send + '_>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    struct TestTransport {
        started: Notify,
        release: Notify,
        calls: AtomicUsize,
        wait_for_release: bool,
    }

    impl TestTransport {
        fn immediate() -> Self {
            Self {
                started: Notify::new(),
                release: Notify::new(),
                calls: AtomicUsize::new(0),
                wait_for_release: false,
            }
        }

        fn blocked() -> Self {
            Self {
                wait_for_release: true,
                ..Self::immediate()
            }
        }
    }

    impl ClaudeRefreshTransport for TestTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a RefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<RefreshHttpResponse, RefreshTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.started.notify_one();
                if self.wait_for_release {
                    self.release.notified().await;
                }
                Ok(RefreshHttpResponse::new(
                    200,
                    None,
                    None,
                    br#"{"access_token":"access-new-do-not-leak","refresh_token":"refresh-new-do-not-leak","expires_in":3600,"account":{"email_address":"operator@example.com"}}"#.to_vec(),
                ))
            })
        }
    }

    fn handles() -> ClaudeCredentialHandles {
        ClaudeCredentialHandles::new(
            ClaudeSecretHandle::new(
                "provider-subscriptions",
                "claude-access",
                ClaudeSecretKind::AccessToken,
            )
            .unwrap(),
            ClaudeSecretHandle::new(
                "provider-subscriptions",
                "claude-refresh",
                ClaudeSecretKind::RefreshToken,
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn auth(store: Arc<MemoryStore>, transport: Arc<TestTransport>) -> Arc<ClaudeSubscriptionAuth> {
        Arc::new(ClaudeSubscriptionAuth::new(
            handles(),
            store,
            transport,
            Arc::new(FixedClock(
                SystemTime::UNIX_EPOCH + Duration::from_secs(10_000),
            )),
            Arc::new(ClaudeRefreshCoordinator::default()),
        ))
    }

    #[tokio::test]
    async fn refresh_loads_and_persists_rotated_pair() {
        let store = Arc::new(MemoryStore::new());
        let transport = Arc::new(TestTransport::immediate());
        let auth = auth(Arc::clone(&store), Arc::clone(&transport));

        let outcome = auth.refresh().await.unwrap();
        assert_eq!(
            outcome.credentials().access_token().expose_secret(),
            "access-new-do-not-leak"
        );
        assert_eq!(outcome.email(), "operator@example.com");
        assert_eq!(store.writes.load(Ordering::SeqCst), 1);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        let persisted = store.load_credentials(&handles()).unwrap();
        assert_eq!(
            persisted.refresh_token().expose_secret(),
            "refresh-new-do-not-leak"
        );
    }

    #[tokio::test]
    async fn only_unauthorized_status_triggers_refresh() {
        let store = Arc::new(MemoryStore::new());
        let transport = Arc::new(TestTransport::immediate());
        let auth = auth(Arc::clone(&store), Arc::clone(&transport));

        assert_eq!(
            auth.refresh_after_status(500).await,
            Err(ClaudeSubscriptionAuthError::NotUnauthorized(500))
        );
        assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
        auth.refresh_after_status(401).await.unwrap();
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn caller_cancellation_does_not_cancel_rotation() {
        let store = Arc::new(MemoryStore::new());
        let transport = Arc::new(TestTransport::blocked());
        let auth = auth(Arc::clone(&store), Arc::clone(&transport));
        let caller = tokio::spawn({
            let auth = Arc::clone(&auth);
            async move { auth.refresh().await }
        });

        transport.started.notified().await;
        caller.abort();
        let _ = caller.await;
        transport.release.notify_one();
        tokio::time::timeout(Duration::from_secs(2), store.persisted.notified())
            .await
            .unwrap();

        let persisted = store.load_credentials(&handles()).unwrap();
        assert_eq!(
            persisted.access_token().expose_secret(),
            "access-new-do-not-leak"
        );
        assert_eq!(store.writes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn debug_and_errors_never_render_credentials() {
        let store = Arc::new(MemoryStore::new());
        let transport = Arc::new(TestTransport::immediate());
        let auth = auth(store, transport);
        let rendered = format!("{auth:?} {}", ClaudeSubscriptionAuthError::Task);
        assert!(!rendered.contains("access-old-do-not-leak"));
        assert!(!rendered.contains("refresh-old-do-not-leak"));
    }
}
