// ref: sdk/cliproxy/auth/conductor_cooldown.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use super::cooldown_state::{
    CooldownErrorState, CooldownQuotaState, CooldownStateRecord, CooldownStateStore,
    CooldownStoreError,
};

const QUOTA_BACKOFF_BASE_MS: i64 = 1_000;
const QUOTA_BACKOFF_MAX_MS: i64 = 30 * 60 * 1_000;
const TRANSIENT_COOLDOWN_MS: i64 = 60 * 1_000;
const AUTH_FAILURE_COOLDOWN_MS: i64 = 30 * 60 * 1_000;
const NOT_FOUND_COOLDOWN_MS: i64 = 12 * 60 * 60 * 1_000;

/// One provider/account/model execution result at a deterministic wall-clock instant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountExecutionResult {
    pub provider: String,
    pub auth_id: String,
    pub model: Option<String>,
    pub status: u16,
    /// Provider-supplied relative retry hint. This is deliberately distinct
    /// from the persisted absolute deadline.
    pub retry_delay_ms: Option<u64>,
    pub observed_at_ms: i64,
}

/// CTOX adaptation of upstream's in-manager cooldown mutations. Serializes
/// load/transition/save so the injected durable snapshot store never sees a lost
/// update from concurrent requests within one proxy process.
pub type CooldownRecordFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool, CooldownStoreError>> + Send>>;
pub type CooldownRecordDispatcher = Arc<dyn Fn(Arc<CooldownConductor>, AccountExecutionResult) -> CooldownRecordFuture + Send + Sync>;
pub struct CooldownConductor {
    store: Arc<dyn CooldownStateStore>,
    transition_lock: Mutex<()>,
    record_dispatcher: Option<CooldownRecordDispatcher>,
}

impl CooldownConductor {
    pub fn new(store: Arc<dyn CooldownStateStore>) -> Self {
        Self {
            store,
            transition_lock: Mutex::new(()),
            record_dispatcher: None,
        }
    }

    /// The native owner can supply its bounded IO executor. The portable
    /// default remains spawn_blocking; no Store authority is manufactured here.
    pub fn with_record_dispatcher(mut self, dispatcher: CooldownRecordDispatcher) -> Self {
        self.record_dispatcher = Some(dispatcher);
        self
    }
    pub async fn record_async(self: &Arc<Self>, result: AccountExecutionResult) -> Result<bool, CooldownStoreError> {
        if let Some(dispatch) = &self.record_dispatcher {
            return dispatch(self.clone(), result).await;
        }
        let conductor = self.clone();
        tokio::task::spawn_blocking(move || conductor.record(result)).await
            .map_err(|_| CooldownStoreError::Write)?
    }

    pub fn record(&self, result: AccountExecutionResult) -> Result<bool, CooldownStoreError> {
        let provider = result.provider.trim();
        let auth_id = result.auth_id.trim();
        let model = result
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if provider.is_empty() || auth_id.is_empty() {
            return Err(CooldownStoreError::InvalidRecord);
        }

        let _guard = self
            .transition_lock
            .lock()
            .map_err(|_| CooldownStoreError::Write)?;
        let mut records = self.store.load()?;
        let previous = records
            .iter()
            .filter(|record| same_scope(record, provider, auth_id, model))
            .max_by_key(|record| record.updated_at_ms)
            .cloned();

        let transition = transition_for(&result, previous.as_ref());
        let Some(replacement) = transition else {
            return Ok(false);
        };
        records.retain(|record| !same_scope(record, provider, auth_id, model));
        if let Some(record) = replacement {
            record.validate()?;
            records.push(record);
        }
        records.sort_by(|left, right| {
            (&left.provider, &left.auth_id, &left.model).cmp(&(
                &right.provider,
                &right.auth_id,
                &right.model,
            ))
        });
        self.store.save(&records)?;
        Ok(true)
    }

