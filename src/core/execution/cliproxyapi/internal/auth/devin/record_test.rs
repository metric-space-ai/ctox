// ref: internal/auth/devin/record_test.go:13-100
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::sdk::pluginapi::{
    Headers, HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk, HttpStreamResponse,
    PluginFuture,
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;
fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_791_000_000, 0).unwrap()
}
fn status() -> DevinUserStatus {
    DevinUserStatus {
        user_name: "status-user".into(),
        user_id: "status-id".into(),
        org_id: "status-org".into(),
        email: "test@example.com".into(),
        plan: "Pro".into(),
        daily_quota_remaining_percent: Some(0),
        weekly_quota_remaining_percent: Some(50),
        daily_quota_reset_at: Some(1_791_086_400),
        ..DevinUserStatus::default()
    }
}
#[test]
fn candidate_devin_record_profile_precedence_and_zero_quota_are_preserved() {
    let profile = DevinSelfProfile {
        user_name: "profile-user".into(),
        user_id: "profile-id".into(),
        org_id: "profile-org".into(),
    };
    let auth = create_record(
        "devin-session-token$eyJ.test",
        profile,
        Some(&status()),
        now(),
    );
    assert_eq!(auth.id, "devin-profile-user.json");
    assert_eq!(auth.file_name, auth.id);
    assert_eq!(auth.provider, "devin");
    assert_eq!(auth.status, AuthStatus::Active);
    assert_eq!(auth.label, "Devin (profile-user - test@example.com)");
    for (key, value) in [
        ("user_name", "profile-user"),
        ("user_id", "profile-id"),
        ("org_id", "profile-org"),
        ("email", "test@example.com"),
        ("plan", "Pro"),
        ("auth_kind", "oauth"),
        ("session_token", "devin-session-token$eyJ.test"),
    ] {
        assert_eq!(auth.attributes[key], value);
        assert_eq!(auth.metadata[key], Value::String(value.into()));
    }
    assert_eq!(auth.quota.signals["daily_quota_remaining_percent"], "0%");
    assert_eq!(auth.quota.signals["weekly_quota_remaining_percent"], "50%");
    assert!(auth.quota.signals.contains_key("daily_quota_reset_at"));
    assert_eq!(auth.quota.observed_at, now());
    assert!(!auth.quota.exceeded);
}
#[test]
fn candidate_devin_record_status_fallback_keeps_missing_limits_unknown() {
    let mut status = status();
    status.daily_quota_remaining_percent = None;
    let auth = create_record(
        "custom-token",
        DevinSelfProfile::default(),
        Some(&status),
        now(),
    );
    assert_eq!(auth.id, "devin-status-user.json");
    assert_eq!(auth.attributes["user_id"], "status-id");
    assert_eq!(auth.attributes["base_url"], DEFAULT_SERVER_URL);
    assert!(!auth
        .quota
        .signals
        .contains_key("daily_quota_remaining_percent"));
    assert_eq!(auth.quota.signals["weekly_quota_remaining_percent"], "50%");
    assert!(!auth.attributes.contains_key("expired"));
    assert!(!auth.metadata.contains_key("expired"));
}
#[test]
fn candidate_devin_record_unsafe_and_oversized_names_cannot_escape_filename() {
    for name in [
        "../../outside/file".to_owned(),
        "..\\outside\\file".to_owned(),
        "姓名".to_owned(),
        "x".repeat(161),
    ] {
        let profile = DevinSelfProfile {
            user_name: name.clone(),
            ..DevinSelfProfile::default()
        };
        let auth = create_record("token", profile, None, now());
        assert!(auth.file_name.starts_with("devin-user-"));
        assert_eq!(
            Path::new(&auth.file_name)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            auth.file_name
        );
        assert!(!auth.file_name.contains(['/', '\\']));
        assert!(auth.file_name.len() < 200);
        assert_eq!(auth.label, format!("Devin ({name})"));
    }
    let name = "x".repeat(160);
    let auth = create_record(
        "token",
        DevinSelfProfile {
            user_name: name.clone(),
            ..DevinSelfProfile::default()
        },
        None,
        now(),
    );
    assert_eq!(auth.file_name, format!("devin-{name}.json"));
}
#[test]
fn candidate_devin_record_unknown_accounts_remain_distinct_and_have_no_fake_limits() {
    let first = create_record(
        "devin-session-token$eyJ.first",
        DevinSelfProfile::default(),
        None,
        now(),
    );
    let second = create_record(
        "devin-session-token$eyJ.second",
        DevinSelfProfile::default(),
        None,
        now(),
    );
    assert_ne!(first.file_name, second.file_name);
    assert!(first.quota.signals.is_empty());
    assert_eq!(first.quota.observed_at, now());
    assert_eq!(first.status, AuthStatus::Active);
}
struct UnavailableClient {
    requests: Mutex<Vec<HttpRequest>>,
}
impl HostHttpClient for UnavailableClient {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("must stream") })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            let (sender, chunks) = mpsc::channel(1);
            sender
                .try_send(HttpStreamChunk {
                    payload: b"unavailable".to_vec(),
                    error: None,
                })
                .unwrap();
            drop(sender);
            Ok(HttpStreamResponse {
                status_code: 403,
                headers: Headers::new(),
                chunks,
            })
        })
    }
}
#[tokio::test]
async fn candidate_devin_record_failed_enrichment_retains_oauth_without_auth_rejection() {
    let client = Arc::new(UnavailableClient {
        requests: Mutex::new(Vec::new()),
    });
    let service = DevinAuthService::new(client.clone())
        .with_api_base_url("https://profile.invalid")
        .with_server_base_url("https://status.invalid");
    let auth = service
        .create_auth_record(" eyJ.test.token ", now())
        .await
        .unwrap();
    assert_eq!(
        auth.attributes["session_token"],
        "devin-session-token$eyJ.test.token"
    );
    assert!(auth.quota.signals.is_empty());
    assert_eq!(auth.status, AuthStatus::Active);
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url, "https://profile.invalid/v3/self");
    assert_eq!(
        requests[0].headers["Authorization"],
        ["Bearer devin-session-token$eyJ.test.token"]
    );
    assert!(requests[1].url.starts_with("https://status.invalid/"));
    assert_eq!(
        requests[1].headers["Authorization"],
        ["Basic devin-session-token$eyJ.test.token-devin-session-token$eyJ.test.token"]
    );
}
#[tokio::test]
async fn candidate_devin_record_empty_token_never_calls_any_provider() {
    let client = Arc::new(UnavailableClient {
        requests: Mutex::new(Vec::new()),
    });
    let service = DevinAuthService::new(client.clone());
    assert!(matches!(
        service.create_auth_record(" \t", now()).await,
        Err(DevinAuthError::EmptySessionToken)
    ));
    assert!(client.requests.lock().unwrap().is_empty());
}
