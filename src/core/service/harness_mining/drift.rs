// Origin: CTOX
// License: Apache-2.0
//
// Tier 2.3 — Concept Drift Detection.
//
// Two complementary tests:
//   * Page-Hinkley on the streaming preventive-fitness signal (accepted/total
//     per proof bucket). Detects sustained downshift of the acceptance ratio.
//   * Chi-squared on activity-frequency between two adjacent windows. Detects
//     a regime change in WHAT the harness is doing — even if the spec is
//     still respected.
//
// Both are bounded, online-friendly, and need no external dependency.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Options {
    pub window: i64,
    pub threshold: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            window: 1000,
            threshold: 5.0,
        }
    }
}

impl Options {
    pub fn from_args(args: &[String]) -> Self {
        let d = Self::default();
        Self {
            window: super::parse_i64_flag(args, "--window", d.window).clamp(20, 1_000_000),
            threshold: super::parse_f64_flag(args, "--threshold", d.threshold).max(0.1),
        }
    }
}

pub fn detect(conn: &Connection, opts: &Options) -> Result<Value> {
    let ph = page_hinkley_on_proof_acceptance(conn, opts)?;
    let chi = chi_squared_on_activity_distribution(conn, opts)?;
    let drift_detected = ph["drift_detected"].as_bool().unwrap_or(false)
        || chi["drift_detected"].as_bool().unwrap_or(false);
    Ok(json!({
        "ok": true,
        "tier": "2.3",
        "algorithm": "concept-drift-detection",
        "options": {
            "window": opts.window,
            "threshold": opts.threshold,
        },
        "drift_detected": drift_detected,
        "page_hinkley_acceptance": ph,
        "chi_squared_activity": chi,
    }))
}

fn page_hinkley_on_proof_acceptance(conn: &Connection, opts: &Options) -> Result<Value> {
    let mut stmt = conn.prepare(
        r#"
        SELECT accepted, updated_at
        FROM ctox_core_transition_proofs
        ORDER BY updated_at ASC
        "#,
    )?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if rows.len() < 20 {
        return Ok(json!({
            "drift_detected": false,
            "reason": "insufficient samples",
            "sample_count": rows.len(),
        }));
    }
    // running mean of acceptance, then Page-Hinkley statistic on (mean_so_far - x_t).
    let mut running_sum = 0.0f64;
    let mut count = 0.0f64;
    let mut m_t = 0.0f64;
    let mut min_m = 0.0f64;
    let mut alarm: Option<(usize, String, f64)> = None;
    let alpha = 0.005f64; // small forgetting factor / tolerance
    for (idx, (accepted, ts)) in rows.iter().enumerate() {
        let x = if *accepted == 1 { 1.0 } else { 0.0 };
        count += 1.0;
        running_sum += x;
        let mean = running_sum / count;
        // PH statistic for *decrease* in acceptance:
        m_t += mean - x - alpha;
        if m_t < min_m {
            min_m = m_t;
        }
        let ph_value = m_t - min_m;
        if alarm.is_none() && ph_value > opts.threshold && idx > 30 {
            alarm = Some((idx, ts.clone(), ph_value));
        }
    }
    let result = if let Some((idx, ts, value)) = alarm {
        json!({
            "drift_detected": true,
            "first_alarm_index": idx,
            "first_alarm_at": ts,
            "ph_value": round4(value),
            "sample_count": rows.len(),
        })
    } else {
        json!({
            "drift_detected": false,
            "sample_count": rows.len(),
        })
    };
    Ok(result)
}

