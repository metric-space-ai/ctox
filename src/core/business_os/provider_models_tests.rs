// Origin: CTOX
// License: AGPL-3.0-only

use super::super::tests::{account, account_id, command, Fixture};
use super::*;

// These exact IDs were returned by the authenticated llm.ctox.dev catalog,
// retained in Models evidence llm-public-catalog-20261009T084220Z.json.
// Tests omit a real ID from an account list rather than inventing fake models.
pub(super) const CURRENT: &str = "MiniMax-M3";
pub(super) const OTHER: &str = "MiniMax-M2.7";

pub(super) fn observe(
    f: &Fixture,
    id: &str,
    revision: i64,
    models: &[&str],
    time: i64,
) -> Result<()> {
    retain_catalog_observation(
        &f.conn,
        &ObserveNativeRequest {
            _inbound_channel: None,
            account_id: id.into(),
            expected_account_revision: revision,
        },
        &crate::coding_agents::pi_sidecar::NativeModelCatalogObservation {
            provider: "minimax".into(),
            checked_at_ms: time,
            models: Some(models.iter().map(|s| (*s).into()).collect()),
            http_status: Some(200),
            elapsed_ms: 10,
            retry_after_seconds: None,
            failure: None,
            private_binding: None,
            inherited_selected_model: None,
        },
    )
}

pub(super) fn select_models(f: &Fixture, models: &[&str], now: i64) -> Result<()> {
    select(
        &f.conn,
        "owner",
        &SelectRequest {
            _inbound_channel: None,
            provider: "minimax".into(),
            models: models.iter().map(|s| (*s).into()).collect(),
            expected_revision: policy_revision(&f.conn, "owner")?,
        },
        now,
    )
}

fn exclude_models(f: &Fixture, id: &str, models: &[&str], revision: i64, now: i64) -> Result<()> {
    exclude(
        &f.conn,
        "owner",
        &ExcludeRequest {
            _inbound_channel: None,
            account_id: id.into(),
            models: models.iter().map(|s| (*s).into()).collect(),
            expected_account_revision: revision,
            expected_revision: policy_revision(&f.conn, "owner")?,
        },
        now,
    )
}

fn id_for(local: &str, f: &Fixture) -> Result<String> {
    // Logical identity is private-source bound, not label order.
    f.conn.query_row("SELECT account_id FROM business_provider_federation_accounts WHERE private_local_account_id=?1",
        [local],|row| row.get(0)).map_err(Into::into)
}

