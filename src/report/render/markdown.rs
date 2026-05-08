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
    ManuscriptTable, ReferenceEntry,
};

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

    for doc in &manuscript.docs {
        write_doc(&mut out, doc);
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

fn write_doc(out: &mut String, doc: &ManuscriptDoc) {
    let _ = writeln!(out, "## {}", ascii_dashes(&doc.title));
    out.push('\n');
    for block in &doc.blocks {
        write_block(out, block);
    }
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
