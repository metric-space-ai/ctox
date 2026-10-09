// Origin: CTOX
// License: AGPL-3.0-only

use super::super::{capability::CapabilityDeviceBinding, mobile_invites, store_workjet_computers};
use super::*;

struct Fixture {
    root: tempfile::TempDir,
    conn: Connection,
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            "owner",
            "Owner",
            "chef",
            store::now_ms() as i64,
        )?;
        let conn = store::open_store(root.path())?;
        let projections = Connection::open(store::rxdb_store_path(root.path()))?;
        projections.execute_batch(
            "CREATE TABLE ctox_business_os__workjet_computers__v0 (
            id TEXT PRIMARY KEY NOT NULL,revision TEXT,deleted INTEGER NOT NULL DEFAULT 0,
            lastWriteTime REAL NOT NULL DEFAULT 0,data TEXT NOT NULL)",
        )?;
        Ok(Self { root, conn })
    }

    fn enroll(&self, computer: &str) -> Result<()> {
        let binding = CapabilityDeviceBinding {
            device_pairing_id: format!("pairing-{computer}"),
            device_id: format!("device-{computer}"),
            proof_key_thumbprint: "A".repeat(43),
        };
        mobile_invites::create_for_owner(
            self.root.path(),
            300,
            None,
            Some(&binding),
            Some("owner"),
        )?;
        store_workjet_computers::handle_workjet_computer_store_command(
            self.root.path(),
            &command(
                "ctox.workjet.computer.assign",
                json!({
                    "computer_id":computer,"display_name":"Fixture computer",
                    "hosting_mode":"workstation","capabilities":[],
                    "device_binding_id":binding.device_pairing_id,
                }),
            ),
            "owner",
            None,
            "chef",
        )?;
        Ok(())
    }

    fn adopt(&self, accounts: &[NativeAccountObservation]) -> Result<Value> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        adopt(&tx, "owner", "native-instance", accounts, 100)?;
        let result = list(&tx, "owner")?;
        tx.commit()?;
        Ok(result)
    }

    // Policy-unit input only. Production has no ConsumerFacts-based public
    // constructor: with_consumable_account requires Architecture's sealed,
    // possession-bound transport authority and revalidates current enrollment.
    fn facts(&self, computer: &str) -> ConsumerFacts {
        ConsumerFacts {
            owner_user_id: "owner".into(),
            owner_epoch: 0,
            actor_user_id: format!("actor-{computer}"),
            actor_epoch: 0,
            computer_id: computer.into(),
            computer_revision: "revision".into(),
            pairing_id: format!("pairing-{computer}"),
            device_id: format!("device-{computer}"),
            proof_key_thumbprint: "A".repeat(43),
            pairing_revision: "witness".into(),
        }
    }
}

fn command(kind: &str, payload: Value) -> BusinessCommand {
    BusinessCommand {
        id: None,
        module: "ctox".into(),
        command_type: kind.into(),
        record_id: None,
        payload,
        client_context: json!({}),
        origin: store::CommandOrigin::TrustedLocal,
    }
}

fn account(local: &str) -> NativeAccountObservation {
    NativeAccountObservation {
        provider: "minimax".into(),
        local_account_id: local.into(),
        enabled: true,
        credential_ready: true,
    }
}

fn account_id(state: &Value) -> &str {
    state["accounts"][0]["id"].as_str().unwrap()
}

