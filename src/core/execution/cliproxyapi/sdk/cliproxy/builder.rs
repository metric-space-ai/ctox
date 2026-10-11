// ref: sdk/cliproxy/builder.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

//! Typed assembly boundary for the still-separate `Service` port.
//!
//! Upstream `Build` constructs file watchers, plugin hosts and token stores
//! through package globals.  This port resolves the same dependencies into an
//! inspectable `ServiceAssembly`. Missing host-owned capabilities remain
//! explicit requirements; no listener, background task, environment lookup or
//! filesystem authority is created by the builder.
//!
//! The separate `Service` port must consume this assembly by (in order):
//! materializing every reported host binding, creating the watcher from the
//! injected factory, loading token/API-key providers, wiring persisted auth
//! updates into the core manager, applying the one-shot server options to a
//! CTOX-supervised router, and executing the before/after hooks around start.
//! Until that file is ported, `requirements()` is the fail-visible record of
//! which pieces the host still owes; an empty list is the materialization gate.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::internal::config::{CliproxyRuntimeConfig, RuntimeConfigError, ValidatedRuntimeConfig};
use crate::sdk::access::{Manager as AccessManager, SharedProvider};
use crate::sdk::api::options::{self, ServerOption};
use crate::sdk::auth::Manager as LegacyAuthManager;

use super::auth::{AuthManager, CooldownStateStore, PostAuthHook};
use super::providers::new_file_token_client_provider;
use super::rtprovider::{new_default_round_tripper_provider, DefaultRoundTripperProvider};
use super::service_config::ServiceConfigRuntime;
use super::service_models::{ServiceModelConfig, ServiceModelRuntime};
use super::types::{ApiKeyClientProvider, AuthUpdate, TokenClientProvider, WatcherFactory};

pub type BeforeStartHook = Arc<dyn Fn(&ValidatedRuntimeConfig) + Send + Sync>;
pub type AfterStartHook = Arc<dyn Fn(&ServiceAssembly) + Send + Sync>;

#[derive(Clone, Default)]
pub struct Hooks {
    pub on_before_start: Option<BeforeStartHook>,
    pub on_after_start: Option<AfterStartHook>,
}

impl fmt::Debug for Hooks {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Hooks")
            .field("has_on_before_start", &self.on_before_start.is_some())
            .field("has_on_after_start", &self.on_after_start.is_some())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginHostError {
    Configuration,
    ProviderRegistration,
}

impl fmt::Display for PluginHostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Configuration => "plugin host configuration failed",
            Self::ProviderRegistration => "plugin access-provider registration failed",
        })
    }
}

impl std::error::Error for PluginHostError {}

pub trait PluginHost: Send + Sync {
    fn apply_config(&self, config: &ValidatedRuntimeConfig) -> Result<(), PluginHostError>;
    fn register_frontend_auth_providers(&self) -> Result<(), PluginHostError>;
    fn access_providers(&self) -> Vec<SharedProvider>;
}

pub trait PersistedAuthUpdateSink: Send + Sync {
    fn dispatch_persisted_auth_update(&self, update: AuthUpdate) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ServiceBindingRequirement {
    ApiKeyClientProvider,
    WatcherFactory,
    CoreAuthManager,
    PluginHost,
    PersistedAuthUpdateSink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuilderErrorKind {
    ConfigurationRequired,
    ConfigPathRequired,
    InvalidConfiguration,
    PluginHost,
}

#[derive(Debug)]
pub struct BuilderError {
    pub kind: BuilderErrorKind,
    pub config: Option<RuntimeConfigError>,
    pub plugin: Option<PluginHostError>,
    pub credential_path: Option<String>,
}

impl fmt::Display for BuilderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            BuilderErrorKind::ConfigurationRequired => {
                formatter.write_str("cliproxy: configuration is required")
            }
            BuilderErrorKind::ConfigPathRequired => {
                formatter.write_str("cliproxy: configuration path is required")
            }
            BuilderErrorKind::InvalidConfiguration => write!(
                formatter,
                "cliproxy: validate {}: {}",
                self.credential_path
                    .as_deref()
                    .unwrap_or("runtime configuration"),
                self.config.as_ref().map_or(
                    "unknown configuration error".to_owned(),
                    ToString::to_string
                )
            ),
            BuilderErrorKind::PluginHost => write!(
                formatter,
                "cliproxy: {}",
                self.plugin
                    .as_ref()
                    .map_or("plugin host failed".to_owned(), ToString::to_string)
            ),
        }
    }
}

