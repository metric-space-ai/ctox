//! Pure-Rust Markdown renderer for `Manuscript v1`. Used as a fast,
//! dependency-free deliverable and as a stable golden-file target for tests.

use anyhow::Result;
use std::collections::HashMap;
use std::fmt::Write;

use crate::report::manuscript::{Block, BulletItem, Citation, Manuscript, Section};

pub fn render(m: &Manuscript) -> Result<Vec<u8>> {
    let mut out = String::new();
    write_title(&mut out, m)?;
    write_scope(&mut out, m)?;
    let cite_index: HashMap<&str, usize> = m
        .citation_register
        .iter()
        .map(|c| (c.evidence_id.as_str(), c.display_index))
        .collect();
    for sec in &m.sections {
        write_section(&mut out, sec, &cite_index, &m.citation_register)?;
    }
    Ok(out.into_bytes())
}

fn write_title(out: &mut String, m: &Manuscript) -> Result<()> {
    writeln!(out, "# {}", m.title)?;
    if let Some(sub) = &m.subtitle {
        writeln!(out, "## {sub}")?;
    }
    writeln!(out, "*{}*", m.version_label)?;
    writeln!(out)?;
    Ok(())
}

fn write_scope(out: &mut String, m: &Manuscript) -> Result<()> {
    writeln!(out, "## Scope and Disclaimer")?;
    writeln!(out)?;
    writeln!(out, "> {}", m.scope.disclaimer_md.replace('\n', "\n> "))?;
    writeln!(out)?;
    if !m.scope.leading_questions.is_empty() {
        writeln!(out, "**Leading questions**\n")?;
        for (idx, q) in m.scope.leading_questions.iter().enumerate() {
            writeln!(out, "{}. {q}", idx + 1)?;
        }
        writeln!(out)?;
    }
    if !m.scope.out_of_scope.is_empty() {
        writeln!(out, "**Out of scope**\n")?;
        for s in &m.scope.out_of_scope {
            writeln!(out, "- {s}")?;
        }
        writeln!(out)?;
    }
    if !m.scope.assumptions.is_empty() {
        writeln!(out, "**Assumptions**\n")?;
        for s in &m.scope.assumptions {
            writeln!(out, "- {s}")?;
        }
        writeln!(out)?;
    }
    Ok(())
}

fn write_section(
    out: &mut String,
    sec: &Section,
    cite_index: &HashMap<&str, usize>,
    register: &[Citation],
) -> Result<()> {
    if !sec.heading.is_empty() {
        let prefix: String = std::iter::repeat('#')
            .take((sec.heading_level + 1).clamp(1, 6) as usize)
            .collect();
        writeln!(out, "{prefix} {}", sec.heading)?;
        writeln!(out)?;
    }
    for block in &sec.blocks {
        write_block(out, block, cite_index, register)?;
    }
    Ok(())
}

