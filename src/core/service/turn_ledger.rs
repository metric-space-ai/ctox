// Origin: CTOX
// License: Apache-2.0

use anyhow::Context;
use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use rusqlite::Connection;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct CliCommandLedger {
    db_path: std::path::PathBuf,
    turn_id: String,
    command_id: String,
    auto_end_implicit_turn: bool,
}

impl CliCommandLedger {
    pub fn start(root: &Path, args: &[String]) -> Result<Self> {
        let db_path = crate::paths::core_db(root);
        ensure_db_parent(&db_path)?;
        let mut conn = open_turn_ledger_connection(&db_path)?;
        crate::service::process_mining::attach_sqlite_access_recorder(&conn, &db_path);
        ensure_turn_ledger_schema(&conn)?;
        crate::service::process_mining::ensure_process_mining_schema(&conn, &db_path)?;

        let (turn_id, auto_end_implicit_turn) = resolve_or_create_turn_id(&mut conn, args)?;
        let command_id = new_id("cmd", args);
        let command_name = args
            .first()
            .map(String::as_str)
            .unwrap_or("tui")
            .to_string();
        let argv_json = serde_json::to_string(args)?;
        let argv_sha256 = full_sha256_hex(&argv_json);
        let started_at = now_iso();

        crate::service::process_mining::activate_command_context(
            &conn,
            &turn_id,
            &command_id,
            &actor_key(),
            "cli",
            &command_name,
            &argv_sha256,
        )?;

        conn.execute(
            r#"
            INSERT INTO ctox_turn_commands (
                command_id, turn_id, command_name, argv_json, argv_sha256,
                started_at, status, process_id
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running', ?7)
            "#,
            params![
                command_id,
                turn_id,
                command_name,
                argv_json,
                argv_sha256,
                started_at,
                std::process::id().to_string()
            ],
        )?;
        conn.execute(
            r#"
            UPDATE ctox_turns
            SET command_count = command_count + 1,
                last_command_id = ?1,
                updated_at = ?2
            WHERE turn_id = ?3
            "#,
            params![command_id, started_at, turn_id],
        )?;

        Ok(Self {
            db_path,
            turn_id,
            command_id,
            auto_end_implicit_turn,
        })
    }

    pub fn finish(&mut self, result: &Result<()>) -> Result<()> {
        let conn = open_turn_ledger_connection(&self.db_path)?;
        crate::service::process_mining::attach_sqlite_access_recorder(&conn, &self.db_path);
        ensure_turn_ledger_schema(&conn)?;
        crate::service::process_mining::ensure_process_mining_schema(&conn, &self.db_path)?;

        let finished_at = now_iso();
        let (status, exit_code, error_text) = match result {
            Ok(()) => ("succeeded", 0_i64, None),
            Err(error) => ("failed", 1_i64, Some(format!("{error:#}"))),
        };

        conn.execute(
            r#"
            UPDATE ctox_turn_commands
            SET finished_at = ?1,
                status = ?2,
                exit_code = ?3,
                error_text = ?4
            WHERE command_id = ?5
            "#,
            params![finished_at, status, exit_code, error_text, self.command_id],
        )?;
        conn.execute(
            r#"
            UPDATE ctox_turns
            SET failed_command_count = failed_command_count + CASE WHEN ?1 = 'failed' THEN 1 ELSE 0 END,
                updated_at = ?2
            WHERE turn_id = ?3
            "#,
            params![status, finished_at, self.turn_id],
        )?;
        crate::service::process_mining::finish_command_context(&conn, &self.command_id, status)?;
        crate::service::process_mining::flush_sqlite_access_events(
            &conn,
            &self.db_path,
            &self.command_id,
        )?;
        if self.auto_end_implicit_turn {
            let terminal_status = if result.is_ok() { "done" } else { "invalid" };
            let terminal_reason = if result.is_ok() {
                "implicit CLI command completed"
            } else {
                "implicit CLI command failed"
            };
            conn.execute(
                r#"
                UPDATE ctox_turns
                SET status = 'terminal',
                    ended_at = ?1,
                    terminal_status = ?2,
                    terminal_reason = ?3,
                    updated_at = ?1
                WHERE turn_id = ?4
                "#,
                params![finished_at, terminal_status, terminal_reason, self.turn_id],
            )?;
        }

        Ok(())
    }

    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }
}

fn open_turn_ledger_connection(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("failed to open turn ledger db {}", db_path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .with_context(|| {
            format!(
                "failed to configure SQLite busy_timeout for turn ledger {}",
                db_path.display()
            )
        })?;
    let busy_timeout_ms = crate::persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        "PRAGMA busy_timeout={busy_timeout_ms};\nPRAGMA journal_mode=WAL;"
    ))
    .with_context(|| {
        format!(
            "failed to configure SQLite pragmas for turn ledger {}",
            db_path.display()
        )
    })?;
    Ok(conn)
}

