// ref: internal/registry/model_registry.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Utc};
use serde_json::{Map, Number, Value};

use super::RegistryModelInfo;
use super::{lookup_static_registry_model_info, ModelCatalogStore, StaticModelsCatalog};

pub const OPENAI_IMAGE_MODEL_TYPE: &str = "openai-image";
pub const DEFAULT_CLAUDE_MAX_INPUT_TOKENS: usize = 200_000;
pub const DEFAULT_CLAUDE_MAX_OUTPUT_TOKENS: usize = 64_000;
pub const MODEL_QUOTA_EXCEEDED_WINDOW: Duration = Duration::from_secs(5 * 60);
pub const DEFAULT_MODEL_REGISTRY_HOOK_TIMEOUT: Duration = Duration::from_secs(5);

pub trait RegistryClock: Send + Sync {
    fn now(&self) -> SystemTime;
}

#[derive(Default)]
pub struct SystemRegistryClock;

impl RegistryClock for SystemRegistryClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

#[derive(Clone, Debug)]
pub struct HookContext {
    deadline: SystemTime,
}

impl HookContext {
    pub fn deadline(&self) -> SystemTime {
        self.deadline
    }

    pub fn is_expired(&self) -> bool {
        SystemTime::now() >= self.deadline
    }
}

pub trait ModelRegistryHook: Send + Sync + 'static {
    fn on_models_registered(
        &self,
        context: HookContext,
        provider: &str,
        client_id: &str,
        models: Vec<RegistryModelInfo>,
    );
    fn on_models_unregistered(&self, context: HookContext, provider: &str, client_id: &str);
}

#[derive(Clone)]
struct AvailableModelsCacheEntry {
    models: Vec<Map<String, Value>>,
    expires_at: Option<SystemTime>,
}

#[derive(Clone)]
struct ModelRegistration {
    info: RegistryModelInfo,
    info_by_provider: HashMap<String, RegistryModelInfo>,
    count: usize,
    last_updated: SystemTime,
    quota_exceeded_clients: HashMap<String, SystemTime>,
    providers: HashMap<String, usize>,
    suspended_clients: HashMap<String, String>,
}

#[derive(Default)]
struct RegistryState {
    models: HashMap<String, ModelRegistration>,
    client_models: HashMap<String, Vec<String>>,
    client_model_infos: HashMap<String, HashMap<String, RegistryModelInfo>>,
    client_providers: HashMap<String, String>,
    available_models_cache: HashMap<String, AvailableModelsCacheEntry>,
    hook: Option<Arc<dyn ModelRegistryHook>>,
}

/// Instance-owned replacement for upstream's package-global registry. Each
/// CTOX harness or gateway host injects its own owner and static catalog.
pub struct ModelRegistry {
    state: Mutex<RegistryState>,
    clock: Arc<dyn RegistryClock>,
    static_catalog: CatalogSource,
}

enum CatalogSource {
    Fixed(Arc<StaticModelsCatalog>),
    Refreshable(Arc<ModelCatalogStore>),
}

impl CatalogSource {
    fn snapshot(&self) -> Arc<StaticModelsCatalog> {
        match self {
            Self::Fixed(catalog) => Arc::clone(catalog),
            Self::Refreshable(store) => store.snapshot().catalog,
        }
    }
}

impl ModelRegistry {
    pub fn new(static_catalog: Arc<StaticModelsCatalog>) -> Self {
        Self::with_clock(static_catalog, Arc::new(SystemRegistryClock))
    }

    pub fn with_clock(
        static_catalog: Arc<StaticModelsCatalog>,
        clock: Arc<dyn RegistryClock>,
    ) -> Self {
        Self {
            state: Mutex::new(RegistryState::default()),
            clock,
            static_catalog: CatalogSource::Fixed(static_catalog),
        }
    }

    pub fn from_store(store: Arc<ModelCatalogStore>) -> Self {
        Self::from_store_with_clock(store, Arc::new(SystemRegistryClock))
    }

    pub fn from_store_with_clock(
        store: Arc<ModelCatalogStore>,
        clock: Arc<dyn RegistryClock>,
    ) -> Self {
        Self {
            state: Mutex::new(RegistryState::default()),
            clock,
            static_catalog: CatalogSource::Refreshable(store),
        }
    }

