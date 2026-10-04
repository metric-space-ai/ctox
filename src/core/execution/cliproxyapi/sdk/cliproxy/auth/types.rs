// ref: sdk/cliproxy/auth/types.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::internal::auth::models::SharedTokenStorage;
use crate::sdk::cliproxy::executor::{Headers, QueryValues};

use super::{AuthError, AuthKind, AuthStatus};

pub const ATTRIBUTE_AUTH_INDEX_SEED: &str = "auth_index_seed";
pub const ATTRIBUTE_PLUGIN_VIRTUAL: &str = "plugin_virtual";
pub const ATTRIBUTE_VIRTUAL_SOURCE: &str = "virtual_source";
const PLUGIN_VIRTUAL_ENABLED: &str = "true";
const RECENT_REQUEST_BUCKET_SECONDS: i64 = 10 * 60;
const RECENT_REQUEST_BUCKET_COUNT: usize = 20;
const EXPIRATION_KEYS: [&str; 6] = [
    "expired",
    "expire",
    "expires_at",
    "expiresAt",
    "expiry",
    "expires",
];

pub trait RefreshLeadRuntime: Send + Sync {
    fn refresh_lead(&self) -> Option<Duration> {
        None
    }

    fn evaluates_refresh(&self) -> bool {
        false
    }

    fn should_refresh(&self, _now: DateTime<Utc>, _auth: &Auth) -> bool {
        false
    }
}

pub type SharedAuthRuntime = Arc<dyn RefreshLeadRuntime>;
type RefreshLeadFactory = Arc<dyn Fn() -> Option<Duration> + Send + Sync>;
pub type PostAuthError = Box<dyn Error + Send + Sync + 'static>;
pub type PostAuthHook =
    Arc<dyn Fn(&PostAuthContext, &mut Auth) -> Result<(), PostAuthError> + Send + Sync + 'static>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RequestInfo {
    pub query: QueryValues,
    pub headers: Headers,
}

#[derive(Clone, Debug, Default)]
pub struct PostAuthContext {
    request_info: Option<Arc<RequestInfo>>,
}

impl PostAuthContext {
    #[must_use]
    pub fn with_request_info(&self, request_info: RequestInfo) -> Self {
        Self {
            request_info: Some(Arc::new(request_info)),
        }
    }

    #[must_use]
    pub fn request_info(&self) -> Option<&RequestInfo> {
        self.request_info.as_deref()
    }
}

fn refresh_lead_factories() -> &'static RwLock<BTreeMap<String, RefreshLeadFactory>> {
    static FACTORIES: OnceLock<RwLock<BTreeMap<String, RefreshLeadFactory>>> = OnceLock::new();
    FACTORIES.get_or_init(|| RwLock::new(BTreeMap::new()))
}

fn is_zero_i64(value: &i64) -> bool {
    *value == 0
}

pub(crate) fn go_zero_time() -> DateTime<Utc> {
    NaiveDate::from_ymd_opt(1, 1, 1)
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|timestamp| timestamp.and_utc())
        .expect("year-one UTC must be representable")
}

pub fn register_refresh_lead_provider(
    provider: &str,
    factory: impl Fn() -> Option<Duration> + Send + Sync + 'static,
) {
    let provider = provider.trim().to_lowercase();
    if provider.is_empty() {
        return;
    }
    refresh_lead_factories()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(provider, Arc::new(factory));
}

