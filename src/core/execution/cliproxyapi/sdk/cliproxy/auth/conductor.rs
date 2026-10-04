// ref: sdk/cliproxy/auth/conductor.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};

use chrono::{DateTime, Utc};

use crate::internal::config::ProviderCompatConfig;
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamResponse, PluginExecutionError, ProviderExecutor as AsyncProviderExecutor,
};

use super::{
    AccountCandidate, Auth, AuthLifecycle, AuthLifecycleError, AuthMutationOptions, AuthRefresher,
    AuthRefresherResolver, AuthSchedulerView, ModelResumeSink, SchedulerViewError,
};

/// Sentinel passed to an executor when every retained execution session must
/// be released.
pub const CLOSE_ALL_EXECUTION_SESSIONS_ID: &str = "__all_execution_sessions__";

/// Optional capability for providers that retain websocket or other
/// execution-session resources.
pub trait ExecutionSessionCloser: Send + Sync {
    fn close_execution_session(&self, session_id: &str);
}

/// Provider-owned, request-time preparation of a selected auth snapshot.
///
/// The Home conductor publishes the snapshot atomically only after this
/// future succeeds, so a failed preparation cannot expose partially populated
/// credentials to an executor.
pub trait AuthPreparer: Send + Sync {
    fn should_prepare(&self, _auth: &Auth) -> bool {
        true
    }

    fn prepare<'a>(
        &'a self,
        auth: &'a mut Auth,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuthPreparationError>> + Send + 'a>>;
}

pub type AuthPreparationError = Arc<dyn Error + Send + Sync + 'static>;

/// Type-safe Rust replacement for Go's runtime interface assertion from
/// `ProviderExecutor` to `ExecutionSessionCloser`.
///
/// The full request-execution surface remains implemented by the provider
/// transports. This registration is the manager-facing capability slice used
/// by credential refresh and session ownership; optional behavior is explicit
/// instead of recovered through downcasting.
pub struct ProviderExecutorRegistration {
    provider: String,
    refresher: Arc<dyn AuthRefresher>,
    execution: Option<Arc<dyn AsyncProviderExecutor>>,
    auth_preparer: Option<Arc<dyn AuthPreparer>>,
    session_closer: Option<Arc<dyn ExecutionSessionCloser>>,
}

impl ProviderExecutorRegistration {
    #[must_use]
    pub fn new(provider: &str, refresher: Arc<dyn AuthRefresher>) -> Option<Self> {
        let provider = normalize_provider(provider)?;
        Some(Self {
            provider,
            refresher,
            execution: None,
            auth_preparer: None,
            session_closer: None,
        })
    }

    pub fn with_execution(
        mut self,
        execution: Arc<dyn AsyncProviderExecutor>,
    ) -> Result<Self, ProviderExecutorRegistrationError> {
        if normalize_provider(execution.identifier()).as_deref() != Some(self.provider.as_str()) {
            return Err(ProviderExecutorRegistrationError::ProviderMismatch);
        }
        self.execution = Some(execution);
        Ok(self)
    }

    #[must_use]
    pub fn with_auth_preparer(mut self, preparer: Arc<dyn AuthPreparer>) -> Self {
        self.auth_preparer = Some(preparer);
        self
    }

    #[must_use]
    pub fn with_session_closer(mut self, closer: Arc<dyn ExecutionSessionCloser>) -> Self {
        self.session_closer = Some(closer);
        self
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn refresher(&self) -> Arc<dyn AuthRefresher> {
        self.refresher.clone()
    }

    #[must_use]
    pub fn execution(&self) -> Option<Arc<dyn AsyncProviderExecutor>> {
        self.execution.clone()
    }

    #[must_use]
    pub fn auth_preparer(&self) -> Option<Arc<dyn AuthPreparer>> {
        self.auth_preparer.clone()
    }

    #[must_use]
    pub fn session_closer(&self) -> Option<Arc<dyn ExecutionSessionCloser>> {
        self.session_closer.clone()
    }

    fn same_capabilities(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.refresher, &other.refresher)
            && match (&self.execution, &other.execution) {
                (None, None) => true,
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                _ => false,
            }
            && match (&self.auth_preparer, &other.auth_preparer) {
                (None, None) => true,
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                _ => false,
            }
            && match (&self.session_closer, &other.session_closer) {
                (None, None) => true,
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                _ => false,
            }
    }
}

