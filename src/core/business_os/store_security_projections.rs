// Origin: CTOX
// License: Apache-2.0

use super::*;

/// Project the outcome of a `ctox.iot.*` business command into the
/// RxDB-visible `business_records` store. The engine state lives in
/// `runtime/ctox.sqlite3` (written by the shared `iot::commands` op via
/// `crate::paths::core_db`); `iot::projector` reads it back and builds the
/// canonical `iot_*` envelopes, and this integrator writes those rows into the
/// business-os store (the read source for `pull_collection_records` and the
/// RxDB peer). No HTTP bridge: every row flows engine -> projector ->
/// business_records -> RxDB/WebRTC.
///
/// Returns the `(collection, record_id)` pairs for the rxdb_peer to stream into
/// the live RxDB collections. Idempotent: a replayed outcome rewrites identical
/// envelopes (only `_rev`/`updated_at_ms` advance) and tombstones stay
/// tombstoned.
pub(in crate::business_os) fn project_iot_business_command_outcome(
    root: &Path,
    result: &Value,
) -> anyhow::Result<Vec<(&'static str, String)>> {
    use crate::iot::projector::ReprojectedRecord;

    let records = crate::iot::projector::reproject_business_command_outcome(root, result)?;
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let conn = open_store(root)?;
    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    for record in records {
        match record {
            ReprojectedRecord::Rows(rows) => {
                for row in rows {
                    upsert_iot_projection_row(
                        &conn,
                        row.collection,
                        &row.record_id,
                        row.updated_at_ms,
                        row.payload.clone(),
                    )?;
                    pairs.push((row.collection, row.record_id));
                }
            }
            ReprojectedRecord::EchoOnly {
                collection,
                record_id,
            } => {
                // The executor already wrote the (query-scoped) datapoint window
                // row into the core db's business_records; mirror it into the
                // business-os store so the RxDB read path can echo it.
                if let Some(payload) = read_core_db_business_record(root, collection, &record_id)? {
                    let updated_at_ms = payload
                        .get("updated_at_ms")
                        .and_then(Value::as_i64)
                        .unwrap_or_else(|| now_ms() as i64);
                    upsert_iot_projection_row(
                        &conn,
                        collection,
                        &record_id,
                        updated_at_ms,
                        payload,
                    )?;
                    pairs.push((collection, record_id));
                }
            }
        }
    }
    Ok(pairs)
}

