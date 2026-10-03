//! An auth-assist request is a durable human wait, not a model execution.
//! Keep its original command/session identity until explicit browser confirmation.
#[cfg(test)]
#[path = "auth_assist_tests.rs"]
mod tests;
use super::command_saga::transition_business_command_for_task_in_transaction;
use super::*;

pub(super) const REQUEST_TYPE: &str = "web_stack.auth_assist.request";
const WAIT_TYPE: &str = "web_stack_auth_assist";
const WAIT_NOTE: &str = "Anmeldung im Browser bestätigen";

/// Called inside canonical admission and restart recovery transactions. The
/// command lifecycle still owns transitions; this never manufactures a lease,
/// a worker result, or review/validation evidence.
pub(super) fn preserve_request(
    tx: &Transaction<'_>,
    task_id: &str,
    command_id: &str,
) -> Result<bool> {
    let eligible: bool = tx.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM business_command_aggregates a
            JOIN business_command_task_links l ON l.command_id = a.command_id
            JOIN communication_routing_state r ON r.message_key = l.task_id
            WHERE a.command_id = ?1 AND l.task_id = ?2 AND a.command_type = ?3
              AND a.execution_phase != 'terminal'
              AND r.route_status IN ('pending', 'leased', 'review_rework', 'blocked')
              AND NOT (
                  r.route_status = 'blocked'
                  AND COALESCE(r.hold_reason, '') = 'waiting_external'
                  AND COALESCE(r.wait_entity_type, '') = ?4
                  AND COALESCE(r.wait_entity_id, '') = ?1
              )
        )",
        params![command_id, task_id, REQUEST_TYPE, WAIT_TYPE],
        |row| row.get(0),
    )?;
    if !eligible {
        return Ok(false);
    }
    transition_business_command_for_task_in_transaction(
        tx,
        task_id,
        "blocked",
        None,
        None,
        Some(WAIT_NOTE),
        "auth_assist_waiting_for_browser_confirmation",
    )?;
    tx.execute(
        "UPDATE communication_routing_state
         SET hold_reason = 'waiting_external', wait_entity_type = ?2,
             wait_entity_id = ?3, retry_not_before = NULL,
             lease_expires_at = NULL, lease_worker_id = NULL,
             last_error = NULL, updated_at = ?4
         WHERE message_key = ?1",
        params![task_id, WAIT_TYPE, command_id, now_iso_string()],
    )?;
    Ok(true)
}

/// Recover pre-upgrade requests before generic stale-lease/worker recovery.
/// Bounded pages release the SQLite writer between batches. Terminal commands
/// remain terminal; an explicitly completed/cancelled login is never revived.
pub(crate) fn recover_auth_assist_requests(root: &Path) -> Result<usize> {
    let mut conn = open_channel_db(&resolve_db_path(root, None))?;
    ensure_queue_account(&mut conn)?;
    attach_queue_projection_store(root, &conn)?;
    let mut cursor = String::new();
    let mut recovered = 0;
    loop {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let rows = {
            let mut statement = tx.prepare(
                "SELECT a.command_id, l.task_id
                 FROM business_command_aggregates a
                 JOIN business_command_task_links l ON l.command_id = a.command_id
                 JOIN communication_routing_state r ON r.message_key = l.task_id
                 WHERE a.command_type = ?1 AND a.execution_phase != 'terminal'
                   AND a.command_id > ?2
                   AND r.route_status IN ('pending', 'leased', 'review_rework', 'blocked')
                 ORDER BY a.command_id LIMIT 32",
            )?;
            let rows = statement.query_map(params![REQUEST_TYPE, cursor], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if rows.is_empty() {
            tx.commit()?;
            return Ok(recovered);
        }
        let mut tasks = Vec::new();
        for (command_id, task_id) in &rows {
            if preserve_request(&tx, task_id, command_id)? {
                tasks.push(
                    load_queue_task_from_conn(&tx, task_id)?
                        .context("auth-assist recovery task missing")?,
                );
            }
        }
        refresh_queue_projection_tasks(root, &tx, &tasks)?;
        tx.commit()?;
        recovered += tasks.len();
        cursor = rows.last().expect("nonempty page").0.clone();
    }
}
