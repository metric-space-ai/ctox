//! Regression tests that reproduce the failure modes of the disabled
//! deep-research skill. Each test seeds a state in which the dead skill
//! would have produced slop, and asserts the new pipeline rejects it
//! before any rendered document escapes.

use rusqlite::params;
use serde_json::json;
use serde_json::Value;
use tempfile::tempdir;

use crate::report::blueprints;
use crate::report::check;
use crate::report::claims;
use crate::report::draft;
use crate::report::evidence;
use crate::report::manuscript;
use crate::report::manuscript::{Block, BulletItem, Manuscript, Section};
use crate::report::scope;
use crate::report::scoring;
use crate::report::store;

fn seed_run(conn: &rusqlite::Connection, language: &str) -> String {
    let now = store::now_iso();
    let run_id = store::new_id("run");
    conn.execute(
        "INSERT INTO report_runs(run_id, preset, blueprint_version, topic, language,
            status, created_at, updated_at)
         VALUES(?1,'feasibility','1','Topic for slop regression',?2,'created',?3,?3)",
        params![run_id, language, now],
    )
    .unwrap();
    run_id
}

fn long_disclaimer() -> String {
    "Scope and limitations: this study draws on public sources and explicit \
     assumptions about the target system. The findings require validation \
     in a representative coupon campaign before any production adoption."
        .to_string()
}

fn seed_evidence(conn: &rusqlite::Connection, run_id: &str, count: usize) -> Vec<String> {
    let mut ids = Vec::new();
    for i in 0..count {
        let input = evidence::EvidenceInput {
            citation_kind: "doi".to_string(),
            canonical_id: format!("10.0000/test.{i}"),
            title: Some(format!(
                "Eddy Current Inspection of Lightning Strike Protection {i}"
            )),
            authors: vec!["Author X".to_string()],
            venue: Some("Test Journal".to_string()),
            year: Some(2024),
            publisher: None,
            landing_url: Some(format!("https://example.test/{i}")),
            full_text_url: None,
            abstract_md: None,
            snippet_md: Some(
                "eddy current testing of copper lightning strike protection grids \
                 detects defects via impedance change."
                    .to_string(),
            ),
            license: None,
            resolver: Some("manual".to_string()),
        };
        ids.push(
            evidence::upsert_evidence(conn, run_id, &input)
                .unwrap()
                .evidence_id,
        );
    }
    ids
}

#[test]
fn unsupported_recommendation_fails_check() {
    // The dead skill emitted "verdict" strings without source backing.
    // Adding a recommendation claim with empty evidence_ids must be
    // rejected at insert time, not at render time.
    let dir = tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let run_id = seed_run(&conn, "en");
    let blueprint = blueprints::load("feasibility").unwrap();
    let scope_input = scope::ScopeInput {
        leading_questions: vec!["A?".to_string(), "B?".to_string()],
        out_of_scope: vec![],
        assumptions: vec![],
        disclaimer_md: long_disclaimer(),
        success_criteria: vec![],
    };
    scope::upsert_scope(&conn, &blueprint, &run_id, &scope_input).unwrap();
    let bad = claims::ClaimInput {
        section_id: "recommendation".to_string(),
        text_md: "Eddy Current is the most promising contactless candidate.".to_string(),
        claim_kind: "recommendation".to_string(),
        evidence_ids: vec![],
        assumption_note_md: None,
        confidence: None,
        primary_recommendation: true,
        scenario_code: None,
        rubric_anchor: None,
    };
    let err = claims::add_claim(&conn, &blueprint, &run_id, &bad).unwrap_err();
    assert!(format!("{err:?}").contains("evidence_id"));
}

