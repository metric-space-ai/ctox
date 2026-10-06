// ref: internal/auth/claude/anthropic_auth.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::watch;
use zeroize::Zeroizing;

use super::anthropic::{ClaudeAuthBundle, ClaudeUserInfo};
use super::identity::{generate_device_id_pool, ClaudeIdentityError};
use super::pkce::PkceCodes;
use super::token::{ClaudeTokenData, ClaudeTokenStorage, SecretString, TokenError};

// ref: internal/auth/claude/anthropic_auth.go:24-34
pub const AUTH_URL: &str = "https://claude.ai/oauth/authorize";
pub const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
pub const REFRESH_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
pub const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
pub const ROLES_URL: &str = "https://api.anthropic.com/api/oauth/claude_cli/roles";
pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const REDIRECT_URI: &str = "http://localhost:54545/callback";
pub const AUTH_SCOPE: &str =
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";
pub const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(30);
pub const REFRESH_TIMEOUT: Duration = Duration::from_secs(30);
const REFRESH_MIN_BACKOFF: Duration = Duration::from_secs(5);
const REFRESH_MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct ExchangeRequest {
    code: SecretString,
    state: SecretString,
    code_verifier: SecretString,
}

impl ExchangeRequest {
    fn new(
        callback_code: &SecretString,
        state: &SecretString,
        pkce: &PkceCodes,
    ) -> Result<Self, AuthFlowError> {
        let mut code_parts = callback_code.expose_secret().split('#');
        let code = code_parts.next().unwrap_or_default();
        if code.is_empty() {
            return Err(AuthFlowError::MissingAuthorizationCode);
        }
        let fragment_state = code_parts.next().filter(|value| !value.is_empty());
        let effective_state = fragment_state.unwrap_or_else(|| state.expose_secret());

        Ok(Self {
            code: SecretString::new(code)?,
            state: SecretString::new(effective_state)?,
            code_verifier: SecretString::new(pkce.code_verifier.clone())?,
        })
    }

    pub fn json_body(&self) -> Result<Zeroizing<Vec<u8>>, AuthFlowError> {
        #[derive(Serialize)]
        struct WireRequest<'a> {
            grant_type: &'static str,
            code: &'a str,
            redirect_uri: &'static str,
            client_id: &'static str,
            code_verifier: &'a str,
            state: &'a str,
        }

        serde_json::to_vec(&WireRequest {
            grant_type: "authorization_code",
            code: self.code.expose_secret(),
            redirect_uri: REDIRECT_URI,
            client_id: CLIENT_ID,
            code_verifier: self.code_verifier.expose_secret(),
            state: self.state.expose_secret(),
        })
        .map(Zeroizing::new)
        .map_err(|_| AuthFlowError::RequestEncoding)
    }
}

impl fmt::Debug for ExchangeRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExchangeRequest")
            .field("code", &"[REDACTED]")
            .field("state", &"[REDACTED]")
            .field("grant_type", &"authorization_code")
            .field("client_id", &CLIENT_ID)
            .field("redirect_uri", &REDIRECT_URI)
            .field("code_verifier", &"[REDACTED]")
            .finish()
    }
}

pub struct ExchangeHttpResponse {
    status: u16,
    body: Zeroizing<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthInspectKind {
    Profile,
    Roles,
}

impl OAuthInspectKind {
    pub fn endpoint(self) -> &'static str {
        match self {
            Self::Profile => PROFILE_URL,
            Self::Roles => ROLES_URL,
        }
    }
}

#[derive(Clone)]
pub struct OAuthInspectRequest {
    kind: OAuthInspectKind,
    access_token: SecretString,
}

impl OAuthInspectRequest {
    pub fn new(kind: OAuthInspectKind, access_token: SecretString) -> Self {
        Self { kind, access_token }
    }

    pub fn kind(&self) -> OAuthInspectKind {
        self.kind
    }

    pub fn endpoint(&self) -> &'static str {
        self.kind.endpoint()
    }

    pub fn access_token(&self) -> &SecretString {
        &self.access_token
    }
}

impl fmt::Debug for OAuthInspectRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthInspectRequest")
            .field("kind", &self.kind)
            .field("endpoint", &self.endpoint())
            .field("access_token", &"[REDACTED]")
            .finish()
    }
}

pub struct OAuthInspectHttpResponse {
    status: u16,
    body: Zeroizing<Vec<u8>>,
}

impl OAuthInspectHttpResponse {
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            body: Zeroizing::new(body),
        }
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn body(&self) -> &[u8] {
        self.body.as_slice()
    }
}

impl fmt::Debug for OAuthInspectHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthInspectHttpResponse")
            .field("status", &self.status)
            .field("body", &"[REDACTED]")
            .finish()
    }
}

impl ExchangeHttpResponse {
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            body: Zeroizing::new(body),
        }
    }
}

impl fmt::Debug for ExchangeHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExchangeHttpResponse")
            .field("status", &self.status)
            .field("body", &"[REDACTED]")
            .finish()
    }
}

pub trait ClaudeCodeExchangeTransport: Send + Sync {
    fn exchange<'a>(
        &'a self,
        request: &'a ExchangeRequest,
        timeout: Duration,
    ) -> Pin<
        Box<dyn Future<Output = Result<ExchangeHttpResponse, RefreshTransportFailure>> + Send + 'a>,
    >;

    fn inspect<'a>(
        &'a self,
        _request: &'a OAuthInspectRequest,
        _timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<OAuthInspectHttpResponse, RefreshTransportFailure>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async { Err(RefreshTransportFailure::Protocol) })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthFlowError {
    EmptyState,
    EmptyCodeChallenge,
    MissingAuthorizationCode,
    RequestEncoding,
    Transport(RefreshTransportFailure),
    Http { status: u16 },
    InvalidResponse,
    Token(TokenError),
    Identity(ClaudeIdentityError),
}

