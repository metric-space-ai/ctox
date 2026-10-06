// ref: sdk/auth/meta.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Origin: CTOX native SDK manager, persistence and cancellation guards
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::internal::auth::meta::MetaClock;
use crate::sdk::auth::{Manager, ManagerErrorKind};
use crate::sdk::cliproxy::auth::{AuthStore, AuthStoreError};
use crate::sdk::pluginapi::{
    Headers, HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk, HttpStreamResponse,
    PluginFuture,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::Duration,
};
use tokio::{sync::mpsc, time};

struct Clock;
impl MetaClock for Clock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }
}
#[derive(Default)]
struct Transport {
    replies: Mutex<VecDeque<(u16, Option<Value>)>>,
    requests: Mutex<Vec<HttpRequest>>,
    held: Mutex<Vec<mpsc::Sender<HttpStreamChunk>>>,
}
impl HostHttpClient for Transport {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("SDK credential operations use the bounded selected stream") })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            let (status_code, response) = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("selected SDK reply");
            let (sender, chunks) = mpsc::channel(1);
            if let Some(response) = response {
                sender
                    .try_send(HttpStreamChunk {
                        payload: serde_json::to_vec(&response).unwrap(),
                        error: None,
                    })
                    .unwrap();
            } else {
                self.held.lock().unwrap().push(sender);
            }
            Ok(HttpStreamResponse {
                status_code,
                headers: Headers::new(),
                chunks,
            })
        })
    }
}
#[derive(Default)]
struct Presenter {
    fail: bool,
    presentations: Mutex<Vec<(String, String, bool)>>,
}
impl MetaLoginPresenter for Presenter {
    fn present(&self, challenge: &MetaDevicePresentation) -> Result<(), PromptError> {
        self.presentations.lock().unwrap().push((
            challenge.verification_url.clone(),
            challenge.user_code.clone(),
            challenge.automatic_browser_allowed,
        ));
        if self.fail {
            Err(PromptError)
        } else {
            Ok(())
        }
    }
}
#[derive(Default)]
struct Store {
    fail: bool,
    calls: AtomicUsize,
    saved: Mutex<Vec<Auth>>,
}
impl AuthStore for Store {
    fn list(&self) -> Result<Vec<Auth>, AuthStoreError> {
        Ok(self.saved.lock().unwrap().clone())
    }
    fn save(&self, record: &Auth) -> Result<String, AuthStoreError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(AuthStoreError::Write);
        }
        self.saved.lock().unwrap().push(record.clone());
        Ok("injected-manager-store".into())
    }
    fn delete(&self, _: &str) -> Result<(), AuthStoreError> {
        panic!("SDK login must not delete another record")
    }
}
struct Fixture {
    auth: Arc<MetaAuthenticator>,
    manager: Arc<Manager>,
    transport: Arc<Transport>,
    presenter: Arc<Presenter>,
    store: Arc<Store>,
}
impl Fixture {
    fn new(replies: Vec<(u16, Option<Value>)>, presenter_fail: bool, store_fail: bool) -> Self {
        let transport = Arc::new(Transport {
            replies: Mutex::new(replies.into()),
            ..Transport::default()
        });
        let presenter = Arc::new(Presenter {
            fail: presenter_fail,
            ..Presenter::default()
        });
        let store = Arc::new(Store {
            fail: store_fail,
            ..Store::default()
        });
        let auth = Arc::new(MetaAuthenticator::new(
            Arc::new(MetaAuth::new(transport.clone()).with_clock(Arc::new(Clock))),
            presenter.clone(),
        ));
        let authenticators: [Arc<dyn Authenticator>; 1] = [auth.clone()];
        let manager = Arc::new(Manager::new(Some(store.clone()), authenticators));
        Self {
            auth,
            manager,
            transport,
            presenter,
            store,
        }
    }
}
fn replies(minted: bool) -> Vec<(u16, Option<Value>)> {
    vec![
        (
            200,
            Some(
                json!({"device_code":"test-only-device","user_code":"USER-123","interval":1,"expires_in":10,
            "verification_uri":" https://auth.meta.com/device ",
            "verification_uri_complete":" https://auth.meta.com/device?code=USER-123 "}),
            ),
        ),
        (
            200,
            Some(
                json!({"access_token":"dca:test-only-device-token","token_type":"Bearer","expires_in":3600}),
            ),
        ),
        if minted {
            (
                200,
                Some(
                    json!({"api_key":"LLM|test-only-minted-key","base_url":"https://regional.fixture.invalid/v1",
            "user_email":"fixture@example.invalid","user_full_name":"Fixture","subs_tier_name":"Pro","subs_tier_id":"tier-fixture",
            "is_subs_active":true,"has_payment_method":true}),
                ),
            )
        } else {
            (503, Some(json!({"error":"mint unavailable"})))
        },
    ]
}

