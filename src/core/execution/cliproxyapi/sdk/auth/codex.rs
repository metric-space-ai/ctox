// ref: sdk/auth/codex.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::Value;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::internal::auth::codex::{
    generate_auth_url, generate_pkce_codes, parse_jwt_token, CodexAuth, CodexAuthBundle,
    CodexCodeExchangeTransport, CodexCredentialHandles,
    CodexExchangeError as InternalCodexExchangeError, CodexSecretStore, CodexTokenStorage,
    PkceCodes, RefreshClock, SecretString, CLIENT_ID,
};
use crate::internal::auth::models::{shared_token_storage, TokenStorage, TokenStorageError};
use crate::internal::misc::generate_random_state;
use crate::sdk::cliproxy::auth::Auth;

use super::{
    parse_codex_device_poll_interval, poll_codex_device_token, request_codex_device_user_code,
    should_use_codex_device_flow, Authenticator, AuthenticatorError, AuthenticatorErrorKind,
    CodexDeviceTransport, DeviceFlowError, DevicePollRuntime, LoginCancellation, LoginConfig,
    LoginFuture, LoginOptions, PromptCallback, PromptError,
    CODEX_DEVICE_TOKEN_EXCHANGE_REDIRECT_URI, CODEX_DEVICE_VERIFICATION_URL,
};

pub const CODEX_CALLBACK_PORT: u16 = 1_455;
pub const CODEX_CALLBACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);
pub const CODEX_MANUAL_PROMPT_DELAY: Duration = Duration::from_secs(15);
pub const CODEX_REFRESH_LEAD: Duration = Duration::from_secs(5 * 24 * 60 * 60);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexCallbackErrorKind {
    Bind,
    Cancelled,
    Closed,
    InvalidCallback,
    Prompt,
    Timeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodexCallbackError(pub CodexCallbackErrorKind);

impl fmt::Display for CodexCallbackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Codex OAuth callback failed")
    }
}
impl std::error::Error for CodexCallbackError {}

#[derive(Clone)]
pub struct CodexCallbackResult {
    code: Option<SecretString>,
    state: Option<SecretString>,
    provider_error: Option<String>,
}

impl CodexCallbackResult {
    pub fn from_parts(code: &str, state: &str, provider_error: &str) -> Self {
        Self {
            code: secret(code),
            state: secret(state),
            provider_error: text(provider_error),
        }
    }
}

impl fmt::Debug for CodexCallbackResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexCallbackResult")
            .field("has_code", &self.code.is_some())
            .field("has_state", &self.state.is_some())
            .field("has_provider_error", &self.provider_error.is_some())
            .finish()
    }
}

pub type CodexCallbackFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CodexCallbackResult, CodexCallbackError>> + Send + 'a>>;
pub type CodexCallbackStartFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Arc<dyn ActiveCodexCallbackSession>, CodexCallbackError>>
            + Send
            + 'a,
    >,
>;

pub trait ActiveCodexCallbackSession: Send + Sync {
    fn port(&self) -> u16;
    fn wait<'a>(
        &'a self,
        cancellation: &'a LoginCancellation,
        prompt: Option<PromptCallback>,
        timeout: Duration,
        manual_prompt_delay: Duration,
    ) -> CodexCallbackFuture<'a>;
}

pub trait CodexCallbackSession: Send + Sync {
    fn start<'a>(
        &'a self,
        requested_port: u16,
        cancellation: &'a LoginCancellation,
    ) -> CodexCallbackStartFuture<'a>;
}

