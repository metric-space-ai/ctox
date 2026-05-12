use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use crate::execution::agent::direct_session::PersistentSession;
use crate::inference::runtime_env;

const REVIEW_TIMEOUT_SECS: u64 = 900;
const REVIEW_MAX_LEGS: usize = 3;

const REVIEW_SYSTEM_PROMPT: &str = r#"You are CTOX Review.

You run an external verification pass for one reviewed task result.

Use the review assignment as the only task definition.
Use the review assignment as the starting point, then read the relevant continuity, runtime, communication, meeting, ticket, artifact, and live-surface context yourself.

Operate in strict read-only verification mode.
You may use read-only shell/CLI/browser/database inspection to gather evidence. Do not mutate files, send messages, update tickets, apply patches, install packages, restart services, join meetings, or perform any worker action.
You are a control-plane reviewer, not an executor. Read deeply, judge skeptically, and report what the worker must do; never do the worker's action yourself.
Base the verdict on inspected evidence, not on prose claims. Treat every worker self-report, completion summary, status sentence, and claimed test result as an unverified lead only. The reviewer's core job is to independently verify the decisive completion claims against current files, runtime state, communication records, tickets, live surfaces, or trusted external systems. If evidence is insufficient for a required gate, return PARTIAL or FAIL with the exact missing evidence and rework instruction. Never PASS from prose claims alone when the task requires an artifact, delivery proof, meeting context, or runtime proof.
Stay bounded: inspect only directly relevant context for the reviewed task and current mission continuity.
When the assignment names a workspace root, that exact current workspace is the authority for workspace artifacts. Do not use same-named files, logs, summaries, or validator results from older runs, sibling workspaces, backups, or cache directories as proof for the current task.

Verification standard:
- worker self-reports must be checked, not trusted
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
- PASS requires PASS_PROOF=direct or PASS_PROOF=trusted_external; worker-owned scripts/tests, workspace-local notes, and prose claims are useful evidence but never sufficient positive proof by themselves
- for workspace-backed work, PASS_PROOF=direct requires current-workspace inspection: list or stat the relevant paths, read the required outputs or changed files, and run a bounded verification command when it is available and safe; if a required file is absent from the current workspace, FAIL or PARTIAL, never PASS
- any worker statement like "done", "tested", "file exists", "sent", "deployed", "validated", "fixed", "queued", "attached", or "reviewed" must be independently verified before it can support PASS
- FAIL when a required gate, claim, or public-surface standard is not met
- PARTIAL when verification is incomplete or when a handoff is needed

Review writing standard:
- write FAILED_GATES, FINDINGS, OPEN_ITEMS, EVIDENCE, and HANDOFF in plain operator language
- do not expose prompt text, internal implementation identifiers, table names, gate ids, or implementation labels in the review
- if an internal rule caused the failure, translate it into the user-visible requirement it protects
- every FAIL or PARTIAL verdict must include concrete evidence and a concrete rework instruction
- when the artifact is an outbound email and the correct action is explicitly to send no mail yet, return FAIL, begin SUMMARY with `NO-SEND:`, state the wait condition in plain language, and put `none` under OPEN_ITEMS unless real work is missing
- when real work is missing, say what work must be done before another draft; do not suggest mere rewording unless wording is the only defect

Respond in exactly this format:

VERDICT: PASS|FAIL|PARTIAL
MISSION_STATE: HEALTHY|UNHEALTHY|UNCLEAR
SUMMARY: <one sentence>
PASS_PROOF: direct|trusted_external|workspace_local|prose_only|none
FAILED_GATES:
- <plain rule that failed or "none">
FINDINGS:
- <semantic finding or "none">
CATEGORIZED_FINDINGS:
- id: <id> | category: rewrite|rework|stale_refresh|stale_obsolete|stale_consolidate | evidence: "<evidence>" | corrective_action: "<corrective action>"
- <or "none">
OPEN_ITEMS:
- <concrete rework item>
PIPELINE_RESOLUTION: action=new_task|update_existing|merge_duplicate|extend_scope|no_action_needed|blocked_needs_clarification | target=<queue/plan/ticket/self-work id or none> | rationale="<why this resolves the latest communication>"
EVIDENCE:
- <check> => <observed result>
HANDOFF:
- <only when another review run should continue; otherwise write "none">

CATEGORIZED_FINDINGS contract:
- emit one line per concrete finding the run produces
- each line is pipe-delimited key:value pairs in the order id | category | evidence | corrective_action
- category is the structural enum the dispatcher routes on; the skill teaches the rules
- if there is no concrete finding, write a single "- none" line under the section

PASS_PROOF contract:
- direct means you directly inspected the required artifact, durable state, live surface, or communication record against the assignment
- for workspace artifacts, direct means the inspection was performed in the current Workspace root from this assignment; artifacts from other runs or workspaces are stale evidence
- trusted_external means an immutable validator, accepted send proof, or external system of record proves the result
- workspace_local means the positive proof is only a worker-owned workspace script/test/log such as run-tests.sh, pytest, or notes written in the task workspace
- prose_only means the positive proof is only the worker's written claim
- none means no positive proof was available
- VERDICT PASS is invalid unless PASS_PROOF is direct or trusted_external
"#;

