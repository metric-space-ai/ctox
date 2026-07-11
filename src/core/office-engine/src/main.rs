use anyhow::{bail, Context};
use ctox_office_engine::{export, inspect, OfficeKind};
use std::fs;
use std::path::Path;

mod ops;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let operation = args.next().context(
        "operation is required: inspect|export|comments-extract|comments-strip|\
         comments-resolve|comments-add|a11y-audit|a11y-fix|privacy-scrub|redact|\
         tracked-changes-accept|tracked-changes-reject|tracked-changes-replace|\
         protection-set|table-export|fields-report|fields-materialize|\
         style-lint|watermark-audit|watermark-remove|merge-append",
    )?;
    // Batch document operations (Ebene B) take the input package directly.
    match operation.as_str() {
        "comments-extract" => {
            let input = args.next().context("input package path is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&ops::extract_comments(&bytes)?)?
            );
            return Ok(());
        }
        "a11y-audit" => {
            let input = args.next().context("input package path is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&ops::a11y_audit(&bytes)?)?
            );
            return Ok(());
        }
        "privacy-scrub" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            let (scrubbed, report) = ops::privacy_scrub(&bytes)?;
            write_output(&output, &scrubbed)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        "tracked-changes-accept" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            write_output(&output, &ops::accept_tracked_changes(&bytes)?)?;
            return Ok(());
        }
        "protection-set" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            let mode = ops::ProtectionMode::parse(
                &args
                    .next()
                    .context("protection mode is required: readonly|comments|forms|none")?,
            )?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            write_output(&output, &ops::set_protection(&bytes, mode)?)?;
            return Ok(());
        }
        "tracked-changes-reject" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            write_output(&output, &ops::reject_tracked_changes(&bytes)?)?;
            return Ok(());
        }
        "redact" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            let mut terms = Vec::new();
            let mut emails = false;
            let mut phones = false;
            for arg in args {
                match arg.as_str() {
                    "--emails" => emails = true,
                    "--phones" => phones = true,
                    term => terms.push(term.to_string()),
                }
            }
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            let (redacted, report) = ops::redact(&bytes, &terms, emails, phones)?;
            write_output(&output, &redacted)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        "comments-strip" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            write_output(&output, &ops::strip_comments(&bytes)?)?;
            return Ok(());
        }
        "comments-resolve" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            let id = args.next();
            ensure_no_more(args)?;
            let id = match id.as_deref() {
                None | Some("--all") => None,
                Some(value) => Some(value.to_string()),
            };
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            write_output(&output, &ops::resolve_comments(&bytes, id.as_deref())?)?;
            return Ok(());
        }
        "comments-add" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            let anchor = args.next().context("anchor substring is required")?;
            let author = args.next().context("author is required")?;
            let text: Vec<String> = args.collect();
            if text.is_empty() {
                bail!("comment text is required");
            }
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            write_output(
                &output,
                &ops::add_comment(&bytes, &anchor, &author, &text.join(" "))?,
            )?;
            return Ok(());
        }
        "table-export" => {
            let input = args.next().context("input package path is required")?;
            let index = args
                .next()
                .map(|value| {
                    value
                        .parse::<usize>()
                        .context("table index must be a number")
                })
                .transpose()?
                .unwrap_or(0);
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            println!("{}", ops::export_table_csv(&bytes, index)?);
            return Ok(());
        }
        "fields-report" => {
            let input = args.next().context("input package path is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&ops::fields_report(&bytes)?)?
            );
            return Ok(());
        }
        "style-lint" => {
            let input = args.next().context("input package path is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&ops::style_lint(&bytes)?)?
            );
            return Ok(());
        }
        "fields-materialize" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            let prefixes: Vec<String> = args.collect();
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            let (materialized, report) = ops::fields_materialize(&bytes, &prefixes)?;
            write_output(&output, &materialized)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        "watermark-audit" => {
            let input = args.next().context("input package path is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&ops::watermark_audit(&bytes)?)?
            );
            return Ok(());
        }
        "watermark-remove" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            let all = match args.next().as_deref() {
                None => false,
                Some("--all") => true,
                Some(other) => bail!("unexpected argument: {other}"),
            };
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            let (removed, count) = ops::watermark_remove(&bytes, all)?;
            write_output(&output, &removed)?;
            println!("{{\"removed\": {count}}}");
            return Ok(());
        }
        "a11y-fix" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            let mut image_alt = false;
            let mut table_headers = false;
            for arg in args {
                match arg.as_str() {
                    "--image-alt-from-name" => image_alt = true,
                    "--table-headers" => table_headers = true,
                    other => bail!("unexpected argument: {other}"),
                }
            }
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            let (fixed, report) = ops::a11y_fix(&bytes, image_alt, table_headers)?;
            write_output(&output, &fixed)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        "tracked-changes-replace" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            let find = args.next().context("find text is required")?;
            let replace = args.next().context("replacement text is required")?;
            let author = args.next().context("author is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            let (replaced, report) = ops::tracked_replace(&bytes, &find, &replace, &author)?;
            write_output(&output, &replaced)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        "merge-append" => {
            let base = args.next().context("base package path is required")?;
            let appendix = args.next().context("appendix package path is required")?;
            let output = args.next().context("output package path is required")?;
            ensure_no_more(args)?;
            let base_bytes = fs::read(&base).with_context(|| format!("read {base}"))?;
            let appendix_bytes = fs::read(&appendix).with_context(|| format!("read {appendix}"))?;
            write_output(&output, &ops::merge_append(&base_bytes, &appendix_bytes)?)?;
            return Ok(());
        }
        "style-normalize" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            let (normalized, report) = ops::style_normalize(&bytes)?;
            write_output(&output, &normalized)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        "watermark-add" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            let text = args.next().context("watermark text is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            let (marked, added) = ops::watermark_add(&bytes, &text)?;
            write_output(&output, &marked)?;
            println!("{{\"headers_marked\": {added}}}");
            return Ok(());
        }
        "table-import" => {
            let input = args.next().context("input package path is required")?;
            let output = args.next().context("output package path is required")?;
            let csv_path = args.next().context("CSV file path is required")?;
            let header_row = match args.next().as_deref() {
                None => true,
                Some("--no-header") => false,
                Some(other) => bail!("unexpected argument: {other}"),
            };
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            let csv = fs::read_to_string(&csv_path).with_context(|| format!("read {csv_path}"))?;
            write_output(&output, &ops::table_import(&bytes, &csv, header_row)?)?;
            return Ok(());
        }
        _ => {}
    }
    let kind = parse_kind(
        &args
            .next()
            .context("kind is required: document|spreadsheet")?,
    )?;
    match operation.as_str() {
        "inspect" => {
            let input = args.next().context("input package path is required")?;
            ensure_no_more(args)?;
            let bytes = fs::read(&input).with_context(|| format!("read {input}"))?;
            println!("{}", serde_json::to_string_pretty(&inspect(kind, &bytes)?)?);
        }
        "export" => {
            let editor = args.next().context("editor package path is required")?;
            let original = args
                .next()
                .context("original escrow package path is required")?;
            let output = args.next().context("output package path is required")?;
            ensure_no_more(args)?;
            let editor_bytes = fs::read(&editor).with_context(|| format!("read {editor}"))?;
            let original_bytes = fs::read(&original).with_context(|| format!("read {original}"))?;
            let package = export(kind, &editor_bytes, Some(&original_bytes))?;
            if let Some(parent) = Path::new(&output).parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&output, &package.bytes).with_context(|| format!("write {output}"))?;
            println!("{}", serde_json::to_string_pretty(&package)?);
        }
        other => bail!("unsupported operation: {other}"),
    }
    Ok(())
}

fn write_output(output: &str, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(output).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, bytes).with_context(|| format!("write {output}"))
}

fn parse_kind(value: &str) -> anyhow::Result<OfficeKind> {
    match value {
        "document" => Ok(OfficeKind::Document),
        "spreadsheet" => Ok(OfficeKind::Spreadsheet),
        other => bail!("unsupported Office kind: {other}"),
    }
}

fn ensure_no_more(mut args: impl Iterator<Item = String>) -> anyhow::Result<()> {
    if let Some(value) = args.next() {
        bail!("unexpected argument: {value}");
    }
    Ok(())
}