#[test]
fn shared_provider_selection_is_intersected_per_account_and_inherited_by_later_accounts(
) -> Result<()> {
    let f = Fixture::new()?;
    f.adopt(&[account("first"), account("second")])?;
    let first = id_for("first", &f)?;
    let second = id_for("second", &f)?;
    observe(&f, &first, 1, &[CURRENT, OTHER], 100)?;
    observe(&f, &second, 1, &[CURRENT], 100)?;
    assert_eq!(
        list(&f.conn, "owner")?["providers"][0]["selection"],
        Value::Null
    );
    select_models(&f, &[CURRENT, OTHER], 101)?;
    exclude_models(&f, &first, &[CURRENT], 1, 101)?;
    let state = list(&f.conn, "owner")?;
    let rows = state["accounts"].as_array().unwrap();
    let row = |id: &str| rows.iter().find(|row| row["id"] == id).unwrap();
    assert_eq!(row(&first)["effectiveModels"], json!([OTHER]));
    assert_eq!(row(&second)["effectiveModels"], json!([CURRENT]));
    assert_eq!(row(&first)["excludedModels"], json!([CURRENT]));
    assert_eq!(row(&second)["excludedModels"], json!([]));
    f.adopt(&[account("later")])?;
    let later_id = id_for("later", &f)?;
    observe(&f, &later_id, 1, &[OTHER], 102)?;
    let state = list(&f.conn, "owner")?;
    let later_row = state["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == later_id)
        .unwrap();
    assert_eq!(later_row["effectiveModels"], json!([OTHER]));
    assert_eq!(state["providers"].as_array().unwrap().len(), 1);
    assert_eq!(state["providers"][0]["selection"], json!([OTHER, CURRENT]));
    assert!(!state.to_string().contains("private_local_account_id"));
    Ok(())
}

#[test]
fn only_new_choices_require_fresh_enabled_live_discovery_and_removal_works_during_outage(
) -> Result<()> {
    let f = Fixture::new()?;
    let state = f.adopt(&[account("first")])?;
    let first = account_id(&state).to_owned();
    assert!(select_models(&f, &[CURRENT], 101).is_err());
    observe(&f, &first, 1, &[CURRENT], 100)?;
    assert!(select_models(&f, &[OTHER], 101).is_err());
    select_models(&f, &[CURRENT], 101)?;
    let revision = policy_revision(&f.conn, "owner")?;
    assert!(select_models(&f, &[CURRENT, OTHER], 100 + CATALOG_FRESHNESS_MS + 1).is_err());
    assert_eq!(policy_revision(&f.conn, "owner")?, revision);
    select_models(&f, &[], 100 + CATALOG_FRESHNESS_MS + 1)?;
    assert_eq!(
        selection(&f.conn, "owner", "minimax")?,
        Some(BTreeSet::new())
    );
    let before = policy_revision(&f.conn, "owner")?;
    select_models(&f, &[], 100 + CATALOG_FRESHNESS_MS + 1)?;
    assert_eq!(policy_revision(&f.conn, "owner")?, before);

    let mut disabled = account("second");
    disabled.enabled = false;
    f.adopt(&[disabled])?;
    let second = id_for("second", &f)?;
    observe(&f, &second, 1, &[OTHER], 200)?;
    assert!(select_models(&f, &[OTHER], 201).is_err());
    Ok(())
}

#[test]
fn selections_and_exclusions_are_owner_and_revision_bound() -> Result<()> {
    let f = Fixture::new()?;
    let state = f.adopt(&[account("first")])?;
    let first = account_id(&state).to_owned();
    observe(&f, &first, 1, &[CURRENT], 100)?;
    let desired = SelectRequest {
        _inbound_channel: None,
        provider: "minimax".into(),
        models: vec![CURRENT.into()],
        expected_revision: policy_revision(&f.conn, "owner")?,
    };
    assert!(select(&f.conn, "foreign", &desired, 101).is_err());
    let stale = SelectRequest {
        expected_revision: desired.expected_revision + 1,
        ..desired
    };
    assert!(select(&f.conn, "owner", &stale, 101).is_err());
    select_models(&f, &[CURRENT], 101)?;
    let desired = ExcludeRequest {
        _inbound_channel: None,
        account_id: first.clone(),
        models: vec![CURRENT.into()],
        expected_account_revision: 1,
        expected_revision: policy_revision(&f.conn, "owner")?,
    };
    assert!(exclude(&f.conn, "foreign", &desired, 101).is_err());
    let stale = ExcludeRequest {
        expected_account_revision: 2,
        ..desired
    };
    assert!(exclude(&f.conn, "owner", &stale, 101).is_err());
    assert!(exclude_models(&f, &first, &[OTHER], 1, 101).is_err());
    assert_eq!(exclusions(&f.conn, &first)?, BTreeSet::new());
    exclude_models(&f, &first, &[CURRENT], 1, 101)?;
    exclude_models(&f, &first, &[], 1, 100 + CATALOG_FRESHNESS_MS + 1)?;
    assert_eq!(exclusions(&f.conn, &first)?, BTreeSet::new());
    assert_eq!(list(&f.conn, "owner")?["accounts"][0]["enabled"], true);
    assert_eq!(list(&f.conn, "owner")?["accounts"][0]["revision"], 1);
    Ok(())
}

#[test]
fn model_eligibility_keeps_default_computer_access_and_exact_consumer_withdrawal() -> Result<()> {
    let f = Fixture::new()?;
    f.enroll("first")?;
    f.enroll("second")?;
    let state = f.adopt(&[account("native")])?;
    let id = account_id(&state).to_owned();
    observe(&f, &id, 1, &[CURRENT, OTHER], 100)?;
    assert!(consumable_model(&f.conn, &f.facts("first"), &id, 1, CURRENT, 101).is_err());
    select_models(&f, &[CURRENT], 101)?;
    for computer in ["first", "second"] {
        let bound = consumable_model(&f.conn, &f.facts(computer), &id, 1, CURRENT, 101)?;
        assert_eq!(bound.account.account_id, id);
        assert_eq!(bound.account.holder_instance_id, "native-instance");
        assert_eq!(bound.model, CURRENT);
        assert_eq!(bound.catalog_checked_at_ms, 100);
    }
    assert!(consumable_model(&f.conn, &f.facts("first"), &id, 1, OTHER, 101).is_err());
    withdraw(
        &f.conn,
        "owner",
        &WithdrawRequest {
            _inbound_channel: None,
            account_id: id.clone(),
            computer_id: "first".into(),
            withdrawn: true,
            expected_revision: policy_revision(&f.conn, "owner")?,
        },
    )?;
    assert!(consumable_model(&f.conn, &f.facts("first"), &id, 1, CURRENT, 101).is_err());
    assert!(consumable_model(&f.conn, &f.facts("second"), &id, 1, CURRENT, 101).is_ok());
    exclude_models(&f, &id, &[CURRENT], 1, 101)?;
    assert!(consumable_model(&f.conn, &f.facts("second"), &id, 1, CURRENT, 101).is_err());
    exclude_models(&f, &id, &[], 1, 101)?;
    assert!(consumable_model(&f.conn, &f.facts("second"), &id, 2, CURRENT, 101).is_err());
    let mut foreign = f.facts("second");
    foreign.owner_user_id = "foreign".into();
    assert!(consumable_model(&f.conn, &foreign, &id, 1, CURRENT, 101).is_err());
    Ok(())
}

#[test]
fn discovery_failure_or_model_disappearance_never_cools_the_whole_account() -> Result<()> {
    let f = Fixture::new()?;
    let state = f.adopt(&[account("native")])?;
    let id = account_id(&state).to_owned();
    observe(&f, &id, 1, &[CURRENT], 100)?;
    select_models(&f, &[CURRENT], 101)?;
    let failed = crate::coding_agents::pi_sidecar::NativeModelCatalogObservation {
        provider: "minimax".into(),
        checked_at_ms: 102,
        models: None,
        http_status: Some(503),
        elapsed_ms: 10,
        retry_after_seconds: None,
        failure: Some("upstream_unavailable".into()),
        private_binding: None,
        inherited_selected_model: None,
    };
    retain_catalog_observation(
        &f.conn,
        &ObserveNativeRequest {
            _inbound_channel: None,
            account_id: id.clone(),
            expected_account_revision: 1,
        },
        &failed,
    )?;
    assert!(consumable_model(&f.conn, &f.facts("first"), &id, 1, CURRENT, 103).is_err());
    let row = list(&f.conn, "owner")?["accounts"][0].clone();
    assert_eq!(row["effectiveModels"], json!([CURRENT]));
    assert_eq!(row["modelCatalog"]["fresh"], false);
    assert_eq!(row["enabled"], true);
    assert_eq!(row["revision"], 1);
    assert_eq!(row["inferenceVerified"], false);
    observe(&f, &id, 1, &[OTHER], 104)?;
    let row = list(&f.conn, "owner")?["accounts"][0].clone();
    assert_eq!(row["effectiveModels"], json!([]));
    assert_eq!(row["enabled"], true);
    assert_eq!(
        selection(&f.conn, "owner", "minimax")?,
        Some(BTreeSet::from([CURRENT.into()]))
    );
    assert!(consumable_model(&f.conn, &f.facts("first"), &id, 1, CURRENT, 105).is_err());
    Ok(())
}

#[test]
fn model_commands_never_accept_unadmitted_or_private_caller_state() -> Result<()> {
    let f = Fixture::new()?;
    for (kind, payload) in [
        (
            "ctox.workjet.providers.models.select",
            json!({"provider":"minimax","models":[CURRENT],"expected_revision":1}),
        ),
        (
            "ctox.workjet.providers.models.exclude",
            json!({"account_id":"logical","models":[CURRENT],"expected_account_revision":1,"expected_revision":1}),
        ),
    ] {
        assert!(super::super::handle_command(
            f.root.path(),
            &command(kind, payload.clone()),
            "owner",
            None
        )
        .is_err());
        let mut private = payload;
        private["key"] = json!("fixture-private");
        if kind.ends_with("select") {
            assert!(serde_json::from_value::<SelectRequest>(private).is_err());
        } else {
            assert!(serde_json::from_value::<ExcludeRequest>(private).is_err());
        }
    }
    assert_eq!(list(&f.conn, "owner")?["accounts"], json!([]));
    Ok(())
}

#[test]
fn provider_selection_and_exclusions_survive_store_reopen_without_secret_or_identity_changes(
) -> Result<()> {
    let f = Fixture::new()?;
    let state = f.adopt(&[account("native")])?;
    let id = account_id(&state).to_owned();
    observe(&f, &id, 1, &[CURRENT, OTHER], 100)?;
    select_models(&f, &[CURRENT, OTHER], 101)?;
    exclude_models(&f, &id, &[OTHER], 1, 101)?;
    let expected = list(&f.conn, "owner")?;
    let reopened = store::open_store(f.root.path())?;
    assert_eq!(list(&reopened, "owner")?, expected);
    assert_eq!(expected["accounts"][0]["effectiveModels"], json!([CURRENT]));
    assert_eq!(expected["accounts"][0]["revision"], 1);
    assert_eq!(expected["accounts"][0]["credentialReady"], true);
    Ok(())
}

#[test]
fn captured_binding_rejects_policy_catalog_identity_and_private_configuration_changes() -> Result<()>
{
    let f = Fixture::new()?;
    let state = f.adopt(&[account("native")])?;
    let id = account_id(&state).to_owned();
    observe(&f, &id, 1, &[CURRENT], 100)?;
    select_models(&f, &[CURRENT], 101)?;
    let facts = f.facts("consumer");
    let captured = consumable_model(&f.conn, &facts, &id, 1, CURRENT, 101)?;
    let unchanged = consumable_model(&f.conn, &facts, &id, 1, CURRENT, 101)?;
    assert_same_binding(&captured, &unchanged)?;
    assert_eq!(captured.account().account_id, id);
    assert_eq!(captured.model(), CURRENT);
    assert_eq!(captured.catalog_checked_at_ms(), 100);

    let mut changed_identity = facts.clone();
    changed_identity.device_id = "another-device".into();
    let current = consumable_model(&f.conn, &changed_identity, &id, 1, CURRENT, 101)?;
    assert!(assert_same_binding(&captured, &current).is_err());

    // Removing and restoring the same choice cannot resurrect an old grant.
    select_models(&f, &[], 101)?;
    select_models(&f, &[CURRENT], 101)?;
    let current = consumable_model(&f.conn, &facts, &id, 1, CURRENT, 101)?;
    assert!(assert_same_binding(&captured, &current).is_err());
    let captured = current;

    observe(&f, &id, 1, &[CURRENT], 102)?;
    let current = consumable_model(&f.conn, &facts, &id, 1, CURRENT, 103)?;
    assert!(assert_same_binding(&captured, &current).is_err());
    let captured = current;

    // Model equality never substitutes for the holder-private configuration.
    set_native_binding(&f.conn, &id, Some(&"a".repeat(64)))?;
    let current = consumable_model(&f.conn, &facts, &id, 1, CURRENT, 103)?;
    assert!(assert_same_binding(&captured, &current).is_err());
    Ok(())
}
