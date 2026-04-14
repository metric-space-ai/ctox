// Origin: CTOX
// License: Apache-2.0
//
// Proactive reminder / approval-chase for open approval-gate self-work
// items.
//
// Normal (non-benchmark) runs leave `CTOX_AUTO_APPROVE_GATES` unset,
// which means CTOX creates approval-gate self-work items and then
// stops. Without this module the owner would never be pinged and the
// gate would sit forever. This module:
//
// 1. discovers new open approval-gate items and schedules a nag
//    cadence for each;
// 2. on each mission-watcher tick, sends the next due nag over the
//    configured owner channel (email first, Jami as escalation);
// 3. embeds a stable `[ctox-approve:<work_id>]` tag in the subject so
//    the owner can reply to the mail with `APPROVE` or `REJECT` and
//    have the gate auto-closed without opening the TUI;
// 4. scans recent inbound messages for those tags and closes / marks
//    failed the matching gate when a structured reply arrives;
// 5. honours a per-gate `approval_modality = "tui-only"` marker stored
//    in the gate's `metadata_json` — those gates still get a complete
//    info mail, but the mail asks the owner to confirm in the local
//    TUI ("security policy: this action is high-impact enough that an
//    email reply is not accepted"), and the inbound parser ignores
//    APPROVE replies for them.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::inference::runtime_env;
use crate::mission::tickets;

const DB_RELATIVE_PATH: &str = "runtime/cto_agent.db";

#[derive(Debug, Default, Clone)]
pub struct NagSweepSummary {
    pub scheduled: usize,
    pub sent: usize,
    pub completed: usize,
    pub replies_processed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalModality {
    EmailReply,
    TuiOnly,
}

impl ApprovalModality {
    fn from_item(item: &tickets::TicketSelfWorkItemView) -> Self {
        let marker = item
            .metadata
            .get("approval_modality")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if marker.eq_ignore_ascii_case("tui-only")
            || marker.eq_ignore_ascii_case("tui_only")
        {
            Self::TuiOnly
        } else {
            Self::EmailReply
        }
    }
}

/// Entry point called once per mission-watcher tick (outside the
/// auto-approve code path — see `service.rs`).
pub fn sweep(root: &Path) -> Result<NagSweepSummary> {
    ensure_schema(root)?;
    let mut summary = NagSweepSummary::default();
    summary.completed = mark_closed_gates_completed(root)?;
    summary.scheduled = schedule_new_gates(root)?;
    summary.sent = send_due_nags(root)?;
    summary.replies_processed = parse_inbound_approval_replies(root)?;
    Ok(summary)
}

fn db_path(root: &Path) -> PathBuf {
    root.join(DB_RELATIVE_PATH)
}

fn open_db(root: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path(root))
        .with_context(|| format!("open {}", db_path(root).display()))?;
    Ok(conn)
}

fn ensure_schema(root: &Path) -> Result<()> {
    let conn = open_db(root)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS ticket_approval_nag_state (
            work_id TEXT PRIMARY KEY,
            attempt_count INTEGER NOT NULL DEFAULT 0,
            first_seen_at TEXT NOT NULL,
            last_nag_at TEXT,
            next_nag_at TEXT NOT NULL,
            last_channel TEXT,
            completed_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_ticket_approval_nag_state_due
            ON ticket_approval_nag_state(next_nag_at)
            WHERE completed_at IS NULL;
        "#,
    )?;
    Ok(())
}

fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

fn add_offset_rfc3339(base_iso: &str, offset_seconds: i64) -> String {
    let base = chrono::DateTime::parse_from_rfc3339(base_iso)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());
    let target = base + chrono::Duration::seconds(offset_seconds);
    target.to_rfc3339()
}

/// Active escalation cadence for the current autonomy level, read
/// fresh on each use so the TUI can change it mid-run. Reminders only,
/// never alerts — the cadence is tuned for an owner who reads email a
/// few times per workday, not for minute-by-minute response. Empty
/// schedule means no nagging (progressive level auto-closes gates
/// instead).
fn nag_schedule() -> &'static [(i64, &'static str)] {
    crate::autonomy::AutonomyLevel::from_env().nag_cadence_seconds()
}

