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
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::AtomicU64;
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
            "Aborted" => Some(AgentOutcome::Aborted),
            "Cancelled" => Some(AgentOutcome::Cancelled),
            _ => None,
        }
    }
}

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
                return Ok(fallback);
            }
            return Err(err);
        }
        Ok(engine)
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
                rewrite_failure_count INTEGER NOT NULL DEFAULT 0
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
            self.conn.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition};"
            ))?;
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
        let seq = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE conversation_id = ?1",
                [conversation_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(1);
        let token_count = estimate_tokens(content) as i64;
        let stored_outcome = if role == "assistant" {
            outcome.map(|value| value.as_str().to_string())
        } else {
            None
        };
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
            let Some(summary_id) = self.compact_leaf_pass(conversation_id, summarizer, force)?
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
            let Some(summary_id) = self.compact_condensed_pass(conversation_id, summarizer)? else {
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
        let tx = self
            .conn
            .unchecked_transaction()
            .context("failed to begin continuity init transaction")?;
        let show_all = load_or_init_continuity_show_all(&tx, conversation_id)?;
        let previous = load_mission_state_with(&tx, conversation_id)?;
        persist_mission_state_with(
            &tx,
            &derive_mission_state_from_continuity(&show_all, previous.as_ref()),
        )?;
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
        let tx = self
            .conn
            .unchecked_transaction()
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
        let document = fetch_continuity_document_with(&tx, conversation_id, kind)?
            .context("continuity document missing after diff apply")?;
        let continuity = load_or_init_continuity_show_all(&tx, conversation_id)?;
        let previous = load_mission_state_with(&tx, conversation_id)?;
        persist_mission_state_with(
            &tx,
            &derive_mission_state_from_continuity(&continuity, previous.as_ref()),
        )?;
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
        let tx = self
            .conn
            .unchecked_transaction()
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
        let document = fetch_continuity_document_with(&tx, conversation_id, kind)?
            .context("continuity document missing after full-replace apply")?;
        let continuity = load_or_init_continuity_show_all(&tx, conversation_id)?;
        let previous = load_mission_state_with(&tx, conversation_id)?;
        persist_mission_state_with(
            &tx,
            &derive_mission_state_from_continuity(&continuity, previous.as_ref()),
        )?;
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
        let tx = self
            .conn
            .unchecked_transaction()
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
        let document = fetch_continuity_document_with(&tx, conversation_id, kind)?
            .context("continuity document missing after string-replace apply")?;
        let continuity = load_or_init_continuity_show_all(&tx, conversation_id)?;
        let previous = load_mission_state_with(&tx, conversation_id)?;
        persist_mission_state_with(
            &tx,
            &derive_mission_state_from_continuity(&continuity, previous.as_ref()),
        )?;
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
        let tx = self
            .conn
            .unchecked_transaction()
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

    pub fn list_mission_states(&self, open_only: bool) -> Result<Vec<MissionStateRecord>> {
        load_mission_states_with(&self.conn, open_only)
    }

    pub fn preview_mission_state_from_continuity(
        &self,
        conversation_id: i64,
    ) -> Result<MissionStateRecord> {
        let continuity = load_continuity_show_all_with(&self.conn, conversation_id)?;
        let previous = load_mission_state_with(&self.conn, conversation_id)?;
        Ok(derive_mission_state_from_continuity(
            &continuity,
            previous.as_ref(),
        ))
    }

    pub fn sync_mission_state_from_continuity_with_repair(
        &self,
        conversation_id: i64,
    ) -> Result<MissionStateRepairOutcome> {
        let tx = self
            .conn
            .unchecked_transaction()
            .context("failed to begin mission sync transaction")?;
        let mut continuity = load_or_init_continuity_show_all(&tx, conversation_id)?;
        let previous = load_mission_state_with(&tx, conversation_id)?;
        let previous_focus_head_commit_id = continuity.focus.head_commit_id.clone();
        let focus_repaired =
            maybe_repair_focus_continuity_with(&tx, &mut continuity, previous.as_ref())?;
        let record = derive_mission_state_from_continuity(&continuity, previous.as_ref());
        persist_mission_state_with(&tx, &record)?;
        tx.commit()
            .context("failed to commit mission sync transaction")?;
        Ok(MissionStateRepairOutcome {
            mission_state: record,
            previous_focus_head_commit_id,
            focus_head_commit_id: continuity.focus.head_commit_id.clone(),
            focus_repaired,
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
    ) -> Result<MissionStateRecord> {
        let mut record = self.mission_state(conversation_id)?;
        if record.agent_failure_count == 0 && record.deferred_reason.is_none() {
            return Ok(record);
        }
        record.agent_failure_count = 0;
        // A successful turn implicitly clears any prior deferral reason.
        record.deferred_reason = None;
        self.persist_mission_state(&record)?;
        Ok(record)
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
    /// spawning continuation self-work for this mission.
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
        let tx = self
            .conn
            .unchecked_transaction()
            .context("failed to begin focus continuity rewrite transaction")?;
        let continuity = load_or_init_continuity_show_all(&tx, conversation_id)?;
        let repaired_content = render_focus_continuity_from_record(&continuity, record);
        if repaired_content.trim() == continuity.focus.content.trim() {
            tx.commit()
                .context("failed to commit no-op focus continuity rewrite transaction")?;
            return Ok(false);
        }

        let created_at = iso_now();
        let diff_text = format!("## Status\n+ {reason}\n");
        let commit_id = continuity_commit_id(
            conversation_id,
            ContinuityKind::Focus,
            &diff_text,
            &repaired_content,
            &created_at,
        );
        let document_id = continuity_document_id(conversation_id, ContinuityKind::Focus);
        tx.execute(
            "INSERT INTO continuity_commits (commit_id, document_id, parent_commit_id, diff_text, rendered_text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                commit_id,
                document_id,
                continuity.focus.head_commit_id,
                diff_text,
                repaired_content,
                created_at
            ],
        )?;
        tx.execute(
            "UPDATE continuity_documents SET head_commit_id = ?1, updated_at = ?2 WHERE document_id = ?3",
            params![commit_id, created_at, document_id],
        )?;
        let mut updated_record = record.clone();
        updated_record.focus_head_commit_id = commit_id;
        updated_record.last_synced_at = created_at;
        persist_mission_state_with(&tx, &updated_record)?;
        tx.commit()
            .context("failed to commit focus continuity rewrite transaction")?;
        Ok(true)
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

        let now = iso_now();
        let mut stmt = self.conn.prepare(
            "SELECT claim_key, last_run_id, claim_kind, claim_status, blocks_closure, subject, summary, evidence_summary, recheck_policy, expires_at, created_at, updated_at
             FROM mission_claims
             WHERE conversation_id = ?1
               AND (claim_status != 'verified' OR (expires_at IS NOT NULL AND CAST(expires_at AS INTEGER) <= ?2))
             ORDER BY CAST(updated_at AS INTEGER) DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![conversation_id, now, limit as i64], |row| {
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

    pub fn continuity_build_prompt(
        &self,
        conversation_id: i64,
        kind: ContinuityKind,
    ) -> Result<ContinuityPromptPayload> {
        let document = self.ensure_continuity_document(conversation_id, kind)?;
        let snapshot = self.snapshot(conversation_id)?;
        let explicit_anchor_literals = if kind == ContinuityKind::Anchors {
            collect_explicit_anchor_literals(&snapshot.messages)
        } else {
            Vec::new()
        };
        let forgotten = self
            .continuity_forgotten(conversation_id, Some(kind), None)?
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
        let snapshot = self.snapshot(conversation_id)?;
        let literals = collect_explicit_anchor_literals(&snapshot.messages);
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
        _force: bool,
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
        let summary_id = self.insert_summary(
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
        )?;
        Ok(Some(summary_id))
    }

    fn compact_condensed_pass<S: Summarizer>(
        &self,
        conversation_id: i64,
        summarizer: &S,
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
            let summary_id = self.insert_summary(
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
            )?;
            return Ok(Some(summary_id));
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
        Ok(self
            .conn
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
            .unwrap_or(0))
    }

    fn next_context_ordinal(&self, conversation_id: i64) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM context_items WHERE conversation_id = ?1",
                [conversation_id],
                |row| row.get(0),
            )
            .unwrap_or(1))
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

fn is_shared_memory_io_error(err: &anyhow::Error) -> bool {
    let text = err.to_string();
    text.contains("xShmMap")
        || text.contains("shared-memory")
        || (text.contains("disk I/O error") && text.contains("resize"))
}

pub fn run_init(db_path: &Path) -> Result<()> {
    let _ = LcmEngine::open(db_path, LcmConfig::default())?;
    Ok(())
}

pub fn run_add_message(
    db_path: &Path,
    conversation_id: i64,
    role: &str,
    content: &str,
) -> Result<MessageRecord> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.add_message(conversation_id, role, content)
}

/// F3: convenience wrapper for the agent harness — record an assistant
/// turn with its structured outcome in a single call.
pub fn run_add_assistant_turn(
    db_path: &Path,
    conversation_id: i64,
    content: &str,
    outcome: AgentOutcome,
) -> Result<MessageRecord> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.add_message_with_outcome(conversation_id, "assistant", content, Some(outcome))
}

pub fn run_compact(
    db_path: &Path,
    conversation_id: i64,
    token_budget: i64,
    force: bool,
) -> Result<CompactionResult> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.compact(conversation_id, token_budget, &HeuristicSummarizer, force)
}

pub fn run_grep(
    db_path: &Path,
    conversation_id: Option<i64>,
    scope: &str,
    mode: &str,
    query: &str,
    limit: usize,
) -> Result<GrepResult> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.grep(
        conversation_id,
        GrepScope::parse(scope)?,
        GrepMode::parse(mode)?,
        query,
        limit,
    )
}

pub fn run_describe(db_path: &Path, id: &str) -> Result<Option<DescribeResult>> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.describe(id)
}

pub fn run_expand(
    db_path: &Path,
    summary_id: &str,
    depth: usize,
    include_messages: bool,
    token_cap: i64,
) -> Result<ExpandResult> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.expand(summary_id, depth, include_messages, token_cap)
}

pub fn run_dump(db_path: &Path, conversation_id: i64) -> Result<LcmSnapshot> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.snapshot(conversation_id)
}

pub fn run_secret_rewrite(
    db_path: &Path,
    conversation_id: i64,
    secret_scope: &str,
    secret_name: &str,
    match_text: &str,
    replacement_text: &str,
) -> Result<SecretRewriteResult> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.rewrite_secret_literal(
        conversation_id,
        secret_scope,
        secret_name,
        match_text,
        replacement_text,
    )
}

pub fn run_refresh_continuity(db_path: &Path, conversation_id: i64) -> Result<ContinuityRevision> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.refresh_continuity(conversation_id)
}

pub fn run_show_continuity(
    db_path: &Path,
    conversation_id: i64,
) -> Result<Option<ContinuityRevision>> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.latest_continuity(conversation_id)
}

pub fn run_continuity_init(db_path: &Path, conversation_id: i64) -> Result<ContinuityShowAll> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.continuity_init_documents(conversation_id)
}

pub fn run_continuity_show(
    db_path: &Path,
    conversation_id: i64,
    kind: Option<&str>,
) -> Result<serde_json::Value> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    if let Some(kind) = kind {
        Ok(serde_json::to_value(engine.continuity_show(
            conversation_id,
            ContinuityKind::parse(kind)?,
        )?)?)
    } else {
        Ok(serde_json::to_value(
            engine.continuity_show_all(conversation_id)?,
        )?)
    }
}

pub fn run_continuity_apply(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
    diff_path: &Path,
) -> Result<ContinuityDocumentState> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    let diff_text = std::fs::read_to_string(diff_path)
        .with_context(|| format!("failed to read continuity diff {}", diff_path.display()))?;
    engine.continuity_apply_diff(conversation_id, ContinuityKind::parse(kind)?, &diff_text)
}

pub fn run_continuity_full_replace(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
    content: &str,
) -> Result<ContinuityDocumentState> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.continuity_full_replace_document(conversation_id, ContinuityKind::parse(kind)?, content)
}

pub fn run_continuity_string_replace(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
    find: &str,
    replace: &str,
) -> Result<ContinuityDocumentState> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.continuity_string_replace_document(
        conversation_id,
        ContinuityKind::parse(kind)?,
        find,
        replace,
    )
}

pub fn run_continuity_log(
    db_path: &Path,
    conversation_id: i64,
    kind: Option<&str>,
) -> Result<Vec<ContinuityCommitRecord>> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.continuity_log(
        conversation_id,
        kind.map(ContinuityKind::parse).transpose()?,
    )
}

