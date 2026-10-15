// Canonical host construction must exercise the real preparation/HTTP path;
// passthrough test engines cannot stand in for production capability rules.
struct ThinkingWithOwnedModel(Arc<crate::internal::modelconfig::ModelInfo>, AtomicUsize);
impl RequestThinkingEngine for ThinkingWithOwnedModel {
    fn apply_request_thinking(
        &self,
        input: RequestThinkingInput<'_>,
    ) -> Result<Vec<u8>, ThinkingError> {
        let selected = input
            .resolved_config_model_info
            .expect("selected capability must reach the canonical engine");
        assert!(std::ptr::eq(selected, self.0.as_ref()));
        assert!(
            input.resolved_model_info.is_none(),
            "configured capabilities cannot be replaced by a static registry guess"
        );
        assert_eq!(
            selected.thinking.as_ref().unwrap().levels,
            vec!["owned-level"]
        );
        assert_eq!(selected.context_length, 123_456);
        self.1.fetch_add(1, Ordering::SeqCst);
        Ok(input.body.to_vec())
    }
}
use crate::sdk::cliproxy::auth::{
    AsyncAuthRefresher, AuthPreparationError, AuthPreparer, AuthRefresher,
    ProviderExecutorRegistration, ProviderExecutorRegistry, RefreshExecutorError,
};
use crate::sdk::cliproxy::service_executors::{ExecutorFactoryError, ServiceExecutorFactory};
use crate::sdk::cliproxy::service_meta::MetaExecutorFactory;
use std::{future::Future, pin::Pin};
#[derive(Default)]
struct Fallback(AtomicUsize);
impl ServiceExecutorFactory for Fallback {
    fn registration_for(
        &self,
        _: &str,
        _: &Auth,
    ) -> Result<Arc<ProviderExecutorRegistration>, ExecutorFactoryError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(ExecutorFactoryError::Unsupported)
    }
}
struct Refresh;
impl AuthRefresher for Refresh {
    fn refresh(&self, _: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        Ok(None)
    }
}
impl AsyncAuthRefresher for Refresh {
    fn refresh<'a>(
        &'a self,
        auth: &'a Auth,
    ) -> Pin<Box<dyn Future<Output = Result<Auth, AuthPreparationError>> + Send + 'a>> {
        Box::pin(async move { Ok(auth.clone()) })
    }
}
struct Preparer;
impl AuthPreparer for Preparer {
    fn prepare<'a>(
        &'a self,
        auth: &'a mut Auth,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuthPreparationError>> + Send + 'a>> {
        Box::pin(async move {
            auth.attributes
                .insert("test-only-prepared".into(), "true".into());
            Ok(())
        })
    }
}
fn factory(
    fixture: &Fixture,
    fallback: Arc<Fallback>,
    sync: Arc<dyn AuthRefresher>,
    asynchronous: Arc<dyn AsyncAuthRefresher>,
    preparer: Arc<dyn AuthPreparer>,
) -> MetaExecutorFactory {
    let execution = Arc::new(MetaExecutor::new(
        fixture.executor.registry.clone(),
        fixture.executor.request_owner.clone(),
        fixture.executor.context.clone(),
    ));
    MetaExecutorFactory::new(fallback, execution, sync, asynchronous, preparer)
}

// ref: internal/runtime/executor/meta_executor_test.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Additional native SDK transport, ownership and usage guards; no external provider calls.
use super::super::helps::{
    CodexMultiAgentV2Processor, PayloadApplyConfig, RequestThinkingEngine, RequestThinkingInput,
};
use super::*;
use crate::internal::thinking::ThinkingError;
use crate::sdk::cliproxy::usage::{Manager, Plugin, Record, UsageContext};
use crate::sdk::pluginapi::{Headers, HostHttpClient, HttpResponse, HttpStreamResponse};
use chrono::{DateTime, TimeZone, Utc};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::Duration,
};

