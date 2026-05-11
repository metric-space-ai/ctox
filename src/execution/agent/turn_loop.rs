use anyhow::Context;
use anyhow::Result;
use sha2::Digest;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

// Re-export PersistentSession so callers (main.rs, service.rs) can hold one.
pub(crate) use super::direct_session::PersistentSession;
use super::direct_session::TerminalBenchPreflightSpec;
use std::sync::OnceLock;
use std::time::Duration;
use toml::Value as TomlValue;

use crate::inference::supervisor;

/// Per-conversation refresh accounting since the last continuity refresh.
/// Lives in process memory so that restarts do not preserve it — that is
/// fine: a restart always starts a fresh budget window.
#[derive(Default, Clone, Copy)]
struct RefreshState {
    /// Cumulative assistant reply characters since the last refresh.
    /// This is telemetry only. It must never be converted into token counts
    /// for control flow; compaction decisions use reported TokenCount events.
    output_chars_since_refresh: u64,
    /// Turns since the last refresh (used only by the optional legacy
    /// interval trigger when the operator explicitly sets one).
    turns_since_refresh: u64,
}

fn turn_counters() -> &'static Mutex<HashMap<i64, RefreshState>> {
    static COUNTERS: OnceLock<Mutex<HashMap<i64, RefreshState>>> = OnceLock::new();
    COUNTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Decide whether the current turn should run a continuity refresh.
///
/// New adaptive model — two passive triggers plus one hard safety net:
///
/// 1. `force_task_boundary` — durable state transition (plan step closed,
///    self-work closed, focus replace). Refreshes immediately and resets
///    all counters. This is the state-transition trigger.
///
/// 2. Legacy interval trigger (`legacy_every_n_turns`) — optional,
///    disabled by default (0). Preserves backward compatibility for
///    operators who explicitly set `CTOX_CONTINUITY_REFRESH_EVERY_N_TURNS`.
///
/// When none of the triggers fire, the turn runs without a continuity
/// refresh. Token-window safety is handled independently by the compact
/// policy from actual TokenCount telemetry.
fn should_refresh_continuity(
    conversation_id: i64,
    reply_output_chars: u64,
    legacy_every_n_turns: u64,
    force_task_boundary: bool,
) -> bool {
    let mut counters = turn_counters().lock().expect("turn_counters poisoned");
    let state = counters
        .entry(conversation_id)
        .or_insert(RefreshState::default());
    state.output_chars_since_refresh = state
        .output_chars_since_refresh
        .saturating_add(reply_output_chars);
    state.turns_since_refresh = state.turns_since_refresh.saturating_add(1);

    let interval_hit =
        legacy_every_n_turns > 0 && state.turns_since_refresh >= legacy_every_n_turns;
    let should_refresh = force_task_boundary || interval_hit;

    if should_refresh {
        state.output_chars_since_refresh = 0;
        state.turns_since_refresh = 0;
    }
    should_refresh
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
    let state = counters.get(&conversation_id).copied().unwrap_or_default();
    RefreshBudgetSnapshot {
        output_chars_since_refresh: state.output_chars_since_refresh,
        turns_since_refresh: state.turns_since_refresh,
    }
}

