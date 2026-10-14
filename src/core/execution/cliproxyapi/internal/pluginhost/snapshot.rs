// ref: internal/pluginhost/snapshot.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: immutable process-plugin snapshot replaces in-process capability objects
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::OnceCell;
type QuotaSupportedCache = Arc<Mutex<BTreeMap<String, Arc<OnceCell<Vec<String>>>>>>;

use crate::sdk::pluginapi::Metadata;

use super::rpc_client::RpcPluginClient;
use super::rpc_schema::RpcCapabilities;

#[derive(Clone)]
pub struct CapabilityRecord {
    pub id: String,
    pub path: PathBuf,
    pub version: Option<String>,
    pub priority: i32,
    pub metadata: Metadata,
    pub capabilities: RpcCapabilities,
    pub identifiers: BTreeMap<String, String>,
    pub client: RpcPluginClient,
}

impl std::fmt::Debug for CapabilityRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CapabilityRecord")
            .field("id", &self.id)
            .field("path", &self.path)
            .field("version", &self.version)
            .field("priority", &self.priority)
            .field("metadata", &self.metadata)
            .field("capabilities", &self.capabilities)
            .field(
                "identifier_keys",
                &self.identifiers.keys().collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    enabled: bool,
    records: Vec<Arc<CapabilityRecord>>,
    quota_supported: QuotaSupportedCache,
}

impl Snapshot {
    pub fn new(enabled: bool, mut records: Vec<Arc<CapabilityRecord>>) -> Self {
        records.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.id.cmp(&right.id))
        });
        Self {
            enabled,
            records,
            quota_supported: Default::default(),
        }
    }

    pub(crate) fn quota_supported_cell(&self, id: &str) -> Arc<OnceCell<Vec<String>>> {
        self.quota_supported
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(id.to_owned())
            .or_default()
            .clone()
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn records(&self) -> &[Arc<CapabilityRecord>] {
        &self.records
    }

    pub fn record(&self, id: &str) -> Option<&Arc<CapabilityRecord>> {
        let id = id.trim();
        self.records.iter().find(|record| record.id == id)
    }

    pub fn registered_plugins(&self) -> Vec<RegisteredPluginInfo> {
        self.records
            .iter()
            .map(|record| RegisteredPluginInfo {
                id: record.id.clone(),
                priority: record.priority,
                metadata: record.metadata.clone(),
                capabilities: capability_names(&record.capabilities),
                oauth_provider: record.identifiers.get("auth_provider").cloned(),
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredPluginInfo {
    pub id: String,
    pub priority: i32,
    pub metadata: Metadata,
    pub capabilities: Vec<String>,
    pub oauth_provider: Option<String>,
}

fn capability_names(capabilities: &RpcCapabilities) -> Vec<String> {
    [
        ("model_registrar", capabilities.model_registrar),
        ("model_provider", capabilities.model_provider),
        ("auth_provider", capabilities.auth_provider),
        (
            "frontend_auth_provider",
            capabilities.frontend_auth_provider,
        ),
        ("scheduler", capabilities.scheduler),
        ("model_router", capabilities.model_router),
        ("executor", capabilities.executor),
        ("request_translator", capabilities.request_translator),
        ("request_normalizer", capabilities.request_normalizer),
        ("request_interceptor", capabilities.request_interceptor),
        (
            "request_lifecycle_plugin",
            capabilities.request_lifecycle_plugin,
        ),
        ("response_translator", capabilities.response_translator),
        (
            "response_before_translator",
            capabilities.response_before_translator,
        ),
        (
            "response_after_translator",
            capabilities.response_after_translator,
        ),
        ("response_interceptor", capabilities.response_interceptor),
        (
            "stream_chunk_interceptor",
            capabilities.stream_chunk_interceptor,
        ),
        ("thinking_applier", capabilities.thinking_applier),
        ("usage_plugin", capabilities.usage_plugin),
        ("quota_provider", capabilities.quota_provider),
        ("command_line_plugin", capabilities.command_line_plugin),
        ("management_api", capabilities.management_api),
    ]
    .into_iter()
    .filter(|(_, enabled)| *enabled)
    .map(|(name, _)| name.to_owned())
    .collect()
}
