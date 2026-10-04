// ref: sdk/cliproxy/auth/conductor_lifecycle.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{
    next_refresh_check_at, should_persist, Auth, AuthMutationOptions, AuthRefresher, AuthStore,
    AuthStoreError, PersistenceIntent, RefreshCancellation, RefreshCoordinator, RefreshOutcome,
    RefreshSchedule, RefreshTransactionError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthLifecycleError {
    InvalidAuthId,
    DuplicateStoreRecord,
    DurableRecordMissing,
    PersistenceClassChange,
    AuthNotRegistered,
    StaleRegistrationEpoch,
    RegistrationEpochExhausted,
    Store(AuthStoreError),
}

impl fmt::Display for AuthLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAuthId => "auth id is empty",
            Self::DuplicateStoreRecord => "auth store contains duplicate stable ids",
            Self::DurableRecordMissing => "durable auth record is missing",
            Self::PersistenceClassChange => "auth update changed persistence ownership",
            Self::AuthNotRegistered => "auth is no longer registered",
            Self::StaleRegistrationEpoch => "auth result belongs to an obsolete registration",
            Self::RegistrationEpochExhausted => "auth registration epoch is exhausted",
            Self::Store(_) => "auth lifecycle store operation failed",
        })
    }
}

impl std::error::Error for AuthLifecycleError {}

pub trait ModelResumeSink: Send + Sync {
    /// Called after durable/cache/schedule publication and after all lifecycle
    /// mutation locks are released, even when a durable refresh failure is
    /// returned to the caller.
    fn auth_published(&self, _auth_id: &str) {}

    fn resume_model(&self, auth_id: &str, model: &str);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthLifecycleRefreshError {
    Refresh(RefreshTransactionError),
    Republish {
        refresh: RefreshTransactionError,
        lifecycle: AuthLifecycleError,
    },
}

impl fmt::Display for AuthLifecycleRefreshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Refresh(_) => "auth lifecycle refresh failed",
            Self::Republish { .. } => "auth refresh failed and durable state republish failed",
        })
    }
}

impl std::error::Error for AuthLifecycleRefreshError {}

/// Active provider-neutral auth lifecycle backed by an injected CTOX store.
///
/// A global gate makes full store reload exclusive, while per-auth locks span
/// store I/O and cache publication. The cache carries runtime-only owners and
/// counters, but durable records are loaded from the store before updates and
/// are saved before they become visible in memory.
pub struct AuthLifecycle {
    store: Arc<dyn AuthStore>,
    records: RwLock<BTreeMap<String, Auth>>,
    mutation: RwLock<()>,
    auth_locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
    registration_epochs: Mutex<BTreeMap<String, u64>>,
    schedule: Arc<RefreshSchedule>,
    refresh: RefreshCoordinator,
    refresh_interval: Duration,
}

impl AuthLifecycle {
    #[must_use]
    pub fn new(
        store: Arc<dyn AuthStore>,
        schedule: Arc<RefreshSchedule>,
        refresh_interval: Duration,
    ) -> Self {
        Self {
            refresh: RefreshCoordinator::new(store.clone()),
            store,
            records: RwLock::new(BTreeMap::new()),
            mutation: RwLock::new(()),
            auth_locks: Mutex::new(BTreeMap::new()),
            registration_epochs: Mutex::new(BTreeMap::new()),
            schedule,
            refresh_interval,
        }
    }

    pub fn load(&self, now: DateTime<Utc>) -> Result<usize, AuthLifecycleError> {
        let _guard = self
            .mutation
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut loaded = BTreeMap::new();
        for mut auth in self.store.list().map_err(AuthLifecycleError::Store)? {
            super::normalize_credential_metadata(&mut auth.metadata);
            let id = auth.id.trim().to_owned();
            if id.is_empty() {
                continue;
            }
            auth.id = id.clone();
            let _ = auth.ensure_index();
            if loaded.insert(id, auth).is_some() {
                return Err(AuthLifecycleError::DuplicateStoreRecord);
            }
        }
        self.replace_records_and_schedule(loaded, now)
    }

