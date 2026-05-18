// Origin: CTOX
// License: Apache-2.0
//
// Tier 2.4 — Multi-Perspective Conformance.
//
// Standard PM control-flow conformance only checks "did transition X→Y happen
// when allowed?". Multi-perspective adds *data constraints*: e.g. founder
// outbound Send requires review_audit_key, body-hash equality, recipient-hash
// equality. CTOX has those constraints already encoded in csm::validate_transition;
// this tier turns them into measurable conformance statistics:
//
//   * per (entity_type, lane, from→to) pair: how many proofs were rejected
//     and which violation_code dominates;
//   * coverage of the declared evidence policy: what fraction of proofs
//     actually carried the declared evidence fields;
//   * mapping rule firing: which inference rules are pulling weight, which
//     are dead.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Options {
    pub entity_type: Option<String>,
    pub limit: i64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            entity_type: None,
            limit: 50,
        }
    }
}

impl Options {
    pub fn from_args(args: &[String]) -> Self {
        Self {
            entity_type: super::parse_string_flag(args, "--entity-type").map(str::to_string),
            limit: super::parse_i64_flag(args, "--limit", 50).clamp(1, 1000),
        }
    }
}

pub fn analyze(conn: &Connection, opts: &Options) -> Result<Value> {
    let constraint_coverage = constraint_coverage(conn, opts)?;
    let evidence_presence = evidence_presence(conn, opts)?;
    let rule_firing = rule_firing(conn, opts)?;
    Ok(json!({
        "ok": true,
        "tier": "2.4",
        "algorithm": "multi-perspective-conformance",
        "options": {
            "entity_type": opts.entity_type,
            "limit": opts.limit,
        },
        "constraint_coverage": constraint_coverage,
        "evidence_presence": evidence_presence,
        "rule_firing": rule_firing,
    }))
}

