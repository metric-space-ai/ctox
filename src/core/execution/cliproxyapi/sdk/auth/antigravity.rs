// ref: sdk/auth/antigravity.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use subtle::ConstantTimeEq;

#[cfg(test)]
use crate::internal::auth::antigravity::REFRESH_SKEW;
use crate::internal::auth::antigravity::{
    build_auth_url, AntigravityAuth, AntigravityAuthError, AntigravityAuthErrorKind,
    AntigravityCredentialHandles, AntigravitySecretStore, AntigravityStoredCredentials,
    AntigravityTokenError, SecretString, CALLBACK_PORT,
};
use crate::internal::auth::models::{shared_token_storage, TokenStorage, TokenStorageError};
use crate::internal::misc::generate_random_state;
use crate::sdk::cliproxy::auth::Auth;

use super::{
    Authenticator, AuthenticatorError, AuthenticatorErrorKind, LoginCancellation, LoginConfig,
    LoginFuture, LoginOptions, PromptCallback, PromptError,
};

pub const ANTIGRAVITY_CALLBACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);
pub const ANTIGRAVITY_MANUAL_PROMPT_DELAY: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntigravityCallbackErrorKind {
    Bind,
    Cancelled,
    Closed,
    InvalidCallback,
    Prompt,
    Timeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AntigravityCallbackError {
    pub kind: AntigravityCallbackErrorKind,
}

impl AntigravityCallbackError {
#[must_use]
    pub const fn new(kind: AntigravityCallbackErrorKind) -> Self {
        Self { kind }
    }
}

impl fmt::Display for AntigravityCallbackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            AntigravityCallbackErrorKind::Bind => "Antigravity callback listener failed",
            AntigravityCallbackErrorKind::Cancelled => "Antigravity callback was cancelled",
            AntigravityCallbackErrorKind::Closed => "Antigravity callback listener closed",
            AntigravityCallbackErrorKind::InvalidCallback => "Antigravity callback is invalid",
            AntigravityCallbackErrorKind::Prompt => "Antigravity callback prompt failed",
            AntigravityCallbackErrorKind::Timeout => "Antigravity callback timed out",
        })
    }
}

impl std::error::Error for AntigravityCallbackError {}

/// Secret-bearing OAuth callback result. Missing and empty query parameters
/// both become `None`, matching `url.Query().Get` followed by `TrimSpace` in
/// upstream. Debug deliberately exposes only presence bits.
#[derive(Clone)]
pub struct AntigravityCallbackResult {
    code: Option<SecretString>,
    state: Option<SecretString>,
    provider_error: Option<String>,
}

impl AntigravityCallbackResult {
#[must_use]
    pub fn from_parts(code: &str, state: &str, provider_error: &str) -> Self {
        Self {
            code: nonempty_secret(code),
            state: nonempty_secret(state),
            provider_error: nonempty_trimmed(provider_error),
        }
    }

    fn code(&self) -> Option<&SecretString> {
        self.code.as_ref()
    }

    fn state(&self) -> Option<&SecretString> {
        self.state.as_ref()
    }

    fn provider_error(&self) -> Option<&str> {
        self.provider_error.as_deref()
    }
}

impl fmt::Debug for AntigravityCallbackResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityCallbackResult")
            .field("has_code", &self.code.is_some())
            .field("has_state", &self.state.is_some())
            .field("has_provider_error", &self.provider_error.is_some())
            .finish()
    }
}

pub type AntigravityCallbackFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AntigravityCallbackResult, AntigravityCallbackError>>
            + Send
            + 'a,
    >,
>;

/// A started callback listener. Implementations own listener shutdown and must
/// release it when the last session reference is dropped.
pub trait ActiveAntigravityCallbackSession: Send + Sync {
    fn port(&self) -> u16;
    fn redirect_uri(&self) -> &str;

    fn wait<'a>(
        &'a self,
        cancellation: &'a LoginCancellation,
        prompt: Option<PromptCallback>,
        timeout: Duration,
        manual_prompt_delay: Duration,
    ) -> AntigravityCallbackFuture<'a>;
}

