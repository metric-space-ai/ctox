// ref: internal/pluginhost/adapters_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: callback IDs are injected and revoked around capability RPC
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use serde_json::{value::to_raw_value, Value};
use tokio::sync::mpsc;

use crate::sdk::pluginabi::Envelope;

use super::abi::{PluginCall, PluginClient, PluginFuture, PluginStream};
use super::adapters::RpcCapabilityClient;
use super::callback_contexts::CallbackContextRegistry;
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
                to_raw_value(&serde_json::json!({"ok": true})).unwrap(),
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
