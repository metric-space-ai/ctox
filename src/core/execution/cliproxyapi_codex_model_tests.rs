// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::execution::cliproxyapi_host::{
    load_instance_proxy_config, save_instance_proxy_config, CtoxCodexSecretStore,
};
use ctox_cliproxyapi::internal::{
    auth::codex::{CodexSecretStore, CodexStoredCredentials, SecretString},
    config::RuntimeSecretRef,
};
#[test]
fn known_dormant_subscription_defaults_survive_load_but_unknown_defaults_are_rejected() -> Result<()>
{
    for provider in ["claude", "codex", "antigravity"] {
        let root = tempfile::tempdir()?;
        let runtime: CliproxyRuntimeConfig = serde_json::from_value(serde_json::json!({}))?;
        save_instance_proxy_config(root.path(), 0, provider, runtime)?;
        assert_eq!(
            load_instance_proxy_config(root.path())?
                .unwrap()
                .default_provider,
            provider
        );
    }
    let (root, _) = fixture()?;
    let cfg = load_instance_proxy_config(root.path())?.unwrap();
    assert!(save_instance_proxy_config(
        root.path(),
        cfg.revision,
        "unregistered-provider",
        cfg.runtime
    )
    .is_err());
    Ok(())
}
pub(crate) fn fixture() -> Result<(tempfile::TempDir, CodexSubscriptionAccountConfig)> {
    let root = tempfile::tempdir()?;
    let key = |name: &str| RuntimeSecretRef {
        scope: "provider-subscriptions".into(),
        name: name.into(),
    };
    let account = CodexSubscriptionAccountConfig {
        id: "codex-model-check-fixture".into(),
        disabled: false,
        priority: 0,
        weight: 1,
        websockets: false,
        models: vec![],
        id_token_secret: key("probe-id"),
        access_token_secret: key("probe-access"),
        refresh_token_secret: key("probe-refresh"),
        upstream_base_url: "https://chatgpt.com/backend-api/codex".into(),
        plan_type: "pro".into(),
        proxy_url_secret: None,
    };
    CtoxCodexSecretStore::new(root.path()).store_credentials(
        &account.credential_handles()?,
        &CodexStoredCredentials::new(
            SecretString::new("fixture-id")?,
            SecretString::new("fixture-access")?,
            SecretString::new("fixture-refresh")?,
        ),
    )?;
    let runtime: CliproxyRuntimeConfig =
        serde_json::from_value(serde_json::json!({"codex_accounts":[account.clone()]}))?;
    save_instance_proxy_config(root.path(), 0, "codex", runtime)?;
    Ok((root, account))
}
#[test]
fn codex_model_publication_all_token_generations_are_required() -> Result<()> {
    for which in 0..3 {
        let (root, a) = fixture()?;
        let check = NativeCodexPublicationCheck::open(root.path(), &a)?;
        check.records(|| Ok(()))?;
        let keys = [
            &a.id_token_secret,
            &a.access_token_secret,
            &a.refresh_token_secret,
        ];
        let key = keys[which];
        let value = ["fixture-id", "fixture-access", "fixture-refresh"][which];
        // Even the same plaintext written again is a new encrypted generation.
        crate::secrets::write_secret_record(
            root.path(),
            &key.scope,
            &key.name,
            value,
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
    }
    Ok(())
}
#[test]
fn codex_model_publication_config_disable_and_removal_retire_results() -> Result<()> {
    for change in ["priority", "disable", "remove"] {
        let (root, a) = fixture()?;
        let check = NativeCodexPublicationCheck::open(root.path(), &a)?;
        let mut cfg = load_instance_proxy_config(root.path())?.unwrap();
        match change {
            "priority" => cfg.runtime.codex_accounts[0].priority = 9,
            "disable" => cfg.runtime.codex_accounts[0].disabled = true,
            _ => cfg.runtime.codex_accounts.clear(),
        }
        if change == "priority" {
            save_instance_proxy_config(root.path(), cfg.revision, "codex", cfg.runtime)?;
        } else {
            let controls = super::super::cliproxyapi_host::account_controls::account_binding(
                root.path(),
                "codex",
                &a.id,
            )?
            .unwrap();
            super::super::cliproxyapi_host::account_controls::apply_account(
                root.path(),
                "model-check-change",
                "hash",
                "owner",
                "codex",
                &a.id,
                &controls,
                if change == "disable" {
                    Some(false)
                } else {
                    None
                },
                || Ok(()),
            )?;
            super::super::cliproxyapi_host::account_controls::finish(
                root.path(),
                "model-check-change",
                "hash",
                "owner",
            )?;
        }
        let mut called = false;
        assert!(check
            .records(|| {
                called = true;
                Ok(())
            })
            .is_err());
        assert!(!called);
    }
    Ok(())
}
#[test]
fn codex_model_publication_excludes_config_writes_during_send() -> Result<()> {
    let (root, a) = fixture()?;
    let check = NativeCodexPublicationCheck::open(root.path(), &a)?;
    let writer = Connection::open(crate::inference::runtime_env::runtime_config_path(
        root.path(),
    ))?;
    writer.busy_timeout(std::time::Duration::ZERO)?;
    check.records(|| {
        assert!(writer
            .execute(
                "UPDATE cliproxyapi_runtime_config SET revision=revision+1 WHERE config_id=1",
                []
            )
            .is_err());
        Ok(())
    })?;
    assert_eq!(
        writer.execute(
            "UPDATE cliproxyapi_runtime_config SET revision=revision+1 WHERE config_id=1",
            []
        )?,
        1
    );
    Ok(())
}
#[test]
fn codex_model_private_binding_rejects_inherited_and_aliased_credentials() -> Result<()> {
    let (root, a) = fixture()?;
    let binding = super::super::cliproxyapi_host::account_controls::account_binding;
    assert!(binding(root.path(), "codex", &a.id)?.is_some());
    assert!(binding(root.path(), "codex", "codex-instance-primary")?.is_none());
    let mut cfg = load_instance_proxy_config(root.path())?.unwrap();
    let mut alias = a.clone();
    alias.id = "codex-alias".into();
    cfg.runtime.codex_accounts.push(alias);
    save_instance_proxy_config(root.path(), cfg.revision, "codex", cfg.runtime)?;
    assert!(binding(root.path(), "codex", &a.id)?.is_none());
    Ok(())
}