struct Processor;
impl CodexMultiAgentV2Processor for Processor {
    fn rewrite_spawn_agent_description(&self, _: &Headers, body: &[u8]) -> Vec<u8> {
        body.to_vec()
    }
    fn rewrite_input(&self, _: &Headers, body: &[u8]) -> Vec<u8> {
        body.to_vec()
    }
    fn translate_request(
        &self,
        _: &Headers,
        _: &Format,
        _: &Format,
        _: &str,
        body: &[u8],
        _: bool,
    ) -> Vec<u8> {
        body.to_vec()
    }
    fn optimize_request(&self, _: &Headers, body: &[u8]) -> (Vec<u8>, bool) {
        (body.to_vec(), false)
    }
    fn restore_response(&self, body: &[u8], _: bool) -> Vec<u8> {
        body.to_vec()
    }
}
struct Thinking;
impl RequestThinkingEngine for Thinking {
    fn apply_request_thinking(
        &self,
        input: RequestThinkingInput<'_>,
    ) -> Result<Vec<u8>, ThinkingError> {
        assert_eq!(input.provider, "meta");
        Ok(input.body.to_vec())
    }
}
struct Clock;
impl MetaClock for Clock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_opt(100, 0).unwrap()
    }
}
#[derive(Default)]
struct Sink(Mutex<Vec<Record>>);
impl Plugin for Sink {
    fn handle_usage(&self, _: &UsageContext, record: &Record) {
        self.0.lock().unwrap().push(record.clone());
    }
}
struct Context {
    reporter: Arc<UsageReporter>,
}
impl MetaAttemptContextProvider for Context {
    fn for_request(
        &self,
        _: &ExecutorRequest,
        base_model: &str,
    ) -> Result<MetaAttemptContext, PluginExecutionError> {
        assert_eq!(base_model, "muse-test");
        Ok(MetaAttemptContext {
            usage: Some(self.reporter.clone()),
        })
    }
}
struct Transport {
    status: u16,
    body: Vec<u8>,
    requests: Mutex<Vec<HttpRequest>>,
    source: Mutex<Option<mpsc::Receiver<HttpStreamChunk>>>,
    stream_calls: AtomicUsize,
}
impl HostHttpClient for Transport {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            Ok(HttpResponse {
                status_code: self.status,
                headers: BTreeMap::from([("X-Test-Reply".into(), vec!["retained".into()])]),
                body: self.body.clone(),
            })
        })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            self.stream_calls.fetch_add(1, Ordering::SeqCst);
            Ok(HttpStreamResponse {
                status_code: self.status,
                headers: Headers::new(),
                chunks: self.source.lock().unwrap().take().unwrap(),
            })
        })
    }
}
struct Fixture {
    executor: MetaExecutor,
    transport: Arc<Transport>,
    sender: Option<mpsc::Sender<HttpStreamChunk>>,
    manager: Arc<Manager>,
    sink: Arc<Sink>,
}
impl Fixture {
    fn new(status: u16, body: &[u8]) -> Self {
        let (sender, receiver) = mpsc::channel(16);
        let transport = Arc::new(Transport {
            status,
            body: body.to_vec(),
            requests: Mutex::new(Vec::new()),
            source: Mutex::new(Some(receiver)),
            stream_calls: AtomicUsize::new(0),
        });
        let manager = Arc::new(Manager::new(16));
        let sink = Arc::new(Sink::default());
        manager.register(sink.clone());
        let reporter = Arc::new(UsageReporter::new(
            manager.clone(),
            UsageContext::default(),
            "meta",
            "MetaExecutor",
            "muse-test",
            None,
            "test-only-key",
        ));
        let owner = Arc::new(MetaRequestOwner {
            processor: Arc::new(Processor),
            thinking: Arc::new(Thinking),
            config: Arc::new(PayloadApplyConfig::default()),
        });
        let executor = MetaExecutor::new(
            Arc::new(Registry::new()),
            owner,
            Arc::new(Context { reporter }),
        )
        .with_clock(Arc::new(Clock));
        Self {
            executor,
            transport,
            sender: Some(sender),
            manager,
            sink,
        }
    }
    fn request(&self) -> ExecutorRequest {
        ExecutorRequest { auth_id: "selected-test-only-account".into(), auth_provider: "meta".into(), model: "muse-test".into(),
            source_format: "codex".into(), format: "codex".into(),
            auth_attributes: BTreeMap::from([("api_key".into(), "test-only-key".into()), ("base_url".into(), "https://meta.test.invalid/v1".into())]),
            payload: br#"{"input":[{"type":"message","content":[{"type":"input_text","text":"hello"}]}],"generate":true,"client_metadata":{"x":1},"tools":[{"type":"web_search","search_content_types":["web"]}]}"#.to_vec(),
            http_client: Some(self.transport.clone()), ..ExecutorRequest::default() }
    }
    async fn send(&self, bytes: &[u8]) {
        self.sender
            .as_ref()
            .unwrap()
            .send(HttpStreamChunk {
                payload: bytes.to_vec(),
                error: None,
            })
            .await
            .unwrap();
    }
    fn records(&self) -> Vec<Record> {
        self.manager.stop();
        self.sink.0.lock().unwrap().clone()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.manager.stop();
    }
}
async fn collect(mut stream: ExecutorStreamResponse) -> (Vec<u8>, Vec<PluginExecutionError>) {
    let mut payload = Vec::new();
    let mut errors = Vec::new();
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(2), stream.chunks.recv())
        .await
        .expect("owned stream must close")
    {
        payload.extend(chunk.payload);
        if let Some(error) = chunk.error {
            errors.push(error);
        }
    }
    (payload, errors)
}
const TERMINAL: &[u8] = br#"{"type":"response.completed","response":{"output":[],"usage":{"input_tokens":11,"output_tokens":3,"total_tokens":14,"input_tokens_details":{"cached_tokens":7}}}}"#;
