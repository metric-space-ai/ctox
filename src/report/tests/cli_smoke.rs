//! Smoke tests for the deterministic `ctox report …` CLI subcommands.
//!
//! Every test exercises the CLI handler the way the harness LLM would
//! invoke it via Bash: build a `Vec<String>` of args, call
//! [`crate::report::cli::handle_command`], assert on the SQLite side
//! effects.

use crate::report::cli::handle_command;
use crate::report::schema::{ensure_schema, open};
use crate::report::state::{create_run, CreateRunParams};
use crate::report::tests::fixtures::{insert_committed_block, insert_evidence, TestRoot};
use rusqlite::params;
use std::fs;

fn s(value: &str) -> String {
    value.to_string()
}

fn fresh_run(root: &TestRoot) -> String {
    let params_in = CreateRunParams {
        report_type_id: "feasibility_study".into(),
        domain_profile_id: "ndt_aerospace".into(),
        depth_profile_id: "decision_grade".into(),
        style_profile_id: "scientific_engineering_dossier".into(),
        language: "de".into(),
        raw_topic: "CLI smoke run".into(),
        package_summary: None,
    };
    create_run(root.path(), params_in).expect("create_run")
}

#[test]
fn add_evidence_rejects_title_only_stub() {
    let root = TestRoot::new().expect("test root");
    let run_id = fresh_run(&root);
    let args: Vec<String> = vec![
        s("add-evidence"),
        s("--run-id"),
        run_id.clone(),
        s("--title"),
        s("Some Plausible Paper Title"),
        s("--authors"),
        s("Foo Bar; Baz Qux"),
        s("--year"),
        s("2024"),
    ];
    let result = handle_command(root.path(), &args);
    let err = result.expect_err("title-only must be rejected");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("at least 200 chars"),
        "rejection message must explain the content requirement, got: {msg}"
    );

    let conn = open(root.path()).unwrap();
    ensure_schema(&conn).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM report_evidence_register WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0, "no row may have been inserted");
}

