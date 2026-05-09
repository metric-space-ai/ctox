//! Deterministic client-deliverable quality gate.
//!
//! The other checks validate run state (blocks, budget, lints, order). This
//! gate validates whether the assembled manuscript can plausibly become a
//! client-ready report: no leaked authoring syntax, no internal tool language,
//! and enough structured visual/table material for report types that require a
//! real Word deliverable.

use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use regex::Regex;
use serde_json::{json, Value};

use crate::report::checks::{dedupe_keep_order, CheckOutcome};
use crate::report::render::manuscript::{build_manuscript, ManuscriptBlockKind};
use crate::report::workspace::{EvidenceEntry, Workspace};

const CHECK_KIND: &str = "deliverable_quality";

#[derive(Debug, Clone)]
struct Issue {
    lint_id: &'static str,
    instance_id: Option<String>,
    reason: String,
    goal: String,
}

pub fn run_deliverable_quality_check(workspace: &Workspace<'_>) -> Result<CheckOutcome> {
    let committed = workspace.committed_blocks()?;
    if committed.is_empty() {
        let payload = json!({
            "summary": "Keine populated blocks - Deliverable-Qualitaet uebersprungen.",
            "check_applicable": false,
            "ready_to_finish": true,
            "needs_revision": false,
            "candidate_instance_ids": Value::Array(Vec::new()),
            "goals": Value::Array(Vec::new()),
            "reasons": Value::Array(Vec::new()),
            "issues": Value::Array(Vec::new()),
        });
        return Ok(CheckOutcome {
            check_kind: CHECK_KIND.to_string(),
            summary: "Keine populated blocks - Deliverable-Qualitaet uebersprungen.".to_string(),
            check_applicable: false,
            ready_to_finish: true,
            needs_revision: false,
            candidate_instance_ids: Vec::new(),
            goals: Vec::new(),
            reasons: Vec::new(),
            raw_payload: payload,
        }
        .cap());
    }

    let metadata = workspace.run_metadata()?;
    let evidence_register = workspace.evidence_register()?;
    let research_log_entries = workspace.research_log_entries()?;
    let manuscript = build_manuscript(workspace)?;
    let mut issues = Vec::new();
    let mut project_specificity_metrics: Option<ProjectSpecificityMetrics> = None;
    let markdown_heading = Regex::new(r"(?m)^\s{0,3}#{1,6}\s+\S").expect("valid regex");
    let csv_like =
        Regex::new(r"(?m)^[^|.\n]{1,180}(?:,[^,|.\n]{1,80}){4,}\s*$").expect("valid regex");
    let raw_xref = Regex::new(r"\{\{(fig|tbl):([^}\s]+)\}\}").expect("valid regex");
    let bare_evidence_id = Regex::new(r"(?i)\bev_?[a-f0-9]{12,}\b").expect("valid regex");
    let bracket_citation_chain = Regex::new(r"(?:\[\d{1,3}\]){2,}").expect("valid regex");
    let internal_terms = [
        "workspace",
        "ctox",
        "run_id",
        "evidence_id",
        "helper_manifest",
        "artifact contract",
        "working draft",
        "arbeitsentwurf",
        "confidential working draft",
        "vertraulicher arbeitsentwurf",
        "synthesis/",
        "research_workspace",
        "tool call",
        "qa-notes",
        "client-ready",
    ];

    let mut structured_table_instances: HashSet<String> = HashSet::new();
    let structured_figure_ids: HashSet<String> = manuscript
        .structured_figures
        .iter()
        .map(|figure| figure.figure_id.clone())
        .collect();
    let structured_table_ids: HashSet<String> = manuscript
        .structured_tables
        .iter()
        .map(|table| table.table_id.clone())
        .collect();
    for table in &manuscript.structured_tables {
        if let Some(instance_id) = &table.instance_id {
            structured_table_instances.insert(instance_id.clone());
        }
    }

    let mut all_text = String::new();
    all_text.push_str(&manuscript.title);
    all_text.push('\n');
    if let Some(subtitle) = &manuscript.subtitle {
        all_text.push_str(subtitle);
        all_text.push('\n');
    }
    all_text.push_str(&manuscript.scope_disclaimer);

    for doc in &manuscript.docs {
        all_text.push('\n');
        all_text.push_str(&doc.title);
        for block in &doc.blocks {
            all_text.push('\n');
            all_text.push_str(&block.title);
            all_text.push('\n');
            all_text.push_str(&block.markdown);

            if markdown_heading.is_match(&block.markdown) {
                issues.push(Issue {
                    lint_id: "DELIVERABLE-MARKDOWN-HEADING",
                    instance_id: Some(block.instance_id.clone()),
                    reason: format!(
                        "Block '{}' enthaelt sichtbare Markdown-Ueberschriften.",
                        block.title
                    ),
                    goal: format!(
                        "Entferne alle Markdown-Heading-Zeilen aus {}. Der Blocktitel kommt aus dem Block-Metadatensatz, nicht aus dem Markdown.",
                        block.instance_id
                    ),
                });
            }
            for cap in raw_xref.captures_iter(&block.markdown) {
                let kind = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
                let id = cap.get(2).map(|m| m.as_str()).unwrap_or_default();
                let known = match kind {
                    "fig" => structured_figure_ids.contains(id),
                    "tbl" => structured_table_ids.contains(id),
                    _ => false,
                };
                if !known {
                    issues.push(Issue {
                        lint_id: "DELIVERABLE-UNRESOLVED-XREF",
                        instance_id: Some(block.instance_id.clone()),
                        reason: format!(
                            "Block '{}' referenziert {kind}:{id}, aber dieses Artefakt ist nicht registriert.",
                            block.title
                        ),
                        goal: format!(
                            "Registriere {kind}:{id} per figure-add/table-add oder entferne den Platzhalter aus {}.",
                            block.instance_id
                        ),
                    });
                }
            }
            if csv_like.is_match(&block.markdown) {
                issues.push(Issue {
                    lint_id: "DELIVERABLE-CSV-AS-PROSE",
                    instance_id: Some(block.instance_id.clone()),
                    reason: format!(
                        "Block '{}' enthaelt tabellenartige CSV-Zeilen im Fliesstext.",
                        block.title
                    ),
                    goal: format!(
                        "Wandle tabellarische Inhalte in {} in eine echte Tabelle um: bevorzugt ctox report table-add, alternativ eine saubere Markdown-Pipe-Table.",
                        block.instance_id
                    ),
                });
            }

            let must_be_table = matches!(
                block.kind,
                ManuscriptBlockKind::Matrix
                    | ManuscriptBlockKind::ScenarioGrid
                    | ManuscriptBlockKind::AbbreviationTable
                    | ManuscriptBlockKind::CompetitorMatrix
                    | ManuscriptBlockKind::CriteriaTable
                    | ManuscriptBlockKind::DefectCatalog
            );
            let has_structured_table = structured_table_instances.contains(&block.instance_id);
            if must_be_table && block.table.is_none() && !has_structured_table {
                issues.push(Issue {
                    lint_id: "DELIVERABLE-MISSING-TYPED-TABLE",
                    instance_id: Some(block.instance_id.clone()),
                    reason: format!(
                        "Block '{}' ist ein Tabellenblock, hat aber keine renderbare Tabelle.",
                        block.title
                    ),
                    goal: format!(
                        "Erzeuge fuer {} eine echte Tabelle per ctox report table-add oder als Markdown-Pipe-Table.",
                        block.instance_id
                    ),
                });
            }
        }
    }

    let all_lower = all_text.to_lowercase();
    for term in internal_terms {
        if all_lower.contains(term) {
            issues.push(Issue {
                lint_id: "DELIVERABLE-INTERNAL-LANGUAGE",
                instance_id: None,
                reason: format!(
                    "Das Manuskript enthaelt internes Tool-/Workspace-Vokabular: '{term}'."
                ),
                goal: "Entferne interne Arbeitsbegriffe aus Titel, Disclaimer und Blocktext; der Report muss wie ein Kundendokument lesbar sein.".to_string(),
            });
        }
    }
    if contains_unbracketed_evidence_id(&all_text, &bare_evidence_id) {
        issues.push(Issue {
            lint_id: "DELIVERABLE-LEAKED-EVIDENCE-ID",
            instance_id: None,
            reason: "Das Manuskript enthaelt rohe Evidence-IDs im sichtbaren Text.".to_string(),
            goal: "Ersetze rohe ev-/ev_-IDs durch bracketed Evidence-Zitationen, die der Renderer in numerische Quellenhinweise umwandelt; bei Bedarf --used-reference-ids am Block korrigieren.".to_string(),
        });
    }

    if metadata.report_type_id == "feasibility_study" {
        let figure_count = manuscript.structured_figures.len();
        let table_count = manuscript.structured_tables.len();
        let missing_image_count = manuscript
            .structured_figures
            .iter()
            .filter(|f| !Path::new(&f.image_path).is_file())
            .count();
        for figure in &manuscript.structured_figures {
            let caption_lower = figure.caption.to_lowercase();
            let source_lower = figure.source_label.to_lowercase();
            let instance_lower = figure.instance_id.as_deref().unwrap_or("").to_lowercase();
            if contains_placeholder_text(&caption_lower)
                || contains_placeholder_text(&source_lower)
                || contains_placeholder_text(&instance_lower)
            {
                issues.push(Issue {
                    lint_id: "DELIVERABLE-PLACEHOLDER-FIGURE",
                    instance_id: figure.instance_id.clone(),
                    reason: format!(
                        "Abbildung {} enthaelt Platzhalter-/Testvokabular.",
                        figure.figure_id
                    ),
                    goal: format!(
                        "Ersetze Abbildung {} durch eine fachlich benannte, zitierfaehige Abbildung mit aussagekraeftiger Caption und korrekter Block-Zuordnung.",
                        figure.figure_id
                    ),
                });
            }
        }
        for table in &manuscript.structured_tables {
            for (idx, row) in table.rows.iter().enumerate() {
                if row.len() != table.headers.len() {
                    issues.push(Issue {
                        lint_id: "DELIVERABLE-TABLE-SHAPE",
                        instance_id: table.instance_id.clone(),
                        reason: format!(
                            "Tabelle {} hat in Datenzeile {} {} Zellen, aber {} Header.",
                            table.table_id,
                            idx + 1,
                            row.len(),
                            table.headers.len()
                        ),
                        goal: format!(
                            "Registriere Tabelle {} mit konsistenter Spaltenzahl neu; Kommas in Zellentexten muessen im CSV korrekt quotiert oder umformuliert werden.",
                            table.table_id
                        ),
                    });
                }
            }
        }
        let has_matrix = manuscript
            .structured_tables
            .iter()
            .any(|t| t.kind == "matrix")
            || manuscript
                .docs
                .iter()
                .flat_map(|d| d.blocks.iter())
                .any(|b| matches!(b.kind, ManuscriptBlockKind::Matrix) && b.table.is_some());
        let has_scenario = manuscript
            .structured_tables
            .iter()
            .any(|t| t.kind == "scenario")
            || manuscript
                .docs
                .iter()
                .flat_map(|d| d.blocks.iter())
                .any(|b| matches!(b.kind, ManuscriptBlockKind::ScenarioGrid) && b.table.is_some());

        if figure_count < 2 {
            issues.push(Issue {
                lint_id: "DELIVERABLE-MIN-FIGURES",
                instance_id: None,
                reason: format!(
                    "Machbarkeitsstudien brauchen mindestens zwei eingebettete Abbildungen; vorhanden: {figure_count}."
                ),
                goal: "Fuege mindestens zwei belastbare Abbildungen mit Quellen per ctox report figure-add hinzu: z.B. Aufbau/Schnittbild, Methodenschema, Prozess-/Versuchsdesign oder quellenbasierte Referenzgrafik.".to_string(),
            });
        }
        if missing_image_count > 0 {
            issues.push(Issue {
                lint_id: "DELIVERABLE-BROKEN-FIGURE-PATH",
                instance_id: None,
                reason: format!(
                    "{missing_image_count} registrierte Abbildung(en) zeigen auf nicht vorhandene Bilddateien."
                ),
                goal: "Repariere die figure-add-Artefakte so, dass jede image_path-Datei existiert und in das DOCX eingebettet werden kann.".to_string(),
            });
        }
        if table_count < 3 {
            issues.push(Issue {
                lint_id: "DELIVERABLE-MIN-TABLES",
                instance_id: None,
                reason: format!(
                    "Machbarkeitsstudien brauchen mindestens drei strukturierte Tabellen; vorhanden: {table_count}."
                ),
                goal: "Fuege strukturierte Tabellen per ctox report table-add hinzu: Bewertungsmatrix, Szenario-/Randbedingungsmatrix und Risiko-/Versuchsplan-/Defektkatalog-Tabelle.".to_string(),
            });
        }
        if !has_matrix {
            issues.push(Issue {
                lint_id: "DELIVERABLE-MISSING-EVALUATION-MATRIX",
                instance_id: Some("doc_study__screening_matrix".to_string()),
                reason: "Keine renderbare Bewertungsmatrix gefunden.".to_string(),
                goal: "Lege eine echte Bewertungsmatrix an und binde sie an den Bewertungsmatrix-Block.".to_string(),
            });
        }
        if !has_scenario {
            issues.push(Issue {
                lint_id: "DELIVERABLE-MISSING-SCENARIO-MATRIX",
                instance_id: Some("doc_study__scenario_matrix".to_string()),
                reason: "Keine renderbare Szenario-Matrix gefunden.".to_string(),
                goal: "Lege eine echte Szenario-/Randbedingungsmatrix an und binde sie an den Szenario-Block.".to_string(),
            });
        }
    }

    if metadata.report_type_id == "project_description" {
        let specificity =
            project_specificity_metrics_for(&all_text, &metadata.raw_topic, &evidence_register);
        if specificity.evidence_entries_with_candidate_anchors >= 6 {
            let min_visible_entries = ((specificity.evidence_entries_with_candidate_anchors * 60)
                .div_ceil(100))
            .clamp(5, 12);
            let min_visible_anchors = specificity
                .evidence_entries_with_candidate_anchors
                .clamp(8, 16);
            if specificity.visible_evidence_entries < min_visible_entries
                || specificity.visible_anchor_count < min_visible_anchors
            {
                issues.push(Issue {
                    lint_id: "PROJECT-LOW-EVIDENCE-FACT-TRANSFER",
                    instance_id: None,
                    reason: format!(
                        "Das Evidence-Register enthaelt {} verwertbare Faktencluster, aber im Kundentext erscheinen nur {} evidence-spezifische Anker aus {} Quellen; erwartet sind mindestens {} Quellenanker und {} konkrete Anker.",
                        specificity.evidence_entries_with_candidate_anchors,
                        specificity.visible_anchor_count,
                        specificity.visible_evidence_entries,
                        min_visible_entries,
                        min_visible_anchors
                    ),
                    goal: "Erstelle vor dem Schreiben ein internes Fact-Ledger und uebertrage konkrete, nicht nur aus dem Prompt stammende Fakten in die Projektbeschreibung: Unternehmensdaten, Standort/Rechts-/Historienanker, Produkt-/Leistungsdetails, Kunden-/Segmentbelege, technische Projektanker, Wettbewerbs-/Marktankerpunkte und Zahlen. Keine Quellen sichtbar zitieren, aber die Fakten muessen im Fliesstext erkennbar sein.".to_string(),
                });
            }
        }
        project_specificity_metrics = Some(specificity);

        let scope_block = manuscript
            .docs
            .iter()
            .flat_map(|d| d.blocks.iter())
            .find(|b| b.block_id == "project_scope_budget_timeline");
        let benefit_block = manuscript
            .docs
            .iter()
            .flat_map(|d| d.blocks.iter())
            .find(|b| b.block_id == "project_economic_benefit");

        let citation_leaks = bracket_citation_chain.find_iter(&all_text).count();
        if citation_leaks >= 3 {
            issues.push(Issue {
                lint_id: "PROJECT-VISIBLE-CITATION-LEAK",
                instance_id: None,
                reason: format!(
                    "Die Projektbeschreibung enthaelt {citation_leaks} sichtbare numerische Zitationsketten."
                ),
                goal: "Formuliere die recherchierten Fakten als integrierte Projektbeschreibung ohne wissenschaftliche [1][2]-Zitierketten; Quellen bleiben internes Evidence-Register.".to_string(),
            });
        }

        let source_appendix_terms = [
            "anhang - quellen",
            "anhang – quellen",
            "anhang — quellen",
            "quellen und recherchebasis",
            "bibliographie",
            "literaturverzeichnis",
            "references",
        ];
        if source_appendix_terms
            .iter()
            .any(|term| all_lower.contains(term))
        {
            issues.push(Issue {
                lint_id: "PROJECT-RESEARCH-APPENDIX-LEAK",
                instance_id: Some("doc_project_description__appendix_sources_project".to_string()),
                reason: "Die Projektbeschreibung enthaelt einen sichtbaren Quellen-/Rechercheanhang.".to_string(),
                goal: "Entferne den Quellenanhang aus dem finalen Kundendokument. Research ist fuer diesen Reporttyp nur Arbeitsgrundlage; die Fakten muessen im Fliesstext aufgehen.".to_string(),
            });
        }

        let min_chars = match metadata.depth_profile_id.as_str() {
            "brief" => 12_000,
            "decision_grade" => 26_000,
            _ => 24_000,
        };
        if all_text.chars().count() < min_chars {
            issues.push(Issue {
                lint_id: "PROJECT-DESCRIPTION-TOO-THIN",
                instance_id: None,
                reason: format!(
                    "Die Projektbeschreibung ist mit {} Zeichen zu knapp fuer ein belastbares Foerdervorhaben-Dokument.",
                    all_text.chars().count()
                ),
                goal: "Baue Unternehmensprofil, Entwicklungsgeschichte, Produkte/Kunden, Problemherleitung, Zielbild, Umsetzung, Kosten/Zeitraum und Nutzen substanzieller aus; keine Quellenanhaenge als Laengenfueller verwenden.".to_string(),
            });
        }

        let required_narrative_terms: [(&str, &[&str]); 8] = [
            (
                "Unternehmensprofil / Antragsteller",
                &[
                    "unternehmensprofil",
                    "gesellschaft",
                    "antragsteller",
                    "rechtsform",
                ],
            ),
            (
                "Unternehmensentwicklung / Historie",
                &[
                    "historie",
                    "entwicklung",
                    "gegruendet",
                    "gegründet",
                    "branchenerfahrung",
                ],
            ),
            (
                "Produkte / Leistungen / Kundensegmente",
                &[
                    "produkte",
                    "leistungen",
                    "kundensegmente",
                    "portfolio",
                    "servicevertraege",
                    "serviceverträge",
                ],
            ),
            (
                "Status quo / Problembereich",
                &[
                    "status quo",
                    "derzeitiger stand",
                    "problembereich",
                    "ausgangszustand",
                    "engpass",
                ],
            ),
            (
                "Entwicklungsziel / Zielbild",
                &[
                    "entwicklungsziel",
                    "zielbild",
                    "soll-zustand",
                    "ziel des vorhabens",
                ],
            ),
            (
                "Stand der Technik / Marktabgrenzung",
                &[
                    "stand der technik",
                    "abgrenzung",
                    "marktuebliche",
                    "marktübliche",
                    "wettbewerber",
                ],
            ),
            (
                "Herausforderungen und Massnahmen",
                &[
                    "herausforderungen",
                    "maßnahmen",
                    "massnahmen",
                    "risiken",
                    "abhängigkeiten",
                    "abhaengigkeiten",
                ],
            ),
            (
                "Arbeitspakete / Umsetzung",
                &[
                    "arbeitspaket",
                    "arbeitspakete",
                    "umsetzung",
                    "umsetzungsschwerpunkt",
                    "umsetzungsschwerpunkte",
                    "umsetzungsschritte",
                    "meilenstein",
                    "rollout",
                ],
            ),
        ];
        for (label, terms) in required_narrative_terms {
            if !terms.iter().any(|term| all_lower.contains(term)) {
                issues.push(Issue {
                    lint_id: "PROJECT-MISSING-FUNDING-NARRATIVE-PART",
                    instance_id: None,
                    reason: format!(
                        "Der Foerdervorhaben-Erzaehlbogen deckt '{label}' nicht erkennbar ab."
                    ),
                    goal: "Ergaenze die Projektbeschreibung so, dass Unternehmensstory, Status quo, Problem, Ziel, Abgrenzung, Herausforderungen, Umsetzung und Nutzen als roter Faden zusammenhaengen.".to_string(),
                });
            }
        }

        if all_lower.contains("forschungsfrage")
            || all_lower.contains("literatur")
            || all_lower.contains("screening")
            || all_lower.contains("evidenz")
        {
            issues.push(Issue {
                lint_id: "PROJECT-SCIENTIFIC-REPORT-VOICE",
                instance_id: None,
                reason: "Die Projektbeschreibung enthaelt Sprache aus Research-/Studienformaten statt Foerdervorhaben-Prosa.".to_string(),
                goal: "Schreibe aus der Perspektive des Vorhabens: Unternehmen, Betriebsrealitaet, Engpass, Innovationssprung, Umsetzung und Nutzen. Research-Begriffe bleiben intern.".to_string(),
            });
        }

        if let Some(block) = scope_block {
            let lower = block.markdown.to_lowercase();
            let has_duration = lower.contains("laufzeit")
                || lower.contains("umsetzungszeitraum")
                || lower.contains("monat");
            let has_budget = lower.contains("budget")
                || lower.contains("kosten")
                || lower.contains("teur")
                || lower.contains("eur");
            let has_status = lower.contains("status")
                || lower.contains("nicht begonnen")
                || lower.contains("noch nicht begonnen")
                || lower.contains("projektfreigabe");
            if !has_duration || !has_budget || !has_status {
                issues.push(Issue {
                    lint_id: "PROJECT-INCOMPLETE-SCOPE",
                    instance_id: Some(block.instance_id.clone()),
                    reason: "Der Projektumfang enthaelt nicht belastbar Laufzeit, Budget/Kosten und Projektstatus.".to_string(),
                    goal: "Ergaenze im Projektumfang Laufzeit/Zeitraum, Status (begonnen/nicht begonnen) und Budget/Kostenbloecke mit Einheiten.".to_string(),
                });
            }
            let scope_table_expected = has_duration || has_budget || has_status;
            let has_scope_table =
                block.table.is_some() || structured_table_instances.contains(&block.instance_id);
            if scope_table_expected && !has_scope_table {
                issues.push(Issue {
                    lint_id: "PROJECT-MISSING-SCOPE-TABLE",
                    instance_id: Some(block.instance_id.clone()),
                    reason: "Der Projektumfang nennt Laufzeit, Status oder Budget, ist aber nicht als kompakte Projektrahmen-Tabelle gerendert.".to_string(),
                    goal: "Fuege eine echte Tabelle per ctox report table-add hinzu, gebunden an project_scope_budget_timeline, mit mindestens Laufzeit, Status, Budget und Kostenbloecken.".to_string(),
                });
            }
        }

        if let Some(block) = benefit_block {
            let lower = block.markdown.to_lowercase();
            if !lower.contains("projektplausibilität") && !lower.contains("projektplausibilitaet")
            {
                issues.push(Issue {
                    lint_id: "PROJECT-MISSING-PLAUSIBILITY-LINE",
                    instance_id: Some(block.instance_id.clone()),
                    reason: "Der Schlussblock enthaelt keine Projektplausibilitaetslinie.".to_string(),
                    goal: "Fuege im wirtschaftlichen Nutzen eine Schlusslinie im Muster 'Projektplausibilitaet (qualitativ): <level>; Foerderlogik: <condition>.' hinzu.".to_string(),
                });
            }
            let has_economic_mechanism = [
                "umsatz",
                "kosten",
                "marge",
                "skalier",
                "service",
                "kunden",
                "wettbewerb",
                "wertschöpf",
                "wertschoepf",
            ]
            .iter()
            .any(|needle| lower.contains(needle));
            if !has_economic_mechanism {
                issues.push(Issue {
                    lint_id: "PROJECT-WEAK-ECONOMIC-BENEFIT",
                    instance_id: Some(block.instance_id.clone()),
                    reason: "Der wirtschaftliche Nutzen benennt keinen konkreten betriebswirtschaftlichen Wirkmechanismus.".to_string(),
                    goal: "Verbinde das Vorhaben mit konkreten Nutzenhebeln wie Umsatz, Kosten, Marge, Skalierung, Servicequalitaet, Kundenbindung oder Wettbewerbsfaehigkeit.".to_string(),
                });
            }
        }
    }

    if metadata.report_type_id == "source_review" {
        let table_count = manuscript.structured_tables.len()
            + manuscript
                .docs
                .iter()
                .flat_map(|d| d.blocks.iter())
                .filter(|b| b.table.is_some())
                .count();
        let source_catalog_block = manuscript
            .docs
            .iter()
            .flat_map(|d| d.blocks.iter())
            .find(|b| b.block_id == "source_review_catalog");
        let search_method_block = manuscript
            .docs
            .iter()
            .flat_map(|d| d.blocks.iter())
            .find(|b| b.block_id == "source_review_search_method");
        let committed_search_method_block = committed
            .iter()
            .find(|b| b.block_id == "source_review_search_method");
        let coverage_block = manuscript
            .docs
            .iter()
            .flat_map(|d| d.blocks.iter())
            .find(|b| b.block_id == "source_review_coverage_gaps");
        let min_refs: usize = match metadata.depth_profile_id.as_str() {
            "decision_grade" => 120,
            "standard" => 80,
            _ => 12,
        };
        let min_catalog_rows = match metadata.depth_profile_id.as_str() {
            "decision_grade" => 300,
            "standard" => 150,
            _ => 40,
        };
        let min_screened_candidates = match metadata.depth_profile_id.as_str() {
            "decision_grade" => 2_500,
            "standard" => 1_000,
            _ => 250,
        };
        let persisted_evidence = evidence_register.len() as i64;
        let persisted_research_sources: i64 = research_log_entries
            .iter()
            .map(|entry| entry.sources_count.max(0))
            .sum();
        let research_focuses: HashSet<String> = research_log_entries
            .iter()
            .filter_map(|entry| entry.focus.as_deref())
            .map(|focus| focus.trim().to_lowercase())
            .filter(|focus| !focus.is_empty())
            .collect();
        let has_snowballing_log = research_focuses.iter().any(|focus| {
            focus.contains("snowball")
                || focus.contains("citation")
                || focus.contains("cited")
                || focus.contains("references")
        });
        let persisted_with_url = evidence_register
            .iter()
            .filter(|entry| {
                entry
                    .url_canonical
                    .as_deref()
                    .or(entry.url_full_text.as_deref())
                    .map(|url| url.starts_with("http://") || url.starts_with("https://"))
                    .unwrap_or(false)
            })
            .count() as i64;
        let persisted_text_backed = evidence_register
            .iter()
            .filter(|entry| entry.content_chars >= 500)
            .count() as i64;
        let evidence_by_id: std::collections::HashMap<&str, i64> = evidence_register
            .iter()
            .map(|entry| (entry.evidence_id.as_str(), entry.content_chars))
            .collect();
        let cited_text_backed = manuscript
            .references
            .iter()
            .filter(|reference| {
                evidence_by_id
                    .get(reference.evidence_id.as_str())
                    .copied()
                    .unwrap_or(0)
                    >= 500
            })
            .count() as i64;

        if manuscript.references.len() < min_refs {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-LOW-REFERENCE-COUNT",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Source Reviews brauchen breite Quellenabdeckung; sichtbar zitiert: {}, erwartet fuer {}: mindestens {}.",
                    manuscript.references.len(),
                    metadata.depth_profile_id,
                    min_refs
                ),
                goal: "Erweitere Recherche und Quellenkatalog mit weiteren belastbaren Web-, Standard-, Regulierungs-, Datensatz-, Industrie- und Literaturquellen; zitiere sie in den passenden Bloecken.".to_string(),
            });
        }
        if persisted_evidence < min_refs as i64 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-LOW-PERSISTED-EVIDENCE",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Im Evidence-Register sind nur {persisted_evidence} Quellen gespeichert; erwartet fuer {} sind mindestens {min_refs}.",
                    metadata.depth_profile_id
                ),
                goal: "Persistiere die tatsaechlich ausgewerteten Quellen per add-evidence; eine sichtbare Quellenzeile darf nicht nur im Word-Text existieren.".to_string(),
            });
        }
        if persisted_with_url * 100 < persisted_evidence.max(1) * 95 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-PERSISTED-EVIDENCE-MISSING-URLS",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Nur {persisted_with_url} von {persisted_evidence} gespeicherten Quellen haben eine HTTP(S)-URL."
                ),
                goal: "Registriere Quellen mit nachpruefbarer URL/DOI/Zugriffsseite; Quellen ohne Zugriff duerfen hoechstens als nicht auswertbare Luecke erscheinen.".to_string(),
            });
        }
        if persisted_text_backed * 100 < persisted_evidence.max(1) * 75 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-PERSISTED-EVIDENCE-NOT-TEXT-BACKED",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Nur {persisted_text_backed} von {persisted_evidence} gespeicherten Quellen enthalten mindestens 500 Zeichen auswertbaren Text."
                ),
                goal: "Lies oder extrahiere fuer die ausgewerteten Quellen Abstract, Snippet oder Volltext; reine Metadatenzeilen zaehlen nicht als gelesene Quelle.".to_string(),
            });
        }
        if cited_text_backed * 100 < (manuscript.references.len() as i64).max(1) * 80 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-CITED-EVIDENCE-NOT-TEXT-BACKED",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Nur {cited_text_backed} von {} sichtbar zitierten Quellen enthalten mindestens 500 Zeichen auswertbaren Text im Evidence-Register.",
                    manuscript.references.len()
                ),
                goal: "Zitiere fuer die Auswertung ueberwiegend Quellen, deren Inhalt im Evidence-Register gespeichert ist; Quellen ohne Textbasis nur als Randhinweis oder Luecke fuehren.".to_string(),
            });
        }

        let source_review_terms = [
            "screening-ledger",
            "screening ledger",
            "screened-candidate",
            "screened candidate",
            "candidate hits",
            "query/",
            "nutzbar/zitiert",
            "usable/cited",
        ];
        for term in source_review_terms {
            if all_lower.contains(term) {
                issues.push(Issue {
                    lint_id: "SOURCE-REVIEW-CLIENT-LANGUAGE-LEAK",
                    instance_id: None,
                    reason: format!(
                        "Der Source Review enthaelt kundenunfreundliches Arbeitsvokabular: '{term}'."
                    ),
                    goal: "Formuliere das Dokument als lesbaren Quellenbericht: z.B. 'search protocol', 'search term', 'reviewed results', 'included sources', 'excluded results' statt interner Ledger-/Candidate-Begriffe.".to_string(),
                });
            }
        }

        if table_count < 2 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-MIN-TABLES",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Source Reviews brauchen mindestens zwei strukturierte Tabellen (Quellenkatalog und Daten-/Coverage-Tabelle); vorhanden: {table_count}."
                ),
                goal: "Erzeuge mindestens zwei echte Tabellen: einen Quellenkatalog und eine Datenextraktions- oder Coverage/Gaps-Tabelle.".to_string(),
            });
        }

        if let Some(block) = source_catalog_block {
            if block.table.is_none()
                && !manuscript
                    .structured_tables
                    .iter()
                    .any(|t| t.instance_id.as_deref() == Some(block.instance_id.as_str()))
            {
                issues.push(Issue {
                    lint_id: "SOURCE-REVIEW-MISSING-CATALOG-TABLE",
                    instance_id: Some(block.instance_id.clone()),
                    reason: "Der Quellenkatalog ist nicht als renderbare Tabelle angelegt.".to_string(),
                    goal: "Lege den Quellenkatalog als echte Tabelle mit Quelle, Typ, Herausgeber/Autor, Jahr, Dateninhalt, Relevanz und Zugriff/URL oder DOI an.".to_string(),
                });
            }
        }

        let catalog_quality = source_review_catalog_quality(&manuscript);
        if catalog_quality.source_rows < min_catalog_rows {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-TOO-FEW-CATALOG-SOURCES",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Der sichtbare Quellenkatalog enthaelt nur {} Quellenzeilen; erwartet fuer {} sind mindestens {}.",
                    catalog_quality.source_rows,
                    metadata.depth_profile_id,
                    min_catalog_rows
                ),
                goal: "Erweitere den Quellenkatalog deutlich und fuehre die relevanten Quellen gruppiert auf. Ein Review-Paper-aehnlicher Source Review darf nicht nur eine kleine Auswahl zeigen, wenn die Suche hunderte nutzbare Treffer behauptet.".to_string(),
            });
        }
        if catalog_quality.source_rows > 0
            && catalog_quality.source_rows * 100 > persisted_evidence.max(1) * 120
        {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-CATALOG-NOT-EVIDENCE-BACKED",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Der sichtbare Quellenkatalog enthaelt {} Quellenzeilen, aber nur {persisted_evidence} Quellen sind im Evidence-Register gespeichert.",
                    catalog_quality.source_rows
                ),
                goal: "Kopple den Quellenkatalog an das Evidence-Register: jede ausgewertete Katalogquelle muss persistiert sein oder klar als nicht ausgewerteter Kandidat gekennzeichnet werden.".to_string(),
            });
        }
        if catalog_quality.linked_source_rows * 100 < catalog_quality.source_rows.max(1) * 80 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-MISSING-SOURCE-LINKS",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Nur {} von {} Quellenzeilen enthalten einen erkennbaren URL-/DOI-/Link-Wert.",
                    catalog_quality.linked_source_rows,
                    catalog_quality.source_rows
                ),
                goal: "Fuege im Quellenkatalog und in den Gruppentabellen eine echte Link-/URL-/DOI-Spalte hinzu; jede Zeile muss nachpruefbar sein.".to_string(),
            });
        }
        if catalog_quality.group_count < 6 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-WEAK-GROUPING",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Der Quellenkatalog weist nur {} unterscheidbare Quellengruppen aus; erwartet sind mindestens 6.",
                    catalog_quality.group_count
                ),
                goal: "Gruppiere die Quellen nach fachlichen Quellenfamilien (z.B. regulation, DoD/NATO, NASA/DTIC/reports, standards, academic, datasets/repositories, OEM/industry, patents) und fuehre jede Gruppe tabellarisch aus.".to_string(),
            });
        }
        if catalog_quality.group_table_count < 4 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-MISSING-GROUP-TABLES",
                instance_id: Some("doc_source_review__source_review_group_synthesis".to_string()),
                reason: format!(
                    "Es wurden nur {} gruppenspezifische Quellentabellen erkannt; erwartet sind mindestens 4.",
                    catalog_quality.group_table_count
                ),
                goal: "Erzeuge zusaetzlich zum Master-Katalog mehrere native Gruppentabellen mit vollstaendigen Quellenzeilen, Links und kurzer Datennutzen-Bewertung pro Quelle.".to_string(),
            });
        }
        if catalog_quality.scored_source_rows * 100 < catalog_quality.source_rows.max(1) * 90 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-MISSING-SOURCE-SCORES",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Nur {} von {} Quellenzeilen enthalten eine erkennbare Score-/Rating-Bewertung.",
                    catalog_quality.scored_source_rows,
                    catalog_quality.source_rows
                ),
                goal: "Fuege im Quellenkatalog und in den Gruppentabellen eine Score-/Grade-/Rating-Spalte hinzu und bewerte jede Quelle nach Datennutzen, Direktheit, Verifizierbarkeit und Zugriff.".to_string(),
            });
        }
        if catalog_quality.score_values.len() < 3 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-UNINFORMATIVE-SCORING",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Das Scoring unterscheidet nur {} Score-Stufen; erwartet sind mindestens 3.",
                    catalog_quality.score_values.len()
                ),
                goal: "Nutze ein differenziertes Scoring (z.B. A-D oder 0-5) mit kurzen Begruendungen, statt alle Quellen gleich zu markieren.".to_string(),
            });
        }
        if !catalog_quality.has_scoring_model_table {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-MISSING-SCORING-MODEL",
                instance_id: Some("doc_source_review__source_review_taxonomy".to_string()),
                reason: "Der Source Review enthaelt kein sichtbares Scoring-Modell mit Kriterien/Legende.".to_string(),
                goal: "Ergaenze eine native Tabelle 'Scoring model' mit Kriterien, Gewichtung/Skala und Bedeutung der Score-Stufen; verwende dieses Modell konsistent im Quellenkatalog.".to_string(),
            });
        }

        if let Some(block) = search_method_block {
            let lower = block.markdown.to_lowercase();
            let has_search_terms = lower.contains("suchbegriff")
                || lower.contains("query")
                || lower.contains("search term");
            let has_source_paths = [
                "web",
                "google",
                "scholar",
                "standard",
                "regulier",
                "behörde",
                "behoerde",
                "dataset",
                "repository",
                "patent",
            ]
            .iter()
            .filter(|needle| lower.contains(**needle))
            .count()
                >= 3;
            if !has_search_terms || !has_source_paths {
                issues.push(Issue {
                    lint_id: "SOURCE-REVIEW-WEAK-SEARCH-METHOD",
                    instance_id: Some(block.instance_id.clone()),
                    reason: "Die Suchstrategie dokumentiert Suchbegriffe und Suchpfade nicht ausreichend.".to_string(),
                    goal: "Ergaenze Suchbegriffe, Synonyme, Plattformen/Datenbanken und Einschluss-/Ausschlusslogik; decke Web, Literatur, Standards/Regulatorik und Daten-/Industriequellen ab.".to_string(),
                });
            }
            let documented_source_paths = count_source_review_search_paths(&lower);
            if documented_source_paths < 6 {
                issues.push(Issue {
                    lint_id: "SOURCE-REVIEW-INSUFFICIENT-SOURCE-PATHS",
                    instance_id: Some(block.instance_id.clone()),
                    reason: format!(
                        "Die Suchstrategie deckt nur {documented_source_paths} unterscheidbare Suchpfade ab; erwartet sind mindestens 6."
                    ),
                    goal: "Ergaenze Suchpfade fuer Web, wissenschaftliche Indizes, Behoerden/Regulatorik, Standards, technische Reports, Datensaetze/Repositories, Hersteller/Industrie und ggf. Patente.".to_string(),
                });
            }
        }

        let screened_candidates = source_review_screened_candidate_total(&manuscript);
        let relevant_sources_claimed = source_review_relevant_source_total(&manuscript);
        if screened_candidates == 0 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-MISSING-SCREENING-LEDGER",
                instance_id: Some("doc_source_review__source_review_search_method".to_string()),
                reason: "Der Source Review dokumentiert keinen maschinenlesbaren Screening-Umfang mit Treffer-/Kandidatenzahlen.".to_string(),
                goal: "Fuege eine echte Suchprotokoll-/Screening-Tabelle hinzu: Suchpfad, Query/Suchbegriff, Treffer/Kandidaten gesichtet, Ausgeschlossen, Nutzbar/zitiert, Begruendung.".to_string(),
            });
        } else if screened_candidates < min_screened_candidates {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-LOW-SCREENED-CANDIDATES",
                instance_id: Some("doc_source_review__source_review_search_method".to_string()),
                reason: format!(
                    "Der dokumentierte Screening-Umfang liegt bei {screened_candidates} Kandidaten; erwartet fuer {} sind mindestens {min_screened_candidates}.",
                    metadata.depth_profile_id
                ),
                goal: "Erweitere die Recherche so, dass das Suchprotokoll einen breiten Kandidatenpool aus Web, Behoerden, Standards, Reports, Datensaetzen, Repositories, Wissenschaft und Industrie dokumentiert; zitiere nur die nutzbaren Quellen.".to_string(),
            });
        }
        if screened_candidates > 0
            && committed_search_method_block
                .map(|block| block.used_research_ids.is_empty())
                .unwrap_or(true)
        {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-SCREENING-NOT-PROVENANCED",
                instance_id: Some("doc_source_review__source_review_search_method".to_string()),
                reason: format!(
                    "Das Suchprotokoll behauptet {screened_candidates} gesichtete Kandidaten, ist aber mit keinem Research-Log verknuepft."
                ),
                goal: "Erzeuge die Treffer-/Screening-Zahlen aus persistierten Recherche-Artefakten und committe den Suchmethodenblock mit used_research_ids; geschaetzte oder frei formulierte Trefferzahlen sind nicht releasefaehig.".to_string(),
            });
        }
        if screened_candidates > 0
            && persisted_research_sources * 100 < screened_candidates as i64 * 90
        {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-SCREENING-COUNT-NOT-BACKED",
                instance_id: Some("doc_source_review__source_review_search_method".to_string()),
                reason: format!(
                    "Das Suchprotokoll behauptet {screened_candidates} gesichtete Kandidaten, aber persistierte Research-Logs decken nur {persisted_research_sources} Quellen/Treffer ab."
                ),
                goal: "Persistiere jeden Recherchepfad mit `ctox report research-log-add --sources-count ...`; die Summe der Research-Logs muss den behaupteten Screening-Umfang belegen.".to_string(),
            });
        }
        if screened_candidates > 0 && research_focuses.len() < 6 {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-INSUFFICIENT-RESEARCH-LOG-PATHS",
                instance_id: Some("doc_source_review__source_review_search_method".to_string()),
                reason: format!(
                    "Es sind nur {} unterscheidbare Research-Log-Foki persistiert; ein Source Review braucht mehrere unabhaengige Suchpfade.",
                    research_focuses.len()
                ),
                goal: "Persistiere getrennte Research-Logs fuer Web, scholarly, agency/regulation, standards, reports/repositories, datasets, OEM/industry, patents und Snowballing, soweit fuer den Scope relevant.".to_string(),
            });
        }
        if screened_candidates > 0 && !has_snowballing_log {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-MISSING-CITATION-SNOWBALLING",
                instance_id: Some("doc_source_review__source_review_search_method".to_string()),
                reason: "Es ist kein Research-Log fuer Citation-Snowballing bzw. Referenz-/Cited-by-Nachverfolgung persistiert.".to_string(),
                goal: "Fuehre nach der Erstsuche eine Snowballing-Runde aus: relevante Paper/Reports auf Referenzen, Cited-by-Metadaten, Autoren, Datensaetze, Reportnummern und Standards pruefen und als eigenen Research-Log persistieren.".to_string(),
            });
        }
        if relevant_sources_claimed > 0
            && catalog_quality.source_rows * 100 < relevant_sources_claimed * 70
        {
            issues.push(Issue {
                lint_id: "SOURCE-REVIEW-CATALOG-DOES-NOT-MATCH-SCREENING",
                instance_id: Some("doc_source_review__source_review_catalog".to_string()),
                reason: format!(
                    "Das Suchprotokoll behauptet {} relevante/eingeschlossene Quellen, der sichtbare Katalog zeigt aber nur {} Quellenzeilen.",
                    relevant_sources_claimed,
                    catalog_quality.source_rows
                ),
                goal: "Mache die Uebergabe vom Suchprotokoll zum Katalog konsistent: Entweder relevante Treffer konservativer zaehlen oder die relevanten Quellen gruppiert mit Links im Katalog ausweisen.".to_string(),
            });
        }

        if let Some(block) = coverage_block {
            let lower = block.markdown.to_lowercase();
            if !lower.contains("quellenabdeckung") && !lower.contains("coverage") {
                issues.push(Issue {
                    lint_id: "SOURCE-REVIEW-MISSING-COVERAGE-LINE",
                    instance_id: Some(block.instance_id.clone()),
                    reason: "Der Coverage/Gaps-Block enthaelt keine Quellenabdeckungs-Linie.".to_string(),
                    goal: "Fuege eine Schlusslinie im Muster 'Quellenabdeckung (qualitativ): <level>; Hauptluecken: <gaps>.' hinzu.".to_string(),
                });
            }
            let has_gap_language = lower.contains("lücke")
                || lower.contains("luecke")
                || lower.contains("gap")
                || lower.contains("nicht auffindbar")
                || lower.contains("nicht frei zugänglich")
                || lower.contains("nicht frei zugaenglich");
            if !has_gap_language {
                issues.push(Issue {
                    lint_id: "SOURCE-REVIEW-NO-GAP-ASSESSMENT",
                    instance_id: Some(block.instance_id.clone()),
                    reason: "Der Coverage/Gaps-Block benennt keine verbleibenden Quellen- oder Datenluecken.".to_string(),
                    goal: "Bewerte explizit nicht gefundene, nicht zugaengliche, uneindeutige oder nur indirekt belegte Quellen-/Datenbereiche.".to_string(),
                });
            }
        }
    }

    issues.sort_by(|a, b| {
        a.lint_id
            .cmp(b.lint_id)
            .then_with(|| a.instance_id.cmp(&b.instance_id))
            .then_with(|| a.reason.cmp(&b.reason))
    });

    let ready_to_finish = issues.is_empty();
    let needs_revision = !ready_to_finish;
    let candidate = dedupe_keep_order(
        issues
            .iter()
            .filter_map(|i| i.instance_id.clone())
            .collect::<Vec<_>>(),
    );
    let goals = dedupe_keep_order(issues.iter().map(|i| i.goal.clone()).collect::<Vec<_>>());
    let reasons = dedupe_keep_order(issues.iter().map(|i| i.reason.clone()).collect::<Vec<_>>());
    let summary = if issues.is_empty() {
        "Deliverable-Qualitaet OK: keine sichtbaren Arbeitsartefakte, Tabellen/Figuren ausreichend."
            .to_string()
    } else {
        format!(
            "{} harte Deliverable-Qualitaetsprobleme erkannt.",
            issues.len()
        )
    };

    let issues_payload: Vec<Value> = issues
        .iter()
        .map(|i| {
            json!({
                "lint_id": i.lint_id,
                "severity": "hard",
                "instance_ids": i.instance_id.iter().cloned().collect::<Vec<_>>(),
                "reason": i.reason,
                "goal": i.goal,
            })
        })
        .collect();

    let project_specificity_payload = project_specificity_metrics.as_ref().map(|metrics| {
        json!({
            "candidate_anchor_count": metrics.candidate_anchor_count,
            "visible_anchor_count": metrics.visible_anchor_count,
            "evidence_entries_with_candidate_anchors": metrics.evidence_entries_with_candidate_anchors,
            "visible_evidence_entries": metrics.visible_evidence_entries,
            "sample_missing_anchors": metrics.sample_missing_anchors,
            "sample_visible_anchors": metrics.sample_visible_anchors,
        })
    });

    let payload = json!({
        "summary": summary,
        "check_applicable": true,
        "ready_to_finish": ready_to_finish,
        "needs_revision": needs_revision,
        "candidate_instance_ids": candidate,
        "goals": goals,
        "reasons": reasons,
        "issues": issues_payload,
        "metrics": {
            "figures": manuscript.structured_figures.len(),
            "structured_tables": manuscript.structured_tables.len(),
            "references": manuscript.references.len(),
            "persisted_evidence": evidence_register.len(),
            "research_log_entries": research_log_entries.len(),
            "research_log_sources": research_log_entries.iter().map(|entry| entry.sources_count.max(0)).sum::<i64>(),
            "documents": manuscript.docs.len(),
        },
        "project_specificity": project_specificity_payload,
    });

    Ok(CheckOutcome {
        check_kind: CHECK_KIND.to_string(),
        summary,
        check_applicable: true,
        ready_to_finish,
        needs_revision,
        candidate_instance_ids: candidate,
        goals,
        reasons,
        raw_payload: payload,
    }
    .cap())
}

