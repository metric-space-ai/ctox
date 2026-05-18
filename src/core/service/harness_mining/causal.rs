// Origin: CTOX
// License: Apache-2.0
//
// Tier 2.2 — Causal / Conditional Mining on Violation Predecessors.
//
// For a given violation_code, find which activities tend to occur in the
// last K events before the violation was detected. Compares the conditional
// probability P(activity | violation) against the marginal P(activity) and
// reports activities with the largest *lift* — these are causal hypothesis
// candidates for what *enabled* the violation.
//
// This is statistical association-mining, not formal causal inference, but
// for "why did this hash mismatch happen?" it's the right first cut.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Options {
    pub violation_code: Option<String>,
    pub lookback: i64,
    pub limit: i64,
    pub min_support: i64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            violation_code: None,
            lookback: 5,
            limit: 25,
            min_support: 3,
        }
    }
}

impl Options {
    pub fn from_args(args: &[String]) -> Self {
        let d = Self::default();
        Self {
            violation_code: super::parse_string_flag(args, "--violation-code").map(str::to_string),
            lookback: super::parse_i64_flag(args, "--lookback", d.lookback).clamp(1, 100),
            limit: super::parse_i64_flag(args, "--limit", d.limit).clamp(1, 200),
            min_support: super::parse_i64_flag(args, "--min-support", d.min_support).max(1),
        }
    }
}

pub fn analyze(conn: &Connection, opts: &Options) -> Result<Value> {
    let codes = if let Some(code) = &opts.violation_code {
        vec![code.clone()]
    } else {
        top_violation_codes(conn, 5)?
    };
    let mut by_code: Vec<Value> = Vec::new();
    let total_events = total_event_count(conn)?;
    let marginal = marginal_activity_distribution(conn, total_events)?;
    for code in &codes {
        let report = analyze_one_code(conn, code, opts, &marginal, total_events)?;
        by_code.push(report);
    }
    Ok(json!({
        "ok": true,
        "tier": "2.2",
        "algorithm": "causal-conditional-mining",
        "options": {
            "violation_code": opts.violation_code,
            "lookback": opts.lookback,
            "limit": opts.limit,
            "min_support": opts.min_support,
        },
        "total_events": total_events,
        "by_violation_code": by_code,
    }))
}

fn analyze_one_code(
    conn: &Connection,
    code: &str,
    opts: &Options,
    marginal: &HashMap<String, f64>,
    total_events: i64,
) -> Result<Value> {
    // Find every (case_id, detected_at) where this violation occurred.
    let mut stmt = conn.prepare(
        r#"
        SELECT case_id, detected_at
        FROM ctox_pm_state_violations
        WHERE violation_code = ?1
        ORDER BY detected_at DESC
        LIMIT 5000
        "#,
    )?;
    let occurrences: Vec<(String, String)> = stmt
        .query_map(params![code], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;

    if occurrences.is_empty() {
        return Ok(json!({
            "violation_code": code,
            "occurrences": 0,
            "predecessor_activity_lift": [],
        }));
    }

    let mut predecessor_counts: HashMap<String, i64> = HashMap::new();
    let mut predecessor_total = 0i64;
    let mut precedes_stmt = conn.prepare(
        r#"
        SELECT activity FROM ctox_process_events
        WHERE case_id = ?1 AND observed_at <= ?2
        ORDER BY observed_at DESC, event_seq DESC
        LIMIT ?3
        "#,
    )?;
    for (case_id, detected_at) in &occurrences {
        let preds: Vec<String> = precedes_stmt
            .query_map(params![case_id, detected_at, opts.lookback], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<_>>()?;
        for activity in preds {
            *predecessor_counts.entry(activity).or_insert(0) += 1;
            predecessor_total += 1;
        }
    }

    let mut lifted: Vec<(String, i64, f64, f64, f64)> = predecessor_counts
        .iter()
        .filter(|(_, count)| **count >= opts.min_support)
        .map(|(activity, count)| {
            let p_given = if predecessor_total == 0 {
                0.0
            } else {
                *count as f64 / predecessor_total as f64
            };
            let p_marginal = marginal.get(activity).copied().unwrap_or_else(|| {
                // unseen marginal: use Laplace-ish floor proportional to total events
                if total_events == 0 {
                    0.0
                } else {
                    1.0 / (total_events as f64 + 1.0)
                }
            });
            let lift = if p_marginal > 0.0 {
                p_given / p_marginal
            } else {
                f64::INFINITY
            };
            (activity.clone(), *count, p_given, p_marginal, lift)
        })
        .collect();
    lifted.sort_by(|a, b| {
        b.4.partial_cmp(&a.4)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.cmp(&a.1))
    });
    let top: Vec<Value> = lifted
        .into_iter()
        .take(opts.limit as usize)
        .map(|(activity, count, p_given, p_marginal, lift)| {
            json!({
                "activity": activity,
                "support": count,
                "p_given_violation": round4(p_given),
                "p_marginal": round4(p_marginal),
                "lift": if lift.is_finite() { json!(round4(lift)) } else { json!("inf") },
            })
        })
        .collect();
    Ok(json!({
        "violation_code": code,
        "occurrences": occurrences.len(),
        "predecessor_activity_lift": top,
    }))
}

fn total_event_count(conn: &Connection) -> Result<i64> {
    Ok(
        conn.query_row("SELECT COUNT(*) FROM ctox_process_events", [], |row| {
            row.get(0)
        })?,
    )
}

fn marginal_activity_distribution(conn: &Connection, total: i64) -> Result<HashMap<String, f64>> {
    if total == 0 {
        return Ok(HashMap::new());
    }
    let mut stmt = conn.prepare(
        r#"
        SELECT activity, COUNT(*)
        FROM ctox_process_events
        GROUP BY activity
        "#,
    )?;
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows
        .into_iter()
        .map(|(a, c)| (a, c as f64 / total as f64))
        .collect())
}