#[derive(Debug, Clone, Default)]
pub struct CompletionReviewRequest {
    pub task_goal: String,
    pub task_prompt: String,
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
    pub artifact_channel: String,
    pub artifact_to: Vec<String>,
    pub artifact_cc: Vec<String>,
    pub artifact_subject: String,
    pub artifact_attachments: Vec<String>,
    pub required_deliverables: Vec<String>,
    pub artifact_commitments: Vec<String>,
    pub commitment_backing: Vec<String>,
    pub deterministic_evidence: Vec<String>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReviewPassProofKind {
    Direct,
    TrustedExternal,
    WorkspaceLocal,
    ProseOnly,
    None,
}

impl ReviewPassProofKind {
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "direct" | "direct_inspection" | "direct-inspection" => Some(Self::Direct),
            "trusted_external" | "trusted-external" | "external" => Some(Self::TrustedExternal),
            "workspace_local" | "workspace-local" | "local" => Some(Self::WorkspaceLocal),
            "prose_only" | "prose-only" | "prose" => Some(Self::ProseOnly),
            "none" | "no_proof" | "no-proof" => Some(Self::None),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::TrustedExternal => "trusted_external",
            Self::WorkspaceLocal => "workspace_local",
            Self::ProseOnly => "prose_only",
            Self::None => "none",
        }
    }

    pub fn is_acceptable_for_pass(&self) -> bool {
        matches!(self, Self::Direct | Self::TrustedExternal)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PipelineResolutionAction {
    NewTask,
    UpdateExisting,
    MergeDuplicate,
    ExtendScope,
    NoActionNeeded,
    BlockedNeedsClarification,
}

impl PipelineResolutionAction {
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "new_task" | "new-task" => Some(Self::NewTask),
            "update_existing" | "update-existing" => Some(Self::UpdateExisting),
            "merge_duplicate" | "merge-duplicate" => Some(Self::MergeDuplicate),
            "extend_scope" | "extend-scope" => Some(Self::ExtendScope),
            "no_action_needed" | "no-action-needed" => Some(Self::NoActionNeeded),
            "blocked_needs_clarification" | "blocked-needs-clarification" => {
                Some(Self::BlockedNeedsClarification)
            }
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NewTask => "new_task",
            Self::UpdateExisting => "update_existing",
            Self::MergeDuplicate => "merge_duplicate",
            Self::ExtendScope => "extend_scope",
            Self::NoActionNeeded => "no_action_needed",
            Self::BlockedNeedsClarification => "blocked_needs_clarification",
        }
    }

    pub fn requires_target(&self) -> bool {
        matches!(
            self,
            Self::NewTask | Self::UpdateExisting | Self::MergeDuplicate | Self::ExtendScope
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineResolution {
    pub action: PipelineResolutionAction,
    pub target: String,
    pub rationale: String,
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
    #[serde(default)]
    pub pipeline_resolution: Option<PipelineResolution>,
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
            pipeline_resolution: None,
        }
    }

    pub fn requires_follow_up(&self) -> bool {
        self.required && matches!(self.verdict, ReviewVerdict::Fail | ReviewVerdict::Partial)
    }

    pub fn pass_proof_kind(&self) -> Option<ReviewPassProofKind> {
        parse_pass_proof(&self.canonical_report())
    }

    pub fn has_acceptable_pass_proof(&self) -> bool {
        let report = self.canonical_report();
        let proof = parse_pass_proof(&report);
        let evidence = if self.evidence.is_empty() {
            parse_section_items(&report, "EVIDENCE:")
        } else {
            self.evidence.clone()
        };
        proof
            .map(|kind| {
                kind.is_acceptable_for_pass()
                    && unsupported_pass_proof_reason(proof, &report, &evidence).is_none()
            })
            .unwrap_or(false)
    }

    pub fn canonical_report(&self) -> String {
        if !self.report.trim().is_empty() {
            return self.report.trim().to_string();
        }

        let mut rendered = Vec::new();
        rendered.push(format!("VERDICT: {}", self.verdict.as_report_label()));
        rendered.push(format!("MISSION_STATE: {}", self.mission_state.trim()));
        rendered.push(format!("SUMMARY: {}", self.summary.trim()));
        if matches!(self.verdict, ReviewVerdict::Pass) {
            rendered.push("PASS_PROOF: none".to_string());
        }
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
        if let Some(resolution) = &self.pipeline_resolution {
            rendered.push(format!(
                "PIPELINE_RESOLUTION: action={} | target={} | rationale=\"{}\"",
                resolution.action.as_str(),
                resolution.target.trim(),
                resolution.rationale.trim()
            ));
        }
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
            pipeline_resolution: None,
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

        if verdict.is_none() && leg + 1 < REVIEW_MAX_LEGS {
            prompt = build_review_format_retry_prompt(request, &last_report);
            continue;
        }
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
    let mut session = PersistentSession::start_review_with_read_only_tools(
        root,
        settings,
        Some(REVIEW_SYSTEM_PROMPT),
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
        "{}\n{}\n{}\n{}\n{}",
        request.task_goal, request.task_prompt, request.preview, request.source_label, result_text
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
    let external_chat_quick_response = request
        .artifact_action
        .as_deref()
        .map(|value| {
            value
                .to_ascii_lowercase()
                .contains("external_chat_quick_response")
        })
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
    let workspace_backed_queue_task = request.source_label.eq_ignore_ascii_case("queue")
        && !request.workspace_root.trim().is_empty()
        && request.artifact_action.is_none()
        && !founder_or_owner_email;
    if workspace_backed_queue_task && !internal_smoke_artifact_slice {
        score = score.saturating_add(3);
        push_unique_reason(&mut reasons, "workspace_backed_queue_task");
    }

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
    if external_chat_quick_response {
        score = score.saturating_add(3);
        push_unique_reason(&mut reasons, "external_chat_quick_response");
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
    let task_goal = if request.task_goal.trim().is_empty() {
        "(none recorded)"
    } else {
        request.task_goal.trim()
    };
    let task_prompt = if request.task_prompt.trim().is_empty() {
        "(none recorded)"
    } else {
        request.task_prompt.trim()
    };
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
    let email_artifact = request.artifact_channel.eq_ignore_ascii_case("email");
    let founder_artifact = matches!(
        request.source_label.to_ascii_lowercase().as_str(),
        "email:owner" | "email:founder" | "email:admin"
    ) || request
        .artifact_action
        .as_deref()
        .map(|value| value.to_ascii_lowercase().contains("founder"))
        .unwrap_or(false)
        || email_artifact;
    let external_chat_artifact = request
        .artifact_action
        .as_deref()
        .map(|value| {
            value
                .to_ascii_lowercase()
                .contains("external_chat_quick_response")
        })
        .unwrap_or(false);
    let workspace_backed_queue_task = request.source_label.eq_ignore_ascii_case("queue")
        && !request.workspace_root.trim().is_empty()
        && request.artifact_action.is_none()
        && !founder_artifact;
    let artifact_kind = if founder_artifact {
        "reviewed_outbound_email_draft"
    } else if external_chat_artifact {
        "external_chat_quick_response"
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
    let artifact_channel = if request.artifact_channel.trim().is_empty() {
        "(none recorded)"
    } else {
        request.artifact_channel.trim()
    };
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
    let artifact_subject = if request.artifact_subject.trim().is_empty() {
        "(none recorded)"
    } else {
        request.artifact_subject.trim()
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
    let deterministic_evidence = if request.deterministic_evidence.is_empty() {
        "(none recorded)".to_string()
    } else {
        request.deterministic_evidence.join("\n- ")
    };
    let founder_specific_work = if founder_artifact {
        "\
Email communication gate:\n\
- judge the outbound draft itself as the artifact under review\n\
- judge the full mail action, not just the prose: recipients, cc list, subject, attachments, and reply/forward behavior are part of the artifact\n\
- verify that the mail is still relevant against the latest communication across all channels, meeting outcomes, ticket/work state, and durable knowledge/runbook context\n\
- verify that email sends the result of completed work; fail when it only promises work that has not actually been done, reviewed, queued, or attached\n\
- verify that the wording is recipient-appropriate, concise, and does not hide material limitations or missing proof\n\
- decide whether the draft should be sent now, blocked, or reworked first\n\
- if newer communication makes this draft stale, fail the review and state which newer message/context must be answered instead\n\
- if the correct outcome is no outbound mail yet because the thread is waiting on specific input, return FAIL, begin SUMMARY with `NO-SEND:`, and state the wait condition; do not invent rework\n\
- emit PIPELINE_RESOLUTION for the mail: use no_action_needed only when the latest communication requires no queue/ticket change; otherwise name the exact queue/plan/ticket/self-work item created, updated, merged, extended, or blocked\n\
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
    let external_chat_specific_work = if external_chat_artifact {
        "\
External chat quick-response gate:\n\
- this is a quick acknowledgement/reply for an external chat channel, not a final result mail\n\
- judge the full chat action: channel, thread, subject when present, recipients when present, attachments, and body\n\
- approve only short, timely, recipient-appropriate responses that either acknowledge the task accurately or ask a necessary clarifying question\n\
- if the body promises follow-up work, verify durable pipeline backing exists first: queue item, plan, ticket case, or self-work linked to this thread\n\
- classify the communication-to-pipeline delta before approving: new task, update existing task, merge duplicate, extend scope, no action needed, or blocked awaiting clarification\n\
- verify that the chosen delta is durably represented by the evidence: every actionable request must be backed by a referenced queue item, plan, ticket, or self-work item; merged/extended work must name the existing item it changes\n\
- fail if newer communication, meeting notes, ticket state, or durable knowledge changes scope, priority, recipient expectations, due date, or result validity and the response leaves stale or superseded work unresolved\n\
- fail if any actionable request is hidden, dropped, vaguely promised, or left without an explicit pipeline resolution; nothing may remain unresolved under the conversation thread\n\
- emit PIPELINE_RESOLUTION with the exact action and target before PASS; use blocked_needs_clarification when the chat response asks for missing input instead of creating/updating work\n\
- do not require the final work result before approving a chat acknowledgement; require a real pipeline item instead\n\
- fail if the response claims the work is done before evidence exists, promises work without backing, omits an obvious clarification, or ignores current communication/meeting/knowledge context\n\
- fail if a requested attachment is mentioned but not attached, or if an attachment is attached without being relevant\n\
"
    } else {
        ""
    };

    let artifact_review_work = if founder_artifact {
        ""
    } else if workspace_backed_queue_task {
        "\
Workspace-backed task review gate:\n\
- inspect the workspace root directly and evaluate the full task result against the original task contract, not just the final response text\n\
- audit worker self-reports explicitly: if the worker says a file exists, tests passed, output was written, code was changed, or a command works, verify that exact claim in the current workspace before relying on it\n\
- treat the current Workspace root above as the only authoritative workspace for this review; ignore same-named files, replay logs, backups, stale validations, and sibling workspaces unless they are explicitly trusted external acceptance records for this exact task\n\
- infer any required output files from the task contract and verify them in the current workspace with direct commands such as `pwd`, `find`, `test -f`, `stat`, `wc -c`, and targeted reads before PASS\n\
- inspect changed, added, and deleted files relevant to the task; compare implementation behavior against the requested outcome\n\
- run bounded verification commands when they are available and safe for review, such as existing smoke tests, command help/version checks, import checks, or project test commands that do not perform the worker's missing implementation work\n\
- if tests/checks are available but cannot be run safely in review, mark PARTIAL and name the exact missing verification instead of PASS\n\
- if the task requires a file and that file is absent, empty when content is required, in the wrong location, or only present in a stale/other workspace, FAIL with rework; do not PASS because the worker summary says it exists\n\
- PASS only when the workspace state and direct evidence support the completion claim; FAIL when required files, implementation changes, or task-specific behavior are missing or contradicted by evidence\n\
- do not approve merely because the worker claimed success, produced a plausible summary, or created some unrelated artifact\n\
"
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
Original task contract:\n\
Task goal: {task_goal}\n\
Task prompt: {task_prompt}\n\
\n\
Artifact under review:\n\
Artifact action: {artifact_action}\n\
Artifact channel: {artifact_channel}\n\
Artifact to: {artifact_to}\n\
Artifact cc: {artifact_cc}\n\
Artifact subject: {artifact_subject}\n\
Artifact attachments: {artifact_attachments}\n\
Required deliverables: {required_deliverables}\n\
Artifact commitments: {artifact_commitments}\n\
Commitment backing: {commitment_backing}\n\
Deterministic review evidence:\n\
- {deterministic_evidence}\n\
--- BEGIN ARTIFACT ---\n\
{artifact_text}\n\
--- END ARTIFACT ---\n\
\n\
Open the review skill first and follow it.\n\
\n\
Start from the deterministic review evidence above, then inspect the underlying runtime/continuity/artifact context yourself with read-only tools. Do not assume a claim is true unless the deterministic evidence or your own read-only inspection supports it.\n\
Treat the worker's artifact text and completion summary as claims to audit, not as proof. For every decisive claim needed for PASS, write the check you performed in EVIDENCE; if you did not verify it yourself or from a trusted external system of record, do not PASS.\n\
\n\
Required review work:\n\
1. load active strategic directives, mission state, and continuity for this conversation/thread\n\
2. inspect recent verification runs, open claims, queue/self-work/ticket state, and the reviewed task result\n\
3. inspect recent same-thread communication and recent relevant meeting outcomes before communication or artifact verdicts\n\
4. inspect explicit artifact paths and attachments directly; for spreadsheets, verify row counts, headers, and whether the content satisfies the requested scope\n\
5. inspect delivery state before accepting any claim that a message, mail, or attachment was sent\n\
6. inspect live public/runtime surfaces when the reviewed work depends on them\n\
7. compare the artifact text, recipients, cc list, subject, attachments, commitments, and required deliverables against the original task contract and inspected evidence\n\
8. decide whether this specific artifact is ready to send/release/close, or whether real rework is required first\n\
9. produce a verdict from evidence\n\
\n\
Use the runtime DB path and workspace root above as the primary grounding points.\n\
\n\
{founder_specific_work}\
{external_chat_specific_work}\
{artifact_review_work}\
\n\
If active vision or active mission is missing for strategic or owner-visible work, that is a review failure unless the current task is explicitly establishing them.\n\
\n\
Respond in exactly this shape:\n\
VERDICT: PASS|FAIL|PARTIAL\n\
MISSION_STATE: HEALTHY|UNHEALTHY|UNCLEAR\n\
SUMMARY: <one sentence>\n\
PASS_PROOF: direct|trusted_external|workspace_local|prose_only|none\n\
FAILED_GATES:\n\
- <plain rule that failed or \"none\">\n\
FINDINGS:\n\
- <semantic finding or \"none\">\n\
CATEGORIZED_FINDINGS:\n\
- id: <id> | category: rewrite|rework|stale_refresh|stale_obsolete|stale_consolidate | evidence: \"<evidence>\" | corrective_action: \"<corrective action>\"\n\
- <or \"none\">\n\
OPEN_ITEMS:\n\
- <concrete rework item>\n\
PIPELINE_RESOLUTION: action=new_task|update_existing|merge_duplicate|extend_scope|no_action_needed|blocked_needs_clarification | target=<queue/plan/ticket/self-work id or none> | rationale=\"<why this resolves the latest communication>\"\n\
EVIDENCE:\n\
- <check> => <observed result>\n\
HANDOFF:\n\
- <only when another review run should continue; otherwise write \"none\">\n\
DISPOSITION: SEND|NO_SEND\n\
\n\
The CATEGORIZED_FINDINGS block is the structural input the dispatcher uses to choose between the lightweight rewrite path (body wording / subject / tonality fixes), the heavy rework loop (durable state changes, missing artefacts, evidence gaps), and stale refresh handling (new inbound/world state made the prior draft obsolete or in need of consolidation). Read the review skill section on Finding categories before assigning.\n\
\n\
PASS_PROOF is a structural trust boundary. Emit `direct` only when you inspected the required artifact, durable state, live surface, or communication record yourself against the assignment. Emit `trusted_external` only when an immutable validator, accepted send proof, or external system of record proves the result. Emit `workspace_local` when the only positive proof is a worker-owned workspace script/test/log such as run-tests.sh or pytest. Emit `prose_only` for worker claims. Emit `none` when no positive proof exists. A PASS without `direct` or `trusted_external` is invalid.\n\
\n\
DISPOSITION is the structural terminal flag: emit `NO_SEND` only when the current task is closed without sending anything (the correct action is to wait for external inputs, the user already received the answer elsewhere, the task was a duplicate, etc.). Default is `SEND` — used for every PASS verdict and for FAIL verdicts that should drive a rewrite or rework loop. The dispatcher reads this enum directly; do not encode the no-send signal as free-text in the summary or findings.\n",
        conversation_id = request.conversation_id,
        task_goal = task_goal,
        task_prompt = task_prompt,
        artifact_action = artifact_action,
        artifact_channel = artifact_channel,
        artifact_to = artifact_to,
        artifact_cc = artifact_cc,
        artifact_subject = artifact_subject,
        artifact_attachments = artifact_attachments,
        required_deliverables = required_deliverables,
        artifact_commitments = artifact_commitments,
        commitment_backing = commitment_backing,
        deterministic_evidence = deterministic_evidence,
        external_chat_specific_work = external_chat_specific_work,
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
Open the review skill first and continue from this review handoff using read-only inspection only.\n\
\n\
Prior handoff:\n\
{}\n\
\n\
Continue the remaining verification work and return the standard review format.\n",
        request.conversation_id,
        handoff.trim()
    )
}

fn build_review_format_retry_prompt(
    request: &CompletionReviewRequest,
    prior_report: &str,
) -> String {
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
    let prior = if prior_report.trim().is_empty() {
        "(review returned an empty response)"
    } else {
        prior_report.trim()
    };

    format!(
        "== REVIEW FORMAT RETRY ==\n\
\n\
The previous review response did not include a parseable `VERDICT:` line. Continue the same read-only review and return the required structured format exactly.\n\
\n\
Thread key: {thread_key}\n\
Workspace root: {workspace_root}\n\
\n\
Previous malformed review response:\n\
--- BEGIN PRIOR REVIEW ---\n\
{}\n\
--- END PRIOR REVIEW ---\n\
\n\
Do not do worker actions. Inspect only as needed to produce a real verdict. Return exactly:\n\
VERDICT: PASS|FAIL|PARTIAL\n\
MISSION_STATE: HEALTHY|UNHEALTHY|UNCLEAR\n\
SUMMARY: <one sentence>\n\
PASS_PROOF: direct|trusted_external|workspace_local|prose_only|none\n\
FAILED_GATES:\n\
- <plain rule that failed or \"none\">\n\
FINDINGS:\n\
- <semantic finding or \"none\">\n\
CATEGORIZED_FINDINGS:\n\
- id: <id> | category: rewrite|rework|stale_refresh|stale_obsolete|stale_consolidate | evidence: \"<evidence>\" | corrective_action: \"<corrective action>\"\n\
- <or \"none\">\n\
OPEN_ITEMS:\n\
- <concrete rework item or \"none\">\n\
PIPELINE_RESOLUTION: action=new_task|update_existing|merge_duplicate|extend_scope|no_action_needed|blocked_needs_clarification | target=<queue/plan/ticket/self-work id or none> | rationale=\"<why this resolves the latest communication>\"\n\
EVIDENCE:\n\
- <check> => <observed result>\n\
HANDOFF:\n\
- none\n\
DISPOSITION: SEND|NO_SEND\n",
        clip_text(prior, 2_000)
    )
}

fn parse_review_report(score: u8, reasons: Vec<String>, report: &str) -> ReviewOutcome {
    let parsed_verdict = parse_verdict(report);
    let mut verdict = parsed_verdict.clone().unwrap_or(ReviewVerdict::Partial);
    let pass_proof = parse_pass_proof(report);
    let mission_state = parse_prefixed_line(report, "MISSION_STATE:")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "UNCLEAR".to_string());
    let mut summary = if parsed_verdict.is_none() {
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
    let mut failed_gates = parse_section_items(report, "FAILED_GATES:");
    let semantic_findings = parse_section_items(report, "FINDINGS:");
    let categorized_findings = parse_categorized_findings(report);
    let mut open_items = parse_section_items(report, "OPEN_ITEMS:");
    let evidence = parse_section_items(report, "EVIDENCE:");
    if verdict == ReviewVerdict::Pass && review_pass_is_unsubstantiated(score, report, &evidence) {
        verdict = ReviewVerdict::Partial;
        summary = format!(
            "Review PASS was rejected because the reviewer did not produce usable inspection evidence. {}",
            summary
        );
        push_unique_item(
            &mut failed_gates,
            "Reviewer pass lacked usable direct evidence.",
        );
        push_unique_item(
            &mut open_items,
            "Re-run review with direct filesystem, database, log, process, or live-surface evidence before accepting completion.",
        );
    }
    if verdict == ReviewVerdict::Pass
        && !pass_proof
            .map(|proof| proof.is_acceptable_for_pass())
            .unwrap_or(false)
    {
        verdict = ReviewVerdict::Partial;
        let observed = pass_proof
            .map(|proof| proof.as_str().to_string())
            .unwrap_or_else(|| "missing".to_string());
        summary = format!(
            "Review PASS was rejected because PASS_PROOF was `{observed}` instead of direct or trusted_external. {summary}"
        );
        push_unique_item(
            &mut failed_gates,
            "Reviewer pass did not declare direct or trusted external proof.",
        );
        push_unique_item(
            &mut open_items,
            "Re-run review and inspect the required artifact, durable state, live surface, communication record, or trusted external validator before accepting completion.",
        );
    }
    if verdict == ReviewVerdict::Pass {
        if let Some(reason) = unsupported_pass_proof_reason(pass_proof, report, &evidence) {
            verdict = ReviewVerdict::Partial;
            summary = format!("Review PASS was rejected because {reason}. {summary}");
            push_unique_item(
                &mut failed_gates,
                "Reviewer PASS_PROOF was not supported by non-worker-owned evidence.",
            );
            push_unique_item(
                &mut open_items,
                "Re-run review with direct artifact/state/communication/live-surface inspection or trusted external acceptance evidence; worker-owned tests, logs, and prose are not sufficient.",
            );
        }
    }
    ReviewOutcome {
        required: true,
        verdict,
        mission_state,
        summary,
        report: report.trim().to_string(),
        score,
        reasons,
        failed_gates,
        semantic_findings,
        categorized_findings,
        open_items,
        evidence,
        handoff: parse_handoff_block(report),
        disposition: parse_disposition(report).unwrap_or_default(),
        pipeline_resolution: parse_pipeline_resolution(report),
    }
}

pub fn parse_pass_proof(report: &str) -> Option<ReviewPassProofKind> {
    parse_prefixed_line(report, "PASS_PROOF:")
        .and_then(|value| ReviewPassProofKind::parse(value.split_whitespace().next().unwrap_or("")))
}

fn parse_pipeline_resolution(report: &str) -> Option<PipelineResolution> {
    for line in report.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("PIPELINE_RESOLUTION:") else {
            continue;
        };
        let mut action = None;
        let mut target = None;
        let mut rationale = None;
        for raw_segment in rest.split('|') {
            let segment = raw_segment.trim();
            let Some((key, value)) = segment.split_once('=') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = unquote_field_value(value.trim());
            match key.as_str() {
                "action" => action = PipelineResolutionAction::parse(&value),
                "target" => target = Some(value),
                "rationale" => rationale = Some(value),
                _ => {}
            }
        }
        let action = action?;
        return Some(PipelineResolution {
            action,
            target: target.unwrap_or_default(),
            rationale: rationale.unwrap_or_default(),
        });
    }
    None
}

fn review_pass_is_unsubstantiated(score: u8, report: &str, evidence: &[String]) -> bool {
    // Production review is only invoked for high-risk slices (score >= 3).
    // A PASS from that path must be backed by direct checks. If the reviewer
    // says tools/sandbox blocked inspection, treat the pass as incomplete even
    // if it still emitted a nominal evidence line.
    score >= 3 && (evidence.is_empty() || report_mentions_review_access_blocker(report))
}

fn unsupported_pass_proof_reason(
    pass_proof: Option<ReviewPassProofKind>,
    report: &str,
    evidence: &[String],
) -> Option<String> {
    let proof = pass_proof?;
    if !proof.is_acceptable_for_pass() {
        return None;
    }
    let evidence_class = classify_pass_evidence(report, evidence);
    match proof {
        ReviewPassProofKind::Direct if evidence_class.direct_or_trusted() => None,
        ReviewPassProofKind::Direct => Some(
            "PASS_PROOF was declared direct, but the evidence is only worker-owned tests/logs, prose, or otherwise not direct inspection".to_string(),
        ),
        ReviewPassProofKind::TrustedExternal if evidence_class.trusted_external => None,
        ReviewPassProofKind::TrustedExternal => Some(
            "PASS_PROOF was declared trusted_external, but the evidence does not cite an immutable validator, accepted send proof, or external system of record".to_string(),
        ),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct PassEvidenceClass {
    trusted_external: bool,
    direct: bool,
    workspace_local: bool,
    prose_only: bool,
}

impl PassEvidenceClass {
    fn direct_or_trusted(&self) -> bool {
        self.trusted_external || self.direct
    }
}

fn classify_pass_evidence(report: &str, evidence: &[String]) -> PassEvidenceClass {
    let mut class = PassEvidenceClass::default();
    let mut saw_evidence = false;
    for raw in evidence {
        let line = raw.trim();
        if line.is_empty() || line.eq_ignore_ascii_case("none") {
            continue;
        }
        saw_evidence = true;
        let lowered = line.to_ascii_lowercase();
        if evidence_line_is_trusted_external(&lowered) {
            class.trusted_external = true;
            continue;
        }
        if evidence_line_is_worker_owned_local(&lowered) {
            class.workspace_local = true;
            continue;
        }
        if evidence_line_is_prose_only(&lowered) {
            class.prose_only = true;
            continue;
        }
        if evidence_line_is_direct_inspection(&lowered) {
            class.direct = true;
        } else {
            class.prose_only = true;
        }
    }
    if !saw_evidence && !report.trim().is_empty() {
        class.prose_only = true;
    }
    class
}

fn evidence_line_is_trusted_external(line: &str) -> bool {
    contains_any(
        line,
        &[
            "trusted external",
            "external validator",
            "immutable validator",
            "official validator",
            "external system of record",
            "system of record",
            "accepted send proof",
            "accepted outbound",
            "delivery receipt",
            "provider accepted",
            "remote api accepted",
            "ci status",
            "github check",
            "deployment status",
        ],
    )
}

fn evidence_line_is_worker_owned_local(line: &str) -> bool {
    contains_any(
        line,
        &[
            "run-tests.sh",
            "run tests",
            "run-tests",
            "pytest",
            "cargo test",
            "npm test",
            "pnpm test",
            "yarn test",
            "go test",
            "make test",
            "workspace test",
            "workspace verification",
            "local verification",
            "worker-owned",
            "worker owned",
            "test suite passes",
            "tests pass",
            "tests passing",
            "all tests pass",
        ],
    )
}

fn evidence_line_is_prose_only(line: &str) -> bool {
    contains_any(
        line,
        &[
            "worker said",
            "worker claims",
            "assistant said",
            "reported that",
            "claims look",
            "seems correct",
            "looks good",
            "prose",
        ],
    )
}

fn evidence_line_is_direct_inspection(line: &str) -> bool {
    contains_any(
        line,
        &[
            "inspected",
            "read ",
            "opened ",
            "file content",
            "artifact content",
            "contains",
            "matches task",
            "matches request",
            "matches assignment",
            "database record",
            "sqlite record",
            "runtime record",
            "communication record",
            "message record",
            "ticket record",
            "durable state",
            "live surface",
            "http ",
            "https://",
            "status=",
            "exists as regular file",
            "required artifact",
            "required output",
        ],
    )
}

fn report_mentions_review_access_blocker(report: &str) -> bool {
    let lowered = report.to_ascii_lowercase();
    contains_any(
        &lowered,
        &[
            "sandbox restriction",
            "sandbox restrictions",
            "blocked by sandbox",
            "blocking all filesystem",
            "filesystem and database inspection",
            "could not inspect",
            "couldn't inspect",
            "unable to inspect",
            "could not access",
            "couldn't access",
            "unable to access",
            "tools unavailable",
            "tool access",
            "without filesystem access",
            "without database access",
            "read-only inspection was unavailable",
        ],
    )
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
                || trimmed.starts_with("PASS_PROOF:")
                || trimmed.starts_with("FAILED_GATES:")
                || trimmed.starts_with("OPEN_ITEMS:")
                || trimmed.starts_with("PIPELINE_RESOLUTION:")
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
                    | "PASS_PROOF:"
                    | "PIPELINE_RESOLUTION:"
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
                    | "PASS_PROOF:"
                    | "PIPELINE_RESOLUTION:"
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

fn push_unique_item(items: &mut Vec<String>, candidate: &str) {
    if !items.iter().any(|existing| existing == candidate) {
        items.push(candidate.to_string());
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
    fn requires_review_for_workspace_backed_queue_task() {
        let request = CompletionReviewRequest {
            task_goal: "Implement the requested CLI behavior.".to_string(),
            task_prompt: "Work only inside this workspace: /tmp/ctox-workspaces/task-001\nUse this workspace as the current directory for the task.\n\nImplement a command line tool and verify it.".to_string(),
            preview: "Workspace task 001: cli-tool".to_string(),
            source_label: "queue".to_string(),
            owner_visible: false,
            workspace_root: "/tmp/ctox-workspaces/task-001".to_string(),
            ..CompletionReviewRequest::default()
        };
        let (required, score, reasons) =
            assess_review_requirement(&request, "Done. Implemented and verified.");
        assert!(required);
        assert!(score >= 3);
        assert!(reasons
            .iter()
            .any(|reason| reason == "workspace_backed_queue_task"));
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
            task_goal: "Reply with the requested QR code and status proof.".to_string(),
            task_prompt: "Prepare a reviewed founder reply with the requested attachment."
                .to_string(),
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
        assert!(rendered.contains("Original task contract:"));
        assert!(rendered.contains("Deterministic review evidence:"));
        assert!(rendered.contains("read-only tools"));
        assert!(rendered.contains("inspect explicit artifact paths and attachments directly"));
        assert!(rendered.contains("claims to audit, not as proof"));
        assert!(rendered.contains("audit worker self-reports explicitly"));
        assert!(
            rendered.contains("current Workspace root above as the only authoritative workspace")
        );
    }

    #[test]
    fn review_system_prompt_requires_self_report_verification() {
        assert!(REVIEW_SYSTEM_PROMPT.contains(
            "Treat every worker self-report, completion summary, status sentence, and claimed test result as an unverified lead only."
        ));
        assert!(REVIEW_SYSTEM_PROMPT.contains("worker self-reports must be checked, not trusted"));
        assert!(REVIEW_SYSTEM_PROMPT
            .contains("must be independently verified before it can support PASS"));
    }

    #[test]
    fn founder_review_prompt_explicitly_reviews_the_mail_artifact() {
        let request = CompletionReviewRequest {
            task_goal: "Reply with the requested QR code and status proof.".to_string(),
            task_prompt: "Prepare a reviewed founder reply with the requested attachment."
                .to_string(),
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
            artifact_channel: "email".to_string(),
            artifact_to: vec!["o.schaefers@gmx.net".to_string()],
            artifact_cc: vec!["michael.welsch@metric-space.ai".to_string()],
            artifact_subject: "Jami Setup und QR-Code".to_string(),
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
            deterministic_evidence: vec![
                "Recent same-thread email: inbound founder requested QR code".to_string(),
                "Recent meeting evidence: none found".to_string(),
            ],
        };
        let rendered = build_review_prompt(&request, &["founder_communication".to_string()]);
        assert!(rendered.contains("Artifact kind: reviewed_outbound_email_draft"));
        assert!(rendered.contains("judge the outbound draft itself as the artifact under review"));
        assert!(rendered.contains("Artifact action: reply"));
        assert!(rendered.contains("Artifact to: o.schaefers@gmx.net"));
        assert!(rendered.contains("Artifact cc: michael.welsch@metric-space.ai"));
        assert!(rendered.contains("Artifact subject: Jami Setup und QR-Code"));
        assert!(rendered.contains(
            "Artifact attachments: /srv/runtime/communication/artifacts/jami/ctox-jami-setup.pdf"
        ));
        assert!(rendered.contains("Required deliverables: qr_code"));
        assert!(rendered
            .contains("Artifact commitments: Today, 24.04.2026, send an update by 20:00 UTC."));
        assert!(rendered.contains(
            "Commitment backing: kunstmen founder update 20utc @ 2026-04-24T20:00:00+00:00"
        ));
        assert!(rendered.contains("Recent same-thread email: inbound founder requested QR code"));
        assert!(rendered.contains("judge the full mail action, not just the prose"));
        assert!(rendered.contains("recipients, cc list, subject, attachments"));
        assert!(rendered.contains("still relevant against the latest communication"));
        assert!(rendered.contains("sends the result of completed work"));
        assert!(rendered.contains("latest communication across all channels"));
        assert!(rendered.contains("recipient-appropriate, concise"));
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
    fn external_chat_review_prompt_requires_pipeline_backed_quick_response() {
        let request = CompletionReviewRequest {
            task_goal: "Acknowledge Jill's Teams scraping request.".to_string(),
            task_prompt: "Create a durable work item, then draft a short Teams acknowledgement."
                .to_string(),
            preview: "[Teams-Nachricht eingegangen] Jill".to_string(),
            source_label: "teams".to_string(),
            owner_visible: true,
            conversation_id: 88,
            thread_key: "teams:inf.yoda@example.test::chat::jill".to_string(),
            runtime_db_path: "/srv/runtime/ctox.sqlite3".to_string(),
            artifact_text: "Verstanden, ich lege das als Aufgabe an und prüfe die Seite."
                .to_string(),
            artifact_action: Some("external_chat_quick_response".to_string()),
            artifact_channel: "teams".to_string(),
            deterministic_evidence: vec![
                "External chat work backing for thread `teams:inf.yoda@example.test::chat::jill`: queue_open=1, plan_open=0, self_work_open=0.".to_string(),
            ],
            ..CompletionReviewRequest::default()
        };
        let (required, _score, reasons) = assess_review_requirement(
            &request,
            "Verstanden, ich lege das als Aufgabe an und prüfe die Seite.",
        );
        assert!(required);
        assert!(reasons
            .iter()
            .any(|reason| reason == "external_chat_quick_response"));
        let rendered = build_review_prompt(&request, &reasons);
        assert!(rendered.contains("Artifact kind: external_chat_quick_response"));
        assert!(rendered.contains("Artifact channel: teams"));
        assert!(rendered.contains("External chat quick-response gate"));
        assert!(rendered.contains("durable pipeline backing exists first"));
        assert!(rendered.contains("communication-to-pipeline delta"));
        assert!(rendered.contains("new task, update existing task, merge duplicate, extend scope"));
        assert!(rendered.contains("nothing may remain unresolved"));
        assert!(rendered.contains("do not require the final work result"));
        assert!(rendered.contains("queue_open=1"));
    }

    #[test]
    fn review_harness_contract_is_finite_isolated_and_simple_compaction_only() {
        assert_eq!(REVIEW_TIMEOUT_SECS, 900);
        assert_eq!(REVIEW_MAX_LEGS, 3);
        assert!(REVIEW_SYSTEM_PROMPT.contains("Operate in strict read-only verification mode."));
        assert!(REVIEW_SYSTEM_PROMPT.contains("normal review compaction is disabled"));
        assert!(REVIEW_SYSTEM_PROMPT.contains("emit a review handoff instead of compacting"));
        assert!(
            !REVIEW_SYSTEM_PROMPT.contains("LCM"),
            "review prompt must not ask the reviewer to run the mission LCM harness"
        );

        let request = CompletionReviewRequest {
            preview: "Review harness proof".to_string(),
            source_label: "queue".to_string(),
            workspace_root: "/tmp/review-proof".to_string(),
            runtime_db_path: "/tmp/ctox.sqlite3".to_string(),
            review_skill_path: "/tmp/external-review/SKILL.md".to_string(),
            ..CompletionReviewRequest::default()
        };
        let handoff =
            build_review_handoff_prompt(&request, "Verified files; still need service log.");
        assert!(handoff.contains("Prior handoff:"));
        assert!(handoff.contains("Verified files; still need service log."));
        assert!(handoff.contains("read-only inspection only"));
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
    fn parses_structured_pipeline_resolution() {
        let outcome = parse_review_report(
            4,
            vec!["external_chat_quick_response".to_string()],
            "VERDICT: PASS\nSUMMARY: ack is backed.\nPIPELINE_RESOLUTION: action=merge_duplicate | target=queue:system::intersolar | rationale=\"Merged Jill's duplicate into the open scraper task.\"\nEVIDENCE:\n- queue row exists\n",
        );
        let resolution = outcome
            .pipeline_resolution
            .expect("pipeline resolution should parse");
        assert_eq!(resolution.action, PipelineResolutionAction::MergeDuplicate);
        assert_eq!(resolution.target, "queue:system::intersolar");
        assert!(resolution.rationale.contains("duplicate"));
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
        let report = "VERDICT: PASS\nSUMMARY: looks good.\nPASS_PROOF: direct\nEVIDENCE:\n- inspected artifact => matches request\nDISPOSITION: SEND\n";
        let outcome = parse_review_report(0, vec![], report);
        assert_eq!(outcome.disposition, ReviewDisposition::Send);
    }

    #[test]
    fn required_pass_without_evidence_is_downgraded_to_partial() {
        let report =
            "VERDICT: PASS\nMISSION_STATE: HEALTHY\nSUMMARY: claims look fine.\nEVIDENCE:\n- none\n";
        let outcome = parse_review_report(4, vec!["runtime_or_infra_change".to_string()], report);

        assert_eq!(outcome.verdict, ReviewVerdict::Partial);
        assert!(outcome.summary.contains("PASS was rejected"));
        assert!(outcome
            .failed_gates
            .iter()
            .any(|gate| gate.contains("lacked usable direct evidence")));
        assert!(outcome.requires_follow_up());
    }

    #[test]
    fn required_pass_with_sandbox_blocker_is_downgraded_to_partial() {
        let report = "VERDICT: PASS\nMISSION_STATE: HEALTHY\nSUMMARY: sandbox restrictions prevented filesystem inspection, but it seems okay.\nEVIDENCE:\n- worker said tests passed\n";
        let outcome = parse_review_report(4, vec!["runtime_or_infra_change".to_string()], report);

        assert_eq!(outcome.verdict, ReviewVerdict::Partial);
        assert!(outcome
            .open_items
            .iter()
            .any(|item| item.contains("Re-run review with direct filesystem")));
    }

    #[test]
    fn required_pass_with_direct_evidence_stays_pass() {
        let report = "VERDICT: PASS\nMISSION_STATE: HEALTHY\nSUMMARY: verified directly.\nPASS_PROOF: direct\nEVIDENCE:\n- inspected required artifact content => matches task contract\nDISPOSITION: SEND\n";
        let outcome = parse_review_report(4, vec!["runtime_or_infra_change".to_string()], report);

        assert_eq!(outcome.verdict, ReviewVerdict::Pass);
        assert_eq!(outcome.evidence.len(), 1);
        assert!(!outcome.requires_follow_up());
    }

    #[test]
    fn required_pass_with_workspace_local_proof_is_downgraded_to_partial() {
        let report = "VERDICT: PASS\nMISSION_STATE: HEALTHY\nSUMMARY: tests pass.\nPASS_PROOF: workspace_local\nEVIDENCE:\n- bash run-tests.sh => exit 0\n";
        let outcome = parse_review_report(4, vec!["runtime_or_infra_change".to_string()], report);

        assert_eq!(outcome.verdict, ReviewVerdict::Partial);
        assert!(outcome
            .failed_gates
            .iter()
            .any(|gate| gate.contains("direct or trusted external proof")));
        assert!(outcome.requires_follow_up());
    }

    #[test]
    fn required_pass_without_pass_proof_is_downgraded_to_partial() {
        let report = "VERDICT: PASS\nMISSION_STATE: HEALTHY\nSUMMARY: checked.\nEVIDENCE:\n- inspected artifact => looked correct\n";
        let outcome = parse_review_report(4, vec!["runtime_or_infra_change".to_string()], report);

        assert_eq!(outcome.verdict, ReviewVerdict::Partial);
        assert!(outcome.summary.contains("PASS_PROOF was `missing`"));
        assert!(outcome.requires_follow_up());
    }

    #[test]
    fn required_pass_with_direct_label_but_only_workspace_local_evidence_is_downgraded() {
        let report = "VERDICT: PASS\nMISSION_STATE: HEALTHY\nSUMMARY: tests pass.\nPASS_PROOF: direct\nEVIDENCE:\n- bash run-tests.sh => all tests pass\n";
        let outcome = parse_review_report(4, vec!["runtime_or_infra_change".to_string()], report);

        assert_eq!(outcome.verdict, ReviewVerdict::Partial);
        assert!(outcome.summary.contains("PASS_PROOF was declared direct"));
        assert!(outcome.requires_follow_up());
    }

    #[test]
    fn trusted_external_requires_external_system_of_record_evidence() {
        let report = "VERDICT: PASS\nMISSION_STATE: HEALTHY\nSUMMARY: tests pass.\nPASS_PROOF: trusted_external\nEVIDENCE:\n- pytest => all tests pass\n";
        let outcome = parse_review_report(4, vec!["runtime_or_infra_change".to_string()], report);

        assert_eq!(outcome.verdict, ReviewVerdict::Partial);
        assert!(outcome
            .summary
            .contains("PASS_PROOF was declared trusted_external"));
    }

    #[test]
    fn trusted_external_with_accepted_send_proof_stays_pass() {
        let report = "VERDICT: PASS\nMISSION_STATE: HEALTHY\nSUMMARY: provider accepted the message.\nPASS_PROOF: trusted_external\nEVIDENCE:\n- accepted send proof from provider system of record => message id msg_123\n";
        let outcome = parse_review_report(4, vec!["communication".to_string()], report);

        assert_eq!(outcome.verdict, ReviewVerdict::Pass);
        assert!(!outcome.requires_follow_up());
    }

    #[test]
    fn unknown_disposition_token_falls_back_to_send() {
        let report = "VERDICT: FAIL\nSUMMARY: weird.\nDISPOSITION: MAYBE\n";
        let outcome = parse_review_report(0, vec![], report);
        assert_eq!(outcome.disposition, ReviewDisposition::Send);
    }
}
