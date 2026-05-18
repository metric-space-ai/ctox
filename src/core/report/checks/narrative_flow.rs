//! Narrative-flow check.
//!
//! This check delegates to the LLM-backed `flow_review` sub-skill —
//! the only one of the four loop-end checks that is not deterministic.
//! Wave 5 wires a real dispatcher; this file ships only the trait
//! surface plus a stub-failing implementation.
//!
//! TODO(Wave 5): wire a real [`NarrativeFlowDispatcher`] that calls
//! the `flow_review_skill` via the in-process inference pipeline. The
//! manager loop will inject the dispatcher into
//! [`run_narrative_flow_check`].

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::report::checks::{dedupe_keep_order, CheckOutcome};
use crate::report::workspace::Workspace;

const CHECK_KIND: &str = "narrative_flow";

/// Builds the JSON input bundle the dispatcher passes to the
/// `flow_review` sub-skill. The default implementation pulls the
/// workspace's `narrative_flow_input` payload verbatim.
pub trait NarrativeFlowInputProvider {
    fn build_input(&self, workspace: &Workspace) -> Result<NarrativeFlowInput>;
}

/// Opaque input bundle. We hold it as `Value` rather than a typed
/// struct so the dispatcher (which lives in a different wave) can
/// evolve the schema without a breaking change here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeFlowInput {
    pub payload: Value,
}

/// LLM-backed dispatcher. Wave 5 will provide a real implementation
/// that calls the `flow_review_skill`.
pub trait NarrativeFlowDispatcher {
    fn run(&self, input: &NarrativeFlowInput) -> Result<NarrativeFlowOutput>;
}

/// Translated output from the `flow_review` sub-skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeFlowOutput {
    pub summary: String,
    pub check_applicable: bool,
    pub ready_to_finish: bool,
    pub needs_revision: bool,
    pub candidate_instance_ids: Vec<String>,
    pub goals: Vec<String>,
    pub reasons: Vec<String>,
    pub raw_payload: Value,
}

/// Default input provider: workspace `narrative_flow_input` JSON.
pub struct DefaultInputProvider;

impl NarrativeFlowInputProvider for DefaultInputProvider {
    fn build_input(&self, workspace: &Workspace) -> Result<NarrativeFlowInput> {
        let payload = workspace.narrative_flow_input()?;
        Ok(NarrativeFlowInput { payload })
    }
}

/// Stub dispatcher that errors on every call. Replace in Wave 5.
pub struct UnimplementedDispatcher;

impl NarrativeFlowDispatcher for UnimplementedDispatcher {
    fn run(&self, _input: &NarrativeFlowInput) -> Result<NarrativeFlowOutput> {
        Err(anyhow!(
            "narrative_flow dispatcher not yet wired (Wave 5 — see TODO in src/report/checks/narrative_flow.rs)"
        ))
    }
}

/// Run the narrative-flow check. Builds input via the workspace, then
/// dispatches to the LLM-backed `flow_review` sub-skill. With fewer
/// than two committed blocks the check reports
/// `check_applicable=false, ready_to_finish=true` without invoking the
/// dispatcher.
pub fn run_narrative_flow_check<D: NarrativeFlowDispatcher>(
    workspace: &Workspace,
    dispatcher: &D,
) -> Result<CheckOutcome> {
    let committed = workspace.committed_blocks()?;
    if committed.len() < 2 {
        let payload = json!({
            "summary": "Weniger als zwei committete Blöcke — Flow-Review wird übersprungen.",
            "check_applicable": false,
            "ready_to_finish": true,
            "needs_revision": false,
            "candidate_instance_ids": Value::Array(Vec::new()),
            "goals": Value::Array(Vec::new()),
            "reasons": Value::Array(Vec::new()),
        });
        return Ok(CheckOutcome {
            check_kind: CHECK_KIND.to_string(),
            summary: "Weniger als zwei committete Blöcke — Flow-Review wird übersprungen."
                .to_string(),
            check_applicable: false,
            ready_to_finish: true,
            needs_revision: false,
            candidate_instance_ids: Vec::new(),
            goals: Vec::new(),
            reasons: Vec::new(),
            raw_payload: payload,
        }
        .cap());
    }

    let provider = DefaultInputProvider;
    let input = provider.build_input(workspace)?;
    let output = dispatcher.run(&input)?;

    let candidate_instance_ids = dedupe_keep_order(output.candidate_instance_ids);
    let goals = dedupe_keep_order(output.goals);
    let reasons = dedupe_keep_order(output.reasons);

    Ok(CheckOutcome {
        check_kind: CHECK_KIND.to_string(),
        summary: output.summary,
        check_applicable: output.check_applicable,
        ready_to_finish: output.ready_to_finish,
        needs_revision: output.needs_revision,
        candidate_instance_ids,
        goals,
        reasons,
        raw_payload: output.raw_payload,
    }
    .cap())
}