    pub fn register(
        &self,
        mut auth: Auth,
        options: AuthMutationOptions,
        now: DateTime<Utc>,
    ) -> Result<Auth, AuthLifecycleError> {
        super::normalize_credential_metadata(&mut auth.metadata);
        auth.id = if auth.id.trim().is_empty() {
            Uuid::new_v4().to_string()
        } else {
            auth.id.trim().to_owned()
        };
        let _gate = self
            .mutation
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let auth_lock = self.lock_for(&auth.id);
        let _guard = auth_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = auth.ensure_index();
        let durable_by_class = should_persist(&auth, PersistenceIntent::Persist);
        if durable_by_class
            && options.persistence == PersistenceIntent::SourceAlreadyPersisted
            && self.load_one(&auth.id)?.is_none()
        {
            return Err(AuthLifecycleError::DurableRecordMissing);
        }
        auth.registration_epoch = self.next_registration_epoch(&auth)?;
        if should_persist(&auth, options.persistence) {
            self.store.save(&auth).map_err(AuthLifecycleError::Store)?;
        }
        self.records_write().insert(auth.id.clone(), auth.clone());
        self.reschedule(&auth, now);
        Ok(auth)
    }

    pub fn update(
        &self,
        auth: Auth,
        options: AuthMutationOptions,
        now: DateTime<Utc>,
    ) -> Result<Option<Auth>, AuthLifecycleError> {
        self.update_internal(None, auth, options, now, false)
    }

    pub fn update_prepared(
        &self,
        base: &Auth,
        auth: Auth,
        options: AuthMutationOptions,
        now: DateTime<Utc>,
    ) -> Result<Option<Auth>, AuthLifecycleError> {
        self.update_internal(Some(base), auth, options, now, false)
    }

    pub fn update_refreshed(
        &self,
        base: &Auth,
        auth: Auth,
        options: AuthMutationOptions,
        now: DateTime<Utc>,
    ) -> Result<Option<Auth>, AuthLifecycleError> {
        self.update_internal(Some(base), auth, options, now, true)
    }

    // ref: sdk/cliproxy/auth/conductor_lifecycle.go:175-278 @ d7914af
    // Validate, persist and install under one lifecycle/per-identity lock scope.
    fn update_internal(
        &self,
        base: Option<&Auth>,
        mut auth: Auth,
        options: AuthMutationOptions,
        now: DateTime<Utc>,
        refreshed: bool,
    ) -> Result<Option<Auth>, AuthLifecycleError> {
        super::normalize_credential_metadata(&mut auth.metadata);
        let id = auth.id.trim().to_owned();
        if id.is_empty() {
            return Ok(None);
        }
        if base.is_some_and(|base| base.id.trim() != id) {
            return Err(AuthLifecycleError::InvalidAuthId);
        }
        auth.id = id.clone();
        let _gate = self
            .mutation
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let auth_lock = self.lock_for(&id);
        let _guard = auth_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(cached) = self.records_read().get(&id).cloned() else {
            return Ok(None);
        };
        if base.is_some_and(|base| base.registration_epoch != cached.registration_epoch)
            || (auth.registration_epoch != 0 && auth.registration_epoch < cached.registration_epoch)
        {
            return Err(AuthLifecycleError::StaleRegistrationEpoch);
        }

        let cached_persistent = should_persist(&cached, PersistenceIntent::Persist);
        let incoming_persistent = should_persist(&auth, PersistenceIntent::Persist);
        if cached_persistent != incoming_persistent {
            return Err(AuthLifecycleError::PersistenceClassChange);
        }
        let mut existing = if cached_persistent {
            self.load_one(&id)?
                .ok_or(AuthLifecycleError::DurableRecordMissing)?
        } else {
            cached.clone()
        };

        super::normalize_credential_metadata(&mut existing.metadata);
        if let Some(base) = base {
            let mut current = existing.clone();
            current.preserve_runtime_state_from(&cached);
            current.registration_epoch = cached.registration_epoch;
            auth = if refreshed {
                super::merge_refreshed_auth(base, &current, &auth, now)
            } else {
                super::merge_prepared_auth(base, &current, &auth)
            };
        } else if auth.registration_epoch == 0 {
            auth.registration_epoch = cached.registration_epoch;
        }
        auth.preserve_runtime_state_from(&cached);
        if existing.disabled
            || existing.status == super::AuthStatus::Disabled
            || auth.disabled
            || auth.status == super::AuthStatus::Disabled
        {
            // Disabled transitions deliberately do not resurrect stale
            // per-model cooldown state.
        } else if auth.model_states.is_empty() && !existing.model_states.is_empty() {
            auth.model_states = existing.model_states;
        }
        let _ = auth.ensure_index();

        if should_persist(&auth, options.persistence) {
            self.store.save(&auth).map_err(AuthLifecycleError::Store)?;
        }
        self.records_write().insert(id, auth.clone());
        self.reschedule(&auth, now);
        Ok(Some(auth))
    }

