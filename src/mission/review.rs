use serde::Deserialize;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

use crate::execution::agent::direct_session::PersistentSession;
use crate::inference::runtime_env;

const REVIEW_TIMEOUT_SECS: u64 = 300;
const REVIEW_MAX_LEGS: usize = 3;

const REVIEW_SYSTEM_PROMPT: &str = r#"You are CTOX Review.

You run an external verification pass for one reviewed slice.

Use the review assignment as the only task definition.
Gather everything else yourself through read-only inspection of the workspace, runtime store, tickets, communication state, live services, logs, browser surface, and other available tools.

Operate in strict read-only verification mode.
Use the same inspection tools as normal CTOX work.
Use multiple tool turns as needed.

Verification standard:
- active vision and active mission are the primary strategic context
- explicit done gates
- reviewed claims
- resulting mission state
- public-surface quality for owner-visible or public work
- commercial credibility and buyer-path integrity for launch work
- SQLite-backed runtime evidence over ad hoc workspace notes or standalone markdown artifacts

Knowledge evidence rule:
- treat runtime SQLite records as canonical durable knowledge
- continuity commits, ticket knowledge, plan state, local ticket state, verification records, communication records, and other runtime DB facts count as durable knowledge
- standalone workspace files may support a claim, but they do not count as durable mission knowledge unless the same insight is also persisted in SQLite-backed runtime state

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
FINDINGS:
- <semantic finding or "none">
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
    pub artifact_text: String,
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

    pub fn as_report_label(&self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Partial => "PARTIAL",
            Self::Skipped => "SKIPPED",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewOutcome {
    pub required: bool,
    pub verdict: ReviewVerdict,
    pub mission_state: String,
    pub summary: String,
    pub report: String,
    pub score: u8,
    pub reasons: Vec<String>,
    pub failed_gates: Vec<String>,
    pub semantic_findings: Vec<String>,
    pub open_items: Vec<String>,
    pub evidence: Vec<String>,
    pub handoff: Option<String>,
}

impl ReviewOutcome {
    pub fn skipped(summary: impl Into<String>) -> Self {
        Self {
            required: false,
            verdict: ReviewVerdict::Skipped,
            mission_state: "UNCLEAR".to_string(),
            summary: summary.into(),
            report: String::new(),
            score: 0,
            reasons: Vec::new(),
            failed_gates: Vec::new(),
            semantic_findings: Vec::new(),
            open_items: Vec::new(),
            evidence: Vec::new(),
            handoff: None,
        }
    }

    pub fn requires_follow_up(&self) -> bool {
        self.required && matches!(self.verdict, ReviewVerdict::Fail)
    }

    pub fn canonical_report(&self) -> String {
        if !self.report.trim().is_empty() {
            return self.report.trim().to_string();
        }

        let mut rendered = Vec::new();
        rendered.push(format!("VERDICT: {}", self.verdict.as_report_label()));
        rendered.push(format!("MISSION_STATE: {}", self.mission_state.trim()));
        rendered.push(format!("SUMMARY: {}", self.summary.trim()));
        append_report_section(&mut rendered, "FAILED_GATES", &self.failed_gates);
        append_report_section(&mut rendered, "FINDINGS", &self.semantic_findings);
        append_report_section(&mut rendered, "OPEN_ITEMS", &self.open_items);
        append_report_section(&mut rendered, "EVIDENCE", &self.evidence);
        rendered.push("HANDOFF:".to_string());
        match self.handoff.as_deref() {
            Some(handoff) if !handoff.trim().is_empty() => {
                for line in handoff.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        rendered.push(format!("- {trimmed}"));
                    }
                }
            }
            _ => rendered.push("- none".to_string()),
        }
        rendered.join("\n")
    }
}

