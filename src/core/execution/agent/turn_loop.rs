use anyhow::Context;
use anyhow::Result;
use sha2::Digest;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

// Re-export PersistentSession so callers (main.rs, service.rs) can hold one.
pub(crate) use super::direct_session::PersistentSession;
use std::sync::OnceLock;
use std::time::Duration;
use toml::Value as TomlValue;

use crate::inference::supervisor;

/// Process-local display telemetry only. Refresh control flow is owned by the
/// durable `continuity_refresh_status` rows below.
#[derive(Default, Clone)]
struct RefreshTelemetry {
    /// Cumulative assistant reply characters since the last refresh.
    /// This is telemetry only. It must never be converted into token counts
    /// for control flow; compaction decisions use reported TokenCount events.
    output_chars_since_refresh: u64,
    /// Turns observed by this process since the last durable refresh.
    turns_since_refresh: u64,
}

fn turn_counters() -> &'static Mutex<HashMap<i64, RefreshTelemetry>> {
    static COUNTERS: OnceLock<Mutex<HashMap<i64, RefreshTelemetry>>> = OnceLock::new();
    COUNTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ChatTurnSessionOptions {
    pub(crate) disable_mcp_servers: bool,
    pub(crate) base_instructions: Option<String>,
    pub(crate) plain_prompt: bool,
    pub(crate) turn_timeout_secs_override: Option<u64>,
}

struct ToolFreeSemanticSummarizer {
    session: Mutex<PersistentSession>,
}

impl ToolFreeSemanticSummarizer {
    fn start(root: &Path, settings: &BTreeMap<String, String>) -> Result<Self> {
        let session = PersistentSession::start_with_instructions(
            root,
            settings,
            Some(
                "You are a semantic continuity summarizer. Use no tools. Preserve decisions, constraints, identifiers, unresolved work, evidence references, and changes of state. Omit conversational filler and do not invent facts. Return only the compact summary.",
            ),
            true,
        )?;
        Ok(Self {
            session: Mutex::new(session),
        })
    }
}

impl lcm::Summarizer for ToolFreeSemanticSummarizer {
    fn summarize(
        &self,
        kind: lcm::SummaryKind,
        depth: i64,
        lines: &[String],
        target_tokens: usize,
    ) -> Result<String> {
        let source = lines.join("\n");
        let prompt = format!(
            "Create a semantic {:?} summary at depth {} in at most {} tokens. Preserve durable facts and explicit omissions.\n\nSOURCE:\n{}",
            kind, depth, target_tokens, source
        );
        let result = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("semantic summarizer session lock poisoned"))?
            .run_turn(&prompt, Some(Duration::from_secs(90)), None, Some(false), 0);
        match result {
            Ok(summary) if !summary.trim().is_empty() => Ok(summary.trim().to_string()),
            Ok(_) | Err(_) => <lcm::HeuristicSummarizer as lcm::Summarizer>::summarize(
                &lcm::HeuristicSummarizer,
                kind,
                depth,
                lines,
                target_tokens,
            ),
        }
    }
}

fn compact_with_semantic_summarizer(
    root: &Path,
    settings: &BTreeMap<String, String>,
    engine: &lcm::LcmEngine,
    conversation_id: i64,
    max_context_tokens: i64,
    force: bool,
) -> Result<lcm::CompactionResult> {
    match ToolFreeSemanticSummarizer::start(root, settings) {
        Ok(summarizer) => engine.compact(conversation_id, max_context_tokens, &summarizer, force),
        Err(_) => engine.compact(
            conversation_id,
            max_context_tokens,
            &lcm::HeuristicSummarizer,
            force,
        ),
    }
}

fn record_refresh_telemetry(conversation_id: i64, reply_output_chars: u64, refresh_due: bool) {
    let mut counters = turn_counters().lock().expect("turn_counters poisoned");
    let state = counters
        .entry(conversation_id)
        .or_insert(RefreshTelemetry::default());
    state.output_chars_since_refresh = state
        .output_chars_since_refresh
        .saturating_add(reply_output_chars);
    state.turns_since_refresh = state.turns_since_refresh.saturating_add(1);

    if refresh_due {
        state.output_chars_since_refresh = 0;
        state.turns_since_refresh = 0;
    }
}

/// Current wall-clock time as an RFC3339 string, matching the format used
/// by `now_iso_string()` in the ticket / plan / continuity subsystems.
/// Used to bracket a turn so we can detect state writes that happened
/// during it.
fn current_rfc3339_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Snapshot of continuity-refresh accounting for display in the TUI.
#[derive(Debug, Clone, Copy)]
pub struct RefreshBudgetSnapshot {
    pub output_chars_since_refresh: u64,
    pub turns_since_refresh: u64,
}

/// Read-only accessor so the TUI can surface live budget telemetry without
/// mutating the per-conversation counters.
pub fn refresh_budget_snapshot(conversation_id: i64) -> RefreshBudgetSnapshot {
    let counters = turn_counters().lock().expect("turn_counters poisoned");
    let state = counters.get(&conversation_id).cloned().unwrap_or_default();
    RefreshBudgetSnapshot {
        output_chars_since_refresh: state.output_chars_since_refresh,
        turns_since_refresh: state.turns_since_refresh,
    }
}

const COMMUNICATION_REFRESH_TURN_LIMIT: i64 = 8;

fn open_turn_state_connection(db_path: &Path) -> Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(db_path)?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("configure turn-state SQLite busy_timeout")?;
    Ok(conn)
}

