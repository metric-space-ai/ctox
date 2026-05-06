use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use crate::execution::agent::direct_session::PersistentSession;
use crate::inference::runtime_env;

const REVIEW_TIMEOUT_SECS: u64 = 300;
const REVIEW_MAX_LEGS: usize = 3;

const REVIEW_SYSTEM_PROMPT: &str = r#"You are CTOX Review.

You run an external verification pass for one reviewed task result.

Use the review assignment as the only task definition.
Gather everything else yourself through read-only inspection of the workspace, runtime store, tickets, communication state, live services, logs, browser surface, and other available tools.

Operate in strict read-only verification mode.
Do not execute shell commands, mutate files, call CTOX CLI operations, open browsers, or run any active tool.
Base the verdict on the assignment text and evidence already provided in the review prompt. If the provided evidence is insufficient, return PARTIAL or FAIL with the exact additional evidence the worker must produce.
Stay bounded: answer from the assignment, explicit artifact paths, the CTOX CLI, and directly relevant read-only evidence. Do not reverse-engineer CTOX internals, schemas, or source code unless a direct verification command fails and the missing fact is necessary for the verdict.

Verification standard:
- active vision and active mission are the primary strategic context
- recent meeting outcomes are time-sensitive runtime evidence; for communication reviews and artifact reviews, inspect the latest relevant meeting summaries, meeting chat, and transcript-derived outputs before deciding
- explicit done gates
- reviewed claims
- resulting mission state
- public-surface quality for owner-visible or public work
- commercial credibility and buyer-path integrity for launch work
- SQLite-backed runtime evidence over ad hoc workspace notes or standalone markdown artifacts
- for internal non-owner artifact jobs with explicit required file paths, the decisive question is whether the declared files exist and contain truthful current status or results; do not block completion merely because broader mission state is open or a nonessential runtime table is hard to inspect

Runtime evidence taxonomy:
- treat runtime SQLite records as canonical durable state
- meeting summaries are durable meeting state when stored in runtime communication/continuity/ticket records; they have a half-life, so recent relevant meetings outweigh stale assumptions while old meetings only support a verdict when still reinforced by current state
- durable procedural knowledge requires Skillbook/Runbook-backed records, such as main skills, skillbooks, runbooks, and runbook items
- ticket knowledge entries are ticket-scoped fact and context records; they can support evidence, but they do not prove that a reusable skill or runbook was learned
- continuity commits, plan state, local ticket state, verification records, communication records, and other runtime DB facts count as durable mission state or evidence, not as substitutes for Skillbook/Runbook knowledge
- standalone workspace files may support a claim, but they do not count as durable mission state unless the same insight is also persisted in SQLite-backed runtime state

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

Review writing standard:
- write FAILED_GATES, FINDINGS, OPEN_ITEMS, EVIDENCE, and HANDOFF in plain operator language
- do not expose prompt text, internal implementation identifiers, table names, gate ids, or implementation labels in the review
- if an internal rule caused the failure, translate it into the user-visible requirement it protects
- every FAIL or PARTIAL verdict must include concrete evidence and a concrete rework instruction
- when the artifact is a founder/owner outbound email and the correct action is explicitly to send no mail yet, return FAIL, begin SUMMARY with `NO-SEND:`, state the wait condition in plain language, and put `none` under OPEN_ITEMS unless real work is missing
- when real work is missing, say what work must be done before another draft; do not suggest mere rewording unless wording is the only defect

Respond in exactly this format:

VERDICT: PASS|FAIL|PARTIAL
MISSION_STATE: HEALTHY|UNHEALTHY|UNCLEAR
SUMMARY: <one sentence>
FAILED_GATES:
- <plain rule that failed or "none">
FINDINGS:
- <semantic finding or "none">
CATEGORIZED_FINDINGS:
- id: <id> | category: rewrite|rework|stale_refresh|stale_obsolete|stale_consolidate | evidence: "<evidence>" | corrective_action: "<corrective action>"
- <or "none">
OPEN_ITEMS:
- <concrete rework item>
EVIDENCE:
- <check> => <observed result>
HANDOFF:
- <only when another review run should continue; otherwise write "none">