pub fn ensure_turn_ledger_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS ctox_turns (
            turn_id TEXT PRIMARY KEY,
            actor_key TEXT NOT NULL,
            source TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            terminal_status TEXT,
            terminal_reason TEXT,
            command_count INTEGER NOT NULL DEFAULT 0,
            failed_command_count INTEGER NOT NULL DEFAULT 0,
            last_command_id TEXT,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ctox_turn_commands (
            command_id TEXT PRIMARY KEY,
            turn_id TEXT NOT NULL,
            command_name TEXT NOT NULL,
            argv_json TEXT NOT NULL,
            argv_sha256 TEXT NOT NULL,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            status TEXT NOT NULL,
            exit_code INTEGER,
            error_text TEXT,
            process_id TEXT,
            result_json TEXT,
            FOREIGN KEY(turn_id) REFERENCES ctox_turns(turn_id)
        );

        CREATE TABLE IF NOT EXISTS ctox_turn_violations (
            violation_id TEXT PRIMARY KEY,
            turn_id TEXT NOT NULL,
            command_id TEXT,
            code TEXT NOT NULL,
            message TEXT NOT NULL,
            recovery_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            FOREIGN KEY(turn_id) REFERENCES ctox_turns(turn_id),
            FOREIGN KEY(command_id) REFERENCES ctox_turn_commands(command_id)
        );

        CREATE INDEX IF NOT EXISTS idx_ctox_turn_commands_turn
          ON ctox_turn_commands(turn_id, started_at);
        CREATE INDEX IF NOT EXISTS idx_ctox_turn_violations_turn
          ON ctox_turn_violations(turn_id, created_at);
        "#,
    )?;
    Ok(())
}

fn resolve_or_create_turn_id(conn: &mut Connection, args: &[String]) -> Result<(String, bool)> {
    if let Some(turn_id) = env_value("CTOX_TURN_ID").or_else(|| env_value("CODEX_TURN_ID")) {
        ensure_turn_row(conn, &turn_id, "env")?;
        return Ok((turn_id, false));
    }

    let turn_id = new_id("implicit-turn", args);
    ensure_turn_row(conn, &turn_id, "implicit_cli_command")?;
    Ok((turn_id, true))
}

fn ensure_turn_row(conn: &mut Connection, turn_id: &str, source: &str) -> Result<()> {
    let now = now_iso();
    let actor_key = actor_key();
    conn.execute(
        r#"
        INSERT INTO ctox_turns (
            turn_id, actor_key, source, status, started_at, updated_at
        )
        VALUES (?1, ?2, ?3, 'active', ?4, ?4)
        ON CONFLICT(turn_id) DO UPDATE SET
            updated_at = excluded.updated_at
        "#,
        params![turn_id, actor_key, source, now],
    )?;
    Ok(())
}

fn actor_key() -> String {
    env_value("CTOX_AGENT_ID")
        .or_else(|| env_value("CODEX_AGENT_ID"))
        .unwrap_or_else(|| "unknown-agent".to_string())
}

pub fn invalid_terminal_turn_message(turn_id: &str, command_id: Option<&str>) -> serde_json::Value {
    json!({
        "ok": false,
        "blocked_by": "ctox_turn_ledger",
        "turn_id": turn_id,
        "command_id": command_id,
        "violation": {
            "code": "missing_terminal_transition",
            "message": "The multi-turn did not finish with a valid terminal CTOX transition."
        },
        "required_next_actions": [
            "call an allowed terminal CTOX command once terminal commands are wired",
            "or continue the turn until the active operation is blocked, escalated, waiting_external, or done"
        ],
        "must_not_claim_done_without_terminal_transition": true
    })
}

pub fn handle_turn_command(root: &Path, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("end") => {
            let status = required_flag_value(args, "--status")?;
            let reason = find_flag_value(args, "--reason").unwrap_or("");
            let turn_id = find_flag_value(args, "--turn-id")
                .map(ToOwned::to_owned)
                .or_else(|| env_value("CTOX_TURN_ID"))
                .or_else(|| env_value("CODEX_TURN_ID"))
                .context("usage: ctox turn end --status <done|blocked|escalated|waiting_external|invalid> [--reason <text>] [--turn-id <id>]")?;
            end_turn(root, &turn_id, status, reason)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "turn_id": turn_id,
                    "terminal_status": status,
                    "terminal_reason": reason,
                }))?
            );
            Ok(())
        }
        Some("status") => {
            let turn_id = find_flag_value(args, "--turn-id")
                .map(ToOwned::to_owned)
                .or_else(|| env_value("CTOX_TURN_ID"))
                .or_else(|| env_value("CODEX_TURN_ID"));
            let snapshot = turn_status(root, turn_id.as_deref())?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
            Ok(())
        }
        _ => anyhow::bail!(
            "usage:\n  ctox turn end --status <done|blocked|escalated|waiting_external|invalid> [--reason <text>] [--turn-id <id>]\n  ctox turn status [--turn-id <id>]"
        ),
    }
}

