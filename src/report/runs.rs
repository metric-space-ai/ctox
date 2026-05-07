//! `report_runs` CRUD: creating runs, listing, showing state.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;

use crate::report::blueprints::Blueprint;
use crate::report::state_machine::Status;
use crate::report::store;

#[derive(Debug, Clone, Serialize)]
pub struct RunView {
    pub run_id: String,
    pub preset: String,
    pub blueprint_version: String,
    pub topic: String,
    pub language: String,
    pub status: String,
    pub last_stage: Option<String>,
    pub next_stage: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub fn create_run(
    conn: &Connection,
    blueprint: &Blueprint,
    topic: &str,
    language: &str,
    locale_hints: Option<&Value>,
) -> Result<RunView> {
    if topic.trim().is_empty() {
        bail!("topic must be a non-empty string");
    }
    let run_id = store::new_id("run");
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_runs(
            run_id, preset, blueprint_version, topic, language, locale_hints,
            status, last_stage, next_stage, state_machine_version, created_at, updated_at
         ) VALUES(?1,?2,?3,?4,?5,?6,'created',NULL,'scope',1,?7,?7)",
        params![
            run_id,
            blueprint.preset,
            blueprint.schema_version,
            topic.trim(),
            language,
            locale_hints.map(|v| v.to_string()),
            now,
        ],
    )
    .context("failed to insert report_runs row")?;
    load_run(conn, &run_id)?.context("freshly inserted run not found")
}

pub fn load_run(conn: &Connection, run_id: &str) -> Result<Option<RunView>> {
    conn.query_row(
        "SELECT run_id, preset, blueprint_version, topic, language, status,
                last_stage, next_stage, created_at, updated_at
         FROM report_runs WHERE run_id = ?1",
        params![run_id],
        |row| {
            Ok(RunView {
                run_id: row.get(0)?,
                preset: row.get(1)?,
                blueprint_version: row.get(2)?,
                topic: row.get(3)?,
                language: row.get(4)?,
                status: row.get(5)?,
                last_stage: row.get(6)?,
                next_stage: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(anyhow::Error::from)
}

pub fn list_runs(conn: &Connection, status: Option<&str>, limit: usize) -> Result<Vec<RunView>> {
    let limit = limit.clamp(1, 200) as i64;
    let mut rows: Vec<RunView> = if let Some(status) = status {
        let mut stmt = conn.prepare(
            "SELECT run_id, preset, blueprint_version, topic, language, status,
                    last_stage, next_stage, created_at, updated_at
             FROM report_runs WHERE status = ?1
             ORDER BY updated_at DESC LIMIT ?2",
        )?;
        let it = stmt.query_map(params![status, limit], |row| {
            Ok(RunView {
                run_id: row.get(0)?,
                preset: row.get(1)?,
                blueprint_version: row.get(2)?,
                topic: row.get(3)?,
                language: row.get(4)?,
                status: row.get(5)?,
                last_stage: row.get(6)?,
                next_stage: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;
        it.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT run_id, preset, blueprint_version, topic, language, status,
                    last_stage, next_stage, created_at, updated_at
             FROM report_runs ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let it = stmt.query_map(params![limit], |row| {
            Ok(RunView {
                run_id: row.get(0)?,
                preset: row.get(1)?,
                blueprint_version: row.get(2)?,
                topic: row.get(3)?,
                language: row.get(4)?,
                status: row.get(5)?,
                last_stage: row.get(6)?,
                next_stage: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?;
        it.collect::<rusqlite::Result<Vec<_>>>()?
    };
    rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(rows)
}

pub fn set_next_stage(conn: &Connection, run_id: &str, next_stage: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE report_runs SET next_stage = ?2, updated_at = ?3 WHERE run_id = ?1",
        params![run_id, next_stage, store::now_iso()],
    )
    .context("failed to set next_stage")?;
    Ok(())
}

/// Record a stage_run row at the start of every stage operation.
pub fn record_stage_started(
    conn: &Connection,
    run_id: &str,
    stage: &str,
    input_payload: Option<&Value>,
) -> Result<String> {
    let stage_run_id = store::new_id("sr");
    let iteration: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(iteration),0) + 1 FROM report_stage_runs
             WHERE run_id = ?1 AND stage = ?2",
            params![run_id, stage],
            |row| row.get(0),
        )
        .unwrap_or(1);
    conn.execute(
        "INSERT INTO report_stage_runs(stage_run_id, run_id, stage, iteration, status,
            input_payload_json, started_at)
         VALUES(?1,?2,?3,?4,'running',?5,?6)",
        params![
            stage_run_id,
            run_id,
            stage,
            iteration,
            input_payload.map(|v| v.to_string()),
            store::now_iso(),
        ],
    )
    .context("failed to record stage_started")?;
    Ok(stage_run_id)
}

pub fn record_stage_finished(
    conn: &Connection,
    stage_run_id: &str,
    output_payload: Option<&Value>,
) -> Result<()> {
    conn.execute(
        "UPDATE report_stage_runs SET status='completed', output_payload_json=?2, finished_at=?3
         WHERE stage_run_id = ?1",
        params![
            stage_run_id,
            output_payload.map(|v| v.to_string()),
            store::now_iso(),
        ],
    )
    .context("failed to record stage_finished")?;
    Ok(())
}

pub fn run_summary(conn: &Connection, run_id: &str) -> Result<Value> {
    let view = load_run(conn, run_id)?.context("run not found")?;
    let counts = json!({
        "evidence": count(conn, "report_evidence", run_id)?,
        "options": count(conn, "report_options", run_id)?,
        "requirements": count(conn, "report_requirements", run_id)?,
        "scoring_rubrics": count(conn, "report_scoring_rubrics", run_id)?,
        "matrix_cells": count(conn, "report_matrix_cells", run_id)?,
        "scenarios": count(conn, "report_scenarios", run_id)?,
        "risks": count(conn, "report_risks", run_id)?,
        "claims": count(conn, "report_claims", run_id)?,
        "versions": count(conn, "report_versions", run_id)?,
        "critiques": count(conn, "report_critiques", run_id)?,
        "renders": count(conn, "report_renders", run_id)?,
        "checks": count(conn, "report_check_reports", run_id)?,
    });
    Ok(json!({
        "ok": true,
        "run": view,
        "counts": counts,
        "current_status": Status::parse(&view.status)?.as_str(),
    }))
}

fn count(conn: &Connection, table: &str, run_id: &str) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE run_id = ?1");
    Ok(conn.query_row(&sql, params![run_id], |row| row.get(0))?)
}
