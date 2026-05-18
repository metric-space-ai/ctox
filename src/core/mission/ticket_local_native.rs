use anyhow::Context;
use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;

use crate::mission::ticket_protocol::TicketEventRecord;
use crate::mission::ticket_protocol::TicketMirrorRecord;
use crate::mission::ticket_protocol::TicketSelfWorkAssignResult;
use crate::mission::ticket_protocol::TicketSyncBatch;
use crate::mission::ticket_protocol::TicketWritebackResult;

const DEFAULT_DB_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";
const DEFAULT_LIST_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalTicketRecord {
    pub ticket_id: String,
    pub title: String,
    pub body_text: String,
    pub status: String,
    pub priority: Option<String>,
    pub requester: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalTicketEventRecord {
    pub event_id: String,
    pub ticket_id: String,
    pub event_type: String,
    pub summary: String,
    pub body_text: String,
    pub created_at: String,
    pub metadata: Value,
}

pub(crate) fn handle_local_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "init" => {
            let conn = open_local_db(root)?;
            print_json(&json!({
                "ok": true,
                "db_path": resolve_db_path(root),
                "initialized": schema_state(&conn)?,
            }))
        }
        "create" => {
            let title = required_flag_value(args, "--title")
                .context("usage: ctox ticket local create --title <text> --body <text>")?;
            let body = required_flag_value(args, "--body")
                .context("usage: ctox ticket local create --title <text> --body <text>")?;
            let record = create_local_ticket(
                root,
                title,
                body,
                find_flag_value(args, "--status"),
                find_flag_value(args, "--priority"),
            )?;
            print_json(&json!({"ok": true, "ticket": record}))
        }
        "comment" => {
            let ticket_id = required_flag_value(args, "--ticket-id")
                .context("usage: ctox ticket local comment --ticket-id <id> --body <text>")?;
            let body = required_flag_value(args, "--body")
                .context("usage: ctox ticket local comment --ticket-id <id> --body <text>")?;
            let event = add_local_comment(root, ticket_id, body)?;
            print_json(&json!({"ok": true, "event": event}))
        }
        "transition" => {
            let ticket_id = required_flag_value(args, "--ticket-id")
                .context("usage: ctox ticket local transition --ticket-id <id> --status <value>")?;
            let status = required_flag_value(args, "--status")
                .context("usage: ctox ticket local transition --ticket-id <id> --status <value>")?;
            let record = transition_local_ticket(root, ticket_id, status)?;
            print_json(&json!({"ok": true, "ticket": record}))
        }
        "list" => {
            let limit = parse_limit(args, DEFAULT_LIST_LIMIT);
            let tickets = list_local_tickets(root, limit)?;
            print_json(&json!({"ok": true, "count": tickets.len(), "tickets": tickets}))
        }
        "show" => {
            let ticket_id = required_flag_value(args, "--ticket-id")
                .context("usage: ctox ticket local show --ticket-id <id>")?;
            let ticket = load_local_ticket(root, ticket_id)?.context("local ticket not found")?;
            let events = list_local_ticket_events(root, ticket_id, DEFAULT_LIST_LIMIT)?;
            print_json(&json!({"ok": true, "ticket": ticket, "events": events}))
        }
        _ => anyhow::bail!(
            "usage:\n  ctox ticket local init\n  ctox ticket local create --title <text> --body <text> [--status <value>] [--priority <value>]\n  ctox ticket local comment --ticket-id <id> --body <text>\n  ctox ticket local transition --ticket-id <id> --status <value>\n  ctox ticket local list [--limit <n>]\n  ctox ticket local show --ticket-id <id>"
        ),
    }
}

pub(crate) fn create_local_ticket(
    root: &Path,
    title: &str,
    body: &str,
    status: Option<&str>,
    priority: Option<&str>,
) -> Result<LocalTicketRecord> {
    let mut conn = open_local_db(root)?;
    let now = now_iso_string();
    let ticket_id = format!("LT-{}", stable_digest(&(title.to_string() + &now)));
    conn.execute(
        r#"
        INSERT INTO local_tickets (
            ticket_id, title, body_text, status, priority, requester, metadata_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, '{}', ?6, ?6)
        "#,
        params![
            ticket_id,
            title.trim(),
            body.trim(),
            status.unwrap_or("open").trim(),
            priority.map(str::trim),
            now,
        ],
    )?;
    insert_local_event(
        &mut conn,
        &ticket_id,
        "created",
        title.trim(),
        body.trim(),
        json!({}),
    )?;
    load_local_ticket(root, &ticket_id)?.context("failed to load local ticket after create")
}

