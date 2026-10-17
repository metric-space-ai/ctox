// ref: internal/pluginhost/rpc_client_error_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: upstream RPC error semantics over the isolated process client
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use serde_json::{json, value::to_raw_value, Value};
use tokio::sync::mpsc;

use crate::sdk::pluginabi::{Envelope, Error};

use super::abi::{PluginCall, PluginClient, PluginClientError, PluginFuture, PluginStream};
use super::rpc_client::{sanitize_plugin_value, RpcPluginClient};

struct EnvelopeClient(Mutex<Option<Envelope>>);

impl PluginClient for EnvelopeClient {
    fn call<'a>(&'a self, _call: PluginCall) -> PluginFuture<'a, Envelope> {
        Box::pin(async move {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .ok_or(PluginClientError::Closed)
        })
    }

    fn call_stream<'a>(&'a self, _call: PluginCall) -> PluginFuture<'a, PluginStream> {
        Box::pin(async move {
            let (_sender, receiver) = mpsc::channel(1);
            Ok(PluginStream { chunks: receiver })
        })
    }

    fn shutdown<'a>(&'a self) -> PluginFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
