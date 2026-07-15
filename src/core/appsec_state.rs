use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn project_cli_result(root: &Path, forwarded_args: &[String], output: &Value) -> Result<Value> {
    let state_dir = appsec_state_dir(root, forwarded_args);
    let command = appsec_command(forwarded_args);
    let db_path = crate::paths::core_db(root);
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open {}", db_path.display()))?;
    ensure_schema(&conn)?;
    let now = now_millis();

    let mut projected = json!({
        "version": "ctox.appsec.durable_projection.v1",
        "database": path_json(&db_path),
        "state_dir": path_json(&state_dir),
        "command": command,
        "projected_at": now,
        "counts": {},
    });

    let assessment_count = project_assessments(&conn, &state_dir, &command, output, now)?;
    let run_count = project_runs(&conn, &state_dir, now)?;
    let finding_count = project_findings(&conn, &state_dir, now)?;
    let investigation_count = project_investigations(&conn, &state_dir, now)?;
    let coverage_count = project_coverage(&conn, &state_dir, now)?;
    let pipeline_stage_count = project_pipeline_stages(&conn, &state_dir, now)?;
    let artifact_count = project_artifacts(&conn, &state_dir, output, now)?;
    let inventory_count = project_scanner_inventory(&conn, &state_dir, now)?;
    let approval_count = project_approvals(&conn, &state_dir, now)?;

    projected["counts"] = json!({
        "appsec_assessments": assessment_count,
        "appsec_runs": run_count,
        "appsec_artifacts": artifact_count,
        "appsec_findings": finding_count,
        "appsec_investigations": investigation_count,
        "appsec_coverage": coverage_count,
        "appsec_pipeline_stages": pipeline_stage_count,
        "appsec_scanner_inventory": inventory_count,
        "appsec_approvals": approval_count,
    });
    projected["completion_review"] = completion_review(&conn, &state_dir)?;
    let business_os_projection =
        crate::business_os::store::project_appsec_durable_state_to_business_os(root, &state_dir)
            .context("failed to project AppSec state into Business OS records")?;
    projected["business_os_projection"] = json!({
        "version": "ctox.appsec.business_os_projection.v1",
        "module_id": "appsec-pentest",
        "projected_records": business_os_projection
            .iter()
            .map(|(collection, record_id)| json!({
                "collection": collection,
                "record_id": record_id,
            }))
            .collect::<Vec<_>>(),
        "projected_count": business_os_projection.len(),
    });
    Ok(projected)
}

pub fn handle_state_command(root: &Path, args: &[String]) -> Result<Value> {
    let Some(command_group) = args.first().map(String::as_str) else {
        return Ok(json!({
            "ok": false,
            "error": "usage: ctox appsec state <status|sync> [--state-dir <dir>] [--sync]",
        }));
    };
    let subcommand = state_subcommand(args).unwrap_or("status");
    let state_dir = state_dir_arg(root, args);
    match subcommand {
        "status" => {
            let sync = args.iter().any(|arg| arg == "--sync");
            let projection = if sync {
                Some(project_cli_result(
                    root,
                    &[
                        "ctox-appsec".to_string(),
                        "--state-dir".to_string(),
                        path_json(&state_dir),
                        "state".to_string(),
                        "sync".to_string(),
                    ],
                    &json!({"ok": true, "command": "state sync"}),
                )?)
            } else {
                None
            };
            let mut status = durable_status(root, &state_dir)?;
            if let Some(object) = status.as_object_mut() {
                object.insert("synced".to_string(), json!(sync));
                object.insert("projection".to_string(), projection.unwrap_or(Value::Null));
                object.insert(
                    "command_group".to_string(),
                    json!(format!("appsec {command_group}")),
                );
            }
            Ok(status)
        }
        "sync" => {
            let projection = project_cli_result(
                root,
                &[
                    "ctox-appsec".to_string(),
                    "--state-dir".to_string(),
                    path_json(&state_dir),
                    "state".to_string(),
                    "sync".to_string(),
                ],
                &json!({"ok": true, "command": "state sync"}),
            )?;
            let mut status = durable_status(root, &state_dir)?;
            if let Some(object) = status.as_object_mut() {
                object.insert("synced".to_string(), json!(true));
                object.insert("projection".to_string(), projection);
                object.insert(
                    "command_group".to_string(),
                    json!(format!("appsec {command_group}")),
                );
            }
            Ok(status)
        }
        "help" | "--help" | "-h" => Ok(json!({
            "ok": true,
            "command": "appsec state help",
            "usage": [
                "ctox appsec state status [--state-dir <dir>] [--sync] --json",
                "ctox appsec state sync [--state-dir <dir>] --json"
            ],
            "purpose": "Inspect CTOX durable AppSec projection tables and completion blockers.",
        })),
        other => Ok(json!({
            "ok": false,
            "error": format!("unknown appsec state command `{other}`"),
            "usage": "ctox appsec state <status|sync> [--state-dir <dir>] [--sync]",
        })),
    }
}

fn durable_status(root: &Path, state_dir: &Path) -> Result<Value> {
    let db_path = crate::paths::core_db(root);
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open {}", db_path.display()))?;
    ensure_schema(&conn)?;
    let counts = projected_counts(&conn, state_dir)?;
    let latest_assessment = latest_assessment(&conn, state_dir)?;
    let review = completion_review(&conn, state_dir)?;
    Ok(json!({
        "ok": true,
        "version": "ctox.appsec.durable_status.v1",
        "command": "appsec state status",
        "database": path_json(&db_path),
        "state_dir": path_json(state_dir),
        "counts": counts,
        "latest_assessment": latest_assessment,
        "completion_review": review,
    }))
}

fn projected_counts(conn: &Connection, state_dir: &Path) -> Result<Value> {
    let state = path_json(state_dir);
    Ok(json!({
        "appsec_assessments": count_where(conn, "appsec_assessments", &state)?,
        "appsec_runs": count_where(conn, "appsec_runs", &state)?,
        "appsec_artifacts": count_where(conn, "appsec_artifacts", &state)?,
        "appsec_findings": count_where(conn, "appsec_findings", &state)?,
        "appsec_investigations": count_where(conn, "appsec_investigations", &state)?,
        "appsec_coverage": count_where(conn, "appsec_coverage", &state)?,
        "appsec_pipeline_stages": count_where(conn, "appsec_pipeline_stages", &state)?,
        "appsec_scanner_inventory": count_where(conn, "appsec_scanner_inventory", &state)?,
        "appsec_approvals": count_where(conn, "appsec_approvals", &state)?,
    }))
}

fn count_where(conn: &Connection, table: &str, state_dir: &str) -> Result<i64> {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE state_dir = ?1"),
        params![state_dir],
        |row| row.get(0),
    )
    .with_context(|| format!("failed to count {table}"))
}

fn latest_assessment(conn: &Connection, state_dir: &Path) -> Result<Value> {
    let state = path_json(state_dir);
    let mut stmt = conn.prepare(
        "SELECT assessment_id, target, profile, status, artifact_path, updated_at
         FROM appsec_assessments
         WHERE state_dir = ?1
         ORDER BY CAST(updated_at AS INTEGER) DESC
         LIMIT 1",
    )?;
    let mut rows = stmt.query(params![state])?;
    if let Some(row) = rows.next()? {
        Ok(json!({
            "assessment_id": row.get::<_, String>(0)?,
            "target": row.get::<_, Option<String>>(1)?,
            "profile": row.get::<_, Option<String>>(2)?,
            "status": row.get::<_, String>(3)?,
            "artifact_path": row.get::<_, Option<String>>(4)?,
            "updated_at": row.get::<_, String>(5)?,
        }))
    } else {
        Ok(Value::Null)
    }
}

