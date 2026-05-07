//! `report draft` — deterministic manuscript assembler.
//!
//! Reads typed DB rows for the run, projects them into a `Manuscript v1`
//! structure, and writes a new `report_versions` row. Pure transformation —
//! no LLM calls, no free prose generation. If a section has no rows, it is
//! omitted (not filled with hot air).

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::report::blueprints::{Blueprint, SectionKind};
use crate::report::claims;
use crate::report::evidence;
use crate::report::manuscript;
use crate::report::manuscript::{
    Block, BulletItem, Citation, Manuscript, MatrixAxis, MatrixCell, MatrixRow, OptionRow,
    RequirementRow, RiskRow, ScopeBlock, Section,
};
use crate::report::scope;
use crate::report::scoring;
use crate::report::state_machine::{self, Status};
use crate::report::store;

pub fn draft_run(conn: &Connection, blueprint: &Blueprint, run_id: &str) -> Result<DraftOutput> {
    state_machine::require_at_least(conn, run_id, Status::Scenarios)?;
    let run = crate::report::runs::load_run(conn, run_id)?
        .with_context(|| format!("run not found: {run_id}"))?;
    let scope_view = scope::load_scope(conn, run_id)?
        .context("cannot draft: scope is unset (run `report scope` first)")?;

    // Collect typed inputs.
    let options = claims::list_options(conn, run_id)?;
    if options.len() < blueprint.bounds.min_options {
        bail!(
            "blueprint requires at least {} options to draft, found {}",
            blueprint.bounds.min_options,
            options.len()
        );
    }
    let requirements = claims::list_requirements(conn, run_id)?;
    let scenarios = claims::list_scenarios(conn, run_id)?;
    if scenarios.len() < blueprint.bounds.min_scenarios {
        bail!(
            "blueprint requires at least {} scenarios to draft, found {}",
            blueprint.bounds.min_scenarios,
            scenarios.len()
        );
    }
    let risks = claims::list_risks(conn, run_id)?;
    let cells = scoring::list_cells(conn, run_id)?;
    let claim_rows = claims::list_claims(conn, run_id)?;
    let evidence_rows = evidence::list_evidence(conn, run_id)?;
    if evidence_rows.len() < blueprint.bounds.min_evidence_count {
        bail!(
            "blueprint requires at least {} evidence rows to draft, found {}",
            blueprint.bounds.min_evidence_count,
            evidence_rows.len()
        );
    }

    let title = match run.language.as_str() {
        "de" => blueprint.title_de.clone(),
        _ => blueprint.title_en.clone(),
    };
    let subtitle = Some(run.topic.clone());
    let version_label = format!("Working draft | {}", run.updated_at);

    let scope_block = ScopeBlock {
        leading_questions: scope_view.leading_questions.clone(),
        out_of_scope: scope_view.out_of_scope.clone(),
        assumptions: scope_view.assumptions.clone(),
        disclaimer_md: scope_view.disclaimer_md.clone(),
        success_criteria: scope_view.success_criteria.clone(),
    };

    let mut manuscript = Manuscript::new(
        run.run_id.clone(),
        run.preset.clone(),
        run.language.clone(),
        title,
        subtitle,
        version_label,
        scope_block,
    );

    let claims_by_section = group_claims_by_section(&claim_rows);

    for section_def in &blueprint.sections {
        let heading = heading_for(&run.language, &section_def.id);
        let mut blocks: Vec<Block> = Vec::new();
        match section_def.kind {
            SectionKind::Deterministic => {
                deterministic_section(
                    &mut blocks,
                    &section_def.id,
                    &options,
                    &requirements,
                    &scenarios,
                    &scope_view.leading_questions,
                );
                if blocks.is_empty() && section_def.id != "title_block" {
                    continue;
                }
            }
            SectionKind::Claims => {
                let entries = claims_by_section.get(&section_def.id);
                if section_def.requires_claim {
                    let count = entries.map(|e| e.len()).unwrap_or(0);
                    if count < section_def.min_claims.max(1) {
                        bail!(
                            "section '{}' requires at least {} claim row(s); found {}",
                            section_def.id,
                            section_def.min_claims.max(1),
                            count
                        );
                    }
                    if section_def.min_claims_per_option > 0 {
                        for opt in &options {
                            let mentions = entries
                                .map(|list| {
                                    list.iter()
                                        .filter(|c| {
                                            c.text_md
                                                .to_lowercase()
                                                .contains(&opt.label.to_lowercase())
                                                || opt.synonyms.iter().any(|s| {
                                                    c.text_md
                                                        .to_lowercase()
                                                        .contains(&s.to_lowercase())
                                                })
                                        })
                                        .count()
                                })
                                .unwrap_or(0);
                            if mentions < section_def.min_claims_per_option {
                                bail!(
                                    "section '{}' requires at least {} claim(s) mentioning option '{}'; found {}",
                                    section_def.id,
                                    section_def.min_claims_per_option,
                                    opt.code,
                                    mentions
                                );
                            }
                        }
                    }
                }
                if let Some(list) = entries {
                    let bullets = list
                        .iter()
                        .map(|c| BulletItem {
                            text_md: c.text_md.clone(),
                            evidence_ids: c.evidence_ids.clone(),
                            primary_recommendation: c.primary_recommendation,
                            assumption_note_md: c.assumption_note_md.clone(),
                            scenario_code: c.scenario_code.clone(),
                        })
                        .collect();
                    blocks.push(Block::Bullets { items: bullets });
                } else {
                    continue;
                }
            }
            SectionKind::Matrix => {
                let kind = section_def
                    .matrix_kind
                    .as_deref()
                    .with_context(|| format!("section '{}' missing matrix_kind", section_def.id))?;
                let matrix_block = build_matrix(&options, &cells, kind, blueprint, &run.language);
                if let Some(block) = matrix_block {
                    blocks.push(block);
                } else {
                    continue;
                }
            }
            SectionKind::RiskRegister => {
                if risks.is_empty() {
                    continue;
                }
                blocks.push(Block::RiskRegister {
                    rows: risks
                        .iter()
                        .map(|r| RiskRow {
                            code: r.code.clone(),
                            title: r.title.clone(),
                            description_md: r.description_md.clone(),
                            mitigation_md: r.mitigation_md.clone(),
                            likelihood: r.likelihood.clone(),
                            impact: r.impact.clone(),
                            evidence_ids: r.evidence_ids.clone(),
                        })
                        .collect(),
                });
            }
            SectionKind::CitationRegister => {
                blocks.push(Block::CitationRegister);
            }
        }
        if !blocks.is_empty() {
            manuscript.sections.push(Section {
                id: section_def.id.clone(),
                heading_level: section_def.heading_level.max(1),
                heading,
                blocks,
            });
        }
    }

    // Citation register: every evidence row that is referenced by any claim
    // / matrix cell / risk gets a numbered display entry.
    let referenced = collect_referenced_evidence_ids(&claim_rows, &cells, &risks);
    let mut filtered: Vec<&evidence::EvidenceView> = evidence_rows
        .iter()
        .filter(|e| referenced.contains(e.evidence_id.as_str()))
        .collect();
    filtered.sort_by(|a, b| a.created_at_key().cmp(&b.created_at_key()));
    manuscript.citation_register = filtered
        .into_iter()
        .enumerate()
        .map(|(idx, ev)| Citation {
            evidence_id: ev.evidence_id.clone(),
            display_index: idx + 1,
            citation_kind: ev.citation_kind.clone(),
            canonical_id: ev.canonical_id.clone(),
            title: ev.title.clone(),
            authors: ev.authors.clone(),
            venue: ev.venue.clone(),
            year: ev.year,
            landing_url: ev.landing_url.clone(),
            full_text_url: ev.full_text_url.clone(),
        })
        .collect();

    let manuscript_json = serde_json::to_string(&manuscript).context("serialise manuscript v1")?;
    let body_hash = manuscript::body_hash(&manuscript);
    let version_id = store::new_id("ver");
    let now = store::now_iso();
    let version_number: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version_number),0)+1 FROM report_versions WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .unwrap_or(1);
    let parent_version_id: Option<String> = conn
        .query_row(
            "SELECT version_id FROM report_versions WHERE run_id = ?1
             ORDER BY version_number DESC LIMIT 1",
            params![run_id],
            |row| row.get(0),
        )
        .unwrap_or(None);
    conn.execute(
        "INSERT INTO report_versions(version_id, run_id, version_number, parent_version_id,
            manuscript_json, body_hash, produced_by, notes_md, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,'draft',NULL,?7)",
        params![
            version_id,
            run_id,
            version_number,
            parent_version_id,
            manuscript_json,
            body_hash,
            now,
        ],
    )
    .context("failed to insert report_versions")?;
    state_machine::advance_to(conn, run_id, Status::Drafting)?;
    crate::report::runs::set_next_stage(conn, run_id, Some("check"))?;
    Ok(DraftOutput {
        version_id,
        version_number,
        body_hash,
        manuscript,
    })
}

