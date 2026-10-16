// ref: internal/runtime/executor/gemini_executor_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};
use tokio::sync::mpsc;

use crate::internal::translator::register_all;
use crate::sdk::pluginapi::{
    ExecutorRequest, HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk,
    HttpStreamResponse, PluginFuture, ProviderExecutor,
};
use crate::sdk::translator::{Format, Registry, TranslationContext};

use super::gemini_executor::{
    apply_interactions_request_headers, apply_interactions_revision_header,
    cap_gemini_max_output_tokens, fix_gemini_image_aspect_ratio, gemini_interactions_sse_done,
    gemini_interactions_sse_payload, native_interactions_source_format, GeminiExecutor,
    GeminiExecutorConfig, GeminiPayloadRule, GEMINI_INTERACTIONS_API_REVISION,
};

#[derive(Default)]
struct MockClient {
    requests: Mutex<Vec<HttpRequest>>,
    response: Mutex<HttpResponse>,
    stream_chunks: Mutex<Vec<Vec<u8>>>,
    entered_at: Mutex<Option<std::time::SystemTime>>,
}

impl MockClient {
    fn with_body(body: &[u8]) -> Arc<Self> {
        Arc::new(Self {
            response: Mutex::new(HttpResponse {
                status_code: 200,
                body: body.to_vec(),
                ..HttpResponse::default()
            }),
            ..Self::default()
        })
    }
    fn request(&self) -> HttpRequest {
        self.requests.lock().unwrap().last().unwrap().clone()
    }
}

impl HostHttpClient for MockClient {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            *self.entered_at.lock().unwrap() = Some(std::time::SystemTime::now());
            self.requests.lock().unwrap().push(request);
            Ok(self.response.lock().unwrap().clone())
        })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            let (sender, receiver) = mpsc::channel(16);
            let chunks = std::mem::take(&mut *self.stream_chunks.lock().unwrap());
            for payload in chunks {
                sender
                    .send(HttpStreamChunk {
                        payload,
                        error: None,
                    })
                    .await
                    .unwrap();
            }
            drop(sender);
            Ok(HttpStreamResponse {
                status_code: 200,
                headers: BTreeMap::new(),
                chunks: receiver,
            })
        })
    }
}

fn request(client: Arc<MockClient>) -> ExecutorRequest {
    ExecutorRequest {
        model: "gemini-3.1-pro-preview".into(),
        source_format: "gemini".into(),
        payload:
            br#"{"contents":[],"generationConfig":{"maxOutputTokens":500000,"temperature":0.2}}"#
                .to_vec(),
        auth_attributes: BTreeMap::from([
            ("api_key".into(), "test-key".into()),
            ("base_url".into(), "http://upstream".into()),
        ]),
        http_client: Some(client),
        ..ExecutorRequest::default()
    }
}

fn header<'a>(headers: &'a BTreeMap<String, Vec<String>>, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(String::as_str)
}

#[derive(Default)]
struct GeminiUsageCollector(
    Mutex<Vec<crate::sdk::cliproxy::usage::Record>>,
    Mutex<Vec<crate::sdk::cliproxy::usage::UsageContext>>,
);

impl crate::sdk::cliproxy::usage::Plugin for GeminiUsageCollector {
    fn handle_usage(
        &self,
        context: &crate::sdk::cliproxy::usage::UsageContext,
        record: &crate::sdk::cliproxy::usage::Record,
    ) {
        self.1.lock().unwrap().push(context.clone());
        self.0.lock().unwrap().push(record.clone());
    }
}

fn usage_executor() -> (
    GeminiExecutor,
    Arc<crate::sdk::cliproxy::usage::Manager>,
    Arc<GeminiUsageCollector>,
) {
    let manager = Arc::new(crate::sdk::cliproxy::usage::Manager::new(8));
    let collector = Arc::new(GeminiUsageCollector::default());
    manager.register(collector.clone());
    let executor = GeminiExecutor::new(
        Arc::new(GeminiExecutorConfig::default()),
        Arc::new(Registry::new()),
    )
    .with_usage_manager(manager.clone());
    (executor, manager, collector)
}

struct PendingGeminiUsageClient(tokio::sync::Notify);

impl HostHttpClient for PendingGeminiUsageClient {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.0.notify_one();
            std::future::pending().await
        })
    }

    fn execute_stream<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(std::future::pending())
    }
}
