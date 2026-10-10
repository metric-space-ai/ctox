// Origin: CTOX
// License: AGPL-3.0-only
use super::super::tests::{account, account_id, Fixture};
use super::tests::{observe, select_models, CURRENT, OTHER};
use super::*;

#[test]
fn inherited_default_preserves_one_existing_live_model_once() -> Result<()> {
    let f = Fixture::new()?;
    let state = f.adopt(&[account("existing-main")])?;
    let id = account_id(&state);
    observe(&f, id, 1, &[CURRENT, OTHER], 100)?;
    let prior = policy_revision(&f.conn, "owner")?;
    initialize_inherited_selection(&f.conn, "owner", "minimax", Some(CURRENT), 101)?;
    assert_eq!(
        selection(&f.conn, "owner", "minimax")?,
        Some(BTreeSet::from([CURRENT.into()]))
    );
    assert_eq!(policy_revision(&f.conn, "owner")?, prior + 1);
    initialize_inherited_selection(&f.conn, "owner", "minimax", Some(OTHER), 102)?;
    assert_eq!(
        selection(&f.conn, "owner", "minimax")?,
        Some(BTreeSet::from([CURRENT.into()]))
    );
    assert_eq!(policy_revision(&f.conn, "owner")?, prior + 1);
    assert_eq!(
        list(&f.conn, "owner")?["accounts"][0]["effectiveModels"],
        json!([CURRENT])
    );
    Ok(())
}

#[test]
fn inherited_default_never_replaces_explicit_choices_or_empty_selection() -> Result<()> {
    for chosen in [vec![OTHER], vec![]] {
        let f = Fixture::new()?;
        let state = f.adopt(&[account("explicit-choice")])?;
        observe(&f, account_id(&state), 1, &[CURRENT, OTHER], 100)?;
        select_models(&f, &chosen, 101)?;
        let prior = policy_revision(&f.conn, "owner")?;
        initialize_inherited_selection(&f.conn, "owner", "minimax", Some(CURRENT), 102)?;
        assert_eq!(
            selection(&f.conn, "owner", "minimax")?,
            Some(chosen.iter().map(|id| (*id).into()).collect())
        );
        assert_eq!(policy_revision(&f.conn, "owner")?, prior);
    }
    Ok(())
}

#[test]
fn inherited_default_requires_fresh_enabled_live_access_and_existing_intent() -> Result<()> {
    let f = Fixture::new()?;
    let state = f.adopt(&[account("unconfirmed-choice")])?;
    let id = account_id(&state);
    let prior = policy_revision(&f.conn, "owner")?;
    initialize_inherited_selection(&f.conn, "owner", "minimax", None, 100)?;
    initialize_inherited_selection(&f.conn, "owner", "minimax", Some(CURRENT), 100)?;
    observe(&f, id, 1, &[OTHER], 100)?;
    initialize_inherited_selection(&f.conn, "owner", "minimax", Some(CURRENT), 101)?;
    observe(&f, id, 1, &[CURRENT], 100)?;
    initialize_inherited_selection(
        &f.conn,
        "owner",
        "minimax",
        Some(CURRENT),
        100 + CATALOG_FRESHNESS_MS + 1,
    )?;
    f.conn.execute(
        "UPDATE business_provider_federation_accounts SET enabled=0 WHERE account_id=?1",
        [id],
    )?;
    initialize_inherited_selection(&f.conn, "owner", "minimax", Some(CURRENT), 101)?;
    assert_eq!(selection(&f.conn, "owner", "minimax")?, None);
    assert_eq!(policy_revision(&f.conn, "owner")?, prior);
    Ok(())
}
