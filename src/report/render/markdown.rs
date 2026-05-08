//! Pure-Rust Markdown renderer.
//!
//! Takes a [`Manuscript`] and produces the Markdown string emitted by
//! `ctox report render <run> --format md`. Output uses GitHub-flavoured
//! Markdown pipe tables, ASCII hyphens only (per
//! `style_guidance.numbers_freshness_rules`), and an optional YAML
//! frontmatter block keyed off the manuscript's manifest.

use std::fmt::Write as _;

use crate::report::render::manuscript::{
    AbbreviationRow, Manuscript, ManuscriptBlock, ManuscriptBlockKind, ManuscriptDoc,
    ManuscriptTable, ReferenceEntry, StructuredFigure, StructuredTable,
};
use std::collections::HashMap;

/// Toggleable rendering options. Defaults are conservative: TOC marker
/// off, frontmatter off, numeric citations on.
#[derive(Debug, Clone)]
pub struct MarkdownRenderOptions {
    /// When true, emits an `## Inhaltsverzeichnis` block listing every
    /// doc title and block heading. Mostly used for QA exports; the
    /// DOCX renderer handles its own TOC field.
    pub include_toc_marker: bool,
    /// When true, prepends a `---`-fenced YAML metadata block built
    /// from the manuscript manifest.
    pub include_metadata_block: bool,
    /// Citation style applied to bibliography entries.
    pub citation_style: CitationStyle,
}

impl Default for MarkdownRenderOptions {
    fn default() -> Self {
        MarkdownRenderOptions {
            include_toc_marker: false,
            include_metadata_block: false,
            citation_style: CitationStyle::Numeric,
        }
    }
}

/// Citation style for the bibliography section. Numeric uses `[N]`
/// markers; AuthorYear uses `Authors (Year).` lead-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CitationStyle {
    Numeric,
    AuthorYear,
}

