// ref: sdk/auth/devin.go:22-303
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — owned async login, selected transport and injected UI
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    Authenticator, AuthenticatorError, AuthenticatorErrorKind, LoginCancellation, LoginConfig,
    LoginFuture, LoginOptions, PromptCallback, PromptError,
};
use crate::internal::auth::devin::{
    auth::{format_session_token, DevinAuthService},
    oauth_server::OAuthServer,
    pkce::generate_pkce_codes,
};
use crate::internal::misc::{generate_random_state, parse_oauth_callback};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime},
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub const DEVIN_LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
pub const DEVIN_MANUAL_PROMPT_DELAY: Duration = Duration::from_secs(5);
const HEADLESS_PROMPT: &str = "Paste the Devin authorization code or session token directly: ";
const BROWSER_PROMPT: &str = "Paste the Devin callback URL, authorization code, or session token directly (or press Enter to keep waiting): ";

pub type DevinPromptFuture<'a> =
    Pin<Box<dyn Future<Output = Result<String, PromptError>> + Send + 'a>>;

/// The host owns browser/UI presentation and execution of its prompt callback.
/// Prompt futures must release their work on drop: do not adapt a blocking
/// callback with an unbounded detached thread. No UI or browser is launched here.
pub trait DevinLoginPresenter: Send + Sync {
    fn present(&self, challenge: &DevinLoginPresentation) -> Result<(), PromptError>;
    fn prompt<'a>(
        &'a self,
        callback: PromptCallback,
        message: &'static str,
        cancellation: &'a LoginCancellation,
    ) -> DevinPromptFuture<'a>;
}

pub struct DevinLoginPresentation {
    auth_url: String,
    callback_port: Option<u16>,
}
impl DevinLoginPresentation {
    pub fn auth_url(&self) -> &str {
        &self.auth_url
    }
    pub fn callback_port(&self) -> Option<u16> {
        self.callback_port
    }
    pub fn automatic_browser_allowed(&self) -> bool {
        self.callback_port.is_some()
    }
}
impl fmt::Debug for DevinLoginPresentation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinLoginPresentation")
            .field("callback_port", &self.callback_port)
            .finish_non_exhaustive()
    }
}

pub trait DevinClock: Send + Sync {
    fn now(&self) -> SystemTime;
}
#[derive(Debug, Default)]
pub struct SystemDevinClock;
impl DevinClock for SystemDevinClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

pub trait DevinStateGenerator: Send + Sync {
    fn generate(&self) -> Result<Zeroizing<String>, AuthenticatorError>;
}
#[derive(Debug, Default)]
pub struct RandomDevinStateGenerator;
impl DevinStateGenerator for RandomDevinStateGenerator {
    fn generate(&self) -> Result<Zeroizing<String>, AuthenticatorError> {
        generate_random_state()
            .map(Zeroizing::new)
            .map_err(login_error)
    }
}

pub struct DevinAuthenticator {
    service: Arc<DevinAuthService>,
    presenter: Arc<dyn DevinLoginPresenter>,
    state: Arc<dyn DevinStateGenerator>,
    clock: Arc<dyn DevinClock>,
    callback_port: u16,
}
impl fmt::Debug for DevinAuthenticator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinAuthenticator")
            .field("callback_port", &self.callback_port)
            .finish_non_exhaustive()
    }
}
impl DevinAuthenticator {
    pub fn new(service: Arc<DevinAuthService>, presenter: Arc<dyn DevinLoginPresenter>) -> Self {
        Self {
            service,
            presenter,
            state: Arc::new(RandomDevinStateGenerator),
            clock: Arc::new(SystemDevinClock),
            callback_port: 0,
        }
    }
    pub fn with_callback_port(mut self, port: u16) -> Self {
        self.callback_port = port;
        self
    }
    pub fn with_state_generator(mut self, state: Arc<dyn DevinStateGenerator>) -> Self {
        self.state = state;
        self
    }
    pub fn with_clock(mut self, clock: Arc<dyn DevinClock>) -> Self {
        self.clock = clock;
        self
    }

