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
#[tokio::test]
async fn candidate_meta_transport_thinking_receives_exact_manager_owned_model_capability() {
    let fixture = Fixture::new(200, TERMINAL);
    let model = Arc::new(crate::internal::modelconfig::ModelInfo {
        id: "muse-test".into(),
        context_length: 123_456,
        thinking: Some(crate::internal::modelconfig::ThinkingSupport {
            min: 37,
            max: 99,
            levels: vec!["owned-level".into()],
            ..Default::default()
        }),
        ..Default::default()
    });
    let thinking = Arc::new(ThinkingWithOwnedModel(model.clone(), AtomicUsize::new(0)));
    let owner = Arc::new(MetaRequestOwner {
        processor: Arc::new(Processor),
        thinking: thinking.clone(),
        config: Arc::new(PayloadApplyConfig::default()),
    });
    let executor = MetaExecutor::new(
        fixture.executor.registry.clone(),
        owner,
        fixture.executor.context.clone(),
    );
    let mut request = fixture.request();
    request.resolved_model_info = Some(model);
    executor.execute(request).await.unwrap();
    assert_eq!(thinking.1.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.transport.requests.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn candidate_meta_transport_read_failure_preserves_final_partial_event_and_failure_usage() {
    let fixture = Fixture::new(200, &[]);
    let stream = fixture
        .executor
        .execute_stream(fixture.request())
        .await
        .unwrap();
    fixture
        .sender
        .as_ref()
        .unwrap()
        .send(HttpStreamChunk {
            payload: b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"retained\"}"
                .to_vec(),
            error: Some(meta_error(503, "test-only interrupted body")),
        })
        .await
        .unwrap();
    let (payload, errors) = collect(stream).await;
    assert!(std::str::from_utf8(&payload).unwrap().contains("retained"));
    assert_eq!(errors.len(), 1);
    assert_eq!(meta_plugin_error_status(errors[0].as_ref()), Some(503));
    let records = fixture.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].fail.status_code, 503);
}

