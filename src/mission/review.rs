use serde::Deserialize;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

use crate::execution::agent::direct_session::PersistentSession;
use crate::inference::runtime_env;

const REVIEW_TIMEOUT_SECS: u64 = 300;

const REVIEW_SYSTEM_PROMPT: &str = r#"You are CTOX Review.

You run an external verification pass for one reviewed slice.

Use the review assignment as the only task definition.
Gather everything else yourself through read-only inspection of the workspace, runtime store, tickets, communication state, live services, logs, browser surface, and other available tools.

Operate in strict read-only verification mode.
Use the same inspection tools as normal CTOX work.
Use multiple tool turns as needed.

Verification standard:
- explicit done gates
- reviewed claims
- resulting mission state
- public-surface quality for owner-visible or public work
- commercial credibility and buyer-path integrity for launch work

Public-surface failures include:
- internal instruction leakage
- planning or operator text shown publicly
- admin or backoffice exposure in the buyer path
- broken critical routes or dependent APIs
- placeholder, internal, technical, or commercially weak copy
- visibly non-launch-worthy layout, hierarchy, or interaction quality

Compaction policy for review:
- normal review compaction is disabled
- if the run reaches the point where another reviewer should continue, stop and emit a review handoff instead of compacting
- a review handoff must summarize what was verified, the decisive facts, the remaining checks, and the next best verification targets

Decision policy:
- PASS only when the gates are satisfied and the mission state is acceptable
- FAIL when a required gate, claim, or public-surface standard is not met
- PARTIAL when verification is incomplete or when a handoff is needed

Respond in exactly this format:

VERDICT: PASS|FAIL|PARTIAL
MISSION_STATE: HEALTHY|UNHEALTHY|UNCLEAR
SUMMARY: <one sentence>
FAILED_GATES:
- <gate or "none">
OPEN_ITEMS:
- <item>
EVIDENCE:
- <check> => <result>
HANDOFF:
- <only when another review run should continue; otherwise write "none">
"#;

#[derive(Debug, Clone, Default)]
pub struct CompletionReviewRequest {
    pub preview: String,
    pub source_label: String,
    pub owner_visible: bool,
    pub conversation_id: i64,
    pub thread_key: String,
    pub workspace_root: String,
    pub runtime_db_path: String,
    pub review_skill_path: String,
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
    let review_prompt = build_review_prompt(request, &reasons);
    let report = (|| -> anyhow::Result<String> {
        let mut session = PersistentSession::start_with_instructions(
            root,
            &settings,
            Some(REVIEW_SYSTEM_PROMPT),
            true,
        )?;
        let result = session.run_turn(
            &review_prompt,
            Some(Duration::from_secs(REVIEW_TIMEOUT_SECS)),
            None,
            Some(false),
            0,
        );
        session.shutdown();
        result
    })();
    match report {
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
        "{}\n{}\n{}",
        request.preview, request.source_label, result_text
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

fn build_review_prompt(request: &CompletionReviewRequest, reasons: &[String]) -> String {
    let reason_block = if reasons.is_empty() {
        "none".to_string()
    } else {
        reasons.join(", ")
    };
    let source = request.source_label.as_str();
    let owner_visible = if request.owner_visible { "yes" } else { "no" };
    let thread_key = if request.thread_key.trim().is_empty() {
        "(none recorded)"
    } else {
        request.thread_key.trim()
    };
    let workspace_root = if request.workspace_root.trim().is_empty() {
        "(none recorded)"
    } else {
        request.workspace_root.trim()
    };
    let runtime_db_path = if request.runtime_db_path.trim().is_empty() {
        "(none recorded)"
    } else {
        request.runtime_db_path.trim()
    };
    let review_skill_path = if request.review_skill_path.trim().is_empty() {
        "(none recorded)"
    } else {
        request.review_skill_path.trim()
    };

    format!(
        "== REVIEW ASSIGNMENT ==\n\
\n\
Source label: {source}\n\
Owner visible: {owner_visible}\n\
Conversation id: {}\n\
Thread key: {thread_key}\n\
Workspace root: {workspace_root}\n\
Runtime DB: {runtime_db_path}\n\
Review skill: {review_skill_path}\n\
Trigger reasons: {reason_block}\n\
\n\
Open the review skill first and follow it.\n\
\n\
Gather the review facts yourself from the runtime store, continuity records, ticket system, communication state, workspace, runtime, live URLs, and browser surface.\n\
\n\
Required review work:\n\
1. discover the mission line and done gate for this conversation or thread\n\
2. discover the reviewed slice or latest claimed progress for this conversation or thread\n\
3. inspect related ticket/self-work/queue state\n\
4. inspect relevant founder or owner communication facts when owner-visible is yes\n\
5. inspect the live public/runtime surface when applicable\n\
6. produce a verdict from evidence\n\
\n\
Use the runtime DB path and workspace root above as the primary grounding points.\n\
\n\
Respond in exactly this shape:\n\
VERDICT: PASS|FAIL|PARTIAL\n\
MISSION_STATE: HEALTHY|UNHEALTHY|UNCLEAR\n\
SUMMARY: <one sentence>\n\
FAILED_GATES:\n\
- <gate or \"none\">\n\
OPEN_ITEMS:\n\
- <item>\n\
EVIDENCE:\n\
- <command or check> => <observed result>\n\
HANDOFF:\n\
- <only when another review run should continue; otherwise write \"none\">\n",
        request.conversation_id
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
            preview: "Install Redis".to_string(),
            source_label: "queue".to_string(),
            owner_visible: true,
            ..CompletionReviewRequest::default()
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
            preview: "Queue summary".to_string(),
            source_label: "tui".to_string(),
            owner_visible: true,
            ..CompletionReviewRequest::default()
        };
        let (required, _, _) = assess_review_requirement(
            &request,
            "Explained the current queue backlog and highlighted the blocked task.",
        );
        assert!(!required);
    }

    #[test]
    fn build_review_prompt_uses_metadata_only_and_points_to_skill() {
        let request = CompletionReviewRequest {
            preview: "v2.3 rollout".to_string(),
            source_label: "queue".to_string(),
            owner_visible: true,
            conversation_id: 42,
            thread_key: "kunstmen-bootstrap".to_string(),
            workspace_root: "/srv/kunstmen".to_string(),
            runtime_db_path: "/srv/runtime/ctox.sqlite3".to_string(),
            review_skill_path: "/srv/skills/system/review/external-review/SKILL.md".to_string(),
        };
        let rendered = build_review_prompt(&request, &["closure_claim".to_string()]);
        assert!(rendered.contains("== REVIEW ASSIGNMENT =="));
        assert!(rendered.contains("Conversation id: 42"));
        assert!(rendered.contains("/srv/skills/system/review/external-review/SKILL.md"));
        assert!(rendered.contains("Open the review skill first and follow it."));
        assert!(!rendered.contains("Latest reported result from the executor"));
        assert!(!rendered.contains("Focus snapshot"));
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
