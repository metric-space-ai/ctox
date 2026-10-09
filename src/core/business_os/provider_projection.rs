// Origin: CTOX
// License: AGPL-3.0-only

//! Native-authored registry metadata over the existing CTOX Sync data plane.
//! This projection is never an inference permit or a credential reference.

use super::super::domain_effect::DomainRecordRef;
use super::*;
use sha2::{Digest, Sha256};

pub(super) const COLLECTION: &str = "workjet_provider_registry";

fn record_id(owner: &str) -> String {
    format!("provider_registry_{:x}", Sha256::digest(owner.as_bytes()))
}

/// Persist the current metadata in the same policy transaction as the mutation
/// and its domain receipt. The normal Core terminal projection repair publishes
/// this reference; it must read the latest record, never a historical result.
pub(super) fn applied(conn: &Connection, owner: &str) -> Result<AppliedDomainEffect> {
    let result = list(conn, owner)?;
    let mut accounts = result["accounts"].clone();
    for account in accounts
        .as_array_mut()
        .context("registry accounts missing")?
    {
        // Freshness expires without a write. Consumers derive it from the last
        // success/attempt and this duration, not a persisted ready flag.
        account["modelCatalog"]
            .as_object_mut()
            .context("registry catalog missing")?
            .remove("fresh");
    }
    let id = record_id(owner);
    let now = store::now_ms() as i64;
    let record = json!({
        "id":id,"owner_user_id":owner,"updated_at_ms":now,
        "policy_revision":result["revision"],
        "catalog_freshness_ms":CATALOG_FRESHNESS_MS,
        "accounts":accounts,"providers":result["providers"],"is_deleted":false,
    });
    ensure!(
        serde_json::to_vec(&record)?.len() <= 1024 * 1024,
        "provider registry projection exceeds the bounded Sync record size"
    );
    super::super::store_workjet_projects::persist_idempotently(conn, COLLECTION, &id, now, record)?;
    Ok(AppliedDomainEffect {
        result,
        projections: vec![DomainRecordRef {
            collection: COLLECTION.into(),
            id,
        }],
    })
}

/// Own-registry constraint BEFORE the generic administrative read shortcut.
/// A foreign administrator or a forged owner field cannot broaden visibility.
pub(super) fn visible(conn: &Connection, document: &Value, actor: &str) -> Result<bool> {
    let owner = management_owner(conn, actor)?;
    let id = record_id(&owner);
    if document["owner_user_id"] != owner || document["id"] != id {
        return Ok(false);
    }
    let Some(current) = store::outbound_load_record(conn, COLLECTION, &id)? else {
        return Ok(false);
    };
    // Never authorize victim metadata with a substituted owner/id envelope.
    Ok([
        "id",
        "owner_user_id",
        "policy_revision",
        "catalog_freshness_ms",
        "accounts",
        "providers",
        "is_deleted",
    ]
    .iter()
    .all(|field| document[*field] == current[*field]))
}

