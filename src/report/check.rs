//! `report check` — validators that gate render.
//!
//! Every validator returns a [`ValidatorResult`]. Hard validators block the
//! render; soft validators only emit warnings. The blueprint chooses
//! severity per validator name. A run with an empty `report_check_reports`
//! row (or last row's `overall_pass = 0`) cannot be rendered.

use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeSet;

use crate::report::blueprints::{validator_severity, Blueprint, ValidatorSeverity};
use crate::report::claims;
use crate::report::draft;
use crate::report::evidence;
use crate::report::manuscript::{Block, BulletItem, Manuscript, Section};
use crate::report::scope;
use crate::report::scoring;
use crate::report::store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorResult {
    pub name: String,
    pub severity: String, // hard | soft | disabled
    pub pass: bool,
    pub details: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    pub check_id: String,
    pub run_id: String,
    pub version_id: String,
    pub overall_pass: bool,
    pub validators: Vec<ValidatorResult>,
}

pub fn run_check(
    conn: &Connection,
    blueprint: &Blueprint,
    run_id: &str,
    version_id: Option<&str>,
) -> Result<CheckReport> {
    let (version_id, _version_number, _body_hash, manuscript) =
        draft::load_version(conn, run_id, version_id)?;
    let scope_view =
        scope::load_scope(conn, run_id)?.context("scope must exist before running check")?;
    let evidence_rows = evidence::list_evidence(conn, run_id)?;
    let claim_rows = claims::list_claims(conn, run_id)?;
    let risks = claims::list_risks(conn, run_id)?;
    let scenarios = claims::list_scenarios(conn, run_id)?;
    let options = claims::list_options(conn, run_id)?;
    let cells = scoring::list_cells(conn, run_id)?;
    let rubrics = scoring::list_rubrics(conn, run_id)?;

    let mut results: Vec<ValidatorResult> = Vec::new();

    macro_rules! run_validator {
        ($name:expr, $body:expr) => {{
            let severity = validator_severity(blueprint, $name);
            if severity != ValidatorSeverity::Disabled {
                let (pass, details, evidence) = $body;
                results.push(ValidatorResult {
                    name: $name.to_string(),
                    severity: match severity {
                        ValidatorSeverity::Hard => "hard".to_string(),
                        ValidatorSeverity::Soft => "soft".to_string(),
                        ValidatorSeverity::Disabled => "disabled".to_string(),
                    },
                    pass,
                    details,
                    evidence,
                });
            }
        }};
    }

    run_validator!("disclaimer_present", {
        let len = scope_view.disclaimer_md.trim().chars().count();
        let ok = len >= blueprint.bounds.min_disclaimer_chars;
        (
            ok,
            format!(
                "disclaimer length {} chars (min {})",
                len, blueprint.bounds.min_disclaimer_chars
            ),
            vec![],
        )
    });

    run_validator!("scope_questions_min", {
        let n = scope_view.leading_questions.len();
        let ok = n >= blueprint.bounds.min_leading_questions;
        (
            ok,
            format!(
                "{} leading questions (min {})",
                n, blueprint.bounds.min_leading_questions
            ),
            vec![],
        )
    });

    run_validator!("min_evidence_count", {
        let n = evidence_rows.len();
        let ok = n >= blueprint.bounds.min_evidence_count;
        (
            ok,
            format!(
                "{} evidence rows (min {})",
                n, blueprint.bounds.min_evidence_count
            ),
            vec![],
        )
    });

    run_validator!("every_section_present", {
        let manuscript_section_ids: BTreeSet<&str> =
            manuscript.sections.iter().map(|s| s.id.as_str()).collect();
        let mut missing: Vec<&str> = Vec::new();
        for sec in &blueprint.sections {
            if matches!(sec.kind, crate::report::blueprints::SectionKind::Claims)
                && sec.requires_claim
            {
                if !manuscript_section_ids.contains(sec.id.as_str()) {
                    missing.push(sec.id.as_str());
                }
            }
        }
        (
            missing.is_empty(),
            if missing.is_empty() {
                "all required sections present".to_string()
            } else {
                format!("missing sections: {}", missing.join(", "))
            },
            vec![],
        )
    });

    run_validator!("every_claim_has_fk_evidence", {
        let mut bad: Vec<Value> = Vec::new();
        for c in &claim_rows {
            if matches!(
                c.claim_kind.as_str(),
                "finding" | "recommendation" | "caveat"
            ) && c.evidence_ids.is_empty()
            {
                bad.push(json!({
                    "claim_id": c.claim_id,
                    "section_id": c.section_id,
                    "kind": c.claim_kind,
                    "text_excerpt": excerpt(&c.text_md, 120),
                }));
            }
        }
        (
            bad.is_empty(),
            format!("{} unsupported claim(s)", bad.len()),
            bad,
        )
    });

    run_validator!("claim_evidence_relevance", {
        let evidence_index = build_evidence_index(&evidence_rows);
        let option_terms = build_option_terms(&options);
        let mut bad: Vec<Value> = Vec::new();
        for c in &claim_rows {
            for ev_id in &c.evidence_ids {
                if let Some(ev) = evidence_index.get(ev_id.as_str()) {
                    let snippet = combine_searchable(ev);
                    let claim_terms = relevant_terms_for_claim(c, &option_terms);
                    let relevant =
                        claim_terms.is_empty() || claim_terms.iter().any(|t| snippet.contains(t));
                    if !relevant {
                        bad.push(json!({
                            "claim_id": c.claim_id,
                            "evidence_id": ev_id,
                            "checked_terms": claim_terms,
                            "evidence_canonical_id": ev.canonical_id,
                        }));
                    }
                }
            }
        }
        (
            bad.is_empty(),
            format!("{} claim/evidence relevance mismatches", bad.len()),
            bad,
        )
    });

    run_validator!("every_matrix_cell_has_rationale", {
        let bad: Vec<Value> = cells
            .iter()
            .filter(|c| c.rationale_md.trim().chars().count() < 16)
            .map(|c| {
                json!({
                    "cell_id": c.cell_id,
                    "matrix_kind": c.matrix_kind,
                    "option_code": c.option_code,
                    "axis_code": c.axis_code,
                })
            })
            .collect();
        (
            bad.is_empty(),
            format!("{} thin rationales", bad.len()),
            bad,
        )
    });

    run_validator!("every_matrix_cell_supported", {
        let bad: Vec<Value> = cells
            .iter()
            .filter(|c| {
                c.evidence_ids.is_empty()
                    && c.assumption_note_md
                        .as_ref()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
            })
            .map(|c| {
                json!({
                    "cell_id": c.cell_id,
                    "option_code": c.option_code,
                    "axis_code": c.axis_code,
                })
            })
            .collect();
        (
            bad.is_empty(),
            format!(
                "{} matrix cells lack evidence and assumption_note",
                bad.len()
            ),
            bad,
        )
    });

    run_validator!("numeric_values_have_rubric", {
        let mut bad: Vec<Value> = Vec::new();
        for c in &cells {
            if c.value_numeric.is_some() && c.rubric_anchor.is_none() {
                bad.push(json!({
                    "cell_id": c.cell_id,
                    "axis_code": c.axis_code,
                }));
            }
        }
        // Also: every rubric_anchor must point at an existing rubric.
        for c in &cells {
            if let Some(anchor) = c.rubric_anchor.as_deref() {
                let parts: Vec<&str> = anchor.split(':').collect();
                if parts.len() != 3
                    || !rubrics
                        .iter()
                        .any(|r| r.axis_code == parts[1] && r.level_code == parts[2])
                {
                    bad.push(json!({
                        "cell_id": c.cell_id,
                        "rubric_anchor": anchor,
                        "issue": "rubric_anchor does not resolve",
                    }));
                }
            }
        }
        (
            bad.is_empty(),
            format!("{} rubric problems", bad.len()),
            bad,
        )
    });

    run_validator!("every_risk_has_mitigation", {
        let bad: Vec<Value> = risks
            .iter()
            .filter(|r| r.mitigation_md.trim().is_empty())
            .map(|r| json!({"code": r.code}))
            .collect();
        (
            bad.is_empty(),
            format!("{} risks missing mitigation", bad.len()),
            bad,
        )
    });

    run_validator!("min_scenarios", {
        let n = scenarios.len();
        let ok = n >= blueprint.bounds.min_scenarios;
        (
            ok,
            format!("{} scenarios (min {})", n, blueprint.bounds.min_scenarios),
            vec![],
        )
    });

    run_validator!("min_options", {
        let n = options.len();
        let ok = n >= blueprint.bounds.min_options;
        (
            ok,
            format!("{} options (min {})", n, blueprint.bounds.min_options),
            vec![],
        )
    });

    run_validator!("forbid_unicode_dashes", {
        let bad = find_dashes(&manuscript);
        (
            bad.is_empty(),
            format!("{} text fragments with non-ASCII dashes", bad.len()),
            bad,
        )
    });

    run_validator!("forbid_unanchored_hedges", {
        let bad = find_unanchored_hedges(&claim_rows);
        (
            bad.is_empty(),
            format!("{} unanchored hedge phrases", bad.len()),
            bad,
        )
    });

    run_validator!("forbid_filler_phrases", {
        let bad = find_filler_phrases(&manuscript);
        (bad.is_empty(), format!("{} filler phrases", bad.len()), bad)
    });

    run_validator!("recommendation_takes_position", {
        let recs: Vec<&claims::ClaimView> = claim_rows
            .iter()
            .filter(|c| c.section_id == "recommendation")
            .collect();
        let primary_count = recs
            .iter()
            .filter(|c| c.primary_recommendation && c.claim_kind == "recommendation")
            .count();
        let not_recommended = recs
            .iter()
            .filter(|c| {
                c.text_md.to_lowercase().contains("not recommended")
                    || c.text_md.to_lowercase().contains("nicht empfohlen")
            })
            .count();
        let scenario_partitioned = recs.iter().any(|c| c.scenario_code.is_some());
        let ok = primary_count == 1 && (not_recommended >= 1 || scenario_partitioned);
        (
            ok,
            format!(
                "primary={primary_count}, not_recommended={not_recommended}, scenario_partitioned={scenario_partitioned}"
            ),
            vec![],
        )
    });

    run_validator!("recommendation_not_tautological", {
        let recs: Vec<&claims::ClaimView> = claim_rows
            .iter()
            .filter(|c| c.section_id == "recommendation" && c.primary_recommendation)
            .collect();
        let mut bad: Vec<Value> = Vec::new();
        for c in &recs {
            for q in &scope_view.leading_questions {
                let sim = jaccard(&c.text_md, q);
                if sim > 0.5 {
                    bad.push(json!({
                        "claim_id": c.claim_id,
                        "leading_question": q,
                        "jaccard": format!("{sim:.2}"),
                    }));
                }
            }
        }
        (
            bad.is_empty(),
            format!("{} tautological recommendation(s)", bad.len()),
            bad,
        )
    });

    run_validator!("figures_have_captions", {
        // No figures supported in v1; placeholder pass.
        (true, "no figure blocks in this build".to_string(), vec![])
    });

    run_validator!("citation_register_complete", {
        let referenced: BTreeSet<&str> = manuscript
            .sections
            .iter()
            .flat_map(section_evidence_ids)
            .collect();
        let registered: BTreeSet<&str> = manuscript
            .citation_register
            .iter()
            .map(|c| c.evidence_id.as_str())
            .collect();
        let mut missing: Vec<&str> = referenced.difference(&registered).copied().collect();
        missing.sort();
        (
            missing.is_empty(),
            format!("{} cited evidence_ids missing from register", missing.len()),
            missing
                .iter()
                .map(|s| Value::String((*s).to_string()))
                .collect(),
        )
    });

    run_validator!("urls_resolve", {
        // Soft validator: we do not perform live URL probes in this code path
        // (would make tests flaky). The skill helper script `url_resolve.py`
        // is the operator-facing tool. Pass with a "skipped" note.
        (
            true,
            "deferred to scripts/url_resolve.py".to_string(),
            vec![],
        )
    });

    run_validator!("dois_resolve", {
        (
            true,
            "deferred to scripts/doi_resolve.py".to_string(),
            vec![],
        )
    });

    run_validator!("forbid_internal_vocab", {
        let bad = find_internal_vocab(&manuscript);
        (
            bad.is_empty(),
            format!("{} internal-vocabulary leaks", bad.len()),
            bad,
        )
    });

    let overall_pass = results
        .iter()
        .filter(|r| r.severity == "hard")
        .all(|r| r.pass);
    let check_id = store::new_id("chk");
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_check_reports(check_id, run_id, version_id, overall_pass,
            validators_json, created_at)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            check_id,
            run_id,
            version_id,
            overall_pass as i64,
            serde_json::to_string(&results)?,
            now,
        ],
    )
    .context("failed to persist report_check_reports")?;
    if overall_pass {
        crate::report::state_machine::advance_to(
            conn,
            run_id,
            crate::report::state_machine::Status::Checked,
        )?;
        crate::report::runs::set_next_stage(conn, run_id, Some("render"))?;
    }
    Ok(CheckReport {
        check_id,
        run_id: run_id.to_string(),
        version_id,
        overall_pass,
        validators: results,
    })
}

