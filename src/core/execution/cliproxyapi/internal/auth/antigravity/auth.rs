// ref: internal/auth/antigravity/auth.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::watch;
use zeroize::Zeroizing;

use crate::sdk::auth::LoginCancellation;

use super::constants::{
    ANTIGRAVITY_GOOG_API_CLIENT_USER_AGENT, ANTIGRAVITY_NODE_API_CLIENT_USER_AGENT,
    ANTIGRAVITY_USER_AGENT, API_ENDPOINT, API_VERSION, AUTH_ENDPOINT, CALLBACK_PORT, CLIENT_ID,
    CLIENT_SECRET, DAILY_API_ENDPOINT, REFRESH_SKEW, SCOPES, TOKEN_ENDPOINT, USER_INFO_ENDPOINT,
};

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const ONBOARD_ATTEMPTS: usize = 5;
const ONBOARD_POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntigravityHttpMethod {
    Get,
    Post,
}

pub struct AntigravityHttpRequest {
    pub method: AntigravityHttpMethod,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for AntigravityHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityHttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field("body", &"[REDACTED]")
            .finish()
    }
}

pub struct AntigravityHttpResponse {
    pub status: u16,
    pub body: Zeroizing<Vec<u8>>,
}

impl AntigravityHttpResponse {
    #[must_use]
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            body: Zeroizing::new(body),
        }
    }
}

impl fmt::Debug for AntigravityHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityHttpResponse")
            .field("status", &self.status)
            .field("body", &"[REDACTED]")
            .finish()
    }
}

pub type AntigravityHttpFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AntigravityHttpResponse, AntigravityHttpTransportFailure>>
            + Send
            + 'a,
    >,
>;

pub trait AntigravityFlowTransport: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: &'a AntigravityHttpRequest,
        timeout: Duration,
        cancellation: &'a LoginCancellation,
    ) -> AntigravityHttpFuture<'a>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntigravityHttpTransportFailure {
    Timeout,
    Connect,
    Protocol,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AntigravityAuthErrorKind {
    Cancelled,
    Transport,
    InvalidArguments,
    TokenExchange,
    UserInfo,
    LoadCodeAssist,
    OnboardUser,
    Decode,
    ResponseTooLarge,
    MissingEmail,
    MissingProjectId,
    InvalidToken,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AntigravityAuthError {
    pub kind: AntigravityAuthErrorKind,
    pub status: Option<u16>,
    pub transport: Option<AntigravityHttpTransportFailure>,
}

impl AntigravityAuthError {
    fn new(kind: AntigravityAuthErrorKind) -> Self {
        Self {
            kind,
            status: None,
            transport: None,
        }
    }

    fn status(kind: AntigravityAuthErrorKind, status: u16) -> Self {
        Self {
            kind,
            status: Some(status),
            transport: None,
        }
    }

    fn transport(failure: AntigravityHttpTransportFailure) -> Self {
        Self {
            kind: if failure == AntigravityHttpTransportFailure::Cancelled {
                AntigravityAuthErrorKind::Cancelled
            } else {
                AntigravityAuthErrorKind::Transport
            },
            status: None,
            transport: Some(failure),
        }
    }
}

impl fmt::Display for AntigravityAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Antigravity authentication failed: {:?}",
            self.kind
        )
    }
}

impl std::error::Error for AntigravityAuthError {}

#[derive(Clone, PartialEq, Eq)]
pub struct AntigravityTokenResponse {
    access_token: SecretString,
    refresh_token: Option<SecretString>,
    pub expires_in: i64,
    pub token_type: String,
}

impl AntigravityTokenResponse {
    pub fn access_token(&self) -> &SecretString {
        &self.access_token
    }

    pub fn refresh_token(&self) -> Option<&SecretString> {
        self.refresh_token.as_ref()
    }
}

impl fmt::Debug for AntigravityTokenResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityTokenResponse")
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_in", &self.expires_in)
            .field("token_type", &self.token_type)
            .finish()
    }
}

