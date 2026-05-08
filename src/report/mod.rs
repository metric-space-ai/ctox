//! `ctox report` — deep research report runs (feasibility, market research, …).
//!
//! Architecture: see CLAUDE.md and the design notes in
//! `skills/system/research/deep-research/references/stage_contracts.md`.
//!
//! Hard rules enforced here, not by prompt discipline:
//! - All durable state lives in `runtime/ctox.sqlite3` under `report_*` tables.
//! - Claims are first-class DB rows with FK to evidence, never free prose.
//! - `draft` is deterministic; it cannot invent prose.
//! - `render` refuses without a prior `check overall_pass=1` for that version.
//! - `revise` requires a body-hash change (witness of progress).

pub mod blueprints;
pub mod check;
pub mod cli;
pub mod claims;
pub mod critique;
pub mod draft;
pub mod evidence;
pub mod manuscript;
pub mod render;
pub mod runs;
pub mod scope;
pub mod scoring;
pub mod sources;
pub mod state_machine;
pub mod store;

pub use cli::handle_report_command;

#[cfg(test)]
pub mod tests;
