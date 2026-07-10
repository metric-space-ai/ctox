// Origin: CTOX
// License: AGPL-3.0-only
//
// Proactive reminder / approval-chase for open approval-gate internal work
// items.
//
// Normal runs leave `CTOX_AUTO_APPROVE_GATES` unset,
// which means CTOX creates approval-gate internal work items and then
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
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashSet;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::inference::runtime_env;
use crate::mission::{channels, plan, tickets};
use crate::service::core_state_machine::{
    CoreEntityType, CoreEvent, CoreEvidenceRefs, CoreState, CoreTransitionRequest, RuntimeLane,
};
use crate::service::core_transition_guard::enforce_core_transition;
use crate::service::governance;

const DB_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";

static APPROVAL_NAG_SCHEMA_READY: OnceLock<Mutex<HashSet<ApprovalNagDbKey>>> = OnceLock::new();

thread_local! {
    static APPROVAL_NAG_DB: RefCell<Option<CachedApprovalNagConnection>> = RefCell::new(None);
}

struct CachedApprovalNagConnection {
    key: ApprovalNagDbKey,
    conn: Connection,
}

#[cfg(unix)]
type ApprovalNagDbKey = (PathBuf, u64, u64);
#[cfg(not(unix))]
type ApprovalNagDbKey = PathBuf;

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
        if marker.eq_ignore_ascii_case("tui-only") || marker.eq_ignore_ascii_case("tui_only") {
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
    let settings = runtime_env::effective_runtime_env_map(root).unwrap_or_default();
    summary.replies_processed = process_inbound_approval_replies(root, &settings)?;
    Ok(summary)
}

fn db_path(root: &Path) -> PathBuf {
    root.join(DB_RELATIVE_PATH)
}

fn open_db(root: &Path) -> Result<Connection> {
    let path = db_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create approval nag db parent {}", parent.display()))?;
    }
    let conn = Connection::open(&path).with_context(|| format!("open {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("configure SQLite busy_timeout for approval nag")?;
    Ok(conn)
}

fn with_db<T>(root: &Path, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let path = db_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create approval nag db parent {}", parent.display()))?;
    }
    APPROVAL_NAG_DB.with(|cell| {
        let mut cached = cell.borrow_mut();
        let key = approval_nag_db_key(&path);
        let needs_open = cached
            .as_ref()
            .map(|entry| entry.key != key)
            .unwrap_or(true);
        if needs_open {
            let conn = open_db(root)?;
            ensure_schema_once(&path, &conn)?;
            let key = approval_nag_db_key(&path);
            *cached = Some(CachedApprovalNagConnection { key, conn });
        }
        let conn = &cached.as_ref().expect("approval nag db initialized").conn;
        f(conn)
    })
}

fn ensure_schema(root: &Path) -> Result<()> {
    with_db(root, |_| Ok(()))
}

fn ensure_schema_once(path: &Path, conn: &Connection) -> Result<()> {
    let key = approval_nag_db_key(path);
    let ready = APPROVAL_NAG_SCHEMA_READY.get_or_init(|| Mutex::new(HashSet::new()));
    let mut ready = ready
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if ready.contains(&key) {
        return Ok(());
    }
    ensure_schema_on_conn(conn)?;
    ready.insert(key);
    Ok(())
}

#[cfg(unix)]
fn approval_nag_db_key(path: &Path) -> ApprovalNagDbKey {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| absolute_db_path(path));
    let metadata = std::fs::metadata(&canonical)
        .or_else(|_| std::fs::metadata(path))
        .ok();
    let (device, inode) = metadata
        .map(|metadata| (metadata.dev(), metadata.ino()))
        .unwrap_or((0, 0));
    (canonical, device, inode)
}

#[cfg(not(unix))]
fn approval_nag_db_key(path: &Path) -> ApprovalNagDbKey {
    std::fs::canonicalize(path).unwrap_or_else(|_| absolute_db_path(path))
}