    pub fn set_hook(&self, hook: Option<Arc<dyn ModelRegistryHook>>) {
        lock_unpoisoned(&self.state).hook = hook;
    }

    pub fn register_client(
        &self,
        client_id: &str,
        client_provider: &str,
        models: &[RegistryModelInfo],
    ) {
        let provider = client_provider.to_ascii_lowercase();
        let raw_ids = models
            .iter()
            .filter(|model| !model.id.is_empty())
            .map(|model| model.id.clone())
            .collect::<Vec<_>>();
        let unique_infos = models
            .iter()
            .filter(|model| !model.id.is_empty())
            .map(|model| (model.id.clone(), model.clone()))
            .collect::<HashMap<_, _>>();
        let hook;
        let registered_models;
        {
            let mut state = lock_unpoisoned(&self.state);
            if raw_ids.is_empty() {
                let event = unregister_client_locked(&mut state, client_id, self.clock.now());
                state.available_models_cache.clear();
                drop(state);
                if let Some((hook, provider)) = event {
                    trigger_unregistered(hook, provider, client_id.to_owned());
                }
                return;
            }

            let now = self.clock.now();
            if let Some(old_ids) = state.client_models.get(client_id).cloned() {
                let old_provider = state
                    .client_providers
                    .get(client_id)
                    .cloned()
                    .unwrap_or_default();
                reconcile_existing_client(
                    &mut state,
                    ClientReconciliation {
                        client_id,
                        old_provider: &old_provider,
                        provider: &provider,
                        old_ids: &old_ids,
                        new_ids: &raw_ids,
                        new_infos: &unique_infos,
                        now,
                    },
                );
            } else {
                for model_id in &raw_ids {
                    if let Some(model) = unique_infos.get(model_id) {
                        add_model_registration(&mut state, model_id, &provider, model, now);
                    }
                }
            }

            state
                .client_models
                .insert(client_id.to_owned(), raw_ids.clone());
            state
                .client_model_infos
                .insert(client_id.to_owned(), unique_infos);
            if provider.is_empty() {
                state.client_providers.remove(client_id);
            } else {
                state
                    .client_providers
                    .insert(client_id.to_owned(), provider.clone());
            }
            state.available_models_cache.clear();
            hook = state.hook.clone();
            registered_models = clone_unique_models(models);
        }
        if let Some(hook) = hook {
            trigger_registered(hook, provider, client_id.to_owned(), registered_models);
        }
    }

    pub fn unregister_client(&self, client_id: &str) {
        let event = {
            let mut state = lock_unpoisoned(&self.state);
            let event = unregister_client_locked(&mut state, client_id, self.clock.now());
            state.available_models_cache.clear();
            event
        };
        if let Some((hook, provider)) = event {
            trigger_unregistered(hook, provider, client_id.to_owned());
        }
    }

    pub fn set_model_quota_exceeded(&self, client_id: &str, model_id: &str) {
        let mut state = lock_unpoisoned(&self.state);
        if let Some(registration) = state.models.get_mut(model_id) {
            registration
                .quota_exceeded_clients
                .insert(client_id.to_owned(), self.clock.now());
            state.available_models_cache.clear();
        }
    }

    pub fn clear_model_quota_exceeded(&self, client_id: &str, model_id: &str) {
        let mut state = lock_unpoisoned(&self.state);
        if let Some(registration) = state.models.get_mut(model_id) {
            registration.quota_exceeded_clients.remove(client_id);
            state.available_models_cache.clear();
        }
    }

    pub fn suspend_client_model(&self, client_id: &str, model_id: &str, reason: &str) {
        if client_id.is_empty() || model_id.is_empty() {
            return;
        }
        let mut state = lock_unpoisoned(&self.state);
        let Some(registration) = state.models.get_mut(model_id) else {
            return;
        };
        if registration.suspended_clients.contains_key(client_id) {
            return;
        }
        registration
            .suspended_clients
            .insert(client_id.to_owned(), reason.to_owned());
        registration.last_updated = self.clock.now();
        state.available_models_cache.clear();
    }

