// ref: sdk/cliproxy/auth/conductor_execution.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamChunk, ExecutorStreamResponse, PluginExecutionError, PluginFuture,
    ProviderExecutor,
};

use super::*;

#[path = "request_auth_prepare_race_test.rs"]
mod prepared_race_tests;

#[path = "request_auth_refresh_race_test.rs"]
mod refresh_race_tests;

#[derive(Default)]
struct MemoryAuthStore(Mutex<BTreeMap<String, Auth>>);

impl AuthStore for MemoryAuthStore {
    fn list(&self) -> Result<Vec<Auth>, AuthStoreError> {
        Ok(self.0.lock().unwrap().values().cloned().collect())
    }

    fn save(&self, auth: &Auth) -> Result<String, AuthStoreError> {
        self.0.lock().unwrap().insert(auth.id.clone(), auth.clone());
        Ok(auth.id.clone())
    }

    fn delete(&self, id: &str) -> Result<(), AuthStoreError> {
        self.0.lock().unwrap().remove(id);
        Ok(())
    }
}

#[derive(Default)]
struct MemoryCooldownStore(Mutex<Vec<CooldownStateRecord>>);

impl CooldownStateStore for MemoryCooldownStore {
    fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn save(&self, records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
        *self.0.lock().unwrap() = records.to_vec();
        Ok(())
    }
}

struct AllCapabilities;

impl SchedulerCapabilitySource for AllCapabilities {
    fn capabilities_for(&self, _: &str, _: &str) -> Option<SchedulerCapabilities> {
        Some(SchedulerCapabilities {
            weight: 1,
            supported_models: vec!["model".into()],
            ..SchedulerCapabilities::default()
        })
    }
}

struct NoopResume;

impl ModelResumeSink for NoopResume {
    fn resume_model(&self, _: &str, _: &str) {}
}

struct FixedClock(DateTime<Utc>);

impl GenericConductorClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Success,
    RefreshUnary,
    Failover,
    FastRequestError,
    CountRouteMissing,
    StreamBootstrap401,
    StreamTail401,
    StreamTailRequestError,
}

struct TestExecutor {
    mode: Mode,
    calls: AtomicUsize,
    refreshes: AtomicUsize,
    prepares: AtomicUsize,
}

impl TestExecutor {
    fn new(mode: Mode) -> Arc<Self> {
        Arc::new(Self {
            mode,
            calls: AtomicUsize::new(0),
            refreshes: AtomicUsize::new(0),
            prepares: AtomicUsize::new(0),
        })
    }

    fn error(status: u16, request_scoped: bool) -> PluginExecutionError {
        Arc::new(AuthError {
            code: if request_scoped {
                "request_scoped"
            } else if status == 401 {
                "unauthorized"
            } else {
                "upstream"
            }
            .into(),
            message: "test failure".into(),
            retryable: status >= 500,
            http_status: status,
        })
    }

    fn stale(request: &ExecutorRequest) -> bool {
        request
            .auth_metadata
            .get("access_token")
            .and_then(serde_json::Value::as_str)
            != Some("fresh")
    }
}

impl AuthRefresher for TestExecutor {
    fn refresh(&self, auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        self.refreshes.fetch_add(1, Ordering::AcqRel);
        auth.metadata
            .insert("access_token".into(), serde_json::json!("fresh"));
        Ok(None)
    }
}

impl AuthPreparer for TestExecutor {
    fn should_prepare(&self, auth: &Auth) -> bool {
        auth.attributes.get("prepared").map(String::as_str) != Some("true")
    }

    fn prepare<'a>(
        &'a self,
        auth: &'a mut Auth,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), AuthPreparationError>> + Send + 'a>,
    > {
        Box::pin(async move {
            self.prepares.fetch_add(1, Ordering::AcqRel);
            auth.attributes.insert("prepared".into(), "true".into());
            Ok(())
        })
    }
}

