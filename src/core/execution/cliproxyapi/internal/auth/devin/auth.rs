// ref: internal/auth/devin/devin_auth.go:22-191
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — selected SDK transport and bounded async calls
// License: MIT (upstream); modifications AGPL-3.0-only

use super::user_status::DevinStatusService;
use crate::internal::runtime::executor::devin_executor_response::go_utf8_text;
use crate::sdk::pluginapi::{Headers, HostHttpClient, HttpRequest, PluginExecutionError};
use std::{fmt, sync::Arc, time::Duration};

pub const DEFAULT_APP_BASE_URL: &str = "https://app.devin.ai";
pub const DEFAULT_API_BASE_URL: &str = "https://api.devin.ai";
pub const DEFAULT_SERVER_URL: &str = "https://server.codeium.com";
const TOKEN_PREFIX: &str = "devin-session-token$";
const AUTH_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_AUTH_BODY: usize = 1 << 20;

#[derive(Clone, Default, Eq, PartialEq)]
pub struct DevinSelfProfile {
    pub user_name: String,
    pub user_id: String,
    pub org_id: String,
}
impl fmt::Debug for DevinSelfProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinSelfProfile")
            .field("has_user_name", &!self.user_name.is_empty())
            .field("has_user_id", &!self.user_id.is_empty())
            .field("has_org_id", &!self.org_id.is_empty())
            .finish()
    }
}

/// Transport is supplied by the selected account/login owner. It remains the
/// sole authority for proxy/TLS. There is no default global client or retry.
pub struct DevinAuthService {
    pub(crate) client: Arc<dyn HostHttpClient>,
    app_base_url: String,
    api_base_url: String,
    pub(crate) server_base_url: String,
}
impl fmt::Debug for DevinAuthService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinAuthService").finish_non_exhaustive()
    }
}
impl DevinAuthService {
    pub fn new(client: Arc<dyn HostHttpClient>) -> Self {
        Self {
            client,
            app_base_url: DEFAULT_APP_BASE_URL.into(),
            api_base_url: DEFAULT_API_BASE_URL.into(),
            server_base_url: DEFAULT_SERVER_URL.into(),
        }
    }
    pub fn with_app_base_url(mut self, value: &str) -> Self {
        set_base_url(&mut self.app_base_url, value);
        self
    }
    pub fn with_api_base_url(mut self, value: &str) -> Self {
        set_base_url(&mut self.api_base_url, value);
        self
    }
    pub fn with_server_base_url(mut self, value: &str) -> Self {
        set_base_url(&mut self.server_base_url, value);
        self
    }
    pub fn status_service(&self) -> DevinStatusService {
        DevinStatusService::new(Arc::clone(&self.client))
            .with_server_base_url(&self.server_base_url)
    }

    // ref: internal/auth/devin/devin_auth.go:84-109
    pub fn build_authorization_url(
        &self,
        redirect_uri: &str,
        code_challenge: &str,
        state: &str,
    ) -> String {
        let redirect = redirect_uri.trim();
        let mut query = Vec::new();
        if !redirect.is_empty() {
            query.push(format!("redirect_uri={}", query_escape(redirect)));
        }
        if !state.is_empty() {
            query.push(format!("state={}", query_escape(state)));
        }
        query.push("prompt=select_account".into());
        query.push(format!("code_challenge={}", query_escape(code_challenge)));
        query.push("code_challenge_method=S256".into());
        if redirect.is_empty() {
            query.push("cli_pkce_marker=1".into());
        }
        format!(
            "{}/auth/cli/continue?{}",
            self.app_base_url.trim_end_matches('/'),
            query.join("&")
        )
    }

