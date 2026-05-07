//! `ctox report …` command dispatcher.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::json;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::report::blueprints;
use crate::report::check;
use crate::report::claims;
use crate::report::critique;
use crate::report::draft;
use crate::report::evidence;
use crate::report::render;
use crate::report::runs;
use crate::report::scope;
use crate::report::scoring;
use crate::report::state_machine::{self, Status};
use crate::report::store;

const USAGE: &str = "\
ctox report new <preset> --topic \"<text>\" [--language en|de] [--locale <hint>]
ctox report list [--status <state>] [--limit <n>]
ctox report state --run-id <id>
ctox report blueprints
ctox report blueprint show <preset>
ctox report scope --run-id <id> --from-file <path>
ctox report frame --run-id <id> --from-file <path>
ctox report enumerate --run-id <id> --from-file <path>
ctox report scoring define-rubric --run-id <id> --from-file <path>
ctox report scoring set-cell --run-id <id> --from-file <path>
ctox report scenarios add --run-id <id> --from-file <path>
ctox report risks add --run-id <id> --from-file <path>
ctox report claims add --run-id <id> --from-file <path>
ctox report evidence add --run-id <id> --from-file <path>
ctox report evidence import --run-id <id> --query <text> [--focus <text>] [--depth quick|standard|exhaustive] [--max-sources <n>] [--no-resolve]
ctox report draft --run-id <id>
ctox report critique --run-id <id> [--mode self|external] [--from-file <path>] [--version-id <id>]
ctox report revise --run-id <id> --from-file <manuscript.json> [--notes <text>]
ctox report check --run-id <id> [--version-id <id>]
ctox report render --run-id <id> --format md|docx|json [--version-id <id>] [--out <path>] [--force-no-check]
ctox report finalize --run-id <id>
ctox report abort --run-id <id> --reason <text>
ctox report export --run-id <id>";

pub fn handle_report_command(root: &Path, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("new") => cmd_new(root, &args[1..]),
        Some("list") => cmd_list(root, &args[1..]),
        Some("state") => cmd_state(root, &args[1..]),
        Some("blueprints") => print_json(&json!({
            "ok": true,
            "presets": blueprints::list(),
        })),
        Some("blueprint") => match args.get(1).map(String::as_str) {
            Some("show") => {
                let preset = args
                    .get(2)
                    .context("usage: ctox report blueprint show <preset>")?;
                let bp = blueprints::load(preset)?;
                print_json(&json!({"ok": true, "blueprint": bp}))
            }
            _ => bail!("usage: ctox report blueprint show <preset>"),
        },
        Some("scope") => cmd_scope(root, &args[1..]),
        Some("frame") => cmd_frame(root, &args[1..]),
        Some("enumerate") => cmd_enumerate(root, &args[1..]),
        Some("scoring") => match args.get(1).map(String::as_str) {
            Some("define-rubric") => cmd_define_rubric(root, &args[2..]),
            Some("set-cell") => cmd_set_cell(root, &args[2..]),
            _ => bail!("usage: ctox report scoring [define-rubric|set-cell] …"),
        },
        Some("scenarios") => match args.get(1).map(String::as_str) {
            Some("add") => cmd_add_scenario(root, &args[2..]),
            _ => bail!("usage: ctox report scenarios add --run-id <id> --from-file <path>"),
        },
        Some("risks") => match args.get(1).map(String::as_str) {
            Some("add") => cmd_add_risk(root, &args[2..]),
            _ => bail!("usage: ctox report risks add --run-id <id> --from-file <path>"),
        },
        Some("claims") => match args.get(1).map(String::as_str) {
            Some("add") => cmd_add_claim(root, &args[2..]),
            _ => bail!("usage: ctox report claims add --run-id <id> --from-file <path>"),
        },
        Some("evidence") => match args.get(1).map(String::as_str) {
            Some("add") => cmd_add_evidence(root, &args[2..]),
            Some("import") => cmd_import_evidence(root, &args[2..]),
            _ => bail!("usage: ctox report evidence [add|import] …"),
        },
        Some("draft") => cmd_draft(root, &args[1..]),
        Some("critique") => cmd_critique(root, &args[1..]),
        Some("revise") => cmd_revise(root, &args[1..]),
        Some("check") => cmd_check(root, &args[1..]),
        Some("render") => cmd_render(root, &args[1..]),
        Some("finalize") => cmd_finalize(root, &args[1..]),
        Some("abort") => cmd_abort(root, &args[1..]),
        Some("export") => cmd_export(root, &args[1..]),
        Some("help") | Some("--help") | Some("-h") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => bail!("unknown report subcommand '{other}'\n{USAGE}"),
    }
}