fn contains_placeholder_text(lower: &str) -> bool {
    [
        "fake",
        "dummy",
        "placeholder",
        "platzhalter",
        "lorem",
        "tbd",
        "todo",
        "test schematic",
        "testgrafik",
        "test figure",
        "fakefig",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[derive(Debug, Clone, Default)]
struct ProjectSpecificityMetrics {
    candidate_anchor_count: usize,
    visible_anchor_count: usize,
    evidence_entries_with_candidate_anchors: usize,
    visible_evidence_entries: usize,
    sample_missing_anchors: Vec<String>,
    sample_visible_anchors: Vec<String>,
}

fn project_specificity_metrics_for(
    manuscript_text: &str,
    raw_topic: &str,
    evidence_register: &[EvidenceEntry],
) -> ProjectSpecificityMetrics {
    let manuscript_norm = normalise_project_anchor_text(manuscript_text);
    let topic_norm = normalise_project_anchor_text(raw_topic);
    let mut all_candidate_anchors: HashSet<String> = HashSet::new();
    let mut visible_anchors: HashSet<String> = HashSet::new();
    let mut sample_missing_anchors: Vec<String> = Vec::new();
    let mut sample_visible_anchors: Vec<String> = Vec::new();
    let mut entries_with_candidate_anchors = 0usize;
    let mut visible_entries = 0usize;

    for entry in evidence_register {
        let anchors = project_fact_anchors_from_evidence(entry, &topic_norm);
        if anchors.is_empty() {
            continue;
        }
        entries_with_candidate_anchors += 1;
        for anchor in &anchors {
            all_candidate_anchors.insert(anchor.normalised.clone());
        }
        let matched: Vec<&ProjectFactAnchor> = anchors
            .iter()
            .filter(|anchor| manuscript_norm.contains(&anchor.normalised))
            .collect();
        if matched.is_empty() {
            for anchor in anchors.iter().take(2) {
                push_sample(&mut sample_missing_anchors, &anchor.original, 10);
            }
        } else {
            visible_entries += 1;
            for anchor in matched {
                visible_anchors.insert(anchor.normalised.clone());
                push_sample(&mut sample_visible_anchors, &anchor.original, 10);
            }
        }
    }

    ProjectSpecificityMetrics {
        candidate_anchor_count: all_candidate_anchors.len(),
        visible_anchor_count: visible_anchors.len(),
        evidence_entries_with_candidate_anchors: entries_with_candidate_anchors,
        visible_evidence_entries: visible_entries,
        sample_missing_anchors,
        sample_visible_anchors,
    }
}

#[derive(Debug, Clone)]
struct ProjectFactAnchor {
    original: String,
    normalised: String,
}

fn project_fact_anchors_from_evidence(
    entry: &EvidenceEntry,
    topic_norm: &str,
) -> Vec<ProjectFactAnchor> {
    let mut metadata_raw = String::new();
    for value in [
        entry.title.as_deref(),
        entry.venue.as_deref(),
        entry.publisher.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        metadata_raw.push_str(value);
        metadata_raw.push('\n');
    }

    let mut body_raw = String::new();
    for value in [entry.snippet_md.as_deref(), entry.abstract_md.as_deref()]
        .into_iter()
        .flatten()
    {
        body_raw.push_str(value);
        body_raw.push('\n');
    }

    let proper_phrase_re =
        Regex::new(r"(?u)\b[A-ZÄÖÜ][\p{L}0-9~_\-]{2,}(?:\s+[A-ZÄÖÜ][\p{L}0-9~_\-]{2,}){0,3}\b")
            .expect("valid regex");
    let number_unit_re =
        Regex::new(r"(?iu)\b\d{1,4}(?:[.,]\d+)?\s*(?:l/h|liter|nutzer|volt|v|w|kw|teur|eur|%)\b")
            .expect("valid regex");

    let mut anchors: Vec<ProjectFactAnchor> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for cap in proper_phrase_re.find_iter(&metadata_raw) {
        maybe_push_project_anchor(cap.as_str(), topic_norm, true, &mut seen, &mut anchors);
    }
    for cap in proper_phrase_re.find_iter(&body_raw) {
        maybe_push_project_anchor(cap.as_str(), topic_norm, false, &mut seen, &mut anchors);
    }
    for cap in number_unit_re.find_iter(&format!("{metadata_raw}\n{body_raw}")) {
        maybe_push_project_anchor(cap.as_str(), topic_norm, true, &mut seen, &mut anchors);
    }

    anchors
}

fn maybe_push_project_anchor(
    raw_anchor: &str,
    topic_norm: &str,
    allow_named_single: bool,
    seen: &mut HashSet<String>,
    anchors: &mut Vec<ProjectFactAnchor>,
) {
    let original = raw_anchor
        .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '~' && ch != '-' && ch != '_')
        .trim();
    if original.chars().count() < 3 {
        return;
    }
    let normalised = normalise_project_anchor_text(original);
    if normalised.chars().count() < 4
        || topic_norm.contains(&normalised)
        || is_generic_project_anchor(original, &normalised, allow_named_single)
        || !seen.insert(normalised.clone())
    {
        return;
    }
    anchors.push(ProjectFactAnchor {
        original: original.to_string(),
        normalised,
    });
}

fn normalise_project_anchor_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_generic_project_anchor(original: &str, normalised: &str, allow_named_single: bool) -> bool {
    let generic = [
        "die",
        "der",
        "das",
        "dies",
        "diese",
        "dieser",
        "dieses",
        "diesen",
        "im",
        "in",
        "am",
        "an",
        "eine",
        "einer",
        "eines",
        "und",
        "oder",
        "fuer",
        "für",
        "mit",
        "ohne",
        "beim",
        "zusätzlich",
        "zusaetzlich",
        "technische",
        "technischen",
        "datenblatt",
        "produktseite",
        "quelle",
        "projektbeschreibung",
        "unternehmen",
        "markt",
        "service",
        "kunden",
        "produkt",
        "produkte",
        "loesung",
        "lösung",
        "loesungen",
        "lösungen",
        "plattform",
        "software",
        "wasserspender",
        "wasser",
        "hygiene",
        "system",
        "systeme",
        "geraet",
        "gerät",
        "geräte",
        "geraete",
        "serie",
        "basis",
        "bereich",
        "kombination",
        "nutzung",
        "installation",
        "wartung",
        "filtration",
        "reduktion",
        "transport",
        "integration",
        "funktion",
        "funktionen",
        "qualität",
        "qualitaet",
        "betrieb",
    ];
    if generic.contains(&normalised) {
        return true;
    }
    let first = normalised.split_whitespace().next().unwrap_or("");
    if generic.contains(&first) {
        return true;
    }
    let tokens: Vec<&str> = normalised.split_whitespace().collect();
    if tokens.iter().all(|token| generic.contains(token)) {
        return true;
    }
    let token_count = tokens.len();
    if token_count == 1 {
        let has_digit = original.chars().any(|ch| ch.is_ascii_digit());
        let is_productish = original
            .chars()
            .any(|ch| ch == '~' || ch == '-' || ch == '_')
            || original
                .chars()
                .filter(|ch| ch.is_ascii_uppercase())
                .count()
                >= 2;
        if has_digit || is_productish {
            return false;
        }
        if allow_named_single && normalised.chars().count() >= 8 {
            return false;
        }
        if !has_digit && !is_productish {
            return true;
        }
    }
    false
}

fn push_sample(out: &mut Vec<String>, value: &str, max: usize) {
    if out.len() >= max || out.iter().any(|existing| existing == value) {
        return;
    }
    out.push(value.to_string());
}

fn contains_unbracketed_evidence_id(text: &str, re: &Regex) -> bool {
    re.find_iter(text).any(|m| {
        let before = text[..m.start()].chars().rev().find(|c| !c.is_whitespace());
        let after = text[m.end()..].chars().find(|c| !c.is_whitespace());
        before != Some('[') || after != Some(']')
    })
}

fn count_source_review_search_paths(lower: &str) -> usize {
    [
        ["web", "google", "suchmaschine", "internet"].as_slice(),
        [
            "scholar", "openalex", "crossref", "scopus", "ieee", "springer", "elsevier", "academic",
        ]
        .as_slice(),
        [
            "behörde", "behoerde", "regulier", "faa", "easa", "nasa", "dtic", "dod",
        ]
        .as_slice(),
        ["standard", "norm", "astm", "iso", "nato", "rtca", "mil-std"].as_slice(),
        [
            "dataset",
            "datensatz",
            "repository",
            "github",
            "zenodo",
            "dataport",
        ]
        .as_slice(),
        [
            "hersteller",
            "manufacturer",
            "datasheet",
            "datenblatt",
            "industrie",
        ]
        .as_slice(),
        ["patent", "espacenet", "google patents"].as_slice(),
        [
            "report",
            "technical report",
            "white paper",
            "handbuch",
            "manual",
        ]
        .as_slice(),
    ]
    .iter()
    .filter(|needles| needles.iter().any(|needle| lower.contains(*needle)))
    .count()
}

fn source_review_screened_candidate_total(
    manuscript: &crate::report::render::manuscript::Manuscript,
) -> i64 {
    let mut total = 0_i64;
    for table in &manuscript.structured_tables {
        total += screened_total_from_headers_and_rows(&table.headers, &table.rows);
    }
    for block in manuscript.docs.iter().flat_map(|doc| doc.blocks.iter()) {
        if let Some(table) = &block.table {
            total += screened_total_from_headers_and_rows(&table.headers, &table.rows);
        }
    }
    total
}

fn source_review_relevant_source_total(
    manuscript: &crate::report::render::manuscript::Manuscript,
) -> i64 {
    let mut total = 0_i64;
    for table in &manuscript.structured_tables {
        total += relevant_total_from_headers_and_rows(&table.headers, &table.rows);
    }
    for block in manuscript.docs.iter().flat_map(|doc| doc.blocks.iter()) {
        if let Some(table) = &block.table {
            total += relevant_total_from_headers_and_rows(&table.headers, &table.rows);
        }
    }
    total
}

fn screened_total_from_headers_and_rows(headers: &[String], rows: &[Vec<String>]) -> i64 {
    let mut candidate_cols: Vec<usize> = Vec::new();
    for (idx, header) in headers.iter().enumerate() {
        let h = header.to_lowercase();
        let is_screening_col = [
            "treffer",
            "hit",
            "kandidat",
            "candidate",
            "gesichtet",
            "screened",
            "suchergebnis",
            "result",
        ]
        .iter()
        .any(|needle| h.contains(needle));
        let is_survivor_col = [
            "nutzbar",
            "cited",
            "zitiert",
            "usable",
            "included",
            "eingeschlossen",
            "ausgeschlossen",
            "excluded",
        ]
        .iter()
        .any(|needle| h.contains(needle));
        if is_screening_col && !is_survivor_col {
            candidate_cols.push(idx);
        }
    }
    if candidate_cols.is_empty() {
        return 0;
    }
    rows.iter()
        .flat_map(|row| {
            candidate_cols
                .iter()
                .filter_map(move |idx| row.get(*idx).map(String::as_str))
        })
        .map(parse_candidate_count)
        .sum()
}

fn relevant_total_from_headers_and_rows(headers: &[String], rows: &[Vec<String>]) -> i64 {
    let mut relevant_cols: Vec<usize> = Vec::new();
    for (idx, header) in headers.iter().enumerate() {
        let h = header.to_lowercase();
        let is_relevant_col = [
            "nutzbar",
            "zitiert",
            "usable",
            "cited",
            "included",
            "relevant",
            "useful",
            "selected",
            "eingeschlossen",
        ]
        .iter()
        .any(|needle| h.contains(needle));
        let is_excluded_col = ["ausgeschlossen", "excluded", "rejected"]
            .iter()
            .any(|needle| h.contains(needle));
        if is_relevant_col && !is_excluded_col {
            relevant_cols.push(idx);
        }
    }
    if relevant_cols.is_empty() {
        return 0;
    }
    rows.iter()
        .flat_map(|row| {
            relevant_cols
                .iter()
                .filter_map(move |idx| row.get(*idx).map(String::as_str))
        })
        .map(parse_candidate_count)
        .sum()
}

fn parse_candidate_count(cell: &str) -> i64 {
    let mut total = 0_i64;
    for token in cell.split(|c: char| !c.is_ascii_digit() && c != '.' && c != ',') {
        let cleaned = token.replace(['.', ','], "");
        if cleaned.is_empty() {
            continue;
        }
        if let Ok(n) = cleaned.parse::<i64>() {
            total += n;
        }
    }
    total
}

#[derive(Debug, Default)]
struct SourceReviewCatalogQuality {
    source_rows: i64,
    linked_source_rows: i64,
    scored_source_rows: i64,
    group_count: usize,
    group_table_count: usize,
    has_scoring_model_table: bool,
    score_values: HashSet<String>,
}

fn source_review_catalog_quality(
    manuscript: &crate::report::render::manuscript::Manuscript,
) -> SourceReviewCatalogQuality {
    let mut quality = SourceReviewCatalogQuality::default();
    let mut groups: HashSet<String> = HashSet::new();

    for table in &manuscript.structured_tables {
        accumulate_source_catalog_quality(
            &mut quality,
            &mut groups,
            table.instance_id.as_deref(),
            &table.caption,
            &table.headers,
            &table.rows,
        );
    }
    for block in manuscript.docs.iter().flat_map(|doc| doc.blocks.iter()) {
        if let Some(table) = &block.table {
            accumulate_source_catalog_quality(
                &mut quality,
                &mut groups,
                Some(block.instance_id.as_str()),
                &block.title,
                &table.headers,
                &table.rows,
            );
        }
    }

    quality.group_count = groups.len();
    quality
}

fn accumulate_source_catalog_quality(
    quality: &mut SourceReviewCatalogQuality,
    groups: &mut HashSet<String>,
    instance_id: Option<&str>,
    caption: &str,
    headers: &[String],
    rows: &[Vec<String>],
) {
    if rows.is_empty() || headers.is_empty() {
        return;
    }
    let caption_lower = caption.to_lowercase();
    let instance_lower = instance_id.unwrap_or("").to_lowercase();
    let headers_lower: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();

    let is_catalog = instance_lower.contains("source_review_catalog")
        || caption_lower.contains("quellenkatalog")
        || caption_lower.contains("source catalog")
        || caption_lower.contains("source list")
        || caption_lower.contains("sources by")
        || caption_lower.contains("sources -")
        || caption_lower.contains("sources:");
    let looks_like_source_table = headers_lower.iter().any(|h| {
        [
            "title",
            "titel",
            "source",
            "quelle",
            "publisher",
            "venue",
            "doi",
            "url",
            "link",
        ]
        .iter()
        .any(|needle| h.contains(needle))
    }) && headers_lower.iter().any(|h| {
        ["url", "doi", "link", "access", "zugriff"]
            .iter()
            .any(|needle| h.contains(needle))
    });
    let is_search_table = caption_lower.contains("search protocol")
        || caption_lower.contains("suchprotokoll")
        || caption_lower.contains("screening")
        || headers_lower
            .iter()
            .any(|h| h.contains("excluded") || h.contains("ausgeschlossen"));
    let has_score_header = headers_lower.iter().any(|h| {
        [
            "score",
            "scoring",
            "grade",
            "rating",
            "bewertung",
            "eignung",
            "quality",
            "confidence",
        ]
        .iter()
        .any(|needle| h.contains(needle))
    });
    let is_scoring_model = caption_lower.contains("scoring model")
        || caption_lower.contains("scoring-modell")
        || caption_lower.contains("bewertungsmodell")
        || (has_score_header
            && headers_lower
                .iter()
                .any(|h| h.contains("criterion") || h.contains("kriter")));
    if is_scoring_model && rows.len() >= 3 {
        quality.has_scoring_model_table = true;
    }
    if is_search_table || !(is_catalog || looks_like_source_table) {
        return;
    }

    quality.source_rows += rows.len() as i64;

    let link_cols: Vec<usize> = headers_lower
        .iter()
        .enumerate()
        .filter_map(|(idx, h)| {
            if ["url", "doi", "link", "access", "zugriff", "identifier"]
                .iter()
                .any(|needle| h.contains(needle))
            {
                Some(idx)
            } else {
                None
            }
        })
        .collect();
    for row in rows {
        let has_link = link_cols.iter().any(|idx| {
            row.get(*idx)
                .map(|cell| {
                    let lower = cell.to_lowercase();
                    lower.contains("http://")
                        || lower.contains("https://")
                        || lower.contains("doi.org/")
                        || lower.contains("10.")
                        || lower.contains("arxiv.org/")
                })
                .unwrap_or(false)
        });
        if has_link {
            quality.linked_source_rows += 1;
        }
    }

    let score_cols: Vec<usize> = headers_lower
        .iter()
        .enumerate()
        .filter_map(|(idx, h)| {
            if [
                "score",
                "grade",
                "rating",
                "bewertung",
                "eignung",
                "quality",
                "confidence",
            ]
            .iter()
            .any(|needle| h.contains(needle))
            {
                Some(idx)
            } else {
                None
            }
        })
        .collect();
    for row in rows {
        let mut row_score: Option<String> = None;
        for idx in &score_cols {
            if let Some(value) = row.get(*idx) {
                let normalised = normalise_score_value(value);
                if !normalised.is_empty() {
                    row_score = Some(normalised);
                    break;
                }
            }
        }
        if let Some(score) = row_score {
            quality.scored_source_rows += 1;
            quality.score_values.insert(score);
        }
    }

    let group_cols: Vec<usize> = headers_lower
        .iter()
        .enumerate()
        .filter_map(|(idx, h)| {
            if [
                "group",
                "gruppe",
                "cluster",
                "category",
                "kategorie",
                "source family",
                "quellenfamilie",
            ]
            .iter()
            .any(|needle| h.contains(needle))
            {
                Some(idx)
            } else {
                None
            }
        })
        .collect();
    for row in rows {
        for idx in &group_cols {
            if let Some(value) = row.get(*idx) {
                let normalised = value.trim().to_lowercase();
                if !normalised.is_empty() && normalised != "other" && normalised != "test" {
                    groups.insert(normalised);
                }
            }
        }
    }
    let caption_says_group = caption_lower.contains("sources by")
        || caption_lower.contains("source group")
        || caption_lower.contains("quellengruppe")
        || caption_lower.contains("quellen nach")
        || caption_lower.contains("group:");
    if caption_says_group && rows.len() >= 3 {
        quality.group_table_count += 1;
    }
}

fn normalise_score_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lower = trimmed.to_lowercase();
    if lower == "registriert"
        || lower == "registered"
        || lower == "source"
        || lower == "quelle"
        || lower == "n/a"
        || lower == "-"
    {
        return String::new();
    }
    if let Some(ch) = trimmed.chars().find(|c| c.is_ascii_alphabetic()) {
        let upper = ch.to_ascii_uppercase();
        if ('A'..='D').contains(&upper) {
            return upper.to_string();
        }
    }
    if let Some(ch) = trimmed.chars().find(|c| c.is_ascii_digit()) {
        return ch.to_string();
    }
    lower
}
