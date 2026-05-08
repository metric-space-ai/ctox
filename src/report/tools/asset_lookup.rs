//! `asset_lookup` tool. Returns the block_defs / references / style_guide
//! / document_flow payload for one or more `instance_id`s. Empty
//! `instance_ids` means "every block in the resolved blueprint".

use anyhow::Result;
use serde::Deserialize;

use crate::report::tools::{ok, ToolContext, ToolEnvelope};

const TOOL: &str = "asset_lookup";

fn default_include_references() -> bool {
    true
}

fn default_include_report_type() -> bool {
    false
}

#[derive(Debug, Clone, Deserialize)]
pub struct Args {
    #[serde(default)]
    pub instance_ids: Vec<String>,
    /// Currently informational. The workspace always returns the
    /// reference catalogue scoped to the in-scope `block_defs[]`; the
    /// flag is preserved so the wire signature matches the JS agent.
    #[serde(default = "default_include_references")]
    pub include_references: bool,
    /// When `true`, the response carries the resolved `report_type`
    /// object. The manager sets this on the bootstrap call only.
    #[serde(default = "default_include_report_type")]
    pub include_report_type: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            instance_ids: Vec::new(),
            include_references: default_include_references(),
            include_report_type: default_include_report_type(),
        }
    }
}

pub fn execute(ctx: &ToolContext, args: &Args) -> Result<ToolEnvelope> {
    let payload = ctx
        .workspace
        .asset_lookup(&args.instance_ids, args.include_report_type)?;
    let _ = args.include_references;
    Ok(ok(TOOL, payload))
}