    pub fn refresh(
        &self,
        id: &str,
        failed_access_token: Option<&str>,
        now: DateTime<Utc>,
        refresher: &dyn AuthRefresher,
        resume_sink: &dyn ModelResumeSink,
    ) -> Result<RefreshOutcome, AuthLifecycleRefreshError> {
        self.refresh_with_cancellation(
            id,
            failed_access_token,
            now,
            refresher,
            resume_sink,
            &RefreshCancellation::default(),
        )
    }

    pub fn refresh_with_cancellation(
        &self,
        id: &str,
        failed_access_token: Option<&str>,
        now: DateTime<Utc>,
        refresher: &dyn AuthRefresher,
        resume_sink: &dyn ModelResumeSink,
        cancellation: &RefreshCancellation,
    ) -> Result<RefreshOutcome, AuthLifecycleRefreshError> {
        let id = id.trim();
        if id.is_empty() {
            return Err(AuthLifecycleRefreshError::Refresh(
                RefreshTransactionError::InvalidAuthId,
            ));
        }
        let gate = self
            .mutation
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let auth_lock = self.lock_for(id);
        let guard = auth_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let cached = self.records_read().get(id).cloned();
        match self.refresh.refresh_with_cancellation(
            id,
            failed_access_token,
            now,
            refresher,
            cancellation,
        ) {
            Ok(mut outcome) => {
                if let Some(cached) = &cached {
                    outcome.auth.preserve_runtime_state_from(cached);
                }
                self.records_write()
                    .insert(outcome.auth.id.clone(), outcome.auth.clone());
                self.reschedule(&outcome.auth, now);
                let auth_id = outcome.auth.id.clone();
                let resumed_models = outcome.resumed_models.clone();
                drop(guard);
                drop(gate);
                resume_sink.auth_published(&auth_id);
                for model in resumed_models {
                    resume_sink.resume_model(&auth_id, &model);
                }
                Ok(outcome)
            }
            Err(refresh_error @ RefreshTransactionError::Refresh(_)) => {
                let mut persisted = self
                    .load_one(id)
                    .map_err(|lifecycle| AuthLifecycleRefreshError::Republish {
                        refresh: refresh_error.clone(),
                        lifecycle,
                    })?
                    .ok_or_else(|| AuthLifecycleRefreshError::Republish {
                        refresh: refresh_error.clone(),
                        lifecycle: AuthLifecycleError::DurableRecordMissing,
                    })?;
                if let Some(cached) = &cached {
                    persisted.preserve_runtime_state_from(cached);
                }
                self.records_write()
                    .insert(persisted.id.clone(), persisted.clone());
                self.reschedule(&persisted, now);
                let auth_id = persisted.id;
                drop(guard);
                drop(gate);
                resume_sink.auth_published(&auth_id);
                Err(AuthLifecycleRefreshError::Refresh(refresh_error))
            }
            Err(refresh_error) => Err(AuthLifecycleRefreshError::Refresh(refresh_error)),
        }
    }

    /// Mirrors upstream `Remove`: runtime state and scheduling are removed,
    /// while the owning durable source remains untouched.
    pub fn remove_runtime(&self, id: &str) -> bool {
        let id = id.trim();
        if id.is_empty() {
            return false;
        }
        let _gate = self
            .mutation
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let auth_lock = self.lock_for(id);
        let _guard = auth_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let removed = self.records_write().remove(id).is_some();
        if removed {
            self.schedule.remove(id);
        }
        removed
    }

    /// Explicit owning-store deletion; kept separate from runtime removal so a
    /// late watcher event cannot accidentally delete subscription credentials.
    pub fn delete(&self, id: &str) -> Result<bool, AuthLifecycleError> {
        let id = id.trim();
        if id.is_empty() {
            return Err(AuthLifecycleError::InvalidAuthId);
        }
        let _gate = self
            .mutation
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let auth_lock = self.lock_for(id);
        let _guard = auth_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.records_read().contains_key(id) {
            return Ok(false);
        }
        self.store.delete(id).map_err(AuthLifecycleError::Store)?;
        self.records_write().remove(id);
        self.schedule.remove(id);
        Ok(true)
    }