pub fn run_continuity_rebuild(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
) -> Result<ContinuityDocumentState> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.continuity_rebuild(conversation_id, ContinuityKind::parse(kind)?)
}

pub fn run_continuity_forgotten(
    db_path: &Path,
    conversation_id: i64,
    kind: Option<&str>,
    query: Option<&str>,
) -> Result<Vec<ContinuityForgottenEntry>> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.continuity_forgotten(
        conversation_id,
        kind.map(ContinuityKind::parse).transpose()?,
        query,
    )
}

pub fn run_continuity_build_prompt(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
) -> Result<ContinuityPromptPayload> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    engine.continuity_build_prompt(conversation_id, ContinuityKind::parse(kind)?)
}

pub fn run_context_retrieve(
    db_path: &Path,
    conversation_id: i64,
    mode: &str,
    query: Option<&str>,
    continuity_kind: Option<&str>,
    summary_id: Option<&str>,
    limit: usize,
    depth: usize,
    include_messages: bool,
    token_cap: i64,
) -> Result<serde_json::Value> {
    let engine = LcmEngine::open(db_path, LcmConfig::default())?;
    match mode {
        "current" => {
            let snapshot = engine.snapshot(conversation_id)?;
            let continuity = engine.continuity_show_all(conversation_id)?;
            Ok(serde_json::json!({
                "mode": "current",
                "conversation_id": conversation_id,
                "continuity": continuity,
                "context_items": snapshot.context_items,
                "messages": snapshot.messages,
                "summaries": snapshot.summaries,
            }))
        }
        "continuity" => {
            if let Some(kind) = continuity_kind {
                Ok(serde_json::to_value(engine.continuity_show(
                    conversation_id,
                    ContinuityKind::parse(kind)?,
                )?)?)
            } else {
                Ok(serde_json::to_value(
                    engine.continuity_show_all(conversation_id)?,
                )?)
            }
        }
        "forgotten" => Ok(serde_json::to_value(engine.continuity_forgotten(
            conversation_id,
            continuity_kind.map(ContinuityKind::parse).transpose()?,
            query,
        )?)?),
        "search" => {
            let query = query.context("context_retrieve mode=search requires query")?;
            Ok(serde_json::to_value(engine.grep(
                Some(conversation_id),
                GrepScope::Both,
                GrepMode::FullText,
                query,
                limit,
            )?)?)
        }
        "describe" => {
            let summary_id =
                summary_id.context("context_retrieve mode=describe requires summary_id")?;
            Ok(serde_json::to_value(engine.describe(summary_id)?)?)
        }
        "expand" => {
            let summary_id =
                summary_id.context("context_retrieve mode=expand requires summary_id")?;
            Ok(serde_json::to_value(engine.expand(
                summary_id,
                depth,
                include_messages,
                token_cap,
            )?)?)
        }
        other => anyhow::bail!(
            "unsupported context_retrieve mode: {other}; expected one of current, continuity, forgotten, search, describe, expand"
        ),
    }
}

