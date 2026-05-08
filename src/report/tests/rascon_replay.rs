//! RASCON-style replay test.
//!
//! Pre-populates a feasibility/ndt_aerospace/decision_grade run with
//! committed prose modelled on the RASCON archetype, builds the
//! manuscript, and validates the structural and verdict-line shape of
//! the rendered Markdown output. No LLM, no network.

use crate::report::render::{build_manuscript, render_markdown, MarkdownRenderOptions};
use crate::report::state::create_run;
use crate::report::tests::fixtures::{
    insert_committed_block, insert_evidence, rascon_create_params, TestRoot,
};
use crate::report::workspace::Workspace;

/// Sixteen RASCON-equivalent evidence rows. The IDs are arbitrary but
/// stable so block bodies can reference them by `ev_rascon_<n>`.
fn seed_rascon_evidence(root: &std::path::Path, run_id: &str) {
    let rows: &[(&str, &str, &str, &[&str], i64)] = &[
        (
            "ev_rascon_01",
            "10.1109/tii.2020.0001",
            "Eddy Current Inspection of CFRP",
            &["Feng"],
            2020,
        ),
        (
            "ev_rascon_02",
            "10.1109/tii.2020.0002",
            "Induction Thermography for CFRP",
            &["Mueller"],
            2019,
        ),
        (
            "ev_rascon_03",
            "10.1109/tii.2021.0003",
            "Lock-in Thermography Review",
            &["Schmidt"],
            2021,
        ),
        (
            "ev_rascon_04",
            "10.1016/j.ndt.2018.0004",
            "Air-Coupled Ultrasonic Inspection",
            &["Park"],
            2018,
        ),
        (
            "ev_rascon_05",
            "10.1016/j.ndt.2019.0005",
            "Microwave NDE for Composites",
            &["Tanaka"],
            2019,
        ),
        (
            "ev_rascon_06",
            "10.1016/j.ndt.2020.0006",
            "Terahertz Imaging Composites",
            &["Singh"],
            2020,
        ),
        (
            "ev_rascon_07",
            "10.1016/j.ndt.2021.0007",
            "Shearography for Aerospace",
            &["Lehmann"],
            2021,
        ),
        (
            "ev_rascon_08",
            "10.1016/j.compstruct.2020.0008",
            "LSP Mesh Defects",
            &["Bauer"],
            2020,
        ),
        (
            "ev_rascon_09",
            "10.1016/j.compstruct.2019.0009",
            "Lightning Strike Protection",
            &["Nakamura"],
            2019,
        ),
        (
            "ev_rascon_10",
            "10.1016/j.ndt.2018.0010",
            "POD Estimation NDT",
            &["Annis"],
            2018,
        ),
        (
            "ev_rascon_11",
            "10.1016/j.compstruct.2017.0011",
            "CFRP Layup Effects",
            &["Chung"],
            2017,
        ),
        (
            "ev_rascon_12",
            "10.1109/tii.2022.0012",
            "Eddy Current Probe Design",
            &["Hassan"],
            2022,
        ),
        (
            "ev_rascon_13",
            "10.1016/j.ndt.2022.0013",
            "Thermographic Defect Mapping",
            &["Roesch"],
            2022,
        ),
        (
            "ev_rascon_14",
            "10.1109/tii.2018.0014",
            "Frequency Response of Cu Mesh",
            &["Yamamoto"],
            2018,
        ),
        (
            "ev_rascon_15",
            "10.1016/j.ndt.2023.0015",
            "Industrial Inline NDT",
            &["Klein"],
            2023,
        ),
        (
            "ev_rascon_16",
            "10.1016/j.compstruct.2023.0016",
            "Validation Coupons CFRP",
            &["Diaz"],
            2023,
        ),
    ];
    for (eid, doi, title, authors, year) in rows {
        insert_evidence(
            root,
            run_id,
            eid,
            "doi",
            Some(doi),
            Some(title),
            authors,
            Some(*year),
        )
        .expect("seed evidence row");
    }
}

