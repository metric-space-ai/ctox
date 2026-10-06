// ref: internal/auth/devin/user_status_test.go:17-201
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use crate::sdk::pluginapi::{HttpResponse, HttpStreamChunk, HttpStreamResponse, PluginFuture};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use tokio::sync::mpsc;

fn mock_status() -> Vec<u8> {
    let mut org = Vec::new();
    append_data(&mut org, 4, b"org-test-123");
    append_data(&mut org, 8, b"TestOrg");
    let mut info = Vec::new();
    append_data(&mut info, 2, b"Pro");
    append_data(&mut info, 33, &org);
    let mut plan = Vec::new();
    append_data(&mut plan, 1, &info);
    for (number, seconds) in [(2, 1789087364_u64), (3, 1791679364)] {
        let mut time = Vec::new();
        append_varint(&mut time, 8);
        append_varint(&mut time, seconds);
        append_data(&mut plan, number, &time);
    }
    for (number, value) in [(14, 100), (15, 50), (17, 1789200000), (18, 1789286400)] {
        append_varint(&mut plan, number << 3);
        append_varint(&mut plan, value);
    }
    let mut user = Vec::new();
    for (number, value) in [
        (3, "testuser"),
        (5, "team-123"),
        (7, "testuser@example.com"),
        (36, "user-id-456"),
    ] {
        append_data(&mut user, number, value.as_bytes());
    }
    append_data(&mut user, 13, &plan);
    let mut body = Vec::new();
    append_data(&mut body, 1, &user);
    body
}
fn unix_now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_791_000_000, 0).unwrap()
}

struct Transport {
    status: u16,
    chunks: Mutex<Option<mpsc::Receiver<HttpStreamChunk>>>,
    requests: Mutex<Vec<HttpRequest>>,
    full_calls: AtomicUsize,
}
impl HostHttpClient for Transport {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.full_calls.fetch_add(1, Ordering::SeqCst);
            panic!("unbounded execute must not be used")
        })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            Ok(HttpStreamResponse {
                status_code: self.status,
                headers: Headers::new(),
                chunks: self.chunks.lock().unwrap().take().unwrap(),
            })
        })
    }
}
fn transport(
    status: u16,
) -> (
    Arc<Transport>,
    mpsc::Sender<HttpStreamChunk>,
    DevinStatusService,
) {
    let (sender, receiver) = mpsc::channel(4);
    let transport = Arc::new(Transport {
        status,
        chunks: Mutex::new(Some(receiver)),
        requests: Mutex::new(Vec::new()),
        full_calls: AtomicUsize::new(0),
    });
    let service = DevinStatusService::new(transport.clone())
        .with_server_base_url(" https://test.invalid/// ");
    (transport, sender, service)
}
async fn send(sender: &mpsc::Sender<HttpStreamChunk>, bytes: Vec<u8>) {
    sender
        .send(HttpStreamChunk {
            payload: bytes,
            error: None,
        })
        .await
        .unwrap();
}

#[test]
fn candidate_devin_status_request_matches_primary_metadata_bytes() {
    let bytes = build_get_user_status_request("abc", "xyz", "linux");
    let hex: String = bytes.iter().map(|value| format!("{value:02x}")).collect();
    assert_eq!(hex, "0a3e0a0663686973656c120a333030302e31302e32311a036162632202656e2a056c696e75783a0a333030302e31302e3231620663686973656cfa010378797a");
    let fallback = build_get_user_status_request("token", "", "darwin");
    let outer = WireReader::new(&fallback).next().unwrap().unwrap();
    let WireValue::Bytes(metadata) = outer.value else {
        panic!("metadata");
    };
    let mut reader = WireReader::new(metadata);
    let mut fingerprint = None;
    while let Some(field) = reader.next().unwrap() {
        if let (31, WireValue::Bytes(value)) = (field.number, field.value) {
            fingerprint = Some(go_utf8_text(value));
        }
    }
    assert_eq!(
        fingerprint.unwrap(),
        generate_devin_device_fingerprint("token")
    );
}

