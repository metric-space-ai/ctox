// Origin: CTOX
// License: AGPL-3.0-only
//! Opt-in, read-only facts from the canonical supervisor task's native ledger.
//! No execution identity is derived from a queue ordinal or a desktop session.
use super::super::workjet_supervisor_execution_contract as wire;
use super::*;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use wire::WireValidate;

const EVENT_KINDS: &str = "'worker.turn_started','worker.tool_started','worker.tool_completed',
 'worker.thinking_started','worker.thinking','worker.plan_updated','worker.token_usage',
 'worker.turn_completed','worker.phase','crew.memory_read','crew.learning'";
fn table(conn: &Connection, name: &str) -> anyhow::Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |r| r.get(0),
    )?)
}
fn millis(value: &str) -> anyhow::Result<i64> {
    Ok(chrono::DateTime::parse_from_rfc3339(value)?.timestamp_millis())
}
fn ordinal(value: Option<i64>) -> anyhow::Result<Option<u64>> {
    value
        .map(|value| u64::try_from(value).context("negative native attempt ordinal"))
        .transpose()
}

/// Called only after owned_turn verifies the current registered supervisor,
/// native owner, admitted envelope and canonical Core command/task link.
pub(super) fn page(
    root: &Path,
    owned_turn: &Value,
    request: &wire::ExecutionPageRequest,
) -> anyhow::Result<wire::ExecutionPage> {
    request.validate().map_err(anyhow::Error::msg)?;
    let command_id = owned_turn["command_id"]
        .as_str()
        .context("authorized native command missing")?;
    let task_id = owned_turn["task_id"]
        .as_str()
        .context("authorized native task missing")?;
    let mut result = wire::ExecutionPage {
        command_id: command_id.into(),
        task_id: task_id.into(),
        attempt: None,
        events: vec![],
        next_cursor: None,
        has_more: false,
    };
    let mut conn = Connection::open_with_flags(
        crate::paths::core_db(root),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    // A DEFERRED read snapshot, without schema initialization or a writer fence.
    let tx = conn.transaction()?;
    if !table(&tx, "ctox_harness_flow_events")? {
        ensure!(
            request.attempt_id.is_none() && request.cursor.is_none(),
            "native attempt is unavailable to this supervisor task"
        );
        return Ok(result);
    }
    let selection = format!(
        "SELECT json_extract(metadata_json,'$.attempt_id'),
        COALESCE(attempt_index,json_extract(metadata_json,'$.attempt'))
        FROM ctox_harness_flow_events WHERE message_key=?1
          AND event_kind IN ({EVENT_KINDS})
          AND length(trim(COALESCE(json_extract(metadata_json,'$.attempt_id'),'')))>0
          AND (?2 IS NULL OR json_extract(metadata_json,'$.attempt_id')=?2)
        ORDER BY rowid DESC LIMIT 1"
    );
    let selected: Option<(String, Option<i64>)> = tx
        .query_row(
            &selection,
            rusqlite::params![task_id, request.attempt_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((attempt_id, attempt_index)) = selected else {
        ensure!(
            request.attempt_id.is_none() && request.cursor.is_none(),
            "native attempt is unavailable to this supervisor task"
        );
        return Ok(result);
    };
    let started: Option<String> = tx.query_row(
        "SELECT MIN(created_at) FROM ctox_harness_flow_events
       WHERE message_key=?1 AND event_kind='worker.turn_started'
         AND json_extract(metadata_json,'$.attempt_id')=?2",
        rusqlite::params![task_id, attempt_id],
        |r| r.get(0),
    )?;
    let run: Option<(String, Option<String>)> = if table(&tx, "worker_attempt_finalizations")? {
        tx.query_row(
            "SELECT status,terminal_at FROM worker_attempt_finalizations WHERE attempt_id=?1",
            [&attempt_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
    } else {
        None
    };
    result.attempt = Some(wire::AttemptRef {
        attempt_id: attempt_id.clone(),
        // ctox_runs.id is this persisted finalization key. Before that row
        // exists, no run_id is claimed, even though the attempt is real.
        run_id: run.as_ref().map(|_| attempt_id.clone()),
        attempt_index: ordinal(attempt_index)?,
        status: run.as_ref().map(|r| r.0.clone()),
        started_at_ms: started.as_deref().map(millis).transpose()?,
        finished_at_ms: run
            .as_ref()
            .and_then(|r| r.1.as_deref())
            .map(millis)
            .transpose()?,
    });
    let after = if let Some(cursor) = &request.cursor {
        let anchored: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM ctox_harness_flow_events
          WHERE rowid=?1 AND event_id=?2 AND message_key=?3
            AND json_extract(metadata_json,'$.attempt_id')=?4)",
            rusqlite::params![
                cursor.after_sequence,
                cursor.after_event_id,
                task_id,
                attempt_id
            ],
            |r| r.get(0),
        )?;
        ensure!(anchored,"native event cursor is unavailable or belongs to another attempt; restart this attempt's page");
        cursor.after_sequence
    } else {
        0
    };
    let limit = request.limit.unwrap_or(25);
    let sql = format!(
        "SELECT rowid,event_id,event_kind,substr(title,1,256),created_at,
          json_extract(metadata_json,'$.tool.name'),json_extract(metadata_json,'$.tool.call_id'),
          json_extract(metadata_json,'$.tool.success')
        FROM ctox_harness_flow_events WHERE message_key=?1
          AND json_extract(metadata_json,'$.attempt_id')=?2 AND rowid>?3
          AND COALESCE(json_extract(metadata_json,'$.cockpit_eligible'),1)=1
          AND event_kind IN ({EVENT_KINDS}) ORDER BY rowid LIMIT ?4"
    );
    let mut statement = tx.prepare(&sql)?;
    let rows = statement
        .query_map(
            rusqlite::params![task_id, attempt_id, after, limit + 1],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<bool>>(7)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    result.has_more = rows.len() > limit as usize;
    for (sequence, id, kind, title, created, tool_name, call_id, success) in
        rows.into_iter().take(limit as usize)
    {
        result.events.push(wire::ExecutionEvent {
            id,
            sequence: u64::try_from(sequence)?,
            kind,
            title,
            created_at_ms: millis(&created)?,
            tool_name,
            call_id,
            success,
        });
    }
    result.next_cursor = result
        .events
        .last()
        .map(|event| wire::EventCursor {
            after_sequence: event.sequence,
            after_event_id: event.id.clone(),
        })
        .or_else(|| request.cursor.clone());
    result.validate().map_err(anyhow::Error::msg)?;
    drop(statement);
    tx.commit()?;
    Ok(result)
}