impl fmt::Debug for ProviderExecutorRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderExecutorRegistration")
            .field("provider", &self.provider)
            .field("has_execution", &self.execution.is_some())
            .field("has_auth_preparer", &self.auth_preparer.is_some())
            .field("has_session_closer", &self.session_closer.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderExecutorRegistrationError {
    ProviderMismatch,
}

impl fmt::Display for ProviderExecutorRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("executor identifier does not match registration provider")
    }
}

impl std::error::Error for ProviderExecutorRegistrationError {}

/// Concurrent provider registry used by manager assembly and auto refresh.
/// Replacement cleanup deliberately runs after releasing the registry lock so
/// a provider callback cannot deadlock a lookup or another registration.
#[derive(Default)]
pub struct ProviderExecutorRegistry {
    registrations: RwLock<BTreeMap<String, Arc<ProviderExecutorRegistration>>>,
}

impl ProviderExecutorRegistry {
    /// Registers a provider and returns whether it replaced different
    /// capabilities. Re-registering the same capability arcs is idempotent.
    pub fn register(&self, registration: Arc<ProviderExecutorRegistration>) -> bool {
        let provider = registration.provider.clone();
        let replaced = self
            .registrations
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(provider, registration.clone());
        let Some(replaced) = replaced else {
            return false;
        };
        if replaced.same_capabilities(&registration) {
            return false;
        }
        if let Some(closer) = replaced.session_closer() {
            closer.close_execution_session(CLOSE_ALL_EXECUTION_SESSIONS_ID);
        }
        true
    }

    #[must_use]
    pub fn get(&self, provider: &str) -> Option<Arc<ProviderExecutorRegistration>> {
        let provider = normalize_provider(provider)?;
        self.registrations
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&provider)
            .cloned()
    }

    /// Mirrors upstream unregister semantics: removing the routing entry does
    /// not implicitly close a still externally owned executor.
    pub fn unregister(&self, provider: &str) -> Option<Arc<ProviderExecutorRegistration>> {
        let provider = normalize_provider(provider)?;
        self.registrations
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&provider)
    }

    pub fn close_all_sessions(&self, provider: &str) -> bool {
        let Some(closer) = self.get(provider).and_then(|entry| entry.session_closer()) else {
            return false;
        };
        closer.close_execution_session(CLOSE_ALL_EXECUTION_SESSIONS_ID);
        true
    }

    pub async fn execute(
        &self,
        provider: &str,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, ProviderDispatchError> {
        self.execution_for(provider)?
            .execute(request)
            .await
            .map_err(ProviderDispatchError::Provider)
    }

    pub async fn execute_stream(
        &self,
        provider: &str,
        request: ExecutorRequest,
    ) -> Result<ExecutorStreamResponse, ProviderDispatchError> {
        self.execution_for(provider)?
            .execute_stream(request)
            .await
            .map_err(ProviderDispatchError::Provider)
    }

    pub async fn count_tokens(
        &self,
        provider: &str,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, ProviderDispatchError> {
        self.execution_for(provider)?
            .count_tokens(request)
            .await
            .map_err(ProviderDispatchError::Provider)
    }

    pub async fn http_request(
        &self,
        provider: &str,
        request: ExecutorHttpRequest,
    ) -> Result<ExecutorHttpResponse, ProviderDispatchError> {
        self.execution_for(provider)?
            .http_request(request)
            .await
            .map_err(ProviderDispatchError::Provider)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.registrations
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn execution_for(
        &self,
        provider: &str,
    ) -> Result<Arc<dyn AsyncProviderExecutor>, ProviderDispatchError> {
        let registration = self
            .get(provider)
            .ok_or(ProviderDispatchError::ProviderNotRegistered)?;
        registration
            .execution()
            .ok_or(ProviderDispatchError::ExecutionUnavailable)
    }
}

pub enum ProviderDispatchError {
    ProviderNotRegistered,
    ExecutionUnavailable,
    Provider(PluginExecutionError),
}

impl fmt::Debug for ProviderDispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ProviderNotRegistered => "ProviderDispatchError::ProviderNotRegistered",
            Self::ExecutionUnavailable => "ProviderDispatchError::ExecutionUnavailable",
            Self::Provider(_) => "ProviderDispatchError::Provider([REDACTED])",
        })
    }
}

