//! `apply_block_patch` tool. Commits previously-staged blocks from
//! `report_pending_blocks` into `report_blocks` via
//! [`crate::report::patch::apply_block_patch`].
//!
//! The arg struct uses `deny_unknown_fields` so any attempt to smuggle
//! `markdown` or other prose through the tool boundary is rejected at
//! deserialisation time. The schema-level guarantee — that markdown only
//! flows in via the recorded skill run — is enforced here.

use anyhow::Result;
use serde::Deserialize;
use serde_json::json;

use crate::report::patch::{apply_block_patch as run_apply, PatchSelection};
use crate::report::schema::{ensure_schema, open};
use crate::report::tools::{err, ok, ToolContext, ToolEnvelope};

const TOOL: &str = "apply_block_patch";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    pub skill_run_id: String,
    #[serde(default)]
    pub instance_ids: Vec<String>,
    #[serde(default)]
    pub used_research_ids: Vec<String>,
}

pub fn execute(ctx: &ToolContext, args: &Args) -> Result<ToolEnvelope> {
    if args.skill_run_id.trim().is_empty() {
        return Ok(err(TOOL, "skill_run_id is required".into()));
    }

    let conn = open(ctx.root)?;
    ensure_schema(&conn)?;

    // Verify the skill run exists for this run.
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM report_skill_runs WHERE skill_run_id = ?1 AND run_id = ?2",
        rusqlite::params![args.skill_run_id, ctx.run_id],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Ok(err(
            TOOL,
            format!(
                "skill_run_id {:?} not found for run {:?}",
                args.skill_run_id, ctx.run_id
            ),
        ));
    }

    let instance_ids = if args.instance_ids.is_empty() {
        None
    } else {
        Some(args.instance_ids.clone())
    };
    let selection = PatchSelection {
        skill_run_id: args.skill_run_id.clone(),
        instance_ids,
        used_research_ids: args.used_research_ids.clone(),
    };

    let outcome = run_apply(&conn, ctx.run_id, &selection)?;
    let data = json!({
        "skill_run_id": args.skill_run_id,
        "changed_blocks": outcome.committed_block_ids,
        "count": outcome.committed_block_ids.len(),
        "now": outcome.now_iso,
    });
    Ok(ok(TOOL, data))
}