pub type AntigravityCallbackStartFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    Arc<dyn ActiveAntigravityCallbackSession>,
                    AntigravityCallbackError,
                >,
            > + Send
            + 'a,
    >,
>;

/// Injected callback-listener factory. This replaces upstream's process-owned
/// TCP listener and detached server task.
pub trait AntigravityCallbackSession: Send + Sync {
    fn start<'a>(
        &'a self,
        requested_port: u16,
        cancellation: &'a LoginCancellation,
    ) -> AntigravityCallbackStartFuture<'a>;
}

pub trait AntigravityClock: Send + Sync {
    fn now(&self) -> SystemTime;
}

#[derive(Debug, Default)]
pub struct SystemAntigravityClock;

impl AntigravityClock for SystemAntigravityClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AntigravityStateError;

impl fmt::Display for AntigravityStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Antigravity OAuth state generation failed")
    }
}

impl std::error::Error for AntigravityStateError {}

pub trait AntigravityStateGenerator: Send + Sync {
    fn generate(&self) -> Result<SecretString, AntigravityStateError>;
}

#[derive(Debug, Default)]
pub struct RandomAntigravityStateGenerator;

impl AntigravityStateGenerator for RandomAntigravityStateGenerator {
    fn generate(&self) -> Result<SecretString, AntigravityStateError> {
        let state = generate_random_state().map_err(|_| AntigravityStateError)?;
        SecretString::new(state).map_err(|_| AntigravityStateError)
    }
}

pub struct AntigravityLoginPresentation {
    auth_url: String,
    pub callback_port: u16,
    pub automatic_browser_allowed: bool,
}

impl AntigravityLoginPresentation {
#[must_use]
    pub fn auth_url(&self) -> &str {
        &self.auth_url
    }
}

impl fmt::Debug for AntigravityLoginPresentation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityLoginPresentation")
            .field("auth_url", &"[REDACTED]")
            .field("callback_port", &self.callback_port)
            .field("automatic_browser_allowed", &self.automatic_browser_allowed)
            .finish()
    }
}

pub trait AntigravityLoginPresenter: Send + Sync {
    fn present(&self, challenge: &AntigravityLoginPresentation) -> Result<(), PromptError>;
}

pub trait AntigravityHandleFactory: Send + Sync {
    fn handles_for(
        &self,
        record_id: &str,
    ) -> Result<AntigravityCredentialHandles, AntigravityTokenError>;
}

pub struct AntigravityAuthenticator {
    service: Arc<AntigravityAuth>,
    callback: Arc<dyn AntigravityCallbackSession>,
    presenter: Arc<dyn AntigravityLoginPresenter>,
    clock: Arc<dyn AntigravityClock>,
    state_generator: Arc<dyn AntigravityStateGenerator>,
    secret_store: Arc<dyn AntigravitySecretStore>,
    handles: Arc<dyn AntigravityHandleFactory>,
}

impl AntigravityAuthenticator {
#[must_use]
    pub fn new(
        service: Arc<AntigravityAuth>,
        callback: Arc<dyn AntigravityCallbackSession>,
        presenter: Arc<dyn AntigravityLoginPresenter>,
        clock: Arc<dyn AntigravityClock>,
        state_generator: Arc<dyn AntigravityStateGenerator>,
        secret_store: Arc<dyn AntigravitySecretStore>,
        handles: Arc<dyn AntigravityHandleFactory>,
    ) -> Self {
        Self {
            service,
            callback,
            presenter,
            clock,
            state_generator,
            secret_store,
            handles,
        }
    }
}

impl fmt::Debug for AntigravityAuthenticator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityAuthenticator")
            .field("service", &"[INJECTED]")
            .field("callback", &"[INJECTED]")
            .field("presenter", &"[INJECTED]")
            .field("clock", &"[INJECTED]")
            .field("state_generator", &"[INJECTED]")
            .field("secret_store", &"[INJECTED]")
            .field("handles", &"[INJECTED]")
            .finish()
    }
}

impl Authenticator for AntigravityAuthenticator {
    fn provider(&self) -> &str {
        "antigravity"
    }