#[test]
fn candidate_devin_status_primary_mock_preserves_plan_identity_and_reset_times() {
    let status = parse_get_user_status_response(&mock_status()).unwrap();
    assert_eq!(
        (
            &*status.user_name,
            &*status.email,
            &*status.user_id,
            &*status.team_id
        ),
        (
            "testuser",
            "testuser@example.com",
            "user-id-456",
            "team-123"
        )
    );
    assert_eq!(
        (&*status.plan, &*status.org_id, &*status.org_name),
        ("Pro", "org-test-123", "TestOrg")
    );
    assert_eq!(status.daily_quota_remaining_percent, Some(100));
    assert_eq!(status.weekly_quota_remaining_percent, Some(50));
    assert_eq!(status.daily_quota_reset_at, Some(1789200000));
    assert_eq!(status.weekly_quota_reset_at, Some(1789286400));
    assert_eq!(status.plan_start, Some(1789087364));
    assert_eq!(status.plan_end, Some(1791679364));
    let debug = format!("{status:?}");
    assert!(!debug.contains("testuser"));
    assert!(!debug.contains("TestOrg"));
}

#[test]
fn candidate_devin_status_outer_errors_nested_partial_unknown_groups_and_utf8() {
    assert!(matches!(
        parse_get_user_status_response(&[]),
        Err(DevinStatusError::EmptyResponse)
    ));
    assert!(matches!(
        parse_get_user_status_response(&[0x0a, 0x03, 1]),
        Err(DevinStatusError::Protobuf(_))
    ));
    let mut user = Vec::new();
    append_data(&mut user, 3, &[b'a', 0xe2, 0x82]);
    user.push(0xff);
    let mut body = vec![0x13, 0x18, 1, 0x14];
    append_data(&mut body, 1, &user);
    let status = parse_get_user_status_response(&body).unwrap();
    assert_eq!(status.user_name, "a\u{fffd}\u{fffd}");
    assert_eq!(status.daily_quota_remaining_percent, None);
}

#[test]
fn candidate_devin_status_signed_percentages_and_zero_observation_are_distinct() {
    let mut plan = Vec::new();
    append_varint(&mut plan, 14 << 3);
    append_varint(&mut plan, 0);
    append_varint(&mut plan, 15 << 3);
    append_varint(&mut plan, u64::MAX);
    append_varint(&mut plan, 17 << 3);
    append_varint(&mut plan, 1_u64 << 63);
    let mut user = Vec::new();
    append_data(&mut user, 13, &plan);
    let mut body = Vec::new();
    append_data(&mut body, 1, &user);
    let status = parse_get_user_status_response(&body).unwrap();
    assert_eq!(status.daily_quota_remaining_percent, Some(0));
    assert_eq!(status.weekly_quota_remaining_percent, Some(-1));
    assert_eq!(status.daily_quota_reset_at, Some(i64::MIN));
    assert_eq!(status.weekly_quota_reset_at, None);
}