#[derive(Deserialize)]
struct TokenWireResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    token_type: String,
}

#[derive(Deserialize)]
struct UserInfoWireResponse {
    #[serde(default)]
    email: String,
}

pub struct AntigravityAuth {
    transport: Arc<dyn AntigravityFlowTransport>,
}

impl fmt::Debug for AntigravityAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityAuth")
            .field("transport", &"[INJECTED]")
            .finish()
    }
}

impl AntigravityAuth {
    #[must_use]
    pub fn new(transport: Arc<dyn AntigravityFlowTransport>) -> Self {
        Self { transport }
    }

    pub async fn exchange_code_for_tokens(
        &self,
        cancellation: &LoginCancellation,
        code: &str,
        redirect_uri: &str,
    ) -> Result<AntigravityTokenResponse, AntigravityAuthError> {
        if code.trim().is_empty() || redirect_uri.trim().is_empty() {
            return Err(AntigravityAuthError::new(
                AntigravityAuthErrorKind::InvalidArguments,
            ));
        }
        let body = form(&[
            ("code", code),
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ]);
        let response = self
            .execute(
                AntigravityHttpRequest {
                    method: AntigravityHttpMethod::Post,
                    url: TOKEN_ENDPOINT.to_owned(),
                    headers: BTreeMap::from([(
                        "Content-Type".to_owned(),
                        "application/x-www-form-urlencoded".to_owned(),
                    )]),
                    body: Zeroizing::new(body.into_bytes()),
                },
                cancellation,
            )
            .await?;
        ensure_success(&response, AntigravityAuthErrorKind::TokenExchange)?;
        let wire: TokenWireResponse = decode(&response)?;
        let access_token = SecretString::new(wire.access_token)
            .map_err(|_| AntigravityAuthError::new(AntigravityAuthErrorKind::InvalidToken))?;
        let refresh_token =
            if wire.refresh_token.trim().is_empty() {
                None
            } else {
                Some(SecretString::new(wire.refresh_token).map_err(|_| {
                    AntigravityAuthError::new(AntigravityAuthErrorKind::InvalidToken)
                })?)
            };
        Ok(AntigravityTokenResponse {
            access_token,
            refresh_token,
            expires_in: wire.expires_in,
            token_type: wire.token_type,
        })
    }

    pub async fn fetch_user_info(
        &self,
        cancellation: &LoginCancellation,
        access_token: &SecretString,
    ) -> Result<String, AntigravityAuthError> {
        let response = self
            .execute(
                json_request(
                    AntigravityHttpMethod::Get,
                    USER_INFO_ENDPOINT,
                    access_token,
                    ANTIGRAVITY_USER_AGENT,
                    None,
                ),
                cancellation,
            )
            .await?;
        ensure_success(&response, AntigravityAuthErrorKind::UserInfo)?;
        let info: UserInfoWireResponse = decode(&response)?;
        let email = info.email.trim();
        if email.is_empty() {
            return Err(AntigravityAuthError::new(
                AntigravityAuthErrorKind::MissingEmail,
            ));
        }
        Ok(email.to_owned())
    }

    pub async fn fetch_project_id(
        &self,
        cancellation: &LoginCancellation,
        access_token: &SecretString,
    ) -> Result<String, AntigravityAuthError> {
        let body = serde_json::to_vec(&LoadCodeAssistRequest {
            metadata: LoadCodeAssistMetadata {
                ide_type: "ANTIGRAVITY",
            },
        })
        .map_err(|_| AntigravityAuthError::new(AntigravityAuthErrorKind::Decode))?;
        let endpoint = format!("{API_ENDPOINT}/{API_VERSION}:loadCodeAssist");
        let response = self
            .execute(
                json_request(
                    AntigravityHttpMethod::Post,
                    &endpoint,
                    access_token,
                    ANTIGRAVITY_USER_AGENT,
                    Some(body),
                ),
                cancellation,
            )
            .await?;
        ensure_success(&response, AntigravityAuthErrorKind::LoadCodeAssist)?;
        let value: serde_json::Value = decode(&response)?;
        if let Some(project_id) = extract_cloudaicompanion_project(&value) {
            return Ok(project_id);
        }
        self.onboard_user(
            cancellation,
            access_token,
            &default_antigravity_tier_id(&value),
        )
        .await
    }

