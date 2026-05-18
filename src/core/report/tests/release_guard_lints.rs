//! Targeted exercise of release_guard lint families.
//!
//! Each case picks a snippet calculated to trigger one specific lint id
//! (or, for the CLEAN case, no hard/critical lint at all) and asserts
//! the readiness verdict. We do NOT assert lint id directly because the
//! same fixture content can trip multiple lints — `ready_to_finish`
//! captures the gate-relevant truth either way.

use crate::report::checks::run_release_guard_check;
use crate::report::state::create_run;
use crate::report::tests::fixtures::{
    insert_committed_block, insert_evidence, rascon_create_params, TestRoot,
};
use crate::report::workspace::Workspace;

/// Run a single release_guard scenario against a fresh workspace.
///
/// Inserts evidence sufficient to clear LINT-EVIDENCE-FLOOR (so the
/// other tests can fire on language shape rather than evidence count).
/// Every per-case snippet is wedged into `detail_assessment_per_option`
/// — a block that the language and structural lints actively target.
fn run_release_guard_with_snippet(case: &str, snippet: &str) -> bool {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    // Pre-seed >= 25 evidence rows so LINT-EVIDENCE-FLOOR cannot fire.
    for i in 0..25 {
        let id = format!("ev_seed_{i:02}");
        insert_evidence(
            root.path(),
            &run_id,
            &id,
            "doi",
            Some(&format!("10.1234/seed.{i:04}")),
            Some(&format!("Seed {i}")),
            &["Seed"],
            Some(2020),
        )
        .expect("seed evidence");
    }
    // Pad the snippet with neutral filler so LINT-MIN-CHARS does not
    // fire on a too-short body. The filler is plain narrative without
    // hedges, dead phrases, fabricated DOIs, or first-person pronouns.
    let neutral_filler = "Kontaktlose Pruefung des Kupfergitters wird sachlich beschrieben. \
                          Schichtaufbau-Variante A bleibt der Bezug; Aerospace-konforme Toleranzen \
                          gelten als gegeben. Die Methodenklasse haengt vom Probenkopf ab. \
                          Sekundaereffekte sind ausgewiesen.\n\n";
    let mut body = snippet.to_string();
    while body.chars().count() < 800 {
        body.push_str(neutral_filler);
    }

    if case == "CLEAN" {
        // Insert a clean scope_disclaimer with all three clusters.
        insert_committed_block(
            root.path(),
            &run_id,
            "doc_study",
            "scope_disclaimer",
            "Scope-Hinweis",
            20,
            "Annahme: Schichtaufbau Variante A. Validierung an Coupons ist erforderlich. \
             Grenze: Inline-Pruefung wurde nicht beruecksichtigt.",
            &[],
        )
        .expect("clean scope_disclaimer");
        // Use context_and_question (min_chars=720) and seed with a
        // padded clean body so LINT-MIN-CHARS clears.
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
        .expect("clean context block");
    } else if case == "LINT-MISSING-DISCLAIMER" {
        // A scope_disclaimer that lacks all three clusters.
        insert_committed_block(
            root.path(),
            &run_id,
            "doc_study",
            "scope_disclaimer",
            "Scope-Hinweis",
            20,
            "Sehr knapper Hinweistext ohne erforderliche Klauseln zum Geltungsbereich.",
            &[],
        )
        .expect("dirty scope_disclaimer");
        // Also seed a body block so the check is applicable.
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
        .expect("body block");
    } else if case == "LINT-INVERTED-PERSPECTIVE" {
        // This lint targets specific templates that include third-person
        // perspective; use management_summary (min_chars=950) so the
        // template targeting is right.
        insert_committed_block(
            root.path(),
            &run_id,
            "doc_study",
            "management_summary",
            "Management Summary",
            40,
            &body,
            &[],
        )
        .expect("management_summary body");
    } else {
        // Non-clean cases: the snippet itself becomes the body of a
        // context_and_question block. This block has min_chars=720
        // (much smaller than detail_assessment) so the padding is feasible.
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
        .expect("snippet block");
    }

    let workspace = Workspace::load(root.path(), &run_id).expect("workspace");
    let outcome = run_release_guard_check(&workspace).expect("release_guard check");
    outcome.ready_to_finish
}

#[test]
fn release_guard_lints_table_drives_outcomes() {
    // Table format: (label, snippet, expected_ready_to_finish).
    let cases: &[(&str, &str, bool)] = &[
        (
            "LINT-FAB-DOI",
            "Eddy current per Mueller et al. (DOI: 10.9999/fake.0001) zeigt POD 87 Prozent.",
            false,
        ),
        (
            "LINT-FAB-AUTHOR",
            "Schmidt 2023 findet POD-Werte von 92 Prozent fuer Variante A.",
            false,
        ),
        (
            "LINT-FAB-ARXIV",
            "Vergleiche arXiv:2501.99999 fuer einen kontaktlosen Pruefansatz.",
            false,
        ),
        (
            "LINT-DEAD-PHRASE",
            "Im Folgenden werden die einzelnen Verfahren naeher betrachtet. Validierung steht aus.",
            false,
        ),
        (
            "LINT-FILLER-OPENING",
            "Im Rahmen dieser Studie betrachten wir kontaktlose Pruefverfahren ohne weitere Belege.",
            false,
        ),
        (
            "LINT-INVERTED-PERSPECTIVE",
            "Wir empfehlen, die Studie fortzusetzen, und unsere Sicht ueberwiegt.",
            false,
        ),
        // LINT-MIN-CHARS: a body so short the lint must fire even with
        // padding logic — bypass padding by handling separately.
        ("LINT-MISSING-DISCLAIMER", "(see fixture)", false),
        (
            "CLEAN",
            "Eddy Current Testing erreicht im Pruefumfeld nachvollziehbare Werte. \
             Schichtaufbau-Variante A bleibt der Bezug; Sekundaereffekte sind dokumentiert.",
            true,
        ),
    ];
    let mut failed: Vec<String> = Vec::new();
    for (label, snippet, expected_ready) in cases {
        let ready = run_release_guard_with_snippet(label, snippet);
        if ready != *expected_ready {
            failed.push(format!(
                "case {label}: expected ready_to_finish={}, got {} (snippet: {:?})",
                expected_ready, ready, snippet
            ));
        }
    }
    assert!(
        failed.is_empty(),
        "release_guard table-drive failures:\n{}",
        failed.join("\n")
    );
}

