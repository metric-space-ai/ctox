//! Deterministic character-budget check.
//!
//! Compares the run's actual character count (sum of trimmed
//! committed-block markdown) against the report-type's
//! `typical_chars` target. Fires `ready_to_finish = false` only when
//! the delta is severe (±50%); minor over/undershoots inside ±20% are
//! acceptable for a finished verdict.

use anyhow::Result;
use serde_json::{json, Value};

use crate::report::checks::{dedupe_keep_order, CheckOutcome};
use crate::report::workspace::{BlockRecord, Workspace};

const CHECK_KIND: &str = "character_budget";

pub fn run_character_budget_check(workspace: &Workspace) -> Result<CheckOutcome> {
    let metadata = workspace.run_metadata()?;
    let asset_pack = crate::report::asset_pack::AssetPack::load()?;
    let report_type = asset_pack.report_type(&metadata.report_type_id)?;
    let target_chars = report_type.typical_chars as i64;

    let committed = workspace.committed_blocks()?;
    let actual_chars: i64 = committed
        .iter()
        .map(|b| b.markdown.trim().chars().count() as i64)
        .sum();
    let delta_chars: i64 = actual_chars - target_chars;

    if committed.is_empty() {
        let payload = json!({
            "target_chars": target_chars,
            "actual_chars": actual_chars,
            "delta_chars": delta_chars,
            "tolerance": 0.20_f64,
            "within_tolerance": true,
            "severely_off_target": false,
            "status": "not_started",
            "summary": "Noch keine Blöcke committet.",
        });
        return Ok(CheckOutcome {
            check_kind: CHECK_KIND.to_string(),
            summary: "Noch keine Blöcke committet.".to_string(),
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

    let abs_delta = delta_chars.unsigned_abs() as i64;
    let within_tolerance = if target_chars == 0 {
        actual_chars == 0
    } else {
        abs_delta <= target_chars / 5
    };
    let severely_off_target = if target_chars == 0 {
        false
    } else {
        abs_delta > target_chars / 2
    };

    let status = if within_tolerance {
        "within"
    } else if delta_chars < 0 {
        if severely_off_target {
            "severely_off"
        } else {
            "low"
        }
    } else if severely_off_target {
        "severely_off"
    } else {
        "high"
    };

    let summary = match status {
        "within" => "Im Korridor.".to_string(),
        "low" | "severely_off" if delta_chars < 0 => {
            format!("Unter Ziel um ~{} Zeichen.", abs_delta)
        }
        _ => format!("Über Ziel um ~{} Zeichen.", abs_delta),
    };

    let ready_to_finish = within_tolerance;
    let needs_revision = severely_off_target;

    // Candidate ids:
    // - over budget -> longest blocks first (up to 6)
    // - under budget -> blocks below their min_chars (up to 6)
    let mut candidate_blocks: Vec<&BlockRecord> = Vec::new();
    let mut goals: Vec<String> = Vec::new();
    if delta_chars > 0 {
        // Over budget — longest first.
        let mut sorted: Vec<&BlockRecord> = committed.iter().collect();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.markdown.trim().chars().count()));
        for block in sorted.into_iter().take(6) {
            candidate_blocks.push(block);
        }
        // Distribute goal hints proportional to overshoot per block.
        let total_actual: i64 = committed
            .iter()
            .map(|b| b.markdown.trim().chars().count() as i64)
            .sum::<i64>()
            .max(1);
        let overshoot = abs_delta;
        for block in &candidate_blocks {
            let block_chars = block.markdown.trim().chars().count() as i64;
            let trim_target =
                ((block_chars as f64) * (overshoot as f64) / (total_actual as f64)).round() as i64;
            let trim_target = trim_target.max(1);
            goals.push(format!(
                "Kürze Block {} um ~{} Zeichen",
                block.title, trim_target
            ));
        }
    } else if delta_chars < 0 {
        // Under budget — find blocks below their min_chars.
        for block in &committed {
            let entry = match asset_pack.block_library_entry(&block.block_id) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let chars = block.markdown.trim().chars().count() as i64;
            let min_chars = entry.min_chars as i64;
            if min_chars > 0 && chars < min_chars {
                candidate_blocks.push(block);
                let needed = (min_chars - chars).max(1);
                goals.push(format!(
                    "Erweitere Block {} um ~{} Zeichen",
                    block.title, needed
                ));
                if candidate_blocks.len() >= 6 {
                    break;
                }
            }
        }
    }

    let candidate_instance_ids = dedupe_keep_order(
        candidate_blocks
            .iter()
            .map(|b| b.instance_id.clone())
            .collect::<Vec<_>>(),
    );
    let goals = dedupe_keep_order(goals);

    let mut reasons: Vec<String> = Vec::new();
    if severely_off_target {
        reasons.push(format!(
            "Delta {:+} Zeichen liegt jenseits der ±50%-Schwelle.",
            delta_chars
        ));
    } else if !within_tolerance {
        reasons.push(format!(
            "Delta {:+} Zeichen außerhalb des ±20%-Korridors.",
            delta_chars
        ));
    }
    if delta_chars > 0 {
        reasons.push("Über Ziel: gewichte auf längste Blöcke.".to_string());
    } else if delta_chars < 0 {
        reasons.push(
            "Unter Ziel: priorisiere Blöcke unter ihrer block_library-Mindestgrenze.".to_string(),
        );
    }
    let reasons = dedupe_keep_order(reasons);

    let payload = json!({
        "target_chars": target_chars,
        "actual_chars": actual_chars,
        "delta_chars": delta_chars,
        "tolerance": 0.20_f64,
        "within_tolerance": within_tolerance,
        "severely_off_target": severely_off_target,
        "status": status,
        "summary": summary,
    });

    Ok(CheckOutcome {
        check_kind: CHECK_KIND.to_string(),
        summary,
        check_applicable: true,
        ready_to_finish,
        needs_revision,
        candidate_instance_ids,
        goals,
        reasons,
        raw_payload: payload,
    }
    .cap())
}
