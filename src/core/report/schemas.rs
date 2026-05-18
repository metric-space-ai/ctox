//! Sub-skill output schemas.
//!
//! Three sub-skills (block_writer, revision, flow_review) emit
//! schema-validated JSON. The manager parses each result via these
//! structs. Manager -> tool boundary: tool args are scalars (skill_id,
//! instance_ids); markdown only flows through SubSkillBlock.
//!
//! These structs are zod-equivalent in Rust: `serde` round-trip plus an
//! explicit `validate()` method that enforces the constraints documented
//! in `references/check_contracts.md`,
//! `references/sub_skill_writer.md`,
//! `references/sub_skill_revisor.md`,
//! `references/sub_skill_flow_reviewer.md`. On a constraint violation
//! the validator returns an `anyhow!`/`bail!` error whose message names
//! the failing rule precisely so the manager can echo it back to the
//! sub-skill as a re-run hint.

use std::collections::HashSet;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Per-call hard limit on `blocks[]` entries from the writer/revisor
/// sub-skills. Mirrors `MAX_BLOCKS_PER_SKILL_CALL` in the schema layer.
pub const MAX_BLOCKS_PER_SKILL_CALL: usize = 6;
/// Per-call hard limit on `blocking_questions[]` entries.
pub const MAX_BLOCKING_QUESTIONS: usize = 3;
/// Per-block hard limit on `used_reference_ids[]` entries.
pub const MAX_USED_REFERENCE_IDS_PER_BLOCK: usize = 8;
/// Hard limit on `candidate_instance_ids[]` from flow_review and
/// release_guard.
pub const MAX_CANDIDATE_INSTANCE_IDS: usize = 6;
/// Hard limit on `goals[]` from flow_review and release_guard.
pub const MAX_FLOW_REVIEW_GOALS: usize = 8;
/// Hard limit on `reasons[]` from flow_review and release_guard.
pub const MAX_FLOW_REVIEW_REASONS: usize = 6;

/// Output envelope shared by `write_with_skill` and `revise_with_skill`.
/// Schema is identical between the two; the manager differentiates on
/// the call site, not the payload shape.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WriteOrReviseSkillOutput {
    pub summary: String,
    #[serde(default)]
    pub blocking_reason: String,
    #[serde(default)]
    pub blocking_questions: Vec<String>,
    #[serde(default)]
    pub blocks: Vec<SubSkillBlock>,
}

/// One block payload inside `WriteOrReviseSkillOutput.blocks[]`. Field
/// names match the sub-skill instructions verbatim.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubSkillBlock {
    pub instance_id: String,
    pub doc_id: String,
    pub block_id: String,
    pub title: String,
    pub order: i64,
    pub markdown: String,
    pub reason: String,
    #[serde(default)]
    pub used_reference_ids: Vec<String>,
}

/// Output envelope for `narrative_flow_check` (the Flow Review sub-skill)
/// and the deterministic `release_guard_check` lint suite. Schema is
/// identical between the two.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FlowReviewOutput {
    pub summary: String,
    pub check_applicable: bool,
    pub ready_to_finish: bool,
    pub needs_revision: bool,
    #[serde(default)]
    pub candidate_instance_ids: Vec<String>,
    #[serde(default)]
    pub goals: Vec<String>,
    #[serde(default)]
    pub reasons: Vec<String>,
}

impl WriteOrReviseSkillOutput {
    /// Apply every constraint from `check_contracts.md` and the
    /// writer/revisor sub-skill specs. Returns the first violation it
    /// finds; the manager surfaces this string verbatim as a re-run hint.
    pub fn validate(&self) -> Result<()> {
        if self.blocking_questions.len() > MAX_BLOCKING_QUESTIONS {
            bail!(
                "blocking_questions exceeds MAX_BLOCKING_QUESTIONS ({} > {})",
                self.blocking_questions.len(),
                MAX_BLOCKING_QUESTIONS
            );
        }
        for (idx, q) in self.blocking_questions.iter().enumerate() {
            if q.trim().is_empty() {
                bail!("blocking_questions[{idx}] is empty");
            }
        }
        if self.blocks.len() > MAX_BLOCKS_PER_SKILL_CALL {
            bail!(
                "blocks exceeds MAX_BLOCKS_PER_SKILL_CALL ({} > {})",
                self.blocks.len(),
                MAX_BLOCKS_PER_SKILL_CALL
            );
        }
        // If blocks is empty, blocking_reason MUST be non-empty
        // (the writer is reporting it cannot produce any block in this call).
        if self.blocks.is_empty() && self.blocking_reason.trim().is_empty() {
            bail!(
                "blocks is empty but blocking_reason is empty: at least one block \
                 or a non-empty blocking_reason is required"
            );
        }
        // Per-block validation + duplicate instance_id detection.
        let mut seen: HashSet<&str> = HashSet::with_capacity(self.blocks.len());
        let blocking_active =
            !self.blocking_reason.trim().is_empty() || !self.blocking_questions.is_empty();
        for (idx, block) in self.blocks.iter().enumerate() {
            if block.instance_id.trim().is_empty() {
                bail!("blocks[{idx}].instance_id is empty");
            }
            if !seen.insert(block.instance_id.as_str()) {
                bail!(
                    "blocks contains duplicate instance_id {:?}",
                    block.instance_id
                );
            }
            if block.markdown.trim().is_empty() && !blocking_active {
                bail!(
                    "blocks[{idx}].markdown is empty and no blocking_reason was \
                     supplied (instance_id={})",
                    block.instance_id
                );
            }
            if block.used_reference_ids.len() > MAX_USED_REFERENCE_IDS_PER_BLOCK {
                bail!(
                    "blocks[{idx}].used_reference_ids exceeds \
                     MAX_USED_REFERENCE_IDS_PER_BLOCK ({} > {}) for instance_id={}",
                    block.used_reference_ids.len(),
                    MAX_USED_REFERENCE_IDS_PER_BLOCK,
                    block.instance_id
                );
            }
        }
        Ok(())
    }
}

