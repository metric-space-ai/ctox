// Origin: CTOX
// License: Apache-2.0
//
// Tier 1.1 — Stuck-Case- / Retry-Loop-Detection.
//
// Detects entities whose lifecycle has accumulated repeated rejected
// transition attempts (preventive layer kept blocking) or that have been
// idle in a non-terminal state for longer than a deadline. Fires on the
// real-world signature seen in forensics: ~60 entities accumulated >100
// rejected founder-send attempts each, with no automatic stop.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct Options {
    /// minimum number of rejected proofs per entity before it counts as stuck
    pub min_attempts: i64,
    /// optional idle threshold in seconds; 0 disables
    pub idle_seconds: i64,
    /// max rows returned
    pub limit: i64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            min_attempts: 5,
            idle_seconds: 0,
            limit: 200,
        }
    }
}

impl Options {
    pub fn from_args(args: &[String]) -> Self {
        let d = Self::default();
        Self {
            min_attempts: super::parse_i64_flag(args, "--min-attempts", d.min_attempts).max(1),
            idle_seconds: super::parse_i64_flag(args, "--idle-seconds", d.idle_seconds).max(0),
            limit: super::parse_i64_flag(args, "--limit", d.limit).clamp(1, 5000),
        }
    }
}

pub fn detect(conn: &Connection, opts: &Options) -> Result<Value> {
    // Build aggregated retry counts directly from the preventive proof ledger.
    // We only count *rejected* proofs (accepted = 0): a stuck case is one that
    // repeatedly tried to do something the state machine refuses.
    let mut stmt = conn.prepare(
        r#"
        SELECT
            entity_type,
            entity_id,
            lane,
            COUNT(*) AS rejected_attempts,
            MIN(created_at) AS first_attempt,
            MAX(updated_at) AS last_attempt,
            (
                SELECT violation_codes_json
                FROM ctox_core_transition_proofs p2
                WHERE p2.entity_type = p1.entity_type
                  AND p2.entity_id   = p1.entity_id
                  AND p2.accepted    = 0
                ORDER BY updated_at DESC
                LIMIT 1
            ) AS dominant_violations,
            (
                SELECT to_state
                FROM ctox_core_transition_proofs p3
                WHERE p3.entity_type = p1.entity_type
                  AND p3.entity_id   = p1.entity_id
                  AND p3.accepted    = 0
                ORDER BY updated_at DESC
                LIMIT 1
            ) AS last_attempted_to_state
        FROM ctox_core_transition_proofs p1
        WHERE accepted = 0
        GROUP BY entity_type, entity_id, lane
        HAVING COUNT(*) >= ?1
        ORDER BY rejected_attempts DESC, last_attempt DESC
        LIMIT ?2
        "#,
    )?;
    let rows = stmt
        .query_map(params![opts.min_attempts, opts.limit], |row| {
            Ok(StuckRow {
                entity_type: row.get(0)?,
                entity_id: row.get(1)?,
                lane: row.get(2)?,
                rejected_attempts: row.get(3)?,
                first_attempt: row.get(4)?,
                last_attempt: row.get(5)?,
                dominant_violations: row.get(6)?,
                last_attempted_to_state: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Optional idle filter: only keep rows whose last_attempt is older than N
    // seconds relative to "now" as observed by SQLite — gives a forensic view
    // of cases that have stopped moving entirely.
    let idle_filtered: Vec<&StuckRow> = if opts.idle_seconds > 0 {
        let cutoff: String = conn.query_row(
            "SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now',?1)",
            params![format!("-{} seconds", opts.idle_seconds)],
            |row| row.get(0),
        )?;
        rows.iter()
            .filter(|r| r.last_attempt.as_str() < cutoff.as_str())
            .collect()
    } else {
        rows.iter().collect()
    };

    let cases: Vec<Value> = idle_filtered
        .iter()
        .map(|r| {
            json!({
                "entity_type": r.entity_type,
                "entity_id": r.entity_id,
                "lane": r.lane,
                "rejected_attempts": r.rejected_attempts,
                "first_attempt": r.first_attempt,
                "last_attempt": r.last_attempt,
                "last_attempted_to_state": r.last_attempted_to_state,
                "dominant_violation_codes_json": r.dominant_violations,
            })
        })
        .collect();

    let by_violation = aggregate_by_top_violation(&idle_filtered);

    Ok(json!({
        "ok": true,
        "tier": "1.1",
        "algorithm": "stuck-case-retry-loop-detection",
        "options": {
            "min_attempts": opts.min_attempts,
            "idle_seconds": opts.idle_seconds,
            "limit": opts.limit,
        },
        "case_count": cases.len(),
        "cases": cases,
        "violation_buckets": by_violation,
    }))
}

#[derive(Debug)]
struct StuckRow {
    entity_type: String,
    entity_id: String,
    lane: String,
    rejected_attempts: i64,
    first_attempt: String,
    last_attempt: String,
    dominant_violations: String,
    last_attempted_to_state: String,
}

fn aggregate_by_top_violation(rows: &[&StuckRow]) -> Vec<Value> {
    use std::collections::HashMap;
    let mut buckets: HashMap<String, (i64, i64)> = HashMap::new();
    for r in rows {
        let top_code =
            first_violation_code(&r.dominant_violations).unwrap_or_else(|| "<unknown>".to_string());
        let entry = buckets.entry(top_code).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += r.rejected_attempts;
    }
    let mut sorted: Vec<_> = buckets.into_iter().collect();
    sorted.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));
    sorted
        .into_iter()
        .map(|(code, (entities, attempts))| {
            json!({
                "top_violation_code": code,
                "entities": entities,
                "total_rejected_attempts": attempts,
            })
        })
        .collect()
}

fn first_violation_code(json_text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(json_text).ok()?;
    value.as_array()?.first()?.as_str().map(|s| s.to_string())
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
            "#,
        )
        .unwrap();
        conn
    }

    fn insert_proof(
        conn: &Connection,
        proof_id: &str,
        entity_id: &str,
        accepted: i64,
        codes: &str,
        ts: &str,
    ) {
        conn.execute(
            r#"INSERT INTO ctox_core_transition_proofs
                (proof_id, entity_type, entity_id, lane, from_state, to_state,
                 core_event, actor, accepted, violation_codes_json, created_at, updated_at)
               VALUES (?1,'FounderCommunication',?2,'P0FounderCommunication',
                       'Approved','Sending','Send','upgrade',?3,?4,?5,?5)"#,
            params![proof_id, entity_id, accepted, codes, ts],
        )
        .unwrap();
    }

    #[test]
    fn stuck_cases_detects_repeated_rejections() {
        let conn = setup_conn();
        for i in 0..7 {
            insert_proof(
                &conn,
                &format!("p{i}"),
                "ent-A",
                0,
                "[\"founder_send_body_hash_mismatch\"]",
                &format!("2026-04-25T22:55:{:02}.000Z", i),
            );
        }
        // an accepted attempt for a different entity must not skew the result
        insert_proof(&conn, "px", "ent-OK", 1, "[]", "2026-04-25T22:55:00.000Z");

        let report = detect(
            &conn,
            &Options {
                min_attempts: 5,
                idle_seconds: 0,
                limit: 100,
            },
        )
        .unwrap();
        let cases = report["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0]["entity_id"], "ent-A");
        assert_eq!(cases[0]["rejected_attempts"], 7);
    }

    #[test]
    fn stuck_cases_respects_min_attempts() {
        let conn = setup_conn();
        for i in 0..3 {
            insert_proof(
                &conn,
                &format!("p{i}"),
                "ent-A",
                0,
                "[\"founder_send_body_hash_mismatch\"]",
                &format!("2026-04-25T22:55:{:02}.000Z", i),
            );
        }
        let report = detect(
            &conn,
            &Options {
                min_attempts: 5,
                idle_seconds: 0,
                limit: 100,
            },
        )
        .unwrap();
        assert_eq!(report["cases"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn stuck_cases_aggregates_violation_buckets() {
        let conn = setup_conn();
        for i in 0..6 {
            insert_proof(
                &conn,
                &format!("p{i}"),
                "ent-A",
                0,
                "[\"founder_send_body_hash_mismatch\"]",
                &format!("2026-04-25T22:55:{:02}.000Z", i),
            );
        }
        for i in 0..5 {
            insert_proof(
                &conn,
                &format!("q{i}"),
                "ent-B",
                0,
                "[\"founder_send_requires_review_audit\"]",
                &format!("2026-04-25T23:00:{:02}.000Z", i),
            );
        }
        let report = detect(&conn, &Options::default()).unwrap();
        let buckets = report["violation_buckets"].as_array().unwrap();
        assert_eq!(buckets.len(), 2);
        // the bucket with 6 attempts should sort first
        assert_eq!(
            buckets[0]["top_violation_code"],
            "founder_send_body_hash_mismatch"
        );
        assert_eq!(buckets[0]["total_rejected_attempts"], 6);
    }
}