/// Render a manuscript to a Markdown string. Pure function: same input,
/// same output, no I/O.
pub fn render_markdown(manuscript: &Manuscript, opts: &MarkdownRenderOptions) -> String {
    let mut out = String::new();

    if opts.include_metadata_block {
        write_frontmatter(&mut out, manuscript);
    }

    write_title_block(&mut out, manuscript);

    if opts.include_toc_marker {
        write_toc(&mut out, manuscript);
    }

    if !manuscript.abbreviations.is_empty() {
        write_abbreviations(&mut out, &manuscript.abbreviations);
    }

    // Build {{fig:ID}} / {{tbl:ID}} → "Abbildung N" / "Tabelle N" maps
    // and instance_id → figures/tables maps so we can both resolve the
    // inline cross-refs and append the artefacts at the right block.
    let fig_token_map: HashMap<String, u32> = manuscript
        .structured_figures
        .iter()
        .map(|f| (f.figure_id.clone(), f.fig_number))
        .collect();
    let tbl_token_map: HashMap<String, u32> = manuscript
        .structured_tables
        .iter()
        .map(|t| (t.table_id.clone(), t.tbl_number))
        .collect();
    let mut figs_by_instance: HashMap<String, Vec<&StructuredFigure>> = HashMap::new();
    for f in &manuscript.structured_figures {
        if let Some(iid) = f.instance_id.as_deref() {
            figs_by_instance.entry(iid.to_string()).or_default().push(f);
        }
    }
    let mut tbls_by_instance: HashMap<String, Vec<&StructuredTable>> = HashMap::new();
    for t in &manuscript.structured_tables {
        if let Some(iid) = t.instance_id.as_deref() {
            tbls_by_instance.entry(iid.to_string()).or_default().push(t);
        }
    }

    for doc in &manuscript.docs {
        write_doc(
            &mut out,
            doc,
            &fig_token_map,
            &tbl_token_map,
            &figs_by_instance,
            &tbls_by_instance,
        );
    }

    // Orphan figures/tables (no instance_id binding) get appended at
    // the end so they're not silently lost.
    let orphan_figs: Vec<&StructuredFigure> = manuscript
        .structured_figures
        .iter()
        .filter(|f| f.instance_id.is_none())
        .collect();
    if !orphan_figs.is_empty() {
        out.push_str("## Abbildungen\n\n");
        for f in orphan_figs {
            write_figure(&mut out, f);
        }
    }
    let orphan_tbls: Vec<&StructuredTable> = manuscript
        .structured_tables
        .iter()
        .filter(|t| t.instance_id.is_none())
        .collect();
    if !orphan_tbls.is_empty() {
        out.push_str("## Tabellen\n\n");
        for t in orphan_tbls {
            write_structured_table(&mut out, t);
        }
    }

    if !manuscript.references.is_empty() {
        write_bibliography(&mut out, &manuscript.references, opts.citation_style);
    }

    // Trailing newline cleanup so the file ends with exactly one blank
    // line, which both git and most editors prefer.
    while out.ends_with("\n\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn write_frontmatter(out: &mut String, manuscript: &Manuscript) {
    out.push_str("---\n");
    let _ = writeln!(out, "run_id: {}", manuscript.manifest.run_id);
    let _ = writeln!(out, "report_type: {}", manuscript.manifest.report_type_id);
    let _ = writeln!(out, "language: {}", manuscript.manifest.language);
    let _ = writeln!(out, "rendered_at: {}", manuscript.manifest.rendered_at);
    let _ = writeln!(out, "version_label: {}", manuscript.manifest.version_label);
    out.push_str("---\n\n");
}

fn write_title_block(out: &mut String, manuscript: &Manuscript) {
    let _ = writeln!(out, "# {}", ascii_dashes(&manuscript.title));
    if let Some(subtitle) = &manuscript.subtitle {
        if !subtitle.trim().is_empty() {
            let _ = writeln!(out, "## {}", ascii_dashes(subtitle));
        }
    }
    let _ = writeln!(out, "*{}*", ascii_dashes(&manuscript.version_line));
    if let Some(context_line) = &manuscript.context_line {
        if !context_line.trim().is_empty() {
            let _ = writeln!(out, "{}", ascii_dashes(context_line));
        }
    }
    out.push('\n');
    let _ = writeln!(out, "> {}", ascii_dashes(&manuscript.scope_disclaimer));
    out.push('\n');
}

fn write_toc(out: &mut String, manuscript: &Manuscript) {
    out.push_str("## Inhaltsverzeichnis\n");
    for doc in &manuscript.docs {
        let _ = writeln!(out, "- {}", ascii_dashes(&doc.title));
        for block in &doc.blocks {
            let _ = writeln!(out, "  - {}", ascii_dashes(&block.title));
        }
    }
    out.push('\n');
}

fn write_abbreviations(out: &mut String, rows: &[AbbreviationRow]) {
    out.push_str("## Abkuerzungsverzeichnis\n\n");
    out.push_str("| Abkuerzung | Bedeutung |\n");
    out.push_str("| --- | --- |\n");
    for row in rows {
        let _ = writeln!(
            out,
            "| {} | {} |",
            escape_pipe(&ascii_dashes(&row.abk)),
            escape_pipe(&ascii_dashes(&row.meaning)),
        );
    }
    out.push('\n');
}

fn write_doc(
    out: &mut String,
    doc: &ManuscriptDoc,
    fig_tokens: &HashMap<String, u32>,
    tbl_tokens: &HashMap<String, u32>,
    figs_by_instance: &HashMap<String, Vec<&StructuredFigure>>,
    tbls_by_instance: &HashMap<String, Vec<&StructuredTable>>,
) {
    let _ = writeln!(out, "## {}", ascii_dashes(&doc.title));
    out.push('\n');
    for block in &doc.blocks {
        write_block_with_artefacts(
            out,
            block,
            fig_tokens,
            tbl_tokens,
            figs_by_instance,
            tbls_by_instance,
        );
    }
}

fn write_block_with_artefacts(
    out: &mut String,
    block: &ManuscriptBlock,
    fig_tokens: &HashMap<String, u32>,
    tbl_tokens: &HashMap<String, u32>,
    figs_by_instance: &HashMap<String, Vec<&StructuredFigure>>,
    tbls_by_instance: &HashMap<String, Vec<&StructuredTable>>,
) {
    // Resolve the cross-ref tokens before we hand the markdown to the
    // existing block writer.
    let resolved = resolve_xrefs(&block.markdown, fig_tokens, tbl_tokens);
    let block_with_resolved = ManuscriptBlock {
        markdown: resolved,
        ..block.clone()
    };
    write_block(out, &block_with_resolved);

    // Append any figures + tables bound to this instance after the
    // block body, so the reader sees the prose then the artefact.
    if let Some(figs) = figs_by_instance.get(&block.instance_id) {
        for f in figs {
            write_figure(out, f);
        }
    }
    if let Some(tbls) = tbls_by_instance.get(&block.instance_id) {
        for t in tbls {
            write_structured_table(out, t);
        }
    }
}

fn resolve_xrefs(
    markdown: &str,
    fig_tokens: &HashMap<String, u32>,
    tbl_tokens: &HashMap<String, u32>,
) -> String {
    // Replace `{{fig:ID}}` and `{{tbl:ID}}` tokens with "Abbildung N"
    // and "Tabelle N". Unknown tokens are left untouched (a renderer
    // warning at the layer above can flag those at a later wave).
    let mut out = String::with_capacity(markdown.len());
    let mut rest = markdown;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        if let Some(close) = after_open.find("}}") {
            let token = &after_open[..close];
            let rest_after_close = &after_open[close + 2..];
            if let Some(id) = token.strip_prefix("fig:") {
                if let Some(n) = fig_tokens.get(id) {
                    out.push_str(&format!("Abbildung {n}"));
                } else {
                    out.push_str("{{");
                    out.push_str(token);
                    out.push_str("}}");
                }
            } else if let Some(id) = token.strip_prefix("tbl:") {
                if let Some(n) = tbl_tokens.get(id) {
                    out.push_str(&format!("Tabelle {n}"));
                } else {
                    out.push_str("{{");
                    out.push_str(token);
                    out.push_str("}}");
                }
            } else {
                out.push_str("{{");
                out.push_str(token);
                out.push_str("}}");
            }
            rest = rest_after_close;
        } else {
            out.push_str("{{");
            rest = after_open;
        }
    }
    out.push_str(rest);
    out
}

