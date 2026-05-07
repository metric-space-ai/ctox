//! `report scope` — bound the run: leading questions, out-of-scope, assumptions, disclaimer.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;

use crate::report::blueprints::Blueprint;
use crate::report::state_machine::{self, Status};
use crate::report::store;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScopeInput {
    pub leading_questions: Vec<String>,
    #[serde(default)]
    pub out_of_scope: Vec<String>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    pub disclaimer_md: String,
    #[serde(default)]
    pub success_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScopeView {
    pub leading_questions: Vec<String>,
    pub out_of_scope: Vec<String>,
    pub assumptions: Vec<String>,
    pub disclaimer_md: String,
    pub success_criteria: Vec<String>,
}

pub fn upsert_scope(
    conn: &Connection,
    blueprint: &Blueprint,
    run_id: &str,
    input: &ScopeInput,
) -> Result<ScopeView> {
    if input.leading_questions.len() < blueprint.bounds.min_leading_questions {
        bail!(
            "blueprint requires at least {} leading questions; got {}",
            blueprint.bounds.min_leading_questions,
            input.leading_questions.len()
        );
    }
    let trimmed_disclaimer = input.disclaimer_md.trim();
    if trimmed_disclaimer.chars().count() < blueprint.bounds.min_disclaimer_chars {
        bail!(
            "disclaimer is too short ({} chars); blueprint requires >= {}",
            trimmed_disclaimer.chars().count(),
            blueprint.bounds.min_disclaimer_chars
        );
    }
    let lower = trimmed_disclaimer.to_lowercase();
    for required in &blueprint.disclaimer.must_contain_all {
        if !lower.contains(&required.to_lowercase()) {
            bail!("disclaimer must contain '{required}' (blueprint disclaimer.must_contain_all)");
        }
    }
    if !blueprint.disclaimer.must_contain_any.is_empty() {
        let any_match = blueprint
            .disclaimer
            .must_contain_any
            .iter()
            .any(|tok| lower.contains(&tok.to_lowercase()));
        if !any_match {
            bail!(
                "disclaimer must contain at least one of {:?} (blueprint disclaimer.must_contain_any)",
                blueprint.disclaimer.must_contain_any
            );
        }
    }
    state_machine::require_at_least(conn, run_id, Status::Created)?;
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_scope(run_id, leading_questions_json, out_of_scope_json,
            assumptions_json, disclaimer_md, success_criteria_json, updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(run_id) DO UPDATE SET
            leading_questions_json = excluded.leading_questions_json,
            out_of_scope_json = excluded.out_of_scope_json,
            assumptions_json = excluded.assumptions_json,
            disclaimer_md = excluded.disclaimer_md,
            success_criteria_json = excluded.success_criteria_json,
            updated_at = excluded.updated_at",
        params![
            run_id,
            serde_json::to_string(&input.leading_questions)?,
            serde_json::to_string(&input.out_of_scope)?,
            serde_json::to_string(&input.assumptions)?,
            trimmed_disclaimer,
            serde_json::to_string(&input.success_criteria)?,
            now,
        ],
    )
    .context("failed to upsert report_scope")?;
    state_machine::advance_to(conn, run_id, Status::Scoped)?;
    crate::report::runs::set_next_stage(conn, run_id, Some("explore"))?;
    load_scope(conn, run_id)?.context("scope row missing after upsert")
}

pub fn load_scope(conn: &Connection, run_id: &str) -> Result<Option<ScopeView>> {
    conn.query_row(
        "SELECT leading_questions_json, out_of_scope_json, assumptions_json,
                disclaimer_md, success_criteria_json
         FROM report_scope WHERE run_id = ?1",
        params![run_id],
        |row| {
            let lq: String = row.get(0)?;
            let oos: String = row.get(1)?;
            let asm: String = row.get(2)?;
            let disc: String = row.get(3)?;
            let succ: String = row.get(4)?;
            Ok((lq, oos, asm, disc, succ))
        },
    )
    .optional()?
    .map(|(lq, oos, asm, disc, succ)| {
        Ok(ScopeView {
            leading_questions: serde_json::from_str(&lq)?,
            out_of_scope: serde_json::from_str(&oos)?,
            assumptions: serde_json::from_str(&asm)?,
            disclaimer_md: disc,
            success_criteria: serde_json::from_str(&succ)?,
        })
    })
    .transpose()
}

pub fn parse_scope_input_from_json(value: &Value) -> Result<ScopeInput> {
    let parsed: ScopeInput = serde_json::from_value(value.clone())
        .context("scope input must match { leading_questions[], out_of_scope[], assumptions[], disclaimer_md, success_criteria[] }")?;
    if parsed.leading_questions.is_empty() {
        bail!("leading_questions must contain at least one entry");
    }
    if parsed.disclaimer_md.trim().is_empty() {
        bail!("disclaimer_md must be a non-empty string");
    }
    Ok(parsed)
}

pub fn scope_payload(view: &ScopeView) -> Value {
    json!({
        "leading_questions": view.leading_questions,
        "out_of_scope": view.out_of_scope,
        "assumptions": view.assumptions,
        "disclaimer_md": view.disclaimer_md,
        "success_criteria": view.success_criteria,
    })
}
