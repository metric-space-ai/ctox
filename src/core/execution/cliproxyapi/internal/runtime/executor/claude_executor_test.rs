// ref: internal/runtime/executor/claude_executor_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use tokio::sync::mpsc;

use super::*;
use crate::internal::auth::claude::{
    ClaudeCredentialHandles, ClaudeRefreshCoordinator, ClaudeRefreshTransport, ClaudeSecretHandle,
    ClaudeSecretKind, ClaudeSecretStore, ClaudeStoredCredentials, RefreshClock,
    RefreshHttpResponse, RefreshRequest, RefreshTransportFailure, SecretStoreError, SecretString,
    CLAUDE_DEVICE_IDS_METADATA_KEY,
};
use crate::internal::runtime::executor::{
    AccountStateClock, ClaudeCloakPolicy, ClaudeExecutionRequestContext, ClaudeMessagesRequest,
    ClaudeMessagesResponse, ClaudeMessagesStreamResponse, ClaudeMessagesStreamingTransport,
    ClaudeMessagesTransport, ClaudeSubscriptionAuth, ClaudeSubscriptionMessagesExecutor,
    ClaudeUpstreamTarget,
};
use crate::sdk::cliproxy::auth::{
    AccountCandidate, AccountRouter, CooldownConductor, CooldownStateRecord, CooldownStateStore,
    CooldownStoreError,
};
use crate::sdk::cliproxy::executor::RequestTerminatedError;
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorRequest, Headers, HostHttpClient, HttpRequest, HttpResponse,
    HttpStreamResponse, PluginFuture, ProviderExecutor,
};

struct MemorySecretStore(Mutex<ClaudeStoredCredentials>);

impl MemorySecretStore {
    fn new() -> Self {
        Self(Mutex::new(ClaudeStoredCredentials::new(
            SecretString::new("access-token").unwrap(),
            SecretString::new("refresh-token").unwrap(),
        )))
    }
}

impl ClaudeSecretStore for MemorySecretStore {
    fn load_credentials(
        &self,
        _handles: &ClaudeCredentialHandles,
    ) -> Result<ClaudeStoredCredentials, SecretStoreError> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn store_credentials(
        &self,
        _handles: &ClaudeCredentialHandles,
        credentials: &ClaudeStoredCredentials,
    ) -> Result<(), SecretStoreError> {
        *self.0.lock().unwrap() = credentials.clone();
        Ok(())
    }
}

struct FixedRefreshClock;

impl RefreshClock for FixedRefreshClock {
    fn now(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000)
    }

    fn sleep(
        &self,
        _duration: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<(), RefreshTransportFailure>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}

struct RefreshTransport;

impl ClaudeRefreshTransport for RefreshTransport {
    fn execute<'a>(
        &'a self,
        _request: &'a RefreshRequest,
        _timeout: Duration,
    ) -> Pin<
        Box<dyn Future<Output = Result<RefreshHttpResponse, RefreshTransportFailure>> + Send + 'a>,
    > {
        Box::pin(async {
            Ok(RefreshHttpResponse::new(
                200,
                None,
                None,
                br#"{"access_token":"new-access","refresh_token":"new-refresh","expires_in":3600}"#
                    .to_vec(),
            ))
        })
    }
}

struct FixedMessagesTransport {
    status: u16,
    body: Vec<u8>,
    headers: Headers,
    calls: AtomicUsize,
    count_authorizations: Mutex<Vec<String>>,
}

#[derive(Clone, Debug)]
struct CapturedClaudeRequest {
    body: Vec<u8>,
    session_id: String,
    user_agent: String,
    os: String,
    authorization: String,
    betas: Vec<String>,
}

#[derive(Default)]
struct CapturingMessagesTransport {
    requests: Mutex<Vec<CapturedClaudeRequest>>,
}

impl ClaudeMessagesTransport for CapturingMessagesTransport {
    fn execute<'a>(
        &'a self,
        request: &'a ClaudeMessagesRequest,
        _timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<ClaudeMessagesResponse, super::ClaudeMessagesTransportFailure>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.requests.lock().unwrap().push(CapturedClaudeRequest {
                body: request.body().to_vec(),
                session_id: request.fingerprint().session_id().to_owned(),
                user_agent: request.fingerprint().device().user_agent().to_owned(),
                os: request.fingerprint().device().os().to_owned(),
                authorization: request.authorization().expose_header_value().to_owned(),
                betas: request.betas().to_vec(),
            });
            Ok(ClaudeMessagesResponse::new(
                200,
                br#"{"type":"message"}"#.to_vec(),
            ))
        })
    }

    fn execute_count_tokens<'a>(
        &'a self,
        request: &'a ClaudeMessagesRequest,
        _timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<ClaudeMessagesResponse, super::ClaudeMessagesTransportFailure>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.requests.lock().unwrap().push(CapturedClaudeRequest {
                body: request.body().to_vec(),
                session_id: request.fingerprint().session_id().to_owned(),
                user_agent: request.fingerprint().device().user_agent().to_owned(),
                os: request.fingerprint().device().os().to_owned(),
                authorization: request.authorization().expose_header_value().to_owned(),
                betas: request.betas().to_vec(),
            });
            Ok(
                ClaudeMessagesResponse::new(200, br#"{"input_tokens":17}"#.to_vec()).with_headers(
                    Headers::from([("request-id".to_owned(), vec!["count-request".to_owned()])]),
                ),
            )
        })
    }
}