    fn login<'a>(
        &'a self,
        cancellation: &'a LoginCancellation,
        _config: &'a LoginConfig,
        options: &'a LoginOptions,
    ) -> LoginFuture<'a> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(AuthenticatorError::new(AuthenticatorErrorKind::Cancelled));
            }

            let state = self.state_generator.generate().map_err(|error| {
                AuthenticatorError::with_source(AuthenticatorErrorKind::LoginFailed, error)
            })?;
            let requested_port = if options.callback_port == 0 {
                CALLBACK_PORT
            } else {
                options.callback_port
            };
            let callback = self
                .callback
                .start(requested_port, cancellation)
                .await
                .map_err(callback_error)?;
            let redirect_uri = callback.redirect_uri().trim();
            if callback.port() == 0 || redirect_uri.is_empty() {
                return Err(AuthenticatorError::new(
                    AuthenticatorErrorKind::InvalidRecord,
                ));
            }

            let auth_url = build_auth_url(state.expose_secret(), Some(redirect_uri));
            self.presenter
                .present(&AntigravityLoginPresentation {
                    auth_url,
                    callback_port: callback.port(),
                    automatic_browser_allowed: !options.no_browser,
                })
                .map_err(|error| {
                    AuthenticatorError::with_source(AuthenticatorErrorKind::LoginFailed, error)
                })?;

            let callback_result = callback
                .wait(
                    cancellation,
                    options.prompt.clone(),
                    ANTIGRAVITY_CALLBACK_TIMEOUT,
                    ANTIGRAVITY_MANUAL_PROMPT_DELAY,
                )
                .await
                .map_err(callback_error)?;
            if callback_result.provider_error().is_some() {
                return Err(AuthenticatorError::new(AuthenticatorErrorKind::LoginFailed));
            }
            let returned_state = callback_result
                .state()
                .ok_or_else(|| AuthenticatorError::new(AuthenticatorErrorKind::InvalidRecord))?;
            if !secret_eq(returned_state, &state) {
                return Err(AuthenticatorError::new(AuthenticatorErrorKind::LoginFailed));
            }
            let code = callback_result
                .code()
                .ok_or_else(|| AuthenticatorError::new(AuthenticatorErrorKind::InvalidRecord))?;

            let token = self
                .service
                .exchange_code_for_tokens(cancellation, code.expose_secret(), redirect_uri)
                .await
                .map_err(auth_error)?;
            // The SDK layer, unlike the lower-level token decoder, mirrors
            // upstream's `TrimSpace` before the token is used or persisted.
            let access_token = SecretString::new(token.access_token().expose_secret().trim())
                .map_err(|error| {
                    AuthenticatorError::with_source(AuthenticatorErrorKind::InvalidRecord, error)
                })?;
            let email = self
                .service
                .fetch_user_info(cancellation, &access_token)
                .await
                .map_err(auth_error)?;
            let project_id = self
                .service
                .fetch_project_id(cancellation, &access_token)
                .await
                .map_err(auth_error)?;
            let refresh_token = token
                .refresh_token()
                .cloned()
                .ok_or_else(|| AuthenticatorError::new(AuthenticatorErrorKind::InvalidRecord))?;

            let now = self.clock.now();
            let expires_at = checked_expiry(now, token.expires_in)
                .ok_or_else(|| AuthenticatorError::new(AuthenticatorErrorKind::InvalidRecord))?;
            let credentials = AntigravityStoredCredentials::new(
                access_token,
                refresh_token,
                expires_at,
                &project_id,
            )
            .map_err(|error| {
                AuthenticatorError::with_source(AuthenticatorErrorKind::InvalidRecord, error)
            })?;

            // Preserve upstream's logical record ID without granting it file
            // authority. The actual credentials are saved only through the
            // opaque handles returned by the injected factory.
            let id = credential_record_id(&email);
            let handles = self.handles.handles_for(&id).map_err(|error| {
                AuthenticatorError::with_source(AuthenticatorErrorKind::LoginFailed, error)
            })?;
            let storage = AntigravityStorageAdapter {
                credentials,
                secret_store: self.secret_store.clone(),
                handles,
            };

            let timestamp = unix_millis(now)
                .ok_or_else(|| AuthenticatorError::new(AuthenticatorErrorKind::InvalidRecord))?;
            let mut metadata = std::collections::BTreeMap::new();
            metadata.insert("type".to_owned(), Value::String("antigravity".to_owned()));
            metadata.insert("expires_in".to_owned(), Value::from(token.expires_in));
            metadata.insert("timestamp".to_owned(), Value::from(timestamp));
            metadata.insert(
                "expired".to_owned(),
                Value::String(format_rfc3339(expires_at)),
            );
            metadata.insert("email".to_owned(), Value::String(email.clone()));
            metadata.insert("project_id".to_owned(), Value::String(project_id.clone()));

            let mut record = Auth::default();
            record.id.clone_from(&id);
            record.provider = "antigravity".to_owned();
            record.file_name = id;
            record.label = email;
            record.storage = Some(shared_token_storage(storage));
            record.metadata = metadata;
            Ok(Some(record))
        })
    }

    fn refresh_lead(&self) -> Option<Duration> {
        // Upstream SDK refreshes five minutes before expiration. The internal
        // executor has its own wider 50-minute credential skew.
        Some(Duration::from_secs(5 * 60))
    }
}