pub fn run_fixture(db_path: &Path, fixture_path: &Path) -> Result<FixtureRunOutput> {
    let fixture_bytes = std::fs::read(fixture_path)
        .with_context(|| format!("failed to read fixture {}", fixture_path.display()))?;
    let fixture: LcmFixture = serde_json::from_slice(&fixture_bytes)
        .with_context(|| format!("failed to parse fixture {}", fixture_path.display()))?;
    let config = merge_fixture_config(fixture.config.clone());
    let engine = LcmEngine::open(db_path, config)?;
    let _ = engine.continuity_init_documents(fixture.conversation_id)?;
    for message in &fixture.messages {
        engine.add_message(fixture.conversation_id, &message.role, &message.content)?;
    }
    let compaction = engine.compact(
        fixture.conversation_id,
        fixture.token_budget,
        &HeuristicSummarizer,
        fixture.force_compact.unwrap_or(false),
    )?;
    let snapshot = engine.snapshot(fixture.conversation_id)?;
    let grep_results = fixture
        .grep_queries
        .unwrap_or_default()
        .into_iter()
        .map(|query| {
            engine.grep(
                Some(fixture.conversation_id),
                GrepScope::parse(&query.scope)?,
                GrepMode::parse(&query.mode)?,
                &query.query,
                query.limit.unwrap_or(20),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let fallback_summary_id = compaction.created_summary_ids.first().cloned();
    let mut expand_results = Vec::new();
    for query in fixture.expand_queries.unwrap_or_default() {
        if let Some(summary_id) = query.summary_id.or_else(|| fallback_summary_id.clone()) {
            expand_results.push(engine.expand(
                &summary_id,
                query.depth.unwrap_or(1),
                query.include_messages.unwrap_or(false),
                query.token_cap.unwrap_or(8_000),
            )?);
        }
    }
    Ok(FixtureRunOutput {
        compaction,
        snapshot,
        grep_results,
        expand_results,
    })
}

fn merge_fixture_config(config: Option<LcmFixtureConfig>) -> LcmConfig {
    let mut merged = LcmConfig::default();
    if let Some(config) = config {
        if let Some(value) = config.context_threshold {
            merged.context_threshold = value;
        }
        if let Some(value) = config.min_compaction_tokens {
            merged.min_compaction_tokens = value;
        }
        if let Some(value) = config.fresh_tail_count {
            merged.fresh_tail_count = value;
        }
        if let Some(value) = config.leaf_chunk_tokens {
            merged.leaf_chunk_tokens = value;
        }
        if let Some(value) = config.leaf_target_tokens {
            merged.leaf_target_tokens = value;
        }
        if let Some(value) = config.condensed_target_tokens {
            merged.condensed_target_tokens = value;
        }
        if let Some(value) = config.leaf_min_fanout {
            merged.leaf_min_fanout = value;
        }
        if let Some(value) = config.condensed_min_fanout {
            merged.condensed_min_fanout = value;
        }
        if let Some(value) = config.max_rounds {
            merged.max_rounds = value;
        }
    }
    merged
}

impl LcmEngine {
    fn persist_mission_state(&self, record: &MissionStateRecord) -> Result<()> {
        persist_mission_state_with(&self.conn, record)
    }
}

fn load_mission_state_with(
    conn: &Connection,
    conversation_id: i64,
) -> Result<Option<MissionStateRecord>> {
    conn.query_row(
        "SELECT mission, mission_status, continuation_mode, trigger_intensity, blocker, next_slice, done_gate, closure_confidence, is_open, allow_idle, focus_head_commit_id, last_synced_at, watcher_last_triggered_at, watcher_trigger_count, agent_failure_count, deferred_reason, rewrite_failure_count FROM mission_states WHERE conversation_id = ?1",
        [conversation_id],
        |row| {
            Ok(MissionStateRecord {
                conversation_id,
                mission: row.get(0)?,
                mission_status: row.get(1)?,
                continuation_mode: row.get(2)?,
                trigger_intensity: row.get(3)?,
                blocker: row.get(4)?,
                next_slice: row.get(5)?,
                done_gate: row.get(6)?,
                closure_confidence: row.get(7)?,
                is_open: row.get::<_, i64>(8)? != 0,
                allow_idle: row.get::<_, i64>(9)? != 0,
                focus_head_commit_id: row.get(10)?,
                last_synced_at: row.get(11)?,
                watcher_last_triggered_at: row.get(12)?,
                watcher_trigger_count: row.get(13)?,
                agent_failure_count: row.get(14)?,
                deferred_reason: row.get(15)?,
                rewrite_failure_count: row.get(16)?,
            })
        },
    )
    .optional()
    .context("failed to load mission state")
}

fn load_mission_states_with(conn: &Connection, open_only: bool) -> Result<Vec<MissionStateRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT conversation_id, mission, mission_status, continuation_mode, trigger_intensity, blocker, next_slice, done_gate, closure_confidence, is_open, allow_idle, focus_head_commit_id, last_synced_at, watcher_last_triggered_at, watcher_trigger_count, agent_failure_count, deferred_reason, rewrite_failure_count
             FROM mission_states
             WHERE (?1 = 0 OR is_open = 1)
             ORDER BY is_open DESC, last_synced_at DESC, conversation_id ASC",
        )
        .context("failed to prepare mission state listing query")?;
    let rows = stmt.query_map(params![if open_only { 1 } else { 0 }], |row| {
        Ok(MissionStateRecord {
            conversation_id: row.get(0)?,
            mission: row.get(1)?,
            mission_status: row.get(2)?,
            continuation_mode: row.get(3)?,
            trigger_intensity: row.get(4)?,
            blocker: row.get(5)?,
            next_slice: row.get(6)?,
            done_gate: row.get(7)?,
            closure_confidence: row.get(8)?,
            is_open: row.get::<_, i64>(9)? != 0,
            allow_idle: row.get::<_, i64>(10)? != 0,
            focus_head_commit_id: row.get(11)?,
            last_synced_at: row.get(12)?,
            watcher_last_triggered_at: row.get(13)?,
            watcher_trigger_count: row.get(14)?,
            agent_failure_count: row.get(15)?,
            deferred_reason: row.get(16)?,
            rewrite_failure_count: row.get(17)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
        .context("failed to load mission states")
}

/// One recorded attempt to clobber a protected `mission_states` field. The
/// guard preserved the prior non-empty value; this entry is staged on the
/// thread-local buffer and flushed to `governance_events` after the
/// surrounding transaction commits (governance writes open a separate
/// connection and would deadlock against an open lcm write transaction on
/// the same DB if emitted inline).
#[derive(Debug, Clone)]
pub(crate) struct PendingMissionStateClobberAttempt {
    pub conversation_id: i64,
    pub field: &'static str,
    pub previous_value: String,
    pub attempted_value: String,
    pub previous_value_chars: usize,
}

thread_local! {
    /// Per-thread buffer of suppressed clobber attempts. Drained by
    /// `LcmEngine::drain_pending_mission_state_clobber_events_to_governance`
    /// once the surrounding lcm transaction has committed and a governance
    /// connection can safely be opened.
    static PENDING_MISSION_STATE_CLOBBERS: RefCell<Vec<PendingMissionStateClobberAttempt>> =
        const { RefCell::new(Vec::new()) };
}

fn push_pending_mission_state_clobber(attempt: PendingMissionStateClobberAttempt) {
    PENDING_MISSION_STATE_CLOBBERS.with(|cell| cell.borrow_mut().push(attempt));
}

pub(crate) fn drain_pending_mission_state_clobbers() -> Vec<PendingMissionStateClobberAttempt> {
    PENDING_MISSION_STATE_CLOBBERS.with(|cell| std::mem::take(&mut *cell.borrow_mut()))
}

/// Drain any clobber attempts that the P2 guard suppressed during this
/// thread's recent persist calls and publish them as
/// `mission_state_field_clobbered_blocked` governance events. Safe to call
/// from any post-turn / post-boot maintenance pass: a no-op when the buffer
/// is empty. Failures are swallowed so the audit channel never breaks a
/// successful state transition (mirrors the `let _ =
/// governance::record_event(...)` pattern in service.rs).
pub fn drain_pending_mission_state_clobber_events_to_governance(root: &Path) {
    let pending = drain_pending_mission_state_clobbers();
    for attempt in pending {
        let _ = crate::governance::record_event(
            root,
            crate::governance::GovernanceEventRequest {
                mechanism_id: "mission_state_field_clobbered_blocked",
                conversation_id: Some(attempt.conversation_id),
                severity: "warning",
                reason: "mission_state_field_clobber_blocked",
                action_taken: "preserved_prior_non_empty_field",
                details: serde_json::json!({
                    "field": attempt.field,
                    "previous_value_chars": attempt.previous_value_chars,
                    "previous_value": attempt.previous_value,
                    "attempted_value": attempt.attempted_value,
                }),
                idempotence_key: None,
            },
        );
    }
}

#[cfg(test)]
pub(crate) fn drain_pending_mission_state_clobbers_for_test(
) -> Vec<PendingMissionStateClobberAttempt> {
    drain_pending_mission_state_clobbers()
}

/// True when `value` is empty after trimming whitespace (`""`, `"   "`,
/// `"\n"`, etc. all collapse to "this writer cleared the field"). Structural
/// — no parsing, no string-matching against any sentinel.
fn is_blank_field(value: &str) -> bool {
    value.trim().is_empty()
}

/// Bypass key the dedicated owner-intent clearer flips before issuing a
/// legitimate clear. Wired through a thread-local so we don't have to
/// thread an extra parameter through every persist path.
thread_local! {
    static OWNER_INTENT_CLEAR_BYPASS_DEPTH: RefCell<u32> = const { RefCell::new(0) };
}

struct OwnerIntentClearGuard;
impl OwnerIntentClearGuard {
    fn enter() -> Self {
        OWNER_INTENT_CLEAR_BYPASS_DEPTH.with(|cell| *cell.borrow_mut() += 1);
        Self
    }
}
impl Drop for OwnerIntentClearGuard {
    fn drop(&mut self) {
        OWNER_INTENT_CLEAR_BYPASS_DEPTH.with(|cell| {
            let mut depth = cell.borrow_mut();
            if *depth > 0 {
                *depth -= 1;
            }
        });
    }
}

fn owner_intent_clear_active() -> bool {
    OWNER_INTENT_CLEAR_BYPASS_DEPTH.with(|cell| *cell.borrow() > 0)
}

fn persist_mission_state_with(conn: &Connection, record: &MissionStateRecord) -> Result<()> {
    // P2 — Mission-state field clobber guard.
    //
    // Production smoke-test (Befund C) saw `next_slice` (81 chars) and
    // `done_gate` (289 chars) silently collapse to length 0 within ~25
    // minutes while `mission` (217 chars) was preserved. The suspected
    // writer is `derive_mission_state_from_continuity`, which produces
    // empty `next_slice` / `done_gate` strings whenever the focus
    // continuity document does not currently carry an explicit
    // `next_slice:` / `done_gate:` line. That overwrite path is a
    // mission-continuity-normalize pass triggered by every
    // `continuity_apply_diff` / full-replace / string-replace / sync.
    //
    // We install a one-way ratchet on `next_slice` and `done_gate`: once
    // they hold non-empty content, automation may only replace them with
    // new non-empty content. A blank-incoming write while the prior row
    // is non-empty preserves the prior value field-locally, and the
    // attempted clobber is staged on a thread-local buffer that the
    // engine flushes to `governance_events` once the surrounding
    // transaction has committed (we cannot open a second connection
    // against the same WAL DB while a write transaction is still open
    // on this thread without risking a busy_timeout deadlock).
    //
    // Operator/skill paths that legitimately *want* to clear these
    // fields call `clear_mission_state_done_fields_with_owner_intent`,
    // which sets a thread-local bypass for the duration of the clear.
    let mut effective_next_slice = record.next_slice.clone();
    let mut effective_done_gate = record.done_gate.clone();
    if !owner_intent_clear_active() {
        let existing = load_mission_state_with(conn, record.conversation_id)?;
        if let Some(existing) = existing {
            if !is_blank_field(&existing.next_slice) && is_blank_field(&effective_next_slice) {
                push_pending_mission_state_clobber(PendingMissionStateClobberAttempt {
                    conversation_id: record.conversation_id,
                    field: "next_slice",
                    previous_value: existing.next_slice.clone(),
                    attempted_value: effective_next_slice.clone(),
                    previous_value_chars: existing.next_slice.chars().count(),
                });
                effective_next_slice = existing.next_slice;
            }
            if !is_blank_field(&existing.done_gate) && is_blank_field(&effective_done_gate) {
                push_pending_mission_state_clobber(PendingMissionStateClobberAttempt {
                    conversation_id: record.conversation_id,
                    field: "done_gate",
                    previous_value: existing.done_gate.clone(),
                    attempted_value: effective_done_gate.clone(),
                    previous_value_chars: existing.done_gate.chars().count(),
                });
                effective_done_gate = existing.done_gate;
            }
        }
    }

    conn.execute(
        "INSERT INTO mission_states (
            conversation_id,
            mission,
            mission_status,
            continuation_mode,
            trigger_intensity,
            blocker,
            next_slice,
            done_gate,
            closure_confidence,
            is_open,
            allow_idle,
            focus_head_commit_id,
            last_synced_at,
            watcher_last_triggered_at,
            watcher_trigger_count,
            agent_failure_count,
            deferred_reason,
            rewrite_failure_count
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
        ON CONFLICT(conversation_id) DO UPDATE SET
            mission = excluded.mission,
            mission_status = excluded.mission_status,
            continuation_mode = excluded.continuation_mode,
            trigger_intensity = excluded.trigger_intensity,
            blocker = excluded.blocker,
            next_slice = excluded.next_slice,
            done_gate = excluded.done_gate,
            closure_confidence = excluded.closure_confidence,
            is_open = excluded.is_open,
            allow_idle = excluded.allow_idle,
            focus_head_commit_id = excluded.focus_head_commit_id,
            last_synced_at = excluded.last_synced_at,
            watcher_last_triggered_at = excluded.watcher_last_triggered_at,
            watcher_trigger_count = excluded.watcher_trigger_count,
            agent_failure_count = excluded.agent_failure_count,
            deferred_reason = excluded.deferred_reason,
            rewrite_failure_count = excluded.rewrite_failure_count",
        params![
            record.conversation_id,
            record.mission,
            record.mission_status,
            record.continuation_mode,
            record.trigger_intensity,
            record.blocker,
            effective_next_slice,
            effective_done_gate,
            record.closure_confidence,
            if record.is_open { 1 } else { 0 },
            if record.allow_idle { 1 } else { 0 },
            record.focus_head_commit_id,
            record.last_synced_at,
            record.watcher_last_triggered_at,
            record.watcher_trigger_count,
            record.agent_failure_count,
            record.deferred_reason,
            record.rewrite_failure_count,
        ],
    )?;
    Ok(())
}

fn load_or_init_continuity_show_all(
    conn: &Connection,
    conversation_id: i64,
) -> Result<ContinuityShowAll> {
    let narrative =
        ensure_continuity_document_with(conn, conversation_id, ContinuityKind::Narrative)?;
    let anchors = ensure_continuity_document_with(conn, conversation_id, ContinuityKind::Anchors)?;
    let focus = ensure_continuity_document_with(conn, conversation_id, ContinuityKind::Focus)?;
    Ok(ContinuityShowAll {
        conversation_id,
        narrative,
        anchors,
        focus,
    })
}

fn load_continuity_show_all_with(
    conn: &Connection,
    conversation_id: i64,
) -> Result<ContinuityShowAll> {
    let narrative =
        fetch_continuity_document_with(conn, conversation_id, ContinuityKind::Narrative)?
            .context("missing stored narrative continuity document")?;
    let anchors = fetch_continuity_document_with(conn, conversation_id, ContinuityKind::Anchors)?
        .context("missing stored anchors continuity document")?;
    let focus = fetch_continuity_document_with(conn, conversation_id, ContinuityKind::Focus)?
        .context("missing stored focus continuity document")?;
    Ok(ContinuityShowAll {
        conversation_id,
        narrative,
        anchors,
        focus,
    })
}

fn ensure_continuity_document_with(
    conn: &Connection,
    conversation_id: i64,
    kind: ContinuityKind,
) -> Result<ContinuityDocumentState> {
    if let Some(state) = fetch_continuity_document_with(conn, conversation_id, kind)? {
        return Ok(state);
    }

    let document_id = continuity_document_id(conversation_id, kind);
    let created_at = iso_now();
    let template = continuity_template(kind).to_string();
    let base_commit_id = continuity_base_commit_id(conversation_id, kind);
    conn.execute(
        "INSERT INTO continuity_commits (commit_id, document_id, parent_commit_id, diff_text, rendered_text, created_at)
         VALUES (?1, ?2, NULL, ?3, ?4, ?5)",
        params![base_commit_id, document_id, "", template, created_at],
    )?;
    conn.execute(
        "INSERT INTO continuity_documents (document_id, conversation_id, kind, head_commit_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![document_id, conversation_id, kind.as_str(), base_commit_id, created_at],
    )?;

    fetch_continuity_document_with(conn, conversation_id, kind)?
        .context("continuity document missing after init")
}

fn fetch_continuity_document_with(
    conn: &Connection,
    conversation_id: i64,
    kind: ContinuityKind,
) -> Result<Option<ContinuityDocumentState>> {
    conn.query_row(
        "SELECT d.head_commit_id, c.rendered_text, d.created_at, d.updated_at
         FROM continuity_documents d
         JOIN continuity_commits c ON c.commit_id = d.head_commit_id
         WHERE d.conversation_id = ?1 AND d.kind = ?2",
        params![conversation_id, kind.as_str()],
        |row| {
            Ok(ContinuityDocumentState {
                conversation_id,
                kind,
                head_commit_id: row.get(0)?,
                content: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn rewrite_message_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn.prepare(
            "SELECT message_id, content FROM messages
             WHERE conversation_id = ?1 AND instr(content, ?2) > 0",
        )?;
        let mapped = stmt.query_map(params![conversation_id, match_text], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (message_id, content) in &rows {
        let replaced = content.replace(match_text, replacement_text);
        conn.execute(
            "UPDATE messages SET content = ?1, token_count = ?2 WHERE message_id = ?3",
            params![replaced, estimate_tokens(&replaced) as i64, message_id],
        )?;
        conn.execute(
            "INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', ?1, ?2)",
            params![message_id, normalize_for_fts(content)],
        )?;
        conn.execute(
            "INSERT INTO messages_fts (rowid, content) VALUES (?1, ?2)",
            params![message_id, normalize_for_fts(&replaced)],
        )?;
    }
    Ok(rows.len())
}

fn rewrite_summary_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT summary_id, content FROM summaries
             WHERE conversation_id = ?1 AND instr(content, ?2) > 0",
        )?;
        let mapped = stmt.query_map(params![conversation_id, match_text], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (summary_id, content) in &rows {
        let replaced = content.replace(match_text, replacement_text);
        conn.execute(
            "UPDATE summaries SET content = ?1, token_count = ?2 WHERE summary_id = ?3",
            params![replaced, estimate_tokens(&replaced) as i64, summary_id],
        )?;
        conn.execute(
            "INSERT INTO summaries_fts(summaries_fts, rowid, summary_id, content)
             VALUES('delete', (SELECT rowid FROM summaries WHERE summary_id = ?1), ?1, ?2)",
            params![summary_id, normalize_for_fts(content)],
        )?;
        conn.execute(
            "INSERT INTO summaries_fts (rowid, summary_id, content)
             VALUES ((SELECT rowid FROM summaries WHERE summary_id = ?1), ?1, ?2)",
            params![summary_id, normalize_for_fts(&replaced)],
        )?;
    }
    Ok(rows.len())
}

fn rewrite_continuity_commit_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    let rows: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT c.commit_id, c.diff_text, c.rendered_text
             FROM continuity_commits c
             JOIN continuity_documents d ON d.document_id = c.document_id
             WHERE d.conversation_id = ?1
               AND (instr(c.diff_text, ?2) > 0 OR instr(c.rendered_text, ?2) > 0)",
        )?;
        let mapped = stmt.query_map(params![conversation_id, match_text], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (commit_id, diff_text, rendered_text) in &rows {
        conn.execute(
            "UPDATE continuity_commits SET diff_text = ?1, rendered_text = ?2 WHERE commit_id = ?3",
            params![
                diff_text.replace(match_text, replacement_text),
                rendered_text.replace(match_text, replacement_text),
                commit_id
            ],
        )?;
    }
    if !rows.is_empty() {
        conn.execute(
            "UPDATE continuity_documents SET updated_at = ?1 WHERE conversation_id = ?2",
            params![iso_now(), conversation_id],
        )?;
    }
    Ok(rows.len())
}

fn rewrite_continuity_revision_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE continuity_revisions
         SET narrative = replace(narrative, ?2, ?3),
             anchors = replace(anchors, ?2, ?3),
             focus = replace(focus, ?2, ?3)
         WHERE conversation_id = ?1
           AND (instr(narrative, ?2) > 0 OR instr(anchors, ?2) > 0 OR instr(focus, ?2) > 0)",
        params![conversation_id, match_text, replacement_text],
    )?)
}

fn rewrite_mission_state_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE mission_states
         SET mission = replace(mission, ?2, ?3),
             blocker = replace(blocker, ?2, ?3),
             next_slice = replace(next_slice, ?2, ?3),
             done_gate = replace(done_gate, ?2, ?3)
         WHERE conversation_id = ?1
           AND (instr(mission, ?2) > 0 OR instr(blocker, ?2) > 0 OR instr(next_slice, ?2) > 0 OR instr(done_gate, ?2) > 0)",
        params![conversation_id, match_text, replacement_text],
    )?)
}

fn rewrite_verification_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE verification_runs
         SET goal = replace(goal, ?2, ?3),
             preview = replace(preview, ?2, ?3),
             result_excerpt = replace(result_excerpt, ?2, ?3),
             blocker = replace(COALESCE(blocker, ''), ?2, ?3),
             review_summary = replace(review_summary, ?2, ?3),
             report_excerpt = replace(report_excerpt, ?2, ?3)
         WHERE conversation_id = ?1
           AND (
             instr(goal, ?2) > 0 OR instr(preview, ?2) > 0 OR instr(result_excerpt, ?2) > 0 OR
             instr(COALESCE(blocker, ''), ?2) > 0 OR instr(review_summary, ?2) > 0 OR instr(report_excerpt, ?2) > 0
           )",
        params![conversation_id, match_text, replacement_text],
    )?)
}