    async fn login_owned(
        &self,
        cancellation: &LoginCancellation,
        options: &LoginOptions,
    ) -> Result<Option<crate::sdk::cliproxy::auth::Auth>, AuthenticatorError> {
        let pkce = generate_pkce_codes().map_err(login_error)?;
        let verifier = Zeroizing::new(pkce.code_verifier);
        let state = self.state.generate()?;
        if state.is_empty() || state.trim() != state.as_str() {
            return Err(AuthenticatorError::new(
                AuthenticatorErrorKind::InvalidRecord,
            ));
        }
        let paste = if options.no_browser {
            let prompt = options
                .prompt
                .clone()
                .ok_or_else(|| login_error(DevinLoginError::PromptRequired))?;
            self.presenter
                .present(&DevinLoginPresentation {
                    auth_url: self.service.build_authorization_url(
                        "",
                        &pkce.code_challenge,
                        &state,
                    ),
                    callback_port: None,
                })
                .map_err(login_error)?;
            let input = Zeroizing::new(
                self.presenter
                    .prompt(prompt, HEADLESS_PROMPT, cancellation)
                    .await
                    .map_err(login_error)?,
            );
            match parse_manual_paste(&input, &state).map_err(login_error)? {
                DevinManualPaste::Empty => {
                    return Err(AuthenticatorError::new(AuthenticatorErrorKind::Cancelled))
                }
                value => value,
            }
        } else {
            let port = if options.callback_port == 0 {
                self.callback_port
            } else {
                options.callback_port
            };
            let server = OAuthServer::start(port, &state)
                .await
                .map_err(login_error)?;
            let redirect = server.redirect_uri().map_err(login_error)?;
            self.presenter
                .present(&DevinLoginPresentation {
                    auth_url: self.service.build_authorization_url(
                        &redirect,
                        &pkce.code_challenge,
                        &state,
                    ),
                    callback_port: Some(server.local_addr().map_err(login_error)?.port()),
                })
                .map_err(login_error)?;
            self.browser_result(server, cancellation, options.prompt.clone(), &state)
                .await?
        };
        let token = match paste {
            DevinManualPaste::Token(token) => Zeroizing::new(format_session_token(&token)),
            DevinManualPaste::Code(code) => Zeroizing::new(
                self.service
                    .exchange_code_for_token(&code, &verifier)
                    .await
                    .map_err(login_error)?,
            ),
            DevinManualPaste::Empty => {
                return Err(AuthenticatorError::new(
                    AuthenticatorErrorKind::InvalidRecord,
                ))
            }
        };
        let record = self
            .service
            .create_auth_record(&token, self.clock.now().into())
            .await
            .map_err(login_error)?;
        if cancellation.is_cancelled() {
            return Err(AuthenticatorError::new(AuthenticatorErrorKind::Cancelled));
        }
        // Only SDK Manager's injected store persists this completed record.
        Ok(Some(record))
    }

    async fn browser_result(
        &self,
        server: OAuthServer,
        cancellation: &LoginCancellation,
        prompt: Option<PromptCallback>,
        state: &str,
    ) -> Result<DevinManualPaste, AuthenticatorError> {
        let callback = server.wait_for_callback();
        tokio::pin!(callback);
        let manual_delay = tokio::time::sleep(DEVIN_MANUAL_PROMPT_DELAY);
        tokio::pin!(manual_delay);
        let mut prompted = false;
        let mut pending_prompt: Option<DevinPromptFuture<'_>> = None;
        loop {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(AuthenticatorError::new(AuthenticatorErrorKind::Cancelled)),
                result = &mut callback => {
                    let result = result.map_err(login_error)?;
                    if !result.is_success() || !same_state(result.state().unwrap_or_default(), state) {
                        return Err(login_error(DevinLoginError::ProviderRejected));
                    }
                    return Ok(DevinManualPaste::Code(Zeroizing::new(result.code().unwrap().to_owned())));
                },
                result = async { pending_prompt.as_mut().unwrap().await }, if pending_prompt.is_some() => {
                    pending_prompt = None;
                    if let Ok(input) = result {
                        let input = Zeroizing::new(input);
                        match parse_manual_paste(&input, state) {
                            Ok(DevinManualPaste::Empty) | Err(DevinLoginError::UnrecognizedInput) => {},
                            Ok(value) => return Ok(value),
                            Err(error) => return Err(login_error(error)),
                        }
                    }
                    // A failed optional prompt leaves the browser login active.
                },
                _ = &mut manual_delay, if prompt.is_some() && !prompted => {
                    prompted = true;
                    pending_prompt = Some(self.presenter.prompt(prompt.clone().unwrap(), BROWSER_PROMPT, cancellation));
                },
            }
        }
    }
}
impl Authenticator for DevinAuthenticator {
    fn provider(&self) -> &str {
        "devin"
    }
    fn login<'a>(
        &'a self,
        cancellation: &'a LoginCancellation,
        _config: &'a LoginConfig,
        options: &'a LoginOptions,
    ) -> LoginFuture<'a> {
        Box::pin(async move {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => Err(AuthenticatorError::new(AuthenticatorErrorKind::Cancelled)),
                result = tokio::time::timeout(DEVIN_LOGIN_TIMEOUT, self.login_owned(cancellation, options)) => {
                    result.unwrap_or_else(|_| Err(login_error(DevinLoginError::Timeout)))
                },
            }
        })
    }
    // Devin session tokens have no scheduled refresh expiry.
    fn refresh_lead(&self) -> Option<Duration> {
        None
    }
}

