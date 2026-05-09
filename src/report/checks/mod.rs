//! The loop-end checks of the deep-research backend.
//!
//! Three are deterministic (no LLM): [`completeness`],
//! [`character_budget`], [`release_guard`]. The fourth,
//! [`narrative_flow`], delegates to an LLM-backed `flow_review`
//! sub-skill via a [`NarrativeFlowDispatcher`] trait so the host can
//! wire the dispatcher in Wave 5 without touching this module.
//!
//! Every check returns a [`CheckOutcome`] with the same envelope shape
//! described in `references/check_contracts.md`. The host gate at
//! `Reviewing -> Finalised` requires all four checks to report
//! `ready_to_finish = true` regardless of what an LLM verdict says.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::report::schema::{new_id, now_iso};

pub mod character_budget;
pub mod completeness;
pub mod deliverable_quality;
pub mod narrative_flow;
pub mod release_guard;

pub use character_budget::run_character_budget_check;
pub use completeness::run_completeness_check;
pub use deliverable_quality::run_deliverable_quality_check;
pub use narrative_flow::{
    run_narrative_flow_check, DefaultInputProvider, NarrativeFlowDispatcher, NarrativeFlowInput,
    NarrativeFlowInputProvider, NarrativeFlowOutput, UnimplementedDispatcher,
};
pub use release_guard::run_release_guard_check;

/// Single return shape for every check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckOutcome {
    pub check_kind: String,
    pub summary: String,
    pub check_applicable: bool,
    pub ready_to_finish: bool,
    pub needs_revision: bool,
    pub candidate_instance_ids: Vec<String>,
    pub goals: Vec<String>,
    pub reasons: Vec<String>,
    pub raw_payload: Value,
}

impl CheckOutcome {
    /// Cap mutation helper used by every concrete check before returning.
    pub fn cap(mut self) -> Self {
        if self.candidate_instance_ids.len() > 6 {
            self.candidate_instance_ids.truncate(6);
        }
        if self.goals.len() > 8 {
            self.goals.truncate(8);
        }
        if self.reasons.len() > 6 {
            self.reasons.truncate(6);
        }
        self
    }
}

/// Persist a check outcome into `report_check_runs`. Returns the new
/// `check_id`. Each invocation is a separate row so the manager can
/// see the full history per check kind.
pub fn record_check_outcome(
    conn: &Connection,
    run_id: &str,
    outcome: &CheckOutcome,
) -> Result<String> {
    let check_id = new_id("check");
    let now = now_iso();
    let payload_json = serde_json::to_string(&outcome.raw_payload)
        .context("failed to encode check outcome payload")?;
    conn.execute(
        "INSERT INTO report_check_runs (
             check_id, run_id, check_kind, checked_at, ready_to_finish,
             needs_revision, payload_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            check_id,
            run_id,
            outcome.check_kind,
            now,
            if outcome.ready_to_finish {
                1_i64
            } else {
                0_i64
            },
            if outcome.needs_revision { 1_i64 } else { 0_i64 },
            payload_json,
        ],
    )
    .context("failed to insert check outcome")?;
    Ok(check_id)
}

/// Deduplicate while preserving first-seen order.
pub(crate) fn dedupe_keep_order<I: IntoIterator<Item = String>>(items: I) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for item in items {
        if seen.insert(item.clone()) {
            out.push(item);
        }
    }
    out
}