impl std::error::Error for BuilderError {}

pub struct ServiceAssembly {
    config: ValidatedRuntimeConfig,
    config_path: PathBuf,
    token_provider: Arc<dyn TokenClientProvider>,
    api_key_provider: Option<Arc<dyn ApiKeyClientProvider>>,
    watcher_factory: Option<Arc<dyn WatcherFactory>>,
    hooks: Hooks,
    auth_manager: Arc<LegacyAuthManager>,
    access_manager: Arc<AccessManager>,
    core_manager: Option<Arc<AuthManager>>,
    cooldown_state_store: Option<Arc<dyn CooldownStateStore>>,
    plugin_host: Option<Arc<dyn PluginHost>>,
    round_tripper_provider: Arc<DefaultRoundTripperProvider>,
    post_auth_hook: Option<PostAuthHook>,
    persisted_auth_update_sink: Option<Arc<dyn PersistedAuthUpdateSink>>,
    server_options: Vec<ServerOption>,
    requirements: Vec<ServiceBindingRequirement>,
}

/// Host-owned dependencies required to turn an assembly into a running
/// service.  Keeping this separate from `Builder` makes authority transfer an
/// explicit, one-shot operation at the embedding boundary.
#[derive(Default)]
pub struct ServiceBindings {
    pub api_key_provider: Option<Arc<dyn ApiKeyClientProvider>>,
    pub watcher_factory: Option<Arc<dyn WatcherFactory>>,
    pub core_manager: Option<Arc<AuthManager>>,
    pub plugin_host: Option<Arc<dyn PluginHost>>,
    pub persisted_auth_update_sink: Option<Arc<dyn PersistedAuthUpdateSink>>,
    /// Optional Home/watcher convergence graph. When bound, `Service` owns
    /// its lifetime and all watcher reloads flow through it.
    pub runtime_graph: Option<Arc<super::service_runtime::ServiceRuntimeGraph>>,
    /// Anonymous version metadata capability, using the embedding host's
    /// existing typed proxy configuration. No account credentials are supplied.
    pub xai_client_version_fetcher: Option<
        Arc<dyn crate::internal::runtime::executor::helps::xai_client_version::XaiVersionFetcher>,
    >,
}

impl fmt::Debug for ServiceAssembly {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServiceAssembly")
            .field("config_path", &self.config_path)
            .field("has_token_provider", &true)
            .field("has_api_key_provider", &self.api_key_provider.is_some())
            .field("has_watcher_factory", &self.watcher_factory.is_some())
            .field("hooks", &self.hooks)
            .field("has_auth_manager", &true)
            .field("has_access_manager", &true)
            .field("has_core_manager", &self.core_manager.is_some())
            .field(
                "has_cooldown_state_store",
                &self.cooldown_state_store.is_some(),
            )
            .field("has_plugin_host", &self.plugin_host.is_some())
            .field("has_round_tripper_provider", &true)
            .field("has_post_auth_hook", &self.post_auth_hook.is_some())
            .field(
                "has_persisted_auth_update_sink",
                &self.persisted_auth_update_sink.is_some(),
            )
            .field("server_option_count", &self.server_options.len())
            .field("requirements", &self.requirements)
            .finish()
    }
}

impl ServiceAssembly {
    /// Creates a per-service config owner. It publishes snapshots only and
    /// cannot start a listener or watcher.
    pub fn config_runtime(&self) -> ServiceConfigRuntime {
        ServiceConfigRuntime::new(self.config.clone())
    }

    pub fn model_runtime(
        &self,
        registry: Arc<dyn super::model_registry::ModelRegistry>,
        catalog: Arc<crate::internal::registry::StaticModelsCatalog>,
    ) -> ServiceModelRuntime {
        ServiceModelRuntime::new(
            ServiceModelConfig::from_runtime(&self.config),
            registry,
            catalog,
        )
    }