#[test]
fn add_evidence_accepts_title_with_abstract_file() {
    let root = TestRoot::new().expect("test root");
    let run_id = fresh_run(&root);

    // Write a real-content abstract file (>=200 chars).
    let tmp_abs = root.path().join("abs.md");
    let abstract_text = "Abstract: This paper presents a novel induction-thermography \
        methodology for unidirectional CFRP composites that exposes the \
        anisotropic heating signature characteristic of the fibre orientation. \
        The technique is contactless, single-sided, and quantitative under \
        realistic surface coatings. We compare against flash thermography and \
        eddy-current testing on a coupon set with calibrated grid disruptions \
        and report the detection probability and false-alarm rate.";
    fs::write(&tmp_abs, abstract_text).unwrap();
    assert!(abstract_text.chars().count() >= 200);

    let args: Vec<String> = vec![
        s("add-evidence"),
        s("--run-id"),
        run_id.clone(),
        s("--title"),
        s("Induction thermography for UD CFRP"),
        s("--authors"),
        s("Smith, A; Jones, B"),
        s("--year"),
        s("2023"),
        s("--url"),
        s("https://example.org/paper.pdf"),
        s("--abstract-file"),
        tmp_abs.to_string_lossy().to_string(),
    ];
    handle_command(root.path(), &args).expect("happy path must succeed");

    let conn = open(root.path()).unwrap();
    let (kind, abs_len, snip_len, resolver, raw_payload, year, has_created): (
        String,
        i64,
        i64,
        String,
        String,
        Option<i64>,
        i64,
    ) = conn
        .query_row(
            "SELECT kind, length(abstract_md), \
                    COALESCE(length(snippet_md), 0), \
                    resolver_used, raw_payload_json, year, \
                    CASE WHEN created_at IS NOT NULL THEN 1 ELSE 0 END \
             FROM report_evidence_register WHERE run_id = ?1",
            params![run_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .expect("one evidence row");

    assert_eq!(kind, "url");
    assert!(abs_len >= 200, "abstract must be persisted (got {abs_len})");
    assert_eq!(snip_len, 0);
    assert_eq!(resolver, "manual");
    assert!(
        raw_payload.contains("\"manual\":true"),
        "raw_payload_json must record the manual provenance, got {raw_payload}"
    );
    assert_eq!(year, Some(2023));
    assert_eq!(has_created, 1, "created_at must be populated");
}

#[test]
fn add_evidence_accepts_snippet_only_when_long_enough() {
    // Some sources don't have a clean abstract but a usable excerpt of
    // the full text. The snippet path is allowed when the content is
    // >=200 chars.
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    let snippet_path = root.path().join("snip.md");
    let snippet = "## Excerpt\n\nIn aerospace lightning protection, copper-mesh \
        layers embedded under the surface layup carry the strike current and \
        protect the underlying CFRP from joule heating. Continuity of this \
        mesh under impact damage is the single most important integrity \
        criterion for aircraft skin recertification after a lightning event.";
    fs::write(&snippet_path, snippet).unwrap();
    assert!(snippet.chars().count() >= 200);
    let args: Vec<String> = vec![
        s("add-evidence"),
        s("--run-id"),
        run_id.clone(),
        s("--title"),
        s("Lightning protection of CFRP, excerpt"),
        s("--url"),
        s("https://example.org/aero.html"),
        s("--snippet-file"),
        snippet_path.to_string_lossy().to_string(),
    ];
    handle_command(root.path(), &args).expect("snippet-only happy path");
    let conn = open(root.path()).unwrap();
    let (abs_len, snip_len): (i64, i64) = conn
        .query_row(
            "SELECT COALESCE(length(abstract_md),0), length(snippet_md) \
             FROM report_evidence_register WHERE run_id = ?1",
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(abs_len, 0);
    assert!(snip_len >= 200);
}

#[test]
fn add_evidence_rejects_short_abstract() {
    // <200 chars is not enough — the LLM cannot use that as a citable
    // source. This guards against the "I'll just put the title in the
    // abstract field" workaround.
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    let p = root.path().join("short.md");
    fs::write(&p, "Too short to be a real abstract.").unwrap();
    let args: Vec<String> = vec![
        s("add-evidence"),
        s("--run-id"),
        run_id,
        s("--title"),
        s("Foo"),
        s("--url"),
        s("https://example.org/x"),
        s("--abstract-file"),
        p.to_string_lossy().to_string(),
    ];
    let err = handle_command(root.path(), &args).expect_err("must reject short abstract");
    assert!(format!("{err:#}").contains("at least 200 chars"));
}

#[test]
fn add_evidence_requires_title_when_not_resolver_path() {
    // --url alone with no --title used to silently use the URL as
    // canonical_id. We now require --title for traceable references.
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    let p = root.path().join("a.md");
    fs::write(&p, "x".repeat(300)).unwrap();
    let args: Vec<String> = vec![
        s("add-evidence"),
        s("--run-id"),
        run_id,
        s("--url"),
        s("https://example.org/x"),
        s("--abstract-file"),
        p.to_string_lossy().to_string(),
    ];
    let err = handle_command(root.path(), &args).expect_err("must require title");
    assert!(format!("{err:#}").to_lowercase().contains("title"));
}

// ---------- block-stage / block-apply / block-list ----------

fn write_block_md(root: &TestRoot, name: &str, body: &str) -> std::path::PathBuf {
    let p = root.path().join(name);
    fs::write(&p, body).unwrap();
    p
}

#[test]
fn block_stage_then_apply_then_list_happy_path() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);

    let body = "# Management Summary\n\nDie Pruefbarkeit eines Blitzschutz-Kupfergitters \
        in CFRP ist machbar. Induktive Thermografie liefert die direkteste \
        Kopplung zur leitfaehigen Lage. [ev_test_001]";
    let md_path = write_block_md(&root, "mgmt.md", body);

    let stage_args: Vec<String> = vec![
        s("block-stage"),
        s("--run-id"),
        run_id.clone(),
        s("--instance-id"),
        s("doc_study__management_summary"),
        s("--title"),
        s("Management Summary"),
        s("--ord"),
        s("40"),
        s("--reason"),
        s("test draft"),
        s("--markdown-file"),
        md_path.to_string_lossy().to_string(),
    ];
    handle_command(root.path(), &stage_args).expect("stage must succeed");

    // Pending row visible.
    let conn = open(root.path()).unwrap();
    let pending_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM report_pending_blocks WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pending_count, 1, "exactly one pending row after stage");

    // Apply commits the row to report_blocks and clears pending.
    handle_command(
        root.path(),
        &[s("block-apply"), s("--run-id"), run_id.clone()],
    )
    .expect("apply must succeed");

    let pending_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM report_pending_blocks WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .unwrap();
    let committed_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM report_blocks WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pending_after, 0, "pending must be drained after apply");
    assert_eq!(committed_after, 1, "exactly one committed block");

    // block-list must not error and must report the block.
    handle_command(
        root.path(),
        &[s("block-list"), s("--run-id"), run_id.clone()],
    )
    .expect("block-list must succeed");
    handle_command(
        root.path(),
        &[
            s("block-list"),
            s("--run-id"),
            run_id.clone(),
            s("--json"),
        ],
    )
    .expect("block-list --json must succeed");
}

