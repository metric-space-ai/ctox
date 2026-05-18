use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::context_health;
use crate::lcm;

const DEFAULT_STRESS_CONVERSATION_ID: i64 = 42;
const DEFAULT_STRESS_ITERATIONS: usize = 24;
const DEFAULT_STRESS_TOKEN_BUDGET: i64 = 160;

#[derive(Debug, Clone, Serialize)]
pub struct ContextStressRoundReport {
    pub round: usize,
    pub compaction_action_taken: bool,
    pub compaction_rounds: usize,
    pub created_summary_ids: usize,
    pub tokens_before: i64,
    pub tokens_after: i64,
    pub overall_score: u8,
    pub status: context_health::ContextHealthStatus,
    pub repair_recommended: bool,
    pub warning_codes: Vec<String>,
    pub forgotten_lines: usize,
    pub degradation_markers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextStressReport {
    pub conversation_id: i64,
    pub iterations_requested: usize,
    pub compactions_completed: usize,
    pub stable: bool,
    pub min_score: u8,
    pub final_score: u8,
    pub final_status: context_health::ContextHealthStatus,
    pub failure_reason: Option<String>,
    pub rounds: Vec<ContextStressRoundReport>,
}

#[derive(Debug, Clone)]
struct DynamicContinuityState {
    focus_status: String,
    focus_next: String,
    narrative_status: String,
    artifact_line: String,
    cause_line: String,
    retry_boundary: String,
}

pub fn run_context_stress(
    db_path: &Path,
    conversation_id: Option<i64>,
    iterations: Option<usize>,
    token_budget: Option<i64>,
) -> Result<ContextStressReport> {
    run_context_stress_with_options(
        db_path,
        conversation_id,
        iterations,
        token_budget,
        &lcm::HeuristicSummarizer,
        false,
    )
}

pub fn run_context_stress_with_options<S: lcm::Summarizer>(
    db_path: &Path,
    conversation_id: Option<i64>,
    iterations: Option<usize>,
    token_budget: Option<i64>,
    summarizer: &S,
    continue_after_degradation: bool,
) -> Result<ContextStressReport> {
    let conversation_id = conversation_id.unwrap_or(DEFAULT_STRESS_CONVERSATION_ID);
    let iterations = iterations.unwrap_or(DEFAULT_STRESS_ITERATIONS).max(1);
    let token_budget = token_budget.unwrap_or(DEFAULT_STRESS_TOKEN_BUDGET).max(64);
    let engine = lcm::LcmEngine::open(db_path, stress_config())?;
    let _ = engine.continuity_init_documents(conversation_id)?;

    let mut continuity_state = seed_stress_continuity(&engine, conversation_id, iterations)?;
    let mut rounds = Vec::with_capacity(iterations);
    let mut min_score = u8::MAX;
    let mut failure_reason = None;
    let mut compactions_completed = 0usize;
    let mut final_status = context_health::ContextHealthStatus::Healthy;
    let mut final_score = 100u8;

    for round in 1..=iterations {
        apply_round_continuity(
            &engine,
            conversation_id,
            iterations,
            round,
            &mut continuity_state,
        )?;
        add_round_messages(&engine, conversation_id, iterations, round)?;

        let compaction = engine.compact(conversation_id, token_budget, summarizer, true)?;
        if compaction.action_taken {
            compactions_completed += 1;
        }

        let snapshot = engine.snapshot(conversation_id)?;
        let continuity = engine.continuity_show_all(conversation_id)?;
        let forgotten = engine.continuity_forgotten(conversation_id, None, None)?;
        let latest_prompt = format!(
            "Continue stress round {round}/{iterations} with the preserved mission contract."
        );
        let health = context_health::assess_with_forgotten(
            &snapshot,
            &continuity,
            &forgotten,
            &latest_prompt,
            token_budget,
        );
        let warning_codes = health
            .warnings
            .iter()
            .map(|warning| warning.code.clone())
            .collect::<Vec<_>>();
        let mut degradation_markers = Vec::new();
        if !compaction.action_taken {
            degradation_markers.push("compaction_skipped".to_string());
        }
        if health.repair_recommended {
            degradation_markers.push("repair_recommended".to_string());
        }
        if health.status == context_health::ContextHealthStatus::Critical {
            degradation_markers.push("critical_status".to_string());
        }
        degradation_markers.extend(warning_codes.iter().cloned());

        min_score = std::cmp::min(min_score, health.overall_score);
        final_status = health.status;
        final_score = health.overall_score;

        rounds.push(ContextStressRoundReport {
            round,
            compaction_action_taken: compaction.action_taken,
            compaction_rounds: compaction.rounds,
            created_summary_ids: compaction.created_summary_ids.len(),
            tokens_before: compaction.tokens_before,
            tokens_after: compaction.tokens_after,
            overall_score: health.overall_score,
            status: health.status,
            repair_recommended: health.repair_recommended,
            warning_codes,
            forgotten_lines: forgotten.len(),
            degradation_markers: degradation_markers.clone(),
        });

        if !degradation_markers.is_empty() {
            if !continue_after_degradation {
                failure_reason = Some(format!(
                    "round {round} produced degradation markers: {}",
                    degradation_markers.join(", ")
                ));
                break;
            }
            if failure_reason.is_none() {
                failure_reason = Some(format!(
                    "first degradation at round {round}: {}",
                    degradation_markers.join(", ")
                ));
            }
        }
    }

    let stable = failure_reason.is_none() && compactions_completed >= iterations;
    Ok(ContextStressReport {
        conversation_id,
        iterations_requested: iterations,
        compactions_completed,
        stable,
        min_score,
        final_score,
        final_status,
        failure_reason,
        rounds,
    })
}

/// Summarizer that simulates a weak LLM returning ~90% of the input,
/// forcing the compaction pipeline to rely on deterministic fallback.
pub struct AdversarialSummarizer;

impl lcm::Summarizer for AdversarialSummarizer {
    fn summarize(
        &self,
        _kind: lcm::SummaryKind,
        _depth: i64,
        lines: &[String],
        _target_tokens: usize,
    ) -> Result<String> {
        let full = lines.join("\n");
        let char_count = full.chars().count();
        Ok(full.chars().take(char_count * 9 / 10).collect())
    }
}

fn stress_config() -> lcm::LcmConfig {
    lcm::LcmConfig {
        context_threshold: 0.25,
        min_compaction_tokens: 0,
        fresh_tail_count: 0,
        leaf_chunk_tokens: 120,
        leaf_target_tokens: 24,
        condensed_target_tokens: 24,
        leaf_min_fanout: 1,
        condensed_min_fanout: 2,
        max_rounds: 6,
    }
}

fn seed_stress_continuity(
    engine: &lcm::LcmEngine,
    conversation_id: i64,
    iterations: usize,
) -> Result<DynamicContinuityState> {
    let focus_status = "active".to_string();
    let focus_next = "process round 1".to_string();
    let narrative_status = format!("stress harness initialized for {iterations} rounds");
    let artifact_line = "stress-report-round-0.json".to_string();
    let cause_line = "Earlier retries without preserved evidence degraded continuity.".to_string();
    let retry_boundary =
        "Do not retry a tactic unless the latest round adds fresh validation evidence.".to_string();

    engine.continuity_apply_diff(
        conversation_id,
        lcm::ContinuityKind::Narrative,
        &format!(
            "## Entries\n+ entry_id: stress_current_status | event_type: initialization | summary: {narrative_status} | consequence: Stability must survive {iterations} forced compactions. | source_class: runtime | source_ref: context_stress | observed_at: round-0\n+ entry_id: stress_failure_memory | event_type: failure_memory | summary: {cause_line} | consequence: {retry_boundary} | source_class: runtime | source_ref: context_stress | observed_at: round-0\n"
        ),
    )?;
    engine.continuity_apply_diff(
        conversation_id,
        lcm::ContinuityKind::Anchors,
        &format!(
            "## Entries\n+ anchor_id: stress_artifact | anchor_type: artifact | statement: {artifact_line} | source_class: runtime | source_ref: context_stress | observed_at: round-0 | confidence: high | supersedes: none | expires_at: none\n+ anchor_id: stress_command | anchor_type: command | statement: ctox context-stress <db-path> {conversation_id} {iterations} {DEFAULT_STRESS_TOKEN_BUDGET} | source_class: runtime | source_ref: context_stress | observed_at: round-0 | confidence: high | supersedes: none | expires_at: none\n+ anchor_id: stress_retry_boundary | anchor_type: retry_boundary | statement: {retry_boundary} | source_class: runtime | source_ref: context_stress | observed_at: round-0 | confidence: high | supersedes: none | expires_at: none\n"
        ),
    )?;
    engine.continuity_apply_diff(
        conversation_id,
        lcm::ContinuityKind::Focus,
        &format!(
            "## Contract\n+ mission: deterministic_context_stress\n+ mission_state: {focus_status}\n+ continuation_mode: continuous\n+ trigger_intensity: hot\n+ slice: round_1\n+ slice_state: ready\n## State\n+ goal: verify repeated compaction does not degrade the loop state\n+ blocker: none\n+ missing_dependency: none\n+ next_slice: {focus_next}\n+ done_gate: complete all rounds with no degradation markers\n+ retry_condition: {retry_boundary}\n+ closure_confidence: medium\n## Sources\n+ source_refs: context_stress\n+ updated_at: round-0\n"
        ),
    )?;

    Ok(DynamicContinuityState {
        focus_status,
        focus_next,
        narrative_status,
        artifact_line,
        cause_line,
        retry_boundary,
    })
}

fn apply_round_continuity(
    engine: &lcm::LcmEngine,
    conversation_id: i64,
    iterations: usize,
    round: usize,
    state: &mut DynamicContinuityState,
) -> Result<()> {
    let next_focus_status = "active".to_string();
    let next_focus_next = if round < iterations {
        format!("process round {}", round + 1)
    } else {
        "finalize stress report".to_string()
    };
    let next_narrative_status = format!("round {round}/{iterations} completed without drift");
    let next_artifact_line = format!("stress-report-round-{round}.json");
    let next_cause_line =
        format!("Round {round} preserved retry evidence before any future replay attempt.");
    let next_retry_boundary =
        format!("Retry only after fresh validation evidence from round {round} is present.");

    apply_line_swap(
        engine,
        conversation_id,
        lcm::ContinuityKind::Focus,
        "Contract",
        Some(&format!("mission_state: {}", state.focus_status)),
        Some(&format!("mission_state: {}", next_focus_status)),
    )?;
    apply_line_swap(
        engine,
        conversation_id,
        lcm::ContinuityKind::Focus,
        "Contract",
        Some(&format!(
            "slice: {}",
            if round > 1 {
                format!("round_{}", round - 1)
            } else {
                "round_1".to_string()
            }
        )),
        Some(&format!("slice: round_{round}")),
    )?;
    apply_line_swap(
        engine,
        conversation_id,
        lcm::ContinuityKind::Focus,
        "State",
        Some(&format!("next_slice: {}", state.focus_next)),
        Some(&format!("next_slice: {}", next_focus_next)),
    )?;
    apply_line_swap(
        engine,
        conversation_id,
        lcm::ContinuityKind::Narrative,
        "Entries",
        Some(&format!(
            "entry_id: stress_current_status | event_type: initialization | summary: {} | consequence: Stability must survive {} forced compactions. | source_class: runtime | source_ref: context_stress | observed_at: round-{}",
            state.narrative_status,
            iterations,
            round.saturating_sub(1)
        )),
        Some(&format!(
            "entry_id: stress_current_status | event_type: compaction_round | summary: {next_narrative_status} | consequence: retrievability preserved after round {round}. | source_class: runtime | source_ref: context_stress | observed_at: round-{round}"
        )),
    )?;
    apply_line_swap(
        engine,
        conversation_id,
        lcm::ContinuityKind::Narrative,
        "Entries",
        Some(&format!(
            "entry_id: stress_failure_memory | event_type: failure_memory | summary: {} | consequence: {} | source_class: runtime | source_ref: context_stress | observed_at: round-{}",
            state.cause_line,
            state.retry_boundary,
            round.saturating_sub(1)
        )),
        Some(&format!(
            "entry_id: stress_failure_memory | event_type: failure_memory | summary: {next_cause_line} | consequence: {next_retry_boundary} | source_class: runtime | source_ref: context_stress | observed_at: round-{round}"
        )),
    )?;
    apply_line_swap(
        engine,
        conversation_id,
        lcm::ContinuityKind::Anchors,
        "Entries",
        Some(&format!(
            "anchor_id: stress_artifact | anchor_type: artifact | statement: {} | source_class: runtime | source_ref: context_stress | observed_at: round-{} | confidence: high | supersedes: none | expires_at: none",
            state.artifact_line,
            round.saturating_sub(1)
        )),
        Some(&format!(
            "anchor_id: stress_artifact | anchor_type: artifact | statement: {next_artifact_line} | source_class: runtime | source_ref: context_stress | observed_at: round-{round} | confidence: high | supersedes: none | expires_at: none"
        )),
    )?;
    apply_line_swap(
        engine,
        conversation_id,
        lcm::ContinuityKind::Anchors,
        "Entries",
        Some(&format!(
            "anchor_id: stress_retry_boundary | anchor_type: retry_boundary | statement: {} | source_class: runtime | source_ref: context_stress | observed_at: round-{} | confidence: high | supersedes: none | expires_at: none",
            state.retry_boundary,
            round.saturating_sub(1)
        )),
        Some(&format!(
            "anchor_id: stress_retry_boundary | anchor_type: retry_boundary | statement: {next_retry_boundary} | source_class: runtime | source_ref: context_stress | observed_at: round-{round} | confidence: high | supersedes: none | expires_at: none"
        )),
    )?;

    state.focus_status = next_focus_status;
    state.focus_next = next_focus_next;
    state.narrative_status = next_narrative_status;
    state.artifact_line = next_artifact_line;
    state.cause_line = next_cause_line;
    state.retry_boundary = next_retry_boundary;
    Ok(())
}

fn apply_line_swap(
    engine: &lcm::LcmEngine,
    conversation_id: i64,
    kind: lcm::ContinuityKind,
    section: &str,
    old_line: Option<&str>,
    new_line: Option<&str>,
) -> Result<()> {
    let mut lines = vec![format!("## {section}")];
    if let Some(old_line) = old_line.filter(|line| !line.trim().is_empty()) {
        lines.push(format!("- {old_line}"));
    }
    if let Some(new_line) = new_line.filter(|line| !line.trim().is_empty()) {
        lines.push(format!("+ {new_line}"));
    }
    engine.continuity_apply_diff(conversation_id, kind, &format!("{}\n", lines.join("\n")))?;
    Ok(())
}

fn add_round_messages(
    engine: &lcm::LcmEngine,
    conversation_id: i64,
    iterations: usize,
    round: usize,
) -> Result<()> {
    let lines = [
        (
            "user",
            format!(
                "Round {round}/{iterations} request: continue the delivery program with the new validation bundle, release notes, rollback checkpoints, and owner acceptance evidence. Preserve the mission contract and keep the done gate explicit."
            ),
        ),
        (
            "assistant",
            format!(
                "Round {round} assessment: the latest evidence bundle is fresh, the previous retry boundary is satisfied, and the loop should preserve the exact blocker-free state before any new compaction pass."
            ),
        ),
        (
            "user",
            format!(
                "Round {round} detail: artifact group {round} contains deployment transcript fragments, telemetry snapshots, and a compact acceptance checklist. Keep the constraints sticky and avoid inventing stale blockers."
            ),
        ),
        (
            "assistant",
            format!(
                "Round {round} continuity note: update the durable record with the current status, next action, and retry condition so the next slice can resume without replaying discarded assumptions."
            ),
        ),
        (
            "user",
            format!(
                "Round {round} verification task: compress earlier details if needed, but retain enough evidence to prove that the loop still knows what success, failure, and the next safe action look like."
            ),
        ),
        (
            "assistant",
            format!(
                "Round {round} verification result: the context remains aligned to the main objective, the constraints are explicit, and the next compaction should preserve retrievability rather than flattening the mission into generic summaries."
            ),
        ),
    ];
    for (role, content) in lines {
        engine.add_message(conversation_id, role, &content)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{run_context_stress, run_context_stress_with_options, AdversarialSummarizer};
    use anyhow::Result;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("ctox-context-stress-{unique}.db"))
    }

