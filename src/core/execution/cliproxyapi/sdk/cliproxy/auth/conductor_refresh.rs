// ref: sdk/cliproxy/auth/conductor_refresh.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::store::{AuthStore, AuthStoreError};
use super::types::{go_zero_time, is_go_zero_time, parse_time_value};
use super::{
    access_token_sha256, Auth, AuthError, AuthKind, AuthStatus, HomeAuthRuntime,
    HomeDispatchSelection, ModelState,
};

pub const REFRESH_CHECK_INTERVAL: Duration = Duration::from_secs(5);
pub const REFRESH_MAX_CONCURRENCY: usize = 16;
pub const REFRESH_PENDING_BACKOFF: Duration = Duration::from_secs(60);
pub const REFRESH_FAILURE_BACKOFF: Duration = Duration::from_secs(5 * 60);
pub const REFRESH_INEFFECTIVE_BACKOFF: Duration = Duration::from_secs(30);

const REFRESH_INTERVAL_KEYS: [&str; 4] = [
    "refresh_interval_seconds",
    "refreshIntervalSeconds",
    "refresh_interval",
    "refreshInterval",
];
const LAST_REFRESH_KEYS: [&str; 4] = [
    "last_refresh",
    "lastRefresh",
    "last_refreshed_at",
    "lastRefreshedAt",
];

#[must_use]
pub fn should_refresh(auth: &Auth, now: DateTime<Utc>) -> bool {
    if has_unauthorized_auth_failure(auth) {
        return false;
    }
    if !is_go_zero_time(&auth.next_refresh_after) && now < auth.next_refresh_after {
        return false;
    }
    if let Some(runtime) = auth
        .runtime
        .as_deref()
        .filter(|runtime| runtime.evaluates_refresh())
    {
        return runtime.should_refresh(now, auth);
    }

    let last_refresh = last_refresh_time(auth);
    let expiration = auth
        .expiration_time()
        .filter(|expiration| !is_go_zero_time(expiration));

    if let Some(interval) = preferred_refresh_interval(auth) {
        if let Some(expiration) = expiration {
            if expiration <= now || within(expiration, now, interval) {
                return true;
            }
        }
        let Some(last_refresh) = last_refresh else {
            return true;
        };
        return elapsed_at_least(now, last_refresh, interval);
    }

    let Some(lead) = auth.refresh_lead() else {
        return false;
    };
    if let Some(expiration) = expiration {
        return expiration <= now || within(expiration, now, lead);
    }
    if let Some(last_refresh) = last_refresh {
        return elapsed_at_least(now, last_refresh, lead);
    }
    true
}

#[must_use]
pub fn preferred_refresh_interval(auth: &Auth) -> Option<Duration> {
    REFRESH_INTERVAL_KEYS
        .into_iter()
        .find_map(|key| auth.metadata.get(key).and_then(duration_from_json))
        .or_else(|| {
            REFRESH_INTERVAL_KEYS.into_iter().find_map(|key| {
                auth.attributes
                    .get(key)
                    .and_then(|value| parse_duration(value))
            })
        })
}

#[must_use]
pub fn last_refresh_time(auth: &Auth) -> Option<DateTime<Utc>> {
    if !is_go_zero_time(&auth.last_refreshed_at) {
        return Some(auth.last_refreshed_at);
    }
    LAST_REFRESH_KEYS
        .into_iter()
        .find_map(|key| auth.metadata.get(key).and_then(parse_time_value))
        .or_else(|| {
            LAST_REFRESH_KEYS.into_iter().find_map(|key| {
                auth.attributes
                    .get(key)
                    .map(|value| Value::String(value.trim().to_owned()))
                    .as_ref()
                    .and_then(parse_time_value)
            })
        })
        .filter(|timestamp| !is_go_zero_time(timestamp))
}

#[must_use]
pub fn has_unauthorized_auth_failure(auth: &Auth) -> bool {
    auth.last_error.as_ref().is_some_and(|error| {
        error.http_status == 401 || error.code.trim().eq_ignore_ascii_case("unauthorized")
    })
}

#[must_use]
pub fn access_token(auth: &Auth) -> Option<&str> {
    ["access_token", "accessToken"]
        .into_iter()
        .find_map(|key| metadata_nonempty_string(auth, key))
}

#[must_use]
pub fn has_refresh_credential(auth: &Auth) -> bool {
    ["refresh_token", "refreshToken"]
        .into_iter()
        .any(|key| metadata_nonempty_string(auth, key).is_some())
}

