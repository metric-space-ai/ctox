//! Module for `report_*` data stages backed by typed rows: requirements,
//! options, scoring rubrics, matrix cells, scenarios, risks, claims.
//!
//! Stages are kept in one file because they share idiomatic CRUD shape and
//! the same input/output contract: every mutation upserts a typed row,
//! returns it as JSON, and may advance the state machine. None of these
//! stages produce free prose; they accept structured input from the SKILL
//! procedure or the operator.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

use crate::report::blueprints::Blueprint;
use crate::report::state_machine::{self, Status};
use crate::report::store;

// ---------- Requirements (frame stage) ----------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequirementInput {
    pub code: String,
    pub title: String,
    #[serde(default)]
    pub description_md: Option<String>,
    #[serde(default = "default_must_have")]
    pub must_have: bool,
    #[serde(default)]
    pub derived_from_question_idx: Option<i64>,
}

fn default_must_have() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct RequirementView {
    pub requirement_id: String,
    pub code: String,
    pub title: String,
    pub description_md: Option<String>,
    pub must_have: bool,
    pub derived_from_question_idx: Option<i64>,
}

pub fn upsert_requirement(
    conn: &Connection,
    run_id: &str,
    input: &RequirementInput,
) -> Result<RequirementView> {
    state_machine::require_at_least(conn, run_id, Status::Scoped)?;
    if input.code.trim().is_empty() || input.title.trim().is_empty() {
        bail!("requirement code and title must be non-empty");
    }
    let now = store::now_iso();
    let requirement_id = store::new_id("req");
    conn.execute(
        "INSERT INTO report_requirements(requirement_id, run_id, code, title, description_md,
            must_have, derived_from_question_idx, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(run_id, code) DO UPDATE SET
            title = excluded.title,
            description_md = excluded.description_md,
            must_have = excluded.must_have,
            derived_from_question_idx = excluded.derived_from_question_idx",
        params![
            requirement_id,
            run_id,
            input.code.trim(),
            input.title.trim(),
            input.description_md.as_deref(),
            input.must_have as i64,
            input.derived_from_question_idx,
            now,
        ],
    )
    .context("failed to upsert report_requirements")?;
    state_machine::advance_to(conn, run_id, Status::Framing)?;
    crate::report::runs::set_next_stage(conn, run_id, Some("enumerate"))?;
    list_requirements(conn, run_id)?
        .into_iter()
        .find(|r| r.code == input.code.trim())
        .context("requirement missing after upsert")
}

