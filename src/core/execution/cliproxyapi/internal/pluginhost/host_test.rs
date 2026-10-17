// ref: internal/pluginhost/host_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: transactional process lifecycle and immutable snapshot coverage
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::value::to_raw_value;
use tokio::sync::mpsc;

use crate::sdk::pluginabi::{
    Envelope, METHOD_AUTH_IDENTIFIER, METHOD_PLUGIN_RECONFIGURE, METHOD_PLUGIN_REGISTER,
};
use crate::sdk::pluginapi::Metadata;

use super::abi::{
    PluginArtifact, PluginCall, PluginClient, PluginClientError, PluginFuture, PluginLoader,
    PluginStream,
};
use super::config::{RuntimeConfig, RuntimeItemConfig};
use super::host::{ApplyFailureReason, PluginHost};
use super::platform::PluginFileInfo;
use super::rpc_schema::{RpcCapabilities, RpcRegistration};

struct Client {
    registration: RpcRegistration,
    calls: Mutex<Vec<String>>,
    shutdowns: AtomicUsize,
}

impl PluginClient for Client {
    fn call<'a>(&'a self, call: PluginCall) -> PluginFuture<'a, Envelope> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(call.method.clone());
            let value = match call.method.as_str() {
                METHOD_PLUGIN_REGISTER | METHOD_PLUGIN_RECONFIGURE => {
                    to_raw_value(&self.registration).unwrap()
                }
                METHOD_AUTH_IDENTIFIER => {
                    to_raw_value(&serde_json::json!({"identifier": "claude"})).unwrap()
                }
                _ => return Err(PluginClientError::UnsupportedCapability),
            };
            Ok(Envelope::success(Some(value)))
        })
    }

    fn call_stream<'a>(&'a self, _call: PluginCall) -> PluginFuture<'a, PluginStream> {
        Box::pin(async {
            let (_sender, receiver) = mpsc::channel(1);
            Ok(PluginStream { chunks: receiver })
        })
    }

    fn shutdown<'a>(&'a self) -> PluginFuture<'a, ()> {
        Box::pin(async move {
            self.shutdowns.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
    }
}

struct Loader {
    loads: AtomicUsize,
    clients: BTreeMap<String, Arc<Client>>,
}

impl PluginLoader for Loader {
    fn open<'a>(&'a self, artifact: &'a PluginArtifact) -> PluginFuture<'a, Arc<dyn PluginClient>> {
        Box::pin(async move {
            self.loads.fetch_add(1, Ordering::Relaxed);
            self.clients
                .get(&artifact.plugin_id)
                .cloned()
                .map(|client| client as Arc<dyn PluginClient>)
                .ok_or_else(|| PluginClientError::Transport("not installed".to_owned()))
        })
    }
}

fn client(name: &str, valid: bool) -> Arc<Client> {
    Arc::new(Client {
        registration: RpcRegistration {
            schema_version: 2,
            metadata: Metadata {
                name: name.to_owned(),
                ..Metadata::default()
            },
            capabilities: RpcCapabilities {
                auth_provider: valid,
                ..RpcCapabilities::default()
            },
        },
        calls: Mutex::new(Vec::new()),
        shutdowns: AtomicUsize::new(0),
    })
}

fn config(enabled: bool) -> RuntimeConfig {
    RuntimeConfig {
        enabled,
        directory: PathBuf::from("/typed/plugins"),
        items: BTreeMap::from([
            (
                "alpha".to_owned(),
                RuntimeItemConfig {
                    id: "alpha".to_owned(),
                    enabled: true,
                    priority: 10,
                    config_yaml: b"enabled: true\n".to_vec(),
                    ..RuntimeItemConfig::default()
                },
            ),
            (
                "beta".to_owned(),
                RuntimeItemConfig {
                    id: "beta".to_owned(),
                    enabled: true,
                    priority: 20,
                    config_yaml: b"enabled: true\n".to_vec(),
                    ..RuntimeItemConfig::default()
                },
            ),
        ]),
    }
}

fn files() -> Vec<PluginFileInfo> {
    ["alpha", "beta"]
        .into_iter()
        .map(|id| PluginFileInfo {
            id: id.to_owned(),
            path: PathBuf::from(format!("/typed/plugins/{id}.ctox-plugin")),
            version: Some("1.0.0".to_owned()),
        })
        .collect()
}