fn schedule_new_gates(root: &Path) -> Result<usize> {
    let conn = open_db(root)?;
    let open_gates = tickets::list_ticket_self_work_items(root, None, Some("open"), 256)?;
    let mut scheduled = 0usize;
    for item in open_gates {
        if item.kind != "approval-gate" {
            continue;
        }
        // Skip if we already track this gate.
        let existing: Option<String> = conn
            .query_row(
                "SELECT work_id FROM ticket_approval_nag_state WHERE work_id = ?1",
                params![&item.work_id],
                |row| row.get(0),
            )
            .optional()?;
        if existing.is_some() {
            continue;
        }
        let now = now_rfc3339();
        let Some(first) = nag_schedule().first() else {
            // Progressive autonomy has no nag schedule — nothing to do.
            continue;
        };
        let next = add_offset_rfc3339(&now, first.0);
        conn.execute(
            r#"
            INSERT INTO ticket_approval_nag_state
              (work_id, attempt_count, first_seen_at, last_nag_at, next_nag_at, last_channel, completed_at)
            VALUES (?1, 0, ?2, NULL, ?3, NULL, NULL)
            "#,
            params![&item.work_id, now, next],
        )?;
        scheduled += 1;
    }
    Ok(scheduled)
}

fn mark_closed_gates_completed(root: &Path) -> Result<usize> {
    let conn = open_db(root)?;
    let now = now_rfc3339();
    // Any nag state whose gate is no longer open gets its completed_at set.
    let changed = conn.execute(
        r#"
        UPDATE ticket_approval_nag_state
        SET completed_at = ?1
        WHERE completed_at IS NULL
          AND work_id IN (
            SELECT work_id FROM ticket_self_work_items
            WHERE state != 'open' AND kind = 'approval-gate'
          )
        "#,
        params![now],
    )?;
    Ok(changed)
}

/// Business-hours window — reminders are only sent Mon–Fri 08:00–20:00
/// in the host's local time. Weekend and out-of-hours ticks defer their
/// due nags to the next window start instead of pinging the owner at
/// 03:00.
fn is_within_business_hours(now: &chrono::DateTime<chrono::Local>) -> bool {
    use chrono::{Datelike, Timelike, Weekday};
    let is_weekday = !matches!(now.weekday(), Weekday::Sat | Weekday::Sun);
    let hour = now.hour();
    is_weekday && (8..20).contains(&hour)
}

/// Next business-hour window start in local time (Mon–Fri 08:00). Used
/// to reschedule due-but-out-of-hours nags so the owner's inbox stays
/// clean overnight and at weekends.
fn next_business_window_start(
    after: &chrono::DateTime<chrono::Local>,
) -> chrono::DateTime<chrono::Local> {
    use chrono::{Datelike, Duration, Timelike, Weekday};
    let mut probe = *after;
    for _ in 0..14 {
        let weekday = probe.weekday();
        let hour = probe.hour();
        if matches!(weekday, Weekday::Sat | Weekday::Sun) {
            let days_to_monday = if weekday == Weekday::Sat { 2 } else { 1 };
            let next = (probe + Duration::days(days_to_monday))
                .with_hour(8)
                .and_then(|dt| dt.with_minute(0))
                .and_then(|dt| dt.with_second(0));
            if let Some(dt) = next {
                probe = dt;
                continue;
            }
            break;
        }
        if hour < 8 {
            if let Some(dt) = probe
                .with_hour(8)
                .and_then(|dt| dt.with_minute(0))
                .and_then(|dt| dt.with_second(0))
            {
                probe = dt;
                continue;
            }
            break;
        }
        if hour >= 20 {
            let next = (probe + Duration::days(1))
                .with_hour(8)
                .and_then(|dt| dt.with_minute(0))
                .and_then(|dt| dt.with_second(0));
            if let Some(dt) = next {
                probe = dt;
                continue;
            }
            break;
        }
        return probe;
    }
    *after + Duration::hours(12)
}

