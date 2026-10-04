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

#[test]
fn candidate_meta_request_auth_credential_precedence_never_uses_dca_as_api_key() {
    let mut record = auth();
    assert!(meta_credentials(&record).api_key().is_empty());
    record
        .attributes
        .insert("api_key".into(), "dca:wrong-field".into());
    record
        .attributes
        .insert("access_token".into(), " attr-access ".into());
    record
        .metadata
        .insert("api_key".into(), json!("metadata-key"));
    assert_eq!(meta_credentials(&record).api_key(), "attr-access");
    record
        .attributes
        .insert("api_key".into(), " attr-key ".into());
    assert_eq!(meta_credentials(&record).api_key(), "attr-key");
    record.attributes.remove("api_key");
    record.attributes.remove("access_token");
    assert_eq!(meta_credentials(&record).api_key(), "metadata-key");
    record
        .metadata
        .insert("api_key".into(), json!("dca:also-not-key"));
    record
        .metadata
        .insert("access_token".into(), json!(" metadata-access "));
    assert_eq!(meta_credentials(&record).api_key(), "metadata-access");
}
#[test]
fn candidate_meta_request_auth_base_url_matches_default_metadata_fallback() {
    let mut record = auth();
    record.metadata.insert(
        "api_base_url".into(),
        json!(" https://metadata.fixture.invalid/v1 "),
    );
    assert_eq!(
        meta_credentials(&record).base_url(),
        "https://metadata.fixture.invalid/v1"
    );
    record
        .attributes
        .insert("base_url".into(), DEFAULT_API_BASE_URL.into());
    assert_eq!(
        meta_credentials(&record).base_url(),
        "https://metadata.fixture.invalid/v1"
    );
    record.attributes.insert(
        "base_url".into(),
        " https://attribute.fixture.invalid/v1 ".into(),
    );
    assert_eq!(
        meta_credentials(&record).base_url(),
        "https://attribute.fixture.invalid/v1"
    );
}
#[test]
fn candidate_meta_request_auth_dca_precedence_and_config_key_exclusion() {
    let mut record = auth();
    assert_eq!(meta_dca_token(&record).as_str(), "dca:test-only-device");
    record
        .metadata
        .insert("dca_token".into(), json!("dca:metadata"));
    record
        .attributes
        .insert("access_token".into(), "dca:attribute-access".into());
    assert_eq!(meta_dca_token(&record).as_str(), "dca:attribute-access");
    record
        .attributes
        .insert("dca_token".into(), " dca:attribute ".into());
    assert_eq!(meta_dca_token(&record).as_str(), "dca:attribute");
    record
        .attributes
        .insert("auth_kind".into(), "apikey".into());
    record
        .attributes
        .insert("source".into(), "config:meta-api-key".into());
    assert!(meta_dca_token(&record).is_empty());
}
#[tokio::test]
async fn candidate_meta_request_auth_prepares_owned_selected_mint_snapshot() {
    let client = Transport::new(200, minted());
    let preparer = capability(client.clone());
    let mut record = auth();
    record.label = "User label".into();
    record
        .metadata
        .insert("notes".into(), json!("user setting"));
    assert!(preparer.should_prepare(&record));
    preparer.prepare(&mut record).await.unwrap();
    assert!(!preparer.should_prepare(&record));
    assert_eq!(record.registration_epoch, 7);
    assert_eq!(record.label, "User label");
    assert_eq!(record.metadata["notes"], "user setting");
    assert_eq!(record.metadata["api_key"], "LLM|test-only-minted");
    assert_eq!(record.metadata["access_token"], "LLM|test-only-minted");
    assert_eq!(record.attributes["api_key"], "LLM|test-only-minted");
    assert_eq!(
        record.attributes["base_url"],
        "https://regional.fixture.invalid/v1"
    );
    assert!(!record.metadata.contains_key("expired"));
    assert_eq!(record.metadata["dca_expires_at"], 1_700_003_600_i64);
    assert_eq!(record.metadata["last_refresh"], "2023-11-14T22:13:20Z");
    assert_eq!(record.metadata["subs_tier_name"], "Muse Pro");
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url, "https://fixture.invalid/key");
    assert_eq!(
        requests[0].headers["Authorization"],
        ["Bearer dca:test-only-device"]
    );
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["dca_token"], "dca:test-only-device");
}
#[tokio::test]
async fn candidate_meta_request_auth_valid_api_key_skips_preparation_http() {
    let client = Transport::new(200, minted());
    let preparer = capability(client.clone());
    let mut record = auth();
    record
        .metadata
        .insert("api_key".into(), json!("LLM|existing"));
    let before = serde_json::to_value(&record).unwrap();
    assert!(!preparer.should_prepare(&record));
    preparer.prepare(&mut record).await.unwrap();
    assert_eq!(serde_json::to_value(&record).unwrap(), before);
    assert!(client.requests.lock().unwrap().is_empty());
}
#[tokio::test]
async fn candidate_meta_request_auth_401_refresh_mints_even_with_existing_api_key() {
    let client = Transport::new(200, minted());
    let preparer = capability(client.clone());
    let mut record = auth();
    record
        .metadata
        .insert("api_key".into(), json!("LLM|failed-key"));
    record
        .metadata
        .insert("dca_token".into(), json!("dca:retained"));
    let candidate = AsyncAuthRefresher::refresh(&preparer, &record)
        .await
        .unwrap();
    assert_eq!(record.metadata["api_key"], "LLM|failed-key");
    assert_eq!(candidate.metadata["api_key"], "LLM|test-only-minted");
    assert_eq!(candidate.registration_epoch, record.registration_epoch);
    assert_eq!(client.requests.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn candidate_meta_request_auth_config_dca_never_mints_or_becomes_inference_key() {
    let client = Transport::new(200, minted());
    let preparer = capability(client.clone());
    let mut record = auth();
    record
        .attributes
        .insert("auth_kind".into(), "apikey".into());
    record
        .attributes
        .insert("source".into(), "config:meta-api-key".into());
    assert!(!preparer.should_prepare(&record));
    let error = preparer.refresh_candidate(&record).await.unwrap_err();
    assert_eq!(error.downcast_ref::<AuthError>().unwrap().http_status, 401);
    assert!(client.requests.lock().unwrap().is_empty());
}
#[tokio::test]
async fn candidate_meta_request_auth_api_only_refresh_retains_snapshot_without_http() {
    let client = Transport::new(200, minted());
    let preparer = capability(client.clone());
    let mut record = auth();
    record.metadata.clear();
    record
        .attributes
        .insert("api_key".into(), "LLM|static".into());
    let candidate = preparer.refresh_candidate(&record).await.unwrap();
    assert_eq!(
        serde_json::to_value(&candidate).unwrap(),
        serde_json::to_value(&record).unwrap()
    );
    assert!(client.requests.lock().unwrap().is_empty());
}
#[tokio::test]
async fn candidate_meta_request_auth_mint_failure_cannot_partially_update_auth() {
    for status in [401, 403, 429, 500] {
        let client = Transport::new(status, json!({"error":"private-provider-body"}));
        let preparer = capability(client);
        let mut record = auth();
        let before = serde_json::to_value(&record).unwrap();
        let error = preparer.prepare(&mut record).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<AuthError>().unwrap().http_status,
            status
        );
        assert_eq!(serde_json::to_value(&record).unwrap(), before);
        assert!(!error.to_string().contains("private-provider-body"));
        assert!(!error.to_string().contains("dca:test-only-device"));
    }
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_request_auth_mint_deadline_drops_receiver_and_retains_record() {
    let client = Transport::held();
    let preparer = capability(client.clone());
    let mut record = auth();
    let before = serde_json::to_value(&record).unwrap();
    let error = preparer.prepare(&mut record).await.unwrap_err();
    assert_eq!(error.downcast_ref::<AuthError>().unwrap().http_status, 408);
    assert_eq!(serde_json::to_value(&record).unwrap(), before);
    assert!(client.sender.lock().unwrap().as_ref().unwrap().is_closed());
}
#[tokio::test]
async fn candidate_meta_request_auth_cancelled_mint_drops_receiver_without_mutation() {
    let client = Transport::held();
    let preparer = capability(client.clone());
    let mut record = auth();
    let before = serde_json::to_value(&record).unwrap();
    let mut operation = Box::pin(preparer.prepare(&mut record));
    tokio::select! {
        result = &mut operation => panic!("held mint completed: {result:?}"),
        permit = client.entered.acquire() => permit.unwrap().forget(),
    }
    drop(operation);
    assert_eq!(serde_json::to_value(&record).unwrap(), before);
    assert!(client.sender.lock().unwrap().as_ref().unwrap().is_closed());
}
#[tokio::test]
async fn candidate_meta_request_auth_refresh_clears_absent_tier_but_keeps_identity_and_dca() {
    let client = Transport::new(200, json!({"api_key":"LLM|new"}));
    let preparer = capability(client);
    let mut record = auth();
    record
        .metadata
        .insert("email".into(), json!("existing@example.invalid"));
    record
        .metadata
        .insert("subs_tier_name".into(), json!("old"));
    record
        .metadata
        .insert("subs_tier_id".into(), json!("old-id"));
    let candidate = preparer.refresh_candidate(&record).await.unwrap();
    assert_eq!(candidate.metadata["email"], "existing@example.invalid");
    assert!(!candidate.metadata.contains_key("subs_tier_name"));

    assert!(!candidate.metadata.contains_key("subs_tier_id"));
    assert_eq!(candidate.metadata["dca_token"], "dca:test-only-device");
    assert_eq!(candidate.metadata["dca_expired"], "2023-11-14T23:13:20Z");
}
#[tokio::test]
async fn candidate_meta_request_auth_wrong_provider_fails_before_http_and_debug_redacts() {
    let client = Transport::new(200, minted());
    let preparer = capability(client.clone());
    let mut record = auth();
    record.provider = "other-provider".into();
    assert!(!preparer.should_prepare(&record));
    assert!(preparer.refresh_candidate(&record).await.is_err());
    assert!(client.requests.lock().unwrap().is_empty());
    let debug = format!("{:?} {:?}", preparer, meta_credentials(&record));
    assert!(!debug.contains("fixture.invalid"));
    assert!(!debug.contains("test-only"));
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
#[tokio::test]
async fn candidate_meta_request_auth_shared_dca_mints_once_for_concurrent_accounts() {
    let client = Transport::held();
    let coordinator = Arc::new(MetaMintCoordinator::default());
    let first = Arc::new(capability(client.clone()).with_mint_coordinator(coordinator.clone()));
    let second = Arc::new(capability(client.clone()).with_mint_coordinator(coordinator.clone()));
    let record = auth();
    let left = spawn_mint(first, record.clone());
    let mut other = record.clone();
    other.id = "second-fixture-meta".into();
    let right = spawn_mint(second, other);
    wait_for_shared_mint(&coordinator).await;
    assert_eq!(client.requests.lock().unwrap().len(), 1);
    finish_held_mint(&client);
    let left = left.await.unwrap().unwrap();
    let right = right.await.unwrap().unwrap();
    assert_eq!(left.id, "fixture-meta");
    assert_eq!(right.id, "second-fixture-meta");
    assert_eq!(left.metadata["api_key"], right.metadata["api_key"]);
    assert!(coordinator.flights.lock().unwrap().is_empty());
}
#[tokio::test]
async fn candidate_meta_request_auth_cancelled_waiter_does_not_cancel_remaining_owner() {
    let client = Transport::held();
    let coordinator = Arc::new(MetaMintCoordinator::default());
    let preparer = Arc::new(capability(client.clone()).with_mint_coordinator(coordinator.clone()));
    let first = spawn_mint(preparer.clone(), auth());
    let second = spawn_mint(preparer, auth());
    wait_for_shared_mint(&coordinator).await;
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert!(!client.sender.lock().unwrap().as_ref().unwrap().is_closed());
    finish_held_mint(&client);
    assert_eq!(
        second.await.unwrap().unwrap().metadata["api_key"],
        "LLM|test-only-minted"
    );
    assert_eq!(client.requests.lock().unwrap().len(), 1);
    assert!(coordinator.flights.lock().unwrap().is_empty());
}
#[tokio::test]
async fn candidate_meta_request_auth_later_401_is_not_served_by_completed_mint_cache() {
    let client = Transport::new(200, minted());
    let preparer = capability(client.clone());
    let first = preparer.refresh_candidate(&auth()).await.unwrap();
    let second = preparer.refresh_candidate(&first).await.unwrap();
    assert_eq!(second.metadata["api_key"], "LLM|test-only-minted");
    assert_eq!(client.requests.lock().unwrap().len(), 2);
    assert!(preparer.mints.flights.lock().unwrap().is_empty());
}