fn write_figure(out: &mut String, f: &StructuredFigure) {
    let _ = writeln!(
        out,
        "![Abbildung {}: {}]({})",
        f.fig_number,
        ascii_dashes(&f.caption),
        f.image_path,
    );
    let _ = writeln!(
        out,
        "*Abbildung {}: {} (Quelle: {})*",
        f.fig_number,
        ascii_dashes(&f.caption),
        ascii_dashes(&f.source_label),
    );
    out.push('\n');
}

fn write_structured_table(out: &mut String, t: &StructuredTable) {
    let _ = writeln!(
        out,
        "*Tabelle {}: {}*",
        t.tbl_number,
        ascii_dashes(&t.caption)
    );
    out.push('\n');
    if !t.headers.is_empty() {
        let _ = write!(out, "|");
        for h in &t.headers {
            let _ = write!(out, " {} |", escape_pipe(&ascii_dashes(h)));
        }
        out.push('\n');
        let _ = write!(out, "|");
        for _ in &t.headers {
            out.push_str(" --- |");
        }
        out.push('\n');
    }
    for row in &t.rows {
        let _ = write!(out, "|");
        for cell in row {
            let _ = write!(out, " {} |", escape_pipe(&ascii_dashes(cell)));
        }
        out.push('\n');
    }
    if let Some(legend) = &t.legend {
        out.push('\n');
        let _ = writeln!(out, "{}", ascii_dashes(legend));
    }
    out.push('\n');
}

