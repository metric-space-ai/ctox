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
