//! `narrative_flow_check` tool. The only LLM-backed check tool. Wraps
//! the [`SubSkillRunner`] flow-reviewer call into a
//! [`NarrativeFlowDispatcher`] so the check module's existing entry
//! point [`run_narrative_flow_check`] can drive it without knowing
//! about the tool layer.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::report::checks::{
    record_check_outcome, run_narrative_flow_check, NarrativeFlowDispatcher, NarrativeFlowInput,
    NarrativeFlowOutput,
};
use crate::report::schema::{ensure_schema, open};
use crate::report::schemas::parse_flow_review;
use crate::report::tools::{ok, ToolContext, ToolEnvelope};

const TOOL: &str = "narrative_flow_check";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Args {}

pub fn execute(ctx: &ToolContext, _args: &Args) -> Result<ToolEnvelope> {
    let dispatcher = ContextDispatcher { ctx };
    let outcome = run_narrative_flow_check(ctx.workspace, &dispatcher)?;
    let conn = open(ctx.root)?;
    ensure_schema(&conn)?;
    record_check_outcome(&conn, ctx.run_id, &outcome)?;
    Ok(ok(TOOL, serde_json::to_value(&outcome)?))
}

/// Bridge between the tool-level [`SubSkillRunner`] and the check-module
/// [`NarrativeFlowDispatcher`] surface.
struct ContextDispatcher<'a, 'b> {
    ctx: &'a ToolContext<'b>,
}

impl<'a, 'b> NarrativeFlowDispatcher for ContextDispatcher<'a, 'b> {
    fn run(&self, input: &NarrativeFlowInput) -> Result<NarrativeFlowOutput> {
        let raw_value: &Value = &input.payload;
        let raw = self
            .ctx
            .sub_skill_runner
            .run_flow_reviewer(raw_value)
            .context("flow_review sub-skill returned an error")?;
        let parsed = parse_flow_review(&raw)
            .context("flow_review sub-skill output failed schema validation")?;
        let raw_payload = serde_json::to_value(&parsed).context("re-encode flow_review output")?;
        Ok(NarrativeFlowOutput {
            summary: parsed.summary,
            check_applicable: parsed.check_applicable,
            ready_to_finish: parsed.ready_to_finish,
            needs_revision: parsed.needs_revision,
            candidate_instance_ids: parsed.candidate_instance_ids,
            goals: parsed.goals,
            reasons: parsed.reasons,
            raw_payload,
        })
    }
}
