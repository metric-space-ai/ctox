// ref: internal/registry/model_updater.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use super::{
    embedded_models_catalog, lookup_static_registry_model_info, model_override_headers,
    models_for_channel, parse_models_catalog, DevinModelsError, DevinModelsStore,
    RegistryModelInfo, StaticModelCatalogError, StaticModelsCatalog,
};

pub const MODELS_FETCH_TIMEOUT: Duration = Duration::from_secs(30);
pub const MODELS_REFRESH_INTERVAL: Duration = Duration::from_secs(3 * 60 * 60);
pub const MAX_MODELS_CATALOG_SIZE: usize = 16 << 20;
pub const MAX_DEVIN_MODELS_CATALOG_SIZE: usize = 8 << 20;
pub const DEFAULT_DEVIN_MODELS_URLS: [&str; 2] = [
    "https://raw.githubusercontent.com/router-for-me/models/refs/heads/main/devin_models.json",
    "https://models.router-for.me/devin_models.json",
];
pub const DEFAULT_MODELS_URLS: [&str; 2] = [
    "https://raw.githubusercontent.com/router-for-me/models/refs/heads/main/models.json",
    "https://models.router-for.me/models.json",
];

pub type ModelsFetchFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + 'a>>;

pub trait ModelsSource: Send + Sync {
    fn fetch<'a>(&'a self, source: &'a str, max_bytes: usize) -> ModelsFetchFuture<'a>;
}

#[derive(Clone, Debug)]
pub struct ModelCatalogSnapshot {
    pub catalog: Arc<StaticModelsCatalog>,
    pub revision: u64,
}

struct CatalogState {
    catalog: Arc<StaticModelsCatalog>,
    revision: u64,
}

/// Instance-owned replacement for upstream's `modelsCatalogStore` global.
pub struct ModelCatalogStore {
    state: RwLock<CatalogState>,
    devin: DevinModelsStore,
}

impl ModelCatalogStore {
    pub fn from_embedded() -> Result<Self, StaticModelCatalogError> {
        Ok(Self::new(embedded_models_catalog()?))
    }

    pub fn new(catalog: StaticModelsCatalog) -> Self {
        Self {
            state: RwLock::new(CatalogState {
                catalog: Arc::new(catalog),
                revision: 1,
            }),
            devin: DevinModelsStore::from_embedded().unwrap_or_default(),
        }
    }

    pub fn snapshot(&self) -> ModelCatalogSnapshot {
        let state = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ModelCatalogSnapshot {
            catalog: Arc::clone(&state.catalog),
            revision: state.revision,
        }
    }

    pub fn devin_store(&self) -> &DevinModelsStore {
        &self.devin
    }

    pub fn load_devin_bytes(
        &self,
        data: &[u8],
        source: &str,
    ) -> Result<CatalogLoad, DevinModelsError> {
        self.devin.load(data, source)
    }

    pub fn models_for_channel(&self, channel: &str) -> Option<Vec<RegistryModelInfo>> {
        let snapshot = self.snapshot();
        if channel.trim().eq_ignore_ascii_case("devin") {
            return Some(self.devin.models(&snapshot.catalog));
        }
        models_for_channel(&snapshot.catalog, channel)
    }

    fn active_catalog(&self) -> StaticModelsCatalog {
        let snapshot = self.snapshot();
        let mut catalog = (*snapshot.catalog).clone();
        catalog.devin = self.devin.models(&catalog);
        catalog
    }

    pub fn lookup_model_info(&self, model_id: &str) -> Option<RegistryModelInfo> {
        lookup_static_registry_model_info(&self.active_catalog(), model_id)
    }

    pub fn model_override_headers(
        &self,
        model_id: &str,
    ) -> Option<std::collections::BTreeMap<String, String>> {
        model_override_headers(&self.active_catalog(), model_id)
    }