impl fmt::Display for AuthFlowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyState => formatter.write_str("Claude OAuth state must not be empty"),
            Self::EmptyCodeChallenge => {
                formatter.write_str("Claude PKCE challenge must not be empty")
            }
            Self::MissingAuthorizationCode => {
                formatter.write_str("Claude authorization code is missing")
            }
            Self::RequestEncoding => {
                formatter.write_str("failed to encode Claude token exchange request")
            }
            Self::Transport(kind) => {
                write!(
                    formatter,
                    "Claude token exchange transport failed: {kind:?}"
                )
            }
            Self::Http { status } => {
                write!(
                    formatter,
                    "Claude token exchange failed with status {status}"
                )
            }
            Self::InvalidResponse => {
                formatter.write_str("Claude token exchange response is invalid")
            }
            Self::Token(_) => {
                formatter.write_str("Claude token exchange returned invalid credentials")
            }
            Self::Identity(_) => {
                formatter.write_str("Claude token exchange could not create device identity")
            }
        }
    }
}

impl std::error::Error for AuthFlowError {}

impl From<TokenError> for AuthFlowError {
    fn from(value: TokenError) -> Self {
        Self::Token(value)
    }
}

impl From<ClaudeIdentityError> for AuthFlowError {
    fn from(value: ClaudeIdentityError) -> Self {
        Self::Identity(value)
    }
}

#[derive(Debug, Default, Deserialize, Clone, PartialEq, Eq)]
pub struct OAuthProfile {
    #[serde(default)]
    account: OAuthProfileAccount,
    #[serde(default)]
    organization: OAuthProfileOrganization,
}

#[derive(Debug, Default, Deserialize, Clone, PartialEq, Eq)]
struct OAuthProfileAccount {
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    email: String,
}

#[derive(Debug, Default, Deserialize, Clone, PartialEq, Eq)]
struct OAuthProfileOrganization {
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    name: String,
}

impl OAuthProfile {
    pub fn user_info(&self) -> ClaudeUserInfo {
        ClaudeUserInfo::new(
            &self.account.uuid,
            &self.account.email,
            &self.organization.uuid,
            &self.organization.name,
        )
    }
}

pub struct ClaudeAuth<T, C = SystemRefreshClock> {
    transport: T,
    clock: C,
    refresh: ClaudeRefreshCoordinator,
}

impl<T> ClaudeAuth<T, SystemRefreshClock> {
    pub fn new(transport: T) -> Self {
        Self::with_clock(transport, SystemRefreshClock)
    }
}

impl<T, C> ClaudeAuth<T, C> {
    pub fn with_clock(transport: T, clock: C) -> Self {
        Self {
            transport,
            clock,
            refresh: ClaudeRefreshCoordinator::default(),
        }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn generate_auth_url(
        &self,
        state: &SecretString,
        pkce: &PkceCodes,
    ) -> Result<(String, SecretString), AuthFlowError> {
        if state.expose_secret().is_empty() {
            return Err(AuthFlowError::EmptyState);
        }
        if pkce.code_challenge.trim().is_empty() {
            return Err(AuthFlowError::EmptyCodeChallenge);
        }

        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query.append_pair("client_id", CLIENT_ID);
        query.append_pair("code", "true");
        query.append_pair("code_challenge", &pkce.code_challenge);
        query.append_pair("code_challenge_method", "S256");
        query.append_pair("redirect_uri", REDIRECT_URI);
        query.append_pair("response_type", "code");
        query.append_pair("scope", AUTH_SCOPE);
        query.append_pair("state", state.expose_secret());
        Ok((format!("{AUTH_URL}?{}", query.finish()), state.clone()))
    }

    pub async fn fetch_oauth_profile(
        &self,
        access_token: &SecretString,
    ) -> Result<OAuthProfile, AuthFlowError>
    where
        T: ClaudeCodeExchangeTransport,
    {
        let request = OAuthInspectRequest::new(OAuthInspectKind::Profile, access_token.clone());
        let response = self
            .transport
            .inspect(&request, EXCHANGE_TIMEOUT)
            .await
            .map_err(AuthFlowError::Transport)?;
        if !(200..300).contains(&response.status) {
            return Err(AuthFlowError::Http {
                status: response.status,
            });
        }
        let profile: OAuthProfile =
            serde_json::from_slice(&response.body).map_err(|_| AuthFlowError::InvalidResponse)?;
        if profile.account.uuid.trim().is_empty() {
            return Err(AuthFlowError::InvalidResponse);
        }
        Ok(profile)
    }

    pub async fn fetch_oauth_roles(
        &self,
        access_token: &SecretString,
    ) -> Result<serde_json::Value, AuthFlowError>
    where
        T: ClaudeCodeExchangeTransport,
    {
        let request = OAuthInspectRequest::new(OAuthInspectKind::Roles, access_token.clone());
        let response = self
            .transport
            .inspect(&request, EXCHANGE_TIMEOUT)
            .await
            .map_err(AuthFlowError::Transport)?;
        if !(200..300).contains(&response.status) {
            return Err(AuthFlowError::Http {
                status: response.status,
            });
        }
        serde_json::from_slice(&response.body).map_err(|_| AuthFlowError::InvalidResponse)
    }

    pub async fn exchange_code_for_tokens(
        &self,
        code: &SecretString,
        state: &SecretString,
        pkce: &PkceCodes,
    ) -> Result<ClaudeAuthBundle, AuthFlowError>
    where
        T: ClaudeCodeExchangeTransport,
        C: RefreshClock,
    {
        let request = ExchangeRequest::new(code, state, pkce)?;
        let response = self
            .transport
            .exchange(&request, EXCHANGE_TIMEOUT)
            .await
            .map_err(AuthFlowError::Transport)?;
        if response.status != 200 {
            return Err(AuthFlowError::Http {
                status: response.status,
            });
        }
        let mut bundle = parse_exchange_response(&response.body, self.clock.now())?;
        let device_ids = generate_device_id_pool()?;
        if let Some(profile) = inspect_oauth_account_after_exchange(
            &self.transport,
            bundle.token_data().access_token(),
        )
        .await
        {
            bundle = bundle.with_user_info(profile.user_info());
        }
        Ok(bundle.with_device_ids(device_ids))
    }

    pub async fn refresh_tokens(
        &self,
        refresh_token: SecretString,
    ) -> Result<ClaudeTokenData, RefreshError>
    where
        T: ClaudeRefreshTransport,
        C: RefreshClock,
    {
        self.refresh
            .refresh(&self.transport, &self.clock, refresh_token, 1)
            .await
    }

    pub async fn refresh_tokens_with_retry(
        &self,
        refresh_token: SecretString,
        max_attempts: usize,
    ) -> Result<ClaudeTokenData, RefreshError>
    where
        T: ClaudeRefreshTransport,
        C: RefreshClock,
    {
        self.refresh
            .refresh(&self.transport, &self.clock, refresh_token, max_attempts)
            .await
    }

    pub fn create_token_storage(&self, bundle: &ClaudeAuthBundle) -> ClaudeTokenStorage {
        ClaudeTokenStorage::from_token_data(bundle.token_data(), bundle.last_refresh(), None)
            .with_device_ids(bundle.device_ids())
    }

    pub fn update_token_storage(
        &self,
        storage: &mut ClaudeTokenStorage,
        token_data: &ClaudeTokenData,
    ) where
        C: RefreshClock,
    {
        storage.update_from_token_data(token_data, self.clock.now());
    }
}

#[cfg(feature = "anthropic-fingerprint-transport")]
#[derive(Clone, Copy)]
pub enum ClaudeProxyOverride<'a> {
    Inherit,
    Direct,
    Proxy(&'a SecretString),
}

#[cfg(feature = "anthropic-fingerprint-transport")]
pub fn new_claude_auth_with_proxy<'a>(
    configured_proxy: Option<&'a SecretString>,
    proxy_override: ClaudeProxyOverride<'a>,
) -> Result<
    ClaudeAuth<super::utls_transport::AnthropicHttpTransport>,
    super::utls_transport::AnthropicTransportBuildError,
