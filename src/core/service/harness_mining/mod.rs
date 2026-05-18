// Origin: CTOX
// License: Apache-2.0
//
// Harness Mining: forensic and conformance analysis of the autonomous-agent
// harness. Replaces the rust4pm-vendored discovery layer with a tier-based
// suite of algorithms tuned to a *known* declarative state machine
// (core_state_machine.rs) rather than to an unknown business process.
//
// Tier 1 — sofort wertvoll, ohne Spec-Externalisierung:
//   * stuck_cases   — retry-loop / stuck-case detection
//   * variants      — trace variant clustering with frequency + edit distance
//   * sojourn       — per-state holding-time distribution
//   * conformance   — threshold-gated token replay against the declared spec
//
// Tier 2 — diagnostic deep dive:
//   * alignment     — alignment-based conformance (A* synchronous product)
//   * causal        — conditional / causal mining on violation predecessors
//   * drift         — Page-Hinkley / chi-squared concept drift detection
//   * multiperspective — data-aware constraint coverage stats

use anyhow::Context;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub mod alignment;
pub mod brief;
pub mod causal;
pub mod conformance;
pub mod drift;
pub mod findings;
pub mod multiperspective;
pub mod sojourn;
pub mod stuck_cases;
pub mod variants;

const USAGE: &str = "Usage:
  ctox harness-mining brief          [--stuck-min-attempts <n>] [--conformance-threshold <0..1>] [--drift-threshold <f>]
  ctox harness-mining stuck-cases    [--min-attempts <n>] [--idle-seconds <s>] [--limit <n>]
  ctox harness-mining variants       [--entity-type <t>] [--limit <n>] [--cluster]
  ctox harness-mining sojourn        [--entity-type <t>] [--limit <n>]
  ctox harness-mining conformance    [--lane <lane>] [--since <iso8601>] [--window <n>] [--fitness-threshold <0.0..1.0>]
  ctox harness-mining alignment      [--entity-type <t>] [--limit <n>]
  ctox harness-mining causal         [--violation-code <code>] [--lookback <n>] [--limit <n>]
  ctox harness-mining drift          [--window <n>] [--threshold <f>]
  ctox harness-mining multiperspective [--entity-type <t>] [--limit <n>]
  ctox harness-mining audit-tick     [--stuck-min-attempts <n>] [--conformance-threshold <0..1>] [--drift-threshold <f>]
  ctox harness-mining findings       [--status <detected|confirmed|acknowledged|mitigated|verified|stale>] [--kind <k>] [--limit <n>]
  ctox harness-mining finding-ack    --finding-id <id> [--note <text>]
  ctox harness-mining finding-mitigate --finding-id <id> --by <agent|operator|spec-change> [--note <text>]
  ctox harness-mining finding-verify --finding-id <id> [--note <text>]";

