// ref: internal/pluginhost/quota_provider.go @ 5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3
// Port-Status: adapted_to_ctox
// Port-Note: immutable process generations replace in-process panic fusing;
// callers supply already-authorized credential context and HTTP authority.
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{
    abi::PluginClientError,
    adapters::RpcCapabilityClient,
    adapters_quota::{bounded_quota_call, RpcQuotaProvider},
    auth_provider::HostConfigSummarySource,
    host::PluginHost,
    snapshot::{CapabilityRecord, Snapshot},
};
use crate::sdk::{
    pluginabi::METHOD_QUOTA_DESCRIBE,
    pluginapi::{
        PluginExecutionError, QuotaDescribeRequest, QuotaDescribeResponse, QuotaFetchRequest,
        QuotaFetchResponse, QuotaProvider, QuotaResetRequest, QuotaResetResponse,
    },
};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RegisteredQuotaProviderInfo {
    pub plugin_id: String,
    pub provider: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub supported_providers: Vec<String>,
    pub supports_reset: bool,
}
fn normalized(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
fn quota_record(record: &CapabilityRecord) -> bool {
    record.capabilities.quota_provider && record.identifiers.contains_key("quota_provider")
}
impl PluginHost {
    fn quota_record_current(&self, record: &Arc<CapabilityRecord>) -> bool {
        self.snapshot()
            .record(&record.id)
            .is_some_and(|current| Arc::ptr_eq(current, record))
    }
    pub fn quota_provider_identifiers(&self) -> Vec<String> {
        let mut out = vec![];
        for record in self.snapshot().records() {
            if quota_record(record) {
                let id = record.identifiers["quota_provider"].clone();
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        out
    }
    async fn describe_quota_record(
        &self,
        record: &Arc<CapabilityRecord>,
    ) -> Result<Option<QuotaDescribeResponse>, PluginClientError> {
        if !quota_record(record) || !self.quota_record_current(record) {
            return Ok(None);
        }
        let response = bounded_quota_call(record.client.call(
            METHOD_QUOTA_DESCRIBE,
            &QuotaDescribeRequest {
                plugin: record.metadata.clone(),
            },
            None,
        ))
        .await?;
        Ok(self.quota_record_current(record).then_some(response))
    }
    async fn quota_supported(
        &self,
        snapshot: &Snapshot,
        record: &Arc<CapabilityRecord>,
    ) -> Vec<String> {
        let cell = snapshot.quota_supported_cell(&record.id);
        cell.get_or_try_init(|| async {
            self.describe_quota_record(record)
                .await?
                .map(|desc| desc.supported_providers)
                .ok_or(PluginClientError::Closed)
        })
        .await
        .cloned()
        .unwrap_or_default()
    }
    pub async fn quota_providers(&self) -> Vec<RegisteredQuotaProviderInfo> {
        let snapshot = self.snapshot();
        let mut out = vec![];
        for record in snapshot.records() {
            if !quota_record(record) || !self.quota_record_current(record) {
                continue;
            }
            let (desc, cache) = match self.describe_quota_record(record).await {
                Ok(Some(desc)) => (desc, true),
                Ok(None) => continue,
                Err(_) => (Default::default(), false),
            };
            if !self.quota_record_current(record) {
                continue;
            }
            if cache {
                let _ = snapshot
                    .quota_supported_cell(&record.id)
                    .set(desc.supported_providers.clone());
            }
            let provider = record.identifiers["quota_provider"].clone();
            let supported = if desc.supported_providers.is_empty() {
                vec![provider.clone()]
            } else {
                desc.supported_providers
            };
            let display_name = if !desc.display_name.is_empty() {
                desc.display_name
            } else if !record.metadata.name.is_empty() {
                record.metadata.name.clone()
            } else {
                provider.clone()
            };
            out.push(RegisteredQuotaProviderInfo {
                plugin_id: record.id.clone(),
                provider,
                display_name,
                supported_providers: supported,
                supports_reset: desc.supports_reset,
            });
        }
        out.retain(|item| {
            snapshot
                .record(&item.plugin_id)
                .is_some_and(|record| self.quota_record_current(record))
        });
        out
    }
    /// Exact quota identifier, plugin id and auth identifier precede every alias,
    /// even when an alias is advertised by a higher-priority plugin.
    pub async fn quota_provider_plugin_ids(&self) -> BTreeMap<String, String> {
        let snapshot = self.snapshot();
        let mut out = BTreeMap::new();
        for record in snapshot.records() {
            if !quota_record(record) || !self.quota_record_current(record) {
                continue;
            }
            for key in [
                Some(record.identifiers["quota_provider"].as_str()),
                Some(record.id.as_str()),
                record.identifiers.get("auth_provider").map(String::as_str),
            ]
            .into_iter()
            .flatten()
            {
                let key = normalized(key);
                if !key.is_empty() {
                    out.entry(key).or_insert_with(|| record.id.clone());
                }
            }
        }
        for record in snapshot.records() {
            if !quota_record(record) || !self.quota_record_current(record) {
                continue;
            }
            for key in self.quota_supported(&snapshot, record).await {
                let key = normalized(&key);
                if !key.is_empty() {
                    out.entry(key).or_insert_with(|| record.id.clone());
                }
            }
        }
        out.retain(|_, id| {
            snapshot
                .record(id)
                .is_some_and(|record| self.quota_record_current(record))
        });
        out
    }
    async fn quota_provider_record(&self, provider: &str) -> Option<Arc<CapabilityRecord>> {
        let provider = normalized(provider);
        if provider.is_empty() {
            return None;
        }
        let snapshot = self.snapshot();
        for record in snapshot.records() {
            if !quota_record(record) || !self.quota_record_current(record) {
                continue;
            }
            if normalized(&record.identifiers["quota_provider"]) == provider
                || normalized(&record.id) == provider
                || record
                    .identifiers
                    .get("auth_provider")
                    .is_some_and(|id| normalized(id) == provider)
            {
                return Some(record.clone());
            }
        }
        for record in snapshot.records() {
            if !quota_record(record) || !self.quota_record_current(record) {
                continue;
            }
            if self
                .quota_supported(&snapshot, record)
                .await
                .iter()
                .any(|id| normalized(id) == provider)
                && self.quota_record_current(record)
            {
                return Some(record.clone());
            }
        }
        None
    }
    pub async fn has_quota_provider(&self, provider: &str) -> bool {
        self.quota_provider_record(provider).await.is_some()
    }
    fn quota_record_by_plugin(&self, id: &str) -> Option<Arc<CapabilityRecord>> {
        self.snapshot()
            .record(id.trim())
            .filter(|record| quota_record(record))
            .cloned()
    }
    pub fn has_quota_provider_for_plugin(&self, id: &str) -> bool {
        self.quota_record_by_plugin(id).is_some()
    }
    pub async fn describe_quota(
        &self,
        id: &str,
    ) -> Result<Option<QuotaDescribeResponse>, PluginClientError> {
        let record = match self.quota_record_by_plugin(id) {
            Some(record) => Some(record),
            None => self.quota_provider_record(id).await,
        };
        match record {
            Some(record) => self.describe_quota_record(&record).await,
            None => Ok(None),
        }
    }
    fn quota_adapter(
        &self,
        record: &CapabilityRecord,
        host: Arc<dyn HostConfigSummarySource>,
    ) -> Result<RpcQuotaProvider, PluginExecutionError> {
        let client = RpcCapabilityClient::new(
            &record.id,
            record.client.clone(),
            self.callback_contexts().clone(),
        )
        .map_err(|error| Arc::new(error) as PluginExecutionError)?;
        RpcQuotaProvider::new(&record.identifiers["quota_provider"], client, host)
            .map_err(|error| Arc::new(error) as PluginExecutionError)
    }
    /// Credential material and the account-scoped HTTP client must already be
    /// resolved by the caller's server-side authority. No ambient fallback.
    pub async fn fetch_quota(
        &self,
        request: QuotaFetchRequest,
        host: Arc<dyn HostConfigSummarySource>,
    ) -> Result<Option<QuotaFetchResponse>, PluginExecutionError> {
        let Some(record) = self.quota_provider_record(&request.provider).await else {
            return Ok(None);
        };
        self.fetch_quota_record(record, request, host).await
    }
    pub async fn fetch_quota_by_plugin(
        &self,
        id: &str,
        request: QuotaFetchRequest,
        host: Arc<dyn HostConfigSummarySource>,
    ) -> Result<Option<QuotaFetchResponse>, PluginExecutionError> {
        let Some(record) = self.quota_record_by_plugin(id) else {
            return Ok(None);
        };
        self.fetch_quota_record(record, request, host).await
    }
    async fn fetch_quota_record(
        &self,
        record: Arc<CapabilityRecord>,
        request: QuotaFetchRequest,
        host: Arc<dyn HostConfigSummarySource>,
    ) -> Result<Option<QuotaFetchResponse>, PluginExecutionError> {
        if !self.quota_record_current(&record) {
            return Ok(None);
        }
        let response = self
            .quota_adapter(&record, host)?
            .fetch_quota(request)
            .await?;
        if !self.quota_record_current(&record) {
            return Err(Arc::new(PluginClientError::Closed));
        }
        Ok(Some(response))
    }
    pub async fn reset_quota(
        &self,
        request: QuotaResetRequest,
        host: Arc<dyn HostConfigSummarySource>,
    ) -> Result<Option<QuotaResetResponse>, PluginExecutionError> {
        let Some(record) = self.quota_provider_record(&request.provider).await else {
            return Ok(None);
        };
        self.reset_quota_record(record, request, host).await
    }
    pub async fn reset_quota_by_plugin(
        &self,
        id: &str,
        request: QuotaResetRequest,
        host: Arc<dyn HostConfigSummarySource>,
    ) -> Result<Option<QuotaResetResponse>, PluginExecutionError> {
        let Some(record) = self.quota_record_by_plugin(id) else {
            return Ok(None);
        };
        self.reset_quota_record(record, request, host).await
    }
    async fn reset_quota_record(
        &self,
        record: Arc<CapabilityRecord>,
        request: QuotaResetRequest,
        host: Arc<dyn HostConfigSummarySource>,
    ) -> Result<Option<QuotaResetResponse>, PluginExecutionError> {
        if !self.quota_record_current(&record) {
            return Ok(None);
        }
        let response = self
            .quota_adapter(&record, host)?
            .reset_quota(request)
            .await?;
        if !self.quota_record_current(&record) {
            return Err(Arc::new(PluginClientError::Closed));
        }
        Ok(Some(response))
    }
}
