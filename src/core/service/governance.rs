use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// DB paths whose governance schema + default-mechanism upsert has already
/// run in this process. Skipped on subsequent opens — the daemon owns the
/// authoritative writes; the TUI's repeated `prompt_snapshot` calls just
/// need to read.
fn initialized_governance_paths() -> &'static Mutex<HashSet<PathBuf>> {
    static INITIALIZED: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    INITIALIZED.get_or_init(|| Mutex::new(HashSet::new()))
}

const DEFAULT_DB_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";
const DEFAULT_EVENT_LIMIT: usize = 8;

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceMechanismRecord {
    pub mechanism_id: String,
    pub mechanism_class: String,
    pub autonomy: String,
    pub description: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceEventRecord {
    pub event_id: String,
    pub mechanism_id: String,
    pub mechanism_class: String,
    pub conversation_id: Option<i64>,
    pub severity: String,
    pub reason: String,
    pub action_taken: String,
    pub details: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct GovernancePromptSnapshot {
    pub mechanisms: Vec<GovernanceMechanismRecord>,
    pub recent_events: Vec<GovernanceEventRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceMechanismInventoryRecord {
    pub mechanism_id: String,
    pub mechanism_class: String,
    pub intervention_mode: String,
    pub prompt_visibility: String,
    pub module_hint: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct GovernanceEventRequest<'a> {
    pub mechanism_id: &'a str,
    pub conversation_id: Option<i64>,
    pub severity: &'a str,
    pub reason: &'a str,
    pub action_taken: &'a str,
    pub details: Value,
    pub idempotence_key: Option<&'a str>,
}

struct DefaultMechanism {
    mechanism_id: &'static str,
    mechanism_class: &'static str,
    autonomy: &'static str,
    prompt_visibility: &'static str,
    module_hint: &'static str,
    description: &'static str,
}

const DEFAULT_MECHANISMS: &[DefaultMechanism] = &[
    DefaultMechanism {
        mechanism_id: "queue_pressure_guard",
        mechanism_class: "survival",
        autonomy: "autonomous_queue_containment",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service.rs",
        description: "Prevents CTOX from self-flooding when pending prompt pressure crosses the configured queue threshold.",
    },
    DefaultMechanism {
        mechanism_id: "runtime_blocker_backoff",
        mechanism_class: "survival",
        autonomy: "autonomous_retry_backoff",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service.rs",
        description: "Delays new prompt dispatch while a hard runtime blocker cooldown is still active, so CTOX does not thrash the same broken runtime surface.",
    },
    DefaultMechanism {
        mechanism_id: "turn_timeout_continuation",
        mechanism_class: "survival",
        autonomy: "autonomous_timeout_containment",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service.rs",
        description: "Records a timed-out turn and suppresses recursive timeout-continuation spawning; retry/defer decisions must stay inside the original queue or mission state.",
    },
    DefaultMechanism {
        mechanism_id: "fatal_harness_loop_guard",
        mechanism_class: "survival",
        autonomy: "autonomous_loop_kill_switch",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service.rs",
        description: "Cancels legacy or externally injected timeout-continuation queue jobs before an agent turn starts, preventing recursive restart loops that burn tokens without progress.",
    },
    DefaultMechanism {
        mechanism_id: "agent_failure_threshold",
        mechanism_class: "survival",
        autonomy: "autonomous_failure_circuit_breaker",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service.rs",
        description: "Defers a mission after repeated structured agent failures so the service stops retrying the same failing work item blindly.",
    },
    DefaultMechanism {
        mechanism_id: "agent_failure_recovery",
        mechanism_class: "recovery",
        autonomy: "autonomous_failure_circuit_breaker",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service.rs",
        description: "Records that a previously deferred mission recovered after a successful agent turn, clearing the agent-failure deferral and reopening the mission, so the defer/recover audit pair is two-sided.",
    },
    DefaultMechanism {
        mechanism_id: "runtime_api_retry_continuation",
        mechanism_class: "survival",
        autonomy: "autonomous_retry_backoff",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service.rs",
        description: "Records transient runtime/API retry deferrals with explicit backoff rather than allowing hidden immediate respawns.",
    },
    DefaultMechanism {
        mechanism_id: "sender_authority_boundary",
        mechanism_class: "safety",
        autonomy: "autonomous_input_block",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service.rs",
        description: "Blocks inbound work from unauthorized email senders instead of letting unsafe requests enter the active loop.",
    },
    DefaultMechanism {
        mechanism_id: "secret_input_boundary",
        mechanism_class: "safety",
        autonomy: "autonomous_input_block",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service.rs",
        description: "Rejects secret-bearing email input and forces secret exchange back to the local TUI.",
    },
    DefaultMechanism {
        mechanism_id: "context_health_assessment",
        mechanism_class: "advisory",
        autonomy: "advisory_diagnostic",
        prompt_visibility: "prompt_visible",
        module_hint: "src/context_health.rs",
        description: "Scores repetition, blocked-loop risk, mission thinness, and memory drift so CTOX can decide whether cleanup or replanning is warranted.",
    },
    DefaultMechanism {
        mechanism_id: "context_health_repair_governor",
        mechanism_class: "advisory",
        autonomy: "advisory_governor",
        prompt_visibility: "inventory_only",
        module_hint: "src/context_health.rs",
        description: "Suggests whether a bounded context-health repair slice would be justified, but does not autonomously enqueue that repair work.",
    },
    DefaultMechanism {
        mechanism_id: "state_invariant_guard",
        mechanism_class: "safety",
        autonomy: "autonomous_state_integrity_repair",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service/state_invariants.rs",
        description: "Surfaces contradictions between stored mission state, continuity, and durable runtime work, and may perform a narrow recorded continuity/mission-state repair at boot or immediately after a turn when that repair is surfaced as a visible governance event.",
    },
    DefaultMechanism {
        mechanism_id: "mission_loop_governor",
        mechanism_class: "advisory",
        autonomy: "advisory_governor",
        prompt_visibility: "inventory_only",
        module_hint: "src/mission_governor.rs",
        description: "Detects repeated-blocker loop patterns and proposes a repair or replan slice instead of another blind retry, but remains advisory.",
    },
    DefaultMechanism {
        mechanism_id: "follow_up_evaluate",
        mechanism_class: "explicit_tool",
        autonomy: "explicit_decision_tool",
        prompt_visibility: "inventory_only",
        module_hint: "src/follow_up.rs",
        description: "Turns explicit blocker, open-item, and review inputs into a durable follow-up decision without inferring hidden status from prose.",
    },
    DefaultMechanism {
        mechanism_id: "completion_review",
        mechanism_class: "explicit_tool",
        autonomy: "explicit_read_only_review",
        prompt_visibility: "inventory_only",
        module_hint: "src/review.rs",
        description: "Runs a separate read-only completion review when explicitly requested, instead of acting as a hidden gate in the main service loop.",
    },
    DefaultMechanism {
        mechanism_id: "verification_assurance",
        mechanism_class: "explicit_tool",
        autonomy: "explicit_evidence_tracking",
        prompt_visibility: "inventory_only",
        module_hint: "src/verification.rs",
        description: "Persists verification runs and mission claims when explicitly invoked, so CTOX can track evidence-bearing closure state across slices.",
    },
    DefaultMechanism {
        mechanism_id: "ticket_control_gate",
        mechanism_class: "safety",
        autonomy: "autonomous_ticket_control_gate",
        prompt_visibility: "prompt_visible",
        module_hint: "src/mission/tickets.rs",
        description: "Prevents ticket work from entering the active loop unless label binding, dry-run controls, and bundle-gated execution state are all explicit and audit-ready.",
    },
    DefaultMechanism {
        mechanism_id: "ticket_knowledge_gate",
        mechanism_class: "safety",
        autonomy: "autonomous_ticket_knowledge_gate",
        prompt_visibility: "prompt_visible",
        module_hint: "src/mission/tickets.rs",
        description: "Prevents synchronized ticket work from entering the active loop until required source knowledge domains have been loaded.",
    },
    DefaultMechanism {
        mechanism_id: "ticket_dispatch_preflight",
        mechanism_class: "safety",
        autonomy: "autonomous_ticket_dispatch_guard",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service.rs",
        description: "Checks configured ticket adapters and runtime prerequisites before synchronized ticket work is dispatched.",
    },
    DefaultMechanism {
        mechanism_id: "ticket_adapter_sync",
        mechanism_class: "recovery",
        autonomy: "autonomous_ticket_sync_recording",
        prompt_visibility: "prompt_visible",
        module_hint: "src/mission/tickets.rs",
        description: "Records adapter sync failures as durable governance and ticket sync state instead of hiding them in service logs.",
    },
    DefaultMechanism {
        mechanism_id: "ticket_reconciliation",
        mechanism_class: "recovery",
        autonomy: "autonomous_ticket_reconciliation",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service.rs",
        description: "Reconciles stale ticket leases and previously blocked ticket events before new external routing.",
    },
    DefaultMechanism {
        mechanism_id: "stuck_lease_escalation",
        mechanism_class: "recovery",
        autonomy: "autonomous_ticket_reconciliation",
        prompt_visibility: "inventory_only",
        module_hint: "src/service.rs",
        description: "Escalates a queue-task lease held past the stuck-lease window by an in-process worker (protected from auto-release) as durable evidence, so the deadlock-recovery audit trail exists instead of only an advisory process-mining finding.",
    },
    DefaultMechanism {
        mechanism_id: "plan_goal_superseded_for_duplicate_slice",
        mechanism_class: "safety",
        autonomy: "autonomous_plan_goal_supersede",
        prompt_visibility: "inventory_only",
        module_hint: "src/mission/plan.rs",
        description: "Marks an older active planned_goal as superseded when a fresh `ctox plan ingest` arrives on the same thread_key, so two competing live goals cannot both light up in a reviewer scan and produce a phantom revision mismatch.",
    },
    DefaultMechanism {
        mechanism_id: "mission_state_field_clobbered_blocked",
        mechanism_class: "safety",
        autonomy: "autonomous_mission_state_field_ratchet",
        prompt_visibility: "inventory_only",
        module_hint: "src/context/lcm.rs",
        description: "One-way ratchet on `mission_states.next_slice` and `mission_states.done_gate`: once non-empty, automation may replace them with new non-empty content but cannot silently clear them — surfaces the attempted clobber as a governance event instead.",
    },
    DefaultMechanism {
        mechanism_id: "review_rewrite_threshold",
        mechanism_class: "safety",
        autonomy: "autonomous_review_rewrite_threshold",
        prompt_visibility: "prompt_visible",
        module_hint: "src/service/service.rs",
        description: "Stops respawning lightweight rewrite-only review retries once the per-mission convergence threshold is hit; defers the mission and records a governance event so operators see why the loop stopped.",
    },
    DefaultMechanism {
        mechanism_id: "strategic_directive_mutation_blocked_non_owner_sender",
        mechanism_class: "safety",
        autonomy: "autonomous_strategic_directive_authority_block",
        prompt_visibility: "prompt_visible",
        module_hint: "src/mission/strategy.rs",
        description: "Blocks an inbound-mail-driven strategic-directive mutation (`ctox strategy set --status active` / `ctox strategy activate`) when the sender of the triggering message is not the configured owner. Owner-authored mail is the canonical authority for active vision/mission/strategy; founder/admin replies may only produce proposals; other senders cannot mutate strategy at all. Operator-direct invocations (no `--triggered-by-inbound`) are unaffected.",
    },
    DefaultMechanism {
        mechanism_id: "strategic_directive_mutation_owner_authorised",
        mechanism_class: "safety",
        autonomy: "autonomous_strategic_directive_authority_audit",
        prompt_visibility: "inventory_only",
        module_hint: "src/mission/strategy.rs",
        description: "Records a positive audit trail when an inbound-mail-driven strategic-directive mutation passes the owner-authority gate. Pairs with `strategic_directive_mutation_blocked_non_owner_sender`; together they make the authority decision visible in `ctox channel pipeline-status`.",
    },
    DefaultMechanism {
        mechanism_id: "plan_routing_repair",
        mechanism_class: "survival",
        autonomy: "autonomous_plan_routing_repair",
        prompt_visibility: "inventory_only",
        module_hint: "src/service.rs",
        description: "Records when boot/turn maintenance releases or closes stale plan queue routes so a crashed or superseded plan step cannot keep holding routing state. Without this registration the repair events were silently dropped by the governance reporting inner-join.",
    },
    DefaultMechanism {
        mechanism_id: "channel_router_core_guard",
        mechanism_class: "survival",
        autonomy: "autonomous_router_core_guard",
        prompt_visibility: "inventory_only",
        module_hint: "src/service.rs",
        description: "Records when the channel router hits a core-workflow guard error, keeps the router alive, and skips the guarded background routing stage instead of crashing the loop. Registration makes these skips visible in governance reporting instead of being dropped by the inner-join.",
    },
    DefaultMechanism {
        mechanism_id: "queue_pressure_router_skip",
        mechanism_class: "survival",
        autonomy: "autonomous_queue_containment",
        prompt_visibility: "inventory_only",
        module_hint: "src/service.rs",
        description: "Records when the channel router skips all downstream stages because pending prompt pressure is active. This skip was previously a silent early-return; registration makes the queue-pressure containment visible in governance reporting instead of being dropped by the inner-join.",
    },
    DefaultMechanism {
        mechanism_id: "channel_router_loop_active",
        mechanism_class: "survival",
        autonomy: "autonomous_router_loop_deferral",
        prompt_visibility: "inventory_only",
        module_hint: "src/service.rs",
        description: "Records when the channel router defers routing because an agent reasoning/tool/review loop is still in progress. This deferral was previously a silent early-return; registration makes the loop-active arbitration visible in governance reporting instead of being dropped by the inner-join.",
    },
    DefaultMechanism {
        mechanism_id: "routing_ack_failed",
        mechanism_class: "survival",
        autonomy: "autonomous_routing_ack_audit",
        prompt_visibility: "inventory_only",
        module_hint: "src/service.rs",
        description: "Records when a routing ack fails (a lease may then re-route the message until resolved). The failure used to be eprintln-only; registration makes a repeated ack-reject loop mineable in governance reporting instead of being dropped by the inner-join.",
    },
    DefaultMechanism {
        mechanism_id: "boot_lease_reclaim",
        mechanism_class: "survival",
        autonomy: "autonomous_crash_recovery_audit",
        prompt_visibility: "inventory_only",
        module_hint: "src/service.rs",
        description: "Records when boot-time repair reclaims stale service communication leases left by a prior crash. The reclaim was previously only a transient feed line; registration makes crash recovery auditable in governance reporting instead of being invisible to process-mining.",
    },
];

pub fn handle_governance_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "init" => {
            ensure_governance(root)?;
            print_json(&json!({"ok": true, "db_path": resolve_db_path(root)}))
        }
        "snapshot" | "status" => {
            let conversation_id = parse_conversation_id(args)?;
            let snapshot = prompt_snapshot(root, conversation_id)?;
            print_json(&json!({"ok": true, "snapshot": snapshot}))
        }
        "inventory" => print_json(&json!({
            "ok": true,
            "count": mechanism_inventory().len(),
            "mechanisms": mechanism_inventory(),
        })),
        "events" => {
            let conversation_id = parse_conversation_id(args)?;
            let limit = parse_limit(args, DEFAULT_EVENT_LIMIT);
            let events = list_recent_events(root, conversation_id, limit)?;
            print_json(&json!({"ok": true, "count": events.len(), "events": events}))
        }
        _ => anyhow::bail!(
            "usage:\n  ctox governance init\n  ctox governance snapshot [--conversation-id <id>]\n  ctox governance inventory\n  ctox governance events [--conversation-id <id>] [--limit <n>]"
        ),
    }
}

pub fn ensure_governance(root: &Path) -> Result<()> {
    let conn = open_governance_db(root)?;
    upsert_default_mechanisms(&conn)
}

/// Insert a governance event, returning its deterministic id and whether THIS
/// call created the row (`true`) or an idempotence-key collision suppressed it
/// (`false`). The `INSERT OR IGNORE` plus the UNIQUE(mechanism_id,
/// idempotence_key) index make the newness verdict atomic, so callers can take
/// a once-per-key side effect without a check-then-act race.
fn insert_governance_event(
    conn: &Connection,
    request: &GovernanceEventRequest<'_>,
    created_at: &str,
) -> Result<(String, bool)> {
    let event_id = governance_event_id(
        request.mechanism_id,
        request.reason,
        request.action_taken,
        &request.details,
        created_at,
    );
    let details_json = serde_json::to_string(&request.details)
        .context("failed to encode governance event details")?;
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO governance_events (
            event_id,
            mechanism_id,
            conversation_id,
            severity,
            reason,
            action_taken,
            details_json,
            idempotence_key,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            event_id,
            request.mechanism_id,
            request.conversation_id,
            request.severity,
            request.reason,
            request.action_taken,
            details_json,
            request.idempotence_key,
            created_at,
        ],
    )?;
    Ok((event_id, inserted != 0))
}