    #[must_use]
    pub fn get_cached(&self, id: &str) -> Option<Auth> {
        self.records_read().get(id.trim()).cloned()
    }

    /// Records process-local request telemetry without rewriting the owning
    /// credential source. Durable availability is maintained separately by
    /// `CooldownStateStore`; these counters mirror upstream's runtime-only
    /// recent-request accounting.
    pub(crate) fn record_execution_outcome(
        &self,
        id: &str,
        observed_at: DateTime<Utc>,
        success: bool,
    ) -> bool {
        let id = id.trim();
        if id.is_empty() {
            return false;
        }
        let _gate = self
            .mutation
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let auth_lock = self.lock_for(id);
        let _guard = auth_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut records = self.records_write();
        let Some(auth) = records.get_mut(id) else {
            return false;
        };
        if success {
            auth.success = auth.success.saturating_add(1);
        } else {
            auth.failed = auth.failed.saturating_add(1);
        }
        auth.record_recent_request(observed_at.timestamp(), success);
        true
    }

    /// Returns a stable clone snapshot for derived runtime views. Credentials
    /// remain owned by this lifecycle; callers must project only the fields
    /// their view requires.
    #[must_use]
    pub fn snapshot_cached(&self) -> Vec<Auth> {
        self.records_read().values().cloned().collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records_read().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn load_one(&self, id: &str) -> Result<Option<Auth>, AuthLifecycleError> {
        let mut found = None;
        for auth in self
            .store
            .list()
            .map_err(AuthLifecycleError::Store)?
            .into_iter()
            .filter(|auth| auth.id == id)
        {
            if found.replace(auth).is_some() {
                return Err(AuthLifecycleError::DuplicateStoreRecord);
            }
        }
        Ok(found)
    }

    fn lock_for(&self, id: &str) -> Arc<Mutex<()>> {
        self.auth_locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(id.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn replace_records_and_schedule(
        &self,
        mut loaded: BTreeMap<String, Auth>,
        now: DateTime<Utc>,
    ) -> Result<usize, AuthLifecycleError> {
        let old_ids = self.records_read().keys().cloned().collect::<BTreeSet<_>>();
        for auth in loaded.values_mut() {
            auth.registration_epoch = self.next_registration_epoch(auth)?;
        }
        for id in old_ids {
            self.schedule.remove(&id);
        }
        for auth in loaded.values() {
            self.reschedule(auth, now);
        }
        let len = loaded.len();
        *self.records_write() = loaded;
        Ok(len)
    }

    fn next_registration_epoch(&self, auth: &Auth) -> Result<u64, AuthLifecycleError> {
        let cached_epoch = self
            .records_read()
            .get(&auth.id)
            .map_or(0, |auth| auth.registration_epoch);
        let mut epochs = self
            .registration_epochs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = epochs.entry(auth.id.clone()).or_default();
        let next = (*previous)
            .max(cached_epoch)
            .max(auth.registration_epoch)
            .checked_add(1)
            .ok_or(AuthLifecycleError::RegistrationEpochExhausted)?;
        *previous = next;
        Ok(next)
    }

    fn reschedule(&self, auth: &Auth, now: DateTime<Utc>) {
        self.schedule.remove(&auth.id);
        if let Some(due) = next_refresh_check_at(now, auth, self.refresh_interval) {
            self.schedule.upsert(&auth.id, due);
        }
    }

    fn records_read(&self) -> std::sync::RwLockReadGuard<'_, BTreeMap<String, Auth>> {
        self.records
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn records_write(&self) -> std::sync::RwLockWriteGuard<'_, BTreeMap<String, Auth>> {
        self.records
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "conductor_lifecycle_candidate_test.rs"]
mod candidate_tests;

impl fmt::Debug for AuthLifecycle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthLifecycle")
            .field("records", &self.len())
            .field("scheduled", &self.schedule.len())
            .field("refresh_interval", &self.refresh_interval)
            .finish_non_exhaustive()
    }
}