pub fn end_turn(root: &Path, turn_id: &str, terminal_status: &str, reason: &str) -> Result<()> {
    validate_terminal_status(terminal_status)?;
    let db_path = crate::paths::core_db(root);
    ensure_db_parent(&db_path)?;
    let mut conn = open_turn_ledger_connection(&db_path)?;
    ensure_turn_ledger_schema(&conn)?;
    ensure_turn_row(&mut conn, turn_id, "terminal_command")?;
    let ended_at = now_iso();
    conn.execute(
        r#"
        UPDATE ctox_turns
        SET status = 'terminal',
            ended_at = ?1,
            terminal_status = ?2,
            terminal_reason = ?3,
            updated_at = ?1
        WHERE turn_id = ?4
        "#,
        params![ended_at, terminal_status, reason, turn_id],
    )?;
    Ok(())
}

pub fn turn_status(root: &Path, turn_id: Option<&str>) -> Result<serde_json::Value> {
    let db_path = crate::paths::core_db(root);
    ensure_db_parent(&db_path)?;
    let conn = open_turn_ledger_connection(&db_path)?;
    ensure_turn_ledger_schema(&conn)?;

    if let Some(turn_id) = turn_id {
        let turn = conn.query_row(
            r#"
            SELECT turn_id, actor_key, source, status, started_at, ended_at,
                   terminal_status, terminal_reason, command_count,
                   failed_command_count, last_command_id
            FROM ctox_turns
            WHERE turn_id = ?1
            "#,
            [turn_id],
            |row| {
                Ok(json!({
                    "turn_id": row.get::<_, String>(0)?,
                    "actor_key": row.get::<_, String>(1)?,
                    "source": row.get::<_, String>(2)?,
                    "status": row.get::<_, String>(3)?,
                    "started_at": row.get::<_, String>(4)?,
                    "ended_at": row.get::<_, Option<String>>(5)?,
                    "terminal_status": row.get::<_, Option<String>>(6)?,
                    "terminal_reason": row.get::<_, Option<String>>(7)?,
                    "command_count": row.get::<_, i64>(8)?,
                    "failed_command_count": row.get::<_, i64>(9)?,
                    "last_command_id": row.get::<_, Option<String>>(10)?,
                }))
            },
        )?;
        return Ok(turn);
    }

    let mut stmt = conn.prepare(
        r#"
        SELECT turn_id, source, status, started_at, ended_at,
               terminal_status, command_count, failed_command_count
        FROM ctox_turns
        ORDER BY updated_at DESC
        LIMIT 20
        "#,
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(json!({
                "turn_id": row.get::<_, String>(0)?,
                "source": row.get::<_, String>(1)?,
                "status": row.get::<_, String>(2)?,
                "started_at": row.get::<_, String>(3)?,
                "ended_at": row.get::<_, Option<String>>(4)?,
                "terminal_status": row.get::<_, Option<String>>(5)?,
                "command_count": row.get::<_, i64>(6)?,
                "failed_command_count": row.get::<_, i64>(7)?,
            }))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(json!({
        "ok": true,
        "turns": rows,
    }))
}

fn validate_terminal_status(status: &str) -> Result<()> {
    if matches!(
        status,
        "done" | "blocked" | "escalated" | "waiting_external" | "no_action_needed" | "invalid"
    ) {
        return Ok(());
    }
    anyhow::bail!(
        "invalid terminal status `{}`; allowed: done, blocked, escalated, waiting_external, no_action_needed, invalid",
        status
    )
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Result<&'a str> {
    find_flag_value(args, flag).with_context(|| format!("missing required flag {flag}"))
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair.first().map(String::as_str) == Some(flag))
        .and_then(|pair| pair.get(1))
        .map(String::as_str)
}