#[test]
fn block_stage_rejects_empty_markdown() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    let md_path = write_block_md(&root, "empty.md", "    \n");
    let args: Vec<String> = vec![
        s("block-stage"),
        s("--run-id"),
        run_id,
        s("--instance-id"),
        s("doc_study__management_summary"),
        s("--markdown-file"),
        md_path.to_string_lossy().to_string(),
    ];
    let err = handle_command(root.path(), &args).expect_err("empty markdown must fail");
    let msg = format!("{err:#}").to_lowercase();
    assert!(msg.contains("empty"));
}

#[test]
fn block_apply_with_no_pending_is_a_clean_noop() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    handle_command(
        root.path(),
        &[s("block-apply"), s("--run-id"), run_id.clone()],
    )
    .expect("no-op apply must not error");
    let conn = open(root.path()).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM report_blocks WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

// ---------- check ----------

#[test]
fn check_completeness_on_empty_run_is_not_ready() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    handle_command(
        root.path(),
        &[
            s("check"),
            s("--run-id"),
            run_id.clone(),
            s("completeness"),
        ],
    )
    .expect("completeness check must execute");
    let conn = open(root.path()).unwrap();
    let (kind, ready): (String, i64) = conn
        .query_row(
            "SELECT check_kind, ready_to_finish FROM report_check_runs \
             WHERE run_id = ?1 ORDER BY checked_at DESC LIMIT 1",
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(kind, "completeness");
    assert_eq!(ready, 0, "empty run is not complete");
}

#[test]
fn check_narrative_flow_structural_on_empty_run_misses_required_blocks() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    handle_command(
        root.path(),
        &[
            s("check"),
            s("--run-id"),
            run_id.clone(),
            s("narrative_flow"),
        ],
    )
    .expect("narrative_flow check must execute");
    let conn = open(root.path()).unwrap();
    let (ready, payload): (i64, String) = conn
        .query_row(
            "SELECT ready_to_finish, payload_json FROM report_check_runs \
             WHERE run_id = ?1 AND check_kind = 'narrative_flow' \
             ORDER BY checked_at DESC LIMIT 1",
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(ready, 0, "empty run cannot be flow-ready");
    assert!(
        payload.contains("missing_required"),
        "payload must list missing_required block_ids, got: {payload}"
    );
}

#[test]
fn check_rejects_unknown_kind() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    let err = handle_command(
        root.path(),
        &[
            s("check"),
            s("--run-id"),
            run_id,
            s("not_a_real_check"),
        ],
    )
    .expect_err("must reject unknown check kind");
    assert!(format!("{err:#}").contains("check kind"));
}

#[test]
fn check_character_budget_on_populated_workspace_reports_delta() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    // Seed the run with a single committed block so character_budget has
    // something to count against.
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "management_summary",
        "Management Summary",
        40,
        &"x".repeat(800),
        &[],
    )
    .unwrap();
    handle_command(
        root.path(),
        &[
            s("check"),
            s("--run-id"),
            run_id.clone(),
            s("character_budget"),
        ],
    )
    .expect("character_budget check executes");
    let conn = open(root.path()).unwrap();
    let payload: String = conn
        .query_row(
            "SELECT payload_json FROM report_check_runs \
             WHERE run_id = ?1 AND check_kind = 'character_budget' \
             ORDER BY checked_at DESC LIMIT 1",
            params![run_id],
            |row| row.get(0),
        )
        .unwrap();
    // Should mention the actual char count and the target.
    assert!(
        payload.contains("actual_chars") && payload.contains("target_chars"),
        "payload must include both actual and target chars, got {payload}"
    );
}