#[must_use]
pub fn provider_refresh_lead(
    provider: &str,
    runtime: Option<&dyn RefreshLeadRuntime>,
) -> Option<Duration> {
    if let Some(lead) = runtime
        .and_then(RefreshLeadRuntime::refresh_lead)
        .filter(|lead| !lead.is_zero())
    {
        return Some(lead);
    }
    let provider = provider.trim().to_lowercase();
    let factory = refresh_lead_factories()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&provider)
        .cloned();
    factory.and_then(|factory| factory().filter(|lead| !lead.is_zero()))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RecentRequestSlot {
    bucket_id: i64,
    success: i64,
    failed: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecentRequestBucket {
    pub time: String,
    pub success: i64,
    pub failed: i64,
}

// ref: sdk/cliproxy/auth/types.go:175-205 @ d7914afdedca7af95ee974a42453dc49fc1388ce
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct QuotaState {
    pub exceeded: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
    pub next_recover_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub backoff_level: i64,
    // Go encoding/json retains a zero time.Time despite its omitempty tag.
    pub observed_at: DateTime<Utc>,
    // One upstream observation, independent of cooldown transitions.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub signals: BTreeMap<String, String>,
}

impl Default for QuotaState {
    fn default() -> Self {
        Self {
            exceeded: false,
            reason: String::new(),
            next_recover_at: go_zero_time(),
            backoff_level: 0,
            observed_at: go_zero_time(),
            signals: BTreeMap::new(),
        }
    }
}

impl QuotaState {
    /// Clear routing cooldown without discarding the last quota observation.
    pub(crate) fn clear_cooldown(&mut self) {
        self.exceeded = false;
        self.reason.clear();
        self.next_recover_at = go_zero_time();
        self.backoff_level = 0;
    }
}

impl fmt::Debug for QuotaState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QuotaState")
            .field("exceeded", &self.exceeded)
            .field("reason_len", &self.reason.len())
            .field("next_recover_at", &self.next_recover_at)
            .field("backoff_level", &self.backoff_level)
            .field("observed_at", &self.observed_at)
            .field("signals_len", &self.signals.len())
            .finish()
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ModelState {
    pub status: AuthStatus,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status_message: String,
    pub unavailable: bool,
    pub next_retry_after: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<AuthError>,
    pub quota: QuotaState,
    pub updated_at: DateTime<Utc>,
}

impl Default for ModelState {
    fn default() -> Self {
        Self {
            status: AuthStatus::Other(String::new()),
            status_message: String::new(),
            unavailable: false,
            next_retry_after: go_zero_time(),
            last_error: None,
            quota: QuotaState::default(),
            updated_at: go_zero_time(),
        }
    }
}

impl fmt::Debug for ModelState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelState")
            .field("status", &self.status)
            .field("status_message_len", &self.status_message.len())
            .field("unavailable", &self.unavailable)
            .field("next_retry_after", &self.next_retry_after)
            .field("last_error", &self.last_error)
            .field("quota", &self.quota)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Provider-neutral Auth aggregate with injected storage/runtime authority.
/// Debug never renders attribute or metadata values because either map can
/// carry credentials; Serde remains the explicit upstream persistence/wire
/// contract and skips all non-persisted authority fields.
#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Auth {
    pub id: String,
    #[serde(skip)]
    pub index: String,
    /// Manager-owned registration cycle; never supplied by JSON or storage.
    #[serde(skip)]
    pub registration_epoch: u64,
    pub provider: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    #[serde(skip)]
    pub file_name: String,
    #[serde(skip)]
    pub storage: Option<SharedTokenStorage>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    pub status: AuthStatus,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status_message: String,
    pub disabled: bool,
    pub unavailable: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub proxy_url: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
    pub quota: QuotaState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<AuthError>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_refreshed_at: DateTime<Utc>,
    pub next_refresh_after: DateTime<Utc>,
    pub next_retry_after: DateTime<Utc>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub model_states: BTreeMap<String, ModelState>,
    #[serde(skip)]
    pub runtime: Option<SharedAuthRuntime>,
    #[serde(skip)]
    pub success: i64,
    #[serde(skip)]
    pub failed: i64,
    #[serde(skip)]
    recent_requests: [RecentRequestSlot; RECENT_REQUEST_BUCKET_COUNT],
    #[serde(skip)]
    index_assigned: bool,
}

impl Default for Auth {
    fn default() -> Self {
        Self {
            id: String::new(),
            index: String::new(),
            registration_epoch: 0,
            provider: String::new(),
            prefix: String::new(),
            file_name: String::new(),
            storage: None,
            label: String::new(),
            status: AuthStatus::Other(String::new()),
            status_message: String::new(),
            disabled: false,
            unavailable: false,
            proxy_url: String::new(),
            attributes: BTreeMap::new(),
            metadata: BTreeMap::new(),
            quota: QuotaState::default(),
            last_error: None,
            created_at: go_zero_time(),
            updated_at: go_zero_time(),
            last_refreshed_at: go_zero_time(),
            next_refresh_after: go_zero_time(),
            next_retry_after: go_zero_time(),
            model_states: BTreeMap::new(),
            runtime: None,
            success: 0,
            failed: 0,
            recent_requests: [RecentRequestSlot::default(); RECENT_REQUEST_BUCKET_COUNT],
            index_assigned: false,
        }
    }
}

