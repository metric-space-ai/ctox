//! End-to-end smoke test: drive a feasibility run through every stage with
//! deterministic inputs (no network), then assert that `check` passes and
//! `render md` produces a non-empty Markdown file.
//!
//! This is the closest thing to the RASCON gold replay we can do without
//! pulling external archetype files into the production tree. The fixtures
//! used here are synthetic and reusable — they do not encode RASCON
//! conclusions or any pre-coded verdicts.

use serde_json::json;
use tempfile::tempdir;

use crate::report::blueprints;
use crate::report::check;
use crate::report::claims;
use crate::report::draft;
use crate::report::evidence;
use crate::report::render;
use crate::report::runs;
use crate::report::scope;
use crate::report::scoring;
use crate::report::store;

fn seed_options(conn: &rusqlite::Connection, run_id: &str) {
    let opts = [
        ("OPT_A", "Option Alpha", vec!["alpha"]),
        ("OPT_B", "Option Beta", vec!["beta"]),
        ("OPT_C", "Option Gamma", vec!["gamma"]),
    ];
    for (code, label, syn) in opts {
        claims::upsert_option(
            conn,
            run_id,
            &claims::OptionInput {
                code: code.into(),
                label: label.into(),
                summary_md: Some(format!("{label} summary.")),
                synonyms: syn.into_iter().map(String::from).collect(),
            },
        )
        .unwrap();
    }
}

fn seed_evidence(conn: &rusqlite::Connection, run_id: &str, n: usize) -> Vec<String> {
    let mut ids = Vec::new();
    for i in 0..n {
        let label = match i % 3 {
            0 => "alpha",
            1 => "beta",
            _ => "gamma",
        };
        let input = evidence::EvidenceInput {
            citation_kind: "doi".into(),
            canonical_id: format!("10.0000/cli.{i}"),
            title: Some(format!("Study of Option {label} {i}")),
            authors: vec!["Researcher".into()],
            venue: Some("Synthetic Journal".into()),
            year: Some(2025),
            publisher: None,
            landing_url: Some(format!("https://example.test/paper/{i}")),
            full_text_url: None,
            abstract_md: None,
            snippet_md: Some(format!(
                "Empirical evaluation of option {label} for the target task; \
                 reports performance on representative coupons."
            )),
            license: None,
            resolver: Some("manual".into()),
        };
        ids.push(
            evidence::upsert_evidence(conn, run_id, &input)
                .unwrap()
                .evidence_id,
        );
    }
    ids
}

fn seed_rubrics(conn: &rusqlite::Connection, run_id: &str) {
    let levels = [
        ("low", 1.0),
        ("medium", 2.0),
        ("high", 3.0),
        ("very_high", 4.0),
    ];
    let axes = [
        "coverage",
        "imaging",
        "defect_sensitivity",
        "delamination_sensitivity",
        "maturity",
    ];
    for axis in axes {
        for (lvl, num) in levels {
            scoring::upsert_rubric(
                conn,
                run_id,
                &scoring::RubricInput {
                    axis_code: axis.into(),
                    level_code: lvl.into(),
                    level_definition_md: format!(
                        "{lvl} level on axis {axis}: defined here so cell values are auditable."
                    ),
                    numeric_value: Some(num),
                },
            )
            .unwrap();
        }
    }
}

fn fill_matrix(conn: &rusqlite::Connection, run_id: &str, evidence_ids: &[String]) {
    let options = ["OPT_A", "OPT_B", "OPT_C"];
    let axes = [
        "coverage",
        "imaging",
        "defect_sensitivity",
        "delamination_sensitivity",
        "maturity",
    ];
    let levels = ["high", "medium", "high", "medium", "high"];
    for (oi, opt) in options.iter().enumerate() {
        for (ai, axis) in axes.iter().enumerate() {
            let level = levels[(oi + ai) % levels.len()];
            let cell = scoring::CellInput {
                matrix_kind: "main".into(),
                matrix_label: None,
                option_code: (*opt).into(),
                axis_code: (*axis).into(),
                value_label: level.into(),
                rationale_md: format!(
                    "Cell {opt}/{axis} rated {level} based on referenced evidence."
                ),
                evidence_ids: vec![evidence_ids[(oi + ai) % evidence_ids.len()].clone()],
                assumption_note_md: None,
                rubric_anchor: Some(format!("rubric:{axis}:{level}")),
            };
            scoring::upsert_cell(conn, run_id, &cell).unwrap();
        }
    }
}

fn add_scenarios(conn: &rusqlite::Connection, run_id: &str) {
    for code in ["A", "B"] {
        claims::upsert_scenario(
            conn,
            run_id,
            &claims::ScenarioInput {
                code: code.into(),
                label: format!("Scenario {code}"),
                description_md: format!(
                    "Scenario {code}: parameter regime under which all options are evaluated."
                ),
                impact_summary_md: None,
            },
        )
        .unwrap();
    }
}

