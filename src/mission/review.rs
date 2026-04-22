use serde::Deserialize;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

use crate::execution::agent::direct_session::PersistentSession;
use crate::inference::runtime_env;

const REVIEW_TIMEOUT_SECS: u64 = 90;

const REVIEW_SYSTEM_PROMPT: &str = r#"You are CTOX's mission-state reviewer.

Your job is to stop CTOX from treating an under-verified execution slice or an unhealthy mission state as complete.

You are a SEPARATE AGENT from the executor that produced the slice. You did not write the work. You have no attachment to it. Your bias is skepticism, not endorsement.

Mission-state scope rule:
- Review the current mission state after the reported slice.
- The explicit done_gate still comes first, but you may FAIL when the public or owner-visible outcome is obviously not commercially credible, leaks internal instructions, exposes admin-only surfaces, or breaks the buyer path.
- Use the supplied mission, focus, anchors, narrative, workflow, and communication context to judge whether the slice moved the mission into a healthy state.
- If the slice scope is ambiguous, return PARTIAL with a one-line clarification request rather than guessing.

Verification discipline (strict read-only review mode):
- Do not modify project files.
- Do not run git write operations.
- Do not install packages or change system configuration.
- Prefer direct checks against the current repo, runtime, processes, logs, and tests over prose-only reasoning.
- If a claim can be verified with a command, run the command instead of restating the claim.

Done-gate-first discipline:
- If an explicit done_gate is provided in SCOPE, test that done_gate FIRST. If unmet, FAIL — regardless of other evidence.
- If no explicit done_gate is provided, derive the narrowest checkable claim from the slice prompt and test that, then evaluate whether the resulting mission state is still obviously unhealthy.

When the slice claims an install, rollout, migration, repair, or service readiness, inspect the live surface.
When the slice claims a code or config change is complete, inspect current workspace state and run the narrowest relevant checks.
When the work is owner-visible or public-facing, inspect the actual buyer-facing surface and the critical routes it depends on.
For public launch surfaces, the following are failures even if the page technically loads:
- internal prompt or instruction leakage
- public navigation that exposes admin or internal operations as primary buyer flow
- public pages that depend on broken critical API routes
- obviously placeholder, raw operator, or implementation text presented as user-facing copy
If evidence is incomplete or you cannot complete a check, return PARTIAL instead of PASS.

Respond in exactly this shape:
VERDICT: PASS|FAIL|PARTIAL
SUMMARY: <one sentence — must reference the specific done_gate or claim being judged>
OPEN_ITEMS:
- <item>
EVIDENCE:
- <command or check> => <observed result>
"#;

