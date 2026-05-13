// Origin: CTOX
// License: Apache-2.0

use crate::service::core_state_machine as csm;
use crate::service::core_transition_guard;
use anyhow::Context;
use anyhow::Result;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::params;
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::Digest;
use sha2::Sha256;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

const PROCESS_CONTEXT_TABLE: &str = "ctox_process_context";
const PROCESS_EVENTS_TABLE: &str = "ctox_process_events";
const PROCESS_TRIGGER_REGISTRY_TABLE: &str = "ctox_process_trigger_registry";
const PROCESS_MODEL_TABLE_PREFIX: &str = "ctox_pm_";
const SCHEMA_VERSION: i64 = 2;
const PROCESS_MINING_USAGE: &str = "usage:
  ctox process-mining ensure
  ctox process-mining schema
  ctox process-mining inventory
  ctox process-mining events [--limit <n>]
  ctox process-mining projection [--case-id <id>] [--limit <n>]
  ctox process-mining cases [--limit <n>]
  ctox process-mining case <case-id> [--limit <n>]
  ctox process-mining objects [--limit <n>]
  ctox process-mining transitions [--limit <n>]
  ctox process-mining dfg [--limit <n>]
  ctox process-mining core-liveness
  ctox process-mining spawn-liveness
  ctox process-mining spawn-edges [--limit <n>]
  ctox process-mining explain-case <case-id> [--limit <n>]
  ctox process-mining deadlocks [--limit <n>]
  ctox process-mining mapping-rules [--limit <n>]
  ctox process-mining proofs [--limit <n>]
  ctox process-mining state-scan [--limit <n>]
  ctox process-mining assert-clean [--limit <n>] [--allow-rejected]
  ctox process-mining self-diagnose [--limit <n>]
  ctox process-mining guidance [--limit <n>]
  ctox process-mining state-audit [--limit <n>]
  ctox process-mining coverage [--limit <n>]
  ctox process-mining violations [--limit <n>]
  ctox process-mining prune [--sqlite-access-window <n>]
  ctox process-mining scan-violations";
static SQLITE_ACCESS_BUFFER: OnceLock<Mutex<Vec<SqliteAccessRecord>>> = OnceLock::new();
static SQLITE_ACCESS_SEQ: AtomicU64 = AtomicU64::new(1);
static SQLITE_ACCESS_RETENTION_FLUSHES: AtomicU64 = AtomicU64::new(0);
const SQLITE_ACCESS_READS_ENV: &str = "CTOX_PROCESS_MINING_RECORD_SQLITE_READS";
const SQLITE_ACCESS_MAX_EVENTS_PER_COMMAND: usize = 512;
const SQLITE_ACCESS_RETENTION_WINDOW: i64 = 200_000;
const SQLITE_ACCESS_RETENTION_INTERVAL_FLUSHES: u64 = 128;

#[derive(Debug, Clone)]
struct TableInfo {
    name: String,
    sql: String,
}

#[derive(Debug, Clone)]
struct ColumnInfo {
    name: String,
    decl_type: String,
    pk_rank: i64,
}

#[derive(Debug, Clone)]
struct SqliteAccessRecord {
    observed_at: String,
    db_path: String,
    operation: String,
    table_name: String,
    column_name: Option<String>,
    action: String,
    database_name: Option<String>,
    accessor: Option<String>,
}

#[derive(Debug, Clone)]
struct ProcessEventForStateMachine {
    event_id: String,
    observed_at: String,
    case_id: String,
    activity: String,
    entity_type: String,
    entity_id: String,
    table_name: String,
    operation: String,
    from_state: Option<String>,
    to_state: Option<String>,
    row_before_json: String,
    row_after_json: String,
    command_name: Option<String>,
}

#[derive(Debug, Clone)]
struct CoreTransitionRule {
    rule_id: String,
    priority: i64,
    table_pattern: Option<String>,
    entity_type_pattern: Option<String>,
    operation_pattern: Option<String>,
    activity_pattern: Option<String>,
    inference_kind: String,
    core_entity_type: String,
    runtime_lane: String,
    petri_transition_id: String,
    evidence_policy_json: String,
}

fn drop_legacy_rust4pm_tables(conn: &Connection) -> Result<()> {
    // Removed in the harness-mining migration: the rust4pm-vendored
    // discovery/Petri/conformance schema was never populated in production
    // and is replaced by the harness-mining tier-1/2 modules. We drop here
    // (rather than ignore) so live deployments reclaim the disk and so that
    // re-introducing one of these names later cannot silently inherit stale
    // foreign-key dependencies.
    conn.execute_batch(
        r#"
        DROP INDEX IF EXISTS idx_ctox_pm_dfg_edges_model_frequency;
        DROP INDEX IF EXISTS idx_ctox_pm_conformance_model_started;
        DROP TABLE IF EXISTS ctox_pm_petri_markings;
        DROP TABLE IF EXISTS ctox_pm_petri_arcs;
        DROP TABLE IF EXISTS ctox_pm_petri_transitions;
        DROP TABLE IF EXISTS ctox_pm_petri_places;
        DROP TABLE IF EXISTS ctox_pm_dfg_edges;
        DROP TABLE IF EXISTS ctox_pm_dfg_activities;
        DROP TABLE IF EXISTS ctox_pm_conformance_runs;
        DROP TABLE IF EXISTS ctox_pm_process_models;
        DROP TABLE IF EXISTS ctox_pm_case_classifiers;
        "#,
    )?;
    Ok(())
}

pub fn ensure_process_mining_schema(conn: &Connection, db_path: &Path) -> Result<()> {
    core_transition_guard::ensure_core_transition_guard_schema(conn)?;
    drop_legacy_rust4pm_tables(conn)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS ctox_process_context (
            command_id TEXT PRIMARY KEY,
            turn_id TEXT NOT NULL,
            actor_key TEXT NOT NULL,
            source TEXT NOT NULL,
            command_name TEXT NOT NULL,
            argv_sha256 TEXT NOT NULL,
            process_id TEXT NOT NULL,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            status TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ctox_process_events (
            event_seq INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id TEXT NOT NULL UNIQUE,
            observed_at TEXT NOT NULL,
            case_id TEXT NOT NULL,
            activity TEXT NOT NULL,
            lifecycle_transition TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            table_name TEXT NOT NULL,
            operation TEXT NOT NULL,
            from_state TEXT,
            to_state TEXT,
            primary_key_json TEXT NOT NULL,
            row_before_json TEXT NOT NULL,
            row_after_json TEXT NOT NULL,
            changed_columns_json TEXT NOT NULL,
            turn_id TEXT,
            command_id TEXT,
            actor_key TEXT,
            source TEXT,
            command_name TEXT,
            db_path TEXT NOT NULL,
            metadata_json TEXT NOT NULL DEFAULT '{}'
        );

        CREATE INDEX IF NOT EXISTS idx_ctox_process_events_case_time
          ON ctox_process_events(case_id, observed_at, event_seq);
        CREATE INDEX IF NOT EXISTS idx_ctox_process_events_activity_time
          ON ctox_process_events(activity, observed_at);
        CREATE INDEX IF NOT EXISTS idx_ctox_process_events_command
          ON ctox_process_events(command_id, event_seq);
        CREATE INDEX IF NOT EXISTS idx_ctox_process_events_entity
          ON ctox_process_events(entity_type, entity_id, event_seq);

        CREATE TABLE IF NOT EXISTS ctox_process_trigger_registry (
            table_name TEXT PRIMARY KEY,
            trigger_insert TEXT,
            trigger_update TEXT,
            trigger_delete TEXT,
            schema_version INTEGER NOT NULL,
            installed_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ctox_pm_state_violations (
            violation_id TEXT PRIMARY KEY,
            event_id TEXT,
            case_id TEXT NOT NULL,
            violation_code TEXT NOT NULL,
            severity TEXT NOT NULL,
            message TEXT NOT NULL,
            detected_at TEXT NOT NULL,
            evidence_json TEXT NOT NULL DEFAULT '{}'
        );

        CREATE TABLE IF NOT EXISTS ctox_pm_core_transition_audit (
            audit_id TEXT PRIMARY KEY,
            event_id TEXT NOT NULL,
            case_id TEXT NOT NULL,
            rule_id TEXT,
            petri_transition_id TEXT,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            lane TEXT NOT NULL,
            from_state TEXT NOT NULL,
            to_state TEXT NOT NULL,
            core_event TEXT NOT NULL,
            accepted INTEGER NOT NULL,
            violation_codes_json TEXT NOT NULL DEFAULT '[]',
            proof_id TEXT,
            request_json TEXT NOT NULL DEFAULT '{}',
            observed_at TEXT NOT NULL,
            scanned_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ctox_pm_core_transition_rules (
            rule_id TEXT PRIMARY KEY,
            priority INTEGER NOT NULL,
            table_pattern TEXT,
            entity_type_pattern TEXT,
            operation_pattern TEXT,
            activity_pattern TEXT,
            inference_kind TEXT NOT NULL,
            core_entity_type TEXT NOT NULL,
            runtime_lane TEXT NOT NULL,
            petri_transition_id TEXT NOT NULL,
            evidence_policy_json TEXT NOT NULL DEFAULT '{}',
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ctox_pm_event_transition_coverage (
            event_id TEXT PRIMARY KEY,
            case_id TEXT NOT NULL,
            table_name TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            operation TEXT NOT NULL,
            activity TEXT NOT NULL,
            mapping_kind TEXT NOT NULL,
            rule_id TEXT,
            petri_transition_id TEXT,
            reason TEXT NOT NULL,
            observed_at TEXT NOT NULL,
            scanned_at TEXT NOT NULL,
            metadata_json TEXT NOT NULL DEFAULT '{}'
        );

        CREATE TABLE IF NOT EXISTS ctox_pm_unmapped_events (
            event_id TEXT PRIMARY KEY,
            case_id TEXT NOT NULL,
            table_name TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            operation TEXT NOT NULL,
            activity TEXT NOT NULL,
            reason TEXT NOT NULL,
            observed_at TEXT NOT NULL,
            scanned_at TEXT NOT NULL,
            metadata_json TEXT NOT NULL DEFAULT '{}'
        );

        CREATE INDEX IF NOT EXISTS idx_ctox_pm_state_violations_detected
          ON ctox_pm_state_violations(detected_at DESC);
        CREATE INDEX IF NOT EXISTS idx_ctox_pm_core_transition_audit_event
          ON ctox_pm_core_transition_audit(event_id);
        CREATE INDEX IF NOT EXISTS idx_ctox_pm_core_transition_audit_scanned
          ON ctox_pm_core_transition_audit(scanned_at DESC);
        CREATE INDEX IF NOT EXISTS idx_ctox_pm_core_transition_rules_priority
          ON ctox_pm_core_transition_rules(enabled, priority, rule_id);
        CREATE INDEX IF NOT EXISTS idx_ctox_pm_event_transition_coverage_scanned
          ON ctox_pm_event_transition_coverage(scanned_at DESC, mapping_kind);
        CREATE INDEX IF NOT EXISTS idx_ctox_pm_unmapped_events_scanned
          ON ctox_pm_unmapped_events(scanned_at DESC);
        "#,
    )?;
    ensure_table_column(
        conn,
        "ctox_pm_core_transition_audit",
        "rule_id",
        "rule_id TEXT",
    )?;
    ensure_table_column(
        conn,
        "ctox_pm_core_transition_audit",
        "petri_transition_id",
        "petri_transition_id TEXT",
    )?;
    ensure_table_column(
        conn,
        "ctox_pm_core_transition_audit",
        "proof_id",
        "proof_id TEXT",
    )?;
    install_process_mining_views(conn)?;
    upsert_default_core_transition_rules(conn)?;

    let db_path_text = db_path.to_string_lossy().to_string();
    for table in list_instrumentable_tables(conn)? {
        if table_triggers_current(conn, &table.name)? {
            continue;
        }
        install_table_triggers(conn, &table, &db_path_text)?;
    }
    Ok(())
}

pub fn attach_sqlite_access_recorder(conn: &Connection, db_path: &Path) {
    let db_path_text = db_path.to_string_lossy().to_string();
    conn.authorizer(Some(move |ctx: AuthContext<'_>| {
        if let Some(record) = sqlite_access_record_from_auth_context(ctx, &db_path_text) {
            if let Ok(mut records) = SQLITE_ACCESS_BUFFER
                .get_or_init(|| Mutex::new(Vec::new()))
                .lock()
            {
                records.push(record);
            }
        }
        Authorization::Allow
    }));
}

fn sqlite_access_read_recording_enabled() -> bool {
    std::env::var(SQLITE_ACCESS_READS_ENV)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn compact_sqlite_access_records(records: Vec<SqliteAccessRecord>) -> Vec<SqliteAccessRecord> {
    let mut seen = BTreeSet::new();
    let mut compacted = Vec::new();
    for record in records {
        let key = (
            record.operation.clone(),
            record.table_name.clone(),
            record.column_name.clone(),
            record.action.clone(),
            record.database_name.clone(),
            record.accessor.clone(),
        );
        if seen.insert(key) {
            compacted.push(record);
        }
        if compacted.len() >= SQLITE_ACCESS_MAX_EVENTS_PER_COMMAND {
            break;
        }
    }
    compacted
}

pub fn flush_sqlite_access_events(
    conn: &Connection,
    db_path: &Path,
    command_id: &str,
) -> Result<usize> {
    let db_path_text = db_path.to_string_lossy().to_string();
    let mut records = Vec::new();
    if let Ok(mut guard) = SQLITE_ACCESS_BUFFER
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
    {
        let mut retained = Vec::new();
        for record in guard.drain(..) {
            if record.db_path == db_path_text {
                records.push(record);
            } else {
                retained.push(record);
            }
        }
        *guard = retained;
    }

    records = compact_sqlite_access_records(records);

    if records.is_empty() {
        return Ok(0);
    }

    let context = conn
        .query_row(
            r#"
            SELECT turn_id, actor_key, source, command_name
            FROM ctox_process_context
            WHERE command_id = ?1
            "#,
            params![command_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .ok();
    let (turn_id, actor_key, source, command_name) = context.unwrap_or_else(|| {
        (
            "unknown-turn".to_string(),
            "unknown-actor".to_string(),
            "sqlite-authorizer".to_string(),
            "unknown-command".to_string(),
        )
    });

    let mut inserted = 0usize;
    for record in records {
        if is_process_mining_internal_table(&record.table_name) {
            continue;
        }
        let sequence = SQLITE_ACCESS_SEQ.fetch_add(1, Ordering::Relaxed);
        let entity_id = json!({
            "table": record.table_name,
            "column": record.column_name,
        })
        .to_string();
        let table_name = if record.table_name.is_empty() {
            "sqlite_statement"
        } else {
            record.table_name.as_str()
        };
        conn.execute(
            r#"
            INSERT INTO ctox_process_events (
                event_id, observed_at, case_id, activity, lifecycle_transition,
                entity_type, entity_id, table_name, operation, from_state, to_state,
                primary_key_json, row_before_json, row_after_json, changed_columns_json,
                turn_id, command_id, actor_key, source, command_name, db_path, metadata_json
            )
            VALUES (
                ?1, ?2, ?3, ?4, 'access',
                ?5, ?6, ?7, ?8, NULL, NULL,
                ?9, json_object(), json_object(), ?10,
                ?11, ?12, ?13, ?14, ?15, ?16, ?17
            )
            "#,
            params![
                format!("sqlite-access-{command_id}-{sequence:016x}"),
                record.observed_at,
                format!("sqlite-access:{}:{table_name}", record.db_path),
                format!("{table_name}.{}", record.operation),
                entity_type_for_table(table_name),
                entity_id,
                table_name,
                record.operation,
                json!({"table": table_name}).to_string(),
                json!(record.column_name.iter().collect::<Vec<_>>()).to_string(),
                turn_id,
                command_id,
                actor_key,
                source,
                command_name,
                record.db_path,
                json!({
                    "schema_version": SCHEMA_VERSION,
                    "action": record.action,
                    "database_name": record.database_name,
                    "accessor": record.accessor,
                    "recorder": "sqlite_authorizer"
                })
                .to_string(),
            ],
        )?;
        inserted += 1;
    }
    maybe_prune_sqlite_access_process_events(conn)?;
    Ok(inserted)
}

fn maybe_prune_sqlite_access_process_events(conn: &Connection) -> Result<usize> {
    let flushes = SQLITE_ACCESS_RETENTION_FLUSHES.fetch_add(1, Ordering::Relaxed) + 1;
    if flushes % SQLITE_ACCESS_RETENTION_INTERVAL_FLUSHES != 0 {
        return Ok(0);
    }
    prune_sqlite_access_process_events(conn, SQLITE_ACCESS_RETENTION_WINDOW)
}

fn prune_sqlite_access_process_events(conn: &Connection, keep_window: i64) -> Result<usize> {
    let keep_window = keep_window.max(1);
    let deleted = conn.execute(
        r#"
        DELETE FROM ctox_process_events
        WHERE case_id LIKE 'sqlite-access:%'
          AND event_seq <= COALESCE((
            SELECT MAX(event_seq) - ?1
            FROM ctox_process_events
            WHERE case_id LIKE 'sqlite-access:%'
          ), -1)
        "#,
        params![keep_window],
    )?;
    Ok(deleted)
}

pub fn activate_command_context(
    conn: &Connection,
    turn_id: &str,
    command_id: &str,
    actor_key: &str,
    source: &str,
    command_name: &str,
    argv_sha256: &str,
) -> Result<()> {
    let started_at = now_expr_value();
    conn.execute(
        r#"
        INSERT INTO ctox_process_context (
            command_id, turn_id, actor_key, source, command_name,
            argv_sha256, process_id, started_at, status
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active')
        ON CONFLICT(command_id) DO UPDATE SET
            turn_id = excluded.turn_id,
            actor_key = excluded.actor_key,
            source = excluded.source,
            command_name = excluded.command_name,
            argv_sha256 = excluded.argv_sha256,
            process_id = excluded.process_id,
            started_at = excluded.started_at,
            ended_at = NULL,
            status = 'active'
        "#,
        params![
            command_id,
            turn_id,
            actor_key,
            source,
            command_name,
            argv_sha256,
            std::process::id().to_string(),
            started_at,
        ],
    )?;
    Ok(())
}

pub fn finish_command_context(conn: &Connection, command_id: &str, status: &str) -> Result<()> {
    conn.execute(
        r#"
        UPDATE ctox_process_context
        SET ended_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
            status = ?2
        WHERE command_id = ?1
        "#,
        params![command_id, status],
    )?;
    Ok(())
}

pub fn handle_process_mining_command(root: &Path, args: &[String]) -> Result<()> {
    let db_path = crate::paths::core_db(root);
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open runtime db {}", db_path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout")?;
    attach_sqlite_access_recorder(&conn, &db_path);

    match args.first().map(String::as_str) {
        Some("ensure") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM ctox_process_trigger_registry",
                [],
                |row| row.get(0),
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "instrumented_tables": count,
                    "db_path": db_path,
                }))?
            );
            Ok(())
        }
        Some("schema") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let mut stmt = conn.prepare(
                r#"
                SELECT name, type
                FROM sqlite_master
                WHERE name = 'ctox_process_events'
                   OR name = 'ctox_process_context'
                   OR name = 'ctox_process_trigger_registry'
                   OR name = 'ctox_core_transition_proofs'
                   OR name LIKE 'ctox_pm_%'
                ORDER BY type, name
                "#,
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(json!({
                        "name": row.get::<_, String>(0)?,
                        "type": row.get::<_, String>(1)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "schema": rows}))?
            );
            Ok(())
        }
        Some("inventory") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let mut stmt = conn.prepare(
                r#"
                SELECT table_name, trigger_insert, trigger_update, trigger_delete, schema_version, installed_at
                FROM ctox_process_trigger_registry
                ORDER BY table_name
                "#,
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(json!({
                        "table_name": row.get::<_, String>(0)?,
                        "trigger_insert": row.get::<_, Option<String>>(1)?,
                        "trigger_update": row.get::<_, Option<String>>(2)?,
                        "trigger_delete": row.get::<_, Option<String>>(3)?,
                        "schema_version": row.get::<_, i64>(4)?,
                        "installed_at": row.get::<_, String>(5)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "tables": rows}))?
            );
            Ok(())
        }
        Some("events") => {
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(50)
                .clamp(1, 500);
            let mut stmt = conn.prepare(
                r#"
                SELECT event_seq, observed_at, case_id, activity, entity_type, entity_id,
                       from_state, to_state, command_id, source, command_name, table_name, operation
                FROM ctox_process_events
                ORDER BY event_seq DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "event_seq": row.get::<_, i64>(0)?,
                        "observed_at": row.get::<_, String>(1)?,
                        "case_id": row.get::<_, String>(2)?,
                        "activity": row.get::<_, String>(3)?,
                        "entity_type": row.get::<_, String>(4)?,
                        "entity_id": row.get::<_, String>(5)?,
                        "from_state": row.get::<_, Option<String>>(6)?,
                        "to_state": row.get::<_, Option<String>>(7)?,
                        "command_id": row.get::<_, Option<String>>(8)?,
                        "source": row.get::<_, Option<String>>(9)?,
                        "command_name": row.get::<_, Option<String>>(10)?,
                        "table_name": row.get::<_, String>(11)?,
                        "operation": row.get::<_, String>(12)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "events": rows}))?
            );
            Ok(())
        }
        Some("projection") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 100, 1000);
            let rows = if let Some(case_id) = find_flag_value(args, "--case-id") {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT event_seq, event_id, case_id, activity, timestamp,
                           lifecycle_transition, table_name, operation, attributes_json
                    FROM ctox_pm_case_events
                    WHERE case_id = ?1
                ORDER BY timestamp, event_seq
                LIMIT ?2
                "#,
                )?;
                let rows = stmt
                    .query_map(params![case_id, limit], |row| {
                        Ok(json!({
                            "event_seq": row.get::<_, i64>(0)?,
                            "event_id": row.get::<_, String>(1)?,
                            "case_id": row.get::<_, String>(2)?,
                            "activity": row.get::<_, String>(3)?,
                            "timestamp": row.get::<_, String>(4)?,
                            "lifecycle_transition": row.get::<_, String>(5)?,
                            "table_name": row.get::<_, String>(6)?,
                            "operation": row.get::<_, String>(7)?,
                            "attributes_json": row.get::<_, String>(8)?,
                        }))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows
            } else {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT event_seq, event_id, case_id, activity, timestamp,
                           lifecycle_transition, table_name, operation, attributes_json
                    FROM ctox_pm_case_events
                ORDER BY event_seq DESC
                LIMIT ?1
                "#,
                )?;
                let rows = stmt
                    .query_map(params![limit], |row| {
                        Ok(json!({
                            "event_seq": row.get::<_, i64>(0)?,
                            "event_id": row.get::<_, String>(1)?,
                            "case_id": row.get::<_, String>(2)?,
                            "activity": row.get::<_, String>(3)?,
                            "timestamp": row.get::<_, String>(4)?,
                            "lifecycle_transition": row.get::<_, String>(5)?,
                            "table_name": row.get::<_, String>(6)?,
                            "operation": row.get::<_, String>(7)?,
                            "attributes_json": row.get::<_, String>(8)?,
                        }))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "events": rows}))?
            );
            Ok(())
        }
        Some("cases") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 50, 500);
            let mut stmt = conn.prepare(
                r#"
                SELECT case_id,
                       COUNT(*) AS event_count,
                       COUNT(DISTINCT activity) AS activity_count,
                       MIN(timestamp) AS first_seen_at,
                       MAX(timestamp) AS last_seen_at
                FROM ctox_pm_case_events
                GROUP BY case_id
                ORDER BY last_seen_at DESC, event_count DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "case_id": row.get::<_, String>(0)?,
                        "event_count": row.get::<_, i64>(1)?,
                        "activity_count": row.get::<_, i64>(2)?,
                        "first_seen_at": row.get::<_, String>(3)?,
                        "last_seen_at": row.get::<_, String>(4)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "cases": rows}))?
            );
            Ok(())
        }
        Some("case") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let case_id = args.get(1).context("missing <case-id>")?;
            let limit = process_mining_limit(args, 200, 2000);
            let mut stmt = conn.prepare(
                r#"
                SELECT event_seq, event_id, activity, timestamp, lifecycle_transition,
                       table_name, operation, from_state, to_state, command_id, command_name
                FROM ctox_pm_case_events
                WHERE case_id = ?1
                ORDER BY timestamp, event_seq
                LIMIT ?2
                "#,
            )?;
            let rows = stmt
                .query_map(params![case_id, limit], |row| {
                    Ok(json!({
                        "event_seq": row.get::<_, i64>(0)?,
                        "event_id": row.get::<_, String>(1)?,
                        "activity": row.get::<_, String>(2)?,
                        "timestamp": row.get::<_, String>(3)?,
                        "lifecycle_transition": row.get::<_, String>(4)?,
                        "table_name": row.get::<_, String>(5)?,
                        "operation": row.get::<_, String>(6)?,
                        "from_state": row.get::<_, Option<String>>(7)?,
                        "to_state": row.get::<_, Option<String>>(8)?,
                        "command_id": row.get::<_, Option<String>>(9)?,
                        "command_name": row.get::<_, Option<String>>(10)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"ok": true, "case_id": case_id, "events": rows})
                )?
            );
            Ok(())
        }
        Some("objects") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 100, 1000);
            let mut stmt = conn.prepare(
                r#"
                SELECT object_type, qualifier, COUNT(DISTINCT object_id) AS object_count,
                       COUNT(*) AS relation_count
                FROM ctox_pm_event_objects
                GROUP BY object_type, qualifier
                ORDER BY relation_count DESC, object_type, qualifier
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "object_type": row.get::<_, String>(0)?,
                        "qualifier": row.get::<_, String>(1)?,
                        "object_count": row.get::<_, i64>(2)?,
                        "relation_count": row.get::<_, i64>(3)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "objects": rows}))?
            );
            Ok(())
        }
        Some("transitions") => {
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(100)
                .clamp(1, 1000);
            let mut stmt = conn.prepare(
                r#"
                SELECT entity_type, table_name, operation,
                       COALESCE(from_state, '<none>') AS from_state,
                       COALESCE(to_state, '<none>') AS to_state,
                       COUNT(*) AS count,
                       MIN(observed_at) AS first_seen_at,
                       MAX(observed_at) AS last_seen_at
                FROM ctox_process_events
                GROUP BY entity_type, table_name, operation, from_state, to_state
                ORDER BY count DESC, last_seen_at DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "entity_type": row.get::<_, String>(0)?,
                        "table_name": row.get::<_, String>(1)?,
                        "operation": row.get::<_, String>(2)?,
                        "from_state": row.get::<_, String>(3)?,
                        "to_state": row.get::<_, String>(4)?,
                        "count": row.get::<_, i64>(5)?,
                        "first_seen_at": row.get::<_, String>(6)?,
                        "last_seen_at": row.get::<_, String>(7)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "transitions": rows}))?
            );
            Ok(())
        }
        Some("dfg") => {
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(100)
                .clamp(1, 1000);
            let mut stmt = conn.prepare(
                r#"
                WITH ordered AS (
                    SELECT
                        case_id,
                        activity,
                        LEAD(activity) OVER (
                            PARTITION BY case_id
                            ORDER BY observed_at, event_seq
                        ) AS next_activity
                    FROM ctox_process_events
                )
                SELECT activity, next_activity, COUNT(*) AS count
                FROM ordered
                WHERE next_activity IS NOT NULL
                GROUP BY activity, next_activity
                ORDER BY count DESC, activity, next_activity
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "from_activity": row.get::<_, String>(0)?,
                        "to_activity": row.get::<_, String>(1)?,
                        "count": row.get::<_, i64>(2)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "dfg": rows}))?
            );
            Ok(())
        }
        Some("core-liveness") => {
            let report = csm::analyze_core_liveness();
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !report.ok {
                anyhow::bail!("core state machine liveness check failed");
            }
            Ok(())
        }
        Some("spawn-liveness") => {
            let core_report = core_transition_guard::analyze_core_spawn_model();
            let harness_report =
                ctox_core::harness_spawn_liveness::analyze_harness_subagent_spawn_model();
            let ok = core_report.ok && harness_report.ok;
            let report = json!({
                "ok": ok,
                "core_spawn_liveness": core_report,
                "harness_subagent_liveness": harness_report,
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !ok {
                anyhow::bail!("core spawn model liveness check failed");
            }
            Ok(())
        }
        Some("spawn-edges") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 50, 500);
            let mut stmt = conn.prepare(
                r#"
                SELECT edge_id, parent_entity_type, parent_entity_id,
                       child_entity_type, child_entity_id, spawn_kind,
                       spawn_reason, actor, checkpoint_key, budget_key,
                       max_attempts, accepted, violation_codes_json,
                       request_json, created_at, updated_at, terminal_reaped_at
                FROM ctox_core_spawn_edges
                ORDER BY updated_at DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    let request_json = row.get::<_, String>(13)?;
                    Ok(json!({
                        "edge_id": row.get::<_, String>(0)?,
                        "parent_entity_type": row.get::<_, String>(1)?,
                        "parent_entity_id": row.get::<_, String>(2)?,
                        "child_entity_type": row.get::<_, String>(3)?,
                        "child_entity_id": row.get::<_, String>(4)?,
                        "spawn_kind": row.get::<_, String>(5)?,
                        "spawn_reason": row.get::<_, String>(6)?,
                        "actor": row.get::<_, String>(7)?,
                        "checkpoint_key": row.get::<_, Option<String>>(8)?,
                        "budget_key": row.get::<_, Option<String>>(9)?,
                        "max_attempts": row.get::<_, Option<i64>>(10)?,
                        "accepted": row.get::<_, i64>(11)? != 0,
                        "violation_codes_json": row.get::<_, String>(12)?,
                        "request_summary": spawn_request_summary(&request_json),
                        "created_at": row.get::<_, String>(14)?,
                        "updated_at": row.get::<_, String>(15)?,
                        "terminal_reaped_at": row.get::<_, Option<String>>(16)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "spawn_edges": rows}))?
            );
            Ok(())
        }
        Some("explain-case") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let case_id = args.get(1).context("missing <case-id>")?;
            let limit = process_mining_limit(args, 100, 1000);
            let mut stmt = conn.prepare(
                r#"
                SELECT event_seq, event_id, activity, timestamp, lifecycle_transition,
                       table_name, operation, from_state, to_state, command_name
                FROM ctox_pm_case_events
                WHERE case_id = ?1
                ORDER BY timestamp, event_seq
                LIMIT ?2
                "#,
            )?;
            let events = stmt
                .query_map(params![case_id, limit], |row| {
                    Ok(json!({
                        "event_seq": row.get::<_, i64>(0)?,
                        "event_id": row.get::<_, String>(1)?,
                        "activity": row.get::<_, String>(2)?,
                        "timestamp": row.get::<_, String>(3)?,
                        "lifecycle_transition": row.get::<_, String>(4)?,
                        "table_name": row.get::<_, String>(5)?,
                        "operation": row.get::<_, String>(6)?,
                        "from_state": row.get::<_, Option<String>>(7)?,
                        "to_state": row.get::<_, Option<String>>(8)?,
                        "command_name": row.get::<_, Option<String>>(9)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut stmt = conn.prepare(
                r#"
                WITH ordered AS (
                    SELECT activity,
                           LEAD(activity) OVER (ORDER BY timestamp, event_seq) AS next_activity
                    FROM ctox_pm_case_events
                    WHERE case_id = ?1
                )
                SELECT activity, next_activity
                FROM ordered
                WHERE next_activity IS NOT NULL
                "#,
            )?;
            let edges = stmt
                .query_map(params![case_id], |row| {
                    Ok(json!({
                        "from_activity": row.get::<_, String>(0)?,
                        "to_activity": row.get::<_, String>(1)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "case_id": case_id,
                    "events": events,
                    "directly_follows": edges
                }))?
            );
            Ok(())
        }
        Some("deadlocks") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 50, 500);
            let mut stmt = conn.prepare(
                r#"
                WITH activities AS (
                    SELECT activity, COUNT(*) AS frequency
                    FROM ctox_pm_case_events
                    GROUP BY activity
                ),
                outgoing AS (
                    SELECT from_activity AS activity, COUNT(*) AS outgoing_count
                    FROM ctox_pm_default_dfg_edges
                    GROUP BY from_activity
                ),
                terminal AS (
                    SELECT activity, COUNT(*) AS terminal_count
                    FROM (
                        SELECT case_id, activity,
                               ROW_NUMBER() OVER (
                                   PARTITION BY case_id
                                   ORDER BY timestamp DESC, event_seq DESC
                               ) AS rn
                        FROM ctox_pm_case_events
                    )
                    WHERE rn = 1
                    GROUP BY activity
                )
                SELECT a.activity, a.frequency,
                       COALESCE(o.outgoing_count, 0) AS outgoing_count,
                       COALESCE(t.terminal_count, 0) AS terminal_count,
                       CASE
                         WHEN COALESCE(o.outgoing_count, 0) = 0
                          AND COALESCE(t.terminal_count, 0) = 0
                         THEN 'dead_end_without_terminal_evidence'
                         WHEN COALESCE(o.outgoing_count, 0) = 0
                         THEN 'terminal_only'
                         ELSE 'ok'
                       END AS classification
                FROM activities a
                LEFT JOIN outgoing o ON o.activity = a.activity
                LEFT JOIN terminal t ON t.activity = a.activity
                WHERE COALESCE(o.outgoing_count, 0) = 0
                ORDER BY classification, a.frequency DESC, a.activity
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "activity": row.get::<_, String>(0)?,
                        "frequency": row.get::<_, i64>(1)?,
                        "outgoing_count": row.get::<_, i64>(2)?,
                        "terminal_count": row.get::<_, i64>(3)?,
                        "classification": row.get::<_, String>(4)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "deadlocks": rows}))?
            );
            Ok(())
        }
        Some("mapping-rules") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 100, 1000);
            let mut stmt = conn.prepare(
                r#"
                SELECT rule_id, priority, table_pattern, entity_type_pattern,
                       operation_pattern, activity_pattern, inference_kind,
                       core_entity_type, runtime_lane, petri_transition_id,
                       evidence_policy_json, enabled
                FROM ctox_pm_core_transition_rules
                ORDER BY enabled DESC, priority, rule_id
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "rule_id": row.get::<_, String>(0)?,
                        "priority": row.get::<_, i64>(1)?,
                        "table_pattern": row.get::<_, Option<String>>(2)?,
                        "entity_type_pattern": row.get::<_, Option<String>>(3)?,
                        "operation_pattern": row.get::<_, Option<String>>(4)?,
                        "activity_pattern": row.get::<_, Option<String>>(5)?,
                        "inference_kind": row.get::<_, String>(6)?,
                        "core_entity_type": row.get::<_, String>(7)?,
                        "runtime_lane": row.get::<_, String>(8)?,
                        "petri_transition_id": row.get::<_, String>(9)?,
                        "evidence_policy_json": row.get::<_, String>(10)?,
                        "enabled": row.get::<_, i64>(11)? != 0,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "mapping_rules": rows}))?
            );
            Ok(())
        }
        Some("proofs") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 50, 500);
            let mut stmt = conn.prepare(
                r#"
                SELECT proof_id, entity_type, entity_id, lane, from_state, to_state,
                       core_event, actor, accepted, violation_codes_json,
                       request_json, report_json, created_at, updated_at
                FROM ctox_core_transition_proofs
                ORDER BY updated_at DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "proof_id": row.get::<_, String>(0)?,
                        "entity_type": row.get::<_, String>(1)?,
                        "entity_id": row.get::<_, String>(2)?,
                        "lane": row.get::<_, String>(3)?,
                        "from_state": row.get::<_, String>(4)?,
                        "to_state": row.get::<_, String>(5)?,
                        "core_event": row.get::<_, String>(6)?,
                        "actor": row.get::<_, String>(7)?,
                        "accepted": row.get::<_, i64>(8)? != 0,
                        "violation_codes_json": row.get::<_, String>(9)?,
                        "evidence_summary": proof_evidence_summary(&row.get::<_, String>(10)?),
                        "report_summary": proof_report_summary(&row.get::<_, String>(11)?),
                        "created_at": row.get::<_, String>(12)?,
                        "updated_at": row.get::<_, String>(13)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "proofs": rows}))?
            );
            Ok(())
        }
        Some("state-audit") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 50, 500);
            let mut stmt = conn.prepare(
                r#"
                SELECT audit_id, event_id, case_id, rule_id, petri_transition_id,
                       entity_type, entity_id, lane, from_state, to_state, core_event, accepted,
                       violation_codes_json, proof_id, observed_at, scanned_at
                FROM ctox_pm_core_transition_audit
                ORDER BY scanned_at DESC, observed_at DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "audit_id": row.get::<_, String>(0)?,
                        "event_id": row.get::<_, String>(1)?,
                        "case_id": row.get::<_, String>(2)?,
                        "rule_id": row.get::<_, Option<String>>(3)?,
                        "petri_transition_id": row.get::<_, Option<String>>(4)?,
                        "entity_type": row.get::<_, String>(5)?,
                        "entity_id": row.get::<_, String>(6)?,
                        "lane": row.get::<_, String>(7)?,
                        "from_state": row.get::<_, String>(8)?,
                        "to_state": row.get::<_, String>(9)?,
                        "core_event": row.get::<_, String>(10)?,
                        "accepted": row.get::<_, i64>(11)? != 0,
                        "violation_codes_json": row.get::<_, String>(12)?,
                        "proof_id": row.get::<_, Option<String>>(13)?,
                        "observed_at": row.get::<_, String>(14)?,
                        "scanned_at": row.get::<_, String>(15)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "state_audit": rows}))?
            );
            Ok(())
        }
        Some("state-scan") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 2000, 20000);
            let summary = scan_core_state_machine_violations(&conn, limit)?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
            Ok(())
        }
        Some("assert-clean") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 2000, 20000);
            let allow_rejected = args.iter().any(|arg| arg == "--allow-rejected");
            let summary = scan_core_state_machine_violations(&conn, limit)?;
            let assertion = assert_process_mining_clean_summary(&summary, allow_rejected)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "assertion": assertion,
                    "summary": summary
                }))?
            );
            Ok(())
        }
        Some("self-diagnose") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 5000, 50000);
            let report = run_process_mining_self_diagnosis(&conn, limit)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        Some("guidance") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 5000, 50000);
            let guidance = run_process_mining_guidance(&conn, limit)?;
            println!("{}", serde_json::to_string_pretty(&guidance)?);
            Ok(())
        }
        Some("coverage") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 50, 500);
            let mut stmt = conn.prepare(
                r#"
                SELECT event_id, case_id, table_name, entity_type, operation,
                       activity, mapping_kind, rule_id, petri_transition_id,
                       reason, observed_at, scanned_at, metadata_json
                FROM ctox_pm_event_transition_coverage
                ORDER BY scanned_at DESC, observed_at DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "event_id": row.get::<_, String>(0)?,
                        "case_id": row.get::<_, String>(1)?,
                        "table_name": row.get::<_, String>(2)?,
                        "entity_type": row.get::<_, String>(3)?,
                        "operation": row.get::<_, String>(4)?,
                        "activity": row.get::<_, String>(5)?,
                        "mapping_kind": row.get::<_, String>(6)?,
                        "rule_id": row.get::<_, Option<String>>(7)?,
                        "petri_transition_id": row.get::<_, Option<String>>(8)?,
                        "reason": row.get::<_, String>(9)?,
                        "observed_at": row.get::<_, String>(10)?,
                        "scanned_at": row.get::<_, String>(11)?,
                        "metadata_json": row.get::<_, String>(12)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut stmt = conn.prepare(
                r#"
                SELECT mapping_kind, COUNT(*) AS event_count
                FROM ctox_pm_event_transition_coverage
                GROUP BY mapping_kind
                ORDER BY event_count DESC, mapping_kind
                "#,
            )?;
            let counts = stmt
                .query_map([], |row| {
                    Ok(json!({
                        "mapping_kind": row.get::<_, String>(0)?,
                        "event_count": row.get::<_, i64>(1)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "coverage_counts": counts,
                    "coverage": rows
                }))?
            );
            Ok(())
        }
        Some("violations") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let limit = process_mining_limit(args, 50, 500);
            let mut stmt = conn.prepare(
                r#"
                SELECT violation_id, event_id, case_id, violation_code, severity,
                       message, detected_at, evidence_json
                FROM ctox_pm_state_violations
                ORDER BY detected_at DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(json!({
                        "violation_id": row.get::<_, String>(0)?,
                        "event_id": row.get::<_, Option<String>>(1)?,
                        "case_id": row.get::<_, String>(2)?,
                        "violation_code": row.get::<_, String>(3)?,
                        "severity": row.get::<_, String>(4)?,
                        "message": row.get::<_, String>(5)?,
                        "detected_at": row.get::<_, String>(6)?,
                        "evidence_json": row.get::<_, String>(7)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "violations": rows}))?
            );
            Ok(())
        }
        Some("prune") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let sqlite_access_window = find_flag_value(args, "--sqlite-access-window")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(SQLITE_ACCESS_RETENTION_WINDOW)
                .clamp(1, 5_000_000);
            let deleted = prune_sqlite_access_process_events(&conn, sqlite_access_window)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "deleted_sqlite_access_events": deleted,
                    "sqlite_access_window": sqlite_access_window
                }))?
            );
            Ok(())
        }
        Some("scan-violations") => {
            ensure_process_mining_schema(&conn, &db_path)?;
            let detected_at = now_expr_value();
            let inserted = conn.execute(
                r#"
                INSERT OR REPLACE INTO ctox_pm_state_violations (
                    violation_id, event_id, case_id, violation_code, severity,
                    message, detected_at, evidence_json
                )
                SELECT
                    'pmv-' || e.event_id,
                    e.event_id,
                    e.case_id,
                    'communication_sent_without_prior_review',
                    'critical',
                    'Communication reached a sent/done state without prior review evidence in the same case.',
                    ?1,
                    json_object(
                        'activity', e.activity,
                        'table_name', e.table_name,
                        'operation', e.operation,
                        'to_state', e.to_state,
                        'event_seq', e.event_seq
                    )
                FROM ctox_pm_case_events e
                WHERE (
                    lower(e.table_name) LIKE '%communicat%'
                    OR lower(e.table_name) LIKE '%mail%'
                    OR lower(e.case_id) LIKE '%communicat%'
                    OR lower(e.case_id) LIKE '%mail%'
                    OR lower(e.activity) LIKE '%send%'
                    OR lower(e.activity) LIKE '%sent%'
                )
                AND (
                    lower(COALESCE(e.to_state, '')) IN ('sent', 'done', 'completed', 'delivered')
                    OR lower(e.activity) LIKE '%send%'
                    OR lower(e.activity) LIKE '%sent%'
                )
                AND NOT EXISTS (
                    SELECT 1
                    FROM ctox_pm_case_events p
                    WHERE p.case_id = e.case_id
                      AND p.event_seq < e.event_seq
                      AND (
                        lower(p.activity) LIKE '%review%'
                        OR lower(COALESCE(p.to_state, '')) IN ('approved', 'reviewed', 'verified')
                        OR lower(COALESCE(p.command_name, '')) LIKE '%review%'
                      )
                )
                "#,
                params![detected_at],
            )?;
            let state_scan = scan_core_state_machine_violations(&conn, 20000)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "inserted_or_replaced": inserted,
                    "state_scan": state_scan,
                    "detected_at": detected_at
                }))?
            );
            Ok(())
        }
        _ => anyhow::bail!(PROCESS_MINING_USAGE),
    }
}