impl FixedMessagesTransport {
    fn new(status: u16, body: &[u8]) -> Self {
        Self {
            status,
            body: body.to_vec(),
            headers: Headers::new(),
            calls: AtomicUsize::new(0),
            count_authorizations: Mutex::new(Vec::new()),
        }
    }

    fn with_headers(mut self, headers: Headers) -> Self {
        self.headers = headers;
        self
    }
}

impl ClaudeMessagesTransport for FixedMessagesTransport {
    fn execute<'a>(
        &'a self,
        _request: &'a ClaudeMessagesRequest,
        _timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<ClaudeMessagesResponse, super::ClaudeMessagesTransportFailure>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ClaudeMessagesResponse::new(self.status, self.body.clone())
                .with_headers(self.headers.clone()))
        })
    }

    fn execute_count_tokens<'a>(
        &'a self,
        request: &'a ClaudeMessagesRequest,
        _timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<ClaudeMessagesResponse, super::ClaudeMessagesTransportFailure>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.count_authorizations
                .lock()
                .unwrap()
                .push(request.authorization().expose_header_value().to_owned());
            Ok(ClaudeMessagesResponse::new(self.status, self.body.clone())
                .with_headers(self.headers.clone()))
        })
    }
}

struct MessagesOnlyTransport;

impl ClaudeMessagesTransport for MessagesOnlyTransport {
    fn execute<'a>(
        &'a self,
        _request: &'a ClaudeMessagesRequest,
        _timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<ClaudeMessagesResponse, super::ClaudeMessagesTransportFailure>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async { Ok(ClaudeMessagesResponse::new(200, b"unused".to_vec())) })
    }
}

struct ArbitraryHostClient;

impl HostHttpClient for ArbitraryHostClient {
    fn execute<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async {
            Ok(HttpResponse {
                status_code: 200,
                headers: Headers::new(),
                body: br#"{"input_tokens":999}"#.to_vec(),
            })
        })
    }

    fn execute_stream<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async { panic!("arbitrary client must never own first-party count_tokens") })
    }
}

struct FixedStreamingTransport {
    status: u16,
    headers: Headers,
    error_body: Vec<u8>,
    chunks: Vec<Result<Vec<u8>, super::ClaudeMessagesTransportFailure>>,
    calls: AtomicUsize,
}

impl ClaudeMessagesStreamingTransport for FixedStreamingTransport {
    fn execute_stream<'a>(
        &'a self,
        _request: &'a ClaudeMessagesRequest,
        _timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        ClaudeMessagesStreamResponse,
                        super::ClaudeMessagesTransportFailure,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let (sender, receiver) = mpsc::channel(8);
            let chunks = self.chunks.clone();
            tokio::spawn(async move {
                for chunk in chunks {
                    if sender.send(chunk).await.is_err() {
                        break;
                    }
                }
            });
            Ok(
                ClaudeMessagesStreamResponse::new(self.status, None, receiver)
                    .with_headers(self.headers.clone())
                    .with_error_body(self.error_body.clone()),
            )
        })
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

struct FixedAccountClock;

impl AccountStateClock for FixedAccountClock {
    fn now_ms(&self) -> i64 {
        10_000
    }
}

fn handles(auth_id: &str) -> ClaudeCredentialHandles {
    ClaudeCredentialHandles::new(
        ClaudeSecretHandle::new(
            format!("subscriptions/{auth_id}"),
            "access",
            ClaudeSecretKind::AccessToken,
        )
        .unwrap(),
        ClaudeSecretHandle::new(
            format!("subscriptions/{auth_id}"),
            "refresh",
            ClaudeSecretKind::RefreshToken,
        )
        .unwrap(),
    )
    .unwrap()
}