impl FlowReviewOutput {
    /// Apply every constraint from `check_contracts.md` plus the
    /// flag-combination matrix from `sub_skill_flow_reviewer.md`.
    pub fn validate(&self) -> Result<()> {
        if self.candidate_instance_ids.len() > MAX_CANDIDATE_INSTANCE_IDS {
            bail!(
                "candidate_instance_ids exceeds MAX_CANDIDATE_INSTANCE_IDS ({} > {})",
                self.candidate_instance_ids.len(),
                MAX_CANDIDATE_INSTANCE_IDS
            );
        }
        if self.goals.len() > MAX_FLOW_REVIEW_GOALS {
            bail!(
                "goals exceeds MAX_FLOW_REVIEW_GOALS ({} > {})",
                self.goals.len(),
                MAX_FLOW_REVIEW_GOALS
            );
        }
        if self.reasons.len() > MAX_FLOW_REVIEW_REASONS {
            bail!(
                "reasons exceeds MAX_FLOW_REVIEW_REASONS ({} > {})",
                self.reasons.len(),
                MAX_FLOW_REVIEW_REASONS
            );
        }
        // ready_to_finish AND needs_revision -> contradiction.
        if self.ready_to_finish && self.needs_revision {
            bail!(
                "ready_to_finish=true and needs_revision=true is a contradiction; \
                 only one of these flags may be true at a time"
            );
        }
        // !check_applicable -> ready_to_finish must be true and
        // needs_revision must be false (per the host loop-end gate's
        // shorthand: a non-applicable check counts as ready).
        if !self.check_applicable && !self.ready_to_finish {
            bail!(
                "check_applicable=false requires ready_to_finish=true \
                 (a non-applicable check is treated as ready by the host gate)"
            );
        }
        if !self.check_applicable && self.needs_revision {
            bail!(
                "check_applicable=false forbids needs_revision=true \
                 (the document is structurally incomplete; call the writer, not the revisor)"
            );
        }
        // needs_revision=true requires a non-empty candidate set and a
        // non-empty goal set (the manager cannot route a revision call
        // without targets and corrective goals).
        if self.needs_revision {
            if self.candidate_instance_ids.is_empty() {
                bail!("needs_revision=true requires non-empty candidate_instance_ids[]");
            }
            if self.goals.is_empty() {
                bail!(
                    "needs_revision=true requires non-empty goals[] \
                     (each candidate must have at least one matching goal)"
                );
            }
        }
        // ready_to_finish=true must come with empty action arrays.
        if self.ready_to_finish && !self.candidate_instance_ids.is_empty() {
            bail!("ready_to_finish=true forbids candidate_instance_ids[] entries");
        }
        if self.ready_to_finish && !self.goals.is_empty() {
            bail!("ready_to_finish=true forbids goals[] entries");
        }
        Ok(())
    }
}

/// Parse a writer or revisor sub-skill output string and validate it.
/// Returns `Err` if the JSON is malformed or any constraint is violated.
pub fn parse_write_or_revise(raw: &str) -> Result<WriteOrReviseSkillOutput> {
    let parsed: WriteOrReviseSkillOutput = serde_json::from_str(raw)?;
    parsed.validate()?;
    Ok(parsed)
}

/// Parse a flow_review (or release_guard) output string and validate it.
pub fn parse_flow_review(raw: &str) -> Result<FlowReviewOutput> {
    let parsed: FlowReviewOutput = serde_json::from_str(raw)?;
    parsed.validate()?;
    Ok(parsed)
}
