//! `ctox report …` — deep research report runs.
//!
//! This is the Rust backend for the deep-research skill at
//! `skills/system/research/deep-research/`. The skill defines seven report
//! types (feasibility_study, market_research, competitive_analysis,
//! technology_screening, whitepaper, literature_review, decision_brief)
//! and a manager-loop architecture analogous to the Förderantrag agent.
//!
//! Hard rules — encoded by the schema and the manager loop, not by prompt
//! discipline:
//! - Manager passes no markdown into tool arguments; only `skill_id` and
//!   `instance_ids`. Markdown only enters the workspace via sub-skill
//!   output (schema-validated) plus `apply_block_patch`.
//! - max 6 instance_ids per write/revise call (schema-enforced).
//! - All four checks (completeness, character_budget, release_guard,
//!   narrative_flow) must report `ready_to_finish=true` before a run can
//!   transition to `Finalised`. The host overrides any LLM `finished`
//!   verdict that violates this gate.

pub mod asset_pack;
pub mod schema;
pub mod state;
pub mod workspace;

// The following modules are built in subsequent waves. Declared here as
// `pub mod` placeholders so other waves can drop their files in without
// touching this file.
pub mod checks;
pub mod cli;
pub mod manager;
pub mod manager_prompt;
pub mod mission_hook;
pub mod patch;
pub mod render;
pub mod schemas;
pub mod sources;
pub mod sub_skill;
pub mod tools;

#[cfg(test)]
mod tests;