fn account_executor(
    auth_id: &str,
    transport: Arc<dyn ClaudeMessagesTransport>,
    stream_transport: Option<Arc<dyn ClaudeMessagesStreamingTransport>>,
    conductor: Arc<CooldownConductor>,
) -> Arc<ClaudeSubscriptionMessagesExecutor> {
    let auth = Arc::new(ClaudeSubscriptionAuth::new(
        handles(auth_id),
        Arc::new(MemorySecretStore::new()),
        Arc::new(RefreshTransport),
        Arc::new(FixedRefreshClock),
        Arc::new(ClaudeRefreshCoordinator::default()),
    ));
    let executor =
        ClaudeSubscriptionMessagesExecutor::new(auth, transport, Duration::from_secs(30))
            .with_account_state_clock(auth_id, conductor, Arc::new(FixedAccountClock))
            .unwrap();
    Arc::new(match stream_transport {
        Some(stream_transport) => executor.with_stream_transport(stream_transport),
        None => executor,
    })
}

fn account_executor_with_policy(
    auth_id: &str,
    transport: Arc<dyn ClaudeMessagesTransport>,
    conductor: Arc<CooldownConductor>,
    policy: ClaudeCloakPolicy,
) -> Arc<ClaudeSubscriptionMessagesExecutor> {
    let auth = Arc::new(ClaudeSubscriptionAuth::new(
        handles(auth_id),
        Arc::new(MemorySecretStore::new()),
        Arc::new(RefreshTransport),
        Arc::new(FixedRefreshClock),
        Arc::new(ClaudeRefreshCoordinator::default()),
    ));
    Arc::new(
        ClaudeSubscriptionMessagesExecutor::new(auth, transport, Duration::from_secs(30))
            .with_account_state_clock(auth_id, conductor, Arc::new(FixedAccountClock))
            .unwrap()
            .with_cloak_policy(policy),
    )
}

fn adapter(
    accounts: Vec<(&str, Arc<ClaudeSubscriptionMessagesExecutor>)>,
    cooldowns: Arc<MemoryCooldownStore>,
) -> ClaudeProviderExecutor {
    let candidates = accounts
        .iter()
        .map(|(auth_id, _)| AccountCandidate {
            auth_id: (*auth_id).to_owned(),
            provider: "claude".to_owned(),
            priority: 0,
            weight: 1,
            websocket_enabled: false,
            supported_models: vec!["sonnet".to_owned()],
            disabled: false,
        })
        .collect();
    let executors = accounts
        .iter()
        .map(|(auth_id, executor)| ((*auth_id).to_owned(), Arc::clone(executor)))
        .collect();
    let target = ClaudeUpstreamTarget::new("https", "api.anthropic.com").unwrap();
    let targets = accounts
        .iter()
        .map(|(auth_id, _)| ((*auth_id).to_owned(), target.clone()))
        .collect::<HashMap<_, _>>();
    let pool = ClaudeSubscriptionAccountPool::with_clock(
        Arc::new(AccountRouter::new(cooldowns)),
        candidates,
        executors,
        Arc::new(FixedAccountClock),
    )
    .unwrap()
    .with_targets(targets)
    .unwrap();
    ClaudeProviderExecutor::new(Arc::new(pool))
}

fn pool_with_disabled_fallback(
    first: Arc<ClaudeSubscriptionMessagesExecutor>,
    cooldowns: Arc<dyn CooldownStateStore>,
) -> ClaudeSubscriptionAccountPool {
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let fallback = account_executor(
        "account-b",
        Arc::new(MessagesOnlyTransport),
        None,
        conductor,
    );
    let candidates = ["account-a", "account-b"].map(|auth_id| AccountCandidate {
        auth_id: auth_id.to_owned(),
        provider: "claude".to_owned(),
        priority: 0,
        weight: 1,
        websocket_enabled: false,
        supported_models: vec!["claude-opus-5-5".to_owned()],
        disabled: auth_id == "account-b",
    });
    let target = ClaudeUpstreamTarget::new("https", "api.anthropic.com").unwrap();
    ClaudeSubscriptionAccountPool::with_clock(
        Arc::new(AccountRouter::new(cooldowns)),
        candidates.to_vec(),
        HashMap::from([
            ("account-a".to_owned(), first),
            ("account-b".to_owned(), fallback),
        ]),
        Arc::new(FixedAccountClock),
    )
    .unwrap()
    .with_targets(HashMap::from([
        ("account-a".to_owned(), target.clone()),
        ("account-b".to_owned(), target),
    ]))
    .unwrap()
}

