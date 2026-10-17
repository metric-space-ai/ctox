// ref: internal/runtime/executor/aistudio_executor_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc;

use crate::sdk::pluginapi::{
    ExecutorRequest, HostHttpClient, HttpRequest, HttpResponse, HttpStreamResponse, PluginFuture,
    ProviderExecutor,
};
use crate::sdk::translator::Registry;

use super::aistudio_executor::{ensure_colon_spaced_json, AiStudioExecutor};

#[derive(Default)]
struct Relay {
    requests: Mutex<Vec<HttpRequest>>,
    response: Mutex<HttpResponse>,
}
impl Relay {
    fn responding(body: &[u8]) -> Arc<Self> {
        Arc::new(Self {
            response: Mutex::new(HttpResponse {
                status_code: 200,
                body: body.to_vec(),
                ..HttpResponse::default()
            }),
            ..Self::default()
        })
    }
}
impl HostHttpClient for Relay {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            Ok(self.response.lock().unwrap().clone())
        })
    }
    fn execute_stream<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            let (_sender, receiver) = mpsc::channel(1);
            Ok(HttpStreamResponse {
                status_code: 200,
                headers: BTreeMap::new(),
                chunks: receiver,
            })
        })
    }
}

fn request() -> ExecutorRequest {
    ExecutorRequest {
        auth_id: "a1".into(), model: "gemini-2.5-pro".into(), source_format: "gemini".into(),
        payload: br#"{"contents":[],"reasoning":{"summary":"detailed"},"generationConfig":{"maxOutputTokens":100,"responseMimeType":"application/json","responseJsonSchema":{}}}"#.to_vec(),
        auth_attributes: BTreeMap::from([("header:X-Test".into(), "yes".into())]),
        ..ExecutorRequest::default()
    }
}
