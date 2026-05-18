// Origin: CTOX
// License: Apache-2.0
//
// Tier 1.2 — Variant Analysis.
//
// Builds per-case activity sequences ("traces"), groups them by sequence
// hash to count variant frequencies, and optionally clusters near-variants
// by Levenshtein edit distance on the activity-token list. The dominant
// variants and their relative frequency surface anti-patterns: e.g. one
// variant accounting for 87% of rejected founder sends is a load-bearing
// bug, not noise.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Options {
    pub entity_type: Option<String>,
    pub limit: i64,
    pub cluster: bool,
    /// max edit distance for fuzzy clustering (only used when cluster=true)
    pub cluster_distance: usize,
    /// max activities per trace (caps memory on pathological cases)
    pub max_trace_len: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            entity_type: None,
            limit: 50,
            cluster: false,
            cluster_distance: 1,
            max_trace_len: 200,
        }
    }
}

impl Options {
    pub fn from_args(args: &[String]) -> Self {
        let d = Self::default();
        Self {
            entity_type: super::parse_string_flag(args, "--entity-type").map(str::to_string),
            limit: super::parse_i64_flag(args, "--limit", d.limit).clamp(1, 1000),
            cluster: super::flag_present(args, "--cluster"),
            cluster_distance: super::parse_i64_flag(args, "--cluster-distance", 1).max(0) as usize,
            max_trace_len: super::parse_i64_flag(args, "--max-trace-len", 200).clamp(10, 10_000)
                as usize,
        }
    }
}

pub fn analyze(conn: &Connection, opts: &Options) -> Result<Value> {
    let traces = load_traces(conn, opts)?;
    let total_cases = traces.len();
    let mut variants: HashMap<String, VariantBuilder> = HashMap::new();
    for trace in &traces {
        let key = sequence_hash(&trace.activities);
        variants
            .entry(key)
            .or_insert_with(|| VariantBuilder::new(trace.activities.clone()))
            .observe(&trace.case_id);
    }
    let mut variants_vec: Vec<VariantBuilder> = variants.into_values().collect();
    variants_vec.sort_by(|a, b| b.case_count.cmp(&a.case_count));

    let clustered = if opts.cluster {
        cluster_variants(&variants_vec, opts.cluster_distance)
    } else {
        Vec::new()
    };

    let pareto = pareto_summary(&variants_vec, total_cases);
    let top_variants: Vec<Value> = variants_vec
        .iter()
        .take(opts.limit as usize)
        .map(|v| v.to_json(total_cases))
        .collect();

    Ok(json!({
        "ok": true,
        "tier": "1.2",
        "algorithm": "variant-analysis",
        "options": {
            "entity_type": opts.entity_type,
            "limit": opts.limit,
            "cluster": opts.cluster,
            "cluster_distance": opts.cluster_distance,
        },
        "total_cases": total_cases,
        "distinct_variants": variants_vec.len(),
        "pareto": pareto,
        "variants": top_variants,
        "clusters": clustered,
    }))
}

struct Trace {
    case_id: String,
    activities: Vec<String>,
}