#[tokio::test]
async fn pool_preserves_buffered_upstream_rejection_when_fallback_is_disabled() {
    for status in [429, 503] {
        let cooldowns = Arc::new(MemoryCooldownStore::default());
        let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
        let body = br#"{"error":{"type":"overloaded_error","message":"upstream failure"}}"#;
        let transport = Arc::new(FixedMessagesTransport::new(status, body).with_headers(
            Headers::from([("request-id".to_owned(), vec!["upstream-request".to_owned()])]),
        ));
        let first = account_executor("account-a", transport.clone(), None, conductor);
        let pool = pool_with_disabled_fallback(first, cooldowns);
        let outcome = pool
            .execute_configured(
                "claude-opus-5-5",
                br#"{"model":"claude-opus-5-5","messages":[{"role":"user","content":"Hi"}]}"#
                    .to_vec(),
                false,
            )
            .await
            .unwrap();
        assert_eq!(outcome.outcome().response().status(), status);
        assert_eq!(outcome.outcome().response().body(), body);
        assert_eq!(outcome.attempted_auth_ids(), &["account-a".to_owned()]);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn pool_preserves_streaming_upstream_rejection_when_fallback_is_disabled() {
    for status in [429, 503] {
        let cooldowns = Arc::new(MemoryCooldownStore::default());
        let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
        let body = br#"{"error":{"type":"overloaded_error","message":"upstream failure"}}"#;
        let transport = Arc::new(FixedStreamingTransport {
            status,
            headers: Headers::new(),
            error_body: body.to_vec(),
            chunks: Vec::new(),
            calls: AtomicUsize::new(0),
        });
        let first = account_executor(
            "account-a",
            Arc::new(MessagesOnlyTransport),
            Some(transport.clone()),
            conductor,
        );
        let pool = pool_with_disabled_fallback(first, cooldowns);
        let outcome = pool.execute_stream_configured(
            "claude-opus-5-5",
            br#"{"model":"claude-opus-5-5","messages":[{"role":"user","content":"Hi"}],"stream":true}"#.to_vec(),
        ).await.unwrap();
        assert_eq!(outcome.outcome().response().status(), status);
        assert_eq!(outcome.outcome().response().error_body(), body);
        assert_eq!(outcome.attempted_auth_ids(), &["account-a".to_owned()]);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    }
}

#[derive(Default)]
struct CooldownStoreFailingAfterSave {
    failed: std::sync::atomic::AtomicBool,
}

impl CooldownStateStore for CooldownStoreFailingAfterSave {
    fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
        if self.failed.load(Ordering::SeqCst) {
            Err(CooldownStoreError::Read)
        } else {
            Ok(Vec::new())
        }
    }

    fn save(&self, _records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
        self.failed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn pool_preserves_fail_closed_storage_errors_after_an_upstream_attempt() {
    use crate::sdk::cliproxy::auth::AccountRoutingError;

    let cooldowns = Arc::new(CooldownStoreFailingAfterSave::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let transport = Arc::new(FixedMessagesTransport::new(
        429,
        br#"{"error":{"type":"rate_limit_error"}}"#,
    ));
    let first = account_executor("account-a", transport.clone(), None, conductor);
    let pool = pool_with_disabled_fallback(first, cooldowns);
    let result = pool
        .execute_configured(
            "claude-opus-5-5",
            br#"{"model":"claude-opus-5-5","messages":[{"role":"user","content":"Hi"}]}"#.to_vec(),
            false,
        )
        .await;
    assert!(matches!(
        result,
        Err(ClaudeAccountPoolError::Routing(AccountRoutingError::Store(
            CooldownStoreError::Read
        )))
    ));
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
}

fn request(auth_id: &str, stream: bool) -> ExecutorRequest {
    ExecutorRequest {
        auth_id: auth_id.to_owned(),
        auth_provider: "claude".to_owned(),
        model: "sonnet".to_owned(),
        format: "claude".to_owned(),
        stream,
        payload: br#"{"model":"sonnet"}"#.to_vec(),
        ..ExecutorRequest::default()
    }
}

#[tokio::test]
async fn unary_uses_exact_manager_selected_auth_without_pool_failover() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let transport_a = Arc::new(FixedMessagesTransport::new(200, b"account-a"));
    let transport_b = Arc::new(FixedMessagesTransport::new(200, b"account-b"));
    let executor_a = account_executor("account-a", transport_a.clone(), None, conductor.clone());
    let executor_b = account_executor("account-b", transport_b.clone(), None, conductor);
    let adapter = adapter(
        vec![("account-a", executor_a), ("account-b", executor_b)],
        cooldowns,
    );

    let response = adapter.execute(request("account-b", false)).await.unwrap();

    assert_eq!(response.payload, b"account-b");
    assert_eq!(transport_a.calls.load(Ordering::SeqCst), 0);
    assert_eq!(transport_b.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn selected_upstream_failure_is_typed_and_never_fails_over() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let transport_a = Arc::new(FixedMessagesTransport::new(429, b"quota"));
    let transport_b = Arc::new(FixedMessagesTransport::new(200, b"account-b"));
    let adapter = adapter(
        vec![
            (
                "account-a",
                account_executor("account-a", transport_a.clone(), None, conductor.clone()),
            ),
            (
                "account-b",
                account_executor("account-b", transport_b.clone(), None, conductor),
            ),
        ],
        cooldowns,
    );

    let error = adapter
        .execute(request("account-a", false))
        .await
        .unwrap_err();
    let error = error
        .as_ref()
        .downcast_ref::<ClaudeProviderExecutorError>()
        .unwrap();

    assert_eq!(error.status(), Some(429));
    assert_eq!(transport_a.calls.load(Ordering::SeqCst), 1);
    assert_eq!(transport_b.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn stream_forwards_bootstrap_and_terminal_transport_error() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let unary = Arc::new(FixedMessagesTransport::new(200, b"unused"));
    let streaming = Arc::new(FixedStreamingTransport {
        status: 200,
        headers: Headers::new(),
        error_body: Vec::new(),
        chunks: vec![
            Ok(b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg\"}}\n\n".to_vec()),
            Ok(b"data: {\"type\":\"content_block_delta\"}\n\n".to_vec()),
            Err(super::ClaudeMessagesTransportFailure::Protocol),
            Ok(b"must-not-pass".to_vec()),
        ],
        calls: AtomicUsize::new(0),
    });
    let executor = account_executor("account-a", unary, Some(streaming.clone()), conductor);
    let adapter = adapter(vec![("account-a", executor)], cooldowns);

    let mut response = adapter
        .execute_stream(request("account-a", true))
        .await
        .unwrap();
    let bootstrap = response.chunks.recv().await.unwrap();
    let delta = response.chunks.recv().await.unwrap();
    let terminal = response.chunks.recv().await.unwrap();

    assert!(String::from_utf8_lossy(&bootstrap.payload).contains("message_start"));
    assert!(String::from_utf8_lossy(&delta.payload).contains("content_block_delta"));
    assert!(terminal.payload.is_empty());
    assert!(terminal.error.is_some());
    assert!(response.chunks.recv().await.is_none());
    assert_eq!(streaming.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unary_success_propagates_upstream_headers() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let headers = [("request-id".to_owned(), vec!["req-123".to_owned()])]
        .into_iter()
        .collect();
    let transport =
        Arc::new(FixedMessagesTransport::new(200, br#"{"type":"message"}"#).with_headers(headers));
    let executor = account_executor("account-a", transport, None, conductor);
    let adapter = adapter(vec![("account-a", executor)], cooldowns);

    let response = adapter.execute(request("account-a", false)).await.unwrap();

    assert_eq!(
        response.headers.get("request-id"),
        Some(&vec!["req-123".to_owned()])
    );
}

#[tokio::test]
async fn fast_stream_error_preserves_status_headers_and_body() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let unary = Arc::new(FixedMessagesTransport::new(200, b"unused"));
    let headers = [
        ("request-id".to_owned(), vec!["req-fast".to_owned()]),
        ("content-encoding".to_owned(), vec!["gzip".to_owned()]),
        ("content-length".to_owned(), vec!["999".to_owned()]),
    ]
    .into_iter()
    .collect();
    let streaming = Arc::new(FixedStreamingTransport {
        status: 429,
        headers,
        error_body: br#"{"error":{"message":"fast mode usage credits required"}}"#.to_vec(),
        chunks: Vec::new(),
        calls: AtomicUsize::new(0),
    });
    let executor = account_executor("account-a", unary, Some(streaming), conductor);
    let adapter = adapter(vec![("account-a", executor)], cooldowns);
    let mut fast_request = request("account-a", true);
    fast_request.payload = br#"{"model":"sonnet","speed":"fast"}"#.to_vec();

    let error = match adapter.execute_stream(fast_request).await {
        Ok(_) => panic!("fast upstream failure unexpectedly succeeded"),
        Err(error) => error,
    };
    let terminated = error
        .as_ref()
        .downcast_ref::<RequestTerminatedError>()
        .expect("fast response remains a direct terminated response");

    assert_eq!(terminated.http_status, 429);
    assert_eq!(
        terminated.headers.get("request-id"),
        Some(&vec!["req-fast".to_owned()])
    );
    assert!(!terminated.headers.contains_key("content-encoding"));
    assert!(!terminated.headers.contains_key("content-length"));
    assert_eq!(
        terminated.body,
        br#"{"error":{"message":"fast mode usage credits required"}}"#
    );
}

#[tokio::test]
async fn invalid_or_unimplemented_contracts_fail_closed() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let executor = account_executor(
        "account-a",
        Arc::new(MessagesOnlyTransport),
        None,
        conductor,
    );
    let adapter = adapter(vec![("account-a", executor)], cooldowns);

    let mut mismatch = request("account-a", false);
    mismatch.auth_provider = "codex".to_owned();
    assert!(adapter.execute(mismatch).await.is_err());
    assert!(adapter.execute(request("unknown", false)).await.is_err());
    let mut count_request = request("account-a", false);
    count_request.payload = br#"{"messages":[{"role":"user","content":"hello"}]}"#.to_vec();
    count_request.http_client = Some(Arc::new(ArbitraryHostClient));
    assert!(adapter.count_tokens(count_request).await.is_err());

    let mut invalid_count = request("account-a", false);
    invalid_count.payload = br#"{"messages":[{"role":"system","content":"no"}]}"#.to_vec();
    assert!(adapter.count_tokens(invalid_count).await.is_err());
    assert!(adapter
        .http_request(ExecutorHttpRequest::default())
        .await
        .is_err());
}

#[tokio::test]
async fn provider_count_tokens_preserves_strong_native_session_and_profile() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let transport = Arc::new(CapturingMessagesTransport::default());
    let executor = account_executor_with_policy(
        "account-a",
        transport.clone(),
        conductor,
        ClaudeCloakPolicy::oauth_default(),
    );
    let adapter = adapter(vec![("account-a", executor)], cooldowns);
    let session_id = "11111111-2222-4333-8444-555555555555";
    let mut count = request("account-a", false);
    count.payload = br#"{"model":"sonnet","system":"native caller system","messages":[{"role":"user","content":"hello"}],"metadata":{"user_id":"must-not-reach-count-endpoint"}}"#.to_vec();
    count.original_request = count.payload.clone();
    count.headers = Headers::from([
        ("X-App".to_owned(), vec!["cli".to_owned()]),
        (
            "User-Agent".to_owned(),
            vec!["claude-cli/2.1.280 (external, cli)".to_owned()],
        ),
        (
            "Anthropic-Beta".to_owned(),
            vec!["claude-code-20250219".to_owned()],
        ),
        (
            "X-Claude-Code-Session-Id".to_owned(),
            vec![session_id.to_owned()],
        ),
        (
            "X-Stainless-Package-Version".to_owned(),
            vec!["0.112.1".to_owned()],
        ),
        (
            "X-Stainless-Runtime-Version".to_owned(),
            vec!["v26.3.0".to_owned()],
        ),
        ("X-Stainless-Os".to_owned(), vec!["MacOS".to_owned()]),
        ("X-Stainless-Arch".to_owned(), vec!["arm64".to_owned()]),
    ]);
    let response = adapter.count_tokens(count).await.unwrap();

    assert_eq!(response.payload, br#"{"input_tokens":17}"#);
    {
        let requests = transport.requests.lock().unwrap();
        let captured = requests.last().unwrap();
        assert_eq!(captured.session_id, session_id);
        assert_eq!(captured.user_agent, "claude-cli/2.1.280 (external, cli)");
        assert_eq!(captured.authorization, "Bearer access-token");
        let body: serde_json::Value = serde_json::from_slice(&captured.body).unwrap();
        assert_eq!(body["system"], "native caller system");
        assert!(body.get("metadata").is_none());
        assert!(captured
            .betas
            .iter()
            .any(|beta| beta == "token-counting-2024-11-01"));
    }

    let metadata_session = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    let mut metadata_count = request("account-a", false);
    metadata_count.payload =
        br#"{"model":"sonnet","messages":[{"role":"user","content":"second"}]}"#.to_vec();
    metadata_count.original_request = metadata_count.payload.clone();
    metadata_count.headers = Headers::from([
        ("X-App".to_owned(), vec!["cli".to_owned()]),
        (
            "User-Agent".to_owned(),
            vec!["claude-cli/2.1.280 (external, cli)".to_owned()],
        ),
        (
            "Anthropic-Beta".to_owned(),
            vec!["claude-code-20250219".to_owned()],
        ),
    ]);
    metadata_count.metadata.insert(
        "execution_session_id".to_owned(),
        serde_json::Value::String(metadata_session.to_owned()),
    );
    adapter.count_tokens(metadata_count).await.unwrap();

    let requests = transport.requests.lock().unwrap();
    assert_eq!(requests.last().unwrap().session_id, metadata_session);
}

#[tokio::test]
async fn provider_count_tokens_refreshes_and_rebuilds_exactly_once_after_401() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let transport = Arc::new(FixedMessagesTransport::new(
        401,
        br#"{"type":"error","error":{"type":"authentication_error"}}"#,
    ));
    let executor = account_executor_with_policy(
        "account-a",
        transport.clone(),
        conductor,
        ClaudeCloakPolicy::oauth_default(),
    );
    let adapter = adapter(vec![("account-a", executor)], cooldowns);
    let mut count = request("account-a", false);
    count.payload =
        br#"{"model":"sonnet","messages":[{"role":"user","content":"hello"}]}"#.to_vec();

    let error = adapter.count_tokens(count).await.unwrap_err();
    let terminated = error
        .as_ref()
        .downcast_ref::<RequestTerminatedError>()
        .unwrap();
    assert_eq!(terminated.http_status, 401);
    assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        *transport.count_authorizations.lock().unwrap(),
        vec![
            "Bearer access-token".to_owned(),
            "Bearer new-access".to_owned()
        ]
    );
}

#[tokio::test]
async fn provider_path_applies_verified_native_identity_and_device_profile_request_scoped() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let transport = Arc::new(CapturingMessagesTransport::default());
    let mut policy = ClaudeCloakPolicy::oauth_default();
    policy.mode = "always".to_owned();
    let executor = account_executor_with_policy("account-a", transport.clone(), conductor, policy);
    let adapter = adapter(vec![("account-a", executor)], cooldowns);

    let session_id = "018f9970-00d1-7d24-bc67-9f4d595c7901";
    let account_uuid = "018f9970-00d1-7d24-bc67-9f4d595c7902";
    let device_id = "a".repeat(64);
    let caller_user_id = serde_json::json!({
        "device_id": device_id.clone(),
        "account_uuid": account_uuid,
        "session_id": session_id,
    })
    .to_string();
    let mut provider_request = request("account-a", false);
    provider_request.payload = serde_json::to_vec(&serde_json::json!({
        "model": "sonnet",
        "system": [{"type": "text", "text": "caller-owned-system"}],
        "messages": [{"role": "user", "content": "hello from native client"}],
        "metadata": {"user_id": caller_user_id},
    }))
    .unwrap();
    provider_request.original_request = provider_request.payload.clone();
    provider_request.headers = [
        ("X-App".to_owned(), vec!["cli".to_owned()]),
        (
            "User-Agent".to_owned(),
            vec!["claude-cli/2.2.0 (external, cli)".to_owned()],
        ),
        (
            "Anthropic-Beta".to_owned(),
            vec!["claude-code-20250219".to_owned()],
        ),
        (
            "X-Claude-Code-Session-Id".to_owned(),
            vec![session_id.to_owned()],
        ),
        (
            "X-Stainless-Package-Version".to_owned(),
            vec!["0.95.0".to_owned()],
        ),
        (
            "X-Stainless-Runtime-Version".to_owned(),
            vec!["v26.4.0".to_owned()],
        ),
        ("X-Stainless-Os".to_owned(), vec!["Windows".to_owned()]),
        ("X-Stainless-Arch".to_owned(), vec!["x64".to_owned()]),
    ]
    .into_iter()
    .collect();
    provider_request.auth_metadata = BTreeMap::from([
        (
            "account_uuid".to_owned(),
            serde_json::Value::String(account_uuid.to_owned()),
        ),
        (
            CLAUDE_DEVICE_IDS_METADATA_KEY.to_owned(),
            serde_json::json!([device_id]),
        ),
    ]);
    provider_request.auth_attributes = BTreeMap::from([
        (
            "claude_header_user_agent".to_owned(),
            "claude-cli/2.2.0 (external, cli)".to_owned(),
        ),
        (
            "claude_header_package_version".to_owned(),
            "0.95.0".to_owned(),
        ),
        (
            "claude_header_runtime_version".to_owned(),
            "v26.4.0".to_owned(),
        ),
        ("claude_header_os".to_owned(), "MacOS".to_owned()),
        ("claude_header_arch".to_owned(), "arm64".to_owned()),
    ]);

    adapter.execute(provider_request).await.unwrap();

    let captured = transport.requests.lock().unwrap();
    assert_eq!(captured.len(), 1);
    let captured = &captured[0];
    assert_eq!(captured.session_id, session_id);
    assert_eq!(captured.user_agent, "claude-cli/2.2.0 (external, cli)");
    assert_eq!(
        captured.os, "MacOS",
        "client platform is pinned to baseline"
    );
    let body: serde_json::Value = serde_json::from_slice(&captured.body).unwrap();
    let encoded_identity = body["metadata"]["user_id"].as_str().unwrap();
    let identity: serde_json::Value = serde_json::from_str(encoded_identity).unwrap();
    assert_eq!(identity["device_id"], "a".repeat(64));
    assert_eq!(identity["account_uuid"], account_uuid);
    assert_eq!(identity["session_id"], session_id);
    let body_text = String::from_utf8_lossy(&captured.body);
    assert!(body_text.contains("caller-owned-system"));
    assert!(!body_text.contains("Anthropic's official CLI"));
}

#[tokio::test]
async fn provider_path_does_not_promote_user_agent_only_to_verified_cloak_bypass() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns.clone()));
    let transport = Arc::new(CapturingMessagesTransport::default());
    let mut policy = ClaudeCloakPolicy::oauth_default();
    policy.mode = "always".to_owned();
    let executor = account_executor_with_policy("account-a", transport.clone(), conductor, policy);
    let adapter = adapter(vec![("account-a", executor)], cooldowns);

    let mut provider_request = request("account-a", false);
    provider_request.payload = br#"{
        "model":"sonnet",
        "messages":[{"role":"user","content":"weak client signal"}]
    }"#
    .to_vec();
    provider_request.original_request = provider_request.payload.clone();
    provider_request.headers = [(
        "User-Agent".to_owned(),
        vec!["claude-cli/2.1.220 (external, cli)".to_owned()],
    )]
    .into_iter()
    .collect();
    provider_request.auth_metadata = BTreeMap::from([
        (
            "account_uuid".to_owned(),
            serde_json::Value::String("018f9970-00d1-7d24-bc67-9f4d595c7902".to_owned()),
        ),
        (
            CLAUDE_DEVICE_IDS_METADATA_KEY.to_owned(),
            serde_json::json!(["b".repeat(64)]),
        ),
    ]);

    adapter.execute(provider_request).await.unwrap();

    let captured = transport.requests.lock().unwrap();
    let body: serde_json::Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert!(body.get("context_management").is_some());
    assert!(String::from_utf8_lossy(&captured[0].body).contains("Anthropic's official CLI"));
}