fn list_instrumentable_tables(conn: &Connection) -> Result<Vec<TableInfo>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT name, COALESCE(sql, '')
        FROM sqlite_master
        WHERE type = 'table'
          AND name NOT LIKE 'sqlite_%'
        ORDER BY name
        "#,
    )?;
    let tables = stmt
        .query_map([], |row| {
            Ok(TableInfo {
                name: row.get(0)?,
                sql: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|table| is_instrumentable_table(table))
        .collect();
    Ok(tables)
}

fn is_instrumentable_table(table: &TableInfo) -> bool {
    let name = table.name.as_str();
    if is_process_mining_internal_table(name) {
        return false;
    }
    if name.contains("_fts") || name.ends_with("_data") || name.ends_with("_idx") {
        return false;
    }
    !table.sql.to_ascii_uppercase().contains("VIRTUAL TABLE")
}

fn is_process_mining_internal_table(table_name: &str) -> bool {
    table_name.starts_with(PROCESS_MODEL_TABLE_PREFIX)
        || matches!(
            table_name,
            PROCESS_CONTEXT_TABLE | PROCESS_EVENTS_TABLE | PROCESS_TRIGGER_REGISTRY_TABLE
        )
}

fn install_process_mining_views(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        DROP VIEW IF EXISTS ctox_pm_case_events;
        CREATE VIEW ctox_pm_case_events AS
        SELECT
            event_seq,
            event_id,
            case_id,
            activity,
            observed_at AS timestamp,
            lifecycle_transition,
            table_name,
            operation,
            from_state,
            to_state,
            turn_id,
            command_id,
            actor_key,
            source,
            command_name,
            json_object(
                'event_seq', event_seq,
                'event_id', event_id,
                'entity_type', entity_type,
                'entity_id', entity_id,
                'table_name', table_name,
                'operation', operation,
                'from_state', from_state,
                'to_state', to_state,
                'turn_id', turn_id,
                'command_id', command_id,
                'source', source,
                'command_name', command_name,
                'primary_key', json(primary_key_json),
                'changed_columns', json(changed_columns_json),
                'metadata', json(metadata_json)
            ) AS attributes_json
        FROM ctox_process_events;

        DROP VIEW IF EXISTS ctox_pm_event_objects;
        CREATE VIEW ctox_pm_event_objects AS
        SELECT
            event_id,
            entity_type AS object_type,
            entity_id AS object_id,
            'primary_entity' AS qualifier
        FROM ctox_process_events
        UNION ALL
        SELECT
            event_id,
            'command' AS object_type,
            command_id AS object_id,
            'command_context' AS qualifier
        FROM ctox_process_events
        WHERE command_id IS NOT NULL
        UNION ALL
        SELECT
            event_id,
            'turn' AS object_type,
            turn_id AS object_id,
            'turn_context' AS qualifier
        FROM ctox_process_events
        WHERE turn_id IS NOT NULL;

        DROP VIEW IF EXISTS ctox_pm_default_dfg_edges;
        CREATE VIEW ctox_pm_default_dfg_edges AS
        WITH ordered AS (
            SELECT
                case_id,
                activity,
                LEAD(activity) OVER (
                    PARTITION BY case_id
                    ORDER BY timestamp, event_seq
                ) AS next_activity
            FROM ctox_pm_case_events
        )
        SELECT
            activity AS from_activity,
            next_activity AS to_activity,
            COUNT(*) AS frequency
        FROM ordered
        WHERE next_activity IS NOT NULL
        GROUP BY activity, next_activity;
        "#,
    )?;
    Ok(())
}

fn sqlite_access_record_from_auth_context(
    ctx: AuthContext<'_>,
    db_path: &str,
) -> Option<SqliteAccessRecord> {
    let (operation, table_name, column_name, action) = match ctx.action {
        AuthAction::Read {
            table_name,
            column_name,
        } => {
            if !sqlite_access_read_recording_enabled() {
                return None;
            }
            if is_ignored_access_table(table_name) {
                return None;
            }
            (
                "READ".to_string(),
                table_name.to_string(),
                Some(column_name.to_string()),
                "Read".to_string(),
            )
        }
        AuthAction::Attach { filename } => (
            "ATTACH".to_string(),
            "sqlite_attach".to_string(),
            Some(filename.to_string()),
            "Attach".to_string(),
        ),
        AuthAction::Detach { database_name } => (
            "DETACH".to_string(),
            "sqlite_detach".to_string(),
            Some(database_name.to_string()),
            "Detach".to_string(),
        ),
        _ => return None,
    };
    Some(SqliteAccessRecord {
        observed_at: now_expr_value(),
        db_path: db_path.to_string(),
        operation,
        table_name,
        column_name,
        action,
        database_name: ctx.database_name.map(ToString::to_string),
        accessor: ctx.accessor.map(ToString::to_string),
    })
}

fn is_ignored_access_table(table_name: &str) -> bool {
    table_name.starts_with("sqlite_") || is_process_mining_internal_table(table_name)
}

fn install_table_triggers(conn: &Connection, table: &TableInfo, db_path: &str) -> Result<()> {
    let columns = table_columns(conn, &table.name)?;
    if columns.is_empty() {
        return Ok(());
    }

    let insert_trigger = trigger_name(&table.name, "ai");
    let update_trigger = trigger_name(&table.name, "au");
    let delete_trigger = trigger_name(&table.name, "ad");

    conn.execute_batch(&format!(
        "DROP TRIGGER IF EXISTS {insert};
         DROP TRIGGER IF EXISTS {update};
         DROP TRIGGER IF EXISTS {delete};",
        insert = quote_ident(&insert_trigger),
        update = quote_ident(&update_trigger),
        delete = quote_ident(&delete_trigger),
    ))?;

    let table_ident = quote_ident(&table.name);
    let entity_type = sql_string(&entity_type_for_table(&table.name));
    let db_path_literal = sql_string(db_path);

    conn.execute_batch(&build_trigger_sql(
        &insert_trigger,
        &table_ident,
        &table.name,
        "INSERT",
        "NEW",
        None,
        Some("NEW"),
        &columns,
        &entity_type,
        &db_path_literal,
    ))?;
    conn.execute_batch(&build_trigger_sql(
        &update_trigger,
        &table_ident,
        &table.name,
        "UPDATE",
        "NEW",
        Some("OLD"),
        Some("NEW"),
        &columns,
        &entity_type,
        &db_path_literal,
    ))?;
    conn.execute_batch(&build_trigger_sql(
        &delete_trigger,
        &table_ident,
        &table.name,
        "DELETE",
        "OLD",
        Some("OLD"),
        None,
        &columns,
        &entity_type,
        &db_path_literal,
    ))?;

    conn.execute(
        r#"
        INSERT INTO ctox_process_trigger_registry (
            table_name, trigger_insert, trigger_update, trigger_delete,
            schema_version, installed_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        ON CONFLICT(table_name) DO UPDATE SET
            trigger_insert = excluded.trigger_insert,
            trigger_update = excluded.trigger_update,
            trigger_delete = excluded.trigger_delete,
            schema_version = excluded.schema_version,
            installed_at = excluded.installed_at
        "#,
        params![
            table.name,
            insert_trigger,
            update_trigger,
            delete_trigger,
            SCHEMA_VERSION,
        ],
    )?;

    Ok(())
}

fn table_triggers_current(conn: &Connection, table_name: &str) -> Result<bool> {
    let insert_trigger = trigger_name(table_name, "ai");
    let update_trigger = trigger_name(table_name, "au");
    let delete_trigger = trigger_name(table_name, "ad");
    let registry_count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_process_trigger_registry
        WHERE table_name = ?1
          AND trigger_insert = ?2
          AND trigger_update = ?3
          AND trigger_delete = ?4
          AND schema_version = ?5
        "#,
        params![
            table_name,
            insert_trigger,
            update_trigger,
            delete_trigger,
            SCHEMA_VERSION,
        ],
        |row| row.get(0),
    )?;
    if registry_count == 0 {
        return Ok(false);
    }

    let trigger_count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM sqlite_master
        WHERE type = 'trigger'
          AND name IN (?1, ?2, ?3)
        "#,
        params![insert_trigger, update_trigger, delete_trigger],
        |row| row.get(0),
    )?;
    Ok(trigger_count == 3)
}

