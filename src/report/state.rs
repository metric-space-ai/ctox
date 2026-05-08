//! Run lifecycle for the deep-research backend.
//!
//! Thin layer over the schema. Higher-level wiring (manager loop, CLI,
//! mission hook) is built in subsequent waves; this file owns only the
//! transitions, the row-level CRUD, and the asset-pack validation that
//! every fresh run needs before its first sub-skill call.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::report::asset_pack::AssetPack;
use crate::report::schema::{ensure_schema, new_id, now_iso, open, RunStatus};

/// Inputs to start a new report run. All four `*_profile_id` fields are
/// validated against the asset pack before insertion.
#[derive(Debug, Clone)]
pub struct CreateRunParams {
    pub report_type_id: String,
    pub domain_profile_id: String,
    pub depth_profile_id: String,
    pub style_profile_id: String,
    pub language: String,
    pub raw_topic: String,
    pub package_summary: Option<Value>,
}

/// Materialised `report_runs` row.
#[derive(Debug, Clone)]
pub struct RunRecord {
    pub run_id: String,
    pub report_type_id: String,
    pub domain_profile_id: String,
    pub depth_profile_id: String,
    pub style_profile_id: String,
    pub language: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub raw_topic: String,
    pub package_summary: Option<Value>,
}

pub fn create_run(root: &Path, params: CreateRunParams) -> Result<String> {
    let pack = AssetPack::load()?;
    pack.report_type(&params.report_type_id)
        .with_context(|| "report_type_id rejected by asset pack")?;
    pack.domain_profile(&params.domain_profile_id)
        .with_context(|| "domain_profile_id rejected by asset pack")?;
    pack.depth_profile(&params.depth_profile_id)
        .with_context(|| "depth_profile_id rejected by asset pack")?;
    pack.style_profile(&params.style_profile_id)
        .with_context(|| "style_profile_id rejected by asset pack")?;

    if params.language.trim().is_empty() {
        return Err(anyhow!("language is required for a report run"));
    }
    if params.raw_topic.trim().is_empty() {
        return Err(anyhow!("raw_topic is required for a report run"));
    }

    let conn = open(root)?;
    ensure_schema(&conn)?;

    let run_id = new_id("run");
    let now = now_iso();
    let package_summary_json = match &params.package_summary {
        Some(v) => Some(serde_json::to_string(v).context("encode package_summary")?),
        None => None,
    };

    conn.execute(
        "INSERT INTO report_runs (
             run_id, report_type_id, domain_profile_id, depth_profile_id,
             style_profile_id, language, status, started_at, finished_at,
             raw_topic, package_summary_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?10)",
        params![
            run_id,
            params.report_type_id,
            params.domain_profile_id,
            params.depth_profile_id,
            params.style_profile_id,
            params.language,
            RunStatus::Created.as_str(),
            now,
            params.raw_topic,
            package_summary_json,
        ],
    )
    .context("failed to insert report run")?;

    Ok(run_id)
}

pub fn load_run(root: &Path, run_id: &str) -> Result<RunRecord> {
    let conn = open(root)?;
    ensure_schema(&conn)?;
    load_run_with(&conn, run_id)
}

pub fn load_run_with(conn: &Connection, run_id: &str) -> Result<RunRecord> {
    conn.query_row(
        "SELECT run_id, report_type_id, domain_profile_id, depth_profile_id,
                style_profile_id, language, status, started_at, finished_at,
                raw_topic, package_summary_json
         FROM report_runs WHERE run_id = ?1",
        params![run_id],
        row_to_run_record,
    )
    .optional()
    .context("failed to query report_runs")?
    .ok_or_else(|| anyhow!("report run {run_id} not found"))
}

pub fn list_runs(root: &Path, limit: usize) -> Result<Vec<RunRecord>> {
    let conn = open(root)?;
    ensure_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT run_id, report_type_id, domain_profile_id, depth_profile_id,
                style_profile_id, language, status, started_at, finished_at,
                raw_topic, package_summary_json
         FROM report_runs ORDER BY started_at DESC LIMIT ?1",
    )?;
    let limit_i = limit as i64;
    let rows = stmt.query_map(params![limit_i], row_to_run_record)?;
    let mut out = Vec::with_capacity(limit);
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Forward-only state machine.
///
/// The only allowed equal-rank step is `Reviewing <-> Revising`; the
/// manager iterates between them while patches land. Aborted is a
/// terminal sink reachable from anywhere; `Finalised` is also terminal
/// and only reachable forward (no resurrection).
pub fn transition_to(conn: &Connection, run_id: &str, new_status: RunStatus) -> Result<()> {
    let current = load_run_with(conn, run_id)?;
    let cur_status = RunStatus::parse(&current.status)?;

    if cur_status == new_status {
        // No-op transitions are silently accepted; the manager calls
        // `transition_to` defensively.
        return Ok(());
    }
    if cur_status == RunStatus::Finalised || cur_status == RunStatus::Aborted {
        return Err(anyhow!(
            "report run {run_id} is in terminal state {} and cannot transition to {}",
            cur_status.as_str(),
            new_status.as_str()
        ));
    }

    let allowed_loop = matches!(
        (cur_status, new_status),
        (RunStatus::Reviewing, RunStatus::Revising) | (RunStatus::Revising, RunStatus::Reviewing)
    );
    let forward = new_status.rank() > cur_status.rank();
    let aborting = new_status == RunStatus::Aborted;

    if !(forward || allowed_loop || aborting) {
        return Err(anyhow!(
            "illegal report run transition {} -> {}",
            cur_status.as_str(),
            new_status.as_str()
        ));
    }

    let finished_at = match new_status {
        RunStatus::Finalised | RunStatus::Aborted => Some(now_iso()),
        _ => None,
    };

    let updated = conn.execute(
        "UPDATE report_runs SET status = ?1, finished_at = COALESCE(?2, finished_at)
         WHERE run_id = ?3",
        params![new_status.as_str(), finished_at, run_id],
    )?;
    if updated == 0 {
        return Err(anyhow!("report run {run_id} disappeared during transition"));
    }
    Ok(())
}

pub fn finalise(conn: &Connection, run_id: &str) -> Result<()> {
    transition_to(conn, run_id, RunStatus::Finalised)
}

pub fn abort(conn: &Connection, run_id: &str, reason: &str) -> Result<()> {
    let now = now_iso();
    let prov_id = new_id("prov");
    let payload = serde_json::json!({ "kind": "abort", "reason": reason });
    conn.execute(
        "INSERT INTO report_provenance (
             prov_id, run_id, kind, occurred_at, instance_id, skill_run_id,
             research_id, payload_json
         ) VALUES (?1, ?2, 'abort', ?3, NULL, NULL, NULL, ?4)",
        params![
            prov_id,
            run_id,
            now,
            serde_json::to_string(&payload).context("encode abort payload")?,
        ],
    )?;
    transition_to(conn, run_id, RunStatus::Aborted)
}

fn row_to_run_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    let package_summary_text: Option<String> = row.get(10)?;
    let package_summary = package_summary_text.and_then(|s| serde_json::from_str(&s).ok());
    Ok(RunRecord {
        run_id: row.get(0)?,
        report_type_id: row.get(1)?,
        domain_profile_id: row.get(2)?,
        depth_profile_id: row.get(3)?,
        style_profile_id: row.get(4)?,
        language: row.get(5)?,
        status: row.get(6)?,
        started_at: row.get(7)?,
        finished_at: row.get(8)?,
        raw_topic: row.get(9)?,
        package_summary,
    })
}
