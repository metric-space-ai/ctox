use anyhow::Result;
use serde::Serialize;

use crate::context_health;
use crate::lcm;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStage {
    Plan,
    Compact,
    Snapshot,
    Invoke,
    Continuity,
    Complete,
}

impl TurnStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Compact => "compact",
            Self::Snapshot => "snapshot",
            Self::Invoke => "invoke",
            Self::Continuity => "continuity",
            Self::Complete => "complete",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatTurnConfig {
    pub max_context_tokens: i64,
    pub turn_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatTurnPlan {
    pub stage: TurnStage,
    pub max_context_tokens: i64,
    pub turn_timeout_secs: u64,
    pub compaction: lcm::CompactionDecision,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ContinuityRefreshStats {
    pub attempted: usize,
    pub updated: usize,
    pub skipped_prompt_build: usize,
    pub skipped_invoke: usize,
    pub skipped_apply: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatTurnOutcome {
    pub stage: TurnStage,
    pub health_status: context_health::ContextHealthStatus,
    pub health_score: u8,
    pub context_items_rendered: usize,
    pub context_items_omitted: usize,
    pub reply_chars: usize,
    pub compaction: Option<lcm::CompactionResult>,
    pub continuity: ContinuityRefreshStats,
    pub compaction_guard: CompactionGuard,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactionGuard {
    pub status: CompactionGuardStatus,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompactionGuardStatus {
    NotNeeded,
    Reduced,
    NoProgress,
    StillHot,
}

#[allow(dead_code)]
impl CompactionGuardStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotNeeded => "not_needed",
            Self::Reduced => "reduced",
            Self::NoProgress => "no_progress",
            Self::StillHot => "still_hot",
        }
    }
}

pub fn build_turn_plan(
    engine: &lcm::LcmEngine,
    conversation_id: i64,
    config: ChatTurnConfig,
) -> Result<ChatTurnPlan> {
    let compaction = engine.evaluate_compaction(conversation_id, config.max_context_tokens)?;
    Ok(ChatTurnPlan {
        stage: TurnStage::Plan,
        max_context_tokens: config.max_context_tokens,
        turn_timeout_secs: config.turn_timeout_secs,
        compaction,
    })
}

pub fn assess_compaction_guard(
    decision: &lcm::CompactionDecision,
    result: Option<&lcm::CompactionResult>,
) -> CompactionGuard {
    match result {
        None => CompactionGuard {
            status: CompactionGuardStatus::NotNeeded,
            summary: format!(
                "compaction guard: skipped because live context is {} / {} tokens",
                decision.current_tokens, decision.threshold
            ),
        },
        Some(result) if result.tokens_after >= result.tokens_before => CompactionGuard {
            status: CompactionGuardStatus::NoProgress,
            summary: format!(
                "compaction guard: no effective reduction ({} -> {} tokens, threshold {})",
                result.tokens_before, result.tokens_after, decision.threshold
            ),
        },
        Some(result) if result.tokens_after > decision.threshold => CompactionGuard {
            status: CompactionGuardStatus::StillHot,
            summary: format!(
                "compaction guard: reduced context but pressure remains high ({} -> {} tokens, threshold {})",
                result.tokens_before, result.tokens_after, decision.threshold
            ),
        },
        Some(result) => CompactionGuard {
            status: CompactionGuardStatus::Reduced,
            summary: format!(
                "compaction guard: reduced context from {} to {} tokens",
                result.tokens_before, result.tokens_after
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::assess_compaction_guard;
    use super::CompactionGuardStatus;
    use crate::lcm;

    #[test]
    fn compaction_guard_marks_missing_result_as_not_needed() {
        let decision = lcm::CompactionDecision {
            should_compact: false,
            reason: "none".to_string(),
            current_tokens: 500,
            threshold: 1000,
        };
        let guard = assess_compaction_guard(&decision, None);
        assert_eq!(guard.status, CompactionGuardStatus::NotNeeded);
    }

    #[test]
    fn compaction_guard_marks_missing_progress() {
        let decision = lcm::CompactionDecision {
            should_compact: true,
            reason: "threshold".to_string(),
            current_tokens: 1400,
            threshold: 1000,
        };
        let result = lcm::CompactionResult {
            action_taken: false,
            tokens_before: 1400,
            tokens_after: 1400,
            created_summary_ids: Vec::new(),
            rounds: 0,
        };
        let guard = assess_compaction_guard(&decision, Some(&result));
        assert_eq!(guard.status, CompactionGuardStatus::NoProgress);
    }

    #[test]
    fn compaction_guard_marks_remaining_pressure() {
        let decision = lcm::CompactionDecision {
            should_compact: true,
            reason: "threshold".to_string(),
            current_tokens: 1600,
            threshold: 1000,
        };
        let result = lcm::CompactionResult {
            action_taken: true,
            tokens_before: 1600,
            tokens_after: 1100,
            created_summary_ids: vec!["sum_1".to_string()],
            rounds: 1,
        };
        let guard = assess_compaction_guard(&decision, Some(&result));
        assert_eq!(guard.status, CompactionGuardStatus::StillHot);
    }
}
