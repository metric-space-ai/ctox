// ref: sdk/cliproxy/auth/home_selection.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: selection owns registry scope, executor, attempts, and bound resources
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};

use crate::sdk::cliproxy::executionregistry::{RegistryError, ReleaseTicket, Scope};
use crate::sdk::pluginapi::ProviderExecutor;

use super::{canonical_home_concurrency_model_key, Auth, AuthPreparer};

const UPSTREAM_MODEL: &str = "home_upstream_model";
const FORCE_MAPPING: &str = "home_force_mapping";
const ORIGINAL_ALIAS: &str = "home_original_alias";

type CloseFn = Box<dyn FnOnce() -> Result<(), String> + Send + 'static>;

#[derive(Default)]
struct SelectionResources {
    closed: bool,
    closers: Vec<CloseFn>,
}

#[derive(Default)]
struct AttemptState {
    closed: bool,
    attempts: BTreeMap<u64, Arc<AtomicBool>>,
}

pub struct HomeAttemptLease {
    token: u64,
    cancelled: Arc<AtomicBool>,
    owner: Weak<Mutex<AttemptState>>,
    released: AtomicBool,
}

impl HomeAttemptLease {
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn release(&self) {
        if self.released.swap(true, Ordering::AcqRel) {
            return;
        }
        self.cancelled.store(true, Ordering::Release);
        if let Some(owner) = self.owner.upgrade() {
            owner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .attempts
                .remove(&self.token);
        }
    }
}

impl Drop for HomeAttemptLease {
    fn drop(&mut self) {
        self.release();
    }
}

pub struct HomeDispatchSelection {
    auth: RwLock<Auth>,
    executor: Arc<dyn ProviderExecutor>,
    auth_preparer: Option<Arc<dyn AuthPreparer>>,
    provider: String,
    scope: Scope,
    resources: Arc<Mutex<SelectionResources>>,
    attempts: Arc<Mutex<AttemptState>>,
    next_attempt: AtomicU64,
    retained: AtomicBool,
    ended: AtomicBool,
    pub(super) model_info: Option<super::home_model_capabilities::HomeDispatchModelInfo>,
}

impl fmt::Debug for HomeDispatchSelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let auth = self.clone_auth();
        formatter
            .debug_struct("HomeDispatchSelection")
            .field("auth_id", &auth.id)
            .field("provider", &self.provider)
            .field("retained", &self.retained())
            .field("active", &self.active())
            .finish_non_exhaustive()
    }
}

impl HomeDispatchSelection {
    pub fn new(
        auth: Auth,
        executor: Arc<dyn ProviderExecutor>,
        provider: &str,
        scope: Scope,
    ) -> Result<Arc<Self>, RegistryError> {
        Self::new_with_auth_preparer(auth, executor, None, provider, scope)
    }

    pub fn new_with_auth_preparer(
        auth: Auth,
        executor: Arc<dyn ProviderExecutor>,
        auth_preparer: Option<Arc<dyn AuthPreparer>>,
        provider: &str,
        scope: Scope,
    ) -> Result<Arc<Self>, RegistryError> {
        Self::new_with_model_info(auth, executor, auth_preparer, provider, scope, None)
    }

    pub(super) fn new_with_model_info(
        auth: Auth,
        executor: Arc<dyn ProviderExecutor>,
        auth_preparer: Option<Arc<dyn AuthPreparer>>,
        provider: &str,
        scope: Scope,
        model_info: Option<super::home_model_capabilities::HomeDispatchModelInfo>,
    ) -> Result<Arc<Self>, RegistryError> {
        let resources = Arc::new(Mutex::new(SelectionResources::default()));
        let attempts = Arc::new(Mutex::new(AttemptState::default()));
        let resources_for_scope = resources.clone();
        let attempts_for_scope = attempts.clone();
        scope.bind(move || {
            close_attempts(&attempts_for_scope);
            close_resources(&resources_for_scope)
        })?;
        Ok(Arc::new(Self {
            auth: RwLock::new(auth),
            executor,
            auth_preparer,
            provider: provider.trim().to_ascii_lowercase(),
            scope,
            resources,
            attempts,
            next_attempt: AtomicU64::new(0),
            retained: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            model_info,
        }))
    }

    pub fn auth(&self) -> Auth {
        self.clone_auth()
    }

