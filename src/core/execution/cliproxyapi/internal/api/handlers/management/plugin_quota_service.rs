// ref: internal/api/handlers/management/plugin_quota.go @ 5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3
// Port-Status: adapted_to_ctox
// Port-Note: account/secret resolution is an injected server authority; errors never echo provider bodies.
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{ManagementQuotaProbe, ManagementQuotaProbeError, ManagementQuotaResetSource};
use crate::{
    internal::pluginhost::{
        auth_provider::HostConfigSummarySource, host::PluginHost,
        quota_provider::RegisteredQuotaProviderInfo,
    },
    sdk::pluginapi::{QuotaFetchRequest, QuotaFetchResponse, QuotaResetRequest},
};
use serde_json::{Map, Value};
use std::{sync::Arc, time::SystemTime};
#[derive(Clone)]
pub struct ManagementQuotaCredential {
    /// Opaque server-only policy/credential generation. Never sent to a plugin.
    pub binding: String,
    pub request: QuotaFetchRequest,
    pub probe: Option<Map<String, Value>>,
}
impl std::fmt::Debug for ManagementQuotaCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagementQuotaCredential")
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagementQuotaCredentialError {
    Unavailable,
}
pub trait ManagementQuotaCredentialSource: Send + Sync {
    /// Resolve only through the configured server-side credential provider.
    /// The public index, credential values and pre-scoped HTTP client must agree.
    fn resolve(
        &self,
        auth_index: &str,
    ) -> Result<Option<ManagementQuotaCredential>, ManagementQuotaCredentialError>;
    /// Under the same fence used by account/credential/policy mutations, verify
    /// the captured binding and call `publish` exactly once while it is current.
    /// A retired binding returns false without calling it; store failures return
    /// an error without publishing. No asynchronous or external I/O belongs here.
    fn with_current_binding(
        &self,
        auth_index: &str,
        binding: &str,
        publish: &mut dyn FnMut(),
    ) -> Result<bool, ManagementQuotaCredentialError>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagementPluginQuotaError {
    RuntimeUnavailable,
    AccountNotFound,
    CredentialSourceUnavailable,
    AccountContextMismatch,
    Unsupported,
    PluginNotFound,
    PluginCallFailed,
    Timeout,
    ResetDeclined,
    RoutingResetFailed,
    Probe(ManagementQuotaProbeError),
}
pub struct ManagementPluginQuotaService {
    host: Arc<PluginHost>,
    credentials: Arc<dyn ManagementQuotaCredentialSource>,
    summary: Arc<dyn HostConfigSummarySource>,
    probe: Option<Arc<ManagementQuotaProbe>>,
    routing_reset: Option<Arc<dyn ManagementQuotaResetSource>>,
}
impl ManagementPluginQuotaService {
    pub fn new(
        host: Arc<PluginHost>,
        credentials: Arc<dyn ManagementQuotaCredentialSource>,
        summary: Arc<dyn HostConfigSummarySource>,
    ) -> Self {
        Self {
            host,
            credentials,
            summary,
            probe: None,
            routing_reset: None,
        }
    }
    pub fn with_probe(mut self, probe: Arc<ManagementQuotaProbe>) -> Self {
        self.probe = Some(probe);
        self
    }
    pub fn with_routing_reset(mut self, reset: Arc<dyn ManagementQuotaResetSource>) -> Self {
        self.routing_reset = Some(reset);
        self
    }
    pub async fn providers(&self) -> Vec<RegisteredQuotaProviderInfo> {
        self.host.quota_providers().await
    }
    fn with_current<T>(
        &self,
        index: &str,
        binding: &str,
        publish: impl FnOnce() -> T,
    ) -> Result<T, ManagementPluginQuotaError> {
        let mut publish = Some(publish);
        let mut result = None;
        let accepted = self
            .credentials
            .with_current_binding(index, binding, &mut || {
                if let Some(publish) = publish.take() {
                    result = Some(publish());
                }
            })
            .map_err(|_| ManagementPluginQuotaError::CredentialSourceUnavailable)?;
        if !accepted {
            return Err(ManagementPluginQuotaError::AccountContextMismatch);
        }
        result.ok_or(ManagementPluginQuotaError::AccountContextMismatch)
    }