fn completion_review(conn: &Connection, state_dir: &Path) -> Result<Value> {
    let state = path_json(state_dir);
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    let assessment_count = count_where(conn, "appsec_assessments", &state)?;
    if assessment_count == 0 {
        blockers.push(json!({
            "kind": "assessment-missing",
            "message": "No AppSec assessment has been projected into CTOX durable state.",
        }));
    }

    let latest = latest_assessment(conn, state_dir)?;
    if let Some(status) = latest.get("status").and_then(Value::as_str) {
        if !matches!(status, "complete" | "completed") {
            blockers.push(json!({
                "kind": "assessment-incomplete",
                "status": status,
                "message": "Latest assessment is not complete.",
            }));
        }
    }

    let coverage_blockers = query_rows_json(
        conn,
        "SELECT coverage_id, phase, target, status FROM appsec_coverage
         WHERE state_dir = ?1 AND status NOT IN ('completed', 'not-applicable')
         ORDER BY coverage_id",
        &state,
        |row| {
            Ok(json!({
                "coverage_id": row.get::<_, String>(0)?,
                "phase": row.get::<_, Option<String>>(1)?,
                "target": row.get::<_, Option<String>>(2)?,
                "status": row.get::<_, String>(3)?,
            }))
        },
    )?;
    if !coverage_blockers.is_empty() {
        blockers.push(json!({
            "kind": "coverage-incomplete",
            "count": coverage_blockers.len(),
            "items": coverage_blockers,
            "message": "Coverage contains workstreams that are not completed or explicitly not-applicable.",
        }));
    }

    let pipeline_blockers = query_rows_json(
        conn,
        "SELECT stage_id, phase, target, status, queue_task_id, queue_status
         FROM appsec_pipeline_stages
         WHERE state_dir = ?1
           AND (
                status NOT IN ('completed', 'not-applicable')
                OR queue_status IN ('pending', 'leased', 'failed', 'review_rework')
           )
         ORDER BY stage_id",
        &state,
        |row| {
            Ok(json!({
                "stage_id": row.get::<_, String>(0)?,
                "phase": row.get::<_, Option<String>>(1)?,
                "target": row.get::<_, Option<String>>(2)?,
                "status": row.get::<_, String>(3)?,
                "queue_task_id": row.get::<_, Option<String>>(4)?,
                "queue_status": row.get::<_, Option<String>>(5)?,
            }))
        },
    )?;
    if !pipeline_blockers.is_empty() {
        blockers.push(json!({
            "kind": "pipeline-stage-incomplete",
            "count": pipeline_blockers.len(),
            "items": pipeline_blockers,
            "message": "Pipeline stages are not completed/not-applicable or still have active/retry queue work; service workers must write stage evidence back before closure.",
        }));
    }

    let run_blockers = query_rows_json(
        conn,
        "SELECT run_id, tool, target, status FROM appsec_runs
         WHERE state_dir = ?1
           AND (status LIKE 'blocked-%' OR status IN ('unavailable', 'failed', 'dry-run'))
         ORDER BY run_id",
        &state,
        |row| {
            Ok(json!({
                "run_id": row.get::<_, String>(0)?,
                "tool": row.get::<_, Option<String>>(1)?,
                "target": row.get::<_, Option<String>>(2)?,
                "status": row.get::<_, String>(3)?,
            }))
        },
    )?;
    if !run_blockers.is_empty() {
        blockers.push(json!({
            "kind": "run-blocked-or-not-executed",
            "count": run_blockers.len(),
            "items": run_blockers,
            "message": "One or more scanner/tool runs are blocked, unavailable, failed, timed out, or only dry-run evidence.",
        }));
    }

    let latest_profile = latest
        .get("profile")
        .and_then(Value::as_str)
        .map(str::to_string);
    let unavailable_scanners =
        unavailable_scanner_blockers(conn, &state, latest_profile.as_deref())?;
    if !unavailable_scanners.is_empty() {
        blockers.push(json!({
            "kind": "scanner-unavailable",
            "count": unavailable_scanners.len(),
            "items": unavailable_scanners,
            "message": "Required scanner inventory includes unavailable scanners; a complete assessment cannot be claimed.",
        }));
    }

    let unresolved_findings = query_rows_json(
        conn,
        "SELECT finding_id, title, severity, category, status, target FROM appsec_findings
         WHERE state_dir = ?1
           AND status NOT IN ('validated', 'fixed', 'resolved', 'false-positive', 'accepted-risk')
         ORDER BY finding_id",
        &state,
        |row| {
            Ok(json!({
                "finding_id": row.get::<_, String>(0)?,
                "title": row.get::<_, Option<String>>(1)?,
                "severity": row.get::<_, Option<String>>(2)?,
                "category": row.get::<_, Option<String>>(3)?,
                "status": row.get::<_, String>(4)?,
                "target": row.get::<_, Option<String>>(5)?,
            }))
        },
    )?;
    if !unresolved_findings.is_empty() {
        warnings.push(json!({
            "kind": "finding-not-validated",
            "count": unresolved_findings.len(),
            "items": unresolved_findings,
            "message": "Findings remain candidate/unresolved and must not be presented as validated vulnerabilities.",
        }));
    }

    let artifact_count = count_where(conn, "appsec_artifacts", &state)?;
    if artifact_count == 0 {
        blockers.push(json!({
            "kind": "artifact-missing",
            "message": "No AppSec artifacts with checksums are projected into durable state.",
        }));
    }

    Ok(json!({
        "closable": blockers.is_empty(),
        "blocker_count": blockers.len(),
        "warning_count": warnings.len(),
        "blockers": blockers,
        "warnings": warnings,
        "policy": "Do not approve or claim a complete AppSec pentest while blockers remain. Candidate findings require separate validation before being reported as validated vulnerabilities.",
    }))
}

fn unavailable_scanner_blockers(
    conn: &Connection,
    state: &str,
    assessment_profile: Option<&str>,
) -> Result<Vec<Value>> {
    let rows = query_rows_json(
        conn,
        "SELECT scanner_id, profile, status, binary_path, payload_json FROM appsec_scanner_inventory
         WHERE state_dir = ?1 AND available = 0
         ORDER BY scanner_id",
        state,
        |row| {
            Ok(json!({
                "scanner_id": row.get::<_, String>(0)?,
                "profile": row.get::<_, Option<String>>(1)?,
                "status": row.get::<_, String>(2)?,
                "binary_path": row.get::<_, Option<String>>(3)?,
                "payload": serde_json::from_str::<Value>(&row.get::<_, String>(4)?)
                    .unwrap_or(Value::Null),
            }))
        },
    )?;
    let assessment_rank = assessment_profile.and_then(profile_rank);
    Ok(rows
        .into_iter()
        .filter_map(|mut item| {
            let payload = item.get("payload").cloned().unwrap_or(Value::Null);
            let scanner_profile = item
                .get("profile")
                .and_then(Value::as_str)
                .or_else(|| payload.get("profile").and_then(Value::as_str));
            let default_enabled = payload
                .get("default_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let required = if let Some(rank) = assessment_rank {
                scanner_profile
                    .and_then(profile_rank)
                    .map(|scanner_rank| scanner_rank <= rank)
                    .unwrap_or(default_enabled)
            } else {
                default_enabled
            };
            if required {
                if let Some(object) = item.as_object_mut() {
                    object.remove("payload");
                }
                Some(item)
            } else {
                None
            }
        })
        .collect())
}

fn profile_rank(profile: &str) -> Option<u8> {
    match profile {
        "minimal" => Some(0),
        "standard" => Some(1),
        "full" => Some(2),
        _ => None,
    }
}