#[test]
fn matrix_cell_without_rubric_fails() {
    // The dead skill produced numeric scores like "success: 4" without any
    // rubric defining what 4 means. We require a rubric before any value.
    let dir = tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let run_id = seed_run(&conn, "en");
    let blueprint = blueprints::load("feasibility").unwrap();
    scope::upsert_scope(
        &conn,
        &blueprint,
        &run_id,
        &scope::ScopeInput {
            leading_questions: vec!["A?".into(), "B?".into()],
            out_of_scope: vec![],
            assumptions: vec![],
            disclaimer_md: long_disclaimer(),
            success_criteria: vec![],
        },
    )
    .unwrap();
    claims::upsert_option(
        &conn,
        &run_id,
        &claims::OptionInput {
            code: "ECT".into(),
            label: "Eddy Current".into(),
            summary_md: None,
            synonyms: vec!["eddy".into()],
        },
    )
    .unwrap();
    let ev_ids = seed_evidence(&conn, &run_id, 8);
    let bad = scoring::CellInput {
        matrix_kind: "main".into(),
        matrix_label: None,
        option_code: "ECT".into(),
        axis_code: "coverage".into(),
        value_label: "4".into(),
        rationale_md: "Looks high enough to be promising overall.".into(),
        evidence_ids: vec![ev_ids[0].clone()],
        assumption_note_md: None,
        rubric_anchor: None,
    };
    let err = scoring::upsert_cell(&conn, &run_id, &bad).unwrap_err();
    assert!(format!("{err:?}").contains("rubric"));
}

#[test]
fn unanchored_hedge_in_claim_fails_check() {
    let dir = tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let run_id = seed_run(&conn, "en");
    let blueprint = blueprints::load("feasibility").unwrap();
    // Build a real version with a hedged claim, then run check and assert
    // the hedge validator surfaces it.
    let manuscript = Manuscript {
        schema: manuscript::MANUSCRIPT_SCHEMA_VERSION.to_string(),
        run_id: run_id.clone(),
        preset: "feasibility".into(),
        language: "en".into(),
        title: "T".into(),
        subtitle: None,
        version_label: "v0".into(),
        scope: manuscript::ScopeBlock {
            leading_questions: vec!["A?".into(), "B?".into()],
            out_of_scope: vec![],
            assumptions: vec![],
            disclaimer_md: long_disclaimer(),
            success_criteria: vec![],
        },
        sections: vec![Section {
            id: "management_summary".into(),
            heading_level: 1,
            heading: "Management Summary".into(),
            blocks: vec![Block::Bullets {
                items: vec![BulletItem {
                    text_md: "Eddy current may potentially detect grid defects in some cases."
                        .into(),
                    evidence_ids: vec![],
                    primary_recommendation: false,
                    assumption_note_md: None,
                    scenario_code: None,
                }],
            }],
        }],
        citation_register: vec![],
    };
    // Insert a faux scope row directly to satisfy the validator dependency.
    scope::upsert_scope(
        &conn,
        &blueprint,
        &run_id,
        &scope::ScopeInput {
            leading_questions: vec!["A?".into(), "B?".into()],
            out_of_scope: vec![],
            assumptions: vec![],
            disclaimer_md: long_disclaimer(),
            success_criteria: vec![],
        },
    )
    .unwrap();
    // Direct insert into report_versions for this targeted test.
    let mj = serde_json::to_string(&manuscript).unwrap();
    conn.execute(
        "INSERT INTO report_versions(version_id, run_id, version_number, parent_version_id,
            manuscript_json, body_hash, produced_by, created_at)
         VALUES('ver_t1',?1,1,NULL,?2,'h','draft',?3)",
        params![run_id, mj, store::now_iso()],
    )
    .unwrap();
    // Insert one matching claim row so the hedge validator has DB material.
    conn.execute(
        "INSERT INTO report_claims(claim_id, run_id, section_id, position, text_md, claim_kind,
            confidence, evidence_ids_json, primary_recommendation, created_at)
         VALUES('cl_t1',?1,'management_summary',1,
                'Eddy current may potentially detect grid defects in some cases.',
                'caveat',NULL,'[]',0,?2)",
        params![run_id, store::now_iso()],
    )
    .unwrap();
    let report = check::run_check(&conn, &blueprint, &run_id, Some("ver_t1")).unwrap();
    let hedge = report
        .validators
        .iter()
        .find(|v| v.name == "forbid_unanchored_hedges")
        .unwrap();
    assert!(!hedge.pass, "hedge validator must fail on unanchored hedge");
    let claim = report
        .validators
        .iter()
        .find(|v| v.name == "every_claim_has_fk_evidence")
        .unwrap();
    assert!(!claim.pass, "caveat without evidence must also fail");
    assert!(!report.overall_pass);
}

