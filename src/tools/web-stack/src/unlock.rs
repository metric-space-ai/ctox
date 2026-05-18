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
    seed_merge_missing(&conn)?;
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

        CREATE TABLE IF NOT EXISTS web_unlock_signals (
            signal_id INTEGER PRIMARY KEY AUTOINCREMENT,
            detected_at TEXT NOT NULL,
            source TEXT NOT NULL,
            probe_url TEXT,
            evidence_json TEXT,
            resolved INTEGER NOT NULL DEFAULT 0,
            resolved_at TEXT,
            resolved_by_repair_id INTEGER,
            notes TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_web_unlock_vectors_probe ON web_unlock_vectors(probe_id);
        CREATE INDEX IF NOT EXISTS idx_web_unlock_vectors_status ON web_unlock_vectors(status);
        CREATE INDEX IF NOT EXISTS idx_web_unlock_test_runs_probe_time ON web_unlock_test_runs(probe_id, executed_at);
        CREATE INDEX IF NOT EXISTS idx_web_unlock_signals_resolved ON web_unlock_signals(resolved, detected_at);
        CREATE INDEX IF NOT EXISTS idx_web_unlock_signals_source ON web_unlock_signals(source);
        "#,
    )
    .context("failed to create web_unlock_* schema")
}

/// Adds rows from the embedded seed JSON to the registry if their primary
/// keys (probe_id / vector_id) are not already present. Operator edits to
/// existing rows are preserved — this uses `INSERT OR IGNORE`, not
/// `OR REPLACE`. Safe to call on every open; cheap when nothing's new.
fn seed_merge_missing(conn: &Connection) -> Result<()> {
    let seed: Value = serde_json::from_str(SEED_JSON)
        .context("failed to parse embedded web_unlock_seed.json")?;
    let now = Utc::now().to_rfc3339();

    let probes = seed
        .get("probes")
        .and_then(Value::as_array)
        .context("seed.probes missing")?;
    for p in probes {
        conn.execute(
            "INSERT OR IGNORE INTO web_unlock_probes
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
            "INSERT OR IGNORE INTO web_unlock_vectors
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
            let probe_filter = first_positional(args);
            cmd_list_vectors(root, probe_filter.as_deref())
        }
        "baseline" => {
            let probe_filter = first_positional(args);
            let record = args.iter().any(|a| a == "--record");
            let auto_repair = args.iter().any(|a| a == "--auto-repair");
            cmd_baseline(root, probe_filter.as_deref(), record, auto_repair)
        }
        "history" => {
            let probe_filter = first_positional(args);
            let limit = find_flag_u64(args, "--limit").unwrap_or(20);
            cmd_history(root, probe_filter.as_deref(), limit)
        }
        "add-vector" => cmd_add_vector(root, args),
        "set-vector-status" => cmd_set_vector_status(root, args),
        "repair" => {
            let action = args.get(1).map(String::as_str).unwrap_or("");
            match action {
                "start" => cmd_repair_start(root, &args[1..]),
                "complete" => cmd_repair_complete(root, &args[1..]),
                "list" => cmd_repair_list(root, &args[1..]),
                "" | "help" | "-h" | "--help" => {
                    print_repair_usage();
                    Ok(())
                }
                _ => {
                    eprintln!("unknown repair action: {action}\n");
                    print_repair_usage();
                    std::process::exit(2);
                }
            }
        }
        "signals" => {
            let action = args.get(1).map(String::as_str).unwrap_or("list");
            match action {
                "list" => cmd_signals_list(root, &args[1..]),
                "resolve" => cmd_signals_resolve(root, &args[1..]),
                "record" => cmd_signals_record(root, &args[1..]),
                "help" | "-h" | "--help" => {
                    print_signals_usage();
                    Ok(())
                }
                _ => {
                    eprintln!("unknown signals action: {action}\n");
                    print_signals_usage();
                    std::process::exit(2);
                }
            }
        }
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
    println!("  baseline [<probe_id>] [--record] [--auto-repair]");
    println!("                                 Run all enabled probes (or one), compare to baseline,");
    println!("                                 exit non-zero if any regressed. --record persists run.");
    println!("                                 --auto-repair opens pending repair rows for vectors");
    println!("                                 matching the failed test names.");
    println!("  history [<probe_id>] [--limit N]");
    println!("                                 Show recent test runs from the run history");
    println!("  add-vector --id <vid> --probe <pid> --test <name> --desc <text> --fix <text>");
    println!("                                 Register a newly discovered vector");
    println!("  set-vector-status --id <vid> --status <working|broken|untested>");
    println!("                                 Mark a vector's current status");
    println!("  repair <start|complete|list>");
    println!("                                 Manage repair attempts (see `repair help`)");
    println!("  signals <list|resolve|record>");
    println!("                                 Detection signals logged by the runners");
    println!("                                 (see `signals help`)");
}

fn print_signals_usage() {
    println!("ctox web unlock signals <action>");
    println!();
    println!("Actions:");
    println!("  list [--unresolved] [--source <name>] [--limit N]");
    println!("                                 Show recent detection signals (CAPTCHAs,");
    println!("                                 Cloudflare challenges, etc.) logged by the");
    println!("                                 web-search and browser-automation runners.");
    println!("  resolve --id <signal_id> [--repair <repair_id>] [--notes <text>]");
    println!("                                 Mark a signal resolved, optionally linking the");
    println!("                                 repair that fixed it.");
    println!("  record --source <name> [--url <url>] [--evidence <json>]");
    println!("                                 Manual signal entry — for skills emitting");
    println!("                                 their own detection observations.");
}

fn print_repair_usage() {
    println!("ctox web unlock repair <action>");
    println!();
    println!("Actions:");
    println!("  start --vector <vid> [--run-id <n>] [--notes <text>]");
    println!("                                 Open a pending repair for a known vector.");
    println!("                                 Emits a plan with patch_files + fix_strategy and");
    println!("                                 the new repair_id. Marks the vector status 'broken'.");
    println!("  complete --id <repair_id> (--succeeded | --failed)");
    println!("           [--commit <sha>] [--notes <text>]");
    println!("                                 Close a repair. On --succeeded, the linked vector");
    println!("                                 flips back to 'working' with a fresh last_verified_at.");
    println!("  list [--status <pending|succeeded|failed>] [--limit N]");
    println!("                                 Show recent repair attempts.");
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

fn cmd_baseline(
    root: &Path,
    probe_filter: Option<&str>,
    record: bool,
    auto_repair: bool,
) -> Result<()> {
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
        let run_id = if record {
            Some(record_run(&conn, &outcome)?)
        } else {
            None
        };

        // Auto-open repairs: each failed_test name is matched to a known
        // vector under the same probe_id. We can't always map test names
        // back to vector_ids automatically (the test_name -> vector_id
        // edge is not always 1:1), so we open at most one repair per probe
        // referencing the first vector with a matching test_name. The
        // skill can refine the mapping later.
        let mut opened_repairs: Vec<i64> = Vec::new();
        if auto_repair && !outcome.passed_baseline {
            let vectors = load_vectors(&conn, Some(&probe.probe_id))?;
            for failed_test in &outcome.failed_tests {
                if let Some(vec_match) = vectors.iter().find(|v| {
                    failed_test == &v.test_name
                        || failed_test.ends_with(&v.test_name)
                        || failed_test.contains(&v.test_name)
                }) {
                    let already_pending: i64 = conn
                        .query_row(
                            "SELECT count(*) FROM web_unlock_repairs
                             WHERE vector_id = ?1 AND succeeded IS NULL",
                            params![vec_match.vector_id],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    if already_pending == 0 {
                        let desc = format!(
                            "auto-opened from baseline regression in {}: {}",
                            probe.probe_id, failed_test
                        );
                        let id = open_repair(
                            &conn,
                            &vec_match.vector_id,
                            run_id,
                            &desc,
                            None,
                        )?;
                        opened_repairs.push(id);
                    }
                }
            }
        }

        summary.push(json!({
            "probe_id": outcome.probe_id,
            "site_name": probe.site_name,
            "passed_baseline": outcome.passed_baseline,
            "failed_count": outcome.failed_count,
            "failed_tests": outcome.failed_tests,
            "duration_ms": outcome.duration_ms,
            "notes": outcome.notes,
            "run_id": run_id,
            "opened_repairs": opened_repairs,
        }));
    }
    let out = json!({
        "ok": all_passed,
        "recorded": record,
        "auto_repair": auto_repair,
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

// ─────────────────────────────────────────────────────────────────────────────
// Signals — call sites in web_search/browser-automation log a detection
// signal whenever they see a CAPTCHA, Cloudflare challenge, /sorry/index,
// or empty-result regression. The skill (or the operator) reviews unresolved
// signals and triggers a repair flow that resolves them.
// ─────────────────────────────────────────────────────────────────────────────

/// Lossy signal recorder — never fails the caller. If the DB is unreachable
/// or the schema is missing, the signal is silently dropped. This matches
/// the lossy semantics of other CTOX evidence recorders (harness flow etc.).
///
/// The `source` identifier should be short and stable (e.g. "google_search",
/// "browser_automation", "web_scrape"). `probe_url` is the URL that triggered
/// the signal, `evidence` is arbitrary JSON.
pub fn record_signal_lossy(root: &Path, source: &str, probe_url: Option<&str>, evidence: Value) {
    let _ = (|| -> Result<()> {
        let conn = open_db(root)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO web_unlock_signals
             (detected_at, source, probe_url, evidence_json, resolved)
             VALUES (?1, ?2, ?3, ?4, 0)",
            params![now, source, probe_url, evidence.to_string()],
        )?;
        Ok(())
    })();
}

/// Strict signal recorder — returns Err if the write fails. Use when the
/// caller specifically wants to know whether the persist succeeded.
pub fn record_signal(
    conn: &Connection,
    source: &str,
    probe_url: Option<&str>,
    evidence: Value,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO web_unlock_signals
         (detected_at, source, probe_url, evidence_json, resolved)
         VALUES (?1, ?2, ?3, ?4, 0)",
        params![now, source, probe_url, evidence.to_string()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn resolve_signal(
    conn: &Connection,
    signal_id: i64,
    repair_id: Option<i64>,
    notes: Option<&str>,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let updated = conn.execute(
        "UPDATE web_unlock_signals
         SET resolved = 1, resolved_at = ?1, resolved_by_repair_id = ?2,
             notes = COALESCE(?3, notes)
         WHERE signal_id = ?4",
        params![now, repair_id, notes, signal_id],
    )?;
    if updated == 0 {
        anyhow::bail!("no signal with id {signal_id}");
    }
    Ok(())
}

fn cmd_signals_list(root: &Path, args: &[String]) -> Result<()> {
    let unresolved_only = args.iter().any(|a| a == "--unresolved");
    let source_filter = find_flag(args, "--source");
    let limit = find_flag_u64(args, "--limit").unwrap_or(50);
    let conn = open_db(root)?;
    let map = |r: &rusqlite::Row| -> rusqlite::Result<Value> {
        let resolved: i64 = r.get(5)?;
        let evidence_raw: Option<String> = r.get(4)?;
        let evidence = evidence_raw
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(Value::Null);
        Ok(json!({
            "signal_id": r.get::<_, i64>(0)?,
            "detected_at": r.get::<_, String>(1)?,
            "source": r.get::<_, String>(2)?,
            "probe_url": r.get::<_, Option<String>>(3)?,
            "evidence": evidence,
            "resolved": resolved != 0,
            "resolved_at": r.get::<_, Option<String>>(6)?,
            "resolved_by_repair_id": r.get::<_, Option<i64>>(7)?,
            "notes": r.get::<_, Option<String>>(8)?,
        }))
    };
    let select = "SELECT signal_id, detected_at, source, probe_url, evidence_json,
                         resolved, resolved_at, resolved_by_repair_id, notes
                  FROM web_unlock_signals";
    let rows: Vec<Value> = match (unresolved_only, source_filter) {
        (true, Some(src)) => {
            let mut stmt = conn.prepare(&format!(
                "{} WHERE resolved = 0 AND source = ?1 ORDER BY signal_id DESC LIMIT ?2",
                select
            ))?;
            let collected = stmt
                .query_map(params![src, limit as i64], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            collected
        }
        (true, None) => {
            let mut stmt = conn.prepare(&format!(
                "{} WHERE resolved = 0 ORDER BY signal_id DESC LIMIT ?1",
                select
            ))?;
            let collected = stmt
                .query_map(params![limit as i64], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            collected
        }
        (false, Some(src)) => {
            let mut stmt = conn.prepare(&format!(
                "{} WHERE source = ?1 ORDER BY signal_id DESC LIMIT ?2",
                select
            ))?;
            let collected = stmt
                .query_map(params![src, limit as i64], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            collected
        }
        (false, None) => {
            let mut stmt = conn.prepare(&format!(
                "{} ORDER BY signal_id DESC LIMIT ?1",
                select
            ))?;
            let collected = stmt
                .query_map(params![limit as i64], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            collected
        }
    };
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}

fn cmd_signals_resolve(root: &Path, args: &[String]) -> Result<()> {
    let signal_id_raw = find_flag(args, "--id").context("--id required")?;
    let signal_id: i64 = signal_id_raw
        .parse()
        .with_context(|| format!("--id must be integer, got `{signal_id_raw}`"))?;
    let repair_id = find_flag(args, "--repair").and_then(|v| v.parse::<i64>().ok());
    let notes = find_flag(args, "--notes");
    let conn = open_db(root)?;
    resolve_signal(&conn, signal_id, repair_id, notes)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "signal_id": signal_id,
            "resolved_by_repair_id": repair_id,
        }))?
    );
    Ok(())
}

fn cmd_signals_record(root: &Path, args: &[String]) -> Result<()> {
    let source = find_flag(args, "--source").context("--source required")?;
    let probe_url = find_flag(args, "--url");
    let evidence_raw = find_flag(args, "--evidence");
    let evidence: Value = match evidence_raw {
        Some(s) => serde_json::from_str(s)
            .with_context(|| format!("--evidence must be JSON, got `{s}`"))?,
        None => Value::Object(Default::default()),
    };
    let conn = open_db(root)?;
    let signal_id = record_signal(&conn, source, probe_url, evidence)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "signal_id": signal_id,
            "source": source,
        }))?
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Repair flow
// ─────────────────────────────────────────────────────────────────────────────

fn load_vector(conn: &Connection, vector_id: &str) -> Result<Option<Vector>> {
    let mut stmt = conn.prepare(
        "SELECT vector_id, probe_id, test_name, description, probe_predicate,
                fix_strategy, patch_files_json, status, last_verified_at, first_introduced_commit
         FROM web_unlock_vectors
         WHERE vector_id = ?1",
    )?;
    let row = stmt
        .query_row(params![vector_id], |r| {
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
        })
        .ok();
    Ok(row)
}

pub fn open_repair(
    conn: &Connection,
    vector_id: &str,
    triggered_by_run_id: Option<i64>,
    description: &str,
    notes: Option<&str>,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO web_unlock_repairs
         (triggered_by_run_id, vector_id, description, succeeded, notes, created_at)
         VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
        params![triggered_by_run_id, vector_id, description, notes, now],
    )?;
    let repair_id = conn.last_insert_rowid();
    // Mark the vector as broken until the repair completes.
    conn.execute(
        "UPDATE web_unlock_vectors
         SET status = 'broken', updated_at = ?1
         WHERE vector_id = ?2",
        params![now, vector_id],
    )?;
    Ok(repair_id)
}

pub fn close_repair(
    conn: &Connection,
    repair_id: i64,
    succeeded: bool,
    resulting_commit: Option<&str>,
    notes: Option<&str>,
) -> Result<Option<String>> {
    let updated = conn.execute(
        "UPDATE web_unlock_repairs
         SET succeeded = ?1, resulting_commit = COALESCE(?2, resulting_commit),
             notes = COALESCE(?3, notes)
         WHERE repair_id = ?4",
        params![
            if succeeded { 1i64 } else { 0i64 },
            resulting_commit,
            notes,
            repair_id,
        ],
    )?;
    if updated == 0 {
        anyhow::bail!("no repair with id {repair_id}");
    }
    let vector_id: Option<String> = conn
        .query_row(
            "SELECT vector_id FROM web_unlock_repairs WHERE repair_id = ?1",
            params![repair_id],
            |r| r.get(0),
        )
        .ok();
    if succeeded {
        if let Some(ref vid) = vector_id {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE web_unlock_vectors
                 SET status = 'working', last_verified_at = ?1, updated_at = ?1
                 WHERE vector_id = ?2",
                params![now, vid],
            )?;
        }
    }
    Ok(vector_id)
}

fn cmd_repair_start(root: &Path, args: &[String]) -> Result<()> {
    let vector_id = find_flag(args, "--vector").context("--vector required")?;
    let triggered_by_run_id = find_flag(args, "--run-id").and_then(|v| v.parse::<i64>().ok());
    let notes = find_flag(args, "--notes");

    let conn = open_db(root)?;
    let vector = load_vector(&conn, vector_id)?
        .with_context(|| format!("no vector with id `{vector_id}`"))?;
    let description = format!(
        "Repair vector `{}` ({}.{}): {}",
        vector.vector_id, vector.probe_id, vector.test_name, vector.fix_strategy
    );
    let repair_id = open_repair(&conn, vector_id, triggered_by_run_id, &description, notes)?;
    let patch_files: Value =
        serde_json::from_str(&vector.patch_files_json).unwrap_or(Value::Null);
    let plan = json!({
        "ok": true,
        "repair_id": repair_id,
        "vector": {
            "vector_id": vector.vector_id,
            "probe_id": vector.probe_id,
            "test_name": vector.test_name,
            "description": vector.description,
            "probe_predicate": vector.probe_predicate,
            "fix_strategy": vector.fix_strategy,
            "patch_files": patch_files,
        },
        "next_steps": [
            "Edit the files listed in `patch_files` according to `fix_strategy`.",
            format!("Run `ctox web unlock baseline {} --record` to verify the fix.", vector.probe_id),
            "Commit the changes and capture the resulting commit hash.",
            format!("Close the repair: `ctox web unlock repair complete --id {repair_id} --commit <sha> --succeeded`."),
            format!("On failure: `ctox web unlock repair complete --id {repair_id} --failed --notes \"<reason>\"`."),
        ],
    });
    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(())
}

fn cmd_repair_complete(root: &Path, args: &[String]) -> Result<()> {
    let repair_id_raw = find_flag(args, "--id").context("--id required")?;
    let repair_id: i64 = repair_id_raw
        .parse()
        .with_context(|| format!("--id must be integer, got `{repair_id_raw}`"))?;
    let succeeded = args.iter().any(|a| a == "--succeeded");
    let failed = args.iter().any(|a| a == "--failed");
    if succeeded == failed {
        anyhow::bail!("exactly one of --succeeded or --failed is required");
    }
    let commit_sha = find_flag(args, "--commit");
    let notes = find_flag(args, "--notes");

    let conn = open_db(root)?;
    let vector_id = close_repair(&conn, repair_id, succeeded, commit_sha, notes)?;
    let out = json!({
        "ok": true,
        "repair_id": repair_id,
        "succeeded": succeeded,
        "resulting_commit": commit_sha,
        "vector_id": vector_id,
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn cmd_repair_list(root: &Path, args: &[String]) -> Result<()> {
    let status_filter = find_flag(args, "--status");
    let limit = find_flag_u64(args, "--limit").unwrap_or(20);
    let conn = open_db(root)?;
    let map = |r: &rusqlite::Row| -> rusqlite::Result<Value> {
        let succeeded: Option<i64> = r.get(3)?;
        let status_str = match succeeded {
            None => "pending",
            Some(0) => "failed",
            Some(_) => "succeeded",
        };
        Ok(json!({
            "repair_id": r.get::<_, i64>(0)?,
            "vector_id": r.get::<_, Option<String>>(1)?,
            "triggered_by_run_id": r.get::<_, Option<i64>>(2)?,
            "succeeded": succeeded.map(|v| v != 0),
            "status": status_str,
            "resulting_commit": r.get::<_, Option<String>>(4)?,
            "description": r.get::<_, String>(5)?,
            "notes": r.get::<_, Option<String>>(6)?,
            "created_at": r.get::<_, String>(7)?,
        }))
    };
    let select = "SELECT repair_id, vector_id, triggered_by_run_id, succeeded,
                         resulting_commit, description, notes, created_at
                  FROM web_unlock_repairs";
    let rows: Vec<Value> = match status_filter {
        Some("pending") => {
            let mut stmt = conn.prepare(&format!(
                "{} WHERE succeeded IS NULL ORDER BY repair_id DESC LIMIT ?1",
                select
            ))?;
            let collected = stmt
                .query_map(params![limit as i64], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            collected
        }
        Some("succeeded") => {
            let mut stmt = conn.prepare(&format!(
                "{} WHERE succeeded = 1 ORDER BY repair_id DESC LIMIT ?1",
                select
            ))?;
            let collected = stmt
                .query_map(params![limit as i64], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            collected
        }
        Some("failed") => {
            let mut stmt = conn.prepare(&format!(
                "{} WHERE succeeded = 0 ORDER BY repair_id DESC LIMIT ?1",
                select
            ))?;
            let collected = stmt
                .query_map(params![limit as i64], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            collected
        }
        Some(other) => anyhow::bail!(
            "--status must be one of pending|succeeded|failed (got `{other}`)"
        ),
        None => {
            let mut stmt = conn.prepare(&format!(
                "{} ORDER BY repair_id DESC LIMIT ?1",
                select
            ))?;
            let collected = stmt
                .query_map(params![limit as i64], map)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            collected
        }
    };
    println!("{}", serde_json::to_string_pretty(&rows)?);
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

/// Return the first positional (non-flag, non-flag-value) argument after
/// the subcommand. `args[0]` is the subcommand itself. We skip flags and
/// the value that follows each known value-bearing flag.
fn first_positional(args: &[String]) -> Option<String> {
    const VALUE_FLAGS: &[&str] = &[
        "--limit", "--id", "--probe", "--test", "--desc", "--fix",
        "--predicate", "--patch-files", "--status",
    ];
    let mut i = 1; // skip subcommand
    while i < args.len() {
        let a = &args[i];
        if a.starts_with("--") {
            if VALUE_FLAGS.iter().any(|f| f == a) {
                i += 2; // skip flag and its value
            } else {
                i += 1; // boolean flag like --record
            }
        } else {
            return Some(a.clone());
        }
    }
    None
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
        assert_eq!(count, 5);
    }

    fn seeded_in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        seed_merge_missing(&conn).unwrap();
        conn
    }

    #[test]
    fn open_repair_marks_vector_broken_and_records_row() {
        let conn = seeded_in_memory_db();
        let id = open_repair(
            &conn,
            "navigator-webdriver-exists",
            None,
            "test repair",
            Some("first attempt"),
        )
        .unwrap();
        assert!(id > 0);
        let status: String = conn
            .query_row(
                "SELECT status FROM web_unlock_vectors WHERE vector_id = ?1",
                params!["navigator-webdriver-exists"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "broken");
        let succeeded: Option<i64> = conn
            .query_row(
                "SELECT succeeded FROM web_unlock_repairs WHERE repair_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(succeeded.is_none(), "newly opened repair should be pending");
    }

    #[test]
    fn close_repair_succeeded_flips_vector_back_to_working() {
        let conn = seeded_in_memory_db();
        let id = open_repair(&conn, "plugins-uint32-overflow", None, "x", None).unwrap();
        let vid = close_repair(&conn, id, true, Some("abc123"), Some("verified")).unwrap();
        assert_eq!(vid.as_deref(), Some("plugins-uint32-overflow"));
        let row: (String, Option<String>) = conn
            .query_row(
                "SELECT status, last_verified_at FROM web_unlock_vectors WHERE vector_id = ?1",
                params!["plugins-uint32-overflow"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(row.0, "working");
        assert!(row.1.is_some(), "last_verified_at should be set");
        let commit: Option<String> = conn
            .query_row(
                "SELECT resulting_commit FROM web_unlock_repairs WHERE repair_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(commit.as_deref(), Some("abc123"));
    }

    #[test]
    fn close_repair_failed_leaves_vector_broken() {
        let conn = seeded_in_memory_db();
        let id = open_repair(&conn, "connection-rtt-zero", None, "x", None).unwrap();
        close_repair(&conn, id, false, None, Some("fix did not stick")).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM web_unlock_vectors WHERE vector_id = ?1",
                params!["connection-rtt-zero"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "broken");
    }

    #[test]
    fn close_repair_missing_id_errors() {
        let conn = seeded_in_memory_db();
        let err = close_repair(&conn, 9_999_999, true, None, None).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("no repair with id"));
    }

    #[test]
    fn record_signal_persists_row() {
        let conn = seeded_in_memory_db();
        let id = record_signal(
            &conn,
            "google_search",
            Some("https://www.google.com/sorry/index"),
            json!({"reason": "captcha", "query": "test"}),
        )
        .unwrap();
        assert!(id > 0);
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM web_unlock_signals WHERE signal_id = ?1 AND resolved = 0",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn resolve_signal_marks_resolved_and_links_repair() {
        let conn = seeded_in_memory_db();
        let signal_id = record_signal(
            &conn,
            "google_search",
            Some("https://www.google.com/sorry/index"),
            json!({"reason": "captcha"}),
        )
        .unwrap();
        let repair_id =
            open_repair(&conn, "navigator-webdriver-exists", None, "fix captcha", None).unwrap();
        resolve_signal(&conn, signal_id, Some(repair_id), Some("captcha addressed")).unwrap();
        let row: (i64, Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT resolved, resolved_by_repair_id, notes
                 FROM web_unlock_signals WHERE signal_id = ?1",
                params![signal_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row.0, 1);
        assert_eq!(row.1, Some(repair_id));
        assert_eq!(row.2.as_deref(), Some("captcha addressed"));
    }

    #[test]
    fn resolve_signal_missing_id_errors() {
        let conn = seeded_in_memory_db();
        let err = resolve_signal(&conn, 9_999_999, None, None).unwrap_err();
        assert!(format!("{err}").contains("no signal with id"));
    }

    #[test]
    fn record_signal_lossy_does_not_panic_on_bad_root() {
        // Path under /dev/null shouldn't exist; lossy should swallow the error.
        let bad_root = std::path::Path::new("/dev/null/never");
        record_signal_lossy(
            bad_root,
            "test_source",
            None,
            json!({}),
        );
    }

    #[test]
    fn first_positional_skips_known_value_flags() {
        let raw = vec![
            "history".to_string(),
            "--limit".to_string(),
            "3".to_string(),
        ];
        assert_eq!(first_positional(&raw), None);
        let raw = vec![
            "history".to_string(),
            "sannysoft".to_string(),
            "--limit".to_string(),
            "3".to_string(),
        ];
        assert_eq!(first_positional(&raw).as_deref(), Some("sannysoft"));
        let raw = vec![
            "baseline".to_string(),
            "--record".to_string(),
            "creepjs".to_string(),
        ];
        assert_eq!(first_positional(&raw).as_deref(), Some("creepjs"));
    }

    #[test]
    fn seed_populates_probes_and_vectors() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        seed_merge_missing(&conn).unwrap();
        let p: i64 = conn
            .query_row("SELECT count(*) FROM web_unlock_probes", [], |r| r.get(0))
            .unwrap();
        let v: i64 = conn
            .query_row("SELECT count(*) FROM web_unlock_vectors", [], |r| r.get(0))
            .unwrap();
        assert_eq!(p, 5, "expected 5 probes seeded (4 detection sites + humanlike)");
        assert!(v >= 15, "expected at least 15 vectors seeded, got {v}");
    }
}