fn metadata_nonempty_string<'a>(auth: &'a Auth, key: &str) -> Option<&'a str> {
    auth.metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn duration_from_json(value: &Value) -> Option<Duration> {
    match value {
        Value::Number(value) => value
            .as_i64()
            .map(|seconds| seconds as f64)
            .or_else(|| value.as_u64().map(|seconds| seconds as f64))
            .or_else(|| value.as_f64())
            .and_then(duration_from_seconds),
        Value::String(value) => parse_duration(value),
        _ => None,
    }
}

fn duration_from_seconds(seconds: f64) -> Option<Duration> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    let nanos = seconds * 1_000_000_000.0;
    if !nanos.is_finite() {
        return None;
    }
    Some(Duration::from_nanos((nanos as u64).min(i64::MAX as u64)))
}

#[must_use]
pub fn parse_duration(raw: &str) -> Option<Duration> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    parse_go_duration(value).or_else(|| value.parse::<f64>().ok().and_then(duration_from_seconds))
}

fn parse_go_duration(value: &str) -> Option<Duration> {
    let value = value.strip_prefix('+').unwrap_or(value);
    if value.is_empty() || value.starts_with('-') {
        return None;
    }
    let bytes = value.as_bytes();
    let mut cursor = 0;
    let mut total_nanos = 0_u128;
    while cursor < bytes.len() {
        let integer_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        let integer_digits = &value[integer_start..cursor];
        let mut fraction_digits = "";
        if cursor < bytes.len() && bytes[cursor] == b'.' {
            cursor += 1;
            let fraction_start = cursor;
            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                cursor += 1;
            }
            fraction_digits = &value[fraction_start..cursor];
        }
        if integer_digits.is_empty() && fraction_digits.is_empty() {
            return None;
        }
        let remaining = &value[cursor..];
        let (unit, unit_nanos) = [
            ("ns", 1_u128),
            ("us", 1_000),
            ("µs", 1_000),
            ("μs", 1_000),
            ("ms", 1_000_000),
            ("s", 1_000_000_000),
            ("m", 60 * 1_000_000_000),
            ("h", 60 * 60 * 1_000_000_000),
        ]
        .into_iter()
        .find(|(unit, _)| remaining.starts_with(unit))?;
        cursor += unit.len();

        let integer = if integer_digits.is_empty() {
            0
        } else {
            integer_digits.parse::<u128>().ok()?
        };
        let mut component = integer.checked_mul(unit_nanos)?;
        if !fraction_digits.is_empty() {
            let (fraction, scale) = leading_fraction(fraction_digits);
            // Go intentionally uses float64 here to keep fractional hours
            // nanosecond-accurate; its rounding is observable for long tails.
            let fraction_nanos = (fraction as f64 * (unit_nanos as f64 / scale)) as u128;
            component = component.checked_add(fraction_nanos)?;
        }
        total_nanos = total_nanos.checked_add(component)?;
        if total_nanos > i64::MAX as u128 {
            return None;
        }
    }
    (total_nanos > 0).then(|| Duration::from_nanos(total_nanos as u64))
}

fn leading_fraction(digits: &str) -> (u64, f64) {
    let mut value = 0_u64;
    let mut scale = 1_f64;
    let mut overflow = false;
    for digit in digits.bytes().map(|digit| u64::from(digit - b'0')) {
        if overflow {
            continue;
        }
        if value > ((1_u64 << 63) - 1) / 10 {
            overflow = true;
            continue;
        }
        let next = value * 10 + digit;
        if next > 1_u64 << 63 {
            overflow = true;
            continue;
        }
        value = next;
        scale *= 10.0;
    }
    (value, scale)
}

fn within(later: DateTime<Utc>, earlier: DateTime<Utc>, duration: Duration) -> bool {
    later
        .signed_duration_since(earlier)
        .to_std()
        .is_ok_and(|remaining| remaining <= duration)
}

fn elapsed_at_least(now: DateTime<Utc>, earlier: DateTime<Utc>, duration: Duration) -> bool {
    now.signed_duration_since(earlier)
        .to_std()
        .is_ok_and(|elapsed| elapsed >= duration)
}

#[derive(Default)]
struct RefreshCancellationState {
    cancelled: AtomicBool,
    wake: tokio::sync::Notify,
}

#[derive(Clone, Default)]
pub struct RefreshCancellation(Arc<RefreshCancellationState>);