    /// Fills only dependencies which the builder deliberately left for the
    /// host. Already-bound dependencies are never replaced.
    pub fn materialize(&mut self, bindings: ServiceBindings) -> Result<(), PluginHostError> {
        self.api_key_provider = self.api_key_provider.take().or(bindings.api_key_provider);
        self.watcher_factory = self.watcher_factory.take().or(bindings.watcher_factory);
        self.core_manager = self.core_manager.take().or(bindings.core_manager);
        if self.plugin_host.is_none() {
            if let Some(plugin_host) = bindings.plugin_host {
                plugin_host.apply_config(&self.config)?;
                plugin_host.register_frontend_auth_providers()?;
                self.access_manager
                    .set_shared_providers(&plugin_host.access_providers());
                self.plugin_host = Some(plugin_host);
            }
        }
        self.persisted_auth_update_sink = self
            .persisted_auth_update_sink
            .take()
            .or(bindings.persisted_auth_update_sink);
        self.requirements.clear();
        if self.api_key_provider.is_none() {
            self.requirements
                .push(ServiceBindingRequirement::ApiKeyClientProvider);
        }
        if self.watcher_factory.is_none() {
            self.requirements
                .push(ServiceBindingRequirement::WatcherFactory);
        }
        if self.core_manager.is_none() {
            self.requirements
                .push(ServiceBindingRequirement::CoreAuthManager);
        }
        if self.plugin_host.is_none() {
            self.requirements
                .push(ServiceBindingRequirement::PluginHost);
        }
        if self.persisted_auth_update_sink.is_none() {
            self.requirements
                .push(ServiceBindingRequirement::PersistedAuthUpdateSink);
        }
        Ok(())
    }
    #[must_use]
    pub fn config(&self) -> &ValidatedRuntimeConfig {
        &self.config
    }

    #[must_use]
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    #[must_use]
    pub fn token_provider(&self) -> Arc<dyn TokenClientProvider> {
        Arc::clone(&self.token_provider)
    }

    #[must_use]
    pub fn api_key_provider(&self) -> Option<Arc<dyn ApiKeyClientProvider>> {
        self.api_key_provider.clone()
    }

    #[must_use]
    pub fn watcher_factory(&self) -> Option<Arc<dyn WatcherFactory>> {
        self.watcher_factory.clone()
    }

    #[must_use]
    pub fn auth_manager(&self) -> Arc<LegacyAuthManager> {
        Arc::clone(&self.auth_manager)
    }

    #[must_use]
    pub fn access_manager(&self) -> Arc<AccessManager> {
        Arc::clone(&self.access_manager)
    }

    #[must_use]
    pub fn core_manager(&self) -> Option<Arc<AuthManager>> {
        self.core_manager.clone()
    }

    #[must_use]
    pub fn cooldown_state_store(&self) -> Option<Arc<dyn CooldownStateStore>> {
        self.cooldown_state_store.clone()
    }

    #[must_use]
    pub fn plugin_host(&self) -> Option<Arc<dyn PluginHost>> {
        self.plugin_host.clone()
    }

    #[must_use]
    pub fn round_tripper_provider(&self) -> Arc<DefaultRoundTripperProvider> {
        Arc::clone(&self.round_tripper_provider)
    }

    #[must_use]
    pub fn post_auth_hook(&self) -> Option<PostAuthHook> {
        self.post_auth_hook.clone()
    }

    #[must_use]
    pub fn persisted_auth_update_sink(&self) -> Option<Arc<dyn PersistedAuthUpdateSink>> {
        self.persisted_auth_update_sink.clone()
    }

    #[must_use]
    pub fn requirements(&self) -> &[ServiceBindingRequirement] {
        &self.requirements
    }

    #[must_use]
    pub fn is_materializable(&self) -> bool {
        self.requirements.is_empty()
    }

    pub fn run_before_start(&self) {
        if let Some(hook) = &self.hooks.on_before_start {
            hook(&self.config);
        }
    }

    pub fn run_after_start(&self) {
        if let Some(hook) = &self.hooks.on_after_start {
            hook(self);
        }
    }

    /// Consumes the functional options exactly once when the Service host
    /// materializes its HTTP router.
    pub fn take_server_options(&mut self) -> Vec<ServerOption> {
        std::mem::take(&mut self.server_options)
    }
}

