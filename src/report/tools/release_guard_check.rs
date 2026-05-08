//! `release_guard_check` tool. Deterministic lint suite. Runs the
//! `release_guard` check against the workspace and persists the
//! outcome into `report_check_runs`.

use anyhow::Result;
use serde::Deserialize;

use crate::report::checks::{record_check_outcome, run_release_guard_check};
use crate::report::schema::{ensure_schema, open};
use crate::report::tools::{ok, ToolContext, ToolEnvelope};

const TOOL: &str = "release_guard_check";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Args {}

pub fn execute(ctx: &ToolContext, _args: &Args) -> Result<ToolEnvelope> {
    let outcome = run_release_guard_check(ctx.workspace)?;
    let conn = open(ctx.root)?;
    ensure_schema(&conn)?;
    record_check_outcome(&conn, ctx.run_id, &outcome)?;
    Ok(ok(TOOL, serde_json::to_value(&outcome)?))
}
