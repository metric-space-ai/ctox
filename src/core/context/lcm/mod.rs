mod mission_state;
mod runtime_support;
pub(crate) use mission_state::drain_pending_mission_state_clobbers;
#[cfg(test)]
pub(crate) use mission_state::drain_pending_mission_state_clobbers_for_test;
use mission_state::{
    apply_canonical_focus_diff_to_mission_state, apply_imported_focus_diff_controls,
    import_legacy_mission_state, load_mission_state_with, load_mission_states_with,
    map_mission_claim_row, map_strategic_directive_row, map_verification_run_row,
    persist_mission_state_with, render_focus_continuity_from_record, OwnerIntentClearGuard,
};
pub use mission_state::{
    count_open_closure_blocking_claims, drain_pending_mission_state_clobber_events_to_governance,
    ClosureConfidence, ContinuationMode, MissionStateFields, MissionStatus, TriggerIntensity,
};
use mission_state::{focus_semantic_conflicts_local, normalize_mission_text};
pub(crate) use runtime_support::seed_mission_state_for_queue_with;
use runtime_support::*;
pub use runtime_support::{
    run_add_assistant_turn, run_add_message, run_begin_worker_attempt_finalization, run_compact,
    run_context_retrieve, run_continuity_apply, run_continuity_build_prompt,
    run_continuity_forgotten, run_continuity_full_replace, run_continuity_init, run_continuity_log,
    run_continuity_rebuild, run_continuity_show, run_continuity_string_replace, run_describe,
    run_dump, run_ensure_worker_attempt_assistant_message, run_expand, run_fixture, run_grep,
    run_init, run_mark_worker_attempt_effects_completed,
    run_mark_worker_attempt_recovery_effects_applied, run_prepare_task_execution_review,
    run_record_task_execution_activity, run_record_task_execution_plan,
    run_record_worker_attempt_artifact_check, run_recoverable_worker_attempt,
    run_refresh_continuity, run_secret_rewrite, run_set_task_execution_review_status,
    run_show_continuity, run_task_execution_progress, run_task_execution_progress_for_task,
    run_terminalize_worker_attempt, run_worker_attempt,
};

use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::atomic::AtomicU64;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const DEFAULT_CONTEXT_THRESHOLD: f64 = 0.75;
const DEFAULT_MIN_COMPACTION_TOKENS: i64 = 12_288;
const DEFAULT_FRESH_TAIL_COUNT: usize = 8;
const DEFAULT_LEAF_CHUNK_TOKENS: i64 = 20_000;
const DEFAULT_LEAF_TARGET_TOKENS: usize = 600;
const DEFAULT_CONDENSED_TARGET_TOKENS: usize = 900;
const DEFAULT_LEAF_MIN_FANOUT: usize = 4;
const DEFAULT_CONDENSED_MIN_FANOUT: usize = 3;
const DEFAULT_MAX_ROUNDS: usize = 6;
const FALLBACK_MAX_CHARS: usize = 512 * 4;
const EMPTY_MISSION_SPLIT_BRAIN_MIGRATION: &str = "i070_empty_active_continuous_mission_state_v1";
const CONDENSED_MIN_INPUT_RATIO: f64 = 0.1;
const MAX_SUMMARY_RATIO: f64 = 0.8;
#[cfg(test)]
static TEMP_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SummaryKind {
    Leaf,
    Condensed,
}

impl SummaryKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Leaf => "leaf",
            Self::Condensed => "condensed",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextItemType {
    Message,
    Summary,
}

impl ContextItemType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Summary => "summary",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageRecord {
    pub message_id: i64,
    pub conversation_id: i64,
    pub seq: i64,
    pub role: String,
    pub content: String,
    pub token_count: i64,
    pub created_at: String,
    /// F3: structured agent outcome for assistant rows. Always `None` for
    /// non-assistant rows (`user`, `system`, etc.). Replaces string-scraping
    /// of `"Status: \`blocked\`"` text-status replies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_outcome: Option<String>,
}

/// F3: structured outcome of a single agent turn. Persisted on the
/// corresponding assistant message row in `messages.agent_outcome` so that
/// downstream consumers (mission watchdog, founder-send pipeline, status
/// snapshots) can branch on the outcome without scraping the reply body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentOutcome {
    /// The turn ran to completion and produced a real reply.
    Success,
    /// The turn hit the configured turn time budget.
    TurnTimeout,
    /// The turn aborted with a runtime / harness execution error.
    ExecutionError,
    /// The turn was rejected deterministically because the rendered prompt could
    /// not fit the context/token budget (turnloop-5: exact-token overflow). A
    /// distinct class so process-mining and the agent-failure counter can tell a
    /// repeated context-budget reject apart from a generic backend crash.
    ContextRejected,
    /// The turn was aborted by the harness (e.g. mission state invariant).
    Aborted,
    /// The turn was cancelled before it could finish (operator stop).
    Cancelled,
}

impl AgentOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentOutcome::Success => "Success",
            AgentOutcome::TurnTimeout => "TurnTimeout",
            AgentOutcome::ExecutionError => "ExecutionError",
            AgentOutcome::ContextRejected => "ContextRejected",
            AgentOutcome::Aborted => "Aborted",
            AgentOutcome::Cancelled => "Cancelled",
        }
    }

    /// True when this outcome represents a non-success that the watchdog
    /// should count toward the agent-failure backoff threshold.
    pub fn is_agent_failure(self) -> bool {
        !matches!(self, AgentOutcome::Success)
    }

    pub fn from_token(value: &str) -> Option<Self> {
        match value {
            "Success" => Some(AgentOutcome::Success),
            "TurnTimeout" => Some(AgentOutcome::TurnTimeout),
            "ExecutionError" => Some(AgentOutcome::ExecutionError),
            "ContextRejected" => Some(AgentOutcome::ContextRejected),
            "Aborted" => Some(AgentOutcome::Aborted),
            "Cancelled" => Some(AgentOutcome::Cancelled),
            _ => None,
        }
    }
}

/// Durable state of one worker-attempt finalization. The row is the recovery
/// marker across process crashes; `finalizing` and terminal rows with
/// `effects_completed = false` are resumed instead of invoking the model again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerAttemptRecord {
    pub attempt_id: String,
    pub work_key: String,
    pub conversation_id: i64,
    pub source_label: String,
    pub status: String,
    pub agent_outcome: AgentOutcome,
    pub reply_text: String,
    pub error_text: Option<String>,
    pub reply_message_id: Option<i64>,
    pub artifact_checked_at: Option<String>,
    pub artifact_check_accepted: Option<bool>,
    pub artifact_check_details: Option<String>,
    pub resumable: bool,
    pub effects_completed: bool,
    pub queue_effects_applied_at: Option<String>,
    pub recovery_effects_applied_at: Option<String>,
    pub finalization_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub terminal_at: Option<String>,
}

/// One user-visible step in the durable execution plan owned by CTOX.
/// Labels are persisted, while model reasoning text is deliberately excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskExecutionPlanStepInput {
    pub label: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct TaskExecutionPlanUpdate<'a> {
    pub work_key: &'a str,
    pub task_id: &'a str,
    pub command_id: &'a str,
    pub attempt_id: &'a str,
    pub explanation: Option<&'a str>,
    pub steps: &'a [TaskExecutionPlanStepInput],
}

#[derive(Debug, Clone)]
pub struct TaskExecutionActivityInput<'a> {
    pub activity_id: &'a str,
    pub work_key: &'a str,
    pub task_id: &'a str,
    pub command_id: &'a str,
    pub attempt_id: &'a str,
    pub kind: &'a str,
    /// `true` attributes the activity to the current in-progress step.
    /// Planning tool calls intentionally remain task-level activity.
    pub attribute_to_current_step: bool,
}

#[derive(Debug, Clone)]
pub struct WorkerAttemptFinalizationInput<'a> {
    pub attempt_id: &'a str,
    pub work_key: &'a str,
    pub conversation_id: i64,
    pub source_label: &'a str,
    pub agent_outcome: AgentOutcome,
    pub reply_text: &'a str,
    pub error_text: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerAttemptTerminalStatus {
    Succeeded,
    Failed,
    TimedOut,
}

impl WorkerAttemptTerminalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
        }
    }
}

fn map_worker_attempt_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkerAttemptRecord> {
    let outcome_token: String = row.get(5)?;
    let agent_outcome = AgentOutcome::from_token(&outcome_token).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid worker attempt outcome {outcome_token}"),
            )),
        )
    })?;
    Ok(WorkerAttemptRecord {
        attempt_id: row.get(0)?,
        work_key: row.get(1)?,
        conversation_id: row.get(2)?,
        source_label: row.get(3)?,
        status: row.get(4)?,
        agent_outcome,
        reply_text: row.get(6)?,
        error_text: row.get(7)?,
        reply_message_id: row.get(8)?,
        artifact_checked_at: row.get(9)?,
        artifact_check_accepted: row.get::<_, Option<i64>>(10)?.map(|value| value != 0),
        artifact_check_details: row.get(11)?,
        resumable: row.get::<_, i64>(12)? != 0,
        effects_completed: row.get::<_, i64>(13)? != 0,
        queue_effects_applied_at: row.get(14)?,
        recovery_effects_applied_at: row.get(15)?,
        finalization_error: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
        terminal_at: row.get(19)?,
    })
}

const WORKER_ATTEMPT_SELECT: &str = "SELECT attempt_id, work_key, conversation_id, source_label, status, agent_outcome, reply_text, error_text, reply_message_id, artifact_checked_at, artifact_check_accepted, artifact_check_details, resumable, effects_completed, queue_effects_applied_at, recovery_effects_applied_at, finalization_error, created_at, updated_at, terminal_at FROM worker_attempt_finalizations";