pub fn record_event(
    root: &Path,
    request: GovernanceEventRequest<'_>,
) -> Result<Option<GovernanceEventRecord>> {
    let conn = open_governance_db(root)?;
    upsert_default_mechanisms(&conn)?;
    let created_at = now_millis_string();
    let (event_id, was_new) = insert_governance_event(&conn, &request, &created_at)?;
    if !was_new {
        let Some(idempotence_key) = request.idempotence_key else {
            return Ok(None);
        };
        let existing = conn
            .query_row(
                "SELECT event_id FROM governance_events
                 WHERE mechanism_id = ?1 AND idempotence_key = ?2
                 ORDER BY CAST(created_at AS INTEGER) DESC
                 LIMIT 1",
                params![request.mechanism_id, idempotence_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        return existing
            .map(|event_id| load_event_by_id(&conn, &event_id))
            .transpose();
    }
    load_event_by_id(&conn, &event_id).map(Some)
}

/// x2-3: process-lifetime count of durable governance audit-write failures that
/// were swallowed by [`record_event_or_count`] callers (`record_event` returning
/// Err — typically SQLITE_BUSY under the serial router's WAL contention). The
/// `let _ = record_event(...)` drop used at ~35 sites made these losses invisible,
/// silently undermining the harness's "auditable" claim; the counter (plus a
/// logged line per failure) gives them a durable, queryable trace.
static DROPPED_AUDIT_WRITES: AtomicU64 = AtomicU64::new(0);

/// x2-3: the swallow-safe form of [`record_event`]. Records the governance event
/// and, on failure, makes the loss VISIBLE — a logged line carrying the mechanism
/// id and the error, plus a bump of the durable [`dropped_audit_writes`] counter —
/// instead of the silent `let _ = record_event(...)` drop it replaces. Returns
/// nothing: callers that previously discarded the `Result` now get accountability
/// without threading error handling through every governance touchpoint.
pub fn record_event_or_count(root: &Path, request: GovernanceEventRequest<'_>) {
    let mechanism_id = request.mechanism_id;
    if let Err(err) = record_event(root, request) {
        DROPPED_AUDIT_WRITES.fetch_add(1, Ordering::Relaxed);
        eprintln!("governance audit write failed [{mechanism_id}]: {err:#}");
    }
}

/// x2-3: process-lifetime count of governance audit-write failures swallowed by
/// [`record_event_or_count`]. Surfaced so a host losing audit rows under DB
/// contention is observable rather than silently lossy.
pub fn dropped_audit_writes() -> u64 {
    DROPPED_AUDIT_WRITES.load(Ordering::Relaxed)
}

/// Record a governance event and report whether THIS call created it.
///
/// Returns `true` when the event was newly inserted and `false` when an
/// idempotence-key collision suppressed it. Callers that must take a side
/// effect at most once per `(mechanism_id, idempotence_key)` (e.g. the loop
/// governor logging a single repair recommendation per repeated blocker) gate
/// the side effect on the returned flag — the atomic `INSERT OR IGNORE` makes a
/// concurrent second call return `false`, with no check-then-act race.
pub fn record_event_if_new(root: &Path, request: GovernanceEventRequest<'_>) -> Result<bool> {
    let conn = open_governance_db(root)?;
    upsert_default_mechanisms(&conn)?;
    let created_at = now_millis_string();
    let (_event_id, was_new) = insert_governance_event(&conn, &request, &created_at)?;
    Ok(was_new)
}

pub fn prompt_snapshot(root: &Path, conversation_id: i64) -> Result<GovernancePromptSnapshot> {
    let conn = open_governance_db(root)?;
    // `upsert_default_mechanisms` is a writer-locked INSERT-OR-UPDATE loop
    // over a fixed set of rows whose values are static. Running it on every
    // TUI refresh contends with the daemon's writer and dominates lag. Skip
    // after first call per process per path — initial population happened
    // either at install time or on the daemon side.
    let conn_path = conn.path().map(PathBuf::from);
    let needs_upsert = match conn_path.as_ref() {
        Some(path) => !initialized_governance_paths()
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .contains(path),
        None => true,
    };
    if needs_upsert {
        upsert_default_mechanisms(&conn)?;
        if let Some(path) = conn_path {
            initialized_governance_paths()
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .insert(path);
        }
    }
    Ok(GovernancePromptSnapshot {
        mechanisms: list_mechanisms(&conn)?,
        recent_events: list_recent_events_from_conn(&conn, conversation_id, DEFAULT_EVENT_LIMIT)?,
    })
}

pub fn render_prompt_block(snapshot: &GovernancePromptSnapshot) -> String {
    let visible_mechanisms = snapshot
        .mechanisms
        .iter()
        .filter(|mechanism| {
            matches!(
                mechanism.mechanism_class.as_str(),
                "survival" | "safety" | "advisory"
            )
        })
        .collect::<Vec<_>>();
    let autonomous = visible_mechanisms
        .iter()
        .filter(|mechanism| matches!(mechanism.mechanism_class.as_str(), "survival" | "safety"))
        .map(|mechanism| mechanism.mechanism_id.as_str())
        .collect::<Vec<_>>();
    let advisory = visible_mechanisms
        .iter()
        .filter(|mechanism| mechanism.mechanism_class == "advisory")
        .map(|mechanism| mechanism.mechanism_id.as_str())
        .collect::<Vec<_>>();
    let mut lines = vec!["Governance:".to_string()];
    lines.push(
        "how_to_use: read this as context only. It explains automatic CTOX actions. Do not invent extra work from this block.".to_string(),
    );
    // The full mechanism inventory is static every-turn noise (~20 lines);
    // render compact counts and spend the token budget on recent events,
    // which carry the actual per-turn signal.
    if !autonomous.is_empty() {
        lines.push(format!(
            "automatic_actions: {} mechanisms armed; they act on their own and their recorded events below are authoritative",
            autonomous.len()
        ));
    }
    if !advisory.is_empty() {
        lines.push(format!(
            "advice_only: {} mechanisms; they act only when explicitly invoked",
            advisory.len()
        ));
    }
    if snapshot.recent_events.is_empty() {
        lines.push("recent_events:".to_string());
        lines.push("- none".to_string());
    } else {
        lines.push("recent_events:".to_string());
        for event in snapshot.recent_events.iter().take(3) {
            let detail = compact_detail(&event.details);
            let reason = clip_text(&event.reason.replace('_', " "), 64);
            let action = clip_text(&event.action_taken.replace('_', " "), 64);
            lines.push(format!(
                "- mechanism: {}",
                event.mechanism_id.replace('_', " ")
            ));
            lines.push(format!("  why: {reason}"));
            lines.push(format!("  what_ctox_did: {action}"));
            if !detail.is_empty() {
                lines.push(format!("  details: {detail}"));
            }
        }
    }
    lines.join("\n")
}

pub fn mechanism_id_for_block_reason(reason: &str) -> &'static str {
    let lowered = reason.to_ascii_lowercase();
    if lowered.contains("secret") {
        "secret_input_boundary"
    } else {
        "sender_authority_boundary"
    }
}

pub fn mechanism_inventory() -> Vec<GovernanceMechanismInventoryRecord> {
    DEFAULT_MECHANISMS
        .iter()
        .map(|mechanism| GovernanceMechanismInventoryRecord {
            mechanism_id: mechanism.mechanism_id.to_string(),
            mechanism_class: mechanism.mechanism_class.to_string(),
            intervention_mode: mechanism.autonomy.to_string(),
            prompt_visibility: mechanism.prompt_visibility.to_string(),
            module_hint: mechanism.module_hint.to_string(),
            description: mechanism.description.to_string(),
        })
        .collect()
}

pub fn list_recent_events(
    root: &Path,
    conversation_id: i64,
    limit: usize,
) -> Result<Vec<GovernanceEventRecord>> {
    let conn = open_governance_db(root)?;
    upsert_default_mechanisms(&conn)?;
    list_recent_events_from_conn(&conn, conversation_id, limit)
}

fn parse_conversation_id(args: &[String]) -> Result<i64> {
    find_flag_value(args, "--conversation-id")
        .map(|value| value.parse::<i64>())
        .transpose()
        .context("failed to parse --conversation-id")?
        .unwrap_or(1)
        .pipe(Ok)
}

fn parse_limit(args: &[String], default: usize) -> usize {
    find_flag_value(args, "--limit")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn print_json(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn resolve_db_path(root: &Path) -> std::path::PathBuf {
    root.join(DEFAULT_DB_RELATIVE_PATH)
}

/// DB paths whose `open_governance_db` has run its writer-locked schema
/// batch (`PRAGMA journal_mode=WAL` + `CREATE TABLE/INDEX IF NOT EXISTS`).
/// Skipped on subsequent opens — same rationale as the persistence /
/// secrets schema caches: the tables exist after the first call, and
/// re-running the batch costs a writer lock that contends with the daemon.
fn governance_schema_initialized() -> &'static Mutex<HashSet<PathBuf>> {
    static INITIALIZED: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    INITIALIZED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn open_governance_db(root: &Path) -> Result<Connection> {
    let path = resolve_db_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create governance db parent {}", parent.display())
        })?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open governance db {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout for governance")?;

    let needs_init = !governance_schema_initialized()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .contains(&path);
    if needs_init {
        let busy_timeout_ms = crate::persistence::sqlite_busy_timeout_millis();
        conn.execute_batch(&format!(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = {busy_timeout_ms};

            CREATE TABLE IF NOT EXISTS governance_mechanisms (
                mechanism_id TEXT PRIMARY KEY,
                mechanism_class TEXT NOT NULL,
                autonomy TEXT NOT NULL,
                description TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS governance_events (
                event_id TEXT PRIMARY KEY,
                mechanism_id TEXT NOT NULL,
                conversation_id INTEGER,
                severity TEXT NOT NULL,
                reason TEXT NOT NULL,
                action_taken TEXT NOT NULL,
                details_json TEXT NOT NULL,
                idempotence_key TEXT,
                created_at TEXT NOT NULL
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_governance_event_dedupe
                ON governance_events(mechanism_id, idempotence_key)
                WHERE idempotence_key IS NOT NULL;

            CREATE INDEX IF NOT EXISTS idx_governance_events_recent
                ON governance_events(created_at DESC, mechanism_id);
            "#,
        ))?;
        governance_schema_initialized()
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(path);
    }
    Ok(conn)
}

fn upsert_default_mechanisms(conn: &Connection) -> Result<()> {
    let now = now_millis_string();
    for mechanism in DEFAULT_MECHANISMS {
        conn.execute(
            "INSERT INTO governance_mechanisms (
                mechanism_id,
                mechanism_class,
                autonomy,
                description,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(mechanism_id) DO UPDATE SET
                mechanism_class = excluded.mechanism_class,
                autonomy = excluded.autonomy,
                description = excluded.description,
                updated_at = excluded.updated_at",
            params![
                mechanism.mechanism_id,
                mechanism.mechanism_class,
                mechanism.autonomy,
                mechanism.description,
                now,
            ],
        )?;
    }
    Ok(())
}

fn list_mechanisms(conn: &Connection) -> Result<Vec<GovernanceMechanismRecord>> {
    let mut stmt = conn.prepare(
        "SELECT mechanism_id, mechanism_class, autonomy, description, updated_at
         FROM governance_mechanisms
         ORDER BY
            CASE mechanism_class
                WHEN 'survival' THEN 0
                WHEN 'safety' THEN 1
                WHEN 'advisory' THEN 2
                ELSE 3
            END,
            mechanism_id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(GovernanceMechanismRecord {
            mechanism_id: row.get(0)?,
            mechanism_class: row.get(1)?,
            autonomy: row.get(2)?,
            description: row.get(3)?,
            updated_at: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn list_recent_events_from_conn(
    conn: &Connection,
    conversation_id: i64,
    limit: usize,
) -> Result<Vec<GovernanceEventRecord>> {
    let mut stmt = conn.prepare(
        "SELECT
            e.event_id,
            e.mechanism_id,
            m.mechanism_class,
            e.conversation_id,
            e.severity,
            e.reason,
            e.action_taken,
            e.details_json,
            e.created_at
         FROM governance_events e
         JOIN governance_mechanisms m ON m.mechanism_id = e.mechanism_id
         WHERE e.conversation_id IS NULL OR e.conversation_id = ?1
         ORDER BY CAST(e.created_at AS INTEGER) DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![conversation_id, limit as i64], |row| {
        let details = serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or(Value::Null);
        Ok(GovernanceEventRecord {
            event_id: row.get(0)?,
            mechanism_id: row.get(1)?,
            mechanism_class: row.get(2)?,
            conversation_id: row.get(3)?,
            severity: row.get(4)?,
            reason: row.get(5)?,
            action_taken: row.get(6)?,
            details,
            created_at: row.get(8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_event_by_id(conn: &Connection, event_id: &str) -> Result<GovernanceEventRecord> {
    conn.query_row(
        "SELECT
            e.event_id,
            e.mechanism_id,
            m.mechanism_class,
            e.conversation_id,
            e.severity,
            e.reason,
            e.action_taken,
            e.details_json,
            e.created_at
         FROM governance_events e
         JOIN governance_mechanisms m ON m.mechanism_id = e.mechanism_id
         WHERE e.event_id = ?1",
        [event_id],
        |row| {
            let details = serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or(Value::Null);
            Ok(GovernanceEventRecord {
                event_id: row.get(0)?,
                mechanism_id: row.get(1)?,
                mechanism_class: row.get(2)?,
                conversation_id: row.get(3)?,
                severity: row.get(4)?,
                reason: row.get(5)?,
                action_taken: row.get(6)?,
                details,
                created_at: row.get(8)?,
            })
        },
    )
    .context("failed to load governance event")
}

fn governance_event_id(
    mechanism_id: &str,
    reason: &str,
    action_taken: &str,
    details: &Value,
    created_at: &str,
) -> String {
    let mut hash = Sha256::new();
    hash.update(mechanism_id.as_bytes());
    hash.update(reason.as_bytes());
    hash.update(action_taken.as_bytes());
    hash.update(details.to_string().as_bytes());
    hash.update(created_at.as_bytes());
    let digest = hash.finalize();
    let prefix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("gov_{prefix}")
}

fn compact_detail(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Object(map) if map.is_empty() => String::new(),
        _ => clip_text(&value.to_string(), 180),
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

fn now_millis_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;

    /// x2-3: a swallowed governance audit-write failure must leave a durable trace
    /// (the dropped_audit_writes counter) instead of vanishing silently. Force
    /// record_event to fail by rooting it at a regular FILE so the governance db
    /// parent directory cannot be created.
    #[test]
    fn record_event_or_count_increments_dropped_counter_on_failure() {
        let not_a_dir = std::env::temp_dir().join(format!(
            "ctox-governance-x2-3-not-a-dir-{}",
            now_millis_string()
        ));
        std::fs::write(&not_a_dir, b"x").expect("failed to create blocking file");

        let before = dropped_audit_writes();
        record_event_or_count(
            &not_a_dir,
            GovernanceEventRequest {
                mechanism_id: "x2_3_forced_failure",
                conversation_id: None,
                severity: "info",
                reason: "forced failure for the dropped-audit-write counter test",
                action_taken: "noop",
                details: serde_json::json!({}),
                idempotence_key: None,
            },
        );
        let after = dropped_audit_writes();
        // Process-global counter: assert the monotonic delta so the test is robust
        // to other tests incrementing it concurrently.
        assert!(
            after >= before + 1,
            "a swallowed audit-write failure must increment dropped_audit_writes \
             (before={before}, after={after})"
        );

        let _ = std::fs::remove_file(&not_a_dir);
    }

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("ctox-governance-{label}-{}", now_millis_string()));
        std::fs::create_dir_all(root.join("runtime")).expect("failed to create runtime dir");
        root
    }

    #[test]
    fn governance_snapshot_surfaces_known_mechanisms_and_recent_event() -> Result<()> {
        let root = temp_root("snapshot");
        ensure_governance(&root)?;
        let _ = record_event(
            &root,
            GovernanceEventRequest {
                mechanism_id: "queue_pressure_guard",
                conversation_id: Some(1),
                severity: "warning",
                reason: "pending prompts crossed the guard threshold",
                action_taken: "inserted queue guard prompt",
                details: json!({"pending": 7, "threshold": 6}),
                idempotence_key: Some("queue-7"),
            },
        )?;
        let snapshot = prompt_snapshot(&root, 1)?;
        assert!(snapshot
            .mechanisms
            .iter()
            .any(|mechanism| mechanism.mechanism_id == "queue_pressure_guard"));
        assert!(snapshot
            .recent_events
            .iter()
            .any(|event| event.mechanism_id == "queue_pressure_guard"));
        Ok(())
    }

    #[test]
    fn duplicate_idempotence_key_reuses_existing_event() -> Result<()> {
        let root = temp_root("dedupe");
        ensure_governance(&root)?;
        let first = record_event(
            &root,
            GovernanceEventRequest {
                mechanism_id: "queue_pressure_guard",
                conversation_id: Some(1),
                severity: "warning",
                reason: "pending prompts crossed the guard threshold",
                action_taken: "inserted queue guard prompt",
                details: json!({"pending": 7, "threshold": 6}),
                idempotence_key: Some("queue-guard-dedupe"),
            },
        )?
        .context("expected first event")?;
        let second = record_event(
            &root,
            GovernanceEventRequest {
                mechanism_id: "queue_pressure_guard",
                conversation_id: Some(1),
                severity: "warning",
                reason: "pending prompts crossed the guard threshold",
                action_taken: "inserted queue guard prompt",
                details: json!({"pending": 7, "threshold": 6}),
                idempotence_key: Some("queue-guard-dedupe"),
            },
        )?
        .context("expected existing event")?;
        assert_eq!(first.event_id, second.event_id);
        Ok(())
    }

    fn loop_governor_event(idempotence_key: &str) -> GovernanceEventRequest<'_> {
        GovernanceEventRequest {
            mechanism_id: "mission_loop_governor",
            conversation_id: Some(1),
            severity: "warning",
            reason: "repeated blocker under loop pressure",
            action_taken: "recorded a forced repair recommendation",
            details: json!({"work_id": "work-1"}),
            idempotence_key: Some(idempotence_key),
        }
    }

    #[test]
    fn record_event_if_new_reports_first_insert_only() -> Result<()> {
        let root = temp_root("record-if-new");
        ensure_governance(&root)?;
        // First emit for a key creates the row; the second is suppressed by the
        // UNIQUE(mechanism_id, idempotence_key) index. This is what gates the
        // loop governor's once-per-(work, blocker) side effect with no
        // check-then-act race.
        assert!(
            record_event_if_new(&root, loop_governor_event("loop:work-1:stuck"))?,
            "first emit must create the event"
        );
        assert!(
            !record_event_if_new(&root, loop_governor_event("loop:work-1:stuck"))?,
            "second emit for the same key must be suppressed"
        );
        // A distinct blocker key is a distinct recovery and emits again.
        assert!(
            record_event_if_new(&root, loop_governor_event("loop:work-1:other"))?,
            "a different key is a different event"
        );
        // Exactly one row per distinct key survived.
        let rows: i64 = open_governance_db(&root)?.query_row(
            "SELECT COUNT(*) FROM governance_events WHERE mechanism_id = 'mission_loop_governor'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(rows, 2, "two distinct keys -> two rows, no duplicates");
        Ok(())
    }

    #[test]
    fn inventory_covers_survival_advisory_and_explicit_mechanisms() {
        let inventory = mechanism_inventory();
        assert!(inventory
            .iter()
            .any(|entry| entry.mechanism_id == "runtime_blocker_backoff"));
        assert!(inventory
            .iter()
            .any(|entry| entry.mechanism_id == "context_health_assessment"));
        assert!(inventory
            .iter()
            .any(|entry| entry.mechanism_id == "follow_up_evaluate"));
        assert!(inventory
            .iter()
            .any(|entry| entry.mechanism_id == "verification_assurance"));
    }

    #[test]
    fn inventory_covers_router_and_plan_repair_mechanisms() {
        // Both ids are emitted in production (run_plan_routing_repair and
        // handle_channel_router_guard_block) but were unregistered, so the
        // governance reporting inner-join silently dropped their events.
        let inventory = mechanism_inventory();
        for id in [
            "plan_routing_repair",
            "channel_router_core_guard",
            "queue_pressure_router_skip",
            "channel_router_loop_active",
            "routing_ack_failed",
            "boot_lease_reclaim",
        ] {
            assert!(
                inventory.iter().any(|entry| entry.mechanism_id == id),
                "mechanism {id} must be registered so governance reporting does not drop its events"
            );
        }
    }

    #[test]
    fn render_prompt_block_stays_compact() -> Result<()> {
        let root = temp_root("render");
        ensure_governance(&root)?;
        let _ = record_event(
            &root,
            GovernanceEventRequest {
                mechanism_id: "queue_pressure_guard",
                conversation_id: Some(1),
                severity: "warning",
                reason: "pending prompts crossed the guard threshold and backlog kept rising",
                action_taken: "inserted queue guard prompt and paused new dispatch briefly",
                details: json!({"pending": 7, "threshold": 6}),
                idempotence_key: Some("queue-render"),
            },
        )?;
        let block = render_prompt_block(&prompt_snapshot(&root, 1)?);
        assert!(block.contains("Governance:"));
        assert!(block.contains("automatic_actions:"));
        assert!(block.contains("recent_events:"));
        assert!(block.contains("why:"));
        assert!(!block.contains("Only `survival` and `safety` mechanisms"));
        assert!(
            block.len() < 1024,
            "governance block too large: {}",
            block.len()
        );
        Ok(())
    }
}