#[derive(Default)]
pub struct Builder {
    config: Option<CliproxyRuntimeConfig>,
    config_path: Option<PathBuf>,
    token_provider: Option<Arc<dyn TokenClientProvider>>,
    api_key_provider: Option<Arc<dyn ApiKeyClientProvider>>,
    watcher_factory: Option<Arc<dyn WatcherFactory>>,
    hooks: Hooks,
    auth_manager: Option<Arc<LegacyAuthManager>>,
    access_manager: Option<Arc<AccessManager>>,
    core_manager: Option<Arc<AuthManager>>,
    cooldown_state_store: Option<Arc<dyn CooldownStateStore>>,
    plugin_host: Option<Arc<dyn PluginHost>>,
    round_tripper_provider: Option<Arc<DefaultRoundTripperProvider>>,
    post_auth_hook: Option<PostAuthHook>,
    persisted_auth_update_sink: Option<Arc<dyn PersistedAuthUpdateSink>>,
    server_options: Vec<ServerOption>,
}

impl fmt::Debug for Builder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Builder")
            .field("has_config", &self.config.is_some())
            .field("config_path", &self.config_path)
            .field("has_token_provider", &self.token_provider.is_some())
            .field("has_api_key_provider", &self.api_key_provider.is_some())
            .field("has_watcher_factory", &self.watcher_factory.is_some())
            .field("hooks", &self.hooks)
            .field("has_auth_manager", &self.auth_manager.is_some())
            .field("has_access_manager", &self.access_manager.is_some())
            .field("has_core_manager", &self.core_manager.is_some())
            .field("has_plugin_host", &self.plugin_host.is_some())
            .field("server_option_count", &self.server_options.len())
            .finish_non_exhaustive()
    }
}

#[must_use]
pub fn new_builder() -> Builder {
    Builder::default()
}

impl Builder {
    #[must_use]
    pub fn with_config(mut self, config: CliproxyRuntimeConfig) -> Self {
        self.config = Some(config);
        self
    }

