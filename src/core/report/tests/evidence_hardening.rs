use crate::report::render::manuscript::build_manuscript;
use crate::report::schema::{ensure_schema, open};
use crate::report::scoring::{self, CellInput, RubricInput};
use crate::report::state::{create_run, CreateRunParams};
use crate::report::store;
use crate::report::tests::fixtures::{insert_committed_block, TestRoot};
use crate::report::workspace::Workspace;
use rusqlite::params;

fn legacy_run_params() -> CreateRunParams {
    CreateRunParams {
        report_type_id: "feasibility_study".into(),
        domain_profile_id: "ndt_aerospace".into(),
        depth_profile_id: "decision_grade".into(),
        style_profile_id: "scientific_engineering_dossier".into(),
        language: "en".into(),
        raw_topic: "evidence hardening".into(),
        package_summary: None,
    }
}

#[test]
fn legacy_evidence_rows_migrate_as_unverified() {
    let root = TestRoot::new().unwrap();
    let conn = store::open(root.path()).unwrap();
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_runs(run_id, preset, blueprint_version, topic, language,
             status, created_at, updated_at)
         VALUES('legacy-run', 'feasibility', '1', 'topic', 'en', 'scoped', ?1, ?1)",
        params![now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO report_evidence(
             evidence_id, run_id, citation_kind, canonical_id, title, authors_json,
             retrieved_at, resolver, created_at)
         VALUES('legacy-ev', 'legacy-run', 'doi', '10.1234/legacy', 'Legacy', '[]',
                ?1, 'manual', ?1)",
        params![now],
    )
    .unwrap();

    ensure_schema(&conn).unwrap();
    let row: (String, Option<i64>, Option<String>, i64) = conn
        .query_row(
            "SELECT verification_status, http_status, snapshot_hash, evidence_eligible
             FROM report_evidence_register
             WHERE run_id = 'legacy-run' AND evidence_id = 'legacy-ev'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(row, ("unverified".into(), None, None, 0));
}

#[test]
fn scoring_rejects_unverified_evidence_until_all_gates_hold() {
    let root = TestRoot::new().unwrap();
    let conn = store::open(root.path()).unwrap();
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_runs(run_id, preset, blueprint_version, topic, language,
             status, created_at, updated_at)
         VALUES('score-run', 'feasibility', '1', 'topic', 'en', 'enumerating', ?1, ?1)",
        params![now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO report_options(option_id, run_id, code, label, created_at)
         VALUES('opt-1', 'score-run', 'opt', 'Option', ?1)",
        params![now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO report_evidence_register(
             evidence_id, run_id, kind, canonical_id, authors_json, resolver_used,
             raw_payload_json, created_at, updated_at)
         VALUES('ev-score', 'score-run', 'url', 'https://example.test/doc', '[]',
                'manual', '{}', ?1, ?1)",
        params![now],
    )
    .unwrap();
    scoring::upsert_rubric(
        &conn,
        "score-run",
        &RubricInput {
            axis_code: "axis".into(),
            level_code: "high".into(),
            level_definition_md: "A verified high-level definition.".into(),
            numeric_value: Some(1.0),
        },
    )
    .unwrap();
    let input = CellInput {
        matrix_kind: "matrix".into(),
        matrix_label: None,
        option_code: "opt".into(),
        axis_code: "axis".into(),
        value_label: "high".into(),
        rationale_md: "This rationale is specific enough for the test.".into(),
        evidence_ids: vec!["ev-score".into()],
        assumption_note_md: None,
        rubric_anchor: None,
    };
    assert!(scoring::upsert_cell(&conn, "score-run", &input).is_err());

    conn.execute(
        "UPDATE report_evidence_register
         SET verification_status = 'verified', http_status = 200,
             snapshot_hash = 'snapshot', evidence_eligible = 1
         WHERE evidence_id = 'ev-score'",
        [],
    )
    .unwrap();
    scoring::upsert_cell(&conn, "score-run", &input).unwrap();
}

#[test]
fn manuscript_drops_unverified_reference_ids() {
    let root = TestRoot::new().unwrap();
    let run_id = create_run(root.path(), legacy_run_params()).unwrap();
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "management_summary",
        "Management Summary",
        1,
        "A short report block.",
        &["ev-unverified"],
    )
    .unwrap();
    let conn = open(root.path()).unwrap();
    conn.execute(
        "INSERT INTO report_evidence_register(
             evidence_id, run_id, kind, canonical_id, authors_json, resolver_used,
             raw_payload_json, created_at, updated_at)
         VALUES('ev-unverified', ?1, 'url', 'https://example.test/doc', '[]',
                'manual', '{}', ?2, ?2)",
        params![run_id, store::now_iso()],
    )
    .unwrap();
    let workspace = Workspace::load(root.path(), &run_id).unwrap();
    let manuscript = build_manuscript(&workspace).unwrap();
    assert!(manuscript.references.is_empty());
}
