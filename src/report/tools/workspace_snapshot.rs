//! `workspace_snapshot` tool. Returns the workspace state payload the
//! manager and sub-skills consume. No arguments, no side effects.

use anyhow::Result;
use serde::Deserialize;

use crate::report::tools::{ok, ToolContext, ToolEnvelope};

const TOOL: &str = "workspace_snapshot";

/// Empty argument struct. The tool takes no parameters but the manager
/// dispatcher always supplies a JSON object; we accept extra keys
/// silently (the JS agent does the same).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Args {}

pub fn execute(ctx: &ToolContext, _args: &Args) -> Result<ToolEnvelope> {
    let snapshot = ctx.workspace.workspace_snapshot()?;
    Ok(ok(TOOL, snapshot))
}