impl RefreshCancellation {
    pub fn cancel(&self) {
        if !self.0.cancelled.swap(true, Ordering::AcqRel) {
            self.0.wake.notify_waiters();
        }
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    /// Wakes all active refresh awaits, and also completes when cancellation
    /// preceded registration. Enable before checking to prevent a lost wake.
    pub async fn cancelled(&self) {
        let notified = self.0.wake.notified();
        tokio::pin!(notified);
        let _ = notified.as_mut().enable();
        if self.is_cancelled() {
            return;
        }
        notified.await;
    }
}

impl std::fmt::Debug for RefreshCancellation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RefreshCancellation")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

pub trait AuthRefresher: Send + Sync {
    /// Receives an owned clone through a mutable reference. Returning `None`
    /// preserves mutations made in place, matching Go's nil-success behavior.
    fn refresh(&self, auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError>;

    fn refresh_with_cancellation(
        &self,
        auth: &mut Auth,
        cancellation: &RefreshCancellation,
    ) -> Result<Option<Auth>, RefreshExecutorError> {
        if cancellation.is_cancelled() {
            return Err(RefreshExecutorError::Cancelled);
        }
        self.refresh(auth)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefreshExecutorError {
    Cancelled,
    Failed(AuthError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HomeRefreshError {
    ProviderUnavailable,
    Cancelled,
    Failed(AuthError),
}

impl std::fmt::Display for HomeRefreshError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ProviderUnavailable => "Home credential refresher is unavailable",
            Self::Cancelled => "Home credential refresh was cancelled",
            Self::Failed(_) => "Home credential refresh failed",
        })
    }
}

impl std::error::Error for HomeRefreshError {}

impl HomeAuthRuntime {
    /// Refreshes the exact ephemeral selection that observed a 401. If another
    /// attempt already installed a newer token, that snapshot is reused rather
    /// than refreshing it a second time.
    pub fn refresh_home_selection_after_unauthorized(
        &self,
        selection: &HomeDispatchSelection,
        failed_auth: &Auth,
    ) -> Result<Option<Auth>, HomeRefreshError> {
        let current = selection.clone_auth();
        let current_fingerprint = access_token_sha256(&current);
        let failed_fingerprint = access_token_sha256(failed_auth);
        if current.id == failed_auth.id
            && !current_fingerprint.is_empty()
            && !failed_fingerprint.is_empty()
            && current_fingerprint != failed_fingerprint
        {
            return Ok(Some(current));
        }
        if failed_auth.auth_kind() != Some(AuthKind::OAuth) {
            return Ok(None);
        }
        let registration = self
            .manager()
            .executors()
            .get(&failed_auth.provider)
            .ok_or(HomeRefreshError::ProviderUnavailable)?;
        let refresher = registration.refresher();
        let mut target = failed_auth.clone();
        let refreshed = match refresher.refresh(&mut target) {
            Ok(Some(refreshed)) => refreshed,
            Ok(None) => target,
            Err(RefreshExecutorError::Cancelled) => return Err(HomeRefreshError::Cancelled),
            Err(RefreshExecutorError::Failed(error)) => {
                return Err(HomeRefreshError::Failed(error))
            }
        };
        let mut refreshed = refreshed;
        if refreshed.id.trim().is_empty() {
            refreshed.id.clone_from(&failed_auth.id);
        }
        if refreshed.index.trim().is_empty() {
            refreshed.index.clone_from(&failed_auth.index);
        }
        if refreshed.provider.trim().is_empty() {
            refreshed.provider.clone_from(&failed_auth.provider);
        }
        if refreshed.runtime.is_none() {
            refreshed.runtime.clone_from(&failed_auth.runtime);
        }
        selection.replace_auth(refreshed);
        Ok(Some(selection.clone_auth()))
    }
}

#[derive(Clone, Debug)]
pub struct RefreshOutcome {
    pub auth: Auth,
    pub coalesced: bool,
    pub ineffective: bool,
    pub resumed_models: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefreshTransactionError {
    InvalidAuthId,
    AuthNotFound,
    NotRefreshable,
    Cancelled,
    InvalidRefreshedIdentity,
    Store(AuthStoreError),
    Refresh(AuthError),
    RefreshAndStore {
        refresh: AuthError,
        store: AuthStoreError,
    },
}

impl std::fmt::Display for RefreshTransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAuthId => "auth id is empty",
            Self::AuthNotFound => "auth not found",
            Self::NotRefreshable => "auth has no refresh credential",
            Self::Cancelled => "credential refresh cancelled",
            Self::InvalidRefreshedIdentity => "refreshed auth changed stable identity",
            Self::Store(_) => "credential refresh store operation failed",
            Self::Refresh(_) => "credential refresh failed",
            Self::RefreshAndStore { .. } => "credential refresh and failure persistence failed",
        })
    }
}

impl std::error::Error for RefreshTransactionError {}