fn chi_squared_on_activity_distribution(conn: &Connection, opts: &Options) -> Result<Value> {
    // Pull the most recent 2*window events, split into newer + older halves,
    // and compare activity frequency vectors via chi-squared.
    let mut stmt = conn.prepare(
        r#"
        SELECT activity, observed_at FROM ctox_process_events
        ORDER BY observed_at DESC, event_seq DESC
        LIMIT ?1
        "#,
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map(params![opts.window * 2], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let n = rows.len();
    if n < 40 {
        return Ok(json!({
            "drift_detected": false,
            "reason": "insufficient samples",
            "sample_count": n,
        }));
    }
    let mid = n / 2;
    let (newer, older) = rows.split_at(mid);
    let mut counts_a: HashMap<String, i64> = HashMap::new();
    let mut counts_b: HashMap<String, i64> = HashMap::new();
    for (a, _) in newer {
        *counts_a.entry(a.clone()).or_insert(0) += 1;
    }
    for (a, _) in older {
        *counts_b.entry(a.clone()).or_insert(0) += 1;
    }
    let total_a = newer.len() as f64;
    let total_b = older.len() as f64;
    let mut chi2 = 0.0f64;
    let mut df = 0i64;
    let mut all_keys: Vec<&String> = counts_a.keys().chain(counts_b.keys()).collect();
    all_keys.sort();
    all_keys.dedup();
    let mut top_drivers: Vec<(String, f64)> = Vec::new();
    for key in all_keys {
        let a_obs = *counts_a.get(key).unwrap_or(&0) as f64;
        let b_obs = *counts_b.get(key).unwrap_or(&0) as f64;
        let row_total = a_obs + b_obs;
        if row_total < 5.0 {
            continue; // skip low-support cells
        }
        let a_exp = row_total * total_a / (total_a + total_b);
        let b_exp = row_total * total_b / (total_a + total_b);
        let mut cell_chi = 0.0;
        if a_exp > 0.0 {
            cell_chi += (a_obs - a_exp).powi(2) / a_exp;
        }
        if b_exp > 0.0 {
            cell_chi += (b_obs - b_exp).powi(2) / b_exp;
        }
        chi2 += cell_chi;
        df += 1;
        top_drivers.push((key.clone(), cell_chi));
    }
    df = (df - 1).max(0);
    top_drivers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let critical_value = chi_squared_critical_p001(df);
    let drift = chi2 > critical_value;
    Ok(json!({
        "drift_detected": drift,
        "sample_count_newer": newer.len(),
        "sample_count_older": older.len(),
        "chi_squared": round4(chi2),
        "degrees_of_freedom": df,
        "critical_value_p_lt_0_001": round4(critical_value),
        "top_drift_activities": top_drivers
            .into_iter()
            .take(10)
            .map(|(k, v)| json!({ "activity": k, "chi_squared_contribution": round4(v) }))
            .collect::<Vec<_>>(),
    }))
}

/// Approximate chi-squared critical values at α = 0.001 (very strict, to keep
/// false-alarm rate low). Looked-up table for df 1..30, then a Wilson-Hilferty
/// approximation beyond that. We keep this lookup-only to avoid a stats dep.
fn chi_squared_critical_p001(df: i64) -> f64 {
    const TABLE: [(i64, f64); 30] = [
        (1, 10.83),
        (2, 13.82),
        (3, 16.27),
        (4, 18.47),
        (5, 20.52),
        (6, 22.46),
        (7, 24.32),
        (8, 26.12),
        (9, 27.88),
        (10, 29.59),
        (11, 31.26),
        (12, 32.91),
        (13, 34.53),
        (14, 36.12),
        (15, 37.70),
        (16, 39.25),
        (17, 40.79),
        (18, 42.31),
        (19, 43.82),
        (20, 45.31),
        (21, 46.80),
        (22, 48.27),
        (23, 49.73),
        (24, 51.18),
        (25, 52.62),
        (26, 54.05),
        (27, 55.48),
        (28, 56.89),
        (29, 58.30),
        (30, 59.70),
    ];
    if df <= 0 {
        return 0.0;
    }
    if df <= 30 {
        for (k, v) in &TABLE {
            if *k == df {
                return *v;
            }
        }
    }
    // Wilson-Hilferty: critical_χ²(df, α) ≈ df * (1 - 2/(9df) + z*sqrt(2/(9df)))^3
    // z(α=0.001) one-sided ≈ 3.0902
    let z = 3.0902f64;
    let dff = df as f64;
    let term = 1.0 - 2.0 / (9.0 * dff) + z * (2.0 / (9.0 * dff)).sqrt();
    dff * term.powi(3)
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
            "#,
        )
        .unwrap();
        conn
    }

    #[test]
    fn page_hinkley_alarms_when_acceptance_collapses() {
        let conn = setup_conn();
        // First 60 proofs all accepted, then 60 mostly rejected.
        for i in 0..60 {
            conn.execute(
                "INSERT INTO ctox_core_transition_proofs (proof_id, entity_type, entity_id, lane, from_state, to_state, core_event, actor, accepted, created_at, updated_at) VALUES (?1, 'X', 'e', 'L', 'A', 'B', 'E', 'u', 1, ?2, ?2)",
                params![format!("a{i}"), format!("2026-04-26T01:00:{:02}Z", i)],
            ).unwrap();
        }
        for i in 0..60 {
            conn.execute(
                "INSERT INTO ctox_core_transition_proofs (proof_id, entity_type, entity_id, lane, from_state, to_state, core_event, actor, accepted, created_at, updated_at) VALUES (?1, 'X', 'e', 'L', 'A', 'B', 'E', 'u', 0, ?2, ?2)",
                params![format!("b{i}"), format!("2026-04-26T02:00:{:02}Z", i)],
            ).unwrap();
        }
        let opts = Options {
            window: 100,
            threshold: 1.0,
        };
        let report = page_hinkley_on_proof_acceptance(&conn, &opts).unwrap();
        assert_eq!(report["drift_detected"], true);
    }

    #[test]
    fn chi_squared_detects_distribution_shift() {
        let conn = setup_conn();
        // older half — only 'A' activity, lots of it
        for i in 0..120 {
            conn.execute(
                "INSERT INTO ctox_process_events (event_id, observed_at, case_id, activity) VALUES (?1, ?2, 'c', 'A')",
                params![format!("a{i}"), format!("2026-04-26T01:00:{:02}Z", i % 60)],
            ).unwrap();
        }
        // newer half — mix of A and B
        for i in 0..60 {
            conn.execute(
                "INSERT INTO ctox_process_events (event_id, observed_at, case_id, activity) VALUES (?1, ?2, 'c', 'A')",
                params![format!("an{i}"), format!("2026-04-26T03:00:{:02}Z", i)],
            ).unwrap();
        }
        for i in 0..60 {
            conn.execute(
                "INSERT INTO ctox_process_events (event_id, observed_at, case_id, activity) VALUES (?1, ?2, 'c', 'B')",
                params![format!("bn{i}"), format!("2026-04-26T03:00:{:02}Z", i)],
            ).unwrap();
        }
        let opts = Options {
            window: 200,
            threshold: 5.0,
        };
        let report = chi_squared_on_activity_distribution(&conn, &opts).unwrap();
        assert_eq!(report["drift_detected"], true);
    }

    #[test]
    fn chi_squared_table_lookup_is_correct() {
        assert!((chi_squared_critical_p001(1) - 10.83).abs() < 0.01);
        assert!((chi_squared_critical_p001(10) - 29.59).abs() < 0.01);
        // beyond table — Wilson-Hilferty approx; df=50 critical ≈ 86.66
        let approx = chi_squared_critical_p001(50);
        assert!(approx > 70.0 && approx < 100.0);
    }
}