fn query_rows_json<F>(
    conn: &Connection,
    sql: &str,
    state_dir: &str,
    mut mapper: F,
) -> Result<Vec<Value>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<Value>,
{
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![state_dir], |row| mapper(row))?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS appsec_assessments (
            assessment_id TEXT PRIMARY KEY,
            state_dir TEXT NOT NULL,
            target TEXT,
            profile TEXT,
            status TEXT NOT NULL,
            command TEXT NOT NULL,
            artifact_path TEXT,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS appsec_runs (
            run_id TEXT PRIMARY KEY,
            assessment_id TEXT,
            state_dir TEXT NOT NULL,
            tool TEXT,
            target TEXT,
            status TEXT NOT NULL,
            command_json TEXT,
            artifact_path TEXT,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS appsec_artifacts (
            artifact_path TEXT PRIMARY KEY,
            state_dir TEXT NOT NULL,
            kind TEXT NOT NULL,
            version TEXT,
            sha256 TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            metadata_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS appsec_findings (
            finding_id TEXT PRIMARY KEY,
            state_dir TEXT NOT NULL,
            title TEXT,
            severity TEXT,
            category TEXT,
            status TEXT NOT NULL,
            target TEXT,
            evidence_artifact TEXT,
            payload_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS appsec_investigations (
            investigation_key TEXT PRIMARY KEY,
            investigation_id TEXT NOT NULL,
            candidate_id TEXT NOT NULL,
            state_dir TEXT NOT NULL,
            status TEXT NOT NULL,
            outcome TEXT,
            hypothesis TEXT,
            expected_signal TEXT,
            falsification_criterion TEXT,
            evidence_artifact TEXT,
            graph_sha256 TEXT,
            payload_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS appsec_coverage (
            coverage_id TEXT PRIMARY KEY,
            state_dir TEXT NOT NULL,
            phase TEXT,
            target TEXT,
            status TEXT NOT NULL,
            artifact_path TEXT,
            payload_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS appsec_pipeline_stages (
            stage_key TEXT PRIMARY KEY,
            state_dir TEXT NOT NULL,
            stage_id TEXT NOT NULL,
            phase TEXT,
            target TEXT,
            status TEXT NOT NULL,
            coverage_status TEXT,
            active_required INTEGER NOT NULL DEFAULT 0,
            queue_task_id TEXT,
            queue_status TEXT,
            queue_updated_at TEXT,
            payload_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS appsec_scanner_inventory (
            scanner_id TEXT PRIMARY KEY,
            state_dir TEXT NOT NULL,
            profile TEXT,
            available INTEGER NOT NULL,
            status TEXT NOT NULL,
            binary_path TEXT,
            detected_version TEXT,
            payload_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS appsec_approvals (
            approval_id TEXT PRIMARY KEY,
            state_dir TEXT NOT NULL,
            status TEXT NOT NULL,
            target_kind TEXT,
            target TEXT,
            tools_json TEXT,
            expires_at TEXT,
            payload_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_appsec_runs_state_status
            ON appsec_runs(state_dir, status);
        CREATE INDEX IF NOT EXISTS idx_appsec_findings_state_status
            ON appsec_findings(state_dir, status);
        CREATE INDEX IF NOT EXISTS idx_appsec_investigations_state_status
            ON appsec_investigations(state_dir, status);
        CREATE INDEX IF NOT EXISTS idx_appsec_investigations_candidate
            ON appsec_investigations(candidate_id);
        CREATE INDEX IF NOT EXISTS idx_appsec_investigations_id
            ON appsec_investigations(investigation_id);
        CREATE INDEX IF NOT EXISTS idx_appsec_coverage_state_status
            ON appsec_coverage(state_dir, status);
        CREATE INDEX IF NOT EXISTS idx_appsec_pipeline_stages_state_status
            ON appsec_pipeline_stages(state_dir, status);
        CREATE INDEX IF NOT EXISTS idx_appsec_pipeline_stages_queue
            ON appsec_pipeline_stages(queue_task_id);
        "#,
    )
    .context("failed to initialize AppSec durable-state tables")?;
    Ok(())
}

fn project_assessments(
    conn: &Connection,
    state_dir: &Path,
    command: &str,
    output: &Value,
    now: u128,
) -> Result<usize> {
    let mut count = 0;
    let assessment_files = list_json_files(&state_dir.join("assessments"))?;
    for file in assessment_files {
        let payload = read_json(&file)?;
        let id = artifact_stem_id(&file, "assessment");
        upsert_assessment(conn, state_dir, &id, command, Some(&file), &payload, now)?;
        count += 1;
    }

    if count == 0
        || output.get("version").and_then(Value::as_str)
            == Some("ctox.appsec_pentest.assessment.v1")
    {
        let id = output
            .get("artifact")
            .and_then(Value::as_str)
            .map(|path| artifact_stem_id(Path::new(path), "assessment"))
            .unwrap_or_else(|| {
                stable_id(
                    "assessment",
                    &format!("{}:{}", path_json(state_dir), command),
                )
            });
        let artifact = output
            .get("artifact")
            .and_then(Value::as_str)
            .map(PathBuf::from);
        upsert_assessment(
            conn,
            state_dir,
            &id,
            command,
            artifact.as_deref(),
            output,
            now,
        )?;
        count += 1;
    }
    Ok(count)
}

fn upsert_assessment(
    conn: &Connection,
    state_dir: &Path,
    id: &str,
    command: &str,
    artifact: Option<&Path>,
    payload: &Value,
    now: u128,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO appsec_assessments
            (assessment_id, state_dir, target, profile, status, command, artifact_path, payload_json, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
        ON CONFLICT(assessment_id) DO UPDATE SET
            state_dir=excluded.state_dir,
            target=excluded.target,
            profile=excluded.profile,
            status=excluded.status,
            command=excluded.command,
            artifact_path=excluded.artifact_path,
            payload_json=excluded.payload_json,
            updated_at=excluded.updated_at
        "#,
        params![
            id,
            path_json(state_dir),
            payload.get("target").and_then(Value::as_str),
            payload.get("profile").and_then(Value::as_str),
            appsec_status(payload),
            command,
            artifact.map(path_json),
            compact_json(payload),
            now.to_string(),
        ],
    )?;
    Ok(())
}

fn project_runs(conn: &Connection, state_dir: &Path, now: u128) -> Result<usize> {
    let mut count = 0;
    for file in list_run_record_files(state_dir)? {
        let payload = read_json(&file)?;
        let id = payload
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| artifact_stem_id(&file, "run"));
        conn.execute(
            r#"
            INSERT INTO appsec_runs
                (run_id, assessment_id, state_dir, tool, target, status, command_json, artifact_path, payload_json, created_at, updated_at)
            VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
            ON CONFLICT(run_id) DO UPDATE SET
                state_dir=excluded.state_dir,
                tool=excluded.tool,
                target=excluded.target,
                status=excluded.status,
                command_json=excluded.command_json,
                artifact_path=excluded.artifact_path,
                payload_json=excluded.payload_json,
                updated_at=excluded.updated_at
            "#,
            params![
                id,
                path_json(state_dir),
                payload.get("tool").and_then(Value::as_str),
                payload.get("target").and_then(Value::as_str),
                payload
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown"),
                payload.get("command").map(compact_json),
                path_json(&file),
                compact_json(&payload),
                now.to_string(),
            ],
        )?;
        count += 1;
    }
    Ok(count)
}

fn project_artifacts(
    conn: &Connection,
    state_dir: &Path,
    output: &Value,
    now: u128,
) -> Result<usize> {
    let mut paths = list_json_files_recursive(state_dir)?;
    paths.extend(list_issue_bundle_files(state_dir)?);
    collect_output_artifact_paths(output, &mut paths);
    paths.sort();
    paths.dedup();
    let mut count = 0;
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let bytes =
            fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        let sha256 = hex_sha256(&bytes);
        let version = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|value| {
                value
                    .get("version")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        let metadata = json!({
            "relative_path": relative_path(state_dir, &path),
            "source": "ctox-appsec-state",
        });
        conn.execute(
            r#"
            INSERT INTO appsec_artifacts
                (artifact_path, state_dir, kind, version, sha256, size_bytes, metadata_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(artifact_path) DO UPDATE SET
                state_dir=excluded.state_dir,
                kind=excluded.kind,
                version=excluded.version,
                sha256=excluded.sha256,
                size_bytes=excluded.size_bytes,
                metadata_json=excluded.metadata_json,
                updated_at=excluded.updated_at
            "#,
            params![
                path_json(&path),
                path_json(state_dir),
                artifact_kind(state_dir, &path),
                version,
                sha256,
                bytes.len() as i64,
                compact_json(&metadata),
                now.to_string(),
            ],
        )?;
        count += 1;
    }
    Ok(count)
}

fn list_issue_bundle_files(state_dir: &Path) -> Result<Vec<PathBuf>> {
    let root = state_dir.join("reports").join("issue-bundles");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    const ALLOWED: &[&str] = &[
        "reproduce.py",
        "github-issue.md",
        "README.md",
        "requirements.txt",
    ];
    let mut files = Vec::new();
    for entry in
        fs::read_dir(&root).with_context(|| format!("failed to read {}", root.display()))?
    {
        let entry = entry?;
        let bundle = entry.path();
        if !bundle.is_dir() {
            continue;
        }
        for name in ALLOWED {
            let path = bundle.join(name);
            if path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn project_findings(conn: &Connection, state_dir: &Path, now: u128) -> Result<usize> {
    let path = state_dir.join("findings.json");
    let findings = read_json_optional(&path)?;
    let items = findings
        .as_ref()
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for item in &items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| stable_id("finding", &compact_json(item)));
        conn.execute(
            r#"
            INSERT INTO appsec_findings
                (finding_id, state_dir, title, severity, category, status, target, evidence_artifact, payload_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(finding_id) DO UPDATE SET
                state_dir=excluded.state_dir,
                title=excluded.title,
                severity=excluded.severity,
                category=excluded.category,
                status=excluded.status,
                target=excluded.target,
                evidence_artifact=excluded.evidence_artifact,
                payload_json=excluded.payload_json,
                updated_at=excluded.updated_at
            "#,
            params![
                id,
                path_json(state_dir),
                item.get("title").and_then(Value::as_str),
                item.get("severity").and_then(Value::as_str),
                item.get("category").and_then(Value::as_str),
                item.get("status").and_then(Value::as_str).unwrap_or("candidate"),
                item.get("target").and_then(Value::as_str),
                item.get("evidence_artifact")
                    .or_else(|| item.get("artifact"))
                    .and_then(Value::as_str),
                compact_json(item),
                now.to_string(),
            ],
        )?;
    }
    Ok(items.len())
}

fn project_investigations(conn: &Connection, state_dir: &Path, now: u128) -> Result<usize> {
    let path = state_dir.join("investigations.json");
    let investigations = read_json_optional(&path)?;
    let items = investigations
        .as_ref()
        .and_then(|value| value.get("investigations"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for item in &items {
        let candidate_id = item
            .get("candidate_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                stable_id(
                    "investigation",
                    &format!("{}:{candidate_id}", path_json(state_dir)),
                )
            });
        let key = stable_id("investigation", &format!("{}:{id}", path_json(state_dir)));
        let evidence_artifact = item
            .pointer("/resolution/evidence/artifact")
            .or_else(|| item.pointer("/execution/artifact"))
            .and_then(Value::as_str);
        conn.execute(
            r#"
            INSERT INTO appsec_investigations
                (investigation_key, investigation_id, candidate_id, state_dir, status, outcome, hypothesis,
                 expected_signal, falsification_criterion, evidence_artifact, graph_sha256,
                 payload_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(investigation_key) DO UPDATE SET
                investigation_id=excluded.investigation_id,
                candidate_id=excluded.candidate_id,
                state_dir=excluded.state_dir,
                status=excluded.status,
                outcome=excluded.outcome,
                hypothesis=excluded.hypothesis,
                expected_signal=excluded.expected_signal,
                falsification_criterion=excluded.falsification_criterion,
                evidence_artifact=excluded.evidence_artifact,
                graph_sha256=excluded.graph_sha256,
                payload_json=excluded.payload_json,
                updated_at=excluded.updated_at
            "#,
            params![
                key,
                id,
                candidate_id,
                path_json(state_dir),
                item.get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("planned"),
                item.pointer("/resolution/outcome").and_then(Value::as_str),
                item.get("hypothesis").and_then(Value::as_str),
                item.get("expected_signal").and_then(Value::as_str),
                item.get("falsification_criterion").and_then(Value::as_str),
                evidence_artifact,
                item.pointer("/refutation/graph_sha256")
                    .and_then(Value::as_str),
                compact_json(item),
                now.to_string(),
            ],
        )?;
    }
    Ok(items.len())
}

fn project_coverage(conn: &Connection, state_dir: &Path, now: u128) -> Result<usize> {
    let path = state_dir.join("coverage.json");
    let coverage = read_json_optional(&path)?;
    let items = coverage
        .as_ref()
        .and_then(|value| value.get("workstreams"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for (index, item) in items.iter().enumerate() {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("coverage-{}", index + 1));
        conn.execute(
            r#"
            INSERT INTO appsec_coverage
                (coverage_id, state_dir, phase, target, status, artifact_path, payload_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(coverage_id) DO UPDATE SET
                state_dir=excluded.state_dir,
                phase=excluded.phase,
                target=excluded.target,
                status=excluded.status,
                artifact_path=excluded.artifact_path,
                payload_json=excluded.payload_json,
                updated_at=excluded.updated_at
            "#,
            params![
                id,
                path_json(state_dir),
                item.get("phase").and_then(Value::as_str),
                item.get("target").and_then(Value::as_str),
                item.get("status").and_then(Value::as_str).unwrap_or("unknown"),
                path_json(&path),
                compact_json(item),
                now.to_string(),
            ],
        )?;
    }
    Ok(items.len())
}

fn project_pipeline_stages(conn: &Connection, state_dir: &Path, now: u128) -> Result<usize> {
    let status_path = state_dir.join("assessment-pipeline-status.json");
    let queue_path = state_dir.join("assessment-pipeline-queue.json");
    let pipeline_path = state_dir.join("assessment-pipeline.json");
    let status = read_json_optional(&status_path)?;
    let queue_spec = read_json_optional(&queue_path)?;
    let pipeline = read_json_optional(&pipeline_path)?;
    let stages = status
        .as_ref()
        .and_then(|value| value.get("stages"))
        .or_else(|| pipeline.as_ref().and_then(|value| value.get("stages")))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut count = 0;
    let mut writeback_stages = Vec::new();
    for (index, stage) in stages.iter().enumerate() {
        let stage_id = stage
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                format!(
                    "stage-{}-{}",
                    stage
                        .get("order")
                        .and_then(Value::as_u64)
                        .unwrap_or((index + 1) as u64),
                    stage
                        .get("phase")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                )
            });
        let stage_key = stable_id(
            "pipeline-stage",
            &format!("{}:{stage_id}", path_json(state_dir)),
        );
        let queue = pipeline_queue_task_for_stage(conn, state_dir, &stage_id, queue_spec.as_ref())?;
        let queue_task_id = queue
            .as_ref()
            .and_then(|item| item.get("message_key"))
            .and_then(Value::as_str);
        let queue_status = queue
            .as_ref()
            .and_then(|item| item.get("route_status"))
            .and_then(Value::as_str);
        let queue_updated_at = queue
            .as_ref()
            .and_then(|item| item.get("updated_at"))
            .and_then(Value::as_str);
        let phase = stage.get("phase").and_then(Value::as_str);
        let target = stage.get("target").and_then(Value::as_str);
        let status_value = stage
            .get("status")
            .and_then(Value::as_str)
            .or_else(|| stage.get("coverage_status").and_then(Value::as_str))
            .unwrap_or("planned");
        let coverage_status = stage.get("coverage_status").and_then(Value::as_str);
        let run_evidence = pipeline_stage_run_evidence(state_dir, stage)?;
        let status_writeback = derive_pipeline_stage_writeback_status(
            status_value,
            queue_status,
            run_evidence.as_ref(),
        );
        let projected_status = status_writeback
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or(status_value);
        let active_required = stage
            .get("active_required")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let stage_kind = pipeline_stage_kind(stage, phase);
        let origin_id = stage
            .get("origin_id")
            .or_else(|| stage.get("investigation_id"))
            .or_else(|| stage.get("candidate_id"))
            .or_else(|| stage.get("finding_id"))
            .and_then(Value::as_str)
            .unwrap_or(&stage_id);
        let required = stage
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let evidence_status = status_writeback
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or(status_value);
        let mut payload = stage.clone();
        if let Some(object) = payload.as_object_mut() {
            object.insert("stage_kind".to_string(), json!(stage_kind));
            object.insert("origin_id".to_string(), json!(origin_id));
            object.insert("required".to_string(), json!(required));
            object.insert("evidence_status".to_string(), json!(evidence_status));
            object.insert("status_writeback".to_string(), status_writeback.clone());
            if let Some(queue) = queue.as_ref() {
                object.insert("queue_task".to_string(), queue.clone());
            }
            if let Some(run_evidence) = run_evidence.as_ref() {
                object.insert("run_evidence".to_string(), run_evidence.clone());
            }
        }
        writeback_stages.push(json!({
            "id": stage_id.clone(),
            "phase": phase,
            "target": target,
            "base_status": status_value,
            "status": projected_status,
            "coverage_status": coverage_status,
            "stage_kind": stage_kind,
            "origin_id": origin_id,
            "required": required,
            "evidence_status": evidence_status,
            "queue_task": queue,
            "run_evidence": run_evidence,
            "writeback": status_writeback,
        }));
        conn.execute(
            r#"
            INSERT INTO appsec_pipeline_stages
                (stage_key, state_dir, stage_id, phase, target, status, coverage_status, active_required, queue_task_id, queue_status, queue_updated_at, payload_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(stage_key) DO UPDATE SET
                state_dir=excluded.state_dir,
                stage_id=excluded.stage_id,
                phase=excluded.phase,
                target=excluded.target,
                status=excluded.status,
                coverage_status=excluded.coverage_status,
                active_required=excluded.active_required,
                queue_task_id=excluded.queue_task_id,
                queue_status=excluded.queue_status,
                queue_updated_at=excluded.queue_updated_at,
                payload_json=excluded.payload_json,
                updated_at=excluded.updated_at
            "#,
            params![
                stage_key,
                path_json(state_dir),
                stage_id,
                phase,
                target,
                projected_status,
                coverage_status,
                if active_required { 1 } else { 0 },
                queue_task_id,
                queue_status,
                queue_updated_at,
                compact_json(&payload),
                now.to_string(),
            ],
        )?;
        count += 1;
    }
    if !writeback_stages.is_empty() {
        write_pipeline_writeback(state_dir, writeback_stages, now)?;
    }
    Ok(count)
}

fn pipeline_stage_kind(stage: &Value, phase: Option<&str>) -> &'static str {
    if let Some(kind) = stage.get("stage_kind").and_then(Value::as_str) {
        return match kind {
            "investigation" => "investigation",
            "refutation" => "refutation",
            "retest" => "retest",
            _ => "baseline",
        };
    }
    let phase = phase.unwrap_or_default().to_ascii_lowercase();
    if phase.contains("refut") {
        "refutation"
    } else if phase.contains("retest") || phase.contains("fix-check") {
        "retest"
    } else if phase.contains("investig") {
        "investigation"
    } else {
        "baseline"
    }
}

fn write_pipeline_writeback(state_dir: &Path, stages: Vec<Value>, now: u128) -> Result<()> {
    let mut status_counts: BTreeMap<String, usize> = BTreeMap::new();
    for status in stages
        .iter()
        .filter_map(|stage| stage.get("status").and_then(Value::as_str))
    {
        *status_counts.entry(status.to_string()).or_insert(0) += 1;
    }
    let completed = status_counts.get("completed").copied().unwrap_or(0)
        + status_counts.get("not-applicable").copied().unwrap_or(0);
    let writeback = json!({
        "version": "ctox.appsec_pentest.assessment_pipeline_writeback.v1",
        "generated_at": now.to_string(),
        "source": "ctox-core-appsec-state",
        "summary": {
            "stages": stages.len(),
            "completed_or_not_applicable": completed,
            "status_counts": status_counts,
            "closable": !stages.is_empty() && completed == stages.len(),
        },
        "stages": stages,
    });
    let path = state_dir.join("assessment-pipeline-writeback.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(&writeback)?;
    fs::write(&path, format!("{content}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn derive_pipeline_stage_writeback_status(
    base_status: &str,
    queue_status: Option<&str>,
    run_evidence: Option<&Value>,
) -> Value {
    let run_status = run_evidence.and_then(derive_status_from_run_evidence);
    let (status, reason) = if matches!(base_status, "completed" | "not-applicable") {
        (base_status.to_string(), "coverage already terminal")
    } else {
        match queue_status {
            Some("failed") => ("blocked-queue-failed".to_string(), "queue task failed"),
            Some("review_rework") => (
                "blocked-queue-review-rework".to_string(),
                "queue task requires review rework",
            ),
            Some("cancelled" | "superseded") => (
                "blocked-queue-cancelled".to_string(),
                "queue task was cancelled or superseded",
            ),
            Some("blocked") => (
                "blocked-queue-blocked".to_string(),
                "queue task is blocked pending external evidence or operator action",
            ),
            Some("leased" | "running") => {
                ("in-progress".to_string(), "queue task is leased or running")
            }
            Some("pending") => ("queued".to_string(), "queue task is pending"),
            Some("handled") => run_status.unwrap_or((
                "blocked-queue-handled-missing-evidence".to_string(),
                "queue task is handled but no matching run or coverage evidence was found",
            )),
            _ => run_status.unwrap_or((base_status.to_string(), "pipeline status unchanged")),
        }
    };
    json!({
        "status": status,
        "base_status": base_status,
        "queue_status": queue_status,
        "reason": reason,
    })
}

fn derive_status_from_run_evidence(run_evidence: &Value) -> Option<(String, &'static str)> {
    let status_counts = run_evidence.get("status_counts")?;
    for status in [
        "blocked-tool-timeout",
        "blocked-tool-failed",
        "blocked-tool-missing",
        "blocked-active-approval-required",
        "blocked-scope-denied",
        "dry-run",
        "skipped",
    ] {
        if status_counts
            .get(status)
            .and_then(Value::as_u64)
            .is_some_and(|count| count > 0)
        {
            return Some((
                status.to_string(),
                "matching run evidence contains a blocking status",
            ));
        }
    }
    if status_counts
        .get("completed")
        .and_then(Value::as_u64)
        .is_some_and(|count| count > 0)
    {
        return Some((
            "attempted".to_string(),
            "matching run evidence completed but coverage is not terminal",
        ));
    }
    None
}

fn pipeline_stage_run_evidence(state_dir: &Path, stage: &Value) -> Result<Option<Value>> {
    let tools = stage
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    if tools.is_empty() {
        return Ok(None);
    }
    let target = stage.get("target").and_then(Value::as_str).unwrap_or("");
    let mut matches = Vec::new();
    let mut status_counts: BTreeMap<String, usize> = BTreeMap::new();
    for file in list_run_record_files(state_dir)? {
        let payload = read_json(&file)?;
        let Some(tool) = payload.get("tool").and_then(Value::as_str) else {
            continue;
        };
        if !tools.contains(&tool) {
            continue;
        }
        let inferred_target = run_payload_target(&payload);
        if !target_matches_run(target, inferred_target.as_deref()) {
            continue;
        }
        let status = payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        *status_counts.entry(status.to_string()).or_insert(0) += 1;
        matches.push(json!({
            "run_id": payload.get("id").cloned().unwrap_or_else(|| json!(artifact_stem_id(&file, "run"))),
            "tool": tool,
            "status": status,
            "target": inferred_target,
            "artifact_path": path_json(&file),
        }));
    }
    if matches.is_empty() {
        return Ok(None);
    }
    Ok(Some(json!({
        "matched_runs": matches,
        "status_counts": status_counts,
    })))
}

fn run_payload_target(payload: &Value) -> Option<String> {
    payload
        .get("target")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| command_arg_target(payload.get("command")))
}

fn command_arg_target(command: Option<&Value>) -> Option<String> {
    let args = command?.as_array()?;
    for (index, arg) in args.iter().enumerate() {
        let Some(text) = arg.as_str() else {
            continue;
        };
        if matches!(
            text,
            "--target" | "--url" | "--host" | "--source" | "--path" | "-u" | "-s" | "-host" | "-d"
        ) {
            return args
                .get(index + 1)
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }
    None
}

fn target_matches_run(stage_target: &str, run_target: Option<&str>) -> bool {
    if stage_target.trim().is_empty() {
        return true;
    }
    let Some(run_target) = run_target.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    run_target == stage_target
        || run_target.starts_with(stage_target)
        || stage_target.starts_with(run_target)
}

fn pipeline_queue_task_for_stage(
    conn: &Connection,
    state_dir: &Path,
    stage_id: &str,
    queue_spec: Option<&Value>,
) -> Result<Option<Value>> {
    if queue_tables_exist(conn)? {
        if let Some(queue) = query_pipeline_queue_task(conn, state_dir, stage_id)? {
            return Ok(Some(queue));
        }
    }
    Ok(queue_spec.and_then(|spec| {
        spec.get("queue_tasks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|task| task.get("stage_id").and_then(Value::as_str) == Some(stage_id))
            .map(|task| {
                json!({
                    "message_key": task.get("message_key").cloned().unwrap_or(Value::Null),
                    "route_status": task.get("route_status").cloned().unwrap_or(Value::Null),
                    "updated_at": Value::Null,
                    "source": "assessment-pipeline-queue.json",
                    "spec": task,
                })
            })
    }))
}

fn queue_tables_exist(conn: &Connection) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table'
           AND name IN ('communication_messages', 'communication_routing_state')",
        [],
        |row| row.get(0),
    )?;
    Ok(count == 2)
}

fn query_pipeline_queue_task(
    conn: &Connection,
    state_dir: &Path,
    stage_id: &str,
) -> Result<Option<Value>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            m.message_key,
            COALESCE(r.route_status, 'pending') AS route_status,
            r.updated_at,
            m.thread_key,
            m.subject,
            m.metadata_json
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = 'queue'
          AND m.direction = 'inbound'
          AND json_extract(m.metadata_json, '$.appsec_state_dir') = ?1
          AND json_extract(m.metadata_json, '$.stage.id') = ?2
        ORDER BY
            CASE COALESCE(r.route_status, 'pending')
                WHEN 'pending' THEN 0
                WHEN 'leased' THEN 1
                WHEN 'review_rework' THEN 2
                WHEN 'failed' THEN 3
                WHEN 'handled' THEN 4
                ELSE 5
            END,
            COALESCE(r.updated_at, m.observed_at) DESC
        LIMIT 1
        "#,
    )?;
    let row = stmt
        .query_row(params![path_json(state_dir), stage_id], |row| {
            let metadata_raw: String = row.get(5)?;
            Ok(json!({
                "message_key": row.get::<_, String>(0)?,
                "route_status": row.get::<_, String>(1)?,
                "updated_at": row.get::<_, Option<String>>(2)?,
                "thread_key": row.get::<_, Option<String>>(3)?,
                "title": row.get::<_, Option<String>>(4)?,
                "metadata": serde_json::from_str::<Value>(&metadata_raw).unwrap_or(Value::Null),
                "source": "ctox-queue",
            }))
        })
        .optional()?;
    Ok(row)
}

fn project_scanner_inventory(conn: &Connection, state_dir: &Path, now: u128) -> Result<usize> {
    let path = state_dir.join("tool-inventory.json");
    let inventory = read_json_optional(&path)?;
    let inventory_profile = inventory
        .as_ref()
        .and_then(|value| value.get("profile"))
        .and_then(Value::as_str);
    let items = inventory
        .as_ref()
        .and_then(|value| value.get("tools"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    conn.execute(
        "DELETE FROM appsec_scanner_inventory WHERE state_dir = ?1",
        params![path_json(state_dir)],
    )?;
    for item in &items {
        let id = item
            .get("id")
            .or_else(|| item.get("tool"))
            .or_else(|| item.get("name"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| stable_id("scanner", &compact_json(item)));
        let available = item
            .get("available")
            .and_then(Value::as_bool)
            .or_else(|| {
                item.get("status")
                    .and_then(Value::as_str)
                    .map(|s| s == "available")
            })
            .unwrap_or(false);
        conn.execute(
            r#"
            INSERT INTO appsec_scanner_inventory
                (scanner_id, state_dir, profile, available, status, binary_path, detected_version, payload_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(scanner_id) DO UPDATE SET
                state_dir=excluded.state_dir,
                profile=excluded.profile,
                available=excluded.available,
                status=excluded.status,
                binary_path=excluded.binary_path,
                detected_version=excluded.detected_version,
                payload_json=excluded.payload_json,
                updated_at=excluded.updated_at
            "#,
            params![
                id,
                path_json(state_dir),
                item.get("profile")
                    .and_then(Value::as_str)
                    .or(inventory_profile),
                if available { 1 } else { 0 },
                item.get("status")
                    .and_then(Value::as_str)
                    .unwrap_or(if available { "available" } else { "missing" }),
                item.get("path")
                    .or_else(|| item.get("binary_path"))
                    .and_then(Value::as_str),
                item.get("detected_version").and_then(Value::as_str),
                compact_json(item),
                now.to_string(),
            ],
        )?;
    }
    Ok(items.len())
}

fn project_approvals(conn: &Connection, state_dir: &Path, now: u128) -> Result<usize> {
    let path = state_dir.join("approvals.json");
    let approvals = read_json_optional(&path)?;
    let items = approvals
        .as_ref()
        .and_then(|value| value.get("approvals"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for item in &items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| stable_id("approval", &compact_json(item)));
        conn.execute(
            r#"
            INSERT INTO appsec_approvals
                (approval_id, state_dir, status, target_kind, target, tools_json, expires_at, payload_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(approval_id) DO UPDATE SET
                state_dir=excluded.state_dir,
                status=excluded.status,
                target_kind=excluded.target_kind,
                target=excluded.target,
                tools_json=excluded.tools_json,
                expires_at=excluded.expires_at,
                payload_json=excluded.payload_json,
                updated_at=excluded.updated_at
            "#,
            params![
                id,
                path_json(state_dir),
                item.get("status").and_then(Value::as_str).unwrap_or("unknown"),
                item.get("target_kind").and_then(Value::as_str),
                item.get("target").and_then(Value::as_str),
                item.get("tools").map(compact_json),
                item.get("expires_at").map(value_to_string),
                compact_json(item),
                now.to_string(),
            ],
        )?;
    }
    Ok(items.len())
}

fn appsec_state_dir(root: &Path, args: &[String]) -> PathBuf {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--state-dir" {
            if let Some(value) = iter.next() {
                return PathBuf::from(value);
            }
        }
    }
    root.join("runtime/appsec/default")
}

fn state_dir_arg(root: &Path, args: &[String]) -> PathBuf {
    arg_value(args, "--state-dir")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("PENTEST_STATE_DIR")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| root.join("runtime/appsec/default"))
}

fn state_subcommand(args: &[String]) -> Option<&str> {
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--state-dir" => index += 2,
            "--json" | "--sync" => index += 1,
            value if value.starts_with('-') => index += 1,
            value => return Some(value),
        }
    }
    None
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().cloned();
        }
    }
    None
}

fn appsec_command(args: &[String]) -> String {
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--state-dir" => index += 2,
            "--json" => index += 1,
            value if value.starts_with('-') => index += 1,
            _ => return args[index..].join(" "),
        }
    }
    "help".to_string()
}

fn appsec_status(value: &Value) -> String {
    value
        .get("status")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            value
                .get("coverage_complete")
                .and_then(Value::as_bool)
                .map(|complete| if complete { "complete" } else { "incomplete" }.to_string())
        })
        .or_else(|| {
            value
                .get("ok")
                .and_then(Value::as_bool)
                .map(|ok| if ok { "ok" } else { "failed" }.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn collect_output_artifact_paths(value: &Value, paths: &mut Vec<PathBuf>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let key_lc = key.to_ascii_lowercase();
                if key_lc.contains("artifact") || key_lc == "report" || key_lc.ends_with("_file") {
                    match child {
                        Value::String(path) => paths.push(PathBuf::from(path)),
                        Value::Array(items) => {
                            paths.extend(items.iter().filter_map(Value::as_str).map(PathBuf::from))
                        }
                        _ => collect_output_artifact_paths(child, paths),
                    }
                } else {
                    collect_output_artifact_paths(child, paths);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_output_artifact_paths(item, paths);
            }
        }
        _ => {}
    }
}

fn list_json_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn list_run_record_files(state_dir: &Path) -> Result<Vec<PathBuf>> {
    let runs_dir = state_dir.join("runs");
    if !runs_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = list_json_files(&runs_dir)?;
    for entry in
        fs::read_dir(&runs_dir).with_context(|| format!("failed to read {}", runs_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let run_json = path.join("run.json");
            if run_json.is_file() {
                files.push(run_json);
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn list_json_files_recursive(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_json_files_recursive(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_json_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_files_recursive(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            files.push(path);
        }
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<Value> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn read_json_optional(path: &Path) -> Result<Option<Value>> {
    if path.exists() {
        read_json(path).map(Some)
    } else {
        Ok(None)
    }
}

fn artifact_kind(state_dir: &Path, path: &Path) -> String {
    path.strip_prefix(state_dir)
        .ok()
        .and_then(|relative| relative.components().next())
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .unwrap_or_else(|| "artifact".to_string())
}

fn artifact_stem_id(path: &Path, fallback_prefix: &str) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| stable_id(fallback_prefix, &path_json(path)))
}

fn stable_id(prefix: &str, input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    format!("{prefix}-{}", &hex[..16])
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn path_json(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn relative_path(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => compact_json(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::OptionalExtension;

    #[test]
    fn projects_appsec_file_state_into_core_sqlite_tables() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("runtime/appsec/default");
        fs::create_dir_all(state.join("runs")).unwrap();
        fs::create_dir_all(state.join("assessments")).unwrap();
        fs::write(
            state.join("runs/run-httpx-test.json"),
            r#"{"id":"run-httpx-test","tool":"httpx","status":"completed","command":["httpx","https://example.test"],"target":"https://example.test"}"#,
        )
        .unwrap();
        fs::create_dir_all(state.join("runs/run-nuclei-nested")).unwrap();
        fs::write(
            state.join("runs/run-nuclei-nested/run.json"),
            r#"{"id":"run-nuclei-nested","tool":"nuclei","status":"completed","command":["nuclei","-u","https://example.test"],"target":"https://example.test"}"#,
        )
        .unwrap();
        fs::write(
            state.join("coverage.json"),
            r#"{"version":"ctox.appsec_pentest.coverage.v1","workstreams":[{"id":"ws-1","phase":"recon","target":"https://example.test","status":"completed"}]}"#,
        )
        .unwrap();
        fs::write(
            state.join("findings.json"),
            r#"[{"id":"F-001","title":"Demo","severity":"high","category":"idor","status":"candidate","target":"https://example.test","evidence_artifact":"authz/demo.json"}]"#,
        )
        .unwrap();
        fs::write(
            state.join("investigations.json"),
            r#"{"version":"ctox.deployment_audit.investigations.v1","investigations":[{"id":"investigation-001","candidate_id":"candidate-001","status":"resolved","hypothesis":"A different account can read the protected resource","expected_signal":"The foreign resource is returned","falsification_criterion":"The server rejects every foreign resource request","trigger":{"scanner":"semgrep","target":"src/api.rs"},"work_order":{"tool":"httpx","authorization":"Bearer secret"},"resolution":{"outcome":"confirmed","evidence":{"artifact":"authz/demo.json","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}},"refutation":{"status":"passed","graph_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}]}"#,
        )
        .unwrap();
        let issue_bundle = state.join("reports/issue-bundles/f-001-demo");
        fs::create_dir_all(&issue_bundle).unwrap();
        fs::write(
            issue_bundle.join("reproduce.py"),
            "print('bounded proof')\n",
        )
        .unwrap();
        fs::write(issue_bundle.join("github-issue.md"), "# Demo\n").unwrap();
        fs::write(
            state.join("tool-inventory.json"),
            r#"{"version":"ctox.appsec_pentest.tools.v1","profile":"standard","tools":[{"id":"httpx","available":true,"status":"available","path":"/tmp/httpx","detected_version":"v1"}]}"#,
        )
        .unwrap();
        fs::write(
            state.join("approvals.json"),
            r#"{"version":"ctox.appsec_pentest.active_approvals.v1","approvals":[{"id":"appr-1","status":"granted","target_kind":"url","target":"https://example.test","tools":["nuclei"],"expires_at":"999999"}]}"#,
        )
        .unwrap();
        fs::write(
            state.join("assessment-pipeline-status.json"),
            r#"{"version":"ctox.appsec_pentest.assessment_pipeline_status.v1","stages":[{"id":"stage-1-blackbox-map","phase":"blackbox-map","target":"https://example.test","status":"completed","coverage_status":"completed","active_required":false}]}"#,
        )
        .unwrap();
        fs::write(
            state.join("assessments/assessment-demo.json"),
            r#"{"version":"ctox.appsec_pentest.assessment.v1","profile":"standard","target":"https://example.test","coverage_complete":true}"#,
        )
        .unwrap();

        let args = vec![
            "ctox-appsec".to_string(),
            "--state-dir".to_string(),
            path_json(&state),
            "assess".to_string(),
            "--profile".to_string(),
            "standard".to_string(),
        ];
        let projection = project_cli_result(
            root.path(),
            &args,
            &json!({"ok": true, "command": "assess", "artifact": path_json(&state.join("assessments/assessment-demo.json"))}),
        )
        .unwrap();
        assert_eq!(
            projection
                .pointer("/counts/appsec_runs")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            projection
                .pointer("/completion_review/closable")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            projection
                .pointer("/completion_review/warning_count")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert!(projection
            .pointer("/business_os_projection/projected_count")
            .and_then(Value::as_u64)
            .is_some_and(|count| count >= 8));

        let conn = Connection::open(crate::paths::core_db(root.path())).unwrap();
        for table in [
            "appsec_assessments",
            "appsec_runs",
            "appsec_artifacts",
            "appsec_findings",
            "appsec_investigations",
            "appsec_coverage",
            "appsec_pipeline_stages",
            "appsec_scanner_inventory",
            "appsec_approvals",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert!(count >= 1, "{table} should have projected rows");
        }
        let sha: Option<String> = conn
            .query_row(
                "SELECT sha256 FROM appsec_artifacts WHERE artifact_path LIKE '%coverage.json'",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        assert!(sha.is_some_and(|value| value.len() == 64));
        let reproduce_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM appsec_artifacts WHERE artifact_path LIKE '%reproduce.py'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            reproduce_count, 1,
            "reproduce.py must be projected as proof metadata"
        );
        let investigation: (String, String, Option<String>) = conn
            .query_row(
                "SELECT status, candidate_id, graph_sha256 FROM appsec_investigations WHERE investigation_id = 'investigation-001'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(investigation.0, "resolved");
        assert_eq!(investigation.1, "candidate-001");
        assert_eq!(
            investigation.2.as_deref(),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
        let pipeline_payload: String = conn
            .query_row(
                "SELECT payload_json FROM appsec_pipeline_stages WHERE stage_id = 'stage-1-blackbox-map'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let pipeline_payload: Value = serde_json::from_str(&pipeline_payload).unwrap();
        assert_eq!(
            pipeline_payload.get("stage_kind").and_then(Value::as_str),
            Some("baseline")
        );
        assert_eq!(
            pipeline_payload.get("required").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            pipeline_payload
                .get("evidence_status")
                .and_then(Value::as_str),
            Some("completed")
        );

        let projected_finding = crate::business_os::store::pull_collection_record(
            root.path(),
            "appsec_findings",
            "F-001",
        )
        .unwrap()
        .expect("AppSec finding projected into Business OS records");
        assert_eq!(
            projected_finding.get("source").and_then(Value::as_str),
            Some("ctox-appsec-core-projection")
        );
        assert_eq!(
            projected_finding.get("title").and_then(Value::as_str),
            Some("Demo")
        );
        let projected_investigation = crate::business_os::store::pull_collection_record(
            root.path(),
            "appsec_investigations",
            &stable_id(
                "investigation",
                &format!("{}:investigation-001", path_json(&state)),
            ),
        )
        .unwrap()
        .expect("AppSec investigation projected into Business OS records");
        assert_eq!(
            projected_investigation
                .pointer("/work_order/authorization")
                .and_then(Value::as_str),
            Some("[redacted]")
        );
        assert_eq!(
            projected_investigation
                .get("next_action")
                .and_then(Value::as_str),
            Some("review-proof")
        );
    }

    #[test]
    fn appsec_state_status_reports_completion_blockers() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("runtime/appsec/default");
        fs::create_dir_all(state.join("runs")).unwrap();
        fs::create_dir_all(state.join("assessments")).unwrap();
        fs::write(
            state.join("runs/run-nuclei-blocked.json"),
            r#"{"id":"run-nuclei-blocked","tool":"nuclei","status":"blocked-active-approval-required","command":["nuclei","-u","https://example.test"],"target":"https://example.test"}"#,
        )
        .unwrap();
        fs::write(
            state.join("coverage.json"),
            r#"{"version":"ctox.appsec_pentest.coverage.v1","workstreams":[{"id":"ws-authz","phase":"authenticated-multi-user-authz","target":"https://example.test","status":"coverage-gap"}]}"#,
        )
        .unwrap();
        fs::write(state.join("findings.json"), "[]").unwrap();
        fs::write(
            state.join("tool-inventory.json"),
            r#"{"version":"ctox.appsec_pentest.tools.v1","profile":"full","tools":[{"id":"nuclei","available":false,"status":"missing"}]}"#,
        )
        .unwrap();
        fs::write(
            state.join("assessments/assessment-gap.json"),
            r#"{"version":"ctox.appsec_pentest.assessment.v1","profile":"full","target":"https://example.test","coverage_complete":false}"#,
        )
        .unwrap();

        let status = handle_state_command(
            root.path(),
            &[
                "state".to_string(),
                "status".to_string(),
                "--state-dir".to_string(),
                path_json(&state),
                "--sync".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(status["ok"].as_bool(), Some(true));
        assert_eq!(
            status
                .pointer("/completion_review/closable")
                .and_then(Value::as_bool),
            Some(false)
        );
        let kinds = status
            .pointer("/completion_review/blockers")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .filter_map(|item| item.get("kind").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(kinds.contains(&"assessment-incomplete"));
        assert!(kinds.contains(&"coverage-incomplete"));
        assert!(kinds.contains(&"run-blocked-or-not-executed"));
        assert!(kinds.contains(&"scanner-unavailable"));
        assert_eq!(
            status
                .pointer("/projection/version")
                .and_then(Value::as_str),
            Some("ctox.appsec.durable_projection.v1")
        );
    }

    #[test]
    fn investigation_projection_keys_are_scoped_per_test_workspace() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("runtime/appsec/tests/first");
        let second = root.path().join("runtime/appsec/tests/second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let payload = r#"{"investigations":[{"id":"shared-id","candidate_id":"candidate-1","status":"planned"}]}"#;
        fs::write(first.join("investigations.json"), payload).unwrap();
        fs::write(second.join("investigations.json"), payload).unwrap();

        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        project_investigations(&conn, &first, 1).unwrap();
        project_investigations(&conn, &second, 2).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM appsec_investigations WHERE investigation_id = 'shared-id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn appsec_pipeline_projection_links_queue_task_and_blocks_incomplete_stage() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("runtime/appsec/default");
        fs::create_dir_all(state.join("assessments")).unwrap();
        fs::write(
            state.join("assessment-pipeline-status.json"),
            r#"{"version":"ctox.appsec_pentest.assessment_pipeline_status.v1","stages":[{"id":"stage-1-authz-probe","phase":"authenticated-multi-user-authz","target":"https://example.test","status":"ready","coverage_status":"planned","active_required":false}]}"#,
        )
        .unwrap();
        fs::write(
            state.join("coverage.json"),
            r#"{"version":"ctox.appsec_pentest.coverage.v1","workstreams":[{"id":"ws-authz","phase":"authenticated-multi-user-authz","target":"https://example.test","status":"planned"}]}"#,
        )
        .unwrap();
        fs::write(state.join("findings.json"), "[]").unwrap();
        fs::write(
            state.join("tool-inventory.json"),
            r#"{"version":"ctox.appsec_pentest.tools.v1","profile":"standard","tools":[{"id":"httpx","profile":"standard","available":true,"status":"available"}]}"#,
        )
        .unwrap();
        fs::write(
            state.join("assessments/assessment-authz.json"),
            r#"{"version":"ctox.appsec_pentest.assessment.v1","profile":"standard","target":"https://example.test","coverage_complete":false}"#,
        )
        .unwrap();

        let task = crate::channels::create_queue_task(
            root.path(),
            crate::channels::QueueTaskCreateRequest {
                title: "AppSec pentest stage: authenticated authz".to_string(),
                prompt:
                    "Run authenticated multi-user authorization checks with evidence writeback."
                        .to_string(),
                thread_key: "appsec:authenticated-multi-user-authz:https-example-test".to_string(),
                workspace_root: Some(path_json(root.path())),
                priority: "normal".to_string(),
                suggested_skill: Some("appsec-pentest".to_string()),
                parent_message_key: None,
                extra_metadata: Some(json!({
                    "source": "ctox-appsec-pipeline",
                    "idempotency_key": "appsec-stage-1-authz-probe",
                    "appsec_state_dir": path_json(&state),
                    "stage": {
                        "id": "stage-1-authz-probe",
                        "phase": "authenticated-multi-user-authz",
                        "target": "https://example.test"
                    }
                })),
            },
        )
        .unwrap();

        let status = handle_state_command(
            root.path(),
            &[
                "state".to_string(),
                "status".to_string(),
                "--state-dir".to_string(),
                path_json(&state),
                "--sync".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            status
                .pointer("/counts/appsec_pipeline_stages")
                .and_then(Value::as_u64),
            Some(1)
        );

        let conn = Connection::open(crate::paths::core_db(root.path())).unwrap();
        let (queue_task_id, queue_status, payload_json): (Option<String>, Option<String>, String) =
            conn.query_row(
                "SELECT queue_task_id, queue_status, payload_json
                 FROM appsec_pipeline_stages
                 WHERE stage_id = 'stage-1-authz-probe'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(queue_task_id.as_deref(), Some(task.message_key.as_str()));
        assert_eq!(queue_status.as_deref(), Some("pending"));
        let payload: Value = serde_json::from_str(&payload_json).unwrap();
        assert_eq!(
            payload
                .pointer("/queue_task/metadata/skill")
                .and_then(Value::as_str),
            Some("appsec-pentest")
        );

        let kinds = status
            .pointer("/completion_review/blockers")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .filter_map(|item| item.get("kind").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(kinds.contains(&"pipeline-stage-incomplete"));
    }

    #[test]
    fn appsec_pipeline_writeback_survives_queue_source_rewrite_and_uses_run_evidence() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("runtime/appsec/default");
        fs::create_dir_all(state.join("assessments")).unwrap();
        fs::write(
            state.join("assessment-pipeline-status.json"),
            r#"{"version":"ctox.appsec_pentest.assessment_pipeline_status.v1","stages":[{"id":"stage-1-httpx","phase":"blackbox-map","target":"https://example.test","status":"ready","coverage_status":"planned","active_required":false,"tools":["httpx"]}]}"#,
        )
        .unwrap();
        fs::write(
            state.join("coverage.json"),
            r#"{"version":"ctox.appsec_pentest.coverage.v1","workstreams":[{"id":"ws-map","phase":"blackbox-map","target":"https://example.test","status":"planned","tools":["httpx"]}]}"#,
        )
        .unwrap();
        fs::write(state.join("findings.json"), "[]").unwrap();
        fs::write(
            state.join("tool-inventory.json"),
            r#"{"version":"ctox.appsec_pentest.tools.v1","profile":"standard","tools":[{"id":"httpx","profile":"standard","available":true,"status":"available"}]}"#,
        )
        .unwrap();
        fs::write(
            state.join("assessments/assessment-map.json"),
            r#"{"version":"ctox.appsec_pentest.assessment.v1","profile":"standard","target":"https://example.test","coverage_complete":false}"#,
        )
        .unwrap();

        let task = crate::channels::create_queue_task(
            root.path(),
            crate::channels::QueueTaskCreateRequest {
                title: "AppSec pentest stage: blackbox map".to_string(),
                prompt: "Run httpx and write stage evidence.".to_string(),
                thread_key: "appsec:blackbox-map:https-example-test".to_string(),
                workspace_root: Some(path_json(root.path())),
                priority: "normal".to_string(),
                suggested_skill: Some("appsec-pentest".to_string()),
                parent_message_key: None,
                extra_metadata: Some(json!({
                    "source": "ctox-appsec-pipeline",
                    "idempotency_key": "appsec-stage-1-httpx",
                    "appsec_state_dir": path_json(&state),
                    "stage": {
                        "id": "stage-1-httpx",
                        "phase": "blackbox-map",
                        "target": "https://example.test"
                    }
                })),
            },
        )
        .unwrap();
        crate::channels::update_queue_task(
            root.path(),
            crate::channels::QueueTaskUpdateRequest {
                message_key: task.message_key.clone(),
                title: Some("AppSec pentest stage: blackbox map updated".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        crate::channels::lease_queue_task(root.path(), &task.message_key, "appsec-test").unwrap();

        let status_without_run = handle_state_command(
            root.path(),
            &[
                "state".to_string(),
                "status".to_string(),
                "--state-dir".to_string(),
                path_json(&state),
                "--sync".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            status_without_run
                .pointer("/counts/appsec_pipeline_stages")
                .and_then(Value::as_u64),
            Some(1)
        );

        let conn = Connection::open(crate::paths::core_db(root.path())).unwrap();
        let (source, projected_status, queue_status): (Option<String>, String, Option<String>) =
            conn.query_row(
                "SELECT json_extract(m.metadata_json, '$.source'), s.status, s.queue_status
                 FROM appsec_pipeline_stages s
                 JOIN communication_messages m ON m.message_key = s.queue_task_id
                 WHERE s.stage_id = 'stage-1-httpx'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(source.as_deref(), Some("ctox-queue"));
        assert_eq!(projected_status, "in-progress");
        assert_eq!(queue_status.as_deref(), Some("leased"));

        let writeback_path = state.join("assessment-pipeline-writeback.json");
        let writeback = read_json(&writeback_path).unwrap();
        assert_eq!(
            writeback
                .pointer("/stages/0/status")
                .and_then(Value::as_str),
            Some("in-progress")
        );
        assert_eq!(
            status_without_run
                .pointer("/completion_review/closable")
                .and_then(Value::as_bool),
            Some(false)
        );

        fs::create_dir_all(state.join("runs/run-httpx-evidence")).unwrap();
        fs::write(
            state.join("runs/run-httpx-evidence/run.json"),
            r#"{"id":"run-httpx-evidence","tool":"httpx","status":"completed","command":["httpx","--url","https://example.test"],"exit_code":0}"#,
        )
        .unwrap();
        let status_with_run = handle_state_command(
            root.path(),
            &[
                "state".to_string(),
                "status".to_string(),
                "--state-dir".to_string(),
                path_json(&state),
                "--sync".to_string(),
            ],
        )
        .unwrap();
        let conn = Connection::open(crate::paths::core_db(root.path())).unwrap();
        let projected_status: String = conn
            .query_row(
                "SELECT status FROM appsec_pipeline_stages WHERE stage_id = 'stage-1-httpx'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(projected_status, "in-progress");
        assert_eq!(
            status_with_run
                .pointer("/counts/appsec_runs")
                .and_then(Value::as_u64),
            Some(1)
        );
        let writeback = read_json(&writeback_path).unwrap();
        assert_eq!(
            writeback
                .pointer("/stages/0/run_evidence/status_counts/completed")
                .and_then(Value::as_u64),
            Some(1)
        );
    }
}