impl fmt::Display for ProviderDispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ProviderNotRegistered => "provider executor is not registered",
            Self::ExecutionUnavailable => "provider execution capability is unavailable",
            Self::Provider(_) => "provider execution failed",
        })
    }
}

impl std::error::Error for ProviderDispatchError {}

impl AuthRefresherResolver for ProviderExecutorRegistry {
    fn resolve(&self, provider: &str) -> Option<Arc<dyn AuthRefresher>> {
        self.get(provider).map(|entry| entry.refresher())
    }
}

impl fmt::Debug for ProviderExecutorRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderExecutorRegistry")
            .field("providers", &self.len())
            .finish_non_exhaustive()
    }
}

fn normalize_provider(provider: &str) -> Option<String> {
    let provider = provider.trim().to_ascii_lowercase();
    (!provider.is_empty()).then_some(provider)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthManagerError {
    Lifecycle(AuthLifecycleError),
    Scheduler(SchedulerViewError),
}

impl fmt::Display for AuthManagerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Lifecycle(_) => "auth manager lifecycle operation failed",
            Self::Scheduler(_) => "auth manager scheduler publication failed",
        })
    }
}

impl std::error::Error for AuthManagerError {}

impl From<AuthLifecycleError> for AuthManagerError {
    fn from(error: AuthLifecycleError) -> Self {
        Self::Lifecycle(error)
    }
}

impl From<SchedulerViewError> for AuthManagerError {
    fn from(error: SchedulerViewError) -> Self {
        Self::Scheduler(error)
    }
}

/// Manager assembly for compound Auth, routing-view and provider-session
/// mutations. The assembly lock establishes publication order across the three
/// independently thread-safe owners; provider callbacks always run after it is
/// released.
pub struct AuthManager {
    pub(super) lifecycle: Arc<AuthLifecycle>,
    executors: Arc<ProviderExecutorRegistry>,
    scheduler: Arc<AuthSchedulerView>,
    mutation: Mutex<()>,
    pub(super) api_key_config: RwLock<Arc<ProviderCompatConfig>>,
    pub(super) api_key_model_routing: RwLock<Arc<super::ApiKeyModelRoutingSnapshot>>,
}

impl AuthManager {
    #[must_use]
    pub fn new(
        lifecycle: Arc<AuthLifecycle>,
        executors: Arc<ProviderExecutorRegistry>,
        scheduler: Arc<AuthSchedulerView>,
    ) -> Self {
        Self {
            lifecycle,
            executors,
            scheduler,
            mutation: Mutex::new(()),
            api_key_config: RwLock::new(Arc::new(ProviderCompatConfig::default())),
            api_key_model_routing: RwLock::new(Arc::new(
                super::ApiKeyModelRoutingSnapshot::default(),
            )),
        }
    }

    pub fn load(&self, now: DateTime<Utc>) -> Result<usize, AuthManagerError> {
        let _guard = self.lock_mutation();
        let loaded = self.lifecycle.load(now)?;
        if let Err(error) = self.scheduler.refresh_all() {
            // Lifecycle authority has changed. Keeping candidates derived from
            // its previous generation would be unsafe, so fail closed.
            self.scheduler.clear();
            return Err(error.into());
        }
        self.rebuild_api_key_model_routing();
        Ok(loaded)
    }

    pub fn register(
        &self,
        auth: Auth,
        options: AuthMutationOptions,
        now: DateTime<Utc>,
    ) -> Result<Auth, AuthManagerError> {
        let _guard = self.lock_mutation();
        let registered = self.lifecycle.register(auth, options, now)?;
        if let Err(error) = self.scheduler.refresh_entry(&registered.id) {
            self.scheduler.remove(&registered.id);
            return Err(error.into());
        }
        self.rebuild_api_key_model_routing();
        Ok(registered)
    }