pub(crate) fn add_local_comment(
    root: &Path,
    ticket_id: &str,
    body: &str,
) -> Result<LocalTicketEventRecord> {
    let mut conn = open_local_db(root)?;
    if load_local_ticket(root, ticket_id)?.is_none() {
        anyhow::bail!("local ticket not found: {ticket_id}");
    }
    let event_id = insert_local_event(
        &mut conn,
        ticket_id,
        "comment",
        "comment",
        body.trim(),
        json!({}),
    )?;
    conn.execute(
        "UPDATE local_tickets SET updated_at = ?2 WHERE ticket_id = ?1",
        params![ticket_id, now_iso_string()],
    )?;
    load_local_event(root, &event_id)?.context("failed to load local event after comment")
}

pub(crate) fn transition_local_ticket(
    root: &Path,
    ticket_id: &str,
    status: &str,
) -> Result<LocalTicketRecord> {
    transition_local_ticket_with_metadata(root, ticket_id, status, json!({}))
}

fn transition_local_ticket_with_metadata(
    root: &Path,
    ticket_id: &str,
    status: &str,
    metadata: Value,
) -> Result<LocalTicketRecord> {
    let mut conn = open_local_db(root)?;
    if load_local_ticket(root, ticket_id)?.is_none() {
        anyhow::bail!("local ticket not found: {ticket_id}");
    }
    let now = now_iso_string();
    conn.execute(
        "UPDATE local_tickets SET status = ?2, updated_at = ?3 WHERE ticket_id = ?1",
        params![ticket_id, status.trim(), now],
    )?;
    insert_local_event(
        &mut conn,
        ticket_id,
        "status_changed",
        &format!("status -> {}", status.trim()),
        &format!("Ticket moved to status {}", status.trim()),
        merge_metadata(json!({"status": status.trim()}), metadata),
    )?;
    load_local_ticket(root, ticket_id)?.context("failed to load local ticket after transition")
}

pub(crate) fn fetch_sync_batch(root: &Path) -> Result<TicketSyncBatch> {
    let tickets = list_local_tickets(root, usize::MAX)?;
    let mut mirror_records = Vec::new();
    let mut event_records = Vec::new();
    for ticket in &tickets {
        mirror_records.push(TicketMirrorRecord {
            remote_ticket_id: ticket.ticket_id.clone(),
            title: ticket.title.clone(),
            body_text: ticket.body_text.clone(),
            remote_status: ticket.status.clone(),
            priority: ticket.priority.clone(),
            requester: ticket.requester.clone(),
            metadata: ticket.metadata.clone(),
            external_created_at: ticket.created_at.clone(),
            external_updated_at: ticket.updated_at.clone(),
        });

        let events = list_local_ticket_events(root, &ticket.ticket_id, usize::MAX)?;
        for event in events {
            let direction =
                if event.metadata.get("origin").and_then(Value::as_str) == Some("ctox-writeback") {
                    "outbound"
                } else {
                    "inbound"
                };
            event_records.push(TicketEventRecord {
                remote_ticket_id: ticket.ticket_id.clone(),
                remote_event_id: event.event_id,
                direction: direction.to_string(),
                event_type: event.event_type,
                summary: event.summary,
                body_text: event.body_text,
                metadata: event.metadata,
                external_created_at: event.created_at,
            });
        }
    }
    Ok(TicketSyncBatch {
        system: "local".to_string(),
        fetched_ticket_count: tickets.len(),
        tickets: mirror_records,
        events: event_records,
        metadata: json!({}),
    })
}