fn send_due_nags(root: &Path) -> Result<usize> {
    let conn = open_db(root)?;
    let now = now_rfc3339();

    // Skip sending outside business hours (Mon–Fri 08:00–20:00 local)
    // and defer the due nags to the next window start so the owner's
    // inbox stays quiet overnight and on weekends. Operators can opt
    // out by exporting `CTOX_APPROVAL_NAG_24_7=1` (useful for on-call
    // setups or tests).
    let ignore_hours = std::env::var("CTOX_APPROVAL_NAG_24_7")
        .map(|value| matches!(value.trim(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false);
    let now_local = chrono::Local::now();
    if !ignore_hours && !is_within_business_hours(&now_local) {
        let next_local = next_business_window_start(&now_local);
        let next_iso = next_local.with_timezone(&chrono::Utc).to_rfc3339();
        // Push any currently-due nag to the next business-window start,
        // but never backwards.
        let _ = conn.execute(
            r#"
            UPDATE ticket_approval_nag_state
            SET next_nag_at = ?1
            WHERE completed_at IS NULL AND next_nag_at <= ?2 AND next_nag_at < ?1
            "#,
            params![&next_iso, &now],
        );
        return Ok(0);
    }

    let mut statement = conn.prepare(
        r#"
        SELECT work_id, attempt_count, first_seen_at
        FROM ticket_approval_nag_state
        WHERE completed_at IS NULL AND next_nag_at <= ?1
        ORDER BY next_nag_at ASC
        LIMIT 16
        "#,
    )?;
    let due: Vec<(String, i64, String)> = statement
        .query_map(params![&now], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(statement);

    let mut sent = 0usize;
    for (work_id, attempt_count, first_seen) in due {
        let item = match tickets::load_ticket_self_work_item(root, &work_id)? {
            Some(view) => view,
            None => continue,
        };
        if item.state != "open" || item.kind != "approval-gate" {
            // Gate disappeared or was already closed — mark nag complete.
            let _ = conn.execute(
                "UPDATE ticket_approval_nag_state SET completed_at = ?1 WHERE work_id = ?2",
                params![&now, &work_id],
            );
            continue;
        }
        let attempt = attempt_count as usize;
        let (channel, subject, body) = compose_nag(&item, attempt);
        match send_via_channel(root, &channel, &subject, &body, &item) {
            Ok(()) => {
                sent += 1;
                let next_attempt = attempt + 1;
                let next_iso = if next_attempt >= nag_schedule().len() {
                    // Stop nagging. Mark as completed so we stop re-sending.
                    let _ = conn.execute(
                        "UPDATE ticket_approval_nag_state SET completed_at = ?1, attempt_count = ?2, last_nag_at = ?1, last_channel = ?3 WHERE work_id = ?4",
                        params![&now, next_attempt as i64, &channel, &work_id],
                    );
                    continue;
                } else {
                    add_offset_rfc3339(&first_seen, nag_schedule()[next_attempt].0)
                };
                conn.execute(
                    r#"
                    UPDATE ticket_approval_nag_state
                    SET attempt_count = ?1,
                        last_nag_at = ?2,
                        last_channel = ?3,
                        next_nag_at = ?4
                    WHERE work_id = ?5
                    "#,
                    params![next_attempt as i64, &now, &channel, &next_iso, &work_id],
                )?;
            }
            Err(err) => {
                // On send failure, delay next attempt by 15 min and keep
                // the attempt counter unchanged so we retry the same step.
                let retry_iso = add_offset_rfc3339(&now, 15 * 60);
                let _ = conn.execute(
                    "UPDATE ticket_approval_nag_state SET next_nag_at = ?1 WHERE work_id = ?2",
                    params![&retry_iso, &work_id],
                );
                eprintln!(
                    "approval_nag: send failed for work_id={} channel={}: {}",
                    work_id, channel, err
                );
            }
        }
    }
    Ok(sent)
}

fn compose_nag(
    item: &tickets::TicketSelfWorkItemView,
    attempt: usize,
) -> (String, String, String) {
    let channel = nag_schedule()
        .get(attempt)
        .map(|(_, c)| c.to_string())
        .unwrap_or_else(|| "email".to_string());
    let modality = ApprovalModality::from_item(item);
    let attempt_label = match attempt {
        0 => "".to_string(),
        1 => "Reminder — ".to_string(),
        2 => "Still pending — ".to_string(),
        _ => "Last reminder — ".to_string(),
    };
    let subject = format!(
        "[ctox-approve:{}] {}{}",
        item.work_id, attempt_label, item.title
    );
    let body = compose_body(item, modality, &channel);
    (channel, subject, body)
}

fn compose_body(
    item: &tickets::TicketSelfWorkItemView,
    modality: ApprovalModality,
    channel: &str,
) -> String {
    let mut out = String::new();
    out.push_str("CTOX is waiting on your approval.\n\n");
    out.push_str("Title:\n");
    out.push_str(&item.title);
    out.push_str("\n\nDetails:\n");
    out.push_str(&item.body_text);
    out.push_str("\n\n");
    out.push_str("Gate reference: ");
    out.push_str(&item.work_id);
    out.push_str("\nFirst seen: ");
    out.push_str(&item.created_at);
    out.push_str("\nChannel: ");
    out.push_str(channel);
    out.push_str("\n\n");

    match modality {
        ApprovalModality::EmailReply => {
            out.push_str(
                "How to respond:\n\
                 - To approve, simply reply to this email with the word APPROVE on its own line (or anywhere in the body).\n\
                 - To reject, reply with REJECT on its own line.\n\
                 - To do nothing, ignore this email; you will be reminded a few more times and then left alone.\n\n\
                 The subject's [ctox-approve:...] tag is how CTOX matches your reply to this gate — please keep it intact.\n"
            );
        }
        ApprovalModality::TuiOnly => {
            out.push_str(
                "Security policy: this action is high-impact enough that an email reply is not accepted as approval.\n\
                 To approve or reject, please open the local CTOX TUI on the host and act on this gate there, or run on the host:\n\
                   ctox ticket self-work-set-state --work-id ");
            out.push_str(&item.work_id);
            out.push_str(" --state closed     # to approve\n   ctox ticket self-work-set-state --work-id ");
            out.push_str(&item.work_id);
            out.push_str(" --state failed     # to reject\n\n\
                 This email contains the full request so you can decide without opening the TUI first; the TUI step is only for the final confirmation.\n");
        }
    }
    out
}

fn send_via_channel(
    root: &Path,
    channel: &str,
    subject: &str,
    body: &str,
    item: &tickets::TicketSelfWorkItemView,
) -> Result<()> {
    let settings = runtime_env::effective_operator_env_map(root).unwrap_or_default();
    let owner_email = settings
        .get("CTOX_OWNER_EMAIL_ADDRESS")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let ctox_bin = std::env::current_exe()
        .or_else(|_| -> std::io::Result<PathBuf> {
            Ok(PathBuf::from("ctox"))
        })
        .unwrap_or_else(|_| PathBuf::from("ctox"));

    let thread_key = format!("approval-nag:{}", item.work_id);

    let mut cmd = Command::new(&ctox_bin);
    cmd.arg("channel").arg("send").arg("--channel").arg(channel);

    match channel {
        "email" => {
            let to = owner_email.ok_or_else(|| {
                anyhow::anyhow!("cannot send email nag: CTOX_OWNER_EMAIL_ADDRESS not set")
            })?;
            let from = settings
                .get("CTO_EMAIL_ADDRESS")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("cannot send email nag: CTO_EMAIL_ADDRESS not configured")
                })?;
            cmd.arg("--account-key")
                .arg(format!("email:{}", from))
                .arg("--thread-key")
                .arg(&thread_key)
                .arg("--to")
                .arg(&to)
                .arg("--subject")
                .arg(subject)
                .arg("--body")
                .arg(body);
        }
        "jami" => {
            let account = settings
                .get("CTO_JAMI_ACCOUNT_ID")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("cannot send jami nag: CTO_JAMI_ACCOUNT_ID not configured")
                })?;
            let owner_jami = settings
                .get("CTOX_OWNER_JAMI_ID")
                .or_else(|| settings.get("CTOX_OWNER_JAMI"))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            cmd.arg("--account-key").arg(format!("jami:{}", account));
            cmd.arg("--thread-key").arg(&thread_key);
            if let Some(owner_jami) = owner_jami {
                cmd.arg("--to").arg(owner_jami);
            }
            cmd.arg("--subject").arg(subject).arg("--body").arg(body);
        }
        other => {
            anyhow::bail!("approval-nag: unsupported channel `{}`", other);
        }
    }

    let output = cmd
        .output()
        .with_context(|| format!("spawn ctox channel send for {}", channel))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "ctox channel send ({}) exited with status {}: {}",
            channel,
            output.status,
            stderr
        );
    }
    Ok(())
}

