// Origin: CTOX; License: AGPL-3.0-only
use super::*;

#[tokio::test]
async fn quota_fetch_rejects_credential_binding_retired_during_plugin_call() {
    let f = fixture().await;
    f.client.retire_on_fetch.store(true, Ordering::SeqCst);
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/fetch",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(
        response.status(),
        500,
        "retired account binding published stale quota"
    );
    assert_eq!(body(&response)["kind"], "account_context_mismatch");
    assert!(!String::from_utf8_lossy(response.body()).contains("fixture-secret-value"));
}

#[tokio::test]
async fn quota_reset_does_not_clear_routing_for_a_replaced_credential() {
    let f = fixture().await;
    f.client.retire_on_reset.store(true, Ordering::SeqCst);
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/reset",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(response.status(), 500);
    assert_eq!(body(&response)["kind"], "account_context_mismatch");
    assert_eq!(f.client.resets.load(Ordering::SeqCst), 1);
    assert_eq!(f.routing.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn quota_routing_reset_runs_inside_the_current_binding_fence() {
    let f = fixture().await;
    f.routing.require_fence.store(true, Ordering::SeqCst);
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/reset",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert_eq!(f.routing.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn quota_binding_store_failure_after_request_is_not_provider_auth_denial() {
    let f = fixture().await;
    f.client.unavailable_on_fetch.store(true, Ordering::SeqCst);
    let response = send(
        &f,
        "POST",
        "/v0/management/quota/fetch",
        json!({"auth_index":"account-index"}),
    )
    .await;
    assert_eq!(response.status(), 500);
    assert_eq!(body(&response)["kind"], "credential_source_unavailable");
}

#[tokio::test]
async fn quota_probe_success_and_auth_error_from_retired_binding_are_both_discarded() {
    for status in [200, 401] {
        let f = fixture().await;
        f.credentials.probe.store(true, Ordering::SeqCst);
        f.probe.retire_on_response.store(true, Ordering::SeqCst);
        f.probe.status.store(status, Ordering::SeqCst);
        let response = send(
            &f,
            "POST",
            "/v0/management/quota/fetch",
            json!({"auth_index":"account-index"}),
        )
        .await;
        assert_eq!(response.status(), 500);
        assert_eq!(body(&response)["kind"], "account_context_mismatch");
        assert_eq!(f.probe.calls.load(Ordering::SeqCst), 1);
    }
}
