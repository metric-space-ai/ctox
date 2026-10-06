// ref: internal/auth/meta/meta_auth_test.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Origin: CTOX selected-transport, lifecycle, clock and snapshot regressions
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::sdk::pluginapi::{
    Headers, HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk, HttpStreamResponse,
    PluginFuture,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::mpsc,
    time::{self, Instant},
};

struct FixedClock;
impl MetaClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }
}
struct Reply {
    status: u16,
    body: Vec<u8>,
    hold: bool,
    chunk_error: Option<String>,
}
impl Reply {
    fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            body: serde_json::to_vec(&body).unwrap(),
            hold: false,
            chunk_error: None,
        }
    }
    fn hold() -> Self {
        Self {
            status: 200,
            body: Vec::new(),
            hold: true,
            chunk_error: None,
        }
    }
}
#[derive(Default)]
struct Transport {
    replies: Mutex<VecDeque<Result<Reply, String>>>,
    requests: Mutex<Vec<(HttpRequest, Instant)>>,
    held: Mutex<Vec<mpsc::Sender<HttpStreamChunk>>>,
}
impl Transport {
    fn new(replies: Vec<Reply>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into_iter().map(Ok).collect()),
            ..Self::default()
        })
    }
    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}
impl HostHttpClient for Transport {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("Meta credential HTTP must retain the bounded stream receiver") })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests
                .lock()
                .unwrap()
                .push((request, Instant::now()));
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("one selected test response");
            let reply = match reply {
                Ok(reply) => reply,
                Err(message) => return Err(Arc::new(std::io::Error::other(message)) as _),
            };
            let (sender, chunks) = mpsc::channel(1);
            if !reply.body.is_empty() || reply.chunk_error.is_some() {
                sender
                    .try_send(HttpStreamChunk {
                        payload: reply.body,
                        error: reply
                            .chunk_error
                            .map(|message| Arc::new(std::io::Error::other(message)) as _),
                    })
                    .unwrap();
            }
            if reply.hold {
                self.held.lock().unwrap().push(sender);
            }
            Ok(HttpStreamResponse {
                status_code: reply.status,
                headers: Headers::new(),
                chunks,
            })
        })
    }
}
fn service(transport: Arc<Transport>) -> MetaAuth {
    MetaAuth::new(transport)
        .with_clock(Arc::new(FixedClock))
        .with_mint_endpoint("https://fixture.invalid/key")
}
fn device() -> DeviceCodeResponse {
    DeviceCodeResponse {
        device_code: "test-only-device-code".into(),
        user_code: "TEST-1234".into(),
        token_endpoint: "https://fixture.invalid/token".into(),
        interval: 1,
        expires_in: 30,
        ..DeviceCodeResponse::default()
    }
}
fn token_reply() -> Reply {
    Reply::json(
        200,
        json!({"access_token":"dca:test-only-device-token","token_type":"Bearer","expires_in":3600}),
    )
}
fn mint_reply() -> Reply {
    Reply::json(
        200,
        json!({"api_key":"LLM|test-only-minted-key","base_url":" https://regional.fixture.invalid/v1 ",
        "user_email":"fixture@example.invalid","user_full_name":"Test Fixture",
        "subs_tier_name":"Muse Pro","subs_tier_id":"tier-fixture","is_subs_active":true,"has_payment_method":true}),
    )
}
fn bundle(minted: bool) -> MetaAuthBundle {
    MetaAuthBundle {
        token_data: Some(TokenData {
            access_token: "dca:test-only-device-token".into(),
            token_type: "Bearer".into(),
            expires_in: 3600,
            expires_at: 1_700_003_600,
            ..TokenData::default()
        }),
        minted_key: minted.then(|| MintedKeyResponse {
            api_key: "LLM|test-only-minted-key".into(),
            base_url: " https://regional.fixture.invalid/v1 ".into(),
            user_email: "minted@example.invalid".into(),
            user_full_name: "Minted Fixture".into(),
            ..MintedKeyResponse::default()
        }),
        email: "fallback@example.invalid".into(),
        name: "Fallback Fixture".into(),
    }
}

