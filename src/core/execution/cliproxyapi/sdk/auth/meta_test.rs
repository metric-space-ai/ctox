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