#[tokio::test]
async fn candidate_devin_status_sdk_request_is_raw_proto_with_selected_headers() {
    let (http, sender, service) = transport(200);
    let bytes = mock_status();
    send(&sender, bytes[..9].to_vec()).await;
    send(&sender, bytes[9..].to_vec()).await;
    drop(sender);
    assert_eq!(
        service
            .fetch_user_status(" token ", "seed")
            .await
            .unwrap()
            .plan,
        "Pro"
    );
    let requests = http.requests.lock().unwrap();
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.url,
        format!("https://test.invalid{DEVIN_GET_USER_STATUS_PATH}")
    );
    assert_eq!(request.headers["Authorization"], vec!["Basic token-token"]);
    assert_eq!(request.headers["Content-Type"], vec!["application/proto"]);
    assert_eq!(request.headers["Connect-Protocol-Version"], vec!["1"]);
    assert_eq!(request.headers["Accept"], vec!["*/*"]);
    assert_eq!(request.headers["User-Agent"], vec![""]);
    assert!(!request.headers.contains_key("Sentry-Trace"));
    assert_eq!(
        request.body,
        build_get_user_status_request(
            "token",
            &generate_devin_device_fingerprint("seed"),
            platform()
        )
    );
    assert_eq!(http.full_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn candidate_devin_status_response_bound_releases_upstream_and_hides_error_body() {
    let (_, sender, service) = transport(429);
    send(&sender, vec![b'x'; MAX_STATUS_BODY + 1]).await;
    let error = service
        .fetch_user_status("token", "seed")
        .await
        .unwrap_err();
    assert!(sender.is_closed());
    let debug = format!("{error:?}");
    assert!(!debug.contains("xxxx"));
    let DevinStatusError::Upstream { status, body } = error else {
        panic!("status error");
    };
    assert_eq!(status, 429);
    assert_eq!(body.len(), MAX_STATUS_BODY);
}

#[tokio::test]
async fn candidate_devin_status_timeout_and_read_error_do_not_publish_success() {
    let (_, sender, service) = transport(200);
    assert!(matches!(
        service
            .fetch_bounded("token", "seed", Duration::from_millis(10))
            .await,
        Err(DevinStatusError::Timeout)
    ));
    assert!(sender.is_closed());
    let (_, sender, service) = transport(200);
    sender
        .send(HttpStreamChunk {
            payload: mock_status(),
            error: Some(Arc::new(std::io::Error::other(
                "fixture stream read failed",
            ))),
        })
        .await
        .unwrap();
    assert!(matches!(
        service.fetch_user_status("token", "seed").await,
        Err(DevinStatusError::Transport(_))
    ));
    assert!(sender.is_closed());
}

#[tokio::test]
async fn candidate_devin_status_refresh_uses_selected_auth_and_preserves_it_on_failure() {
    let mut auth = Auth::default();
    auth.id = "selected".into();
    auth.provider = "devin".into();
    auth.attributes
        .insert("api_key".into(), "selected-token".into());
    auth.attributes
        .insert("base_url".into(), "https://selected.invalid/".into());
    auth.attributes
        .insert("device_seed".into(), "selected-seed".into());
    auth.metadata
        .insert("session_token".into(), Value::String("wrong-token".into()));
    auth.quota.exceeded = true;
    auth.quota.reason = "existing cooldown".into();
    auth.quota
        .signals
        .insert("stale".into(), "old observation".into());
    let before = serde_json::to_value(&auth).unwrap();
    let (http, sender, service) = transport(200);
    send(&sender, mock_status()).await;
    drop(sender);
    let updated = service.refresh_auth(&auth, unix_now()).await.unwrap();
    assert_eq!(serde_json::to_value(&auth).unwrap(), before);
    assert_eq!(updated.id, "selected");
    assert!(updated.quota.exceeded);
    assert_eq!(updated.quota.reason, "existing cooldown");
    assert!(!updated.quota.signals.contains_key("stale"));
    assert_eq!(
        updated.quota.signals["daily_quota_remaining_percent"],
        "100%"
    );
    assert_eq!(updated.quota.observed_at, unix_now());
    assert_eq!(updated.last_refreshed_at, unix_now());
    assert_eq!(updated.attributes["email"], "testuser@example.com");
    assert_eq!(
        updated.metadata["email"],
        Value::String("testuser@example.com".into())
    );
    let requests = http.requests.lock().unwrap();
    assert_eq!(
        requests[0].url,
        format!("https://selected.invalid{DEVIN_GET_USER_STATUS_PATH}")
    );
    assert_eq!(
        requests[0].headers["Authorization"],
        vec!["Basic selected-token-selected-token"]
    );
    drop(requests);
    let (_, sender, service) = transport(403);
    send(&sender, b"quota endpoint denied".to_vec()).await;
    drop(sender);
    assert!(service.refresh_auth(&auth, unix_now()).await.is_err());
    assert_eq!(serde_json::to_value(&auth).unwrap(), before);
}

#[tokio::test]
async fn candidate_devin_status_no_credentials_does_not_issue_http_or_stamp_refresh() {
    let (http, _sender, service) = transport(200);
    let mut auth = Auth::default();
    auth.metadata.insert(
        "token".into(),
        Value::String("metadata-only-is-not-a-credential".into()),
    );
    let before = serde_json::to_value(&auth).unwrap();
    let unchanged = service.refresh_auth(&auth, unix_now()).await.unwrap();
    assert_eq!(serde_json::to_value(unchanged).unwrap(), before);
    assert!(http.requests.lock().unwrap().is_empty());
    assert!(matches!(
        service.fetch_user_status("  ", "seed").await,
        Err(DevinStatusError::MissingToken)
    ));
    assert!(http.requests.lock().unwrap().is_empty());
}

#[test]
fn candidate_devin_status_projection_keeps_unknown_limits_and_utc_calendar_domain() {
    for seconds in [-62135596800, -1, 0, 86400, 1789200000, 253402300799] {
        assert_eq!(
            format_unix_rfc3339(seconds),
            DateTime::<Utc>::from_timestamp(seconds, 0)
                .unwrap()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        );
    }
    assert_eq!(format_unix_rfc3339(253402300800), "10000-01-01T00:00:00Z");
    let mut auth = Auth::default();
    auth.attributes
        .insert("email".into(), "existing@example.invalid".into());
    auth.quota.signals = BTreeMap::from([("daily_quota_remaining_percent".into(), "0%".into())]);
    let updated = apply_user_status(&auth, &DevinUserStatus::default(), unix_now());
    assert!(updated.quota.signals.is_empty());
    assert_eq!(updated.attributes["email"], "existing@example.invalid");
    assert_eq!(updated.quota.observed_at, unix_now());
}