#[tokio::test]
async fn candidate_meta_auth_start_device_flow_uses_selected_transport_and_fixed_token_endpoint() {
    let transport = Transport::new(vec![Reply::json(
        201,
        json!({
            "device_code":"test-only-device-code","user_code":"TEST-1234","expires_in":900,"interval":5,
            "verification_uri":"https://auth.meta.com/device","token_endpoint":"https://attacker.invalid/token"
        }),
    )]);
    let result = service(transport.clone())
        .start_device_flow_with_endpoint(" https://fixture.invalid/device ")
        .await
        .unwrap();
    assert_eq!(result.token_endpoint, TOKEN_ENDPOINT);
    let requests = transport.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0.url, "https://fixture.invalid/device");
    assert_eq!(requests[0].0.method, "POST");
    assert_eq!(requests[0].0.body, b"client_id=1031625952748946");
    assert_eq!(
        requests[0].0.headers.get("User-Agent").unwrap(),
        &["muse-code/1.0.2"]
    );
    assert_eq!(
        requests[0].0.headers.get("Content-Type").unwrap(),
        &["application/x-www-form-urlencoded"]
    );
}
#[tokio::test]
async fn candidate_meta_auth_start_device_flow_rejects_missing_fields_and_http_failure() {
    for (status, body) in [
        (200, json!({"device_code":"  ","user_code":"USER"})),
        (200, json!({"device_code":"DEVICE","user_code":""})),
        (403, json!({"error":"test-only-secret"})),
    ] {
        let transport = Transport::new(vec![Reply::json(status, body)]);
        let error = service(transport.clone())
            .start_device_flow()
            .await
            .unwrap_err();
        if status == 403 {
            assert!(matches!(error, MetaAuthError::Upstream { status: 403, .. }));
        } else {
            assert!(matches!(error, MetaAuthError::MissingField(_)));
        }
        assert!(!format!("{error:?} {error}").contains("test-only-secret"));
        assert_eq!(transport.request_count(), 1);
    }
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_auth_pending_authorization_then_mint_preserves_clock_and_subscription() {
    let transport = Transport::new(vec![
        Reply::json(400, json!({"error":"authorization_pending"})),
        token_reply(),
        mint_reply(),
    ]);
    let start = Instant::now();
    let auth = service(transport.clone());
    let result = auth.wait_for_authorization(&device()).await.unwrap();
    assert_eq!(
        result.token_data.as_ref().unwrap().expires_at,
        1_700_003_600
    );
    let minted = result.minted_key.as_ref().unwrap();
    assert!(minted.is_subs_active && minted.has_payment_method);
    assert_eq!(minted.subs_tier_name, "Muse Pro");
    assert_eq!(result.email, "fixture@example.invalid");
    let requests = transport.requests.lock().unwrap();
    assert_eq!(requests[0].1.duration_since(start), Duration::from_secs(1));
    assert_eq!(requests[1].1.duration_since(start), Duration::from_secs(2));
    let form: BTreeMap<_, _> = url::form_urlencoded::parse(&requests[0].0.body)
        .into_owned()
        .collect();
    assert_eq!(form.get("grant_type").unwrap(), DEVICE_CODE_GRANT_TYPE);
    assert_eq!(form.get("device_code").unwrap(), "test-only-device-code");
    assert_eq!(requests[2].0.url, "https://fixture.invalid/key");
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_auth_slow_down_adds_five_seconds_before_the_next_poll() {
    let transport = Transport::new(vec![
        Reply::json(400, json!({"error":"slow_down"})),
        token_reply(),
        mint_reply(),
    ]);
    let start = Instant::now();
    service(transport.clone())
        .wait_for_authorization(&device())
        .await
        .unwrap();
    let requests = transport.requests.lock().unwrap();
    assert_eq!(requests[0].1.duration_since(start), Duration::from_secs(1));
    assert_eq!(requests[1].1.duration_since(start), Duration::from_secs(7));
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_auth_transient_transport_and_unstructured_http_errors_retry_until_authorized(
) {
    let transport = Transport::new(vec![
        Reply::json(500, json!({"unstructured":"test-only-provider-text"})),
        token_reply(),
        Reply::json(503, json!({"error":"mint unavailable"})),
    ]);
    transport
        .replies
        .lock()
        .unwrap()
        .push_front(Err("test-only-transport-failure".into()));
    let result = service(transport.clone())
        .wait_for_authorization(&device())
        .await
        .unwrap();
    assert!(result.minted_key.is_none());
    assert_eq!(
        result.token_data.unwrap().access_token,
        "dca:test-only-device-token"
    );
    assert_eq!(transport.request_count(), 4);
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_auth_denied_and_expired_device_codes_are_terminal_without_minting() {
    for code in ["access_denied", "expired_token"] {
        let transport = Transport::new(vec![Reply::json(400, json!({"error":code}))]);
        let error = service(transport.clone())
            .wait_for_authorization(&device())
            .await
            .unwrap_err();
        assert!(matches!(
            (code, error),
            ("access_denied", MetaAuthError::AccessDenied)
                | ("expired_token", MetaAuthError::ExpiredDeviceCode)
        ));
        assert_eq!(transport.request_count(), 1);
    }
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_auth_provider_error_and_malformed_success_do_not_mint() {
    for reply in [
        Reply::json(
            400,
            json!({"error":"test-only-secret","error_description":"test-only-secret"}),
        ),
        Reply::json(200, json!({"token_type":"Bearer"})),
        Reply {
            status: 200,
            body: b"invalid-json".to_vec(),
            hold: false,
            chunk_error: None,
        },
    ] {
        let transport = Transport::new(vec![reply]);
        let error = service(transport.clone())
            .wait_for_authorization(&device())
            .await
            .unwrap_err();
        assert!(!format!("{error:?} {error}").contains("test-only-secret"));
        assert_eq!(transport.request_count(), 1);
    }
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_auth_device_expiry_and_maximum_deadline_do_not_send_an_early_poll() {
    for (expiry, interval, elapsed) in [(2, 3, 2), (i64::MAX, i64::MAX, 900), (0, 1000, 900)] {
        let transport = Transport::new(Vec::new());
        let mut response = device();
        response.expires_in = expiry;
        response.interval = interval;
        let start = Instant::now();
        assert!(matches!(
            service(transport.clone())
                .wait_for_authorization(&response)
                .await,
            Err(MetaAuthError::Timeout)
        ));
        assert_eq!(
            Instant::now().duration_since(start),
            Duration::from_secs(elapsed)
        );
        assert_eq!(transport.request_count(), 0);
    }
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_auth_mint_at_authorization_deadline_keeps_recoverable_dca_and_drops_receiver(
) {
    let transport = Transport::new(vec![token_reply(), Reply::hold()]);
    let mut response = device();
    response.expires_in = 2;
    let result = service(transport.clone())
        .wait_for_authorization(&response)
        .await
        .unwrap();
    assert!(result.minted_key.is_none());
    assert_eq!(
        result.token_data.unwrap().access_token,
        "dca:test-only-device-token"
    );
    assert_eq!(transport.request_count(), 2);
    assert!(transport.held.lock().unwrap()[0].is_closed());
}
#[tokio::test(start_paused = true)]
async fn candidate_meta_auth_cancelled_owner_drops_the_selected_upstream_without_background_polling(
) {
    let transport = Transport::new(vec![Reply::hold()]);
    let auth = Arc::new(service(transport.clone()));
    let response = device();
    let task = tokio::spawn(async move { auth.wait_for_authorization(&response).await });
    tokio::task::yield_now().await;
    time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(transport.request_count(), 1);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(transport.held.lock().unwrap()[0].is_closed());
    time::advance(Duration::from_secs(30)).await;
    assert_eq!(transport.request_count(), 1);
}
#[tokio::test]
async fn candidate_meta_auth_key_minting_preserves_selected_endpoint_headers_and_trimmed_token() {
    let transport = Transport::new(vec![mint_reply()]);
    let auth = service(transport.clone());
    assert!(matches!(
        auth.mint_api_key("  ").await,
        Err(MetaAuthError::MissingDcaToken)
    ));
    auth.mint_api_key(" dca:test-only-device-token ")
        .await
        .unwrap();
    let requests = transport.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].0.headers.get("Authorization").unwrap(),
        &["Bearer dca:test-only-device-token"]
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[0].0.body).unwrap(),
        json!({"dca_token":"dca:test-only-device-token"})
    );
}
#[tokio::test]
async fn candidate_meta_auth_bounded_body_and_read_failure_close_owned_responses() {
    let transport = Transport::new(vec![Reply {
        status: 200,
        body: vec![b'x'; 1024 * 1024 + 1],
        hold: true,
        chunk_error: None,
    }]);
    assert!(matches!(
        service(transport.clone())
            .mint_api_key("dca:test-only")
            .await,
        Err(MetaAuthError::BodyLimit)
    ));
    assert!(transport.held.lock().unwrap()[0].is_closed());
    let transport = Transport::new(vec![Reply {
        status: 200,
        body: Vec::new(),
        hold: true,
        chunk_error: Some("test-only-read-error".into()),
    }]);
    assert!(matches!(
        service(transport.clone())
            .mint_api_key("dca:test-only")
            .await,
        Err(MetaAuthError::Transport(_))
    ));
    assert!(transport.held.lock().unwrap()[0].is_closed());
}
#[test]
fn candidate_meta_auth_storage_uses_minted_identity_base_url_and_preserves_separate_dca_expiry() {
    let storage = MetaTokenStorage::from_bundle(&bundle(true), FixedClock.now()).unwrap();
    assert_eq!(storage.access_token, "LLM|test-only-minted-key");
    assert_eq!(storage.dca_token, "dca:test-only-device-token");
    assert_eq!(storage.base_url, "https://regional.fixture.invalid/v1");
    assert_eq!(storage.email, "minted@example.invalid");
    assert_eq!(storage.name, "Minted Fixture");
    assert!(storage.expired.is_empty());
    assert_eq!(storage.dca_expired, "2023-11-14T23:13:20Z");
    assert_eq!(storage.dca_expires_at, 1_700_003_600);
}
#[test]
fn candidate_meta_auth_dca_only_storage_retains_expiry_without_fabricating_api_key_expiration() {
    let mut input = bundle(false);
    let storage = MetaTokenStorage::from_bundle(&input, FixedClock.now()).unwrap();
    assert_eq!(storage.expired, storage.dca_expired);
    assert_eq!(storage.base_url, DEFAULT_API_BASE_URL);
    assert!(storage.api_key.is_empty());
    input.token_data.as_mut().unwrap().expires_at = 0;
    assert!(MetaTokenStorage::from_bundle(&input, FixedClock.now())
        .unwrap()
        .expired
        .is_empty());
    assert!(MetaTokenStorage::from_bundle(&MetaAuthBundle::default(), FixedClock.now()).is_none());
}
#[test]
fn candidate_meta_auth_snapshot_protects_cleared_credentials_and_inherits_only_noncredential_settings(
) {
    let previous: BTreeMap<String, Value> = BTreeMap::from([
        ("access_token".into(), json!("old-token")),
        ("api_key".into(), json!("old-key")),
        ("expired".into(), json!("old-expiry")),
        ("dca_expired".into(), json!("old-dca-expiry")),
        ("dca_expires_at".into(), json!(123)),
        ("priority".into(), json!(42)),
        ("models".into(), json!(["muse-latest"])),
        ("disable-cooling".into(), json!(true)),
    ]);
    let mut storage = MetaTokenStorage::default();
    storage.access_token = "LLM|current-key".into();
    storage.api_key = "LLM|current-key".into();
    let snapshot = storage.snapshot(Some(&previous));
    assert_eq!(snapshot["access_token"], "LLM|current-key");
    assert_eq!(snapshot["api_key"], "LLM|current-key");
    for key in ["expired", "dca_expired", "dca_expires_at"] {
        assert!(!snapshot.contains_key(key));
    }
    assert_eq!(snapshot["priority"], 42);
    assert_eq!(snapshot["models"], json!(["muse-latest"]));
    storage.set_metadata(BTreeMap::from([
        ("disabled".into(), json!(true)),
        ("priority".into(), json!(2)),
        (
            "access_token".into(),
            json!("cannot-override-current-token"),
        ),
    ]));
    let snapshot = storage.snapshot(Some(&previous));
    assert_eq!(snapshot["disabled"], true);
    assert_eq!(snapshot["priority"], 2);
    assert_eq!(snapshot["access_token"], "LLM|current-key");
    assert!(!snapshot.contains_key("models"));
    assert!(!snapshot.contains_key("disable-cooling"));
}
#[test]
fn candidate_meta_auth_credential_names_are_stable_collision_safe_and_local() {
    assert_eq!(
        credential_file_name(" user@example.com ", ""),
        "meta-user_example.com-b4c9a289323b21a0.json"
    );
    assert_eq!(
        credential_file_name("", " 12345 "),
        "meta-5994471abb01112a.json"
    );
    assert_eq!(credential_file_name("", ""), "meta-oauth.json");
    assert_eq!(
        credential_file_name("alice+work@example.com", "dca:first"),
        "meta-alice_work_example.com-9fd097dc84cf05c9.json"
    );
    assert_eq!(
        credential_file_name("alice_work@example.com", "dca:second"),
        "meta-alice_work_example.com-a76718d79230ea74.json"
    );
    assert_eq!(
        credential_file_name("alice+work@example.com", "dca:first"),
        credential_file_name("alice+work@example.com", "dca:replacement")
    );
    let name = credential_file_name(&("界/\\:".repeat(100) + "@example.com"), "");
    assert!(name.len() <= 255 && !name.contains('/') && !name.contains('\\'));
}
#[test]
fn candidate_meta_auth_debug_and_error_formatting_never_disclose_credentials_or_provider_bodies() {
    let mut input = bundle(true);
    input.token_data.as_mut().unwrap().access_token = "test-only-private".into();
    let response = DeviceCodeResponse {
        device_code: "test-only-private".into(),
        user_code: "test-only-private".into(),
        verification_uri: "test-only-private".into(),
        ..DeviceCodeResponse::default()
    };
    let error = MetaAuthError::Upstream {
        status: 401,
        body: b"test-only-private".to_vec(),
    };
    let provider = MetaAuthError::Provider {
        code: "test-only-private".into(),
        description: "test-only-private".into(),
    };
    let storage = MetaTokenStorage::from_bundle(&input, FixedClock.now()).unwrap();
    let formatted = format!(
        "{input:?} {:?} {response:?} {error:?} {error} {provider:?} {provider} {storage:?}",
        input.token_data
    );
    assert!(!formatted.contains("test-only-private"));
}
