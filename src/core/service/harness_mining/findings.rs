// Origin: CTOX
// License: Apache-2.0
//
// Findings — first-class persistence layer for harness-mining results.
//
// The audit-tick runs `brief` periodically and writes structured findings
// here with a lifecycle:
//
//   detected      → first observation; not yet acted on
//   confirmed     → seen at two consecutive ticks (2-tick confirmation gate)
//   acknowledged  → agent or operator has read the finding
//   mitigated     → corrective action applied (queue block, spec change, etc.)
//   verified      → post-mitigation tick shows the finding cleared
//   stale         → not observed at the most recent tick before confirmation
//
// Other subsystems (queue_repair, self-diagnose, skills) read findings as
// SQL — they do not call into harness_mining functions. That keeps the
// dependency direction clean: harness_mining writes, everyone else reads.
//
// Identity: a finding has a `signature` derived from (kind, entity_type,
// entity_id, lane). At each tick, `record_or_confirm` looks up the latest
// non-resolved finding with that signature and either inserts (first time),
// confirms (second consecutive tick), or refreshes the row. Resolved
// findings (mitigated/verified/stale) do not block a fresh detection.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub fn ensure_findings_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS ctox_hm_findings (
            finding_id TEXT PRIMARY KEY,
            signature TEXT NOT NULL,
            kind TEXT NOT NULL,
            severity TEXT NOT NULL,
            entity_type TEXT,
            entity_id TEXT,
            lane TEXT,
            evidence_json TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL,
            detected_at TEXT NOT NULL,
            last_seen_at TEXT NOT NULL,
            confirmed_at TEXT,
            acknowledged_at TEXT,
            mitigated_at TEXT,
            verified_at TEXT,
            resolved_by TEXT,
            note TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_ctox_hm_findings_signature_status
          ON ctox_hm_findings(signature, status, last_seen_at DESC);
        CREATE INDEX IF NOT EXISTS idx_ctox_hm_findings_status_seen
          ON ctox_hm_findings(status, last_seen_at DESC);
        CREATE TABLE IF NOT EXISTS ctox_hm_audit_runs (
            run_id TEXT PRIMARY KEY,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            status TEXT NOT NULL,
            brief_json TEXT NOT NULL DEFAULT '{}',
            findings_recorded INTEGER NOT NULL DEFAULT 0,
            findings_confirmed INTEGER NOT NULL DEFAULT 0,
            findings_marked_stale INTEGER NOT NULL DEFAULT 0,
            error_text TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_ctox_hm_audit_runs_started
          ON ctox_hm_audit_runs(started_at DESC);
        "#,
    )?;
    Ok(())
}