fn write_block(out: &mut String, block: &ManuscriptBlock) {
    let level = (block.level as usize).clamp(1, 6);
    let prefix = "#".repeat(level + 1); // doc heading is ##, blocks default to ###
    let _ = writeln!(out, "{prefix} {}", ascii_dashes(&block.title));
    out.push('\n');

    let body = ascii_dashes(&block.markdown);
    let body_trimmed = body.trim();
    if !body_trimmed.is_empty() {
        out.push_str(body_trimmed);
        out.push_str("\n\n");
    }

    if matches!(
        block.kind,
        ManuscriptBlockKind::Matrix
            | ManuscriptBlockKind::ScenarioGrid
            | ManuscriptBlockKind::RiskRegister
            | ManuscriptBlockKind::DefectCatalog
            | ManuscriptBlockKind::CompetitorMatrix
            | ManuscriptBlockKind::CriteriaTable
            | ManuscriptBlockKind::AbbreviationTable
    ) {
        if let Some(table) = &block.table {
            write_table(out, table);
        }
    }
}

fn write_table(out: &mut String, table: &ManuscriptTable) {
    if table.headers.is_empty() {
        return;
    }
    let header_line: String = table
        .headers
        .iter()
        .map(|h| escape_pipe(&ascii_dashes(h)))
        .collect::<Vec<_>>()
        .join(" | ");
    let _ = writeln!(out, "| {header_line} |");
    let sep: String = table
        .headers
        .iter()
        .map(|_| "---".to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    let _ = writeln!(out, "| {sep} |");
    for row in &table.rows {
        let mut cells: Vec<String> = row.iter().map(|c| escape_pipe(&ascii_dashes(c))).collect();
        // Pad short rows so the rendered table stays rectangular.
        while cells.len() < table.headers.len() {
            cells.push(String::new());
        }
        let _ = writeln!(out, "| {} |", cells.join(" | "));
    }
    out.push('\n');
}

fn write_bibliography(out: &mut String, refs: &[ReferenceEntry], style: CitationStyle) {
    out.push_str("## Anhang - Quellen\n\n");
    for entry in refs {
        let line = format_reference(entry, style);
        out.push_str(&line);
        out.push('\n');
    }
    out.push('\n');
}

fn format_reference(entry: &ReferenceEntry, style: CitationStyle) -> String {
    let title = ascii_dashes(&entry.title);
    let venue = ascii_dashes(&entry.venue);
    let authors = ascii_dashes(&entry.authors);
    let year_str = entry
        .year
        .map(|y| y.to_string())
        .unwrap_or_else(String::new);
    let url = ascii_dashes(&entry.url);
    match style {
        CitationStyle::Numeric => {
            let mut parts: Vec<String> = Vec::new();
            parts.push(format!("[{}]", entry.ref_n));
            if !authors.is_empty() {
                if !year_str.is_empty() {
                    parts.push(format!("{authors} ({year_str})."));
                } else {
                    parts.push(format!("{authors}."));
                }
            }
            if !title.is_empty() {
                parts.push(format!("*{title}*."));
            }
            if !venue.is_empty() {
                parts.push(format!("{venue}."));
            }
            if !url.is_empty() {
                parts.push(format!("<{url}>"));
            }
            parts.join(" ")
        }
        CitationStyle::AuthorYear => {
            let mut parts: Vec<String> = Vec::new();
            if !authors.is_empty() {
                if !year_str.is_empty() {
                    parts.push(format!("{authors} ({year_str})."));
                } else {
                    parts.push(format!("{authors}."));
                }
            } else {
                parts.push(format!("[{}]", entry.ref_n));
            }
            if !title.is_empty() {
                parts.push(format!("*{title}*."));
            }
            if !venue.is_empty() {
                parts.push(format!("{venue}."));
            }
            if !url.is_empty() {
                parts.push(format!("<{url}>"));
            }
            parts.join(" ")
        }
    }
}

fn escape_pipe(value: &str) -> String {
    value.replace('|', "\\|")
}

/// Replace Unicode hyphen / dash characters with ASCII equivalents to
/// match `style_guidance: ASCII hyphens only`. Keeps every other Unicode
/// codepoint intact.
fn ascii_dashes(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        let replacement = match ch {
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' => '-',
            '\u{2032}' => '\'',
            '\u{2033}' => '"',
            other => other,
        };
        out.push(replacement);
    }
    out
}
