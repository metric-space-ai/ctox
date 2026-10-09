// Origin: CTOX
// License: AGPL-3.0-only
//! Enumerate admitted Supervisor turns without replaying prompts or copying replies.
use super::super::workjet_supervisor_execution_contract as wire;
use super::*;
use rusqlite::{OpenFlags, OptionalExtension};
use wire::WireValidate;

pub(super) const CONTRACT: &str = "ctox.workjet.supervisor_history.v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HistoryPayload {
    project_id: String,
    thread_id: String,
    #[serde(default)]
    history_page: Option<wire::TurnHistoryRequest>,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

pub(super) fn history(
    root: &Path,
    owner: &str,
    payload: Value,
) -> anyhow::Result<Value> {
    let request: HistoryPayload = serde_json::from_value(payload)?;
    let binding = supervisor_turns::binding(
        root, owner, &request.project_id, &request.thread_id, false,
    )?;
    let page = request.history_page.unwrap_or(wire::TurnHistoryRequest {
        cursor: None, limit: None,
    });
    page.validate().map_err(anyhow::Error::msg)?;
    let limit = page.limit.unwrap_or(10);
    let mut conn = Connection::open_with_flags(
        crate::paths::core_db(root), OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let tx = conn.transaction()?;
    // These are the native admitted intents, not the redacted audit projection.
    // Every selected command is rechecked against owned_turn below.
    let scope = "module='ctox' AND command_type='business_os.chat.task'
        AND record_id=?1
        AND json_extract(intent_json,'$.payload.thread_id')=?2
        AND json_extract(intent_json,'$.payload.thread_key')=?3
        AND json_extract(intent_json,'$.payload.risk_class')='internal'
        AND json_extract(intent_json,'$.client_context.actor.id')=?4";
    if let Some(cursor) = &page.cursor {
        let anchor: Option<i64> = tx.query_row(
            &format!("SELECT created_at_ms FROM business_command_aggregates
                WHERE {scope} AND command_id=?5"),
            params![binding.project_id, binding.thread_id, binding.thread_key,
                owner, cursor.before_command_id],
            |r| r.get(0),
        ).optional()?;
        ensure!(anchor == Some(cursor.before_created_at_ms),
            "history cursor is not an admitted turn of this Owner's Supervisor");
        supervisor_turns::owned_turn(root, owner, &binding, &cursor.before_command_id)?;
    }
    let before_ms = page.cursor.as_ref().map(|c| c.before_created_at_ms);
    let before_id = page.cursor.as_ref().map(|c| c.before_command_id.as_str());
    let entries = {
        let sql = format!("SELECT command_id, created_at_ms,
                substr(json_extract(intent_json,'$.payload.user_message'),1,4096),
                length(json_extract(intent_json,'$.payload.user_message'))>4096
            FROM business_command_aggregates WHERE {scope}
                AND (?5 IS NULL OR created_at_ms<?5
                    OR (created_at_ms=?5 AND command_id<?6))
            ORDER BY created_at_ms DESC, command_id DESC LIMIT ?7");
        let mut statement = tx.prepare(&sql)?;
        let rows = statement.query_map(
            params![binding.project_id,binding.thread_id,binding.thread_key,
                owner,before_ms,before_id,limit+1],
            |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,
                row.get::<_,String>(2)?,row.get::<_,bool>(3)?)),
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    tx.commit()?;
    let mut turns = Vec::with_capacity(entries.len());
    for (command_id, created_at_ms, user_text, user_text_truncated) in entries {
        let turn = supervisor_turns::owned_turn(root, owner, &binding, &command_id)?;
        turns.push(wire::TurnHistoryEntry {
            command_id,
            task_id: turn["task_id"].as_str()
                .context("admitted history turn has no native task")?.to_owned(),
            created_at_ms, user_text, user_text_truncated,
        });
    }
    let has_more = turns.len() > limit as usize;
    turns.truncate(limit as usize);
    let next_cursor = if has_more {
        turns.last().map(|turn| wire::TurnHistoryCursor {
            before_created_at_ms: turn.created_at_ms,
            before_command_id: turn.command_id.clone(),
        })
    } else { None };
    // A removed/replaced binding must not deliver the old thread's history.
    let current = supervisor_turns::binding(
        root, owner, &request.project_id, &request.thread_id, false,
    )?;
    ensure!(current.thread_key == binding.thread_key,
        "Supervisor history binding changed during the read");
    let result = wire::TurnHistoryPage {
        project_id: binding.project_id.clone(), thread_id: binding.thread_id.clone(),
        thread_key: binding.thread_key.clone(), turns, next_cursor, has_more,
    };
    result.validate().map_err(anyhow::Error::msg)?;
    Ok(json!({"ok":true,"contract":CONTRACT,"history_contract":CONTRACT,
        "binding":binding,"history_page":result}))
}