CATEGORIZED_FINDINGS contract:
- emit one line per concrete finding the run produces
- each line is pipe-delimited key:value pairs in the order id | category | evidence | corrective_action
- category is the structural enum the dispatcher routes on; the skill teaches the rules
- if there is no concrete finding, write a single "- none" line under the section
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
    pub artifact_action: Option<String>,
    pub artifact_to: Vec<String>,
    pub artifact_cc: Vec<String>,
    pub artifact_attachments: Vec<String>,
    pub required_deliverables: Vec<String>,
    pub artifact_commitments: Vec<String>,
    pub commitment_backing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReviewVerdict {
    Pass,
    Fail,
    Partial,
    Skipped,
    Unavailable,
}

/// Structured terminal disposition emitted by the reviewer.
///
/// `Send` is the default for any FAIL verdict that should drive a rewrite
/// or rework loop. `NoSend` is the explicit terminal "do not send anything,
/// the task is closed" signal, set by the reviewer in the structured
/// `DISPOSITION:` block. The dispatcher reads this enum directly — no
/// keyword scraping on summary or evidence text in the service core.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReviewDisposition {
    Send,
    NoSend,
}

impl Default for ReviewDisposition {
    fn default() -> Self {
        Self::Send
    }
}

impl ReviewDisposition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Send => "SEND",
            Self::NoSend => "NO_SEND",
        }
    }

    /// Parse a single disposition token. Unknown tokens fall through to the
    /// caller, which defaults to `Send` to preserve the existing rewrite /
    /// rework loop behaviour.
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_uppercase().as_str() {
            "SEND" => Some(Self::Send),
            "NO_SEND" | "NO-SEND" | "NOSEND" => Some(Self::NoSend),
            _ => None,
        }
    }
}

/// Structural category emitted by the reviewer for every concrete finding.
///
/// `Rewrite` means the task can be repaired by editing the prior outbound
/// body without mutating durable state. `Rework` means the finding requires
/// a substantive change (durable record, fresh research, structural artefact)
/// and must run on the heavy review-rework path. `Stale*` findings mean the
/// world changed underneath the draft and the queue/thread state must be
/// refreshed, obsoleted, or consolidated before any send/closure.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FindingCategory {
    Rewrite,
    Rework,
    StaleRefresh,
    StaleObsolete,
    StaleConsolidate,
}

impl FindingCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rewrite => "rewrite",
            Self::Rework => "rework",
            Self::StaleRefresh => "stale_refresh",
            Self::StaleObsolete => "stale_obsolete",
            Self::StaleConsolidate => "stale_consolidate",
        }
    }

    /// Parse a single category token. Returns `None` for unknown values; the
    /// caller decides the legacy fallback (the dispatcher defaults to
    /// `Rework` to preserve safety).
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "rewrite" => Some(Self::Rewrite),
            "rework" => Some(Self::Rework),
            "stale_refresh" | "stale-refresh" => Some(Self::StaleRefresh),
            "stale_obsolete" | "stale-obsolete" => Some(Self::StaleObsolete),
            "stale_consolidate" | "stale-consolidate" => Some(Self::StaleConsolidate),
            _ => None,
        }
    }

    pub fn is_stale(self) -> bool {
        matches!(
            self,
            Self::StaleRefresh | Self::StaleObsolete | Self::StaleConsolidate
        )
    }
}

/// Structured reviewer finding paired with its category.
///
/// Carries the deterministic metadata the dispatcher consumes to choose
/// between the lightweight rewrite path and the heavy rework loop. Plain
/// string findings (the legacy `semantic_findings` list) stay unchanged for
/// backward compatibility and operator surfaces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CategorizedFinding {
    pub id: String,
    pub category: FindingCategory,
    pub evidence: String,
    pub corrective_action: String,
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
    /// Structured per-finding entries with deterministic `category` tags.
    /// Populated when the reviewer emits a `CATEGORIZED_FINDINGS:` block.
    /// Empty for legacy reports — callers must not infer category from the
    /// plain `semantic_findings` strings.
    #[serde(default)]
    pub categorized_findings: Vec<CategorizedFinding>,
    pub open_items: Vec<String>,
    pub evidence: Vec<String>,
    pub handoff: Option<String>,
    /// Structured terminal disposition. `Send` (default) means the dispatcher
    /// should run the rewrite/rework loop on a FAIL verdict; `NoSend` means
    /// the task is closed without further outbound. The reviewer sets this
    /// via the `DISPOSITION:` block — the service core never scrapes summary
    /// or finding text to derive it.
    #[serde(default)]
    pub disposition: ReviewDisposition,
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
            categorized_findings: Vec::new(),
            open_items: Vec::new(),
            evidence: Vec::new(),
            handoff: None,
            disposition: ReviewDisposition::Send,
        }
    }

    pub fn requires_follow_up(&self) -> bool {
        self.required && matches!(self.verdict, ReviewVerdict::Fail | ReviewVerdict::Partial)
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
        rendered.push("CATEGORIZED_FINDINGS:".to_string());
        if self.categorized_findings.is_empty() {
            rendered.push("- none".to_string());
        } else {
            for finding in &self.categorized_findings {
                rendered.push(format!(
                    "- id: {} | category: {} | evidence: {} | corrective_action: {}",
                    finding.id,
                    finding.category.as_str(),
                    finding.evidence,
                    finding.corrective_action,
                ));
            }
        }
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
        rendered.push(format!("DISPOSITION: {}", self.disposition.as_str()));
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
        return ReviewOutcome::skipped("Completion review gate not triggered for this task.");
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
            categorized_findings: Vec::new(),
            open_items: Vec::new(),
            evidence: Vec::new(),
            handoff: None,
            disposition: ReviewDisposition::Send,
        },
    }
}

