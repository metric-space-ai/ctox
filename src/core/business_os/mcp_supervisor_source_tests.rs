// Origin: CTOX
// License: AGPL-3.0-only
// Isolated regression fixtures only; these are not SDK/model execution evidence.
use super::*;

#[test]
fn default_project_stays_on_its_original_path_without_source_schema_or_secret() -> anyhow::Result<()>
{
    let (root, token) = super::super::super::tests::fixture(false)?;
    assert!(super::super::super::capture_lease(root.path(), Some(&token))?.is_none());
    let conn = core(root.path())?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_supervisor_source_offers')",
        [], |r|r.get(0),
    )?;
    assert!(!exists);
    assert_eq!(
        crate::secrets::list_secret_records(root.path(), Some(SECRET_SCOPE))?.len(),
        0
    );
    Ok(())
}

fn fixture() -> anyhow::Result<(tempfile::TempDir, NativeSupervisorExecutionLease)> {
    let (root, token) = super::super::super::tests::fixture(true)?;
    let lease = NativeSupervisorExecutionLease::capture(root.path(), &token)?;
    Ok((root, lease))
}
#[test]
fn source_operation_cannot_create_authority_or_retarget_the_original_lease() -> anyhow::Result<()> {
    for bad in [
        json!({"version":1,"action":"poll","computer_id":"caller"}),
        json!({"version":1,"action":"poll","owner":"caller"}),
        json!({"version":1,"action":"claim","offer_id":uuid::Uuid::new_v4().to_string(),"lease":{"worker":"caller"}}),
        json!({"version":1,"action":"report","actual":true}),
        json!({"version":1,"action":"claim"}),
        json!({"version":1,"action":"status","offer_id":uuid::Uuid::new_v4().to_string()}),
        json!({"version":2,"action":"poll"}),
    ] {
        assert!(parse_operation(vec![bad]).is_err());
    }
    assert!(parse_operation(vec![]).is_err());
    assert!(parse_operation(vec![json!({"version":1,"action":"poll"}), json!({})]).is_err());
    parse_operation(vec![json!({"version":1,"action":"poll"})])?;
    parse_operation(vec![
        json!({"version":1,"action":"claim","offer_id":uuid::Uuid::new_v4().to_string()}),
    ])?;
    Ok(())
}
#[test]
fn one_original_native_lease_has_one_offer_and_the_token_is_only_in_the_secret_store(
) -> anyhow::Result<()> {
    let (root, lease) = fixture()?;
    let expected = Zeroizing::new(lease.token.to_string());
    let offer = NativeSupervisorSourceOffer::open(lease, "real fixture execution prompt")?;
    let core = core(root.path())?;
    let row: (String, String, String) = core.query_row(
        "SELECT prompt,state,execution_key FROM workjet_supervisor_source_offers",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    assert_eq!(row.0, "real fixture execution prompt");
    assert_eq!(row.1, "offered");
    assert_eq!(row.2, offer.lease.execution_key);
    let stored = Zeroizing::new(crate::secrets::read_secret_value(
        root.path(),
        SECRET_SCOPE,
        &offer.id,
    )?);
    assert!(stored.as_str() == expected.as_str());
    let duplicate = NativeSupervisorExecutionLease::capture(root.path(), &expected)?;
    assert!(NativeSupervisorSourceOffer::open(duplicate, "second prompt").is_err());
    assert_eq!(
        core.query_row(
            "SELECT count(*) FROM workjet_supervisor_source_offers",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        1
    );
    assert_eq!(
        crate::secrets::list_secret_records(root.path(), Some(SECRET_SCOPE))?.len(),
        1
    );
    let offer_id = offer.id.clone();
    drop(offer);
    assert_eq!(
        core.query_row(
            "SELECT state FROM workjet_supervisor_source_offers",
            [],
            |r| r.get::<_, String>(0)
        )?,
        "closed"
    );
    assert!(!crate::secrets::secret_exists(
        root.path(),
        SECRET_SCOPE,
        &offer_id
    )?);
    assert_eq!(
        crate::secrets::list_secret_records(root.path(), Some(SECRET_SCOPE))?.len(),
        0
    );
    Ok(())
}
#[test]
fn expired_or_changed_original_lease_closes_without_accepting_a_source_result() -> anyhow::Result<()>
{
    for expired in [false, true] {
        let (root, lease) = fixture()?;
        let mut offer = NativeSupervisorSourceOffer::open(lease, "fixture prompt")?;
        if expired {
            offer.deadline_ms = 0;
        } else {
            core(root.path())?.execute("UPDATE communication_routing_state SET lease_worker_id='replacement' WHERE route_status='leased'",[])?;
        }
        let error = offer.wait_for_native_result().unwrap_err();
        if expired {
            assert_eq!(
                error
                    .downcast_ref::<SupervisorLumaUnavailable>()
                    .unwrap()
                    .code,
                "project_supervisor_source_wait_timeout"
            );
        }
        let id = offer.id.clone();
        drop(offer);
        assert_eq!(
            core(root.path())?.query_row(
                "SELECT state FROM workjet_supervisor_source_offers WHERE offer_id=?1",
                [&id],
                |r| r.get::<_, String>(0)
            )?,
            "closed"
        );
        assert!(!crate::secrets::secret_exists(
            root.path(),
            SECRET_SCOPE,
            &id
        )?);
    }
    Ok(())
}
#[test]
fn foreign_owner_or_computer_cannot_read_even_a_known_offer_id() -> anyhow::Result<()> {
    let (root, lease) = fixture()?;
    let offer = NativeSupervisorSourceOffer::open(lease, "private fixture prompt")?;
    let core = core(root.path())?;
    let facts = ConsumerFacts {
        owner_user_id: "owner".into(),
        owner_epoch: 0,
        actor_user_id: "source".into(),
        actor_epoch: 0,
        computer_id: "network-computer".into(),
        computer_revision: "1".into(),
        pairing_id: "pair".into(),
        device_id: "device".into(),
        proof_key_thumbprint: "thumb".into(),
        pairing_revision: "1".into(),
    };
    let row = read_offer(&core, &offer.id, &facts)?;
    check_offer_lease(&row, &offer.lease)?;
    for wrong in ["owner", "computer"] {
        let mut foreign = facts.clone();
        if wrong == "owner" {
            foreign.owner_user_id = "foreign".into()
        } else {
            foreign.computer_id = "foreign".into()
        }
        assert!(read_offer(&core, &offer.id, &foreign).is_err());
    }
    Ok(())
}
#[test]
fn invalid_or_oversized_offer_never_retains_a_native_scope_secret() -> anyhow::Result<()> {
    let (root, lease) = fixture()?;
    assert!(NativeSupervisorSourceOffer::open(lease, &"x".repeat(MAX_PROMPT + 1)).is_err());
    assert_eq!(
        crate::secrets::list_secret_records(root.path(), Some(SECRET_SCOPE))?.len(),
        0
    );
    Ok(())
}