#[tokio::test]
async fn candidate_meta_sdk_provider_and_precancelled_login_have_no_effect_or_refresh_expiry() {
    let fixture = Fixture::new(Vec::new(), false, false);
    assert_eq!(fixture.auth.provider(), "meta");
    assert!(fixture.auth.refresh_lead().is_none());
    let cancellation = LoginCancellation::default();
    cancellation.cancel();
    let result = fixture
        .manager
        .login(
            &cancellation,
            "meta",
            &LoginConfig::default(),
            &LoginOptions::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        result.authentication.unwrap().kind,
        AuthenticatorErrorKind::Cancelled
    );
    assert!(fixture.transport.requests.lock().unwrap().is_empty());
    assert!(fixture.presenter.presentations.lock().unwrap().is_empty());
    assert_eq!(fixture.store.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_sdk_manager_persists_one_minted_account_with_current_subscription_fields() {
    let fixture = Fixture::new(replies(true), false, false);
    let (record, path) = fixture
        .manager
        .login(
            &LoginCancellation::default(),
            "meta",
            &LoginConfig::default(),
            &LoginOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(path, "injected-manager-store");
    assert_eq!(record.provider, "meta");
    assert_eq!(record.label, "fixture@example.invalid");
    assert_eq!(
        record.id,
        credential_file_name("fixture@example.invalid", "dca:test-only-device-token")
    );
    assert_eq!(record.file_name, record.id);
    assert_eq!(record.metadata["access_token"], "LLM|test-only-minted-key");
    assert_eq!(record.metadata["expired"], "");
    assert_eq!(record.metadata["dca_expires_at"], 1_700_003_600i64);
    assert_eq!(record.metadata["subs_tier_id"], "tier-fixture");
    assert_eq!(record.metadata["is_subs_active"], true);
    assert_eq!(record.metadata["has_payment_method"], true);
    assert_eq!(record.attributes["api_key"], "LLM|test-only-minted-key");
    assert_eq!(
        record.attributes["base_url"],
        "https://regional.fixture.invalid/v1"
    );
    assert!(
        record.storage.is_none(),
        "only the injected Manager store persists metadata"
    );
    assert_eq!(fixture.store.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.store.saved.lock().unwrap()[0].id, record.id);
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_sdk_headless_presentation_prefers_complete_uri_and_keeps_browser_policy() {
    for complete in [true, false] {
        let mut values = replies(true);
        if !complete {
            values[0].1.as_mut().unwrap()["verification_uri_complete"] = json!(" ");
        }
        let fixture = Fixture::new(values, false, false);
        let options = LoginOptions {
            no_browser: true,
            ..LoginOptions::default()
        };
        fixture
            .manager
            .login(
                &LoginCancellation::default(),
                "meta",
                &LoginConfig::default(),
                &options,
            )
            .await
            .unwrap();
        let presentations = fixture.presenter.presentations.lock().unwrap();
        assert_eq!(presentations.len(), 1);
        assert_eq!(
            presentations[0].0,
            if complete {
                "https://auth.meta.com/device?code=USER-123"
            } else {
                "https://auth.meta.com/device"
            }
        );
        assert_eq!(presentations[0].1, "USER-123");
        assert!(!presentations[0].2);
    }
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_sdk_dca_only_account_remains_recoverable_without_minted_key_expiry() {
    let fixture = Fixture::new(replies(false), false, false);
    let (record, _) = fixture
        .manager
        .login(
            &LoginCancellation::default(),
            "meta",
            &LoginConfig::default(),
            &LoginOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(record.label, "Meta");
    assert_eq!(
        record.metadata["access_token"],
        "dca:test-only-device-token"
    );
    assert_eq!(record.metadata["expired"], "2023-11-14T23:13:20Z");
    assert_eq!(record.attributes["dca_token"], "dca:test-only-device-token");
    assert!(!record.attributes.contains_key("api_key"));
    assert!(!record.metadata.contains_key("api_key"));
    assert_eq!(fixture.store.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_sdk_cancellation_closes_device_and_poll_responses_without_persistence() {
    for at_poll in [false, true] {
        let values = if at_poll {
            let mut values = replies(true);
            values.truncate(1);
            values.push((200, None));
            values
        } else {
            vec![(200, None)]
        };
        let fixture = Fixture::new(values, false, false);
        let manager = fixture.manager.clone();
        let cancellation = LoginCancellation::default();
        let owned = cancellation.clone();
        let task = tokio::spawn(async move {
            manager
                .login(
                    &owned,
                    "meta",
                    &LoginConfig::default(),
                    &LoginOptions::default(),
                )
                .await
        });
        tokio::task::yield_now().await;
        if at_poll {
            time::advance(Duration::from_secs(1)).await;
            tokio::task::yield_now().await;
        }
        assert_eq!(fixture.transport.held.lock().unwrap().len(), 1);
        cancellation.cancel();
        let error = task.await.unwrap().unwrap_err();
        assert_eq!(
            error.authentication.unwrap().kind,
            AuthenticatorErrorKind::Cancelled
        );
        assert!(fixture.transport.held.lock().unwrap()[0].is_closed());
        assert_eq!(fixture.store.calls.load(Ordering::SeqCst), 0);
    }
}
#[tokio::test]
async fn candidate_meta_sdk_missing_uri_or_presenter_failure_does_not_poll_or_save() {
    for presenter_fail in [false, true] {
        let mut values = replies(true);
        if !presenter_fail {
            let value = values[0].1.as_mut().unwrap();
            value["verification_uri"] = json!("");
            value["verification_uri_complete"] = json!("");
        }
        let fixture = Fixture::new(values, presenter_fail, false);
        let error = fixture
            .manager
            .login(
                &LoginCancellation::default(),
                "meta",
                &LoginConfig::default(),
                &LoginOptions::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind, ManagerErrorKind::Authentication);
        assert_eq!(fixture.transport.requests.lock().unwrap().len(), 1);
        assert_eq!(fixture.store.calls.load(Ordering::SeqCst), 0);
    }
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_sdk_injected_store_failure_is_reported_without_a_fallback_writer() {
    let fixture = Fixture::new(replies(true), false, true);
    let error = fixture
        .manager
        .login(
            &LoginCancellation::default(),
            "meta",
            &LoginConfig::default(),
            &LoginOptions::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, ManagerErrorKind::Store);
    assert_eq!(fixture.store.calls.load(Ordering::SeqCst), 1);
    assert!(fixture.store.saved.lock().unwrap().is_empty());
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_sdk_without_injected_store_returns_owned_record_and_no_persistence_path() {
    let fixture = Fixture::new(replies(true), false, false);
    let authenticators: [Arc<dyn Authenticator>; 1] = [fixture.auth.clone()];
    let manager = Manager::new(None, authenticators);
    let (record, path) = manager
        .login(
            &LoginCancellation::default(),
            "meta",
            &LoginConfig::default(),
            &LoginOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(record.provider, "meta");
    assert!(path.is_empty());
    assert_eq!(fixture.store.calls.load(Ordering::SeqCst), 0);
}