impl fmt::Debug for Auth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Auth")
            .field("id", &self.id)
            .field("index", &self.index)
            .field("provider", &self.provider)
            .field("prefix", &self.prefix)
            .field("has_storage", &self.storage.is_some())
            .field("label", &self.label)
            .field("status", &self.status)
            .field("status_message_len", &self.status_message.len())
            .field("disabled", &self.disabled)
            .field("unavailable", &self.unavailable)
            .field("has_proxy", &!self.proxy_url.trim().is_empty())
            .field(
                "attribute_keys",
                &self.attributes.keys().collect::<Vec<_>>(),
            )
            .field("metadata_keys", &self.metadata.keys().collect::<Vec<_>>())
            .field("quota", &self.quota)
            .field("last_error", &self.last_error)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("last_refreshed_at", &self.last_refreshed_at)
            .field("next_refresh_after", &self.next_refresh_after)
            .field("next_retry_after", &self.next_retry_after)
            .field(
                "model_state_keys",
                &self.model_states.keys().collect::<Vec<_>>(),
            )
            .field("has_runtime", &self.runtime.is_some())
            .field("success", &self.success)
            .field("failed", &self.failed)
            .finish()
    }
}

impl Auth {
    /// Preserves process-owned state when an external source supplies a fresh
    /// durable representation of the same auth record.
    pub(crate) fn preserve_runtime_state_from(&mut self, existing: &Self) {
        if self.registration_epoch == 0 {
            self.registration_epoch = existing.registration_epoch;
        }
        if self.index.trim().is_empty() {
            self.index.clone_from(&existing.index);
            self.index_assigned = existing.index_assigned;
        }
        if self.storage.is_none() {
            self.storage.clone_from(&existing.storage);
        }
        if self.runtime.is_none() {
            self.runtime.clone_from(&existing.runtime);
        }
        self.success = existing.success;
        self.failed = existing.failed;
        self.recent_requests = existing.recent_requests;
    }

    pub fn mark_plugin_virtual(&mut self, source_path: &str, ordinal: usize) {
        self.attributes.insert(
            ATTRIBUTE_PLUGIN_VIRTUAL.to_owned(),
            PLUGIN_VIRTUAL_ENABLED.to_owned(),
        );
        let source_path = source_path.trim();
        if !source_path.is_empty() {
            self.attributes
                .insert(ATTRIBUTE_VIRTUAL_SOURCE.to_owned(), source_path.to_owned());
        }
        let seed_id = nonempty(self.id.as_str())
            .or_else(|| nonempty(self.file_name.as_str()))
            .map(str::to_owned)
            .unwrap_or_else(|| ordinal.to_string());
        self.attributes.insert(
            ATTRIBUTE_AUTH_INDEX_SEED.to_owned(),
            format!(
                "{}|{}|{}|{}",
                self.provider.trim().to_lowercase(),
                source_path,
                seed_id,
                ordinal
            ),
        );
    }

    #[must_use]
    pub fn is_plugin_virtual(&self) -> bool {
        self.attributes
            .get(ATTRIBUTE_PLUGIN_VIRTUAL)
            .is_some_and(|value| value.trim().eq_ignore_ascii_case(PLUGIN_VIRTUAL_ENABLED))
    }

    #[must_use]
    pub fn ensure_index(&mut self) -> String {
        let existing = self.index.trim();
        if !existing.is_empty() {
            let existing = existing.to_owned();
            self.index = existing.clone();
            self.index_assigned = true;
            return existing;
        }
        let Some(seed) = self.index_seed() else {
            return String::new();
        };
        let index = stable_auth_index(&seed);
        self.index = index.clone();
        self.index_assigned = true;
        index
    }

    fn index_seed(&self) -> Option<String> {
        if let Some(seed) = self
            .attributes
            .get(ATTRIBUTE_AUTH_INDEX_SEED)
            .and_then(|value| nonempty(value))
        {
            return Some(format!("{ATTRIBUTE_AUTH_INDEX_SEED}:{seed}"));
        }

        let provider = self.provider.trim().to_lowercase();
        let compat_name = attribute(self, "compat_name");
        let base_url = attribute(self, "base_url");
        let api_key = attribute(self, "api_key");
        let mut file_path = attribute(self, "path")
            .or_else(|| attribute(self, "source"))
            .map(str::to_owned)
            .or_else(|| nonempty(&self.file_name).map(str::to_owned))
            .or_else(|| nonempty(&self.id).map(str::to_owned))
            .unwrap_or_default();

        if file_path.to_lowercase().ends_with(".json") {
            file_path = absolute_clean_path(&file_path);
            let auth_type = metadata_string(self, "type")
                .unwrap_or(provider.as_str())
                .trim()
                .to_lowercase();
            if !auth_type.is_empty() {
                return Some(format!("{auth_type}:{file_path}"));
            }
        }

        if let Some(api_key) = api_key {
            let prefix =
                if compat_name.is_some() || provider.eq_ignore_ascii_case("openai-compatibility") {
                    Some("openai-compatibility")
                } else {
                    match provider.as_str() {
                        "gemini" => Some("gemini-api-key"),
                        "gemini-interactions" => Some("interactions-api-key"),
                        "codex" => Some("codex-api-key"),
                        "xai" => Some("xai-api-key"),
                        "claude" => Some("claude-api-key"),
                        _ => None,
                    }
                };
            if let Some(prefix) = prefix {
                return Some(format!(
                    "{prefix}:{}+{}",
                    base_url.unwrap_or_default().trim(),
                    api_key.trim()
                ));
            }
        }

        nonempty(&self.id).map(|id| format!("id:{id}"))
    }

