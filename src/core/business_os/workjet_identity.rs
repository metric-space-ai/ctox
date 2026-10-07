// Origin: CTOX
// License: AGPL-3.0-only

//! Verified Workjet owner aliases. Profile names/emails and caller payloads
//! cannot enroll an alias. Only the authenticated managed-user issuer can.
#[cfg(test)]
#[path = "workjet_identity_tests.rs"]
mod tests;

use anyhow::{ensure, Context};
use rusqlite::{params, Connection, OptionalExtension};

pub(super) fn remember_managed_identity(
    conn: &Connection,
    user: &str,
    email: Option<&str>,
    now: i64,
) -> anyhow::Result<()> {
    let Some(email) = email else { return Ok(()) };
    // A managed UUID is the stable identity; a legacy email login is its alias.
    if uuid::Uuid::parse_str(user).is_err() {
        return Ok(());
    }
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let existing: Option<String> = tx
        .query_row(
            "SELECT canonical_user_id FROM business_user_identity_aliases WHERE alias=?1",
            [email],
            |row| row.get(0),
        )
        .optional()?;
    ensure!(
        existing.as_deref().is_none_or(|id| id == user),
        "verified identity alias already belongs to another managed user"
    );
    tx.execute(
        "DELETE FROM business_user_identity_aliases WHERE canonical_user_id=?1 AND alias<>?2",
        params![user, email],
    )?;
    let changed = tx.execute("INSERT INTO business_user_identity_aliases(alias,canonical_user_id,verified_at_ms) VALUES (?1,?2,?3)
        ON CONFLICT(alias) DO UPDATE SET verified_at_ms=excluded.verified_at_ms
        WHERE canonical_user_id=excluded.canonical_user_id", params![email,user,now])?;
    ensure!(changed == 1, "verified identity alias changed concurrently");
    tx.commit()?;
    Ok(())
}

pub(super) fn owner_from_connection(
    conn: &Connection,
    authenticated_user: &str,
) -> anyhow::Result<String> {
    let has_registry: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='business_user_identity_aliases')", [], |row| row.get(0))?;
    if !has_registry {
        return Ok(authenticated_user.to_owned());
    }
    let owner: Option<String> = conn
        .query_row(
            "SELECT canonical_user_id FROM business_user_identity_aliases WHERE alias=?1",
            [authenticated_user.to_ascii_lowercase()],
            |row| row.get(0),
        )
        .optional()?;
    let Some(owner) = owner else {
        return Ok(authenticated_user.to_owned());
    };
    for id in [authenticated_user, owner.as_str()] {
        let active: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM business_users WHERE user_id=?1 AND active=1)",
            [id],
            |row| row.get(0),
        )?;
        ensure!(active, "verified Workjet identity is inactive");
    }
    Ok(owner)
}

pub(super) fn owner(root: &std::path::Path, authenticated_user: &str) -> anyhow::Result<String> {
    owner_from_connection(&super::store::open_store(root)?, authenticated_user)
        .context("Workjet owner identity is unavailable")
}
