// Origin: CTOX
// License: AGPL-3.0-only

//! Owner input belongs to an existing command/task. Admission never revokes its
//! worker lease, approves its result, or starts another task. Input arriving
//! after a worker's prompt snapshot prevents that snapshot from closing work.
use super::*;
use anyhow::ensure;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SupervisorOwnerInput {
    pub input_id: String,
    pub sequence: i64,
    pub body: String,
    pub created_at: String,
}

pub(crate) fn admit(
    root: &Path,
    task_id: &str,
    command_id: &str,
    input_id: &str,
    owner: &str,
    body: &str,
) -> Result<SupervisorOwnerInput> {
    ensure!(!body.trim().is_empty() && body.chars().count() <= 4096, "invalid supervisor input");
    let mut conn = open_channel_db(&resolve_db_path(root, None))?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let replay = tx.query_row(
        "SELECT task_id, command_id, owner_user_id, body_text, sequence, created_at
         FROM supervisor_owner_inputs WHERE input_id=?1",
        [input_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?,
            row.get::<_, String>(2)?, row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?, row.get::<_, String>(5)?)),
    ).optional()?;
    if let Some((task, command, actor, text, sequence, created_at)) = replay {
        ensure!(task == task_id && command == command_id && actor == owner && text == body,
            "supervisor input id has a different admitted intent");
        tx.commit()?;
        return Ok(SupervisorOwnerInput { input_id: input_id.to_owned(), sequence,
            body: text, created_at });
    }
    let supported: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_command_task_links l
         JOIN business_command_aggregates a ON a.command_id=l.command_id
         JOIN communication_messages m ON m.message_key=l.task_id
         WHERE l.task_id=?1 AND a.command_id=?2
           AND a.execution_phase!='terminal' AND a.terminal_status='none'
           AND a.module='ctox' AND a.command_type='business_os.chat.task'
           AND m.channel='queue' AND m.direction='inbound'
           AND json_extract(a.intent_json,'$.payload.risk_class')='internal'
           AND json_type(a.intent_json,'$.payload.external_executor') IS NULL)",
        params![task_id, command_id], |row| row.get(0),
    )?;
    ensure!(supported, "supervisor input target is terminal, conflicting, or unsupported");
    let (count, bytes, sequence): (i64, i64, i64) = tx.query_row(
        "SELECT COUNT(*),COALESCE(SUM(length(CAST(body_text AS BLOB))),0),
                COALESCE(MAX(sequence),0)+1 FROM supervisor_owner_inputs WHERE task_id=?1",
        [task_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    ensure!(count < 128 && bytes + body.len() as i64 <= 512 * 1024,
        "supervisor follow-up context limit reached");
    let created_at = now_iso_string();
    tx.execute(
        "INSERT INTO supervisor_owner_inputs
         (input_id,task_id,command_id,owner_user_id,sequence,body_text,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![input_id, task_id, command_id, owner, sequence, body, created_at],
    )?;
    // New facts may address a missing-source review. Keep the review, failure
    // budget and all other holds; only shorten this specific review retry.
    tx.execute(
        "UPDATE communication_routing_state SET retry_not_before=NULL, updated_at=?2
         WHERE message_key=?1 AND route_status='pending'
           AND hold_reason IN ('missing_artifact','missing_review_evidence')",
        params![task_id, created_at],
    )?;
    tx.commit()?;
    crate::business_os::harness_cockpit::schedule_refresh(root);
    Ok(SupervisorOwnerInput { input_id: input_id.to_owned(), sequence,
        body: body.to_owned(), created_at })
}

/// Called after registering the actual worker run and before invoking its
/// provider. The expiring native lease fences the captured input high-water.
pub(crate) fn capture(
    root: &Path,
    task_id: &str,
    attempt_id: &str,
    worker_id: &str,
) -> Result<Vec<SupervisorOwnerInput>> {
    let mut conn = open_channel_db(&resolve_db_path(root, None))?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let command_attempt: Option<(String, i64)> = tx.query_row(
        "SELECT a.command_id,a.attempt FROM business_command_task_links l
         JOIN business_command_aggregates a ON a.command_id=l.command_id
         JOIN communication_routing_state r ON r.message_key=l.task_id
         WHERE l.task_id=?1 AND a.command_type='business_os.chat.task'
           AND a.execution_phase='running' AND a.terminal_status='none'
           AND r.route_status='leased' AND r.lease_worker_id=?2
           AND r.lease_expires_at IS NOT NULL AND r.lease_expires_at>?3",
        params![task_id, worker_id, now_iso_string()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    let Some((command_id, command_attempt)) = command_attempt else {
        // Ordinary queue work has no supervisor input channel.
        let linked: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM business_command_task_links l
             JOIN business_command_aggregates a ON a.command_id=l.command_id
             WHERE l.task_id=?1 AND a.command_type='business_os.chat.task')",
            [task_id], |row| row.get(0),
        )?;
        ensure!(!linked, "supervisor input capture lost its current worker lease");
        tx.commit()?;
        return Ok(Vec::new());
    };
    let inputs = {
        let mut query = tx.prepare(
            "SELECT input_id,sequence,body_text,created_at
             FROM supervisor_owner_inputs WHERE task_id=?1 ORDER BY sequence",
        )?;
        let rows = query.query_map([task_id], |row| Ok(SupervisorOwnerInput {
            input_id: row.get(0)?, sequence: row.get(1)?,
            body: row.get(2)?, created_at: row.get(3)?,
        }))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let high_water = inputs.last().map_or(0, |input| input.sequence);
    tx.execute(
        "INSERT INTO supervisor_owner_input_snapshots
         (task_id,attempt_id,command_id,command_attempt,lease_worker_id,through_sequence,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(task_id,attempt_id) DO NOTHING",
        params![task_id, attempt_id, command_id, command_attempt, worker_id,
            high_water, now_iso_string()],
    )?;
    let (existing, captured_worker, captured_attempt): (i64, String, i64) = tx.query_row(
        "SELECT through_sequence,lease_worker_id,command_attempt FROM supervisor_owner_input_snapshots
         WHERE task_id=?1 AND attempt_id=?2",
        params![task_id, attempt_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    ensure!(captured_worker == worker_id && captured_attempt == command_attempt,
        "worker input snapshot belongs to a different lease or command attempt");
    tx.commit()?;
    // Recovery retains the original snapshot. Later accepted input belongs to
    // the next slice, rather than silently rewriting an existing attempt.
    Ok(inputs.into_iter().filter(|input| input.sequence <= existing).collect())
}

/// Must run under the same transaction as terminal success. An input/terminal
/// race therefore has exactly one winner, never an accepted but discarded input.
pub(super) fn has_uncaptured(conn: &Connection, task_id: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM supervisor_owner_inputs i
         JOIN business_command_aggregates a ON a.command_id=i.command_id
         WHERE i.task_id=?1 AND i.sequence>COALESCE(
           (SELECT MAX(s.through_sequence) FROM supervisor_owner_input_snapshots s
            WHERE s.task_id=i.task_id AND s.command_id=i.command_id
              AND s.command_attempt=a.attempt),0))",
        [task_id], |row| row.get(0),
    )?)
}

/// Retire only the finished slice, preserving the same command/task and all
/// completion guards for its next attempt. Never interrupt a running provider.
pub(crate) fn continue_pending(
    root: &Path,
    task_id: &str,
    attempt_id: &str,
) -> Result<bool> {
    let mut conn = open_channel_db(&resolve_db_path(root, None))?;
    attach_queue_projection_store(root, &conn)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if !has_uncaptured(&tx, task_id)? {
        tx.commit()?;
        return Ok(false);
    }
    let current = load_queue_task_from_conn(&tx, task_id)?.context("supervisor input task disappeared")?;
    let snapshot_owner: Option<String> = tx.query_row(
        "SELECT s.lease_worker_id FROM supervisor_owner_input_snapshots s
         JOIN business_command_aggregates a ON a.command_id=s.command_id
         WHERE s.task_id=?1 AND s.attempt_id=?2 AND s.command_attempt=a.attempt",
        params![task_id, attempt_id], |row| row.get(0),
    ).optional()?;
    ensure!(current.route_status == "leased"
        && current.lease_worker_id.as_deref() == snapshot_owner.as_deref()
        && snapshot_owner.is_some()
        && current.lease_expires_at.as_deref().is_some_and(|expires| expires > now_iso_string().as_str()),
        "supervisor input continuation lost its current worker lease");
    ensure!(transition_business_command_for_task_in_transaction(
        &tx, task_id, "pending", None, None, None,
        "Owner input arrived after this slice's context; continue the same task",
    )?, "supervisor input continuation lost its command link");
    let changed = tx.execute(
        "UPDATE worker_attempt_finalizations
         SET queue_effects_applied_at=COALESCE(queue_effects_applied_at,?2),updated_at=?2
         WHERE attempt_id=?1",
        params![attempt_id, now_iso_string()],
    )?;
    ensure!(changed == 1, "supervisor input continuation has no durable worker attempt");
    let tasks = load_queue_projection_tasks(&tx, &[task_id.to_owned()])?;
    refresh_queue_projection_tasks(root, &tx, &tasks)?;
    tx.commit()?;
    Ok(true)
}
