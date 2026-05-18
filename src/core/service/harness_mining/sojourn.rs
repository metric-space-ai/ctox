// Origin: CTOX
// License: Apache-2.0
//
// Tier 1.3 — Sojourn / State-Holding-Time-Distribution.
//
// Per (entity_type, state) we compute the distribution of how long a case
// resides in that state before transitioning. Output: median, p95, p99,
// max, plus the per-state and per-entity_type aggregates. This surfaces
// bottlenecks ("Communication cases sit in Approved for p95=2 days") that
// are invisible in pure violation counts.

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
            limit: 200,
        }
    }
}

impl Options {
    pub fn from_args(args: &[String]) -> Self {
        Self {
            entity_type: super::parse_string_flag(args, "--entity-type").map(str::to_string),
            limit: super::parse_i64_flag(args, "--limit", 200).clamp(1, 5000),
        }
    }
}

pub fn analyze(conn: &Connection, opts: &Options) -> Result<Value> {
    // We compute Δt between adjacent events of the same case. This is
    // dwell-in-prior-state, which is the canonical PM definition. We
    // aggregate by (entity_type, prior_to_state) — i.e. the state the case
    // was sitting in until the next event arrived.
    let (sql, has_filter) = if opts.entity_type.is_some() {
        (
            r#"
            WITH ordered AS (
                SELECT case_id, entity_type, to_state AS prior_state,
                       observed_at,
                       LEAD(observed_at) OVER (
                           PARTITION BY case_id ORDER BY observed_at, event_seq
                       ) AS next_at
                FROM ctox_process_events
                WHERE entity_type = ?1
            )
            SELECT entity_type, prior_state,
                   (julianday(next_at) - julianday(observed_at)) * 86400.0 AS dwell_seconds
            FROM ordered
            WHERE next_at IS NOT NULL
              AND prior_state IS NOT NULL
            "#,
            true,
        )
    } else {
        (
            r#"
            WITH ordered AS (
                SELECT case_id, entity_type, to_state AS prior_state,
                       observed_at,
                       LEAD(observed_at) OVER (
                           PARTITION BY case_id ORDER BY observed_at, event_seq
                       ) AS next_at
                FROM ctox_process_events
            )
            SELECT entity_type, prior_state,
                   (julianday(next_at) - julianday(observed_at)) * 86400.0 AS dwell_seconds
            FROM ordered
            WHERE next_at IS NOT NULL
              AND prior_state IS NOT NULL
            "#,
            false,
        )
    };
    let mut stmt = conn.prepare(sql)?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(String, String, f64)> {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
        ))
    };
    let rows: Vec<(String, String, f64)> = if has_filter {
        let entity = opts.entity_type.as_deref().unwrap();
        stmt.query_map(params![entity], map_row)?
            .collect::<rusqlite::Result<_>>()?
    } else {
        stmt.query_map([], map_row)?
            .collect::<rusqlite::Result<_>>()?
    };

    let mut buckets: HashMap<(String, String), Vec<f64>> = HashMap::new();
    for (et, st, dwell) in rows {
        if dwell < 0.0 || !dwell.is_finite() {
            continue;
        }
        buckets.entry((et, st)).or_default().push(dwell);
    }

    let mut summaries: Vec<StateSummary> = buckets
        .into_iter()
        .map(|((et, st), mut samples)| {
            samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            StateSummary::from_sorted(&et, &st, &samples)
        })
        .collect();
    summaries.sort_by(|a, b| {
        b.p95_seconds
            .partial_cmp(&a.p95_seconds)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let by_entity = aggregate_by_entity(&summaries);
    let total_observations: i64 = summaries.iter().map(|s| s.observations).sum();
    let states: Vec<Value> = summaries
        .iter()
        .take(opts.limit as usize)
        .map(StateSummary::to_json)
        .collect();

    Ok(json!({
        "ok": true,
        "tier": "1.3",
        "algorithm": "sojourn-time-distribution",
        "options": {
            "entity_type": opts.entity_type,
            "limit": opts.limit,
        },
        "total_observations": total_observations,
        "distinct_states": summaries.len(),
        "states": states,
        "by_entity_type": by_entity,
    }))
}

#[derive(Debug)]
struct StateSummary {
    entity_type: String,
    state: String,
    observations: i64,
    median_seconds: f64,
    p95_seconds: f64,
    p99_seconds: f64,
    max_seconds: f64,
    mean_seconds: f64,
}

impl StateSummary {
    fn from_sorted(entity_type: &str, state: &str, sorted: &[f64]) -> Self {
        let n = sorted.len();
        let mean = if n == 0 {
            0.0
        } else {
            sorted.iter().sum::<f64>() / n as f64
        };
        Self {
            entity_type: entity_type.to_string(),
            state: state.to_string(),
            observations: n as i64,
            median_seconds: percentile(sorted, 0.50),
            p95_seconds: percentile(sorted, 0.95),
            p99_seconds: percentile(sorted, 0.99),
            max_seconds: sorted.last().copied().unwrap_or(0.0),
            mean_seconds: mean,
        }
    }
    fn to_json(&self) -> Value {
        json!({
            "entity_type": self.entity_type,
            "state": self.state,
            "observations": self.observations,
            "median_seconds": round2(self.median_seconds),
            "mean_seconds": round2(self.mean_seconds),
            "p95_seconds": round2(self.p95_seconds),
            "p99_seconds": round2(self.p99_seconds),
            "max_seconds": round2(self.max_seconds),
        })
    }
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = q * (sorted.len() as f64 - 1.0);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = pos - lo as f64;
        sorted[lo] + (sorted[hi] - sorted[lo]) * frac
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn aggregate_by_entity(summaries: &[StateSummary]) -> Vec<Value> {
    let mut grouped: HashMap<String, (i64, f64, f64)> = HashMap::new();
    // (observations, max p95 across states, total mean*observations for weighted)
    for s in summaries {
        let entry = grouped
            .entry(s.entity_type.clone())
            .or_insert((0, 0.0, 0.0));
        entry.0 += s.observations;
        if s.p95_seconds > entry.1 {
            entry.1 = s.p95_seconds;
        }
        entry.2 += s.mean_seconds * s.observations as f64;
    }
    let mut rows: Vec<Value> = grouped
        .into_iter()
        .map(|(et, (obs, worst_p95, weighted_sum))| {
            let weighted_mean = if obs == 0 {
                0.0
            } else {
                weighted_sum / obs as f64
            };
            json!({
                "entity_type": et,
                "observations": obs,
                "weighted_mean_dwell_seconds": round2(weighted_mean),
                "worst_state_p95_seconds": round2(worst_p95),
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        b["worst_state_p95_seconds"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&a["worst_state_p95_seconds"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows
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
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                table_name TEXT NOT NULL,
                operation TEXT NOT NULL,
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
            "#,
        )
        .unwrap();
        conn
    }

    fn ev(conn: &Connection, eid: &str, case: &str, et: &str, state: &str, ts: &str) {
        conn.execute(
            r#"INSERT INTO ctox_process_events
                (event_id, observed_at, case_id, activity, entity_type, entity_id,
                 table_name, operation, from_state, to_state)
               VALUES (?1, ?2, ?3, 't', ?4, ?3, 'tab', 'UPDATE', NULL, ?5)"#,
            params![eid, ts, case, et, state],
        )
        .unwrap();
    }

    #[test]
    fn percentile_works() {
        let v = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile(&v, 0.50), 3.0);
        assert!((percentile(&v, 0.95) - 4.8).abs() < 1e-9);
    }

    #[test]
    fn sojourn_aggregates_state_dwell_time() {
        let conn = setup_conn();
        // case A sits in 'Approved' for 60s, then transitions to 'Sending'
        ev(
            &conn,
            "1",
            "A",
            "communication",
            "Approved",
            "2026-04-26T01:00:00Z",
        );
        ev(
            &conn,
            "2",
            "A",
            "communication",
            "Sending",
            "2026-04-26T01:01:00Z",
        );
        // case B sits in 'Approved' for 120s, then 'Sending'
        ev(
            &conn,
            "3",
            "B",
            "communication",
            "Approved",
            "2026-04-26T02:00:00Z",
        );
        ev(
            &conn,
            "4",
            "B",
            "communication",
            "Sending",
            "2026-04-26T02:02:00Z",
        );

        let report = analyze(&conn, &Options::default()).unwrap();
        let states = report["states"].as_array().unwrap();
        let approved = states
            .iter()
            .find(|s| s["state"] == "Approved")
            .expect("Approved state present");
        assert_eq!(approved["observations"], 2);
        assert!((approved["median_seconds"].as_f64().unwrap() - 90.0).abs() < 0.01);
        assert!((approved["max_seconds"].as_f64().unwrap() - 120.0).abs() < 0.01);
    }

    #[test]
    fn sojourn_filters_by_entity_type() {
        let conn = setup_conn();
        ev(
            &conn,
            "1",
            "A",
            "communication",
            "Approved",
            "2026-04-26T01:00:00Z",
        );
        ev(
            &conn,
            "2",
            "A",
            "communication",
            "Sending",
            "2026-04-26T01:01:00Z",
        );
        ev(&conn, "3", "X", "queue", "Queued", "2026-04-26T01:00:00Z");
        ev(&conn, "4", "X", "queue", "Running", "2026-04-26T01:00:30Z");

        let report = analyze(
            &conn,
            &Options {
                entity_type: Some("queue".to_string()),
                limit: 200,
            },
        )
        .unwrap();
        let states = report["states"].as_array().unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0]["entity_type"], "queue");
    }
}
