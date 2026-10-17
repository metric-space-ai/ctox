// ref: internal/pluginhost/host_model_stream_callbacks_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: owner-bound model callback stream lifecycle over process RPC
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use serde_json::value::to_raw_value;
use tokio::sync::mpsc;

use crate::sdk::pluginabi::{METHOD_HOST_MODEL_EXECUTE_STREAM, METHOD_HOST_MODEL_STREAM_READ};
use crate::sdk::pluginapi::{
    HostModelExecutionRequest, HostModelExecutionResponse, HostModelStreamReadRequest,
    HostModelStreamReadResponse, HostModelStreamResponse, PluginFuture,
};

use super::callback_contexts::{CallbackAuthority, CallbackContextRegistry};
use super::host_callbacks::{HostCallbackRouteError, HostCallbackRouter};
use super::host_model_stream_callbacks::{
    install_host_model_callbacks, HostModelExecutionAuthority,
};
use super::model_stream_bridge::{ModelExecutionChunk, ModelExecutionStream, ModelStreamBridge};

#[derive(Default)]
struct Authority {
    callers: Mutex<Vec<String>>,
}

impl HostModelExecutionAuthority for Authority {
    fn execute<'a>(
        &'a self,
        caller_plugin_id: &'a str,
        _request: HostModelExecutionRequest,
    ) -> PluginFuture<'a, HostModelExecutionResponse> {
        Box::pin(async move {
            self.callers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(caller_plugin_id.to_owned());
            Ok(HostModelExecutionResponse::default())
        })
    }

    fn execute_stream<'a>(
        &'a self,
        caller_plugin_id: &'a str,
        _request: HostModelExecutionRequest,
    ) -> PluginFuture<'a, ModelExecutionStream> {
        Box::pin(async move {
            self.callers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(caller_plugin_id.to_owned());
            let (sender, receiver) = mpsc::channel(1);
            sender
                .send(ModelExecutionChunk {
                    payload: b"model-chunk".to_vec(),
                    error: None,
                })
                .await
                .unwrap();
            drop(sender);
            Ok(ModelExecutionStream {
                status_code: 200,
                headers: Default::default(),
                chunks: receiver,
            })
        })
    }
}
