use serde::Deserialize;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

use crate::inference::runtime_env;

const REVIEW_TIMEOUT_SECS: u64 = 90;

const REVIEW_SYSTEM_PROMPT: &str = r#"You are CTOX's completion reviewer.

Your job is to stop CTOX from treating an under-verified execution slice as complete.

Operate in strict read-only review mode:
- Do not modify project files.
- Do not run git write operations.
- Do not install packages or change system configuration.
- Prefer direct checks against the current repo, runtime, processes, logs, and tests over prose-only reasoning.
- If a claim can be verified with a command, do that instead of merely restating the claim.

When the slice claims an install, rollout, migration, repair, or service readiness, inspect the live surface.
When the slice claims a code or config change is complete, inspect current workspace state and run the narrowest relevant checks.
If evidence is incomplete, return PARTIAL instead of PASS.

Respond in exactly this shape:
VERDICT: PASS|FAIL|PARTIAL
SUMMARY: <one sentence>
OPEN_ITEMS:
- <item>
EVIDENCE:
- <command or check> => <observed result>
"#;

#[derive(Debug, Clone)]
pub struct CompletionReviewRequest {
    pub goal: String,
    pub prompt: String,
    pub preview: String,
    pub source_label: String,
    pub owner_visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReviewVerdict {
    Pass,
    Fail,
    Partial,
    Skipped,
    Unavailable,
}

impl ReviewVerdict {
    pub fn as_gate_label(&self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Partial => "partial",
            Self::Skipped => "skipped",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewOutcome {
    pub required: bool,
    pub verdict: ReviewVerdict,
    pub summary: String,
    pub report: String,
    pub score: u8,
    pub reasons: Vec<String>,
}

impl ReviewOutcome {
    pub fn skipped(summary: impl Into<String>) -> Self {
        Self {
            required: false,
            verdict: ReviewVerdict::Skipped,
            summary: summary.into(),
            report: String::new(),
            score: 0,
            reasons: Vec::new(),
        }
    }

    pub fn requires_follow_up(&self) -> bool {
        self.required && self.verdict != ReviewVerdict::Pass
    }
}

pub fn review_completion_if_needed(
    root: &Path,
    request: &CompletionReviewRequest,
    result_text: &str,
) -> ReviewOutcome {
    let (required, score, reasons) = assess_review_requirement(request, result_text);
    if !required {
        return ReviewOutcome::skipped("Completion review gate not triggered for this slice.");
    }

    let settings = runtime_env::effective_runtime_env_map(root).unwrap_or_default();
    let review_prompt = build_review_prompt(request, result_text, &reasons);
    match crate::execution::agent::direct_session::run_direct_session(
        crate::execution::agent::direct_session::DirectSessionRequest {
            root,
            settings: &settings,
            prompt: &review_prompt,
            workspace_root: None,
            timeout: Some(Duration::from_secs(REVIEW_TIMEOUT_SECS)),
            base_instructions: Some(REVIEW_SYSTEM_PROMPT),
            include_apply_patch_tool: Some(false),
            conversation_id: 0,
        },
    ) {
        Ok(report) => parse_review_report(score, reasons, &report),
        Err(err) => ReviewOutcome {
            required: true,
            verdict: ReviewVerdict::Unavailable,
            summary: format!(
                "Completion review could not finish: {}",
                clip_text(&err.to_string(), 180)
            ),
            report: err.to_string(),
            score,
            reasons,
        },
    }
}

fn assess_review_requirement(
    request: &CompletionReviewRequest,
    result_text: &str,
) -> (bool, u8, Vec<String>) {
    let combined = format!(
        "{}\n{}\n{}\n{}",
        request.goal, request.prompt, request.preview, result_text
    );
    let lowered = combined.to_ascii_lowercase();
    let mut score = 0u8;
    let mut reasons = Vec::new();

    let closure_claim = contains_any(
        &lowered,
        &[
            "done",
            "completed",
            "finished",
            "verified",
            "works now",
            "fixed",
            "installed",
            "configured",
            "rolled out",
            "deploy",
            "smoke test",
            "tests pass",
            "validated",
        ],
    );
    if closure_claim {
        score = score.saturating_add(1);
        push_unique_reason(&mut reasons, "closure_claim");
    }

    let runtime_or_infra_change = contains_any(
        &lowered,
        &[
            "deploy",
            "rollout",
            "install",
            "migration",
            "database",
            "schema",
            "service",
            "systemd",
            "restart",
            "http",
            "api",
            "endpoint",
            "config",
            "nginx",
            "docker",
            "compose",
            "secret",
            "credential",
        ],
    );
    if runtime_or_infra_change {
        score = score.saturating_add(2);
        push_unique_reason(&mut reasons, "runtime_or_infra_change");
    }

    let code_or_artifact_change = contains_any(
        &lowered,
        &[
            "patch",
            "refactor",
            "updated",
            "changed",
            "edit",
            "helper",
            "skill",
            "contract",
            "src/",
            ".rs",
            ".ts",
            ".py",
            "cargo.toml",
            "package.json",
        ],
    );
    if code_or_artifact_change {
        score = score.saturating_add(1);
        push_unique_reason(&mut reasons, "code_or_artifact_change");
    }

    if combined.chars().count() > 900 {
        score = score.saturating_add(1);
        push_unique_reason(&mut reasons, "long_complex_slice");
    }

    if request.owner_visible && (closure_claim || runtime_or_infra_change) {
        score = score.saturating_add(1);
        push_unique_reason(&mut reasons, "owner_visible_claim");
    }

    (score >= 3, score, reasons)
}

fn build_review_prompt(
    request: &CompletionReviewRequest,
    result_text: &str,
    reasons: &[String],
) -> String {
    let reason_block = if reasons.is_empty() {
        "none".to_string()
    } else {
        reasons.join(", ")
    };
    format!(
        "Review whether the latest CTOX execution slice is actually safe to treat as complete.\n\nReview trigger reasons: {reason_block}\nSource label: {}\nOwner visible: {}\nGoal:\n{}\n\nOriginal slice prompt:\n{}\n\nLatest reported result:\n{}\n\nUse direct evidence where feasible. If the slice claims a runtime change, inspect the live runtime or repo state. If it claims a fix or artifact change, inspect current workspace state and run the narrowest relevant checks. If the slice is not safe to treat as complete yet, do not wave it through.",
        request.source_label,
        if request.owner_visible { "yes" } else { "no" },
        request.goal.trim(),
        request.prompt.trim(),
        result_text.trim(),
    )
}

fn parse_review_report(score: u8, reasons: Vec<String>, report: &str) -> ReviewOutcome {
    let parsed_verdict = parse_verdict(report);
    let verdict = parsed_verdict.clone().unwrap_or(ReviewVerdict::Partial);
    let summary = if parsed_verdict.is_none() {
        match parse_prefixed_line(report, "SUMMARY:") {
            Some(summary) if !summary.is_empty() => format!(
                "Review report did not contain an explicit verdict, so the slice stays open. {}",
                summary
            ),
            _ => "Review report did not contain an explicit verdict, so the slice stays open."
                .to_string(),
        }
    } else {
        parse_prefixed_line(report, "SUMMARY:")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| clip_text(report, 180))
    };
    ReviewOutcome {
        required: true,
        verdict,
        summary,
        report: report.trim().to_string(),
        score,
        reasons,
    }
}

fn parse_verdict(report: &str) -> Option<ReviewVerdict> {
    for line in report.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("VERDICT:") else {
            continue;
        };
        return match rest.trim().to_ascii_uppercase().as_str() {
            "PASS" => Some(ReviewVerdict::Pass),
            "FAIL" => Some(ReviewVerdict::Fail),
            "PARTIAL" => Some(ReviewVerdict::Partial),
            _ => None,
        };
    }
    None
}

fn parse_prefixed_line(report: &str, prefix: &str) -> Option<String> {
    for line in report.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(prefix) else {
            continue;
        };
        let value = rest.trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn push_unique_reason(reasons: &mut Vec<String>, candidate: &str) {
    if !reasons.iter().any(|existing| existing == candidate) {
        reasons.push(candidate.to_string());
    }
}

fn clip_text(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut clipped = collapsed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_review_for_owner_visible_runtime_completion_claim() {
        let request = CompletionReviewRequest {
            goal: "Install Redis and finish the rollout".to_string(),
            prompt: "Install Redis, configure systemd, and verify the HTTP admin surface."
                .to_string(),
            preview: "Install Redis".to_string(),
            source_label: "queue".to_string(),
            owner_visible: true,
        };
        let (required, score, reasons) = assess_review_requirement(
            &request,
            "Installed Redis, restarted the service, and verified the smoke test.",
        );
        assert!(required);
        assert!(score >= 3);
        assert!(reasons
            .iter()
            .any(|reason| reason == "runtime_or_infra_change"));
    }

    #[test]
    fn skips_review_for_short_explanatory_slice() {
        let request = CompletionReviewRequest {
            goal: "Explain the current queue state".to_string(),
            prompt: "Summarize the queue status for the owner.".to_string(),
            preview: "Queue summary".to_string(),
            source_label: "tui".to_string(),
            owner_visible: true,
        };
        let (required, _, _) = assess_review_requirement(
            &request,
            "Explained the current queue backlog and highlighted the blocked task.",
        );
        assert!(!required);
    }

    #[test]
    fn parses_review_report_with_explicit_verdict() {
        let outcome = parse_review_report(
            4,
            vec!["closure_claim".to_string()],
            "VERDICT: FAIL\nSUMMARY: HTTP health check still returns 502.\nOPEN_ITEMS:\n- Repair upstream config",
        );
        assert_eq!(outcome.verdict, ReviewVerdict::Fail);
        assert!(outcome.summary.contains("502"));
        assert!(outcome.requires_follow_up());
    }

    #[test]
    fn missing_verdict_keeps_slice_open() {
        let outcome = parse_review_report(3, vec![], "SUMMARY: Looked okay overall.");
        assert_eq!(outcome.verdict, ReviewVerdict::Partial);
        assert!(outcome.summary.contains("stays open"));
    }
}
