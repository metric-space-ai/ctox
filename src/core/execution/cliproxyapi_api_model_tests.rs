// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::execution::cliproxyapi_integration::{
    save_provider_integration_config, ApiKeyEndpoint, ProviderSecretRef,
};
fn fixture() -> Result<(tempfile::TempDir, ApiKeyAccountConfig)> {
    let root = tempfile::tempdir()?;
    let a = ApiKeyAccountConfig {
        id: "api-probe-fixture".into(),
        endpoint: ApiKeyEndpoint::Xai,
        api_key_secret: ProviderSecretRef::new("provider-api-keys", "api-probe-fixture"),
        owner_user_id: Some("owner".into()),
        creation_operation: Some(uuid::Uuid::new_v4().to_string()),
        disabled: false,
        priority: 0,
        models: vec![],
    };
    crate::secrets::write_secret_record(
        root.path(),
        &a.api_key_secret.scope,
        &a.api_key_secret.name,
        "synthetic-api-probe-key",
        None,
        serde_json::json!({}),
    )?;
    let mut cfg = load_provider_integration_config(root.path())?;
    cfg.config.api_key_accounts.push(a.clone());
    save_provider_integration_config(root.path(), cfg.revision, &cfg.config)?;
    Ok((root, a))
}
#[test]
fn api_model_publication_same_plaintext_rotation_retires_the_encrypted_generation() -> Result<()> {
    let (root, a) = fixture()?;
    let check = NativeApiPublicationCheck::open(root.path(), &a)?;
    check.records(|| Ok(()))?;
    crate::secrets::write_secret_record(
        root.path(),
        &a.api_key_secret.scope,
        &a.api_key_secret.name,
        "synthetic-api-probe-key",
        None,
        serde_json::json!({}),
    )?;
    let mut called = false;
    assert!(check
        .records(|| {
            called = true;
            Ok(())
        })
        .is_err());
    assert!(!called);
    Ok(())
}
#[test]
fn api_model_publication_config_change_and_removal_never_publish_old_results() -> Result<()> {
    let (root, a) = fixture()?;
    let check = NativeApiPublicationCheck::open(root.path(), &a)?;
    let mut cfg = load_provider_integration_config(root.path())?;
    cfg.config.api_key_accounts[0].priority = 9;
    save_provider_integration_config(root.path(), cfg.revision, &cfg.config)?;
    let mut called = false;
    assert!(check
        .records(|| {
            called = true;
            Ok(())
        })
        .is_err());
    assert!(!called);
    let cfg = load_provider_integration_config(root.path())?;
    let changed = cfg.config.api_key_accounts[0].clone();
    let check = NativeApiPublicationCheck::open(root.path(), &changed)?;
    let mut empty = cfg.config;
    empty.api_key_accounts.clear();
    save_provider_integration_config(root.path(), cfg.revision, &empty)?;
    assert!(check
        .records(|| {
            called = true;
            Ok(())
        })
        .is_err());
    assert!(!called);
    Ok(())
}
#[test]
fn api_model_publication_fence_excludes_runtime_writes_until_send_returns() -> Result<()> {
    let (root, a) = fixture()?;
    let check = NativeApiPublicationCheck::open(root.path(), &a)?;
    let writer = Connection::open(crate::inference::runtime_env::runtime_config_path(
        root.path(),
    ))?;
    writer.busy_timeout(std::time::Duration::ZERO)?;
    check.records(||{assert!(writer.execute(&format!("UPDATE {PROVIDER_INTEGRATION_CONFIG_TABLE} SET revision=revision+1 WHERE config_id=1"),[]).is_err());Ok(())})?;
    assert_eq!(writer.execute(&format!("UPDATE {PROVIDER_INTEGRATION_CONFIG_TABLE} SET revision=revision+1 WHERE config_id=1"),[])?,1);
    Ok(())
}