    pub fn resume_client_model(&self, client_id: &str, model_id: &str) {
        if client_id.is_empty() || model_id.is_empty() {
            return;
        }
        let mut state = lock_unpoisoned(&self.state);
        let Some(registration) = state.models.get_mut(model_id) else {
            return;
        };
        if registration.suspended_clients.remove(client_id).is_none() {
            return;
        }
        registration.last_updated = self.clock.now();
        state.available_models_cache.clear();
    }

    pub fn client_supports_model(&self, client_id: &str, model_id: &str) -> bool {
        let client_id = client_id.trim();
        let model_id = model_id.trim();
        if client_id.is_empty() || model_id.is_empty() {
            return false;
        }
        lock_unpoisoned(&self.state)
            .client_models
            .get(client_id)
            .is_some_and(|models| {
                models
                    .iter()
                    .any(|id| id.trim().eq_ignore_ascii_case(model_id))
            })
    }

    pub fn available_models(&self, handler_type: &str) -> Vec<Map<String, Value>> {
        let now = self.clock.now();
        let mut state = lock_unpoisoned(&self.state);
        if let Some(cache) = state.available_models_cache.get(handler_type) {
            if cache.expires_at.is_none_or(|expiry| now < expiry) {
                return cache.models.clone();
            }
        }
        let (models, expires_at) = build_available_models(&state, handler_type, now);
        state.available_models_cache.insert(
            handler_type.to_owned(),
            AvailableModelsCacheEntry {
                models: models.clone(),
                expires_at,
            },
        );
        models
    }

    pub fn available_models_by_provider(&self, provider: &str) -> Vec<RegistryModelInfo> {
        let provider = provider.trim().to_ascii_lowercase();
        if provider.is_empty() {
            return Vec::new();
        }
        let state = lock_unpoisoned(&self.state);
        let mut provider_models: HashMap<String, (usize, RegistryModelInfo)> = HashMap::new();
        for (client_id, client_provider) in &state.client_providers {
            if client_provider != &provider {
                continue;
            }
            let Some(model_ids) = state.client_models.get(client_id) else {
                continue;
            };
            let client_infos = state.client_model_infos.get(client_id);
            for model_id in model_ids {
                let id = model_id.trim();
                if id.is_empty() {
                    continue;
                }
                let fallback = state.models.get(id).map(|registration| &registration.info);
                let Some(info) = client_infos
                    .and_then(|infos| infos.get(id))
                    .or(fallback)
                    .cloned()
                else {
                    continue;
                };
                provider_models
                    .entry(id.to_owned())
                    .and_modify(|entry| entry.0 += 1)
                    .or_insert((1, info));
            }
        }
        let now = self.clock.now();
        provider_models
            .into_iter()
            .filter_map(|(model_id, (available, info))| {
                let (quota, quota_suspended, other_suspended) = state
                    .models
                    .get(&model_id)
                    .map(|registration| {
                        provider_unavailable_counts(registration, &state, &provider, now)
                    })
                    .unwrap_or_default();
                let effective = available.saturating_sub(quota + other_suspended);
                (effective > 0
                    || (available > 0
                        && (quota > 0 || quota_suspended > 0)
                        && other_suspended == 0))
                    .then_some(info)
            })
            .collect()
    }

    pub fn model_count(&self, model_id: &str) -> usize {
        let now = self.clock.now();
        let state = lock_unpoisoned(&self.state);
        let Some(registration) = state.models.get(model_id) else {
            return 0;
        };
        let quota = registration
            .quota_exceeded_clients
            .values()
            .filter(|timestamp| elapsed(now, **timestamp) < MODEL_QUOTA_EXCEEDED_WINDOW)
            .count();
        registration
            .count
            .saturating_sub(quota + registration.suspended_clients.len())
    }

