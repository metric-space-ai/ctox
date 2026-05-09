//! Manuscript struct + builder.
//!
//! Reads a [`Workspace`] and produces a [`Manuscript`]. The Manuscript
//! is the deterministic intermediate that both renderers (pure-Rust
//! Markdown and Python-helper DOCX) consume. The builder walks the
//! blueprint sequence in order, looks each block up in `committed_blocks`,
//! and copies the markdown verbatim. Required-but-missing blocks are
//! omitted from the manuscript (the manager's completeness check is
//! responsible for catching that case before render time).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};

use crate::report::asset_pack::{
    AssetPack, BlockLibraryEntry, BlueprintSequenceEntry, DocumentBaseDoc, OptionalModule,
};
use crate::report::workspace::{BlockRecord, EvidenceEntry, FigureRow, TableRow, Workspace};

/// Top-level manuscript shape consumed by the renderers. All fields are
/// derived from the workspace + asset pack at render time; nothing is
/// invented here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manuscript {
    pub manifest: ManuscriptManifest,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    pub version_line: String,
    #[serde(default)]
    pub context_line: Option<String>,
    pub scope_disclaimer: String,
    #[serde(default)]
    pub abbreviations: Vec<AbbreviationRow>,
    pub docs: Vec<ManuscriptDoc>,
    #[serde(default)]
    pub references: Vec<ReferenceEntry>,
    #[serde(default)]
    pub figures: Vec<FigurePlaceholder>,
    /// Figures registered via `ctox report figure-add`. Resolved
    /// against `{{fig:<figure_id>}}` tokens in block markdown by both
    /// renderers; auto-numbered in document order.
    #[serde(default)]
    pub structured_figures: Vec<StructuredFigure>,
    /// Tables registered via `ctox report table-add`. Resolved against
    /// `{{tbl:<table_id>}}` tokens; auto-numbered in document order.
    #[serde(default)]
    pub structured_tables: Vec<StructuredTable>,
}

/// Figure registered via `figure-add`. The `fig_number` is assigned at
/// build_manuscript time in document order so cross-refs are stable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredFigure {
    pub figure_id: String,
    pub fig_number: u32,
    pub kind: String,
    pub instance_id: Option<String>,
    pub image_path: String,
    pub caption: String,
    pub source_label: String,
    #[serde(default)]
    pub width_px: Option<i64>,
    #[serde(default)]
    pub height_px: Option<i64>,
}

/// Table registered via `table-add`. Auto-numbered in document order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredTable {
    pub table_id: String,
    pub tbl_number: u32,
    pub kind: String,
    pub instance_id: Option<String>,
    pub caption: String,
    #[serde(default)]
    pub legend: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Metadata block surfaced as YAML-frontmatter / DOCX manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManuscriptManifest {
    pub run_id: String,
    pub report_type_id: String,
    pub report_type_label: String,
    pub domain_profile_label: String,
    pub language: String,
    pub rendered_at: String,
    pub version_label: String,
}

/// One document inside the manuscript. A run may carry one or more
/// docs (the blueprint's `base_docs[]`); the renderer joins them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManuscriptDoc {
    pub doc_id: String,
    pub title: String,
    pub blocks: Vec<ManuscriptBlock>,
}

/// One block inside a [`ManuscriptDoc`]. Carries both the markdown body
/// and, for tabular kinds, a typed table extracted from the markdown so
/// the DOCX renderer can lay it out as a Word table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManuscriptBlock {
    pub instance_id: String,
    pub block_id: String,
    pub title: String,
    pub ord: i64,
    pub level: u8,
    pub kind: ManuscriptBlockKind,
    pub markdown: String,
    #[serde(default)]
    pub table: Option<ManuscriptTable>,
}

