//! CTOX web-unlock registry.
//!
//! Persists known browser-stealth probes and detection vectors in the
//! consolidated runtime SQLite database (`runtime/ctox.sqlite3`). Provides
//! a CLI surface for listing, running baseline probes, recording test
//! runs, and tracking repair attempts.
//!
//! Schema is created idempotently on first use; the seed JSON at
//! `assets/web_unlock_seed.json` is loaded into an empty registry on
//! first run.
//!
//! All tables are namespaced `web_unlock_*` so they coexist with the
//! existing mission / LCM / harness-flow tables.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::browser::{run_browser_automation, BrowserAutomationRequest};

const SEED_JSON: &str = include_str!("../assets/web_unlock_seed.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    pub probe_id: String,
    pub site_name: String,
    pub probe_url: String,
    pub script_path: String,
    pub parser_kind: String,
    pub expected_baseline_json: String,
    pub timeout_ms: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector {
    pub vector_id: String,
    pub probe_id: String,
    pub test_name: String,
    pub description: String,
    pub probe_predicate: Option<String>,
    pub fix_strategy: String,
    pub patch_files_json: String,
    pub status: String,
    pub last_verified_at: Option<String>,
    pub first_introduced_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeOutcome {
    pub probe_id: String,
    pub passed_baseline: bool,
    pub failed_count: usize,
    pub failed_tests: Vec<String>,
    pub duration_ms: u64,
    pub raw_excerpt: Value,
    pub notes: Option<String>,
}

fn core_db_path(root: &Path) -> PathBuf {
    root.join("runtime").join("ctox.sqlite3")
}

pub fn open_db(root: &Path) -> Result<Connection> {
    let path = core_db_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create runtime dir {}", parent.display())
        })?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    ensure_schema(&conn)?;
    seed_if_empty(&conn)?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS web_unlock_probes (
            probe_id TEXT PRIMARY KEY,
            site_name TEXT NOT NULL,
            probe_url TEXT NOT NULL,
            script_path TEXT NOT NULL,
            parser_kind TEXT NOT NULL,
            expected_baseline_json TEXT NOT NULL,
            timeout_ms INTEGER NOT NULL DEFAULT 60000,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS web_unlock_vectors (
            vector_id TEXT PRIMARY KEY,
            probe_id TEXT NOT NULL,
            test_name TEXT NOT NULL,
            description TEXT NOT NULL,
            probe_predicate TEXT,
            fix_strategy TEXT NOT NULL,
            patch_files_json TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'working',
            last_verified_at TEXT,
            first_introduced_commit TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS web_unlock_test_runs (
            run_id INTEGER PRIMARY KEY AUTOINCREMENT,
            probe_id TEXT NOT NULL,
            executed_at TEXT NOT NULL,
            duration_ms INTEGER NOT NULL,
            passed_baseline INTEGER NOT NULL,
            failed_count INTEGER NOT NULL,
            failed_tests_json TEXT,
            result_excerpt_json TEXT,
            notes TEXT
        );

        CREATE TABLE IF NOT EXISTS web_unlock_repairs (
            repair_id INTEGER PRIMARY KEY AUTOINCREMENT,
            triggered_by_run_id INTEGER,
            vector_id TEXT,
            description TEXT NOT NULL,
            patches_applied_json TEXT,
            resulting_commit TEXT,
            succeeded INTEGER,
            notes TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_web_unlock_vectors_probe ON web_unlock_vectors(probe_id);
        CREATE INDEX IF NOT EXISTS idx_web_unlock_vectors_status ON web_unlock_vectors(status);
        CREATE INDEX IF NOT EXISTS idx_web_unlock_test_runs_probe_time ON web_unlock_test_runs(probe_id, executed_at);
        "#,
    )
    .context("failed to create web_unlock_* schema")
}

fn seed_if_empty(conn: &Connection) -> Result<()> {
    let probe_count: i64 = conn
        .query_row("SELECT count(*) FROM web_unlock_probes", [], |r| r.get(0))
        .context("failed to count web_unlock_probes")?;
    if probe_count > 0 {
        return Ok(());
    }
    let seed: Value = serde_json::from_str(SEED_JSON)
        .context("failed to parse embedded web_unlock_seed.json")?;
    let now = Utc::now().to_rfc3339();

    let probes = seed
        .get("probes")
        .and_then(Value::as_array)
        .context("seed.probes missing")?;
    for p in probes {
        conn.execute(
            "INSERT OR REPLACE INTO web_unlock_probes
             (probe_id, site_name, probe_url, script_path, parser_kind,
              expected_baseline_json, timeout_ms, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                p["probe_id"].as_str().unwrap_or_default(),
                p["site_name"].as_str().unwrap_or_default(),
                p["probe_url"].as_str().unwrap_or_default(),
                p["script_path"].as_str().unwrap_or_default(),
                p["parser_kind"].as_str().unwrap_or_default(),
                p["expected_baseline_json"].as_str().unwrap_or_default(),
                p["timeout_ms"].as_u64().unwrap_or(60_000) as i64,
                p["enabled"].as_i64().unwrap_or(1),
                now,
                now,
            ],
        )?;
    }

    let vectors = seed
        .get("vectors")
        .and_then(Value::as_array)
        .context("seed.vectors missing")?;
    for v in vectors {
        let patch_files = v
            .get("patch_files")
            .map(|p| p.to_string())
            .unwrap_or_else(|| "[]".to_string());
        conn.execute(
            "INSERT OR REPLACE INTO web_unlock_vectors
             (vector_id, probe_id, test_name, description, probe_predicate,
              fix_strategy, patch_files_json, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                v["vector_id"].as_str().unwrap_or_default(),
                v["probe_id"].as_str().unwrap_or_default(),
                v["test_name"].as_str().unwrap_or_default(),
                v["description"].as_str().unwrap_or_default(),
                v["probe_predicate"].as_str(),
                v["fix_strategy"].as_str().unwrap_or_default(),
                patch_files,
                "working",
                now,
                now,
            ],
        )?;
    }
    Ok(())
}

pub fn load_probes(conn: &Connection, only_enabled: bool) -> Result<Vec<Probe>> {
    let sql = if only_enabled {
        "SELECT probe_id, site_name, probe_url, script_path, parser_kind,
                expected_baseline_json, timeout_ms, enabled
         FROM web_unlock_probes
         WHERE enabled=1
         ORDER BY probe_id"
    } else {
        "SELECT probe_id, site_name, probe_url, script_path, parser_kind,
                expected_baseline_json, timeout_ms, enabled
         FROM web_unlock_probes
         ORDER BY probe_id"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Probe {
                probe_id: r.get(0)?,
                site_name: r.get(1)?,
                probe_url: r.get(2)?,
                script_path: r.get(3)?,
                parser_kind: r.get(4)?,
                expected_baseline_json: r.get(5)?,
                timeout_ms: r.get::<_, i64>(6)? as u64,
                enabled: r.get::<_, i64>(7)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn load_vectors(conn: &Connection, probe_filter: Option<&str>) -> Result<Vec<Vector>> {
    let sql = if probe_filter.is_some() {
        "SELECT vector_id, probe_id, test_name, description, probe_predicate,
                fix_strategy, patch_files_json, status, last_verified_at, first_introduced_commit
         FROM web_unlock_vectors
         WHERE probe_id = ?1
         ORDER BY vector_id"
    } else {
        "SELECT vector_id, probe_id, test_name, description, probe_predicate,
                fix_strategy, patch_files_json, status, last_verified_at, first_introduced_commit
         FROM web_unlock_vectors
         ORDER BY probe_id, vector_id"
    };
    let mut stmt = conn.prepare(sql)?;
    let mapper = |r: &rusqlite::Row| -> rusqlite::Result<Vector> {
        Ok(Vector {
            vector_id: r.get(0)?,
            probe_id: r.get(1)?,
            test_name: r.get(2)?,
            description: r.get(3)?,
            probe_predicate: r.get(4)?,
            fix_strategy: r.get(5)?,
            patch_files_json: r.get(6)?,
            status: r.get(7)?,
            last_verified_at: r.get(8)?,
            first_introduced_commit: r.get(9)?,
        })
    };
    let rows: Vec<Vector> = if let Some(p) = probe_filter {
        stmt.query_map(params![p], mapper)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], mapper)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(rows)
}

pub fn run_probe(root: &Path, probe: &Probe) -> Result<ProbeOutcome> {
    let script_path = root.join(&probe.script_path);
    let source = std::fs::read_to_string(&script_path)
        .with_context(|| format!("failed to read probe script {}", script_path.display()))?;
    let started = Instant::now();
    let raw = run_browser_automation(
        root,
        &BrowserAutomationRequest {
            dir: None,
            timeout_ms: Some(probe.timeout_ms),
            source,
        },
    )?;
    let duration_ms = started.elapsed().as_millis() as u64;
    let (passed, failed_count, failed_tests, notes) = evaluate_outcome(&probe.parser_kind, &raw);
    let excerpt = raw.get("result").cloned().unwrap_or(Value::Null);
    Ok(ProbeOutcome {
        probe_id: probe.probe_id.clone(),
        passed_baseline: passed,
        failed_count,
        failed_tests,
        duration_ms,
        raw_excerpt: excerpt,
        notes,
    })
}

fn evaluate_outcome(parser_kind: &str, raw: &Value) -> (bool, usize, Vec<String>, Option<String>) {
    let result = raw.get("result").unwrap_or(&Value::Null);
    match parser_kind {
        "sannysoft" => {
            let failed = result
                .get("failed")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let names: Vec<String> = failed
                .iter()
                .filter_map(|r| r.get("name").and_then(Value::as_str).map(String::from))
                .collect();
            (names.is_empty(), names.len(), names, None)
        }
        "areyouheadless" => {
            let is_headless = result
                .get("isHeadless")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if is_headless {
                let verdict = result
                    .get("verdict")
                    .and_then(Value::as_str)
                    .unwrap_or("(missing verdict)")
                    .to_string();
                (false, 1, vec!["headless".into()], Some(verdict))
            } else {
                (true, 0, vec![], None)
            }
        }
        "incolumitas" => {
            let fails = result
                .get("fails")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let warnings = result
                .get("workerWarnings")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut all: Vec<String> = fails
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            for w in warnings.iter() {
                if let Some(s) = w.as_str() {
                    all.push(format!("worker:{}", s));
                }
            }
            (all.is_empty(), all.len(), all, None)
        }
        "creepjs" => {
            let signal = result
                .get("headlessSignal")
                .and_then(Value::as_str)
                .unwrap_or("");
            if signal == "UA LEAK" {
                let leaks = result
                    .get("result")
                    .and_then(|r| r.get("uaLeakStrings"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>();
                (false, leaks.len().max(1), leaks, Some("UA LEAK in fingerprint dump".into()))
            } else {
                (true, 0, vec![], None)
            }
        }
        _ => (false, 0, vec![], Some(format!("unknown parser_kind: {parser_kind}"))),
    }
}

pub fn record_run(conn: &Connection, outcome: &ProbeOutcome) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO web_unlock_test_runs
         (probe_id, executed_at, duration_ms, passed_baseline, failed_count,
          failed_tests_json, result_excerpt_json, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            outcome.probe_id,
            now,
            outcome.duration_ms as i64,
            if outcome.passed_baseline { 1i64 } else { 0i64 },
            outcome.failed_count as i64,
            serde_json::to_string(&outcome.failed_tests)?,
            serde_json::to_string(&outcome.raw_excerpt)?,
            outcome.notes.clone(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn last_run_summary(conn: &Connection, probe_id: &str) -> Result<Option<Value>> {
    let mut stmt = conn.prepare(
        "SELECT executed_at, duration_ms, passed_baseline, failed_count, failed_tests_json, notes
         FROM web_unlock_test_runs
         WHERE probe_id = ?1
         ORDER BY run_id DESC
         LIMIT 1",
    )?;
    let row = stmt
        .query_row(params![probe_id], |r| {
            Ok(json!({
                "executed_at": r.get::<_, String>(0)?,
                "duration_ms": r.get::<_, i64>(1)?,
                "passed_baseline": r.get::<_, i64>(2)? != 0,
                "failed_count": r.get::<_, i64>(3)?,
                "failed_tests": serde_json::from_str::<Value>(&r.get::<_, String>(4)?).unwrap_or(Value::Null),
                "notes": r.get::<_, Option<String>>(5)?,
            }))
        })
        .ok();
    Ok(row)
}

pub fn handle_unlock_command(root: &Path, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    match sub {
        "help" | "-h" | "--help" | "" => {
            print_usage();
            Ok(())
        }
        "list-probes" => cmd_list_probes(root),
        "list-vectors" => {
            let probe_filter = args.iter().skip(1).find(|a| !a.starts_with('-')).cloned();
            cmd_list_vectors(root, probe_filter.as_deref())
        }
        "baseline" => {
            let probe_filter = args.iter().skip(1).find(|a| !a.starts_with('-')).cloned();
            let record = args.iter().any(|a| a == "--record");
            cmd_baseline(root, probe_filter.as_deref(), record)
        }
        "history" => {
            let probe_filter = args.iter().skip(1).find(|a| !a.starts_with('-')).cloned();
            let limit = find_flag_u64(args, "--limit").unwrap_or(20);
            cmd_history(root, probe_filter.as_deref(), limit)
        }
        "add-vector" => cmd_add_vector(root, args),
        "set-vector-status" => cmd_set_vector_status(root, args),
        _ => {
            eprintln!("unknown subcommand: {sub}\n");
            print_usage();
            std::process::exit(2);
        }
    }
}

fn print_usage() {
    println!("ctox web unlock <subcommand>");
    println!();
    println!("Subcommands:");
    println!("  list-probes                    Print all registered detection-site probes");
    println!("  list-vectors [<probe_id>]      Print known vectors, optionally filtered by probe");
    println!("  baseline [<probe_id>] [--record]");
    println!("                                 Run all enabled probes (or one), compare to baseline,");
    println!("                                 exit non-zero if any regressed. --record persists run.");
    println!("  history [<probe_id>] [--limit N]");
    println!("                                 Show recent test runs from the run history");
    println!("  add-vector --id <vid> --probe <pid> --test <name> --desc <text> --fix <text>");
    println!("                                 Register a newly discovered vector");
    println!("  set-vector-status --id <vid> --status <working|broken|untested>");
    println!("                                 Mark a vector's current status");
}

fn cmd_list_probes(root: &Path) -> Result<()> {
    let conn = open_db(root)?;
    let probes = load_probes(&conn, false)?;
    let out: Vec<Value> = probes
        .iter()
        .map(|p| {
            let last = last_run_summary(&conn, &p.probe_id).ok().flatten();
            json!({
                "probe_id": p.probe_id,
                "site_name": p.site_name,
                "probe_url": p.probe_url,
                "script_path": p.script_path,
                "parser_kind": p.parser_kind,
                "timeout_ms": p.timeout_ms,
                "enabled": p.enabled,
                "last_run": last,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn cmd_list_vectors(root: &Path, probe_filter: Option<&str>) -> Result<()> {
    let conn = open_db(root)?;
    let vectors = load_vectors(&conn, probe_filter)?;
    let out: Vec<Value> = vectors
        .iter()
        .map(|v| {
            json!({
                "vector_id": v.vector_id,
                "probe_id": v.probe_id,
                "test_name": v.test_name,
                "description": v.description,
                "probe_predicate": v.probe_predicate,
                "fix_strategy": v.fix_strategy,
                "patch_files": serde_json::from_str::<Value>(&v.patch_files_json).unwrap_or(Value::Null),
                "status": v.status,
                "last_verified_at": v.last_verified_at,
                "first_introduced_commit": v.first_introduced_commit,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn cmd_baseline(root: &Path, probe_filter: Option<&str>, record: bool) -> Result<()> {
    let conn = open_db(root)?;
    let probes_all = load_probes(&conn, true)?;
    let probes: Vec<Probe> = if let Some(p) = probe_filter {
        probes_all.into_iter().filter(|x| x.probe_id == p).collect()
    } else {
        probes_all
    };
    if probes.is_empty() {
        anyhow::bail!("no matching enabled probes registered");
    }
    let mut all_passed = true;
    let mut summary: Vec<Value> = Vec::new();
    for probe in &probes {
        let outcome = match run_probe(root, probe) {
            Ok(o) => o,
            Err(err) => {
                all_passed = false;
                summary.push(json!({
                    "probe_id": probe.probe_id,
                    "site_name": probe.site_name,
                    "passed_baseline": false,
                    "error": format!("{err:#}"),
                }));
                continue;
            }
        };
        if !outcome.passed_baseline {
            all_passed = false;
        }
        if record {
            let _ = record_run(&conn, &outcome)?;
        }
        summary.push(json!({
            "probe_id": outcome.probe_id,
            "site_name": probe.site_name,
            "passed_baseline": outcome.passed_baseline,
            "failed_count": outcome.failed_count,
            "failed_tests": outcome.failed_tests,
            "duration_ms": outcome.duration_ms,
            "notes": outcome.notes,
        }));
    }
    let out = json!({
        "ok": all_passed,
        "recorded": record,
        "probes": summary,
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    if !all_passed {
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_history(root: &Path, probe_filter: Option<&str>, limit: u64) -> Result<()> {
    let conn = open_db(root)?;
    let map = |r: &rusqlite::Row| -> rusqlite::Result<Value> {
        Ok(json!({
            "run_id": r.get::<_, i64>(0)?,
            "probe_id": r.get::<_, String>(1)?,
            "executed_at": r.get::<_, String>(2)?,
            "duration_ms": r.get::<_, i64>(3)?,
            "passed_baseline": r.get::<_, i64>(4)? != 0,
            "failed_count": r.get::<_, i64>(5)?,
            "failed_tests": serde_json::from_str::<Value>(&r.get::<_, String>(6)?).unwrap_or(Value::Null),
            "notes": r.get::<_, Option<String>>(7)?,
        }))
    };
    let rows: Vec<Value> = if let Some(p) = probe_filter {
        let mut stmt = conn.prepare(
            "SELECT run_id, probe_id, executed_at, duration_ms, passed_baseline,
                    failed_count, failed_tests_json, notes
             FROM web_unlock_test_runs
             WHERE probe_id = ?1
             ORDER BY run_id DESC
             LIMIT ?2",
        )?;
        let collected = stmt
            .query_map(params![p, limit as i64], map)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        collected
    } else {
        let mut stmt = conn.prepare(
            "SELECT run_id, probe_id, executed_at, duration_ms, passed_baseline,
                    failed_count, failed_tests_json, notes
             FROM web_unlock_test_runs
             ORDER BY run_id DESC
             LIMIT ?1",
        )?;
        let collected = stmt
            .query_map(params![limit as i64], map)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        collected
    };
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}

fn cmd_add_vector(root: &Path, args: &[String]) -> Result<()> {
    let vector_id = find_flag(args, "--id").context("--id required")?;
    let probe_id = find_flag(args, "--probe").context("--probe required")?;
    let test_name = find_flag(args, "--test").context("--test required")?;
    let description = find_flag(args, "--desc").context("--desc required")?;
    let fix_strategy = find_flag(args, "--fix").context("--fix required")?;
    let probe_predicate = find_flag(args, "--predicate");
    let patch_files = find_flag(args, "--patch-files");
    let patch_files_json = patch_files
        .map(|s| {
            let v: Vec<&str> = s.split(',').map(str::trim).collect();
            serde_json::to_string(&v).unwrap_or_else(|_| "[]".to_string())
        })
        .unwrap_or_else(|| "[]".to_string());

    let conn = open_db(root)?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO web_unlock_vectors
         (vector_id, probe_id, test_name, description, probe_predicate,
          fix_strategy, patch_files_json, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            vector_id,
            probe_id,
            test_name,
            description,
            probe_predicate,
            fix_strategy,
            patch_files_json,
            "untested",
            now,
            now,
        ],
    )?;
    println!("{}", serde_json::to_string_pretty(&json!({"ok": true, "vector_id": vector_id}))?);
    Ok(())
}

fn cmd_set_vector_status(root: &Path, args: &[String]) -> Result<()> {
    let vector_id = find_flag(args, "--id").context("--id required")?;
    let status = find_flag(args, "--status").context("--status required")?;
    if !matches!(status, "working" | "broken" | "untested") {
        anyhow::bail!("--status must be one of: working, broken, untested");
    }
    let conn = open_db(root)?;
    let now = Utc::now().to_rfc3339();
    let last_verified = if status == "working" {
        Some(now.clone())
    } else {
        None
    };
    let updated = conn.execute(
        "UPDATE web_unlock_vectors
         SET status = ?1, last_verified_at = COALESCE(?2, last_verified_at), updated_at = ?3
         WHERE vector_id = ?4",
        params![status, last_verified, now, vector_id],
    )?;
    if updated == 0 {
        anyhow::bail!("no vector with id {vector_id}");
    }
    println!("{}", serde_json::to_string_pretty(&json!({"ok": true, "vector_id": vector_id, "status": status}))?);
    Ok(())
}

fn find_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == flag {
            return it.next().map(String::as_str);
        }
    }
    None
}

fn find_flag_u64(args: &[String], flag: &str) -> Option<u64> {
    find_flag(args, flag).and_then(|v| v.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_sannysoft_passed_when_no_failed() {
        let raw = json!({
            "result": {
                "failed": [],
                "totals": {"failed": 0}
            }
        });
        let (passed, count, list, _) = evaluate_outcome("sannysoft", &raw);
        assert!(passed);
        assert_eq!(count, 0);
        assert!(list.is_empty());
    }

    #[test]
    fn evaluate_sannysoft_failed_lists_names() {
        let raw = json!({
            "result": {
                "failed": [
                    {"name": "User Agent (Old)", "cls": "result failed"}
                ],
                "totals": {"failed": 1}
            }
        });
        let (passed, count, list, _) = evaluate_outcome("sannysoft", &raw);
        assert!(!passed);
        assert_eq!(count, 1);
        assert_eq!(list, vec!["User Agent (Old)".to_string()]);
    }

    #[test]
    fn evaluate_areyouheadless_passes_when_not_headless() {
        let raw = json!({"result": {"isHeadless": false}});
        let (passed, count, _, _) = evaluate_outcome("areyouheadless", &raw);
        assert!(passed);
        assert_eq!(count, 0);
    }

    #[test]
    fn evaluate_incolumitas_aggregates_fails_and_warnings() {
        let raw = json!({
            "result": {
                "fails": ["new-tests.overflowTest"],
                "workerWarnings": ["serviceWorker.userAgent contains HeadlessChrome"]
            }
        });
        let (passed, count, list, _) = evaluate_outcome("incolumitas", &raw);
        assert!(!passed);
        assert_eq!(count, 2);
        assert_eq!(list[0], "new-tests.overflowTest");
        assert!(list[1].starts_with("worker:"));
    }

    #[test]
    fn evaluate_creepjs_passes_when_clean() {
        let raw = json!({"result": {"headlessSignal": "clean"}});
        let (passed, _, _, _) = evaluate_outcome("creepjs", &raw);
        assert!(passed);
    }

    #[test]
    fn evaluate_creepjs_fails_on_ua_leak() {
        let raw = json!({
            "result": {
                "headlessSignal": "UA LEAK",
                "result": {"uaLeakStrings": ["HeadlessChrome/147"]}
            }
        });
        let (passed, _, list, notes) = evaluate_outcome("creepjs", &raw);
        assert!(!passed);
        assert_eq!(list, vec!["HeadlessChrome/147".to_string()]);
        assert!(notes.is_some());
    }

    #[test]
    fn schema_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        ensure_schema(&conn).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name LIKE 'web_unlock_%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn seed_populates_probes_and_vectors() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        seed_if_empty(&conn).unwrap();
        let p: i64 = conn
            .query_row("SELECT count(*) FROM web_unlock_probes", [], |r| r.get(0))
            .unwrap();
        let v: i64 = conn
            .query_row("SELECT count(*) FROM web_unlock_vectors", [], |r| r.get(0))
            .unwrap();
        assert_eq!(p, 4, "expected 4 probes seeded");
        assert!(v >= 15, "expected at least 15 vectors seeded, got {v}");
    }
}
