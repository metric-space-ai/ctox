//! Adapter around the existing `tools/web-stack` deep_research engine.
//!
//! This module is kept thin on purpose: the report skill never re-implements
//! search/read; it composes on top of `run_ctox_deep_research_tool`.

use anyhow::Result;
use serde_json::Value;
use std::path::Path;

use ctox_web_stack::{DeepResearchDepth, DeepResearchRequest, run_ctox_deep_research_tool};

/// Run a deep_research call rooted in the run topic. Returns the raw evidence
/// bundle; the evidence stage is responsible for normalising the bundle into
/// `report_evidence` rows.
pub fn deep_research(
    root: &Path,
    query: &str,
    focus: Option<&str>,
    depth: &str,
    max_sources: usize,
) -> Result<Value> {
    let depth_enum = DeepResearchDepth::from_label(depth).unwrap_or_default();
    let request = DeepResearchRequest {
        query: query.to_string(),
        focus: focus.map(|s| s.to_string()),
        depth: depth_enum,
        max_sources: max_sources.clamp(8, 200),
        include_annas_archive: false,
        include_papers: true,
        workspace: None,
        persist_workspace: false,
    };
    run_ctox_deep_research_tool(root, &request)
}
