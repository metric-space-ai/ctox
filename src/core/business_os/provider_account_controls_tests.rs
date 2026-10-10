// Origin: CTOX
// License: AGPL-3.0-only
use super::super::tests::{account_id, command, Fixture};
use super::*;
use crate::execution::{cliproxyapi_claude_catalog as catalog, cliproxyapi_host as proxy};
use ctox_cliproxyapi::internal::auth::claude::{ClaudeStoredCredentials, SecretString};

fn credentials(marker: &str) -> ClaudeStoredCredentials {
    ClaudeStoredCredentials::new(
        SecretString::new(format!("access-{marker}")).unwrap(),
        SecretString::new(format!("refresh-{marker}")).unwrap(),
    )
}
fn fixture() -> Result<(Fixture, String)> {
    let f = Fixture::new()?;
    proxy::install_claude_subscription(
        f.root.path(),
        "local-claude-control",
        &credentials("first"),
    )?;
    proxy::install_claude_subscription(f.root.path(), "local-claude-other", &credentials("other"))?;
    let holder = store::existing_instance_id(f.root.path())?;
    let mut conn = store::open_store(f.root.path())?;
    let tx = conn.transaction()?;
    let observations =
        ["local-claude-control", "local-claude-other"].map(|local| NativeAccountObservation {
            provider: "claude".into(),
            local_account_id: local.into(),
            enabled: true,
            credential_ready: true,
            private_binding: catalog::account_binding(f.root.path(), local).unwrap(),
        });
    adopt(&tx, "owner", &holder, &observations, store::now_ms() as i64)?;
    tx.commit()?;
    let id=f.conn.query_row("SELECT account_id FROM business_provider_federation_accounts WHERE private_local_account_id='local-claude-control'",[],|r|r.get(0))?;
    Ok((f, id))
}
fn admitted(
    f: &Fixture,
    id: &str,
    enabled: Option<bool>,
    actor: &str,
) -> Result<(BusinessCommand, DomainEffectAdmission, String)> {
    let state = list(&f.conn, "owner")?;
    let rev = f.conn.query_row(
        "SELECT revision FROM business_provider_federation_accounts WHERE account_id=?1",
        [id],
        |r| r.get::<_, i64>(0),
    )?;
    let mut payload = json!({"account_id":id,"expected_account_revision":rev,"expected_revision":state["revision"]});
    let kind = if let Some(enabled) = enabled {
        payload["enabled"] = json!(enabled);
        "ctox.workjet.providers.account.enable"
    } else {
        "ctox.workjet.providers.account.remove"
    };
    let mut cmd = command(kind, payload);
    cmd.id = Some(uuid::Uuid::new_v4().to_string());
    let claim = store::business_command_core_claim(cmd.id.as_deref().unwrap(), &cmd)?;
    let hash = claim.payload_hash.clone();
    crate::channels::claim_business_control_command(f.root.path(), claim)?;
    let admission = DomainEffectAdmission::newly_claimed(cmd.id.as_deref().unwrap(), &hash, actor)?;
    Ok((cmd, admission, hash))
}
#[test]
fn native_controls_persist_disable_enable_and_remove_exact_account() -> Result<()> {
    let (f, id) = fixture()?;
    let (cmd, admission, _) = admitted(&f, &id, Some(false), "owner")?;
    let disabled = handle(f.root.path(), &cmd, "owner", Some(&admission))?;
    let row = disabled["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap();
    assert_eq!(row["enabled"], false);
    assert_eq!(row["controls"], json!({"canEnable":true,"canRemove":true}));
    let runtime = proxy::load_instance_proxy_config(f.root.path())?.unwrap();
    assert!(
        runtime
            .runtime
            .claude_accounts
            .iter()
            .find(|a| a.id == "local-claude-control")
            .unwrap()
            .disabled
    );
    // Adoption refresh uses persisted topology, not the pre-control snapshot.
    let mut adopt_cmd = command("ctox.workjet.providers.adopt_native", json!({}));
    adopt_cmd.id = Some(uuid::Uuid::new_v4().to_string());
    let adopt_claim =
        store::business_command_core_claim(adopt_cmd.id.as_deref().unwrap(), &adopt_cmd)?;
    let adopt_admission = DomainEffectAdmission::newly_claimed(
        adopt_cmd.id.as_deref().unwrap(),
        &adopt_claim.payload_hash,
        "owner",
    )?;
    crate::channels::claim_business_control_command(f.root.path(), adopt_claim)?;
    let refreshed = handle_command(f.root.path(), &adopt_cmd, "owner", Some(&adopt_admission))?;
    let refreshed_row = refreshed["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap();
    assert_eq!(refreshed_row["enabled"], false);
    assert_eq!(refreshed_row["revision"], row["revision"]);
    assert_eq!(refreshed["revision"], disabled["revision"]);
    let (cmd, admission, _) = admitted(&f, &id, Some(true), "owner")?;
    handle(f.root.path(), &cmd, "owner", Some(&admission))?;
    assert!(
        !proxy::load_instance_proxy_config(f.root.path())?
            .unwrap()
            .runtime
            .claude_accounts
            .iter()
            .find(|a| a.id == "local-claude-control")
            .unwrap()
            .disabled
    );
    f.conn.execute(
        "INSERT INTO business_provider_federation_model_exclusions VALUES (?1,'[]')",
        [&id],
    )?;
    let (cmd, admission, hash) = admitted(&f, &id, None, "owner")?;
    let removed = handle(f.root.path(), &cmd, "owner", Some(&admission))?;
    assert_eq!(removed["accounts"].as_array().unwrap().len(), 1);
    assert_ne!(account_id(&removed), id);
    assert!(catalog::capture(f.root.path(), "local-claude-control")?.is_none());
    assert!(catalog::capture(f.root.path(), "local-claude-other")?.is_some());
    assert!(crate::secrets::secret_record_content_version(
        f.root.path(),
        "provider-subscriptions",
        "local-claude-control-access-token"
    )?
    .is_none());
    assert_eq!(
        recover(f.root.path(), &cmd, &hash, "owner")?,
        Some(removed.clone())
    );
    let public = serde_json::to_string(&removed)?;
    assert!(
        !public.contains("local-claude")
            && !public.contains("access-first")
            && !public.contains("before_binding")
    );
    Ok(())
}
#[test]
fn native_controls_reject_stale_foreign_inherited_and_relogin_generations() -> Result<()> {
    let (f, id) = fixture()?;
    let (mut cmd, admission, _) = admitted(&f, &id, Some(false), "owner")?;
    cmd.payload["expected_account_revision"] = json!(999);
    assert!(handle(f.root.path(), &cmd, "owner", Some(&admission)).is_err());
    let (cmd, admission, _) = admitted(&f, &id, Some(false), "foreign")?;
    assert!(handle(f.root.path(), &cmd, "foreign", Some(&admission)).is_err());
    let (cmd, admission, _) = admitted(&f, &id, Some(false), "owner")?;
    proxy::install_claude_subscription(
        f.root.path(),
        "local-claude-control",
        &credentials("relogin"),
    )?;
    assert!(handle(f.root.path(), &cmd, "owner", Some(&admission)).is_err());
    assert!(
        !proxy::load_instance_proxy_config(f.root.path())?
            .unwrap()
            .runtime
            .claude_accounts
            .iter()
            .find(|a| a.id == "local-claude-control")
            .unwrap()
            .disabled
    );
    let mut inherited = super::super::tests::account(INHERITED_NATIVE_ACCOUNT_ID);
    inherited.provider = "claude".into();
    inherited.private_binding = Some("inherited-binding".into());
    let state = f.adopt(&[inherited])?;
    let inherited_id = state["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["controls"]["canEnable"] == false)
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let request = Request {
        _inbound_channel: None,
        account_id: inherited_id.into(),
        expected_account_revision: 1,
        expected_revision: state["revision"].as_i64().unwrap(),
        enabled: Some(false),
    };
    assert!(target(&f.conn, "owner", "native-instance", &request).is_err());
    Ok(())
}
#[test]
fn native_controls_recover_after_runtime_and_secrets_commit_without_reapplying() -> Result<()> {
    let (f, id) = fixture()?;
    let (cmd, admission, hash) = admitted(&f, &id, None, "owner")?;
    f.conn.execute_batch("CREATE TRIGGER account_receipt_failure BEFORE INSERT ON business_command_domain_effects BEGIN SELECT RAISE(ABORT,'receipt unavailable'); END;")?;
    assert!(handle(f.root.path(), &cmd, "owner", Some(&admission)).is_err());
    assert!(holder::has_effect(
        f.root.path(),
        cmd.id.as_deref().unwrap()
    )?);
    assert!(catalog::capture(f.root.path(), "local-claude-control")?.is_none());
    assert!(crate::secrets::secret_record_content_version(
        f.root.path(),
        "provider-subscriptions",
        "local-claude-control-access-token"
    )?
    .is_none());
    let revision = proxy::load_instance_proxy_config(f.root.path())?
        .unwrap()
        .revision;
    assert!(super::super::super::domain_effect::identity_at_root(
        f.root.path(),
        &f.conn,
        cmd.id.as_deref().unwrap()
    )?
    .is_some());
    f.conn
        .execute_batch("DROP TRIGGER account_receipt_failure")?;
    assert!(recover(f.root.path(), &cmd, &hash, "owner")?.is_some());
    assert_eq!(
        revision,
        proxy::load_instance_proxy_config(f.root.path())?
            .unwrap()
            .revision
    );
    assert!(recover(f.root.path(), &cmd, "forged-hash", "owner").is_err());
    Ok(())
}
#[test]
fn native_controls_never_delete_relogin_after_topology_commit() -> Result<()> {
    let (f, id) = fixture()?;
    let (cmd, admission, hash) = admitted(&f, &id, None, "owner")?;
    f.conn.execute_batch("CREATE TRIGGER account_receipt_failure BEFORE INSERT ON business_command_domain_effects BEGIN SELECT RAISE(ABORT,'receipt unavailable'); END;")?;
    assert!(handle(f.root.path(), &cmd, "owner", Some(&admission)).is_err());
    proxy::install_claude_subscription(
        f.root.path(),
        "local-claude-control",
        &credentials("replacement"),
    )?;
    f.conn
        .execute_batch("DROP TRIGGER account_receipt_failure")?;
    assert!(recover(f.root.path(), &cmd, &hash, "owner").is_err());
    assert!(catalog::capture(f.root.path(), "local-claude-control")?.is_some());
    Ok(())
}
#[test]
fn native_controls_canceled_or_revoked_after_reservation_never_mutate_holder() -> Result<()> {
    for cancel in [true, false] {
        let (f, id) = fixture()?;
        let (cmd, _, hash) = admitted(&f, &id, Some(false), "owner")?;
        let request: Request = serde_json::from_value(cmd.payload.clone())?;
        let holder_id = store::existing_instance_id(f.root.path())?;
        let pending = Pending {
            target: target(&f.conn, "owner", &holder_id, &request)?,
            request,
        };
        f.conn.execute("INSERT INTO business_provider_account_controls(command_id,payload_hash,actor_user_id,owner_user_id,pending_json) VALUES (?1,?2,?3,?3,?4)", params![cmd.id, hash, "owner", serde_json::to_string(&pending)?])?;
        let revision = proxy::load_instance_proxy_config(f.root.path())?
            .unwrap()
            .revision;
        if cancel {
            crate::channels::complete_business_control_command(
                f.root.path(),
                cmd.id.as_deref().unwrap(),
                "cancelled",
                &json!({"canceled":true}),
                None,
            )?;
        } else {
            f.conn.execute(
                "UPDATE business_users SET active=0 WHERE user_id=?1",
                ["owner"],
            )?;
        }
        assert!(execute_reserved(f.root.path(), &cmd, &hash, "owner", &pending).is_err());
        assert!(!holder::has_effect(
            f.root.path(),
            cmd.id.as_deref().unwrap()
        )?);
        assert_eq!(
            revision,
            proxy::load_instance_proxy_config(f.root.path())?
                .unwrap()
                .revision
        );
        assert!(catalog::capture(f.root.path(), "local-claude-control")?.is_some());
    }
    Ok(())
}

#[test]
fn native_controls_secret_cleanup_is_generation_checked_and_atomic() -> Result<()> {
    let root = tempfile::tempdir()?;
    proxy::install_claude_subscription(root.path(), "guarded-control", &credentials("old"))?;
    let keys = [
        "guarded-control-access-token",
        "guarded-control-refresh-token",
    ]
    .map(|name| {
        (
            "provider-subscriptions".to_string(),
            name.to_string(),
            crate::secrets::secret_record_content_version(
                root.path(),
                "provider-subscriptions",
                name,
            )
            .unwrap()
            .unwrap(),
        )
    });
    proxy::install_claude_subscription(root.path(), "guarded-control", &credentials("new"))?;
    assert!(crate::secrets::delete_secret_records_if_versions(root.path(), &keys).is_err());
    for name in [
        "guarded-control-access-token",
        "guarded-control-refresh-token",
    ] {
        assert!(crate::secrets::secret_record_content_version(
            root.path(),
            "provider-subscriptions",
            name
        )?
        .is_some());
    }
    Ok(())
}