#[test]
fn release_guard_min_chars_fires_for_short_required_block() {
    // Targets LINT-MIN-CHARS without going through the table-drive
    // padder. We seed enough evidence to clear the floor and then
    // commit a block whose body is far below the 65% min_chars floor.
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    for i in 0..25 {
        let id = format!("ev_short_{i:02}");
        insert_evidence(
            root.path(),
            &run_id,
            &id,
            "doi",
            Some(&format!("10.1234/short.{i:04}")),
            Some(&format!("Short {i}")),
            &["Short"],
            Some(2020),
        )
        .expect("seed evidence");
    }
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "management_summary",
        "Management Summary",
        40,
        "kurz",
        &[],
    )
    .expect("short body");
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
        lint_ids.iter().any(|id| id == "LINT-MIN-CHARS"),
        "expected LINT-MIN-CHARS to fire for a four-character body; got {:?}",
        lint_ids
    );
    assert!(
        !outcome.ready_to_finish,
        "min-chars violation must block release"
    );
}

#[test]
fn release_guard_evidence_floor_fires_when_register_undersized() {
    // Build a feasibility-study run that has SOME committed content but
    // an evidence register far below decision_grade's floor (20). The
    // floor lint must fire even though no language-shape lint trips.
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    // Only seed a couple of evidence rows.
    for i in 0..3 {
        let id = format!("ev_thin_{i:02}");
        insert_evidence(
            root.path(),
            &run_id,
            &id,
            "doi",
            Some(&format!("10.1234/thin.{i:04}")),
            Some(&format!("Thin {i}")),
            &["Thin"],
            Some(2020),
        )
        .expect("seed thin evidence");
    }
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "context_and_question",
        "Kontext",
        50,
        "Kontaktlose Pruefung des Kupfergitters wird sachlich beschrieben. \
         Validierung steht im Vordergrund; Annahmen sind explizit; Limits werden benannt.",
        &[],
    )
    .expect("body block");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace");
    let outcome = run_release_guard_check(&workspace).expect("release_guard");
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
        lint_ids.iter().any(|id| id == "LINT-EVIDENCE-FLOOR"),
        "expected LINT-EVIDENCE-FLOOR to fire when register has only 3 entries; got {:?}",
        lint_ids
    );
    assert!(
        !outcome.ready_to_finish,
        "thin evidence register must block release"
    );
}

#[test]
fn release_guard_accepts_doi_embedded_in_url_evidence() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    for i in 0..25 {
        let id = format!("ev_seed_{i:02}");
        insert_evidence(
            root.path(),
            &run_id,
            &id,
            "doi",
            Some(&format!("10.1234/seed.{i:04}")),
            Some(&format!("Seed {i}")),
            &["Seed"],
            Some(2020),
        )
        .expect("seed evidence");
    }
    insert_evidence(
        root.path(),
        &run_id,
        "ev_url_doi",
        "url",
        Some("https://link.springer.com/article/10.1007/s13272-023-00702-w"),
        Some("Free fall drag estimation of small-scale multirotor unmanned aircraft systems"),
        &["Author"],
        Some(2023),
    )
    .expect("url evidence with DOI");
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "scope_disclaimer",
        "Scope-Hinweis",
        20,
        "Annahme: Schichtaufbau Variante A. Validierung an Coupons ist erforderlich. \
         Grenze: Inline-Pruefung wurde nicht beruecksichtigt.",
        &[],
    )
    .expect("scope_disclaimer");
    insert_committed_block(
        root.path(),
        &run_id,
        "doc_study",
        "context_and_question",
        "Kontext",
        50,
        "Die Drag-Quelle wird als URL-Evidence registriert, nennt aber sichtbar die DOI \
         10.1007/s13272-023-00702-w. Kontaktlose Pruefung des Kupfergitters wird sachlich \
         beschrieben. Schichtaufbau-Variante A bleibt der Bezug; Aerospace-konforme \
         Toleranzen gelten als gegeben. Die Methodenklasse haengt vom Probenkopf ab. \
         Sekundaereffekte sind ausgewiesen. Kontaktlose Pruefung des Kupfergitters wird \
         sachlich beschrieben. Schichtaufbau-Variante A bleibt der Bezug; Aerospace-konforme \
         Toleranzen gelten als gegeben. Die Methodenklasse haengt vom Probenkopf ab. \
         Sekundaereffekte sind ausgewiesen. Kontaktlose Pruefung des Kupfergitters wird \
         sachlich beschrieben. Schichtaufbau-Variante A bleibt der Bezug; Aerospace-konforme \
         Toleranzen gelten als gegeben. Die Methodenklasse haengt vom Probenkopf ab. \
         Sekundaereffekte sind ausgewiesen.",
        &["ev_url_doi"],
    )
    .expect("body block");
    let workspace = Workspace::load(root.path(), &run_id).expect("workspace");
    let outcome = run_release_guard_check(&workspace).expect("release_guard");
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
        !lint_ids.iter().any(|id| id == "LINT-FAB-DOI"),
        "DOI embedded in URL evidence should count as registered evidence; got {:?}",
        lint_ids
    );
}