fn cmd_new(root: &Path, args: &[String]) -> Result<()> {
    let preset = args
        .first()
        .filter(|s| !s.starts_with("--"))
        .map(String::as_str)
        .context("usage: ctox report new <preset> --topic \"…\"")?;
    let topic = required_flag_value(args, "--topic").context("--topic is required")?;
    let language = find_flag_value(args, "--language").unwrap_or("en");
    let locale = find_flag_value(args, "--locale");
    let blueprint = blueprints::load(preset)?;
    let conn = store::open(root)?;
    let view = runs::create_run(
        &conn,
        &blueprint,
        topic,
        language,
        locale.map(|s| Value::String(s.to_string())).as_ref(),
    )?;
    runs::set_next_stage(&conn, &view.run_id, Some("scope"))?;
    print_json(&json!({"ok": true, "run": view}))
}

fn cmd_list(root: &Path, args: &[String]) -> Result<()> {
    let conn = store::open(root)?;
    let limit: usize = find_flag_value(args, "--limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let status = find_flag_value(args, "--status");
    let runs = runs::list_runs(&conn, status, limit)?;
    print_json(&json!({"ok": true, "count": runs.len(), "runs": runs}))
}

fn cmd_state(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let conn = store::open(root)?;
    print_json(&runs::run_summary(&conn, run_id)?)
}

fn cmd_scope(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let payload = read_json_arg(args, "--from-file")?;
    let input = scope::parse_scope_input_from_json(&payload)?;
    let conn = store::open(root)?;
    let run = runs::load_run(&conn, run_id)?.context("run not found")?;
    let blueprint = blueprints::load(&run.preset)?;
    let view = scope::upsert_scope(&conn, &blueprint, run_id, &input)?;
    print_json(&json!({"ok": true, "scope": scope::scope_payload(&view)}))
}

fn cmd_frame(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let payload = read_json_arg(args, "--from-file")?;
    let conn = store::open(root)?;
    let inputs: Vec<claims::RequirementInput> = serde_json::from_value(payload)
        .context("frame --from-file must contain an array of {code,title,description_md,…}")?;
    if inputs.is_empty() {
        bail!("frame requires at least one requirement");
    }
    let mut out = Vec::new();
    for inp in inputs {
        out.push(claims::upsert_requirement(&conn, run_id, &inp)?);
    }
    print_json(&json!({"ok": true, "requirements": out}))
}

fn cmd_enumerate(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let payload = read_json_arg(args, "--from-file")?;
    let conn = store::open(root)?;
    let inputs: Vec<claims::OptionInput> = serde_json::from_value(payload).context(
        "enumerate --from-file must contain an array of {code,label,summary_md,synonyms[]}",
    )?;
    if inputs.is_empty() {
        bail!("enumerate requires at least one option");
    }
    let mut out = Vec::new();
    for inp in inputs {
        out.push(claims::upsert_option(&conn, run_id, &inp)?);
    }
    print_json(&json!({"ok": true, "options": out}))
}

fn cmd_define_rubric(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let payload = read_json_arg(args, "--from-file")?;
    let conn = store::open(root)?;
    let inputs: Vec<scoring::RubricInput> = serde_json::from_value(payload)
        .context("define-rubric --from-file must contain an array of {axis_code,level_code,level_definition_md,…}")?;
    let mut out = Vec::new();
    for inp in inputs {
        out.push(scoring::upsert_rubric(&conn, run_id, &inp)?);
    }
    print_json(&json!({"ok": true, "rubrics": out}))
}

fn cmd_set_cell(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let payload = read_json_arg(args, "--from-file")?;
    let conn = store::open(root)?;
    let inputs: Vec<scoring::CellInput> = serde_json::from_value(payload)
        .context("set-cell --from-file must contain an array of cells")?;
    let mut out = Vec::new();
    for inp in inputs {
        out.push(scoring::upsert_cell(&conn, run_id, &inp)?);
    }
    print_json(&json!({"ok": true, "cells": out}))
}

fn cmd_add_scenario(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let payload = read_json_arg(args, "--from-file")?;
    let conn = store::open(root)?;
    let inputs: Vec<claims::ScenarioInput> = serde_json::from_value(payload)
        .context("scenarios add --from-file must contain an array of scenarios")?;
    let mut out = Vec::new();
    for inp in inputs {
        out.push(claims::upsert_scenario(&conn, run_id, &inp)?);
    }
    print_json(&json!({"ok": true, "scenarios": out}))
}

fn cmd_add_risk(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let payload = read_json_arg(args, "--from-file")?;
    let conn = store::open(root)?;
    let inputs: Vec<claims::RiskInput> = serde_json::from_value(payload)
        .context("risks add --from-file must contain an array of risks")?;
    let mut out = Vec::new();
    for inp in inputs {
        out.push(claims::upsert_risk(&conn, run_id, &inp)?);
    }
    print_json(&json!({"ok": true, "risks": out}))
}

fn cmd_add_claim(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let payload = read_json_arg(args, "--from-file")?;
    let conn = store::open(root)?;
    let run = runs::load_run(&conn, run_id)?.context("run not found")?;
    let blueprint = blueprints::load(&run.preset)?;
    let inputs: Vec<claims::ClaimInput> = serde_json::from_value(payload)
        .context("claims add --from-file must contain an array of claims")?;
    let mut out = Vec::new();
    for inp in inputs {
        out.push(claims::add_claim(&conn, &blueprint, run_id, &inp)?);
    }
    print_json(&json!({"ok": true, "claims": out}))
}

fn cmd_add_evidence(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let payload = read_json_arg(args, "--from-file")?;
    let conn = store::open(root)?;
    let inputs: Vec<evidence::EvidenceInput> = serde_json::from_value(payload)
        .context("evidence add --from-file must contain an array of evidence rows")?;
    let mut out = Vec::new();
    for inp in inputs {
        out.push(evidence::upsert_evidence(&conn, run_id, &inp)?);
    }
    print_json(&json!({"ok": true, "evidence": out}))
}

fn cmd_import_evidence(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let query = required_flag_value(args, "--query").context("--query is required")?;
    let focus = find_flag_value(args, "--focus");
    let depth = find_flag_value(args, "--depth").unwrap_or("standard");
    let max_sources: usize = find_flag_value(args, "--max-sources")
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);
    let resolve = !args.iter().any(|a| a == "--no-resolve");
    let conn = store::open(root)?;
    let summary = evidence::import_from_deep_research_bundle(
        root,
        &conn,
        run_id,
        query,
        focus,
        depth,
        max_sources,
        resolve,
    )?;
    print_json(&json!({"ok": true, "import": summary}))
}

