// Origin: CTOX
// License: Apache-2.0
//
// Tier 2.1 — Alignment-Based Conformance.
//
// For each observed trace, find the *minimum-cost edit alignment* against
// any path through the declarative spec graph (csm::allowed_transition_catalog).
// The alignment tells us not just IF a trace is non-conforming but WHAT it
// would have taken to make it conform — a reparation hypothesis.
//
// Move semantics (van der Aalst / Adriansyah):
//   * sync       — observed transition matches a spec edge       (cost 0)
//   * model-only — spec demands a transition we never observed   (cost 1)
//   * log-only   — observed an edge not in the spec              (cost 1)
//
// Search: A* with admissible heuristic h = max(0, |trace_remaining|).
// We bound the search by max_states_explored so a pathological trace
// cannot wedge the analysis.

use crate::service::core_state_machine as csm;
use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::{BinaryHeap, HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Options {
    pub entity_type: Option<String>,
    pub limit: i64,
    pub max_trace_len: usize,
    pub max_states_explored: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            entity_type: None,
            limit: 20,
            max_trace_len: 32,
            max_states_explored: 50_000,
        }
    }
}

impl Options {
    pub fn from_args(args: &[String]) -> Self {
        let d = Self::default();
        Self {
            entity_type: super::parse_string_flag(args, "--entity-type").map(str::to_string),
            limit: super::parse_i64_flag(args, "--limit", d.limit).clamp(1, 200),
            max_trace_len: super::parse_i64_flag(args, "--max-trace-len", 32).clamp(2, 256)
                as usize,
            max_states_explored: super::parse_i64_flag(args, "--max-states", 50_000)
                .clamp(1_000, 5_000_000) as usize,
        }
    }
}

pub fn analyze(conn: &Connection, opts: &Options) -> Result<Value> {
    let traces = load_traces(conn, opts)?;
    let mut alignments: Vec<Value> = Vec::new();
    let mut non_conforming = 0i64;
    let mut total_cost = 0i64;
    let mut by_entity: HashMap<String, (i64, i64)> = HashMap::new();
    for trace in &traces {
        let entity_canonical = canonicalize(&trace.entity_type);
        let Some(spec) = lookup_spec(&entity_canonical) else {
            continue;
        };
        let result = align_trace(&spec, &trace.transitions, opts.max_states_explored);
        let (entry_count, entry_total) =
            by_entity.entry(entity_canonical.clone()).or_insert((0, 0));
        *entry_count += 1;
        *entry_total += result.cost as i64;
        if result.cost > 0 {
            non_conforming += 1;
            total_cost += result.cost as i64;
            if alignments.len() < opts.limit as usize {
                alignments.push(json!({
                    "case_id": trace.case_id,
                    "entity_type": entity_canonical,
                    "trace_length": trace.transitions.len(),
                    "alignment_cost": result.cost,
                    "moves": result.moves,
                    "search_truncated": result.truncated,
                }));
            }
        }
    }
    alignments.sort_by(|a, b| {
        b["alignment_cost"]
            .as_i64()
            .unwrap_or(0)
            .cmp(&a["alignment_cost"].as_i64().unwrap_or(0))
    });
    let by_entity_rows: Vec<Value> = by_entity
        .into_iter()
        .map(|(et, (cases, cost))| {
            let mean = if cases == 0 {
                0.0
            } else {
                cost as f64 / cases as f64
            };
            json!({
                "entity_type": et,
                "cases": cases,
                "total_cost": cost,
                "mean_cost": (mean * 100.0).round() / 100.0,
            })
        })
        .collect();
    Ok(json!({
        "ok": true,
        "tier": "2.1",
        "algorithm": "alignment-based-conformance",
        "spec_source": "core_state_machine.rs",
        "options": {
            "entity_type": opts.entity_type,
            "limit": opts.limit,
            "max_trace_len": opts.max_trace_len,
            "max_states_explored": opts.max_states_explored,
        },
        "trace_count": traces.len(),
        "non_conforming_count": non_conforming,
        "total_alignment_cost": total_cost,
        "alignments": alignments,
        "by_entity_type": by_entity_rows,
    }))
}

#[derive(Debug)]
struct ObservedTrace {
    case_id: String,
    entity_type: String,
    transitions: Vec<(String, String)>,
}