impl ProviderExecutor for TestExecutor {
    fn identifier(&self) -> &str {
        "claude"
    }

    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::AcqRel);
            assert_eq!(
                request.auth_attributes.get("prepared").map(String::as_str),
                Some("true")
            );
            assert_eq!(request.metadata["selected_auth_id"], request.auth_id);
            match self.mode {
                Mode::RefreshUnary if Self::stale(&request) => Err(Self::error(401, false)),
                Mode::Failover if request.auth_id == "auth-a" => Err(Self::error(503, false)),
                Mode::FastRequestError => Err(Self::error(422, true)),
                Mode::CountRouteMissing => Err(Self::error(404, false)),
                _ => Ok(ExecutorResponse {
                    payload: request.auth_id.into_bytes(),
                    ..ExecutorResponse::default()
                }),
            }
        })
    }

    fn count_tokens<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        self.execute(request)
    }

    fn execute_stream<'a>(
        &'a self,
        request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::AcqRel);
            let (sender, receiver) = tokio::sync::mpsc::channel(3);
            match self.mode {
                Mode::StreamBootstrap401 if Self::stale(&request) => {
                    sender
                        .send(ExecutorStreamChunk {
                            payload: Vec::new(),
                            error: Some(Self::error(401, false)),
                        })
                        .await
                        .unwrap();
                }
                Mode::StreamTail401 | Mode::StreamTailRequestError => {
                    sender
                        .send(ExecutorStreamChunk {
                            payload: b"committed".to_vec(),
                            error: None,
                        })
                        .await
                        .unwrap();
                    sender
                        .send(ExecutorStreamChunk {
                            payload: Vec::new(),
                            error: Some(Self::error(
                                if matches!(self.mode, Mode::StreamTail401) {
                                    401
                                } else {
                                    422
                                },
                                matches!(self.mode, Mode::StreamTailRequestError),
                            )),
                        })
                        .await
                        .unwrap();
                }
                _ => {
                    sender
                        .send(ExecutorStreamChunk {
                            payload: b"fresh-stream".to_vec(),
                            error: None,
                        })
                        .await
                        .unwrap();
                }
            }
            drop(sender);
            Ok(ExecutorStreamResponse {
                headers: Default::default(),
                chunks: receiver,
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

fn runtime(
    mode: Mode,
    auth_ids: &[&str],
) -> (
    Arc<GenericAuthRuntime>,
    Arc<TestExecutor>,
    Arc<AuthManager>,
    Arc<MemoryCooldownStore>,
) {
    let now = DateTime::parse_from_rfc3339("2026-08-04T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let lifecycle = Arc::new(AuthLifecycle::new(
        Arc::new(MemoryAuthStore::default()),
        Arc::new(RefreshSchedule::default()),
        Duration::from_secs(60),
    ));
    let manager = Arc::new(AuthManager::new(
        lifecycle.clone(),
        Arc::new(ProviderExecutorRegistry::default()),
        Arc::new(AuthSchedulerView::new(lifecycle, Arc::new(AllCapabilities))),
    ));
    for id in auth_ids {
        let mut auth = Auth::default();
        auth.id = (*id).into();
        auth.provider = "claude".into();
        auth.metadata
            .insert("access_token".into(), serde_json::json!("stale"));
        auth.metadata
            .insert("refresh_token".into(), serde_json::json!("refresh"));
        manager
            .register(auth, AuthMutationOptions::default(), now)
            .unwrap();
    }
    let executor = TestExecutor::new(mode);
    manager.register_executor(Arc::new(
        ProviderExecutorRegistration::new("claude", executor.clone())
            .unwrap()
            .with_execution(executor.clone())
            .unwrap()
            .with_auth_preparer(executor.clone()),
    ));
    let cooldown = Arc::new(MemoryCooldownStore::default());
    let runtime = Arc::new(GenericAuthRuntime::new_with_clock(
        manager.clone(),
        cooldown.clone(),
        Arc::new(NoopResume),
        Arc::new(FixedClock(now)),
    ));
    (runtime, executor, manager, cooldown)
}

fn request() -> ExecutorRequest {
    ExecutorRequest {
        model: "model".into(),
        ..ExecutorRequest::default()
    }
}

#[tokio::test]
async fn unary_selection_preparation_and_one_401_refresh_are_end_to_end() {
    let (runtime, executor, manager, cooldown) = runtime(Mode::RefreshUnary, &["auth-a"]);
    let response = runtime
        .execute(&["claude".into()], request())
        .await
        .unwrap();
    assert_eq!(response.payload, b"auth-a");
    assert_eq!(executor.calls.load(Ordering::Acquire), 2);
    assert_eq!(executor.refreshes.load(Ordering::Acquire), 1);
    assert_eq!(executor.prepares.load(Ordering::Acquire), 1);
    let auth = manager.lifecycle().get_cached("auth-a").unwrap();
    assert_eq!(auth.metadata["access_token"], "fresh");
    assert_eq!((auth.success, auth.failed), (1, 0));
    assert!(cooldown.load().unwrap().is_empty());
}

#[tokio::test]
async fn concurrent_requests_coalesce_request_auth_preparation_per_identity() {
    let (runtime, executor, _, _) = runtime(Mode::Success, &["auth-a"]);
    let providers = ["claude".into()];
    let left = runtime.execute(&providers, request());
    let right = runtime.execute(&providers, request());
    let (left, right) = tokio::join!(left, right);
    assert!(left.is_ok());
    assert!(right.is_ok());
    assert_eq!(executor.prepares.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn failed_credential_is_cooled_before_next_selected_account_succeeds() {
    let (runtime, executor, manager, cooldown) = runtime(Mode::Failover, &["auth-a", "auth-b"]);
    let response = runtime
        .count_tokens(&["claude".into()], request())
        .await
        .unwrap();
    assert_eq!(response.payload, b"auth-b");
    assert_eq!(executor.calls.load(Ordering::Acquire), 2);
    let records = cooldown.load().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].auth_id, "auth-a");
    assert_eq!(records[0].reason, "transient_upstream_error");
    assert_eq!(manager.lifecycle().get_cached("auth-a").unwrap().failed, 1);
    assert_eq!(manager.lifecycle().get_cached("auth-b").unwrap().success, 1);
}

#[tokio::test]
async fn fast_request_scoped_error_neither_refreshes_nor_switches_credentials() {
    let (runtime, executor, _, cooldown) = runtime(Mode::FastRequestError, &["auth-a", "auth-b"]);
    assert!(matches!(
        runtime.execute(&["claude".into()], request()).await,
        Err(GenericExecutionError::Provider(_))
    ));
    assert_eq!(executor.calls.load(Ordering::Acquire), 1);
    assert_eq!(executor.refreshes.load(Ordering::Acquire), 0);
    assert!(cooldown.load().unwrap().is_empty());
}

#[tokio::test]
async fn generic_count_route_404_does_not_suspend_working_message_credentials() {
    let (runtime, executor, _, cooldown) = runtime(Mode::CountRouteMissing, &["auth-a", "auth-b"]);
    assert!(matches!(
        runtime.count_tokens(&["claude".into()], request()).await,
        Err(GenericExecutionError::Provider(_))
    ));
    assert_eq!(executor.calls.load(Ordering::Acquire), 2);
    assert!(cooldown.load().unwrap().is_empty());
}

#[tokio::test]
async fn stream_bootstrap_401_refreshes_once_before_commit() {
    let (runtime, executor, _, cooldown) = runtime(Mode::StreamBootstrap401, &["auth-a"]);
    let mut stream = runtime
        .execute_stream(&["claude".into()], request())
        .await
        .unwrap();
    let first = stream.chunks.recv().await.unwrap();
    assert_eq!(first.payload, b"fresh-stream");
    assert!(first.error.is_none());
    assert!(stream.chunks.recv().await.is_none());
    assert_eq!(executor.calls.load(Ordering::Acquire), 2);
    assert_eq!(executor.refreshes.load(Ordering::Acquire), 1);
    assert!(cooldown.load().unwrap().is_empty());
}

#[tokio::test]
async fn committed_stream_tail_never_replays_and_request_scoped_tail_is_neutral() {
    for mode in [Mode::StreamTail401, Mode::StreamTailRequestError] {
        let (runtime, executor, _, cooldown) = runtime(mode, &["auth-a", "auth-b"]);
        let mut stream = runtime
            .execute_stream(&["claude".into()], request())
            .await
            .unwrap();
        assert_eq!(stream.chunks.recv().await.unwrap().payload, b"committed");
        assert!(stream.chunks.recv().await.unwrap().error.is_some());
        assert!(stream.chunks.recv().await.is_none());
        assert_eq!(executor.calls.load(Ordering::Acquire), 1);
        assert_eq!(executor.refreshes.load(Ordering::Acquire), 0);
        let records = cooldown.load().unwrap();
        if matches!(mode, Mode::StreamTail401) {
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].reason, "unauthorized");
        } else {
            assert!(records.is_empty());
        }
    }
}
