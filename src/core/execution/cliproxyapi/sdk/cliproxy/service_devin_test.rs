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