> {
    let proxy_url = match proxy_override {
        ClaudeProxyOverride::Inherit => configured_proxy.map(SecretString::expose_secret),
        ClaudeProxyOverride::Direct => Some("direct"),
        ClaudeProxyOverride::Proxy(proxy) => Some(proxy.expose_secret()),
    };
    super::utls_transport::AnthropicHttpTransport::new(proxy_url).map(ClaudeAuth::new)
}

#[derive(Clone)]
pub struct RefreshRequest {
    refresh_token: SecretString,
}

impl RefreshRequest {
    pub fn new(refresh_token: SecretString) -> Self {
        Self { refresh_token }
    }

    pub fn refresh_token(&self) -> &SecretString {
        &self.refresh_token
    }

    pub fn json_body(&self) -> Result<Zeroizing<Vec<u8>>, RefreshError> {
        #[derive(Serialize)]
        struct WireRequest<'a> {
            client_id: &'static str,
            grant_type: &'static str,
            refresh_token: &'a str,
            scope: &'static str,
        }

        serde_json::to_vec(&WireRequest {
            client_id: CLIENT_ID,
            grant_type: "refresh_token",
            refresh_token: self.refresh_token.expose_secret(),
            scope: AUTH_SCOPE,
        })
        .map(Zeroizing::new)
        .map_err(|_| RefreshError::RequestEncoding)
    }
}

impl fmt::Debug for RefreshRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RefreshRequest")
            .field("client_id", &CLIENT_ID)
            .field("grant_type", &"refresh_token")
            .field("refresh_token", &"[REDACTED]")
            .finish()
    }
}

/// Transport result with a zeroized body because a successful body contains
/// fresh credentials. Header values are copied individually to keep the core
/// refresh policy independent from an HTTP crate.
pub struct RefreshHttpResponse {
    status: u16,
    retry_after: Option<String>,
    retry_after_ms: Option<String>,
    body: Zeroizing<Vec<u8>>,
}

impl RefreshHttpResponse {
    pub fn new(
        status: u16,
        retry_after: Option<String>,
        retry_after_ms: Option<String>,
        body: Vec<u8>,
    ) -> Self {
        Self {
            status,
            retry_after,
            retry_after_ms,
            body: Zeroizing::new(body),
        }
    }
}

impl fmt::Debug for RefreshHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RefreshHttpResponse")
            .field("status", &self.status)
            .field("retry_after", &self.retry_after)
            .field("retry_after_ms", &self.retry_after_ms)
            .field("body", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshTransportFailure {
    Timeout,
    Connect,
    Protocol,
    Cancelled,
}

pub trait ClaudeRefreshTransport: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: &'a RefreshRequest,
        timeout: Duration,
    ) -> Pin<
        Box<dyn Future<Output = Result<RefreshHttpResponse, RefreshTransportFailure>> + Send + 'a>,
    >;

    fn inspect<'a>(
        &'a self,
        _request: &'a OAuthInspectRequest,
        _timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<OAuthInspectHttpResponse, RefreshTransportFailure>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async { Err(RefreshTransportFailure::Protocol) })
    }
}

