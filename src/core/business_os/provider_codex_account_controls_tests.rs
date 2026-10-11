// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::execution::cliproxyapi_host::account_controls as holder;

fn install(f: &Fixture, id: &str, marker: &str) -> Result<()> {
    use ctox_cliproxyapi::internal::config::{
        CliproxyRuntimeConfig, CodexSubscriptionAccountConfig,
    };
    let key =
        |suffix: &str| json!({"scope":"provider-subscriptions","name":format!("{id}-{suffix}")});
    let account: CodexSubscriptionAccountConfig = serde_json::from_value(
        json!({"id":id,"plan_type":"pro","id_token_secret":key("id"),"access_token_secret":key("access"),"refresh_token_secret":key("refresh")}),
    )?;
    use ctox_cliproxyapi::internal::auth::codex::{
        CodexSecretStore, CodexStoredCredentials, SecretString,
    };
    proxy::CtoxCodexSecretStore::new(f.root.path()).store_credentials(
        &account
            .credential_handles()
            .map_err(|_| anyhow::anyhow!("fixture handles"))?,
        &CodexStoredCredentials::new(
            SecretString::new(format!("id-{marker}"))?,
            SecretString::new(format!("access-{marker}"))?,
            SecretString::new(format!("refresh-{marker}"))?,
        ),
    )?;
    let stored = proxy::load_instance_proxy_config(f.root.path())?;
    let revision = stored.as_ref().map_or(0, |v| v.revision);
    let mut runtime = stored
        .map(|v| v.runtime)
        .unwrap_or(serde_json::from_value::<CliproxyRuntimeConfig>(json!({}))?);
    runtime.codex_accounts.retain(|a| a.id != id);
    runtime.codex_accounts.push(account);
    proxy::save_instance_proxy_config(f.root.path(), revision, "codex", runtime)?;
    Ok(())
}

fn adopt_native(f: &Fixture) -> Result<Value> {
    let mut cmd = command("ctox.workjet.providers.adopt_native", json!({}));
    cmd.id = Some(uuid::Uuid::new_v4().to_string());
    let claim = store::business_command_core_claim(cmd.id.as_deref().unwrap(), &cmd)?;
    let admission = DomainEffectAdmission::newly_claimed(
        cmd.id.as_deref().unwrap(),
        &claim.payload_hash,
        "owner",
    )?;
    crate::channels::claim_business_control_command(f.root.path(), claim)?;
    handle_command(f.root.path(), &cmd, "owner", Some(&admission))
}
fn codex_fixture() -> Result<(Fixture, String)> {
    let f = Fixture::new()?;
    store::stable_instance_id(f.root.path())?;
    install(&f, "owned-codex-a", "first")?;
    install(&f, "owned-codex-b", "sibling")?;
    let state = adopt_native(&f)?;
    assert_eq!(state["accounts"].as_array().unwrap().len(), 2);
    // Public rows intentionally cannot disclose the holder-local selector.
    let id = f.conn.query_row("SELECT account_id FROM business_provider_federation_accounts WHERE private_local_account_id='owned-codex-a'", [], |r| r.get::<_,String>(0))?;
    Ok((f, id))
}