fn cmd_draft(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let conn = store::open(root)?;
    let run = runs::load_run(&conn, run_id)?.context("run not found")?;
    let blueprint = blueprints::load(&run.preset)?;
    let out = draft::draft_run(&conn, &blueprint, run_id)?;
    print_json(&out.payload())
}

fn cmd_critique(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let mode = find_flag_value(args, "--mode").unwrap_or("self");
    let version_id = find_flag_value(args, "--version-id");
    let conn = store::open(root)?;
    let out = match mode {
        "self" => critique::record_self_critique(&conn, run_id, version_id)?,
        "external" => {
            let payload = read_json_arg(args, "--from-file")?;
            let summary_md = payload
                .get("summary_md")
                .and_then(Value::as_str)
                .unwrap_or("External critique")
                .to_string();
            let findings_value = payload
                .get("findings")
                .cloned()
                .unwrap_or(Value::Array(vec![]));
            let findings: Vec<critique::Finding> = serde_json::from_value(findings_value)
                .context("--from-file must contain {summary_md, findings: [...]}")?;
            critique::record_external_critique(&conn, run_id, version_id, findings, &summary_md)?
        }
        other => bail!("--mode must be self or external (got '{other}')"),
    };
    print_json(&critique::payload(&out))
}

fn cmd_revise(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let payload = read_json_arg(args, "--from-file")?;
    let mut input: critique::ReviseInput = serde_json::from_value(payload)
        .context("revise --from-file must be a {from_version_id?, manuscript, notes_md?}")?;
    if let Some(notes) = find_flag_value(args, "--notes") {
        input.notes_md = Some(notes.to_string());
    }
    let conn = store::open(root)?;
    let out = critique::revise(&conn, run_id, &input)?;
    print_json(&critique::revise_payload(&out))
}

