// ref: internal/pluginhost/scheduler_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: scheduler selection delegates over isolated RPC
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use serde_json::value::to_raw_value;
use tokio::sync::mpsc;

use crate::sdk::pluginabi::{Envelope, METHOD_SCHEDULER_PICK};
use crate::sdk::pluginapi::{Scheduler, SchedulerAuthCandidate, SchedulerPickRequest};

use super::abi::{PluginCall, PluginClient, PluginClientError, PluginFuture, PluginStream};
use super::rpc_client::RpcPluginClient;
use super::scheduler::RpcScheduler;

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
                    "AuthID": "auth-b",
                    "DelegateBuiltin": "",
                    "Handled": true
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