    pub async fn onboard_user(
        &self,
        cancellation: &LoginCancellation,
        access_token: &SecretString,
        tier_id: &str,
    ) -> Result<String, AntigravityAuthError> {
        let tier_id = if tier_id.trim().is_empty() {
            "free-tier"
        } else {
            tier_id.trim()
        };
        let body = serde_json::to_vec(&OnboardRequest {
            tier_id,
            metadata: ControlPlaneMetadata {
                ide_type: "ANTIGRAVITY",
                ide_version: "2.2.1",
                ide_name: "antigravity",
            },
        })
        .map_err(|_| AntigravityAuthError::new(AntigravityAuthErrorKind::Decode))?;
        let endpoint = format!("{DAILY_API_ENDPOINT}/{API_VERSION}:onboardUser");
        for attempt in 0..ONBOARD_ATTEMPTS {
            if cancellation.is_cancelled() {
                return Err(AntigravityAuthError::new(
                    AntigravityAuthErrorKind::Cancelled,
                ));
            }
            let mut request = json_request(
                AntigravityHttpMethod::Post,
                &endpoint,
                access_token,
                &format!("{ANTIGRAVITY_USER_AGENT} {ANTIGRAVITY_NODE_API_CLIENT_USER_AGENT}"),
                Some(body.clone()),
            );
            request.headers.insert(
                "X-Goog-Api-Client".to_owned(),
                ANTIGRAVITY_GOOG_API_CLIENT_USER_AGENT.to_owned(),
            );
            let response = self.execute(request, cancellation).await?;
            ensure_success(&response, AntigravityAuthErrorKind::OnboardUser)?;
            let value: serde_json::Value = decode(&response)?;
            if value.get("done").and_then(serde_json::Value::as_bool) == Some(true) {
                return value
                    .get("response")
                    .and_then(extract_cloudaicompanion_project)
                    .ok_or_else(|| {
                        AntigravityAuthError::new(AntigravityAuthErrorKind::MissingProjectId)
                    });
            }
            if attempt + 1 < ONBOARD_ATTEMPTS {
                tokio::select! {
                    () = tokio::time::sleep(ONBOARD_POLL_INTERVAL) => {}
                    () = cancellation.cancelled() => {
                        return Err(AntigravityAuthError::new(AntigravityAuthErrorKind::Cancelled));
                    }
                }
            }
        }
        Err(AntigravityAuthError::new(
            AntigravityAuthErrorKind::MissingProjectId,
        ))
    }

    async fn execute(
        &self,
        request: AntigravityHttpRequest,
        cancellation: &LoginCancellation,
    ) -> Result<AntigravityHttpResponse, AntigravityAuthError> {
        let response = self
            .transport
            .execute(&request, HTTP_TIMEOUT, cancellation)
            .await
            .map_err(AntigravityAuthError::transport)?;
        if response.body.len() > MAX_RESPONSE_BYTES {
            return Err(AntigravityAuthError::new(
                AntigravityAuthErrorKind::ResponseTooLarge,
            ));
        }
        Ok(response)
    }
}

#[derive(Serialize)]
struct LoadCodeAssistRequest<'a> {
    metadata: LoadCodeAssistMetadata<'a>,
}

#[derive(Serialize)]
struct LoadCodeAssistMetadata<'a> {
    #[serde(rename = "ideType")]
    ide_type: &'a str,
}

#[derive(Serialize)]
struct OnboardRequest<'a> {
    tier_id: &'a str,
    metadata: ControlPlaneMetadata<'a>,
}

#[derive(Serialize)]
struct ControlPlaneMetadata<'a> {
    ide_type: &'a str,
    ide_version: &'a str,
    ide_name: &'a str,
}