#[allow(clippy::too_many_arguments)]
fn build_trigger_sql(
    trigger_name: &str,
    table_ident: &str,
    table_name: &str,
    operation: &str,
    pk_alias: &str,
    before_alias: Option<&str>,
    after_alias: Option<&str>,
    columns: &[ColumnInfo],
    entity_type: &str,
    db_path_literal: &str,
) -> String {
    let table_literal = sql_string(table_name);
    let operation_literal = sql_string(operation);
    let activity_literal = sql_string(&format!("{table_name}.{operation}"));
    let pk_json = primary_key_json_expr(columns, pk_alias);
    let entity_id = format!("({pk_json})");
    let case_id = format!("({table_literal} || ':' || {pk_json})");
    let before_json = row_json_expr(columns, before_alias, table_name);
    let after_json = row_json_expr(columns, after_alias, table_name);
    let changed_columns = changed_columns_expr(columns, before_alias, after_alias);
    let from_state = state_expr(columns, before_alias);
    let to_state = state_expr(columns, after_alias);

    format!(
        r#"
        CREATE TRIGGER {trigger_name}
        AFTER {operation} ON {table_ident}
        BEGIN
            INSERT INTO ctox_process_events (
                event_id, observed_at, case_id, activity, lifecycle_transition,
                entity_type, entity_id, table_name, operation,
                from_state, to_state, primary_key_json, row_before_json,
                row_after_json, changed_columns_json, turn_id, command_id,
                actor_key, source, command_name, db_path, metadata_json
            )
            VALUES (
                lower(hex(randomblob(16))),
                strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                {case_id},
                {activity_literal},
                'complete',
                {entity_type},
                {entity_id},
                {table_literal},
                {operation_literal},
                {from_state},
                {to_state},
                {pk_json},
                {before_json},
                {after_json},
                {changed_columns},
                (SELECT turn_id FROM ctox_process_context WHERE status = 'active' ORDER BY started_at DESC LIMIT 1),
                (SELECT command_id FROM ctox_process_context WHERE status = 'active' ORDER BY started_at DESC LIMIT 1),
                (SELECT actor_key FROM ctox_process_context WHERE status = 'active' ORDER BY started_at DESC LIMIT 1),
                (SELECT source FROM ctox_process_context WHERE status = 'active' ORDER BY started_at DESC LIMIT 1),
                (SELECT command_name FROM ctox_process_context WHERE status = 'active' ORDER BY started_at DESC LIMIT 1),
                {db_path_literal},
                json_object('schema_version', {SCHEMA_VERSION})
            );
        END;
        "#,
        trigger_name = quote_ident(trigger_name),
        operation = operation,
        table_ident = table_ident,
    )
}

fn table_columns(conn: &Connection, table_name: &str) -> Result<Vec<ColumnInfo>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", quote_ident(table_name)))?;
    let columns = stmt
        .query_map([], |row| {
            Ok(ColumnInfo {
                name: row.get(1)?,
                decl_type: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                pk_rank: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns)
}

fn primary_key_json_expr(columns: &[ColumnInfo], alias: &str) -> String {
    let pk_columns = columns
        .iter()
        .filter(|column| column.pk_rank > 0)
        .collect::<Vec<_>>();
    if pk_columns.is_empty() {
        return format!("json_object('rowid', {alias}.rowid)");
    }
    json_object_expr(pk_columns.into_iter(), Some(alias))
}

fn row_json_expr(columns: &[ColumnInfo], alias: Option<&str>, table_name: &str) -> String {
    let Some(alias) = alias else {
        return "json_object()".to_string();
    };
    if is_sensitive_table(table_name) {
        let pk_columns = columns
            .iter()
            .filter(|column| column.pk_rank > 0)
            .collect::<Vec<_>>();
        let pk_json = if pk_columns.is_empty() {
            format!("json_object('rowid', {alias}.rowid)")
        } else {
            json_object_expr(pk_columns.into_iter(), Some(alias))
        };
        return format!("json_object('_redacted', 1, '_pk', {pk_json})");
    }
    json_object_expr(columns.iter(), Some(alias))
}

fn changed_columns_expr(
    columns: &[ColumnInfo],
    before_alias: Option<&str>,
    after_alias: Option<&str>,
) -> String {
    match (before_alias, after_alias) {
        (Some(before), Some(after)) => {
            let parts = columns
                .iter()
                .map(|column| {
                    format!(
                        "CASE WHEN {before}.{col} IS NOT {after}.{col} THEN {name} END",
                        before = before,
                        after = after,
                        col = quote_ident(&column.name),
                        name = sql_string(&column.name),
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("json_array({parts})")
        }
        _ => {
            let names = columns
                .iter()
                .map(|column| sql_string(&column.name))
                .collect::<Vec<_>>()
                .join(", ");
            format!("json_array({names})")
        }
    }
}

fn state_expr(columns: &[ColumnInfo], alias: Option<&str>) -> String {
    let Some(alias) = alias else {
        return "NULL".to_string();
    };
    for candidate in [
        "state",
        "status",
        "route_status",
        "mission_status",
        "enabled",
    ] {
        if columns.iter().any(|column| column.name == candidate) {
            return format!("CAST({alias}.{} AS TEXT)", quote_ident(candidate));
        }
    }
    "'row_present'".to_string()
}

fn json_object_expr<'a>(
    columns: impl Iterator<Item = &'a ColumnInfo>,
    alias: Option<&str>,
) -> String {
    let parts = columns
        .flat_map(|column| {
            let value = if let Some(alias) = alias {
                json_safe_column_expr(alias, column)
            } else {
                "NULL".to_string()
            };
            [sql_string(&column.name), value]
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("json_object({parts})")
}

fn json_safe_column_expr(alias: &str, column: &ColumnInfo) -> String {
    let column_ref = format!("{alias}.{}", quote_ident(&column.name));
    if column.decl_type.to_ascii_uppercase().contains("BLOB") {
        return format!(
            "CASE WHEN {column_ref} IS NULL THEN NULL ELSE '[blob:' || length({column_ref}) || ':' || lower(hex({column_ref})) || ']' END"
        );
    }
    format!(
        "CASE WHEN typeof({column_ref}) = 'blob' THEN '[blob:' || length({column_ref}) || ':' || lower(hex({column_ref})) || ']' ELSE {column_ref} END"
    )
}

fn entity_type_for_table(table_name: &str) -> String {
    if table_name == "communication_founder_reply_reviews" {
        "founder_review".to_string()
    } else if table_name.starts_with("communication_") {
        "communication".to_string()
    } else if table_name.starts_with("scheduled_") {
        "schedule".to_string()
    } else if table_name.starts_with("ticket_") {
        "ticket".to_string()
    } else if table_name.starts_with("knowledge_") {
        "knowledge".to_string()
    } else if table_name.starts_with("mission_") {
        "mission".to_string()
    } else if table_name.starts_with("planned_") {
        "plan".to_string()
    } else if table_name.starts_with("governance_") {
        "governance".to_string()
    } else if table_name.starts_with("ctox_secret") {
        "secret".to_string()
    } else if table_name.starts_with("ctox_") || table_name.starts_with("runtime_") {
        "runtime_state".to_string()
    } else if matches!(
        table_name,
        "messages" | "summaries" | "summary_messages" | "summary_edges" | "context_items"
    ) {
        "context".to_string()
    } else {
        "sqlite_table".to_string()
    }
}

fn is_sensitive_table(table_name: &str) -> bool {
    table_name.contains("secret")
        || table_name.contains("credential")
        || table_name.contains("token")
        || table_name == "runtime_env_kv"
}

fn trigger_name(table_name: &str, suffix: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(table_name.as_bytes());
    hasher.update([b':']);
    hasher.update(suffix.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("ctox_pm_{}_{}", &digest[..16], suffix)
}

fn quote_ident(raw: &str) -> String {
    format!("\"{}\"", raw.replace('"', "\"\""))
}

fn sql_string(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', "''"))
}

fn now_expr_value() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
}

fn process_mining_limit(args: &[String], default: i64, max: i64) -> i64 {
    find_flag_value(args, "--limit")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(default)
        .clamp(1, max)
}

fn proof_evidence_summary(request_json: &str) -> Value {
    let Ok(value) = serde_json::from_str::<Value>(request_json) else {
        return json!({
            "parse_error": true,
            "expected_artifact_ref_count": 0,
            "delivered_artifact_ref_count": 0,
        });
    };
    let evidence = value.get("evidence").unwrap_or(&Value::Null);
    let expected = evidence
        .get("expected_artifact_refs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let delivered = evidence
        .get("delivered_artifact_refs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    json!({
        "review_audit_key": evidence.get("review_audit_key").and_then(Value::as_str),
        "approved_body_sha256": evidence.get("approved_body_sha256").and_then(Value::as_str),
        "outgoing_body_sha256": evidence.get("outgoing_body_sha256").and_then(Value::as_str),
        "approved_recipient_set_sha256": evidence.get("approved_recipient_set_sha256").and_then(Value::as_str),
        "outgoing_recipient_set_sha256": evidence.get("outgoing_recipient_set_sha256").and_then(Value::as_str),
        "verification_id": evidence.get("verification_id").and_then(Value::as_str),
        "schedule_task_id": evidence.get("schedule_task_id").and_then(Value::as_str),
        "replacement_schedule_task_id": evidence.get("replacement_schedule_task_id").and_then(Value::as_str),
        "escalation_id": evidence.get("escalation_id").and_then(Value::as_str),
        "knowledge_entry_id": evidence.get("knowledge_entry_id").and_then(Value::as_str),
        "incident_id": evidence.get("incident_id").and_then(Value::as_str),
        "canonical_hot_path": evidence.get("canonical_hot_path").cloned().unwrap_or_else(|| json!([])),
        "expected_artifact_ref_count": expected.len(),
        "delivered_artifact_ref_count": delivered.len(),
        "expected_artifact_refs": expected,
        "delivered_artifact_refs": delivered,
    })
}

fn proof_report_summary(report_json: &str) -> Value {
    let Ok(value) = serde_json::from_str::<Value>(report_json) else {
        return json!({"parse_error": true});
    };
    let violations = value
        .get("violations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let violation_codes = violations
        .iter()
        .filter_map(|violation| violation.get("code").and_then(Value::as_str))
        .collect::<Vec<_>>();
    json!({
        "accepted": value.get("accepted").and_then(Value::as_bool),
        "violation_count": violations.len(),
        "violation_codes": violation_codes,
    })
}

fn spawn_request_summary(request_json: &str) -> Value {
    let Ok(value) = serde_json::from_str::<Value>(request_json) else {
        return json!({"parse_error": true});
    };
    json!({
        "parent": {
            "entity_type": value.get("parent_entity_type").and_then(Value::as_str),
            "entity_id": value.get("parent_entity_id").and_then(Value::as_str),
        },
        "child": {
            "entity_type": value.get("child_entity_type").and_then(Value::as_str),
            "entity_id": value.get("child_entity_id").and_then(Value::as_str),
        },
        "spawn_kind": value.get("spawn_kind").and_then(Value::as_str),
        "checkpoint_key": value.get("checkpoint_key").and_then(Value::as_str),
        "budget_key": value.get("budget_key").and_then(Value::as_str),
        "max_attempts": value.get("max_attempts").and_then(Value::as_i64),
    })
}

fn json_usize(value: &Value, key: &str) -> usize {
    value.get(key).and_then(Value::as_u64).unwrap_or_default() as usize
}

fn assert_process_mining_clean_summary(summary: &Value, allow_rejected: bool) -> Result<Value> {
    let unmapped = json_usize(summary, "unmapped");
    let rule_matched_without_core_transition =
        json_usize(summary, "rule_matched_without_core_transition");
    let rejected = json_usize(summary, "rejected");
    let violation_count = json_usize(summary, "violation_count");
    let mut failures = Vec::new();

    if unmapped > 0 {
        failures.push(json!({
            "code": "unmapped_events",
            "count": unmapped,
            "message": "SQLite events exist without an enabled process-mining mapping rule"
        }));
    }
    if rule_matched_without_core_transition > 0 {
        failures.push(json!({
            "code": "rule_without_core_transition",
            "count": rule_matched_without_core_transition,
            "message": "A mapping rule matched but could not produce a deterministic core-state transition"
        }));
    }
    if !allow_rejected && rejected > 0 {
        failures.push(json!({
            "code": "rejected_core_transitions",
            "count": rejected,
            "violation_count": violation_count,
            "message": "Core-state transition proofs rejected harness behavior"
        }));
    }

    if !failures.is_empty() {
        let mut hints = Vec::new();
        for failure in &failures {
            match failure.get("code").and_then(Value::as_str) {
                Some("unmapped_events") => hints.push(
                    "Es gibt SQLite-Aenderungen ohne bekannte Prozess-Mapping-Regel. Behandle das als Harness-Arbeit: fuehre `ctox process-mining guidance --limit 50` aus und erweitere zuerst die Mapping-Regeln, bevor du fachlich weiterarbeitest.",
                ),
                Some("rule_without_core_transition") => hints.push(
                    "Eine Prozessregel konnte keinen eindeutigen Core-Zustandswechsel bilden. Fuehre `ctox process-mining guidance --limit 50` aus und korrigiere zuerst die betroffene State-Machine-Abbildung.",
                ),
                Some("rejected_core_transitions") => hints.push(
                    "Mindestens ein Core-Zustandswechsel wurde abgelehnt. Stoppe den betroffenen Pfad, fuehre `ctox process-mining guidance --limit 50` aus und arbeite die dort genannten Zwischenschritte nach.",
                ),
                _ => hints.push(
                    "Die Prozess-Mining-Pruefung ist nicht sauber. Fuehre `ctox process-mining guidance --limit 50` aus und behebe die dort genannten Punkte, bevor du den Pfad fortsetzt.",
                ),
            }
        }
        hints.sort_unstable();
        hints.dedup();
        anyhow::bail!(
            "Die Harness-Pruefung hat den naechsten Schritt gestoppt. {}",
            hints.join(" ")
        );
    }

    Ok(json!({
        "clean": true,
        "allow_rejected": allow_rejected,
        "checked": {
            "unmapped": unmapped,
            "rule_matched_without_core_transition": rule_matched_without_core_transition,
            "rejected": rejected,
            "violation_count": violation_count
        }
    }))
}

fn run_process_mining_self_diagnosis(conn: &Connection, limit: i64) -> Result<Value> {
    let scanned_at = now_expr_value();
    let state_summary = scan_core_state_machine_violations(conn, limit)?;
    let liveness = csm::analyze_core_liveness();
    let mut subsystems = Vec::new();

    let unmapped = json_usize(&state_summary, "unmapped");
    let rule_without = json_usize(&state_summary, "rule_matched_without_core_transition");
    let rejected = json_usize(&state_summary, "rejected");
    let violation_count = json_usize(&state_summary, "violation_count");
    push_subsystem(
        &mut subsystems,
        "process_mining_coverage",
        if unmapped == 0 && rule_without == 0 {
            "ok"
        } else {
            "critical"
        },
        "SQLite mutations must be either explicit telemetry or deterministic core transitions.",
        json!({
            "scanned_events": json_usize(&state_summary, "scanned_events"),
            "mapped_telemetry": json_usize(&state_summary, "mapped_telemetry"),
            "core_transitions": json_usize(&state_summary, "inferred_transitions"),
            "accepted": json_usize(&state_summary, "accepted"),
            "rejected": rejected,
            "unmapped": unmapped,
            "rule_matched_without_core_transition": rule_without,
            "violation_count": violation_count,
        }),
        findings_for_mapping(unmapped, rule_without),
    );

    push_subsystem(
        &mut subsystems,
        "core_liveness",
        if liveness.ok { "ok" } else { "critical" },
        "Every modeled harness entity must have reachable states and a terminal path.",
        serde_json::to_value(&liveness)?,
        liveness_findings(&liveness),
    );
    let spawn_liveness = core_transition_guard::analyze_core_spawn_model();
    push_subsystem(
        &mut subsystems,
        "core_spawn_liveness",
        if spawn_liveness.ok { "ok" } else { "critical" },
        "Every internal task spawn must be registered, budgeted, parent-linked, and handled by a non-spawning intervention effect.",
        serde_json::to_value(&spawn_liveness)?,
        spawn_liveness
            .violations
            .iter()
            .map(|violation| json!({ "finding": violation }))
            .collect(),
    );
    let harness_spawn_liveness =
        ctox_core::harness_spawn_liveness::analyze_harness_subagent_spawn_model();
    push_subsystem(
        &mut subsystems,
        "harness_subagent_liveness",
        if harness_spawn_liveness.ok {
            "ok"
        } else {
            "critical"
        },
        "Every harness subagent path must be depth-bounded, count-bounded, leaf-only, and unable to spawn recursive worker trees.",
        serde_json::to_value(&harness_spawn_liveness)?,
        harness_spawn_liveness
            .violations
            .iter()
            .map(|violation| json!({ "finding": violation }))
            .collect(),
    );

    subsystems.push(diagnose_knowledge(conn)?);
    subsystems.push(diagnose_lcm(conn)?);
    subsystems.push(diagnose_queue(conn)?);
    let current_scan_at = state_summary
        .get("scanned_at")
        .and_then(Value::as_str)
        .map(str::to_string);
    subsystems.push(diagnose_founder_review(conn, current_scan_at.as_deref())?);
    subsystems.push(diagnose_tickets(conn)?);
    subsystems.push(diagnose_schedules(conn)?);
    subsystems.push(diagnose_harness_findings(conn)?);

    let critical_count = subsystems
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) == Some("critical"))
        .count();
    let warning_count = subsystems
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) == Some("warning"))
        .count();

    Ok(json!({
        "ok": critical_count == 0,
        "scanned_at": scanned_at,
        "event_limit": limit,
        "critical_count": critical_count,
        "warning_count": warning_count,
        "state_summary": state_summary,
        "subsystems": subsystems,
    }))
}

fn run_process_mining_guidance(conn: &Connection, limit: i64) -> Result<Value> {
    let diagnosis = run_process_mining_self_diagnosis(conn, limit)?;
    let mut next_actions = Vec::<String>::new();
    let mut attention = Vec::<Value>::new();

    if let Some(subsystems) = diagnosis.get("subsystems").and_then(Value::as_array) {
        for subsystem in subsystems {
            let status = subsystem
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if status == "ok" {
                continue;
            }
            let name = subsystem
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let summary = subsystem
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("Harness subsystem needs attention.");
            attention.push(json!({
                "subsystem": name,
                "status": status,
                "summary": summary,
            }));
            if let Some(findings) = subsystem.get("findings").and_then(Value::as_array) {
                for finding in findings {
                    let code = finding
                        .get("code")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    next_actions.push(guidance_action_for_finding(name, code).to_string());
                }
            }
        }
    }

    if next_actions.is_empty() {
        next_actions.push(
            "Der Harness sieht aktuell sauber aus. Arbeite mit normaler Priorisierung weiter und pruefe bei Blockaden erneut `ctox process-mining guidance --limit 50`."
                .to_string(),
        );
    }
    next_actions.sort();
    next_actions.dedup();

    Ok(json!({
        "ok": diagnosis.get("ok").and_then(Value::as_bool).unwrap_or(false),
        "status": if attention.iter().any(|item| item.get("status").and_then(Value::as_str) == Some("critical")) {
            "critical"
        } else if attention.is_empty() {
            "ok"
        } else {
            "warning"
        },
        "summary": "Natuerliche Harness-Diagnose fuer den Agenten. Keine Rohdaten, keine internen Proofs, keine Source-Code-Hinweise.",
        "next_actions": next_actions,
        "attention": attention,
        "safe_commands": [
            "ctox process-mining guidance --limit 50",
            "ctox process-mining self-diagnose --limit 20000",
            "ctox process-mining coverage --limit 20",
            "ctox process-mining violations --limit 20"
        ]
    }))
}

fn guidance_action_for_finding(subsystem: &str, code: &str) -> &'static str {
    match code {
        "unmapped_sqlite_events" => {
            "Stoppe fachliche Arbeit kurz und erweitere die Prozess-Mapping-Regeln fuer die unbekannten SQLite-Aenderungen. Danach `ctox process-mining self-diagnose --limit 20000` erneut ausfuehren."
        }
        "non_deterministic_mapping_rule" => {
            "Korrigiere die betroffene Mapping-Regel so, dass sie eindeutig einen Core-Zustandswechsel oder explizite Telemetrie erzeugt."
        }
        "core_graph_liveness_gap" => {
            "Repariere den Core-State-Graphen: jeder erreichbare Zustand braucht einen erlaubten Pfad zu einem Terminalzustand."
        }
        "no_knowledge_entries" => {
            "Speichere die Erkenntnis korrekt: Fakten/Entscheidungen als Ticket-Kontext, wiederverwendbare Prozeduren als Skillbook/Runbook/Runbook-Item, jeweils mit Evidenz verknuepft."
        }
        "knowledge_not_loaded" => {
            "Lade vorhandenen Ticket-Kontext oder passende Skillbook/Runbook-Eintraege aktiv in die naechste Arbeitseinheit, bevor du weiter entscheidest."
        }
        "no_operational_knowledge_hierarchy" => {
            "Pruefe, ob aktuelle Erkenntnisse nur Fakten sind oder als wiederverwendbare Prozedur in Skillbook/Runbook/Runbook-Item gehoeren. Ticket-Kontext allein ist kein Skill."
        }
        "no_recent_knowledge_activity" => {
            "Pruefe nach Abschluss der aktuellen Arbeit, ob Ticket-Kontext, Continuity oder eine echte Runbook-Ergaenzung dauerhaft gespeichert werden muss."
        }
        "missing_lcm_continuity" => {
            "Repariere Continuity: aktueller Kontext muss als LCM-Dokument und Commit dauerhaft vorhanden sein."
        }
        "broken_lcm_head" => {
            "Repariere den LCM-Head, bevor weitere Langzeit-Arbeit fortgesetzt wird."
        }
        "compaction_without_lcm_change" => {
            "Pruefe Compaction: nach einer Verdichtung muss ein LCM-Dokument oder Commit die Kontextveraenderung belegen."
        }
        "stuck_queue_items" => {
            "Bearbeite zuerst stale leased/running Queue-Eintraege: entscheiden, ob fortsetzen, requeue, blocken oder superseden."
        }
        "negative_queue_durations" => {
            "Korrigiere Queue-Zeitstempel, damit Latenzen auswertbar bleiben."
        }
        "founder_review_gate_rejections" => {
            "Stoppe Founder-Kommunikation. Baue Kontext, Entwurf, echtes Review und matching Empfaenger/Text-Freigabe neu auf, bevor gesendet wird."
        }
        "historical_founder_review_gate_rejections" => {
            "Behandle Founder-Kommunikation weiterhin als P0: bei jeder neuen Mail Review, Empfaengerabgleich und Versand-Audit pruefen."
        }
        "self_work_without_canonical_tickets" => {
            "Ueberfuehre wiederkehrende Self-Work in kanonische Tickets, damit Arbeit priorisierbar und forensisch vergleichbar wird."
        }
        "large_active_self_work_backlog" => {
            "Reduziere den aktiven Self-Work-Backlog: P0/P1 priorisieren, Ueberholtes superseden, Blockiertes explizit markieren."
        }
        "scheduled_tasks_without_runs" => {
            "Pruefe Scheduler-Ausfuehrung: faellige Zusagen brauchen emit/run-Evidenz vor der Deadline."
        }
        "overdue_scheduled_tasks" => {
            "Bearbeite ueberfaellige Scheduled Tasks sofort und sende keine neue Zusage ohne neue Absicherung."
        }
        _ => match subsystem {
            "process_mining_coverage" => {
                "Pruefe Process-Mining-Coverage und entscheide fuer neue SQLite-Aenderungen explizit: Telemetrie oder Core-Zustandswechsel."
            }
            "founder_communication_review" => {
                "Pruefe Founder-Kommunikation vor jeder Sendung auf Kontext, Review, Empfaengerliste und unveraenderten finalen Text."
            }
            "schedules_and_commitments" => {
                "Pruefe Zusagen und Deadlines: jede Zusage braucht einen rechtzeitig angesetzten, beobachtbaren Trigger."
            }
            _ => "Fuehre `ctox process-mining self-diagnose --limit 20000` aus, behebe das genannte Subsystem und pruefe danach erneut.",
        },
    }
}

fn push_subsystem(
    out: &mut Vec<Value>,
    name: &str,
    status: &str,
    summary: &str,
    metrics: Value,
    findings: Vec<Value>,
) {
    out.push(json!({
        "name": name,
        "status": status,
        "summary": summary,
        "metrics": metrics,
        "findings": findings,
    }));
}

fn diagnose_harness_findings(conn: &Connection) -> Result<Value> {
    // Surface confirmed harness-mining findings as a self-diagnose subsystem
    // so callers like queue-cleanup, incident-response, and reliability-ops
    // see the same signal whether they read self-diagnose or harness-mining
    // directly. We do NOT write here — findings flow only from the
    // audit-tick. Read-only.
    let table_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='ctox_hm_findings'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if table_exists == 0 {
        // Audit-tick has not run yet on this deployment — nothing to surface.
        return Ok(subsystem_json(
            "harness_findings",
            "ok",
            "Harness-mining audit findings (none yet — audit-tick has not run).",
            json!({"confirmed": 0, "acknowledged": 0, "mitigated": 0, "verified": 0}),
            Vec::new(),
        ));
    }

    let confirmed =
        crate::service::harness_mining::findings::list(conn, Some("confirmed"), None, 100)?;
    let acknowledged =
        crate::service::harness_mining::findings::list(conn, Some("acknowledged"), None, 100)?;
    let mitigated_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ctox_hm_findings WHERE status = 'mitigated'",
        [],
        |row| row.get(0),
    )?;
    let verified_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ctox_hm_findings WHERE status = 'verified'",
        [],
        |row| row.get(0),
    )?;

    let critical_confirmed = confirmed
        .iter()
        .filter(|f| f.get("severity").and_then(Value::as_str) == Some("critical"))
        .count();
    let warning_confirmed = confirmed.len() - critical_confirmed;

    let mut findings: Vec<Value> = Vec::new();
    for f in confirmed.iter().take(10) {
        let kind = f.get("kind").and_then(Value::as_str).unwrap_or("?");
        let severity = f.get("severity").and_then(Value::as_str).unwrap_or("?");
        let entity_type = f.get("entity_type").and_then(Value::as_str).unwrap_or("");
        let lane = f.get("lane").and_then(Value::as_str).unwrap_or("");
        let confirmed_at = f.get("confirmed_at").and_then(Value::as_str).unwrap_or("");
        findings.push(json!({
            "code": format!("harness_finding:{kind}"),
            "severity": severity,
            "message": format!(
                "{kind} confirmed at {confirmed_at} ({entity_type}{}{}) — review via `ctox harness-mining findings --status confirmed` and acknowledge / mitigate before continuing protected work",
                if lane.is_empty() { "" } else { ", " },
                lane
            ),
            "finding_id": f.get("finding_id"),
        }));
    }

    let status = if critical_confirmed > 0 {
        "critical"
    } else if !confirmed.is_empty() || !acknowledged.is_empty() {
        "warning"
    } else {
        "ok"
    };

    Ok(subsystem_json(
        "harness_findings",
        status,
        "Confirmed harness-mining findings drive recovery decisions; the agent must transition them through the lifecycle (acknowledge → mitigate → verify) instead of letting them accumulate.",
        json!({
            "confirmed": confirmed.len(),
            "confirmed_critical": critical_confirmed,
            "confirmed_warning": warning_confirmed,
            "acknowledged": acknowledged.len(),
            "mitigated": mitigated_count,
            "verified": verified_count,
        }),
        findings,
    ))
}

