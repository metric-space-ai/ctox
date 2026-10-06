// ref: internal/auth/meta/meta.go:25-118,228-536 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — selected owned HTTP transport, bounded bodies and injected clock
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::sdk::pluginapi::{
    Headers, HostHttpClient, HttpRequest, HttpResponse, PluginExecutionError,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc, time::Duration};
use tokio::time::{self, Instant};
use zeroize::Zeroizing;

pub const DEFAULT_API_BASE_URL: &str = "https://api.meta.ai/v1";
pub const DEVICE_AUTHORIZATION_ENDPOINT: &str = "https://auth.meta.com/oidc/device/authorization/";
pub const TOKEN_ENDPOINT: &str = "https://auth.meta.com/oidc/device/token/";
pub const MINT_ENDPOINT: &str = "https://api.meta.ai/muse-code/key";
pub const CLIENT_ID: &str = "1031625952748946";
pub const DEVICE_CODE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);
pub const MAX_POLL_DURATION: Duration = Duration::from_secs(15 * 60);
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_AUTH_BODY: usize = 1024 * 1024;
const USER_AGENT: &str = "muse-code/1.0.2";

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: i64,
    pub interval: i64,
    // The provider response cannot redirect a credential-bearing token request.
    #[serde(skip)]
    pub token_endpoint: String,
}
impl fmt::Debug for DeviceCodeResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaDeviceCodeResponse")
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct TokenData {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub expires_at: i64,
    pub error: String,
    pub error_description: String,
}
impl fmt::Debug for TokenData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaTokenData")
            .field("expires_in", &self.expires_in)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct MintedKeyResponse {
    pub api_key: String,
    pub base_url: String,
    pub user_email: String,
    pub user_full_name: String,
    pub subs_tier_name: String,
    pub subs_tier_id: String,
    pub is_subs_active: bool,
    pub has_payment_method: bool,
    pub require_payment: bool,
    pub can_subscribe: bool,
}
impl fmt::Debug for MintedKeyResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaMintedKeyResponse")
            .field("is_subs_active", &self.is_subs_active)
            .field("has_payment_method", &self.has_payment_method)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Default)]
pub struct MetaAuthBundle {
    pub token_data: Option<TokenData>,
    pub minted_key: Option<MintedKeyResponse>,
    pub email: String,
    pub name: String,
}
impl fmt::Debug for MetaAuthBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaAuthBundle")
            .field("has_token", &self.token_data.is_some())
            .field("has_minted_key", &self.minted_key.is_some())
            .finish_non_exhaustive()
    }
}

pub trait MetaClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
#[derive(Default, Debug)]
pub struct SystemMetaClock;
impl MetaClock for SystemMetaClock {
    fn now(&self) -> DateTime<Utc> {
        std::time::SystemTime::now().into()
    }
}

pub enum MetaAuthError {
    MissingDeviceResponse,
    MissingDcaToken,
    MissingField(&'static str),
    InvalidUrl(url::ParseError),
    InvalidJson(serde_json::Error),
    Transport(PluginExecutionError),
    Upstream { status: u16, body: Vec<u8> },
    Provider { code: String, description: String },
    AccessDenied,
    ExpiredDeviceCode,
    Timeout,
    BodyLimit,
}
impl fmt::Debug for MetaAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaAuthError")
            .field("message", &self.to_string())
            .finish()
    }
}
impl fmt::Display for MetaAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDeviceResponse => f.write_str("Meta login requires a device code"),
            Self::MissingDcaToken => f.write_str("Meta key minting requires a DCA token"),
            Self::MissingField(field) => write!(f, "Meta response is missing {field}"),
            Self::InvalidUrl(_) => f.write_str("Meta authentication endpoint is invalid"),
            Self::InvalidJson(_) => f.write_str("Meta authentication response is invalid JSON"),
            Self::Transport(_) => f.write_str("Meta authentication transport failed"),
            Self::Upstream { status, .. } => {
                write!(f, "Meta authentication returned HTTP {status}")
            }
            Self::Provider { .. } => f.write_str("Meta authorization server rejected the login"),
            Self::AccessDenied => f.write_str("Meta access was denied by the user"),
            Self::ExpiredDeviceCode => f.write_str("Meta device code has expired"),
            Self::Timeout => f.write_str("Meta authorization timed out"),
            Self::BodyLimit => f.write_str("Meta authentication response exceeds its body limit"),
        }
    }
}
impl std::error::Error for MetaAuthError {}

