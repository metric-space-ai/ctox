//! Smoke tests for the deterministic checks (completeness,
//! character_budget, release_guard).

use crate::report::asset_pack::AssetPack;
use crate::report::checks::{
    run_character_budget_check, run_completeness_check, run_release_guard_check,
};
use crate::report::state::create_run;
use crate::report::tests::fixtures::{
    insert_committed_block, insert_evidence, rascon_create_params, TestRoot,
};
use crate::report::workspace::Workspace;

#[test]
fn completeness_check_empty_run_is_not_ready() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace");
    let outcome = run_completeness_check(&workspace).expect("completeness check");
    assert!(
        !outcome.ready_to_finish,
        "empty run should not be ready_to_finish: {outcome:?}"
    );
    assert!(outcome.needs_revision, "empty run needs revision");
    assert!(
        !outcome.candidate_instance_ids.is_empty(),
        "empty run must surface candidate instance_ids; got {:?}",
        outcome.candidate_instance_ids
    );
}

#[test]
fn character_budget_check_empty_run_is_not_started() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace");
    let outcome = run_character_budget_check(&workspace).expect("character_budget check");
    assert!(
        !outcome.check_applicable,
        "empty run should report check_applicable=false; got {outcome:?}"
    );
    let status = outcome
        .raw_payload
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        status, "not_started",
        "empty run status should be 'not_started'; got {status:?}"
    );
}

#[test]
fn character_budget_check_within_tolerance() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    let pack = AssetPack::load().expect("asset pack");
    let report_type = pack
        .report_type("feasibility_study")
        .expect("feasibility_study");
    let target = report_type.typical_chars as usize;
    // Build one fat block whose char count lands inside the +/-20% corridor.
    let chunk = "Eddy Current Testing erreicht laut Feng et al. (2020) Detektionsraten von 87 Prozent fuer kontaktlose CFRP-Pruefung.\n\n";
    let mut markdown = String::new();
    while markdown.chars().count() < target {
        markdown.push_str(chunk);
    }
    // Trim back into the corridor (target chars exact-ish).
    if markdown.chars().count() > target {
        let trimmed: String = markdown.chars().take(target).collect();
        markdown = trimmed;
    }
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "detail_assessment_per_option",
        "Detailbewertung",
        120,
        &markdown,
        &[],
    )
    .expect("insert committed block");

    let workspace = Workspace::load(root.path(), &run_id).expect("workspace");
    let outcome = run_character_budget_check(&workspace).expect("character_budget check");
    assert!(
        outcome.check_applicable,
        "with content the check is applicable: {outcome:?}"
    );
    let within = outcome
        .raw_payload
        .get("within_tolerance")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        within,
        "char count near typical_chars should be within tolerance: {outcome:?}"
    );
    assert!(
        outcome.ready_to_finish,
        "within tolerance implies ready_to_finish: {outcome:?}"
    );
}

#[test]
fn release_guard_no_blocks_is_check_applicable_false() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace");
    let outcome = run_release_guard_check(&workspace).expect("release_guard check");
    assert!(
        !outcome.check_applicable,
        "no committed blocks -> not applicable: {outcome:?}"
    );
    assert!(
        outcome.ready_to_finish,
        "non-applicable check counts as ready: {outcome:?}"
    );
}

#[test]
fn release_guard_fab_doi_fires() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    // Insert a block that cites a DOI which is NOT in the evidence register.
    let body = "Eddy current per Mueller et al. (DOI: 10.9999/fake.0001) zeigt POD 87 Prozent. \
                Validierung an Coupons mit Kupfergitter ist erforderlich, sonst bleibt die Aussage \
                eine Annahme; Limits ergeben sich aus dem Rauschpegel.";
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "detail_assessment_per_option",
        "Detailbewertung",
        120,
        body,
        &[],
    )
    .expect("insert block with fake DOI");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace");
    let outcome = run_release_guard_check(&workspace).expect("release_guard check");
    let lint_ids: Vec<String> = outcome
        .raw_payload
        .get("issues")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|i| {
                    i.get("lint_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(
        lint_ids.iter().any(|id| id == "LINT-FAB-DOI"),
        "expected LINT-FAB-DOI to fire; got lint ids {:?}",
        lint_ids
    );
    assert!(
        !outcome.ready_to_finish,
        "fabricated DOI must block release: {outcome:?}"
    );
}

#[test]
fn release_guard_dead_phrase_fires() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    let body = "Im Folgenden werden die einzelnen Verfahren naeher betrachtet. \
                Die Aussagen sind nicht abschliessend belegt; Validierung steht aus.";
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "detail_assessment_per_option",
        "Detailbewertung",
        120,
        body,
        &[],
    )
    .expect("insert block with dead phrase");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace");
    let outcome = run_release_guard_check(&workspace).expect("release_guard check");
    let lint_ids: Vec<String> = outcome
        .raw_payload
        .get("issues")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|i| {
                    i.get("lint_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(
        lint_ids.iter().any(|id| id == "LINT-DEAD-PHRASE"),
        "expected LINT-DEAD-PHRASE to fire; got lint ids {:?}",
        lint_ids
    );
}

#[test]
fn release_guard_no_violations_is_ready() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    // Pre-populate evidence so LINT-EVIDENCE-FLOOR cannot fire.
    for i in 0..25 {
        let id = format!("ev_clean_{i:02}");
        insert_evidence(
            root.path(),
            &run_id,
            &id,
            "doi",
            Some(&format!("10.1234/clean.{i:04}")),
            Some(&format!("Source {i}")),
            &["Author"],
            Some(2020),
        )
        .expect("insert evidence");
    }
    // Plain narrative content with no DOIs/arXiv IDs and no dead phrases.
    // Padded to clear LINT-MIN-CHARS for context_and_question (min_chars=720).
    let chunk = "Kontaktlose Pruefung des Kupfergitters wird sachlich beschrieben. \
                 Validierung steht im Vordergrund; Annahmen sind explizit; Limits werden benannt. \
                 Aerospace-konforme POD-Ziele bleiben das Mass; Schichtaufbau-Variante A ist Bezug.\n\n";
    let mut body = String::new();
    while body.chars().count() < 1500 {
        body.push_str(chunk);
    }
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "context_and_question",
        "Kontext",
        50,
        &body,
        &[],
    )
    .expect("insert clean block");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace");
    let outcome = run_release_guard_check(&workspace).expect("release_guard check");
    // We do not require zero issues — soft lints unrelated to language
    // shape may still fire; require ready_to_finish.
    assert!(
        outcome.ready_to_finish,
        "clean fixture should be ready_to_finish; outcome={outcome:?}"
    );
}