fn cmd_check(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let version_id = find_flag_value(args, "--version-id");
    let conn = store::open(root)?;
    let run = runs::load_run(&conn, run_id)?.context("run not found")?;
    let blueprint = blueprints::load(&run.preset)?;
    let report = check::run_check(&conn, &blueprint, run_id, version_id)?;
    print_json(&json!({
        "ok": true,
        "check": report,
    }))
}

fn cmd_render(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let format = required_flag_value(args, "--format").context("--format is required")?;
    let version_id = find_flag_value(args, "--version-id");
    let out_path = find_flag_value(args, "--out").map(PathBuf::from);
    let force = args.iter().any(|a| a == "--force-no-check");
    let conn = store::open(root)?;
    let out = render::render(
        &conn,
        root,
        run_id,
        version_id,
        format,
        out_path.as_deref(),
        force,
    )?;
    print_json(&render::payload(&out))
}

fn cmd_finalize(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let conn = store::open(root)?;
    state_machine::terminate(&conn, run_id, Status::Finalized)?;
    print_json(&runs::run_summary(&conn, run_id)?)
}

fn cmd_abort(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let reason = required_flag_value(args, "--reason").context("--reason is required")?;
    let conn = store::open(root)?;
    state_machine::terminate(&conn, run_id, Status::Abandoned)?;
    print_json(&json!({"ok": true, "abandoned": run_id, "reason": reason}))
}

fn cmd_export(root: &Path, args: &[String]) -> Result<()> {
    let run_id = required_flag_value(args, "--run-id").context("--run-id is required")?;
    let conn = store::open(root)?;
    let summary = runs::run_summary(&conn, run_id)?;
    let scope = scope::load_scope(&conn, run_id)?;
    let evidence = evidence::list_evidence(&conn, run_id)?;
    let options = claims::list_options(&conn, run_id)?;
    let requirements = claims::list_requirements(&conn, run_id)?;
    let scenarios = claims::list_scenarios(&conn, run_id)?;
    let risks = claims::list_risks(&conn, run_id)?;
    let cells = scoring::list_cells(&conn, run_id)?;
    let rubrics = scoring::list_rubrics(&conn, run_id)?;
    let claim_rows = claims::list_claims(&conn, run_id)?;
    print_json(&json!({
        "ok": true,
        "summary": summary,
        "scope": scope,
        "evidence": evidence,
        "options": options,
        "requirements": requirements,
        "scenarios": scenarios,
        "risks": risks,
        "rubrics": rubrics,
        "matrix_cells": cells,
        "claims": claim_rows,
    }))
}

// ---------- helpers ----------

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let mut iter = args.iter().enumerate();
    while let Some((i, val)) = iter.next() {
        if val == flag {
            return args.get(i + 1).map(String::as_str);
        }
    }
    None
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    required_flag_value(args, flag)
}

fn read_json_arg(args: &[String], flag: &str) -> Result<Value> {
    let path = required_flag_value(args, flag).with_context(|| format!("{flag} is required"))?;
    let bytes = fs::read(path).with_context(|| format!("failed to read {path}"))?;
    let value: Value =
        serde_json::from_slice(&bytes).with_context(|| format!("{path} is not valid JSON"))?;
    Ok(value)
}

// Suppress connection-unused lint when handlers don't need it.
#[allow(dead_code)]
fn touch_conn(_c: &Connection) {}