fn absolute_db_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn ensure_schema_on_conn(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS ticket_approval_nag_state (
            work_id TEXT PRIMARY KEY,
            attempt_count INTEGER NOT NULL DEFAULT 0,
            first_seen_at TEXT NOT NULL,
            last_nag_at TEXT,
            next_nag_at TEXT NOT NULL,
            last_channel TEXT,
            completed_at TEXT,
            exhausted_at TEXT,
            escalated_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_ticket_approval_nag_state_due
            ON ticket_approval_nag_state(next_nag_at)
            WHERE completed_at IS NULL;

        CREATE TABLE IF NOT EXISTS ticket_approval_reply_ledger (
            message_key TEXT PRIMARY KEY,
            work_id TEXT NOT NULL,
            action TEXT NOT NULL,
            sender_address TEXT NOT NULL,
            body_sha256 TEXT NOT NULL,
            residual_text TEXT NOT NULL,
            decision_status TEXT NOT NULL,
            followup_message_key TEXT,
            observed_at TEXT NOT NULL,
            applied_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_ticket_approval_reply_work
            ON ticket_approval_reply_ledger(work_id, observed_at DESC);
        "#,
    )?;
    ensure_nag_state_column(conn, "exhausted_at", "TEXT")?;
    ensure_nag_state_column(conn, "escalated_at", "TEXT")?;
    Ok(())
}

fn ensure_nag_state_column(conn: &Connection, column: &str, sql_type: &str) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('ticket_approval_nag_state') WHERE name = ?1)",
        params![column],
        |row| row.get::<_, i64>(0),
    )? != 0;
    if !exists {
        let sql = format!("ALTER TABLE ticket_approval_nag_state ADD COLUMN {column} {sql_type}");
        if let Err(err) = conn.execute(&sql, []) {
            if !err
                .to_string()
                .to_ascii_lowercase()
                .contains("duplicate column name")
            {
                return Err(err).with_context(|| format!("add approval nag column {column}"));
            }
        }
    }
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
fn nag_schedule(root: &Path) -> &'static [(i64, &'static str)] {
    crate::autonomy::AutonomyLevel::from_root(root).nag_cadence_seconds()
}

