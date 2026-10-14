// ref: internal/pluginhost/rpc_client.go:646-671 @ 5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3
// Port-Status: adapted_to_ctox
// Port-Note: account-scoped HTTP authority is retained by the callback lease
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{
    adapters::{CapabilityClientConfigError, RpcCapabilityClient},
    auth_provider::HostConfigSummarySource,
};
use crate::sdk::{
    pluginabi::{METHOD_QUOTA_DESCRIBE, METHOD_QUOTA_FETCH, METHOD_QUOTA_RESET},
    pluginapi::{
        PluginFuture, QuotaDescribeRequest, QuotaDescribeResponse, QuotaFetchRequest,
        QuotaFetchResponse, QuotaProvider, QuotaResetRequest, QuotaResetResponse,
    },
};
use std::sync::Arc;
#[derive(Clone)]
pub struct RpcQuotaProvider {
    identifier: String,
    client: RpcCapabilityClient,
    host: Arc<dyn HostConfigSummarySource>,
}
impl std::fmt::Debug for RpcQuotaProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcQuotaProvider")
            .field("identifier", &self.identifier)
            .finish_non_exhaustive()
    }
}
impl RpcQuotaProvider {
    pub fn new(
        identifier: &str,
        client: RpcCapabilityClient,
        host: Arc<dyn HostConfigSummarySource>,
    ) -> Result<Self, CapabilityClientConfigError> {
        let identifier = identifier.trim().to_ascii_lowercase();
        if identifier.is_empty()
            || identifier.len() > 128
            || identifier.chars().any(char::is_control)
        {
            return Err(CapabilityClientConfigError::InvalidPluginId);
        }
        Ok(Self {
            identifier,
            client,
            host,
        })
    }
}
impl QuotaProvider for RpcQuotaProvider {
    fn identifier(&self) -> &str {
        &self.identifier
    }
    fn describe_quota<'a>(
        &'a self,
        request: QuotaDescribeRequest,
    ) -> PluginFuture<'a, QuotaDescribeResponse> {
        Box::pin(async move {
            self.client
                .client()
                .call(METHOD_QUOTA_DESCRIBE, &request, None)
                .await
                .map_err(|error| Arc::new(error) as _)
        })
    }
    fn fetch_quota<'a>(
        &'a self,
        mut request: QuotaFetchRequest,
    ) -> PluginFuture<'a, QuotaFetchResponse> {
        Box::pin(async move {
            request.host = self.host.snapshot();
            let http = request.http_client.take();
            self.client
                .call_scoped_http(METHOD_QUOTA_FETCH, &request, http, None)
                .await
                .map_err(|error| Arc::new(error) as _)
        })
    }
    fn reset_quota<'a>(
        &'a self,
        mut request: QuotaResetRequest,
    ) -> PluginFuture<'a, QuotaResetResponse> {
        Box::pin(async move {
            request.host = self.host.snapshot();
            let http = request.http_client.take();
            self.client
                .call_scoped_http(METHOD_QUOTA_RESET, &request, http, None)
                .await
                .map_err(|error| Arc::new(error) as _)
        })
    }
}
