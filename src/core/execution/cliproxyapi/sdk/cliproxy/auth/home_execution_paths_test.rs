// ref: sdk/cliproxy/auth/home_execution_paths_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: shared injected Home/runtime fixtures plus success and count execution paths
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::TimeZone;

use crate::internal::home::{
    Client, DispatchFailureStage, HomeConfig, HomeError, HomeTransport, KvSetOptions,
    TransportFailure,
};
use crate::sdk::cliproxy::executionregistry::Registry;
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamChunk, ExecutorStreamResponse, PluginExecutionError, PluginFuture,
    ProviderExecutor,
};

use super::{
    Auth, AuthPreparationError, AuthPreparer, AuthRefresher, HomeAuthRuntime, HomeExecutionError,
    ProviderExecutorRegistration, RefreshExecutorError,
};

#[derive(Default)]
pub(super) struct TestHomeTransport {
    responses: Mutex<VecDeque<Result<Vec<u8>, TransportFailure>>>,
    requests: Mutex<Vec<Vec<u8>>>,
    pushes: Mutex<Vec<(String, Vec<u8>)>>,
}

impl TestHomeTransport {
    pub(super) fn with_auth_ids(ids: &[&str]) -> Arc<Self> {
        let transport = Arc::new(Self::default());
        for id in ids {
            transport.push_auth(id);
        }
        transport
    }

    pub(super) fn push_auth(&self, id: &str) {
        self.push_auth_with_metadata(id, serde_json::Map::new());
    }

    pub(super) fn push_auth_with_metadata(
        &self,
        id: &str,
        metadata: serde_json::Map<String, serde_json::Value>,
    ) {
        self.responses
            .lock()
            .unwrap()
            .push_back(Ok(serde_json::to_vec(&serde_json::json!({
                "provider": "codex",
                "auth_index": id,
                "auth": {
                    "id": id,
                    "index": id,
                    "provider": "codex",
                    "metadata": metadata
                }
            }))
            .unwrap()));
    }

    pub(super) fn push_dispatch(&self, response: serde_json::Value) {
        self.responses
            .lock()
            .unwrap()
            .push_back(Ok(serde_json::to_vec(&response).unwrap()));
    }

    pub(super) fn requests(&self) -> Vec<Vec<u8>> {
        self.requests.lock().unwrap().clone()
    }
}

impl HomeTransport for TestHomeTransport {
    fn ping(&self) -> Result<(), HomeError> {
        Ok(())
    }
    fn get(&self, _: &str) -> Result<Option<Vec<u8>>, HomeError> {
        Ok(None)
    }
    fn set(&self, _: &str, _: &[u8], _: KvSetOptions) -> Result<bool, HomeError> {
        Ok(true)
    }
    fn compare_and_swap(
        &self,
        _: &str,
        _: Option<&[u8]>,
        _: &[u8],
        _: Duration,
    ) -> Result<bool, HomeError> {
        Ok(true)
    }
    fn delete(&self, _: &[String]) -> Result<i64, HomeError> {
        Ok(0)
    }
    fn expire(&self, _: &str, _: Duration) -> Result<bool, HomeError> {
        Ok(true)
    }
    fn ttl(&self, _: &str) -> Result<Option<Duration>, HomeError> {
        Ok(None)
    }
    fn increment(&self, _: &str, delta: i64) -> Result<i64, HomeError> {
        Ok(delta)
    }
    fn push(&self, key: &str, payload: &[u8], _: bool) -> Result<(), HomeError> {
        self.pushes
            .lock()
            .unwrap()
            .push((key.to_owned(), payload.to_vec()));
        Ok(())
    }
    fn request(&self, _: &str, payload: &[u8]) -> Result<Vec<u8>, TransportFailure> {
        self.requests.lock().unwrap().push(payload.to_vec());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Err(TransportFailure {
                    stage: DispatchFailureStage::BeforeSend,
                    message: "fixture exhausted".to_owned(),
                })
            })
    }
    fn request_with_timeout(
        &self,
        key: &str,
        payload: &[u8],
        _: Duration,
    ) -> Result<Vec<u8>, TransportFailure> {
        self.request(key, payload)
    }
}

#[derive(Debug)]
struct ExpectedFailure;
impl fmt::Display for ExpectedFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("expected failure")
    }
}
impl std::error::Error for ExpectedFailure {}