#[derive(Debug)]
pub struct DraftOutput {
    pub version_id: String,
    pub version_number: i64,
    pub body_hash: String,
    pub manuscript: Manuscript,
}

impl DraftOutput {
    pub fn payload(&self) -> Value {
        json!({
            "ok": true,
            "version_id": self.version_id,
            "version_number": self.version_number,
            "body_hash": self.body_hash,
            "section_count": self.manuscript.sections.len(),
            "citation_count": self.manuscript.citation_register.len(),
        })
    }
}

pub fn load_version(
    conn: &Connection,
    run_id: &str,
    version_id: Option<&str>,
) -> Result<(String, i64, String, Manuscript)> {
    let row: (String, i64, String, String) = if let Some(vid) = version_id {
        conn.query_row(
            "SELECT version_id, version_number, body_hash, manuscript_json
             FROM report_versions WHERE run_id = ?1 AND version_id = ?2",
            params![run_id, vid],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?
    } else {
        conn.query_row(
            "SELECT version_id, version_number, body_hash, manuscript_json
             FROM report_versions WHERE run_id = ?1
             ORDER BY version_number DESC LIMIT 1",
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?
    };
    let manuscript: Manuscript = serde_json::from_str(&row.3).context("parse manuscript_json")?;
    Ok((row.0, row.1, row.2, manuscript))
}

fn deterministic_section(
    blocks: &mut Vec<Block>,
    section_id: &str,
    options: &[claims::OptionView],
    requirements: &[claims::RequirementView],
    scenarios: &[claims::ScenarioView],
    leading_questions: &[String],
) {
    match section_id {
        "title_block" | "scope_disclaimer" => {
            // these are handled by the renderer using Manuscript.scope and metadata;
            // emit a Note block so the section is preserved.
            blocks.push(Block::Note {
                text_md: String::new(),
            });
        }
        "requirements" => {
            if !requirements.is_empty() {
                blocks.push(Block::RequirementsTable {
                    rows: requirements
                        .iter()
                        .map(|r| RequirementRow {
                            code: r.code.clone(),
                            title: r.title.clone(),
                            must_have: r.must_have,
                            description_md: r.description_md.clone(),
                        })
                        .collect(),
                });
            }
        }
        "options_overview" => {
            if !options.is_empty() {
                blocks.push(Block::OptionsTable {
                    options: options
                        .iter()
                        .map(|o| OptionRow {
                            code: o.code.clone(),
                            label: o.label.clone(),
                            summary_md: o.summary_md.clone(),
                        })
                        .collect(),
                });
            }
        }
        "context_and_question" => {
            if !leading_questions.is_empty() {
                blocks.push(Block::Numbered {
                    items: leading_questions
                        .iter()
                        .map(|q| BulletItem {
                            text_md: q.clone(),
                            evidence_ids: Vec::new(),
                            primary_recommendation: false,
                            assumption_note_md: None,
                            scenario_code: None,
                        })
                        .collect(),
                });
            }
            if !scenarios.is_empty() {
                for s in scenarios {
                    blocks.push(Block::ScenarioBlock {
                        code: s.code.clone(),
                        label: s.label.clone(),
                        description_md: s.description_md.clone(),
                    });
                }
            }
        }
        _ => {}
    }
}

fn build_matrix(
    options: &[claims::OptionView],
    cells: &[scoring::CellView],
    matrix_kind: &str,
    blueprint: &Blueprint,
    language: &str,
) -> Option<Block> {
    let kind_cells: Vec<&scoring::CellView> = cells
        .iter()
        .filter(|c| c.matrix_kind == matrix_kind)
        .collect();
    if kind_cells.is_empty() {
        return None;
    }
    let mut axes: Vec<MatrixAxis> = Vec::new();
    if let Some(def) = blueprint.matrices.get(matrix_kind) {
        for (idx, code) in def.axis_codes.iter().enumerate() {
            let label = def
                .axis_labels_en
                .get(idx)
                .cloned()
                .unwrap_or_else(|| code.clone());
            axes.push(MatrixAxis {
                code: code.clone(),
                label,
            });
        }
    }
    if axes.is_empty() {
        let mut seen = std::collections::BTreeSet::new();
        for c in &kind_cells {
            seen.insert(c.axis_code.clone());
        }
        for code in seen {
            axes.push(MatrixAxis {
                code: code.clone(),
                label: code,
            });
        }
    }
    let mut rows: Vec<MatrixRow> = Vec::new();
    for opt in options {
        let mut option_cells: Vec<MatrixCell> = Vec::new();
        for axis in &axes {
            if let Some(cell) = kind_cells
                .iter()
                .find(|c| c.option_code == opt.code && c.axis_code == axis.code)
            {
                option_cells.push(MatrixCell {
                    axis_code: cell.axis_code.clone(),
                    value_label: cell.value_label.clone(),
                    value_numeric: cell.value_numeric,
                    rationale_md: cell.rationale_md.clone(),
                    evidence_ids: cell.evidence_ids.clone(),
                    assumption_note_md: cell.assumption_note_md.clone(),
                });
            }
        }
        if !option_cells.is_empty() {
            rows.push(MatrixRow {
                option_code: opt.code.clone(),
                option_label: opt.label.clone(),
                cells: option_cells,
            });
        }
    }
    if rows.is_empty() {
        return None;
    }
    let label = blueprint
        .matrices
        .get(matrix_kind)
        .map(|d| match (language, d.label_de.as_deref()) {
            ("de", Some(de)) => de.to_string(),
            _ => d.label_en.clone(),
        })
        .unwrap_or_else(|| matrix_kind.to_string());
    Some(Block::MatrixTable {
        matrix_kind: matrix_kind.to_string(),
        label,
        axes,
        rows,
    })
}

fn group_claims_by_section(
    list: &[claims::ClaimView],
) -> BTreeMap<String, Vec<&claims::ClaimView>> {
    let mut out: BTreeMap<String, Vec<&claims::ClaimView>> = BTreeMap::new();
    for c in list {
        out.entry(c.section_id.clone()).or_default().push(c);
    }
    for v in out.values_mut() {
        v.sort_by_key(|c| c.position);
    }
    out
}

fn collect_referenced_evidence_ids<'a>(
    claim_rows: &'a [claims::ClaimView],
    cells: &'a [scoring::CellView],
    risks: &'a [claims::RiskView],
) -> std::collections::BTreeSet<&'a str> {
    let mut set: std::collections::BTreeSet<&'a str> = std::collections::BTreeSet::new();
    for c in claim_rows {
        for id in &c.evidence_ids {
            set.insert(id.as_str());
        }
    }
    for c in cells {
        for id in &c.evidence_ids {
            set.insert(id.as_str());
        }
    }
    for r in risks {
        for id in &r.evidence_ids {
            set.insert(id.as_str());
        }
    }
    set
}