pub trait RefreshClock: Send + Sync {
    fn now(&self) -> SystemTime;
    fn sleep(
        &self,
        duration: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<(), RefreshTransportFailure>> + Send + '_>>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemRefreshClock;

impl RefreshClock for SystemRefreshClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }

    fn sleep(
        &self,
        duration: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<(), RefreshTransportFailure>> + Send + '_>> {
        Box::pin(async move {
            tokio::time::sleep(duration).await;
            Ok(())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshError {
    RequestEncoding,
    Transport(RefreshTransportFailure),
    Http { status: u16, retryable: bool },
    RateLimited { blocked_until: SystemTime },
    InvalidResponse,
    Token(TokenError),
    InvalidSingleFlightResult,
}

impl RefreshError {
    pub fn retryable(&self) -> bool {
        match self {
            // ref: anthropic_auth.go:120-129 @ 2044a01f (v8.0.12)
            // The single-use refresh token may already have been consumed when
            // a transport response is lost. Only explicit retryable HTTP
            // responses may replay it.
            Self::Transport(_) => false,
            Self::Http { retryable, .. } => *retryable,
            Self::RequestEncoding
            | Self::RateLimited { .. }
            | Self::InvalidResponse
            | Self::Token(_)
            | Self::InvalidSingleFlightResult => false,
        }
    }
}

impl fmt::Display for RefreshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RequestEncoding => formatter.write_str("failed to encode Claude refresh request"),
            Self::Transport(kind) => write!(formatter, "Claude refresh transport failed: {kind:?}"),
            Self::Http { status, .. } => write!(
                formatter,
                "Claude token refresh failed with status {status}"
            ),
            Self::RateLimited { blocked_until } => write!(
                formatter,
                "Claude refresh is temporarily blocked until {blocked_until:?}"
            ),
            Self::InvalidResponse => formatter.write_str("Claude token response is invalid"),
            Self::Token(error) => write!(formatter, "Claude token is invalid: {error}"),
            Self::InvalidSingleFlightResult => {
                formatter.write_str("Claude singleflight result is invalid")
            }
        }
    }
}

impl std::error::Error for RefreshError {}

impl From<TokenError> for RefreshError {
    fn from(value: TokenError) -> Self {
        Self::Token(value)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RefreshFingerprint([u8; 32]);

impl RefreshFingerprint {
    fn for_token(token: &SecretString) -> Self {
        Self(Sha256::digest(token.expose_secret().as_bytes()).into())
    }
}

impl fmt::Debug for RefreshFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RefreshFingerprint([REDACTED])")
    }
}

type RefreshResult = Result<ClaudeTokenData, RefreshError>;
type FlightReceiver = watch::Receiver<Option<RefreshResult>>;

/// Per-process Claude refresh coordinator.
///
/// It mirrors upstream's singleflight and 429 replay-blocking behavior while
/// indexing internal state by SHA-256 fingerprints instead of raw tokens.
/// ref: internal/auth/claude/anthropic_auth.go:37-82,326-458
#[derive(Default)]
pub struct ClaudeRefreshCoordinator {
    blocked_until: Mutex<HashMap<RefreshFingerprint, SystemTime>>,
    flights: Mutex<HashMap<RefreshFingerprint, FlightReceiver>>,
}

impl ClaudeRefreshCoordinator {
    pub async fn refresh<T, C>(
        &self,
        transport: &T,
        clock: &C,
        refresh_token: SecretString,
        max_attempts: usize,
    ) -> RefreshResult
    where
        T: ClaudeRefreshTransport + ?Sized,
        C: RefreshClock + ?Sized,
    {
        let fingerprint = RefreshFingerprint::for_token(&refresh_token);
        self.check_cooldown(fingerprint, clock.now())?;

        let (mut receiver, leader) = {
            let mut flights = lock_recover(&self.flights);
            if let Some(flight) = flights.get(&fingerprint) {
                (flight.clone(), None)
            } else {
                let (sender, receiver) = watch::channel(None);
                flights.insert(fingerprint, receiver.clone());
                (receiver, Some(sender))
            }
        };

        let Some(sender) = leader else {
            return wait_for_flight(&mut receiver).await;
        };

        let result = self
            .refresh_with_retry(transport, clock, fingerprint, &refresh_token, max_attempts)
            .await;
        if sender.send(Some(result.clone())).is_err() {
            lock_recover(&self.flights).remove(&fingerprint);
            return Err(RefreshError::InvalidSingleFlightResult);
        }
        lock_recover(&self.flights).remove(&fingerprint);
        result
    }