pub fn handle_harness_mining_command(root: &Path, args: &[String]) -> Result<()> {
    let db_path = harness_mining_db_path(root);
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open runtime database at {}", db_path.display()))?;

    match args.first().map(String::as_str) {
        Some("brief") => {
            let report = brief::synthesize(&conn, &brief::Options::from_args(args))?;
            print_json(&report)
        }
        Some("audit-tick") => {
            let opts = brief::Options::from_args(args);
            let report = findings::run_audit_tick(&conn, &opts, &now_iso_z())?;
            print_json(&serde_json::json!({
                "ok": true,
                "run_id": report.run_id,
                "recorded": report.recorded,
                "confirmed": report.confirmed,
                "marked_stale": report.stale,
                "brief": report.brief,
            }))
        }
        Some("findings") => {
            findings::ensure_findings_schema(&conn)?;
            let status = parse_string_flag(args, "--status");
            let kind = parse_string_flag(args, "--kind");
            let limit = parse_i64_flag(args, "--limit", 50);
            let rows = findings::list(&conn, status, kind, limit)?;
            print_json(&serde_json::json!({
                "ok": true,
                "findings": rows,
                "count": rows.len(),
            }))
        }
        Some("finding-ack") => {
            findings::ensure_findings_schema(&conn)?;
            let id = parse_string_flag(args, "--finding-id")
                .ok_or_else(|| anyhow::anyhow!("missing --finding-id"))?;
            let note = parse_string_flag(args, "--note");
            findings::acknowledge(&conn, id, note, &now_iso_z())?;
            print_json(&serde_json::json!({"ok": true, "finding_id": id, "status": "acknowledged"}))
        }
        Some("finding-mitigate") => {
            findings::ensure_findings_schema(&conn)?;
            let id = parse_string_flag(args, "--finding-id")
                .ok_or_else(|| anyhow::anyhow!("missing --finding-id"))?;
            let by = parse_string_flag(args, "--by")
                .ok_or_else(|| anyhow::anyhow!("missing --by (agent|operator|spec-change)"))?;
            let note = parse_string_flag(args, "--note");
            findings::mitigate(&conn, id, by, note, &now_iso_z())?;
            print_json(&serde_json::json!({"ok": true, "finding_id": id, "status": "mitigated"}))
        }
        Some("finding-verify") => {
            findings::ensure_findings_schema(&conn)?;
            let id = parse_string_flag(args, "--finding-id")
                .ok_or_else(|| anyhow::anyhow!("missing --finding-id"))?;
            let note = parse_string_flag(args, "--note");
            findings::verify(&conn, id, note, &now_iso_z())?;
            print_json(&serde_json::json!({"ok": true, "finding_id": id, "status": "verified"}))
        }
        Some("stuck-cases") => {
            let report = stuck_cases::detect(&conn, &stuck_cases::Options::from_args(args))?;
            print_json(&report)
        }
        Some("variants") => {
            let report = variants::analyze(&conn, &variants::Options::from_args(args))?;
            print_json(&report)
        }
        Some("sojourn") => {
            let report = sojourn::analyze(&conn, &sojourn::Options::from_args(args))?;
            print_json(&report)
        }
        Some("conformance") => {
            let report = conformance::replay(&conn, &conformance::Options::from_args(args))?;
            let fitness_ok = report
                .get("fitness_ok")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            print_json(&report)?;
            if !fitness_ok {
                anyhow::bail!("harness conformance fitness below threshold");
            }
            return Ok(());
        }
        Some("alignment") => {
            let report = alignment::analyze(&conn, &alignment::Options::from_args(args))?;
            print_json(&report)
        }
        Some("causal") => {
            let report = causal::analyze(&conn, &causal::Options::from_args(args))?;
            print_json(&report)
        }
        Some("drift") => {
            let report = drift::detect(&conn, &drift::Options::from_args(args))?;
            print_json(&report)
        }
        Some("multiperspective") => {
            let report =
                multiperspective::analyze(&conn, &multiperspective::Options::from_args(args))?;
            print_json(&report)
        }
        Some("help") | Some("--help") | Some("-h") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => {
            anyhow::bail!("unknown harness-mining subcommand: {other}\n\n{USAGE}");
        }
    }
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn harness_mining_db_path(root: &Path) -> PathBuf {
    root.join("runtime").join("ctox.sqlite3")
}

pub(crate) fn now_iso_z() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Compact aggregate snapshot for the TUI. Contains only counts, ratios, and
/// boolean flags — never case IDs, never bodies, never hashes. This is the
/// only API the UI is allowed to call into harness_mining: keeps PII out of
/// the rendering layer by construction.
#[derive(Debug, Clone, Default)]
pub struct UiSnapshot {
    pub stuck_case_count: i64,
    pub stuck_top_violation_codes: Vec<String>,
    pub preventive_fitness: f64,
    pub trigger_fitness: f64,
    pub conformance_ok: bool,
    pub drift_detected: bool,
    pub variant_count: i64,
    pub dominant_variant_share: f64,
    pub worst_state_p95_seconds: f64,
    pub samples_known: bool,
    pub error: Option<String>,
}

pub fn ui_snapshot(root: &Path) -> UiSnapshot {
    let db_path = harness_mining_db_path(root);
    let conn =
        match Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(c) => c,
            Err(e) => {
                return UiSnapshot {
                    error: Some(format!("db unavailable: {}", e)),
                    ..Default::default()
                };
            }
        };
    let mut snap = UiSnapshot {
        samples_known: true,
        ..Default::default()
    };
    if let Ok(stuck) = stuck_cases::detect(
        &conn,
        &stuck_cases::Options {
            min_attempts: 5,
            idle_seconds: 0,
            limit: 200,
        },
    ) {
        snap.stuck_case_count = stuck["case_count"].as_i64().unwrap_or(0);
        if let Some(buckets) = stuck["violation_buckets"].as_array() {
            snap.stuck_top_violation_codes = buckets
                .iter()
                .take(3)
                .filter_map(|b| b["top_violation_code"].as_str().map(str::to_string))
                .collect();
        }
    }
    if let Ok(report) = conformance::replay(&conn, &conformance::Options::default()) {
        snap.preventive_fitness = report["preventive"]["fitness"].as_f64().unwrap_or(1.0);
        snap.trigger_fitness = report["trigger"]["fitness"].as_f64().unwrap_or(1.0);
        snap.conformance_ok = report["fitness_ok"].as_bool().unwrap_or(true);
    }
    if let Ok(drift) = drift::detect(&conn, &drift::Options::default()) {
        snap.drift_detected = drift["drift_detected"].as_bool().unwrap_or(false);
    }
    if let Ok(report) = variants::analyze(&conn, &variants::Options::default()) {
        snap.variant_count = report["distinct_variants"].as_i64().unwrap_or(0);
        if let Some(top) = report["variants"].as_array().and_then(|a| a.first()) {
            snap.dominant_variant_share = top["share"].as_f64().unwrap_or(0.0);
        }
    }
    if let Ok(report) = sojourn::analyze(&conn, &sojourn::Options::default()) {
        if let Some(states) = report["states"].as_array() {
            snap.worst_state_p95_seconds = states
                .iter()
                .map(|s| s["p95_seconds"].as_f64().unwrap_or(0.0))
                .fold(0.0, f64::max);
        }
    }
    snap
}

pub(crate) fn parse_i64_flag(args: &[String], name: &str, default: i64) -> i64 {
    find_flag(args, name)
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default)
}

pub(crate) fn parse_f64_flag(args: &[String], name: &str, default: f64) -> f64 {
    find_flag(args, name)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}

pub(crate) fn parse_string_flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    find_flag(args, name)
}

pub(crate) fn flag_present(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn find_flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if a == name {
            return iter.next().map(String::as_str);
        }
        if let Some(rest) = a.strip_prefix(&format!("{name}=")) {
            return Some(rest);
        }
    }
    None
}
