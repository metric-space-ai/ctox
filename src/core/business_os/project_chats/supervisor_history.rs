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

/// The same admitted-envelope and task-link fences as owned_turn, borrowing
/// the held Core read snapshot instead of opening a migrating channel writer.
fn authorized_task(
    core: &Connection, business: &Connection, owner: &str,
    binding: &supervisor_binding::SupervisorBinding, id: &str,
) -> anyhow::Result<String> {
    let sizes: (i64,i64) = business.query_row(
        "SELECT length(CAST(payload_json AS BLOB)), length(CAST(client_context_json AS BLOB))
            FROM business_commands WHERE command_id=?1", [id],
        |r| Ok((r.get(0)?,r.get(1)?)),
    )?;
    ensure!(sizes.0<=65536 && sizes.1<=65536,
        "admitted history envelope exceeds the bounded reader");
    let admitted = store::load_business_command(business,id)?;
    let (canonical, task_id, message_key, thread_key, channel, direction, message_command):
        (String,String,Option<String>,Option<String>,Option<String>,Option<String>,Option<String>) =
        core.query_row(
            "SELECT substr(json_extract(a.intent_json,'$.payload'),1,65537),
                l.task_id,m.message_key,m.thread_key,m.channel,m.direction,
                json_extract(m.metadata_json,'$.business_os_command_id')
             FROM business_command_aggregates a
             JOIN business_command_task_links l ON l.command_id=a.command_id
             LEFT JOIN communication_messages m ON m.message_key=l.task_id
             WHERE a.command_id=?1", [id],
             |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)),
        )?;
    ensure!(canonical.len()<=65536,"native history intent exceeds the bounded reader");
    let canonical: Value = serde_json::from_str(&canonical)?;
    ensure!(
        admitted.module=="ctox" && admitted.command_type=="business_os.chat.task"
        && admitted.record_id.as_deref()==Some(binding.project_id.as_str())
        && admitted.payload["thread_id"]==binding.thread_id
        && admitted.payload["thread_key"]==binding.thread_key
        && admitted.payload["risk_class"]=="internal"
        && admitted.client_context.pointer("/actor/id").and_then(Value::as_str)==Some(owner)
        && admitted.payload==canonical,
        "turn does not belong to this Owner's admitted Supervisor"
    );
    ensure!(
        message_key.as_deref()==Some(task_id.as_str())
        && thread_key.as_deref()==Some(binding.thread_key.as_str())
        && channel.as_deref()==Some("queue") && direction.as_deref()==Some("inbound")
        && message_command.as_deref()==Some(id),
        "Supervisor history has a conflicting native queue link"
    );
    Ok(task_id)
}

pub(super) fn history(root: &Path, owner: &str, payload: Value) -> anyhow::Result<Value> {
    let request: HistoryPayload = serde_json::from_value(payload)?;
    let binding = supervisor_turns::binding(root,owner,&request.project_id,&request.thread_id,false)?;
    let page = request.history_page.unwrap_or(wire::TurnHistoryRequest {cursor:None,limit:None});
    page.validate().map_err(anyhow::Error::msg)?;
    let limit = page.limit.unwrap_or(10);
    let business = open_store(root)?;
    let mut conn = Connection::open_with_flags(crate::paths::core_db(root),OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let tx = conn.transaction()?;
    let scope = "module='ctox' AND command_type='business_os.chat.task'
        AND record_id=?1
        AND json_extract(intent_json,'$.payload.thread_id')=?2
        AND json_extract(intent_json,'$.payload.thread_key')=?3
        AND json_extract(intent_json,'$.payload.risk_class')='internal'
        AND json_extract(intent_json,'$.client_context.actor.id')=?4";
    if let Some(cursor)=&page.cursor {
        let anchor: Option<i64> = tx.query_row(
            &format!("SELECT created_at_ms FROM business_command_aggregates
                WHERE {scope} AND command_id=?5"),
            params![binding.project_id,binding.thread_id,binding.thread_key,owner,cursor.before_command_id],
            |r| r.get(0),
        ).optional()?;
        ensure!(anchor==Some(cursor.before_created_at_ms),
            "history cursor is not an admitted turn of this Owner's Supervisor");
        authorized_task(&tx,&business,owner,&binding,&cursor.before_command_id)?;
    }
    let before_ms=page.cursor.as_ref().map(|c| c.before_created_at_ms);
    let before_id=page.cursor.as_ref().map(|c| c.before_command_id.as_str());
    let entries = {
        let mut statement=tx.prepare(&format!(
            "SELECT command_id,created_at_ms,
                substr(json_extract(intent_json,'$.payload.user_message'),1,4096),
                length(json_extract(intent_json,'$.payload.user_message'))>4096
             FROM business_command_aggregates WHERE {scope}
                AND (?5 IS NULL OR created_at_ms<?5 OR (created_at_ms=?5 AND command_id<?6))
             ORDER BY created_at_ms DESC,command_id DESC LIMIT ?7"))?;
        let rows=statement.query_map(
            params![binding.project_id,binding.thread_id,binding.thread_key,owner,before_ms,before_id,limit+1],
            |r| Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?)),
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut turns=Vec::with_capacity(entries.len());
    for (command_id,created_at_ms,user_text,user_text_truncated) in entries {
        let task_id=authorized_task(&tx,&business,owner,&binding,&command_id)?;
        turns.push(wire::TurnHistoryEntry {command_id,task_id,created_at_ms,user_text,user_text_truncated});
    }
    tx.commit()?;
    let has_more=turns.len()>limit as usize;
    turns.truncate(limit as usize);
    let next_cursor=if has_more {turns.last().map(|t| wire::TurnHistoryCursor {
        before_created_at_ms:t.created_at_ms,before_command_id:t.command_id.clone(),
    })} else {None};
    let current=supervisor_turns::binding(root,owner,&request.project_id,&request.thread_id,false)?;
    ensure!(current.thread_key==binding.thread_key,"Supervisor history binding changed during the read");
    let result=wire::TurnHistoryPage {
        project_id:binding.project_id.clone(),thread_id:binding.thread_id.clone(),
        thread_key:binding.thread_key.clone(),turns,next_cursor,has_more,
    };
    result.validate().map_err(anyhow::Error::msg)?;
    Ok(json!({"ok":true,"contract":CONTRACT,"history_contract":CONTRACT,
        "binding":binding,"history_page":result}))
}