fn rewrite_claim_rows_with(
    conn: &Connection,
    conversation_id: i64,
    match_text: &str,
    replacement_text: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE mission_claims
         SET subject = replace(subject, ?2, ?3),
             summary = replace(summary, ?2, ?3),
             evidence_summary = replace(evidence_summary, ?2, ?3)
         WHERE conversation_id = ?1
           AND (instr(subject, ?2) > 0 OR instr(summary, ?2) > 0 OR instr(evidence_summary, ?2) > 0)",
        params![conversation_id, match_text, replacement_text],
    )?)
}

fn derive_mission_state_from_continuity(
    continuity: &ContinuityShowAll,
    previous: Option<&MissionStateRecord>,
) -> MissionStateRecord {
    let contract_lines = continuity_section_lines(&continuity.focus.content, "Contract");
    let state_lines = continuity_section_lines(&continuity.focus.content, "State");
    let legacy_status_lines = continuity_section_lines(&continuity.focus.content, "Status");
    let legacy_blocker_lines = continuity_section_lines(&continuity.focus.content, "Blocker");
    let legacy_next_lines = continuity_section_lines(&continuity.focus.content, "Next");
    let legacy_gate_lines = continuity_section_lines(&continuity.focus.content, "Done / Gate");

    let mission = last_named_value(&contract_lines, &["mission", "goal"])
        .or_else(|| last_named_value(&legacy_status_lines, &["Mission"]))
        .or_else(|| first_non_meta_line(&contract_lines))
        .or_else(|| first_non_meta_line(&legacy_status_lines))
        .filter(|value| !value.trim().is_empty())
        .or_else(|| previous.map(|record| record.mission.clone()))
        .unwrap_or_default();
    let mission_status = canonicalize_mission_status(
        last_named_value(&contract_lines, &["mission_state", "mission state"])
            .or_else(|| last_named_value(&legacy_status_lines, &["Mission state"]))
            .as_deref(),
    )
    .or_else(|| previous.map(|record| record.mission_status.clone()))
    .unwrap_or_else(|| "active".to_string());
    let continuation_mode = canonicalize_continuation_mode(
        last_named_value(&contract_lines, &["continuation_mode", "continuation mode"])
            .or_else(|| last_named_value(&legacy_status_lines, &["Continuation mode"]))
            .as_deref(),
    )
    .or_else(|| previous.map(|record| record.continuation_mode.clone()))
    .unwrap_or_else(|| "continuous".to_string());
    let trigger_intensity = canonicalize_trigger_intensity(
        last_named_value(&contract_lines, &["trigger_intensity", "trigger intensity"])
            .or_else(|| last_named_value(&legacy_status_lines, &["Trigger intensity"]))
            .as_deref(),
    )
    .or_else(|| previous.map(|record| record.trigger_intensity.clone()))
    .unwrap_or_else(|| "hot".to_string());
    let blocker = last_named_value_allow_empty(&state_lines, &["blocker", "current blocker"])
        .or_else(|| last_named_value_allow_empty(&legacy_blocker_lines, &["Current blocker"]))
        .or_else(|| first_meaningful_line(&state_lines))
        .or_else(|| first_meaningful_line(&legacy_blocker_lines))
        .unwrap_or_default();
    let next_slice = last_named_value_allow_empty(&state_lines, &["next_slice", "next slice"])
        .or_else(|| last_named_value_allow_empty(&legacy_next_lines, &["Next slice"]))
        .or_else(|| first_meaningful_line(&state_lines))
        .or_else(|| first_meaningful_line(&legacy_next_lines))
        .unwrap_or_default();
    let done_gate = last_named_value_allow_empty(&state_lines, &["done_gate", "done gate"])
        .or_else(|| last_named_value_allow_empty(&legacy_gate_lines, &["Done gate"]))
        .or_else(|| first_non_meta_line(&state_lines))
        .or_else(|| first_non_meta_line(&legacy_gate_lines))
        .unwrap_or_default();
    let closure_confidence = canonicalize_closure_confidence(
        last_named_value(&state_lines, &["closure_confidence", "closure confidence"])
            .or_else(|| last_named_value(&legacy_gate_lines, &["Closure confidence"]))
            .as_deref(),
    )
    .or_else(|| previous.map(|record| record.closure_confidence.clone()))
    .unwrap_or_else(|| "low".to_string());
    let is_open = mission_is_open(
        &mission,
        &mission_status,
        &continuation_mode,
        &next_slice,
        &done_gate,
        &closure_confidence,
    );
    let allow_idle = mission_allows_idle(&mission_status, &continuation_mode, &trigger_intensity);

    MissionStateRecord {
        conversation_id: continuity.conversation_id,
        mission,
        mission_status,
        continuation_mode,
        trigger_intensity,
        blocker,
        next_slice,
        done_gate,
        closure_confidence,
        is_open,
        allow_idle,
        focus_head_commit_id: continuity.focus.head_commit_id.clone(),
        last_synced_at: iso_now(),
        watcher_last_triggered_at: previous
            .and_then(|record| record.watcher_last_triggered_at.clone()),
        watcher_trigger_count: previous
            .map(|record| record.watcher_trigger_count)
            .unwrap_or(0),
        agent_failure_count: previous
            .map(|record| record.agent_failure_count)
            .unwrap_or(0),
        deferred_reason: previous.and_then(|record| record.deferred_reason.clone()),
        rewrite_failure_count: previous
            .map(|record| record.rewrite_failure_count)
            .unwrap_or(0),
    }
}

fn maybe_repair_focus_continuity_with(
    conn: &Connection,
    continuity: &mut ContinuityShowAll,
    previous: Option<&MissionStateRecord>,
) -> Result<bool> {
    if focus_semantic_conflicts_local(&continuity.focus.content).is_empty() {
        return Ok(false);
    }

    let repaired_content = render_canonical_focus_continuity(continuity, previous);
    if repaired_content.trim() == continuity.focus.content.trim() {
        return Ok(false);
    }

    let created_at = iso_now();
    let commit_id = continuity_commit_id(
        continuity.conversation_id,
        ContinuityKind::Focus,
        "## Status\n+ Canonicalized conflicting focus fields during mission-state resync.\n",
        &repaired_content,
        &created_at,
    );
    let document_id = continuity_document_id(continuity.conversation_id, ContinuityKind::Focus);
    conn.execute(
        "INSERT INTO continuity_commits (commit_id, document_id, parent_commit_id, diff_text, rendered_text, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            commit_id,
            document_id,
            continuity.focus.head_commit_id,
            "## Status\n+ Canonicalized conflicting focus fields during mission-state resync.\n",
            repaired_content,
            created_at
        ],
    )?;
    conn.execute(
        "UPDATE continuity_documents SET head_commit_id = ?1, updated_at = ?2 WHERE document_id = ?3",
        params![commit_id, created_at, document_id],
    )?;
    continuity.focus =
        fetch_continuity_document_with(conn, continuity.conversation_id, ContinuityKind::Focus)?
            .context("focus continuity missing after repair")?;
    Ok(true)
}

fn render_canonical_focus_continuity(
    continuity: &ContinuityShowAll,
    previous: Option<&MissionStateRecord>,
) -> String {
    let record = derive_mission_state_from_continuity(continuity, previous);
    render_focus_continuity_from_record(continuity, &record)
}

fn render_focus_continuity_from_record(
    continuity: &ContinuityShowAll,
    record: &MissionStateRecord,
) -> String {
    let contract_lines = continuity_section_lines(&continuity.focus.content, "Contract");
    let state_lines = continuity_section_lines(&continuity.focus.content, "State");
    let legacy_gate_lines = continuity_section_lines(&continuity.focus.content, "Done / Gate");
    let source_lines = continuity_section_lines(&continuity.focus.content, "Sources");
    let retry_condition = last_named_value(&state_lines, &["retry_condition", "retry condition"])
        .or_else(|| last_named_value(&legacy_gate_lines, &["Retry condition"]))
        .unwrap_or_default();
    let missing_dependency =
        last_named_value(&state_lines, &["missing_dependency", "missing dependency"])
            .unwrap_or_default();
    let slice = last_named_value(&contract_lines, &["slice"])
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| record.next_slice.clone());
    let slice_state = last_named_value(&contract_lines, &["slice_state", "slice state"])
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if record.is_open {
                "active".to_string()
            } else {
                "closed".to_string()
            }
        });
    let canonical_source_lines = canonical_focus_source_lines(&source_lines);
    let mut lines = vec![
        "# ACTIVE FOCUS".to_string(),
        String::new(),
        "## Status".to_string(),
        format!("- Mission: {}", record.mission),
        format!("- Mission state: {}", record.mission_status),
        format!("- Continuation mode: {}", record.continuation_mode),
        format!("- Trigger intensity: {}", record.trigger_intensity),
        String::new(),
        "## Blocker".to_string(),
        format!("- Current blocker: {}", record.blocker),
        String::new(),
        "## Next".to_string(),
        format!("- Next slice: {}", record.next_slice),
        String::new(),
        "## Done / Gate".to_string(),
        format!("- Done gate: {}", record.done_gate),
        format!("- Retry condition: {}", retry_condition),
        format!("- Closure confidence: {}", record.closure_confidence),
        String::new(),
        "## Contract".to_string(),
        format!("- mission: {}", record.mission),
        format!("- mission_state: {}", record.mission_status),
        format!("- continuation_mode: {}", record.continuation_mode),
        format!("- trigger_intensity: {}", record.trigger_intensity),
        format!("- slice: {}", slice),
        format!("- slice_state: {}", slice_state),
        String::new(),
        "## State".to_string(),
        format!("- goal: {}", record.mission),
        format!("- blocker: {}", record.blocker),
        format!("- missing_dependency: {}", missing_dependency),
        format!("- next_slice: {}", record.next_slice),
        format!("- done_gate: {}", record.done_gate),
        format!("- retry_condition: {}", retry_condition),
        format!("- closure_confidence: {}", record.closure_confidence),
        String::new(),
        "## Sources".to_string(),
    ];
    lines.extend(
        canonical_source_lines
            .into_iter()
            .map(|line| format!("- {line}")),
    );
    lines.push(String::new());
    lines.join("\n")
}

fn canonical_focus_source_lines(lines: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !out.iter().any(|existing: &String| {
            normalize_mission_text(existing) == normalize_mission_text(trimmed)
        }) {
            out.push(trimmed.to_string());
        }
    }
    if out.is_empty() {
        out.push("source_refs:".to_string());
        out.push("none".to_string());
        out.push("updated_at:".to_string());
    }
    out
}

fn focus_semantic_conflicts_local(content: &str) -> Vec<String> {
    let tracked_fields = [
        "Mission",
        "Mission state",
        "Continuation mode",
        "Trigger intensity",
        "Current blocker",
        "Next slice",
        "Done gate",
        "Closure confidence",
    ];
    let mut seen: std::collections::BTreeMap<&'static str, Vec<String>> =
        std::collections::BTreeMap::new();

    for raw_line in content.lines() {
        let line = raw_line.trim_start_matches(['-', '+', '*', ' ']).trim();
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        for field in tracked_fields {
            if normalize_mission_text(name) == normalize_mission_text(field) {
                let value = value.trim();
                if !value.is_empty() {
                    seen.entry(field).or_default().push(value.to_string());
                }
            }
        }
    }

    let mut conflicts = Vec::new();
    for (field, values) in seen {
        let mut distinct = Vec::new();
        for value in values {
            if !distinct.iter().any(|existing: &String| {
                normalize_mission_text(existing) == normalize_mission_text(&value)
            }) {
                distinct.push(value);
            }
        }
        if distinct.len() > 1 {
            conflicts.push(format!("{field} has conflicting values {:?}", distinct));
        }
    }
    conflicts
}