fn subsystem_json(
    name: &str,
    status: &str,
    summary: &str,
    metrics: Value,
    findings: Vec<Value>,
) -> Value {
    json!({
        "name": name,
        "status": status,
        "summary": summary,
        "metrics": metrics,
        "findings": findings,
    })
}

fn findings_for_mapping(unmapped: usize, rule_without: usize) -> Vec<Value> {
    let mut findings = Vec::new();
    if unmapped > 0 {
        findings.push(json!({
            "severity": "critical",
            "code": "unmapped_sqlite_events",
            "message": "At least one SQLite table mutation has no explicit process-mining mapping rule."
        }));
    }
    if rule_without > 0 {
        findings.push(json!({
            "severity": "critical",
            "code": "non_deterministic_mapping_rule",
            "message": "A mapping rule matched but could not produce a deterministic core-state transition."
        }));
    }
    findings
}

fn liveness_findings(report: &csm::CoreLivenessReport) -> Vec<Value> {
    let mut findings = Vec::new();
    for entity in &report.entities {
        if !entity.unreachable_states.is_empty()
            || !entity.nonterminal_dead_end_states.is_empty()
            || !entity.states_without_terminal_path.is_empty()
        {
            findings.push(json!({
                "severity": "critical",
                "code": "core_graph_liveness_gap",
                "entity_type": format!("{:?}", entity.entity_type),
                "unreachable_states": entity.unreachable_states,
                "nonterminal_dead_end_states": entity.nonterminal_dead_end_states,
                "states_without_terminal_path": entity.states_without_terminal_path,
            }));
        }
    }
    findings
}

fn diagnose_knowledge(conn: &Connection) -> Result<Value> {
    let entries = table_count(conn, "ticket_knowledge_entries")?;
    let loads = table_count(conn, "ticket_knowledge_loads")?;
    let main_skills = table_count(conn, "knowledge_main_skills")?;
    let skillbooks = table_count(conn, "knowledge_skillbooks")?;
    let runbooks = table_count(conn, "knowledge_runbooks")?;
    let runbook_items = table_count(conn, "knowledge_runbook_items")?;
    let recent_events = recent_table_event_count(conn, "%knowledge%")?;
    let mut findings = Vec::new();
    if entries == 0 && main_skills == 0 && skillbooks == 0 && runbooks == 0 && runbook_items == 0 {
        findings.push(json!({
            "severity": "critical",
            "code": "no_knowledge_entries",
            "message": "The SQLite knowledge subsystem has no ticket fact/context entries and no Skillbook/Runbook hierarchy records."
        }));
    }
    if entries > 0 && loads == 0 {
        findings.push(json!({
            "severity": "warning",
            "code": "knowledge_not_loaded",
            "message": "Ticket fact/context entries exist but no ticket knowledge load is recorded."
        }));
    }
    if entries > 0 && main_skills == 0 && skillbooks == 0 && runbooks == 0 && runbook_items == 0 {
        findings.push(json!({
            "severity": "warning",
            "code": "no_operational_knowledge_hierarchy",
            "message": "Ticket fact/context entries exist, but no source-skill, Skillbook, Runbook, or Runbook-Item record exists. Do not treat ticket facts as learned operational procedure."
        }));
    }
    if recent_events == 0 {
        findings.push(json!({
            "severity": "warning",
            "code": "no_recent_knowledge_activity",
            "message": "No recent knowledge-table mutation was observed in the process log."
        }));
    }
    let status = status_from_findings(&findings);
    Ok(subsystem_json(
        "knowledge",
        status,
        "Facts and decisions must accumulate as durable ticket/context records; reusable procedure must be promoted into Skillbook/Runbook records and loaded back into work.",
        json!({
            "ticket_knowledge_entries": entries,
            "ticket_knowledge_loads": loads,
            "knowledge_main_skills": main_skills,
            "knowledge_skillbooks": skillbooks,
            "knowledge_runbooks": runbooks,
            "knowledge_runbook_items": runbook_items,
            "recent_process_events": recent_events,
        }),
        findings,
    ))
}