#[derive(Debug)]
pub struct CodexBrowserPresentation {
    auth_url: String,
    pub callback_port: u16,
    pub automatic_browser_allowed: bool,
}
impl CodexBrowserPresentation {
    pub fn auth_url(&self) -> &str {
        &self.auth_url
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CodexDevicePresentation {
    pub verification_url: String,
    pub user_code: String,
    pub automatic_browser_allowed: bool,
}

pub trait CodexLoginPresenter: Send + Sync {
    fn present_browser(&self, challenge: &CodexBrowserPresentation) -> Result<(), PromptError>;
    fn present_device(&self, challenge: &CodexDevicePresentation) -> Result<(), PromptError>;
}

pub trait CodexStateGenerator: Send + Sync {
    fn generate(&self) -> Result<SecretString, AuthenticatorError>;
}
#[derive(Debug, Default)]
pub struct RandomCodexStateGenerator;
impl CodexStateGenerator for RandomCodexStateGenerator {
    fn generate(&self) -> Result<SecretString, AuthenticatorError> {
        let value = generate_random_state().map_err(login_error)?;
        SecretString::new(value).map_err(login_error)
    }
}

pub trait CodexClock: Send + Sync {
    fn now(&self) -> SystemTime;
}
#[derive(Debug, Default)]
pub struct SystemCodexClock;
impl CodexClock for SystemCodexClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

pub trait CodexHandleFactory: Send + Sync {
    fn handles_for(
        &self,
        record_id: &str,
    ) -> Result<CodexCredentialHandles, crate::internal::auth::codex::SecretStoreError>;
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CodexExchangeError {
    Cancelled,
    Failed,
    InvalidToken,
}
impl fmt::Display for CodexExchangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Codex authorization-code exchange failed")
    }
}
impl std::error::Error for CodexExchangeError {}

pub type CodexExchangeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CodexAuthBundle, CodexExchangeError>> + Send + 'a>>;

/// Injected exchange boundary retained for SDK tests and alternative hosts.
/// `CodexAuth<T, C>` implements it for the canonical internal transport/clock.
pub trait CodexOAuthService: Send + Sync {
    fn exchange<'a>(
        &'a self,
        cancellation: &'a LoginCancellation,
        code: &'a SecretString,
        redirect_uri: &'a str,
        pkce: &'a PkceCodes,
    ) -> CodexExchangeFuture<'a>;
}

impl<T, C> CodexOAuthService for CodexAuth<T, C>
where
    T: CodexCodeExchangeTransport + Send + Sync,
    C: RefreshClock + Send + Sync,
{
    fn exchange<'a>(
        &'a self,
        cancellation: &'a LoginCancellation,
        code: &'a SecretString,
        redirect_uri: &'a str,
        pkce: &'a PkceCodes,
    ) -> CodexExchangeFuture<'a> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(CodexExchangeError::Cancelled);
            }
            let result = self
                .exchange_code_for_tokens_with_redirect(code, redirect_uri, pkce)
                .await
                .map_err(map_internal_exchange_error)?;
            if cancellation.is_cancelled() {
                Err(CodexExchangeError::Cancelled)
            } else {
                Ok(result)
            }
        })
    }
}

fn map_internal_exchange_error(error: InternalCodexExchangeError) -> CodexExchangeError {
    match error {
        InternalCodexExchangeError::Transport(
            crate::internal::auth::codex::CodexRefreshTransportFailure::Cancelled,
        ) => CodexExchangeError::Cancelled,
        InternalCodexExchangeError::Token(_) | InternalCodexExchangeError::InvalidResponse => {
            CodexExchangeError::InvalidToken
        }
        _ => CodexExchangeError::Failed,
    }
}

pub struct CodexAuthenticator {
    service: Arc<dyn CodexOAuthService>,
    callback: Arc<dyn CodexCallbackSession>,
    presenter: Arc<dyn CodexLoginPresenter>,
    state: Arc<dyn CodexStateGenerator>,
    clock: Arc<dyn CodexClock>,
    secret_store: Arc<dyn CodexSecretStore>,
    handles: Arc<dyn CodexHandleFactory>,
    device_transport: Arc<dyn CodexDeviceTransport>,
    device_runtime: Arc<dyn DevicePollRuntime>,
}

impl CodexAuthenticator {
#[allow(clippy::too_many_arguments)]
    pub fn new(
        service: Arc<dyn CodexOAuthService>,
        callback: Arc<dyn CodexCallbackSession>,
        presenter: Arc<dyn CodexLoginPresenter>,
        state: Arc<dyn CodexStateGenerator>,
        clock: Arc<dyn CodexClock>,
        secret_store: Arc<dyn CodexSecretStore>,
        handles: Arc<dyn CodexHandleFactory>,
        device_transport: Arc<dyn CodexDeviceTransport>,
        device_runtime: Arc<dyn DevicePollRuntime>,
    ) -> Self {
        Self {
            service,
            callback,
            presenter,
            state,
            clock,
            secret_store,
            handles,
            device_transport,
            device_runtime,
        }
    }