fn json_request(
    method: AntigravityHttpMethod,
    url: &str,
    access_token: &SecretString,
    user_agent: &str,
    body: Option<Vec<u8>>,
) -> AntigravityHttpRequest {
    let mut headers = BTreeMap::from([
        ("Accept".to_owned(), "*/*".to_owned()),
        (
            "Authorization".to_owned(),
            format!("Bearer {}", access_token.expose_secret()),
        ),
        ("User-Agent".to_owned(), user_agent.to_owned()),
    ]);
    if body.is_some() {
        headers.insert("Content-Type".to_owned(), "application/json".to_owned());
    }
    AntigravityHttpRequest {
        method,
        url: url.to_owned(),
        headers,
        body: Zeroizing::new(body.unwrap_or_default()),
    }
}

fn extract_cloudaicompanion_project(value: &serde_json::Value) -> Option<String> {
    ["cloudaicompanionProject", "projectId", "project"]
        .into_iter()
        .find_map(|key| match value.get(key) {
            Some(serde_json::Value::String(project)) => non_empty(project),
            Some(serde_json::Value::Object(project)) => project
                .get("id")
                .and_then(serde_json::Value::as_str)
                .and_then(non_empty),
            _ => None,
        })
}

fn default_antigravity_tier_id(value: &serde_json::Value) -> String {
    value
        .get("allowedTiers")
        .and_then(serde_json::Value::as_array)
        .and_then(|tiers| {
            tiers.iter().find_map(|tier| {
                (tier.get("isDefault").and_then(serde_json::Value::as_bool) == Some(true))
                    .then(|| tier.get("id").and_then(serde_json::Value::as_str))
                    .flatten()
                    .and_then(non_empty)
            })
        })
        .or_else(|| {
            value
                .get("currentTier")
                .and_then(|tier| tier.get("id"))
                .and_then(serde_json::Value::as_str)
                .and_then(non_empty)
        })
        .unwrap_or_else(|| "free-tier".to_owned())
}

fn non_empty(raw: &str) -> Option<String> {
    (!raw.trim().is_empty()).then(|| raw.trim().to_owned())
}

fn form(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .fold(
            url::form_urlencoded::Serializer::new(String::new()),
            |mut serializer, (key, value)| {
                serializer.append_pair(key, value);
                serializer
            },
        )
        .finish()
}

fn decode<'a, T: Deserialize<'a>>(
    response: &'a AntigravityHttpResponse,
) -> Result<T, AntigravityAuthError> {
    serde_json::from_slice(&response.body)
        .map_err(|_| AntigravityAuthError::new(AntigravityAuthErrorKind::Decode))
}

fn ensure_success(
    response: &AntigravityHttpResponse,
    kind: AntigravityAuthErrorKind,
) -> Result<(), AntigravityAuthError> {
    if (200..300).contains(&response.status) {
        Ok(())
    } else {
        Err(AntigravityAuthError::status(kind, response.status))
    }
}

