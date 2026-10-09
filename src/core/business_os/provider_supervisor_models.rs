// Origin: CTOX
// License: AGPL-3.0-only

//! Model eligibility for the instance's durable project Supervisor.
//! Call only inside Crew's verified command/plan lease and current project
//! Owner/binding fence. This is not a lease, consumer identity or holder permit.

use super::*;

pub(crate) struct SupervisorModelEligibility {
    owner: String,
    account: ConsumableAccount,
    model: String,
    catalog_checked_at_ms: i64,
    private_binding: Option<String>,
    catalog_fingerprint: Vec<u8>,
}

impl SupervisorModelEligibility {
    pub(crate) fn account(&self) -> &ConsumableAccount {
        &self.account
    }

    pub(crate) fn model(&self) -> &str {
        &self.model
    }

    pub(crate) fn catalog_checked_at_ms(&self) -> i64 {
        self.catalog_checked_at_ms
    }

    /// Re-enter Crew's real lease/project fence before this check and before
    /// dispatch/publication. No network or secret API re-entry under the DB
    /// fence. The private holding adapter separately verifies its current
    /// credential/configuration and exact account pin; remote holders need
    /// their authenticated execution authority.
    pub(crate) fn revalidate(&self, conn: &Connection, verified_owner: &str) -> Result<()> {
        self.revalidate_at(conn, verified_owner, store::now_ms() as i64)
    }

    fn revalidate_at(&self, conn: &Connection, owner: &str, now: i64) -> Result<()> {
        let current = resolve_at(
            conn,
            owner,
            &self.account.account_id,
            self.account.account_revision,
            &self.model,
            now,
        )?;
        ensure!(
            self.owner == current.owner
                && self.account.account_id == current.account.account_id
                && self.account.holder_instance_id == current.account.holder_instance_id
                && self.account.provider == current.account.provider
                && self.account.private_local_account_id
                    == current.account.private_local_account_id
                && self.account.account_revision == current.account.account_revision
                && self.account.policy_revision == current.account.policy_revision
                && self.model == current.model
                && self.catalog_checked_at_ms == current.catalog_checked_at_ms
                && self.private_binding == current.private_binding
                && self.catalog_fingerprint == current.catalog_fingerprint,
            "Supervisor model eligibility changed"
        );
        Ok(())
    }
}

/// verified_owner is obtained from Crew's current admitted Supervisor/plan
/// lease, never an untrusted request field. The passed Policy connection must
/// already be under that Owner/project/binding fence. No browser transport is
/// required and no ConsumerFacts are manufactured. Opaque account IDs resolve
/// only through the canonical native registry.
pub(crate) fn resolve_supervisor_model(
    conn: &Connection,
    verified_owner: &str,
    account_id: &str,
    expected_account_revision: i64,
    model: &str,
) -> Result<SupervisorModelEligibility> {
    resolve_at(
        conn,
        verified_owner,
        account_id,
        expected_account_revision,
        model,
        store::now_ms() as i64,
    )
}

