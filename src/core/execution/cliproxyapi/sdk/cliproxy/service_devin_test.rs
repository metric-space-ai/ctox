// Origin: CTOX native Devin factory capability guards
// License: AGPL-3.0-only

use super::super::auth::{AuthPreparationError, ProviderExecutorRegistry, RefreshExecutorError};
use super::super::service_test_support::{auth, RecordingExecutorFactory};
use super::*;
use crate::internal::registry::{DevinModelsStore, StaticModelsCatalog};
use crate::internal::runtime::executor::devin_executor::{
    DevinAttemptContext, DevinAttemptContextProvider,
};
use crate::internal::runtime::executor::helps::devin_request::DevinSessionTurns;
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorRequest, Headers, HostHttpClient, HttpRequest, HttpResponse,
    HttpStreamResponse, PluginExecutionError, PluginFuture, ProviderExecutor,
};
use crate::sdk::translator::Registry;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Mutex};

struct OwnedContext;
impl DevinAttemptContextProvider for OwnedContext {
    fn for_request(
        &self,
        _: &ExecutorRequest,
    ) -> Result<DevinAttemptContext, PluginExecutionError> {
        panic!("unary HTTP capability must not fabricate an inference session")
    }
}
struct TestRefresher;
impl AuthRefresher for TestRefresher {
    fn refresh(&self, _: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        Ok(None)
    }
}
struct TestPreparer;
impl AuthPreparer for TestPreparer {
    fn prepare<'a>(
        &'a self,
        auth: &'a mut Auth,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuthPreparationError>> + Send + 'a>> {
        Box::pin(async move {
            auth.attributes
                .insert("prepared-by-owner".into(), "true".into());
            Ok(())
        })
    }
}
#[derive(Default)]
struct TestCloser(Mutex<Vec<String>>);
impl ExecutionSessionCloser for TestCloser {
    fn close_execution_session(&self, session_id: &str) {
        self.0.lock().unwrap().push(session_id.into());
    }
}
struct Fixture {
    factory: DevinExecutorFactory,
    execution: Arc<dyn ProviderExecutor>,
    refresher: Arc<dyn AuthRefresher>,
    preparer: Arc<dyn AuthPreparer>,
    closer: Arc<TestCloser>,
    fallback: Arc<RecordingExecutorFactory>,
}
impl Fixture {
    fn new() -> Self {
        let execution = Arc::new(DevinExecutor::new(
            Arc::new(Registry::new()),
            Arc::new(DevinModelsStore::default()),
            Arc::new(StaticModelsCatalog::default()),
            Arc::new(DevinSessionTurns::default()),
            Arc::new(OwnedContext),
        ));
        let refresher: Arc<dyn AuthRefresher> = Arc::new(TestRefresher);
        let preparer: Arc<dyn AuthPreparer> = Arc::new(TestPreparer);
        let closer = Arc::new(TestCloser::default());
        let fallback = Arc::new(RecordingExecutorFactory::default());
        let factory = DevinExecutorFactory::new(
            fallback.clone(),
            execution.clone(),
            refresher.clone(),
            preparer.clone(),
        )
        .with_session_closer(closer.clone());
        Self {
            factory,
            execution,
            refresher,
            preparer,
            closer,
            fallback,
        }
    }
}
#[derive(Default)]
struct SelectedTransport(Mutex<Vec<HttpRequest>>);
impl HostHttpClient for SelectedTransport {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.0.lock().unwrap().push(request);
            Ok(HttpResponse {
                status_code: 200,
                headers: Headers::from([("X-Owned-Transport".into(), vec!["selected".into()])]),
                body: b"selected-native-response".to_vec(),
            })
        })
    }
    fn execute_stream<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async { panic!("unary capability cannot invoke a stream transport") })
    }
}

