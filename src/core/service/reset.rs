// Origin: CTOX
// License: AGPL-3.0-only
//
// `ctox reset` — operator recovery for the logging / process-mining audit
// trail. The process-mining layer instruments every runtime table with SQLite
// triggers that record into `ctox_process_events`. That trail is essential for
// auditability, but if the recordings or the triggers themselves get into a bad
// state the instrumentation can amplify a bug — failing the very writes it only
// means to observe. This command gives a stable, scoped way to clear or rebuild
// that trail without touching business data.
//
// Two recovery depths for process-mining:
//   * soft (default) — empty the recorded-data tables, keep schema + triggers.
//   * hard (--hard)  — drop all process-mining triggers and tables, then rebuild
//                      a clean schema (reinstalls fresh triggers, re-seeds rules).
//
// Destructive runs require `--confirm`. Without it the command performs a
// dry-run: it reports exactly what would be deleted and changes nothing.

use crate::service::harness_mining;
use crate::service::process_mining;
use anyhow::Context;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;

const RESET_USAGE: &str = "usage:
  ctox reset process-mining [--hard] [--confirm]
  ctox reset harness-mining [--confirm]
  ctox reset all [--hard] [--confirm]

Without --confirm this is a dry-run: it reports what would be deleted and
changes nothing. --hard drops and rebuilds the process-mining schema and
triggers (recovery from corrupted instrumentation); the default soft reset only
empties the recorded-data tables.";

pub fn handle_reset_command(root: &Path, args: &[String]) -> Result<()> {
    let target = args.first().map(String::as_str);
    let confirm = has_flag(args, "--confirm");
    let hard = has_flag(args, "--hard");

    match target {
        Some("process-mining") => reset_process_mining(root, hard, confirm),
        Some("harness-mining") => {
            if hard {
                anyhow::bail!("--hard is not supported for harness-mining (no instrumentation triggers to rebuild)");
            }
            reset_harness_mining(root, confirm)
        }
        Some("all") => reset_all(root, hard, confirm),
        Some("help") | Some("--help") | Some("-h") | None => {
            println!("{RESET_USAGE}");
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown reset target: {other}\n\n{RESET_USAGE}"),
    }
}

fn open_core_db(root: &Path) -> Result<Connection> {
    let db_path = crate::paths::core_db(root);
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open runtime db {}", db_path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout")?;
    Ok(conn)
}

fn reset_process_mining(root: &Path, hard: bool, confirm: bool) -> Result<()> {
    let mut conn = open_core_db(root)?;
    let counts = process_mining::recorded_data_counts(&conn)?;
    let mode = if hard { "hard" } else { "soft" };

    if !confirm {
        print_plan("process-mining", mode, &counts);
        return Ok(());
    }

    let tx = conn.transaction()?;
    let cleared = if hard {
        process_mining::hard_reset(&tx, &crate::paths::core_db(root))?;
        counts.clone()
    } else {
        process_mining::clear_recorded_data(&tx)?
    };
    tx.commit()?;

    print_result("process-mining", mode, &cleared);
    Ok(())
}

fn reset_harness_mining(root: &Path, confirm: bool) -> Result<()> {
    let mut conn = open_core_db(root)?;
    let counts = harness_mining::recorded_counts(&conn)?;

    if !confirm {
        print_plan("harness-mining", "soft", &counts);
        return Ok(());
    }

    let tx = conn.transaction()?;
    let cleared = harness_mining::clear_recorded(&tx)?;
    tx.commit()?;

    print_result("harness-mining", "soft", &cleared);
    Ok(())
}

fn reset_all(root: &Path, hard: bool, confirm: bool) -> Result<()> {
    let mut conn = open_core_db(root)?;
    let pm_counts = process_mining::recorded_data_counts(&conn)?;
    let hm_counts = harness_mining::recorded_counts(&conn)?;
    let pm_mode = if hard { "hard" } else { "soft" };

    if !confirm {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "confirmed": false,
                "dry_run": true,
                "hint": "re-run with --confirm to delete the rows below",
                "targets": {
                    "process-mining": { "mode": pm_mode, "rows": counts_to_json(&pm_counts) },
                    "harness-mining": { "mode": "soft", "rows": counts_to_json(&hm_counts) },
                }
            }))
            .unwrap_or_default()
        );
        return Ok(());
    }

    let tx = conn.transaction()?;
    let pm_cleared = if hard {
        process_mining::hard_reset(&tx, &crate::paths::core_db(root))?;
        pm_counts.clone()
    } else {
        process_mining::clear_recorded_data(&tx)?
    };
    let hm_cleared = harness_mining::clear_recorded(&tx)?;
    tx.commit()?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "confirmed": true,
            "targets": {
                "process-mining": { "mode": pm_mode, "deleted": counts_to_json(&pm_cleared) },
                "harness-mining": { "mode": "soft", "deleted": counts_to_json(&hm_cleared) },
            }
        }))
        .unwrap_or_default()
    );
    Ok(())
}

fn print_plan(target: &str, mode: &str, counts: &[(String, i64)]) {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "confirmed": false,
            "dry_run": true,
            "target": target,
            "mode": mode,
            "rows": counts_to_json(counts),
            "hint": "re-run with --confirm to delete the rows above"
        }))
        .unwrap_or_default()
    );
}

fn print_result(target: &str, mode: &str, cleared: &[(String, i64)]) {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "confirmed": true,
            "target": target,
            "mode": mode,
            "deleted": counts_to_json(cleared)
        }))
        .unwrap_or_default()
    );
}

fn counts_to_json(counts: &[(String, i64)]) -> Value {
    let map = counts
        .iter()
        .map(|(table, count)| (table.clone(), json!(count)))
        .collect::<serde_json::Map<String, Value>>();
    Value::Object(map)
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}
