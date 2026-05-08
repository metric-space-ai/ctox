//! `write_with_skill` tool. Validates the requested `instance_ids`,
//! builds the writer sub-skill input bundle from the workspace, invokes
//! the Wave-5 [`SubSkillRunner`], parses & validates the JSON output,
//! and stages the resulting blocks into `report_pending_blocks`.
//!
//! No markdown ever flows through the tool's argument struct: the
//! manager only supplies `instance_ids[]` and an optional `brief`.
//! Markdown enters the workspace via the validated sub-skill output and
//! the patch layer.

use std::collections::HashSet;

use anyhow::{Context, Result};
use rusqlite::params;
use serde::Deserialize;
use serde_json::json;

use crate::report::patch::{
    record_skill_run, stage_pending_blocks, SkillRunKind, SkillRunRecord, StagedBlock,
};
use crate::report::schema::{ensure_schema, new_id, now_iso, open};
use crate::report::schemas::{parse_write_or_revise, MAX_BLOCKS_PER_SKILL_CALL};
use crate::report::tools::{err, ok, user_input, ToolContext, ToolEnvelope};
use crate::report::workspace::SkillMode;

const TOOL: &str = "write_with_skill";

#[derive(Debug, Clone, Deserialize)]
pub struct Args {
    pub instance_ids: Vec<String>,
    #[serde(default)]
    pub brief: String,
}

pub fn execute(ctx: &ToolContext, args: &Args) -> Result<ToolEnvelope> {
    if args.instance_ids.is_empty() {
        return Ok(err(
            TOOL,
            "write_with_skill requires at least one instance_id".into(),
        ));
    }
    if args.instance_ids.len() > MAX_BLOCKS_PER_SKILL_CALL {
        return Ok(err(
            TOOL,
            format!(
                "write_with_skill accepts at most {MAX_BLOCKS_PER_SKILL_CALL} instance_ids per call (got {})",
                args.instance_ids.len()
            ),
        ));
    }

    // Cross-type guard: every instance_id's resolved block_id must be in
    // the run's report_type.block_library_keys[].
    let metadata = ctx.workspace.run_metadata()?;
    let report_type = ctx.asset_pack.report_type(&metadata.report_type_id)?;
    let allowed: HashSet<&str> = report_type
        .block_library_keys
        .iter()
        .map(String::as_str)
        .collect();
    for instance_id in &args.instance_ids {
        let block_id = block_id_from_instance(instance_id);
        if !allowed.contains(block_id) {
            return Ok(err(
                TOOL,
                format!(
                    "instance_id {instance_id:?} resolves to block_id {block_id:?}, which is not in report_type.block_library_keys for report type {:?}",
                    metadata.report_type_id
                ),
            ));
        }
    }

    // Build sub-skill input.
    let input =
        ctx.workspace
            .skill_input(SkillMode::Write, &args.instance_ids, Some(&args.brief), &[])?;

    // Invoke the writer.
    let raw = ctx
        .sub_skill_runner
        .run_writer(&input)
        .context("writer sub-skill returned an error")?;
    let parsed =
        parse_write_or_revise(&raw).context("writer sub-skill output failed schema validation")?;

    // Persist the skill run.
    let conn = open(ctx.root)?;
    ensure_schema(&conn)?;
    let skill_run_id = new_id("skill_write");
    let raw_output_json =
        serde_json::to_value(&parsed).context("re-encode writer output for skill_run record")?;
    let blocks: Vec<StagedBlock> = parsed
        .blocks
        .iter()
        .map(|b| StagedBlock {
            instance_id: b.instance_id.clone(),
            doc_id: b.doc_id.clone(),
            block_id: b.block_id.clone(),
            block_template_id: b.block_id.clone(),
            title: b.title.clone(),
            ord: b.order,
            markdown: b.markdown.clone(),
            reason: b.reason.clone(),
            used_reference_ids: b.used_reference_ids.clone(),
        })
        .collect();
    let blocking_reason = if parsed.blocking_reason.trim().is_empty() {
        None
    } else {
        Some(parsed.blocking_reason.clone())
    };
    let record = SkillRunRecord {
        skill_run_id: skill_run_id.clone(),
        run_id: ctx.run_id.to_string(),
        kind: SkillRunKind::Write,
        summary: parsed.summary.clone(),
        blocking_reason,
        blocking_questions: parsed.blocking_questions.clone(),
        blocks: blocks.clone(),
        raw_output: raw_output_json,
    };
    record_skill_run(&conn, &record)?;
    stage_pending_blocks(
        &conn,
        ctx.run_id,
        &skill_run_id,
        SkillRunKind::Write,
        &blocks,
    )?;

    // Soft-error path: empty blocks + blocking_reason + blocking_questions.
    if parsed.blocks.is_empty() {
        if !parsed.blocking_questions.is_empty() && !parsed.blocking_reason.trim().is_empty() {
            // Persist a question card so the operator answers via the same
            // surface as ask_user.
            let question_id = new_id("q");
            let questions_json = serde_json::to_string(&parsed.blocking_questions)
                .context("encode blocking_questions for question card")?;
            conn.execute(
                "INSERT INTO report_questions (
                     question_id, run_id, section, reason, questions_json,
                     allow_fallback, raised_at, answered_at, answer_text
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, NULL, NULL)",
                params![
                    question_id,
                    ctx.run_id,
                    "write_with_skill",
                    parsed.blocking_reason,
                    questions_json,
                    now_iso(),
                ],
            )
            .context("failed to persist write_with_skill question card")?;
            let payload = json!({
                "skill_run_id": skill_run_id,
                "question_id": question_id,
                "blocking_reason": parsed.blocking_reason,
                "questions": parsed.blocking_questions,
            });
            return Ok(user_input(TOOL, payload));
        }
        return Ok(err(TOOL, "skill_empty".into()));
    }

    let titles: Vec<String> = parsed.blocks.iter().map(|b| b.title.clone()).collect();
    let instance_ids: Vec<String> = parsed
        .blocks
        .iter()
        .map(|b| b.instance_id.clone())
        .collect();
    let data = json!({
        "skill_run_id": skill_run_id,
        "blocks_count": parsed.blocks.len(),
        "instance_ids": instance_ids,
        "titles": titles,
        "summary": parsed.summary,
    });
    Ok(ok(TOOL, data))
}

/// Instance ids are minted as `{doc_id}__{block_id}`. The block id is
/// the suffix after the last `__` separator — see
/// `Workspace::asset_lookup` for the canonical generator.
fn block_id_from_instance(instance_id: &str) -> &str {
    instance_id
        .rsplit_once("__")
        .map(|(_, b)| b)
        .unwrap_or(instance_id)
}