/// Credential acquisition always uses the selected host client's proxy/TLS policy.
/// Dropping an operation drops its upstream receiver; no task or client is detached.
pub struct MetaAuth {
    client: Arc<dyn HostHttpClient>,
    clock: Arc<dyn MetaClock>,
    mint_endpoint: String,
}
impl fmt::Debug for MetaAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaAuth").finish_non_exhaustive()
    }
}
impl MetaAuth {
    pub fn new(client: Arc<dyn HostHttpClient>) -> Self {
        Self {
            client,
            clock: Arc::new(SystemMetaClock),
            mint_endpoint: MINT_ENDPOINT.into(),
        }
    }
    pub fn with_clock(mut self, clock: Arc<dyn MetaClock>) -> Self {
        self.clock = clock;
        self
    }
    // An explicit host/test endpoint replaces upstream's ambient META_MINT_URL.
    pub fn with_mint_endpoint(mut self, endpoint: &str) -> Self {
        self.mint_endpoint = if endpoint.trim().is_empty() {
            MINT_ENDPOINT.into()
        } else {
            endpoint.trim().into()
        };
        self
    }
    pub async fn start_device_flow(&self) -> Result<DeviceCodeResponse, MetaAuthError> {
        self.start_device_flow_with_endpoint(DEVICE_AUTHORIZATION_ENDPOINT)
            .await
    }
    pub async fn start_device_flow_with_endpoint(
        &self,
        endpoint: &str,
    ) -> Result<DeviceCodeResponse, MetaAuthError> {
        let endpoint = if endpoint.trim().is_empty() {
            DEVICE_AUTHORIZATION_ENDPOINT
        } else {
            endpoint.trim()
        };
        let form = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("client_id", CLIENT_ID)
            .finish();
        let response = self
            .request(
                auth_request(
                    endpoint,
                    "application/x-www-form-urlencoded",
                    form.into_bytes(),
                )?,
                HTTP_TIMEOUT,
            )
            .await?;
        if !(200..300).contains(&response.status_code) {
            return Err(MetaAuthError::Upstream {
                status: response.status_code,
                body: response.body,
            });
        }
        let mut device: DeviceCodeResponse =
            serde_json::from_slice(&response.body).map_err(MetaAuthError::InvalidJson)?;
        if device.device_code.trim().is_empty() {
            return Err(MetaAuthError::MissingField("device_code"));
        }
        if device.user_code.trim().is_empty() {
            return Err(MetaAuthError::MissingField("user_code"));
        }
        device.token_endpoint = TOKEN_ENDPOINT.into();
        Ok(device)
    }
    pub async fn wait_for_authorization(
        &self,
        device: &DeviceCodeResponse,
    ) -> Result<MetaAuthBundle, MetaAuthError> {
        if device.device_code.is_empty() {
            return Err(MetaAuthError::MissingDeviceResponse);
        }
        let token_endpoint = if device.token_endpoint.is_empty() {
            TOKEN_ENDPOINT
        } else {
            &device.token_endpoint
        };
        // Validate before waiting, without sending the device code.
        url::Url::parse(token_endpoint).map_err(MetaAuthError::InvalidUrl)?;
        let mut interval = if device.interval > 0 {
            Duration::from_secs(device.interval as u64)
        } else {
            DEFAULT_POLL_INTERVAL
        };
        // A larger interval cannot tick before the fixed authorization deadline.
        // Bound Instant arithmetic without turning it into an earlier request.
        interval = interval.min(MAX_POLL_DURATION + Duration::from_secs(5));
        let budget = if device.expires_in > 0 {
            MAX_POLL_DURATION.min(Duration::from_secs(device.expires_in as u64))
        } else {
            MAX_POLL_DURATION
        };
        let deadline = Instant::now() + budget;
        let operation = async {
            let start = Instant::now();
            let mut ticker = time::interval_at(start + interval, interval);
            ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                let form = Zeroizing::new(
                    url::form_urlencoded::Serializer::new(String::new())
                        .append_pair("client_id", CLIENT_ID)
                        .append_pair("device_code", &device.device_code)
                        .append_pair("grant_type", DEVICE_CODE_GRANT_TYPE)
                        .finish(),
                );
                let request = auth_request(
                    token_endpoint,
                    "application/x-www-form-urlencoded",
                    form.as_bytes().to_vec(),
                )?;
                let response = match self.request(request, HTTP_TIMEOUT).await {
                    Ok(response) => response,
                    Err(MetaAuthError::Transport(_) | MetaAuthError::Timeout) => continue,
                    Err(error) => return Err(error),
                };
                if response.status_code == 200 {
                    let mut token: TokenData = serde_json::from_slice(&response.body)
                        .map_err(MetaAuthError::InvalidJson)?;
                    if token.access_token.is_empty() {
                        return Err(MetaAuthError::MissingField("access_token"));
                    }
                    if token.expires_in > 0 {
                        token.expires_at = self
                            .clock
                            .now()
                            .timestamp()
                            .saturating_add(token.expires_in);
                    }
                    return Ok(token);
                }
                let failure: TokenData = serde_json::from_slice(&response.body).unwrap_or_default();
                match failure.error.as_str() {
                    "authorization_pending" => {}
                    "slow_down" => {
                        interval = interval
                            .saturating_add(Duration::from_secs(5))
                            .min(MAX_POLL_DURATION + Duration::from_secs(5));
                        ticker = time::interval_at(Instant::now() + interval, interval);
                        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
                    }
                    "access_denied" => return Err(MetaAuthError::AccessDenied),
                    "expired_token" => return Err(MetaAuthError::ExpiredDeviceCode),
                    "" => {}
                    _ => {
                        return Err(MetaAuthError::Provider {
                            code: failure.error,
                            description: failure.error_description,
                        })
                    }
                }
            }
        };
        let token = time::timeout_at(deadline, operation)
            .await
            .map_err(|_| MetaAuthError::Timeout)??;
        // Upstream returns the authorized DCA record when minting times out.
        // Caller cancellation still drops the entire future before persistence.
        let minted_key = time::timeout_at(deadline, self.mint_api_key(&token.access_token))
            .await
            .ok()
            .and_then(Result::ok);
        let email = minted_key
            .as_ref()
            .map_or_else(String::new, |key| key.user_email.clone());
        let name = minted_key
            .as_ref()
            .map_or_else(String::new, |key| key.user_full_name.clone());
        Ok(MetaAuthBundle {
            token_data: Some(token),
            minted_key,
            email,
            name,
        })
    }
    pub async fn mint_api_key(&self, token: &str) -> Result<MintedKeyResponse, MetaAuthError> {
        let token = Zeroizing::new(token.trim().to_owned());
        if token.is_empty() {
            return Err(MetaAuthError::MissingDcaToken);
        }
        let body = serde_json::to_vec(&serde_json::json!({"dca_token": token.as_str()}))
            .map_err(MetaAuthError::InvalidJson)?;
        let mut request = auth_request(&self.mint_endpoint, "application/json", body)?;
        request.headers.insert(
            "Authorization".into(),
            vec![format!("Bearer {}", token.as_str())],
        );
        let response = self.request(request, HTTP_TIMEOUT).await?;
        if !(200..300).contains(&response.status_code) {
            return Err(MetaAuthError::Upstream {
                status: response.status_code,
                body: response.body,
            });
        }
        let minted: MintedKeyResponse =
            serde_json::from_slice(&response.body).map_err(MetaAuthError::InvalidJson)?;
        if minted.api_key.trim().is_empty() {
            return Err(MetaAuthError::MissingField("api_key"));
        }
        Ok(minted)
    }
    pub fn create_token_storage(
        &self,
        bundle: &MetaAuthBundle,
    ) -> Option<super::token::MetaTokenStorage> {
        super::token::MetaTokenStorage::from_bundle(bundle, self.clock.now())
    }
    async fn request(
        &self,
        request: HttpRequest,
        deadline: Duration,
    ) -> Result<HttpResponse, MetaAuthError> {
        let operation = async {
            let mut response = self
                .client
                .execute_stream(request)
                .await
                .map_err(MetaAuthError::Transport)?;
            let mut body = Vec::new();
            while let Some(chunk) = response.chunks.recv().await {
                if let Some(error) = chunk.error {
                    return Err(MetaAuthError::Transport(error));
                }
                if chunk.payload.len() > MAX_AUTH_BODY - body.len() {
                    return Err(MetaAuthError::BodyLimit);
                }
                body.extend_from_slice(&chunk.payload);
            }
            Ok(HttpResponse {
                status_code: response.status_code,
                headers: response.headers,
                body,
            })
        };
        time::timeout(deadline, operation)
            .await
            .map_err(|_| MetaAuthError::Timeout)?
    }
}
fn auth_request(
    url: &str,
    content_type: &str,
    body: Vec<u8>,
) -> Result<HttpRequest, MetaAuthError> {
    url::Url::parse(url).map_err(MetaAuthError::InvalidUrl)?;
    Ok(HttpRequest {
        method: "POST".into(),
        url: url.into(),
        body,
        headers: Headers::from([
            ("Content-Type".into(), vec![content_type.into()]),
            ("Accept".into(), vec!["application/json".into()]),
            ("User-Agent".into(), vec![USER_AGENT.into()]),
        ]),
    })
}
pub(crate) fn rfc3339(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}
