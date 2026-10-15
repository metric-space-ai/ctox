// ref: sdk/auth/claude.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
#[cfg(test)]
use std::time::SystemTime;

use serde_json::Value;
use subtle::ConstantTimeEq;

use crate::internal::auth::claude::{
    generate_pkce_codes, AuthFlowError, ClaudeAuth, ClaudeAuthBundle, ClaudeCodeExchangeTransport,
    ClaudeCredentialHandles, ClaudeSecretStore, ClaudeTokenStorage, PkceCodes, RefreshClock,
    SecretString, CLAUDE_REFRESH_LEAD,
};
use crate::internal::auth::models::{shared_token_storage, TokenStorage, TokenStorageError};
use crate::internal::misc::generate_random_state;
use crate::sdk::cliproxy::auth::Auth;

use super::{
    Authenticator, AuthenticatorError, AuthenticatorErrorKind, LoginCancellation, LoginConfig,
    LoginFuture, LoginOptions, PromptCallback, PromptError,
};

pub const CLAUDE_CALLBACK_PORT: u16 = 54_545;
pub const CLAUDE_CALLBACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);
pub const CLAUDE_MANUAL_PROMPT_DELAY: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaudeCallbackErrorKind {
    Bind,
    Cancelled,
    Closed,
    InvalidCallback,
    Prompt,
    Timeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaudeCallbackError(pub ClaudeCallbackErrorKind);

impl fmt::Display for ClaudeCallbackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Claude OAuth callback failed")
    }
}

impl std::error::Error for ClaudeCallbackError {}

#[derive(Clone)]
pub struct ClaudeCallbackResult {
    code: Option<SecretString>,
    state: Option<SecretString>,
    provider_error: Option<String>,
}

impl ClaudeCallbackResult {
    #[must_use]
    pub fn from_parts(code: &str, state: &str, provider_error: &str) -> Self {
        Self {
            code: secret(code),
            state: secret(state),
            provider_error: text(provider_error),
        }
    }
}

impl fmt::Debug for ClaudeCallbackResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeCallbackResult")
            .field("has_code", &self.code.is_some())
            .field("has_state", &self.state.is_some())
            .field("has_provider_error", &self.provider_error.is_some())
            .finish()
    }
}

pub type ClaudeCallbackFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ClaudeCallbackResult, ClaudeCallbackError>> + Send + 'a>>;
pub type ClaudeCallbackStartFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Arc<dyn ActiveClaudeCallbackSession>, ClaudeCallbackError>>
            + Send
            + 'a,
    >,
>;

pub trait ActiveClaudeCallbackSession: Send + Sync {
    fn port(&self) -> u16;
    fn wait<'a>(
        &'a self,
        cancellation: &'a LoginCancellation,
        prompt: Option<PromptCallback>,
        timeout: Duration,
        manual_prompt_delay: Duration,
    ) -> ClaudeCallbackFuture<'a>;
}

pub trait ClaudeCallbackSession: Send + Sync {
    fn start<'a>(
        &'a self,
        requested_port: u16,
        cancellation: &'a LoginCancellation,
    ) -> ClaudeCallbackStartFuture<'a>;
}

#[derive(Debug)]
pub struct ClaudeLoginPresentation {
    auth_url: String,
    pub callback_port: u16,
    pub automatic_browser_allowed: bool,
}

impl ClaudeLoginPresentation {
    pub fn auth_url(&self) -> &str {
        &self.auth_url
    }
}

pub trait ClaudeLoginPresenter: Send + Sync {
    fn present(&self, challenge: &ClaudeLoginPresentation) -> Result<(), PromptError>;
}

pub trait ClaudeStateGenerator: Send + Sync {
    fn generate(&self) -> Result<SecretString, AuthenticatorError>;
}

#[derive(Debug, Default)]
pub struct RandomClaudeStateGenerator;

impl ClaudeStateGenerator for RandomClaudeStateGenerator {
    fn generate(&self) -> Result<SecretString, AuthenticatorError> {
        let state = generate_random_state().map_err(|error| {
            AuthenticatorError::with_source(AuthenticatorErrorKind::LoginFailed, error)
        })?;
        SecretString::new(state).map_err(|error| {
            AuthenticatorError::with_source(AuthenticatorErrorKind::LoginFailed, error)
        })
    }
}

pub trait ClaudeHandleFactory: Send + Sync {
    fn handles_for(
        &self,
        record_id: &str,
    ) -> Result<ClaudeCredentialHandles, crate::internal::auth::claude::SecretStoreError>;
}