fn map_verification_run_row(
    row: &rusqlite::Row<'_>,
    conversation_id: i64,
) -> rusqlite::Result<VerificationRunRecord> {
    let review_reasons: Vec<String> =
        serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default();
    let failed_gates: Vec<String> =
        serde_json::from_str(&row.get::<_, String>(14)?).unwrap_or_default();
    let semantic_findings: Vec<String> =
        serde_json::from_str(&row.get::<_, String>(15)?).unwrap_or_default();
    let open_items: Vec<String> =
        serde_json::from_str(&row.get::<_, String>(16)?).unwrap_or_default();
    let evidence: Vec<String> =
        serde_json::from_str(&row.get::<_, String>(17)?).unwrap_or_default();
    let handoff = row.get::<_, String>(18)?;
    Ok(VerificationRunRecord {
        run_id: row.get(0)?,
        conversation_id,
        source_label: row.get(1)?,
        goal: row.get(2)?,
        preview: row.get(3)?,
        result_excerpt: row.get(4)?,
        blocker: row.get(5)?,
        review_required: row.get::<_, i64>(6)? != 0,
        review_verdict: row.get(7)?,
        review_summary: row.get(8)?,
        review_score: row.get(9)?,
        review_reasons,
        report_excerpt: row.get(11)?,
        raw_report: row.get(12)?,
        mission_state: row.get(13)?,
        failed_gates,
        semantic_findings,
        open_items,
        evidence,
        handoff: if handoff.trim().is_empty() {
            None
        } else {
            Some(handoff)
        },
        claim_count: row.get(19)?,
        open_claim_count: row.get(20)?,
        closure_blocking_claim_count: row.get(21)?,
        created_at: row.get(22)?,
    })
}

