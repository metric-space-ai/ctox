//! `report critique` and `report revise`.
//!
//! Critique findings can come from two sources:
//! - `mode=self`: derive from the most recent `report_check_reports` row.
//! - `mode=external`: a JSON file produced by the SKILL agent containing a
//!   findings array.
//!
//! Revise produces a new manuscript version. The witness-of-progress rule is
//! enforced: the new `body_hash` must differ from the parent version's hash,
//! otherwise the revise call fails.

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

use crate::report::check;
use crate::report::draft;
use crate::report::manuscript;
use crate::report::manuscript::Manuscript;
use crate::report::state_machine::{self, Status, MAX_REVISE_ITERATIONS};
use crate::report::store;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Finding {
    pub id: String,
    pub category: String, // wording | substantive | stale
    pub severity: String, // info | warn | error
    pub location_path: String,
    pub evidence: String,
    pub corrective_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CritiqueOutput {
    pub critique_id: String,
    pub run_id: String,
    pub version_id: String,
    pub findings: Vec<Finding>,
    pub summary_md: String,
}

pub fn record_self_critique(
    conn: &Connection,
    run_id: &str,
    version_id: Option<&str>,
) -> Result<CritiqueOutput> {
    let (version_id, _, _, _manuscript) = draft::load_version(conn, run_id, version_id)?;
    let row: Option<String> = conn
        .query_row(
            "SELECT validators_json FROM report_check_reports
             WHERE run_id = ?1 AND version_id = ?2
             ORDER BY created_at DESC LIMIT 1",
            params![run_id, version_id],
            |row| row.get(0),
        )
        .optional()?;
    let validators: Vec<check::ValidatorResult> = match row {
        Some(j) => serde_json::from_str(&j).unwrap_or_default(),
        None => bail!("no check report exists yet for this version; run `report check` first"),
    };
    let findings: Vec<Finding> = validators
        .into_iter()
        .filter(|v| !v.pass)
        .enumerate()
        .map(|(i, v)| Finding {
            id: format!("F{:03}", i + 1),
            category: validator_to_category(&v.name),
            severity: if v.severity == "hard" {
                "error".to_string()
            } else {
                "warn".to_string()
            },
            location_path: format!("validator:{}", v.name),
            evidence: serde_json::to_string(&v.evidence).unwrap_or_default(),
            corrective_action: format!("Resolve validator '{}': {}", v.name, v.details),
        })
        .collect();
    let summary_md = format!(
        "Self-critique derived from check report: {} finding(s).",
        findings.len()
    );
    persist_critique(conn, run_id, &version_id, &findings, &summary_md)
}

pub fn record_external_critique(
    conn: &Connection,
    run_id: &str,
    version_id: Option<&str>,
    findings: Vec<Finding>,
    summary_md: &str,
) -> Result<CritiqueOutput> {
    let (version_id, _, _, _) = draft::load_version(conn, run_id, version_id)?;
    persist_critique(conn, run_id, &version_id, &findings, summary_md)
}

fn persist_critique(
    conn: &Connection,
    run_id: &str,
    version_id: &str,
    findings: &[Finding],
    summary_md: &str,
) -> Result<CritiqueOutput> {
    let critique_id = store::new_id("crit");
    conn.execute(
        "INSERT INTO report_critiques(critique_id, run_id, version_id, findings_json,
            summary_md, created_at)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            critique_id,
            run_id,
            version_id,
            serde_json::to_string(findings)?,
            summary_md,
            store::now_iso(),
        ],
    )
    .context("failed to insert report_critiques")?;
    state_machine::advance_to(conn, run_id, Status::Critiquing).ok();
    crate::report::runs::set_next_stage(conn, run_id, Some("revise"))?;
    Ok(CritiqueOutput {
        critique_id,
        run_id: run_id.to_string(),
        version_id: version_id.to_string(),
        findings: findings.to_vec(),
        summary_md: summary_md.to_string(),
    })
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReviseInput {
    pub from_version_id: Option<String>,
    pub manuscript: Manuscript,
    #[serde(default)]
    pub notes_md: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviseOutput {
    pub version_id: String,
    pub version_number: i64,
    pub body_hash: String,
    pub parent_version_id: Option<String>,
}

pub fn revise(conn: &Connection, run_id: &str, input: &ReviseInput) -> Result<ReviseOutput> {
    let revise_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM report_versions WHERE run_id = ?1 AND produced_by = 'revise'",
            params![run_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if revise_count >= MAX_REVISE_ITERATIONS {
        bail!(
            "revise iteration cap of {} reached for this run; abort and re-scope, or use `report abort`",
            MAX_REVISE_ITERATIONS
        );
    }

    let (parent_id, parent_number, parent_hash, _parent) =
        draft::load_version(conn, run_id, input.from_version_id.as_deref())?;
    let new_hash = manuscript::body_hash(&input.manuscript);
    if new_hash == parent_hash {
        bail!(
            "revise produced identical body_hash {new_hash}; refusing without witness of progress"
        );
    }
    if input.manuscript.run_id != run_id {
        bail!(
            "manuscript.run_id '{}' does not match run_id '{}'",
            input.manuscript.run_id,
            run_id
        );
    }
    state_machine::reenter_revise(conn, run_id)?;
    let version_id = store::new_id("ver");
    let manuscript_json = serde_json::to_string(&input.manuscript)?;
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_versions(version_id, run_id, version_number, parent_version_id,
            manuscript_json, body_hash, produced_by, notes_md, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,'revise',?7,?8)",
        params![
            version_id,
            run_id,
            parent_number + 1,
            parent_id.clone(),
            manuscript_json,
            new_hash,
            input.notes_md.as_deref(),
            now,
        ],
    )
    .context("failed to insert revised version")?;
    crate::report::runs::set_next_stage(conn, run_id, Some("check"))?;
    Ok(ReviseOutput {
        version_id,
        version_number: parent_number + 1,
        body_hash: new_hash,
        parent_version_id: Some(parent_id),
    })
}

pub fn payload(out: &CritiqueOutput) -> Value {
    json!({
        "ok": true,
        "critique_id": out.critique_id,
        "run_id": out.run_id,
        "version_id": out.version_id,
        "findings_count": out.findings.len(),
        "findings": out.findings,
        "summary_md": out.summary_md,
    })
}

pub fn revise_payload(out: &ReviseOutput) -> Value {
    json!({
        "ok": true,
        "version_id": out.version_id,
        "version_number": out.version_number,
        "body_hash": out.body_hash,
        "parent_version_id": out.parent_version_id,
    })
}

fn validator_to_category(name: &str) -> String {
    match name {
        "forbid_unicode_dashes" | "forbid_filler_phrases" | "forbid_internal_vocab" => {
            "wording".to_string()
        }
        "every_section_present"
        | "every_claim_has_fk_evidence"
        | "claim_evidence_relevance"
        | "every_matrix_cell_supported"
        | "every_matrix_cell_has_rationale"
        | "every_risk_has_mitigation"
        | "min_options"
        | "min_scenarios"
        | "min_evidence_count"
        | "numeric_values_have_rubric"
        | "recommendation_takes_position"
        | "recommendation_not_tautological"
        | "citation_register_complete" => "substantive".to_string(),
        "urls_resolve" | "dois_resolve" | "scope_questions_min" | "disclaimer_present" => {
            "stale".to_string()
        }
        _ => "substantive".to_string(),
    }
}
