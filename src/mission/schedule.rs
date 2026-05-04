use anyhow::Context;
use anyhow::Result;
use chrono::DateTime;
use chrono::Datelike;
use chrono::Duration;
use chrono::Timelike;
use chrono::Utc;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::Transaction;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::time::SystemTime;

use crate::channels;

const DEFAULT_DB_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";
const CRON_SCAN_MINUTES: i64 = 366 * 24 * 60;
const MEETING_JOIN_MARKER: &str = "CTOX_MEETING_JOIN:";

#[derive(Debug, Clone, Serialize)]
pub struct ScheduledTaskView {
    pub task_id: String,
    pub name: String,
    pub cron_expr: String,
    pub prompt: String,
    pub thread_key: String,
    pub skill: Option<String>,
    pub enabled: bool,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleRunView {
    pub run_id: String,
    pub task_id: String,
    pub scheduled_for: String,
    pub emitted_at: String,
    pub message_key: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct EmitDueSummary {
    pub emitted_count: usize,
    pub emitted_runs: Vec<ScheduleRunView>,
}

#[derive(Debug)]
struct ScheduleCreateRequest {
    name: String,
    cron_expr: String,
    prompt: String,
    thread_key: Option<String>,
    skill: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ScheduleEnsureRequest {
    pub name: String,
    pub cron_expr: String,
    pub prompt: String,
    pub thread_key: String,
    pub skill: Option<String>,
}

pub fn handle_schedule_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "init" => {
            let conn = open_schedule_db(root)?;
            print_json(&json!({
                "ok": true,
                "db_path": resolve_db_path(root),
                "initialized": schema_state(&conn)?,
            }))
        }
        "add" => {
            let request = parse_add_request(args)?;
            let created = add_task(root, request)?;
            print_json(&json!({"ok": true, "task": created}))
        }
        "list" => {
            let tasks = list_tasks(root)?;
            print_json(&json!({"ok": true, "count": tasks.len(), "tasks": tasks}))
        }
        "pause" => {
            let task_id = required_flag_value(args, "--task-id")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox schedule pause --task-id <id>")?;
            let task = set_task_enabled(root, task_id, false)?;
            print_json(&json!({"ok": true, "task": task}))
        }
        "resume" => {
            let task_id = required_flag_value(args, "--task-id")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox schedule resume --task-id <id>")?;
            let task = set_task_enabled(root, task_id, true)?;
            print_json(&json!({"ok": true, "task": task}))
        }
        "remove" => {
            let task_id = required_flag_value(args, "--task-id")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox schedule remove --task-id <id>")?;
            remove_task(root, task_id)?;
            print_json(&json!({"ok": true, "task_id": task_id}))
        }
        "run-now" => {
            let task_id = required_flag_value(args, "--task-id")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox schedule run-now --task-id <id>")?;
            let run = emit_task_now(root, task_id)?;
            print_json(&json!({"ok": true, "run": run}))
        }
        "tick" => {
            let summary = emit_due_tasks(root)?;
            print_json(&json!({"ok": true, "summary": summary}))
        }
        _ => anyhow::bail!(
            "usage:\n  ctox schedule init\n  ctox schedule add --name <label> --cron '<expr>' --prompt <text> [--thread-key <key>] [--skill <name>]\n  ctox schedule list\n  ctox schedule pause --task-id <id>\n  ctox schedule resume --task-id <id>\n  ctox schedule remove --task-id <id>\n  ctox schedule run-now --task-id <id>\n  ctox schedule tick"
        ),
    }
}

pub fn emit_due_tasks(root: &Path) -> Result<EmitDueSummary> {
    let conn = open_schedule_db(root)?;
    let now = now_utc();
    let mut due = list_due_tasks(&conn, &now)?;
    if due.is_empty() {
        return Ok(EmitDueSummary::default());
    }
    let tx = conn.unchecked_transaction()?;
    let mut summary = EmitDueSummary::default();
    for task in due.drain(..) {
        let scheduled_for = task
            .next_run_at
            .as_deref()
            .context("due task missing next_run_at")?;
        let is_one_shot_meeting_join = meeting_join_payload(&task.prompt).is_some();
        let run = emit_task_run_tx(root, &tx, &task, scheduled_for)?;
        let (next_run, enabled) = next_task_state_after_emit(
            is_one_shot_meeting_join,
            &run.status,
            &task.cron_expr,
            scheduled_for,
            now,
        )?;
        let now_iso = now_iso_string();
        tx.execute(
            r#"
            UPDATE scheduled_tasks
            SET last_run_at = ?2,
                next_run_at = ?3,
                enabled = ?4,
                updated_at = ?5
            WHERE task_id = ?1
            "#,
            params![
                task.task_id,
                scheduled_for,
                next_run.as_deref(),
                if enabled { 1 } else { 0 },
                now_iso
            ],
        )?;
        summary.emitted_count += 1;
        summary.emitted_runs.push(run);
    }
    tx.commit()?;
    Ok(summary)
}

fn add_task(root: &Path, request: ScheduleCreateRequest) -> Result<ScheduledTaskView> {
    validate_cron_expr(&request.cron_expr)?;
    let conn = open_schedule_db(root)?;
    let now = now_iso_string();
    let task_id = format!(
        "sched_{}",
        stable_digest(&format!("{}:{}:{}", request.name, request.cron_expr, now))
    );
    let thread_key = request
        .thread_key
        .unwrap_or_else(|| format!("cron/{}", task_id));
    let next_run_at = next_run_after(&request.cron_expr, now_utc())?;
    conn.execute(
        r#"
        INSERT INTO scheduled_tasks (
            task_id, name, cron_expr, prompt, thread_key, skill, enabled,
            next_run_at, last_run_at, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, NULL, ?8, ?8)
        "#,
        params![
            task_id,
            request.name.trim(),
            request.cron_expr.trim(),
            request.prompt.trim(),
            thread_key,
            request.skill.as_deref(),
            next_run_at.as_deref(),
            now,
        ],
    )?;
    load_task(&conn, &task_id)?.context("failed to reload inserted scheduled task")
}

pub fn list_tasks(root: &Path) -> Result<Vec<ScheduledTaskView>> {
    let conn = open_schedule_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT task_id, name, cron_expr, prompt, thread_key, skill, enabled,
               next_run_at, last_run_at, created_at, updated_at
        FROM scheduled_tasks
        ORDER BY enabled DESC, next_run_at ASC, created_at ASC
        "#,
    )?;
    let rows = statement.query_map([], map_task_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub fn set_task_enabled(root: &Path, task_id: &str, enabled: bool) -> Result<ScheduledTaskView> {
    let conn = open_schedule_db(root)?;
    let task = load_task(&conn, task_id)?.context("scheduled task not found")?;
    let next_run_at = if enabled {
        next_run_after(&task.cron_expr, now_utc())?
    } else {
        None
    };
    let now = now_iso_string();
    conn.execute(
        r#"
        UPDATE scheduled_tasks
        SET enabled = ?2, next_run_at = ?3, updated_at = ?4
        WHERE task_id = ?1
        "#,
        params![
            task_id,
            if enabled { 1 } else { 0 },
            next_run_at.as_deref(),
            now
        ],
    )?;
    load_task(&conn, task_id)?.context("failed to reload scheduled task")
}

pub fn remove_task(root: &Path, task_id: &str) -> Result<()> {
    let conn = open_schedule_db(root)?;
    conn.execute(
        "DELETE FROM scheduled_task_runs WHERE task_id = ?1",
        params![task_id],
    )?;
    conn.execute(
        "DELETE FROM scheduled_tasks WHERE task_id = ?1",
        params![task_id],
    )?;
    Ok(())
}

pub fn emit_task_now(root: &Path, task_id: &str) -> Result<ScheduleRunView> {
    let conn = open_schedule_db(root)?;
    let task = load_task(&conn, task_id)?.context("scheduled task not found")?;
    let scheduled_for = now_iso_string();
    let tx = conn.unchecked_transaction()?;
    let run = emit_task_run_tx(root, &tx, &task, &scheduled_for)?;
    let next_run_at = if task.enabled {
        next_run_after(&task.cron_expr, now_utc())?
    } else {
        task.next_run_at.clone()
    };
    let now = now_iso_string();
    tx.execute(
        r#"
        UPDATE scheduled_tasks
        SET last_run_at = ?2, next_run_at = ?3, updated_at = ?4
        WHERE task_id = ?1
        "#,
        params![task_id, scheduled_for, next_run_at.as_deref(), now],
    )?;
    tx.commit()?;
    Ok(run)
}

pub fn ensure_task(root: &Path, request: ScheduleEnsureRequest) -> Result<ScheduledTaskView> {
    validate_cron_expr(&request.cron_expr)?;
    let conn = open_schedule_db(root)?;
    let now = now_iso_string();
    let next_run_at = next_run_after(&request.cron_expr, now_utc())?;
    let existing_task_id: Option<String> = conn
        .query_row(
            r#"
            SELECT task_id
            FROM scheduled_tasks
            WHERE name = ?1 AND thread_key = ?2
            LIMIT 1
            "#,
            params![request.name.trim(), request.thread_key.trim()],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(task_id) = existing_task_id {
        conn.execute(
            r#"
            UPDATE scheduled_tasks
            SET cron_expr = ?2,
                prompt = ?3,
                skill = ?4,
                enabled = 1,
                next_run_at = ?5,
                updated_at = ?6
            WHERE task_id = ?1
            "#,
            params![
                task_id,
                request.cron_expr.trim(),
                request.prompt.trim(),
                request.skill.as_deref(),
                next_run_at.as_deref(),
                now,
            ],
        )?;
        return load_task(&conn, &task_id)?.context("failed to reload ensured scheduled task");
    }
    let task_id = format!(
        "sched_{}",
        stable_digest(&format!(
            "{}:{}",
            request.name.trim(),
            request.thread_key.trim()
        ))
    );
    conn.execute(
        r#"
        INSERT INTO scheduled_tasks (
            task_id, name, cron_expr, prompt, thread_key, skill, enabled,
            next_run_at, last_run_at, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, NULL, ?8, ?8)
        "#,
        params![
            task_id,
            request.name.trim(),
            request.cron_expr.trim(),
            request.prompt.trim(),
            request.thread_key.trim(),
            request.skill.as_deref(),
            next_run_at.as_deref(),
            now,
        ],
    )?;
    load_task(&conn, &task_id)?.context("failed to reload inserted scheduled task")
}

fn emit_task_run_tx(
    root: &Path,
    tx: &Transaction<'_>,
    task: &ScheduledTaskView,
    scheduled_for: &str,
) -> Result<ScheduleRunView> {
    if let Some(payload) = meeting_join_payload(&task.prompt) {
        return emit_meeting_join_run_tx(root, tx, task, scheduled_for, &payload);
    }
    let run_id = format!("{}::{}", task.task_id, scheduled_for);
    let prompt = render_scheduled_prompt(task, scheduled_for);
    let message_key = channels::ingest_cron_message(
        root,
        &run_id,
        &task.thread_key,
        &task.name,
        &prompt,
        task.skill.as_deref(),
        scheduled_for,
    )?;
    let emitted_at = now_iso_string();
    tx.execute(
        r#"
        INSERT INTO scheduled_task_runs (
            run_id, task_id, scheduled_for, emitted_at, message_key, status, error_text
        ) VALUES (?1, ?2, ?3, ?4, ?5, 'emitted', '')
        ON CONFLICT(run_id) DO UPDATE SET
            emitted_at = excluded.emitted_at,
            message_key = excluded.message_key,
            status = excluded.status,
            error_text = excluded.error_text
        "#,
        params![run_id, task.task_id, scheduled_for, emitted_at, message_key],
    )?;
    Ok(ScheduleRunView {
        run_id,
        task_id: task.task_id.clone(),
        scheduled_for: scheduled_for.to_string(),
        emitted_at,
        message_key,
        status: "emitted".to_string(),
    })
}

fn next_task_state_after_emit(
    is_one_shot_meeting_join: bool,
    run_status: &str,
    cron_expr: &str,
    scheduled_for: &str,
    now: DateTime<Utc>,
) -> Result<(Option<String>, bool)> {
    if is_one_shot_meeting_join {
        if run_status == "started" {
            return Ok((None, false));
        }
        return Ok((Some((now + Duration::minutes(1)).to_rfc3339()), true));
    }
    Ok((
        next_run_after(cron_expr, parse_rfc3339_utc(scheduled_for)?)?,
        true,
    ))
}

#[derive(Debug, Clone, Deserialize)]
struct MeetingJoinPayload {
    url: String,
    #[serde(default)]
    bot_name: Option<String>,
}

fn meeting_join_payload(prompt: &str) -> Option<MeetingJoinPayload> {
    prompt.lines().find_map(|line| {
        let raw = line.trim().strip_prefix(MEETING_JOIN_MARKER)?.trim();
        serde_json::from_str::<MeetingJoinPayload>(raw)
            .ok()
            .filter(|payload| !payload.url.trim().is_empty())
    })
}

fn emit_meeting_join_run_tx(
    root: &Path,
    tx: &Transaction<'_>,
    task: &ScheduledTaskView,
    scheduled_for: &str,
    payload: &MeetingJoinPayload,
) -> Result<ScheduleRunView> {
    let run_id = format!("{}::{}", task.task_id, scheduled_for);
    let message_key = format!("meeting-join::{}", stable_digest(&run_id));
    let emitted_at = now_iso_string();
    let spawn_result = spawn_meeting_join(root, task, scheduled_for, payload);
    let (status, error_text) = match spawn_result {
        Ok(pid) => ("started".to_string(), format!("pid={pid}")),
        Err(err) => ("failed".to_string(), err.to_string()),
    };
    tx.execute(
        r#"
        INSERT INTO scheduled_task_runs (
            run_id, task_id, scheduled_for, emitted_at, message_key, status, error_text
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(run_id) DO UPDATE SET
            emitted_at = excluded.emitted_at,
            message_key = excluded.message_key,
            status = excluded.status,
            error_text = excluded.error_text
        "#,
        params![
            run_id,
            task.task_id,
            scheduled_for,
            emitted_at,
            message_key,
            status,
            error_text
        ],
    )?;
    Ok(ScheduleRunView {
        run_id,
        task_id: task.task_id.clone(),
        scheduled_for: scheduled_for.to_string(),
        emitted_at,
        message_key,
        status,
    })
}

fn spawn_meeting_join(
    root: &Path,
    task: &ScheduledTaskView,
    scheduled_for: &str,
    payload: &MeetingJoinPayload,
) -> Result<u32> {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ctox"));
    let log_dir = root.join("runtime").join("meeting_sessions");
    fs::create_dir_all(&log_dir)
        .with_context(|| format!("failed to create meeting log dir {}", log_dir.display()))?;
    let log_path = log_dir.join(format!(
        "{}-join.log",
        stable_digest(&format!("{}:{scheduled_for}", task.task_id))
    ));
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open meeting join log {}", log_path.display()))?;
    let stderr = log_file
        .try_clone()
        .context("failed to clone meeting join log handle")?;
    let mut command = Command::new(exe);
    command
        .current_dir(root)
        .env("CTOX_ROOT", root)
        .arg("meeting")
        .arg("join")
        .arg(payload.url.trim())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(stderr));
    if let Some(bot_name) = payload
        .bot_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        command.arg("--name").arg(bot_name);
    }
    let child = command.spawn().with_context(|| {
        format!(
            "failed to spawn scheduled meeting join for {}",
            payload.url.trim()
        )
    })?;
    Ok(child.id())
}

fn render_scheduled_prompt(task: &ScheduledTaskView, scheduled_for: &str) -> String {
    let mut lines = vec![
        format!("Scheduled task: {}", task.name),
        format!("Scheduled for: {scheduled_for}"),
        "If work remains open after this run, leave exactly one open CTOX follow-up, plan, or queue item. A sentence in the reply does not count as open work.".to_string(),
    ];
    if let Some(skill) = task
        .skill
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("Preferred skill/tooling: {skill}"));
    }
    lines.push(String::new());
    lines.push(task.prompt.clone());
    lines.join("\n")
}