    pub fn load(&self, catalog: StaticModelsCatalog) -> CatalogLoad {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let changed_providers = detect_changed_providers(&state.catalog, &catalog);
        let changed = *state.catalog != catalog;
        if changed {
            state.catalog = Arc::new(catalog);
            state.revision = state.revision.saturating_add(1);
        }
        CatalogLoad {
            changed,
            changed_providers,
            revision: state.revision,
        }
    }

    pub fn load_bytes(
        &self,
        data: &[u8],
        source: &str,
    ) -> Result<CatalogLoad, StaticModelCatalogError> {
        parse_models_catalog(data)
            .map(|catalog| self.load(catalog))
            .map_err(|error| StaticModelCatalogError::InvalidCatalog(format!("{source}: {error}")))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogLoad {
    pub changed: bool,
    pub changed_providers: Vec<String>,
    pub revision: u64,
}

pub type ModelRefreshCallback = Arc<dyn Fn(Vec<String>) + Send + Sync + 'static>;

#[derive(Default)]
struct CallbackState {
    callback: Option<ModelRefreshCallback>,
    pending: Vec<String>,
}

pub struct ModelRefreshSink {
    state: Mutex<CallbackState>,
}

impl Default for ModelRefreshSink {
    fn default() -> Self {
        Self {
            state: Mutex::new(CallbackState::default()),
        }
    }
}

impl ModelRefreshSink {
    pub fn set_callback(&self, callback: Option<ModelRefreshCallback>) {
        let pending = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.callback = callback.clone();
            if callback.is_some() {
                std::mem::take(&mut state.pending)
            } else {
                Vec::new()
            }
        };
        if let Some(callback) = callback {
            if !pending.is_empty() {
                callback(pending);
            }
        }
    }

    pub fn notify(&self, providers: &[String]) {
        if providers.is_empty() {
            return;
        }
        let callback = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.callback.is_none() {
                state.pending = merge_provider_names(&state.pending, providers);
            }
            state.callback.clone()
        };
        if let Some(callback) = callback {
            callback(providers.to_vec());
        }
    }
}

pub struct ModelsUpdater {
    store: Arc<ModelCatalogStore>,
    source: Arc<dyn ModelsSource>,
    sources: Vec<String>,
    devin_sources: Vec<String>,
    refresh_sink: Arc<ModelRefreshSink>,
    interval: Duration,
}