    async fn browser_login(
        &self,
        cancellation: &LoginCancellation,
        options: &LoginOptions,
    ) -> Result<Auth, AuthenticatorError> {
        let pkce = generate_pkce_codes().map_err(login_error)?;
        let state = self.state.generate()?;
        let port = if options.callback_port == 0 {
            CODEX_CALLBACK_PORT
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
        let auth_url = generate_auth_url(state.expose_secret(), &pkce);
        self.presenter
            .present_browser(&CodexBrowserPresentation {
                auth_url,
                callback_port: callback.port(),
                automatic_browser_allowed: !options.no_browser,
            })
            .map_err(login_error)?;
        let result = callback
            .wait(
                cancellation,
                options.prompt.clone(),
                CODEX_CALLBACK_TIMEOUT,
                CODEX_MANUAL_PROMPT_DELAY,
            )
            .await
            .map_err(callback_error)?;
        if result.provider_error.is_some() {
            return Err(AuthenticatorError::new(AuthenticatorErrorKind::LoginFailed));
        }
        let returned = result
            .state
            .as_ref()
            .ok_or_else(|| AuthenticatorError::new(AuthenticatorErrorKind::InvalidRecord))?;
        if !secret_eq(returned, &state) {
            return Err(AuthenticatorError::new(AuthenticatorErrorKind::LoginFailed));
        }
        let code = result
            .code
            .as_ref()
            .ok_or_else(|| AuthenticatorError::new(AuthenticatorErrorKind::InvalidRecord))?;
        let bundle = self
            .service
            .exchange(
                cancellation,
                code,
                crate::internal::auth::codex::REDIRECT_URI,
                &pkce,
            )
            .await
            .map_err(exchange_error)?;
        self.build_record(bundle)
    }

    async fn device_login(
        &self,
        cancellation: &LoginCancellation,
        options: &LoginOptions,
    ) -> Result<Auth, AuthenticatorError> {
        let user =
            request_codex_device_user_code(self.device_transport.as_ref(), cancellation, CLIENT_ID)
                .await
                .map_err(device_error)?;
        let user_code = user.effective_user_code().to_owned();
        self.presenter
            .present_device(&CodexDevicePresentation {
                verification_url: CODEX_DEVICE_VERIFICATION_URL.to_owned(),
                user_code: user_code.clone(),
                automatic_browser_allowed: !options.no_browser,
            })
            .map_err(login_error)?;
        let token = poll_codex_device_token(
            self.device_transport.as_ref(),
            self.device_runtime.as_ref(),
            cancellation,
            user.device_auth_id.trim(),
            &user_code,
            parse_codex_device_poll_interval(&user.interval),
        )
        .await
        .map_err(device_error)?;
        let pkce = PkceCodes::new(
            SecretString::new(token.code_verifier.trim()).map_err(login_error)?,
            token.code_challenge.trim(),
        )
        .map_err(login_error)?;
        let code = SecretString::new(token.authorization_code.trim()).map_err(login_error)?;
        let bundle = self
            .service
            .exchange(
                cancellation,
                &code,
                CODEX_DEVICE_TOKEN_EXCHANGE_REDIRECT_URI,
                &pkce,
            )
            .await
            .map_err(exchange_error)?;
        self.build_record(bundle)
    }

    fn build_record(&self, bundle: CodexAuthBundle) -> Result<Auth, AuthenticatorError> {
        let token = bundle.token_data();
        let email = token.email().trim();
        if email.is_empty() {
            return Err(AuthenticatorError::new(
                AuthenticatorErrorKind::InvalidRecord,
            ));
        }
        let storage = CodexTokenStorage::from_token_data(token, bundle.last_refresh());
        let (plan, account) = parse_jwt_token(token.id_token().expose_secret())
            .map(|claims| {
                let account = claims.account_id().to_owned();
                let plan = claims.codex_auth_info.chatgpt_plan_type;
                (plan, account)
            })
            .unwrap_or_default();
        let hash = if account.trim().is_empty() {
            String::new()
        } else {
            format!("{:x}", Sha256::digest(account.trim().as_bytes()))[..8].to_owned()
        };
        let id = credential_record_id(email, &plan, &hash);
        let handles = self.handles.handles_for(&id).map_err(login_error)?;
        let storage = CodexStorageAdapter {
            storage,
            store: self.secret_store.clone(),
            handles,
        };
        let mut record = Auth::default();
        record.id.clone_from(&id);
        record.provider = "codex".to_owned();
        record.file_name = id;
        record.label = email.to_owned();
        record.storage = Some(shared_token_storage(storage));
        record
            .metadata
            .insert("email".to_owned(), Value::String(email.to_owned()));
        record.attributes.insert("plan_type".to_owned(), plan);
        record.last_refreshed_at = chrono::DateTime::from(bundle.last_refresh());
        record.updated_at = chrono::DateTime::from(self.clock.now());
        Ok(record)
    }
}

impl fmt::Debug for CodexAuthenticator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexAuthenticator")
            .field("dependencies", &"[INJECTED]")
            .finish()
    }
}