#[test]
fn inherited_main_account_retirement_preserves_identity_withdrawals_and_other_accounts(
) -> Result<()> {
    let f = Fixture::new()?;
    f.enroll("first")?;
    let mut main = account(INHERITED_NATIVE_ACCOUNT_ID);
    main.provider = "ctox_proxy".into();
    let original = f.adopt(&[main, account("private-subscription")])?;
    let entry = original["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["provider"] == "ctox_proxy")
        .unwrap();
    let id = entry["id"].as_str().unwrap().to_owned();
    withdraw(
        &f.conn,
        "owner",
        &WithdrawRequest {
            _inbound_channel: None,
            account_id: id.clone(),
            computer_id: "first".into(),
            withdrawn: true,
            expected_revision: 1,
        },
    )?;
    let before = policy_revision(&f.conn, "owner")?;
    retire_inherited_route(&f.conn, "foreign", "native-instance", None, 101)?;
    retire_inherited_route(&f.conn, "owner", "other-holder", None, 101)?;
    retire_inherited_route(&f.conn, "owner", "native-instance", Some("ctox_proxy"), 101)?;
    assert_eq!(policy_revision(&f.conn, "owner")?, before);
    retire_inherited_route(&f.conn, "owner", "native-instance", None, 102)?;
    let retired_revision = policy_revision(&f.conn, "owner")?;
    assert_eq!(retired_revision, before + 1);
    retire_inherited_route(&f.conn, "owner", "native-instance", None, 103)?;
    assert_eq!(policy_revision(&f.conn, "owner")?, retired_revision);
    assert!(consumable(&f.conn, &f.facts("second"), &id, 1).is_err());
    let retired = list(&f.conn, "owner")?;
    let main = retired["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == id)
        .unwrap();
    assert_eq!(main["enabled"], false);
    assert_eq!(main["credentialReady"], false);
    assert_eq!(main["revision"], 2);
    let subscription = retired["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["provider"] == "minimax")
        .unwrap();
    assert_eq!(subscription["enabled"], true);
    assert_eq!(subscription["credentialReady"], true);

    let mut main = account(INHERITED_NATIVE_ACCOUNT_ID);
    main.provider = "ctox_proxy".into();
    let restored = f.adopt(&[main])?;
    let restored_main = restored["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == id)
        .unwrap();
    assert_eq!(restored_main["revision"], 3);
    assert!(consumable(&f.conn, &f.facts("first"), &id, 3).is_err());
    assert!(consumable(&f.conn, &f.facts("second"), &id, 3).is_ok());
    for private in [INHERITED_NATIVE_ACCOUNT_ID, "private-subscription"] {
        assert!(!restored.to_string().contains(private));
    }
    Ok(())
}

#[test]
fn inherited_provider_switch_retires_only_the_previous_provider_and_keeps_its_uuid() -> Result<()> {
    let f = Fixture::new()?;
    let mut original = account(INHERITED_NATIVE_ACCOUNT_ID);
    original.provider = "ctox_proxy".into();
    let first = f.adopt(&[original])?;
    let original_id = account_id(&first).to_owned();
    retire_inherited_route(&f.conn, "owner", "native-instance", Some("zai"), 101)?;
    let mut current = account(INHERITED_NATIVE_ACCOUNT_ID);
    current.provider = "zai".into();
    let switched = f.adopt(&[current])?;
    let entries = switched["accounts"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    let old = entries
        .iter()
        .find(|entry| entry["provider"] == "ctox_proxy")
        .unwrap();
    let new = entries
        .iter()
        .find(|entry| entry["provider"] == "zai")
        .unwrap();
    assert_eq!(old["id"], original_id);
    assert_eq!(old["enabled"], false);
    assert_eq!(new["enabled"], true);
    assert_ne!(new["id"], original_id);
    assert!(consumable(&f.conn, &f.facts("consumer"), &original_id, 1).is_err());
    assert!(consumable(
        &f.conn,
        &f.facts("consumer"),
        new["id"].as_str().unwrap(),
        1
    )
    .is_ok());
    Ok(())
}

#[test]
fn adoption_is_stable_private_and_does_not_invent_model_or_health_proof() -> Result<()> {
    let f = Fixture::new()?;
    let trusted = observations(&json!({"accounts":[{
        "id":"private-account-selector","provider":"minimax","enabled":true,"status":"ready",
        "models":["MiniMax-M3"],"preset_ids":["ignored-preset"],"secret":"ignored-sensitive-value",
    }]}))?;
    let first = f.adopt(&trusted)?;
    let second = f.adopt(&trusted)?;
    assert_eq!(first, second);
    assert_eq!(first["revision"], 1);
    assert_eq!(first["accounts"][0]["credentialReady"], true);
    assert_eq!(first["accounts"][0]["modelCatalogObserved"], false);
    assert_eq!(first["accounts"][0]["inferenceVerified"], false);
    let serialized = first.to_string();
    for private in [
        "private-account-selector",
        "ignored-sensitive-value",
        "ignored-preset",
        "MiniMax-M3",
    ] {
        assert!(!serialized.contains(private));
    }
    assert_eq!(
        f.adopt(&[])?,
        first,
        "an absent snapshot is not account removal"
    );
    assert_eq!(list(&f.conn, "foreign")?["accounts"], json!([]));
    Ok(())
}

#[test]
fn default_consumption_survives_new_accounts_and_new_computers_without_opt_in() -> Result<()> {
    let f = Fixture::new()?;
    let first = f.adopt(&[account("held-a")])?;
    for computer in ["first", "second", "new-after-adoption"] {
        assert!(consumable(&f.conn, &f.facts(computer), account_id(&first), 1).is_ok());
    }
    f.adopt(&[account("held-a"), account("held-b")])?;
    let state = list(&f.conn, "owner")?;
    for entry in state["accounts"].as_array().unwrap() {
        for computer in ["first", "second", "new-after-adoption"] {
            let selected = consumable(
                &f.conn,
                &f.facts(computer),
                entry["id"].as_str().unwrap(),
                1,
            )?;
            assert_eq!(selected.account_id, entry["id"].as_str().unwrap());
            assert_eq!(selected.holder_instance_id, "native-instance");
            assert_eq!(selected.provider, "minimax");
            assert!(matches!(
                selected.private_local_account_id.as_str(),
                "held-a" | "held-b"
            ));
            assert_eq!(selected.account_revision, 1);
            assert_eq!(selected.policy_revision, 2);
        }
    }
    Ok(())
}

#[test]
fn explicit_withdrawal_blocks_only_the_actual_consumer_and_restoration_is_idempotent() -> Result<()>
{
    let f = Fixture::new()?;
    f.enroll("first")?;
    f.enroll("second")?;
    let state = f.adopt(&[account("held-a")])?;
    let id = account_id(&state);
    let mut request = WithdrawRequest {
        _inbound_channel: None,
        account_id: id.into(),
        computer_id: "first".into(),
        withdrawn: true,
        expected_revision: 1,
    };
    withdraw(&f.conn, "owner", &request)?;
    assert_eq!(policy_revision(&f.conn, "owner")?, 2);
    assert!(consumable(&f.conn, &f.facts("first"), id, 1).is_err());
    assert!(consumable(&f.conn, &f.facts("second"), id, 1).is_ok());
    assert!(consumable(&f.conn, &f.facts("new-after-withdrawal"), id, 1).is_ok());
    request.expected_revision = 2;
    withdraw(&f.conn, "owner", &request)?;
    assert_eq!(policy_revision(&f.conn, "owner")?, 2);
    request.withdrawn = false;
    withdraw(&f.conn, "owner", &request)?;
    assert!(consumable(&f.conn, &f.facts("first"), id, 1).is_ok());
    assert_eq!(policy_revision(&f.conn, "owner")?, 3);
    Ok(())
}

#[test]
fn stale_foreign_unassigned_or_revoked_withdrawal_targets_fail_closed() -> Result<()> {
    let f = Fixture::new()?;
    f.enroll("first")?;
    let state = f.adopt(&[account("held-a")])?;
    let mut request = WithdrawRequest {
        _inbound_channel: None,
        account_id: account_id(&state).into(),
        computer_id: "first".into(),
        withdrawn: true,
        expected_revision: 2,
    };
    assert!(withdraw(&f.conn, "owner", &request).is_err());
    request.expected_revision = 1;
    assert!(withdraw(&f.conn, "foreign", &request).is_err());
    request.computer_id = "unassigned".into();
    assert!(withdraw(&f.conn, "owner", &request).is_err());
    request.computer_id = "first".into();
    mobile_invites::revoke_by_device_pairing_id(f.root.path(), "pairing-first")?;
    assert!(withdraw(&f.conn, "owner", &request).is_err());
    assert_eq!(policy_revision(&f.conn, "owner")?, 1);
    Ok(())
}

#[test]
fn adoption_changes_fence_old_account_snapshots_without_reassigning_custody() -> Result<()> {
    let f = Fixture::new()?;
    let state = f.adopt(&[account("held-a")])?;
    let id = account_id(&state);
    let mut disabled = account("held-a");
    disabled.enabled = false;
    f.adopt(&[disabled])?;
    assert!(consumable(&f.conn, &f.facts("first"), id, 1).is_err());
    assert_eq!(policy_revision(&f.conn, "owner")?, 2);
    f.adopt(&[account("held-a")])?;
    assert!(consumable(&f.conn, &f.facts("first"), id, 1).is_err());
    assert!(consumable(&f.conn, &f.facts("first"), id, 3).is_ok());
    assert!(adopt(
        &f.conn,
        "foreign",
        "native-instance",
        &[account("held-a")],
        101
    )
    .is_err());
    assert_eq!(list(&f.conn, "owner")?["accounts"][0]["id"], id);
    let mut foreign = f.facts("first");
    foreign.owner_user_id = "foreign".into();
    assert!(consumable(&f.conn, &foreign, id, 3).is_err());
    Ok(())
}

#[test]
fn metadata_commands_reject_caller_identity_secret_models_and_missing_admission() -> Result<()> {
    let f = Fixture::new()?;
    for payload in [
        json!({"owner_user_id":"owner"}),
        json!({"models":["MiniMax-M3"]}),
        json!({"secret":"ignored-sensitive-value"}),
        json!({"holder":"other-computer"}),
    ] {
        assert!(handle_command(
            f.root.path(),
            &command("ctox.workjet.providers.adopt_native", payload),
            "owner",
            None
        )
        .is_err());
    }
    assert!(handle_command(
        f.root.path(),
        &command("ctox.workjet.providers.adopt_native", json!({})),
        "owner",
        None
    )
    .is_err());
    assert!(handle_command(
        f.root.path(),
        &command("ctox.workjet.providers.list", json!({})),
        "foreign",
        None
    )
    .is_err());
    f.conn.execute(
        "UPDATE business_users SET active=0 WHERE user_id='owner'",
        [],
    )?;
    assert!(handle_command(
        f.root.path(),
        &command("ctox.workjet.providers.list", json!({})),
        "owner",
        None
    )
    .is_err());
    Ok(())
}

#[test]
fn bounded_native_observations_reject_duplicates_and_control_characters() -> Result<()> {
    assert!(observations(&json!({"accounts":[
        {"id":"held-a","provider":"minimax"},{"id":"held-a","provider":"minimax"}
    ]}))
    .is_err());
    assert!(
        observations(&json!({"accounts":[{"id":"held\naccount","provider":"minimax"}]})).is_err()
    );
    assert!(observations(
        &json!({"accounts":vec![json!({"id":"held-a","provider":"minimax"}); MAX_ACCOUNTS+1]})
    )
    .is_err());
    Ok(())
}

#[test]
fn policy_mutation_and_replay_receipt_share_one_transaction() -> Result<()> {
    let f = Fixture::new()?;
    let admission = DomainEffectAdmission::newly_claimed("fixture-command", "intent", "owner")?;
    let mut conn = store::open_store(f.root.path())?;
    let apply = |tx: &rusqlite::Transaction<'_>| -> Result<AppliedDomainEffect> {
        adopt(tx, "owner", "native-instance", &[account("held-a")], 100)?;
        Ok(AppliedDomainEffect {
            result: list(tx, "owner")?,
            projections: vec![],
        })
    };
    let first = admission.apply(&mut conn, apply)?;
    let second = admission.apply(&mut conn, |_| {
        anyhow::bail!("replay must not invoke effect")
    })?;
    assert_eq!(first, second);
    let rejected = DomainEffectAdmission::newly_claimed("rejected-command", "intent2", "owner")?;
    assert!(rejected
        .apply(&mut conn, |tx| {
            adopt(tx, "owner", "native-instance", &[account("held-b")], 101)?;
            anyhow::bail!("effect fails before commit")
        })
        .is_err());
    assert_eq!(
        list(&conn, "owner")?["accounts"].as_array().unwrap().len(),
        1
    );
    assert_eq!(policy_revision(&conn, "owner")?, 1);
    Ok(())
}