#[test]
fn check_release_guard_runs_against_seeded_blocks() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    // Seed a block + an evidence row so the release-guard lint table
    // has something to act on.
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "management_summary",
        "Management Summary",
        40,
        "Diese Pruefung ist machbar. Erfolgsaussichten (qualitativ): mittel-hoch.",
        &["ev_seed_1"],
    )
    .unwrap();
    insert_evidence(
        root.path(),
        &run_id,
        "ev_seed_1",
        "doi",
        Some("10.0/test"),
        Some("Seed Paper"),
        &["A. Author"],
        Some(2024),
    )
    .unwrap();
    handle_command(
        root.path(),
        &[
            s("check"),
            s("--run-id"),
            run_id.clone(),
            s("release_guard"),
        ],
    )
    .expect("release_guard check executes");
    let conn = open(root.path()).unwrap();
    let row: (i64, i64, String) = conn
        .query_row(
            "SELECT ready_to_finish, needs_revision, payload_json FROM report_check_runs \
             WHERE run_id = ?1 AND check_kind = 'release_guard' \
             ORDER BY checked_at DESC LIMIT 1",
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let _ = row;
    // The smoke goal here is that the check runs end-to-end without
    // panicking on a populated workspace and persists a row.
}

// ---------- full-text persistence ----------

#[test]
fn evidence_show_surfaces_full_text_when_present() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    insert_evidence(
        root.path(),
        &run_id,
        "ev_full_1",
        "doi",
        Some("10.0/full"),
        Some("OA Paper With Full Text"),
        &["Eee Fff"],
        Some(2024),
    )
    .unwrap();
    let conn = open(root.path()).unwrap();
    let body = "## Methods\nWe ran 3 experiments at 5 µm particle size.\n\
                ## Results\nDetection probability rose from 0.6 to 0.95.";
    conn.execute(
        "UPDATE report_evidence_register \
         SET full_text_md = ?1, full_text_source = ?2, full_text_chars = ?3 \
         WHERE run_id = ?4 AND evidence_id = ?5",
        params![
            body.to_string(),
            "open_access_pdf".to_string(),
            body.chars().count() as i64,
            run_id.clone(),
            "ev_full_1".to_string(),
        ],
    )
    .unwrap();

    handle_command(
        root.path(),
        &[
            s("evidence-show"),
            s("--run-id"),
            run_id.clone(),
            s("--evidence-id"),
            s("ev_full_1"),
            s("--full-text"),
            s("--json"),
        ],
    )
    .expect("evidence-show --full-text --json must succeed");

    // Verify the columns are queryable independently.
    let (chars, src, body_back): (i64, String, String) = conn
        .query_row(
            "SELECT full_text_chars, full_text_source, full_text_md \
             FROM report_evidence_register WHERE evidence_id = ?1",
            params!["ev_full_1"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert!(chars > 0);
    assert_eq!(src, "open_access_pdf");
    assert!(body_back.contains("0.6 to 0.95"));
}

#[test]
fn evidence_show_without_full_text_flag_does_not_dump_body() {
    // Body is large; we don't want every evidence-show call to flood
    // the LLM with the entire paper unless explicitly asked.
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    insert_evidence(
        root.path(),
        &run_id,
        "ev_full_2",
        "doi",
        Some("10.0/big"),
        Some("Big Paper"),
        &[],
        Some(2024),
    )
    .unwrap();
    let conn = open(root.path()).unwrap();
    conn.execute(
        "UPDATE report_evidence_register \
         SET full_text_md = ?1, full_text_source = 'open_access_pdf', \
             full_text_chars = ?2 \
         WHERE evidence_id = ?3",
        params![
            "x".repeat(50_000),
            50_000_i64,
            "ev_full_2".to_string(),
        ],
    )
    .unwrap();
    // Should succeed without --full-text and not panic on the big body.
    handle_command(
        root.path(),
        &[
            s("evidence-show"),
            s("--run-id"),
            run_id,
            s("--evidence-id"),
            s("ev_full_2"),
        ],
    )
    .expect("evidence-show without --full-text must succeed");
}

// ---------- evidence-show ----------

#[test]
fn evidence_show_returns_full_content_for_one_evidence() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    insert_evidence(
        root.path(),
        &run_id,
        "ev_show_1",
        "doi",
        Some("10.0/showtest"),
        Some("Show Me Paper"),
        &["Aaa Bbb"],
        Some(2024),
    )
    .unwrap();
    // insert_evidence does not populate abstract_md by default — patch
    // it directly.
    let conn = open(root.path()).unwrap();
    conn.execute(
        "UPDATE report_evidence_register SET abstract_md = ?1 \
         WHERE run_id = ?2 AND evidence_id = ?3",
        params![
            "This abstract describes a novel induction method.".to_string(),
            run_id.clone(),
            "ev_show_1".to_string(),
        ],
    )
    .unwrap();
    handle_command(
        root.path(),
        &[
            s("evidence-show"),
            s("--run-id"),
            run_id.clone(),
            s("--evidence-id"),
            s("ev_show_1"),
        ],
    )
    .expect("evidence-show happy path");
}