/// Record a fresh observation at the current audit tick. Returns the
/// finding_id and whether it was just confirmed (i.e. transitioned from
/// `detected` → `confirmed`).
pub fn record_or_confirm(
    conn: &Connection,
    kind: &str,
    severity: &str,
    entity_type: Option<&str>,
    entity_id: Option<&str>,
    lane: Option<&str>,
    evidence: &Value,
    now_iso: &str,
) -> Result<RecordOutcome> {
    let signature = signature_for(kind, entity_type, entity_id, lane);
    let existing: Option<(String, String)> = conn
        .query_row(
            r#"
            SELECT finding_id, status
            FROM ctox_hm_findings
            WHERE signature = ?1
              AND status NOT IN ('mitigated','verified','stale')
            ORDER BY last_seen_at DESC
            LIMIT 1
            "#,
            params![signature],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    if let Some((finding_id, status)) = existing {
        let new_status = if status == "detected" {
            "confirmed"
        } else {
            status.as_str()
        };
        let confirmed_at = if status == "detected" {
            Some(now_iso)
        } else {
            None
        };
        conn.execute(
            r#"
            UPDATE ctox_hm_findings
            SET evidence_json = ?2,
                last_seen_at = ?3,
                severity = CASE WHEN severity = 'critical' THEN 'critical' ELSE ?4 END,
                status = ?5,
                confirmed_at = COALESCE(confirmed_at, ?6)
            WHERE finding_id = ?1
            "#,
            params![
                finding_id,
                serde_json::to_string(evidence)?,
                now_iso,
                severity,
                new_status,
                confirmed_at,
            ],
        )?;
        return Ok(RecordOutcome {
            finding_id,
            confirmed_now: status == "detected",
            inserted: false,
        });
    }
    let finding_id = format!("hmf-{}", sha8(&format!("{signature}|{now_iso}")));
    conn.execute(
        r#"
        INSERT INTO ctox_hm_findings
          (finding_id, signature, kind, severity, entity_type, entity_id, lane,
           evidence_json, status, detected_at, last_seen_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'detected', ?9, ?9)
        "#,
        params![
            finding_id,
            signature,
            kind,
            severity,
            entity_type,
            entity_id,
            lane,
            serde_json::to_string(evidence)?,
            now_iso,
        ],
    )?;
    Ok(RecordOutcome {
        finding_id,
        confirmed_now: false,
        inserted: true,
    })
}

/// At end of an audit tick: any finding with status in (detected, confirmed)
/// whose `last_seen_at` is older than this tick's start is marked `stale`.
/// Returns the number of rows transitioned. Non-confirmed findings going
/// stale is *expected* — that is the point of the 2-tick gate.
pub fn mark_unseen_stale(conn: &Connection, tick_started_at: &str) -> Result<i64> {
    let n = conn.execute(
        r#"
        UPDATE ctox_hm_findings
        SET status = 'stale',
            resolved_by = COALESCE(resolved_by, 'audit-tick:no-longer-observed')
        WHERE status IN ('detected','confirmed')
          AND last_seen_at < ?1
        "#,
        params![tick_started_at],
    )?;
    Ok(n as i64)
}

pub fn list(
    conn: &Connection,
    status: Option<&str>,
    kind: Option<&str>,
    limit: i64,
) -> Result<Vec<Value>> {
    let limit = limit.clamp(1, 1000);
    let mut sql = String::from(
        r#"
        SELECT finding_id, signature, kind, severity, entity_type, entity_id, lane,
               evidence_json, status, detected_at, last_seen_at, confirmed_at,
               acknowledged_at, mitigated_at, verified_at, resolved_by, note
        FROM ctox_hm_findings
        "#,
    );
    let mut where_clauses: Vec<String> = Vec::new();
    let mut bound: Vec<String> = Vec::new();
    if let Some(s) = status {
        where_clauses.push(format!("status = ?{}", bound.len() + 1));
        bound.push(s.to_string());
    }
    if let Some(k) = kind {
        where_clauses.push(format!("kind = ?{}", bound.len() + 1));
        bound.push(k.to_string());
    }
    if !where_clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&where_clauses.join(" AND "));
    }
    sql.push_str(&format!(
        " ORDER BY last_seen_at DESC LIMIT ?{}",
        bound.len() + 1
    ));
    let mut stmt = conn.prepare(&sql)?;
    let bound_refs: Vec<&dyn rusqlite::ToSql> = bound
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect::<Vec<_>>();
    let mut all_params: Vec<&dyn rusqlite::ToSql> = bound_refs;
    all_params.push(&limit);
    let rows = stmt
        .query_map(&*all_params, |row| {
            Ok(json!({
                "finding_id": row.get::<_, String>(0)?,
                "signature": row.get::<_, String>(1)?,
                "kind": row.get::<_, String>(2)?,
                "severity": row.get::<_, String>(3)?,
                "entity_type": row.get::<_, Option<String>>(4)?,
                "entity_id": row.get::<_, Option<String>>(5)?,
                "lane": row.get::<_, Option<String>>(6)?,
                "evidence_json": row.get::<_, String>(7)?,
                "status": row.get::<_, String>(8)?,
                "detected_at": row.get::<_, String>(9)?,
                "last_seen_at": row.get::<_, String>(10)?,
                "confirmed_at": row.get::<_, Option<String>>(11)?,
                "acknowledged_at": row.get::<_, Option<String>>(12)?,
                "mitigated_at": row.get::<_, Option<String>>(13)?,
                "verified_at": row.get::<_, Option<String>>(14)?,
                "resolved_by": row.get::<_, Option<String>>(15)?,
                "note": row.get::<_, Option<String>>(16)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn acknowledge(
    conn: &Connection,
    finding_id: &str,
    note: Option<&str>,
    now_iso: &str,
) -> Result<()> {
    let n = conn.execute(
        r#"
        UPDATE ctox_hm_findings
        SET status = 'acknowledged',
            acknowledged_at = ?2,
            note = COALESCE(?3, note)
        WHERE finding_id = ?1
          AND status IN ('detected','confirmed')
        "#,
        params![finding_id, now_iso, note],
    )?;
    if n == 0 {
        anyhow::bail!("no detected/confirmed finding with id {finding_id}");
    }
    Ok(())
}

pub fn mitigate(
    conn: &Connection,
    finding_id: &str,
    by: &str,
    note: Option<&str>,
    now_iso: &str,
) -> Result<()> {
    let n = conn.execute(
        r#"
        UPDATE ctox_hm_findings
        SET status = 'mitigated',
            mitigated_at = ?2,
            resolved_by = ?3,
            note = COALESCE(?4, note)
        WHERE finding_id = ?1
          AND status IN ('detected','confirmed','acknowledged')
        "#,
        params![finding_id, now_iso, by, note],
    )?;
    if n == 0 {
        anyhow::bail!(
            "finding {finding_id} not in detected/confirmed/acknowledged state, cannot mitigate"
        );
    }
    Ok(())
}

pub fn verify(
    conn: &Connection,
    finding_id: &str,
    note: Option<&str>,
    now_iso: &str,
) -> Result<()> {
    let n = conn.execute(
        r#"
        UPDATE ctox_hm_findings
        SET status = 'verified',
            verified_at = ?2,
            note = COALESCE(?3, note)
        WHERE finding_id = ?1
          AND status = 'mitigated'
        "#,
        params![finding_id, now_iso, note],
    )?;
    if n == 0 {
        anyhow::bail!("finding {finding_id} not in mitigated state, cannot verify");
    }
    Ok(())
}

pub struct AuditTickReport {
    pub run_id: String,
    pub recorded: i64,
    pub confirmed: i64,
    pub stale: i64,
    pub brief: Value,
}

/// Run a full audit tick:
/// 1. Synthesize a brief.
/// 2. For each finding in the brief, record_or_confirm.
/// 3. Mark anything not seen at this tick as stale.
/// 4. Persist a summary row in ctox_hm_audit_runs.
pub fn run_audit_tick(
    conn: &Connection,
    opts: &super::brief::Options,
    now_iso: &str,
) -> Result<AuditTickReport> {
    ensure_findings_schema(conn)?;
    let run_id = format!("hmr-{}", sha8(&format!("audit|{now_iso}")));
    let started_at = now_iso.to_string();
    conn.execute(
        r#"
        INSERT INTO ctox_hm_audit_runs (run_id, started_at, status)
        VALUES (?1, ?2, 'running')
        "#,
        params![run_id, started_at],
    )?;

    let outcome = (|| -> Result<(Value, i64, i64)> {
        let brief = super::brief::synthesize(conn, opts)?;
        let mut recorded = 0i64;
        let mut confirmed = 0i64;
        if let Some(findings) = brief["findings"].as_array() {
            for f in findings {
                let kind = f["kind"].as_str().unwrap_or("unknown").to_string();
                let severity = f["severity"].as_str().unwrap_or("warning").to_string();
                let (entity_type, entity_id, lane) = extract_anchor(f);
                let r = record_or_confirm(
                    conn,
                    &kind,
                    &severity,
                    entity_type.as_deref(),
                    entity_id.as_deref(),
                    lane.as_deref(),
                    f,
                    now_iso,
                )?;
                if r.inserted {
                    recorded += 1;
                }
                if r.confirmed_now {
                    confirmed += 1;
                }
            }
        }
        Ok((brief, recorded, confirmed))
    })();

    match outcome {
        Ok((brief, recorded, confirmed)) => {
            let stale = mark_unseen_stale(conn, &started_at)?;
            conn.execute(
                r#"
                UPDATE ctox_hm_audit_runs
                SET finished_at = ?2,
                    status = 'completed',
                    brief_json = ?3,
                    findings_recorded = ?4,
                    findings_confirmed = ?5,
                    findings_marked_stale = ?6
                WHERE run_id = ?1
                "#,
                params![
                    run_id,
                    now_iso,
                    serde_json::to_string(&brief)?,
                    recorded,
                    confirmed,
                    stale,
                ],
            )?;
            Ok(AuditTickReport {
                run_id,
                recorded,
                confirmed,
                stale,
                brief,
            })
        }
        Err(e) => {
            let msg = e.to_string();
            conn.execute(
                r#"
                UPDATE ctox_hm_audit_runs
                SET finished_at = ?2,
                    status = 'failed',
                    error_text = ?3
                WHERE run_id = ?1
                "#,
                params![run_id, now_iso, msg],
            )?;
            Err(e)
        }
    }
}

fn extract_anchor(f: &Value) -> (Option<String>, Option<String>, Option<String>) {
    // Look in worst_case (stuck_cases), top_breach (conformance), state (bottleneck),
    // top_driver (drift) for entity-bound anchors.
    let candidates = [
        f.get("worst_case"),
        f.get("top_breach"),
        f.get("state"),
        f.get("top_driver"),
    ];
    for c in candidates.into_iter().flatten() {
        let entity_type = c
            .get("entity_type")
            .and_then(Value::as_str)
            .map(str::to_string);
        let entity_id = c
            .get("entity_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let lane = c.get("lane").and_then(Value::as_str).map(str::to_string);
        if entity_type.is_some() || entity_id.is_some() || lane.is_some() {
            return (entity_type, entity_id, lane);
        }
    }
    (None, None, None)
}

fn signature_for(
    kind: &str,
    entity_type: Option<&str>,
    entity_id: Option<&str>,
    lane: Option<&str>,
) -> String {
    let raw = format!(
        "{}|{}|{}|{}",
        kind,
        entity_type.unwrap_or("-"),
        entity_id.unwrap_or("-"),
        lane.unwrap_or("-"),
    );
    sha16(&raw)
}

fn sha8(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(input.as_bytes());
    let mut out = String::with_capacity(16);
    for b in &digest[..8] {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn sha16(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(input.as_bytes());
    let mut out = String::with_capacity(32);
    for b in &digest[..16] {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[derive(Debug, Clone)]
pub struct RecordOutcome {
    pub finding_id: String,
    pub confirmed_now: bool,
    pub inserted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ensure_findings_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn first_observation_inserts_with_status_detected() {
        let conn = setup();
        let r = record_or_confirm(
            &conn,
            "stuck_cases",
            "critical",
            Some("FounderCommunication"),
            Some("e1"),
            Some("P0FounderCommunication"),
            &json!({"case_count": 3}),
            "2026-04-27T00:00:00Z",
        )
        .unwrap();
        assert!(r.inserted);
        assert!(!r.confirmed_now);
        let rows = list(&conn, None, None, 100).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["status"], "detected");
    }

    #[test]
    fn second_observation_confirms() {
        let conn = setup();
        record_or_confirm(
            &conn,
            "stuck_cases",
            "warning",
            Some("X"),
            Some("e1"),
            None,
            &json!({}),
            "2026-04-27T00:00:00Z",
        )
        .unwrap();
        let r = record_or_confirm(
            &conn,
            "stuck_cases",
            "warning",
            Some("X"),
            Some("e1"),
            None,
            &json!({}),
            "2026-04-27T00:05:00Z",
        )
        .unwrap();
        assert!(!r.inserted);
        assert!(r.confirmed_now);
        let rows = list(&conn, Some("confirmed"), None, 100).unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn third_observation_does_not_re_confirm() {
        let conn = setup();
        for ts in &[
            "2026-04-27T00:00:00Z",
            "2026-04-27T00:05:00Z",
            "2026-04-27T00:10:00Z",
        ] {
            record_or_confirm(
                &conn,
                "stuck_cases",
                "warning",
                Some("X"),
                Some("e1"),
                None,
                &json!({}),
                ts,
            )
            .unwrap();
        }
        let confirmed = list(&conn, Some("confirmed"), None, 100).unwrap();
        let detected = list(&conn, Some("detected"), None, 100).unwrap();
        assert_eq!(confirmed.len(), 1);
        assert_eq!(detected.len(), 0);
    }

    #[test]
    fn mark_unseen_stale_only_drops_old_observations() {
        let conn = setup();
        record_or_confirm(
            &conn,
            "stuck_cases",
            "warning",
            Some("X"),
            Some("e1"),
            None,
            &json!({}),
            "2026-04-27T00:00:00Z",
        )
        .unwrap();
        record_or_confirm(
            &conn,
            "drift",
            "warning",
            None,
            None,
            None,
            &json!({}),
            "2026-04-27T00:01:00Z",
        )
        .unwrap();
        let n = mark_unseen_stale(&conn, "2026-04-27T00:00:30Z").unwrap();
        assert_eq!(n, 1);
        let stale = list(&conn, Some("stale"), None, 100).unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0]["kind"], "stuck_cases");
    }

    #[test]
    fn lifecycle_transitions_acknowledge_mitigate_verify() {
        let conn = setup();
        let r = record_or_confirm(
            &conn,
            "stuck_cases",
            "warning",
            Some("X"),
            Some("e1"),
            None,
            &json!({}),
            "2026-04-27T00:00:00Z",
        )
        .unwrap();
        let id = r.finding_id;
        acknowledge(&conn, &id, Some("agent reviewed"), "2026-04-27T00:01:00Z").unwrap();
        mitigate(
            &conn,
            &id,
            "agent",
            Some("queue blocked"),
            "2026-04-27T00:02:00Z",
        )
        .unwrap();
        verify(&conn, &id, Some("loop stopped"), "2026-04-27T00:03:00Z").unwrap();
        let rows = list(&conn, Some("verified"), None, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["resolved_by"], "agent");
    }

    #[test]
    fn mitigate_refuses_for_unrelated_state() {
        let conn = setup();
        let r = record_or_confirm(
            &conn,
            "stuck_cases",
            "warning",
            Some("X"),
            Some("e1"),
            None,
            &json!({}),
            "2026-04-27T00:00:00Z",
        )
        .unwrap();
        let id = r.finding_id;
        mitigate(&conn, &id, "agent", None, "2026-04-27T00:01:00Z").unwrap();
        // second mitigate should now fail
        let result = mitigate(&conn, &id, "agent", None, "2026-04-27T00:02:00Z");
        assert!(result.is_err());
    }
}