    /// Clears every persisted account- and model-scoped cooldown for one
    /// stable auth identity under the same writer lock as request outcomes.
    pub fn reset_account(&self, auth_id: &str) -> Result<Vec<String>, CooldownStoreError> {
        let auth_id = auth_id.trim();
        if auth_id.is_empty() {
            return Err(CooldownStoreError::InvalidRecord);
        }
        let _guard = self
            .transition_lock
            .lock()
            .map_err(|_| CooldownStoreError::Write)?;
        let mut records = self.store.load()?;
        let mut models = records
            .iter()
            .filter(|record| record.auth_id.trim() == auth_id)
            .filter_map(|record| record.model.clone())
            .collect::<Vec<_>>();
        let previous_len = records.len();
        records.retain(|record| record.auth_id.trim() != auth_id);
        if records.len() != previous_len {
            self.store.save(&records)?;
        }
        models.sort();
        models.dedup();
        Ok(models)
    }
}

impl std::fmt::Debug for CooldownConductor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CooldownConductor")
            .field("store", &"CooldownStateStore")
            .finish_non_exhaustive()
    }
}

fn same_scope(
    record: &CooldownStateRecord,
    provider: &str,
    auth_id: &str,
    model: Option<&str>,
) -> bool {
    record.provider.trim().eq_ignore_ascii_case(provider)
        && record.auth_id.trim() == auth_id
        && record
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            == model
}

/// `None` means the result is request-scoped and does not mutate account
/// state. `Some(None)` clears an existing cooldown; `Some(Some(record))`
/// replaces it.
fn transition_for(
    result: &AccountExecutionResult,
    previous: Option<&CooldownStateRecord>,
) -> Option<Option<CooldownStateRecord>> {
    if (200..300).contains(&result.status) {
        return Some(None);
    }

    let (reason, retry_after_ms, quota) = match result.status {
        401 => (
            "unauthorized",
            result
                .observed_at_ms
                .saturating_add(AUTH_FAILURE_COOLDOWN_MS),
            CooldownQuotaState::default(),
        ),
        402 | 403 => (
            "payment_required",
            result
                .observed_at_ms
                .saturating_add(AUTH_FAILURE_COOLDOWN_MS),
            CooldownQuotaState::default(),
        ),
        404 => (
            "not_found",
            result.observed_at_ms.saturating_add(NOT_FOUND_COOLDOWN_MS),
            CooldownQuotaState::default(),
        ),
        429 => {
            let (deadline, backoff_level) = quota_deadline(result, previous);
            (
                "quota",
                deadline,
                CooldownQuotaState {
                    exceeded: true,
                    reason: "quota".to_owned(),
                    next_recover_at_ms: Some(deadline),
                    backoff_level,
                },
            )
        }
        408 | 500 | 502 | 503 | 504 => (
            "transient_upstream_error",
            result.observed_at_ms.saturating_add(TRANSIENT_COOLDOWN_MS),
            CooldownQuotaState::default(),
        ),
        // Upstream classifies invalid request shapes before account outcome
        // mutation. Preserve the existing account state for those responses.
        400 | 422 => return None,
        _ => return None,
    };

    Some(Some(CooldownStateRecord {
        provider: result.provider.trim().to_owned(),
        auth_id: result.auth_id.trim().to_owned(),
        model: result
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        status: "cooling".to_owned(),
        next_retry_after_ms: Some(retry_after_ms),
        reason: reason.to_owned(),
        quota,
        last_error: Some(CooldownErrorState {
            code: format!("http_{}", result.status),
            message: String::new(),
            retryable: matches!(result.status, 408 | 429 | 500 | 502 | 503 | 504),
            http_status: Some(result.status),
        }),
        updated_at_ms: result.observed_at_ms,
    }))
}

