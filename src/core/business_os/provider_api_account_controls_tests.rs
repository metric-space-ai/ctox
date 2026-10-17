// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::execution::cliproxyapi_integration::{
    load_provider_integration_config, save_provider_integration_config, ApiKeyAccountConfig,
    ApiKeyEndpoint, ProviderSecretRef,
};
fn api_fixture(
    root: std::sync::Arc<tempfile::TempDir>,
    endpoint: ApiKeyEndpoint,
) -> Result<(Fixture, String)> {
    let f = Fixture::with_root(root)?;
    let mut config = load_provider_integration_config(f.root.path())?;
    for local in ["private-api-one", "private-api-two"] {
        let reference = ProviderSecretRef::new("provider-api-keys", format!("{local}-key"));
        crate::secrets::write_secret_record(
            f.root.path(),
            &reference.scope,
            &reference.name,
            "synthetic-api-control-key",
            None,
            json!({}),
        )?;
        config.config.api_key_accounts.push(ApiKeyAccountConfig {
            id: local.into(),
            endpoint,
            api_key_secret: reference,
            owner_user_id: Some("owner".into()),
            creation_operation: Some(uuid::Uuid::new_v4().to_string()),
            disabled: false,
            priority: 0,
            models: Vec::new(),
        });
    }
    save_provider_integration_config(f.root.path(), config.revision, &config.config)?;
    store::stable_instance_id(f.root.path())?;
    refresh(&f).context("initial API fixture adoption")?;
    let id=f.conn.query_row("SELECT account_id FROM business_provider_federation_accounts WHERE private_local_account_id='private-api-one'",[],|r|r.get(0))?;
    Ok((f, id))
}
fn refresh(f: &Fixture) -> Result<Value> {
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
fn reserve(f: &Fixture, cmd: &BusinessCommand, hash: &str) -> Result<Pending> {
    let request: Request = serde_json::from_value(cmd.payload.clone())?;
    let pending = Pending {
        target: target(
            &f.conn,
            "owner",
            &store::existing_instance_id(f.root.path())?,
            &request,
        )?,
        request,
    };
    f.conn.execute("INSERT INTO business_provider_account_controls(command_id,payload_hash,actor_user_id,owner_user_id,pending_json) VALUES (?1,?2,?3,?3,?4)",params![cmd.id,hash,"owner",serde_json::to_string(&pending)?])?;
    Ok(pending)
}
#[test]
fn api_controls_admitted_toggle_and_remove_preserve_other_accounts_and_no_key_rotation(
) -> Result<()> {
    for endpoint in [
        ApiKeyEndpoint::Xai,
        ApiKeyEndpoint::KimiCoding,
        ApiKeyEndpoint::ZaiPlatform,
        ApiKeyEndpoint::MiniMax,
    ] {
        crate::persistence::test_support::run(move |owner_root| {
            let (f, id) = api_fixture(owner_root, endpoint)?;
            let before = crate::secrets::secret_record_content_version(
                f.root.path(),
                "provider-api-keys",
                "private-api-one-key",
            )?;
            for enabled in [false, true] {
                let (cmd, admission, _) = admitted(&f, &id, Some(enabled), "owner")?;
                let result = handle(f.root.path(), &cmd, "owner", Some(&admission))?;
                let row = result["accounts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["id"] == id)
                    .unwrap();
                assert_eq!(row["enabled"], enabled);
                assert_eq!(row["credentialReady"], true);
                assert_eq!(row["controls"], json!({"canEnable":true,"canRemove":true}));
                let refreshed = refresh(&f)?;
                assert_eq!(refreshed["revision"], result["revision"]);
                assert_eq!(
                    crate::secrets::secret_record_content_version(
                        f.root.path(),
                        "provider-api-keys",
                        "private-api-one-key"
                    )?,
                    before
                );
            }
            let config = load_provider_integration_config(f.root.path())?;
            let (cmd, admission, hash) = admitted(&f, &id, None, "owner")?;
            let result = handle(f.root.path(), &cmd, "owner", Some(&admission))?;
            assert_eq!(result["accounts"].as_array().unwrap().len(), 1);
            let current = load_provider_integration_config(f.root.path())?;
            assert_eq!(current.config.api_key_accounts.len(), 1);
            assert_eq!(current.config.api_key_accounts[0].id, "private-api-two");
            assert!(crate::secrets::secret_record_content_version(
                f.root.path(),
                "provider-api-keys",
                "private-api-one-key"
            )?
            .is_none());
            assert!(crate::secrets::secret_record_content_version(
                f.root.path(),
                "provider-api-keys",
                "private-api-two-key"
            )?
            .is_some());
            assert_eq!(
                recover(f.root.path(), &cmd, &hash, "owner")?,
                Some(result.clone())
            );
            assert!(save_provider_integration_config(
                f.root.path(),
                current.revision,
                &config.config
            )
            .is_err());
            for private in [
                "synthetic-api-control-key",
                "private-api-one",
                "api-key:v1:",
            ] {
                assert!(!result.to_string().contains(private));
            }
            Ok(())
        })?;
    }
    Ok(())
}
#[test]
fn api_controls_reject_foreign_and_changed_credentials_before_any_runtime_effect() -> Result<()> {
    crate::persistence::test_support::run(move |owner_root| {
        let (f, id) = api_fixture(owner_root, ApiKeyEndpoint::Xai)?;
        let before = load_provider_integration_config(f.root.path())?;
        let (cmd, admission, _) = admitted(&f, &id, Some(false), "foreign")?;
        assert!(handle(f.root.path(), &cmd, "foreign", Some(&admission)).is_err());
        assert_eq!(load_provider_integration_config(f.root.path())?, before);
        crate::secrets::write_secret_record(
            f.root.path(),
            "provider-api-keys",
            "private-api-one-key",
            "synthetic-replacement",
            None,
            json!({}),
        )?;
        let (cmd, admission, _) = admitted(&f, &id, None, "owner")?;
        assert!(handle(f.root.path(), &cmd, "owner", Some(&admission)).is_err());
        assert!(!holder::has_effect(
            f.root.path(),
            cmd.id.as_deref().unwrap()
        )?);
        assert_eq!(load_provider_integration_config(f.root.path())?, before);
        Ok(())
    })
}
#[test]
fn api_removal_missing_secret_can_complete_but_never_deletes_a_late_replacement() -> Result<()> {
    for replace in [false, true] {
        crate::persistence::test_support::run(move |owner_root| {
            let (f, id) = api_fixture(owner_root, ApiKeyEndpoint::Xai)?;
            let version = crate::secrets::secret_record_content_version(
                f.root.path(),
                "provider-api-keys",
                "private-api-one-key",
            )?
            .unwrap();
            crate::secrets::delete_secret_records_if_versions(
                f.root.path(),
                &[(
                    "provider-api-keys".into(),
                    "private-api-one-key".into(),
                    version,
                )],
            )?;
            refresh(&f)?;
            let (cmd, _, hash) = admitted(&f, &id, None, "owner")?;
            let pending = reserve(&f, &cmd, &hash)?;
            execute_reserved(f.root.path(), &cmd, &hash, "owner", &pending)?;
            assert!(holder::has_effect(
                f.root.path(),
                cmd.id.as_deref().unwrap()
            )?);
            if replace {
                crate::secrets::write_secret_record(
                    f.root.path(),
                    "provider-api-keys",
                    "private-api-one-key",
                    "synthetic-late-replacement",
                    None,
                    json!({}),
                )?;
            }
            let result = recover(f.root.path(), &cmd, &hash, "owner");
            assert_eq!(result.is_ok(), !replace);
            if replace {
                assert_eq!(
                    crate::secrets::read_secret_value(
                        f.root.path(),
                        "provider-api-keys",
                        "private-api-one-key"
                    )?,
                    "synthetic-late-replacement"
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}
#[test]
fn api_removal_interruption_recovers_exact_proof_and_refuses_replaced_generation() -> Result<()> {
    for replace in [false, true] {
        crate::persistence::test_support::run(move |owner_root| {
            let (f, id) = api_fixture(owner_root, ApiKeyEndpoint::KimiPlatformGlobal)?;
            let (cmd, _, hash) = admitted(&f, &id, None, "owner")?;
            let pending = reserve(&f, &cmd, &hash)?;
            execute_reserved(f.root.path(), &cmd, &hash, "owner", &pending)?;
            assert!(holder::has_effect(
                f.root.path(),
                cmd.id.as_deref().unwrap()
            )?);
            assert_eq!(
                load_provider_integration_config(f.root.path())?
                    .config
                    .api_key_accounts
                    .len(),
                1
            );
            if replace {
                crate::secrets::write_secret_record(
                    f.root.path(),
                    "provider-api-keys",
                    "private-api-one-key",
                    "synthetic-new-generation",
                    None,
                    json!({}),
                )?;
            }
            assert_eq!(
                recover(f.root.path(), &cmd, &hash, "owner").is_ok(),
                !replace
            );
            if replace {
                assert_eq!(
                    crate::secrets::read_secret_value(
                        f.root.path(),
                        "provider-api-keys",
                        "private-api-one-key"
                    )?,
                    "synthetic-new-generation"
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}
