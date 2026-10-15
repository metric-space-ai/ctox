// ref: sdk/pluginhost/host.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: public SDK facade preserves the process/capability boundary
// License: MIT (upstream); modifications AGPL-3.0-only

//! Public, provider-neutral facade for the process-isolated plugin host.
//!
//! Upstream constructs an in-process dynamic-library host. CTOX deliberately
//! requires callers to inject a [`PluginLoader`], so this facade cannot acquire
//! filesystem, process, network, or secret authority by itself.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::internal::config::config_normalization::{
    PluginInstanceConfig as InternalPluginConfig, PluginsConfig,
};
use crate::internal::pluginhost::abi::PluginLoader;
use crate::internal::pluginhost::adapters::{plugin_from_record, CapabilityClientConfigError};
use crate::internal::pluginhost::auth_provider::HostConfigSummarySource;
use crate::internal::pluginhost::config::{
    runtime_config_from_config, ConfigError as InternalConfigError,
};
use crate::internal::pluginhost::host::{ApplyReport, PluginHost};
use crate::internal::pluginhost::platform::PluginFileInfo;
use crate::sdk::pluginapi::{
    AuthLoginPollRequest, AuthLoginPollResponse, AuthLoginStartRequest, AuthLoginStartResponse,
    AuthModelRequest, AuthParseRequest, AuthParseResponse, AuthRefreshRequest, AuthRefreshResponse,
    HostConfigSummary, ModelAlias, ModelResponse, Plugin, SchedulerPickRequest,
    SchedulerPickResponse, StaticModelRequest,
};

pub use crate::sdk::pluginapi::{ModelInfo, ThinkingSupport};

