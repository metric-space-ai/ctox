// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use ctox_cliproxyapi::internal::auth::claude::{ClaudeStoredCredentials, SecretString};
#[tokio::test]
async fn native_controls_dormant_default_loads_starts_and_never_routes_to_another_provider(
) -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let creds = ClaudeStoredCredentials::new(
        SecretString::new("dormant-access")?,
        SecretString::new("dormant-refresh")?,
    );
    install_claude_subscription(root.path(), "dormant-claude", &creds)?;
    let binding = super::super::super::cliproxyapi_claude_catalog::account_binding(
        root.path(),
        "dormant-claude",
    )?
    .unwrap();
    // A different provider is genuinely configured in this isolated fixture.
    // The dormant Claude default must still fail locally, without calling it.
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let jwt = format!("{}.{}.fixture", URL_SAFE_NO_PAD.encode(b"{}"), URL_SAFE_NO_PAD.encode(serde_json::to_vec(&serde_json::json!({"https://api.openai.com/auth":{"chatgpt_plan_type":"pro","chatgpt_account_id":"fixture-other-provider"}}))?));
    crate::secrets::write_secret_record(
        root.path(),
        INSTANCE_CHATGPT_AUTH_SCOPE,
        INSTANCE_CHATGPT_AUTH_NAME,
        &serde_json::to_string(
            &serde_json::json!({"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"id_token":jwt,"access_token":"other-access","refresh_token":"other-refresh","account_id":"fixture-other-provider"},"last_refresh":null}),
        )?,
        None,
        serde_json::json!({"test":true}),
    )?;
    let retained_config = effective_instance_proxy_config(root.path())?.unwrap();
    let retained_routes = build_instance_provider_routes(root.path())?.unwrap();
    assert!(routes_for_connection_admission(root.path(), &retained_config)?.is_none());
    apply(
        root.path(),
        "disable-default",
        "hash-disable",
        "owner",
        "dormant-claude",
        &binding,
        Some(false),
        || Ok(()),
    )?;
    finish(root.path(), "disable-default", "hash-disable", "owner")?;
    let stored = load_instance_proxy_config(root.path())?.unwrap();
    assert_eq!(stored.default_provider, "claude");
    assert!(stored.runtime.claude_accounts[0].disabled);
    // Exercise the same admission hook used by the running listener, retaining
    // the old router across the committed control instead of constructing a
    // fresh listener. New traffic must receive the disabled topology.
    let routes = routes_for_connection_admission(root.path(), &retained_config)?
        .context("retained listener must rebuild disabled routes")?;
    assert!(!Arc::ptr_eq(&retained_routes.responses, &routes.responses));
    let explicit = routes
        .responses
        .handle_provider_route(Some("codex"), b"{}")
        .await;
    assert!(
        matches!(explicit, OpenAiResponsesRouteResponse::Buffered(ref response)
        if response.status() == 400 && !String::from_utf8_lossy(response.body()).contains("requested provider is not configured"))
    );
    let response = routes.responses.handle_provider_route(None, b"{}").await;
    assert!(
        matches!(response,OpenAiResponsesRouteResponse::Buffered(ref response) if response.status()==400)
    );
    let retained_config = effective_instance_proxy_config(root.path())?.unwrap();
    assert!(routes_for_connection_admission(root.path(), &retained_config)?.is_none());
    let binding = super::super::super::cliproxyapi_claude_catalog::account_binding(
        root.path(),
        "dormant-claude",
    )?
    .unwrap();
    apply(
        root.path(),
        "remove-default",
        "hash-remove",
        "owner",
        "dormant-claude",
        &binding,
        None,
        || Ok(()),
    )?;
    finish(root.path(), "remove-default", "hash-remove", "owner")?;
    assert_eq!(
        load_instance_proxy_config(root.path())?
            .unwrap()
            .default_provider,
        "claude"
    );
    let removed_routes = routes_for_connection_admission(root.path(), &retained_config)?
        .context("retained listener must rebuild removed routes")?;
    assert!(!Arc::ptr_eq(&routes.responses, &removed_routes.responses));
    let routes = removed_routes;
    let explicit = routes
        .responses
        .handle_provider_route(Some("codex"), b"{}")
        .await;
    assert!(
        matches!(explicit, OpenAiResponsesRouteResponse::Buffered(ref response)
        if response.status() == 400 && !String::from_utf8_lossy(response.body()).contains("requested provider is not configured"))
    );
    let response = routes.responses.handle_provider_route(None, b"{}").await;
    assert!(
        matches!(response,OpenAiResponsesRouteResponse::Buffered(ref response) if response.status()==400)
    );
    Ok(())
}

#[test]
fn native_controls_remove_quota_without_late_response_resurrection_or_sibling_loss() -> anyhow::Result<()> {
    use ctox_cliproxyapi::sdk::cliproxy::auth::{AccountQuotaStore, QuotaWindowKind, ReportedQuotaSnapshot, ReportedQuotaStatus, ReportedQuotaWindow};
    let root = tempfile::tempdir()?;
    let install = |id: &str, access: &str| -> anyhow::Result<()> {
        install_claude_subscription(root.path(), id, &ClaudeStoredCredentials::new(SecretString::new(access)?, SecretString::new(format!("refresh-{access}"))?))
    };
    let quota = |at: i64| ReportedQuotaSnapshot { observed_at_ms: at, windows: vec![ReportedQuotaWindow {
        kind: QuotaWindowKind::FiveHour, status: ReportedQuotaStatus::Allowed, reset_at_ms: at + 10_000, utilization: Some(0.25)
    }] };
    let binding = |id: &str| -> anyhow::Result<String> {
        super::super::super::cliproxyapi_claude_catalog::account_binding(root.path(), id)?.context("fixture binding missing")
    };
    let ids = ["quota-control-a".to_string(), "quota-control-b".to_string()];
    install(&ids[0], "old-quota-control-access")?;
    install(&ids[1], "sibling-quota-control-access")?;
    let store = super::super::account_quotas::CtoxAccountQuotaStore::new(root.path());
    store.save("claude", &ids[0], &quota(1_000))?;
    store.save("claude", &ids[1], &quota(2_000))?;
    let in_flight = store.capture_generation("claude", &ids[0])?;

    apply(root.path(), "quota-disable", "disable-hash", "owner", &ids[0], &binding(&ids[0])?, Some(false), || Ok(()))?;
    finish(root.path(), "quota-disable", "disable-hash", "owner")?;
    assert_eq!(store.load("claude", &ids)?[&ids[0]], quota(1_000), "disable retains actual observations");
    apply(root.path(), "quota-remove", "remove-hash", "owner", &ids[0], &binding(&ids[0])?, None, || Ok(()))?;
    finish(root.path(), "quota-remove", "remove-hash", "owner")?;
    finish(root.path(), "quota-remove", "remove-hash", "owner")?;
    store.save_if_generation("claude", &ids[0], &quota(3_000), in_flight.as_ref())?;
    assert_eq!(store.load("claude", &ids)?, std::collections::BTreeMap::from([(ids[1].clone(), quota(2_000))]));
    let conn = crate::persistence::open_sqlite(root.path())?;
    let rows: i64 = conn.query_row("SELECT COUNT(*) FROM ctox_payload_store WHERE payload_key LIKE 'cliproxyapi.auth.reported-quota.v1:%'", [], |row| row.get(0))?;
    assert_eq!(rows, 1, "the removed row was physically deleted and not recreated");
    drop(conn);

    install(&ids[0], "new-quota-control-access")?;
    store.save("claude", &ids[0], &quota(4_000))?;
    store.save_if_generation("claude", &ids[0], &quota(5_000), in_flight.as_ref())?;
    assert!(finish(root.path(), "quota-remove", "remove-hash", "owner").is_err(), "old removal proof cannot clear a re-created account");
    assert_eq!(store.load("claude", &ids)?[&ids[0]], quota(4_000));
    assert_eq!(store.load("claude", &ids)?[&ids[1]], quota(2_000));
    Ok(())
}