fn run_external_review_legs(
    root: &Path,
    request: &CompletionReviewRequest,
    settings: &BTreeMap<String, String>,
    reasons: &[String],
) -> anyhow::Result<String> {
    let mut prompt = build_review_prompt(request, reasons);
    let mut last_report = String::new();

    for leg in 0..REVIEW_MAX_LEGS {
        let report = run_external_review_leg_with_wall_timeout(
            root,
            request,
            settings,
            &prompt,
            leg,
            Duration::from_secs(REVIEW_TIMEOUT_SECS),
        )?;

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

fn run_external_review_leg_with_wall_timeout(
    root: &Path,
    request: &CompletionReviewRequest,
    settings: &BTreeMap<String, String>,
    prompt: &str,
    leg: usize,
    timeout: Duration,
) -> anyhow::Result<String> {
    let mut session = PersistentSession::start_with_instructions(
        root,
        settings,
        Some(REVIEW_SYSTEM_PROMPT),
        true,
    )
    .map_err(|err| {
        anyhow::anyhow!(
            "completion review leg {} could not start for {}: {}",
            leg + 1,
            clip_text(&request.preview, 120),
            err
        )
    })?;
    let report = session.run_turn(prompt, Some(timeout), None, Some(false), 0);
    session.shutdown();
    report.map_err(|err| {
        anyhow::anyhow!(
            "completion review leg {} did not produce a verdict within {}s for {}: {}",
            leg + 1,
            timeout.as_secs(),
            clip_text(&request.preview, 120),
            err
        )
    })
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
    let founder_or_owner_email = matches!(
        request.source_label.to_ascii_lowercase().as_str(),
        "email:owner" | "email:founder" | "email:admin"
    ) || request
        .artifact_action
        .as_deref()
        .map(|value| value.to_ascii_lowercase().contains("founder"))
        .unwrap_or(false);
    let internal_artifact_slice = !founder_or_owner_email
        && !request.owner_visible
        && request.artifact_action.is_none()
        && contains_any(
            &lowered,
            &[
                "required artifact",
                "required artifacts",
                "required file",
                "required files",
                "durable artifact",
                "durable file",
                "initialisiere die datei",
                "create and verify the smoke artifact",
            ],
        );
    if internal_artifact_slice {
        push_unique_reason(&mut reasons, "internal_artifact_slice");
    }
    let internal_smoke_artifact_slice = internal_artifact_slice
        && contains_any(
            &lowered,
            &["smoke", "qwen36-local", "response adapter", "local backend"],
        );

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
        if internal_smoke_artifact_slice {
            push_unique_reason(&mut reasons, "smoke_artifact_witnessed");
        } else {
            score = score.saturating_add(1);
            push_unique_reason(&mut reasons, "closure_claim");
        }
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
            "runtime failure",
            "without assistant message",
            "api is still unavailable",
        ],
    );
    if runtime_or_infra_change {
        if internal_smoke_artifact_slice {
            push_unique_reason(&mut reasons, "smoke_runtime_feedback_ignored");
        } else {
            score = score.saturating_add(2);
            push_unique_reason(&mut reasons, "runtime_or_infra_change");
        }
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
        if internal_smoke_artifact_slice {
            push_unique_reason(&mut reasons, "smoke_contract_feedback_ignored");
        } else {
            score = score.saturating_add(1);
            push_unique_reason(&mut reasons, "code_or_artifact_change");
        }
    }

    if combined.chars().count() > 900 {
        if internal_artifact_slice {
            // Internal file-artifact slices are guarded by the outcome
            // witness; length alone must not send them into external review.
        } else {
            score = score.saturating_add(1);
            push_unique_reason(&mut reasons, "long_complex_slice");
        }
    }

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
    let founder_artifact = matches!(
        request.source_label.to_ascii_lowercase().as_str(),
        "email:owner" | "email:founder" | "email:admin"
    ) || request
        .artifact_action
        .as_deref()
        .map(|value| value.to_ascii_lowercase().contains("founder"))
        .unwrap_or(false);
    let artifact_kind = if founder_artifact {
        "founder_or_owner_outbound_email_draft"
    } else {
        "reviewed_output_artifact"
    };
    let artifact_text = if request.artifact_text.trim().is_empty() {
        "(empty artifact)"
    } else {
        request.artifact_text.trim()
    };
    let artifact_action = request
        .artifact_action
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("(none recorded)");
    let artifact_to = if request.artifact_to.is_empty() {
        "(none recorded)".to_string()
    } else {
        request.artifact_to.join(", ")
    };
    let artifact_cc = if request.artifact_cc.is_empty() {
        "(none recorded)".to_string()
    } else {
        request.artifact_cc.join(", ")
    };
    let artifact_attachments = if request.artifact_attachments.is_empty() {
        "(none recorded)".to_string()
    } else {
        request.artifact_attachments.join(", ")
    };
    let required_deliverables = if request.required_deliverables.is_empty() {
        "(none recorded)".to_string()
    } else {
        request.required_deliverables.join(", ")
    };
    let artifact_commitments = if request.artifact_commitments.is_empty() {
        "(none recorded)".to_string()
    } else {
        request.artifact_commitments.join(" | ")
    };
    let commitment_backing = if request.commitment_backing.is_empty() {
        "(none recorded)".to_string()
    } else {
        request.commitment_backing.join(" | ")
    };
    let founder_specific_work = if founder_artifact {
        "\
Founder/owner communication gate:\n\
- judge the outbound draft itself as the artifact under review\n\
- judge the full mail action, not just the prose: recipients, cc list, and reply/forward behavior are part of the artifact\n\
- decide whether the draft should be sent now, blocked, or reworked first\n\
- if the correct outcome is no outbound mail yet because the thread is waiting on specific founder input, return FAIL, begin SUMMARY with `NO-SEND:`, and state the wait condition; do not invent rework\n\
- treat every listed required deliverable as mandatory; if a required deliverable is missing, the mail must fail review and be reworked first\n\
- treat every listed future promise, dated commitment, or deadline promise as mandatory review context; if a promise is not backed by a concrete CTOX schedule or open follow-up, the mail must fail review and be reworked first\n\
- inspect recent relevant meeting outcomes before judging the draft; if the latest meeting changed decisions, blockers, commitments, names, recipients, or proof expectations, the draft must reflect that newer context\n\
- compare the draft against observed runtime facts and completed work; fail the review when the mail contradicts verified actions, hides completed verification, or says work still needs to be set up after it was already installed/tested\n\
- when a founder explicitly requests setup, access, credentials, links, or next operational steps, require the draft to state the exact verified state and the exact remaining blocker or access path; vague option lists are not enough\n\
- fail the review when the draft does not answer the latest founder mail, dodges the requested deliverable, promises future work instead of delivering, or leaks internal/system language\n\
- fail the review when the recipients or cc list are wrong, when sender-only reply is incorrect, or when a forwarded/delegated founder mail should target different recipients\n\
- do not fail only because the broader mission is still open; fail only when the draft makes a false claim, omits a required answer, or the missing deliverable means the mail should not be sent yet\n\
- explain communication failures in plain recipient-facing terms, with evidence from the current thread; do not mention internal gate names or prompt rules\n\
"
    } else {
        ""
    };

    let artifact_review_work = if founder_artifact {
        ""
    } else {
        "\
Internal artifact review gate:\n\
- if the assignment lists explicit durable file paths, inspect those paths first with read-only shell checks such as `test -f`, `wc -c`, `head`, `tail`, `jq`, or `sqlite3` only when the file itself is a SQLite database\n\
- PASS when the explicit artifact contract is satisfied and the artifact content truthfully records current status, evidence, or next action\n\
- FAIL when a required artifact is missing, is a directory instead of a file, is empty when it must carry status, or contradicts verified runtime/workspace evidence\n\
- PARTIAL only when a specific required check cannot be completed within the review budget; include the exact remaining check in HANDOFF\n\
- do not inspect CTOX source code or infer private table schemas for ordinary file-artifact review unless the artifact itself points there as the necessary evidence\n\
"
    };

    format!(
        "== REVIEW ASSIGNMENT ==\n\
\n\
Source label: {source}\n\
Artifact kind: {artifact_kind}\n\
Owner visible: {owner_visible}\n\
Conversation id: {conversation_id}\n\
Thread key: {thread_key}\n\
Workspace root: {workspace_root}\n\
Runtime DB: {runtime_db_path}\n\
Review skill: {review_skill_path}\n\
Trigger reasons: {reason_block}\n\
\n\
Artifact under review:\n\
Artifact action: {artifact_action}\n\
Artifact to: {artifact_to}\n\
Artifact cc: {artifact_cc}\n\
Artifact attachments: {artifact_attachments}\n\
Required deliverables: {required_deliverables}\n\
Artifact commitments: {artifact_commitments}\n\
Commitment backing: {commitment_backing}\n\
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
3. discover the done gate and the reviewed task result or latest claimed progress for this conversation or thread\n\
4. inspect related ticket/self-work/queue state\n\
5. inspect recent relevant meeting outcomes before communication or artifact verdicts; prioritize meeting summaries from the last 7 days, treat 30-day-old meeting outcomes as supporting context, and ignore stale meeting notes when contradicted by newer runtime state\n\
6. inspect relevant founder or owner communication facts when owner-visible is yes\n\
7. inspect the live public/runtime surface when applicable\n\
8. decide whether this specific artifact is ready to send/release/close, or whether real rework is required first\n\
9. produce a verdict from evidence\n\
\n\
Use the runtime DB path and workspace root above as the primary grounding points.\n\
\n\
{founder_specific_work}\
{artifact_review_work}\
\n\
Helpful runtime entrypoint:\n\
- use `ctox strategy show --conversation-id {strategy_conversation_id}` and `ctox verification runs --conversation-id {verification_conversation_id}` as starting lookups, then continue with direct SQLite/runtime/browser inspection\n\
- for recent meeting outcomes, query `communication_messages` for `channel='meeting'` and bodies or subjects containing `Meeting Summary`, then inspect same-thread entries first and recent cross-thread entries when they mention the reviewed artifact, system, recipient, or deliverable\n\
- if a suggested SQLite query fails because a column or table is absent, do not spend the review budget on schema exploration unless that exact fact is decisive; use `PRAGMA table_info(<table>)` once, adapt the query, or record the missing evidence as an open item\n\
\n\
If active vision or active mission is missing for strategic or owner-visible work, that is a review failure unless the current task is explicitly establishing them.\n\
\n\
Respond in exactly this shape:\n\
VERDICT: PASS|FAIL|PARTIAL\n\
MISSION_STATE: HEALTHY|UNHEALTHY|UNCLEAR\n\
SUMMARY: <one sentence>\n\
FAILED_GATES:\n\
- <plain rule that failed or \"none\">\n\
FINDINGS:\n\
- <semantic finding or \"none\">\n\
CATEGORIZED_FINDINGS:\n\
- id: <id> | category: rewrite|rework|stale_refresh|stale_obsolete|stale_consolidate | evidence: \"<evidence>\" | corrective_action: \"<corrective action>\"\n\
- <or \"none\">\n\
OPEN_ITEMS:\n\
- <concrete rework item>\n\
EVIDENCE:\n\
- <check> => <observed result>\n\
HANDOFF:\n\
- <only when another review run should continue; otherwise write \"none\">\n\
DISPOSITION: SEND|NO_SEND\n\
\n\
The CATEGORIZED_FINDINGS block is the structural input the dispatcher uses to choose between the lightweight rewrite path (body wording / subject / tonality fixes), the heavy rework loop (durable state changes, missing artefacts, evidence gaps), and stale refresh handling (new inbound/world state made the prior draft obsolete or in need of consolidation). Read the review skill section on Finding categories before assigning.\n\
\n\
DISPOSITION is the structural terminal flag: emit `NO_SEND` only when the current task is closed without sending anything (the correct action is to wait for external inputs, the user already received the answer elsewhere, the task was a duplicate, etc.). Default is `SEND` — used for every PASS verdict and for FAIL verdicts that should drive a rewrite or rework loop. The dispatcher reads this enum directly; do not encode the no-send signal as free-text in the summary or findings.\n",
        conversation_id = request.conversation_id,
        artifact_action = artifact_action,
        artifact_to = artifact_to,
        artifact_cc = artifact_cc,
        artifact_attachments = artifact_attachments,
        required_deliverables = required_deliverables,
        artifact_commitments = artifact_commitments,
        commitment_backing = commitment_backing,
        strategy_conversation_id = request.conversation_id,
        verification_conversation_id = request.conversation_id
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
                "Review report did not contain an explicit verdict, so the task stays open. {}",
                summary
            ),
            _ => "Review report did not contain an explicit verdict, so the task stays open."
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
        categorized_findings: parse_categorized_findings(report),
        open_items: parse_section_items(report, "OPEN_ITEMS:"),
        evidence: parse_section_items(report, "EVIDENCE:"),
        handoff: parse_handoff_block(report),
        disposition: parse_disposition(report).unwrap_or_default(),
    }
}

