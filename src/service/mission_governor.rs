use serde::Serialize;

use crate::context_health;
use crate::lcm;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MissionLoopGovernorDecision {
    pub should_enqueue_repair: bool,
    pub reason: String,
    pub repeated_blocker: bool,
    pub blocker_summary: Option<String>,
    pub repair_title: Option<String>,
    pub repair_prompt: Option<String>,
}

pub fn evaluate_loop_governor(
    goal: &str,
    mission: Option<&lcm::MissionStateRecord>,
    health: Option<&context_health::ContextHealthSnapshot>,
    latest_result: &str,
    latest_runtime_error: Option<&str>,
) -> MissionLoopGovernorDecision {
    let latest_blocker = latest_runtime_error
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| extract_blocker(latest_result));
    let repeated_blocker = mission
        .and_then(|record| {
            latest_blocker
                .as_deref()
                .map(|blocker| blockers_equivalent(&record.blocker, blocker))
        })
        .unwrap_or(false);
    let loop_warning = health.map(has_loop_warning).unwrap_or(false);
    let progress_visible = progress_signal_visible(latest_result);

    if !(repeated_blocker && loop_warning && !progress_visible) {
        return MissionLoopGovernorDecision {
            should_enqueue_repair: false,
            reason: if repeated_blocker {
                "repeated blocker observed but loop pressure or progress signal did not cross the repair threshold".to_string()
            } else {
                "no repeated blocker pattern detected".to_string()
            },
            repeated_blocker,
            blocker_summary: latest_blocker,
            repair_title: None,
            repair_prompt: None,
        };
    }

    let blocker = latest_blocker.clone().unwrap_or_else(|| {
        "The same blocker pattern keeps resurfacing without new evidence.".to_string()
    });
    let title = format!("Repair mission loop for {}", clip_text(goal.trim(), 48));
    let prompt = render_loop_repair_prompt(goal, mission, health, &blocker);
    MissionLoopGovernorDecision {
        should_enqueue_repair: true,
        reason:
            "repeated blocker detected without fresh progress; force a repair/replan slice instead of another normal retry"
                .to_string(),
        repeated_blocker,
        blocker_summary: latest_blocker,
        repair_title: Some(title),
        repair_prompt: Some(prompt),
    }
}