fn add_claims(conn: &rusqlite::Connection, run_id: &str, evidence_ids: &[String]) {
    let blueprint = blueprints::load("feasibility").unwrap();
    let claim_specs: Vec<(&str, &str, &str, bool)> = vec![
        (
            "management_summary",
            "Option Alpha consistently outperforms Beta and Gamma on representative coupons.",
            "finding",
            false,
        ),
        (
            "management_summary",
            "Option Beta is competitive on coverage but weaker on defect sensitivity.",
            "finding",
            false,
        ),
        (
            "management_summary",
            "Option Gamma trails Alpha and Beta on imaging quality.",
            "finding",
            false,
        ),
        (
            "context_and_question",
            "The target system requires non-contact inspection of representative coupons.",
            "finding",
            false,
        ),
        (
            "detail_assessment",
            "Option Alpha sustains its rating across the alpha coupon series.",
            "finding",
            false,
        ),
        (
            "detail_assessment",
            "Option Beta delivers stable beta-grade outputs across runs.",
            "finding",
            false,
        ),
        (
            "detail_assessment",
            "Option Gamma exhibits gamma-typical degradation under the same loads.",
            "finding",
            false,
        ),
        (
            "recommendation",
            "Option Alpha is the recommended primary path for the next project phase.",
            "recommendation",
            true,
        ),
        (
            "recommendation",
            "Option Gamma is not recommended without major redesign.",
            "recommendation",
            false,
        ),
    ];
    for (_idx, (section, text, kind, is_primary)) in claim_specs.into_iter().enumerate() {
        // Pin every claim to evidence whose snippet mentions the same option
        // label. Evidence index i has snippet about "alpha" if i%3==0, "beta"
        // if i%3==1, "gamma" otherwise — so ev_ids[0/1/2] cover all three.
        let lower = text.to_lowercase();
        let pin_idx = if lower.contains("alpha") {
            0
        } else if lower.contains("beta") {
            1
        } else if lower.contains("gamma") {
            2
        } else {
            0
        };
        let input = claims::ClaimInput {
            section_id: section.into(),
            text_md: text.into(),
            claim_kind: kind.into(),
            evidence_ids: vec![evidence_ids[pin_idx].clone()],
            assumption_note_md: None,
            confidence: Some("high".into()),
            primary_recommendation: is_primary,
            scenario_code: None,
            rubric_anchor: None,
        };
        claims::add_claim(conn, &blueprint, run_id, &input).unwrap();
    }
}

#[test]
fn end_to_end_feasibility_run() {
    let dir = tempdir().unwrap();
    let conn = store::open(dir.path()).unwrap();
    let blueprint = blueprints::load("feasibility").unwrap();

    // 1. new
    let run = runs::create_run(&conn, &blueprint, "Synthetic CLI smoke topic", "en", None).unwrap();
    let run_id = run.run_id;

    // 2. scope
    scope::upsert_scope(
        &conn,
        &blueprint,
        &run_id,
        &scope::ScopeInput {
            leading_questions: vec![
                "Which option performs best on the target coupons?".into(),
                "What are the main risks for production rollout?".into(),
            ],
            out_of_scope: vec!["Cost analysis is out of scope.".into()],
            assumptions: vec!["Coupons are representative of the production population.".into()],
            disclaimer_md:
                "This study makes assumptions about coupon representativeness and requires \
                 validation in a follow-up campaign before production deployment."
                    .into(),
            success_criteria: vec!["A primary recommendation is produced with evidence.".into()],
        },
    )
    .unwrap();

    // 3. options + 4. evidence + 5. rubric + 6. matrix + 7. scenarios + 8. claims
    seed_options(&conn, &run_id);
    let ev_ids = seed_evidence(&conn, &run_id, 12);
    seed_rubrics(&conn, &run_id);
    fill_matrix(&conn, &run_id, &ev_ids);
    add_scenarios(&conn, &run_id);
    add_claims(&conn, &run_id, &ev_ids);

    // 9. add at least one risk
    claims::upsert_risk(
        &conn,
        &run_id,
        &claims::RiskInput {
            code: "R1".into(),
            title: "Coupon non-representativeness".into(),
            description_md: "Coupons may underrepresent production-scale geometric features."
                .into(),
            mitigation_md: "Run a second campaign with full-scale coupons before lock-in.".into(),
            likelihood: Some("medium".into()),
            impact: Some("high".into()),
            evidence_ids: vec![ev_ids[0].clone()],
        },
    )
    .unwrap();

    // 10. draft
    let draft_out = draft::draft_run(&conn, &blueprint, &run_id).unwrap();
    assert!(draft_out.manuscript.sections.len() >= 5);

    // 11. check
    let report = check::run_check(&conn, &blueprint, &run_id, Some(&draft_out.version_id)).unwrap();
    if !report.overall_pass {
        let failing: Vec<_> = report
            .validators
            .iter()
            .filter(|v| v.severity == "hard" && !v.pass)
            .collect();
        panic!(
            "smoke test expected overall_pass=true; failing hard validators:\n{}",
            serde_json::to_string_pretty(&failing).unwrap()
        );
    }

    // 12. render md
    let render_out = render::render(
        &conn,
        dir.path(),
        &run_id,
        Some(&draft_out.version_id),
        "md",
        None,
        false,
    )
    .unwrap();
    let bytes = std::fs::read(&render_out.output_path).unwrap();
    assert!(!bytes.is_empty());
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("# Feasibility Study"));
    assert!(text.contains("Recommendation"));
    assert!(text.contains("Option Alpha"));

    // 13. finalize
    let summary = runs::run_summary(&conn, &run_id).unwrap();
    assert_eq!(summary["ok"], json!(true));
}