    pub fn model_providers(&self, model_id: &str) -> Vec<String> {
        let state = lock_unpoisoned(&self.state);
        let Some(registration) = state.models.get(model_id) else {
            return Vec::new();
        };
        let mut providers = registration
            .providers
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(name, count)| (name.clone(), *count))
            .collect::<Vec<_>>();
        providers.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        providers.into_iter().map(|(name, _)| name).collect()
    }

    pub fn model_info(&self, model_id: &str, provider: &str) -> Option<RegistryModelInfo> {
        let state = lock_unpoisoned(&self.state);
        let registration = state.models.get(model_id)?;
        if !provider.is_empty()
            && registration
                .providers
                .get(provider)
                .is_some_and(|count| *count > 0)
        {
            if let Some(info) = registration.info_by_provider.get(provider) {
                return Some(info.clone());
            }
        }
        Some(registration.info.clone())
    }

    pub fn lookup_model_info(&self, model_id: &str, provider: &str) -> Option<RegistryModelInfo> {
        let model_id = model_id.trim();
        if model_id.is_empty() {
            return None;
        }
        let provider = provider.trim().to_ascii_lowercase();
        self.model_info(model_id, &provider).or_else(|| {
            let catalog = self.static_catalog.snapshot();
            super::models_for_channel(&catalog, &provider)
                .and_then(|models| models.into_iter().find(|model| model.id == model_id))
                .or_else(|| lookup_static_registry_model_info(&catalog, model_id))
        })
    }

    pub fn model_override_headers(
        &self,
        model_id: &str,
        provider: &str,
    ) -> Option<BTreeMap<String, String>> {
        let headers = self
            .lookup_model_info(model_id, provider)?
            .config?
            .override_header;
        let filtered = headers
            .into_iter()
            .filter_map(|(key, value)| {
                let key = key.trim().to_owned();
                (!key.is_empty()).then_some((key, value))
            })
            .collect::<BTreeMap<_, _>>();
        (!filtered.is_empty()).then_some(filtered)
    }

    pub fn cleanup_expired_quotas(&self) {
        let now = self.clock.now();
        let mut state = lock_unpoisoned(&self.state);
        let mut changed = false;
        for registration in state.models.values_mut() {
            let before = registration.quota_exceeded_clients.len();
            registration
                .quota_exceeded_clients
                .retain(|_, timestamp| elapsed(now, *timestamp) < MODEL_QUOTA_EXCEEDED_WINDOW);
            changed |= registration.quota_exceeded_clients.len() != before;
        }
        if changed {
            state.available_models_cache.clear();
        }
    }

    pub fn first_available_model(&self, handler_type: &str) -> Result<String, RegistryError> {
        let mut models = self.available_models(handler_type);
        if models.is_empty() {
            return Err(RegistryError::NoModels(handler_type.to_owned()));
        }
        models.sort_by(|left, right| {
            let left = left.get("created").and_then(Value::as_i64).unwrap_or(0);
            let right = right.get("created").and_then(Value::as_i64).unwrap_or(0);
            right.cmp(&left)
        });
        models
            .into_iter()
            .filter_map(|model| model.get("id").and_then(Value::as_str).map(str::to_owned))
            .find(|id| self.model_count(id) > 0)
            .ok_or_else(|| RegistryError::NoAvailableClients(handler_type.to_owned()))
    }

    pub fn models_for_client(&self, client_id: &str) -> Vec<RegistryModelInfo> {
        let state = lock_unpoisoned(&self.state);
        let Some(model_ids) = state.client_models.get(client_id) else {
            return Vec::new();
        };
        let client_infos = state.client_model_infos.get(client_id);
        let mut seen = HashSet::new();
        model_ids
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .filter_map(|id| {
                client_infos
                    .and_then(|infos| infos.get(id))
                    .or_else(|| state.models.get(id).map(|registration| &registration.info))
                    .cloned()
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    NoModels(String),
    NoAvailableClients(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoModels(handler) => {
                write!(formatter, "no models available for handler type: {handler}")
            }
            Self::NoAvailableClients(handler) => write!(
                formatter,
                "no available clients for any model in handler type: {handler}"
            ),
        }
    }
}

impl std::error::Error for RegistryError {}

struct ClientReconciliation<'a> {
    client_id: &'a str,
    old_provider: &'a str,
    provider: &'a str,
    old_ids: &'a [String],
    new_ids: &'a [String],
    new_infos: &'a HashMap<String, RegistryModelInfo>,
    now: SystemTime,
}