impl ModelsUpdater {
    pub fn new(
        store: Arc<ModelCatalogStore>,
        source: Arc<dyn ModelsSource>,
        sources: Vec<String>,
        refresh_sink: Arc<ModelRefreshSink>,
    ) -> Self {
        Self {
            store,
            source,
            sources,
            devin_sources: DEFAULT_DEVIN_MODELS_URLS
                .iter()
                .map(|source| (*source).into())
                .collect(),
            refresh_sink,
            interval: MODELS_REFRESH_INTERVAL,
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    pub fn with_devin_sources(mut self, sources: Vec<String>) -> Self {
        self.devin_sources = sources;
        self
    }

    pub async fn refresh_once(&self) -> Result<ModelsRefresh, ModelsRefreshError> {
        let mut failures = Vec::with_capacity(self.sources.len());
        for source in &self.sources {
            let data = match self.source.fetch(source, MAX_MODELS_CATALOG_SIZE).await {
                Ok(data) if data.len() <= MAX_MODELS_CATALOG_SIZE => data,
                Ok(_) => {
                    failures.push(ModelFetchFailure {
                        source: source.clone(),
                        reason: format!("catalog exceeded {MAX_MODELS_CATALOG_SIZE} bytes"),
                    });
                    continue;
                }
                Err(reason) => {
                    failures.push(ModelFetchFailure {
                        source: source.clone(),
                        reason,
                    });
                    continue;
                }
            };
            let catalog = match parse_models_catalog(&data) {
                Ok(catalog) => catalog,
                Err(error) => {
                    failures.push(ModelFetchFailure {
                        source: source.clone(),
                        reason: error.to_string(),
                    });
                    continue;
                }
            };
            let load = self.store.load(catalog);
            self.refresh_sink.notify(&load.changed_providers);
            return Ok(ModelsRefresh {
                source: source.clone(),
                changed: load.changed,
                changed_providers: load.changed_providers,
                revision: load.revision,
            });
        }
        Err(ModelsRefreshError { failures })
    }

    /// ref: internal/registry/devin_models_updater.go:53-107 @ d7914afd.
    /// The first successfully fetched body owns validation; a rejected payload
    /// keeps the current catalog, without silently switching catalog authority.
    pub async fn refresh_devin_once(&self) -> Result<ModelsRefresh, ModelsRefreshError> {
        let mut failures = Vec::with_capacity(self.devin_sources.len());
        for source in &self.devin_sources {
            let data = match tokio::time::timeout(
                MODELS_FETCH_TIMEOUT,
                self.source.fetch(source, MAX_DEVIN_MODELS_CATALOG_SIZE),
            )
            .await
            {
                Ok(Ok(data)) if data.len() <= MAX_DEVIN_MODELS_CATALOG_SIZE => data,
                Ok(Ok(_)) => {
                    failures.push(ModelFetchFailure {
                        source: source.clone(),
                        reason: format!("catalog exceeded {MAX_DEVIN_MODELS_CATALOG_SIZE} bytes"),
                    });
                    continue;
                }
                Ok(Err(reason)) => {
                    failures.push(ModelFetchFailure {
                        source: source.clone(),
                        reason,
                    });
                    continue;
                }
                Err(_) => {
                    failures.push(ModelFetchFailure {
                        source: source.clone(),
                        reason: "catalog fetch timed out".into(),
                    });
                    continue;
                }
            };
            let load = self
                .store
                .load_devin_bytes(&data, source)
                .map_err(|error| {
                    failures.push(ModelFetchFailure {
                        source: source.clone(),
                        reason: error.to_string(),
                    });
                    ModelsRefreshError {
                        failures: std::mem::take(&mut failures),
                    }
                })?;
            self.refresh_sink.notify(&load.changed_providers);
            return Ok(ModelsRefresh {
                source: source.clone(),
                changed: load.changed,
                changed_providers: load.changed_providers,
                revision: load.revision,
            });
        }
        Err(ModelsRefreshError { failures })
    }

    async fn refresh_catalogs(&self) {
        let _ = self.refresh_once().await;
        let _ = self.refresh_devin_once().await;
    }

    /// The existing host-owned task refreshes both catalogs at startup and
    /// every three hours. Cancellation drops in-flight header/body reads.
    /// No process-global goroutine, retry worker or second timer is created.
    pub async fn run(&self, mut cancelled: tokio::sync::watch::Receiver<bool>) {
        if *cancelled.borrow() {
            return;
        }
        tokio::select! {
            biased;
            _ = cancelled.wait_for(|value| *value) => return,
            _ = self.refresh_catalogs() => {}
        }
        let mut ticker = tokio::time::interval(self.interval);
        ticker.tick().await;
        loop {
            let stop = tokio::select! {
                biased;
                _ = cancelled.wait_for(|value| *value) => true,
                _ = ticker.tick() => false,
            };
            if stop {
                return;
            }
            tokio::select! {
                biased;
                _ = cancelled.wait_for(|value| *value) => return,
                _ = self.refresh_catalogs() => {}
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelsRefresh {
    pub source: String,
    pub changed: bool,
    pub changed_providers: Vec<String>,
    pub revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelFetchFailure {
    pub source: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelsRefreshError {
    pub failures: Vec<ModelFetchFailure>,
}

impl std::fmt::Display for ModelsRefreshError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "models fetch failed from all {} sources",
            self.failures.len()
        )
    }
}

impl std::error::Error for ModelsRefreshError {}

pub fn detect_changed_providers(
    old: &StaticModelsCatalog,
    new: &StaticModelsCatalog,
) -> Vec<String> {
    let sections = [
        ("claude", &old.claude, &new.claude),
        ("gemini", &old.gemini, &new.gemini),
        ("vertex", &old.vertex, &new.vertex),
        ("aistudio", &old.aistudio, &new.aistudio),
        ("codex", &old.codex_free, &new.codex_free),
        ("codex", &old.codex_team, &new.codex_team),
        ("codex", &old.codex_plus, &new.codex_plus),
        ("codex", &old.codex_pro, &new.codex_pro),
        ("kimi", &old.kimi, &new.kimi),
        ("antigravity", &old.antigravity, &new.antigravity),
        ("xai", &old.xai, &new.xai),
        ("meta", &old.meta, &new.meta),
        ("devin", &old.devin, &new.devin),
    ];
    let mut seen = std::collections::HashSet::new();
    let mut changed = Vec::new();
    for (provider, old_section, new_section) in sections {
        if seen.contains(provider) {
            continue;
        }
        if old_section != new_section {
            seen.insert(provider);
            changed.push(provider.to_owned());
        }
    }
    changed
}

pub fn merge_provider_names(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    existing
        .iter()
        .chain(incoming)
        .filter_map(|provider| {
            let provider = provider.trim().to_ascii_lowercase();
            (!provider.is_empty() && seen.insert(provider.clone())).then_some(provider)
        })
        .collect()
}

#[cfg(feature = "codex-http-transport")]
mod http {
    use futures_util::StreamExt;
    use wreq::{Client, Proxy};

    use super::{ModelsFetchFuture, ModelsSource, MODELS_FETCH_TIMEOUT};

    #[derive(Clone)]
    pub struct WreqModelsSource {
        client: Client,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum WreqModelsSourceBuildError {
        InvalidProxy,
        Client,
    }

    impl WreqModelsSource {
        pub fn new(proxy_url: Option<&str>) -> Result<Self, WreqModelsSourceBuildError> {
            let mut builder = Client::builder()
                .connect_timeout(MODELS_FETCH_TIMEOUT)
                .retry(wreq::retry::Policy::never())
                .redirect(wreq::redirect::Policy::none());
            if let Some(proxy_url) = proxy_url.map(str::trim).filter(|url| !url.is_empty()) {
                builder = builder.proxy(
                    Proxy::all(proxy_url).map_err(|_| WreqModelsSourceBuildError::InvalidProxy)?,
                );
            } else {
                builder = builder.no_proxy();
            }
            let client = builder
                .build()
                .map_err(|_| WreqModelsSourceBuildError::Client)?;
            Ok(Self { client })
        }
    }

    impl ModelsSource for WreqModelsSource {
        fn fetch<'a>(&'a self, source: &'a str, max_bytes: usize) -> ModelsFetchFuture<'a> {
            Box::pin(async move {
                let response = self
                    .client
                    .get(source)
                    .timeout(MODELS_FETCH_TIMEOUT)
                    .send()
                    .await
                    .map_err(|error| format!("request failed: {error}"))?;
                if response.status().as_u16() != 200 {
                    return Err(format!("HTTP status {}", response.status().as_u16()));
                }
                let mut output = Vec::new();
                let mut chunks = response.bytes_stream();
                while let Some(chunk) = chunks.next().await {
                    let chunk = chunk.map_err(|error| format!("response read failed: {error}"))?;
                    if output.len().saturating_add(chunk.len()) > max_bytes {
                        return Err(format!("catalog exceeded {max_bytes} bytes"));
                    }
                    output.extend_from_slice(&chunk);
                }
                Ok(output)
            })
        }
    }
}

#[cfg(feature = "codex-http-transport")]
pub use http::{WreqModelsSource, WreqModelsSourceBuildError};
