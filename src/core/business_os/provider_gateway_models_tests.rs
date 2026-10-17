// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::execution::cliproxyapi_integration::{ApiKeyEndpoint, ProviderSecretRef};
fn model_fixture() -> String {
    let catalog = ctox_cliproxyapi::internal::registry::embedded_models_catalog().unwrap();
    ctox_cliproxyapi::internal::registry::models_for_channel(&catalog, "xai")
        .unwrap()
        .remove(0)
        .id
}
/// Unit fixture facts only: production catalog writes use admitted discovery.
/// The returned model IDs are existing wire fixtures, never product defaults.
pub(crate) fn seed(root: &Path, account: &ApiKeyAccountConfig, ids: &[String]) -> Result<String> {
    let owner = account
        .owner_user_id
        .as_deref()
        .context("fixture owner missing")?;
    store::issue_business_os_capability_token_for_managed_user(
        root,
        owner,
        "Fixture owner",
        "chef",
        store::now_ms() as i64,
    )?;
    let holder = store::stable_instance_id(root)?;
    let binding = crate::execution::cliproxyapi_host::account_controls::account_binding(
        root,
        account.endpoint.provider(),
        &account.id,
    )?;
    let mut conn = store::open_store(root)?;
    let tx = conn.transaction()?;
    adopt(
        &tx,
        owner,
        &holder,
        &[NativeAccountObservation {
            provider: account.endpoint.provider().into(),
            local_account_id: account.id.clone(),
            enabled: !account.disabled,
            credential_ready: true,
            private_binding: binding,
        }],
        store::now_ms() as i64,
    )?;
    let id:String=tx.query_row("SELECT account_id FROM business_provider_federation_accounts WHERE owner_user_id=?1 AND private_local_account_id=?2",params![owner,account.id],|r|r.get(0))?;
    let observation = crate::coding_agents::pi_sidecar::NativeModelCatalogObservation {
        provider: account.endpoint.provider().into(),
        checked_at_ms: store::now_ms() as i64,
        models: Some(ids.to_vec()),
        http_status: Some(200),
        elapsed_ms: 1,
        retry_after_seconds: None,
        failure: None,
        private_binding: None,
        inherited_selected_model: None,
    };
    retain_catalog_observation(
        &tx,
        &ObserveNativeRequest {
            _inbound_channel: None,
            account_id: id.clone(),
            expected_account_revision: 1,
        },
        &observation,
    )?;
    tx.commit()?;
    Ok(id)
}
fn fixture(
    root: std::sync::Arc<tempfile::TempDir>,
) -> Result<(
    std::sync::Arc<tempfile::TempDir>,
    ApiKeyAccountConfig,
    String,
)> {
    let account = ApiKeyAccountConfig {
        id: "gateway-api-fixture".into(),
        endpoint: ApiKeyEndpoint::Xai,
        api_key_secret: ProviderSecretRef::new("provider-api-keys", "gateway-api-key"),
        owner_user_id: Some("owner".into()),
        creation_operation: Some(uuid::Uuid::new_v4().to_string()),
        disabled: false,
        priority: 0,
        models: Vec::new(),
    };
    crate::secrets::write_secret_record(
        root.path(),
        "provider-api-keys",
        "gateway-api-key",
        "synthetic-gateway-key",
        None,
        json!({}),
    )?;
    let mut config =
        crate::execution::cliproxyapi_integration::load_provider_integration_config(root.path())?;
    config.config.api_key_accounts.push(account.clone());
    crate::execution::cliproxyapi_integration::save_provider_integration_config(
        root.path(),
        config.revision,
        &config.config,
    )?;
    let id = seed(root.path(), &account, &[model_fixture()])?;
    Ok((root, account, id))
}
#[test]
fn gateway_models_read_current_catalog_and_preserve_configuration() -> Result<()> {
    crate::persistence::test_support::run(move |owner_root| {
        let (root, account, _) = fixture(owner_root)?;
        let before = crate::execution::cliproxyapi_integration::load_provider_integration_config(
            root.path(),
        )?;
        let binding = crate::execution::cliproxyapi_host::account_controls::account_binding(
            root.path(),
            "xai",
            &account.id,
        )?
        .unwrap();
        assert_eq!(
            read(root.path(), &account, &binding)?.models,
            vec![model_fixture()]
        );
        assert!(account.models.is_empty());
        assert_eq!(
            crate::execution::cliproxyapi_integration::load_provider_integration_config(
                root.path()
            )?,
            before
        );
        assert!(read(root.path(), &account, "stale-binding")?
            .models
            .is_empty());
        Ok(())
    })
}
#[test]
fn gateway_models_apply_selection_exclusion_and_retire_remove_readd_witness() -> Result<()> {
    crate::persistence::test_support::run(move |owner_root| {
        let (root, account, id) = fixture(owner_root)?;
        let binding = crate::execution::cliproxyapi_host::account_controls::account_binding(
            root.path(),
            "xai",
            &account.id,
        )?
        .unwrap();
        let before = read(root.path(), &account, &binding)?;
        let conn = store::open_store(root.path())?;
        conn.execute(
            "INSERT INTO business_provider_federation_models VALUES ('owner','xai','[]')",
            [],
        )?;
        bump_policy(&conn, "owner")?;
        assert!(read(root.path(), &account, &binding)?.models.is_empty());
        conn.execute(
        "UPDATE business_provider_federation_models SET models_json=?1 WHERE owner_user_id='owner'",
        [serde_json::to_string(&vec![model_fixture()])?],
    )?;
        bump_policy(&conn, "owner")?;
        let after = read(root.path(), &account, &binding)?;
        assert_eq!(before.models, after.models);
        assert!(before != after);
        conn.execute(
            "INSERT INTO business_provider_federation_model_exclusions VALUES (?1,?2)",
            params![id, serde_json::to_string(&vec![model_fixture()])?],
        )?;
        bump_policy(&conn, "owner")?;
        assert!(read(root.path(), &account, &binding)?.models.is_empty());
        Ok(())
    })
}
#[test]
fn gateway_models_denies_removed_disabled_wrong_owner_and_revoked_owner() -> Result<()> {
    crate::persistence::test_support::run(move |owner_root| {
        let (root, account, id) = fixture(owner_root)?;
        let binding = crate::execution::cliproxyapi_host::account_controls::account_binding(
            root.path(),
            "xai",
            &account.id,
        )?
        .unwrap();
        let mut foreign = account.clone();
        foreign.owner_user_id = Some("foreign".into());
        assert!(read(root.path(), &foreign, &binding)?.models.is_empty());
        let conn = store::open_store(root.path())?;
        conn.execute(
            "UPDATE business_provider_federation_accounts SET enabled=0 WHERE account_id=?1",
            [&id],
        )?;
        assert!(read(root.path(), &account, &binding)?.models.is_empty());
        conn.execute(
            "UPDATE business_provider_federation_accounts SET enabled=1 WHERE account_id=?1",
            [&id],
        )?;
        conn.execute(
            "UPDATE business_users SET active=0 WHERE user_id='owner'",
            [],
        )?;
        assert!(read(root.path(), &account, &binding)?.models.is_empty());
        conn.execute(
            "UPDATE business_users SET active=1 WHERE user_id='owner'",
            [],
        )?;
        // Retire dependent fixture metadata in the normal removal order. Keep
        // SQLite foreign-key enforcement enabled throughout this regression.
        conn.execute(
            "DELETE FROM business_provider_federation_model_observations WHERE account_id=?1",
            [&id],
        )?;
        conn.execute(
            "DELETE FROM business_provider_federation_native_bindings WHERE account_id=?1",
            [&id],
        )?;
        conn.execute(
            "DELETE FROM business_provider_federation_accounts WHERE account_id=?1",
            [&id],
        )?;
        assert!(read(root.path(), &account, &binding)?.models.is_empty());
        Ok(())
    })
}
