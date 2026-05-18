// Origin: CTOX
// License: Apache-2.0
//
// Tier 1.4 — Threshold-gated Conformance Replay against the declared spec.
//
// We do NOT perform classical Petri-net token replay. The CTOX harness has a
// fully declarative state-machine spec in `core_state_machine` — replay here
// means: scan observed transitions, compare against `allowed_transition_catalog`,
// compute fitness, and fail loudly if fitness drops below threshold.
//
// Two metrics are computed:
//   * preventive_fitness  = accepted_proofs / (accepted_proofs + rejected_proofs)
//   * trigger_fitness     = in_catalog_transitions / total_observed_transitions
//
// Together they answer: "is the harness behaving inside the spec, both by
// what the preventive layer accepted AND by what the unbypassable trigger
// ledger actually recorded?"

use crate::service::core_state_machine as csm;
use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Options {
    /// Restrict to this lane (canonical csm string, e.g. "P0FounderCommunication").
    pub lane: Option<String>,
    /// Restrict the replay to observations at or after this timestamp.
    ///
    /// This is intentionally explicit. It lets operators prove the currently
    /// deployed harness after a known fix point without deleting historical
    /// violations from the forensic ledger.
    pub since: Option<String>,
    /// Sliding window size; only the last N proofs / transitions per metric.
    pub window: i64,
    /// Failure threshold; below this either fitness is reported as not OK.
    pub fitness_threshold: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            lane: None,
            since: None,
            window: 1000,
            fitness_threshold: 0.95,
        }
    }
}

impl Options {
    pub fn from_args(args: &[String]) -> Self {
        let d = Self::default();
        Self {
            lane: super::parse_string_flag(args, "--lane").map(str::to_string),
            since: super::parse_string_flag(args, "--since").map(str::to_string),
            window: super::parse_i64_flag(args, "--window", d.window).clamp(10, 1_000_000),
            fitness_threshold: super::parse_f64_flag(
                args,
                "--fitness-threshold",
                d.fitness_threshold,
            )
            .clamp(0.0, 1.0),
        }
    }
}

pub fn replay(conn: &Connection, opts: &Options) -> Result<Value> {
    let preventive = preventive_fitness(conn, opts)?;
    let trigger = trigger_fitness(conn, opts)?;

    let prev_ok = preventive.fitness >= opts.fitness_threshold;
    let trig_ok = trigger.fitness >= opts.fitness_threshold;
    let fitness_ok = prev_ok && trig_ok;

    Ok(json!({
        "ok": true,
        "tier": "1.4",
        "algorithm": "threshold-conformance-replay",
        "spec_source": "core_state_machine.rs",
        "options": {
            "lane": opts.lane,
            "since": opts.since,
            "window": opts.window,
            "fitness_threshold": opts.fitness_threshold,
        },
        "preventive": preventive.to_json(opts.fitness_threshold),
        "trigger": trigger.to_json(opts.fitness_threshold),
        "fitness_ok": fitness_ok,
    }))
}

#[derive(Debug)]
struct FitnessReport {
    fitness: f64,
    numerator: i64,
    denominator: i64,
    failing_buckets: Vec<Value>,
}