#[derive(Default)]
pub(super) struct TestExecutor {
    failures: AtomicUsize,
    seen: Mutex<Vec<ExecutorRequest>>,
}

impl TestExecutor {
    pub(super) fn failing(count: usize) -> Arc<Self> {
        Arc::new(Self {
            failures: AtomicUsize::new(count),
            seen: Mutex::new(Vec::new()),
        })
    }
    pub(super) fn seen(&self) -> Vec<ExecutorRequest> {
        self.seen.lock().unwrap().clone()
    }
    fn result(&self, request: ExecutorRequest) -> Result<ExecutorResponse, PluginExecutionError> {
        self.seen.lock().unwrap().push(request.clone());
        if self
            .failures
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_sub(1)
            })
            .is_ok()
        {
            return Err(Arc::new(ExpectedFailure));
        }
        Ok(ExecutorResponse {
            payload: request.payload,
            ..ExecutorResponse::default()
        })
    }
}

impl AuthRefresher for TestExecutor {
    fn refresh(&self, _: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        Ok(None)
    }
}

impl ProviderExecutor for TestExecutor {
    fn identifier(&self) -> &str {
        "codex"
    }
    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move { self.result(request) })
    }
    fn execute_stream<'a>(
        &'a self,
        request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(request.clone());
            let (sender, receiver) = tokio::sync::mpsc::channel(2);
            sender
                .send(ExecutorStreamChunk {
                    payload: request.payload,
                    error: None,
                })
                .await
                .unwrap();
            drop(sender);
            Ok(ExecutorStreamResponse {
                headers: Default::default(),
                chunks: receiver,
            })
        })
    }
    fn count_tokens<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(request.clone());
            Ok(ExecutorResponse {
                payload: request.payload.len().to_string().into_bytes(),
                ..ExecutorResponse::default()
            })
        })
    }
    fn http_request<'a>(
        &'a self,
        request: ExecutorHttpRequest,
    ) -> PluginFuture<'a, ExecutorHttpResponse> {
        Box::pin(async move {
            Ok(ExecutorHttpResponse {
                status_code: 200,
                body: request.body,
                ..ExecutorHttpResponse::default()
            })
        })
    }
}

pub(super) fn runtime(
    transport: Arc<TestHomeTransport>,
    executor: Arc<TestExecutor>,
) -> (Arc<HomeAuthRuntime>, Arc<Registry>) {
    runtime_with_preparer(transport, executor, None)
}

fn runtime_with_preparer(
    transport: Arc<TestHomeTransport>,
    executor: Arc<TestExecutor>,
    preparer: Option<Arc<dyn AuthPreparer>>,
) -> (Arc<HomeAuthRuntime>, Arc<Registry>) {
    let manager = Arc::new(super::api_key_model_capabilities_test::manager());
    let refresher: Arc<dyn AuthRefresher> = executor.clone();
    let execution: Arc<dyn ProviderExecutor> = executor;
    let mut registration = ProviderExecutorRegistration::new("codex", refresher)
        .unwrap()
        .with_execution(execution)
        .unwrap();
    if let Some(preparer) = preparer {
        registration = registration.with_auth_preparer(preparer);
    }
    manager.register_executor(Arc::new(registration));
    let facade: Arc<dyn HomeTransport> = transport;
    let client = Arc::new(Client::new(
        HomeConfig {
            enabled: true,
            ..HomeConfig::default()
        },
        facade,
    ));
    client.set_heartbeat(true);
    let registry = Arc::new(Registry::new());
    let runtime = Arc::new(HomeAuthRuntime::new(manager));
    runtime.publish_dispatch(client, registry.clone(), 1);
    (runtime, registry)
}

struct TestAuthPreparer {
    calls: AtomicUsize,
    fail: bool,
}

impl TestAuthPreparer {
    fn new(fail: bool) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            fail,
        })
    }
}

impl AuthPreparer for TestAuthPreparer {
    fn prepare<'a>(
        &'a self,
        auth: &'a mut Auth,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuthPreparationError>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::AcqRel);
            auth.metadata
                .insert("prepared".into(), serde_json::json!(true));
            if self.fail {
                return Err(Arc::new(ExpectedFailure) as AuthPreparationError);
            }
            Ok(())
        })
    }
}