pub type ClaudeExchangeFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<(ClaudeAuthBundle, ClaudeTokenStorage), AuthFlowError>>
            + Send
            + 'a,
    >,
>;

pub trait ClaudeOAuthService: Send + Sync {
    fn auth_url(
        &self,
        state: &SecretString,
        pkce: &PkceCodes,
    ) -> Result<(String, SecretString), AuthFlowError>;
    fn exchange<'a>(
        &'a self,
        code: &'a SecretString,
        state: &'a SecretString,
        pkce: &'a PkceCodes,
    ) -> ClaudeExchangeFuture<'a>;
}

impl<T, C> ClaudeOAuthService for ClaudeAuth<T, C>
where
    T: ClaudeCodeExchangeTransport + Send + Sync,
    C: RefreshClock + Send + Sync,
{
    fn auth_url(
        &self,
        state: &SecretString,
        pkce: &PkceCodes,
    ) -> Result<(String, SecretString), AuthFlowError> {
        self.generate_auth_url(state, pkce)
    }

    fn exchange<'a>(
        &'a self,
        code: &'a SecretString,
        state: &'a SecretString,
        pkce: &'a PkceCodes,
    ) -> ClaudeExchangeFuture<'a> {
        Box::pin(async move {
            let bundle = self.exchange_code_for_tokens(code, state, pkce).await?;
            let storage = self.create_token_storage(&bundle);
            Ok((bundle, storage))
        })
    }
}

pub struct ClaudeAuthenticator {
    service: Arc<dyn ClaudeOAuthService>,
    callback: Arc<dyn ClaudeCallbackSession>,
    presenter: Arc<dyn ClaudeLoginPresenter>,
    state: Arc<dyn ClaudeStateGenerator>,
    secret_store: Arc<dyn ClaudeSecretStore>,
    handles: Arc<dyn ClaudeHandleFactory>,
}

impl ClaudeAuthenticator {
    pub fn new(
        service: Arc<dyn ClaudeOAuthService>,
        callback: Arc<dyn ClaudeCallbackSession>,
        presenter: Arc<dyn ClaudeLoginPresenter>,
        state: Arc<dyn ClaudeStateGenerator>,
        secret_store: Arc<dyn ClaudeSecretStore>,
        handles: Arc<dyn ClaudeHandleFactory>,
    ) -> Self {
        Self {
            service,
            callback,
            presenter,
            state,
            secret_store,
            handles,
        }
    }
}

impl fmt::Debug for ClaudeAuthenticator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeAuthenticator")
            .field("dependencies", &"[INJECTED]")
            .finish()
    }
}