fn constraint_coverage(conn: &Connection, opts: &Options) -> Result<Vec<Value>> {
    // Per (entity_type, lane, from_state, to_state): counts of accepted,
    // rejected, dominant violation. Output is sorted by rejected-count DESC.
    let (sql, has_filter) = if opts.entity_type.is_some() {
        (
            r#"
            SELECT entity_type, lane, from_state, to_state,
                   SUM(CASE WHEN accepted = 1 THEN 1 ELSE 0 END) AS accepted_count,
                   SUM(CASE WHEN accepted = 0 THEN 1 ELSE 0 END) AS rejected_count,
                   COUNT(*) AS total_count
            FROM ctox_core_transition_proofs
            WHERE entity_type = ?1
            GROUP BY entity_type, lane, from_state, to_state
            ORDER BY rejected_count DESC, total_count DESC
            LIMIT ?2
            "#,
            true,
        )
    } else {
        (
            r#"
            SELECT entity_type, lane, from_state, to_state,
                   SUM(CASE WHEN accepted = 1 THEN 1 ELSE 0 END) AS accepted_count,
                   SUM(CASE WHEN accepted = 0 THEN 1 ELSE 0 END) AS rejected_count,
                   COUNT(*) AS total_count
            FROM ctox_core_transition_proofs
            GROUP BY entity_type, lane, from_state, to_state
            ORDER BY rejected_count DESC, total_count DESC
            LIMIT ?1
            "#,
            false,
        )
    };
    let mut stmt = conn.prepare(sql)?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(
        String, String, String, String, i64, i64, i64,
    )> {
        Ok((
            row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
            row.get(4)?, row.get(5)?, row.get(6)?,
        ))
    };
    let rows: Vec<(String, String, String, String, i64, i64, i64)> = if has_filter {
        let entity = opts.entity_type.as_deref().unwrap();
        stmt.query_map(params![entity, opts.limit], map_row)?
            .collect::<rusqlite::Result<_>>()?
    } else {
        stmt.query_map(params![opts.limit], map_row)?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut out = Vec::new();
    for (et, lane, from, to, accepted, rejected, total) in rows {
        let dominant_code =
            dominant_violation_code(conn, &et, &lane, &from, &to).unwrap_or_default();
        let acceptance = if total == 0 {
            0.0
        } else {
            accepted as f64 / total as f64
        };
        out.push(json!({
            "entity_type": et,
            "lane": lane,
            "from_state": from,
            "to_state": to,
            "accepted": accepted,
            "rejected": rejected,
            "total": total,
            "acceptance_ratio": (acceptance * 10_000.0).round() / 10_000.0,
            "dominant_violation_code": dominant_code,
        }));
    }
    Ok(out)
}

fn dominant_violation_code(
    conn: &Connection,
    entity_type: &str,
    lane: &str,
    from: &str,
    to: &str,
) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT violation_codes_json
        FROM ctox_core_transition_proofs
        WHERE entity_type = ?1 AND lane = ?2 AND from_state = ?3 AND to_state = ?4
          AND accepted = 0
        "#,
    )?;
    let rows: Vec<String> = stmt
        .query_map(params![entity_type, lane, from, to], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut counts: HashMap<String, i64> = HashMap::new();
    for raw in rows {
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            if let Some(arr) = value.as_array() {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        *counts.entry(s.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    Ok(counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(code, _)| code))
}

fn evidence_presence(conn: &Connection, opts: &Options) -> Result<Vec<Value>> {
    // Sample the most recent proofs and inspect whether the declared evidence
    // keys (review_audit_key, approved_body_sha256, etc.) are present in the
    // request_json. This shows whether the harness is consistently *delivering*
    // evidence even on accepted proofs.
    let limit = (opts.limit * 4).clamp(50, 2000);
    let (sql, has_filter) = if opts.entity_type.is_some() {
        (
            r#"
            SELECT entity_type, request_json
            FROM ctox_core_transition_proofs
            WHERE entity_type = ?1
            ORDER BY updated_at DESC
            LIMIT ?2
            "#,
            true,
        )
    } else {
        (
            r#"
            SELECT entity_type, request_json
            FROM ctox_core_transition_proofs
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
            false,
        )
    };
    let mut stmt = conn.prepare(sql)?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(String, String)> {
        Ok((row.get(0)?, row.get(1)?))
    };
    let rows: Vec<(String, String)> = if has_filter {
        let entity = opts.entity_type.as_deref().unwrap();
        stmt.query_map(params![entity, limit], map_row)?
            .collect::<rusqlite::Result<_>>()?
    } else {
        stmt.query_map(params![limit], map_row)?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut by_entity: HashMap<String, EvidenceCounter> = HashMap::new();
    for (et, raw) in rows {
        let counter = by_entity.entry(et).or_default();
        counter.total += 1;
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            if let Some(evidence) = value.get("evidence") {
                for key in EVIDENCE_KEYS {
                    if value_is_present(evidence.get(*key)) {
                        *counter.present.entry((*key).to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for (et, counter) in by_entity {
        let mut keys: Vec<Value> = counter
            .present
            .into_iter()
            .map(|(key, count)| {
                let ratio = if counter.total == 0 {
                    0.0
                } else {
                    count as f64 / counter.total as f64
                };
                json!({
                    "evidence_key": key,
                    "present_count": count,
                    "presence_ratio": (ratio * 10_000.0).round() / 10_000.0,
                })
            })
            .collect();
        keys.sort_by(|a, b| {
            b["presence_ratio"]
                .as_f64()
                .unwrap_or(0.0)
                .partial_cmp(&a["presence_ratio"].as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.push(json!({
            "entity_type": et,
            "samples": counter.total,
            "evidence_keys": keys,
        }));
    }
    out.sort_by(|a, b| {
        b["samples"]
            .as_i64()
            .unwrap_or(0)
            .cmp(&a["samples"].as_i64().unwrap_or(0))
    });
    Ok(out)
}

#[derive(Default)]
struct EvidenceCounter {
    total: i64,
    present: HashMap<String, i64>,
}

const EVIDENCE_KEYS: &[&str] = &[
    "review_audit_key",
    "approved_body_sha256",
    "approved_recipient_set_sha256",
    "outgoing_body_sha256",
    "outgoing_recipient_set_sha256",
    "verification_id",
    "schedule_task_id",
    "replacement_schedule_task_id",
    "escalation_id",
    "knowledge_entry_id",
    "incident_id",
    "canonical_hot_path",
    "expected_artifact_refs",
    "delivered_artifact_refs",
];

fn value_is_present(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Null) | None => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn rule_firing(conn: &Connection, opts: &Options) -> Result<Vec<Value>> {
    // Compares each declared transition rule against how many events its
    // petri_transition_id has been associated with in the audit table.
    let mut stmt = conn.prepare(
        r#"
        SELECT r.rule_id, r.priority, r.core_entity_type, r.runtime_lane, r.petri_transition_id,
               r.enabled,
               COALESCE(a.audit_count, 0) AS audit_count
        FROM ctox_pm_core_transition_rules r
        LEFT JOIN (
            SELECT rule_id, COUNT(*) AS audit_count
            FROM ctox_pm_core_transition_audit
            GROUP BY rule_id
        ) a ON a.rule_id = r.rule_id
        ORDER BY audit_count DESC, r.priority ASC, r.rule_id
        LIMIT ?1
        "#,
    )?;
    let rows: Vec<Value> = stmt
        .query_map(params![opts.limit], |row| {
            Ok(json!({
                "rule_id": row.get::<_, String>(0)?,
                "priority": row.get::<_, i64>(1)?,
                "core_entity_type": row.get::<_, String>(2)?,
                "runtime_lane": row.get::<_, String>(3)?,
                "petri_transition_id": row.get::<_, String>(4)?,
                "enabled": row.get::<_, i64>(5)? == 1,
                "audit_count": row.get::<_, i64>(6)?,
            }))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE ctox_core_transition_proofs (
                proof_id TEXT PRIMARY KEY,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                lane TEXT NOT NULL,
                from_state TEXT NOT NULL,
                to_state TEXT NOT NULL,
                core_event TEXT NOT NULL,
                actor TEXT NOT NULL,
                accepted INTEGER NOT NULL,
                violation_codes_json TEXT NOT NULL DEFAULT '[]',
                request_json TEXT NOT NULL DEFAULT '{}',
                report_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE ctox_pm_core_transition_audit (
                audit_id TEXT PRIMARY KEY,
                event_id TEXT NOT NULL,
                case_id TEXT NOT NULL,
                rule_id TEXT,
                petri_transition_id TEXT,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                lane TEXT NOT NULL,
                from_state TEXT NOT NULL,
                to_state TEXT NOT NULL,
                core_event TEXT NOT NULL,
                accepted INTEGER NOT NULL,
                violation_codes_json TEXT NOT NULL DEFAULT '[]',
                proof_id TEXT,
                request_json TEXT NOT NULL DEFAULT '{}',
                observed_at TEXT NOT NULL,
                scanned_at TEXT NOT NULL
            );
            CREATE TABLE ctox_pm_core_transition_rules (
                rule_id TEXT PRIMARY KEY,
                priority INTEGER NOT NULL,
                table_pattern TEXT,
                entity_type_pattern TEXT,
                operation_pattern TEXT,
                activity_pattern TEXT,
                inference_kind TEXT NOT NULL,
                core_entity_type TEXT NOT NULL,
                runtime_lane TEXT NOT NULL,
                petri_transition_id TEXT NOT NULL,
                evidence_policy_json TEXT NOT NULL DEFAULT '{}',
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .unwrap();
        conn
    }

    #[test]
    fn constraint_coverage_groups_and_finds_dominant_code() {
        let conn = setup_conn();
        for i in 0..5 {
            conn.execute(
                r#"INSERT INTO ctox_core_transition_proofs
                    (proof_id, entity_type, entity_id, lane, from_state, to_state,
                     core_event, actor, accepted, violation_codes_json, created_at, updated_at)
                   VALUES (?1, 'FounderCommunication', 'e', 'P0FounderCommunication',
                           'Approved', 'Sending', 'Send', 'u', 0,
                           '["founder_send_body_hash_mismatch"]',
                           ?2, ?2)"#,
                params![format!("r{i}"), format!("2026-04-26T01:00:0{i}Z")],
            )
            .unwrap();
        }
        conn.execute(
            r#"INSERT INTO ctox_core_transition_proofs
                (proof_id, entity_type, entity_id, lane, from_state, to_state,
                 core_event, actor, accepted, violation_codes_json, created_at, updated_at)
               VALUES ('a1', 'FounderCommunication', 'e', 'P0FounderCommunication',
                       'Approved', 'Sending', 'Send', 'u', 1, '[]',
                       '2026-04-26T02:00:00Z', '2026-04-26T02:00:00Z')"#,
            [],
        )
        .unwrap();

        let report = constraint_coverage(&conn, &Options::default()).unwrap();
        assert_eq!(report.len(), 1);
        let row = &report[0];
        assert_eq!(row["rejected"], 5);
        assert_eq!(row["accepted"], 1);
        assert_eq!(
            row["dominant_violation_code"],
            "founder_send_body_hash_mismatch"
        );
    }

    #[test]
    fn evidence_presence_counts_review_audit_key() {
        let conn = setup_conn();
        conn.execute(
            r#"INSERT INTO ctox_core_transition_proofs
                (proof_id, entity_type, entity_id, lane, from_state, to_state,
                 core_event, actor, accepted, request_json, created_at, updated_at)
               VALUES ('p1', 'FounderCommunication', 'e', 'L', 'A', 'B', 'X', 'u', 1,
                       '{"evidence":{"review_audit_key":"abc","approved_body_sha256":"x","expected_artifact_refs":[{"kind":"OutboundEmail","primary_key":"thread:t","expected_terminal_state":"accepted"}],"delivered_artifact_refs":[{"kind":"OutboundEmail","primary_key":"msg-1","expected_terminal_state":"accepted"}]}}',
                       '2026-04-26T01:00:00Z', '2026-04-26T01:00:00Z')"#,
            [],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO ctox_core_transition_proofs
                (proof_id, entity_type, entity_id, lane, from_state, to_state,
                 core_event, actor, accepted, request_json, created_at, updated_at)
               VALUES ('p2', 'FounderCommunication', 'e', 'L', 'A', 'B', 'X', 'u', 0,
                       '{"evidence":{"review_audit_key":null}}',
                       '2026-04-26T01:00:01Z', '2026-04-26T01:00:01Z')"#,
            [],
        )
        .unwrap();
        let report = evidence_presence(&conn, &Options::default()).unwrap();
        assert_eq!(report.len(), 1);
        let keys = report[0]["evidence_keys"].as_array().unwrap();
        let review = keys
            .iter()
            .find(|k| k["evidence_key"] == "review_audit_key")
            .unwrap();
        assert_eq!(review["present_count"], 1); // only the accepted one had a non-null
        let delivered = keys
            .iter()
            .find(|k| k["evidence_key"] == "delivered_artifact_refs")
            .unwrap();
        assert_eq!(delivered["present_count"], 1);
    }
}
