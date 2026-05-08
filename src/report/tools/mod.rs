//! Manager tool layer. Eleven tools that the manager invokes by name.
//!
//! Each tool returns a JSON [`ToolEnvelope`] so the manager-LLM bridge
//! can pass results unchanged into the conversation. The shape mirrors
//! `Foerdervorhaben-Agent.html` lines 6236-6735 (the JS implementations
//! of the eleven tools), adapted to CTOX's Rust types.
//!
//! The actual sub-skill orchestration (the LLM-side dispatch for
//! `write_with_skill`, `revise_with_skill`, `narrative_flow_check`)
//! lives in Wave 5. This layer takes a [`SubSkillRunner`] trait object
//! so it can compile and be exercised independently.

use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::report::asset_pack::AssetPack;
use crate::report::sources::ResolverStack;
use crate::report::workspace::Workspace;

pub mod apply_block_patch;
pub mod ask_user;
pub mod asset_lookup;
pub mod character_budget_check;
pub mod completeness_check;
pub mod narrative_flow_check;
pub mod public_research;
pub mod release_guard_check;
pub mod revise_with_skill;
pub mod workspace_snapshot;
pub mod write_with_skill;

pub use apply_block_patch::execute as apply_block_patch_execute;
pub use ask_user::execute as ask_user_execute;
pub use asset_lookup::execute as asset_lookup_execute;
pub use character_budget_check::execute as character_budget_check_execute;
pub use completeness_check::execute as completeness_check_execute;
pub use narrative_flow_check::execute as narrative_flow_check_execute;
pub use public_research::execute as public_research_execute;
pub use release_guard_check::execute as release_guard_check_execute;
pub use revise_with_skill::execute as revise_with_skill_execute;
pub use workspace_snapshot::execute as workspace_snapshot_execute;
pub use write_with_skill::execute as write_with_skill_execute;

/// Wave-5 sub-skill dispatch surface. The concrete implementation that
/// wires this trait to the in-process inference pipeline lives in the
/// `sub_skill` module shipped by the next wave; the tool layer only
/// holds a `&dyn SubSkillRunner` so it can compile and be tested
/// independently.
pub trait SubSkillRunner: Send + Sync {
    /// Run the writer sub-skill. Returns the raw JSON output as a string
    /// — the tool layer parses and validates it via
    /// [`crate::report::schemas::parse_write_or_revise`].
    fn run_writer(&self, input: &Value) -> Result<String>;
    /// Run the revisor sub-skill. Same wire shape as the writer.
    fn run_revisor(&self, input: &Value) -> Result<String>;
    /// Run the flow-review sub-skill. Returns the raw JSON output.
    fn run_flow_reviewer(&self, input: &Value) -> Result<String>;
}

/// Execution context threaded through every tool. The manager builds
/// one of these once per turn and reuses it across tool calls.
#[derive(Clone, Copy)]
pub struct ToolContext<'a> {
    pub root: &'a Path,
    pub run_id: &'a str,
    pub workspace: &'a Workspace<'a>,
    pub asset_pack: &'a AssetPack,
    pub resolver: &'a ResolverStack,
    pub sub_skill_runner: &'a dyn SubSkillRunner,
}

/// Unified tool result envelope. Matches the JS agent's `{ok, data, ...}`
/// shape so the manager bridge can pass it through unchanged.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolEnvelope {
    pub ok: bool,
    pub tool: &'static str,
    pub data: Value,
    #[serde(default)]
    pub user_input_required: bool,
    #[serde(default)]
    pub error: Option<String>,
}

/// Build an `ok` envelope with arbitrary JSON `data`.
pub fn ok(tool: &'static str, data: Value) -> ToolEnvelope {
    ToolEnvelope {
        ok: true,
        tool,
        data,
        user_input_required: false,
        error: None,
    }
}

/// Build an error envelope. The `error` string is shown verbatim to the
/// manager-LLM as a re-run hint.
pub fn err(tool: &'static str, error: String) -> ToolEnvelope {
    ToolEnvelope {
        ok: false,
        tool,
        data: Value::Null,
        user_input_required: false,
        error: Some(error),
    }
}

/// Build an envelope that asks the user for input. The manager treats
/// this as a soft-error and ends the run with `decision: needs_user_input`.
pub fn user_input(tool: &'static str, data: Value) -> ToolEnvelope {
    ToolEnvelope {
        ok: false,
        tool,
        data,
        user_input_required: true,
        error: None,
    }
}

/// Stable list of every tool name the manager may call. The manager
/// validates the LLM's `tool` field against this list before dispatch.
pub const TOOL_NAMES: &[&str] = &[
    "workspace_snapshot",
    "asset_lookup",
    "ask_user",
    "public_research",
    "write_with_skill",
    "revise_with_skill",
    "apply_block_patch",
    "completeness_check",
    "character_budget_check",
    "release_guard_check",
    "narrative_flow_check",
];
