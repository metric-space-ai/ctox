// ref: internal/pluginhost/host.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: transactional process-plugin lifecycle with immutable snapshots
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

use tokio::sync::Mutex;

use crate::sdk::pluginabi::{
    METHOD_AUTH_IDENTIFIER, METHOD_EXECUTOR_IDENTIFIER, METHOD_FRONTEND_AUTH_IDENTIFIER,
    METHOD_PLUGIN_RECONFIGURE, METHOD_PLUGIN_REGISTER, METHOD_QUOTA_IDENTIFIER,
    METHOD_THINKING_IDENTIFIER,
};

use super::abi::{PluginArtifact, PluginLoader};
use super::callback_contexts::CallbackContextRegistry;
use super::config::RuntimeConfig;
use super::platform::PluginFileInfo;
use super::rpc_client::RpcPluginClient;
use super::rpc_schema::{RpcCapabilities, RpcIdentifierResponse};
use super::snapshot::{CapabilityRecord, Snapshot};
use super::stream_bridge::StreamBridge;

pub struct PluginHost {
    loader: Arc<dyn PluginLoader>,
    apply: Mutex<()>,
    snapshot: RwLock<Arc<Snapshot>>,
    callback_contexts: CallbackContextRegistry,
    streams: StreamBridge,
}

impl std::fmt::Debug for PluginHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let snapshot = self.snapshot();
        formatter
            .debug_struct("PluginHost")
            .field("enabled", &snapshot.enabled())
            .field("active_plugins", &snapshot.records().len())
            .field("active_callback_contexts", &self.callback_contexts.len())
            .finish_non_exhaustive()
    }
}

impl PluginHost {
    pub fn new(loader: Arc<dyn PluginLoader>) -> Self {
        Self::with_callback_contexts(loader, CallbackContextRegistry::new())
    }

