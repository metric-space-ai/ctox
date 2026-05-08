//! `character_budget_check` tool. Deterministic. Runs the
//! `character_budget` check against the workspace and persists the
//! outcome into `report_check_runs`.

use anyhow::Result;
use serde::Deserialize;

use crate::report::checks::{record_check_outcome, run_character_budget_check};
use crate::report::schema::{ensure_schema, open};
use crate::report::tools::{ok, ToolContext, ToolEnvelope};

const TOOL: &str = "character_budget_check";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Args {}

pub fn execute(ctx: &ToolContext, _args: &Args) -> Result<ToolEnvelope> {
    let outcome = run_character_budget_check(ctx.workspace)?;
    let conn = open(ctx.root)?;
    ensure_schema(&conn)?;
    record_check_outcome(&conn, ctx.run_id, &outcome)?;
    Ok(ok(TOOL, serde_json::to_value(&outcome)?))
}