fn reconcile_existing_client(state: &mut RegistryState, input: ClientReconciliation<'_>) {
    let ClientReconciliation {
        client_id,
        old_provider,
        provider,
        old_ids,
        new_ids,
        new_infos,
        now,
    } = input;
    let old_counts = counts(old_ids);
    let new_counts = counts(new_ids);
    let provider_changed = old_provider != provider;

    if provider_changed && !old_provider.is_empty() {
        for (id, new_count) in &new_counts {
            let overlap = (*new_count).min(*old_counts.get(id).unwrap_or(&0));
            if overlap == 0 {
                continue;
            }
            if let Some(registration) = state.models.get_mut(id) {
                decrement_provider(registration, old_provider, overlap);
            }
        }
    }
    for (id, old_count) in &old_counts {
        let new_count = *new_counts.get(id).unwrap_or(&0);
        for _ in 0..old_count.saturating_sub(new_count) {
            remove_model_registration(state, client_id, id, old_provider, now);
        }
    }
    for (id, new_count) in &new_counts {
        let old_count = *old_counts.get(id).unwrap_or(&0);
        for _ in 0..new_count.saturating_sub(old_count) {
            if let Some(model) = new_infos.get(id) {
                add_model_registration(state, id, provider, model, now);
            }
        }
    }
    for (id, model) in new_infos {
        if let Some(registration) = state.models.get_mut(id) {
            registration.info = model.clone();
            if !provider.is_empty() {
                registration
                    .info_by_provider
                    .insert(provider.to_owned(), model.clone());
            }
            registration.last_updated = now;
            registration.quota_exceeded_clients.remove(client_id);
            registration.suspended_clients.remove(client_id);
            if provider_changed && !provider.is_empty() {
                let overlap = new_counts[id].min(*old_counts.get(id).unwrap_or(&0));
                if overlap > 0 {
                    *registration
                        .providers
                        .entry(provider.to_owned())
                        .or_default() += overlap;
                }
            }
        }
    }
}

fn add_model_registration(
    state: &mut RegistryState,
    model_id: &str,
    provider: &str,
    model: &RegistryModelInfo,
    now: SystemTime,
) {
    if let Some(registration) = state.models.get_mut(model_id) {
        registration.count += 1;
        registration.last_updated = now;
        registration.info = model.clone();
        if !provider.is_empty() {
            *registration
                .providers
                .entry(provider.to_owned())
                .or_default() += 1;
            registration
                .info_by_provider
                .insert(provider.to_owned(), model.clone());
        }
        return;
    }
    let mut providers = HashMap::new();
    let mut info_by_provider = HashMap::new();
    if !provider.is_empty() {
        providers.insert(provider.to_owned(), 1);
        info_by_provider.insert(provider.to_owned(), model.clone());
    }
    state.models.insert(
        model_id.to_owned(),
        ModelRegistration {
            info: model.clone(),
            info_by_provider,
            count: 1,
            last_updated: now,
            quota_exceeded_clients: HashMap::new(),
            providers,
            suspended_clients: HashMap::new(),
        },
    );
}

fn remove_model_registration(
    state: &mut RegistryState,
    client_id: &str,
    model_id: &str,
    provider: &str,
    now: SystemTime,
) {
    let Some(registration) = state.models.get_mut(model_id) else {
        return;
    };
    registration.count = registration.count.saturating_sub(1);
    registration.last_updated = now;
    registration.quota_exceeded_clients.remove(client_id);
    registration.suspended_clients.remove(client_id);
    decrement_provider(registration, provider, 1);
    if registration.count == 0 {
        state.models.remove(model_id);
    }
}

fn decrement_provider(registration: &mut ModelRegistration, provider: &str, amount: usize) {
    if provider.is_empty() {
        return;
    }
    let Some(count) = registration.providers.get_mut(provider) else {
        return;
    };
    if *count <= amount {
        registration.providers.remove(provider);
        registration.info_by_provider.remove(provider);
    } else {
        *count -= amount;
    }
}

fn unregister_client_locked(
    state: &mut RegistryState,
    client_id: &str,
    now: SystemTime,
) -> Option<(Arc<dyn ModelRegistryHook>, String)> {
    let model_ids = state.client_models.remove(client_id)?;
    let provider = state.client_providers.remove(client_id).unwrap_or_default();
    for model_id in model_ids {
        remove_model_registration(state, client_id, &model_id, &provider, now);
    }
    state.client_model_infos.remove(client_id);
    state.hook.clone().map(|hook| (hook, provider))
}