    /// The process loader and host must share the same callback leases.
    pub fn with_callback_contexts(
        loader: Arc<dyn PluginLoader>,
        callback_contexts: CallbackContextRegistry,
    ) -> Self {
        Self {
            loader,
            apply: Mutex::new(()),
            snapshot: RwLock::new(Arc::new(Snapshot::default())),
            callback_contexts,
            streams: StreamBridge::new(),
        }
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn callback_contexts(&self) -> &CallbackContextRegistry {
        &self.callback_contexts
    }

    pub fn streams(&self) -> &StreamBridge {
        &self.streams
    }

    pub async fn apply_config(
        &self,
        config: &RuntimeConfig,
        files: &[PluginFileInfo],
    ) -> ApplyReport {
        let _apply = self.apply.lock().await;
        if !config.enabled {
            let old = self.replace_snapshot(Snapshot::default());
            shutdown_records(old.records()).await;
            return ApplyReport::default();
        }

        let old = self.snapshot();
        let mut records = Vec::new();
        let mut failures = Vec::new();
        let mut reused = BTreeSet::new();
        for file in files {
            let Some(item) = config.items.get(&file.id) else {
                continue;
            };
            if !item.enabled {
                continue;
            }
            let prior = old
                .record(&file.id)
                .filter(|record| record.path == file.path && record.version == file.version);
            let (client, method) = match prior {
                Some(record) => (record.client.clone(), METHOD_PLUGIN_RECONFIGURE),
                None => {
                    let artifact = PluginArtifact {
                        plugin_id: file.id.clone(),
                        executable: file.path.clone(),
                    };
                    match self.loader.open(&artifact).await {
                        Ok(client) => (RpcPluginClient::new(client), METHOD_PLUGIN_REGISTER),
                        Err(_) => {
                            failures.push(ApplyFailure {
                                plugin_id: file.id.clone(),
                                reason: ApplyFailureReason::Load,
                            });
                            continue;
                        }
                    }
                }
            };
            let registration = match client
                .register(method, item.config_yaml.clone(), None)
                .await
            {
                Ok(registration)
                    if valid_registration(
                        &registration.metadata.name,
                        &registration.capabilities,
                    ) =>
                {
                    registration
                }
                Ok(_) => {
                    failures.push(ApplyFailure {
                        plugin_id: file.id.clone(),
                        reason: ApplyFailureReason::InvalidRegistration,
                    });
                    let _ = client.shutdown().await;
                    continue;
                }
                Err(_) => {
                    failures.push(ApplyFailure {
                        plugin_id: file.id.clone(),
                        reason: ApplyFailureReason::Registration,
                    });
                    let _ = client.shutdown().await;
                    continue;
                }
            };
            let identifiers = collect_identifiers(&client, &registration.capabilities).await;
            if registration.capabilities.quota_provider
                && !identifiers.contains_key("quota_provider")
            {
                failures.push(ApplyFailure {
                    plugin_id: file.id.clone(),
                    reason: ApplyFailureReason::InvalidRegistration,
                });
                let _ = client.shutdown().await;
                continue;
            }
            if prior.is_some() {
                reused.insert(file.id.clone());
            }
            records.push(Arc::new(CapabilityRecord {
                id: file.id.clone(),
                path: file.path.clone(),
                version: file.version.clone(),
                priority: item.priority,
                metadata: registration.metadata,
                capabilities: registration.capabilities,
                identifiers,
                client,
            }));
        }

        let next = Snapshot::new(true, records);
        let old = self.replace_snapshot(next);
        let retired = old
            .records()
            .iter()
            .filter(|record| !reused.contains(&record.id))
            .cloned()
            .collect::<Vec<_>>();
        shutdown_records(&retired).await;
        ApplyReport { failures }
    }

    pub async fn unload(&self, plugin_id: &str) -> bool {
        let _apply = self.apply.lock().await;
        let plugin_id = plugin_id.trim();
        let old = self.snapshot();
        let mut removed = None;
        let records = old
            .records()
            .iter()
            .filter_map(|record| {
                if record.id == plugin_id {
                    removed = Some(record.clone());
                    None
                } else {
                    Some(record.clone())
                }
            })
            .collect();
        let Some(removed) = removed else {
            return false;
        };
        self.replace_snapshot(Snapshot::new(old.enabled(), records));
        let _ = removed.client.shutdown().await;
        true
    }

    pub async fn shutdown(&self) {
        let _apply = self.apply.lock().await;
        let old = self.replace_snapshot(Snapshot::default());
        shutdown_records(old.records()).await;
    }

    fn replace_snapshot(&self, snapshot: Snapshot) -> Arc<Snapshot> {
        let mut current = self
            .snapshot
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::replace(&mut *current, Arc::new(snapshot))
    }
}

async fn collect_identifiers(
    client: &RpcPluginClient,
    capabilities: &RpcCapabilities,
) -> BTreeMap<String, String> {
    let mut identifiers = BTreeMap::new();
    for (name, enabled, method) in [
        (
            "quota_provider",
            capabilities.quota_provider,
            METHOD_QUOTA_IDENTIFIER,
        ),
        (
            "auth_provider",
            capabilities.auth_provider,
            METHOD_AUTH_IDENTIFIER,
        ),
        (
            "frontend_auth_provider",
            capabilities.frontend_auth_provider,
            METHOD_FRONTEND_AUTH_IDENTIFIER,
        ),
        (
            "executor",
            capabilities.executor,
            METHOD_EXECUTOR_IDENTIFIER,
        ),
        (
            "thinking_applier",
            capabilities.thinking_applier,
            METHOD_THINKING_IDENTIFIER,
        ),
    ] {
        if !enabled {
            continue;
        }
        let response = client
            .call::<_, RpcIdentifierResponse>(method, &BTreeMap::<String, String>::new(), None)
            .await;
        if let Ok(response) = response {
            let identifier = response.identifier.trim().to_ascii_lowercase();
            if !identifier.is_empty()
                && identifier.len() <= 128
                && (name != "quota_provider" || !identifier.chars().any(char::is_control))
            {
                identifiers.insert(name.to_owned(), identifier);
            }
        }
    }
    identifiers
}

fn valid_registration(name: &str, capabilities: &RpcCapabilities) -> bool {
    !name.trim().is_empty()
        && [
            capabilities.model_registrar,
            capabilities.model_provider,
            capabilities.auth_provider,
            capabilities.frontend_auth_provider,
            capabilities.scheduler,
            capabilities.model_router,
            capabilities.executor,
            capabilities.request_translator,
            capabilities.request_normalizer,
            capabilities.request_interceptor,
            capabilities.request_lifecycle_plugin,
            capabilities.response_translator,
            capabilities.response_before_translator,
            capabilities.response_after_translator,
            capabilities.response_interceptor,
            capabilities.stream_chunk_interceptor,
            capabilities.thinking_applier,
            capabilities.usage_plugin,
            capabilities.quota_provider,
            capabilities.command_line_plugin,
            capabilities.management_api,
        ]
        .into_iter()
        .any(|enabled| enabled)
}

async fn shutdown_records(records: &[Arc<CapabilityRecord>]) {
    for record in records {
        let _ = record.client.shutdown().await;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ApplyReport {
    pub failures: Vec<ApplyFailure>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyFailure {
    pub plugin_id: String,
    pub reason: ApplyFailureReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplyFailureReason {
    Load,
    Registration,
    InvalidRegistration,
}