fn reserved_exec_command_body() -> Vec<u8> {
    br#"{
        "model":"claude-sonnet-4-5",
        "max_tokens":32,
        "messages":[{"role":"user","content":"run"}],
        "tools":[{
            "name":"exec_command",
            "input_schema":{
                "type":"object",
                "properties":{"yield_time_ms":{"type":"number"}}
            }
        }]
    }"#
    .to_vec()
}

fn yield_time_ms_type(body: &[u8]) -> String {
    let value: serde_json::Value = serde_json::from_slice(body).unwrap();
    value["tools"]
        .as_array()
        .and_then(|tools| {
            tools.iter().find_map(|tool| {
                tool.pointer("/input_schema/properties/yield_time_ms/type")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn codex_client_integer_schema_reaches_claude_upstream_body() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns));
    let transport = Arc::new(CapturingMessagesTransport::default());
    let executor = account_executor("account-a", transport.clone(), None, conductor);
    let body = reserved_exec_command_body();
    let headers = BTreeMap::from([(
        "User-Agent".to_owned(),
        vec!["Codex Desktop/1.0".to_owned()],
    )]);
    let context = ClaudeExecutionRequestContext::from_provider_request(
        "account-a",
        headers,
        &body,
        &body,
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let target = ClaudeUpstreamTarget::new("https", "api.anthropic.com").unwrap();
    executor
        .execute_for_model_with_context(
            target,
            Some("claude-sonnet-4-5"),
            body,
            false,
            Some(&context),
        )
        .await
        .unwrap();
    let captured = transport.requests.lock().unwrap();
    assert_eq!(captured.len(), 1);
    assert_eq!(yield_time_ms_type(&captured[0].body), "integer");
}

#[tokio::test]
async fn non_codex_client_keeps_claude_reserved_number_schema() {
    let cooldowns = Arc::new(MemoryCooldownStore::default());
    let conductor = Arc::new(CooldownConductor::new(cooldowns));
    let transport = Arc::new(CapturingMessagesTransport::default());
    let executor = account_executor("account-a", transport.clone(), None, conductor);
    let body = reserved_exec_command_body();
    let headers = BTreeMap::from([("User-Agent".to_owned(), vec!["curl/8.0".to_owned()])]);
    let context = ClaudeExecutionRequestContext::from_provider_request(
        "account-a",
        headers,
        &body,
        &body,
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let target = ClaudeUpstreamTarget::new("https", "api.anthropic.com").unwrap();
    executor
        .execute_for_model_with_context(
            target,
            Some("claude-sonnet-4-5"),
            body,
            false,
            Some(&context),
        )
        .await
        .unwrap();
    let captured = transport.requests.lock().unwrap();
    assert_eq!(yield_time_ms_type(&captured[0].body), "number");
}