fn counts(ids: &[String]) -> HashMap<String, usize> {
    let mut result = HashMap::new();
    for id in ids {
        *result.entry(id.clone()).or_default() += 1;
    }
    result
}

fn clone_unique_models(models: &[RegistryModelInfo]) -> Vec<RegistryModelInfo> {
    let mut seen = HashSet::new();
    models
        .iter()
        .filter(|model| !model.id.is_empty() && seen.insert(model.id.clone()))
        .cloned()
        .collect()
}

fn trigger_registered(
    hook: Arc<dyn ModelRegistryHook>,
    provider: String,
    client_id: String,
    models: Vec<RegistryModelInfo>,
) {
    let _ = std::thread::Builder::new()
        .name("cliproxy-registry-hook-register".to_owned())
        .spawn(move || {
            let context = HookContext {
                deadline: SystemTime::now() + DEFAULT_MODEL_REGISTRY_HOOK_TIMEOUT,
            };
            let _ = catch_unwind(AssertUnwindSafe(|| {
                hook.on_models_registered(context, &provider, &client_id, models);
            }));
        });
}

fn trigger_unregistered(hook: Arc<dyn ModelRegistryHook>, provider: String, client_id: String) {
    let _ = std::thread::Builder::new()
        .name("cliproxy-registry-hook-unregister".to_owned())
        .spawn(move || {
            let context = HookContext {
                deadline: SystemTime::now() + DEFAULT_MODEL_REGISTRY_HOOK_TIMEOUT,
            };
            let _ = catch_unwind(AssertUnwindSafe(|| {
                hook.on_models_unregistered(context, &provider, &client_id);
            }));
        });
}

fn build_available_models(
    state: &RegistryState,
    handler_type: &str,
    now: SystemTime,
) -> (Vec<Map<String, Value>>, Option<SystemTime>) {
    let mut models = Vec::with_capacity(state.models.len());
    let mut expires_at = None;
    for registration in state.models.values() {
        let mut quota = 0;
        for timestamp in registration.quota_exceeded_clients.values() {
            let recovery = *timestamp + MODEL_QUOTA_EXCEEDED_WINDOW;
            if now < recovery {
                quota += 1;
                expires_at = Some(expires_at.map_or(recovery, |old: SystemTime| old.min(recovery)));
            }
        }
        let quota_suspended = registration
            .suspended_clients
            .values()
            .filter(|reason| reason.eq_ignore_ascii_case("quota"))
            .count();
        let other_suspended = registration.suspended_clients.len() - quota_suspended;
        let effective = registration.count.saturating_sub(quota + other_suspended);
        if effective > 0
            || (registration.count > 0
                && (quota > 0 || quota_suspended > 0)
                && other_suspended == 0)
        {
            models.push(convert_model_to_map(&registration.info, handler_type));
        }
    }
    (models, expires_at)
}

fn provider_unavailable_counts(
    registration: &ModelRegistration,
    state: &RegistryState,
    provider: &str,
    now: SystemTime,
) -> (usize, usize, usize) {
    let quota = registration
        .quota_exceeded_clients
        .iter()
        .filter(|(client, timestamp)| {
            state
                .client_providers
                .get(*client)
                .is_some_and(|p| p == provider)
                && elapsed(now, **timestamp) < MODEL_QUOTA_EXCEEDED_WINDOW
        })
        .count();
    let mut quota_suspended = 0;
    let mut other_suspended = 0;
    for (client, reason) in &registration.suspended_clients {
        if state
            .client_providers
            .get(client)
            .is_none_or(|p| p != provider)
        {
            continue;
        }
        if reason.eq_ignore_ascii_case("quota") {
            quota_suspended += 1;
        } else {
            other_suspended += 1;
        }
    }
    (quota, quota_suspended, other_suspended)
}