pub fn list_requirements(conn: &Connection, run_id: &str) -> Result<Vec<RequirementView>> {
    let mut stmt = conn.prepare(
        "SELECT requirement_id, code, title, description_md, must_have, derived_from_question_idx
         FROM report_requirements WHERE run_id = ?1 ORDER BY code ASC",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        Ok(RequirementView {
            requirement_id: row.get(0)?,
            code: row.get(1)?,
            title: row.get(2)?,
            description_md: row.get(3)?,
            must_have: row.get::<_, i64>(4)? != 0,
            derived_from_question_idx: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------- Options (enumerate stage) ----------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OptionInput {
    pub code: String,
    pub label: String,
    #[serde(default)]
    pub summary_md: Option<String>,
    #[serde(default)]
    pub synonyms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptionView {
    pub option_id: String,
    pub code: String,
    pub label: String,
    pub summary_md: Option<String>,
    pub synonyms: Vec<String>,
}

pub fn upsert_option(conn: &Connection, run_id: &str, input: &OptionInput) -> Result<OptionView> {
    state_machine::require_at_least(conn, run_id, Status::Scoped)?;
    if input.code.trim().is_empty() || input.label.trim().is_empty() {
        bail!("option code and label must be non-empty");
    }
    let option_id = store::new_id("opt");
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_options(option_id, run_id, code, label, summary_md, synonyms_json, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(run_id, code) DO UPDATE SET
            label = excluded.label,
            summary_md = excluded.summary_md,
            synonyms_json = excluded.synonyms_json",
        params![
            option_id,
            run_id,
            input.code.trim(),
            input.label.trim(),
            input.summary_md.as_deref(),
            serde_json::to_string(&input.synonyms)?,
            now,
        ],
    )
    .context("failed to upsert report_options")?;
    state_machine::advance_to(conn, run_id, Status::Enumerating)?;
    crate::report::runs::set_next_stage(conn, run_id, Some("evidence"))?;
    list_options(conn, run_id)?
        .into_iter()
        .find(|o| o.code == input.code.trim())
        .context("option missing after upsert")
}

pub fn list_options(conn: &Connection, run_id: &str) -> Result<Vec<OptionView>> {
    let mut stmt = conn.prepare(
        "SELECT option_id, code, label, summary_md, synonyms_json
         FROM report_options WHERE run_id = ?1 ORDER BY code ASC",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        let synonyms_raw: String = row.get(4)?;
        Ok(OptionView {
            option_id: row.get(0)?,
            code: row.get(1)?,
            label: row.get(2)?,
            summary_md: row.get(3)?,
            synonyms: serde_json::from_str(&synonyms_raw).unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn lookup_option_by_code(
    conn: &Connection,
    run_id: &str,
    code: &str,
) -> Result<Option<OptionView>> {
    Ok(list_options(conn, run_id)?
        .into_iter()
        .find(|o| o.code == code))
}

// ---------- Scenarios ----------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScenarioInput {
    pub code: String,
    pub label: String,
    pub description_md: String,
    #[serde(default)]
    pub impact_summary_md: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioView {
    pub scenario_id: String,
    pub code: String,
    pub label: String,
    pub description_md: String,
    pub impact_summary_md: Option<String>,
}

pub fn upsert_scenario(
    conn: &Connection,
    run_id: &str,
    input: &ScenarioInput,
) -> Result<ScenarioView> {
    state_machine::require_at_least(conn, run_id, Status::Scoring)?;
    if input.code.trim().is_empty() {
        bail!("scenario code must be non-empty");
    }
    if input.description_md.trim().is_empty() {
        bail!("scenario description_md must be non-empty");
    }
    let scenario_id = store::new_id("scn");
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_scenarios(scenario_id, run_id, code, label, description_md, impact_summary_md, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(run_id, code) DO UPDATE SET
            label = excluded.label,
            description_md = excluded.description_md,
            impact_summary_md = excluded.impact_summary_md",
        params![
            scenario_id,
            run_id,
            input.code.trim(),
            input.label.trim(),
            input.description_md.trim(),
            input.impact_summary_md.as_deref(),
            now,
        ],
    )
    .context("failed to upsert report_scenarios")?;
    state_machine::advance_to(conn, run_id, Status::Scenarios)?;
    crate::report::runs::set_next_stage(conn, run_id, Some("draft"))?;
    list_scenarios(conn, run_id)?
        .into_iter()
        .find(|s| s.code == input.code.trim())
        .context("scenario missing after upsert")
}

pub fn list_scenarios(conn: &Connection, run_id: &str) -> Result<Vec<ScenarioView>> {
    let mut stmt = conn.prepare(
        "SELECT scenario_id, code, label, description_md, impact_summary_md
         FROM report_scenarios WHERE run_id = ?1 ORDER BY code ASC",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        Ok(ScenarioView {
            scenario_id: row.get(0)?,
            code: row.get(1)?,
            label: row.get(2)?,
            description_md: row.get(3)?,
            impact_summary_md: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------- Risks ----------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RiskInput {
    pub code: String,
    pub title: String,
    pub description_md: String,
    pub mitigation_md: String,
    #[serde(default)]
    pub likelihood: Option<String>,
    #[serde(default)]
    pub impact: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskView {
    pub risk_id: String,
    pub code: String,
    pub title: String,
    pub description_md: String,
    pub mitigation_md: String,
    pub likelihood: Option<String>,
    pub impact: Option<String>,
    pub evidence_ids: Vec<String>,
}

pub fn upsert_risk(conn: &Connection, run_id: &str, input: &RiskInput) -> Result<RiskView> {
    state_machine::require_at_least(conn, run_id, Status::Scoring)?;
    if input.mitigation_md.trim().is_empty() {
        bail!("risk mitigation_md must be non-empty");
    }
    if input.description_md.trim().is_empty() {
        bail!("risk description_md must be non-empty");
    }
    let risk_id = store::new_id("rsk");
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_risks(risk_id, run_id, code, title, description_md, likelihood,
            impact, mitigation_md, evidence_ids_json, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(run_id, code) DO UPDATE SET
            title = excluded.title,
            description_md = excluded.description_md,
            likelihood = excluded.likelihood,
            impact = excluded.impact,
            mitigation_md = excluded.mitigation_md,
            evidence_ids_json = excluded.evidence_ids_json",
        params![
            risk_id,
            run_id,
            input.code.trim(),
            input.title.trim(),
            input.description_md.trim(),
            input.likelihood.as_deref(),
            input.impact.as_deref(),
            input.mitigation_md.trim(),
            serde_json::to_string(&input.evidence_ids)?,
            now,
        ],
    )
    .context("failed to upsert report_risks")?;
    list_risks(conn, run_id)?
        .into_iter()
        .find(|r| r.code == input.code.trim())
        .context("risk missing after upsert")
}

pub fn list_risks(conn: &Connection, run_id: &str) -> Result<Vec<RiskView>> {
    let mut stmt = conn.prepare(
        "SELECT risk_id, code, title, description_md, likelihood, impact, mitigation_md, evidence_ids_json
         FROM report_risks WHERE run_id = ?1 ORDER BY code ASC",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        let ev_json: String = row.get(7)?;
        Ok(RiskView {
            risk_id: row.get(0)?,
            code: row.get(1)?,
            title: row.get(2)?,
            description_md: row.get(3)?,
            likelihood: row.get(4)?,
            impact: row.get(5)?,
            mitigation_md: row.get(6)?,
            evidence_ids: serde_json::from_str(&ev_json).unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------- Claims ----------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaimInput {
    pub section_id: String,
    pub text_md: String,
    pub claim_kind: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub assumption_note_md: Option<String>,
    #[serde(default)]
    pub confidence: Option<String>,
    #[serde(default)]
    pub primary_recommendation: bool,
    #[serde(default)]
    pub scenario_code: Option<String>,
    #[serde(default)]
    pub rubric_anchor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaimView {
    pub claim_id: String,
    pub section_id: String,
    pub position: i64,
    pub text_md: String,
    pub claim_kind: String,
    pub evidence_ids: Vec<String>,
    pub assumption_note_md: Option<String>,
    pub confidence: Option<String>,
    pub primary_recommendation: bool,
    pub scenario_code: Option<String>,
    pub rubric_anchor: Option<String>,
}

const VALID_CLAIM_KINDS: [&str; 5] = [
    "finding",
    "recommendation",
    "caveat",
    "assumption",
    "scope_note",
];

pub fn add_claim(
    conn: &Connection,
    blueprint: &Blueprint,
    run_id: &str,
    input: &ClaimInput,
) -> Result<ClaimView> {
    state_machine::require_at_least(conn, run_id, Status::Scoped)?;
    if input.text_md.trim().is_empty() {
        bail!("claim text_md must be non-empty");
    }
    if !VALID_CLAIM_KINDS.contains(&input.claim_kind.as_str()) {
        bail!(
            "claim_kind must be one of {:?}, got {}",
            VALID_CLAIM_KINDS,
            input.claim_kind
        );
    }
    let kind = input.claim_kind.as_str();
    let evidence_required = matches!(kind, "finding" | "recommendation" | "caveat");
    if evidence_required && input.evidence_ids.is_empty() {
        bail!(
            "claim_kind '{kind}' requires at least one evidence_id; \
             use 'assumption' or 'scope_note' for unsupported text"
        );
    }
    if !evidence_required
        && input.evidence_ids.is_empty()
        && input
            .assumption_note_md
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    {
        bail!("claim of kind '{kind}' without evidence_ids must carry assumption_note_md");
    }
    if !blueprint.sections.iter().any(|s| s.id == input.section_id) {
        bail!(
            "section_id '{}' is not in the blueprint section list",
            input.section_id
        );
    }
    if input.primary_recommendation && kind != "recommendation" {
        bail!("primary_recommendation flag is only valid on claim_kind = 'recommendation'");
    }
    // Validate evidence_ids exist.
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
    let position: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position),0) + 1 FROM report_claims
             WHERE run_id = ?1 AND section_id = ?2",
            params![run_id, input.section_id],
            |row| row.get(0),
        )
        .unwrap_or(1);
    let claim_id = store::new_id("cl");
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_claims(claim_id, run_id, section_id, position, text_md,
            claim_kind, confidence, evidence_ids_json, assumption_note_md, rubric_anchor,
            primary_recommendation, scenario_code, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            claim_id,
            run_id,
            input.section_id,
            position,
            input.text_md.trim(),
            kind,
            input.confidence.as_deref(),
            serde_json::to_string(&input.evidence_ids)?,
            input.assumption_note_md.as_deref(),
            input.rubric_anchor.as_deref(),
            input.primary_recommendation as i64,
            input.scenario_code.as_deref(),
            now,
        ],
    )
    .context("failed to insert report_claims")?;
    Ok(ClaimView {
        claim_id,
        section_id: input.section_id.clone(),
        position,
        text_md: input.text_md.trim().to_string(),
        claim_kind: kind.to_string(),
        evidence_ids: input.evidence_ids.clone(),
        assumption_note_md: input.assumption_note_md.clone(),
        confidence: input.confidence.clone(),
        primary_recommendation: input.primary_recommendation,
        scenario_code: input.scenario_code.clone(),
        rubric_anchor: input.rubric_anchor.clone(),
    })
}

pub fn list_claims(conn: &Connection, run_id: &str) -> Result<Vec<ClaimView>> {
    let mut stmt = conn.prepare(
        "SELECT claim_id, section_id, position, text_md, claim_kind, confidence,
                evidence_ids_json, assumption_note_md, rubric_anchor,
                primary_recommendation, scenario_code
         FROM report_claims WHERE run_id = ?1
         ORDER BY section_id ASC, position ASC",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        let ev_json: String = row.get(6)?;
        Ok(ClaimView {
            claim_id: row.get(0)?,
            section_id: row.get(1)?,
            position: row.get(2)?,
            text_md: row.get(3)?,
            claim_kind: row.get(4)?,
            confidence: row.get(5)?,
            evidence_ids: serde_json::from_str(&ev_json).unwrap_or_default(),
            assumption_note_md: row.get(7)?,
            rubric_anchor: row.get(8)?,
            primary_recommendation: row.get::<_, i64>(9)? != 0,
            scenario_code: row.get(10)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn claim_payload(claim: &ClaimView) -> Value {
    json!({
        "claim_id": claim.claim_id,
        "section_id": claim.section_id,
        "position": claim.position,
        "text_md": claim.text_md,
        "claim_kind": claim.claim_kind,
        "evidence_ids": claim.evidence_ids,
        "assumption_note_md": claim.assumption_note_md,
        "confidence": claim.confidence,
        "primary_recommendation": claim.primary_recommendation,
        "scenario_code": claim.scenario_code,
        "rubric_anchor": claim.rubric_anchor,
    })
}