fn env_value(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn ensure_db_parent(db_path: &Path) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

fn new_id(prefix: &str, args: &[String]) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let payload = format!(
        "{}:{}:{}:{}",
        std::process::id(),
        now,
        prefix,
        args.join("\u{1f}")
    );
    format!("{prefix}-{}", short_sha256_hex(&payload))
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn full_sha256_hex(input: &str) -> String {
    format!("{:x}", Sha256::digest(input.as_bytes()))
}

fn short_sha256_hex(input: &str) -> String {
    let hex = full_sha256_hex(input);
    hex[..24].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Mutex, OnceLock};

    #[test]
    fn command_ledger_records_successful_cli_call() {
        let _env_guard = clear_turn_env_for_test();
        let root = unique_test_dir("success");
        let args = vec!["status".to_string()];
        let mut ledger = CliCommandLedger::start(&root, &args).expect("start ledger");
        let turn_id = ledger.turn_id().to_string();
        let command_id = ledger.command_id().to_string();

        ledger.finish(&Ok(())).expect("finish ledger");

        let conn = Connection::open(crate::paths::core_db(&root)).expect("open db");
        let (status, terminal_status): (String, String) = conn
            .query_row(
                r#"
                SELECT c.status, t.terminal_status
                FROM ctox_turn_commands c
                JOIN ctox_turns t ON t.turn_id = c.turn_id
                WHERE c.command_id = ?1
                "#,
                [command_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("command row");
        let command_count: i64 = conn
            .query_row(
                "SELECT command_count FROM ctox_turns WHERE turn_id = ?1",
                [turn_id],
                |row| row.get(0),
            )
            .expect("turn row");

        assert_eq!(status, "succeeded");
        assert_eq!(terminal_status, "done");
        assert_eq!(command_count, 1);
        cleanup_test_dir(&root);
    }

    #[test]
    fn command_ledger_records_failed_cli_call() {
        let _env_guard = clear_turn_env_for_test();
        let root = unique_test_dir("failed");
        let args = vec!["queue".to_string(), "unknown".to_string()];
        let mut ledger = CliCommandLedger::start(&root, &args).expect("start ledger");
        let command_id = ledger.command_id().to_string();
        let error = anyhow::anyhow!("boom");

        ledger.finish(&Err(error)).expect("finish ledger");

        let conn = Connection::open(crate::paths::core_db(&root)).expect("open db");
        let (status, exit_code, terminal_status): (String, i64, String) = conn
            .query_row(
                r#"
                SELECT c.status, c.exit_code, t.terminal_status
                FROM ctox_turn_commands c
                JOIN ctox_turns t ON t.turn_id = c.turn_id
                WHERE c.command_id = ?1
                "#,
                [command_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("command row");

        assert_eq!(status, "failed");
        assert_eq!(exit_code, 1);
        assert_eq!(terminal_status, "invalid");
        cleanup_test_dir(&root);
    }

    #[test]
    fn env_turn_stays_active_until_terminal_command() {
        let _env_guard = clear_turn_env_for_test();
        let root = unique_test_dir("env-active");
        let args = vec!["status".to_string()];
        std::env::set_var("CTOX_TURN_ID", "turn-explicit-test");
        let mut ledger = CliCommandLedger::start(&root, &args).expect("start ledger");
        ledger.finish(&Ok(())).expect("finish ledger");
        std::env::remove_var("CTOX_TURN_ID");

        let conn = Connection::open(crate::paths::core_db(&root)).expect("open db");
        let (status, terminal_status): (String, Option<String>) = conn
            .query_row(
                "SELECT status, terminal_status FROM ctox_turns WHERE turn_id = ?1",
                ["turn-explicit-test"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("turn row");

        assert_eq!(status, "active");
        assert_eq!(terminal_status, None);
        cleanup_test_dir(&root);
    }

    struct TurnEnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        ctox_turn_id: Option<String>,
        codex_turn_id: Option<String>,
    }

    impl Drop for TurnEnvGuard {
        fn drop(&mut self) {
            restore_env("CTOX_TURN_ID", self.ctox_turn_id.as_deref());
            restore_env("CODEX_TURN_ID", self.codex_turn_id.as_deref());
        }
    }

    fn clear_turn_env_for_test() -> TurnEnvGuard {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let lock = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let guard = TurnEnvGuard {
            _lock: lock,
            ctox_turn_id: std::env::var("CTOX_TURN_ID").ok(),
            codex_turn_id: std::env::var("CODEX_TURN_ID").ok(),
        };
        std::env::remove_var("CTOX_TURN_ID");
        std::env::remove_var("CODEX_TURN_ID");
        guard
    }

    fn restore_env(key: &str, value: Option<&str>) {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn end_turn_records_terminal_status() {
        let root = unique_test_dir("terminal");
        let turn_id = "turn-terminal-test";

        end_turn(&root, turn_id, "blocked", "missing_review").expect("end turn");

        let conn = Connection::open(crate::paths::core_db(&root)).expect("open db");
        let (status, terminal_status, reason): (String, String, String) = conn
            .query_row(
                "SELECT status, terminal_status, terminal_reason FROM ctox_turns WHERE turn_id = ?1",
                [turn_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("turn row");

        assert_eq!(status, "terminal");
        assert_eq!(terminal_status, "blocked");
        assert_eq!(reason, "missing_review");
        cleanup_test_dir(&root);
    }

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ctox-turn-ledger-tests-{name}-{unique}"))
    }

    fn cleanup_test_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }
}