/// Full idempotent resync of EVERY projectable iot engine row into the
/// RxDB-visible business-os store (`open_store`). This is the bridge `ctox iot
/// project all` calls: without it, CLI mutations (asset.upsert, attribute.write,
/// …) only write engine state + an inline core-db row and never reach the
/// `business-os.sqlite3` store the apps read, so they never replicate over
/// RxDB/WebRTC. The projector is the canonical envelope producer
/// (`projector::project_all` reads `runtime/ctox.sqlite3` engine tables, never
/// writes); this function owns the `business_records` write into the
/// RxDB-visible store, mirroring `project_iot_business_command_outcome`. No HTTP
/// bridge: engine -> projector -> business_records -> RxDB/WebRTC. Returns the
/// `(collection, record_id)` pairs written.
///
/// `realm` selects the projection/sync scope: `Some(r)` projects ONLY realm
/// `r`'s rows into the RxDB-visible store (the session/executor path must use
/// this so WebRTC never replicates other realms' rows to a paired peer);
/// `None` is the trusted operator resync (`ctox iot project all`) that mirrors
/// every realm. Realm isolation on the projection/sync surface is enforced in
/// `projector::project_all_in_realm`.
pub(crate) fn project_all_iot(
    root: &Path,
    realm: Option<&str>,
) -> anyhow::Result<Vec<(&'static str, String)>> {
    let engine = crate::iot::store::open_iot_store(root)?;
    let rows = crate::iot::projector::project_all_in_realm(&engine, realm)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let conn = open_store(root)?;
    let mut pairs: Vec<(&'static str, String)> = Vec::with_capacity(rows.len());
    for row in rows {
        upsert_iot_projection_row(
            &conn,
            row.collection,
            &row.record_id,
            row.updated_at_ms,
            row.payload.clone(),
        )?;
        pairs.push((row.collection, row.record_id));
    }
    Ok(pairs)
}

/// Project already-canonical IoT rows into the RxDB-visible Business OS store.
/// Runtime agent pumps use this path after `iot::runtime::run_agent_step`
/// returns projector rows; command execution uses
/// `project_iot_business_command_outcome`, which first re-derives rows from a
/// command outcome. Both converge on the same tombstone-aware upsert below.
pub(in crate::business_os) fn project_iot_projection_rows(
    root: &Path,
    rows: Vec<crate::iot::projector::ProjectionRow>,
) -> anyhow::Result<Vec<(&'static str, String)>> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let conn = open_store(root)?;
    let mut pairs = Vec::with_capacity(rows.len());
    for row in rows {
        upsert_iot_projection_row(
            &conn,
            row.collection,
            &row.record_id,
            row.updated_at_ms,
            row.payload.clone(),
        )?;
        pairs.push((row.collection, row.record_id));
    }
    Ok(pairs)
}

pub(in crate::business_os) fn is_appsec_business_command(command_type: &str) -> bool {
    command_type.starts_with("ctox.appsec.")
}

pub(crate) fn appsec_business_command_requires_data_write(command_type: &str) -> bool {
    matches!(
        command_type,
        "ctox.appsec.app.audit"
            | "ctox.appsec.assessment.create"
            | "ctox.appsec.assessment.run"
            | "ctox.appsec.assessment.archive"
            | "ctox.appsec.audit.run"
            | "ctox.appsec.exploit.verify"
            | "ctox.appsec.lab.create"
            | "ctox.appsec.lab.run"
            | "ctox.appsec.report.export"
            | "ctox.appsec.release.review"
            | "ctox.appsec.authz.plan"
            | "ctox.appsec.authz.credential_proof_template"
            | "ctox.appsec.authz.credential_proof_from_evidence"
            | "ctox.appsec.authz.preflight"
            | "ctox.appsec.authz.run"
            | "ctox.appsec.authz.build_matrix"
            | "ctox.appsec.graph.build"
            | "ctox.appsec.investigation.plan"
            | "ctox.appsec.investigation.execute"
            | "ctox.appsec.investigation.resolve"
            | "ctox.appsec.investigation.refute"
            | "ctox.appsec.replay.baseline"
            | "ctox.appsec.replay.investigations"
            | "ctox.appsec.pipeline.rework"
            | "ctox.appsec.approval.request"
            | "ctox.appsec.approval.grant"
            | "ctox.appsec.approval.revoke"
    )
}

pub(crate) fn project_appsec_durable_state_to_business_os(
    root: &Path,
    state_dir: &Path,
) -> anyhow::Result<Vec<(&'static str, String)>> {
    let core_path = crate::paths::core_db(root);
    if !core_path.is_file() {
        return Ok(Vec::new());
    }
    let core = Connection::open(&core_path)
        .with_context(|| format!("failed to open {}", core_path.display()))?;
    core.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure AppSec core SQLite busy_timeout")?;
    if !APPSEC_BUSINESS_OS_COLLECTIONS
        .iter()
        .all(|collection| sqlite_table_exists(&core, collection).unwrap_or(false))
    {
        return Ok(Vec::new());
    }

    let business = open_store(root)?;
    let state = state_dir.display().to_string();
    let mut pairs = Vec::new();
    project_appsec_assessments(&core, &business, &state, &mut pairs)?;
    project_appsec_runs(&core, &business, &state, &mut pairs)?;
    project_appsec_artifacts(&core, &business, &state, &mut pairs)?;
    project_appsec_findings(&core, &business, &state, &mut pairs)?;
    project_appsec_investigations(&core, &business, &state, &mut pairs)?;
    project_appsec_coverage(&core, &business, &state, &mut pairs)?;
    project_appsec_pipeline_stages(&core, &business, &state, &mut pairs)?;
    project_appsec_scanner_inventory(&core, &business, &state, &mut pairs)?;
    project_appsec_approvals(&core, &business, &state, &mut pairs)?;
    Ok(pairs)
}

fn project_appsec_assessments(
    core: &Connection,
    business: &Connection,
    state: &str,
    pairs: &mut Vec<(&'static str, String)>,
) -> anyhow::Result<()> {
    let mut stmt = core.prepare(
        "SELECT assessment_id, target, profile, status, command, artifact_path, payload_json, created_at, updated_at
         FROM appsec_assessments
         WHERE state_dir = ?1
         ORDER BY CAST(updated_at AS INTEGER) ASC, assessment_id ASC",
    )?;
    let rows = stmt.query_map(params![state], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    for row in rows {
        let (
            id,
            target,
            profile,
            status,
            command,
            artifact_path,
            payload_json,
            created_at,
            updated_at,
        ) = row?;
        let payload = parse_json_value(&payload_json);
        let record = serde_json::json!({
            "assessment_id": id,
            "state_dir": state,
            "name": payload.get("name").cloned().unwrap_or(Value::Null),
            "target": target,
            "profile": profile,
            "status": status,
            "mode": payload.get("mode").cloned().unwrap_or(Value::Null),
            "source_path": payload.get("source_path").cloned().unwrap_or(Value::Null),
            "authz_subjects": payload.get("authz_subjects").cloned().unwrap_or(Value::Null),
            "active": payload.get("active").cloned().unwrap_or(Value::Bool(false)),
            "approval_id": payload.get("approval_id").cloned().unwrap_or(Value::Null),
            "wordlist": payload.get("wordlist").cloned().unwrap_or(Value::Null),
            "command": command,
            "artifact_path": artifact_path,
            "scan_completed": payload.get("scan_completed").cloned().unwrap_or(Value::Null),
            "coverage_complete": payload.get("coverage_complete").cloned().unwrap_or(Value::Null),
            "created_at": created_at,
            "updated_at": updated_at,
            "source": "ctox-appsec-core-projection",
        });
        let updated_at_ms = appsec_updated_at_ms(&updated_at);
        upsert_business_record(business, "appsec_assessments", &id, updated_at_ms, record)?;
        pairs.push(("appsec_assessments", id));
    }
    Ok(())
}

fn project_appsec_runs(
    core: &Connection,
    business: &Connection,
    state: &str,
    pairs: &mut Vec<(&'static str, String)>,
) -> anyhow::Result<()> {
    let mut stmt = core.prepare(
        "SELECT run_id, assessment_id, tool, target, status, command_json, artifact_path, created_at, updated_at
         FROM appsec_runs
         WHERE state_dir = ?1
         ORDER BY CAST(updated_at AS INTEGER) ASC, run_id ASC",
    )?;
    let rows = stmt.query_map(params![state], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    for row in rows {
        let (
            id,
            assessment_id,
            tool,
            target,
            status,
            command_json,
            artifact_path,
            created_at,
            updated_at,
        ) = row?;
        let command_summary = command_json
            .as_deref()
            .map(parse_json_value)
            .map(|value| summarize_appsec_command_json(&value))
            .unwrap_or(Value::Null);
        let record = serde_json::json!({
            "run_id": id,
            "assessment_id": assessment_id,
            "state_dir": state,
            "tool": tool,
            "target": target,
            "status": status,
            "command_summary": command_summary,
            "artifact_path": artifact_path,
            "created_at": created_at,
            "updated_at": updated_at,
            "source": "ctox-appsec-core-projection",
        });
        let updated_at_ms = appsec_updated_at_ms(&updated_at);
        upsert_business_record(business, "appsec_runs", &id, updated_at_ms, record)?;
        pairs.push(("appsec_runs", id));
    }
    Ok(())
}

#[derive(Clone)]
struct AppsecProjectedArtifact {
    artifact_path: String,
    kind: String,
    version: Option<String>,
    sha256: String,
    size_bytes: i64,
    metadata: Value,
    updated_at: String,
}

fn load_appsec_projected_artifacts(
    core: &Connection,
    state: &str,
) -> anyhow::Result<Vec<AppsecProjectedArtifact>> {
    let mut stmt = core.prepare(
        "SELECT artifact_path, kind, version, sha256, size_bytes, metadata_json, updated_at
         FROM appsec_artifacts
         WHERE state_dir = ?1
         ORDER BY CAST(updated_at AS INTEGER) ASC, artifact_path ASC",
    )?;
    let rows = stmt.query_map(params![state], |row| {
        Ok(AppsecProjectedArtifact {
            artifact_path: row.get(0)?,
            kind: row.get(1)?,
            version: row.get(2)?,
            sha256: row.get(3)?,
            size_bytes: row.get(4)?,
            metadata: sanitize_appsec_projection_json(parse_json_value(&row.get::<_, String>(5)?)),
            updated_at: row.get(6)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn appsec_evidence_artifact_rank(artifact: &AppsecProjectedArtifact) -> u8 {
    let path = format!(
        "{} {}",
        artifact.artifact_path,
        artifact
            .metadata
            .get("relative_path")
            .and_then(Value::as_str)
            .unwrap_or("")
    )
    .to_ascii_lowercase();
    if path.contains("reproduce.py") {
        0
    } else if path.contains("latest-verification") {
        1
    } else if path.contains("http-proof") || path.contains("source-proof") {
        2
    } else if path.contains("verification-") && path.ends_with(".json") {
        3
    } else if path.contains("evidence-manifest") {
        4
    } else if path.contains("github-issue") {
        5
    } else {
        10
    }
}

fn appsec_finding_evidence_summary(
    state: &str,
    finding_id: &str,
    evidence_artifact: Option<&str>,
    artifacts: &[AppsecProjectedArtifact],
) -> Vec<Value> {
    let finding_marker = format!("{}-", finding_id.to_ascii_lowercase());
    let explicit = evidence_artifact.unwrap_or("").to_ascii_lowercase();
    let mut matching = artifacts
        .iter()
        .filter(|artifact| {
            let path = format!(
                "{} {} {}",
                artifact.artifact_path,
                artifact
                    .metadata
                    .get("relative_path")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                artifact.kind
            )
            .to_ascii_lowercase();
            (!explicit.is_empty() && path.contains(&explicit))
                || (!finding_marker.is_empty() && path.contains(&finding_marker))
        })
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| {
        appsec_evidence_artifact_rank(left)
            .cmp(&appsec_evidence_artifact_rank(right))
            .then_with(|| left.artifact_path.cmp(&right.artifact_path))
    });
    matching
        .into_iter()
        .take(5)
        .map(|artifact| {
            serde_json::json!({
                "artifact_id": stable_record_id("appsec_artifact", &artifact.artifact_path),
                "artifact_path": artifact.artifact_path,
                "state_dir": state,
                "kind": artifact.kind,
                "version": artifact.version,
                "sha256": artifact.sha256,
                "size_bytes": artifact.size_bytes,
                "metadata": artifact.metadata,
                "content_available": false,
                "content_policy": "metadata-only; use redacted AppSec report/finding tools for detail reads",
                "updated_at": artifact.updated_at,
                "source": "ctox-appsec-core-projection",
            })
        })
        .collect()
}

fn project_appsec_artifacts(
    core: &Connection,
    business: &Connection,
    state: &str,
    pairs: &mut Vec<(&'static str, String)>,
) -> anyhow::Result<()> {
    for artifact in load_appsec_projected_artifacts(core, state)? {
        let id = stable_record_id("appsec_artifact", &artifact.artifact_path);
        let updated_at_ms = appsec_updated_at_ms(&artifact.updated_at);
        let record = serde_json::json!({
            "artifact_id": id,
            "artifact_path": artifact.artifact_path,
            "state_dir": state,
            "kind": artifact.kind,
            "version": artifact.version,
            "sha256": artifact.sha256,
            "size_bytes": artifact.size_bytes,
            "metadata": artifact.metadata,
            "content_available": false,
            "content_policy": "metadata-only; use redacted AppSec report/finding tools for detail reads",
            "updated_at": artifact.updated_at,
            "source": "ctox-appsec-core-projection",
        });
        upsert_business_record(business, "appsec_artifacts", &id, updated_at_ms, record)?;
        pairs.push(("appsec_artifacts", id));
    }
    Ok(())
}

pub(super) fn project_appsec_findings(
    core: &Connection,
    business: &Connection,
    state: &str,
    pairs: &mut Vec<(&'static str, String)>,
) -> anyhow::Result<()> {
    let artifacts = load_appsec_projected_artifacts(core, state)?;
    let mut stmt = core.prepare(
        "SELECT finding_id, title, severity, category, status, target, evidence_artifact, payload_json, updated_at
         FROM appsec_findings
         WHERE state_dir = ?1
         ORDER BY CAST(updated_at AS INTEGER) ASC, finding_id ASC",
    )?;
    let rows = stmt.query_map(params![state], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    for row in rows {
        let (
            id,
            title,
            severity,
            category,
            status,
            target,
            evidence_artifact,
            payload_json,
            updated_at,
        ) = row?;
        let payload = parse_json_value(&payload_json);
        let evidence_artifacts =
            appsec_finding_evidence_summary(state, &id, evidence_artifact.as_deref(), &artifacts);
        let updated_at_ms = evidence_artifacts
            .iter()
            .filter_map(|artifact| artifact.get("updated_at").and_then(Value::as_str))
            .map(appsec_updated_at_ms)
            .fold(appsec_updated_at_ms(&updated_at), i64::max);
        let record = serde_json::json!({
            "finding_id": id,
            "state_dir": state,
            "title": title,
            "severity": severity,
            "category": category,
            "status": status,
            "target": target,
            "evidence_artifact": evidence_artifact,
            "evidence_artifacts": evidence_artifacts,
            "source_tool": payload.get("source_tool").cloned().unwrap_or(Value::Null),
            "signal": payload.get("signal").cloned().unwrap_or(Value::Null),
            "validation_state": payload.get("validation_state").cloned().unwrap_or(Value::Null),
            "cve": payload.get("cve").cloned().unwrap_or(Value::Null),
            "installed_version": payload.get("installed_version").cloned().unwrap_or(Value::Null),
            "affected_versions": payload.get("affected_versions").cloned().unwrap_or(Value::Null),
            "fixed_versions": payload.get("fixed_versions").cloned().unwrap_or(Value::Null),
            "reachability": payload.get("reachability").cloned().unwrap_or(Value::Null),
            "updated_at": updated_at,
            "source": "ctox-appsec-core-projection",
        });
        upsert_business_record(business, "appsec_findings", &id, updated_at_ms, record)?;
        pairs.push(("appsec_findings", id));
    }
    Ok(())
}

fn project_appsec_investigations(
    core: &Connection,
    business: &Connection,
    state: &str,
    pairs: &mut Vec<(&'static str, String)>,
) -> anyhow::Result<()> {
    let mut stmt = core.prepare(
        "SELECT investigation_key, investigation_id, candidate_id, status, outcome, hypothesis, expected_signal,
                falsification_criterion, evidence_artifact, graph_sha256, payload_json, updated_at
         FROM appsec_investigations
         WHERE state_dir = ?1
         ORDER BY CAST(updated_at AS INTEGER) ASC, investigation_key ASC",
    )?;
    let rows = stmt.query_map(params![state], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, String>(11)?,
        ))
    })?;
    for row in rows {
        let (
            key,
            id,
            candidate_id,
            status,
            outcome,
            hypothesis,
            expected_signal,
            falsification_criterion,
            evidence_artifact,
            graph_sha256,
            payload_json,
            updated_at,
        ) = row?;
        let payload = sanitize_appsec_projection_json(parse_json_value(&payload_json));
        let record = serde_json::json!({
            "investigation_id": id,
            "candidate_id": candidate_id,
            "state_dir": state,
            "status": status,
            "outcome": outcome,
            "hypothesis": hypothesis,
            "expected_signal": expected_signal,
            "falsification_criterion": falsification_criterion,
            "evidence_artifact": evidence_artifact,
            "graph_sha256": graph_sha256,
            "trigger": payload.get("trigger").cloned().unwrap_or(Value::Null),
            "candidate_binding": payload.get("candidate_binding").cloned().unwrap_or(Value::Null),
            "work_order": payload.get("work_order").cloned().unwrap_or(Value::Null),
            "work_order_sha256": payload.get("work_order_sha256").cloned().unwrap_or(Value::Null),
            "execution": payload.get("execution").cloned().unwrap_or(Value::Null),
            "resolution": payload.get("resolution").cloned().unwrap_or(Value::Null),
            "refutation": payload.get("refutation").cloned().unwrap_or(Value::Null),
            "next_action": appsec_investigation_next_action(&status, outcome.as_deref()),
            "updated_at": updated_at,
            "source": "ctox-appsec-core-projection",
        });
        let updated_at_ms = appsec_updated_at_ms(&updated_at);
        upsert_business_record(
            business,
            "appsec_investigations",
            &key,
            updated_at_ms,
            record,
        )?;
        pairs.push(("appsec_investigations", key));
    }
    Ok(())
}

fn appsec_investigation_next_action(status: &str, outcome: Option<&str>) -> &'static str {
    match (status, outcome) {
        ("planned", _) => "configure",
        ("ready", _) => "execute",
        ("evidence-ready", _) => "resolve",
        ("resolved", Some("confirmed")) => "review-proof",
        ("refutation-blocked", _) => "review-refutation",
        ("blocked", _) => "review-blocker",
        ("resolved", _) => "none",
        _ => "review",
    }
}

fn project_appsec_coverage(
    core: &Connection,
    business: &Connection,
    state: &str,
    pairs: &mut Vec<(&'static str, String)>,
) -> anyhow::Result<()> {
    let mut stmt = core.prepare(
        "SELECT coverage_id, phase, target, status, artifact_path, payload_json, updated_at
         FROM appsec_coverage
         WHERE state_dir = ?1
         ORDER BY CAST(updated_at AS INTEGER) ASC, coverage_id ASC",
    )?;
    let rows = stmt.query_map(params![state], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    for row in rows {
        let (id, phase, target, status, artifact_path, payload_json, updated_at) = row?;
        let payload = parse_json_value(&payload_json);
        let record = serde_json::json!({
            "coverage_id": id,
            "state_dir": state,
            "phase": phase,
            "target": target,
            "status": status,
            "artifact_path": artifact_path,
            "blocker_kind": payload.get("blocker_kind").cloned().unwrap_or(Value::Null),
            "requires_active_approval": payload.get("requires_active_approval").cloned().unwrap_or(Value::Null),
            "updated_at": updated_at,
            "source": "ctox-appsec-core-projection",
        });
        let updated_at_ms = appsec_updated_at_ms(&updated_at);
        upsert_business_record(business, "appsec_coverage", &id, updated_at_ms, record)?;
        pairs.push(("appsec_coverage", id));
    }
    Ok(())
}

fn project_appsec_pipeline_stages(
    core: &Connection,
    business: &Connection,
    state: &str,
    pairs: &mut Vec<(&'static str, String)>,
) -> anyhow::Result<()> {
    let mut stmt = core.prepare(
        "SELECT stage_key, stage_id, phase, target, status, coverage_status, active_required,
                queue_task_id, queue_status, queue_updated_at, payload_json, updated_at
         FROM appsec_pipeline_stages
         WHERE state_dir = ?1
         ORDER BY CAST(updated_at AS INTEGER) ASC, stage_key ASC",
    )?;
    let rows = stmt.query_map(params![state], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, String>(11)?,
        ))
    })?;
    for row in rows {
        let (
            key,
            stage_id,
            phase,
            target,
            status,
            coverage_status,
            active_required,
            queue_task_id,
            queue_status,
            queue_updated_at,
            payload_json,
            updated_at,
        ) = row?;
        let payload = parse_json_value(&payload_json);
        let record = serde_json::json!({
            "stage_key": key,
            "stage_id": stage_id,
            "state_dir": state,
            "phase": phase,
            "target": target,
            "status": status,
            "coverage_status": coverage_status,
            "active_required": active_required != 0,
            "queue_task_id": queue_task_id,
            "queue_status": queue_status,
            "queue_updated_at": queue_updated_at,
            "stage_kind": payload.get("stage_kind").cloned().unwrap_or(Value::Null),
            "origin_id": payload.get("origin_id").cloned().unwrap_or(Value::Null),
            "required": payload.get("required").cloned().unwrap_or(Value::Bool(true)),
            "evidence_status": payload.get("evidence_status").cloned().unwrap_or(Value::Null),
            "resume_gate": payload.get("resume_gate").cloned().unwrap_or(Value::Null),
            "completion_gate": payload.get("completion_gate").cloned().unwrap_or(Value::Null),
            "updated_at": updated_at,
            "source": "ctox-appsec-core-projection",
        });
        let updated_at_ms = appsec_updated_at_ms(&updated_at);
        upsert_business_record(
            business,
            "appsec_pipeline_stages",
            &key,
            updated_at_ms,
            record,
        )?;
        pairs.push(("appsec_pipeline_stages", key));
    }
    Ok(())
}

fn project_appsec_scanner_inventory(
    core: &Connection,
    business: &Connection,
    state: &str,
    pairs: &mut Vec<(&'static str, String)>,
) -> anyhow::Result<()> {
    let mut stmt = core.prepare(
        "SELECT scanner_id, profile, available, status, binary_path, detected_version, payload_json, updated_at
         FROM appsec_scanner_inventory
         WHERE state_dir = ?1
         ORDER BY CAST(updated_at AS INTEGER) ASC, scanner_id ASC",
    )?;
    let rows = stmt.query_map(params![state], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    for row in rows {
        let (
            id,
            profile,
            available,
            status,
            binary_path,
            detected_version,
            payload_json,
            updated_at,
        ) = row?;
        let payload = parse_json_value(&payload_json);
        let record = serde_json::json!({
            "scanner_id": id,
            "state_dir": state,
            "profile": profile,
            "available": available != 0,
            "status": status,
            "binary_path": binary_path,
            "detected_version": detected_version,
            "category": payload.get("category").cloned().unwrap_or(Value::Null),
            "install_kind": payload.get("install_kind").cloned().unwrap_or(Value::Null),
            "active": payload.get("active").cloned().unwrap_or(Value::Null),
            "default_enabled": payload.get("default_enabled").cloned().unwrap_or(Value::Null),
            "updated_at": updated_at,
            "source": "ctox-appsec-core-projection",
        });
        let updated_at_ms = appsec_updated_at_ms(&updated_at);
        upsert_business_record(
            business,
            "appsec_scanner_inventory",
            &id,
            updated_at_ms,
            record,
        )?;
        pairs.push(("appsec_scanner_inventory", id));
    }
    Ok(())
}

fn project_appsec_approvals(
    core: &Connection,
    business: &Connection,
    state: &str,
    pairs: &mut Vec<(&'static str, String)>,
) -> anyhow::Result<()> {
    let mut stmt = core.prepare(
        "SELECT approval_id, status, target_kind, target, tools_json, expires_at, payload_json, updated_at
         FROM appsec_approvals
         WHERE state_dir = ?1
         ORDER BY CAST(updated_at AS INTEGER) ASC, approval_id ASC",
    )?;
    let rows = stmt.query_map(params![state], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    for row in rows {
        let (id, status, target_kind, target, tools_json, expires_at, payload_json, updated_at) =
            row?;
        let payload = parse_json_value(&payload_json);
        let tools = tools_json
            .as_deref()
            .map(parse_json_value)
            .and_then(|value| match value {
                Value::Array(_) => Some(value),
                _ => None,
            })
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let record = serde_json::json!({
            "approval_id": id,
            "state_dir": state,
            "status": status,
            "target_kind": target_kind,
            "target": target,
            "tools": tools,
            "expires_at": expires_at,
            "profile": payload.get("profile").cloned().unwrap_or(Value::Null),
            "reason": payload.get("reason").cloned().unwrap_or(Value::Null),
            "requested_by": payload.get("requested_by").cloned().unwrap_or(Value::Null),
            "approved_by": payload.get("approved_by").cloned().unwrap_or(Value::Null),
            "approved_at": payload.get("approved_at").cloned().unwrap_or(Value::Null),
            "revoked_at": payload.get("revoked_at").cloned().unwrap_or(Value::Null),
            "grant_reason": payload.get("grant_reason").cloned().unwrap_or(Value::Null),
            "grant_review": sanitize_appsec_projection_json(
                payload.get("grant_review").cloned().unwrap_or(Value::Null),
            ),
            "review_policy": sanitize_appsec_projection_json(
                payload.get("review_policy").cloned().unwrap_or(Value::Null),
            ),
            "board_review": sanitize_appsec_projection_json(
                payload.get("board_review").cloned().unwrap_or(Value::Null),
            ),
            "board_reviews": sanitize_appsec_projection_json(
                payload.get("board_reviews").cloned().unwrap_or(Value::Array(Vec::new())),
            ),
            "approved_by_operators": payload
                .get("approved_by_operators")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
            "policy": sanitize_appsec_projection_json(payload.get("policy").cloned().unwrap_or(Value::Null)),
            "updated_at": updated_at,
            "source": "ctox-appsec-core-projection",
        });
        let updated_at_ms = appsec_updated_at_ms(&updated_at);
        upsert_business_record(business, "appsec_approvals", &id, updated_at_ms, record)?;
        pairs.push(("appsec_approvals", id));
    }
    Ok(())
}

fn parse_json_value(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or(Value::Null)
}

fn appsec_updated_at_ms(value: &str) -> i64 {
    value.parse::<i64>().unwrap_or_else(|_| now_ms() as i64)
}

fn summarize_appsec_command_json(value: &Value) -> Value {
    if let Some(items) = value.as_array() {
        return serde_json::json!({
            "argv0": items.first().and_then(Value::as_str).unwrap_or(""),
            "arg_count": items.len(),
        });
    }
    if let Some(object) = value.as_object() {
        return serde_json::json!({
            "program": object.get("program").and_then(Value::as_str).unwrap_or(""),
            "arg_count": object.get("args").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        });
    }
    Value::Null
}

fn sanitize_appsec_projection_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, item) in map {
                let lowered = key.to_ascii_lowercase();
                if lowered.contains("cookie")
                    || lowered.contains("token")
                    || lowered.contains("authorization")
                    || lowered.contains("password")
                    || lowered.contains("secret")
                    || lowered.contains("private_key")
                    || lowered.contains("screenshot")
                    || lowered.contains("raw_stream")
                    || lowered == "body"
                    || lowered == "headers"
                {
                    out.insert(key, Value::String("[redacted]".to_string()));
                } else {
                    out.insert(key, sanitize_appsec_projection_json(item));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(sanitize_appsec_projection_json)
                .collect(),
        ),
        other => other,
    }
}

fn stable_record_id(prefix: &str, value: &str) -> String {
    format!("{prefix}_{}", hex_sha256(value.as_bytes()))
}

/// Tombstone-aware `business_records` upsert for iot projection rows. Unlike
/// `upsert_business_record` (which always forces `_deleted:false`), this honors a
/// `_deleted: true` payload so deletion tombstones set the `deleted` column and
/// reach RxDB as a doc removal.
fn upsert_iot_projection_row(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    updated_at_ms: i64,
    mut payload: Value,
) -> anyhow::Result<()> {
    let deleted = payload
        .get("_deleted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let rev = format!("rev_{}", Uuid::new_v4());
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("id".to_string(), Value::String(record_id.to_string()));
        obj.insert("_rev".to_string(), Value::String(rev.clone()));
        obj.insert("_deleted".to_string(), Value::Bool(deleted));
        obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    }
    conn.execute(
        "INSERT INTO business_records
            (collection, record_id, rev, deleted, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(collection, record_id) DO UPDATE SET
            rev = excluded.rev,
            deleted = excluded.deleted,
            updated_at_ms = excluded.updated_at_ms,
            payload_json = excluded.payload_json",
        params![
            collection,
            record_id,
            rev,
            if deleted { 1 } else { 0 },
            updated_at_ms,
            serde_json::to_string(&payload)?
        ],
    )?;
    Ok(())
}

/// Read a single `business_records` row from the core db (ctox.sqlite3). Used
/// only to mirror executor-written iot_datapoints window rows (which the
/// projector cannot re-derive) into the business-os store.
fn read_core_db_business_record(
    root: &Path,
    collection: &str,
    record_id: &str,
) -> anyhow::Result<Option<Value>> {
    let path = crate::paths::core_db(root);
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open core db {}", path.display()))?;
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='business_records'",
            [],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Ok(None);
    }
    let payload_json: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM business_records WHERE collection = ?1 AND record_id = ?2",
            params![collection, record_id],
            |row| row.get(0),
        )
        .optional()?;
    match payload_json {
        Some(json) => Ok(Some(serde_json::from_str(&json).with_context(|| {
            format!("invalid core db business_record {collection}/{record_id}")
        })?)),
        None => Ok(None),
    }
}

pub(in crate::business_os) fn find_queue_task_for_command(
    root: &Path,
    command_id: &str,
) -> Option<String> {
    let command_id = command_id.trim();
    if command_id.is_empty() {
        return None;
    }
    if let Some(task_id) = channels::inspect_business_command(root, command_id)
        .ok()
        .flatten()
        .and_then(|context| {
            context
                .get("execution_task_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
    {
        return Some(task_id);
    }
    if let Some(task) = channels::load_queue_task_for_business_os_command(root, command_id)
        .ok()
        .flatten()
    {
        return Some(task.message_key);
    }
    None
}