pub(crate) fn test(root: &Path) -> Result<Value> {
    let conn = open_local_db(root)?;
    Ok(json!({
        "ok": true,
        "system": "local",
        "db_path": resolve_db_path(root),
        "state": schema_state(&conn)?,
    }))
}

pub(crate) fn writeback_comment(
    root: &Path,
    ticket_id: &str,
    body: &str,
    _internal: bool,
) -> Result<TicketWritebackResult> {
    let mut conn = open_local_db(root)?;
    let event_id = insert_local_event(
        &mut conn,
        ticket_id,
        "comment",
        "comment",
        body.trim(),
        json!({"origin": "ctox-writeback"}),
    )?;
    conn.execute(
        "UPDATE local_tickets SET updated_at = ?2 WHERE ticket_id = ?1",
        params![ticket_id, now_iso_string()],
    )?;
    Ok(TicketWritebackResult {
        remote_event_ids: vec![event_id],
    })
}

pub(crate) fn writeback_transition(
    root: &Path,
    ticket_id: &str,
    state: &str,
    note_body: Option<&str>,
    internal_note: bool,
) -> Result<TicketWritebackResult> {
    let _record = transition_local_ticket_with_metadata(
        root,
        ticket_id,
        state,
        json!({"origin": "ctox-writeback"}),
    )?;
    let mut remote_event_ids = Vec::new();
    if let Some(note_body) = note_body.map(str::trim).filter(|value| !value.is_empty()) {
        let result = writeback_comment(root, ticket_id, note_body, internal_note)?;
        remote_event_ids.extend(result.remote_event_ids);
    }
    Ok(TicketWritebackResult { remote_event_ids })
}

pub(crate) fn assign_local_ticket(
    root: &Path,
    ticket_id: &str,
    assignee: &str,
) -> Result<TicketSelfWorkAssignResult> {
    let mut conn = open_local_db(root)?;
    let mut ticket = load_local_ticket(root, ticket_id)?.context("local ticket not found")?;
    let mut metadata = ticket.metadata.as_object().cloned().unwrap_or_default();
    metadata.insert(
        "assigned_to".to_string(),
        Value::String(assignee.trim().to_string()),
    );
    let now = now_iso_string();
    conn.execute(
        "UPDATE local_tickets SET metadata_json = ?2, updated_at = ?3 WHERE ticket_id = ?1",
        params![
            ticket_id,
            serde_json::to_string(&Value::Object(metadata))?,
            now
        ],
    )?;
    let event_id = insert_local_event(
        &mut conn,
        ticket_id,
        "assignment_changed",
        &format!("assigned -> {}", assignee.trim()),
        &format!("Assigned to {}", assignee.trim()),
        json!({
            "assigned_to": assignee.trim(),
            "origin": "ctox-self-work",
        }),
    )?;
    ticket =
        load_local_ticket(root, ticket_id)?.context("failed to load local ticket after assign")?;
    let remote_assignee = ticket
        .metadata
        .get("assigned_to")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(TicketSelfWorkAssignResult {
        remote_assignee,
        remote_event_ids: vec![event_id],
    })
}