#[derive(Clone)]
struct AntigravityStorageAdapter {
    credentials: AntigravityStoredCredentials,
    secret_store: Arc<dyn AntigravitySecretStore>,
    handles: AntigravityCredentialHandles,
}

impl fmt::Debug for AntigravityStorageAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityStorageAdapter")
            .field("credentials", &"[REDACTED]")
            .field("secret_store", &"[INJECTED]")
            .field("handles", &self.handles)
            .finish()
    }
}

impl TokenStorage for AntigravityStorageAdapter {
    fn save_token_to_file(&mut self, _auth_file_path: &Path) -> Result<(), TokenStorageError> {
        self.secret_store
            .store_credentials(&self.handles, &self.credentials)
            .map_err(|error| Box::new(error) as TokenStorageError)
    }
}

fn auth_error(error: AntigravityAuthError) -> AuthenticatorError {
    let kind = if error.kind == AntigravityAuthErrorKind::Cancelled {
        AuthenticatorErrorKind::Cancelled
    } else {
        AuthenticatorErrorKind::LoginFailed
    };
    AuthenticatorError::with_source(kind, error)
}

fn callback_error(error: AntigravityCallbackError) -> AuthenticatorError {
    let kind = if error.kind == AntigravityCallbackErrorKind::Cancelled {
        AuthenticatorErrorKind::Cancelled
    } else {
        AuthenticatorErrorKind::LoginFailed
    };
    AuthenticatorError::with_source(kind, error)
}

fn secret_eq(left: &SecretString, right: &SecretString) -> bool {
    let left = left.expose_secret().as_bytes();
    let right = right.expose_secret().as_bytes();
    left.len() == right.len() && bool::from(left.ct_eq(right))
}

fn nonempty_secret(value: &str) -> Option<SecretString> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        SecretString::new(value.to_owned()).ok()
    }
}

fn nonempty_trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn checked_expiry(now: SystemTime, expires_in: i64) -> Option<SystemTime> {
    if expires_in >= 0 {
        now.checked_add(Duration::from_secs(expires_in as u64))
    } else {
        now.checked_sub(Duration::from_secs(expires_in.unsigned_abs()))
    }
}

fn unix_millis(value: SystemTime) -> Option<i64> {
    match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_millis()).ok(),
        Err(error) => i64::try_from(error.duration().as_millis())
            .ok()
            .and_then(i64::checked_neg),
    }
}

fn credential_record_id(email: &str) -> String {
    let email = email.trim();
    if email.is_empty() {
        "antigravity.json".to_owned()
    } else {
        format!("antigravity-{email}.json")
    }
}