#[test]
fn native_controls_codex_disable_enable_remove_keeps_sibling_default_and_private_secrets(
) -> Result<()> {
    let (f, id) = codex_fixture()?;
    let initial = list(&f.conn, "owner")?;
    assert!(initial["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["controls"] == json!({"canEnable":true,"canRemove":true})));
    let (disable, admission, _) = admitted(&f, &id, Some(false), "owner")?;
    let disabled = handle(f.root.path(), &disable, "owner", Some(&admission))?;
    assert_eq!(
        disabled["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()["enabled"],
        false
    );
    let stored = proxy::load_instance_proxy_config(f.root.path())?.unwrap();
    assert_eq!(stored.default_provider, "codex");
    assert!(
        stored
            .runtime
            .codex_accounts
            .iter()
            .find(|a| a.id == "owned-codex-a")
            .unwrap()
            .disabled
    );
    let refreshed = adopt_native(&f)?;
    assert_eq!(refreshed["revision"], disabled["revision"]);
    let (enable, admission, _) = admitted(&f, &id, Some(true), "owner")?;
    handle(f.root.path(), &enable, "owner", Some(&admission))?;
    let (remove, admission, hash) = admitted(&f, &id, None, "owner")?;
    let removed = handle(f.root.path(), &remove, "owner", Some(&admission))?;
    assert_eq!(removed["accounts"].as_array().unwrap().len(), 1);
    assert!(holder::account_binding(f.root.path(), "codex", "owned-codex-a")?.is_none());
    assert!(holder::account_binding(f.root.path(), "codex", "owned-codex-b")?.is_some());
    for suffix in ["id", "access", "refresh"] {
        assert!(crate::secrets::secret_record_content_version(
            f.root.path(),
            "provider-subscriptions",
            &format!("owned-codex-a-{suffix}")
        )?
        .is_none());
        assert!(crate::secrets::secret_record_content_version(
            f.root.path(),
            "provider-subscriptions",
            &format!("owned-codex-b-{suffix}")
        )?
        .is_some());
    }
    assert_eq!(
        recover(f.root.path(), &remove, &hash, "owner")?,
        Some(removed.clone())
    );
    let public = removed.to_string();
    for private in [
        "owned-codex",
        "access-sibling",
        "id-sibling",
        "provider-subscriptions",
        "before_binding",
    ] {
        assert!(!public.contains(private));
    }
    assert_eq!(
        proxy::load_instance_proxy_config(f.root.path())?
            .unwrap()
            .default_provider,
        "codex"
    );
    Ok(())
}

#[test]
fn native_controls_codex_credential_rotation_and_foreign_actor_fail_before_mutation() -> Result<()>
{
    let (f, id) = codex_fixture()?;
    let (cmd, admission, _) = admitted(&f, &id, Some(false), "foreign")?;
    assert!(handle(f.root.path(), &cmd, "foreign", Some(&admission)).is_err());
    let (cmd, admission, _) = admitted(&f, &id, Some(false), "owner")?;
    // Even a replacement with identical token bytes changes its private generation.
    crate::secrets::write_secret_record(
        f.root.path(),
        "provider-subscriptions",
        "owned-codex-a-access",
        "access-first",
        None,
        json!({"rotation":true}),
    )?;
    assert!(handle(f.root.path(), &cmd, "owner", Some(&admission)).is_err());
    assert!(
        !proxy::load_instance_proxy_config(f.root.path())?
            .unwrap()
            .runtime
            .codex_accounts
            .iter()
            .find(|a| a.id == "owned-codex-a")
            .unwrap()
            .disabled
    );
    Ok(())
}

#[test]
fn native_controls_codex_borrowed_or_aliased_credentials_never_advertise_controls() -> Result<()> {
    let (f, _) = codex_fixture()?;
    let mut stored = proxy::load_instance_proxy_config(f.root.path())?.unwrap();
    let mut borrowed = stored.runtime.codex_accounts[0].clone();
    borrowed.id = "borrowed-codex".into();
    stored.runtime.codex_accounts.push(borrowed);
    proxy::save_instance_proxy_config(f.root.path(), stored.revision, "codex", stored.runtime)?;
    assert!(holder::account_binding(f.root.path(), "codex", "borrowed-codex")?.is_none());
    assert!(holder::account_binding(f.root.path(), "codex", "owned-codex-a")?.is_none());
    let state = adopt_native(&f)?;
    for local in ["borrowed-codex", "owned-codex-a"] {
        let id=f.conn.query_row("SELECT account_id FROM business_provider_federation_accounts WHERE private_local_account_id=?1",[local],|r|r.get::<_,String>(0))?;
        assert_eq!(
            state["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == id)
                .unwrap()["controls"],
            json!({"canEnable":false,"canRemove":false})
        );
        let (cmd, admission, _) = admitted(&f, &id, None, "owner")?;
        assert!(handle(f.root.path(), &cmd, "owner", Some(&admission)).is_err());
    }
    assert!(crate::secrets::secret_record_content_version(
        f.root.path(),
        "provider-subscriptions",
        "owned-codex-a-access"
    )?
    .is_some());
    Ok(())
}

#[test]
fn native_controls_codex_old_removal_receipt_cannot_remove_recreated_generation() -> Result<()> {
    let (f, id) = codex_fixture()?;
    let (cmd, admission, hash) = admitted(&f, &id, None, "owner")?;
    f.conn.execute_batch("CREATE TRIGGER account_receipt_failure BEFORE INSERT ON business_command_domain_effects BEGIN SELECT RAISE(ABORT,'receipt unavailable'); END;")?;
    assert!(handle(f.root.path(), &cmd, "owner", Some(&admission)).is_err());
    install(&f, "owned-codex-a", "replacement")?;
    f.conn
        .execute_batch("DROP TRIGGER account_receipt_failure")?;
    assert!(recover(f.root.path(), &cmd, &hash, "owner").is_err());
    assert!(crate::secrets::secret_record_content_version(
        f.root.path(),
        "provider-subscriptions",
        "owned-codex-a-access"
    )?
    .is_some());
    Ok(())
}