    #[must_use]
    pub fn with_config_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config_path = Some(path.into());
        self
    }

    #[must_use]
    pub fn with_token_client_provider(mut self, provider: Arc<dyn TokenClientProvider>) -> Self {
        self.token_provider = Some(provider);
        self
    }

    #[must_use]
    pub fn with_api_key_client_provider(mut self, provider: Arc<dyn ApiKeyClientProvider>) -> Self {
        self.api_key_provider = Some(provider);
        self
    }

    #[must_use]
    pub fn with_watcher_factory(mut self, factory: Arc<dyn WatcherFactory>) -> Self {
        self.watcher_factory = Some(factory);
        self
    }

    #[must_use]
    pub fn with_hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = hooks;
        self
    }

    #[must_use]
    pub fn with_auth_manager(mut self, manager: Arc<LegacyAuthManager>) -> Self {
        self.auth_manager = Some(manager);
        self
    }

    #[must_use]
    pub fn with_request_access_manager(mut self, manager: Arc<AccessManager>) -> Self {
        self.access_manager = Some(manager);
        self
    }

    #[must_use]
    pub fn with_core_auth_manager(mut self, manager: Arc<AuthManager>) -> Self {
        self.core_manager = Some(manager);
        self
    }

    #[must_use]
    pub fn with_cooldown_state_store(mut self, store: Arc<dyn CooldownStateStore>) -> Self {
        self.cooldown_state_store = Some(store);
        self
    }

    #[must_use]
    pub fn with_plugin_host(mut self, host: Arc<dyn PluginHost>) -> Self {
        self.plugin_host = Some(host);
        self
    }

    #[must_use]
    pub fn with_round_tripper_provider(
        mut self,
        provider: Arc<DefaultRoundTripperProvider>,
    ) -> Self {
        self.round_tripper_provider = Some(provider);
        self
    }

    #[must_use]
    pub fn with_server_options(mut self, options: Vec<ServerOption>) -> Self {
        self.server_options.extend(options);
        self
    }

    #[must_use]
    pub fn with_local_management_password(mut self, password: impl Into<String>) -> Self {
        let password = password.into();
        if !password.is_empty() {
            self.server_options
                .push(options::with_local_management_password(password));
        }
        self
    }

    #[must_use]
    pub fn with_post_auth_hook(mut self, hook: Option<PostAuthHook>) -> Self {
        if let Some(hook) = hook {
            self.post_auth_hook = Some(hook);
        }
        self
    }

    #[must_use]
    pub fn with_persisted_auth_update_sink(
        mut self,
        sink: Arc<dyn PersistedAuthUpdateSink>,
    ) -> Self {
        self.persisted_auth_update_sink = Some(sink);
        self
    }

    pub fn build(self) -> Result<ServiceAssembly, BuilderError> {
        let config = self.config.ok_or(BuilderError {
            kind: BuilderErrorKind::ConfigurationRequired,
            config: None,
            plugin: None,
            credential_path: None,
        })?;
        let config_path = self
            .config_path
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or(BuilderError {
                kind: BuilderErrorKind::ConfigPathRequired,
                config: None,
                plugin: None,
                credential_path: None,
            })?;
        let credential_path = invalid_credential_weight_path(&config);
        let config = config.validate().map_err(|error| BuilderError {
            kind: BuilderErrorKind::InvalidConfiguration,
            config: Some(error),
            plugin: None,
            credential_path,
        })?;

        let access_manager = self
            .access_manager
            .unwrap_or_else(|| Arc::new(AccessManager::new()));
        if let Some(plugin_host) = &self.plugin_host {
            plugin_host
                .apply_config(&config)
                .map_err(|error| BuilderError {
                    kind: BuilderErrorKind::PluginHost,
                    config: None,
                    plugin: Some(error),
                    credential_path: None,
                })?;
            plugin_host
                .register_frontend_auth_providers()
                .map_err(|error| BuilderError {
                    kind: BuilderErrorKind::PluginHost,
                    config: None,
                    plugin: Some(error),
                    credential_path: None,
                })?;
            access_manager.set_shared_providers(&plugin_host.access_providers());
        }

        let mut requirements = Vec::new();
        if self.api_key_provider.is_none() {
            requirements.push(ServiceBindingRequirement::ApiKeyClientProvider);
        }
        if self.watcher_factory.is_none() {
            requirements.push(ServiceBindingRequirement::WatcherFactory);
        }
        if self.core_manager.is_none() {
            requirements.push(ServiceBindingRequirement::CoreAuthManager);
        }
        if self.plugin_host.is_none() {
            requirements.push(ServiceBindingRequirement::PluginHost);
        }
        if self.persisted_auth_update_sink.is_none() {
            requirements.push(ServiceBindingRequirement::PersistedAuthUpdateSink);
        }

        let token_provider = self.token_provider.unwrap_or_else(|| {
            Arc::new(new_file_token_client_provider()) as Arc<dyn TokenClientProvider>
        });
        let auth_manager = self
            .auth_manager
            .unwrap_or_else(|| Arc::new(LegacyAuthManager::default()));
        let round_tripper_provider = self
            .round_tripper_provider
            .unwrap_or_else(|| Arc::new(new_default_round_tripper_provider()));

        Ok(ServiceAssembly {
            config,
            config_path,
            token_provider,
            api_key_provider: self.api_key_provider,
            watcher_factory: self.watcher_factory,
            hooks: self.hooks,
            auth_manager,
            access_manager,
            core_manager: self.core_manager,
            cooldown_state_store: self.cooldown_state_store,
            plugin_host: self.plugin_host,
            round_tripper_provider,
            post_auth_hook: self.post_auth_hook,
            persisted_auth_update_sink: self.persisted_auth_update_sink,
            server_options: self.server_options,
            requirements,
        })
    }
}

fn invalid_credential_weight_path(config: &CliproxyRuntimeConfig) -> Option<String> {
    config
        .claude_accounts
        .iter()
        .position(|account| {
            crate::internal::config::weight::validate_credential_weight(Some(account.weight))
                .is_err()
        })
        .map(|index| format!("credential weights: claude-accounts[{index}].weight"))
        .or_else(|| {
            config
                .codex_accounts
                .iter()
                .position(|account| {
                    crate::internal::config::weight::validate_credential_weight(Some(
                        account.weight,
                    ))
                    .is_err()
                })
                .map(|index| format!("credential weights: codex-accounts[{index}].weight"))
        })
        .or_else(|| {
            config
                .antigravity_accounts
                .iter()
                .position(|account| {
                    crate::internal::config::weight::validate_credential_weight(Some(
                        account.weight,
                    ))
                    .is_err()
                })
                .map(|index| format!("credential weights: antigravity-accounts[{index}].weight"))
        })
}