impl Authenticator for CodexAuthenticator {
    fn provider(&self) -> &str {
        "codex"
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
            let record = if should_use_codex_device_flow(options) {
                self.device_login(cancellation, options).await?
            } else {
                self.browser_login(cancellation, options).await?
            };
            Ok(Some(record))
        })
    }
    fn refresh_lead(&self) -> Option<Duration> {
        Some(CODEX_REFRESH_LEAD)
    }
}

struct CodexStorageAdapter {
    storage: CodexTokenStorage,
    store: Arc<dyn CodexSecretStore>,
    handles: CodexCredentialHandles,
}
impl fmt::Debug for CodexStorageAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexStorageAdapter")
            .field("storage", &self.storage)
            .field("store", &"[INJECTED]")
            .field("handles", &self.handles)
            .finish()
    }
}
impl TokenStorage for CodexStorageAdapter {
    fn save_token_to_file(&mut self, _path: &Path) -> Result<(), TokenStorageError> {
        self.storage
            .persist_credentials(self.store.as_ref(), &self.handles)
            .map_err(|error| Box::new(error) as TokenStorageError)
    }
}

fn credential_record_id(email: &str, plan: &str, hash: &str) -> String {
    let normalized = plan
        .trim()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-");
    match (hash.trim().is_empty(), normalized.is_empty()) {
        (false, false) => format!("codex-{}-{}-{normalized}.json", hash.trim(), email.trim()),
        (false, true) => format!("codex-{}-{}.json", hash.trim(), email.trim()),
        (true, false) => format!("codex-{}-{normalized}.json", email.trim()),
        (true, true) => format!("codex-{}.json", email.trim()),
    }
}

fn callback_error(error: CodexCallbackError) -> AuthenticatorError {
    let kind = if error.0 == CodexCallbackErrorKind::Cancelled {
        AuthenticatorErrorKind::Cancelled
    } else {
        AuthenticatorErrorKind::LoginFailed
    };
    AuthenticatorError::with_source(kind, error)
}
fn device_error(error: DeviceFlowError) -> AuthenticatorError {
    let kind = if error.kind == super::DeviceFlowErrorKind::Cancelled {
        AuthenticatorErrorKind::Cancelled
    } else {
        AuthenticatorErrorKind::LoginFailed
    };
    AuthenticatorError::with_source(kind, error)
}
fn exchange_error(error: CodexExchangeError) -> AuthenticatorError {
    let kind = if error == CodexExchangeError::Cancelled {
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
    use super::*;

#[test]
    fn callback_debug_redacts_secrets() {
        let result = CodexCallbackResult::from_parts("do-not-leak-code", "do-not-leak-state", "");
        assert!(!format!("{result:?}").contains("do-not-leak"));
    }
}
