//! Scoring rubrics + matrix cells.
//!
//! Numeric scores are not allowed without a defined rubric. A rubric maps
//! `(axis_code, level_code) -> level_definition_md + numeric_value`. Matrix
//! cells reference a rubric anchor of the form
//! `rubric:<axis_code>:<level_code>` so a validator can check the cell value
//! against the run's own rubric definition.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;

use crate::report::claims;
use crate::report::state_machine::{self, Status};
use crate::report::store;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RubricInput {
    pub axis_code: String,
    pub level_code: String,
    pub level_definition_md: String,
    #[serde(default)]
    pub numeric_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RubricView {
    pub rubric_id: String,
    pub axis_code: String,
    pub level_code: String,
    pub level_definition_md: String,
    pub numeric_value: Option<f64>,
}

pub fn upsert_rubric(conn: &Connection, run_id: &str, input: &RubricInput) -> Result<RubricView> {
    state_machine::require_at_least(conn, run_id, Status::Enumerating)?;
    if input.axis_code.trim().is_empty() || input.level_code.trim().is_empty() {
        bail!("rubric axis_code and level_code must be non-empty");
    }
    if input.level_definition_md.trim().chars().count() < 16 {
        bail!("rubric level_definition_md must be specific (at least 16 characters)");
    }
    let rubric_id = store::new_id("rb");
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_scoring_rubrics(rubric_id, run_id, axis_code, level_code,
            level_definition_md, numeric_value, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(run_id, axis_code, level_code) DO UPDATE SET
            level_definition_md = excluded.level_definition_md,
            numeric_value = excluded.numeric_value",
        params![
            rubric_id,
            run_id,
            input.axis_code.trim(),
            input.level_code.trim(),
            input.level_definition_md.trim(),
            input.numeric_value,
            now,
        ],
    )
    .context("failed to upsert report_scoring_rubrics")?;
    list_rubrics(conn, run_id)?
        .into_iter()
        .find(|r| r.axis_code == input.axis_code.trim() && r.level_code == input.level_code.trim())
        .context("rubric missing after upsert")
}

pub fn list_rubrics(conn: &Connection, run_id: &str) -> Result<Vec<RubricView>> {
    let mut stmt = conn.prepare(
        "SELECT rubric_id, axis_code, level_code, level_definition_md, numeric_value
         FROM report_scoring_rubrics WHERE run_id = ?1
         ORDER BY axis_code ASC, level_code ASC",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        Ok(RubricView {
            rubric_id: row.get(0)?,
            axis_code: row.get(1)?,
            level_code: row.get(2)?,
            level_definition_md: row.get(3)?,
            numeric_value: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn lookup_rubric(
    conn: &Connection,
    run_id: &str,
    axis_code: &str,
    level_code: &str,
) -> Result<Option<RubricView>> {
    Ok(list_rubrics(conn, run_id)?
        .into_iter()
        .find(|r| r.axis_code == axis_code && r.level_code == level_code))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CellInput {
    pub matrix_kind: String,
    #[serde(default)]
    pub matrix_label: Option<String>,
    pub option_code: String,
    pub axis_code: String,
    pub value_label: String,
    pub rationale_md: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub assumption_note_md: Option<String>,
    #[serde(default)]
    pub rubric_anchor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CellView {
    pub cell_id: String,
    pub matrix_kind: String,
    pub matrix_label: Option<String>,
    pub option_code: String,
    pub axis_code: String,
    pub value_label: String,
    pub value_numeric: Option<f64>,
    pub rubric_anchor: Option<String>,
    pub rationale_md: String,
    pub evidence_ids: Vec<String>,
    pub assumption_note_md: Option<String>,
}

pub fn upsert_cell(conn: &Connection, run_id: &str, input: &CellInput) -> Result<CellView> {
    state_machine::require_at_least(conn, run_id, Status::Enumerating)?;
    if input.rationale_md.trim().chars().count() < 16 {
        bail!("matrix cell rationale_md must be specific (at least 16 characters)");
    }
    if input.evidence_ids.is_empty()
        && input
            .assumption_note_md
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    {
        bail!(
            "matrix cell with no evidence_ids must carry assumption_note_md (this is a hard slop guard)"
        );
    }
    // option must exist
    let option = claims::lookup_option_by_code(conn, run_id, &input.option_code)?
        .with_context(|| format!("option_code '{}' not enumerated", input.option_code))?;

    // resolve rubric anchor if provided
    let mut numeric_value: Option<f64> = None;
    let mut rubric_anchor = input.rubric_anchor.clone();
    if let Some(anchor) = rubric_anchor.as_deref() {
        let parts: Vec<&str> = anchor.split(':').collect();
        if parts.len() != 3 || parts[0] != "rubric" {
            bail!(
                "rubric_anchor must be 'rubric:<axis_code>:<level_code>', got '{anchor}'"
            );
        }
        let axis = parts[1];
        let level = parts[2];
        if axis != input.axis_code {
            bail!(
                "rubric_anchor axis '{axis}' does not match cell axis '{}'",
                input.axis_code
            );
        }
        let rubric = lookup_rubric(conn, run_id, axis, level)?.with_context(|| {
            format!(
                "rubric_anchor refers to undefined rubric '{anchor}'; \
                 define it with `report scoring define-rubric` before scoring"
            )
        })?;
        numeric_value = rubric.numeric_value;
    } else if !input.value_label.trim().is_empty() {
        // If no anchor but a label is given, ensure a rubric exists for this
        // label on this axis. Otherwise the value is meaningless.
        let label = input.value_label.trim();
        let rubric = lookup_rubric(conn, run_id, &input.axis_code, label)?;
        if rubric.is_none() {
            bail!(
                "value_label '{label}' has no rubric on axis '{}'; \
                 define a rubric with this level_code or set rubric_anchor explicitly",
                input.axis_code
            );
        }
        rubric_anchor = Some(format!("rubric:{}:{}", input.axis_code, label));
        numeric_value = rubric.and_then(|r| r.numeric_value);
    }
    // evidence_ids must be registered
    for ev_id in &input.evidence_ids {
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM report_evidence WHERE run_id = ?1 AND evidence_id = ?2",
                params![run_id, ev_id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            bail!("evidence_id '{ev_id}' is not registered for this run");
        }
    }
    let cell_id = store::new_id("mc");
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_matrix_cells(cell_id, run_id, matrix_kind, matrix_label, option_id,
            axis_code, value_label, value_numeric, rubric_anchor, rationale_md,
            evidence_ids_json, assumption_note_md, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
         ON CONFLICT(run_id, matrix_kind, matrix_label, option_id, axis_code) DO UPDATE SET
            value_label = excluded.value_label,
            value_numeric = excluded.value_numeric,
            rubric_anchor = excluded.rubric_anchor,
            rationale_md = excluded.rationale_md,
            evidence_ids_json = excluded.evidence_ids_json,
            assumption_note_md = excluded.assumption_note_md",
        params![
            cell_id,
            run_id,
            input.matrix_kind.trim(),
            input.matrix_label.as_deref(),
            option.option_id,
            input.axis_code.trim(),
            input.value_label.trim(),
            numeric_value,
            rubric_anchor.as_deref(),
            input.rationale_md.trim(),
            serde_json::to_string(&input.evidence_ids)?,
            input.assumption_note_md.as_deref(),
            now,
        ],
    )
    .context("failed to upsert report_matrix_cells")?;
    state_machine::advance_to(conn, run_id, Status::Scoring)?;
    crate::report::runs::set_next_stage(conn, run_id, Some("scenarios"))?;
    Ok(CellView {
        cell_id,
        matrix_kind: input.matrix_kind.trim().to_string(),
        matrix_label: input.matrix_label.clone(),
        option_code: option.code,
        axis_code: input.axis_code.trim().to_string(),
        value_label: input.value_label.trim().to_string(),
        value_numeric: numeric_value,
        rubric_anchor,
        rationale_md: input.rationale_md.trim().to_string(),
        evidence_ids: input.evidence_ids.clone(),
        assumption_note_md: input.assumption_note_md.clone(),
    })
}

pub fn list_cells(conn: &Connection, run_id: &str) -> Result<Vec<CellView>> {
    let mut stmt = conn.prepare(
        "SELECT mc.cell_id, mc.matrix_kind, mc.matrix_label, opt.code, mc.axis_code,
                mc.value_label, mc.value_numeric, mc.rubric_anchor, mc.rationale_md,
                mc.evidence_ids_json, mc.assumption_note_md
         FROM report_matrix_cells mc
         JOIN report_options opt ON opt.option_id = mc.option_id
         WHERE mc.run_id = ?1
         ORDER BY mc.matrix_kind ASC, COALESCE(mc.matrix_label,''), opt.code ASC, mc.axis_code ASC",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        let ev_json: String = row.get(9)?;
        Ok(CellView {
            cell_id: row.get(0)?,
            matrix_kind: row.get(1)?,
            matrix_label: row.get(2)?,
            option_code: row.get(3)?,
            axis_code: row.get(4)?,
            value_label: row.get(5)?,
            value_numeric: row.get(6)?,
            rubric_anchor: row.get(7)?,
            rationale_md: row.get(8)?,
            evidence_ids: serde_json::from_str(&ev_json).unwrap_or_default(),
            assumption_note_md: row.get(10)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