fn append_report_section(rendered: &mut Vec<String>, header: &str, items: &[String]) {
    rendered.push(format!("{header}:"));
    if items.is_empty() {
        rendered.push("- none".to_string());
        return;
    }
    for item in items {
        let trimmed = item.trim();
        if !trimmed.is_empty() {
            rendered.push(format!("- {trimmed}"));
        }
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
    let report = run_external_review_legs(root, request, &settings, &reasons);
    match report {
        Ok(report) => parse_review_report(score, reasons, &report),
        Err(err) => ReviewOutcome {
            required: true,
            verdict: ReviewVerdict::Unavailable,
            mission_state: "UNCLEAR".to_string(),
            summary: format!(
                "Completion review could not finish: {}",
                clip_text(&err.to_string(), 180)
            ),
            report: err.to_string(),
            score,
            reasons,
            failed_gates: Vec::new(),
            semantic_findings: Vec::new(),
            open_items: Vec::new(),
            evidence: Vec::new(),
            handoff: None,
        },
    }
}

fn run_external_review_legs(
    root: &Path,
    request: &CompletionReviewRequest,
    settings: &std::collections::BTreeMap<String, String>,
    reasons: &[String],
) -> anyhow::Result<String> {
    let mut prompt = build_review_prompt(request, reasons);
    let mut last_report = String::new();

    for leg in 0..REVIEW_MAX_LEGS {
        let mut session = PersistentSession::start_with_instructions(
            root,
            settings,
            Some(REVIEW_SYSTEM_PROMPT),
            true,
        )?;
        let report = session.run_turn(
            &prompt,
            Some(Duration::from_secs(REVIEW_TIMEOUT_SECS)),
            None,
            Some(false),
            0,
        )?;
        session.shutdown();

        let verdict = parse_verdict(&report);
        let handoff = parse_handoff_block(&report);
        last_report = report;

        if !matches!(verdict, Some(ReviewVerdict::Partial)) {
            break;
        }
        let Some(handoff) = handoff else {
            break;
        };
        if leg + 1 >= REVIEW_MAX_LEGS {
            break;
        }
        prompt = build_review_handoff_prompt(request, &handoff);
    }

    Ok(last_report)
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

    let founder_or_owner_email = matches!(
        request.source_label.to_ascii_lowercase().as_str(),
        "email:owner" | "email:founder" | "email:admin"
    );
    if founder_or_owner_email {
        score = score.saturating_add(3);
        push_unique_reason(&mut reasons, "founder_communication");
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
    let artifact_kind = match request.source_label.to_ascii_lowercase().as_str() {
        "email:owner" | "email:founder" | "email:admin" => "founder_or_owner_outbound_email_draft",
        _ => "reviewed_output_artifact",
    };
    let artifact_text = if request.artifact_text.trim().is_empty() {
        "(empty artifact)"
    } else {
        request.artifact_text.trim()
    };
    let founder_specific_work = if matches!(
        request.source_label.to_ascii_lowercase().as_str(),
        "email:owner" | "email:founder" | "email:admin"
    ) {
        "\
Founder/owner communication gate:\n\
- judge the outbound draft itself as the artifact under review\n\
- decide whether the draft should be sent now, blocked, or reworked first\n\
- fail the review when the draft does not answer the latest founder mail, dodges the requested deliverable, promises future work instead of delivering, or leaks internal/system language\n\
- do not fail only because the broader mission is still open; fail only when the draft makes a false claim, omits a required answer, or the missing deliverable means the mail should not be sent yet\n\
"
    } else {
        ""
    };

    format!(
        "== REVIEW ASSIGNMENT ==\n\
\n\
Source label: {source}\n\
Artifact kind: {artifact_kind}\n\
Owner visible: {owner_visible}\n\
Conversation id: {}\n\
Thread key: {thread_key}\n\
Workspace root: {workspace_root}\n\
Runtime DB: {runtime_db_path}\n\
Review skill: {review_skill_path}\n\
Trigger reasons: {reason_block}\n\
\n\
Artifact under review:\n\
--- BEGIN ARTIFACT ---\n\
{artifact_text}\n\
--- END ARTIFACT ---\n\
\n\
Open the review skill first and follow it.\n\
\n\
Gather the review facts yourself from the runtime store, continuity records, ticket system, communication state, workspace, runtime, live URLs, and browser surface.\n\
\n\
Required review work:\n\
1. load the active strategic directives for this conversation or thread from runtime SQLite state first\n\
2. treat active vision and active mission as the primary review context\n\
3. discover the done gate and the reviewed slice or latest claimed progress for this conversation or thread\n\
4. inspect related ticket/self-work/queue state\n\
5. inspect relevant founder or owner communication facts when owner-visible is yes\n\
6. inspect the live public/runtime surface when applicable\n\
7. decide whether this specific artifact is ready to send/release/close, or whether real rework is required first\n\
8. produce a verdict from evidence\n\
\n\
Use the runtime DB path and workspace root above as the primary grounding points.\n\
\n\
{founder_specific_work}\
\n\
Helpful runtime entrypoint:\n\
- use `ctox strategy show --conversation-id {}` and `ctox verification runs --conversation-id {}` as starting lookups, then continue with direct SQLite/runtime/browser inspection\n\
\n\
If active vision or active mission is missing for strategic or owner-visible work, that is a review failure unless the slice itself is explicitly establishing them.\n\
\n\
Respond in exactly this shape:\n\
VERDICT: PASS|FAIL|PARTIAL\n\
MISSION_STATE: HEALTHY|UNHEALTHY|UNCLEAR\n\
SUMMARY: <one sentence>\n\
FAILED_GATES:\n\
- <gate or \"none\">\n\
FINDINGS:\n\
- <semantic finding or \"none\">\n\
OPEN_ITEMS:\n\
- <item>\n\
EVIDENCE:\n\
- <command or check> => <observed result>\n\
HANDOFF:\n\
- <only when another review run should continue; otherwise write \"none\">\n",
        request.conversation_id,
        request.conversation_id,
        request.conversation_id
    )
}

fn build_review_handoff_prompt(request: &CompletionReviewRequest, handoff: &str) -> String {
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
        "== REVIEW CONTINUATION ==\n\
\n\
Conversation id: {}\n\
Thread key: {thread_key}\n\
Workspace root: {workspace_root}\n\
Runtime DB: {runtime_db_path}\n\
Review skill: {review_skill_path}\n\
\n\
Open the review skill first and continue from this review handoff.\n\
\n\
Prior handoff:\n\
{}\n\
\n\
Continue the remaining verification work and return the standard review format.\n",
        request.conversation_id,
        handoff.trim()
    )
}

fn parse_review_report(score: u8, reasons: Vec<String>, report: &str) -> ReviewOutcome {
    let parsed_verdict = parse_verdict(report);
    let verdict = parsed_verdict.clone().unwrap_or(ReviewVerdict::Partial);
    let mission_state = parse_prefixed_line(report, "MISSION_STATE:")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "UNCLEAR".to_string());
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
        mission_state,
        summary,
        report: report.trim().to_string(),
        score,
        reasons,
        failed_gates: parse_section_items(report, "FAILED_GATES:"),
        semantic_findings: parse_section_items(report, "FINDINGS:"),
        open_items: parse_section_items(report, "OPEN_ITEMS:"),
        evidence: parse_section_items(report, "EVIDENCE:"),
        handoff: parse_handoff_block(report),
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

fn parse_handoff_block(report: &str) -> Option<String> {
    let mut collecting = false;
    let mut lines = Vec::new();
    for line in report.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("HANDOFF:") {
            collecting = true;
            let remainder = rest.trim();
            if !remainder.is_empty() {
                lines.push(remainder.to_string());
            }
            continue;
        }
        if collecting {
            if trimmed.starts_with("VERDICT:")
                || trimmed.starts_with("MISSION_STATE:")
                || trimmed.starts_with("SUMMARY:")
                || trimmed.starts_with("FAILED_GATES:")
                || trimmed.starts_with("OPEN_ITEMS:")
                || trimmed.starts_with("EVIDENCE:")
            {
                break;
            }
            if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
        }
    }
    let joined = lines.join("\n");
    let normalized = joined.trim();
    if normalized.is_empty()
        || normalized.eq_ignore_ascii_case("none")
        || normalized.eq_ignore_ascii_case("- none")
    {
        None
    } else {
        Some(normalized.to_string())
    }
}

