// Origin: CTOX selected Meta mint/preparation integration guards.
// ref: internal/runtime/executor/meta_executor_test.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::sdk::pluginapi::{
    Headers, HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk, HttpStreamResponse,
    PluginFuture,
};
use chrono::{DateTime, Utc};
use serde_json::json;
use std::sync::Mutex;
use tokio::sync::{mpsc, Semaphore};

#[derive(Default)]
struct ScheduledStore(Mutex<BTreeMap<String, Auth>>);
impl crate::sdk::cliproxy::auth::AuthStore for ScheduledStore {
    fn list(&self) -> Result<Vec<Auth>, crate::sdk::cliproxy::auth::AuthStoreError> {
        Ok(self.0.lock().unwrap().values().cloned().collect())
    }
    fn save(&self, account: &Auth) -> Result<String, crate::sdk::cliproxy::auth::AuthStoreError> {
        self.0
            .lock()
            .unwrap()
            .insert(account.id.clone(), account.clone());
        Ok(account.id.clone())
    }
    fn delete(&self, id: &str) -> Result<(), crate::sdk::cliproxy::auth::AuthStoreError> {
        self.0.lock().unwrap().remove(id);
        Ok(())
    }
}
struct ScheduledClock;
impl crate::sdk::cliproxy::auth::AutoRefreshClock for ScheduledClock {
    fn now(&self) -> DateTime<Utc> {
        Clock.now()
    }
}
struct ScheduledSink;
impl crate::sdk::cliproxy::auth::ModelResumeSink for ScheduledSink {
    fn resume_model(&self, _: &str, _: &str) {}
}
fn scheduled_worker(
    client: Arc<Transport>,
) -> (
    crate::sdk::cliproxy::auth::AutoRefreshWorker,
    Arc<crate::sdk::cliproxy::auth::AuthLifecycle>,
    Arc<ScheduledStore>,
    Arc<crate::sdk::cliproxy::auth::RefreshSchedule>,
    u64,
) {
    use crate::internal::runtime::executor::meta_executor_scheduled::MetaScheduledRefresher;
    use crate::sdk::cliproxy::auth::{
        AuthLifecycle, AuthMutationOptions, AutoRefreshConfig, AutoRefreshWorker,
        ProviderExecutorRegistration, ProviderExecutorRegistry, RefreshSchedule,
    };
    let store = Arc::new(ScheduledStore::default());
    let schedule = Arc::new(RefreshSchedule::default());
    let lifecycle = Arc::new(AuthLifecycle::new(
        store.clone(),
        schedule.clone(),
        std::time::Duration::from_secs(5),
    ));
    let registry = Arc::new(ProviderExecutorRegistry::default());
    registry.register(Arc::new(
        ProviderExecutorRegistration::new(
            "meta",
            Arc::new(MetaScheduledRefresher::new(
                Arc::new(capability(client)),
                tokio::runtime::Handle::current(),
            )),
        )
        .unwrap(),
    ));
    let mut account = auth();
    account
        .metadata
        .insert("refresh_interval_seconds".into(), json!(60));
    let registered = lifecycle
        .register(account, AuthMutationOptions::default(), Clock.now())
        .unwrap();
    let worker = AutoRefreshWorker::spawn(
        lifecycle.clone(),
        schedule.clone(),
        registry,
        Arc::new(ScheduledSink),
        Arc::new(ScheduledClock),
        AutoRefreshConfig {
            interval: std::time::Duration::from_secs(5),
            concurrency: 1,
            job_buffer: 1,
        },
    );
    (
        worker,
        lifecycle,
        store,
        schedule,
        registered.registration_epoch,
    )
}
async fn await_scheduled(predicate: impl Fn() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !predicate() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("owned scheduled worker must finish the observed transition");
}

struct Clock;

impl MetaClock for Clock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }
}
struct Transport {
    status: u16,
    body: Vec<u8>,
    hold: bool,
    requests: Mutex<Vec<HttpRequest>>,
    sender: Mutex<Option<mpsc::Sender<HttpStreamChunk>>>,
    entered: Semaphore,
}
impl Transport {
    fn new(status: u16, body: Value) -> Arc<Self> {
        Arc::new(Self {
            status,
            body: serde_json::to_vec(&body).unwrap(),
            hold: false,
            requests: Mutex::new(Vec::new()),
            sender: Mutex::new(None),
            entered: Semaphore::new(0),
        })
    }
    fn held() -> Arc<Self> {
        Arc::new(Self {
            status: 200,
            body: Vec::new(),
            hold: true,
            requests: Mutex::new(Vec::new()),
            sender: Mutex::new(None),
            entered: Semaphore::new(0),
        })
    }
}
impl HostHttpClient for Transport {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("mint must own the bounded selected stream") })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            let (sender, chunks) = mpsc::channel(1);
            if self.hold {
                *self.sender.lock().unwrap() = Some(sender);
            } else {
                sender
                    .try_send(HttpStreamChunk {
                        payload: self.body.clone(),
                        error: None,
                    })
                    .unwrap();
            }
            self.entered.add_permits(1);
            Ok(HttpStreamResponse {
                status_code: self.status,
                headers: Headers::new(),
                chunks,
            })
        })
    }
}
fn capability(transport: Arc<Transport>) -> MetaRequestAuthPreparer {
    MetaRequestAuthPreparer::new(Arc::new(
        MetaAuth::new(transport)
            .with_clock(Arc::new(Clock))
            .with_mint_endpoint("https://fixture.invalid/key"),
    ))
    .with_clock(Arc::new(Clock))
}
fn auth() -> Auth {
    let mut auth = Auth::default();
    auth.id = "fixture-meta".into();
    auth.provider = "meta".into();
    auth.registration_epoch = 7;
    auth.attributes.insert("auth_kind".into(), "oauth".into());
    auth.metadata
        .insert("access_token".into(), json!("dca:test-only-device"));
    auth.metadata
        .insert("dca_expired".into(), json!("2023-11-14T23:13:20Z"));
    auth.metadata
        .insert("dca_expires_at".into(), json!(1_700_003_600_i64));
    auth.metadata
        .insert("expired".into(), json!("2023-11-14T23:13:20Z"));
    auth
}
fn minted() -> Value {
    json!({"api_key":"LLM|test-only-minted","base_url":" https://regional.fixture.invalid/v1 ",
        "user_email":"fixture@example.invalid","user_full_name":"Fixture",
        "subs_tier_name":"Muse Pro","subs_tier_id":"pro","is_subs_active":true,"has_payment_method":true})
}

fn spawn_mint(
    preparer: Arc<MetaRequestAuthPreparer>,
    record: Auth,
) -> tokio::task::JoinHandle<Result<Auth, AuthPreparationError>> {
    tokio::spawn(async move { preparer.refresh_candidate(&record).await })
}
async fn wait_for_shared_mint(coordinator: &MetaMintCoordinator) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let joined = coordinator
                .flights
                .lock()
                .unwrap()
                .values()
                .any(|flight| flight.strong_count() >= 2);
            if joined {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn finish_held_mint(client: &Transport) {
    let sender = client.sender.lock().unwrap().take().unwrap();
    sender
        .try_send(HttpStreamChunk {
            payload: serde_json::to_vec(&minted()).unwrap(),
            error: None,
        })
        .unwrap();
}
