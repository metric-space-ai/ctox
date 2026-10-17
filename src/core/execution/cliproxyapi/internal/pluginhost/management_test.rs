// ref: internal/pluginhost/management_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: typed management route registration and dispatch without an HTTP data bridge
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};

use crate::sdk::pluginapi::{
    ManagementApi, ManagementHandler, ManagementRegistrationRequest,
    ManagementRegistrationResponse, ManagementRequest, ManagementResponse, ManagementRoute,
    Metadata, PluginFuture, ResourceRoute,
};

use super::management::{ManagementDispatchError, ManagementPluginRecord, ManagementRegistry};

struct Handler {
    response: Vec<u8>,
    seen: Mutex<Vec<ManagementRequest>>,
}

impl ManagementHandler for Handler {
    fn handle_management<'a>(
        &'a self,
        request: ManagementRequest,
    ) -> PluginFuture<'a, ManagementResponse> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(request);
            Ok(ManagementResponse {
                status_code: 200,
                body: self.response.clone(),
                ..ManagementResponse::default()
            })
        })
    }
}

struct Api {
    route: String,
    handler: Arc<Handler>,
}

impl ManagementApi for Api {
    fn register_management<'a>(
        &'a self,
        _request: ManagementRegistrationRequest,
    ) -> PluginFuture<'a, ManagementRegistrationResponse> {
        Box::pin(async move {
            Ok(ManagementRegistrationResponse {
                routes: vec![ManagementRoute {
                    method: "GET".to_owned(),
                    path: self.route.clone(),
                    menu: String::new(),
                    description: String::new(),
                    handler: self.handler.clone(),
                }],
                resources: vec![ResourceRoute {
                    path: format!("{}/asset", self.route),
                    menu: String::new(),
                    description: String::new(),
                    handler: self.handler.clone(),
                }],
            })
        })
    }
}

fn record(id: &str, priority: i32, api: Arc<Api>) -> ManagementPluginRecord {
    ManagementPluginRecord {
        plugin_id: id.to_owned(),
        priority,
        metadata: Metadata {
            name: id.to_owned(),
            ..Metadata::default()
        },
        api,
    }
}