fn map_mission_claim_row(
    row: &rusqlite::Row<'_>,
    conversation_id: i64,
) -> rusqlite::Result<MissionClaimRecord> {
    Ok(MissionClaimRecord {
        claim_key: row.get(0)?,
        conversation_id,
        last_run_id: row.get(1)?,
        claim_kind: row.get(2)?,
        claim_status: row.get(3)?,
        blocks_closure: row.get::<_, i64>(4)? != 0,
        subject: row.get(5)?,
        summary: row.get(6)?,
        evidence_summary: row.get(7)?,
        recheck_policy: row.get(8)?,
        expires_at: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn map_strategic_directive_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StrategicDirectiveRecord> {
    Ok(StrategicDirectiveRecord {
        directive_id: row.get(0)?,
        conversation_id: row.get(1)?,
        thread_key: row.get(2)?,
        directive_kind: row.get(3)?,
        title: row.get(4)?,
        body_text: row.get(5)?,
        status: row.get(6)?,
        revision: row.get(7)?,
        previous_directive_id: row.get(8)?,
        author: row.get(9)?,
        decided_by: row.get(10)?,
        decision_reason: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn continuity_section_lines(content: &str, section_name: &str) -> Vec<String> {
    let mut active = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(header) = trimmed.strip_prefix("## ") {
            active = header == section_name;
            continue;
        }
        if active && !trimmed.is_empty() && !trimmed.starts_with('#') {
            lines.push(trimmed.trim_start_matches("- ").trim().to_string());
        }
    }
    lines
}

fn last_named_value(lines: &[String], names: &[&str]) -> Option<String> {
    let mut out = None;
    for line in lines {
        if let Some((prefix, value)) = line.split_once(':') {
            if names
                .iter()
                .any(|name| prefix.trim().eq_ignore_ascii_case(name))
            {
                let value = value.trim();
                if !value.is_empty() {
                    out = Some(value.to_string());
                }
            }
        }
    }
    out
}

fn last_named_value_allow_empty(lines: &[String], names: &[&str]) -> Option<String> {
    let mut out = None;
    for line in lines {
        if let Some((prefix, value)) = line.split_once(':') {
            if names
                .iter()
                .any(|name| prefix.trim().eq_ignore_ascii_case(name))
            {
                out = Some(value.trim().to_string());
            }
        }
    }
    out
}

fn first_non_meta_line(lines: &[String]) -> Option<String> {
    lines
        .iter()
        .find(|line| !line.contains(':'))
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
}

fn first_meaningful_line(lines: &[String]) -> Option<String> {
    lines
        .iter()
        .map(|line| line.trim())
        .find(|line| {
            !line.is_empty()
                && !line.ends_with(':')
                && !line.eq_ignore_ascii_case("none")
                && !line.eq_ignore_ascii_case("kein")
                && !line.eq_ignore_ascii_case("keiner")
                && !line.eq_ignore_ascii_case("no blocker")
                && !line.eq_ignore_ascii_case("n/a")
                && !line.eq_ignore_ascii_case("na")
        })
        .map(ToOwned::to_owned)
}

fn canonicalize_mission_status(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let normalized = normalize_mission_text(raw);
    match normalized.as_str() {
        "done" | "complete" | "completed" | "closed" | "abgeschlossen" => Some("done".to_string()),
        "maintenance" => Some("maintenance".to_string()),
        "scheduled" => Some("scheduled".to_string()),
        "dormant" => Some("dormant".to_string()),
        "open" | "active" | "ongoing" | "in progress" => Some("active".to_string()),
        _ => None,
    }
}

fn canonicalize_continuation_mode(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let normalized = normalize_mission_text(raw);
    match normalized.as_str() {
        "maintenance" => Some("maintenance".to_string()),
        "scheduled" | "cron" => Some("scheduled".to_string()),
        "dormant" | "archive" => Some("dormant".to_string()),
        "closed" => Some("closed".to_string()),
        "continuous" | "continue" | "open" | "reopen" | "reopened" | "resume" | "active"
        | "ongoing" => Some("continuous".to_string()),
        _ => None,
    }
}

fn canonicalize_trigger_intensity(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let normalized = normalize_mission_text(raw);
    match normalized.as_str() {
        "archive" => Some("archive".to_string()),
        "cold" | "low" => Some("cold".to_string()),
        "warm" | "medium" | "moderate" => Some("warm".to_string()),
        "hot" | "high" | "urgent" => Some("hot".to_string()),
        _ => None,
    }
}

fn canonicalize_closure_confidence(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let normalized = normalize_mission_text(raw);
    match normalized.as_str() {
        "complete" | "completed" | "certain" => Some("complete".to_string()),
        "high" => Some("high".to_string()),
        "medium" | "moderate" => Some("medium".to_string()),
        "low" | "partial" | "provisional" | "tentative" | "pending" | "unverified" | "unclear"
        | "unknown" => Some("low".to_string()),
        _ => None,
    }
}

fn mission_is_open(
    mission: &str,
    mission_status: &str,
    continuation_mode: &str,
    next_slice: &str,
    done_gate: &str,
    closure_confidence: &str,
) -> bool {
    let status = normalize_mission_text(mission_status);
    let mode = normalize_mission_text(continuation_mode);
    let _ = closure_confidence;
    if status == "done" || mode == "closed" || mode == "dormant" {
        return false;
    }
    !mission.trim().is_empty() || !next_slice.trim().is_empty() || !done_gate.trim().is_empty()
}

fn mission_allows_idle(
    mission_status: &str,
    continuation_mode: &str,
    trigger_intensity: &str,
) -> bool {
    let status = normalize_mission_text(mission_status);
    let mode = normalize_mission_text(continuation_mode);
    let intensity = normalize_mission_text(trigger_intensity);
    status == "done"
        || mode == "closed"
        || mode == "dormant"
        || (mode == "scheduled" && intensity != "hot")
}

fn normalize_mission_text(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn continuity_template(kind: ContinuityKind) -> &'static str {
    match kind {
        ContinuityKind::Narrative => {
            "# CONTINUITY NARRATIVE\n\n## Situation\nsummary:\nstate:\n\n## Entries\nentry_id:\nevent_type:\nsummary:\nconsequence:\nsource_class:\nsource_ref:\nobserved_at:\n"
        }
        ContinuityKind::Anchors => {
            "# CONTINUITY ANCHORS\n\n## Entries\nanchor_id:\nanchor_type:\nstatement:\nsource_class:\nsource_ref:\nobserved_at:\nconfidence:\nsupersedes:\nexpires_at:\n"
        }
        ContinuityKind::Focus => {
            "# ACTIVE FOCUS\n\n## Status\nMission:\nMission state:\nContinuation mode:\nTrigger intensity:\n\n## Blocker\nCurrent blocker:\n\n## Next\nNext slice:\n\n## Done / Gate\nDone gate:\nRetry condition:\nClosure confidence:\n\n## Contract\nmission:\nmission_state:\ncontinuation_mode:\ntrigger_intensity:\nslice:\nslice_state:\n\n## State\ngoal:\nblocker:\nmissing_dependency:\nnext_slice:\ndone_gate:\nretry_condition:\nclosure_confidence:\n\n## Sources\nsource_refs:\nnone\nupdated_at:\n"
        }
    }
}

fn continuity_document_id(conversation_id: i64, kind: ContinuityKind) -> String {
    format!("contdoc_{}_{}", conversation_id, kind.as_str())
}

fn strategic_directive_id(
    conversation_id: i64,
    thread_key: Option<&str>,
    directive_kind: &str,
    revision: i64,
    created_at: &str,
) -> String {
    let mut hash = Sha256::new();
    hash.update(conversation_id.to_string().as_bytes());
    hash.update(thread_key.unwrap_or_default().as_bytes());
    hash.update(directive_kind.as_bytes());
    hash.update(revision.to_string().as_bytes());
    hash.update(created_at.as_bytes());
    let digest = hash.finalize();
    let prefix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sdir_{prefix}")
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
    for raw_line in diff_text.lines() {
        let line = raw_line.trim_end();
        let syntax_line = line.trim_start();
        if syntax_line.trim().is_empty() {
            continue;
        }
        if syntax_line.starts_with("## ") {
            current_section = Some(syntax_line.trim().to_string());
            normalized.push(syntax_line.trim().to_string());
            continue;
        }
        if current_section.is_none() {
            if let Some(stripped) = syntax_line
                .strip_prefix('+')
                .or_else(|| syntax_line.strip_prefix('-'))
                .map(str::trim)
            {
                if let Some(section) = infer_continuity_section(kind, stripped) {
                    current_section = Some(section.to_string());
                    normalized.push(section.to_string());
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
            "    printf '%s' \"<FULL NEW DOCUMENT BODY>\" | ctox continuity-update --kind {kind_str} --mode full"
        ),
        "  Use this when the current document is empty or its structure has to change substantially. \
         Keep section headers the same (`## Status`, `## Blocker`, ...). Write each field on its own line as `- field: value` or `field: value`.".to_string(),
        String::new(),
        "MODE B — single targeted string replacement (best for one-field updates):".to_string(),
        format!(
            "    ctox continuity-update --kind {kind_str} --mode replace --find '<OLD EXACT TEXT>' --replace '<NEW EXACT TEXT>'"
        ),
        "  `--find` must match exactly once in the document. A match of zero or >1 fails loudly. \
         Best for edits like changing `Mission state: open` to `Mission state: done`.".to_string(),
        String::new(),
        "MODE C — structured +/- diff (advanced; read from stdin):".to_string(),
        format!(
            "    printf '## Section\\n- old line\\n+ new line\\n' | ctox continuity-update --kind {kind_str} --mode diff"
        ),
        "  Use only when you have several coordinated changes across the same document.".to_string(),
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

fn estimate_tokens(content: &str) -> usize {
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
    hash.update(iso_now().as_bytes());
    let digest = hash.finalize();
    let prefix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sum_{prefix}")
}

fn iso_now() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0);
    millis.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let counter = TEMP_DB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        path.push(format!("ctox-lcm-{nanos}-{counter}.sqlite"));
        path
    }

    #[test]
    fn compacts_messages_and_supports_retrieval() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(
            &db_path,
            LcmConfig {
                context_threshold: 0.4,
                min_compaction_tokens: 0,
                fresh_tail_count: 2,
                leaf_chunk_tokens: 20,
                leaf_target_tokens: 120,
                condensed_target_tokens: 120,
                leaf_min_fanout: 3,
                condensed_min_fanout: 2,
                max_rounds: 4,
            },
        )?;

        for idx in 0..8 {
            engine.add_message(
                1,
                if idx % 2 == 0 { "user" } else { "assistant" },
                &format!("message {idx} about postgres migration planning and rollout details"),
            )?;
        }

        let result = engine.compact(1, 40, &HeuristicSummarizer, false)?;
        assert!(result.action_taken);
        assert!(!result.created_summary_ids.is_empty());

        let grep = engine.grep(Some(1), GrepScope::Both, GrepMode::FullText, "postgres", 10)?;
        assert!(grep.total_matches > 0);

        let described = engine.describe(&result.created_summary_ids[0])?;
        assert!(described.is_some());

        let expanded = engine.expand(&result.created_summary_ids[0], 1, true, 10_000)?;
        assert!(!expanded.messages.is_empty());

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn creates_condensed_summary_from_leaf_summaries() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(
            &db_path,
            LcmConfig {
                context_threshold: 0.2,
                min_compaction_tokens: 0,
                fresh_tail_count: 0,
                leaf_chunk_tokens: 60,
                leaf_target_tokens: 10,
                condensed_target_tokens: 10,
                leaf_min_fanout: 2,
                condensed_min_fanout: 2,
                max_rounds: 6,
            },
        )?;

        let leaf_a = engine.insert_summary(
            7,
            SummaryKind::Leaf,
            0,
            "leaf summary A with rollout evidence and retrieval details",
            0,
            0,
            24,
            &[],
            Vec::new(),
            0,
            Vec::new(),
        )?;
        let leaf_b = engine.insert_summary(
            7,
            SummaryKind::Leaf,
            0,
            "leaf summary B with fallback notes and verification details",
            0,
            0,
            26,
            &[],
            Vec::new(),
            1,
            Vec::new(),
        )?;

        let condensed_id = engine
            .compact_condensed_pass(7, &HeuristicSummarizer)?
            .context("expected condensed summary")?;
        let condensed = engine
            .get_summary(&condensed_id)?
            .context("missing condensed summary")?;

        assert_eq!(condensed.kind, SummaryKind::Condensed);
        assert_eq!(condensed.depth, 1);
        assert_eq!(condensed.source_message_token_count, 50);
        assert_eq!(condensed.descendant_count, 2);
        assert_eq!(
            engine.summary_parent_ids(&leaf_a)?,
            vec![condensed_id.clone()]
        );
        assert_eq!(engine.summary_parent_ids(&leaf_b)?, vec![condensed_id]);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn secret_rewrite_replaces_literals_across_memory_without_breaking_structure() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let literal = "sk-live-very-secret";
        let replacement = "[secret-ref:ticket/zammad/api-token]";

        engine.add_message(
            91,
            "user",
            &format!("Please use {literal} for the monitoring API"),
        )?;
        let ordinal = engine.next_context_ordinal(91)?;
        let _ = engine.insert_summary(
            91,
            SummaryKind::Leaf,
            0,
            &format!("Summary still mentions {literal} before hygiene."),
            0,
            0,
            0,
            &[],
            vec![],
            ordinal,
            vec![],
        )?;
        engine.continuity_apply_diff(
            91,
            ContinuityKind::Focus,
            &format!(
                "## Status\n+ Mission: stabilize monitoring with {literal}\n## Blocker\n+ Current blocker: waiting for {literal}\n"
            ),
        )?;

        let rewrite = engine.rewrite_secret_literal(
            91,
            "ticket/zammad",
            "api-token",
            literal,
            replacement,
        )?;
        assert!(rewrite.message_rows_updated >= 1);
        assert!(rewrite.summary_rows_updated >= 1);
        assert!(rewrite.continuity_commit_rows_updated >= 1);

        let snapshot = engine.snapshot(91)?;
        assert!(snapshot
            .messages
            .iter()
            .all(|item| !item.content.contains(literal)));
        assert!(snapshot
            .messages
            .iter()
            .any(|item| item.content.contains(replacement)));
        assert!(snapshot
            .summaries
            .iter()
            .all(|item| !item.content.contains(literal)));
        assert!(snapshot
            .summaries
            .iter()
            .any(|item| item.content.contains(replacement)));

        let continuity = engine.continuity_show_all(91)?;
        assert!(!continuity.focus.content.contains(literal));
        assert!(continuity.focus.content.contains(replacement));

        let mission = engine.mission_state(91)?;
        assert!(!mission.blocker.contains(literal));

        let grep_old = engine.grep(Some(91), GrepScope::Both, GrepMode::FullText, literal, 10)?;
        assert_eq!(grep_old.total_matches, 0);
        let grep_new = engine.grep(Some(91), GrepScope::Both, GrepMode::FullText, "api", 10)?;
        assert!(grep_new.total_matches > 0);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn new_session_starts_with_raw_continuity_templates() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;

        engine.add_message(9, "user", "First session message.")?;

        let current = engine
            .latest_continuity(9)?
            .context("expected continuity state")?;
        assert!(current.narrative.contains("# CONTINUITY NARRATIVE"));
        assert!(current.narrative.contains("## Situation"));
        assert!(current.anchors.contains("# CONTINUITY ANCHORS"));
        assert!(current.focus.contains("# ACTIVE FOCUS"));
        assert!(current.focus.contains("Mission:"));
        assert!(!current.narrative.contains("- "));

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn mission_state_tracks_focus_contract_and_preserves_watcher_metadata() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(13)?;
        engine.continuity_apply_diff(
            13,
            ContinuityKind::Focus,
            "## Status\n+ Mission: Build and operate the Airbnb clone.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: hot.\n## Blocker\n+ Current blocker: none.\n## Next\n+ Next slice: implement the host onboarding shell.\n## Done / Gate\n+ Done gate: never claim completion while the capability audit is still open.\n+ Closure confidence: low.\n",
        )?;

        let mission = engine.mission_state(13)?;
        assert_eq!(mission.mission, "Build and operate the Airbnb clone.");
        assert_eq!(mission.mission_status, "active");
        assert_eq!(mission.continuation_mode, "continuous");
        assert_eq!(mission.trigger_intensity, "hot");
        assert!(mission.is_open);
        assert!(!mission.allow_idle);

        let triggered = engine.note_mission_watcher_triggered(13, "2026-03-31T12:00:00Z")?;
        assert_eq!(triggered.watcher_trigger_count, 1);
        assert_eq!(
            triggered.watcher_last_triggered_at.as_deref(),
            Some("2026-03-31T12:00:00Z")
        );

        let synced = engine.sync_mission_state_from_continuity(13)?;
        assert_eq!(synced.watcher_trigger_count, 1);
        assert_eq!(
            synced.watcher_last_triggered_at.as_deref(),
            Some("2026-03-31T12:00:00Z")
        );

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn mission_state_normalizes_free_form_focus_controls_and_preserves_mission() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(17)?;
        engine.continuity_apply_diff(
            17,
            ContinuityKind::Focus,
            "## Status\n+ Mission: Keep the marketplace delivery mission durable.\n+ Mission state: Still open; roadmap and progress docs were updated, but the mission has not reached a stable stopping point.\n+ Continuation mode: Keep the discovery work attached to the marketplace core and continue the same durable slice.\n+ Trigger intensity: High while the mission remains open and idle-watch pressure is present.\n## Blocker\n+ Current blocker: Keep the discovery work attached to the marketplace core.\n## Next\n+ Next slice: Carry the discovery expectations through the roadmap slice.\n## Done / Gate\n+ Done gate: Preserve mission continuity while advancing durable slices.\n+ Closure confidence: Low until the marketplace core reaches a stable stopping point.\n",
        )?;
        let initial = engine.mission_state(17)?;
        assert_eq!(
            initial.mission,
            "Keep the marketplace delivery mission durable."
        );
        assert_eq!(initial.mission_status, "active");
        assert_eq!(initial.continuation_mode, "continuous");
        assert_eq!(initial.trigger_intensity, "hot");
        assert_eq!(initial.closure_confidence, "low");
        assert!(initial.is_open);
        assert!(!initial.allow_idle);

        engine.continuity_apply_diff(
            17,
            ContinuityKind::Focus,
            "## Status\n- Mission: Keep the marketplace delivery mission durable.\n## Next\n+ Next slice: Continue the same durable slice.\n",
        )?;
        let synced = engine.sync_mission_state_from_continuity(17)?;
        assert_eq!(
            synced.mission,
            "Keep the marketplace delivery mission durable."
        );
        assert_eq!(synced.continuation_mode, "continuous");
        assert_eq!(synced.trigger_intensity, "hot");

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn continuity_init_documents_keeps_mission_state_on_focus_head() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;

        let continuity = engine.continuity_init_documents(18)?;
        let mission = engine.mission_state(18)?;

        assert_eq!(
            mission.focus_head_commit_id,
            continuity.focus.head_commit_id
        );

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn continuity_apply_diff_updates_mission_state_to_new_focus_head() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(19)?;

        let updated = engine.continuity_apply_diff(
            19,
            ContinuityKind::Focus,
            "## Status\n+ Mission: Keep continuity crash-safe.\n+ Mission state: active.\n## Next\n+ Next slice: verify the focus head remains aligned.\n## Done / Gate\n+ Done gate: mission state must stay on the latest focus head.\n",
        )?;
        let mission = engine.mission_state(19)?;

        assert_eq!(mission.focus_head_commit_id, updated.head_commit_id);
        assert_eq!(mission.mission, "Keep continuity crash-safe.");

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn sync_mission_state_with_repair_canonicalizes_conflicting_focus_values() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(54)?;
        engine.continuity_apply_diff(
            54,
            ContinuityKind::Focus,
            "## Status\n+ Mission: Old continuity head before partial-commit recovery.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: warm.\n## Blocker\n+ Current blocker: the recovery path still points at the old continuity head.\n## Next\n+ Next slice: advance to the new continuity head.\n## Done / Gate\n+ Done gate: resync the live mission state to the newest continuity head.\n+ Closure confidence: low.\n",
        )?;
        engine.continuity_apply_diff(
            54,
            ContinuityKind::Focus,
            "## Status\n+ Mission: Keep the newest continuity head primary after partial-commit recovery.\n+ Trigger intensity: hot.\n## Blocker\n+ Current blocker: the live mission cache may still point at the old focus head.\n## Next\n+ Next slice: verify the newest focus head is the active runtime truth.\n## Done / Gate\n+ Done gate: keep the newest focus head primary and leave exactly one bounded continuation open.\n",
        )?;

        let before = engine.stored_continuity_show_all(54)?;
        assert!(!focus_semantic_conflicts_local(&before.focus.content).is_empty());

        let repair = engine.sync_mission_state_from_continuity_with_repair(54)?;
        let after = engine.stored_continuity_show_all(54)?;

        assert!(repair.focus_repaired);
        assert_eq!(
            repair.mission_state.mission,
            "Keep the newest continuity head primary after partial-commit recovery."
        );
        assert_eq!(repair.mission_state.trigger_intensity, "hot");
        assert!(focus_semantic_conflicts_local(&after.focus.content).is_empty());
        assert!(!after
            .focus
            .content
            .contains("Mission: Old continuity head before partial-commit recovery."));
        assert!(after.focus.content.contains(
            "Mission: Keep the newest continuity head primary after partial-commit recovery."
        ));

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn mission_state_does_not_close_when_done_gate_mentions_completed_slice() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(23)?;
        engine.continuity_apply_diff(
            23,
            ContinuityKind::Focus,
            "## Status\n+ Mission: Keep the marketplace core as the main thread, with discovery features folded into the same slice.\n+ Mission state: Slice 2 remains the main marketplace thread; a trust-and-safety response slice is now tracked alongside it for the suspicious host cluster.\n+ Continuation mode: Keep the main mission thread intact, advance Slice 2, and keep the trust-and-safety response slice as a contained response path.\n+ Trigger intensity: High until Slice 2 and the trust-and-safety response slice are both advanced and recorded.\n## Blocker\n+ Current blocker: Suspicious new host cluster with repeated near-duplicate listings and mismatched identity signals needs a concrete response without derailing the marketplace core.\n## Next\n+ Next slice: Advance the marketplace core slice with mobile-first search, map-based discovery, and saved-search support, while keeping the trust-and-safety response slice contained and tracked.\n## Done / Gate\n+ Done gate: Slice completed cleanly with continuity preserved, discovery kept inside the marketplace core thread, and the trust-and-safety response slice documented without displacing the main roadmap.\n+ Closure confidence: Low until Slice 2 and the trust-and-safety response path are both stable.\n",
        )?;

        let mission = engine.mission_state(23)?;
        assert_eq!(mission.mission_status, "active");
        assert_eq!(mission.continuation_mode, "continuous");
        assert_eq!(mission.trigger_intensity, "hot");
        assert_eq!(mission.closure_confidence, "low");
        assert!(mission.is_open);
        assert!(!mission.allow_idle);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn mission_state_ignores_empty_focus_template_placeholders() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(29)?;
        let continuity = engine.continuity_show_all(29)?;
        let previous = MissionStateRecord {
            conversation_id: 29,
            mission: String::new(),
            mission_status: "active".to_string(),
            continuation_mode: "continuous".to_string(),
            trigger_intensity: "hot".to_string(),
            blocker: "goal: stale placeholder".to_string(),
            next_slice: "goal: stale placeholder".to_string(),
            done_gate: String::new(),
            closure_confidence: "low".to_string(),
            is_open: true,
            allow_idle: false,
            focus_head_commit_id: continuity.focus.head_commit_id.clone(),
            last_synced_at: iso_now(),
            watcher_last_triggered_at: Some("2026-04-05T23:02:04Z".to_string()),
            watcher_trigger_count: 16,
            agent_failure_count: 0,
            deferred_reason: None,
            rewrite_failure_count: 0,
        };

        let mission = derive_mission_state_from_continuity(&continuity, Some(&previous));
        assert_eq!(mission.mission, "");
        assert_eq!(mission.blocker, "");
        assert_eq!(mission.next_slice, "");
        assert_eq!(mission.done_gate, "");
        assert!(!mission.is_open);
        assert_eq!(mission.watcher_trigger_count, 16);
        assert_eq!(
            mission.watcher_last_triggered_at.as_deref(),
            Some("2026-04-05T23:02:04Z")
        );

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn mission_state_accepts_open_and_partial_focus_values_without_falling_back() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(49)?;
        engine.continuity_apply_diff(
            49,
            ContinuityKind::Focus,
            "## Status\n+ Mission: Rehydrate Split Brain Gate (reconcile seeded contradiction; treat live runtime track as canonical).\n+ Mission state: in_progress.\n+ Continuation mode: open.\n+ Trigger intensity: cold.\n## Blocker\n+ Current blocker: Seeded focus marks closed, but live runtime has work.\n## Next\n+ Next slice: Persist exactly one open canonical continuation in runtime state.\n## Done / Gate\n+ Done gate: Keep the continuation open until runtime state is verified.\n+ Closure confidence: partial (until runtime state is verified).\n",
        )?;
        let continuity = engine.continuity_show_all(49)?;
        let previous = MissionStateRecord {
            conversation_id: 49,
            mission: "Legacy split-brain closure state.".to_string(),
            mission_status: "done".to_string(),
            continuation_mode: "closed".to_string(),
            trigger_intensity: "cold".to_string(),
            blocker: "continuity said closed".to_string(),
            next_slice: "none".to_string(),
            done_gate: "legacy closure".to_string(),
            closure_confidence: "complete".to_string(),
            is_open: false,
            allow_idle: true,
            focus_head_commit_id: continuity.focus.head_commit_id.clone(),
            last_synced_at: iso_now(),
            watcher_last_triggered_at: None,
            watcher_trigger_count: 0,
            agent_failure_count: 0,
            deferred_reason: None,
            rewrite_failure_count: 0,
        };

        let mission = derive_mission_state_from_continuity(&continuity, Some(&previous));
        assert_eq!(
            mission.mission,
            "Rehydrate Split Brain Gate (reconcile seeded contradiction; treat live runtime track as canonical)."
        );
        assert_eq!(mission.mission_status, "active");
        assert_eq!(mission.continuation_mode, "continuous");
        assert_eq!(mission.closure_confidence, "low");
        assert!(mission.is_open);
        assert!(!mission.allow_idle);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn mission_state_keeps_explicit_blank_focus_fields_blank() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(57)?;
        engine.continuity_apply_diff(
            57,
            ContinuityKind::Focus,
            "## Status\n+ Mission: Keep the restore follow-up open until queue pressure drops.\n+ Mission state: active.\n+ Continuation mode: continuous.\n+ Trigger intensity: hot.\n## Blocker\n+ Current blocker:\n## Next\n+ Next slice:\n## Done / Gate\n+ Done gate:\n+ Closure confidence: low.\n",
        )?;

        let mission = engine.mission_state(57)?;
        assert_eq!(mission.blocker, "");
        assert_eq!(mission.next_slice, "");
        assert_eq!(mission.done_gate, "");
        assert_eq!(mission.closure_confidence, "low");

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn verification_runs_and_open_claims_persist_in_lcm_db() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let created_at = iso_now();
        let run = VerificationRunRecord {
            run_id: verification_run_id(
                41,
                "queue",
                "Repair deployment",
                "Repair deployment",
                "Deployment still looked broken after the patch.",
                &created_at,
            ),
            conversation_id: 41,
            source_label: "queue".to_string(),
            goal: "Repair deployment".to_string(),
            preview: "Repair deployment".to_string(),
            result_excerpt: "Deployment still looked broken after the patch.".to_string(),
            blocker: None,
            review_required: true,
            review_verdict: "fail".to_string(),
            review_summary: "HTTP health check still returns 502.".to_string(),
            review_score: 4,
            review_reasons: vec![
                "closure_claim".to_string(),
                "runtime_or_infra_change".to_string(),
            ],
            report_excerpt: "VERDICT: FAIL".to_string(),
            raw_report: "VERDICT: FAIL\nMISSION_STATE: UNHEALTHY".to_string(),
            mission_state: "UNHEALTHY".to_string(),
            failed_gates: vec!["HTTP health check still returns 502.".to_string()],
            semantic_findings: vec!["Deployment is still unhealthy.".to_string()],
            open_items: vec!["Repair upstream health failure.".to_string()],
            evidence: vec!["curl /health => 502".to_string()],
            handoff: None,
            claim_count: 2,
            open_claim_count: 2,
            closure_blocking_claim_count: 2,
            created_at: created_at.clone(),
        };
        let claims = vec![
            MissionClaimRecord {
                claim_key: mission_claim_key(41, "operational_state", "Repair deployment"),
                conversation_id: 41,
                last_run_id: run.run_id.clone(),
                claim_kind: "operational_state".to_string(),
                claim_status: "needs_recheck".to_string(),
                blocks_closure: true,
                subject: "Repair deployment".to_string(),
                summary: "Operational state still needs live revalidation.".to_string(),
                evidence_summary: "Review FAIL: HTTP health check still returns 502.".to_string(),
                recheck_policy: "revalidate_live_state_before_close".to_string(),
                expires_at: None,
                created_at: created_at.clone(),
                updated_at: created_at.clone(),
            },
            MissionClaimRecord {
                claim_key: mission_claim_key(41, "completion_gate", "Repair deployment"),
                conversation_id: 41,
                last_run_id: run.run_id.clone(),
                claim_kind: "completion_gate".to_string(),
                claim_status: "needs_recheck".to_string(),
                blocks_closure: true,
                subject: "Repair deployment".to_string(),
                summary: "Completion gate must stay open.".to_string(),
                evidence_summary: "Review FAIL: HTTP health check still returns 502.".to_string(),
                recheck_policy: "keep_open_until_supporting_claims_verified".to_string(),
                expires_at: None,
                created_at: created_at.clone(),
                updated_at: created_at.clone(),
            },
        ];
        engine.persist_verification_run(&run, &claims)?;

        let latest = engine
            .latest_verification_run(41)?
            .context("expected latest verification run")?;
        assert_eq!(latest.run_id, run.run_id);
        assert_eq!(latest.open_claim_count, 2);

        let assurance = engine.mission_assurance_snapshot(41)?;
        assert_eq!(assurance.open_claims.len(), 2);
        assert_eq!(assurance.closure_blocking_claims.len(), 2);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn strategic_directives_are_versioned_and_activate_cleanly() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let active = engine.create_strategic_directive(
            99,
            Some("kunstmen-supervisor"),
            "mission",
            "Launch the platform",
            "Build a credible marketplace for hiring AI employees.",
            "active",
            "founder",
            Some("initial mission"),
        )?;
        let proposed = engine.create_strategic_directive(
            99,
            Some("kunstmen-supervisor"),
            "mission",
            "Tighten the launch scope",
            "Start with three hireable roles and a clear interview-to-hire path.",
            "proposed",
            "ctox",
            Some("scope refinement"),
        )?;
        let activated = engine.activate_strategic_directive(
            &proposed.directive_id,
            "founder",
            Some("approved refinement"),
        )?;
        let snapshot = engine.active_strategy_snapshot(99, Some("kunstmen-supervisor"))?;
        assert_eq!(
            snapshot
                .active_mission
                .as_ref()
                .map(|item| item.directive_id.clone()),
            Some(activated.directive_id.clone())
        );
        let history = engine.list_strategic_directives(
            99,
            Some("kunstmen-supervisor"),
            Some("mission"),
            10,
        )?;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].status, "active");
        let superseded = history
            .iter()
            .find(|item| item.directive_id == active.directive_id)
            .context("missing superseded mission revision")?;
        assert_eq!(superseded.status, "superseded");
        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn continuity_diff_documents_apply_and_track_forgotten_lines() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;

        let docs = engine.continuity_init_documents(11)?;
        assert!(docs.narrative.content.contains("## Entries"));

        let updated = engine.continuity_apply_diff(
            11,
            ContinuityKind::Narrative,
            "## Entries\n+ entry_id: rollout-break\n+ event_type: failure\n+ summary: Service started with a fragile migration plan.\n+ consequence: Cache warmer timing caused the breakage.\n+ source_class: tool_observed\n+ source_ref: log://deploy\n+ observed_at: 2026-04-02T10:00:00Z\n",
        )?;
        assert!(updated
            .content
            .contains("Service started with a fragile migration plan."));
        assert!(updated
            .content
            .contains("Cache warmer timing caused the breakage."));

        let updated_again = engine.continuity_apply_diff(
            11,
            ContinuityKind::Narrative,
            "## Entries\n- consequence: Cache warmer timing caused the breakage.\n+ consequence: Cache warmer timing after verification caused the breakage.\n",
        )?;
        assert!(updated_again
            .content
            .contains("Cache warmer timing after verification caused the breakage."));
        assert!(!updated_again
            .content
            .contains("Cache warmer timing caused the breakage."));

        let forgotten = engine.continuity_forgotten(
            11,
            Some(ContinuityKind::Narrative),
            Some("Cache warmer"),
        )?;
        assert_eq!(forgotten.len(), 1);
        assert!(forgotten[0]
            .line
            .contains("Cache warmer timing caused the breakage."));

        let rebuilt = engine.continuity_rebuild(11, ContinuityKind::Narrative)?;
        assert_eq!(rebuilt.content, updated_again.content);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn continuity_apply_diff_accepts_headerless_anchor_entries() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(46)?;

        let updated = engine.continuity_apply_diff(
            46,
            ContinuityKind::Anchors,
            "+ anchor_id: ANCHOR_MAIN_GATEWAY\n+ anchor_type: invariant\n+ statement: Keep the gateway mission primary.\n+ source_class: assistant_reply\n",
        )?;

        assert!(updated.content.contains("ANCHOR_MAIN_GATEWAY"));
        assert!(updated
            .content
            .contains("Keep the gateway mission primary."));

        let rebuilt = engine.continuity_rebuild(46, ContinuityKind::Anchors)?;
        assert_eq!(rebuilt.content, updated.content);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn continuity_apply_diff_routes_headerless_focus_fields_to_known_sections() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(47)?;

        let updated = engine.continuity_apply_diff(
            47,
            ContinuityKind::Focus,
            "+ Mission: Keep gateway intake hardening as the main mission.\n+ Mission state: active.\n+ Next slice: record the interrupt buffer without changing the main mission.\n+ Done gate: leave exactly one bounded continuation open.\n+ mission: keep gateway intake hardening primary\n+ next_slice: record interrupt buffer and return to the main thread\n",
        )?;

        assert!(updated
            .content
            .contains("Mission: Keep gateway intake hardening as the main mission."));
        assert!(updated.content.contains(
            "Next slice: record the interrupt buffer without changing the main mission."
        ));
        assert!(updated
            .content
            .contains("mission: keep gateway intake hardening primary"));
        assert!(updated
            .content
            .contains("next_slice: record interrupt buffer and return to the main thread"));

        let mission = engine.mission_state(47)?;
        assert_eq!(
            mission.mission,
            "Keep gateway intake hardening as the main mission."
        );
        assert_eq!(mission.mission_status, "active");
        assert!(mission.is_open);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn continuity_apply_diff_accepts_indented_focus_diff_lines() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(48)?;

        let updated = engine.continuity_apply_diff(
            48,
            ContinuityKind::Focus,
            "  + Mission: Keep gateway intake hardening as the main mission.\n  + Mission state: active.\n  + Next slice: record the interrupt buffer without changing the main mission.\n  + Done gate: leave exactly one bounded continuation open.\n  + mission: keep gateway intake hardening primary\n  - none\n",
        )?;

        assert!(updated
            .content
            .contains("Mission: Keep gateway intake hardening as the main mission."));
        assert!(updated.content.contains(
            "Next slice: record the interrupt buffer without changing the main mission."
        ));

        let mission = engine.mission_state(48)?;
        assert_eq!(
            mission.mission,
            "Keep gateway intake hardening as the main mission."
        );
        assert_eq!(mission.mission_status, "active");
        assert!(mission.is_open);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn continuity_prompt_contains_document_and_diff_rules() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(12)?;
        engine.add_message(
            12,
            "user",
            "Keep the rollout gate active until validation passes on db-prod.internal.",
        )?;

        let payload = engine.continuity_build_prompt(12, ContinuityKind::Narrative)?;
        assert!(payload
            .prompt
            .contains("Reply with only a diff that uses the existing sections."));
        assert!(payload.prompt.contains("<CURRENT_DOCUMENT>"));
        assert!(payload.prompt.contains("<RECENT_MESSAGES>"));
        assert!(payload.prompt.contains("## Entries"));
        assert!(payload
            .prompt
            .contains("The first non-empty diff line must be a `## ...` section header"));
        assert!(payload.prompt.contains("Example valid diff:"));

        let focus_payload = engine.continuity_build_prompt(12, ContinuityKind::Focus)?;
        assert!(focus_payload.prompt.contains("mission_state:"));
        assert!(focus_payload.prompt.contains("continuation_mode:"));
        assert!(focus_payload.prompt.contains("next_slice:"));
        assert!(focus_payload
            .prompt
            .contains("update both `## Status` and `## Contract`/`## State`"));
        assert!(focus_payload
            .prompt
            .contains("Do not keep stale closed fields"));
        assert!(focus_payload
            .prompt
            .contains("+ Continuation mode: continuous"));

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn focus_continuity_prompt_keeps_open_continuation_signal_from_long_assistant_reply(
    ) -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(13)?;
        engine.add_message(
            13,
            "assistant",
            "Mission\n\nRehydrated the partial-commit state to the newest durable focus head and kept that head as the active truth.\n\nCompleted\n\n- Verified continuity focus head is current.\n- Verified the old head is no longer authoritative.\n- Created both required workspace artifacts.\n- Verified mission-state focus_head_commit_id is resynced to the newest head.\n- Left exactly 1 open CTOX runtime item: active plan `partial commit resync: verify restart stays on new head`.\n- Verified runtime open work counts: `ctox plan list` = 1, `ctox queue list` = 0.\n\nArtifacts\n\n- `docs/partial-commit-recovery.md`\n- `ops/progress/progress-latest.md`\n\nNext\n\n- Open bounded continuation: `partial commit resync: verify restart stays on new head`.",
        )?;

        let focus_payload = engine.continuity_build_prompt(13, ContinuityKind::Focus)?;
        assert!(focus_payload
            .prompt
            .contains("Left exactly 1 open CTOX runtime item"));
        assert!(focus_payload
            .prompt
            .contains("partial commit resync: verify restart stays on new head"));

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn continuity_anchor_prompt_preserves_explicit_literals() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(44)?;
        engine.add_message(
            44,
            "assistant",
            "Retained the same three anchors:\n- `ANCHOR_REDWOOD`\n- `ANCHOR_GLASS_BRIDGE`\n- `ANCHOR_QUEUE_LANTERN`\nAlso kept `BENCH_CORE_CONTINUITY`.",
        )?;

        let payload = engine.continuity_build_prompt(44, ContinuityKind::Anchors)?;
        assert!(payload.prompt.contains("<EXPLICIT_ANCHOR_LITERALS>"));
        assert!(payload.prompt.contains("ANCHOR_REDWOOD"));
        assert!(payload.prompt.contains("ANCHOR_GLASS_BRIDGE"));
        assert!(payload.prompt.contains("ANCHOR_QUEUE_LANTERN"));
        assert!(payload.prompt.contains("BENCH_CORE_CONTINUITY"));

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn continuity_preserve_recent_anchor_literals_adds_missing_tokens() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(45)?;
        engine.add_message(
            45,
            "assistant",
            "Retained the same three anchors without change:\n- `ANCHOR_REDWOOD`\n- `ANCHOR_GLASS_BRIDGE`\n- `ANCHOR_QUEUE_LANTERN`",
        )?;

        let updated = engine
            .continuity_preserve_recent_anchor_literals(45)?
            .context("expected literal preservation diff")?;
        assert!(updated.content.contains("ANCHOR_REDWOOD"));
        assert!(updated.content.contains("ANCHOR_GLASS_BRIDGE"));
        assert!(updated.content.contains("ANCHOR_QUEUE_LANTERN"));

        let repeated = engine.continuity_preserve_recent_anchor_literals(45)?;
        assert!(repeated.is_none());

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn sentence_fragment_handles_multibyte_unicode_boundaries() {
        let content = "### Mission Diagnose the real bug with “smart quotes” intact.";
        let fragment = sentence_fragment(content, 42);
        assert!(fragment.ends_with("..."));
        assert!(fragment.contains('“'));
        assert!(std::str::from_utf8(fragment.as_bytes()).is_ok());
    }

    #[test]
    fn deterministic_fallback_handles_multibyte_unicode_boundaries() {
        let content = "é".repeat(FALLBACK_MAX_CHARS + 8);
        let fallback = build_deterministic_fallback(&content, 1234);
        assert!(fallback.contains("[Truncated from 1234 tokens]"));
        assert!(std::str::from_utf8(fallback.as_bytes()).is_ok());
    }

    // F3: structured agent_outcome round-trips on assistant rows and is
    // ignored on non-assistant rows.
    #[test]
    fn add_message_with_outcome_persists_for_assistant_only() -> Result<()> {
        let db_path = temp_db();
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;
        let _ = engine.continuity_init_documents(7)?;

        // user row: outcome must be ignored.
        let user_record =
            engine.add_message_with_outcome(7, "user", "ping", Some(AgentOutcome::TurnTimeout))?;
        assert!(user_record.agent_outcome.is_none());

        // assistant success row.
        let success_record = engine.add_message_with_outcome(
            7,
            "assistant",
            "all done",
            Some(AgentOutcome::Success),
        )?;
        assert_eq!(success_record.agent_outcome.as_deref(), Some("Success"));
        assert_eq!(engine.last_agent_outcome(7)?, Some(AgentOutcome::Success));

        // assistant timeout row supersedes the success.
        let _ = engine.add_message_with_outcome(
            7,
            "assistant",
            "(agent turn did not complete)",
            Some(AgentOutcome::TurnTimeout),
        )?;
        assert_eq!(
            engine.last_agent_outcome(7)?,
            Some(AgentOutcome::TurnTimeout)
        );

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn agent_outcome_token_round_trips() {
        for outcome in [
            AgentOutcome::Success,
            AgentOutcome::TurnTimeout,
            AgentOutcome::ExecutionError,
            AgentOutcome::Aborted,
            AgentOutcome::Cancelled,
        ] {
            let token = outcome.as_str();
            assert_eq!(AgentOutcome::from_token(token), Some(outcome));
        }
        assert!(AgentOutcome::from_token("unknown").is_none());
    }

    #[test]
    fn agent_outcome_failure_predicate_only_excludes_success() {
        assert!(!AgentOutcome::Success.is_agent_failure());
        assert!(AgentOutcome::TurnTimeout.is_agent_failure());
        assert!(AgentOutcome::ExecutionError.is_agent_failure());
        assert!(AgentOutcome::Aborted.is_agent_failure());
        assert!(AgentOutcome::Cancelled.is_agent_failure());
    }

    /// P2 — clobber guard: a watchdog write that tries to clear
    /// `done_gate` while the prior row carried a non-empty `done_gate`
    /// must be silently downgraded to a no-op for that field, the prior
    /// value preserved, and the attempt audited as a governance event.
    /// (The reviewer-rework loop in production saw `next_slice` /
    /// `done_gate` collapse to length 0 within ~25 minutes; this guard
    /// is the structural fix.)
    #[test]
    fn mission_state_done_gate_clobber_is_blocked_and_audited() -> Result<()> {
        // Test root layout: runtime/ctox.sqlite3 is the shared DB used
        // by both LcmEngine and governance::record_event.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let counter = TEMP_DB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("ctox-clobber-guard-{nanos}-{counter}"));
        std::fs::create_dir_all(root.join("runtime"))?;
        let db_path = root.join("runtime/ctox.sqlite3");
        let engine = LcmEngine::open(&db_path, LcmConfig::default())?;

        // Drain anything other tests may have leaked onto this thread.
        let _ = drain_pending_mission_state_clobbers_for_test();

        // Seed an existing mission_states row with non-empty done_gate.
        let baseline = MissionStateRecord {
            conversation_id: 7,
            mission: "Founder mail covering vision and mission".to_string(),
            mission_status: "active".to_string(),
            continuation_mode: "continuous".to_string(),
            trigger_intensity: "hot".to_string(),
            blocker: "operator-set blocker".to_string(),
            next_slice: "wait for reviewer disposition before sending".to_string(),
            done_gate: "X".to_string(),
            closure_confidence: "low".to_string(),
            is_open: true,
            allow_idle: false,
            focus_head_commit_id: "focus-clobber".to_string(),
            last_synced_at: iso_now(),
            watcher_last_triggered_at: None,
            watcher_trigger_count: 0,
            agent_failure_count: 0,
            deferred_reason: None,
            rewrite_failure_count: 0,
        };
        engine.overwrite_mission_state(&baseline)?;
        let after_seed = engine
            .stored_mission_state(7)?
            .expect("seeded mission state should be visible");
        assert_eq!(after_seed.done_gate, "X");
        assert_eq!(
            after_seed.next_slice,
            "wait for reviewer disposition before sending"
        );
        // Drain the buffer caused by the seed write itself (none expected,
        // but keep the test deterministic).
        let _ = drain_pending_mission_state_clobbers_for_test();

        // Watchdog-shaped write: same row, but `done_gate` empty and
        // `next_slice` empty. The guard must preserve both prior
        // non-empty values.
        let watchdog_write = MissionStateRecord {
            done_gate: String::new(),
            next_slice: String::new(),
            mission: "Founder mail covering vision and mission updated".to_string(),
            ..baseline.clone()
        };
        engine.overwrite_mission_state(&watchdog_write)?;

        let after_watchdog = engine
            .stored_mission_state(7)?
            .expect("mission state still present after blocked clobber");
        assert_eq!(
            after_watchdog.done_gate, "X",
            "guard must preserve the prior non-empty done_gate"
        );
        assert_eq!(
            after_watchdog.next_slice, "wait for reviewer disposition before sending",
            "guard must preserve the prior non-empty next_slice"
        );
        // Other fields keep their existing semantics: `mission` was
        // overwritten exactly as the writer requested.
        assert_eq!(
            after_watchdog.mission, "Founder mail covering vision and mission updated",
            "guard must not interfere with non-protected fields"
        );

        // Flush the suppressed clobber attempts to governance and verify
        // the audit event landed.
        engine.drain_pending_mission_state_clobber_events_to_governance(&root);
        let events = crate::governance::list_recent_events(&root, 7, 16)
            .expect("failed to list governance events");
        let clobber_events: Vec<_> = events
            .iter()
            .filter(|event| event.mechanism_id == "mission_state_field_clobbered_blocked")
            .collect();
        assert_eq!(
            clobber_events.len(),
            2,
            "expected exactly two clobber-blocked events (next_slice, done_gate); got {clobber_events:?}",
        );
        let blocked_fields: std::collections::BTreeSet<String> = clobber_events
            .iter()
            .filter_map(|event| {
                event
                    .details
                    .get("field")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
            })
            .collect();
        assert!(blocked_fields.contains("done_gate"));
        assert!(blocked_fields.contains("next_slice"));
        for event in &clobber_events {
            assert_eq!(event.severity, "warning");
            assert_eq!(event.action_taken, "preserved_prior_non_empty_field");
        }

        // Replacement with NEW non-empty content must succeed (the
        // ratchet allows replace, only blocks silent clear).
        let replacement = MissionStateRecord {
            done_gate: "fresh non-empty done gate".to_string(),
            next_slice: "fresh non-empty next slice".to_string(),
            ..baseline.clone()
        };
        engine.overwrite_mission_state(&replacement)?;
        let after_replace = engine.stored_mission_state(7)?.unwrap();
        assert_eq!(after_replace.done_gate, "fresh non-empty done gate");
        assert_eq!(after_replace.next_slice, "fresh non-empty next slice");

        // Owner-intent clear bypasses the guard.
        let cleared = engine.clear_mission_state_done_fields_with_owner_intent(7, true, true)?;
        assert!(cleared.next_slice.is_empty());
        assert!(cleared.done_gate.is_empty());
        let after_owner_clear = engine.stored_mission_state(7)?.unwrap();
        assert!(after_owner_clear.next_slice.is_empty());
        assert!(after_owner_clear.done_gate.is_empty());

        let _ = std::fs::remove_dir_all(root);
        Ok(())
    }
}