pub fn build_auth_url(state: &str, redirect_uri: Option<&str>) -> String {
    let default_redirect = format!("http://localhost:{CALLBACK_PORT}/oauth-callback");
    let redirect_uri = redirect_uri
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&default_redirect);
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("access_type", "offline")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("prompt", "consent")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &SCOPES.join(" "))
        .append_pair("state", state)
        .finish();
    format!("{AUTH_ENDPOINT}?{query}")
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Result<Self, AntigravityTokenError> {
        let value = value.into();
        if value.is_empty() {
            return Err(AntigravityTokenError::EmptySecret);
        }
        Ok(Self(Zeroizing::new(value)))
    }

    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravitySecretKind {
    AccessToken,
    RefreshToken,
    State,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravitySecretHandle {
    scope: String,
    name: String,
    kind: AntigravitySecretKind,
}

impl AntigravitySecretHandle {
    pub fn new(
        scope: impl Into<String>,
        name: impl Into<String>,
        kind: AntigravitySecretKind,
    ) -> Result<Self, AntigravityTokenError> {
        let scope = scope.into();
        let name = name.into();
        if scope.trim().is_empty() {
            return Err(AntigravityTokenError::EmptyHandleField("scope"));
        }
        if name.trim().is_empty() {
            return Err(AntigravityTokenError::EmptyHandleField("name"));
        }
        Ok(Self { scope, name, kind })
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> AntigravitySecretKind {
        self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravityCredentialHandles {
    access_token: AntigravitySecretHandle,
    refresh_token: AntigravitySecretHandle,
    state: AntigravitySecretHandle,
}

impl AntigravityCredentialHandles {
    pub fn new(
        access_token: AntigravitySecretHandle,
        refresh_token: AntigravitySecretHandle,
        state: AntigravitySecretHandle,
    ) -> Result<Self, AntigravityTokenError> {
        if access_token.kind() != AntigravitySecretKind::AccessToken
            || refresh_token.kind() != AntigravitySecretKind::RefreshToken
            || state.kind() != AntigravitySecretKind::State
        {
            return Err(AntigravityTokenError::HandleKindMismatch);
        }
        Ok(Self {
            access_token,
            refresh_token,
            state,
        })
    }

    pub fn access_token(&self) -> &AntigravitySecretHandle {
        &self.access_token
    }

    pub fn refresh_token(&self) -> &AntigravitySecretHandle {
        &self.refresh_token
    }

    pub fn state(&self) -> &AntigravitySecretHandle {
        &self.state
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravityStoredCredentials {
    access_token: SecretString,
    refresh_token: SecretString,
    expires_at: SystemTime,
    project_id: String,
}

impl AntigravityStoredCredentials {
    pub fn new(
        access_token: SecretString,
        refresh_token: SecretString,
        expires_at: SystemTime,
        project_id: impl Into<String>,
    ) -> Result<Self, AntigravityTokenError> {
        let project_id = project_id.into();
        if project_id.trim().is_empty() || project_id.chars().any(char::is_control) {
            return Err(AntigravityTokenError::InvalidProjectId);
        }
        Ok(Self {
            access_token,
            refresh_token,
            expires_at,
            project_id: project_id.trim().to_owned(),
        })
    }

    pub fn access_token(&self) -> &SecretString {
        &self.access_token
    }

    pub fn refresh_token(&self) -> &SecretString {
        &self.refresh_token
    }

    pub fn expires_at(&self) -> SystemTime {
        self.expires_at
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn refresh_due(&self, now: SystemTime) -> bool {
        self.expires_at
            .duration_since(now)
            .map_or(true, |remaining| remaining <= REFRESH_SKEW)
    }
}

pub trait AntigravitySecretStore: Send + Sync {
    /// Async host boundary. Native stores override this to admit the job on
    /// their existing file owners; the portable default performs no Core IO.
    fn load_credentials_async(
        self: std::sync::Arc<Self>,
        handles: AntigravityCredentialHandles,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<AntigravityStoredCredentials, AntigravityTokenError>,
                > + Send,
        >,
    >
    where
        Self: 'static,
    {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || self.load_credentials(&handles))
                .await
                .map_err(|_| AntigravityTokenError::Read)?
        })
    }

    /// Async host boundary. Native stores override this to admit the job on
    /// their existing file owners; the portable default performs no Core IO.
    fn store_credentials_async(
        self: std::sync::Arc<Self>,
        handles: AntigravityCredentialHandles,
        credentials: AntigravityStoredCredentials,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), AntigravityTokenError>> + Send>,
    >
    where
        Self: 'static,
    {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || self.store_credentials(&handles, &credentials))
                .await
                .map_err(|_| AntigravityTokenError::Write)?
        })
    }

    fn load_credentials(
        &self,
        handles: &AntigravityCredentialHandles,
    ) -> Result<AntigravityStoredCredentials, AntigravityTokenError>;

    fn store_credentials(
        &self,
        handles: &AntigravityCredentialHandles,
        credentials: &AntigravityStoredCredentials,
    ) -> Result<(), AntigravityTokenError>;

    /// Async callers lend IO to the host's actual owner executor. Portable
    /// stores retain the blocking-pool default; native stores override it.
    fn load_async(self: std::sync::Arc<Self>, handles: AntigravityCredentialHandles)
        -> std::pin::Pin<Box<dyn std::future::Future<Output=Result<AntigravityStoredCredentials,AntigravityTokenError>> + Send>>
        where Self: 'static {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || self.load_credentials(&handles)).await
                .map_err(|_| AntigravityTokenError::Read)?
        })
    }
    fn store_async(self: std::sync::Arc<Self>, handles: AntigravityCredentialHandles, credentials: AntigravityStoredCredentials)
        -> std::pin::Pin<Box<dyn std::future::Future<Output=Result<(),AntigravityTokenError>> + Send>>
        where Self: 'static {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || self.store_credentials(&handles,&credentials)).await
                .map_err(|_| AntigravityTokenError::Write)?
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntigravityTokenError {
    EmptySecret,
    EmptyHandleField(&'static str),
    HandleKindMismatch,
    InvalidProjectId,
    Missing,
    Read,
    Write,
    ExpiryOverflow,
}

impl fmt::Display for AntigravityTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySecret => formatter.write_str("secret must not be empty"),
            Self::EmptyHandleField(field) => write!(formatter, "secret handle {field} is empty"),
            Self::HandleKindMismatch => formatter.write_str("secret handle kind does not match"),
            Self::InvalidProjectId => formatter.write_str("Antigravity project ID is invalid"),
            Self::Missing => formatter.write_str("Antigravity credential is missing"),
            Self::Read => formatter.write_str("Antigravity credential read failed"),
            Self::Write => formatter.write_str("Antigravity credential write failed"),
            Self::ExpiryOverflow => {
                formatter.write_str("Antigravity token expiry exceeds SystemTime")
            }
        }
    }
}

