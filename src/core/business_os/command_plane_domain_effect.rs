// Origin: CTOX
// License: AGPL-3.0-only

use super::*;

/// Invoked only behind the command's existing central authorization.
pub(super) fn recover_applied_domain_effect(
    root: &Path,
    command: &BusinessCommand,
    payload_hash: &str,
    actor_user_id: &str,
) -> anyhow::Result<Option<Value>> {
    let command_id = command.id.as_deref().context("command id is required")?;
    let conn = open_store(root)?;
    let Some(applied) = domain_effect::load(&conn, command_id, payload_hash, actor_user_id)? else {
        return Ok(None);
    };
    let canonical = channels::business_command_projection(root, command_id)?;
    anyhow::ensure!(
        canonical["execution_mode"] == "control" && canonical["payload_hash"] == payload_hash,
        "domain effect receipt does not match canonical control intent"
    );
    let terminal_status = canonical["terminal_status"].as_str().unwrap_or("none");
    anyhow::ensure!(
        matches!(terminal_status, "none" | "completed"),
        "applied domain effect conflicts with terminal command outcome; reconciliation required"
    );

    let mut writers = RxdbProjectionWriterCache::new(root);
    for reference in &applied.projections {
        let mut delivered = false;
        // A concurrent domain commit can race publication. Re-read its revision
        // after delivery, and repair from the latest source before acknowledging.
        // No lock is held across the two SQLite stores.
        for _ in 0..3 {
            let (revision, deleted, updated_at_ms, raw): (String, bool, i64, String) = conn
                .query_row(
                    "SELECT rev, deleted, updated_at_ms, payload_json FROM business_records
                     WHERE collection = ?1 AND record_id = ?2",
                    params![reference.collection, reference.id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?
                .context("committed domain projection source is unavailable")?;
            let payload: Value = serde_json::from_str(&raw)?;
            writers.replace_domain_record_required(
                &reference.collection,
                &reference.id,
                updated_at_ms,
                payload,
                deleted,
            )?;
            let current_revision: Option<String> = conn
                .query_row(
                    "SELECT rev FROM business_records WHERE collection = ?1 AND record_id = ?2",
                    params![reference.collection, reference.id],
                    |row| row.get(0),
                )
                .optional()?;
            if current_revision.as_deref() == Some(revision.as_str()) {
                delivered = true;
                break;
            }
        }
        anyhow::ensure!(
            delivered,
            "domain source changed during recovery; delivery remains pending"
        );
    }
    drop(conn);

    // Keep the original response, not a reconstructed answer from today's data.
    // Core's existing completion remains idempotent and does not reopen terminals.
    let outcome = write_rxdb_control_command_outcome(
        root,
        command,
        "completed",
        None,
        Some("completed"),
        applied.result,
    )?;
    let canonical = channels::business_command_projection(root, command_id)?;
    let updated = canonical["updated_at_ms"]
        .as_i64()
        .context("canonical timestamp missing")?;
    writers.upsert_required("business_commands", command_id, updated, canonical)?;
    Ok(Some(outcome))
}