    pub fn executor(&self) -> Arc<dyn ProviderExecutor> {
        self.executor.clone()
    }

    pub fn auth_preparer(&self) -> Option<Arc<dyn AuthPreparer>> {
        self.auth_preparer.clone()
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn bind<F>(&self, closer: F) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String> + Send + 'static,
    {
        let mut resources = self
            .resources
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if resources.closed {
            drop(resources);
            closer()?;
            return Err("Home dispatch selection is not accepting resources".to_owned());
        }
        resources.closers.push(Box::new(closer));
        Ok(())
    }

    pub fn attempt(&self) -> Result<HomeAttemptLease, RegistryError> {
        let mut attempts = self
            .attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if attempts.closed {
            return Err(RegistryError::NotAccepting);
        }
        let token = self.next_attempt.fetch_add(1, Ordering::Relaxed) + 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        attempts.attempts.insert(token, cancelled.clone());
        Ok(HomeAttemptLease {
            token,
            cancelled,
            owner: Arc::downgrade(&self.attempts),
            released: AtomicBool::new(false),
        })
    }

    pub fn attempt_count(&self) -> usize {
        self.attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .attempts
            .len()
    }

    pub fn retain(&self) {
        if self.active() {
            self.retained.store(true, Ordering::Release);
        }
    }

    pub fn retained(&self) -> bool {
        self.retained.load(Ordering::Acquire) && self.active()
    }

    pub fn active(&self) -> bool {
        !self.ended.load(Ordering::Acquire)
    }

    pub fn end(&self, reason: &str) {
        let _ = self.end_with_release(reason);
    }

    pub fn end_with_release(&self, reason: &str) -> Option<ReleaseTicket> {
        if self.ended.swap(true, Ordering::AcqRel) {
            return self.scope.end_with_release("");
        }
        self.scope.end_with_release(reason.trim())
    }

    pub fn clone_auth(&self) -> Auth {
        self.auth
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Atomically installs refreshed Home credentials while retaining the
    /// routing attributes attached by dispatch selection.
    pub fn replace_auth(&self, mut auth: Auth) {
        let mut current = self
            .auth
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        preserve_home_routing_attributes(&mut auth, &current);
        *current = auth;
    }

    pub fn clone_auth_for_route(&self, route_model: &str) -> Auth {
        let mut auth = self.clone_auth();
        if !self.retained() {
            return auth;
        }
        let Some(upstream) = auth.attributes.get(UPSTREAM_MODEL).cloned() else {
            return auth;
        };
        let base = canonical_home_concurrency_model_key(&upstream);
        let suffix = reasoning_suffix(route_model);
        auth.attributes
            .insert(UPSTREAM_MODEL.to_owned(), format!("{base}{suffix}"));
        if auth
            .attributes
            .get(FORCE_MAPPING)
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        {
            auth.attributes
                .insert(ORIGINAL_ALIAS.to_owned(), route_model.trim().to_owned());
        }
        auth
    }
}

fn preserve_home_routing_attributes(updated: &mut Auth, previous: &Auth) {
    for key in [UPSTREAM_MODEL, FORCE_MAPPING, ORIGINAL_ALIAS] {
        if let Some(value) = previous
            .attributes
            .get(key)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            updated.attributes.insert(key.to_owned(), value.to_owned());
        }
    }
}

fn reasoning_suffix(model: &str) -> &str {
    let model = model.trim_ascii();
    let Some(open) = model.rfind('(') else {
        return "";
    };
    if model.ends_with(')')
        && canonical_home_concurrency_model_key(model) != model.to_ascii_lowercase()
    {
        &model[open..]
    } else {
        ""
    }
}

fn close_attempts(attempts: &Mutex<AttemptState>) {
    let attempts = {
        let mut state = attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed = true;
        std::mem::take(&mut state.attempts)
    };
    for attempt in attempts.into_values() {
        attempt.store(true, Ordering::Release);
    }
}

fn close_resources(resources: &Mutex<SelectionResources>) -> Result<(), String> {
    let closers = {
        let mut state = resources
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return Ok(());
        }
        state.closed = true;
        std::mem::take(&mut state.closers)
    };
    let mut failures = Vec::new();
    for closer in closers.into_iter().rev() {
        if let Err(error) = closer() {
            failures.push(error);
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} Home execution resources failed to close",
            failures.len()
        ))
    }
}