fn parse_disposition(report: &str) -> Option<ReviewDisposition> {
    for line in report.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("DISPOSITION:") else {
            continue;
        };
        return ReviewDisposition::parse(rest);
    }
    None
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
                || trimmed.starts_with("CATEGORIZED_FINDINGS:")
                || trimmed.starts_with("DISPOSITION:")
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
                    | "CATEGORIZED_FINDINGS:"
                    | "OPEN_ITEMS:"
                    | "EVIDENCE:"
                    | "HANDOFF:"
                    | "DISPOSITION:"
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

/// Parse the optional `CATEGORIZED_FINDINGS:` block. Each item is a single
/// line of pipe-delimited `key: value` pairs:
///
/// ```text
/// - id: f1 | category: rewrite | evidence: "internal vocab leak in greeting" | corrective_action: "use 'Hallo Founder,' instead"
/// ```
///
/// Items missing a recognised `category` token are dropped. The dispatcher
/// applies a conservative `Rework` default when it falls back to the legacy
/// `semantic_findings` list (i.e. when this returns empty); inside this
/// parser we never coerce unknown tokens — silent rejection keeps the
/// classification structural.
fn parse_categorized_findings(report: &str) -> Vec<CategorizedFinding> {
    let mut collecting = false;
    let mut findings = Vec::new();
    for line in report.lines() {
        let trimmed = line.trim();
        if trimmed == "CATEGORIZED_FINDINGS:" {
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
            let Some(item) = trimmed.strip_prefix("- ") else {
                continue;
            };
            let value = item.trim();
            if value.is_empty() || value.eq_ignore_ascii_case("none") {
                continue;
            }
            if let Some(finding) = parse_categorized_finding_line(value) {
                findings.push(finding);
            }
        }
    }
    findings
}