#[test]
fn filler_phrase_fails_check() {
    let dir = tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let run_id = seed_run(&conn, "en");
    let blueprint = blueprints::load("feasibility").unwrap();
    scope::upsert_scope(
        &conn,
        &blueprint,
        &run_id,
        &scope::ScopeInput {
            leading_questions: vec!["A?".into(), "B?".into()],
            out_of_scope: vec![],
            assumptions: vec![],
            disclaimer_md: long_disclaimer(),
            success_criteria: vec![],
        },
    )
    .unwrap();
    let manuscript = Manuscript {
        schema: manuscript::MANUSCRIPT_SCHEMA_VERSION.to_string(),
        run_id: run_id.clone(),
        preset: "feasibility".into(),
        language: "en".into(),
        title: "T".into(),
        subtitle: None,
        version_label: "v0".into(),
        scope: manuscript::ScopeBlock {
            leading_questions: vec!["A?".into(), "B?".into()],
            out_of_scope: vec![],
            assumptions: vec![],
            disclaimer_md: long_disclaimer(),
            success_criteria: vec![],
        },
        sections: vec![Section {
            id: "management_summary".into(),
            heading_level: 1,
            heading: "Management Summary".into(),
            blocks: vec![Block::Paragraph {
                text_md: "In the following sections we explore the feasibility space.".into(),
                evidence_ids: vec![],
            }],
        }],
        citation_register: vec![],
    };
    let mj = serde_json::to_string(&manuscript).unwrap();
    conn.execute(
        "INSERT INTO report_versions(version_id, run_id, version_number, parent_version_id,
            manuscript_json, body_hash, produced_by, created_at)
         VALUES('ver_t2',?1,1,NULL,?2,'h2','draft',?3)",
        params![run_id, mj, store::now_iso()],
    )
    .unwrap();
    let report = check::run_check(&conn, &blueprint, &run_id, Some("ver_t2")).unwrap();
    let filler = report
        .validators
        .iter()
        .find(|v| v.name == "forbid_filler_phrases")
        .unwrap();
    assert!(!filler.pass);
}

#[test]
fn revise_without_progress_witness_fails() {
    let dir = tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let run_id = seed_run(&conn, "en");
    // Insert a v1 manuscript directly.
    let m = Manuscript {
        schema: manuscript::MANUSCRIPT_SCHEMA_VERSION.to_string(),
        run_id: run_id.clone(),
        preset: "feasibility".into(),
        language: "en".into(),
        title: "T".into(),
        subtitle: None,
        version_label: "v0".into(),
        scope: manuscript::ScopeBlock {
            leading_questions: vec![],
            out_of_scope: vec![],
            assumptions: vec![],
            disclaimer_md: "ok".into(),
            success_criteria: vec![],
        },
        sections: vec![],
        citation_register: vec![],
    };
    let mj = serde_json::to_string(&m).unwrap();
    let hash = manuscript::body_hash(&m);
    conn.execute(
        "INSERT INTO report_versions(version_id, run_id, version_number, parent_version_id,
            manuscript_json, body_hash, produced_by, created_at)
         VALUES('ver_p1',?1,1,NULL,?2,?3,'draft',?4)",
        params![run_id, mj, hash, store::now_iso()],
    )
    .unwrap();
    // Move run to drafting so reenter_revise is allowed.
    conn.execute(
        "UPDATE report_runs SET status='drafting' WHERE run_id = ?1",
        params![run_id],
    )
    .unwrap();
    let revise_input = crate::report::critique::ReviseInput {
        from_version_id: Some("ver_p1".into()),
        manuscript: m.clone(),
        notes_md: None,
    };
    let err = crate::report::critique::revise(&conn, &run_id, &revise_input).unwrap_err();
    assert!(format!("{err:?}").contains("witness"));
}