/// Query the mission and LCM databases for durable state changes written
/// between `turn_start_ts` and now. Returns `true` if any of the following
/// happened during the turn:
///
/// - a self-work item transitioned to `state = 'closed'`
/// - a new ticket-knowledge entry was inserted
/// - a focus continuity commit was written
///
/// Any error (missing DB, missing table on a fresh install) is swallowed
/// as `Ok(false)` by the caller. The explicit interval trigger can still be
/// enabled by operators; token-window safety is handled by actual TokenCount
/// telemetry in the compact policy.
fn detect_durable_state_transition(
    root: &Path,
    lcm_db_path: &Path,
    conversation_id: i64,
    turn_start_ts: &str,
) -> Result<bool> {
    use rusqlite::Connection;

    // Mission-side tables live in the unified CTOX runtime database.
    let mission_db = crate::persistence::sqlite_path(root);
    if mission_db.exists() {
        let conn = Connection::open_with_flags(
            &mission_db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )?;
        let self_work_closed: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM ticket_self_work_items \
                 WHERE state = 'closed' AND updated_at > ?1",
                rusqlite::params![turn_start_ts],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if self_work_closed > 0 {
            return Ok(true);
        }
        let knowledge_added: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM ticket_knowledge_entries WHERE created_at > ?1",
                rusqlite::params![turn_start_ts],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if knowledge_added > 0 {
            return Ok(true);
        }
    }

    // Focus-document commits live in the LCM database alongside Narrative
    // and Anchors. A focus replacement during the turn is a boundary.
    if lcm_db_path.exists() {
        let conn = Connection::open_with_flags(
            lcm_db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )?;
        let focus_commits: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM continuity_commits c \
                 JOIN continuity_documents d ON c.document_id = d.id \
                 WHERE d.conversation_id = ?1 AND d.kind = 'Focus' \
                 AND c.created_at > ?2",
                rusqlite::params![conversation_id, turn_start_ts],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if focus_commits > 0 {
            return Ok(true);
        }
    }

    Ok(false)
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
// terminal-bench tasks (SWE-Bench, large workspaces) hit
// "direct session timeout after 900s" mid-turn, the persistent session is
// dropped and the ggml backend is killed — leaving leases held and 0 passes.
// 3600 s gives long-running tasks room while still bounding genuinely stuck
// turns. Operators can override via CTOX_CHAT_TURN_TIMEOUT_SECS.
const DEFAULT_LOCAL_CHAT_TURN_TIMEOUT_SECS: u64 = 3600;
const CONTINUITY_REFRESH_FAULT_FILE_ENV_KEY: &str = "CTOX_CONTINUITY_REFRESH_FAULT_FILE";
const CONTINUITY_REFRESH_TIMEOUT_ENV_KEY: &str = "CTOX_CONTINUITY_REFRESH_TIMEOUT_SECS";

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
    run_chat_turn_with_events_extended_guarded(
        root,
        db_path,
        prompt,
        _workspace_root,
        conversation_id,
        suggested_skill,
        force_continuity_refresh,
        None,
        true,
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
    terminal_bench_preflight: Option<TerminalBenchPreflightSpec>,
    infer_terminal_bench_preflight_from_prompt: bool,
    mut session: Option<&mut PersistentSession>,
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
        Some(PersistentSession::start(root, &operator_settings)?)
    } else {
        None
    };
    emit("session-ready");
    let default_turn_timeout_secs = if runtime.state.source.is_local() {
        DEFAULT_LOCAL_CHAT_TURN_TIMEOUT_SECS
    } else {
        DEFAULT_REMOTE_CHAT_TURN_TIMEOUT_SECS
    };
    let config = turn_engine::ChatTurnConfig {
        max_context_tokens: runtime.turn_context_tokens(),
        turn_timeout_secs: read_usize_setting(
            &operator_settings,
            "CTOX_CHAT_TURN_TIMEOUT_SECS",
            default_turn_timeout_secs as usize,
        ) as u64,
    };
    emit("lcm-open");
    let engine = lcm::LcmEngine::open(db_path, lcm::LcmConfig::default())?;
    let _ = engine.continuity_init_documents(conversation_id)?;
    emit("persist-user-turn");
    persist_lcm_message_with_retry(db_path, conversation_id, "user", prompt, &mut emit)
        .context("failed to persist user message into LCM")?;
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
        let result = engine.compact(
            conversation_id,
            config.max_context_tokens,
            &lcm::HeuristicSummarizer,
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
    let snapshot = engine.snapshot(conversation_id)?;
    let continuity = engine.continuity_show_all(conversation_id)?;
    let mission_state = engine.mission_state(conversation_id)?;
    let mission_assurance = engine.mission_assurance_snapshot(conversation_id)?;
    let forgotten_entries = engine.continuity_forgotten(conversation_id, None, None)?;
    let health = context_health::assess_with_forgotten(
        &snapshot,
        &continuity,
        &forgotten_entries,
        prompt,
        config.max_context_tokens,
    );
    let governance_snapshot =
        governance::prompt_snapshot(root, conversation_id).unwrap_or_default();
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
        &governance_snapshot,
        &health,
        suggested_skill,
    )?;
    let current_prompt = prompt.trim();
    let latest_empty = rendered_prompt.latest_user_prompt.trim().is_empty();
    let missing_current_prompt =
        !current_prompt.is_empty() && !rendered_prompt.prompt.contains(current_prompt);
    if latest_empty || missing_current_prompt {
        emit(&format!(
            "context-selection fallback-current-prompt latest_empty={} missing_current={}",
            latest_empty, missing_current_prompt
        ));
        rendered_prompt.prompt = render_current_prompt_fallback(&rendered_prompt.prompt, prompt);
        rendered_prompt.latest_user_prompt = prompt.to_string();
    }
    if rendered_context_empty_with_existing_history(
        &snapshot,
        rendered_prompt.rendered_context_items,
    ) {
        anyhow::bail!(
            "context_selection_empty: refusing model invocation because LCM history exists but no context evidence rendered"
        );
    }
    if health.status == context_health::ContextHealthStatus::Critical
        && critical_context_selection_is_empty(&rendered_prompt)
    {
        anyhow::bail!(
            "context_selection_empty_critical: refusing model invocation because context health is critical and no context evidence rendered"
        );
    }
    emit(&format!(
        "context-selection rendered={} omitted={}",
        rendered_prompt.rendered_context_items, rendered_prompt.omitted_context_items
    ));
    let turn_start_ts = current_rfc3339_timestamp();
    emit("invoke-model");
    let reply = match session.as_deref_mut() {
        Some(sess) => sess.run_turn_with_terminal_bench_preflight(
            &rendered_prompt.prompt,
            Some(Duration::from_secs(config.turn_timeout_secs)),
            terminal_bench_preflight.clone(),
            infer_terminal_bench_preflight_from_prompt,
        )?,
        None => owned_session
            .as_mut()
            .expect("owned persistent session should exist when no session was supplied")
            .run_turn_with_terminal_bench_preflight(
                &rendered_prompt.prompt,
                Some(Duration::from_secs(config.turn_timeout_secs)),
                terminal_bench_preflight,
                infer_terminal_bench_preflight_from_prompt,
            )?,
    };
    emit("persist-assistant-turn");
    persist_lcm_message_with_retry(db_path, conversation_id, "assistant", &reply, &mut emit)?;
    // Detect durable state transitions triggered by the agent's tool calls
    // during this turn (self-work closed, knowledge entry added, focus
    // document replaced). These count as task boundaries and force a
    // continuity refresh even if the output budget has not yet been hit.
    let state_transition_detected =
        detect_durable_state_transition(root, db_path, conversation_id, &turn_start_ts)
            .unwrap_or(false);
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
    let refresh_now = should_refresh_continuity(
        conversation_id,
        reply_chars,
        refresh_every_n,
        effective_force_refresh,
    );
    let continuity_stats = if refresh_now {
        let reason = if force_continuity_refresh {
            "state-transition-plan"
        } else if state_transition_detected {
            "state-transition-tickets"
        } else {
            "turn-interval"
        };
        emit(&format!("continuity-refresh reason={}", reason));
        match session.as_deref_mut() {
            Some(refresh_session) => refresh_continuity_documents(
                root,
                &operator_settings,
                &engine,
                conversation_id,
                refresh_session,
                &mut emit,
            )?,
            None => refresh_continuity_documents(
                root,
                &operator_settings,
                &engine,
                conversation_id,
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
    engine: &lcm::LcmEngine,
    conversation_id: i64,
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
        stats.attempted += 1;
        emit(&format!("continuity-{kind_label}-build"));
        let payload = match engine.continuity_build_prompt(conversation_id, kind) {
            Ok(payload) => payload,
            Err(err) => {
                stats.skipped_prompt_build += 1;
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
                        eprintln!(
                            "ctox continuity refresh skipped invalid injected {kind_label} diff: {err}"
                        );
                    } else {
                        stats.updated += 1;
                    }
                }
                if kind == lcm::ContinuityKind::Anchors {
                    let _ = engine.continuity_preserve_recent_anchor_literals(conversation_id);
                }
                continue;
            }
            Ok(None) => {}
            Err(err) => {
                stats.skipped_invoke += 1;
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
        } else {
            eprintln!(
                "ctox continuity refresh {kind_label}: no tool-driven change (reply preview: {})",
                summarize_continuity_diff_for_log(&reply)
            );
        }

        if kind == lcm::ContinuityKind::Anchors {
            emit("continuity-anchors-preserve-literals");
            match engine.continuity_preserve_recent_anchor_literals(conversation_id) {
                Ok(Some(_)) => stats.updated += 1,
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
    // (env_key, default_provider_for_url, wire_api)
    let (env_key, default_provider, wire_api) = match normalized.as_str() {
        "anthropic" => ("ANTHROPIC_API_KEY", "anthropic", "anthropic_messages"),
        "openrouter" => ("OPENROUTER_API_KEY", "openrouter", "responses"),
        "minimax" => ("MINIMAX_API_KEY", "minimax", "responses"),
        "azure_foundry" => ("AZURE_FOUNDRY_API_KEY", "azure_foundry", "responses"),
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
        || lower.contains("terminal-bench preflight violation")
        || lower.contains("mid-task compaction failed")
        || lower.contains("failed to parse structured compaction response")
        || lower.contains("exceed_context_size_error")
        || lower.contains("exceeds the available context size")
    {
        return Some(60);
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
        assert!(!overrides.contains_key("model_providers.ctox_core_api.env_key"));
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
            rendered_context_items: 0,
            omitted_context_items: 0,
        };
        assert!(!critical_context_selection_is_empty(&rendered));

        let empty = live_context::RenderedRuntimePrompt {
            prompt: "CURRENT REQUEST\n".to_string(),
            latest_user_prompt: "   ".to_string(),
            rendered_context_items: 0,
            omitted_context_items: 0,
        };
        assert!(critical_context_selection_is_empty(&empty));
    }
}
