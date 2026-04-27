use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::lcm;

const NARRATIVE_TEMPLATE: &str = "# Narrative\n\n## Entries\n- entry_id:\n  event_type:\n  summary:\n  consequence:\n  source_class:\n  source_ref:\n  observed_at:\n";
const ANCHORS_TEMPLATE: &str = "# Anchors\n\n## Entries\n- anchor_id:\n  anchor_type:\n  statement:\n  source_class:\n  source_ref:\n  observed_at:\n  confidence:\n  supersedes:\n  expires_at:\n";
const FOCUS_TEMPLATE: &str = "# Focus\n\n## Contract\nmission:\nmission_state:\ncontinuation_mode:\ntrigger_intensity:\nslice:\nslice_state:\n\n## State\ngoal:\nblocker:\nmissing_dependency:\nnext_slice:\ndone_gate:\nretry_condition:\nclosure_confidence:\n\n## Sources\nsource_refs:\n- none\nupdated_at:\n";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextHealthStatus {
    Healthy,
    Watch,
    Degraded,
    Critical,
}

impl ContextHealthStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Watch => "watch",
            Self::Degraded => "degraded",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WarningSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextHealthDimension {
    pub name: String,
    pub score: u8,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextHealthWarning {
    pub code: String,
    pub severity: WarningSeverity,
    pub summary: String,
    pub evidence: String,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextHealthSnapshot {
    pub conversation_id: i64,
    pub overall_score: u8,
    pub status: ContextHealthStatus,
    pub summary: String,
    pub repair_recommended: bool,
    pub dimensions: Vec<ContextHealthDimension>,
    pub warnings: Vec<ContextHealthWarning>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextRepairGovernorDecision {
    pub should_enqueue_repair: bool,
    pub reason: String,
}

pub fn assess_for_conversation(
    db_path: &Path,
    conversation_id: i64,
    token_budget: i64,
    latest_user_prompt: Option<&str>,
) -> Result<ContextHealthSnapshot> {
    let engine = lcm::LcmEngine::open(db_path, lcm::LcmConfig::default())?;
    let snapshot = engine.snapshot(conversation_id)?;
    let continuity = engine.continuity_show_all(conversation_id)?;
    let forgotten_entries = engine.continuity_forgotten(conversation_id, None, None)?;
    Ok(assess_with_forgotten(
        &snapshot,
        &continuity,
        &forgotten_entries,
        latest_user_prompt.unwrap_or(""),
        token_budget,
    ))
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn assess(
    snapshot: &lcm::LcmSnapshot,
    continuity: &lcm::ContinuityShowAll,
    latest_user_prompt: &str,
    token_budget: i64,
) -> ContextHealthSnapshot {
    assess_with_forgotten(snapshot, continuity, &[], latest_user_prompt, token_budget)
}

pub fn assess_with_forgotten(
    snapshot: &lcm::LcmSnapshot,
    continuity: &lcm::ContinuityShowAll,
    forgotten_entries: &[lcm::ContinuityForgottenEntry],
    latest_user_prompt: &str,
    token_budget: i64,
) -> ContextHealthSnapshot {
    let context_tokens = snapshot
        .context_items
        .iter()
        .map(|item| item.token_count.max(0))
        .sum::<i64>();
    let pressure_score = score_context_pressure(context_tokens, token_budget);
    let continuity_score = score_continuity_coverage(continuity);
    let repetition = repeated_recent_user_turns(snapshot, latest_user_prompt);
    let repetition_score = score_repetition_risk(repetition);
    let blocked_count = recent_blocked_status_count(snapshot);
    let blocked_score = score_blocked_loop(blocked_count);
    let repair_prompt_count = recent_internal_repair_prompt_count(snapshot);
    let repair_score = score_repair_churn(repair_prompt_count);
    let mission_contract = inspect_mission_contract(continuity);
    let mission_contract_score = score_mission_contract(&mission_contract);
    let negative_memory = inspect_negative_memory(continuity, forgotten_entries);
    let negative_memory_score = score_negative_memory(
        &negative_memory,
        repetition,
        blocked_count,
        repair_prompt_count,
    );

    let dimensions = vec![
        ContextHealthDimension {
            name: "context_pressure".to_string(),
            score: pressure_score,
            summary: format!(
                "Live context uses about {} tokens against a {} token budget.",
                context_tokens,
                token_budget.max(1)
            ),
        },
        ContextHealthDimension {
            name: "continuity_coverage".to_string(),
            score: continuity_score,
            summary: continuity_coverage_summary(continuity),
        },
        ContextHealthDimension {
            name: "mission_contract".to_string(),
            score: mission_contract_score,
            summary: mission_contract.summary(),
        },
        ContextHealthDimension {
            name: "negative_memory".to_string(),
            score: negative_memory_score,
            summary: negative_memory.summary(),
        },
        ContextHealthDimension {
            name: "repetition_risk".to_string(),
            score: repetition_score,
            summary: if repetition == 0 {
                "The latest user turn does not look like a recent duplicate.".to_string()
            } else {
                format!(
                    "The latest user turn overlaps with {} recent user turn(s).",
                    repetition
                )
            },
        },
        ContextHealthDimension {
            name: "blocked_loop_risk".to_string(),
            score: blocked_score,
            summary: if blocked_count == 0 {
                "Recent assistant history does not show repeated blocked-status notes.".to_string()
            } else {
                format!("{blocked_count} recent assistant status note(s) look blocked or stalled.")
            },
        },
        ContextHealthDimension {
            name: "repair_churn".to_string(),
            score: repair_score,
            summary: if repair_prompt_count == 0 {
                "Recent context is not dominated by internal repair or continuation prompts."
                    .to_string()
            } else {
                format!(
                    "{repair_prompt_count} recent internal prompt(s) look like repair or continuation churn."
                )
            },
        },
    ];

    let weighted_score = weighted_average(&[
        (pressure_score, 15_u32),
        (continuity_score, 15_u32),
        (mission_contract_score, 20_u32),
        (negative_memory_score, 15_u32),
        (repetition_score, 15_u32),
        (blocked_score, 10_u32),
        (repair_score, 10_u32),
    ]);
    let warnings = build_warnings(
        snapshot,
        snapshot.conversation_id,
        continuity,
        forgotten_entries,
        latest_user_prompt,
        token_budget,
        context_tokens,
        &mission_contract,
        &negative_memory,
        repetition,
        blocked_count,
        repair_prompt_count,
    );
    let effective_score = cap_score_for_warnings(weighted_score, &warnings);
    let status = merge_status_with_warnings(status_for_score(effective_score), &warnings);
    let repair_recommended = effective_score < 60
        || warnings
            .iter()
            .any(|warning| warning.severity == WarningSeverity::Critical);
    let summary = summarize_dimensions(&dimensions, &warnings, status, effective_score);

    ContextHealthSnapshot {
        conversation_id: snapshot.conversation_id,
        overall_score: effective_score,
        status,
        summary,
        repair_recommended,
        dimensions,
        warnings,
    }
}

pub fn render_prompt_block(health: &ContextHealthSnapshot) -> String {
    let mut lines = vec!["Context health:".to_string()];
    lines.push(format!("status: {}", health.status.as_str()));
    lines.push(format!(
        "repair_recommended: {}",
        if health.repair_recommended {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "preempt_current_slice: {}",
        if context_health_preempts_current_slice(health) {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "how_to_use: {}",
        if context_health_preempts_current_slice(health) {
            "pause the current task and repair the task contract before continuing"
        } else {
            "keep the current task primary; use this block only as guidance"
        }
    ));
    if health.warnings.is_empty() {
        lines.push("warnings: []".to_string());
    } else {
        lines.push("warnings:".to_string());
        for warning in health.warnings.iter().take(3) {
            lines.push(format!("- warning: {}", warning.summary));
            lines.push(format!(
                "  severity: {}",
                warning_severity_label(warning.severity)
            ));
            lines.push(format!(
                "  what_to_do: {}",
                health_warning_action_label(&warning.code)
            ));
            lines.push(format!("  code: {}", warning.code));
        }
    }
    lines.join("\n")
}

fn context_health_preempts_current_slice(health: &ContextHealthSnapshot) -> bool {
    health.repair_recommended
        && health.warnings.iter().any(|warning| {
            matches!(
                warning.code.as_str(),
                "thin_mission_contract" | "missing_failure_memory"
            ) && warning.severity == WarningSeverity::Critical
        })
}

fn warning_severity_label(severity: WarningSeverity) -> &'static str {
    match severity {
        WarningSeverity::Info => "info",
        WarningSeverity::Warning => "warning",
        WarningSeverity::Critical => "critical",
    }
}

fn health_warning_action_label(code: &str) -> &'static str {
    match code {
        "mission_switch_pending" => "pick one main task and rebuild the current-task summary",
        "mission_contamination" => {
            "drop stale mission detail that no longer matches the current task"
        }
        "thin_mission_contract" => {
            "rewrite the current task in simple terms: what to do now, blocker, next step, completion rule"
        }
        "missing_failure_memory" => "record what failed and what evidence would justify retrying",
        "context_pressure" => {
            "load less detail and rely on durable state until the current task is complete"
        }
        _ => "inspect the current task state before changing course",
    }
}

pub fn evaluate_repair_governor(
    health: &ContextHealthSnapshot,
    source_label: &str,
    current_goal: &str,
    existing_open_repair_task: bool,
    open_repair_task_count: usize,
) -> ContextRepairGovernorDecision {
    if !health.repair_recommended {
        return ContextRepairGovernorDecision {
            should_enqueue_repair: false,
            reason: "context health is still within the no-repair band".to_string(),
        };
    }
    if existing_open_repair_task {
        return ContextRepairGovernorDecision {
            should_enqueue_repair: false,
            reason: "an open context-health repair task already exists".to_string(),
        };
    }
    if open_repair_task_count >= 2 {
        return ContextRepairGovernorDecision {
            should_enqueue_repair: false,
            reason: "context-health repair is already consuming multiple open work slots"
                .to_string(),
        };
    }
    if is_context_repair_source(source_label) || looks_like_context_repair_goal(current_goal) {
        return ContextRepairGovernorDecision {
            should_enqueue_repair: false,
            reason: "the current work already is a context-health repair slice".to_string(),
        };
    }
    ContextRepairGovernorDecision {
        should_enqueue_repair: true,
        reason:
            "context-health warnings crossed the repair threshold without an active repair slice"
                .to_string(),
    }
}

pub fn render_repair_task_prompt(health: &ContextHealthSnapshot) -> String {
    let mut lines = vec![
        "Repair the context only enough to make the next safe step clear. Do not let repair become the main mission."
            .to_string(),
        String::new(),
        format!(
            "Current context health score: {} ({})",
            health.overall_score,
            health.status.as_str()
        ),
        health.summary.clone(),
        String::new(),
        "Warnings:".to_string(),
    ];
    for warning in &health.warnings {
        lines.push(format!(
            "- {}: {} | evidence: {} | recommended action: {}",
            warning.code, warning.summary, warning.evidence, warning.recommended_action
        ));
    }
    lines.push(String::new());
    lines.push("What to fix:".to_string());
    lines.push("- Write down only the minimum state needed for the next safe step: current status, blocker, next step, finish rule, and lasting constraints.".to_string());
    lines.push(
        "- Record what failed, why it failed, and what new evidence would justify trying again."
            .to_string(),
    );
    lines.push("- Do not create another context-health repair task.".to_string());
    lines.push("- If one repair pass does not clearly improve clarity or reduce repetition risk, stop and either replan, escalate, or ask for the exact missing input.".to_string());
    lines.push(
        "- In the final reply, separate real mission progress from context cleanup.".to_string(),
    );
    lines.join("\n")
}

fn weighted_average(values: &[(u8, u32)]) -> u8 {
    let total_weight = values.iter().map(|(_, weight)| *weight).sum::<u32>().max(1);
    let total = values
        .iter()
        .map(|(score, weight)| u32::from(*score) * *weight)
        .sum::<u32>();
    ((total + (total_weight / 2)) / total_weight) as u8
}

fn cap_score_for_warnings(score: u8, warnings: &[ContextHealthWarning]) -> u8 {
    let critical_count = warnings
        .iter()
        .filter(|warning| warning.severity == WarningSeverity::Critical)
        .count();
    if critical_count >= 2 {
        score.min(45)
    } else if critical_count == 1 {
        score.min(55)
    } else if warnings
        .iter()
        .any(|warning| warning.severity == WarningSeverity::Warning)
    {
        score.min(74)
    } else {
        score
    }
}

fn status_for_score(score: u8) -> ContextHealthStatus {
    match score {
        80..=100 => ContextHealthStatus::Healthy,
        60..=79 => ContextHealthStatus::Watch,
        40..=59 => ContextHealthStatus::Degraded,
        _ => ContextHealthStatus::Critical,
    }
}

fn merge_status_with_warnings(
    base_status: ContextHealthStatus,
    warnings: &[ContextHealthWarning],
) -> ContextHealthStatus {
    let strongest_warning_status = warnings
        .iter()
        .filter_map(|warning| match warning.severity {
            WarningSeverity::Info => None,
            WarningSeverity::Warning => Some(ContextHealthStatus::Watch),
            WarningSeverity::Critical => Some(ContextHealthStatus::Critical),
        })
        .max_by_key(|status| status_rank(*status));
    strongest_warning_status
        .filter(|warning_status| status_rank(*warning_status) > status_rank(base_status))
        .unwrap_or(base_status)
}

fn status_rank(status: ContextHealthStatus) -> u8 {
    match status {
        ContextHealthStatus::Healthy => 0,
        ContextHealthStatus::Watch => 1,
        ContextHealthStatus::Degraded => 2,
        ContextHealthStatus::Critical => 3,
    }
}

fn score_context_pressure(tokens: i64, budget: i64) -> u8 {
    let budget = budget.max(1) as f64;
    let ratio = (tokens.max(0) as f64) / budget;
    if ratio <= 0.45 {
        100
    } else if ratio <= 0.65 {
        85
    } else if ratio <= 0.80 {
        65
    } else if ratio <= 0.92 {
        40
    } else {
        20
    }
}

fn score_continuity_coverage(continuity: &lcm::ContinuityShowAll) -> u8 {
    let scores = [
        score_continuity_document(&continuity.narrative.content, NARRATIVE_TEMPLATE),
        score_continuity_document(&continuity.anchors.content, ANCHORS_TEMPLATE),
        score_continuity_document(&continuity.focus.content, FOCUS_TEMPLATE),
    ];
    ((u16::from(scores[0]) + u16::from(scores[1]) + u16::from(scores[2]) + 1) / 3) as u8
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MissionContractCoverage {
    status_lines: usize,
    blocker_lines: usize,
    next_lines: usize,
    done_gate_lines: usize,
    invariant_lines: usize,
}

impl MissionContractCoverage {
    fn control_points(&self) -> usize {
        [
            self.status_lines > 0,
            self.blocker_lines > 0,
            self.next_lines > 0,
            self.done_gate_lines > 0,
            self.invariant_lines > 0,
        ]
        .into_iter()
        .filter(|value| *value)
        .count()
    }

    fn summary(&self) -> String {
        format!(
            "The mission state currently captures {}/5 key points: status={}, blocker={}, next step={}, finish rule={}, constraints={}.",
            self.control_points(),
            yes_no(self.status_lines > 0),
            yes_no(self.blocker_lines > 0),
            yes_no(self.next_lines > 0),
            yes_no(self.done_gate_lines > 0),
            yes_no(self.invariant_lines > 0),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NegativeMemoryCoverage {
    cause_lines: usize,
    turning_point_lines: usize,
    invariant_lines: usize,
    forgotten_lines: usize,
}

impl NegativeMemoryCoverage {
    fn total_signals(&self) -> usize {
        self.cause_lines + self.turning_point_lines + self.invariant_lines + self.forgotten_lines
    }

    fn summary(&self) -> String {
        format!(
            "Negative memory exposes {} signal(s): cause={}, turning_points={}, constraints={}, forgotten={}.",
            self.total_signals(),
            self.cause_lines,
            self.turning_point_lines,
            self.invariant_lines,
            self.forgotten_lines,
        )
    }
}

fn score_continuity_document(content: &str, template: &str) -> u8 {
    if normalize_text(content) == normalize_text(template) {
        return 20;
    }
    let meaningful_lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .count();
    match meaningful_lines {
        0 => 20,
        1 => 45,
        2 => 65,
        3 => 80,
        _ => 100,
    }
}

fn score_mission_contract(contract: &MissionContractCoverage) -> u8 {
    match contract.control_points() {
        0 => 20,
        1 => 35,
        2 => 55,
        3 => 75,
        4 => 90,
        _ => 100,
    }
}

fn score_negative_memory(
    memory: &NegativeMemoryCoverage,
    repetition_count: usize,
    blocked_count: usize,
    repair_prompt_count: usize,
) -> u8 {
    let loop_pressure = repetition_count + blocked_count + repair_prompt_count;
    if loop_pressure == 0 {
        return match memory.total_signals() {
            0 => 85,
            1 => 92,
            _ => 100,
        };
    }
    match memory.total_signals() {
        0 => 20,
        1 => 55,
        2 => 75,
        _ => 100,
    }
}

fn score_repetition_risk(repetition_count: usize) -> u8 {
    match repetition_count {
        0 => 100,
        1 => 55,
        _ => 20,
    }
}

fn score_blocked_loop(blocked_count: usize) -> u8 {
    match blocked_count {
        0 => 100,
        1 => 75,
        2 => 45,
        _ => 15,
    }
}

fn score_repair_churn(repair_prompt_count: usize) -> u8 {
    match repair_prompt_count {
        0 => 100,
        1 => 80,
        2 => 50,
        _ => 20,
    }
}

fn build_warnings(
    snapshot: &lcm::LcmSnapshot,
    conversation_id: i64,
    continuity: &lcm::ContinuityShowAll,
    forgotten_entries: &[lcm::ContinuityForgottenEntry],
    latest_user_prompt: &str,
    token_budget: i64,
    context_tokens: i64,
    mission_contract: &MissionContractCoverage,
    negative_memory: &NegativeMemoryCoverage,
    repetition: usize,
    blocked_count: usize,
    repair_prompt_count: usize,
) -> Vec<ContextHealthWarning> {
    let mut warnings = Vec::new();
    if let Some(workspace_root) = detect_current_workspace_root(snapshot, latest_user_prompt) {
        let continuity_text = format!(
            "{}\n{}\n{}",
            continuity.focus.content, continuity.anchors.content, continuity.narrative.content
        );
        if !continuity_text.contains(&workspace_root) {
            warnings.push(ContextHealthWarning {
                code: "mission_switch_pending".to_string(),
                severity: WarningSeverity::Critical,
                summary: "The live mission appears to have changed, but continuity still points at an older mission.".to_string(),
                evidence: format!(
                    "latest mission workspace is {}, but focus/anchors/narrative do not mention it",
                    workspace_root
                ),
                recommended_action: "Rebuild focus, anchors, and narrative from the active mission before trusting older continuity.".to_string(),
            });
        }
        if continuity_text.contains("rust-blog-feed")
            || continuity_text.contains("planet-python-feed")
        {
            warnings.push(ContextHealthWarning {
                code: "mission_contamination".to_string(),
                severity: WarningSeverity::Critical,
                summary: "Continuity still carries a previous mission into the current mission window.".to_string(),
                evidence: "scraper-specific continuity is still present while the active mission targets another workspace".to_string(),
                recommended_action: "Demote or discard stale mission-specific continuity and promote only the current mission contract.".to_string(),
            });
        }
    }
    let ratio = (context_tokens.max(0) as f64) / (token_budget.max(1) as f64);
    if ratio > 0.80 {
        warnings.push(ContextHealthWarning {
            code: "context_window_pressure".to_string(),
            severity: if ratio > 0.92 {
                WarningSeverity::Critical
            } else {
                WarningSeverity::Warning
            },
            summary: "The live context window is under heavy pressure.".to_string(),
            evidence: format!(
                "conversation {} is using about {} / {} tokens of active context",
                conversation_id, context_tokens, token_budget
            ),
            recommended_action:
                "Compact or refresh continuity before repeating verbose recovery history."
                    .to_string(),
        });
    }
    if normalize_text(&continuity.focus.content) == normalize_text(FOCUS_TEMPLATE)
        || meaningful_continuity_lines(&continuity.focus.content) == 0
    {
        warnings.push(ContextHealthWarning {
            code: "focus_document_thin".to_string(),
            severity: WarningSeverity::Critical,
            summary: "The active focus document is still effectively empty.".to_string(),
            evidence: "the focus document still matches the bootstrap template".to_string(),
            recommended_action: "Rebuild focus with the current status, blocker, next step, and finish rule before doing more continuity work.".to_string(),
        });
    }
    if mission_contract.control_points() <= 2 {
        warnings.push(ContextHealthWarning {
            code: "mission_contract_thin".to_string(),
            severity: if mission_contract.control_points() <= 1 {
                WarningSeverity::Critical
            } else {
                WarningSeverity::Warning
            },
            summary: "The durable mission contract is underspecified.".to_string(),
            evidence: mission_contract.summary(),
            recommended_action: "Refresh focus and anchors so the loop has the current status, blocker, next step, finish rule, and lasting constraints.".to_string(),
        });
    }
    if repetition > 0 {
        warnings.push(ContextHealthWarning {
            code: "recent_user_turn_repeated".to_string(),
            severity: if repetition > 1 {
                WarningSeverity::Critical
            } else {
                WarningSeverity::Warning
            },
            summary: "The latest user turn overlaps with a recent user turn.".to_string(),
            evidence: format!(
                "detected {} recent duplicate-like user prompt(s)",
                repetition
            ),
            recommended_action:
                "Check whether the loop is retrying a failed tactic without new evidence."
                    .to_string(),
        });
    }
    if blocked_count >= 2 {
        warnings.push(ContextHealthWarning {
            code: "blocked_status_loop".to_string(),
            severity: if blocked_count >= 3 {
                WarningSeverity::Critical
            } else {
                WarningSeverity::Warning
            },
            summary: "Recent assistant history shows repeated blocked-status notes.".to_string(),
            evidence: format!("{blocked_count} recent assistant messages look blocked or stalled"),
            recommended_action: "Revalidate the blocker against current evidence and ban the failed tactic unless new inputs appeared.".to_string(),
        });
    }
    if repair_prompt_count >= 2 {
        warnings.push(ContextHealthWarning {
            code: "repair_prompt_churn".to_string(),
            severity: if repair_prompt_count >= 3 {
                WarningSeverity::Critical
            } else {
                WarningSeverity::Warning
            },
            summary: "Internal continuation or repair prompts are crowding out normal work."
                .to_string(),
            evidence: format!(
                "{repair_prompt_count} recent internal prompts look like recovery or cleanup work"
            ),
            recommended_action:
                "Do one bounded repair pass only, then replan or resume the real goal.".to_string(),
        });
    }
    let loop_pressure = repetition + blocked_count + repair_prompt_count;
    if loop_pressure > 0 && negative_memory.total_signals() == 0 {
        warnings.push(ContextHealthWarning {
            code: "failure_memory_missing".to_string(),
            severity: WarningSeverity::Critical,
            summary: "The loop is under pressure, but the context does not preserve any explicit failure memory.".to_string(),
            evidence: format!(
                "loop pressure={} while cause={}, turning_points={}, constraints={}, forgotten={}",
                loop_pressure,
                negative_memory.cause_lines,
                negative_memory.turning_point_lines,
                negative_memory.invariant_lines,
                forgotten_entries.len()
            ),
            recommended_action: "Record the failed tactic, the real blocker, and the retry condition before trying again.".to_string(),
        });
    }
    warnings
}

fn continuity_coverage_summary(continuity: &lcm::ContinuityShowAll) -> String {
    let narrative = meaningful_continuity_lines(&continuity.narrative.content);
    let anchors = meaningful_continuity_lines(&continuity.anchors.content);
    let focus = meaningful_continuity_lines(&continuity.focus.content);
    format!(
        "Continuity currently exposes {} narrative, {} anchor, and {} focus line(s) beyond section headers.",
        narrative, anchors, focus
    )
}

fn inspect_mission_contract(continuity: &lcm::ContinuityShowAll) -> MissionContractCoverage {
    let focus_contract_lines = section_lines(&continuity.focus.content, "Contract");
    let focus_state_lines = section_lines(&continuity.focus.content, "State");
    let anchor_entry_lines = section_lines(&continuity.anchors.content, "Entries");
    MissionContractCoverage {
        status_lines: count_named_presence(
            &focus_contract_lines,
            &["mission", "mission_state", "slice", "slice_state"],
        )
        .max(meaningful_section_lines(
            &continuity.focus.content,
            "Status",
        )),
        blocker_lines: count_named_presence(
            &focus_state_lines,
            &["blocker", "verification_gap", "missing_dependency"],
        )
        .max(meaningful_section_lines(
            &continuity.focus.content,
            "Blocker",
        )),
        next_lines: count_named_presence(&focus_state_lines, &["next_slice"])
            .max(meaningful_section_lines(&continuity.focus.content, "Next")),
        done_gate_lines: count_named_presence(&focus_state_lines, &["done_gate"]).max(
            meaningful_section_lines(&continuity.focus.content, "Done / Gate"),
        ),
        invariant_lines: count_anchor_types(
            &anchor_entry_lines,
            &["constraint", "prohibition", "retry_boundary"],
        )
        .max(meaningful_section_lines(
            &continuity.anchors.content,
            "Invarianten / Verbote",
        )),
    }
}

fn inspect_negative_memory(
    continuity: &lcm::ContinuityShowAll,
    forgotten_entries: &[lcm::ContinuityForgottenEntry],
) -> NegativeMemoryCoverage {
    let narrative_entry_lines = section_lines(&continuity.narrative.content, "Entries");
    let anchor_entry_lines = section_lines(&continuity.anchors.content, "Entries");
    let invariant_lines = count_anchor_types(
        &anchor_entry_lines,
        &["constraint", "prohibition", "retry_boundary"],
    )
    .max(
        section_lines(&continuity.anchors.content, "Invarianten / Verbote")
            .into_iter()
            .filter(|line| looks_like_negative_constraint(line))
            .count(),
    );
    NegativeMemoryCoverage {
        cause_lines: count_narrative_event_types(
            &narrative_entry_lines,
            &["failure", "blocked", "incident", "timeout", "regression"],
        )
        .max(meaningful_section_lines(
            &continuity.narrative.content,
            "Ursache",
        )),
        turning_point_lines: count_narrative_event_types(
            &narrative_entry_lines,
            &["decision", "turning_point", "repair", "replan", "handoff"],
        )
        .max(meaningful_section_lines(
            &continuity.narrative.content,
            "Wendepunkte",
        )),
        invariant_lines,
        forgotten_lines: forgotten_entries.len(),
    }
}

fn count_named_presence(lines: &[String], names: &[&str]) -> usize {
    names
        .iter()
        .filter(|name| line_has_named_value(lines, name))
        .count()
}

fn line_has_named_value(lines: &[String], name: &str) -> bool {
    lines.iter().any(|line| {
        normalize_data_line(line)
            .split_once(':')
            .map(|(prefix, value)| {
                prefix.trim().eq_ignore_ascii_case(name) && !value.trim().is_empty()
            })
            .unwrap_or(false)
    })
}

fn count_anchor_types(lines: &[String], wanted: &[&str]) -> usize {
    lines
        .iter()
        .filter_map(|line| normalize_data_line(line).split_once(':'))
        .filter(|(prefix, value)| {
            prefix.trim().eq_ignore_ascii_case("anchor_type")
                && wanted
                    .iter()
                    .any(|name| value.trim().eq_ignore_ascii_case(name))
        })
        .count()
}

fn count_narrative_event_types(lines: &[String], wanted: &[&str]) -> usize {
    lines
        .iter()
        .filter_map(|line| normalize_data_line(line).split_once(':'))
        .filter(|(prefix, value)| {
            prefix.trim().eq_ignore_ascii_case("event_type")
                && wanted
                    .iter()
                    .any(|name| value.trim().eq_ignore_ascii_case(name))
        })
        .count()
}

fn section_lines(content: &str, section_name: &str) -> Vec<String> {
    let mut active = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(header) = trimmed.strip_prefix("## ") {
            active = header == section_name;
            continue;
        }
        if active && !trimmed.is_empty() && !trimmed.starts_with('#') {
            lines.push(trimmed.to_string());
        }
    }
    lines
}

fn meaningful_section_lines(content: &str, section_name: &str) -> usize {
    section_lines(content, section_name)
        .into_iter()
        .filter(|line| line_carries_payload(line))
        .count()
}

fn normalize_data_line(line: &str) -> &str {
    line.trim()
        .strip_prefix("- ")
        .map(str::trim_start)
        .unwrap_or_else(|| line.trim())
}

fn line_carries_payload(line: &str) -> bool {
    let normalized = normalize_data_line(line);
    if normalized.is_empty() || normalized.eq_ignore_ascii_case("none") {
        return false;
    }
    normalized
        .split_once(':')
        .map(|(_, value)| {
            let value = value.trim();
            !value.is_empty() && !value.eq_ignore_ascii_case("none")
        })
        .unwrap_or(true)
}

fn detect_current_workspace_root(
    snapshot: &lcm::LcmSnapshot,
    latest_user_prompt: &str,
) -> Option<String> {
    if let Some(workspace) = extract_workspace_root(latest_user_prompt) {
        return Some(workspace);
    }
    snapshot
        .messages
        .iter()
        .rev()
        .filter(|message| message.role.eq_ignore_ascii_case("user"))
        .find_map(|message| extract_workspace_root(&message.content))
}

fn extract_workspace_root(content: &str) -> Option<String> {
    let mut lines = content.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "Work only inside this workspace:" {
            for candidate in lines.by_ref() {
                let trimmed = candidate.trim();
                if trimmed.is_empty() {
                    continue;
                }
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn looks_like_negative_constraint(line: &str) -> bool {
    let normalized = normalize_text(line);
    [
        "nicht",
        "never",
        "avoid",
        "verbot",
        "ban",
        "retry only",
        "do not",
        "kein",
        "ohne",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn summarize_dimensions(
    dimensions: &[ContextHealthDimension],
    warnings: &[ContextHealthWarning],
    status: ContextHealthStatus,
    overall_score: u8,
) -> String {
    let mut weakest = dimensions.to_vec();
    weakest.sort_by_key(|dimension| dimension.score);
    let drivers = weakest
        .into_iter()
        .take(2)
        .map(|dimension| format!("{}={}", dimension.name, dimension.score))
        .collect::<Vec<_>>()
        .join(", ");
    let warning_summary = if warnings.is_empty() {
        "No active warnings.".to_string()
    } else {
        let strongest = warnings
            .iter()
            .max_by_key(|warning| match warning.severity {
                WarningSeverity::Info => 0,
                WarningSeverity::Warning => 1,
                WarningSeverity::Critical => 2,
            })
            .map(|warning| match warning.severity {
                WarningSeverity::Info => "info",
                WarningSeverity::Warning => "warning",
                WarningSeverity::Critical => "critical",
            })
            .unwrap_or("warning");
        format!(
            "{} active warning(s), strongest severity {}.",
            warnings.len(),
            strongest
        )
    };
    format!(
        "Context health is {} at score {}. Weakest dimensions: {}. {}",
        status.as_str(),
        overall_score,
        drivers,
        warning_summary
    )
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn repeated_recent_user_turns(snapshot: &lcm::LcmSnapshot, latest_user_prompt: &str) -> usize {
    let target = normalize_text(latest_user_prompt);
    if target.is_empty() {
        return 0;
    }
    snapshot
        .messages
        .iter()
        .rev()
        .filter(|message| message.role == "user")
        .skip(1)
        .take(8)
        .filter(|message| normalize_text(&message.content) == target)
        .count()
}

fn recent_blocked_status_count(snapshot: &lcm::LcmSnapshot) -> usize {
    snapshot
        .messages
        .iter()
        .rev()
        .filter(|message| message.role == "assistant")
        .take(8)
        .filter(|message| message_indicates_blocked_status(message))
        .count()
}

/// Prefer the structured `agent_outcome` (F3) when present; fall back to
/// the legacy text-status scrape only for assistant rows that predate the
/// schema upgrade and therefore have a NULL outcome. New code paths must
/// populate `agent_outcome` so this fallback fades away.
fn message_indicates_blocked_status(message: &lcm::MessageRecord) -> bool {
    if let Some(token) = message.agent_outcome.as_deref() {
        if let Some(outcome) = lcm::AgentOutcome::from_token(token) {
            return outcome.is_agent_failure();
        }
    }
    looks_like_blocked_status(&message.content)
}

fn looks_like_blocked_status(content: &str) -> bool {
    let normalized = normalize_text(content);
    normalized.starts_with("status blocked")
        || normalized.starts_with("blocked")
        || normalized.contains("still blocked")
        || normalized.contains("remains blocked")
        || normalized.contains("bleibt blockiert")
}

fn recent_internal_repair_prompt_count(snapshot: &lcm::LcmSnapshot) -> usize {
    snapshot
        .messages
        .iter()
        .rev()
        .filter(|message| message.role == "user")
        .take(8)
        .filter(|message| is_internal_repair_prompt(&message.content))
        .count()
}

fn is_internal_repair_prompt(content: &str) -> bool {
    let trimmed = content.trim_start();
    [
        "Continue the broader goal using the latest completed turn as the starting point.",
        "Review the blocked owner-visible task without losing continuity.",
        "Recover or finish the owner-visible task without losing continuity.",
        "Use the queue-cleanup skill first.",
        "Review and repair CTOX context health without letting repair become the main mission.",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}

fn is_context_repair_source(source_label: &str) -> bool {
    let normalized = normalize_text(source_label);
    normalized.contains("context-health") || normalized.contains("queue-guard")
}

fn looks_like_context_repair_goal(goal: &str) -> bool {
    let normalized = normalize_text(goal);
    normalized.contains("context health")
        || normalized.contains("repair ctox context")
        || normalized.contains("queue cleanup")
}

fn meaningful_continuity_lines(content: &str) -> usize {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && line_carries_payload(line))
        .count()
}

fn normalize_text(content: &str) -> String {
    content
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::assess;
    use super::assess_with_forgotten;
    use super::evaluate_repair_governor;
    use super::render_prompt_block;
    use super::ContextHealthStatus;
    use super::WarningSeverity;
    use crate::lcm;

    fn sample_snapshot(messages: Vec<(&str, &str)>, tokens: i64) -> lcm::LcmSnapshot {
        let message_records = messages
            .into_iter()
            .enumerate()
            .map(|(index, (role, content))| lcm::MessageRecord {
                message_id: index as i64 + 1,
                conversation_id: 1,
                seq: index as i64 + 1,
                role: role.to_string(),
                content: content.to_string(),
                token_count: 100,
                created_at: "2026-03-31T00:00:00Z".to_string(),
                agent_outcome: None,
            })
            .collect::<Vec<_>>();
        lcm::LcmSnapshot {
            conversation_id: 1,
            messages: message_records,
            summaries: Vec::new(),
            context_items: vec![lcm::ContextItemSnapshot {
                ordinal: 1,
                item_type: lcm::ContextItemType::Message,
                message_id: Some(1),
                summary_id: None,
                seq: 1,
                depth: 0,
                token_count: tokens,
            }],
            summary_edges: Vec::new(),
            summary_messages: Vec::new(),
        }
    }

    fn healthy_continuity() -> lcm::ContinuityShowAll {
        lcm::ContinuityShowAll {
            conversation_id: 1,
            narrative: lcm::ContinuityDocumentState {
                conversation_id: 1,
                kind: lcm::ContinuityKind::Narrative,
                head_commit_id: "n1".to_string(),
                content: "# Narrative\n\n## Entries\n- entry_id: drift-1\n  event_type: failure\n  summary: Queue drift was observed.\n  consequence: Stale blocker notes kept getting reused.\n  source_class: tool_observed\n  source_ref: log://queue\n  observed_at: 2026-03-31T00:00:00Z\n- entry_id: drift-2\n  event_type: turning_point\n  summary: A failed retry was already observed.\n  consequence: Repair remained pending.\n  source_class: tool_observed\n  source_ref: log://queue\n  observed_at: 2026-03-31T00:05:00Z\n".to_string(),
                created_at: "2026-03-31T00:00:00Z".to_string(),
                updated_at: "2026-03-31T00:00:00Z".to_string(),
            },
            anchors: lcm::ContinuityDocumentState {
                conversation_id: 1,
                kind: lcm::ContinuityKind::Anchors,
                head_commit_id: "a1".to_string(),
                content: "# Anchors\n\n## Entries\n- anchor_id: a-1\n  anchor_type: constraint\n  statement: runtime/ctox.sqlite3 is canonical state.\n  source_class: static_policy\n  source_ref: contract://context\n  observed_at: 2026-03-31T00:00:00Z\n  confidence: high\n  supersedes:\n  expires_at:\n- anchor_id: a-2\n  anchor_type: retry_boundary\n  statement: Do not retry the same repair without new evidence.\n  source_class: tool_observed\n  source_ref: log://queue\n  observed_at: 2026-03-31T00:00:00Z\n  confidence: high\n  supersedes:\n  expires_at:\n".to_string(),
                created_at: "2026-03-31T00:00:00Z".to_string(),
                updated_at: "2026-03-31T00:00:00Z".to_string(),
            },
            focus: lcm::ContinuityDocumentState {
                conversation_id: 1,
                kind: lcm::ContinuityKind::Focus,
                head_commit_id: "f1".to_string(),
                content: "# Focus\n\n## Contract\nmission: repair queue drift\nmission_state: active\ncontinuation_mode: continuous\ntrigger_intensity: hot\nslice: score context health\nslice_state: in_progress\n\n## State\ngoal: add scoring\nblocker: none\nmissing_dependency:\nnext_slice: add scoring\ndone_gate: tests green\nretry_condition:\nclosure_confidence: medium\n\n## Sources\nsource_refs:\n- log://queue\nupdated_at: 2026-03-31T00:00:00Z\n".to_string(),
                created_at: "2026-03-31T00:00:00Z".to_string(),
                updated_at: "2026-03-31T00:00:00Z".to_string(),
            },
        }
    }

    #[test]
    fn assess_marks_repeated_blocked_context_as_critical() {
        let snapshot = sample_snapshot(
            vec![
                ("user", "retry redis"),
                (
                    "assistant",
                    "Status: `blocked`\n\nBlocker: redis still offline",
                ),
                ("user", "retry redis"),
                (
                    "assistant",
                    "Status: `blocked`\n\nBlocker: redis still offline",
                ),
                ("user", "retry redis"),
            ],
            118_000,
        );
        let mut continuity = healthy_continuity();
        continuity.focus.content =
            "# Focus\n\n## Contract\nmission:\nmission_state:\ncontinuation_mode:\ntrigger_intensity:\nslice:\nslice_state:\n\n## State\ngoal:\nblocker:\nmissing_dependency:\nnext_slice:\ndone_gate:\nretry_condition:\nclosure_confidence:\n\n## Sources\nsource_refs:\n- none\nupdated_at:\n".to_string();
        let health = assess(&snapshot, &continuity, "retry redis", 131_072);
        assert_eq!(health.status, ContextHealthStatus::Critical);
        assert!(health.repair_recommended);
        assert!(health
            .warnings
            .iter()
            .any(|warning| warning.code == "blocked_status_loop"));
        assert!(health
            .warnings
            .iter()
            .any(|warning| warning.severity == WarningSeverity::Critical));
    }

    #[test]
    fn warns_when_mission_contract_and_failure_memory_are_missing() {
        let snapshot = sample_snapshot(
            vec![
                ("user", "retry deploy"),
                (
                    "assistant",
                    "Status: `blocked`\n\nBlocker: deploy still failing",
                ),
                ("user", "retry deploy"),
            ],
            10_000,
        );
        let continuity = lcm::ContinuityShowAll {
            conversation_id: 1,
            narrative: lcm::ContinuityDocumentState {
                conversation_id: 1,
                kind: lcm::ContinuityKind::Narrative,
                head_commit_id: "n1".to_string(),
                content: "# Narrative\n\n## Entries\n- entry_id: deploy-1\n  event_type: status\n  summary: Rollout in progress.\n  consequence: Deployment drift is present.\n  source_class: tool_observed\n  source_ref: log://deploy\n  observed_at: 2026-03-31T00:00:00Z\n".to_string(),
                created_at: "2026-03-31T00:00:00Z".to_string(),
                updated_at: "2026-03-31T00:00:00Z".to_string(),
            },
            anchors: lcm::ContinuityDocumentState {
                conversation_id: 1,
                kind: lcm::ContinuityKind::Anchors,
                head_commit_id: "a1".to_string(),
                content: "# Anchors\n\n## Entries\n- anchor_id: deploy-a1\n  anchor_type: fact\n  statement: release.tar is the current release artifact.\n  source_class: tool_observed\n  source_ref: file://release.tar\n  observed_at: 2026-03-31T00:00:00Z\n  confidence: medium\n  supersedes:\n  expires_at:\n".to_string(),
                created_at: "2026-03-31T00:00:00Z".to_string(),
                updated_at: "2026-03-31T00:00:00Z".to_string(),
            },
            focus: lcm::ContinuityDocumentState {
                conversation_id: 1,
                kind: lcm::ContinuityKind::Focus,
                head_commit_id: "f1".to_string(),
                content: "# Focus\n\n## Contract\nmission: repair deploy drift\nmission_state: active\ncontinuation_mode: continuous\ntrigger_intensity: hot\nslice:\nslice_state: blocked\n\n## State\ngoal: investigate deploy drift\nblocker:\nmissing_dependency:\nnext_slice:\ndone_gate:\nretry_condition:\nclosure_confidence:\n\n## Sources\nsource_refs:\n- log://deploy\nupdated_at: 2026-03-31T00:00:00Z\n".to_string(),
                created_at: "2026-03-31T00:00:00Z".to_string(),
                updated_at: "2026-03-31T00:00:00Z".to_string(),
            },
        };
        let health = assess(&snapshot, &continuity, "retry deploy", 131_072);
        assert_eq!(health.status, ContextHealthStatus::Critical);
        assert!(health
            .warnings
            .iter()
            .any(|warning| warning.code == "mission_contract_thin"));
        assert!(health
            .warnings
            .iter()
            .any(|warning| warning.code == "failure_memory_missing"));
    }

    #[test]
    fn verification_gap_counts_as_mission_contract_coverage() {
        let snapshot = sample_snapshot(vec![("user", "continue progress report")], 10_000);
        let continuity = lcm::ContinuityShowAll {
            conversation_id: 1,
            narrative: healthy_continuity().narrative,
            anchors: healthy_continuity().anchors,
            focus: lcm::ContinuityDocumentState {
                conversation_id: 1,
                kind: lcm::ContinuityKind::Focus,
                head_commit_id: "f1".to_string(),
                content: "# Focus\n\n## Contract\nmission: marketplace delivery mission\nmission_state: active\ncontinuation_mode: continuous\ntrigger_intensity: hot\nslice: report cycle 2\nslice_state: report_due\n\n## State\ngoal: deliver the progress report\nverification_gap: workspace state not yet re-verified after a prior timeout\nnext_slice: inspect workspace and update progress artifact\ndone_gate: progress artifact updated and report returned\nretry_condition:\nclosure_confidence: medium\n\n## Sources\nsource_refs:\n- file://ops/progress/progress-latest.md\nupdated_at: 2026-03-31T00:00:00Z\n".to_string(),
                created_at: "2026-03-31T00:00:00Z".to_string(),
                updated_at: "2026-03-31T00:00:00Z".to_string(),
            },
        };
        let health = assess(&snapshot, &continuity, "continue progress report", 131_072);
        assert!(!health
            .warnings
            .iter()
            .any(|warning| warning.code == "mission_contract_thin"));
    }

    #[test]
    fn render_prompt_block_surfaces_repair_guidance() {
        let snapshot = sample_snapshot(vec![("user", "continue task")], 120_000);
        let mut continuity = healthy_continuity();
        continuity.focus.content =
            "# Focus\n\n## Contract\nmission:\nmission_state:\ncontinuation_mode:\ntrigger_intensity:\nslice:\nslice_state:\n\n## State\ngoal:\nblocker:\nmissing_dependency:\nnext_slice:\ndone_gate:\nretry_condition:\nclosure_confidence:\n\n## Sources\nsource_refs:\n- none\nupdated_at:\n".to_string();
        let health = assess(&snapshot, &continuity, "continue task", 131_072);
        let block = render_prompt_block(&health);
        assert!(block.contains("Context health:"));
        assert!(block.contains("repair_recommended:"));
    }

    #[test]
    fn render_prompt_block_stays_compact() {
        let snapshot = sample_snapshot(
            vec![
                ("user", "retry deploy"),
                ("assistant", "blocked: deploy still failing"),
                ("user", "retry deploy"),
            ],
            120_000,
        );
        let mut continuity = healthy_continuity();
        continuity.focus.content =
            "# Focus\n\n## Contract\nmission:\nmission_state:\ncontinuation_mode:\ntrigger_intensity:\nslice:\nslice_state:\n\n## State\ngoal:\nblocker:\nmissing_dependency:\nnext_slice:\ndone_gate:\nretry_condition:\nclosure_confidence:\n\n## Sources\nsource_refs:\n- none\nupdated_at:\n".to_string();
        let health = assess(&snapshot, &continuity, "retry deploy", 131_072);
        let block = render_prompt_block(&health);
        assert!(block.contains("Context health:"));
        assert!(block.contains("status:"));
        assert!(block.contains("preempt_current_slice:"));
        assert!(block.contains("warnings:"));
        assert!(!block.contains("This block is advisory."));
        assert!(
            block.len() < 1000,
            "context health block too large: {}",
            block.len()
        );
    }

    #[test]
    fn forgotten_lines_count_as_negative_memory() {
        let snapshot = sample_snapshot(
            vec![
                ("user", "retry deploy"),
                (
                    "assistant",
                    "Status: `blocked`\n\nBlocker: deploy still failing",
                ),
                ("user", "retry deploy"),
            ],
            10_000,
        );
        let mut continuity = healthy_continuity();
        continuity.narrative.content =
            "# Narrative\n\n## Entries\n- entry_id: drift-1\n  event_type: status\n  summary: Queue drift was observed.\n  consequence:\n  source_class: tool_observed\n  source_ref: log://queue\n  observed_at: 2026-03-31T00:00:00Z\n".to_string();
        let forgotten = vec![lcm::ContinuityForgottenEntry {
            commit_id: "c1".to_string(),
            conversation_id: 1,
            kind: lcm::ContinuityKind::Narrative,
            line: "Do not retry deploy.sh until the missing secret is restored.".to_string(),
            created_at: "2026-03-31T00:00:00Z".to_string(),
        }];
        let health =
            assess_with_forgotten(&snapshot, &continuity, &forgotten, "retry deploy", 131_072);
        assert!(!health
            .warnings
            .iter()
            .any(|warning| warning.code == "failure_memory_missing"));
    }

    #[test]
    fn critical_warning_overrides_healthy_score_band() {
        let snapshot = sample_snapshot(vec![("user", "continue task")], 0);
        let mut continuity = healthy_continuity();
        continuity.focus.content =
            "# Focus\n\n## Contract\nmission:\nmission_state:\ncontinuation_mode:\ntrigger_intensity:\nslice:\nslice_state:\n\n## State\ngoal:\nblocker:\nmissing_dependency:\nnext_slice:\ndone_gate:\nretry_condition:\nclosure_confidence:\n\n## Sources\nsource_refs:\n- none\nupdated_at:\n".to_string();
        let health = assess(&snapshot, &continuity, "continue task", 131_072);
        assert_eq!(health.status, ContextHealthStatus::Critical);
        assert!(health.summary.contains("strongest severity critical"));
    }

    #[test]
    fn repair_governor_blocks_recursive_repair() {
        let snapshot = sample_snapshot(vec![("user", "continue task")], 120_000);
        let mut continuity = healthy_continuity();
        continuity.focus.content =
            "# Focus\n\n## Contract\nmission:\nmission_state:\ncontinuation_mode:\ntrigger_intensity:\nslice:\nslice_state:\n\n## State\ngoal:\nblocker:\nmissing_dependency:\nnext_slice:\ndone_gate:\nretry_condition:\nclosure_confidence:\n\n## Sources\nsource_refs:\n- none\nupdated_at:\n".to_string();
        let health = assess(&snapshot, &continuity, "continue task", 131_072);
        let decision = evaluate_repair_governor(
            &health,
            "context-health",
            "Review and repair CTOX context health",
            false,
            0,
        );
        assert!(!decision.should_enqueue_repair);
        assert!(decision.reason.contains("already is"));
    }
}