    pub fn record_recent_request(&mut self, unix_seconds: i64, success: bool) {
        let bucket_id = recent_request_bucket_id(unix_seconds);
        let index = recent_request_bucket_index(bucket_id);
        let bucket = &mut self.recent_requests[index];
        if bucket.bucket_id != bucket_id {
            *bucket = RecentRequestSlot {
                bucket_id,
                ..RecentRequestSlot::default()
            };
        }
        if success {
            bucket.success += 1;
        } else {
            bucket.failed += 1;
        }
    }

    #[must_use]
    pub fn recent_requests_snapshot(&self, unix_seconds: i64) -> Vec<RecentRequestBucket> {
        let current = recent_request_bucket_id(unix_seconds);
        (0..RECENT_REQUEST_BUCKET_COUNT)
            .map(|offset| {
                let bucket_id = current - (RECENT_REQUEST_BUCKET_COUNT - 1 - offset) as i64;
                let stored = self.recent_requests[recent_request_bucket_index(bucket_id)];
                let (success, failed) = if stored.bucket_id == bucket_id {
                    (stored.success, stored.failed)
                } else {
                    (0, 0)
                };
                RecentRequestBucket {
                    time: format_recent_request_bucket_label(bucket_id),
                    success,
                    failed,
                }
            })
            .collect()
    }

    #[must_use]
    pub fn proxy_info(&self) -> String {
        let proxy = self.proxy_url.trim();
        if proxy.is_empty() {
            return String::new();
        }
        proxy
            .find("://")
            .filter(|index| *index > 0)
            .map(|index| format!("via {} proxy", &proxy[..index]))
            .unwrap_or_else(|| "via proxy".to_owned())
    }

    #[must_use]
    pub fn disable_cooling_override(&self) -> Option<bool> {
        ["disable_cooling", "disable-cooling"]
            .into_iter()
            .find_map(|key| self.metadata.get(key).and_then(parse_bool_value))
            .filter(|enabled| *enabled)
    }

    #[must_use]
    pub fn tool_prefix_disabled(&self) -> bool {
        ["tool_prefix_disabled", "tool-prefix-disabled"]
            .into_iter()
            .find_map(|key| self.metadata.get(key).and_then(parse_bool_value))
            .unwrap_or(false)
    }

    #[must_use]
    pub fn request_retry_override(&self) -> Option<u64> {
        ["request_retry", "request-retry"]
            .into_iter()
            .find_map(|key| self.metadata.get(key).and_then(parse_i64_value))
            .map(|value| value.max(0) as u64)
    }

    #[must_use]
    pub fn account_info(&self) -> (String, String) {
        match self.auth_kind() {
            Some(AuthKind::OAuth) => (
                "oauth".to_owned(),
                metadata_string(self, "email")
                    .unwrap_or_default()
                    .to_owned(),
            ),
            Some(AuthKind::ApiKey) => (
                "api_key".to_owned(),
                attribute(self, "api_key").unwrap_or_default().to_owned(),
            ),
            None => (String::new(), String::new()),
        }
    }

    /// Extracts a credential expiration timestamp using the pinned upstream
    /// key precedence and legacy nested-token compatibility rules.
    #[must_use]
    pub fn expiration_time(&self) -> Option<DateTime<Utc>> {
        expiration_from_map(&self.metadata)
    }

    #[must_use]
    pub fn refresh_lead(&self) -> Option<Duration> {
        provider_refresh_lead(&self.provider, self.runtime.as_deref())
    }
}

pub(crate) fn attribute<'a>(auth: &'a Auth, key: &str) -> Option<&'a str> {
    auth.attributes.get(key).and_then(|value| nonempty(value))
}

pub(crate) fn metadata_string<'a>(auth: &'a Auth, key: &str) -> Option<&'a str> {
    auth.metadata
        .get(key)
        .and_then(Value::as_str)
        .and_then(nonempty)
}