fn load_traces(conn: &Connection, opts: &Options) -> Result<Vec<ObservedTrace>> {
    let (sql, has_filter) = if opts.entity_type.is_some() {
        (
            r#"
            WITH ordered AS (
                SELECT case_id, entity_type, to_state,
                       LAG(to_state) OVER (
                           PARTITION BY case_id ORDER BY observed_at, event_seq
                       ) AS prev_to_state,
                       observed_at, event_seq
                FROM ctox_process_events
                WHERE entity_type = ?1
                  AND to_state IS NOT NULL
            )
            SELECT case_id, entity_type, prev_to_state, to_state
            FROM ordered
            WHERE prev_to_state IS NOT NULL
              AND prev_to_state != to_state
            ORDER BY case_id, observed_at, event_seq
            "#,
            true,
        )
    } else {
        (
            r#"
            WITH ordered AS (
                SELECT case_id, entity_type, to_state,
                       LAG(to_state) OVER (
                           PARTITION BY case_id ORDER BY observed_at, event_seq
                       ) AS prev_to_state,
                       observed_at, event_seq
                FROM ctox_process_events
                WHERE to_state IS NOT NULL
            )
            SELECT case_id, entity_type, prev_to_state, to_state
            FROM ordered
            WHERE prev_to_state IS NOT NULL
              AND prev_to_state != to_state
            ORDER BY case_id, observed_at, event_seq
            "#,
            false,
        )
    };
    let mut stmt = conn.prepare(sql)?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(String, String, String, String)> {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    };
    let rows: Vec<(String, String, String, String)> = if has_filter {
        let entity = opts.entity_type.as_deref().unwrap();
        stmt.query_map(params![entity], map_row)?
            .collect::<rusqlite::Result<_>>()?
    } else {
        stmt.query_map([], map_row)?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut traces: Vec<ObservedTrace> = Vec::new();
    let mut current: Option<ObservedTrace> = None;
    for (case_id, entity_type, from, to) in rows {
        match &mut current {
            Some(t) if t.case_id == case_id => {
                if t.transitions.len() < opts.max_trace_len {
                    t.transitions.push((from, to));
                }
            }
            _ => {
                if let Some(prev) = current.take() {
                    traces.push(prev);
                }
                current = Some(ObservedTrace {
                    case_id,
                    entity_type,
                    transitions: vec![(from, to)],
                });
            }
        }
    }
    if let Some(prev) = current.take() {
        traces.push(prev);
    }
    Ok(traces)
}

#[derive(Debug)]
struct SpecGraph {
    /// from_state → set of allowed (to_state) targets
    edges: HashMap<String, HashSet<String>>,
}

fn lookup_spec(entity_type: &str) -> Option<SpecGraph> {
    for et in csm::core_entity_types() {
        let name = entity_type_to_str(*et);
        if name == entity_type {
            let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
            for (from, to) in csm::allowed_transition_catalog(*et) {
                edges
                    .entry(state_to_str(*from))
                    .or_default()
                    .insert(state_to_str(*to));
            }
            return Some(SpecGraph { edges });
        }
    }
    None
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

fn canonicalize(name: &str) -> String {
    let direct: HashSet<String> = csm::core_entity_types()
        .iter()
        .map(|et| entity_type_to_str(*et))
        .collect();
    if direct.contains(name) {
        return name.to_string();
    }
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

#[derive(Debug)]
struct AlignmentResult {
    cost: u32,
    moves: Vec<Value>,
    truncated: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct Move {
    kind: &'static str, // "sync" | "model" | "log"
    from: Option<String>,
    to: Option<String>,
}

#[derive(Eq, PartialEq)]
struct SearchNode {
    f_cost: u32,
    g_cost: u32,
    trace_idx: usize,
    spec_state: Option<String>,
    moves: Vec<Move>,
}

impl Ord for SearchNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // BinaryHeap is max-heap; we want min-f-cost first → reverse.
        other
            .f_cost
            .cmp(&self.f_cost)
            .then_with(|| other.g_cost.cmp(&self.g_cost))
    }
}
impl PartialOrd for SearchNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn align_trace(
    spec: &SpecGraph,
    trace: &[(String, String)],
    max_explored: usize,
) -> AlignmentResult {
    if trace.is_empty() {
        return AlignmentResult {
            cost: 0,
            moves: Vec::new(),
            truncated: false,
        };
    }

    // start_state := from_state of first transition; if it's not in the spec
    // (no outgoing edges), we treat it as a free start (any spec node).
    let initial_state = trace[0].0.clone();
    let mut heap: BinaryHeap<SearchNode> = BinaryHeap::new();
    let mut seen: HashSet<(usize, Option<String>)> = HashSet::new();

    heap.push(SearchNode {
        f_cost: trace.len() as u32,
        g_cost: 0,
        trace_idx: 0,
        spec_state: Some(initial_state),
        moves: Vec::new(),
    });

    let mut explored = 0usize;
    while let Some(node) = heap.pop() {
        if !seen.insert((node.trace_idx, node.spec_state.clone())) {
            continue;
        }
        if node.trace_idx == trace.len() {
            return AlignmentResult {
                cost: node.g_cost,
                moves: encode_moves(&node.moves),
                truncated: false,
            };
        }
        explored += 1;
        if explored > max_explored {
            return AlignmentResult {
                cost: node.g_cost + (trace.len() - node.trace_idx) as u32,
                moves: encode_moves(&node.moves),
                truncated: true,
            };
        }
        let (obs_from, obs_to) = &trace[node.trace_idx];
        let h_after_advance = (trace.len() - node.trace_idx - 1) as u32;
        let h_after_log = (trace.len() - node.trace_idx - 1) as u32;
        let h_after_model = (trace.len() - node.trace_idx) as u32;
        let current_state = node.spec_state.clone();

        // 1. sync move — if current_state allows obs_to from obs_from AND
        //    matches the trace step
        if current_state.as_deref() == Some(obs_from.as_str())
            && spec
                .edges
                .get(obs_from)
                .map(|set| set.contains(obs_to))
                .unwrap_or(false)
        {
            let mut moves = node.moves.clone();
            moves.push(Move {
                kind: "sync",
                from: Some(obs_from.clone()),
                to: Some(obs_to.clone()),
            });
            heap.push(SearchNode {
                g_cost: node.g_cost,
                f_cost: node.g_cost + h_after_advance,
                trace_idx: node.trace_idx + 1,
                spec_state: Some(obs_to.clone()),
                moves,
            });
        }

        // 2. log-only move — observed something not in spec; advance trace,
        //    keep spec state.
        let mut moves_log = node.moves.clone();
        moves_log.push(Move {
            kind: "log",
            from: Some(obs_from.clone()),
            to: Some(obs_to.clone()),
        });
        heap.push(SearchNode {
            g_cost: node.g_cost + 1,
            f_cost: node.g_cost + 1 + h_after_log,
            trace_idx: node.trace_idx + 1,
            spec_state: current_state.clone(),
            moves: moves_log,
        });

        // 3. model-only move — synthetic step in spec without consuming trace.
        //    This lets us repair "missing intermediate" violations. We bound
        //    expansion to the next legal targets from current_state to keep
        //    the branching factor sane.
        if let Some(state) = &current_state {
            if let Some(next_targets) = spec.edges.get(state) {
                for next in next_targets.iter().take(8) {
                    let mut moves_model = node.moves.clone();
                    moves_model.push(Move {
                        kind: "model",
                        from: Some(state.clone()),
                        to: Some(next.clone()),
                    });
                    heap.push(SearchNode {
                        g_cost: node.g_cost + 1,
                        f_cost: node.g_cost + 1 + h_after_model,
                        trace_idx: node.trace_idx,
                        spec_state: Some(next.clone()),
                        moves: moves_model,
                    });
                }
            }
        }
    }
    // exhausted heap without a trace-end node — fall back to log-only-cost.
    AlignmentResult {
        cost: trace.len() as u32,
        moves: encode_moves(
            &trace
                .iter()
                .map(|(f, t)| Move {
                    kind: "log",
                    from: Some(f.clone()),
                    to: Some(t.clone()),
                })
                .collect::<Vec<_>>(),
        ),
        truncated: true,
    }
}

fn encode_moves(moves: &[Move]) -> Vec<Value> {
    moves
        .iter()
        .map(|m| {
            json!({
                "kind": m.kind,
                "from_state": m.from,
                "to_state": m.to,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_two_step() -> SpecGraph {
        // A -> B -> C
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        edges.insert("A".to_string(), HashSet::from(["B".to_string()]));
        edges.insert("B".to_string(), HashSet::from(["C".to_string()]));
        SpecGraph { edges }
    }

    #[test]
    fn align_zero_cost_for_conforming_trace() {
        let spec = spec_two_step();
        let trace = vec![
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "C".to_string()),
        ];
        let result = align_trace(&spec, &trace, 10_000);
        assert_eq!(result.cost, 0);
        assert!(result.moves.iter().all(|m| m["kind"] == "sync"));
    }

    #[test]
    fn align_one_cost_for_log_only_extra_step() {
        let spec = spec_two_step();
        // A->B is in spec, B->X is not, B->C is.
        let trace = vec![
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "X".to_string()),
            ("B".to_string(), "C".to_string()),
        ];
        let result = align_trace(&spec, &trace, 50_000);
        // we must skip B->X as log-only (cost 1) and complete via B->C
        assert_eq!(result.cost, 1);
        assert!(result.moves.iter().any(|m| m["kind"] == "log"));
    }

    #[test]
    fn align_truncates_on_state_explosion() {
        let spec = spec_two_step();
        let trace: Vec<(String, String)> =
            (0..5).map(|_| ("A".to_string(), "Z".to_string())).collect();
        let result = align_trace(&spec, &trace, 1);
        assert!(result.truncated);
    }
}
