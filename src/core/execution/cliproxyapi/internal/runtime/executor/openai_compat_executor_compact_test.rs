// ref: internal/runtime/executor/openai_compat_executor_compact_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};
use tokio::sync::mpsc;

use super::openai_compat_executor::{
    prepare_openai_compat_images_payload, OpenAiCompatConfig, OpenAiCompatError,
    OpenAiCompatExecutor, OpenAiCompatPayloadModelRule, OpenAiCompatPayloadRule,
    OpenAiCompatibility,
};
use crate::sdk::pluginapi::{
    ExecutorRequest, HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk,
    HttpStreamResponse, PluginFuture, ProviderExecutor,
};
use crate::sdk::translator::{Format, Registry};

#[derive(Default)]
struct RecordedClient {
    requests: Mutex<Vec<HttpRequest>>,
    response_body: Mutex<Vec<u8>>,
    stream_chunks: Mutex<Vec<Vec<u8>>>,
}

impl RecordedClient {
    fn with_response(body: &[u8]) -> Arc<Self> {
        Arc::new(Self {
            response_body: Mutex::new(body.to_vec()),
            ..Self::default()
        })
    }

    fn with_stream(chunks: &[&[u8]]) -> Arc<Self> {
        Arc::new(Self {
            stream_chunks: Mutex::new(chunks.iter().map(|chunk| chunk.to_vec()).collect()),
            ..Self::default()
        })
    }

    fn request(&self) -> HttpRequest {
        self.requests.lock().unwrap().last().unwrap().clone()
    }
}

impl HostHttpClient for RecordedClient {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        self.requests.lock().unwrap().push(request);
        let body = self.response_body.lock().unwrap().clone();
        Box::pin(async move {
            Ok(HttpResponse {
                status_code: 200,
                body,
                ..HttpResponse::default()
            })
        })
    }

    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        self.requests.lock().unwrap().push(request);
        let chunks = self.stream_chunks.lock().unwrap().clone();
        Box::pin(async move {
            let (sender, receiver) = mpsc::channel(chunks.len().max(1));
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

fn executor(config: OpenAiCompatConfig) -> OpenAiCompatExecutor {
    OpenAiCompatExecutor::new(
        "openai-compatibility",
        Arc::new(config),
        Arc::new(Registry::new()),
    )
}

fn compat_config(support_prompt_cache_key: bool) -> OpenAiCompatConfig {
    OpenAiCompatConfig {
        compatibility: vec![OpenAiCompatibility {
            name: "compat".into(),
            support_prompt_cache_key,
            ..OpenAiCompatibility::default()
        }],
        ..OpenAiCompatConfig::default()
    }
}

fn request(client: Arc<RecordedClient>, payload: &[u8]) -> ExecutorRequest {
    ExecutorRequest {
        auth_provider: "openai-compatibility".into(),
        model: "gpt-5.6".into(),
        format: "openai".into(),
        source_format: "openai".into(),
        payload: payload.to_vec(),
        original_request: payload.to_vec(),
        auth_attributes: BTreeMap::from([
            ("base_url".into(), "https://provider.example/v1".into()),
            ("api_key".into(), "test".into()),
            ("compat_name".into(), "compat".into()),
            ("provider_key".into(), "compat".into()),
        ]),
        http_client: Some(client),
        ..ExecutorRequest::default()
    }
}

fn prompt_key(payload: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(payload)
        .ok()?
        .get("prompt_cache_key")?
        .as_str()
        .map(str::to_owned)
}

async fn collect_stream(
    mut response: crate::sdk::pluginapi::ExecutorStreamResponse,
) -> (Vec<u8>, Option<crate::sdk::pluginapi::PluginExecutionError>) {
    let mut output = Vec::new();
    while let Some(chunk) = response.chunks.recv().await {
        if chunk.error.is_some() {
            return (output, chunk.error);
        }
        output.extend_from_slice(&chunk.payload);
    }
    (output, None)
}

fn multipart_payload() -> (Vec<u8>, &'static str) {
    let boundary = "upstream-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\ncompat-image\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nedit\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"image.webp\"\r\nContent-Type: image/webp\r\n\r\nwebp-data\r\n--{boundary}--\r\n"
    );
    (body.into_bytes(), boundary)
}