    async fn refresh_with_retry<T, C>(
        &self,
        transport: &T,
        clock: &C,
        fingerprint: RefreshFingerprint,
        refresh_token: &SecretString,
        max_attempts: usize,
    ) -> RefreshResult
    where
        T: ClaudeRefreshTransport + ?Sized,
        C: RefreshClock + ?Sized,
    {
        let request = RefreshRequest::new(refresh_token.clone());
        let attempts = max_attempts.max(1);
        let mut last_error = None;

        for attempt in 0..attempts {
            if attempt > 0 {
                let delay = Duration::from_secs(attempt as u64);
                clock.sleep(delay).await.map_err(RefreshError::Transport)?;
            }

            match transport.execute(&request, REFRESH_TIMEOUT).await {
                Ok(response) => {
                    match self.accept_response(response, fingerprint, refresh_token, clock.now()) {
                        Ok(mut token) => {
                            if let Some(profile) =
                                fetch_oauth_profile_after_refresh(transport, token.access_token())
                                    .await
                            {
                                let user = profile.user_info();
                                token.set_email(user.email());
                                token.set_identity_if_present(
                                    user.account_uuid(),
                                    user.organization_uuid(),
                                    user.organization_name(),
                                );
                            }
                            return Ok(token);
                        }
                        Err(error) if error.retryable() => last_error = Some(error),
                        Err(error) => return Err(error),
                    }
                }
                Err(kind) => {
                    let error = RefreshError::Transport(kind);
                    if !error.retryable() {
                        return Err(error);
                    }
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or(RefreshError::InvalidSingleFlightResult))
    }

    fn accept_response(
        &self,
        response: RefreshHttpResponse,
        fingerprint: RefreshFingerprint,
        current_refresh_token: &SecretString,
        now: SystemTime,
    ) -> RefreshResult {
        if response.status == 429 {
            let delay = parse_retry_after(
                response.retry_after.as_deref(),
                response.retry_after_ms.as_deref(),
                now,
            );
            let blocked_until = now.checked_add(delay).unwrap_or(now);
            lock_recover(&self.blocked_until).insert(fingerprint, blocked_until);
            return Err(RefreshError::RateLimited { blocked_until });
        }
        if response.status != 200 {
            return Err(RefreshError::Http {
                status: response.status,
                retryable: response.status >= 500,
            });
        }

        let token = parse_token_response(&response.body, current_refresh_token, now)?;
        lock_recover(&self.blocked_until).remove(&fingerprint);
        Ok(token)
    }

    fn check_cooldown(
        &self,
        fingerprint: RefreshFingerprint,
        now: SystemTime,
    ) -> Result<(), RefreshError> {
        let mut blocked = lock_recover(&self.blocked_until);
        if let Some(until) = blocked.get(&fingerprint).copied() {
            if until > now {
                return Err(RefreshError::RateLimited {
                    blocked_until: until,
                });
            }
            blocked.remove(&fingerprint);
        }
        Ok(())
    }
}

async fn wait_for_flight(receiver: &mut FlightReceiver) -> RefreshResult {
    loop {
        if let Some(result) = receiver.borrow().clone() {
            return result;
        }
        receiver
            .changed()
            .await
            .map_err(|_| RefreshError::InvalidSingleFlightResult)?;
    }
}

fn parse_oauth_profile(response: OAuthInspectHttpResponse) -> Option<OAuthProfile> {
    if !(200..300).contains(&response.status) {
        return None;
    }
    let profile: OAuthProfile = serde_json::from_slice(&response.body).ok()?;
    (!profile.account.uuid.trim().is_empty()).then_some(profile)
}

async fn inspect_oauth_account_after_exchange<T: ClaudeCodeExchangeTransport + ?Sized>(
    transport: &T,
    access_token: &SecretString,
) -> Option<OAuthProfile> {
    let profile_request = OAuthInspectRequest::new(OAuthInspectKind::Profile, access_token.clone());
    let profile = transport
        .inspect(&profile_request, EXCHANGE_TIMEOUT)
        .await
        .ok()
        .and_then(parse_oauth_profile);

    let roles_request = OAuthInspectRequest::new(OAuthInspectKind::Roles, access_token.clone());
    if let Ok(response) = transport.inspect(&roles_request, EXCHANGE_TIMEOUT).await {
        if (200..300).contains(&response.status) {
            let _: Option<serde_json::Value> = serde_json::from_slice(&response.body).ok();
        }
    }
    profile
}

async fn fetch_oauth_profile_after_refresh<T: ClaudeRefreshTransport + ?Sized>(
    transport: &T,
    access_token: &SecretString,
) -> Option<OAuthProfile> {
    let request = OAuthInspectRequest::new(OAuthInspectKind::Profile, access_token.clone());
    transport
        .inspect(&request, REFRESH_TIMEOUT)
        .await
        .ok()
        .and_then(parse_oauth_profile)
}

#[derive(Default, Deserialize)]
struct WireOrganization {
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    name: String,
}

#[derive(Default, Deserialize)]
struct WireAccount {
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    email_address: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WireScopes {
    Text(String),
    List(Vec<String>),
}

#[derive(Deserialize)]
struct WireTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    organization: WireOrganization,
    #[serde(default)]
    account: WireAccount,
    #[serde(default)]
    api_key: String,
    #[serde(default, alias = "scopes")]
    scope: Option<WireScopes>,
}

fn parse_wire_token_response(body: &[u8]) -> Result<WireTokenResponse, AuthFlowError> {
    serde_json::from_slice(body).map_err(|_| AuthFlowError::InvalidResponse)
}

fn expires_at(issued_at: SystemTime, expires_in: i64) -> Result<SystemTime, TokenError> {
    let value = if expires_in >= 0 {
        issued_at.checked_add(Duration::from_secs(expires_in as u64))
    } else {
        issued_at.checked_sub(Duration::from_secs(expires_in.unsigned_abs()))
    };
    value.ok_or(TokenError::ExpiryOverflow)
}

fn normalized_scopes(scope: Option<WireScopes>) -> Vec<String> {
    match scope {
        Some(WireScopes::Text(scopes)) => scopes
            .split_whitespace()
            .filter(|scope| !scope.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        Some(WireScopes::List(scopes)) => scopes
            .into_iter()
            .filter(|scope| !scope.trim().is_empty())
            .collect(),
        None => Vec::new(),
    }
}

fn parse_exchange_response(
    body: &[u8],
    issued_at: SystemTime,
) -> Result<ClaudeAuthBundle, AuthFlowError> {
    let wire = parse_wire_token_response(body)?;
    let access_token = SecretString::new(wire.access_token)?;
    let refresh_token = SecretString::new(wire.refresh_token)?;
    let api_key = if wire.api_key.is_empty() {
        None
    } else {
        Some(SecretString::new(wire.api_key)?)
    };
    let expiry = expires_at(issued_at, wire.expires_in)?;
    let user_info = ClaudeUserInfo::new(
        wire.account.uuid,
        wire.account.email_address.clone(),
        wire.organization.uuid,
        wire.organization.name,
    );
    let scopes = normalized_scopes(wire.scope);
    let token_data = ClaudeTokenData::new(
        access_token,
        refresh_token,
        wire.account.email_address,
        expiry,
    );

    Ok(
        ClaudeAuthBundle::new(api_key, token_data, issued_at).with_exchange_metadata(
            wire.token_type,
            scopes,
            user_info,
        ),
    )
}

fn parse_token_response(
    body: &[u8],
    current_refresh_token: &SecretString,
    issued_at: SystemTime,
) -> RefreshResult {
    let wire: WireTokenResponse =
        serde_json::from_slice(body).map_err(|_| RefreshError::InvalidResponse)?;
    let access_token = SecretString::new(wire.access_token)?;
    let refresh_token = if wire.refresh_token.is_empty() {
        current_refresh_token.clone()
    } else {
        SecretString::new(wire.refresh_token)?
    };
    let expires_at = expires_at(issued_at, wire.expires_in)?;

    Ok(ClaudeTokenData::new(
        access_token,
        refresh_token,
        wire.account.email_address,
        expires_at,
    ))
}

fn parse_retry_after(
    retry_after: Option<&str>,
    retry_after_ms: Option<&str>,
    now: SystemTime,
) -> Duration {
    if let Some(raw) = retry_after.map(str::trim).filter(|raw| !raw.is_empty()) {
        if let Ok(seconds) = raw.parse::<u64>() {
            return clamp_backoff(Duration::from_secs(seconds));
        }
        if let Ok(when) = httpdate::parse_http_date(raw) {
            return clamp_backoff(when.duration_since(now).unwrap_or(Duration::ZERO));
        }
    }
    if let Some(raw) = retry_after_ms.map(str::trim).filter(|raw| !raw.is_empty()) {
        if let Ok(milliseconds) = raw.parse::<u64>() {
            return clamp_backoff(Duration::from_millis(milliseconds));
        }
    }
    REFRESH_MIN_BACKOFF
}

fn clamp_backoff(duration: Duration) -> Duration {
    duration.clamp(REFRESH_MIN_BACKOFF, REFRESH_MAX_BACKOFF)
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestClock {
        now: Mutex<SystemTime>,
        sleeps: Mutex<Vec<Duration>>,
    }

    impl TestClock {
        fn at(now: SystemTime) -> Self {
            Self {
                now: Mutex::new(now),
                sleeps: Mutex::new(Vec::new()),
            }
        }
    }

    impl RefreshClock for TestClock {
        fn now(&self) -> SystemTime {
            *lock_recover(&self.now)
        }

        fn sleep(
            &self,
            duration: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<(), RefreshTransportFailure>> + Send + '_>>
        {
            Box::pin(async move {
                lock_recover(&self.sleeps).push(duration);
                Ok(())
            })
        }
    }

    fn success(refresh_token: &str) -> RefreshHttpResponse {
        RefreshHttpResponse::new(
            200,
            None,
            None,
            format!(
                r#"{{"access_token":"new-access","refresh_token":"{refresh_token}","expires_in":3600,"account":{{"email_address":"shared@example.com"}}}}"#
            )
            .into_bytes(),
        )
    }

    struct SequenceTransport {
        responses: Mutex<VecDeque<Result<RefreshHttpResponse, RefreshTransportFailure>>>,
        calls: AtomicUsize,
        timeouts: Mutex<Vec<Duration>>,
    }

    impl SequenceTransport {
        fn new(responses: Vec<Result<RefreshHttpResponse, RefreshTransportFailure>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                calls: AtomicUsize::new(0),
                timeouts: Mutex::new(Vec::new()),
            }
        }
    }

    impl ClaudeRefreshTransport for SequenceTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a RefreshRequest,
            timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<RefreshHttpResponse, RefreshTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                lock_recover(&self.timeouts).push(timeout);
                lock_recover(&self.responses)
                    .pop_front()
                    .expect("test response")
            })
        }
    }

