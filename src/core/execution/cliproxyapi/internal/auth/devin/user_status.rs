// ref: internal/auth/devin/user_status.go:26-377
// ref: internal/runtime/executor/devin_executor.go:142-226
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — selected HTTP owner, fixed deadline, optional quota observations
// License: MIT (upstream); modifications AGPL-3.0-only

use crate::internal::runtime::executor::devin_executor_request::devin_auth_credentials;
use crate::internal::runtime::executor::devin_executor_response::go_utf8_text;
use crate::internal::runtime::executor::helps::devin_proto::{
    DevinProtoError, WireReader, WireValue,
};
use crate::internal::runtime::executor::helps::devin_request::{
    generate_devin_device_fingerprint, DEVIN_DEFAULT_BASE_URL,
};
use crate::sdk::cliproxy::auth::Auth;
use crate::sdk::pluginapi::{Headers, HostHttpClient, HttpRequest, PluginExecutionError};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::{fmt, sync::Arc, time::Duration};

pub const DEVIN_GET_USER_STATUS_PATH: &str =
    "/exa.seat_management_pb.SeatManagementService/GetUserStatus";
const MAX_STATUS_BODY: usize = 4 * 1024 * 1024;
const STATUS_TIMEOUT: Duration = Duration::from_secs(30);

/// Wire timestamps retain their signed Unix seconds, including Go varint casts.
/// Optional percentages distinguish an absent API observation from known zero.
#[derive(Clone, Default, Eq, PartialEq)]
pub struct DevinUserStatus {
    pub email: String,
    pub user_name: String,
    pub user_id: String,
    pub team_id: String,
    pub org_id: String,
    pub org_name: String,
    pub plan: String,
    pub daily_quota_remaining_percent: Option<i64>,
    pub weekly_quota_remaining_percent: Option<i64>,
    pub daily_quota_reset_at: Option<i64>,
    pub weekly_quota_reset_at: Option<i64>,
    pub plan_start: Option<i64>,
    pub plan_end: Option<i64>,
}
impl fmt::Debug for DevinUserStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinUserStatus")
            .field("has_plan", &!self.plan.is_empty())
            .field(
                "has_daily_quota",
                &self.daily_quota_remaining_percent.is_some(),
            )
            .field(
                "has_weekly_quota",
                &self.weekly_quota_remaining_percent.is_some(),
            )
            .finish_non_exhaustive()
    }
}

pub enum DevinStatusError {
    MissingToken,
    EmptyResponse,
    InvalidUrl(url::ParseError),
    Protobuf(DevinProtoError),
    Transport(PluginExecutionError),
    Timeout,
    Upstream { status: u16, body: Vec<u8> },
}
impl fmt::Debug for DevinStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Upstream { status, body } => f
                .debug_struct("DevinStatusError")
                .field("status", status)
                .field("body_bytes", &body.len())
                .finish(),
            Self::Transport(_) => f.write_str("DevinStatusError::Transport"),
            Self::InvalidUrl(_) => f.write_str("DevinStatusError::InvalidUrl"),
            Self::Protobuf(error) => f
                .debug_tuple("DevinStatusError::Protobuf")
                .field(error)
                .finish(),
            Self::MissingToken => f.write_str("DevinStatusError::MissingToken"),
            Self::EmptyResponse => f.write_str("DevinStatusError::EmptyResponse"),
            Self::Timeout => f.write_str("DevinStatusError::Timeout"),
        }
    }
}
impl fmt::Display for DevinStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingToken => f.write_str("devin auth service: session token is required"),
            Self::EmptyResponse => f.write_str("empty response data"),
            Self::InvalidUrl(error) => write!(f, "devin seat management URL is invalid: {error}"),
            Self::Protobuf(error) => write!(f, "{error}"),
            Self::Transport(error) => write!(f, "{error}"),
            Self::Timeout => {
                f.write_str("devin seat management request exceeded its 30 second deadline")
            }
            Self::Upstream { status, body } => write!(
                f,
                "devin seat management error (status {status}): {}",
                go_utf8_text(body)
            ),
        }
    }
}
impl std::error::Error for DevinStatusError {}

fn append_varint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        bytes.push(value as u8 | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
}
fn append_data(bytes: &mut Vec<u8>, field: u32, data: &[u8]) {
    append_varint(bytes, u64::from(field) << 3 | 2);
    append_varint(bytes, data.len() as u64);
    bytes.extend_from_slice(data);
}
pub fn build_get_user_status_request(token: &str, fingerprint: &str, platform: &str) -> Vec<u8> {
    let generated = fingerprint
        .is_empty()
        .then(|| generate_devin_device_fingerprint(token));
    let fingerprint = generated.as_deref().unwrap_or(fingerprint);
    let mut metadata = Vec::new();
    for (number, value) in [
        (1, "chisel"),
        (2, "3000.10.21"),
        (3, token),
        (4, "en"),
        (5, platform),
        (7, "3000.10.21"),
        (12, "chisel"),
        (31, fingerprint),
    ] {
        append_data(&mut metadata, number, value.as_bytes());
    }
    let mut body = Vec::new();
    append_data(&mut body, 1, &metadata);
    body
}