fn parse_section_items(report: &str, header: &str) -> Vec<String> {
    let mut collecting = false;
    let mut items = Vec::new();
    for line in report.lines() {
        let trimmed = line.trim();
        if trimmed == header {
            collecting = true;
            continue;
        }
        if collecting {
            if matches!(
                trimmed,
                "VERDICT:"
                    | "MISSION_STATE:"
                    | "SUMMARY:"
                    | "FAILED_GATES:"
                    | "FINDINGS:"
                    | "OPEN_ITEMS:"
                    | "EVIDENCE:"
                    | "HANDOFF:"
            ) {
                break;
            }
            if let Some(item) = trimmed.strip_prefix("- ") {
                let value = item.trim();
                if !value.is_empty() && !value.eq_ignore_ascii_case("none") {
                    items.push(value.to_string());
                }
            }
        }
    }
    items
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
    fn requires_review_for_founder_email_even_without_runtime_change() {
        let request = CompletionReviewRequest {
            preview: "[E-Mail eingegangen] Sender: Founder".to_string(),
            source_label: "email:owner".to_string(),
            owner_visible: true,
            ..CompletionReviewRequest::default()
        };
        let (required, score, reasons) = assess_review_requirement(
            &request,
            "Kurzstand: Ich habe die 5 Mockups vorliegen und schicke dir als Nächstes die Auswahl.",
        );
        assert!(required);
        assert!(score >= 3);
        assert!(reasons
            .iter()
            .any(|reason| reason == "founder_communication"));
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
            artifact_text: "Patched rollout artifact".to_string(),
        };
        let rendered = build_review_prompt(&request, &["closure_claim".to_string()]);
        assert!(rendered.contains("== REVIEW ASSIGNMENT =="));
        assert!(rendered.contains("Conversation id: 42"));
        assert!(rendered.contains("/srv/skills/system/review/external-review/SKILL.md"));
        assert!(rendered.contains("Open the review skill first and follow it."));
        assert!(rendered.contains("Artifact under review:"));
        assert!(rendered.contains("Patched rollout artifact"));
    }

    #[test]
    fn founder_review_prompt_explicitly_reviews_the_mail_artifact() {
        let request = CompletionReviewRequest {
            preview: "[E-Mail eingegangen] Sender: Founder".to_string(),
            source_label: "email:owner".to_string(),
            owner_visible: true,
            conversation_id: 77,
            thread_key: "email-review:owner:abc".to_string(),
            workspace_root: "/srv/kunstmen".to_string(),
            runtime_db_path: "/srv/runtime/ctox.sqlite3".to_string(),
            review_skill_path: "/srv/skills/system/review/external-review/SKILL.md".to_string(),
            artifact_text: "Kurzstand: Ich liefere spaeter.".to_string(),
        };
        let rendered = build_review_prompt(&request, &["founder_communication".to_string()]);
        assert!(rendered.contains("Artifact kind: founder_or_owner_outbound_email_draft"));
        assert!(rendered.contains("judge the outbound draft itself as the artifact under review"));
        assert!(rendered.contains("does not answer the latest founder mail"));
        assert!(rendered.contains("Kurzstand: Ich liefere spaeter."));
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

    #[test]
    fn parse_handoff_block_extracts_multiline_body() {
        let report = "VERDICT: PARTIAL\nMISSION_STATE: UNCLEAR\nSUMMARY: Need more checks.\nHANDOFF:\n- Verified public URL returns 200\n- Still need /api/state and buyer path\n";
        let handoff = parse_handoff_block(report).expect("handoff missing");
        assert!(handoff.contains("Verified public URL returns 200"));
        assert!(handoff.contains("Still need /api/state"));
    }
}