#[derive(Debug, Clone, Default)]
pub struct CompletionReviewRequest {
    pub goal: String,
    pub prompt: String,
    pub preview: String,
    pub source_label: String,
    pub owner_visible: bool,
    /// High-level mission line from MissionStateRecord (one-liner).
    /// Empty if no mission context is available.
    pub mission: String,
    /// Explicit done-gate from MissionStateRecord. The reviewer is
    /// instructed to test this FIRST before any other criterion.
    /// Empty if no done-gate has been set.
    pub done_gate: String,
    /// Focus continuity excerpt (current task focus / next slice / blocker).
    /// Already-clipped text; passed through verbatim into the review brief.
    pub focus_excerpt: String,
    /// Anchors continuity excerpt (key facts discovered during the mission).
    /// Already-clipped text; passed through verbatim into the review brief.
    pub anchors_excerpt: String,
    /// Narrative continuity excerpt (causal history / prior failures).
    pub narrative_excerpt: String,
    /// Workflow excerpt (open work / planning / queue state relevant to the slice).
    pub workflow_excerpt: String,
    /// Relevant communication excerpt (owner/founder/stakeholder thread context).
    pub communication_excerpt: String,
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
    let report = (|| -> anyhow::Result<String> {
        let mut session = PersistentSession::start(root, &settings)?;
        let result = session.run_turn(
            &review_prompt,
            Some(Duration::from_secs(REVIEW_TIMEOUT_SECS)),
            Some(REVIEW_SYSTEM_PROMPT),
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
    let mission_line = if request.mission.trim().is_empty() {
        "(no mission line on record)".to_string()
    } else {
        clip_text(request.mission.trim(), 240)
    };
    let done_gate_block = if request.done_gate.trim().is_empty() {
        "(none provided — derive the narrowest checkable claim from the slice prompt and test that)"
            .to_string()
    } else {
        request.done_gate.trim().to_string()
    };
    let focus_block = if request.focus_excerpt.trim().is_empty() {
        "(focus continuity is empty)".to_string()
    } else {
        request.focus_excerpt.trim().to_string()
    };
    let anchors_block = if request.anchors_excerpt.trim().is_empty() {
        "(no anchors recorded)".to_string()
    } else {
        request.anchors_excerpt.trim().to_string()
    };
    let narrative_block = if request.narrative_excerpt.trim().is_empty() {
        "(no narrative recorded)".to_string()
    } else {
        request.narrative_excerpt.trim().to_string()
    };
    let workflow_block = if request.workflow_excerpt.trim().is_empty() {
        "(no workflow state recorded)".to_string()
    } else {
        request.workflow_excerpt.trim().to_string()
    };
    let communication_block = if request.communication_excerpt.trim().is_empty() {
        "(no relevant communication recorded)".to_string()
    } else {
        request.communication_excerpt.trim().to_string()
    };
    let source = request.source_label.as_str();
    let owner_visible = if request.owner_visible { "yes" } else { "no" };
    let goal = request.goal.trim();
    let prompt = request.prompt.trim();
    let result = result_text.trim();

    format!(
        "==REVIEWER ROLE==\n\
You are a separate, skeptical CTOX mission-state reviewer. You did not produce the work below — you are reviewing it cold. Your bias is skepticism, not endorsement. Operate strictly read-only: do not modify files, do not run git write operations, do not install or restart services. Use shell/read tools and browser/read verification when the slice is a public web, deploy, landing-page, or owner-visible surface.\n\
\n\
==SCOPE (judge the mission state after the slice using the context below)==\n\
Mission: {mission_line}\n\
Source label: {source}\n\
Owner visible: {owner_visible}\n\
Trigger reasons: {reason_block}\n\
\n\
Explicit done_gate (test this FIRST):\n\
{done_gate_block}\n\
\n\
==CONTEXT (read-only, for grounding only — do not extend scope from this)==\n\
Focus snapshot:\n\
{focus_block}\n\
\n\
Anchors snapshot:\n\
{anchors_block}\n\
\n\
Narrative snapshot:\n\
{narrative_block}\n\
\n\
Workflow snapshot:\n\
{workflow_block}\n\
\n\
Relevant communication state:\n\
{communication_block}\n\
\n\
==WHAT THE EXECUTOR DID==\n\
Slice goal:\n\
{goal}\n\
\n\
Slice prompt the executor was given:\n\
{prompt}\n\
\n\
Latest reported result from the executor:\n\
{result}\n\
\n\
==REVIEW INSTRUCTIONS==\n\
1. Test the done_gate first if one is provided. If unmet, return FAIL.\n\
2. If the done_gate is missing or unclear, derive the narrowest checkable claim from the slice prompt and test that.\n\
3. Use direct evidence (shell/read tools) instead of prose-only reasoning.\n\
3a. If the slice is owner-visible and touches a public website, landing page, deploy, domain, or browser-facing flow, verify the live surface directly. Prefer an actual browser or at least direct HTTP checks of the public URL plus one critical route the page depends on.\n\
3b. A public page that returns HTML while its critical API route is 404/500 is FAIL, not PASS.\n\
3c. If a public page visibly leaks internal instructions, planning text, prompt material, admin-only UI, operator notes, or broken buyer flow, return FAIL.\n\
4. Use the mission, focus, anchors, narrative, workflow, and communication snapshots to judge whether this slice actually moved the mission into a healthier state. Do not bless a local success that leaves the public mission state obviously weak.\n\
5. If the scope is ambiguous or under-specified, return PARTIAL with a one-line clarification request — do not guess.\n\
6. If you cannot complete a check (timeout, missing artifact, permission), return PARTIAL — never PASS by default.\n\
\n\
Respond in exactly this shape:\n\
VERDICT: PASS|FAIL|PARTIAL\n\
SUMMARY: <one sentence — must reference the specific done_gate or claim being judged>\n\
OPEN_ITEMS:\n\
- <item>\n\
EVIDENCE:\n\
- <command or check> => <observed result>\n"
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
            goal: "Explain the current queue state".to_string(),
            prompt: "Summarize the queue status for the owner.".to_string(),
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
    fn build_review_prompt_includes_role_scope_and_done_gate_blocks() {
        let request = CompletionReviewRequest {
            goal: "Roll out v2.3".to_string(),
            prompt: "Deploy v2.3 to staging and run smoke tests.".to_string(),
            preview: "v2.3 rollout".to_string(),
            source_label: "queue".to_string(),
            owner_visible: true,
            mission: "Stabilize staging deploys".to_string(),
            done_gate: "curl -f https://staging/health returns 200".to_string(),
            focus_excerpt: "Active task: deploy v2.3".to_string(),
            anchors_excerpt: "Repo: /opt/api".to_string(),
            narrative_excerpt: "Turn 1 failed on staging health.".to_string(),
            workflow_excerpt: "Open slice: verify deploy and smoke tests.".to_string(),
            communication_excerpt: "Owner asked whether staging is actually healthy.".to_string(),
        };
        let rendered = build_review_prompt(
            &request,
            "Smoke test passed.",
            &["closure_claim".to_string()],
        );
        assert!(rendered.contains("==REVIEWER ROLE=="));
        assert!(rendered.contains("==SCOPE"));
        assert!(rendered.contains("Explicit done_gate"));
        assert!(rendered.contains("curl -f https://staging/health"));
        assert!(rendered.contains("Stabilize staging deploys"));
        assert!(rendered.contains("==WHAT THE EXECUTOR DID=="));
        assert!(rendered.contains("Smoke test passed."));
        assert!(rendered.contains("Relevant communication state"));
        assert!(rendered.contains("Open slice: verify deploy and smoke tests."));
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