/// Block kind classification used by the renderer to pick a layout
/// strategy. Inferred from the block_id pattern; see [`classify_block`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ManuscriptBlockKind {
    Narrative,
    Matrix,
    ScenarioGrid,
    RiskRegister,
    EvidenceRegister,
    AbbreviationTable,
    DefectCatalog,
    CompetitorMatrix,
    CriteriaTable,
}

/// Typed table payload for tabular block kinds. Populated by parsing a
/// GitHub-flavoured Markdown pipe table out of `block.markdown`. Left
/// `None` when the markdown is plain prose; the renderer then falls
/// back to narrative rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManuscriptTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// One entry in the manuscript-level references list. Numbered in the
/// order they first appear in the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceEntry {
    pub ref_n: u32,
    pub evidence_id: String,
    pub kind: String,
    pub authors: String,
    pub year: Option<i32>,
    pub title: String,
    pub venue: String,
    pub url: String,
}

/// One row of an abbreviations table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbbreviationRow {
    pub abk: String,
    pub meaning: String,
}

/// Placeholder for a figure; figures are surfaced as caption-only
/// anchors in this wave (image rendering ships later).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FigurePlaceholder {
    pub instance_id_anchor: String,
    pub caption: String,
    #[serde(default)]
    pub image_path: Option<String>,
}

