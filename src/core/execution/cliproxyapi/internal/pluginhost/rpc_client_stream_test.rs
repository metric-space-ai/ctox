// ref: internal/pluginhost/rpc_client_stream_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: upstream stream ordering and terminal-error evidence over process IPC
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use serde_json::{json, value::to_raw_value, Value};
use tokio::sync::mpsc;

use crate::sdk::pluginabi::Envelope;

use super::abi::{PluginCall, PluginClient, PluginClientError, PluginFuture, PluginStream};
use super::rpc_client::RpcPluginClient;

struct StreamClient {
    chunks: Vec<Result<Box<serde_json::value::RawValue>, PluginClientError>>,
}

impl PluginClient for StreamClient {
    fn call<'a>(&'a self, _call: PluginCall) -> PluginFuture<'a, Envelope> {
        Box::pin(async { Err(PluginClientError::UnsupportedCapability) })
    }

    fn call_stream<'a>(&'a self, _call: PluginCall) -> PluginFuture<'a, PluginStream> {
        Box::pin(async move {
            let (sender, receiver) = mpsc::channel(4);
            for chunk in &self.chunks {
                let copy = match chunk {
                    Ok(raw) => Ok(
                        to_raw_value(&serde_json::from_str::<Value>(raw.get()).unwrap()).unwrap(),
                    ),
                    Err(error) => Err(error.clone()),
                };
                sender.send(copy).await.unwrap();
            }
            drop(sender);
            Ok(PluginStream { chunks: receiver })
        })
    }

    fn shutdown<'a>(&'a self) -> PluginFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
