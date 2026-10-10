// Origin: CTOX
// License: AGPL-3.0-only
// DB/source facts below are synthetic regression fixtures, never holder or SDK
// execution evidence. Production claim accepts only AdmittedConsumerAuthority.
use super::*;

#[test]
fn physical_publication_rechecks_the_original_session_expiry() -> anyhow::Result<()> {
    let (root, mut lease, facts) = fixture()?;
    let core = Connection::open(crate::paths::core_db(root.path()))?;
    let policy = store::open_store(root.path())?;
    claim_in_fence(&lease, &core, &policy, &facts)?;
    // Private fixture-only expiry; no caller DTO can change the captured token.
    lease.trusted["expires_at_ms"] = json!(0);
    let error = lease.current(&core, &policy, &facts).unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<SupervisorLumaUnavailable>()
            .unwrap()
            .code,
        "supervisor_execution_fenced"
    );
    Ok(())
}

fn fixture() -> anyhow::Result<(
    tempfile::TempDir,
    NativeSupervisorExecutionLease,
    ConsumerFacts,
)> {
    let (root, token) = super::super::tests::fixture(true)?;
    let lease = NativeSupervisorExecutionLease::capture(root.path(), &token)?;
    let facts = ConsumerFacts {
        owner_user_id: "owner".into(),
        owner_epoch: 0,
        actor_user_id: "device-actor".into(),
        actor_epoch: 0,
        computer_id: "network-computer".into(),
        computer_revision: "1".into(),
        pairing_id: "pair".into(),
        device_id: "device".into(),
        proof_key_thumbprint: "thumb".into(),
        pairing_revision: "1".into(),
    };
    Ok((root, lease, facts))
}
#[test]
fn capture_requires_the_real_signed_restricted_selected_native_lease() -> anyhow::Result<()> {
    let (root, token) = super::super::tests::fixture(false)?;
    assert!(NativeSupervisorExecutionLease::capture(root.path(), &token).is_err());
    let (root, lease, _) = fixture()?;
    assert!(NativeSupervisorExecutionLease::capture(root.path(), "forged").is_err());
    lease.verify_session()?;
    let actual: Option<String> = Connection::open(crate::paths::core_db(root.path()))?.query_row(
        "SELECT actual_json FROM workjet_supervisor_route_attempts",
        [],
        |r| r.get(0),
    )?;
    assert!(actual.is_none());
    Ok(())
}
#[test]
fn the_same_original_lease_never_gets_a_second_controller() -> anyhow::Result<()> {
    let (root, lease, facts) = fixture()?;
    let mut core = Connection::open(crate::paths::core_db(root.path()))?;
    let policy = store::open_store(root.path())?;
    let tx = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (id, consumer) = claim_in_fence(&lease, &tx, &policy, &facts)?;
    current_controller(&tx, &lease, &id, &consumer)?;
    assert!(claim_in_fence(&lease, &tx, &policy, &facts).is_err());
    tx.commit()?;
    let actual: Option<String> = core.query_row(
        "SELECT actual_json FROM workjet_supervisor_route_attempts",
        [],
        |r| r.get(0),
    )?;
    assert!(actual.is_none());
    Ok(())
}
#[test]
fn a_retired_controller_cannot_publish_or_reclaim_the_lease() -> anyhow::Result<()> {
    for state in ["cancelled", "finished"] {
        let (root, lease, facts) = fixture()?;
        let core = Connection::open(crate::paths::core_db(root.path()))?;
        let policy = store::open_store(root.path())?;
        let (id, consumer) = claim_in_fence(&lease, &core, &policy, &facts)?;
        core.execute(
            "UPDATE workjet_supervisor_execution_controllers SET state=?1",
            [state],
        )?;
        assert!(current_controller(&core, &lease, &id, &consumer).is_err());
        assert!(claim_in_fence(&lease, &core, &policy, &facts).is_err());
    }
    Ok(())
}
#[test]
fn only_the_selected_owner_computer_and_exact_controller_are_current() -> anyhow::Result<()> {
    let (root, lease, facts) = fixture()?;
    let core = Connection::open(crate::paths::core_db(root.path()))?;
    let policy = store::open_store(root.path())?;
    for changed in ["owner", "computer"] {
        let mut foreign = facts.clone();
        if changed == "owner" {
            foreign.owner_user_id = "foreign".into();
        } else {
            foreign.computer_id = "foreign-computer".into();
        }
        assert!(claim_in_fence(&lease, &core, &policy, &foreign).is_err());
    }
    let (id, consumer) = claim_in_fence(&lease, &core, &policy, &facts)?;
    assert!(current_controller(&core, &lease, "other-controller", &consumer).is_err());
    assert!(current_controller(&core, &lease, &id, "other-enrollment").is_err());
    Ok(())
}
#[test]
fn replacement_expiry_cancellation_and_terminal_native_command_fence_late_events(
) -> anyhow::Result<()> {
    for update in [
        "UPDATE communication_routing_state SET lease_worker_id='replacement' WHERE route_status='leased'",
        "UPDATE communication_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE route_status='leased'",
        "UPDATE communication_routing_state SET route_status='cancelled' WHERE route_status='leased'",
        "UPDATE business_command_aggregates SET execution_phase='terminal'",
    ] {
        let (root,lease,facts)=fixture()?;
        let core=Connection::open(crate::paths::core_db(root.path()))?;
        let policy=store::open_store(root.path())?;
        claim_in_fence(&lease,&core,&policy,&facts)?;
        core.execute_batch(update)?;
        assert!(lease.current(&core,&policy,&facts).is_err(),"{update}");
    }
    Ok(())
}
#[test]
fn project_selection_or_binding_change_fences_an_already_claimed_controller() -> anyhow::Result<()>
{
    for (collection, id, key, value) in [
        (
            "workjet_projects",
            "project",
            "supervisor_luma_id",
            Value::Null,
        ),
        (
            "workjet_projects",
            "project",
            "owner_user_id",
            json!("foreign"),
        ),
        (
            "workjet_luma_configuration",
            "instance",
            "revision",
            json!(2),
        ),
        (
            "workjet_computers",
            "network-computer",
            "status",
            json!("unassigned"),
        ),
    ] {
        let (root, lease, facts) = fixture()?;
        let core = Connection::open(crate::paths::core_db(root.path()))?;
        let policy = store::open_store(root.path())?;
        claim_in_fence(&lease, &core, &policy, &facts)?;
        let mut record =
            store::outbound_load_record(&policy, collection, id)?.context("fixture record")?;
        record[key] = value;
        store::upsert_business_record(&policy, collection, id, 2, record)?;
        assert!(
            lease.current(&core, &policy, &facts).is_err(),
            "{collection}:{key}"
        );
    }
    Ok(())
}
#[test]
fn changed_native_account_catalog_and_policy_cannot_publish_old_streams() -> anyhow::Result<()> {
    for update in [
        "UPDATE business_provider_federation_accounts SET revision=revision+1",
        "UPDATE business_provider_federation_accounts SET enabled=0",
        "UPDATE business_provider_federation_policy SET revision=revision+1",
        "UPDATE business_provider_federation_model_observations SET last_success_at_ms=0",
        "UPDATE business_provider_federation_models SET models_json='[]'",
    ] {
        let (root, lease, facts) = fixture()?;
        let core = Connection::open(crate::paths::core_db(root.path()))?;
        let policy = store::open_store(root.path())?;
        claim_in_fence(&lease, &core, &policy, &facts)?;
        policy.execute_batch(update)?;
        assert!(lease.current(&core, &policy, &facts).is_err(), "{update}");
    }
    Ok(())
}