fn quota_deadline(
    result: &AccountExecutionResult,
    previous: Option<&CooldownStateRecord>,
) -> (i64, u32) {
    if let Some(delay) = result.retry_delay_ms {
        let delay = i64::try_from(delay).unwrap_or(i64::MAX);
        return (
            result.observed_at_ms.saturating_add(delay),
            previous.map_or(0, |record| record.quota.backoff_level),
        );
    }
    if let Some(record) = previous {
        if let Some(deadline) = record
            .blocking_until_ms()
            .filter(|deadline| *deadline > result.observed_at_ms)
        {
            return (deadline, record.quota.backoff_level);
        }
    }

    let level = previous.map_or(0, |record| record.quota.backoff_level);
    let shift = level.min(62);
    let delay = QUOTA_BACKOFF_BASE_MS
        .checked_mul(1_i64 << shift)
        .unwrap_or(i64::MAX)
        .clamp(QUOTA_BACKOFF_BASE_MS, QUOTA_BACKOFF_MAX_MS);
    let next_level = if delay >= QUOTA_BACKOFF_MAX_MS {
        level
    } else {
        level.saturating_add(1)
    };
    (result.observed_at_ms.saturating_add(delay), next_level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MemoryStore(Mutex<Vec<CooldownStateRecord>>);

    impl CooldownStateStore for MemoryStore {
        fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn save(&self, records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
            *self.0.lock().unwrap() = records.to_vec();
            Ok(())
        }
    }

    fn result(
        status: u16,
        retry_delay_ms: Option<u64>,
        observed_at_ms: i64,
    ) -> AccountExecutionResult {
        AccountExecutionResult {
            provider: "claude".to_owned(),
            auth_id: "account-a".to_owned(),
            model: Some("sonnet".to_owned()),
            status,
            retry_delay_ms,
            observed_at_ms,
        }
    }

    #[test]
    fn explicit_quota_retry_is_persisted_without_response_body() {
        let store = Arc::new(MemoryStore::default());
        let conductor = CooldownConductor::new(store.clone());
        assert!(conductor.record(result(429, Some(7_000), 10_000)).unwrap());
        let record = &store.0.lock().unwrap()[0];
        assert_eq!(record.next_retry_after_ms, Some(17_000));
        assert_eq!(record.quota.next_recover_at_ms, Some(17_000));
        assert_eq!(record.quota.backoff_level, 0);
        assert_eq!(record.last_error.as_ref().unwrap().message, "");
    }

    #[test]
    fn fallback_quota_backoff_reuses_open_window_then_escalates() {
        let store = Arc::new(MemoryStore::default());
        let conductor = CooldownConductor::new(store.clone());
        conductor.record(result(429, None, 10_000)).unwrap();
        assert_eq!(store.0.lock().unwrap()[0].next_retry_after_ms, Some(11_000));
        assert_eq!(store.0.lock().unwrap()[0].quota.backoff_level, 1);

        conductor.record(result(429, None, 10_500)).unwrap();
        assert_eq!(store.0.lock().unwrap()[0].next_retry_after_ms, Some(11_000));
        assert_eq!(store.0.lock().unwrap()[0].quota.backoff_level, 1);

        conductor.record(result(429, None, 11_000)).unwrap();
        assert_eq!(store.0.lock().unwrap()[0].next_retry_after_ms, Some(13_000));
        assert_eq!(store.0.lock().unwrap()[0].quota.backoff_level, 2);
    }

    #[test]
    fn success_clears_only_the_matching_account_model_scope() {
        let store = Arc::new(MemoryStore::default());
        let conductor = CooldownConductor::new(store.clone());
        conductor.record(result(429, Some(7_000), 10_000)).unwrap();
        let mut other = result(429, Some(8_000), 10_000);
        other.model = Some("opus".to_owned());
        conductor.record(other).unwrap();
        conductor.record(result(200, None, 11_000)).unwrap();
        let records = store.0.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model.as_deref(), Some("opus"));
    }

    #[test]
    fn request_shape_error_does_not_penalize_account() {
        let store = Arc::new(MemoryStore::default());
        let conductor = CooldownConductor::new(store.clone());
        conductor.record(result(429, Some(7_000), 10_000)).unwrap();
        assert!(!conductor.record(result(400, None, 11_000)).unwrap());
        assert_eq!(store.0.lock().unwrap()[0].next_retry_after_ms, Some(17_000));
    }

    #[test]
    fn terminal_auth_and_transient_failures_use_upstream_windows() {
        let store = Arc::new(MemoryStore::default());
        let conductor = CooldownConductor::new(store.clone());
        conductor.record(result(401, None, 10_000)).unwrap();
        assert_eq!(
            store.0.lock().unwrap()[0].next_retry_after_ms,
            Some(10_000 + AUTH_FAILURE_COOLDOWN_MS)
        );
        conductor.record(result(503, None, 20_000)).unwrap();
        assert_eq!(
            store.0.lock().unwrap()[0].next_retry_after_ms,
            Some(20_000 + TRANSIENT_COOLDOWN_MS)
        );
    }
}
