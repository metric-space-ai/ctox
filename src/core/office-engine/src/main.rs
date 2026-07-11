use anyhow::{bail, Context};
use ctox_office_engine::{export, inspect, OfficeKind};
use std::fs;
use std::path::Path;

mod ops;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let operation = args.next().context(
        "operation is required: inspect|export|comments-extract|a11y-audit|\
         privacy-scrub|tracked-changes-accept|protection-set",
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