/// Build a [`Manuscript`] from a loaded workspace. The function is
/// deterministic given a fixed workspace + asset-pack pair; it never
/// invents content, never calls out, and never mutates the workspace.
pub fn build_manuscript(workspace: &Workspace<'_>) -> Result<Manuscript> {
    let metadata = workspace.run_metadata()?;
    let asset_pack = AssetPack::load()?;
    let report_type = asset_pack.report_type(&metadata.report_type_id)?;
    let blueprint = asset_pack.document_blueprint(&report_type.document_blueprint_id)?;

    let domain_label = asset_pack
        .domain_profile(&metadata.domain_profile_id)
        .map(|p| p.label.clone())
        .unwrap_or_default();

    let active_modules: std::collections::HashSet<&str> = report_type
        .default_modules
        .iter()
        .map(String::as_str)
        .collect();
    let modules: HashMap<String, &OptionalModule> = asset_pack
        .optional_modules
        .iter()
        .map(|m| (m.id.clone(), m))
        .collect();

    let committed = workspace.committed_blocks()?;
    let committed_by_instance: HashMap<String, &BlockRecord> = committed
        .iter()
        .map(|b| (b.instance_id.clone(), b))
        .collect();

    let evidence_register = workspace.evidence_register()?;
    let evidence_by_id: HashMap<String, &EvidenceEntry> = evidence_register
        .iter()
        .map(|e| (e.evidence_id.clone(), e))
        .collect();

    let now = chrono::Utc::now();
    let rendered_at = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let stamp_date = now.format("%Y-%m-%d").to_string();
    let language_short = short_language_tag(&metadata.language);
    let version_line = if language_short == "de" {
        format!("Stand: {stamp_date}")
    } else {
        format!("Report date: {stamp_date}")
    };

    // package_summary may carry an operator-supplied context_line.
    let context_line = workspace.run_metadata().ok().and_then(|_| {
        // Round-trip the run row to fish package_summary out via
        // the snapshot value the workspace already exposes.
        workspace.workspace_snapshot().ok().and_then(|snap| {
            snap.get("package_summary")
                .and_then(|v| v.get("context_line"))
                .and_then(|v| v.as_str().map(str::to_string))
        })
    });

    let title = build_title(
        &metadata.report_type_id,
        &report_type.label,
        &metadata.raw_topic,
    );
    let subtitle = if metadata.report_type_id == "project_description" {
        None
    } else if !report_type.purpose.is_empty() {
        Some(report_type.purpose.clone())
    } else {
        None
    };

    let scope_disclaimer =
        build_scope_disclaimer(&language_short, &report_type.id, &report_type.label);

    // Walk the blueprint sequence in declaration order, grouped by doc.
    let mut docs: Vec<ManuscriptDoc> = Vec::with_capacity(blueprint.base_docs.len());
    let mut used_reference_ids_in_order: Vec<String> = Vec::new();
    let mut seen_reference_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut abbreviations: Vec<AbbreviationRow> = Vec::new();

    for base_doc in &blueprint.base_docs {
        let doc_blocks = collect_doc_blocks(
            asset_pack,
            &blueprint.sequence,
            base_doc,
            &report_type.block_library_keys,
            &active_modules,
            &modules,
            &committed_by_instance,
            &mut used_reference_ids_in_order,
            &mut seen_reference_ids,
            &mut abbreviations,
        )?;
        if doc_blocks.is_empty() {
            continue;
        }
        docs.push(ManuscriptDoc {
            doc_id: base_doc.id.clone(),
            title: localized_doc_title(&metadata.report_type_id, &base_doc.title, &language_short),
            blocks: doc_blocks,
        });
    }

    let references = if metadata.report_type_id == "project_description" {
        strip_reference_markers_from_docs(&mut docs, &used_reference_ids_in_order);
        Vec::new()
    } else {
        build_references(&used_reference_ids_in_order, &evidence_by_id)
    };
    let block_instance_ids = collect_block_instance_ids(&docs);

    // Load structured figures + tables from the run and assign
    // deterministic numbers in document (insertion) order.
    let figure_rows = workspace.figures().unwrap_or_default();
    let structured_figures: Vec<StructuredFigure> = figure_rows
        .into_iter()
        .enumerate()
        .map(|(idx, row): (usize, FigureRow)| StructuredFigure {
            figure_id: row.figure_id,
            fig_number: (idx + 1) as u32,
            kind: row.kind,
            instance_id: normalize_attachment_instance_id(row.instance_id, &block_instance_ids),
            image_path: row.image_path,
            caption: row.caption,
            source_label: row.source_label,
            width_px: row.width_px,
            height_px: row.height_px,
        })
        .collect();
    let table_rows = workspace.tables().unwrap_or_default();
    let structured_tables: Vec<StructuredTable> = table_rows
        .into_iter()
        .enumerate()
        .map(|(idx, row): (usize, TableRow)| StructuredTable {
            table_id: row.table_id,
            tbl_number: (idx + 1) as u32,
            kind: row.kind,
            instance_id: normalize_attachment_instance_id(row.instance_id, &block_instance_ids),
            caption: row.caption,
            legend: row.legend,
            headers: row.headers,
            rows: row.rows,
        })
        .collect();

    Ok(Manuscript {
        manifest: ManuscriptManifest {
            run_id: metadata.run_id.clone(),
            report_type_id: metadata.report_type_id.clone(),
            report_type_label: report_type.label.clone(),
            domain_profile_label: domain_label,
            language: metadata.language.clone(),
            rendered_at,
            version_label: "report".to_string(),
        },
        title,
        subtitle,
        version_line,
        context_line,
        scope_disclaimer,
        abbreviations,
        docs,
        references,
        figures: Vec::new(),
        structured_figures,
        structured_tables,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_doc_blocks(
    pack: &AssetPack,
    sequence: &[BlueprintSequenceEntry],
    base_doc: &DocumentBaseDoc,
    block_library_keys: &[String],
    active_modules: &std::collections::HashSet<&str>,
    modules: &HashMap<String, &OptionalModule>,
    committed_by_instance: &HashMap<String, &BlockRecord>,
    used_reference_ids_in_order: &mut Vec<String>,
    seen_reference_ids: &mut std::collections::HashSet<String>,
    abbreviations: &mut Vec<AbbreviationRow>,
) -> Result<Vec<ManuscriptBlock>> {
    let allowed: std::collections::HashSet<&str> =
        block_library_keys.iter().map(String::as_str).collect();
    let mut blocks: Vec<ManuscriptBlock> = Vec::new();

    let mut entries: Vec<&BlueprintSequenceEntry> = sequence
        .iter()
        .filter(|e| e.doc_id == base_doc.id)
        .collect();
    entries.sort_by_key(|e| e.order);

    for entry in entries {
        if !allowed.contains(entry.block_id.as_str()) {
            // Cross-type slot — silently skipped, validate() at bootstrap
            // would have caught a structurally bad asset pack.
            continue;
        }
        if let Some(module_id) = &entry.module {
            if !active_modules.contains(module_id.as_str()) {
                // Module not active for this run -> slot omitted.
                continue;
            }
            if !modules.contains_key(module_id.as_str()) {
                // Module not declared at all.
                continue;
            }
        }
        let instance_id = format!("{}__{}", entry.doc_id, entry.block_id);
        let library_entry = pack.block_library_entry(&entry.block_id).with_context(|| {
            format!(
                "blueprint references unknown block_library entry {}",
                entry.block_id
            )
        })?;
        let Some(record) = committed_by_instance.get(&instance_id).copied() else {
            // Required+missing is logged elsewhere (manager's completeness
            // check). The manuscript omits the block.
            continue;
        };
        let kind = classify_block(&entry.block_id);
        let table = parse_first_markdown_table(&record.markdown);
        let markdown = if table.is_some() {
            // Strip the consumed table out of the markdown body so the
            // narrative renderer doesn't re-emit it. We keep any prose
            // before/after the table.
            extract_non_table_markdown(&record.markdown)
        } else {
            record.markdown.clone()
        };

        // Track reference usage in document order for the bibliography.
        for ref_id in &record.used_reference_ids {
            if seen_reference_ids.insert(ref_id.clone()) {
                used_reference_ids_in_order.push(ref_id.clone());
            }
        }

        // If this is the abbreviations block, populate the manuscript-
        // level abbreviation register from its table.
        if matches!(kind, ManuscriptBlockKind::AbbreviationTable) {
            if let Some(t) = &table {
                for row in &t.rows {
                    if row.len() >= 2 && !row[0].trim().is_empty() {
                        abbreviations.push(AbbreviationRow {
                            abk: row[0].trim().to_string(),
                            meaning: row[1].trim().to_string(),
                        });
                    }
                }
            }
        }

        blocks.push(ManuscriptBlock {
            instance_id: record.instance_id.clone(),
            block_id: record.block_id.clone(),
            title: title_for_block(&library_entry, record),
            ord: record.ord,
            level: 2,
            kind,
            markdown,
            table,
        });
    }
    Ok(blocks)
}

fn collect_block_instance_ids(docs: &[ManuscriptDoc]) -> HashSet<String> {
    docs.iter()
        .flat_map(|doc| doc.blocks.iter().map(|block| block.instance_id.clone()))
        .collect()
}

fn normalize_attachment_instance_id(
    instance_id: Option<String>,
    block_instance_ids: &HashSet<String>,
) -> Option<String> {
    let Some(raw) = instance_id else {
        return None;
    };
    if block_instance_ids.contains(&raw) {
        return Some(raw);
    }
    let mut matches: Vec<&String> = block_instance_ids
        .iter()
        .filter(|candidate| raw.starts_with(candidate.as_str()))
        .filter(|candidate| {
            raw.as_bytes()
                .get(candidate.len())
                .map(|b| *b == b'_' || *b == b'-' || *b == b'.')
                .unwrap_or(false)
        })
        .collect();
    matches.sort_by_key(|candidate| std::cmp::Reverse(candidate.len()));
    matches.first().map(|candidate| (*candidate).clone())
}

/// Title resolution: the committed block carries the canonical title;
/// fall back to the library entry's title when the committed value is
/// empty.
fn title_for_block(entry: &BlockLibraryEntry, record: &BlockRecord) -> String {
    if !record.title.trim().is_empty() {
        record.title.clone()
    } else {
        entry.title.clone()
    }
}

/// Classify a `block_id` into a [`ManuscriptBlockKind`] using the
/// pattern rules documented in the spec. Patterns are matched in
/// specificity order so that e.g. `competitor_matrix` does not collapse
/// onto the generic `Matrix` arm.
pub fn classify_block(block_id: &str) -> ManuscriptBlockKind {
    let lower = block_id.to_ascii_lowercase();
    if lower.starts_with("competitor_matrix") {
        ManuscriptBlockKind::CompetitorMatrix
    } else if lower.starts_with("scenario_") || lower.contains("_scenario") {
        ManuscriptBlockKind::ScenarioGrid
    } else if lower.starts_with("defect_catalog") {
        ManuscriptBlockKind::DefectCatalog
    } else if lower.starts_with("risk_register") {
        ManuscriptBlockKind::RiskRegister
    } else if lower.starts_with("appendix_sources") || lower.starts_with("evidence_register") {
        ManuscriptBlockKind::EvidenceRegister
    } else if lower.starts_with("abbreviations") || lower == "abkuerzungsverzeichnis" {
        ManuscriptBlockKind::AbbreviationTable
    } else if lower.starts_with("criteria") {
        ManuscriptBlockKind::CriteriaTable
    } else if lower.contains("matrix") || lower.starts_with("screening_matrix") {
        ManuscriptBlockKind::Matrix
    } else {
        ManuscriptBlockKind::Narrative
    }
}

/// Parse the first GitHub-flavoured Markdown pipe table out of a body.
/// Returns `None` when no recognisable table is present.
pub fn parse_first_markdown_table(body: &str) -> Option<ManuscriptTable> {
    let mut header: Option<Vec<String>> = None;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut state: TableState = TableState::Searching;
    for raw_line in body.lines() {
        let line = raw_line.trim();
        match state {
            TableState::Searching => {
                if is_pipe_row(line) {
                    if let Some(cells) = split_pipe_row(line) {
                        header = Some(cells);
                        state = TableState::ExpectSeparator;
                    }
                }
            }
            TableState::ExpectSeparator => {
                if is_separator_row(line) {
                    state = TableState::Body;
                } else {
                    // False alarm: rewind, treat the candidate header as
                    // prose and resume searching from the next line.
                    header = None;
                    state = TableState::Searching;
                }
            }
            TableState::Body => {
                if is_pipe_row(line) {
                    if let Some(cells) = split_pipe_row(line) {
                        rows.push(cells);
                    }
                } else {
                    break;
                }
            }
        }
    }
    let headers = header?;
    if rows.is_empty() {
        return None;
    }
    Some(ManuscriptTable { headers, rows })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableState {
    Searching,
    ExpectSeparator,
    Body,
}

fn is_pipe_row(line: &str) -> bool {
    line.starts_with('|') && line.contains('|') && line.len() >= 3
}

fn is_separator_row(line: &str) -> bool {
    if !is_pipe_row(line) {
        return false;
    }
    let trimmed = line.trim_matches('|');
    trimmed
        .split('|')
        .map(str::trim)
        .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':' || c == ' '))
}

fn split_pipe_row(line: &str) -> Option<Vec<String>> {
    if !is_pipe_row(line) {
        return None;
    }
    let trimmed = line.trim_matches('|');
    let cells: Vec<String> = trimmed.split('|').map(|c| c.trim().to_string()).collect();
    if cells.is_empty() {
        return None;
    }
    Some(cells)
}

/// Strip the first markdown table out of a body, returning everything
/// else (prose before and after the table) joined by a blank line.
fn extract_non_table_markdown(body: &str) -> String {
    let mut before: Vec<&str> = Vec::new();
    let mut after: Vec<&str> = Vec::new();
    let mut state: TableState = TableState::Searching;
    let mut header_line: Option<&str> = None;
    for raw_line in body.lines() {
        let line = raw_line.trim_end();
        match state {
            TableState::Searching => {
                if is_pipe_row(line.trim()) {
                    header_line = Some(raw_line);
                    state = TableState::ExpectSeparator;
                } else {
                    before.push(raw_line);
                }
            }
            TableState::ExpectSeparator => {
                if is_separator_row(line.trim()) {
                    header_line = None;
                    state = TableState::Body;
                } else {
                    if let Some(h) = header_line.take() {
                        before.push(h);
                    }
                    before.push(raw_line);
                    state = TableState::Searching;
                }
            }
            TableState::Body => {
                if is_pipe_row(line.trim()) {
                    // consumed
                } else {
                    after.push(raw_line);
                    state = TableState::Searching;
                }
            }
        }
    }
    let mut out = String::new();
    if !before.is_empty() {
        out.push_str(&before.join("\n"));
    }
    if !after.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&after.join("\n"));
    }
    out.trim().to_string()
}

