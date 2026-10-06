// ref: internal/auth/devin/devin_auth_test.go:14-220
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::sdk::pluginapi::{HttpResponse, HttpStreamChunk, HttpStreamResponse, PluginFuture};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use tokio::sync::{mpsc, Notify};

struct Client {
    replies: Mutex<VecDeque<(u16, Vec<u8>, Option<PluginExecutionError>)>>,
    requests: Mutex<Vec<HttpRequest>>,
}
impl HostHttpClient for Client {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("unbounded full-body HTTP is forbidden") })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            let (status_code, payload, error) = self.replies.lock().unwrap().pop_front().unwrap();
            let (sender, chunks) = mpsc::channel(1);
            sender.try_send(HttpStreamChunk { payload, error }).unwrap();
            drop(sender);
            Ok(HttpStreamResponse {
                status_code,
                headers: Headers::new(),
                chunks,
            })
        })
    }
}
fn client(
    replies: Vec<(u16, Vec<u8>, Option<PluginExecutionError>)>,
) -> (Arc<Client>, DevinAuthService) {
    let client = Arc::new(Client {
        replies: Mutex::new(replies.into()),
        requests: Mutex::new(Vec::new()),
    });
    let service =
        DevinAuthService::new(client.clone()).with_api_base_url(" https://selected.invalid/// ");
    (client, service)
}