#[test]
fn rascon_replay_produces_feasibility_shaped_manuscript() {
    let root = TestRoot::new().expect("temp root");
    let run_id = create_run(root.path(), rascon_create_params()).expect("create_run");
    seed_rascon_evidence(root.path(), &run_id);

    // Compose committed blocks. Each is short prose (4-6 sentences).
    let blocks: &[(&str, &str, &str, i64, &str, &[&str])] = &[
        (
            "doc_study",
            "study_title_block",
            "Machbarkeitsstudie - Kontaktlose Pruefung des LSP-Kupfergitters",
            10,
            "Machbarkeitsstudie zur kontaktlosen Pruefung des Lightning-Strike-Protection-Kupfergitters \
             in CFRP-Strukturen. Untertitel: Methodenraum kontaktloser NDT-Verfahren mit Fokus auf \
             aerospace-Validierung. Versionslabel: Arbeitsfassung | Stand: 2026-05-08. Vorhaben: \
             Bewertung der Detektionsraten unterschiedlicher Methoden auf realitaetsnahen Coupons.",
            &[],
        ),
        (
            "doc_study",
            "scope_disclaimer",
            "Scope-Hinweis und Validierungsvorbehalt",
            20,
            "Diese Studie stuetzt sich auf oeffentlich zugaengliche Literatur zur kontaktlosen NDT \
             von CFRP-Strukturen. Annahme: das Kupfergitter ist auf der Aussenflaeche eingebettet. \
             Eine belastbare Aussage erfordert Validierung an repraesentativen Coupons. Limit: \
             Inline-Pruefung wurde nicht beruecksichtigt; Grenze der Aussage liegt bei \
             Schichtaufbau-Variante A.",
            &[],
        ),
        (
            "doc_study",
            "management_summary",
            "Management Summary",
            40,
            "Eddy Current Testing erreicht laut Feng et al. (2020) Detektionsraten von 87 Prozent \
             [ev:rascon_01]. Induction Thermography ergaenzt das Verfahren sekundaer [ev:rascon_02]. \
             Hauptrisiko: Anisotropie des Schichtaufbaus erschwert die Defektklassifikation. \
             Empfehlung: Phase 1 Coupons priorisieren, danach prototypische Inline-Integration.",
            &["ev_rascon_01", "ev_rascon_02"],
        ),
        (
            "doc_study",
            "context_and_question",
            "Kontext und Fragestellung",
            50,
            "Das Kupfergitter dient dem Lightning Strike Protection in CFRP-Strukturen. Aufgabe: \
             pruefen, welche kontaktlose NDT-Methode die Defekte zuverlaessig detektiert. \
             Validierung steht im Vordergrund; Annahmen werden explizit benannt; Limits ergeben \
             sich aus dem Rauschpegel der Methode.",
            &[],
        ),
        (
            "doc_study",
            "component_layout",
            "Bauteil und Layout",
            60,
            "Aussenlage CFRP, darunter Kupfergitter, danach weitere CFRP-Lagen. Defekte: \
             Aufrisse, Verschiebungen und Maschenbruch. Variante A: gleichmaessiger Schichtaufbau. \
             Variante B: lokale Verdickungen. Variante C: Worst-case mit dominanter Abschirmung.",
            &[],
        ),
        (
            "doc_study",
            "requirements",
            "Anforderungen",
            70,
            "Kontaktlose Anwendung [ev:rascon_05]. Einseitiger Zugriff [ev:rascon_06]. \
             Defektklassen: Aufrisse, Verschiebungen, Maschenbruch [ev:rascon_08]. \
             Aerospace-konforme POD-Ziele.",
            &["ev_rascon_05", "ev_rascon_06", "ev_rascon_08"],
        ),
        (
            "doc_study",
            "screening_logic",
            "Screening-Logik",
            80,
            "Die Bewertung folgt fuenf Achsen: Flaeche, Strukturbild, Defektsensitivitaet, \
             Sekundaereffekt-Sensitivitaet, Reifegrad. Jede Methode wird gegen jede Achse \
             qualitativ bewertet. Validierungsgrad ist explizite Voraussetzung.",
            &[],
        ),
        (
            "doc_study",
            "screening_matrix",
            "Bewertungsmatrix",
            90,
            "| Methode | Flaeche | Strukturbild | Defektsensitivitaet | Reifegrad |\n\
             | --- | --- | --- | --- | --- |\n\
             | Eddy Current | hoch | mittel | hoch | hoch |\n\
             | Induction Thermography | mittel | hoch | mittel | mittel |\n\
             | Air-Coupled Ultrasonic | hoch | mittel | mittel | mittel |\n",
            &[],
        ),
        (
            "doc_study",
            "detail_assessment_per_option",
            "Detailbewertung pro Methode",
            120,
            "Eddy Current Testing reagiert auf das Kupfergitter direkt; Defekte zeigen sich als \
             lokale Impedanzaenderungen. Aufloesung haengt vom Probenkopf ab; Park (2018) berichtet \
             POD 87 Prozent fuer 1mm-Defekte [ev:rascon_04]. Induction Thermography ergaenzt mit \
             einem Sekundaersignal aus Waermeleitung. Validierung an Coupons mit Variante A bestaetigt \
             die Erwartung. Erfolgsaussichten (qualitativ): hoch fuer Eddy Current; mittel fuer \
             Induction Thermography.",
            &["ev_rascon_04", "ev_rascon_02"],
        ),
        (
            "doc_study",
            "risk_register",
            "Risikoregister",
            140,
            "R1: Anisotropie des Schichtaufbaus (Mitigation: variantenspezifische Kalibration). \
             R2: Sekundaereffekt durch Feuchtigkeit (Mitigation: kontrollierte Klimakammer). \
             R3: Schwankende Maschengeometrie (Mitigation: Toleranzbereich messen). \
             R4: Thermisches Rauschen (Mitigation: Lock-in Auswertung). \
             R5: Inline-Reifegrad (Mitigation: Phase 2 Prototyp).",
            &[],
        ),
        (
            "doc_study",
            "recommendation",
            "Empfehlung",
            150,
            "Phase 1: Coupons mit Schichtaufbau-Variante A pruefen. Phase 2: prototypische \
             Inline-Integration auf Variante B. No-Go-Kriterium: POD unter 60 Prozent fuer \
             priorisierte Defektklassen.",
            &[],
        ),
        (
            "doc_study",
            "appendix_sources",
            "Anhang Quellen",
            200,
            "Quellenliste mit Verweis auf das Evidence-Register dieses Runs. \
             Sechzehn Quellen sind aktuell registriert.",
            &["ev_rascon_01", "ev_rascon_02"],
        ),
    ];

    for (doc_id, block_id, title, ord, body, refs) in blocks {
        insert_committed_block(
            root.path(),
            &run_id,
            doc_id,
            block_id,
            title,
            *ord,
            body,
            refs,
        )
        .expect("insert RASCON-style committed block");
    }

    let workspace = Workspace::load(root.path(), &run_id).expect("workspace load");
    let manuscript = build_manuscript(&workspace).expect("build manuscript");

    assert_eq!(
        manuscript.manifest.report_type_id, "feasibility_study",
        "manuscript manifest must reflect feasibility_study"
    );
    assert!(
        manuscript.title.contains("Machbarkeitsstudie") || manuscript.title.contains("Feasibility"),
        "title should mention feasibility/Machbarkeitsstudie; got {:?}",
        manuscript.title
    );
    let scope = manuscript.scope_disclaimer.as_str();
    assert!(
        scope.to_lowercase().contains("arbeitsentwurf") || scope.to_lowercase().contains("draft"),
        "auto scope disclaimer should mention working-draft status; got {scope:?}"
    );

    let total_blocks: usize = manuscript.docs.iter().map(|d| d.blocks.len()).sum();
    assert!(
        total_blocks >= 8,
        "expected >= 8 manuscript blocks; got {total_blocks} across {} docs",
        manuscript.docs.len()
    );
    assert!(
        !manuscript.docs.is_empty(),
        "manuscript must contain at least one doc"
    );

    assert!(
        !manuscript.references.is_empty(),
        "manuscript references[] must be non-empty for a run that cites evidence"
    );

    let md = render_markdown(&manuscript, &MarkdownRenderOptions::default());
    assert!(
        md.contains("Erfolgsaussichten (qualitativ):"),
        "rendered markdown must contain the verdict-line phrase; first 400 chars:\n{}",
        &md.chars().take(400).collect::<String>()
    );
    // The committed scope_disclaimer block flows into the manuscript as
    // a normal block; its prose must surface in the rendered Markdown.
    assert!(
        md.to_lowercase().contains("validierung") || md.to_lowercase().contains("validation"),
        "rendered markdown should reflect the committed scope_disclaimer's validation note"
    );
}