impl Authenticator for ClaudeAuthenticator {
    fn provider(&self) -> &str {
        "claude"
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
            let pkce = generate_pkce_codes().map_err(login_error)?;
            let state = self.state.generate()?;
            let port = if options.callback_port == 0 {
                CLAUDE_CALLBACK_PORT
            } else {
                options.callback_port
            };
            let callback = self
                .callback
                .start(port, cancellation)
                .await
                .map_err(callback_error)?;
            if callback.port() == 0 {
                return Err(AuthenticatorError::new(
                    AuthenticatorErrorKind::InvalidRecord,
                ));
            }
            let (auth_url, effective_state) =
                self.service.auth_url(&state, &pkce).map_err(login_error)?;
            self.presenter
                .present(&ClaudeLoginPresentation {
                    auth_url,
                    callback_port: callback.port(),
                    automatic_browser_allowed: !options.no_browser,
                })
                .map_err(login_error)?;
            let result = callback
                .wait(
                    cancellation,
                    options.prompt.clone(),
                    CLAUDE_CALLBACK_TIMEOUT,
                    CLAUDE_MANUAL_PROMPT_DELAY,
                )
                .await
                .map_err(callback_error)?;
            if result.provider_error.is_some() {
                return Err(AuthenticatorError::new(AuthenticatorErrorKind::LoginFailed));
            }
            let returned_state = result
                .state
                .as_ref()
                .ok_or_else(|| AuthenticatorError::new(AuthenticatorErrorKind::InvalidRecord))?;
            if !secret_eq(returned_state, &effective_state) {
                return Err(AuthenticatorError::new(AuthenticatorErrorKind::LoginFailed));
            }
            let code = result
                .code
                .as_ref()
                .ok_or_else(|| AuthenticatorError::new(AuthenticatorErrorKind::InvalidRecord))?;
            let (_bundle, mut storage) = self
                .service
                .exchange(code, &effective_state, &pkce)
                .await
                .map_err(login_error)?;
            let email = storage.email().trim().to_owned();
            if email.is_empty() {
                return Err(AuthenticatorError::new(
                    AuthenticatorErrorKind::InvalidRecord,
                ));
            }
            storage.set_metadata(
                options
                    .metadata
                    .iter()
                    .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                    .collect(),
            );
            let id = format!("claude-{email}.json");
            let handles = self.handles.handles_for(&id).map_err(login_error)?;
            let adapter = ClaudeStorageAdapter {
                storage,
                store: self.secret_store.clone(),
                handles,
            };
            let mut record = Auth::default();
            record.id.clone_from(&id);
            record.provider = "claude".to_owned();
            record.file_name = id;
            record.label = email.clone();
            record
                .metadata
                .insert("email".to_owned(), Value::String(email));
            if !adapter.storage.account_uuid().trim().is_empty() {
                record.metadata.insert(
                    "account_uuid".to_owned(),
                    Value::String(adapter.storage.account_uuid().to_owned()),
                );
            }
            if !adapter.storage.organization_uuid().trim().is_empty() {
                record.metadata.insert(
                    "organization_uuid".to_owned(),
                    Value::String(adapter.storage.organization_uuid().to_owned()),
                );
            }
            if !adapter.storage.organization_name().trim().is_empty() {
                record.metadata.insert(
                    "organization_name".to_owned(),
                    Value::String(adapter.storage.organization_name().to_owned()),
                );
            }
            if !adapter.storage.device_ids().is_empty() {
                record.metadata.insert(
                    crate::internal::auth::claude::CLAUDE_DEVICE_IDS_METADATA_KEY.to_owned(),
                    Value::Array(
                        adapter
                            .storage
                            .device_ids()
                            .iter()
                            .cloned()
                            .map(Value::String)
                            .collect(),
                    ),
                );
            }
            record.storage = Some(shared_token_storage(adapter));
            Ok(Some(record))
        })
    }

    fn refresh_lead(&self) -> Option<Duration> {
        Some(CLAUDE_REFRESH_LEAD)
    }
}

struct ClaudeStorageAdapter {
    storage: ClaudeTokenStorage,
    store: Arc<dyn ClaudeSecretStore>,
    handles: ClaudeCredentialHandles,
}

impl fmt::Debug for ClaudeStorageAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeStorageAdapter")
            .field("storage", &self.storage)
            .field("store", &"[INJECTED]")
            .field("handles", &self.handles)
            .finish()
    }
}

impl TokenStorage for ClaudeStorageAdapter {
    fn save_token_to_file(&mut self, _path: &Path) -> Result<(), TokenStorageError> {
        self.storage
            .persist_credentials(self.store.as_ref(), &self.handles)
            .map_err(|error| Box::new(error) as TokenStorageError)
    }
}

fn callback_error(error: ClaudeCallbackError) -> AuthenticatorError {
    let kind = if error.0 == ClaudeCallbackErrorKind::Cancelled {
        AuthenticatorErrorKind::Cancelled
    } else {
        AuthenticatorErrorKind::LoginFailed
    };
    AuthenticatorError::with_source(kind, error)
}

fn login_error(error: impl std::error::Error + Send + Sync + 'static) -> AuthenticatorError {
    AuthenticatorError::with_source(AuthenticatorErrorKind::LoginFailed, error)
}

fn secret(value: &str) -> Option<SecretString> {
    let value = value.trim();
    (!value.is_empty())
        .then(|| SecretString::new(value.to_owned()).ok())
        .flatten()
}