    pub fn update(
        &self,
        auth: Auth,
        options: AuthMutationOptions,
        now: DateTime<Utc>,
    ) -> Result<Option<Auth>, AuthManagerError> {
        self.publish_auth_update(|| self.lifecycle.update(auth, options, now))
    }

    pub fn update_prepared_auth(
        &self,
        base: &Auth,
        auth: Auth,
        options: AuthMutationOptions,
        now: DateTime<Utc>,
    ) -> Result<Option<Auth>, AuthManagerError> {
        self.publish_auth_update(|| self.lifecycle.update_prepared(base, auth, options, now))
    }

    pub fn update_refreshed_auth(
        &self,
        base: &Auth,
        auth: Auth,
        options: AuthMutationOptions,
        now: DateTime<Utc>,
    ) -> Result<Option<Auth>, AuthManagerError> {
        self.publish_auth_update(|| self.lifecycle.update_refreshed(base, auth, options, now))
    }

    fn publish_auth_update(
        &self,
        operation: impl FnOnce() -> Result<Option<Auth>, super::AuthLifecycleError>,
    ) -> Result<Option<Auth>, AuthManagerError> {
        let _guard = self.lock_mutation();
        let updated = operation()?;
        if let Some(updated) = &updated {
            if let Err(error) = self.scheduler.refresh_entry(&updated.id) {
                self.scheduler.remove(&updated.id);
                return Err(error.into());
            }
            self.rebuild_api_key_model_routing();
        }
        Ok(updated)
    }

    pub fn refresh_scheduler_entry(&self, auth_id: &str) -> Result<bool, AuthManagerError> {
        let _guard = self.lock_mutation();
        match self.scheduler.refresh_entry(auth_id) {
            Ok(schedulable) => Ok(schedulable),
            Err(error) => {
                self.scheduler.remove(auth_id);
                Err(error.into())
            }
        }
    }

    pub fn refresh_scheduler_all(&self) -> Result<usize, AuthManagerError> {
        let _guard = self.lock_mutation();
        self.scheduler.refresh_all().map_err(Into::into)
    }

    /// Runtime removal leaves the owning store intact and closes retained
    /// provider sessions only after lifecycle and routing state are absent.
    pub fn remove_runtime(&self, auth_id: &str) -> bool {
        let guard = self.lock_mutation();
        let provider = self.lifecycle.get_cached(auth_id).map(|auth| auth.provider);
        let removed = self.lifecycle.remove_runtime(auth_id);
        if removed {
            self.scheduler.remove(auth_id);
            self.rebuild_api_key_model_routing();
        }
        drop(guard);
        if removed {
            if let Some(provider) = provider {
                self.executors.close_all_sessions(&provider);
            }
        }
        removed
    }

    /// Explicit owning-store deletion with the same publication and session
    /// cleanup order as runtime removal.
    pub fn delete(&self, auth_id: &str) -> Result<bool, AuthManagerError> {
        let guard = self.lock_mutation();
        let provider = self.lifecycle.get_cached(auth_id).map(|auth| auth.provider);
        let removed = self.lifecycle.delete(auth_id)?;
        if removed {
            self.scheduler.remove(auth_id);
            self.rebuild_api_key_model_routing();
        }
        drop(guard);
        if removed {
            if let Some(provider) = provider {
                self.executors.close_all_sessions(&provider);
            }
        }
        Ok(removed)
    }

    pub fn register_executor(&self, registration: Arc<ProviderExecutorRegistration>) -> bool {
        self.executors.register(registration)
    }

    #[must_use]
    pub fn candidates(&self) -> Vec<AccountCandidate> {
        self.scheduler.snapshot()
    }