fn record_durable_refresh_demand(
    db_path: &Path,
    conversation_id: i64,
    force_boundary: bool,
    legacy_every_n_turns: u64,
    reply_output_chars: u64,
    source_ref: &str,
) -> Result<HashSet<String>> {
    let mut conn = open_turn_state_connection(db_path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS continuity_refresh_status (
            conversation_id INTEGER NOT NULL,
            continuity_kind TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'idle',
            successful_turn_count INTEGER NOT NULL DEFAULT 0,
            output_chars_since_refresh INTEGER NOT NULL DEFAULT 0,
            trigger_source_ref TEXT,
            observed_head_commit_id TEXT,
            consumed_head_commit_id TEXT,
            failure_attempt_count INTEGER NOT NULL DEFAULT 0,
            retry_not_before TEXT,
            last_error TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY(conversation_id, continuity_kind)
        );
        CREATE INDEX IF NOT EXISTS idx_continuity_refresh_due
            ON continuity_refresh_status(status, retry_not_before, updated_at);
        "#,
    )?;
    let has_output_chars: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('continuity_refresh_status') WHERE name='output_chars_since_refresh')",
        [],
        |row| row.get(0),
    )?;
    if has_output_chars == 0 {
        conn.execute_batch(
            "ALTER TABLE continuity_refresh_status ADD COLUMN output_chars_since_refresh INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let now = current_rfc3339_timestamp();
    for kind in ["narrative", "anchors", "focus"] {
        tx.execute(
            r#"
            INSERT INTO continuity_refresh_status (
                conversation_id, continuity_kind, status, successful_turn_count,
                output_chars_since_refresh, trigger_source_ref,
                observed_head_commit_id, consumed_head_commit_id,
                failure_attempt_count, retry_not_before, last_error, created_at, updated_at
            ) VALUES (?1, ?2, 'idle', 0, 0, NULL, NULL, NULL, 0, NULL, NULL, ?3, ?3)
            ON CONFLICT(conversation_id, continuity_kind) DO NOTHING
            "#,
            rusqlite::params![conversation_id, kind, now],
        )?;
        let previous_turns: i64 = tx.query_row(
            "SELECT successful_turn_count FROM continuity_refresh_status WHERE conversation_id=?1 AND continuity_kind=?2",
            rusqlite::params![conversation_id, kind],
            |row| row.get(0),
        )?;
        let turns = previous_turns.saturating_add(1);
        let previous_output_chars: i64 = tx.query_row(
            "SELECT output_chars_since_refresh FROM continuity_refresh_status WHERE conversation_id=?1 AND continuity_kind=?2",
            rusqlite::params![conversation_id, kind],
            |row| row.get(0),
        )?;
        let output_chars = previous_output_chars
            .saturating_add(i64::try_from(reply_output_chars).unwrap_or(i64::MAX));
        let interval_boundary = legacy_every_n_turns > 0
            && u64::try_from(turns).unwrap_or(u64::MAX) >= legacy_every_n_turns;
        let communication_boundary =
            matches!(kind, "narrative" | "anchors") && turns >= COMMUNICATION_REFRESH_TURN_LIMIT;
        let pending = force_boundary || interval_boundary || communication_boundary;
        tx.execute(
            r#"
            UPDATE continuity_refresh_status
            SET successful_turn_count=?3,
                output_chars_since_refresh=?4,
                status=CASE WHEN ?5=1 THEN 'pending' ELSE status END,
                trigger_source_ref=CASE WHEN ?5=1 THEN ?6 ELSE trigger_source_ref END,
                retry_not_before=CASE WHEN ?5=1 AND status!='pending' THEN NULL ELSE retry_not_before END,
                updated_at=?7
            WHERE conversation_id=?1 AND continuity_kind=?2
            "#,
            rusqlite::params![
                conversation_id,
                kind,
                turns,
                output_chars,
                if pending { 1 } else { 0 },
                source_ref,
                now,
            ],
        )?;
    }
    let due = due_refresh_kinds(&tx, conversation_id, &now)?;
    tx.commit()?;
    Ok(due)
}

fn due_refresh_kinds(
    conn: &rusqlite::Connection,
    conversation_id: i64,
    now: &str,
) -> Result<HashSet<String>> {
    let mut statement = conn.prepare(
        r#"
        SELECT continuity_kind
        FROM continuity_refresh_status
        WHERE conversation_id=?1
          AND status='pending'
          AND (retry_not_before IS NULL OR retry_not_before='' OR retry_not_before<=?2)
        "#,
    )?;
    let rows = statement.query_map(rusqlite::params![conversation_id, now], |row| {
        row.get::<_, String>(0)
    })?;
    rows.collect::<rusqlite::Result<HashSet<_>>>()
        .map_err(anyhow::Error::from)
}

fn mark_durable_refresh_consumed(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
    head_before: &str,
    head_after: &str,
) -> Result<()> {
    anyhow::ensure!(
        !head_after.is_empty() && head_after != head_before,
        "continuity head did not advance"
    );
    let conn = open_turn_state_connection(db_path)?;
    conn.execute(
        r#"
        UPDATE continuity_refresh_status
        SET status='consumed', successful_turn_count=0,
            output_chars_since_refresh=0,
            observed_head_commit_id=?3, consumed_head_commit_id=?4,
            failure_attempt_count=0, retry_not_before=NULL, last_error=NULL,
            updated_at=?5
        WHERE conversation_id=?1 AND continuity_kind=?2
        "#,
        rusqlite::params![
            conversation_id,
            kind,
            head_before,
            head_after,
            current_rfc3339_timestamp()
        ],
    )?;
    Ok(())
}

fn mark_durable_refresh_failed(
    db_path: &Path,
    conversation_id: i64,
    kind: &str,
    error: &str,
) -> Result<()> {
    let mut conn = open_turn_state_connection(db_path)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let previous: i64 = tx.query_row(
        "SELECT failure_attempt_count FROM continuity_refresh_status WHERE conversation_id=?1 AND continuity_kind=?2",
        rusqlite::params![conversation_id, kind],
        |row| row.get(0),
    )?;
    let attempt = previous.saturating_add(1);
    let exponent = u32::try_from(attempt.saturating_sub(1))
        .unwrap_or(16)
        .min(16);
    let delay = 300_i64
        .saturating_mul(2_i64.saturating_pow(exponent))
        .min(3_600);
    let retry_not_before = (chrono::Utc::now() + chrono::Duration::seconds(delay)).to_rfc3339();
    tx.execute(
        r#"
        UPDATE continuity_refresh_status
        SET status='pending', failure_attempt_count=?3, retry_not_before=?4,
            last_error=?5, updated_at=?6
        WHERE conversation_id=?1 AND continuity_kind=?2
        "#,
        rusqlite::params![
            conversation_id,
            kind,
            attempt,
            retry_not_before,
            error.chars().take(900).collect::<String>(),
            current_rfc3339_timestamp()
        ],
    )?;
    tx.commit()?;
    Ok(())
}

/// Start of the boundary-detection window for this conversation: the moment
/// of the last continuity refresh, or this turn's start on first contact.
/// Service-side internal work closures land between turns; a turn-local bracket
/// never sees them.
fn durable_boundary_window_start(
    db_path: &Path,
    conversation_id: i64,
    turn_start_ts: &str,
) -> String {
    let Ok(conn) = rusqlite::Connection::open(db_path) else {
        return turn_start_ts.to_string();
    };
    let table_exists = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='continuity_refresh_status')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        != 0;
    if !table_exists {
        return turn_start_ts.to_string();
    }
    conn.query_row(
        "SELECT MAX(updated_at) FROM continuity_refresh_status WHERE conversation_id=?1",
        rusqlite::params![conversation_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| turn_start_ts.to_string())
}

/// Query the mission and LCM databases for durable state changes written
/// between `turn_start_ts` and now. Returns `true` if any of the following
/// happened during the turn:
///
/// - an internal work item transitioned to `state = 'closed'`
/// - a new ticket-knowledge entry was inserted
/// - a focus continuity commit was written
///
/// Any error (missing DB, missing table on a fresh install) is swallowed
/// as `Ok(false)` by the caller. The explicit interval trigger can still be
/// enabled by operators; token-window safety is handled by actual TokenCount
/// telemetry in the compact policy.
/// Outcome of probing for a durable state transition. `detected` preserves the
/// historical default-false semantics; `probe_errors` distinguishes a probe
/// that ERRORED (e.g. a schema regression renamed a column) from one that
/// genuinely found no boundary, so a query regression is surfaced instead of
/// silently suppressing continuity refreshes.
#[derive(Debug, Default)]
struct DurableStateProbe {
    detected: bool,
    probe_errors: Vec<String>,
}

fn detect_durable_state_transition(
    root: &Path,
    lcm_db_path: &Path,
    conversation_id: i64,
    turn_start_ts: &str,
    boundary_window_ts: &str,
) -> Result<DurableStateProbe> {
    use rusqlite::Connection;

    let mut probe = DurableStateProbe::default();

    // Mission-side tables live in the unified CTOX runtime database. These
    // use the boundary WINDOW (since the last refresh), not the turn
    // bracket: the service closes internal work items after the turn, post
    // completion review, and a turn-local bracket never sees those.
    let mission_db = crate::persistence::sqlite_path(root);
    if mission_db.exists() {
        let conn = Connection::open_with_flags(
            &mission_db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )?;
        let self_work_closed: i64 = match conn.query_row(
            "SELECT COUNT(1) FROM ticket_self_work_items \
             WHERE state = 'closed' AND updated_at > ?1",
            rusqlite::params![boundary_window_ts],
            |row| row.get(0),
        ) {
            Ok(count) => count,
            Err(err) => {
                probe
                    .probe_errors
                    .push(format!("ticket_self_work_items probe failed: {err}"));
                0
            }
        };
        if self_work_closed > 0 {
            probe.detected = true;
            return Ok(probe);
        }
        let knowledge_added: i64 = match conn.query_row(
            "SELECT COUNT(1) FROM ticket_knowledge_entries WHERE created_at > ?1",
            rusqlite::params![boundary_window_ts],
            |row| row.get(0),
        ) {
            Ok(count) => count,
            Err(err) => {
                probe
                    .probe_errors
                    .push(format!("ticket_knowledge_entries probe failed: {err}"));
                0
            }
        };
        if knowledge_added > 0 {
            probe.detected = true;
            return Ok(probe);
        }
    }

    // Focus-document commits live in the LCM database alongside Narrative
    // and Anchors. A focus replacement during the turn is a boundary.
    // LCM stamps `continuity_commits.created_at` as an epoch-millis string
    // (lcm::iso_now), NOT RFC3339 — compare numerically, otherwise the text
    // collation ("17..." vs "20...") makes this check permanently false.
    if lcm_db_path.exists() {
        let turn_start_millis = chrono::DateTime::parse_from_rfc3339(turn_start_ts)
            .map(|ts| ts.timestamp_millis())
            .unwrap_or(i64::MAX);
        let conn = Connection::open_with_flags(
            lcm_db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )?;
        // `continuity_documents.kind` is stored lowercase ("focus", see
        // ContinuityKind::as_str) and its primary key is `document_id`.
        let focus_commits: i64 = match conn.query_row(
            "SELECT COUNT(1) FROM continuity_commits c \
             JOIN continuity_documents d ON c.document_id = d.document_id \
             WHERE d.conversation_id = ?1 AND d.kind = 'focus' \
             AND CAST(c.created_at AS INTEGER) > ?2",
            rusqlite::params![conversation_id, turn_start_millis],
            |row| row.get(0),
        ) {
            Ok(count) => count,
            Err(err) => {
                probe
                    .probe_errors
                    .push(format!("continuity_commits probe failed: {err}"));
                0
            }
        };
        if focus_commits > 0 {
            probe.detected = true;
            return Ok(probe);
        }
    }

    Ok(probe)
}

use crate::context_health;
use crate::governance;
use crate::inference::engine;
use crate::inference::runtime_env;
use crate::inference::runtime_kernel;
use crate::inference::runtime_state;
use crate::inference::turn_engine;
use crate::lcm;
use crate::live_context;

pub const CHAT_CONVERSATION_ID: i64 = 1;
const DEFAULT_CONTINUITY_REFRESH_TIMEOUT_SECS: u64 = 45;
const DEFAULT_REMOTE_CHAT_TURN_TIMEOUT_SECS: u64 = 180;
// Local inference (llama-cli per-call cold-start architecture) needs a much
// longer turn budget than remote APIs. A single agent turn often involves
// dozens of tool round-trips, each carrying ~5-15 s GPU model-load overhead
// plus the actual generation time. With a 900 s ceiling, complex
// large workspace tasks hit
// "direct session timeout after 900s" mid-turn, the persistent session is
// dropped and the ggml backend is killed — leaving leases held and 0 passes.
// 3600 s gives long-running tasks room while still bounding genuinely stuck
// turns. Operators can override via CTOX_CHAT_TURN_TIMEOUT_SECS.
const DEFAULT_LOCAL_CHAT_TURN_TIMEOUT_SECS: u64 = 3600;
const CONTINUITY_REFRESH_FAULT_FILE_ENV_KEY: &str = "CTOX_CONTINUITY_REFRESH_FAULT_FILE";
const CONTINUITY_REFRESH_TIMEOUT_ENV_KEY: &str = "CTOX_CONTINUITY_REFRESH_TIMEOUT_SECS";

fn default_chat_turn_timeout_secs(source_is_local: bool, api_provider_resolved: bool) -> u64 {
    if !source_is_local || api_provider_resolved {
        DEFAULT_REMOTE_CHAT_TURN_TIMEOUT_SECS
    } else {
        DEFAULT_LOCAL_CHAT_TURN_TIMEOUT_SECS
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalModelProviderSpec {
    pub(crate) provider_id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) transport_endpoint: String,
    pub(crate) wire_api: &'static str,
}

impl LocalModelProviderSpec {
    pub(crate) fn ctox_core_cli_overrides(&self) -> Vec<(String, TomlValue)> {
        vec![
            (
                format!("model_providers.{}.name", self.provider_id),
                TomlValue::String(self.name.to_string()),
            ),
            (
                format!("model_providers.{}.transport_endpoint", self.provider_id),
                TomlValue::String(self.transport_endpoint.clone()),
            ),
            (
                format!(
                    "model_providers.{}.socket_transport_required",
                    self.provider_id
                ),
                TomlValue::Boolean(true),
            ),
            (
                format!("model_providers.{}.wire_api", self.provider_id),
                TomlValue::String(self.wire_api.to_string()),
            ),
            (
                format!("model_providers.{}.requires_openai_auth", self.provider_id),
                TomlValue::Boolean(false),
            ),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApiModelProviderSpec {
    pub(crate) provider_id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) base_url: String,
    pub(crate) env_key: &'static str,
    /// Upstream edge transport handed to ctox-core for the selected provider.
    /// CTOX itself remains canonical Responses internally; adapters normalize
    /// into provider-native forms only at this outer boundary.
    pub(crate) wire_api: &'static str,
    pub(crate) requires_full_responses_history: bool,
}

impl ApiModelProviderSpec {
    pub(crate) fn ctox_core_cli_overrides(&self) -> Vec<(String, TomlValue)> {
        vec![
            (
                format!("model_providers.{}.name", self.provider_id),
                TomlValue::String(self.name.to_string()),
            ),
            (
                format!("model_providers.{}.base_url", self.provider_id),
                TomlValue::String(self.base_url.clone()),
            ),
            (
                format!("model_providers.{}.wire_api", self.provider_id),
                TomlValue::String(self.wire_api.to_string()),
            ),
            (
                format!("model_providers.{}.requires_openai_auth", self.provider_id),
                TomlValue::Boolean(false),
            ),
            (
                format!(
                    "model_providers.{}.requires_full_responses_history",
                    self.provider_id
                ),
                TomlValue::Boolean(self.requires_full_responses_history),
            ),
        ]
    }
}

pub fn run_chat_turn_with_events<F>(
    root: &Path,
    db_path: &Path,
    prompt: &str,
    workspace_root: Option<&Path>,
    conversation_id: i64,
    suggested_skill: Option<&str>,
    emit: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    run_chat_turn_with_events_extended(
        root,
        db_path,
        prompt,
        workspace_root,
        conversation_id,
        suggested_skill,
        false,
        None, // no persistent session
        emit,
    )
}

/// Like `run_chat_turn_with_events` but accepts a `force_continuity_refresh`
/// hint and an optional `PersistentSession`.
///
/// When `session` is `Some`, the turn reuses the existing ctox-core client
/// so that context accumulates across turns (tool results, prior replies,
/// conversation history all stay in ctox-core's thread state). This is
/// critical for the CompactPolicy to observe real context growth and fire
/// Emergency/Adaptive compaction when needed.
/// When `session` is `None`, the turn now provisions its own local
/// `PersistentSession` so the main turn and continuity refresh still share
/// one in-process runtime.
pub(crate) fn run_chat_turn_with_events_extended<F>(
    root: &Path,
    db_path: &Path,
    prompt: &str,
    workspace_root: Option<&Path>,
    conversation_id: i64,
    suggested_skill: Option<&str>,
    force_continuity_refresh: bool,
    session: Option<&mut PersistentSession>,
    emit: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    run_chat_turn_with_events_extended_guarded(
        root,
        db_path,
        prompt,
        workspace_root,
        conversation_id,
        suggested_skill,
        force_continuity_refresh,
        session,
        emit,
    )
}

pub(crate) fn run_chat_turn_with_events_extended_guarded<F>(
    root: &Path,
    db_path: &Path,
    prompt: &str,
    _workspace_root: Option<&Path>,
    conversation_id: i64,
    suggested_skill: Option<&str>,
    force_continuity_refresh: bool,
    session: Option<&mut PersistentSession>,
    emit: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    run_chat_turn_with_events_extended_guarded_with_options(
        root,
        db_path,
        prompt,
        _workspace_root,
        conversation_id,
        suggested_skill,
        force_continuity_refresh,
        session,
        ChatTurnSessionOptions::default(),
        emit,
    )
}

pub(crate) fn run_chat_turn_with_events_extended_guarded_with_options<F>(
    root: &Path,
    db_path: &Path,
    prompt: &str,
    workspace_root: Option<&Path>,
    conversation_id: i64,
    suggested_skill: Option<&str>,
    force_continuity_refresh: bool,
    mut session: Option<&mut PersistentSession>,
    options: ChatTurnSessionOptions,
    mut emit: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    emit("runtime-resolve");
    let runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root)?;
    emit("runtime-settings");
    if runtime.state.source.is_local() {
        emit("runtime-backend-ready");
        supervisor::ensure_chat_backend_ready(root, false)
            .context("failed to ensure local chat backend before direct session")?;
    }
    let operator_settings = runtime_env::effective_operator_env_map(root).unwrap_or_default();
    emit("session-start");
    let mut owned_session = if session.is_none() {
        if options.disable_mcp_servers || options.base_instructions.is_some() {
            emit("session-mcp-servers-disabled");
            Some(
                PersistentSession::start_without_mcp_servers_with_instructions(
                    root,
                    &operator_settings,
                    options.base_instructions.as_deref(),
                )?,
            )
        } else {
            Some(PersistentSession::start(root, &operator_settings)?)
        }
    } else {
        None
    };
    if let Some(workspace_root) = workspace_root {
        if let Some(session) = session.as_deref_mut() {
            session.set_turn_cwd(workspace_root);
        } else if let Some(session) = owned_session.as_mut() {
            session.set_turn_cwd(workspace_root);
        }
    }
    emit("session-ready");
    let selected_model = runtime.state.active_or_selected_model().unwrap_or_default();
    let api_provider_resolved =
        resolve_api_model_provider_spec(selected_model, &operator_settings, Some(&runtime))
            .is_some();
    // A local gateway/process can still proxy a remote provider (for example
    // Pi driving MiniMax). Timeout semantics follow the resolved provider
    // boundary, not merely the process location, so a hung remote API call
    // cannot inherit the one-hour local-inference budget.
    let default_turn_timeout_secs =
        default_chat_turn_timeout_secs(runtime.state.source.is_local(), api_provider_resolved);
    let configured_turn_timeout_secs = read_usize_setting(
        &operator_settings,
        "CTOX_CHAT_TURN_TIMEOUT_SECS",
        default_turn_timeout_secs as usize,
    ) as u64;
    let config = turn_engine::ChatTurnConfig {
        max_context_tokens: runtime.turn_context_tokens(),
        turn_timeout_secs: options
            .turn_timeout_secs_override
            .unwrap_or(configured_turn_timeout_secs),
    };
    emit("lcm-open");
    let engine = lcm::LcmEngine::open(db_path, lcm::LcmConfig::default())?;
    let _ = engine.continuity_init_documents(conversation_id)?;
    emit("persist-user-turn");
    persist_lcm_message_with_retry(db_path, conversation_id, "user", prompt, &mut emit)
        .context("failed to persist user message into LCM")?;
    if options.plain_prompt {
        emit("plain-prompt-context");
        let preflight_base_instructions = session
            .as_deref()
            .or(owned_session.as_ref())
            .map(|sess| sess.base_instructions().to_string())
            .unwrap_or_default();
        if !runtime.state.source.is_local() {
            let heuristic_text = format!("{preflight_base_instructions}\n\n{prompt}");
            let estimated_tokens = lcm::estimate_tokens(&heuristic_text) as i64;
            let context_limit = config.max_context_tokens.max(1);
            let heuristic_budget = context_limit
                .saturating_mul(95)
                .checked_div(100)
                .unwrap_or(1)
                .max(1);
            emit(&format!(
                "heuristic-token-preflight tokens={} budget={} context={} source=plain-heuristic-api",
                estimated_tokens, heuristic_budget, context_limit
            ));
            if estimated_tokens > heuristic_budget {
                anyhow::bail!(
                    "context_preflight_heuristic_overflow: estimated plain prompt tokens {} exceed heuristic input budget {} for context window {} via plain-heuristic-api",
                    estimated_tokens,
                    heuristic_budget,
                    context_limit
                );
            }
        }
        emit("invoke-model");
        let mut emit_progress = |event: &serde_json::Value| {
            emit(&format!("worker-progress {}", event));
        };
        let reply = match session.as_deref_mut() {
            Some(sess) => sess.run_turn_inner_with_context_and_progress(
                prompt,
                None,
                Some(Duration::from_secs(config.turn_timeout_secs)),
                None,
                &mut emit_progress,
            )?,
            None => owned_session
                .as_mut()
                .expect("owned persistent session should exist when no session was supplied")
                .run_turn_inner_with_context_and_progress(
                    prompt,
                    None,
                    Some(Duration::from_secs(config.turn_timeout_secs)),
                    None,
                    &mut emit_progress,
                )?,
        };
        emit("persist-assistant-turn");
        persist_lcm_message_with_retry(db_path, conversation_id, "assistant", &reply, &mut emit)?;
        emit("continuity-refresh-skipped");
        emit(&format!(
            "turn-outcome stage=complete health=plain score=100 reply_chars={} continuity_updates=0 continuity_skips=1 omitted=0",
            reply.chars().count()
        ));
        emit("turn-complete");
        return Ok(reply);
    }
    emit("turn-plan");
    let plan = turn_engine::build_turn_plan(&engine, conversation_id, config.clone())?;
    emit(&format!(
        "turn-plan context={} timeout={}s stage={}",
        plan.max_context_tokens,
        plan.turn_timeout_secs,
        plan.stage.as_str()
    ));
    emit("compaction-check");
    let decision = plan.compaction.clone();
    emit(&format!(
        "compaction-window {} / {} ({})",
        decision.current_tokens, decision.threshold, decision.reason
    ));
    let mut compaction_result = None;
    if decision.should_compact {
        emit("compaction-run");
        let result = compact_with_semantic_summarizer(
            root,
            &operator_settings,
            &engine,
            conversation_id,
            config.max_context_tokens,
            false,
        )?;
        emit(&format!(
            "compaction-result before={} after={} rounds={} created={}",
            result.tokens_before,
            result.tokens_after,
            result.rounds,
            result.created_summary_ids.len()
        ));
        compaction_result = Some(result);
        emit("compaction-complete");
    }
    let compaction_guard =
        turn_engine::assess_compaction_guard(&decision, compaction_result.as_ref());
    emit(&format!("compaction-guard {}", compaction_guard.summary));
    emit("snapshot-context");
    let mut snapshot = engine.working_set_snapshot(conversation_id, 512)?;
    let mut continuity = engine.continuity_show_all(conversation_id)?;
    let mut mission_state = engine.mission_state(conversation_id)?;
    let mut mission_assurance = engine.mission_assurance_snapshot(conversation_id)?;
    let mut strategy = engine.active_strategy_snapshot(conversation_id, None)?;
    let mut forgotten_entries = engine.continuity_forgotten_recent(conversation_id, None, 128)?;
    let mut health = context_health::assess_with_forgotten(
        &snapshot,
        &continuity,
        &forgotten_entries,
        prompt,
        config.max_context_tokens,
    );
    let mut governance_snapshot = governance::prompt_snapshot(root, conversation_id)
        .unwrap_or_else(|err| {
            // An empty governance block is indistinguishable from a quiet
            // one for the model; at least leave an operator-visible trace.
            eprintln!("ctox turn: governance snapshot unavailable: {err:#}");
            Default::default()
        });
    emit(&format!(
        "context-health {} {}",
        health.status.as_str(),
        health.overall_score
    ));
    emit("render-prompt");
    let mut rendered_prompt = live_context::render_runtime_prompt(
        root,
        &snapshot,
        &continuity,
        &mission_state,
        &mission_assurance,
        &strategy,
        &governance_snapshot,
        &health,
        suggested_skill,
    )?;
    ensure_rendered_prompt_is_invocable(
        &snapshot,
        &mut rendered_prompt,
        prompt,
        &health,
        &mut emit,
    )?;
    emit(&format!(
        "context-selection rendered={} omitted={}",
        rendered_prompt.rendered_context_items, rendered_prompt.omitted_context_items
    ));
    // Budget the SAME text the session-level preflight counts: the session
    // sends base_instructions + prompt, so counting the rendered prompt
    // alone lets a turn pass this loop and then hard-fail inside
    // run_turn_async with no compaction retry (the worker system prompt is
    // several thousand tokens, not noise).
    let preflight_base_instructions = session
        .as_deref()
        .or(owned_session.as_ref())
        .map(|sess| sess.base_instructions().to_string())
        .unwrap_or_default();
    let mut previous_preflight_tokens: Option<i64> = None;
    let mut exact_prompt_preflight: Option<super::direct_session::ExactPromptTokenCount> = None;
    for exact_preflight_round in 0..=2 {
        let preflight_text = format!(
            "{preflight_base_instructions}\n\n{}\n\n{}",
            rendered_prompt.context_instructions, rendered_prompt.latest_user_prompt
        );
        let Some(count) = super::direct_session::exact_prompt_token_count(root, &preflight_text)?
        else {
            break;
        };
        let safe_budget =
            super::direct_session::exact_prompt_safe_input_budget(count.context_limit);
        emit(&format!(
            "exact-token-preflight round={} tokens={} safe_budget={} context={} source={}",
            exact_preflight_round, count.tokens, safe_budget, count.context_limit, count.source
        ));
        if count.tokens <= safe_budget {
            exact_prompt_preflight = Some(count);
            break;
        }
        // LCM compaction only shrinks the conversation-evidence section;
        // every other block (system prompt, CURRENT REQUEST, continuity,
        // strategy, governance, workflow) is invariant under it. If a round
        // barely moved the count, the overflow is invariant-dominated —
        // bail now instead of permanently coarsening stored history for
        // nothing.
        if let Some(previous) = previous_preflight_tokens {
            let progress = previous.saturating_sub(count.tokens);
            if progress * 50 < previous {
                anyhow::bail!(
                    "context_preflight_exact_overflow: rendered prompt tokens {} exceed safe input budget {} for context window {} and LCM compaction reduced them by only {} tokens — the overflow is dominated by compaction-invariant sections (system prompt, current request, runtime blocks) via {}",
                    count.tokens,
                    safe_budget,
                    count.context_limit,
                    progress,
                    count.source
                );
            }
        }
        previous_preflight_tokens = Some(count.tokens);
        if exact_preflight_round >= 2 {
            anyhow::bail!(
                "context_preflight_exact_overflow: exact rendered prompt tokens {} exceed safe input budget {} for context window {} after {} LCM compaction rounds via {}",
                count.tokens,
                safe_budget,
                count.context_limit,
                exact_preflight_round,
                count.source
            );
        }
        emit("exact-token-preflight-compaction-run");
        let result = compact_with_semantic_summarizer(
            root,
            &operator_settings,
            &engine,
            conversation_id,
            config.max_context_tokens,
            true,
        )?;
        emit(&format!(
            "exact-token-preflight-compaction-result before={} after={} rounds={} created={}",
            result.tokens_before,
            result.tokens_after,
            result.rounds,
            result.created_summary_ids.len()
        ));
        compaction_result = Some(result);
        snapshot = engine.working_set_snapshot(conversation_id, 512)?;
        continuity = engine.continuity_show_all(conversation_id)?;
        mission_state = engine.mission_state(conversation_id)?;
        mission_assurance = engine.mission_assurance_snapshot(conversation_id)?;
        strategy = engine.active_strategy_snapshot(conversation_id, None)?;
        forgotten_entries = engine.continuity_forgotten_recent(conversation_id, None, 128)?;
        health = context_health::assess_with_forgotten(
            &snapshot,
            &continuity,
            &forgotten_entries,
            prompt,
            config.max_context_tokens,
        );
        governance_snapshot =
            governance::prompt_snapshot(root, conversation_id).unwrap_or_else(|err| {
                eprintln!("ctox turn: governance snapshot unavailable: {err:#}");
                Default::default()
            });
        rendered_prompt = live_context::render_runtime_prompt(
            root,
            &snapshot,
            &continuity,
            &mission_state,
            &mission_assurance,
            &strategy,
            &governance_snapshot,
            &health,
            suggested_skill,
        )?;
        ensure_rendered_prompt_is_invocable(
            &snapshot,
            &mut rendered_prompt,
            prompt,
            &health,
            &mut emit,
        )?;
        emit(&format!(
            "context-selection rendered={} omitted={}",
            rendered_prompt.rendered_context_items, rendered_prompt.omitted_context_items
        ));
    }
    // The exact-token preflight above is a deliberate no-op on API runtimes:
    // `exact_prompt_token_count` returns `Ok(None)` for any non-local kernel,
    // so the loop breaks on the first iteration and an API turn reaches the
    // model with zero overflow protection. Add a SEPARATE heuristic preflight
    // for the API path that estimates the same `base_instructions + prompt`
    // text the session sends, using lcm's char/4 token estimate against a
    // looser ~95% input budget (the estimate is coarse, so the budget must not
    // be as tight as the exact path). This only bails; it never compacts, and
    // the distinct `context_preflight_heuristic_overflow` marker
    // (source `heuristic-api`) gets a cooldown via
    // `hard_runtime_blocker_retry_cooldown_secs`.
    if !runtime.state.source.is_local() {
        let heuristic_text = format!(
            "{preflight_base_instructions}\n\n{}\n\n{}",
            rendered_prompt.context_instructions, rendered_prompt.latest_user_prompt
        );
        let estimated_tokens = lcm::estimate_tokens(&heuristic_text) as i64;
        let context_limit = config.max_context_tokens.max(1);
        let heuristic_budget = context_limit
            .saturating_mul(95)
            .checked_div(100)
            .unwrap_or(1)
            .max(1);
        emit(&format!(
            "heuristic-token-preflight tokens={} budget={} context={} source=heuristic-api",
            estimated_tokens, heuristic_budget, context_limit
        ));
        if estimated_tokens > heuristic_budget {
            anyhow::bail!(
                "context_preflight_heuristic_overflow: estimated rendered prompt tokens {} exceed heuristic input budget {} for context window {} via heuristic-api — API-runtime preflight cannot tokenize exactly, so this is a char/4 estimate of base_instructions + rendered prompt",
                estimated_tokens,
                heuristic_budget,
                context_limit
            );
        }
    }
    let turn_start_ts = current_rfc3339_timestamp();
    emit("invoke-model");
    let mut emit_progress = |event: &serde_json::Value| {
        emit(&format!("worker-progress {}", event));
    };
    let reply = match session.as_deref_mut() {
        Some(sess) => sess.run_turn_inner_with_context_and_progress(
            &rendered_prompt.latest_user_prompt,
            Some(&rendered_prompt.context_instructions),
            Some(Duration::from_secs(config.turn_timeout_secs)),
            exact_prompt_preflight.clone(),
            &mut emit_progress,
        )?,
        None => owned_session
            .as_mut()
            .expect("owned persistent session should exist when no session was supplied")
            .run_turn_inner_with_context_and_progress(
                &rendered_prompt.latest_user_prompt,
                Some(&rendered_prompt.context_instructions),
                Some(Duration::from_secs(config.turn_timeout_secs)),
                exact_prompt_preflight.clone(),
                &mut emit_progress,
            )?,
    };
    emit("persist-assistant-turn");
    persist_lcm_message_with_retry(db_path, conversation_id, "assistant", &reply, &mut emit)?;
    // Detect durable state transitions since the last refresh (internal work
    // closed — including service-side closures between turns — knowledge
    // entry added, focus document replaced this turn). These count as task
    // boundaries and force a continuity refresh even if the output budget
    // has not yet been hit.
    let boundary_window_ts =
        durable_boundary_window_start(db_path, conversation_id, &turn_start_ts);
    let state_probe = detect_durable_state_transition(
        root,
        db_path,
        conversation_id,
        &turn_start_ts,
        &boundary_window_ts,
    )
    .unwrap_or_default();
    if !state_probe.probe_errors.is_empty() {
        // A probe that ERRORED (e.g. a schema regression) is degraded, not a
        // genuine "no boundary"; surface it instead of silently suppressing
        // the continuity refresh.
        eprintln!(
            "[ctox turn-loop] durable state-transition probe degraded ({} error(s)): {}",
            state_probe.probe_errors.len(),
            state_probe.probe_errors.join("; ")
        );
    }
    let state_transition_detected = state_probe.detected;
    let effective_force_refresh = force_continuity_refresh || state_transition_detected;
    let engine = lcm::LcmEngine::open(db_path, lcm::LcmConfig::default())?;
    // New adaptive model: refresh only on durable state transition
    // (force_continuity_refresh) or when cumulative output tokens exceed
    // the configured percentage of the context window. Legacy interval
    // knob defaults to 0 (disabled); operators can re-enable it explicitly.
    let refresh_every_n = read_usize_setting(
        &operator_settings,
        "CTOX_CONTINUITY_REFRESH_EVERY_N_TURNS",
        0,
    ) as u64;
    let reply_chars = reply.chars().count() as u64;
    let trigger_reason = if force_continuity_refresh {
        "state-transition-plan"
    } else if state_transition_detected {
        "state-transition-tickets"
    } else {
        "durable-turn-budget"
    };
    let due_refreshes = record_durable_refresh_demand(
        db_path,
        conversation_id,
        effective_force_refresh,
        refresh_every_n,
        reply_chars,
        &format!("{trigger_reason}:{boundary_window_ts}"),
    )?;
    let refresh_now = !due_refreshes.is_empty();
    record_refresh_telemetry(conversation_id, reply_chars, refresh_now);
    let continuity_stats = if refresh_now {
        emit(&format!(
            "continuity-refresh reason={} kinds={}",
            trigger_reason,
            due_refreshes.iter().cloned().collect::<Vec<_>>().join(",")
        ));
        match session.as_deref_mut() {
            Some(refresh_session) => refresh_continuity_documents(
                root,
                &operator_settings,
                db_path,
                &engine,
                conversation_id,
                &due_refreshes,
                refresh_session,
                &mut emit,
            )?,
            None => refresh_continuity_documents(
                root,
                &operator_settings,
                db_path,
                &engine,
                conversation_id,
                &due_refreshes,
                owned_session
                    .as_mut()
                    .expect("owned persistent session should exist for continuity refresh"),
                &mut emit,
            )?,
        }
    } else {
        emit("continuity-refresh-skipped");
        Default::default()
    };
    let budget_snapshot = refresh_budget_snapshot(conversation_id);
    emit(&format!(
        "refresh-telemetry output_chars_since_refresh={} turns_since_refresh={}",
        budget_snapshot.output_chars_since_refresh, budget_snapshot.turns_since_refresh
    ));
    let outcome = turn_engine::ChatTurnOutcome {
        stage: turn_engine::TurnStage::Complete,
        health_status: health.status,
        health_score: health.overall_score,
        context_items_rendered: rendered_prompt.rendered_context_items,
        context_items_omitted: rendered_prompt.omitted_context_items,
        reply_chars: reply.chars().count(),
        compaction: compaction_result,
        continuity: continuity_stats,
        compaction_guard,
    };
    emit(&format!(
        "turn-outcome stage={} health={} score={} reply_chars={} continuity_updates={} continuity_skips={} omitted={}",
        outcome.stage.as_str(),
        outcome.health_status.as_str(),
        outcome.health_score,
        outcome.reply_chars,
        outcome.continuity.updated,
        outcome.continuity.skipped_prompt_build
            + outcome.continuity.skipped_invoke
            + outcome.continuity.skipped_apply,
        outcome.context_items_omitted
    ));
    emit("turn-complete");
    Ok(reply)
}

fn persist_lcm_message_with_retry(
    db_path: &Path,
    conversation_id: i64,
    role: &str,
    content: &str,
    emit: &mut dyn FnMut(&str),
) -> Result<lcm::MessageRecord> {
    let mut last_error = None;
    for attempt in 1..=4 {
        match lcm::run_add_message(db_path, conversation_id, role, content) {
            Ok(record) => return Ok(record),
            Err(err) => {
                let summary = err.to_string();
                last_error = Some(err);
                if attempt == 4 {
                    break;
                }
                emit(&format!(
                    "persist-{role}-turn-retry attempt={attempt} error={}",
                    clip_for_log(&summary, 160)
                ));
                std::thread::sleep(Duration::from_millis(250 * attempt as u64));
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("LCM message persistence failed without error")))
}

fn clip_for_log(value: &str, max_chars: usize) -> String {
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

fn ensure_rendered_prompt_is_invocable(
    snapshot: &lcm::LcmSnapshot,
    rendered_prompt: &mut live_context::RenderedRuntimePrompt,
    prompt: &str,
    health: &context_health::ContextHealthSnapshot,
    emit: &mut dyn FnMut(&str),
) -> Result<()> {
    let current_prompt = prompt.trim();
    let latest_empty = rendered_prompt.latest_user_prompt.trim().is_empty();
    // Compare against the clipped form the renderer actually emits: for
    // prompts over the echo budget the raw text can never be contained, and
    // prepending it would duplicate the first 8k chars on every such turn.
    let rendered_echo = live_context::sanitize_latest_prompt(current_prompt);
    let missing_current_prompt = !current_prompt.is_empty()
        && !rendered_echo.is_empty()
        && !rendered_prompt.prompt.contains(rendered_echo.as_str());
    if latest_empty || missing_current_prompt {
        emit(&format!(
            "context-selection fallback-current-prompt latest_empty={} missing_current={}",
            latest_empty, missing_current_prompt
        ));
        rendered_prompt.prompt = render_current_prompt_fallback(&rendered_prompt.prompt, prompt);
        rendered_prompt.latest_user_prompt = prompt.to_string();
    }
    if rendered_context_empty_with_existing_history(
        snapshot,
        rendered_prompt.rendered_context_items,
    ) {
        anyhow::bail!(
            "context_selection_empty: refusing model invocation because LCM history exists but no context evidence rendered"
        );
    }
    if health.status == context_health::ContextHealthStatus::Critical
        && critical_context_selection_is_empty(rendered_prompt)
    {
        anyhow::bail!(
            "context_selection_empty_critical: refusing model invocation because context health is critical and no context evidence rendered"
        );
    }
    // Deterministic doomed-retry gate: when the EXACT same user turn is being
    // re-issued (recent_user_turn_repeated, exact-normalized match, repetition>1)
    // into an N-deep structured-failure loop (blocked_status_loop, >=3 blocked
    // assistant rows) and both have already reached Critical, the harness would
    // otherwise burn a full local-inference turn on a retry the deterministic
    // signal already condemned. Refuse on the conjunction of these two hard-key
    // facts (never a similarity judgement) so the existing classify/cooldown
    // path handles it instead of re-firing the identical prompt.
    let exact_duplicate_user_turn = health.warnings.iter().any(|warning| {
        warning.code.as_str() == "recent_user_turn_repeated"
            && warning.severity == context_health::WarningSeverity::Critical
    });
    let structured_failure_loop = health.warnings.iter().any(|warning| {
        warning.code.as_str() == "blocked_status_loop"
            && warning.severity == context_health::WarningSeverity::Critical
    });
    if exact_duplicate_user_turn && structured_failure_loop {
        emit(
            "context-selection context_loop_short_circuit critical_duplicate_user_turn critical_blocked_status_loop",
        );
        anyhow::bail!(
            "context_loop_short_circuit: exact-duplicate user turn re-entering an N-deep structured-failure loop with no new evidence"
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "turn_loop_boundary_tests.rs"]
mod boundary_tests;

/// Tool-based continuity refresh. Sends the model a prompt describing the
/// `ctox continuity-update` CLI (three modes: full / replace / diff) and
/// expects the model to invoke it via its shell tool. We then detect
/// whether the doc actually changed by comparing head commit ids before
/// and after the invocation — no more parsing of the model's reply text.
///
/// A fault-injection override still exists for tests: when present, the
/// injected diff is applied directly via `continuity_apply_diff` without
/// calling the model.
fn refresh_continuity_documents(
    root: &Path,
    settings: &BTreeMap<String, String>,
    db_path: &Path,
    engine: &lcm::LcmEngine,
    conversation_id: i64,
    due_kinds: &HashSet<String>,
    session: &mut PersistentSession,
    emit: &mut impl FnMut(&str),
) -> Result<turn_engine::ContinuityRefreshStats> {
    let mut stats = turn_engine::ContinuityRefreshStats::default();
    let refresh_timeout_secs = continuity_refresh_timeout_secs(settings);
    for kind in [
        lcm::ContinuityKind::Narrative,
        lcm::ContinuityKind::Anchors,
        lcm::ContinuityKind::Focus,
    ] {
        let kind_label = match kind {
            lcm::ContinuityKind::Narrative => "narrative",
            lcm::ContinuityKind::Anchors => "anchors",
            lcm::ContinuityKind::Focus => "focus",
        };
        if !due_kinds.contains(kind_label) {
            continue;
        }
        stats.attempted += 1;
        emit(&format!("continuity-{kind_label}-build"));
        let payload = match engine.continuity_build_prompt(conversation_id, kind) {
            Ok(payload) => payload,
            Err(err) => {
                stats.skipped_prompt_build += 1;
                let _ = mark_durable_refresh_failed(
                    db_path,
                    conversation_id,
                    kind_label,
                    &format!("prompt build failed: {err}"),
                );
                eprintln!("ctox continuity refresh skipped {kind_label} prompt build: {err}");
                continue;
            }
        };
        let head_before = engine
            .continuity_show(conversation_id, kind)
            .map(|doc| doc.head_commit_id)
            .unwrap_or_default();

        // Fault-injection override: bypass the model entirely, still apply
        // via the legacy diff path so existing tests keep working.
        match take_continuity_refresh_fault(root, settings, kind_label) {
            Ok(Some(injected_diff)) => {
                emit(&format!("continuity-{kind_label}-fault-injected"));
                eprintln!(
                    "ctox continuity refresh injected {kind_label} fault preview: {}",
                    summarize_continuity_diff_for_log(&injected_diff)
                );
                if !injected_diff.trim().is_empty() {
                    if let Err(err) =
                        engine.continuity_apply_diff(conversation_id, kind, injected_diff.trim())
                    {
                        stats.skipped_apply += 1;
                        let _ = mark_durable_refresh_failed(
                            db_path,
                            conversation_id,
                            kind_label,
                            &format!("fault-injected apply failed: {err}"),
                        );
                        eprintln!(
                            "ctox continuity refresh skipped invalid injected {kind_label} diff: {err}"
                        );
                    } else {
                        let head_after = engine
                            .continuity_show(conversation_id, kind)
                            .map(|doc| doc.head_commit_id)
                            .unwrap_or_default();
                        if mark_durable_refresh_consumed(
                            db_path,
                            conversation_id,
                            kind_label,
                            &head_before,
                            &head_after,
                        )
                        .is_ok()
                        {
                            stats.updated += 1;
                        } else {
                            let _ = mark_durable_refresh_failed(
                                db_path,
                                conversation_id,
                                kind_label,
                                "fault-injected refresh did not advance the head commit",
                            );
                        }
                    }
                } else {
                    let _ = mark_durable_refresh_failed(
                        db_path,
                        conversation_id,
                        kind_label,
                        "fault-injected refresh was empty and did not advance the head commit",
                    );
                }
                if kind == lcm::ContinuityKind::Anchors {
                    let _ = engine.continuity_preserve_recent_anchor_literals(conversation_id);
                }
                continue;
            }
            Ok(None) => {}
            Err(err) => {
                stats.skipped_invoke += 1;
                let _ = mark_durable_refresh_failed(
                    db_path,
                    conversation_id,
                    kind_label,
                    &format!("fault injection lookup failed: {err}"),
                );
                eprintln!("ctox continuity refresh skipped {kind_label} fault injection: {err}");
                continue;
            }
        }

        emit(&format!("continuity-{kind_label}-invoke"));
        let reply = match session.run_turn(
            &payload.prompt,
            Some(Duration::from_secs(refresh_timeout_secs)),
            None,
            None,
            conversation_id,
        ) {
            Ok(reply) => reply,
            Err(err) => {
                stats.skipped_invoke += 1;
                let _ = mark_durable_refresh_failed(
                    db_path,
                    conversation_id,
                    kind_label,
                    &format!("model refresh failed: {err}"),
                );
                // A poisoned session must not be swallowed as an optional
                // helper failure: its detached turn may still be running, and
                // reusing the session is refused anyway. Propagate so the
                // slice fails and the service discards the session instead of
                // carrying it into the next job (ctox#21 review round 3).
                if err
                    .downcast_ref::<super::direct_session::SessionPoisoned>()
                    .is_some()
                {
                    return Err(err.context(format!(
                        "continuity refresh {kind_label} poisoned the worker session"
                    )));
                }
                eprintln!("ctox continuity refresh skipped {kind_label} invocation: {err}");
                continue;
            }
        };

        // The model applies the change via the `ctox continuity-update`
        // CLI. We verify by re-reading the head commit id from the DB
        // rather than parsing `reply` — tool calls either wrote a new
        // commit or they didn't.
        let head_after = engine
            .continuity_show(conversation_id, kind)
            .map(|doc| doc.head_commit_id)
            .unwrap_or_default();
        if head_after != head_before && !head_after.is_empty() {
            emit(&format!("continuity-{kind_label}-apply"));
            eprintln!(
                "ctox continuity refresh {kind_label}: head advanced {} -> {} (tool-applied)",
                head_before, head_after
            );
            stats.updated += 1;
            mark_durable_refresh_consumed(
                db_path,
                conversation_id,
                kind_label,
                &head_before,
                &head_after,
            )?;
        } else {
            let _ = mark_durable_refresh_failed(
                db_path,
                conversation_id,
                kind_label,
                "model returned without advancing the continuity head commit",
            );
            eprintln!(
                "ctox continuity refresh {kind_label}: no tool-driven change (reply preview: {})",
                summarize_continuity_diff_for_log(&reply)
            );
        }

        if kind == lcm::ContinuityKind::Anchors {
            emit("continuity-anchors-preserve-literals");
            match engine.continuity_preserve_recent_anchor_literals(conversation_id) {
                // Literal preservation is a mechanical safety net, not a
                // model-driven refresh; counting it as `updated` masked
                // refresh turns where the model never called the CLI.
                Ok(Some(_)) => {}
                Ok(None) => {}
                Err(err) => {
                    stats.skipped_apply += 1;
                    eprintln!("ctox continuity refresh skipped anchor literal preservation: {err}");
                }
            }
        }
    }
    Ok(stats)
}

fn summarize_continuity_diff_for_log(diff: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 480;

    let trimmed = diff.trim();
    let preview = if trimmed.chars().count() > MAX_PREVIEW_CHARS {
        let head = trimmed.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
        format!("{head}...")
    } else {
        trimmed.to_string()
    };
    let escaped = preview
        .replace('\\', "\\\\")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t");
    format!(
        "chars={} lines={} text=\"{}\"",
        trimmed.chars().count(),
        trimmed.lines().count(),
        escaped
    )
}

fn continuity_refresh_fault_file_path(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    let raw_path = settings
        .get(CONTINUITY_REFRESH_FAULT_FILE_ENV_KEY)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let path = PathBuf::from(raw_path);
    Some(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

fn take_continuity_refresh_fault(
    root: &Path,
    settings: &BTreeMap<String, String>,
    kind_label: &str,
) -> Result<Option<String>> {
    let Some(path) = continuity_refresh_fault_file_path(root, settings) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read continuity fault script {}", path.display()))?;
    let mut payload: serde_json::Value = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse continuity fault script JSON {}",
            path.display()
        )
    })?;
    let Some(entries) = payload
        .get_mut(kind_label)
        .and_then(|value| value.as_array_mut())
    else {
        return Ok(None);
    };
    if entries.is_empty() {
        return Ok(None);
    }

    let entry = entries.remove(0);
    let raw_diff = match entry {
        serde_json::Value::String(text) => text,
        serde_json::Value::Object(map) => map
            .get("raw")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .with_context(|| {
                format!(
                    "continuity fault entry for {kind_label} in {} is missing `raw` text",
                    path.display()
                )
            })?,
        other => {
            anyhow::bail!(
                "unsupported continuity fault entry for {kind_label} in {}: {other}",
                path.display()
            );
        }
    };

    std::fs::write(&path, serde_json::to_vec_pretty(&payload)?).with_context(|| {
        format!(
            "failed to persist updated continuity fault script {}",
            path.display()
        )
    })?;
    Ok(Some(raw_diff))
}

pub fn conversation_id_for_thread_key(thread_key: Option<&str>) -> i64 {
    let Some(thread_key) = thread_key.map(str::trim).filter(|value| !value.is_empty()) else {
        return CHAT_CONVERSATION_ID;
    };

    let digest = sha2::Sha256::digest(thread_key.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    let value = (u64::from_be_bytes(bytes) & 0x3fff_ffff_ffff_ffff) as i64;
    if value < 2 {
        2
    } else {
        value
    }
}

fn responses_api_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

pub(crate) fn resolve_api_model_provider_spec(
    model: &str,
    settings: &BTreeMap<String, String>,
    resolved_runtime: Option<&runtime_kernel::InferenceRuntimeKernel>,
) -> Option<ApiModelProviderSpec> {
    let runtime_provider = resolved_runtime.map(|runtime| {
        runtime_state::api_provider_for_upstream_base_url(&runtime.state.upstream_base_url)
            .to_string()
    });
    let provider = settings
        .get("CTOX_API_PROVIDER")
        .map(|value| runtime_state::normalize_api_provider(value).to_string())
        .or(runtime_provider)
        .filter(|provider| !provider.eq_ignore_ascii_case("local"))
        .or_else(|| {
            settings
                .get("CTOX_CHAT_SOURCE")
                .filter(|value| value.trim().eq_ignore_ascii_case("api"))
                .map(|_| engine::default_api_provider_for_model(model).to_string())
        })
        .filter(|provider| engine::api_provider_supports_model(provider, model))?;
    if !engine::api_provider_supports_model(&provider, model) {
        return None;
    }
    // OpenAI is the agent-runtime built-in default provider. The remaining
    // supported API-backed providers are normalized into one explicit CTOX
    // mode, `ctox_core_api`, which still speaks Responses internally and only
    // differs at the outer provider edge.
    let normalized = provider.to_ascii_lowercase();
    // (env_key, default_provider_for_url, wire_api, requires_full_responses_history)
    let (env_key, default_provider, wire_api, requires_full_responses_history) =
        match normalized.as_str() {
            "anthropic" => (
                "ANTHROPIC_API_KEY",
                "anthropic",
                "anthropic_messages",
                false,
            ),
            "openrouter" => ("OPENROUTER_API_KEY", "openrouter", "responses", false),
            "minimax" => (
                runtime_state::api_key_env_var_for_provider_with_env_map("minimax", settings),
                "minimax",
                "responses",
                false,
            ),
            "ctox_proxy" => (
                runtime_state::CTOX_LLM_PROXY_API_KEY_ENV,
                "ctox_proxy",
                "responses",
                false,
            ),
            "azure_foundry" => ("AZURE_FOUNDRY_API_KEY", "azure_foundry", "responses", false),
            _ => return None,
        };
    let base_url = resolved_runtime
        .map(|runtime| runtime.internal_responses_base_url())
        .or_else(|| {
            settings
                .get("CTOX_UPSTREAM_BASE_URL")
                .map(|value| responses_api_base_url(value))
        })
        .unwrap_or_else(|| {
            responses_api_base_url(runtime_state::default_api_upstream_base_url_for_provider(
                default_provider,
            ))
        });
    Some(ApiModelProviderSpec {
        provider_id: "ctox_core_api",
        name: "ctox-core-api",
        base_url,
        env_key,
        wire_api,
        requires_full_responses_history,
    })
}

pub(crate) fn resolve_local_model_provider_spec(
    resolved_runtime: Option<&runtime_kernel::InferenceRuntimeKernel>,
) -> Option<LocalModelProviderSpec> {
    let runtime = resolved_runtime?;
    let binding = runtime.primary_generation.as_ref()?;
    Some(LocalModelProviderSpec {
        provider_id: "ctox_core_local",
        name: "ctox-core-local",
        transport_endpoint: binding.transport.endpoint_string(),
        wire_api: "responses",
    })
}

pub fn summarize_runtime_error(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return "CTOX execution failed without a stable error payload.".to_string();
    }
    if let Some(summary) = summarize_known_infra_error(trimmed) {
        return summary;
    }
    // (Subprocess event-stream parsing removed — DirectSession returns text directly.)
    live_context::clip_prompt_text(trimmed, 700)
}

pub fn synthesize_failure_reply(content: &str) -> String {
    let summary = summarize_runtime_error(content);
    format!("Status: `blocked`\n\nBlocker: {summary}")
}

pub fn hard_runtime_blocker_retry_cooldown_secs(content: &str) -> Option<u64> {
    let lower = content.to_ascii_lowercase();
    if let Some(secs) = parse_retry_after_seconds(&lower) {
        return Some(secs.clamp(30, 1_800));
    }
    if lower.contains("turn completed without assistant message")
        || lower.contains("completed without assistant message")
        || lower.contains("no assistant message")
        || lower.contains("empty assistant message")
        || lower.contains("context_selection_empty")
        || lower.contains("context selection is empty")
        || lower.contains("no context evidence rendered")
        || lower.contains("context_loop_short_circuit")
        || lower.contains("context_preflight_heuristic_overflow")
        || lower.contains("mid-task compaction failed")
        || lower.contains("failed to parse structured compaction response")
        || lower.contains("stream disconnected before completion")
        || lower.contains("error sending request for url")
        || lower.contains("connection reset by peer")
        || lower.contains("max_output_tokens")
        || lower.contains("incomplete response returned")
    {
        return Some(60);
    }
    if lower.contains("database is locked")
        || lower.contains("database is busy")
        || lower.contains("sqlite_busy")
        || lower.contains("sqlite locked")
    {
        return Some(30);
    }
    if lower.contains("too many requests")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("http 429")
        || lower.contains("status 429")
        || lower.contains(" 429")
    {
        return Some(300);
    }
    if lower.contains("temporarily unavailable")
        || lower.contains("server overloaded")
        || lower.contains("bad gateway")
        || lower.contains("gateway timeout")
        || lower.contains("service unavailable")
        || lower.contains("http 502")
        || lower.contains("http 503")
        || lower.contains("http 504")
        || lower.contains("status 502")
        || lower.contains("status 503")
        || lower.contains("status 504")
    {
        return Some(180);
    }
    if lower.contains("quota exceeded")
        || lower.contains("billing details")
        || lower.contains("openai api quota is exhausted")
        || lower.contains("billing is unavailable for the selected model")
    {
        return Some(1_800);
    }
    if summarize_known_infra_error(content).is_some()
        || lower.contains("chat backend could not start on this host")
    {
        return Some(900);
    }
    None
}

fn parse_retry_after_seconds(lower: &str) -> Option<u64> {
    for marker in ["retry-after:", "retry after "] {
        let Some(rest) = lower.split(marker).nth(1) else {
            continue;
        };
        let digits = rest
            .trim_start()
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if let Ok(secs) = digits.parse::<u64>() {
            return Some(secs);
        }
    }
    None
}

fn summarize_known_infra_error(content: &str) -> Option<String> {
    let lower = content.to_ascii_lowercase();
    if lower.contains("too many requests")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("http 429")
        || lower.contains("status 429")
        || lower.contains(" 429")
    {
        return Some(
            "CTOX chat could not continue because the model API is rate-limited. The task must stay open and retry after cooldown."
                .to_string(),
        );
    }
    if lower.contains("temporarily unavailable")
        || lower.contains("server overloaded")
        || lower.contains("bad gateway")
        || lower.contains("gateway timeout")
        || lower.contains("service unavailable")
        || lower.contains("http 502")
        || lower.contains("http 503")
        || lower.contains("http 504")
        || lower.contains("status 502")
        || lower.contains("status 503")
        || lower.contains("status 504")
    {
        return Some(
            "CTOX chat could not continue because the model API is temporarily unavailable. The task must stay open and retry after cooldown."
                .to_string(),
        );
    }
    if lower.contains("quota exceeded") || lower.contains("billing details") {
        return Some(
            "CTOX chat could not continue because the configured OpenAI API quota is exhausted or billing is unavailable for the selected model.".to_string(),
        );
    }
    if lower.contains("access token could not be refreshed")
        || lower.contains("refresh token was already used")
        || lower.contains("refresh token has expired")
        || lower.contains("refresh token was revoked")
    {
        return Some(
            "CTOX chat could not continue because the ChatGPT subscription session on this host needs re-authentication. The task must stay open until login is refreshed.".to_string(),
        );
    }
    if lower.contains("feature `edition2024` is required")
        || (lower.contains("edition2024") && lower.contains("cargo"))
    {
        return Some(
            "CTOX chat backend could not start on this host because the integrated agent runtime requires a newer Rust/Cargo toolchain with Edition 2024 support.".to_string(),
        );
    }
    if lower.contains("error[e0583]")
        && lower.contains("file not found for module")
        && lower.contains("state/src/runtime.rs")
        && (lower.contains("`agent_jobs`") || lower.contains("`backfill`"))
    {
        return Some(
            "CTOX chat backend could not start on this host because the integrated agent runtime checkout is incomplete: `state/src/runtime/` is missing required module files such as `agent_jobs.rs` and `backfill.rs`.".to_string(),
        );
    }
    if lower.contains("failed to load manifest for workspace member")
        && lower.contains("cargo.toml")
    {
        return Some(
            "CTOX chat backend could not start on this host because the integrated agent-runtime workspace manifest is not buildable in its current remote environment.".to_string(),
        );
    }
    // gateway-1: every supervisor readiness failure on the local path (did not
    // become ready within Ns / exited before becoming ready / did not become
    // healthy while waiting) is wrapped with this .context prefix before it
    // reaches the worker. Matching the prefix (not the three variant fragments)
    // classifies a crashed or OOM managed local backend as a transient
    // host-infra blocker so the task holds for cooldown instead of burning.
    if lower.contains("failed to ensure local chat backend") {
        return Some(
            "CTOX chat backend could not start on this host because the managed local backend did not become ready. The task must stay open and retry after cooldown.".to_string(),
        );
    }
    None
}

fn render_current_prompt_fallback(rendered_prompt: &str, current_prompt: &str) -> String {
    let current_prompt = current_prompt.trim();
    if current_prompt.is_empty() {
        return rendered_prompt.to_string();
    }

    format!(
        "CURRENT REQUEST (authoritative)\n{}\n\n{}",
        current_prompt,
        rendered_prompt.trim_start()
    )
}

fn rendered_context_empty_with_existing_history(
    snapshot: &lcm::LcmSnapshot,
    rendered_context_items: usize,
) -> bool {
    if rendered_context_items > 0 {
        return false;
    }
    let non_empty_messages = snapshot
        .messages
        .iter()
        .filter(|message| !message.content.trim().is_empty())
        .count();
    let has_summary = snapshot
        .summaries
        .iter()
        .any(|summary| !summary.content.trim().is_empty());
    non_empty_messages > 1 || has_summary
}

fn critical_context_selection_is_empty(
    rendered_prompt: &live_context::RenderedRuntimePrompt,
) -> bool {
    rendered_prompt.rendered_context_items == 0
        && rendered_prompt.latest_user_prompt.trim().is_empty()
}

fn read_usize_setting(settings: &BTreeMap<String, String>, key: &str, default: usize) -> usize {
    settings
        .get(key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn continuity_refresh_timeout_secs(settings: &BTreeMap<String, String>) -> u64 {
    read_usize_setting(
        settings,
        CONTINUITY_REFRESH_TIMEOUT_ENV_KEY,
        DEFAULT_CONTINUITY_REFRESH_TIMEOUT_SECS as usize,
    ) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_provider_overrides_define_core_provider_without_env_key() {
        let spec = ApiModelProviderSpec {
            provider_id: "ctox_core_api",
            name: "ctox-core-api",
            base_url: "https://contoso.cognitiveservices.azure.com/openai/v1".to_string(),
            env_key: "AZURE_FOUNDRY_API_KEY",
            wire_api: "responses",
            requires_full_responses_history: false,
        };

        let overrides = spec
            .ctox_core_cli_overrides()
            .into_iter()
            .collect::<BTreeMap<_, _>>();

        assert_eq!(
            overrides.get("model_providers.ctox_core_api.name"),
            Some(&TomlValue::String("ctox-core-api".to_string()))
        );
        assert_eq!(
            overrides.get("model_providers.ctox_core_api.base_url"),
            Some(&TomlValue::String(
                "https://contoso.cognitiveservices.azure.com/openai/v1".to_string()
            ))
        );
        assert_eq!(
            overrides.get("model_providers.ctox_core_api.wire_api"),
            Some(&TomlValue::String("responses".to_string()))
        );
        assert_eq!(
            overrides.get("model_providers.ctox_core_api.requires_openai_auth"),
            Some(&TomlValue::Boolean(false))
        );
        assert_eq!(
            overrides.get("model_providers.ctox_core_api.requires_full_responses_history"),
            Some(&TomlValue::Boolean(false))
        );
        assert!(!overrides.contains_key("model_providers.ctox_core_api.env_key"));
    }

    #[test]
    fn minimax_m3_proxy_settings_resolve_core_api_provider() {
        let mut settings = BTreeMap::new();
        settings.insert("CTOX_API_PROVIDER".to_string(), "minimax".to_string());
        settings.insert(
            "CTOX_UPSTREAM_BASE_URL".to_string(),
            "https://llm.ctox.dev".to_string(),
        );

        let spec =
            resolve_api_model_provider_spec("MiniMax-M3", &settings, None).expect("provider spec");

        assert_eq!(spec.provider_id, "ctox_core_api");
        assert_eq!(spec.base_url, "https://llm.ctox.dev/v1");
        assert_eq!(spec.env_key, runtime_state::CTOX_LLM_PROXY_API_KEY_ENV);
        assert_eq!(spec.wire_api, "responses");
        assert!(!spec.requires_full_responses_history);
    }

    #[test]
    fn kimi_k3_proxy_settings_resolve_core_api_provider() {
        let mut settings = BTreeMap::new();
        settings.insert("CTOX_API_PROVIDER".to_string(), "ctox_proxy".to_string());
        settings.insert(
            "CTOX_UPSTREAM_BASE_URL".to_string(),
            "https://llm.ctox.dev".to_string(),
        );

        let spec =
            resolve_api_model_provider_spec("kimi-k3", &settings, None).expect("provider spec");

        assert_eq!(spec.provider_id, "ctox_core_api");
        assert_eq!(spec.base_url, "https://llm.ctox.dev/v1");
        assert_eq!(spec.env_key, runtime_state::CTOX_LLM_PROXY_API_KEY_ENV);
        assert_eq!(spec.wire_api, "responses");
        assert!(!spec.requires_full_responses_history);
    }

    #[test]
    fn remote_api_provider_uses_remote_timeout_even_behind_local_process() {
        assert_eq!(
            default_chat_turn_timeout_secs(true, true),
            DEFAULT_REMOTE_CHAT_TURN_TIMEOUT_SECS
        );
        assert_eq!(
            default_chat_turn_timeout_secs(true, false),
            DEFAULT_LOCAL_CHAT_TURN_TIMEOUT_SECS
        );
        assert_eq!(
            default_chat_turn_timeout_secs(false, false),
            DEFAULT_REMOTE_CHAT_TURN_TIMEOUT_SECS
        );
    }

    #[test]
    fn current_prompt_fallback_preserves_authoritative_prompt() {
        let rendered = "CURRENT REQUEST\n- User asked:\n\nRECENT CONVERSATION EVIDENCE\n- none\n";
        let prompt = "Do the queued work in /tmp/worktree.";

        let fallback = render_current_prompt_fallback(rendered, prompt);

        assert!(fallback.starts_with("CURRENT REQUEST (authoritative)\n"));
        assert!(fallback.contains(prompt));
        assert!(fallback.contains(rendered.trim_start()));
    }

    #[test]
    fn communication_refresh_is_durable_due_on_eighth_successful_turn() -> Result<()> {
        let db_path = std::env::temp_dir().join(format!(
            "ctox-refresh-eight-{}-{}.sqlite",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;
        for turn in 1..COMMUNICATION_REFRESH_TURN_LIMIT {
            let due = record_durable_refresh_demand(
                &db_path,
                77,
                false,
                0,
                120,
                &format!("communication-turn:{turn}"),
            )?;
            assert!(due.is_empty());
        }
        let due =
            record_durable_refresh_demand(&db_path, 77, false, 0, 120, "communication-turn:8")?;
        assert!(due.contains("narrative"));
        assert!(due.contains("anchors"));
        assert!(!due.contains("focus"));

        mark_durable_refresh_failed(&db_path, 77, "narrative", "model unavailable")?;
        let conn = rusqlite::Connection::open(&db_path)?;
        let (status, attempts, error): (String, i64, String) = conn.query_row(
            "SELECT status, failure_attempt_count, last_error FROM continuity_refresh_status WHERE conversation_id=77 AND continuity_kind='narrative'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(status, "pending");
        assert_eq!(attempts, 1);
        assert_eq!(error, "model unavailable");
        conn.execute(
            "UPDATE continuity_refresh_status SET retry_not_before='2000-01-01T00:00:00Z' WHERE conversation_id=77 AND continuity_kind='narrative'",
            [],
        )?;
        let due_after_restart = due_refresh_kinds(&conn, 77, &current_rfc3339_timestamp())?;
        assert!(due_after_restart.contains("narrative"));
        assert!(
            mark_durable_refresh_consumed(&db_path, 77, "narrative", "head-a", "head-a").is_err()
        );

        drop(conn);
        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn durable_refresh_waits_for_a_short_parallel_writer() -> Result<()> {
        let db_path = std::env::temp_dir().join(format!(
            "ctox-refresh-lock-{}-{}.sqlite",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;
        record_durable_refresh_demand(&db_path, 91, false, 0, 10, "initial")?;

        let holder = rusqlite::Connection::open(&db_path)?;
        holder.execute_batch("BEGIN IMMEDIATE")?;
        let worker_path = db_path.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            started_tx.send(()).expect("signal refresh worker start");
            record_durable_refresh_demand(&worker_path, 91, false, 0, 10, "parallel")
        });
        started_rx.recv()?;
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            !worker.is_finished(),
            "refresh accounting must wait for a short writer instead of failing immediately"
        );
        holder.execute_batch("COMMIT")?;
        worker.join().expect("refresh worker panicked")?;

        let conn = rusqlite::Connection::open(&db_path)?;
        let turns: i64 = conn.query_row(
            "SELECT successful_turn_count FROM continuity_refresh_status WHERE conversation_id=91 AND continuity_kind='narrative'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(turns, 2);
        drop(conn);
        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn heuristic_api_overflow_marker_gets_a_cooldown() {
        // turnloop-4: the API-runtime heuristic preflight bails with a
        // distinctly-named marker. It must be classified as a hard runtime
        // blocker with a cooldown, exactly like the exact/context bails,
        // otherwise an overflowing API turn retries with no backoff.
        let bail = format!(
            "context_preflight_heuristic_overflow: estimated rendered prompt tokens {} exceed heuristic input budget {} for context window {} via heuristic-api",
            200_000, 124_640, 131_200
        );
        assert_eq!(hard_runtime_blocker_retry_cooldown_secs(&bail), Some(60));

        // The estimate helper this preflight relies on must be reachable and
        // monotone in input size (this is what makes the budget comparison
        // meaningful).
        let small = lcm::estimate_tokens("hi");
        let large = lcm::estimate_tokens(&"x".repeat(8_000));
        assert!(large > small);
        assert!(large >= 2_000); // ~8000 chars / 4
    }

    #[test]
    fn max_output_tokens_disconnect_gets_retry_cooldown() {
        let error = "direct session error: stream disconnected before completion: Incomplete response returned, reason: max_output_tokens";

        assert_eq!(hard_runtime_blocker_retry_cooldown_secs(error), Some(60));
    }

    fn test_message(message_id: i64, seq: i64, role: &str, content: &str) -> lcm::MessageRecord {
        lcm::MessageRecord {
            message_id,
            conversation_id: 9,
            seq,
            role: role.to_string(),
            content: content.to_string(),
            token_count: 1,
            created_at: "2026-05-11T00:00:00Z".to_string(),
            agent_outcome: None,
        }
    }

    #[test]
    fn empty_rendered_context_is_harness_error_when_history_exists() {
        let first_turn = lcm::LcmSnapshot {
            conversation_id: 9,
            messages: vec![test_message(1, 1, "user", "first task")],
            summaries: Vec::new(),
            context_items: Vec::new(),
            summary_edges: Vec::new(),
            summary_messages: Vec::new(),
        };
        assert!(!rendered_context_empty_with_existing_history(
            &first_turn,
            0
        ));

        let continued_turn = lcm::LcmSnapshot {
            conversation_id: 9,
            messages: vec![
                test_message(1, 1, "user", "first task"),
                test_message(2, 2, "assistant", "completed first task"),
                test_message(3, 3, "user", "second task"),
            ],
            summaries: Vec::new(),
            context_items: Vec::new(),
            summary_edges: Vec::new(),
            summary_messages: Vec::new(),
        };
        assert!(rendered_context_empty_with_existing_history(
            &continued_turn,
            0
        ));
        assert!(!rendered_context_empty_with_existing_history(
            &continued_turn,
            1
        ));
    }

    #[test]
    fn critical_context_selection_allows_authoritative_current_prompt() {
        let rendered = live_context::RenderedRuntimePrompt {
            prompt: "CURRENT REQUEST (authoritative)\nDo the queued work.".to_string(),
            latest_user_prompt: "Do the queued work.".to_string(),
            context_instructions: String::new(),
            rendered_context_items: 0,
            omitted_context_items: 0,
        };
        assert!(!critical_context_selection_is_empty(&rendered));

        let empty = live_context::RenderedRuntimePrompt {
            prompt: "CURRENT REQUEST\n".to_string(),
            latest_user_prompt: "   ".to_string(),
            context_instructions: String::new(),
            rendered_context_items: 0,
            omitted_context_items: 0,
        };
        assert!(critical_context_selection_is_empty(&empty));
    }

    #[test]
    fn invocable_guard_accepts_clipped_echo_of_oversized_prompt() {
        // A prompt over the echo budget renders clipped; the guard must
        // compare against that clipped form instead of prepending the full
        // raw prompt again (which duplicated the first 8k chars per turn).
        let oversized = format!("execute the long task. {}", "x".repeat(9_000));
        let rendered_echo = live_context::sanitize_latest_prompt(&oversized);
        assert!(rendered_echo.len() < oversized.len());
        let mut rendered = live_context::RenderedRuntimePrompt {
            prompt: format!("CURRENT REQUEST\n- User asked: {rendered_echo}\n"),
            latest_user_prompt: oversized.clone(),
            context_instructions: String::new(),
            rendered_context_items: 1,
            omitted_context_items: 0,
        };
        let snapshot = lcm::LcmSnapshot {
            conversation_id: 9,
            messages: Vec::new(),
            summaries: Vec::new(),
            context_items: Vec::new(),
            summary_edges: Vec::new(),
            summary_messages: Vec::new(),
        };
        let health = context_health::ContextHealthSnapshot {
            conversation_id: 9,
            overall_score: 90,
            status: context_health::ContextHealthStatus::Healthy,
            summary: "healthy".to_string(),
            repair_recommended: false,
            dimensions: Vec::new(),
            warnings: Vec::new(),
        };
        let before = rendered.prompt.clone();
        ensure_rendered_prompt_is_invocable(
            &snapshot,
            &mut rendered,
            &oversized,
            &health,
            &mut |_event| {},
        )
        .expect("guard must accept the clipped echo");
        assert_eq!(
            rendered.prompt, before,
            "guard must not prepend the raw prompt when the clipped echo is present"
        );
    }

    #[test]
    fn invocable_guard_bails_on_conjunctive_critical_loop() {
        let prompt = "retry the same blocked task";
        let rendered_echo = live_context::sanitize_latest_prompt(prompt);
        let make_rendered = || live_context::RenderedRuntimePrompt {
            prompt: format!("CURRENT REQUEST\n- User asked: {rendered_echo}\n"),
            latest_user_prompt: prompt.to_string(),
            context_instructions: String::new(),
            rendered_context_items: 1,
            omitted_context_items: 0,
        };
        let snapshot = lcm::LcmSnapshot {
            conversation_id: 9,
            messages: Vec::new(),
            summaries: Vec::new(),
            context_items: Vec::new(),
            summary_edges: Vec::new(),
            summary_messages: Vec::new(),
        };
        let warning = |code: &str, severity: context_health::WarningSeverity| {
            context_health::ContextHealthWarning {
                code: code.to_string(),
                severity,
                summary: String::new(),
                evidence: String::new(),
                recommended_action: String::new(),
            }
        };
        let make_health = |warnings: Vec<context_health::ContextHealthWarning>| {
            context_health::ContextHealthSnapshot {
                conversation_id: 9,
                overall_score: 10,
                status: context_health::ContextHealthStatus::Critical,
                summary: "critical".to_string(),
                repair_recommended: true,
                dimensions: Vec::new(),
                warnings,
            }
        };
        let run = |warnings: Vec<context_health::ContextHealthWarning>| {
            let mut rendered = make_rendered();
            ensure_rendered_prompt_is_invocable(
                &snapshot,
                &mut rendered,
                prompt,
                &make_health(warnings),
                &mut |_event| {},
            )
        };

        use context_health::WarningSeverity::{Critical, Warning};

        // POSITIVE: both Critical loop facts present -> deterministic short-circuit.
        let err = run(vec![
            warning("recent_user_turn_repeated", Critical),
            warning("blocked_status_loop", Critical),
        ])
        .unwrap_err();
        assert!(err.to_string().contains("context_loop_short_circuit"));

        // NEGATIVE: only one of the two facts -> still invocable.
        assert!(run(vec![warning("recent_user_turn_repeated", Critical)]).is_ok());
        assert!(run(vec![warning("blocked_status_loop", Critical)]).is_ok());
        // NEGATIVE: both present but only Warning severity -> still invocable.
        assert!(run(vec![
            warning("recent_user_turn_repeated", Warning),
            warning("blocked_status_loop", Warning),
        ])
        .is_ok());

        // The marker cools down like the other context bails (60s).
        assert_eq!(
            hard_runtime_blocker_retry_cooldown_secs(
                "context_loop_short_circuit: exact-duplicate user turn re-entering an N-deep structured-failure loop with no new evidence"
            ),
            Some(60)
        );
    }

    #[test]
    fn focus_commit_boundary_detection_compares_epoch_millis() {
        // continuity_commits.created_at is an epoch-millis string; the
        // RFC3339 turn timestamp must be converted before comparing,
        // otherwise text collation makes the check permanently false.
        let root = std::env::temp_dir().join(format!(
            "ctox-turn-boundary-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        let db_path = crate::paths::lcm_db(&root);
        let turn_start = current_rfc3339_timestamp();
        {
            let engine =
                lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default()).expect("open lcm engine");
            engine
                .continuity_full_replace_document(
                    7,
                    lcm::ContinuityKind::Focus,
                    "## Status\n- Mission state: open\n",
                )
                .expect("apply focus document");
        }
        let detected =
            detect_durable_state_transition(&root, &db_path, 7, &turn_start, &turn_start)
                .expect("detection query must run")
                .detected;
        assert!(
            detected,
            "a focus commit after turn start must register as a durable boundary"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn durable_state_probe_surfaces_a_query_error_instead_of_suppressing() {
        // turnloop-6: a probe whose query ERRORS (e.g. a schema regression
        // renames a column or drops a table) must be reported via probe_errors,
        // not collapsed into "no boundary" — otherwise a query regression
        // silently stops forcing continuity refreshes.
        let root = std::env::temp_dir().join(format!(
            "ctox-turn-probe-err-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        // Create the mission db so it EXISTS but WITHOUT ticket_self_work_items,
        // so the first probe query errors with "no such table".
        let mission_db = crate::persistence::sqlite_path(&root);
        std::fs::create_dir_all(mission_db.parent().unwrap()).unwrap();
        rusqlite::Connection::open(&mission_db)
            .unwrap()
            .execute_batch("CREATE TABLE unrelated (x INTEGER);")
            .unwrap();
        let turn_start = current_rfc3339_timestamp();
        let absent_lcm = root.join("no-such-lcm.sqlite");
        let probe =
            detect_durable_state_transition(&root, &absent_lcm, 7, &turn_start, &turn_start)
                .expect("probe must not hard-error on a missing table");
        assert!(
            !probe.detected,
            "a degraded probe must not claim a boundary"
        );
        assert!(
            !probe.probe_errors.is_empty(),
            "a missing-table probe error must be surfaced, not swallowed to no-boundary"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn boundary_window_survives_process_local_state_loss() -> Result<()> {
        let db_path = std::env::temp_dir().join(format!(
            "ctox-refresh-window-{}-{}.sqlite",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _engine = lcm::LcmEngine::open(&db_path, lcm::LcmConfig::default())?;
        let conversation_id = 991_001;
        let first_turn = "2026-06-10T10:00:00+00:00";
        assert_eq!(
            durable_boundary_window_start(&db_path, conversation_id, first_turn),
            first_turn
        );
        record_durable_refresh_demand(
            &db_path,
            conversation_id,
            false,
            0,
            10,
            "communication-turn:first",
        )?;
        let persisted =
            durable_boundary_window_start(&db_path, conversation_id, "2099-01-01T00:00:00+00:00");
        assert_ne!(persisted, "2099-01-01T00:00:00+00:00");
        // A second fresh SQLite connection observes the same durable window,
        // which models a daemon restart without relying on process globals.
        assert_eq!(
            durable_boundary_window_start(&db_path, conversation_id, "2099-02-01T00:00:00+00:00"),
            persisted
        );
        let _ = std::fs::remove_file(db_path);
        Ok(())
    }
}