fn resolve_at(
    conn: &Connection,
    owner: &str,
    id: &str,
    revision: i64,
    model: &str,
    now: i64,
) -> Result<SupervisorModelEligibility> {
    bounded_id(id)?;
    bounded_id(model)?;
    ensure!(
        management_owner(conn, owner)? == owner,
        "Supervisor Owner changed"
    );
    let (holder, provider, local, current): (String, String, String, i64) = conn
        .query_row(
            "SELECT holder_instance_id,provider,private_local_account_id,revision
         FROM business_provider_federation_accounts
         WHERE account_id=?1 AND owner_user_id=?2 AND enabled=1 AND credential_ready=1",
            params![id, owner],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .context("Supervisor account is unavailable")?;
    ensure!(
        revision > 0 && current == revision,
        "Supervisor account revision changed"
    );
    let catalog = catalog_projection(conn, id, revision, now)?;
    ensure!(
        catalog["fresh"] == true,
        "Supervisor account live model discovery is stale or unavailable"
    );
    ensure!(
        observed_models(&catalog)?.contains(model),
        "Supervisor model is not in this account's live catalog"
    );
    ensure!(
        selection(conn, owner, &provider)?.is_some_and(|models| models.contains(model)),
        "Supervisor model is not selected for this provider"
    );
    ensure!(
        !exclusions(conn, id)?.contains(model),
        "Supervisor model is excluded on this account"
    );
    use sha2::Digest;
    Ok(SupervisorModelEligibility {
        owner: owner.to_owned(),
        account: ConsumableAccount {
            account_id: id.to_owned(),
            holder_instance_id: holder,
            provider,
            private_local_account_id: local,
            account_revision: current,
            policy_revision: policy_revision(conn, owner)?,
        },
        model: model.to_owned(),
        catalog_checked_at_ms: catalog["lastSuccessAtMs"]
            .as_i64()
            .context("catalog time missing")?,
        private_binding: native_binding(conn, id)?,
        catalog_fingerprint: sha2::Sha256::digest(serde_json::to_vec(&catalog)?).to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::super::tests::{account, account_id, Fixture};
    use super::super::tests::{observe, select_models, CURRENT, OTHER};
    use super::*;

    #[test]
    fn supervisor_uses_only_current_owned_selected_account_models_without_browser_facts(
    ) -> Result<()> {
        let f = Fixture::new()?;
        let state = f.adopt(&[account("native")])?;
        let id = account_id(&state);
        observe(&f, id, 1, &[CURRENT, OTHER], 100)?;
        assert!(resolve_at(&f.conn, "owner", id, 1, CURRENT, 101).is_err());
        select_models(&f, &[CURRENT], 101)?;
        let chosen = resolve_at(&f.conn, "owner", id, 1, CURRENT, 101)?;
        assert_eq!(chosen.account().account_id, id);
        assert_eq!(chosen.account().holder_instance_id, "native-instance");
        assert_eq!(chosen.account().private_local_account_id, "native");
        assert_eq!(chosen.model(), CURRENT);
        assert_eq!(chosen.catalog_checked_at_ms(), 100);
        chosen.revalidate_at(&f.conn, "owner", 101)?;
        assert!(resolve_at(&f.conn, "foreign", id, 1, CURRENT, 101).is_err());
        assert!(resolve_at(&f.conn, "owner", id, 2, CURRENT, 101).is_err());
        assert!(resolve_at(&f.conn, "owner", id, 1, OTHER, 101).is_err());
        assert!(resolve_at(
            &f.conn,
            "owner",
            id,
            1,
            CURRENT,
            100 + CATALOG_FRESHNESS_MS + 1
        )
        .is_err());
        assert!(resolve_at(&f.conn, "owner", id, 1, CURRENT, 99).is_err());
        f.conn.execute(
            "UPDATE business_users SET active=0 WHERE user_id='owner'",
            [],
        )?;
        assert!(chosen.revalidate_at(&f.conn, "owner", 101).is_err());
        Ok(())
    }

    #[test]
    fn captured_eligibility_retires_on_catalog_private_binding_policy_and_holder_changes(
    ) -> Result<()> {
        let f = Fixture::new()?;
        let state = f.adopt(&[account("native")])?;
        let id = account_id(&state);
        observe(&f, id, 1, &[CURRENT], 100)?;
        select_models(&f, &[CURRENT], 101)?;
        let chosen = resolve_at(&f.conn, "owner", id, 1, CURRENT, 101)?;
        observe(&f, id, 1, &[CURRENT], 102)?;
        assert!(chosen.revalidate_at(&f.conn, "owner", 103).is_err());
        let chosen = resolve_at(&f.conn, "owner", id, 1, CURRENT, 103)?;
        set_native_binding(&f.conn, id, Some("private-fixture-generation"))?;
        assert!(chosen.revalidate_at(&f.conn, "owner", 103).is_err());
        let chosen = resolve_at(&f.conn, "owner", id, 1, CURRENT, 103)?;
        bump_policy(&f.conn, "owner")?;
        assert!(chosen.revalidate_at(&f.conn, "owner", 103).is_err());
        let chosen = resolve_at(&f.conn, "owner", id, 1, CURRENT, 103)?;
        f.conn.execute("UPDATE business_provider_federation_accounts SET holder_instance_id='replacement' WHERE account_id=?1",[id])?;
        assert!(chosen.revalidate_at(&f.conn, "owner", 103).is_err());
        f.conn.execute(
            "UPDATE business_provider_federation_accounts SET enabled=0 WHERE account_id=?1",
            [id],
        )?;
        assert!(resolve_at(&f.conn, "owner", id, 1, CURRENT, 103).is_err());
        Ok(())
    }

    #[test]
    fn exclusion_and_failed_discovery_reject_without_mutating_the_account() -> Result<()> {
        let f = Fixture::new()?;
        let state = f.adopt(&[account("native")])?;
        let id = account_id(&state);
        observe(&f, id, 1, &[CURRENT], 100)?;
        select_models(&f, &[CURRENT], 101)?;
        f.conn.execute("INSERT INTO business_provider_federation_model_exclusions(account_id,models_json) VALUES (?1,?2)",
            params![id,serde_json::to_string(&[CURRENT])?])?;
        assert!(resolve_at(&f.conn, "owner", id, 1, CURRENT, 101).is_err());
        f.conn.execute(
            "DELETE FROM business_provider_federation_model_exclusions WHERE account_id=?1",
            [id],
        )?;
        f.conn.execute("UPDATE business_provider_federation_model_observations SET last_attempt_json=?2 WHERE account_id=?1",
            params![id,r#"{"checkedAtMs":102,"success":false,"failure":"transport_failed"}"#])?;
        assert!(resolve_at(&f.conn, "owner", id, 1, CURRENT, 103).is_err());
        assert_eq!(list(&f.conn, "owner")?["accounts"][0]["enabled"], true);
        assert_eq!(list(&f.conn, "owner")?["accounts"][0]["revision"], 1);
        Ok(())
    }
}