fn write_block(
    out: &mut String,
    block: &Block,
    cite_index: &HashMap<&str, usize>,
    register: &[Citation],
) -> Result<()> {
    match block {
        Block::Paragraph {
            text_md,
            evidence_ids,
        } => {
            let cited = format_citation_suffix(evidence_ids, cite_index);
            writeln!(out, "{text_md}{cited}")?;
            writeln!(out)?;
        }
        Block::Bullets { items } => {
            for it in items {
                write_bullet(out, "-", it, cite_index)?;
            }
            writeln!(out)?;
        }
        Block::Numbered { items } => {
            for (idx, it) in items.iter().enumerate() {
                let prefix = format!("{}. ", idx + 1);
                write_bullet(out, &prefix, it, cite_index)?;
            }
            writeln!(out)?;
        }
        Block::OptionsTable { options } => {
            writeln!(out, "| Code | Option | Summary |")?;
            writeln!(out, "|------|--------|---------|")?;
            for o in options {
                writeln!(
                    out,
                    "| {} | {} | {} |",
                    o.code,
                    o.label,
                    o.summary_md.as_deref().unwrap_or("").replace('\n', " ")
                )?;
            }
            writeln!(out)?;
        }
        Block::RequirementsTable { rows } => {
            writeln!(out, "| Code | Title | Must-have | Description |")?;
            writeln!(out, "|------|-------|-----------|-------------|")?;
            for r in rows {
                writeln!(
                    out,
                    "| {} | {} | {} | {} |",
                    r.code,
                    r.title,
                    if r.must_have { "yes" } else { "no" },
                    r.description_md.as_deref().unwrap_or("").replace('\n', " ")
                )?;
            }
            writeln!(out)?;
        }
        Block::MatrixTable {
            matrix_kind: _,
            label,
            axes,
            rows,
        } => {
            writeln!(out, "**{label}**")?;
            writeln!(out)?;
            let mut header = String::from("| Option |");
            let mut sep = String::from("|--------|");
            for axis in axes {
                header.push_str(&format!(" {} |", axis.label));
                sep.push_str("------|");
            }
            writeln!(out, "{header}")?;
            writeln!(out, "{sep}")?;
            for row in rows {
                let mut line = format!("| **{}** ({}) |", row.option_label, row.option_code);
                for axis in axes {
                    let cell = row.cells.iter().find(|c| c.axis_code == axis.code);
                    match cell {
                        Some(c) => {
                            let cite = format_citation_suffix(&c.evidence_ids, cite_index);
                            line.push_str(&format!(" {}{} |", c.value_label, cite));
                        }
                        None => line.push_str(" — |"),
                    }
                }
                writeln!(out, "{line}")?;
            }
            writeln!(out)?;
            // Also write rationale paragraphs so they survive the table.
            for row in rows {
                for cell in &row.cells {
                    if !cell.rationale_md.is_empty() {
                        writeln!(
                            out,
                            "- _{} / {}_: {}{}",
                            row.option_code,
                            cell.axis_code,
                            cell.rationale_md,
                            format_citation_suffix(&cell.evidence_ids, cite_index)
                        )?;
                    }
                }
            }
            writeln!(out)?;
        }
        Block::ScenarioBlock {
            code,
            label,
            description_md,
        } => {
            writeln!(out, "**Scenario {code}: {label}**")?;
            writeln!(out)?;
            writeln!(out, "{description_md}")?;
            writeln!(out)?;
        }
        Block::RiskRegister { rows } => {
            writeln!(out, "| Code | Risk | Likelihood | Impact | Mitigation |")?;
            writeln!(out, "|------|------|------------|--------|------------|")?;
            for r in rows {
                writeln!(
                    out,
                    "| {} | **{}** — {} | {} | {} | {} |",
                    r.code,
                    r.title,
                    r.description_md.replace('\n', " "),
                    r.likelihood.as_deref().unwrap_or("—"),
                    r.impact.as_deref().unwrap_or("—"),
                    r.mitigation_md.replace('\n', " "),
                )?;
            }
            writeln!(out)?;
        }
        Block::CitationRegister => {
            for c in register {
                let authors = if c.authors.is_empty() {
                    "—".to_string()
                } else {
                    c.authors.join("; ")
                };
                let url = c
                    .full_text_url
                    .as_deref()
                    .or(c.landing_url.as_deref())
                    .unwrap_or("");
                writeln!(
                    out,
                    "[{idx}] {authors}. {title}. {venue}{year}. {kind} {canonical} {url}",
                    idx = c.display_index,
                    title = c.title.as_deref().unwrap_or("(untitled)"),
                    venue = c.venue.as_deref().unwrap_or(""),
                    year = c.year.map(|y| format!(" ({y})")).unwrap_or_default(),
                    kind = c.citation_kind,
                    canonical = c.canonical_id,
                )?;
            }
            writeln!(out)?;
        }
        Block::Note { text_md } => {
            if !text_md.is_empty() {
                writeln!(out, "{text_md}")?;
                writeln!(out)?;
            }
        }
    }
    Ok(())
}

fn write_bullet(
    out: &mut String,
    prefix: &str,
    item: &BulletItem,
    cite_index: &HashMap<&str, usize>,
) -> Result<()> {
    let cite = format_citation_suffix(&item.evidence_ids, cite_index);
    let primary = if item.primary_recommendation {
        " **(primary)**"
    } else {
        ""
    };
    let scen = item
        .scenario_code
        .as_deref()
        .map(|c| format!(" _[scenario {c}]_"))
        .unwrap_or_default();
    writeln!(out, "{prefix} {}{cite}{primary}{scen}", item.text_md)?;
    if let Some(asm) = &item.assumption_note_md {
        if !asm.trim().is_empty() {
            writeln!(out, "  _assumption: {asm}_")?;
        }
    }
    Ok(())
}

fn format_citation_suffix(evidence_ids: &[String], cite_index: &HashMap<&str, usize>) -> String {
    if evidence_ids.is_empty() {
        return String::new();
    }
    let nums: Vec<String> = evidence_ids
        .iter()
        .filter_map(|id| cite_index.get(id.as_str()))
        .map(|n| n.to_string())
        .collect();
    if nums.is_empty() {
        String::new()
    } else {
        format!(" [{}]", nums.join(","))
    }
}
