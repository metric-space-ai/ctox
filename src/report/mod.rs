//! `ctox report …` — decision-grade research report runs.
//!
//! This module is the deterministic Rust backend behind the
//! `skills/system/research/systematic-research/` skill's decision-report
//! mode. The skill itself is the prompt that drives the harness LLM;
//! this backend exposes a set of `ctox report …` CLI subcommands the
//! harness LLM calls (via Bash) to create runs, register evidence, stage
//! and commit block markdown, run the four deterministic checks, and
//! render the final manuscript.
//!
//! There is no LLM loop in this backend — every command is a deterministic
//! transform on the SQLite report store. The intelligence lives in the
//! harness, not here.
//!
//! Schema-encoded hard rules:
//! - max 6 staged blocks per `block-stage` call.
//! - Every `instance_id` must resolve to a `block_id` in the run's
//!   `report_type.block_library_keys[]`.
//! - All four checks (completeness, character_budget, release_guard,
//!   narrative_flow) must return `ready_to_finish=true` before
//!   `ctox report finalise` succeeds.

pub mod asset_pack;
pub mod schema;
pub mod state;
pub mod workspace;

pub mod checks;
pub mod cli;
pub mod mission_hook;
pub mod patch;
pub mod render;
pub mod schemas;
pub mod sources;

#[cfg(test)]
mod tests;
