// ref: internal/home/client.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Home wire protocol behind host-injected transport authority.
//!
//! Upstream owns Redis pools, discovery and a process-global client. CTOX
//! already owns durable queueing/control state, so this module ports request,
//! KV, membership, fencing and error semantics without creating Redis or a
//! second control plane.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::requests::{AuthDispatchRequest, ModelsRequest, RefreshRequest};

pub const KEY_CONFIG: &str = "config";
pub const KEY_USAGE: &str = "usage";
pub const KEY_IN_FLIGHT_SNAPSHOT: &str = "in-flight-snapshot";
pub const KEY_CONCURRENCY_RELEASE: &str = "concurrency-release";
pub const KEY_REQUEST_LOG: &str = "request-log";
pub const KEY_APP_LOG: &str = "app-log";
pub const KEY_PLUGIN_STATUS: &str = "plugin-status";
pub const KEY_PLUGIN_TASKS: &str = "plugin-tasks";
pub const HOME_REFRESH_OPERATION_TIMEOUT: Duration = Duration::from_secs(35);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HomeTlsConfig {
    pub enabled: bool,
    pub ca_cert: String,
    pub client_cert: String,
    pub client_key: String,
    pub server_name: String,
    pub use_target_server_name: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HomeConfig {
    pub enabled: bool,
    pub node_id: String,
    pub host: String,
    pub port: u16,
    pub tls: HomeTlsConfig,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KvSetOptions {
    pub ex: Duration,
    pub px: Duration,
    pub nx: bool,
    pub xx: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HomeError {
    Disabled,
    NotConnected,
    EmptyResponse,
    AuthNotFound,
    ConfigNotFound,
    ModelsNotFound,
    DispatchFenced,
    CompareAndSwapUnsupported,
    InvalidRequest(String),
    Transport(String),
    AmbiguousDispatch(String),
}

impl fmt::Display for HomeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str("home client disabled"),
            Self::NotConnected => f.write_str("home not connected"),
            Self::EmptyResponse => f.write_str("home returned empty response"),
            Self::AuthNotFound => f.write_str("home auth not found"),
            Self::ConfigNotFound => f.write_str("home config not found"),
            Self::ModelsNotFound => f.write_str("home models not found"),
            Self::DispatchFenced => f.write_str("home auth dispatch is fenced"),
            Self::CompareAndSwapUnsupported => f.write_str("home compare-and-swap is unsupported"),
            Self::InvalidRequest(message)
            | Self::Transport(message)
            | Self::AmbiguousDispatch(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for HomeError {}

pub fn is_ambiguous_dispatch_error(error: &HomeError) -> bool {
    matches!(error, HomeError::AmbiguousDispatch(_))
}
pub fn is_membership_takeover_unavailable_error(error: &str) -> bool {
    matches!(
        error.trim().to_ascii_lowercase().as_str(),
        "membership_takeover_unavailable" | "err membership_takeover_unavailable"
    )
}
pub fn is_legacy_membership_protocol_error(error: &str) -> bool {
    matches!(
        error.trim().to_ascii_lowercase().as_str(),
        "wrong number of arguments for 'subscribe' command"
            | "err wrong number of arguments for 'subscribe' command"
    )
}
pub fn is_home_command_unsupported(error: &str) -> bool {
    let value = error.trim().to_ascii_lowercase();
    value.contains("unknown command") || value.contains("unsupported command")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchFailureStage {
    BeforeSend,
    AfterSend,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportFailure {
    pub stage: DispatchFailureStage,
    pub message: String,
}

/// Injected adapter implemented by the CTOX durable store/queue boundary.
pub trait HomeTransport: Send + Sync {
    fn ping(&self) -> Result<(), HomeError>;
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, HomeError>;
    fn set(&self, key: &str, value: &[u8], options: KvSetOptions) -> Result<bool, HomeError>;
    fn compare_and_swap(
        &self,
        key: &str,
        expected: Option<&[u8]>,
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, HomeError>;
    fn delete(&self, keys: &[String]) -> Result<i64, HomeError>;
    fn expire(&self, key: &str, ttl: Duration) -> Result<bool, HomeError>;
    fn ttl(&self, key: &str) -> Result<Option<Duration>, HomeError>;
    fn increment(&self, key: &str, delta: i64) -> Result<i64, HomeError>;
    fn push(&self, key: &str, payload: &[u8], right: bool) -> Result<(), HomeError>;
    fn request(&self, key: &str, payload: &[u8]) -> Result<Vec<u8>, TransportFailure>;
    /// Executes a deterministic request under a call-site-specific deadline.
    /// Implementations must enforce the supplied deadline; silently delegating
    /// to an unbounded request would break refresh liveness.
    fn request_with_timeout(
        &self,
        key: &str,
        payload: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>, TransportFailure>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginTask {
    pub id: u64,
    pub operation: String,
    pub plugin_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_node_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_node_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecoveryState {
    Stable = 0,
    TakeoverEligible = 1,
}

pub struct Client {
    config: HomeConfig,
    transport: Arc<dyn HomeTransport>,
    heartbeat_ok: AtomicBool,
    dispatch_fenced: AtomicBool,
    ambiguous_dispatch: AtomicBool,
    legacy_membership: AtomicBool,
    recovery_state: AtomicU32,
    membership_instance_id: String,
    lifecycle: Mutex<CredentialConcurrencyConfig>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("enabled", &self.enabled())
            .field("heartbeat_ok", &self.heartbeat_ok())
            .field(
                "dispatch_fenced",
                &self.dispatch_fenced.load(Ordering::Acquire),
            )
            .finish_non_exhaustive()
    }
}

impl Client {
    pub fn new(config: HomeConfig, transport: Arc<dyn HomeTransport>) -> Self {
        Self {
            config,
            transport,
            heartbeat_ok: AtomicBool::new(false),
            dispatch_fenced: AtomicBool::new(false),
            ambiguous_dispatch: AtomicBool::new(false),
            legacy_membership: AtomicBool::new(false),
            recovery_state: AtomicU32::new(RecoveryState::Stable as u32),
            membership_instance_id: Uuid::new_v4().to_string(),
            lifecycle: Mutex::new(CredentialConcurrencyConfig::default()),
        }
    }
    pub fn new_lifetime(&self) -> Self {
        let next = Self::new(self.config.clone(), Arc::clone(&self.transport));
        next.legacy_membership
            .store(self.legacy_membership(), Ordering::Release);
        next.recovery_state.store(
            self.recovery_state.load(Ordering::Acquire),
            Ordering::Release,
        );
        Self {
            membership_instance_id: self.membership_instance_id.clone(),
            ..next
        }
    }
    pub fn enabled(&self) -> bool {
        self.config.enabled
    }
    pub fn heartbeat_ok(&self) -> bool {
        self.heartbeat_ok.load(Ordering::Acquire)
    }
    pub fn set_heartbeat(&self, ready: bool) {
        self.heartbeat_ok.store(ready, Ordering::Release);
    }
    pub fn membership_instance_id(&self) -> &str {
        &self.membership_instance_id
    }
    pub fn legacy_membership(&self) -> bool {
        self.legacy_membership.load(Ordering::Acquire)
    }
    pub fn enable_legacy_membership(&self) {
        self.legacy_membership.store(true, Ordering::Release);
        self.suppress_takeover();
    }
    pub fn suppress_takeover(&self) {
        self.recovery_state
            .store(RecoveryState::Stable as u32, Ordering::Release);
    }
    pub fn mark_membership_takeover_eligible(&self) {
        if !self.legacy_membership() {
            self.recovery_state
                .store(RecoveryState::TakeoverEligible as u32, Ordering::Release);
        }
    }
    pub fn takeover_eligible(&self) -> bool {
        self.recovery_state.load(Ordering::Acquire) == RecoveryState::TakeoverEligible as u32
    }
    pub fn close(&self) {
        self.heartbeat_ok.store(false, Ordering::Release);
        self.dispatch_fenced.store(true, Ordering::Release);
    }
    pub fn abort_ambiguous_dispatch(&self) {
        self.ambiguous_dispatch.store(true, Ordering::Release);
        self.dispatch_fenced.store(true, Ordering::Release);
        self.suppress_takeover();
    }
    pub fn ambiguous_dispatch(&self) -> bool {
        self.ambiguous_dispatch.load(Ordering::Acquire)
    }
    pub fn ping(&self) -> Result<(), HomeError> {
        self.require_enabled()?;
        self.transport.ping()?;
        self.set_heartbeat(true);
        Ok(())
    }
    fn require_enabled(&self) -> Result<(), HomeError> {
        if self.enabled() {
            Ok(())
        } else {
            Err(HomeError::Disabled)
        }
    }

    pub fn get_config(&self) -> Result<Vec<u8>, HomeError> {
        self.require_enabled()?;
        self.transport
            .get(KEY_CONFIG)?
            .filter(|v| !v.is_empty())
            .ok_or(HomeError::ConfigNotFound)
    }
    pub fn get_models(
        &self,
        headers: BTreeMap<String, String>,
        query: BTreeMap<String, String>,
    ) -> Result<Vec<u8>, HomeError> {
        self.require_enabled()?;
        let payload = serde_json::to_vec(&ModelsRequest {
            request_type: "models".into(),
            headers: normalize_map(headers),
            query: normalize_map(query),
        })
        .map_err(|e| HomeError::InvalidRequest(e.to_string()))?;
        self.request_deterministic(KEY_CONFIG, &payload)?
            .into_nonempty(HomeError::ModelsNotFound)
    }
    pub fn rpop_auth(
        &self,
        model: &str,
        session_id: &str,
        headers: BTreeMap<String, String>,
        count: i32,
        credential_policy: &str,
    ) -> Result<Vec<u8>, HomeError> {
        if self.dispatch_fenced.load(Ordering::Acquire) {
            return Err(HomeError::DispatchFenced);
        }
        self.require_enabled()?;
        let payload = serde_json::to_vec(&new_auth_dispatch_request(
            model,
            session_id,
            headers,
            count,
            credential_policy,
        ))
        .map_err(|e| HomeError::InvalidRequest(e.to_string()))?;
        match self.transport.request("auth", &payload) {
            Ok(raw) => raw.into_nonempty(HomeError::AuthNotFound),
            Err(failure) if failure.stage == DispatchFailureStage::AfterSend => {
                self.abort_ambiguous_dispatch();
                Err(HomeError::AmbiguousDispatch(failure.message))
            }
            Err(failure) => Err(HomeError::Transport(failure.message)),
        }
    }
    pub fn get_refresh_auth(&self, auth_index: &str) -> Result<Vec<u8>, HomeError> {
        self.get_refresh_auth_with_fingerprint(auth_index, "")
    }

    pub fn get_refresh_auth_with_fingerprint(
        &self,
        auth_index: &str,
        access_token_sha256: &str,
    ) -> Result<Vec<u8>, HomeError> {
        let payload = serde_json::to_vec(&RefreshRequest {
            request_type: "refresh".into(),
            auth_index: auth_index.trim().into(),
            access_token_sha256: access_token_sha256.trim().into(),
        })
        .map_err(|e| HomeError::InvalidRequest(e.to_string()))?;
        self.transport
            .request_with_timeout("auth", &payload, HOME_REFRESH_OPERATION_TIMEOUT)
            .map_err(|error| HomeError::Transport(error.message))?
            .into_nonempty(HomeError::AuthNotFound)
    }
    fn request_deterministic(&self, key: &str, payload: &[u8]) -> Result<Vec<u8>, HomeError> {
        self.transport
            .request(key, payload)
            .map_err(|e| HomeError::Transport(e.message))
    }
    pub fn kv_get(&self, key: &str) -> Result<Option<Vec<u8>>, HomeError> {
        self.require_enabled()?;
        self.transport.get(key)
    }
    pub fn kv_set(
        &self,
        key: &str,
        value: &[u8],
        options: KvSetOptions,
    ) -> Result<bool, HomeError> {
        validate_set_options(options)?;
        self.transport.set(key, value, options)
    }
    pub fn kv_set_nx(&self, key: &str, value: &[u8], ttl: Duration) -> Result<bool, HomeError> {
        self.kv_set(
            key,
            value,
            KvSetOptions {
                ex: ttl,
                nx: true,
                ..KvSetOptions::default()
            },
        )
    }
    pub fn kv_compare_and_swap(
        &self,
        key: &str,
        expected: Option<&[u8]>,
        value: &[u8],
        ttl: Duration,
    ) -> Result<bool, HomeError> {
        self.transport.compare_and_swap(key, expected, value, ttl)
    }
    pub fn kv_del(&self, keys: &[String]) -> Result<i64, HomeError> {
        self.transport.delete(keys)
    }
    pub fn kv_expire(&self, key: &str, ttl: Duration) -> Result<bool, HomeError> {
        self.transport.expire(key, ttl)
    }
    pub fn kv_ttl(&self, key: &str) -> Result<Option<Duration>, HomeError> {
        self.transport.ttl(key)
    }
    pub fn kv_incr_by(&self, key: &str, delta: i64) -> Result<i64, HomeError> {
        self.transport.increment(key, delta)
    }
    pub fn push_usage(&self, payload: &[u8]) -> Result<(), HomeError> {
        self.transport.push(KEY_USAGE, payload, false)
    }
    pub fn push_in_flight_snapshot(&self, payload: &[u8]) -> Result<(), HomeError> {
        self.transport.push(KEY_IN_FLIGHT_SNAPSHOT, payload, false)
    }
    pub fn push_concurrency_release(&self, payload: &[u8]) -> Result<(), HomeError> {
        self.transport.push(KEY_CONCURRENCY_RELEASE, payload, false)
    }
    pub fn push_request_log(&self, payload: &[u8]) -> Result<(), HomeError> {
        self.transport.push(KEY_REQUEST_LOG, payload, true)
    }
    pub fn push_app_log(&self, payload: &[u8]) -> Result<(), HomeError> {
        self.transport.push(KEY_APP_LOG, payload, true)
    }
    pub fn push_plugin_status(&self, payload: &[u8]) -> Result<(), HomeError> {
        self.transport.push(KEY_PLUGIN_STATUS, payload, true)
    }
    pub fn get_plugin_tasks(&self) -> Result<Vec<PluginTask>, HomeError> {
        match self.transport.get(KEY_PLUGIN_TASKS)? {
            None => Ok(Vec::new()),
            Some(raw) => {
                serde_json::from_slice(&raw).map_err(|e| HomeError::InvalidRequest(e.to_string()))
            }
        }
    }
    pub fn set_lifecycle_config(
        &self,
        config: CredentialConcurrencyConfig,
    ) -> Result<(), HomeError> {
        config.validate()?;
        *self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = config;
        Ok(())
    }
    pub fn limiter_config(&self) -> CredentialConcurrencyConfig {
        *self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

trait NonEmpty {
    fn into_nonempty(self, error: HomeError) -> Result<Vec<u8>, HomeError>;
}
impl NonEmpty for Vec<u8> {
    fn into_nonempty(self, error: HomeError) -> Result<Vec<u8>, HomeError> {
        if self.is_empty() {
            Err(error)
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialConcurrencyConfig {
    pub protocol: i32,
    pub heartbeat_interval: Duration,
    pub release_flush_interval: Duration,
    pub release_max_backoff: Duration,
}
impl Default for CredentialConcurrencyConfig {
    fn default() -> Self {
        Self {
            protocol: 1,
            heartbeat_interval: Duration::from_secs(5),
            release_flush_interval: Duration::from_millis(250),
            release_max_backoff: Duration::from_secs(5),
        }
    }
}
impl CredentialConcurrencyConfig {
    pub fn validate(self) -> Result<(), HomeError> {
        if self.protocol <= 0
            || self.heartbeat_interval.is_zero()
            || self.release_flush_interval.is_zero()
            || self.release_max_backoff < self.release_flush_interval
        {
            Err(HomeError::InvalidRequest(
                "home credential concurrency config is invalid".into(),
            ))
        } else {
            Ok(())
        }
    }
}

pub fn new_auth_dispatch_request(
    model: &str,
    session_id: &str,
    headers: BTreeMap<String, String>,
    count: i32,
    credential_policy: &str,
) -> AuthDispatchRequest {
    AuthDispatchRequest {
        request_type: "auth".into(),
        model: model.trim().into(),
        count: count.max(1),
        concurrency_protocol: 1,
        session_id: session_id.trim().into(),
        parent_session_id: String::new(),
        node_kind: String::new(),
        headers: normalize_map(headers),
        credential_policy: credential_policy.trim().into(),
        retry_round: None,
        excluded_auth_ids: None,
        pinned_auth_id: String::new(),
    }
}
pub fn normalize_map(values: BTreeMap<String, String>) -> BTreeMap<String, String> {
    values
        .into_iter()
        .filter_map(|(k, v)| {
            let key = k.trim().to_ascii_lowercase();
            (!key.is_empty()).then_some((key, v))
        })
        .collect()
}
pub fn build_kv_set_args(
    key: &str,
    value: &[u8],
    options: KvSetOptions,
) -> Result<Vec<Value>, HomeError> {
    validate_set_options(options)?;
    let mut args = vec![
        Value::String(key.into()),
        Value::String(String::from_utf8_lossy(value).into_owned()),
    ];
    if !options.ex.is_zero() {
        args.extend([
            Value::String("EX".into()),
            Value::from(duration_ceil(options.ex, Duration::from_secs(1))),
        ]);
    }
    if !options.px.is_zero() {
        args.extend([
            Value::String("PX".into()),
            Value::from(duration_ceil(options.px, Duration::from_millis(1))),
        ]);
    }
    if options.nx {
        args.push(Value::String("NX".into()));
    }
    if options.xx {
        args.push(Value::String("XX".into()));
    }
    Ok(args)
}
fn validate_set_options(options: KvSetOptions) -> Result<(), HomeError> {
    if (!options.ex.is_zero() && !options.px.is_zero()) || (options.nx && options.xx) {
        Err(HomeError::InvalidRequest(
            "home kv set options conflict".into(),
        ))
    } else {
        Ok(())
    }
}
pub fn duration_ceil(value: Duration, unit: Duration) -> u64 {
    let unit_ns = unit.as_nanos();
    value.as_nanos().div_ceil(unit_ns) as u64
}