fn render_loop_repair_prompt(
    goal: &str,
    mission: Option<&lcm::MissionStateRecord>,
    health: Option<&context_health::ContextHealthSnapshot>,
    blocker: &str,
) -> String {
    let mission_status = mission
        .map(|record| fallback_text(&record.mission_status, "active"))
        .unwrap_or("unknown");
    let next_slice = mission
        .map(|record| fallback_text(&record.next_slice, "reconstruct the next bounded slice"))
        .unwrap_or("reconstruct the next bounded slice");
    let done_gate = mission
        .map(|record| fallback_text(&record.done_gate, "close only on current verified evidence"))
        .unwrap_or("close only on current verified evidence");
    let warning_lines = health
        .map(|snapshot| {
            snapshot
                .warnings
                .iter()
                .take(4)
                .map(|warning| format!("- {}: {}", warning.code, warning.summary))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut lines = vec![
        "Before trying again, fix the stuck mission state.".to_string(),
        String::new(),
        "Goal:".to_string(),
        goal.trim().to_string(),
        String::new(),
        format!("Current mission state: {mission_status}"),
        format!("Current blocker: {}", blocker.trim()),
        format!("Previous next step: {}", next_slice.trim()),
        format!("Finish only when: {}", done_gate.trim()),
        String::new(),
        "Important rules:".to_string(),
        "- Do not retry the same tactic unless you first produce new evidence that changes the blocker.".to_string(),
        "- Restate the real current task in simple language: current status, blocker, next step, and finish rule.".to_string(),
        "- Record what failed, why it failed, and what exact condition would justify a retry.".to_string(),
        "- If the blocker is external, keep the mission blocked and state the exact missing input or approval.".to_string(),
        "- If the blocker is internal, choose one materially different next step or replan the work into smaller steps.".to_string(),
    ];
    if !warning_lines.is_empty() {
        lines.push(String::new());
        lines.push("Current loop-pressure warnings:".to_string());
        lines.extend(warning_lines);
    }
    lines.push(String::new());
    lines.push("Return a real runtime outcome, not another vague continuation note.".to_string());
    lines.join("\n")
}

fn extract_blocker(result: &str) -> Option<String> {
    for line in result.lines() {
        let trimmed = line.trim();
        let lowered = normalize_token(trimmed);
        if lowered.starts_with("blocked ") || lowered == "blocked" {
            return Some(trimmed.to_string());
        }
        if let Some((prefix, value)) = trimmed.split_once(':') {
            let key = normalize_token(prefix);
            if key == "blocked" || key == "blocker" || key == "current blocker" {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

fn blockers_equivalent(left: &str, right: &str) -> bool {
    let left = normalize_token(left);
    let right = normalize_token(right);
    !left.is_empty()
        && !right.is_empty()
        && (left == right || left.contains(&right) || right.contains(&left))
}

fn has_loop_warning(health: &context_health::ContextHealthSnapshot) -> bool {
    health.warnings.iter().any(|warning| {
        matches!(
            warning.code.as_str(),
            "blocked_status_loop"
                | "repair_prompt_churn"
                | "failure_memory_missing"
                | "recent_user_turn_repeated"
                | "mission_contract_thin"
        )
    })
}

fn progress_signal_visible(result: &str) -> bool {
    let lowered = result.to_ascii_lowercase();
    [
        "completed",
        "done",
        "implemented",
        "fixed",
        "created",
        "updated",
        "tests passed",
        "verified",
        "green",
        "applied",
        "persisted",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn normalize_token(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn clip_text(value: &str, max_chars: usize) -> String {
    let mut iter = value.chars();
    let clipped = iter.by_ref().take(max_chars).collect::<String>();
    if iter.next().is_some() {
        format!("{clipped}…")
    } else {
        clipped
    }
}

fn fallback_text<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::evaluate_loop_governor;
    use crate::context_health;
    use crate::lcm;

    fn mission() -> lcm::MissionStateRecord {
        lcm::MissionStateRecord {
            conversation_id: 1,
            mission: "Repair deployment loop".to_string(),
            mission_status: "active".to_string(),
            continuation_mode: "continuous".to_string(),
            trigger_intensity: "hot".to_string(),
            blocker: "deploy still failing because SECRET_TOKEN is missing".to_string(),
            next_slice: "verify the real blocker and pick a smaller recovery slice".to_string(),
            done_gate: "deployment works with verified credentials".to_string(),
            closure_confidence: "low".to_string(),
            is_open: true,
            allow_idle: false,
            focus_head_commit_id: "focus".to_string(),
            last_synced_at: "2026-04-01T00:00:00Z".to_string(),
            watcher_last_triggered_at: None,
            watcher_trigger_count: 0,
            agent_failure_count: 0,
            deferred_reason: None,
            rewrite_failure_count: 0,
        }
    }

    fn health() -> context_health::ContextHealthSnapshot {
        context_health::ContextHealthSnapshot {
            conversation_id: 1,
            overall_score: 22,
            status: context_health::ContextHealthStatus::Critical,
            summary: "loop pressure high".to_string(),
            repair_recommended: true,
            dimensions: Vec::new(),
            warnings: vec![context_health::ContextHealthWarning {
                code: "blocked_status_loop".to_string(),
                severity: context_health::WarningSeverity::Critical,
                summary: "Recent assistant history shows repeated blocked-status notes."
                    .to_string(),
                evidence: "3 recent blocked notes".to_string(),
                recommended_action: "Record failed tactic and retry condition".to_string(),
            }],
        }
    }

    #[test]
    fn governor_enqueues_repair_for_repeated_blocker_without_progress() {
        let decision = evaluate_loop_governor(
            "Repair deployment loop",
            Some(&mission()),
            Some(&health()),
            "Status: `blocked`\n\nBlocker: deploy still failing because SECRET_TOKEN is missing",
            None,
        );
        assert!(decision.should_enqueue_repair);
        assert!(decision.repeated_blocker);
        assert!(decision
            .repair_prompt
            .as_deref()
            .unwrap_or_default()
            .contains("Do not retry the same tactic"));
    }

    #[test]
    fn governor_stays_quiet_when_progress_signal_exists() {
        let decision = evaluate_loop_governor(
            "Repair deployment loop",
            Some(&mission()),
            Some(&health()),
            "Implemented a smaller recovery path and updated the config. Tests passed.",
            None,
        );
        assert!(!decision.should_enqueue_repair);
    }
}