fn load_traces(conn: &Connection, opts: &Options) -> Result<Vec<Trace>> {
    // Pull events in case-order so we can partition them into traces with a
    // single linear scan. We use ctox_process_events directly because it is
    // the agent-unbypassable trigger ledger.
    let (sql, has_filter) = if opts.entity_type.is_some() {
        (
            r#"
            SELECT case_id, activity, observed_at, event_seq, entity_type
            FROM ctox_process_events
            WHERE entity_type = ?1
            ORDER BY case_id, observed_at, event_seq
            "#,
            true,
        )
    } else {
        (
            r#"
            SELECT case_id, activity, observed_at, event_seq, entity_type
            FROM ctox_process_events
            ORDER BY case_id, observed_at, event_seq
            "#,
            false,
        )
    };
    let mut stmt = conn.prepare(sql)?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(String, String)> {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    };
    let rows: Vec<(String, String)> = if has_filter {
        let entity = opts.entity_type.as_deref().unwrap();
        stmt.query_map(params![entity], map_row)?
            .collect::<rusqlite::Result<_>>()?
    } else {
        stmt.query_map([], map_row)?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut traces: Vec<Trace> = Vec::new();
    let mut current: Option<Trace> = None;
    for (case_id, activity) in rows {
        match &mut current {
            Some(t) if t.case_id == case_id => {
                if t.activities.len() < opts.max_trace_len {
                    t.activities.push(activity);
                }
            }
            _ => {
                if let Some(prev) = current.take() {
                    traces.push(prev);
                }
                current = Some(Trace {
                    case_id,
                    activities: vec![activity],
                });
            }
        }
    }
    if let Some(prev) = current.take() {
        traces.push(prev);
    }
    Ok(traces)
}

fn sequence_hash(activities: &[String]) -> String {
    let mut hasher = Sha256::new();
    for a in activities {
        hasher.update(a.as_bytes());
        hasher.update(b"\x1f"); // unit separator
    }
    let digest = hasher.finalize();
    hex_short(&digest[..8])
}

fn hex_short(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[derive(Debug)]
struct VariantBuilder {
    activities: Vec<String>,
    case_count: i64,
    sample_case_ids: Vec<String>,
}

impl VariantBuilder {
    fn new(activities: Vec<String>) -> Self {
        Self {
            activities,
            case_count: 0,
            sample_case_ids: Vec::new(),
        }
    }
    fn observe(&mut self, case_id: &str) {
        self.case_count += 1;
        if self.sample_case_ids.len() < 3 {
            self.sample_case_ids.push(case_id.to_string());
        }
    }
    fn key(&self) -> String {
        sequence_hash(&self.activities)
    }
    fn to_json(&self, total_cases: usize) -> Value {
        let share = if total_cases == 0 {
            0.0
        } else {
            self.case_count as f64 / total_cases as f64
        };
        json!({
            "variant_id": self.key(),
            "length": self.activities.len(),
            "case_count": self.case_count,
            "share": share,
            "activities": self.activities,
            "sample_case_ids": self.sample_case_ids,
        })
    }
}

fn pareto_summary(variants: &[VariantBuilder], total_cases: usize) -> Value {
    if total_cases == 0 {
        return json!({ "variants_for_50pct": 0, "variants_for_80pct": 0, "variants_for_95pct": 0 });
    }
    let mut cumulative = 0i64;
    let mut for_50 = 0usize;
    let mut for_80 = 0usize;
    let mut for_95 = 0usize;
    for (idx, v) in variants.iter().enumerate() {
        cumulative += v.case_count;
        let share = cumulative as f64 / total_cases as f64;
        if for_50 == 0 && share >= 0.5 {
            for_50 = idx + 1;
        }
        if for_80 == 0 && share >= 0.8 {
            for_80 = idx + 1;
        }
        if for_95 == 0 && share >= 0.95 {
            for_95 = idx + 1;
            break;
        }
    }
    json!({
        "variants_for_50pct": for_50,
        "variants_for_80pct": for_80,
        "variants_for_95pct": for_95,
    })
}

fn cluster_variants(variants: &[VariantBuilder], max_distance: usize) -> Vec<Value> {
    // Greedy single-link clustering on edit-distance over activity tokens.
    // Variants are pre-sorted by case_count DESC so heaviest variants seed
    // clusters first — what we want for a forensic top-N report.
    let mut assigned: Vec<Option<usize>> = vec![None; variants.len()];
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    for (i, v) in variants.iter().enumerate() {
        if assigned[i].is_some() {
            continue;
        }
        let cluster_id = clusters.len();
        clusters.push(vec![i]);
        assigned[i] = Some(cluster_id);
        for (j, w) in variants.iter().enumerate().skip(i + 1) {
            if assigned[j].is_some() {
                continue;
            }
            if levenshtein(&v.activities, &w.activities) <= max_distance {
                clusters[cluster_id].push(j);
                assigned[j] = Some(cluster_id);
            }
        }
    }
    clusters
        .iter()
        .enumerate()
        .map(|(cid, members)| {
            let total: i64 = members.iter().map(|i| variants[*i].case_count).sum();
            let representative = &variants[members[0]];
            json!({
                "cluster_id": cid,
                "member_variant_count": members.len(),
                "total_case_count": total,
                "representative_variant_id": representative.key(),
                "representative_activities": representative.activities,
            })
        })
        .collect()
}

fn levenshtein<T: Eq>(a: &[T], b: &[T]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for (i, ai) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, bj) in b.iter().enumerate() {
            let cost = if ai == bj { 0 } else { 1 };
            curr[j + 1] = (curr[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
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

    fn ev(conn: &Connection, eid: &str, case: &str, act: &str, ts: &str) {
        conn.execute(
            r#"INSERT INTO ctox_process_events
                (event_id, observed_at, case_id, activity, entity_type, entity_id, table_name, operation)
               VALUES (?1, ?2, ?3, ?4, 'communication', ?3, 't', 'INSERT')"#,
            params![eid, ts, case, act],
        )
        .unwrap();
    }

    #[test]
    fn variants_groups_identical_sequences() {
        let conn = setup_conn();
        ev(&conn, "1", "caseA", "Approve", "2026-04-26T01:00:00Z");
        ev(&conn, "2", "caseA", "Send", "2026-04-26T01:00:01Z");
        ev(&conn, "3", "caseB", "Approve", "2026-04-26T01:01:00Z");
        ev(&conn, "4", "caseB", "Send", "2026-04-26T01:01:01Z");
        ev(&conn, "5", "caseC", "Approve", "2026-04-26T01:02:00Z");
        ev(&conn, "6", "caseC", "Reject", "2026-04-26T01:02:01Z");

        let report = analyze(&conn, &Options::default()).unwrap();
        assert_eq!(report["total_cases"], 3);
        assert_eq!(report["distinct_variants"], 2);
        let variants = report["variants"].as_array().unwrap();
        assert_eq!(variants[0]["case_count"], 2);
    }

    #[test]
    fn pareto_finds_dominant_variant() {
        let conn = setup_conn();
        for i in 0..8 {
            ev(
                &conn,
                &format!("e{i}"),
                &format!("happy{i}"),
                "OK",
                &format!("2026-04-26T02:00:{:02}Z", i),
            );
        }
        for i in 0..2 {
            ev(
                &conn,
                &format!("f{i}"),
                &format!("bad{i}"),
                "FAIL",
                &format!("2026-04-26T03:00:{:02}Z", i),
            );
        }
        let report = analyze(&conn, &Options::default()).unwrap();
        assert_eq!(report["pareto"]["variants_for_80pct"], 1);
    }

    #[test]
    fn levenshtein_is_correct() {
        assert_eq!(levenshtein::<u8>(&[], &[]), 0);
        assert_eq!(levenshtein(b"abc", b"abc"), 0);
        assert_eq!(levenshtein(b"abc", b"abd"), 1);
        assert_eq!(levenshtein(b"abc", b"abcd"), 1);
        assert_eq!(levenshtein(b"abc", b"xyz"), 3);
    }

    #[test]
    fn clustering_merges_near_variants() {
        let conn = setup_conn();
        // happy: A,B,C  (×3)
        for i in 0..3 {
            ev(
                &conn,
                &format!("h{i}1"),
                &format!("h{i}"),
                "A",
                "2026-04-26T01:00:00Z",
            );
            ev(
                &conn,
                &format!("h{i}2"),
                &format!("h{i}"),
                "B",
                "2026-04-26T01:00:01Z",
            );
            ev(
                &conn,
                &format!("h{i}3"),
                &format!("h{i}"),
                "C",
                "2026-04-26T01:00:02Z",
            );
        }
        // near: A,B,C,D  (×1) — distance 1 from happy
        ev(&conn, "n1", "near", "A", "2026-04-26T02:00:00Z");
        ev(&conn, "n2", "near", "B", "2026-04-26T02:00:01Z");
        ev(&conn, "n3", "near", "C", "2026-04-26T02:00:02Z");
        ev(&conn, "n4", "near", "D", "2026-04-26T02:00:03Z");
        // far: X,Y,Z (×1) — distance 3
        ev(&conn, "f1", "far", "X", "2026-04-26T03:00:00Z");
        ev(&conn, "f2", "far", "Y", "2026-04-26T03:00:01Z");
        ev(&conn, "f3", "far", "Z", "2026-04-26T03:00:02Z");

        let opts = Options {
            cluster: true,
            cluster_distance: 1,
            ..Default::default()
        };
        let report = analyze(&conn, &opts).unwrap();
        let clusters = report["clusters"].as_array().unwrap();
        // happy + near merge into one cluster, far is its own cluster
        assert_eq!(clusters.len(), 2);
    }
}