#[derive(Debug, Clone, Serialize)]
pub struct SummaryRecord {
    pub summary_id: String,
    pub conversation_id: i64,
    pub kind: SummaryKind,
    pub depth: i64,
    pub content: String,
    pub token_count: i64,
    pub descendant_count: i64,
    pub descendant_token_count: i64,
    pub source_message_token_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SummarySubtreeNode {
    pub summary_id: String,
    pub parent_summary_id: Option<String>,
    pub depth_from_root: i64,
    pub kind: SummaryKind,
    pub depth: i64,
    pub token_count: i64,
    pub descendant_count: i64,
    pub descendant_token_count: i64,
    pub source_message_token_count: i64,
    pub child_count: i64,
    pub path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DescribeSummary {
    pub summary: SummaryRecord,
    pub parent_ids: Vec<String>,
    pub child_ids: Vec<String>,
    pub message_ids: Vec<i64>,
    pub subtree: Vec<SummarySubtreeNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DescribeResult {
    Summary(DescribeSummary),
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageSearchResult {
    pub message_id: i64,
    pub conversation_id: i64,
    pub role: String,
    pub snippet: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SummarySearchResult {
    pub summary_id: String,
    pub conversation_id: i64,
    pub kind: SummaryKind,
    pub snippet: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GrepResult {
    pub messages: Vec<MessageSearchResult>,
    pub summaries: Vec<SummarySearchResult>,
    pub total_matches: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpandChild {
    pub summary_id: String,
    pub kind: SummaryKind,
    pub content: String,
    pub token_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpandMessage {
    pub message_id: i64,
    pub role: String,
    pub content: String,
    pub token_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpandResult {
    pub children: Vec<ExpandChild>,
    pub messages: Vec<ExpandMessage>,
    pub estimated_tokens: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextItemSnapshot {
    pub ordinal: i64,
    pub item_type: ContextItemType,
    pub message_id: Option<i64>,
    pub summary_id: Option<String>,
    pub seq: i64,
    pub depth: i64,
    pub token_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LcmSnapshot {
    pub conversation_id: i64,
    pub messages: Vec<MessageRecord>,
    pub summaries: Vec<SummaryRecord>,
    pub context_items: Vec<ContextItemSnapshot>,
    pub summary_edges: Vec<(String, String)>,
    pub summary_messages: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContinuityRevision {
    pub revision_id: String,
    pub conversation_id: i64,
    pub narrative: String,
    pub anchors: String,
    pub focus: String,
    pub source_summary_ids: Vec<String>,
    pub source_message_ids: Vec<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecretRewriteResult {
    pub rewrite_id: String,
    pub conversation_id: i64,
    pub secret_scope: String,
    pub secret_name: String,
    pub replacement_text: String,
    pub message_rows_updated: usize,
    pub summary_rows_updated: usize,
    pub continuity_commit_rows_updated: usize,
    pub continuity_revision_rows_updated: usize,
    pub mission_state_rows_updated: usize,
    pub verification_rows_updated: usize,
    pub claim_rows_updated: usize,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContinuityKind {
    Narrative,
    Anchors,
    Focus,
}

impl ContinuityKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Narrative => "narrative",
            Self::Anchors => "anchors",
            Self::Focus => "focus",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "narrative" => Ok(Self::Narrative),
            "anchors" => Ok(Self::Anchors),
            "focus" => Ok(Self::Focus),
            other => anyhow::bail!("unsupported continuity kind: {other}"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ContinuityDocumentState {
    pub conversation_id: i64,
    pub kind: ContinuityKind,
    pub head_commit_id: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContinuityCommitRecord {
    pub commit_id: String,
    pub conversation_id: i64,
    pub kind: ContinuityKind,
    pub parent_commit_id: Option<String>,
    pub diff_text: String,
    pub rendered_text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContinuityForgottenEntry {
    pub commit_id: String,
    pub conversation_id: i64,
    pub kind: ContinuityKind,
    pub line: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContinuityShowAll {
    pub conversation_id: i64,
    pub narrative: ContinuityDocumentState,
    pub anchors: ContinuityDocumentState,
    pub focus: ContinuityDocumentState,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionStateRecord {
    pub conversation_id: i64,
    pub mission: String,
    pub mission_status: String,
    pub continuation_mode: String,
    pub trigger_intensity: String,
    pub blocker: String,
    pub next_slice: String,
    pub done_gate: String,
    pub closure_confidence: String,
    pub is_open: bool,
    pub allow_idle: bool,
    pub focus_head_commit_id: String,
    pub last_synced_at: String,
    pub watcher_last_triggered_at: Option<String>,
    pub watcher_trigger_count: i64,
    /// F2: number of consecutive agent-failure outcomes for this mission.
    /// Reset to 0 on a successful agent turn; incremented on
    /// `AgentOutcome::TurnTimeout`, `ExecutionError`, `Aborted`.
    #[serde(default)]
    pub agent_failure_count: i64,
    /// F2: structured reason set when the watchdog deferred the mission
    /// (e.g. `agent_failure_threshold`). `None` for active missions.
    #[serde(default)]
    pub deferred_reason: Option<String>,
    /// Number of consecutive rewrite-only review iterations that failed to
    /// converge for this mission. Reset on a successful approval; bumped on
    /// each non-converging rewrite turn. Once it crosses the configured
    /// threshold the mission is deferred with reason
    /// `rewrite_failure_threshold`.
    #[serde(default)]
    pub rewrite_failure_count: i64,
}

/// govrec-5: outcome of clearing a mission's agent-failure deferral. Carries the
/// pre-reset `deferred_reason` (already `None` on `record`) so the caller can
/// emit a one-sided-defer's matching recovery governance event exactly once, and
/// `recovered_at` (a fresh ms timestamp) as the per-recovery idempotence
/// discriminator — real recovery cycles are seconds apart, so it distinguishes
/// each cycle while staying stable for a single reset (the dead
/// `watcher_trigger_count` could not, as it never increments in production).
#[derive(Debug, Clone, Serialize)]
pub struct MissionFailureReset {
    pub record: MissionStateRecord,
    pub previous_deferred_reason: Option<String>,
    pub recovered_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionStateRepairOutcome {
    pub mission_state: MissionStateRecord,
    pub previous_focus_head_commit_id: String,
    pub focus_head_commit_id: String,
    pub focus_repaired: bool,
    pub reopened_for_open_runtime_work: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerificationRunRecord {
    pub run_id: String,
    pub conversation_id: i64,
    pub source_label: String,
    pub goal: String,
    pub preview: String,
    pub result_excerpt: String,
    pub blocker: Option<String>,
    pub review_required: bool,
    pub review_verdict: String,
    pub review_summary: String,
    pub review_score: i64,
    pub review_reasons: Vec<String>,
    pub report_excerpt: String,
    pub raw_report: String,
    pub mission_state: String,
    pub failed_gates: Vec<String>,
    pub semantic_findings: Vec<String>,
    pub open_items: Vec<String>,
    pub evidence: Vec<String>,
    pub handoff: Option<String>,
    pub claim_count: i64,
    pub open_claim_count: i64,
    pub closure_blocking_claim_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategicDirectiveRecord {
    pub directive_id: String,
    pub conversation_id: i64,
    pub thread_key: Option<String>,
    pub directive_kind: String,
    pub title: String,
    pub body_text: String,
    pub status: String,
    pub revision: i64,
    pub previous_directive_id: Option<String>,
    pub author: String,
    pub decided_by: Option<String>,
    pub decision_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategySnapshot {
    pub conversation_id: i64,
    pub thread_key: Option<String>,
    pub active_vision: Option<StrategicDirectiveRecord>,
    pub active_mission: Option<StrategicDirectiveRecord>,
    pub directives: Vec<StrategicDirectiveRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionClaimRecord {
    pub claim_key: String,
    pub conversation_id: i64,
    pub last_run_id: String,
    pub claim_kind: String,
    pub claim_status: String,
    pub blocks_closure: bool,
    pub subject: String,
    pub summary: String,
    pub evidence_summary: String,
    pub recheck_policy: String,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionAssuranceSnapshot {
    pub conversation_id: i64,
    pub latest_run: Option<VerificationRunRecord>,
    pub open_claims: Vec<MissionClaimRecord>,
    pub closure_blocking_claims: Vec<MissionClaimRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContinuityPromptPayload {
    pub conversation_id: i64,
    pub kind: ContinuityKind,
    pub current_document: String,
    pub recent_messages: Vec<String>,
    pub recent_summaries: Vec<String>,
    pub forgotten_lines: Vec<String>,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExplicitAnchorLiteral {
    literal: String,
    source_ref: String,
    observed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactionDecision {
    pub should_compact: bool,
    pub reason: String,
    pub current_tokens: i64,
    pub threshold: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactionResult {
    pub action_taken: bool,
    pub tokens_before: i64,
    pub tokens_after: i64,
    pub created_summary_ids: Vec<String>,
    pub rounds: usize,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FixtureMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FixtureGrep {
    pub scope: String,
    pub mode: String,
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FixtureExpand {
    pub summary_id: Option<String>,
    pub depth: Option<usize>,
    pub include_messages: Option<bool>,
    pub token_cap: Option<i64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LcmFixture {
    pub conversation_id: i64,
    pub token_budget: i64,
    pub force_compact: Option<bool>,
    pub config: Option<LcmFixtureConfig>,
    pub messages: Vec<FixtureMessage>,
    pub grep_queries: Option<Vec<FixtureGrep>>,
    pub expand_queries: Option<Vec<FixtureExpand>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LcmFixtureConfig {
    pub context_threshold: Option<f64>,
    pub min_compaction_tokens: Option<i64>,
    pub fresh_tail_count: Option<usize>,
    pub leaf_chunk_tokens: Option<i64>,
    pub leaf_target_tokens: Option<usize>,
    pub condensed_target_tokens: Option<usize>,
    pub leaf_min_fanout: Option<usize>,
    pub condensed_min_fanout: Option<usize>,
    pub max_rounds: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixtureRunOutput {
    pub compaction: CompactionResult,
    pub snapshot: LcmSnapshot,
    pub grep_results: Vec<GrepResult>,
    pub expand_results: Vec<ExpandResult>,
}

#[derive(Debug, Clone)]
pub struct LcmConfig {
    pub context_threshold: f64,
    pub min_compaction_tokens: i64,
    pub fresh_tail_count: usize,
    pub leaf_chunk_tokens: i64,
    pub leaf_target_tokens: usize,
    pub condensed_target_tokens: usize,
    pub leaf_min_fanout: usize,
    pub condensed_min_fanout: usize,
    pub max_rounds: usize,
}

impl Default for LcmConfig {
    fn default() -> Self {
        Self {
            context_threshold: DEFAULT_CONTEXT_THRESHOLD,
            min_compaction_tokens: DEFAULT_MIN_COMPACTION_TOKENS,
            fresh_tail_count: DEFAULT_FRESH_TAIL_COUNT,
            leaf_chunk_tokens: DEFAULT_LEAF_CHUNK_TOKENS,
            leaf_target_tokens: DEFAULT_LEAF_TARGET_TOKENS,
            condensed_target_tokens: DEFAULT_CONDENSED_TARGET_TOKENS,
            leaf_min_fanout: DEFAULT_LEAF_MIN_FANOUT,
            condensed_min_fanout: DEFAULT_CONDENSED_MIN_FANOUT,
            max_rounds: DEFAULT_MAX_ROUNDS,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum GrepMode {
    Regex,
    FullText,
}

impl GrepMode {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "regex" => Ok(Self::Regex),
            "full_text" | "full-text" | "fts" => Ok(Self::FullText),
            other => anyhow::bail!("unsupported grep mode: {other}"),
        }
    }
}

impl LcmConfig {
    fn compaction_threshold(&self, token_budget: i64) -> i64 {
        if token_budget <= 0 {
            return 0;
        }
        let percent_threshold = ((token_budget as f64) * self.context_threshold).floor() as i64;
        percent_threshold
            .max(self.min_compaction_tokens.min(token_budget))
            .max(0)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum GrepScope {
    Messages,
    Summaries,
    Both,
}

impl GrepScope {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "messages" => Ok(Self::Messages),
            "summaries" => Ok(Self::Summaries),
            "both" => Ok(Self::Both),
            other => anyhow::bail!("unsupported grep scope: {other}"),
        }
    }
}

#[derive(Debug, Clone)]
struct ContextEntry {
    ordinal: i64,
    item_type: ContextItemType,
    message_id: Option<i64>,
    summary_id: Option<String>,
    seq: i64,
    depth: i64,
    token_count: i64,
}

pub trait Summarizer {
    fn summarize(
        &self,
        kind: SummaryKind,
        depth: i64,
        lines: &[String],
        target_tokens: usize,
    ) -> Result<String>;
}

struct EscalatedSummary {
    content: String,
}

pub struct HeuristicSummarizer;

impl Summarizer for HeuristicSummarizer {
    fn summarize(
        &self,
        kind: SummaryKind,
        depth: i64,
        lines: &[String],
        target_tokens: usize,
    ) -> Result<String> {
        let mut header = match kind {
            SummaryKind::Leaf => format!("LCM leaf summary at depth {depth}:"),
            SummaryKind::Condensed => format!("LCM condensed summary at depth {depth}:"),
        };
        let mut output = Vec::new();
        let max_chars = target_tokens.saturating_mul(4);
        let mut current_len = header.len();
        output.push(header.clone());
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let bullet = format!("- {}", collapse_whitespace(trimmed));
            if current_len + bullet.len() + 1 > max_chars {
                break;
            }
            current_len += bullet.len() + 1;
            output.push(bullet);
        }
        if output.len() == 1 {
            header.push_str(" no significant content captured.");
            output[0] = header;
        }
        Ok(output.join("\n"))
    }
}

pub struct LcmEngine {
    conn: Connection,
    config: LcmConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JournalMode {
    Wal,
    Delete,
    Truncate,
}

impl JournalMode {
    fn from_env() -> Self {
        let value = std::env::var("CTOX_LCM_JOURNAL_MODE")
            .ok()
            .or_else(|| std::env::var("CTOX_SQLITE_JOURNAL_MODE").ok())
            .unwrap_or_else(|| "wal".to_string());
        match value.trim().to_ascii_lowercase().as_str() {
            "delete" => Self::Delete,
            "truncate" => Self::Truncate,
            _ => Self::Wal,
        }
    }

    fn as_sql(self) -> &'static str {
        match self {
            Self::Wal => "WAL",
            Self::Delete => "DELETE",
            Self::Truncate => "TRUNCATE",
        }
    }
}

impl LcmEngine {
    pub fn open(path: &Path, config: LcmConfig) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open SQLite database {}", path.display()))?;
        conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
            .context("failed to configure SQLite busy_timeout for LCM")?;
        let engine = Self {
            conn,
            config: config.clone(),
        };
        // Skip the writer-locked schema-init batch on subsequent opens of the
        // same database file. The first open per process still runs the full
        // init (with WAL fallback); later opens just reuse it.
        let canonical = path.to_path_buf();
        if initialized_lcm_paths()
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .contains(&canonical)
        {
            return Ok(engine);
        }
        let journal_mode = JournalMode::from_env();
        if let Err(err) = engine.init_schema(journal_mode) {
            if journal_mode == JournalMode::Wal && is_shared_memory_io_error(&err) {
                let conn = Connection::open(path).with_context(|| {
                    format!(
                        "failed to reopen SQLite database {} after WAL error",
                        path.display()
                    )
                })?;
                conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
                    .context("failed to configure SQLite busy_timeout for LCM fallback")?;
                let fallback = Self {
                    conn,
                    config: config.clone(),
                };
                fallback.init_schema(JournalMode::Delete)?;
                initialized_lcm_paths()
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .insert(canonical);
                return Ok(fallback);
            }
            return Err(err);
        }
        initialized_lcm_paths()
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(canonical);
        Ok(engine)
    }

    /// Override the connection-level busy timeout. Read-mostly UI consumers
    /// use a short timeout so a daemon-held write lock degrades into stale
    /// data on screen instead of stalling their render loop for the full
    /// default 30s window.
    pub fn set_busy_timeout(&self, timeout: Duration) -> Result<()> {
        self.conn
            .busy_timeout(timeout)
            .context("failed to override SQLite busy_timeout for LCM")
    }

    fn init_schema(&self, journal_mode: JournalMode) -> Result<()> {
        self.conn.execute_batch(&format!(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = {};

            CREATE TABLE IF NOT EXISTS messages (
                message_id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id INTEGER NOT NULL,
                seq INTEGER NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                token_count INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_conversation_seq
                ON messages(conversation_id, seq);

            CREATE TABLE IF NOT EXISTS summaries (
                summary_id TEXT PRIMARY KEY,
                conversation_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                depth INTEGER NOT NULL,
                content TEXT NOT NULL,
                token_count INTEGER NOT NULL,
                descendant_count INTEGER NOT NULL DEFAULT 0,
                descendant_token_count INTEGER NOT NULL DEFAULT 0,
                source_message_token_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS summary_edges (
                parent_summary_id TEXT NOT NULL,
                child_summary_id TEXT NOT NULL,
                PRIMARY KEY(parent_summary_id, child_summary_id)
            );

            CREATE TABLE IF NOT EXISTS summary_messages (
                summary_id TEXT NOT NULL,
                message_id INTEGER NOT NULL,
                PRIMARY KEY(summary_id, message_id)
            );

            CREATE TABLE IF NOT EXISTS context_items (
                conversation_id INTEGER NOT NULL,
                ordinal INTEGER NOT NULL,
                item_type TEXT NOT NULL,
                message_id INTEGER,
                summary_id TEXT,
                created_at TEXT NOT NULL,
                PRIMARY KEY(conversation_id, ordinal)
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                content,
                content='',
                tokenize='unicode61'
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS summaries_fts USING fts5(
                summary_id UNINDEXED,
                content,
                content='',
                tokenize='unicode61'
            );

            CREATE TABLE IF NOT EXISTS continuity_revisions (
                revision_id TEXT PRIMARY KEY,
                conversation_id INTEGER NOT NULL,
                narrative TEXT NOT NULL,
                anchors TEXT NOT NULL,
                focus TEXT NOT NULL,
                source_summary_ids TEXT NOT NULL,
                source_message_ids TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS continuity_documents (
                document_id TEXT PRIMARY KEY,
                conversation_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                head_commit_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(conversation_id, kind)
            );

            CREATE TABLE IF NOT EXISTS continuity_commits (
                commit_id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                parent_commit_id TEXT,
                diff_text TEXT NOT NULL,
                rendered_text TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS mission_states (
                conversation_id INTEGER PRIMARY KEY,
                mission TEXT NOT NULL,
                mission_status TEXT NOT NULL,
                continuation_mode TEXT NOT NULL,
                trigger_intensity TEXT NOT NULL,
                blocker TEXT NOT NULL,
                next_slice TEXT NOT NULL,
                done_gate TEXT NOT NULL,
                closure_confidence TEXT NOT NULL,
                is_open INTEGER NOT NULL,
                allow_idle INTEGER NOT NULL,
                focus_head_commit_id TEXT NOT NULL,
                last_synced_at TEXT NOT NULL,
                watcher_last_triggered_at TEXT,
                watcher_trigger_count INTEGER NOT NULL DEFAULT 0,
                agent_failure_count INTEGER NOT NULL DEFAULT 0,
                deferred_reason TEXT,
                rewrite_failure_count INTEGER NOT NULL DEFAULT 0,
                structured_state_version INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS lcm_data_migrations (
                migration_id TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL,
                details_json TEXT NOT NULL DEFAULT '{{}}'
            );

            CREATE TABLE IF NOT EXISTS verification_runs (
                run_id TEXT PRIMARY KEY,
                conversation_id INTEGER NOT NULL,
                source_label TEXT NOT NULL,
                goal TEXT NOT NULL,
                preview TEXT NOT NULL,
                result_excerpt TEXT NOT NULL,
                blocker TEXT,
                review_required INTEGER NOT NULL,
                review_verdict TEXT NOT NULL,
                review_summary TEXT NOT NULL,
                review_score INTEGER NOT NULL,
                review_reasons TEXT NOT NULL,
                report_excerpt TEXT NOT NULL,
                claim_count INTEGER NOT NULL,
                open_claim_count INTEGER NOT NULL,
                closure_blocking_claim_count INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_verification_runs_conversation_created_at
                ON verification_runs(conversation_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS strategic_directives (
                directive_id TEXT PRIMARY KEY,
                conversation_id INTEGER NOT NULL,
                thread_key TEXT,
                directive_kind TEXT NOT NULL,
                title TEXT NOT NULL,
                body_text TEXT NOT NULL,
                status TEXT NOT NULL,
                revision INTEGER NOT NULL,
                previous_directive_id TEXT,
                author TEXT NOT NULL,
                decided_by TEXT,
                decision_reason TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_strategic_directives_scope
                ON strategic_directives(conversation_id, directive_kind, status, updated_at DESC);

            CREATE TABLE IF NOT EXISTS mission_claims (
                claim_key TEXT PRIMARY KEY,
                conversation_id INTEGER NOT NULL,
                last_run_id TEXT NOT NULL,
                claim_kind TEXT NOT NULL,
                claim_status TEXT NOT NULL,
                blocks_closure INTEGER NOT NULL,
                subject TEXT NOT NULL,
                summary TEXT NOT NULL,
                evidence_summary TEXT NOT NULL,
                recheck_policy TEXT NOT NULL,
                expires_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mission_claims_conversation_status
                ON mission_claims(conversation_id, claim_status, updated_at DESC);

            CREATE TABLE IF NOT EXISTS secret_rewrites (
                rewrite_id TEXT PRIMARY KEY,
                conversation_id INTEGER NOT NULL,
                secret_scope TEXT NOT NULL,
                secret_name TEXT NOT NULL,
                replacement_text TEXT NOT NULL,
                match_digest TEXT NOT NULL,
                message_rows_updated INTEGER NOT NULL,
                summary_rows_updated INTEGER NOT NULL,
                continuity_commit_rows_updated INTEGER NOT NULL,
                continuity_revision_rows_updated INTEGER NOT NULL,
                mission_state_rows_updated INTEGER NOT NULL,
                verification_rows_updated INTEGER NOT NULL,
                claim_rows_updated INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_secret_rewrites_conversation_time
                ON secret_rewrites(conversation_id, created_at DESC);
            "#,
            journal_mode.as_sql()
        ))?;
        self.ensure_schema_upgrades()?;
        self.ensure_worker_attempt_finalization_schema()?;
        self.ensure_task_execution_progress_schema()?;
        migrate_empty_mission_split_brain_with(&self.conn)?;
        Ok(())
    }

    fn ensure_schema_upgrades(&self) -> Result<()> {
        self.ensure_column(
            "verification_runs",
            "raw_report",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        self.ensure_column(
            "verification_runs",
            "mission_state",
            "TEXT NOT NULL DEFAULT 'UNCLEAR'",
        )?;
        self.ensure_column(
            "verification_runs",
            "failed_gates_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        self.ensure_column(
            "verification_runs",
            "semantic_findings_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        self.ensure_column(
            "verification_runs",
            "open_items_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        self.ensure_column(
            "verification_runs",
            "evidence_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        self.ensure_column(
            "verification_runs",
            "handoff_text",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        // F2: per-(conversation, mission) agent-failure tracking for the
        // watchdog backoff. `agent_failure_count` increments on
        // non-Success agent outcomes (timeout, panic, runtime error) and
        // resets on success; `deferred_reason` stores the structured reason
        // when the watchdog stops spawning continuations.
        self.ensure_column(
            "mission_states",
            "agent_failure_count",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.ensure_column("mission_states", "deferred_reason", "TEXT")?;
        // F3: structured agent outcome on assistant rows; NULL for non-assistant rows.
        self.ensure_column("messages", "agent_outcome", "TEXT")?;
        // Review rewrite/rework split: per-(conversation, mission)
        // counter for consecutive non-converging rewrite-only review
        // iterations. Trips the mission into `deferred` once the
        // configured threshold is reached.
        self.ensure_column(
            "mission_states",
            "rewrite_failure_count",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        // P8b: marks rows whose runtime controls are stored in the typed-v1
        // fieldset. Added idempotently through the PRAGMA table_info guard in
        // `ensure_column`; pre-existing rows already contain these columns and
        // become typed-v1 without re-reading continuity text.
        self.ensure_column(
            "mission_states",
            "structured_state_version",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        Ok(())
    }

    /// I-071: install the worker-attempt marker once per database. The
    /// `lcm_data_migrations` row is the durable migration marker; the partial
    /// unique index permits only one recoverable finalization per logical work
    /// key while retaining terminal attempt history.
    fn ensure_worker_attempt_finalization_schema(&self) -> Result<()> {
        const MIGRATION_ID: &str = "i-071-worker-attempt-finalization-v1";
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin worker-attempt schema migration")?;
        let already_applied: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM lcm_data_migrations WHERE migration_id = ?1)",
            [MIGRATION_ID],
            |row| row.get(0),
        )?;
        if !already_applied {
            tx.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS worker_attempt_finalizations (
                    attempt_id TEXT PRIMARY KEY,
                    work_key TEXT NOT NULL,
                    conversation_id INTEGER NOT NULL,
                    source_label TEXT NOT NULL,
                    status TEXT NOT NULL CHECK(status IN ('finalizing', 'succeeded', 'failed', 'timed_out')),
                    agent_outcome TEXT NOT NULL,
                    reply_text TEXT NOT NULL,
                    error_text TEXT,
                    reply_message_id INTEGER UNIQUE,
                    artifact_checked_at TEXT,
                    artifact_check_accepted INTEGER,
                    artifact_check_details TEXT,
                    resumable INTEGER NOT NULL DEFAULT 0,
                    effects_completed INTEGER NOT NULL DEFAULT 0,
                    queue_effects_applied_at TEXT,
                    recovery_effects_applied_at TEXT,
                    finalization_error TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    terminal_at TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_worker_attempt_work_history
                    ON worker_attempt_finalizations(work_key, created_at DESC);
                CREATE UNIQUE INDEX IF NOT EXISTS idx_worker_attempt_one_recoverable
                    ON worker_attempt_finalizations(work_key)
                    WHERE effects_completed = 0;
                "#,
            )?;
            tx.execute(
                "INSERT INTO lcm_data_migrations (migration_id, applied_at, details_json)
                 VALUES (?1, ?2, ?3)",
                params![
                    MIGRATION_ID,
                    iso_now(),
                    r#"{"surface":"worker_attempt_finalizations","version":1}"#
                ],
            )?;
        }
        tx.commit()
            .context("failed to commit worker-attempt schema migration")?;
        // I-072 extends the existing attempt surface without replacing the
        // once-per-database I-071 migration marker. Existing databases already
        // carrying that marker still receive the new idempotence column.
        self.ensure_column(
            "worker_attempt_finalizations",
            "recovery_effects_applied_at",
            "TEXT",
        )?;
        Ok(())
    }

    /// Durable, revisioned user-visible execution plans and idempotent
    /// activity turns. This is authoritative task state; the harness-flow
    /// ledger remains a best-effort forensic projection.
    fn ensure_task_execution_progress_schema(&self) -> Result<()> {
        const MIGRATION_ID: &str = "task-execution-progress-v1";
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin task execution progress schema migration")?;
        let already_applied: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM lcm_data_migrations WHERE migration_id = ?1)",
            [MIGRATION_ID],
            |row| row.get(0),
        )?;
        if !already_applied {
            tx.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS task_execution_plan_revisions (
                    work_key TEXT NOT NULL,
                    revision INTEGER NOT NULL,
                    task_id TEXT NOT NULL DEFAULT '',
                    command_id TEXT NOT NULL DEFAULT '',
                    attempt_id TEXT NOT NULL,
                    plan_signature TEXT NOT NULL,
                    explanation TEXT,
                    steps_json TEXT NOT NULL,
                    phase TEXT NOT NULL CHECK(phase IN ('working', 'review', 'completed', 'failed', 'blocked')),
                    completed_steps INTEGER NOT NULL,
                    total_steps INTEGER NOT NULL,
                    percent INTEGER NOT NULL CHECK(percent BETWEEN 0 AND 100),
                    review_status TEXT NOT NULL CHECK(review_status IN ('pending', 'in_progress', 'completed', 'failed')),
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(work_key, revision)
                );
                CREATE INDEX IF NOT EXISTS idx_task_execution_plan_task
                    ON task_execution_plan_revisions(task_id, revision DESC);
                CREATE INDEX IF NOT EXISTS idx_task_execution_plan_command
                    ON task_execution_plan_revisions(command_id, revision DESC);

                CREATE TABLE IF NOT EXISTS task_execution_activity_turns (
                    activity_id TEXT PRIMARY KEY,
                    work_key TEXT NOT NULL,
                    task_id TEXT NOT NULL DEFAULT '',
                    command_id TEXT NOT NULL DEFAULT '',
                    attempt_id TEXT NOT NULL,
                    plan_revision INTEGER,
                    step_position INTEGER,
                    kind TEXT NOT NULL CHECK(kind IN ('thinking', 'tool')),
                    created_at_ms INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_task_execution_activity_work
                    ON task_execution_activity_turns(work_key, created_at_ms);
                CREATE INDEX IF NOT EXISTS idx_task_execution_activity_step
                    ON task_execution_activity_turns(work_key, plan_revision, step_position);
                "#,
            )?;
            tx.execute(
                "INSERT INTO lcm_data_migrations (migration_id, applied_at, details_json)
                 VALUES (?1, ?2, ?3)",
                params![
                    MIGRATION_ID,
                    iso_now(),
                    r#"{"surface":"task_execution_progress","version":1}"#
                ],
            )?;
        }
        tx.commit()
            .context("failed to commit task execution progress schema migration")?;
        Ok(())
    }

    fn ensure_column(&self, table: &str, column: &str, definition: &str) -> Result<()> {
        let pragma = format!("PRAGMA table_info({table})");
        let mut stmt = self
            .conn
            .prepare(&pragma)
            .with_context(|| format!("failed to inspect table {table}"))?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = false;
        for value in rows {
            if value? == column {
                found = true;
                break;
            }
        }
        if !found {
            if let Err(err) = self.conn.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition};"
            )) {
                // Cross-process race: another writer may have added the column
                // between the probe above and this ALTER. A duplicate-column
                // error means the column now exists (the desired end state), so
                // tolerate it instead of failing engine open.
                if !err
                    .to_string()
                    .to_ascii_lowercase()
                    .contains("duplicate column name")
                {
                    return Err(err.into());
                }
            }
        }
        Ok(())
    }

    pub fn add_message(
        &self,
        conversation_id: i64,
        role: &str,
        content: &str,
    ) -> Result<MessageRecord> {
        self.add_message_with_outcome(conversation_id, role, content, None)
    }

    /// F3: insert an assistant turn with a structured `AgentOutcome` recorded
    /// in `messages.agent_outcome`. Non-assistant rows always store NULL;
    /// callers that pass an outcome for a non-assistant role are corrected
    /// silently (and the helper logs nothing — the column column is the
    /// authoritative state, not the role argument).
    pub fn add_message_with_outcome(
        &self,
        conversation_id: i64,
        role: &str,
        content: &str,
        outcome: Option<AgentOutcome>,
    ) -> Result<MessageRecord> {
        let _ = self.continuity_init_documents(conversation_id)?;
        let now = iso_now();
        let token_count = estimate_tokens(content) as i64;
        let stored_outcome = if role == "assistant" {
            outcome.map(|value| value.as_str().to_string())
        } else {
            None
        };
        // Persist the message row, its FTS row, and its context_items row atomically.
        // A mid-sequence failure (e.g. SQLITE_BUSY, or a failing ordinal read) must not
        // commit an orphan messages row: render/compaction JOIN through context_items and
        // would never see it, while the persist-retry helper would insert a duplicate.
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin add-message transaction")?;
        let seq = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE conversation_id = ?1",
                [conversation_id],
                |row| row.get::<_, i64>(0),
            )
            .context("failed to compute next message seq")?;
        self.conn.execute(
            "INSERT INTO messages (conversation_id, seq, role, content, token_count, created_at, agent_outcome)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                conversation_id,
                seq,
                role,
                content,
                token_count,
                now,
                stored_outcome,
            ],
        )?;
        let message_id = self.conn.last_insert_rowid();
        self.conn.execute(
            "INSERT INTO messages_fts (rowid, content) VALUES (?1, ?2)",
            params![message_id, normalize_for_fts(content)],
        )?;
        let ordinal = self.next_context_ordinal(conversation_id)?;
        self.conn.execute(
            "INSERT INTO context_items (conversation_id, ordinal, item_type, message_id, summary_id, created_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
            params![conversation_id, ordinal, ContextItemType::Message.as_str(), message_id, iso_now()],
        )?;
        tx.commit()
            .context("failed to commit add-message transaction")?;
        Ok(MessageRecord {
            message_id,
            conversation_id,
            seq,
            role: role.to_string(),
            content: content.to_string(),
            token_count,
            created_at: now,
            agent_outcome: stored_outcome,
        })
    }

    /// Persist the durable `finalizing` marker before any completion effect.
    /// Repeating the same input is idempotent; a different attempt racing for
    /// the same recoverable work key receives the already-authoritative row.
    pub fn begin_worker_attempt_finalization(
        &self,
        input: WorkerAttemptFinalizationInput<'_>,
    ) -> Result<WorkerAttemptRecord> {
        // Serialize the two identity probes and insert. The partial unique index
        // remains the database invariant, while the immediate transaction makes
        // a cross-process race return the authoritative row instead of leaking a
        // uniqueness error to the worker.
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin worker-attempt finalization transaction")?;
        if let Some(existing) = tx
            .query_row(
                &format!("{WORKER_ATTEMPT_SELECT} WHERE attempt_id = ?1"),
                [input.attempt_id],
                map_worker_attempt_row,
            )
            .optional()?
        {
            anyhow::ensure!(
                existing.work_key == input.work_key
                    && existing.conversation_id == input.conversation_id
                    && existing.source_label == input.source_label
                    && existing.agent_outcome == input.agent_outcome
                    && existing.reply_text == input.reply_text
                    && existing.error_text.as_deref() == input.error_text,
                "worker attempt {} was reused with different immutable finalization data",
                input.attempt_id
            );
            tx.commit()?;
            return Ok(existing);
        }
        if let Some(existing) = tx
            .query_row(
                &format!(
                    "{WORKER_ATTEMPT_SELECT} WHERE work_key = ?1 AND effects_completed = 0 ORDER BY created_at DESC LIMIT 1"
                ),
                [input.work_key],
                map_worker_attempt_row,
            )
            .optional()?
        {
            tx.commit()?;
            return Ok(existing);
        }
        let now = iso_now();
        tx.execute(
            "INSERT INTO worker_attempt_finalizations (
                attempt_id, work_key, conversation_id, source_label, status,
                agent_outcome, reply_text, error_text, resumable,
                effects_completed, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, 'finalizing', ?5, ?6, ?7, 0, 0, ?8, ?8)",
            params![
                input.attempt_id,
                input.work_key,
                input.conversation_id,
                input.source_label,
                input.agent_outcome.as_str(),
                input.reply_text,
                input.error_text,
                now,
            ],
        )?;
        let record = tx.query_row(
            &format!("{WORKER_ATTEMPT_SELECT} WHERE attempt_id = ?1"),
            [input.attempt_id],
            map_worker_attempt_row,
        )?;
        tx.commit()
            .context("failed to commit worker-attempt finalization marker")?;
        Ok(record)
    }

    pub fn worker_attempt(&self, attempt_id: &str) -> Result<Option<WorkerAttemptRecord>> {
        self.conn
            .query_row(
                &format!("{WORKER_ATTEMPT_SELECT} WHERE attempt_id = ?1"),
                [attempt_id],
                map_worker_attempt_row,
            )
            .optional()
            .context("failed to load worker attempt")
    }

    pub fn recoverable_worker_attempt(
        &self,
        work_key: &str,
    ) -> Result<Option<WorkerAttemptRecord>> {
        self.conn
            .query_row(
                &format!(
                    "{WORKER_ATTEMPT_SELECT} WHERE work_key = ?1 AND effects_completed = 0 ORDER BY created_at DESC LIMIT 1"
                ),
                [work_key],
                map_worker_attempt_row,
            )
            .optional()
            .context("failed to load recoverable worker attempt")
    }

    pub fn record_task_execution_plan(
        &self,
        input: TaskExecutionPlanUpdate<'_>,
    ) -> Result<serde_json::Value> {
        self.record_task_execution_plan_guarded(input, |_| Ok(()))
    }

    /// Check execution authority under the same write lock as the update.
    pub(crate) fn record_task_execution_plan_guarded(
        &self,
        input: TaskExecutionPlanUpdate<'_>,
        authorize: impl FnOnce(&Connection) -> Result<()>,
    ) -> Result<serde_json::Value> {
        let (steps, completed_steps) = validate_task_execution_steps(input.steps)?;
        let total_steps = i64::try_from(steps.len()).unwrap_or(i64::MAX);
        let signature = task_execution_plan_signature(&steps);
        let now_ms = epoch_millis_i64();
        let steps_json = serde_json::to_string(&steps)?;
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin task execution plan transaction")?;
        authorize(&tx)?;
        let latest = tx
            .query_row(
                "SELECT revision, plan_signature, review_status, created_at_ms
                 FROM task_execution_plan_revisions
                 WHERE work_key = ?1
                 ORDER BY revision DESC LIMIT 1",
                [input.work_key],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;
        let (revision, review_status, created_at_ms) = match latest {
            Some((revision, latest_signature, review_status, created_at_ms))
                if latest_signature == signature =>
            {
                (revision, review_status, created_at_ms)
            }
            Some((revision, _, _, _)) => {
                (revision.saturating_add(1), "pending".to_string(), now_ms)
            }
            None => (1, "pending".to_string(), now_ms),
        };
        let review_status = if completed_steps == total_steps {
            review_status
        } else {
            "pending".to_string()
        };
        let percent = task_execution_percent(completed_steps, total_steps, &review_status);
        let phase = task_execution_phase(completed_steps, total_steps, &review_status);
        tx.execute(
            "INSERT INTO task_execution_plan_revisions (
                work_key, revision, task_id, command_id, attempt_id,
                plan_signature, explanation, steps_json, phase,
                completed_steps, total_steps, percent, review_status,
                created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(work_key, revision) DO UPDATE SET
                task_id = excluded.task_id,
                command_id = excluded.command_id,
                attempt_id = excluded.attempt_id,
                explanation = excluded.explanation,
                steps_json = excluded.steps_json,
                phase = excluded.phase,
                completed_steps = excluded.completed_steps,
                total_steps = excluded.total_steps,
                percent = excluded.percent,
                review_status = excluded.review_status,
                updated_at_ms = excluded.updated_at_ms",
            params![
                input.work_key,
                revision,
                input.task_id,
                input.command_id,
                input.attempt_id,
                signature,
                input.explanation,
                steps_json,
                phase,
                completed_steps,
                total_steps,
                percent,
                review_status,
                created_at_ms,
                now_ms,
            ],
        )?;
        tx.commit()
            .context("failed to commit task execution plan")?;
        self.task_execution_progress(input.work_key)?
            .context("task execution plan vanished after commit")
    }

    pub fn record_task_execution_activity(
        &self,
        input: TaskExecutionActivityInput<'_>,
    ) -> Result<bool> {
        anyhow::ensure!(
            matches!(input.kind, "thinking" | "tool"),
            "task activity kind must be thinking or tool"
        );
        anyhow::ensure!(
            !input.activity_id.trim().is_empty(),
            "task activity id is required"
        );
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin task activity transaction")?;
        let current = tx
            .query_row(
                "SELECT revision, steps_json
                 FROM task_execution_plan_revisions
                 WHERE work_key = ?1
                 ORDER BY revision DESC LIMIT 1",
                [input.work_key],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let (revision, step_position) = if let Some((revision, steps_json)) = current {
            let position = if input.attribute_to_current_step {
                serde_json::from_str::<Vec<TaskExecutionPlanStepInput>>(&steps_json)?
                    .iter()
                    .position(|step| step.status == "in_progress")
                    .and_then(|position| i64::try_from(position + 1).ok())
            } else {
                None
            };
            (Some(revision), position)
        } else {
            (None, None)
        };
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO task_execution_activity_turns (
                activity_id, work_key, task_id, command_id, attempt_id,
                plan_revision, step_position, kind, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                input.activity_id,
                input.work_key,
                input.task_id,
                input.command_id,
                input.attempt_id,
                revision,
                step_position,
                input.kind,
                epoch_millis_i64(),
            ],
        )? > 0;
        if inserted {
            tx.execute(
                "UPDATE task_execution_plan_revisions
                 SET updated_at_ms = ?2
                 WHERE work_key = ?1
                   AND revision = (SELECT MAX(revision) FROM task_execution_plan_revisions WHERE work_key = ?1)",
                params![input.work_key, epoch_millis_i64()],
            )?;
        }
        tx.commit().context("failed to commit task activity")?;
        Ok(inserted)
    }

    pub fn prepare_task_execution_review(&self, work_key: &str) -> Result<serde_json::Value> {
        let latest = self
            .task_execution_progress(work_key)?
            .with_context(|| format!("task {work_key} has no durable execution plan"))?;
        let completed = latest
            .get("completed_steps")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let total = latest
            .get("total_steps")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        anyhow::ensure!(
            total > 0,
            "task execution plan must contain at least one step"
        );
        anyhow::ensure!(
            completed == total,
            "task execution plan is incomplete ({completed}/{total} steps completed)"
        );
        self.set_task_execution_review_status(work_key, "in_progress")
    }

    pub fn set_task_execution_review_status(
        &self,
        work_key: &str,
        review_status: &str,
    ) -> Result<serde_json::Value> {
        anyhow::ensure!(
            matches!(review_status, "in_progress" | "completed" | "failed"),
            "invalid task review status {review_status}"
        );
        let (phase, percent) = if review_status == "completed" {
            ("completed", 100)
        } else {
            ("review", 90)
        };
        let changed = self.conn.execute(
            "UPDATE task_execution_plan_revisions
             SET review_status = ?2, phase = ?3, percent = ?4, updated_at_ms = ?5
             WHERE work_key = ?1
               AND revision = (SELECT MAX(revision) FROM task_execution_plan_revisions WHERE work_key = ?1)",
            params![work_key, review_status, phase, percent, epoch_millis_i64()],
        )?;
        anyhow::ensure!(
            changed == 1,
            "task {work_key} has no durable execution plan"
        );
        self.task_execution_progress(work_key)?
            .context("task execution progress vanished after review update")
    }

    pub fn task_execution_progress(&self, work_key: &str) -> Result<Option<serde_json::Value>> {
        let row = self
            .conn
            .query_row(
                "SELECT revision, task_id, command_id, explanation, steps_json,
                        phase, completed_steps, total_steps, percent, review_status, updated_at_ms
                 FROM task_execution_plan_revisions
                 WHERE work_key = ?1
                 ORDER BY revision DESC LIMIT 1",
                [work_key],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, i64>(10)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            revision,
            task_id,
            command_id,
            explanation,
            steps_json,
            phase,
            completed_steps,
            total_steps,
            percent,
            review_status,
            updated_at_ms,
        )) = row
        else {
            return Ok(None);
        };
        let steps = serde_json::from_str::<Vec<TaskExecutionPlanStepInput>>(&steps_json)?;
        let mut rendered_steps = Vec::with_capacity(steps.len());
        for (index, step) in steps.iter().enumerate() {
            let position = i64::try_from(index + 1).unwrap_or(i64::MAX);
            let activity_turns = self.conn.query_row(
                "SELECT COUNT(*) FROM task_execution_activity_turns
                 WHERE work_key = ?1 AND plan_revision = ?2 AND step_position = ?3",
                params![work_key, revision, position],
                |row| row.get::<_, i64>(0),
            )?;
            rendered_steps.push(serde_json::json!({
                "position": position,
                "label": step.label,
                "status": step.status,
                "activity_turns": activity_turns,
            }));
        }
        let (thinking_turns, tool_turns) = self.conn.query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN kind = 'thinking' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN kind = 'tool' THEN 1 ELSE 0 END), 0)
             FROM task_execution_activity_turns WHERE work_key = ?1",
            [work_key],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )?;
        let last_activity_kind = self
            .conn
            .query_row(
                "SELECT kind FROM task_execution_activity_turns
                 WHERE work_key = ?1 ORDER BY created_at_ms DESC, rowid DESC LIMIT 1",
                [work_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let current_step = rendered_steps
            .iter()
            .find(|step| {
                step.get("status").and_then(serde_json::Value::as_str) == Some("in_progress")
            })
            .and_then(|step| step.get("position"))
            .and_then(serde_json::Value::as_i64);
        Ok(Some(serde_json::json!({
            "version": 1,
            "revision": revision,
            "task_id": task_id,
            "command_id": command_id,
            "phase": phase,
            "percent": percent,
            "current_step": current_step,
            "completed_steps": completed_steps,
            "total_steps": total_steps,
            "steps": rendered_steps,
            "review": { "status": review_status },
            "activity_turns": {
                "total": thinking_turns + tool_turns,
                "thinking": thinking_turns,
                "tools": tool_turns,
                "last_kind": last_activity_kind,
            },
            "explanation": explanation,
            "updated_at_ms": updated_at_ms,
        })))
    }

    pub fn task_execution_progress_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<serde_json::Value>> {
        let work_key = self
            .conn
            .query_row(
                "SELECT work_key FROM task_execution_plan_revisions
                 WHERE task_id = ?1 ORDER BY updated_at_ms DESC, revision DESC LIMIT 1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match work_key {
            Some(work_key) => self.task_execution_progress(&work_key),
            None => Ok(None),
        }
    }

    /// Insert the attempt's assistant row once and bind its id to the marker in
    /// the same SQLite transaction. A crash after `finalizing` but before this
    /// method is recovered by calling it again; a crash after commit returns the
    /// original row without allocating another sequence number.
    pub fn ensure_worker_attempt_assistant_message(
        &self,
        attempt_id: &str,
    ) -> Result<MessageRecord> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin worker-attempt reply transaction")?;
        let attempt = tx
            .query_row(
                &format!("{WORKER_ATTEMPT_SELECT} WHERE attempt_id = ?1"),
                [attempt_id],
                map_worker_attempt_row,
            )
            .optional()?
            .with_context(|| format!("worker attempt {attempt_id} does not exist"))?;
        if let Some(message_id) = attempt.reply_message_id {
            let record = tx.query_row(
                "SELECT message_id, conversation_id, seq, role, content, token_count, created_at, agent_outcome FROM messages WHERE message_id = ?1",
                [message_id],
                |row| {
                    Ok(MessageRecord {
                        message_id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        seq: row.get(2)?,
                        role: row.get(3)?,
                        content: row.get(4)?,
                        token_count: row.get(5)?,
                        created_at: row.get(6)?,
                        agent_outcome: row.get(7)?,
                    })
                },
            )?;
            tx.commit()?;
            return Ok(record);
        }

        let now = iso_now();
        let token_count = estimate_tokens(&attempt.reply_text) as i64;
        let seq = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE conversation_id = ?1",
            [attempt.conversation_id],
            |row| row.get::<_, i64>(0),
        )?;
        tx.execute(
            "INSERT INTO messages (conversation_id, seq, role, content, token_count, created_at, agent_outcome)
             VALUES (?1, ?2, 'assistant', ?3, ?4, ?5, ?6)",
            params![
                attempt.conversation_id,
                seq,
                attempt.reply_text,
                token_count,
                now,
                attempt.agent_outcome.as_str(),
            ],
        )?;
        let message_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO messages_fts (rowid, content) VALUES (?1, ?2)",
            params![message_id, normalize_for_fts(&attempt.reply_text)],
        )?;
        let ordinal = tx.query_row(
            "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM context_items WHERE conversation_id = ?1",
            [attempt.conversation_id],
            |row| row.get::<_, i64>(0),
        )?;
        tx.execute(
            "INSERT INTO context_items (conversation_id, ordinal, item_type, message_id, summary_id, created_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
            params![
                attempt.conversation_id,
                ordinal,
                ContextItemType::Message.as_str(),
                message_id,
                iso_now(),
            ],
        )?;
        tx.execute(
            "UPDATE worker_attempt_finalizations
             SET reply_message_id = ?2, updated_at = ?3
             WHERE attempt_id = ?1 AND reply_message_id IS NULL",
            params![attempt_id, message_id, iso_now()],
        )?;
        tx.commit()
            .context("failed to commit worker-attempt reply transaction")?;
        Ok(MessageRecord {
            message_id,
            conversation_id: attempt.conversation_id,
            seq,
            role: "assistant".to_string(),
            content: attempt.reply_text,
            token_count,
            created_at: now,
            agent_outcome: Some(attempt.agent_outcome.as_str().to_string()),
        })
    }

    pub fn record_worker_attempt_artifact_check(
        &self,
        attempt_id: &str,
        accepted: bool,
        details: &str,
    ) -> Result<WorkerAttemptRecord> {
        self.conn.execute(
            "UPDATE worker_attempt_finalizations
             SET artifact_checked_at = ?2, artifact_check_accepted = ?3,
                 artifact_check_details = ?4, updated_at = ?2
             WHERE attempt_id = ?1",
            params![attempt_id, iso_now(), accepted as i64, details],
        )?;
        self.worker_attempt(attempt_id)?
            .with_context(|| format!("worker attempt {attempt_id} does not exist"))
    }

    pub fn terminalize_worker_attempt(
        &self,
        attempt_id: &str,
        status: WorkerAttemptTerminalStatus,
        resumable: bool,
        effects_completed: bool,
        finalization_error: Option<&str>,
    ) -> Result<WorkerAttemptRecord> {
        let current = self
            .worker_attempt(attempt_id)?
            .with_context(|| format!("worker attempt {attempt_id} does not exist"))?;
        anyhow::ensure!(
            current.artifact_checked_at.is_some(),
            "worker attempt {attempt_id} cannot become terminal before artifact recheck"
        );
        if current.status != "finalizing" {
            anyhow::ensure!(
                current.status == status.as_str(),
                "worker attempt {attempt_id} is already terminal as {}",
                current.status
            );
        }
        let now = iso_now();
        self.conn.execute(
            "UPDATE worker_attempt_finalizations
             SET status = ?2, resumable = ?3, effects_completed = ?4,
                 finalization_error = ?5, terminal_at = COALESCE(terminal_at, ?6),
                 updated_at = ?6
             WHERE attempt_id = ?1",
            params![
                attempt_id,
                status.as_str(),
                resumable as i64,
                effects_completed as i64,
                finalization_error,
                now,
            ],
        )?;
        self.worker_attempt(attempt_id)?
            .context("worker attempt disappeared after terminal transition")
    }

    pub fn mark_worker_attempt_effects_completed(
        &self,
        attempt_id: &str,
    ) -> Result<WorkerAttemptRecord> {
        self.conn.execute(
            "UPDATE worker_attempt_finalizations
             SET effects_completed = 1, updated_at = ?2
             WHERE attempt_id = ?1 AND status != 'finalizing'",
            params![attempt_id, iso_now()],
        )?;
        self.worker_attempt(attempt_id)?
            .with_context(|| format!("worker attempt {attempt_id} does not exist"))
    }

    /// Mark the process-local recovery enqueue as applied for this attempt.
    /// The enqueue is performed before this marker is written; after a real
    /// process crash the old in-memory queue is gone, while a committed marker
    /// suppresses replay after any later finalization step fails in-process.
    pub fn mark_worker_attempt_recovery_effects_applied(&self, attempt_id: &str) -> Result<bool> {
        let now = iso_now();
        let updated = self.conn.execute(
            "UPDATE worker_attempt_finalizations
             SET recovery_effects_applied_at = ?2, updated_at = ?2
             WHERE attempt_id = ?1 AND recovery_effects_applied_at IS NULL",
            params![attempt_id, now],
        )?;
        anyhow::ensure!(
            updated != 0 || self.worker_attempt(attempt_id)?.is_some(),
            "worker attempt {attempt_id} does not exist"
        );
        Ok(updated != 0)
    }

    /// F3: read the most recent assistant `agent_outcome` for a conversation.
    /// Returns `None` if there is no assistant row yet, or if the latest
    /// assistant row predates the schema upgrade and has a NULL outcome.
    pub fn last_agent_outcome(&self, conversation_id: i64) -> Result<Option<AgentOutcome>> {
        let raw: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT agent_outcome FROM messages
                 WHERE conversation_id = ?1 AND role = 'assistant'
                 ORDER BY seq DESC LIMIT 1",
                [conversation_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .context("failed to load last agent outcome")?;
        Ok(raw
            .flatten()
            .and_then(|token| AgentOutcome::from_token(&token)))
    }

    /// Read the most recent assistant message row for a conversation.
    ///
    /// `ctox chat --wait` polls this to detect that *its* turn produced a
    /// durable terminal outcome. Completion must be judged per conversation
    /// against the messages table — the service-global `last_completed_at`
    /// advances for unrelated jobs and reported false "completed" exits for
    /// turns that never delivered anything (ctox#21).
    pub fn latest_assistant_message(&self, conversation_id: i64) -> Result<Option<MessageRecord>> {
        self.conn
            .query_row(
                "SELECT message_id, conversation_id, seq, role, content, token_count, created_at, agent_outcome
                 FROM messages
                 WHERE conversation_id = ?1 AND role = 'assistant'
                 ORDER BY seq DESC LIMIT 1",
                [conversation_id],
                |row| {
                    Ok(MessageRecord {
                        message_id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        seq: row.get(2)?,
                        role: row.get(3)?,
                        content: row.get(4)?,
                        token_count: row.get(5)?,
                        created_at: row.get(6)?,
                        agent_outcome: row.get(7)?,
                    })
                },
            )
            .optional()
            .context("failed to load latest assistant message")
    }

    pub fn evaluate_compaction(
        &self,
        conversation_id: i64,
        token_budget: i64,
    ) -> Result<CompactionDecision> {
        let current_tokens = self.context_token_count(conversation_id)?;
        let threshold = self.config.compaction_threshold(token_budget);
        Ok(CompactionDecision {
            should_compact: current_tokens > threshold,
            reason: if current_tokens > threshold {
                "threshold".to_string()
            } else {
                "none".to_string()
            },
            current_tokens,
            threshold,
        })
    }

    pub fn compact<S: Summarizer>(
        &self,
        conversation_id: i64,
        token_budget: i64,
        summarizer: &S,
        force: bool,
    ) -> Result<CompactionResult> {
        let tokens_before = self.context_token_count(conversation_id)?;
        let threshold = self.config.compaction_threshold(token_budget);
        if !force && tokens_before <= threshold {
            return Ok(CompactionResult {
                action_taken: false,
                tokens_before,
                tokens_after: tokens_before,
                created_summary_ids: Vec::new(),
                rounds: 0,
            });
        }

        let mut created = Vec::new();
        let mut rounds = 0usize;
        let mut previous_tokens = tokens_before;

        while rounds < self.config.max_rounds {
            rounds += 1;
            let Some(summary_id) =
                self.compact_leaf_pass(conversation_id, summarizer, force, previous_tokens)?
            else {
                break;
            };
            created.push(summary_id);

            let current = self.context_token_count(conversation_id)?;
            if !force && current <= threshold {
                break;
            }
            if current >= previous_tokens {
                break;
            }
            previous_tokens = current;
        }

        while rounds < self.config.max_rounds && (force || previous_tokens > threshold) {
            rounds += 1;
            let Some(summary_id) =
                self.compact_condensed_pass(conversation_id, summarizer, force, previous_tokens)?
            else {
                break;
            };
            created.push(summary_id);

            let current = self.context_token_count(conversation_id)?;
            if !force && current <= threshold {
                break;
            }
            if current >= previous_tokens {
                break;
            }
            previous_tokens = current;
        }

        if !created.is_empty() {
            self.resequence_context_items(conversation_id)?;
        }
        let tokens_after = self.context_token_count(conversation_id)?;
        Ok(CompactionResult {
            action_taken: !created.is_empty(),
            tokens_before,
            tokens_after,
            created_summary_ids: created,
            rounds,
        })
    }

    pub fn grep(
        &self,
        conversation_id: Option<i64>,
        scope: GrepScope,
        mode: GrepMode,
        query: &str,
        limit: usize,
    ) -> Result<GrepResult> {
        let messages = match scope {
            GrepScope::Messages | GrepScope::Both => {
                self.search_messages(conversation_id, mode, query, limit)?
            }
            GrepScope::Summaries => Vec::new(),
        };
        let summaries = match scope {
            GrepScope::Summaries | GrepScope::Both => {
                self.search_summaries(conversation_id, mode, query, limit)?
            }
            GrepScope::Messages => Vec::new(),
        };
        Ok(GrepResult {
            total_matches: messages.len() + summaries.len(),
            messages,
            summaries,
        })
    }

    pub fn describe(&self, id: &str) -> Result<Option<DescribeResult>> {
        let summary = self.get_summary(id)?;
        let Some(summary) = summary else {
            return Ok(None);
        };
        let parent_ids = self.summary_parent_ids(id)?;
        let child_ids = self.summary_child_ids(id)?;
        let message_ids = self.summary_message_ids(id)?;
        let subtree = self.summary_subtree(id)?;
        Ok(Some(DescribeResult::Summary(DescribeSummary {
            summary,
            parent_ids,
            child_ids,
            message_ids,
            subtree,
        })))
    }

    pub fn expand(
        &self,
        summary_id: &str,
        depth: usize,
        include_messages: bool,
        token_cap: i64,
    ) -> Result<ExpandResult> {
        let mut estimated = 0i64;
        let mut truncated = false;
        let mut children = Vec::new();
        let mut messages = Vec::new();
        let mut queue = vec![(summary_id.to_string(), 0usize)];

        while let Some((current, current_depth)) = queue.pop() {
            if current_depth >= depth {
                continue;
            }
            for child in self.child_summaries(&current)? {
                if estimated + child.token_count > token_cap {
                    truncated = true;
                    break;
                }
                estimated += child.token_count;
                children.push(ExpandChild {
                    summary_id: child.summary_id.clone(),
                    kind: child.kind,
                    content: child.content.clone(),
                    token_count: child.token_count,
                });
                queue.push((child.summary_id, current_depth + 1));
            }
            if truncated {
                break;
            }
        }

        if include_messages && !truncated {
            for message in self.messages_for_summary(summary_id)? {
                if estimated + message.token_count > token_cap {
                    truncated = true;
                    break;
                }
                estimated += message.token_count;
                messages.push(ExpandMessage {
                    message_id: message.message_id,
                    role: message.role,
                    content: message.content,
                    token_count: message.token_count,
                });
            }
        }

        Ok(ExpandResult {
            children,
            messages,
            estimated_tokens: estimated,
            truncated,
        })
    }

    pub fn snapshot(&self, conversation_id: i64) -> Result<LcmSnapshot> {
        let messages = self.messages_for_conversation(conversation_id)?;
        let summaries = self.summaries_for_conversation(conversation_id)?;
        let context_items = self
            .context_entries(conversation_id)?
            .into_iter()
            .map(|entry| ContextItemSnapshot {
                ordinal: entry.ordinal,
                item_type: entry.item_type,
                message_id: entry.message_id,
                summary_id: entry.summary_id,
                seq: entry.seq,
                depth: entry.depth,
                token_count: entry.token_count,
            })
            .collect();
        let summary_edges = self.summary_edges_for_conversation(conversation_id)?;
        let summary_messages = self.summary_message_links_for_conversation(conversation_id)?;
        Ok(LcmSnapshot {
            conversation_id,
            messages,
            summaries,
            context_items,
            summary_edges,
            summary_messages,
        })
    }

    /// Bounded live-turn view. Full history remains available through
    /// `snapshot` for audit/retrieval/maintenance commands, but the agent turn
    /// only materializes current context items and their referenced records.
    pub fn working_set_snapshot(
        &self,
        conversation_id: i64,
        max_context_items: usize,
    ) -> Result<LcmSnapshot> {
        let limit = max_context_items.max(1).min(2_048) as i64;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT ci.ordinal, ci.item_type, ci.message_id, ci.summary_id,
                   COALESCE(m.seq, ci.ordinal), COALESCE(s.depth, 0),
                   COALESCE(m.token_count, s.token_count, 0)
            FROM context_items ci
            LEFT JOIN messages m ON m.message_id=ci.message_id
            LEFT JOIN summaries s ON s.summary_id=ci.summary_id
            WHERE ci.conversation_id=?1
            ORDER BY ci.ordinal DESC
            LIMIT ?2
            "#,
        )?;
        let mut entries = stmt
            .query_map(rusqlite::params![conversation_id, limit], |row| {
                Ok(ContextEntry {
                    ordinal: row.get(0)?,
                    item_type: match row.get::<_, String>(1)?.as_str() {
                        "message" => ContextItemType::Message,
                        _ => ContextItemType::Summary,
                    },
                    message_id: row.get(2)?,
                    summary_id: row.get(3)?,
                    seq: row.get(4)?,
                    depth: row.get(5)?,
                    token_count: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        entries.reverse();
        let mut messages = Vec::new();
        let mut summaries = Vec::new();
        for entry in &entries {
            if let Some(message_id) = entry.message_id {
                messages.push(self.get_message(message_id)?);
            }
            if let Some(summary_id) = entry.summary_id.as_deref() {
                if let Some(mut summary) = self.get_summary(summary_id)? {
                    summary.content = format!(
                        "[summary_id={} source_tokens={} omissions=possible]\n{}",
                        summary.summary_id, summary.source_message_token_count, summary.content
                    );
                    summaries.push(summary);
                }
            }
        }
        let context_items = entries
            .into_iter()
            .map(|entry| ContextItemSnapshot {
                ordinal: entry.ordinal,
                item_type: entry.item_type,
                message_id: entry.message_id,
                summary_id: entry.summary_id,
                seq: entry.seq,
                depth: entry.depth,
                token_count: entry.token_count,
            })
            .collect();
        Ok(LcmSnapshot {
            conversation_id,
            messages,
            summaries,
            context_items,
            summary_edges: Vec::new(),
            summary_messages: Vec::new(),
        })
    }

    pub fn refresh_continuity(&self, conversation_id: i64) -> Result<ContinuityRevision> {
        let _ = self.continuity_init_documents(conversation_id)?;
        self.latest_continuity(conversation_id)?
            .context("continuity documents missing after init")
    }

    pub fn latest_continuity(&self, conversation_id: i64) -> Result<Option<ContinuityRevision>> {
        let show_all = self.continuity_show_all(conversation_id)?;
        let snapshot = self.snapshot(conversation_id)?;
        let revision_id = continuity_heads_revision_id(
            conversation_id,
            &show_all.narrative.head_commit_id,
            &show_all.anchors.head_commit_id,
            &show_all.focus.head_commit_id,
        );
        let created_at = std::cmp::max(
            show_all.narrative.updated_at.clone(),
            std::cmp::max(
                show_all.anchors.updated_at.clone(),
                show_all.focus.updated_at.clone(),
            ),
        );
        Ok(Some(ContinuityRevision {
            revision_id,
            conversation_id,
            narrative: show_all.narrative.content,
            anchors: show_all.anchors.content,
            focus: show_all.focus.content,
            source_summary_ids: snapshot
                .summaries
                .iter()
                .map(|summary| summary.summary_id.clone())
                .collect(),
            source_message_ids: snapshot
                .messages
                .iter()
                .map(|message| message.message_id)
                .collect(),
            created_at,
        }))
    }

    pub fn continuity_init_documents(&self, conversation_id: i64) -> Result<ContinuityShowAll> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin continuity init transaction")?;
        let mut show_all = load_or_init_continuity_show_all(&tx, conversation_id)?;
        let (record, imported) = load_or_import_mission_state_with(&tx, &show_all)?;
        let reason = if imported {
            "Imported legacy focus continuity into typed mission state and rendered its canonical view."
        } else {
            "Rendered focus continuity from typed mission state."
        };
        let _ = render_focus_continuity_with(&tx, &mut show_all, &record, reason)?;
        tx.commit()
            .context("failed to commit continuity init transaction")?;
        Ok(show_all)
    }

    pub fn continuity_show(
        &self,
        conversation_id: i64,
        kind: ContinuityKind,
    ) -> Result<ContinuityDocumentState> {
        self.ensure_continuity_document(conversation_id, kind)
    }

    pub fn continuity_show_all(&self, conversation_id: i64) -> Result<ContinuityShowAll> {
        self.continuity_init_documents(conversation_id)
    }

    pub fn stored_continuity_show_all(&self, conversation_id: i64) -> Result<ContinuityShowAll> {
        load_continuity_show_all_with(&self.conn, conversation_id)
    }

    pub fn continuity_log(
        &self,
        conversation_id: i64,
        kind: Option<ContinuityKind>,
    ) -> Result<Vec<ContinuityCommitRecord>> {
        let mut out = Vec::new();
        let kinds = if let Some(kind) = kind {
            vec![kind]
        } else {
            vec![
                ContinuityKind::Narrative,
                ContinuityKind::Anchors,
                ContinuityKind::Focus,
            ]
        };
        for kind in kinds {
            let document = self.ensure_continuity_document(conversation_id, kind)?;
            let mut commits = self.continuity_commits_for_document(
                &document.head_commit_id,
                conversation_id,
                kind,
            )?;
            out.append(&mut commits);
        }
        out.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        Ok(out)
    }

    pub fn continuity_apply_diff(
        &self,
        conversation_id: i64,
        kind: ContinuityKind,
        diff_text: &str,
    ) -> Result<ContinuityDocumentState> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin continuity apply transaction")?;
        let document = ensure_continuity_document_with(&tx, conversation_id, kind)?;
        let normalized_diff = normalize_continuity_diff(kind, diff_text)?;
        let rendered = apply_continuity_diff(kind, &document.content, &normalized_diff)?;
        let created_at = iso_now();
        let commit_id = continuity_commit_id(
            conversation_id,
            kind,
            &normalized_diff,
            &rendered,
            &created_at,
        );
        let document_id = continuity_document_id(conversation_id, kind);
        tx.execute(
            "INSERT INTO continuity_commits (commit_id, document_id, parent_commit_id, diff_text, rendered_text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                commit_id,
                document_id,
                document.head_commit_id,
                normalized_diff,
                rendered,
                created_at
            ],
        )?;
        tx.execute(
            "UPDATE continuity_documents SET head_commit_id = ?1, updated_at = ?2 WHERE document_id = ?3",
            params![commit_id, created_at, document_id],
        )?;
        let mut continuity = load_or_init_continuity_show_all(&tx, conversation_id)?;
        if kind == ContinuityKind::Focus {
            let (record, imported) = load_or_import_mission_state_with(&tx, &continuity)?;
            // The just-applied model diff remains the last writer even when this
            // transaction also performed the one-time legacy import. Imported
            // free-form text keeps legacy parsing semantics; only stale control
            // defaults from the old focus body are reconciled here (e10735662).
            let updated = if imported {
                apply_imported_focus_diff_controls(
                    &record,
                    &continuity.focus.content,
                    &normalized_diff,
                    &continuity.focus.head_commit_id,
                )?
            } else {
                apply_canonical_focus_diff_to_mission_state(
                    &record,
                    &continuity.focus.content,
                    &normalized_diff,
                    &continuity.focus.head_commit_id,
                )?
            };
            persist_mission_state_with(&tx, &updated)?;
            let record = load_mission_state_with(&tx, conversation_id)?
                .context("mission state missing after applying canonical focus fields")?;
            let reason = if imported {
                "Imported the legacy focus document once and rendered typed mission state."
            } else {
                "Applied canonical focus fields to typed mission state and rendered them."
            };
            let _ = render_focus_continuity_with(&tx, &mut continuity, &record, reason)?;
        }
        let document = match kind {
            ContinuityKind::Narrative => continuity.narrative,
            ContinuityKind::Anchors => continuity.anchors,
            ContinuityKind::Focus => continuity.focus,
        };
        tx.commit()
            .context("failed to commit continuity apply transaction")?;
        Ok(document)
    }

    /// Replace the entire body of a continuity document. The previous
    /// content is discarded; `new_content` becomes the new `rendered_text`.
    /// Used by the tool-based refresh path where the model decides the full
    /// new document rather than emitting a diff. The `diff_text` audit trail
    /// is a sentinel so we can distinguish tool-written commits from
    /// diff-merge commits when debugging.
    pub fn continuity_full_replace_document(
        &self,
        conversation_id: i64,
        kind: ContinuityKind,
        new_content: &str,
    ) -> Result<ContinuityDocumentState> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin continuity full-replace transaction")?;
        let document = ensure_continuity_document_with(&tx, conversation_id, kind)?;
        let rendered = new_content.trim().to_string();
        if rendered.is_empty() {
            anyhow::bail!("continuity_full_replace_document: empty content");
        }
        let created_at = iso_now();
        let diff_audit = format!("<tool:full_replace len={}>", rendered.len());
        let commit_id =
            continuity_commit_id(conversation_id, kind, &diff_audit, &rendered, &created_at);
        let document_id = continuity_document_id(conversation_id, kind);
        tx.execute(
            "INSERT INTO continuity_commits (commit_id, document_id, parent_commit_id, diff_text, rendered_text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                commit_id,
                document_id,
                document.head_commit_id,
                diff_audit,
                rendered,
                created_at
            ],
        )?;
        tx.execute(
            "UPDATE continuity_documents SET head_commit_id = ?1, updated_at = ?2 WHERE document_id = ?3",
            params![commit_id, created_at, document_id],
        )?;
        let mut continuity = load_or_init_continuity_show_all(&tx, conversation_id)?;
        if kind == ContinuityKind::Focus {
            let (record, imported) = load_or_import_mission_state_with(&tx, &continuity)?;
            let reason = if imported {
                "Imported the legacy focus document once and rendered typed mission state."
            } else {
                "Rendered focus continuity from typed mission state after a full replacement."
            };
            let _ = render_focus_continuity_with(&tx, &mut continuity, &record, reason)?;
        }
        let document = match kind {
            ContinuityKind::Narrative => continuity.narrative,
            ContinuityKind::Anchors => continuity.anchors,
            ContinuityKind::Focus => continuity.focus,
        };
        tx.commit()
            .context("failed to commit continuity full-replace transaction")?;
        Ok(document)
    }

    /// Apply a single literal string replacement to a continuity document.
    /// `find` must occur exactly once in the current content; otherwise the
    /// call errors (fail-loud rather than silently applying the wrong edit).
    /// Used by the tool-based refresh path for small targeted updates like
    /// "Mission state: open" -> "Mission state: done".
    pub fn continuity_string_replace_document(
        &self,
        conversation_id: i64,
        kind: ContinuityKind,
        find: &str,
        replace: &str,
    ) -> Result<ContinuityDocumentState> {
        if find.is_empty() {
            anyhow::bail!("continuity_string_replace_document: find is empty");
        }
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin continuity string-replace transaction")?;
        let document = ensure_continuity_document_with(&tx, conversation_id, kind)?;
        let before = document.content.clone();
        let matches: usize = before.matches(find).count();
        if matches == 0 {
            anyhow::bail!(
                "continuity_string_replace_document: find string not present in {} document",
                kind.as_str()
            );
        }
        if matches > 1 {
            anyhow::bail!(
                "continuity_string_replace_document: find string matches {matches} times in {} document; refusing ambiguous replace",
                kind.as_str()
            );
        }
        let rendered = before.replacen(find, replace, 1);
        if rendered == before {
            anyhow::bail!("continuity_string_replace_document: replace produced no change");
        }
        let created_at = iso_now();
        let diff_audit = format!(
            "<tool:string_replace find_len={} replace_len={}>",
            find.len(),
            replace.len()
        );
        let commit_id =
            continuity_commit_id(conversation_id, kind, &diff_audit, &rendered, &created_at);
        let document_id = continuity_document_id(conversation_id, kind);
        tx.execute(
            "INSERT INTO continuity_commits (commit_id, document_id, parent_commit_id, diff_text, rendered_text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                commit_id,
                document_id,
                document.head_commit_id,
                diff_audit,
                rendered,
                created_at
            ],
        )?;
        tx.execute(
            "UPDATE continuity_documents SET head_commit_id = ?1, updated_at = ?2 WHERE document_id = ?3",
            params![commit_id, created_at, document_id],
        )?;
        let mut continuity = load_or_init_continuity_show_all(&tx, conversation_id)?;
        if kind == ContinuityKind::Focus {
            let (record, imported) = load_or_import_mission_state_with(&tx, &continuity)?;
            let reason = if imported {
                "Imported the legacy focus document once and rendered typed mission state."
            } else {
                "Rendered focus continuity from typed mission state after a string replacement."
            };
            let _ = render_focus_continuity_with(&tx, &mut continuity, &record, reason)?;
        }
        let document = match kind {
            ContinuityKind::Narrative => continuity.narrative,
            ContinuityKind::Anchors => continuity.anchors,
            ContinuityKind::Focus => continuity.focus,
        };
        tx.commit()
            .context("failed to commit continuity string-replace transaction")?;
        Ok(document)
    }

    pub fn rewrite_secret_literal(
        &self,
        conversation_id: i64,
        secret_scope: &str,
        secret_name: &str,
        match_text: &str,
        replacement_text: &str,
    ) -> Result<SecretRewriteResult> {
        anyhow::ensure!(
            !match_text.trim().is_empty(),
            "match_text must not be empty for secret rewrite"
        );
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin secret rewrite transaction")?;
        let message_rows_updated =
            rewrite_message_rows_with(&tx, conversation_id, match_text, replacement_text)?;
        let summary_rows_updated =
            rewrite_summary_rows_with(&tx, conversation_id, match_text, replacement_text)?;
        let continuity_commit_rows_updated = rewrite_continuity_commit_rows_with(
            &tx,
            conversation_id,
            match_text,
            replacement_text,
        )?;
        let continuity_revision_rows_updated = rewrite_continuity_revision_rows_with(
            &tx,
            conversation_id,
            match_text,
            replacement_text,
        )?;
        let mission_state_rows_updated =
            rewrite_mission_state_rows_with(&tx, conversation_id, match_text, replacement_text)?;
        let verification_rows_updated =
            rewrite_verification_rows_with(&tx, conversation_id, match_text, replacement_text)?;
        let claim_rows_updated =
            rewrite_claim_rows_with(&tx, conversation_id, match_text, replacement_text)?;
        let created_at = iso_now();
        let rewrite_id = format!(
            "secret-rewrite:{}:{}:{}",
            conversation_id,
            explicit_anchor_literal_suffix(secret_scope),
            explicit_anchor_literal_suffix(&(secret_name.to_string() + replacement_text))
        );
        tx.execute(
            "INSERT INTO secret_rewrites (
                rewrite_id, conversation_id, secret_scope, secret_name, replacement_text, match_digest,
                message_rows_updated, summary_rows_updated, continuity_commit_rows_updated,
                continuity_revision_rows_updated, mission_state_rows_updated, verification_rows_updated,
                claim_rows_updated, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                rewrite_id,
                conversation_id,
                secret_scope,
                secret_name,
                replacement_text,
                explicit_anchor_literal_suffix(match_text),
                message_rows_updated as i64,
                summary_rows_updated as i64,
                continuity_commit_rows_updated as i64,
                continuity_revision_rows_updated as i64,
                mission_state_rows_updated as i64,
                verification_rows_updated as i64,
                claim_rows_updated as i64,
                created_at,
            ],
        )?;
        tx.commit()
            .context("failed to commit secret rewrite transaction")?;
        Ok(SecretRewriteResult {
            rewrite_id,
            conversation_id,
            secret_scope: secret_scope.to_string(),
            secret_name: secret_name.to_string(),
            replacement_text: replacement_text.to_string(),
            message_rows_updated,
            summary_rows_updated,
            continuity_commit_rows_updated,
            continuity_revision_rows_updated,
            mission_state_rows_updated,
            verification_rows_updated,
            claim_rows_updated,
            created_at,
        })
    }

    pub fn mission_state(&self, conversation_id: i64) -> Result<MissionStateRecord> {
        self.sync_mission_state_from_continuity(conversation_id)
    }

    pub fn stored_mission_state(&self, conversation_id: i64) -> Result<Option<MissionStateRecord>> {
        load_mission_state_with(&self.conn, conversation_id)
    }

    /// Read structured mission state without rendering focus continuity or
    /// persisting the legacy fallback. This is the inspection path for guards
    /// that must report drift without repairing it.
    pub fn peek_mission_state(&self, conversation_id: i64) -> Result<MissionStateRecord> {
        if let Some(record) = self.stored_mission_state(conversation_id)? {
            return Ok(record);
        }
        let continuity = self.stored_continuity_show_all(conversation_id)?;
        Ok(import_legacy_mission_state(&continuity))
    }

    pub fn list_mission_states(&self, open_only: bool) -> Result<Vec<MissionStateRecord>> {
        load_mission_states_with(&self.conn, open_only)
    }

    /// Compatibility name retained for service callers. The old repair loop is
    /// gone: this loads (or one-time imports) structured state and normally
    /// renders focus continuity from that source.
    pub fn sync_mission_state_from_continuity_with_repair(
        &self,
        conversation_id: i64,
    ) -> Result<MissionStateRepairOutcome> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin mission render transaction")?;
        let mut continuity = load_or_init_continuity_show_all(&tx, conversation_id)?;
        let previous_focus_head_commit_id = continuity.focus.head_commit_id.clone();
        let (record, imported) = load_or_import_mission_state_with(&tx, &continuity)?;
        let reason = if imported {
            "Imported legacy focus continuity into typed mission state and rendered it."
        } else {
            "Rendered focus continuity from typed mission state."
        };
        let (record, focus_rendered) =
            render_focus_continuity_with(&tx, &mut continuity, &record, reason)?;
        tx.commit()
            .context("failed to commit mission render transaction")?;
        Ok(MissionStateRepairOutcome {
            mission_state: record,
            previous_focus_head_commit_id,
            focus_head_commit_id: continuity.focus.head_commit_id.clone(),
            focus_repaired: focus_rendered,
            reopened_for_open_runtime_work: false,
        })
    }

    pub fn sync_mission_state_from_continuity(
        &self,
        conversation_id: i64,
    ) -> Result<MissionStateRecord> {
        Ok(self
            .sync_mission_state_from_continuity_with_repair(conversation_id)?
            .mission_state)
    }

    pub fn note_mission_watcher_triggered(
        &self,
        conversation_id: i64,
        triggered_at: &str,
    ) -> Result<MissionStateRecord> {
        let mut record = self.mission_state(conversation_id)?;
        record.watcher_last_triggered_at = Some(triggered_at.to_string());
        record.watcher_trigger_count += 1;
        self.persist_mission_state(&record)?;
        Ok(record)
    }

    /// F2: increment the per-mission agent-failure counter when an agent
    /// turn ended with a non-success outcome (timeout, panic, runtime error).
    /// Returns the post-increment record so the caller can decide whether
    /// the watchdog should defer the mission.
    pub fn increment_mission_agent_failure_count(
        &self,
        conversation_id: i64,
    ) -> Result<MissionStateRecord> {
        let mut record = self.mission_state(conversation_id)?;
        record.agent_failure_count = record.agent_failure_count.saturating_add(1);
        self.persist_mission_state(&record)?;
        Ok(record)
    }

    /// F2: reset the per-mission agent-failure counter on a successful turn.
    /// No-op when already zero (avoids touching the row unnecessarily).
    pub fn reset_mission_agent_failure_count(
        &self,
        conversation_id: i64,
    ) -> Result<MissionFailureReset> {
        let mut record = self.mission_state(conversation_id)?;
        // govrec-5: capture the pre-reset deferral reason so the caller can audit
        // the recovery (deferred -> running) exactly once. The post-reset record
        // already has deferred_reason == None, so without this the un-defer is
        // invisible and the defer/recover governance pair is one-sided.
        let previous_deferred_reason = record.deferred_reason.clone();
        let recovered_at = iso_now();
        if record.agent_failure_count == 0 && record.deferred_reason.is_none() {
            return Ok(MissionFailureReset {
                record,
                previous_deferred_reason,
                recovered_at,
            });
        }
        record.agent_failure_count = 0;
        // Defer/recover is one symmetric state transition. The threshold writer
        // stores deferred/closed/idle; recovery restores the full active/open/
        // non-idle combination instead of clearing only two fields of the state.
        if previous_deferred_reason.as_deref() == Some("agent_failure_threshold") {
            record.mission_status = "active".to_string();
            record.is_open = true;
            record.allow_idle = false;
        }
        record.deferred_reason = None;
        self.persist_mission_state(&record)?;
        Ok(MissionFailureReset {
            record,
            previous_deferred_reason,
            recovered_at,
        })
    }

    /// Increment the per-mission rewrite-only review failure counter when a
    /// rewrite-class review iteration failed to converge (next reviewer
    /// verdict is again non-PASS for the same artifact). Returns the
    /// post-increment record so the caller can decide whether the
    /// dispatcher should defer the mission.
    pub fn increment_mission_rewrite_failure_count(
        &self,
        conversation_id: i64,
    ) -> Result<MissionStateRecord> {
        let mut record = self.mission_state(conversation_id)?;
        record.rewrite_failure_count = record.rewrite_failure_count.saturating_add(1);
        self.persist_mission_state(&record)?;
        Ok(record)
    }

    /// Reset the per-mission rewrite-only review failure counter on a
    /// successful approval. No-op when already zero.
    pub fn reset_mission_rewrite_failure_count(
        &self,
        conversation_id: i64,
    ) -> Result<MissionStateRecord> {
        let mut record = self.mission_state(conversation_id)?;
        if record.rewrite_failure_count == 0 {
            return Ok(record);
        }
        record.rewrite_failure_count = 0;
        self.persist_mission_state(&record)?;
        Ok(record)
    }

    /// F2: defer a mission because the agent-failure threshold was hit.
    /// Sets `mission_status = 'deferred'`, stores a structured reason, and
    /// flips `is_open=false` / `allow_idle=true` so the watchdog stops
    /// spawning continuation internal work for this mission.
    pub fn defer_mission_for_reason(
        &self,
        conversation_id: i64,
        reason: &str,
    ) -> Result<MissionStateRecord> {
        let mut record = self.mission_state(conversation_id)?;
        record.mission_status = "deferred".to_string();
        record.deferred_reason = Some(reason.to_string());
        record.is_open = false;
        record.allow_idle = true;
        self.persist_mission_state(&record)?;
        Ok(record)
    }

    pub fn overwrite_mission_state(&self, record: &MissionStateRecord) -> Result<()> {
        self.persist_mission_state(record)
    }

    /// P2 — explicit owner-intent path for clearing the protected
    /// `mission_states.next_slice` / `mission_states.done_gate` fields.
    ///
    /// The clobber guard in `persist_mission_state_with` rejects any
    /// automation write that would silently empty those fields. When an
    /// operator or skill genuinely needs to clear them (e.g. a mission was
    /// completed and the owner is retiring the slice), this method flips a
    /// thread-local bypass for the duration of the write so the guard does
    /// not interpret it as accidental clobbering. **Do not call this from
    /// automation.** The harness uses the guarded path; operator/skill
    /// callers can reach this through a dedicated entry point.
    pub fn clear_mission_state_done_fields_with_owner_intent(
        &self,
        conversation_id: i64,
        clear_next_slice: bool,
        clear_done_gate: bool,
    ) -> Result<MissionStateRecord> {
        let mut record = self.mission_state(conversation_id)?;
        if clear_next_slice {
            record.next_slice = String::new();
        }
        if clear_done_gate {
            record.done_gate = String::new();
        }
        let _bypass = OwnerIntentClearGuard::enter();
        self.persist_mission_state(&record)?;
        Ok(record)
    }

    /// Convenience method calling [`drain_pending_mission_state_clobber_events_to_governance`].
    pub fn drain_pending_mission_state_clobber_events_to_governance(&self, root: &Path) {
        drain_pending_mission_state_clobber_events_to_governance(root);
    }

    pub fn rewrite_focus_continuity_from_mission_state(
        &self,
        conversation_id: i64,
        record: &MissionStateRecord,
        reason: &str,
    ) -> Result<bool> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .context("failed to begin focus continuity rewrite transaction")?;
        anyhow::ensure!(
            record.conversation_id == conversation_id,
            "mission-state conversation id does not match focus render target"
        );
        let mut continuity = load_or_init_continuity_show_all(&tx, conversation_id)?;
        persist_mission_state_with(&tx, record)?;
        let effective = load_mission_state_with(&tx, conversation_id)?
            .context("mission state missing before focus render")?;
        let (_, rendered) = render_focus_continuity_with(&tx, &mut continuity, &effective, reason)?;
        tx.commit()
            .context("failed to commit focus continuity render transaction")?;
        Ok(rendered)
    }

    pub fn persist_verification_run(
        &self,
        run: &VerificationRunRecord,
        claims: &[MissionClaimRecord],
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO verification_runs (
                run_id,
                conversation_id,
                source_label,
                goal,
                preview,
                result_excerpt,
                blocker,
                review_required,
                review_verdict,
                review_summary,
                review_score,
                review_reasons,
                report_excerpt,
                raw_report,
                mission_state,
                failed_gates_json,
                semantic_findings_json,
                open_items_json,
                evidence_json,
                handoff_text,
                claim_count,
                open_claim_count,
                closure_blocking_claim_count,
                created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            params![
                run.run_id,
                run.conversation_id,
                run.source_label,
                run.goal,
                run.preview,
                run.result_excerpt,
                run.blocker,
                if run.review_required { 1 } else { 0 },
                run.review_verdict,
                run.review_summary,
                run.review_score,
                serde_json::to_string(&run.review_reasons)?,
                run.report_excerpt,
                run.raw_report,
                run.mission_state,
                serde_json::to_string(&run.failed_gates)?,
                serde_json::to_string(&run.semantic_findings)?,
                serde_json::to_string(&run.open_items)?,
                serde_json::to_string(&run.evidence)?,
                run.handoff.clone().unwrap_or_default(),
                run.claim_count,
                run.open_claim_count,
                run.closure_blocking_claim_count,
                run.created_at,
            ],
        )?;

        for claim in claims {
            self.upsert_mission_claim(claim)?;
        }
        Ok(())
    }

    pub fn upsert_mission_claim(&self, claim: &MissionClaimRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO mission_claims (
                claim_key,
                conversation_id,
                last_run_id,
                claim_kind,
                claim_status,
                blocks_closure,
                subject,
                summary,
                evidence_summary,
                recheck_policy,
                expires_at,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(claim_key) DO UPDATE SET
                conversation_id = excluded.conversation_id,
                last_run_id = excluded.last_run_id,
                claim_kind = excluded.claim_kind,
                claim_status = excluded.claim_status,
                blocks_closure = excluded.blocks_closure,
                subject = excluded.subject,
                summary = excluded.summary,
                evidence_summary = excluded.evidence_summary,
                recheck_policy = excluded.recheck_policy,
                expires_at = excluded.expires_at,
                updated_at = excluded.updated_at",
            params![
                claim.claim_key,
                claim.conversation_id,
                claim.last_run_id,
                claim.claim_kind,
                claim.claim_status,
                if claim.blocks_closure { 1 } else { 0 },
                claim.subject,
                claim.summary,
                claim.evidence_summary,
                claim.recheck_policy,
                claim.expires_at,
                claim.created_at,
                claim.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_verification_runs(
        &self,
        conversation_id: i64,
        limit: usize,
    ) -> Result<Vec<VerificationRunRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT run_id, source_label, goal, preview, result_excerpt, blocker, review_required, review_verdict, review_summary, review_score, review_reasons, report_excerpt, raw_report, mission_state, failed_gates_json, semantic_findings_json, open_items_json, evidence_json, handoff_text, claim_count, open_claim_count, closure_blocking_claim_count, created_at
             FROM verification_runs
             WHERE conversation_id = ?1
             ORDER BY CAST(created_at AS INTEGER) DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![conversation_id, limit as i64], |row| {
            Ok(map_verification_run_row(row, conversation_id)?)
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn latest_verification_run(
        &self,
        conversation_id: i64,
    ) -> Result<Option<VerificationRunRecord>> {
        self.list_verification_runs(conversation_id, 1)
            .map(|mut runs| runs.pop())
    }

    pub fn list_mission_claims(
        &self,
        conversation_id: i64,
        include_verified: bool,
        limit: usize,
    ) -> Result<Vec<MissionClaimRecord>> {
        if include_verified {
            let mut stmt = self.conn.prepare(
                "SELECT claim_key, last_run_id, claim_kind, claim_status, blocks_closure, subject, summary, evidence_summary, recheck_policy, expires_at, created_at, updated_at
                 FROM mission_claims
                 WHERE conversation_id = ?1
                 ORDER BY CAST(updated_at AS INTEGER) DESC
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![conversation_id, limit as i64], |row| {
                Ok(map_mission_claim_row(row, conversation_id)?)
            })?;
            return Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?);
        }

        // Bind the expiry cutoff as an INTEGER: binding the epoch-millis
        // string as TEXT makes `CAST(expires_at AS INTEGER) <= ?2` compare
        // INTEGER vs TEXT, which is always true in SQLite — every verified
        // claim with an expiry then permanently re-enters the open set.
        let now_millis: i64 = iso_now().parse().unwrap_or(i64::MAX);
        let mut stmt = self.conn.prepare(
            "SELECT claim_key, last_run_id, claim_kind, claim_status, blocks_closure, subject, summary, evidence_summary, recheck_policy, expires_at, created_at, updated_at
             FROM mission_claims
             WHERE conversation_id = ?1
               AND (claim_status != 'verified' OR (expires_at IS NOT NULL AND CAST(expires_at AS INTEGER) <= ?2))
             ORDER BY CAST(updated_at AS INTEGER) DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![conversation_id, now_millis, limit as i64], |row| {
            Ok(map_mission_claim_row(row, conversation_id)?)
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn mission_assurance_snapshot(
        &self,
        conversation_id: i64,
    ) -> Result<MissionAssuranceSnapshot> {
        let latest_run = self.latest_verification_run(conversation_id)?;
        let open_claims = self.list_mission_claims(conversation_id, false, 64)?;
        let closure_blocking_claims = open_claims
            .iter()
            .filter(|claim| claim.blocks_closure)
            .cloned()
            .collect();
        Ok(MissionAssuranceSnapshot {
            conversation_id,
            latest_run,
            open_claims,
            closure_blocking_claims,
        })
    }

    pub fn create_strategic_directive(
        &self,
        conversation_id: i64,
        thread_key: Option<&str>,
        directive_kind: &str,
        title: &str,
        body_text: &str,
        status: &str,
        author: &str,
        decision_reason: Option<&str>,
    ) -> Result<StrategicDirectiveRecord> {
        let now = iso_now();
        let normalized_thread_key = thread_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let revision = self.next_strategy_revision(
            conversation_id,
            normalized_thread_key.as_deref(),
            directive_kind,
        )?;
        let directive_id = strategic_directive_id(
            conversation_id,
            normalized_thread_key.as_deref(),
            directive_kind,
            revision,
            &now,
        );
        let previous = if status == "active" {
            self.active_strategic_directive(
                conversation_id,
                normalized_thread_key.as_deref(),
                directive_kind,
            )?
        } else {
            None
        };
        if status == "active" {
            if let Some(previous) = previous.as_ref() {
                self.conn.execute(
                    "UPDATE strategic_directives
                     SET status = 'superseded', updated_at = ?1
                     WHERE directive_id = ?2",
                    params![now, previous.directive_id],
                )?;
            }
        }
        self.conn.execute(
            "INSERT INTO strategic_directives (
                directive_id,
                conversation_id,
                thread_key,
                directive_kind,
                title,
                body_text,
                status,
                revision,
                previous_directive_id,
                author,
                decided_by,
                decision_reason,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                directive_id,
                conversation_id,
                normalized_thread_key,
                directive_kind.trim(),
                title.trim(),
                body_text.trim(),
                status.trim(),
                revision,
                previous.as_ref().map(|item| item.directive_id.clone()),
                author.trim(),
                if status == "active" {
                    Some(author.trim().to_string())
                } else {
                    None
                },
                decision_reason.map(str::trim),
                now,
                now,
            ],
        )?;
        self.load_strategic_directive(&directive_id)?
            .context("new strategic directive missing after insert")
    }

    pub fn activate_strategic_directive(
        &self,
        directive_id: &str,
        decided_by: &str,
        decision_reason: Option<&str>,
    ) -> Result<StrategicDirectiveRecord> {
        let existing = self
            .load_strategic_directive(directive_id)?
            .with_context(|| format!("unknown strategic directive {directive_id}"))?;
        let now = iso_now();
        if let Some(active) = self.active_strategic_directive(
            existing.conversation_id,
            existing.thread_key.as_deref(),
            &existing.directive_kind,
        )? {
            if active.directive_id != existing.directive_id {
                self.conn.execute(
                    "UPDATE strategic_directives
                     SET status = 'superseded', updated_at = ?1
                     WHERE directive_id = ?2",
                    params![now, active.directive_id],
                )?;
            }
        }
        self.conn.execute(
            "UPDATE strategic_directives
             SET status = 'active',
                 decided_by = ?1,
                 decision_reason = COALESCE(?2, decision_reason),
                 updated_at = ?3
             WHERE directive_id = ?4",
            params![
                decided_by.trim(),
                decision_reason.map(str::trim),
                now,
                directive_id
            ],
        )?;
        self.load_strategic_directive(directive_id)?
            .context("strategic directive missing after activation")
    }

    pub fn active_strategy_snapshot(
        &self,
        conversation_id: i64,
        thread_key: Option<&str>,
    ) -> Result<StrategySnapshot> {
        let directives = self.list_strategic_directives(conversation_id, thread_key, None, 64)?;
        let active_vision =
            self.active_strategic_directive(conversation_id, thread_key, "vision")?;
        let active_mission =
            self.active_strategic_directive(conversation_id, thread_key, "mission")?;
        Ok(StrategySnapshot {
            conversation_id,
            thread_key: thread_key.map(ToOwned::to_owned),
            active_vision,
            active_mission,
            directives,
        })
    }

    pub fn list_strategic_directives(
        &self,
        conversation_id: i64,
        thread_key: Option<&str>,
        directive_kind: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StrategicDirectiveRecord>> {
        let normalized_thread_key = thread_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let mut stmt = self.conn.prepare(
            "SELECT directive_id, conversation_id, thread_key, directive_kind, title, body_text, status, revision, previous_directive_id, author, decided_by, decision_reason, created_at, updated_at
             FROM strategic_directives
             WHERE conversation_id = ?1
               AND (?2 IS NULL OR thread_key = ?2 OR thread_key IS NULL)
               AND (?3 IS NULL OR directive_kind = ?3)
             ORDER BY CASE WHEN thread_key = ?2 THEN 0 ELSE 1 END, revision DESC, updated_at DESC
             LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![
                conversation_id,
                normalized_thread_key,
                directive_kind.map(str::trim),
                limit as i64
            ],
            map_strategic_directive_row,
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn active_strategic_directive(
        &self,
        conversation_id: i64,
        thread_key: Option<&str>,
        directive_kind: &str,
    ) -> Result<Option<StrategicDirectiveRecord>> {
        let normalized_thread_key = thread_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let mut stmt = self.conn.prepare(
            "SELECT directive_id, conversation_id, thread_key, directive_kind, title, body_text, status, revision, previous_directive_id, author, decided_by, decision_reason, created_at, updated_at
             FROM strategic_directives
             WHERE conversation_id = ?1
               AND directive_kind = ?2
               AND status = 'active'
               AND (?3 IS NULL OR thread_key = ?3 OR thread_key IS NULL)
             ORDER BY CASE WHEN thread_key = ?3 THEN 0 ELSE 1 END, revision DESC, updated_at DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![
            conversation_id,
            directive_kind.trim(),
            normalized_thread_key
        ])?;
        match rows.next()? {
            Some(row) => Ok(Some(map_strategic_directive_row(row)?)),
            None => Ok(None),
        }
    }

    pub fn load_strategic_directive(
        &self,
        directive_id: &str,
    ) -> Result<Option<StrategicDirectiveRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT directive_id, conversation_id, thread_key, directive_kind, title, body_text, status, revision, previous_directive_id, author, decided_by, decision_reason, created_at, updated_at
             FROM strategic_directives
             WHERE directive_id = ?1
             LIMIT 1",
        )?;
        let mut rows = stmt.query([directive_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(map_strategic_directive_row(row)?)),
            None => Ok(None),
        }
    }

    fn next_strategy_revision(
        &self,
        conversation_id: i64,
        thread_key: Option<&str>,
        directive_kind: &str,
    ) -> Result<i64> {
        let normalized_thread_key = thread_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let revision = self.conn.query_row(
            "SELECT COALESCE(MAX(revision), 0) + 1
             FROM strategic_directives
             WHERE conversation_id = ?1
               AND directive_kind = ?2
               AND ((?3 IS NULL AND thread_key IS NULL) OR thread_key = ?3)",
            params![
                conversation_id,
                directive_kind.trim(),
                normalized_thread_key
            ],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(revision)
    }

    pub fn continuity_rebuild(
        &self,
        conversation_id: i64,
        kind: ContinuityKind,
    ) -> Result<ContinuityDocumentState> {
        let document_id = continuity_document_id(conversation_id, kind);
        let commits =
            self.continuity_commits_for_document_id(&document_id, conversation_id, kind)?;
        let base = continuity_template(kind).to_string();
        let rebuilt = commits.iter().skip(1).try_fold(base, |current, commit| {
            apply_continuity_diff(kind, &current, &commit.diff_text)
        })?;
        let head_commit_id = commits
            .last()
            .map(|commit| commit.commit_id.clone())
            .unwrap_or_else(|| continuity_base_commit_id(conversation_id, kind));
        let updated_at = commits
            .last()
            .map(|commit| commit.created_at.clone())
            .unwrap_or_else(iso_now);
        Ok(ContinuityDocumentState {
            conversation_id,
            kind,
            head_commit_id,
            content: rebuilt,
            created_at: commits
                .first()
                .map(|commit| commit.created_at.clone())
                .unwrap_or_else(iso_now),
            updated_at,
        })
    }

    pub fn continuity_forgotten(
        &self,
        conversation_id: i64,
        kind: Option<ContinuityKind>,
        query: Option<&str>,
    ) -> Result<Vec<ContinuityForgottenEntry>> {
        let query_lower = query.map(|value| value.to_lowercase());
        let commits = self.continuity_log(conversation_id, kind)?;
        let mut out = Vec::new();
        for commit in commits {
            for line in removed_lines_from_diff(&commit.diff_text) {
                if query_lower
                    .as_ref()
                    .map(|needle| line.to_lowercase().contains(needle))
                    .unwrap_or(true)
                {
                    out.push(ContinuityForgottenEntry {
                        commit_id: commit.commit_id.clone(),
                        conversation_id,
                        kind: commit.kind,
                        line,
                        created_at: commit.created_at.clone(),
                    });
                }
            }
        }
        Ok(out)
    }

    pub fn continuity_forgotten_recent(
        &self,
        conversation_id: i64,
        kind: Option<ContinuityKind>,
        limit: usize,
    ) -> Result<Vec<ContinuityForgottenEntry>> {
        let limit = limit.max(1).min(512) as i64;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT c.commit_id, d.kind, c.diff_text, c.created_at
            FROM continuity_commits c
            JOIN continuity_documents d ON d.document_id=c.document_id
            WHERE d.conversation_id=?1
              AND (?2 IS NULL OR d.kind=?2)
            ORDER BY c.created_at DESC, c.commit_id DESC
            LIMIT ?3
            "#,
        )?;
        let kind_filter = kind.map(|value| value.as_str().to_string());
        let rows = stmt
            .query_map(
                rusqlite::params![conversation_id, kind_filter, limit],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = Vec::new();
        for (commit_id, kind, diff_text, created_at) in rows {
            let kind = ContinuityKind::parse(&kind)?;
            for line in removed_lines_from_diff(&diff_text) {
                out.push(ContinuityForgottenEntry {
                    commit_id: commit_id.clone(),
                    conversation_id,
                    kind,
                    line,
                    created_at: created_at.clone(),
                });
            }
        }
        Ok(out)
    }

    pub fn continuity_build_prompt(
        &self,
        conversation_id: i64,
        kind: ContinuityKind,
    ) -> Result<ContinuityPromptPayload> {
        let document = self.ensure_continuity_document(conversation_id, kind)?;
        let snapshot = self.working_set_snapshot(conversation_id, 512)?;
        let explicit_anchor_literals = if kind == ContinuityKind::Anchors {
            collect_explicit_anchor_literals(&snapshot.messages)
        } else {
            Vec::new()
        };
        let forgotten = self
            .continuity_forgotten_recent(conversation_id, Some(kind), 64)?
            .into_iter()
            .rev()
            .take(8)
            .map(|entry| entry.line)
            .collect::<Vec<_>>();
        let recent_messages = snapshot
            .messages
            .iter()
            .rev()
            .take(8)
            .map(|message| {
                format!(
                    "[{} #{}] {}",
                    message.role,
                    message.seq,
                    sentence_fragment(
                        &message.content,
                        if kind == ContinuityKind::Anchors {
                            420
                        } else if kind == ContinuityKind::Focus {
                            520
                        } else {
                            220
                        },
                    )
                )
            })
            .collect::<Vec<_>>();
        let recent_summaries = snapshot
            .summaries
            .iter()
            .rev()
            .take(4)
            .map(|summary| {
                format!(
                    "[{} depth={}] {}",
                    summary.kind.as_str(),
                    summary.depth,
                    sentence_fragment(&summary.content, 240)
                )
            })
            .collect::<Vec<_>>();
        let prompt = build_continuity_prompt_text(
            conversation_id,
            kind,
            &document.content,
            &recent_messages,
            &recent_summaries,
            &forgotten,
            &explicit_anchor_literals,
        );

        Ok(ContinuityPromptPayload {
            conversation_id,
            kind,
            current_document: document.content,
            recent_messages,
            recent_summaries,
            forgotten_lines: forgotten,
            prompt,
        })
    }

    pub fn continuity_preserve_recent_anchor_literals(
        &self,
        conversation_id: i64,
    ) -> Result<Option<ContinuityDocumentState>> {
        let document = self.ensure_continuity_document(conversation_id, ContinuityKind::Anchors)?;
        let snapshot = self.working_set_snapshot(conversation_id, 512)?;
        let mut literals = collect_explicit_anchor_literals(&snapshot.messages);
        // Respect deliberate deletions: a literal whose anchor entry was
        // removed by a refresh sits in the forgotten ledger — re-adding it
        // here would resurrect what the refresh just deleted.
        let forgotten =
            self.continuity_forgotten_recent(conversation_id, Some(ContinuityKind::Anchors), 256)?;
        literals.retain(|literal| {
            !forgotten
                .iter()
                .any(|entry| entry.line.contains(&literal.literal))
        });
        let Some(diff_text) = build_anchor_literal_preservation_diff(&document.content, &literals)
        else {
            return Ok(None);
        };
        self.continuity_apply_diff(conversation_id, ContinuityKind::Anchors, &diff_text)
            .map(Some)
    }

    fn compact_leaf_pass<S: Summarizer>(
        &self,
        conversation_id: i64,
        summarizer: &S,
        force: bool,
        previous_tokens: i64,
    ) -> Result<Option<String>> {
        let entries = self.context_entries(conversation_id)?;
        let message_entries: Vec<_> = entries
            .iter()
            .filter(|entry| entry.item_type == ContextItemType::Message)
            .cloned()
            .collect();
        if message_entries.len() <= self.config.fresh_tail_count {
            return Ok(None);
        }

        let tail_start_ordinal = if self.config.fresh_tail_count == 0 {
            i64::MAX
        } else {
            message_entries[message_entries.len() - self.config.fresh_tail_count].ordinal
        };
        let mut selected = Vec::new();
        let mut selected_tokens = 0i64;
        let mut started = false;
        for entry in entries {
            if entry.ordinal >= tail_start_ordinal {
                break;
            }
            if !started {
                if entry.item_type != ContextItemType::Message || entry.message_id.is_none() {
                    continue;
                }
                started = true;
            } else if entry.item_type != ContextItemType::Message || entry.message_id.is_none() {
                break;
            }

            if selected_tokens > 0
                && selected_tokens + entry.token_count > self.config.leaf_chunk_tokens
            {
                break;
            }
            selected_tokens += entry.token_count;
            selected.push(entry.clone());
            if selected_tokens >= self.config.leaf_chunk_tokens {
                break;
            }
        }
        if selected.is_empty() {
            return Ok(None);
        }

        let first_ordinal = selected[0].ordinal;
        let source_text = self.leaf_source_text(&selected)?;
        let content = self
            .summarize_with_escalation(
                SummaryKind::Leaf,
                0,
                &source_text,
                self.config.leaf_target_tokens,
                summarizer,
            )?
            .content;
        let source_message_token_count = selected
            .iter()
            .filter_map(|entry| entry.message_id)
            .map(|message_id| {
                self.get_message(message_id)
                    .map(|message| message.token_count)
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .sum();
        let summary_id = self.insert_summary_token_gated(
            conversation_id,
            SummaryKind::Leaf,
            0,
            &content,
            0,
            0,
            source_message_token_count,
            &[],
            selected
                .iter()
                .filter_map(|entry| entry.message_id)
                .collect(),
            first_ordinal,
            selected.iter().map(|entry| entry.ordinal).collect(),
            previous_tokens,
            force,
        )?;
        Ok(summary_id)
    }

    fn compact_condensed_pass<S: Summarizer>(
        &self,
        conversation_id: i64,
        summarizer: &S,
        force: bool,
        previous_tokens: i64,
    ) -> Result<Option<String>> {
        let entries = self.context_entries(conversation_id)?;
        let message_entries: Vec<_> = entries
            .iter()
            .filter(|entry| entry.item_type == ContextItemType::Message)
            .cloned()
            .collect();
        let tail_start_ordinal = if self.config.fresh_tail_count == 0 {
            None
        } else if message_entries.len() > self.config.fresh_tail_count {
            Some(message_entries[message_entries.len() - self.config.fresh_tail_count].ordinal)
        } else {
            None
        };
        let eligible_entries: Vec<_> = entries
            .into_iter()
            .take_while(|entry| {
                tail_start_ordinal
                    .map(|ordinal| entry.ordinal < ordinal)
                    .unwrap_or(true)
            })
            .collect();
        let min_chunk_tokens = self.resolve_condensed_min_chunk_tokens();

        for depth in self.distinct_summary_depths(&eligible_entries)? {
            let same_depth = self.select_oldest_summary_chunk_at_depth(&eligible_entries, depth)?;
            if same_depth.len() < self.config.condensed_min_fanout {
                continue;
            }

            let token_count: i64 = same_depth.iter().map(|entry| entry.token_count).sum();
            if token_count < min_chunk_tokens {
                continue;
            }

            let first_ordinal = same_depth[0].ordinal;
            let child_ids: Vec<String> = same_depth
                .iter()
                .filter_map(|entry| entry.summary_id.clone())
                .collect();
            let source_text = self.condensed_source_text(&child_ids)?;
            let source_message_token_count = child_ids
                .iter()
                .map(|id| self.summary_source_message_token_count(id))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .sum();
            let descendant_count = child_ids
                .iter()
                .map(|id| Ok(self.summary_descendant_count(id)? + 1))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .sum();
            let descendant_tokens = child_ids
                .iter()
                .map(|id| {
                    Ok(self.summary_token_count(id)? + self.summary_descendant_token_count(id)?)
                })
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .sum();
            let content = self
                .summarize_with_escalation(
                    SummaryKind::Condensed,
                    depth + 1,
                    &source_text,
                    self.config.condensed_target_tokens,
                    summarizer,
                )?
                .content;
            let summary_id = self.insert_summary_token_gated(
                conversation_id,
                SummaryKind::Condensed,
                depth + 1,
                &content,
                descendant_count,
                descendant_tokens,
                source_message_token_count,
                &child_ids,
                Vec::new(),
                first_ordinal,
                same_depth.iter().map(|entry| entry.ordinal).collect(),
                previous_tokens,
                force,
            )?;
            return Ok(summary_id);
        }

        Ok(None)
    }

    fn distinct_summary_depths(&self, entries: &[ContextEntry]) -> Result<Vec<i64>> {
        let mut depths = Vec::new();
        for entry in entries {
            if entry.item_type != ContextItemType::Summary {
                continue;
            }
            let Some(summary_id) = entry.summary_id.as_deref() else {
                continue;
            };
            let Some(summary) = self.get_summary(summary_id)? else {
                continue;
            };
            if !depths.contains(&summary.depth) {
                depths.push(summary.depth);
            }
        }
        depths.sort_unstable();
        Ok(depths)
    }

    fn select_oldest_summary_chunk_at_depth(
        &self,
        entries: &[ContextEntry],
        target_depth: i64,
    ) -> Result<Vec<ContextEntry>> {
        let mut chunk = Vec::new();
        let mut token_count = 0i64;
        for entry in entries {
            if entry.item_type != ContextItemType::Summary {
                if !chunk.is_empty() {
                    break;
                }
                continue;
            }
            let Some(summary_id) = entry.summary_id.as_deref() else {
                if !chunk.is_empty() {
                    break;
                }
                continue;
            };
            let Some(summary) = self.get_summary(summary_id)? else {
                if !chunk.is_empty() {
                    break;
                }
                continue;
            };
            if summary.depth != target_depth {
                if !chunk.is_empty() {
                    break;
                }
                continue;
            }
            if token_count > 0 && token_count + summary.token_count > self.config.leaf_chunk_tokens
            {
                break;
            }
            token_count += summary.token_count;
            chunk.push(entry.clone());
            if token_count >= self.config.leaf_chunk_tokens {
                break;
            }
        }
        Ok(chunk)
    }

    // Superseded in production by insert_summary_token_gated; retained only for
    // tests that want an unconditional (non-token-gated) insert.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn insert_summary(
        &self,
        conversation_id: i64,
        kind: SummaryKind,
        depth: i64,
        content: &str,
        descendant_count: i64,
        descendant_token_count: i64,
        source_message_token_count: i64,
        child_summary_ids: &[String],
        message_ids: Vec<i64>,
        ordinal: i64,
        replaced_ordinals: Vec<i64>,
    ) -> Result<String> {
        let created_at = iso_now();
        let summary_id = summary_id_for(conversation_id, content, depth);
        let token_count = estimate_tokens(content) as i64;
        self.conn
            .execute_batch("SAVEPOINT insert_summary")
            .context("failed to begin savepoint for insert_summary")?;
        let result = self.insert_summary_inner(
            conversation_id,
            &summary_id,
            kind,
            depth,
            content,
            token_count,
            descendant_count,
            descendant_token_count,
            source_message_token_count,
            &created_at,
            child_summary_ids,
            message_ids,
            ordinal,
            replaced_ordinals,
        );
        match result {
            Ok(()) => {
                self.conn
                    .execute_batch("RELEASE insert_summary")
                    .context("failed to release savepoint for insert_summary")?;
                Ok(summary_id)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK TO insert_summary");
                let _ = self.conn.execute_batch("RELEASE insert_summary");
                Err(err)
            }
        }
    }

    /// Insert a compaction summary inside a savepoint and keep it only if the
    /// resulting context token count did not regress. The summarizer call must
    /// have already run before this point so no LLM work happens under the
    /// held write lock. On non-force passes the count must strictly decrease;
    /// on a force pass it must merely not increase. Otherwise the insert and
    /// the source-ordinal deletes are rolled back and `Ok(None)` is returned,
    /// so a regressing pass can never durably enlarge the context.
    #[allow(clippy::too_many_arguments)]
    fn insert_summary_token_gated(
        &self,
        conversation_id: i64,
        kind: SummaryKind,
        depth: i64,
        content: &str,
        descendant_count: i64,
        descendant_token_count: i64,
        source_message_token_count: i64,
        child_summary_ids: &[String],
        message_ids: Vec<i64>,
        ordinal: i64,
        replaced_ordinals: Vec<i64>,
        previous_tokens: i64,
        force: bool,
    ) -> Result<Option<String>> {
        let created_at = iso_now();
        let summary_id = summary_id_for(conversation_id, content, depth);
        let token_count = estimate_tokens(content) as i64;
        self.conn
            .execute_batch("SAVEPOINT insert_summary_gated")
            .context("failed to begin savepoint for insert_summary_token_gated")?;
        let result = self
            .insert_summary_inner(
                conversation_id,
                &summary_id,
                kind,
                depth,
                content,
                token_count,
                descendant_count,
                descendant_token_count,
                source_message_token_count,
                &created_at,
                child_summary_ids,
                message_ids,
                ordinal,
                replaced_ordinals,
            )
            .and_then(|()| self.context_token_count(conversation_id));
        match result {
            Ok(current) => {
                let regressed = if force {
                    current > previous_tokens
                } else {
                    current >= previous_tokens
                };
                if regressed {
                    let _ = self.conn.execute_batch("ROLLBACK TO insert_summary_gated");
                    self.conn
                        .execute_batch("RELEASE insert_summary_gated")
                        .context("failed to release savepoint for insert_summary_token_gated")?;
                    Ok(None)
                } else {
                    self.conn
                        .execute_batch("RELEASE insert_summary_gated")
                        .context("failed to release savepoint for insert_summary_token_gated")?;
                    Ok(Some(summary_id))
                }
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK TO insert_summary_gated");
                let _ = self.conn.execute_batch("RELEASE insert_summary_gated");
                Err(err)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_summary_inner(
        &self,
        conversation_id: i64,
        summary_id: &str,
        kind: SummaryKind,
        depth: i64,
        content: &str,
        token_count: i64,
        descendant_count: i64,
        descendant_token_count: i64,
        source_message_token_count: i64,
        created_at: &str,
        child_summary_ids: &[String],
        message_ids: Vec<i64>,
        ordinal: i64,
        replaced_ordinals: Vec<i64>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO summaries (
                summary_id, conversation_id, kind, depth, content, token_count,
                descendant_count, descendant_token_count, source_message_token_count, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                summary_id,
                conversation_id,
                kind.as_str(),
                depth,
                content,
                token_count,
                descendant_count,
                descendant_token_count,
                source_message_token_count,
                created_at
            ],
        )?;
        self.conn.execute(
            "DELETE FROM summaries_fts WHERE summary_id = ?1",
            params![summary_id],
        )?;
        self.conn.execute(
            "INSERT INTO summaries_fts (rowid, summary_id, content)
             VALUES ((SELECT rowid FROM summaries WHERE summary_id = ?1), ?1, ?2)",
            params![summary_id, normalize_for_fts(content)],
        )?;

        for child_id in child_summary_ids {
            self.conn.execute(
                "INSERT OR IGNORE INTO summary_edges (parent_summary_id, child_summary_id)
                 VALUES (?1, ?2)",
                params![summary_id, child_id],
            )?;
        }
        for message_id in message_ids {
            self.conn.execute(
                "INSERT OR IGNORE INTO summary_messages (summary_id, message_id)
                 VALUES (?1, ?2)",
                params![summary_id, message_id],
            )?;
        }

        for old_ordinal in replaced_ordinals {
            self.conn.execute(
                "DELETE FROM context_items WHERE conversation_id = ?1 AND ordinal = ?2",
                params![conversation_id, old_ordinal],
            )?;
        }
        self.conn.execute(
            "INSERT INTO context_items (conversation_id, ordinal, item_type, message_id, summary_id, created_at)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
            params![conversation_id, ordinal, ContextItemType::Summary.as_str(), summary_id, iso_now()],
        )?;
        Ok(())
    }

    fn resequence_context_items(&self, conversation_id: i64) -> Result<()> {
        let ordinals = {
            let mut stmt = self.conn.prepare(
                "SELECT ordinal FROM context_items WHERE conversation_id = ?1 ORDER BY ordinal ASC",
            )?;
            let rows = stmt.query_map([conversation_id], |row| row.get::<_, i64>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (new_ordinal, old_ordinal) in ordinals.into_iter().enumerate() {
            self.conn.execute(
                "UPDATE context_items SET ordinal = -?1 - 1 WHERE conversation_id = ?2 AND ordinal = ?3",
                params![new_ordinal as i64, conversation_id, old_ordinal],
            )?;
        }
        self.conn.execute(
            "UPDATE context_items SET ordinal = (-ordinal) - 1 WHERE conversation_id = ?1",
            [conversation_id],
        )?;
        Ok(())
    }

    fn context_entries(&self, conversation_id: i64) -> Result<Vec<ContextEntry>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                ci.ordinal,
                ci.item_type,
                ci.message_id,
                ci.summary_id,
                COALESCE(m.seq, ci.ordinal) AS seq,
                COALESCE(s.depth, 0) AS depth,
                COALESCE(m.token_count, s.token_count, 0) AS token_count
            FROM context_items ci
            LEFT JOIN messages m ON m.message_id = ci.message_id
            LEFT JOIN summaries s ON s.summary_id = ci.summary_id
            WHERE ci.conversation_id = ?1
            ORDER BY ci.ordinal ASC
            "#,
        )?;
        let rows = stmt.query_map([conversation_id], |row| {
            Ok(ContextEntry {
                ordinal: row.get(0)?,
                item_type: match row.get::<_, String>(1)?.as_str() {
                    "message" => ContextItemType::Message,
                    _ => ContextItemType::Summary,
                },
                message_id: row.get(2)?,
                summary_id: row.get(3)?,
                seq: row.get(4)?,
                depth: row.get(5)?,
                token_count: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn context_token_count(&self, conversation_id: i64) -> Result<i64> {
        self.conn
            .query_row(
                r#"
                SELECT COALESCE(SUM(COALESCE(m.token_count, s.token_count, 0)), 0)
                FROM context_items ci
                LEFT JOIN messages m ON m.message_id = ci.message_id
                LEFT JOIN summaries s ON s.summary_id = ci.summary_id
                WHERE ci.conversation_id = ?1
                "#,
                [conversation_id],
                |row| row.get(0),
            )
            .context("failed to compute context token count")
    }

    fn next_context_ordinal(&self, conversation_id: i64) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM context_items WHERE conversation_id = ?1",
                [conversation_id],
                |row| row.get(0),
            )
            .context("failed to compute next context ordinal")
    }

    fn leaf_source_text(&self, entries: &[ContextEntry]) -> Result<String> {
        let mut chunks = Vec::new();
        for entry in entries {
            let Some(message_id) = entry.message_id else {
                continue;
            };
            let message = self.get_message(message_id)?;
            chunks.push(format!(
                "[{}]\n{}",
                format_summary_timestamp(&message.created_at),
                message.content
            ));
        }
        Ok(chunks.join("\n\n"))
    }

    fn condensed_source_text(&self, summary_ids: &[String]) -> Result<String> {
        let mut chunks = Vec::new();
        for summary_id in summary_ids {
            let Some(summary) = self.get_summary(summary_id)? else {
                continue;
            };
            let timestamp = format_summary_timestamp(&summary.created_at);
            chunks.push(format!("[{timestamp} - {timestamp}]\n{}", summary.content));
        }
        Ok(chunks.join("\n\n"))
    }

    fn summarize_with_escalation<S: Summarizer>(
        &self,
        kind: SummaryKind,
        depth: i64,
        source_text: &str,
        target_tokens: usize,
        summarizer: &S,
    ) -> Result<EscalatedSummary> {
        let trimmed = source_text.trim();
        if trimmed.is_empty() {
            return Ok(EscalatedSummary {
                content: "[Truncated from 0 tokens]".to_string(),
            });
        }

        let input_tokens = estimate_tokens(trimmed) as i64;
        let lines: Vec<String> = trimmed.lines().map(str::to_string).collect();
        let summary = summarizer.summarize(kind, depth, &lines, target_tokens)?;
        let summary_tokens = estimate_tokens(&summary) as i64;
        let content = if summary.trim().is_empty()
            || summary_tokens >= input_tokens
            || (input_tokens > 0
                && (summary_tokens as f64 / input_tokens as f64) > MAX_SUMMARY_RATIO)
        {
            build_deterministic_fallback(trimmed, input_tokens)
        } else {
            summary.trim().to_string()
        };
        Ok(EscalatedSummary { content })
    }

    fn get_message(&self, message_id: i64) -> Result<MessageRecord> {
        self.conn
            .query_row(
                "SELECT message_id, conversation_id, seq, role, content, token_count, created_at, agent_outcome
             FROM messages WHERE message_id = ?1",
                [message_id],
                |row| {
                    Ok(MessageRecord {
                        message_id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        seq: row.get(2)?,
                        role: row.get(3)?,
                        content: row.get(4)?,
                        token_count: row.get(5)?,
                        created_at: row.get(6)?,
                        agent_outcome: row.get(7)?,
                    })
                },
            )
            .context("message not found")
    }

    pub(crate) fn messages_for_conversation(
        &self,
        conversation_id: i64,
    ) -> Result<Vec<MessageRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT message_id, conversation_id, seq, role, content, token_count, created_at, agent_outcome
             FROM messages WHERE conversation_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map([conversation_id], |row| {
            Ok(MessageRecord {
                message_id: row.get(0)?,
                conversation_id: row.get(1)?,
                seq: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                token_count: row.get(5)?,
                created_at: row.get(6)?,
                agent_outcome: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Cheap change-detection marker for UI polling over the most recent
    /// `limit` messages: (max seq, row count, agent-outcome count, summed
    /// token count). UI consumers compare markers between ticks and skip
    /// materializing the full message window when nothing changed. The
    /// token-count sum makes in-place rewrites visible
    /// (`rewrite_message_rows_with` updates content and token_count without
    /// touching seq or row count); the outcome count covers any path that
    /// back-fills `agent_outcome` on an existing row.
    pub fn conversation_refresh_marker(
        &self,
        conversation_id: i64,
        limit: usize,
    ) -> Result<(i64, i64, i64, i64)> {
        let limit = limit.max(1).min(500) as i64;
        self.conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0), COUNT(*),
                        COALESCE(SUM(CASE WHEN agent_outcome IS NOT NULL THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(token_count), 0)
                 FROM (SELECT seq, agent_outcome, token_count FROM messages
                       WHERE conversation_id = ?1 ORDER BY seq DESC LIMIT ?2)",
                (conversation_id, limit),
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(Into::into)
    }

    pub fn recent_messages_for_conversation(
        &self,
        conversation_id: i64,
        limit: usize,
    ) -> Result<Vec<MessageRecord>> {
        let limit = limit.max(1).min(500) as i64;
        let mut stmt = self.conn.prepare(
            "SELECT message_id, conversation_id, seq, role, content, token_count, created_at, agent_outcome
             FROM messages WHERE conversation_id = ?1 ORDER BY seq DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map((conversation_id, limit), |row| {
            Ok(MessageRecord {
                message_id: row.get(0)?,
                conversation_id: row.get(1)?,
                seq: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                token_count: row.get(5)?,
                created_at: row.get(6)?,
                agent_outcome: row.get(7)?,
            })
        })?;
        let mut messages = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        messages.reverse();
        Ok(messages)
    }

    fn get_summary(&self, summary_id: &str) -> Result<Option<SummaryRecord>> {
        self.conn
            .query_row(
                "SELECT summary_id, conversation_id, kind, depth, content, token_count,
                    descendant_count, descendant_token_count, source_message_token_count, created_at
             FROM summaries WHERE summary_id = ?1",
                [summary_id],
                |row| {
                    Ok(SummaryRecord {
                        summary_id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        kind: parse_summary_kind(&row.get::<_, String>(2)?),
                        depth: row.get(3)?,
                        content: row.get(4)?,
                        token_count: row.get(5)?,
                        descendant_count: row.get(6)?,
                        descendant_token_count: row.get(7)?,
                        source_message_token_count: row.get(8)?,
                        created_at: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    fn summaries_for_conversation(&self, conversation_id: i64) -> Result<Vec<SummaryRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT summary_id, conversation_id, kind, depth, content, token_count,
                    descendant_count, descendant_token_count, source_message_token_count, created_at
             FROM summaries WHERE conversation_id = ?1 ORDER BY depth ASC, created_at ASC",
        )?;
        let rows = stmt.query_map([conversation_id], |row| {
            Ok(SummaryRecord {
                summary_id: row.get(0)?,
                conversation_id: row.get(1)?,
                kind: parse_summary_kind(&row.get::<_, String>(2)?),
                depth: row.get(3)?,
                content: row.get(4)?,
                token_count: row.get(5)?,
                descendant_count: row.get(6)?,
                descendant_token_count: row.get(7)?,
                source_message_token_count: row.get(8)?,
                created_at: row.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn summary_edges_for_conversation(
        &self,
        conversation_id: i64,
    ) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT e.parent_summary_id, e.child_summary_id
            FROM summary_edges e
            JOIN summaries parent ON parent.summary_id = e.parent_summary_id
            JOIN summaries child ON child.summary_id = e.child_summary_id
            WHERE parent.conversation_id = ?1 AND child.conversation_id = ?1
            ORDER BY e.parent_summary_id ASC, e.child_summary_id ASC
            "#,
        )?;
        let rows = stmt.query_map([conversation_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn summary_message_links_for_conversation(
        &self,
        conversation_id: i64,
    ) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT sm.summary_id, sm.message_id
            FROM summary_messages sm
            JOIN summaries s ON s.summary_id = sm.summary_id
            WHERE s.conversation_id = ?1
            ORDER BY sm.summary_id ASC, sm.message_id ASC
            "#,
        )?;
        let rows = stmt.query_map([conversation_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn summary_parent_ids(&self, summary_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT parent_summary_id FROM summary_edges WHERE child_summary_id = ?1 ORDER BY parent_summary_id",
        )?;
        let rows = stmt.query_map([summary_id], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn summary_child_ids(&self, summary_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT child_summary_id FROM summary_edges WHERE parent_summary_id = ?1 ORDER BY child_summary_id",
        )?;
        let rows = stmt.query_map([summary_id], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn summary_message_ids(&self, summary_id: &str) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT message_id FROM summary_messages WHERE summary_id = ?1 ORDER BY message_id",
        )?;
        let rows = stmt.query_map([summary_id], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn child_summaries(&self, summary_id: &str) -> Result<Vec<SummaryRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT s.summary_id, s.conversation_id, s.kind, s.depth, s.content, s.token_count,
                   s.descendant_count, s.descendant_token_count, s.source_message_token_count, s.created_at
            FROM summary_edges e
            JOIN summaries s ON s.summary_id = e.child_summary_id
            WHERE e.parent_summary_id = ?1
            ORDER BY s.depth ASC, s.created_at ASC
            "#,
        )?;
        let rows = stmt.query_map([summary_id], |row| {
            Ok(SummaryRecord {
                summary_id: row.get(0)?,
                conversation_id: row.get(1)?,
                kind: parse_summary_kind(&row.get::<_, String>(2)?),
                depth: row.get(3)?,
                content: row.get(4)?,
                token_count: row.get(5)?,
                descendant_count: row.get(6)?,
                descendant_token_count: row.get(7)?,
                source_message_token_count: row.get(8)?,
                created_at: row.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn messages_for_summary(&self, summary_id: &str) -> Result<Vec<MessageRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT m.message_id, m.conversation_id, m.seq, m.role, m.content, m.token_count, m.created_at, m.agent_outcome
            FROM summary_messages sm
            JOIN messages m ON m.message_id = sm.message_id
            WHERE sm.summary_id = ?1
            ORDER BY m.seq ASC
            "#,
        )?;
        let rows = stmt.query_map([summary_id], |row| {
            Ok(MessageRecord {
                message_id: row.get(0)?,
                conversation_id: row.get(1)?,
                seq: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                token_count: row.get(5)?,
                created_at: row.get(6)?,
                agent_outcome: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn summary_descendant_count(&self, summary_id: &str) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT descendant_count FROM summaries WHERE summary_id = ?1",
                [summary_id],
                |row| row.get(0),
            )
            .unwrap_or(0))
    }

    fn summary_token_count(&self, summary_id: &str) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT token_count FROM summaries WHERE summary_id = ?1",
                [summary_id],
                |row| row.get(0),
            )
            .unwrap_or(0))
    }

    fn summary_descendant_token_count(&self, summary_id: &str) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT descendant_token_count FROM summaries WHERE summary_id = ?1",
                [summary_id],
                |row| row.get(0),
            )
            .unwrap_or(0))
    }

    fn resolve_condensed_min_chunk_tokens(&self) -> i64 {
        let ratio_floor =
            ((self.config.leaf_chunk_tokens as f64) * CONDENSED_MIN_INPUT_RATIO).floor() as i64;
        std::cmp::max(self.config.condensed_target_tokens as i64, ratio_floor)
    }

    fn ensure_continuity_document(
        &self,
        conversation_id: i64,
        kind: ContinuityKind,
    ) -> Result<ContinuityDocumentState> {
        ensure_continuity_document_with(&self.conn, conversation_id, kind)
    }

    fn continuity_commits_for_document(
        &self,
        _head_commit_id: &str,
        conversation_id: i64,
        kind: ContinuityKind,
    ) -> Result<Vec<ContinuityCommitRecord>> {
        let document_id = continuity_document_id(conversation_id, kind);
        self.continuity_commits_for_document_id(&document_id, conversation_id, kind)
    }

    fn continuity_commits_for_document_id(
        &self,
        document_id: &str,
        conversation_id: i64,
        kind: ContinuityKind,
    ) -> Result<Vec<ContinuityCommitRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT commit_id, parent_commit_id, diff_text, rendered_text, created_at
             FROM continuity_commits
             WHERE document_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([document_id], |row| {
            Ok(ContinuityCommitRecord {
                commit_id: row.get(0)?,
                conversation_id,
                kind,
                parent_commit_id: row.get(1)?,
                diff_text: row.get(2)?,
                rendered_text: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn summary_source_message_token_count(&self, summary_id: &str) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT source_message_token_count FROM summaries WHERE summary_id = ?1",
                [summary_id],
                |row| row.get(0),
            )
            .unwrap_or(0))
    }

    fn summary_subtree(&self, summary_id: &str) -> Result<Vec<SummarySubtreeNode>> {
        let mut stmt = self.conn.prepare(
            r#"
            WITH RECURSIVE subtree(summary_id, parent_summary_id, depth_from_root, path) AS (
                SELECT s.summary_id, NULL, 0, s.summary_id
                FROM summaries s
                WHERE s.summary_id = ?1
                UNION ALL
                SELECT child.summary_id, edge.parent_summary_id, subtree.depth_from_root + 1,
                       subtree.path || '>' || child.summary_id
                FROM subtree
                JOIN summary_edges edge ON edge.parent_summary_id = subtree.summary_id
                JOIN summaries child ON child.summary_id = edge.child_summary_id
            )
            SELECT
                subtree.summary_id,
                subtree.parent_summary_id,
                subtree.depth_from_root,
                s.kind,
                s.depth,
                s.token_count,
                s.descendant_count,
                s.descendant_token_count,
                s.source_message_token_count,
                (
                    SELECT COUNT(*)
                    FROM summary_edges edge2
                    WHERE edge2.parent_summary_id = subtree.summary_id
                ) AS child_count,
                subtree.path,
                s.created_at
            FROM subtree
            JOIN summaries s ON s.summary_id = subtree.summary_id
            ORDER BY subtree.depth_from_root ASC, subtree.path ASC
            "#,
        )?;
        let rows = stmt.query_map([summary_id], |row| {
            Ok(SummarySubtreeNode {
                summary_id: row.get(0)?,
                parent_summary_id: row.get(1)?,
                depth_from_root: row.get(2)?,
                kind: parse_summary_kind(&row.get::<_, String>(3)?),
                depth: row.get(4)?,
                token_count: row.get(5)?,
                descendant_count: row.get(6)?,
                descendant_token_count: row.get(7)?,
                source_message_token_count: row.get(8)?,
                child_count: row.get(9)?,
                path: row.get(10)?,
                created_at: row.get(11)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn search_messages(
        &self,
        conversation_id: Option<i64>,
        mode: GrepMode,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MessageSearchResult>> {
        match mode {
            GrepMode::FullText => self.search_messages_fts(conversation_id, query, limit),
            GrepMode::Regex => self.search_messages_regex(conversation_id, query, limit),
        }
    }

    fn search_summaries(
        &self,
        conversation_id: Option<i64>,
        mode: GrepMode,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SummarySearchResult>> {
        match mode {
            GrepMode::FullText => self.search_summaries_fts(conversation_id, query, limit),
            GrepMode::Regex => self.search_summaries_regex(conversation_id, query, limit),
        }
    }

    fn search_messages_fts(
        &self,
        conversation_id: Option<i64>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MessageSearchResult>> {
        let sql = if conversation_id.is_some() {
            r#"
            SELECT m.message_id, m.conversation_id, m.role, m.content, m.created_at
            FROM messages_fts f
            JOIN messages m ON m.rowid = f.rowid
            WHERE messages_fts MATCH ?1 AND m.conversation_id = ?2
            ORDER BY m.created_at DESC
            LIMIT ?3
            "#
        } else {
            r#"
            SELECT m.message_id, m.conversation_id, m.role, m.content, m.created_at
            FROM messages_fts f
            JOIN messages m ON m.rowid = f.rowid
            WHERE messages_fts MATCH ?1
            ORDER BY m.created_at DESC
            LIMIT ?2
            "#
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = if let Some(conversation_id) = conversation_id {
            stmt.query_map(
                params![sanitize_fts_query(query), conversation_id, limit as i64],
                |row| {
                    Ok(MessageSearchResult {
                        message_id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        role: row.get(2)?,
                        snippet: snippet(&row.get::<_, String>(3)?, query),
                        created_at: row.get(4)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map(params![sanitize_fts_query(query), limit as i64], |row| {
                Ok(MessageSearchResult {
                    message_id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    role: row.get(2)?,
                    snippet: snippet(&row.get::<_, String>(3)?, query),
                    created_at: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(rows)
    }

    fn search_messages_regex(
        &self,
        conversation_id: Option<i64>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MessageSearchResult>> {
        let regex = Regex::new(query).with_context(|| format!("invalid regex: {query}"))?;
        let mut stmt = if conversation_id.is_some() {
            self.conn.prepare(
                "SELECT message_id, conversation_id, role, content, created_at
                 FROM messages WHERE conversation_id = ?1 ORDER BY created_at DESC",
            )?
        } else {
            self.conn.prepare(
                "SELECT message_id, conversation_id, role, content, created_at
                 FROM messages ORDER BY created_at DESC",
            )?
        };
        let mut out = Vec::new();
        if let Some(conversation_id) = conversation_id {
            let rows = stmt.query_map([conversation_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            for row in rows {
                let (message_id, conversation_id, role, content, created_at) = row?;
                if regex.is_match(&content) {
                    out.push(MessageSearchResult {
                        message_id,
                        conversation_id,
                        role,
                        snippet: snippet(&content, query),
                        created_at,
                    });
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        } else {
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            for row in rows {
                let (message_id, conversation_id, role, content, created_at) = row?;
                if regex.is_match(&content) {
                    out.push(MessageSearchResult {
                        message_id,
                        conversation_id,
                        role,
                        snippet: snippet(&content, query),
                        created_at,
                    });
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        }
        Ok(out)
    }

    fn search_summaries_fts(
        &self,
        conversation_id: Option<i64>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SummarySearchResult>> {
        let sql = if conversation_id.is_some() {
            r#"
            SELECT s.summary_id, s.conversation_id, s.kind, s.content, s.created_at
            FROM summaries_fts f
            JOIN summaries s ON s.rowid = f.rowid
            WHERE summaries_fts MATCH ?1 AND s.conversation_id = ?2
            ORDER BY s.created_at DESC
            LIMIT ?3
            "#
        } else {
            r#"
            SELECT s.summary_id, s.conversation_id, s.kind, s.content, s.created_at
            FROM summaries_fts f
            JOIN summaries s ON s.rowid = f.rowid
            WHERE summaries_fts MATCH ?1
            ORDER BY s.created_at DESC
            LIMIT ?2
            "#
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = if let Some(conversation_id) = conversation_id {
            stmt.query_map(
                params![sanitize_fts_query(query), conversation_id, limit as i64],
                |row| {
                    Ok(SummarySearchResult {
                        summary_id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        kind: parse_summary_kind(&row.get::<_, String>(2)?),
                        snippet: snippet(&row.get::<_, String>(3)?, query),
                        created_at: row.get(4)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map(params![sanitize_fts_query(query), limit as i64], |row| {
                Ok(SummarySearchResult {
                    summary_id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    kind: parse_summary_kind(&row.get::<_, String>(2)?),
                    snippet: snippet(&row.get::<_, String>(3)?, query),
                    created_at: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(rows)
    }

    fn search_summaries_regex(
        &self,
        conversation_id: Option<i64>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SummarySearchResult>> {
        let regex = Regex::new(query).with_context(|| format!("invalid regex: {query}"))?;
        let mut stmt = if conversation_id.is_some() {
            self.conn.prepare(
                "SELECT summary_id, conversation_id, kind, content, created_at
                 FROM summaries WHERE conversation_id = ?1 ORDER BY created_at DESC",
            )?
        } else {
            self.conn.prepare(
                "SELECT summary_id, conversation_id, kind, content, created_at
                 FROM summaries ORDER BY created_at DESC",
            )?
        };
        let mut out = Vec::new();
        if let Some(conversation_id) = conversation_id {
            let rows = stmt.query_map([conversation_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            for row in rows {
                let (summary_id, conversation_id, kind, content, created_at) = row?;
                if regex.is_match(&content) {
                    out.push(SummarySearchResult {
                        summary_id,
                        conversation_id,
                        kind: parse_summary_kind(&kind),
                        snippet: snippet(&content, query),
                        created_at,
                    });
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        } else {
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            for row in rows {
                let (summary_id, conversation_id, kind, content, created_at) = row?;
                if regex.is_match(&content) {
                    out.push(SummarySearchResult {
                        summary_id,
                        conversation_id,
                        kind: parse_summary_kind(&kind),
                        snippet: snippet(&content, query),
                        created_at,
                    });
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
fn verification_run_id(
    conversation_id: i64,
    source_label: &str,
    goal: &str,
    preview: &str,
    result_excerpt: &str,
    created_at: &str,
) -> String {
    let mut hash = Sha256::new();
    hash.update(conversation_id.to_string().as_bytes());
    hash.update(source_label.as_bytes());
    hash.update(goal.as_bytes());
    hash.update(preview.as_bytes());
    hash.update(result_excerpt.as_bytes());
    hash.update(created_at.as_bytes());
    let digest = hash.finalize();
    let prefix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("vrun_{prefix}")
}

#[cfg(test)]
fn mission_claim_key(conversation_id: i64, claim_kind: &str, subject: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(conversation_id.to_string().as_bytes());
    hash.update(claim_kind.as_bytes());
    hash.update(normalize_mission_text(subject).as_bytes());
    let digest = hash.finalize();
    let prefix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("claim_{prefix}")
}

fn continuity_base_commit_id(conversation_id: i64, kind: ContinuityKind) -> String {
    format!("contbase_{}_{}", conversation_id, kind.as_str())
}

fn continuity_commit_id(
    conversation_id: i64,
    kind: ContinuityKind,
    diff_text: &str,
    rendered_text: &str,
    created_at: &str,
) -> String {
    let mut hash = Sha256::new();
    hash.update(conversation_id.to_string().as_bytes());
    hash.update(kind.as_str().as_bytes());
    hash.update(diff_text.as_bytes());
    hash.update(rendered_text.as_bytes());
    hash.update(created_at.as_bytes());
    let digest = hash.finalize();
    let prefix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("contc_{prefix}")
}

fn continuity_heads_revision_id(
    conversation_id: i64,
    narrative_head: &str,
    anchors_head: &str,
    focus_head: &str,
) -> String {
    let mut hash = Sha256::new();
    hash.update(conversation_id.to_string().as_bytes());
    hash.update(narrative_head.as_bytes());
    hash.update(anchors_head.as_bytes());
    hash.update(focus_head.as_bytes());
    let digest = hash.finalize();
    let prefix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("contrev_{prefix}")
}

fn normalize_continuity_diff(kind: ContinuityKind, diff_text: &str) -> Result<String> {
    let mut normalized = Vec::new();
    let mut current_section: Option<String> = None;
    let mut is_explicit_section = false;
    for raw_line in diff_text.lines() {
        let line = raw_line.trim_end();
        let syntax_line = line.trim_start();
        if syntax_line.trim().is_empty() {
            continue;
        }
        if syntax_line.starts_with("## ") {
            current_section = Some(syntax_line.trim().to_string());
            is_explicit_section = true;
            normalized.push(syntax_line.trim().to_string());
            continue;
        }
        if !is_explicit_section {
            if let Some(stripped) = syntax_line
                .strip_prefix('+')
                .or_else(|| syntax_line.strip_prefix('-'))
                .map(str::trim)
            {
                if let Some(section) = infer_continuity_section(kind, stripped) {
                    if current_section.as_deref() != Some(section) {
                        current_section = Some(section.to_string());
                        normalized.push(section.to_string());
                    }
                }
            }
        }
        normalized.push(syntax_line.to_string());
    }
    Ok(normalized.join("\n"))
}

fn infer_continuity_section(kind: ContinuityKind, diff_line: &str) -> Option<&'static str> {
    let trimmed = diff_line.trim();
    let normalized = collapse_whitespace(diff_line).to_ascii_lowercase();
    match kind {
        ContinuityKind::Anchors => Some("## Entries"),
        ContinuityKind::Narrative => {
            if normalized.starts_with("summary:") || normalized.starts_with("state:") {
                Some("## Situation")
            } else {
                Some("## Entries")
            }
        }
        ContinuityKind::Focus => {
            if trimmed.starts_with("Mission:")
                || trimmed.starts_with("Mission state:")
                || trimmed.starts_with("Continuation mode:")
                || trimmed.starts_with("Trigger intensity:")
            {
                Some("## Status")
            } else if trimmed.starts_with("Current blocker:") {
                Some("## Blocker")
            } else if trimmed.starts_with("Next slice:") {
                Some("## Next")
            } else if trimmed.starts_with("Done gate:")
                || trimmed.starts_with("Retry condition:")
                || trimmed.starts_with("Closure confidence:")
            {
                Some("## Done / Gate")
            } else if trimmed.starts_with("mission:")
                || normalized.starts_with("mission_state:")
                || normalized.starts_with("continuation_mode:")
                || normalized.starts_with("trigger_intensity:")
                || normalized.starts_with("slice:")
                || normalized.starts_with("slice_state:")
            {
                Some("## Contract")
            } else if normalized.starts_with("goal:")
                || normalized.starts_with("blocker:")
                || normalized.starts_with("missing_dependency:")
                || normalized.starts_with("next_slice:")
                || normalized.starts_with("done_gate:")
                || normalized.starts_with("retry_condition:")
                || normalized.starts_with("closure_confidence:")
            {
                Some("## State")
            } else if normalized.starts_with("source_refs:")
                || normalized == "none"
                || normalized.starts_with("updated_at:")
            {
                Some("## Sources")
            } else {
                None
            }
        }
    }
}

fn apply_continuity_diff(kind: ContinuityKind, base: &str, diff_text: &str) -> Result<String> {
    let mut sections = parse_continuity_sections(base)?;
    let mut current_section: Option<String> = None;
    for raw_line in diff_text.lines() {
        let line = raw_line.trim_end();
        let syntax_line = line.trim_start();
        if syntax_line.trim().is_empty() {
            continue;
        }
        if syntax_line.starts_with("## ") {
            let section = syntax_line.trim().to_string();
            if !sections.contains_key(&section) {
                anyhow::bail!("unknown continuity section in diff: {section}");
            }
            current_section = Some(section);
            continue;
        }
        if current_section.is_none() {
            if let Some(stripped) = syntax_line
                .strip_prefix('+')
                .or_else(|| syntax_line.strip_prefix('-'))
                .map(str::trim)
            {
                current_section = infer_continuity_section(kind, stripped).map(str::to_string);
            }
        }
        let section = current_section
            .as_ref()
            .context("continuity diff requires a section header before +/- lines")?;
        let entry = sections
            .get_mut(section)
            .context("diff section missing in document")?;
        if let Some(added) = syntax_line.strip_prefix('+') {
            let value = collapse_whitespace(added);
            if !value.is_empty() && !entry.contains(&value) {
                entry.push(value);
            }
        } else if let Some(removed) = syntax_line.strip_prefix('-') {
            let value = collapse_whitespace(removed);
            entry.retain(|existing| existing != &value);
        } else {
            anyhow::bail!("unsupported continuity diff line: {syntax_line}");
        }
    }
    render_continuity_sections(base, &sections)
}

fn parse_continuity_sections(
    base: &str,
) -> Result<std::collections::BTreeMap<String, Vec<String>>> {
    let mut sections = std::collections::BTreeMap::new();
    let mut current_section: Option<String> = None;
    for raw_line in base.lines() {
        let line = raw_line.trim_end();
        if line.starts_with("## ") {
            current_section = Some(line.to_string());
            sections.entry(line.to_string()).or_insert_with(Vec::new);
            continue;
        }
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Some(section) = current_section.as_ref() {
            sections
                .entry(section.clone())
                .or_insert_with(Vec::new)
                .push(collapse_whitespace(line.trim_start_matches("- ").trim()));
        }
    }
    Ok(sections)
}

fn render_continuity_sections(
    template: &str,
    sections: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<String> {
    let mut out = Vec::new();
    for raw_line in template.lines() {
        let line = raw_line.trim_end();
        if line.starts_with("## ") {
            out.push(line.to_string());
            if let Some(items) = sections.get(line) {
                if !items.is_empty() {
                    for item in items {
                        out.push(format!("- {item}"));
                    }
                }
            }
            out.push(String::new());
        } else if line.starts_with("# ") {
            out.push(line.to_string());
            out.push(String::new());
        }
    }
    Ok(out.join("\n").trim_end().to_string() + "\n")
}

fn removed_lines_from_diff(diff_text: &str) -> Vec<String> {
    diff_text
        .lines()
        .filter_map(|line| line.strip_prefix('-'))
        .map(collapse_whitespace)
        .filter(|line| !line.is_empty())
        .collect()
}

fn build_continuity_prompt_text(
    conversation_id: i64,
    kind: ContinuityKind,
    current_document: &str,
    recent_messages: &[String],
    recent_summaries: &[String],
    forgotten_lines: &[String],
    explicit_anchor_literals: &[ExplicitAnchorLiteral],
) -> String {
    let kind_label = match kind {
        ContinuityKind::Narrative => "CONTINUITY NARRATIVE",
        ContinuityKind::Anchors => "CONTINUITY ANCHORS",
        ContinuityKind::Focus => "ACTIVE FOCUS",
    };
    let kind_expectations = match kind {
        ContinuityKind::Narrative => {
            "Keep short narrative entries that say what happened, why it matters, and where it came from."
        }
        ContinuityKind::Anchors => {
            "Keep short anchor entries for facts, constraints, do-not-do rules, invariants, and retry boundaries."
        }
        ContinuityKind::Focus => {
            "Keep one short focus record that says what the mission is, whether it is still open, what is blocked, what to do next, when it is really finished, and when a retry would make sense. If recent messages show live runtime work or a reopened mission, replace stale closed values instead of keeping them."
        }
    };
    let kind_str = kind.as_str();
    let mut prompt = vec![
        format!(
        "You are updating durable memory for CTOX conversation {}.",
        conversation_id
    ),
        format!("Memory document: {kind_str}."),
        kind_expectations.to_string(),
        String::new(),
        "IMPORTANT: Your reply text does not update memory. You must call `ctox continuity-update` with a shell command. If no update is needed, make no CLI call and reply exactly `noop`.".to_string(),
        String::new(),
        "Three modes are available. Pick the smallest one that fits your change.".to_string(),
        String::new(),
        "MODE A — full replacement (write the new document body to stdin):".to_string(),
        format!(
            "    printf '%s' \"<FULL NEW DOCUMENT BODY>\" | ctox continuity-update --kind {kind_str} --mode full --conversation-id {conversation_id}"
        ),
        "  Use this when the current document is empty or its structure has to change substantially. \
         Keep section headers the same (`## Status`, `## Blocker`, ...). Write each field on its own line as `- field: value` or `field: value`.".to_string(),
        String::new(),
        "MODE B — single targeted string replacement (best for one-field updates):".to_string(),
        format!(
            "    ctox continuity-update --kind {kind_str} --mode replace --find '<OLD EXACT TEXT>' --replace '<NEW EXACT TEXT>' --conversation-id {conversation_id}"
        ),
        "  `--find` must match exactly once in the document. A match of zero or >1 fails loudly. \
         Best for edits like changing `Mission state: open` to `Mission state: done`.".to_string(),
        String::new(),
        "MODE C — structured +/- diff (advanced; read from stdin):".to_string(),
        format!(
            "    printf '## Section\\n- old line\\n+ new line\\n' | ctox continuity-update --kind {kind_str} --mode diff --conversation-id {conversation_id}"
        ),
        "  Use only when you have several coordinated changes across the same document.".to_string(),
        String::new(),
        "Always pass --conversation-id exactly as shown; omitting it writes to the wrong conversation.".to_string(),
        String::new(),
        "CONTENT RULES".to_string(),
        "- Keep the existing `##` section names. Do not invent new headings.".to_string(),
        "- Do not invent facts not supported by recent messages or summaries.".to_string(),
        "- If recent work failed or repeated, keep the failed tactic / blocker / retry condition.".to_string(),
    ];
    if kind == ContinuityKind::Anchors {
        prompt.push(
            "- Keep explicit anchor literals exactly as written (identifiers like `ANCHOR_*` or `BENCH_*`).".to_string(),
        );
    } else if kind == ContinuityKind::Focus {
        prompt.push(
            "- If recent messages show live runtime work or a reopened mission, keep `mission_state: active` / `continuation_mode: continuous`.".to_string(),
        );
        prompt.push(
            "- Do not keep stale closed fields (`Mission state: done`, `Continuation mode: closed`) when the mission is still open.".to_string(),
        );
    }
    prompt.push(String::new());
    prompt.push("EXIT GATE: memory is updated only after the CLI command succeeds.".to_string());
    prompt.push(String::new());
    prompt.push(format!("<DOCUMENT_KIND>\n{}\n</DOCUMENT_KIND>", kind_label));
    prompt.push(String::new());
    prompt.push(format!(
        "<CURRENT_DOCUMENT>\n{}\n</CURRENT_DOCUMENT>",
        current_document.trim_end()
    ));
    prompt.push(String::new());
    prompt.push(format!(
        "<RECENT_MESSAGES>\n{}\n</RECENT_MESSAGES>",
        if recent_messages.is_empty() {
            "(none)".to_string()
        } else {
            recent_messages.join("\n")
        }
    ));
    prompt.push(String::new());
    prompt.push(format!(
        "<RECENT_SUMMARIES>\n{}\n</RECENT_SUMMARIES>",
        if recent_summaries.is_empty() {
            "(none)".to_string()
        } else {
            recent_summaries.join("\n")
        }
    ));
    if kind == ContinuityKind::Anchors {
        prompt.push(String::new());
        prompt.push(format!(
            "<EXPLICIT_ANCHOR_LITERALS>\n{}\n</EXPLICIT_ANCHOR_LITERALS>",
            if explicit_anchor_literals.is_empty() {
                "(none)".to_string()
            } else {
                explicit_anchor_literals
                    .iter()
                    .map(|literal| {
                        format!(
                            "{} (source: {}, observed_at: {})",
                            literal.literal, literal.source_ref, literal.observed_at
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        ));
    }
    prompt.push(String::new());
    prompt.push(format!(
        "<PREVIOUSLY_FORGOTTEN_LINES>\n{}\n</PREVIOUSLY_FORGOTTEN_LINES>",
        if forgotten_lines.is_empty() {
            "(none)".to_string()
        } else {
            forgotten_lines.join("\n")
        }
    ));
    prompt.push(String::new());
    prompt.push(
        "Reminder: call `ctox continuity-update` to save changes. Replying with a diff or summary does not save anything."
            .to_string(),
    );
    prompt.join("\n")
}

fn collect_explicit_anchor_literals(messages: &[MessageRecord]) -> Vec<ExplicitAnchorLiteral> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for message in messages.iter().rev().take(8) {
        for literal in extract_explicit_anchor_literals(&message.content) {
            if seen.insert(literal.clone()) {
                out.push(ExplicitAnchorLiteral {
                    literal,
                    source_ref: format!("{}#{}", message.role, message.seq),
                    observed_at: continuity_observed_at(&message.created_at),
                });
            }
        }
    }
    out
}

fn extract_explicit_anchor_literals(content: &str) -> Vec<String> {
    let code_span_pattern = Regex::new(r"`([^`\n]{1,128})`").expect("valid code span regex");
    let explicit_literal_pattern =
        Regex::new(r"\b(?:ANCHOR|BENCH)_[A-Z0-9_]{2,}\b").expect("valid literal regex");
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for captures in code_span_pattern.captures_iter(content) {
        let Some(matched) = captures.get(1) else {
            continue;
        };
        let literal = matched.as_str().trim();
        if looks_like_explicit_anchor_literal(literal) && seen.insert(literal.to_string()) {
            out.push(literal.to_string());
        }
    }
    for matched in explicit_literal_pattern.find_iter(content) {
        let literal = matched.as_str();
        if looks_like_explicit_anchor_literal(literal) && seen.insert(literal.to_string()) {
            out.push(literal.to_string());
        }
    }
    out
}

fn looks_like_explicit_anchor_literal(value: &str) -> bool {
    let literal = value.trim();
    if literal.is_empty() || literal.chars().any(char::is_whitespace) {
        return false;
    }
    if literal.starts_with("ANCHOR_") || literal.starts_with("BENCH_") {
        return true;
    }
    literal.len() >= 8
        && literal.contains('_')
        && literal
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
}

fn continuity_observed_at(created_at: &str) -> String {
    let millis = created_at.parse::<u128>().unwrap_or(0);
    let secs = (millis / 1000) as i64;
    if let Some(dt) = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0) {
        dt.format("%Y-%m-%d").to_string()
    } else {
        "1970-01-01".to_string()
    }
}

fn build_anchor_literal_preservation_diff(
    current_document: &str,
    literals: &[ExplicitAnchorLiteral],
) -> Option<String> {
    let missing = literals
        .iter()
        .filter(|literal| !current_document.contains(&literal.literal))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return None;
    }

    let mut lines = vec!["## Entries".to_string()];
    for literal in missing {
        lines.push(format!(
            "+ anchor_id: explicit_literal_{}",
            explicit_anchor_literal_suffix(&literal.literal)
        ));
        lines.push("+ anchor_type: fact".to_string());
        lines.push(format!(
            "+ statement: Explicit continuity literal retained: `{}`.",
            literal.literal
        ));
        lines.push("+ source_class: recent_message".to_string());
        lines.push(format!("+ source_ref: {}", literal.source_ref));
        lines.push(format!("+ observed_at: {}", literal.observed_at));
        lines.push("+ confidence: high".to_string());
        lines.push("+ supersedes:".to_string());
        lines.push("+ expires_at:".to_string());
    }
    Some(lines.join("\n"))
}

fn explicit_anchor_literal_suffix(literal: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(literal.as_bytes());
    let digest = hash.finalize();
    digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn parse_summary_kind(value: &str) -> SummaryKind {
    match value {
        "condensed" => SummaryKind::Condensed,
        _ => SummaryKind::Leaf,
    }
}

pub(crate) fn estimate_tokens(content: &str) -> usize {
    let chars = content.chars().count();
    chars.div_ceil(4).max(1)
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_for_fts(value: &str) -> String {
    collapse_whitespace(value)
}

fn sanitize_fts_query(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter(|ch| ch.is_alphanumeric() || ch.is_whitespace() || *ch == '_')
        .collect::<String>();
    if sanitized.trim().is_empty() {
        "match".to_string()
    } else {
        sanitized
    }
}

fn snippet(content: &str, query: &str) -> String {
    let content_lower = content.to_lowercase();
    let query_lower = query.to_lowercase();
    if let Some(pos) = content_lower.find(&query_lower) {
        let start = pos.saturating_sub(40);
        let end = (pos + query.len() + 80).min(content.len());
        return content[start..end].to_string();
    }
    content.chars().take(140).collect()
}

fn build_deterministic_fallback(source_text: &str, input_tokens: i64) -> String {
    let truncated = if source_text.chars().count() > FALLBACK_MAX_CHARS {
        source_text
            .chars()
            .take(FALLBACK_MAX_CHARS)
            .collect::<String>()
    } else {
        source_text.to_string()
    };
    format!(
        "{} [Truncated from {input_tokens} tokens]",
        collapse_whitespace(&truncated)
    )
}

fn format_summary_timestamp(value: &str) -> String {
    let millis = value.parse::<u128>().unwrap_or(0);
    let secs = (millis / 1000) as i64;
    if let Some(dt) = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0) {
        dt.format("%Y-%m-%d %H:%M UTC").to_string()
    } else {
        "1970-01-01 00:00 UTC".to_string()
    }
}

fn sentence_fragment(content: &str, max_chars: usize) -> String {
    let collapsed = collapse_whitespace(content);
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let clipped = collapsed.chars().take(max_chars).collect::<String>();
    let clipped = clipped.trim_end();
    format!("{clipped}...")
}

fn summary_id_for(conversation_id: i64, content: &str, depth: i64) -> String {
    let mut hash = Sha256::new();
    hash.update(conversation_id.to_string().as_bytes());
    hash.update(depth.to_string().as_bytes());
    hash.update(content.as_bytes());
    let digest = hash.finalize();
    let prefix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sum_{prefix}")
}

fn validate_task_execution_steps(
    input: &[TaskExecutionPlanStepInput],
) -> Result<(Vec<TaskExecutionPlanStepInput>, i64)> {
    anyhow::ensure!(
        !input.is_empty(),
        "task execution plan must contain at least one step"
    );
    let mut steps = Vec::with_capacity(input.len());
    for step in input {
        let label = collapse_whitespace(&step.label);
        anyhow::ensure!(!label.is_empty(), "task execution step label is required");
        let status = step.status.trim().to_ascii_lowercase();
        anyhow::ensure!(
            matches!(status.as_str(), "completed" | "in_progress" | "pending"),
            "invalid task execution step status {}",
            step.status
        );
        steps.push(TaskExecutionPlanStepInput { label, status });
    }
    // Models report progress out of order: a later step finished before an
    // earlier one, two steps active at once, or nothing active while work is
    // still open. The plan's meaning is the per-step status; the invariants
    // "at most one active step" and "an incomplete plan has exactly one active
    // step" are restored by demotion and promotion. They must never fail the
    // durable task: on 07.09.2026 a customer research run died after two
    // minutes with "steps must be a completed prefix" while the worker was
    // still working.
    let mut seen_active = false;
    for step in &mut steps {
        if step.status == "in_progress" {
            if seen_active {
                step.status = "pending".to_string();
            } else {
                seen_active = true;
            }
        }
    }
    let completed = steps
        .iter()
        .filter(|step| step.status == "completed")
        .count();
    if !seen_active && completed < steps.len() {
        if let Some(next) = steps.iter_mut().find(|step| step.status == "pending") {
            next.status = "in_progress".to_string();
        }
    }
    Ok((steps, i64::try_from(completed).unwrap_or(i64::MAX)))
}

#[cfg(test)]
mod task_execution_step_tests {
    use super::{validate_task_execution_steps, TaskExecutionPlanStepInput};

    fn step(label: &str, status: &str) -> TaskExecutionPlanStepInput {
        TaskExecutionPlanStepInput {
            label: label.to_string(),
            status: status.to_string(),
        }
    }

    #[test]
    fn out_of_order_progress_is_accepted_and_counted() {
        let (steps, completed) = validate_task_execution_steps(&[
            step("Sellify prüfen", "completed"),
            step("Quellen sammeln", "pending"),
            step("Personen recherchieren", "completed"),
            step("Rückschreiben", "in_progress"),
        ])
        .expect("out-of-order progress is normalized, not rejected");
        assert_eq!(completed, 2);
        assert_eq!(
            steps.iter().map(|s| s.status.as_str()).collect::<Vec<_>>(),
            ["completed", "pending", "completed", "in_progress"]
        );
    }

    #[test]
    fn a_second_active_step_is_demoted_to_pending() {
        let (steps, completed) = validate_task_execution_steps(&[
            step("a", "in_progress"),
            step("b", "in_progress"),
            step("c", "pending"),
        ])
        .unwrap();
        assert_eq!(completed, 0);
        assert_eq!(
            steps.iter().map(|s| s.status.as_str()).collect::<Vec<_>>(),
            ["in_progress", "pending", "pending"]
        );
    }

    #[test]
    fn an_incomplete_plan_without_active_step_promotes_the_first_pending_step() {
        let (steps, completed) = validate_task_execution_steps(&[
            step("a", "completed"),
            step("b", "pending"),
            step("c", "pending"),
        ])
        .unwrap();
        assert_eq!(completed, 1);
        assert_eq!(steps[1].status, "in_progress");
        assert_eq!(steps[2].status, "pending");
    }

    #[test]
    fn a_fully_completed_plan_stays_completed() {
        let (steps, completed) =
            validate_task_execution_steps(&[step("a", "completed"), step("b", "completed")])
                .unwrap();
        assert_eq!(completed, 2);
        assert!(steps.iter().all(|s| s.status == "completed"));
    }

    #[test]
    fn invalid_status_and_empty_labels_are_still_rejected() {
        assert!(validate_task_execution_steps(&[step("a", "done")]).is_err());
        assert!(validate_task_execution_steps(&[step("   ", "pending")]).is_err());
        assert!(validate_task_execution_steps(&[]).is_err());
    }
}

fn task_execution_plan_signature(steps: &[TaskExecutionPlanStepInput]) -> String {
    let labels = steps
        .iter()
        .map(|step| step.label.as_str())
        .collect::<Vec<_>>();
    let encoded = serde_json::to_vec(&labels).unwrap_or_default();
    let digest = Sha256::digest(encoded);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn task_execution_percent(completed: i64, total: i64, review_status: &str) -> i64 {
    if review_status == "completed" {
        return 100;
    }
    if total <= 0 {
        return 0;
    }
    ((90_f64 * completed as f64 / total as f64).round() as i64).clamp(0, 90)
}

fn task_execution_phase(completed: i64, total: i64, review_status: &str) -> &'static str {
    match review_status {
        "completed" => "completed",
        "in_progress" | "failed" => "review",
        _ if total > 0 && completed == total => "review",
        _ => "working",
    }
}

fn epoch_millis_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn iso_now() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0);
    millis.to_string()
}

#[cfg(test)]
mod tests;
