//! Deterministic completeness check.
//!
//! Mirrors the Förderantrag agent's `computeCompleteness` and the
//! `completenessCheck` tool. For each required block in the resolved
//! blueprint we measure the committed markdown length against the
//! block's `min_chars` floor; the run is `ready_to_finish` only when
//! every required block is `done` (i.e. >= 65% of `min_chars`).

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use serde_json::{json, Value};

use crate::report::asset_pack::{BlockLibraryEntry, OptionalModule};
use crate::report::checks::{dedupe_keep_order, CheckOutcome};
use crate::report::workspace::{BlockRecord, Workspace};

const CHECK_KIND: &str = "completeness";

#[derive(Debug, Clone)]
struct ExpectedBlock {
    instance_id: String,
    block_id: String,
    title: String,
    required: bool,
    module: Option<String>,
    min_chars: u32,
    status: BlockStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockStatus {
    Done,
    Thin,
    Missing,
}

impl BlockStatus {
    fn as_str(self) -> &'static str {
        match self {
            BlockStatus::Done => "done",
            BlockStatus::Thin => "thin",
            BlockStatus::Missing => "missing",
        }
    }
}

pub fn run_completeness_check(workspace: &Workspace) -> Result<CheckOutcome> {
    let metadata = workspace.run_metadata()?;
    let asset_pack = crate::report::asset_pack::AssetPack::load()?;
    let report_type = asset_pack.report_type(&metadata.report_type_id)?;
    let blueprint = asset_pack.document_blueprint(&report_type.document_blueprint_id)?;

    let modules: HashMap<String, &OptionalModule> = asset_pack
        .optional_modules
        .iter()
        .map(|m| (m.id.clone(), m))
        .collect();
    let allowed: HashSet<&str> = report_type
        .block_library_keys
        .iter()
        .map(String::as_str)
        .collect();
    let active_modules: HashSet<&str> = report_type
        .default_modules
        .iter()
        .map(String::as_str)
        .collect();

    let committed = workspace.committed_blocks()?;
    let by_instance: HashMap<String, &BlockRecord> = committed
        .iter()
        .map(|b| (b.instance_id.clone(), b))
        .collect();

    let mut expected: Vec<ExpectedBlock> = Vec::new();
    for entry in &blueprint.sequence {
        if !allowed.contains(entry.block_id.as_str()) {
            continue;
        }
        if let Some(module_id) = &entry.module {
            if !active_modules.contains(module_id.as_str())
                && !modules.contains_key(module_id.as_str())
            {
                continue;
            }
            if !active_modules.contains(module_id.as_str()) {
                continue;
            }
        }
        let instance_id = format!("{}__{}", entry.doc_id, entry.block_id);
        let library_entry: BlockLibraryEntry = asset_pack.block_library_entry(&entry.block_id)?;
        let status = block_status(by_instance.get(&instance_id).copied(), &library_entry);
        expected.push(ExpectedBlock {
            instance_id,
            block_id: entry.block_id.clone(),
            title: library_entry.title.clone(),
            required: entry.required,
            module: entry.module.clone(),
            min_chars: library_entry.min_chars,
            status,
        });
    }

    if expected.is_empty() {
        // No package or no expected blocks at all.
        let payload = json!({
            "total_required": 0,
            "done_required": 0,
            "missing_required": Value::Array(Vec::new()),
            "thin_required": Value::Array(Vec::new()),
            "missing_optional": Value::Array(Vec::new()),
            "ready_to_finish": true,
        });
        return Ok(CheckOutcome {
            check_kind: CHECK_KIND.to_string(),
            summary: "Keine Pflichtblöcke definiert.".to_string(),
            check_applicable: false,
            ready_to_finish: true,
            needs_revision: false,
            candidate_instance_ids: Vec::new(),
            goals: Vec::new(),
            reasons: Vec::new(),
            raw_payload: payload,
        }
        .cap());
    }

    let mut total_required = 0usize;
    let mut done_required = 0usize;
    let mut missing_required: Vec<&ExpectedBlock> = Vec::new();
    let mut thin_required: Vec<&ExpectedBlock> = Vec::new();
    let mut missing_optional: Vec<&ExpectedBlock> = Vec::new();
    for block in &expected {
        if block.required {
            total_required += 1;
            match block.status {
                BlockStatus::Done => done_required += 1,
                BlockStatus::Thin => thin_required.push(block),
                BlockStatus::Missing => missing_required.push(block),
            }
        } else if block.status == BlockStatus::Missing {
            missing_optional.push(block);
        }
    }

    let ready_to_finish =
        total_required > 0 && done_required == total_required && thin_required.is_empty();
    let needs_revision = !ready_to_finish;

    // Build candidate ids: missing first, then thin, capped at 6.
    let mut candidates: Vec<String> = Vec::new();
    for b in &missing_required {
        candidates.push(b.instance_id.clone());
    }
    for b in &thin_required {
        candidates.push(b.instance_id.clone());
    }
    let candidates = dedupe_keep_order(candidates);

    // Goals: per-block phrasing.
    let mut goals: Vec<String> = Vec::new();
    for b in &missing_required {
        goals.push(format!("Vervollständige Block {}", b.title));
    }
    for b in &thin_required {
        goals.push(format!(
            "Verstärke Block {} auf mindestens {} Zeichen",
            b.title, b.min_chars
        ));
    }
    let goals = dedupe_keep_order(goals);

    // Reasons: compact diagnosis strings.
    let mut reasons: Vec<String> = Vec::new();
    if !missing_required.is_empty() {
        reasons.push(format!(
            "{} Pflichtblöcke fehlen vollständig.",
            missing_required.len()
        ));
    }
    if !thin_required.is_empty() {
        reasons.push(format!(
            "{} Pflichtblöcke unter dem 65%-Floor.",
            thin_required.len()
        ));
    }
    if !missing_optional.is_empty() {
        reasons.push(format!(
            "{} optionale Blöcke noch nicht ausgearbeitet.",
            missing_optional.len()
        ));
    }
    let reasons = dedupe_keep_order(reasons);

    let summary = if ready_to_finish {
        "Alle Pflichtblöcke abgeschlossen.".to_string()
    } else {
        format!(
            "{}/{} Pflichtblöcke abgeschlossen.",
            done_required, total_required
        )
    };

    let payload = json!({
        "total_required": total_required,
        "done_required": done_required,
        "missing_required": missing_required.iter().map(|b| b.instance_id.clone()).collect::<Vec<_>>(),
        "thin_required": thin_required.iter().map(|b| b.instance_id.clone()).collect::<Vec<_>>(),
        "missing_optional": missing_optional.iter().map(|b| b.instance_id.clone()).collect::<Vec<_>>(),
        "ready_to_finish": ready_to_finish,
        "blocks": expected.iter().map(|b| json!({
            "instance_id": b.instance_id,
            "block_id": b.block_id,
            "title": b.title,
            "required": b.required,
            "module": b.module,
            "min_chars": b.min_chars,
            "status": b.status.as_str(),
        })).collect::<Vec<_>>(),
    });

    Ok(CheckOutcome {
        check_kind: CHECK_KIND.to_string(),
        summary,
        check_applicable: true,
        ready_to_finish,
        needs_revision,
        candidate_instance_ids: candidates,
        goals,
        reasons,
        raw_payload: payload,
    }
    .cap())
}

fn block_status(record: Option<&BlockRecord>, entry: &BlockLibraryEntry) -> BlockStatus {
    match record {
        None => BlockStatus::Missing,
        Some(block) => {
            let chars = block.markdown.trim().chars().count();
            let floor = ((entry.min_chars as f64) * 0.65).ceil() as usize;
            if entry.min_chars == 0 || chars >= floor {
                BlockStatus::Done
            } else if chars == 0 {
                BlockStatus::Missing
            } else {
                BlockStatus::Thin
            }
        }
    }
}