    struct ExchangeTransport {
        response: Mutex<Option<Result<ExchangeHttpResponse, RefreshTransportFailure>>>,
        request_bodies: Mutex<Vec<Vec<u8>>>,
        timeouts: Mutex<Vec<Duration>>,
    }

    impl ExchangeTransport {
        fn responding(response: ExchangeHttpResponse) -> Self {
            Self {
                response: Mutex::new(Some(Ok(response))),
                request_bodies: Mutex::new(Vec::new()),
                timeouts: Mutex::new(Vec::new()),
            }
        }
    }

    impl ClaudeCodeExchangeTransport for ExchangeTransport {
        fn exchange<'a>(
            &'a self,
            request: &'a ExchangeRequest,
            timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<ExchangeHttpResponse, RefreshTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                let body = request
                    .json_body()
                    .map_err(|_| RefreshTransportFailure::Protocol)?;
                lock_recover(&self.request_bodies).push(body.as_slice().to_vec());
                lock_recover(&self.timeouts).push(timeout);
                lock_recover(&self.response)
                    .take()
                    .expect("test exchange response")
            })
        }
    }

    fn pkce() -> PkceCodes {
        PkceCodes {
            code_verifier: "verifier-do-not-log".to_owned(),
            code_challenge: "fixed-challenge".to_owned(),
        }
    }

    #[test]
    fn authorization_url_matches_upstream_contract() {
        let auth = ClaudeAuth::new(ExchangeTransport::responding(ExchangeHttpResponse::new(
            500,
            Vec::new(),
        )));
        let state = SecretString::new("fixed-state").unwrap();
        let (url, returned_state) = auth.generate_auth_url(&state, &pkce()).unwrap();

        assert_eq!(returned_state.expose_secret(), "fixed-state");
        assert_eq!(
            url,
            concat!(
                "https://claude.ai/oauth/authorize?",
                "client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&",
                "code=true&code_challenge=fixed-challenge&",
                "code_challenge_method=S256&",
                "redirect_uri=http%3A%2F%2Flocalhost%3A54545%2Fcallback&",
                "response_type=code&",
                "scope=user%3Aprofile+user%3Ainference+user%3Asessions%3Aclaude_code+",
                "user%3Amcp_servers+user%3Afile_upload&state=fixed-state"
            )
        );
    }

    #[test]
    fn exchange_request_uses_fragment_state_and_redacts_pkce_material() {
        let request = ExchangeRequest::new(
            &SecretString::new("actual-code#fragment-state").unwrap(),
            &SecretString::new("original-state").unwrap(),
            &pkce(),
        )
        .unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&request.json_body().unwrap()).unwrap();

        assert_eq!(body["code"], "actual-code");
        assert_eq!(body["state"], "fragment-state");
        assert_eq!(body["grant_type"], "authorization_code");
        assert_eq!(body["client_id"], CLIENT_ID);
        assert_eq!(body["redirect_uri"], REDIRECT_URI);
        assert_eq!(body["code_verifier"], "verifier-do-not-log");
        let debug = format!("{request:?}");
        assert!(!debug.contains("actual-code"));
        assert!(!debug.contains("fragment-state"));
        assert!(!debug.contains("verifier-do-not-log"));
    }

    #[tokio::test]
    async fn exchange_maps_tokens_userinfo_scopes_api_key_and_expiry() {
        let issued_at = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let transport = ExchangeTransport::responding(ExchangeHttpResponse::new(
            200,
            br#"{
                "access_token":"exchange-access-do-not-log",
                "refresh_token":"exchange-refresh-do-not-log",
                "token_type":"Bearer",
                "expires_in":3600,
                "organization":{"uuid":"org-uuid","name":"Example Org"},
                "account":{"uuid":"account-uuid","email_address":"operator@example.com"},
                "api_key":"api-key-do-not-log",
                "scope":"user:profile user:inference"
            }"#
            .to_vec(),
        ));
        let auth = ClaudeAuth::with_clock(transport, TestClock::at(issued_at));
        let bundle = auth
            .exchange_code_for_tokens(
                &SecretString::new("exchange-code").unwrap(),
                &SecretString::new("exchange-state").unwrap(),
                &pkce(),
            )
            .await
            .unwrap();

        assert_eq!(
            bundle.token_data().access_token().expose_secret(),
            "exchange-access-do-not-log"
        );
        assert_eq!(
            bundle.token_data().refresh_token().expose_secret(),
            "exchange-refresh-do-not-log"
        );
        assert_eq!(bundle.token_data().email(), "operator@example.com");
        assert_eq!(
            bundle.token_data().expires_at(),
            issued_at + Duration::from_secs(3600)
        );
        assert_eq!(bundle.last_refresh(), issued_at);
        assert_eq!(bundle.token_type(), "Bearer");
        assert_eq!(bundle.scopes(), &["user:profile", "user:inference"]);
        assert_eq!(bundle.user_info().account_uuid(), "account-uuid");
        assert_eq!(bundle.user_info().email(), "operator@example.com");
        assert_eq!(bundle.user_info().organization_uuid(), "org-uuid");
        assert_eq!(bundle.user_info().organization_name(), "Example Org");
        assert_eq!(
            bundle.api_key().unwrap().expose_secret(),
            "api-key-do-not-log"
        );
        assert_eq!(
            *lock_recover(&auth.transport().timeouts),
            vec![EXCHANGE_TIMEOUT]
        );

        let mut storage = auth.create_token_storage(&bundle);
        assert_eq!(storage.storage_type(), "claude");
        assert_eq!(storage.email(), "operator@example.com");
        assert_eq!(
            storage.credentials().access_token().expose_secret(),
            "exchange-access-do-not-log"
        );

        let refreshed = ClaudeTokenData::new(
            SecretString::new("updated-access-do-not-log").unwrap(),
            SecretString::new("updated-refresh-do-not-log").unwrap(),
            "updated@example.com",
            issued_at + Duration::from_secs(7200),
        );
        auth.update_token_storage(&mut storage, &refreshed);
        assert_eq!(
            storage.credentials().access_token().expose_secret(),
            "updated-access-do-not-log"
        );
        assert_eq!(
            storage.credentials().refresh_token().expose_secret(),
            "updated-refresh-do-not-log"
        );
        assert_eq!(storage.email(), "updated@example.com");
        let debug = format!("{bundle:?} {storage:?}");
        assert!(!debug.contains("exchange-access-do-not-log"));
        assert!(!debug.contains("exchange-refresh-do-not-log"));
        assert!(!debug.contains("api-key-do-not-log"));
        assert!(!debug.contains("updated-access-do-not-log"));
        assert!(!debug.contains("updated-refresh-do-not-log"));
    }

    #[tokio::test]
    async fn exchange_accepts_missing_optional_api_key_and_redacts_http_body() {
        let issued_at = SystemTime::UNIX_EPOCH + Duration::from_secs(20_000);
        let successful = ClaudeAuth::with_clock(
            ExchangeTransport::responding(ExchangeHttpResponse::new(
                200,
                br#"{"access_token":"access","refresh_token":"refresh","expires_in":0}"#.to_vec(),
            )),
            TestClock::at(issued_at),
        );
        let bundle = successful
            .exchange_code_for_tokens(
                &SecretString::new("code").unwrap(),
                &SecretString::new("state").unwrap(),
                &pkce(),
            )
            .await
            .unwrap();
        assert!(bundle.api_key().is_none());
        assert_eq!(bundle.token_data().expires_at(), issued_at);

        let rejected = ClaudeAuth::with_clock(
            ExchangeTransport::responding(ExchangeHttpResponse::new(
                401,
                br#"{"error":"bad","token":"response-secret-do-not-log"}"#.to_vec(),
            )),
            TestClock::at(issued_at),
        );
        let error = rejected
            .exchange_code_for_tokens(
                &SecretString::new("code-secret-do-not-log").unwrap(),
                &SecretString::new("state-secret-do-not-log").unwrap(),
                &pkce(),
            )
            .await
            .unwrap_err();
        assert_eq!(error, AuthFlowError::Http { status: 401 });
        let rendered = format!("{error:?} {error}");
        assert!(rendered.contains("401"));
        assert!(!rendered.contains("response-secret-do-not-log"));
        assert!(!rendered.contains("code-secret-do-not-log"));
        assert!(!rendered.contains("state-secret-do-not-log"));
    }

    #[test]
    fn request_contract_matches_upstream_without_debug_leak() {
        let request = RefreshRequest::new(SecretString::new("refresh-do-not-log").unwrap());
        let body: serde_json::Value =
            serde_json::from_slice(&request.json_body().unwrap()).unwrap();
        assert_eq!(body["client_id"], CLIENT_ID);
        assert_eq!(body["grant_type"], "refresh_token");
        assert_eq!(body["refresh_token"], "refresh-do-not-log");
        assert!(!format!("{request:?}").contains("refresh-do-not-log"));
    }

    #[test]
    fn response_parser_rotates_tokens_and_applies_expiry() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let current = SecretString::new("old-refresh").unwrap();
        let token = parse_token_response(&success("new-refresh").body, &current, now).unwrap();
        assert_eq!(token.access_token().expose_secret(), "new-access");
        assert_eq!(token.refresh_token().expose_secret(), "new-refresh");
        assert_eq!(token.email(), "shared@example.com");
        assert_eq!(token.expires_at(), now + Duration::from_secs(3600));

        let without_rotation = RefreshHttpResponse::new(
            200,
            None,
            None,
            br#"{"access_token":"next-access","expires_in":60}"#.to_vec(),
        );
        let token = parse_token_response(&without_rotation.body, &current, now).unwrap();
        assert_eq!(token.refresh_token().expose_secret(), "old-refresh");
    }

    #[tokio::test]
    async fn retryable_failures_back_off_and_keep_thirty_second_timeout() {
        let transport = SequenceTransport::new(vec![
            Ok(RefreshHttpResponse::new(
                503,
                None,
                None,
                b"secret body".to_vec(),
            )),
            Ok(success("rotated-refresh")),
        ]);
        let clock = TestClock::at(SystemTime::UNIX_EPOCH + Duration::from_secs(500));
        let coordinator = ClaudeRefreshCoordinator::default();
        let token = coordinator
            .refresh(
                &transport,
                &clock,
                SecretString::new("original-refresh").unwrap(),
                3,
            )
            .await
            .unwrap();

        assert_eq!(token.access_token().expose_secret(), "new-access");
        assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
        assert_eq!(*lock_recover(&clock.sleeps), vec![Duration::from_secs(1)]);
        assert_eq!(
            *lock_recover(&transport.timeouts),
            vec![REFRESH_TIMEOUT, REFRESH_TIMEOUT]
        );
    }

    #[tokio::test]
    async fn ambiguous_transport_failures_never_replay_single_use_refresh_tokens() {
        for failure in [
            RefreshTransportFailure::Timeout,
            RefreshTransportFailure::Connect,
            RefreshTransportFailure::Protocol,
            RefreshTransportFailure::Cancelled,
        ] {
            let transport = SequenceTransport::new(vec![
                Err(failure.clone()),
                Ok(success("unused-rotated-refresh")),
            ]);
            let clock = TestClock::at(SystemTime::UNIX_EPOCH + Duration::from_secs(500));
            let coordinator = ClaudeRefreshCoordinator::default();
            let result = coordinator
                .refresh(
                    &transport,
                    &clock,
                    SecretString::new("original-refresh").unwrap(),
                    3,
                )
                .await;
            assert!(matches!(result, Err(RefreshError::Transport(kind)) if kind == failure));
            assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
            assert!(lock_recover(&clock.sleeps).is_empty());
            assert_eq!(*lock_recover(&transport.timeouts), vec![REFRESH_TIMEOUT]);
        }
    }

    #[test]
    fn retry_after_values_are_clamped_like_upstream() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        assert_eq!(parse_retry_after(Some("1"), None, now), REFRESH_MIN_BACKOFF);
        assert_eq!(
            parse_retry_after(Some("9999"), None, now),
            REFRESH_MAX_BACKOFF
        );
        assert_eq!(
            parse_retry_after(None, Some("9000"), now),
            Duration::from_secs(9)
        );
        let date = httpdate::fmt_http_date(now + Duration::from_secs(30));
        assert_eq!(
            parse_retry_after(Some(&date), None, now),
            Duration::from_secs(30)
        );
    }
}