fn list_due_tasks(conn: &Connection, now: &DateTime<Utc>) -> Result<Vec<ScheduledTaskView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT task_id, name, cron_expr, prompt, thread_key, skill, enabled,
               next_run_at, last_run_at, created_at, updated_at
        FROM scheduled_tasks
        WHERE enabled = 1
          AND next_run_at IS NOT NULL
          AND next_run_at <= ?1
        ORDER BY next_run_at ASC, created_at ASC
        "#,
    )?;
    let rows = statement.query_map(params![now.to_rfc3339()], map_task_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_task(conn: &Connection, task_id: &str) -> Result<Option<ScheduledTaskView>> {
    conn.query_row(
        r#"
        SELECT task_id, name, cron_expr, prompt, thread_key, skill, enabled,
               next_run_at, last_run_at, created_at, updated_at
        FROM scheduled_tasks
        WHERE task_id = ?1
        LIMIT 1
        "#,
        params![task_id],
        map_task_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledTaskView> {
    Ok(ScheduledTaskView {
        task_id: row.get(0)?,
        name: row.get(1)?,
        cron_expr: row.get(2)?,
        prompt: row.get(3)?,
        thread_key: row.get(4)?,
        skill: row.get(5)?,
        enabled: row.get::<_, i64>(6)? != 0,
        next_run_at: row.get(7)?,
        last_run_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn open_schedule_db(root: &Path) -> Result<Connection> {
    let path = resolve_db_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create schedule db parent {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open schedule db {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout for schedules")?;
    let busy_timeout_ms = crate::persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = {busy_timeout_ms};

        CREATE TABLE IF NOT EXISTS scheduled_tasks (
            task_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            cron_expr TEXT NOT NULL,
            prompt TEXT NOT NULL,
            thread_key TEXT NOT NULL,
            skill TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            next_run_at TEXT,
            last_run_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS scheduled_task_runs (
            run_id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            scheduled_for TEXT NOT NULL,
            emitted_at TEXT NOT NULL,
            message_key TEXT NOT NULL,
            status TEXT NOT NULL,
            error_text TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_due
            ON scheduled_tasks(enabled, next_run_at);
        CREATE INDEX IF NOT EXISTS idx_scheduled_task_runs_task
            ON scheduled_task_runs(task_id, scheduled_for DESC);
        "#,
    ))?;
    Ok(conn)
}

fn schema_state(conn: &Connection) -> Result<serde_json::Value> {
    let task_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM scheduled_tasks", [], |row| row.get(0))?;
    let run_count: i64 = conn.query_row("SELECT COUNT(*) FROM scheduled_task_runs", [], |row| {
        row.get(0)
    })?;
    Ok(json!({
        "scheduled_tasks": task_count,
        "scheduled_runs": run_count,
    }))
}

fn resolve_db_path(root: &Path) -> std::path::PathBuf {
    root.join(DEFAULT_DB_RELATIVE_PATH)
}

fn parse_add_request(args: &[String]) -> Result<ScheduleCreateRequest> {
    Ok(ScheduleCreateRequest {
        name: required_flag_value(args, "--name")
            .context("usage: ctox schedule add --name <label> --cron '<expr>' --prompt <text>")?
            .to_string(),
        cron_expr: required_flag_value(args, "--cron")
            .context("usage: ctox schedule add --name <label> --cron '<expr>' --prompt <text>")?
            .to_string(),
        prompt: required_flag_value(args, "--prompt")
            .context("usage: ctox schedule add --name <label> --cron '<expr>' --prompt <text>")?
            .to_string(),
        thread_key: find_flag_value(args, "--thread-key").map(ToOwned::to_owned),
        skill: find_flag_value(args, "--skill").map(ToOwned::to_owned),
    })
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn print_json(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn now_iso_string() -> String {
    DateTime::<Utc>::from(SystemTime::now()).to_rfc3339()
}

fn now_utc() -> DateTime<Utc> {
    DateTime::<Utc>::from(SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn ensure_task_upserts_existing_schedule_by_name_and_thread() {
        let root = std::env::temp_dir().join(format!(
            "ctox_schedule_test_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("runtime")).expect("runtime dir");

        let first = ensure_task(
            &root,
            ScheduleEnsureRequest {
                name: "blocked-review".to_string(),
                cron_expr: "0 */6 * * *".to_string(),
                prompt: "first".to_string(),
                thread_key: "thread/demo".to_string(),
                skill: Some("follow-up-orchestrator".to_string()),
            },
        )
        .expect("first ensure");
        let second = ensure_task(
            &root,
            ScheduleEnsureRequest {
                name: "blocked-review".to_string(),
                cron_expr: "0 */3 * * *".to_string(),
                prompt: "second".to_string(),
                thread_key: "thread/demo".to_string(),
                skill: Some("follow-up-orchestrator".to_string()),
            },
        )
        .expect("second ensure");

        assert_eq!(first.task_id, second.task_id);
        assert_eq!(second.cron_expr, "0 */3 * * *");
        assert_eq!(second.prompt, "second");

        let tasks = list_tasks(&root).expect("list tasks");
        assert_eq!(tasks.len(), 1);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn meeting_join_payload_is_detected_from_schedule_prompt() {
        let prompt = r#"CTOX_MEETING_JOIN: {"url":"https://meet.google.com/abc-defg-hij","bot_name":"CTOX Notetaker"}"#;
        let payload = meeting_join_payload(prompt).expect("meeting join payload");
        assert_eq!(payload.url, "https://meet.google.com/abc-defg-hij");
        assert_eq!(payload.bot_name.as_deref(), Some("CTOX Notetaker"));
    }

    #[test]
    fn meeting_one_shot_retry_state_depends_on_spawn_status() {
        let now = DateTime::parse_from_rfc3339("2026-04-28T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (next_run, enabled) =
            next_task_state_after_emit(true, "started", "0 12 28 4 *", "2026-04-28T12:00:00Z", now)
                .expect("started state");
        assert_eq!(next_run, None);
        assert!(!enabled);

        let (next_run, enabled) =
            next_task_state_after_emit(true, "failed", "0 12 28 4 *", "2026-04-28T12:00:00Z", now)
                .expect("failed state");
        assert_eq!(next_run.as_deref(), Some("2026-04-28T12:01:00+00:00"));
        assert!(enabled);
    }

    #[test]
    fn emit_task_now_persists_observable_cron_message_with_scrape_prompt() {
        let root = std::env::temp_dir().join(format!(
            "ctox_schedule_emit_test_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("runtime")).expect("runtime dir");

        let task = ensure_task(
            &root,
            ScheduleEnsureRequest {
                name: "refresh scrape fixture".to_string(),
                cron_expr: "0 * * * *".to_string(),
                prompt: "Run target_key=fixture-multi-feed. Expect schema=articles.v1.".to_string(),
                thread_key: "scrape/fixture-multi-feed".to_string(),
                skill: Some("universal-scraping".to_string()),
            },
        )
        .expect("ensure task");

        let run = emit_task_now(&root, &task.task_id).expect("emit run");
        let db = Connection::open(root.join("runtime/ctox.sqlite3")).expect("open channel db");
        let row = db
            .query_row(
                "SELECT thread_key, subject, body_text, metadata_json FROM communication_messages WHERE message_key = ?1",
                params![run.message_key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .expect("load cron message");

        assert_eq!(row.0, "scrape/fixture-multi-feed");
        assert_eq!(row.1, "refresh scrape fixture");
        assert!(row.2.contains("Scheduled task: refresh scrape fixture"));
        assert!(row.2.contains("target_key=fixture-multi-feed"));
        assert!(row
            .2
            .contains("Preferred skill/tooling: universal-scraping"));
        assert!(row.3.contains("\"skill\":\"universal-scraping\""));
        assert!(row.3.contains("\"source\":\"ctox-schedule\""));
        let spawn_edge_count: i64 = db
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM ctox_core_spawn_edges
                WHERE child_entity_type = 'Message'
                  AND child_entity_id = ?1
                  AND spawn_kind = 'schedule-run-message'
                  AND parent_entity_type = 'ScheduleTask'
                  AND parent_entity_id = ?2
                  AND accepted = 1
                "#,
                params![&run.message_key, &task.name],
                |row| row.get(0),
            )
            .expect("load schedule spawn edge count");
        assert_eq!(spawn_edge_count, 1);

        let _ = std::fs::remove_dir_all(root);
    }
}

fn parse_rfc3339_utc(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("failed to parse RFC3339 timestamp {value}"))?
        .with_timezone(&Utc))
}

fn stable_digest(input: &str) -> String {
    use sha2::Digest;
    use sha2::Sha256;
    let digest = Sha256::digest(input.as_bytes());
    let hex = format!("{digest:x}");
    hex[..24].to_string()
}

fn validate_cron_expr(expr: &str) -> Result<()> {
    let _ = CronExpr::parse(expr)?;
    Ok(())
}

fn next_run_after(expr: &str, after: DateTime<Utc>) -> Result<Option<String>> {
    let parsed = CronExpr::parse(expr)?;
    let mut candidate = after
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .context("failed to normalize cron timestamp")?
        + Duration::minutes(1);
    for _ in 0..CRON_SCAN_MINUTES {
        if parsed.matches(&candidate) {
            return Ok(Some(candidate.to_rfc3339()));
        }
        candidate += Duration::minutes(1);
    }
    Ok(None)
}

#[derive(Debug, Clone)]
struct CronExpr {
    minute: FieldSpec,
    hour: FieldSpec,
    day_of_month: FieldSpec,
    month: FieldSpec,
    day_of_week: FieldSpec,
}

impl CronExpr {
    fn parse(expr: &str) -> Result<Self> {
        let parts = expr.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 5 {
            anyhow::bail!("cron expression must have 5 fields: minute hour day month weekday");
        }
        Ok(Self {
            minute: FieldSpec::parse(parts[0], 0, 59)?,
            hour: FieldSpec::parse(parts[1], 0, 23)?,
            day_of_month: FieldSpec::parse(parts[2], 1, 31)?,
            month: FieldSpec::parse(parts[3], 1, 12)?,
            day_of_week: FieldSpec::parse(parts[4], 0, 6)?,
        })
    }

    fn matches(&self, dt: &DateTime<Utc>) -> bool {
        self.minute.matches(dt.minute())
            && self.hour.matches(dt.hour())
            && self.day_of_month.matches(dt.day())
            && self.month.matches(dt.month())
            && self
                .day_of_week
                .matches(dt.weekday().num_days_from_sunday())
    }
}

#[derive(Debug, Clone)]
struct FieldSpec {
    any: bool,
    allowed: BTreeSet<u32>,
}

impl FieldSpec {
    fn parse(raw: &str, min: u32, max: u32) -> Result<Self> {
        if raw.trim() == "*" {
            return Ok(Self {
                any: true,
                allowed: BTreeSet::new(),
            });
        }
        let mut allowed = BTreeSet::new();
        for token in raw.split(',') {
            let token = token.trim();
            if token.is_empty() {
                anyhow::bail!("invalid empty cron token in {raw}");
            }
            expand_token(token, min, max, &mut allowed)?;
        }
        if allowed.is_empty() {
            anyhow::bail!("cron field {raw} produced no allowed values");
        }
        Ok(Self {
            any: false,
            allowed,
        })
    }

    fn matches(&self, value: u32) -> bool {
        self.any || self.allowed.contains(&value)
    }
}

fn expand_token(token: &str, min: u32, max: u32, allowed: &mut BTreeSet<u32>) -> Result<()> {
    if token == "*" {
        for value in min..=max {
            allowed.insert(value);
        }
        return Ok(());
    }
    let (base, step) = if let Some((left, right)) = token.split_once('/') {
        let step = right
            .parse::<u32>()
            .with_context(|| format!("invalid cron step {right}"))?;
        if step == 0 {
            anyhow::bail!("cron step must be > 0");
        }
        (left, step)
    } else {
        (token, 1)
    };

    let (start, end) = if base == "*" {
        (min, max)
    } else if let Some((left, right)) = base.split_once('-') {
        let start = parse_field_value(left, min, max)?;
        let end = parse_field_value(right, min, max)?;
        if end < start {
            anyhow::bail!("invalid cron range {base}");
        }
        (start, end)
    } else {
        let value = parse_field_value(base, min, max)?;
        (value, value)
    };

    let mut current = start;
    while current <= end {
        allowed.insert(current);
        match current.checked_add(step) {
            Some(next) if next > current => current = next,
            _ => break,
        }
    }
    Ok(())
}

fn parse_field_value(raw: &str, min: u32, max: u32) -> Result<u32> {
    let value = raw
        .parse::<u32>()
        .with_context(|| format!("invalid cron field value {raw}"))?;
    if !(min..=max).contains(&value) {
        anyhow::bail!("cron field value {value} out of range {min}..={max}");
    }
    Ok(value)
}