fn text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn secret_eq(left: &SecretString, right: &SecretString) -> bool {
    let left = left.expose_secret().as_bytes();
    let right = right.expose_secret().as_bytes();
    left.len() == right.len() && bool::from(left.ct_eq(right))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use crate::internal::auth::claude::{
        ClaudeSecretHandle, ClaudeSecretKind, ClaudeStoredCredentials, ClaudeTokenData,
        SecretStoreError,
    };

    use super::*;

    struct State;
    impl ClaudeStateGenerator for State {
        fn generate(&self) -> Result<SecretString, AuthenticatorError> {
            Ok(SecretString::new("expected-state").unwrap())
        }
    }

    struct Callback;
    struct Active;
    impl ClaudeCallbackSession for Callback {
        fn start<'a>(&'a self, _: u16, _: &'a LoginCancellation) -> ClaudeCallbackStartFuture<'a> {
            Box::pin(async { Ok(Arc::new(Active) as Arc<dyn ActiveClaudeCallbackSession>) })
        }
    }
    impl ActiveClaudeCallbackSession for Active {
        fn port(&self) -> u16 {
            CLAUDE_CALLBACK_PORT
        }
        fn wait<'a>(
            &'a self,
            _: &'a LoginCancellation,
            _: Option<PromptCallback>,
            _: Duration,
            _: Duration,
        ) -> ClaudeCallbackFuture<'a> {
            Box::pin(async {
                Ok(ClaudeCallbackResult::from_parts(
                    "secret-code",
                    "expected-state",
                    "",
                ))
            })
        }
    }

    struct Presenter;
    impl ClaudeLoginPresenter for Presenter {
        fn present(&self, challenge: &ClaudeLoginPresentation) -> Result<(), PromptError> {
            assert!(challenge.auth_url().contains("expected-state"));
            Ok(())
        }
    }

    struct Service;
    impl ClaudeOAuthService for Service {
        fn auth_url(
            &self,
            state: &SecretString,
            _: &PkceCodes,
        ) -> Result<(String, SecretString), AuthFlowError> {
            Ok((
                format!("https://example.test/?state={}", state.expose_secret()),
                state.clone(),
            ))
        }
        fn exchange<'a>(
            &'a self,
            code: &'a SecretString,
            _: &'a SecretString,
            _: &'a PkceCodes,
        ) -> ClaudeExchangeFuture<'a> {
            assert_eq!(code.expose_secret(), "secret-code");
            Box::pin(async {
                let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
                let token = ClaudeTokenData::new(
                    SecretString::new("access-secret").unwrap(),
                    SecretString::new("refresh-secret").unwrap(),
                    "operator@example.com",
                    now + Duration::from_secs(3600),
                )
                .with_identity("account-uuid", "organization-uuid", "Example Org");
                let bundle = ClaudeAuthBundle::new(None, token, now).with_device_ids(vec![
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                ]);
                let storage = ClaudeTokenStorage::from_token_data(
                    bundle.token_data(),
                    bundle.last_refresh(),
                    None,
                )
                .with_device_ids(bundle.device_ids());
                Ok((bundle, storage))
            })
        }
    }

    struct Handles;
    impl ClaudeHandleFactory for Handles {
        fn handles_for(&self, id: &str) -> Result<ClaudeCredentialHandles, SecretStoreError> {
            ClaudeCredentialHandles::new(
                ClaudeSecretHandle::new(
                    "subscriptions",
                    format!("{id}/access"),
                    ClaudeSecretKind::AccessToken,
                )
                .unwrap(),
                ClaudeSecretHandle::new(
                    "subscriptions",
                    format!("{id}/refresh"),
                    ClaudeSecretKind::RefreshToken,
                )
                .unwrap(),
            )
        }
    }

    #[derive(Default)]
    struct Store(Mutex<Option<ClaudeStoredCredentials>>);
    impl ClaudeSecretStore for Store {
        fn load_credentials(
            &self,
            _: &ClaudeCredentialHandles,
        ) -> Result<ClaudeStoredCredentials, SecretStoreError> {
            self.0
                .lock()
                .unwrap()
                .clone()
                .ok_or(SecretStoreError::Missing)
        }
        fn store_credentials(
            &self,
            _: &ClaudeCredentialHandles,
            credentials: &ClaudeStoredCredentials,
        ) -> Result<(), SecretStoreError> {
            *self.0.lock().unwrap() = Some(credentials.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn full_login_uses_injected_callback_and_secret_store_only() {
        let store = Arc::new(Store::default());
        let auth = ClaudeAuthenticator::new(
            Arc::new(Service),
            Arc::new(Callback),
            Arc::new(Presenter),
            Arc::new(State),
            store.clone(),
            Arc::new(Handles),
        );
        let record = auth
            .login(
                &LoginCancellation::default(),
                &LoginConfig::default(),
                &LoginOptions::default(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.id, "claude-operator@example.com.json");
        assert_eq!(record.metadata["account_uuid"], "account-uuid");
        assert_eq!(record.metadata["organization_uuid"], "organization-uuid");
        assert_eq!(record.metadata["organization_name"], "Example Org");
        assert_eq!(
            record.metadata[crate::internal::auth::claude::CLAUDE_DEVICE_IDS_METADATA_KEY],
            serde_json::json!(["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"])
        );
        assert!(!format!("{record:?}").contains("access-secret"));
        record
            .storage
            .unwrap()
            .lock()
            .unwrap()
            .save_token_to_file(Path::new("ignored.json"))
            .unwrap();
        assert_eq!(
            store
                .0
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .access_token()
                .expose_secret(),
            "access-secret"
        );
    }

}