fn nonempty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn stable_auth_index(seed: &str) -> String {
    let digest = Sha256::digest(seed.trim().as_bytes());
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn absolute_clean_path(value: &str) -> String {
    let source = Path::new(value);
    let absolute = if source.is_absolute() {
        source.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(source)
    };
    let mut clean = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other.as_os_str()),
        }
    }
    clean.to_string_lossy().into_owned()
}

fn recent_request_bucket_id(unix_seconds: i64) -> i64 {
    unix_seconds.div_euclid(RECENT_REQUEST_BUCKET_SECONDS)
}

fn recent_request_bucket_index(bucket_id: i64) -> usize {
    bucket_id.rem_euclid(RECENT_REQUEST_BUCKET_COUNT as i64) as usize
}

fn format_recent_request_bucket_label(bucket_id: i64) -> String {
    let start_seconds = bucket_id.saturating_mul(RECENT_REQUEST_BUCKET_SECONDS);
    let Some((start_hour, start_minute)) = local_hour_minute(start_seconds) else {
        return String::new();
    };
    let Some((end_hour, end_minute)) =
        local_hour_minute(start_seconds.saturating_add(RECENT_REQUEST_BUCKET_SECONDS))
    else {
        return String::new();
    };
    format!("{start_hour:02}:{start_minute:02}-{end_hour:02}:{end_minute:02}")
}

#[cfg(unix)]
fn local_hour_minute(unix_seconds: i64) -> Option<(i32, i32)> {
    use std::mem::MaybeUninit;

    let timestamp: libc::time_t = unix_seconds;
    let mut local = MaybeUninit::<libc::tm>::uninit();
    // SAFETY: `timestamp` and `local` are valid for the duration of the call;
    // `localtime_r` initializes the caller-owned `tm` and does not retain them.
    let result = unsafe { libc::localtime_r(&timestamp, local.as_mut_ptr()) };
    if result.is_null() {
        return None;
    }
    // SAFETY: a non-null `localtime_r` result guarantees full initialization.
    let local = unsafe { local.assume_init() };
    Some((local.tm_hour, local.tm_min))
}

#[cfg(not(unix))]
fn local_hour_minute(unix_seconds: i64) -> Option<(i32, i32)> {
    let seconds = unix_seconds.rem_euclid(24 * 60 * 60);
    Some(((seconds / 3600) as i32, ((seconds % 3600) / 60) as i32))
}

fn parse_bool_value(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::String(value) => value.trim().parse().ok(),
        Value::Number(value) => value.as_i64().map(|value| value != 0),
        _ => None,
    }
}

fn parse_i64_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(value) => value.as_i64().or_else(|| value.as_f64().map(|v| v as i64)),
        Value::String(value) => value.trim().parse().ok(),
        _ => None,
    }
}

fn expiration_from_map(metadata: &BTreeMap<String, Value>) -> Option<DateTime<Utc>> {
    for key in EXPIRATION_KEYS {
        if let Some(timestamp) = metadata.get(key).and_then(parse_time_value) {
            return Some(timestamp);
        }
    }
    for key in ["token", "Token"] {
        if let Some(Value::Object(nested)) = metadata.get(key) {
            let nested = nested
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            if let Some(timestamp) = expiration_from_map(&nested) {
                return Some(timestamp);
            }
        }
    }
    None
}

pub(crate) fn parse_time_value(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::String(value) => {
            let value = value.trim();
            if value.is_empty() {
                return None;
            }
            DateTime::parse_from_rfc3339(value)
                .map(|timestamp| timestamp.with_timezone(&Utc))
                .ok()
                .or_else(|| parse_naive_utc(value, "%Y-%m-%d %H:%M:%S"))
                .or_else(|| parse_naive_utc(value, "%Y-%m-%d %H:%M"))
                .or_else(|| value.parse::<i64>().ok().and_then(normalize_unix))
        }
        Value::Number(value) => value
            .as_i64()
            .or_else(|| value.as_f64().map(|value| value as i64))
            .and_then(normalize_unix),
        _ => None,
    }
}

pub(crate) fn is_go_zero_time(value: &DateTime<Utc>) -> bool {
    *value == go_zero_time()
}

fn parse_naive_utc(value: &str, format: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(value, format)
        .ok()
        .map(|timestamp| timestamp.and_utc())
}

fn normalize_unix(raw: i64) -> Option<DateTime<Utc>> {
    if raw <= 0 {
        return Some(go_zero_time());
    }
    if raw > 1_000_000_000_000 {
        DateTime::from_timestamp_millis(raw)
    } else {
        DateTime::from_timestamp(raw, 0)
    }
}