#[tokio::test]
async fn candidate_devin_factory_preserves_owned_capabilities_and_idempotent_sessions() {
    let fixture = Fixture::new();
    let registry = ProviderExecutorRegistry::default();
    let mut account = auth("native-devin-account", " Devin ");
    let first = fixture
        .factory
        .registration_for(" DEVIN ", &account)
        .unwrap();
    assert_eq!(first.provider(), "devin");
    assert!(Arc::ptr_eq(&first.execution().unwrap(), &fixture.execution));
    assert!(Arc::ptr_eq(&first.refresher(), &fixture.refresher));
    assert!(Arc::ptr_eq(
        &first.auth_preparer().unwrap(),
        &fixture.preparer
    ));
    let closer: Arc<dyn ExecutionSessionCloser> = fixture.closer.clone();
    assert!(Arc::ptr_eq(&first.session_closer().unwrap(), &closer));
    first
        .auth_preparer()
        .unwrap()
        .prepare(&mut account)
        .await
        .unwrap();
    assert_eq!(
        account
            .attributes
            .get("prepared-by-owner")
            .map(String::as_str),
        Some("true")
    );
    assert!(!registry.register(first));
    assert!(!registry.register(fixture.factory.registration_for("devin", &account).unwrap()));
    assert!(
        fixture.closer.0.lock().unwrap().is_empty(),
        "same capabilities retain existing sessions"
    );
    assert!(registry.close_all_sessions("DEVIN"));
    assert_eq!(fixture.closer.0.lock().unwrap().len(), 1);
    assert!(fixture.fallback.calls().is_empty());
}

#[tokio::test]
async fn candidate_devin_factory_registry_dispatch_uses_selected_http_transport_only() {
    let fixture = Fixture::new();
    let registry = ProviderExecutorRegistry::default();
    let account = auth("native-devin-account", "devin");
    registry.register(fixture.factory.registration_for("devin", &account).unwrap());
    let transport = Arc::new(SelectedTransport::default());
    let request = ExecutorHttpRequest {
        auth_id: account.id.clone(),
        auth_provider: "devin".into(),
        method: "POST".into(),
        url: "https://fixture.invalid/test/GetUserStatus".into(),
        body: b"owned-unary-body".to_vec(),
        attributes: BTreeMap::from([("session_token".into(), "test-only-selected-token".into())]),
        http_client: Some(transport.clone()),
        ..ExecutorHttpRequest::default()
    };
    let response = registry.http_request("DEVIN", request).await.unwrap();
    assert_eq!(response.status_code, 200);
    assert_eq!(response.body, b"selected-native-response");
    assert_eq!(
        response.headers.get("X-Owned-Transport").unwrap(),
        &["selected"]
    );
    let requests = transport.0.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(
        requests[0].url,
        "https://fixture.invalid/test/GetUserStatus"
    );
    assert_eq!(requests[0].body, b"owned-unary-body");
    assert_eq!(
        requests[0].headers.get("Authorization").unwrap(),
        &["Basic test-only-selected-token-test-only-selected-token"]
    );
    drop(requests);
    assert!(registry
        .http_request("devin", ExecutorHttpRequest::default())
        .await
        .is_err());
    assert_eq!(
        transport.0.lock().unwrap().len(),
        1,
        "missing transport cannot silently use another client"
    );
}

#[test]
fn candidate_devin_factory_keeps_other_and_compatibility_providers_on_original_factory() {
    let fixture = Fixture::new();
    let mut compatibility = auth("compatible-devin-account", "openai-compatibility");
    compatibility.label = "devin".into();
    compatibility
        .attributes
        .insert("compat_name".into(), "devin".into());
    compatibility
        .attributes
        .insert("provider_key".into(), "devin".into());
    for (key, account) in [
        ("codex", auth("native-codex-account", "codex")),
        ("openai-compatible-devin", compatibility),
        ("custom-sdk", auth("custom-account", "custom-sdk")),
    ] {
        let registration = fixture.factory.registration_for(key, &account).unwrap();
        assert_eq!(registration.provider(), key);
        assert!(
            registration.execution().is_none(),
            "native Devin must not capture {key}"
        );
    }
    assert_eq!(
        fixture.fallback.calls(),
        ["codex", "openai-compatible-devin", "custom-sdk"]
    );
}

#[test]
fn candidate_devin_factory_rejects_mismatched_compatibility_and_disabled_native_accounts() {
    let fixture = Fixture::new();
    let mismatched = auth("wrong-native-account", "codex");
    assert!(matches!(
        fixture.factory.registration_for("devin", &mismatched),
        Err(ExecutorFactoryError::InvalidRegistration)
    ));
    let mut compatibility = auth("compatible-native-account", "devin");
    compatibility
        .attributes
        .insert("compat_name".into(), "devin".into());
    assert!(matches!(
        fixture.factory.registration_for("devin", &compatibility),
        Err(ExecutorFactoryError::InvalidRegistration)
    ));
    let mut disabled = auth("disabled-native-account", "devin");
    disabled.disabled = true;
    assert!(matches!(
        fixture.factory.registration_for("devin", &disabled),
        Err(ExecutorFactoryError::Unsupported)
    ));
    assert!(
        fixture.fallback.calls().is_empty(),
        "invalid native bindings must not escape to another factory"
    );
}