impl std::error::Error for AntigravityTokenError {}

#[derive(Clone)]
pub struct AntigravityRefreshRequest {
    refresh_token: SecretString,
}

impl AntigravityRefreshRequest {
    pub fn new(refresh_token: SecretString) -> Self {
        Self { refresh_token }
    }

    pub fn form_body(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(
            url::form_urlencoded::Serializer::new(String::new())
                .append_pair("client_id", CLIENT_ID)
                .append_pair("client_secret", CLIENT_SECRET)
                .append_pair("grant_type", "refresh_token")
                .append_pair("refresh_token", self.refresh_token.expose_secret())
                .finish()
                .into_bytes(),
        )
    }
}

impl fmt::Debug for AntigravityRefreshRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityRefreshRequest")
            .field("grant_type", &"refresh_token")
            .field("refresh_token", &"[REDACTED]")
            .finish()
    }
}

pub struct AntigravityRefreshHttpResponse {
    pub status: u16,
    body: Zeroizing<Vec<u8>>,
}

impl AntigravityRefreshHttpResponse {
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            body: Zeroizing::new(body),
        }
    }
}

impl fmt::Debug for AntigravityRefreshHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityRefreshHttpResponse")
            .field("status", &self.status)
            .field("body", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityRefreshTransportFailure {
    Timeout,
    Connect,
    Protocol,
    Cancelled,
}

pub trait AntigravityRefreshTransport: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: &'a AntigravityRefreshRequest,
        timeout: Duration,
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
    >;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntigravityRefreshError {
    Transport(AntigravityRefreshTransportFailure),
    Http { status: u16 },
    InvalidResponse,
    Token(AntigravityTokenError),
    InvalidSingleFlightResult,
}

impl fmt::Display for AntigravityRefreshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(kind) => {
                write!(formatter, "Antigravity refresh transport failed: {kind:?}")
            }
            Self::Http { status } => write!(
                formatter,
                "Antigravity token refresh failed with status {status}"
            ),
            Self::InvalidResponse => formatter.write_str("Antigravity token response is invalid"),
            Self::Token(error) => write!(formatter, "Antigravity token is invalid: {error}"),
            Self::InvalidSingleFlightResult => {
                formatter.write_str("Antigravity singleflight result is invalid")
            }
        }
    }
}