fn format_rfc3339(value: SystemTime) -> String {
    DateTime::<Utc>::from(value).to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use crate::internal::auth::antigravity::{
        AntigravityFlowTransport, AntigravityHttpFuture, AntigravityHttpRequest,
        AntigravityHttpResponse, AntigravityHttpTransportFailure, AntigravitySecretHandle,
        AntigravitySecretKind,
    };

    use super::*;

    struct FixedClock(SystemTime);

    impl AntigravityClock for FixedClock {
        fn now(&self) -> SystemTime {
            self.0
        }
    }

    struct FixedState;

    impl AntigravityStateGenerator for FixedState {
        fn generate(&self) -> Result<SecretString, AntigravityStateError> {
            SecretString::new("state-secret").map_err(|_| AntigravityStateError)
        }
    }

    struct SequenceTransport {
        responses: Mutex<VecDeque<AntigravityHttpResponse>>,
        requests: Mutex<Vec<String>>,
    }

    impl SequenceTransport {
        fn new(responses: impl IntoIterator<Item = AntigravityHttpResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().collect()),
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl AntigravityFlowTransport for SequenceTransport {
        fn execute<'a>(
            &'a self,
            request: &'a AntigravityHttpRequest,
            _timeout: Duration,
            cancellation: &'a LoginCancellation,
        ) -> AntigravityHttpFuture<'a> {
            Box::pin(async move {
                if cancellation.is_cancelled() {
                    return Err(AntigravityHttpTransportFailure::Cancelled);
                }
                self.requests.lock().unwrap().push(request.url.clone());
                self.responses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .ok_or(AntigravityHttpTransportFailure::Protocol)
            })
        }
    }

    struct CallbackFactory {
        active: Arc<Callback>,
        requested_ports: Mutex<Vec<u16>>,
    }

    impl AntigravityCallbackSession for CallbackFactory {
        fn start<'a>(
            &'a self,
            requested_port: u16,
            cancellation: &'a LoginCancellation,
        ) -> AntigravityCallbackStartFuture<'a> {
            Box::pin(async move {
                if cancellation.is_cancelled() {
                    return Err(AntigravityCallbackError::new(
                        AntigravityCallbackErrorKind::Cancelled,
                    ));
                }
                self.requested_ports.lock().unwrap().push(requested_port);
                let active: Arc<dyn ActiveAntigravityCallbackSession> = self.active.clone();
                Ok(active)
            })
        }
    }

    struct Callback {
        result: AntigravityCallbackResult,
        wait_arguments: Mutex<Vec<(bool, Duration, Duration)>>,
    }

    impl ActiveAntigravityCallbackSession for Callback {
        fn port(&self) -> u16 {
            42_424
        }

        fn redirect_uri(&self) -> &str {
            "http://localhost:42424/oauth-callback"
        }

        fn wait<'a>(
            &'a self,
            cancellation: &'a LoginCancellation,
            prompt: Option<PromptCallback>,
            timeout: Duration,
            manual_prompt_delay: Duration,
        ) -> AntigravityCallbackFuture<'a> {
            Box::pin(async move {
                if cancellation.is_cancelled() {
                    return Err(AntigravityCallbackError::new(
                        AntigravityCallbackErrorKind::Cancelled,
                    ));
                }
                self.wait_arguments.lock().unwrap().push((
                    prompt.is_some(),
                    timeout,
                    manual_prompt_delay,
                ));
                Ok(self.result.clone())
            })
        }
    }

    #[derive(Default)]
    struct Presenter(Mutex<Vec<(String, u16, bool)>>);

    impl AntigravityLoginPresenter for Presenter {
        fn present(&self, challenge: &AntigravityLoginPresentation) -> Result<(), PromptError> {
            self.0.lock().unwrap().push((
                challenge.auth_url().to_owned(),
                challenge.callback_port,
                challenge.automatic_browser_allowed,
            ));
            Ok(())
        }
    }

    #[derive(Default)]
    struct Store(Mutex<Option<AntigravityStoredCredentials>>);

    impl AntigravitySecretStore for Store {
        fn load_credentials(
            &self,
            _handles: &AntigravityCredentialHandles,
        ) -> Result<AntigravityStoredCredentials, AntigravityTokenError> {
            self.0
                .lock()
                .unwrap()
                .clone()
                .ok_or(AntigravityTokenError::Missing)
        }

        fn store_credentials(
            &self,
            _handles: &AntigravityCredentialHandles,
            credentials: &AntigravityStoredCredentials,
        ) -> Result<(), AntigravityTokenError> {
            *self.0.lock().unwrap() = Some(credentials.clone());
            Ok(())
        }
    }

    struct Handles;

    impl AntigravityHandleFactory for Handles {
        fn handles_for(
            &self,
            record_id: &str,
        ) -> Result<AntigravityCredentialHandles, AntigravityTokenError> {
            AntigravityCredentialHandles::new(
                AntigravitySecretHandle::new(
                    record_id,
                    "access",
                    AntigravitySecretKind::AccessToken,
                )?,
                AntigravitySecretHandle::new(
                    record_id,
                    "refresh",
                    AntigravitySecretKind::RefreshToken,
                )?,
                AntigravitySecretHandle::new(record_id, "state", AntigravitySecretKind::State)?,
            )
        }
    }

    fn response(body: &str) -> AntigravityHttpResponse {
        AntigravityHttpResponse::new(200, body.as_bytes().to_vec())
    }

    fn authenticator(
        callback_result: AntigravityCallbackResult,
        token_json: &str,
    ) -> (
        AntigravityAuthenticator,
        Arc<SequenceTransport>,
        Arc<CallbackFactory>,
        Arc<Presenter>,
        Arc<Store>,
    ) {
        let transport = Arc::new(SequenceTransport::new([
            response(token_json),
            response(r#"{"email":" user@example.com "}"#),
            response(r#"{"cloudaicompanionProject":" project-42 "}"#),
        ]));
        let callback = Arc::new(Callback {
            result: callback_result,
            wait_arguments: Mutex::new(Vec::new()),
        });
        let callback_factory = Arc::new(CallbackFactory {
            active: callback,
            requested_ports: Mutex::new(Vec::new()),
        });
        let presenter = Arc::new(Presenter::default());
        let store = Arc::new(Store::default());
        let authenticator = AntigravityAuthenticator::new(
            Arc::new(AntigravityAuth::new(transport.clone())),
            callback_factory.clone(),
            presenter.clone(),
            Arc::new(FixedClock(UNIX_EPOCH + Duration::from_secs(1_000))),
            Arc::new(FixedState),
            store.clone(),
            Arc::new(Handles),
        );
        (authenticator, transport, callback_factory, presenter, store)
    }

#[tokio::test]
    async fn full_login_preserves_public_metadata_and_persists_only_in_secret_store() {
        let (authenticator, transport, callback, presenter, store) = authenticator(
            AntigravityCallbackResult::from_parts(" oauth-code ", " state-secret ", ""),
            r#"{"access_token":" access-secret ","refresh_token":"refresh-secret","expires_in":3600,"token_type":"Bearer"}"#,
        );
        let mut options = LoginOptions {
            no_browser: true,
            callback_port: 31_337,
            ..LoginOptions::default()
        };
        options.prompt = Some(Arc::new(|_| Ok(String::new())));
        let mut record = authenticator
            .login(
                &LoginCancellation::default(),
                &LoginConfig::default(),
                &options,
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(*callback.requested_ports.lock().unwrap(), vec![31_337]);
        assert_eq!(record.id, "antigravity-user@example.com.json");
        assert_eq!(record.file_name, record.id);
        assert_eq!(record.label, "user@example.com");
        assert_eq!(record.provider, "antigravity");
        assert_eq!(record.metadata["type"], "antigravity");
        assert_eq!(record.metadata["email"], "user@example.com");
        assert_eq!(record.metadata["project_id"], "project-42");
        assert_eq!(record.metadata["expires_in"], 3600);
        assert_eq!(record.metadata["timestamp"], 1_000_000);
        assert_eq!(record.metadata["expired"], "1970-01-01T01:16:40Z");
        assert!(!record.metadata.contains_key("access_token"));
        assert!(!record.metadata.contains_key("refresh_token"));
        let encoded = serde_json::to_string(&record.metadata).unwrap();
        assert!(!encoded.contains("access-secret"));
        assert!(!encoded.contains("refresh-secret"));

        let presented = presenter.0.lock().unwrap();
        assert_eq!(presented.len(), 1);
        assert_eq!(presented[0].1, 42_424);
        assert!(!presented[0].2);
        let url = url::Url::parse(&presented[0].0).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["state"], "state-secret");
        assert_eq!(
            query["redirect_uri"],
            "http://localhost:42424/oauth-callback"
        );
        drop(presented);

        let wait = callback.active.wait_arguments.lock().unwrap();
        assert_eq!(
            wait.as_slice(),
            &[(
                true,
                ANTIGRAVITY_CALLBACK_TIMEOUT,
                ANTIGRAVITY_MANUAL_PROMPT_DELAY
            )]
        );
        drop(wait);

        assert!(store.0.lock().unwrap().is_none());
        record
            .storage
            .take()
            .unwrap()
            .lock()
            .unwrap()
            .save_token_to_file(Path::new("ignored-plaintext-path"))
            .unwrap();
        let persisted = store.0.lock().unwrap().clone().unwrap();
        assert_eq!(persisted.access_token().expose_secret(), "access-secret");
        assert_eq!(persisted.refresh_token().expose_secret(), "refresh-secret");
        assert_eq!(persisted.project_id(), "project-42");
        assert_eq!(transport.requests.lock().unwrap().len(), 3);
    }

#[tokio::test]
    async fn invalid_state_stops_before_token_exchange_and_redacts_callback() {
        let callback_result =
            AntigravityCallbackResult::from_parts("authorization-secret", "wrong-state-secret", "");
        let rendered = format!("{callback_result:?}");
        assert!(!rendered.contains("authorization-secret"));
        assert!(!rendered.contains("wrong-state-secret"));
        let (authenticator, transport, _, _, _) = authenticator(
            callback_result,
            r#"{"access_token":"unused","refresh_token":"unused","expires_in":1}"#,
        );
        let error = authenticator
            .login(
                &LoginCancellation::default(),
                &LoginConfig::default(),
                &LoginOptions::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind, AuthenticatorErrorKind::LoginFailed);
        assert!(transport.requests.lock().unwrap().is_empty());
    }

#[tokio::test]
    async fn empty_refresh_token_remains_distinct_and_is_not_persisted() {
        let (authenticator, _, _, _, store) = authenticator(
            AntigravityCallbackResult::from_parts("code", "state-secret", ""),
            r#"{"access_token":"access-secret","refresh_token":"","expires_in":3600}"#,
        );
        let error = authenticator
            .login(
                &LoginCancellation::default(),
                &LoginConfig::default(),
                &LoginOptions::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind, AuthenticatorErrorKind::InvalidRecord);
        assert!(store.0.lock().unwrap().is_none());
    }

#[tokio::test]
    async fn cancelled_login_never_opens_callback_or_calls_provider() {
        let (authenticator, transport, callback, presenter, _) = authenticator(
            AntigravityCallbackResult::from_parts("code", "state-secret", ""),
            r#"{"access_token":"access","refresh_token":"refresh","expires_in":3600}"#,
        );
        let cancellation = LoginCancellation::default();
        cancellation.cancel();
        let error = authenticator
            .login(
                &cancellation,
                &LoginConfig::default(),
                &LoginOptions::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind, AuthenticatorErrorKind::Cancelled);
        assert!(callback.requested_ports.lock().unwrap().is_empty());
        assert!(presenter.0.lock().unwrap().is_empty());
        assert!(transport.requests.lock().unwrap().is_empty());
    }

#[test]
    fn callback_error_and_presentation_debug_do_not_render_secret_values() {
        let result =
            AntigravityCallbackResult::from_parts("code-secret", "state-secret", "provider-secret");
        let rendered = format!("{result:?}");
        for secret in ["code-secret", "state-secret", "provider-secret"] {
            assert!(!rendered.contains(secret));
        }
        let presentation = AntigravityLoginPresentation {
            auth_url: "https://example.test/?state=state-secret".to_owned(),
            callback_port: 1,
            automatic_browser_allowed: true,
        };
        assert!(!format!("{presentation:?}").contains("state-secret"));
    }
}