/// Scan recent inbound messages for an `[ctox-approve:<work_id>]` subject
/// tag. If the body contains APPROVE / REJECT on its own and the gate is
/// still open and is not `tui-only`, close / fail the gate and mark the
/// nag state complete. Returns the number of replies processed.
fn parse_inbound_approval_replies(root: &Path) -> Result<usize> {
    // Look back 7 days to catch delayed replies.
    let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
    let cutoff_iso = cutoff.to_rfc3339();

    let conn = open_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT message_key, subject, body_text, channel
        FROM communication_messages
        WHERE direction = 'inbound'
          AND (observed_at IS NULL OR observed_at > ?1)
          AND subject LIKE '%[ctox-approve:%'
          AND (
            SELECT route_status FROM communication_routing_state
            WHERE communication_routing_state.message_key = communication_messages.message_key
          ) IS NOT 'approval-nag-handled'
        ORDER BY rowid DESC
        LIMIT 32
        "#,
    )?;

    let rows: Vec<(String, String, String, String)> = statement
        .query_map(params![&cutoff_iso], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(statement);

    let mut processed = 0usize;
    for (message_key, subject, body, _channel) in rows {
        let work_id = match extract_work_id(&subject) {
            Some(id) => id,
            None => continue,
        };
        let action = classify_reply_action(&body);
        let Some(action) = action else {
            continue;
        };
        let item = match tickets::load_ticket_self_work_item(root, &work_id)? {
            Some(i) if i.kind == "approval-gate" && i.state == "open" => i,
            _ => continue,
        };
        let modality = ApprovalModality::from_item(&item);
        if modality == ApprovalModality::TuiOnly {
            // tui-only gates do NOT accept email replies as approval.
            // Mark the reply as acknowledged so we stop reprocessing it.
            let _ = mark_reply_handled(&conn, &message_key);
            continue;
        }
        let new_state = match action {
            ReplyAction::Approve => "closed",
            ReplyAction::Reject => "failed",
        };
        match tickets::set_ticket_self_work_state(root, &work_id, new_state) {
            Ok(_) => {
                processed += 1;
                let _ = mark_reply_handled(&conn, &message_key);
            }
            Err(err) => {
                eprintln!(
                    "approval_nag: failed to set state {} on {}: {}",
                    new_state, work_id, err
                );
            }
        }
    }
    Ok(processed)
}