fn parse_categorized_finding_line(line: &str) -> Option<CategorizedFinding> {
    let mut id: Option<String> = None;
    let mut category: Option<FindingCategory> = None;
    let mut evidence: Option<String> = None;
    let mut corrective_action: Option<String> = None;
    for raw_segment in line.split('|') {
        let segment = raw_segment.trim();
        if segment.is_empty() {
            continue;
        }
        let Some((key, value)) = segment.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = unquote_field_value(value.trim());
        match key.as_str() {
            "id" => id = Some(value),
            "category" => category = FindingCategory::parse(&value),
            "evidence" => evidence = Some(value),
            "corrective_action" => corrective_action = Some(value),
            _ => {}
        }
    }
    let category = category?;
    Some(CategorizedFinding {
        id: id.unwrap_or_default(),
        category,
        evidence: evidence.unwrap_or_default(),
        corrective_action: corrective_action.unwrap_or_default(),
    })
}

fn unquote_field_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
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
    fn skips_review_for_internal_smoke_artifact_slice() {
        let request = CompletionReviewRequest {
            preview: "Qwen smoke".to_string(),
            source_label: "queue".to_string(),
            owner_visible: false,
            ..CompletionReviewRequest::default()
        };
        let (required, score, reasons) = assess_review_requirement(
            &request,
            "Created and verified the smoke artifact. Required file exists and contains qwen36-local-tool-ok.",
        );
        assert!(!required);
        assert!(score < 3);
        assert!(reasons
            .iter()
            .any(|reason| reason == "internal_artifact_slice"));
    }

    #[test]
    fn skips_review_for_internal_smoke_retry_feedback() {
        let request = CompletionReviewRequest {
            preview: "qwen36-local-smoke".to_string(),
            source_label: "queue".to_string(),
            owner_visible: false,
            ..CompletionReviewRequest::default()
        };
        let (required, score, reasons) = assess_review_requirement(
            &request,
            "Execution contract: use shell tools. HARNESS FEEDBACK: runtime failure; turn completed without assistant message. Required artifact result.txt exists. This is a smoke test for the response adapter and local backend. Verified qwen36-local-tool-ok.",
        );
        assert!(!required);
        assert!(score < 3);
        assert!(reasons
            .iter()
            .any(|reason| reason == "smoke_runtime_feedback_ignored"));
    }

    #[test]
    fn still_requires_review_for_owner_visible_internal_artifact_claim() {
        let request = CompletionReviewRequest {
            preview: "Production rollout".to_string(),
            source_label: "queue".to_string(),
            owner_visible: true,
            ..CompletionReviewRequest::default()
        };
        let (required, score, reasons) = assess_review_requirement(
            &request,
            "Created and verified the required artifact after restarting the service.",
        );
        assert!(required);
        assert!(score >= 3);
        assert!(reasons.iter().any(|reason| reason == "owner_visible_claim"));
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
            ..CompletionReviewRequest::default()
        };
        let rendered = build_review_prompt(&request, &["closure_claim".to_string()]);
        assert!(rendered.contains("== REVIEW ASSIGNMENT =="));
        assert!(rendered.contains("Conversation id: 42"));
        assert!(rendered.contains("/srv/skills/system/review/external-review/SKILL.md"));
        assert!(rendered.contains("Open the review skill first and follow it."));
        assert!(rendered.contains("Artifact under review:"));
        assert!(rendered.contains("Patched rollout artifact"));
        assert!(rendered.contains("inspect recent relevant meeting outcomes"));
        assert!(rendered.contains("channel='meeting'"));
        assert!(rendered.contains("Meeting Summary"));
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
            artifact_action: Some("reply".to_string()),
            artifact_to: vec!["o.schaefers@gmx.net".to_string()],
            artifact_cc: vec!["michael.welsch@metric-space.ai".to_string()],
            artifact_attachments: vec![
                "/srv/runtime/communication/artifacts/jami/ctox-jami-setup.pdf".to_string(),
            ],
            required_deliverables: vec!["qr_code".to_string()],
            artifact_commitments: vec![
                "Today, 24.04.2026, send an update by 20:00 UTC.".to_string()
            ],
            commitment_backing: vec![
                "kunstmen founder update 20utc @ 2026-04-24T20:00:00+00:00".to_string()
            ],
        };
        let rendered = build_review_prompt(&request, &["founder_communication".to_string()]);
        assert!(rendered.contains("Artifact kind: founder_or_owner_outbound_email_draft"));
        assert!(rendered.contains("judge the outbound draft itself as the artifact under review"));
        assert!(rendered.contains("Artifact action: reply"));
        assert!(rendered.contains("Artifact to: o.schaefers@gmx.net"));
        assert!(rendered.contains("Artifact cc: michael.welsch@metric-space.ai"));
        assert!(rendered.contains(
            "Artifact attachments: /srv/runtime/communication/artifacts/jami/ctox-jami-setup.pdf"
        ));
        assert!(rendered.contains("Required deliverables: qr_code"));
        assert!(rendered
            .contains("Artifact commitments: Today, 24.04.2026, send an update by 20:00 UTC."));
        assert!(rendered.contains(
            "Commitment backing: kunstmen founder update 20utc @ 2026-04-24T20:00:00+00:00"
        ));
        assert!(rendered.contains("judge the full mail action, not just the prose"));
        assert!(rendered.contains("treat every listed required deliverable as mandatory"));
        assert!(rendered.contains("future promise, dated commitment, or deadline promise"));
        assert!(rendered.contains("inspect recent relevant meeting outcomes"));
        assert!(rendered.contains("latest meeting changed decisions"));
        assert!(rendered.contains("contradicts verified actions"));
        assert!(rendered.contains("setup, access, credentials, links, or next operational steps"));
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
        assert!(outcome.requires_follow_up());
    }

    #[test]
    fn parse_handoff_block_extracts_multiline_body() {
        let report = "VERDICT: PARTIAL\nMISSION_STATE: UNCLEAR\nSUMMARY: Need more checks.\nHANDOFF:\n- Verified public URL returns 200\n- Still need /api/state and buyer path\n";
        let handoff = parse_handoff_block(report).expect("handoff missing");
        assert!(handoff.contains("Verified public URL returns 200"));
        assert!(handoff.contains("Still need /api/state"));
    }

    #[test]
    fn parses_categorized_findings_block() {
        let report = "VERDICT: FAIL\nMISSION_STATE: UNCLEAR\nSUMMARY: wording issues.\nFAILED_GATES:\n- none\nFINDINGS:\n- internal vocab leak\nCATEGORIZED_FINDINGS:\n- id: f1 | category: rewrite | evidence: \"greeting uses TUI jargon\" | corrective_action: \"replace with neutral salutation\"\n- id: f2 | category: rework | evidence: \"body claims send before approval row exists\" | corrective_action: \"create approval and re-run review\"\nOPEN_ITEMS:\n- none\n";
        let outcome = parse_review_report(4, vec![], report);
        assert_eq!(outcome.verdict, ReviewVerdict::Fail);
        assert_eq!(outcome.categorized_findings.len(), 2);
        assert_eq!(outcome.categorized_findings[0].id, "f1");
        assert_eq!(
            outcome.categorized_findings[0].category,
            FindingCategory::Rewrite
        );
        assert!(outcome.categorized_findings[0]
            .evidence
            .contains("greeting"));
        assert!(outcome.categorized_findings[0]
            .corrective_action
            .contains("salutation"));
        assert_eq!(
            outcome.categorized_findings[1].category,
            FindingCategory::Rework
        );
    }

    #[test]
    fn parses_stale_categorized_findings() {
        let report = "VERDICT: FAIL\nSUMMARY: world changed.\nCATEGORIZED_FINDINGS:\n- id: f1 | category: stale_refresh | evidence: \"new inbound arrived\" | corrective_action: \"reload thread\"\nOPEN_ITEMS:\n- reload thread\n";
        let outcome = parse_review_report(3, vec![], report);
        assert_eq!(outcome.categorized_findings.len(), 1);
        assert_eq!(
            outcome.categorized_findings[0].category,
            FindingCategory::StaleRefresh
        );
        assert!(outcome.categorized_findings[0].category.is_stale());
    }

    #[test]
    fn parses_review_report_without_categorized_block_keeps_findings_empty() {
        let report = "VERDICT: FAIL\nSUMMARY: legacy report.\nFINDINGS:\n- something is off\nOPEN_ITEMS:\n- none\n";
        let outcome = parse_review_report(3, vec![], report);
        assert_eq!(outcome.verdict, ReviewVerdict::Fail);
        assert!(outcome.categorized_findings.is_empty());
        assert_eq!(outcome.semantic_findings.len(), 1);
    }

    #[test]
    fn drops_findings_with_unknown_or_missing_category() {
        let report = "VERDICT: FAIL\nSUMMARY: noise.\nCATEGORIZED_FINDINGS:\n- id: f1 | category: bogus | evidence: \"x\" | corrective_action: \"y\"\n- id: f2 | evidence: \"x\" | corrective_action: \"y\"\n- id: f3 | category: rewrite | evidence: \"x\" | corrective_action: \"y\"\nOPEN_ITEMS:\n- none\n";
        let outcome = parse_review_report(3, vec![], report);
        assert_eq!(outcome.categorized_findings.len(), 1);
        assert_eq!(outcome.categorized_findings[0].id, "f3");
    }

    #[test]
    fn parses_no_send_disposition_block() {
        let report =
            "VERDICT: FAIL\nSUMMARY: wait for inputs.\nOPEN_ITEMS:\n- none\nDISPOSITION: NO_SEND\n";
        let outcome = parse_review_report(3, vec![], report);
        assert_eq!(outcome.disposition, ReviewDisposition::NoSend);
    }

    #[test]
    fn defaults_to_send_disposition_when_block_missing() {
        let report =
            "VERDICT: FAIL\nSUMMARY: rewrite the body.\nOPEN_ITEMS:\n- correct the salutation\n";
        let outcome = parse_review_report(3, vec![], report);
        assert_eq!(outcome.disposition, ReviewDisposition::Send);
    }

    #[test]
    fn parses_send_disposition_block_explicitly() {
        let report = "VERDICT: PASS\nSUMMARY: looks good.\nDISPOSITION: SEND\n";
        let outcome = parse_review_report(0, vec![], report);
        assert_eq!(outcome.disposition, ReviewDisposition::Send);
    }

    #[test]
    fn unknown_disposition_token_falls_back_to_send() {
        let report = "VERDICT: FAIL\nSUMMARY: weird.\nDISPOSITION: MAYBE\n";
        let outcome = parse_review_report(0, vec![], report);
        assert_eq!(outcome.disposition, ReviewDisposition::Send);
    }
}