pub(crate) fn load_local_ticket(root: &Path, ticket_id: &str) -> Result<Option<LocalTicketRecord>> {
    let conn = open_local_db(root)?;
    conn.query_row(
        r#"
        SELECT ticket_id, title, body_text, status, priority, requester, created_at, updated_at, metadata_json
        FROM local_tickets
        WHERE ticket_id = ?1
        LIMIT 1
        "#,
        params![ticket_id],
        map_local_ticket_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn list_local_tickets(root: &Path, limit: usize) -> Result<Vec<LocalTicketRecord>> {
    let conn = open_local_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT ticket_id, title, body_text, status, priority, requester, created_at, updated_at, metadata_json
        FROM local_tickets
        ORDER BY updated_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], map_local_ticket_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_local_event(root: &Path, event_id: &str) -> Result<Option<LocalTicketEventRecord>> {
    let conn = open_local_db(root)?;
    conn.query_row(
        r#"
        SELECT event_id, ticket_id, event_type, summary, body_text, created_at, metadata_json
        FROM local_ticket_events
        WHERE event_id = ?1
        LIMIT 1
        "#,
        params![event_id],
        map_local_event_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

pub(crate) fn list_local_ticket_events(
    root: &Path,
    ticket_id: &str,
    limit: usize,
) -> Result<Vec<LocalTicketEventRecord>> {
    let conn = open_local_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT event_id, ticket_id, event_type, summary, body_text, created_at, metadata_json
        FROM local_ticket_events
        WHERE ticket_id = ?1
        ORDER BY created_at ASC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![ticket_id, limit as i64], map_local_event_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn insert_local_event(
    conn: &mut Connection,
    ticket_id: &str,
    event_type: &str,
    summary: &str,
    body_text: &str,
    metadata: Value,
) -> Result<String> {
    let now = now_iso_string();
    let event_id = format!(
        "LE-{}",
        stable_digest(&(ticket_id.to_string() + event_type + &now))
    );
    conn.execute(
        r#"
        INSERT INTO local_ticket_events (
            event_id, ticket_id, event_type, summary, body_text, created_at, metadata_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            event_id,
            ticket_id,
            event_type.trim(),
            summary.trim(),
            body_text.trim(),
            now,
            serde_json::to_string(&metadata)?,
        ],
    )?;
    Ok(event_id)
}

fn open_local_db(root: &Path) -> Result<Connection> {
    let path = resolve_db_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create local ticket db parent {}",
                parent.display()
            )
        })?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open local ticket db {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout for local ticket db")?;
    ensure_schema(&conn)?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    let busy_timeout_ms = crate::persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        r#"
        PRAGMA journal_mode=WAL;
        PRAGMA busy_timeout={busy_timeout_ms};

        CREATE TABLE IF NOT EXISTS local_tickets (
            ticket_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            body_text TEXT NOT NULL,
            status TEXT NOT NULL,
            priority TEXT,
            requester TEXT,
            metadata_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS local_ticket_events (
            event_id TEXT PRIMARY KEY,
            ticket_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            summary TEXT NOT NULL,
            body_text TEXT NOT NULL,
            created_at TEXT NOT NULL,
            metadata_json TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_local_ticket_events_ticket_time
            ON local_ticket_events(ticket_id, created_at ASC);
        "#,
    ))?;
    Ok(())
}

fn schema_state(conn: &Connection) -> Result<Value> {
    let ticket_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM local_tickets", [], |row| row.get(0))?;
    let event_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM local_ticket_events", [], |row| {
            row.get(0)
        })?;
    Ok(json!({
        "tickets": ticket_count,
        "events": event_count,
    }))
}

fn resolve_db_path(root: &Path) -> std::path::PathBuf {
    root.join(DEFAULT_DB_RELATIVE_PATH)
}

fn now_iso_string() -> String {
    Utc::now().to_rfc3339()
}

fn stable_digest(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let hex = format!("{digest:x}");
    hex[..12].to_string()
}

fn parse_limit(args: &[String], default: usize) -> usize {
    find_flag_value(args, "--limit")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn map_local_ticket_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalTicketRecord> {
    Ok(LocalTicketRecord {
        ticket_id: row.get(0)?,
        title: row.get(1)?,
        body_text: row.get(2)?,
        status: row.get(3)?,
        priority: row.get(4)?,
        requester: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        metadata: parse_json_column(row.get::<_, String>(8)?),
    })
}

fn map_local_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalTicketEventRecord> {
    Ok(LocalTicketEventRecord {
        event_id: row.get(0)?,
        ticket_id: row.get(1)?,
        event_type: row.get(2)?,
        summary: row.get(3)?,
        body_text: row.get(4)?,
        created_at: row.get(5)?,
        metadata: parse_json_column(row.get::<_, String>(6)?),
    })
}

fn parse_json_column(raw: String) -> Value {
    serde_json::from_str(&raw).unwrap_or_else(|_| json!({}))
}

fn merge_metadata(base: Value, extra: Value) -> Value {
    let mut base_map = base.as_object().cloned().unwrap_or_default();
    if let Some(extra_map) = extra.as_object() {
        for (key, value) in extra_map {
            base_map.insert(key.clone(), value.clone());
        }
    }
    Value::Object(base_map)
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