pub fn parse_get_user_status_response(bytes: &[u8]) -> Result<DevinUserStatus, DevinStatusError> {
    if bytes.is_empty() {
        return Err(DevinStatusError::EmptyResponse);
    }
    let mut status = DevinUserStatus::default();
    let mut reader = WireReader::new(bytes);
    while let Some(field) = reader.next().map_err(DevinStatusError::Protobuf)? {
        if let (1, WireValue::Bytes(value)) = (field.number, field.value) {
            parse_user_status(value, &mut status);
        }
    }
    Ok(status)
}
fn parse_user_status(bytes: &[u8], status: &mut DevinUserStatus) {
    let mut reader = WireReader::new(bytes);
    // Upstream deliberately keeps valid preceding fields when a nested message
    // ends malformed, while top-level errors are returned to the caller.
    while let Ok(Some(field)) = reader.next() {
        if let WireValue::Bytes(value) = field.value {
            match field.number {
                3 => status.user_name = go_utf8_text(value),
                5 => status.team_id = go_utf8_text(value),
                7 => status.email = go_utf8_text(value),
                13 => parse_plan_status(value, status),
                36 => status.user_id = go_utf8_text(value),
                _ => {}
            }
        }
    }
}
fn parse_plan_status(bytes: &[u8], status: &mut DevinUserStatus) {
    let mut reader = WireReader::new(bytes);
    while let Ok(Some(field)) = reader.next() {
        match field.value {
            WireValue::Bytes(value) => match field.number {
                1 => parse_plan_info(value, status),
                2 => {
                    let seconds = parse_seconds(value);
                    if seconds > 0 {
                        status.plan_start = Some(seconds);
                    }
                }
                3 => {
                    let seconds = parse_seconds(value);
                    if seconds > 0 {
                        status.plan_end = Some(seconds);
                    }
                }
                _ => {}
            },
            WireValue::Varint(value) => match field.number {
                14 => status.daily_quota_remaining_percent = Some(value as i64),
                15 => status.weekly_quota_remaining_percent = Some(value as i64),
                17 if value > 0 => status.daily_quota_reset_at = Some(value as i64),
                18 if value > 0 => status.weekly_quota_reset_at = Some(value as i64),
                _ => {}
            },
            _ => {}
        }
    }
}
fn parse_plan_info(bytes: &[u8], status: &mut DevinUserStatus) {
    let mut reader = WireReader::new(bytes);
    while let Ok(Some(field)) = reader.next() {
        if let WireValue::Bytes(value) = field.value {
            match field.number {
                2 => status.plan = go_utf8_text(value),
                33 => parse_org(value, status),
                _ => {}
            }
        }
    }
}
fn parse_org(bytes: &[u8], status: &mut DevinUserStatus) {
    let mut reader = WireReader::new(bytes);
    while let Ok(Some(field)) = reader.next() {
        if let WireValue::Bytes(value) = field.value {
            match field.number {
                4 => status.org_id = go_utf8_text(value),
                8 => status.org_name = go_utf8_text(value),
                _ => {}
            }
        }
    }
}
fn parse_seconds(bytes: &[u8]) -> i64 {
    let mut reader = WireReader::new(bytes);
    while let Ok(Some(field)) = reader.next() {
        if let (1, WireValue::Varint(value)) = (field.number, field.value) {
            return value as i64;
        }
    }
    0
}
fn platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

/// UTC calendar formatting retains Go's signed Unix-second domain rather than
/// silently discarding dates outside chrono's calendar range.
fn format_unix_rfc3339(seconds: i64) -> String {
    let days = i128::from(seconds.div_euclid(86_400)) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    let year = if year < 0 {
        format!("-{:04}", -year)
    } else {
        format!("{year:04}")
    };
    let time = seconds.rem_euclid(86_400);
    format!(
        "{year}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time % 3600 / 60,
        time % 60
    )
}

