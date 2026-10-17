// ref: internal/pluginhost/model_router_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: model route decisions cross isolated RPC without host mutation authority
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use serde_json::value::to_raw_value;
use tokio::sync::mpsc;

use crate::sdk::pluginabi::{Envelope, METHOD_MODEL_ROUTE};
use crate::sdk::pluginapi::{ModelRouteRequest, ModelRouter};

use super::abi::{PluginCall, PluginClient, PluginFuture, PluginStream};
use super::model_router::RpcModelRouter;
use super::rpc_client::RpcPluginClient;

struct Client(Mutex<Option<PluginCall>>);

impl PluginClient for Client {
    fn call<'a>(&'a self, call: PluginCall) -> PluginFuture<'a, Envelope> {
        Box::pin(async move {
            *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(call);
            Ok(Envelope::success(Some(
                to_raw_value(&serde_json::json!({
                    "Handled": true,
                    "TargetKind": "provider",
                    "Target": "anthropic",
                    "TargetModel": "claude-sonnet",
                    "Reason": "policy"
                }))
                .unwrap(),
            )))
        })
    }

    fn call_stream<'a>(&'a self, _call: PluginCall) -> PluginFuture<'a, PluginStream> {
        Box::pin(async {
            let (_sender, receiver) = mpsc::channel(1);
            Ok(PluginStream { chunks: receiver })
        })
    }

    fn shutdown<'a>(&'a self) -> PluginFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