fn diagnose_lcm(conn: &Connection) -> Result<Value> {
    let documents = table_count(conn, "continuity_documents")?;
    let commits = table_count(conn, "continuity_commits")?;
    let verification_runs = table_count(conn, "verification_runs")?;
    let broken_heads = if table_exists(conn, "continuity_documents")?
        && table_exists(conn, "continuity_commits")?
    {
        conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM continuity_documents d
            LEFT JOIN continuity_commits c ON c.commit_id = d.head_commit_id
            WHERE c.commit_id IS NULL
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    let compact_without_lcm_change = if table_exists(conn, PROCESS_EVENTS_TABLE)? {
        conn.query_row(
            r#"
            SELECT COUNT(DISTINCT e.command_id)
            FROM ctox_process_events e
            WHERE e.command_id IS NOT NULL
              AND lower(COALESCE(e.command_name, '')) LIKE '%compact%'
              AND NOT EXISTS (
                  SELECT 1
                  FROM ctox_process_events c
                  WHERE c.command_id = e.command_id
                    AND c.table_name IN ('continuity_documents', 'continuity_commits')
              )
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    let mut findings = Vec::new();
    if documents == 0 || commits == 0 {
        findings.push(json!({
            "severity": "critical",
            "code": "missing_lcm_continuity",
            "message": "LCM continuity documents or commits are missing."
        }));
    }
    if broken_heads > 0 {
        findings.push(json!({
            "severity": "critical",
            "code": "broken_lcm_head",
            "message": "At least one continuity document points to a missing head commit."
        }));
    }
    if compact_without_lcm_change > 0 {
        findings.push(json!({
            "severity": "warning",
            "code": "compaction_without_lcm_change",
            "message": "A compact command was observed without continuity document/commit mutation."
        }));
    }
    let status = status_from_findings(&findings);
    Ok(subsystem_json(
        "lcm_continuity",
        status,
        "Compaction and continuity updates must leave durable LCM document/commit evidence.",
        json!({
            "continuity_documents": documents,
            "continuity_commits": commits,
            "verification_runs": verification_runs,
            "broken_document_heads": broken_heads,
            "compact_commands_without_lcm_change": compact_without_lcm_change,
        }),
        findings,
    ))
}

fn diagnose_queue(conn: &Connection) -> Result<Value> {
    let routing_rows = table_count(conn, "communication_routing_state")?;
    let status_counts = grouped_counts(conn, "communication_routing_state", "route_status")?;
    let (completed_count, avg_seconds, fastest, slowest) =
        routing_duration_stats(conn, "communication_routing_state", "message_key")?;
    let negative_duration_count =
        negative_routing_duration_count(conn, "communication_routing_state")?;
    let stale_lease_cutoff = (chrono::Utc::now() - chrono::Duration::minutes(15))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let active_leased = if table_exists(conn, "communication_routing_state")? {
        conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM communication_routing_state
            WHERE route_status IN ('leased', 'running')
              AND leased_at IS NOT NULL
              AND (acked_at IS NULL OR acked_at = '')
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    let stuck_leased = if table_exists(conn, "communication_routing_state")? {
        conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM communication_routing_state
            WHERE route_status IN ('leased', 'running')
              AND leased_at IS NOT NULL
              AND leased_at < ?1
              AND (acked_at IS NULL OR acked_at = '')
            "#,
            params![stale_lease_cutoff],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    let mut findings = Vec::new();
    if stuck_leased > 0 {
        findings.push(json!({
            "severity": "critical",
            "code": "stuck_queue_items",
            "message": "Queue items are stale leased/running without acknowledgement."
        }));
    }
    if completed_count == 0 && routing_rows > 0 {
        findings.push(json!({
            "severity": "warning",
            "code": "no_completed_queue_latency",
            "message": "Queue rows exist but no completed leased-to-acknowledged duration can be measured."
        }));
    }
    if negative_duration_count > 0 {
        findings.push(json!({
            "severity": "warning",
            "code": "negative_queue_durations",
            "message": "Queue rows contain acknowledgement timestamps before lease timestamps; those rows are excluded from latency statistics."
        }));
    }
    let status = status_from_findings(&findings);
    Ok(subsystem_json(
        "queue_processing",
        status,
        "Queue forensics must expose throughput, stuck items, and fastest/slowest completed tasks.",
        json!({
            "routing_rows": routing_rows,
            "status_counts": status_counts,
            "active_leased": active_leased,
            "stuck_leased": stuck_leased,
            "stale_lease_cutoff": stale_lease_cutoff,
            "negative_duration_rows": negative_duration_count,
            "completed_with_duration": completed_count,
            "average_seconds": avg_seconds,
            "fastest": fastest,
            "slowest": slowest,
        }),
        findings,
    ))
}

fn diagnose_founder_review(conn: &Connection, current_scan_at: Option<&str>) -> Result<Value> {
    let reviews = table_count(conn, "communication_founder_reply_reviews")?;
    let historical_rejected_founder = if table_exists(conn, "ctox_pm_core_transition_audit")? {
        conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_core_transition_audit
            WHERE entity_type = 'FounderCommunication'
              AND accepted = 0
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    let current_rejected_founder = if let Some(scan_at) = current_scan_at {
        if table_exists(conn, "ctox_pm_core_transition_audit")? {
            conn.query_row(
                r#"
                SELECT COUNT(*)
                FROM ctox_pm_core_transition_audit
                WHERE entity_type = 'FounderCommunication'
                  AND accepted = 0
                  AND scanned_at = ?1
                "#,
                params![scan_at],
                |row| row.get::<_, i64>(0),
            )?
        } else {
            0
        }
    } else {
        historical_rejected_founder
    };
    let historical_critical_review_violations = if table_exists(conn, "ctox_pm_state_violations")? {
        conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_state_violations
            WHERE severity = 'critical'
              AND (
                  violation_code LIKE 'founder_%'
                  OR message LIKE '%Founder%'
                  OR message LIKE '%Communication%'
              )
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    let current_critical_review_violations = if let Some(scan_at) = current_scan_at {
        if table_exists(conn, "ctox_pm_state_violations")? {
            conn.query_row(
                r#"
                SELECT COUNT(*)
                FROM ctox_pm_state_violations
                WHERE severity = 'critical'
                  AND detected_at = ?1
                  AND (
                      violation_code LIKE 'founder_%'
                      OR message LIKE '%Founder%'
                      OR message LIKE '%Communication%'
                  )
                "#,
                params![scan_at],
                |row| row.get::<_, i64>(0),
            )?
        } else {
            0
        }
    } else {
        historical_critical_review_violations
    };
    let mut findings = Vec::new();
    if reviews == 0 {
        findings.push(json!({
            "severity": "warning",
            "code": "no_founder_review_rows",
            "message": "No founder reply review rows are present."
        }));
    }
    if current_rejected_founder > 0 || current_critical_review_violations > 0 {
        findings.push(json!({
            "severity": "critical",
            "code": "founder_review_gate_rejections",
            "message": "The current process-mining scan contains rejected or critical founder review-gate evidence."
        }));
    } else if historical_rejected_founder > 0 || historical_critical_review_violations > 0 {
        findings.push(json!({
            "severity": "warning",
            "code": "historical_founder_review_gate_rejections",
            "message": "Historical founder review-gate violations remain in the forensic ledger, but the current scan did not reproduce them."
        }));
    }
    let status = status_from_findings(&findings);
    Ok(subsystem_json(
        "founder_communication_review",
        status,
        "Founder communication must be blocked unless reviewed with matching content and recipients.",
        json!({
            "founder_reply_reviews": reviews,
            "current_scan_at": current_scan_at,
            "current_rejected_founder_transition_audits": current_rejected_founder,
            "current_critical_founder_review_violations": current_critical_review_violations,
            "historical_rejected_founder_transition_audits": historical_rejected_founder,
            "historical_critical_founder_review_violations": historical_critical_review_violations,
        }),
        findings,
    ))
}

fn diagnose_tickets(conn: &Connection) -> Result<Value> {
    let local_tickets = table_count(conn, "local_tickets")?;
    let self_work = table_count(conn, "ticket_self_work_items")?;
    let active_self_work = if table_exists(conn, "ticket_self_work_items")? {
        conn.query_row(
            "SELECT COUNT(*) FROM ticket_self_work_items WHERE state IN ('open','queued','published','blocked')",
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    let (closed_count, avg_seconds, fastest, slowest) = ticket_self_work_duration_stats(conn)?;
    let mut findings = Vec::new();
    if self_work >= 10 && local_tickets == 0 {
        findings.push(json!({
            "severity": "warning",
            "code": "self_work_without_canonical_tickets",
            "message": "Self-work dominates while canonical local tickets remain empty."
        }));
    }
    if active_self_work > 25 {
        findings.push(json!({
            "severity": "warning",
            "code": "large_active_self_work_backlog",
            "message": "The active self-work backlog is high."
        }));
    }
    let status = status_from_findings(&findings);
    Ok(subsystem_json(
        "tickets_and_self_work",
        status,
        "Ticket/self-work forensics must expose backlog, closure rate, and task duration extremes.",
        json!({
            "local_tickets": local_tickets,
            "ticket_self_work_items": self_work,
            "active_self_work": active_self_work,
            "closed_self_work_with_duration": closed_count,
            "average_seconds": avg_seconds,
            "fastest": fastest,
            "slowest": slowest,
        }),
        findings,
    ))
}

fn diagnose_schedules(conn: &Connection) -> Result<Value> {
    let tasks = table_count(conn, "scheduled_tasks")?;
    let runs = table_count(conn, "scheduled_task_runs")?;
    let enabled = if table_exists(conn, "scheduled_tasks")? {
        conn.query_row(
            "SELECT COUNT(*) FROM scheduled_tasks WHERE enabled != 0",
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    let due_enabled = if table_exists(conn, "scheduled_tasks")? {
        conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM scheduled_tasks
            WHERE enabled != 0
              AND next_run_at IS NOT NULL
              AND next_run_at <= strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
    let mut findings = Vec::new();
    if enabled > 0 && runs == 0 {
        findings.push(json!({
            "severity": "warning",
            "code": "scheduled_tasks_without_runs",
            "message": "Enabled scheduled tasks exist but no emitted runs are recorded."
        }));
    }
    if due_enabled > 0 {
        findings.push(json!({
            "severity": "critical",
            "code": "overdue_scheduled_tasks",
            "message": "Enabled scheduled tasks are due and have not been emitted."
        }));
    }
    let status = status_from_findings(&findings);
    Ok(subsystem_json(
        "schedules_and_commitments",
        status,
        "Deadline and commitment backing needs scheduled tasks with emitted runs before due time.",
        json!({
            "scheduled_tasks": tasks,
            "enabled_scheduled_tasks": enabled,
            "scheduled_task_runs": runs,
            "due_enabled_tasks": due_enabled,
        }),
        findings,
    ))
}

fn status_from_findings(findings: &[Value]) -> &'static str {
    if findings
        .iter()
        .any(|finding| finding.get("severity").and_then(Value::as_str) == Some("critical"))
    {
        "critical"
    } else if findings.is_empty() {
        "ok"
    } else {
        "warning"
    }
}

fn table_exists(conn: &Connection, table_name: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![table_name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn table_count(conn: &Connection, table_name: &str) -> Result<i64> {
    if !table_exists(conn, table_name)? {
        return Ok(0);
    }
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {}", quote_ident(table_name)),
        [],
        |row| row.get(0),
    )
    .map_err(anyhow::Error::from)
}

fn recent_table_event_count(conn: &Connection, table_like: &str) -> Result<i64> {
    if !table_exists(conn, PROCESS_EVENTS_TABLE)? {
        return Ok(0);
    }
    conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM (
            SELECT table_name
            FROM ctox_process_events
            ORDER BY event_seq DESC
            LIMIT 5000
        )
        WHERE table_name LIKE ?1
        "#,
        params![table_like],
        |row| row.get(0),
    )
    .map_err(anyhow::Error::from)
}

fn grouped_counts(conn: &Connection, table_name: &str, column_name: &str) -> Result<Value> {
    if !table_exists(conn, table_name)? {
        return Ok(json!({}));
    }
    let sql = format!(
        "SELECT CAST({column} AS TEXT), COUNT(*) FROM {table} GROUP BY {column} ORDER BY COUNT(*) DESC, {column}",
        column = quote_ident(column_name),
        table = quote_ident(table_name)
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?
                    .unwrap_or_else(|| "<null>".to_string()),
                row.get::<_, i64>(1)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!(rows
        .into_iter()
        .map(|(state, count)| json!({"state": state, "count": count}))
        .collect::<Vec<_>>()))
}

fn routing_duration_stats(
    conn: &Connection,
    table_name: &str,
    key_column: &str,
) -> Result<(i64, Option<f64>, Value, Value)> {
    if !table_exists(conn, table_name)? {
        return Ok((0, None, Value::Null, Value::Null));
    }
    if !table_has_column(conn, table_name, "leased_at")?
        || !table_has_column(conn, table_name, "acked_at")?
    {
        return Ok((0, None, Value::Null, Value::Null));
    }
    let table = quote_ident(table_name);
    let (count, avg): (i64, Option<f64>) = conn.query_row(
        &format!(
            "SELECT COUNT(*), AVG((julianday(acked_at) - julianday(leased_at)) * 86400.0)
             FROM {table}
             WHERE leased_at IS NOT NULL AND acked_at IS NOT NULL
               AND ((julianday(acked_at) - julianday(leased_at)) * 86400.0) >= 0.0"
        ),
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let fastest = duration_extreme(conn, table_name, key_column, "ASC")?;
    let slowest = duration_extreme(conn, table_name, key_column, "DESC")?;
    Ok((count, avg, fastest, slowest))
}

fn duration_extreme(
    conn: &Connection,
    table_name: &str,
    key_column: &str,
    direction: &str,
) -> Result<Value> {
    if !table_has_column(conn, table_name, key_column)?
        || !table_has_column(conn, table_name, "leased_at")?
        || !table_has_column(conn, table_name, "acked_at")?
    {
        return Ok(Value::Null);
    }
    let sql = format!(
        "SELECT {key}, ((julianday(acked_at) - julianday(leased_at)) * 86400.0) AS seconds
         FROM {table}
         WHERE leased_at IS NOT NULL AND acked_at IS NOT NULL
           AND ((julianday(acked_at) - julianday(leased_at)) * 86400.0) >= 0.0
         ORDER BY seconds {direction}
         LIMIT 1",
        key = quote_ident(key_column),
        table = quote_ident(table_name),
        direction = direction,
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "seconds": row.get::<_, Option<f64>>(1)?,
        }))
    } else {
        Ok(Value::Null)
    }
}

fn negative_routing_duration_count(conn: &Connection, table_name: &str) -> Result<i64> {
    if !table_exists(conn, table_name)?
        || !table_has_column(conn, table_name, "leased_at")?
        || !table_has_column(conn, table_name, "acked_at")?
    {
        return Ok(0);
    }
    let sql = format!(
        "SELECT COUNT(*)
         FROM {table}
         WHERE leased_at IS NOT NULL AND acked_at IS NOT NULL
           AND ((julianday(acked_at) - julianday(leased_at)) * 86400.0) < 0.0",
        table = quote_ident(table_name),
    );
    Ok(conn.query_row(&sql, [], |row| row.get(0))?)
}

fn ticket_self_work_duration_stats(conn: &Connection) -> Result<(i64, Option<f64>, Value, Value)> {
    if !table_exists(conn, "ticket_self_work_items")? {
        return Ok((0, None, Value::Null, Value::Null));
    }
    let has_created_at = table_has_column(conn, "ticket_self_work_items", "created_at")?;
    let has_updated_at = table_has_column(conn, "ticket_self_work_items", "updated_at")?;
    if !has_created_at || !has_updated_at {
        return Ok((0, None, Value::Null, Value::Null));
    }
    let (count, avg): (i64, Option<f64>) = conn.query_row(
        r#"
        SELECT COUNT(*), AVG((julianday(updated_at) - julianday(created_at)) * 86400.0)
        FROM ticket_self_work_items
        WHERE state = 'closed'
          AND created_at IS NOT NULL
          AND updated_at IS NOT NULL
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let fastest = self_work_duration_extreme(conn, "ASC")?;
    let slowest = self_work_duration_extreme(conn, "DESC")?;
    Ok((count, avg, fastest, slowest))
}

fn self_work_duration_extreme(conn: &Connection, direction: &str) -> Result<Value> {
    let sql = format!(
        "SELECT work_id, title, ((julianday(updated_at) - julianday(created_at)) * 86400.0) AS seconds
         FROM ticket_self_work_items
         WHERE state = 'closed'
           AND created_at IS NOT NULL
           AND updated_at IS NOT NULL
         ORDER BY seconds {direction}
         LIMIT 1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        Ok(json!({
            "work_id": row.get::<_, String>(0)?,
            "title": row.get::<_, Option<String>>(1)?,
            "seconds": row.get::<_, Option<f64>>(2)?,
        }))
    } else {
        Ok(Value::Null)
    }
}

fn table_has_column(conn: &Connection, table_name: &str, column_name: &str) -> Result<bool> {
    if !table_exists(conn, table_name)? {
        return Ok(false);
    }
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", quote_ident(table_name)))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column_name {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ensure_table_column(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    column_ddl: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", quote_ident(table_name)))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .any(|name| name == column_name);
    if !exists {
        conn.execute_batch(&format!(
            "ALTER TABLE {} ADD COLUMN {};",
            quote_ident(table_name),
            column_ddl
        ))?;
    }
    Ok(())
}

fn upsert_default_core_transition_rules(conn: &Connection) -> Result<()> {
    let now = now_expr_value();
    let defaults = [
        (
            "context-message-telemetry",
            5,
            Some("=messages"),
            None,
            None,
            None,
            "telemetry",
            "ContextMessage",
            "P1RuntimeSafety",
            "telemetry.context.message",
            json!({"core_transition": false}),
        ),
        (
            "context-item-telemetry",
            6,
            Some("=context_items"),
            None,
            None,
            None,
            "telemetry",
            "ContextItem",
            "P1RuntimeSafety",
            "telemetry.context.item",
            json!({"core_transition": false}),
        ),
        (
            "governance-event-telemetry",
            7,
            Some("=governance_events"),
            None,
            None,
            None,
            "telemetry",
            "GovernanceEvent",
            "P3Housekeeping",
            "telemetry.governance.event",
            json!({"core_transition": false}),
        ),
        (
            "mission-claim-telemetry",
            8,
            Some("=mission_claims"),
            None,
            None,
            None,
            "telemetry",
            "MissionClaim",
            "P1RuntimeSafety",
            "telemetry.mission.claim",
            json!({"core_transition": false}),
        ),
        (
            "ticket-audit-telemetry",
            9,
            Some("=ticket_audit_log"),
            None,
            None,
            None,
            "telemetry",
            "TicketAuditLog",
            "P2MissionDelivery",
            "telemetry.ticket.audit",
            json!({"core_transition": false}),
        ),
        (
            "ticket-note-telemetry",
            10,
            Some("=ticket_self_work_notes"),
            None,
            None,
            None,
            "telemetry",
            "TicketSelfWorkNote",
            "P2MissionDelivery",
            "telemetry.ticket.note",
            json!({"core_transition": false}),
        ),
        (
            "local-ticket-event-telemetry",
            11,
            Some("=local_ticket_events"),
            None,
            None,
            None,
            "telemetry",
            "LocalTicketEvent",
            "P2MissionDelivery",
            "telemetry.ticket.local_event",
            json!({"core_transition": false}),
        ),
        (
            "continuity-document-telemetry",
            12,
            Some("=continuity_documents"),
            None,
            None,
            None,
            "telemetry",
            "ContinuityDocument",
            "P1RuntimeSafety",
            "telemetry.continuity.document",
            json!({"core_transition": false}),
        ),
        (
            "continuity-commit-telemetry",
            13,
            Some("=continuity_commits"),
            None,
            None,
            None,
            "telemetry",
            "ContinuityCommit",
            "P1RuntimeSafety",
            "telemetry.continuity.commit",
            json!({"core_transition": false}),
        ),
        (
            "verification-run-telemetry",
            14,
            Some("=verification_runs"),
            None,
            None,
            None,
            "telemetry",
            "VerificationRun",
            "P1RuntimeSafety",
            "telemetry.verification.run",
            json!({"core_transition": false}),
        ),
        (
            "ticket-knowledge-entry-telemetry",
            15,
            Some("=ticket_knowledge_entries"),
            None,
            None,
            None,
            "telemetry",
            "TicketKnowledgeEntry",
            "P1RuntimeSafety",
            "telemetry.ticket.knowledge_entry",
            json!({"core_transition": false}),
        ),
        (
            "ticket-knowledge-load-telemetry",
            16,
            Some("=ticket_knowledge_loads"),
            None,
            None,
            None,
            "telemetry",
            "TicketKnowledgeLoad",
            "P1RuntimeSafety",
            "telemetry.ticket.knowledge_load",
            json!({"core_transition": false}),
        ),
        (
            "ticket-self-work-assignment-telemetry",
            17,
            Some("=ticket_self_work_assignments"),
            None,
            None,
            None,
            "telemetry",
            "TicketSelfWorkAssignment",
            "P2MissionDelivery",
            "telemetry.ticket.self_work_assignment",
            json!({"core_transition": false}),
        ),
        (
            "skill-bundle-telemetry",
            18,
            Some("=ctox_skill_bundles"),
            None,
            None,
            None,
            "telemetry",
            "SkillBundle",
            "P1RuntimeSafety",
            "telemetry.skill.bundle",
            json!({"core_transition": false, "records_runtime_skill_registry": true}),
        ),
        (
            "skill-file-telemetry",
            19,
            Some("=ctox_skill_files"),
            None,
            None,
            None,
            "telemetry",
            "SkillFile",
            "P1RuntimeSafety",
            "telemetry.skill.file",
            json!({"core_transition": false, "records_runtime_skill_registry": true}),
        ),
        (
            "strategic-directive-telemetry",
            20,
            Some("=strategic_directives"),
            None,
            None,
            None,
            "telemetry",
            "StrategicDirective",
            "P1RuntimeSafety",
            "telemetry.strategy.directive",
            json!({"core_transition": false, "records_runtime_strategy": true}),
        ),
        (
            "communication-founder",
            10,
            Some("communication_founder"),
            None,
            None,
            None,
            "communication",
            "FounderCommunication",
            "P0FounderCommunication",
            "core.communication.founder",
            json!({
                "requires_review_for_send": true,
                "requires_body_hash_match": true,
                "requires_recipient_hash_match": true
            }),
        ),
        (
            "communication-mail",
            20,
            Some("mail"),
            None,
            None,
            None,
            "communication",
            "FounderCommunication",
            "P0FounderCommunication",
            "core.communication.mail",
            json!({
                "requires_review_for_send": true,
                "requires_body_hash_match": true,
                "requires_recipient_hash_match": true
            }),
        ),
        (
            "communication-routing-state",
            25,
            Some("communication_routing_state"),
            None,
            None,
            None,
            "queue",
            "QueueItem",
            "P2MissionDelivery",
            "core.queue.routing",
            json!({"route_status_is_queue_state": true}),
        ),
        (
            "communication-account-telemetry",
            26,
            Some("communication_accounts"),
            None,
            None,
            None,
            "telemetry",
            "CommunicationAccount",
            "P3Housekeeping",
            "telemetry.communication.account",
            json!({"core_transition": false}),
        ),
        (
            "communication-thread-telemetry",
            27,
            Some("communication_threads"),
            None,
            None,
            None,
            "telemetry",
            "CommunicationThread",
            "P3Housekeeping",
            "telemetry.communication.thread",
            json!({"core_transition": false}),
        ),
        (
            "communication-sync-telemetry",
            28,
            Some("communication_sync_runs"),
            None,
            None,
            None,
            "telemetry",
            "CommunicationSyncRun",
            "P3Housekeeping",
            "telemetry.communication.sync",
            json!({"core_transition": false}),
        ),
        (
            "communication-message",
            30,
            Some("=communication_messages"),
            None,
            None,
            None,
            "communication",
            "FounderCommunication",
            "P0FounderCommunication",
            "core.communication.message",
            json!({
                "requires_review_for_send": true,
                "requires_body_hash_match": true,
                "requires_recipient_hash_match": true
            }),
        ),
        (
            "queue-item",
            40,
            Some("queue"),
            None,
            None,
            None,
            "queue",
            "QueueItem",
            "P2MissionDelivery",
            "core.queue.item",
            json!({"protected_party_escalates_lane": true}),
        ),
        (
            "ticket",
            50,
            Some("ticket"),
            None,
            None,
            None,
            "ticket",
            "Ticket",
            "P2MissionDelivery",
            "core.ticket",
            json!({"closed_requires_verification": true}),
        ),
        (
            "work-item",
            60,
            Some("work_item"),
            None,
            None,
            None,
            "ticket",
            "WorkItem",
            "P2MissionDelivery",
            "core.work_item",
            json!({"closed_requires_verification": true}),
        ),
        (
            "commitment",
            70,
            Some("commitment"),
            None,
            None,
            None,
            "commitment",
            "Commitment",
            "P0CommitmentBacking",
            "core.commitment",
            json!({"active_requires_schedule": true}),
        ),
        (
            "deadline",
            80,
            Some("deadline"),
            None,
            None,
            None,
            "commitment",
            "Commitment",
            "P0CommitmentBacking",
            "core.deadline",
            json!({"active_requires_schedule": true}),
        ),
        (
            "schedule",
            90,
            Some("schedule"),
            None,
            None,
            None,
            "schedule",
            "Schedule",
            "P0CommitmentBacking",
            "core.schedule",
            json!({"commitment_schedule_requires_replacement_or_escalation": true}),
        ),
        (
            "cron",
            100,
            Some("cron"),
            None,
            None,
            None,
            "schedule",
            "Schedule",
            "P0CommitmentBacking",
            "core.cron",
            json!({"commitment_schedule_requires_replacement_or_escalation": true}),
        ),
        (
            "repair",
            110,
            Some("repair"),
            None,
            None,
            None,
            "repair",
            "Repair",
            "P1QueueRepair",
            "core.repair",
            json!({"requires_canonical_hot_path": true}),
        ),
        (
            "knowledge",
            120,
            Some("knowledge"),
            None,
            None,
            None,
            "knowledge",
            "Knowledge",
            "P1RuntimeSafety",
            "core.knowledge",
            json!({"active_requires_incident": true}),
        ),
        (
            "scrape-subsystem-telemetry",
            130,
            Some("scrape_"),
            None,
            None,
            None,
            "telemetry",
            "ScrapeTelemetry",
            "P2MissionDelivery",
            "telemetry.scrape.subsystem",
            json!({
                "core_transition": false,
                "records_scrape_run": true,
                "records_scrape_snapshot": true
            }),
        ),
        (
            "secret-store-telemetry",
            140,
            Some("ctox_secret"),
            None,
            None,
            None,
            "telemetry",
            "SecretStore",
            "P1RuntimeSafety",
            "telemetry.secret.store",
            json!({
                "core_transition": false,
                "records_secret_store_mutation": true,
                "requires_redaction": true
            }),
        ),
        (
            "turn-ledger",
            800,
            Some("ctox_turns"),
            None,
            None,
            None,
            "telemetry",
            "TurnLedger",
            "P1RuntimeSafety",
            "telemetry.turn.ledger",
            json!({"core_transition": false, "records_multiturn_lifecycle": true}),
        ),
        (
            "turn-command-ledger",
            810,
            Some("ctox_turn_commands"),
            None,
            None,
            None,
            "telemetry",
            "TurnCommandLedger",
            "P1RuntimeSafety",
            "telemetry.turn.command",
            json!({"core_transition": false, "records_cli_command_lifecycle": true}),
        ),
        (
            "core-transition-proof-ledger",
            820,
            Some("ctox_core_transition_proofs"),
            None,
            None,
            None,
            "telemetry",
            "CoreTransitionProofLedger",
            "P1RuntimeSafety",
            "telemetry.core.transition.proof",
            json!({"core_transition": false, "records_state_machine_proofs": true}),
        ),
        (
            "core-spawn-edge-ledger",
            821,
            Some("=ctox_core_spawn_edges"),
            None,
            None,
            None,
            "telemetry",
            "CoreSpawnEdgeLedger",
            "P1RuntimeSafety",
            "telemetry.core.spawn.edge",
            json!({"core_transition": false, "records_core_spawn_proofs": true}),
        ),
        (
            "harness-mining-audit-run-telemetry",
            822,
            Some("=ctox_hm_audit_runs"),
            None,
            None,
            None,
            "telemetry",
            "HarnessMiningAuditRun",
            "P1RuntimeSafety",
            "telemetry.harness_mining.audit_run",
            json!({"core_transition": false, "records_harness_mining": true}),
        ),
        (
            "harness-mining-finding-telemetry",
            823,
            Some("=ctox_hm_findings"),
            None,
            None,
            None,
            "telemetry",
            "HarnessMiningFinding",
            "P1RuntimeSafety",
            "telemetry.harness_mining.finding",
            json!({"core_transition": false, "records_harness_mining": true}),
        ),
        (
            "runtime-env-telemetry",
            824,
            Some("=runtime_env_kv"),
            None,
            None,
            None,
            "telemetry",
            "RuntimeEnvKv",
            "P1RuntimeSafety",
            "telemetry.runtime.env",
            json!({"core_transition": false, "requires_redaction": true}),
        ),
        (
            "payload-store-telemetry",
            830,
            Some("ctox_payload_store"),
            None,
            None,
            None,
            "telemetry",
            "PayloadStore",
            "P3Housekeeping",
            "telemetry.payload.store",
            json!({"core_transition": false}),
        ),
        (
            "operating-health-telemetry",
            840,
            Some("operating_health_snapshots"),
            None,
            None,
            None,
            "telemetry",
            "OperatingHealthSnapshot",
            "P3Housekeeping",
            "telemetry.operating_health.snapshot",
            json!({"core_transition": false}),
        ),
        (
            "governance-telemetry",
            850,
            Some("governance_mechanisms"),
            None,
            None,
            None,
            "telemetry",
            "GovernanceMechanism",
            "P3Housekeeping",
            "telemetry.governance.mechanism",
            json!({"core_transition": false}),
        ),
        (
            "mission-state-telemetry",
            860,
            Some("mission_states"),
            None,
            None,
            None,
            "telemetry",
            "MissionState",
            "P3Housekeeping",
            "telemetry.mission.state",
            json!({"core_transition": false}),
        ),
        (
            "sqlite-read-telemetry",
            900,
            None,
            None,
            Some("READ"),
            None,
            "telemetry",
            "Telemetry",
            "P3Housekeeping",
            "telemetry.sqlite.read",
            json!({"core_transition": false}),
        ),
        (
            "sqlite-attach-telemetry",
            910,
            None,
            None,
            Some("ATTACH"),
            None,
            "telemetry",
            "Telemetry",
            "P3Housekeeping",
            "telemetry.sqlite.attach",
            json!({"core_transition": false}),
        ),
    ];
    for (
        rule_id,
        priority,
        table_pattern,
        entity_type_pattern,
        operation_pattern,
        activity_pattern,
        inference_kind,
        core_entity_type,
        runtime_lane,
        petri_transition_id,
        evidence_policy,
    ) in defaults
    {
        upsert_core_transition_rule(
            conn,
            rule_id,
            priority,
            table_pattern,
            entity_type_pattern,
            operation_pattern,
            activity_pattern,
            inference_kind,
            core_entity_type,
            runtime_lane,
            petri_transition_id,
            &serde_json::to_string(&evidence_policy)?,
            &now,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn upsert_core_transition_rule(
    conn: &Connection,
    rule_id: &str,
    priority: i64,
    table_pattern: Option<&str>,
    entity_type_pattern: Option<&str>,
    operation_pattern: Option<&str>,
    activity_pattern: Option<&str>,
    inference_kind: &str,
    core_entity_type: &str,
    runtime_lane: &str,
    petri_transition_id: &str,
    evidence_policy_json: &str,
    now: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO ctox_pm_core_transition_rules (
            rule_id, priority, table_pattern, entity_type_pattern,
            operation_pattern, activity_pattern, inference_kind,
            core_entity_type, runtime_lane, petri_transition_id,
            evidence_policy_json, enabled, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12, ?12)
        ON CONFLICT(rule_id) DO UPDATE SET
            priority = excluded.priority,
            table_pattern = excluded.table_pattern,
            entity_type_pattern = excluded.entity_type_pattern,
            operation_pattern = excluded.operation_pattern,
            activity_pattern = excluded.activity_pattern,
            inference_kind = excluded.inference_kind,
            core_entity_type = excluded.core_entity_type,
            runtime_lane = excluded.runtime_lane,
            petri_transition_id = excluded.petri_transition_id,
            evidence_policy_json = excluded.evidence_policy_json,
            enabled = 1,
            updated_at = excluded.updated_at
        "#,
        params![
            rule_id,
            priority,
            table_pattern,
            entity_type_pattern,
            operation_pattern,
            activity_pattern,
            inference_kind,
            core_entity_type,
            runtime_lane,
            petri_transition_id,
            evidence_policy_json,
            now,
        ],
    )?;
    Ok(())
}

fn load_core_transition_rules(conn: &Connection) -> Result<Vec<CoreTransitionRule>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT rule_id, priority, table_pattern, entity_type_pattern,
               operation_pattern, activity_pattern, inference_kind,
               core_entity_type, runtime_lane, petri_transition_id,
               evidence_policy_json
        FROM ctox_pm_core_transition_rules
        WHERE enabled = 1
        ORDER BY priority, rule_id
        "#,
    )?;
    let rules = stmt
        .query_map([], |row| {
            Ok(CoreTransitionRule {
                rule_id: row.get(0)?,
                priority: row.get(1)?,
                table_pattern: row.get(2)?,
                entity_type_pattern: row.get(3)?,
                operation_pattern: row.get(4)?,
                activity_pattern: row.get(5)?,
                inference_kind: row.get(6)?,
                core_entity_type: row.get(7)?,
                runtime_lane: row.get(8)?,
                petri_transition_id: row.get(9)?,
                evidence_policy_json: row.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rules)
}

fn match_core_transition_rule<'a>(
    event: &ProcessEventForStateMachine,
    rules: &'a [CoreTransitionRule],
) -> Option<&'a CoreTransitionRule> {
    rules.iter().find(|rule| {
        pattern_matches(rule.table_pattern.as_deref(), &event.table_name)
            && pattern_matches(rule.entity_type_pattern.as_deref(), &event.entity_type)
            && pattern_matches(rule.operation_pattern.as_deref(), &event.operation)
            && pattern_matches(rule.activity_pattern.as_deref(), &event.activity)
    })
}

fn pattern_matches(pattern: Option<&str>, value: &str) -> bool {
    let Some(pattern) = pattern else {
        return true;
    };
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern == "*" {
        return true;
    }
    if let Some(exact) = pattern.strip_prefix('=') {
        return value.eq_ignore_ascii_case(exact.trim());
    }
    value
        .to_ascii_lowercase()
        .contains(&pattern.to_ascii_lowercase())
}

fn is_telemetry_rule(rule: &CoreTransitionRule) -> bool {
    rule.inference_kind == "telemetry"
}

fn infer_core_transition_from_rule(
    rule: &CoreTransitionRule,
    event: &ProcessEventForStateMachine,
) -> Option<csm::CoreTransitionRequest> {
    let before = parse_json_value(&event.row_before_json);
    let after = parse_json_value(&event.row_after_json);
    let haystack = event_haystack(event, &before, &after);
    let mut request = match rule.inference_kind.as_str() {
        "communication" => infer_communication_transition(event, &after, &haystack),
        "queue" => infer_queue_transition(event, &before, &after, &haystack),
        "ticket" => infer_ticket_transition(event, &after, &haystack),
        "commitment" => infer_commitment_transition(event, &after),
        "schedule" => infer_schedule_transition(event, &after),
        "repair" => infer_repair_transition(event, &after),
        "knowledge" => infer_knowledge_transition(event, &after),
        _ => None,
    }?;
    request
        .metadata
        .insert("mapping_rule_id".to_string(), rule.rule_id.clone());
    request.metadata.insert(
        "petri_transition_id".to_string(),
        rule.petri_transition_id.clone(),
    );
    request.metadata.insert(
        "rule_core_entity_type".to_string(),
        rule.core_entity_type.clone(),
    );
    request
        .metadata
        .insert("rule_runtime_lane".to_string(), rule.runtime_lane.clone());
    request.metadata.insert(
        "rule_evidence_policy".to_string(),
        rule.evidence_policy_json.clone(),
    );
    request
        .metadata
        .insert("rule_priority".to_string(), rule.priority.to_string());
    Some(request)
}

fn record_event_coverage(
    conn: &Connection,
    event: &ProcessEventForStateMachine,
    mapping_kind: &str,
    rule_id: Option<&str>,
    petri_transition_id: Option<&str>,
    reason: &str,
    scanned_at: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO ctox_pm_event_transition_coverage (
            event_id, case_id, table_name, entity_type, operation, activity,
            mapping_kind, rule_id, petri_transition_id, reason,
            observed_at, scanned_at, metadata_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(event_id) DO UPDATE SET
            case_id = excluded.case_id,
            table_name = excluded.table_name,
            entity_type = excluded.entity_type,
            operation = excluded.operation,
            activity = excluded.activity,
            mapping_kind = excluded.mapping_kind,
            rule_id = excluded.rule_id,
            petri_transition_id = excluded.petri_transition_id,
            reason = excluded.reason,
            observed_at = excluded.observed_at,
            scanned_at = excluded.scanned_at,
            metadata_json = excluded.metadata_json
        "#,
        params![
            event.event_id,
            event.case_id,
            event.table_name,
            event.entity_type,
            event.operation,
            event.activity,
            mapping_kind,
            rule_id,
            petri_transition_id,
            reason,
            event.observed_at,
            scanned_at,
            serde_json::to_string(&json!({
                "from_state": event.from_state,
                "to_state": event.to_state,
                "command_name": event.command_name,
            }))?,
        ],
    )?;
    Ok(())
}

fn record_unmapped_event(
    conn: &Connection,
    event: &ProcessEventForStateMachine,
    reason: &str,
    scanned_at: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO ctox_pm_unmapped_events (
            event_id, case_id, table_name, entity_type, operation,
            activity, reason, observed_at, scanned_at, metadata_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(event_id) DO UPDATE SET
            case_id = excluded.case_id,
            table_name = excluded.table_name,
            entity_type = excluded.entity_type,
            operation = excluded.operation,
            activity = excluded.activity,
            reason = excluded.reason,
            observed_at = excluded.observed_at,
            scanned_at = excluded.scanned_at,
            metadata_json = excluded.metadata_json
        "#,
        params![
            event.event_id,
            event.case_id,
            event.table_name,
            event.entity_type,
            event.operation,
            event.activity,
            reason,
            event.observed_at,
            scanned_at,
            serde_json::to_string(&json!({
                "from_state": event.from_state,
                "to_state": event.to_state,
                "command_name": event.command_name,
            }))?,
        ],
    )?;
    Ok(())
}

fn clear_unmapped_event(conn: &Connection, event_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM ctox_pm_unmapped_events WHERE event_id = ?1",
        params![event_id],
    )?;
    Ok(())
}

fn clear_state_violations_for_event(conn: &Connection, event_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM ctox_pm_state_violations WHERE event_id = ?1",
        params![event_id],
    )?;
    Ok(())
}

fn unmapped_event_json(event: &ProcessEventForStateMachine, reason: &str) -> Value {
    json!({
        "event_id": event.event_id,
        "case_id": event.case_id,
        "table_name": event.table_name,
        "entity_type": event.entity_type,
        "operation": event.operation,
        "activity": event.activity,
        "from_state": event.from_state,
        "to_state": event.to_state,
        "reason": reason,
    })
}

fn scan_core_state_machine_violations(conn: &Connection, limit: i64) -> Result<Value> {
    let scanned_at = now_expr_value();
    let events = load_state_machine_events(conn, limit)?;
    let rules = load_core_transition_rules(conn)?;
    let scanned_events = events.len();
    let mut inferred_transitions = 0usize;
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut mapped_telemetry = 0usize;
    let mut rule_matched_without_core_transition = 0usize;
    let mut unmapped = 0usize;
    let mut violation_count = 0usize;
    let mut recent_rejections = Vec::new();
    let mut recent_unmapped = Vec::new();

    for event in events {
        let Some(rule) = match_core_transition_rule(&event, &rules) else {
            unmapped += 1;
            record_event_coverage(
                conn,
                &event,
                "unmapped",
                None,
                None,
                "no_enabled_core_transition_rule_matched",
                &scanned_at,
            )?;
            record_unmapped_event(
                conn,
                &event,
                "no_enabled_core_transition_rule_matched",
                &scanned_at,
            )?;
            if recent_unmapped.len() < 20 {
                recent_unmapped.push(unmapped_event_json(
                    &event,
                    "no_enabled_core_transition_rule_matched",
                ));
            }
            continue;
        };
        if is_telemetry_rule(&rule) {
            mapped_telemetry += 1;
            record_event_coverage(
                conn,
                &event,
                "telemetry",
                Some(&rule.rule_id),
                Some(&rule.petri_transition_id),
                "explicit_telemetry_rule",
                &scanned_at,
            )?;
            clear_unmapped_event(conn, &event.event_id)?;
            continue;
        }

        if is_state_preserving_update(&event) {
            mapped_telemetry += 1;
            record_event_coverage(
                conn,
                &event,
                "telemetry",
                Some(&rule.rule_id),
                Some(&rule.petri_transition_id),
                "state_preserving_update",
                &scanned_at,
            )?;
            clear_unmapped_event(conn, &event.event_id)?;
            continue;
        }

        let Some(request) = infer_core_transition_from_rule(&rule, &event) else {
            rule_matched_without_core_transition += 1;
            record_event_coverage(
                conn,
                &event,
                "rule_matched_without_core_transition",
                Some(&rule.rule_id),
                Some(&rule.petri_transition_id),
                "rule_matched_but_state_or_inference_kind_is_unmapped",
                &scanned_at,
            )?;
            record_unmapped_event(
                conn,
                &event,
                "rule_matched_but_state_or_inference_kind_is_unmapped",
                &scanned_at,
            )?;
            if recent_unmapped.len() < 20 {
                recent_unmapped.push(unmapped_event_json(
                    &event,
                    "rule_matched_but_state_or_inference_kind_is_unmapped",
                ));
            }
            continue;
        };

        if event.operation.eq_ignore_ascii_case("UPDATE") && request.from_state == request.to_state
        {
            mapped_telemetry += 1;
            record_event_coverage(
                conn,
                &event,
                "telemetry",
                Some(&rule.rule_id),
                Some(&rule.petri_transition_id),
                "state_preserving_inferred_transition",
                &scanned_at,
            )?;
            clear_unmapped_event(conn, &event.event_id)?;
            clear_state_violations_for_event(conn, &event.event_id)?;
            continue;
        }

        inferred_transitions += 1;
        let proof = core_transition_guard::evaluate_core_transition(conn, &request)?;
        let report = proof.report.clone();
        let violation_codes = report
            .violations
            .iter()
            .map(|violation| violation.code.clone())
            .collect::<Vec<_>>();
        let request_json = serde_json::to_string(&request)?;
        let audit_id = format!("core-{}", event.event_id);

        conn.execute(
            r#"
            INSERT INTO ctox_pm_core_transition_audit (
                audit_id, event_id, case_id, rule_id, petri_transition_id,
                entity_type, entity_id, lane,
                from_state, to_state, core_event, accepted,
                violation_codes_json, proof_id, request_json, observed_at, scanned_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            ON CONFLICT(audit_id) DO UPDATE SET
                case_id = excluded.case_id,
                rule_id = excluded.rule_id,
                petri_transition_id = excluded.petri_transition_id,
                entity_type = excluded.entity_type,
                entity_id = excluded.entity_id,
                lane = excluded.lane,
                from_state = excluded.from_state,
                to_state = excluded.to_state,
                core_event = excluded.core_event,
                accepted = excluded.accepted,
                violation_codes_json = excluded.violation_codes_json,
                proof_id = excluded.proof_id,
                request_json = excluded.request_json,
                observed_at = excluded.observed_at,
                scanned_at = excluded.scanned_at
            "#,
            params![
                audit_id,
                event.event_id,
                event.case_id,
                rule.rule_id,
                rule.petri_transition_id,
                format!("{:?}", request.entity_type),
                request.entity_id,
                format!("{:?}", request.lane),
                format!("{:?}", request.from_state),
                format!("{:?}", request.to_state),
                format!("{:?}", request.event),
                if report.accepted { 1 } else { 0 },
                serde_json::to_string(&violation_codes)?,
                proof.proof_id,
                request_json,
                event.observed_at,
                scanned_at,
            ],
        )?;
        record_event_coverage(
            conn,
            &event,
            "core_transition",
            Some(&rule.rule_id),
            Some(&rule.petri_transition_id),
            "core_transition_request_validated",
            &scanned_at,
        )?;
        clear_unmapped_event(conn, &event.event_id)?;
        clear_state_violations_for_event(conn, &event.event_id)?;

        if report.accepted {
            accepted += 1;
            continue;
        }

        rejected += 1;
        violation_count += report.violations.len();
        if recent_rejections.len() < 20 {
            recent_rejections.push(json!({
                "event_id": event.event_id,
                "case_id": event.case_id,
                "entity_type": format!("{:?}", request.entity_type),
                "entity_id": request.entity_id.clone(),
                "from_state": format!("{:?}", request.from_state),
                "to_state": format!("{:?}", request.to_state),
                "violations": violation_codes,
            }));
        }

        for violation in report.violations {
            let violation_id = format!("core-{}-{}", event.event_id, violation.code);
            conn.execute(
                r#"
                INSERT OR REPLACE INTO ctox_pm_state_violations (
                    violation_id, event_id, case_id, violation_code, severity,
                    message, detected_at, evidence_json
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    violation_id,
                    event.event_id,
                    event.case_id,
                    violation.code,
                    core_violation_severity(&request, &violation.code),
                    violation.message,
                    scanned_at,
                    serde_json::to_string(&json!({
                        "source": "core_state_machine",
                        "activity": event.activity,
                        "table_name": event.table_name,
                        "operation": event.operation,
                        "from_state": event.from_state,
                        "to_state": event.to_state,
                        "command_name": event.command_name,
                        "request": &request,
                    }))?,
                ],
            )?;
        }
    }

    Ok(json!({
        "ok": true,
        "scanned_events": scanned_events,
        "inferred_transitions": inferred_transitions,
        "accepted": accepted,
        "rejected": rejected,
        "mapped_telemetry": mapped_telemetry,
        "rule_matched_without_core_transition": rule_matched_without_core_transition,
        "unmapped": unmapped,
        "violation_count": violation_count,
        "recent_rejections": recent_rejections,
        "recent_unmapped": recent_unmapped,
        "scanned_at": scanned_at,
    }))
}

fn is_state_preserving_update(event: &ProcessEventForStateMachine) -> bool {
    if !event.operation.eq_ignore_ascii_case("UPDATE") {
        return false;
    }
    if let (Some(from_state), Some(to_state)) = (
        normalize_state(event.from_state.as_deref()),
        normalize_state(event.to_state.as_deref()),
    ) {
        return from_state == to_state;
    }

    let before = parse_json_value(&event.row_before_json);
    let after = parse_json_value(&event.row_after_json);
    match (
        inferred_domain_state_for_preserving_update(event, &before),
        inferred_domain_state_for_preserving_update(event, &after),
    ) {
        (Some(from_state), Some(to_state)) => from_state == to_state,
        _ => false,
    }
}

fn inferred_domain_state_for_preserving_update(
    event: &ProcessEventForStateMachine,
    row: &Value,
) -> Option<csm::CoreState> {
    if event.table_name == "communication_routing_state"
        || event.table_name.contains("queue")
        || event.entity_type.eq_ignore_ascii_case("queue")
    {
        return json_string(row, &["route_status", "queue_status", "status", "state"])
            .and_then(|value| map_queue_state(Some(&value)));
    }
    if event.table_name.starts_with("communication_") || event.table_name.contains("mail") {
        return communication_state_from_row(event, row).or_else(|| {
            json_string(row, &["status", "state"])
                .and_then(|value| map_communication_state(Some(&value)))
        });
    }
    if event.table_name.contains("ticket") || event.table_name.contains("work_item") {
        return json_string(row, &["status", "state"])
            .and_then(|value| map_ticket_state_for_event(event, Some(&value)));
    }
    if event.table_name.contains("commitment") {
        return json_string(row, &["status", "state"])
            .and_then(|value| map_commitment_state(Some(&value)));
    }
    if event.table_name.starts_with("scheduled_") || event.table_name.contains("schedule") {
        return json_string(row, &["status", "state", "enabled"])
            .and_then(|value| map_schedule_state(Some(&value)));
    }
    if event.table_name.contains("knowledge") {
        return json_string(row, &["status", "state"])
            .and_then(|value| map_knowledge_state(Some(&value)));
    }
    if event.table_name.contains("repair") || event.table_name.contains("health") {
        return json_string(row, &["status", "state"])
            .and_then(|value| map_repair_state(Some(&value)));
    }
    None
}

fn load_state_machine_events(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<ProcessEventForStateMachine>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT event_id, observed_at, case_id, activity, entity_type, entity_id,
               table_name, operation, from_state, to_state, row_before_json,
               row_after_json, command_name
        FROM ctox_process_events
        ORDER BY event_seq DESC
        LIMIT ?1
        "#,
    )?;
    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(ProcessEventForStateMachine {
                event_id: row.get(0)?,
                observed_at: row.get(1)?,
                case_id: row.get(2)?,
                activity: row.get(3)?,
                entity_type: row.get(4)?,
                entity_id: row.get(5)?,
                table_name: row.get(6)?,
                operation: row.get(7)?,
                from_state: row.get(8)?,
                to_state: row.get(9)?,
                row_before_json: row.get(10)?,
                row_after_json: row.get(11)?,
                command_name: row.get(12)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn infer_communication_transition(
    event: &ProcessEventForStateMachine,
    after: &Value,
    haystack: &str,
) -> Option<csm::CoreTransitionRequest> {
    let to_state = communication_state_from_row(event, after)
        .or_else(|| map_communication_state(event.to_state.as_deref()))
        .or_else(|| communication_state_from_activity(&event.activity))?;

    // Inbound-direction rows must not be classified as outgoing founder
    // sends. Production saw 109+ rejected `founder_send_body_hash_mismatch`
    // proofs per inbound mail because every UPDATE on a received mail was
    // routed into the FounderCommunication-Send rule and asked to honour an
    // approved-body-hash that never existed for an incoming message.  The
    // guard here filters at the inference layer instead of relying on every
    // downstream gate to reject silently.
    if matches!(to_state, csm::CoreState::Sending | csm::CoreState::Sent) {
        if let Some(direction) = after.get("direction").and_then(Value::as_str) {
            if !direction.eq_ignore_ascii_case("outbound") {
                return None;
            }
        }
    }
    let from_state =
        map_communication_state(event.from_state.as_deref()).unwrap_or_else(|| match to_state {
            csm::CoreState::InboundObserved => csm::CoreState::InboundObserved,
            csm::CoreState::ContextBuilt => csm::CoreState::InboundObserved,
            csm::CoreState::ReplyNeeded | csm::CoreState::NoResponseNeeded => {
                csm::CoreState::ContextBuilt
            }
            csm::CoreState::Drafting => csm::CoreState::ReplyNeeded,
            csm::CoreState::DraftReady => csm::CoreState::Drafting,
            csm::CoreState::Reviewing => csm::CoreState::DraftReady,
            csm::CoreState::Approved | csm::CoreState::ReworkRequired => csm::CoreState::Reviewing,
            csm::CoreState::Sending | csm::CoreState::Sent => csm::CoreState::Approved,
            csm::CoreState::SendFailed => csm::CoreState::Sending,
            csm::CoreState::DeliveryRepair => csm::CoreState::SendFailed,
            csm::CoreState::AwaitingAcknowledgement => csm::CoreState::Sent,
            csm::CoreState::Done => csm::CoreState::AwaitingAcknowledgement,
            csm::CoreState::Escalated => csm::CoreState::ReplyNeeded,
            _ => csm::CoreState::InboundObserved,
        });
    let core_event = match to_state {
        csm::CoreState::ContextBuilt => csm::CoreEvent::BuildContext,
        csm::CoreState::ReplyNeeded => csm::CoreEvent::BuildContext,
        csm::CoreState::NoResponseNeeded => csm::CoreEvent::DecideNoResponseNeeded,
        csm::CoreState::Drafting => csm::CoreEvent::DraftReply,
        csm::CoreState::DraftReady => csm::CoreEvent::DraftReply,
        csm::CoreState::Reviewing => csm::CoreEvent::RequestReview,
        csm::CoreState::Approved => csm::CoreEvent::Approve,
        csm::CoreState::ReworkRequired => csm::CoreEvent::RequireRework,
        csm::CoreState::Sending | csm::CoreState::Sent => csm::CoreEvent::Send,
        csm::CoreState::SendFailed => csm::CoreEvent::Fail,
        csm::CoreState::DeliveryRepair => csm::CoreEvent::StartRepair,
        csm::CoreState::AwaitingAcknowledgement => csm::CoreEvent::ConfirmDelivery,
        csm::CoreState::Done => csm::CoreEvent::ConfirmDelivery,
        csm::CoreState::Escalated => csm::CoreEvent::Escalate,
        _ => csm::CoreEvent::ObserveInbound,
    };
    let mut metadata = common_metadata(event);
    metadata.insert("protected_party".to_string(), "founder".to_string());
    if owner_visible_text(haystack) {
        metadata.insert("owner_visible_completion".to_string(), "true".to_string());
    }
    Some(csm::CoreTransitionRequest {
        entity_type: csm::CoreEntityType::FounderCommunication,
        entity_id: stable_entity_id(event),
        lane: csm::RuntimeLane::P0FounderCommunication,
        from_state,
        to_state,
        event: core_event,
        actor: actor_from_event(event),
        evidence: evidence_from_row(after),
        metadata,
    })
}

fn infer_queue_transition(
    event: &ProcessEventForStateMachine,
    before: &Value,
    after: &Value,
    haystack: &str,
) -> Option<csm::CoreTransitionRequest> {
    let after_state = json_string(after, &["route_status", "queue_status", "status", "state"]);
    let before_state = json_string(before, &["route_status", "queue_status", "status", "state"]);
    let to_state = map_queue_state(event.to_state.as_deref())
        .or_else(|| map_queue_state(after_state.as_deref()))?;
    let from_state = map_queue_state(event.from_state.as_deref())
        .or_else(|| map_queue_state(before_state.as_deref()))
        .unwrap_or(match to_state {
            csm::CoreState::Pending => csm::CoreState::Leased,
            csm::CoreState::Leased => csm::CoreState::Pending,
            csm::CoreState::Running => csm::CoreState::Leased,
            csm::CoreState::Completed | csm::CoreState::Blocked | csm::CoreState::Failed => {
                csm::CoreState::Running
            }
            csm::CoreState::Superseded => csm::CoreState::Pending,
            _ => csm::CoreState::Pending,
        });
    let core_event = match to_state {
        csm::CoreState::Leased => csm::CoreEvent::Lease,
        csm::CoreState::Pending if from_state == csm::CoreState::Leased => csm::CoreEvent::Release,
        csm::CoreState::Pending => csm::CoreEvent::Retry,
        csm::CoreState::Running => csm::CoreEvent::Execute,
        csm::CoreState::Completed => csm::CoreEvent::Complete,
        csm::CoreState::Blocked => csm::CoreEvent::Block,
        csm::CoreState::Failed => csm::CoreEvent::Fail,
        csm::CoreState::Superseded => csm::CoreEvent::Supersede,
        _ => csm::CoreEvent::Execute,
    };
    let mut metadata = common_metadata(event);
    if founder_text(haystack) {
        metadata.insert("protected_party".to_string(), "founder".to_string());
    }
    Some(csm::CoreTransitionRequest {
        entity_type: csm::CoreEntityType::QueueItem,
        entity_id: stable_entity_id(event),
        lane: if founder_text(haystack) {
            csm::RuntimeLane::P0FounderCommunication
        } else {
            csm::RuntimeLane::P2MissionDelivery
        },
        from_state,
        to_state,
        event: core_event,
        actor: actor_from_event(event),
        evidence: evidence_from_row(after),
        metadata,
    })
}

fn infer_ticket_transition(
    event: &ProcessEventForStateMachine,
    after: &Value,
    haystack: &str,
) -> Option<csm::CoreTransitionRequest> {
    let to_state = map_ticket_state_for_event(event, event.to_state.as_deref())?;
    let from_state =
        map_ticket_state_for_event(event, event.from_state.as_deref()).unwrap_or(match to_state {
            csm::CoreState::Closed => csm::CoreState::Verified,
            csm::CoreState::Superseded => csm::CoreState::Created,
            csm::CoreState::Verified => csm::CoreState::AwaitingVerification,
            csm::CoreState::AwaitingVerification => csm::CoreState::AwaitingReview,
            csm::CoreState::AwaitingReview => csm::CoreState::Executing,
            csm::CoreState::Executing => csm::CoreState::Planned,
            _ => csm::CoreState::Created,
        });
    let core_event = match to_state {
        csm::CoreState::Classified => csm::CoreEvent::Classify,
        csm::CoreState::TicketBacked => csm::CoreEvent::CreateTicket,
        csm::CoreState::Planned => csm::CoreEvent::Plan,
        csm::CoreState::Executing => csm::CoreEvent::Execute,
        csm::CoreState::AwaitingReview => csm::CoreEvent::RequestReview,
        csm::CoreState::ReworkRequired => csm::CoreEvent::RequireRework,
        csm::CoreState::AwaitingVerification => csm::CoreEvent::Approve,
        csm::CoreState::Verified => csm::CoreEvent::Verify,
        csm::CoreState::Closed => csm::CoreEvent::Close,
        csm::CoreState::Blocked => csm::CoreEvent::Block,
        csm::CoreState::Superseded => csm::CoreEvent::Supersede,
        _ => csm::CoreEvent::Execute,
    };
    let mut metadata = common_metadata(event);
    if ticket_event_can_mark_owner_visible(event) && owner_visible_text(haystack) {
        metadata.insert("owner_visible_completion".to_string(), "true".to_string());
    }
    Some(csm::CoreTransitionRequest {
        entity_type: if event.table_name.contains("work") {
            csm::CoreEntityType::WorkItem
        } else {
            csm::CoreEntityType::Ticket
        },
        entity_id: stable_entity_id(event),
        lane: csm::RuntimeLane::P2MissionDelivery,
        from_state,
        to_state,
        event: core_event,
        actor: actor_from_event(event),
        evidence: evidence_from_row(after),
        metadata,
    })
}

fn infer_commitment_transition(
    event: &ProcessEventForStateMachine,
    after: &Value,
) -> Option<csm::CoreTransitionRequest> {
    let to_state = map_commitment_state(event.to_state.as_deref())?;
    let from_state = map_commitment_state(event.from_state.as_deref()).unwrap_or(match to_state {
        csm::CoreState::Reviewed => csm::CoreState::Proposed,
        csm::CoreState::Committed => csm::CoreState::Reviewed,
        csm::CoreState::BackingScheduled => csm::CoreState::Committed,
        csm::CoreState::DueSoon => csm::CoreState::BackingScheduled,
        csm::CoreState::InProgress => csm::CoreState::DueSoon,
        csm::CoreState::Delivered => csm::CoreState::InProgress,
        _ => csm::CoreState::Proposed,
    });
    let core_event = match to_state {
        csm::CoreState::Reviewed => csm::CoreEvent::Approve,
        csm::CoreState::Committed => csm::CoreEvent::Commit,
        csm::CoreState::BackingScheduled => csm::CoreEvent::ScheduleBackingTask,
        csm::CoreState::DueSoon => csm::CoreEvent::MarkDueSoon,
        csm::CoreState::InProgress => csm::CoreEvent::Execute,
        csm::CoreState::Delivered => csm::CoreEvent::Deliver,
        csm::CoreState::AtRisk => csm::CoreEvent::MarkAtRisk,
        csm::CoreState::Escalated => csm::CoreEvent::Escalate,
        csm::CoreState::CancelledWithNotice => csm::CoreEvent::CancelWithNotice,
        _ => csm::CoreEvent::ProposeCommitment,
    };
    Some(csm::CoreTransitionRequest {
        entity_type: csm::CoreEntityType::Commitment,
        entity_id: stable_entity_id(event),
        lane: csm::RuntimeLane::P0CommitmentBacking,
        from_state,
        to_state,
        event: core_event,
        actor: actor_from_event(event),
        evidence: evidence_from_row(after),
        metadata: common_metadata(event),
    })
}

fn infer_schedule_transition(
    event: &ProcessEventForStateMachine,
    after: &Value,
) -> Option<csm::CoreTransitionRequest> {
    let to_state = map_schedule_state(event.to_state.as_deref())?;
    let from_state = map_schedule_state(event.from_state.as_deref()).unwrap_or(match to_state {
        csm::CoreState::Enabled => csm::CoreState::Created,
        csm::CoreState::Due => csm::CoreState::Enabled,
        csm::CoreState::Emitted => csm::CoreState::Due,
        csm::CoreState::BackingWorkQueued => csm::CoreState::Emitted,
        csm::CoreState::Acknowledged => csm::CoreState::BackingWorkQueued,
        csm::CoreState::Paused | csm::CoreState::Expired | csm::CoreState::DisabledByPolicy => {
            csm::CoreState::Enabled
        }
        _ => csm::CoreState::Created,
    });
    let core_event = match to_state {
        csm::CoreState::Enabled => csm::CoreEvent::EnableSchedule,
        csm::CoreState::Due => csm::CoreEvent::MarkDueSoon,
        csm::CoreState::Emitted => csm::CoreEvent::EmitSchedule,
        csm::CoreState::BackingWorkQueued => csm::CoreEvent::ScheduleBackingTask,
        csm::CoreState::Acknowledged => csm::CoreEvent::AcknowledgeSchedule,
        csm::CoreState::Paused => csm::CoreEvent::PauseSchedule,
        csm::CoreState::Expired => csm::CoreEvent::ExpireSchedule,
        csm::CoreState::DisabledByPolicy => csm::CoreEvent::DisableSchedule,
        _ => csm::CoreEvent::EnableSchedule,
    };
    let mut metadata = common_metadata(event);
    if json_bool(
        after,
        &[
            "backs_commitment",
            "commitment_backing",
            "is_commitment_backing",
        ],
    ) {
        metadata.insert("backs_commitment".to_string(), "true".to_string());
    }
    Some(csm::CoreTransitionRequest {
        entity_type: csm::CoreEntityType::Schedule,
        entity_id: stable_entity_id(event),
        lane: csm::RuntimeLane::P0CommitmentBacking,
        from_state,
        to_state,
        event: core_event,
        actor: actor_from_event(event),
        evidence: evidence_from_row(after),
        metadata,
    })
}

fn infer_repair_transition(
    event: &ProcessEventForStateMachine,
    after: &Value,
) -> Option<csm::CoreTransitionRequest> {
    let to_state = map_repair_state(event.to_state.as_deref())?;
    let from_state = map_repair_state(event.from_state.as_deref()).unwrap_or(match to_state {
        csm::CoreState::PressureDetected => csm::CoreState::Healthy,
        csm::CoreState::RepairPlanning => csm::CoreState::PressureDetected,
        csm::CoreState::RepairPlanReviewed => csm::CoreState::RepairPlanning,
        csm::CoreState::ApplyingDeterministicActions => csm::CoreState::RepairPlanReviewed,
        csm::CoreState::RepairVerification => csm::CoreState::ApplyingDeterministicActions,
        csm::CoreState::Restored | csm::CoreState::StillDegraded => {
            csm::CoreState::RepairVerification
        }
        _ => csm::CoreState::Healthy,
    });
    Some(csm::CoreTransitionRequest {
        entity_type: csm::CoreEntityType::Repair,
        entity_id: stable_entity_id(event),
        lane: csm::RuntimeLane::P1QueueRepair,
        from_state,
        to_state,
        event: match to_state {
            csm::CoreState::PressureDetected => csm::CoreEvent::DetectPressure,
            csm::CoreState::RepairPlanning => csm::CoreEvent::PlanRepair,
            csm::CoreState::RepairPlanReviewed => csm::CoreEvent::ReviewRepairPlan,
            csm::CoreState::ApplyingDeterministicActions => csm::CoreEvent::ApplyRepairActions,
            csm::CoreState::RepairVerification => csm::CoreEvent::VerifyRepair,
            csm::CoreState::Restored => csm::CoreEvent::MarkRestored,
            _ => csm::CoreEvent::PlanRepair,
        },
        actor: actor_from_event(event),
        evidence: evidence_from_row(after),
        metadata: common_metadata(event),
    })
}

fn infer_knowledge_transition(
    event: &ProcessEventForStateMachine,
    after: &Value,
) -> Option<csm::CoreTransitionRequest> {
    let to_state = map_knowledge_state(event.to_state.as_deref())?;
    let from_state = map_knowledge_state(event.from_state.as_deref()).unwrap_or(match to_state {
        csm::CoreState::LessonDrafted => csm::CoreState::IncidentObserved,
        csm::CoreState::AwaitingReview => csm::CoreState::LessonDrafted,
        csm::CoreState::EvidenceAttached => csm::CoreState::AwaitingReview,
        csm::CoreState::Active => csm::CoreState::EvidenceAttached,
        csm::CoreState::Superseded => csm::CoreState::Active,
        _ => csm::CoreState::IncidentObserved,
    });
    Some(csm::CoreTransitionRequest {
        entity_type: csm::CoreEntityType::Knowledge,
        entity_id: stable_entity_id(event),
        lane: csm::RuntimeLane::P1RuntimeSafety,
        from_state,
        to_state,
        event: match to_state {
            csm::CoreState::LessonDrafted => csm::CoreEvent::DraftLesson,
            csm::CoreState::AwaitingReview => csm::CoreEvent::RequestReview,
            csm::CoreState::EvidenceAttached => csm::CoreEvent::AttachEvidence,
            csm::CoreState::Active => csm::CoreEvent::ActivateKnowledge,
            csm::CoreState::Superseded => csm::CoreEvent::Supersede,
            _ => csm::CoreEvent::CaptureIncident,
        },
        actor: actor_from_event(event),
        evidence: evidence_from_row(after),
        metadata: common_metadata(event),
    })
}

fn map_communication_state(raw: Option<&str>) -> Option<csm::CoreState> {
    match normalize_state(raw).as_deref()? {
        "inbound" | "inbound_observed" | "observed" | "received" | "receive" | "inbox" => {
            Some(csm::CoreState::InboundObserved)
        }
        "context" | "context_built" => Some(csm::CoreState::ContextBuilt),
        "reply_needed" | "needs_reply" | "pending_reply" => Some(csm::CoreState::ReplyNeeded),
        "no_response_needed" | "no_reply_needed" => Some(csm::CoreState::NoResponseNeeded),
        "draft" | "drafting" => Some(csm::CoreState::Drafting),
        "draft_ready" | "ready_for_review" => Some(csm::CoreState::DraftReady),
        "review" | "reviewing" | "under_review" => Some(csm::CoreState::Reviewing),
        "approved" | "reviewed" => Some(csm::CoreState::Approved),
        "rework" | "rework_required" | "sent_back_for_rework" => {
            Some(csm::CoreState::ReworkRequired)
        }
        "sending" | "queued_to_send" | "outbox" => Some(csm::CoreState::Sending),
        "sent" | "delivered" => Some(csm::CoreState::Sent),
        "send_failed" | "failed" => Some(csm::CoreState::SendFailed),
        "delivery_repair" => Some(csm::CoreState::DeliveryRepair),
        "awaiting_acknowledgement" | "awaiting_ack" => {
            Some(csm::CoreState::AwaitingAcknowledgement)
        }
        "done" | "handled" | "completed" | "closed" => Some(csm::CoreState::Done),
        "escalated" => Some(csm::CoreState::Escalated),
        _ => None,
    }
}

fn communication_state_from_row(
    event: &ProcessEventForStateMachine,
    row: &Value,
) -> Option<csm::CoreState> {
    let direction = json_string(row, &["direction"]).map(|value| normalize_text(&value));
    let folder =
        json_string(row, &["folder_hint", "folder", "mailbox"]).map(|value| normalize_text(&value));
    let message_key = json_string(row, &["message_key", "mailbox_key", "external_key"])
        .map(|value| normalize_text(&value));
    let status = json_string(row, &["status", "state", "route_status", "delivery_status"])
        .and_then(|value| map_communication_state(Some(&value)));

    if matches!(direction.as_deref(), Some("inbound"))
        || matches!(folder.as_deref(), Some("inbox"))
        || message_key
            .as_deref()
            .is_some_and(|value| value.contains("::inbox::"))
    {
        return Some(csm::CoreState::InboundObserved);
    }

    if matches!(direction.as_deref(), Some("outbound")) {
        if matches!(folder.as_deref(), Some("sent") | Some("sent_mail")) {
            return Some(csm::CoreState::Sent);
        }
        if matches!(
            folder.as_deref(),
            Some("outbox") | Some("queued") | Some("send_queue")
        ) {
            return Some(csm::CoreState::Sending);
        }
        if let Some(status) = status {
            return Some(status);
        }
    }

    if event.table_name.contains("communication_messages")
        && matches!(
            event.to_state.as_deref().map(normalize_text).as_deref(),
            Some("received") | Some("inbound") | Some("inbox")
        )
    {
        return Some(csm::CoreState::InboundObserved);
    }

    None
}

fn communication_state_from_activity(activity: &str) -> Option<csm::CoreState> {
    let activity = activity.to_ascii_lowercase();
    if activity.contains("send") {
        Some(csm::CoreState::Sending)
    } else if activity.contains("sent") {
        Some(csm::CoreState::Sent)
    } else if activity.contains("review") {
        Some(csm::CoreState::Reviewing)
    } else if activity.contains("draft") {
        Some(csm::CoreState::Drafting)
    } else {
        None
    }
}

fn map_queue_state(raw: Option<&str>) -> Option<csm::CoreState> {
    match normalize_state(raw).as_deref()? {
        "pending" | "queued" | "ready" => Some(csm::CoreState::Pending),
        "leased" | "claimed" => Some(csm::CoreState::Leased),
        "running" | "processing" | "active" => Some(csm::CoreState::Running),
        "blocked" | "stuck" => Some(csm::CoreState::Blocked),
        "failed" | "error" => Some(csm::CoreState::Failed),
        "completed" | "done" | "handled" => Some(csm::CoreState::Completed),
        "superseded" | "cancelled" | "canceled" => Some(csm::CoreState::Superseded),
        _ => None,
    }
}

fn map_ticket_state(raw: Option<&str>) -> Option<csm::CoreState> {
    match normalize_state(raw).as_deref()? {
        "created" | "open" | "queued" => Some(csm::CoreState::Created),
        "classified" => Some(csm::CoreState::Classified),
        "ticket_backed" => Some(csm::CoreState::TicketBacked),
        "planned" | "ready" | "publishing" | "published" => Some(csm::CoreState::Planned),
        "executing" | "in_progress" | "running" => Some(csm::CoreState::Executing),
        "awaiting_review" | "review" | "reviewing" => Some(csm::CoreState::AwaitingReview),
        "rework_required" | "rework" => Some(csm::CoreState::ReworkRequired),
        "awaiting_verification" | "verification" => Some(csm::CoreState::AwaitingVerification),
        "verified" | "writeback_pending" => Some(csm::CoreState::Verified),
        "closed" | "done" | "completed" => Some(csm::CoreState::Closed),
        "superseded" | "cancelled" | "canceled" => Some(csm::CoreState::Superseded),
        "blocked" => Some(csm::CoreState::Blocked),
        _ => None,
    }
}

fn ticket_event_can_mark_owner_visible(event: &ProcessEventForStateMachine) -> bool {
    matches!(
        event.table_name.as_str(),
        "ticket_cases" | "ticket_case_events" | "communication_messages"
    )
}

fn map_ticket_state_for_event(
    event: &ProcessEventForStateMachine,
    raw: Option<&str>,
) -> Option<csm::CoreState> {
    let normalized = normalize_state(raw)?;
    if event.table_name == "ticket_self_work_items"
        && matches!(
            normalized.as_str(),
            "queued" | "publishing" | "published" | "ready"
        )
    {
        return Some(csm::CoreState::Planned);
    }
    map_ticket_state(Some(&normalized))
}

fn map_commitment_state(raw: Option<&str>) -> Option<csm::CoreState> {
    match normalize_state(raw).as_deref()? {
        "proposed" => Some(csm::CoreState::Proposed),
        "reviewed" => Some(csm::CoreState::Reviewed),
        "committed" | "active" => Some(csm::CoreState::Committed),
        "backing_scheduled" | "scheduled" => Some(csm::CoreState::BackingScheduled),
        "due_soon" | "due" => Some(csm::CoreState::DueSoon),
        "in_progress" | "running" => Some(csm::CoreState::InProgress),
        "delivered" | "done" | "completed" => Some(csm::CoreState::Delivered),
        "at_risk" | "late" => Some(csm::CoreState::AtRisk),
        "escalated" => Some(csm::CoreState::Escalated),
        "cancelled_with_notice" | "canceled_with_notice" | "cancelled" | "canceled" => {
            Some(csm::CoreState::CancelledWithNotice)
        }
        _ => None,
    }
}

fn map_schedule_state(raw: Option<&str>) -> Option<csm::CoreState> {
    match normalize_state(raw).as_deref()? {
        "created" => Some(csm::CoreState::Created),
        "enabled" | "active" | "1" | "true" | "yes" | "y" => Some(csm::CoreState::Enabled),
        "due" => Some(csm::CoreState::Due),
        "emitted" | "fired" => Some(csm::CoreState::Emitted),
        "backing_work_queued" | "queued" => Some(csm::CoreState::BackingWorkQueued),
        "acknowledged" | "ack" => Some(csm::CoreState::Acknowledged),
        "paused" => Some(csm::CoreState::Paused),
        "expired" => Some(csm::CoreState::Expired),
        "disabled_by_policy" | "disabled" | "0" | "false" | "no" | "n" => {
            Some(csm::CoreState::DisabledByPolicy)
        }
        _ => None,
    }
}

fn map_repair_state(raw: Option<&str>) -> Option<csm::CoreState> {
    match normalize_state(raw).as_deref()? {
        "healthy" => Some(csm::CoreState::Healthy),
        "pressure_detected" | "pressure" => Some(csm::CoreState::PressureDetected),
        "repair_planning" | "planning" => Some(csm::CoreState::RepairPlanning),
        "repair_plan_reviewed" | "plan_reviewed" => Some(csm::CoreState::RepairPlanReviewed),
        "applying_deterministic_actions" | "applying" => {
            Some(csm::CoreState::ApplyingDeterministicActions)
        }
        "repair_verification" | "verifying" => Some(csm::CoreState::RepairVerification),
        "restored" => Some(csm::CoreState::Restored),
        "still_degraded" => Some(csm::CoreState::StillDegraded),
        _ => None,
    }
}

fn map_knowledge_state(raw: Option<&str>) -> Option<csm::CoreState> {
    match normalize_state(raw).as_deref()? {
        "incident_observed" | "incident" => Some(csm::CoreState::IncidentObserved),
        "lesson_drafted" | "drafted" => Some(csm::CoreState::LessonDrafted),
        "awaiting_review" | "review" | "reviewing" => Some(csm::CoreState::AwaitingReview),
        "evidence_attached" | "evidence" => Some(csm::CoreState::EvidenceAttached),
        "active" | "published" => Some(csm::CoreState::Active),
        "superseded" => Some(csm::CoreState::Superseded),
        _ => None,
    }
}

fn evidence_from_row(row: &Value) -> csm::CoreEvidenceRefs {
    let body = json_string(
        row,
        &["body", "body_text", "message", "message_body", "content"],
    );
    let outgoing_body_sha256 = json_string(
        row,
        &[
            "outgoing_body_sha256",
            "message_body_sha256",
            "body_sha256",
            "content_sha256",
        ],
    )
    .or_else(|| body.as_deref().map(full_sha256_hex));
    let approved_body_sha256 = json_string(
        row,
        &[
            "approved_body_sha256",
            "reviewed_body_sha256",
            "approved_message_body_sha256",
            "body_sha256",
        ],
    );
    let outgoing_recipient_set_sha256 = json_string(
        row,
        &[
            "outgoing_recipient_set_sha256",
            "recipient_set_sha256",
            "recipients_sha256",
            "to_cc_bcc_sha256",
        ],
    )
    .or_else(|| {
        json_string(row, &["to", "recipients", "recipient", "email"])
            .map(|value| full_sha256_hex(&value))
    });
    let approved_recipient_set_sha256 = json_string(
        row,
        &[
            "approved_recipient_set_sha256",
            "reviewed_recipient_set_sha256",
            "approved_recipients_sha256",
            "recipient_set_sha256",
        ],
    );
    csm::CoreEvidenceRefs {
        review_audit_key: json_string(
            row,
            &[
                "review_audit_key",
                "review_audit_id",
                "review_id",
                "approval_id",
                "approval_key",
            ],
        ),
        approved_body_sha256,
        outgoing_body_sha256,
        approved_recipient_set_sha256,
        outgoing_recipient_set_sha256,
        verification_id: json_string(
            row,
            &[
                "verification_id",
                "verification_key",
                "verified_by",
                "test_run_id",
                "evidence_id",
            ],
        ),
        schedule_task_id: json_string(
            row,
            &[
                "schedule_task_id",
                "scheduled_task_id",
                "backing_schedule_task_id",
                "cron_id",
            ],
        ),
        replacement_schedule_task_id: json_string(row, &["replacement_schedule_task_id"]),
        escalation_id: json_string(row, &["escalation_id"]),
        knowledge_entry_id: json_string(row, &["knowledge_entry_id", "knowledge_id"]),
        incident_id: json_string(row, &["incident_id", "root_incident_id"]),
        canonical_hot_path: json_string_array(row, &["canonical_hot_path", "hot_path"]),
        ..csm::CoreEvidenceRefs::default()
    }
}

fn common_metadata(event: &ProcessEventForStateMachine) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert("event_id".to_string(), event.event_id.clone());
    metadata.insert("case_id".to_string(), event.case_id.clone());
    metadata.insert("activity".to_string(), event.activity.clone());
    metadata.insert("table_name".to_string(), event.table_name.clone());
    metadata.insert("operation".to_string(), event.operation.clone());
    if let Some(command_name) = &event.command_name {
        metadata.insert("command_name".to_string(), command_name.clone());
    }
    metadata
}

fn stable_entity_id(event: &ProcessEventForStateMachine) -> String {
    if event.entity_id.trim().is_empty() || event.entity_id == "{}" {
        event.case_id.clone()
    } else {
        format!("{}:{}", event.table_name, event.entity_id)
    }
}

fn actor_from_event(event: &ProcessEventForStateMachine) -> String {
    event
        .command_name
        .clone()
        .unwrap_or_else(|| "ctox-runtime".to_string())
}

fn parse_json_value(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or(Value::Null)
}

fn event_haystack(event: &ProcessEventForStateMachine, before: &Value, after: &Value) -> String {
    format!(
        "{} {} {} {} {} {} {} {}",
        event.activity,
        event.entity_type,
        event.entity_id,
        event.table_name,
        event.operation,
        event.command_name.as_deref().unwrap_or_default(),
        before,
        after,
    )
    .to_ascii_lowercase()
}

fn normalize_state(raw: Option<&str>) -> Option<String> {
    let value = raw?.trim();
    if value.is_empty() || value == "row_present" {
        return None;
    }
    Some(normalize_text(value))
}

fn normalize_text(value: &str) -> String {
    value
        .trim()
        .replace('-', "_")
        .replace(' ', "_")
        .to_ascii_lowercase()
}

fn json_string(row: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = row.get(*key).and_then(json_value_to_string) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn json_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(value) => Some(
            value
                .iter()
                .filter_map(json_value_to_string)
                .collect::<Vec<_>>()
                .join(","),
        ),
        _ => None,
    }
}

fn json_bool(row: &Value, keys: &[&str]) -> bool {
    keys.iter().any(|key| match row.get(*key) {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_i64().unwrap_or_default() != 0,
        Some(Value::String(value)) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "y"
        ),
        _ => false,
    })
}

fn json_string_array(row: &Value, keys: &[&str]) -> Vec<String> {
    for key in keys {
        match row.get(*key) {
            Some(Value::Array(values)) => {
                return values
                    .iter()
                    .filter_map(json_value_to_string)
                    .filter(|value| !value.trim().is_empty())
                    .collect();
            }
            Some(Value::String(value)) if !value.trim().is_empty() => {
                return value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .collect();
            }
            _ => {}
        }
    }
    Vec::new()
}

fn full_sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn founder_text(haystack: &str) -> bool {
    has_domain_token(
        haystack,
        &[
            "founder", "owner", "admin", "ceo", "michael", "olaf", "marco",
        ],
    )
}

fn has_domain_token(haystack: &str, needles: &[&str]) -> bool {
    haystack
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|token| !token.is_empty())
        .any(|token| needles.iter().any(|needle| token == *needle))
}

fn owner_visible_text(haystack: &str) -> bool {
    founder_text(haystack)
        || haystack.contains("external")
        || haystack.contains("mail")
        || haystack.contains("email")
        || haystack.contains("customer")
}

fn core_violation_severity(request: &csm::CoreTransitionRequest, code: &str) -> &'static str {
    if request.entity_type == csm::CoreEntityType::FounderCommunication
        || request.lane == csm::RuntimeLane::P0FounderCommunication
        || code.contains("founder")
        || code.contains("commitment")
    {
        "critical"
    } else {
        "warning"
    }
}

#[cfg(test)]
mod tests {
    // ctox-allow-direct-state-write: test fixture module
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn process_mining_triggers_record_table_mutations() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE ticket_self_work_items (
                work_id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                state TEXT NOT NULL
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        activate_command_context(
            &conn, "turn-1", "cmd-1", "agent-1", "test", "ticket", "argv",
        )?;
        conn.execute(
            "INSERT INTO ticket_self_work_items (work_id, title, state) VALUES ('w1', 'Test', 'queued')",
            [],
        )?;
        conn.execute(
            "UPDATE ticket_self_work_items SET state = 'closed' WHERE work_id = 'w1'",
            [],
        )?;

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM ctox_process_events", [], |row| {
            row.get(0)
        })?;
        let transition: (String, Option<String>, Option<String>, Option<String>) = conn.query_row(
            "SELECT activity, from_state, to_state, command_id FROM ctox_process_events WHERE operation = 'UPDATE'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;

        assert_eq!(count, 2);
        assert_eq!(transition.0, "ticket_self_work_items.UPDATE");
        assert_eq!(transition.1.as_deref(), Some("queued"));
        assert_eq!(transition.2.as_deref(), Some("closed"));
        assert_eq!(transition.3.as_deref(), Some("cmd-1"));
        Ok(())
    }

    #[test]
    fn process_mining_redacts_secret_rows() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE ctox_secret_records (
                key TEXT PRIMARY KEY,
                secret_value TEXT NOT NULL,
                status TEXT NOT NULL
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            "INSERT INTO ctox_secret_records (key, secret_value, status) VALUES ('openai', 'secret', 'active')",
            [],
        )?;

        let row_after: String = conn.query_row(
            "SELECT row_after_json FROM ctox_process_events WHERE table_name = 'ctox_secret_records'",
            [],
            |row| row.get(0),
        )?;
        assert!(row_after.contains("_redacted"));
        assert!(!row_after.contains("secret_value"));
        assert!(!row_after.contains("secret"));
        Ok(())
    }

    #[test]
    fn sqlite_authorizer_skips_read_events_by_default() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE knowledge_notes (
                note_id TEXT PRIMARY KEY,
                body TEXT NOT NULL,
                status TEXT NOT NULL
            );
            INSERT INTO knowledge_notes (note_id, body, status)
            VALUES ('n1', 'important', 'active');
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        activate_command_context(
            &conn,
            "turn-read",
            "cmd-read",
            "agent-read",
            "test",
            "knowledge",
            "argv",
        )?;
        attach_sqlite_access_recorder(&conn, &db_path);

        let _: String = conn.query_row(
            "SELECT body FROM knowledge_notes WHERE note_id = 'n1'",
            [],
            |row| row.get(0),
        )?;
        let flushed = flush_sqlite_access_events(&conn, &db_path, "cmd-read")?;

        let read_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_process_events WHERE table_name = 'knowledge_notes' AND operation = 'READ'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(flushed, 0);
        assert_eq!(read_count, 0);
        Ok(())
    }

    #[test]
    fn sqlite_access_prune_keeps_recent_debug_window() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        ensure_process_mining_schema(&conn, &db_path)?;
        for idx in 0..5 {
            conn.execute(
                r#"
                INSERT INTO ctox_process_events (
                    event_id, observed_at, case_id, activity, lifecycle_transition,
                    entity_type, entity_id, table_name, operation, from_state, to_state,
                    primary_key_json, row_before_json, row_after_json, changed_columns_json,
                    turn_id, command_id, actor_key, source, command_name, db_path, metadata_json
                )
                VALUES (
                    ?1, ?2, 'sqlite-access:/tmp/test:knowledge_notes', 'knowledge_notes.READ',
                    'access', 'knowledge', ?3, 'knowledge_notes', 'READ', NULL, NULL,
                    json_object(), json_object(), json_object(), json_array(),
                    'turn', 'cmd', 'agent', 'test', 'read', ?4, json_object()
                )
                "#,
                params![
                    format!("sqlite-access-test-{idx}"),
                    format!("2026-05-01T00:00:0{idx}Z"),
                    format!("entity-{idx}"),
                    db_path.to_string_lossy().to_string(),
                ],
            )?;
        }
        let deleted = prune_sqlite_access_process_events(&conn, 2)?;
        let remaining: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_process_events WHERE case_id LIKE 'sqlite-access:%'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(deleted, 3);
        assert_eq!(remaining, 2);
        Ok(())
    }

    #[test]
    fn process_mining_schema_exposes_state_machine_surfaces() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE ticket_self_work_items (
                work_id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                state TEXT NOT NULL
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            "INSERT INTO ticket_self_work_items (work_id, title, state) VALUES ('w1', 'Test', 'queued')",
            [],
        )?;

        let projection_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_pm_case_events WHERE case_id = 'ticket_self_work_items:{\"work_id\":\"w1\"}'",
            [],
            |row| row.get(0),
        )?;
        let object_relation_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_pm_event_objects WHERE object_type = 'ticket'",
            [],
            |row| row.get(0),
        )?;
        let live_table_count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type = 'table'
              AND name IN (
                'ctox_pm_state_violations',
                'ctox_pm_core_transition_audit',
                'ctox_pm_core_transition_rules',
                'ctox_pm_event_transition_coverage',
                'ctox_pm_unmapped_events'
              )
            "#,
            [],
            |row| row.get(0),
        )?;
        let legacy_rust4pm_table_count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type = 'table'
              AND name IN (
                'ctox_pm_process_models',
                'ctox_pm_dfg_activities',
                'ctox_pm_dfg_edges',
                'ctox_pm_petri_places',
                'ctox_pm_petri_transitions',
                'ctox_pm_petri_arcs',
                'ctox_pm_petri_markings',
                'ctox_pm_conformance_runs',
                'ctox_pm_case_classifiers'
              )
            "#,
            [],
            |row| row.get(0),
        )?;
        let instrumented_pm_table_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_process_trigger_registry WHERE table_name LIKE 'ctox_pm_%'",
            [],
            |row| row.get(0),
        )?;
        let proof_table_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'ctox_core_transition_proofs'",
            [],
            |row| row.get(0),
        )?;

        assert_eq!(projection_count, 1);
        assert_eq!(object_relation_count, 1);
        assert_eq!(live_table_count, 5);
        assert_eq!(legacy_rust4pm_table_count, 0);
        assert_eq!(instrumented_pm_table_count, 0);
        assert_eq!(proof_table_count, 1);
        Ok(())
    }

    #[test]
    fn ensure_schema_drops_legacy_rust4pm_tables() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE ctox_pm_process_models (
                model_id TEXT PRIMARY KEY,
                model_kind TEXT NOT NULL,
                algorithm TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE ctox_pm_dfg_edges (
                model_id TEXT NOT NULL,
                from_activity TEXT NOT NULL,
                to_activity TEXT NOT NULL,
                frequency INTEGER NOT NULL,
                PRIMARY KEY(model_id, from_activity, to_activity)
            );
            INSERT INTO ctox_pm_process_models VALUES ('m1','dfg','rust4pm','2026-01-01T00:00:00Z');
            INSERT INTO ctox_pm_dfg_edges VALUES ('m1','A','B',5);
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        let remaining: i64 = conn.query_row(
            r#"
            SELECT COUNT(*) FROM sqlite_master WHERE type='table'
              AND name IN ('ctox_pm_process_models','ctox_pm_dfg_edges')
            "#,
            [],
            |row| row.get(0),
        )?;
        assert_eq!(remaining, 0, "legacy rust4pm tables must be dropped");
        Ok(())
    }

    #[test]
    fn state_scan_blocks_founder_send_without_review_evidence() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE communication_founder_outbox (
                mail_id TEXT PRIMARY KEY,
                protected_party TEXT NOT NULL,
                status TEXT NOT NULL,
                body TEXT NOT NULL,
                recipients TEXT NOT NULL,
                review_audit_key TEXT,
                approved_body_sha256 TEXT,
                approved_recipient_set_sha256 TEXT
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            r#"
            INSERT INTO communication_founder_outbox (
                mail_id, protected_party, status, body, recipients
            )
            VALUES ('mail-1', 'founder', 'approved', 'Status update', 'michael@example.com')
            "#,
            [],
        )?;
        conn.execute(
            "UPDATE communication_founder_outbox SET status = 'sending' WHERE mail_id = 'mail-1'",
            [],
        )?;

        let summary = scan_core_state_machine_violations(&conn, 100)?;
        let rejected = summary
            .get("rejected")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let violation_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_pm_state_violations WHERE violation_code = 'founder_send_requires_review_audit'",
            [],
            |row| row.get(0),
        )?;
        let rejected_audits: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_pm_core_transition_audit WHERE accepted = 0",
            [],
            |row| row.get(0),
        )?;
        let rejected_proofs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_core_transition_proofs WHERE accepted = 0",
            [],
            |row| row.get(0),
        )?;

        assert!(rejected > 0, "{summary}");
        assert!(violation_count > 0);
        assert!(rejected_audits > 0);
        assert!(rejected_proofs > 0);
        Ok(())
    }

    #[test]
    fn state_scan_accepts_reviewed_founder_send_with_matching_hashes() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        let body = "Reviewed status update";
        let recipients = "michael@example.com";
        let body_sha = full_sha256_hex(body);
        let recipients_sha = full_sha256_hex(recipients);
        conn.execute_batch(
            r#"
            CREATE TABLE communication_founder_outbox (
                mail_id TEXT PRIMARY KEY,
                protected_party TEXT NOT NULL,
                status TEXT NOT NULL,
                body TEXT NOT NULL,
                recipients TEXT NOT NULL,
                review_audit_key TEXT,
                approved_body_sha256 TEXT,
                approved_recipient_set_sha256 TEXT
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            r#"
            INSERT INTO communication_founder_outbox (
                mail_id, protected_party, status, body, recipients,
                review_audit_key, approved_body_sha256, approved_recipient_set_sha256
            )
            VALUES ('mail-1', 'founder', 'approved', ?1, ?2, 'review-1', ?3, ?4)
            "#,
            params![body, recipients, body_sha, recipients_sha],
        )?;
        conn.execute(
            "UPDATE communication_founder_outbox SET status = 'sending' WHERE mail_id = 'mail-1'",
            [],
        )?;

        let summary = scan_core_state_machine_violations(&conn, 100)?;
        let rejected = summary
            .get("rejected")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let accepted_send_audits: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_core_transition_audit
            WHERE to_state = 'Sending' AND accepted = 1
            "#,
            [],
            |row| row.get(0),
        )?;
        let accepted_proofs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_core_transition_proofs WHERE accepted = 1",
            [],
            |row| row.get(0),
        )?;

        assert_eq!(rejected, 0, "{summary}");
        assert!(accepted_send_audits > 0);
        assert!(accepted_proofs > 0);
        Ok(())
    }

    #[test]
    fn state_scan_records_explicit_mapping_coverage() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE misc_runtime_notes (
                note_id TEXT PRIMARY KEY,
                status TEXT NOT NULL
            );
            CREATE TABLE ticket_self_work_items (
                work_id TEXT PRIMARY KEY,
                state TEXT NOT NULL,
                verification_id TEXT
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            "INSERT INTO misc_runtime_notes (note_id, status) VALUES ('n1', 'created')",
            [],
        )?;
        conn.execute(
            "INSERT INTO ticket_self_work_items (work_id, state, verification_id) VALUES ('w1', 'verified', 'verify-1')",
            [],
        )?;
        conn.execute(
            "UPDATE ticket_self_work_items SET state = 'closed' WHERE work_id = 'w1'",
            [],
        )?;

        let summary = scan_core_state_machine_violations(&conn, 100)?;
        let core_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_pm_event_transition_coverage WHERE mapping_kind = 'core_transition'",
            [],
            |row| row.get(0),
        )?;
        let unmapped_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_pm_event_transition_coverage WHERE mapping_kind = 'unmapped'",
            [],
            |row| row.get(0),
        )?;
        let rule_id: String = conn.query_row(
            "SELECT rule_id FROM ctox_pm_event_transition_coverage WHERE mapping_kind = 'core_transition' LIMIT 1",
            [],
            |row| row.get(0),
        )?;

        assert!(core_count > 0, "{summary}");
        assert!(unmapped_count > 0, "{summary}");
        assert_eq!(rule_id, "ticket");
        Ok(())
    }

    #[test]
    fn communication_runtime_tables_are_not_misclassified_as_founder_mail() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE communication_accounts (
                account_id TEXT PRIMARY KEY,
                address TEXT NOT NULL
            );
            CREATE TABLE communication_threads (
                thread_id TEXT PRIMARY KEY,
                subject TEXT NOT NULL
            );
            CREATE TABLE communication_routing_state (
                route_id TEXT PRIMARY KEY,
                route_status TEXT NOT NULL,
                protected_party TEXT
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            "INSERT INTO communication_accounts (account_id, address) VALUES ('a1', 'cto1@example.com')",
            [],
        )?;
        conn.execute(
            "INSERT INTO communication_threads (thread_id, subject) VALUES ('t1', 'CRM')",
            [],
        )?;
        conn.execute(
            "INSERT INTO communication_routing_state (route_id, route_status, protected_party) VALUES ('r1', 'leased', 'founder')",
            [],
        )?;

        let summary = scan_core_state_machine_violations(&conn, 100)?;
        let without_transition = summary
            .get("rule_matched_without_core_transition")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let account_telemetry: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_event_transition_coverage
            WHERE rule_id = 'communication-account-telemetry'
              AND mapping_kind = 'telemetry'
            "#,
            [],
            |row| row.get(0),
        )?;
        let thread_telemetry: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_event_transition_coverage
            WHERE rule_id = 'communication-thread-telemetry'
              AND mapping_kind = 'telemetry'
            "#,
            [],
            |row| row.get(0),
        )?;
        let routing_core: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_core_transition_audit
            WHERE rule_id = 'communication-routing-state'
              AND entity_type = 'QueueItem'
              AND to_state = 'Leased'
              AND accepted = 1
            "#,
            [],
            |row| row.get(0),
        )?;

        assert_eq!(without_transition, 0, "{summary}");
        assert_eq!(account_telemetry, 1);
        assert_eq!(thread_telemetry, 1);
        assert_eq!(routing_core, 1);
        Ok(())
    }

    #[test]
    fn queue_lease_owner_key_does_not_mark_founder_lane() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE communication_routing_state (
                message_key TEXT PRIMARY KEY,
                route_status TEXT NOT NULL,
                lease_owner TEXT,
                leased_at TEXT,
                acked_at TEXT,
                last_error TEXT,
                updated_at TEXT NOT NULL
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            r#"
            INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, updated_at
            )
            VALUES (
                'queue:system::normal', 'leased', 'ctox-service',
                '2026-04-26T01:50:16Z', '2026-04-26T01:50:16Z'
            )
            "#,
            [],
        )?;
        conn.execute(
            r#"
            UPDATE communication_routing_state
            SET route_status = 'cancelled',
                lease_owner = NULL,
                leased_at = NULL,
                acked_at = '2026-04-26T01:50:23Z',
                updated_at = '2026-04-26T01:50:23Z'
            WHERE message_key = 'queue:system::normal'
            "#,
            [],
        )?;

        let _ = scan_core_state_machine_violations(&conn, 100)?;
        let event_id: String = conn.query_row(
            r#"
            SELECT event_id
            FROM ctox_pm_core_transition_audit
            WHERE rule_id = 'communication-routing-state'
              AND to_state = 'Superseded'
            LIMIT 1
            "#,
            [],
            |row| row.get(0),
        )?;
        conn.execute(
            r#"
            INSERT INTO ctox_pm_state_violations (
                violation_id, event_id, case_id, violation_code, severity, message, detected_at
            )
            VALUES (
                'stale-founder-spill', ?1, 'case', 'founder_work_cannot_spill',
                'critical', 'stale violation from an older model', '2026-04-26T01:00:00Z'
            )
            "#,
            params![event_id],
        )?;

        let summary = scan_core_state_machine_violations(&conn, 100)?;
        let founder_spill_violations: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_state_violations
            WHERE violation_code = 'founder_work_cannot_spill'
            "#,
            [],
            |row| row.get(0),
        )?;
        let lane: String = conn.query_row(
            r#"
            SELECT lane
            FROM ctox_pm_core_transition_audit
            WHERE rule_id = 'communication-routing-state'
              AND to_state = 'Superseded'
            LIMIT 1
            "#,
            [],
            |row| row.get(0),
        )?;

        assert_eq!(founder_spill_violations, 0, "{summary}");
        assert_eq!(lane, "P2MissionDelivery");
        Ok(())
    }

    #[test]
    fn strategic_directives_are_explicit_runtime_telemetry() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE strategic_directives (
                directive_id TEXT PRIMARY KEY,
                conversation_id INTEGER NOT NULL,
                thread_key TEXT,
                directive_kind TEXT NOT NULL,
                title TEXT NOT NULL,
                body_text TEXT NOT NULL,
                status TEXT NOT NULL,
                revision INTEGER NOT NULL,
                previous_directive_id TEXT,
                author TEXT NOT NULL,
                decided_by TEXT,
                decision_reason TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            r#"
            INSERT INTO strategic_directives (
                directive_id, conversation_id, thread_key, directive_kind,
                title, body_text, status, revision, previous_directive_id,
                author, decided_by, decision_reason, created_at, updated_at
            )
            VALUES (
                'sdir-test', 42, 'queue/follow-up-test', 'mission',
                'Runtime mission', 'Keep canonical direction in SQLite.',
                'active', 1, NULL, 'ctox', 'ctox', 'test',
                '2026-04-26T00:00:00Z', '2026-04-26T00:00:00Z'
            )
            "#,
            [],
        )?;

        let summary = scan_core_state_machine_violations(&conn, 100)?;
        let unmapped = summary
            .get("unmapped")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let strategy_telemetry: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_event_transition_coverage
            WHERE rule_id = 'strategic-directive-telemetry'
              AND mapping_kind = 'telemetry'
            "#,
            [],
            |row| row.get(0),
        )?;

        assert_eq!(unmapped, 0, "{summary}");
        assert_eq!(strategy_telemetry, 1);
        Ok(())
    }

    #[test]
    fn inbound_message_status_update_to_sending_value_is_not_classified_as_outgoing_send(
    ) -> Result<()> {
        // Bug A regression: production saw 109 rejected
        // `founder_send_body_hash_mismatch` proofs per inbound mail because
        // every UPDATE on `communication_messages` was routed into the
        // FounderCommunication-Send rule, regardless of the row's
        // `direction` column. The inference must skip Sending/Sent
        // classification when the underlying row is not direction=outbound.
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE communication_messages (
                message_key TEXT PRIMARY KEY,
                direction TEXT NOT NULL,
                folder_hint TEXT NOT NULL,
                status TEXT NOT NULL,
                sender_address TEXT,
                recipient_addresses_json TEXT,
                body_text TEXT
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            r#"
            INSERT INTO communication_messages (
                message_key, direction, folder_hint, status,
                sender_address, recipient_addresses_json, body_text
            )
            VALUES (
                'email:cto1@example::INBOX::42', 'inbound', 'INBOX', 'received',
                'sender@external', '["cto1@example"]', 'incoming text'
            )
            "#,
            [],
        )?;
        // Now an UPDATE bumps the status to a value the existing inference
        // historically treated as a Sending-trigger. The fix must keep this
        // a noop classification — we are NOT sending an inbound mail.
        conn.execute(
            "UPDATE communication_messages SET status = 'sending' WHERE message_key = 'email:cto1@example::INBOX::42'",
            [],
        )?;

        let summary = scan_core_state_machine_violations(&conn, 100)?;
        let send_proofs: i64 = conn.query_row(
            r#"
            SELECT COUNT(*) FROM ctox_core_transition_proofs
            WHERE entity_type = 'FounderCommunication'
              AND to_state IN ('Sending','Sent')
            "#,
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            send_proofs, 0,
            "inbound mail must not produce Sending/Sent proofs, got summary {summary}"
        );
        Ok(())
    }

    #[test]
    fn inbound_communication_messages_do_not_trip_founder_send_review_gate() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE communication_messages (
                message_key TEXT PRIMARY KEY,
                direction TEXT NOT NULL,
                folder_hint TEXT NOT NULL,
                status TEXT NOT NULL,
                sender_address TEXT,
                recipient_addresses_json TEXT,
                body_text TEXT
            );
            CREATE TABLE communication_sync_runs (
                run_key TEXT PRIMARY KEY,
                channel TEXT NOT NULL,
                folder_hint TEXT NOT NULL,
                ok INTEGER NOT NULL
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            r#"
            INSERT INTO communication_messages (
                message_key, direction, folder_hint, status, sender_address,
                recipient_addresses_json, body_text
            )
            VALUES (
                'jami:ctox::INBOX::m1', 'inbound', 'INBOX', 'received',
                'founder@example.com', '["ctox"]', 'Bitte Stand an Michael senden'
            )
            "#,
            [],
        )?;
        conn.execute(
            "UPDATE communication_messages SET status = 'received' WHERE message_key = 'jami:ctox::INBOX::m1'",
            [],
        )?;
        conn.execute(
            "INSERT INTO communication_sync_runs (run_key, channel, folder_hint, ok) VALUES ('sync-1', 'jami', 'INBOX', 1)",
            [],
        )?;

        let summary = scan_core_state_machine_violations(&conn, 100)?;
        let rejected = summary
            .get("rejected")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let send_gate_violations: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_state_violations
            WHERE violation_code IN (
                'founder_send_requires_review_audit',
                'founder_send_body_hash_mismatch',
                'founder_send_recipient_hash_mismatch'
            )
            "#,
            [],
            |row| row.get(0),
        )?;
        let inbound_core: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_core_transition_audit
            WHERE rule_id = 'communication-message'
              AND entity_type = 'FounderCommunication'
              AND to_state = 'InboundObserved'
              AND accepted = 1
            "#,
            [],
            |row| row.get(0),
        )?;
        let sync_telemetry: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_event_transition_coverage
            WHERE rule_id = 'communication-sync-telemetry'
              AND mapping_kind = 'telemetry'
            "#,
            [],
            |row| row.get(0),
        )?;

        assert_eq!(rejected, 0, "{summary}");
        assert_eq!(send_gate_violations, 0);
        assert!(inbound_core > 0);
        assert_eq!(sync_telemetry, 1);
        Ok(())
    }

    #[test]
    fn runtime_housekeeping_tables_have_explicit_telemetry_coverage() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE ctox_payload_store (
                payload_key TEXT PRIMARY KEY,
                payload BLOB,
                updated_at TEXT
            );
            CREATE TABLE operating_health_snapshots (
                snapshot_id TEXT PRIMARY KEY,
                status TEXT NOT NULL
            );
            CREATE TABLE governance_mechanisms (
                mechanism_id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL
            );
            CREATE TABLE mission_states (
                state_id TEXT PRIMARY KEY,
                state_json TEXT
            );
            CREATE TABLE messages (
                message_id INTEGER PRIMARY KEY,
                body TEXT
            );
            CREATE TABLE context_items (
                conversation_id INTEGER NOT NULL,
                ordinal INTEGER NOT NULL,
                body TEXT,
                PRIMARY KEY (conversation_id, ordinal)
            );
            CREATE TABLE governance_events (
                event_id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL
            );
            CREATE TABLE mission_claims (
                claim_key TEXT PRIMARY KEY,
                status TEXT NOT NULL
            );
            CREATE TABLE ticket_audit_log (
                audit_id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL
            );
            CREATE TABLE ticket_self_work_notes (
                note_id TEXT PRIMARY KEY,
                body TEXT NOT NULL
            );
            CREATE TABLE local_ticket_events (
                event_id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL
            );
            CREATE TABLE ctox_core_spawn_edges (
                edge_id TEXT PRIMARY KEY,
                parent_entity_type TEXT NOT NULL,
                parent_entity_id TEXT NOT NULL,
                child_entity_type TEXT NOT NULL,
                child_entity_id TEXT NOT NULL,
                spawn_kind TEXT NOT NULL,
                spawn_reason TEXT NOT NULL,
                actor TEXT NOT NULL,
                checkpoint_key TEXT,
                budget_key TEXT,
                max_attempts INTEGER,
                accepted INTEGER NOT NULL,
                violation_codes_json TEXT NOT NULL DEFAULT '[]',
                request_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                terminal_reaped_at TEXT
            );
            CREATE TABLE ctox_hm_audit_runs (
                audit_id TEXT PRIMARY KEY,
                status TEXT NOT NULL
            );
            CREATE TABLE ctox_hm_findings (
                finding_id TEXT PRIMARY KEY,
                status TEXT NOT NULL
            );
            CREATE TABLE runtime_env_kv (
                key TEXT PRIMARY KEY,
                value TEXT
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            "INSERT INTO ctox_payload_store (payload_key, payload, updated_at) VALUES ('p1', x'01', 'now')",
            [],
        )?;
        conn.execute(
            "UPDATE ctox_payload_store SET updated_at = 'later' WHERE payload_key = 'p1'",
            [],
        )?;
        conn.execute(
            "INSERT INTO operating_health_snapshots (snapshot_id, status) VALUES ('h1', 'ok')",
            [],
        )?;
        conn.execute(
            "INSERT INTO governance_mechanisms (mechanism_id, enabled) VALUES ('g1', 1)",
            [],
        )?;
        conn.execute(
            "UPDATE governance_mechanisms SET enabled = 0 WHERE mechanism_id = 'g1'",
            [],
        )?;
        conn.execute(
            "INSERT INTO mission_states (state_id, state_json) VALUES ('m1', '{}')",
            [],
        )?;
        conn.execute(
            "INSERT INTO messages (message_id, body) VALUES (1, 'context message')",
            [],
        )?;
        conn.execute(
            "INSERT INTO context_items (conversation_id, ordinal, body) VALUES (1, 1, 'context item')",
            [],
        )?;
        conn.execute(
            "INSERT INTO governance_events (event_id, event_type) VALUES ('g1', 'created')",
            [],
        )?;
        conn.execute(
            "INSERT INTO mission_claims (claim_key, status) VALUES ('c1', 'open')",
            [],
        )?;
        conn.execute(
            "UPDATE mission_claims SET status = 'open' WHERE claim_key = 'c1'",
            [],
        )?;
        conn.execute(
            "INSERT INTO ticket_audit_log (audit_id, event_type) VALUES ('a1', 'created')",
            [],
        )?;
        conn.execute(
            "INSERT INTO ticket_self_work_notes (note_id, body) VALUES ('n1', 'note')",
            [],
        )?;
        conn.execute(
            "INSERT INTO local_ticket_events (event_id, event_type) VALUES ('le1', 'created')",
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO ctox_core_spawn_edges (
                edge_id, parent_entity_type, parent_entity_id,
                child_entity_type, child_entity_id, spawn_kind, spawn_reason,
                actor, accepted, violation_codes_json, request_json,
                created_at, updated_at
            )
            VALUES (
                'e1', 'Thread', 'queue-test', 'WorkItem', 'w1',
                'self-work:test', 'test', 'ctox-test', 1, '[]', '{}',
                'now', 'now'
            )
            "#,
            [],
        )?;
        conn.execute(
            "INSERT INTO ctox_hm_audit_runs (audit_id, status) VALUES ('hm1', 'ok')",
            [],
        )?;
        conn.execute(
            "INSERT INTO ctox_hm_findings (finding_id, status) VALUES ('hfind1', 'verified')",
            [],
        )?;
        conn.execute(
            "INSERT INTO runtime_env_kv (key, value) VALUES ('CTOX_CHAT_MODEL_MAX_CONTEXT', '131072')",
            [],
        )?;

        let summary = scan_core_state_machine_violations(&conn, 100)?;
        let unmapped = summary
            .get("unmapped")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let telemetry_count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_event_transition_coverage
            WHERE mapping_kind = 'telemetry'
              AND rule_id IN (
                  'payload-store-telemetry',
                  'operating-health-telemetry',
                  'governance-telemetry',
                  'mission-state-telemetry',
                  'context-message-telemetry',
                  'context-item-telemetry',
                  'governance-event-telemetry',
                  'mission-claim-telemetry',
                  'ticket-audit-telemetry',
                  'ticket-note-telemetry',
                  'local-ticket-event-telemetry',
                  'core-spawn-edge-ledger',
                  'harness-mining-audit-run-telemetry',
                  'harness-mining-finding-telemetry',
                  'runtime-env-telemetry'
              )
            "#,
            [],
            |row| row.get(0),
        )?;
        let communication_message_misclassified: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_event_transition_coverage
            WHERE table_name = 'messages'
              AND rule_id = 'communication-message'
            "#,
            [],
            |row| row.get(0),
        )?;

        assert_eq!(unmapped, 0, "{summary}");
        assert_eq!(telemetry_count, 18);
        assert_eq!(communication_message_misclassified, 0);
        Ok(())
    }

    #[test]
    fn state_scan_treats_domain_noop_route_updates_as_telemetry() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE communication_routing_state (
                message_key TEXT PRIMARY KEY,
                route_status TEXT NOT NULL,
                leased_at TEXT,
                acked_at TEXT
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            r#"
            INSERT INTO communication_routing_state (
                message_key, route_status, leased_at, acked_at
            )
            VALUES (
                'plan:system::goal::step', 'handled',
                '2026-04-25T10:00:00Z', '2026-04-25T10:01:00Z'
            )
            "#,
            [],
        )?;
        conn.execute(
            "UPDATE communication_routing_state SET acked_at = '2026-04-25T10:02:00Z' WHERE message_key = 'plan:system::goal::step'",
            [],
        )?;

        let summary = scan_core_state_machine_violations(&conn, 100)?;
        let rejected = summary
            .get("rejected")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let noop_telemetry: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_pm_event_transition_coverage
            WHERE table_name = 'communication_routing_state'
              AND reason = 'state_preserving_update'
            "#,
            [],
            |row| row.get(0),
        )?;

        assert_eq!(rejected, 0, "{summary}");
        assert_eq!(noop_telemetry, 1);
        Ok(())
    }

    #[test]
    fn self_diagnose_reports_subsystem_forensics() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE ticket_knowledge_entries (
                entry_id TEXT PRIMARY KEY,
                status TEXT NOT NULL
            );
            CREATE TABLE ticket_knowledge_loads (
                load_id TEXT PRIMARY KEY,
                ticket_key TEXT NOT NULL
            );
            CREATE TABLE continuity_documents (
                document_id TEXT PRIMARY KEY,
                conversation_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                head_commit_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE continuity_commits (
                commit_id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                parent_commit_id TEXT,
                diff_text TEXT NOT NULL,
                rendered_text TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE verification_runs (
                run_id TEXT PRIMARY KEY,
                conversation_id INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE communication_routing_state (
                message_key TEXT PRIMARY KEY,
                route_status TEXT NOT NULL,
                leased_at TEXT,
                acked_at TEXT
            );
            CREATE TABLE communication_founder_reply_reviews (
                review_id TEXT PRIMARY KEY,
                verdict TEXT NOT NULL
            );
            CREATE TABLE ticket_self_work_items (
                work_id TEXT PRIMARY KEY,
                title TEXT,
                state TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE scheduled_tasks (
                task_id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL,
                next_run_at TEXT
            );
            CREATE TABLE scheduled_task_runs (
                run_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                scheduled_for TEXT NOT NULL
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            "INSERT INTO ticket_knowledge_entries (entry_id, status) VALUES ('k1', 'active')",
            [],
        )?;
        conn.execute(
            "INSERT INTO ticket_knowledge_loads (load_id, ticket_key) VALUES ('l1', 't1')",
            [],
        )?;
        conn.execute(
            "INSERT INTO continuity_commits (commit_id, document_id, diff_text, rendered_text, created_at) VALUES ('c1', 'd1', '+x', 'x', '2026-04-25T10:00:00Z')",
            [],
        )?;
        conn.execute(
            "INSERT INTO continuity_documents (document_id, conversation_id, kind, head_commit_id, created_at, updated_at) VALUES ('d1', 1, 'focus', 'c1', '2026-04-25T10:00:00Z', '2026-04-25T10:00:00Z')",
            [],
        )?;
        conn.execute(
            "INSERT INTO verification_runs (run_id, conversation_id, created_at) VALUES ('v1', 1, '2026-04-25T10:00:00Z')",
            [],
        )?;
        conn.execute(
            "INSERT INTO communication_founder_reply_reviews (review_id, verdict) VALUES ('r1', 'approved')",
            [],
        )?;
        conn.execute(
            "INSERT INTO communication_routing_state (message_key, route_status, leased_at, acked_at) VALUES ('q1', 'handled', '2026-04-25T10:00:00Z', '2026-04-25T10:01:00Z')",
            [],
        )?;
        conn.execute(
            "INSERT INTO ticket_self_work_items (work_id, title, state, created_at, updated_at) VALUES ('w1', 'Done', 'closed', '2026-04-25T10:00:00Z', '2026-04-25T10:05:00Z')",
            [],
        )?;
        conn.execute(
            "INSERT INTO scheduled_tasks (task_id, enabled, next_run_at) VALUES ('s1', 1, '2999-01-01T00:00:00Z')",
            [],
        )?;
        conn.execute(
            "INSERT INTO scheduled_task_runs (run_id, task_id, scheduled_for) VALUES ('sr1', 's1', '2026-04-25T10:00:00Z')",
            [],
        )?;

        let report = run_process_mining_self_diagnosis(&conn, 100)?;
        let names = report
            .get("subsystems")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                item.get("name")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            })
            .collect::<Vec<_>>();

        assert!(names.contains(&"knowledge".to_string()), "{report}");
        assert!(names.contains(&"lcm_continuity".to_string()), "{report}");
        assert!(names.contains(&"queue_processing".to_string()), "{report}");
        assert!(
            names.contains(&"founder_communication_review".to_string()),
            "{report}"
        );
        assert!(
            names.contains(&"tickets_and_self_work".to_string()),
            "{report}"
        );
        assert!(
            names.contains(&"schedules_and_commitments".to_string()),
            "{report}"
        );
        Ok(())
    }

    #[test]
    fn self_diagnose_keeps_historical_founder_violations_noncritical() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE communication_founder_reply_reviews (
                review_id TEXT PRIMARY KEY,
                verdict TEXT NOT NULL
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            "INSERT INTO communication_founder_reply_reviews (review_id, verdict) VALUES ('r1', 'approved')",
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO ctox_pm_state_violations (
                violation_id, event_id, case_id, violation_code, severity, message, detected_at
            )
            VALUES (
                'old-founder-send', 'old-event', 'old-case',
                'founder_send_requires_review_audit', 'critical',
                'old founder communication review failure', '2026-04-25T22:54:55Z'
            )
            "#,
            [],
        )?;

        let report = run_process_mining_self_diagnosis(&conn, 100)?;
        let founder_status = report
            .get("subsystems")
            .and_then(Value::as_array)
            .and_then(|items| {
                items.iter().find(|item| {
                    item.get("name").and_then(Value::as_str) == Some("founder_communication_review")
                })
            })
            .and_then(|item| item.get("status"))
            .and_then(Value::as_str);

        assert_eq!(founder_status, Some("warning"), "{report}");
        Ok(())
    }

    #[test]
    fn queue_diagnosis_does_not_flag_fresh_active_lease_as_stuck() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE communication_routing_state (
                message_key TEXT PRIMARY KEY,
                route_status TEXT NOT NULL,
                leased_at TEXT,
                acked_at TEXT
            );
            "#,
        )?;
        let fresh = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        conn.execute(
            "INSERT INTO communication_routing_state (message_key, route_status, leased_at, acked_at) VALUES ('fresh', 'leased', ?1, NULL)",
            params![fresh],
        )?;

        let report = diagnose_queue(&conn)?;
        let findings = report
            .get("findings")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            !findings.iter().any(|finding| {
                finding.get("code").and_then(Value::as_str) == Some("stuck_queue_items")
            }),
            "{report}"
        );
        assert_eq!(
            report
                .pointer("/metrics/active_leased")
                .and_then(Value::as_i64),
            Some(1),
            "{report}"
        );
        assert_eq!(
            report
                .pointer("/metrics/stuck_leased")
                .and_then(Value::as_i64),
            Some(0),
            "{report}"
        );
        Ok(())
    }

    #[test]
    fn queue_diagnosis_flags_only_stale_unacked_leases_as_stuck() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE communication_routing_state (
                message_key TEXT PRIMARY KEY,
                route_status TEXT NOT NULL,
                leased_at TEXT,
                acked_at TEXT
            );
            "#,
        )?;
        let stale = (chrono::Utc::now() - chrono::Duration::minutes(30))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        conn.execute(
            "INSERT INTO communication_routing_state (message_key, route_status, leased_at, acked_at) VALUES ('stale', 'leased', ?1, NULL)",
            params![stale],
        )?;

        let report = diagnose_queue(&conn)?;
        assert_eq!(
            report.get("status").and_then(Value::as_str),
            Some("critical"),
            "{report}"
        );
        assert_eq!(
            report
                .pointer("/metrics/stuck_leased")
                .and_then(Value::as_i64),
            Some(1),
            "{report}"
        );
        Ok(())
    }

    #[test]
    fn assert_clean_fails_on_mapping_gaps_and_passes_clean_reviewed_flow() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ctox.sqlite3");
        let conn = Connection::open(&db_path)?;
        let body = "Reviewed status update";
        let recipients = "michael@example.com";
        let body_sha = full_sha256_hex(body);
        let recipients_sha = full_sha256_hex(recipients);
        conn.execute_batch(
            r#"
            CREATE TABLE communication_founder_outbox (
                mail_id TEXT PRIMARY KEY,
                protected_party TEXT NOT NULL,
                status TEXT NOT NULL,
                body TEXT NOT NULL,
                recipients TEXT NOT NULL,
                review_audit_key TEXT,
                approved_body_sha256 TEXT,
                approved_recipient_set_sha256 TEXT
            );
            CREATE TABLE misc_runtime_notes (
                note_id TEXT PRIMARY KEY,
                status TEXT NOT NULL
            );
            "#,
        )?;
        ensure_process_mining_schema(&conn, &db_path)?;
        conn.execute(
            r#"
            INSERT INTO communication_founder_outbox (
                mail_id, protected_party, status, body, recipients,
                review_audit_key, approved_body_sha256, approved_recipient_set_sha256
            )
            VALUES ('mail-1', 'founder', 'approved', ?1, ?2, 'review-1', ?3, ?4)
            "#,
            params![body, recipients, body_sha, recipients_sha],
        )?;
        conn.execute(
            "UPDATE communication_founder_outbox SET status = 'sending' WHERE mail_id = 'mail-1'",
            [],
        )?;

        let clean_summary = scan_core_state_machine_violations(&conn, 100)?;
        assert_process_mining_clean_summary(&clean_summary, false)?;

        conn.execute(
            "INSERT INTO misc_runtime_notes (note_id, status) VALUES ('n1', 'created')",
            [],
        )?;
        let dirty_summary = scan_core_state_machine_violations(&conn, 100)?;
        let err = assert_process_mining_clean_summary(&dirty_summary, false)
            .expect_err("dirty process mining summary must fail");
        let message = err.to_string();
        assert!(message.contains("ctox process-mining guidance --limit 50"));
        assert!(!message.contains("\"summary\""));
        assert!(!message.contains("row_after_json"));
        Ok(())
    }

    #[test]
    fn proof_evidence_summary_surfaces_outcome_witness_refs() {
        let summary = proof_evidence_summary(
            r#"{
                "evidence": {
                    "review_audit_key": "review-1",
                    "expected_artifact_refs": [{
                        "kind": "OutboundEmail",
                        "primary_key": "thread:queue/final-mail",
                        "expected_terminal_state": "accepted"
                    }],
                    "delivered_artifact_refs": [{
                        "kind": "OutboundEmail",
                        "primary_key": "msg-accepted-1",
                        "expected_terminal_state": "accepted"
                    }]
                }
            }"#,
        );
        assert_eq!(summary["review_audit_key"], "review-1");
        assert_eq!(summary["expected_artifact_ref_count"], 1);
        assert_eq!(summary["delivered_artifact_ref_count"], 1);
        assert_eq!(
            summary["expected_artifact_refs"][0]["primary_key"],
            "thread:queue/final-mail"
        );
    }

    #[test]
    fn proof_report_summary_surfaces_outcome_violation_codes() {
        let summary = proof_report_summary(
            r#"{
                "accepted": false,
                "violations": [
                    {"code": "WP-Outcome-Missing", "message": "missing artifact"}
                ]
            }"#,
        );
        assert_eq!(summary["accepted"], false);
        assert_eq!(summary["violation_count"], 1);
        assert_eq!(summary["violation_codes"][0], "WP-Outcome-Missing");
    }
}