    fn account(
        &self,
        index: &str,
    ) -> Result<ManagementQuotaCredential, ManagementPluginQuotaError> {
        let account = self
            .credentials
            .resolve(index)
            .map_err(|_| ManagementPluginQuotaError::CredentialSourceUnavailable)?
            .ok_or(ManagementPluginQuotaError::AccountNotFound)?;
        if account.request.auth_index != index
            || account.binding.is_empty()
            || account.binding.len() > 256
        {
            return Err(ManagementPluginQuotaError::AccountContextMismatch);
        }
        self.with_current(index, &account.binding, || ())?;
        Ok(account)
    }
    pub async fn fetch(
        &self,
        index: &str,
        provider: &str,
        plugin: Option<&str>,
        plugin_only: bool,
    ) -> Result<QuotaFetchResponse, ManagementPluginQuotaError> {
        let mut account = self.account(index)?;
        let binding = account.binding.clone();
        if !provider.trim().is_empty() {
            account.request.provider = provider.trim().into();
        }
        if plugin_only && !plugin.is_some_and(|id| self.host.has_quota_provider_for_plugin(id)) {
            return Err(ManagementPluginQuotaError::PluginNotFound);
        }
        let result = match plugin {
            Some(id) => {
                self.host
                    .fetch_quota_by_plugin(id, account.request, self.summary.clone())
                    .await
            }
            None => {
                self.host
                    .fetch_quota(account.request, self.summary.clone())
                    .await
            }
        };
        let result = self.with_current(index, &binding, || result)?;
        let result = result.map_err(|error| {
            if error.downcast_ref::<crate::internal::pluginhost::abi::PluginClientError>()
                == Some(&crate::internal::pluginhost::abi::PluginClientError::DeadlineExceeded)
            {
                ManagementPluginQuotaError::Timeout
            } else {
                ManagementPluginQuotaError::PluginCallFailed
            }
        })?;
        if let Some(result) = result {
            return Ok(result);
        }
        if plugin_only {
            return Err(ManagementPluginQuotaError::PluginNotFound);
        }
        if let (Some(probe), Some(config)) = (&self.probe, account.probe) {
            let result = probe.fetch(index, &config, SystemTime::now());
            if let Some(result) = self
                .with_current(index, &binding, || result)?
                .map_err(ManagementPluginQuotaError::Probe)?
            {
                return Ok(result);
            }
        }
        Err(ManagementPluginQuotaError::Unsupported)
    }
    pub async fn reset(
        &self,
        index: &str,
        provider: &str,
        plugin: Option<&str>,
    ) -> Result<(), ManagementPluginQuotaError> {
        let credential = self.account(index)?;
        let binding = credential.binding;
        let mut account = credential.request;
        if !provider.trim().is_empty() {
            account.provider = provider.trim().into();
        }
        if let Some(id) = plugin {
            if !self.host.has_quota_provider_for_plugin(id) {
                return Err(ManagementPluginQuotaError::PluginNotFound);
            }
        } else if !self.host.has_quota_provider(&account.provider).await {
            return Err(ManagementPluginQuotaError::Unsupported);
        }
        self.with_current(index, &binding, || ())?;
        let request = QuotaResetRequest {
            auth_index: account.auth_index,
            auth_id: account.auth_id,
            provider: account.provider,
            storage_json: account.storage_json,
            metadata: account.metadata,
            attributes: account.attributes,
            host: account.host,
            http_client: account.http_client,
        };
        let result = match plugin {
            Some(id) => {
                self.host
                    .reset_quota_by_plugin(id, request, self.summary.clone())
                    .await
            }
            None => self.host.reset_quota(request, self.summary.clone()).await,
        };
        let result = self.with_current(index, &binding, || result)?;
        let result = result
            .map_err(|error| {
                if error.downcast_ref::<crate::internal::pluginhost::abi::PluginClientError>()
                    == Some(&crate::internal::pluginhost::abi::PluginClientError::DeadlineExceeded)
                {
                    ManagementPluginQuotaError::Timeout
                } else {
                    ManagementPluginQuotaError::PluginCallFailed
                }
            })?
            .ok_or(ManagementPluginQuotaError::Unsupported)?;
        if !result.success {
            return Err(ManagementPluginQuotaError::ResetDeclined);
        }
        // The provider acknowledgement must precede any routing cooldown mutation.
        if let Some(reset) = &self.routing_reset {
            self.with_current(index, &binding, || reset.reset_by_index(index))?
                .map_err(|_| ManagementPluginQuotaError::RoutingResetFailed)?
                .ok_or(ManagementPluginQuotaError::RoutingResetFailed)?;
        }
        Ok(())
    }
}