fn heading_for(language: &str, section_id: &str) -> String {
    let (en, de) = match section_id {
        "title_block" => ("", ""),
        "scope_disclaimer" => ("Scope and Disclaimer", "Geltungsbereich und Hinweis"),
        "management_summary" => ("Management Summary", "Management Summary"),
        "context_and_question" => ("Context and Leading Questions", "Kontext und Leitfragen"),
        "requirements" => (
            "Requirements and Constraints",
            "Anforderungen und Randbedingungen",
        ),
        "options_overview" => ("Options Overview", "Optionsübersicht"),
        "main_matrix" => (
            "Qualitative Evaluation Matrix",
            "Qualitative Bewertungsmatrix",
        ),
        "scenario_matrix" => ("Outcome by Scenario", "Erfolgsaussichten nach Szenario"),
        "detail_assessment" => (
            "Detailed Assessment per Option",
            "Detailbewertung pro Option",
        ),
        "risks" => ("Risks and Mitigation", "Risiken und Mitigation"),
        "recommendation" => ("Recommendation", "Empfehlung"),
        "appendix_sources" => ("Appendix A: Sources", "Anhang A: Quellen"),
        _ => (section_id, section_id),
    };
    if language == "de" {
        de.to_string()
    } else {
        en.to_string()
    }
}

// Helpers used for stable citation ordering in the register.
trait EvidenceCreatedKey {
    fn created_at_key(&self) -> String;
}

impl EvidenceCreatedKey for evidence::EvidenceView {
    fn created_at_key(&self) -> String {
        // EvidenceView intentionally does not carry created_at (slim view); use
        // (citation_kind, canonical_id) for deterministic ordering.
        format!("{}|{}", self.citation_kind, self.canonical_id)
    }
}