    // ref: internal/auth/devin/devin_auth.go:111-154
    pub async fn exchange_code_for_token(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<String, DevinAuthError> {
        self.exchange_bounded(code, code_verifier, AUTH_TIMEOUT)
            .await
    }
    async fn exchange_bounded(
        &self,
        code: &str,
        code_verifier: &str,
        deadline: Duration,
    ) -> Result<String, DevinAuthError> {
        let body = serde_json::to_vec(&serde_json::json!({
            "code": code.trim(), "code_verifier": code_verifier.trim(),
        }))
        .map_err(DevinAuthError::Encode)?;
        let request = HttpRequest {
            method: "POST".into(),
            url: format!("{}/auth/cli/token", self.api_base_url.trim_end_matches('/')),
            headers: Headers::from([
                ("Content-Type".into(), vec!["application/json".into()]),
                ("Accept".into(), vec!["application/json".into()]),
            ]),
            body,
        };
        let (status, body) = self.fetch_json(request, deadline).await?;
        if !(200..300).contains(&status) {
            return Err(DevinAuthError::Upstream { status, body });
        }
        let token = json_field_string(&body, "token").trim().to_owned();
        if token.is_empty() {
            return Err(DevinAuthError::MissingToken { body });
        }
        Ok(token)
    }

    // ref: internal/auth/devin/devin_auth.go:156-184
    pub async fn fetch_self_profile(
        &self,
        session_token: &str,
    ) -> Result<DevinSelfProfile, DevinAuthError> {
        let request = HttpRequest {
            method: "GET".into(),
            url: format!("{}/v3/self", self.api_base_url.trim_end_matches('/')),
            headers: Headers::from([
                (
                    "Authorization".into(),
                    vec![format!("Bearer {session_token}")],
                ),
                ("Accept".into(), vec!["application/json".into()]),
            ]),
            body: Vec::new(),
        };
        let (status, body) = self.fetch_json(request, AUTH_TIMEOUT).await?;
        if status != 200 {
            return Ok(DevinSelfProfile::default());
        }
        Ok(DevinSelfProfile {
            user_name: json_field_string(&body, "user_name"),
            user_id: json_field_string(&body, "user_id"),
            org_id: json_field_string(&body, "org_id"),
        })
    }

    async fn fetch_json(
        &self,
        request: HttpRequest,
        deadline: Duration,
    ) -> Result<(u16, Vec<u8>), DevinAuthError> {
        url::Url::parse(&request.url).map_err(DevinAuthError::InvalidUrl)?;
        let operation = async {
            let mut response = self
                .client
                .execute_stream(request)
                .await
                .map_err(DevinAuthError::Transport)?;
            let mut body = Vec::new();
            while let Some(chunk) = response.chunks.recv().await {
                let count = chunk.payload.len().min(MAX_AUTH_BODY - body.len());
                body.extend_from_slice(&chunk.payload[..count]);
                if let Some(error) = chunk.error {
                    return Err(DevinAuthError::Transport(error));
                }
                if body.len() == MAX_AUTH_BODY {
                    break;
                }
                tokio::task::yield_now().await;
            }
            Ok((response.status_code, body))
        };
        tokio::time::timeout(deadline, operation)
            .await
            .map_err(|_| DevinAuthError::Timeout)?
    }
}

pub fn format_session_token(raw_token: &str) -> String {
    let token = raw_token.trim();
    if token.starts_with(TOKEN_PREFIX) || !token.starts_with("eyJ") {
        token.into()
    } else {
        format!("{TOKEN_PREFIX}{token}")
    }
}
fn set_base_url(destination: &mut String, value: &str) {
    if !value.trim().is_empty() {
        *destination = value.trim().trim_end_matches('/').into();
    }
}
// Go url.QueryEscape preserves '~' and escapes '*', unlike a WHATWG form encoder.
fn query_escape(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(char::from(byte))
            }
            b' ' => output.push('+'),
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}
fn json_field_string(body: &[u8], key: &str) -> String {
    let text = go_utf8_text(body);
    let value = gjson::get(&text, key);
    let raw = value.str();
    if value.kind() == gjson::Kind::Number
        && !raw
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
    {
        let number = value.f64();
        if number == f64::INFINITY {
            return "+Inf".into();
        }
        if number == f64::NEG_INFINITY {
            return "-Inf".into();
        }
        return number.to_string();
    }
    raw.into()
}

pub enum DevinAuthError {
    InvalidUrl(url::ParseError),
    Encode(serde_json::Error),
    Transport(PluginExecutionError),
    Upstream { status: u16, body: Vec<u8> },
    MissingToken { body: Vec<u8> },
    EmptySessionToken,
    Timeout,
}
impl fmt::Debug for DevinAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Upstream { status, body } => f
                .debug_struct("DevinAuthError::Upstream")
                .field("status", status)
                .field("body_bytes", &body.len())
                .finish(),
            Self::MissingToken { body } => f
                .debug_struct("DevinAuthError::MissingToken")
                .field("body_bytes", &body.len())
                .finish(),
            Self::InvalidUrl(_) => f.write_str("DevinAuthError::InvalidUrl"),
            Self::Encode(_) => f.write_str("DevinAuthError::Encode"),
            Self::Transport(_) => f.write_str("DevinAuthError::Transport"),
            Self::EmptySessionToken => f.write_str("DevinAuthError::EmptySessionToken"),
            Self::Timeout => f.write_str("DevinAuthError::Timeout"),
        }
    }
}
impl fmt::Display for DevinAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(error) => write!(f, "{error}"),
            Self::Encode(error) => write!(f, "{error}"),
            Self::Transport(error) => write!(f, "devin authentication request failed: {error}"),
            Self::Upstream { status, body } => write!(
                f,
                "token exchange failed with status {status}: {}",
                go_utf8_text(body)
            ),
            Self::MissingToken { body } => write!(
                f,
                "response did not contain a valid token: {}",
                go_utf8_text(body)
            ),
            Self::EmptySessionToken => f.write_str("devin session token is required"),
            Self::Timeout => f.write_str("devin authentication request timed out"),
        }
    }
}
impl std::error::Error for DevinAuthError {}

#[cfg(test)]
#[path = "auth_test.rs"]
mod tests;