enum DevinManualPaste {
    Empty,
    Code(Zeroizing<String>),
    Token(Zeroizing<String>),
}
impl fmt::Debug for DevinManualPaste {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "DevinManualPaste::Empty",
            Self::Code(_) => "DevinManualPaste::Code([REDACTED])",
            Self::Token(_) => "DevinManualPaste::Token([REDACTED])",
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DevinLoginError {
    PromptRequired,
    StateMismatch,
    ProviderRejected,
    UnrecognizedInput,
    Timeout,
}
impl fmt::Display for DevinLoginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PromptRequired => "Devin headless login requires a host-owned prompt",
            Self::StateMismatch => "Devin OAuth state mismatch",
            Self::ProviderRejected => "Devin OAuth authorization rejected",
            Self::UnrecognizedInput => "unrecognized Devin callback, code or token",
            Self::Timeout => "Devin login timed out",
        })
    }
}
impl std::error::Error for DevinLoginError {}
fn login_error(error: impl std::error::Error + Send + Sync + 'static) -> AuthenticatorError {
    AuthenticatorError::with_source(AuthenticatorErrorKind::LoginFailed, error)
}
fn same_state(left: &str, right: &str) -> bool {
    let left: [u8; 32] = Sha256::digest(left.as_bytes()).into();
    let right: [u8; 32] = Sha256::digest(right.as_bytes()).into();
    bool::from(left.ct_eq(&right))
}
// ref: sdk/auth/devin.go:262-303
fn parse_manual_paste(
    input: &str,
    expected_state: &str,
) -> Result<DevinManualPaste, DevinLoginError> {
    let input = input.trim().trim_matches(['"', '\'']).trim();
    if input.is_empty() {
        return Ok(DevinManualPaste::Empty);
    }
    if input.starts_with("devin-session-token$") || input.starts_with("eyJ") {
        return Ok(DevinManualPaste::Token(Zeroizing::new(input.to_owned())));
    }
    if let Ok(Some(callback)) = parse_oauth_callback(input) {
        if !callback.error.trim().is_empty() {
            return Err(DevinLoginError::ProviderRejected);
        }
        if !callback.code.trim().is_empty() {
            // Manual code-only pastes are protected by this flow's PKCE verifier.
            // State is mandatory on the network callback, optional on a manual paste.
            if !expected_state.is_empty()
                && !callback.state.trim().is_empty()
                && !same_state(callback.state.trim(), expected_state)
            {
                return Err(DevinLoginError::StateMismatch);
            }
            return Ok(DevinManualPaste::Code(Zeroizing::new(
                callback.code.trim().to_owned(),
            )));
        }
    }
    if !input.contains([' ', '\t', '\r', '\n', '/', '?', '#', '=']) {
        return Ok(DevinManualPaste::Code(Zeroizing::new(input.to_owned())));
    }
    Err(DevinLoginError::UnrecognizedInput)
}

#[cfg(test)]
#[path = "devin_test.rs"]
mod tests;
