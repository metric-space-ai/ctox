//! Chat lifecycle messages are server projections, never harness-authored chat rows.
#[cfg(test)]
#[path = "harness_cockpit_chat_tests.rs"]
mod tests;
use crate::business_os::{store, store_projections};
use crate::mission::channels;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;

const CHAT_CANDIDATES_SQL: &str = "SELECT c.command_id FROM business_commands c
    LEFT JOIN cockpit_chat_delivery d ON d.command_id=c.command_id
    WHERE c.command_type='business_os.chat.task' AND c.command_id>?1 AND COALESCE(d.terminal,0)=0
      AND (c.status NOT IN ('completed','failed','cancelled') OR c.command_id IN (
        SELECT command_id FROM business_commands
        WHERE command_type='business_os.chat.task' AND status IN ('completed','failed','cancelled')
        ORDER BY observed_at_ms DESC,command_id DESC LIMIT 300))
    ORDER BY c.command_id LIMIT 33";

fn ensure_chat_candidate_indexes(business: &Connection) -> Result<()> {
    business.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_cockpit_chat_candidates
            ON business_commands(command_id)
            WHERE command_type='business_os.chat.task';
         CREATE INDEX IF NOT EXISTS idx_cockpit_chat_terminal
            ON business_commands(observed_at_ms DESC,command_id DESC)
            WHERE command_type='business_os.chat.task'
              AND status IN ('completed','failed','cancelled');",
    )?;
    Ok(())
}

pub(crate) fn trim_messages(messages: &mut Vec<Value>) {
    while messages.len() > 40 {
        let position = messages
            .iter()
            .position(|message| message.get("kind").and_then(Value::as_str) == Some("status"))
            .unwrap_or(0);
        messages.remove(position);
    }
}

// origin/main has no resolve_chat_reply_language helper yet. Explicit persisted
// language/locale wins; the established Business OS chat fallback is German.
fn resolve_chat_reply_language(command: &store::BusinessCommand) -> &'static str {
    let language = [&command.payload, &command.client_context]
        .into_iter()
        .find_map(|value| {
            ["reply_language", "language", "locale"]
                .into_iter()
                .find_map(|key| value.get(key).and_then(Value::as_str))
        })
        .unwrap_or("de");
    if language.to_ascii_lowercase().starts_with("en") {
        "en"
    } else {
        "de"
    }
}

fn text(language: &str, key: &str) -> &'static str {
    match (language, key) {
        ("en", "queued") => {
            "Task added to the CTOX queue. Progress and the reply will appear here."
        }
        (_, "queued") => {
            "Aufgabe in der CTOX Queue angelegt. Fortschritt und Antwort erscheinen hier."
        }
        ("en", "leased") => "Work started.",
        (_, "leased") => "Die Bearbeitung hat begonnen.",
        ("en", "plan") => "Plan updated",
        (_, "plan") => "Plan aktualisiert",
        ("en", "retry_wait") => "The attempt will be retried",
        (_, "retry_wait") => "Der Versuch wird wiederholt",
        ("en", "blocked") => "Work is paused until the cause is resolved",
        (_, "blocked") => "Die Bearbeitung wartet, bis die Ursache behoben ist",
        ("en", "review_rework") => "Review requested changes; the task will be revised",
        (_, "review_rework") => "Die Prüfung verlangt Änderungen; die Aufgabe wird überarbeitet",
        _ => "",
    }
}

pub(crate) fn queued_chat_text(command: &store::BusinessCommand) -> &'static str {
    text(resolve_chat_reply_language(command), "queued")
}