    #[must_use]
    pub fn available_providers(&self) -> Vec<String> {
        self.scheduler
            .snapshot()
            .into_iter()
            .map(|candidate| candidate.provider)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    #[must_use]
    pub fn has_provider_auth(&self, provider: &str) -> bool {
        let provider = provider.trim();
        !provider.is_empty()
            && self
                .scheduler
                .snapshot()
                .iter()
                .any(|candidate| candidate.provider.eq_ignore_ascii_case(provider))
    }

    #[must_use]
    pub fn lifecycle(&self) -> Arc<AuthLifecycle> {
        self.lifecycle.clone()
    }

    #[must_use]
    pub fn executors(&self) -> Arc<ProviderExecutorRegistry> {
        self.executors.clone()
    }

    /// Adapts lifecycle/auto-refresh completion into manager-owned routing
    /// publication while preserving the caller's model-resumption sink.
    #[must_use]
    pub fn refresh_publication_sink(
        self: &Arc<Self>,
        downstream: Arc<dyn ModelResumeSink>,
    ) -> Arc<ManagerRefreshPublicationSink> {
        Arc::new(ManagerRefreshPublicationSink {
            manager: Arc::downgrade(self),
            downstream,
            publication_failures: AtomicUsize::new(0),
        })
    }

    pub(super) fn lock_mutation(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Lock-free completion bridge used by `AutoRefreshWorker`. Lifecycle invokes
/// it only after releasing its mutation locks; the bridge may therefore enter
/// the Manager assembly and fail closed without a lock cycle.
pub struct ManagerRefreshPublicationSink {
    manager: Weak<AuthManager>,
    downstream: Arc<dyn ModelResumeSink>,
    publication_failures: AtomicUsize,
}

impl ManagerRefreshPublicationSink {
    #[must_use]
    pub fn publication_failures(&self) -> usize {
        self.publication_failures.load(Ordering::Relaxed)
    }
}

impl ModelResumeSink for ManagerRefreshPublicationSink {
    fn auth_published(&self, auth_id: &str) {
        if let Some(manager) = self.manager.upgrade() {
            if manager.refresh_scheduler_entry(auth_id).is_err() {
                self.publication_failures.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.downstream.auth_published(auth_id);
    }

    fn resume_model(&self, auth_id: &str, model: &str) {
        self.downstream.resume_model(auth_id, model);
    }
}

impl fmt::Debug for ManagerRefreshPublicationSink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagerRefreshPublicationSink")
            .field("manager_alive", &self.manager.strong_count().gt(&0))
            .field("publication_failures", &self.publication_failures())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for AuthManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthManager")
            .field("auths", &self.lifecycle.len())
            .field("candidates", &self.scheduler.len())
            .field("executors", &self.executors.len())
            .finish_non_exhaustive()
    }
}

/// Provider-neutral one-refresh replay budget.
///
/// This is deliberately smaller than upstream's full account scheduler. It is
/// the accepted core needed to prove one provider path before cooldown,
/// selection and multi-account state are ported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnauthorizedReplayState {
    attempts: u8,
    refreshes: u8,
}

impl Default for UnauthorizedReplayState {
    fn default() -> Self {
        Self {
            attempts: 1,
            refreshes: 0,
        }
    }
}

impl UnauthorizedReplayState {
    pub fn observe(
        &mut self,
        status: u16,
        credential_is_refreshable: bool,
    ) -> UnauthorizedReplayDecision {
        if status == 401 && credential_is_refreshable && self.refreshes == 0 {
            self.refreshes = 1;
            self.attempts = self.attempts.saturating_add(1);
            UnauthorizedReplayDecision::RefreshAndReplay
        } else {
            UnauthorizedReplayDecision::Return
        }
    }

    pub fn attempts(&self) -> u8 {
        self.attempts
    }

    pub fn refreshed(&self) -> bool {
        self.refreshes != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnauthorizedReplayDecision {
    Return,
    RefreshAndReplay,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_refreshable_unauthorized_response_replays() {
        let mut state = UnauthorizedReplayState::default();
        assert_eq!(
            state.observe(401, true),
            UnauthorizedReplayDecision::RefreshAndReplay
        );
        assert_eq!(state.observe(401, true), UnauthorizedReplayDecision::Return);
        assert_eq!(state.attempts(), 2);
        assert!(state.refreshed());
    }

    #[test]
    fn success_server_errors_and_static_credentials_do_not_replay() {
        for (status, refreshable) in [(200, true), (500, true), (401, false)] {
            let mut state = UnauthorizedReplayState::default();
            assert_eq!(
                state.observe(status, refreshable),
                UnauthorizedReplayDecision::Return
            );
            assert_eq!(state.attempts(), 1);
            assert!(!state.refreshed());
        }
    }
}