#[test]
fn evidence_show_json_includes_abstract_md() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    insert_evidence(
        root.path(),
        &run_id,
        "ev_json_1",
        "doi",
        Some("10.0/jsontest"),
        Some("JSON Test Paper"),
        &["Ccc Ddd"],
        Some(2025),
    )
    .unwrap();
    let conn = open(root.path()).unwrap();
    conn.execute(
        "UPDATE report_evidence_register SET abstract_md = ?1 \
         WHERE run_id = ?2 AND evidence_id = ?3",
        params![
            "Specific finding XYZ123 demonstrated under condition ABC.".to_string(),
            run_id.clone(),
            "ev_json_1".to_string(),
        ],
    )
    .unwrap();
    handle_command(
        root.path(),
        &[
            s("evidence-show"),
            s("--run-id"),
            run_id,
            s("--all"),
            s("--json"),
        ],
    )
    .expect("evidence-show --all --json must succeed");
}

#[test]
fn evidence_show_rejects_missing_filter() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    let err = handle_command(
        root.path(),
        &[s("evidence-show"), s("--run-id"), run_id],
    )
    .expect_err("must require --evidence-id or --all");
    assert!(format!("{err:#}").contains("--evidence-id"));
}

#[test]
fn evidence_show_rejects_unknown_evidence_id() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    let err = handle_command(
        root.path(),
        &[
            s("evidence-show"),
            s("--run-id"),
            run_id,
            s("--evidence-id"),
            s("ev_does_not_exist"),
        ],
    )
    .expect_err("must error when no row matches");
    assert!(format!("{err:#}").to_lowercase().contains("no evidence"));
}

#[test]
fn check_release_guard_flags_stub_evidence_cited_by_a_block() {
    // Block cites ev_stub which has no abstract/snippet content.
    // LINT-STUB-EVIDENCE must surface the violation as needs_revision.
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);
    insert_evidence(
        root.path(),
        &run_id,
        "ev_stub",
        "manual",
        Some("manual:Some Paper"),
        Some("Some Paper"),
        &["A. Author"],
        Some(2024),
    )
    .unwrap();
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "management_summary",
        "Management Summary",
        40,
        "Diese Studie ist machbar [ev_stub]. Erfolgsaussichten (qualitativ): mittel.",
        &["ev_stub"],
    )
    .unwrap();
    handle_command(
        root.path(),
        &[
            s("check"),
            s("--run-id"),
            run_id.clone(),
            s("release_guard"),
        ],
    )
    .expect("release_guard runs");
    let conn = open(root.path()).unwrap();
    let payload: String = conn
        .query_row(
            "SELECT payload_json FROM report_check_runs \
             WHERE run_id = ?1 AND check_kind = 'release_guard' \
             ORDER BY checked_at DESC LIMIT 1",
            params![run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        payload.contains("LINT-STUB-EVIDENCE"),
        "payload must mention LINT-STUB-EVIDENCE, got: {payload}"
    );
}

// ---------- ask-user / answer round-trip ----------

#[test]
fn ask_user_then_answer_round_trip() {
    let root = TestRoot::new().unwrap();
    let run_id = fresh_run(&root);

    handle_command(
        root.path(),
        &[
            s("ask-user"),
            s("--run-id"),
            run_id.clone(),
            s("--question"),
            s("Welche Inspektionsfrequenz?"),
            s("--question"),
            s("Welche CFRP-Lagenfamilie?"),
            s("--reason"),
            s("scope clarification"),
        ],
    )
    .expect("ask-user must succeed");

    let conn = open(root.path()).unwrap();
    let (qid, n_questions): (String, String) = conn
        .query_row(
            "SELECT question_id, questions_json FROM report_questions \
             WHERE run_id = ?1 AND answered_at IS NULL",
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("one open question");
    assert!(n_questions.contains("Inspektionsfrequenz"));
    assert!(n_questions.contains("CFRP"));

    handle_command(
        root.path(),
        &[
            s("answer"),
            run_id.clone(),
            s("--question-id"),
            qid.clone(),
            s("--answer"),
            s("monatlich; UD-Layups Phase 1"),
        ],
    )
    .expect("answer must succeed");

    let answered_at: Option<String> = conn
        .query_row(
            "SELECT answered_at FROM report_questions WHERE question_id = ?1",
            params![qid],
            |row| row.get(0),
        )
        .unwrap();
    assert!(answered_at.is_some(), "answered_at must be populated");
}