pub fn apply_user_status(auth: &Auth, status: &DevinUserStatus, now: DateTime<Utc>) -> Auth {
    let mut updated = auth.clone();
    for (key, value) in [
        ("email", &status.email),
        ("user_name", &status.user_name),
        ("user_id", &status.user_id),
        ("team_id", &status.team_id),
        ("plan", &status.plan),
        ("org_id", &status.org_id),
        ("org_name", &status.org_name),
    ] {
        if !value.is_empty() {
            updated
                .metadata
                .insert(key.into(), Value::String(value.clone()));
            updated.attributes.insert(key.into(), value.clone());
        }
    }
    // A successful quota observation replaces the previous provider snapshot.
    // Missing percentages remain unknown; they must not become a fabricated 0%.
    updated.quota.signals.clear();
    if !status.plan.is_empty() {
        updated
            .quota
            .signals
            .insert("plan".into(), status.plan.clone());
    }
    for (key, value) in [
        (
            "daily_quota_remaining_percent",
            status.daily_quota_remaining_percent,
        ),
        (
            "weekly_quota_remaining_percent",
            status.weekly_quota_remaining_percent,
        ),
    ] {
        if let Some(value) = value {
            updated
                .quota
                .signals
                .insert(key.into(), format!("{value}%"));
        }
    }
    for (key, seconds) in [
        ("daily_quota_reset_at", status.daily_quota_reset_at),
        ("weekly_quota_reset_at", status.weekly_quota_reset_at),
        ("plan_start", status.plan_start),
        ("plan_end", status.plan_end),
    ] {
        if let Some(seconds) = seconds {
            updated
                .quota
                .signals
                .insert(key.into(), format_unix_rfc3339(seconds));
        }
    }
    updated.quota.observed_at = now;
    updated.last_refreshed_at = now;
    updated
}

/// The caller supplies the selected account's transport, including its proxy
/// and TLS policy. No global client, credential store or automatic retry exists.
pub struct DevinStatusService {
    client: Arc<dyn HostHttpClient>,
    server_base_url: String,
}
impl fmt::Debug for DevinStatusService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinStatusService").finish_non_exhaustive()
    }
}
impl DevinStatusService {
    pub fn new(client: Arc<dyn HostHttpClient>) -> Self {
        Self {
            client,
            server_base_url: DEVIN_DEFAULT_BASE_URL.into(),
        }
    }
    pub fn with_server_base_url(mut self, value: &str) -> Self {
        if !value.trim().is_empty() {
            self.server_base_url = value.trim().trim_end_matches('/').into();
        }
        self
    }
    pub async fn fetch_user_status(
        &self,
        token: &str,
        device_seed: &str,
    ) -> Result<DevinUserStatus, DevinStatusError> {
        self.fetch_bounded(token, device_seed, STATUS_TIMEOUT).await
    }
    async fn fetch_bounded(
        &self,
        token: &str,
        device_seed: &str,
        deadline: Duration,
    ) -> Result<DevinUserStatus, DevinStatusError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(DevinStatusError::MissingToken);
        }
        let body = build_get_user_status_request(
            token,
            &generate_devin_device_fingerprint(device_seed),
            platform(),
        );
        let url = format!(
            "{}{}",
            self.server_base_url.trim_end_matches('/'),
            DEVIN_GET_USER_STATUS_PATH
        );
        url::Url::parse(&url).map_err(DevinStatusError::InvalidUrl)?;
        let headers = Headers::from([
            (
                "Authorization".into(),
                vec![format!("Basic {token}-{token}")],
            ),
            ("Connect-Protocol-Version".into(), vec!["1".into()]),
            ("Content-Type".into(), vec!["application/proto".into()]),
            ("Accept".into(), vec!["*/*".into()]),
            ("User-Agent".into(), vec![String::new()]),
        ]);
        let operation = async {
            let mut response = self
                .client
                .execute_stream(HttpRequest {
                    method: "POST".into(),
                    url,
                    headers,
                    body,
                })
                .await
                .map_err(DevinStatusError::Transport)?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunks.recv().await {
                let count = chunk.payload.len().min(MAX_STATUS_BODY - bytes.len());
                bytes.extend_from_slice(&chunk.payload[..count]);
                if let Some(error) = chunk.error {
                    return Err(DevinStatusError::Transport(error));
                }
                if bytes.len() == MAX_STATUS_BODY {
                    break;
                }
                tokio::task::yield_now().await;
            }
            if response.status_code != 200 {
                return Err(DevinStatusError::Upstream {
                    status: response.status_code,
                    body: bytes,
                });
            }
            parse_get_user_status_response(&bytes)
        };
        tokio::time::timeout(deadline, operation)
            .await
            .map_err(|_| DevinStatusError::Timeout)?
    }
    pub async fn refresh_auth(
        &self,
        auth: &Auth,
        now: DateTime<Utc>,
    ) -> Result<Auth, DevinStatusError> {
        let credentials = devin_auth_credentials(&auth.attributes, &auth.metadata);
        if credentials.session_token.is_empty() {
            return Ok(auth.clone());
        }
        let selected = Self::new(self.client.clone()).with_server_base_url(credentials.base_url);
        let status = selected
            .fetch_user_status(credentials.session_token, credentials.device_seed)
            .await?;
        Ok(apply_user_status(auth, &status, now))
    }
}

#[cfg(test)]
#[path = "user_status_test.rs"]
mod tests;