fn top_violation_codes(conn: &Connection, limit: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT violation_code, COUNT(*) AS c
        FROM ctox_pm_state_violations
        GROUP BY violation_code
        ORDER BY c DESC
        LIMIT ?1
        "#,
    )?;
    let codes: Vec<String> = stmt
        .query_map(params![limit], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(codes)
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE ctox_process_events (
                event_seq INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL UNIQUE,
                observed_at TEXT NOT NULL,
                case_id TEXT NOT NULL,
                activity TEXT NOT NULL,
                lifecycle_transition TEXT NOT NULL DEFAULT 'complete',
                entity_type TEXT NOT NULL DEFAULT 'x',
                entity_id TEXT NOT NULL DEFAULT 'x',
                table_name TEXT NOT NULL DEFAULT 'x',
                operation TEXT NOT NULL DEFAULT 'INSERT',
                from_state TEXT,
                to_state TEXT,
                primary_key_json TEXT NOT NULL DEFAULT '{}',
                row_before_json TEXT NOT NULL DEFAULT '{}',
                row_after_json TEXT NOT NULL DEFAULT '{}',
                changed_columns_json TEXT NOT NULL DEFAULT '[]',
                turn_id TEXT,
                command_id TEXT,
                actor_key TEXT,
                source TEXT,
                command_name TEXT,
                db_path TEXT NOT NULL DEFAULT '',
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE ctox_pm_state_violations (
                violation_id TEXT PRIMARY KEY,
                event_id TEXT,
                case_id TEXT NOT NULL,
                violation_code TEXT NOT NULL,
                severity TEXT NOT NULL DEFAULT 'critical',
                message TEXT NOT NULL DEFAULT '',
                detected_at TEXT NOT NULL,
                evidence_json TEXT NOT NULL DEFAULT '{}'
            );
            "#,
        )
        .unwrap();
        conn
    }

    fn ev(conn: &Connection, eid: &str, case: &str, act: &str, ts: &str) {
        conn.execute(
            "INSERT INTO ctox_process_events (event_id, observed_at, case_id, activity) VALUES (?1, ?2, ?3, ?4)",
            params![eid, ts, case, act],
        )
        .unwrap();
    }

    fn viol(conn: &Connection, vid: &str, case: &str, code: &str, ts: &str) {
        conn.execute(
            "INSERT INTO ctox_pm_state_violations (violation_id, case_id, violation_code, detected_at) VALUES (?1, ?2, ?3, ?4)",
            params![vid, case, code, ts],
        )
        .unwrap();
    }

    #[test]
    fn causal_mining_finds_high_lift_predecessor() {
        let conn = setup_conn();
        // 100 generic events
        for i in 0..100 {
            ev(
                &conn,
                &format!("e{i}"),
                "happy",
                "RoutineWrite",
                &format!("2026-04-26T01:{:02}:00Z", i / 60),
            );
        }
        // bad cases: each has Recompose right before the violation
        for i in 0..5 {
            ev(
                &conn,
                &format!("bx{i}"),
                &format!("bad{i}"),
                "RoutineWrite",
                "2026-04-26T02:00:00Z",
            );
            ev(
                &conn,
                &format!("by{i}"),
                &format!("bad{i}"),
                "Recompose",
                "2026-04-26T02:00:01Z",
            );
            viol(
                &conn,
                &format!("v{i}"),
                &format!("bad{i}"),
                "founder_send_body_hash_mismatch",
                "2026-04-26T02:00:02Z",
            );
        }
        let opts = Options {
            violation_code: Some("founder_send_body_hash_mismatch".to_string()),
            lookback: 2,
            limit: 10,
            min_support: 3,
        };
        let report = analyze(&conn, &opts).unwrap();
        let by_code = report["by_violation_code"].as_array().unwrap();
        assert_eq!(by_code.len(), 1);
        let lift_rows = by_code[0]["predecessor_activity_lift"].as_array().unwrap();
        // Recompose should be the highest-lift activity — it appears in 5/5 violations
        // but is rare in the marginal distribution.
        let top = &lift_rows[0];
        assert_eq!(top["activity"], "Recompose");
        let lift = top["lift"].as_f64().unwrap_or(0.0);
        assert!(lift > 5.0, "expected high lift, got {lift}");
    }
}