/// Startup/backfill repair for already adopted accounts. Rebuild only public
/// metadata from current native policy; never execute a historical command.
pub(super) fn repair(root: &Path) -> Result<()> {
    let mut conn = store::open_store(root)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let owners = {
        let mut statement = tx.prepare(
            "SELECT owner_user_id FROM business_provider_federation_policy
             UNION SELECT owner_user_id FROM business_provider_federation_accounts
             ORDER BY owner_user_id LIMIT 257",
        )?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(
            rows.len() <= 256,
            "provider projection owner limit exceeded"
        );
        rows
    };
    for owner in owners {
        applied(&tx, &owner)?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::tests::{account, Fixture};
    use super::*;

    #[test]
    fn startup_backfill_keeps_existing_accounts_and_current_metadata() -> Result<()> {
        let f = Fixture::new()?;
        f.adopt(&[account("private-backfill")])?;
        assert!(store::outbound_load_record(&f.conn, COLLECTION, &record_id("owner"))?.is_none());
        repair(f.root.path())?;
        let first = store::outbound_load_record(&f.conn, COLLECTION, &record_id("owner"))?.unwrap();
        assert_eq!(first["accounts"].as_array().unwrap().len(), 1);
        repair(f.root.path())?;
        assert_eq!(
            first,
            store::outbound_load_record(&f.conn, COLLECTION, &record_id("owner"))?.unwrap()
        );
        f.conn.execute(
            "UPDATE business_provider_federation_accounts SET enabled=0,revision=revision+1",
            [],
        )?;
        repair(f.root.path())?;
        let current =
            store::outbound_load_record(&f.conn, COLLECTION, &record_id("owner"))?.unwrap();
        assert_eq!(current["accounts"][0]["enabled"], false);
        assert_eq!(current["accounts"].as_array().unwrap().len(), 1);
        assert!(!visible(&f.conn, &first, "owner")?);
        assert!(visible(&f.conn, &current, "owner")?);
        Ok(())
    }

    #[test]
    fn actual_sync_readers_apply_owner_filter_and_reject_peer_writes() -> Result<()> {
        let f = Fixture::new()?;
        f.adopt(&[account("private-transport")])?;
        repair(f.root.path())?;
        let record =
            store::outbound_load_record(&f.conn, COLLECTION, &record_id("owner"))?.unwrap();
        let (owner_token, _) = store::issue_business_os_capability_token_for_managed_user(
            f.root.path(),
            "owner",
            "Owner",
            "chef",
            store::now_ms() as i64,
        )?;
        let (foreign_token, _) = store::issue_business_os_capability_token_for_managed_user(
            f.root.path(),
            "foreign",
            "Foreign",
            "admin",
            store::now_ms() as i64,
        )?;
        let threads = super::super::super::threads::may_replicate_document;
        assert!(threads(f.root.path(), &owner_token, COLLECTION, &record));
        assert!(!threads(f.root.path(), &foreign_token, COLLECTION, &record));
        assert!(
            !super::super::super::threads::may_accept_peer_document_write(
                f.root.path(),
                &owner_token,
                COLLECTION,
                &record,
            )
        );
        let core = Connection::open_in_memory()?;
        let projection = Connection::open(store::rxdb_store_path(f.root.path()))?;
        let held =
            super::super::super::threads::native_business_data_document_visible_from_connections;
        for (user, role, allowed, expected) in [
            ("owner", "chef", true, true),
            ("foreign", "admin", true, false),
            ("owner", "chef", false, false),
        ] {
            assert_eq!(
                held(
                    &core,
                    &f.conn,
                    &projection,
                    COLLECTION,
                    &record,
                    super::super::super::threads::ReplicationActor {
                        user_id: user,
                        role,
                        collection_read_allowed: allowed,
                    }
                )?,
                expected
            );
        }
        Ok(())
    }

    #[test]
    fn registry_projection_is_content_only_and_idempotent() -> Result<()> {
        let f = Fixture::new()?;
        f.adopt(&[account("private-selector-never-synced")])?;
        let first = applied(&f.conn, "owner")?;
        assert_eq!(first.projections.len(), 1);
        let reference = &first.projections[0];
        let stored = store::outbound_load_record(&f.conn, COLLECTION, &reference.id)?
            .context("registry projection missing")?;
        for forbidden in [
            "private-selector-never-synced",
            "private_local_account_id",
            "secret_ref",
            "fingerprint",
        ] {
            assert!(!stored.to_string().contains(forbidden));
        }
        assert!(stored["accounts"][0]["modelCatalog"].get("fresh").is_none());
        assert_eq!(stored["accounts"][0]["inferenceVerified"], false);
        assert_eq!(stored["catalog_freshness_ms"], CATALOG_FRESHNESS_MS);
        applied(&f.conn, "owner")?;
        let repeated = store::outbound_load_record(&f.conn, COLLECTION, &reference.id)?.unwrap();
        assert_eq!(stored, repeated);
        Ok(())
    }

    #[test]
    fn registry_projection_requires_current_own_management_identity() -> Result<()> {
        let f = Fixture::new()?;
        f.adopt(&[account("private")])?;
        let applied = applied(&f.conn, "owner")?;
        let mut record =
            store::outbound_load_record(&f.conn, COLLECTION, &applied.projections[0].id)?.unwrap();
        assert!(visible(&f.conn, &record, "owner")?);
        store::issue_business_os_capability_token_for_managed_user(
            f.root.path(),
            "foreign",
            "Foreign",
            "admin",
            store::now_ms() as i64,
        )?;
        assert!(!visible(&f.conn, &record, "foreign")?);
        record["owner_user_id"] = json!("foreign");
        assert!(!visible(&f.conn, &record, "foreign")?);
        record["id"] = json!(record_id("foreign"));
        assert!(!visible(&f.conn, &record, "foreign")?);
        f.conn.execute(
            "UPDATE business_users SET active=0 WHERE user_id='foreign'",
            [],
        )?;
        assert!(visible(&f.conn, &record, "foreign").is_err());
        Ok(())
    }
}