/// Serializes refreshes per stable auth ID and treats the injected store as the
/// authoritative state before and after every transition.
pub struct RefreshCoordinator {
    store: Arc<dyn AuthStore>,
    locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
}

impl RefreshCoordinator {
    pub fn new(store: Arc<dyn AuthStore>) -> Self {
        Self {
            store,
            locks: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn refresh(
        &self,
        id: &str,
        failed_access_token: Option<&str>,
        now: DateTime<Utc>,
        refresher: &dyn AuthRefresher,
    ) -> Result<RefreshOutcome, RefreshTransactionError> {
        self.refresh_with_cancellation(
            id,
            failed_access_token,
            now,
            refresher,
            &RefreshCancellation::default(),
        )
    }

    pub fn refresh_with_cancellation(
        &self,
        id: &str,
        failed_access_token: Option<&str>,
        now: DateTime<Utc>,
        refresher: &dyn AuthRefresher,
        cancellation: &RefreshCancellation,
    ) -> Result<RefreshOutcome, RefreshTransactionError> {
        let id = id.trim();
        if id.is_empty() {
            return Err(RefreshTransactionError::InvalidAuthId);
        }
        let auth_lock = self.lock_for(id);
        let _guard = auth_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let auth = self.load_one(id)?;

        if let Some(failed_access_token) = failed_access_token
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            if !has_refresh_credential(&auth) {
                return Err(RefreshTransactionError::NotRefreshable);
            }
            if access_token(&auth).is_some_and(|current| current != failed_access_token) {
                return Ok(RefreshOutcome {
                    auth,
                    coalesced: true,
                    ineffective: false,
                    resumed_models: Vec::new(),
                });
            }
        }

        if cancellation.is_cancelled() {
            return Err(RefreshTransactionError::Cancelled);
        }
        let mut cloned = auth.clone();
        let result = refresher.refresh_with_cancellation(&mut cloned, cancellation);
        // A provider may finish with a candidate or an error while shutdown is
        // requested. Neither result may publish after the cancellation signal.
        if cancellation.is_cancelled() {
            return Err(RefreshTransactionError::Cancelled);
        }
        let mut updated = match result {
            Err(RefreshExecutorError::Cancelled) => return Err(RefreshTransactionError::Cancelled),
            Err(RefreshExecutorError::Failed(error)) => {
                return self.persist_failure(auth, error, now);
            }
            Ok(updated) => updated.unwrap_or(cloned),
        };

        if updated.id != auth.id {
            return Err(RefreshTransactionError::InvalidRefreshedIdentity);
        }
        if updated.runtime.is_none() {
            updated.runtime = auth.runtime;
        }
        updated.last_refreshed_at = now;
        updated.next_refresh_after = go_zero_time();
        updated.last_error = None;
        updated.status_message.clear();
        updated.unavailable = false;
        if updated.status == AuthStatus::Error {
            updated.status = AuthStatus::Active;
        }
        updated.updated_at = now;
        let resumed_models = clear_unauthorized_model_states(&mut updated, now);
        let ineffective = should_refresh(&updated, now);
        if ineffective {
            updated.next_refresh_after = add_std(now, REFRESH_INEFFECTIVE_BACKOFF)
                .ok_or(RefreshTransactionError::InvalidRefreshedIdentity)?;
        }
        if cancellation.is_cancelled() {
            return Err(RefreshTransactionError::Cancelled);
        }
        self.store
            .save(&updated)
            .map_err(RefreshTransactionError::Store)?;
        Ok(RefreshOutcome {
            auth: updated,
            coalesced: false,
            ineffective,
            resumed_models,
        })
    }

    fn persist_failure(
        &self,
        mut auth: Auth,
        error: AuthError,
        now: DateTime<Utc>,
    ) -> Result<RefreshOutcome, RefreshTransactionError> {
        let unauthorized =
            error.http_status == 401 || error.code.trim().eq_ignore_ascii_case("unauthorized");
        let normalized = AuthError {
            code: if unauthorized {
                "unauthorized".to_owned()
            } else {
                String::new()
            },
            message: error.message,
            retryable: false,
            http_status: error.http_status,
        };
        auth.last_error = Some(normalized.clone());
        auth.updated_at = now;
        if unauthorized {
            auth.next_refresh_after = go_zero_time();
            auth.unavailable = true;
            auth.status = AuthStatus::Error;
            auth.status_message = "unauthorized".to_owned();
        } else {
            auth.next_refresh_after = add_std(now, REFRESH_FAILURE_BACKOFF)
                .ok_or(RefreshTransactionError::InvalidRefreshedIdentity)?;
        }
        match self.store.save(&auth) {
            Ok(_) => Err(RefreshTransactionError::Refresh(normalized)),
            Err(store) => Err(RefreshTransactionError::RefreshAndStore {
                refresh: normalized,
                store,
            }),
        }
    }

    fn lock_for(&self, id: &str) -> Arc<Mutex<()>> {
        self.locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(id.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn load_one(&self, id: &str) -> Result<Auth, RefreshTransactionError> {
        let mut matches = self
            .store
            .list()
            .map_err(RefreshTransactionError::Store)?
            .into_iter()
            .filter(|auth| auth.id == id);
        let auth = matches
            .next()
            .ok_or(RefreshTransactionError::AuthNotFound)?;
        if matches.next().is_some() {
            return Err(RefreshTransactionError::Store(
                AuthStoreError::InvalidRecord,
            ));
        }
        Ok(auth)
    }
}

impl std::fmt::Debug for RefreshCoordinator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RefreshCoordinator")
            .field("store", &"AuthStore")
            .field(
                "auth_lock_count",
                &self
                    .locks
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .len(),
            )
            .finish()
    }
}

fn add_std(timestamp: DateTime<Utc>, duration: Duration) -> Option<DateTime<Utc>> {
    chrono::Duration::from_std(duration)
        .ok()
        .and_then(|duration| timestamp.checked_add_signed(duration))
}

fn clear_unauthorized_model_states(auth: &mut Auth, now: DateTime<Utc>) -> Vec<String> {
    let mut resumed = Vec::new();
    for (model, state) in &mut auth.model_states {
        let unauthorized = state.last_error.as_ref().is_some_and(|error| {
            error.http_status == 401 || error.code.trim().eq_ignore_ascii_case("unauthorized")
        });
        if unauthorized {
            reset_model_state(state, now);
            resumed.push(model.clone());
        }
    }
    if !resumed.is_empty() {
        update_aggregated_availability(auth, now);
    }
    resumed
}

fn reset_model_state(state: &mut ModelState, now: DateTime<Utc>) {
    state.unavailable = false;
    state.status = AuthStatus::Active;
    state.status_message.clear();
    state.next_retry_after = go_zero_time();
    state.last_error = None;
    state.quota.clear_cooldown();
    state.updated_at = now;
}

fn update_aggregated_availability(auth: &mut Auth, now: DateTime<Utc>) {
    if auth.model_states.is_empty() {
        clear_aggregated_availability(auth);
        return;
    }
    let mut all_unavailable = true;
    let mut earliest_retry = None;
    let mut quota_exceeded = false;
    let mut quota_recover = None;
    let mut max_backoff_level = 0;

    for state in auth.model_states.values_mut() {
        let state_unavailable = if state.status == AuthStatus::Disabled {
            true
        } else if state.unavailable {
            if is_go_zero_time(&state.next_retry_after) {
                false
            } else if state.next_retry_after > now {
                earliest_retry = Some(
                    earliest_retry.map_or(state.next_retry_after, |current: DateTime<Utc>| {
                        current.min(state.next_retry_after)
                    }),
                );
                true
            } else {
                state.unavailable = false;
                state.next_retry_after = go_zero_time();
                false
            }
        } else {
            false
        };
        if !state_unavailable {
            all_unavailable = false;
        }
        if state.quota.exceeded {
            quota_exceeded = true;
            if !is_go_zero_time(&state.quota.next_recover_at) {
                quota_recover = Some(
                    quota_recover.map_or(state.quota.next_recover_at, |current: DateTime<Utc>| {
                        current.min(state.quota.next_recover_at)
                    }),
                );
            }
            max_backoff_level = max_backoff_level.max(state.quota.backoff_level);
        }
    }
    auth.unavailable = all_unavailable;
    auth.next_retry_after = if all_unavailable {
        earliest_retry.unwrap_or_else(go_zero_time)
    } else {
        go_zero_time()
    };
    if quota_exceeded {
        auth.quota.exceeded = true;
        auth.quota.reason = "quota".to_owned();
        auth.quota.next_recover_at = quota_recover.unwrap_or_else(go_zero_time);
        auth.quota.backoff_level = max_backoff_level;
    } else {
        auth.quota.clear_cooldown();
    }
}

fn clear_aggregated_availability(auth: &mut Auth) {
    auth.unavailable = false;
    auth.next_retry_after = go_zero_time();
    auth.quota.clear_cooldown();
}

#[cfg(test)]
#[path = "quota_observation_test.rs"]
mod quota_observation_test;