/// Public plugin-host configuration used by embedders.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuntimeConfig {
    pub enabled: bool,
    /// Must already be resolved by CTOX typed configuration when enabled.
    pub directory: PathBuf,
    pub auth_dir: String,
    pub proxy_url: String,
    pub force_model_prefix: bool,
    pub oauth_model_alias: BTreeMap<String, Vec<OAuthModelAlias>>,
    pub oauth_excluded_models: BTreeMap<String, Vec<String>>,
    pub configs: BTreeMap<String, PluginInstanceConfig>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OAuthModelAlias {
    pub name: String,
    pub alias: String,
    pub fork: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PluginInstanceConfig {
    pub enabled: Option<bool>,
    pub priority: i32,
    pub raw: serde_yaml::Value,
}

impl Default for PluginInstanceConfig {
    fn default() -> Self {
        Self {
            enabled: Some(false),
            priority: 0,
            raw: serde_yaml::Value::Mapping(Default::default()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AuthModelResult {
    pub provider: String,
    pub models: Vec<ModelInfo>,
    pub auth_update: Option<crate::sdk::pluginapi::AuthData>,
    pub handled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredPluginMenu {
    pub path: String,
    pub menu: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredPluginInfo {
    pub id: String,
    pub priority: i32,
    pub metadata: crate::sdk::pluginapi::Metadata,
    pub supports_oauth: bool,
    pub oauth_provider: Option<String>,
    pub capabilities: Vec<String>,
    pub menus: Vec<RegisteredPluginMenu>,
}

/// SDK facade over the isolated internal host.
pub struct Host {
    inner: Arc<PluginHost>,
    summary: Arc<SummarySource>,
    loading: Arc<RwLock<BTreeMap<String, usize>>>,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Host")
            .field("registered_plugins", &self.inner.snapshot().records().len())
            .finish_non_exhaustive()
    }
}

impl Host {
    /// Constructs a host from an explicitly injected process loader.
    pub fn new(loader: Arc<dyn PluginLoader>) -> Self {
        Self {
            inner: Arc::new(PluginHost::new(loader)),
            summary: Arc::new(SummarySource::default()),
            loading: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    /// Applies already-normalized configuration to an explicit discovery set.
    pub async fn apply_config(
        &self,
        config: RuntimeConfig,
        files: &[PluginFileInfo],
    ) -> Result<ApplyReport, HostConfigError> {
        let loading = if config.enabled {
            files
                .iter()
                .filter(|file| {
                    config
                        .configs
                        .get(&file.id)
                        .is_some_and(|item| item.enabled.unwrap_or(false))
                })
                .map(|file| file.id.clone())
                .collect()
        } else {
            BTreeSet::new()
        };
        let _loading = LoadingLease::new(self.loading.clone(), loading);
        let (internal, summary) = translate_config(config)?;
        let report = self.inner.apply_config(&internal, files).await;
        self.summary.replace(summary);
        Ok(report)
    }

    pub async fn shutdown_all(&self) {
        self.inner.shutdown().await;
    }

    pub async fn unload_plugin(&self, id: &str) -> bool {
        self.inner.unload(id).await
    }

    pub fn plugin_busy(&self, id: &str) -> bool {
        let id = id.trim();
        !id.is_empty()
            && (self
                .loading
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_key(id)
                || self.inner.snapshot().record(id).is_some())
    }

    pub fn plugin_registered(&self, id: &str) -> bool {
        self.plugin_busy(id)
    }

    pub fn registered_plugins(&self) -> Vec<RegisteredPluginInfo> {
        self.inner
            .snapshot()
            .registered_plugins()
            .into_iter()
            .map(|item| RegisteredPluginInfo {
                id: item.id,
                priority: item.priority,
                metadata: item.metadata,
                supports_oauth: item.oauth_provider.is_some(),
                oauth_provider: item.oauth_provider,
                capabilities: item.capabilities,
                // Management routes remain owned by CTOX's policy-gated router.
                menus: Vec::new(),
            })
            .collect()
    }

    pub fn has_auth_provider(&self, provider: &str) -> bool {
        let provider = normalize_provider(provider);
        !provider.is_empty()
            && self.registered_plugins().iter().any(|plugin| {
                plugin
                    .oauth_provider
                    .as_deref()
                    .map(normalize_provider)
                    .as_deref()
                    == Some(provider.as_str())
            })
    }

    pub fn has_scheduler(&self) -> bool {
        self.inner
            .snapshot()
            .records()
            .iter()
            .any(|record| record.capabilities.scheduler)
    }

    pub async fn parse_auth(
        &self,
        request: AuthParseRequest,
    ) -> Result<(Option<crate::sdk::pluginapi::AuthData>, bool), HostCallError> {
        let (auths, handled) = self.parse_auths(request).await?;
        Ok((auths.into_iter().next(), handled))
    }

    pub async fn parse_auths(
        &self,
        request: AuthParseRequest,
    ) -> Result<(Vec<crate::sdk::pluginapi::AuthData>, bool), HostCallError> {
        let requested = normalize_provider(&request.provider);
        for plugin in self.plugins()? {
            let Some(provider) = plugin.capabilities.auth_provider else {
                continue;
            };
            if !requested.is_empty() && normalize_provider(provider.identifier()) != requested {
                continue;
            }
            let response = provider
                .parse_auth(request.clone())
                .await
                .map_err(|_| HostCallError::PluginCall)?;
            if !response.handled {
                continue;
            }
            return Ok((auths_from_response(response), true));
        }
        Ok((Vec::new(), false))
    }

    pub async fn start_login(
        &self,
        request: AuthLoginStartRequest,
    ) -> Result<(AuthLoginStartResponse, bool), HostCallError> {
        let provider = normalize_provider(&request.provider);
        let Some(capability) = self.auth_provider(&provider)? else {
            return Ok((AuthLoginStartResponse::default(), false));
        };
        capability
            .start_login(request)
            .await
            .map(|response| (response, true))
            .map_err(|_| HostCallError::PluginCall)
    }

    pub async fn poll_login(
        &self,
        request: AuthLoginPollRequest,
    ) -> Result<(AuthLoginPollResponse, bool), HostCallError> {
        let provider = normalize_provider(&request.provider);
        let Some(capability) = self.auth_provider(&provider)? else {
            return Ok((AuthLoginPollResponse::default(), false));
        };
        capability
            .poll_login(request)
            .await
            .map(|response| (response, true))
            .map_err(|_| HostCallError::PluginCall)
    }

    pub async fn refresh_auth(
        &self,
        request: AuthRefreshRequest,
    ) -> Result<(Option<AuthRefreshResponse>, bool), HostCallError> {
        let provider = normalize_provider(&request.auth_provider);
        let Some(capability) = self.auth_provider(&provider)? else {
            return Ok((None, false));
        };
        capability
            .refresh_auth(request)
            .await
            .map(|response| (Some(response), true))
            .map_err(|_| HostCallError::PluginCall)
    }

    pub async fn models_for_auth(
        &self,
        request: AuthModelRequest,
    ) -> Result<AuthModelResult, HostCallError> {
        let provider = normalize_provider(&request.auth_provider);
        for plugin in self.plugins()? {
            let Some(model_provider) = plugin.capabilities.model_provider else {
                continue;
            };
            if let Some(auth) = plugin.capabilities.auth_provider.as_ref() {
                if normalize_provider(auth.identifier()) != provider {
                    continue;
                }
            }
            let response = model_provider
                .models_for_auth(request.clone())
                .await
                .map_err(|_| HostCallError::PluginCall)?;
            if !response.provider.trim().is_empty()
                && normalize_provider(&response.provider) != provider
            {
                continue;
            }
            return Ok(model_result(response, &provider));
        }
        Ok(AuthModelResult::default())
    }

    pub async fn models_for_provider(
        &self,
        provider: &str,
    ) -> Result<Vec<ModelInfo>, HostCallError> {
        let provider = normalize_provider(provider);
        if provider.is_empty() {
            return Ok(Vec::new());
        }
        for plugin in self.plugins()? {
            let Some(model_provider) = plugin.capabilities.model_provider else {
                continue;
            };
            let response = model_provider
                .static_models(StaticModelRequest {
                    plugin: plugin.metadata,
                    host: self.summary.snapshot(),
                })
                .await
                .map_err(|_| HostCallError::PluginCall)?;
            if normalize_provider(&response.provider) == provider {
                return Ok(response.models);
            }
        }
        Ok(Vec::new())
    }

    pub async fn pick_auth(
        &self,
        mut request: SchedulerPickRequest,
    ) -> Result<(SchedulerPickResponse, bool), HostCallError> {
        for plugin in self.plugins()? {
            let Some(scheduler) = plugin.capabilities.scheduler else {
                continue;
            };
            request.plugin = plugin.metadata;
            let response = scheduler
                .pick(request)
                .await
                .map_err(|_| HostCallError::PluginCall)?;
            let handled = response.handled;
            return Ok((response, handled));
        }
        Ok((SchedulerPickResponse::default(), false))
    }

    fn plugins(&self) -> Result<Vec<Plugin>, HostCallError> {
        let snapshot = self.inner.snapshot();
        snapshot
            .records()
            .iter()
            .map(|record| {
                plugin_from_record(
                    record,
                    self.inner.callback_contexts().clone(),
                    self.inner.streams().clone(),
                    self.summary.clone(),
                )
                .map_err(HostCallError::Capability)
            })
            .collect()
    }

    fn auth_provider(
        &self,
        provider: &str,
    ) -> Result<Option<Arc<dyn crate::sdk::pluginapi::AuthProvider>>, HostCallError> {
        if provider.is_empty() {
            return Ok(None);
        }
        Ok(self.plugins()?.into_iter().find_map(|plugin| {
            plugin
                .capabilities
                .auth_provider
                .filter(|candidate| normalize_provider(candidate.identifier()) == provider)
        }))
    }
}

#[derive(Default)]
struct SummarySource {
    value: RwLock<HostConfigSummary>,
}

impl SummarySource {
    fn replace(&self, value: HostConfigSummary) {
        *self
            .value
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = value;
    }
}

impl HostConfigSummarySource for SummarySource {
    fn snapshot(&self) -> HostConfigSummary {
        self.value
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

struct LoadingLease {
    loading: Arc<RwLock<BTreeMap<String, usize>>>,
    ids: BTreeSet<String>,
}

impl LoadingLease {
    fn new(loading: Arc<RwLock<BTreeMap<String, usize>>>, ids: BTreeSet<String>) -> Self {
        let mut loading_guard = loading
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for id in &ids {
            *loading_guard.entry(id.clone()).or_default() += 1;
        }
        drop(loading_guard);
        Self { loading, ids }
    }
}

impl Drop for LoadingLease {
    fn drop(&mut self) {
        let mut loading = self
            .loading
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for id in &self.ids {
            if let Some(count) = loading.get_mut(id) {
                *count -= 1;
                if *count == 0 {
                    loading.remove(id);
                }
            }
        }
    }
}

fn translate_config(
    config: RuntimeConfig,
) -> Result<
    (
        crate::internal::pluginhost::config::RuntimeConfig,
        HostConfigSummary,
    ),
    HostConfigError,
> {
    let summary = HostConfigSummary {
        auth_dir: config.auth_dir,
        proxy_url: config.proxy_url,
        force_model_prefix: config.force_model_prefix,
        oauth_model_alias: config
            .oauth_model_alias
            .iter()
            .map(|(provider, aliases)| {
                (
                    normalize_provider(provider),
                    aliases
                        .iter()
                        .map(|alias| ModelAlias {
                            name: alias.name.clone(),
                            alias: alias.alias.clone(),
                        })
                        .collect(),
                )
            })
            .filter(|(provider, aliases): &(String, Vec<ModelAlias>)| {
                !provider.is_empty() && !aliases.is_empty()
            })
            .collect(),
        excluded_models: config.oauth_excluded_models.clone(),
    };
    let plugins = PluginsConfig {
        enabled: config.enabled,
        dir: config.directory.to_string_lossy().into_owned(),
        configs: config
            .configs
            .into_iter()
            .map(|(id, item)| {
                (
                    id,
                    InternalPluginConfig {
                        enabled: item.enabled,
                        priority: item.priority,
                        raw: item.raw,
                    },
                )
            })
            .collect(),
        ..PluginsConfig::default()
    };
    runtime_config_from_config(&plugins)
        .map(|runtime| (runtime, summary))
        .map_err(HostConfigError::Internal)
}

fn auths_from_response(response: AuthParseResponse) -> Vec<crate::sdk::pluginapi::AuthData> {
    if response.auths.is_empty() {
        vec![response.auth]
    } else {
        response.auths
    }
}

fn model_result(response: ModelResponse, fallback_provider: &str) -> AuthModelResult {
    let has_update = response.auth_update != crate::sdk::pluginapi::AuthData::default();
    AuthModelResult {
        provider: if response.provider.trim().is_empty() {
            fallback_provider.to_owned()
        } else {
            normalize_provider(&response.provider)
        },
        models: response.models,
        auth_update: has_update.then_some(response.auth_update),
        handled: true,
    }
}

fn normalize_provider(provider: &str) -> String {
    provider.trim().to_ascii_lowercase()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostConfigError {
    Internal(InternalConfigError),
}

impl std::fmt::Display for HostConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("plugin host configuration is invalid")
    }
}

impl std::error::Error for HostConfigError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostCallError {
    Capability(CapabilityClientConfigError),
    PluginCall,
}

impl std::fmt::Display for HostCallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Capability(_) => "plugin capability configuration is invalid",
            Self::PluginCall => "plugin capability call failed",
        })
    }
}

impl std::error::Error for HostCallError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::pluginhost::abi::{
        PluginArtifact, PluginClient, PluginClientError, PluginFuture,
    };

    struct RejectingLoader;

    impl PluginLoader for RejectingLoader {
        fn open<'a>(
            &'a self,
            _artifact: &'a PluginArtifact,
        ) -> PluginFuture<'a, Arc<dyn PluginClient>> {
            Box::pin(async { Err(PluginClientError::Closed) })
        }
    }

}