#[tokio::test]
async fn candidate_meta_transport_bootstrap_error_keeps_real_body_scope_and_releases_receiver() {
    let mut fixture = Fixture::new(429, &[]);
    fixture
        .send(br#"{"error":{"message":"subscription quota exhausted","resets_at":160}}"#)
        .await;
    let sender = fixture.sender.take().unwrap();
    drop(sender);
    let error = fixture
        .executor
        .execute_stream(fixture.request())
        .await
        .err()
        .unwrap();
    let status = error.downcast_ref::<MetaHttpStatusError>().unwrap();
    assert_eq!(status.retry_after, Some(Duration::from_secs(60)));
    assert!(status.credential_scoped);
    assert_eq!(fixture.transport.stream_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.records()[0].fail.status_code, 429);
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

#[tokio::test]
async fn candidate_meta_factory_native_owner_shares_preparation_and_refresh_and_honors_stop() {
    use crate::internal::auth::meta::MetaAuth;
    use crate::internal::runtime::executor::meta_executor_auth::MetaRequestAuthPreparer;
    use crate::sdk::cliproxy::auth::{RefreshCancellation, RefreshExecutorError};
    let fixture = Fixture::new(200, &[]);
    let native = Arc::new(MetaRequestAuthPreparer::new(Arc::new(MetaAuth::new(
        fixture.transport.clone(),
    ))));
    let execution = Arc::new(MetaExecutor::new(
        fixture.executor.registry.clone(),
        fixture.executor.request_owner.clone(),
        fixture.executor.context.clone(),
    ));
    let factory = MetaExecutorFactory::with_native_auth(
        Arc::new(Fallback::default()),
        execution,
        native.clone(),
        tokio::runtime::Handle::current(),
    );
    let mut account = Auth::default();
    account.id = "test-only-meta-native".into();
    account.provider = "meta".into();
    let registration = factory.registration_for("meta", &account).unwrap();
    let expected_async: Arc<dyn AsyncAuthRefresher> = native.clone();
    let expected_prepare: Arc<dyn AuthPreparer> = native;
    assert!(Arc::ptr_eq(
        &registration.async_auth_refresher().unwrap(),
        &expected_async
    ));
    assert!(Arc::ptr_eq(
        &registration.auth_preparer().unwrap(),
        &expected_prepare
    ));
    let scheduled = registration.refresher();
    let cancellation = RefreshCancellation::default();
    cancellation.cancel();
    let result = tokio::task::spawn_blocking(move || {
        scheduled.refresh_with_cancellation(&mut account, &cancellation)
    })
    .await
    .unwrap();
    assert!(matches!(result, Err(RefreshExecutorError::Cancelled)));
    assert!(fixture.transport.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn candidate_meta_factory_keeps_all_owned_capabilities_and_dispatches_real_selected_http() {
    let fixture = Fixture::new(403, b"owned reply");
    let fallback = Arc::new(Fallback::default());
    let scheduled: Arc<dyn AuthRefresher> = Arc::new(Refresh);
    let asynchronous: Arc<dyn AsyncAuthRefresher> = Arc::new(Refresh);
    let preparer: Arc<dyn AuthPreparer> = Arc::new(Preparer);
    let factory = factory(
        &fixture,
        fallback.clone(),
        scheduled.clone(),
        asynchronous.clone(),
        preparer.clone(),
    );
    let mut account = Auth::default();
    account.id = "test-only-meta-account".into();
    account.provider = "meta".into();
    let registration = factory.registration_for(" META ", &account).unwrap();
    assert!(Arc::ptr_eq(&registration.refresher(), &scheduled));
    assert!(Arc::ptr_eq(
        &registration.async_auth_refresher().unwrap(),
        &asynchronous
    ));
    assert!(Arc::ptr_eq(
        &registration.auth_preparer().unwrap(),
        &preparer
    ));
    registration
        .auth_preparer()
        .unwrap()
        .prepare(&mut account)
        .await
        .unwrap();
    assert_eq!(account.attributes["test-only-prepared"], "true");
    let registry = ProviderExecutorRegistry::default();
    registry.register(registration);
    registry.register(factory.registration_for("meta", &account).unwrap());
    let response = registry
        .http_request(
            "meta",
            ExecutorHttpRequest {
                auth_provider: "meta".into(),
                method: "GET".into(),
                url: "https://meta.test.invalid/resource".into(),
                attributes: BTreeMap::from([("api_key".into(), "test-only-key".into())]),
                http_client: Some(fixture.transport.clone()),
                ..ExecutorHttpRequest::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(response.status_code, 403);
    assert_eq!(response.body, b"owned reply");
    assert_eq!(fixture.transport.requests.lock().unwrap().len(), 1);
    assert_eq!(fallback.0.load(Ordering::SeqCst), 0);
}

#[test]
fn candidate_meta_factory_rejects_disabled_mismatched_accounts_and_delegates_other_providers() {
    let fixture = Fixture::new(200, &[]);
    let fallback = Arc::new(Fallback::default());
    let factory = factory(
        &fixture,
        fallback.clone(),
        Arc::new(Refresh),
        Arc::new(Refresh),
        Arc::new(Preparer),
    );
    let mut other = Auth::default();
    other.provider = "codex".into();
    assert!(factory.registration_for("meta", &other).is_err());
    assert_eq!(fallback.0.load(Ordering::SeqCst), 0);
    let mut disabled = Auth::default();
    disabled.provider = "meta".into();
    disabled.disabled = true;
    assert!(factory.registration_for("meta", &disabled).is_err());
    assert!(factory.registration_for("codex", &other).is_err());
    assert_eq!(fallback.0.load(Ordering::SeqCst), 1);
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

#[tokio::test]
async fn candidate_meta_transport_unary_uses_selected_account_and_preserves_raw_terminal_items() {
    let body = b"data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"id\":\"item-1\",\"number\":900719925474099312345}}\n\ndata: {\"type\":\"response.incomplete\",\"response\":{\"output\":[],\"usage\":{\"input_tokens\":11,\"output_tokens\":3,\"total_tokens\":14}}}\n\n";
    let fixture = Fixture::new(200, body);
    let result = fixture.executor.execute(fixture.request()).await.unwrap();
    let document = std::str::from_utf8(&result.payload).unwrap();
    assert_eq!(gjson::get(document, "type").str(), "response.incomplete");
    assert_eq!(gjson::get(document, "response.output.0.id").str(), "item-1");
    assert!(document.contains("900719925474099312345"));
    assert_eq!(result.headers["X-Test-Reply"], vec!["retained"]);
    let requests = fixture.transport.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url, "https://meta.test.invalid/v1/responses");
    assert_eq!(
        requests[0].headers["Authorization"],
        vec!["Bearer test-only-key"]
    );
    let request = std::str::from_utf8(&requests[0].body).unwrap();
    assert!(gjson::get(request, "stream").bool());
    assert!(!gjson::get(request, "generate").exists());
    assert!(!gjson::get(request, "client_metadata").exists());
    assert!(!gjson::get(request, "tools.0.search_content_types").exists());
    assert_eq!(gjson::get(request, "instructions").str(), "");
    drop(requests);
    let records = fixture.records();
    assert_eq!(records.len(), 1);
    assert!(!records[0].failed);
    assert_eq!(records[0].detail.total_tokens, 14);
}

#[tokio::test]
async fn candidate_meta_transport_unary_plain_response_and_usage_details() {
    let fixture = Fixture::new(200, br#"{"object":"response","output":[],"usage":{"input_tokens":2,"output_tokens":1,"total_tokens":3}}"#);
    let mut request = fixture.request();
    request.format = "openai-response".into();
    let result = fixture.executor.execute(request).await.unwrap();
    let document = std::str::from_utf8(&result.payload).unwrap();
    assert_eq!(
        gjson::get(
            document,
            "response.usage.input_tokens_details.cached_tokens"
        )
        .json(),
        "0"
    );
    assert_eq!(
        gjson::get(
            document,
            "response.usage.output_tokens_details.reasoning_tokens"
        )
        .json(),
        "0"
    );
    assert_eq!(fixture.records()[0].detail.total_tokens, 3);
}

#[tokio::test]
async fn candidate_meta_transport_unary_never_turns_arbitrary_json_into_success() {
    for body in [
        b"{}".as_slice(),
        br#"{"error":{"message":"provider failure"}}"#,
    ] {
        let fixture = Fixture::new(200, body);
        let error = fixture
            .executor
            .execute(fixture.request())
            .await
            .err()
            .unwrap();
        assert_eq!(meta_plugin_error_status(error.as_ref()), Some(408));
        let records = fixture.records();
        assert_eq!(records.len(), 1);
        assert!(records[0].failed);
        assert_eq!(records[0].fail.status_code, 408);
    }
}

#[tokio::test]
async fn candidate_meta_transport_http_status_retains_reset_and_account_scope() {
    let fixture = Fixture::new(429, br#"{"error":{"code":"rate_limit_exceeded","message":"subscription quota exhausted","resets_at":160}}"#);
    let error = fixture
        .executor
        .execute(fixture.request())
        .await
        .err()
        .unwrap();
    let status = error.downcast_ref::<MetaHttpStatusError>().unwrap();
    assert_eq!(status.status_code(), 429);
    assert_eq!(status.retry_after, Some(Duration::from_secs(60)));
    assert!(status.credential_scoped);
    let records = fixture.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].fail.status_code, 429);
}

#[tokio::test]
async fn candidate_meta_transport_stream_fragmentation_framing_usage_and_final_tail() {
    let mut fixture = Fixture::new(200, &[]);
    let stream = fixture
        .executor
        .execute_stream(fixture.request())
        .await
        .unwrap();
    let mut body = b"event: response.output_item.done\r\ndata: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"id\":\"stream-item\"}}\r\n\r\ndata: ".to_vec();
    body.extend_from_slice(TERMINAL); // intentionally no final newline
    for fragment in body.chunks(7) {
        fixture.send(fragment).await;
    }
    fixture.sender.take();
    let (payload, errors) = collect(stream).await;
    assert!(errors.is_empty());
    let text = std::str::from_utf8(&payload).unwrap();
    assert!(text.starts_with("event: response.output_item.done\ndata: "));
    assert!(text.contains("\n\ndata: "));
    let event = text
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .last()
        .unwrap();
    assert_eq!(
        gjson::get(event, "response.output.0.id").str(),
        "stream-item"
    );
    let records = fixture.records();
    assert_eq!(records.len(), 1);
    assert!(!records[0].failed);
    assert_eq!(records[0].detail.input_tokens, 11);
    assert_eq!(records[0].detail.cached_tokens, 7);
    assert_eq!(fixture.transport.stream_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn candidate_meta_transport_stream_provider_error_is_typed_and_never_replayed() {
    let fixture = Fixture::new(200, &[]);
    let stream = fixture
        .executor
        .execute_stream(fixture.request())
        .await
        .unwrap();
    fixture.send(b"data: {\"type\":\"error\",\"error\":{\"code\":429,\"message\":\"subscription quota exhausted\",\"resets_at\":160}}\n\n").await;
    let (payload, errors) = collect(stream).await;
    assert!(payload.is_empty());
    assert_eq!(errors.len(), 1);
    let status = errors[0].downcast_ref::<MetaHttpStatusError>().unwrap();
    assert!(status.credential_scoped);
    assert_eq!(status.retry_after, Some(Duration::from_secs(60)));
    tokio::time::timeout(
        Duration::from_secs(1),
        fixture.sender.as_ref().unwrap().closed(),
    )
    .await
    .unwrap();
    assert_eq!(fixture.transport.stream_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.records()[0].fail.status_code, 429);
}

#[tokio::test]
async fn candidate_meta_transport_downstream_close_releases_upstream_and_records_cancellation() {
    let fixture = Fixture::new(200, &[]);
    let stream = fixture
        .executor
        .execute_stream(fixture.request())
        .await
        .unwrap();
    drop(stream);
    tokio::time::timeout(
        Duration::from_secs(1),
        fixture.sender.as_ref().unwrap().closed(),
    )
    .await
    .unwrap();
    let records = fixture.records();
    assert_eq!(records.len(), 1);
    assert!(records[0].failed);
    assert_eq!(records[0].fail.status_code, 499);
}

#[tokio::test]
async fn candidate_meta_transport_rejects_compact_wrong_provider_and_unprepared_dca_without_http() {
    for case in 0..4 {
        let fixture = Fixture::new(200, TERMINAL);
        let mut request = fixture.request();
        match case {
            0 => request.alt = "responses/compact".into(),
            1 => request.auth_provider = "codex".into(),
            2 => {
                request.auth_attributes.insert(
                    "api_key".into(),
                    "dca:test-only-not-an-inference-key".into(),
                );
            }
            _ => request.http_client = None,
        }
        let error = fixture.executor.execute(request).await.err().unwrap();
        assert_eq!(
            meta_plugin_error_status(error.as_ref()),
            Some([501, 400, 401, 500][case])
        );
        assert!(fixture.transport.requests.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn candidate_meta_transport_token_count_is_local_and_fixed_o200k() {
    let fixture = Fixture::new(200, &[]);
    let mut request = fixture.request();
    request.alt = "responses/compact".into();
    let prepared = fixture
        .executor
        .request_owner
        .prepare(&request, false)
        .unwrap();
    let expected = count_meta_input_tokens(&prepared.body).unwrap();
    let result = fixture.executor.count_tokens(request).await.unwrap();
    assert_eq!(
        gjson::get(
            std::str::from_utf8(&result.payload).unwrap(),
            "response.usage.input_tokens"
        )
        .i64(),
        expected
    );
    assert!(fixture.transport.requests.lock().unwrap().is_empty());
    assert!(fixture.records().is_empty());
}

#[tokio::test]
async fn candidate_meta_transport_passthrough_uses_selected_key_and_preserves_method_url_reply() {
    let fixture = Fixture::new(403, b"preserved reply");
    let request = ExecutorHttpRequest {
        auth_provider: "meta".into(),
        method: "GET".into(),
        url: "https://meta.test.invalid/owned-resource".into(),
        attributes: BTreeMap::from([("api_key".into(), "test-only-key".into())]),
        headers: BTreeMap::from([
            ("authorization".into(), vec!["must-be-replaced".into()]),
            ("X-Test-Input".into(), vec!["retained".into()]),
            ("Accept".into(), vec!["application/octet-stream".into()]),
            ("Content-Type".into(), vec!["text/plain".into()]),
            ("Cache-Control".into(), vec!["private".into()]),
        ]),
        http_client: Some(fixture.transport.clone()),
        ..ExecutorHttpRequest::default()
    };
    let result = fixture.executor.http_request(request).await.unwrap();
    assert_eq!(result.status_code, 403);
    assert_eq!(result.body, b"preserved reply");
    let requests = fixture.transport.requests.lock().unwrap();
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].url, "https://meta.test.invalid/owned-resource");
    assert_eq!(
        requests[0].headers["Authorization"],
        vec!["Bearer test-only-key"]
    );
    assert!(!requests[0].headers.contains_key("authorization"));
    assert_eq!(
        requests[0].headers["Accept"],
        vec!["application/octet-stream"]
    );
    assert_eq!(requests[0].headers["Content-Type"], vec!["text/plain"]);
    assert_eq!(requests[0].headers["Cache-Control"], vec!["private"]);
    assert_eq!(requests[0].headers["X-Test-Input"], vec!["retained"]);
}

#[tokio::test]
async fn candidate_meta_transport_custom_patch_failure_cannot_publish_success() {
    let fixture = Fixture::new(200, br#"{"object":"response","output":[{"type":"function_call","name":"apply_patch","call_id":"patch-1","arguments":"not-json"}]}"#);
    let mut request = fixture.request();
    request.payload = br#"{"input":[],"tools":[{"type":"custom","name":"apply_patch"}]}"#.to_vec();
    let error = fixture.executor.execute(request).await.err().unwrap();
    assert_eq!(meta_plugin_error_status(error.as_ref()), Some(502));
    assert!(fixture.records()[0].failed);
}