fn convert_model_to_map(model: &RegistryModelInfo, handler_type: &str) -> Map<String, Value> {
    let mut result = Map::new();
    match handler_type {
        "openai" => {
            insert_string(&mut result, "id", &model.id, true);
            insert_string(&mut result, "object", "model", true);
            insert_string(&mut result, "owned_by", &model.owned_by, true);
            insert_i64(&mut result, "created", model.created);
            insert_string(&mut result, "type", &model.provider_type, false);
            insert_string(&mut result, "display_name", &model.display_name, false);
            insert_string(&mut result, "version", &model.version, false);
            insert_string(&mut result, "description", &model.description, false);
            insert_usize(&mut result, "context_length", model.context_length);
            insert_usize(&mut result, "max_context_length", model.max_context_length);
            insert_usize(
                &mut result,
                "max_completion_tokens",
                model.max_completion_tokens,
            );
            if !model.supported_parameters.is_empty() {
                result.insert(
                    "supported_parameters".to_owned(),
                    serde_json::to_value(&model.supported_parameters).unwrap_or(Value::Null),
                );
            }
        }
        "claude" => {
            insert_string(&mut result, "id", &model.id, true);
            insert_string(&mut result, "object", "model", true);
            insert_string(&mut result, "owned_by", &model.owned_by, true);
            if model.created > 0 {
                if let Some(timestamp) = DateTime::<Utc>::from_timestamp(model.created, 0) {
                    result.insert(
                        "created_at".to_owned(),
                        Value::String(timestamp.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
                    );
                }
            }
            result.insert("type".to_owned(), Value::String("model".to_owned()));
            result.insert(
                "display_name".to_owned(),
                Value::String(if model.display_name.is_empty() {
                    model.id.clone()
                } else {
                    model.display_name.clone()
                }),
            );
            result.insert(
                "max_input_tokens".to_owned(),
                Value::Number(Number::from(if model.context_length == 0 {
                    DEFAULT_CLAUDE_MAX_INPUT_TOKENS
                } else {
                    model.context_length
                } as u64)),
            );
            result.insert(
                "max_tokens".to_owned(),
                Value::Number(Number::from(if model.max_completion_tokens == 0 {
                    DEFAULT_CLAUDE_MAX_OUTPUT_TOKENS
                } else {
                    model.max_completion_tokens
                } as u64)),
            );
        }
        "gemini" => {
            insert_string(
                &mut result,
                "name",
                if model.name.is_empty() {
                    &model.id
                } else {
                    &model.name
                },
                true,
            );
            insert_string(&mut result, "version", &model.version, false);
            insert_string(&mut result, "displayName", &model.display_name, false);
            insert_string(&mut result, "description", &model.description, false);
            insert_usize(&mut result, "inputTokenLimit", model.input_token_limit);
            insert_usize(&mut result, "outputTokenLimit", model.output_token_limit);
            insert_vec(
                &mut result,
                "supportedGenerationMethods",
                &model.supported_generation_methods,
            );
            insert_vec(
                &mut result,
                "supportedInputModalities",
                &model.supported_input_modalities,
            );
            insert_vec(
                &mut result,
                "supportedOutputModalities",
                &model.supported_output_modalities,
            );
        }
        _ => {
            insert_string(&mut result, "id", &model.id, true);
            insert_string(&mut result, "object", "model", true);
            insert_string(&mut result, "owned_by", &model.owned_by, false);
            insert_string(&mut result, "type", &model.provider_type, false);
            insert_i64(&mut result, "created", model.created);
        }
    }
    result
}

fn insert_string(map: &mut Map<String, Value>, key: &str, value: &str, required: bool) {
    if required || !value.is_empty() {
        map.insert(key.to_owned(), Value::String(value.to_owned()));
    }
}

fn insert_i64(map: &mut Map<String, Value>, key: &str, value: i64) {
    if value != 0 {
        map.insert(key.to_owned(), Value::Number(Number::from(value)));
    }
}

fn insert_usize(map: &mut Map<String, Value>, key: &str, value: usize) {
    if value != 0 {
        map.insert(key.to_owned(), Value::Number(Number::from(value as u64)));
    }
}

fn insert_vec(map: &mut Map<String, Value>, key: &str, value: &[String]) {
    if !value.is_empty() {
        map.insert(
            key.to_owned(),
            serde_json::to_value(value).unwrap_or(Value::Null),
        );
    }
}

fn elapsed(now: SystemTime, then: SystemTime) -> Duration {
    now.duration_since(then).unwrap_or(Duration::ZERO)
}

pub(super) fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