pub fn last_pass(conn: &Connection, run_id: &str, version_id: &str) -> Result<bool> {
    let row: Option<i64> = conn
        .query_row(
            "SELECT overall_pass FROM report_check_reports
             WHERE run_id = ?1 AND version_id = ?2
             ORDER BY created_at DESC LIMIT 1",
            params![run_id, version_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(matches!(row, Some(1)))
}

// ---------- helpers ----------

fn excerpt(s: &str, max: usize) -> String {
    s.chars().take(max).collect::<String>()
}

fn build_evidence_index(
    rows: &[evidence::EvidenceView],
) -> std::collections::HashMap<&str, &evidence::EvidenceView> {
    rows.iter().map(|r| (r.evidence_id.as_str(), r)).collect()
}

fn build_option_terms(options: &[claims::OptionView]) -> Vec<(String, Vec<String>)> {
    options
        .iter()
        .map(|o| {
            let mut terms = vec![o.label.to_lowercase()];
            for s in &o.synonyms {
                terms.push(s.to_lowercase());
            }
            (o.code.to_lowercase(), terms)
        })
        .collect()
}

fn combine_searchable(ev: &evidence::EvidenceView) -> String {
    let mut s = String::new();
    if let Some(t) = &ev.title {
        s.push_str(&t.to_lowercase());
        s.push(' ');
    }
    if let Some(t) = &ev.snippet_md {
        s.push_str(&t.to_lowercase());
        s.push(' ');
    }
    s.push_str(&ev.canonical_id.to_lowercase());
    s
}

fn relevant_terms_for_claim(
    c: &claims::ClaimView,
    option_terms: &[(String, Vec<String>)],
) -> Vec<String> {
    let lowered = c.text_md.to_lowercase();
    let mut hits: Vec<String> = Vec::new();
    for (code, terms) in option_terms {
        for t in terms {
            if lowered.contains(t) {
                hits.push(t.clone());
                hits.push(code.clone());
            }
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

fn find_dashes(m: &Manuscript) -> Vec<Value> {
    let mut bad = Vec::new();
    let mut visit = |where_: &str, text: &str| {
        for ch in text.chars() {
            if matches!(
                ch,
                '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
            ) {
                bad.push(json!({
                    "where": where_,
                    "char": format!("U+{:04X}", ch as u32),
                    "excerpt": excerpt(text, 80),
                }));
                return;
            }
        }
    };
    visit("disclaimer", &m.scope.disclaimer_md);
    for q in &m.scope.leading_questions {
        visit("leading_question", q);
    }
    for sec in &m.sections {
        for block in &sec.blocks {
            visit_block_text(block, &mut |where_, t| {
                visit(&format!("{}/{}", sec.id, where_), t);
            });
        }
    }
    bad
}

const FILLER_PHRASES_DE: &[&str] = &[
    "im folgenden werden",
    "es ist anzumerken, dass",
    "im rahmen dieser analyse",
    "die folgenden abschnitte beleuchten",
    "diesbezüglich ist festzuhalten",
    "vor dem hintergrund",
    "an dieser stelle sei erwähnt",
];

const FILLER_PHRASES_EN: &[&str] = &[
    "in the following sections",
    "it is worth noting that",
    "as part of this analysis",
    "the following sections illuminate",
    "it should be mentioned at this point",
    "against this backdrop",
    "in this context",
];

fn find_filler_phrases(m: &Manuscript) -> Vec<Value> {
    let mut bad = Vec::new();
    for sec in &m.sections {
        for block in &sec.blocks {
            visit_block_text(block, &mut |where_, t| {
                let lower = t.to_lowercase();
                for needle in FILLER_PHRASES_DE.iter().chain(FILLER_PHRASES_EN.iter()) {
                    if lower.contains(needle) {
                        bad.push(json!({
                            "section": sec.id,
                            "where": where_,
                            "phrase": needle,
                            "excerpt": excerpt(t, 120),
                        }));
                    }
                }
            });
        }
    }
    bad
}

const HEDGE_TERMS: &[&str] = &[
    "könnte",
    "möglicherweise",
    "in bestimmten fällen",
    "tendenziell",
    "vielleicht",
    "in der regel",
    "may potentially",
    "could possibly",
    "in some cases",
    "generally speaking",
    "tend to",
];

fn find_unanchored_hedges(claims: &[claims::ClaimView]) -> Vec<Value> {
    let mut bad = Vec::new();
    for c in claims {
        let lower = c.text_md.to_lowercase();
        for needle in HEDGE_TERMS {
            if lower.contains(needle) {
                let anchored = matches!(c.confidence.as_deref(), Some("low") | Some("medium"))
                    || c.assumption_note_md
                        .as_ref()
                        .map(|s| !s.trim().is_empty())
                        .unwrap_or(false);
                if !anchored {
                    bad.push(json!({
                        "claim_id": c.claim_id,
                        "section_id": c.section_id,
                        "phrase": needle,
                        "excerpt": excerpt(&c.text_md, 120),
                    }));
                }
            }
        }
    }
    bad
}

const INTERNAL_VOCAB: &[&str] = &["ctox", "sqlite", "tui", "queue task", "harness", "kernel"];

fn find_internal_vocab(m: &Manuscript) -> Vec<Value> {
    let mut bad = Vec::new();
    for sec in &m.sections {
        if sec.id == "appendix_sources" {
            continue;
        }
        for block in &sec.blocks {
            visit_block_text(block, &mut |where_, t| {
                let lower = t.to_lowercase();
                for needle in INTERNAL_VOCAB {
                    if lower.contains(needle) {
                        bad.push(json!({
                            "section": sec.id,
                            "where": where_,
                            "phrase": needle,
                            "excerpt": excerpt(t, 120),
                        }));
                    }
                }
            });
        }
    }
    bad
}

fn visit_block_text<F: FnMut(&str, &str)>(block: &Block, visit: &mut F) {
    match block {
        Block::Paragraph { text_md, .. } => visit("paragraph", text_md),
        Block::Bullets { items } | Block::Numbered { items } => {
            for (i, it) in items.iter().enumerate() {
                visit_bullet(visit, i, it);
            }
        }
        Block::OptionsTable { options } => {
            for o in options {
                if let Some(s) = &o.summary_md {
                    visit("option_summary", s);
                }
            }
        }
        Block::RequirementsTable { rows } => {
            for r in rows {
                if let Some(s) = &r.description_md {
                    visit("requirement_description", s);
                }
            }
        }
        Block::MatrixTable { rows, .. } => {
            for r in rows {
                for c in &r.cells {
                    visit("matrix_rationale", &c.rationale_md);
                    if let Some(a) = &c.assumption_note_md {
                        visit("matrix_assumption", a);
                    }
                }
            }
        }
        Block::ScenarioBlock { description_md, .. } => {
            visit("scenario_description", description_md)
        }
        Block::RiskRegister { rows } => {
            for r in rows {
                visit("risk_description", &r.description_md);
                visit("risk_mitigation", &r.mitigation_md);
            }
        }
        Block::CitationRegister | Block::Note { .. } => {}
    }
}

fn visit_bullet<F: FnMut(&str, &str)>(visit: &mut F, idx: usize, item: &BulletItem) {
    visit(&format!("bullet[{idx}]"), &item.text_md);
    if let Some(a) = &item.assumption_note_md {
        visit(&format!("bullet[{idx}].assumption"), a);
    }
}

fn section_evidence_ids(sec: &Section) -> Vec<&str> {
    let mut out = Vec::new();
    for block in &sec.blocks {
        match block {
            Block::Paragraph { evidence_ids, .. } => {
                out.extend(evidence_ids.iter().map(|s| s.as_str()))
            }
            Block::Bullets { items } | Block::Numbered { items } => {
                for it in items {
                    out.extend(it.evidence_ids.iter().map(|s| s.as_str()));
                }
            }
            Block::MatrixTable { rows, .. } => {
                for r in rows {
                    for c in &r.cells {
                        out.extend(c.evidence_ids.iter().map(|s| s.as_str()));
                    }
                }
            }
            Block::RiskRegister { rows } => {
                for r in rows {
                    out.extend(r.evidence_ids.iter().map(|s| s.as_str()));
                }
            }
            _ => {}
        }
    }
    out
}

fn jaccard(a: &str, b: &str) -> f64 {
    let aw: BTreeSet<String> = a
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(|w| w.to_string())
        .collect();
    let bw: BTreeSet<String> = b
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(|w| w.to_string())
        .collect();
    if aw.is_empty() && bw.is_empty() {
        return 0.0;
    }
    let inter = aw.intersection(&bw).count() as f64;
    let union = aw.union(&bw).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}