enum ReplyAction {
    Approve,
    Reject,
}

fn classify_reply_action(body: &str) -> Option<ReplyAction> {
    // Look at the first ~30 lines; owners sometimes write their answer
    // above the quoted original. Case-insensitive token match on isolated
    // words.
    let lines_to_scan: Vec<&str> = body.lines().take(40).collect();
    for line in lines_to_scan {
        let trimmed = line.trim_start_matches(|c: char| c == '>' || c.is_whitespace());
        let upper = trimmed.to_uppercase();
        for token in upper.split(|c: char| !c.is_ascii_alphabetic()) {
            match token {
                "APPROVE" | "APPROVED" | "YES" | "OK" => {
                    return Some(ReplyAction::Approve);
                }
                "REJECT" | "REJECTED" | "NO" | "DECLINE" | "DECLINED" => {
                    return Some(ReplyAction::Reject);
                }
                _ => {}
            }
        }
    }
    None
}

fn extract_work_id(subject: &str) -> Option<String> {
    // Matches `[ctox-approve:<id>]` with <id> being a self-work work_id
    // such as `self-work:i-hate-ai:abc123`.
    let start = subject.find("[ctox-approve:")?;
    let rest = &subject[start + "[ctox-approve:".len()..];
    let end = rest.find(']')?;
    let id = rest[..end].trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

fn mark_reply_handled(conn: &Connection, message_key: &str) -> Result<()> {
    conn.execute(
        r#"
        UPDATE communication_routing_state
        SET route_status = 'approval-nag-handled',
            updated_at = datetime('now')
        WHERE message_key = ?1
        "#,
        params![message_key],
    )?;
    Ok(())
}

// Backwards-compat shim so unused `Serialize` / `Deserialize` imports do
// not trigger dead-code warnings if this module grows later.
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
struct _UnusedMarker;