#[test]
fn candidate_devin_oauth_authorization_urls_keep_primary_order_and_go_escape() {
    let (_, service) = client(Vec::new());
    assert_eq!(service.build_authorization_url(" http://127.0.0.1:1234/callback ", "challenge", "state"),
        "https://app.devin.ai/auth/cli/continue?redirect_uri=http%3A%2F%2F127.0.0.1%3A1234%2Fcallback&state=state&prompt=select_account&code_challenge=challenge&code_challenge_method=S256");
    assert_eq!(service.build_authorization_url(" \t", "c+*~", "s &ß"),
        "https://app.devin.ai/auth/cli/continue?state=s+%26%C3%9F&prompt=select_account&code_challenge=c%2B%2A~&code_challenge_method=S256&cli_pkce_marker=1");
    assert_eq!(service.with_app_base_url(" https://app.selected.invalid/// ").with_app_base_url(" ").build_authorization_url("", "", ""),
        "https://app.selected.invalid/auth/cli/continue?prompt=select_account&code_challenge=&code_challenge_method=S256&cli_pkce_marker=1");
}
#[test]
fn candidate_devin_oauth_session_prefix_and_go_json_string_values() {
    for (input, expected) in [
        (" devin-session-token$eyJ123 ", "devin-session-token$eyJ123"),
        ("eyJ123.456.789", "devin-session-token$eyJ123.456.789"),
        (" custom-token ", "custom-token"),
        (" \t", ""),
    ] {
        assert_eq!(format_session_token(input), expected);
    }
    for (raw, expected) in [
        (r#"{"token":true}"#, "true"),
        (r#"{"token":null}"#, ""),
        (r#"{"token":1.20}"#, "1.2"),
        (r#"{"token":1e-7}"#, "0.0000001"),
        (r#"{"token":9007199254740993}"#, "9007199254740993"),
        (r#"{"token":{"x": 1}}"#, r#"{"x": 1}"#),
    ] {
        assert_eq!(json_field_string(raw.as_bytes(), "token"), expected);
    }
    assert_eq!(
        json_field_string(b"{\"token\":\"a\xff\xffb\"}", "token"),
        "a\u{fffd}\u{fffd}b"
    );
}
#[tokio::test]
async fn candidate_devin_oauth_exchange_and_profile_use_selected_transport() {
    let (client, service) = client(vec![
        (201, br#"{"token":" eyJtest-jwt-token "}"#.to_vec(), None),
        (
            200,
            br#"{"user_name":"profile-user","user_id":"profile-id","org_id":"profile-org"}"#
                .to_vec(),
            None,
        ),
    ]);
    let token = service
        .exchange_code_for_token(" code ", " verifier ")
        .await
        .unwrap();
    assert_eq!(token, "eyJtest-jwt-token");
    let profile = service
        .fetch_self_profile(&format_session_token(&token))
        .await
        .unwrap();
    assert_eq!(profile.user_name, "profile-user");
    assert_eq!(profile.user_id, "profile-id");
    assert_eq!(profile.org_id, "profile-org");
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].url, "https://selected.invalid/auth/cli/token");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&requests[0].body).unwrap(),
        serde_json::json!({"code":"code", "code_verifier":"verifier"})
    );
    assert_eq!(requests[0].headers["Content-Type"], ["application/json"]);
    assert!(!requests[0].headers.contains_key("Authorization"));
    assert_eq!(requests[1].method, "GET");
    assert_eq!(requests[1].url, "https://selected.invalid/v3/self");
    assert_eq!(
        requests[1].headers["Authorization"],
        ["Bearer devin-session-token$eyJtest-jwt-token"]
    );
    assert!(requests[1].body.is_empty());
    assert!(!format!("{profile:?}").contains("profile-user"));
}
#[tokio::test]
async fn candidate_devin_oauth_missing_token_and_http_failure_never_retry() {
    for body in [
        br#"{}"#.as_slice(),
        br#"{"token":null}"#,
        br#"{"token":" \t"}"#,
    ] {
        let (client, service) = client(vec![(200, body.to_vec(), None)]);
        assert!(matches!(
            service.exchange_code_for_token("once", "once").await,
            Err(DevinAuthError::MissingToken { .. })
        ));
        assert_eq!(client.requests.lock().unwrap().len(), 1);
    }
    let body = br#"{"error":"invalid_grant","echo":"private-test-value"}"#.to_vec();
    let (client, service) = client(vec![(401, body.clone(), None)]);
    let error = service
        .exchange_code_for_token("once", "once")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, DevinAuthError::Upstream { status: 401, body: actual } if actual == &body)
    );
    assert!(!format!("{error:?}").contains("private-test-value"));
    assert_eq!(client.requests.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn candidate_devin_oauth_unavailable_profile_is_best_effort() {
    let (_, service) = client(vec![(
        403,
        br#"{"user_name":"must-not-use"}"#.to_vec(),
        None,
    )]);
    assert_eq!(
        service.fetch_self_profile("known-token").await.unwrap(),
        DevinSelfProfile::default()
    );
    let (_, service) = client(vec![(200, b"not-json".to_vec(), None)]);
    assert_eq!(
        service.fetch_self_profile("known-token").await.unwrap(),
        DevinSelfProfile::default()
    );
}
#[tokio::test]
async fn candidate_devin_oauth_body_is_bounded_and_read_failure_is_not_success() {
    let (_, service) = client(vec![(429, vec![b'x'; MAX_AUTH_BODY + 100], None)]);
    assert!(
        matches!(service.exchange_code_for_token("code", "verifier").await, Err(DevinAuthError::Upstream { status: 429, body }) if body.len() == MAX_AUTH_BODY)
    );
    let error: PluginExecutionError = Arc::new(std::io::Error::other("stream failed"));
    let (_, service) = client(vec![(200, br#"{"token":"partial"}"#.to_vec(), Some(error))]);
    assert!(matches!(
        service.exchange_code_for_token("code", "verifier").await,
        Err(DevinAuthError::Transport(_))
    ));
}
struct PendingClient {
    started: Notify,
    calls: AtomicUsize,
    dropped: Arc<AtomicUsize>,
}
struct DropWitness(Arc<AtomicUsize>);
impl Drop for DropWitness {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl HostHttpClient for PendingClient {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("must stream") })
    }
    fn execute_stream<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _witness = DropWitness(self.dropped.clone());
            self.started.notify_one();
            std::future::pending().await
        })
    }
}
fn pending_client() -> Arc<PendingClient> {
    Arc::new(PendingClient {
        started: Notify::new(),
        calls: AtomicUsize::new(0),
        dropped: Arc::new(AtomicUsize::new(0)),
    })
}
#[tokio::test]
async fn candidate_devin_oauth_cancellation_releases_single_use_exchange() {
    let client = pending_client();
    let service = DevinAuthService::new(client.clone());
    let task = tokio::spawn(async move { service.exchange_code_for_token("once", "once").await });
    tokio::time::timeout(Duration::from_secs(1), client.started.notified())
        .await
        .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
    assert_eq!(client.dropped.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn candidate_devin_oauth_timeout_releases_selected_request_without_replay() {
    let client = pending_client();
    let service = DevinAuthService::new(client.clone());
    assert!(matches!(
        service
            .exchange_bounded("once", "once", Duration::from_millis(10))
            .await,
        Err(DevinAuthError::Timeout)
    ));
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
    assert_eq!(client.dropped.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn candidate_devin_oauth_invalid_url_is_rejected_before_transport() {
    let (client, service) = client(Vec::new());
    assert!(matches!(
        service
            .with_api_base_url("relative-url")
            .exchange_code_for_token("code", "verifier")
            .await,
        Err(DevinAuthError::InvalidUrl(_))
    ));
    assert!(client.requests.lock().unwrap().is_empty());
}