impl std::error::Error for AntigravityRefreshError {}

impl From<AntigravityTokenError> for AntigravityRefreshError {
    fn from(value: AntigravityTokenError) -> Self {
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

type RefreshResult = Result<AntigravityStoredCredentials, AntigravityRefreshError>;
type FlightReceiver = watch::Receiver<Option<RefreshResult>>;

#[derive(Default)]
pub struct AntigravityRefreshCoordinator {
    flights: Mutex<HashMap<RefreshFingerprint, FlightReceiver>>,
}

impl AntigravityRefreshCoordinator {
    pub async fn refresh<T: AntigravityRefreshTransport + ?Sized>(
        &self,
        transport: &T,
        current: AntigravityStoredCredentials,
        now: SystemTime,
    ) -> RefreshResult {
        let fingerprint = RefreshFingerprint::for_token(current.refresh_token());
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
        let result = refresh_once(transport, &current, now).await;
        if sender.send(Some(result.clone())).is_err() {
            lock_recover(&self.flights).remove(&fingerprint);
            return Err(AntigravityRefreshError::InvalidSingleFlightResult);
        }
        lock_recover(&self.flights).remove(&fingerprint);
        result
    }
}

#[derive(Deserialize)]
struct RefreshWireResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    expires_in: u64,
}

async fn refresh_once<T: AntigravityRefreshTransport + ?Sized>(
    transport: &T,
    current: &AntigravityStoredCredentials,
    now: SystemTime,
) -> RefreshResult {
    let request = AntigravityRefreshRequest::new(current.refresh_token().clone());
    let response = transport
        .execute(&request, Duration::from_secs(30))
        .await
        .map_err(AntigravityRefreshError::Transport)?;
    if !(200..300).contains(&response.status) {
        return Err(AntigravityRefreshError::Http {
            status: response.status,
        });
    }
    let wire: RefreshWireResponse = serde_json::from_slice(&response.body)
        .map_err(|_| AntigravityRefreshError::InvalidResponse)?;
    let access_token = SecretString::new(wire.access_token)?;
    let refresh_token = if wire.refresh_token.is_empty() {
        current.refresh_token().clone()
    } else {
        SecretString::new(wire.refresh_token)?
    };
    let expires_at = now
        .checked_add(Duration::from_secs(wire.expires_in))
        .ok_or(AntigravityTokenError::ExpiryOverflow)?;
    AntigravityStoredCredentials::new(
        access_token,
        refresh_token,
        expires_at,
        current.project_id(),
    )
    .map_err(Into::into)
}

async fn wait_for_flight(receiver: &mut FlightReceiver) -> RefreshResult {
    loop {
        if let Some(result) = receiver.borrow().clone() {
            return result;
        }
        if receiver.changed().await.is_err() {
            return Err(AntigravityRefreshError::InvalidSingleFlightResult);
        }
    }
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;

    struct MockTransport {
        calls: AtomicUsize,
        response: Vec<u8>,
    }

    impl AntigravityRefreshTransport for MockTransport {
        fn execute<'a>(
            &'a self,
            request: &'a AntigravityRefreshRequest,
            timeout: Duration,
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
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(timeout, Duration::from_secs(30));
                let form = String::from_utf8(request.form_body().to_vec()).unwrap();
                assert!(form.contains("grant_type=refresh_token"));
                assert!(form.contains("refresh_token=refresh-old"));
                tokio::time::sleep(Duration::from_millis(20)).await;
                Ok(AntigravityRefreshHttpResponse::new(
                    200,
                    self.response.clone(),
                ))
            })
        }
    }

    fn credentials(now: SystemTime) -> AntigravityStoredCredentials {
        AntigravityStoredCredentials::new(
            SecretString::new("access-old").unwrap(),
            SecretString::new("refresh-old").unwrap(),
            now + Duration::from_secs(60),
            "project-1",
        )
        .unwrap()
    }

    #[test]
    fn auth_url_matches_google_offline_consent_contract() {
        let url = url::Url::parse(&build_auth_url("state with space", None)).unwrap();
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(url.as_str().split('?').next().unwrap(), AUTH_ENDPOINT);
        assert_eq!(query["access_type"], "offline");
        assert_eq!(query["prompt"], "consent");
        assert_eq!(query["state"], "state with space");
        assert_eq!(
            query["redirect_uri"],
            "http://localhost:51121/oauth-callback"
        );
        assert_eq!(query["scope"], SCOPES.join(" "));
    }

    #[test]
    fn secrets_handles_and_refresh_request_are_redacted() {
        let access =
            AntigravitySecretHandle::new("scope", "access", AntigravitySecretKind::AccessToken)
                .unwrap();
        let refresh =
            AntigravitySecretHandle::new("scope", "refresh", AntigravitySecretKind::RefreshToken)
                .unwrap();
        let state =
            AntigravitySecretHandle::new("scope", "state", AntigravitySecretKind::State).unwrap();
        assert!(AntigravityCredentialHandles::new(refresh.clone(), access.clone(), state).is_err());
        let request = AntigravityRefreshRequest::new(SecretString::new("do-not-leak").unwrap());
        let rendered = format!("{request:?}");
        assert!(!rendered.contains("do-not-leak"));
        assert!(!rendered.contains(CLIENT_SECRET));
    }

    #[test]
    fn fifty_minute_skew_forces_early_refresh() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let creds = credentials(now);
        assert!(creds.refresh_due(now));
        let far = AntigravityStoredCredentials::new(
            SecretString::new("a").unwrap(),
            SecretString::new("r").unwrap(),
            now + REFRESH_SKEW + Duration::from_secs(1),
            "project-1",
        )
        .unwrap();
        assert!(!far.refresh_due(now));
    }

    #[tokio::test]
    async fn concurrent_refresh_uses_one_call_and_preserves_project() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let coordinator = Arc::new(AntigravityRefreshCoordinator::default());
        let transport = Arc::new(MockTransport {
            calls: AtomicUsize::new(0),
            response:
                br#"{"access_token":"access-new","refresh_token":"refresh-new","expires_in":3600}"#
                    .to_vec(),
        });
        let first = {
            let coordinator = Arc::clone(&coordinator);
            let transport = Arc::clone(&transport);
            tokio::spawn(async move {
                coordinator
                    .refresh(&*transport, credentials(now), now)
                    .await
            })
        };
        tokio::task::yield_now().await;
        let second = {
            let coordinator = Arc::clone(&coordinator);
            let transport = Arc::clone(&transport);
            tokio::spawn(async move {
                coordinator
                    .refresh(&*transport, credentials(now), now)
                    .await
            })
        };
        let a = first.await.unwrap().unwrap();
        let b = second.await.unwrap().unwrap();
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        assert_eq!(a.access_token().expose_secret(), "access-new");
        assert_eq!(b.refresh_token().expose_secret(), "refresh-new");
        assert_eq!(a.project_id(), "project-1");
        assert_eq!(a.expires_at(), now + Duration::from_secs(3600));
    }

    #[tokio::test]
    async fn provider_error_does_not_echo_body_or_tokens() {
        struct Failure;
        impl AntigravityRefreshTransport for Failure {
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
                    Ok(AntigravityRefreshHttpResponse::new(
                        429,
                        b"provider do-not-leak refresh-old".to_vec(),
                    ))
                })
            }
        }
        let now = SystemTime::UNIX_EPOCH;
        let error = AntigravityRefreshCoordinator::default()
            .refresh(&Failure, credentials(now), now)
            .await
            .unwrap_err();
        let rendered = error.to_string();
        assert_eq!(rendered, "Antigravity token refresh failed with status 429");
        assert!(!rendered.contains("do-not-leak"));
        assert!(!rendered.contains("refresh-old"));
    }
}