#[test]
fn render_refuses_without_passing_check() {
    let dir = tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let run_id = seed_run(&conn, "en");
    let m = Manuscript {
        schema: manuscript::MANUSCRIPT_SCHEMA_VERSION.to_string(),
        run_id: run_id.clone(),
        preset: "feasibility".into(),
        language: "en".into(),
        title: "T".into(),
        subtitle: None,
        version_label: "v0".into(),
        scope: manuscript::ScopeBlock {
            leading_questions: vec![],
            out_of_scope: vec![],
            assumptions: vec![],
            disclaimer_md: "ok".into(),
            success_criteria: vec![],
        },
        sections: vec![],
        citation_register: vec![],
    };
    let mj = serde_json::to_string(&m).unwrap();
    conn.execute(
        "INSERT INTO report_versions(version_id, run_id, version_number, parent_version_id,
            manuscript_json, body_hash, produced_by, created_at)
         VALUES('ver_r1',?1,1,NULL,?2,'h','draft',?3)",
        params![run_id, mj, store::now_iso()],
    )
    .unwrap();
    let err = crate::report::render::render(
        &conn,
        dir.path(),
        &run_id,
        Some("ver_r1"),
        "md",
        None,
        false,
    )
    .unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("check"),
        "expected 'check' in error, got: {msg}"
    );
}

#[test]
fn forbid_unicode_dashes_catches_em_dash() {
    let dir = tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let run_id = seed_run(&conn, "en");
    let blueprint = blueprints::load("feasibility").unwrap();
    scope::upsert_scope(
        &conn,
        &blueprint,
        &run_id,
        &scope::ScopeInput {
            leading_questions: vec!["A?".into(), "B?".into()],
            out_of_scope: vec![],
            assumptions: vec![],
            disclaimer_md: long_disclaimer(),
            success_criteria: vec![],
        },
    )
    .unwrap();
    let m = Manuscript {
        schema: manuscript::MANUSCRIPT_SCHEMA_VERSION.to_string(),
        run_id: run_id.clone(),
        preset: "feasibility".into(),
        language: "en".into(),
        title: "T".into(),
        subtitle: None,
        version_label: "v0".into(),
        scope: manuscript::ScopeBlock {
            leading_questions: vec!["A?".into(), "B?".into()],
            out_of_scope: vec![],
            assumptions: vec![],
            disclaimer_md: long_disclaimer(),
            success_criteria: vec![],
        },
        sections: vec![Section {
            id: "management_summary".into(),
            heading_level: 1,
            heading: "Summary".into(),
            blocks: vec![Block::Paragraph {
                text_md: "Eddy current \u{2014} promising candidate.".into(),
                evidence_ids: vec![],
            }],
        }],
        citation_register: vec![],
    };
    let mj = serde_json::to_string(&m).unwrap();
    conn.execute(
        "INSERT INTO report_versions(version_id, run_id, version_number, parent_version_id,
            manuscript_json, body_hash, produced_by, created_at)
         VALUES('ver_t3',?1,1,NULL,?2,'h3','draft',?3)",
        params![run_id, mj, store::now_iso()],
    )
    .unwrap();
    let report = check::run_check(&conn, &blueprint, &run_id, Some("ver_t3")).unwrap();
    let dashes = report
        .validators
        .iter()
        .find(|v| v.name == "forbid_unicode_dashes")
        .unwrap();
    assert!(!dashes.pass);
}

// Suppress unused-import warnings in this module under cfg(test).
#[allow(dead_code)]
fn _suppress(_: Value) {
    let _ = json!({});
    let _ = draft::DraftOutput {
        version_id: String::new(),
        version_number: 0,
        body_hash: String::new(),
        manuscript: Manuscript {
            schema: String::new(),
            run_id: String::new(),
            preset: String::new(),
            language: String::new(),
            title: String::new(),
            subtitle: None,
            version_label: String::new(),
            scope: manuscript::ScopeBlock {
                leading_questions: vec![],
                out_of_scope: vec![],
                assumptions: vec![],
                disclaimer_md: String::new(),
                success_criteria: vec![],
            },
            sections: vec![],
            citation_register: vec![],
        },
    };
}
