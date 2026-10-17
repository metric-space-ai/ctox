// ref: internal/pluginhost/auth_callbacks_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: auth callbacks preserve identity and delegate all persistence
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use serde_json::{json, value::to_raw_value, Value};

use crate::sdk::pluginabi::{METHOD_HOST_AUTH_GET, METHOD_HOST_AUTH_LIST, METHOD_HOST_AUTH_SAVE};
use crate::sdk::pluginapi::{
    HostAuthFileEntry, HostAuthGetResponse, HostAuthGetRuntimeResponse, HostAuthSaveResponse,
    PluginFuture,
};

use super::auth_callbacks::{install_host_auth_callbacks, HostAuthAuthority, HostAuthListResponse};
use super::callback_contexts::{CallbackAuthority, CallbackContextRegistry};
use super::host_callbacks::{HostCallbackRouteError, HostCallbackRouter};

#[derive(Default)]
struct Authority {
    calls: Mutex<Vec<(String, String)>>,
}

impl HostAuthAuthority for Authority {
    fn list<'a>(&'a self, caller: &'a str) -> PluginFuture<'a, Vec<HostAuthFileEntry>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((caller.to_owned(), "list".to_owned()));
            Ok(vec![HostAuthFileEntry {
                auth_index: "idx-1".to_owned(),
                name: "claude.json".to_owned(),
                provider: "claude".to_owned(),
                ..HostAuthFileEntry::default()
            }])
        })
    }

    fn get<'a>(
        &'a self,
        caller: &'a str,
        auth_index: &'a str,
    ) -> PluginFuture<'a, HostAuthGetResponse> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((caller.to_owned(), auth_index.to_owned()));
            Ok(HostAuthGetResponse {
                auth_index: auth_index.to_owned(),
                name: "claude.json".to_owned(),
                json: json!({"type": "claude"}),
                ..HostAuthGetResponse::default()
            })
        })
    }

    fn get_runtime<'a>(
        &'a self,
        _caller: &'a str,
        _auth_index: &'a str,
    ) -> PluginFuture<'a, HostAuthGetRuntimeResponse> {
        Box::pin(async { Ok(HostAuthGetRuntimeResponse::default()) })
    }

    fn save<'a>(
        &'a self,
        caller: &'a str,
        name: &'a str,
        _json: Value,
    ) -> PluginFuture<'a, HostAuthSaveResponse> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((caller.to_owned(), name.to_owned()));
            Ok(HostAuthSaveResponse {
                name: name.to_owned(),
                path: format!("/authority/{name}"),
            })
        })
    }
}

fn configured() -> (
    HostCallbackRouter,
    super::callback_contexts::CallbackContextLease,
    Arc<Authority>,
) {
    let contexts = CallbackContextRegistry::new();
    let lease = contexts.open(CallbackAuthority::new("plugin-a", None));
    let mut router = HostCallbackRouter::new(contexts);
    let authority = Arc::new(Authority::default());
    install_host_auth_callbacks(&mut router, authority.clone()).unwrap();
    (router, lease, authority)
}