pub(super) fn request(model: &str) -> ExecutorRequest {
    ExecutorRequest {
        model: model.to_owned(),
        payload: b"payload".to_vec(),
        metadata: BTreeMap::from([("request_id".to_owned(), serde_json::json!("request-1"))]),
        ..ExecutorRequest::default()
    }
}

#[tokio::test]
async fn selected_executor_handles_execute_and_count_paths() {
    let transport = TestHomeTransport::with_auth_ids(&["auth-execute", "auth-count"]);
    let executor = TestExecutor::failing(0);
    let (runtime, _) = runtime(transport, executor.clone());
    assert_eq!(
        runtime
            .execute_home(request("gpt"), "", false)
            .await
            .unwrap()
            .payload,
        b"payload"
    );
    assert_eq!(
        runtime
            .execute_home(request("gpt"), "", true)
            .await
            .unwrap()
            .payload,
        b"7"
    );
    let seen = executor.seen();
    assert_eq!(seen[0].auth_id, "auth-execute");
    assert_eq!(seen[1].auth_id, "auth-count");
}

#[tokio::test]
async fn stream_attempt_is_released_after_forwarding_finishes() {
    let transport = TestHomeTransport::with_auth_ids(&["auth-stream"]);
    let executor = TestExecutor::failing(0);
    let (runtime, registry) = runtime(transport, executor);
    let mut response = runtime
        .execute_home_stream(request("gpt"), "")
        .await
        .unwrap();
    assert_eq!(response.chunks.recv().await.unwrap().payload, b"payload");
    assert!(response.chunks.recv().await.is_none());
    assert!(registry
        .freeze_in_flight(chrono::Utc.timestamp_opt(1, 0).unwrap())
        .executions
        .is_empty());
}

#[tokio::test]
async fn auth_preparer_runs_before_non_stream_and_stream_execution() {
    let transport = TestHomeTransport::with_auth_ids(&["auth-execute", "auth-stream"]);
    let executor = TestExecutor::failing(0);
    let preparer = TestAuthPreparer::new(false);
    let (runtime, _) = runtime_with_preparer(transport, executor.clone(), Some(preparer.clone()));

    runtime
        .execute_home(request("gpt"), "session-execute", false)
        .await
        .unwrap();
    let mut stream = runtime
        .execute_home_stream(request("gpt-stream"), "session-stream")
        .await
        .unwrap();
    while stream.chunks.recv().await.is_some() {}

    let seen = executor.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].auth_metadata["prepared"], true);
    assert_eq!(seen[1].auth_metadata["prepared"], true);
    assert_eq!(preparer.calls.load(Ordering::Acquire), 2);
    assert_eq!(
        runtime
            .retained_selection("session-execute", "gpt")
            .unwrap()
            .auth()
            .metadata["prepared"],
        true
    );
    assert_eq!(
        runtime
            .retained_selection("session-stream", "gpt-stream")
            .unwrap()
            .auth()
            .metadata["prepared"],
        true
    );
}

#[tokio::test]
async fn auth_preparation_failure_is_terminal_and_never_reaches_executor() {
    let transport = TestHomeTransport::with_auth_ids(&["auth-execute", "auth-stream"]);
    let executor = TestExecutor::failing(0);
    let preparer = TestAuthPreparer::new(true);
    let (runtime, _) = runtime_with_preparer(transport, executor.clone(), Some(preparer.clone()));

    assert!(matches!(
        runtime.execute_home(request("gpt"), "", false).await,
        Err(HomeExecutionError::Preparation(_))
    ));
    assert!(matches!(
        runtime.execute_home_stream(request("gpt"), "").await,
        Err(HomeExecutionError::Preparation(_))
    ));
    assert!(executor.seen().is_empty());
    assert_eq!(preparer.calls.load(Ordering::Acquire), 2);
}

#[tokio::test]
async fn absent_auth_preparer_preserves_existing_execution_path() {
    let transport = TestHomeTransport::with_auth_ids(&["auth-execute"]);
    let executor = TestExecutor::failing(0);
    let (runtime, _) = runtime(transport, executor.clone());

    runtime
        .execute_home(request("gpt"), "", false)
        .await
        .unwrap();
    let seen = executor.seen();
    assert_eq!(seen.len(), 1);
    assert!(!seen[0].auth_metadata.contains_key("prepared"));
}
