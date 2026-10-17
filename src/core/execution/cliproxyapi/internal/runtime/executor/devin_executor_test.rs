// ref: internal/runtime/executor/devin_executor.go:230-387,464-583,990-1123,2465-2482
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::super::helps::devin_wire::{
    wrap_connect_envelope, wrap_connect_envelope_with_flag, CONNECT_FLAG_END_STREAM,
};
use super::*;
use crate::sdk::pluginapi::{HostHttpClient, HttpResponse, HttpStreamResponse};
use crate::sdk::translator::ResponseTransform;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

struct Context {
    calls: AtomicUsize,
}
impl DevinAttemptContextProvider for Context {
    fn for_request(
        &self,
        _: &ExecutorRequest,
    ) -> Result<DevinAttemptContext, PluginExecutionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(DevinAttemptContext {
            canonical_session_id: "owned-session".into(),
            usage: None,
        })
    }
}
struct Transport {
    status: u16,
    headers: Headers,
    receiver: Mutex<Option<mpsc::Receiver<HttpStreamChunk>>>,
    requests: Mutex<Vec<HttpRequest>>,
    unary_calls: AtomicUsize,
}
impl HostHttpClient for Transport {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.unary_calls.fetch_add(1, Ordering::SeqCst);
            self.requests.lock().unwrap().push(request);
            Ok(HttpResponse {
                status_code: self.status,
                headers: self.headers.clone(),
                body: b"native-http-response".to_vec(),
            })
        })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            let chunks = self
                .receiver
                .lock()
                .unwrap()
                .take()
                .expect("one owned attempt");
            Ok(HttpStreamResponse {
                status_code: self.status,
                headers: self.headers.clone(),
                chunks,
            })
        })
    }
}
struct Fixture {
    executor: Arc<DevinExecutor>,
    transport: Arc<Transport>,
    sender: mpsc::Sender<HttpStreamChunk>,
    context: Arc<Context>,
    registry: Arc<Registry>,
}
impl Fixture {
    fn new(status: u16, headers: Headers) -> Self {
        let (sender, receiver) = mpsc::channel(32);
        let registry = Arc::new(Registry::new());
        let context = Arc::new(Context {
            calls: AtomicUsize::new(0),
        });
        let transport = Arc::new(Transport {
            status,
            headers,
            receiver: Mutex::new(Some(receiver)),
            requests: Mutex::new(Vec::new()),
            unary_calls: AtomicUsize::new(0),
        });
        let executor = Arc::new(DevinExecutor::new(
            registry.clone(),
            Arc::new(DevinModelsStore::default()),
            Arc::new(StaticModelsCatalog::default()),
            Arc::new(DevinSessionTurns::default()),
            context.clone(),
        ));
        Self {
            executor,
            transport,
            sender,
            context,
            registry,
        }
    }
    fn request(&self) -> ExecutorRequest {
        ExecutorRequest {
            auth_id: "private-test-only-account".into(),
            model: "swe-2".into(),
            source_format: "interactions".into(),
            format: "interactions".into(),
            auth_attributes: BTreeMap::from([("api_key".into(), "test-only-token".into())]),
            payload: br#"{"input":[{"type":"user_input","content":"test-only prompt"}]}"#.to_vec(),
            http_client: Some(self.transport.clone()),
            ..ExecutorRequest::default()
        }
    }
    async fn send(&self, payload: Vec<u8>) {
        self.sender
            .send(HttpStreamChunk {
                payload,
                error: None,
            })
            .await
            .unwrap();
    }
}
fn text_frame(text: &str) -> Vec<u8> {
    assert!(text.len() < 128);
    let mut proto = vec![26, text.len() as u8];
    proto.extend_from_slice(text.as_bytes());
    wrap_connect_envelope(&proto).unwrap()
}
fn eos() -> Vec<u8> {
    wrap_connect_envelope_with_flag(CONNECT_FLAG_END_STREAM, b"{}").unwrap()
}
fn failed_eos() -> Vec<u8> {
    wrap_connect_envelope_with_flag(
        CONNECT_FLAG_END_STREAM,
        br#"{"error":{"code":"unauthenticated","message":"test-only rejection"}}"#,
    )
    .unwrap()
}
async fn collect(response: &mut ExecutorStreamResponse) -> (Vec<u8>, Vec<PluginExecutionError>) {
    let mut bytes = Vec::new();
    let mut errors = Vec::new();
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(1), response.chunks.recv())
        .await
        .expect("owned stream must terminate")
    {
        bytes.extend_from_slice(&chunk.payload);
        if let Some(error) = chunk.error {
            errors.push(error);
        }
    }
    (bytes, errors)
}