fn schedule_new_gates(root: &Path) -> Result<usize> {
    let open_gates = tickets::list_ticket_self_work_items(root, None, Some("open"), 256)?;
    with_db(root, |conn| {
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
            let Some(first) = nag_schedule(root).first() else {
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
    })
}

fn mark_closed_gates_completed(root: &Path) -> Result<usize> {
    with_db(root, |conn| {
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
    })
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
    with_db(root, |conn| {
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
            let (channel, subject, body) = compose_nag(root, &item, attempt);
            match send_via_channel(root, &channel, &subject, &body, &item) {
                Ok(()) => {
                    sent += 1;
                    let next_attempt = attempt + 1;
                    let next_iso = if next_attempt >= nag_schedule(root).len() {
                        // Stop email nagging, but keep the approval gate open and
                        // surface the exhausted wait as durable operator evidence.
                        let _ = conn.execute(
                            "UPDATE ticket_approval_nag_state
                             SET completed_at = ?1, exhausted_at = ?1, escalated_at = COALESCE(escalated_at, ?1),
                                 attempt_count = ?2, last_nag_at = ?1, last_channel = ?3
                             WHERE work_id = ?4",
                            params![&now, next_attempt as i64, &channel, &work_id],
                        );
                        governance::record_event_or_count(
                            root,
                            governance::GovernanceEventRequest {
                                mechanism_id: "approval_nag_exhausted",
                                conversation_id: None,
                                severity: "warning",
                                reason: "approval reminder cadence exhausted while the gate remains open",
                                action_taken: "stopped repeated email reminders and escalated the open gate for operator attention",
                                details: serde_json::json!({
                                    "work_id": work_id,
                                    "attempt_count": next_attempt,
                                    "last_channel": channel,
                                }),
                                idempotence_key: Some(&format!("approval-nag-exhausted:{work_id}")),
                            },
                        );
                        continue;
                    } else {
                        add_offset_rfc3339(&first_seen, nag_schedule(root)[next_attempt].0)
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
    })
}

fn compose_nag(
    root: &Path,
    item: &tickets::TicketSelfWorkItemView,
    attempt: usize,
) -> (String, String, String) {
    let channel = nag_schedule(root)
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
                 - To approve, reply with APPROVE on a separate, unquoted line.\n\
                 - To reject, reply with REJECT on a separate, unquoted line.\n\
                 - YES, OK, NO and words inside quoted reply history are ignored.\n\
                 - To do nothing, ignore this email; you will be reminded a few more times and then left alone.\n\n\
                 The subject's [ctox-approve:...] tag is how CTOX matches your reply to this gate — please keep it intact.\n"
            );
        }
        ApprovalModality::TuiOnly => {
            out.push_str(
                "Security policy: this action is high-impact enough that an email reply is not accepted as approval.\n\
                 To approve or reject, please open the local CTOX TUI on the host and act on this gate there, or run on the host:\n\
                   ctox ticket internal-work-transition --work-id ");
            out.push_str(&item.work_id);
            out.push_str(
                " --state closed     # to approve\n   ctox ticket internal-work-transition --work-id ",
            );
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
        .or_else(|_| -> std::io::Result<PathBuf> { Ok(PathBuf::from("ctox")) })
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
pub(crate) fn process_inbound_approval_replies(
    root: &Path,
    settings: &std::collections::BTreeMap<String, String>,
) -> Result<usize> {
    // Look back 7 days to catch delayed replies.
    let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
    let cutoff_iso = cutoff.to_rfc3339();

    with_db(root, |conn| {
        let mut statement = conn.prepare(
            r#"
        SELECT m.message_key, m.subject, m.body_text, m.channel,
               m.sender_address, m.thread_key
        FROM communication_messages m
        JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.direction = 'inbound'
          AND (m.observed_at IS NULL OR m.observed_at > ?1)
          AND m.subject LIKE '%[ctox-approve:%'
          AND r.route_status = 'pending'
        ORDER BY m.rowid DESC
        LIMIT 32
        "#,
        )?;

        let rows: Vec<(String, String, String, String, String, String)> = statement
            .query_map(params![&cutoff_iso], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        drop(statement);

        let mut processed = 0usize;
        for (message_key, subject, body, _channel, sender_address, thread_key) in rows {
            let work_id = match extract_work_id(&subject) {
                Some(id) => id,
                None => continue,
            };
            let Some(parsed) = parse_reply_action(&body) else {
                continue;
            };
            let policy = channels::classify_email_sender(settings, &sender_address);
            if !policy.allowed
                || !policy.allow_admin_actions
                || !matches!(policy.role.as_str(), "owner" | "founder" | "admin")
            {
                governance::record_event_or_count(
                    root,
                    governance::GovernanceEventRequest {
                        mechanism_id: "sender_authority_boundary",
                        conversation_id: None,
                        severity: "warning",
                        reason: "unauthorized sender attempted to resolve an approval gate",
                        action_taken: "ignored the approval token and left the message for normal sender-policy routing",
                        details: serde_json::json!({
                            "message_key": message_key,
                            "work_id": work_id,
                            "sender_address": sender_address,
                            "sender_role": policy.role,
                        }),
                        idempotence_key: Some(&format!("unauthorized-approval:{message_key}")),
                    },
                );
                continue;
            }
            let item = match tickets::load_ticket_self_work_item(root, &work_id)? {
                Some(i) if i.kind == "approval-gate" => i,
                _ => continue,
            };
            let modality = ApprovalModality::from_item(&item);
            if modality == ApprovalModality::TuiOnly {
                // tui-only gates do NOT accept email replies as approval.
                // Mark the reply as acknowledged so we stop reprocessing it.
                let _ = mark_reply_handled(conn, &message_key);
                continue;
            }
            let body_sha256 = format!("{:x}", Sha256::digest(body.as_bytes()));
            let now = now_rfc3339();
            conn.execute(
                r#"
                INSERT INTO ticket_approval_reply_ledger (
                    message_key, work_id, action, sender_address, body_sha256,
                    residual_text, decision_status, followup_message_key,
                    observed_at, applied_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'observed', NULL, ?7, NULL)
                ON CONFLICT(message_key) DO NOTHING
                "#,
                params![
                    message_key,
                    work_id,
                    parsed.action.as_str(),
                    policy.normalized_email,
                    body_sha256,
                    parsed.residual_text,
                    now,
                ],
            )?;
            let new_state = parsed.action.target_state();
            let transition_result = if item.state == new_state {
                Ok(item.clone())
            } else if item.state != "open" {
                continue;
            } else {
                tickets::set_ticket_approval_gate_state_from_authorized_reply(
                    root,
                    &work_id,
                    new_state,
                    &message_key,
                )
            };
            match transition_result {
                Ok(_) => {
                    conn.execute(
                        "UPDATE ticket_approval_reply_ledger
                         SET decision_status='applied', applied_at=?2
                         WHERE message_key=?1",
                        params![message_key, now_rfc3339()],
                    )?;
                    persist_approval_reply_followup(
                        root,
                        conn,
                        &message_key,
                        &work_id,
                        &thread_key,
                        &item.title,
                        &parsed.residual_text,
                    )?;
                    let _ = plan::satisfy_wait_for_work_item(root, &work_id, new_state)?;
                    if new_state == "closed" {
                        for entity_type in ["approval", "approval-gate", "ticket-self-work"] {
                            let _ =
                                channels::wake_messages_waiting_for(root, entity_type, &work_id)?;
                            let _ = tickets::wake_ticket_events_waiting_for(
                                root,
                                entity_type,
                                &work_id,
                            )?;
                        }
                    }
                    processed += 1;
                    let _ = mark_reply_handled(conn, &message_key);
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
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplyAction {
    Approve,
    Reject,
}

impl ReplyAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }

    fn target_state(self) -> &'static str {
        match self {
            Self::Approve => "closed",
            Self::Reject => "failed",
        }
    }
}

struct ParsedApprovalReply {
    action: ReplyAction,
    residual_text: String,
}

fn parse_reply_action(body: &str) -> Option<ParsedApprovalReply> {
    let mut action = None;
    let mut residual = Vec::new();
    for line in body.lines().take(80) {
        let trimmed = line.trim();
        let lowered = trimmed.to_ascii_lowercase();
        if trimmed.starts_with('>')
            || lowered.starts_with("-----original message-----")
            || (lowered.starts_with("on ") && lowered.ends_with(" wrote:"))
        {
            break;
        }
        let candidate = match trimmed.to_ascii_uppercase().as_str() {
            "APPROVE" => Some(ReplyAction::Approve),
            "REJECT" => Some(ReplyAction::Reject),
            _ => None,
        };
        if let Some(candidate) = candidate {
            if action.is_some_and(|existing| existing != candidate) {
                return None;
            }
            action = Some(candidate);
        } else if !trimmed.is_empty() {
            residual.push(trimmed.to_string());
        }
    }
    Some(ParsedApprovalReply {
        action: action?,
        residual_text: residual.join("\n"),
    })
}

fn persist_approval_reply_followup(
    root: &Path,
    conn: &Connection,
    message_key: &str,
    work_id: &str,
    thread_key: &str,
    gate_title: &str,
    residual_text: &str,
) -> Result<()> {
    if residual_text.trim().is_empty() {
        return Ok(());
    }
    let existing: Option<String> = conn
        .query_row(
            "SELECT followup_message_key FROM ticket_approval_reply_ledger WHERE message_key=?1",
            params![message_key],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    if existing.is_some() {
        return Ok(());
    }
    let followup = channels::create_queue_task(
        root,
        channels::QueueTaskCreateRequest {
            title: format!("Follow-up from approval reply: {gate_title}"),
            prompt: format!(
                "The authorized approval reply for gate `{work_id}` also contained follow-up work. Handle this text in the original thread without repeating the approval action:\n\n{}",
                residual_text.trim()
            ),
            thread_key: thread_key.to_string(),
            workspace_root: None,
            priority: "high".to_string(),
            suggested_skill: None,
            parent_message_key: Some(message_key.to_string()),
            extra_metadata: Some(serde_json::json!({
                "idempotency_key": format!("approval-followup:{message_key}"),
                "source": "approval-reply-residual",
                "approval_work_id": work_id,
            })),
        },
    )?;
    conn.execute(
        "UPDATE ticket_approval_reply_ledger SET followup_message_key=?2 WHERE message_key=?1",
        params![message_key, followup.message_key],
    )?;
    Ok(())
}

fn extract_work_id(subject: &str) -> Option<String> {
    // Matches `[ctox-approve:<id>]` with <id> being a legacy internal-work id
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
    let previous = conn
        .query_row(
            "SELECT route_status FROM communication_routing_state WHERE message_key = ?1 LIMIT 1",
            params![message_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| "pending".to_string());
    let mut metadata = std::collections::BTreeMap::new();
    metadata.insert("from_route_status".to_string(), previous.clone());
    metadata.insert(
        "to_route_status".to_string(),
        "approval-nag-handled".to_string(),
    );
    metadata.insert(
        "reason".to_string(),
        "approval_nag_reply_handled".to_string(),
    );
    enforce_core_transition(
        conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: message_key.to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: approval_nag_route_core_state(&previous),
            to_state: CoreState::Blocked,
            event: CoreEvent::Block,
            actor: "ctox-approval-nag".to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata,
        },
    )?;
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

fn approval_nag_route_core_state(route_status: &str) -> CoreState {
    match route_status.trim().to_ascii_lowercase().as_str() {
        "leased" => CoreState::Leased,
        "blocked" | "review_rework" | "approval-nag-handled" => CoreState::Blocked,
        "failed" => CoreState::Failed,
        "handled" | "completed" => CoreState::Completed,
        "cancelled" | "superseded" => CoreState::Superseded,
        _ => CoreState::Pending,
    }
}

// Backwards-compat shim so unused `Serialize` / `Deserialize` imports do
// not trigger dead-code warnings if this module grows later.
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
struct _UnusedMarker;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn approval_reply_requires_exact_unquoted_action_line() {
        assert!(parse_reply_action("OK").is_none());
        assert!(parse_reply_action("Please APPROVE this").is_none());
        assert!(parse_reply_action("> APPROVE\n> quoted instructions").is_none());

        let parsed = parse_reply_action("APPROVE\nPlease correct the invoice date.\n\n> REJECT")
            .expect("exact unquoted action should parse");
        assert_eq!(parsed.action, ReplyAction::Approve);
        assert_eq!(parsed.residual_text, "Please correct the invoice date.");
    }

    #[test]
    fn approval_reply_rejects_conflicting_action_lines() {
        assert!(parse_reply_action("APPROVE\nREJECT").is_none());
    }

    #[test]
    fn approval_reply_authority_ledger_and_followup_are_end_to_end_idempotent() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "ctox-approval-reply-e2e-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let thread_key = "email/approval-reply-e2e";
        let gate = tickets::put_ticket_self_work_item(
            &root,
            tickets::TicketSelfWorkUpsertInput {
                source_system: "internal".to_string(),
                kind: "approval-gate".to_string(),
                title: "Approve the tested action".to_string(),
                body_text: "Exercise the durable approval control plane.".to_string(),
                state: "open".to_string(),
                metadata: serde_json::json!({
                    "thread_key": thread_key,
                    "dedupe_key": "approval-reply-e2e-gate",
                }),
            },
            false,
        )?;
        let subject = format!("Re: [ctox-approve:{}]", gate.work_id);
        let create_email = |sender: &str, body: &str, suffix: &str| -> Result<String> {
            let task = channels::create_queue_task(
                &root,
                channels::QueueTaskCreateRequest {
                    title: subject.clone(),
                    prompt: body.to_string(),
                    thread_key: thread_key.to_string(),
                    workspace_root: None,
                    priority: "normal".to_string(),
                    suggested_skill: None,
                    parent_message_key: None,
                    extra_metadata: Some(serde_json::json!({
                        "idempotency_key": format!("approval-reply-e2e:{suffix}"),
                    })),
                },
            )?;
            let conn = open_db(&root)?;
            conn.execute(
                "UPDATE communication_messages SET channel='email', sender_address=?2 WHERE message_key=?1",
                params![task.message_key, sender],
            )?;
            Ok(task.message_key)
        };
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
            "owner@example.com".to_string(),
        );

        create_email("outsider@example.net", "APPROVE", "unauthorized")?;
        assert_eq!(process_inbound_approval_replies(&root, &settings)?, 0);
        assert_eq!(
            tickets::load_ticket_self_work_item(&root, &gate.work_id)?
                .expect("gate after unauthorized reply")
                .state,
            "open"
        );

        create_email(
            "owner@example.com",
            "> APPROVE\n> quoted approval instructions",
            "quoted",
        )?;
        assert_eq!(process_inbound_approval_replies(&root, &settings)?, 0);
        assert_eq!(
            tickets::load_ticket_self_work_item(&root, &gate.work_id)?
                .expect("gate after quoted reply")
                .state,
            "open"
        );

        let authorized_key = create_email(
            "owner@example.com",
            "APPROVE\nPlease verify the invoice date.",
            "authorized",
        )?;
        assert_eq!(process_inbound_approval_replies(&root, &settings)?, 1);
        assert_eq!(process_inbound_approval_replies(&root, &settings)?, 0);
        assert_eq!(
            tickets::load_ticket_self_work_item(&root, &gate.work_id)?
                .expect("gate after authorized reply")
                .state,
            "closed"
        );

        let conn = open_db(&root)?;
        let ledger: (i64, String, String, String, Option<String>) = conn.query_row(
            r#"
            SELECT COUNT(*), action, sender_address, decision_status, followup_message_key
            FROM ticket_approval_reply_ledger
            WHERE message_key=?1
            "#,
            params![authorized_key],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
        assert_eq!(ledger.0, 1);
        assert_eq!(ledger.1, "approve");
        assert_eq!(ledger.2, "owner@example.com");
        assert_eq!(ledger.3, "applied");
        let followup_key = ledger.4.expect("residual text creates follow-up work");
        let followup_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM communication_messages WHERE message_key=?1 AND thread_key=?2",
            params![followup_key, thread_key],
            |row| row.get(0),
        )?;
        assert_eq!(followup_count, 1);

        let _ = std::fs::remove_dir_all(root);
        Ok(())
    }
}