impl FitnessReport {
    fn empty() -> Self {
        Self {
            fitness: 1.0,
            numerator: 0,
            denominator: 0,
            failing_buckets: Vec::new(),
        }
    }
    fn to_json(&self, threshold: f64) -> Value {
        json!({
            "fitness": round4(self.fitness),
            "numerator": self.numerator,
            "denominator": self.denominator,
            "fitness_ok": self.fitness >= threshold,
            "failing_buckets": self.failing_buckets,
        })
    }
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

fn preventive_fitness(conn: &Connection, opts: &Options) -> Result<FitnessReport> {
    // pull the most recent N proofs (filtered by lane if requested)
    let (sql, has_lane, has_since) = match (&opts.lane, &opts.since) {
        (Some(_), Some(_)) => (
            r#"
            SELECT entity_type, lane, accepted FROM ctox_core_transition_proofs
            WHERE lane = ?1
              AND updated_at >= ?2
            ORDER BY updated_at DESC
            LIMIT ?3
            "#,
            true,
            true,
        ),
        (Some(_), None) => (
            r#"
            SELECT entity_type, lane, accepted FROM ctox_core_transition_proofs
            WHERE lane = ?1
            ORDER BY updated_at DESC
            LIMIT ?2
            "#,
            true,
            false,
        ),
        (None, Some(_)) => (
            r#"
            SELECT entity_type, lane, accepted FROM ctox_core_transition_proofs
            WHERE updated_at >= ?1
            ORDER BY updated_at DESC
            LIMIT ?2
            "#,
            false,
            true,
        ),
        (None, None) => (
            r#"
            SELECT entity_type, lane, accepted FROM ctox_core_transition_proofs
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
            false,
            false,
        ),
    };
    let mut stmt = conn.prepare(sql)?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(String, String, i64)> {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    };
    let rows: Vec<(String, String, i64)> = match (has_lane, has_since) {
        (true, true) => stmt
            .query_map(
                params![
                    opts.lane.as_deref().unwrap(),
                    opts.since.as_deref().unwrap(),
                    opts.window
                ],
                map_row,
            )?
            .collect::<rusqlite::Result<_>>()?,
        (true, false) => stmt
            .query_map(params![opts.lane.as_deref().unwrap(), opts.window], map_row)?
            .collect::<rusqlite::Result<_>>()?,
        (false, true) => stmt
            .query_map(
                params![opts.since.as_deref().unwrap(), opts.window],
                map_row,
            )?
            .collect::<rusqlite::Result<_>>()?,
        (false, false) => stmt
            .query_map(params![opts.window], map_row)?
            .collect::<rusqlite::Result<_>>()?,
    };

    if rows.is_empty() {
        return Ok(FitnessReport::empty());
    }

    let mut buckets: HashMap<(String, String), (i64, i64)> = HashMap::new();
    let mut accepted_total = 0i64;
    for (et, lane, accepted) in &rows {
        let entry = buckets.entry((et.clone(), lane.clone())).or_insert((0, 0));
        if *accepted == 1 {
            entry.0 += 1;
            accepted_total += 1;
        } else {
            entry.1 += 1;
        }
    }
    let denom = rows.len() as i64;
    let fitness = accepted_total as f64 / denom as f64;

    let mut failing: Vec<((String, String), f64, i64, i64)> = buckets
        .into_iter()
        .map(|((et, lane), (acc, rej))| {
            let total = acc + rej;
            let f = if total == 0 {
                1.0
            } else {
                acc as f64 / total as f64
            };
            ((et, lane), f, acc, rej)
        })
        .filter(|(_, f, _, _)| *f < opts.fitness_threshold)
        .collect();
    failing.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let failing_buckets = failing
        .into_iter()
        .take(20)
        .map(|((et, lane), f, acc, rej)| {
            json!({
                "entity_type": et,
                "lane": lane,
                "fitness": round4(f),
                "accepted": acc,
                "rejected": rej,
            })
        })
        .collect();

    Ok(FitnessReport {
        fitness,
        numerator: accepted_total,
        denominator: denom,
        failing_buckets,
    })
}

fn trigger_fitness(conn: &Connection, opts: &Options) -> Result<FitnessReport> {
    let (sql, has_since) = if opts.since.is_some() {
        (
            r#"
        WITH ordered AS (
            SELECT case_id, entity_type,
                   to_state AS to_state,
                   LAG(to_state) OVER (
                       PARTITION BY case_id ORDER BY observed_at, event_seq
                   ) AS prev_to_state,
                   observed_at, event_seq
            FROM ctox_process_events
            WHERE to_state IS NOT NULL
              AND observed_at >= ?1
        )
        SELECT entity_type, prev_to_state, to_state
        FROM ordered
        WHERE prev_to_state IS NOT NULL
          AND prev_to_state != to_state
        ORDER BY observed_at DESC, event_seq DESC
        LIMIT ?2
        "#,
            true,
        )
    } else {
        (
            r#"
        WITH ordered AS (
            SELECT case_id, entity_type,
                   to_state AS to_state,
                   LAG(to_state) OVER (
                       PARTITION BY case_id ORDER BY observed_at, event_seq
                   ) AS prev_to_state,
                   observed_at, event_seq
            FROM ctox_process_events
            WHERE to_state IS NOT NULL
        )
        SELECT entity_type, prev_to_state, to_state
        FROM ordered
        WHERE prev_to_state IS NOT NULL
          AND prev_to_state != to_state
        ORDER BY observed_at DESC, event_seq DESC
        LIMIT ?1
        "#,
            false,
        )
    };
    let mut stmt = conn.prepare(sql)?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(String, String, String)> {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    };
    let rows: Vec<(String, String, String)> = if has_since {
        stmt.query_map(
            params![opts.since.as_deref().unwrap(), opts.window],
            map_row,
        )?
        .collect::<rusqlite::Result<_>>()?
    } else {
        stmt.query_map(params![opts.window], map_row)?
            .collect::<rusqlite::Result<_>>()?
    };

    if rows.is_empty() {
        return Ok(FitnessReport::empty());
    }

    let catalog = build_catalog();
    let mut in_spec = 0i64;
    let mut violations: HashMap<(String, String, String), i64> = HashMap::new();
    for (et, from, to) in &rows {
        if catalog.is_allowed(et, from, to) {
            in_spec += 1;
        } else {
            *violations
                .entry((et.clone(), from.clone(), to.clone()))
                .or_insert(0) += 1;
        }
    }
    let denom = rows.len() as i64;
    let fitness = in_spec as f64 / denom as f64;

    let mut violation_rows: Vec<((String, String, String), i64)> = violations.into_iter().collect();
    violation_rows.sort_by(|a, b| b.1.cmp(&a.1));
    let failing_buckets = violation_rows
        .into_iter()
        .take(20)
        .map(|((et, from, to), count)| {
            json!({
                "entity_type": et,
                "from_state": from,
                "to_state": to,
                "out_of_catalog_transitions": count,
            })
        })
        .collect();

    Ok(FitnessReport {
        fitness,
        numerator: in_spec,
        denominator: denom,
        failing_buckets,
    })
}

/// Lookup: entity_type-string → set of allowed (from,to) state-pairs as strings.
struct AllowedCatalog {
    by_entity: HashMap<String, HashSet<(String, String)>>,
    /// entity_types not present here (e.g. lowercased trigger-level names) get
    /// a permissive pass — we only fail transitions for *known* entity types.
    known_entity_types: HashSet<String>,
}

impl AllowedCatalog {
    fn is_allowed(&self, entity_type: &str, from: &str, to: &str) -> bool {
        let key = canonicalize_entity_type(entity_type);
        if !self.known_entity_types.contains(&key) {
            return true; // unknown entity type — out of scope for this metric
        }
        self.by_entity
            .get(&key)
            .map(|set| set.contains(&(from.to_string(), to.to_string())))
            .unwrap_or(true)
    }
}

fn build_catalog() -> AllowedCatalog {
    let mut by_entity: HashMap<String, HashSet<(String, String)>> = HashMap::new();
    let mut known_entity_types: HashSet<String> = HashSet::new();
    for et in csm::core_entity_types() {
        let et_name = entity_type_to_str(*et);
        known_entity_types.insert(et_name.clone());
        let mut set: HashSet<(String, String)> = HashSet::new();
        for (from, to) in csm::allowed_transition_catalog(*et) {
            set.insert((state_to_str(*from), state_to_str(*to)));
        }
        by_entity.insert(et_name, set);
    }
    AllowedCatalog {
        by_entity,
        known_entity_types,
    }
}

fn entity_type_to_str(et: csm::CoreEntityType) -> String {
    serde_json::to_value(et)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn state_to_str(s: csm::CoreState) -> String {
    serde_json::to_value(s)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Trigger-level entity_type strings sometimes use lowercase/alternative
/// labels. We accept either the canonical csm-name or a whitelist of known
/// trigger labels and map them to the canonical form.
fn canonicalize_entity_type(name: &str) -> String {
    // Try direct match first (csm canonical naming).
    let direct: HashSet<String> = canonical_known_entity_types();
    if direct.contains(name) {
        return name.to_string();
    }
    // Best-effort PascalCase from snake_case.
    let pascal: String = name
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    if direct.contains(&pascal) {
        return pascal;
    }
    // Common aliases used at trigger / inference layer.
    match name {
        "communication" | "founder_communication" => "FounderCommunication".to_string(),
        "queue" => "QueueItem".to_string(),
        "ticket" => "Ticket".to_string(),
        "work_item" => "WorkItem".to_string(),
        "commitment" => "Commitment".to_string(),
        "schedule" => "Schedule".to_string(),
        "knowledge" => "Knowledge".to_string(),
        "repair" => "Repair".to_string(),
        _ => name.to_string(),
    }
}

fn canonical_known_entity_types() -> HashSet<String> {
    csm::core_entity_types()
        .iter()
        .map(|et| entity_type_to_str(*et))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_proofs() -> Connection {
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

    fn proof(conn: &Connection, id: &str, entity_type: &str, lane: &str, accepted: i64, ts: &str) {
        conn.execute(
            r#"INSERT INTO ctox_core_transition_proofs
                (proof_id, entity_type, entity_id, lane, from_state, to_state,
                 core_event, actor, accepted, created_at, updated_at)
               VALUES (?1, ?2, 'e', ?3, 'A', 'B', 'X', 'upgrade', ?4, ?5, ?5)"#,
            params![id, entity_type, lane, accepted, ts],
        )
        .unwrap();
    }

    #[test]
    fn preventive_fitness_handles_empty() {
        let conn = setup_proofs();
        let report = preventive_fitness(&conn, &Options::default()).unwrap();
        assert_eq!(report.denominator, 0);
        assert!((report.fitness - 1.0).abs() < 1e-9);
    }

    #[test]
    fn preventive_fitness_below_threshold_is_caught() {
        let conn = setup_proofs();
        for i in 0..6 {
            proof(
                &conn,
                &format!("a{i}"),
                "FounderCommunication",
                "P0FounderCommunication",
                0,
                &format!("2026-04-26T10:00:0{i}Z"),
            );
        }
        for i in 0..4 {
            proof(
                &conn,
                &format!("b{i}"),
                "FounderCommunication",
                "P0FounderCommunication",
                1,
                &format!("2026-04-26T10:01:0{i}Z"),
            );
        }
        let opts = Options {
            window: 100,
            fitness_threshold: 0.9,
            ..Default::default()
        };
        let report = preventive_fitness(&conn, &opts).unwrap();
        assert!((report.fitness - 0.4).abs() < 1e-9);
        assert!(!report.failing_buckets.is_empty());
    }

    #[test]
    fn preventive_fitness_since_filters_legacy_rejections() {
        let conn = setup_proofs();
        proof(
            &conn,
            "legacy-reject",
            "FounderCommunication",
            "P0FounderCommunication",
            0,
            "2026-04-26T10:00:00Z",
        );
        proof(
            &conn,
            "current-pass",
            "FounderCommunication",
            "P0FounderCommunication",
            1,
            "2026-04-26T11:00:00Z",
        );

        let report = preventive_fitness(
            &conn,
            &Options {
                since: Some("2026-04-26T10:30:00Z".to_string()),
                window: 100,
                fitness_threshold: 1.0,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(report.denominator, 1);
        assert_eq!(report.numerator, 1);
        assert!((report.fitness - 1.0).abs() < 1e-9);
    }

    #[test]
    fn replay_top_level_combines_metrics() {
        let conn = setup_proofs();
        proof(
            &conn,
            "p1",
            "FounderCommunication",
            "P0FounderCommunication",
            1,
            "2026-04-26T10:00:00Z",
        );
        let report = replay(
            &conn,
            &Options {
                fitness_threshold: 0.5,
                window: 100,
                lane: None,
                since: None,
            },
        )
        .unwrap();
        assert_eq!(report["fitness_ok"], true);
        assert_eq!(report["preventive"]["fitness_ok"], true);
    }
}