fn build_references(
    used_in_order: &[String],
    evidence_by_id: &HashMap<String, &EvidenceEntry>,
) -> Vec<ReferenceEntry> {
    let mut out: Vec<ReferenceEntry> = Vec::new();
    for (idx, ref_id) in used_in_order.iter().enumerate() {
        let n = (idx + 1) as u32;
        if let Some(entry) = evidence_by_id.get(ref_id) {
            out.push(ReferenceEntry {
                ref_n: n,
                evidence_id: ref_id.clone(),
                kind: entry.kind.clone(),
                authors: entry.authors.join("; "),
                year: entry.year.map(|y| y as i32),
                title: entry.title.clone().unwrap_or_default(),
                venue: entry.venue.clone().unwrap_or_default(),
                url: entry
                    .url_canonical
                    .clone()
                    .or_else(|| entry.url_full_text.clone())
                    .unwrap_or_default(),
            });
        } else {
            // Unknown reference id: keep the slot but leave the metadata
            // empty so the renderer can still print [n] markers.
            out.push(ReferenceEntry {
                ref_n: n,
                evidence_id: ref_id.clone(),
                kind: String::new(),
                authors: String::new(),
                year: None,
                title: ref_id.clone(),
                venue: String::new(),
                url: String::new(),
            });
        }
    }
    out
}