pub(super) fn project(root: &Path, core: &Connection) -> Result<()> {
    let business = store::open_store(root)?;
    let mut writer = store::BusinessProjectionWriter::open(root)?;
    business.busy_timeout(std::time::Duration::from_millis(100))?;
    writer
        .source_connection()
        .busy_timeout(std::time::Duration::from_millis(100))?;
    business.execute_batch("CREATE TABLE IF NOT EXISTS cockpit_chat_delivery(command_id TEXT PRIMARY KEY, fingerprint TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS cockpit_chat_message_delivery(command_id TEXT NOT NULL, message_id TEXT NOT NULL, PRIMARY KEY(command_id,message_id));")?;
    // `terminal` marks a delivery made after the task reached a terminal route
    // status: nothing about that chat changes any more, so later passes skip it
    // before the expensive projection/progress lookups. Without this every pass
    // re-projected all 300 retained terminal chats (thesen 07.09.2026:
    // project_chat 190–345 s per pass, browser replication starved).
    let _ = business.execute(
        "ALTER TABLE cockpit_chat_delivery ADD COLUMN terminal INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = business.execute(
        "ALTER TABLE cockpit_chat_delivery ADD COLUMN source_fingerprint TEXT",
        [],
    );
    business.execute_batch(
        "CREATE TABLE IF NOT EXISTS cockpit_chat_cursor (
        id INTEGER PRIMARY KEY CHECK(id=1), command_id TEXT NOT NULL
    );",
    )?;
    ensure_chat_candidate_indexes(&business)?;
    let cursor: String = business
        .query_row(
            "SELECT command_id FROM cockpit_chat_cursor WHERE id=1",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_default();
    // A queued chat can precede the first LCM turn. Missing optional evidence
    // is empty progress; observing it must never initialize or scan LCM.
    let has_plans: bool = core.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='task_execution_plan_revisions')",
        [], |row| row.get(0),
    )?;
    let has_flow: bool = core.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ctox_harness_flow_events')",
        [], |row| row.get(0),
    )?;
    let started = std::time::Instant::now();
    // A durable round-robin cursor prevents a busy/failed early chat from
    // starving later chats. At most 32 candidates, and 250 ms between items;
    // lock waits have a short timeout (this is not a hard SQL execution deadline).
    {
        let mut statement = business.prepare(CHAT_CANDIDATES_SQL)?;
        let mut commands = statement
            .query_map([&cursor], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if commands.is_empty() && !cursor.is_empty() {
            commands = statement
                .query_map([""], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        let has_more = commands.len() > 32;
        for (position, command_id) in commands.into_iter().take(32).enumerate() {
            if position > 0 && started.elapsed() >= std::time::Duration::from_millis(250) {
                super::projections::schedule_chat_refresh(root);
                break;
            }
            business.execute(
                "INSERT INTO cockpit_chat_cursor(id,command_id) VALUES(1,?1)
                ON CONFLICT(id) DO UPDATE SET command_id=excluded.command_id",
                [&command_id],
            )?;
            let settled: bool = business
                .query_row(
                    "SELECT terminal FROM cockpit_chat_delivery WHERE command_id=?1",
                    [&command_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .is_some_and(|flag| flag != 0);
            if settled {
                continue;
            }
            // Read only lifecycle evidence through the pump's connection. The
            // general command/queue readers initialize LCM and load message
            // bodies; a chat projection must never read the worker transcript.
            let lifecycle = core
                .query_row(
                    "SELECT l.task_id, a.execution_phase, a.result_json,
                        r.route_status, r.attempt, r.hold_reason,
                        r.last_error, r.retry_not_before
                 FROM business_command_task_links l
                 JOIN business_command_aggregates a ON a.command_id=l.command_id
                 JOIN communication_routing_state r ON r.message_key=l.task_id
                 WHERE l.command_id=?1",
                    [&command_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, Option<String>>(6)?,
                            row.get::<_, Option<String>>(7)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                task_id,
                execution_phase,
                result,
                route_status,
                attempt,
                hold_reason,
                status_note,
                retry_not_before,
            )) = lifecycle
            else {
                continue;
            };
            let task_id = task_id.as_str();
            let run_id: Option<String> = if has_flow {
                core.query_row(
                    "SELECT json_extract(metadata_json,'$.attempt_id')
                     FROM ctox_harness_flow_events
                     WHERE message_key=?1 AND json_extract(metadata_json,'$.attempt_id') IS NOT NULL
                     ORDER BY created_at DESC LIMIT 1",
                    [task_id],
                    |row| row.get(0),
                )
                .optional()?
            } else {
                None
            };
            let plan_stamp: Option<(i64, i64)> = if has_plans {
                core.query_row(
                    "SELECT revision,updated_at_ms FROM task_execution_plan_revisions
                     WHERE task_id=?1 AND (?2 IS NULL OR attempt_id=?2)
                     ORDER BY revision DESC LIMIT 1",
                    params![task_id, run_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
            } else {
                None
            };
            let source_fingerprint = channels::stable_digest(&serde_json::to_string(&json!([
                task_id,
                execution_phase,
                result,
                route_status,
                attempt,
                hold_reason,
                status_note,
                retry_not_before,
                run_id,
                plan_stamp
            ]))?);
            let prior_source: Option<String> = business
                .query_row(
                    "SELECT source_fingerprint FROM cockpit_chat_delivery WHERE command_id=?1",
                    [&command_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten();
            if prior_source.as_deref() == Some(&source_fingerprint) {
                continue;
            }
            let command = store::load_business_command(&business, &command_id)?;
            let chat_id = store_projections::business_chat_id(&command, &command_id);
            let raw:Option<String>=business.query_row("SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id=?1 AND deleted=0",[&chat_id],|row|row.get(0)).optional()?;
            let Some(raw) = raw else {
                continue;
            };
            let mut chat: Value = serde_json::from_str(&raw)?;
            let result: Value = result
                .map(|raw| serde_json::from_str(&raw))
                .transpose()?
                .unwrap_or(Value::Null);
            let language = resolve_chat_reply_language(&command);
            let mut additions = Vec::new();
            let terminal = matches!(route_status.as_str(), "handled" | "failed" | "cancelled");
            let status = if !terminal && execution_phase == "retry_wait" {
                "retry_wait"
            } else {
                route_status.as_str()
            };
            let note = hold_reason
                .as_deref()
                .or(status_note.as_deref())
                .unwrap_or("");
            // The durable lease counter also covers a slice that finished before delivery.
            if attempt > 0 {
                additions.push(("status", text(language, "leased").to_string()));
            }
            if matches!(status, "retry_wait" | "blocked" | "review_rework") {
                let mut message = text(language, status).to_string();
                if status != "leased" && !note.is_empty() {
                    message.push_str(": ");
                    message.push_str(note);
                }
                if status == "retry_wait" {
                    if let Some(next) = &retry_not_before {
                        message.push_str(" — ");
                        message.push_str(next);
                    }
                }
                additions.push(("status", message));
            }
            if has_plans {
                // Replay every retained revision, not just the latest coalesced snapshot.
                // Forty is the chat's own message cap; older status messages cannot survive it.
                let mut plans = core.prepare("SELECT revision,steps_json FROM (SELECT revision,steps_json FROM task_execution_plan_revisions WHERE task_id=?1 AND (?2 IS NULL OR attempt_id=?2) ORDER BY revision DESC LIMIT 40) ORDER BY revision")?;
                let plans = plans
                    .query_map(params![task_id, run_id], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                for (revision, steps) in plans {
                    let mut message = format!("{} ({revision})", text(language, "plan"));
                    let steps: Vec<Value> = serde_json::from_str(&steps)?;
                    if let Some((index, step)) = steps.iter().enumerate().find(|(_, step)| {
                        step.get("status").and_then(Value::as_str) == Some("in_progress")
                    }) {
                        let position = index + 1;
                        let step_word = if language == "en" { "Step" } else { "Schritt" };
                        message.push_str(&format!(": {step_word} {position}"));
                        if let Some(title) = step.get("label").and_then(Value::as_str) {
                            message.push_str(" — ");
                            message.push_str(title);
                        }
                    }
                    additions.push(("status", message));
                }
            }
            if !terminal {
                if let Some(message) = result
                    .get("user_message")
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                {
                    let kind =
                        if result.get("requires_input").and_then(Value::as_bool) == Some(true) {
                            "question"
                        } else {
                            "interim"
                        };
                    additions.push((kind, message.to_string()));
                }
            }
            // Receipt survives status-first trimming, so maintenance cannot resurrect old messages.
            // Record it only after delivery to both stores; partial delivery remains retryable.
            let fingerprint = channels::stable_digest(&serde_json::to_string(&json!({
                "task_id":task_id,"attempt":attempt,"run_id":run_id,"messages":additions
            }))?);
            let delivered: Option<String> = business
                .query_row(
                    "SELECT fingerprint FROM cockpit_chat_delivery WHERE command_id=?1",
                    [&command_id],
                    |row| row.get(0),
                )
                .optional()?;
            if delivered.as_deref() == Some(&fingerprint) {
                business.execute(
                    "UPDATE cockpit_chat_delivery SET source_fingerprint=?2 WHERE command_id=?1",
                    params![command_id, source_fingerprint],
                )?;
                if terminal {
                    business.execute(
                        "UPDATE cockpit_chat_delivery SET terminal=1 WHERE command_id=?1",
                        [&command_id],
                    )?;
                }
                continue;
            }
            let Some(messages) = chat.get_mut("messages").and_then(Value::as_array_mut) else {
                continue;
            };
            if terminal {
                for message in messages
                    .iter_mut()
                    .filter(|m| m["kind"] == "reply" && m["command_id"] == command_id)
                {
                    if message.get("run_id").is_none_or(Value::is_null) {
                        message["run_id"] = json!(run_id);
                    }
                }
            }
            let now = chrono::Utc::now().timestamp_millis();
            let mut message_receipts = Vec::new();
            for (kind, message) in additions {
                let digest =
                    channels::stable_digest(&format!("{task_id}:{}:{kind}:{message}", attempt));
                let id = format!("cockpit_{digest}");
                message_receipts.push(id.clone());
                if let Some(existing) = messages
                    .iter_mut()
                    .find(|m| m.get("id").and_then(Value::as_str) == Some(&id))
                {
                    existing["run_id"] = json!(run_id);
                    continue;
                }
                let already_delivered: bool = business.query_row("SELECT EXISTS(SELECT 1 FROM cockpit_chat_message_delivery WHERE command_id=?1 AND message_id=?2)",params![command_id,id], |row|row.get(0))?;
                if already_delivered {
                    continue;
                }
                messages.push(json!({"id":id,"role":"ctox","kind":kind,"text":message,
                    "task_id":task_id,"command_id":command_id,"run_id":run_id,
                    "taskId":task_id,"commandId":command_id,"status":status,"createdAt":now}));
            }
            {
                trim_messages(messages);
                chat["updated_at_ms"] = json!(now);
                writer.upsert_source_projection("business_chats", &chat_id, now, chat)?;
                if writer.delivered_to_rxdb("business_chats") {
                    let tx = business.unchecked_transaction()?;
                    for id in message_receipts {
                        tx.execute("INSERT OR IGNORE INTO cockpit_chat_message_delivery(command_id,message_id) VALUES(?1,?2)",params![command_id,id])?;
                    }
                    tx.execute("INSERT INTO cockpit_chat_delivery(command_id,fingerprint,terminal,source_fingerprint) VALUES(?1,?2,?3,?4) ON CONFLICT(command_id) DO UPDATE SET fingerprint=excluded.fingerprint, terminal=excluded.terminal, source_fingerprint=excluded.source_fingerprint",params![command_id,fingerprint,i64::from(terminal),source_fingerprint])?;
                    tx.commit()?;
                }
            }
        }
        if has_more {
            super::projections::schedule_chat_refresh(root);
        }
    }
    business.execute("DELETE FROM cockpit_chat_delivery WHERE command_id NOT IN (SELECT command_id FROM business_commands WHERE command_type='business_os.chat.task' AND (status NOT IN ('completed','failed','cancelled') OR command_id IN (SELECT command_id FROM business_commands WHERE command_type='business_os.chat.task' AND status IN ('completed','failed','cancelled') ORDER BY observed_at_ms DESC,command_id DESC LIMIT 300)))",[])?;
    business.execute("DELETE FROM cockpit_chat_message_delivery WHERE command_id NOT IN (SELECT command_id FROM cockpit_chat_delivery)",[])?;
    Ok(())
}
