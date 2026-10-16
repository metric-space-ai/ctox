// ref: internal/pluginhost/host_callbacks_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: all callback methods are identity-bound to injected authorities
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use serde_json::{value::to_raw_value, value::RawValue, Value};

use crate::sdk::pluginabi::{Envelope, METHOD_HOST_HTTP_DO, METHOD_HOST_LOG};
use crate::sdk::pluginapi::{
    HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk, HttpStreamResponse, PluginFuture,
};
use tokio::sync::mpsc;

use super::abi::PluginClientError;
use super::callback_contexts::{CallbackAuthority, CallbackContextRegistry};
use super::host_callbacks::{
    install_standard_host_callbacks, HostCallbackFuture, HostCallbackHandler,
    HostCallbackRouteError, HostCallbackRouter, HostLogAuthority,
};
use super::http_bridge::HostHttpBridge;
use super::http_stream_bridge::{HttpStreamBridge, HttpStreamBridgeError};
use super::stream_bridge::StreamBridge;

struct Echo;

impl HostCallbackHandler for Echo {
    fn call<'a>(
        &'a self,
        _authority: &'a CallbackAuthority,
        payload: &'a RawValue,
    ) -> HostCallbackFuture<'a> {
        Box::pin(async move {
            Ok(Envelope::success(Some(
                serde_json::value::RawValue::from_string(payload.get().to_owned())
                    .map_err(|_| PluginClientError::InvalidRequest)?,
            )))
        })
    }
}

struct HttpAuthority {
    seen: Mutex<Vec<HttpRequest>>,
}

impl HostHttpClient for HttpAuthority {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(request);
            Ok(HttpResponse {
                status_code: 204,
                ..HttpResponse::default()
            })
        })
    }

    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(request);
            let (sender, receiver) = mpsc::channel(2);
            sender
                .send(HttpStreamChunk {
                    payload: b"chunk".to_vec(),
                    error: None,
                })
                .await
                .unwrap();
            drop(sender);
            Ok(HttpStreamResponse {
                status_code: 200,
                headers: Default::default(),
                chunks: receiver,
            })
        })
    }
}

#[derive(Default)]
struct LogAuthority {
    calls: Mutex<Vec<(String, String, String)>>,
}

impl HostLogAuthority for LogAuthority {
    fn log<'a>(
        &'a self,
        caller_plugin_id: &'a str,
        level: &'a str,
        message: &'a str,
        _fields: std::collections::BTreeMap<String, Value>,
    ) -> PluginFuture<'a, ()> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((
                    caller_plugin_id.to_owned(),
                    level.to_owned(),
                    message.to_owned(),
                ));
            Ok(())
        })
    }
}