fn short_language_tag(language: &str) -> String {
    language
        .split(|c| c == '-' || c == '_')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn build_title(report_type_id: &str, report_type_label: &str, raw_topic: &str) -> String {
    let topic = raw_topic.trim();
    if report_type_id == "project_description" {
        if let Some(project_name) = quoted_project_name(topic) {
            return format!("Fördervorhabenbeschreibung: {project_name}");
        }
        let short_topic = topic
            .split(';')
            .next()
            .unwrap_or(topic)
            .trim()
            .chars()
            .take(140)
            .collect::<String>();
        if short_topic.is_empty() {
            return "Projektbeschreibung".to_string();
        }
        return format!("Projektbeschreibung: {short_topic}");
    }
    if topic.is_empty() {
        report_type_label.to_string()
    } else if report_type_label.is_empty() {
        topic.to_string()
    } else {
        format!("{report_type_label}: {topic}")
    }
}

fn quoted_project_name(topic: &str) -> Option<String> {
    let openers = [('\'', '\''), ('„', '“'), ('"', '"')];
    for (open, close) in openers {
        if let Some(start) = topic.find(open) {
            let rest = &topic[start + open.len_utf8()..];
            if let Some(end) = rest.find(close) {
                let value = rest[..end].trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

fn strip_reference_markers_from_docs(docs: &mut [ManuscriptDoc], reference_ids: &[String]) {
    let reference_id_set: HashSet<&str> = reference_ids.iter().map(String::as_str).collect();
    for doc in docs {
        for block in &mut doc.blocks {
            block.markdown = strip_reference_markers(&block.markdown, &reference_id_set);
        }
    }
}

fn strip_reference_markers(text: &str, reference_ids: &HashSet<&str>) -> String {
    static BRACKET_RE: OnceLock<Regex> = OnceLock::new();
    static BARE_EV_RE: OnceLock<Regex> = OnceLock::new();
    static SPACE_BEFORE_PUNCT_RE: OnceLock<Regex> = OnceLock::new();
    static MULTI_SPACE_RE: OnceLock<Regex> = OnceLock::new();

    let bracket_re = BRACKET_RE.get_or_init(|| Regex::new(r"\[([^\]\n]{1,500})\]").unwrap());
    let bare_ev_re = BARE_EV_RE.get_or_init(|| Regex::new(r"(?i)\bev_?[a-f0-9]{8,}\b").unwrap());
    let space_before_punct_re =
        SPACE_BEFORE_PUNCT_RE.get_or_init(|| Regex::new(r"\s+([,.;:])").unwrap());
    let multi_space_re = MULTI_SPACE_RE.get_or_init(|| Regex::new(r"[ \t]{2,}").unwrap());

    let without_brackets = bracket_re.replace_all(text, |caps: &Captures<'_>| {
        let marker = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        if is_reference_marker_group(marker, reference_ids) {
            String::new()
        } else {
            caps.get(0)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default()
        }
    });
    let without_bare = bare_ev_re.replace_all(&without_brackets, "");
    let cleaned = space_before_punct_re.replace_all(&without_bare, "$1");
    multi_space_re.replace_all(&cleaned, " ").to_string()
}

fn is_reference_marker_group(marker: &str, reference_ids: &HashSet<&str>) -> bool {
    let parts: Vec<&str> = marker
        .split(|ch| ch == ',' || ch == ';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    !parts.is_empty()
        && parts
            .iter()
            .all(|part| is_reference_marker(part, reference_ids))
}

fn is_reference_marker(marker: &str, reference_ids: &HashSet<&str>) -> bool {
    static NUMERIC_RE: OnceLock<Regex> = OnceLock::new();
    static EV_RE: OnceLock<Regex> = OnceLock::new();
    static SHORT_REF_RE: OnceLock<Regex> = OnceLock::new();

    let marker = marker.trim();
    reference_ids.contains(marker)
        || NUMERIC_RE
            .get_or_init(|| Regex::new(r"^\d{1,3}$").unwrap())
            .is_match(marker)
        || EV_RE
            .get_or_init(|| Regex::new(r"(?i)^ev_?[a-f0-9]{8,}$").unwrap())
            .is_match(marker)
        || SHORT_REF_RE
            .get_or_init(|| Regex::new(r"(?i)^(e|ref)[_-]?\d{1,4}$").unwrap())
            .is_match(marker)
}

fn build_scope_disclaimer(
    language_short: &str,
    report_type_id: &str,
    report_type_label: &str,
) -> String {
    if report_type_id == "project_description" {
        if language_short == "de" {
            return "Hinweis: Dieses Dokument beschreibt das geplante Vorhaben auf Basis der vorliegenden Unternehmens-, Projekt- und Kontextinformationen.".to_string();
        }
        return "Note: This document describes the planned project on the basis of the available company, project and context information.".to_string();
    }
    if language_short == "de" {
        format!(
            "Hinweis zum Umfang ({report_type_label}): Die Aussagen beruhen auf den im \
             Bericht dokumentierten Quellen, Suchwegen und Abgrenzungen."
        )
    } else {
        format!(
            "Scope note ({report_type_label}): The findings are limited to the \
             documented sources, search paths and exclusions described in this report."
        )
    }
}

fn localized_doc_title(report_type_id: &str, fallback: &str, language_short: &str) -> String {
    if language_short == "de" {
        if report_type_id == "project_description" {
            return String::new();
        }
        return fallback.to_string();
    }
    match report_type_id {
        "source_review" => "Source Review - Main Report".to_string(),
        "project_description" => "Project Description - Main Report".to_string(),
        "feasibility_study" => "Feasibility Study - Main Report".to_string(),
        _ => fallback.to_string(),
    }
}