    #[test]
    fn stress_harness_survives_twenty_forced_compactions() -> Result<()> {
        let db_path = temp_db();
        let report = run_context_stress(&db_path, Some(77), Some(20), Some(160))?;
        assert!(report.stable, "{report:#?}");
        assert_eq!(report.compactions_completed, 20);
        assert!(report.min_score >= 70, "{report:#?}");
        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn stress_harness_survives_adversarial_summarizer() -> Result<()> {
        let db_path = temp_db();
        let report = run_context_stress_with_options(
            &db_path,
            Some(88),
            Some(20),
            Some(160),
            &AdversarialSummarizer,
            false,
        )?;
        // The adversarial summarizer returns ~90% of input, which triggers
        // the deterministic fallback. Compaction must still complete without
        // crashing or panicking.
        assert!(
            report.compactions_completed > 0,
            "adversarial: expected at least one compaction, got {report:#?}"
        );
        assert!(
            report.rounds.len() > 0,
            "adversarial: expected at least one round, got {report:#?}"
        );
        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn stress_harness_recovery_after_degradation() -> Result<()> {
        let db_path = temp_db();
        let report = run_context_stress_with_options(
            &db_path,
            Some(99),
            Some(20),
            Some(160),
            &AdversarialSummarizer,
            true, // continue after degradation
        )?;
        // In recovery mode, the harness must complete all iterations without
        // panicking, even if degradation markers appear. The key assertion is
        // that it ran through every round.
        assert_eq!(
            report.rounds.len(),
            20,
            "recovery: expected all 20 rounds to complete, got {report:#?}"
        );
        let _ = std::fs::remove_file(db_path);
        Ok(())
    }
}
